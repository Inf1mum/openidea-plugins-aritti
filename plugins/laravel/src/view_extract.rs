//! Previewed current-buffer HTML/simple-echo extraction, never arbitrary PHP scope inference.
use crate::{Main, view_creation};
use openidea_plugin::{
    Error, Range, TextEdit,
    file_actions::{FileAction, FileOperation},
    host,
    providers::Document,
    text::offset,
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
pub(crate) struct State {
    next: u64,
    pending: BTreeMap<String, Pending>,
}
#[derive(Clone)]
struct Pending {
    doc: Document,
    root: String,
    from: usize,
    to: usize,
    vars: Vec<String>,
    component: bool,
}
const TAGS: &[&str] = &[
    "html",
    "head",
    "body",
    "title",
    "div",
    "span",
    "p",
    "section",
    "article",
    "header",
    "footer",
    "main",
    "nav",
    "aside",
    "ul",
    "ol",
    "li",
    "a",
    "strong",
    "em",
    "b",
    "i",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "table",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "td",
    "th",
    "form",
    "label",
    "input",
    "button",
    "select",
    "option",
    "textarea",
    "img",
    "br",
    "hr",
    "small",
    "pre",
    "code",
    "figure",
    "figcaption",
];
const VOID: &[&str] = &["input", "img", "br", "hr"];
fn variable(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || i > 0 && b.is_ascii_digit())
}
/// Read-only expression grammar: literals, variable/property/index reads and operators.
/// Caller-variable mutation, reference arguments and implicit runtime state are excluded.
/// Zero-argument instance calls retain the same captured object without exposing
/// caller variables to by-reference arguments.
struct CaptureParser<'a> {
    source: &'a str,
    at: usize,
    depth: usize,
    vars: BTreeSet<String>,
}
impl<'a> CaptureParser<'a> {
    fn ws(&mut self) {
        while self
            .source
            .as_bytes()
            .get(self.at)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.at += 1;
        }
    }
    fn take(&mut self, value: &str) -> bool {
        self.ws();
        if self.source[self.at..].starts_with(value) {
            self.at += value.len();
            true
        } else {
            false
        }
    }
    fn identifier(&mut self) -> Option<&'a str> {
        self.ws();
        let start = self.at;
        while self
            .source
            .as_bytes()
            .get(self.at)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            self.at += 1;
        }
        let name = &self.source[start..self.at];
        variable(name).then_some(name)
    }
    fn atom(&mut self) -> Option<()> {
        self.ws();
        self.depth += 1;
        if self.source[self.at..].starts_with("++") || self.source[self.at..].starts_with("--") {
            return None;
        }
        if self.depth > 32 {
            return None;
        }
        if self.take("!") || self.take("-") || self.take("+") {
            self.atom()?;
        } else if self.take("(") {
            self.expression()?;
            if !self.take(")") {
                return None;
            }
        } else if self.take("[") {
            if !self.take("]") {
                loop {
                    self.expression()?;
                    if self.take("=>") {
                        self.expression()?;
                    }
                    if self.take("]") {
                        break;
                    }
                    if !self.take(",") {
                        return None;
                    }
                    if self.take("]") {
                        break;
                    }
                }
            }
        } else if self.take("$") {
            let name = self.identifier()?;
            if [
                "loop",
                "component",
                "__env",
                "__data",
                "__path",
                "GLOBALS",
                "this",
            ]
            .contains(&name)
            {
                return None;
            }
            self.vars.insert(name.into());
            if self.vars.len() > 32 {
                return None;
            }
        } else if self
            .source
            .as_bytes()
            .get(self.at)
            .is_some_and(|b| matches!(b, b'\'' | b'"'))
        {
            let start = self.at;
            let quote = self.source.as_bytes()[start];
            let end = crate::php::string_end(self.source.as_bytes(), start);
            if end <= start + 1
                || self.source.as_bytes().get(end - 1) != Some(&quote)
                || quote == b'"' && self.source[start + 1..end - 1].contains('$')
            {
                return None;
            }
            self.at = end;
        } else if self
            .source
            .as_bytes()
            .get(self.at)
            .is_some_and(u8::is_ascii_digit)
        {
            while self
                .source
                .as_bytes()
                .get(self.at)
                .is_some_and(u8::is_ascii_digit)
            {
                self.at += 1;
            }
        } else {
            let name = self.identifier()?;
            if !["true", "false", "null"].contains(&name) {
                return None;
            }
        }
        loop {
            if self.take("?->") || self.take("->") {
                self.identifier()?;
                if self.take("(") && !self.take(")") {
                    return None;
                }
            } else if self.take("[") {
                self.expression()?;
                if !self.take("]") {
                    return None;
                }
            } else {
                break;
            }
        }
        self.depth -= 1;
        Some(())
    }
    fn expression(&mut self) -> Option<()> {
        self.depth += 1;
        if self.depth > 32 {
            return None;
        }
        self.atom()?;
        loop {
            self.ws();
            let remaining = &self.source[self.at..];
            let Some(operator) = [
                "===", "!==", "??", "==", "!=", "<=", ">=", "&&", "||", ".", "+", "-", "*", "/",
                "%", "<", ">",
            ]
            .into_iter()
            .find(|op| remaining.starts_with(op)) else {
                break;
            };
            // ++/-- and assignment must never be interpreted as binary/unary pairs.
            if remaining.starts_with("++") || remaining.starts_with("--") {
                return None;
            }
            self.at += operator.len();
            self.atom()?;
        }
        if self.take("?") {
            if !self.take(":") {
                self.expression()?;
                if !self.take(":") {
                    return None;
                }
            }
            self.expression()?;
        }
        self.depth -= 1;
        Some(())
    }
}
fn captures(expression: &str) -> Option<Vec<String>> {
    let mut parser = CaptureParser {
        source: expression,
        at: 0,
        depth: 0,
        vars: BTreeSet::new(),
    };
    parser.expression()?;
    parser.ws();
    if parser.vars.contains("slot") && expression.trim() != "$slot" {
        return None;
    }
    (parser.at == expression.len()).then(|| parser.vars.into_iter().collect())
}

