//! Background document indexing of attribute names: `mappedBy`, MapStruct `@Mapping` literals and Hibernate
//! `@Type(type = "...")` literals become references to the field (or class) they name, so the host can answer
//! Go to Declaration, Find Usages and Rename across files.
//!
//! A document is indexed alone, so every key is built from what that document proves: a type spelled in it is
//! resolved by a single-type import, a qualified name, or its own package (which wins over on-demand imports).
//! A reference resolves only when some other document defines exactly that key; XML mappings, library types,
//! inherited attributes and anything unproven publish nothing, and Kotlin sources are not read.
use crate::file_actions::mapstruct_model::{Imports, MAX_TEXT, fields, first_segment, parse_mapper, string_arguments, type_decls};
use crate::model::{Annotation, annotations, mask};
use crate::JpaAssist;
use openidea_plugin::indexing::{Anchor, Document, DocumentIndexer, Edges};
use openidea_plugin::text::position;
use openidea_plugin::Range;

/// Definitions plus references one document may publish; the host's own limit is 4,096 and 256.
const MAX_ANCHORS: usize = 4000;
const JPA: [&str; 2] = ["jakarta.persistence", "javax.persistence"];
const ASSOCIATIONS: [&str; 4] = ["OneToMany", "ManyToMany", "OneToOne", "ManyToOne"];
const COLLECTIONS: [&str; 4] = ["List", "Set", "Collection", "SortedSet"];

fn attribute_key(owner: &str, name: &str) -> String {
    format!("jpa-assist:attr:{owner}#{name}")
}

fn class_key(qualified: &str) -> String {
    format!("jpa-assist:class:{qualified}")
}

fn range(text: &str, from: usize, to: usize) -> Range {
    Range::new(position(text, from), position(text, to))
}

/// The entity type a `mappedBy` field points at: its type, or the single argument of a plain collection.
fn target_type(ty: &str) -> Option<&str> {
    let ty = ty.trim();
    let bare = ty.strip_prefix("java.util.").unwrap_or(ty);
    let wrapped = COLLECTIONS.iter().find_map(|name| bare.strip_prefix(name)?.trim_start().strip_prefix('<'));
    let candidate = match wrapped {
        Some(rest) => rest.strip_suffix('>')?.trim(),
        None => ty,
    };
    (!candidate.is_empty() && !candidate.contains(['<', '>', '?', ',', ' ', '[', '&'])).then_some(candidate)
}

pub fn index(path: &str, text: &str) -> Edges {
    let mut edges = Edges::default();
    if !path.ends_with(".java") || text.len() > MAX_TEXT {
        return edges;
    }
    let masked = mask(text);
    let imports = Imports::read(&masked);
    let all = annotations(&masked);
    let (tops, nested) = type_decls(&masked);
    let qualified = |name: &str| imports.package.as_ref().map_or_else(|| name.to_owned(), |package| format!("{package}.{name}"));

    for top in tops.iter().filter(|top| matches!(top.kind.as_str(), "class" | "record")) {
        let owner = qualified(&top.name);
        edges.definitions.push(Anchor { key: class_key(&owner), range: range(text, top.name_span.0, top.name_span.1) });
        for field in fields(&masked, top, &all) {
            edges.definitions.push(Anchor { key: attribute_key(&owner, &field.name), range: range(text, field.name_span.0, field.name_span.1) });
            for a in all.iter().filter(|a| a.start >= field.member.0 && a.end() <= field.member.1) {
                if let Some(reference) = mapped_by(text, &masked, &imports, &nested, a, &field.ty) {
                    edges.references.push(reference);
                }
            }
        }
    }

    for a in all.iter().filter(|a| imports.owns(&masked, a, &["org.hibernate.annotations"], "Type")) {
        let literal = string_arguments(text, &masked, a).into_iter().find(|(key, _)| key == "type").and_then(|(_, literal)| literal);
        let Some(literal) = literal.filter(|literal| literal.value.split('.').all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))) else { continue };
        let simple_start = literal.end - literal.value.rsplit('.').next().map_or(0, str::len);
        edges.references.push(Anchor { key: class_key(&literal.value), range: range(text, simple_start, literal.end) });
    }

    if let Some(mapper) = parse_mapper(text) {
        for method in &mapper.methods {
            let (Some((source, target)), Some(param)) = (method.bean_types(), method.source_param()) else { continue };
            let (source, target) = (mapper.resolve(source), mapper.resolve(target));
            for mapping in &method.mappings {
                let sides = [(&mapping.source, &source, true), (&mapping.target, &target, false)];
                for (literal, owner, is_source) in sides {
                    let (Some(literal), Some(owner)) = (literal, owner) else { continue };
                    let Some((segment, from, to)) = first_segment(literal) else { continue };
                    if is_source && segment == param.name {
                        continue;
                    }
                    edges.references.push(Anchor { key: attribute_key(owner, segment), range: range(text, from, to) });
                }
            }
        }
    }

    if edges.definitions.len() + edges.references.len() > MAX_ANCHORS {
        return Edges::default();
    }
    edges
}

