//! Spring Data JPA auditing, two reviewed actions.
//!
//! Facts come from the Spring Data auditing reference: `@CreatedDate`/`@LastModifiedDate` apply to
//! JDK 8 date and time types, `@CreatedBy`/`@LastModifiedBy` to the auditor type of an
//! `AuditorAware<T>` bean (`Optional<T> getCurrentAuditor()`), an entity opts in with
//! `@EntityListeners(AuditingEntityListener.class)`, and `@EnableJpaAuditing` takes
//! `auditorAwareRef` and `dateTimeProviderRef`. The reference shows no package names; the
//! packages below are checked by the opt-in compile test against the Spring Data jars.
//! The reference also asks for an `orm.xml` default listener when it is used instead of
//! `@EntityListeners`; the per-entity annotation used here needs no XML.
use super::*;
use crate::model::import_edit;

pub(super) const FIELDS_TITLE: &str = "Add auditing fields…";
pub(super) const CONFIG_TITLE: &str = "Create JPA auditing configuration…";
const ANNOTATION_PACKAGE: &str = "org.springframework.data.annotation";
const LISTENER: &str = "org.springframework.data.jpa.domain.support.AuditingEntityListener";
const ENABLE: &str = "org.springframework.data.jpa.repository.config.EnableJpaAuditing";
const AUDITOR_AWARE: &str = "org.springframework.data.domain.AuditorAware";
const FIELDS: [(&str, &str, bool); 4] = [
    ("createdDate", "CreatedDate", true),
    ("lastModifiedDate", "LastModifiedDate", true),
    ("createdBy", "CreatedBy", false),
    ("lastModifiedBy", "LastModifiedBy", false),
];
const DATE_TYPES: [(&str, Option<&str>); 3] = [
    ("Instant", Some("java.time.Instant")),
    ("LocalDateTime", Some("java.time.LocalDateTime")),
    ("OffsetDateTime", Some("java.time.OffsetDateTime")),
];
const AUDITOR_TYPES: [(&str, Option<&str>); 3] = [("String", None), ("Long", None), ("UUID", Some("java.util.UUID"))];
const SCAN_LIMIT: u32 = 2001;

struct Source {
    class: Class,
    masked: String,
    ns: &'static str,
    entity_start: usize,
    body: (usize, usize),
    listeners: Option<((usize, usize), Vec<String>)>,
}

fn simple(fq: &str) -> &str {
    fq.rsplit('.').next().unwrap_or(fq)
}

fn clashes(class: &Class, fq: &str) -> bool {
    class.imports.iter().any(|import| simple(import) == simple(fq) && import != fq)
}

fn listener_items(inner: &str, kotlin: bool) -> Option<Vec<String>> {
    let inner = inner.trim();
    let (open, close, suffix) = if kotlin { ('[', ']', "::class") } else { ('{', '}', ".class") };
    let inner = inner.strip_prefix(open).and_then(|s| s.strip_suffix(close)).unwrap_or(inner);
    let mut items = Vec::new();
    for item in inner.split(',') {
        let path = item.trim().strip_suffix(suffix)?;
        if path.is_empty() || !path.split('.').all(identifier) {
            return None;
        }
        items.push(path.to_owned());
    }
    Some(items)
}

fn source(doc: &Document) -> Result<Source, String> {
    let class = current_class(doc).filter(|c| c.kind == Kind::Entity).ok_or("Select a plain entity with no superclass.")?;
    let masked = mask(&doc.text);
    let ns = namespace(&doc.text, &class).ok_or("The entity's persistence namespace is not unambiguous.")?;
    let from = offset(&doc.text, class.pos).ok_or("Stale source.")?;
    let at = masked[..from].trim_end().strip_suffix("class").map(str::len).ok_or("Only a single source-owned entity class is supported.")?;
    let end = class.body_end.ok_or("The entity has no class body to add fields to.")?;
    let tops = super::index_query_action::top(&masked, at);
    let entity = tops.iter().find(|a| a.name == "Entity").ok_or("The @Entity annotation was not found.")?;
    let found: Vec<_> = tops.iter().filter(|a| a.name == "EntityListeners").collect();
    let listeners = match found.as_slice() {
        [] => None,
        [one] => {
            if !annotation_is(&doc.text, &class, "EntityListeners", ns) {
                return Err("The @EntityListeners annotation is not provably from the entity's persistence namespace.".into());
            }
            let args = one.args.ok_or("The @EntityListeners list is not a literal; edit it manually.")?;
            let items = listener_items(&masked[args.0..args.1], class.kotlin)
                .ok_or("The @EntityListeners list is not a literal list of class references; edit it manually.")?;
            Some((args, items))
        }
        _ => return Err("Several @EntityListeners annotations are present; edit them manually.".into()),
    };
    Ok(Source { entity_start: entity.start, class, masked, ns, body: (from, end), listeners })
}

