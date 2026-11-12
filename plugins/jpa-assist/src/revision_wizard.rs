//! Create Revision Entity wizard. The JPA Buddy documentation (Entity Designer, Envers Support, Revision Entity) lists
//! what the wizard lets the user choose: the parent entity (DefaultRevisionEntity or
//! DefaultTrackingModifyingEntitiesRevisionEntity; the Hibernate class is DefaultTrackingModifiedEntitiesRevisionEntity), the table name, a new or an existing revision listener, the revision
//! number and timestamp fields (only when no parent is set) and the source root and package.
//! Both default parents are `@MappedSuperclass` types of `org.hibernate.envers` that already declare the revision number
//! and timestamp; the tracking variant adds `modifiedEntityNames`.
use super::{ENVERS, MAX_EXTRA, MAX_SCANNED, RESERVED_TYPES};
use super::*;

const DEFAULT_PARENT: &str = "DefaultRevisionEntity";
/// The documentation spells it DefaultTrackingModifyingEntitiesRevisionEntity; the Hibernate class is ...Modified... (checked in hibernate-envers 6.6.49).
const TRACKING_PARENT: &str = "DefaultTrackingModifiedEntitiesRevisionEntity";
const MAX_ROOTS: usize = 8;
const MAX_LISTENERS: usize = 16;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Parent {
    None,
    Default,
    Tracking,
}

