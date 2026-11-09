//! Explicit annotation source docs; never a resolved schema/runtime association.
use super::*;
use openidea_plugin::hover::Hover;
fn name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || i > 0 && b.is_ascii_digit())
}
fn imported(file: &java::JavaFile, simple: &str, expected: &str) -> bool {
    !file.types.iter().any(|t| t.name == simple)
        && file
            .imports
            .iter()
            .any(|i| !i.is_static && i.path == expected)
        && !file.imports.iter().any(|i| {
            !i.is_static && i.path.rsplit('.').next() == Some(simple) && i.path != expected
        })
}
pub(super) fn hover(doc: &Document, at: Position) -> Option<Hover> {
    if doc.language != "java"
        || !doc.path.ends_with(".java")
        || doc.text.len() > 128 << 10
        || doc.text.contains("\\u")
    {
        return None;
    }
    let off = offset(&doc.text, at)?;
    let file = java::parse(&doc.text);
    if !imported(
        &file,
        "Controller",
        "org.springframework.stereotype.Controller",
    ) || file.imports.iter().any(|i| i.path.ends_with(".*"))
    {
        return None;
    }
    for class in file
        .types
        .iter()
        .filter(|t| t.has(&["Controller"]) && t.kind == "class" && t.supers().next().is_none())
    {
        for method in class.methods() {
            let mappings: Vec<_> = method
                .annotations
                .iter()
                .filter(|a| matches!(a.name.as_str(), "QueryMapping" | "SchemaMapping"))
                .collect();
            let [a] = mappings.as_slice() else {
                continue;
            };
            if !(a.at <= off && off <= a.end)
                || !imported(
                    &file,
                    &a.name,
                    &format!(
                        "org.springframework.graphql.data.method.annotation.{}",
                        a.name
                    ),
                )
            {
                continue;
            }
            let allowed: &[&str] = if a.name == "QueryMapping" {
                &["value", "name"]
            } else {
                &["value", "field", "typeName"]
            };
            if a.args.iter().any(|arg| {
                !allowed.contains(&arg.name.as_str())
                    || arg.strings.len() != 1
                    || java::tokens(&arg.text).len() != 1
            }) {
                continue;
            }
            let field_args: Vec<_> = a.args.iter().filter(|arg| arg.name != "typeName").collect();
            if field_args.len() > 1 {
                continue;
            }
            let field = field_args
                .first()
                .map_or(method.name.as_str(), |arg| arg.strings[0].value.as_str());
            let field = if field.is_empty() {
                method.name.as_str()
            } else {
                field
            };
            if !name(field) {
                continue;
            }
            let types: Vec<_> = a.args.iter().filter(|arg| arg.name == "typeName").collect();
            if types.len() > 1 {
                continue;
            }
            let parent = if a.name == "QueryMapping" {
                "Query (composed annotation contract)".into()
            } else if let Some(arg) = types.first() {
                let value = &arg.strings[0].value;
                if !name(value) {
                    continue;
                }
                format!("{value} (explicit method source)")
            } else {
                "UNRESOLVED (method typeName omitted; class/source-argument inference not evaluated)".into()
            };
            return Some(Hover::new(format!("GraphQL @{} mapping source\n\nField: `{field}`.\n\nParent type: {parent}.\n\nSpring for GraphQL1.3 annotation contract: field defaults to method name; method typeName overrides class-level defaults. This is current-source documentation only. Installed version, merged schema, classpath/meta-annotations, resolver registration and runtime execution are not established.",a.name)).range(range(&doc.text,a.at,a.end)));
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    fn doc(mapping: &str) -> Document {
        Document {
            path: "/project/G.java".into(),
            language: "java".into(),
            version: 3,
            text: format!(
                "import org.springframework.stereotype.Controller; import org.springframework.graphql.data.method.annotation.QueryMapping; import org.springframework.graphql.data.method.annotation.SchemaMapping; @Controller class G {{ /* 😀 */ {mapping} Object book() {{return null;}} }}"
            ),
        }
    }
    #[test]
    fn explicit_query_and_schema_mapping_docs_preserve_range_and_withhold_inference() {
        for (mapping, field, parent) in [
            ("@QueryMapping", "book", "Query (composed"),
            (
                "@SchemaMapping(typeName=\"Book\",field=\"title\")",
                "title",
                "Book (explicit",
            ),
            ("@SchemaMapping", "book", "UNRESOLVED"),
        ] {
            let d = doc(mapping);
            let at = openidea_plugin::text::position(&d.text, d.text.find(mapping).unwrap() + 1);
            let h = hover(&d, at).unwrap();
            assert!(h.markdown.contains(&format!("`{field}`")));
            assert!(h.markdown.contains(parent));
            assert!(h.markdown.contains("not established"));
            assert_eq!(
                h.range.unwrap().start,
                openidea_plugin::text::position(&d.text, d.text.find(mapping).unwrap())
            );
        }
    }
    #[test]
    fn conflicting_aliases_foreign_or_dynamic_mapping_docs_stay_quiet() {
        for mapping in [
            "@QueryMapping(value=\"a\",name=\"b\")",
            "@QueryMapping(NAME)",
            "@SchemaMapping(typeName=TYPE)",
        ] {
            let d = doc(mapping);
            assert!(
                hover(
                    &d,
                    openidea_plugin::text::position(&d.text, d.text.find(mapping).unwrap() + 1)
                )
                .is_none()
            );
        }
        let d = doc("@QueryMapping");
        let d = Document {
            text: d.text.replace(
                "org.springframework.graphql.data.method.annotation.QueryMapping",
                "foreign.QueryMapping",
            ),
            ..d
        };
        assert!(
            hover(
                &d,
                openidea_plugin::text::position(&d.text, d.text.find("@QueryMapping").unwrap() + 1)
            )
            .is_none()
        );
    }
}
