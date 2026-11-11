//! Settings for generated changeset attributes (G116) and for file names (G120), shared by the Liquibase and
//! Flyway generation actions. Absent or empty settings mean today's behavior; a present but invalid value refuses
//! the action with a message instead of being guessed at.
use super::*;
use crate::file_actions::migration_action::read_optional;
use persistence_core::liquibase::{ChangeSetAttributes, ChangeSetDefaults, PreconditionAction, CHANGE_TYPES};
use persistence_core::naming::{self, DateTime, DescriptionStyle, Needs};
use quick_xml::{events::Event, reader::Reader};

pub(super) const LIQUIBASE_DEFAULT_PATTERN: &str = "{seq}-{description}.xml";
const MAX_ROWS: usize = 64;

fn setting(key: &str) -> Option<String> {
    host::setting_string(key).map(|value| value.trim().to_owned()).filter(|value| !value.is_empty())
}

fn flag(row: &serde_json::Value, key: &str) -> Result<Option<bool>, Error> {
    match &row[key] {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::Bool(value) => Ok(Some(*value)),
        serde_json::Value::String(text) => match text.as_str() {
            "" | "unset" => Ok(None),
            "true" => Ok(Some(true)),
            "false" => Ok(Some(false)),
            _ => Err(format!("liquibaseChangeSetDefaults: {key} must be unset, true or false.").into()),
        },
        _ => Err(format!("liquibaseChangeSetDefaults: {key} must be unset, true or false.").into()),
    }
}

fn text(row: &serde_json::Value, key: &str) -> Result<Option<String>, Error> {
    match &row[key] {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(value) => Ok(Some(value.trim().to_owned()).filter(|value| !value.is_empty())),
        _ => Err(format!("liquibaseChangeSetDefaults: {key} must be text.").into()),
    }
}

fn action(row: &serde_json::Value, key: &str) -> Result<Option<PreconditionAction>, Error> {
    match row[key].as_str() {
        None | Some("") | Some("unset") | Some("off") => Ok(None),
        Some(value) => PreconditionAction::parse(value)
            .map(Some)
            .ok_or_else(|| Error::from(format!("liquibaseChangeSetDefaults: {key} must be unset, HALT, CONTINUE, MARK_RAN or WARN."))),
    }
}

/// Rows of `liquibaseChangeSetDefaults`: `changeType` `all` or a change element name; later rows of the same
/// type override earlier ones field by field. Context and label syntax is validated where the XML is emitted.
pub(super) fn changeset_defaults() -> Result<ChangeSetDefaults, Error> {
    let mut defaults = ChangeSetDefaults::default();
    let Some(raw) = host::setting("liquibaseChangeSetDefaults") else {
        return Ok(defaults);
    };
    let rows: Vec<serde_json::Value> = serde_json::from_str(&raw).map_err(|_| Error::from("liquibaseChangeSetDefaults is not a list of rows."))?;
    if rows.len() > MAX_ROWS {
        return Err(format!("liquibaseChangeSetDefaults has more than {MAX_ROWS} rows.").into());
    }
    for row in &rows {
        let attributes = ChangeSetAttributes {
            fail_on_error: flag(row, "failOnError")?,
            run_on_change: flag(row, "runOnChange")?,
            context: text(row, "context")?,
            labels: text(row, "labels")?,
            on_fail: action(row, "preConditionsOnFail")?,
            on_error: action(row, "preConditionsOnError")?,
            empty_rollback: flag(row, "addEmptyRollback")?,
        };
        match row["changeType"].as_str().unwrap_or("all") {
            "all" | "" => defaults.all.overlay(&attributes),
            kind if CHANGE_TYPES.contains(&kind) => defaults.by_type.push((kind.to_owned(), attributes)),
            other => return Err(format!("liquibaseChangeSetDefaults: unknown changeType {other:?}.").into()),
        }
    }
    Ok(defaults)
}

pub(super) fn description_style() -> Result<DescriptionStyle, Error> {
    match setting("migrationDescriptionCase") {
        None => Ok(DescriptionStyle::Keep),
        Some(value) => DescriptionStyle::parse(&value).ok_or_else(|| Error::from("migrationDescriptionCase must be keep, lower_snake or kebab.")),
    }
}

fn checked_pattern(key: &str, pattern: &str) -> Result<Needs, Error> {
    naming::needs(pattern).map_err(|e| Error::from(format!("{key} {pattern:?}: {e}.")))
}

