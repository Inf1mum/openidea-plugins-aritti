//! Reviewed single-document `equals`/`hashCode` and `toString` generation for a plain entity.
//!
//! Sources: Hibernate ORM 6.4 user guide, section "Implementing equals() and hashCode()" (an identifier
//! based equality needs a constant `hashCode` and must compare the identifier only for non-transient
//! entities; natural-id/business-key equality is preferred when one exists; the guide's own examples use
//! `instanceof`). `org.hibernate.Hibernate.getClass(T)` is a hibernate-core 6.6.49 API (checked with javap),
//! used only when hibernate-core 6 is provable from the build files.
//!
//! Conservative scope: one source-owned entity without superclass, no Lombok equality annotations, no
//! existing `equals`/`hashCode`/`toString`, field access, scalar attributes only (never associations).
use super::*;
use crate::model::{Attribute, Edit, import_edit, without_annotations};

pub(super) const TITLE_EQUALS: &str = "Generate proxy-safe equals and hashCode with preview…";
pub(super) const TITLE_TO_STRING: &str = "Generate lazy-safe toString with preview…";
const MAX_FIELDS: usize = 16;
const PRIMITIVES: [&str; 8] = ["byte", "short", "int", "long", "float", "double", "boolean", "char"];
const MAPPING_ANNOTATIONS: [&str; 9] = ["Id", "Column", "Basic", "ManyToOne", "OneToMany", "OneToOne", "ManyToMany", "Embedded", "Version"];
const EQUALS_METHODS: [&str; 2] = ["equals", "hashCode"];
const EQUALS_LOMBOK: [&str; 3] = ["Data", "EqualsAndHashCode", "Value"];
const TO_STRING_LOMBOK: [&str; 3] = ["Data", "ToString", "Value"];

struct Field {
    a: Attribute,
    span: (usize, usize),
    getter: Option<String>,
    lazy_or_version: (bool, bool),
}

struct Source {
    class: Class,
    masked: String,
    end: usize,
    fields: Vec<Field>,
}

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn declares(masked: &str, name: &str) -> bool {
    words(masked, name).any(|at| masked[at + name.len()..].trim_start().starts_with('(') && !masked[..at].trim_end().ends_with('.'))
}

fn getter(masked: &str, members: &[(usize, usize)], a: &Attribute) -> Option<String> {
    let cap = capitalize(&a.name);
    let ty = compact(&a.ty);
    let mut names = vec![format!("get{cap}")];
    if a.ty == "boolean" || a.ty == "Boolean" {
        names.push(format!("is{cap}"));
    }
    names.into_iter().find(|n| {
        members.iter().any(|&(s, e)| {
            let shape = compact(&without_annotations(masked, s, e));
            shape == format!("public{ty}{n}(){{return{};}}", a.name) || shape == format!("public{ty}{n}(){{returnthis.{};}}", a.name)
        })
    })
}

fn source(doc: &Document, declared: &[&str], lombok: &[&str]) -> Result<Source, String> {
    if doc.text.len() > MAX_TEXT {
        return Err("The source exceeds the supported bound.".into());
    }
    let kotlin = doc.language == "kotlin";
    let masked = mask(&doc.text);
    let forbidden: &[&str] = if kotlin { &["record", "enum", "interface", "object"] } else { &["record", "enum", "interface"] };
    if forbidden.iter().any(|w| has_word(&masked, w)) || words(&masked, "data").any(|at| masked[at + 4..].trim_start().starts_with("class")) {
        return Err("Records, enums, interfaces, objects and Kotlin data classes are not supported.".into());
    }
    if parse_class(&doc.text, &doc.path).is_some_and(|class| class.superclass.is_some()) {
        return Err("This entity has a superclass, so inherited equality is unknown; nothing was generated.".into());
    }
    let found = annotations(&masked);
    if let Some(a) = found.iter().find(|a| lombok.contains(&a.name)) {
        return Err(format!("@{} is present and already generates this method; nothing was generated.", a.name));
    }
    if found.iter().any(|a| matches!(a.name, "Access" | "EmbeddedId" | "IdClass") || masked[a.start + 1..a.name_end].starts_with("get:")) {
        return Err("@Access, composite identifiers and property-site annotations are not supported.".into());
    }
    if let Some(name) = declared.iter().find(|name| declares(&masked, name)) {
        return Err(format!("{name} is already declared; replacing it is not offered."));
    }
    let class = dto_action::source(doc).filter(|class| class.kind == Kind::Entity).ok_or("Select a plain, source-owned entity class.")?;
    let at = offset(&doc.text, class.pos).ok_or("Stale source.")?;
    let open = at + masked[at..].find('{').ok_or("Stale source.")?;
    let rest = masked[at..open].trim().strip_prefix(class.name.as_str()).ok_or("Stale source.")?.trim();
    let constructor = kotlin && rest.starts_with('(') && crate::model::closing(rest.as_bytes(), 0) == Some(rest.len() - 1);
    if !rest.is_empty() && !constructor {
        return Err("Generic or extended classes are not supported.".into());
    }
    let end = class.body_end.ok_or("Stale source.")?;
    let members = crate::model::members(&masked, open + 1, end, kotlin, false);
    if !kotlin
        && members.iter().any(|&(s, e)| {
            !masked[..e].ends_with(';') && annotations(&masked[s..e]).iter().any(|a| MAPPING_ANNOTATIONS.contains(&a.name))
        })
    {
        return Err("Mapping annotations on methods suggest property access; only field access is supported.".into());
    }
    let mut names = HashSet::new();
    if class.attributes.iter().any(|a| !names.insert(a.name.as_str())) {
        return Err("Attribute names are ambiguous.".into());
    }
    let mut fields = Vec::new();
    for a in &class.attributes {
        let Some(off) = offset(&doc.text, a.pos) else { continue };
        let Some(&span) = members.iter().find(|&&(s, e)| s <= off && off < e) else { continue };
        let slice = &masked[span.0..span.1];
        let lazy_or_version = (has_word(slice, "LAZY"), annotations(slice).iter().any(|x| matches!(x.name, "Version" | "Transient")));
        let getter = if kotlin { None } else { getter(&masked, &members, a) };
        fields.push(Field { a: a.clone(), span, getter, lazy_or_version });
    }
    Ok(Source { class, masked, end, fields })
}