fn local_binding(name: &str) -> bool {
    variable(name)
        && ![
            "GLOBALS",
            "this",
            "loop",
            "slot",
            "attributes",
            "component",
            "__env",
            "__data",
            "__path",
            "_GET",
            "_POST",
            "_REQUEST",
            "_SERVER",
            "_SESSION",
            "_COOKIE",
            "_FILES",
            "_ENV",
        ]
        .contains(&name)
}
fn php_block_facts(block: &str) -> Option<(Vec<String>, Vec<String>)> {
    let body = block.strip_prefix("@php")?.strip_suffix("@endphp")?;
    let code = crate::php::mask_comments(body);
    let bytes = code.as_bytes();
    let mut at = 0;
    let mut start = 0;
    let mut stack = Vec::new();
    let mut reads = BTreeSet::new();
    let mut writes = BTreeSet::new();
    let mut statements = 0;
    while at < bytes.len() {
        if matches!(bytes[at], b'\'' | b'"') {
            at = crate::php::string_end(bytes, at);
            continue;
        }
        match bytes[at] {
            b'(' => stack.push(b')'),
            b'[' => stack.push(b']'),
            b')' | b']' => {
                if stack.pop() != Some(bytes[at]) {
                    return None;
                }
            }
            b'{' | b'}' | b'`' => return None,
            b';' if stack.is_empty() => {
                let statement = code[start..at].trim();
                if !statement.is_empty() {
                    statements += 1;
                    if statements > 32 {
                        return None;
                    }
                    if let Some(value) = statement.strip_prefix("echo ") {
                        reads.extend(captures(value)?);
                    } else {
                        let (left, right) = statement.split_once('=')?;
                        let name = left.trim().strip_prefix('$')?;
                        if !local_binding(name) || right.trim_start().starts_with(['=', '>']) {
                            return None;
                        }
                        reads.extend(captures(right)?);
                        writes.insert(name.to_owned());
                    }
                }
                start = at + 1;
            }
            _ => {}
        }
        if stack.len() > 32 {
            return None;
        }
        at += 1;
    }
    if !stack.is_empty() || !code[start..].trim().is_empty() {
        return None;
    }
    Some((reads.into_iter().collect(), writes.into_iter().collect()))
}
fn selected_writes(selected: &str) -> Option<Vec<String>> {
    let mut writes = BTreeSet::new();
    for span in crate::blade_php::ranges(selected)? {
        if selected[span.clone()].starts_with("@php") {
            writes.extend(php_block_facts(&selected[span])?.1);
        }
    }
    let code = crate::php::mask_comments(selected);
    for (_, tail) in code
        .match_indices("@foreach")
        .chain(code.match_indices("@forelse"))
        .map(|(at, name)| (at, &code[at + name.len()..]))
    {
        let tail = tail.trim_start();
        if !tail.starts_with('(') {
            return None;
        }
        let end = crate::php::closing(tail, 0)?;
        let (_, binding) = tail[1..end].split_once(" as ")?;
        for binder in binding.split("=>") {
            let name = binder.trim().strip_prefix('$')?;
            if !local_binding(name) {
                return None;
            }
            writes.insert(name.to_owned());
        }
    }
    Some(writes.into_iter().collect())
}
fn writes_do_not_escape(doc: &Document, to: usize, writes: &[String]) -> bool {
    if writes.is_empty() {
        return true;
    }
    let suffix = &doc.text[to..];
    if doc.text.contains("@extends")
        || [
            "@include",
            "@each",
            "@component",
            "<x-",
            "@php",
            "<?",
            "get_defined_vars",
            "compact(",
            "$$",
        ]
        .iter()
        .any(|token| suffix.contains(token))
    {
        return false;
    }
    !writes.iter().any(|name| {
        suffix
            .match_indices(&format!("${name}"))
            .any(|(at, token)| {
                suffix
                    .as_bytes()
                    .get(at + token.len())
                    .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_')
            })
    })
}

fn selected_captures(selected: &str) -> Option<Vec<String>> {
    let bytes = selected.as_bytes();
    let mut at = 0;
    let mut stack: Vec<&str> = Vec::new();
    let mut vars = BTreeSet::new();
    while at < bytes.len() {
        if bytes[at..].starts_with(b"@php") {
            let span = crate::blade_php::ranges(selected)?
                .into_iter()
                .find(|span| span.start == at)?;
            let (reads, writes) = php_block_facts(&selected[span.clone()])?;
            vars.extend(reads);
            vars.extend(writes);
            at = span.end;
            continue;
        }
        if bytes[at..].starts_with(b"{{") {
            let end = echo_end(selected, at, false)?;
            vars.extend(captures(selected[at + 2..end - 2].trim())?);
            at = end;
            continue;
        }
        if bytes[at] != b'@'
            || at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_')
        {
            at += 1;
            continue;
        }
        at += 1;
        let start = at;
        while bytes.get(at).is_some_and(u8::is_ascii_alphabetic) {
            at += 1;
        }
        let name = selected.get(start..at)?;
        let mut open = at;
        while bytes.get(open).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            open += 1;
        }
        let args = if bytes.get(open) == Some(&b'(') {
            at = crate::php::closing(selected, open)? + 1;
            Some(crate::php::arguments(selected, open))
        } else {
            None
        };
        match name {
            "foreach" | "forelse" => {
                let args = args.as_ref()?;
                if args.len() != 1 {
                    return None;
                }
                let (iterable, binding) = selected[args[0].clone()].split_once(" as ")?;
                vars.extend(captures(iterable)?);
                for binder in binding.split("=>") {
                    let name = binder.trim().strip_prefix('$')?;
                    if !local_binding(name) {
                        return None;
                    }
                    vars.insert(name.to_owned());
                }
                stack.push(name);
            }
            "empty" if args.is_none() && stack.last() == Some(&"forelse") => {}
            "break" | "continue" => {
                if !stack
                    .iter()
                    .any(|name| matches!(*name, "foreach" | "forelse"))
                {
                    return None;
                }
                if let Some(args) = args.as_ref() {
                    if args.len() != 1 {
                        return None;
                    }
                    vars.extend(captures(&selected[args[0].clone()])?);
                }
            }
            "if" | "unless" | "isset" | "empty" => {
                let args = args.as_ref()?;
                if args.len() != 1 {
                    return None;
                }
                vars.extend(captures(&selected[args[0].clone()])?);
                stack.push(name);
            }
            "elseif" => {
                if stack.last() != Some(&"if") {
                    return None;
                }
                let args = args.as_ref()?;
                if args.len() != 1 {
                    return None;
                }
                vars.extend(captures(&selected[args[0].clone()])?);
            }
            "else" => {
                if args.is_some() || stack.is_empty() {
                    return None;
                }
            }
            "endif" | "endunless" | "endisset" | "endempty" | "endforeach" | "endforelse" => {
                if args.is_some() || stack.pop() != name.strip_prefix("end") {
                    return None;
                }
            }
            "class" | "style" | "json" => {
                let args = args.as_ref()?;
                if args.len() != 1 {
                    return None;
                }
                vars.extend(captures(&selected[args[0].clone()])?);
            }
            "include" | "includeIf" => {
                let args = args.as_ref()?;
                if args.is_empty()
                    || args.len() > 2
                    || crate::php::literal(selected, args[0].clone()).is_none()
                {
                    return None;
                }
                if args.len() == 2 {
                    vars.extend(captures(&selected[args[1].clone()])?);
                }
            }
            "method" => {
                let args = args.as_ref()?;
                if args.len() != 1 || crate::php::literal(selected, args[0].clone()).is_none() {
                    return None;
                }
            }
            "csrf" => {
                if args.is_some() {
                    return None;
                }
            }
            _ => return None,
        }
        if stack.len() > 32 || vars.len() > 32 {
            return None;
        }
    }
    stack.is_empty().then(|| vars.into_iter().collect())
}

fn overlaps(a: std::ops::Range<usize>, from: usize, to: usize) -> bool {
    a.start < to && from < a.end
}
fn blank(bytes: &mut [u8], from: usize, to: usize) {
    for byte in &mut bytes[from..to] {
        if !matches!(*byte, b'\r' | b'\n') {
            *byte = b' ';
        }
    }
}
fn echo_end(text: &str, start: usize, raw: bool) -> Option<usize> {
    let bytes = text.as_bytes();
    let marker = if raw {
        b"!!}".as_slice()
    } else {
        b"}}".as_slice()
    };
    let mut at = start + if raw { 3 } else { 2 };
    let mut stack = Vec::new();
    while at < bytes.len() {
        if stack.is_empty() && bytes[at..].starts_with(marker) {
            return Some(at + marker.len());
        }
        if matches!(bytes[at], b'\'' | b'"') {
            let quote = bytes[at];
            let begin = at;
            at = crate::php::string_end(bytes, at);
            if at <= begin + 1 || at > bytes.len() || bytes.get(at - 1) != Some(&quote) {
                return None;
            }
        } else if bytes[at..].starts_with(b"/*") {
            at += text[at + 2..].find("*/")? + 4;
        } else if bytes[at..].starts_with(b"//") || bytes[at] == b'#' {
            at += text[at..].find('\n')?;
        } else {
            match bytes[at] {
                b'(' => stack.push(b')'),
                b'[' => stack.push(b']'),
                b'{' => stack.push(b'}'),
                b')' | b']' | b'}' if stack.pop() != Some(bytes[at]) => return None,
                b'`' => return None,
                _ => {}
            }
            if stack.len() > 64 {
                return None;
            }
            at += 1;
        }
    }
    None
}