fn mapped_by(text: &str, masked: &str, imports: &Imports, nested: &[String], a: &Annotation<'_>, field_type: &str) -> Option<Anchor> {
    if !ASSOCIATIONS.iter().any(|name| imports.owns(masked, a, &JPA, name)) {
        return None;
    }
    let arguments = string_arguments(text, masked, a);
    if arguments.iter().any(|(key, _)| key == "targetEntity") {
        return None;
    }
    let literal = arguments.into_iter().find(|(key, _)| key == "mappedBy").and_then(|(_, literal)| literal)?;
    let (segment, from, to) = first_segment(&literal)?;
    if segment.len() != literal.value.len() {
        return None;
    }
    let owner = imports.resolve(target_type(field_type)?, nested)?;
    Some(Anchor { key: attribute_key(&owner, segment), range: range(text, from, to) })
}

impl DocumentIndexer for JpaAssist {
    fn index(&mut self, doc: &Document) -> Edges {
        index(&doc.path, &doc.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(edges: &Edges) -> (Vec<String>, Vec<String>) {
        (edges.definitions.iter().map(|a| a.key.clone()).collect(), edges.references.iter().map(|a| a.key.clone()).collect())
    }

    fn word<'a>(text: &'a str, anchor: &Anchor) -> &'a str {
        let line = text.lines().nth(anchor.range.start.line as usize).unwrap();
        &line[anchor.range.start.character as usize..anchor.range.end.character as usize]
    }

    const ORDER: &str = "package shop;\nimport jakarta.persistence.*;\nimport java.util.List;\n@Entity\npublic class Order {\n    @Id Long id;\n    @ManyToOne(fetch = FetchType.LAZY)\n    Customer customer;\n}\n";
    const CUSTOMER: &str = "package shop.crm;\nimport jakarta.persistence.*;\nimport shop.Order;\nimport java.util.List;\n@Entity\npublic class Customer {\n    @OneToMany(mappedBy = \"customer\")\n    List<Order> orders;\n    @OneToMany(mappedBy = \"other\", targetEntity = Order.class)\n    List<Order> unsure;\n}\n";

    #[test]
    fn fields_are_defined_and_mapped_by_points_at_the_attribute_of_the_proved_target() {
        let order = index("/p/Order.java", ORDER);
        let (definitions, _) = keys(&order);
        assert!(definitions.contains(&"jpa-assist:attr:shop.Order#customer".to_owned()));
        assert!(definitions.contains(&"jpa-assist:class:shop.Order".to_owned()));
        assert_eq!(word(ORDER, order.definitions.iter().find(|a| a.key.ends_with("#customer")).unwrap()), "customer");
        let customer = index("/p/Customer.java", CUSTOMER);
        let (_, references) = keys(&customer);
        assert_eq!(references, ["jpa-assist:attr:shop.Order#customer"]);
        assert_eq!(word(CUSTOMER, &customer.references[0]), "customer");
    }