impl Parent {
    fn simple(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Default => Some(DEFAULT_PARENT),
            Self::Tracking => Some(TRACKING_PARENT),
        }
    }

    fn inherited_fields(self) -> &'static [&'static str] {
        match self {
            Self::None => &[],
            Self::Default => &["id", "timestamp"],
            Self::Tracking => &["id", "timestamp", "modifiedEntityNames"],
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub(super) enum ListenerChoice {
    None,
    New,
    Existing(String),
}

pub(super) struct Options<'a> {
    pub package: &'a str,
    pub ns: &'a str,
    pub directory: &'a str,
    pub name: &'a str,
    pub table: &'a str,
    pub parent: Parent,
    pub number: &'a str,
    pub timestamp: &'a str,
    pub extra: &'a [String],
    pub listener: ListenerChoice,
    pub crlf: bool,
}

pub(super) fn render(options: &Options) -> Result<Vec<(String, String)>, Error> {
    let Options { package, ns, directory, name, table, parent, number, timestamp, extra, .. } = *options;
    if !["jakarta.persistence", "javax.persistence"].contains(&ns) {
        return Err(Error::Stale);
    }
    let listener_name = format!("{name}Listener");
    let new_listener = options.listener == ListenerChoice::New;
    if !identifier(name) || RESERVED_TYPES.contains(&name) || parent.simple() == Some(name) || (new_listener && (!identifier(&listener_name) || RESERVED_TYPES.contains(&listener_name.as_str()))) {
        return Err("Choose a valid revision entity name that does not clash with a type the file imports.".into());
    }
    if !identifier(table) {
        return Err("Choose a table name made of letters, digits and underscores.".into());
    }
    let own_fields: Vec<&str> = if parent == Parent::None { vec![number, timestamp] } else { Vec::new() };
    let mut seen: HashSet<&str> = parent.inherited_fields().iter().copied().collect();
    seen.extend(own_fields.iter().copied());
    let distinct_own = own_fields.iter().all(|field| identifier(field)) && own_fields.len() == own_fields.iter().collect::<HashSet<_>>().len();
    if !distinct_own || extra.len() > MAX_EXTRA || extra.iter().any(|field| !identifier(field) || !seen.insert(field.as_str())) {
        return Err(format!("Field names must be distinct valid names, none of them inherited from the parent; at most {MAX_EXTRA} extra fields.").into());
    }
    let mut members = String::new();
    if parent == Parent::None {
        members.push_str(&format!("    @Id\n    @GeneratedValue\n    @RevisionNumber\n    private int {number};\n\n    @RevisionTimestamp\n    private long {timestamp};\n"));
    }
    for field in extra {
        members.push_str(&format!("{}    private String {field};\n", if members.is_empty() { "" } else { "\n" }));
    }
    let mut accessors: Vec<(String, String)> = Vec::new();
    if parent == Parent::None {
        accessors.push(("int".to_owned(), number.to_owned()));
        accessors.push(("long".to_owned(), timestamp.to_owned()));
    }
    accessors.extend(extra.iter().map(|field| ("String".to_owned(), field.clone())));
    for (ty, field) in &accessors {
        let cap = capitalize(field);
        members.push_str(&format!(
            "\n    public {ty} get{cap}() {{\n        return {field};\n    }}\n\n    public void set{cap}({ty} {field}) {{\n        this.{field} = {field};\n    }}\n"
        ));
    }
    let mut imports = vec![format!("{ns}.Entity"), format!("{ns}.Table"), format!("{ENVERS}.RevisionEntity")];
    if parent == Parent::None {
        imports.extend([format!("{ns}.GeneratedValue"), format!("{ns}.Id"), format!("{ENVERS}.RevisionNumber"), format!("{ENVERS}.RevisionTimestamp")]);
    }
    if let Some(simple) = parent.simple() {
        imports.push(format!("{ENVERS}.{simple}"));
    }
    let annotation = match &options.listener {
        ListenerChoice::None => "@RevisionEntity".to_owned(),
        ListenerChoice::New => format!("@RevisionEntity({listener_name}.class)"),
        ListenerChoice::Existing(fqcn) => {
            let (listener_package, simple) = fqcn.rsplit_once('.').ok_or(Error::Stale)?;
            if !identifier(simple) || listener_package.split('.').any(|part| !identifier(part)) || RESERVED_TYPES.contains(&simple) || simple == name {
                return Err("The chosen listener class name clashes with a type the file imports.".into());
            }
            if listener_package != package {
                if imports.iter().any(|import| import.rsplit('.').next() == Some(simple)) {
                    return Err("The chosen listener class name clashes with a type the file imports.".into());
                }
                imports.push(fqcn.clone());
            }
            format!("@RevisionEntity({simple}.class)")
        }
    };
    imports.sort();
    let import_block: String = imports.iter().map(|import| format!("import {import};\n")).collect();
    let extends = parent.simple().map_or_else(String::new, |simple| format!(" extends {simple}"));
    let entity = format!("package {package};\n\n{import_block}\n@Entity\n@Table(name = \"{table}\")\n{annotation}\npublic class {name}{extends} {{\n{members}}}\n");
    let mut files = vec![(name.to_owned(), entity)];
    if new_listener {
        files.push((
            listener_name.clone(),
            format!(
                "package {package};\n\nimport {ENVERS}.RevisionListener;\n\npublic class {listener_name} implements RevisionListener {{\n    @Override\n    public void newRevision(Object revisionEntity) {{\n        {name} revision = ({name}) revisionEntity;\n    }}\n}}\n"
            ),
        ));
    }
    files
        .into_iter()
        .map(|(stem, text)| {
            let target = format!("{directory}/{stem}.java");
            let text = if options.crlf { text.replace('\n', "\r\n") } else { text };
            if text.len() > MAX_TEXT || path(&target).as_deref() != Some(target.as_str()) {
                return Err(Error::from("Generated source exceeds the supported bound."));
            }
            let entity = stem != name || parse_class(&text, &target).is_some_and(|class| class.kind == Kind::Entity);
            entity.then_some((target, text)).ok_or(Error::Stale)
        })
        .collect()
}

/// Fully qualified name of the public top-level class of a Java source that implements `org.hibernate.envers.RevisionListener`.
pub(super) fn listener_of(text: &str) -> Option<String> {
    let masked = mask(text);
    let (at, _) = crate::model::declaration_keyword(&masked, 0, &["class"])?;
    let open = masked[at..].find('{')? + at;
    let header = &masked[at + "class".len()..open];
    if header.contains('<') {
        return None;
    }
    let mut tokens = header.split(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '.'))).filter(|token| !token.is_empty());
    let name = tokens.next()?;
    let rest: Vec<&str> = tokens.collect();
    let implemented = &rest[rest.iter().position(|token| *token == "implements")? + 1..];
    let imported = masked.contains(&format!("import {ENVERS}.RevisionListener;")) || masked.contains(&format!("import {ENVERS}.*;"));
    let named = implemented.contains(&"RevisionListener") && imported || implemented.contains(&format!("{ENVERS}.RevisionListener").as_str());
    let before = masked[..at].trim_end();
    if !named || !(before.ends_with("public") || before.ends_with("public final")) || !identifier(name) {
        return None;
    }
    let package_at = words(&masked, "package").next()?;
    let package = masked[package_at + "package".len()..].split(';').next()?.trim();
    (!package.is_empty() && package.split('.').all(identifier)).then(|| format!("{package}.{name}"))
}

