//! Inheritance annotation rules provable from one literal class and its closed `extends` chain.
//! Jakarta Persistence 3.1: 2.12.2 (a mapped superclass is not an entity), 11.1.12 (strategy and discriminator
//! column belong to the root of a hierarchy; JOINED/SINGLE_TABLE use a discriminator) and 11.1.13 (DiscriminatorValue on an entity).
use crate::{
    attribute_info,
    checks::Finding,
    local_hierarchy::{extended, header, read, resolve, sibling_path},
    model::{self, Annotation},
    orm_xml,
};
use openidea_plugin::{Severity, providers::Document};

const MAX_HOPS: usize = 8;
const MAX_TEXT: usize = 128 * 1024;
const OWNED: [&str; 6] = ["Entity", "MappedSuperclass", "Embeddable", "Inheritance", "DiscriminatorColumn", "DiscriminatorValue"];

pub(super) fn findings(doc: &Document) -> Vec<Finding> {
    findings_with(doc, &read)
}

pub(crate) fn class_annotations<'a>(masked: &'a str, annotations: &'a [Annotation<'a>]) -> Option<Vec<&'a Annotation<'a>>> {
    let (class_at, _) = model::declaration_keyword(masked, 0, &["class"])?;
    Some(annotations.iter().filter(|a| a.start < class_at).collect())
}

pub(crate) fn owned(imports: &[String], masked: &str, a: &Annotation) -> bool {
    let known = |n: &str| n == format!("jakarta.persistence.{}", a.name) || n == format!("javax.persistence.{}", a.name);
    let written = &masked[a.start + 1..a.name_end];
    if written.contains('.') {
        return known(written);
    }
    let mut found = imports.iter().filter(|i| i.rsplit('.').next() == Some(a.name));
    matches!((found.next(), found.next()), (Some(i), None) if known(i))
}

fn literal_strategy(text: &str, masked: &str, a: &Annotation) -> Option<String> {
    let value = model::arg(text, masked, a, "strategy")?;
    let name = value.trim().rsplit('.').next()?;
    matches!(name, "SINGLE_TABLE" | "JOINED" | "TABLE_PER_CLASS").then(|| name.to_owned())
}

fn findings_with(doc: &Document, load: &dyn Fn(&str) -> Option<String>) -> Vec<Finding> {
    let masked = model::mask(&doc.text);
    let declarations = model::words(&masked, "class").filter(|&at| !masked[..at].trim_end().ends_with('.')).count();
    if doc.language != "java"
        || !doc.path.ends_with(".java")
        || doc.text.len() > MAX_TEXT
        || declarations != 1
        || ["interface", "enum", "record"].iter().any(|w| model::has_word(&masked, w))
        || !crate::live::trusted_annotations(&doc.text)
    {
        return vec![];
    }
    let all = model::annotations(&masked);
    let Some(own) = class_annotations(&masked, &all) else { return vec![] };
    let imports: Vec<String> = doc
        .text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("import "))
        .map(|r| r.trim().trim_end_matches(';').trim().to_owned())
        .collect();
    let relevant: Vec<_> = own.iter().filter(|a| OWNED.contains(&a.name)).collect();
    if relevant.iter().any(|a| !owned(&imports, &masked, a)) {
        return vec![];
    }
    let named = |n: &str| relevant.iter().find(|a| a.name == n).copied().copied();
    let range = |a: &Annotation| (a.start, a.end());
    let mut found = vec![];
    let (entity, mapped) = (named("Entity"), named("MappedSuperclass"));
    if let (Some(_), Some(mapped)) = (entity, mapped) {
        let (from, to) = range(mapped);
        found.push(Finding::new(from, to, Severity::Warning, "jpa-entity-mapped-superclass", "A class cannot be both an entity and a mapped superclass; a mapped superclass is not itself an entity (Jakarta Persistence 3.1, section 2.12.2)."));
        return found;
    }
    let inheritance = named("Inheritance");
    let discriminators: Vec<_> = ["DiscriminatorColumn", "DiscriminatorValue"].iter().filter_map(|n| named(n)).collect();
    if entity.is_none() && !discriminators.is_empty() {
        let package = doc.text.lines().find_map(|l| l.trim().strip_prefix("package ")).map(|p| p.trim().trim_end_matches(';').trim());
        let name = declaration_name(&masked);
        let (Some(package), Some(name)) = (package, name) else { return found };
        if orm_xml::load(&doc.path).may_declare(&format!("{package}.{name}")) {
            return found;
        }
        for a in discriminators {
            let (from, to) = range(a);
            found.push(Finding::new(from, to, Severity::Warning, "jpa-discriminator-not-entity", format!("@{} applies to an entity class, but this class is not annotated @Entity (Jakarta Persistence 3.1, sections 11.1.12 and 11.1.13).", a.name)));
        }
        return found;
    }
    let (Some(_), Some(inheritance)) = (entity, inheritance) else { return found };
    let Some(strategy) = literal_strategy(&doc.text, &masked, inheritance) else { return found };
    if strategy == "TABLE_PER_CLASS" {
        if let Some(column) = named("DiscriminatorColumn") {
            let (from, to) = range(column);
            found.push(Finding::new(from, to, Severity::Info, "jpa-discriminator-table-per-class", "TABLE_PER_CLASS uses no discriminator column; @DiscriminatorColumn applies to the SINGLE_TABLE and JOINED strategies (Jakarta Persistence 3.1, section 11.1.12)."));
        }
    }
    if root_strategy(doc, load).is_some_and(|root| root == strategy) {
        let (from, to) = range(inheritance);
        found.push(Finding::new(from, to, Severity::Info, "jpa-inheritance-strategy-not-root", format!("The strategy is specified on the root of an entity hierarchy (Jakarta Persistence 3.1, section 11.1.12); this subclass repeats the root's effective strategy {strategy}.")));
    }
    found
}

