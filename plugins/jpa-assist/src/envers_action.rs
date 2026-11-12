//! Reviewed Hibernate Envers actions: toggle `@Audited`/`@NotAudited` in the invoking entity's document,
//! and create a Java `@RevisionEntity` sibling (plus an optional `RevisionListener`) in the same package.
//!
//! Verified against the Hibernate 5.6 and 6.6 Javadoc and the 6.6 user guide (Envers chapter):
//! the annotations live in `org.hibernate.envers` in both major versions; `@Audited` targets TYPE/METHOD/FIELD
//! ("When applied to a class, indicates that all of its properties should be audited. When applied to a field,
//! indicates that this field should be audited."); `@NotAudited` targets METHOD/FIELD ("When applied to a field,
//! indicates that this field should not be audited."); `@RevisionEntity` targets TYPE and needs a `@RevisionNumber`
//! integer-valued property and a `@RevisionTimestamp` long/Date property, and only one such entity may exist.
//! The user guide's custom revision entity uses `@Id @GeneratedValue @RevisionNumber private int` and
//! `@RevisionTimestamp private long`. `DefaultRevisionEntity` is `javax.persistence` in 5.6 and Jakarta in 6.6,
//! so the persistence namespace is taken from the invoking entity, never guessed.
//! Not documented, hence refused: a field `@Audited` inside an `@Audited` class, and `@NotAudited` outside one.
use super::index_query_action as iq;
use super::*;
use crate::model::{Annotation, Edit};

pub(super) const TOGGLE_TITLE: &str = "Toggle audited (Envers) with preview…";
pub(super) const REVISION_TITLE: &str = "Create Envers revision entity with preview…";
#[path = "revision_wizard.rs"]
mod revision_wizard;
const ENVERS: &str = "org.hibernate.envers";
const MAX_FIELDS: usize = 32;
const MAX_EXTRA: usize = 8;
const MAX_SCANNED: usize = 1000;
const RESERVED_TYPES: [&str; 9] =
    ["Entity", "Id", "Table", "GeneratedValue", "RevisionEntity", "RevisionNumber", "RevisionTimestamp", "RevisionListener", "String"];

pub(super) fn eligible_toggle(doc: &Document) -> bool {
    iq::source(doc).is_some()
}

pub(super) fn eligible_revision(doc: &Document) -> bool {
    doc.language == "java" && iq::source(doc).is_some()
}

fn envers_declared(build: &str) -> bool {
    let name = "hibernate-envers";
    build.match_indices(name).any(|(at, _)| {
        let before = build[..at].chars().next_back();
        let after = build[at + name.len()..].chars().next();
        before.is_some_and(|c| matches!(c, ':' | '>' | '"' | '\'') || c.is_whitespace())
            && after.is_none_or(|c| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '.')))
    })
}

fn proof(project: &Project, root: &str, directory: &str) -> Result<(), Error> {
    let build = super::mapper_action::build_text(project, root, directory)?;
    if build.trim().is_empty() {
        return Err("No pom.xml, build.gradle or build.gradle.kts could be read above this source, so Envers cannot be proven; nothing was changed.".into());
    }
    if !envers_declared(&build) {
        return Err("Envers is not provably on this project: hibernate-envers must be written literally in a pom.xml/build.gradle(.kts) above this source (version catalogs and indirect declarations are not verified); nothing was changed.".into());
    }
    Ok(())
}

fn owned(written: &str, name: &str, imports: &[String]) -> bool {
    let full = format!("{ENVERS}.{name}");
    if written.contains(['.', ':']) {
        return written == full;
    }
    let explicit: Vec<_> = imports.iter().filter(|i| i.rsplit('.').next() == Some(name)).collect();
    if !explicit.is_empty() {
        return explicit.len() == 1 && *explicit[0] == full;
    }
    imports.iter().any(|i| *i == format!("{ENVERS}.*"))
}

struct Mark {
    start: usize,
    end: usize,
    plain: bool,
}

struct Field {
    name: String,
    at: usize,
    audited: Option<Mark>,
    not_audited: Option<Mark>,
}

struct State {
    class_audited: Option<Mark>,
    fields: Vec<Field>,
}

fn is_envers(name: &str) -> bool {
    matches!(name, "Audited" | "NotAudited")
}