    #[test]
    fn mapped_by_stays_silent_for_unproved_targets() {
        let wild = CUSTOMER.replace("import shop.Order;\n", "import shop.*;\n");
        let (_, references) = keys(&index("/p/Customer.java", &wild));
        assert_eq!(references, ["jpa-assist:attr:shop.crm.Order#customer"], "same-package key resolves to nothing without a definition there");
        let map = CUSTOMER.replace("List<Order> orders", "Map<String, Order> orders");
        assert!(keys(&index("/p/Customer.java", &map)).1.is_empty());
        let foreign = CUSTOMER.replace("import jakarta.persistence.*;", "import other.*;");
        assert!(keys(&index("/p/Customer.java", &foreign)).1.is_empty());
        let nested = CUSTOMER.replace("public class Customer {", "public class Customer {\n    static class Order {}");
        assert!(keys(&index("/p/Customer.java", &nested)).1.is_empty());
        assert!(index("/p/Customer.kt", CUSTOMER).references.is_empty());
    }

    const MAPPER: &str = "package shop.web;\nimport org.mapstruct.*;\nimport shop.Order;\n@Mapper\npublic interface OrderMapper {\n    @Mapping(source = \"customer.name\", target = \"owner\")\n    @Mapping(source = \"source\", target = \"x\")\n    OrderDto toDto(Order source);\n    @Mapping(target = \"owner\", source = \"a\")\n    OrderDto two(Order first, Order second);\n}\n";

    #[test]
    fn mapping_literals_reference_the_first_segment_on_the_provably_typed_side() {
        let edges = index("/p/OrderMapper.java", MAPPER);
        let (_, references) = keys(&edges);
        assert_eq!(references, ["jpa-assist:attr:shop.Order#customer", "jpa-assist:attr:shop.web.OrderDto#owner", "jpa-assist:attr:shop.web.OrderDto#x"]);
        assert_eq!(word(MAPPER, &edges.references[0]), "customer");
        assert_eq!(word(MAPPER, &edges.references[1]), "owner");
    }

    #[test]
    fn hibernate_type_literal_points_at_the_class_name_only() {
        let text = "package shop;\nimport org.hibernate.annotations.Type;\npublic class A {\n    @Type(type = \"shop.types.MoneyType\")\n    Object price;\n    @Type(type = \"a\" + \"b\") Object c;\n}\n";
        let edges = index("/p/A.java", text);
        let types: Vec<_> = edges.references.iter().collect();
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].key, "jpa-assist:class:shop.types.MoneyType");
        assert_eq!(word(text, types[0]), "MoneyType");
        let own = index("/p/MoneyType.java", "package shop.types;\npublic class MoneyType {}\n");
        assert_eq!(own.definitions[0].key, "jpa-assist:class:shop.types.MoneyType");
    }

    #[test]
    fn oversized_or_non_java_documents_publish_nothing_and_anchor_counts_are_bounded() {
        assert!(index("/p/A.txt", ORDER).definitions.is_empty());
        let big = format!("package a;\nclass A {{\n{}}}\n", (0..4100).map(|i| format!("int f{i};\n")).collect::<String>());
        let edges = index("/p/A.java", &big);
        assert!(edges.definitions.is_empty() && edges.references.is_empty());
        let huge = "x".repeat(MAX_TEXT + 1);
        assert!(index("/p/A.java", &huge).definitions.is_empty());
    }

    #[test]
    fn utf16_columns_are_used() {
        let text = "package a;\nimport jakarta.persistence.*;\nclass A {\n    /* 😀 */ @ManyToOne B bb;\n}\nclass B {}\n";
        let edges = index("/p/A.java", text);
        let anchor = edges.definitions.iter().find(|a| a.key.ends_with("#bb")).unwrap();
        let line = text.lines().nth(3).unwrap();
        let expected: usize = line[..line.find("bb;").unwrap()].encode_utf16().count();
        assert_eq!(anchor.range.start.character as usize, expected);
    }
}