pub(crate) fn declaration_name(masked: &str) -> Option<&str> {
    let (at, _) = model::declaration_keyword(masked, 0, &["class"])?;
    let rest = masked[at + 5..].trim_start();
    let end = rest.bytes().take_while(|&b| model::is_ident(b)).count();
    (end > 0).then(|| &rest[..end])
}

/// The effective strategy of the nearest entity ancestor that is a proven hierarchy root, only for a fully literal closed chain.
pub(crate) fn root_strategy(doc: &Document, load: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let mut current = attribute_info::snapshot(doc)?;
    let masked = model::mask(&doc.text);
    let mut parent = resolve(&current, extended(header(&masked, &current)?)?)?;
    let mut inherited = None;
    let mut seen_entity = false;
    let mut closed = false;
    for _ in 0..MAX_HOPS {
        let path = sibling_path(&current, &parent)?;
        let ancestor = Document { path: path.clone(), language: "java".into(), text: load(&path)?, version: 0 };
        let up = attribute_info::snapshot(&ancestor)?;
        if format!("{}.{}", up.package.as_deref()?, up.name) != parent || up.kind == model::Kind::Embeddable {
            return None;
        }
        let masked = model::mask(&ancestor.text);
        let all = model::annotations(&masked);
        let own = class_annotations(&masked, &all)?;
        if up.kind == model::Kind::Entity {
            seen_entity = true;
            if let Some(a) = own.iter().find(|a| a.name == "Inheritance") {
                inherited = Some(literal_strategy(&ancestor.text, &masked, a)?);
                closed = true;
                break;
            }
        }
        let head = header(&masked, &up)?;
        if !model::has_word(head, "extends") {
            closed = true;
            break;
        }
        parent = resolve(&up, extended(head)?)?;
        current = up;
    }
    if !closed {
        return None;
    }
    inherited.or_else(|| seen_entity.then(|| "SINGLE_TABLE".to_owned()))
}

#[cfg(test)]
mod tests {
    use crate::JpaAssist;
    use openidea_plugin::{Diagnostic, Severity, diagnostics::DiagnosticsProvider, mock::MockHost, providers::Document};

    const IMPORTS: &str = "package shop;\nimport jakarta.persistence.*;\n";
    const ORM: &str = "src/main/resources/META-INF/orm.xml";

    fn imports(names: &[&str]) -> String {
        let lines: String = names.iter().map(|n| format!("import jakarta.persistence.{n};\n")).collect();
        format!("package shop;\n{lines}")
    }

