//! Spec-derived live inspections over one closed, source-owned class (see `attribute_info::snapshot`):
//! every annotation must be a persistence annotation with exact ownership, otherwise the class stays quiet.
//! No inherited, XML or external mappings are resolved; messages say so where it matters.
//!
//! Sources:
//! - Jakarta Persistence 3.2 section 2.1: public/protected no-arg constructor, non-final methods and persistent instance variables.
//! - `jakarta.persistence.GenerationType`: IDENTITY/SEQUENCE/TABLE apply to `long`/`int`/`Long`/`Integer`, UUID to `UUID`/`String`.
//! - `jakarta.persistence.GeneratedValue`: applied to a primary key property or field.
//! - `jakarta.persistence.Column`: `length` only for length-parameterised types; `precision`/`scale` only for exact numeric types.
//! - Hibernate ORM 6.6 User Guide, `@OneToMany`: a unidirectional association uses a link table; a bidirectional one is much more efficient.
//! - Hibernate ORM 6.6 User Guide 3.8.4 `@ManyToMany`: for `@ManyToMany`, cascading REMOVE (so also ALL) does not make sense because it propagates beyond the link table
//!   and can end in a ConstraintViolationException; its lifecycle example removes one element of a `List` and Hibernate deletes every link row of the owner, then re-inserts the rest.
//! - Hibernate ORM 6.6 User Guide 29.4.2 Associations: "For unidirectional collections, Sets are the best choice"; `@ManyToMany` treats both sides as unidirectional.
use crate::{
    attribute_info,
    checks::Finding,
    model::{self, Annotation, Class, Kind},
    setting,
};
use openidea_plugin::{Severity, providers::Document, text::offset};

#[derive(Clone, Copy, PartialEq)]
enum Scalar {
    Integral,
    Numeric,
    Decimal,
    Text,
    Uuid,
    Flag,
    Character,
}

fn known_scalar(name: &str, kotlin: bool) -> Option<(&'static str, Scalar)> {
    use Scalar::*;
    let (package, scalar) = match (kotlin, name) {
        (true, "Long" | "Int") => ("kotlin", Integral),
        (true, "Short" | "Byte" | "Double" | "Float") => ("kotlin", Numeric),
        (true, "Boolean") => ("kotlin", Flag),
        (true, "Char") => ("kotlin", Character),
        (true, "String") => ("kotlin", Text),
        (false, "Long" | "Integer") => ("java.lang", Integral),
        (false, "Short" | "Byte" | "Double" | "Float") => ("java.lang", Numeric),
        (false, "Boolean") => ("java.lang", Flag),
        (false, "Character") => ("java.lang", Character),
        (false, "String") => ("java.lang", Text),
        (_, "UUID") => ("java.util", Uuid),
        (_, "BigDecimal" | "BigInteger") => ("java.math", Decimal),
        _ => return None,
    };
    Some((package, scalar))
}

fn scalar(class: &Class, ty: &str) -> Option<Scalar> {
    use Scalar::*;
    let ty = ty.trim();
    if !class.kotlin {
        match ty {
            "long" | "int" => return Some(Integral),
            "short" | "byte" | "double" | "float" => return Some(Numeric),
            "boolean" => return Some(Flag),
            "char" => return Some(Character),
            _ => {}
        }
    }
    let simple = ty.rsplit('.').next()?;
    let (package, scalar) = known_scalar(simple, class.kotlin)?;
    if ty.contains('.') {
        return (ty == format!("{package}.{simple}")).then_some(scalar);
    }
    if class.name == simple {
        return None;
    }
    let imports: Vec<_> = class.imports.iter().filter(|i| i.rsplit('.').next() == Some(simple)).collect();
    let owned_by_default = matches!(package, "java.lang" | "kotlin");
    match imports.as_slice() {
        [] => owned_by_default.then_some(scalar),
        [import] => (**import == format!("{package}.{simple}")).then_some(scalar),
        _ => None,
    }
}

struct Member<'a> {
    start: usize,
    decl: String,
    annotations: Vec<&'a Annotation<'a>>,
}

impl Member<'_> {
    fn has(&self, name: &str) -> bool {
        self.annotations.iter().any(|a| a.name == name)
    }

    fn named(&self, name: &str) -> Option<&Annotation<'_>> {
        self.annotations.iter().copied().find(|a| a.name == name)
    }

    fn head(&self) -> &str {
        let stop = self.decl.find(['(', '=', '{', ';']).unwrap_or(self.decl.len());
        &self.decl[..stop]
    }

    fn opens_parameters(&self) -> bool {
        self.decl.as_bytes().get(self.head().len()) == Some(&b'(')
    }
}

