//! String-literal references that must resolve (`mappedBy`, MapStruct `@Mapping`), each only on positive evidence:
//! the referenced classes are read from the same source root through exact imports or the package, and anything the
//! scan cannot prove (wildcard imports, library types, XML mappings, inherited attributes, annotation processors that
//! add properties) keeps the rule silent. Child of `live_project`, whose resolution helpers it reuses.
use super::{attribute_names, file_of, identifier, member_ranges, read_class, resolve_in, source_root, statements};
use crate::{
    attribute_info,
    checks::Finding,
    local_hierarchy, model,
    model::{Kind, Relation},
    orm_xml::{self, Orm},
    setting,
};
use openidea_plugin::{Severity, providers::Document, text::offset};
use std::collections::HashSet;

pub(super) fn mapped_by(doc: &Document) -> Vec<Finding> {
    if !setting("checkMappedBy") || !doc.text.contains("mappedBy") {
        return vec![];
    }
    let Some(class) = attribute_info::snapshot(doc) else { return vec![] };
    let Some(package) = class.package.as_deref() else { return vec![] };
    let Some(root) = source_root(&class.path, package) else { return vec![] };
    let orm = orm_xml::load(&doc.path);
    if !matches!(orm, Orm::Absent) {
        return vec![];
    }
    let masked = model::mask(&doc.text);
    let annotations = model::annotations(&masked);
    let ranges = member_ranges(&masked, &class);
    let mut found = vec![];
    for attribute in class
        .attributes
        .iter()
        .filter(|a| a.mapped_by && matches!(a.relation, Relation::OneToMany | Relation::OneToOne | Relation::ManyToMany))
        .take(64)
    {
        let Some(start) = offset(&doc.text, attribute.pos) else { continue };
        let Some(&(from, to)) = ranges.iter().find(|&&(from, to)| from <= start && start < to) else { continue };
        if model::has_word(&masked[from..to], "targetEntity") {
            continue;
        }
        let Some(association) = annotations
            .iter()
            .find(|a| from <= a.start && a.start < to && matches!(a.name, "OneToMany" | "OneToOne" | "ManyToMany"))
        else {
            continue;
        };
        let Some(range) = model::value_range(&doc.text, &masked, association, "mappedBy") else { continue };
        let Some((at, value)) = model::plain_literal(&doc.text, range, false) else { continue };
        if !identifier(value) {
            continue;
        }
        let Some(fqcn) = resolve_in(package, &class.imports, &attribute.target) else { continue };
        if !read_class(&root, "java", &fqcn).is_some_and(|(target, _)| target.kind == Kind::Entity) {
            continue;
        }
        let Some(names) = attribute_names(&class.path, package, &class.imports, &attribute.target, &orm) else { continue };
        if names.iter().any(|name| name == value) {
            continue;
        }
        found.push(Finding::new(
            at,
            at + value.len(),
            Severity::Warning,
            "jpa-unknown-mapped-by",
            format!("{} has no attribute named {value}, so mappedBy cannot point at the owning side.", attribute.target),
        ));
    }
    found
}

const MAPPING_PARAMETER_MARKERS: &[&str] = &["MappingTarget", "Context", "TargetType"];