    fn run(host: &MockHost, name: &str, header: &str, body: &str, code: &str) -> Vec<Diagnostic> {
        let path = format!("src/main/java/shop/{name}.java");
        let text = format!("{header}{body}");
        host.add_file(&path, &text);
        let doc = Document { path: format!("/project/{path}"), language: "java".into(), text, version: 1 };
        JpaAssist::default().diagnostics(&doc).into_iter().filter(|d| d.code.as_deref() == Some(code)).collect()
    }

    fn all(extra: &[&str]) -> String {
        let mut names = vec!["Entity", "MappedSuperclass", "Embeddable", "Inheritance", "InheritanceType", "DiscriminatorColumn", "DiscriminatorValue", "Id", "Convert"];
        names.extend_from_slice(extra);
        imports(&names)
    }

    #[test]
    fn entity_and_mapped_superclass_on_one_class_is_a_warning() {
        let host = MockHost::install();
        let found = run(&host, "Both", &all(&[]), "@Entity @MappedSuperclass class Both { @Id Long id; }", "jpa-entity-mapped-superclass");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Warning);
        assert!(run(&host, "Both", &all(&[]), "@Entity class Both { @Id Long id; }", "jpa-entity-mapped-superclass").is_empty());
        assert!(run(&host, "Both", IMPORTS, "@Entity @MappedSuperclass class Both { @Id Long id; }", "jpa-entity-mapped-superclass").is_empty());
        assert!(run(&host, "Both", &all(&[]), "@Entity @vendor.MappedSuperclass class Both { }", "jpa-entity-mapped-superclass").is_empty());
    }

    #[test]
    fn discriminator_on_a_non_entity_is_a_warning_unless_xml_may_map_it() {
        let host = MockHost::install();
        let code = "jpa-discriminator-not-entity";
        let found = run(&host, "Base", &all(&[]), "@MappedSuperclass @DiscriminatorColumn(name = \"T\") @DiscriminatorValue(\"b\") class Base { }", code);
        assert_eq!(found.len(), 2);
        assert!(run(&host, "Base", &all(&[]), "@Entity @DiscriminatorValue(\"b\") class Base { @Id Long id; }", code).is_empty());
        host.add_file(ORM, "<entity-mappings xmlns=\"x\" ><package>shop</package><entity class=\"Base\"/></entity-mappings>");
        assert!(run(&host, "Base", &all(&[]), "@MappedSuperclass @DiscriminatorValue(\"b\") class Base { }", code).is_empty());
        host.add_file(ORM, "<entity-mappings xmlns=\"x\"><entity class=\"shop.Other\"/></entity-mappings>");
        assert_eq!(run(&host, "Base", &all(&[]), "@MappedSuperclass @DiscriminatorValue(\"b\") class Base { }", code).len(), 1);
        host.add_file("src/main/resources/other-orm.xml", "<entity-mappings/>");
        assert!(run(&host, "Base", &all(&[]), "@MappedSuperclass @DiscriminatorValue(\"b\") class Base { }", code).is_empty());
    }

    #[test]
    fn table_per_class_with_a_discriminator_column_is_info() {
        let host = MockHost::install();
        let code = "jpa-discriminator-table-per-class";
        let body = |strategy: &str| format!("@Entity @Inheritance(strategy = InheritanceType.{strategy}) @DiscriminatorColumn(name = \"T\") class Root {{ @Id Long id; }}");
        let found = run(&host, "Root", &all(&[]), &body("TABLE_PER_CLASS"), code);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Info);
        assert!(run(&host, "Root", &all(&[]), &body("SINGLE_TABLE"), code).is_empty());
        assert!(run(&host, "Root", &all(&[]), &body("JOINED"), code).is_empty());
    }

    #[test]
    fn repeating_the_root_strategy_on_a_subclass_is_info_and_a_different_one_is_quiet() {
        let host = MockHost::install();
        let code = "jpa-inheritance-strategy-not-root";
        let header = all(&[]);
        run(&host, "Root", &header, "@Entity @Inheritance(strategy = InheritanceType.JOINED) class Root { @Id Long id; }", code);
        run(&host, "Plain", &header, "@Entity class Plain { @Id Long id; }", code);
        run(&host, "Mapped", &header, "@MappedSuperclass class Mapped extends Plain { }", code);
        let child = |parent: &str, strategy: &str| run(&host, "Child", &header, &format!("@Entity @Inheritance(strategy = InheritanceType.{strategy}) class Child extends {parent} {{ }}"), code);
        assert_eq!(child("Root", "JOINED").len(), 1);
        assert!(child("Root", "TABLE_PER_CLASS").is_empty());
        assert_eq!(child("Plain", "SINGLE_TABLE").len(), 1);
        assert_eq!(child("Mapped", "SINGLE_TABLE").len(), 1);
        assert!(child("Plain", "JOINED").is_empty());
        assert!(child("Unknown", "JOINED").is_empty());
        assert!(child("other.Root", "JOINED").is_empty());
        host.add_file("src/main/java/shop/Plain.java", &format!("{header}@Entity class Plain extends Library {{ @Id Long id; }}"));
        assert!(child("Plain", "SINGLE_TABLE").is_empty());
    }

    #[test]
    fn xml_identifiers_and_attributes_are_honoured() {
        let host = MockHost::install();
        let header = all(&[]);
        let hierarchy = "jpa-source-hierarchy-identifier-absent";
        run(&host, "Bare", &header, "@MappedSuperclass class Bare { String x; }", hierarchy);
        let child = |host: &MockHost| run(host, "Child", &header, "@Entity class Child extends Bare { }", hierarchy);
        assert_eq!(child(&host).len(), 1);
        let mapping = |body: &str| format!("<entity-mappings xmlns=\"https://jakarta.ee/xml/ns/persistence/orm\"><package>shop</package>{body}</entity-mappings>");
        host.add_file(ORM, &mapping("<mapped-superclass class=\"Bare\"><attributes><id name=\"key\"/></attributes></mapped-superclass>"));
        assert!(child(&host).is_empty());
        host.add_file(ORM, &mapping("<entity class=\"Child\"><id-class class=\"shop.Key\"/></entity>"));
        assert!(child(&host).is_empty());
        host.add_file(ORM, &mapping("<entity class=\"Child\"><attributes><embedded-id name=\"key\"/></attributes></entity>"));
        assert!(child(&host).is_empty());
        host.add_file(ORM, &mapping("<mapped-superclass class=\"Bare\"><attributes><basic name=\"x\"/></attributes></mapped-superclass>"));
        assert_eq!(child(&host).len(), 1);
        host.add_file(ORM, &mapping("<mapped-superclass class=\"Bare\" metadata-complete=\"true\"/>"));
        assert!(child(&host).is_empty());
        host.add_file(ORM, &mapping("<mapped-superclass class=\"Bare\"><attributes><id name=\"key\"/></attributes></mapped-superclass>"));
        host.add_file("src/main/resources/META-INF/persistence.xml", "<persistence><persistence-unit name=\"u\"><mapping-file>META-INF/missing.xml</mapping-file></persistence-unit></persistence>");
        assert!(child(&host).is_empty());
        host.add_file("src/main/resources/META-INF/missing.xml", &mapping("<entity class=\"Child\"><attributes><id name=\"k\"/></attributes></entity>"));
        assert!(child(&host).is_empty());
    }

    #[test]
    fn xml_declared_attribute_names_count_for_entity_graph_nodes() {
        let host = MockHost::install();
        let header = format!("{}import java.lang.Long;\n", imports(&["Entity", "Id", "NamedEntityGraph", "NamedAttributeNode"]));
        let body = "@Entity\n@NamedEntityGraph(name=\"summary\", attributeNodes=@NamedAttributeNode(\"extra\"))\nclass Item { @Id Long id; }";
        let code = "jpa-unknown-entity-graph-node";
        assert_eq!(run(&host, "Item", &header, body, code).len(), 1);
        let mapping = |attribute: &str| format!("<entity-mappings xmlns=\"x\"><package>shop</package><entity class=\"Item\"><attributes><basic name=\"{attribute}\"/></attributes></entity></entity-mappings>");
        host.add_file(ORM, &mapping("extra"));
        assert!(run(&host, "Item", &header, body, code).is_empty());
        host.add_file(ORM, &mapping("other"));
        assert_eq!(run(&host, "Item", &header, body, code).len(), 1);
    }

    #[test]
    fn converter_type_parameter_is_compared_with_the_attribute_type() {
        let host = MockHost::install();
        let code = "jpa-converter-type-mismatch";
        let header = all(&[]);
        let entity = |ty: &str, args: &str| run(&host, "Order", &header, &format!("@Entity class Order {{ @Id Long id; @Convert(converter = Flag.class{args}) {ty} s; }}"), code);
        let converter = |x: &str| host.add_file("src/main/java/shop/Flag.java", &format!("package shop;\nimport jakarta.persistence.AttributeConverter;\nimport jakarta.persistence.Converter;\n@Converter(autoApply = true)\nclass Flag implements AttributeConverter<{x}, String> {{ }}"));
        converter("Boolean");
        let found = entity("String", "");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(entity("long", "").len(), 1);
        for quiet in ["Boolean", "boolean", "java.lang.Boolean", "List<String>", "Custom", "Date"] {
            assert!(entity(quiet, "").is_empty(), "{quiet}");
        }
        assert!(entity("String", ", attributeName = \"x\"").is_empty());
        converter("T");
        assert!(entity("String", "").is_empty());
        converter("Date");
        assert!(entity("String", "").is_empty());
        host.add_file("src/main/java/shop/Flag.java", "package shop;\nimport java.util.Date;\nimport jakarta.persistence.AttributeConverter;\nclass Flag implements AttributeConverter<Date, String> { }");
        assert_eq!(entity("String", "").len(), 1);
    }

    #[test]
    fn converter_accepts_supertypes_of_the_attribute_type() {
        let host = MockHost::install();
        let code = "jpa-converter-type-mismatch";
        let header = format!("{}import java.io.Serializable;
", all(&[]));
        let entity = |ty: &str| run(&host, "Order", &header, &format!("@Entity class Order {{ @Id Long id; @Convert(converter = Flag.class) {ty} s; }}"), code);
        let converter = |x: &str| host.add_file("src/main/java/shop/Flag.java", &format!("package shop;\nimport java.io.Serializable;\nimport jakarta.persistence.AttributeConverter;\nclass Flag implements AttributeConverter<{x}, String> {{ }}"));
        for (converts, quiet, flagged) in [("Object", "Long", ""), ("Number", "Long", "String"), ("CharSequence", "String", "Long"), ("Serializable", "String", "Object")] {
            converter(converts);
            assert!(entity(quiet).is_empty(), "{converts} accepts {quiet}");
            if !flagged.is_empty() {
                assert_eq!(entity(flagged).len(), 1, "{converts} rejects {flagged}");
            }
        }
    }

    #[test]
    fn converter_between_project_classes_follows_the_extends_chain() {
        let host = MockHost::install();
        let code = "jpa-converter-type-mismatch";
        let header = all(&[]);
        let entity = |ty: &str| run(&host, "Order", &header, &format!("@Entity class Order {{ @Id Long id; @Convert(converter = Flag.class) {ty} s; }}"), code);
        host.add_file("src/main/java/shop/Flag.java", "package shop;\nimport jakarta.persistence.AttributeConverter;\nclass Flag implements AttributeConverter<Base, String> { }");
        host.add_file("src/main/java/shop/Base.java", "package shop;\nclass Base { }");
        host.add_file("src/main/java/shop/Child.java", "package shop;\nclass Child extends Base { }");
        host.add_file("src/main/java/shop/Other.java", "package shop;\nclass Other { }");
        host.add_file("src/main/java/shop/Contract.java", "package shop;\ninterface Contract { }");
        assert!(entity("Base").is_empty());
        assert!(entity("Child").is_empty());
        assert_eq!(entity("Other").len(), 1);
        assert!(entity("Missing").is_empty());
        host.add_file("src/main/java/shop/Child.java", "package shop;\nimport other.Parent;\nclass Child extends Parent { }");
        assert!(entity("Child").is_empty());
        host.add_file("src/main/java/shop/Flag.java", "package shop;\nimport jakarta.persistence.AttributeConverter;\nclass Flag implements AttributeConverter<Contract, String> { }");
        assert!(entity("Other").is_empty());
    }
}