pub(super) fn eligible_fields(doc: &Document) -> bool {
    source(doc).is_ok()
}

pub(super) fn eligible_config(doc: &Document) -> bool {
    current_class(doc).is_some_and(|class| class.kind == Kind::Entity)
}

fn spring_data_jpa(project: &Project, root: &str, directory: &str) -> Result<(), Error> {
    let text = super::mapper_action::build_text(project, root, directory)?;
    crate::project_facts::gate(crate::project_facts::Feature::SpringDataJpa, &text).into_result()
}

pub(super) fn render_fields(doc: &Document, chosen: &[String], date_type: &str, auditor_type: &str) -> Result<Vec<TextEdit>, Error> {
    let src = source(doc).map_err(Error::from)?;
    let class = &src.class;
    let mut picked = Vec::new();
    for name in chosen {
        let field = FIELDS.iter().find(|f| f.0 == name).filter(|f| !picked.iter().any(|p: &&(&str, &str, bool)| p.0 == f.0));
        picked.push(field.ok_or("Choose offered auditing fields exactly once.")?);
    }
    if picked.is_empty() {
        return Err("Choose at least one auditing field.".into());
    }
    let lookup = |table: &[(&'static str, Option<&'static str>)], wanted: &str| table.iter().copied().find(|(name, _)| *name == wanted);
    let date = lookup(&DATE_TYPES, date_type);
    let auditor = lookup(&AUDITOR_TYPES, auditor_type);
    if picked.iter().any(|f| f.2) && date.is_none() {
        return Err("Choose a date type from Instant, LocalDateTime or OffsetDateTime.".into());
    }
    if picked.iter().any(|f| !f.2) && auditor.is_none() {
        return Err("Choose an auditor type from String, Long or UUID.".into());
    }

    let body = &src.masked[src.body.0..src.body.1];
    let annotated: Vec<_> = annotations(&src.masked).into_iter().filter(|a| a.start > src.body.0 && a.start < src.body.1).map(|a| a.name).collect();
    for (name, annotation, _) in &picked {
        if class.attributes.iter().any(|a| a.name == *name) || has_word(body, name) {
            return Err(format!("{} already uses the name {name}; choose other fields or edit it manually.", class.name).into());
        }
        if annotated.contains(annotation) {
            return Err(format!("{} already has a member annotated @{annotation}; it is not duplicated.", class.name).into());
        }
    }

    let mut imports: Vec<String> = Vec::new();
    for (_, annotation, is_date) in &picked {
        imports.push(format!("{ANNOTATION_PACKAGE}.{annotation}"));
        imports.extend((if *is_date { date } else { auditor }).and_then(|t| t.1).map(str::to_owned));
    }
    let listener_present = match &src.listeners {
        Some((_, items)) => {
            let imported = class.imports.iter().any(|i| i == LISTENER) || class.imports.iter().any(|i| i == "org.springframework.data.jpa.domain.support.*");
            let present = items.iter().any(|i| i == LISTENER || (i == "AuditingEntityListener" && imported));
            if !present && items.iter().any(|i| simple(i) == "AuditingEntityListener") {
                return Err("An AuditingEntityListener that is not Spring Data's is already listed; edit it manually.".into());
            }
            present
        }
        None => false,
    };
    if !listener_present {
        imports.push(LISTENER.into());
    }
    if src.listeners.is_none() {
        imports.push(format!("{}.EntityListeners", src.ns));
    }
    if let Some(clash) = imports.iter().find(|i| clashes(class, i)) {
        return Err(format!("An import with the simple name of `{clash}` already exists; edit it manually.").into());
    }

    let kotlin = class.kotlin;
    let reference = if kotlin { "AuditingEntityListener::class" } else { "AuditingEntityListener.class" };
    let mut edits: Vec<Edit> = Vec::new();
    let refs: Vec<&str> = imports.iter().map(String::as_str).collect();
    edits.extend(import_edit(&doc.text, &refs, kotlin));
    match &src.listeners {
        None => {
            let line_start = doc.text[..src.entity_start].rfind('\n').map_or(0, |i| i + 1);
            let prefix = &doc.text[line_start..src.entity_start];
            edits.push(if prefix.trim().is_empty() {
                Edit::insert(line_start, &format!("{prefix}@EntityListeners({reference})\n"))
            } else {
                Edit::insert(src.entity_start, &format!("@EntityListeners({reference}) "))
            });
        }
        Some((args, items)) if !listener_present => {
            let suffix = if kotlin { "::class" } else { ".class" };
            let mut all: Vec<String> = items.iter().map(|i| format!("{i}{suffix}")).collect();
            all.push(reference.to_owned());
            let list = all.join(", ");
            let wrapped = if kotlin { format!("[{list}]") } else { format!("{{{list}}}") };
            edits.push(Edit { start: args.0, end: args.1, text: wrapped });
        }
        Some(_) => {}
    }

    let accessors = class.attributes.iter().any(|a| has_word(&src.masked, &format!("get{}", capitalize(&a.name))));
    let fields: Vec<String> = picked
        .iter()
        .map(|(name, annotation, is_date)| {
            let ty = if *is_date { date } else { auditor }.map_or("", |t| t.0);
            crate::wizard::plain_field(vec![format!("@{annotation}")], ty, name, kotlin, accessors)
        })
        .collect();
    edits.push(Edit::insert(src.body.1, &format!("\n{}\n", fields.join("\n\n"))));

    if doc.text.contains("\r\n") {
        edits.iter_mut().for_each(|e| e.text = e.text.replace('\n', "\r\n"));
    }
    let edits: Vec<TextEdit> = edits.iter().map(|e| e.to_text_edit(&doc.text)).collect();
    checked_after(doc, &edits)?;
    Ok(edits)
}

