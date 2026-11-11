//! Reviewed Bean Validation constraint insertion for one plain scalar source attribute.
//! Only the invoking document is edited. Constraint/type pairs follow the Jakarta Bean
//! Validation 3.0 specification (section 8); javax.validation 2.0 has the same set.
use super::*;
use crate::model::{Annotation, Attribute, arg};

pub(super) const TITLE: &str = "Add validation constraint with preview…";

#[derive(Clone, Copy, PartialEq, Debug)]
enum Cat {
    Text,
    Whole,
    Fraction,
    Temporal,
    Plain,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Constraint {
    NotNull,
    Null,
    NotBlank,
    NotEmpty,
    Size,
    Min,
    Max,
    Positive,
    Past,
    Future,
    Pattern,
    Email,
}

const ALL: [Constraint; 12] = [
    Constraint::NotNull,
    Constraint::Null,
    Constraint::NotBlank,
    Constraint::NotEmpty,
    Constraint::Size,
    Constraint::Min,
    Constraint::Max,
    Constraint::Positive,
    Constraint::Past,
    Constraint::Future,
    Constraint::Pattern,
    Constraint::Email,
];

impl Constraint {
    fn name(self) -> &'static str {
        match self {
            Self::NotNull => "NotNull",
            Self::Null => "Null",
            Self::NotBlank => "NotBlank",
            Self::NotEmpty => "NotEmpty",
            Self::Size => "Size",
            Self::Min => "Min",
            Self::Max => "Max",
            Self::Positive => "Positive",
            Self::Past => "Past",
            Self::Future => "Future",
            Self::Pattern => "Pattern",
            Self::Email => "Email",
        }
    }
    fn from_name(name: &str) -> Option<Self> {
        ALL.into_iter().find(|c| c.name() == name)
    }
    fn valid_for(self, cat: Cat, primitive: bool) -> bool {
        match self {
            Self::NotNull | Self::Null => !primitive,
            Self::NotBlank | Self::NotEmpty | Self::Size | Self::Pattern | Self::Email => cat == Cat::Text,
            Self::Min | Self::Max => cat == Cat::Whole,
            Self::Positive => matches!(cat, Cat::Whole | Cat::Fraction),
            Self::Past | Self::Future => cat == Cat::Temporal,
        }
    }
}

const JAVA_LANG: [&str; 9] = ["String", "Long", "Integer", "Short", "Byte", "Boolean", "Double", "Float", "Character"];
const KOTLIN_NATIVE: [(&str, &str); 9] = [
    ("String", "java.lang.String"),
    ("Long", "java.lang.Long"),
    ("Int", "java.lang.Integer"),
    ("Short", "java.lang.Short"),
    ("Byte", "java.lang.Byte"),
    ("Boolean", "java.lang.Boolean"),
    ("Double", "java.lang.Double"),
    ("Float", "java.lang.Float"),
    ("Char", "java.lang.Character"),
];
const JDK: [&str; 11] = [
    "java.math.BigDecimal",
    "java.math.BigInteger",
    "java.time.Instant",
    "java.time.LocalDate",
    "java.time.LocalDateTime",
    "java.time.LocalTime",
    "java.time.OffsetDateTime",
    "java.time.ZonedDateTime",
    "java.time.Duration",
    "java.util.UUID",
    "java.util.Date",
];

fn category(canonical: &str) -> Option<Cat> {
    Some(match canonical {
        "java.lang.String" => Cat::Text,
        "java.lang.Long" | "java.lang.Integer" | "java.lang.Short" | "java.lang.Byte" | "java.math.BigDecimal"
        | "java.math.BigInteger" => Cat::Whole,
        "java.lang.Double" | "java.lang.Float" => Cat::Fraction,
        "java.time.Instant" | "java.time.LocalDate" | "java.time.LocalDateTime" | "java.time.LocalTime"
        | "java.time.OffsetDateTime" | "java.time.ZonedDateTime" | "java.util.Date" => Cat::Temporal,
        "java.lang.Boolean" | "java.lang.Character" | "java.util.UUID" | "java.time.Duration" => Cat::Plain,
        _ => return None,
    })
}

/// A simple name is trusted only when no import redirects it; same-package shadowing stays unresolved.
fn resolve_type(class: &Class, ty: &str) -> Option<(Cat, bool)> {
    let primitive = match ty {
        "byte" | "short" | "int" | "long" if !class.kotlin => Some(Cat::Whole),
        "float" | "double" if !class.kotlin => Some(Cat::Fraction),
        "boolean" | "char" if !class.kotlin => Some(Cat::Plain),
        _ => None,
    };
    if let Some(cat) = primitive {
        return Some((cat, true));
    }
    let canonical = if ty.contains('.') {
        ty.to_owned()
    } else if class.kotlin && KOTLIN_NATIVE.iter().any(|(k, _)| *k == ty) {
        KOTLIN_NATIVE.iter().find(|(k, _)| *k == ty)?.1.to_owned()
    } else if !class.kotlin && JAVA_LANG.contains(&ty) {
        format!("java.lang.{ty}")
    } else {
        let explicit: Vec<_> = class.imports.iter().filter(|i| i.rsplit('.').next() == Some(ty)).collect();
        let [only] = explicit.as_slice() else {
            return None;
        };
        (*only).clone()
    };
    if !ty.contains('.') {
        let explicit: Vec<_> = class.imports.iter().filter(|i| i.rsplit('.').next() == Some(ty)).collect();
        if explicit.iter().any(|i| **i != canonical) {
            return None;
        }
    }
    if !JDK.contains(&canonical.as_str()) && !canonical.starts_with("java.lang.") {
        return None;
    }
    category(&canonical).map(|cat| (cat, false))
}

