//! The project's Hibernate naming strategy, read from literal configuration only.
//!
//! Verified against (fetched 2026-11): Spring Boot appendix (`spring.jpa.hibernate.naming.physical-strategy` and
//! `.implicit-strategy`, "fully qualified name"), Spring Boot v3.4.0 and v2.7.18 `HibernateProperties.Naming`
//! (defaults `SpringImplicitNamingStrategy` and Hibernate's `CamelCaseToUnderscoresNamingStrategy`; a configured value
//! replaces the default), Hibernate 6.6 `CamelCaseToUnderscoresNamingStrategy` and `PhysicalNamingStrategyStandardImpl`
//! sources, Hibernate 6.6 user guide ("Naming strategies": `hibernate.physical_naming_strategy`,
//! `hibernate.implicit_naming_strategy`, short names `default`/`jpa`).
use persistence_core::schema::Fact;

const MAX_FILES: u32 = 40;
const MAX_TEXT: usize = 256 * 1024;
const PHYSICAL_KEY: &str = "spring.jpa.hibernate.naming.physical-strategy";
const IMPLICIT_KEY: &str = "spring.jpa.hibernate.naming.implicit-strategy";
const CAMEL: &str = "org.hibernate.boot.model.naming.CamelCaseToUnderscoresNamingStrategy";
const SPRING_PHYSICAL: &str = "org.springframework.boot.orm.jpa.hibernate.SpringPhysicalNamingStrategy";
const STANDARD: &str = "org.hibernate.boot.model.naming.PhysicalNamingStrategyStandardImpl";
const SPRING_IMPLICIT: &str = "org.springframework.boot.orm.jpa.hibernate.SpringImplicitNamingStrategy";
const JPA_IMPLICIT: &str = "org.hibernate.boot.model.naming.ImplicitNamingStrategyJpaCompliantImpl";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Physical {
    CamelCaseToUnderscores,
    SpringPhysical,
    Standard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Implicit {
    Spring,
    JpaCompliant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamingStrategy {
    pub physical: Fact<Physical>,
    pub implicit: Fact<Implicit>,
    pub source: String,
}

impl NamingStrategy {
    pub fn unknown(reason: &str) -> Self {
        Self { physical: Fact::unknown(reason), implicit: Fact::unknown(reason), source: reason.to_owned() }
    }

    /// `CamelCaseToUnderscoresNamingStrategy` or the unknown reason, for labels.
    pub fn physical_label(&self) -> Option<&'static str> {
        match self.physical.known()? {
            Physical::CamelCaseToUnderscores => Some("CamelCaseToUnderscoresNamingStrategy"),
            Physical::SpringPhysical => Some("SpringPhysicalNamingStrategy"),
            Physical::Standard => Some("PhysicalNamingStrategyStandardImpl"),
        }
    }

    /// Origin label for a derived name: `derived (<strategy>)` when known, else today's unverified label.
    pub fn derived_label(&self) -> String {
        match self.physical_label() {
            Some(strategy) => format!("derived ({strategy})"),
            None => "derived (naming strategy not verified)".to_owned(),
        }
    }
}

/// Hibernate's `CamelCaseToUnderscoresNamingStrategy.apply`: dots become underscores, an underscore is inserted
/// between a lower-case letter and an upper-case letter that is followed by a lower-case letter, then lower-cased.
/// `customerID` therefore stays `customerid`, and `userName` becomes `user_name`.
fn camel_case_to_underscores(name: &str) -> String {
    let mut chars: Vec<char> = name.replace('.', "_").chars().collect();
    let mut i = 1;
    while i + 1 < chars.len() {
        if chars[i - 1].is_lowercase() && chars[i].is_uppercase() && chars[i + 1].is_lowercase() {
            chars.insert(i, '_');
            i += 1;
        }
        i += 1;
    }
    chars.into_iter().collect::<String>().to_lowercase()
}

/// The physical name for a logical JPA name, or `None` when the strategy is unknown or the name is not a plain
/// unquoted ASCII identifier (quoted identifiers keep their quoting rules and are not transformed here).
pub fn apply_physical(strategy: &NamingStrategy, logical: &str) -> Option<String> {
    let plain = !logical.is_empty() && logical.is_ascii() && !logical.contains(['"', '`', '[', ']', '\'']);
    if !plain {
        return None;
    }
    match strategy.physical.known()? {
        Physical::CamelCaseToUnderscores | Physical::SpringPhysical => Some(camel_case_to_underscores(logical)),
        Physical::Standard => Some(logical.to_owned()),
    }
}

fn physical_of(value: &str) -> Fact<Physical> {
    match value {
        CAMEL => Fact::Known(Physical::CamelCaseToUnderscores),
        SPRING_PHYSICAL => Fact::Known(Physical::SpringPhysical),
        STANDARD => Fact::Known(Physical::Standard),
        other => Fact::Unknown(format!("physical naming strategy {other} is not recognised")),
    }
}

fn implicit_of(value: &str) -> Fact<Implicit> {
    match value {
        SPRING_IMPLICIT => Fact::Known(Implicit::Spring),
        JPA_IMPLICIT | "default" | "jpa" => Fact::Known(Implicit::JpaCompliant),
        other => Fact::Unknown(format!("implicit naming strategy {other} is not recognised")),
    }
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    let value = match value.find(" #") {
        Some(at) => value[..at].trim(),
        None => value,
    };
    value.trim_matches(['"', '\'']).trim().to_owned()
}

/// `(key, value, from_later_document)` pairs for the two literal keys in a `.properties` or YAML file.
fn read_application(text: &str, yaml: bool) -> Vec<(&'static str, String, bool)> {
    read_keys(text, yaml, &[PHYSICAL_KEY, IMPLICIT_KEY])
}

/// Like [`read_application`] for any literal keys.
pub(crate) fn read_keys(text: &str, yaml: bool, keys: &[&'static str]) -> Vec<(&'static str, String, bool)> {
    let mut found = Vec::new();
    let mut later = false;
    let mut stack: Vec<(usize, String)> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if yaml && trimmed == "---" {
            later = true;
            stack.clear();
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        let (key, value) = if yaml {
            let Some((key, value)) = trimmed.split_once(':') else {
                continue;
            };
            (key.trim().trim_matches(['"', '\'']).to_owned(), value.to_owned())
        } else {
            let Some(at) = trimmed.find(['=', ':']) else {
                continue;
            };
            (trimmed[..at].trim().to_owned(), trimmed[at + 1..].to_owned())
        };
        let full = if yaml {
            let indent = line.len() - line.trim_start().len();
            while stack.last().is_some_and(|(i, _)| *i >= indent) {
                stack.pop();
            }
            let full = stack.iter().map(|(_, k)| k.as_str()).chain([key.as_str()]).collect::<Vec<_>>().join(".");
            if unquote(&value).is_empty() {
                stack.push((indent, key));
            }
            full
        } else {
            key
        };
        for &name in keys {
            if full == name {
                found.push((name, unquote(&value), later));
            }
        }
    }
    found
}

/// `(name, value)` of `<property .../>` elements whose name is one of the two Hibernate JPA properties.
fn read_persistence_xml(text: &str) -> Vec<(&'static str, String)> {
    let mut found = Vec::new();
    for tag in text.split("<property").skip(1) {
        let Some(tag) = tag.split('>').next() else {
            continue;
        };
        let attr = |name: &str| {
            let at = tag.find(&format!("{name}=\""))? + name.len() + 2;
            let rest = &tag[at..];
            Some(rest[..rest.find('"')?].trim().to_owned())
        };
        let (Some(name), Some(value)) = (attr("name"), attr("value")) else {
            continue;
        };
        match name.as_str() {
            "hibernate.physical_naming_strategy" => found.push(("physical", value)),
            "hibernate.implicit_naming_strategy" => found.push(("implicit", value)),
            _ => {}
        }
    }
    found
}

pub struct Sources<'a> {
    pub applications: &'a [(String, String)],
    pub persistence_xml: &'a [(String, String)],
    pub build_text: &'a str,
}

fn proves_spring_boot(build_text: &str) -> bool {
    build_text.contains("org.springframework.boot") && build_text.contains("spring-boot-starter-data-jpa")
}

/// Values of one setting across files: `Ok(None)` unset, `Ok(Some(v))` one agreed value, `Err` unusable.
pub(crate) fn settle(values: &[(String, String, bool)]) -> Result<Option<String>, String> {
    let mut distinct: Vec<&str> = Vec::new();
    for (_, value, _) in values {
        if !distinct.contains(&value.as_str()) {
            distinct.push(value);
        }
    }
    if distinct.len() > 1 {
        return Err(format!("conflicting values {}", distinct.join(", ")));
    }
    let Some(value) = distinct.first() else {
        return Ok(None);
    };
    if value.contains("${") || value.is_empty() {
        return Err(format!("value `{value}` is a placeholder or empty"));
    }
    if values.iter().all(|(_, _, profile)| *profile) {
        return Err("only set in profile-specific configuration, which may not be active".to_owned());
    }
    Ok(Some((*value).to_owned()))
}

pub fn detect_from(sources: &Sources) -> NamingStrategy {
    let mut physical: Vec<(String, String, bool)> = Vec::new();
    let mut implicit: Vec<(String, String, bool)> = Vec::new();
    for (path, text) in sources.applications {
        let file = path.rsplit('/').next().unwrap_or(path);
        let profile = file.starts_with("application-");
        for (key, value, later) in read_application(text, !file.ends_with(".properties")) {
            let target = if key == PHYSICAL_KEY { &mut physical } else { &mut implicit };
            target.push((path.clone(), value, profile || later));
        }
    }
    for (path, text) in sources.persistence_xml {
        for (kind, value) in read_persistence_xml(text) {
            let target = if kind == "physical" { &mut physical } else { &mut implicit };
            target.push((path.clone(), value, false));
        }
    }

    let boot = proves_spring_boot(sources.build_text);
    let resolve = |values: &[(String, String, bool)], what: &str| -> Result<Option<String>, String> {
        settle(values).map_err(|why| format!("{what} naming strategy: {why}"))
    };
    let physical_value = resolve(&physical, "physical");
    let implicit_value = resolve(&implicit, "implicit");
    let default_note = "Spring Boot default (HibernateProperties.Naming; assumes Boot's Hibernate auto-configuration is in use)";

    let physical_fact = match &physical_value {
        Ok(Some(value)) => physical_of(value),
        Ok(None) if boot => Fact::Known(Physical::CamelCaseToUnderscores),
        Ok(None) => Fact::unknown("no physical naming strategy is configured and Spring Boot is not provable from a literal build file"),
        Err(why) => Fact::Unknown(why.clone()),
    };
    let implicit_fact = match &implicit_value {
        Ok(Some(value)) => implicit_of(value),
        Ok(None) if boot => Fact::Known(Implicit::Spring),
        Ok(None) => Fact::unknown("no implicit naming strategy is configured and Spring Boot is not provable from a literal build file"),
        Err(why) => Fact::Unknown(why.clone()),
    };
    let configured = matches!(physical_value, Ok(Some(_))) || matches!(implicit_value, Ok(Some(_)));
    let source = if configured { "literal application properties or persistence.xml" } else if boot { default_note } else { "none found" };
    NamingStrategy { physical: physical_fact, implicit: implicit_fact, source: source.to_owned() }
}

pub(crate) fn skipped(path: &str) -> bool {
    ["/src/test/", "/target/", "/build/", "/node_modules/"].iter().any(|part| path.contains(part))
}

/// Bounded literal sources read with the existing `project.read` capability only; unreadable or oversized files
/// are skipped, so the result can only be less certain.
pub(crate) fn gather(project: &openidea_plugin::Project, patterns: &[&str], keep: &dyn Fn(&str) -> bool) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for pattern in patterns {
        let Ok(files) = project.find_files(pattern, MAX_FILES) else {
            continue;
        };
        for file in files.into_iter().filter(|f| !skipped(f) && keep(f)).take(MAX_FILES as usize) {
            if let Ok(text) = project.read_file(&file).map(|t| t.to_string()) {
                if text.len() <= MAX_TEXT {
                    out.push((file, text));
                }
            }
        }
    }
    out
}