fn scalar(src: &Source, f: &Field) -> bool {
    let a = &f.a;
    a.relation == Relation::None
        && identifier(&a.name)
        && super::dto_action::supported(&src.class, &a.ty)
        && !a.ty.contains("[]")
        && a.ty != "ByteArray"
        && !a.lob
        && !f.lazy_or_version.0
}

/// Fields a business key (`key`) or `toString` may use; each needs a trivial getter in Java so proxies work.
fn offered(src: &Source, key: bool) -> Vec<&Field> {
    src.fields
        .iter()
        .filter(|f| scalar(src, f))
        .filter(|f| !key || (!f.a.id && f.a.generated.is_none() && !f.lazy_or_version.1 && (src.class.kotlin || f.getter.is_some())))
        .take(MAX_FIELDS)
        .collect()
}

fn nullable(src: &Source, f: &Field) -> bool {
    let clean = without_annotations(&src.masked, f.span.0, f.span.1);
    let Some(at) = words(&clean, &f.a.name).next() else { return false };
    let Some(after) = clean[at + f.a.name.len()..].trim_start().strip_prefix(':') else { return false };
    let ty = after.split(['=', '\n', ',', ')']).next().unwrap_or("");
    ty.trim().ends_with('?')
}

fn id_field(src: &Source) -> Result<&Field, String> {
    let ids: Vec<_> = src.fields.iter().filter(|f| f.a.id).collect();
    let [f] = ids.as_slice() else {
        return Err("Exactly one simple @Id attribute is required for identifier-based equality.".into());
    };
    if f.a.relation != Relation::None || !super::dto_action::supported(&src.class, &f.a.ty) || f.a.ty.contains("[]") {
        return Err("The identifier is not a recognised scalar.".into());
    }
    if !src.class.kotlin && PRIMITIVES.contains(&f.a.ty.as_str()) {
        return Err("A primitive identifier cannot express the transient state; use a wrapper type.".into());
    }
    if src.class.kotlin && !nullable(src, f) {
        return Err("A non-null Kotlin identifier cannot express the transient state; declare it nullable.".into());
    }
    if !src.class.kotlin && f.getter.is_none() {
        return Err(format!("A trivial public getter for `{}` is required: a proxy's fields are never loaded.", f.a.name));
    }
    Ok(f)
}

pub(super) fn eligible_equals(doc: &Document) -> bool {
    source(doc, &EQUALS_METHODS, &EQUALS_LOMBOK).is_ok_and(|src| id_field(&src).is_ok() || !offered(&src, true).is_empty())
}

pub(super) fn eligible_to_string(doc: &Document) -> bool {
    source(doc, &["toString"], &TO_STRING_LOMBOK).is_ok_and(|src| !offered(&src, false).is_empty())
}

fn read(f: &Field, kotlin: bool) -> String {
    if kotlin { f.a.name.clone() } else { format!("{}()", f.getter.clone().unwrap_or_default()) }
}

