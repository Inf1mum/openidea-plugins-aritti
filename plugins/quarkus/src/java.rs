//! Naming rules for MicroProfile Config property and Quarkus config-mapping keys.

use openidea_plugin::providers::{CompletionItem, CompletionKind};
use spring_text::java::{self, Annotation, JavaFile, TypeRef};
use spring_text::metadata::{Lookup, Meta, Prop};
use spring_text::range;
use std::collections::BTreeSet;

/// `maxPoolSize` -> `max-pool-size`; a run of capitals is one hump (`URLPath` -> `url-path`).
pub fn hyphenate(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && c.is_uppercase() {
            let after_lower = chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit();
            let ends_run = chars[i - 1].is_uppercase() && chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if after_lower || ends_run {
                out.push('-');
            }
        }
        out.extend(c.to_lowercase());
    }
    out
}

#[derive(Clone, Copy)]
enum Naming {
    Kebab,
    Snake,
    Verbatim,
}

impl Naming {
    fn of(annotation: &Annotation, file: &JavaFile) -> Option<Self> {
        let Some(argument) = annotation.arg("namingStrategy") else { return Some(Self::Kebab); };
        let expression: String = java::tokens(&argument.text).iter().map(|token| token.s).collect();
        let (owner, value) = expression.rsplit_once('.').unwrap_or(("", expression.as_str()));
        let qualified = "io.smallrye.config.ConfigMapping.NamingStrategy";
        let known = owner == qualified
            || owner == "ConfigMapping.NamingStrategy" && source_type_owned(file, CONFIG_MAPPING)
            || owner == "NamingStrategy" && source_type_owned(file, qualified)
            || owner.is_empty() && !file.types.iter().flat_map(|ty| &ty.members).any(|member| member.name == value)
                && file.imports.iter().filter(|import| import.is_static && import.path.rsplit('.').next() == Some(value)).count() == 1
                && file.imports.iter().any(|import| import.is_static && import.path == format!("{qualified}.{value}"));
        if !known { return None; }
        match value { "VERBATIM" => Some(Self::Verbatim), "SNAKE_CASE" => Some(Self::Snake), "KEBAB_CASE" => Some(Self::Kebab), _ => None }
    }

    fn convert(self, name: &str) -> String {
        match self {
            Self::Kebab => hyphenate(name),
            Self::Snake => hyphenate(name).replace('-', "_"),
            Self::Verbatim => name.to_owned(),
        }
    }
}

fn unwrapped<'a>(file: &JavaFile, ty: &'a TypeRef) -> &'a TypeRef {
    match ty.args.as_slice() {
        [inner] if ty.name == "Optional" && (ty.text.split('<').next().map(str::trim) == Some("java.util.Optional")
            || ty.text.split('<').next().map(str::trim) == Some("Optional") && source_type_owned(file, "java.util.Optional")) => inner,
        _ => ty,
    }
}

fn first_string(annotation: Option<&Annotation>, names: &[&str]) -> Option<String> {
    let annotation = annotation?;
    let argument = names.iter().find_map(|name| annotation.arg(name))?;
    let tokens = java::tokens(&argument.text);
    let [token] = tokens.as_slice() else { return None; };
    if !token.s.starts_with('"') || !token.s.ends_with('"') || token.s.starts_with("\"\"\"") || token.s.contains('\\') { return None; }
    let [literal] = argument.strings.as_slice() else { return None; };
    Some(literal.value.clone())
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() { name.to_owned() } else { format!("{prefix}.{name}") }
}

