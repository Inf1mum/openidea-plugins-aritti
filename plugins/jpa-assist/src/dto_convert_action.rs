//! Reviewed single-document conversion between a Java record and a plain JavaBean class, plus the
//! shared "reuse an existing compatible DTO" discovery used by the DTO creation flows.
//!
//! Facts (JLS 21 section 8.10): a record has a canonical constructor and one `x()` accessor per
//! component, no instance fields beyond the components, no instance initializers, optional static
//! members and compact constructors, `Record` as superclass (so it cannot extend a class) and
//! annotations on components propagate to every applicable member. Hence a converted class
//! exposes `getX()` while the record exposed `x()`: callers in other files are not updated.
//! Spring Data's projections page says records fit class-based DTOs (constructor parameter names
//! select fields) and any class with a single constructor works too; Jackson supports records
//! since 2.12.0 (databind #2709).
//!
//! Supported annotations (moved between record component and private field, verified by the
//! opt-in `generated_` compile test): Jackson `@JsonProperty` and common Bean Validation
//! constraints. Everything else is refused, never dropped.
use super::*;

pub(super) const TO_CLASS_TITLE: &str = "Convert record to class…";
pub(super) const TO_RECORD_TITLE: &str = "Convert class to record…";
const MAX_COMPONENTS: usize = 32;
const VALIDATION: [&str; 21] = [
    "NotNull", "NotBlank", "NotEmpty", "Size", "Min", "Max", "Pattern", "Email", "Positive", "PositiveOrZero", "Negative",
    "NegativeOrZero", "Past", "PastOrPresent", "Future", "FutureOrPresent", "DecimalMin", "DecimalMax", "Digits", "AssertTrue",
    "AssertFalse",
];
const SUPPORTED: &str = "Jackson @JsonProperty and Bean Validation constraints (NotNull, NotBlank, NotEmpty, Size, Min, Max, Pattern, Email, Positive, PositiveOrZero, Negative, NegativeOrZero, Past, PastOrPresent, Future, FutureOrPresent, DecimalMin, DecimalMax, Digits, AssertTrue, AssertFalse), written unqualified and imported";

#[derive(Clone, PartialEq, Debug)]
pub(super) struct Field {
    pub(super) name: String,
    pub(super) ty: String,
    pub(super) annotations: Vec<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Keyword {
    Record,
    Class,
    Interface,
    Enum,
}
const KEYWORDS: [(&str, Keyword); 4] =
    [("record", Keyword::Record), ("class", Keyword::Class), ("interface", Keyword::Interface), ("enum", Keyword::Enum)];

struct Parsed {
    name: String,
    public: bool,
    span: (usize, usize),
    fields: Vec<Field>,
}

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn declarations(masked: &str) -> Vec<(usize, Keyword)> {
    let mut found = Vec::new();
    for (word, keyword) in KEYWORDS {
        for at in words(masked, word) {
            let before = masked[..at].trim_end();
            let rest = masked[at + word.len()..].trim_start();
            if !(before.ends_with('.') || before.ends_with("::")) && rest.starts_with(|c: char| c.is_alphabetic() || c == '_') {
                found.push((at, keyword));
            }
        }
    }
    found.sort_by_key(|(at, _)| *at);
    found
}

fn sole(doc: &Document, masked: &str, expected: Keyword) -> Result<usize, String> {
    if doc.language != "java" || !doc.path.ends_with(".java") {
        return Err("Only Java sources are converted.".into());
    }
    match declarations(masked).as_slice() {
        [(at, keyword)] if *keyword == expected => Ok(*at),
        _ => Err("Exactly one top-level declaration of the expected kind is required (nested, extra or other types are refused).".into()),
    }
}

pub(super) fn eligible_record(doc: &Document) -> bool {
    doc.language == "java" && doc.path.ends_with(".java") && matches!(declarations(&mask(&doc.text)).as_slice(), [(_, Keyword::Record)])
}

pub(super) fn eligible_class(doc: &Document) -> bool {
    let masked = mask(&doc.text);
    doc.language == "java"
        && doc.path.ends_with(".java")
        && matches!(declarations(&masked).as_slice(), [(_, Keyword::Class)])
        && !["Entity", "Embeddable", "MappedSuperclass"].iter().any(|name| has_word(&masked, name))
}

fn split_top(masked: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    let (mut depth, mut from, mut parts) = (0i32, start, Vec::new());
    for (at, ch) in masked[start..end].char_indices() {
        match ch {
            '<' | '(' | '[' | '{' => depth += 1,
            '>' | ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push((from, start + at));
                from = start + at + 1;
            }
            _ => {}
        }
    }
    parts.push((from, end));
    parts.retain(|&(s, e)| !masked[s..e].trim().is_empty());
    parts
}

fn spelled(text: &str, span: (usize, usize), a: &crate::model::Annotation) -> String {
    text[span.0 + a.start..span.0 + a.end()].split_whitespace().collect::<Vec<_>>().join(" ")
}

fn written_annotations(text: &str, masked: &str, span: (usize, usize)) -> Vec<(String, bool, String)> {
    annotations(&masked[span.0..span.1])
        .iter()
        .map(|a| (a.name.to_owned(), masked[span.0 + a.start + 1..span.0 + a.name_end].contains('.'), spelled(text, span, a)))
        .collect()
}

fn vetted(doc: &Document, masked: &str, span: (usize, usize), name: &str) -> Result<Vec<String>, String> {
    let imports: Vec<String> = mask_imports(masked);
    let mut out = Vec::new();
    for (annotation, qualified, written) in written_annotations(&doc.text, masked, span) {
        let packages: &[&str] = if annotation == "JsonProperty" {
            &["com.fasterxml.jackson.annotation"]
        } else if VALIDATION.contains(&annotation.as_str()) {
            &["jakarta.validation.constraints", "javax.validation.constraints"]
        } else {
            &[]
        };
        let imported = packages.iter().any(|p| imports.contains(&format!("{p}.{annotation}")) || imports.contains(&format!("{p}.*")));
        if qualified || !imported || out.contains(&written) {
            return Err(format!("`{name}`: annotation @{annotation} cannot be moved safely; supported: {SUPPORTED}."));
        }
        out.push(written);
    }
    Ok(out)
}