fn state(doc: &Document, s: &iq::Source) -> Result<State, Error> {
    let all = annotations(&s.masked);
    for a in all.iter().filter(|a| is_envers(a.name)) {
        if !owned(&s.masked[a.start + 1..a.name_end], a.name, &s.class.imports) {
            return Err(format!("@{} in this file is not {ENVERS}.{}; nothing was changed.", a.name, a.name).into());
        }
    }
    let tops = iq::top(&s.masked, s.class_at);
    if tops.iter().filter(|a| a.name == "Audited").count() > 1 || tops.iter().any(|a| a.name == "NotAudited") {
        return Err("The class has a repeated @Audited or a class-level @NotAudited; edit manually.".into());
    }
    let mark = |a: &Annotation, base: usize| Mark { start: base + a.start, end: base + a.end(), plain: a.args.is_none() };
    let class_audited = tops.iter().find(|a| a.name == "Audited").map(|a| mark(a, 0));
    let mut fields = Vec::new();
    if s.class.kotlin {
        if all.iter().filter(|a| is_envers(a.name)).count() != usize::from(class_audited.is_some()) {
            return Err("Kotlin supports the class-level @Audited only; field-level Envers annotations are present, edit manually.".into());
        }
        return Ok(State { class_audited, fields });
    }
    let name_end = offset(&doc.text, s.class.pos).ok_or(Error::Stale)? + s.class.name.len();
    let open = name_end + s.masked[name_end..].find('{').ok_or(Error::Stale)?;
    let end = s.class.body_end.ok_or(Error::Stale)?;
    for (start, finish) in crate::model::members(&s.masked, open + 1, end, false, false) {
        let text = &s.masked[start..finish];
        let found = annotations(text);
        let envers: Vec<_> = found.iter().filter(|a| is_envers(a.name)).collect();
        let clean = crate::model::without_annotations(&s.masked, start, finish);
        let name = text
            .ends_with(';')
            .then(|| crate::model::declaration(&clean, false))
            .flatten()
            .map(|(_, name, _)| name)
            .filter(|name| s.class.attributes.iter().any(|a| a.name == *name));
        let Some(name) = name else {
            if envers.is_empty() {
                continue;
            }
            return Err("@Audited/@NotAudited on a method, nested type or non-attribute member is not supported; edit manually.".into());
        };
        let single = |wanted: &str| -> Result<Option<Mark>, Error> {
            let mut matching = envers.iter().filter(|a| a.name == wanted);
            let first = matching.next().map(|a| mark(a, start));
            match matching.next() {
                Some(_) => Err(format!("Field {name} repeats @{wanted}; edit manually.").into()),
                None => Ok(first),
            }
        };
        fields.push(Field {
            at: start + (text.len() - text.trim_start().len()),
            audited: single("Audited")?,
            not_audited: single("NotAudited")?,
            name,
        });
    }
    Ok(State { class_audited, fields })
}

