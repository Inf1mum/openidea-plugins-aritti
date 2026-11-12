//! Spring Data auditing warnings on an entity that uses `@CreatedDate`, `@LastModifiedDate`,
//! `@CreatedBy` or `@LastModifiedBy`: `@EnableJpaAuditing` is not declared anywhere in the project, and
//! `AuditingEntityListener` is not registered on the entity.
//!
//! "Not declared anywhere" needs every project source to have been read, so a bounded per-project index
//! reads a few files per diagnostics call and stays silent until it is complete. Files are re-read into the
//! index whenever their document is diagnosed; a change made in another file shows on the entity the next
//! time it is diagnosed. An entity with a superclass is never flagged (the listener may sit on the parent),
//! and any `orm.xml` mentioning `AuditingEntityListener` counts as a registration.
use crate::{
    checks::Finding,
    model::{self, Kind, annotations, has_word, mask},
};
use openidea_plugin::{Severity, host, providers::Document};
use std::{cell::RefCell, collections::HashMap, time::{Duration, Instant}};

const AUDITING_ANNOTATIONS: [&str; 4] = ["CreatedDate", "LastModifiedDate", "CreatedBy", "LastModifiedBy"];
const ANNOTATION_PACKAGE: &str = "org.springframework.data.annotation";
const READS_PER_CALL: usize = 64;
const MAX_FILES: u32 = 2001;
const MAX_TEXT: usize = 256 * 1024;

#[derive(Default)]
struct Index {
    listed: bool,
    overflow: bool,
    unread: Vec<String>,
    enables: HashMap<String, bool>,
    listener_in_xml: HashMap<String, bool>,
}

thread_local! {
    static INDEX: RefCell<HashMap<String, Index>> = RefCell::new(HashMap::new());
}

fn enables_auditing(path: &str, text: &str) -> (bool, bool) {
    if path.ends_with(".xml") {
        return (text.contains("jpa:auditing"), text.contains("AuditingEntityListener"));
    }
    (has_word(&mask(text), "EnableJpaAuditing"), false)
}

/// Records the facts of `doc` and reads a few more project files; `true` once every file was read.
fn refresh(root: &str, doc: &Document) -> (bool, bool) {
    INDEX.with(|cell| {
        let mut all = cell.borrow_mut();
        let index = all.entry(root.to_owned()).or_default();
        let (enables, listener) = enables_auditing(&doc.path, &doc.text);
        index.enables.insert(doc.path.clone(), enables);
        index.listener_in_xml.insert(doc.path.clone(), listener);
        let Some(project) = host::project_of(&doc.path) else { return (false, false) };
        if !index.listed {
            index.listed = true;
            for pattern in ["**/*.java", "**/*.kt", "**/*.xml"] {
                match project.find_files(pattern, MAX_FILES) {
                    Ok(files) if files.len() < MAX_FILES as usize => index.unread.extend(files),
                    _ => index.overflow = true,
                }
            }
            let known: Vec<String> = index.enables.keys().cloned().collect();
            index.unread.retain(|file| !known.iter().any(|path| path == file || file.ends_with(path.trim_start_matches('/'))));
        }
        let deadline = Instant::now() + Duration::from_millis(20);
        for _ in 0..READS_PER_CALL {
            if Instant::now() >= deadline {
                break;
            }
            let Some(file) = index.unread.pop() else { break };
            let absolute = if file.starts_with('/') { file.clone() } else { format!("{}/{file}", root.trim_end_matches('/')) };
            match project.read_file(&absolute) {
                Ok(text) if text.len() <= MAX_TEXT => {
                    let (enables, listener) = enables_auditing(&absolute, &text);
                    index.enables.insert(absolute.clone(), enables);
                    index.listener_in_xml.insert(absolute, listener);
                }
                _ => index.overflow = true,
            }
        }
        let complete = index.listed && !index.overflow && index.unread.is_empty();
        (complete && index.enables.values().any(|found| *found), complete && index.listener_in_xml.values().any(|found| *found))
    })
}

fn indexed_complete(root: &str) -> bool {
    INDEX.with(|cell| cell.borrow().get(root).is_some_and(|index| index.listed && !index.overflow && index.unread.is_empty()))
}

/// Keeps the project index current for every Java, Kotlin and XML document.
pub(super) fn observe(doc: &Document) {
    if doc.text.len() > MAX_TEXT {
        return;
    }
    if let Some(root) = host::project_of(&doc.path).and_then(|p| p.root().ok()) {
        refresh(&root, doc);
    }
}

