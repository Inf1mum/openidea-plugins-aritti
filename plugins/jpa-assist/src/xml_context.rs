//! Namespace-owned XML 1.0 descriptor contexts, not an effective persistence model.
use quick_xml::{events::Event, name::ResolveResult, reader::NsReader};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};
const MAX_TEXT: usize = 512 * 1024;
const PERSISTENCE: &[&str] =
    &["https://jakarta.ee/xml/ns/persistence", "http://xmlns.jcp.org/xml/ns/persistence", "http://java.sun.com/xml/ns/persistence"];
const ORM: &[&str] = &[
    "https://jakarta.ee/xml/ns/persistence/orm",
    "http://xmlns.jcp.org/xml/ns/persistence/orm",
    "http://java.sun.com/xml/ns/persistence/orm",
];
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Persistence,
    Orm,
}
struct Open {
    name: String,
    local: String,
    content: usize,
    children: bool,
}
pub(crate) struct Model {
    pub(crate) elements: BTreeSet<usize>,
    classes: Vec<(usize, usize)>,
    properties: Vec<(usize, usize)>,
    kind: Kind,
    prefix: &'static str,
    open: Vec<Open>,
}
pub(crate) enum Completion {
    Class { content: usize },
    Property { tag: usize, prefix: &'static str },
}
fn namespaces<R>(reader: &NsReader<R>, e: &quick_xml::events::BytesStart<'_>) -> bool {
    const XML: &str = "http://www.w3.org/XML/1998/namespace";
    const XMLNS: &str = "http://www.w3.org/2000/xmlns/";
    if e.name().as_ref().starts_with("xmlns:") || matches!(reader.resolver().resolve_element(e.name()).0, ResolveResult::Unknown(_)) {
        return false;
    }
    for a in e.attributes() {
        let Ok(a) = a else {
            return false;
        };
        let key = a.key.as_ref();
        if key == "xmlns" || key.starts_with("xmlns:") {
            let Ok(value) = a.normalized_value(quick_xml::XmlVersion::Implicit1_0) else {
                return false;
            };
            if key == "xmlns:xmlns"
                || key.starts_with("xmlns:") && value.is_empty()
                || key == "xmlns:xml" && value != XML
                || key != "xmlns:xml" && matches!(value.as_ref(), XML | XMLNS)
            {
                return false;
            }
        } else if matches!(reader.resolver().resolve_attribute(a.key).0, ResolveResult::Unknown(_)) {
            return false;
        }
    }
    true
}
fn is_path(stack: &[Open], path: &[&str]) -> bool {
    stack.iter().map(|o| o.local.as_str()).eq(path.iter().copied())
}
fn owned(kind: Kind, stack: &[Open], local: &str) -> bool {
    match kind {
        Kind::Persistence => {
            stack.is_empty()
                || is_path(stack, &["persistence"]) && local == "persistence-unit"
                || is_path(stack, &["persistence", "persistence-unit"])
                    && matches!(
                        local,
                        "provider" | "class" | "exclude-unlisted-classes" | "shared-cache-mode" | "validation-mode" | "properties"
                    )
                || is_path(stack, &["persistence", "persistence-unit", "properties"]) && local == "property"
        }
        Kind::Orm => {
            stack.is_empty()
                || is_path(stack, &["entity-mappings"]) && matches!(local, "package" | "entity" | "mapped-superclass" | "embeddable")
        }
    }
}
fn parse(text: &str, partial: bool) -> Option<Model> {
    if text.len() > MAX_TEXT {
        return None;
    }
    let deadline = Instant::now() + Duration::from_millis(10);
    let mut reader = NsReader::from_str(text);
    let mut stack: Vec<Open> = vec![];
    let mut root = None;
    let mut model = None::<Model>;
    let mut roots = 0;
    for _ in 0..8192 {
        if Instant::now() >= deadline {
            return None;
        }
        let start = reader.buffer_position() as usize;
        match reader.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                if stack.len() >= 64 || !namespaces(&reader, &e) {
                    return None;
                }
                let (resolved, local) = reader.resolver().resolve_element(e.name());
                let uri = match resolved {
                    ResolveResult::Bound(uri) => uri.as_ref().to_owned(),
                    ResolveResult::Unbound => String::new(),
                    _ => return None,
                };
                let local = local.as_ref().to_owned();
                if e.name().as_ref().matches(':').count() > 1 {
                    return None;
                }
                if stack.is_empty() {
                    roots += 1;
                    if roots != 1 {
                        return None;
                    }
                    let kind = match local.as_str() {
                        "persistence" if uri.is_empty() || PERSISTENCE.contains(&uri.as_str()) => Kind::Persistence,
                        "entity-mappings" if uri.is_empty() || ORM.contains(&uri.as_str()) => Kind::Orm,
                        _ => return None,
                    };
                    let prefix = if uri.starts_with("https://jakarta.ee/") {
                        "jakarta.persistence."
                    } else if !uri.is_empty() {
                        "javax.persistence."
                    } else if text.contains("javax.persistence.") {
                        "javax.persistence."
                    } else {
                        "jakarta.persistence."
                    };
                    root = Some(uri.clone());
                    model = Some(Model { elements: BTreeSet::new(), classes: vec![], properties: vec![], kind, prefix, open: vec![] });
                }
                // Unknown/foreign child elements cannot be relabelled by local name.
                if root.as_deref() != Some(&uri) {
                    return None;
                }
                let m = model.as_mut()?;
                if owned(m.kind, &stack, &local) {
                    m.elements.insert(start);
                }
                let empty = text[..reader.buffer_position() as usize].ends_with("/>");
                if m.kind == Kind::Persistence
                    && is_path(&stack, &["persistence", "persistence-unit", "properties"])
                    && local == "property"
                    && e.attributes().all(|a| {
                        a.is_ok_and(|a| {
                            matches!(a.key.as_ref(), "name" | "value" | "xml:lang")
                                || a.key.as_ref() == "xmlns"
                                || a.key.as_ref().starts_with("xmlns:")
                        })
                    })
                {
                    m.properties.push((start, reader.buffer_position() as usize));
                }
                if let Some(parent) = stack.last_mut() {
                    parent.children = true;
                }
                if !empty {
                    stack.push(Open { name: e.name().as_ref().into(), local, content: reader.buffer_position() as usize, children: false });
                }
            }
            Ok(Event::End(_)) => {
                let open = stack.pop()?;
                if model.as_ref()?.kind == Kind::Persistence
                    && open.local == "class"
                    && !open.children
                    && is_path(&stack, &["persistence", "persistence-unit"])
                {
                    model.as_mut()?.classes.push((open.content, start));
                }
            }
            Ok(Event::DocType(_) | Event::GeneralRef(_)) | Err(_) => return None,
            Ok(Event::CData(_)) => return None,
            Ok(Event::Text(t)) if stack.is_empty() && !t.as_ref().trim().is_empty() => return None,
            Ok(Event::Decl(_)) if roots != 0 => return None,
            Ok(Event::Eof) => {
                if roots != 1 || !partial && !stack.is_empty() {
                    return None;
                }
                let mut m = model?;
                m.open = stack;
                return Some(m);
            }
            _ => {}
        }
    }
    None
}
pub(crate) fn descriptor(text: &str) -> Option<Model> {
    parse(text, false)
}
pub(crate) fn completion(text: &str, caret: usize) -> Option<Completion> {
    if caret > text.len() || !text.is_char_boundary(caret) {
        return None;
    }
    if let Some(m) = descriptor(text) {
        if let Some((content, _)) = m.classes.iter().find(|(start, end)| *start <= caret && caret <= *end) {
            return Some(Completion::Class { content: *content });
        }
        if let Some((tag, _)) = m.properties.iter().find(|(start, end)| *start <= caret && caret < *end) {
            return Some(Completion::Property { tag: *tag, prefix: m.prefix });
        }
        return None;
    }
    if caret == text.len() {
        if let Some(m) = parse(text, true) {
            if m.kind == Kind::Persistence && is_path(&m.open, &["persistence", "persistence-unit", "class"]) {
                let class = m.open.last()?;
                if !class.children {
                    return Some(Completion::Class { content: class.content });
                }
            }
        }
    }
    // Only a partial property attribute is repaired for completion. Its ancestor
    // scope is parsed first; no namespace declaration or other attribute is guessed.
    let tag = text[..caret].rfind('<')?;
    let fragment = &text[tag..caret];
    let prefix = parse(&text[..tag], true)?;
    if prefix.kind != Kind::Persistence || !is_path(&prefix.open, &["persistence", "persistence-unit", "properties"]) {
        return None;
    }
    let name = fragment[1..].split_whitespace().next()?;
    if name.rsplit(':').next() != Some("property") || fragment.contains(['&', '>']) || fragment.contains("xmlns") {
        return None;
    }
    let quotes = fragment.matches('"').count();
    if quotes % 2 != 1 || fragment.contains('\'') {
        return None;
    }
    let repaired = format!("{}\"/>{}", &text[..caret], &text[caret..]);
    let completed = if text[caret..].trim().is_empty() {
        format!("{repaired}{}", prefix.open.iter().rev().map(|o| format!("</{}>", o.name)).collect::<String>())
    } else {
        repaired
    };
    let m = descriptor(&completed)?;
    let (_, end) = m.properties.iter().find(|(start, _)| *start == tag)?;
    // Admit only name/value attributes, so a partial foreign/qualified attribute
    // cannot smuggle a different context through the repaired tag.
    let mut r = NsReader::from_str(&completed[tag..*end]);
    if let Ok(Event::Empty(e)) = r.read_event() {
        if e.attributes().any(|a| a.is_err() || !matches!(a.unwrap().key.as_ref(), "name" | "value")) {
            return None;
        }
    } else {
        return None;
    }
    Some(Completion::Property { tag, prefix: m.prefix })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::JpaAssist;
    use openidea_plugin::{
        Range,
        diagnostics::DiagnosticsProvider,
        file_actions::FileActions,
        mock::MockHost,
        providers::{Document, Providers},
        text::position,
    };
    fn doc(text: &str) -> Document {
        Document { path: "/project/persistence.xml".into(), language: "xml".into(), version: 1, text: text.into() }
    }
    fn host() -> MockHost {
        let h = MockHost::install();
        h.add_file("src/main/java/example/Owned.java", "package example; public class Owned {}");
        h
    }
    fn complete(p: &mut JpaAssist, text: &str) -> Vec<String> {
        let at = position(text, text.find("example.O").unwrap() + "example.O".len());
        p.completion(&doc(text), at, None).into_iter().map(|c| c.label).collect()
    }
    #[test]
    fn public_four_fixture_regression_rejects_reserved_unbound_and_foreign_owners() {
        let _h = host();
        let mut p = JpaAssist::default();
        let good = "<persistence xmlns='https://jakarta.ee/xml/ns/persistence'><persistence-unit name='u' transaction-type='BAD'><class>example.O</class></persistence-unit></persistence>";
        assert_eq!(p.diagnostics(&doc(good)).len(), 1);
        assert_eq!(complete(&mut p, good), ["example.Owned"]);
        for bad in [
            good.replace("xmlns=", "xmlns:xml=").replace("<persistence", "<xml:persistence").replace("</persistence", "</xml:persistence"),
            "<q:persistence><q:persistence-unit transaction-type='BAD'><class>example.O</class></q:persistence-unit></q:persistence>"
                .into(),
            good.replace("https://jakarta.ee/xml/ns/persistence", "urn:foreign"),
        ] {
            assert!(p.diagnostics(&doc(&bad)).is_empty());
            assert!(complete(&mut p, &bad).is_empty());
            assert!(p.file_actions(&doc(&bad), Range::default()).is_empty());
        }
    }
    #[test]
    fn standard_legacy_jakarta_and_namespace_less_class_contexts_remain_usable() {
        let _h = host();
        let mut p = JpaAssist::default();
        for uri in PERSISTENCE.iter().copied().chain([""]) {
            let declaration = if uri.is_empty() { String::new() } else { format!("xmlns='{uri}'") };
            let text =
                format!("<persistence {declaration}><persistence-unit name='u'><class>example.O</class></persistence-unit></persistence>");
            assert_eq!(complete(&mut p, &text), ["example.Owned"]);
        }
        for uri in ORM {
            let text = format!("<entity-mappings xmlns='{uri}'><entity class='example.Owned' access='BAD'/></entity-mappings>");
            assert_eq!(p.diagnostics(&doc(&text)).len(), 1);
            assert!(
                descriptor(&text.replace("<entity-mappings", "<persistence").replace("</entity-mappings>", "</persistence>")).is_none()
            );
        }
    }
    #[test]
    fn namespace_scopes_are_resolved_by_uri_and_foreign_shadowing_is_quiet() {
        let _h = host();
        let mut p = JpaAssist::default();
        let text = "<p:persistence xmlns:p='https://jakarta.ee/xml/ns/persistence' xmlns:xml='http://www.w3.org/XML/1998/namespace' xml:lang='en'><p:persistence-unit name='u'><q:class xmlns:q='https://jakarta.ee/xml/ns/persistence'>example.O</q:class></p:persistence-unit></p:persistence>";
        assert_eq!(complete(&mut p, text), ["example.Owned"]);
        assert!(complete(&mut p, &text.replace("xmlns:q='https://jakarta.ee/xml/ns/persistence'", "xmlns:q='urn:foreign'")).is_empty());
        let shadow = text
            .replace("<q:class xmlns:q='https://jakarta.ee/xml/ns/persistence'>", "<p:class xmlns:p='urn:foreign'>")
            .replace("</q:class>", "</p:class>");
        assert!(complete(&mut p, &shadow).is_empty());
    }
    #[test]
    fn class_completion_requires_exact_persistence_unit_path_and_supports_unfinished_class_text() {
        let _h = host();
        let mut p = JpaAssist::default();
        for text in [
            "<persistence-unit><class>example.O</class></persistence-unit>",
            "<persistence><class>example.O</class></persistence>",
            "<persistence><persistence-unit><properties><class>example.O</class></properties></persistence-unit></persistence>",
            "<entity-mappings><class>example.O</class></entity-mappings>",
            "<other><class>example.O</class></other>",
        ] {
            assert!(complete(&mut p, text).is_empty());
        }
        assert_eq!(complete(&mut p, "<persistence><persistence-unit><class>example.O"), ["example.Owned"]);
    }
    #[test]
    fn public_property_completion_keeps_owned_partial_values_and_schema_api_prefix() {
        let _h = host();
        let mut p = JpaAssist::default();
        for (uri, key) in [
            ("https://jakarta.ee/xml/ns/persistence", "jakarta.persistence.jdbc.u"),
            ("http://xmlns.jcp.org/xml/ns/persistence", "javax.persistence.jdbc.u"),
        ] {
            let text = format!("<persistence xmlns=\"{uri}\"><persistence-unit name=\"u\"><properties><property name=\"{key}");
            let labels = p.completion(&doc(&text), position(&text, text.len()), None);
            assert_eq!(labels.len(), 2);
            assert!(labels.iter().all(|c| c.label.starts_with(key)));
        }
        let text = "<persistence><persistence-unit name=\"u\"><properties><property name=\"hibernate.hbm2ddl.auto\" value=\"cr";
        let values = p.completion(&doc(text), position(text, text.len()), None);
        assert_eq!(values.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["create", "create-drop"]);
        for text in [
            "<persistence><property name=\"jakarta.persistence.j",
            "<other><properties><property name=\"jakarta.persistence.j",
            "<persistence><persistence-unit><properties><property xmlns=\"urn:foreign\" name=\"jakarta.persistence.j",
        ] {
            assert!(p.completion(&doc(text), position(text, text.len()), None).is_empty());
        }
    }
    #[test]
    fn inspections_ignore_wrong_schema_paths_and_never_assert_missing_external_types() {
        let _h = host();
        let mut p = JpaAssist::default();
        for text in [
            "<persistence><wrong><shared-cache-mode>BAD</shared-cache-mode></wrong></persistence>",
            "<entity-mappings><validation-mode>BAD</validation-mode></entity-mappings>",
            "<persistence><persistence-unit name='u'><class>library.External</class></persistence-unit></persistence>",
        ] {
            assert!(p.diagnostics(&doc(text)).is_empty());
        }
    }
    #[test]
    fn malformed_reserved_and_oversized_descriptor_sources_are_quiet() {
        let _h = host();
        let mut p = JpaAssist::default();
        let good = "<persistence><persistence-unit name='u' transaction-type='BAD'/></persistence>";
        for bad in [
            good.replace("<persistence>", "<persistence xmlns:q='http://www.w3.org/XML/1998/namespac&#101;'>"),
            good.replace("<persistence>", "<persistence xmlns='http://www.w3.org/2000/xmlns/'>"),
            good.replace("</persistence>", "</wrong>"),
            good.replace("name='u'", "name='u' name='v'"),
            format!("<!DOCTYPE persistence>{good}"),
            format!("{good}{}", " ".repeat(MAX_TEXT)),
        ] {
            assert!(p.diagnostics(&doc(&bad)).is_empty());
        }
    }
}
