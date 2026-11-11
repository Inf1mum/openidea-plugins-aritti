//! Bounded, source-only live checks. This cache is not a complete persistence model.
use crate::{
    checks::*,
    model::{Class, annotations, mask, parse_class, words},
    setting, xml,
};
use openidea_plugin::providers::Document;
use openidea_plugin::{Diagnostic, host};
use std::collections::HashMap;
use std::time::{Duration, Instant};
const MAX_TEXT: usize = 512 * 1024;
const MAX_FILES: usize = 256;
const TTL: Duration = Duration::from_secs(120);
struct TypeFact {
    qualified: String,
    group: String,
    attributes: usize,
    identifier: Option<(String, String)>,
}
#[derive(Default)]
pub struct State {
    roots: HashMap<String, HashMap<String, (Instant, TypeFact)>>,
}
fn source_identifier(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.bytes().enumerate().all(|(i,b)| {
        b.is_ascii_alphabetic() || b == b'_' || b == b'$' || i > 0 && b.is_ascii_digit()
    })
}
pub(super) fn top_level_identity(text: &str, class: &Class) -> bool {
    let Some(package) = &class.package else { return false; };
    if package.len() > 512 || !package.split('.').all(source_identifier) || !source_identifier(&class.name) { return false; }
    let masked = mask(text);
    let Some(name_at) = openidea_plugin::text::offset(text, class.pos) else { return false; };
    let Some(prefix) = masked.get(..name_at) else { return false; };
    let prefix = prefix.trim_end();
    let Some(before_class) = prefix.strip_suffix("class") else { return false; };
    let class_at = before_class.len();
    if !masked[name_at..].starts_with(&class.name)
        || before_class.as_bytes().last().is_some_and(|b| !b.is_ascii() || crate::model::is_ident(*b)) { return false; }
    let mut stack = Vec::new();
    for byte in masked[..class_at].bytes() {
        match byte {
            b'(' | b'[' | b'{' => { if stack.len() >= 128 { return false; } stack.push(byte); }
            b')' | b']' | b'}' => {
                let expected = match byte { b')' => b'(', b']' => b'[', _ => b'{' };
                if stack.pop() != Some(expected) { return false; }
            }
            _ => {}
        }
    }
    stack.is_empty()
}

// Explicit source-input grouping, not a compiler/build/classpath module model.
fn source_group(root: &str, class: &Class) -> Option<String> {
    if class.path.len() > 4096 || !class.path.strip_prefix(root).is_some_and(|rest| root == "/" || rest.starts_with('/'))
        || class.path.split('/').any(|part| matches!(part, "." | "..")) { return None; }
    let package = class.package.as_ref()?;
    if package.len() > 512 || !package.split('.').all(source_identifier) { return None; }
    let (directory, _) = class.path.rsplit_once('/')?;
    let suffix = format!("/{}", package.replace('.', "/"));
    if let Some(base) = directory.strip_suffix(&suffix) {
        let owner = root.trim_end_matches('/');
        if base == owner || base.strip_prefix(owner).is_some_and(|rest| rest.starts_with('/')) {
            return Some(format!("package-root:{base}"));
        }
    }
    Some(format!("flat-directory:{directory}"))
}