fn mask_imports(masked: &str) -> Vec<String> {
    masked
        .lines()
        .filter_map(|line| line.trim().strip_prefix("import "))
        .map(|rest| rest.trim().trim_end_matches(';').trim().to_owned())
        .collect()
}

fn header(masked: &str, at: usize, word: &str) -> Result<(String, bool, usize, usize), String> {
    let boundary = masked[..at].rfind([';', '}']).map_or(0, |i| i + 1);
    let prefix = &masked[boundary..at];
    if prefix.contains('@') {
        return Err("The type declaration carries annotations; they are not moved.".into());
    }
    let tokens: Vec<_> = prefix.split_whitespace().collect();
    if tokens.iter().any(|t| *t != "public" && *t != "final") {
        return Err("Only a `public` or `final` modifier on the type is supported.".into());
    }
    let start = tokens.first().map_or(at, |first| boundary + prefix.find(first).unwrap_or(0));
    let after = &masked[at + word.len()..];
    let trimmed = after.trim_start();
    let name: String = trimmed.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    if !identifier(&name) {
        return Err("Unsupported type name.".into());
    }
    let name_end = at + word.len() + (after.len() - trimmed.len()) + name.len();
    if masked[name_end..].trim_start().starts_with('<') {
        return Err("Generic types are refused.".into());
    }
    Ok((name, tokens.contains(&"public"), start, name_end))
}

fn components(doc: &Document, masked: &str, open: usize, close: usize, strict: bool) -> Result<Vec<Field>, String> {
    let mut fields: Vec<Field> = Vec::new();
    for span in split_top(masked, open + 1, close) {
        let clean = crate::model::without_annotations(masked, span.0, span.1);
        let clean = clean.trim();
        if clean.contains("...") {
            return Err("Varargs components are refused.".into());
        }
        let (ty, name) = clean.rsplit_once(char::is_whitespace).ok_or("Unsupported record component.")?;
        let name = name.trim();
        if !identifier(name) || fields.iter().any(|f| f.name == name) {
            return Err("Unsupported or duplicate record component name.".into());
        }
        let annotations = if strict { vetted(doc, masked, span, name)? } else { Vec::new() };
        fields.push(Field { name: name.to_owned(), ty: ty.trim().to_owned(), annotations });
    }
    if fields.is_empty() || fields.len() > MAX_COMPONENTS {
        return Err("Between one and 32 components are supported.".into());
    }
    Ok(fields)
}

fn parse_record(doc: &Document) -> Result<Parsed, String> {
    let masked = mask(&doc.text);
    let at = sole(doc, &masked, Keyword::Record)?;
    let (name, public, start, name_end) = header(&masked, at, "record")?;
    let open = name_end + masked[name_end..].find(|c: char| !c.is_whitespace()).ok_or("Stale source.")?;
    if !masked[open..].starts_with('(') {
        return Err("Unsupported record header.".into());
    }
    let close = crate::model::closing(masked.as_bytes(), open).ok_or("Stale source.")?;
    let fields = components(doc, &masked, open, close, true)?;
    let tail = masked[close + 1..].trim_start();
    if tail.starts_with("implements") {
        return Err("The record implements interfaces; a class would need the same contract and callers checked.".into());
    }
    if !tail.starts_with('{') {
        return Err("Unsupported record header.".into());
    }
    let brace = close + 1 + (masked[close + 1..].len() - tail.len());
    let end = crate::model::closing(masked.as_bytes(), brace).ok_or("Stale source.")?;
    let body = &masked[brace + 1..end];
    if !body.trim().is_empty() {
        let reason = if has_word(body, "static") {
            "static members"
        } else if body.contains(&format!("{name} {{")) || body.contains(&format!("{name}{{")) {
            "a compact constructor"
        } else if body.contains(&format!("{name}(")) {
            "an explicit constructor"
        } else {
            "members or logic"
        };
        return Err(format!("The record body declares {reason}; only empty-bodied records are converted."));
    }
    if !masked[end + 1..].trim().is_empty() || doc.text[brace..end].contains("//") || doc.text[brace..end].contains("/*") {
        return Err("Code or comments after or inside the record body would be lost.".into());
    }
    Ok(Parsed { name, public, span: (start, end + 1), fields })
}

fn getter_forms(field: &Field) -> Vec<String> {
    let (name, ty, cap) = (&field.name, &field.ty, capitalize(&field.name));
    let ty = compact(ty);
    let mut forms = vec![format!("public{ty}get{cap}(){{return{name};}}"), format!("public{ty}get{cap}(){{returnthis.{name};}}")];
    if ty == "boolean" || ty == "Boolean" {
        forms.push(format!("public{ty}is{cap}(){{return{name};}}"));
        forms.push(format!("public{ty}is{cap}(){{returnthis.{name};}}"));
    }
    forms
}