pub(super) fn resolve_fields(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    source(doc).map_err(Error::from)?;
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    spring_data_jpa(project, &pending.root, directory)?;
    let items: Vec<_> = FIELDS.iter().map(|(name, annotation, _)| PickItem::new(name, name).description(&format!("@{annotation}"))).collect();
    let chosen = host::pick_many("Auditing fields to add", &items)?.ok_or(Error::Cancelled)?;
    let mut date = String::new();
    if FIELDS.iter().any(|f| f.2 && chosen.iter().any(|c| c == f.0)) {
        let items: Vec<_> = DATE_TYPES.iter().map(|(name, _)| PickItem::new(name, name)).collect();
        date = host::pick("Date type for the audit timestamps", &items)?.ok_or(Error::Cancelled)?;
    }
    let mut auditor = String::new();
    if FIELDS.iter().any(|f| !f.2 && chosen.iter().any(|c| c == f.0)) {
        let items: Vec<_> = AUDITOR_TYPES.iter().map(|(name, _)| PickItem::new(name, name)).collect();
        auditor = host::pick("Auditor type (the T of your AuditorAware<T>)", &items)?.ok_or(Error::Cancelled)?;
    }
    let edits = render_fields(doc, &chosen, &date, &auditor)?;
    let after = patched(&doc.text, &edits)?;
    if !ui_kit::prompts::confirm_exact(
        "Add auditing fields?",
        &format!(
            "Target: {}\nSource version: {}\n\n{after}\nAuditing only runs once @EnableJpaAuditing is declared (see \"Create JPA auditing configuration\"). Fields named createdBy/lastModifiedBy need an AuditorAware<T> bean of the chosen type. Only this source is edited.",
            doc.path, doc.version
        ),
    )? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    Ok(FileAction {
        title: "Add auditing fields".into(),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), edits)],
        ..FileAction::default()
    })
}

fn auditing_declared(project: &Project) -> Result<(), Error> {
    let deadline = Instant::now() + Duration::from_secs(3);
    for (pattern, needle) in [("**/*.java", "EnableJpaAuditing"), ("**/*.kt", "EnableJpaAuditing"), ("**/*.xml", "jpa:auditing")] {
        let files = project.find_files(pattern, SCAN_LIMIT)?;
        if files.len() >= SCAN_LIMIT as usize {
            return Err("The project has too many sources to prove that JPA auditing is not already enabled.".into());
        }
        for file in files {
            if Instant::now() >= deadline {
                return Err("Scanning for an existing auditing declaration timed out.".into());
            }
            let text = project.read_file(&file)?;
            if text.len() > MAX_TEXT {
                return Err(format!("{file} is too large to verify that it does not enable auditing.").into());
            }
            let found = if needle == "jpa:auditing" { text.contains(needle) } else { has_word(&mask(&text), needle) };
            if found {
                return Err(format!("JPA auditing is already enabled in {file}; no second declaration is generated.").into());
            }
        }
    }
    Ok(())
}