pub(crate) fn source_type_owned(file: &JavaFile, qualified: &str) -> bool {
    let simple = qualified.rsplit('.').next().unwrap_or(qualified);
    if file.types.iter().any(|ty| ty.name == simple) { return false; }
    let imports: Vec<_> = file.imports.iter().filter(|import| !import.is_static && import.path.rsplit('.').next() == Some(simple)).collect();
    match imports.as_slice() { [import] => import.path == qualified,
        [] => qualified.rsplit_once('.').is_some_and(|(package, _)| package == file.package), _ => false }
}
fn annotation_written(text: &str, annotation: &Annotation) -> Option<String> {
    Some(java::tokens(text.get(annotation.at+1..annotation.name_at+annotation.name.len())?).iter().map(|token| token.s).collect())
}
pub(crate) fn annotation_owned(file: &JavaFile, text: &str, annotation: &Annotation, qualified: &str) -> bool {
    let Some(written) = annotation_written(text, annotation) else { return false; };
    written == qualified || written == qualified.rsplit('.').next().unwrap_or(qualified) && source_type_owned(file, qualified)
}
fn annotation_foreign(file: &JavaFile, text: &str, annotation: &Annotation, qualified: &str) -> bool {
    let Some(written) = annotation_written(text, annotation) else { return false; };
    let simple = qualified.rsplit('.').next().unwrap_or(qualified);
    if written.contains('.') { return written != qualified; }
    if written != simple { return false; }
    let imports: Vec<_> = file.imports.iter().filter(|import| !import.is_static && import.path.rsplit('.').next() == Some(simple)).collect();
    match imports.as_slice() { [import] => import.path != qualified, [] => file.types.iter().any(|ty| ty.name == simple), _ => false }
}
fn modifier_unknown(file: &JavaFile, text: &str, annotation: &Annotation) -> bool {
    let qualified = match annotation.name.as_str() { "WithName" => "io.smallrye.config.WithName", "WithDefault" => "io.smallrye.config.WithDefault",
        "WithParentName" => "io.smallrye.config.WithParentName", _ => return false };
    if annotation_owned(file, text, annotation, qualified) {
        if annotation.name == "WithParentName" { !annotation.args.is_empty() } else { first_string(Some(annotation), &["value"]).is_none() }
    } else { !annotation_foreign(file, text, annotation, qualified) }
}

const CONFIG_PROPERTY: &str = "org.eclipse.microprofile.config.inject.ConfigProperty";
const CONFIG_MAPPING: &str = "io.smallrye.config.ConfigMapping";

/// The configuration keys a Java file declares, `None` when it declares none.
pub fn config_meta(_path: &str, text: &str) -> Option<Meta> {
    if !text.contains("ConfigProperty") && !text.contains("ConfigMapping") {
        return None;
    }
    let file = java::parse(text);
    let mut props = Vec::new();
    for (i, t) in file.types.iter().enumerate() {
        for m in &t.members {
            if let (Some(a), Some(ty)) = (m.annotation("ConfigProperty").filter(|annotation| annotation_owned(&file, text, annotation, CONFIG_PROPERTY)), &m.ty) {
                injection(&file, a, ty, &mut props);
            }
            for p in &m.params {
                if let Some(a) = p.annotation("ConfigProperty").filter(|annotation| annotation_owned(&file, text, annotation, CONFIG_PROPERTY)) {
                    injection(&file, a, &p.ty, &mut props);
                }
            }
        }
        for c in &t.components {
            if let Some(a) = c.annotation("ConfigProperty").filter(|annotation| annotation_owned(&file, text, annotation, CONFIG_PROPERTY)) {
                injection(&file, a, &c.ty, &mut props);
            }
        }
        if let Some(mapping) = t.annotation("ConfigMapping").filter(|annotation| annotation_owned(&file, text, annotation, CONFIG_MAPPING)) {
            let prefix = first_string(Some(mapping), &["prefix", "value"]);
            if mapping.arg("prefix").or_else(|| mapping.arg("value")).is_some() && prefix.is_none() { continue; }
            let prefix = prefix.unwrap_or_default();
            let Some(naming) = Naming::of(mapping, &file) else { continue; };
            bind(&file, text, i, &prefix, naming, 0, &mut props, &mut Vec::new());
        }
    }
    (!props.is_empty()).then_some(Meta { props, hints: Vec::new() })
}

/// What the project index keeps of a Java file.
#[derive(Clone)]
pub struct JavaInfo {
    pub locations: Vec<crate::navigation::Target>,
    pub package: String,
    pub complete: bool,
    pub meta: Option<Meta>,
    /// Literal owned REST interfaces and their explicit registration annotation.
    pub interfaces: Vec<(String, bool)>,
    pub jaxrs: crate::jaxrs::Facts,
}