pub(super) fn findings(doc: &Document) -> Vec<Finding> {
    let Some(class) = attribute_info::snapshot(doc) else {
        return vec![];
    };
    let masked = model::mask(&doc.text);
    let annotations = model::annotations(&masked);
    let Some((class_at, _)) = model::declaration_keyword(&masked, 0, &["class"]) else {
        return vec![];
    };
    let (Some(body_end), Some(open)) = (class.body_end, masked[class_at..].find('{').map(|p| p + class_at)) else {
        return vec![];
    };
    let mut ranges = vec![];
    if class.kotlin
        && let Some(paren) = masked[class_at..open].find('(').map(|p| p + class_at)
        && let Some(close) = model::closing(masked.as_bytes(), paren)
    {
        ranges.extend(model::members(&masked, paren + 1, close, true, true));
    }
    ranges.extend(model::members(&masked, open + 1, body_end, class.kotlin, false));
    let members: Vec<Member> = ranges
        .into_iter()
        .map(|(start, end)| Member {
            start,
            decl: model::without_annotations(&masked, start, end),
            annotations: annotations.iter().filter(|a| start <= a.start && a.start < end).collect(),
        })
        .collect();

    let mut found = vec![];
    if class.kind == Kind::Entity && !class.kotlin {
        constructors(doc, &class, &members, &mut found);
        final_members(&members, &mut found);
    }
    for member in &members {
        let Some((_, _, ty)) = model::declaration(&member.decl, class.kotlin) else {
            continue;
        };
        let kind = scalar(&class, &ty);
        generated_value(doc, &masked, &class, member, kind, &mut found);
        column_arguments(doc, &masked, member, kind, &mut found);
        if class.kind != Kind::Embeddable {
            unidirectional_one_to_many(doc, &masked, member, &mut found);
            many_to_many_advice(doc, &masked, &class, member, &ty, &mut found);
        }
    }
    found
}

fn constructors(doc: &Document, class: &Class, members: &[Member], found: &mut Vec<Finding>) {
    if !setting("checkNoArgConstructor") {
        return;
    }
    let mut any = false;
    for member in members.iter().filter(|m| m.opens_parameters()) {
        let tokens: Vec<_> = member.head().split_whitespace().filter(|t| !matches!(*t, "public" | "protected" | "private")).collect();
        if tokens != [class.name.as_str()] {
            continue;
        }
        any = true;
        let stop = member.head().len();
        let Some(close) = model::closing(member.decl.as_bytes(), stop) else {
            return;
        };
        let accessible = model::has_word(member.head(), "public") || model::has_word(member.head(), "protected");
        if accessible && member.decl[stop + 1..close].trim().is_empty() {
            return;
        }
    }
    let Some(at) = offset(&doc.text, class.pos) else {
        return;
    };
    if any {
        found.push(Finding::new(
            at,
            at + class.name.len(),
            Severity::Error,
            "jpa-no-arg-constructor",
            "An entity class must have a public or protected constructor with no parameters (Jakarta Persistence 2.1). Constructors from Lombok or XML are not resolved.",
        ));
    }
}

fn final_members(members: &[Member], found: &mut Vec<Finding>) {
    if !setting("checkFinalMembers") {
        return;
    }
    for member in members {
        let head = member.head();
        let Some(at) = model::words(head, "final").next() else {
            continue;
        };
        if member.has("Transient") || model::has_word(head, "static") || model::has_word(head, "transient") {
            continue;
        }
        let constructor = member.opens_parameters() && head.split_whitespace().filter(|t| !matches!(*t, "public" | "protected" | "private")).count() == 1;
        if constructor || (!member.opens_parameters() && model::declaration(&member.decl, false).is_none()) {
            continue;
        }
        let from = member.start + at;
        let to = from + "final".len();
        let removed = to + member.decl[at + "final".len()..].bytes().take_while(u8::is_ascii_whitespace).count();
        let what = if member.opens_parameters() { "method" } else { "persistent field" };
        let mut finding = Finding::new(
            from,
            to,
            Severity::Warning,
            "jpa-final-member",
            format!("Every method and persistent instance variable of an entity class must be non-final (Jakarta Persistence 2.1); this {what} is final, which also prevents lazy-loading proxies."),
        );
        finding.fix = Some(("Remove final", vec![model::Edit { start: from, end: removed, text: String::new() }]));
        found.push(finding);
    }
}

fn generation_strategy(class: &Class, text: &str, masked: &str, generated: &Annotation) -> Option<String> {
    let value = model::arg(text, masked, generated, "strategy")?.trim();
    let owned = |full: &str| full == "jakarta.persistence.GenerationType" || full == "javax.persistence.GenerationType";
    let (owner, name) = value.rsplit_once('.')?;
    let owned_owner = if owner.contains('.') {
        owned(owner)
    } else {
        let imports: Vec<_> = class.imports.iter().filter(|i| i.rsplit('.').next() == Some(owner)).collect();
        owner == "GenerationType" && matches!(imports.as_slice(), [import] if owned(import))
    };
    owned_owner.then(|| name.trim().to_owned())
}

