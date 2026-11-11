//! Effective persistence settings that are provable without a database or a provider run.
//! Jakarta Persistence 3.1, section 8.2.1.6: a unit manages the annotated classes in the root of the persistence unit
//! (unless `exclude-unlisted-classes`), the classes of its mapping files and jar files, and its listed `<class>` elements.
//! Without a persistence.xml, literal Spring Boot `spring.jpa.*` / `spring.datasource.*` properties are the settings.
use crate::checks::Finding;
use crate::local_inheritance::{class_annotations, declaration_name, owned};
use crate::{model, xml};
use openidea_plugin::providers::Document;
use openidea_plugin::{Error, PickItem, Project, Severity, host};
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Unit {
    pub name: String,
    pub root: String,
    pub classes: Vec<String>,
    pub mapping_files: Vec<String>,
    pub jar_files: bool,
    pub exclude_unlisted: bool,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Namespace {
    Jakarta,
    Javax,
    Mixed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Coverage {
    Covered,
    Uncovered,
    Unknown,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Settings {
    pub origin: String,
    pub dialect: Option<String>,
    pub ddl_auto: Option<String>,
    pub naming: Option<String>,
    pub url: Option<String>,
}

impl Settings {
    /// `postgresql`, `mysql`, `mariadb` or `h2`; an unrecognized explicit dialect is not replaced by the URL's.
    pub fn family(&self) -> Option<&'static str> {
        let probe = |text: &str, pairs: &[(&str, &'static str)]| pairs.iter().find(|(n, _)| text.contains(n)).map(|(_, f)| *f);
        if let Some(dialect) = &self.dialect {
            return probe(&dialect.to_lowercase(), &[("postgres", "postgresql"), ("mariadb", "mariadb"), ("mysql", "mysql"), ("h2", "h2")]);
        }
        let url = self.url.as_deref()?.to_lowercase();
        probe(&url, &[("jdbc:postgresql:", "postgresql"), ("jdbc:mariadb:", "mariadb"), ("jdbc:mysql:", "mysql"), ("jdbc:h2:", "h2")])
    }

    pub fn summary(&self) -> String {
        let parts: Vec<String> = [("ddl-auto", &self.ddl_auto), ("naming", &self.naming)]
            .into_iter()
            .filter_map(|(k, v)| v.as_ref().map(|v| format!("{k}={v}")))
            .collect();
        if parts.is_empty() {
            return self.origin.clone();
        }
        format!("{}; {}", self.origin, parts.join(", "))
    }
}

#[derive(Debug, Default)]
pub struct Config {
    pub units: Vec<Unit>,
    pub namespace: Option<Namespace>,
    pub spring: Option<Settings>,
}

fn directory_of_meta_inf(path: &str) -> String {
    let root = path.strip_suffix("META-INF/persistence.xml").unwrap_or(path);
    root.trim_end_matches('/').to_owned()
}

/// Source sets of one module (`<module>/src/<set>/…`) share one classes root.
fn module_key(path: &str) -> Option<String> {
    let (prefix, rest) = match path.find("/src/") {
        Some(i) => (&path[..i], &path[i + 5..]),
        None => ("", path.strip_prefix("src/")?),
    };
    Some(format!("{prefix}/src/{}", rest.split('/').next()?))
}

pub fn parse_units(text: &str, path: &str) -> (Vec<Unit>, Namespace) {
    let root = directory_of_meta_inf(path);
    let mut units: Vec<Unit> = vec![];
    let mut namespace = Namespace::Unknown;
    for e in xml::parse(text) {
        let value = e.text.as_ref().map_or("", |(_, t)| t.as_str()).to_owned();
        match e.name.as_str() {
            "persistence" => {
                let uri = e.attributes.iter().find(|a| a.name == "xmlns").map_or("", |a| a.value.as_str());
                namespace = match uri {
                    "https://jakarta.ee/xml/ns/persistence" => Namespace::Jakarta,
                    "http://xmlns.jcp.org/xml/ns/persistence" | "http://java.sun.com/xml/ns/persistence" => Namespace::Javax,
                    _ => Namespace::Unknown,
                };
            }
            "persistence-unit" => units.push(Unit {
                name: e.attributes.iter().find(|a| a.name == "name").map_or(String::new(), |a| a.value.clone()),
                root: root.clone(),
                ..Unit::default()
            }),
            name => {
                let Some(unit) = units.last_mut() else { continue };
                match name {
                    "class" => unit.classes.push(value),
                    "mapping-file" => unit.mapping_files.push(value),
                    "jar-file" => unit.jar_files = true,
                    "exclude-unlisted-classes" => unit.exclude_unlisted = value != "false",
                    "property" => {
                        let attr = |n: &str| e.attributes.iter().find(|a| a.name == n).map(|a| a.value.clone());
                        if let (Some(k), Some(v)) = (attr("name"), attr("value")) {
                            unit.properties.insert(k, v);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    (units, namespace)
}

pub fn unit_settings(unit: &Unit) -> Settings {
    let get = |names: &[&str]| names.iter().find_map(|n| unit.properties.get(*n)).cloned();
    Settings {
        origin: format!("persistence unit {}", unit.name),
        dialect: get(&["hibernate.dialect"]),
        ddl_auto: get(&["hibernate.hbm2ddl.auto", "jakarta.persistence.schema-generation.database.action", "javax.persistence.schema-generation.database.action"]),
        naming: get(&["hibernate.physical_naming_strategy", "hibernate.implicit_naming_strategy"]),
        url: get(&["jakarta.persistence.jdbc.url", "javax.persistence.jdbc.url"]),
    }
}

fn literal(value: &str) -> Option<String> {
    let value = value.trim();
    let value = match value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\''))) {
        Some(inner) => inner,
        None => value.split(" #").next().unwrap_or("").trim(),
    };
    let dynamic = value.contains("${") || value.contains("#{") || value.contains('\\') || value.starts_with(['|', '>', '&', '*', '[', '{', '!']);
    (!value.is_empty() && !dynamic).then(|| value.to_owned())
}

pub fn properties_pairs(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with(['#', '!']))
        .filter_map(|l| {
            let at = l.find(['=', ':'])?;
            Some((l[..at].trim().to_owned(), l[at + 1..].trim().to_owned()))
        })
        .collect()
}

/// Leaf scalars of a single-document YAML file as dotted keys; multi-document files yield nothing.
pub fn yaml_pairs(text: &str) -> Vec<(String, String)> {
    if text.lines().filter(|l| l.trim_end() == "---").count() > 1 || text.contains("on-profile") {
        return vec![];
    }
    let mut stack: Vec<(usize, String)> = vec![];
    let mut pairs = vec![];
    for line in text.lines() {
        let body = line.trim_start();
        if body.is_empty() || body.starts_with(['#', '-']) || line.contains('\t') {
            continue;
        }
        let indent = line.len() - body.len();
        let Some((key, value)) = body.split_once(':') else { continue };
        while stack.last().is_some_and(|(i, _)| *i >= indent) {
            stack.pop();
        }
        let path = stack.iter().map(|(_, k)| k.as_str()).chain([key.trim()]).collect::<Vec<_>>().join(".");
        if value.trim().is_empty() {
            stack.push((indent, key.trim().to_owned()));
        } else {
            pairs.push((path, value.to_owned()));
        }
    }
    pairs
}

fn relaxed(key: &str) -> String {
    key.chars().filter(|c| !matches!(c, '-' | '_')).collect::<String>().to_lowercase()
}

/// A key bound more than once (here or in a profile file) has no single literal value and is dropped.
pub fn spring_settings(pairs: &[(String, String)]) -> Option<Settings> {
    let mut seen: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for (k, v) in pairs {
        seen.entry(relaxed(k)).or_default().push(v);
    }
    let get = |names: &[&str]| {
        names.iter().find_map(|n| match seen.get(*n).map(Vec::as_slice) {
            Some([only]) => literal(only),
            _ => None,
        })
    };
    let settings = Settings {
        origin: "Spring Boot properties".into(),
        dialect: get(&["spring.jpa.properties.hibernate.dialect", "spring.jpa.databaseplatform"]),
        ddl_auto: get(&["spring.jpa.hibernate.ddlauto"]),
        naming: get(&["spring.jpa.hibernate.naming.physicalstrategy", "spring.jpa.hibernate.naming.implicitstrategy"]),
        url: get(&["spring.datasource.url"]),
    };
    (settings.dialect.is_some() || settings.ddl_auto.is_some() || settings.naming.is_some() || settings.url.is_some()).then_some(settings)
}

pub fn source_namespace(text: &str) -> Namespace {
    match (text.contains("jakarta.persistence"), text.contains("javax.persistence")) {
        (true, true) => Namespace::Mixed,
        (true, false) => Namespace::Jakarta,
        (false, true) => Namespace::Javax,
        _ => Namespace::Unknown,
    }
}

impl Config {
    pub fn coverage(&self, fqcn: &str, path: &str, xml_may_declare: bool) -> Coverage {
        if self.units.is_empty() {
            return Coverage::Unknown;
        }
        let mut verdict = if xml_may_declare { Coverage::Unknown } else { Coverage::Uncovered };
        for unit in &self.units {
            if unit.classes.iter().any(|c| c == fqcn) {
                return Coverage::Covered;
            }
            if unit.jar_files || !unit.mapping_files.is_empty() {
                verdict = Coverage::Unknown;
                continue;
            }
            let same_module = matches!((module_key(path), module_key(&unit.root)), (Some(a), Some(b)) if a == b);
            let inside = same_module || path.starts_with(&format!("{}/", unit.root)) || unit.root.is_empty();
            if !unit.exclude_unlisted && inside {
                return Coverage::Covered;
            }
        }
        verdict
    }

    pub fn needs_choice(&self) -> bool {
        self.units.len() > 1
    }

    /// `index`: the answer of the unit prompt when several units exist.
    pub fn settings(&self, index: usize) -> Option<Settings> {
        match self.units.len() {
            0 => self.spring.clone(),
            _ => self.units.get(index).map(unit_settings),
        }
    }
}

const SKIPPED: [&str; 4] = ["/target/", "/build/", "/out/", "/node_modules/"];

pub fn load(project: &Project) -> Config {
    let root = project.root().unwrap_or_default();
    let relative = |p: &str| p.strip_prefix(&format!("{root}/")).unwrap_or(p).to_owned();
    let find = |glob: &str| -> Vec<String> {
        project.find_files(glob, 20).unwrap_or_default().into_iter().filter(|p| !SKIPPED.iter().any(|s| format!("/{p}").contains(s))).collect()
    };
    let mut config = Config::default();
    let mut namespaces = vec![];
    for file in find("**/persistence.xml") {
        let Ok(text) = project.read_file(&file) else { continue };
        let (units, namespace) = parse_units(&text, &relative(&file));
        config.units.extend(units);
        namespaces.push(namespace);
    }
    config.namespace = namespaces.first().copied().filter(|n| namespaces.iter().all(|o| o == n));
    if !config.units.is_empty() {
        return config;
    }
    let mut pairs = vec![];
    for glob in ["**/application*.properties", "**/application*.yml", "**/application*.yaml"] {
        for file in find(glob) {
            let Ok(text) = project.read_file(&file) else { continue };
            pairs.extend(if glob.ends_with("properties") { properties_pairs(&text) } else { yaml_pairs(&text) });
        }
    }
    config.spring = spring_settings(&pairs);
    config
}

/// The effective settings; several persistence units prompt for one, the first answer being the default.
pub fn effective(project: &Project) -> Result<Option<Settings>, Error> {
    let config = load(project);
    if !config.needs_choice() {
        return Ok(config.settings(0));
    }
    let items: Vec<PickItem> = config.units.iter().map(|u| PickItem::new(&u.name, &u.name)).collect();
    let Some(key) = crate::choose("Persistence unit", &items)? else {
        return Err(Error::Cancelled);
    };
    Ok(config.units.iter().position(|u| u.name == key).and_then(|i| config.settings(i)))
}

/// Info notes for the entity of one saved Java file: no persistence unit manages it, or it imports the other persistence namespace than the persistence.xml schema.
pub(super) fn findings(doc: &Document) -> Vec<Finding> {
    if doc.language != "java" || !doc.path.ends_with(".java") || doc.text.len() > 128 * 1024 || !crate::live::trusted_annotations(&doc.text) {
        return vec![];
    }
    let Some(project) = host::project_of(&doc.path) else { return vec![] };
    let masked = model::mask(&doc.text);
    let all = model::annotations(&masked);
    let Some(own) = class_annotations(&masked, &all) else { return vec![] };
    let imports: Vec<String> = doc.text.lines().filter_map(|l| l.trim().strip_prefix("import ")).map(|r| r.trim().trim_end_matches(';').trim().to_owned()).collect();
    let Some(entity) = own.iter().find(|a| a.name == "Entity" && owned(&imports, &masked, a)) else { return vec![] };
    let package = doc.text.lines().find_map(|l| l.trim().strip_prefix("package ")).map(|p| p.trim().trim_end_matches(';').trim());
    let (Some(package), Some(name)) = (package, declaration_name(&masked)) else { return vec![] };
    let fqcn = format!("{package}.{name}");
    let config = load(&project);
    let root = project.root().unwrap_or_default();
    let path = doc.path.strip_prefix(&format!("{root}/")).unwrap_or(&doc.path);
    let mut found = vec![];
    let declared_by_xml = crate::orm_xml::load(&doc.path).may_declare(&fqcn);
    if config.coverage(&fqcn, path, declared_by_xml) == Coverage::Uncovered {
        found.push(Finding::new(entity.start, entity.end(), Severity::Info, "jpa-entity-no-persistence-unit", format!("No persistence unit in this project manages {fqcn}: it is neither listed in a <class> element nor in the scanned root of a unit without exclude-unlisted-classes (Jakarta Persistence 3.1, section 8.2.1.6).")));
    }
    let source = source_namespace(&doc.text);
    let mismatch = matches!((source, config.namespace), (Namespace::Javax, Some(Namespace::Jakarta)) | (Namespace::Jakarta, Some(Namespace::Javax)));
    if mismatch {
        found.push(Finding::new(entity.start, entity.end(), Severity::Info, "jpa-persistence-namespace", "This entity imports one persistence namespace (javax or jakarta) while the persistence.xml schema declares the other; Hibernate 5 reads javax.persistence, Hibernate 6 reads jakarta.persistence."));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<persistence xmlns="https://jakarta.ee/xml/ns/persistence" version="3.0">
<persistence-unit name="main"><class>shop.Order</class><exclude-unlisted-classes>true</exclude-unlisted-classes>
<properties><property name="hibernate.dialect" value="org.hibernate.dialect.PostgreSQLDialect"/>
<property name="hibernate.hbm2ddl.auto" value="validate"/><property name="jakarta.persistence.jdbc.url" value="jdbc:h2:mem:x"/></properties></persistence-unit>
<persistence-unit name="audit"><properties><property name="jakarta.persistence.jdbc.url" value="jdbc:mysql://h/db"/></properties></persistence-unit></persistence>"#;

    fn config() -> Config {
        let (units, namespace) = parse_units(XML, "app/src/main/resources/META-INF/persistence.xml");
        Config { units, namespace: Some(namespace), spring: None }
    }

    #[test]
    fn units_carry_classes_exclusion_and_properties() {
        let config = config();
        assert_eq!(config.units.len(), 2);
        assert_eq!(config.namespace, Some(Namespace::Jakarta));
        assert!(config.units[0].exclude_unlisted && !config.units[1].exclude_unlisted);
        assert_eq!(config.units[0].classes, ["shop.Order"]);
        let main = config.settings(0).unwrap();
        assert_eq!((main.family(), main.ddl_auto.as_deref()), (Some("postgresql"), Some("validate")));
        assert_eq!(config.settings(1).unwrap().family(), Some("mysql"));
        assert!(config.needs_choice());
    }

    #[test]
    fn membership_follows_the_listing_and_scanning_rules() {
        let config = config();
        let listed = config.coverage("shop.Order", "app/src/main/java/shop/Order.java", false);
        let scanned = config.coverage("shop.Item", "app/src/main/java/shop/Item.java", false);
        let other_module = config.coverage("shop.Item", "other/src/main/java/shop/Item.java", false);
        assert_eq!((listed, scanned, other_module), (Coverage::Covered, Coverage::Covered, Coverage::Uncovered));
        let mut closed = config.units.clone();
        closed.remove(1);
        let closed = Config { units: closed, ..Config::default() };
        assert_eq!(closed.coverage("shop.Item", "app/src/main/java/shop/Item.java", false), Coverage::Uncovered);
        assert_eq!(closed.coverage("shop.Item", "app/src/main/java/shop/Item.java", true), Coverage::Unknown);
    }

    #[test]
    fn mapping_and_jar_files_keep_membership_unknown() {
        let text = "<persistence><persistence-unit name=\"u\"><mapping-file>META-INF/x.xml</mapping-file><exclude-unlisted-classes/></persistence-unit></persistence>";
        let (units, namespace) = parse_units(text, "META-INF/persistence.xml");
        assert_eq!(namespace, Namespace::Unknown);
        assert!(units[0].exclude_unlisted);
        let config = Config { units, ..Config::default() };
        assert_eq!(config.coverage("a.B", "src/a/B.java", false), Coverage::Unknown);
        assert_eq!(Config::default().coverage("a.B", "src/a/B.java", false), Coverage::Unknown);
    }

    #[test]
    fn spring_literals_become_settings_and_dynamic_values_do_not() {
        let props = "spring.jpa.database-platform=org.hibernate.dialect.MySQLDialect\nspring.jpa.hibernate.ddl-auto=update\n# c\nspring.jpa.hibernate.naming.physical-strategy=${NAMING}\nspring.datasource.url=jdbc:postgresql://h/db\n";
        let settings = spring_settings(&properties_pairs(props)).unwrap();
        assert_eq!((settings.family(), settings.ddl_auto.as_deref(), settings.naming.as_deref()), (Some("mysql"), Some("update"), None));
        let yaml = "spring:\n  jpa:\n    hibernate:\n      ddl-auto: validate\n      naming:\n        physical-strategy: org.hibernate.boot.model.naming.PhysicalNamingStrategyStandardImpl\n  datasource:\n    url: jdbc:h2:mem:t # inline\n";
        let settings = spring_settings(&yaml_pairs(yaml)).unwrap();
        assert_eq!((settings.family(), settings.ddl_auto.as_deref()), (Some("h2"), Some("validate")));
        assert!(settings.naming.unwrap().ends_with("PhysicalNamingStrategyStandardImpl"));
        let twice = "spring.jpa.hibernate.ddl-auto=update\nspring.jpa.hibernate.ddlAuto=none\n";
        assert_eq!(spring_settings(&properties_pairs(twice)), None);
        assert_eq!(yaml_pairs("a: 1\n---\nb: 2\n---\nc: 3\n"), vec![]);
        assert_eq!(config().spring, None);
    }

    #[test]
    fn source_namespaces_are_detected_and_mixed_is_reported_as_such() {
        assert_eq!(source_namespace("import jakarta.persistence.Entity;"), Namespace::Jakarta);
        assert_eq!(source_namespace("import javax.persistence.Entity;"), Namespace::Javax);
        assert_eq!(source_namespace("import javax.persistence.Id;\nimport jakarta.persistence.Entity;"), Namespace::Mixed);
        assert_eq!(source_namespace("class A {}"), Namespace::Unknown);
    }

    fn diagnose(host: &openidea_plugin::mock::MockHost, imports: &str) -> Vec<String> {
        use openidea_plugin::diagnostics::DiagnosticsProvider;
        let path = "src/main/java/shop/Item.java";
        let text = format!("package shop;\n{imports}\n@Entity class Item {{ @Id Long id; }}");
        host.add_file(path, &text);
        let doc = Document { path: format!("/project/{path}"), language: "java".into(), text, version: 1 };
        crate::JpaAssist::default().diagnostics(&doc).into_iter().filter_map(|d| d.code).filter(|c| c.starts_with("jpa-entity-no-persistence") || c == "jpa-persistence-namespace").collect()
    }

    #[test]
    fn uncovered_entities_and_namespace_mismatch_get_an_info() {
        let host = openidea_plugin::mock::MockHost::install();
        let jakarta = "import jakarta.persistence.Entity;\nimport jakarta.persistence.Id;";
        assert!(diagnose(&host, jakarta).is_empty(), "no persistence.xml");
        let xml = |body: &str| format!("<persistence xmlns=\"https://jakarta.ee/xml/ns/persistence\"><persistence-unit name=\"u\">{body}</persistence-unit></persistence>");
        host.add_file("src/main/resources/META-INF/persistence.xml", &xml("<exclude-unlisted-classes>true</exclude-unlisted-classes><class>shop.Other</class>"));
        assert_eq!(diagnose(&host, jakarta), ["jpa-entity-no-persistence-unit"]);
        host.add_file("src/main/resources/META-INF/persistence.xml", &xml("<exclude-unlisted-classes>true</exclude-unlisted-classes><class>shop.Item</class>"));
        assert!(diagnose(&host, jakarta).is_empty());
        host.add_file("src/main/resources/META-INF/persistence.xml", &xml(""));
        assert!(diagnose(&host, jakarta).is_empty(), "scanned");
        let javax = "import javax.persistence.Entity;\nimport javax.persistence.Id;";
        assert_eq!(diagnose(&host, javax), ["jpa-persistence-namespace"]);
        let mixed = "import javax.persistence.Entity;\nimport jakarta.persistence.Id;";
        assert!(diagnose(&host, mixed).is_empty());
    }
}