// Resolve only an unqualified, literal same-file type in its lexical owners.
// Inheritance, imported/external groups and qualified-name substitution stay unknown.
fn nested_group(file: &JavaFile, from: usize, ty: &TypeRef) -> Result<Option<usize>, ()> {
    if !file.types.iter().any(|candidate| candidate.name == ty.name) { return Ok(None); }
    if ty.text.trim() != ty.name || !ty.args.is_empty() { return Err(()); }
    let mut owner = Some(from);
    for _ in 0..64 {
        let matches: Vec<_> = file.types.iter().enumerate().filter(|(_, candidate)| candidate.name == ty.name && candidate.outer == owner).collect();
        if matches.len() > 1 { return Err(()); }
        if let Some((index, candidate)) = matches.first() {
            if owner.is_none() && file.imports.iter().any(|import| !import.is_static && import.path.rsplit('.').next() == Some(ty.name.as_str())
                && import.path != format!("{}.{}", file.package, ty.name)) { return Err(()); }
            return Ok((candidate.kind == "interface").then_some(*index));
        }
        match owner { Some(index) => owner = file.types.get(index).ok_or(())?.outer, None => return Err(()) }
    }
    Err(())
}
fn scalar_model_complete(file: &JavaFile, text: &str) -> bool {
    let literal = |annotation: &Annotation, key: &str| annotation.arg(key).is_none() || first_string(Some(annotation), &[key]).is_some();
    let mut complete = true;
    let mapped = file.types.iter().any(|ty| ty.annotation("ConfigMapping").is_some_and(|a| annotation_owned(file, text, a, CONFIG_MAPPING)));
    for (index, ty) in file.types.iter().enumerate() {
        if mapped && ty.methods().any(|method| method.ty.as_ref().is_some_and(|ty| nested_group(file, index, unwrapped(file, ty)).is_err())) { complete = false; }
        if let Some(mapping) = ty.annotation("ConfigMapping").filter(|a| annotation_owned(file, text, a, CONFIG_MAPPING)) {
            complete &= literal(mapping, "prefix") && literal(mapping, "value") && Naming::of(mapping, file).is_some();
        } else if ty.annotation("ConfigMapping").is_some_and(|a| !annotation_foreign(file, text, a, CONFIG_MAPPING)) { complete = false; }
        let annotations = ty.members.iter().flat_map(|member| member.annotations.iter().chain(member.params.iter().flat_map(|p| &p.annotations)))
            .chain(ty.components.iter().flat_map(|component| &component.annotations));
        for annotation in annotations {
            if annotation.name == "ConfigProperty" {
                if annotation_owned(file, text, annotation, CONFIG_PROPERTY) {
                    complete &= annotation.arg("name").is_some() && literal(annotation, "name") && literal(annotation, "defaultValue");
                } else if !annotation_foreign(file, text, annotation, CONFIG_PROPERTY) { complete = false; }
            }
            if mapped && modifier_unknown(file, text, annotation) { complete = false; }
        }
    }
    complete
}
pub fn java_info(path: &str, text: &str) -> Option<JavaInfo> {
    if text.len() > 128 * 1024 {
        return Some(JavaInfo { package: String::new(), complete: false, locations: Vec::new(), meta: None, interfaces: Vec::new(), jaxrs: Default::default() });
    }
    let file = java::parse(text);
    let mut complete = scalar_model_complete(&file, text);
    let meta = config_meta(path, text);
    let mut interfaces = Vec::new();
    for ty in file.types.iter().filter(|ty| ty.kind == "interface" && ty.outer.is_none()) {
        let path = ty.annotation("Path");
        let registered = ty.annotation("RegisterRestClient");
        let path_owned = path.is_some_and(|a| annotation_owned(&file, text, a, "jakarta.ws.rs.Path") || annotation_owned(&file, text, a, "javax.ws.rs.Path"));
        let registered_owned = registered.is_some_and(|a| annotation_owned(&file, text, a, "org.eclipse.microprofile.rest.client.inject.RegisterRestClient"));
        let path_unknown = path.is_some_and(|a| !path_owned && !annotation_foreign(&file, text, a, "jakarta.ws.rs.Path") && !annotation_foreign(&file, text, a, "javax.ws.rs.Path"));
        let registration_unknown = registered.is_some_and(|a| !registered_owned && !annotation_foreign(&file, text, a, "org.eclipse.microprofile.rest.client.inject.RegisterRestClient"));
        if path_unknown || registration_unknown { complete = false; }
        if (path_owned || registered_owned) && !registration_unknown { interfaces.push((ty.name.clone(), registered_owned)); }
    }
    let jaxrs = crate::jaxrs::facts(&file, text);
    let package = file.package;
    (meta.is_some() || !interfaces.is_empty() || !complete || !jaxrs.is_empty()).then_some(JavaInfo {
        package,
        complete,
        locations: crate::navigation::java_targets(path, text),
        meta,
        interfaces,
        jaxrs,
    })
}

