//! Self-contained first-statement validation in ordinary controller methods.
//! Rules retain their spelling/comments; moved class references become explicit
//! FQNs. Variables, closures and context-dependent rule expressions are withheld.
use super::{Shape, identifier, ordinary_variable};
use openidea_plugin::providers::Document;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy)]
struct T<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}
fn lex(text: &str) -> Option<Vec<T<'_>>> {
    if text.len() > 128 * 1024
        || !text.starts_with("<?php")
        || text.contains("<<<")
        || text.contains('`')
    {
        return None;
    }
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 5;
    while at < b.len() {
        if b[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if b[at..].starts_with(b"/*") {
            at += text[at + 2..].find("*/")? + 4;
            continue;
        }
        if b[at..].starts_with(b"//") || b[at] == b'#' && b.get(at + 1) != Some(&b'[') {
            at += text[at..].find('\n').unwrap_or(b.len() - at);
            continue;
        }
        if b[at..].starts_with(b"?>") || b[at..].starts_with(b"<?") {
            return None;
        }
        let start = at;
        if matches!(b[at], b'\'' | b'"') {
            let quote = b[at];
            at += 1;
            while at < b.len() && b[at] != quote {
                at += if b[at] == b'\\' { 2 } else { 1 };
            }
            if at >= b.len() {
                return None;
            }
            at += 1;
        } else if b[at].is_ascii_alphabetic() || matches!(b[at], b'_' | b'\\' | b'$') {
            at += 1;
            while at < b.len() && (b[at].is_ascii_alphanumeric() || matches!(b[at], b'_' | b'\\')) {
                at += 1;
            }
        } else if b[at].is_ascii_digit() {
            at += 1;
            while at < b.len() && (b[at].is_ascii_digit() || b[at] == b'.') {
                at += 1;
            }
        } else if ["?->", "...", "->", "=>", "::", "??"]
            .iter()
            .any(|op| b[at..].starts_with(op.as_bytes()))
        {
            at += ["?->", "...", "->", "=>", "::", "??"]
                .iter()
                .find(|op| b[at..].starts_with(op.as_bytes()))?
                .len();
        } else {
            at += text[at..].chars().next()?.len_utf8();
        }
        out.push(T {
            text: text.get(start..at)?,
            start,
            end: at,
        });
        if out.len() > 12000 {
            return None;
        }
    }
    Some(out)
}
fn paired(ts: &[T<'_>], open: usize, depth: usize) -> Option<usize> {
    if depth > 64 {
        return None;
    }
    let close = match ts.get(open)?.text {
        "(" => ")",
        "[" => "]",
        "{" => "}",
        _ => return None,
    };
    let mut at = open + 1;
    while at < ts.len() {
        match ts[at].text {
            "(" | "[" | "{" => at = paired(ts, at, depth + 1)? + 1,
            ")" | "]" | "}" => return (ts[at].text == close).then_some(at),
            _ => at += 1,
        }
    }
    None
}
fn parts(ts: &[T<'_>], from: usize, to: usize) -> Option<Vec<(usize, usize)>> {
    let mut start = from;
    let mut at = from;
    let mut out = Vec::new();
    while at < to {
        match ts[at].text {
            "(" | "[" | "{" => at = paired(ts, at, 0)? + 1,
            "," => {
                if at == start {
                    return None;
                }
                out.push((start, at));
                at += 1;
                start = at;
            }
            _ => at += 1,
        }
    }
    if start < to {
        out.push((start, to));
    }
    Some(out)
}
fn class(name: &str) -> bool {
    !name.is_empty() && name.trim_start_matches('\\').split('\\').all(identifier)
}
fn resolved(name: &str, namespace: &str, imports: &BTreeMap<String, String>) -> Option<String> {
    if !class(name)
        || matches!(
            name.to_ascii_lowercase().as_str(),
            "self" | "static" | "parent"
        )
    {
        return None;
    }
    if let Some(name) = name.strip_prefix('\\') {
        return Some(name.into());
    }
    if let Some(name) = name.strip_prefix("namespace\\") {
        return Some(format!("{namespace}\\{name}"));
    }
    let (first, rest) = name.split_once('\\').map_or((name, ""), |(a, b)| (a, b));
    if let Some(full) = imports.get(&first.to_ascii_lowercase()) {
        Some(if rest.is_empty() {
            full.clone()
        } else {
            format!("{full}\\{rest}")
        })
    } else {
        Some(format!("{namespace}\\{name}"))
    }
}
fn string(token: T<'_>) -> Option<&str> {
    let quote = *token.text.as_bytes().first()?;
    if !matches!(quote, b'\'' | b'"') || token.text.as_bytes().last() != Some(&quote) {
        return None;
    }
    let content = &token.text[1..token.text.len() - 1];
    if quote == b'"' && content.contains('$') {
        return None;
    }
    Some(content)
}
struct Expression<'a> {
    ts: &'a [T<'a>],
    namespace: &'a str,
    imports: &'a BTreeMap<String, String>,
    controller: &'a str,
    edits: Vec<(usize, usize, String)>,
}
impl Expression<'_> {
    fn scalar(&mut self, from: usize, to: usize, depth: usize) -> Option<()> {
        if depth > 12 || from >= to {
            return None;
        }
        let first = self.ts[from];
        if to == from + 1
            && (string(first).is_some()
                || matches!(first.text, "true" | "false" | "null")
                || first.text.parse::<f64>().ok().is_some_and(f64::is_finite))
        {
            return Some(());
        }
        if first.text == "-"
            && to == from + 2
            && self.ts[from + 1]
                .text
                .parse::<f64>()
                .ok()
                .is_some_and(f64::is_finite)
        {
            return Some(());
        }
        if first.text == "[" && paired(self.ts, from, 0)? + 1 == to {
            for (a, b) in parts(self.ts, from + 1, to - 1)? {
                if let Some(arrow) = (a..b).find(|at| self.ts[*at].text == "=>") {
                    self.scalar(a, arrow, depth + 1)?;
                    self.scalar(arrow + 1, b, depth + 1)?;
                } else {
                    self.scalar(a, b, depth + 1)?;
                }
            }
            return Some(());
        }
        let creating = first.text == "new";
        let at = from + usize::from(creating);
        let ty = *self.ts.get(at)?;
        let full = resolved(ty.text, self.namespace, self.imports)?;
        if full == self.controller || full == "Illuminate\\Routing\\Controller" {
            return None;
        }
        self.edits.push((ty.start, ty.end, format!("\\{full}")));
        let mut cursor = at + 1;
        if creating {
            if !["Enum", "Exists", "Unique", "In", "NotIn", "Dimensions"]
                .iter()
                .any(|name| full == format!("Illuminate\\Validation\\Rules\\{name}"))
            {
                return None;
            }
            if cursor < to && self.ts[cursor].text == "(" {
                cursor = self.arguments(cursor, to, depth + 1)?;
            }
        } else {
            if self.ts.get(cursor)?.text != "::" {
                return None;
            }
            cursor += 1;
            let member = *self.ts.get(cursor)?;
            if !identifier(member.text) {
                return None;
            }
            cursor += 1;
            if cursor < to && self.ts[cursor].text == "(" {
                if full != "Illuminate\\Validation\\Rule"
                    || ![
                        "unique",
                        "exists",
                        "in",
                        "notIn",
                        "enum",
                        "dimensions",
                        "array",
                        "file",
                        "imageFile",
                        "date",
                        "email",
                        "numeric",
                        "string",
                    ]
                    .contains(&member.text)
                {
                    return None;
                }
                cursor = self.arguments(cursor, to, depth + 1)?;
            } else {
                return (member.text == "class" && cursor == to).then_some(());
            }
        }
        while cursor < to {
            if self.ts[cursor].text != "->" {
                return None;
            }
            cursor += 1;
            if !identifier(self.ts.get(cursor)?.text) {
                return None;
            }
            cursor += 1;
            cursor = self.arguments(cursor, to, depth + 1)?;
        }
        (cursor == to).then_some(())
    }
    fn arguments(&mut self, open: usize, limit: usize, depth: usize) -> Option<usize> {
        if self.ts.get(open)?.text != "(" {
            return None;
        }
        let end = paired(self.ts, open, 0)?;
        if end >= limit {
            return None;
        }
        for (from, to) in parts(self.ts, open + 1, end)? {
            self.scalar(from, to, depth + 1)?;
        }
        Some(end + 1)
    }
}
fn rules(
    text: &str,
    ts: &[T<'_>],
    open: usize,
    close: usize,
    namespace: &str,
    imports: &BTreeMap<String, String>,
    controller: &str,
) -> Option<String> {
    if ts[close].end - ts[open].start > 16 * 1024 {
        return None;
    }
    let fields = parts(ts, open + 1, close)?;
    if fields.is_empty() || fields.len() > 128 {
        return None;
    }
    let mut keys = BTreeSet::new();
    let mut expression = Expression {
        ts,
        namespace,
        imports,
        controller,
        edits: Vec::new(),
    };
    for (from, to) in fields {
        let name = string(ts[from])?;
        if name.is_empty()
            || name.contains('\\')
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._*-".contains(&b))
            || !keys.insert(name)
            || ts.get(from + 1)?.text != "=>"
        {
            return None;
        }
        expression.scalar(from + 2, to, 0)?;
    }
    let begin = ts[open].start;
    let mut result = text[begin..ts[close].end].to_owned();
    expression
        .edits
        .sort_by_key(|(from, _, _)| std::cmp::Reverse(*from));
    for (from, to, replacement) in expression.edits {
        result.replace_range(from - begin..to - begin, &replacement);
    }
    Some(result)
}

pub(super) fn shapes(doc: &Document) -> Option<Vec<Shape>> {
    if doc.language != "php" || !doc.path.ends_with(".php") || doc.path.ends_with(".blade.php") {
        return None;
    }
    let ts = lex(&doc.text)?;
    let mut at = 0;
    let mut strict = false;
    if ts.get(at)?.text == "declare" {
        let declaration = ["declare", "(", "strict_types", "=", "1", ")", ";"];
        let zero = ["declare", "(", "strict_types", "=", "0", ")", ";"];
        let actual = ts
            .get(at..at + 7)?
            .iter()
            .map(|token| token.text)
            .collect::<Vec<_>>();
        if actual == declaration {
            strict = true;
        } else if actual != zero {
            return None;
        }
        at += 7;
    }
    if ts.get(at)?.text != "namespace" {
        return None;
    }
    at += 1;
    let namespace = ts.get(at)?.text;
    let (prefix, controller_namespace) = namespace.split_once("Http\\Controllers")?;
    if !super::request_creation::promotion_namespace(prefix)
        || !controller_namespace.is_empty()
            && (!controller_namespace.starts_with('\\')
                || !class(controller_namespace.trim_start_matches('\\')))
    {
        return None;
    }
    at += 1;
    if ts.get(at)?.text != ";" {
        return None;
    }
    at += 1;
    let mut imports = BTreeMap::new();
    while ts.get(at).is_some_and(|t| t.text == "use") {
        at += 1;
        let full = ts.get(at)?.text.trim_start_matches('\\');
        if !class(full) {
            return None;
        }
        at += 1;
        let alias = if ts.get(at)?.text == "as" {
            at += 1;
            let name = ts.get(at)?.text;
            if !identifier(name) {
                return None;
            }
            at += 1;
            name
        } else {
            full.rsplit('\\').next()?
        };
        if imports
            .insert(alias.to_ascii_lowercase(), full.into())
            .is_some()
        {
            return None;
        }
        if ts.get(at)?.text != ";" {
            return None;
        }
        at += 1;
    }
    if ts.get(at)?.text == "final" {
        at += 1;
    }
    if ts.get(at)?.text != "class" {
        return None;
    }
    at += 1;
    let controller = ts.get(at)?.text;
    if !identifier(controller)
        || !super::request_creation::promotion_namespace(&format!("{controller}\\"))
    {
        return None;
    }
    at += 1;
    if ts.get(at)?.text == "extends" {
        at += 1;
        let parent = resolved(ts.get(at)?.text, namespace, &imports)?;
        if parent != "Illuminate\\Routing\\Controller" {
            return None;
        }
        at += 1;
    }
    if ts.get(at)?.text != "{" {
        return None;
    }
    let class_end = paired(&ts, at, 0)?;
    if class_end + 1 != ts.len() {
        return None;
    }
    at += 1;
    let mut found = Vec::new();
    while at < class_end {
        let begin = at;
        if ts[at].text == "use" {
            return None;
        }
        let mut public = true;
        let mut target = true;
        while matches!(
            ts[at].text,
            "public" | "protected" | "private" | "static" | "final" | "abstract"
        ) {
            public &= !matches!(ts[at].text, "private" | "protected");
            target &= !matches!(ts[at].text, "static" | "abstract");
            at += 1;
        }
        if ts[at].text != "function" {
            at = begin;
            while at < class_end && ts[at].text != ";" {
                if matches!(ts[at].text, "(" | "[" | "{") {
                    at = paired(&ts, at, 0)? + 1;
                } else {
                    at += 1;
                }
            }
            if at == class_end {
                return None;
            }
            at += 1;
            continue;
        }
        at += 1;
        let name = ts.get(at)?.text;
        if !identifier(name) {
            return None;
        }
        at += 1;
        if ts.get(at)?.text != "(" {
            return None;
        }
        let params_end = paired(&ts, at, 0)?;
        let params = parts(&ts, at + 1, params_end)?;
        at = params_end + 1;
        while at < class_end && !matches!(ts[at].text, "{" | ";") {
            at += 1;
        }
        if at >= class_end {
            return None;
        }
        if ts[at].text == ";" {
            at += 1;
            continue;
        }
        let body = at;
        let body_end = paired(&ts, body, 0)?;
        at = body_end + 1;
        if !public
            || !target
            || name.starts_with("__")
            || ["callAction", "middleware", "getMiddleware"].contains(&name)
        {
            continue;
        }
        let matching = params
            .into_iter()
            .filter_map(|(from, to)| {
                if to != from + 2 {
                    return None;
                }
                let full = resolved(ts[from].text, namespace, &imports)?;
                (full == "Illuminate\\Http\\Request"
                    && ts[from + 1]
                        .text
                        .strip_prefix('$')
                        .is_some_and(ordinary_variable))
                .then_some((ts[from], ts[from + 1]))
            })
            .collect::<Vec<_>>();
        let [(parameter, variable)] = matching.as_slice() else {
            continue;
        };
        let mut cursor = body + 1;
        if ts.get(cursor)?.text == "return" {
            cursor += 1;
        } else if ts.get(cursor)?.text.starts_with('$') && ts.get(cursor + 1)?.text == "=" {
            let local = ts[cursor].text.strip_prefix('$')?;
            if !ordinary_variable(local) || ts[cursor].text == variable.text {
                continue;
            }
            cursor += 2;
        }
        let Some(receiver) = ts.get(cursor).copied() else {
            continue;
        };
        if receiver.text != variable.text
            || ts.get(cursor + 1)?.text != "->"
            || ts.get(cursor + 2)?.text != "validate"
            || ts.get(cursor + 3)?.text != "("
            || ts.get(cursor + 4)?.text != "["
        {
            continue;
        }
        let open = cursor + 4;
        let close = paired(&ts, open, 0)?;
        if ts.get(close + 1)?.text != ")" || ts.get(close + 2)?.text != ";" {
            continue;
        }
        if ts[body + 1..body_end]
            .windows(3)
            .filter(|part| {
                part[0].text == variable.text
                    && part[1].text == "->"
                    && matches!(part[2].text, "validate" | "validateWithBag")
            })
            .count()
            != 1
        {
            continue;
        }
        let controller_full = format!("{namespace}\\{controller}");
        let Some(rules) = rules(
            &doc.text,
            &ts,
            open,
            close,
            namespace,
            &imports,
            &controller_full,
        ) else {
            continue;
        };
        found.push(Shape {
            controller: if controller_namespace.is_empty() {
                controller.into()
            } else {
                format!(
                    "{}/{}",
                    controller_namespace
                        .trim_start_matches('\\')
                        .replace('\\', "/"),
                    controller
                )
            },
            prefix: prefix.into(),
            parameter: parameter.start..parameter.end,
            call: receiver.start..ts[close + 1].end,
            rules,
            variable: variable.text.strip_prefix('$')?.into(),
            strict,
        });
    }
    Some(found)
}