pub(super) fn findings(doc: &Document) -> Vec<Finding> {
    if !matches!(doc.language.as_str(), "java" | "kotlin")
        || doc.text.len() > MAX_TEXT
        || !AUDITING_ANNOTATIONS.iter().any(|name| doc.text.contains(&format!("@{name}")))
    {
        return Vec::new();
    }
    let Some(class) = model::parse_class(&doc.text, &doc.path).filter(|class| class.kind == Kind::Entity && class.superclass.is_none()) else {
        return Vec::new();
    };
    let masked = mask(&doc.text);
    let used: Vec<_> = annotations(&masked)
        .into_iter()
        .filter(|a| AUDITING_ANNOTATIONS.contains(&a.name) && class.imports.iter().any(|import| import == &format!("{ANNOTATION_PACKAGE}.{}", a.name)))
        .collect();
    let Some(first) = used.first() else { return Vec::new() };
    let Some(root) = host::project_of(&doc.path).and_then(|p| p.root().ok()) else { return Vec::new() };
    let (enabled_somewhere, listener_in_xml) = refresh(&root, doc);
    if !indexed_complete(&root) {
        return Vec::new();
    }
    let mut found = Vec::new();
    if !enabled_somewhere {
        found.push(Finding::new(
            first.start,
            first.name_end,
            Severity::Warning,
            "jpa-auditing-not-enabled",
            "Auditing annotations have no effect: @EnableJpaAuditing is not declared in this project's Java, Kotlin or XML sources.",
        ));
    }
    let registered = annotations(&masked).iter().any(|a| {
        a.name == "EntityListeners" && a.args.is_some_and(|(start, end)| has_word(&masked[start..end], "AuditingEntityListener"))
    });
    if !registered && !listener_in_xml {
        found.push(Finding::new(
            first.start,
            first.name_end,
            Severity::Warning,
            "jpa-auditing-listener-missing",
            "AuditingEntityListener is not registered on this entity: add @EntityListeners(AuditingEntityListener.class) or register it as an orm.xml default listener.",
        ));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const PATH: &str = "/project/src/main/java/shop/Invoice.java";
    const AUDITED: &str = "package shop;\n\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport org.springframework.data.annotation.CreatedDate;\nimport java.time.Instant;\n\n@Entity\npublic class Invoice {\n    @Id Long id;\n    @CreatedDate Instant createdDate;\n}\n";
    const CONFIG: &str = "package shop;\n\nimport org.springframework.data.jpa.repository.config.EnableJpaAuditing;\n\n@EnableJpaAuditing\nclass Config {\n}\n";

    fn codes(files: &[(&str, &str)], text: &str) -> Vec<&'static str> {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        for (path, content) in files {
            host.add_file(path, content);
        }
        host.add_file("src/main/java/shop/Invoice.java", text);
        let doc = Document { path: PATH.into(), language: "java".into(), text: text.into(), version: 1 };
        INDEX.with(|cell| cell.borrow_mut().clear());
        findings(&doc).iter().map(|f| f.code).collect()
    }

    #[test]
    fn both_warnings_appear_when_nothing_enables_or_registers_auditing() {
        assert_eq!(codes(&[], AUDITED), ["jpa-auditing-not-enabled", "jpa-auditing-listener-missing"]);
    }

    #[test]
    fn a_declared_enable_annotation_silences_only_the_enable_warning() {
        assert_eq!(codes(&[("src/main/java/shop/Config.java", CONFIG)], AUDITED), ["jpa-auditing-listener-missing"]);
    }

    #[test]
    fn a_registered_listener_silences_the_listener_warning() {
        let text = AUDITED.replace("@Entity\n", "@Entity\n@EntityListeners(AuditingEntityListener.class)\n");
        assert_eq!(codes(&[], &text), ["jpa-auditing-not-enabled"]);
        let xml = "<entity-mappings><persistence-unit-metadata><persistence-unit-defaults><entity-listeners><entity-listener class=\"org.springframework.data.jpa.domain.support.AuditingEntityListener\"/></entity-listeners></persistence-unit-defaults></persistence-unit-metadata></entity-mappings>";
        assert_eq!(codes(&[("src/main/resources/META-INF/orm.xml", xml)], AUDITED), ["jpa-auditing-not-enabled"]);
    }

    #[test]
    fn entities_with_a_superclass_or_without_audit_annotations_are_not_checked() {
        assert!(codes(&[], &AUDITED.replace("class Invoice", "class Invoice extends Base")).is_empty());
        assert!(codes(&[], &AUDITED.replace("@CreatedDate ", "")).is_empty());
    }
}