pub(super) fn source(doc: &Document) -> Option<(Class, &'static str)> {
    let class = dto_action::source(doc)?;
    let masked = mask(&doc.text);
    // attribute_info::snapshot rejects any non-persistence annotation, including validation ones.
    if doc.text.len() > MAX_TEXT
        || ["interface", "enum", "object", "typealias"].iter().any(|w| has_word(&masked, w))
        || !crate::live::trusted_annotations(&doc.text)
        || !crate::attribute_info::balanced(&masked)
        || class.attributes.len() > 128
        || annotations(&masked).len() > 256
        || annotations(&masked).iter().any(|a| a.name == "Access")
        || crate::checks::owned_class_start(&doc.text, &masked, &class).is_none()
    {
        return None;
    }
    let at = offset(&doc.text, class.pos)?;
    let open = at + masked[at..].find('{')?;
    if masked[at..open].trim() != class.name {
        return None;
    }
    if ["java", "kotlin", "jakarta", "javax"]
        .iter()
        .any(|p| class.name == *p || class.imports.iter().any(|i| i.rsplit('.').next() == Some(p)))
    {
        return None;
    }
    let namespace = match super::namespace(&doc.text, &class)? {
        "jakarta.persistence" => "jakarta.validation",
        "javax.persistence" => "javax.validation",
        _ => return None,
    };
    let body = &masked[open..];
    let annotations = annotations(body);
    for a in &annotations {
        let written = &body[a.start + 1..a.name_end];
        let persistence = matches!(
            a.name,
            "Id" | "EmbeddedId" | "Column" | "Basic" | "Version" | "Temporal" | "Enumerated" | "OneToOne" | "OneToMany"
                | "ManyToOne" | "ManyToMany" | "ElementCollection" | "Embedded" | "Lob" | "Transient" | "JoinColumn"
                | "JoinTable"
        );
        if !persistence {
            continue;
        }
        if written.starts_with("get:") || written.starts_with("set:") {
            return None;
        }
        let mut rest = a.end();
        loop {
            let tail = body[rest..].trim_start();
            rest = body.len() - tail.len();
            let Some(next) = annotations.iter().find(|n| n.start == rest) else {
                break;
            };
            rest = next.end();
        }
        let member = body[rest..].find([';', '{', '=', '(']).map(|i| body.as_bytes()[rest + i]);
        if member == Some(b'(') {
            return None;
        }
    }
    Some((class, namespace))
}

fn candidates(class: &Class) -> Vec<(&Attribute, Cat, bool)> {
    class
        .attributes
        .iter()
        .filter(|a| a.relation == Relation::None && !a.ty.contains(['<', '[', '?']))
        .filter_map(|a| {
            let (cat, primitive) = resolve_type(class, &a.ty)?;
            ALL.iter().any(|c| c.valid_for(cat, primitive)).then_some((a, cat, primitive))
        })
        .collect()
}

pub(super) fn eligible(doc: &Document) -> bool {
    source(doc).is_some_and(|(class, _)| !candidates(&class).is_empty())
}

pub(super) struct Site {
    line_start: usize,
    indent: String,
    pub(super) existing: Vec<(String, Option<String>)>,
}

fn open_balance(text: &str) -> i32 {
    text.bytes().fold(0, |n, b| n + i32::from(b == b'(') - i32::from(b == b')'))
}

pub(super) fn site(doc: &Document, class: &Class, attribute: &Attribute) -> Result<Site, Error> {
    let masked = mask(&doc.text);
    let at = offset(&doc.text, attribute.pos).ok_or(Error::Stale)?;
    if doc.text.get(at..at + attribute.name.len()) != Some(attribute.name.as_str()) {
        return Err(Error::Stale);
    }
    let layout = "Unsupported declaration layout; edit this attribute manually.";
    let line_start = masked[..at].rfind('\n').map_or(0, |i| i + 1);
    let line_end = at + masked[at..].find('\n').unwrap_or(masked.len() - at);
    let line = &masked[line_start..line_end];
    let prefix = &line[..at - line_start];
    let suffix = &line[at - line_start + attribute.name.len()..];
    if class.kotlin {
        if line.contains(';')
            || !suffix.trim_start().starts_with(':')
            || !(has_word(prefix, "var") || has_word(prefix, "val"))
            || has_word(suffix, "by")
            || suffix.contains("get(")
            || suffix.contains("set(")
        {
            return Err(layout.into());
        }
        let next = masked[line_end..].lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
        if next.starts_with("get") || next.starts_with("set") || next.contains(" set") {
            return Err("Custom Kotlin accessors may have no backing field; edit manually.".into());
        }
    } else {
        let tail = suffix.trim_start();
        if line.matches(';').count() != 1
            || !(tail.starts_with(';') || tail.starts_with('='))
            || !prefix.trim_end().ends_with(attribute.ty.as_str())
        {
            return Err(layout.into());
        }
    }
    if has_word(prefix, "static") || has_word(prefix, "transient") {
        return Err("Static and transient members are not persistent attributes.".into());
    }
    let mut region_start = line_start;
    let mut cursor = line_start;
    let mut above = "";
    while cursor > 0 {
        let end = cursor - 1;
        let start = masked[..end].rfind('\n').map_or(0, |i| i + 1);
        let row = masked[start..end].trim();
        cursor = start;
        if row.is_empty() {
            continue;
        }
        if row.starts_with('@') {
            region_start = start;
            continue;
        }
        above = row;
        break;
    }
    let region = &masked[region_start..at];
    if open_balance(region) != 0 || open_balance(above) < 0 || region.bytes().filter(|b| *b == b')').count() > region.bytes().filter(|b| *b == b'(').count() {
        return Err("Multi-line annotations are not supported; edit this attribute manually.".into());
    }
    let last = above.rsplit(|c: char| !c.is_ascii_alphanumeric()).next().unwrap_or("");
    if above.ends_with([',', '(', '=', '.', ':', '<', '&', '|', '+', '-', '@'])
        || [
            "private", "protected", "public", "final", "static", "transient", "volatile", "var", "val", "lateinit", "open",
            "override", "internal",
        ]
        .contains(&last)
    {
        return Err(layout.into());
    }
    let anns: Vec<Annotation> = annotations(&masked).into_iter().filter(|a| a.start >= region_start && a.start < at).collect();
    let existing = anns
        .iter()
        .map(|a| (a.name.to_owned(), a.args.map(|(s, e)| doc.text[s..e].to_owned())))
        .collect();
    let indent: String = doc.text[line_start..].chars().take_while(|c| *c == ' ' || *c == '\t').collect();
    Ok(Site { line_start, indent, existing })
}