/// Prove the selected fragment is template content rather than a comment, tag
/// attribute, directive argument or control-flow body. Known section/stack
/// wrappers preserve the fragment's data environment; unknown directives do not.
fn boundary(text: &str, from: usize, to: usize) -> Option<String> {
    let bytes = text.as_bytes();
    let mut result = bytes.to_vec();
    let opaque = crate::blade_php::ranges(text)?;
    let mut span = 0;
    let mut at = 0;
    let mut stack: Vec<(&str, bool)> = Vec::new();
    let mut checked = false;
    while at < bytes.len() {
        if !checked && at >= from {
            if stack.iter().any(|(_, safe)| !*safe) {
                return None;
            }
            checked = true;
        }
        while opaque.get(span).is_some_and(|range| range.end <= at) {
            span += 1;
        }
        if let Some(range) = opaque.get(span).filter(|range| range.contains(&at)) {
            if overlaps(range.clone(), from, to)
                && !(from <= range.start
                    && range.end <= to
                    && php_block_facts(&text[range.clone()]).is_some())
            {
                return None;
            }
            blank(&mut result, range.start, range.end);
            at = range.end;
            continue;
        }
        if bytes[at..].starts_with(b"<!--") {
            let end = at + 4 + text[at + 4..].find("-->")? + 3;
            if overlaps(at..end, from, to) {
                return None;
            }
            blank(&mut result, at, end);
            at = end;
            continue;
        }
        if bytes[at..].starts_with(b"{{") || bytes[at..].starts_with(b"{!!") {
            let raw = bytes[at..].starts_with(b"{!!");
            let end = echo_end(text, at, raw)?;
            if overlaps(at..end, from, to) {
                if raw || from > at || end > to {
                    return None;
                }
            } else {
                blank(&mut result, at, end);
            }
            at = end;
            continue;
        }
        if bytes[at] != b'@'
            || at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_')
        {
            at += 1;
            continue;
        }
        let begin = at;
        at += 1;
        let start = at;
        while bytes.get(at).is_some_and(|b| b.is_ascii_alphabetic()) {
            at += 1;
        }
        let name = text.get(start..at)?;
        if name.is_empty() {
            return None;
        }
        let mut open = at;
        while bytes.get(open).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            open += 1;
        }
        let arguments = if bytes.get(open) == Some(&b'(') {
            let end = crate::php::closing(text, open)?;
            at = end + 1;
            Some(crate::php::arguments(text, open))
        } else {
            None
        };
        if overlaps(begin..at, from, to)
            && !(from <= begin
                && at <= to
                && matches!(
                    name,
                    "foreach"
                        | "forelse"
                        | "endforeach"
                        | "endforelse"
                        | "break"
                        | "continue"
                        | "if"
                        | "unless"
                        | "isset"
                        | "empty"
                        | "elseif"
                        | "else"
                        | "endif"
                        | "endunless"
                        | "endisset"
                        | "endempty"
                        | "class"
                        | "style"
                        | "json"
                        | "include"
                        | "includeIf"
                        | "method"
                        | "csrf"
                ))
        {
            return None;
        }
        match name {
            "props" | "aware" => {
                let args = arguments.as_ref()?;
                if args.len() != 1 || crate::php::array_open(text, args[0].clone()).is_none() {
                    return None;
                }
            }
            "extends" => {
                let args = arguments.as_ref()?;
                if args.len() != 1 || crate::php::literal(text, args[0].clone()).is_none() {
                    return None;
                }
            }
            "section" => {
                let args = arguments.as_ref()?;
                if args.is_empty()
                    || args.len() > 2
                    || crate::php::literal(text, args[0].clone()).is_none()
                {
                    return None;
                }
                if args.len() == 1 || text[args[1].clone()].trim() == "null" {
                    stack.push(("section", true));
                } else if crate::php::literal(text, args[1].clone()).is_none() {
                    return None;
                }
            }
            "push" | "prepend" => {
                let args = arguments.as_ref()?;
                if args.len() != 1 || crate::php::literal(text, args[0].clone()).is_none() {
                    return None;
                }
                stack.push((name, true));
            }
            "endsection" | "show" | "stop" | "append" | "overwrite" => {
                if stack.pop()?.0 != "section" {
                    return None;
                }
            }
            "endpush" => {
                if stack.pop()?.0 != "push" {
                    return None;
                }
            }
            "endprepend" => {
                if stack.pop()?.0 != "prepend" {
                    return None;
                }
            }
            "empty"
                if arguments.is_none()
                    && stack.last().is_some_and(|(name, _)| *name == "forelse") => {}
            "if" | "unless" | "foreach" | "forelse" | "for" | "while" | "isset" | "empty"
            | "hasSection" | "sectionMissing" => {
                if arguments.as_ref().is_none_or(Vec::is_empty) {
                    return None;
                }
                stack.push((name, true));
            }
            "auth" | "guest" | "env" | "production" | "once" => stack.push((name, true)),
            "else" | "elseif" => {
                if stack.last().is_none_or(|(name, _)| {
                    !matches!(
                        *name,
                        "if" | "unless"
                            | "isset"
                            | "empty"
                            | "auth"
                            | "guest"
                            | "env"
                            | "production"
                            | "hasSection"
                            | "sectionMissing"
                    )
                }) {
                    return None;
                }
            }
            "endif" | "endunless" | "endforeach" | "endforelse" | "endfor" | "endwhile"
            | "endisset" | "endempty" | "endauth" | "endguest" | "endenv" | "endproduction"
            | "endonce" => {
                let expected = name.strip_prefix("end")?;
                let actual = stack.pop()?.0;
                if actual != expected
                    && !(name == "endif" && matches!(actual, "hasSection" | "sectionMissing"))
                {
                    return None;
                }
            }
            "break" | "continue" => {}
            "class" | "style" | "include" | "includeIf" | "includeWhen" | "includeUnless"
            | "includeFirst" | "each" | "yield" | "stack" | "vite" | "json" | "method" => {
                arguments.as_ref()?;
            }
            "csrf" | "livewireStyles" | "livewireScripts" => {
                if arguments.is_some() {
                    return None;
                }
            }
            _ => return None,
        }
        if stack.len() > 64 {
            return None;
        }
        blank(&mut result, begin, at);
    }
    if !checked || !stack.is_empty() {
        return None;
    }
    String::from_utf8(result).ok()
}

