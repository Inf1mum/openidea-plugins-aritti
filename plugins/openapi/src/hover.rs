//! Current-document JSON declaration facts. This is documentation, not a
//! reference graph or permission to rename a component.
use crate::spec;
use openidea_plugin::{Position, hover::Hover, providers::Document};
use serde_json::Value;
const MAX_BYTES: usize = 512 * 1024;
const MAX_NODES: usize = 8192;
const MAX_DEPTH: usize = 48;
fn bounded(value: &Value, depth: usize, count: &mut usize) -> bool {
    *count += 1;
    if depth > MAX_DEPTH || *count > MAX_NODES {
        return false;
    }
    match value {
        Value::Object(o) => o.values().all(|v| bounded(v, depth + 1, count)),
        Value::Array(a) => a.iter().all(|v| bounded(v, depth + 1, count)),
        _ => true,
    }
}
fn safe(text: &str, max: usize) -> String {
    let mut out = String::new();
    for c in text.chars().take(max) {
        if c.is_control() {
            if c == '\n' {
                out.push_str("  \n");
            }
            continue;
        }
        if "\\`*_{}[]()<>#+!|".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
fn supported(root: &Value) -> bool {
    if root.get("openapi").is_some() && root.get("swagger").is_some() {
        return false;
    }
    root["swagger"] == "2.0"
        || root["openapi"].as_str().is_some_and(|v| {
            v.rsplit_once('.').is_some_and(|(minor, patch)| {
                matches!(minor, "3.0" | "3.1")
                    && !patch.is_empty()
                    && patch.bytes().all(|b| b.is_ascii_digit())
            })
        })
}
fn unique<'a>(
    keys: &std::collections::BTreeMap<&str, Option<&'a spec::Key>>,
    path: &str,
) -> Option<&'a spec::Key> {
    keys.get(path).copied().flatten()
}
// An actual object-key chain must own every decoded pointer segment. Flattened
// path equality alone cannot distinguish duplicate ancestors or dotted impostors.
fn pointer_source<'a>(
    root: &'a Value,
    pointer: &str,
    keys: &std::collections::BTreeMap<&str, Option<&spec::Key>>,
    text: &str,
) -> Option<&'a Value> {
    let mut value = root;
    let mut path = String::new();
    for token in pointer.strip_prefix('/')?.split('/') {
        if token.contains('%') {
            return None;
        }
        let name = token.replace("~1", "/").replace("~0", "~");
        if name.replace('~', "~0").replace('/', "~1") != token {
            return None;
        }
        value = value.as_object()?.get(&name)?;
        if !path.is_empty() {
            path.push('.');
        }
        path.push_str(&name);
        let key = unique(keys, &path)?;
        if crate::json::unescape(text.get(key.at..key.end)?) != name {
            return None;
        }
    }
    Some(value)
}

