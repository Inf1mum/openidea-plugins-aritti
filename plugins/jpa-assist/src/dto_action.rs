//! Reviewed source-derived DTO creation. No database/schema facts or guessed
//! association IDs participate; existing copy-only generators remain unchanged.
use super::*;
const MAX_FIELDS: usize = 32;
const PLATFORM: &[(&str, &str)] = &[
    ("BigDecimal", "java.math.BigDecimal"),
    ("BigInteger", "java.math.BigInteger"),
    ("UUID", "java.util.UUID"),
    ("LocalDate", "java.time.LocalDate"),
    ("LocalDateTime", "java.time.LocalDateTime"),
    ("LocalTime", "java.time.LocalTime"),
    ("Instant", "java.time.Instant"),
    ("OffsetDateTime", "java.time.OffsetDateTime"),
    ("ZonedDateTime", "java.time.ZonedDateTime"),
    ("Duration", "java.time.Duration"),
    ("Date", "java.util.Date"),
];
pub(super) fn supported(class: &Class, ty: &str) -> bool {
    let native = if class.kotlin {
        ["String", "Long", "Int", "Short", "Byte", "Boolean", "Double", "Float", "Char", "ByteArray"].as_slice()
    } else {
        [
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
            "byte[]",
        ]
        .as_slice()
    };
    let simple = ty.rsplit('.').next().unwrap_or(ty);
    let explicit: Vec<_> = class.imports.iter().filter(|import| import.rsplit('.').next() == Some(simple)).collect();
    if native.contains(&ty) {
        let namespace = if class.kotlin { "kotlin" } else { "java.lang" };
        return explicit.iter().all(|import| import.as_str() == format!("{namespace}.{simple}"));
    }
    PLATFORM.iter().any(|(name, fqcn)| {
        if ty == *fqcn {
            return true;
        }
        ty == *name && explicit.len() == 1 && explicit[0].as_str() == *fqcn
    })
}
pub(super) fn source(doc: &Document) -> Option<Class> {
    let class = current_class(doc)?;
    let masked = mask(&doc.text);
    let at = words(&masked, "class").next()?;
    let header = masked[at + 5..].trim_start().strip_prefix(&class.name)?.trim_start();
    if header.starts_with('<') || class.attributes.len() > 128 {
        return None;
    }
    Some(class)
}
pub(super) fn fields(class: &Class) -> Option<Vec<(String, String)>> {
    let mut names = HashSet::new();
    if class.attributes.iter().any(|attribute| !names.insert(attribute.name.as_str())) {
        return None;
    }
    let mut candidate = class.clone();
    candidate.attributes.retain(|attribute| {
        attribute.relation == Relation::None
            && identifier(&attribute.name)
            && supported(class, &attribute.ty)
            && (class.kotlin
                || !["clone", "finalize", "getClass", "hashCode", "notify", "notifyAll", "toString", "wait"]
                    .contains(&attribute.name.as_str()))
    });
    let none = |_: &str| None;
    let fields = dto_fields(&candidate, &none);
    (!fields.is_empty() && fields.len() <= MAX_FIELDS).then_some(fields)
}
/// A plain Java class (no JPA marker, no inheritance, no generics) is parsed as if it were an entity so its own
/// scalar fields can be picked; documented DTO generation is not limited to entities (G067-C3, G068-C1).
fn plain_source(doc: &Document) -> Option<Class> {
    if doc.language != "java" || !doc.path.ends_with(".java") {
        return None;
    }
    let masked = mask(&doc.text);
    let top_level = |keyword: &str| words(&masked, keyword).filter(|at| *at == 0 || masked.as_bytes()[at - 1] != b'.').count();
    let marked = annotations(&masked).iter().any(|a| ["Entity", "MappedSuperclass", "Embeddable"].contains(&a.name));
    if marked || top_level("class") != 1 || ["interface", "enum", "record"].iter().any(|keyword| top_level(keyword) != 0) || masked.contains("@interface") {
        return None;
    }
    let at = words(&masked, "class").find(|at| *at == 0 || masked.as_bytes()[at - 1] != b'.')?;
    let line_start = masked[..at].rfind('\n').map_or(0, |newline| newline + 1);
    let synthetic = format!("{}@Entity\n{}", &doc.text[..line_start], &doc.text[line_start..]);
    let class = parse_class(&synthetic, &doc.path).filter(|class| class.superclass.is_none())?;
    let header = masked[at + 5..].trim_start().strip_prefix(&class.name)?.trim_start();
    if !header.starts_with('{') || !identifier(&class.name) || class.package.is_none() || package(doc).as_ref() != class.package.as_ref() || class.attributes.len() > 128 {
        return None;
    }
    Some(class)
}

fn dto_source(doc: &Document) -> Option<Class> {
    source(doc).or_else(|| plain_source(doc))
}

pub(super) fn java_level_below_record(path: &str) -> Option<u32> {
    let project = host::project_of(path)?;
    project_facts::java_level_below_record(&project, path.rsplit_once('/')?.0)
}

pub(super) fn record_refusal(level: u32) -> String {
    format!("The project's Java level is {level}; Java records need 16 or newer, so no record was generated.")
}

pub(super) const UNPROVEN_LEVEL_NOTE: &str = "Java record: requires Java 16 or newer; the project's Java level could not be proven from the build files (version catalogs, BOMs and inherited settings are not resolved).";

/// Whether a Java record may be offered for the file's module: false only when a level below 16 is proven.
pub(super) fn record_offered(path: &str) -> bool {
    java_level_below_record(path).is_none()
}

pub(super) fn eligible(doc: &Document) -> bool {
    let Some(class) = dto_source(doc) else {
        return false;
    };
    fields(&class).is_some() && (class.kotlin || record_offered(&doc.path))
}
pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let mut class = dto_source(doc).ok_or(Error::Stale)?;
    if !class.kotlin {
        if let Some(level) = java_level_below_record(&doc.path) {
            return Err(record_refusal(level).into());
        }
    }
    let unproven = !class.kotlin && project_facts::detect(project, doc.path.rsplit_once('/').ok_or(Error::Stale)?.0).java_level.is_none();
    let available = fields(&class).ok_or("No bounded unambiguous scalar fields are available for a DTO.")?;
    let default = format!("{}Dto", class.name);
    let Some(name) = host::input("DTO type name (same package; ending in Dto)", Some(&default))? else {
        return Err(Error::Cancelled);
    };
    if !identifier(&name)
        || !name.ends_with("Dto")
        || name == class.name
        || class.imports.iter().any(|import| import.rsplit('.').next() == Some(name.as_str()))
        || class.attributes.iter().any(|attribute| attribute.ty.rsplit('.').next() == Some(name.as_str()))
    {
        return Err("Use a non-conflicting Java/Kotlin identifier ending in Dto (at most 128 bytes).".into());
    }
    current(pending, doc)?;
    let items: Vec<_> = available.iter().map(|(name, ty)| PickItem::new(name, name).description(ty)).collect();
    let Some(chosen) = host::pick_many("Source scalar fields (relationships, embedded and unknown types omitted)", &items)? else {
        return Err(Error::Cancelled);
    };
    let mut selected = HashSet::new();
    if chosen.is_empty()
        || chosen.len() > MAX_FIELDS
        || chosen.iter().any(|name| !available.iter().any(|(field, _)| field == name) || !selected.insert(name.as_str()))
    {
        return Err("Choose one or more offered scalar fields exactly once.".into());
    }
    let wanted: Vec<_> = available.iter().filter(|(field, _)| selected.contains(field.as_str())).cloned().collect();
    let reuse_directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    dto_convert_action::offer_reuse(project, &pending.root, reuse_directory, class.kotlin, &doc.path, &wanted)?;
    class.attributes.retain(|attribute| attribute.relation == Relation::None && selected.contains(attribute.name.as_str()));
    let none = |_: &str| None;
    let generated = dto(&class, &none, Some(&chosen));
    let original = if class.kotlin { format!("data class {default}(") } else { format!("public record {default}(") };
    let replacement = if class.kotlin { format!("data class {name}(") } else { format!("public record {name}(") };
    if !generated.contains(&original) {
        return Err(Error::Stale);
    }
    let generated = generated.replacen(&original, &replacement, 1);
    let declared = package(doc).ok_or(Error::Stale)?;
    if generated.len() > MAX_TEXT || !generated.starts_with(&format!("package {declared}{}\n", if class.kotlin { "" } else { ";" })) {
        return Err("DTO output exceeds the validated source/package bound.".into());
    }
    if generated
        .lines()
        .filter_map(|line| line.trim().strip_prefix("import "))
        .any(|import| import.trim_end_matches(';').rsplit('.').next() == Some(name.as_str()))
    {
        return Err("DTO name conflicts with an imported output type.".into());
    }
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let target = format!("{directory}/{name}.{}", if class.kotlin { "kt" } else { "java" });
    if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
        return Err(Error::Stale);
    }
    current(pending, doc)?;
    no_collision(project, &pending.root, directory, &name, None)?;
    let note = if class.kotlin {
        "Kotlin data class: selected transfer fields are nullable with null defaults."
    } else {
        if unproven { UNPROVEN_LEVEL_NOTE } else { "Java record: the project's Java level is 16 or newer." }
    };
    let preview = format!(
        "Source: {}\nSource version: {}\nPackage: {declared}\nTarget: {target}\n\n{note}\nSource-derived draft only: no DB/schema diff, mapper, inherited fields or association-ID guesses.\n\nExact new file:\n{generated}\nOne atomic host-managed Create; no existing file is overwritten.",
        doc.path, doc.version
    );
    if !ui_kit::prompts::confirm_exact("Create source-derived scalar DTO?", &preview)? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    no_collision(project, &pending.root, directory, &name, None)?;
    Ok(FileAction {
        title: format!("Create scalar DTO {name}"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target.clone(), text: generated, overwrite: false }],
        open: Some(target),
        ..FileAction::default()
    })
}