pub(super) fn number(text: &str) -> Option<i64> {
    let t = text.trim().trim_end_matches(['L', 'l']);
    t.parse().ok()
}

fn existing_value(site: &Site, name: &str, key: &str, default: Option<i64>) -> Result<Option<i64>, Error> {
    let Some((_, args)) = site.existing.iter().find(|(n, _)| n == name) else {
        return Ok(None);
    };
    let args = args.as_deref().unwrap_or("");
    let found = if key == "value" && number(args).is_some() {
        number(args)
    } else {
        let masked = mask(args);
        let a = Annotation { name: "", start: 0, name_end: 0, args: Some((0, args.len())) };
        arg(args, &masked, &a, key).and_then(number)
    };
    match found.or(default) {
        Some(v) => Ok(Some(v)),
        None => Err(format!("The existing @{name} has a non-literal {key}; edit manually.").into()),
    }
}

fn literal(value: &str, kotlin: bool) -> Result<String, Error> {
    if value.chars().any(|c| c.is_control()) {
        return Err("Control characters are not allowed in string literals.".into());
    }
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '$' if kotlin => out.push_str("\\$"),
            _ => out.push(c),
        }
    }
    out.push('"');
    Ok(out)
}

fn long_literal(value: i64) -> String {
    if i32::try_from(value).is_ok() { value.to_string() } else { format!("{value}L") }
}

#[derive(Clone, Debug, PartialEq)]
enum Args {
    None,
    Size { min: i32, max: Option<i32> },
    Bound(i64),
    Pattern(String),
}

fn render(constraint: Constraint, args: &Args, message: Option<&str>, kotlin: bool) -> Result<String, Error> {
    let mut parts = Vec::new();
    match args {
        Args::None => {}
        Args::Size { min, max } => {
            if *min != 0 {
                parts.push(format!("min = {min}"));
            }
            if let Some(max) = max {
                parts.push(format!("max = {max}"));
            }
        }
        Args::Bound(v) => parts.push(format!("value = {}", long_literal(*v))),
        Args::Pattern(regexp) => parts.push(format!("regexp = {}", literal(regexp, kotlin)?)),
    }
    if let Some(message) = message {
        parts.push(format!("message = {}", literal(message, kotlin)?));
    }
    let prefix = if kotlin { "@field:" } else { "@" };
    let name = constraint.name();
    Ok(match (parts.as_slice(), args) {
        ([], _) => format!("{prefix}{name}"),
        ([_], Args::Bound(v)) => format!("{prefix}{name}({})", long_literal(*v)),
        _ => format!("{prefix}{name}({})", parts.join(", ")),
    })
}

fn conflicts(constraint: Constraint, args: &Args, site: &Site) -> Result<(), Error> {
    let has = |c: Constraint| site.existing.iter().any(|(n, _)| n == c.name());
    if has(constraint) {
        return Err(format!("@{} is already declared on this attribute.", constraint.name()).into());
    }
    let clash = |a: Constraint| -> Error { format!("@{} conflicts with the existing @{}.", constraint.name(), a.name()).into() };
    for (new, old) in [
        (Constraint::NotNull, Constraint::Null),
        (Constraint::Null, Constraint::NotNull),
        (Constraint::Null, Constraint::NotBlank),
        (Constraint::Null, Constraint::NotEmpty),
        (Constraint::NotBlank, Constraint::Null),
        (Constraint::NotEmpty, Constraint::Null),
        (Constraint::Past, Constraint::Future),
        (Constraint::Future, Constraint::Past),
    ] {
        if constraint == new && has(old) {
            return Err(clash(old));
        }
    }
    let max_of = |args: &Args| match args {
        Args::Bound(v) => Some(*v),
        _ => None,
    };
    match constraint {
        Constraint::Min => {
            if let (Args::Bound(min), Some(max)) = (args, existing_value(site, "Max", "value", None)?) {
                if *min > max {
                    return Err("Min is greater than the existing Max.".into());
                }
            }
        }
        Constraint::Max => {
            if let (Some(max), Some(min)) = (max_of(args), existing_value(site, "Min", "value", None)?) {
                if min > max {
                    return Err("Max is smaller than the existing Min.".into());
                }
            }
            if let (Some(max), true) = (max_of(args), has(Constraint::Positive)) {
                if max <= 0 {
                    return Err("Max must be positive when @Positive is present.".into());
                }
            }
        }
        Constraint::Positive => {
            if existing_value(site, "Max", "value", None)?.is_some_and(|max| max <= 0) {
                return Err("@Positive conflicts with the existing non-positive Max.".into());
            }
        }
        Constraint::NotEmpty | Constraint::NotBlank => {
            if existing_value(site, "Size", "max", Some(i64::from(i32::MAX)))?.is_some_and(|max| max < 1) {
                return Err("The existing @Size allows no characters.".into());
            }
        }
        _ => {}
    }
    Ok(())
}