fn generated_value(doc: &Document, masked: &str, class: &Class, member: &Member, kind: Option<Scalar>, found: &mut Vec<Finding>) {
    let Some(generated) = member.named("GeneratedValue") else {
        return;
    };
    let has_identifier = member.has("Id") || member.has("EmbeddedId");
    if !has_identifier {
        if setting("checkGeneratedValueId") {
            found.push(Finding::new(
                generated.start,
                generated.end(),
                Severity::Warning,
                "jpa-generated-value-without-id",
                "@GeneratedValue applies to a primary key property or field, but this attribute has no @Id. An identifier declared in XML is not resolved.",
            ));
        }
        return;
    }
    if !setting("checkGeneratedValueType") {
        return;
    }
    let (Some(kind), Some(strategy)) = (kind, generation_strategy(class, &doc.text, masked, generated)) else {
        return;
    };
    let message = match strategy.as_str() {
        "IDENTITY" | "SEQUENCE" | "TABLE" if kind != Scalar::Integral => {
            format!("GenerationType.{strategy} applies to long, int, Long or Integer keys.")
        }
        "UUID" if !matches!(kind, Scalar::Uuid | Scalar::Text) => "GenerationType.UUID applies to UUID or String keys.".to_owned(),
        _ => return,
    };
    found.push(Finding::new(generated.start, generated.end(), Severity::Warning, "jpa-generated-value-type", message));
}

fn column_arguments(doc: &Document, masked: &str, member: &Member, kind: Option<Scalar>, found: &mut Vec<Finding>) {
    let (Some(column), Some(kind)) = (member.named("Column"), kind) else {
        return;
    };
    let present = |key: &str| model::arg(&doc.text, masked, column, key).is_some_and(|v| v.trim() != "0");
    if setting("checkColumnLength") && model::arg(&doc.text, masked, column, "length").is_some() && matches!(kind, Scalar::Integral | Scalar::Numeric | Scalar::Decimal | Scalar::Flag) {
        found.push(Finding::new(
            column.start,
            column.end(),
            Severity::Warning,
            "jpa-column-length-type",
            "@Column(length) applies only to columns whose type is parameterized by length, such as varchar or varbinary; it has no effect on this type.",
        ));
    }
    if setting("checkColumnPrecision") && (present("precision") || present("scale")) && matches!(kind, Scalar::Text | Scalar::Flag | Scalar::Character) {
        found.push(Finding::new(
            column.start,
            column.end(),
            Severity::Warning,
            "jpa-column-precision-type",
            "@Column(precision) and @Column(scale) apply only to columns of exact numeric type; they have no effect on this type.",
        ));
    }
}

fn unidirectional_one_to_many(doc: &Document, masked: &str, member: &Member, found: &mut Vec<Finding>) {
    let Some(association) = member.named("OneToMany") else {
        return;
    };
    if !setting("checkUnidirectionalOneToMany")
        || model::arg(&doc.text, masked, association, "mappedBy").is_some()
        || ["JoinColumn", "JoinColumns", "JoinTable"].iter().any(|n| member.has(n))
    {
        return;
    }
    found.push(Finding::new(
        association.start,
        association.end(),
        Severity::Info,
        "jpa-unidirectional-one-to-many",
        "A unidirectional @OneToMany uses a link table; a bidirectional association owned by the @ManyToOne side is much more efficient (Hibernate User Guide). Add mappedBy or a @JoinColumn if a link table is not intended.",
    ));
}

fn owned_list(class: &Class, ty: &str) -> bool {
    let base = ty.split('<').next().unwrap_or("").trim();
    if base.contains('.') {
        return base == "java.util.List" || (class.kotlin && matches!(base, "kotlin.collections.List" | "kotlin.collections.MutableList"));
    }
    if base != "List" && !(class.kotlin && base == "MutableList") || class.name == base {
        return false;
    }
    let imports: Vec<_> = class.imports.iter().filter(|i| i.rsplit('.').next() == Some(base)).collect();
    match imports.as_slice() {
        [] => class.kotlin,
        [import] => **import == format!("java.util.{base}") || (class.kotlin && **import == format!("kotlin.collections.{base}")),
        _ => false,
    }
}

fn cascades_remove(class: &Class, text: &str) -> bool {
    let mut removes = false;
    for token in text.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.'))).filter(|t| !t.is_empty()) {
        let (owner, value) = token.rsplit_once('.').unwrap_or(("", token));
        if token == "arrayOf" && class.kotlin {
            continue;
        }
        let owned = if owner == "CascadeType" {
            let imports: Vec<_> = class.imports.iter().filter(|i| i.rsplit('.').next() == Some("CascadeType")).collect();
            matches!(imports.as_slice(), [i] if matches!(i.as_str(), "jakarta.persistence.CascadeType" | "javax.persistence.CascadeType"))
        } else {
            matches!(owner, "jakarta.persistence.CascadeType" | "javax.persistence.CascadeType")
        };
        if !owned || !matches!(value, "ALL" | "PERSIST" | "MERGE" | "REMOVE" | "REFRESH" | "DETACH") {
            return false;
        }
        removes |= matches!(value, "ALL" | "REMOVE");
    }
    removes
}