fn equals_text(src: &Source, basis: &[&Field], identifier: bool, hibernate: bool) -> String {
    let name = &src.class.name;
    let kotlin = src.class.kotlin;
    let key = |f: &&Field| read(f, kotlin);
    let (comparison, hash) = if identifier {
        let id = key(&basis[0]);
        let hash = match (hibernate, kotlin) {
            (true, _) => "Hibernate.getClass(this).hashCode()".to_owned(),
            (false, true) => format!("{name}::class.hashCode()"),
            (false, false) => format!("{name}.class.hashCode()"),
        };
        let comparison = if kotlin {
            format!("{id} != null && {id} == other.{id}")
        } else {
            format!("{id} != null && {id}.equals(other.{id})")
        };
        (comparison, hash)
    } else if kotlin {
        let parts: Vec<_> = basis.iter().map(|f| format!("{0} == other.{0}", key(f))).collect();
        let hash = format!("Objects.hash({})", basis.iter().map(key).collect::<Vec<_>>().join(", "));
        (parts.join("\n            && "), hash)
    } else {
        let parts: Vec<_> = basis.iter().map(|f| format!("Objects.equals({0}, other.{0})", key(f))).collect();
        let hash = format!("Objects.hash({})", basis.iter().map(key).collect::<Vec<_>>().join(", "));
        (parts.join("\n                && "), hash)
    };
    if kotlin {
        let guard = if hibernate {
            format!("if (other == null || Hibernate.getClass(this) != Hibernate.getClass(other)) return false\n        other as {name}")
        } else {
            format!("if (other !is {name}) return false")
        };
        return format!(
            "\n    override fun equals(other: Any?): Boolean {{\n        if (this === other) return true\n        {guard}\n        return {comparison}\n    }}\n\n    override fun hashCode(): Int = {hash}\n"
        );
    }
    let guard = if hibernate {
        "if (o == null || Hibernate.getClass(this) != Hibernate.getClass(o)) {".to_owned()
    } else {
        format!("if (!(o instanceof {name})) {{")
    };
    format!(
        "\n    @Override\n    public boolean equals(Object o) {{\n        if (this == o) {{\n            return true;\n        }}\n        {guard}\n            return false;\n        }}\n        {name} other = ({name}) o;\n        return {comparison};\n    }}\n\n    @Override\n    public int hashCode() {{\n        return {hash};\n    }}\n"
    )
}

fn to_string_text(src: &Source, picked: &[&Field]) -> String {
    let name = &src.class.name;
    if src.class.kotlin {
        let parts: Vec<_> = picked.iter().map(|f| format!("{0}=${{{0}}}", f.a.name)).collect();
        return format!("\n    override fun toString(): String = \"{name}({})\"\n", parts.join(", "));
    }
    let parts: Vec<_> = picked
        .iter()
        .enumerate()
        .map(|(i, f)| format!("+ \"{}{}=\" + {}", if i == 0 { "" } else { ", " }, f.a.name, f.a.name))
        .collect();
    format!(
        "\n    @Override\n    public String toString() {{\n        return \"{name}{{\"\n                {}\n                + '}}';\n    }}\n",
        parts.join("\n                ")
    )
}

fn edits(doc: &Document, src: &Source, method: String, imports: &[&str]) -> Result<Vec<TextEdit>, Error> {
    for full in imports {
        let simple = full.rsplit('.').next().unwrap_or(full);
        if src.class.name == simple || src.class.imports.iter().any(|i| i.rsplit('.').next() == Some(simple) && i != full) {
            return Err(format!("The name {simple} is already used in this file; edit manually.").into());
        }
    }
    let crlf = doc.text.contains("\r\n");
    let mut all = vec![Edit::insert(src.end, &method)];
    all.extend(import_edit(&doc.text, imports, src.class.kotlin));
    Ok(all
        .into_iter()
        .map(|mut edit| {
            if crlf {
                edit.text = edit.text.replace("\r\n", "\n").replace('\n', "\r\n");
            }
            edit.to_text_edit(&doc.text)
        })
        .collect())
}

/// Edits plus the preview notes. `chosen` names business-key fields and is ignored for identifier equality.
pub(super) fn render_equals(doc: &Document, identifier_based: bool, chosen: &[String], hibernate: bool) -> Result<(Vec<TextEdit>, String), Error> {
    let src = source(doc, &EQUALS_METHODS, &EQUALS_LOMBOK).map_err(Error::from)?;
    let basis: Vec<&Field> = if identifier_based {
        vec![id_field(&src).map_err(Error::from)?]
    } else {
        let allowed = offered(&src, true);
        let mut seen = HashSet::new();
        let mut picked = Vec::new();
        for name in chosen {
            let field = allowed.iter().find(|f| f.a.name == *name).filter(|_| seen.insert(name.as_str()));
            picked.push(*field.ok_or("Choose offered business-key fields exactly once (identifiers, generated values, associations and lazy attributes are not offered).")?);
        }
        if picked.is_empty() {
            return Err("Choose at least one business-key field.".into());
        }
        picked
    };
    let mut imports = Vec::new();
    if hibernate {
        imports.push("org.hibernate.Hibernate");
    }
    if !identifier_based {
        imports.push("java.util.Objects");
    }
    let method = equals_text(&src, &basis, identifier_based, hibernate);
    let mut notes = String::new();
    if identifier_based {
        notes.push_str("Identifier equality: equals compares the identifier only when it is non-null and hashCode is constant per class, so the hash does not change when the id is assigned on persist (Hibernate user guide, \"Implementing equals() and hashCode()\"). ");
    } else {
        notes.push_str("Business-key equality: the picked fields must be unique per row and should not change after the entity is added to a Set; the Hibernate user guide prefers a natural id/business key when one exists. ");
    }
    if hibernate {
        notes.push_str("Hibernate.getClass unwraps proxies (hibernate-core 6 is declared in a build file). ");
    } else {
        notes.push_str("hibernate-core 6 is not provable from the build files, so Hibernate.getClass is not used: the instanceof form accepts proxy subclasses of this class but is not symmetric if you subclass the entity yourself. ");
    }
    notes.push_str("Other instances are read through accessors because a proxy's fields are never loaded.");
    Ok((edits(doc, &src, method, &imports)?, notes))
}

