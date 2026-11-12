//! Literal same-source-root checks: identifier absence along a closed `extends` chain and
//! `@Convert(converter = X.class)` naming a class that cannot implement AttributeConverter.
//! Jakarta Persistence 3.2 section 2.4 (primary key declared by root entity or mapped superclass)
//! and section 10.6 (converter classes must implement AttributeConverter). XML mappings are read only from a unique orm.xml set.
use crate::{
    attribute_info,
    checks::Finding,
    model::{self, Class, Kind},
};
use openidea_plugin::{Severity, host, providers::Document, text::offset};

const MAX_HOPS: usize = 8;
const MAX_TEXT: usize = 128 * 1024;

pub(super) fn read(path: &str) -> Option<String> {
    host::project_of(path)?.read_file(path).ok().filter(|t| t.len() <= MAX_TEXT)
}

fn valid_name(name: &str) -> bool {
    name.split('.').all(|part| {
        !part.is_empty() && part.bytes().enumerate().all(|(i, b)| model::is_ident(b) && !(i == 0 && b.is_ascii_digit()))
    })
}

pub(super) fn resolve(class: &Class, name: &str) -> Option<String> {
    if !valid_name(name) || name == class.name {
        return None;
    }
    if name.contains('.') {
        return name.starts_with(|c: char| c.is_ascii_lowercase()).then(|| name.to_owned());
    }
    if class.imports.iter().any(|i| i.ends_with(".*")) {
        return None;
    }
    let suffix = format!(".{name}");
    let mut imported = class.imports.iter().filter(|i| !i.starts_with("static ") && i.ends_with(&suffix));
    match (imported.next(), imported.next()) {
        (Some(_), Some(_)) => None,
        (Some(import), None) => Some(import.clone()),
        (None, _) => Some(format!("{}.{name}", class.package.as_ref()?)),
    }
}

pub(super) fn sibling_path(class: &Class, fqcn: &str) -> Option<String> {
    let package = class.package.as_ref()?;
    let directory = class.path.rsplit_once('/')?.0;
    let base = directory.strip_suffix(&format!("/{}", package.replace('.', "/")))?;
    Some(format!("{base}/{}.java", fqcn.replace('.', "/")))
}

pub(super) fn header<'a>(masked: &'a str, class: &Class) -> Option<&'a str> {
    let (at, _) = model::declaration_keyword(masked, 0, &["class"])?;
    let open = at + masked[at..].find('{')?;
    let rest = masked[at + 5..open].trim().strip_prefix(class.name.as_str())?;
    if rest.bytes().next().is_some_and(model::is_ident) || rest.trim_start().starts_with('<') {
        return None;
    }
    Some(rest.trim())
}

pub(super) fn extended(header: &str) -> Option<&str> {
    let rest = header.strip_prefix("extends")?;
    if rest.bytes().next().is_some_and(model::is_ident) {
        return None;
    }
    let rest = rest.trim_start();
    let end = rest.find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '$' | '.'))).unwrap_or(rest.len());
    let tail = rest[end..].trim_start();
    (tail.is_empty() || tail.starts_with("implements")).then_some(&rest[..end])
}

pub(super) fn identifier_absent(doc: &Document) -> Option<Finding> {
    hierarchy_identifier_absent(doc, &read)
}

