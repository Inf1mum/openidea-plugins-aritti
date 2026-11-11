//! Liquibase XML changelog draft from entity-source model changes (baseline diff, as the Flyway action).
//!
//! Location rule (a deliberately small subset of the Liquibase plugin's conventions: XML masters with
//! `include`/`includeAll`, `relativeToChangelogFile`, default namespace; nothing is guessed): the master changelog
//! is the literal `classpath:` value of `spring.liquibase.change-log` / `changeLogFile`-style keys in
//! `src/main/resources/application*` or `liquibase*.properties`, else the single
//! `src/main/resources/db/changelog/db.changelog-master.*` (that default name is not verified against the
//! Liquibase docs). It must be XML. With no `includeAll` the new file goes beside the master; with exactly one
//! `includeAll` (and no `include`) it goes into that directory. Anything else refuses. The file name is
//! `<seq>-<description>.xml`, the sequence continuing the numeric prefixes found; with no numeric prefix, or when
//! the name would not sort after every sibling under `includeAll`, the action refuses.
//!
//! The master is never edited: released FileAction edits carry no expected document version (S007), so an edit of
//! a document other than the invoking one cannot be guarded. The exact `include` line is shown instead.
use super::*;
use super::migration_action::{BASELINE, BASELINE_NEXT, absent, absolute, acknowledge, ask_backfills, description_of, read_optional, scan, summary, target_in_root};
use persistence_core::liquibase::{self, LiquibaseOptions};
use persistence_core::schema::{self, Danger};
use persistence_core::sql::EmitOptions;
use quick_xml::{events::Event, name::ResolveResult, reader::NsReader};
use std::collections::BTreeSet;

pub(super) const TITLE: &str = "Generate Liquibase changelog from model changes…";
const NAMESPACE: &str = "http://www.liquibase.org/xml/ns/dbchangelog";
const MAX_FILES: u32 = 2_000;
const MAX_MASTER: usize = 256 << 10;
const EXTENSIONS: [&str; 5] = ["xml", "yaml", "yml", "json", "sql"];
const RESOURCES: &str = "/src/main/resources";