pub(super) fn render_to_string(doc: &Document, chosen: &[String]) -> Result<Vec<TextEdit>, Error> {
    let src = source(doc, &["toString"], &TO_STRING_LOMBOK).map_err(Error::from)?;
    let allowed = offered(&src, false);
    let mut seen = HashSet::new();
    let mut picked = Vec::new();
    for name in chosen {
        let field = allowed.iter().find(|f| f.a.name == *name).filter(|_| seen.insert(name.as_str()));
        picked.push(*field.ok_or("Choose offered scalar attributes exactly once (associations, collections, arrays and lazy attributes are never offered).")?);
    }
    if picked.is_empty() {
        return Err("Choose at least one attribute.".into());
    }
    let method = to_string_text(&src, &picked);
    edits(doc, &src, method, &[])
}

fn items(fields: &[&Field]) -> Vec<PickItem> {
    fields.iter().map(|f| PickItem::new(&f.a.name, &f.a.name).description(&f.a.ty)).collect()
}

pub(super) fn resolve_equals(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let src = source(doc, &EQUALS_METHODS, &EQUALS_LOMBOK).map_err(Error::from)?;
    let mut options = Vec::new();
    if id_field(&src).is_ok() {
        options.push(PickItem::new("identifier", "Identifier-based (non-null id, constant hashCode)"));
    }
    let fields = offered(&src, true);
    if !fields.is_empty() {
        options.push(PickItem::new("business-key", "Business key (pick scalar fields)"));
    }
    if options.is_empty() {
        return Err("Neither an identifier nor a business-key field qualifies for equality.".into());
    }
    let basis = host::pick("Equality basis", &options)?.ok_or(Error::Cancelled)?;
    let identifier_based = match basis.as_str() {
        "identifier" => true,
        "business-key" => false,
        _ => return Err(Error::Stale),
    };
    let chosen = if identifier_based {
        Vec::new()
    } else {
        host::pick_many("Business key fields (identifiers, generated values and associations are not offered)", &items(&fields))?
            .ok_or(Error::Cancelled)?
    };
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let build = super::mapper_action::build_text(project, &pending.root, directory)?;
    let hibernate = super::hibernate_action::hibernate6(&build).is_ok();
    let (edits, notes) = render_equals(doc, identifier_based, &chosen, hibernate)?;
    let after = checked_after(doc, &edits)?;
    if !ui_kit::prompts::confirm_exact(
        "Generate equals and hashCode?",
        &format!("Target: {}\nSource version: {}\n\n{after}\n{notes}\nCurrent source only; other files and subclasses are not checked. One undoable host edit.", doc.path, doc.version),
    )? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    Ok(FileAction { title: "Generate equals and hashCode".into(), kind: Some("refactor.rewrite".into()), edits: vec![(doc.path.clone(), edits)], ..FileAction::default() })
}