fn existing_listeners(project: &Project) -> Result<Vec<String>, Error> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut found = Vec::new();
    let files = project.find_files("**/*.java", MAX_SCANNED as u32 + 1)?;
    if files.len() > MAX_SCANNED {
        return Err("Existing revision listener discovery is incomplete for a project this large; nothing was created.".into());
    }
    for file in files {
        if Instant::now() >= deadline {
            return Err("Existing revision listener discovery timed out; nothing was created.".into());
        }
        let text = match project.read_file(&file) {
            Ok(text) => text,
            Err(Error::NotFound(_) | Error::Invalid(_)) => continue,
            Err(error) => return Err(error),
        };
        if text.contains("RevisionListener") && let Some(fqcn) = listener_of(&text) {
            found.push(fqcn);
        }
    }
    found.sort();
    found.dedup();
    found.truncate(MAX_LISTENERS);
    Ok(found)
}

fn own_root(doc_path: &str, package: &str) -> Option<String> {
    let directory = doc_path.rsplit_once('/')?.0;
    directory.strip_suffix(&format!("/{}", package.replace('.', "/"))).map(str::to_owned)
}

/// The invoking file's own Java source root first, then the other `src/<set>/java` roots of the same module that hold Java sources.
fn source_roots(project: &Project, own: &str) -> Result<Vec<String>, Error> {
    let module = own.split_once("/src/").map_or("", |(module, _)| module);
    let prefix = format!("{module}/src/");
    let mut others = HashSet::new();
    if own.starts_with(&prefix) {
        for file in project.find_files("**/*.java", MAX_SCANNED as u32 + 1)? {
            let Some((set, after)) = file.strip_prefix(&prefix).and_then(|rest| rest.split_once('/')) else {
                continue;
            };
            if after.starts_with("java/") {
                others.insert(format!("{prefix}{set}/java"));
            }
        }
    }
    others.remove(own);
    let mut others: Vec<String> = others.into_iter().collect();
    others.sort();
    others.truncate(MAX_ROOTS - 1);
    Ok(std::iter::once(own.to_owned()).chain(others).collect())
}