fn hierarchy_identifier_absent(doc: &Document, load: &dyn Fn(&str) -> Option<String>) -> Option<Finding> {
    let class = attribute_info::snapshot(doc)?;
    if class.kind != Kind::Entity || class.has_id {
        return None;
    }
    let masked = model::mask(&doc.text);
    let mut parent = resolve(&class, extended(header(&masked, &class)?)?)?;
    let mut current = class.clone();
    let mut chain = vec![format!("{}.{}", class.package.as_deref()?, class.name)];
    for _ in 0..MAX_HOPS {
        let path = sibling_path(&current, &parent)?;
        let ancestor = Document { path: path.clone(), language: "java".into(), text: load(&path)?, version: 0 };
        let up = attribute_info::snapshot(&ancestor)?;
        if format!("{}.{}", up.package.as_deref()?, up.name) != parent || up.kind == Kind::Embeddable || up.has_id {
            return None;
        }
        let masked = model::mask(&ancestor.text);
        let head = header(&masked, &up)?;
        chain.push(parent.clone());
        if !model::has_word(head, "extends") {
            let orm = crate::orm_xml::load(&doc.path);
            if matches!(orm, crate::orm_xml::Orm::Unresolved) || chain.iter().any(|c| orm.declares_identifier(c)) {
                return None;
            }
            let from = offset(&doc.text, class.pos)?;
            return Some(Finding::new(
                from,
                from + class.name.len(),
                Severity::Info,
                "jpa-source-hierarchy-identifier-absent",
                "No class in this entity's literal extends chain declares an Id, EmbeddedId or IdClass in saved source; the primary key must come from the root entity or a mapped superclass. Resolved XML mappings do not declare one either; provider mappings are unresolved.",
            ));
        }
        parent = resolve(&up, extended(head)?)?;
        current = up;
    }
    None
}

pub(super) fn converter_findings(doc: &Document) -> Vec<Finding> {
    converter_findings_with(doc, &read)
}

fn converter_findings_with(doc: &Document, load: &dyn Fn(&str) -> Option<String>) -> Vec<Finding> {
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
    let Some(class) = model::parse_class(&doc.text, &doc.path) else { return vec![] };
    let owned = |written: &str| {
        let known = |n: &str| matches!(n, "jakarta.persistence.Convert" | "javax.persistence.Convert");
        if written.contains('.') {
            return known(written);
        }
        let mut imports = class.imports.iter().filter(|i| i.rsplit('.').next() == Some("Convert"));
        matches!((imports.next(), imports.next()), (Some(i), None) if known(i)) && class.name != "Convert"
    };
    let mut findings = vec![];
    for a in model::annotations(&masked).iter().filter(|a| a.name == "Convert").take(32) {
        if !owned(&masked[a.start + 1..a.name_end]) {
            continue;
        }
        let Some((from, to)) = a.args else { continue };
        let args = &masked[from..to];
        let mut named = model::words(args, "converter");
        let (Some(at), None) = (named.next(), named.next()) else { continue };
        let after = &args[at + 9..];
        let Some(eq) = after.find('=') else { continue };
        if !after[..eq].trim().is_empty() {
            continue;
        }
        let value = &after[eq + 1..];
        let trimmed = value.trim_start();
        let literal = trimmed[..trimmed.find(',').unwrap_or(trimmed.len())].trim_end();
        let Some(name) = literal.strip_suffix(".class") else { continue };
        let Some(fqcn) = resolve(&class, name) else { continue };
        let Some(path) = sibling_path(&class, &fqcn) else { continue };
        let Some(text) = load(&path) else { continue };
        let start = from + at + 9 + eq + 1 + (value.len() - trimmed.len());
        let Some((severity, message)) = converter_problem(&text, &fqcn) else {
            let plain = !model::has_word(args, "attributeName") && !model::has_word(args, "disableConversion");
            let declared = plain.then(|| attribute_type(doc, &masked, &class, a)).flatten();
            if let (Some(declared), Some(converts)) = (declared, converter_source_type(&text, &fqcn)) {
                let by_jdk = jdk_type(&declared, &class.imports).zip(jdk_type(&converts.0, &converts.1));
                let mismatch = match by_jdk {
                    Some((d, c)) => Some(!jdk_assignable(&d, &c)),
                    None => plain_class(&text, &path, &fqcn).and_then(|converter| project_mismatch(&class, &declared, &converter, &converts.0, load)),
                };
                if mismatch == Some(true) {
                    findings.push(Finding::new(start, start + literal.len(), Severity::Warning, "jpa-converter-type-mismatch", format!("The attribute is declared {declared}, but this AttributeConverter converts {}, which is not assignable from it (Jakarta Persistence 3.1, section 10.6).", converts.0)));
                }
            }
            continue;
        };
        findings.push(Finding::new(start, start + literal.len(), severity, "jpa-converter-not-attribute-converter", message));
    }
    findings
}