fn many_to_many_advice(doc: &Document, masked: &str, class: &Class, member: &Member, ty: &str, found: &mut Vec<Finding>) {
    let Some(association) = member.named("ManyToMany") else {
        return;
    };
    if setting("checkManyToManyCollectionType") && owned_list(class, ty) {
        found.push(Finding::new(
            association.start,
            association.end(),
            Severity::Info,
            "jpa-many-to-many-list",
            "A @ManyToMany List is a bag: removing one element makes Hibernate delete every link row of the owner and re-insert the rest. Hibernate User Guide 29.4.2: for unidirectional collections, Sets are the best choice. Duplicates are not expected here, so consider a Set.",
        ));
    }
    let cascade = model::arg(&doc.text, masked, association, "cascade");
    if setting("checkManyToManyCascade") && cascade.is_some_and(|c| cascades_remove(class, c)) {
        found.push(Finding::new(
            association.start,
            association.end(),
            Severity::Warning,
            "jpa-many-to-many-cascade-remove",
            "Cascading REMOVE (or ALL) on a @ManyToMany propagates beyond the link table: the other side may still be referenced by other entities, so deleting this entity can fail with a ConstraintViolationException (Hibernate User Guide 3.8.4). Cascade PERSIST and MERGE only.",
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::*;
    use openidea_plugin::{diagnostics::DiagnosticsProvider, mock::MockHost, providers::Document, text::offset};

    fn imports(names: &[&str]) -> String {
        let mut text = "package shop;\n".to_owned();
        for name in names {
            text.push_str(&format!("import {name};\n"));
        }
        text
    }

    fn java(body: &str, extra: &[&str]) -> String {
        let mut names = vec!["jakarta.persistence.Entity", "jakarta.persistence.Id", "jakarta.persistence.Column", "jakarta.persistence.GeneratedValue", "jakarta.persistence.GenerationType", "jakarta.persistence.OneToMany", "jakarta.persistence.JoinColumn"];
        names.extend_from_slice(extra);
        format!("{}@Entity\npublic class Person {{\n{body}\n}}", imports(&names))
    }

    fn kotlin(body: &str, extra: &[&str]) -> String {
        let mut names = vec!["jakarta.persistence.Entity", "jakarta.persistence.Id", "jakarta.persistence.Column", "jakarta.persistence.GeneratedValue", "jakarta.persistence.GenerationType", "jakarta.persistence.OneToMany", "jakarta.persistence.JoinColumn"];
        names.extend_from_slice(extra);
        format!("{}@Entity\nclass Person(\n{body}\n) {{\n}}", imports(&names))
    }

    fn run(path: &str, text: &str, code: &str) -> Vec<String> {
        let _host = MockHost::install();
        let language = if path.ends_with(".kt") { "kotlin" } else { "java" };
        let doc = Document { path: path.into(), language: language.into(), text: text.into(), version: 1 };
        JpaAssist::default()
            .diagnostics(&doc)
            .into_iter()
            .filter(|d| d.code.as_deref() == Some(code))
            .map(|d| text[offset(text, d.range.start).unwrap()..offset(text, d.range.end).unwrap()].to_owned())
            .collect()
    }

    fn j(text: &str, code: &str) -> Vec<String> {
        run("/project/src/main/java/shop/Person.java", text, code)
    }

    fn k(text: &str, code: &str) -> Vec<String> {
        run("/project/src/main/kotlin/shop/Person.kt", text, code)
    }

    fn disabled(text: &str, code: &str, setting: &str) -> bool {
        let host = MockHost::install();
        host.set_setting(&format!("jpaAssist.{setting}"), "false");
        let doc = Document { path: "/project/src/main/java/shop/Person.java".into(), language: "java".into(), text: text.into(), version: 1 };
        JpaAssist::default().diagnostics(&doc).iter().all(|d| d.code.as_deref() != Some(code))
    }

    #[test]
    fn missing_no_arg_constructor() {
        let bad = java("@Id long id;\npublic Person(long id) { this.id = id; }", &[]);
        assert_eq!(j(&bad, "jpa-no-arg-constructor"), ["Person"]);
        let private = java("@Id long id;\nprivate Person() {}\npublic Person(long id) {}", &[]);
        assert_eq!(j(&private, "jpa-no-arg-constructor"), ["Person"]);
        for ok in [
            "@Id long id;",
            "@Id long id;\nprotected Person() {}\npublic Person(long id) {}",
            "@Id long id;\npublic Person() {}",
            "@Id long id;\nPerson create() { return new Person(1); }",
        ] {
            assert!(j(&java(ok, &[]), "jpa-no-arg-constructor").is_empty(), "{ok}");
        }
        let lombok = bad.replace("@Entity", "@lombok.NoArgsConstructor\n@Entity");
        assert!(j(&lombok, "jpa-no-arg-constructor").is_empty());
        let foreign = bad.replace("jakarta.persistence.Entity", "vendor.Entity");
        assert!(j(&foreign, "jpa-no-arg-constructor").is_empty());
        assert!(disabled(&bad, "jpa-no-arg-constructor", "checkNoArgConstructor"));
    }

    #[test]
    fn final_persistent_members() {
        let bad = java("@Id long id;\nprivate final String name = \"x\";\npublic final String label() { return name; }", &[]);
        assert_eq!(j(&bad, "jpa-final-member"), ["final", "final"]);
        for ok in [
            "@Id long id;\nprivate static final String NAME = \"x\";",
            "@Id long id;\nprivate transient final String name = \"x\";",
            "@Id long id;\npublic String label() { return \"x\"; }",
        ] {
            assert!(j(&java(ok, &[]), "jpa-final-member").is_empty(), "{ok}");
        }
        let foreign = bad.replace("jakarta.persistence.Id", "vendor.Id");
        assert!(j(&foreign, "jpa-final-member").is_empty());
        assert!(disabled(&bad, "jpa-final-member", "checkFinalMembers"));
    }

    #[test]
    fn final_member_fix_removes_the_modifier() {
        let _host = MockHost::install();
        let text = java("@Id long id;\nprivate final String name = \"x\";", &[]);
        let doc = Document { path: "/project/src/main/java/shop/Person.java".into(), language: "java".into(), text: text.clone(), version: 1 };
        let at = text.find("final").unwrap();
        let line = openidea_plugin::text::position(&text, at);
        let actions = JpaAssist::default().code_actions(&doc, openidea_plugin::Range::new(line, line));
        let fix = actions.iter().find(|a| a.title == "Remove final").unwrap();
        let edit = &fix.edits[0].1[0];
        assert_eq!(edit.text, "");
    }

    #[test]
    fn generated_value_needs_an_identifier() {
        let bad = java("@GeneratedValue long id;\n@Id long other;", &[]);
        assert_eq!(j(&bad, "jpa-generated-value-without-id"), ["@GeneratedValue"]);
        assert!(j(&java("@Id @GeneratedValue long id;", &[]), "jpa-generated-value-without-id").is_empty());
        assert!(j(&bad.replace("jakarta.persistence.GeneratedValue", "vendor.GeneratedValue"), "jpa-generated-value-without-id").is_empty());
        assert_eq!(k(&kotlin("@GeneratedValue var id: Long? = null", &[]), "jpa-generated-value-without-id"), ["@GeneratedValue"]);
        assert!(k(&kotlin("@Id @GeneratedValue var id: Long? = null", &[]), "jpa-generated-value-without-id").is_empty());
        assert!(disabled(&bad, "jpa-generated-value-without-id", "checkGeneratedValueId"));
    }

    #[test]
    fn generated_value_strategy_must_fit_the_key_type() {
        let identity_string = java("@Id @GeneratedValue(strategy = GenerationType.IDENTITY) String id;", &[]);
        assert_eq!(j(&identity_string, "jpa-generated-value-type").len(), 1);
        let uuid_long = java("@Id @GeneratedValue(strategy = GenerationType.UUID) Long id;", &[]);
        assert_eq!(j(&uuid_long, "jpa-generated-value-type").len(), 1);
        for ok in [
            "@Id @GeneratedValue(strategy = GenerationType.IDENTITY) Long id;",
            "@Id @GeneratedValue(strategy = GenerationType.SEQUENCE) int id;",
            "@Id @GeneratedValue(strategy = GenerationType.UUID) String id;",
            "@Id @GeneratedValue String id;",
            "@Id @GeneratedValue(strategy = GenerationType.IDENTITY) Money id;",
            "@Id @GeneratedValue(strategy = IDENTITY) String id;",
        ] {
            assert!(j(&java(ok, &[]), "jpa-generated-value-type").is_empty(), "{ok}");
        }
        let uuid = java("@Id @GeneratedValue(strategy = GenerationType.UUID) UUID id;", &["java.util.UUID"]);
        assert!(j(&uuid, "jpa-generated-value-type").is_empty());
        let shadowed = java("@Id @GeneratedValue(strategy = GenerationType.IDENTITY) Long id;", &["app.Long"]);
        assert!(j(&shadowed, "jpa-generated-value-type").is_empty());
        let foreign_enum = identity_string.replace("jakarta.persistence.GenerationType", "app.GenerationType");
        assert!(j(&foreign_enum, "jpa-generated-value-type").is_empty());
        let kt = kotlin("@Id @GeneratedValue(strategy = GenerationType.IDENTITY) var id: String", &[]);
        assert_eq!(k(&kt, "jpa-generated-value-type").len(), 1);
        assert!(k(&kotlin("@Id @GeneratedValue(strategy = GenerationType.IDENTITY) var id: Long? = null", &[]), "jpa-generated-value-type").is_empty());
        assert!(disabled(&identity_string, "jpa-generated-value-type", "checkGeneratedValueType"));
    }

    #[test]
    fn column_length_on_a_type_without_length() {
        let bad = java("@Id long id;\n@Column(length = 20) int quantity;", &[]);
        assert_eq!(j(&bad, "jpa-column-length-type").len(), 1);
        for ok in ["@Id long id;\n@Column(length = 20) String name;", "@Id long id;\n@Column(nullable = false) int quantity;", "@Id long id;\n@Column(length = 20) Money m;"] {
            assert!(j(&java(ok, &[]), "jpa-column-length-type").is_empty(), "{ok}");
        }
        assert_eq!(k(&kotlin("@Id var id: Long = 0,\n@Column(length = 20) var quantity: Int = 0", &[]), "jpa-column-length-type").len(), 1);
        assert!(k(&kotlin("@Id var id: Long = 0,\n@Column(length = 20) var name: String = \"\"", &[]), "jpa-column-length-type").is_empty());
        assert!(j(&bad.replace("jakarta.persistence.Column", "app.Column"), "jpa-column-length-type").is_empty());
        assert!(disabled(&bad, "jpa-column-length-type", "checkColumnLength"));
    }

    #[test]
    fn column_precision_on_a_non_numeric_type() {
        let bad = java("@Id long id;\n@Column(precision = 10, scale = 2) String name;", &[]);
        assert_eq!(j(&bad, "jpa-column-precision-type").len(), 1);
        for ok in [
            "@Id long id;\n@Column(precision = 10, scale = 2) BigDecimal total;",
            "@Id long id;\n@Column(precision = 0) String name;",
            "@Id long id;\n@Column(length = 5) String name;",
        ] {
            assert!(j(&java(ok, &["java.math.BigDecimal"]), "jpa-column-precision-type").is_empty(), "{ok}");
        }
        assert_eq!(k(&kotlin("@Id var id: Long = 0,\n@Column(scale = 2) var flag: Boolean = false", &[]), "jpa-column-precision-type").len(), 1);
        assert!(disabled(&bad, "jpa-column-precision-type", "checkColumnPrecision"));
    }

    #[test]
    fn unidirectional_one_to_many() {
        let bad = java("@Id long id;\n@OneToMany List<Phone> phones;", &[]);
        assert_eq!(j(&bad, "jpa-unidirectional-one-to-many"), ["@OneToMany"]);
        for ok in [
            "@Id long id;\n@OneToMany(mappedBy = \"owner\") List<Phone> phones;",
            "@Id long id;\n@OneToMany @JoinColumn(name = \"owner_id\") List<Phone> phones;",
        ] {
            assert!(j(&java(ok, &[]), "jpa-unidirectional-one-to-many").is_empty(), "{ok}");
        }
        assert!(j(&bad.replace("jakarta.persistence.OneToMany", "app.OneToMany"), "jpa-unidirectional-one-to-many").is_empty());
        let body = |args: &str| kotlin("@Id var id: Long = 0", &[]).replace(") {\n}", &format!(") {{\n@OneToMany{args} var phones: List<Phone> = listOf()\n}}"));
        assert_eq!(k(&body(""), "jpa-unidirectional-one-to-many").len(), 1);
        assert!(k(&body("(mappedBy = \"owner\")"), "jpa-unidirectional-one-to-many").is_empty());
        assert!(disabled(&bad, "jpa-unidirectional-one-to-many", "checkUnidirectionalOneToMany"));
    }
    fn kbody(member: &str, extra: &[&str]) -> String {
        kotlin("@Id var id: Long = 0", extra).replace(") {\n}", &format!(") {{\n{member}\n}}"))
    }

    const MM: &str = "jakarta.persistence.ManyToMany";
    const CT: &str = "jakarta.persistence.CascadeType";

    fn jl(body: &str, extra: &[&str]) -> String {
        let mut names = vec![MM, "java.util.List"];
        names.extend_from_slice(extra);
        java(body, &names)
    }

    #[test]
    fn many_to_many_list_suggests_a_set() {
        let bad = jl("@Id long id;\n@ManyToMany List<Tag> tags;", &[]);
        assert_eq!(j(&bad, "jpa-many-to-many-list"), ["@ManyToMany"]);
        let set = java("@Id long id;\n@ManyToMany Set<Tag> tags;", &[MM, "java.util.Set"]);
        assert!(j(&set, "jpa-many-to-many-list").is_empty());
        let one_to_many = java("@Id long id;\n@OneToMany(mappedBy = \"o\") List<Tag> tags;", &["java.util.List"]);
        assert!(j(&one_to_many, "jpa-many-to-many-list").is_empty());
        assert!(j(&java("@Id long id;\n@ManyToMany List<Tag> tags;", &[MM]), "jpa-many-to-many-list").is_empty());
        assert!(j(&bad.replace("java.util.List", "app.List"), "jpa-many-to-many-list").is_empty());
        assert!(j(&bad.replace("jakarta.persistence.ManyToMany", "app.ManyToMany"), "jpa-many-to-many-list").is_empty());
        assert_eq!(k(&kbody("@ManyToMany var tags: List<Tag> = listOf()", &[MM]), "jpa-many-to-many-list").len(), 1);
        assert_eq!(k(&kbody("@ManyToMany var tags: MutableList<Tag> = mutableListOf()", &[MM]), "jpa-many-to-many-list").len(), 1);
        assert!(k(&kbody("@ManyToMany var tags: Set<Tag> = setOf()", &[MM]), "jpa-many-to-many-list").is_empty());
        assert!(disabled(&bad, "jpa-many-to-many-list", "checkManyToManyCollectionType"));
    }

    #[test]
    fn many_to_many_cascade_remove_is_flagged() {
        for bad in ["cascade = CascadeType.ALL", "cascade = {CascadeType.PERSIST, CascadeType.REMOVE}", "cascade = jakarta.persistence.CascadeType.REMOVE"] {
            let text = java(&format!("@Id long id;\n@ManyToMany({bad}) Set<Tag> tags;"), &[MM, CT]);
            assert_eq!(j(&text, "jpa-many-to-many-cascade-remove").len(), 1, "{bad}");
        }
        for ok in ["cascade = {CascadeType.PERSIST, CascadeType.MERGE}", "mappedBy = \"tags\"", "cascade = CascadeType.DETACH"] {
            let text = java(&format!("@Id long id;\n@ManyToMany({ok}) Set<Tag> tags;"), &[MM, CT]);
            assert!(j(&text, "jpa-many-to-many-cascade-remove").is_empty(), "{ok}");
        }
        let text = java("@Id long id;\n@ManyToMany(cascade = CascadeType.ALL) Set<Tag> tags;", &[MM, CT]);
        assert!(j(&text.replace("jakarta.persistence.CascadeType", "app.CascadeType"), "jpa-many-to-many-cascade-remove").is_empty());
        assert!(j(&text.replace("CascadeType.ALL", "Policies.ALL"), "jpa-many-to-many-cascade-remove").is_empty());
        let orphan = java("@Id long id;\n@OneToMany(mappedBy = \"o\", orphanRemoval = true) Set<Tag> tags;", &[]);
        assert!(j(&orphan, "jpa-many-to-many-cascade-remove").is_empty() && j(&orphan, "jpa-eager-fetch").is_empty());
        let kt = kbody("@ManyToMany(cascade = [CascadeType.ALL]) var tags: Set<Tag> = setOf()", &[MM, CT]);
        assert_eq!(k(&kt, "jpa-many-to-many-cascade-remove").len(), 1);
        assert!(k(&kt.replace("CascadeType.ALL", "CascadeType.MERGE"), "jpa-many-to-many-cascade-remove").is_empty());
        assert!(disabled(&text, "jpa-many-to-many-cascade-remove", "checkManyToManyCascade"));
    }

    #[test]
    fn fetch_advice_has_a_toggle_and_cites_the_guide() {
        let eager = java("@Id long id;\n@OneToMany(mappedBy = \"o\", fetch = FetchType.EAGER) java.util.Set<Tag> tags;\n@ManyToOne Owner owner;", &["jakarta.persistence.FetchType", "jakarta.persistence.ManyToOne"]);
        assert_eq!(j(&eager, "jpa-eager-fetch").len(), 1);
        assert_eq!(j(&eager, "jpa-eager-to-one").len(), 1);
        assert!(disabled(&eager, "jpa-eager-fetch", "checkAssociationFetch"));
        assert!(disabled(&eager, "jpa-eager-to-one", "checkAssociationFetch"));
    }

    fn diagnostics_with(text: &str, rows: &str) -> Vec<openidea_plugin::Diagnostic> {
        let host = MockHost::install();
        if !rows.is_empty() {
            host.set_setting("jpaAssist.inspectionSeverities", rows);
        }
        let doc = Document { path: "/project/src/main/java/shop/Person.java".into(), language: "java".into(), text: text.into(), version: 1 };
        JpaAssist::default().diagnostics(&doc)
    }

    fn level(text: &str, rows: &str, code: &str) -> Option<openidea_plugin::Severity> {
        diagnostics_with(text, rows).iter().find(|d| d.code.as_deref() == Some(code)).map(|d| d.severity)
    }

    #[test]
    fn configured_severities_remap_disable_and_ignore_junk() {
        use openidea_plugin::Severity::*;
        let text = java("@Id long id;\n@Column(length = 20) int quantity;\n@ManyToMany List<Tag> tags;", &[MM, "java.util.List"]);
        let code = "jpa-column-length-type";
        assert_eq!(level(&text, "", code), Some(Warning));
        assert_eq!(level(&text, "[]", code), Some(Warning));
        assert_eq!(level(&text, r#"[{"code":"jpa-column-length-type","severity":"error"}]"#, code), Some(Error));
        assert_eq!(level(&text, r#"[{"code":"jpa-column-length-type","severity":"hint"}]"#, code), Some(Hint));
        assert_eq!(level(&text, r#"[{"code":"jpa-column-length-type","severity":"off"}]"#, code), None);
        assert_eq!(level(&text, r#"[{"code":"jpa-many-to-many-list","severity":"off"}]"#, code), Some(Warning));
        assert_eq!(level(&text, r#"[{"code":"jpa-many-to-many-list","severity":"warning"}]"#, "jpa-many-to-many-list"), Some(Warning));
        for junk in [
            "not json",
            r#"{"jpa-column-length-type":"off"}"#,
            r#"[{"code":"jpa-made-up","severity":"off"},{"code":"jpa-column-length-type","severity":"loud"},{"code":7},"x",null]"#,
        ] {
            assert_eq!(level(&text, junk, code), Some(Warning), "{junk}");
        }
        let both = r#"[{"code":"jpa-column-length-type","severity":"error"}]"#;
        let host = MockHost::install();
        host.set_setting("jpaAssist.inspectionSeverities", both);
        host.set_setting("jpaAssist.checkColumnLength", "false");
        let doc = Document { path: "/project/src/main/java/shop/Person.java".into(), language: "java".into(), text: text.clone(), version: 1 };
        assert!(JpaAssist::default().diagnostics(&doc).iter().all(|d| d.code.as_deref() != Some(code)));
    }

    #[test]
    fn quick_fixes_attach_to_remapped_findings() {
        let text = java("@Id long id;\nprivate final String name = \"x\";", &[]);
        let rows = r#"[{"code":"jpa-final-member","severity":"hint"}]"#;
        let host = MockHost::install();
        host.set_setting("jpaAssist.inspectionSeverities", rows);
        let found = JpaAssist::default().diagnostics(&Document { path: "/project/src/main/java/shop/Person.java".into(), language: "java".into(), text: text.clone(), version: 1 });
        let remapped = found.iter().find(|d| d.code.as_deref() == Some("jpa-final-member")).unwrap();
        assert_eq!(remapped.severity, openidea_plugin::Severity::Hint);
        let doc = Document { path: "/project/src/main/java/shop/Person.java".into(), language: "java".into(), text: text.clone(), version: 1 };
        let line = openidea_plugin::text::position(&text, text.find("final").unwrap());
        let actions = JpaAssist::default().code_actions(&doc, openidea_plugin::Range::new(line, line));
        assert!(actions.iter().any(|a| a.title == "Remove final"));
    }

    #[test]
    fn schema_enum_matches_the_known_code_registry() {
        let schema: serde_json::Value = serde_json::from_str(include_str!("../settings.schema.json")).unwrap();
        let listed: Vec<&str> = schema["properties"]["jpaAssist.inspectionSeverities"]["items"]["properties"]["code"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(listed, crate::live::KNOWN_CODES);
    }

    #[test]
    fn every_inspection_is_enabled_and_rules_the_original_forces_to_error_keep_their_severity() {
        let schema: serde_json::Value = serde_json::from_str(include_str!("../settings.schema.json")).unwrap();
        let toggles: Vec<_> = schema["properties"].as_object().unwrap().iter().filter(|(name, _)| name.starts_with("jpaAssist.check")).collect();
        assert!(toggles.len() > 20);
        for (name, definition) in toggles {
            assert_eq!(definition["default"], serde_json::Value::Bool(true), "{name}");
        }
        assert_eq!(schema["properties"]["jpaAssist.inspectionSeverities"]["default"], serde_json::json!([]));
        let _host = MockHost::install();
        let header = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.Version;\n";
        for (body, code, expected) in [
            ("@Entity class Person { @Id long id; @Version long revision; @Version long other; }", "jpa-duplicate-version", openidea_plugin::Severity::Error),
            ("@Entity class Person { String name; }", "jpa-source-identifier-absent", openidea_plugin::Severity::Info),
        ] {
            let document = Document { path: "/project/src/main/java/shop/Person.java".into(), language: "java".into(), text: format!("{header}{body}"), version: 1 };
            let found = JpaAssist::default().diagnostics(&document);
            let reported: Vec<_> = found.iter().filter(|d| d.code.as_deref() == Some(code)).collect();
            assert!(!reported.is_empty(), "{code}: {found:?}");
            assert!(reported.iter().all(|d| d.severity == expected), "{code}: {reported:?}");
        }
    }
}