/// A current-source JSON object-key location, independent of OpenAPI category
/// documentation. A valid external fragment need not repeat the API version.
/// This proves a source location only, never a closed replaceable reference set.
pub(crate) fn pointer_definition(text: &str, pointer: &str) -> Option<openidea_plugin::Range> {
    if text.len()>MAX_BYTES { return None; }
    let root:Value=serde_json::from_str(text).ok()?;
    if !bounded(&root,0,&mut 0) { return None; }
    let keys=crate::json::bounded_keys(text,MAX_NODES,4<<20)?;
    if keys.len()>MAX_NODES || keys.iter().any(|key|crate::json::unescape(&text[key.at..key.end])=="$id") { return None; }
    let mut by_key=std::collections::BTreeMap::new();
    for key in &keys {
        by_key.entry(key.key.as_str()).and_modify(|value|*value=None).or_insert(Some(key));
    }
    pointer_source(&root,pointer,&by_key,text)?;
    let path=pointer.strip_prefix('/')?.split('/').map(|token|token.replace("~1","/").replace("~0","~")).collect::<Vec<_>>().join(".");
    let key=unique(&by_key,&path)?;
    (key.at<key.end).then(||spring_text::range(text,key.at,key.end))
}
pub(crate) fn nested_schema_pointer(rest: &str) -> bool {
    let mut tokens = rest.split('/');
    if tokens.next().is_none() {
        return false;
    }
    let mut nested = false;
    while let Some(keyword) = tokens.next() {
        match keyword {
            "properties" => {
                if tokens.next().is_none() {
                    return false;
                }
            }
            "items" | "additionalProperties" | "not" => {}
            _ => return false,
        }
        nested = true;
    }
    nested
}
pub(crate) fn facts(heading: &str, value: &Value) -> String {
    let mut markdown = format!("**{}**", safe(heading, 256));
    for (key, label) in [
        ("title", "Title"),
        ("summary", "Summary"),
        ("description", "Description"),
        ("type", "Declared type"),
        ("format", "Format"),
        ("operationId", "Operation id"),
    ] {
        if let Some(value) = value[key].as_str() {
            if !value.trim().is_empty() {
                markdown.push_str(&format!("\n\n{label}: {}", safe(value, 768)));
            }
        }
    }
    if let Some(value) = value.as_bool() {
        markdown.push_str(&format!("\n\nBoolean schema: {value}"));
    }
    if value["deprecated"] == true {
        markdown.push_str("\n\nDeclared deprecated.");
    }
    markdown
        .push_str("\n\nSource declaration in the current document; references are not fetched.");
    if markdown.len() > 8192 {
        let mut end = 8192;
        while !markdown.is_char_boundary(end) {
            end -= 1;
        }
        markdown.truncate(end);
    }
    markdown
}
pub fn hover(doc: &Document, at: Position) -> Option<Hover> {
    if doc.language=="yaml" { return crate::yaml_hover::hover(doc,at); }
    if doc.language != "json" || doc.text.len() > MAX_BYTES {
        return None;
    }
    let root: Value = serde_json::from_str(&doc.text).ok()?;
    if !supported(&root) || !bounded(&root, 0, &mut 0) {
        return None;
    }
    let keys = crate::json::bounded_keys(&doc.text,MAX_NODES,4<<20)?;
    if keys.len() > MAX_NODES {
        return None;
    }
    let mut by_key = std::collections::BTreeMap::new();
    for key in &keys {
        by_key
            .entry(key.key.as_str())
            .and_modify(|value| *value = None)
            .or_insert(Some(key));
    }
    unique(
        &by_key,
        if root["swagger"] == "2.0" {
            "swagger"
        } else {
            "openapi"
        },
    )?;
    let at = openidea_plugin::text::offset(&doc.text, at)?;
    let selected = keys
        .iter()
        .find(|k| (k.at <= at && at < k.end) || (k.value_at <= at && at < k.value_end))?;
    if unique(&by_key, &selected.key).is_none() {
        return None;
    }
    let key_text = crate::json::unescape(&doc.text[selected.at..selected.end]);
    let reference = key_text == "$ref";
    let target = if reference {
        let pointer = selected.value.strip_prefix('#')?;
        if !pointer.starts_with('/') || pointer.contains('%') {
            return None;
        }
        Some((pointer, pointer_source(&root, pointer, &by_key, &doc.text)?))
    } else {
        None
    };
    let categories: &[(&str, &str)] = if root["swagger"] == "2.0" {
        &[
            ("definitions", "Schema"),
            ("parameters", "Parameter"),
            ("responses", "Response"),
            ("securityDefinitions", "Security scheme"),
        ]
    } else {
        &[
            ("components.schemas", "Schema"),
            ("components.parameters", "Parameter"),
            ("components.responses", "Response"),
            ("components.requestBodies", "Request body"),
            ("components.headers", "Header"),
            ("components.examples", "Example"),
            ("components.links", "Link"),
            ("components.callbacks", "Callback"),
            ("components.securitySchemes", "Security scheme"),
            ("components.pathItems", "Path item"),
        ]
    };
    // Current-file nested schema facts are useful documentation, without
    // claiming a closed reference set or publishing a Rename-facing graph.
    if let Some((pointer, value)) = &target {
        let prefix = if root["swagger"] == "2.0" {
            "/definitions/"
        } else {
            "/components/schemas/"
        };
        if let Some(rest) = pointer.strip_prefix(prefix) {
            if nested_schema_pointer(rest)
                && (value.is_object()
                    || (root["openapi"]
                        .as_str()
                        .is_some_and(|v| v.starts_with("3.1."))
                        && value.is_boolean()))
            {
                return Some(
                    Hover::new(facts(&format!("Local schema: #{pointer}"), value)).range(
                        spring_text::range(&doc.text, selected.value_at, selected.value_end),
                    ),
                );
            }
        }
    }
    for (path, label) in categories {
        let pointer = format!("/{}", path.replace('.', "/"));
        let Some(definitions) = root.pointer(&pointer).and_then(Value::as_object) else {
            continue;
        };
        for (name, value) in definitions {
            let pointer = format!("{pointer}/{}", name.replace('~', "~0").replace('/', "~1"));
            if pointer_source(&root, &pointer, &by_key, &doc.text).is_none() {
                continue;
            }
            let path = format!("{path}.{name}");
            let Some(definition) = unique(&by_key, &path) else {
                continue;
            };
            if crate::json::unescape(&doc.text[definition.at..definition.end]) != *name {
                continue;
            }
            let matched = target
                .as_ref()
                .is_some_and(|(target, _)| *target == pointer)
                || (!reference
                    && ((selected.key == path && selected.at == definition.at)
                        || ["title", "summary", "description", "type", "format"]
                            .iter()
                            .any(|field| {
                                selected.key == format!("{path}.{field}") && key_text == *field
                            })));
            if matched {
                let span = if reference || (selected.value_at <= at && at < selected.value_end) {
                    (selected.value_at, selected.value_end)
                } else {
                    (selected.at, selected.end)
                };
                return Some(
                    Hover::new(facts(&format!("{label}: {name}"), value))
                        .range(spring_text::range(&doc.text, span.0, span.1)),
                );
            }
        }
    }
    if reference {
        return None;
    }
    for (path, item) in root["paths"].as_object().into_iter().flatten() {
        for method in spec::METHODS {
            if root["swagger"] == "2.0" && method == "trace" {
                continue;
            }
            let Some(operation) = item.get(method).filter(|v| v.is_object()) else {
                continue;
            };
            let prefix = format!("paths.{path}.{method}");
            if (selected.key != prefix || key_text != method)
                && !["summary", "description", "operationId"]
                    .iter()
                    .any(|field| selected.key == format!("{prefix}.{field}") && key_text == *field)
            {
                continue;
            }
            if unique(&by_key, &selected.key).is_none() {
                return None;
            }
            let (start, end) = if selected.value_at <= at && at < selected.value_end {
                (selected.value_at, selected.value_end)
            } else {
                (selected.at, selected.end)
            };
            return Some(
                Hover::new(facts(
                    &format!("{} {path}", method.to_ascii_uppercase()),
                    operation,
                ))
                .range(spring_text::range(&doc.text, start, end)),
            );
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document(text: &str) -> Document {
        Document {
            path: "/one/api.json".into(),
            language: "json".into(),
            text: text.into(),
            version: 1,
        }
    }
    fn at(text: &str, needle: &str) -> Position {
        openidea_plugin::text::position(text, text.find(needle).unwrap())
    }
    const SPEC: &str = r##"{"openapi":"3.0.4","info":{"title":"😀","version":"1"},"paths":{"/pets":{"get":{"summary":"List pets","operationId":"listPets","responses":{}}}},"components":{"schemas":{"Pet":{"type":"object","description":"A <script> pet [link](https://evil.test)"}}},"x":{"$ref":"#/components/schemas/Pet"}}"##;
    #[test]
    fn component_and_local_reference_show_declared_facts_and_exact_utf16_ranges() {
        let doc = document(SPEC);
        let found = hover(&doc, at(SPEC, "Pet\"")).unwrap();
        assert!(found.markdown.contains("Schema: Pet"));
        assert!(found.markdown.contains("Declared type: object"));
        assert!(!found.markdown.contains("<script>"));
        assert!(!found.markdown.contains("[link]("));
        let span = found.range.unwrap();
        let start = openidea_plugin::text::offset(SPEC, span.start).unwrap();
        let end = openidea_plugin::text::offset(SPEC, span.end).unwrap();
        assert_eq!(&SPEC[start..end], "Pet");
        assert_eq!(
            span.start.character as usize,
            SPEC[..start].encode_utf16().count()
        );
        let reference = hover(&doc, at(SPEC, "#/components/schemas/Pet")).unwrap();
        assert!(reference.markdown.contains("Declared type: object"));
        let span = reference.range.unwrap();
        assert_eq!(
            &SPEC[openidea_plugin::text::offset(SPEC, span.start).unwrap()
                ..openidea_plugin::text::offset(SPEC, span.end).unwrap()],
            "#/components/schemas/Pet"
        );
    }
    #[test]
    fn operation_and_current_buffer_facts_do_not_need_project_access() {
        use openidea_plugin::{hover::HoverProvider, mock::MockHost};
        let host = MockHost::install().with_capabilities(&[]);
        let mut plugin = crate::OpenApi::default();
        let mut doc = document(SPEC);
        let found = plugin.hover(&doc, at(SPEC, "listPets")).unwrap();
        assert!(found.markdown.contains("GET /pets"));
        assert!(found.markdown.contains("List pets"));
        doc.text = SPEC.replace("List pets", "Current unsaved summary");
        let found = plugin.hover(&doc, at(&doc.text, "listPets")).unwrap();
        assert!(found.markdown.contains("Current unsaved summary"));
        assert!(!found.markdown.contains("List pets"));
        assert!(host.requests().is_empty());
        assert!(host.opened_locations().is_empty());
    }
    #[test]
    fn swagger_definitions_and_boolean_schemas_are_source_facts() {
        let text = r#"{"swagger":"2.0","definitions":{"Pet":{"type":"object"}}}"#;
        assert!(
            hover(&document(text), at(text, "Pet"))
                .unwrap()
                .markdown
                .contains("Declared type: object")
        );
        let text = r#"{"openapi":"3.1.1","components":{"schemas":{"Never":false}}}"#;
        assert!(
            hover(&document(text), at(text, "Never"))
                .unwrap()
                .markdown
                .contains("Boolean schema: false")
        );
    }
    #[test]
    fn external_unresolved_unknown_versions_and_yaml_are_quiet() {
        for text in [
            SPEC.replace(
                "#/components/schemas/Pet",
                "https://example.test/api.json#/components/schemas/Pet",
            ),
            SPEC.replace("#/components/schemas/Pet", "#/components/schemas/Missing"),
            SPEC.replace("3.0.4", "4.0.0"),
        ] {
            let offset = text.rfind("$ref").unwrap();
            assert!(
                hover(
                    &document(&text),
                    openidea_plugin::text::position(&text, offset)
                )
                .is_none()
            );
        }
        let mut doc = document(SPEC);
        doc.language = "yaml".into();
        assert!(hover(&doc, at(SPEC, "Pet")).is_none());
        assert!(hover(&document("{\"openapi\":"), Position::new(0, 1)).is_none());
    }
    #[test]
    fn bounds_and_duplicate_flattened_owners_are_conservative() {
        let mut text = SPEC.to_owned();
        text.push_str(&" ".repeat(MAX_BYTES));
        assert!(hover(&document(&text), at(SPEC, "Pet")).is_none());
        let text = SPEC.replace(
            "\"components\":",
            "\"components.schemas.Pet\":false,\"components\":",
        );
        assert!(hover(&document(&text), at(&text, "Pet\":false")).is_none());
        let text = SPEC.replace(
            "A <script> pet [link](https://evil.test)",
            &"😀".repeat(10000),
        );
        let found = hover(&document(&text), at(&text, "Pet\"")).unwrap();
        assert!(found.markdown.len() <= 8192);
        for end in (0..SPEC.len()).filter(|end| SPEC.is_char_boundary(*end)) {
            let _ = hover(&document(&SPEC[..end]), Position::new(0, 0));
        }
    }
    #[test]
    fn plain_token_identity_is_not_closed_reference_ownership() {
        let definition = "Pet";
        let local = "#/components/schemas/Pet";
        let external = "other.json#/components/schemas/Pet";
        let encoded = "#/components/schemas/P%65t";
        assert_eq!(local.rsplit('/').next(), Some(definition));
        assert_eq!(external.rsplit('/').next(), Some(definition));
        assert_ne!(encoded.rsplit('/').next(), Some(definition));
        let manifest = include_str!("../plugin.toml");
        assert!(!manifest.contains("contributes.documentIndexers"));
        let registration = include_str!("lib.rs")
            .lines()
            .find(|line| line.starts_with("register_plugin!("))
            .unwrap();
        assert!(!registration.contains("indexing"));
    }
}

#[cfg(test)]
mod declaration_regressions {
    use super::*;
    fn doc(path: &str, text: &str) -> Document {
        Document {
            path: path.into(),
            language: "json".into(),
            text: text.into(),
            version: 1,
        }
    }
    #[test]
    fn equal_names_in_other_files_and_projects_do_not_leak_facts() {
        use openidea_plugin::hover::HoverProvider;
        let mut plugin = crate::OpenApi::default();
        for (path, description) in [
            ("/one/a.json", "First"),
            ("/one/b.json", "Second"),
            ("/two/a.json", "Third"),
        ] {
            let text = format!(
                r#"{{"openapi":"3.1.1","components":{{"schemas":{{"Pet":{{"description":"{description}"}}}}}}}}"#
            );
            let found = plugin
                .hover(
                    &doc(path, &text),
                    openidea_plugin::text::position(&text, text.find(description).unwrap()),
                )
                .unwrap();
            assert!(found.markdown.contains(description));
            let span = found.range.unwrap();
            let start = openidea_plugin::text::offset(&text, span.start).unwrap();
            let end = openidea_plugin::text::offset(&text, span.end).unwrap();
            assert_eq!(&text[start..end], description);
        }
    }
    #[test]
    fn missing_description_is_not_invented_and_flattened_impostors_are_quiet() {
        let text = r#"{"openapi":"3.1.1","components":{"schemas":{"Empty":{}}}}"#;
        let found = hover(
            &doc("/one/a.json", text),
            openidea_plugin::text::position(text, text.find("Empty").unwrap()),
        )
        .unwrap();
        assert!(!found.markdown.contains("Description:"));
        let text = r#"{"openapi":"3.1.1","components.schemas.Pet.description":"impostor","components":{"schemas":{"Pet":{"type":"object"}}}}"#;
        assert!(
            hover(
                &doc("/one/a.json", text),
                openidea_plugin::text::position(text, text.find("impostor").unwrap())
            )
            .is_none()
        );
    }
}

#[cfg(test)]
mod pointer_owner_tests {
    use super::*;
    use openidea_plugin::{hover::HoverProvider, mock::MockHost};
    fn doc(text: &str) -> Document {
        Document {
            path: "/one/openapi.json".into(),
            language: "json".into(),
            text: text.into(),
            version: 7,
        }
    }
    fn ref_hover(text: &str) -> Option<Hover> {
        hover(
            &doc(text),
            openidea_plugin::text::position(
                text,
                text.rfind("#/components")
                    .or_else(|| text.rfind("#/definitions"))
                    .unwrap(),
            ),
        )
    }
    #[test]
    fn nested_schema_reference_uses_current_source_and_exact_utf16_reference_range() {
        let host = MockHost::install().with_capabilities(&[]);
        let mut plugin = crate::OpenApi::default();
        let text = r##"{"openapi":"3.1.1","info":{"title":"😀"},"components":{"schemas":{"Pet":{"properties":{"id":{"type":"integer","description":"Current <id>"}}}}},"x":{"$ref":"#/components/schemas/Pet/properties/id"}}"##;
        let at = openidea_plugin::text::position(text, text.rfind("#/components").unwrap());
        let h = plugin.hover(&doc(text), at).unwrap();
        assert!(h.markdown.contains("Declared type: integer"));
        assert!(h.markdown.contains("Local schema:"));
        assert!(!h.markdown.contains("<id>"));
        let r = h.range.unwrap();
        let a = openidea_plugin::text::offset(text, r.start).unwrap();
        let b = openidea_plugin::text::offset(text, r.end).unwrap();
        assert_eq!(&text[a..b], "#/components/schemas/Pet/properties/id");
        assert_eq!(r.start.character as usize, text[..a].encode_utf16().count());
        let changed = text.replace("Current <id>", "Unsaved changed");
        let mut d = doc(&changed);
        d.version += 1;
        assert!(
            plugin
                .hover(
                    &d,
                    openidea_plugin::text::position(
                        &changed,
                        changed.rfind("#/components").unwrap()
                    )
                )
                .unwrap()
                .markdown
                .contains("Unsaved changed")
        );
        assert!(host.requests().is_empty());
        assert!(host.opened_locations().is_empty());
    }
    #[test]
    fn escaped_pointer_names_are_documentation_not_replaceable_symbol_claims() {
        let text = r##"{"openapi":"3.1.1","components":{"schemas":{"A/B":{"properties":{"~id":{"description":"literal"}}}}},"x":{"$ref":"#/components/schemas/A~1B/properties/~0id"}}"##;
        assert!(ref_hover(text).unwrap().markdown.contains("literal"));
        for replacement in [
            "A%2FB/properties/~0id",
            "A~2B/properties/~0id",
            "A~1B/properties/missing",
        ] {
            let invalid = text.replace("A~1B/properties/~0id", replacement);
            assert!(ref_hover(&invalid).is_none());
        }
        let manifest = include_str!("../plugin.toml");
        assert!(!manifest.contains("contributes.documentIndexers"));
    }
    #[test]
    fn duplicate_ancestors_and_flattened_impostors_withhold_reference_facts() {
        for text in [
            r##"{"openapi":"3.0.4","openapi":"3.1.1","components":{"schemas":{"Pet":{"description":"ambiguous version"}}},"x":{"$ref":"#/components/schemas/Pet"}}"##,
            r##"{"openapi":"3.1.1","components":{},"components":{"schemas":{"Pet":{"description":"ambiguous"}}},"x":{"$ref":"#/components/schemas/Pet"}}"##,
            r##"{"openapi":"3.1.1","components":{"schemas":{"Pet":{"properties":{},"properties":{"id":{"description":"ambiguous"}}}}},"x":{"$ref":"#/components/schemas/Pet/properties/id"}}"##,
            r##"{"openapi":"3.1.1","components.schemas.Pet.properties.id":{"description":"impostor"},"components":{"schemas":{"Pet":{"properties":{"id":{"description":"real"}}}}},"x":{"$ref":"#/components/schemas/Pet/properties/id"}}"##,
        ] {
            assert!(ref_hover(text).is_none(), "{text}");
        }
    }
    #[test]
    fn swagger_nested_schema_and_31_boolean_targets_are_source_facts_only() {
        let swagger = r##"{"swagger":"2.0","definitions":{"Pet":{"properties":{"id":{"type":"integer"}}}},"x":{"$ref":"#/definitions/Pet/properties/id"}}"##;
        assert!(
            ref_hover(swagger)
                .unwrap()
                .markdown
                .contains("Declared type: integer")
        );
        let boolean = r##"{"openapi":"3.1.1","components":{"schemas":{"Pet":{"properties":{"id":false}}}},"x":{"$ref":"#/components/schemas/Pet/properties/id"}}"##;
        assert!(
            ref_hover(boolean)
                .unwrap()
                .markdown
                .contains("Boolean schema: false")
        );
        assert!(ref_hover(&boolean.replace("3.1.1", "3.0.4")).is_none());
        let extension = r##"{"openapi":"3.1.1","components":{"schemas":{"Pet":{"x-custom":{"description":"not a schema owner"}}}},"x":{"$ref":"#/components/schemas/Pet/x-custom"}}"##;
        assert!(ref_hover(extension).is_none());
        let array = r##"{"openapi":"3.1.1","components":{"schemas":{"Pet":{"allOf":[{"description":"unknown array owner"}]}}},"x":{"$ref":"#/components/schemas/Pet/allOf/0"}}"##;
        assert!(ref_hover(array).is_none());
    }
}