fn attribute_type(doc: &Document, masked: &str, class: &Class, a: &model::Annotation) -> Option<String> {
    let (at, _) = model::declaration_keyword(masked, 0, &["class"])?;
    let open = at + masked[at..].find('{')?;
    let (from, to) = model::members(masked, open + 1, class.body_end?, false, false).into_iter().find(|&(f, t)| f <= a.start && a.start < t)?;
    let mut inside = class.attributes.iter().filter(|x| offset(&doc.text, x.pos).is_some_and(|p| from <= p && p < to));
    match (inside.next(), inside.next()) {
        (Some(attribute), None) => Some(attribute.ty.clone()),
        _ => None,
    }
}

fn jdk_assignable(declared: &str, converts: &str) -> bool {
    let supers: &[&str] = match declared {
        "String" => &["CharSequence", "Comparable", "Serializable"],
        "Long" | "Integer" | "Short" | "Byte" | "Double" | "Float" | "BigDecimal" | "BigInteger" => &["Number", "Comparable", "Serializable"],
        "Object" => &[],
        "Number" | "Serializable" | "CharSequence" | "Comparable" => &[],
        _ => &["Comparable", "Serializable"],
    };
    declared == converts || converts == "Object" || supers.contains(&converts)
}

fn plain_class(text: &str, path: &str, fqcn: &str) -> Option<Class> {
    let (package, name) = fqcn.rsplit_once('.')?;
    let imports = text.lines().filter_map(|l| l.trim().strip_prefix("import ")).map(|r| r.trim().trim_end_matches(';').trim().to_owned()).collect();
    Some(Class {
        name: name.to_owned(),
        kind: Kind::Entity,
        kotlin: false,
        path: path.to_owned(),
        pos: openidea_plugin::text::position(text, 0),
        package: Some(package.to_owned()),
        imports,
        table: None,
        superclass: None,
        has_id: false,
        attributes: vec![],
        body_end: None,
    })
}

/// `Some(true)` only when both are classes in saved project sources and the declared class's closed `extends` chain never reaches the converter's type.
fn project_mismatch(class: &Class, declared: &str, converter: &Class, converts: &str, load: &dyn Fn(&str) -> Option<String>) -> Option<bool> {
    let target = resolve(converter, converts)?;
    converter_head(&load(&sibling_path(converter, &target)?)?, &target)?;
    let mut current = resolve(class, declared)?;
    let mut from = class.clone();
    for _ in 0..MAX_HOPS {
        if current == target {
            return Some(false);
        }
        let path = sibling_path(&from, &current)?;
        let text = load(&path)?;
        let head = converter_head(&text, &current)?;
        if !head.starts_with("extends") {
            return (!head.starts_with('<')).then_some(true);
        }
        from = plain_class(&text, &path, &current)?;
        current = resolve(&from, extended(&head)?)?;
    }
    None
}