fn parent_choice() -> Result<Parent, Error> {
    let items = [
        PickItem::new("none", "No parent: declare the revision number and timestamp fields"),
        PickItem::new("default", DEFAULT_PARENT),
        PickItem::new("tracking", "DefaultTrackingModifiedEntitiesRevisionEntity (documented as DefaultTrackingModifyingEntitiesRevisionEntity)"),
    ];
    match host::pick("Revision entity parent", &items)?.ok_or(Error::Cancelled)?.as_str() {
        "none" => Ok(Parent::None),
        "default" => Ok(Parent::Default),
        "tracking" => Ok(Parent::Tracking),
        _ => Err(Error::Stale),
    }
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    super::super::current(pending, doc)?;
    let s = iq::source(doc).filter(|s| !s.class.kotlin && doc.language == "java").ok_or("A revision entity is generated for Java projects only; Kotlin is refused.")?;
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    super::proof(project, &pending.root, directory)?;
    super::no_existing_revision_entity(project)?;
    let current_package = s.class.package.clone().ok_or(Error::Stale)?;
    let name = iq::input("Revision entity name", "CustomRevisionEntity")?;
    let fields = host::input("Extra String fields, comma separated (empty for none)", Some("username"))?.ok_or(Error::Cancelled)?;
    let extra: Vec<String> = fields.split(',').map(str::trim).filter(|f| !f.is_empty()).map(str::to_owned).collect();
    let mut choices = vec![PickItem::new("none", "No listener"), PickItem::new("listener", "Also create a RevisionListener skeleton")];
    let existing = existing_listeners(project)?;
    choices.extend(existing.iter().map(|fqcn| PickItem::new(&format!("existing:{fqcn}"), &format!("Use the existing listener {fqcn}"))));
    let listener = match host::pick("RevisionListener", &choices)?.ok_or(Error::Cancelled)?.as_str() {
        "none" => ListenerChoice::None,
        "listener" => ListenerChoice::New,
        other => ListenerChoice::Existing(other.strip_prefix("existing:").filter(|fqcn| existing.iter().any(|e| e == fqcn)).ok_or(Error::Stale)?.to_owned()),
    };
    let parent = parent_choice()?;
    let table = iq::input("Revision table name", "revinfo")?;
    let (number, timestamp) = if parent == Parent::None {
        (iq::input("Revision number field name", "id")?, iq::input("Revision timestamp field name", "timestamp")?)
    } else {
        (String::new(), String::new())
    };
    let own = own_root(&doc.path, &current_package).ok_or("The source root of this file cannot be derived from its package; nothing was created.")?;
    let roots = source_roots(project, &own)?;
    let root = if roots.len() == 1 {
        own
    } else {
        let relative = |root: &str| root.strip_prefix(&format!("{}/", pending.root.trim_end_matches('/'))).unwrap_or(root).to_owned();
        let items: Vec<_> = roots.iter().map(|root| PickItem::new(root, &relative(root))).collect();
        let chosen = host::pick("Source root", &items)?.ok_or(Error::Cancelled)?;
        roots.into_iter().find(|root| *root == chosen).ok_or(Error::Stale)?
    };
    let package = iq::input("Package", &current_package)?;
    if package.is_empty() || package.split('.').any(|part| !identifier(part)) {
        return Err("Choose a package of dot separated valid names.".into());
    }
    let target_directory = format!("{root}/{}", package.replace('.', "/"));
    let files = render(&Options {
        package: &package,
        ns: s.ns,
        directory: &target_directory,
        name: &name,
        table: &table,
        parent,
        number: &number,
        timestamp: &timestamp,
        extra: &extra,
        listener: listener.clone(),
        crlf: doc.text.contains("\r\n"),
    })?;
    for (target, _) in &files {
        if !inside(&pending.root, target) {
            return Err(Error::Stale);
        }
        no_collision(project, &pending.root, &target_directory, file_stem(target), None)?;
    }
    let shown: String = files.iter().map(|(target, text)| format!("New file: {target}\n\n{text}\n")).collect();
    let note = match parent {
        Parent::Tracking => " The tracking parent needs hibernate.envers.track_entities_changed_in_revision=true.",
        _ => "",
    };
    if !ui_kit::prompts::confirm_exact(
        "Create Envers revision entity?",
        &format!("Source: {}\nSource version: {}\nNamespace: {}\n\n{shown}The revision number is an int as in the Hibernate user guide example. Envers allows one revision entity per persistence unit; other mappings are not read.{note} No existing file is overwritten.", doc.path, doc.version, s.ns),
    )? {
        return Err(Error::Cancelled);
    }
    super::super::current(pending, doc)?;
    for (target, _) in &files {
        no_collision(project, &pending.root, &target_directory, file_stem(target), None)?;
    }
    Ok(FileAction {
        title: format!("Create Envers revision entity {name}"),
        kind: Some("refactor.rewrite".into()),
        operations: files.into_iter().map(|(path, text)| FileOperation::Create { path, text, overwrite: false }).collect(),
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    fn options<'a>(parent: Parent, listener: ListenerChoice, extra: &'a [String]) -> Options<'a> {
        Options {
            package: "shop.audit",
            ns: "jakarta.persistence",
            directory: "/project/src/main/java/shop/audit",
            name: "Rev",
            table: "revinfo",
            parent,
            number: "id",
            timestamp: "timestamp",
            extra,
            listener,
            crlf: false,
        }
    }

    fn entity(options: &Options) -> String {
        render(options).unwrap().remove(0).1
    }

    #[test]
    fn parent_is_extended_and_declares_no_revision_fields() {
        for (parent, simple) in [(Parent::Default, DEFAULT_PARENT), (Parent::Tracking, TRACKING_PARENT)] {
            let text = entity(&options(parent, ListenerChoice::None, &[]));
            assert!(text.contains(&format!("import org.hibernate.envers.{simple};")), "{text}");
            assert!(text.contains(&format!("public class Rev extends {simple} {{\n}}\n")), "{text}");
            assert!(!text.contains("@RevisionNumber") && !text.contains("@Id") && !text.contains("GeneratedValue"), "{text}");
            assert!(text.contains("@Table(name = \"revinfo\")"));
        }
    }

    #[test]
    fn parent_with_extra_fields_keeps_the_extra_accessors_only() {
        let extra = ["username".to_owned()];
        let text = entity(&options(Parent::Default, ListenerChoice::None, &extra));
        assert!(text.contains("    private String username;\n\n    public String getUsername()"), "{text}");
        assert!(!text.contains("getId") && !text.contains("getTimestamp"), "{text}");
    }

    #[test]
    fn table_and_revision_field_names_are_taken_from_the_wizard() {
        let mut o = options(Parent::None, ListenerChoice::None, &[]);
        o.table = "audit_revision";
        o.number = "rev";
        o.timestamp = "revtstmp";
        let text = entity(&o);
        assert!(text.contains("@Table(name = \"audit_revision\")"), "{text}");
        assert!(text.contains("@RevisionNumber\n    private int rev;"), "{text}");
        assert!(text.contains("@RevisionTimestamp\n    private long revtstmp;"), "{text}");
        assert!(text.contains("public int getRev()") && text.contains("public void setRevtstmp(long revtstmp)"), "{text}");
    }

    #[test]
    fn existing_listener_is_imported_from_another_package_and_not_generated() {
        let o = options(Parent::None, ListenerChoice::Existing("shop.listeners.AuditListener".into()), &[]);
        let files = render(&o).unwrap();
        assert_eq!(files.len(), 1, "no listener file for an existing listener");
        assert!(files[0].1.contains("import shop.listeners.AuditListener;"), "{}", files[0].1);
        assert!(files[0].1.contains("@RevisionEntity(AuditListener.class)"));
        let same = options(Parent::None, ListenerChoice::Existing("shop.audit.AuditListener".into()), &[]);
        let text = render(&same).unwrap().remove(0).1;
        assert!(!text.contains("import shop.audit."), "{text}");
    }

    #[test]
    fn wizard_refusals() {
        let extra = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
        let fails = |mutate: &dyn Fn(&mut Options), extra: &[String]| {
            let mut o = options(Parent::Default, ListenerChoice::None, extra);
            mutate(&mut o);
            render(&o).is_err()
        };
        assert!(fails(&|o| o.table = "bad name", &[]));
        assert!(fails(&|o| o.name = DEFAULT_PARENT, &[]));
        assert!(fails(&|_| {}, &extra(&["timestamp"])), "inherited name");
        assert!(!fails(&|_| {}, &extra(&["modifiedEntityNames"])), "plain default parent has no tracking field");
        assert!(fails(&|o| o.parent = Parent::Tracking, &extra(&["modifiedEntityNames"])));
        assert!(fails(&|o| { o.parent = Parent::None; o.number = "x"; o.timestamp = "x"; }, &[]));
        assert!(fails(&|o| o.listener = ListenerChoice::Existing("shop.Entity".into()), &[]));
        assert!(fails(&|o| o.listener = ListenerChoice::Existing("nopackage".into()), &[]));
    }

    #[test]
    fn listener_discovery_reads_public_envers_listeners_only() {
        let listener = "package shop.listeners;\n\nimport org.hibernate.envers.RevisionListener;\n\npublic class AuditListener implements RevisionListener {\n    public void newRevision(Object o) {}\n}\n";
        assert_eq!(listener_of(listener).as_deref(), Some("shop.listeners.AuditListener"));
        assert_eq!(listener_of(&listener.replace("public class", "class")), None, "package-private");
        assert_eq!(listener_of(&listener.replace("import org.hibernate.envers.RevisionListener;", "import other.RevisionListener;")), None, "foreign type");
        assert_eq!(listener_of(&listener.replace("implements RevisionListener", "")), None);
        assert_eq!(listener_of(&listener.replace("AuditListener", "AuditListener<T>")), None, "generic");
        assert_eq!(
            listener_of("package a;\npublic class L implements org.hibernate.envers.RevisionListener {\n}\n").as_deref(),
            Some("a.L")
        );
    }

    #[test]
    fn source_roots_and_package_follow_the_invoking_file() {
        assert_eq!(own_root("/project/src/main/java/shop/Invoice.java", "shop").as_deref(), Some("/project/src/main/java"));
        assert_eq!(own_root("/project/src/main/java/shop/Invoice.java", "other"), None);
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("src/main/java/shop/Invoice.java", "package shop;\n");
        host.add_file("src/test/java/shop/InvoiceTest.java", "package shop;\n");
        host.add_file("src/main/kotlin/shop/K.kt", "package shop\n");
        host.add_file("src/main/java/shop/audit/AuditListener.java", "package shop.audit;\n\nimport org.hibernate.envers.RevisionListener;\n\npublic class AuditListener implements RevisionListener {\n}\n");
        let project = host::project_of("/project/src/main/java/shop/Invoice.java").unwrap();
        let roots = source_roots(&project, "/project/src/main/java").unwrap();
        assert_eq!(roots, ["/project/src/main/java", "/project/src/test/java"]);
        assert_eq!(existing_listeners(&project).unwrap(), ["shop.audit.AuditListener"]);
    }
}