impl State {
    pub fn invalidate(&mut self, path: &str) {
        for files in self.roots.values_mut() {
            files.remove(path);
        }
    }
    pub fn update(&mut self, root: &str, path: &str, text: &str) {
        self.invalidate(path);
        if !path.strip_prefix(root).is_some_and(|rest| root == "/" || rest.starts_with('/'))
            || path.split('/').any(|part| matches!(part, "." | "..")) {
            return;
        }
        if text.len() > MAX_TEXT || text.bytes().filter(|b| *b == b'@').take(2049).count() > 2048 || !trusted_annotations(text) {
            return;
        }
        let Some(class) = parse_class(text, path) else {
            return;
        };
        if class.body_end.is_none() || class.attributes.len() > 512 || !top_level_identity(text, &class) {
            return;
        }
        let Some(group) = source_group(root, &class) else { return; };
        if !self.roots.contains_key(root) && self.roots.len() >= 8 {
            self.roots.clear();
        }
        let files = self.roots.entry(root.into()).or_default();
        files.retain(|_, (time, _)| time.elapsed() < TTL);
        if files.len() >= MAX_FILES || files.values().map(|(_, c)| c.attributes).sum::<usize>() + class.attributes.len() > 4096 {
            files.clear();
        }
        let doc = Document {path:path.into(),language:if path.ends_with(".java") {"java"} else {"kotlin"}.into(),text:text.into(),version:0};
        let identifier = crate::local_repository::identifier(&doc).map(|id|(id,text.to_owned()));
        let fact = TypeFact { identifier, qualified: format!("{}.{}", class.package.as_deref().unwrap(), class.name), group, attributes: class.attributes.len() };
        files.insert(path.into(), (Instant::now(), fact));
    }
    fn known(&self, root: &str, current: &Class, name: &str) -> bool {
        if name.split('.').next() == Some(current.name.as_str()) && name.contains('.') { return false; }
        let fqcn = if name.contains('.') { name.to_owned() } else {
            let imports: Vec<_> = current.imports.iter().filter(|i| i.rsplit('.').next() == Some(name)).collect();
            if imports.len() > 1 { return false; }
            if let Some(import) = imports.first() { (*import).clone() } else {
                let Some(qualified) = current.qualified(name) else { return false; };
                qualified
            }
        };
        let Some(group) = source_group(root, current) else { return false; };
        let Some(files) = self.roots.get(root) else {
            return false;
        };
        let mut matches = files.iter().filter(|(_, (time, fact))| {
            time.elapsed() < TTL && fact.qualified == fqcn && fact.group == group
        });
        let Some((path, _)) = matches.next() else { return false };
        matches.next().is_none()
            && host::project_of(path).and_then(|project| project.root().ok()).as_deref() == Some(root)
    }
    pub fn diagnostics(&mut self, doc: &Document) -> Vec<Diagnostic> {
        self.invalidate(&doc.path);
        if doc.text.len() > MAX_TEXT
            || doc.text.bytes().filter(|b| *b == b'@').take(2049).count() > 2048
            || !matches!(doc.language.as_str(), "java" | "kotlin" | "xml")
        {
            return vec![];
        }
        let root = host::project_of(&doc.path).and_then(|p| p.root().ok());
        if let Some(root) = &root {
            self.update(root, &doc.path, &doc.text);
        }
        let text = doc.text.as_str();
        let mut findings = if doc.language == "xml" {
            if !setting("checkPersistenceXml") {
                return vec![];
            }
            // Unknown project/library mappings are not negative evidence.
            xml::inspect_owned(text)
        } else {
            if !trusted_annotations(text) {
                return vec![];
            }
            let kotlin = doc.language == "kotlin";
            let mut findings = vec![];
            if setting("checkClassSignature") {
                findings.extend(inspect_class_signature_owned(text, kotlin));
            }
            if let Some(class) = parse_class(text, &doc.path) {
                findings.extend(inspect_entity_owned(text, kotlin, &class));
                if kotlin && setting("checkKotlinEntities") {
                    findings.extend(inspect_kotlin_entity_owned(text, &class));
                }
                if !setting("checkKotlinEntities") {
                    findings.retain(|f| f.code != "jpa-data-class");
                }
                if setting("checkAttributeSignatures") {
                    findings.extend(inspect_attribute_signatures_owned(text, kotlin, &class, persistence_annotations_only(text)));
                }
                if setting("checkAttributeTypes") {
                    let known = std::cell::RefCell::new(HashMap::new());
                    let plain_owner = plain_type_owner(text, kotlin, &class);
                    findings.extend(inspect_attribute_types_owned(text, kotlin, &class, &|name| {
                        if !plain_owner { return false; }
                        let mut known = known.borrow_mut();
                        if let Some(found) = known.get(name) { return *found; }
                        if known.len() >= 32 { return false; }
                        let found = root.as_ref().is_some_and(|r| self.known(r, &class, name));
                        known.insert(name.to_owned(), found);
                        found
                    }));
                }
            }
            if setting("checkAttributeTypes") { findings.extend(crate::local_versions::findings(doc)); findings.extend(crate::local_identifiers::findings(doc)); }
            if setting("checkAttributeTypes") {
                if let Some(finding)=crate::local_repository::missing_identifier(doc) {findings.push(finding);}
                if let Some(root)=&root {
                    if let Some((entity,id,from,to))=crate::local_repository::repository(doc) {
                        let facts:Vec<_>=self.roots.get(root).into_iter().flat_map(|files|files.iter()).filter(|(_, (time,fact))|time.elapsed()<TTL && fact.qualified==entity).collect();
                        if let [(path,(_,fact))]=facts.as_slice() {
                            if let Some((declared,source))=&fact.identifier {
                                let source_root=format!("{}/src/main/java/",root.trim_end_matches('/'));
                                let expected=format!("{}{}.java",source_root,entity.replace('.',"/"));
                                let source_owned=path.as_str()==expected && doc.path.starts_with(&source_root);
                                let fresh=source_owned && host::project_of(path).filter(|p|p.root().ok().as_deref()==Some(root.as_str())).and_then(|p|p.read_file(path).ok()).as_deref()==Some(source.as_str());
                                if fresh && declared!=&id { findings.push(Finding::new(from,to,openidea_plugin::Severity::Warning,"jpa-source-repository-id",format!("Repository identifier {id} differs from the current saved source identifier {declared} of {entity}. XML/provider mapping is unresolved."))); }
                            }
                        }
                    }
                }
            }
            if setting("checkEntityId") { findings.extend(crate::local_hierarchy::identifier_absent(doc)); }
            if setting("checkAttributeTypes") { findings.extend(crate::local_hierarchy::converter_findings(doc)); }
            findings.extend(crate::local_graph::findings(doc));
            findings.extend(crate::local_listener::findings(doc));
            findings.extend(crate::local_listener::callback_findings(doc));
            if setting("checkListenerSignatures") {
                findings.extend(crate::local_callbacks::findings(doc));
            }
            if setting("checkQueryReferences") {
                findings.extend(inspect_query_parameters_owned(text, kotlin));
            }
            findings
        };
        findings.retain(|f| f.start < f.end && f.end <= text.len() && text.is_char_boundary(f.start) && text.is_char_boundary(f.end));
        findings.truncate(512);
        findings.iter().map(|f| f.diagnostic(text)).collect()
    }
}
// Every consumed framework annotation needs canonical source ownership before
// live diagnostics/cache admission. Wildcards and local lookalikes are unknown.
pub(super) fn trusted_annotations(text: &str) -> bool {
    annotation_admission(text, false)
}
pub(super) fn persistence_annotations_only(text: &str) -> bool {
    annotation_admission(text, true)
}
fn annotation_admission(text: &str, persistence_only: bool) -> bool {
    const NAMES: &[&str] = &[
        "Entity", "MappedSuperclass", "Embeddable", "Id", "EmbeddedId", "IdClass", "Column", "Basic",
        "Version", "Temporal", "Enumerated", "OneToOne", "OneToMany", "ManyToOne", "ManyToMany", "ElementCollection",
        "JoinColumn", "JoinColumns", "JoinTable", "Lob", "Embedded", "Access", "Transient",
        "EntityListeners", "PrePersist", "PostPersist", "PreUpdate", "PostUpdate", "PreRemove", "PostRemove", "PostLoad",
        "NamedEntityGraph", "NamedEntityGraphs", "NamedAttributeNode", "NamedSubgraph",
        "AttributeOverride", "AttributeOverrides", "MapsId", "MapKey",
    ];
    let registered = |name: &str| NAMES.contains(&name) || matches!(name, "Query" | "Param" | "Data" | "EqualsAndHashCode" | "Immutable");
    let known_owner = |name: &str, full: &str| {
        if NAMES.contains(&name) {
            return full == format!("jakarta.persistence.{name}") || full == format!("javax.persistence.{name}");
        }
        match name {
            "Query" => full == "org.springframework.data.jpa.repository.Query",
            "Param" => full == "org.springframework.data.repository.query.Param",
            "Data" | "EqualsAndHashCode" => full == format!("lombok.{name}"),
            "Immutable" => full == "org.hibernate.annotations.Immutable",
            _ => true,
        }
    };
    let masked = mask(text);
    let imports: Vec<_> = words(&masked, "import").map(|at| {
        let rest = masked[at + "import".len()..].trim_start();
        rest.split([';', '\n', '\r']).next().unwrap_or(rest).trim()
    }).collect();
    if imports.iter().any(|import| {
        let name = import.rsplit('.').next().unwrap_or(import);
        let ordinary = ["String", "List", "Set", "Map", "Collection", "Date", "Calendar", "Long", "Integer", "Boolean"];
        let foreign_basic = ordinary.contains(&name)
            && !(import.starts_with("java.lang.") || import.starts_with("java.util.") || import.starts_with("kotlin."));
        foreign_basic || !known_owner(name, import) || import.contains(" as ")
    }) { return false; }
    let all = annotations(&masked);
    if persistence_only && all.iter().any(|a| !NAMES.contains(&a.name)) { return false; }
    let mut short_names = Vec::new();
    let mut prefixes = Vec::new();
    let mut persistence_namespace = None;
    for a in all.iter().filter(|a| registered(a.name)) {
        let written = &masked[a.start + 1..a.name_end];
        if written.contains(':') { return false; }
        let full = if written.contains('.') {
            if !known_owner(a.name, written) { return false; }
            prefixes.push(written.split('.').next().unwrap());
            written
        } else {
            let candidates: Vec<_> = imports.iter().filter(|i| i.rsplit('.').next() == Some(a.name)).collect();
            let [owner] = candidates.as_slice() else { return false; };
            if !known_owner(a.name, owner) { return false; }
            short_names.push(a.name);
            **owner
        };
        if NAMES.contains(&a.name) {
            let ns = if full.starts_with("jakarta.persistence.") { "jakarta" } else { "javax" };
            if persistence_namespace.is_some_and(|prior| prior != ns) { return false; }
            persistence_namespace = Some(ns);
        }
    }
    // A visible source type declaration may hide a short annotation or the first
    // segment of a qualified name. Do not pretend an import resolves that case.
    for keyword in ["class", "interface", "enum", "record", "object", "typealias"] {
        for at in words(&masked, keyword) {
            let rest = masked[at + keyword.len()..].trim_start();
            let len = rest.bytes().take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'$').count();
            let name = &rest[..len];
            if short_names.contains(&name) || prefixes.contains(&name) { return false; }
            if let Some(end) = rest.find('{') {
                let header = &rest[..end];
                if header.contains('<') && short_names.iter().chain(prefixes.iter()).any(|n| words(header, n).next().is_some()) { return false; }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::JpaAssist;
    use openidea_plugin::diagnostics::DiagnosticsProvider;
    use openidea_plugin::mock::MockHost;
    use openidea_plugin::{Event, Plugin};
    fn doc(path: &str, text: &str) -> Document {
        Document {
            path: path.into(),
            language: if path.ends_with(".xml") {
                "xml"
            } else if path.ends_with(".kt") {
                "kotlin"
            } else {
                "java"
            }
            .into(),
            text: text.into(),
            version: 7,
        }
    }
    const BAD: &str = "// 😀\n@jakarta.persistence.Entity\npublic final class Example { @jakarta.persistence.ManyToOne Object other; }";
    fn has(d: &[Diagnostic], code: &str) -> bool {
        d.iter().any(|d| d.code.as_deref() == Some(code))
    }
    #[test]
    fn unsaved_versioned_provider_clears_without_manual_publication_or_project_grant() {
        for caps in [vec![], vec!["events", "project.read", "editor.read"]] {
            let host = MockHost::install().with_capabilities(&caps);
            host.add_file("Example.java", "class Example {}");
            host.open_editor("/project/Example.java", "java", BAD, &[]);
            let mut plugin = JpaAssist::default();
            host.refresh_diagnostics(&mut plugin);
            let d = host.provider_diagnostics();
            assert!(has(&d["/project/Example.java"], "jpa-final-entity"));
            assert!(host.diagnostics().is_empty());
            host.open_editor("/project/Example.java", "java", "class Example {}", &[]);
            host.refresh_diagnostics(&mut plugin);
            assert!(host.provider_diagnostics().values().all(Vec::is_empty));
        }
    }
    #[test]
    fn current_entity_graph_node_names_use_supplied_source_and_exact_utf16_ranges() {
        let _host = MockHost::install();
        let source = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport java.lang.Long;\nimport java.lang.String;\nimport jakarta.persistence.NamedEntityGraph;\nimport jakarta.persistence.NamedAttributeNode;\n@jakarta.persistence.Entity\n@NamedEntityGraph(name=\"summary\", attributeNodes={@NamedAttributeNode(\"name\"), /* 😀 */ @NamedAttributeNode(\"missing\")})\nclass Person { @jakarta.persistence.Id Long id; String name; }";
        let mut plugin = JpaAssist::default();
        let d = doc("/project/src/main/java/shop/Person.java", source);
        let findings = plugin.diagnostics(&d);
        let nodes: Vec<_> = findings.iter().filter(|f|f.code.as_deref()==Some("jpa-unknown-entity-graph-node")).collect();
        assert_eq!(nodes.len(),1);
        let finding=nodes[0];
        let a=openidea_plugin::text::offset(source,finding.range.start).unwrap();
        let b=openidea_plugin::text::offset(source,finding.range.end).unwrap();
        assert_eq!(&source[a..b],"missing");
        assert!(finding.message.contains("source-declared") && finding.message.contains("current"));
        let fixed=Document{text:source.replace("\"missing\"","\"name\""),version:2,..d};
        assert!(!plugin.diagnostics(&fixed).iter().any(|f|f.code.as_deref()==Some("jpa-unknown-entity-graph-node")));
        assert!(plugin.sources.is_empty() && plugin.classes.is_empty());
    }
    #[test]
    fn actual_events_do_not_publish_legacy_scanning_results() {
        let host = MockHost::install().with_capabilities(&["events", "project.read", "editor.read", "storage"]);
        host.add_file("Example.java", BAD);
        let mut plugin = JpaAssist::default();
        plugin.handle_events(&[Event::FileOpened("/project/Example.java".into())], &host.invocation());
        assert!(host.diagnostics().is_empty());
        assert!(!plugin.scanned && plugin.sources.is_empty() && plugin.classes.is_empty());
        assert!(has(&plugin.diagnostics(&doc("/project/Example.java", BAD)), "jpa-final-entity"));
    }
    #[test]
    fn utf16_ranges_select_exact_nonempty_source_tokens() {
        let _host = MockHost::install();
        let mut state = State::default();
        let d = state.diagnostics(&doc("/project/Example.java", BAD));
        for finding in d {
            let a = openidea_plugin::text::offset(BAD, finding.range.start).unwrap();
            let b = openidea_plugin::text::offset(BAD, finding.range.end).unwrap();
            assert!(a < b && b <= BAD.len());
            if finding.code.as_deref() == Some("jpa-final-entity") {
                assert_eq!(&BAD[a..b], "final");
                assert_eq!(finding.range.start.line, 2);
            }
        }
    }
    #[test]
    fn unknown_inheritance_library_and_xml_mapping_absence_are_quiet() {
        let _host = MockHost::install();
        let mut state = State::default();
        assert!(state.diagnostics(&doc("/project/Example.java", "@jakarta.persistence.Entity class Example extends LibraryBase {} ")).is_empty());
        assert!(
            state
                .diagnostics(&doc(
                    "/project/persistence.xml",
                    "<persistence><persistence-unit name=\"x\"><class>library.Unknown</class></persistence-unit></persistence>"
                ))
                .is_empty()
        );
        let d = state.diagnostics(&doc(
            "/project/persistence.xml",
            "<persistence><persistence-unit name=\"x\" transaction-type=\"BAD\"/></persistence>",
        ));
        assert!(!d.is_empty());
        assert!(
            state
                .diagnostics(&doc(
                    "/project/custom.xml",
                    "<persistence xmlns=\"urn:custom\"><persistence-unit transaction-type=\"BAD\"/></persistence>"
                ))
                .is_empty()
        );
    }
    #[test]
    fn foreign_annotations_aliases_and_foreign_basic_types_are_unknown() {
        let _host = MockHost::install();
        for text in [
            "import app.Entity;\n@jakarta.persistence.Entity public final class Example {}",
            "@app.Entity public final class Example {}",
            "import app.Entity; @jakarta.persistence.Entity public final class Example {}",
            "@app.Query(\"select :missing\") interface R { Object m(); }",
            "import app.Marker as Entity\n@jakarta.persistence.Entity class Example",
            "import app.String;\n@jakarta.persistence.Entity class Example { @jakarta.persistence.Version String v; }",
        ] {
            assert!(State::default().diagnostics(&doc("/project/Example.java", text)).is_empty(), "{text}");
        }
    }
    #[test]
    fn kotlin_local_checks_and_settings_are_respected() {
        let host = MockHost::install();
        let text = "@jakarta.persistence.Entity\ndata class Example { val name: String = \"\" }";
        let d = State::default().diagnostics(&doc("/project/Example.kt", text));
        assert!(has(&d, "jpa-data-class"));
        host.set_setting("jpaAssist.checkKotlinEntities", "false");
        let d = State::default().diagnostics(&doc("/project/Example.kt", text));
        assert!(!has(&d, "jpa-data-class") && !has(&d, "jpa-val-attribute"));
    }
    #[test]
    fn cache_is_root_and_exact_qualified_name_owned_invalidated_and_expiring() {
        let host = MockHost::install();
        host.add_project("/a");
        let mut state = State::default();
        let text = "package shop;\n@jakarta.persistence.Entity class Value {}";
        state.update("/a", "/a/Value.java", text);
        let c = parse_class("package shop;\n@jakarta.persistence.Entity class Example {}", "/a/Example.java").unwrap();
        assert!(state.known("/a", &c, "Value"));
        assert!(!state.known("/b", &c, "Value"));
        let other = parse_class("package other;\n@jakarta.persistence.Entity class Example {}", "/a/Other.java").unwrap();
        assert!(!state.known("/a", &other, "Value"));
        state.update("/a", "/a/duplicate.java", text);
        assert!(!state.known("/a", &c, "Value"));
        state.invalidate("/a/duplicate.java");
        state.roots.get_mut("/a").unwrap().get_mut("/a/Value.java").unwrap().0 = Instant::now() - TTL;
        assert!(!state.known("/a", &c, "Value"));
        state.update("/a", "/a/Value.java", "class Value {}");
        assert!(!state.known("/a", &c, "Value"));
    }
    #[test]
    fn limits_drop_overflow_and_current_document_removal() {
        let _host = MockHost::install();
        let mut state = State::default();
        for i in 0..=MAX_FILES {
            state.update("/a", &format!("/a/C{i}.java"), &format!("package p;\n@jakarta.persistence.Entity class C{i} {{}}"));
        }
        assert!(state.roots["/a"].len() <= MAX_FILES);
        assert!(state.diagnostics(&doc("/project/Example.java", &"x".repeat(MAX_TEXT + 1))).is_empty());
        let mut d = doc("/project/Example.java", BAD);
        d.language = "plaintext".into();
        assert!(state.diagnostics(&d).is_empty());
    }
    #[test]
    fn repeated_and_distinct_positive_types_share_bounded_owner_queries() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        for i in 0..33 {
            plugin.diagnostics(&doc(&format!("/project/Value{i}.java"), &format!("package shop;\n@jakarta.persistence.Entity class Value{i} {{}}")));
        }
        let mut source = "package shop;\n@jakarta.persistence.Entity class Example {".to_owned();
        for i in 0..33 { source.push_str(&format!(" @jakarta.persistence.Enumerated Value{i} field{i};")); }
        source.push_str(" @jakarta.persistence.Enumerated Value0 repeated; }");
        let findings = plugin.diagnostics(&doc("/project/Example.java", &source));
        assert_eq!(findings.iter().filter(|finding| finding.code.as_deref() == Some("attribute-type")).count(), 33,
            "32 distinct owners plus one repeated already-established type; later unknown types stay quiet");
    }
    #[test]
    fn nested_project_reassignment_withholds_cached_positive_type_diagnostics() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let value = "package shop.domain;\n@jakarta.persistence.Entity class Value {}";
        let owner = "package shop;\nimport shop.domain.Value;\n@jakarta.persistence.Entity class Example { @jakarta.persistence.Enumerated Value value; }";
        plugin.diagnostics(&doc("/project/src/main/java/shop/domain/Value.java", value));
        assert!(has(&plugin.diagnostics(&doc("/project/src/main/java/shop/Example.java", owner)), "attribute-type"));
        host.add_project("/project/src/main/java/shop/domain");
        assert!(!has(&plugin.diagnostics(&doc("/project/src/main/java/shop/Example.java", owner)), "attribute-type"));
        assert!(plugin.sources.is_empty() && plugin.classes.is_empty());
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }
    #[test]
    fn positive_type_facts_follow_current_text_and_files_changed_without_source_scan() {
        let host = MockHost::install().with_capabilities(&["project.read", "events"]);
        let mut plugin = JpaAssist::default();
        let value = "package shop;\n@jakarta.persistence.Entity class Value {}";
        let owner = "package shop;\n@jakarta.persistence.Entity class Example { @jakarta.persistence.Enumerated Value value; }";
        // A disk file alone is not authorization to discover it in the provider.
        host.add_file("Value.java", value);
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", owner)), "attribute-type"));
        plugin.diagnostics(&doc("/project/Value.java", value));
        assert!(has(&plugin.diagnostics(&doc("/project/Example.java", owner)), "attribute-type"));
        plugin.handle_events(
            &[Event::FilesChanged(openidea_plugin::FilesChanged { modified: vec!["/project/Value.java".into()], ..Default::default() })],
            &host.invocation(),
        );
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", owner)), "attribute-type"));
        plugin.diagnostics(&doc("/project/Value.java", value));
        plugin.diagnostics(&doc("/project/Value.java", "package shop; enum Value { A }"));
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", owner)), "attribute-type"));
        assert!(plugin.sources.is_empty() && plugin.classes.is_empty() && host.diagnostics().is_empty());
    }
    #[test]
    fn foreign_qualified_scalar_names_do_not_borrow_jdk_type_semantics() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        for ty in ["foreign.String", "foreign.Boolean", "vendor.BigDecimal", "vendor.UUID"] {
            let source = format!("package shop;\n@jakarta.persistence.Entity class Example {{ @jakarta.persistence.Version {ty} stamp; @jakarta.persistence.Enumerated {ty} kind; }}");
            assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", &source)), "attribute-type"), "{source}");
        }
    }

    #[test]
    fn explicit_scalar_owner_current_text_and_primitive_ranges_are_preserved() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = "package shop;\n@jakarta.persistence.Entity class Example { /* 🦉 */ @jakarta.persistence.Version java.lang.String stamp; }";
        let mut d = doc("/project/Example.java", source);
        let found = plugin.diagnostics(&d);
        let finding = found.iter().find(|d| d.code.as_deref() == Some("attribute-type")).unwrap();
        let a = openidea_plugin::text::offset(source, finding.range.start).unwrap();
        let b = openidea_plugin::text::offset(source, finding.range.end).unwrap();
        assert_eq!(&source[a..b], "@jakarta.persistence.Version");
        d.text = source.replace("java.lang.String", "foreign.String"); d.version += 1;
        assert!(!has(&plugin.diagnostics(&d), "attribute-type"));
        let imported = "package shop;\nimport java.lang.String;\n@jakarta.persistence.Entity class Example { @jakarta.persistence.Version String stamp; }";
        assert!(has(&plugin.diagnostics(&doc("/project/Example.java", imported)), "attribute-type"));
        for unknown in [
            imported.replace("import java.lang.String;", ""),
            imported.replace("import java.lang.String;", "import java.lang.*;"),
            imported.replace("import java.lang.String;", "import vendor.String;"),
            imported.replace("class Example", "class String"),
            imported.replace("class Example", "class Example<String>"),
            imported.replace("class Example", "class Example extends LibraryBase"),
            imported.replace("String stamp;", "String stamp; class String {}"),
            imported.replace("import java.lang.String;", "import java.lang.String; import java.util.String;"),
        ] {
            assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", &unknown)), "attribute-type"), "{unknown}");
        }
        let primitives = "@jakarta.persistence.Entity class Example { @jakarta.persistence.Version boolean stamp; @jakarta.persistence.Temporal long date; @jakarta.persistence.Enumerated int kind; }";
        assert_eq!(plugin.diagnostics(&doc("/project/Example.java", primitives)).iter().filter(|d|d.code.as_deref()==Some("attribute-type")).count(), 3);
    }
    #[test]
    fn qualified_positive_model_facts_do_not_borrow_a_different_simple_type_owner() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        plugin.diagnostics(&doc("/project/Value.java", "package shop;\n@jakarta.persistence.Entity class Value {}"));
        let source = "package shop;\n@jakarta.persistence.Entity class Example { @jakarta.persistence.Enumerated other.Value field; }";
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", source)), "attribute-type"));
        let exact = source.replace("other.Value", "shop.Value");
        assert!(has(&plugin.diagnostics(&doc("/project/Example.java", &exact)), "attribute-type"));
        host.add_project("/project/Value.java");
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", &exact)), "attribute-type"));
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn shadowed_or_ambiguous_short_names_do_not_borrow_positive_cached_roles() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        plugin.diagnostics(&doc("/project/Value.java", "package shop;\n@jakarta.persistence.Entity class Value {}"));
        for source in [
            "package shop;\n@jakarta.persistence.Entity class Example<Value> { @jakarta.persistence.Enumerated Value value; }",
            "package shop;\n@jakarta.persistence.Entity class Example extends LibraryBase { @jakarta.persistence.Enumerated Value value; }",
            "package shop;\n@jakarta.persistence.Entity class Example { class Value {} @jakarta.persistence.Enumerated Value value; }",
            "package shop;\nimport shop.Value;\nimport other.Value;\n@jakarta.persistence.Entity class Example { @jakarta.persistence.Enumerated Value value; }",
        ] {
            assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", source)), "attribute-type"), "{source}");
        }
    }

    #[test]
    fn current_import_correction_restores_exact_cached_role_ranges_without_scanning() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        plugin.diagnostics(&doc("/project/Value.java", "package shop;\n@jakarta.persistence.Entity class Value {}"));
        let source = "package shop;\nimport shop.Value;\nimport other.Value;\n@jakarta.persistence.Entity class Example { /* 🦉 */ @jakarta.persistence.Enumerated Value field; }";
        let mut d = doc("/project/Example.java", source);
        assert!(!has(&plugin.diagnostics(&d), "attribute-type"));
        d.text = source.replace("import other.Value;", ""); d.version += 1;
        let found = plugin.diagnostics(&d);
        let finding = found.iter().find(|d|d.code.as_deref()==Some("attribute-type")).unwrap();
        let a = openidea_plugin::text::offset(&d.text, finding.range.start).unwrap();
        let b = openidea_plugin::text::offset(&d.text, finding.range.end).unwrap();
        assert_eq!(&d.text[a..b], "@jakarta.persistence.Enumerated");
        d.text = d.text.replace("class Example", "class Example<Value>"); d.version += 1;
        assert!(!has(&plugin.diagnostics(&d), "attribute-type"));
        for source in [
            "package shop;\n@jakarta.persistence.Entity class Example<java> { @jakarta.persistence.Version java.lang.String field; }",
            "package shop;\n@jakarta.persistence.Entity class java { @jakarta.persistence.Version java.lang.String field; }",
            "package shop;\n@jakarta.persistence.Entity class shop { @jakarta.persistence.Enumerated shop.Value field; }",
            "package shop;\n@jakarta.persistence.Entity class Example<Value> { @jakarta.persistence.Enumerated shop.Value field; }",
        ] {
            assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", source)), "attribute-type"), "{source}");
        }
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn annotation_subpackages_do_not_borrow_framework_semantics() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        for source in [
            "@jakarta.persistence.fake.Entity public final class Example {}",
            "import javax.persistence.fake.Entity;\n@jakarta.persistence.Entity public final class Example {}",
            "@jakarta.persistence.Entity @lombok.fake.Data class Example {}",
            "import lombok.fake.Data;\n@jakarta.persistence.Entity @lombok.Data class Example {}",
            "interface R { @org.springframework.data.fake.Query(\"select :missing\") Object read(); }",
            "import org.springframework.data.mongodb.repository.Query;\ninterface R { @org.springframework.data.jpa.repository.Query(\"select :missing\") Object read(); }",
            "interface R { @org.springframework.data.jpa.repository.Query(\"select :missing\") Object read(@org.springframework.data.fake.Param(\"other\") int value); }",
        ] {
            assert!(plugin.diagnostics(&doc("/project/Example.java", source)).is_empty(), "{source}");
        }
    }

    #[test]
    fn exact_annotation_owner_current_text_restores_supported_diagnostics() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let text = "/* 🦉 */ @jakarta.persistence.fake.Entity public final class Example {}";
        let mut d = doc("/project/Example.java", text);
        assert!(plugin.diagnostics(&d).is_empty());
        d.text = text.replace("persistence.fake.Entity", "persistence.Entity"); d.version += 1;
        let findings = plugin.diagnostics(&d);
        let finding = findings.iter().find(|d|d.code.as_deref()==Some("jpa-final-entity")).unwrap();
        let a = openidea_plugin::text::offset(&d.text, finding.range.start).unwrap();
        let b = openidea_plugin::text::offset(&d.text, finding.range.end).unwrap();
        assert_eq!(&d.text[a..b], "final");
        for source in [
            "@javax.persistence.Entity public final class Example {}",
            "import jakarta.persistence.Entity;\n@jakarta.persistence.Entity public final class Example {}",
            "@jakarta.persistence.Entity @lombok.Data class Example {}",
            "interface R { @org.springframework.data.jpa.repository.Query(\"select :missing\") Object read(); }",
            "import org.springframework.data.jpa.repository.Query;\nimport org.springframework.data.repository.query.Param;\ninterface R { @org.springframework.data.jpa.repository.Query(\"select :missing\") Object read(@org.springframework.data.repository.query.Param(\"other\") int value); }",
        ] {
            assert!(!plugin.diagnostics(&doc("/project/Example.java", source)).is_empty(), "{source}");
        }
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn foreign_relationship_type_names_are_not_jdk_basic_or_collection_evidence() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        for ty in ["vendor.String", "vendor.Integer", "vendor.List", "vendor.Map"] {
            let source = format!("package shop;\n@jakarta.persistence.Entity class Example {{ @jakarta.persistence.ManyToOne {ty} value; }}");
            assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", &source)), "jpa-relationship-type"), "{source}");
        }
    }

    #[test]
    fn relationship_owner_current_text_and_exact_ranges_keep_supported_structural_checks() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = "package shop;\n@jakarta.persistence.Entity class Example { /* 🦉 */ @jakarta.persistence.ManyToOne java.util.List<Target> value; }";
        let mut d = doc("/project/Example.java", source);
        let found = plugin.diagnostics(&d);
        let finding = found.iter().find(|d|d.code.as_deref()==Some("jpa-relationship-type")).unwrap();
        let a = openidea_plugin::text::offset(&d.text, finding.range.start).unwrap();
        let b = openidea_plugin::text::offset(&d.text, finding.range.end).unwrap();
        assert_eq!(&d.text[a..b], "@jakarta.persistence.ManyToOne");
        d.text = source.replace("java.util.List", "vendor.List"); d.version += 1;
        assert!(!has(&plugin.diagnostics(&d), "jpa-relationship-type"));
        let imported = "package shop;\nimport java.util.List;\n@jakarta.persistence.Entity class Example { @jakarta.persistence.ManyToOne List<Target> value; }";
        assert!(has(&plugin.diagnostics(&doc("/project/Example.java", imported)), "jpa-relationship-type"));
        for unknown in [
            imported.replace("import java.util.List;", ""),
            imported.replace("import java.util.List;", "import java.util.*;"),
            imported.replace("import java.util.List;", "import vendor.List;"),
            imported.replace("class Example", "class Example<List>"),
            imported.replace("class Example", "class Example extends Base"),
            imported.replace("List<Target> value;", "List<Target> value; class List {}"),
            imported.replace("import java.util.List;", "import java.util.List;\nimport java.awt.List;"),
        ] {
            assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", &unknown)), "jpa-relationship-type"), "{unknown}");
        }
        for ty in ["long", "Target[]", "java.lang.String"] {
            let source = format!("@jakarta.persistence.Entity class Example {{ @jakarta.persistence.ManyToOne {ty} value; }}");
            assert!(has(&plugin.diagnostics(&doc("/project/Example.java", &source)), "jpa-relationship-type"));
        }
        let structural = "@jakarta.persistence.Entity class Example { @jakarta.persistence.ManyToOne @jakarta.persistence.OneToOne vendor.Value value; }";
        assert!(has(&plugin.diagnostics(&doc("/project/Example.java", structural)), "jpa-attribute-multiple-relations"));
    }

    #[test]
    fn live_and_cached_framework_roles_require_explicit_canonical_annotation_owners() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        for source in [
            "@Entity public final class Example {}",
            "import app.*;\n@Entity public final class Example {}",
            "import jakarta.persistence.*;\n@Entity public final class Example {}",
            "import jakarta.persistence.Entity;\n@Entity class Example { @ManyToOne Object value; }",
            "@jakarta.persistence.Entity class Example { @jakarta.persistence.Id long id; @app.JoinColumn long getValue() {return 0;} }",
            "import jakarta.persistence.Entity;\n@interface Entity {}\n@Entity public final class Example {}",
        ] {
            assert!(plugin.diagnostics(&doc("/project/Example.java", source)).is_empty(), "{source}");
        }
        let value = "package shop;\n@Entity class Value {}";
        plugin.diagnostics(&doc("/project/Value.java", value));
        let owner = "package shop;\n@jakarta.persistence.Entity class Example { @jakarta.persistence.Enumerated Value value; }";
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", owner)), "attribute-type"));
    }

    #[test]
    fn explicit_import_current_text_changes_control_live_and_cached_owner_admission() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let text = "/* 🦉 */\n@Entity public final class Example {}";
        let mut d = doc("/project/Example.java", text);
        assert!(plugin.diagnostics(&d).is_empty());
        d.text = format!("import jakarta.persistence.Entity;\n{text}"); d.version += 1;
        let findings = plugin.diagnostics(&d);
        let finding = findings.iter().find(|d|d.code.as_deref()==Some("jpa-final-entity")).unwrap();
        let a = openidea_plugin::text::offset(&d.text, finding.range.start).unwrap();
        let b = openidea_plugin::text::offset(&d.text, finding.range.end).unwrap();
        assert_eq!(&d.text[a..b], "final");
        d.text = text.into(); d.version += 1;
        assert!(plugin.diagnostics(&d).is_empty());
        let owner = "package shop;\n@jakarta.persistence.Entity class Example { @jakarta.persistence.Enumerated Value value; }";
        let valid = "package shop;\nimport jakarta.persistence.Entity;\n@Entity class Value {}";
        plugin.diagnostics(&doc("/project/Value.java", valid));
        assert!(has(&plugin.diagnostics(&doc("/project/Example.java", owner)), "attribute-type"));
        plugin.diagnostics(&doc("/project/Value.java", &valid.replace("import jakarta.persistence.Entity;", "import app.*;")));
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", owner)), "attribute-type"));
        let mapping = "@jakarta.persistence.Entity class Example { @jakarta.persistence.Id long id; @jakarta.persistence.JoinColumn long getValue() {return 0;} }";
        assert!(has(&plugin.diagnostics(&doc("/project/Example.java", mapping)), "jpa-mixed-access"));
        for unknown in [
            mapping.replace("@jakarta.persistence.JoinColumn", "@app.JoinColumn"),
            mapping.replace("@jakarta.persistence.JoinColumn", "@javax.persistence.JoinColumn"),
            "import jakarta.persistence.Entity;\nimport other.Entity;\n@Entity public final class Example {}".into(),
            "import jakarta.persistence.Entity;\n@Entity public final class Example<Entity> {}".into(),
            "@jakarta.persistence.Entity public final class jakarta {}".into(),
        ] {
            assert!(plugin.diagnostics(&doc("/project/Example.java", &unknown)).is_empty(), "{unknown}");
        }
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn foreign_kotlin_collection_names_do_not_borrow_read_only_source_api() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        for ty in ["vendor.List", "vendor.Set", "vendor.Map", "vendor.Collection", "vendor.Iterable"] {
            let source = format!("@jakarta.persistence.Entity\nclass Example {{ @jakarta.persistence.OneToMany var values: {ty}<Value> = TODO() }}");
            assert!(!has(&plugin.diagnostics(&doc("/project/Example.kt", &source)), "jpa-immutable-collection"), "{source}");
        }
    }

    #[test]
    fn owned_kotlin_collection_hint_is_source_only_and_tracks_current_text_ranges() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = "@jakarta.persistence.Entity\nclass Example { /* 🦉 */ @jakarta.persistence.OneToMany var values: kotlin.collections.List<Value> = TODO() }";
        let mut d = doc("/project/Example.kt", source);
        let found = plugin.diagnostics(&d);
        let finding = found.iter().find(|d|d.code.as_deref()==Some("jpa-immutable-collection")).unwrap();
        let a = openidea_plugin::text::offset(&d.text, finding.range.start).unwrap();
        let b = openidea_plugin::text::offset(&d.text, finding.range.end).unwrap();
        assert_eq!(&d.text[a..b], "kotlin.collections.List");
        assert_eq!(finding.severity, openidea_plugin::Severity::Info);
        assert!(finding.message.contains("source-declared") && finding.message.contains("read-only operations") && !finding.message.contains("provider"));
        d.text = source.replace("kotlin.collections.List", "vendor.List"); d.version += 1;
        assert!(!has(&plugin.diagnostics(&d), "jpa-immutable-collection"));
        let imported = "import kotlin.collections.List\n@jakarta.persistence.Entity\nclass Example { @jakarta.persistence.OneToMany var values: List<Value> = TODO() }";
        assert!(has(&plugin.diagnostics(&doc("/project/Example.kt", imported)), "jpa-immutable-collection"));
        let constructor = "@jakarta.persistence.Entity\nclass Example(@jakarta.persistence.OneToMany var values: kotlin.collections.List<Value>) {}";
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.kt", constructor)), "jpa-immutable-collection"), "primary constructor context remains unsupported");
        for unknown in [
            imported.replace("import kotlin.collections.List\n", ""),
            imported.replace("import kotlin.collections.List", "import kotlin.collections.*"),
            imported.replace("import kotlin.collections.List", "import vendor.List"),
            imported.replace("class Example", "class Example<List>"),
            imported.replace("class Example", "class Example : Base()"),
            imported.replace("TODO()", "TODO(); class List {}"),
            source.replace("class Example", "class kotlin"),
            source.replace("class Example", "class Example<T>"),
            source.replace("kotlin.collections.List", "kotlin.collections.MutableList"),
        ] {
            assert!(!has(&plugin.diagnostics(&doc("/project/Example.kt", &unknown)), "jpa-immutable-collection"), "{unknown}");
        }
        host.set_setting("jpaAssist.checkKotlinEntities", "false");
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.kt", source)), "jpa-immutable-collection"));
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn explicit_native_sql_comment_colons_are_not_method_parameters() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        for sql in ["select * from orders /* :ignored */ where id = :id", "select * from orders where id = :id -- :ignored"] {
            let source = format!("interface R {{ @org.springframework.data.jpa.repository.Query(value=\"{sql}\", nativeQuery=true) Object read(@org.springframework.data.repository.query.Param(\"id\") long id); }}");
            assert!(!has(&plugin.diagnostics(&doc("/project/R.java", &source)), "jpa-unknown-query-parameter"), "{source}");
        }
    }

    #[test]
    fn native_comment_ranges_current_text_and_unknown_sql_shapes_are_source_safe() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let wrap = |sql: &str| format!("interface R {{ @org.springframework.data.jpa.repository.Query(value=\"{sql}\", nativeQuery=true) Object read(@org.springframework.data.repository.query.Param(\"id\") long id); }}");
        let sql = "select 'a''b:quoted' /* 🦉 :ignored */ from orders where id = :missing";
        let source = wrap(sql);
        let mut d = doc("/project/R.java", &source);
        let found: Vec<_> = plugin.diagnostics(&d).into_iter().filter(|d|d.code.as_deref()==Some("jpa-unknown-query-parameter")).collect();
        assert_eq!(found.len(), 1);
        let a = openidea_plugin::text::offset(&d.text, found[0].range.start).unwrap();
        let b = openidea_plugin::text::offset(&d.text, found[0].range.end).unwrap();
        assert_eq!(&d.text[a..b], ":missing");
        d.text = source.replace(":missing", ":id"); d.version += 1;
        assert!(!has(&plugin.diagnostics(&d), "jpa-unknown-query-parameter"));
        for sql in [
            "select :missing /* unclosed", "select :missing /* outer /* inner */ end */",
            "select :missing from $tag$body$tag$", "select :missing from `table`", "select :missing from [table]",
            "select :missing where x = :#{#value}", "select :missing where x = 'unclosed",
            "select :missing /*+ :hint */", "select :missing /*! :executable */", "select :missing \\n from orders",
        ] {
            assert!(!has(&plugin.diagnostics(&doc("/project/R.java", &wrap(sql))), "jpa-unknown-query-parameter"), "{sql}");
        }
        assert!(!has(&plugin.diagnostics(&doc("/project/R.java", &source.replace("nativeQuery=true", "nativeQuery=FLAG"))), "jpa-unknown-query-parameter"));
        let jpql = "interface R { @org.springframework.data.jpa.repository.Query(\"select e from E e where e.id = :missing\") Object read(); }";
        assert!(has(&plugin.diagnostics(&doc("/project/R.java", jpql)), "jpa-unknown-query-parameter"));
        host.set_setting("jpaAssist.checkQueryReferences", "false");
        assert!(!has(&plugin.diagnostics(&doc("/project/R.java", &source)), "jpa-unknown-query-parameter"));
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn dynamic_query_expression_tokens_are_not_literal_method_bindings() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = "interface R { @org.springframework.data.jpa.repository.Query(\"select e from E e where e.name = :#{#value?:T(java.lang.String).valueOf(1)}\") Object read(@org.springframework.data.repository.query.Param(\"value\") String value); }";
        assert!(!has(&plugin.diagnostics(&doc("/project/R.java", source)), "jpa-unknown-query-parameter"));
    }

    #[test]
    fn dynamic_query_current_text_correction_restores_literal_ranges_only() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = "/* 🦉 */ interface R { @org.springframework.data.jpa.repository.Query(\"select e from #{#entityName} e where e.id = :missing\") Object read(); }";
        let mut d = doc("/project/R.java", source);
        assert!(!has(&plugin.diagnostics(&d), "jpa-unknown-query-parameter"));
        d.text = source.replace("#{#entityName}", "Person"); d.version += 1;
        let findings = plugin.diagnostics(&d);
        let finding = findings.iter().find(|d|d.code.as_deref()==Some("jpa-unknown-query-parameter")).unwrap();
        let a = openidea_plugin::text::offset(&d.text, finding.range.start).unwrap();
        let b = openidea_plugin::text::offset(&d.text, finding.range.end).unwrap();
        assert_eq!(&d.text[a..b], ":missing");
        for query in [
            "select e from #{#entityName} e where e.id = :missing",
            "select e from ${entity.name} e where e.id = :missing",
            "select e from E e where e.application = ?${spring.application.name:unknown}",
            "select e from #{unclosed e where e.id = :missing",
            "select e from E e where e.id = :#{#value?:T(Integer).valueOf(1)}",
        ] {
            let source = format!("interface R {{ @org.springframework.data.jpa.repository.Query(\"{query}\") Object read(); }}");
            assert!(!has(&plugin.diagnostics(&doc("/project/R.java", &source)), "jpa-unknown-query-parameter"), "{source}");
        }
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn inherited_or_generated_setters_are_unknown_to_source_absence_checks() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let getter = "@jakarta.persistence.Id public long getId() {return id;}";
        for source in [
            format!("@jakarta.persistence.Entity class Example extends LibraryBase {{ long id; {getter} }}"),
            format!("@jakarta.persistence.Entity class Example implements LibraryInterface {{ long id; {getter} }}"),
            format!("@jakarta.persistence.Entity @lombok.Setter class Example {{ long id; {getter} }}"),
            format!("@jakarta.persistence.Entity @app.GenerateSetter class Example {{ long id; {getter} }}"),
            format!("@jakarta.persistence.MappedSuperclass abstract class Example {{ long id; {getter} }}"),
        ] {
            assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", &source)), "jpa-missing-setter"), "{source}");
        }
    }

    #[test]
    fn plain_source_setter_absence_has_exact_ranges_and_unsaved_addition_clears_it() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = "@jakarta.persistence.Entity class Example { long id; /* 🦉 */ @jakarta.persistence.Id public long getId() { return id; } }";
        let mut d = doc("/project/Example.java", source);
        let found: Vec<_> = plugin.diagnostics(&d).into_iter().filter(|d|d.code.as_deref()==Some("jpa-missing-setter")).collect();
        assert_eq!(found.len(),1);
        let a = openidea_plugin::text::offset(&d.text, found[0].range.start).unwrap();
        let b = openidea_plugin::text::offset(&d.text, found[0].range.end).unwrap();
        assert_eq!(&d.text[a..b], "@jakarta.persistence.Id");
        assert_eq!(found[0].severity, openidea_plugin::Severity::Info);
        assert!(found[0].message.contains("Current source") && found[0].message.contains("setId()") && found[0].message.contains("unresolved"));
        d.text = source.replace("return id; }", "return id; } public void setId(long value) { id = value; }"); d.version += 1;
        assert!(!has(&plugin.diagnostics(&d), "jpa-missing-setter"));
        for unknown in [
            source.replace("class Example", "abstract class Example"),
            source.replace("class Example", "class Example<T>"),
            source.replace("long id;", "long id; class Other {}"),
            source.replace("@jakarta.persistence.Entity", "@jakarta.persistence.Entity @lombok.Data"),
            source.replace("@jakarta.persistence.Entity", "@jakarta.persistence.Entity @app.Marker"),
        ] {
            assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", &unknown)), "jpa-missing-setter"), "{unknown}");
        }
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn literal_class_property_access_retains_source_setter_absence_notes() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = "@jakarta.persistence.Entity\n@jakarta.persistence.Access(jakarta.persistence.AccessType.PROPERTY)\nclass Example { long id; @jakarta.persistence.Id public long getId() {return id;} }";
        assert!(has(&plugin.diagnostics(&doc("/project/Example.java", source)), "jpa-missing-setter"));
    }

    #[test]
    fn explicit_property_setter_current_text_ranges_and_unknown_access_roles() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let access = "@jakarta.persistence.Access(jakarta.persistence.AccessType.PROPERTY)";
        let source = format!("@jakarta.persistence.Entity\n{access}\nclass Example {{ long id; /* 🦉 */ @jakarta.persistence.Id public long getId() {{return id;}} }}");
        let mut d = doc("/project/Example.java", &source);
        let found: Vec<_> = plugin.diagnostics(&d).into_iter().filter(|d|d.code.as_deref()==Some("jpa-missing-setter")).collect();
        assert_eq!(found.len(),1);
        let a = openidea_plugin::text::offset(&d.text, found[0].range.start).unwrap();
        let b = openidea_plugin::text::offset(&d.text, found[0].range.end).unwrap();
        assert_eq!(&d.text[a..b], "@jakarta.persistence.Id");
        d.text = source.replace("AccessType.PROPERTY", "AccessType.FIELD"); d.version += 1;
        assert!(!has(&plugin.diagnostics(&d), "jpa-missing-setter"));
        for supported in [
            source.replace("Access(jakarta.persistence.AccessType.PROPERTY)", "Access(value = jakarta.persistence.AccessType.PROPERTY)"),
            source.replace("jakarta.persistence", "javax.persistence"),
            format!("import jakarta.persistence.Access;\n{}", source.replace(access, "@Access(jakarta.persistence.AccessType.PROPERTY)")),
        ] { assert!(has(&plugin.diagnostics(&doc("/project/Example.java", &supported)), "jpa-missing-setter"), "{supported}"); }
        for unknown in [
            source.replace("jakarta.persistence.AccessType.PROPERTY", "AccessType.PROPERTY"),
            source.replace("jakarta.persistence.AccessType.PROPERTY", "javax.persistence.AccessType.PROPERTY"),
            source.replace("jakarta.persistence.AccessType.PROPERTY", "ACCESS_MODE"),
            source.replace("jakarta.persistence.AccessType.PROPERTY", "jakarta.persistence.AccessType.PROPERTY, unknown=true"),
            source.replace("@jakarta.persistence.Id", &format!("{access} @jakarta.persistence.Id")),
            source.replace(&format!("{access}\n"), "").replace("@jakarta.persistence.Id", &format!("{access} @jakarta.persistence.Id")),
            source.replace("class Example", "class Example extends Base"),
            source.replace("@jakarta.persistence.Entity", "@jakarta.persistence.Entity @lombok.Setter"),
        ] { assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", &unknown)), "jpa-missing-setter"), "{unknown}"); }
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn setter_notes_require_a_mapping_on_a_supported_instance_getter_declaration() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let transient = "@jakarta.persistence.Entity public class Example { @jakarta.persistence.Id public long getId() {return 0;} public void setId(long value) {} @jakarta.persistence.Transient public long getValue() { Object helper = new Object() { @jakarta.persistence.Id long nested; }; return 0; } }";
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", transient)), "jpa-missing-setter"));
        for member in [
            "public long getValue() { @jakarta.persistence.Id long local = 0; return local; }",
            "@jakarta.persistence.Id public static long getId() {return 0;}",
            "@jakarta.persistence.Id public void getId() {}",
            "@jakarta.persistence.Id private long getId() {return 0;}",
            "@jakarta.persistence.Id public long isId() {return 0;}",
            "@jakarta.persistence.Transient @jakarta.persistence.Id public long getId() {return 0;}",
        ] {
            let source = format!("@jakarta.persistence.Entity class Example {{ {member} }}");
            assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", &source)), "jpa-missing-setter"), "{source}");
        }
    }

    #[test]
    fn getter_declaration_role_current_text_correction_restores_exact_source_note() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = "@jakarta.persistence.Entity class Example { /* 🦉 */ @jakarta.persistence.Id public static long getId() {return 0;} }";
        let mut d = doc("/project/Example.java", source);
        assert!(!has(&plugin.diagnostics(&d), "jpa-missing-setter"));
        d.text = source.replace("public static long", "protected long"); d.version += 1;
        let found: Vec<_> = plugin.diagnostics(&d).into_iter().filter(|d|d.code.as_deref()==Some("jpa-missing-setter")).collect();
        assert_eq!(found.len(),1);
        let a = openidea_plugin::text::offset(&d.text, found[0].range.start).unwrap();
        let b = openidea_plugin::text::offset(&d.text, found[0].range.end).unwrap();
        assert_eq!(&d.text[a..b], "@jakarta.persistence.Id");
        for unknown in [
            d.text.trim_end_matches('}').to_owned(),
            d.text.replace("protected long", "long"),
            d.text.replace("protected long getId()", "public <T> T getId()"),
            d.text.replace("protected long getId()", "public long getId(long value)"),
            d.text.replace("@jakarta.persistence.Id protected long getId() {return 0;}", "public long getValue() { @jakarta.persistence.Id long local = 0; return local; }"),
        ] {
            assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", &unknown)), "jpa-missing-setter"), "{unknown}");
        }
        let boolean = "@jakarta.persistence.Entity class Example { @jakarta.persistence.Id public boolean isActive() {return true;} }";
        assert!(has(&plugin.diagnostics(&doc("/project/Example.java", boolean)), "jpa-missing-setter"));
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn nested_and_local_class_roles_do_not_become_top_level_positive_type_facts() {
        let _host = MockHost::install();
        for (path, source) in [
            ("/project/Outer.java", "package shop;\nclass Outer { @jakarta.persistence.Entity static class Value {} }"),
            ("/project/Outer.kt", "package shop\nfun make() { @jakarta.persistence.Entity class Value {} }"),
        ] {
            let mut plugin = JpaAssist::default();
            plugin.diagnostics(&doc(path, source));
            let current = "package shop;\n@jakarta.persistence.Entity class Example { @jakarta.persistence.Enumerated Value value; }";
            assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", current)), "attribute-type"), "{source}");
        }
    }

    #[test]
    fn source_type_scope_current_text_replacement_invalidates_wrong_qualified_identity() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let value = "package shop;\n/* 🦉 */ @jakarta.persistence.Entity class Value { static class Helper {} }";
        let owner = "package shop;\n@jakarta.persistence.Entity class Example { /* 🦉 */ @jakarta.persistence.Enumerated Value value; }";
        let mut d = doc("/project/Value.java", value);
        plugin.diagnostics(&d);
        let found: Vec<_> = plugin.diagnostics(&doc("/project/Example.java", owner)).into_iter().filter(|d|d.code.as_deref()==Some("attribute-type")).collect();
        assert_eq!(found.len(),1, "top-level class with an unrelated nested helper remains a valid source type identity");
        let a = openidea_plugin::text::offset(owner, found[0].range.start).unwrap();
        let b = openidea_plugin::text::offset(owner, found[0].range.end).unwrap();
        assert_eq!(&owner[a..b], "@jakarta.persistence.Enumerated");
        d.text = "package shop;\nclass Outer { @jakarta.persistence.Entity static class Value {} }".into(); d.version += 1;
        plugin.diagnostics(&d);
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", owner)), "attribute-type"));
        plugin.diagnostics(&doc("/project/../other/Value.java", value));
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", owner)), "attribute-type"));
        for (package, name) in [
            ("shop".to_owned(), "123Value".to_owned()),
            ("shop".to_owned(), "V".repeat(129)),
            ("p".repeat(513), "Value".to_owned()),
            ("shop.bad-name".to_owned(), "Value".to_owned()),
        ] {
            let source = format!("package {package};\n@jakarta.persistence.Entity class {name} {{}}");
            let current = format!("package {package};\n@jakarta.persistence.Entity class Example {{ @jakarta.persistence.Enumerated {name} value; }}");
            plugin.diagnostics(&doc("/project/Value.java", &source));
            assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", &current)), "attribute-type"));
        }
        plugin.diagnostics(&doc("/project/Value.java", "package shop;\n@jakarta.persistence.Entity αclass Value {}"));
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", owner)), "attribute-type"));
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn cached_test_source_roles_do_not_supply_main_source_type_meaning() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        plugin.diagnostics(&doc("/project/src/test/java/shop/Value.java", "package shop;\n@jakarta.persistence.Entity class Value {}"));
        plugin.diagnostics(&doc("/project/src/main/java/shop/Value.java", "package shop;\nenum Value { A }"));
        let current = "package shop;\n@jakarta.persistence.Entity class Example { @jakarta.persistence.Enumerated Value value; }";
        assert!(!has(&plugin.diagnostics(&doc("/project/src/main/java/shop/Example.java", current)), "attribute-type"));
    }

    #[test]
    fn source_group_matches_preserve_positive_facts_and_current_scope_replacement() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let entity = "package shop;\n@jakarta.persistence.Entity class Value { @jakarta.persistence.Column(name=\"unused_metadata\") long id; }";
        let owner = "package shop;\n@jakarta.persistence.Entity class Example { /* 🦉 */ @jakarta.persistence.Enumerated Value value; }";
        plugin.diagnostics(&doc("/project/src/test/java/shop/Value.java", entity));
        let mut value = doc("/project/src/main/java/shop/Value.java", entity);
        plugin.diagnostics(&value);
        let findings = plugin.diagnostics(&doc("/project/src/main/java/shop/Example.java", owner));
        let finding = findings.iter().find(|d|d.code.as_deref()==Some("attribute-type")).unwrap();
        assert_eq!(finding.severity, openidea_plugin::Severity::Info);
        assert!(finding.message.contains("supplied source class") && finding.message.contains("unresolved"));
        let a = openidea_plugin::text::offset(owner, finding.range.start).unwrap();
        let b = openidea_plugin::text::offset(owner, finding.range.end).unwrap();
        assert_eq!(&owner[a..b], "@jakarta.persistence.Enumerated");
        value.text = "package shop;\nenum Value { A }".into(); value.version += 1;
        plugin.diagnostics(&value);
        assert!(!has(&plugin.diagnostics(&doc("/project/src/main/java/shop/Example.java", owner)), "attribute-type"), "test scope must not backfill the removed main fact");
        plugin.diagnostics(&doc("/project/module-b/src/main/java/shop/Value.java", entity));
        assert!(!has(&plugin.diagnostics(&doc("/project/src/main/java/shop/Example.java", owner)), "attribute-type"));
        plugin.diagnostics(&doc("/project/flat/Value.java", entity));
        assert!(has(&plugin.diagnostics(&doc("/project/flat/Example.java", owner)), "attribute-type"));
        assert!(!has(&plugin.diagnostics(&doc("/project/other-flat/Example.java", owner)), "attribute-type"));
        let large = entity.replace("unused_metadata", &"x".repeat(64*1024));
        plugin.diagnostics(&doc("/project/flat/Value.java", &large));
        assert!(has(&plugin.diagnostics(&doc("/project/flat/Example.java", owner)), "attribute-type"), "unused mapping payload is not needed for qualified source-class identity");
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn anonymous_helper_mappings_do_not_belong_to_outer_entity_attributes() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        for source in [
            "@jakarta.persistence.Entity public class Example { @jakarta.persistence.Id long id; @jakarta.persistence.Transient Object helper = new Object() { @jakarta.persistence.EmbeddedId Object embedded; }; }",
            "@jakarta.persistence.Entity public class Example { @jakarta.persistence.Id long id; @jakarta.persistence.Transient Object helper = new Object() { @jakarta.persistence.ManyToOne Object first; @jakarta.persistence.OneToOne Object second; }; }",
            "@jakarta.persistence.Entity public class Example { @jakarta.persistence.Id public long getId() {return 0;} public void setId(long id) {} @jakarta.persistence.Transient Object helper = new Object() { @jakarta.persistence.Column long nested; }; }",
        ] {
            let findings = plugin.diagnostics(&doc("/project/Example.java", source));
            assert!(findings.iter().all(|d| !matches!(d.code.as_deref(), Some("jpa-multiple-id-kinds" | "jpa-attribute-multiple-relations" | "jpa-mixed-access"))), "{source}");
        }
    }

    #[test]
    fn direct_attribute_conflicts_keep_ranges_and_unsaved_scope_moves_clear_them() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = "@jakarta.persistence.Entity public class Example { @jakarta.persistence.Id long id; /* 🦉 */ @jakarta.persistence.EmbeddedId Object embedded; }";
        let mut d = doc("/project/Example.java", source);
        let found: Vec<_> = plugin.diagnostics(&d).into_iter().filter(|d|d.code.as_deref()==Some("jpa-multiple-id-kinds")).collect();
        assert_eq!(found.len(),1);
        let a = openidea_plugin::text::offset(&d.text, found[0].range.start).unwrap();
        let b = openidea_plugin::text::offset(&d.text, found[0].range.end).unwrap();
        assert_eq!(&d.text[a..b], "@jakarta.persistence.EmbeddedId");
        d.text = source.replace("@jakarta.persistence.EmbeddedId Object embedded;", "@jakarta.persistence.Transient Object helper = new Object() { @jakarta.persistence.EmbeddedId Object embedded; };"); d.version += 1;
        assert!(!has(&plugin.diagnostics(&d), "jpa-multiple-id-kinds"));
        for moved in [
            source.replace("@jakarta.persistence.EmbeddedId Object embedded;", "static class Helper { @jakarta.persistence.EmbeddedId Object embedded; }"),
            source.replace("@jakarta.persistence.EmbeddedId Object embedded;", "@app.NestedId(@jakarta.persistence.EmbeddedId) Object helper;"),
        ] {
            assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", &moved)), "jpa-multiple-id-kinds"), "{moved}");
        }
        let relations = "@jakarta.persistence.Entity class Example { @jakarta.persistence.ManyToOne /* 🦉 */ @jakarta.persistence.OneToOne Object value; }";
        assert!(has(&plugin.diagnostics(&doc("/project/Example.java", relations)), "jpa-attribute-multiple-relations"));
        let oversized = source.replace("long id;", &format!("long id; {}", (0..1025).map(|i| format!("long field{i}; ")).collect::<String>()));
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", &oversized)), "jpa-multiple-id-kinds"));
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn live_entity_and_scalar_notes_do_not_borrow_nonpersistent_helper_annotations() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        for source in [
            "@jakarta.persistence.Entity public class Example { @jakarta.persistence.Id long id; @jakarta.persistence.Transient Object helper = new Object() { @jakarta.persistence.ManyToOne Object nested; }; }",
            "@jakarta.persistence.Entity public class Example { @jakarta.persistence.Id long id; @jakarta.persistence.Transient boolean flag = new Object() { @jakarta.persistence.Version java.lang.String stamp; boolean evaluate() {return true;} }.evaluate(); }",
            "@jakarta.persistence.Entity public class Example { @jakarta.persistence.Id long id; @lombok.Data static class Helper {} }",
        ] {
            let found = plugin.diagnostics(&doc("/project/Example.java", source));
            assert!(found.iter().all(|d|!matches!(d.code.as_deref(), Some("jpa-eager-to-one" | "jpa-lombok-data" | "attribute-type"))), "{source}");
        }
    }

    #[test]
    fn direct_entity_and_scalar_notes_keep_source_ranges_and_scope_corrections() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = "@jakarta.persistence.Entity public class Example { @jakarta.persistence.Id long id; /* 🦉 */ @jakarta.persistence.ManyToOne Object value; }";
        let mut d = doc("/project/Example.java", source);
        let found = plugin.diagnostics(&d);
        let finding = found.iter().find(|d|d.code.as_deref()==Some("jpa-eager-to-one")).unwrap();
        let a = openidea_plugin::text::offset(&d.text, finding.range.start).unwrap();
        let b = openidea_plugin::text::offset(&d.text, finding.range.end).unwrap();
        assert_eq!(&d.text[a..b], "@jakarta.persistence.ManyToOne");
        d.text = source.replace("@jakarta.persistence.ManyToOne Object value;", "@jakarta.persistence.Transient Object helper = new Object() { @jakarta.persistence.ManyToOne Object value; };"); d.version += 1;
        assert!(!has(&plugin.diagnostics(&d), "jpa-eager-to-one"));
        let scalar = "@jakarta.persistence.Entity class Example { /* 🦉 */ @jakarta.persistence.Version boolean version; }";
        let found = plugin.diagnostics(&doc("/project/Example.java", scalar));
        let finding = found.iter().find(|d|d.code.as_deref()==Some("attribute-type")).unwrap();
        let a = openidea_plugin::text::offset(scalar, finding.range.start).unwrap();
        let b = openidea_plugin::text::offset(scalar, finding.range.end).unwrap();
        assert_eq!(&scalar[a..b], "@jakarta.persistence.Version");
        assert_eq!(finding.severity, openidea_plugin::Severity::Warning);
        assert!(has(&plugin.diagnostics(&doc("/project/Example.java", "@jakarta.persistence.Entity @lombok.Data class Example {}")), "jpa-lombok-data"));
        for unknown in [
            "class Outer { @jakarta.persistence.Entity static class Example { @jakarta.persistence.ManyToOne Object nested; } }",
            "@jakarta.persistence.Entity @app.Note(@lombok.Data) class Example {}",
        ] {
            assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", unknown)), "jpa-eager-to-one"));
            assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", unknown)), "jpa-lombok-data"));
        }
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn owned_embedded_id_getter_establishes_source_property_intent() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = "@jakarta.persistence.Entity public class Example { Key id; @jakarta.persistence.EmbeddedId public Key getId() {return id;} }";
        assert!(has(&plugin.diagnostics(&doc("/project/Example.java", source)), "jpa-missing-setter"));
    }

    #[test]
    fn embedded_property_intent_ranges_current_text_and_source_access_labels() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = "@jakarta.persistence.Entity public class Example { Key id; /* 🦉 */ @jakarta.persistence.EmbeddedId public Key getId() {return id;} }";
        let mut d = doc("/project/Example.java", source);
        let found = plugin.diagnostics(&d);
        let finding = found.iter().find(|d|d.code.as_deref()==Some("jpa-missing-setter")).unwrap();
        let a = openidea_plugin::text::offset(&d.text, finding.range.start).unwrap();
        let b = openidea_plugin::text::offset(&d.text, finding.range.end).unwrap();
        assert_eq!(&d.text[a..b], "@jakarta.persistence.EmbeddedId");
        assert!(finding.message.contains("Current source") && finding.message.contains("unresolved"));
        d.text = source.replace("return id;}", "return id;} public void setId(Key value) {id = value;}"); d.version += 1;
        assert!(!has(&plugin.diagnostics(&d), "jpa-missing-setter"));
        let mixed = "@jakarta.persistence.Entity class Example { @jakarta.persistence.EmbeddedId Key id; /* 🦉 */ @jakarta.persistence.Column public String getName() {return null;} }";
        let found = plugin.diagnostics(&doc("/project/Example.java", mixed));
        let finding = found.iter().find(|d|d.code.as_deref()==Some("jpa-mixed-access")).unwrap();
        assert_eq!(finding.severity, openidea_plugin::Severity::Info);
        assert!(finding.message.contains("Current source") && finding.message.contains("unresolved") && !finding.message.contains("ignored"));
        for unknown in [
            source.replace("class Example", "class Example extends Base"),
            source.replace("@jakarta.persistence.Entity", "@jakarta.persistence.Entity @lombok.Setter"),
            source.replace("@jakarta.persistence.EmbeddedId", "@app.EmbeddedId"),
            source.replace("@jakarta.persistence.EmbeddedId public Key getId() {return id;}", "public Key getOther() { Object helper = new Object() { @jakarta.persistence.EmbeddedId Key id; }; return null; }"),
        ] { assert!(!has(&plugin.diagnostics(&doc("/project/Example.java", &unknown)), "jpa-missing-setter"), "{unknown}"); }
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn same_source_attached_static_listener_has_owned_constructor_role() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = "package shop;\n@jakarta.persistence.Entity\n@jakarta.persistence.EntityListeners(Owner.Audit.class)\npublic class Owner { @jakarta.persistence.Id long id; public static class Audit { private Audit() {} } }";
        let d = doc("/project/src/main/java/shop/Owner.java", source);
        assert!(has(&plugin.diagnostics(&d), "jpa-listener-constructor"));
    }

    #[test]
    fn nested_listener_constructor_current_text_ranges_and_unknown_contexts() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = "package shop;\n@jakarta.persistence.Entity\n@jakarta.persistence.EntityListeners(value = {Owner.Audit.class})\npublic class Owner { @jakarta.persistence.Id long id; public static class Audit { /*😀*/ protected Audit() {} } }";
        let path = "/project/src/main/java/shop/Owner.java";
        let findings = plugin.diagnostics(&doc(path, source));
        let finding = findings.iter().find(|d| d.code.as_deref() == Some("jpa-listener-constructor")).unwrap();
        let at = source.rfind("Audit()").unwrap();
        assert_eq!(openidea_plugin::text::offset(source, finding.range.start), Some(at));
        assert_eq!(openidea_plugin::text::offset(source, finding.range.end), Some(at + 5));
        assert!(finding.message.contains("unresolved"));
        for unknown in [
            source.replace("protected Audit()", "public Audit()"),
            source.replace("protected Audit()", "protected Audit(long id)"),
            source.replace("Owner.Audit.class", "Other.Audit.class"),
            source.replace("Owner.Audit.class", "chooseListener()"),
            source.replace("public static class Audit", "public class Audit"),
            source.replace("public static class Audit", "public static class Audit extends Base"),
            source.replace("class Owner {", "class Owner extends Base {"),
            source.replace("class Owner {", "class Owner<T> {"),
            source.replace("@jakarta.persistence.EntityListeners", "@app.EntityListeners"),
            source.replace("@jakarta.persistence.Entity", "@Entity"),
            source.replace("public static class Audit", "@lombok.NoArgsConstructor public static class Audit"),
            source.replace("protected Audit() {}", "protected Audit() {} static class Other {}"),
        ] { assert!(!has(&plugin.diagnostics(&doc(path, &unknown)), "jpa-listener-constructor"), "{unknown}"); }
        assert!(!has(&plugin.diagnostics(&doc("/project/Owner.java", source)), "jpa-listener-constructor"));
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn attached_listener_callbacks_require_void_single_argument_and_instance_method() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = "package shop;\n@jakarta.persistence.Entity\n@jakarta.persistence.EntityListeners(Owner.Audit.class)\npublic class Owner { @jakarta.persistence.Id long id; public static class Audit { public Audit() {} @jakarta.persistence.PrePersist static void persist(Object value) {} @jakarta.persistence.PostLoad int loaded() { return 1; } } }";
        let findings = plugin.diagnostics(&doc("/project/src/main/java/shop/Owner.java", source));
        assert!(has(&findings, "jpa-callback-modifiers"));
        assert!(has(&findings, "entity-listener-signature"));
    }

    #[test]
    fn attached_listener_callback_ranges_corrections_settings_and_unknowns() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = "package shop;\n@jakarta.persistence.Entity\n@jakarta.persistence.EntityListeners(Owner.Audit.class)\npublic class Owner { @jakarta.persistence.Id long id; public static class Audit { /*😀*/ @jakarta.persistence.PrePersist final void persist(Object value) {} @jakarta.persistence.PostLoad void loaded(Object first, Owner second) {} } }";
        let path = "/project/src/main/java/shop/Owner.java";
        let is_callback = |d:&Diagnostic| matches!(d.code.as_deref(), Some("jpa-callback-modifiers" | "entity-listener-signature"));
        let found: Vec<_> = plugin.diagnostics(&doc(path, source)).into_iter().filter(&is_callback).collect();
        assert_eq!(found.len(), 2);
        let names:Vec<_> = found.iter().map(|d| &source[openidea_plugin::text::offset(source,d.range.start).unwrap()..openidea_plugin::text::offset(source,d.range.end).unwrap()]).collect();
        assert_eq!(names, ["persist", "loaded"]);
        let corrected = source.replace("final void", "private void").replace("Object first, Owner second", "LibraryType value");
        assert!(plugin.diagnostics(&doc(path,&corrected)).iter().all(|d| !is_callback(d)));
        for unknown in [
            source.replace("Owner.Audit.class", "External.class"),
            source.replace("static class Audit", "static class Audit extends Base"),
            source.replace("@jakarta.persistence.PrePersist", "@app.PrePersist").replace("@jakarta.persistence.PostLoad", "@app.PostLoad"),
            source.replace("@jakarta.persistence.PrePersist", "@PrePersist").replace("@jakarta.persistence.PostLoad", "@PostLoad"),
            source.replace("final void persist(Object value)", "final LibraryType persist(Object value)").replace("void loaded(Object first, Owner second)", "LibraryType loaded(Object first, Owner second)"),
            source.replace("final void persist(Object value)", "final void persist(java.util.List<Object> value)").replace("void loaded(Object first, Owner second)", "void loaded(Object... value)"),
            source.replace("@jakarta.persistence.PrePersist final void persist(Object value) {}", "void helper() { Object value = new Object() { @jakarta.persistence.PrePersist final void persist(Object value) {} }; }").replace("@jakarta.persistence.PostLoad void loaded(Object first, Owner second) {}", ""),
        ] { assert!(plugin.diagnostics(&doc(path,&unknown)).iter().all(|d| !is_callback(d)), "{unknown}"); }
        host.set_setting("jpaAssist.checkListenerSignatures", "false");
        assert!(plugin.diagnostics(&doc(path,source)).iter().all(|d| !is_callback(d)));
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn attached_listener_duplicate_events_require_supported_source_owner_parameter() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = "package shop;\n@jakarta.persistence.Entity\n@jakarta.persistence.EntityListeners(Owner.Audit.class)\npublic class Owner { @jakarta.persistence.Id long id; public static class Audit { @jakarta.persistence.PrePersist void first(Owner value) {} @jakarta.persistence.PrePersist void second(Owner value) {} } }";
        assert!(has(&plugin.diagnostics(&doc("/project/src/main/java/shop/Owner.java",source)), "jpa-duplicate-lifecycle-event"));
    }

    #[test]
    fn listener_event_ranges_current_changes_and_unknown_argument_roles() {
        let host = MockHost::install(); let mut plugin = JpaAssist::default();
        let source = "package shop;\n@jakarta.persistence.Entity\n@jakarta.persistence.EntityListeners(Owner.Audit.class)\npublic class Owner { @jakarta.persistence.Id long id; public static class Audit { @jakarta.persistence.PrePersist @jakarta.persistence.PostLoad void first(Owner value) {} /*😀*/ @jakarta.persistence.PrePersist void second(Owner value) {} } }";
        let path = "/project/src/main/java/shop/Owner.java";
        let found:Vec<_>=plugin.diagnostics(&doc(path,source)).into_iter().filter(|d|d.code.as_deref()==Some("jpa-duplicate-lifecycle-event")).collect();
        assert_eq!(found.len(),1); let at=source.rfind("second(").unwrap();
        assert_eq!(openidea_plugin::text::offset(source,found[0].range.start),Some(at));
        assert_eq!(openidea_plugin::text::offset(source,found[0].range.end),Some(at+6));
        for unknown in [
            source.replace("@jakarta.persistence.PrePersist void second", "void second"),
            source.replace("Owner value", "LibraryType value"),
            source.replace("Owner value", "Object value"),
            source.replace("void first", "static void first"),
            source.replace("void second", "final void second"),
            source.replace("Owner.Audit.class", "External.class"),
            source.replace("class Audit {", "class Audit extends Base {"),
            source.replace("@jakarta.persistence.PrePersist", "@foreign.PrePersist"),
            source.replace("@jakarta.persistence.PrePersist void second(Owner value) {}", "void helper() { Object value = new Object() { @jakarta.persistence.PrePersist void second(Owner value) {} }; }"),
        ] {assert!(!has(&plugin.diagnostics(&doc(path,&unknown)),"jpa-duplicate-lifecycle-event"),"{unknown}");}
        host.set_setting("jpaAssist.checkListenerSignatures","false");
        assert!(!has(&plugin.diagnostics(&doc(path,source)),"jpa-duplicate-lifecycle-event"));
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn live_fetch_notes_do_not_borrow_dynamic_or_foreign_enum_values() {
        let _host = MockHost::install(); let mut plugin=JpaAssist::default();
        let source="@jakarta.persistence.Entity class Example { @jakarta.persistence.ManyToOne(fetch = helper(EAGER)) Object value; }";
        let findings=plugin.diagnostics(&doc("/project/Example.java",source));
        assert!(!has(&findings,"jpa-eager-fetch") && !has(&findings,"jpa-eager-to-one"));
    }

    #[test]
    fn direct_fetch_enum_owner_keeps_ranges_and_current_unknowns_clear_notes() {
        let host=MockHost::install();let mut plugin=JpaAssist::default();
        let source="import jakarta.persistence.FetchType;\n@jakarta.persistence.Entity class Example { /*😀*/ @jakarta.persistence.ManyToOne(fetch = FetchType.EAGER) Object value; }";
        let path="/project/Example.java";
        let found=plugin.diagnostics(&doc(path,source));let f=found.iter().find(|d|d.code.as_deref()==Some("jpa-eager-fetch")).unwrap();
        let a=openidea_plugin::text::offset(source,f.range.start).unwrap();let b=openidea_plugin::text::offset(source,f.range.end).unwrap();
        assert_eq!(&source[a..b],"@jakarta.persistence.ManyToOne(fetch = FetchType.EAGER)");
        let qualified=source.replace("FetchType.EAGER","jakarta.persistence.FetchType.EAGER");
        assert!(has(&plugin.diagnostics(&doc(path,&qualified)),"jpa-eager-fetch"));
        for unknown in [
            source.replace("FetchType.EAGER","FetchType.LAZY"),
            source.replace("FetchType.EAGER","vendor.FetchType.EAGER"),
            source.replace("FetchType.EAGER","javax.persistence.FetchType.EAGER"),
            source.replace("FetchType.EAGER","EAGER"),
            source.replace("FetchType.EAGER","helper(EAGER)"),
            source.replace("import jakarta.persistence.FetchType;","import vendor.FetchType;"),
            source.replace("import jakarta.persistence.FetchType;","import jakarta.persistence.*;"),
            source.replace("Object value;","Object value; Object FetchType;"),
            qualified.replace("Object value;","Object value; Object jakarta;"),
            source.replace("fetch = FetchType.EAGER","fetch = FetchType.EAGER, fetch = FetchType.LAZY"),
        ] { let found=plugin.diagnostics(&doc(path,&unknown));assert!(!has(&found,"jpa-eager-fetch") && !has(&found,"jpa-eager-to-one"),"{unknown}"); }
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn class_signature_does_not_borrow_entity_annotation_argument_role() {
        let _host=MockHost::install();let mut plugin=JpaAssist::default();
        let source="@app.Container(@jakarta.persistence.Entity) interface Example {}";
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.java",source)),"jpa-entity-not-a-class"));
    }

    #[test]
    fn direct_class_roles_keep_ranges_and_nested_metadata_or_member_contexts_are_unknown() {
        let host=MockHost::install();let mut plugin=JpaAssist::default();
        let source="/*😀*/ @jakarta.persistence.Entity @jakarta.persistence.Embeddable public interface Example {}";
        let found=plugin.diagnostics(&doc("/project/Example.java",source));
        let role=found.iter().find(|d|d.code.as_deref()==Some("jpa-entity-multiple-roles")).unwrap();
        let a=openidea_plugin::text::offset(source,role.range.start).unwrap();let b=openidea_plugin::text::offset(source,role.range.end).unwrap();assert_eq!(&source[a..b],"@jakarta.persistence.Embeddable");
        let declaration=found.iter().find(|d|d.code.as_deref()==Some("jpa-entity-not-a-class")).unwrap();let a=openidea_plugin::text::offset(source,declaration.range.start).unwrap();let b=openidea_plugin::text::offset(source,declaration.range.end).unwrap();assert_eq!(&source[a..b],"interface");
        for unknown in [
            source.replace("@jakarta.persistence.Embeddable ","").replace("interface","class"),
            "@jakarta.persistence.Entity @app.Container(@jakarta.persistence.Embeddable) class Example {}".into(),
            "class Outer { @jakarta.persistence.Entity Object helper; interface Example {} }".into(),
            "class Outer { @jakarta.persistence.Entity void helper() {} interface Example {} }".into(),
            source.replace("@jakarta.persistence.Entity","@vendor.Entity"),
            source.replace("@jakarta.persistence.Entity","@Entity"),
            source.trim_end_matches('}').into(),
        ] {let found=plugin.diagnostics(&doc("/project/Example.java",&unknown));assert!(found.iter().all(|d|!matches!(d.code.as_deref(),Some("jpa-entity-multiple-roles"|"jpa-entity-not-a-class"|"jpa-entity-nested"))),"{unknown}");}
        host.set_setting("jpaAssist.checkClassSignature","false");assert!(plugin.diagnostics(&doc("/project/Example.java",source)).iter().all(|d|d.code.as_deref()!=Some("jpa-entity-not-a-class")));
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn fetch_enum_notes_do_not_assume_inherited_name_ownership() {
        let _host=MockHost::install();let mut plugin=JpaAssist::default();
        let source="import jakarta.persistence.FetchType;\n@jakarta.persistence.Entity class Example extends Base { @jakarta.persistence.ManyToOne(fetch = FetchType.EAGER) Object value; }";
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.java",source)),"jpa-eager-fetch"));
    }

    #[test]
    fn fetch_notes_and_exported_edits_require_plain_current_type_context() {
        use openidea_plugin::providers::Providers;
        let host=MockHost::install();let mut plugin=JpaAssist::default();
        let source="import jakarta.persistence.FetchType;\n@jakarta.persistence.Entity class Example { @jakarta.persistence.ManyToOne(fetch = FetchType.EAGER) Object value; }";
        let path="/project/Example.java";let selection=openidea_plugin::Range::new(openidea_plugin::Position::new(1,0),openidea_plugin::Position::new(1,200));
        assert!(has(&plugin.diagnostics(&doc(path,source)),"jpa-eager-fetch"));assert!(plugin.code_actions(&doc(path,source),selection).iter().any(|a|a.title=="Fetch lazily"));
        let default=source.replace("(fetch = FetchType.EAGER)","");assert!(has(&plugin.diagnostics(&doc(path,&default)),"jpa-eager-to-one"));
        for unknown in [
            source.replace("class Example {","class Example extends Base {"),
            source.replace("class Example {","class Example implements Marker {"),
            source.replace("class Example {","class Example<T> {"),
            source.replace("Object value;","Object value; class Helper {}"),
            default.replace("class Example {","class Example extends Base {"),
        ] {let d=doc(path,&unknown);let found=plugin.diagnostics(&d);assert!(!has(&found,"jpa-eager-fetch") && !has(&found,"jpa-eager-to-one"),"{unknown}");assert!(plugin.code_actions(&d,selection).iter().all(|a|a.title!="Fetch lazily"),"{unknown}");}
        let kotlin="@jakarta.persistence.Entity class Example { @jakarta.persistence.ManyToOne var value: Target? = null }";
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.kt",kotlin)),"jpa-eager-to-one"));
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn kotlin_val_note_cannot_borrow_nested_entity_metadata_role() {
        let _host=MockHost::install();let mut plugin=JpaAssist::default();
        let source="@app.Container(@jakarta.persistence.Entity) class Example { val title: kotlin.String = \"name\" }";
        assert!(!has(&plugin.diagnostics(&doc("/project/Example.kt",source)),"jpa-val-attribute"));
    }

    #[test]
    fn kotlin_val_source_hints_require_owned_scalar_type_and_direct_plain_entity() {
        let host=MockHost::install();let mut plugin=JpaAssist::default();
        let source="@jakarta.persistence.Entity class Example { /*😀*/ val title: kotlin.String = \"name\" }";
        let path="/project/Example.kt";let found=plugin.diagnostics(&doc(path,source));let f=found.iter().find(|d|d.code.as_deref()==Some("jpa-val-attribute")).unwrap();
        assert_eq!(f.severity,openidea_plugin::Severity::Info);assert!(f.message.contains("unresolved") && !f.message.contains("cannot set"));
        let a=openidea_plugin::text::offset(source,f.range.start).unwrap();let b=openidea_plugin::text::offset(source,f.range.end).unwrap();assert_eq!(&source[a..b],"val");
        let imported=format!("import kotlin.String\n{}",source.replace("kotlin.String","String"));assert!(has(&plugin.diagnostics(&doc(path,&imported)),"jpa-val-attribute"));
        for unknown in [
            source.replace("val title","var title"),
            source.replace("kotlin.String","LibraryType"),
            source.replace("kotlin.String","String"),
            source.replace("kotlin.String","vendor.String"),
            format!("import vendor.kotlin\n{source}"),
            source.replace("class Example {","class Example : Base() {"),
            source.replace("class Example {","class Example<T> {"),
            source.replace("class Example {","@lombok.Data class Example {"),
            source.replace("@jakarta.persistence.Entity","@jakarta.persistence.NamedEntityGraph(@jakarta.persistence.Entity)"),
            source.replace("@jakarta.persistence.Entity","@Entity"),
            source.replace("val title","@jakarta.persistence.Transient val title"),
        ] {assert!(!has(&plugin.diagnostics(&doc(path,&unknown)),"jpa-val-attribute"),"{unknown}");}
        host.set_setting("jpaAssist.checkKotlinEntities","false");assert!(!has(&plugin.diagnostics(&doc(path,source)),"jpa-val-attribute"));
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn owned_attribute_checks_cannot_borrow_annotation_value_class_role() {
        let _host=MockHost::install();let mut plugin=JpaAssist::default();
        let source="@app.Container(@jakarta.persistence.Entity) class Example { @jakarta.persistence.Version boolean stamp; @jakarta.persistence.ManyToOne @jakarta.persistence.OneToOne Object value; }";
        let found=plugin.diagnostics(&doc("/project/Example.java",source));
        assert!(!has(&found,"attribute-type") && !has(&found,"jpa-attribute-multiple-relations"));
    }

    #[test]
    fn direct_attribute_class_roles_keep_exact_ranges_and_current_unknowns_clear() {
        let host=MockHost::install();let mut plugin=JpaAssist::default();
        let source="/*😀*/ @jakarta.persistence.Entity class Example { @jakarta.persistence.Version boolean stamp; @jakarta.persistence.ManyToOne @jakarta.persistence.OneToOne Object value; }";
        let path="/project/Example.java";
        for role in ["Entity","MappedSuperclass","Embeddable"] {let text=source.replace("persistence.Entity",&format!("persistence.{role}"));let found=plugin.diagnostics(&doc(path,&text));assert!(has(&found,"attribute-type") && has(&found,"jpa-attribute-multiple-relations"),"{text}");}
        let found=plugin.diagnostics(&doc(path,source));let f=found.iter().find(|d|d.code.as_deref()==Some("attribute-type")).unwrap();let a=openidea_plugin::text::offset(source,f.range.start).unwrap();let b=openidea_plugin::text::offset(source,f.range.end).unwrap();assert_eq!(&source[a..b],"@jakarta.persistence.Version");
        for unknown in [
            source.replace("@jakarta.persistence.Entity","@jakarta.persistence.NamedEntityGraph(@jakarta.persistence.Entity)"),
            source.replace("@jakarta.persistence.Entity","@vendor.Entity"),
            source.replace("@jakarta.persistence.Entity","@Entity"),
            source.replace("@jakarta.persistence.Entity","@jakarta.persistence.Entity @jakarta.persistence.Embeddable"),
            format!("void wrapper() {{ {source} }}"),
            source.trim_end_matches('}').into(),
        ] {let found=plugin.diagnostics(&doc(path,&unknown));assert!(!has(&found,"attribute-type") && !has(&found,"jpa-attribute-multiple-relations"),"{unknown}");}
        host.set_setting("jpaAssist.checkAttributeTypes","false");host.set_setting("jpaAssist.checkAttributeSignatures","false");let found=plugin.diagnostics(&doc(path,source));assert!(!has(&found,"attribute-type") && !has(&found,"jpa-attribute-multiple-relations"));
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn owned_embeddable_identifiers_are_live_and_current_source_scoped() {
        let host=MockHost::install(); let mut plugin=JpaAssist::default();
        let path="/project/Key.java";
        let source="/* 🦉 */ @jakarta.persistence.Embeddable class Key { @jakarta.persistence.Id long id; }";
        let found=plugin.diagnostics(&doc(path,source));
        let f=found.iter().find(|d|d.code.as_deref()==Some("jpa-embeddable-identifier")).unwrap();
        let a=openidea_plugin::text::offset(source,f.range.start).unwrap(); let b=openidea_plugin::text::offset(source,f.range.end).unwrap();
        assert_eq!(&source[a..b],"@jakarta.persistence.Id");
        for unknown in [source.replace("persistence.Embeddable","persistence.Entity"),source.replace("persistence.Embeddable","vendor.Embeddable"),source.replace("@jakarta.persistence.Id", ""),source.replace("class Key {","class Key { void helper() {"),source.replace("@jakarta.persistence.Id long id;","@jakarta.persistence.NamedAttributeNode(value=@jakarta.persistence.Id) long id;")] {
            assert!(!has(&plugin.diagnostics(&doc(path,&unknown)),"jpa-embeddable-identifier"),"{unknown}");
        }
        host.set_setting("jpaAssist.checkAttributeSignatures","false");assert!(!has(&plugin.diagnostics(&doc(path,source)),"jpa-embeddable-identifier"));
    }

}
