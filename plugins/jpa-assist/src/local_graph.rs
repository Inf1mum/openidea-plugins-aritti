//! Direct current-class graph nodes only; never a persistence/classpath absence proof.
use crate::{
    attribute_info,
    checks::{Finding, name_references},
    model::{self, Class, Kind},
    setting,
};
use openidea_plugin::{Severity, host, providers::Document};

fn scalar(class: &Class, ty: &str) -> bool {
    if let Some((prefix, _)) = ty.split_once('.') {
        if prefix == class.name
            || class
                .imports
                .iter()
                .any(|i| i.rsplit('.').next() == Some(prefix))
        {
            return false;
        }
    }
    const NATIVE: &[&str] = &[
        "String",
        "Long",
        "Integer",
        "Short",
        "Byte",
        "Boolean",
        "Double",
        "Float",
        "Character",
        "long",
        "int",
        "short",
        "byte",
        "boolean",
        "double",
        "float",
        "char",
    ];
    let simple = ty.rsplit('.').next().unwrap_or(ty);
    if simple == class.name {
        return false;
    }
    if NATIVE.contains(&ty) && ty.as_bytes().first().is_some_and(u8::is_ascii_lowercase) {
        return true;
    }
    if NATIVE.contains(&simple)
        && simple
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_uppercase)
    {
        let full = format!("java.lang.{simple}");
        if ty == full {
            return true;
        }
        return class
            .imports
            .iter()
            .filter(|i| i.rsplit('.').next() == Some(simple))
            .map(String::as_str)
            .eq(std::iter::once(full.as_str()));
    }
    let platform = [
        "java.math.BigDecimal",
        "java.math.BigInteger",
        "java.util.UUID",
        "java.time.LocalDate",
        "java.time.LocalTime",
        "java.time.LocalDateTime",
        "java.time.Instant",
    ];
    platform.iter().any(|full| {
        ty == *full
            || full.rsplit('.').next() == Some(ty)
                && class
                    .imports
                    .iter()
                    .filter(|i| i.rsplit('.').next() == Some(ty))
                    .map(String::as_str)
                    .eq(std::iter::once(*full))
    })
}
fn identifier(value: &str) -> bool {
    value.len() <= 128
        && value
            .as_bytes()
            .first()
            .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_' || *b == b'$')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$')
}
pub(super) fn source(doc: &Document) -> Option<Class> {
    if doc.language != "java" || !doc.text.contains("NamedAttributeNode") {
        return None;
    }
    let Some(class) = attribute_info::snapshot(doc) else {
        return None;
    };
    let Some(root) = host::project_of(&doc.path).and_then(|p| p.root().ok()) else {
        return None;
    };
    let Some(package) = &class.package else {
        return None;
    };
    if class.kind != Kind::Entity
        || class.superclass.is_some()
        || doc.path
            != format!(
                "{root}/src/main/java/{}/{}.java",
                package.replace('.', "/"),
                class.name
            )
        || class.attributes.is_empty()
        || class.attributes.len() > 128
        || class.attributes.iter().filter(|a| a.id).count() != 1
        || class.attributes.iter().any(|a| !scalar(&class, &a.ty))
        || class.imports.iter().any(|i| i.ends_with(".*"))
    {
        return None;
    }
    let masked = model::mask(&doc.text);
    let anns = model::annotations(&masked);
    let namespace = |a: &model::Annotation<'_>| {
        let written = &masked[a.start + 1..a.name_end];
        if written.contains(':') {
            return None;
        }
        let full = if written.contains('.') {
            written
        } else {
            class
                .imports
                .iter()
                .find(|i| i.rsplit('.').next() == Some(a.name))?
                .as_str()
        };
        if full.starts_with("jakarta.persistence.") {
            Some("jakarta.persistence")
        } else if full.starts_with("javax.persistence.") {
            Some("javax.persistence")
        } else {
            None
        }
    };
    let entities: Vec<_> = anns.iter().filter(|a| a.name == "Entity").collect();
    let [entity] = entities.as_slice() else {
        return None;
    };
    let Some(entity_namespace) = namespace(entity) else {
        return None;
    };
    if anns
        .iter()
        .any(|a| namespace(a) != Some(entity_namespace) || a.name == class.name)
    {
        return None;
    }
    let Some((class_at, _)) = model::declaration_keyword(&masked, 0, &["class"]) else {
        return None;
    };
    let Some(open) = masked[class_at..].find('{').map(|i| class_at + i) else {
        return None;
    };
    let Some(end) = class.body_end else {
        return None;
    };
    if masked[class_at + 5..open].trim() != class.name
        || anns.iter().filter(|a| a.name == "Id").count() != 1
        || anns.iter().any(|a| a.name == "Id" && a.args.is_some())
        || anns.iter().any(|a| {
            matches!(
                a.name,
                "Access"
                    | "EmbeddedId"
                    | "IdClass"
                    | "Transient"
                    | "Convert"
                    | "Converts"
                    | "AttributeOverride"
                    | "AttributeOverrides"
            ) || a.name == "Entity" && a.start >= class_at
                || matches!(
                    a.name,
                    "NamedEntityGraph"
                        | "NamedEntityGraphs"
                        | "NamedAttributeNode"
                        | "NamedSubgraph"
                ) && a.start >= class_at
        })
    {
        return None;
    }
    // Every nonempty member must be a plain parsed field. No methods, getters,
    // constructors, initializers, nested bodies or multi-field declarations.
    let mut fields = masked[open + 1..end].as_bytes().to_vec();
    for a in anns.iter().filter(|a| a.start > open && a.end() <= end) {
        fields[a.start - open - 1..a.end() - open - 1].fill(b' ');
    }
    let Ok(fields) = String::from_utf8(fields) else {
        return None;
    };
    if fields.contains(['(', ')', '{', '}', '=', ',', '<', '>', '[', ']'])
        || !fields.trim_end().ends_with(';')
    {
        return None;
    }
    let mut count = 0;
    for field in fields.split(';').filter(|s| !s.trim().is_empty()) {
        let words: Vec<_> = field
            .split_whitespace()
            .filter(|w| !matches!(*w, "public" | "protected" | "private" | "final"))
            .collect();
        let [ty, name] = words.as_slice() else {
            return None;
        };
        if !class
            .attributes
            .iter()
            .any(|a| a.name == *name && a.ty == *ty)
        {
            return None;
        }
        count += 1;
    }
    if count != class.attributes.len() {
        return None;
    }
    Some(class)
}
pub(super) fn findings(doc: &Document) -> Vec<Finding> {
    if !setting("checkMappingNames") {
        return vec![];
    }
    let Some(class) = source(doc) else {
        return vec![];
    };
    let masked = model::mask(&doc.text);
    let anns = model::annotations(&masked);
    let Some((class_at, _)) = model::declaration_keyword(&masked, 0, &["class"]) else {
        return vec![];
    };
    let unknown: Vec<_> = name_references(&doc.text,false,&class).into_iter()
        .filter(|r|r.code=="jpa-unknown-entity-graph-node" && identifier(&r.value)
            && anns.iter().any(|a|a.name=="NamedEntityGraph" && a.start<class_at && a.args.is_some_and(|(a,b)|a<=r.start && r.end<=b))
            && !class.attributes.iter().any(|a|a.name==r.value))
        .take(128)
        .collect();
    if unknown.is_empty() {
        return vec![];
    }
    let orm = crate::orm_xml::load(&doc.path);
    let fqcn = format!("{}.{}", class.package.as_deref().unwrap_or_default(), class.name);
    unknown.into_iter()
        .filter(|r| !orm.declares_attribute(&fqcn, &r.value) && !matches!(orm, crate::orm_xml::Orm::Unresolved))
        .map(|r|Finding::new(r.start,r.end,Severity::Warning,"jpa-unknown-entity-graph-node",
            format!("No source-declared persistent field '{}' in this current entity's supported field-access model; external mappings are not resolved.",r.value)))
        .collect()
}