pub(super) const CLASS_TITLE: &str = "Create DTO class with preview…";

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Pojo,
    Fluent,
    LombokAccessors,
    LombokChain,
    LombokData,
    LombokValue,
}

const STYLES: [(Style, &str, &str); 6] = [
    (Style::Pojo, "pojo", "Plain class: private fields, getters and void setters"),
    (Style::Fluent, "fluent", "Fluent class: chained setters returning this, conventional getters"),
    (Style::LombokAccessors, "lombok-getter-setter", "Lombok @Getter @Setter"),
    (Style::LombokChain, "lombok-chain", "Lombok @Getter @Setter @Accessors(chain = true)"),
    (Style::LombokData, "lombok-data", "Lombok @Data"),
    (Style::LombokValue, "lombok-value", "Lombok @Value (immutable)"),
];

impl Style {
    fn lombok(self) -> bool {
        !matches!(self, Self::Pojo | Self::Fluent)
    }
}

const TRANSFER: [&str; 7] = ["NotNull", "NotBlank", "Size", "Min", "Max", "Pattern", "Email"];

pub(super) fn class_eligible(doc: &Document) -> bool {
    dto_source(doc).is_some_and(|class| !class.kotlin && !class_fields(&class).is_empty())
}

fn class_fields(class: &Class) -> Vec<(String, String)> {
    fields(class).unwrap_or_default().into_iter().filter(|(name, ty)| accessor_safe(name, ty)).collect()
}

fn accessor_safe(name: &str, ty: &str) -> bool {
    let mut chars = name.chars();
    let first_lower = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    let second_upper = chars.next().is_some_and(|c| c.is_ascii_uppercase());
    let is_prefixed = ty == "boolean" && name.strip_prefix("is").and_then(|r| r.chars().next()).is_some_and(|c| c.is_ascii_uppercase());
    first_lower && !second_upper && !is_prefixed
}

fn capitalised(name: &str) -> String {
    let mut chars = name.chars();
    let head = chars.next().map(|c| c.to_ascii_uppercase()).unwrap_or_default();
    format!("{head}{}", chars.as_str())
}