fn removal(text: &str, mark: &Mark) -> Edit {
    let line_start = text[..mark.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[mark.end..].find('\n').map_or(text.len(), |i| mark.end + i + 1);
    if text[line_start..mark.start].trim().is_empty() && text[mark.end..line_end].trim().is_empty() {
        return Edit { start: line_start, end: line_end, text: String::new() };
    }
    let spaces = text[mark.end..].len() - text[mark.end..].trim_start_matches([' ', '\t']).len();
    Edit { start: mark.start, end: mark.end + spaces, text: String::new() }
}

fn toggle_items(s: &iq::Source, state: &State) -> Vec<PickItem> {
    let mut items = Vec::new();
    match &state.class_audited {
        Some(mark) if mark.plain => items.push(PickItem::new("class", &format!("Class {}: remove @Audited", s.class.name))),
        Some(_) => {}
        None => items.push(PickItem::new("class", &format!("Class {}: add @Audited", s.class.name))),
    }
    for field in &state.fields {
        for (kind, label, present) in [("audited", "Audited", &field.audited), ("notaudited", "NotAudited", &field.not_audited)] {
            let verb = if present.is_some() { "remove" } else { "add" };
            items.push(PickItem::new(&format!("{kind}:{}", field.name), &format!("Field {}: {verb} @{label}", field.name)));
        }
    }
    items
}

pub(super) fn plan(doc: &Document, picks: &[String]) -> Result<Vec<TextEdit>, Error> {
    let s = iq::source(doc).ok_or(Error::Stale)?;
    let state = state(doc, &s)?;
    if state.fields.len() > MAX_FIELDS {
        return Err(format!("More than {MAX_FIELDS} fields; edit manually.").into());
    }
    let offered: Vec<String> = toggle_items(&s, &state).iter().map(|item| item.key.clone()).collect();
    let mut seen = HashSet::new();
    if picks.is_empty() || picks.iter().any(|p| !offered.contains(p) || !seen.insert(p.as_str())) {
        return Err("Choose offered toggles, each once.".into());
    }
    let newline = if doc.text.contains("\r\n") { "\r\n" } else { "\n" };
    let class_toggle = seen.contains("class");
    let class_final = state.class_audited.is_some() != class_toggle;
    let mut edits = Vec::new();
    let mut needed: Vec<&str> = Vec::new();
    if class_toggle {
        match &state.class_audited {
            Some(mark) => edits.push(removal(&doc.text, mark)),
            None => {
                let anchor = iq::top(&s.masked, s.class_at).into_iter().find(|a| a.name == "Entity").ok_or(Error::Stale)?;
                edits.push(iq::insert_annotation_line(&doc.text, &s.masked, &anchor, "@Audited"));
                needed.push("Audited");
            }
        }
    }
    for field in &state.fields {
        let toggled = |kind: &str| seen.contains(format!("{kind}:{}", field.name).as_str());
        let (add_audited, add_not_audited) = (toggled("audited"), toggled("notaudited"));
        if add_audited && add_not_audited {
            return Err(format!("Toggle only one of @Audited and @NotAudited on {} per run.", field.name).into());
        }
        let audited = field.audited.is_some() != add_audited;
        let not_audited = field.not_audited.is_some() != add_not_audited;
        if audited && not_audited {
            return Err(format!("Field {} would carry both @Audited and @NotAudited.", field.name).into());
        }
        if class_final && audited {
            return Err(format!("Field {} would carry @Audited inside an @Audited class; the docs do not define that combination, so it is refused.", field.name).into());
        }
        if !class_final && not_audited {
            return Err(format!("Field {} would carry @NotAudited in a class that is not @Audited; the docs only define it as an exclusion, so it is refused.", field.name).into());
        }
        let line_start = doc.text[..field.at].rfind('\n').map_or(0, |i| i + 1);
        let indent: String = doc.text[line_start..].chars().take_while(|c| matches!(c, ' ' | '\t')).collect();
        for (name, toggle, present) in [("Audited", add_audited, &field.audited), ("NotAudited", add_not_audited, &field.not_audited)] {
            match (toggle, present) {
                (false, _) => {}
                (true, Some(mark)) if mark.plain => edits.push(removal(&doc.text, mark)),
                (true, Some(_)) => return Err(format!("@{name} on {} has arguments; edit manually.", field.name).into()),
                (true, None) => {
                    edits.push(Edit::insert(field.at, &format!("@{name}{newline}{indent}")));
                    needed.push(name);
                }
            }
        }
    }
    let wanted: Vec<String> = needed.iter().map(|n| format!("{ENVERS}.{n}")).collect();
    for full in &wanted {
        if s.class.imports.iter().any(|i| i.rsplit('.').next() == full.rsplit('.').next() && i != full) {
            return Err("An import with the same simple name already refers to another type; nothing was changed.".into());
        }
    }
    let refs: Vec<&str> = wanted.iter().map(String::as_str).collect();
    if let Some(mut import) = crate::model::import_edit(&doc.text, &refs, s.class.kotlin) {
        import.text = import.text.replace('\n', newline);
        edits.push(import);
    }
    Ok(edits.iter().map(|edit| edit.to_text_edit(&doc.text)).collect())
}

pub(super) fn resolve_toggle(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    super::current(pending, doc)?;
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    proof(project, &pending.root, directory)?;
    let s = iq::source(doc).ok_or(Error::Stale)?;
    let items = toggle_items(&s, &state(doc, &s)?);
    if items.is_empty() {
        return Err("Nothing can be toggled here.".into());
    }
    let picks = host::pick_many("Envers toggles (annotations with arguments are not offered for removal)", &items)?.ok_or(Error::Cancelled)?;
    let edits = plan(doc, &picks)?;
    let after = checked_after(doc, &edits)?;
    if !ui_kit::prompts::confirm_exact(
        "Toggle Envers auditing?",
        &format!("Target: {}\nSource version: {}\n\n{after}\nCurrent source only; an import left unused by a removal is kept. One undoable host edit of this document.", doc.path, doc.version),
    )? {
        return Err(Error::Cancelled);
    }
    super::current(pending, doc)?;
    Ok(FileAction {
        title: "Toggle Envers auditing".into(),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), edits)],
        ..FileAction::default()
    })
}