pub(crate) const APPLICATION_PATTERNS: [&str; 3] = ["**/application*.properties", "**/application*.yml", "**/application*.yaml"];

pub(crate) fn is_main_resource(file: &str) -> bool {
    file.contains("/src/main/resources/") || file.starts_with("src/main/resources/")
}

pub fn detect(project: &openidea_plugin::Project) -> NamingStrategy {
    let applications = gather(project, &APPLICATION_PATTERNS, &is_main_resource);
    let persistence_xml = gather(project, &["**/persistence.xml"], &|_| true);
    let build = gather(project, &["**/pom.xml", "**/build.gradle", "**/build.gradle.kts"], &|_| true);
    let build_text: String = build.iter().map(|(_, text)| text.as_str()).collect::<Vec<_>>().join("\n");
    detect_from(&Sources { applications: &applications, persistence_xml: &persistence_xml, build_text: &build_text })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentifierPolicy {
    AsWritten,
    Lowercase,
    Uppercase,
}

impl IdentifierPolicy {
    pub fn from_setting(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("lowercase") => Self::Lowercase,
            Some("uppercase") => Self::Uppercase,
            _ => Self::AsWritten,
        }
    }

    pub fn apply(self, name: &str) -> String {
        match self {
            Self::AsWritten => name.to_owned(),
            Self::Lowercase => name.to_lowercase(),
            Self::Uppercase => name.to_uppercase(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOOT_BUILD: &str = "<parent><groupId>org.springframework.boot</groupId></parent><artifactId>spring-boot-starter-data-jpa</artifactId>";

    fn strategy(applications: &[(&str, &str)], xml: &[(&str, &str)], build: &str) -> NamingStrategy {
        let own = |items: &[(&str, &str)]| items.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
        detect_from(&Sources { applications: &own(applications), persistence_xml: &own(xml), build_text: build })
    }

    #[test]
    fn camel_case_edge_cases_follow_hibernate_source() {
        let name = |input: &str| camel_case_to_underscores(input);
        assert_eq!(name("OrderLine"), "order_line");
        assert_eq!(name("userName"), "user_name");
        assert_eq!(name("customerID"), "customerid");
        assert_eq!(name("HTTPServer"), "httpserver");
        assert_eq!(name("address2Line"), "address2line");
        assert_eq!(name("already_snake"), "already_snake");
        assert_eq!(name("my_Field"), "my_field");
        assert_eq!(name("a.bC"), "a_bc");
        assert_eq!(name("aBc"), "a_bc");
        assert_eq!(name("X"), "x");
    }

    #[test]
    fn apply_physical_is_none_for_unknown_and_quoted_names() {
        let unknown = NamingStrategy::unknown("none");
        let camel = strategy(&[], &[], BOOT_BUILD);
        let standard = strategy(&[("/p/src/main/resources/application.properties", &format!("{PHYSICAL_KEY}={STANDARD}"))], &[], "");
        assert_eq!(apply_physical(&unknown, "Order"), None);
        assert_eq!(apply_physical(&camel, "OrderLine").as_deref(), Some("order_line"));
        assert_eq!(apply_physical(&camel, "`Order`"), None);
        assert_eq!(apply_physical(&camel, ""), None);
        assert_eq!(apply_physical(&standard, "OrderLine").as_deref(), Some("OrderLine"));
    }

    #[test]
    fn detects_properties_yaml_and_persistence_xml() {
        let props = strategy(&[("/p/src/main/resources/application.properties", &format!("# c\n{PHYSICAL_KEY} = {STANDARD}\n{IMPLICIT_KEY}=jpa"))], &[], "");
        assert_eq!(props.physical, Fact::Known(Physical::Standard));
        assert_eq!(props.implicit, Fact::Known(Implicit::JpaCompliant));

        let yaml = "spring:\n  jpa:\n    hibernate:\n      naming:\n        physical-strategy: \"org.hibernate.boot.model.naming.PhysicalNamingStrategyStandardImpl\" # keep\n";
        let nested = strategy(&[("/p/src/main/resources/application.yml", yaml)], &[], "");
        assert_eq!(nested.physical, Fact::Known(Physical::Standard));

        let xml = "<properties><property name=\"hibernate.physical_naming_strategy\" value=\"com.acme.Custom\"/></properties>";
        let custom = strategy(&[], &[("/p/src/main/resources/META-INF/persistence.xml", xml)], BOOT_BUILD);
        assert!(matches!(&custom.physical, Fact::Unknown(r) if r.contains("com.acme.Custom")));
        assert_eq!(custom.implicit, Fact::Known(Implicit::Spring));
    }

    #[test]
    fn profile_disagreement_placeholders_and_profile_only_values_are_unknown() {
        let base = ("/p/src/main/resources/application.properties", format!("{PHYSICAL_KEY}={STANDARD}"));
        let dev = ("/p/src/main/resources/application-dev.properties", format!("{PHYSICAL_KEY}={CAMEL}"));
        let conflict = strategy(&[(base.0, &base.1), (dev.0, &dev.1)], &[], BOOT_BUILD);
        assert!(matches!(&conflict.physical, Fact::Unknown(r) if r.contains("conflicting")));

        let agree = strategy(&[(base.0, &base.1), ("/p/src/main/resources/application-dev.properties", &base.1)], &[], "");
        assert_eq!(agree.physical, Fact::Known(Physical::Standard));

        let profile_only = strategy(&[(dev.0, &dev.1)], &[], BOOT_BUILD);
        assert!(matches!(&profile_only.physical, Fact::Unknown(r) if r.contains("profile")));

        let placeholder = strategy(&[(base.0, &format!("{PHYSICAL_KEY}=${{NAMING}}"))], &[], BOOT_BUILD);
        assert!(matches!(&placeholder.physical, Fact::Unknown(r) if r.contains("placeholder")));

        let later_doc = strategy(&[("/p/src/main/resources/application.yml", &format!("a: b\n---\nspring.jpa.hibernate.naming.physical-strategy: {CAMEL}\n"))], &[], BOOT_BUILD);
        assert!(matches!(&later_doc.physical, Fact::Unknown(_)));
    }

    #[test]
    fn nothing_configured_defaults_only_when_spring_boot_is_provable() {
        let boot = strategy(&[], &[], BOOT_BUILD);
        assert_eq!(boot.physical, Fact::Known(Physical::CamelCaseToUnderscores));
        assert_eq!(boot.implicit, Fact::Known(Implicit::Spring));
        assert_eq!(boot.derived_label(), "derived (CamelCaseToUnderscoresNamingStrategy)");

        let plain = strategy(&[], &[], "<artifactId>hibernate-core</artifactId>");
        assert!(matches!(plain.physical, Fact::Unknown(_)));
        assert_eq!(plain.derived_label(), "derived (naming strategy not verified)");
    }

    #[test]
    fn identifier_policy_defaults_to_as_written() {
        assert_eq!(IdentifierPolicy::from_setting(None), IdentifierPolicy::AsWritten);
        assert_eq!(IdentifierPolicy::from_setting(Some("bogus")), IdentifierPolicy::AsWritten);
        assert_eq!(IdentifierPolicy::from_setting(Some("uppercase")).apply("OrderLine"), "ORDERLINE");
        assert_eq!(IdentifierPolicy::from_setting(Some("lowercase")).apply("OrderLine"), "orderline");
        assert_eq!(IdentifierPolicy::AsWritten.apply("OrderLine"), "OrderLine");
    }
}