#[derive(Clone, Debug, PartialEq, Eq)]
enum Link {
    /// New file beside the master; the user adds an include line.
    Include,
    /// The master's single `includeAll` directory picks the file up.
    IncludeAll { path: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Located {
    master: String,
    master_text: String,
    directory: String,
    link: Link,
    names: Vec<String>,
    sequence: String,
}

fn resources_dir(file: &str) -> Option<String> {
    let at = file.find(&format!("{RESOURCES}/"))?;
    Some(format!("{}{RESOURCES}", &file[..at]))
}

fn changelog_value(line: &str) -> Option<String> {
    let line = line.trim();
    if line.starts_with('#') {
        return None;
    }
    let (key, value) = line.split_once(['=', ':'])?;
    let key: String = key.trim().rsplit('.').next()?.chars().filter(|c| !matches!(c, '-' | '_')).collect::<String>().to_ascii_lowercase();
    if key != "changelog" && key != "changelogfile" {
        return None;
    }
    let value = value.split(" #").next().unwrap_or("").trim().trim_matches(['"', '\'']).trim();
    Some(value.to_owned())
}

fn literal_path(value: &str) -> Result<&str, Error> {
    let value = value.trim_start_matches('/').trim_end_matches('/');
    if value.is_empty()
        || value.len() > 1024
        || value.contains("${")
        || value.contains(['\\', ':', '*', '?', '[', ']', '{', '}', '\0'])
        || value.chars().any(char::is_control)
        || value.split('/').any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err("A changelog path is not a plain literal path; nothing is guessed.".into());
    }
    Ok(value)
}

fn configured_master(project: &Project, root: &str) -> Result<Option<String>, Error> {
    if let Some(text) = read_optional(project, "liquibase.properties")? {
        if text.lines().any(|line| changelog_value(line).is_some()) {
            return Err("liquibase.properties in the project root configures a changelog; only classpath settings under src/main/resources are supported, and nothing is guessed.".into());
        }
    }
    let mut files = project.find_files("**/src/main/resources/application*", 100)?;
    files.extend(project.find_files("**/src/main/resources/liquibase*.properties", 100)?);
    if files.len() >= 200 {
        return Err("Liquibase configuration discovery reached its limit.".into());
    }
    let mut found = BTreeSet::new();
    for file in files {
        let file = absolute(root, &file);
        let Some(resources) = resources_dir(&file) else {
            continue;
        };
        let text = project.read_file(&file)?;
        if !text.to_ascii_lowercase().contains("liquibase") {
            continue;
        }
        for value in text.lines().filter_map(changelog_value) {
            let relative = value
                .strip_prefix("classpath:")
                .ok_or("A configured Liquibase changelog is not a literal classpath: location; nothing is guessed.")?;
            found.insert(format!("{resources}/{}", literal_path(relative)?));
        }
    }
    if found.len() > 1 {
        return Err("Several different Liquibase changelog locations are configured; the master is ambiguous.".into());
    }
    Ok(found.into_iter().next())
}

struct MasterXml {
    includes: Vec<String>,
    include_all: Vec<(String, bool)>,
}

fn parse_master(text: &str) -> Result<MasterXml, Error> {
    if text.len() > MAX_MASTER {
        return Err("The master changelog exceeds 256 KiB; it is not read.".into());
    }
    let refuse = |why: &str| Error::from(format!("The master changelog cannot be used: {why}."));
    let mut reader = NsReader::from_str(text);
    let mut depth = 0usize;
    let mut master = MasterXml { includes: Vec::new(), include_all: Vec::new() };
    let mut rooted = false;
    for _ in 0..50_000 {
        let (namespace, event) = reader.read_resolved_event().map_err(|_| refuse("it is not well-formed XML"))?;
        let (element, closes) = match event {
            Event::Eof => return if rooted && depth == 0 { Ok(master) } else { Err(refuse("no closed databaseChangeLog root")) },
            Event::DocType(_) => return Err(refuse("a DOCTYPE is not supported")),
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                continue;
            }
            Event::Start(e) => (e, false),
            Event::Empty(e) => (e, true),
            _ => continue,
        };
        let bound = matches!(&namespace, ResolveResult::Bound(ns) if ns.as_ref() == NAMESPACE);
        let local = element.local_name();
        let local = local.as_ref();
        let mut attrs = Vec::new();
        for attribute in element.attributes() {
            let attribute = attribute.map_err(|_| refuse("attributes are invalid"))?;
            let value = attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0).map_err(|_| refuse("attributes contain entities"))?;
            attrs.push((attribute.key.as_ref().to_owned(), value.into_owned()));
        }
        if depth == 0 {
            if rooted || local != "databaseChangeLog" || !bound || element.name().as_ref() != "databaseChangeLog" {
                return Err(refuse("the root is not a default-namespace databaseChangeLog"));
            }
            if attrs.iter().any(|(key, _)| key == "logicalFilePath" || key == "xml:base") {
                return Err(refuse("logicalFilePath or xml:base makes include ownership ambiguous"));
            }
            rooted = true;
        } else if depth == 1 && (local == "include" || local == "includeAll") {
            if !bound || element.name().as_ref() != local {
                return Err(refuse("an include is not in the default Liquibase namespace"));
            }
            let get = |name: &str| attrs.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone());
            let relative = get("relativeToChangelogFile").is_some_and(|v| v == "true");
            if local == "include" {
                master.includes.push(get("file").ok_or_else(|| refuse("an include has no file"))?);
            } else {
                master.include_all.push((get("path").ok_or_else(|| refuse("an includeAll has no path"))?, relative));
            }
        }
        if !closes {
            depth += 1;
        }
    }
    Err(refuse("it has too many XML events"))
}

fn leading_number(name: &str) -> Option<(u64, usize)> {
    let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
    let rest = &name[digits.len()..];
    if digits.is_empty() || digits.len() > 12 || !rest.starts_with(['-', '_']) {
        return None;
    }
    Some((digits.parse().ok()?, digits.len()))
}

fn next_sequence(names: &[String]) -> Option<String> {
    let (highest, width) = names.iter().filter_map(|n| leading_number(n)).max_by_key(|(value, _)| *value)?;
    Some(format!("{:0width$}", highest + 1))
}