fn jdk_type(ty: &str, imports: &[String]) -> Option<String> {
    const LANG: [&str; 13] = ["Object", "Number", "CharSequence", "Comparable", "String", "Long", "Integer", "Short", "Byte", "Boolean", "Double", "Float", "Character"];
    const OTHER: [&str; 12] = ["Serializable", "BigDecimal", "BigInteger", "LocalDate", "LocalDateTime", "LocalTime", "Instant", "OffsetDateTime", "ZonedDateTime", "Duration", "UUID", "Date"];
    let boxed = match ty {
        "long" => "Long",
        "int" => "Integer",
        "short" => "Short",
        "byte" => "Byte",
        "boolean" => "Boolean",
        "double" => "Double",
        "float" => "Float",
        "char" => "Character",
        other => other,
    };
    if boxed.contains('.') {
        let (package, simple) = boxed.rsplit_once('.')?;
        let jdk = matches!(package, "java.lang" | "java.io" | "java.math" | "java.time" | "java.util");
        return (jdk && (LANG.contains(&simple) || OTHER.contains(&simple))).then(|| simple.to_owned());
    }
    if LANG.contains(&boxed) {
        return Some(boxed.to_owned());
    }
    let suffix = format!(".{boxed}");
    let mut imported = imports.iter().filter(|i| i.ends_with(&suffix));
    let jdk = matches!((imported.next(), imported.next()), (Some(i), None) if i.starts_with("java."));
    (jdk && OTHER.contains(&boxed)).then(|| boxed.to_owned())
}

/// The first type argument of the single `AttributeConverter<X, Y>` the class lists, with that file's imports.
fn converter_source_type(text: &str, fqcn: &str) -> Option<(String, Vec<String>)> {
    let head = converter_head(text, fqcn)?;
    let mut at = model::words(&head, "AttributeConverter");
    let (Some(start), None) = (at.next(), at.next()) else { return None };
    let open = start + "AttributeConverter".len();
    if !head[open..].starts_with('<') {
        return None;
    }
    let close = open + head[open..].find('>')?;
    let inner = &head[open + 1..close];
    let parts = model::split_top_level(inner);
    let [(_, first), (_, _)] = parts.as_slice() else { return None };
    let first = first.trim();
    if first.contains(['<', '?', '[']) {
        return None;
    }
    let imports = text.lines().filter_map(|l| l.trim().strip_prefix("import ")).map(|r| r.trim().trim_end_matches(';').trim().to_owned()).collect();
    Some((first.to_owned(), imports))
}

fn converter_problem(text: &str, fqcn: &str) -> Option<(Severity, String)> {
    let head = converter_head(text, fqcn)?;
    let simple = fqcn.rsplit_once('.')?.1;
    if head.starts_with('<') || model::has_word(&head, "extends") || model::has_word(&head, "AttributeConverter") {
        return None;
    }
    if head.is_empty() {
        return Some((Severity::Warning, format!("{simple} neither extends nor implements anything, so it cannot implement jakarta.persistence.AttributeConverter, which every attribute converter class must (Jakarta Persistence 3.2, section 10.6).")));
    }
    Some((Severity::Info, format!("{simple} lists no AttributeConverter among its interfaces in saved source; an in-project interface could still extend it, which is not resolved here.")))
}

pub(super) fn converter_head(text: &str, fqcn: &str) -> Option<String> {
    let (package, simple) = fqcn.rsplit_once('.')?;
    let masked = model::mask(text);
    let declared = masked.lines().find_map(|l| l.trim().strip_prefix("package "))?;
    if declared.trim().trim_end_matches(';').trim() != package {
        return None;
    }
    for keyword in ["interface", "enum", "record"] {
        if model::words(&masked, keyword).any(|at| masked[at + keyword.len()..].trim_start().starts_with(simple)) {
            return None;
        }
    }
    let mut top = model::words(&masked, "class").filter(|&at| {
        masked[at + 5..].trim_start().strip_prefix(simple).is_some_and(|r| !r.bytes().next().is_some_and(model::is_ident))
            && !masked[..at].trim_end().ends_with('.')
    });
    let (Some(at), None) = (top.next(), top.next()) else { return None };
    let depth = masked[..at].bytes().fold(0i32, |d, b| d + i32::from(b == b'{') - i32::from(b == b'}'));
    if depth != 0 {
        return None;
    }
    let open = at + masked[at..].find('{')?;
    Some(masked[at + 5..open].trim().strip_prefix(simple)?.trim().to_owned())
}