fn imports_edit(doc: &Document, class: &Class, namespace: &str, name: &str, eol: &str) -> Result<Option<TextEdit>, Error> {
    let full = format!("{namespace}.constraints.{name}");
    let other = if namespace == "jakarta.validation" { "javax.validation" } else { "jakarta.validation" };
    if class.name == name || class.attributes.iter().any(|a| a.ty.rsplit('.').next() == Some(name)) {
        return Err(format!("The name {name} is already used in this file; edit manually.").into());
    }
    if class.imports.iter().any(|i| i.starts_with(&format!("{other}."))) {
        return Err("The file already mixes in the other validation namespace.".into());
    }
    let explicit: Vec<_> = class.imports.iter().filter(|i| i.rsplit('.').next() == Some(name)).collect();
    if !explicit.is_empty() {
        return if explicit.iter().all(|i| **i == full) {
            Ok(None)
        } else {
            Err(format!("An import named {name} already exists for another type.").into())
        };
    }
    if class.imports.iter().any(|i| *i == format!("{namespace}.constraints.*")) {
        return Ok(None);
    }
    let masked = mask(&doc.text);
    let limit = offset(&doc.text, class.pos).ok_or(Error::Stale)?;
    let (mut last_import, mut package) = (None, None);
    let mut at = 0;
    for line in masked[..limit].split_inclusive('\n') {
        let end = at + line.len();
        let trimmed = line.trim_start();
        if trimmed.starts_with("import ") {
            last_import = Some(end);
        } else if trimmed.starts_with("package ") {
            package = Some(end);
        }
        at = end;
    }
    let statement = if class.kotlin { format!("import {full}") } else { format!("import {full};") };
    let (target, text) = match (last_import, package) {
        (Some(end), _) => (end, format!("{statement}{eol}")),
        (None, Some(end)) => (end, format!("{eol}{statement}{eol}")),
        (None, None) => (0, format!("{statement}{eol}{eol}")),
    };
    if target > 0 && !doc.text[..target].ends_with('\n') {
        return Err(Error::Stale);
    }
    Ok(Some(TextEdit { range: Range::new(position(&doc.text, target), position(&doc.text, target)), text }))
}

fn input(label: &str, default: &str) -> Result<String, Error> {
    host::input(label, Some(default))?.map(|s| s.trim().to_owned()).ok_or(Error::Cancelled)
}

fn prompt_args(constraint: Constraint, kotlin: bool) -> Result<(Args, Option<String>), Error> {
    let args = match constraint {
        Constraint::Size => {
            let min: i32 = input("Minimum length", "0")?.parse().map_err(|_| Error::from("Minimum length must be an integer."))?;
            let max_text = input("Maximum length (empty for no maximum)", "")?;
            let max = if max_text.is_empty() {
                None
            } else {
                Some(max_text.parse::<i32>().map_err(|_| Error::from("Maximum length must be an integer."))?)
            };
            if min < 0 || max.is_some_and(|m| m < 0) {
                return Err("Size bounds must not be negative.".into());
            }
            if max.is_some_and(|m| min > m) {
                return Err("Size minimum is greater than the maximum.".into());
            }
            if min == 0 && max.is_none() {
                return Err("Size needs a minimum above 0 or a maximum.".into());
            }
            Args::Size { min, max }
        }
        Constraint::Min | Constraint::Max => {
            let label = if constraint == Constraint::Min { "Minimum value" } else { "Maximum value" };
            let value: i64 = input(label, "0")?.parse().map_err(|_| Error::from("Enter a 64-bit integer."))?;
            if value == i64::MIN {
                return Err("Choose a value above -9223372036854775808.".into());
            }
            Args::Bound(value)
        }
        Constraint::Pattern => {
            let regexp = host::input("Regular expression (Java syntax, typed unescaped)", Some(""))?
                .ok_or(Error::Cancelled)?;
            if regexp.is_empty() || regexp.chars().count() > 256 {
                return Err("The regular expression must be 1 to 256 characters.".into());
            }
            literal(&regexp, kotlin)?;
            Args::Pattern(regexp)
        }
        _ => Args::None,
    };
    let message = host::input("Message (empty keeps the default)", Some(""))?.ok_or(Error::Cancelled)?;
    if message.chars().count() > 256 {
        return Err("The message is limited to 256 characters.".into());
    }
    Ok((args, (!message.is_empty()).then_some(message)))
}