fn locate(project: &Project, root: &str) -> Result<Located, Error> {
    let master = match configured_master(project, root)? {
        Some(master) => master,
        None => {
            let found: Vec<String> = project
                .find_files("**/src/main/resources/db/changelog/db.changelog-master.*", 20)?
                .iter()
                .map(|f| absolute(root, f))
                .collect();
            match found.as_slice() {
                [only] => only.clone(),
                [] => return Err("No Liquibase master changelog is configured or found at db/changelog/db.changelog-master.*; the changelog folder cannot be determined.".into()),
                _ => return Err("Several db.changelog-master.* files exist; the master is ambiguous.".into()),
            }
        }
    };
    if path(&master).as_deref() != Some(master.as_str()) || !inside(root, &master) {
        return Err(Error::Stale);
    }
    if !master.ends_with(".xml") {
        return Err(format!("The master changelog {master} is not XML; only XML masters are supported, and nothing is guessed.").into());
    }
    let master_text = read_optional(project, &master)?.ok_or_else(|| Error::from(format!("The configured master changelog {master} does not exist.")))?;
    let parsed = parse_master(&master_text)?;
    let master_dir = master.rsplit_once('/').map(|(dir, _)| dir.to_owned()).ok_or(Error::Stale)?;
    let (directory, link) = match (parsed.include_all.as_slice(), parsed.includes.is_empty()) {
        ([], _) => (master_dir.clone(), Link::Include),
        ([(include_path, relative)], true) => {
            let include_path = literal_path(include_path)?;
            let directory = if *relative {
                format!("{master_dir}/{include_path}")
            } else {
                let resources = resources_dir(&master).ok_or("The master is outside src/main/resources, so its includeAll classpath path cannot be resolved.")?;
                format!("{resources}/{include_path}")
            };
            (directory, Link::IncludeAll { path: include_path.to_owned() })
        }
        _ => return Err("The master mixes include and includeAll or has several includeAll entries; where a new changelog belongs is ambiguous.".into()),
    };
    if path(&directory).as_deref() != Some(directory.as_str()) || !inside(root, &directory) {
        return Err(Error::Stale);
    }
    let relative_dir = directory.strip_prefix(&format!("{}/", root.trim_end_matches('/'))).ok_or(Error::Stale)?;
    if relative_dir.contains(['*', '?', '[', ']', '{', '}']) {
        return Err("The changelog directory has glob characters; it is not searched.".into());
    }
    let files = project.find_files(&format!("{relative_dir}/*"), MAX_FILES)?;
    if files.len() >= MAX_FILES as usize {
        return Err("The changelog directory has too many files; the sequence cannot be determined.".into());
    }
    let mut names = BTreeSet::new();
    for file in files.iter().map(|f| absolute(root, f)) {
        let (parent, name) = file.rsplit_once('/').ok_or(Error::Stale)?;
        let known = name.rsplit_once('.').is_some_and(|(_, ext)| EXTENSIONS.contains(&ext));
        if parent == directory && known && file != master {
            names.insert(name.to_owned());
        }
    }
    if link == Link::Include {
        for include in &parsed.includes {
            if let Some(name) = include.rsplit('/').next() {
                names.insert(name.to_owned());
            }
        }
    }
    let names: Vec<String> = names.into_iter().collect();
    let sequence = next_sequence(&names).ok_or("No existing changelog name has a numeric prefix like 001-; the sequence cannot be determined and is not guessed.")?;
    Ok(Located { master, master_text, directory, link, names, sequence })
}

fn include_line(name: &str) -> String {
    format!("<include file=\"{name}\" relativeToChangelogFile=\"true\"/>")
}