#[cfg(test)]
pub(super) fn render_revision(
    package: &str,
    ns: &str,
    directory: &str,
    name: &str,
    extra: &[String],
    listener: bool,
    crlf: bool,
) -> Result<Vec<(String, String)>, Error> {
    use revision_wizard::{ListenerChoice, Options, Parent};
    let listener = if listener { ListenerChoice::New } else { ListenerChoice::None };
    revision_wizard::render(&Options { package, ns, directory, name, table: "revinfo", parent: Parent::None, number: "id", timestamp: "timestamp", extra, listener, crlf })
}

fn no_existing_revision_entity(project: &Project) -> Result<(), Error> {
    let deadline = Instant::now() + Duration::from_secs(5);
    for extension in ["java", "kt"] {
        let files = project.find_files(&format!("**/*.{extension}"), MAX_SCANNED as u32 + 1)?;
        if files.len() > MAX_SCANNED {
            return Err("Existing @RevisionEntity discovery is incomplete for a project this large; Envers allows only one, so nothing was created.".into());
        }
        for file in files {
            if Instant::now() >= deadline {
                return Err("Existing @RevisionEntity discovery timed out; nothing was created.".into());
            }
            let text = match project.read_file(&file) {
                Ok(text) => text,
                Err(Error::NotFound(_) | Error::Invalid(_)) => continue,
                Err(error) => return Err(error),
            };
            if text.contains("RevisionEntity") {
                return Err(format!("{file} already mentions RevisionEntity; Envers allows only one revision entity, so nothing was created.").into());
            }
        }
    }
    Ok(())
}