pub(super) fn resolve_to_string(pending: &Pending, doc: &Document) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let src = source(doc, &["toString"], &TO_STRING_LOMBOK).map_err(Error::from)?;
    let fields = offered(&src, false);
    if fields.is_empty() {
        return Err("No scalar attribute can be shown.".into());
    }
    let chosen = host::pick_many("Attributes to show (associations, collections, arrays and lazy attributes are never offered)", &items(&fields))?
        .ok_or(Error::Cancelled)?;
    let edits = render_to_string(doc, &chosen)?;
    let after = checked_after(doc, &edits)?;
    if !ui_kit::prompts::confirm_exact(
        "Generate toString?",
        &format!("Target: {}\nSource version: {}\n\n{after}\nOnly the attributes you picked are read, so no lazy association can be initialised. Current source only. One undoable host edit.", doc.path, doc.version),
    )? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    Ok(FileAction { title: "Generate toString".into(), kind: Some("refactor.rewrite".into()), edits: vec![(doc.path.clone(), edits)], ..FileAction::default() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const JAVA: &str = "package shop;\n\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.GeneratedValue;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.ManyToOne;\n\n@Entity\npublic class Invoice {\n    @Id\n    @GeneratedValue\n    private Long id;\n    private String number;\n    private String title;\n    private boolean paid;\n    @ManyToOne\n    private Customer customer;\n\n    public Long getId() {\n        return id;\n    }\n\n    public String getNumber() {\n        return this.number;\n    }\n\n    public boolean isPaid() {\n        return paid;\n    }\n}\n";
    const KOTLIN: &str = "package shop\n\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\nimport jakarta.persistence.ManyToOne\n\n@Entity\nopen class Invoice {\n    @Id\n    var id: Long? = null\n    var number: String? = null\n    var count: Int = 0\n    @ManyToOne\n    var customer: Customer? = null\n}\n";
    const GRADLE: &str = "dependencies {\n    implementation 'org.hibernate.orm:hibernate-core:6.5.3.Final'\n}\n";

    fn doc(text: &str, language: &str) -> Document {
        let ext = if language == "java" { "java" } else { "kt" };
        Document { path: format!("/project/src/main/{language}/shop/Invoice.{ext}"), language: language.into(), text: text.into(), version: 1 }
    }
    fn strings(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| (*s).to_owned()).collect()
    }
    fn equals(text: &str, language: &str, identifier: bool, names: &[&str], hibernate: bool) -> Result<String, Error> {
        let _host = MockHost::install();
        let d = doc(text, language);
        let (edits, _) = render_equals(&d, identifier, &strings(names), hibernate)?;
        patched(text, &edits)
    }
    fn to_string(text: &str, language: &str, names: &[&str]) -> Result<String, Error> {
        let _host = MockHost::install();
        patched(text, &render_to_string(&doc(text, language), &strings(names))?)
    }
    fn refused(result: Result<String, Error>, needle: &str) {
        match result {
            Err(Error::Failed(message)) => assert!(message.contains(needle), "{needle}: {message}"),
            other => panic!("expected refusal containing {needle:?}, got {other:?}"),
        }
    }

    #[test]
    fn java_identifier_with_hibernate_unwraps_proxies() {
        let after = equals(JAVA, "java", true, &[], true).unwrap();
        let expected = "    @Override\n    public boolean equals(Object o) {\n        if (this == o) {\n            return true;\n        }\n        if (o == null || Hibernate.getClass(this) != Hibernate.getClass(o)) {\n            return false;\n        }\n        Invoice other = (Invoice) o;\n        return getId() != null && getId().equals(other.getId());\n    }\n\n    @Override\n    public int hashCode() {\n        return Hibernate.getClass(this).hashCode();\n    }\n}\n";
        assert!(after.ends_with(&format!("    }}\n\n{expected}")), "{after}");
        assert!(after.contains("import jakarta.persistence.ManyToOne;\nimport org.hibernate.Hibernate;\n\n@Entity"), "{after}");
        assert!(!after.contains("java.util.Objects"));
    }

    #[test]
    fn java_identifier_fallback_uses_instanceof_and_constant_hash() {
        let after = equals(JAVA, "java", true, &[], false).unwrap();
        assert!(after.contains("        if (!(o instanceof Invoice)) {\n            return false;\n        }\n        Invoice other = (Invoice) o;"), "{after}");
        assert!(after.contains("return Invoice.class.hashCode();") && !after.contains("Hibernate"), "{after}");
    }

    #[test]
    fn java_business_key_uses_objects_and_getters() {
        let after = equals(JAVA, "java", false, &["number", "paid"], true).unwrap();
        assert!(
            after.contains("        return Objects.equals(getNumber(), other.getNumber())\n                && Objects.equals(isPaid(), other.isPaid());"),
            "{after}"
        );
        assert!(after.contains("        return Objects.hash(getNumber(), isPaid());"), "{after}");
        assert!(after.contains("import jakarta.persistence.ManyToOne;\nimport java.util.Objects;\nimport org.hibernate.Hibernate;\n"), "{after}");
    }

    #[test]
    fn existing_imports_are_not_duplicated() {
        let text = JAVA.replace("import jakarta.persistence.Id;\n", "import jakarta.persistence.Id;\nimport java.util.Objects;\nimport org.hibernate.Hibernate;\n");
        let after = equals(&text, "java", false, &["number"], true).unwrap();
        assert_eq!(after.matches("import java.util.Objects;").count(), 1);
        assert_eq!(after.matches("import org.hibernate.Hibernate;").count(), 1);
        let wildcard = JAVA.replace("import jakarta.persistence.Id;\n", "import jakarta.persistence.Id;\nimport java.util.*;\n");
        let after = equals(&wildcard, "java", false, &["number"], false).unwrap();
        assert!(!after.contains("import java.util.Objects;"), "{after}");
    }

    #[test]
    fn kotlin_variants_use_override_fun() {
        let hib = equals(KOTLIN, "kotlin", true, &[], true).unwrap();
        let expected = "    override fun equals(other: Any?): Boolean {\n        if (this === other) return true\n        if (other == null || Hibernate.getClass(this) != Hibernate.getClass(other)) return false\n        other as Invoice\n        return id != null && id == other.id\n    }\n\n    override fun hashCode(): Int = Hibernate.getClass(this).hashCode()\n}\n";
        assert!(hib.ends_with(expected), "{hib}");
        assert!(hib.contains("import jakarta.persistence.ManyToOne\nimport org.hibernate.Hibernate\n"), "{hib}");
        let plain = equals(KOTLIN, "kotlin", true, &[], false).unwrap();
        assert!(plain.contains("if (other !is Invoice) return false") && plain.contains("Invoice::class.hashCode()"), "{plain}");
        let key = equals(KOTLIN, "kotlin", false, &["number", "count"], false).unwrap();
        assert!(key.contains("return number == other.number\n            && count == other.count"), "{key}");
        assert!(key.contains("override fun hashCode(): Int = Objects.hash(number, count)") && key.contains("import java.util.Objects\n"), "{key}");
    }

    #[test]
    fn equality_refusals() {
        refused(equals(&JAVA.replace("    public Long getId()", "    @Override\n    public boolean equals(Object o) { return true; }\n    public Long getId()"), "java", true, &[], false), "already declared");
        refused(equals(&JAVA.replace("    public Long getId()", "    public int hashCode() { return 1; }\n    public Long getId()"), "java", true, &[], false), "already declared");
        refused(equals(&JAVA.replace("@Entity\n", "@Entity\n@lombok.Data\n"), "java", true, &[], false), "@Data");
        refused(equals(&JAVA.replace("@Entity\n", "@Entity\n@lombok.EqualsAndHashCode\n"), "java", true, &[], false), "@EqualsAndHashCode");
        refused(equals(&JAVA.replace("public class Invoice {", "public class Invoice extends Base {"), "java", true, &[], false), "superclass");
        refused(equals(&JAVA.replace("private Long id;", "private long id;").replace("public Long getId()", "public long getId()"), "java", true, &[], false), "primitive identifier");
        refused(equals(&JAVA.replace("    public Long getId() {\n        return id;\n    }\n\n", ""), "java", true, &[], false), "getter");
        refused(equals(&JAVA.replace("public class Invoice", "public record Invoice"), "java", true, &[], false), "Records");
        refused(equals(&KOTLIN.replace("var id: Long? = null", "var id: Long = 0"), "kotlin", true, &[], false), "non-null Kotlin identifier");
        refused(equals(&KOTLIN.replace("open class Invoice", "data class Invoice"), "kotlin", true, &[], false), "data classes");
        refused(equals(&KOTLIN.replace("open class Invoice {", "open class Invoice : Base() {"), "kotlin", true, &[], false), "superclass");
        refused(equals(JAVA, "java", false, &["id"], false), "offered business-key");
        refused(equals(JAVA, "java", false, &["customer"], false), "offered business-key");
        refused(equals(JAVA, "java", false, &["title"], false), "offered business-key");
        refused(equals(JAVA, "java", false, &["number", "number"], false), "exactly once");
        refused(equals(JAVA, "java", false, &[], false), "at least one");
        refused(equals(&JAVA.replace("import jakarta.persistence.Id;\n", "import jakarta.persistence.Id;\nimport other.Objects;\n"), "java", false, &["number"], false), "already used");
        refused(equals(&JAVA.replace("import jakarta.persistence.Id;\n", "import jakarta.persistence.Id;\nimport other.Hibernate;\n"), "java", true, &[], true), "already used");
    }

    #[test]
    fn generated_id_is_never_a_business_key_but_assigned_unique_value_is() {
        let _host = MockHost::install();
        let d = doc(JAVA, "java");
        let src = source(&d, &EQUALS_METHODS, &EQUALS_LOMBOK).unwrap();
        let names: Vec<_> = offered(&src, true).iter().map(|f| f.a.name.clone()).collect();
        assert_eq!(names, ["number", "paid"]);
        let all: Vec<_> = offered(&src, false).iter().map(|f| f.a.name.clone()).collect();
        assert_eq!(all, ["id", "number", "title", "paid"]);
    }

    #[test]
    fn to_string_shows_only_picked_scalars() {
        let java = to_string(JAVA, "java", &["id", "title"]).unwrap();
        assert!(
            java.ends_with("    @Override\n    public String toString() {\n        return \"Invoice{\"\n                + \"id=\" + id\n                + \", title=\" + title\n                + '}';\n    }\n}\n"),
            "{java}"
        );
        assert!(!java.contains("customer="));
        let kotlin = to_string(KOTLIN, "kotlin", &["number", "count"]).unwrap();
        assert!(kotlin.ends_with("    override fun toString(): String = \"Invoice(number=${number}, count=${count})\"\n}\n"), "{kotlin}");
        refused(to_string(JAVA, "java", &["customer"]), "never offered");
        refused(to_string(JAVA, "java", &[]), "at least one");
        refused(to_string(&JAVA.replace("    public Long getId()", "    public String toString() { return \"\"; }\n    public Long getId()"), "java", &["id"]), "already declared");
        refused(to_string(&JAVA.replace("@Entity\n", "@Entity\n@lombok.ToString\n"), "java", &["id"]), "@ToString");
        refused(to_string(&JAVA.replace("@Entity\n", "@Entity\n@lombok.Data\n"), "java", &["id"]), "@Data");
    }

    #[test]
    fn crlf_is_preserved() {
        let text = JAVA.replace('\n', "\r\n");
        let after = equals(&text, "java", true, &[], true).unwrap();
        assert_eq!(after.matches('\n').count(), after.matches("\r\n").count(), "{after:?}");
        assert!(after.contains("import org.hibernate.Hibernate;\r\n"));
        let after = to_string(&text, "java", &["id"]).unwrap();
        assert_eq!(after.matches('\n').count(), after.matches("\r\n").count());
    }

    fn setup(text: &str, language: &str, build: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let d = doc(text, language);
        host.open_editor(&d.path, language, text, &[]);
        host.add_file("build.gradle", build);
        (host, JpaAssist::default(), d)
    }
    fn run(text: &str, language: &str, build: &str, title: &str, rows: &[&[&str]]) -> Result<String, Error> {
        let (host, mut plugin, d) = setup(text, language, build);
        let pending = plugin.file_actions(&d, Range::default()).into_iter().find(|a| a.title == title).ok_or(Error::NotFound("not offered".into()))?;
        for row in rows {
            host.answer_next_prompt(Some(row));
        }
        let result = plugin.resolve_file_action(&d, pending)?;
        assert_eq!(host.editor_text(), text);
        assert!(result.operations.is_empty() && result.edits.len() == 1);
        patched(&d.text, &result.edits[0].1)
    }

    #[test]
    fn menu_and_prompts_round_trip_with_build_proof() {
        let with = run(JAVA, "java", GRADLE, TITLE_EQUALS, &[&["identifier"], &[]]).unwrap();
        assert!(with.contains("Hibernate.getClass(this)"), "{with}");
        let without = run(JAVA, "java", "plugins {}\n", TITLE_EQUALS, &[&["identifier"], &[]]).unwrap();
        assert!(without.contains("o instanceof Invoice") && !without.contains("Hibernate"), "{without}");
        let key = run(JAVA, "java", GRADLE, TITLE_EQUALS, &[&["business-key"], &["number"], &[]]).unwrap();
        assert!(key.contains("Objects.hash(getNumber())"), "{key}");
        let text = run(JAVA, "java", GRADLE, TITLE_TO_STRING, &[&["id", "number"], &[]]).unwrap();
        assert!(text.contains("+ \"id=\" + id"), "{text}");
        let spring = "plugins { id 'org.springframework.boot' version '3.3.0' }\ndependencies { implementation 'org.springframework.boot:spring-boot-starter-data-jpa' }\n";
        assert!(run(JAVA, "java", spring, TITLE_EQUALS, &[&["identifier"], &[]]).unwrap().contains("Hibernate.getClass"));
        let five = "dependencies { implementation 'org.hibernate:hibernate-core:5.6.15.Final' }\n";
        assert!(!run(JAVA, "java", five, TITLE_EQUALS, &[&["identifier"], &[]]).unwrap().contains("Hibernate"));
    }

    #[test]
    fn cancel_and_stale_and_not_offered() {
        let (host, mut plugin, d) = setup(JAVA, "java", GRADLE);
        let pending = plugin.file_actions(&d, Range::default()).into_iter().find(|a| a.title == TITLE_EQUALS).unwrap();
        host.answer_next_prompt(Some(&["identifier"]));
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&d, pending.clone()), Err(Error::Cancelled)));
        let mut changed = d.clone();
        changed.version += 1;
        changed.text.push('\n');
        assert!(matches!(plugin.resolve_file_action(&changed, pending), Err(Error::Stale)));
        let with_methods = JAVA.replace("    public Long getId()", "    public String toString() { return \"\"; }\n    public Long getId()");
        assert!(matches!(run(&with_methods, "java", GRADLE, TITLE_TO_STRING, &[]), Err(Error::NotFound(_))));
        assert!(matches!(run(&JAVA.replace("@Entity\n", "@Entity\n@lombok.Data\n"), "java", GRADLE, TITLE_EQUALS, &[]), Err(Error::NotFound(_))));
    }

    fn javac_run(label: &str, classpath: &str, files: &[(&str, String)], main: Option<&str>) {
        let dir = std::env::temp_dir().join(format!("jpa-equality-{}-{label}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths: Vec<_> = files
            .iter()
            .map(|(file, body)| {
                let path = dir.join(file);
                std::fs::write(&path, body).unwrap();
                path
            })
            .collect();
        let result = std::process::Command::new("javac")
            .args(["--release", "17", "-proc:none", "-classpath", classpath, "-d"])
            .arg(&dir)
            .args(&paths)
            .output()
            .expect("javac installed");
        assert!(result.status.success(), "{label}: {}", String::from_utf8_lossy(&result.stderr));
        if let Some(main) = main {
            let run = std::process::Command::new("java").arg("-ea").args(["-cp", &format!("{}:{classpath}", dir.display()), main]).output().expect("java installed");
            assert!(run.status.success(), "{label}: {}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    const MAIN: &str = "package shop;\npublic class Main {\n    public static void main(String[] args) {\n        Invoice a = new Invoice(); Invoice b = new Invoice();\n        assert !a.equals(b) : \"transient instances are not equal\";\n        assert a.equals(a);\n        a.setId(1L); b.setId(1L);\n        assert a.equals(b) && b.equals(a) : \"same id\";\n        assert a.hashCode() == b.hashCode();\n        Proxy p = new Proxy(); p.setId(1L);\n        assert a.equals(p) && p.equals(a) : \"subclass standing in for a proxy\";\n        b.setId(2L);\n        assert !a.equals(b);\n        assert !a.equals(null) && !a.equals(\"x\");\n        int before = a.hashCode(); a.setId(9L);\n        assert before == a.hashCode() : \"hash is stable across id assignment\";\n    }\n}\n";
    const PROXY: &str = "package shop;\npublic class Proxy extends Invoice {\n    private Long target;\n    @Override public Long getId() { return target; }\n    @Override public void setId(Long id) { this.target = id; }\n}\n";
    const CUSTOMER: &str = "package shop;\npublic class Customer {}\n";

    fn compile_env() -> Option<String> {
        let api = std::env::var("JPA_JAKARTA_API").ok()?;
        Some(api)
    }

    #[test]
    #[ignore = "requires javac, java and JPA_JAKARTA_API / HIBERNATE_CORE_JAR from scripts/jpa-compile-env.sh"]
    fn generated_java_equality_compiles_and_behaves_like_a_proxy_safe_equality() {
        let api = compile_env().expect("JPA_JAKARTA_API");
        let core = std::env::var("HIBERNATE_CORE_JAR").expect("HIBERNATE_CORE_JAR");
        let setters = JAVA.replace("    public Long getId() {", "    public void setId(Long id) {\n        this.id = id;\n    }\n\n    public Long getId() {");
        let text = setters;
        for (label, hibernate) in [("fallback", false), ("hibernate", true)] {
            let after = equals(&text, "java", true, &[], hibernate).unwrap();
            let classpath = if hibernate { format!("{api}:{core}") } else { api.clone() };
            let files = [("Invoice.java", after), ("Customer.java", CUSTOMER.into()), ("Proxy.java", PROXY.into()), ("Main.java", MAIN.into())];
            if hibernate {
                // Hibernate.getClass(this) on a plain subclass returns the subclass: only a real proxy unwraps.
                javac_run(label, &classpath, &files[..3], None);
            } else {
                javac_run(label, &classpath, &files, Some("shop.Main"));
            }
        }
        let key = equals(&text, "java", false, &["number", "paid"], true).unwrap();
        javac_run("key", &format!("{api}:{core}"), &[("Invoice.java", key), ("Customer.java", CUSTOMER.into())], None);
        let shown = to_string(&text, "java", &["id", "number", "title", "paid"]).unwrap();
        javac_run("tostring", &api, &[("Invoice.java", shown), ("Customer.java", CUSTOMER.into())], None);
    }

    #[test]
    #[ignore = "requires kotlinc classpath from scripts/jpa-compile-env.sh"]
    fn generated_kotlin_equality_compiles() {
        let compiler = std::env::var("KOTLIN_COMPILER_CLASSPATH").expect("KOTLIN_COMPILER_CLASSPATH");
        let api = std::env::var("JPA_JAKARTA_API").expect("JPA_JAKARTA_API");
        let core = std::env::var("HIBERNATE_CORE_JAR").expect("HIBERNATE_CORE_JAR");
        let stdlib = std::env::var("KOTLIN_STDLIB").expect("KOTLIN_STDLIB");
        let customer = "package shop\nopen class Customer\n";
        let outputs = [
            ("hib", equals(KOTLIN, "kotlin", true, &[], true).unwrap()),
            ("fall", equals(KOTLIN, "kotlin", true, &[], false).unwrap()),
            ("key", equals(KOTLIN, "kotlin", false, &["number", "count"], true).unwrap()),
            ("str", to_string(KOTLIN, "kotlin", &["id", "number", "count"]).unwrap()),
        ];
        for (label, source) in outputs {
            let dir = std::env::temp_dir().join(format!("jpa-equality-kt-{}-{label}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("Invoice.kt"), source).unwrap();
            std::fs::write(dir.join("Customer.kt"), customer).unwrap();
            let result = std::process::Command::new("java")
                .args(["-cp", &compiler, "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler", "-no-stdlib", "-nowarn", "-cp", &format!("{api}:{core}:{stdlib}"), "-d"])
                .arg(dir.join("out"))
                .arg(dir.join("Invoice.kt"))
                .arg(dir.join("Customer.kt"))
                .output()
                .expect("java installed");
            assert!(result.status.success(), "{label}: {}{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