fn parse_class(doc: &Document) -> Result<Parsed, String> {
    let masked = mask(&doc.text);
    let at = sole(doc, &masked, Keyword::Class)?;
    let (name, public, start, name_end) = header(&masked, at, "class")?;
    let open = name_end + masked[name_end..].find('{').ok_or("Unsupported class header.")?;
    let between = masked[name_end..open].trim();
    if between.contains("extends") || between.contains("implements") {
        return Err("The class extends or implements a type; inheritance is refused.".into());
    }
    if !between.is_empty() {
        return Err("Unsupported class header.".into());
    }
    let end = crate::model::closing(masked.as_bytes(), open).ok_or("Stale source.")?;
    if !masked[end + 1..].trim().is_empty() || doc.text[open..end].contains("//") || doc.text[open..end].contains("/*") {
        return Err("Code or comments after or inside the class body would be lost.".into());
    }
    let mut fields = Vec::new();
    let mut methods = Vec::new();
    for (s, e) in crate::model::members(&masked, open + 1, end, false, false) {
        let clean = crate::model::without_annotations(&masked, s, e);
        let clean = clean.trim();
        if clean.is_empty() {
            continue;
        }
        if let Some(declaration) = clean.strip_suffix(';') {
            if declaration.contains('=') {
                return Err("A field has an initializer.".into());
            }
            let Some(rest) = declaration.strip_prefix("private ").filter(|rest| !rest.trim_start().starts_with("static ")) else {
                return Err(format!("`{declaration}`: only private instance fields are converted (public, static or package fields are refused)."));
            };
            let rest = rest.trim_start().strip_prefix("final ").unwrap_or(rest).trim();
            let (ty, field) = rest.rsplit_once(char::is_whitespace).ok_or("Unsupported field.")?;
            let field = field.trim();
            if !identifier(field) || ty.contains("static") || fields.iter().any(|f: &Field| f.name == field) {
                return Err(format!("Unsupported or duplicate field `{field}`."));
            }
            let annotations = vetted(doc, &masked, (s, e), field)?;
            fields.push(Field { name: field.to_owned(), ty: ty.trim().to_owned(), annotations });
        } else {
            if !annotations(&masked[s..e]).is_empty() {
                return Err("A method or constructor is annotated; annotations on methods cannot move to record components.".into());
            }
            methods.push(clean.to_owned());
        }
    }
    if fields.is_empty() || fields.len() > MAX_COMPONENTS {
        return Err("Between one and 32 fields are supported.".into());
    }
    let canonical = format!(
        "{}{name}({}){{{}}}",
        if public { "public" } else { "" },
        fields.iter().map(|f| format!("{}{}", compact(&f.ty), f.name)).collect::<Vec<_>>().join(","),
        fields.iter().map(|f| format!("this.{0}={0};", f.name)).collect::<String>()
    );
    let mut constructors = 0;
    for method in methods {
        let shape = compact(&method);
        let setter = |f: &Field| format!("publicvoidset{}({}{}){{this.{}={};}}", capitalize(&f.name), compact(&f.ty), f.name, f.name, f.name);
        if shape == canonical || shape == canonical.replace("public", "") && !public {
            constructors += 1;
        } else if !fields.iter().any(|f| getter_forms(f).contains(&shape) || setter(f) == shape) {
            let head = method.split('{').next().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ");
            return Err(format!(
                "`{head}` is an extra member; only canonical constructor, plain getters and plain setters are converted (logic, equals/hashCode/toString, other constructors and fluent setters are refused)."
            ));
        }
    }
    if constructors > 1 {
        return Err("More than one canonical constructor.".into());
    }
    Ok(Parsed { name, public, span: (start, end + 1), fields })
}

pub(super) fn getter(field: &Field) -> String {
    let prefix = if field.ty == "boolean" { "is" } else { "get" };
    format!("{prefix}{}", capitalize(&field.name))
}

fn render_class(p: &Parsed, form: &str) -> String {
    let mutable = form != "immutable";
    let mut out = format!("{}class {} {{\n", if p.public { "public " } else { "" }, p.name);
    for f in &p.fields {
        for a in &f.annotations {
            out.push_str(&format!("    {a}\n"));
        }
        out.push_str(&format!("    private {}{} {};\n", if mutable { "" } else { "final " }, f.ty, f.name));
    }
    let params: Vec<_> = p.fields.iter().map(|f| format!("{} {}", f.ty, f.name)).collect();
    if form == "bean" {
        out.push_str(&format!("\n    public {}() {{\n    }}\n", p.name));
    }
    out.push_str(&format!("\n    public {}({}) {{\n", p.name, params.join(", ")));
    for f in &p.fields {
        out.push_str(&format!("        this.{0} = {0};\n", f.name));
    }
    out.push_str("    }\n");
    for f in &p.fields {
        out.push_str(&format!("\n    public {} {}() {{\n        return {};\n    }}\n", f.ty, getter(f), f.name));
        if mutable {
            out.push_str(&format!("\n    public void set{}({} {}) {{\n        this.{} = {};\n    }}\n", capitalize(&f.name), f.ty, f.name, f.name, f.name));
        }
    }
    out.push('}');
    out
}

fn render_record(p: &Parsed) -> String {
    let components: Vec<_> = p
        .fields
        .iter()
        .map(|f| format!("        {}{} {}", f.annotations.iter().map(|a| format!("{a} ")).collect::<String>(), f.ty, f.name))
        .collect();
    format!("{}record {}(\n{}\n) {{\n}}", if p.public { "public " } else { "" }, p.name, components.join(",\n"))
}

fn convert(doc: &Document, to_class: bool, form: &str) -> Result<(TextEdit, String, Vec<Field>), Error> {
    let parsed = if to_class { parse_record(doc) } else { parse_class(doc) }.map_err(Error::from)?;
    let mut text = if to_class { render_class(&parsed, form) } else { render_record(&parsed) };
    if doc.text.contains("\r\n") {
        text = text.replace('\n', "\r\n");
    }
    let edit = TextEdit {
        range: Range::new(position(&doc.text, parsed.span.0), position(&doc.text, parsed.span.1)),
        text,
    };
    let after = patched(&doc.text, std::slice::from_ref(&edit))?;
    if after.len() > MAX_TEXT {
        return Err("Generated source exceeds the supported bound.".into());
    }
    let again = Document { text: after.clone(), ..doc.clone() };
    let reread: Vec<(String, String)> = if to_class {
        declared_fields(&after, &doc.path).map(|(_, fields)| fields).unwrap_or_default()
    } else {
        parse_record(&again).map(|p| p.fields.into_iter().map(|f| (f.name, norm(&f.ty))).collect()).unwrap_or_default()
    };
    let expected: Vec<_> = parsed.fields.iter().map(|f| (f.name.clone(), norm(&f.ty))).collect();
    if reread != expected {
        return Err("Round-trip check failed: the converted type does not carry the same fields; nothing was changed.".into());
    }
    Ok((edit, after, parsed.fields))
}