/// Target path and text of the configuration class; `auditor` is the T of the placeholder bean.
pub(super) fn render_config(doc: &Document, name: &str, auditor: Option<&str>) -> Result<(String, String), Error> {
    let kotlin = doc.language == "kotlin";
    let package_name = package(doc).ok_or(Error::Stale)?;
    let auditor = match auditor {
        Some(wanted) => Some(AUDITOR_TYPES.iter().find(|t| t.0 == wanted).ok_or("Choose an auditor type from String, Long or UUID.")?),
        None => None,
    };
    let mut imports = vec!["org.springframework.context.annotation.Configuration".to_owned(), ENABLE.to_owned()];
    if let Some((_, import)) = auditor {
        imports.extend(["org.springframework.context.annotation.Bean", AUDITOR_AWARE, "java.util.Optional"].map(str::to_owned));
        imports.extend(import.map(str::to_owned));
    }
    imports.sort();
    let names: Vec<_> = imports.iter().map(|i| simple(i)).collect();
    if !identifier(name) || names.contains(&name) {
        return Err("Choose a valid class name that does not clash with the imported types.".into());
    }
    let semicolon = if kotlin { "" } else { ";" };
    let imports: String = imports.iter().map(|i| format!("import {i}{semicolon}\n")).collect();
    let enable = if auditor.is_some() { "@EnableJpaAuditing(auditorAwareRef = \"auditorProvider\")" } else { "@EnableJpaAuditing" };
    let mut text = match (kotlin, auditor) {
        (false, None) => format!("package {package_name};\n\n{imports}\n@Configuration\n{enable}\npublic class {name} {{\n}}\n"),
        (false, Some((ty, _))) => format!(
            "package {package_name};\n\n{imports}\n@Configuration\n{enable}\npublic class {name} {{\n\n    @Bean\n    public AuditorAware<{ty}> auditorProvider() {{\n        return Optional::empty;\n    }}\n}}\n"
        ),
        (true, None) => format!("package {package_name}\n\n{imports}\n@Configuration\n{enable}\nopen class {name}\n"),
        (true, Some((ty, _))) => format!(
            "package {package_name}\n\n{imports}\n@Configuration\n{enable}\nopen class {name} {{\n\n    @Bean\n    open fun auditorProvider(): AuditorAware<{ty}> = AuditorAware<{ty}> {{ Optional.empty() }}\n}}\n"
        ),
    };
    if doc.text.contains("\r\n") {
        text = text.replace('\n', "\r\n");
    }
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    Ok((format!("{directory}/{name}.{}", if kotlin { "kt" } else { "java" }), text))
}