pub(super) fn mapstruct(doc: &Document) -> Vec<Finding> {
    if !setting("checkMapStructMappings") || !doc.text.contains("@Mapping") || doc.text.len() > super::MAX_TEXT {
        return vec![];
    }
    let masked = model::mask(&doc.text);
    let package = statements(&doc.text, "package ").into_iter().next();
    let imports = statements(&doc.text, "import ");
    let owned = |name: &str| imports.iter().filter(|i| i.rsplit('.').next() == Some(name)).map(String::as_str).eq([format!("org.mapstruct.{name}").as_str()]);
    let (Some(package), true, true) = (package, owned("Mapper"), owned("Mapping")) else { return vec![] };
    let Some(root) = source_root(&doc.path, &package) else { return vec![] };
    if model::words(&masked, "class").next().is_some() || model::words(&masked, "interface").count() != 1 {
        return vec![];
    }
    let Some((interface_at, _)) = model::declaration_keyword(&masked, 0, &["interface"]) else { return vec![] };
    let annotations = model::annotations(&masked);
    if !annotations.iter().any(|a| a.name == "Mapper" && a.start < interface_at) {
        return vec![];
    }
    let Some(open) = masked[interface_at..].find('{').map(|p| p + interface_at) else { return vec![] };
    let Some(close) = model::closing(masked.as_bytes(), open) else { return vec![] };
    let mut found = vec![];
    let mut cache: Vec<(String, Option<HashSet<String>>)> = vec![];
    let mut properties = |fqcn: String| -> Option<HashSet<String>> {
        if let Some((_, known)) = cache.iter().find(|(name, _)| *name == fqcn) {
            return known.clone();
        }
        let known = property_names(&root, &fqcn);
        cache.push((fqcn, known.clone()));
        known
    };
    for (start, end) in model::members(&masked, open + 1, close, false, false).into_iter().take(128) {
        let mappings: Vec<_> = annotations.iter().filter(|a| a.name == "Mapping" && start <= a.start && a.start < end).collect();
        if mappings.is_empty() || annotations.iter().any(|a| MAPPING_PARAMETER_MARKERS.contains(&a.name) && start <= a.start && a.start < end) {
            continue;
        }
        let decl = model::without_annotations(&masked, start, end);
        let Some(paren) = decl.find('(') else { continue };
        let Some(close_paren) = model::closing(decl.as_bytes(), paren) else { continue };
        let head: Vec<_> = decl[..paren].split_whitespace().collect();
        let parameter: Vec<_> = decl[paren + 1..close_paren].split_whitespace().collect();
        let ([return_type, _], [parameter_type, parameter_name], ";") = (head.as_slice(), parameter.as_slice(), decl[close_paren + 1..].trim()) else { continue };
        if ![return_type, parameter_type, parameter_name].into_iter().all(|t| identifier(t)) {
            continue;
        }
        let source = resolve_in(&package, &imports, parameter_type).and_then(&mut properties);
        let target = resolve_in(&package, &imports, return_type).and_then(&mut properties);
        for (key, type_name, known) in [("source", *parameter_type, &source), ("target", *return_type, &target)] {
            let Some(known) = known else { continue };
            for mapping in &mappings {
                let Some(range) = model::value_range(&doc.text, &masked, mapping, key) else { continue };
                let Some((at, value)) = model::plain_literal(&doc.text, range, false) else { continue };
                if !identifier(value) || value == *parameter_name || known.contains(value) {
                    continue;
                }
                found.push(Finding::new(
                    at,
                    at + value.len(),
                    Severity::Warning,
                    "jpa-unknown-mapstruct-property",
                    format!("{type_name} has no property named {value}, so MapStruct cannot use it as the {key} of this mapping."),
                ));
            }
        }
    }
    found
}

/// Every name a MapStruct property of the class could have: all identifiers of a plain top-level class plus the bean
/// names behind its `get`/`set`/`is` identifiers. `None` unless the class is closed (no supertypes, generics,
/// wildcard imports, or Lombok features that rename or borrow members).
fn property_names(root: &str, fqcn: &str) -> Option<HashSet<String>> {
    let (package, name) = fqcn.rsplit_once('.')?;
    let text = local_hierarchy::read(&file_of(root, fqcn, "java"))?;
    if text.len() > super::MAX_TEXT {
        return None;
    }
    let masked = model::mask(&text);
    let keywords_ok = ["interface", "enum", "record"].iter().all(|w| model::words(&masked, w).next().is_none());
    let (class_at, _) = model::declaration_keyword(&masked, 0, &["class"])?;
    let header = masked[class_at + "class".len()..].split('{').next()?;
    let declared_package = statements(&text, "package ").into_iter().next()?;
    let closed = header.trim() == name
        && declared_package == package
        && keywords_ok
        && model::words(&masked, "class").count() == 1
        && !statements(&text, "import ").iter().any(|i| i.ends_with(".*"))
        && !["Delegate", "Accessors"].iter().any(|w| model::has_word(&masked, w));
    if !closed {
        return None;
    }
    let mut names = HashSet::new();
    let bytes = masked.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !model::is_ident(bytes[i]) {
            i += 1;
            continue;
        }
        let from = i;
        while i < bytes.len() && model::is_ident(bytes[i]) {
            i += 1;
        }
        let word = &masked[from..i];
        names.insert(word.to_owned());
        for prefix in ["get", "set", "is"] {
            if let Some(rest) = word.strip_prefix(prefix).filter(|r| r.starts_with(|c: char| c.is_ascii_uppercase())) {
                names.insert(bean_name(rest));
            }
        }
    }
    Some(names)
}

