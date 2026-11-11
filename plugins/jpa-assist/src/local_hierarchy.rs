//! Literal same-source-root checks: identifier absence along a closed `extends` chain and
//! `@Convert(converter = X.class)` naming a class that cannot implement AttributeConverter.
//! Jakarta Persistence 3.2 section 2.4 (primary key declared by root entity or mapped superclass)
//! and section 10.6 (converter classes must implement AttributeConverter). XML mappings are unresolved.
use crate::{
    attribute_info,
    checks::Finding,
    model::{self, Class, Kind},
};
use openidea_plugin::{Severity, host, providers::Document, text::offset};

const MAX_HOPS: usize = 8;
const MAX_TEXT: usize = 128 * 1024;

fn read(path: &str) -> Option<String> {
    host::project_of(path)?.read_file(path).ok().filter(|t| t.len() <= MAX_TEXT)
}

fn valid_name(name: &str) -> bool {
    name.split('.').all(|part| {
        !part.is_empty() && part.bytes().enumerate().all(|(i, b)| model::is_ident(b) && !(i == 0 && b.is_ascii_digit()))
    })
}

fn resolve(class: &Class, name: &str) -> Option<String> {
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

fn sibling_path(class: &Class, fqcn: &str) -> Option<String> {
    let package = class.package.as_ref()?;
    let directory = class.path.rsplit_once('/')?.0;
    let base = directory.strip_suffix(&format!("/{}", package.replace('.', "/")))?;
    Some(format!("{base}/{}.java", fqcn.replace('.', "/")))
}

fn header<'a>(masked: &'a str, class: &Class) -> Option<&'a str> {
    let (at, _) = model::declaration_keyword(masked, 0, &["class"])?;
    let open = at + masked[at..].find('{')?;
    let rest = masked[at + 5..open].trim().strip_prefix(class.name.as_str())?;
    if rest.bytes().next().is_some_and(model::is_ident) || rest.trim_start().starts_with('<') {
        return None;
    }
    Some(rest.trim())
}