/// A `@ConfigProperty(name = "…")` injection point with a literal name.
pub struct Injection {
    pub key: String,
    pub at: usize,
    pub end: usize,
    pub optional: bool,
    pub has_default: bool,
    pub ty: String,
    /// Where `, defaultValue = "…"` goes.
    pub insert_at: usize,
}

pub fn injections(file: &JavaFile, text: &str) -> Vec<Injection> {
    let mut found = Vec::new();
    for t in &file.types {
        for m in &t.members {
            if let (Some(a), Some(ty)) = (m.annotation("ConfigProperty").filter(|annotation| annotation_owned(&file, text, annotation, CONFIG_PROPERTY)), &m.ty) {
                found.push((a, ty));
            }
            for p in &m.params {
                if let Some(a) = p.annotation("ConfigProperty").filter(|annotation| annotation_owned(&file, text, annotation, CONFIG_PROPERTY)) {
                    found.push((a, &p.ty));
                }
            }
        }
        for c in &t.components {
            if let Some(a) = c.annotation("ConfigProperty").filter(|annotation| annotation_owned(&file, text, annotation, CONFIG_PROPERTY)) {
                found.push((a, &c.ty));
            }
        }
    }
    found
        .into_iter()
        .filter_map(|(a, ty)| {
            let key = first_string(Some(a), &["name"])?;
            let name = a.arg("name")?;
            let [literal] = name.strings.as_slice() else { return None };
            if name.text.matches('"').count() != 2 || !name.text.starts_with('"') || !name.text.ends_with('"') {
                return None;
            }
            let optional = ty.name.starts_with("Optional") || ty.name == "Provider" || ty.name == "Instance";
            Some(Injection {
                key,
                at: literal.at,
                end: literal.end,
                optional,
                has_default: a.arg("defaultValue").is_some(),
                ty: ty.text.clone(),
                insert_at: name.end,
            })
        })
        .collect()
}

pub enum Slot {
    Key,
    Prefix,
}

/// Whether `at` is inside the `name` of `@ConfigProperty(` or the `prefix` of `@ConfigMapping(`: the bytes of
/// the string's contents. Read from the text around the cursor, so it also works in a file that does not parse.
pub fn completion_slot(text: &str, at: usize) -> Option<(Slot, usize, usize)> {
    let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[at..].find('\n').map_or(text.len(), |i| at + i);
    let before = &text[line_start..at];
    if before.matches('"').count() % 2 == 0 {
        return None;
    }
    let open = line_start + before.rfind('"')?;
    let head = text[line_start..open].trim_end().strip_suffix('=')?.trim_end();
    let attribute = head.rsplit(|c: char| !c.is_alphanumeric() && c != '_').next()?;
    let slot = match attribute {
        "name" => Slot::Key,
        "prefix" => Slot::Prefix,
        _ => return None,
    };
    let window = &text[text.floor_char_boundary(open.saturating_sub(400))..open];
    let annotation = window.rfind("@Config")?;
    let tail = &window[annotation..];
    let expected = match slot {
        Slot::Key => "@ConfigProperty(",
        Slot::Prefix => "@ConfigMapping(",
    };
    let opened = tail.starts_with(expected) && tail.matches('(').count() > tail.matches(')').count();
    if !opened || tail.contains([';', '{', '}']) {
        return None;
    }
    let end = text[at..line_end].find('"').map_or(at, |i| at + i);
    Some((slot, open + 1, end))
}