pub(super) fn liquibase_name_pattern() -> Result<(String, Needs), Error> {
    let pattern = setting("liquibaseChangelogNamePattern").unwrap_or_else(|| LIQUIBASE_DEFAULT_PATTERN.to_owned());
    let needs = checked_pattern("liquibaseChangelogNamePattern", &pattern)?;
    if !pattern.ends_with(".xml") {
        return Err("liquibaseChangelogNamePattern must end with .xml (only XML changelogs are generated).".into());
    }
    Ok((pattern, needs))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FlywayNaming {
    pub(super) prefix: String,
    pub(super) separator: String,
    pub(super) version_pattern: Option<(String, Needs)>,
}

impl Default for FlywayNaming {
    fn default() -> Self {
        Self { prefix: "V".into(), separator: "__".into(), version_pattern: None }
    }
}

pub(super) fn flyway_naming() -> Result<FlywayNaming, Error> {
    let mut naming = FlywayNaming::default();
    if let Some(prefix) = setting("flywayMigrationPrefix") {
        if !matches!(prefix.as_str(), "V" | "U" | "B") {
            return Err("flywayMigrationPrefix must be V, U or B.".into());
        }
        naming.prefix = prefix;
    }
    if let Some(separator) = host::setting_string("flywayMigrationSeparator").filter(|value| !value.is_empty()) {
        if !(2..=4).contains(&separator.len()) || !separator.bytes().all(|b| b == b'_') {
            return Err("flywayMigrationSeparator must be 2 to 4 underscores.".into());
        }
        naming.separator = separator;
    }
    if let Some(pattern) = setting("flywayVersionPattern") {
        let needs = checked_pattern("flywayVersionPattern", &pattern)?;
        naming.version_pattern = Some((pattern, needs));
    }
    Ok(naming)
}

/// Date for `#date`/`{date}`: the WASI clock (documented in plugin.wit as available to plugins) proposes it and
/// the user confirms or edits it; the expander itself never reads a clock.
pub(super) fn ask_date(needs: &Needs) -> Result<Option<DateTime>, Error> {
    if !needs.date {
        return Ok(None);
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().and_then(|since| DateTime::from_unix(since.as_secs()));
    let proposal = now.map(|d| format!("{:04}{:02}{:02}{:02}{:02}{:02}", d.year, d.month, d.day, d.hour, d.minute, d.second));
    let answer = host::input("Date for the file name (UTC, yyyyMMdd or yyyyMMddHHmmss)", proposal.as_deref())?.ok_or(Error::Cancelled)?;
    DateTime::parse_compact(answer.trim()).map(Some).ok_or_else(|| Error::from("Enter the date as yyyyMMdd or yyyyMMddHHmmss (a real calendar date)."))
}

fn gradle_version(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("version") else {
            continue;
        };
        let rest = rest.trim_start().trim_start_matches('=').trim();
        let Some(quote) = rest.chars().next().filter(|c| matches!(c, '"' | '\'')) else {
            continue;
        };
        if let Some(value) = rest[1..].split(quote).next().filter(|_| rest[1..].contains(quote)) {
            found.push(value.to_owned());
        }
    }
    found
}

fn pom_version(text: &str) -> Vec<String> {
    let mut reader = Reader::from_str(text);
    let (mut depth, mut in_version, mut found) = (0usize, false, Vec::new());
    for _ in 0..50_000 {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                depth += 1;
                in_version = depth == 2 && e.name().as_ref() == "version";
            }
            Ok(Event::End(_)) => {
                depth = depth.saturating_sub(1);
                in_version = false;
            }
            Ok(Event::Text(t)) if in_version => found.push(t.xml10_content().trim().to_owned()),
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    found
}

/// The project's own literal version: a top-level `version = "x"` in `build.gradle(.kts)` or the project's direct
/// `<version>` in `pom.xml`, at the project root only. Several sources or no literal refuse.
pub(super) fn project_version(project: &Project) -> Result<String, Error> {
    let mut versions = Vec::new();
    for file in ["build.gradle.kts", "build.gradle"] {
        if let Some(text) = read_optional(project, file)? {
            versions.extend(gradle_version(&text));
        }
    }
    if let Some(text) = read_optional(project, "pom.xml")? {
        versions.extend(pom_version(&text));
    }
    match versions.as_slice() {
        [only] if !only.is_empty() && !only.contains("${") => Ok(only.clone()),
        [] => Err("${semVer.*} needs a literal project version (top-level version = \"x\" in build.gradle(.kts) or the project <version> in pom.xml at the project root); none was found.".into()),
        _ => Err("The project version is not one plain literal (several definitions or a property reference); ${semVer.*} is not guessed.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    #[test]
    fn gradle_and_pom_versions() {
        assert_eq!(gradle_version("plugins {}\nversion = \"1.2.3-SNAPSHOT\"\n"), ["1.2.3-SNAPSHOT"]);
        assert_eq!(gradle_version("version '2.0.0'\n"), ["2.0.0"]);
        assert!(gradle_version("    version = \"1.0\"\n").is_empty(), "indented lines are nested blocks");
        assert!(gradle_version("version = project.property('v')\n").is_empty());
        let pom = "<project><parent><version>9</version></parent><modelVersion>4.0.0</modelVersion><version>1.4.0</version><dependencies><dependency><version>3</version></dependency></dependencies></project>";
        assert_eq!(pom_version(pom), ["1.4.0"]);
    }

    #[test]
    fn defaults_when_unset_and_validation_when_set() {
        let host = MockHost::install();
        assert_eq!(changeset_defaults().unwrap(), ChangeSetDefaults::default());
        assert_eq!(description_style().unwrap(), DescriptionStyle::Keep);
        assert_eq!(liquibase_name_pattern().unwrap().0, LIQUIBASE_DEFAULT_PATTERN);
        assert_eq!(flyway_naming().unwrap(), FlywayNaming::default());

        host.set_setting("liquibaseChangeSetDefaults", r#"[{"changeType":"all","failOnError":"unset","runOnChange":"true","context":" dev ","labels":"","preConditionsOnFail":"WARN","preConditionsOnError":"off","addEmptyRollback":"true"},{"changeType":"dropTable","failOnError":"false"}]"#);
        let defaults = changeset_defaults().unwrap();
        assert_eq!(defaults.all.run_on_change, Some(true));
        assert_eq!((defaults.all.fail_on_error, defaults.all.labels.clone(), defaults.all.on_error), (None, None, None));
        assert_eq!(defaults.all.context.as_deref(), Some("dev"));
        assert_eq!(defaults.all.on_fail, Some(PreconditionAction::Warn));
        assert_eq!(defaults.all.empty_rollback, Some(true));
        assert_eq!(defaults.by_type.len(), 1);
        for bad in [r#"[{"changeType":"nope"}]"#, r#"[{"failOnError":"maybe"}]"#, r#"[{"preConditionsOnFail":"FAIL"}]"#, "{}", r#"[{"context":5}]"#] {
            host.set_setting("liquibaseChangeSetDefaults", bad);
            assert!(changeset_defaults().is_err(), "{bad}");
        }

        host.set_setting("migrationDescriptionCase", "\"kebab\"");
        assert_eq!(description_style().unwrap(), DescriptionStyle::Kebab);
        host.set_setting("migrationDescriptionCase", "\"Kebab\"");
        assert!(description_style().is_err());

        for bad in ["{nope}.xml", "{seq}-{description}", "a/{seq}.xml", ""] {
            host.set_setting("liquibaseChangelogNamePattern", &format!("{bad:?}"));
            if !bad.is_empty() {
                assert!(liquibase_name_pattern().is_err(), "{bad}");
            }
        }
        host.set_setting("flywayMigrationPrefix", "\"R\"");
        assert!(flyway_naming().is_err());
        host.set_setting("flywayMigrationPrefix", "\"U\"");
        host.set_setting("flywayMigrationSeparator", "\"_\"");
        assert!(flyway_naming().is_err());
        host.set_setting("flywayMigrationSeparator", "\"___\"");
        host.set_setting("flywayVersionPattern", "\"#date(yyyyMMdd)\"");
        let naming = flyway_naming().unwrap();
        assert_eq!((naming.prefix.as_str(), naming.separator.as_str()), ("U", "___"));
        assert!(naming.version_pattern.unwrap().1.date);
        host.set_setting("flywayVersionPattern", "\"#date(YYYY)\"");
        assert!(flyway_naming().is_err());
    }

    #[test]
    fn project_version_needs_one_literal() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        let project = host::project_of("/project/src/x.java").unwrap();
        assert!(project_version(&project).is_err());
        host.add_file("build.gradle.kts", "version = \"3.1.4\"\n");
        assert_eq!(project_version(&project).unwrap(), "3.1.4");
        host.add_file("pom.xml", "<project><version>3.1.4</version></project>");
        assert!(project_version(&project).is_err(), "two definitions");
    }
}