fn extended(header: &str) -> Option<&str> {
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
    for _ in 0..MAX_HOPS {
        let path = sibling_path(&current, &parent)?;
        let ancestor = Document { path: path.clone(), language: "java".into(), text: load(&path)?, version: 0 };
        let up = attribute_info::snapshot(&ancestor)?;
        if format!("{}.{}", up.package.as_deref()?, up.name) != parent || up.kind == Kind::Embeddable || up.has_id {
            return None;
        }
        let masked = model::mask(&ancestor.text);
        let head = header(&masked, &up)?;
        if !model::has_word(head, "extends") {
            let from = offset(&doc.text, class.pos)?;
            return Some(Finding::new(
                from,
                from + class.name.len(),
                Severity::Info,
                "jpa-source-hierarchy-identifier-absent",
                "No class in this entity's literal extends chain declares an Id, EmbeddedId or IdClass in saved source; the primary key must come from the root entity or a mapped superclass. XML/provider mappings are unresolved.",
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
        let Some((severity, message)) = converter_problem(&text, &fqcn) else { continue };
        let start = from + at + 9 + eq + 1 + (value.len() - trimmed.len());
        findings.push(Finding::new(start, start + literal.len(), severity, "jpa-converter-not-attribute-converter", message));
    }
    findings
}

fn converter_problem(text: &str, fqcn: &str) -> Option<(Severity, String)> {
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
    let head = masked[at + 5..open].trim().strip_prefix(simple)?.trim();
    if head.starts_with('<') || model::has_word(head, "extends") || model::has_word(head, "AttributeConverter") {
        return None;
    }
    if head.is_empty() {
        return Some((Severity::Warning, format!("{simple} neither extends nor implements anything, so it cannot implement jakarta.persistence.AttributeConverter, which every attribute converter class must (Jakarta Persistence 3.2, section 10.6).")));
    }
    Some((Severity::Info, format!("{simple} lists no AttributeConverter among its interfaces in saved source; an in-project interface could still extend it, which is not resolved here.")))
}
#[cfg(test)]
mod tests {
    use crate::JpaAssist;
    use openidea_plugin::{Diagnostic, diagnostics::DiagnosticsProvider, mock::MockHost, providers::Document};
    const IMPORTS: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.MappedSuperclass;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.Convert;\n";
    const HIERARCHY: &str = "jpa-source-hierarchy-identifier-absent";
    const CONVERTER: &str = "jpa-converter-not-attribute-converter";
    fn run(host: &MockHost, name: &str, body: &str, code: &str) -> Vec<Diagnostic> {
        let path = format!("src/main/java/shop/{name}.java");
        let text = format!("{IMPORTS}{body}");
        host.add_file(&path, &text);
        let doc = Document { path: format!("/project/{path}"), language: "java".into(), text, version: 1 };
        JpaAssist::default().diagnostics(&doc).into_iter().filter(|d| d.code.as_deref() == Some(code)).collect()
    }
    #[test]
    fn hierarchy_without_identifier_is_noted_only_for_a_closed_literal_chain() {
        let host = MockHost::install();
        run(&host, "Bare", "@MappedSuperclass class Bare { String x; }", HIERARCHY);
        run(&host, "Keyed", "@MappedSuperclass class Keyed { @Id Long id; }", HIERARCHY);
        run(&host, "Mid", "@MappedSuperclass class Mid extends Bare { }", HIERARCHY);
        run(&host, "KeyedMid", "@MappedSuperclass class KeyedMid extends Keyed { }", HIERARCHY);
        for parent in ["Bare", "Mid"] {
            let found = run(&host, "Child", &format!("@Entity class Child extends {parent} {{ }}"), HIERARCHY);
            assert_eq!(found.len(), 1, "{parent}");
            assert_eq!(found[0].severity, openidea_plugin::Severity::Info);
        }
        for parent in ["Keyed", "KeyedMid", "Missing", "other.Base", "Child"] {
            assert!(run(&host, "Child", &format!("@Entity class Child extends {parent} {{ }}"), HIERARCHY).is_empty(), "{parent}");
        }
        assert!(run(&host, "Child", "@Entity class Child extends Bare { @Id Long id; }", HIERARCHY).is_empty());
        assert!(run(&host, "Child", "@Entity class Child extends Bare<T> { }", HIERARCHY).is_empty());
        host.add_file("src/main/java/shop/Mid.java", &format!("{IMPORTS}@MappedSuperclass class Mid extends Library {{ }}"));
        assert!(run(&host, "Child", "@Entity class Child extends Mid { }", HIERARCHY).is_empty());
    }
    #[test]
    fn convert_converter_without_attribute_converter_is_reported() {
        let host = MockHost::install();
        let entity = "@Entity class Order { @Id Long id; @Convert(converter = Plain.class) String s; }";
        let convert = |converter: &str| {
            host.add_file("src/main/java/shop/Plain.java", &format!("package shop;\n{converter}"));
            run(&host, "Order", entity, CONVERTER)
        };
        let found = convert("class Plain { }");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, openidea_plugin::Severity::Warning);
        assert_eq!(convert("class Plain implements Marker { }")[0].severity, openidea_plugin::Severity::Info);
        for ok in [
            "class Plain implements AttributeConverter<String, String> { }",
            "class Plain implements jakarta.persistence.AttributeConverter<String, String> { }",
            "class Plain extends Base { }",
            "class Plain<T> { }",
            "interface Plain { }",
        ] {
            assert!(convert(ok).is_empty(), "{ok}");
        }
        host.add_file("src/main/java/shop/Plain.java", "package other;\nclass Plain { }");
        assert!(run(&host, "Order", entity, CONVERTER).is_empty());
        host.add_file("src/main/java/shop/Plain.java", "package shop;\nclass Plain { }");
        let foreign = "@Entity class Order { @Id Long id; @vendor.Convert(converter = Plain.class) String s; }";
        assert!(run(&host, "Order", foreign, CONVERTER).is_empty());
        let missing = "@Entity class Order { @Id Long id; @Convert(converter = Gone.class) String s; }";
        assert!(run(&host, "Order", missing, CONVERTER).is_empty());
    }
}