fn resolve_with(pending: &Pending, doc: &Document, project: &Project, to_class: bool) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let form = if to_class {
        let choices = [
            PickItem::new("immutable", "Immutable: final fields, getters, all-args constructor"),
            PickItem::new("mutable", "Mutable: getters, setters, all-args constructor"),
            PickItem::new("bean", "JavaBean: getters, setters, no-args and all-args constructors"),
        ];
        let key = host::pick("Class form", &choices)?.ok_or(Error::Cancelled)?;
        if !["immutable", "mutable", "bean"].contains(&key.as_str()) {
            return Err("Choose one of the offered class forms.".into());
        }
        key
    } else {
        String::new()
    };
    let unproven_level = !to_class && project_facts::detect(project, doc.path.rsplit_once('/').ok_or(Error::Stale)?.0).java_level.is_none();
    if !to_class {
        if let Some(level) = dto_action::java_level_below_record(&doc.path) {
            return Err(dto_action::record_refusal(level).into());
        }
    }
    let (edit, after, fields) = convert(doc, to_class, &form)?;
    let (title, warning) = if to_class {
        (
            "Convert record to class?",
            "Existing callers are NOT updated: record accessors `x()` become `getX()` (`isX()` for primitive boolean) and the record's generated equals/hashCode/toString are not generated. Other files, mappers, JPQL constructor expressions and Jackson usage are not checked; Jackson deserialisation of the immutable form needs a creator (the JavaBean form has one).",
        )
    } else {
        (
            "Convert class to record?",
            if unproven_level {
                "Existing callers are NOT updated: `getX()`/`isX()` become `x()` and every `setX()` is dropped (a record is immutable); equals/hashCode/toString are generated by the record. Other files, mappers and framework usage are not checked. Requires Java 16 or newer; the project's Java level could not be proven from the build files (version catalogs, BOMs and inherited settings are not resolved)."
            } else {
                "Existing callers are NOT updated: `getX()`/`isX()` become `x()` and every `setX()` is dropped (a record is immutable); equals/hashCode/toString are generated by the record. Other files, mappers and framework usage are not checked. Requires Java 16 or newer; the project's Java level is 16 or newer."
            },
        )
    };
    let names: Vec<_> = fields.iter().map(|f| f.name.as_str()).collect();
    if !ui_kit::prompts::confirm_exact(
        title,
        &format!(
            "Target: {}\nSource version: {}\nComponents: {}\n{warning}\nOne undoable single-document edit; package and imports are kept (unused imports stay).\n\nUpdated source:\n{after}",
            doc.path,
            doc.version,
            names.join(", ")
        ),
    )? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    Ok(FileAction {
        title: if to_class { "Convert record to class".into() } else { "Convert class to record".into() },
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), vec![edit])],
        ..FileAction::default()
    })
}

pub(super) fn resolve_to_class(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    resolve_with(pending, doc, project, true)
}

pub(super) fn resolve_to_record(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    resolve_with(pending, doc, project, false)
}

// ---- reuse of an existing compatible DTO ----

pub(super) struct Candidate {
    pub(super) path: String,
    pub(super) name: String,
    pub(super) badge: &'static str,
}

fn norm(ty: &str) -> String {
    compact(ty.trim_start_matches("final ")).trim_end_matches('?').to_owned()
}

fn declared_fields(text: &str, path: &str) -> Option<(String, Vec<(String, String)>)> {
    if text.len() > MAX_TEXT {
        return None;
    }
    let doc = Document { path: path.into(), text: text.into(), language: if path.ends_with(".kt") { "kotlin" } else { "java" }.into(), version: 0 };
    let masked = mask(text);
    if ["Entity", "Embeddable", "MappedSuperclass"].iter().any(|n| has_word(&masked, n)) {
        return None;
    }
    let pairs = |fields: Vec<Field>| fields.into_iter().map(|f| (f.name, norm(&f.ty))).collect::<Vec<_>>();
    if path.ends_with(".kt") {
        let at = words(&masked, "class").next()?;
        if !masked[..at].trim_end().ends_with("data") {
            return None;
        }
        let rest = &masked[at + 5..];
        let open = at + 5 + rest.find('(')?;
        let name = masked[at + 5..open].trim().to_owned();
        let close = crate::model::closing(masked.as_bytes(), open)?;
        let mut out = Vec::new();
        for span in split_top(&masked, open + 1, close) {
            let clean = crate::model::without_annotations(&masked, span.0, span.1);
            let declaration = clean.split('=').next()?.trim();
            let declaration = declaration.strip_prefix("val ").or_else(|| declaration.strip_prefix("var "))?;
            let (field, ty) = declaration.split_once(':')?;
            out.push((field.trim().to_owned(), norm(ty)));
        }
        return (identifier(&name) && !out.is_empty()).then_some((name, out));
    }
    match declarations(&masked).as_slice() {
        [(at, Keyword::Record)] => {
            let (name, _, _, name_end) = header(&masked, *at, "record").ok()?;
            let open = name_end + masked[name_end..].find('(')?;
            let close = crate::model::closing(masked.as_bytes(), open)?;
            Some((name, pairs(components(&doc, &masked, open, close, false).ok()?)))
        }
        [(at, Keyword::Class)] => {
            let (name, _, _, name_end) = header(&masked, *at, "class").ok()?;
            let open = name_end + masked[name_end..].find('{')?;
            let end = crate::model::closing(masked.as_bytes(), open)?;
            let mut out = Vec::new();
            for (s, e) in crate::model::members(&masked, open + 1, end, false, false) {
                let clean = crate::model::without_annotations(&masked, s, e);
                let Some(declaration) = clean.trim().strip_suffix(';') else {
                    continue;
                };
                let declaration = declaration.split('=').next()?.trim();
                if let Some(rest) = declaration.strip_prefix("private ") {
                    if rest.starts_with("static ") {
                        continue;
                    }
                    let (ty, field) = rest.trim_start_matches("final ").rsplit_once(char::is_whitespace)?;
                    out.push((field.trim().to_owned(), norm(ty)));
                }
            }
            (!out.is_empty()).then_some((name, out))
        }
        [(at, Keyword::Interface)] => {
            let (name, _, _, name_end) = header(&masked, *at, "interface").ok()?;
            let open = name_end + masked[name_end..].find('{')?;
            let end = crate::model::closing(masked.as_bytes(), open)?;
            let mut out = Vec::new();
            for (s, e) in crate::model::members(&masked, open + 1, end, false, false) {
                let clean = crate::model::without_annotations(&masked, s, e);
                let Some(declaration) = clean.trim().strip_suffix(';') else {
                    continue;
                };
                let declaration = declaration.trim();
                let head = declaration.strip_suffix("()")?;
                let (ty, method) = head.rsplit_once(char::is_whitespace)?;
                let property = method.strip_prefix("get").or_else(|| method.strip_prefix("is"))?;
                out.push((decapitalize(property), norm(ty)));
            }
            (!out.is_empty()).then_some((name, out))
        }
        _ => None,
    }
}