pub(super) fn resolve_config(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    spring_data_jpa(project, &pending.root, directory)?;
    auditing_declared(project)?;
    let choices = [
        PickItem::new("none", "No AuditorAware bean (declare your own)"),
        PickItem::new("String", "Placeholder AuditorAware<String> bean"),
        PickItem::new("Long", "Placeholder AuditorAware<Long> bean"),
        PickItem::new("UUID", "Placeholder AuditorAware<UUID> bean"),
    ];
    let auditor = host::pick("AuditorAware bean", &choices)?.ok_or(Error::Cancelled)?;
    let name = host::input("Configuration class name (same package)", Some("JpaAuditingConfiguration"))?.ok_or(Error::Cancelled)?;
    let name = name.trim().to_owned();
    current(pending, doc)?;
    let (target, text) = render_config(doc, &name, (auditor != "none").then_some(auditor.as_str()))?;
    if text.len() > MAX_TEXT || path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
        return Err(Error::Stale);
    }
    no_collision(project, &pending.root, directory, &name, None)?;
    let note = if auditor == "none" {
        "No AuditorAware bean is generated: createdBy/lastModifiedBy stay empty until you expose one."
    } else {
        "The generated AuditorAware bean returns Optional.empty() and is a placeholder: replace it with real logic (for example from the security context) before relying on createdBy/lastModifiedBy."
    };
    if !ui_kit::prompts::confirm_exact(
        "Create JPA auditing configuration?",
        &format!("Source: {}\nSource version: {}\nNew file: {target}\n\n{text}\n{note} No existing file is overwritten.", doc.path, doc.version),
    )? {
        return Err(Error::Cancelled);
    }
    let project = current(pending, doc)?;
    no_collision(&project, &pending.root, directory, &name, None)?;
    auditing_declared(&project)?;
    Ok(FileAction {
        title: format!("Create JPA auditing configuration {name}"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target.clone(), text, overwrite: false }],
        open: Some(target),
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const JAVA_PATH: &str = "/project/src/main/java/shop/Invoice.java";
    const KOTLIN_PATH: &str = "/project/src/main/kotlin/shop/Invoice.kt";
    const JAVA: &str = "package shop;\n\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n\n@Entity\npublic class Invoice {\n    @Id\n    private Long id;\n\n    public Long getId() {\n        return id;\n    }\n}\n";
    const KOTLIN: &str = "package shop\n\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\n\n@Entity\nopen class Invoice {\n    @Id\n    var id: Long? = null\n}\n";

    fn doc(path: &str, text: &str) -> Document {
        Document { path: path.into(), language: if path.ends_with(".kt") { "kotlin" } else { "java" }.into(), text: text.into(), version: 1 }
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn add(path: &str, text: &str, fields: &[&str], date: &str, auditor: &str) -> Result<String, Error> {
        let d = doc(path, text);
        let edits = render_fields(&d, &names(fields), date, auditor)?;
        patched(text, &edits)
    }

    fn refused(path: &str, text: &str, fields: &[&str], needle: &str) {
        let message = format!("{:?}", add(path, text, fields, "Instant", "String").err().expect("refusal"));
        assert!(message.contains(needle), "{needle}: {message}");
    }

    const ALL: [&str; 4] = ["createdDate", "lastModifiedDate", "createdBy", "lastModifiedBy"];

    #[test]
    fn java_adds_annotated_fields_accessors_listener_and_imports_in_both_namespaces() {
        for ns in ["jakarta.persistence", "javax.persistence"] {
            let text = JAVA.replace("jakarta.persistence", ns);
            let out = add(JAVA_PATH, &text, &ALL, "LocalDateTime", "UUID").unwrap();
            for expected in [
                &format!("import {ns}.EntityListeners;"),
                "import java.util.UUID;",
                "import java.time.LocalDateTime;",
                "import org.springframework.data.annotation.CreatedBy;",
                "import org.springframework.data.annotation.CreatedDate;",
                "import org.springframework.data.annotation.LastModifiedBy;",
                "import org.springframework.data.annotation.LastModifiedDate;",
                "import org.springframework.data.jpa.domain.support.AuditingEntityListener;",
                "@EntityListeners(AuditingEntityListener.class)\n@Entity\npublic class Invoice {",
                "    @CreatedDate\n    private LocalDateTime createdDate;",
                "    @LastModifiedBy\n    private UUID lastModifiedBy;",
                "    public LocalDateTime getCreatedDate() {\n        return createdDate;\n    }",
                "    public void setLastModifiedBy(UUID lastModifiedBy) {\n        this.lastModifiedBy = lastModifiedBy;\n    }",
            ] {
                assert!(out.contains(expected), "{expected}\n{out}");
            }
        }
    }

    #[test]
    fn kotlin_adds_nullable_vars_and_class_reference() {
        let out = add(KOTLIN_PATH, KOTLIN, &["createdDate", "createdBy"], "OffsetDateTime", "Long").unwrap();
        for expected in [
            "import jakarta.persistence.EntityListeners\n",
            "import org.springframework.data.annotation.CreatedDate\n",
            "import java.time.OffsetDateTime\n",
            "@EntityListeners(AuditingEntityListener::class)\n@Entity\nopen class Invoice {",
            "    @CreatedDate\n    var createdDate: OffsetDateTime? = null",
            "    @CreatedBy\n    var createdBy: Long? = null",
        ] {
            assert!(out.contains(expected), "{expected}\n{out}");
        }
        assert!(!out.contains(';') && !out.contains("getCreatedDate"), "{out}");
    }

    #[test]
    fn java_without_existing_accessors_gets_none() {
        let text = JAVA.replace("    public Long getId() {\n        return id;\n    }\n", "");
        let out = add(JAVA_PATH, &text, &["createdBy"], "", "String").unwrap();
        assert!(out.contains("private String createdBy;") && !out.contains("getCreatedBy"), "{out}");
    }

    #[test]
    fn existing_literal_listener_lists_are_merged() {
        let single = JAVA.replace("@Entity\n", "@EntityListeners(Audit.class)\n@Entity\n").replace("jakarta.persistence.Id;", "jakarta.persistence.Id;\nimport jakarta.persistence.EntityListeners;");
        let out = add(JAVA_PATH, &single, &["createdDate"], "Instant", "").unwrap();
        assert!(out.contains("@EntityListeners({Audit.class, AuditingEntityListener.class})"), "{out}");
        assert_eq!(out.matches("import jakarta.persistence.EntityListeners;").count(), 1, "{out}");
        let braces = single.replace("(Audit.class)", "({Audit.class, Other.class})");
        let out = add(JAVA_PATH, &braces, &["createdDate"], "Instant", "").unwrap();
        assert!(out.contains("{Audit.class, Other.class, AuditingEntityListener.class}"), "{out}");
        let kotlin = KOTLIN.replace("@Entity\n", "@EntityListeners(Audit::class)\n@Entity\n").replace("jakarta.persistence.Id", "jakarta.persistence.Id\nimport jakarta.persistence.EntityListeners");
        assert!(add(KOTLIN_PATH, &kotlin, &["createdDate"], "Instant", "").is_err(), "Kotlin ::class lists are refused by the shared single-class gate");
    }

    #[test]
    fn an_already_listed_spring_listener_is_not_duplicated_and_a_foreign_one_is_refused() {
        let listed = JAVA
            .replace("@Entity\n", "@EntityListeners(AuditingEntityListener.class)\n@Entity\n")
            .replace("jakarta.persistence.Id;", "jakarta.persistence.Id;\nimport jakarta.persistence.EntityListeners;\nimport org.springframework.data.jpa.domain.support.AuditingEntityListener;");
        let out = add(JAVA_PATH, &listed, &["createdDate"], "Instant", "").unwrap();
        assert_eq!(out.matches("AuditingEntityListener").count(), 2, "{out}");
        let foreign = listed.replace("import org.springframework.data.jpa.domain.support.AuditingEntityListener;", "");
        refused(JAVA_PATH, &foreign, &["createdDate"], "not Spring Data's");
    }

    #[test]
    fn non_literal_or_ambiguous_listener_declarations_are_refused() {
        for annotation in ["@EntityListeners(value = Audit.class)", "@EntityListeners(LISTENERS)", "@EntityListeners({Audit.class,})", "@EntityListeners", "@EntityListeners(Audit.class)\n@EntityListeners(Other.class)"] {
            let text = JAVA.replace("@Entity\n", &format!("{annotation}\n@Entity\n")).replace("jakarta.persistence.Id;", "jakarta.persistence.Id;\nimport jakarta.persistence.EntityListeners;");
            let message = format!("{:?}", add(JAVA_PATH, &text, &["createdDate"], "Instant", "").err().expect(annotation));
            assert!(message.contains("literal") || message.contains("Several"), "{annotation}: {message}");
        }
    }

    #[test]
    fn collisions_duplicates_and_bad_choices_are_refused() {
        refused(JAVA_PATH, &JAVA.replace("private Long id;", "private Long id;\n    private String createdBy;"), &["createdBy"], "already uses the name");
        refused(JAVA_PATH, &JAVA.replace("private Long id;", "private Long id;\n    @CreatedDate\n    private Instant created;"), &["createdDate"], "already has a member annotated @CreatedDate");
        refused(JAVA_PATH, &JAVA.replace("return id;", "return createdDate == null ? id : id;"), &["createdDate"], "already uses the name");
        refused(JAVA_PATH, &JAVA.replace("import jakarta.persistence.Id;", "import jakarta.persistence.Id;\nimport other.CreatedDate;"), &["createdDate"], "simple name");
        refused(JAVA_PATH, JAVA, &[], "at least one");
        refused(JAVA_PATH, JAVA, &["createdDate", "createdDate"], "exactly once");
        refused(JAVA_PATH, JAVA, &["version"], "exactly once");
        let message = format!("{:?}", add(JAVA_PATH, JAVA, &["createdDate"], "Date", "").err().unwrap());
        assert!(message.contains("date type"), "{message}");
        let message = format!("{:?}", add(JAVA_PATH, JAVA, &["createdBy"], "", "BigDecimal").err().unwrap());
        assert!(message.contains("auditor type"), "{message}");
    }

    #[test]
    fn entities_with_a_superclass_or_without_a_body_are_not_offered() {
        assert!(eligible_fields(&doc(JAVA_PATH, JAVA)) && eligible_config(&doc(JAVA_PATH, JAVA)));
        assert!(!eligible_fields(&doc(JAVA_PATH, &JAVA.replace("class Invoice {", "class Invoice extends Base {"))));
        let bodyless = "package shop\n\nimport jakarta.persistence.Entity\n\n@Entity\nopen class Invoice(val id: Long)\n";
        assert!(!eligible_fields(&doc(KOTLIN_PATH, bodyless)));
    }

    #[test]
    fn same_line_entity_annotation_and_crlf_are_handled() {
        let inline = JAVA.replace("@Entity\n", "@Deprecated @Entity\n");
        let out = add(JAVA_PATH, &inline, &["createdDate"], "Instant", "").unwrap();
        assert!(out.contains("@Deprecated @EntityListeners(AuditingEntityListener.class) @Entity\npublic class Invoice {"), "{out}");
        let crlf = JAVA.replace('\n', "\r\n");
        let out = add(JAVA_PATH, &crlf, &["createdDate", "createdBy"], "Instant", "String").unwrap();
        assert!(out.contains("\r\n") && !out.replace("\r\n", "").contains('\n'), "{out:?}");
    }

    fn config(path: &str, text: &str, name: &str, auditor: Option<&str>) -> Result<(String, String), Error> {
        render_config(&doc(path, text), name, auditor)
    }

    #[test]
    fn configuration_renders_in_java_and_kotlin_with_or_without_the_placeholder_bean() {
        let (path, text) = config(JAVA_PATH, JAVA, "JpaAuditingConfiguration", None).unwrap();
        assert_eq!(path, "/project/src/main/java/shop/JpaAuditingConfiguration.java");
        assert_eq!(
            text,
            "package shop;\n\nimport org.springframework.context.annotation.Configuration;\nimport org.springframework.data.jpa.repository.config.EnableJpaAuditing;\n\n@Configuration\n@EnableJpaAuditing\npublic class JpaAuditingConfiguration {\n}\n"
        );
        let (_, text) = config(JAVA_PATH, JAVA, "Audit", Some("UUID")).unwrap();
        for expected in [
            "import java.util.Optional;",
            "import java.util.UUID;",
            "import org.springframework.data.domain.AuditorAware;",
            "@EnableJpaAuditing(auditorAwareRef = \"auditorProvider\")",
            "public AuditorAware<UUID> auditorProvider() {\n        return Optional::empty;",
        ] {
            assert!(text.contains(expected), "{expected}\n{text}");
        }
        let (path, text) = config(KOTLIN_PATH, KOTLIN, "Audit", Some("String")).unwrap();
        assert!(path.ends_with("/Audit.kt") && !text.contains(';'), "{text}");
        assert!(text.contains("open fun auditorProvider(): AuditorAware<String> = AuditorAware<String> { Optional.empty() }"), "{text}");
        let (_, text) = config(KOTLIN_PATH, KOTLIN, "Audit", None).unwrap();
        assert!(text.ends_with("@EnableJpaAuditing\nopen class Audit\n"), "{text}");
        let (_, text) = config(JAVA_PATH, &JAVA.replace('\n', "\r\n"), "Audit", None).unwrap();
        assert!(!text.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn configuration_refuses_bad_names_and_types() {
        for name in ["", "1Bad", "Configuration", "EnableJpaAuditing", "bad name", "class"] {
            assert!(config(JAVA_PATH, JAVA, name, None).is_err(), "{name}");
        }
        assert!(config(JAVA_PATH, JAVA, "Audit", Some("Date")).is_err());
        assert!(config(JAVA_PATH, JAVA, "Optional", Some("String")).is_err());
    }

    fn flow(files: &[(&str, &str)], answers: &[&str], many: Option<&[&str]>, title: &str) -> (MockHost, Result<FileAction, Error>) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(JAVA_PATH, "java", JAVA, &[]);
        for (name, body) in files {
            host.add_file(name, body);
        }
        let d = doc(JAVA_PATH, JAVA);
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&d, Range::default()).into_iter().find(|a| a.title == title).expect("menu entry");
        assert!(pending.operations.is_empty() && pending.edits.is_empty());
        if let Some(many) = many {
            host.answer_next_prompt(Some(many));
        }
        for answer in answers {
            host.answer_next_prompt(Some(&[*answer]));
        }
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&d, pending);
        (host, result)
    }

    const POM: (&str, &str) = ("pom.xml", "<project><dependency><artifactId>spring-boot-starter-data-jpa</artifactId></dependency></project>");

    #[test]
    fn fields_flow_returns_one_reviewed_edit_and_needs_spring_data_jpa() {
        let (_, result) = flow(&[POM], &["Instant", "String"], Some(&["createdDate", "createdBy"]), FIELDS_TITLE);
        let action = result.unwrap();
        assert!(action.operations.is_empty());
        let [(path, edits)] = action.edits.as_slice() else { panic!("one document") };
        assert_eq!(path, JAVA_PATH);
        assert!(patched(JAVA, edits).unwrap().contains("@CreatedBy"));
        let (_, result) = flow(&[], &["Instant", "String"], Some(&["createdDate"]), FIELDS_TITLE);
        assert!(format!("{:?}", result.err().unwrap()).contains("Spring Data JPA"));
        let (_, result) = flow(&[("pom.xml", "<project/>")], &[], Some(&["createdDate"]), FIELDS_TITLE);
        assert!(format!("{:?}", result.err().unwrap()).contains("not provably"));
    }

    #[test]
    fn configuration_flow_creates_one_no_overwrite_file_and_refuses_existing_auditing() {
        let (_, result) = flow(&[POM], &["String", "JpaAuditingConfiguration"], None, CONFIG_TITLE);
        let action = result.unwrap();
        let [FileOperation::Create { path, text, overwrite }] = action.operations.as_slice() else { panic!("one create") };
        assert_eq!(path, "/project/src/main/java/shop/JpaAuditingConfiguration.java");
        assert!(!overwrite && text.contains("@EnableJpaAuditing") && action.edits.is_empty());
        let existing = "package app;\nimport org.springframework.data.jpa.repository.config.EnableJpaAuditing;\n@EnableJpaAuditing\nclass Other {}\n";
        let (_, result) = flow(&[POM, ("src/main/java/app/Other.java", existing)], &["none", "JpaAuditingConfiguration"], None, CONFIG_TITLE);
        assert!(format!("{:?}", result.err().unwrap()).contains("already enabled"));
        let xml = "<beans><jpa:auditing auditor-aware-ref=\"a\"/></beans>";
        let (_, result) = flow(&[POM, ("src/main/resources/app.xml", xml)], &["none", "JpaAuditingConfiguration"], None, CONFIG_TITLE);
        assert!(format!("{:?}", result.err().unwrap()).contains("already enabled"));
        let commented = "package app;\n// @EnableJpaAuditing\nclass Other {}\n";
        let (_, result) = flow(&[POM, ("src/main/java/app/Other.java", commented)], &["none", "JpaAuditingConfiguration"], None, CONFIG_TITLE);
        assert!(result.is_ok());
        let (_, result) = flow(&[POM, ("src/main/java/shop/JpaAuditingConfiguration.java", "class JpaAuditingConfiguration {}")], &["none", "JpaAuditingConfiguration"], None, CONFIG_TITLE);
        assert!(format!("{:?}", result.err().unwrap()).contains("already exists"));
    }

    #[test]
    #[ignore = "requires javac, kotlinc env and SPRING_DATA_CLASSPATH + SPRING_CONTEXT_CLASSPATH (source scripts/jpa-compile-env.sh)"]
    fn generated_auditing_sources_compile_against_spring_data() {
        let spring = std::env::var("SPRING_DATA_CLASSPATH").expect("explicit classpath");
        let context = std::env::var("SPRING_CONTEXT_CLASSPATH").expect("explicit classpath");
        let classpath = format!("{spring}:{context}");
        let base = std::env::temp_dir().join(format!("jpa-auditing-compile-{}", std::process::id()));
        let java_dir = base.join("java");
        let kotlin_dir = base.join("kotlin");
        std::fs::create_dir_all(&java_dir).unwrap();
        std::fs::create_dir_all(&kotlin_dir).unwrap();
        let entity = add(JAVA_PATH, JAVA, &ALL, "OffsetDateTime", "UUID").unwrap();
        let (_, with_bean) = config(JAVA_PATH, JAVA, "AuditConfig", Some("UUID")).unwrap();
        let (_, without) = config(JAVA_PATH, JAVA, "PlainAuditConfig", None).unwrap();
        let files: Vec<_> = [("Invoice.java", entity), ("AuditConfig.java", with_bean), ("PlainAuditConfig.java", without)]
            .into_iter()
            .map(|(name, body)| {
                let file = java_dir.join(name);
                std::fs::write(&file, body).unwrap();
                file
            })
            .collect();
        let result = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &classpath, "-d"]).arg(&java_dir).args(&files).output().expect("javac installed");
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));

        let compiler = std::env::var("KOTLIN_COMPILER_CLASSPATH").expect("explicit Kotlin compiler classpath");
        let stdlib = std::env::var("KOTLIN_STDLIB").expect("explicit Kotlin standard library jar");
        let entity = add(KOTLIN_PATH, KOTLIN, &ALL, "Instant", "Long").unwrap();
        let (_, with_bean) = config(KOTLIN_PATH, KOTLIN, "AuditConfig", Some("Long")).unwrap();
        let (_, without) = config(KOTLIN_PATH, KOTLIN, "PlainAuditConfig", None).unwrap();
        let files: Vec<_> = [("Invoice.kt", entity), ("AuditConfig.kt", with_bean), ("PlainAuditConfig.kt", without)]
            .into_iter()
            .map(|(name, body)| {
                let file = kotlin_dir.join(name);
                std::fs::write(&file, body).unwrap();
                file
            })
            .collect();
        let result = std::process::Command::new("java")
            .args(["-classpath", &compiler, "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler", "-no-stdlib", "-no-reflect", "-jvm-target", "17", "-classpath", &format!("{classpath}:{stdlib}"), "-d"])
            .arg(&kotlin_dir)
            .args(&files)
            .output()
            .expect("java installed");
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(base).unwrap();
    }
}