fn bean_name(rest: &str) -> String {
    let mut chars = rest.chars();
    match (chars.next(), chars.next()) {
        (Some(_), Some(second)) if second.is_ascii_uppercase() => rest.to_owned(),
        (Some(first), _) => format!("{}{}", first.to_ascii_lowercase(), &rest[1..]),
        _ => rest.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use crate::JpaAssist;
    use openidea_plugin::{Severity, diagnostics::DiagnosticsProvider, mock::MockHost, providers::Document};

    fn run(host: &MockHost, path: &str, text: &str, code: &str) -> Vec<(Severity, String)> {
        host.add_file(path, text);
        let doc = Document { path: format!("/project/{path}"), language: "java".into(), text: text.into(), version: 1 };
        JpaAssist::default().diagnostics(&doc).into_iter().filter(|d| d.code.as_deref() == Some(code)).map(|d| (d.severity, d.message)).collect()
    }

    const OWNER: &str = "src/main/java/shop/Customer.java";
    const HEAD: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.ManyToOne;\nimport jakarta.persistence.MappedSuperclass;\nimport jakarta.persistence.OneToMany;\nimport java.util.List;\n";

    fn owner(mapped_by: &str) -> String {
        format!("{HEAD}@Entity\npublic class Customer {{\n    @Id Long id;\n    @OneToMany(mappedBy = \"{mapped_by}\") List<Order> orders;\n}}\n")
    }

    fn order(extends: &str, body: &str) -> String {
        format!("{HEAD}@Entity\npublic class Order {extends}{{\n    @Id Long id;\n{body}\n}}\n")
    }

    fn with_order(extends: &str, body: &str) -> MockHost {
        let host = MockHost::install();
        host.add_file("src/main/java/shop/Order.java", &order(extends, body));
        host
    }

    #[test]
    fn mapped_by_naming_no_attribute_of_the_target_warns_on_the_literal() {
        let host = with_order("", "    @ManyToOne Customer customer;");
        let text = owner("custmer");
        let found = run(&host, OWNER, &text, "jpa-unknown-mapped-by");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, Severity::Warning);
        assert!(found[0].1.contains("custmer"), "{}", found[0].1);
        assert!(run(&host, OWNER, &owner("customer"), "jpa-unknown-mapped-by").is_empty());
    }

    #[test]
    fn mapped_by_accepts_an_attribute_inherited_through_a_project_mapped_superclass() {
        let host = with_order("extends Base ", "");
        host.add_file("src/main/java/shop/Base.java", &format!("{HEAD}@MappedSuperclass\npublic class Base {{\n    @ManyToOne Customer customer;\n}}\n"));
        assert!(run(&host, OWNER, &owner("customer"), "jpa-unknown-mapped-by").is_empty());
        assert_eq!(run(&host, OWNER, &owner("custmer"), "jpa-unknown-mapped-by").len(), 1);
    }

    #[test]
    fn mapped_by_stays_silent_when_the_target_or_its_attributes_are_unproven() {
        let wrong = owner("nothing");
        assert!(run(&MockHost::install(), OWNER, &wrong, "jpa-unknown-mapped-by").is_empty());
        let library = with_order("extends vendor.Base ", "");
        assert!(run(&library, OWNER, &wrong, "jpa-unknown-mapped-by").is_empty());
        let unknown_parent = with_order("extends Missing ", "");
        assert!(run(&unknown_parent, OWNER, &wrong, "jpa-unknown-mapped-by").is_empty());
        let wildcard = with_order("", "");
        assert!(run(&wildcard, OWNER, &wrong.replace("import java.util.List;\n", "import java.util.List;\nimport vendor.*;\n"), "jpa-unknown-mapped-by").is_empty());
        let mapped = with_order("", "");
        mapped.add_file("src/main/resources/META-INF/orm.xml", "<entity-mappings/>");
        assert!(run(&mapped, OWNER, &wrong, "jpa-unknown-mapped-by").is_empty());
        let dotted = with_order("", "");
        assert!(run(&dotted, OWNER, &owner("embedded.customer"), "jpa-unknown-mapped-by").is_empty());
        let explicit = with_order("", "");
        assert!(run(&explicit, OWNER, &wrong.replace("@OneToMany(", "@OneToMany(targetEntity = Order.class, "), "jpa-unknown-mapped-by").is_empty());
        let not_entity = MockHost::install();
        not_entity.add_file("src/main/java/shop/Order.java", "package shop;\npublic class Order {\n}\n");
        assert!(run(&not_entity, OWNER, &wrong, "jpa-unknown-mapped-by").is_empty());
        let off = with_order("", "");
        off.set_setting("jpaAssist.checkMappedBy", "false");
        assert!(run(&off, OWNER, &wrong, "jpa-unknown-mapped-by").is_empty());
    }

    const MAPPER: &str = "src/main/java/shop/OrderMapper.java";

    fn mapper(methods: &str) -> String {
        format!("package shop;\nimport org.mapstruct.Mapper;\nimport org.mapstruct.Mapping;\nimport org.mapstruct.MappingTarget;\n\n@Mapper\npublic interface OrderMapper {{\n{methods}\n}}\n")
    }

    const TO_DTO: &str = "    @Mapping(source = \"{source}\", target = \"{target}\")\n    OrderDto toDto(Order order);";

    fn with_types(entity_body: &str, dto_body: &str) -> MockHost {
        let host = MockHost::install();
        host.add_file("src/main/java/shop/Order.java", &format!("package shop;\npublic class Order {{\n{entity_body}\n}}\n"));
        host.add_file("src/main/java/shop/OrderDto.java", &format!("package shop;\npublic class OrderDto {{\n{dto_body}\n}}\n"));
        host
    }

    const ENTITY: &str = "    private String number;\n    public String getNumber() { return number; }";
    const DTO: &str = "    private String label;\n    public void setLabel(String label) { this.label = label; }";

    fn mapping(source: &str, target: &str) -> String {
        mapper(&TO_DTO.replace("{source}", source).replace("{target}", target))
    }

    #[test]
    fn mapstruct_source_and_target_names_that_match_no_property_of_the_plain_classes_warn() {
        let host = with_types(ENTITY, DTO);
        let both = run(&host, MAPPER, &mapping("numbr", "lable"), "jpa-unknown-mapstruct-property");
        assert_eq!(both.len(), 2);
        assert!(both.iter().all(|(severity, _)| *severity == Severity::Warning));
        assert!(both[0].1.contains("numbr") && both[1].1.contains("lable"), "{both:?}");
        assert!(run(&host, MAPPER, &mapping("number", "label"), "jpa-unknown-mapstruct-property").is_empty());
    }

    #[test]
    fn mapstruct_stays_silent_when_a_type_or_mapping_is_not_provable() {
        let wrong = mapping("numbr", "lable");
        assert!(run(&MockHost::install(), MAPPER, &wrong, "jpa-unknown-mapstruct-property").is_empty());
        let inherits = with_types(ENTITY, DTO);
        inherits.add_file("src/main/java/shop/Order.java", &format!("package shop;\npublic class Order extends Base {{\n{ENTITY}\n}}\n"));
        assert_eq!(run(&inherits, MAPPER, &wrong, "jpa-unknown-mapstruct-property").len(), 1, "only the target is still proven");
        let lombok = with_types(ENTITY, DTO);
        lombok.add_file("src/main/java/shop/OrderDto.java", &format!("package shop;\nimport lombok.*;\npublic class OrderDto {{\n{DTO}\n}}\n"));
        assert_eq!(run(&lombok, MAPPER, &wrong, "jpa-unknown-mapstruct-property").len(), 1, "only the source is still proven");
        let host = with_types(ENTITY, DTO);
        for silent in [
            mapping("order.numbr", "label"),
            mapping("order", "label.x"),
            mapping("number", "."),
            mapper("    @Mapping(target = \"lable\", source = \"number\")\n    void update(@MappingTarget OrderDto dto, Order order);"),
            mapper("    @Mapping(source = \"numbr\", target = \"label\")\n    OrderDto toDto(Order order, String other);"),
            mapper("    @Mapping(source = \"numbr\", target = \"label\")\n    java.util.List<OrderDto> toDtos(Order order);"),
        ] {
            assert!(run(&host, MAPPER, &silent, "jpa-unknown-mapstruct-property").is_empty(), "{silent}");
        }
        let foreign = wrong.replace("org.mapstruct.Mapping;", "vendor.Mapping;");
        assert!(run(&host, MAPPER, &foreign, "jpa-unknown-mapstruct-property").is_empty());
        let wildcard = wrong.replace("import org.mapstruct.Mapper;", "import org.mapstruct.Mapper;\nimport vendor.*;");
        assert!(run(&host, MAPPER, &wildcard, "jpa-unknown-mapstruct-property").is_empty());
        host.set_setting("jpaAssist.checkMapStructMappings", "false");
        assert!(run(&host, MAPPER, &wrong, "jpa-unknown-mapstruct-property").is_empty());
    }
}
