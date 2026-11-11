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
fn import_name<'a>(text: &'a str, import: &'a java::Import) -> (&'a str, bool) {
    let tail = text
        .get(import.end..)
        .unwrap_or("")
        .split('\n')
        .next()
        .unwrap_or("");
    let tokens = java::tokens(tail);
    if let [alias, name, ..] = tokens.as_slice() {
        if alias.s == "as" && name.is_ident() {
            return (name.s, true);
        }
    }
    (import.path.rsplit('.').next().unwrap_or(""), false)
}
fn shadowed(file: &java::JavaFile, text: &str, name: &str) -> bool {
    if file.types.iter().any(|t| t.name == name) {
        return true;
    }
    let tokens = java::tokens(text);
    let mut depth = 0usize;
    for token in &tokens {
        match token.s {
            "<" => depth += 1,
            ">" => depth = depth.saturating_sub(1),
            _ if depth > 0 && token.s == name => return true,
            _ => {}
        }
    }
    tokens
        .windows(2)
        .any(|pair| pair[0].s == "typealias" && pair[1].s == name)
}
fn owned(file: &java::JavaFile, text: &str, a: &java::Annotation, expected: &str) -> bool {
    let Some(raw) = text.get(a.at + 1..a.name_at + a.name.len()) else {
        return false;
    };
    let written = java::tokens(raw).iter().map(|t| t.s).collect::<String>();
    if written == expected {
        return !shadowed(file, text, "org")
            && !file.imports.iter().any(|i| import_name(text, i).0 == "org");
    }
    let simple = expected.rsplit('.').next().unwrap_or("");
    if written != simple || shadowed(file, text, simple) {
        return false;
    }
    let imports = file
        .imports
        .iter()
        .filter(|i| import_name(text, i).0 == simple)
        .collect::<Vec<_>>();
    matches!(imports.as_slice(),[import] if import.path==expected&&!import.is_static&&!import_name(text,import).1)
}
pub(super) fn hover(doc: &Document, at: Position) -> Option<Hover> {
    if !((doc.language == "java" && doc.path.ends_with(".java"))
        || (doc.language == "kotlin" && doc.path.ends_with(".kt")))
        || doc.text.len() > 128 << 10
        || doc.text.contains("\\u")
    {
        return None;
    }
    let off = offset(&doc.text, at)?;
    let file = spring_text::parse_source(&doc.path, &doc.text);
    if file.imports.iter().any(|i| i.path.ends_with(".*")) {
        return None;
    }
    for class in file.types.iter().filter(|t| {
        t.annotations.iter().any(|a| {
            a.name == "Controller"
                && owned(
                    &file,
                    &doc.text,
                    a,
                    "org.springframework.stereotype.Controller",
                )
        }) && t.kind == "class"
            && t.supers().next().is_none()
    }) {
        for method in class.methods() {
            let mappings: Vec<_> = method
                .annotations
                .iter()
                .filter(|a| {
                    matches!(
                        a.name.as_str(),
                        "QueryMapping"
                            | "MutationMapping"
                            | "SubscriptionMapping"
                            | "SchemaMapping"
                            | "BatchMapping"
                    )
                })
                .collect();
            let [a] = mappings.as_slice() else {
                continue;
            };
            if !(a.at <= off && off <= a.end)
                || a.name == "BatchMapping"
                || !owned(
                    &file,
                    &doc.text,
                    a,
                    &format!(
                        "org.springframework.graphql.data.method.annotation.{}",
                        a.name
                    ),
                )
            {
                continue;
            }
            let allowed: &[&str] = if a.name != "SchemaMapping" {
                &["value", "name"]
            } else {
                &["value", "field", "typeName"]
            };
            if a.args.iter().any(|arg| {
                !allowed.contains(&arg.name.as_str())
                    || arg.strings.len() != 1
                    || java::tokens(&arg.text).len() != 1
                    || arg.text.contains('\\')
                    || !arg.text.trim().starts_with('"')
                    || arg.text.trim().starts_with("\"\"\"")
                    || doc.text.get(arg.strings[0].at..arg.strings[0].end)
                        != Some(arg.strings[0].value.as_str())
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
            let shortcut = match a.name.as_str() {
                "QueryMapping" => Some("Query"),
                "MutationMapping" => Some("Mutation"),
                "SubscriptionMapping" => Some("Subscription"),
                _ => None,
            };
            let parent = if let Some(parent) = shortcut {
                format!("{parent} (composed annotation contract)")
            } else if let Some(arg) = types.first() {
                let value = &arg.strings[0].value;
                if !name(value) {
                    continue;
                }
                format!("{value} (explicit method source)")
            } else {
                "UNRESOLVED (method typeName omitted; class/source-argument inference not evaluated)".into()
            };
            return Some(Hover::new(format!("GraphQL @{} mapping source\n\nField: `{field}`.\n\nParent type: {parent}.\n\nDeclared Spring for GraphQL annotation contract: field defaults to method name; method typeName overrides class-level defaults. This is current-source documentation only. Installed version, merged schema, classpath/meta-annotations, resolver registration and runtime execution are not established.",a.name)).range(range(&doc.text,a.at,a.end)));
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

#[cfg(test)]
mod shortcut_kotlin_tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, text::position};
    fn document(mapping: &str, kotlin: bool) -> Document {
        let imports = "import org.springframework.stereotype.Controller; import org.springframework.graphql.data.method.annotation.QueryMapping; import org.springframework.graphql.data.method.annotation.MutationMapping; import org.springframework.graphql.data.method.annotation.SubscriptionMapping; import org.springframework.graphql.data.method.annotation.SchemaMapping;";
        Document {
            path: if kotlin {
                "/project/G.kt"
            } else {
                "/project/G.java"
            }
            .into(),
            language: if kotlin { "kotlin" } else { "java" }.into(),
            version: 1,
            text: if kotlin {
                format!(
                    "{}\n@Controller @SchemaMapping(typeName=\"CustomRoot\") class G {{ /*😀*/ {mapping} fun book(): Any {{return Any()}} }}",
                    imports.replace(';', "\n")
                )
            } else {
                format!(
                    "{imports} @Controller @SchemaMapping(typeName=\"CustomRoot\") class G {{ /*😀*/ {mapping} Object book(){{return null;}} }}"
                )
            },
        }
    }
    fn query(d: &Document, mapping: &str) -> Option<Hover> {
        GraphQlTools::default().live_hover(d, position(&d.text, d.text.find(mapping).unwrap() + 1))
    }
    #[test]
    fn java_and_kotlin_mutation_subscription_contracts_keep_fixed_parent_types() {
        let _h = MockHost::install().with_capabilities(&[]);
        for kotlin in [false, true] {
            for (mapping, parent, field) in [
                ("@MutationMapping", "Mutation", "book"),
                (
                    "@SubscriptionMapping(name=\"changed\")",
                    "Subscription",
                    "changed",
                ),
                ("@MutationMapping(\"create\")", "Mutation", "create"),
            ] {
                let d = document(mapping, kotlin);
                let value = query(&d, mapping).unwrap();
                assert!(value.markdown.contains(&format!(
                    "Parent type: {parent} (composed annotation contract)"
                )));
                assert!(value.markdown.contains(&format!("Field: `{field}`")));
                assert!(!value.markdown.contains("Parent type: CustomRoot"));
                assert!(value.markdown.contains("not established"));
            }
        }
    }
    #[test]
    fn kotlin_import_alias_foreign_shadow_and_dynamic_literals_do_not_borrow_owned_docs() {
        let _h = MockHost::install().with_capabilities(&[]);
        let base = document("@MutationMapping", true);
        for d in [
            Document{text:base.text.replace("import org.springframework.graphql.data.method.annotation.MutationMapping\n","import org.springframework.graphql.data.method.annotation.MutationMapping as Other\n"),..base.clone()},
            Document{text:base.text.replace("import org.springframework.graphql.data.method.annotation.MutationMapping\n","import foreign.MutationMapping\n"),..base.clone()},
            Document{text:base.text.replace("class G {","class G<MutationMapping> {"),..base.clone()},
            Document{text:base.text.replace("@Controller", "typealias MutationMapping = Other\n@Controller"),..base.clone()},
            Document{text:base.text.replace("import org.springframework.stereotype.Controller\n","import org.springframework.stereotype.Controller as Other\n"),..base.clone()},
        ] {assert!(query(&d,"@MutationMapping").is_none(),"{}",d.text);}
        for mapping in [
            "@MutationMapping(FIELD)",
            "@MutationMapping(name=\"${field}\")",
            "@MutationMapping(name=\"$field\")",
            "@MutationMapping(name=\"b\\u006fok\")",
            "@SubscriptionMapping(name=\"one\",value=\"two\")",
            "@MutationMapping(typeName=\"CustomRoot\")",
        ] {
            let d = document(mapping, true);
            assert!(query(&d, mapping).is_none(), "{mapping}");
        }
    }
    #[test]
    fn fully_qualified_controller_and_shortcut_owners_work_without_import_guessing() {
        let text = "@org.springframework.stereotype.Controller class G { @org.springframework.graphql.data.method.annotation.SubscriptionMapping(name=\"changed\") Object event(){return null;} }";
        let d = Document {
            path: "/project/G.java".into(),
            language: "java".into(),
            text: text.into(),
            version: 1,
        };
        assert!(
            GraphQlTools::default()
                .live_hover(
                    &d,
                    position(text, text.find("@org.springframework.graphql").unwrap() + 1)
                )
                .unwrap()
                .markdown
                .contains("Subscription (composed")
        );
        let shadow = Document {
            text: text.replace("class G", "class org {} class G"),
            ..d
        };
        assert!(
            GraphQlTools::default()
                .live_hover(
                    &shadow,
                    position(
                        &shadow.text,
                        shadow.text.find("@org.springframework.graphql").unwrap() + 1
                    )
                )
                .is_none()
        );
    }
}