pub fn key_items(text: &str, at: usize, lookup: &Lookup) -> Vec<CompletionItem> {
    let Some((slot, start, end)) = completion_slot(text, at) else { return Vec::new() };
    let file = java::parse(text);
    let tokens = java::tokens(text);
    let Some(index) = tokens.iter().enumerate().rev().find(|(_, token)| token.s == "@" && token.at < start).map(|(index, _)| index) else { return Vec::new(); };
    let mut annotations = Vec::new();
    java::annotation(text, &tokens, index, &mut annotations);
    let (qualified, argument) = match slot { Slot::Key => (CONFIG_PROPERTY, "name"), Slot::Prefix => (CONFIG_MAPPING, "prefix") };
    let literal = tokens.iter().position(|token| token.at + 1 == start && token.s.starts_with('"'));
    let flat_argument = literal.is_some_and(|literal| literal >= 2 && tokens[literal-1].s == "=" && tokens[literal-2].s == argument
        && tokens[index..literal].iter().fold(0i32, |depth, token| depth + i32::from(token.s == "(") - i32::from(token.s == ")")) == 1);
    if !flat_argument || !annotations.first().is_some_and(|annotation| annotation_owned(&file, text, annotation, qualified)) { return Vec::new(); }
    let replace = range(text, start, end);
    let typed = text[start..at].to_lowercase();
    let mut prefixes = BTreeSet::new();
    let mut items = Vec::new();
    for p in lookup.all().filter(|p| p.deprecated.as_ref().is_none_or(|d| !d.removed)) {
        if !p.name.to_lowercase().contains(&typed) {
            continue;
        }
        match slot {
            Slot::Prefix => {
                let mut group = p.name.as_str();
                while let Some((parent, _)) = group.rsplit_once('.') {
                    prefixes.insert(parent.to_owned());
                    group = parent;
                }
            }
            Slot::Key => {
                let documentation =
                    [p.description.clone(), if p.default.is_empty() { String::new() } else { format!("Default: `{}`", p.default) }];
                let mut item = CompletionItem::new(&p.name, CompletionKind::Property);
                item.detail = Some(p.ty.clone()).filter(|t| !t.is_empty());
                item.insert_text = p.name.clone();
                item.replace = Some(replace);
                item.documentation = Some(documentation.join("\n\n").trim().to_owned()).filter(|d| !d.is_empty());
                items.push(item);
            }
        }
    }
    items.extend(prefixes.into_iter().map(|prefix| {
        let mut item = CompletionItem::new(&prefix, CompletionKind::Module);
        item.insert_text = prefix.clone();
        item.replace = Some(replace);
        item
    }));
    items
}

fn injection(file: &JavaFile, annotation: &Annotation, ty: &TypeRef, out: &mut Vec<Prop>) {
    let Some(name) = first_string(Some(annotation), &["name"]) else { return };
    let default = first_string(Some(annotation), &["defaultValue"]);
    if annotation.arg("defaultValue").is_some() && default.is_none() { return; }
    let default = default.unwrap_or_default();
    out.push(Prop { name, ty: unwrapped(file, ty).text.clone(), default, ..Prop::default() });
}