fn snake(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for (at, ch) in chars.iter().enumerate() {
        if !ch.is_ascii_uppercase() {
            out.push(*ch);
            continue;
        }
        let after_lower = at > 0 && (chars[at - 1].is_ascii_lowercase() || chars[at - 1].is_ascii_digit());
        let acronym_end = at > 0 && chars[at - 1].is_ascii_uppercase() && chars.get(at + 1).is_some_and(|n| n.is_ascii_lowercase());
        if after_lower || acronym_end {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

fn declares(text: &str, group: &str, artifact: &str) -> bool {
    let needle = format!("{group}:{artifact}");
    let gradle = text.match_indices(&needle).any(|(at, found)| {
        text[at + found.len()..].chars().next().is_none_or(|ch| !(ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.')))
    });
    gradle || (text.contains(&format!("<groupId>{group}</groupId>")) && text.contains(&format!("<artifactId>{artifact}</artifactId>")))
}

fn build_declares(project: &Project, root: &str, directory: &str, any: &[(&str, &str)]) -> Result<bool, Error> {
    let text = mapper_action::build_text(project, root, directory)?;
    Ok(any.iter().any(|(group, artifact)| declares(&text, group, artifact)))
}

fn string_literal(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'"' || bytes[bytes.len() - 1] != b'"' {
        return false;
    }
    let mut at = 1;
    while at < bytes.len() - 1 {
        match bytes[at] {
            b'\\' => at += 2,
            b'"' => return false,
            _ => at += 1,
        }
    }
    at == bytes.len() - 1
}

pub(super) fn split_outside_strings(text: &str, separator: u8) -> Option<Vec<&str>> {
    let bytes = text.as_bytes();
    let mut parts = Vec::new();
    let (mut start, mut at, mut quoted) = (0, 0, false);
    while at < bytes.len() {
        match bytes[at] {
            b'\\' if quoted => at += 1,
            b'"' => quoted = !quoted,
            b'(' | b')' | b'{' | b'}' if !quoted => return None,
            byte if byte == separator && !quoted => {
                parts.push(&text[start..at]);
                start = at + 1;
            }
            _ => {}
        }
        at += 1;
    }
    if quoted {
        return None;
    }
    parts.push(&text[start..]);
    Some(parts)
}

fn transfer_one(name: &str, args: Option<&str>) -> Result<String, Error> {
    let refusal = || Error::from(format!("The entity @{name} has a non-literal or unsupported argument; no DTO was generated."));
    let allowed: &[(&str, bool)] = match name {
        "NotNull" | "NotBlank" | "Email" => &[("message", false)],
        "Size" => &[("min", true), ("max", true), ("message", false)],
        "Min" | "Max" => &[("value", true), ("message", false)],
        "Pattern" => &[("regexp", false), ("message", false)],
        _ => return Err(refusal()),
    };
    let text = args.map(str::trim).unwrap_or("");
    let mut kept: Vec<(String, String)> = Vec::new();
    if !text.is_empty() {
        for part in split_outside_strings(text, b',').ok_or_else(refusal)? {
            let part = part.trim();
            let (key, value) = match split_outside_strings(part, b'=').filter(|pieces| pieces.len() == 2) {
                Some(pieces) => (pieces[0].trim(), pieces[1].trim()),
                None => ("value", part),
            };
            let Some((_, numeric)) = allowed.iter().find(|(allowed_key, _)| *allowed_key == key) else {
                return Err(refusal());
            };
            let literal = if *numeric { validation_action::number(value).is_some() && value.is_ascii() } else { string_literal(value) };
            if !literal || kept.iter().any(|(k, _)| k == key) {
                return Err(refusal());
            }
            kept.push((key.to_owned(), value.to_owned()));
        }
    }
    let required = match name {
        "Min" | "Max" => Some("value"),
        "Pattern" => Some("regexp"),
        _ => None,
    };
    if required.is_some_and(|key| !kept.iter().any(|(k, _)| k == key)) {
        return Err(refusal());
    }
    if kept.is_empty() {
        return Ok(format!("@{name}"));
    }
    let joined: Vec<_> = kept.iter().map(|(k, v)| format!("{k} = {v}")).collect();
    Ok(format!("@{name}({})", joined.join(", ")))
}

pub(super) fn transferred(doc: &Document, chosen: &[(String, String)]) -> Result<(HashMap<String, Vec<String>>, String), Error> {
    let (class, namespace) = validation_action::source(doc)
        .ok_or("The entity validation annotations cannot be read (unsupported entity shape); no DTO was generated.")?;
    let masked = mask(&doc.text);
    let namespace = namespace.to_owned();
    if annotations(&masked).iter().any(|a| TRANSFER.contains(&a.name) && masked[a.start + 1..a.name_end] != *a.name) {
        return Err("A validation annotation is written fully qualified; spelling is not verified, no DTO was generated.".into());
    }
    let mut found = HashMap::new();
    for (name, _) in chosen {
        let attribute = class.attributes.iter().find(|attribute| &attribute.name == name).ok_or(Error::Stale)?;
        let site = validation_action::site(doc, &class, attribute)?;
        let mut lines = Vec::new();
        for (annotation, args) in &site.existing {
            if !TRANSFER.contains(&annotation.as_str()) {
                continue;
            }
            let wanted = format!("{namespace}.constraints.{annotation}");
            let star = format!("{namespace}.constraints.*");
            let conflicting = class.imports.iter().any(|i| i.rsplit('.').next() == Some(annotation.as_str()) && *i != wanted);
            if conflicting || !(class.imports.contains(&wanted) || class.imports.contains(&star)) {
                return Err(format!("@{annotation} is not provably a {namespace} constraint in the entity imports; no DTO was generated.").into());
            }
            lines.push(transfer_one(annotation, args.as_deref())?);
        }
        if !lines.is_empty() {
            found.insert(name.clone(), lines);
        }
    }
    Ok((found, namespace))
}

struct Plan<'a> {
    style: Style,
    name: &'a str,
    fields: &'a [(String, String)],
    no_args: bool,
    all_args: bool,
    equals: bool,
    to_string: bool,
    snake: bool,
    ignore_unknown: bool,
    constraints: &'a HashMap<String, Vec<String>>,
    namespace: &'a str,
}

fn lombok_annotation(imports: &mut Vec<String>, annotations: &mut Vec<String>, name: &str) {
    imports.push(format!("lombok.{name}"));
    annotations.push(format!("@{name}"));
}

fn render_class(class: &Class, package_name: &str, plan: &Plan) -> String {
    let style = plan.style;
    let pojo = !style.lombok();
    let mut imports: Vec<String> = plan
        .fields
        .iter()
        .filter_map(|(_, ty)| class.qualified(ty.split('<').next().unwrap_or(ty).rsplit('.').next().unwrap_or(ty)))
        .collect();
    let renamed = |name: &str| plan.snake && snake(name) != name;
    let mut class_annotations: Vec<String> = Vec::new();
    match style {
        Style::LombokAccessors | Style::LombokChain => {
            lombok_annotation(&mut imports, &mut class_annotations, "Getter");
            lombok_annotation(&mut imports, &mut class_annotations, "Setter");
            if style == Style::LombokChain {
                imports.push("lombok.experimental.Accessors".into());
                class_annotations.push("@Accessors(chain = true)".into());
            }
        }
        Style::LombokData => lombok_annotation(&mut imports, &mut class_annotations, "Data"),
        Style::LombokValue => lombok_annotation(&mut imports, &mut class_annotations, "Value"),
        Style::Pojo | Style::Fluent => {}
    }
    if style.lombok() {
        if plan.no_args {
            lombok_annotation(&mut imports, &mut class_annotations, "NoArgsConstructor");
        }
        if plan.all_args {
            lombok_annotation(&mut imports, &mut class_annotations, "AllArgsConstructor");
        }
        if plan.equals {
            lombok_annotation(&mut imports, &mut class_annotations, "EqualsAndHashCode");
        }
        if plan.to_string {
            lombok_annotation(&mut imports, &mut class_annotations, "ToString");
        }
    }
    if plan.ignore_unknown {
        imports.push("com.fasterxml.jackson.annotation.JsonIgnoreProperties".into());
        class_annotations.push("@JsonIgnoreProperties(ignoreUnknown = true)".into());
    }
    if plan.fields.iter().any(|(name, _)| renamed(name)) {
        imports.push("com.fasterxml.jackson.annotation.JsonProperty".into());
    }
    for lines in plan.constraints.values() {
        for line in lines {
            let name: String = line[1..].chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            imports.push(format!("{}.constraints.{name}", plan.namespace));
        }
    }
    let has_array = plan.fields.iter().any(|(_, ty)| ty == "byte[]");
    if pojo && (plan.equals || plan.to_string) && has_array {
        imports.push("java.util.Arrays".into());
    }
    if pojo && plan.equals {
        imports.push("java.util.Objects".into());
    }
    imports.sort();
    imports.dedup();
    imports.retain(|i| i.rsplit_once('.').map(|(pk, _)| pk) != Some(package_name));

    let mut out = format!("package {package_name};\n\n");
    for import in &imports {
        out.push_str(&format!("import {import};\n"));
    }
    if !imports.is_empty() {
        out.push('\n');
    }
    for annotation in &class_annotations {
        out.push_str(&format!("{annotation}\n"));
    }
    out.push_str(&format!("public class {} {{\n", plan.name));
    let modifier = if style == Style::LombokValue { "" } else { "private " };
    let annotated = plan.fields.iter().any(|(name, _)| renamed(name) || plan.constraints.contains_key(name));
    for (at, (name, ty)) in plan.fields.iter().enumerate() {
        if annotated || at == 0 {
            out.push('\n');
        }
        for line in plan.constraints.get(name).into_iter().flatten() {
            out.push_str(&format!("    {line}\n"));
        }
        if renamed(name) {
            out.push_str(&format!("    @JsonProperty(\"{}\")\n", snake(name)));
        }
        out.push_str(&format!("    {modifier}{ty} {name};\n"));
    }
    if pojo {
        let class_name = plan.name;
        if plan.no_args {
            out.push_str(&format!("\n    public {class_name}() {{\n    }}\n"));
        }
        if plan.all_args {
            let params: Vec<_> = plan.fields.iter().map(|(n, t)| format!("{t} {n}")).collect();
            let assigns: String = plan.fields.iter().map(|(n, _)| format!("        this.{n} = {n};\n")).collect();
            out.push_str(&format!("\n    public {class_name}({}) {{\n{assigns}    }}\n", params.join(", ")));
        }
        for (name, ty) in plan.fields {
            let suffix = capitalised(name);
            let prefix = if ty == "boolean" { "is" } else { "get" };
            out.push_str(&format!("\n    public {ty} {prefix}{suffix}() {{\n        return {name};\n    }}\n"));
            if style == Style::Fluent {
                out.push_str(&format!("\n    public {class_name} set{suffix}({ty} {name}) {{\n        this.{name} = {name};\n        return this;\n    }}\n"));
            } else {
                out.push_str(&format!("\n    public void set{suffix}({ty} {name}) {{\n        this.{name} = {name};\n    }}\n"));
            }
        }
        if plan.equals {
            let compares: Vec<_> = plan
                .fields
                .iter()
                .map(|(n, t)| if t == "byte[]" { format!("Arrays.equals(this.{n}, other.{n})") } else { format!("Objects.equals(this.{n}, other.{n})") })
                .collect();
            out.push_str(&format!(
                "\n    @Override\n    public boolean equals(Object object) {{\n        if (this == object) {{\n            return true;\n        }}\n        if (object == null || getClass() != object.getClass()) {{\n            return false;\n        }}\n        {class_name} other = ({class_name}) object;\n        return {};\n    }}\n",
                compares.join("\n                && ")
            ));
            let plain: Vec<_> = plan.fields.iter().filter(|(_, t)| t != "byte[]").map(|(n, _)| format!("this.{n}")).collect();
            let arrays: Vec<_> = plan.fields.iter().filter(|(_, t)| t == "byte[]").map(|(n, _)| n.as_str()).collect();
            let body = if arrays.is_empty() {
                format!("        return Objects.hash({});\n", plain.join(", "))
            } else {
                let mut body = if plain.is_empty() { "        int result = 1;\n".to_owned() } else { format!("        int result = Objects.hash({});\n", plain.join(", ")) };
                for array in arrays {
                    body.push_str(&format!("        result = 31 * result + Arrays.hashCode(this.{array});\n"));
                }
                body.push_str("        return result;\n");
                body
            };
            out.push_str(&format!("\n    @Override\n    public int hashCode() {{\n{body}    }}\n"));
        }
        if plan.to_string {
            let parts: Vec<_> = plan
                .fields
                .iter()
                .enumerate()
                .map(|(at, (n, t))| {
                    let value = if t == "byte[]" { format!("Arrays.toString(this.{n})") } else { format!("this.{n}") };
                    format!("+ \"{}{n}=\" + {value}", if at == 0 { "" } else { ", " })
                })
                .collect();
            out.push_str(&format!(
                "\n    @Override\n    public String toString() {{\n        return \"{class_name}{{\"\n                {}\n                + '}}';\n    }}\n",
                parts.join("\n                ")
            ));
        }
    }
    out.push_str("}\n");
    out
}

pub(super) fn resolve_class(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let class = dto_source(doc).filter(|class| !class.kotlin).ok_or(Error::Stale)?;
    let available = class_fields(&class);
    if available.is_empty() {
        return Err("No bounded unambiguous scalar field has a bean-safe name for a DTO class.".into());
    }
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let choices: Vec<_> = STYLES.iter().map(|(_, key, label)| PickItem::new(key, label)).collect();
    let key = host::pick("DTO class form", &choices)?.ok_or(Error::Cancelled)?;
    let style = STYLES.iter().find(|(_, k, _)| *k == key).map(|(s, _, _)| *s).ok_or("Choose one of the offered DTO class forms.")?;
    if style.lombok()
        && !mapper_action::uses_lombok(&doc.text)
        && !build_declares(project, &pending.root, directory, &[("org.projectlombok", "lombok")])?
    {
        return Err("Lombok is not provably used: the entity imports no lombok type and no pom.xml/build.gradle(.kts) above it declares org.projectlombok:lombok literally; no DTO was generated.".into());
    }
    let default = format!("{}Dto", class.name);
    let Some(name) = host::input("DTO class name (same package; ending in Dto)", Some(&default))? else {
        return Err(Error::Cancelled);
    };
    let name = name.trim().to_owned();
    if !identifier(&name)
        || !name.ends_with("Dto")
        || name == class.name
        || class.imports.iter().any(|import| import.rsplit('.').next() == Some(name.as_str()))
        || class.attributes.iter().any(|attribute| attribute.ty.rsplit('.').next() == Some(name.as_str()))
    {
        return Err("Use a non-conflicting Java identifier ending in Dto (at most 128 bytes).".into());
    }
    current(pending, doc)?;
    let items: Vec<_> = available.iter().map(|(n, t)| PickItem::new(n, n).description(t)).collect();
    let chosen = host::pick_many("Source scalar fields (relationships, embedded and unknown types omitted)", &items)?.ok_or(Error::Cancelled)?;
    let mut seen = HashSet::new();
    if chosen.is_empty() || chosen.len() > MAX_FIELDS || chosen.iter().any(|n| !available.iter().any(|(f, _)| f == n) || !seen.insert(n.as_str())) {
        return Err("Choose one or more offered scalar fields exactly once.".into());
    }
    let fields: Vec<_> = available.iter().filter(|(n, _)| chosen.contains(n)).cloned().collect();
    dto_convert_action::offer_reuse(project, &pending.root, directory, false, &doc.path, &fields)?;

    let mut members: Vec<(&str, &str)> = Vec::new();
    match style {
        Style::LombokValue => {}
        Style::LombokData => members.extend([("no-args", "No-args constructor"), ("all-args", "All-args constructor")]),
        _ => members.extend([
            ("no-args", "No-args constructor"),
            ("all-args", "All-args constructor"),
            ("equals-hash-code", "equals and hashCode"),
            ("to-string", "toString"),
        ]),
    }
    let mut picked: Vec<String> = Vec::new();
    if !members.is_empty() {
        let items: Vec<_> = members.iter().map(|(key, label)| PickItem::new(key, label)).collect();
        picked = host::pick_many("Extra members (none is allowed)", &items)?.ok_or(Error::Cancelled)?;
        let mut unique = HashSet::new();
        if picked.iter().any(|key| !members.iter().any(|(k, _)| k == key) || !unique.insert(key.as_str())) {
            return Err("Choose only the offered members, each once.".into());
        }
    }
    let has = |key: &str| picked.iter().any(|k| k == key);

    let jackson_choices = [
        ("none", "No Jackson annotations"),
        ("snake-case", "@JsonProperty with camelCase to snake_case names"),
        ("ignore-unknown", "@JsonIgnoreProperties(ignoreUnknown = true)"),
        ("snake-case-ignore-unknown", "Both"),
    ];
    let items: Vec<_> = jackson_choices.iter().map(|(key, label)| PickItem::new(key, label)).collect();
    let jackson = host::pick("Jackson annotations", &items)?.ok_or(Error::Cancelled)?;
    if !jackson_choices.iter().any(|(key, _)| *key == jackson) {
        return Err("Choose one of the offered Jackson options.".into());
    }
    let (snake_case, ignore_unknown) = (jackson.starts_with("snake-case"), jackson.ends_with("ignore-unknown"));
    if snake_case || ignore_unknown {
        if style == Style::LombokValue {
            return Err("Jackson deserialisation of an immutable @Value class needs a creator this generator does not emit; choose another form.".into());
        }
        current(pending, doc)?;
        if !build_declares(
            project,
            &pending.root,
            directory,
            &[("com.fasterxml.jackson.core", "jackson-annotations"), ("com.fasterxml.jackson.core", "jackson-databind")],
        )? {
            return Err("Jackson is not provably on this project: neither com.fasterxml.jackson.core:jackson-annotations nor jackson-databind is written literally in a pom.xml/build.gradle(.kts) above this source; no DTO was generated.".into());
        }
    }
    if snake_case {
        let mut names = HashSet::new();
        if !fields.iter().all(|(n, _)| names.insert(snake(n))) {
            return Err("Two selected fields map to the same snake_case JSON name; no DTO was generated.".into());
        }
    }

    let mut constraints = HashMap::new();
    let mut namespace = String::new();
    let applicable = annotations(&mask(&doc.text)).iter().any(|a| TRANSFER.contains(&a.name));
    if applicable {
        let validator_proven = project_facts::detect(project, directory).bean_validation == project_facts::Fact::Yes;
        let choices: Vec<_> = [
            ("none", "Do not copy validation"),
            ("copy", "Copy NotNull, NotBlank, Size, Min, Max, Pattern, Email (literal arguments)"),
            ("configure", "Configure each constraint: keep or drop it, set its message, add NotNull, NotBlank or Email"),
        ]
        .into_iter()
        .filter(|(key, _)| *key == "none" || validator_proven)
            .collect();
        let items: Vec<_> = choices.iter().map(|(key, label)| PickItem::new(key, label)).collect();
        let answer = host::pick("Bean Validation transfer", &items)?.ok_or(Error::Cancelled)?;
        match answer.as_str() {
            "none" => {}
            "copy" => (constraints, namespace) = transferred(doc, &fields)?,
            "configure" => (constraints, namespace) = dto_validation::configure(doc, &fields)?,
            _ => return Err("Choose one of the offered validation options.".into()),
        }
    }

    let declared = package(doc).ok_or(Error::Stale)?;
    let plan = Plan {
        style,
        name: &name,
        fields: &fields,
        no_args: has("no-args"),
        all_args: has("all-args"),
        equals: has("equals-hash-code"),
        to_string: has("to-string"),
        snake: snake_case,
        ignore_unknown,
        constraints: &constraints,
        namespace: &namespace,
    };
    let mut generated = render_class(&class, &declared, &plan);
    if generated.lines().filter_map(|line| line.trim().strip_prefix("import ")).any(|import| import.trim_end_matches(';').rsplit('.').next() == Some(name.as_str())) {
        return Err("DTO name conflicts with an imported output type.".into());
    }
    if doc.text.contains("\r\n") {
        generated = generated.replace('\n', "\r\n");
    }
    if generated.len() > MAX_TEXT {
        return Err("DTO output exceeds the validated source/package bound.".into());
    }
    let target = format!("{directory}/{name}.java");
    if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
        return Err(Error::Stale);
    }
    current(pending, doc)?;
    no_collision(project, &pending.root, directory, &name, None)?;
    let mut notes = vec!["Java class DTO: source-derived draft only (no mapper, inherited fields or association IDs).".to_owned()];
    match style {
        Style::LombokValue => notes.push("Lombok @Value: implies a final class and private final fields (so no modifiers are written), getters, no setters, all-args constructor generated by Lombok; no no-args constructor.".into()),
        Style::LombokData => notes.push("Lombok @Data: getters, setters, equals/hashCode/toString; Lombok skips its constructor when an explicit one exists.".into()),
        Style::LombokChain => notes.push("Lombok @Accessors is an experimental feature; chain = true makes setters return this.".into()),
        _ => {}
    }
    if style.lombok() {
        notes.push("Lombok must be on the compile classpath and annotation processor path.".into());
    }
    if style == Style::Fluent && (snake_case || ignore_unknown) {
        notes.push("Jackson binding to chained (non-void) setters was not verified.".into());
    }
    if !constraints.is_empty() {
        let count: usize = constraints.values().map(Vec::len).sum();
        notes.push(format!("Copied {count} literal Bean Validation constraint(s); other entity annotations are not transferred."));
    }
    let preview = format!(
        "Source: {}\nSource version: {}\nPackage: {declared}\nTarget: {target}\n\n{}\n\nExact new file:\n{generated}\nOne atomic host-managed Create; no existing file is overwritten.",
        doc.path,
        doc.version,
        notes.join("\n")
    );
    if !ui_kit::prompts::confirm_exact("Create DTO class?", &preview)? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    no_collision(project, &pending.root, directory, &name, None)?;
    Ok(FileAction {
        title: format!("Create DTO class {name}"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target.clone(), text: generated, overwrite: false }],
        open: Some(target),
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const JAVA: &str = "package shop;\nimport jakarta.persistence.*;\nimport java.time.LocalDate;\n@Entity\npublic class Customer {\n @Id Long id;\n String title;\n LocalDate joined;\n @ManyToOne Other parent;\n @Embedded Address address;\n Money amount;\n}\n";
    fn setup(path: &str, source: &str, language: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(path, language, source, &[]);
        let doc = Document { path: path.into(), text: source.into(), language: language.into(), version: 1 };
        (host, JpaAssist::default(), doc)
    }
    fn action(plugin: &mut JpaAssist, doc: &Document) -> FileAction {
        let action = plugin
            .file_actions(doc, Range::default())
            .into_iter()
            .find(|action| action.title == "Create scalar DTO with preview…")
            .unwrap();
        assert!(action.operations.is_empty() && action.edits.is_empty());
        action
    }
    fn answer(host: &MockHost, name: &str, fields: &[&str], confirm: bool) {
        host.answer_next_prompt(Some(&[name]));
        host.answer_next_prompt(Some(fields));
        host.answer_next_prompt(confirm.then_some(&[]));
    }
    fn created(action: &FileAction) -> (&str, &str) {
        let [FileOperation::Create { path, text, overwrite: false }] = action.operations.as_slice() else {
            panic!("one non-overwriting create")
        };
        assert!(action.edits.is_empty());
        (path, text)
    }
    #[test]
    fn java_dto_is_exact_reviewed_create_from_selected_current_source_fields_only() {
        let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Customer.java", JAVA, "java");
        host.add_file(&doc.path, &JAVA.replace("String title;", "String oldDiskField;"));
        let pending = action(&mut plugin, &doc);
        answer(&host, "CustomerSummaryDto", &["id", "title", "joined"], true);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let (path, text) = created(&resolved);
        assert_eq!(path, "/project/src/main/java/shop/CustomerSummaryDto.java");
        assert_eq!(resolved.open.as_deref(), Some(path));
        assert!(
            text.contains("package shop;")
                && text.contains("import java.time.LocalDate;")
                && text.contains("public record CustomerSummaryDto(")
        );
        for field in ["Long id", "String title", "LocalDate joined"] {
            assert!(text.contains(field), "{text}");
        }
        for omitted in ["oldDiskField", "parentId", "address", "amount"] {
            assert!(!text.contains(omitted));
        }
        assert_eq!(host.editor_text(), JAVA);
        let project = host.invocation().project().unwrap();
        assert!(matches!(project.read_file(path), Err(Error::NotFound(_))));
        assert!(host.requests().is_empty());
    }
    #[test]
    fn kotlin_reuses_data_class_generator_and_documents_nullable_transfer_shape() {
        let source =
            "package shop\nimport jakarta.persistence.*\n@Entity\nclass Customer(@Id var id: Long? = null, var title: String = \"\") {}\n";
        let (host, mut plugin, doc) = setup("/project/src/main/kotlin/shop/Customer.kt", source, "kotlin");
        let pending = action(&mut plugin, &doc);
        answer(&host, "CustomerDto", &["id", "title"], true);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let (path, text) = created(&resolved);
        assert!(path.ends_with("/CustomerDto.kt"));
        assert!(text.contains("package shop\n") && text.contains("data class CustomerDto(") && text.contains("val title: String? = null"));
        assert_eq!(host.editor_text(), source);
    }
    #[test]
    fn each_cancellation_stage_and_forged_field_selection_leave_files_untouched() {
        for stage in 0..3 {
            let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", JAVA, "java");
            let pending = action(&mut plugin, &doc);
            if stage > 0 {
                host.answer_next_prompt(Some(&["CustomerDto"]));
            }
            if stage > 1 {
                host.answer_next_prompt(Some(&["id"]));
            }
            assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)));
            assert_eq!(host.editor_text(), JAVA);
        }
        for fields in [&["parentId"][..], &["id", "id"][..], &[][..]] {
            let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", JAVA, "java");
            let pending = action(&mut plugin, &doc);
            answer(&host, "CustomerDto", fields, true);
            assert!(plugin.resolve_file_action(&doc, pending).is_err());
            assert_eq!(host.editor_text(), JAVA);
        }
    }
    #[test]
    fn target_names_existing_files_and_declared_type_collisions_are_refused() {
        for name in ["../EscapeDto", "wrong.PackageDto", "Customer", "class", "CustomerDto;", "x\nDto"] {
            let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", JAVA, "java");
            let pending = action(&mut plugin, &doc);
            answer(&host, name, &["id"], true);
            assert!(plugin.resolve_file_action(&doc, pending).is_err());
        }
        for (extension, body) in [("java", "package shop; public class CustomerDto {}"), ("kt", "package shop\nclass CustomerDto")] {
            let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", JAVA, "java");
            host.add_file(&format!("src/shop/CustomerDto.{extension}"), body);
            let pending = action(&mut plugin, &doc);
            answer(&host, "CustomerDto", &["id"], true);
            assert!(plugin.resolve_file_action(&doc, pending).is_err());
        }
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", JAVA, "java");
        host.add_file("src/shop/Other.java", "package shop; interface CustomerDto {}");
        let pending = action(&mut plugin, &doc);
        answer(&host, "CustomerDto", &["id"], true);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
    }
    #[test]
    fn stale_source_project_changes_and_unprovable_entity_shapes_have_no_output() {
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", JAVA, "java");
        let pending = action(&mut plugin, &doc);
        let mut changed = doc.clone();
        changed.version += 1;
        assert!(matches!(plugin.resolve_file_action(&changed, pending.clone()), Err(Error::Stale)));
        host.type_in_editor("// changed");
        assert!(matches!(plugin.resolve_file_action(&doc, pending.clone()), Err(Error::Stale)));
        host.set_project_root(Some("/other"));
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        for source in [
            JAVA.replace("class Customer", "class Customer<T>"),
            JAVA.replace("class Customer", "class Customer extends Base"),
            JAVA.replace("package shop;", "package wrong;"),
            JAVA.replace("import jakarta.persistence.*;", "import unknown.Entity;"),
        ] {
            let (_host, mut plugin, doc) = setup("/project/src/shop/Customer.java", &source, "java");
            assert!(!plugin.file_actions(&doc, Range::default()).iter().any(|action| action.title.starts_with("Create scalar DTO")));
        }
    }
    #[test]
    fn shadowed_imports_relationships_unknown_types_and_output_bounds_are_conservative() {
        let source = JAVA.replace("import java.time.LocalDate;", "import foreign.String;\nimport foreign.LocalDate;");
        let (_host, _, doc) = setup("/project/src/shop/Customer.java", &source, "java");
        let class = source_class(&doc);
        let offered = fields(&class).unwrap();
        assert_eq!(offered, [("id".into(), "Long".into())]);
        let source = JAVA.replace(
            "@Id Long id;",
            &format!("@Id Long id;\n{}", (0..MAX_FIELDS).map(|n| format!("String field{n};\n")).collect::<String>()),
        );
        let (_host, mut plugin, doc) = setup("/project/src/shop/Customer.java", &source, "java");
        assert!(!plugin.file_actions(&doc, Range::default()).iter().any(|action| action.title.starts_with("Create scalar DTO")));
    }
    fn source_class(doc: &Document) -> Class {
        source(doc).unwrap()
    }
    #[test]
    fn dirty_destination_and_bounded_folder_discovery_refuse_creates() {
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", JAVA, "java");
        host.add_file(&doc.path, JAVA);
        let pending = action(&mut plugin, &doc);
        host.open_editor("/project/src/shop/CustomerDto.java", "java", "unsaved destination", &[]);
        answer(&host, "CustomerDto", &["id"], true);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert_eq!(host.editor_text(), "unsaved destination");
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", JAVA, "java");
        for n in 0..65 {
            host.add_file(&format!("src/shop/Type{n}.java"), &format!("package shop; class Type{n} {{}}"));
        }
        let pending = action(&mut plugin, &doc);
        answer(&host, "CustomerDto", &["id"], true);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert_eq!(host.editor_text(), JAVA);
    }

    const PLAIN: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Customer {\n @Id Long id;\n String title;\n boolean flag;\n Boolean seen;\n byte[] blob;\n}\n";
    const VALID: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.validation.constraints.NotBlank;\nimport jakarta.validation.constraints.Size;\nimport jakarta.validation.constraints.Pattern;\nimport jakarta.validation.constraints.Email;\nimport jakarta.validation.constraints.Min;\nimport jakarta.validation.constraints.Max;\nimport jakarta.validation.constraints.NotNull;\n@Entity\npublic class Customer {\n @Id Long id;\n @NotBlank @Size(min = 2, max = 40, message = \"too, long\") String title;\n @Pattern(regexp = \"[a-z]+,\\\\d\") String code;\n @Email String mail;\n @Min(1) @Max(value = 9L) Integer rank;\n @NotNull Boolean seen;\n}\n";
    const POM_LOMBOK: &str = "<project><dependency><groupId>org.projectlombok</groupId><artifactId>lombok</artifactId></dependency></project>";
    const POM_VALIDATION: &str = "<project><dependency><artifactId>spring-boot-starter-validation</artifactId></dependency></project>";
    const POM_JACKSON: &str = "<project><dependency><groupId>com.fasterxml.jackson.core</groupId><artifactId>jackson-databind</artifactId></dependency><dependency><artifactId>spring-boot-starter-validation</artifactId></dependency></project>";
    const CPATH: &str = "/project/src/main/java/shop/Customer.java";

    fn run_class(source: &str, build: Option<(&str, &str)>, answers: &[&[&str]], confirm: bool) -> (MockHost, Result<FileAction, Error>) {
        let (host, mut plugin, doc) = setup(CPATH, source, "java");
        if let Some((name, text)) = build {
            host.add_file(name, text);
        }
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == CLASS_TITLE).unwrap();
        assert!(pending.operations.is_empty() && pending.edits.is_empty());
        for answer in answers {
            host.answer_next_prompt(Some(answer));
        }
        host.answer_next_prompt(confirm.then_some(&[]));
        let result = plugin.resolve_file_action(&doc, pending);
        assert_eq!(host.editor_text(), source);
        (host, result)
    }
    fn text_of(result: Result<FileAction, Error>) -> String {
        let action = result.unwrap_or_else(|e| panic!("{e:?}"));
        created(&action).1.to_owned()
    }

    #[test]
    fn plain_pojo_is_exact_with_constructors_equals_hash_code_and_to_string() {
        let answers: [&[&str]; 5] = [&["pojo"], &["CustomerDto"], &["id", "flag", "blob"], &["no-args", "all-args", "equals-hash-code", "to-string"], &["none"]];
        let (_, result) = run_class(PLAIN, None, &answers, true);
        let expected = "package shop;\n\nimport java.util.Arrays;\nimport java.util.Objects;\n\npublic class CustomerDto {\n\n    private Long id;\n    private boolean flag;\n    private byte[] blob;\n\n    public CustomerDto() {\n    }\n\n    public CustomerDto(Long id, boolean flag, byte[] blob) {\n        this.id = id;\n        this.flag = flag;\n        this.blob = blob;\n    }\n\n    public Long getId() {\n        return id;\n    }\n\n    public void setId(Long id) {\n        this.id = id;\n    }\n\n    public boolean isFlag() {\n        return flag;\n    }\n\n    public void setFlag(boolean flag) {\n        this.flag = flag;\n    }\n\n    public byte[] getBlob() {\n        return blob;\n    }\n\n    public void setBlob(byte[] blob) {\n        this.blob = blob;\n    }\n\n    @Override\n    public boolean equals(Object object) {\n        if (this == object) {\n            return true;\n        }\n        if (object == null || getClass() != object.getClass()) {\n            return false;\n        }\n        CustomerDto other = (CustomerDto) object;\n        return Objects.equals(this.id, other.id)\n                && Objects.equals(this.flag, other.flag)\n                && Arrays.equals(this.blob, other.blob);\n    }\n\n    @Override\n    public int hashCode() {\n        int result = Objects.hash(this.id, this.flag);\n        result = 31 * result + Arrays.hashCode(this.blob);\n        return result;\n    }\n\n    @Override\n    public String toString() {\n        return \"CustomerDto{\"\n                + \"id=\" + this.id\n                + \", flag=\" + this.flag\n                + \", blob=\" + Arrays.toString(this.blob)\n                + '}';\n    }\n}\n";
        assert_eq!(text_of(result), expected);
    }

    #[test]
    fn fluent_form_chains_setters_and_keeps_conventional_getters() {
        let answers: [&[&str]; 5] = [&["fluent"], &["CustomerDto"], &["title", "seen"], &[], &["none"]];
        let text = text_of(run_class(PLAIN, None, &answers, true).1);
        assert!(text.contains("public CustomerDto setTitle(String title) {\n        this.title = title;\n        return this;\n    }"), "{text}");
        assert!(text.contains("public Boolean getSeen()") && !text.contains("void set") && !text.contains("equals"), "{text}");
    }

    #[test]
    fn lombok_forms_need_provable_lombok_and_render_verified_annotations() {
        fn answers<'a>(form: &'a [&'a str]) -> [&'a [&'a str]; 5] {
            [form, &["CustomerDto"], &["id", "title"], &["no-args", "equals-hash-code"], &["none"]]
        }
        for form in [&["lombok-getter-setter"][..], &["lombok-chain"], &["lombok-data"]] {
            let (_, result) = run_class(PLAIN, None, &answers(form), true);
            let error = format!("{:?}", result.err().unwrap());
            assert!(error.contains("Lombok is not provably used"), "{form:?}: {error}");
        }
        let imported = PLAIN.replace("import jakarta.persistence.*;", "import jakarta.persistence.*;\nimport lombok.Getter;");
        let text = text_of(run_class(&imported, None, &answers(&["lombok-getter-setter"]), true).1);
        assert!(text.contains("import lombok.EqualsAndHashCode;\nimport lombok.Getter;\nimport lombok.NoArgsConstructor;\nimport lombok.Setter;\n\n@Getter\n@Setter\n@NoArgsConstructor\n@EqualsAndHashCode\npublic class CustomerDto {"), "{text}");
        assert!(!text.contains("public Long getId") && text.contains("    private Long id;\n    private String title;\n"), "{text}");
        let text = text_of(run_class(PLAIN, Some(("pom.xml", POM_LOMBOK)), &answers(&["lombok-chain"]), true).1);
        assert!(text.contains("import lombok.experimental.Accessors;") && text.contains("@Accessors(chain = true)\n"), "{text}");
        let data: [&[&str]; 5] = [&["lombok-data"], &["CustomerDto"], &["id"], &["all-args"], &["none"]];
        let gradle = ("build.gradle", "dependencies { compileOnly 'org.projectlombok:lombok:1.18.30' }");
        let text = text_of(run_class(PLAIN, Some(gradle), &data, true).1);
        assert!(text.contains("@Data\n@AllArgsConstructor\npublic class CustomerDto"), "{text}");
        let lookalike = ("build.gradle", "dependencies { compileOnly 'org.projectlombok:lombok-mapstruct-binding:0.2.0' }");
        assert!(run_class(PLAIN, Some(lookalike), &data, true).1.is_err());
    }

    #[test]
    fn lombok_value_is_immutable_without_constructor_options_and_refuses_jackson() {
        let value: [&[&str]; 4] = [&["lombok-value"], &["CustomerDto"], &["id", "title"], &["none"]];
        let text = text_of(run_class(PLAIN, Some(("pom.xml", POM_LOMBOK)), &value, true).1);
        assert!(text.contains("@Value\npublic class CustomerDto {") && text.contains("    Long id;\n    String title;\n"), "{text}");
        assert!(!text.contains("Constructor") && !text.contains("@Setter"), "{text}");
        let jackson: [&[&str]; 4] = [&["lombok-value"], &["CustomerDto"], &["id"], &["snake-case"]];
        let pom = format!("{POM_LOMBOK}{POM_JACKSON}");
        let error = format!("{:?}", run_class(PLAIN, Some(("pom.xml", &pom)), &jackson, true).1.err().unwrap());
        assert!(error.contains("immutable"), "{error}");
    }

    #[test]
    fn lombok_value_writes_no_modifiers_that_value_implies() {
        let answers: [&[&str]; 5] = [&["lombok-value"], &["CustomerDto"], &["id", "title"], &["none"], &[]];
        let text = text_of(run_class(PLAIN, Some(("pom.xml", POM_LOMBOK)), &answers[..4], true).1);
        assert!(text.contains("@Value\npublic class CustomerDto {") && text.contains("    Long id;\n    String title;\n"), "{text}");
        assert!(!text.contains("private") && !text.contains("final"), "{text}");
    }

    #[test]
    fn validation_transfer_is_offered_only_with_hibernate_validator_or_the_starter() {
        let offered = |build: Option<(&str, &str)>| {
            let answers: [&[&str]; 6] = [&["pojo"], &["CustomerDto"], &["title"], &[], &["none"], &["copy"]];
            match run_class(VALID, build, &answers, true).1 {
                Ok(_) => Some(true),
                Err(Error::Invalid(message)) if message.contains("not one of the items") => Some(false),
                Err(other) => panic!("{other:?}"),
            }
        };
        assert_eq!(offered(Some(("pom.xml", POM_VALIDATION))), Some(true));
        assert_eq!(offered(Some(("build.gradle", "implementation 'org.hibernate.validator:hibernate-validator:8.0.1.Final'"))), Some(true));
        assert_eq!(offered(Some(("pom.xml", "<project><artifactId>spring-boot-starter-web</artifactId></project>"))), Some(false));
        assert_eq!(offered(None), Some(false));
        assert_eq!(offered(Some(("build.gradle", "implementation(libs.validation)"))), Some(false));
    }

    #[test]
    fn configured_validation_keeps_drops_rewords_and_adds_constraints() {
        let answers: [&[&str]; 11] = [&["pojo"], &["CustomerDto"], &["title", "code"], &[], &["none"], &["configure"], &["0", "1"], &["title|NotNull"], &["Title is required"], &[""], &["Must be set"]];
        let text = text_of(run_class(VALID, Some(("pom.xml", POM_VALIDATION)), &answers, true).1);
        assert!(text.contains("    @NotBlank(message = \"Title is required\")\n    @Size(min = 2, max = 40, message = \"too, long\")\n    @NotNull(message = \"Must be set\")\n    private String title;"), "{text}");
        assert!(text.contains("    @NotNull(message = \"Must be set\")\n    private String title;") && !text.contains("@Pattern"), "{text}");
        assert!(text.contains("import jakarta.validation.constraints.NotNull;"), "{text}");
        let nothing: [&[&str]; 8] = [&["pojo"], &["CustomerDto"], &["title"], &[], &["none"], &["configure"], &[], &[]];
        let text = text_of(run_class(VALID, Some(("pom.xml", POM_VALIDATION)), &nothing, true).1);
        assert!(!text.contains("@NotBlank") && !text.contains("validation"), "{text}");
        let forged: [&[&str]; 7] = [&["pojo"], &["CustomerDto"], &["title"], &[], &["none"], &["configure"], &["9"]];
        assert!(run_class(VALID, Some(("pom.xml", POM_VALIDATION)), &forged, true).1.is_err());
        let unproven: [&[&str]; 6] = [&["pojo"], &["CustomerDto"], &["title"], &[], &["none"], &["configure"]];
        assert!(run_class(VALID, None, &unproven, true).1.is_err());
    }

    #[test]
    fn plain_non_entity_classes_get_scalar_dto_and_dto_class_offers() {
        let plain = "package shop;\n\npublic class PlainThing {\n\n    private String label;\n\n    private int count;\n}\n";
        let path = "/project/src/main/java/shop/PlainThing.java";
        let (host, mut plugin, doc) = setup(path, plain, "java");
        let titles: Vec<_> = plugin.file_actions(&doc, Range::default()).into_iter().map(|a| a.title).collect();
        assert!(titles.iter().any(|t| t == "Create scalar DTO with preview…") && titles.iter().any(|t| t == CLASS_TITLE), "{titles:?}");
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title.starts_with("Create scalar DTO")).unwrap();
        answer(&host, "PlainThingDto", &["label", "count"], true);
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        let (target, text) = created(&action);
        assert_eq!(target, "/project/src/main/java/shop/PlainThingDto.java");
        assert!(text.contains("public record PlainThingDto(") && text.contains("String label") && text.contains("int count"), "{text}");
        for refused in [
            plain.replace("PlainThing {", "PlainThing extends Base {"),
            plain.replace("PlainThing {", "PlainThing<T> {"),
            plain.replace("PlainThing {", "PlainThing implements Marker {"),
            plain.replace("}\n", "    static class Inner {}\n}\n"),
            plain.replace("public class", "public interface"),
            plain.replace("package shop;\n\n", ""),
            "package shop;\n\npublic class A {\n int x;\n}\nclass B {\n int y;\n}\n".to_owned(),
        ] {
            let (_host, mut plugin, doc) = setup(path, &refused, "java");
            assert!(!plugin.file_actions(&doc, Range::default()).iter().any(|a| a.title.starts_with("Create scalar DTO") || a.title == CLASS_TITLE), "{refused}");
        }
    }

    fn java_record_offered(build: Option<(&str, &str)>) -> bool {
        project_facts::invalidate_all();
        let (host, mut plugin, doc) = setup(CPATH, JAVA, "java");
        if let Some((name, text)) = build {
            host.add_file(name, text);
        }
        plugin.file_actions(&doc, Range::default()).iter().any(|a| a.title.starts_with("Create scalar DTO"))
    }

    #[test]
    fn record_dto_is_offered_only_when_the_java_level_is_not_proven_below_16() {
        assert!(java_record_offered(None));
        assert!(java_record_offered(Some(("pom.xml", "<properties><java.version>17</java.version></properties>"))));
        assert!(java_record_offered(Some(("pom.xml", "<properties><java.version>16</java.version></properties>"))));
        assert!(java_record_offered(Some(("build.gradle", "sourceCompatibility = JavaVersion.VERSION_21"))));
        assert!(java_record_offered(Some(("pom.xml", "<parent><artifactId>spring-boot-starter-parent</artifactId><version>3.3.0</version></parent>"))));
        assert!(!java_record_offered(Some(("pom.xml", "<properties><java.version>11</java.version></properties>"))));
        assert!(!java_record_offered(Some(("pom.xml", "<properties><maven.compiler.source>1.8</maven.compiler.source></properties>"))));
        assert!(!java_record_offered(Some(("pom.xml", "<parent><artifactId>spring-boot-starter-parent</artifactId><version>2.7.18</version></parent><properties><java.version>11</java.version></properties>"))));
        assert!(!java_record_offered(Some(("build.gradle", "java { toolchain { languageVersion = JavaLanguageVersion.of(11) } }"))));
        assert!(java_record_offered(Some(("build.gradle", "sourceCompatibility = JavaVersion.toVersion(libs.versions.jdk.get())"))));
    }

    #[test]
    fn proven_java_11_hides_record_forms_everywhere_but_keeps_other_forms() {
        let pojo = "package shop;\n\npublic class Thing {\n    private String label;\n    public String getLabel() {\n        return label;\n    }\n}\n";
        let order = JAVA;
        let titles = |source: &str, path: &str, pom: &str| {
            project_facts::invalidate_all();
            let (host, mut plugin, doc) = setup(path, source, "java");
            host.add_file("pom.xml", pom);
            host.add_file("src/main/java/shop/Other.java", "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Other {\n @Id Long id;\n}\n");
            plugin.file_actions(&doc, Range::default()).into_iter().map(|a| a.title).collect::<Vec<_>>()
        };
        let java11 = "<properties><java.version>11</java.version></properties>";
        let java17 = "<properties><java.version>17</java.version></properties>";
        let has = |titles: &[String], start: &str| titles.iter().any(|t| t.starts_with(start));
        let old = titles(order, CPATH, java11);
        assert!(!has(&old, "Create scalar DTO") && !has(&old, "Create flattened DTO") && has(&old, "Create projection") && has(&old, "Create projection with nested") && has(&old, CLASS_TITLE), "{old:?}");
        let new = titles(order, CPATH, java17);
        assert!(has(&new, "Create scalar DTO") && has(&new, "Create flattened DTO"), "{new:?}");
        let convert_path = "/project/src/main/java/shop/Thing.java";
        assert!(!has(&titles(pojo, convert_path, java11), dto_convert_action::TO_RECORD_TITLE));
        assert!(has(&titles(pojo, convert_path, java17), dto_convert_action::TO_RECORD_TITLE));
        assert!(has(&titles(pojo, convert_path, "<project/>"), dto_convert_action::TO_RECORD_TITLE));
    }

    #[test]
    fn projection_record_form_is_not_a_choice_when_java_is_below_16() {
        project_facts::invalidate_all();
        let (host, mut plugin, doc) = setup(CPATH, JAVA, "java");
        host.add_file("pom.xml", "<properties><java.version>11</java.version></properties>");
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == projection_action::TITLE).unwrap();
        host.answer_next_prompt(Some(&["record"]));
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Invalid(_))));
    }

    #[test]
    fn jackson_is_refused_unless_provable_and_snake_case_names_are_explicit() {
        let source = PLAIN.replace("String title;", "String titleText;\n String httpURLValue;\n String plain;");
        let answers: [&[&str]; 5] = [&["pojo"], &["CustomerDto"], &["id", "titleText", "httpURLValue", "plain"], &[], &["snake-case-ignore-unknown"]];
        let error = format!("{:?}", run_class(&source, None, &answers, true).1.err().unwrap());
        assert!(error.contains("Jackson is not provably"), "{error}");
        let text = text_of(run_class(&source, Some(("pom.xml", POM_JACKSON)), &answers, true).1);
        for expected in [
            "import com.fasterxml.jackson.annotation.JsonIgnoreProperties;",
            "import com.fasterxml.jackson.annotation.JsonProperty;",
            "@JsonIgnoreProperties(ignoreUnknown = true)\npublic class CustomerDto",
            "@JsonProperty(\"title_text\")\n    private String titleText;",
            "@JsonProperty(\"http_url_value\")\n    private String httpURLValue;",
            "\n    private String plain;",
        ] {
            assert!(text.contains(expected), "{expected}\n{text}");
        }
        assert!(!text.contains("@JsonProperty(\"id\")") && !text.contains("@JsonProperty(\"plain\")"), "{text}");
        let ignore_only: [&[&str]; 5] = [&["pojo"], &["CustomerDto"], &["id"], &[], &["ignore-unknown"]];
        let gradle = ("build.gradle", "implementation 'com.fasterxml.jackson.core:jackson-annotations:2.17.0'");
        let text = text_of(run_class(&source, Some(gradle), &ignore_only, true).1);
        assert!(text.contains("@JsonIgnoreProperties(ignoreUnknown = true)") && !text.contains("JsonProperty"), "{text}");
        let clash = source.replace("String plain;", "String title_text;");
        let clash_answers: [&[&str]; 5] = [&["pojo"], &["CustomerDto"], &["titleText", "title_text"], &[], &["snake-case"]];
        assert!(run_class(&clash, Some(("pom.xml", POM_JACKSON)), &clash_answers, true).1.is_err());
    }

    #[test]
    fn snake_case_conversion_splits_words_digits_and_acronyms() {
        for (from, to) in [("firstName", "first_name"), ("address2Line", "address2_line"), ("httpURLValue", "http_url_value"), ("id", "id"), ("userID", "user_id")] {
            assert_eq!(snake(from), to);
        }
    }

    #[test]
    fn validation_transfer_copies_only_literal_supported_constraints_in_the_matching_namespace() {
        let answers: [&[&str]; 6] = [&["pojo"], &["CustomerDto"], &["id", "title", "code", "mail", "rank", "seen"], &[], &["none"], &["copy"]];
        let text = text_of(run_class(VALID, Some(("pom.xml", POM_VALIDATION)), &answers, true).1);
        for expected in [
            "import jakarta.validation.constraints.Email;",
            "@NotBlank\n    @Size(min = 2, max = 40, message = \"too, long\")\n    private String title;",
            "@Pattern(regexp = \"[a-z]+,\\\\d\")\n    private String code;",
            "@Email\n    private String mail;",
            "@Min(value = 1)\n    @Max(value = 9L)\n    private Integer rank;",
            "@NotNull\n    private Boolean seen;",
        ] {
            assert!(text.contains(expected), "{expected}\n{text}");
        }
        let declined: [&[&str]; 6] = [&["pojo"], &["CustomerDto"], &["title"], &[], &["none"], &["none"]];
        assert!(!text_of(run_class(VALID, Some(("pom.xml", POM_VALIDATION)), &declined, true).1).contains("@NotBlank"));
        let javax = VALID.replace("jakarta.", "javax.");
        let text = text_of(run_class(&javax, Some(("pom.xml", POM_VALIDATION)), &answers, true).1);
        assert!(text.contains("import javax.validation.constraints.NotBlank;") && !text.contains("jakarta"), "{text}");
        let none: [&[&str]; 4] = [&["pojo"], &["CustomerDto"], &["id"], &[]];
        let mut prompts = Vec::from(none);
        prompts.push(&["none"]);
        assert!(run_class(PLAIN, None, &prompts[..], true).1.is_ok());
    }

    #[test]
    fn validation_transfer_refuses_non_literal_unprovable_and_qualified_spellings() {
        for (source, expect) in [
            (VALID.replace("max = 40", "max = LIMIT"), "non-literal"),
            (VALID.replace("message = \"too, long\"", "message = \"a\" + \"b\""), "non-literal"),
            (VALID.replace("@Pattern(regexp = \"[a-z]+,\\\\d\")", "@Pattern(regexp = \"x\", flags = Pattern.Flag.CASE_INSENSITIVE)"), "non-literal"),
            (VALID.replace("import jakarta.validation.constraints.Pattern;\n", ""), "Pattern"),
            (VALID.replace("import jakarta.validation.constraints.NotNull;\n", "import org.jetbrains.annotations.NotNull;\n"), "NotNull"),
            (VALID.replace("@Email", "@jakarta.validation.constraints.Email"), "fully qualified"),
        ] {
            let all: [&[&str]; 6] = [&["pojo"], &["CustomerDto"], &["title", "code", "mail", "seen"], &[], &["none"], &["copy"]];
            let error = format!("{:?}", run_class(&source, Some(("pom.xml", POM_VALIDATION)), &all, true).1.err().unwrap());
            assert!(error.contains(expect) || expect == "non-literal" && error.contains("non-literal"), "{expect}: {error}");
        }
    }

    #[test]
    fn cancellations_forged_answers_collisions_and_stale_state_leave_no_output() {
        let valid: [&[&str]; 5] = [&["pojo"], &["CustomerDto"], &["id"], &[], &["none"]];
        for stage in 0..5 {
            let (_, result) = run_class(PLAIN, None, &valid[..stage], false);
            assert!(matches!(result, Err(Error::Cancelled)), "{stage}");
        }
        let bad: [(&[&str], usize); 5] = [(&["rubbish"], 0), (&["Customer"], 1), (&["bogus"], 2), (&["id", "id"], 2), (&["x"], 3)];
        for (value, at) in bad {
            let mut answers: Vec<&[&str]> = valid.to_vec();
            answers[at] = value;
            assert!(run_class(PLAIN, None, &answers, true).1.is_err(), "{value:?}");
        }
        let mut forged: Vec<&[&str]> = valid.to_vec();
        forged[4] = &["sideways"];
        assert!(run_class(PLAIN, None, &forged, true).1.is_err());
        let (host, mut plugin, doc) = setup(CPATH, PLAIN, "java");
        host.add_file("src/main/java/shop/CustomerDto.java", "package shop; class CustomerDto {}");
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == CLASS_TITLE).unwrap();
        for answer in &valid[..4] {
            host.answer_next_prompt(Some(answer));
        }
        host.answer_next_prompt(Some(&[]));
        assert!(plugin.resolve_file_action(&doc, pending.clone()).is_err());
        host.type_in_editor("// changed");
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Stale)));
    }

    #[test]
    fn crlf_entities_produce_crlf_only_dto_classes_and_kotlin_is_not_offered() {
        let answers: [&[&str]; 5] = [&["pojo"], &["CustomerDto"], &["id"], &["all-args"], &["none"]];
        let text = text_of(run_class(&PLAIN.replace('\n', "\r\n"), None, &answers, true).1);
        assert!(text.contains("\r\n") && !text.replace("\r\n", "").contains('\n'), "{text:?}");
        let kotlin = "package shop\nimport jakarta.persistence.*\n@Entity\nclass Customer(@Id var id: Long? = null) {}\n";
        let (_host, mut plugin, doc) = setup("/project/src/shop/Customer.kt", kotlin, "kotlin");
        assert!(!plugin.file_actions(&doc, Range::default()).iter().any(|a| a.title == CLASS_TITLE));
    }

    #[test]
    fn bean_unsafe_names_are_not_offered_for_class_forms() {
        let source = PLAIN.replace("byte[] blob;", "boolean isActive;\n int aB;");
        let (_host, _plugin, doc) = setup(CPATH, &source, "java");
        let offered: Vec<_> = class_fields(&source_class(&doc)).into_iter().map(|(n, _)| n).collect();
        assert_eq!(offered, ["id", "title", "flag", "seen"]);
    }

    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_VALIDATION_API; Jackson jar is taken from JACKSON_ANNOTATIONS_JAR or ~/.m2; Lombok forms are not compiled (no annotation-processing jar in ~/.m2)"]
    fn generated_dto_classes_compile_with_validation_and_jackson() {
        let validation = std::env::var("JPA_JAKARTA_VALIDATION_API").expect("jakarta validation api jar");
        let jackson = std::env::var("JACKSON_ANNOTATIONS_JAR")
            .unwrap_or_else(|_| format!("{}/.m2/repository/com/fasterxml/jackson/core/jackson-annotations/2.21/jackson-annotations-2.21.jar", std::env::var("HOME").unwrap()));
        assert!(std::path::Path::new(&jackson).exists(), "jackson-annotations jar missing: {jackson}");
        let valid_answers: [&[&str]; 5] = [&["pojo"], &["CustomerDto"], &["id", "title", "code", "mail", "rank", "seen"], &["no-args", "all-args", "equals-hash-code", "to-string"], &["snake-case-ignore-unknown"]];
        let source = VALID.replace("@Email String mail;", "@Email String mailAddress;\n byte[] blob;");
        let mut cases = Vec::new();
        for form in ["pojo", "fluent"] {
            let mut answers = valid_answers.to_vec();
            answers[0] = if form == "pojo" { &["pojo"] } else { &["fluent"] };
            answers[2] = &["id", "title", "code", "mailAddress", "rank", "seen", "blob"];
            answers.insert(5, &["copy"]);
            cases.push(text_of(run_class(&source, Some(("pom.xml", POM_JACKSON)), &answers, true).1));
        }
        for (at, text) in cases.iter().enumerate() {
            let dir = std::env::temp_dir().join(format!("jpa-dto-class-{}-{at}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let file = dir.join("CustomerDto.java");
            std::fs::write(&file, text).unwrap();
            let result = std::process::Command::new("javac")
                .args(["--release", "17", "-Xlint:all", "-classpath", &format!("{validation}:{jackson}"), "-d"])
                .arg(&dir)
                .arg(&file)
                .output()
                .expect("javac installed");
            assert!(result.status.success(), "{}\n{text}", String::from_utf8_lossy(&result.stderr));
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