#[cfg(test)]
mod compile {
    use super::*;

    #[test]
    #[ignore = "requires javac, HIBERNATE_ENVERS_JAR, HIBERNATE_CORE_JAR and JAKARTA_PERSISTENCE_JAR"]
    fn wizard_variants_compile_against_envers() {
        let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name}"));
        let classpath = [var("HIBERNATE_ENVERS_JAR"), var("HIBERNATE_CORE_JAR"), var("JAKARTA_PERSISTENCE_JAR")].join(":");
        let extra = ["username".to_owned()];
        let variants = [
            (Parent::None, ListenerChoice::New, "RevNone"),
            (Parent::Default, ListenerChoice::New, "RevDefault"),
            (Parent::Tracking, ListenerChoice::Existing("shop.listeners.AuditListener".into()), "RevTracking"),
        ];
        for (index, (parent, listener, name)) in variants.into_iter().enumerate() {
            let root = std::env::temp_dir().join(format!("jpa-revwiz-{}-{index}", std::process::id()));
            let audit = root.join("shop/audit");
            std::fs::create_dir_all(&audit).unwrap();
            std::fs::create_dir_all(root.join("shop/listeners")).unwrap();
            let existing = root.join("shop/listeners/AuditListener.java");
            std::fs::write(&existing, "package shop.listeners;\n\nimport org.hibernate.envers.RevisionListener;\n\npublic class AuditListener implements RevisionListener {\n    @Override\n    public void newRevision(Object revisionEntity) {\n    }\n}\n").unwrap();
            let directory = audit.to_str().unwrap().to_owned();
            let options = Options { package: "shop.audit", ns: "jakarta.persistence", directory: &directory, name, table: "revinfo", parent, number: "id", timestamp: "timestamp", extra: &extra, listener, crlf: false };
            let mut paths = vec![existing.to_str().unwrap().to_owned()];
            for (path, text) in render(&options).unwrap() {
                std::fs::write(&path, text).unwrap();
                paths.push(path);
            }
            let result = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &classpath, "-d"]).arg(&root).args(&paths).output().expect("javac installed");
            assert!(result.status.success(), "{name}: {}", String::from_utf8_lossy(&result.stderr));
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