/// The keys of `file.types[i]` under `prefix`: its accessor methods, descending into
/// interfaces of the same file.
// ponytail: super-interfaces and types from other files are not followed; needs a cross-file type index
fn bind(file: &JavaFile, text: &str, i: usize, prefix: &str, naming: Naming, depth: usize, out: &mut Vec<Prop>, spans: &mut Vec<(usize, usize)>) {
    for m in file.types[i].methods() {
        let skipped = m.modifiers.iter().any(|x| x == "static" || x == "default");
        let Some(ty) = &m.ty else { continue };
        if skipped || !m.params.is_empty() || ty.text == "void" {
            continue;
        }
        if m.annotations.iter().any(|a| modifier_unknown(file, text, a)) { continue; }
        let key = if m.annotation("WithParentName").is_some_and(|a| annotation_owned(file, text, a, "io.smallrye.config.WithParentName")) {
            prefix.to_owned()
        } else {
            let name = first_string(m.annotation("WithName").filter(|a| annotation_owned(file, text, a, "io.smallrye.config.WithName")), &["value"]).unwrap_or_else(|| naming.convert(&m.name));
            join(prefix, &name)
        };
        let inner = unwrapped(file, ty);
        let Ok(nested) = nested_group(file, i, inner) else { continue; };
        match nested {
            Some(j) if j != i && depth < 3 => bind(file, text, j, &key, naming, depth + 1, out, spans),
            _ => {
                let default = first_string(m.annotation("WithDefault").filter(|a| annotation_owned(file, text, a, "io.smallrye.config.WithDefault")), &["value"]).unwrap_or_default();
                out.push(Prop { name: key, ty: inner.text.clone(), default, ..Prop::default() });
                spans.push((m.at, m.at + m.name.len()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREETING: &str = r#"package org.acme;
    import org.eclipse.microprofile.config.inject.ConfigProperty;
    import java.util.Optional;
    public class GreetingResource {
        @ConfigProperty(name = "greeting.message", defaultValue = "hello")
        String message;
        @ConfigProperty(name = "greeting.suffix")
        Optional<String> suffix;
        @Inject
        GreetingResource(@ConfigProperty(name = "greeting.count", defaultValue = "3") int count) {}
        @ConfigProperty
        String unnamed;
    }"#;

    const SERVER: &str = r#"package org.acme;
    import io.smallrye.config.ConfigMapping;
    import java.util.Optional;
    import io.smallrye.config.WithName;
    import io.smallrye.config.WithDefault;
    import io.smallrye.config.WithParentName;
    @ConfigMapping(prefix = "server")
    public interface ServerConfig {
        String hostName();
        @WithDefault("8080") int port();
        @WithName("ssl-enabled") boolean ssl();
        Optional<Log> log();
        Map<String, String> labels();
        static ServerConfig none() { return null; }
        default String url() { return hostName(); }
        interface Log {
            @WithDefault("false") boolean enabled();
            @WithParentName Level level();
        }
    }
    @ConfigMapping(prefix = "verbatim", namingStrategy = ConfigMapping.NamingStrategy.VERBATIM)
    interface Verbatim { String maxSize(); }
    @ConfigMapping(prefix = "snake", namingStrategy = ConfigMapping.NamingStrategy.SNAKE_CASE)
    interface Snake { String maxSize(); }
    "#;

    fn props(meta: &Meta) -> Vec<(&str, &str, &str)> {
        meta.props.iter().map(|p| (p.name.as_str(), p.ty.as_str(), p.default.as_str())).collect()
    }

    #[test]
    fn reads_config_property_injection_points() {
        let meta = config_meta("/p/GreetingResource.java", GREETING).unwrap();
        assert_eq!(
            props(&meta),
            [("greeting.message", "String", "hello"), ("greeting.suffix", "String", ""), ("greeting.count", "int", "3")]
        );
        assert_eq!(config_meta("/p/A.java", "class A {}"), None);
    }

    #[test]
    fn maps_config_mapping_interfaces() {
        let meta = config_meta("/p/ServerConfig.java", SERVER).unwrap();
        assert_eq!(
            props(&meta),
            [
                ("server.host-name", "String", ""),
                ("server.port", "int", "8080"),
                ("server.ssl-enabled", "boolean", ""),
                ("server.log.enabled", "boolean", "false"),
                ("server.log", "Level", ""),
                ("server.labels", "Map<String, String>", ""),
                ("verbatim.maxSize", "String", ""),
                ("snake.max_size", "String", ""),
            ]
        );
    }

    #[test]
    fn hyphenates_like_quarkus() {
        assert_eq!(hyphenate("maxPoolSize"), "max-pool-size");
        assert_eq!(hyphenate("URLPath"), "url-path");
        assert_eq!(hyphenate("useSSL"), "use-ssl");
        assert_eq!(hyphenate("host"), "host");
    }

    #[test]
    fn survives_every_prefix() {
        for end in (0..=SERVER.len()).filter(|&e| SERVER.is_char_boundary(e)) {
            config_meta("/p/ServerConfig.java", &SERVER[..end]);
        }
    }
}

pub fn parse_interface_start(text: &str, name: &str) -> Option<usize> {
    let file = java::parse(text);
    let matches: Vec<_> = file.types.iter().filter(|ty| ty.kind == "interface" && ty.outer.is_none()
        && (if file.package.is_empty() { ty.name.clone() } else { format!("{}.{}", file.package, ty.name) }) == name).collect();
    let [target] = matches.as_slice() else { return None; };
    Some(target.start)
}