fn fragment(doc: &Document, from: usize, to: usize) -> Option<Vec<String>> {
    if doc.language != "blade"
        || !doc.path.ends_with(".blade.php")
        || doc.text.len() > 128 << 10
        || from >= to
        || to > doc.text.len()
        || to - from > 4096
        || !doc.text.is_char_boundary(from)
        || !doc.text.is_char_boundary(to)
    {
        return None;
    }
    let text = &doc.text;
    let selected = &text[from..to];
    if selected.contains("<?")
        || selected.contains("{!!")
        || selected.contains("<!--")
        || selected.contains("{{--")
        || selected.contains('`')
    {
        return None;
    }
    let writes = selected_writes(selected)?;
    if !writes_do_not_escape(doc, to, &writes) {
        return None;
    }
    let boundary = boundary(text, from, to)?;
    let mut masked = boundary.as_bytes().to_vec();
    let mut cursor = 0;
    let mut vars: BTreeSet<String> = selected_captures(selected)?.into_iter().collect();
    while let Some(open) = boundary[cursor..].find("{{").map(|n| cursor + n) {
        let end = echo_end(text, open, false)?;
        let raw = text[open + 2..end - 2].trim();
        let captured = captures(raw)?;
        if open < to && from < end {
            if open < from || end > to {
                return None;
            }
            vars.extend(captured);
            if vars.len() > 32 {
                return None;
            }
        }
        for b in &mut masked[open..end] {
            if !matches!(*b, b'\r' | b'\n') {
                *b = b' ';
            }
        }
        cursor = end;
    }
    let masked = String::from_utf8(masked).ok()?;
    let bytes = masked.as_bytes();
    let mut at = 0;
    let mut stack: Vec<(String, usize)> = Vec::new();
    let mut matched = false;
    let mut count = 0;
    while at < bytes.len() {
        if bytes[at] != b'<' {
            at += 1;
            continue;
        }
        let start = at;
        at += 1;
        let close = bytes.get(at) == Some(&b'/');
        if close {
            at += 1;
        }
        let name_at = at;
        while bytes
            .get(at)
            .is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b':' | b'.'))
        {
            at += 1;
        }
        let name = masked.get(name_at..at)?;
        let component_wrapper = name.starts_with("x-")
            && name != "x-dynamic-component"
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b':' | b'.'));
        if !TAGS.contains(&name) && !component_wrapper {
            return None;
        }
        let mut quote = 0;
        let mut ended = false;
        while at < bytes.len() {
            let b = bytes[at];
            if quote != 0 {
                if b == quote {
                    quote = 0;
                }
            } else if matches!(b, b'\'' | b'"') {
                quote = b;
            } else if b == b'>' {
                at += 1;
                ended = true;
                break;
            } else if b == b'<' {
                return None;
            }
            at += 1;
        }
        if !ended || quote != 0 {
            return None;
        }
        // Component/slot wrappers remain in the caller. Moving their bindings is separate work.

        count += 1;
        if count > 2048 {
            return None;
        }
        let self_closing = bytes.get(at.saturating_sub(2)) == Some(&b'/');
        if close {
            if !masked[name_at + name.len()..at - 1].trim().is_empty() {
                return None;
            }
            let (opened, begin) = stack.pop()?;
            if opened != name {
                return None;
            }
            if begin == from && at == to {
                matched = true;
            }
        } else if self_closing || VOID.contains(&name) {
            if start == from && at == to {
                matched = true;
            }
        } else {
            stack.push((name.into(), start));
            if stack.len() > 64 {
                return None;
            }
        }
    }
    (stack.is_empty() && matched).then(|| vars.into_iter().collect())
}
fn lazy_captures(selected: &str) -> bool {
    ["??", "&&", "||", "@"]
        .iter()
        .any(|operator| selected.contains(operator))
}
fn shadowed(index: &crate::index::Index, doc: &Document, component: bool) -> bool {
    index.directives.keys().any(|name| {
        name == "include" || component && name == "props" || doc.text.contains(&format!("@{name}"))
    })
}
fn component_name(name: &str) -> bool {
    view_creation::valid_name(name)
        && !matches!(name, "slot" | "dynamic-component")
        && name.split('.').all(|part| {
            part.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}
fn target_plan(
    main: &Main,
    p: &Pending,
    doc: &Document,
    name: &str,
    selected: Option<(&str, &str)>,
) -> Result<(String, String), Error> {
    let (project, _) = view_creation::owner(doc, Some(&p.root))?;
    let index = main
        .indexes
        .get(&p.root)
        .filter(|index| !index.partial)
        .ok_or(Error::Stale)?;
    if p.component {
        if !component_name(name) || index.open_components || index.components.contains_key(name) {
            return Err(Error::Invalid(
                crate::ui_text::text("native.view_extract.54710f1e9b", "Use a new kebab-case anonymous component name with proven registration ownership.")
                    .into(),
            ));
        }
        let class = name
            .split('.')
            .map(|part| {
                part.split('-')
                    .map(|word| {
                        let mut chars = word.chars();
                        chars
                            .next()
                            .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                            .unwrap_or_default()
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("/");
        view_creation::missing(
            &project,
            &format!("{}/app/View/Components/{class}.php", p.root),
        )?;
    } else if view_creation::roots::split(name).is_none() || index.views.contains_key(name) {
        return Err(Error::Stale);
    }
    if !p.component {
        return if let Some((target, proof)) = selected {
            view_creation::roots::revalidate(&project, &p.root, doc, name, target, proof)
        } else {
            view_creation::roots::select(&project, &p.root, doc, name)
        };
    }
    view_creation::validate(
        &project,
        &p.root,
        doc,
        &if p.component {
            format!("components.{name}")
        } else {
            name.into()
        },
    )
}

fn admitted(main: &Main, p: &Pending, doc: &Document) -> Result<(), Error> {
    if doc.path != p.doc.path
        || doc.language != p.doc.language
        || doc.version != p.doc.version
        || doc.text != p.doc.text
        || fragment(doc, p.from, p.to).as_ref() != Some(&p.vars)
    {
        return Err(Error::Stale);
    }
    let (project, root) = view_creation::owner(doc, Some(&p.root))?;
    let index = main
        .indexes
        .get(&root)
        .filter(|i| !i.partial)
        .ok_or(Error::Stale)?;
    if shadowed(index, doc, p.component)
        || p.component && index.open_components
        || project.read_file(&doc.path)? != doc.text
    {
        return Err(Error::Stale);
    }
    Ok(())
}
pub(crate) fn actions(main: &mut Main, doc: &Document, selection: Range) -> Vec<FileAction> {
    let Ok((_, root)) = view_creation::owner(doc, None) else {
        return vec![];
    };
    let Some(index) = main.indexes.get(&root).filter(|i| !i.partial) else {
        return vec![];
    };
    if shadowed(index, doc, false) {
        return vec![];
    }
    let (Some(from), Some(to)) = (
        offset(&doc.text, selection.start),
        offset(&doc.text, selection.end),
    ) else {
        return vec![];
    };
    let Some(vars) = fragment(doc, from, to) else {
        return vec![];
    };
    if main.view_extractions.pending.len() >= 8 {
        main.view_extractions.pending.clear();
    }
    let component_allowed = !index.open_components
        && !shadowed(index, doc, true)
        && !vars.iter().any(|name| name == "attributes")
        && !lazy_captures(&doc.text[from..to])
        && !doc.text[from..to].contains('@')
        && !doc.text[from..to].contains("<x-");
    let mut result = Vec::new();
    for component in [false, true] {
        if component && !component_allowed {
            continue;
        }
        let Some(next) = main.view_extractions.next.checked_add(1) else {
            break;
        };
        main.view_extractions.next = next;
        let key = format!("extract-view:{next}:{}", doc.version);
        main.view_extractions.pending.insert(
            key.clone(),
            Pending {
                doc: doc.clone(),
                root: root.clone(),
                from,
                to,
                vars: vars.clone(),
                component,
            },
        );
        result.push(FileAction {
            title: if component {
                crate::ui_text::t(
                    "action.component-extract",
                    "Extract reviewed anonymous Blade component…",
                )
            } else {
                crate::ui_text::t(
                    "action.view-extract",
                    "Extract reviewed HTML/simple-variable Blade view…",
                )
            },
            kind: Some("refactor.extract".into()),
            data: Some(key),
            ..Default::default()
        });
    }
    result
}

pub(crate) fn resolve(
    main: &mut Main,
    doc: &Document,
    action: FileAction,
) -> Result<FileAction, Error> {
    let key = action.data.as_deref().ok_or(Error::Stale)?;
    let p = main
        .view_extractions
        .pending
        .get(key)
        .cloned()
        .ok_or(Error::Stale)?;
    admitted(main, &p, doc)?;
    let name = host::input(
        &if p.component {
            crate::ui_text::t(
                "prompt.component-name",
                "New anonymous Blade component name (dot/kebab-case)",
            )
        } else {
            crate::ui_text::t("prompt.view-name", "New conventional Blade view name")
        },
        None,
    )?
    .ok_or(Error::Cancelled)?;
    admitted(main, &p, doc)?;
    let (target, config) = target_plan(main, &p, doc, &name, None)?;
    let data = if p.vars.is_empty() {
        String::new()
    } else if lazy_captures(&doc.text[p.from..p.to]) {
        format!(
            ", array_intersect_key(get_defined_vars(), array_flip([{}]))",
            p.vars
                .iter()
                .map(|name| format!("'{name}'"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!(
            ", [{}]",
            p.vars
                .iter()
                .map(|v| format!("'{v}' => ${v}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let replacement = if p.component {
        let attributes = p
            .vars
            .iter()
            .filter(|name| name.as_str() != "slot")
            .map(|name| format!(" :{name}=\"${name}\""))
            .collect::<String>();
        if p.vars.iter().any(|name| name == "slot") {
            format!("<x-{name}{attributes}>{{{{ $slot }}}}</x-{name}>")
        } else {
            format!("<x-{name}{attributes} />")
        }
    } else {
        format!("@include('{name}'{data})")
    };
    let selected = &doc.text[p.from..p.to];
    let props: Vec<_> = p
        .vars
        .iter()
        .filter(|name| name.as_str() != "slot")
        .collect();
    let content = if p.component && !props.is_empty() {
        format!(
            "@props([{}]){selected}",
            props
                .iter()
                .map(|name| format!("'{name}'"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        selected.into()
    };
    let preview = crate::ui_text::format(
        "extraction.review",
        "Project: {root}\nSource: {source}\nDocument version: {version}\n\nCreate (no overwrite): {target}\nExact new template contents:\n{content}\n\nReplace this exact selected fragment with:\n{replacement}\n\nCaptured names: {variables}. Supported read-only expressions and balanced directives remain intact. Parsed local assignments and loops move only when no outward binding use is observed; runtime/compiler bindings are excluded. Default slot content is forwarded for anonymous components. Custom PHP calls, unknown directives, configured roots/namespaces and dynamic rendering remain withheld. No PHP/Artisan/process executes during this edit.\n\nApply one undoable Create + source Edit atomically?",
        &[
            ("root", &p.root),
            ("source", &doc.path),
            ("version", &doc.version.to_string()),
            ("target", &target),
            ("content", &content),
            ("replacement", &replacement),
            ("variables", &p.vars.join(", ")),
        ],
    );
    if !ui_kit::prompts::confirm_exact(
        &if p.component {
            crate::ui_text::t(
                "prompt.component-confirm",
                "Extract anonymous Blade component?",
            )
        } else {
            crate::ui_text::t("prompt.view-confirm", "Extract local Blade fragment?")
        },
        &preview,
    )? {
        return Err(Error::Cancelled);
    }
    admitted(main, &p, doc)?;
    let (current, current_config) = target_plan(main, &p, doc, &name, Some((&target, &config)))?;
    if current != target || current_config != config {
        return Err(Error::Stale);
    }
    main.view_extractions.pending.remove(key);
    Ok(FileAction {
        title: if p.component {
            format!("Extract Blade component {name}")
        } else {
            format!("Extract Blade view {name}")
        },
        operations: vec![FileOperation::Create {
            path: target.clone(),
            text: content.into(),
            overwrite: false,
        }],
        edits: vec![(
            doc.path.clone(),
            vec![TextEdit {
                range: Range::new(
                    openidea_plugin::text::position(&doc.text, p.from),
                    openidea_plugin::text::position(&doc.text, p.to),
                ),
                text: replacement,
            }],
        )],
        open: Some(target),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{file_actions::FileActions, mock::MockHost, text::position};
    fn setup(text: &str) -> (MockHost, Main, Document, Range) {
        let h = MockHost::install();
        h.add_dir("resources/views");
        h.add_file("resources/views/main.blade.php", text);
        h.open_editor(
            "/project/resources/views/main.blade.php",
            "blade",
            text,
            &[],
        );
        let mut main = Main::default();
        main.indexes
            .insert("/project".into(), crate::index::Index::default());
        let d = Document {
            path: "/project/resources/views/main.blade.php".into(),
            language: "blade".into(),
            version: 3,
            text: text.into(),
        };
        let from = text.find("<section").unwrap_or(0);
        let to = text.rfind("</section>").map_or(text.len(), |n| n + 10);
        let selection = Range::new(position(text, from), position(text, to));
        (h, main, d, selection)
    }
    #[test]
    fn exact_fragment_and_readonly_variables_are_one_atomic_create_plus_edit_without_execution() {
        let text = "😀\r\n<main><section>Hi {{ $name }}<b>{{ $title }}</b></section><footer>keep</footer></main>";
        let (h, mut main, d, selected) = setup(text);
        let a = main.file_actions(&d, selected).remove(0);
        let replay = a.clone();
        assert!(a.edits.is_empty() && a.operations.is_empty());
        h.answer_next_prompt(Some(&["partials.card"]));
        h.answer_next_prompt(Some(&["yes"]));
        let fixed = main.resolve_file_action(&d, a).unwrap();
        assert_eq!(
            fixed.operations,
            vec![FileOperation::Create {
                path: "/project/resources/views/partials/card.blade.php".into(),
                text: "<section>Hi {{ $name }}<b>{{ $title }}</b></section>".into(),
                overwrite: false
            }]
        );
        assert!(
            h.invocation()
                .project()
                .unwrap()
                .read_file("resources/views/partials/card.blade.php")
                .is_err()
        );
        assert_eq!(fixed.edits.len(), 1);
        let edit = &fixed.edits[0].1[0];
        assert_eq!(edit.range, selected);
        assert_eq!(
            edit.text,
            "@include('partials.card', ['name' => $name, 'title' => $title])"
        );
        assert_eq!(
            h.invocation()
                .project()
                .unwrap()
                .read_file(&d.path)
                .unwrap(),
            text
        );
        assert!(main.resolve_file_action(&d, replay).is_err());
        assert!(h.process_runs().is_empty());
        assert_eq!(h.fetches(), 0);
    }
    #[test]
    fn cancelled_stale_foreign_and_existing_targets_never_return_partial_transactions() {
        for mode in 0..6 {
            let (h, mut main, mut d, selected) = setup("<section>{{ $name }}</section>");
            let a = main.file_actions(&d, selected).remove(0);
            match mode {
                0 => h.answer_next_prompt(None),
                1 => {
                    h.answer_next_prompt(Some(&["partials.card"]));
                    h.answer_next_prompt(None);
                }
                2 => d.version += 1,
                3 => h.set_project_root(Some("/other")),
                4 => {
                    h.add_file("resources/views/partials/card.blade.php", "KEEP");
                    h.answer_next_prompt(Some(&["partials.card"]));
                }
                _ => {
                    h.add_project("/project/resources/views/partials");
                    h.answer_next_prompt(Some(&["partials.card"]));
                }
            }
            assert!(main.resolve_file_action(&d, a).is_err(), "{mode}");
            assert!(h.process_runs().is_empty());
        }
    }
    #[test]
    fn unsaved_source_is_supported_but_directive_php_component_and_variable_scopes_are_withheld() {
        let (h, mut main, d, selected) = setup("<section>{{ $name }}</section>");
        h.add_file("resources/views/main.blade.php", "disk differs");
        let a = main.file_actions(&d, selected).remove(0);
        h.answer_next_prompt(Some(&["card"]));
        h.answer_next_prompt(Some(&["yes"]));
        assert_eq!(main.resolve_file_action(&d, a).unwrap().edits.len(), 1);
        for source in [
            "<section>{{ $loop }}</section>",
            "<section>{{ $x = 1 }}</section>",
            "@verbatim<section>{{ $name }}</section>@endverbatim",
            "<section><b>bad</section></b>",
            "<!-- <section>hidden</section> -->",
            "<section>{!! $name !!}</section>",
        ] {
            let d = Document {
                text: source.into(),
                ..d.clone()
            };
            let selected = Range::new(position(source, 0), position(source, source.len()));
            assert!(main.file_actions(&d, selected).is_empty(), "{source}");
        }
    }
    #[test]
    fn partial_nodes_oversize_and_custom_include_ownership_do_not_get_extraction() {
        let (_h, mut main, d, selected) = setup("<section>{{ $name }}</section>");
        let wrong = Range::new(position(&d.text, 1), selected.end);
        assert!(main.file_actions(&d, wrong).is_empty());
        main.indexes.get_mut("/project").unwrap().directives.insert(
            "include".into(),
            crate::index::Entry {
                detail: None,
                path: "/project/Provider.php".into(),
                at: openidea_plugin::Position::new(0, 0),
                width: 7,
            },
        );
        assert!(main.file_actions(&d, selected).is_empty());
        let big = Document {
            text: format!("<section>{}</section>", "x".repeat(4096)),
            ..d
        };
        assert!(fragment(&big, 0, big.text.len()).is_none());
    }
    #[test]
    fn registered_extract_refuses_view_configuration_newly_owned_by_nested_project() {
        let (host, mut main, doc, range) = setup("<section>Hello {{ $name }}</section>");
        let lazy = main.file_actions(&doc, range).remove(0);
        host.add_project("/project/config");
        host.add_file(
            "config/view.php",
            "<?php return ['paths' => [resource_path('views')]];",
        );
        host.answer_next_prompt(Some(&["partials.card"]));
        host.answer_next_prompt(Some(&["yes"]));
        assert!(main.resolve_file_action(&doc, lazy).is_err());
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }

    #[test]
    fn ordinary_layout_sections_and_unrelated_code_do_not_block_selection() {
        for (prefix, suffix) in [
            (
                "@extends('layouts.app')\n@section('title', 'Hello')\n@section('content')\n",
                "\n@endsection",
            ),
            (
                "@extends('layouts.app')\n{{-- @bad('<broken>') --}}\n@section('content')\n{{ $user->name }}\n",
                "\n@endsection",
            ),
            ("<!-- <bad> -->\n@push('scripts')\n", "\n@endpush"),
            ("@if($show) <p>unrelated</p> @endif\n", ""),
            ("@php $greeting = 'Hello'; @endphp\n", ""),
        ] {
            let text = format!("{prefix}<section>Hello {{{{ $name }}}}</section>{suffix}");
            let (host, mut main, doc, selection) = setup(&text);
            let lazy = main
                .file_actions(&doc, selection)
                .into_iter()
                .find(|action| action.title.contains("Blade view"))
                .unwrap_or_else(|| panic!("{text}"));
            host.answer_next_prompt(Some(&["partials.card"]));
            host.answer_next_prompt(Some(&["yes"]));
            let fixed = main.resolve_file_action(&doc, lazy).unwrap();
            assert_eq!(fixed.edits[0].1[0].range, selection);
            assert_eq!(
                fixed.edits[0].1[0].text,
                "@include('partials.card', ['name' => $name])"
            );
            assert_eq!(
                host.invocation()
                    .project()
                    .unwrap()
                    .read_file(&doc.path)
                    .unwrap(),
                text
            );
            assert!(host.process_runs().is_empty());
        }
    }
    #[test]
    fn control_scopes_opaque_regions_directive_arguments_and_malformed_wrappers_are_withheld() {
        for (prefix, suffix) in [
            ("@verbatim", "@endverbatim"),
            ("@php $x = '", "'; @endphp"),
            ("{{-- ", " --}}"),
            ("<!-- ", " -->"),
            ("@include('", "')"),
            ("@section('content')", "@endpush"),
            ("@custom ", ""),
            ("@section($dynamic)", "@endsection"),
        ] {
            let text = format!("{prefix}<section>Hello {{{{ $name }}}}</section>{suffix}");
            let (_host, mut main, doc, selection) = setup(&text);
            assert!(
                main.file_actions(&doc, selection)
                    .iter()
                    .all(|action| !action.title.starts_with("Extract reviewed")),
                "{text}"
            );
        }
    }
    #[test]
    fn anonymous_component_extracts_explicit_props_as_one_create_and_source_edit() {
        let text = "😀\r\n@extends('layouts.app')\r\n@section('content')\r\n<section>Hello {{ $userName }}<b>{{ $title }}</b></section>\r\n@endsection";
        let (host, mut main, doc, selection) = setup(text);
        let lazy = main
            .file_actions(&doc, selection)
            .into_iter()
            .find(|action| action.title.contains("anonymous Blade component"))
            .unwrap();
        let replay = lazy.clone();
        host.answer_next_prompt(Some(&["forms.user-card"]));
        host.answer_next_prompt(Some(&["yes"]));
        let fixed = main.resolve_file_action(&doc, lazy).unwrap();
        assert_eq!(fixed.operations,vec![FileOperation::Create{
            path:"/project/resources/views/components/forms/user-card.blade.php".into(),
            text:"@props(['title', 'userName'])<section>Hello {{ $userName }}<b>{{ $title }}</b></section>".into(),
            overwrite:false,
        }]);
        assert_eq!(fixed.edits[0].1[0].range, selection);
        assert_eq!(
            fixed.edits[0].1[0].text,
            "<x-forms.user-card :title=\"$title\" :userName=\"$userName\" />"
        );
        assert_eq!(
            host.invocation()
                .project()
                .unwrap()
                .read_file(&doc.path)
                .unwrap(),
            text
        );
        assert!(
            host.invocation()
                .project()
                .unwrap()
                .read_file("resources/views/components/forms/user-card.blade.php")
                .is_err()
        );
        assert!(main.resolve_file_action(&doc, replay).is_err());
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn anonymous_component_collision_cancel_custom_props_and_nested_ownership_return_no_transaction()
     {
        for mode in 0..6 {
            let (host, mut main, mut doc, selection) =
                setup("<section>Hello {{ $name }}</section>");
            let lazy = main
                .file_actions(&doc, selection)
                .into_iter()
                .find(|action| action.title.contains("anonymous Blade component"))
                .unwrap();
            match mode {
                0 => host.answer_next_prompt(None),
                1 => {
                    host.answer_next_prompt(Some(&["card"]));
                    host.answer_next_prompt(None);
                }
                2 => {
                    host.add_file("resources/views/components/card.blade.php", "KEEP");
                    host.answer_next_prompt(Some(&["card"]));
                }
                3 => {
                    host.add_file("app/View/Components/Card.php", "KEEP");
                    host.answer_next_prompt(Some(&["card"]));
                }
                4 => {
                    host.add_project("/project/resources/views/components");
                    host.answer_next_prompt(Some(&["card"]));
                }
                _ => doc.version += 1,
            }
            assert!(main.resolve_file_action(&doc, lazy).is_err(), "{mode}");
            assert_eq!(
                host.invocation()
                    .project()
                    .unwrap()
                    .read_file(&doc.path)
                    .unwrap(),
                "<section>Hello {{ $name }}</section>"
            );
            assert!(host.process_runs().is_empty());
        }
        let (_host, mut main, doc, selection) = setup("<section>Hello {{ $name }}</section>");
        main.indexes.get_mut("/project").unwrap().directives.insert(
            "props".into(),
            crate::index::Entry {
                detail: None,
                path: "/project/app/Provider.php".into(),
                at: openidea_plugin::Position::new(0, 0),
                width: 5,
            },
        );
        assert!(
            main.file_actions(&doc, selection)
                .iter()
                .all(|action| !action.title.contains("anonymous Blade component"))
        );
    }
    #[test]
    fn overridden_wrapper_directive_and_incomplete_boundary_do_not_get_extraction() {
        let text = "@section('content')<section>Hello {{ $name }}</section>@endsection";
        let (_host, mut main, doc, selection) = setup(text);
        main.indexes.get_mut("/project").unwrap().directives.insert(
            "section".into(),
            crate::index::Entry {
                detail: None,
                path: "/project/app/Provider.php".into(),
                at: openidea_plugin::Position::new(0, 0),
                width: 7,
            },
        );
        assert!(
            main.file_actions(&doc, selection)
                .iter()
                .all(|action| !action.title.starts_with("Extract reviewed"))
        );
        for text in [
            "@section('content')<section>Hello</section>",
            "<div title=\"<section>Hello</section>\"></div>",
            "{{ '<section>Hello</section>' }}",
        ] {
            let (_host, mut main, doc, selection) = setup(text);
            assert!(
                main.file_actions(&doc, selection)
                    .iter()
                    .all(|action| !action.title.starts_with("Extract reviewed")),
                "{text}"
            );
        }
    }
    #[test]
    fn readonly_capture_grammar_rejects_mutation_and_implicit_slot_object_reads() {
        assert_eq!(
            captures("$user?->name ?? $fallback['label']").unwrap(),
            vec!["fallback", "user"]
        );
        for expression in [
            "++$x",
            "--$x",
            "$x++",
            "$x = 1",
            "$user->save($name)",
            "$user->{$method}()",
            "$this->title",
            "$$name",
            "$slot->attributes",
            "($slot)->attributes",
            "call($name)",
            "$loop->index",
        ] {
            assert!(captures(expression).is_none(), "{expression}");
        }
        assert_eq!(
            captures("$items->count() + $other?->count()").unwrap(),
            vec!["items", "other"]
        );
    }
    #[test]
    fn extraction_inside_control_scopes_explicitly_preserves_bound_read_values() {
        for (prefix, suffix) in [
            ("@foreach($users as $user)", "@endforeach"),
            ("@if($show)", "@else<p>Other</p>@endif"),
        ] {
            let source = format!(
                "{prefix}<section>{{{{ $user->name }}}} {{{{ $fallback['label'] }}}}</section>{suffix}"
            );
            let (host, mut main, doc, selection) = setup(&source);
            let lazy = main
                .file_actions(&doc, selection)
                .into_iter()
                .find(|a| a.title.contains("Blade view"))
                .unwrap();
            host.answer_next_prompt(Some(&["partials.name"]));
            host.answer_next_prompt(Some(&["yes"]));
            let fixed = main.resolve_file_action(&doc, lazy).unwrap();
            assert_eq!(
                fixed.edits[0].1[0].text,
                "@include('partials.name', ['fallback' => $fallback, 'user' => $user])"
            );
            let [FileOperation::Create { text, .. }] = fixed.operations.as_slice() else {
                panic!("create");
            };
            assert!(text.contains("$user->name"));
            assert!(text.contains("$fallback['label']"));
            assert!(host.process_runs().is_empty());
        }
    }
    #[test]
    fn extracted_component_forwards_default_slot_and_declares_only_explicit_data_props() {
        let source = "<section>{{ $title }}<div>{{ $slot }}</div></section>";
        let (host, mut main, doc, selection) = setup(source);
        let lazy = main
            .file_actions(&doc, selection)
            .into_iter()
            .find(|a| a.title.contains("anonymous Blade component"))
            .unwrap();
        host.answer_next_prompt(Some(&["card"]));
        host.answer_next_prompt(Some(&["yes"]));
        let fixed = main.resolve_file_action(&doc, lazy).unwrap();
        assert_eq!(
            fixed.edits[0].1[0].text,
            "<x-card :title=\"$title\">{{ $slot }}</x-card>"
        );
        let [FileOperation::Create { text, .. }] = fixed.operations.as_slice() else {
            panic!("create");
        };
        assert_eq!(
            text,
            "@props(['title'])<section>{{ $title }}<div>{{ $slot }}</div></section>"
        );
    }
    #[test]
    fn ordinary_fragment_within_literal_component_and_named_slot_wrappers_keeps_wrappers_in_caller()
    {
        let source = "<x-card :user=\"$user\"><x-slot:title><section>{{ $title }}</section></x-slot:title></x-card>";
        let (host, mut main, doc, selection) = setup(source);
        let lazy = main
            .file_actions(&doc, selection)
            .into_iter()
            .find(|action| action.title.contains("Blade view"))
            .unwrap();
        host.answer_next_prompt(Some(&["partials.title"]));
        host.answer_next_prompt(Some(&["yes"]));
        let fixed = main.resolve_file_action(&doc, lazy).unwrap();
        assert_eq!(fixed.edits[0].1[0].range, selection);
        assert_eq!(
            fixed.edits[0].1[0].text,
            "@include('partials.title', ['title' => $title])"
        );
        let mut rendered = doc.text.clone();
        rendered.replace_range(
            offset(&doc.text, selection.start).unwrap()..offset(&doc.text, selection.end).unwrap(),
            &fixed.edits[0].1[0].text,
        );
        assert!(rendered.starts_with("<x-card :user=\"$user\"><x-slot:title>"));
        assert!(rendered.ends_with("</x-slot:title></x-card>"));
    }
    #[test]
    fn partial_extract_can_forward_existing_attribute_bag_without_rebinding_component_props() {
        let source = "<section {{ $attributes }}>{{ $title }}</section>";
        let (_host, mut main, doc, selection) = setup(source);
        let actions = main.file_actions(&doc, selection);
        assert!(actions.iter().any(|a| a.title.contains("Blade view")));
        assert!(
            !actions
                .iter()
                .any(|a| a.title.contains("anonymous Blade component"))
        );
    }
    #[test]
    fn short_circuit_reads_do_not_eagerly_evaluate_missing_variables_at_the_call_site() {
        let source = "<section>{{ $user?->name ?? $fallback }}</section>";
        let (host, mut main, doc, selection) = setup(source);
        let actions = main.file_actions(&doc, selection);
        assert!(
            !actions
                .iter()
                .any(|action| action.title.contains("anonymous Blade component"))
        );
        let lazy = actions
            .into_iter()
            .find(|action| action.title.contains("Blade view"))
            .unwrap();
        host.answer_next_prompt(Some(&["partials.name"]));
        host.answer_next_prompt(Some(&["yes"]));
        let fixed = main.resolve_file_action(&doc, lazy).unwrap();
        assert_eq!(
            fixed.edits[0].1[0].text,
            "@include('partials.name', array_intersect_key(get_defined_vars(), array_flip(['fallback', 'user'])))"
        );
    }
    #[test]
    fn balanced_selected_directives_capture_conditions_without_eager_optional_reads() {
        let source = "@props(['title'])<section @class(['active' => $active])>@if($show)<b>{{ $title }}</b>@else<i>{{ $fallback ?? 'None' }}</i>@endif @csrf @method('PUT')</section>";
        let (host, mut main, doc, selection) = setup(source);
        let actions = main.file_actions(&doc, selection);
        assert!(
            !actions
                .iter()
                .any(|a| a.title.contains("anonymous Blade component"))
        );
        let lazy = actions
            .into_iter()
            .find(|a| a.title.contains("Blade view"))
            .unwrap();
        host.answer_next_prompt(Some(&["partials.conditional"]));
        host.answer_next_prompt(Some(&["yes"]));
        let fixed = main.resolve_file_action(&doc, lazy).unwrap();
        assert!(
            fixed.edits[0].1[0]
                .text
                .contains("array_flip(['active', 'fallback', 'show', 'title'])")
        );
        let [FileOperation::Create { text, .. }] = fixed.operations.as_slice() else {
            panic!("create");
        };
        assert!(text.contains("@if($show)"));
        assert!(text.contains("@class(['active' => $active])"));
        assert!(text.contains("@csrf"));
    }
    #[test]
    fn partial_directive_scopes_and_mutating_selected_expressions_are_refused() {
        for source in [
            "<section>@if($show){{ $name }}</section>@endif",
            "<section>@if($show = true){{ $name }}@endif</section>",
            "<section>@unknown($name)</section>",
            "<section>{{ ++$counter }}</section>",
        ] {
            let (_host, mut main, doc, selection) = setup(source);
            assert!(
                main.file_actions(&doc, selection)
                    .iter()
                    .all(|a| !a.title.starts_with("Extract reviewed")),
                "{source}"
            );
        }
    }
    #[test]
    fn selected_local_php_and_foreach_keep_bindings_inside_the_extracted_template() {
        for source in [
            "<section>@php $title = $name ?? 'Fallback'; @endphp<b>{{ $title }}</b></section>",
            "<section>@foreach($items as $key => $item)<b>{{ $item['name'] }}</b>@endforeach</section>",
        ] {
            let (host, mut main, doc, selection) = setup(source);
            let lazy = main
                .file_actions(&doc, selection)
                .into_iter()
                .find(|a| a.title.contains("Blade view"))
                .unwrap_or_else(|| panic!("{source}"));
            host.answer_next_prompt(Some(&["partials.content"]));
            host.answer_next_prompt(Some(&["yes"]));
            let fixed = main.resolve_file_action(&doc, lazy).unwrap();
            assert!(fixed.edits[0].1[0].text.contains("get_defined_vars()"));
            let [FileOperation::Create { text, .. }] = fixed.operations.as_slice() else {
                panic!("create");
            };
            assert_eq!(text, source);
        }
    }
    #[test]
    fn selected_writes_that_escape_or_mutate_runtime_bindings_are_refused() {
        for source in [
            "<section>@php $title = $name; @endphp{{ $title }}</section><p>{{ $title }}</p>",
            "<section>@php $__env = null; @endphp</section>",
            "@extends('layout')<section>@foreach($items as $item){{ $item }}@endforeach</section>",
            "<section>@foreach($items as &$item){{ $item }}@endforeach</section>",
        ] {
            let (_host, mut main, doc, selection) = setup(source);
            assert!(
                main.file_actions(&doc, selection)
                    .iter()
                    .all(|a| !a.title.starts_with("Extract reviewed")),
                "{source}"
            );
        }
    }
    #[test]
    #[ignore = "requires LARAVEL_EXTRACTION_FIXTURE disposable Laravel application and local PHP"]
    fn generated_extractions_preserve_real_blade_rendering_for_controls_locals_loops_and_slots() {
        let fixture =
            std::env::var("LARAVEL_EXTRACTION_FIXTURE").expect("disposable Laravel fixture");
        assert!(fixture.starts_with("/tmp/laravel-") && fixture.contains("acceptance"));
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let scratch = std::env::temp_dir().join(format!(
            "laravel-extraction-acceptance-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&scratch).unwrap();
        std::fs::create_dir(scratch.join("compiled")).unwrap();
        std::fs::create_dir(scratch.join("components")).unwrap();
        std::fs::write(
            scratch.join("components/parity-shell.blade.php"),
            "@props(['title'])<article><h1>{{ $title }}</h1>{{ $heading }}{{ $slot }}</article>",
        )
        .unwrap();
        let samples = [
            (
                "<section>@if($show)<b>{{ $title }}</b>@else<i>{{ $fallback ?? 'None' }}</i>@endif</section>",
                serde_json::json!({"show":false,"title":"Unused"}),
            ),
            (
                "<section>@php $title = $name ?? 'Guest'; @endphp<b>{{ $title }}</b></section>",
                serde_json::json!({"name":"Bonjour 😀"}),
            ),
            (
                "<section>@foreach($items as $item)<b>{{ $item['name'] }}</b>@endforeach</section>",
                serde_json::json!({"items":[{"name":"A"},{"name":"B"}]}),
            ),
            (
                "<section>{{ $items->count() }} {{ $items->first()->count() }}</section>",
                serde_json::json!({"collection":true,"items":[[1,2],[3]]}),
            ),
            (
                "<section>{{ $counter->next() }} {{ $counter->next() }}</section>",
                serde_json::json!({"counter":true}),
            ),
            (
                "<x-parity-shell :title=\"$title\"><x-slot:heading><b>{{ $heading }}</b></x-slot:heading><section>{{ $content }}</section></x-parity-shell>",
                serde_json::json!({"title":"Title","heading":"Heading","content":"Content"}),
            ),
        ];
        for (number, (source, data)) in samples.iter().enumerate() {
            let (host, mut main, doc, mut selection) = setup(source);
            if source.starts_with("<x-") {
                selection = Range::new(position(source, 0), position(source, source.len()));
            }
            let lazy = main
                .file_actions(&doc, selection)
                .into_iter()
                .find(|action| action.title.contains("Blade view"))
                .unwrap_or_else(|| panic!("no extraction: {source}"));
            host.answer_next_prompt(Some(&[&format!("partials.sample{number}")]));
            host.answer_next_prompt(Some(&["yes"]));
            let fixed = main.resolve_file_action(&doc, lazy).unwrap();
            let edit = &fixed.edits[0].1[0];
            let mut after = doc.text.clone();
            after.replace_range(
                offset(&doc.text, edit.range.start).unwrap()
                    ..offset(&doc.text, edit.range.end).unwrap(),
                &edit.text,
            );
            for operation in fixed.operations {
                let FileOperation::Create {
                    path,
                    text,
                    overwrite,
                } = operation
                else {
                    panic!("create");
                };
                assert!(!overwrite);
                let relative = path.strip_prefix("/project/resources/views/").unwrap();
                let target = scratch.join(relative);
                std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                std::fs::write(target, text).unwrap();
            }
            std::fs::write(scratch.join("before.blade.php"), source).unwrap();
            std::fs::write(scratch.join("after.blade.php"), after).unwrap();
            std::fs::write(scratch.join("data.json"), serde_json::to_vec(data).unwrap()).unwrap();
            let script = r#"require $argv[1].'/vendor/autoload.php';$app=require $argv[1].'/bootstrap/app.php';$app->make(\Illuminate\Contracts\Console\Kernel::class)->bootstrap();$app['config']->set('view.compiled',$argv[2].'/compiled');$view=$app->make('view');$view->getFinder()->setPaths([$argv[2]]);$data=json_decode(file_get_contents($argv[2].'/data.json'),true,512,JSON_THROW_ON_ERROR);if($data['collection']??false){$data['items']=collect($data['items'])->map(fn($row)=>collect($row));unset($data['collection']);}if($data['counter']??false){$data['counter']=new class{public int $value=0;public function next(){return ++$this->value;}};}$afterData=$data;if(isset($data['counter'])){$afterData['counter']=clone $data['counter'];}$before=$view->file($argv[2].'/before.blade.php',$data)->render();$after=$view->file($argv[2].'/after.blade.php',$afterData)->render();echo json_encode(['before'=>$before,'after'=>$after],JSON_THROW_ON_ERROR);"#;
            let result = std::process::Command::new("php")
                .arg("-r")
                .arg(script)
                .arg(&fixture)
                .arg(&scratch)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "case {number}: {}\n{}",
                String::from_utf8_lossy(&result.stderr),
                String::from_utf8_lossy(&result.stdout)
            );
            let rendered: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
            assert_eq!(rendered["before"], rendered["after"], "case {number}");
            // Every case gets its own compiler process; remove two caller cache files to avoid second-resolution mtime reuse.
            for file in std::fs::read_dir(scratch.join("compiled")).unwrap() {
                std::fs::remove_file(file.unwrap().path()).unwrap();
            }
        }
        std::fs::remove_dir_all(&scratch).unwrap();
    }
}