pub(super) fn resolve_revision(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    revision_wizard::resolve(pending, doc, project)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTITY: &str = "package shop;\n\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n\n@Entity\npublic class Invoice {\n    @Id\n    private Long id;\n\n    private String title;\n}\n";
    const PATH: &str = "/project/src/main/java/shop/Invoice.java";

    fn doc(text: &str) -> Document {
        Document { path: PATH.into(), language: "java".into(), text: text.into(), version: 1 }
    }
    fn toggled(text: &str, picks: &[&str]) -> Result<String, String> {
        let picks: Vec<String> = picks.iter().map(|p| (*p).to_owned()).collect();
        plan(&doc(text), &picks).map(|edits| patched(text, &edits).unwrap()).map_err(|e| format!("{e:?}"))
    }
    fn audited_class() -> String {
        ENTITY.replace("@Entity\n", "@Entity\n@Audited\n").replace("import jakarta.persistence.Id;\n", "import jakarta.persistence.Id;\nimport org.hibernate.envers.Audited;\n")
    }

    #[test]
    fn class_add_inserts_annotation_and_single_import() {
        let after = toggled(ENTITY, &["class"]).unwrap();
        assert!(after.contains("@Entity\n@Audited\npublic class Invoice"), "{after}");
        assert_eq!(after.matches("import org.hibernate.envers.Audited;").count(), 1);
    }

    #[test]
    fn class_remove_deletes_the_line_and_keeps_the_import() {
        let after = toggled(&audited_class(), &["class"]).unwrap();
        assert!(after.contains("@Entity\npublic class Invoice"), "{after}");
        assert!(!after.contains("@Audited"));
    }

    #[test]
    fn wildcard_or_existing_import_is_not_duplicated() {
        let wildcard = ENTITY.replace("import jakarta.persistence.Id;\n", "import jakarta.persistence.Id;\nimport org.hibernate.envers.*;\n");
        assert_eq!(toggled(&wildcard, &["class"]).unwrap().matches("import org.hibernate.envers").count(), 1);
        let existing = audited_class().replace("@Audited\n", "");
        assert_eq!(toggled(&existing, &["audited:title"]).unwrap().matches("import org.hibernate.envers.Audited;").count(), 1);
    }

    #[test]
    fn field_audited_and_not_audited_follow_the_class_state() {
        let after = toggled(ENTITY, &["audited:title"]).unwrap();
        assert!(after.contains("    @Audited\n    private String title;"), "{after}");
        let after = toggled(&audited_class(), &["notaudited:title"]).unwrap();
        assert!(after.contains("    @NotAudited\n    private String title;"), "{after}");
        assert!(after.contains("import org.hibernate.envers.NotAudited;"));
    }

    #[test]
    fn field_toggle_removes_an_existing_plain_annotation() {
        let text = ENTITY.replace("    private String title;", "    @Audited\n    private String title;").replace("import jakarta.persistence.Id;\n", "import jakarta.persistence.Id;\nimport org.hibernate.envers.Audited;\n");
        let after = toggled(&text, &["audited:title"]).unwrap();
        assert!(after.contains("\n    private String title;"), "{after}");
        assert!(!after.contains("@Audited"));
    }

    #[test]
    fn conflicts_are_refused() {
        let redundant = toggled(&ENTITY.replace("    private String title;", "    @Audited\n    private String title;").replace("import jakarta.persistence.Id;\n", "import jakarta.persistence.Id;\nimport org.hibernate.envers.Audited;\n"), &["class"]).unwrap_err();
        assert!(redundant.contains("inside an @Audited class"), "{redundant}");
        assert!(toggled(ENTITY, &["notaudited:title"]).unwrap_err().contains("not @Audited"));
        assert!(toggled(ENTITY, &["class", "class"]).is_err());
        assert!(toggled(ENTITY, &["audited:missing"]).is_err());
        assert!(toggled(ENTITY, &[]).is_err());
        assert!(toggled(&audited_class(), &["notaudited:title", "audited:title"]).unwrap_err().contains("only one"));
    }

    #[test]
    fn foreign_audited_arguments_and_methods_are_refused() {
        let foreign = audited_class().replace("org.hibernate.envers.Audited", "org.example.Audited");
        assert!(toggled(&foreign, &["class"]).unwrap_err().contains("not org.hibernate.envers.Audited"));
        let arguments = audited_class().replace("@Audited\n", "@Audited(withModifiedFlag = true)\n");
        assert!(toggled(&arguments, &["class"]).is_err());
        let method = ENTITY.replace("    private String title;", "    private String title;\n\n    @Audited\n    public String getTitle() {\n        return title;\n    }").replace("import jakarta.persistence.Id;\n", "import jakarta.persistence.Id;\nimport org.hibernate.envers.Audited;\n");
        assert!(toggled(&method, &["class"]).unwrap_err().contains("method"));
    }

    #[test]
    fn crlf_is_kept_for_annotations_and_imports() {
        let text = ENTITY.replace('\n', "\r\n");
        let after = toggled(&text, &["class", "audited:title"]).unwrap_err();
        assert!(after.contains("inside an @Audited class"));
        let after = toggled(&text, &["audited:title"]).unwrap();
        assert!(!after.replace("\r\n", "").contains('\n'), "{after:?}");
        assert!(after.contains("    @Audited\r\n    private String title;"));
    }

    #[test]
    fn kotlin_supports_the_class_level_only() {
        let kotlin = "package shop\n\nimport jakarta.persistence.Entity\n\n@Entity\nopen class Invoice {\n    var title: String? = null\n}\n";
        let document = Document { path: "/project/src/main/kotlin/shop/Invoice.kt".into(), language: "kotlin".into(), text: kotlin.into(), version: 1 };
        let edits = plan(&document, &["class".to_owned()]).unwrap();
        let after = patched(kotlin, &edits).unwrap();
        assert!(after.contains("import org.hibernate.envers.Audited\n") && after.contains("@Entity\n@Audited\nopen class Invoice"), "{after}");
        assert!(plan(&document, &["audited:title".to_owned()]).is_err());
    }

    #[test]
    fn envers_must_be_written_literally_in_the_build() {
        assert!(envers_declared("<artifactId>hibernate-envers</artifactId>"));
        assert!(envers_declared("implementation 'org.hibernate.orm:hibernate-envers:6.6.0.Final'"));
        assert!(envers_declared("implementation(\"org.hibernate:hibernate-envers\")"));
        assert!(!envers_declared("implementation(libs.hibernate.envers)"));
        assert!(!envers_declared("<artifactId>hibernate-envers-extras</artifactId>"));
        assert!(!envers_declared("<artifactId>hibernate-core</artifactId>"));
    }

    #[test]
    fn revision_entity_is_rendered_in_both_namespaces_with_a_listener() {
        for ns in ["jakarta.persistence", "javax.persistence"] {
            let files = render_revision("shop", ns, "/project/src/main/java/shop", "CustomRevisionEntity", &["username".to_owned()], true, false).unwrap();
            assert_eq!(files.len(), 2);
            let (target, entity) = &files[0];
            assert_eq!(target, "/project/src/main/java/shop/CustomRevisionEntity.java");
            for expected in [
                format!("import {ns}.Entity;\nimport {ns}.GeneratedValue;\nimport {ns}.Id;\nimport {ns}.Table;\nimport org.hibernate.envers.RevisionEntity;"),
                "@Entity\n@Table(name = \"revinfo\")\n@RevisionEntity(CustomRevisionEntityListener.class)\npublic class CustomRevisionEntity {".to_owned(),
                "    @Id\n    @GeneratedValue\n    @RevisionNumber\n    private int id;\n\n    @RevisionTimestamp\n    private long timestamp;\n\n    private String username;".to_owned(),
                "public String getUsername() {".to_owned(),
                "public void setTimestamp(long timestamp) {".to_owned(),
            ] {
                assert!(entity.contains(&expected), "{expected}\n{entity}");
            }
            let (target, listener) = &files[1];
            assert!(target.ends_with("CustomRevisionEntityListener.java"));
            assert!(listener.contains("implements RevisionListener {") && listener.contains("(CustomRevisionEntity) revisionEntity;"), "{listener}");
        }
    }

    #[test]
    fn revision_entity_without_listener_or_fields_and_with_crlf() {
        let files = render_revision("shop", "jakarta.persistence", "/project/src/main/java/shop", "Rev", &[], false, true).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].1.contains("@Table(name = \"revinfo\")\r\n@RevisionEntity\r\npublic class Rev {"));
        assert!(!files[0].1.replace("\r\n", "").contains('\n'));
        assert!(!files[0].1.contains("username"));
    }

    #[test]
    fn revision_entity_refusals() {
        let render = |name: &str, extra: &[&str], ns: &str| {
            let extra: Vec<String> = extra.iter().map(|e| (*e).to_owned()).collect();
            render_revision("shop", ns, "/project/src/main/java/shop", name, &extra, true, false).map(|_| ())
        };
        assert!(render("Rev", &[], "jakarta.persistence").is_ok());
        assert!(render("1Rev", &[], "jakarta.persistence").is_err());
        assert!(render("Id", &[], "jakarta.persistence").is_err());
        assert!(render("class", &[], "jakarta.persistence").is_err());
        assert!(render("Rev", &["id"], "jakarta.persistence").is_err());
        assert!(render("Rev", &["a", "a"], "jakarta.persistence").is_err());
        assert!(render("Rev", &["bad name"], "jakarta.persistence").is_err());
        assert!(render("Rev", &[], "org.example").is_err());
        let many: Vec<&str> = ["a", "b", "c", "d", "e", "f", "g", "h", "i"].to_vec();
        assert!(render("Rev", &many, "jakarta.persistence").is_err());
    }

    #[test]
    fn eligibility_follows_the_entity_and_language() {
        assert!(eligible_toggle(&doc(ENTITY)) && eligible_revision(&doc(ENTITY)));
        assert!(!eligible_toggle(&doc("package shop;\n\npublic class Plain {\n}\n")));
    }
}

#[cfg(test)]
mod compile {
    use super::*;

    #[test]
    #[ignore = "requires javac, HIBERNATE_ENVERS_JAR, HIBERNATE_CORE_JAR and JAKARTA_PERSISTENCE_JAR"]
    fn generated_revision_entity_compiles_against_envers() {
        let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name}"));
        let classpath = [var("HIBERNATE_ENVERS_JAR"), var("HIBERNATE_CORE_JAR"), var("JAKARTA_PERSISTENCE_JAR")].join(":");
        let dir = std::env::temp_dir().join(format!("jpa-envers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let files = render_revision("shop", "jakarta.persistence", dir.to_str().unwrap(), "AuditRevision", &["username".to_owned()], true, false).unwrap();
        let paths: Vec<_> = files.iter().map(|(path, text)| (std::fs::write(path, text).unwrap(), path.clone()).1).collect();
        let result = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &classpath, "-d"]).arg(&dir).args(&paths).output().expect("javac installed");
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