pub(super) fn badge(wanted: &[(String, String)], found: &[(String, String)]) -> Option<&'static str> {
    let covered = wanted.iter().all(|(n, t)| found.iter().any(|(fn_, ft)| fn_ == n && *ft == norm(t)));
    if !covered {
        return None;
    }
    Some(if found.len() == wanted.len() { "exact match" } else { "superset" })
}

/// Rank and label of a DTO against the entity scalars it could be mapped from: every DTO field
/// present on the entity is the safest mapper target.
pub(super) fn compatibility(entity: &[(String, String)], dto: &[(String, String)]) -> (u8, &'static str) {
    let shared = dto.iter().filter(|(n, t)| entity.iter().any(|(en, et)| en == n && norm(et) == norm(t))).count();
    match (shared == dto.len(), shared == entity.len()) {
        (true, true) => (0, "exact match"),
        (true, false) => (1, "all DTO fields exist on the entity"),
        (false, true) => (2, "covers every entity field, DTO has extra fields"),
        (false, false) => (3, "partial overlap"),
    }
}

/// Types in the same folder, sibling folders of the parent and `dto`/`dtos` folders whose
/// fields cover every wanted `(name, type)` pair. Bounded to 65 files per glob and two seconds;
/// an incomplete scan only means fewer offers.
pub(super) fn reusable(project: &Project, root: &str, directory: &str, kotlin: bool, own: &str, wanted: &[(String, String)]) -> Result<Vec<Candidate>, Error> {
    let relative = directory.strip_prefix(&format!("{}/", root.trim_end_matches('/'))).ok_or(Error::Stale)?;
    let extension = if kotlin { "kt" } else { "java" };
    let mut globs = vec![format!("{relative}/*.{extension}"), format!("{relative}/dto/*.{extension}"), format!("{relative}/dtos/*.{extension}")];
    if let Some((parent, _)) = relative.rsplit_once('/') {
        globs.push(format!("{parent}/*/*.{extension}"));
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut found: Vec<Candidate> = Vec::new();
    for glob in globs {
        for file in project.find_files(&glob, 65)? {
            if Instant::now() >= deadline {
                return Ok(found);
            }
            if file == own || found.iter().any(|c| c.path == file) || path(&file).as_deref() != Some(file.as_str()) || !inside(root, &file) {
                continue;
            }
            let Ok(text) = project.read_file(&file) else {
                continue;
            };
            let Some((name, fields)) = declared_fields(&text, &file) else {
                continue;
            };
            if let Some(badge) = badge(wanted, &fields) {
                found.push(Candidate { path: file, name, badge });
            }
        }
    }
    found.sort_by(|a, b| (a.badge != "exact match", &a.path).cmp(&(b.badge != "exact match", &b.path)));
    found.truncate(8);
    Ok(found)
}

/// Offers to reuse a compatible existing DTO. Reusing creates nothing, so it ends the creation
/// action with an "already exists" message; declining continues.
pub(super) fn offer_reuse(project: &Project, root: &str, directory: &str, kotlin: bool, own: &str, wanted: &[(String, String)]) -> Result<(), Error> {
    let found = reusable(project, root, directory, kotlin, own, wanted)?;
    if found.is_empty() {
        return Ok(());
    }
    let mut items: Vec<_> = found
        .iter()
        .map(|c| PickItem::new(&c.path, &format!("Use existing {}", c.name)).description(&format!("{} - {}", c.badge, c.path)))
        .collect();
    items.push(PickItem::new("create-new", "Create a new type anyway"));
    let key = host::pick("A compatible type with these fields already exists", &items)?.ok_or(Error::Cancelled)?;
    if key == "create-new" {
        return Ok(());
    }
    if !found.iter().any(|c| c.path == key) {
        return Err("Choose one of the offered options.".into());
    }
    let listing: Vec<_> = found.iter().map(|c| format!("{} ({})", c.path, c.badge)).collect();
    Err(format!("Already exists: {key}. Reuse it instead of creating a duplicate; no file was created. Compatible candidates: {}.", listing.join("; ")).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const RPATH: &str = "/project/src/main/java/shop/CustomerDto.java";
    const RECORD: &str = "package shop;\n\nimport com.fasterxml.jackson.annotation.JsonProperty;\nimport jakarta.validation.constraints.NotNull;\nimport java.util.List;\n\n/** Doc. */\npublic record CustomerDto(\n        @NotNull Long id,\n        @JsonProperty(\"full_name\") String name,\n        boolean active,\n        List<String> tags\n) {\n}\n";
    const CLASS: &str = "package shop;\n\nimport jakarta.validation.constraints.NotNull;\n\npublic class CustomerDto {\n    @NotNull\n    private Long id;\n    private String name;\n    private boolean active;\n\n    public CustomerDto(Long id, String name, boolean active) {\n        this.id = id;\n        this.name = name;\n        this.active = active;\n    }\n\n    public Long getId() {\n        return id;\n    }\n\n    public String getName() {\n        return this.name;\n    }\n\n    public void setName(String name) {\n        this.name = name;\n    }\n\n    public boolean isActive() {\n        return active;\n    }\n}\n";

    fn run(source: &str, title: &str, answers: &[&[&str]], confirm: bool) -> (MockHost, Result<FileAction, Error>) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(RPATH, "java", source, &[]);
        host.add_file(RPATH, source);
        let doc = Document { path: RPATH.into(), text: source.into(), language: "java".into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == title);
        let Some(pending) = pending else {
            return (host, Err(Error::Stale));
        };
        for answer in answers {
            host.answer_next_prompt(Some(answer));
        }
        host.answer_next_prompt(confirm.then_some(&[]));
        let result = plugin.resolve_file_action(&doc, pending);
        assert_eq!(host.editor_text(), source);
        (host, result)
    }

    fn after(source: &str, result: Result<FileAction, Error>) -> String {
        let action = result.unwrap_or_else(|e| panic!("{e:?}"));
        assert!(action.operations.is_empty());
        let [(path, edits)] = action.edits.as_slice() else { panic!("one document") };
        assert_eq!(path, RPATH);
        patched(source, edits).unwrap()
    }

    fn refusal(result: Result<FileAction, Error>) -> String {
        match result {
            Err(Error::Failed(message)) => message,
            Err(other) => panic!("{other:?}"),
            Ok(_) => panic!("expected refusal"),
        }
    }

    #[test]
    fn record_becomes_immutable_class_with_moved_annotations() {
        let (_h, result) = run(RECORD, TO_CLASS_TITLE, &[&["immutable"]], true);
        let text = after(RECORD, result);
        for expected in [
            "import com.fasterxml.jackson.annotation.JsonProperty;",
            "/** Doc. */\npublic class CustomerDto {",
            "    @NotNull\n    private final Long id;",
            "    @JsonProperty(\"full_name\")\n    private final String name;",
            "    private final boolean active;",
            "    public CustomerDto(Long id, String name, boolean active, List<String> tags) {",
            "    public Long getId() {\n        return id;\n    }",
            "    public boolean isActive() {",
            "    public List<String> getTags() {",
        ] {
            assert!(text.contains(expected), "{expected}\n{text}");
        }
        assert!(!text.contains("record") && !text.contains("void set"));
    }

    #[test]
    fn mutable_and_bean_forms_add_setters_and_constructor() {
        let (_h, result) = run(RECORD, TO_CLASS_TITLE, &[&["bean"]], true);
        let text = after(RECORD, result);
        assert!(text.contains("    public CustomerDto() {\n    }") && text.contains("public void setName(String name) {"));
        assert!(text.contains("private String name;") && !text.contains("final"));
        let (_h, result) = run(RECORD, TO_CLASS_TITLE, &[&["mutable"]], true);
        let text = after(RECORD, result);
        assert!(!text.contains("CustomerDto() {") && text.contains("public void setId(Long id)"));
    }

    #[test]
    fn class_becomes_record_dropping_accessors_and_setters() {
        let (_h, result) = run(CLASS, TO_RECORD_TITLE, &[], true);
        let text = after(CLASS, result);
        assert!(text.contains("public record CustomerDto(\n        @NotNull Long id,\n        String name,\n        boolean active\n) {\n}"), "{text}");
        assert!(text.starts_with("package shop;\n\nimport jakarta.validation.constraints.NotNull;"));
        assert!(!text.contains("setName") && !text.contains("class "));
    }

    #[test]
    fn class_without_constructor_and_package_private_forms_convert() {
        let source = "package shop;\nclass Tiny {\n    private int a;\n    int getA() {\n        return a;\n    }\n}\n";
        let (_h, result) = run(source, TO_RECORD_TITLE, &[], true);
        let refused = refusal(result);
        assert!(refused.contains("extra member"), "{refused}");
        let source = "package shop;\nclass Tiny {\n    private final int a;\n    Tiny(int a) {\n        this.a = a;\n    }\n}\n";
        let (_h, result) = run(source, TO_RECORD_TITLE, &[], true);
        assert_eq!(after(source, result), "package shop;\nrecord Tiny(\n        int a\n) {\n}\n");
    }

    #[test]
    fn round_trip_preserves_components() {
        let (_h, result) = run(RECORD, TO_CLASS_TITLE, &[&["immutable"]], true);
        let class_text = after(RECORD, result);
        let (_h, result) = run(&class_text, TO_RECORD_TITLE, &[], true);
        let record_text = after(&class_text, result);
        assert!(record_text.contains("@JsonProperty(\"full_name\") String name") && record_text.contains("List<String> tags"));
    }

    #[test]
    fn record_refusals_name_the_cause() {
        let cases = [
            ("public record A(int a) {\n    A {\n        if (a < 0) throw new IllegalArgumentException();\n    }\n}\n", "compact constructor"),
            ("public record A(int a) {\n    static A of() { return new A(1); }\n}\n", "static members"),
            ("public record A(int a) implements Comparable<A> {\n}\n", "implements"),
            ("public record A(@Deprecated int a) {\n}\n", "@Deprecated cannot be moved"),
            ("@Deprecated\npublic record A(int a) {\n}\n", "annotations"),
            ("public record A(int a) {\n    int twice() { return a * 2; }\n}\n", "members or logic"),
            ("public record A<T>(T a) {\n}\n", "Generic"),
            ("public record A(int a) {\n    // keep\n}\n", "comments"),
        ];
        for (body, needle) in cases {
            let source = format!("package shop;\nimport java.util.*;\n{body}");
            let (_h, result) = run(&source, TO_CLASS_TITLE, &[&["immutable"]], true);
            assert!(refusal(result).contains(needle), "{needle}: {body}");
        }
    }

    #[test]
    fn unimported_or_qualified_annotations_are_refused() {
        let source = "package shop;\npublic record A(@NotNull Long a) {\n}\n";
        let (_h, result) = run(source, TO_CLASS_TITLE, &[&["immutable"]], true);
        assert!(refusal(result).contains("supported: Jackson"));
        let source = "package shop;\npublic record A(@jakarta.validation.constraints.NotNull Long a) {\n}\n";
        let (_h, result) = run(source, TO_CLASS_TITLE, &[&["immutable"]], true);
        assert!(refusal(result).contains("cannot be moved"));
    }

    #[test]
    fn class_refusals_name_the_cause() {
        let cases = [
            ("public class A extends B {\n    private int a;\n}\n", "inheritance"),
            ("public class A implements B {\n    private int a;\n}\n", "inheritance"),
            ("public class A {\n    public int a;\n}\n", "only private instance fields"),
            ("public class A {\n    private int a = 1;\n}\n", "initializer"),
            ("public class A {\n    private int a;\n    public int getA() {\n        return a + 1;\n    }\n}\n", "extra member"),
            ("public class A {\n    private int a;\n    @Override\n    public String toString() {\n        return \"\";\n    }\n}\n", "annotated"),
            ("public class A {\n    private int a;\n    public A() {\n    }\n}\n", "extra member"),
            ("public class A {\n    private int a;\n    public A withA(int a) {\n        this.a = a;\n        return this;\n    }\n}\n", "extra member"),
            ("public class A {\n    @Deprecated\n    private int a;\n}\n", "cannot be moved"),
            ("public class A {\n    private int a; // note\n}\n", "comments"),
            ("public class A {\n    private static int a;\n}\n", "only private instance fields"),
        ];
        for (body, needle) in cases {
            let source = format!("package shop;\n{body}");
            let (_h, result) = run(&source, TO_RECORD_TITLE, &[], true);
            let message = refusal(result);
            assert!(message.contains(needle), "{needle}: {body}: {message}");
        }
    }

    #[test]
    fn entities_kotlin_and_non_java_are_not_offered() {
        let entity = "package shop;\n@Entity\npublic class A {\n    private int a;\n}\n";
        let (_h, result) = run(entity, TO_RECORD_TITLE, &[], true);
        assert!(matches!(result, Err(Error::Stale)));
        let kotlin = "package shop\ndata class A(val a: Int)\n";
        let doc = Document { path: "/project/src/A.kt".into(), text: kotlin.into(), language: "kotlin".into(), version: 1 };
        assert!(!eligible_record(&doc) && !eligible_class(&doc));
    }

    #[test]
    fn cancellations_stale_source_and_crlf_are_safe() {
        let (_h, result) = run(RECORD, TO_CLASS_TITLE, &[&["immutable"]], false);
        assert!(matches!(result, Err(Error::Cancelled)));
        let (_h, result) = run(RECORD, TO_CLASS_TITLE, &[&["nonsense"]], true);
        assert!(result.is_err());
        let crlf = RECORD.replace('\n', "\r\n");
        let (_h, result) = run(&crlf, TO_CLASS_TITLE, &[&["immutable"]], true);
        let text = after(&crlf, result);
        assert!(text.contains("public class CustomerDto {\r\n") && !text.replace("\r\n", "").contains('\n'));
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(RPATH, "java", RECORD, &[]);
        host.add_file(RPATH, RECORD);
        let doc = Document { path: RPATH.into(), text: RECORD.into(), language: "java".into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TO_CLASS_TITLE).unwrap();
        let mut changed = doc.clone();
        changed.version += 1;
        assert!(matches!(plugin.resolve_file_action(&changed, pending.clone()), Err(Error::Stale)));
        host.type_in_editor("// changed");
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Stale)));
        assert_eq!(host.editor_text(), format!("{RECORD}// changed").replace("}
// changed", "}
// changed"));
    }

    const ENTITY: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Customer {\n @Id Long id;\n String title;\n boolean active;\n}\n";
    const EPATH: &str = "/project/src/main/java/shop/Customer.java";

    fn flow(title: &str, files: &[(&str, &str)], answers: &[&[&str]], confirm: bool) -> Result<FileAction, Error> {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(EPATH, "java", ENTITY, &[]);
        host.add_file(EPATH, ENTITY);
        for (path, text) in files {
            host.add_file(path, text);
        }
        let doc = Document { path: EPATH.into(), text: ENTITY.into(), language: "java".into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == title).unwrap();
        for answer in answers {
            host.answer_next_prompt(Some(answer));
        }
        host.answer_next_prompt(confirm.then_some(&[]));
        plugin.resolve_file_action(&doc, pending)
    }

    const SUMMARY: (&str, &str) = ("/project/src/main/java/shop/CustomerSummary.java", "package shop;\npublic record CustomerSummary(Long id, String title, boolean active, String extra) {\n}\n");
    const EXACT_CLASS: (&str, &str) = ("/project/src/main/java/shop/dto/CustomerView.java", "package shop.dto;\npublic class CustomerView {\n    private Long id;\n    private String title;\n    public Long getId() {\n        return id;\n    }\n}\n");

    #[test]
    fn scalar_dto_creation_refuses_with_already_exists_when_a_compatible_type_is_reused() {
        let result = flow("Create scalar DTO with preview…", &[SUMMARY, EXACT_CLASS], &[&["CustomerDto"], &["id", "title"], &[SUMMARY.0]], true);
        let Err(Error::Failed(message)) = result else { panic!("expected refusal") };
        assert!(message.starts_with(&format!("Already exists: {}", SUMMARY.0)), "{message}");
        assert!(message.contains("superset") && message.contains(EXACT_CLASS.0) && message.contains("exact match"), "{message}");
    }

    #[test]
    fn declining_reuse_creates_the_new_type_and_incompatible_types_are_not_offered() {
        let result = flow("Create scalar DTO with preview…", &[SUMMARY], &[&["CustomerDto"], &["id", "title"], &["create-new"]], true).unwrap();
        assert!(matches!(result.operations.as_slice(), [FileOperation::Create { .. }]));
        let too_small = ("/project/src/main/java/shop/Small.java", "package shop;\npublic record Small(Long id) {\n}\n");
        let result = flow("Create scalar DTO with preview…", &[too_small], &[&["CustomerDto"], &["id", "title"]], true).unwrap();
        assert!(matches!(result.operations.as_slice(), [FileOperation::Create { .. }]));
        let entity_like = ("/project/src/main/java/shop/Other.java", "package shop;\n@Entity\npublic class Other {\n    private Long id;\n    private String title;\n}\n");
        assert!(flow("Create scalar DTO with preview…", &[entity_like], &[&["CustomerDto"], &["id", "title"]], true).is_ok());
    }

    #[test]
    fn forged_reuse_choice_is_refused_and_projection_and_class_flows_offer_reuse() {
        let result = flow("Create scalar DTO with preview…", &[SUMMARY], &[&["CustomerDto"], &["id", "title"], &["/etc/passwd"]], true);
        assert!(result.is_err());
        let result = flow("Create projection with preview…", &[SUMMARY], &[&["record"], &["CustomerView2"], &["id", "title"], &[SUMMARY.0]], true);
        let Err(Error::Failed(message)) = result else { panic!("expected refusal") };
        assert!(message.starts_with("Already exists:"), "{message}");
        let result = flow("Create DTO class with preview…", &[EXACT_CLASS], &[&["pojo"], &["CustomerDto"], &["id", "title"], &[EXACT_CLASS.0]], true);
        let Err(Error::Failed(message)) = result else { panic!("expected refusal {result:?}") };
        assert!(message.contains("exact match"), "{message}");
    }

    #[test]
    fn flattened_dto_offers_reuse_too() {
        let order = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Order {\n @Id Long id;\n String note;\n @ManyToOne Customer customer;\n}\n";
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let path = "/project/src/main/java/shop/Order.java";
        host.open_editor(path, "java", order, &[]);
        host.add_file(path, order);
        host.add_file(EPATH, ENTITY);
        host.add_file("/project/src/main/java/shop/Flat.java", "package shop;\npublic record Flat(String note, String customerTitle) {\n}\n");
        let doc = Document { path: path.into(), text: order.into(), language: "java".into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == "Create flattened DTO…").unwrap();
        for answer in [&["OrderFlat"][..], &["note"], &["customer"], &["title"], &["/project/src/main/java/shop/Flat.java"]] {
            host.answer_next_prompt(Some(answer));
        }
        let Err(Error::Failed(message)) = plugin.resolve_file_action(&doc, pending) else { panic!("expected refusal") };
        assert!(message.contains("Flat.java") && message.contains("exact match"), "{message}");
    }

    #[test]
    fn badge_ranks_exact_and_superset_and_rejects_partial() {
        let wanted = vec![("id".to_owned(), "Long".to_owned()), ("name".to_owned(), "String".to_owned())];
        let exact = vec![("name".to_owned(), "String".to_owned()), ("id".to_owned(), "Long".to_owned())];
        let more = [exact.clone(), vec![("x".to_owned(), "int".to_owned())]].concat();
        assert_eq!(badge(&wanted, &exact), Some("exact match"));
        assert_eq!(badge(&wanted, &more), Some("superset"));
        assert_eq!(badge(&wanted, &exact[..1]), None);
        assert_eq!(badge(&wanted, &[("id".into(), "Integer".into()), ("name".into(), "String".into())]), None);
    }

    #[test]
    fn mapper_compatibility_ranks_exact_before_partial() {
        let entity = vec![("id".to_owned(), "Long".to_owned()), ("name".to_owned(), "String".to_owned())];
        assert_eq!(compatibility(&entity, &entity).0, 0);
        assert_eq!(compatibility(&entity, &entity[..1]).0, 1);
        assert_eq!(compatibility(&entity, &[entity.clone(), vec![("x".to_owned(), "int".to_owned())]].concat()).0, 2);
        assert_eq!(compatibility(&entity, &[entity[0].clone(), ("x".to_owned(), "int".to_owned())]).0, 3);
    }

    #[test]
    fn declared_fields_reads_records_classes_interfaces_and_data_classes() {
        assert_eq!(declared_fields(RECORD, RPATH).unwrap().1[1], ("name".to_owned(), "String".to_owned()));
        assert_eq!(declared_fields(CLASS, RPATH).unwrap().1.len(), 3);
        let interface = "package s;\ninterface P {\n    Long getId();\n    boolean isOn();\n}\n";
        assert_eq!(declared_fields(interface, "/p/P.java").unwrap().1, vec![("id".to_owned(), "Long".to_owned()), ("on".to_owned(), "boolean".to_owned())]);
        let kotlin = "package s\ndata class K(val id: Long?, var name: String = \"\")\n";
        assert_eq!(declared_fields(kotlin, "/p/K.kt").unwrap().1, vec![("id".to_owned(), "Long".to_owned()), ("name".to_owned(), "String".to_owned())]);
        assert!(declared_fields("package s;\n@Entity\npublic class E {\n    private Long id;\n}\n", "/p/E.java").is_none());
    }

    #[test]
    #[ignore = "requires javac and java (source scripts/jpa-compile-env.sh); validation/Jackson jars from JPA_JAKARTA_VALIDATION_API and JACKSON_ANNOTATIONS_JAR or ~/.m2"]
    fn generated_conversions_compile_both_directions_and_getters_return_constructor_values() {
        let validation = std::env::var("JPA_JAKARTA_VALIDATION_API").expect("jakarta validation api jar");
        let jackson = std::env::var("JACKSON_ANNOTATIONS_JAR")
            .unwrap_or_else(|_| format!("{}/.m2/repository/com/fasterxml/jackson/core/jackson-annotations/2.21/jackson-annotations-2.21.jar", std::env::var("HOME").unwrap()));
        let classpath = format!("{validation}:{jackson}");
        let mut cases = Vec::new();
        for form in ["immutable", "mutable", "bean"] {
            let (_h, result) = run(RECORD, TO_CLASS_TITLE, &[&[form]], true);
            cases.push((form, after(RECORD, result)));
        }
        let (_h, result) = run(CLASS, TO_RECORD_TITLE, &[], true);
        cases.push(("record", after(CLASS, result)));
        for (form, text) in cases {
            let dir = std::env::temp_dir().join(format!("jpa-convert-{}-{form}", std::process::id()));
            std::fs::create_dir_all(dir.join("shop")).unwrap();
            let file = dir.join("shop/CustomerDto.java");
            std::fs::write(&file, &text).unwrap();
            let main = dir.join("Main.java");
            let (args, check) = if form == "record" {
                ("1L, \"n\", true", "c.id() == 1L && c.name().equals(\"n\") && c.active()")
            } else {
                ("1L, \"n\", true, java.util.List.of(\"t\")", "c.getId() == 1L && c.getName().equals(\"n\") && c.isActive() && c.getTags().size() == 1")
            };
            std::fs::write(
                &main,
                format!("import shop.CustomerDto;\npublic class Main {{\n    public static void main(String[] a) {{\n        CustomerDto c = new CustomerDto({args});\n        if (!({check})) {{ throw new AssertionError(); }}\n        System.out.println(\"ok\");\n    }}\n}}\n"),
            )
            .unwrap();
            let compile = std::process::Command::new("javac")
                .args(["--release", "17", "-Xlint:all", "-classpath", &classpath, "-d"])
                .arg(&dir)
                .arg(&file)
                .arg(&main)
                .output()
                .expect("javac installed");
            assert!(compile.status.success(), "{form}\n{}\n{text}", String::from_utf8_lossy(&compile.stderr));
            let run = std::process::Command::new("java").args(["-classpath", &format!("{}:{classpath}", dir.display()), "Main"]).output().expect("java installed");
            assert!(run.status.success() && String::from_utf8_lossy(&run.stdout).trim() == "ok", "{form}: {}", String::from_utf8_lossy(&run.stderr));
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