/// Names from the supported current field-access model, never an external graph.
pub(super) fn completion(
    doc: &Document,
    at: openidea_plugin::Position,
) -> Vec<openidea_plugin::providers::CompletionItem> {
    use openidea_plugin::{
        Range,
        providers::{CompletionItem, CompletionKind},
        text::{offset, position},
    };
    let Some(class) = source(doc) else {
        return vec![];
    };
    let Some(cursor) = offset(&doc.text, at) else {
        return vec![];
    };
    let masked = model::mask(&doc.text);
    let anns = model::annotations(&masked);
    let Some((class_at, _)) = model::declaration_keyword(&masked, 0, &["class"]) else {
        return vec![];
    };
    let reference = name_references(&doc.text, false, &class)
        .into_iter()
        .find(|r| {
            r.code == "jpa-unknown-entity-graph-node"
                && r.owner == class.name
                && cursor >= r.start
                && cursor <= r.end
                && (r.value.is_empty() || identifier(&r.value))
                && anns.iter().any(|a| {
                    a.name == "NamedEntityGraph"
                        && a.start < class_at
                        && a.args.is_some_and(|(a, b)| a <= r.start && r.end <= b)
                })
        });
    let Some(reference) = reference else {
        return vec![];
    };
    let prefix = &doc.text[reference.start..cursor];
    class
        .attributes
        .iter()
        .filter(|a| a.name.starts_with(prefix))
        .take(128)
        .map(|a| CompletionItem {
            insert_text: a.name.clone(),
            replace: Some(Range::new(
                position(&doc.text, reference.start),
                position(&doc.text, reference.end),
            )),
            detail: Some(format!(
                "Source field of {}.{}; external mappings unresolved",
                class.package.as_deref().unwrap_or(""),
                class.name
            )),
            ..CompletionItem::new(&a.name, CompletionKind::Property)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::JpaAssist;
    use openidea_plugin::{diagnostics::DiagnosticsProvider, mock::MockHost};
    const SOURCE: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport java.lang.Long;\nimport java.lang.String;\nimport jakarta.persistence.NamedEntityGraph;\nimport jakarta.persistence.NamedAttributeNode;\n@Entity\n@NamedEntityGraph(name=\"summary\", attributeNodes=@NamedAttributeNode(\"missing\"))\nclass Person { @Id Long id; String name; }";
    fn doc(text: &str) -> Document {
        Document {
            path: "/project/src/main/java/shop/Person.java".into(),
            language: "java".into(),
            version: 1,
            text: text.into(),
        }
    }
    fn nodes(plugin: &mut JpaAssist, d: &Document) -> Vec<openidea_plugin::Diagnostic> {
        plugin
            .diagnostics(d)
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("jpa-unknown-entity-graph-node"))
            .collect()
    }
    #[test]
    fn inherited_property_access_unknown_type_and_annotation_owners_stay_quiet() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        for text in [
            SOURCE.replace("class Person {", "class Person extends LibraryBase {"),
            SOURCE.replace("class Person {", "class Person<T> {"),
            SOURCE.replace("@Id Long id;", "@Id Long getId() { return null; }"),
            SOURCE.replace("String name;", "Other name;"),
            SOURCE.replace("import java.lang.Long;\nimport java.lang.String;\n", ""),
            SOURCE.replace(
                "import jakarta.persistence.NamedEntityGraph;",
                "import javax.persistence.NamedEntityGraph;",
            ),
            SOURCE.replace("String name;", "String name; String name;"),
            SOURCE.replace("String name;", "String name = build();"),
            SOURCE.replace("@Id Long id;", "Long id;"),
            SOURCE.replace(
                "import jakarta.persistence.NamedAttributeNode;",
                "import foreign.NamedAttributeNode;",
            ),
            SOURCE.replace("import jakarta.persistence.NamedEntityGraph;", ""),
            SOURCE.replace("import jakarta.persistence.Entity;", ""),
            SOURCE.replace(
                "import jakarta.persistence.Id;",
                "import jakarta.persistence.Id;\nimport foreign.*;",
            ),
            SOURCE.replace(
                "@Entity\n",
                "@Entity\n@jakarta.persistence.Access(jakarta.persistence.AccessType.PROPERTY)\n",
            ),
            SOURCE.replace(
                "@Entity\n",
                "@Entity\n@jakarta.persistence.Convert(converter=LibraryConverter.class)\n",
            ),
            SOURCE.replace("package shop;", "package other;"),
            format!("{SOURCE}}}"),
            format!("{SOURCE}{}", " ".repeat(128 << 10)),
        ] {
            assert!(nodes(&mut plugin, &doc(&text)).is_empty(), "{text}");
        }
        for path in [
            "/project/Person.java",
            "/project/module/src/main/java/shop/Person.java",
            "/project/src/test/java/shop/Person.java",
        ] {
            assert!(
                nodes(
                    &mut plugin,
                    &Document {
                        path: path.into(),
                        ..doc(SOURCE)
                    }
                )
                .is_empty()
            );
        }
        assert!(
            nodes(
                &mut plugin,
                &Document {
                    language: "kotlin".into(),
                    ..doc(SOURCE)
                }
            )
            .is_empty()
        );
    }
    #[test]
    fn nested_dynamic_escaped_and_dotted_nodes_are_not_interpreted_as_local_fields() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        for value in ["name", "name.other", "${missing}"] {
            assert!(
                nodes(
                    &mut plugin,
                    &doc(&SOURCE.replace("\"missing\"", &format!("\"{value}\"")))
                )
                .is_empty()
            );
        }
        for value in ["\"mis\" + \"sing\"", "\"mis\\u0073ing\""] {
            assert!(nodes(&mut plugin, &doc(&SOURCE.replace("\"missing\"", value))).is_empty());
        }
        let nested=SOURCE.replace("import jakarta.persistence.NamedEntityGraph;","import jakarta.persistence.NamedEntityGraph;\nimport jakarta.persistence.NamedSubgraph;")
            .replace("attributeNodes=@NamedAttributeNode(\"missing\")","subgraphs=@NamedSubgraph(name=\"child\", attributeNodes=@NamedAttributeNode(\"missing\"))");
        assert!(nodes(&mut plugin, &doc(&nested)).is_empty());
        let fq = SOURCE
            .lines()
            .filter(|line| !line.starts_with("import "))
            .collect::<Vec<_>>()
            .join("\n")
            .replace("@Entity", "@jakarta.persistence.Entity")
            .replace("@Id", "@jakarta.persistence.Id")
            .replace("@NamedEntityGraph", "@jakarta.persistence.NamedEntityGraph")
            .replace(
                "@NamedAttributeNode",
                "@jakarta.persistence.NamedAttributeNode",
            )
            .replace("Long id;", "java.lang.Long id;")
            .replace("String name;", "java.lang.String name;");
        assert_eq!(
            nodes(&mut plugin, &doc(&fq)).len(),
            1,
            "explicit qualified ownership remains supported"
        );
    }
    #[test]
    fn setting_denial_and_new_nested_project_owner_disable_source_only_check() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        assert_eq!(nodes(&mut plugin, &doc(SOURCE)).len(), 1);
        let primitives = SOURCE
            .replace("import java.lang.Long;\nimport java.lang.String;\n", "")
            .replace("Long id;", "long id;")
            .replace("String name;", "int name;");
        assert_eq!(nodes(&mut plugin, &doc(&primitives)).len(), 1);
        host.set_setting("jpaAssist.checkMappingNames", "false");
        assert!(nodes(&mut plugin, &doc(SOURCE)).is_empty());
        host.set_setting("jpaAssist.checkMappingNames", "true");
        host.add_project("/project/src/main/java/shop");
        assert!(nodes(&mut plugin, &doc(SOURCE)).is_empty());
        let _host = MockHost::install().with_capabilities(&[]);
        assert!(nodes(&mut JpaAssist::default(), &doc(SOURCE)).is_empty());
    }
    #[test]
    fn current_field_access_graph_literal_completes_owned_source_attribute_names() {
        use openidea_plugin::providers::Providers;
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let text = SOURCE.replace("\"missing\"", "\"na\"");
        let document = doc(&text);
        let byte = text.find("\"na\"").unwrap() + 3;
        let items = plugin.completion(
            &document,
            openidea_plugin::text::position(&text, byte),
            None,
        );
        assert!(items.iter().any(|i| i.label == "name"));
    }
    #[test]
    fn graph_literal_completion_preserves_quotes_utf16_current_fields_and_unknown_contexts() {
        use openidea_plugin::{
            Range,
            providers::Providers,
            text::{offset, position},
        };
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = SOURCE.replace(
            "@NamedAttributeNode(\"missing\")",
            "/*😀*/ @NamedAttributeNode(\"na\")",
        );
        let document = doc(&source);
        let start = source.find("\"na\"").unwrap() + 1;
        let items = plugin.completion(&document, position(&source, start + 2), None);
        let item = items.iter().find(|i| i.label == "name").unwrap();
        assert_eq!(
            item.replace,
            Some(Range::new(
                position(&source, start),
                position(&source, start + 2)
            ))
        );
        assert_eq!(item.insert_text, "name");
        assert!(
            item.detail.as_deref().unwrap().contains("Source field")
                && item.additional_edits.is_empty()
        );
        let range = item.replace.unwrap();
        let a = offset(&source, range.start).unwrap();
        let b = offset(&source, range.end).unwrap();
        let mut after = source.clone();
        after.replace_range(a..b, &item.insert_text);
        assert!(after.contains("@NamedAttributeNode(\"name\")"));
        let current = source.replace("String name;", "String nation;");
        assert!(
            plugin
                .completion(
                    &doc(&current),
                    position(&current, current.find("\"na\"").unwrap() + 3),
                    None
                )
                .iter()
                .any(|i| i.label == "nation")
        );
        let empty = source.replace("\"na\"", "\"\"");
        let byte =
            empty.find("@NamedAttributeNode(\"\")").unwrap() + "@NamedAttributeNode(\"".len();
        let items = plugin.completion(&doc(&empty), position(&empty, byte), None);
        assert!(items.iter().any(|i| i.label == "id") && items.iter().any(|i| i.label == "name"));
        for unknown in [
            source.replace("class Person {", "class Person extends Base {"),
            source.replace("class Person {", "class Person<T> {"),
            source.replace("String name;", "LibraryType name;"),
            source.replace("import java.lang.Long;", "import java.lang.Long;\nimport vendor.java;").replace("Long id; String name;", "java.lang.Long id; java.lang.String name;"),
            source.replace("@Entity", "@foreign.Entity"),
            source.replace("import jakarta.persistence.Entity;", ""),
            source.replace("\"na\"", "\"na.child\""),
            source
                .replace(
                    "attributeNodes=",
                    "subgraphs=@jakarta.persistence.NamedSubgraph(name=\"sub\",attributeNodes=",
                )
                .replace("))\nclass", ")))\nclass"),
            source.replace(
                "@NamedAttributeNode(\"na\")",
                "@NamedAttributeNode(\"na\" + \"me\")",
            ),
        ] {
            let byte =
                unknown.find("@NamedAttributeNode(").unwrap() + "@NamedAttributeNode(\"".len();
            assert!(
                plugin
                    .completion(&doc(&unknown), position(&unknown, byte + 1), None)
                    .is_empty(),
                "{unknown}"
            );
            assert!(nodes(&mut plugin,&doc(&unknown)).is_empty(), "{unknown}");
        }
        host.set_setting("jpaAssist.checkMappingNames", "false");
        assert!(
            plugin
                .completion(&document, position(&source, start + 2), None)
                .iter()
                .any(|i| i.label == "name")
        );
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }
}
