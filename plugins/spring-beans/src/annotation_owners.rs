//! Written source annotation ownership; no meta-annotation or classpath inference.
use spring_text::java::{self, Annotation, JavaFile};
use std::collections::{BTreeMap, BTreeSet};
struct Import {
    path: String,
    is_static: bool,
    aliased: bool,
}
pub(crate) struct Owners {
    names: BTreeSet<String>,
    imports: BTreeMap<String, Vec<Import>>,
    package: String,
    wildcards: bool,
    unknown_imports: bool,
}
enum Alias<'a> {
    None,
    Name(&'a str),
    Unknown,
}
fn trivia(text: &str, mut at: usize) -> Option<usize> {
    loop {
        while let Some(c) = text.get(at..)?.chars().next().filter(|c| c.is_whitespace()) {
            at += c.len_utf8();
        }
        let tail = text.get(at..)?;
        if tail.starts_with("//") {
            at += tail.find('\n').unwrap_or(tail.len());
            continue;
        }
        if tail.starts_with("/*") {
            at += 2;
            let mut depth = 1;
            while depth > 0 {
                let tail = text.get(at..)?;
                if tail.starts_with("/*") {
                    depth += 1;
                    at += 2;
                } else if tail.starts_with("*/") {
                    depth -= 1;
                    at += 2;
                } else {
                    at += tail.chars().next()?.len_utf8();
                }
            }
            continue;
        }
        return Some(at);
    }
}
fn word(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '$') || !c.is_ascii() && !c.is_whitespace()
}
fn alias(text: &str, start: usize) -> Alias<'_> {
    let Some(at) = trivia(text, start) else {
        return Alias::Unknown;
    };
    let tail = &text[at..];
    if !tail.starts_with("as")
        || tail
            .get(2..)
            .and_then(|s| s.chars().next())
            .is_some_and(word)
    {
        return Alias::None;
    }
    let Some(at) = trivia(text, at + 2) else {
        return Alias::Unknown;
    };
    let tail = &text[at..];
    if let Some(escaped) = tail.strip_prefix('`') {
        let Some(end) = escaped.find('`') else {
            return Alias::Unknown;
        };
        let name = &escaped[..end];
        return if name.is_empty() || name.contains(['\r', '\n']) {
            Alias::Unknown
        } else {
            Alias::Name(name)
        };
    }
    let end = tail
        .char_indices()
        .find(|(_, c)| !word(*c))
        .map_or(tail.len(), |(i, _)| i);
    let name = &tail[..end];
    if name.is_empty() || name.as_bytes()[0].is_ascii_digit() {
        Alias::Unknown
    } else {
        Alias::Name(name)
    }
}
fn generic_names(code: &str, names: &mut BTreeSet<String>) {
    let tokens = java::tokens(code);
    let mut at = 0;
    loop {
        let Some(token) = tokens.get(at) else {
            return;
        };
        if token.s == "@" {
            at = java::annotation(code, &tokens, at, &mut Vec::new());
        } else if matches!(
            token.s,
            "public"
                | "private"
                | "protected"
                | "static"
                | "final"
                | "abstract"
                | "synchronized"
                | "native"
                | "strictfp"
                | "default"
                | "override"
                | "internal"
                | "suspend"
                | "inline"
                | "operator"
                | "infix"
                | "tailrec"
                | "open"
                | "fun"
        ) {
            at += 1;
        } else {
            break;
        }
    }
    if tokens.get(at).is_none_or(|t| t.s != "<") {
        return;
    }
    let mut depth = 1usize;
    let mut want_name = true;
    at += 1;
    while let Some(token) = tokens.get(at) {
        if token.s == "@" {
            at = java::annotation(code, &tokens, at, &mut Vec::new());
            continue;
        }
        match token.s {
            "<" => depth += 1,
            ">" if at
                .checked_sub(1)
                .and_then(|i| tokens.get(i))
                .is_none_or(|t| t.s != "-") =>
            {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            "," if depth == 1 => want_name = true,
            "in" | "out" | "reified" if depth == 1 && want_name => {}
            _ if depth == 1 && want_name && token.is_ident() => {
                names.insert(token.s.into());
                want_name = false;
            }
            _ => {}
        }
        at += 1;
    }
}
fn class_header_end(text: &str, tokens: &[java::Tok<'_>], ty: &java::TypeDecl) -> usize {
    let mut at = tokens.partition_point(|t| t.at < ty.at + ty.name.len());
    let (mut angle, mut paren, mut bracket) = (0usize, 0usize, 0usize);
    while let Some(token) = tokens.get(at).filter(|t| t.at < ty.end) {
        if token.s == "@" {
            at = java::annotation(text, tokens, at, &mut Vec::new());
            continue;
        }
        match token.s {
            "{" if angle == 0 && paren == 0 && bracket == 0 => return token.at,
            "class" | "interface" | "object" if angle == 0 && paren == 0 && bracket == 0 => {
                return token.at;
            }
            "(" => paren += 1,
            ")" => paren = paren.saturating_sub(1),
            "[" => bracket += 1,
            "]" => bracket = bracket.saturating_sub(1),
            "<" if paren == 0 => angle += 1,
            ">" if paren == 0
                && at
                    .checked_sub(1)
                    .and_then(|i| tokens.get(i))
                    .is_none_or(|t| t.s != "-") =>
            {
                angle = angle.saturating_sub(1)
            }
            _ => {}
        }
        at += 1;
    }
    ty.end
}
impl Owners {
    pub(crate) fn new(file: &JavaFile, text: &str, kotlin: bool) -> Self {
        let mut names: BTreeSet<_> = file.types.iter().map(|t| t.name.clone()).collect();
        let source_tokens = java::tokens(text);
        for ty in &file.types {
            let end = class_header_end(text, &source_tokens, ty);
            generic_names(
                text.get(ty.at + ty.name.len()..end).unwrap_or(""),
                &mut names,
            );
            for method in ty.members.iter().filter(|m| {
                matches!(
                    m.kind,
                    java::MemberKind::Method | java::MemberKind::Constructor
                )
            }) {
                let end = method
                    .ty
                    .as_ref()
                    .map_or(method.at, |ty| ty.at.min(method.at));
                generic_names(text.get(method.start..end).unwrap_or(""), &mut names);
            }
        }
        if kotlin {
            for pair in source_tokens.windows(2) {
                if pair[0].s == "typealias" && pair[1].is_ident() {
                    names.insert(pair[1].s.into());
                }
            }
        }
        let mut imports: BTreeMap<String, Vec<Import>> = BTreeMap::new();
        let mut unknown_imports = false;
        for import in &file.imports {
            let alias = if kotlin {
                alias(text, import.end)
            } else {
                Alias::None
            };
            let simple = import
                .path
                .rsplit('.')
                .next()
                .unwrap_or("")
                .trim_matches('`');
            let (name, aliased) = match alias {
                Alias::None => (simple, false),
                Alias::Name(name) => (name, true),
                Alias::Unknown => {
                    unknown_imports = true;
                    (simple, true)
                }
            };
            imports.entry(name.into()).or_default().push(Import {
                path: import.path.clone(),
                is_static: import.is_static,
                aliased,
            });
        }
        Self {
            names,
            imports,
            package: file.package.clone(),
            wildcards: file.imports.iter().any(|i| i.path.ends_with(".*")),
            unknown_imports,
        }
    }
    pub(crate) fn known(&self, text: &str, a: &Annotation, expected: &str) -> bool {
        if self.unknown_imports {
            return false;
        }
        let Some(tail) = text.get(a.at..) else {
            return false;
        };
        // Kotlin use-site annotations have ':' as the scanner's synthetic @.
        let start = a.at + usize::from(tail.starts_with('@') || tail.starts_with(':'));
        let Some(raw) = text.get(start..a.name_at + a.name.len()) else {
            return false;
        };
        let written: String = java::tokens(raw).iter().map(|t| t.s).collect();
        let root = expected.split('.').next().unwrap_or("");
        if written == expected {
            return !self.wildcards
                && !self.names.contains(root)
                && !self.imports.contains_key(root);
        }
        let Some((package, short)) = expected.rsplit_once('.') else {
            return false;
        };
        if written != short || self.names.contains(short) {
            return false;
        }
        let Some(imports) = self.imports.get(short) else {
            return !self.wildcards && self.package == package;
        };
        imports
            .iter()
            .any(|i| i.path == expected && !i.is_static && !i.aliased)
            && !imports
                .iter()
                .any(|i| i.path != expected || i.is_static || i.aliased)
    }
    pub(crate) fn keep_definition(&self, text: &str, a: &Annotation) -> bool {
        if a.name == "Inject" {
            return self.known(text,a,"javax.inject.Inject") || self.known(text,a,"jakarta.inject.Inject");
        }
        if a.name == "Named" {
            return self.known(text,a,"javax.inject.Named") || self.known(text,a,"jakarta.inject.Named");
        }
        if a.name == "Resource" {
            return self.known(text,a,"javax.annotation.Resource") || self.known(text,a,"jakarta.annotation.Resource");
        }
        let expected = match a.name.as_str() {
            "Bean" => "org.springframework.context.annotation.Bean",
            "Primary" => "org.springframework.context.annotation.Primary",
            "Profile" => "org.springframework.context.annotation.Profile",
            "Qualifier" => "org.springframework.beans.factory.annotation.Qualifier",
            "Autowired" => "org.springframework.beans.factory.annotation.Autowired",
            "Value" => "org.springframework.beans.factory.annotation.Value",
            "Component" => "org.springframework.stereotype.Component",
            "Service" => "org.springframework.stereotype.Service",
            "Repository" => "org.springframework.stereotype.Repository",
            "Controller" => "org.springframework.stereotype.Controller",
            "RestController" => "org.springframework.web.bind.annotation.RestController",
            "ControllerAdvice" => "org.springframework.web.bind.annotation.ControllerAdvice",
            "RestControllerAdvice" => {
                "org.springframework.web.bind.annotation.RestControllerAdvice"
            }
            "Configuration" => "org.springframework.context.annotation.Configuration",
            "SpringBootApplication" => {
                "org.springframework.boot.autoconfigure.SpringBootApplication"
            }
            "Endpoint" => "org.springframework.ws.server.endpoint.annotation.Endpoint",
            "ShellComponent" => "org.springframework.shell.standard.ShellComponent",
            "TestConfiguration" => "org.springframework.boot.test.context.TestConfiguration",
            _ => return !java::STEREOTYPES.contains(&a.name.as_str()),
        };
        self.known(text, a, expected)
    }
    pub(crate) fn filter_definitions(&self, file: &mut JavaFile, text: &str) {
        for ty in &mut file.types {
            ty.annotations.retain(|a| self.keep_definition(text, a));
            for param in &mut ty.components {
                param.annotations.retain(|a| self.keep_definition(text, a));
            }
            for member in &mut ty.members {
                member.annotations.retain(|a| self.keep_definition(text, a));
                for param in &mut member.params {
                    param.annotations.retain(|a| self.keep_definition(text, a));
                }
            }
        }
    }
}