pub(super) fn resolve(pending: &Pending, doc: &Document) -> Result<FileAction, Error> {
    super::current(pending, doc)?;
    let (class, namespace) = source(doc).ok_or(Error::Stale)?;
    let options = candidates(&class);
    if options.is_empty() {
        return Err("No attribute has a proven scalar type that supports a constraint.".into());
    }
    let items: Vec<_> = options.iter().map(|(a, _, _)| PickItem::new(&a.name, &format!("{} : {}", a.name, a.ty))).collect();
    let name = host::pick("Attribute to validate", &items)?.ok_or(Error::Cancelled)?;
    let (attribute, cat, primitive) = options.into_iter().find(|(a, _, _)| a.name == name).ok_or(Error::Stale)?;
    let site = site(doc, &class, attribute)?;
    let valid: Vec<_> = ALL.into_iter().filter(|c| c.valid_for(cat, primitive)).collect();
    let items: Vec<_> = valid.iter().map(|c| PickItem::new(c.name(), &format!("@{}", c.name()))).collect();
    let picked = host::pick("Constraint", &items)?.ok_or(Error::Cancelled)?;
    let constraint = Constraint::from_name(&picked).filter(|c| valid.contains(c)).ok_or(Error::Stale)?;
    let (args, message) = prompt_args(constraint, class.kotlin)?;
    conflicts(constraint, &args, &site)?;
    super::current(pending, doc)?;
    let eol = if doc.text.contains("\r\n") { "\r\n" } else { "\n" };
    let line = render(constraint, &args, message.as_deref(), class.kotlin)?;
    let mut edits = vec![TextEdit {
        range: Range::new(position(&doc.text, site.line_start), position(&doc.text, site.line_start)),
        text: format!("{}{line}{eol}", site.indent),
    }];
    if let Some(import) = imports_edit(doc, &class, namespace, constraint.name(), eol)? {
        edits.push(import);
    }
    let after = super::checked_after(doc, &edits)?;
    if !ui_kit::prompts::confirm_exact(
        "Add validation constraint?",
        &format!(
            "Target: {}\nSource version: {}\n\n{after}\nCurrent source only; type names are trusted by import, same-package shadowing, effective access and XML overrides are unresolved.",
            doc.path, doc.version
        ),
    )? {
        return Err(Error::Cancelled);
    }
    super::current(pending, doc)?;
    Ok(FileAction {
        title: format!("Add @{} to {}", constraint.name(), attribute.name),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), edits)],
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const JAVA: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\nclass Invoice {\n    @Id\n    private Long id;\n    // note\n    private String title;\n    private java.time.LocalDate due;\n    private double ratio;\n    private boolean paid;\n    private java.math.BigDecimal amount;\n}\n";
    const KOTLIN: &str = "package shop\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\n@Entity\nclass Invoice {\n    @Id\n    var id: Long? = null\n    var title: String? = null\n    var count: Int = 0\n}\n";

    fn setup(text: &str, language: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let path = format!("/project/src/main/{language}/shop/Invoice.{}", if language == "java" { "java" } else { "kt" });
        host.open_editor(&path, language, text, &[]);
        (host, JpaAssist::default(), Document { path, language: language.into(), text: text.into(), version: 1 })
    }
    fn run(text: &str, language: &str, rows: &[&[&str]]) -> Result<String, Error> {
        let (host, mut plugin, doc) = setup(text, language);
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).ok_or(Error::NotFound("not offered".into()))?;
        for row in rows {
            host.answer_next_prompt(Some(row));
        }
        let result = plugin.resolve_file_action(&doc, pending)?;
        assert_eq!(host.editor_text(), text);
        assert!(result.operations.is_empty() && result.edits.len() == 1 && result.edits[0].0 == doc.path);
        patched(&doc.text, &result.edits[0].1)
    }
    fn failed(result: Result<String, Error>, needle: &str) {
        match result {
            Err(Error::Failed(message)) => assert!(message.contains(needle), "{needle}: {message}"),
            other => panic!("expected refusal containing {needle:?}, got {other:?}"),
        }
    }
    fn not_offered(result: Result<String, Error>) {
        assert!(matches!(result, Err(Error::NotFound(_))), "{result:?}");
    }
    fn simple(text: &str, language: &str, attribute: &str, constraint: &str) -> Result<String, Error> {
        run(text, language, &[&[attribute], &[constraint], &[""], &[]])
    }

    #[test]
    fn java_size_is_inserted_above_the_field_after_comments_with_a_new_import() {
        let after = run(JAVA, "java", &[&["title"], &["Size"], &["2"], &["10"], &[""], &[]]).unwrap();
        assert_eq!(
            after,
            JAVA.replace("import jakarta.persistence.Id;\n", "import jakarta.persistence.Id;\nimport jakarta.validation.constraints.Size;\n")
                .replace("    private String title;", "    @Size(min = 2, max = 10)\n    private String title;")
        );
    }

    #[test]
    fn kotlin_uses_the_field_target_and_javax_files_get_javax_imports() {
        let after = run(KOTLIN, "kotlin", &[&["title"], &["NotBlank"], &["must not be blank"], &[]]).unwrap();
        assert!(after.contains("    @field:NotBlank(message = \"must not be blank\")\n    var title: String? = null"), "{after}");
        assert!(after.contains("import jakarta.validation.constraints.NotBlank\n"), "{after}");
        let javax = JAVA.replace("jakarta.persistence", "javax.persistence");
        let after = simple(&javax, "java", "due", "Past").unwrap();
        assert!(after.contains("import javax.validation.constraints.Past;\n") && !after.contains("jakarta"), "{after}");
    }

    #[test]
    fn crlf_is_preserved_for_annotation_and_import() {
        let text = JAVA.replace('\n', "\r\n");
        let after = simple(&text, "java", "title", "NotNull").unwrap();
        assert!(after.contains("import jakarta.validation.constraints.NotNull;\r\n"));
        assert!(after.contains("    @NotNull\r\n    private String title;"));
        assert_eq!(after.matches('\n').count(), after.matches("\r\n").count());
    }

    #[test]
    fn every_constraint_renders_its_parameters() {
        for (attribute, constraint, rows, expected) in [
            ("title", "NotNull", vec![""], "@NotNull"),
            ("title", "Null", vec![""], "@Null"),
            ("title", "NotEmpty", vec![""], "@NotEmpty"),
            ("title", "Email", vec!["bad"], "@Email(message = \"bad\")"),
            ("title", "Pattern", vec!["[a-z]+", ""], "@Pattern(regexp = \"[a-z]+\")"),
            ("title", "Size", vec!["0", "5", ""], "@Size(max = 5)"),
            ("title", "Size", vec!["3", "", ""], "@Size(min = 3)"),
            ("id", "Min", vec!["5", ""], "@Min(5)"),
            ("id", "Max", vec!["3000000000", "too big"], "@Max(value = 3000000000L, message = \"too big\")"),
            ("amount", "Positive", vec![""], "@Positive"),
            ("ratio", "Positive", vec![""], "@Positive"),
            ("due", "Future", vec![""], "@Future"),
            ("due", "Past", vec![""], "@Past"),
            ("paid", "NotNull", vec![], "never"),
        ] {
            let mut answers: Vec<Vec<&str>> = vec![vec![attribute], vec![constraint]];
            answers.extend(rows.iter().map(|r| vec![*r]));
            answers.push(vec![]);
            let refs: Vec<&[&str]> = answers.iter().map(Vec::as_slice).collect();
            let result = run(JAVA, "java", &refs);
            if expected == "never" {
                assert!(matches!(result, Err(Error::Invalid(_))), "{result:?}");
                continue;
            }
            let after = result.unwrap();
            assert!(after.contains(&format!("    {expected}\n")), "{constraint}: {after}");
        }
    }

    #[test]
    fn string_literals_are_escaped_for_java_and_kotlin() {
        let rows: &[&[&str]] = &[&["title"], &["Pattern"], &["^\\d+\\u0041\"$"], &["say \"hi\" \\ $x"], &[]];
        let java = run(JAVA, "java", rows).unwrap();
        assert!(java.contains(r#"@Pattern(regexp = "^\\d+\\u0041\"$", message = "say \"hi\" \\ $x")"#), "{java}");
        let kotlin = run(KOTLIN, "kotlin", rows).unwrap();
        assert!(kotlin.contains(r#"@field:Pattern(regexp = "^\\d+\\u0041\"\$", message = "say \"hi\" \\ \$x")"#), "{kotlin}");
        failed(run(JAVA, "java", &[&["title"], &["Pattern"], &["a\nb"], &[""], &[]]), "Control characters");
        failed(run(JAVA, "java", &[&["title"], &["Pattern"], &[""], &[""], &[]]), "1 to 256");
    }

    #[test]
    fn invalid_parameters_and_numeric_conflicts_are_refused() {
        let with_bounds = JAVA.replace("    private Long id;", "    @Max(10)\n    private Long id;");
        failed(run(&with_bounds, "java", &[&["id"], &["Min"], &["11"], &[""], &[]]), "greater than the existing Max");
        let ok = run(&with_bounds, "java", &[&["id"], &["Min"], &["10"], &[""], &[]]);
        assert!(ok.is_ok(), "{ok:?}");
        let with_min = JAVA.replace("    private Long id;", "    @Min(value = 10)\n    private Long id;");
        failed(run(&with_min, "java", &[&["id"], &["Max"], &["9"], &[""], &[]]), "smaller than the existing Min");
        let non_positive = JAVA.replace("    private Long id;", "    @Max(0)\n    private Long id;");
        failed(run(&non_positive, "java", &[&["id"], &["Positive"], &[""], &[]]), "non-positive Max");
        for (rows, needle) in [
            (vec![vec!["title"], vec!["Size"], vec!["5"], vec!["2"]], "minimum is greater"),
            (vec![vec!["title"], vec!["Size"], vec!["-1"], vec!["2"]], "negative"),
            (vec![vec!["title"], vec!["Size"], vec!["0"], vec![""]], "needs a minimum"),
            (vec![vec!["title"], vec!["Size"], vec!["x"], vec![""]], "must be an integer"),
            (vec![vec!["id"], vec!["Min"], vec!["1.5"]], "64-bit integer"),
            (vec![vec!["id"], vec!["Min"], vec!["-9223372036854775808"]], "above -9223372036854775808"),
        ] {
            let refs: Vec<&[&str]> = rows.iter().map(Vec::as_slice).collect();
            failed(run(JAVA, "java", &refs), needle);
        }
        let sized = JAVA.replace("    private String title;", "    @Size(max = 0)\n    private String title;");
        failed(simple(&sized, "java", "title", "NotBlank"), "allows no characters");
    }

    #[test]
    fn duplicates_and_contradictions_are_refused() {
        for (existing, constraint) in [
            ("@NotNull", "NotNull"),
            ("@jakarta.validation.constraints.NotNull", "NotNull"),
            ("@javax.validation.constraints.NotNull", "NotNull"),
            ("@Size(max = 3)", "Size"),
            ("@NotNull", "Null"),
            ("@Null", "NotNull"),
            ("@Null", "NotBlank"),
            ("@NotEmpty", "Null"),
        ] {
            let text = JAVA.replace("    private String title;", &format!("    {existing}\n    private String title;"));
            let result = if constraint == "Size" {
                run(&text, "java", &[&["title"], &[constraint], &["0"], &["5"], &[""], &[]])
            } else {
                simple(&text, "java", "title", constraint)
            };
            let written = existing.rsplit(['.', '@']).next().unwrap().split('(').next().unwrap();
            let needle = if written == constraint { "already declared" } else { "conflicts" };
            failed(result, needle);
        }
        let past = JAVA.replace("    private java.time.LocalDate due;", "    @Past\n    private java.time.LocalDate due;");
        failed(simple(&past, "java", "due", "Future"), "conflicts");
        let spaced = JAVA.replace("    private String title;", "    @NotNull\n    // note\n    @jakarta.persistence.Column(name = \"t\")\n    private String title;");
        failed(simple(&spaced, "java", "title", "NotNull"), "already declared");
        let kotlin = KOTLIN.replace("    var title", "    @field:NotNull\n    var title");
        failed(simple(&kotlin, "kotlin", "title", "NotNull"), "already declared");
    }

    #[test]
    fn unsupported_types_and_constraints_are_not_guessed() {
        assert!(matches!(simple(JAVA, "java", "id", "NotBlank"), Err(Error::Invalid(_))));
        assert!(matches!(simple(JAVA, "java", "title", "Past"), Err(Error::Invalid(_))));
        assert!(matches!(simple(JAVA, "java", "ratio", "NotNull"), Err(Error::Invalid(_))));
        assert!(matches!(simple(JAVA, "java", "paid", "Positive"), Err(Error::Invalid(_))));
        assert!(matches!(simple(JAVA, "java", "missing", "NotNull"), Err(Error::Invalid(_))));
        for (from, to) in [
            ("private String title;", "private Foo title;"),
            ("private String title;", "private java.util.List<String> title;"),
            ("private String title;", "private String[] title;"),
            ("private String title;", "private Date title;"),
        ] {
            let text = JAVA.replace(from, to).replace("private Long id;", "private Long id;");
            let (host, mut plugin, doc) = setup(&text, "java");
            let titled = candidates(&source(&doc).unwrap().0).iter().any(|(a, _, _)| a.name == "title");
            assert!(!titled, "{to}");
            drop((host, &mut plugin));
        }
        let shadowed = JAVA.replace("import jakarta.persistence.Id;", "import jakarta.persistence.Id;\nimport other.String;");
        let (_host, _plugin, doc) = setup(&shadowed, "java");
        assert!(source(&doc).is_none_or(|(c, _)| !candidates(&c).iter().any(|(a, _, _)| a.name == "title")));
        let kotlin = KOTLIN.replace("var title: String?", "var title: Foo?");
        let (_host, _plugin, doc) = setup(&kotlin, "kotlin");
        assert!(source(&doc).is_none_or(|(c, _)| !candidates(&c).iter().any(|(a, _, _)| a.name == "title")));
    }

    #[test]
    fn inherited_generic_property_access_and_unsafe_layouts_are_refused() {
        for (text, needle) in [
            (JAVA.replace("class Invoice {", "class Invoice extends Base {"), ""),
            (JAVA.replace("class Invoice {", "class Invoice<T> {"), ""),
            (JAVA.replace("@Entity", "@Entity\n@jakarta.persistence.Access(jakarta.persistence.AccessType.PROPERTY)"), ""),
            (JAVA.replace("    private String title;", "    @jakarta.persistence.Column\n    String getTitle() { return null; }\n    private String title;"), ""),
            (JAVA.replace("@Entity", "@Entity\n@jakarta.persistence.Entity"), ""),
            (JAVA.replace("    private String title;", "    private String title, other;"), "~invalid"),
            (JAVA.replace("    private String title;", "    private String\n        title;"), "Unsupported declaration layout"),
            (JAVA.replace("    private String title;", "    private String\n    title = \"a\";"), "Unsupported declaration layout"),
            (JAVA.replace("    private String title;", "    private String title; private String other;"), "Unsupported declaration layout"),
            (JAVA.replace("    private String title;", "    @Size(\n        max = 3)\n    private String title;"), "Multi-line annotations"),
            (JAVA.replace("    private String title;", "    private static String title;"), "~invalid"),
            (JAVA.replace("    private String title;", "    private transient String title;"), "~invalid"),
        ] {
            let result = simple(&text, "java", "title", "NotNull");
            match needle {
                "" => not_offered(result),
                "~invalid" => assert!(matches!(result, Err(Error::Invalid(_))), "{result:?}"),
                _ => failed(result, needle)
            }
        }
        for (text, needle) in [
            (KOTLIN.replace("class Invoice {", "class Invoice(val a: Int) {"), ""),
            (KOTLIN.replace("class Invoice {", "class Invoice : Base() {"), ""),
            (KOTLIN.replace("var title: String? = null", "var title: String? = null\n        get() = field"), "Custom Kotlin accessors"),
            (KOTLIN.replace("var title: String? = null", "val title: String by lazy { \"x\" }"), "~invalid"),
        ] {
            let result = simple(&text, "kotlin", "title", "NotNull");
            match needle {
                "" => not_offered(result),
                "~invalid" => assert!(matches!(result, Err(Error::Invalid(_))), "{result:?}"),
                _ => failed(result, needle),
            }
        }
        let embeddable = JAVA.replace("Entity", "Embeddable").replace("    @Id\n", "");
        assert!(simple(&embeddable, "java", "title", "NotNull").is_ok());
        let mapped = JAVA.replace("Entity", "MappedSuperclass");
        assert!(simple(&mapped, "java", "title", "NotNull").is_ok());
    }

    #[test]
    fn imports_are_not_duplicated_and_collisions_are_refused() {
        let explicit = JAVA.replace("import jakarta.persistence.Id;", "import jakarta.persistence.Id;\nimport jakarta.validation.constraints.NotNull;");
        let after = simple(&explicit, "java", "title", "NotNull").unwrap();
        assert_eq!(after.matches("constraints.NotNull;").count(), 1);
        let wildcard = JAVA.replace("import jakarta.persistence.Id;", "import jakarta.persistence.Id;\nimport jakarta.validation.constraints.*;");
        let after = simple(&wildcard, "java", "title", "NotNull").unwrap();
        assert_eq!(after.matches("import ").count(), 3);
        for (foreign, needle) in [("import org.jetbrains.annotations.NotNull;", "import named"), ("import javax.validation.constraints.NotNull;", "mixes")] {
            let text = JAVA.replace("import jakarta.persistence.Id;", &format!("import jakarta.persistence.Id;\n{foreign}"));
            failed(simple(&text, "java", "title", "NotNull"), needle);
        }
        let regex = JAVA.replace("import jakarta.persistence.Id;", "import jakarta.persistence.Id;\nimport java.util.regex.Pattern;");
        failed(run(&regex, "java", &[&["title"], &["Pattern"], &["a"], &[""], &[]]), "already exists");
        let no_imports = "package shop;\n@jakarta.persistence.Entity\nclass Invoice {\n    private String title;\n}\n";
        let after = simple(no_imports, "java", "title", "NotNull").unwrap();
        assert!(after.starts_with("package shop;\n\nimport jakarta.validation.constraints.NotNull;\n@jakarta"), "{after}");
    }

    #[test]
    fn cancellation_and_source_drift_never_apply_changes() {
        let (host, mut plugin, doc) = setup(JAVA, "java");
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)));
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.type_in_editor("// changed");
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert_eq!(host.editor_text(), format!("{JAVA}// changed"));
        let (host, mut plugin, doc) = setup(JAVA, "java");
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        for row in [&["title"][..], &["NotNull"], &[""]] {
            host.answer_next_prompt(Some(row));
        }
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)));
    }

    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_API / JPA_JAVAX_API plus JPA_JAKARTA_VALIDATION_API / JPA_JAVAX_VALIDATION_API jars (Kotlin: KOTLIN_COMPILER_CLASSPATH, KOTLIN_STDLIB)"]
    fn generated_validation_constraints_compile_for_java_kotlin_and_both_namespaces() {
        let base = std::env::temp_dir().join(format!("jpa-validation-compile-{}", std::process::id()));
        let cases: [(&str, &str, &[&[&str]]); 12] = [
            ("title", "NotNull", &[&[""]]),
            ("title", "NotBlank", &[&["blank"]]),
            ("title", "NotEmpty", &[&[""]]),
            ("title", "Size", &[&["1"], &["9"], &[""]]),
            ("title", "Pattern", &[&["^\\d+\\u0041\"$"], &["a \"quoted\" \\ $x"]]),
            ("title", "Email", &[&[""]]),
            ("id", "Min", &[&["1"], &[""]]),
            ("id", "Max", &[&["3000000000"], &[""]]),
            ("amount", "Positive", &[&[""]]),
            ("due", "Past", &[&[""]]),
            ("when", "Future", &[&[""]]),
            ("ratio", "Positive", &[&[""]]),
        ];
        for language in ["java", "kotlin"] {
            for ns in ["jakarta", "javax"] {
                let mut text = if language == "java" {
                    "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport java.math.BigDecimal;\nimport java.time.LocalDate;\nimport java.util.Date;\n@Entity\nclass Invoice {\n    @Id\n    private Long id;\n    private String title;\n    private BigDecimal amount;\n    private LocalDate due;\n    private Date when;\n    private double ratio;\n}\n"
                } else {
                    "package shop\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\nimport java.math.BigDecimal\nimport java.time.LocalDate\nimport java.util.Date\n@Entity\nclass Invoice {\n    @Id\n    var id: Long? = null\n    var title: String? = null\n    var amount: BigDecimal? = null\n    var due: LocalDate? = null\n    var `when`: Date? = null\n    var ratio: Double = 0.0\n}\n"
                }
                .replace("jakarta.persistence", &format!("{ns}.persistence"));
                for (attribute, constraint, rows) in cases {
                    if language == "kotlin" && attribute == "when" {
                        continue;
                    }
                    let (a, c) = ([attribute], [constraint]);
                    let mut full: Vec<&[&str]> = vec![&a, &c];
                    full.extend_from_slice(rows);
                    full.push(&[]);
                    text = run(&text, language, &full).unwrap_or_else(|e| panic!("{language} {ns} {attribute} {constraint}: {e}"));
                }
                let directory = base.join(format!("{language}-{ns}"));
                std::fs::create_dir_all(&directory).unwrap();
                let owner = directory.join(if language == "java" { "Invoice.java" } else { "Invoice.kt" });
                std::fs::write(&owner, &text).unwrap();
                let jars = [
                    std::env::var(format!("JPA_{}_API", ns.to_uppercase())).expect("persistence API jar"),
                    std::env::var(format!("JPA_{}_VALIDATION_API", ns.to_uppercase())).expect("validation API jar"),
                ];
                let separator = if cfg!(windows) { ";" } else { ":" };
                if language == "java" {
                    let result = std::process::Command::new("javac")
                        .args(["--release", "17", "-proc:none", "-classpath", &jars.join(separator), "-d"])
                        .arg(&directory)
                        .arg(&owner)
                        .output()
                        .expect("javac installed");
                    assert!(result.status.success(), "{ns}: {}\n{text}", String::from_utf8_lossy(&result.stderr));
                    continue;
                }
                let compiler = std::env::var("KOTLIN_COMPILER_CLASSPATH").expect("Kotlin compiler classpath");
                let stdlib = std::env::var("KOTLIN_STDLIB").expect("Kotlin stdlib");
                let classpath = format!("{}{separator}{stdlib}", jars.join(separator));
                let result = std::process::Command::new("java")
                    .args(["-classpath", &compiler, "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler", "-no-stdlib", "-no-reflect", "-jvm-target", "17", "-classpath"])
                    .arg(classpath)
                    .arg("-d")
                    .arg(&directory)
                    .arg(&owner)
                    .output()
                    .expect("java installed");
                assert!(result.status.success(), "{ns}: {}\n{text}", String::from_utf8_lossy(&result.stderr));
            }
        }
        std::fs::remove_dir_all(base).unwrap();
    }
}