fn describe_options(picked: &[String]) -> Result<Vec<Danger>, Error> {
    picked
        .iter()
        .map(|key| match key.as_str() {
            "needs-backfill" => Ok(Danger::NeedsBackfill),
            "data-loss" => Ok(Danger::DataLoss),
            _ => Err(Error::Stale),
        })
        .collect()
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let root = pending.root.as_str();
    let baseline_path = target_in_root(project, root, BASELINE)?;
    let next_path = target_in_root(project, root, BASELINE_NEXT)?;
    if project.exists(&next_path)? || read_optional(project, &next_path)?.is_some() {
        return Err(format!("{next_path} exists: move it over {baseline_path} (or delete it) before generating another changelog.").into());
    }
    let baseline_text = read_optional(project, &baseline_path)?.ok_or("No schema baseline exists; run 'Create schema baseline…' first.")?;
    let baseline = crate::schema_snapshot::snapshot_from_json(&baseline_text).map_err(|e| Error::from(format!("The schema baseline is unreadable: {e}")))?;
    let model = scan(project, root)?;
    let plan = schema::diff(&baseline, &model).map_err(|e| Error::from(format!("The snapshots cannot be compared: {e:?}")))?;
    if plan.changes.is_empty() && plan.notes.is_empty() {
        return Err("The model matches the baseline; there is nothing to write.".into());
    }
    let located = locate(project, root)?;

    let withheld = [
        PickItem::new("needs-backfill", "NeedsBackfill: NOT NULL without default or value"),
        PickItem::new("data-loss", "DataLoss: drops and narrowing changes"),
    ];
    let picked = host::pick_many("Also emit changes withheld by default (each is marked in the preview)", &withheld)?.ok_or(Error::Cancelled)?;
    let mut emit = EmitOptions { allow: describe_options(&picked)?, ..EmitOptions::default() };
    ask_backfills(&plan, &mut emit)?;
    let author = host::input("changeSet author", Some("jpa-assist"))?.ok_or(Error::Cancelled)?;
    let id_prefix = host::input("changeSet id prefix (ids are <prefix>-<n>)", Some(&located.sequence))?.ok_or(Error::Cancelled)?;
    let description = host::input("Changelog description (ASCII letters and digits)", Some("jpa_model_changes"))?.ok_or(Error::Cancelled)?;
    let description = description_of(&description);
    if description.is_empty() {
        return Err("Enter a description containing ASCII letters or digits.".into());
    }

    let options = LiquibaseOptions {
        emit,
        author: author.trim().to_owned(),
        id_prefix: id_prefix.trim().to_owned(),
    };
    let emitted = liquibase::emit_xml(&plan, &options).map_err(|e| Error::from(format!("The changelog cannot be generated: {e:?}")))?;
    if emitted.emitted() == 0 {
        return Err("Every change is withheld or unsupported; there is nothing to write. Adjust the options or the model.".into());
    }
    let mut incomplete = Vec::new();
    if !baseline.complete {
        incomplete.push("the baseline snapshot is incomplete".to_owned());
    }
    if !model.complete {
        incomplete.push("the current model scan is incomplete".to_owned());
    }
    if !plan.notes.is_empty() {
        incomplete.push(format!("{} facts could not be compared (see NOTE comments)", plan.notes.len()));
    }
    if emitted.unsupported() > 0 {
        incomplete.push(format!("{} changes were skipped as unsupported", emitted.unsupported()));
    }
    if !incomplete.is_empty() {
        acknowledge(&incomplete)?;
    }

    let name = format!("{}-{description}.xml", located.sequence);
    if located.names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
        return Err(Error::Stale);
    }
    if matches!(located.link, Link::IncludeAll { .. }) && located.names.iter().any(|n| n.as_str() >= name.as_str()) {
        return Err("The new name would not sort after every file the includeAll directory already holds, so it would run out of order; nothing is written.".into());
    }
    let target = format!("{}/{name}", located.directory);
    if path(&target).as_deref() != Some(target.as_str()) || !inside(root, &target) {
        return Err(Error::Stale);
    }
    absent(project, &target)?;
    let text = emitted.render();
    let mut next_baseline = crate::schema_snapshot::snapshot_json(&model).map_err(Error::from)?;
    next_baseline.push('\n');

    let linking = match &located.link {
        Link::Include => format!(
            "The master {} is NOT edited (an edit of another document cannot be version-guarded). After creating the file, add this line to it:\n{}",
            located.master,
            include_line(&name)
        ),
        Link::IncludeAll { path } => format!("The master {} includes the directory '{path}' with includeAll, so no include line is needed.", located.master),
    };
    let preview = format!(
        "Target: {target}\nPlan: {}\nChangeSets: {} emitted, {} withheld by policy, {} unsupported\n{linking}\n\nExact changelog file:\n{text}\nAlso creates {next_path} ({} tables, deterministic serialization of this scan). The plugin never overwrites files: afterwards move it over {baseline_path} to refresh the baseline; until then this action refuses. Nothing is executed.",
        summary(&plan),
        emitted.emitted(),
        emitted.policy_skipped(),
        emitted.unsupported(),
        model.tables.len(),
    );
    if !ui_kit::prompts::confirm_exact("Create Liquibase changelog?", &preview)? {
        return Err(Error::Cancelled);
    }

    current(pending, doc)?;
    if read_optional(project, &baseline_path)?.as_deref() != Some(baseline_text.as_str()) {
        return Err(Error::Stale);
    }
    let fresh = scan(project, root)?;
    if crate::schema_snapshot::snapshot_json(&fresh).map_err(Error::from)? != next_baseline.trim_end() {
        return Err(Error::Stale);
    }
    if locate(project, root)? != located {
        return Err(Error::Stale);
    }
    absent(project, &target)?;
    absent(project, &next_path)?;
    if located.link == Link::Include {
        host::notify(
            NotificationLevel::Info,
            "JPA Assist",
            &format!("After the changelog is created, add to {}: {}", located.master, include_line(&name)),
        );
    }
    Ok(FileAction {
        title: format!("Create Liquibase changelog {name}"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![
            FileOperation::Create { path: target.clone(), text, overwrite: false },
            FileOperation::Create { path: next_path, text: next_baseline, overwrite: false },
        ],
        open: Some(target),
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ENTITY: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\npublic class Customer {\n    @Id Long id;\n    String name;\n}\n";
    const ENTITY_PLUS: &str = "package shop;\nimport jakarta.persistence.Column;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\npublic class Customer {\n    @Id Long id;\n    String name;\n    @Column(length = 100) String email;\n}\n";
    const PATH: &str = "/project/src/main/java/shop/Customer.java";
    const DIR: &str = "/project/src/main/resources/db/changelog";
    const MASTER_INCLUDES: &str = "<databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\">\n    <include file=\"001-init.xml\" relativeToChangelogFile=\"true\"/>\n    <include file=\"002-more.xml\" relativeToChangelogFile=\"true\"/>\n</databaseChangeLog>\n";
    const MASTER_ALL: &str = "<databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\">\n    <includeAll path=\"db/changelog/changes/\"/>\n</databaseChangeLog>\n";

    fn baseline_json(source: &str) -> String {
        let doc = Document { path: PATH.into(), language: "java".into(), text: source.into(), version: 0 };
        let snapshot = crate::schema_snapshot::snapshot_from_entities(&[doc]);
        format!("{}\n", crate::schema_snapshot::snapshot_json(&snapshot).unwrap())
    }

    fn setup(source: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(PATH, "java", source, &[]);
        host.add_file("src/main/java/shop/Customer.java", source);
        let doc = Document { path: PATH.into(), language: "java".into(), text: source.into(), version: 1 };
        (host, JpaAssist::default(), doc)
    }

    fn ready(source: &str) -> (MockHost, JpaAssist, Document) {
        let (host, plugin, doc) = setup(source);
        host.add_file("db/jpa-schema-baseline.json", &baseline_json(ENTITY));
        host.add_file("src/main/resources/db/changelog/db.changelog-master.xml", MASTER_INCLUDES);
        host.add_file("src/main/resources/db/changelog/001-init.xml", "<databaseChangeLog/>");
        host.add_file("src/main/resources/db/changelog/002-more.xml", "<databaseChangeLog/>");
        (host, plugin, doc)
    }

    fn generate(host: &MockHost, plugin: &mut JpaAssist, doc: &Document, allow: &[&str], confirms: &[bool]) -> Result<FileAction, Error> {
        let pending = plugin.file_actions(doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        assert!(pending.operations.is_empty() && pending.edits.is_empty());
        host.answer_next_prompt(Some(allow));
        host.answer_next_prompt(Some(&["Alice"]));
        host.answer_next_prompt(Some(&["add-email"]));
        host.answer_next_prompt(Some(&["Add customer email!"]));
        for confirm in confirms {
            host.answer_next_prompt(confirm.then_some(&[]));
        }
        plugin.resolve_file_action(doc, pending)
    }

    fn assert_parses(text: &str) {
        let mut reader = quick_xml::Reader::from_str(text);
        loop {
            match reader.read_event().expect("well-formed") {
                Event::Eof => break,
                _ => {}
            }
        }
    }

    #[test]
    fn include_mode_creates_beside_the_master_and_shows_the_include_line() {
        let (host, mut plugin, doc) = ready(ENTITY_PLUS);
        let resolved = generate(&host, &mut plugin, &doc, &[], &[true]).unwrap();
        let [FileOperation::Create { path, text, overwrite: false }, FileOperation::Create { path: next, text: next_text, overwrite: false }] =
            resolved.operations.as_slice()
        else {
            panic!("two creates")
        };
        assert_eq!(path, &format!("{DIR}/003-Add_customer_email.xml"));
        assert_eq!(next, "/project/db/jpa-schema-baseline.next.json");
        assert_eq!(next_text, &baseline_json(ENTITY_PLUS));
        assert!(text.contains("<changeSet id=\"add-email-1\" author=\"Alice\">"), "{text}");
        assert!(text.contains("<addColumn tableName=\"customer\">"), "{text}");
        assert!(text.contains("<column name=\"email\" type=\"varchar(100)\"/>"), "{text}");
        assert_parses(text);
        assert!(resolved.edits.is_empty(), "the master is never edited");
        assert_eq!(resolved.open.as_ref(), Some(path));
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }

    #[test]
    fn include_all_mode_needs_no_include_and_checks_ordering() {
        let (host, mut plugin, doc) = setup(ENTITY_PLUS);
        host.add_file("db/jpa-schema-baseline.json", &baseline_json(ENTITY));
        host.add_file("src/main/resources/db/changelog/db.changelog-master.xml", MASTER_ALL);
        host.add_file("src/main/resources/db/changelog/changes/009_a.xml", "<databaseChangeLog/>");
        let resolved = generate(&host, &mut plugin, &doc, &[], &[true]).unwrap();
        let FileOperation::Create { path, .. } = &resolved.operations[0] else { panic!() };
        assert_eq!(path, &format!("{DIR}/changes/010-Add_customer_email.xml"));

        let (host, mut plugin, doc) = setup(ENTITY_PLUS);
        host.add_file("db/jpa-schema-baseline.json", &baseline_json(ENTITY));
        host.add_file("src/main/resources/db/changelog/db.changelog-master.xml", MASTER_ALL);
        host.add_file("src/main/resources/db/changelog/changes/9-a.xml", "x");
        host.add_file("src/main/resources/db/changelog/changes/zz-late.xml", "x");
        let err = generate(&host, &mut plugin, &doc, &[], &[true]).unwrap_err();
        assert!(format!("{err:?}").contains("sort after"), "{err:?}");
    }

    #[test]
    fn configured_classpath_master_is_used_and_conflicts_refuse() {
        let (host, mut plugin, doc) = ready(ENTITY_PLUS);
        host.add_file("src/main/resources/application.properties", "spring.liquibase.change-log=classpath:db/changelog/db.changelog-master.xml\n");
        assert!(generate(&host, &mut plugin, &doc, &[], &[true]).is_ok());

        let (host, mut plugin, doc) = ready(ENTITY_PLUS);
        host.add_file("src/main/resources/application.yml", "spring:\n  liquibase:\n    change-log: \"classpath:/db/changelog/db.changelog-master.xml\"\n");
        assert!(generate(&host, &mut plugin, &doc, &[], &[true]).is_ok());

        for config in [
            "spring.liquibase.change-log=file:/etc/master.xml\n",
            "spring.liquibase.change-log=classpath:${x}/m.xml\n",
            "spring.liquibase.change-log=classpath:db/other.xml\nspring.liquibase.change-log=classpath:db/changelog/db.changelog-master.xml\n",
        ] {
            let (host, mut plugin, doc) = ready(ENTITY_PLUS);
            host.add_file("src/main/resources/application.properties", config);
            assert!(generate(&host, &mut plugin, &doc, &[], &[true]).is_err(), "{config}");
        }
        let (host, mut plugin, doc) = ready(ENTITY_PLUS);
        host.add_file("liquibase.properties", "changeLogFile=db/x.xml\n");
        assert!(generate(&host, &mut plugin, &doc, &[], &[true]).is_err());
    }

    #[test]
    fn undeterminable_locations_refuse() {
        let refused = |extra: &dyn Fn(&MockHost)| {
            let (host, mut plugin, doc) = ready(ENTITY_PLUS);
            extra(&host);
            generate(&host, &mut plugin, &doc, &[], &[true, true]).is_err()
        };
        assert!(refused(&|h| h.add_file("other/src/main/resources/db/changelog/db.changelog-master.xml", MASTER_INCLUDES)), "two masters");
        assert!(refused(&|h| h.add_file("src/main/resources/db/changelog/db.changelog-master.xml", "<databaseChangeLog xmlns=\"x\"/>")), "foreign namespace");
        assert!(refused(&|h| h.add_file("src/main/resources/db/changelog/db.changelog-master.xml", "<databaseChangeLog")), "malformed");
        assert!(refused(&|h| h.add_file("src/main/resources/db/changelog/db.changelog-master.xml", "<!DOCTYPE a><databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\"/>")), "doctype");
        assert!(refused(&|h| h.add_file("src/main/resources/db/changelog/db.changelog-master.xml", &MASTER_ALL.replace("</databaseChangeLog>", "<include file=\"a.xml\"/></databaseChangeLog>"))), "mixed");
        assert!(refused(&|h| h.add_file("src/main/resources/db/changelog/db.changelog-master.xml", &MASTER_ALL.replace("db/changelog/changes/", "${dir}"))), "dynamic path");
        assert!(refused(&|h| h.add_file("db/jpa-schema-baseline.next.json", "{}")), "staged baseline");

        let (host, mut plugin, doc) = setup(ENTITY_PLUS);
        host.add_file("db/jpa-schema-baseline.json", &baseline_json(ENTITY));
        assert!(generate(&host, &mut plugin, &doc, &[], &[true]).is_err(), "no master");

        let (host, mut plugin, doc) = setup(ENTITY_PLUS);
        host.add_file("db/jpa-schema-baseline.json", &baseline_json(ENTITY));
        host.add_file("src/main/resources/db/changelog/db.changelog-master.xml", "<databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\"/>");
        let err = generate(&host, &mut plugin, &doc, &[], &[true]).unwrap_err();
        assert!(format!("{err:?}").contains("numeric prefix"), "{err:?}");

        let (host, mut plugin, doc) = setup(ENTITY_PLUS);
        host.add_file("db/jpa-schema-baseline.json", &baseline_json(ENTITY));
        host.add_file("src/main/resources/db/changelog/db.changelog-master.yaml", "databaseChangeLog: []\n");
        assert!(generate(&host, &mut plugin, &doc, &[], &[true]).is_err(), "yaml master");

        let (host, mut plugin, doc) = ready(ENTITY);
        assert!(generate(&host, &mut plugin, &doc, &[], &[true]).is_err(), "no model changes");
        let (host, mut plugin, doc) = setup(ENTITY_PLUS);
        assert!(generate(&host, &mut plugin, &doc, &[], &[true]).is_err(), "no baseline");
        let (host, mut plugin, doc) = ready(ENTITY_PLUS);
        assert!(matches!(generate(&host, &mut plugin, &doc, &[], &[false]), Err(Error::Cancelled)));
    }

    const ENTITY_REQUIRED: &str = "package shop;\nimport jakarta.persistence.Column;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\npublic class Customer {\n    @Id Long id;\n    String name;\n    @Column(nullable = false, length = 5) String email;\n}\n";

    #[test]
    fn needs_backfill_adds_update_and_not_null_changesets() {
        let (host, mut plugin, doc) = ready(ENTITY_REQUIRED);
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.answer_next_prompt(Some(&["needs-backfill"]));
        host.answer_next_prompt(Some(&["n/a"]));
        host.answer_next_prompt(Some(&["Alice"]));
        host.answer_next_prompt(Some(&["add-email"]));
        host.answer_next_prompt(Some(&["Add customer email!"]));
        host.answer_next_prompt(Some(&[]));
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let FileOperation::Create { text, .. } = &resolved.operations[0] else { panic!() };
        assert!(text.contains("<column name=\"email\" value=\"n/a\"/>\n            <where>email IS NULL</where>"), "{text}");
        assert!(text.contains("<addNotNullConstraint tableName=\"customer\" columnName=\"email\" columnDataType=\"varchar(5)\"/>"), "{text}");
        assert_parses(text);
    }

    #[test]
    fn withheld_changes_and_invalid_options() {
        let wider = baseline_json(ENTITY_PLUS);
        let (host, mut plugin, doc) = ready(ENTITY);
        host.add_file("db/jpa-schema-baseline.json", &wider);
        assert!(generate(&host, &mut plugin, &doc, &[], &[true]).is_err(), "only a withheld drop");

        let (host, mut plugin, doc) = ready(ENTITY);
        host.add_file("db/jpa-schema-baseline.json", &wider);
        let resolved = generate(&host, &mut plugin, &doc, &["data-loss"], &[true]).unwrap();
        let FileOperation::Create { text, .. } = &resolved.operations[0] else { panic!() };
        assert!(text.contains("<dropColumn tableName=\"customer\" columnName=\"email\"/>\n    </changeSet>"), "{text}");
        assert_parses(text);

        let (host, mut plugin, doc) = ready(ENTITY_PLUS);
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.answer_next_prompt(Some(&[]));
        host.answer_next_prompt(Some(&["Alice"]));
        host.answer_next_prompt(Some(&["bad prefix!"]));
        host.answer_next_prompt(Some(&["x"]));
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
    }

    #[test]
    fn incomplete_baseline_needs_a_second_confirmation() {
        let incomplete = baseline_json(ENTITY).replace("\"complete\": true", "\"complete\": false");
        let (host, mut plugin, doc) = ready(ENTITY_PLUS);
        host.add_file("db/jpa-schema-baseline.json", &incomplete);
        assert!(matches!(generate(&host, &mut plugin, &doc, &[], &[false]), Err(Error::Cancelled)));
        let (host, mut plugin, doc) = ready(ENTITY_PLUS);
        host.add_file("db/jpa-schema-baseline.json", &incomplete);
        assert!(generate(&host, &mut plugin, &doc, &[], &[true, true]).is_ok());
    }

    #[test]
    fn oversized_preview_is_refused() {
        let (host, mut plugin, doc) = ready(ENTITY_PLUS);
        let mut fields = String::new();
        for n in 0..400 {
            fields.push_str(&format!("    Integer longColumnNameForPreviewLimit{n};\n"));
        }
        let big = ENTITY
            .replace("String name;\n", &format!("String name;\n{fields}"));
        host.add_file("src/main/java/shop/Customer.java", &big);
        host.open_editor(PATH, "java", &big, &[]);
        let doc = Document { text: big, ..doc };
        let err = generate(&host, &mut plugin, &doc, &[], &[true]).unwrap_err();
        assert!(format!("{err:?}").contains("16 KiB"), "{err:?}");
    }

    #[test]
    fn sequence_and_value_parsing() {
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(next_sequence(&names(&["001-a.xml", "009_b.xml"])).as_deref(), Some("010"));
        assert_eq!(next_sequence(&names(&["99-a.xml"])).as_deref(), Some("100"));
        assert_eq!(next_sequence(&names(&["a.xml", "1.xml", "x-1.xml"])), None);
        assert_eq!(changelog_value("spring.liquibase.change-log=classpath:a.xml # c").as_deref(), Some("classpath:a.xml"));
        assert_eq!(changelog_value("  changeLogFile: 'x.xml'").as_deref(), Some("x.xml"));
        assert_eq!(changelog_value("# change-log=x"), None);
        assert_eq!(changelog_value("changelog-dir=x"), None);
        assert_eq!(include_line("003-x.xml"), "<include file=\"003-x.xml\" relativeToChangelogFile=\"true\"/>");
    }
}
