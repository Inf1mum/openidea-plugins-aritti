//! Reading PHP without a parser: comments, strings, brackets and array
//! literals. Everything works on byte offsets of the original text.

use std::ops::Range;

pub fn is_identifier(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The index after the string literal whose opening quote is at `start`
/// (the end of the text when it isn't closed).
pub fn string_end(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut i = start + 1;

    while i < bytes.len() {
        if bytes[i] == quote {
            return i + 1;
        }
        i += if bytes[i] == b'\\' { 2 } else { 1 };
    }

    bytes.len()
}

/// The text with every comment blanked out, so offsets stay the same.
pub fn mask_comments(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = bytes.to_vec();
    let mut blank = |range: Range<usize>| {
        for b in &mut out[range] {
            if *b != b'\n' {
                *b = b' ';
            }
        }
    };
    let mut i = 0;

    while i < bytes.len() {
        let next = bytes.get(i + 1).copied();
        match bytes[i] {
            b'\'' | b'"' => i = string_end(bytes, i).min(bytes.len()),
            b'/' if next == Some(b'/') => {
                let end = text[i..].find('\n').map_or(bytes.len(), |n| i + n);
                blank(i..end);
                i = end;
            }
            b'#' if next != Some(b'[') => {
                let end = text[i..].find('\n').map_or(bytes.len(), |n| i + n);
                blank(i..end);
                i = end;
            }
            b'/' if next == Some(b'*') => {
                let end = text[i + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |n| i + 2 + n + 2);
                blank(i..end);
                i = end;
            }
            _ => i += 1,
        }
    }

    String::from_utf8(out).unwrap_or_else(|_| text.to_owned())
}

/// End of PHP comment trivia, called only outside a quoted string. Preserve
/// source coordinates and do not treat PHP8 attributes as hash comments.
fn comment_end(bytes: &[u8], at: usize) -> Option<usize> {
    if bytes[at..].starts_with(b"/*") {
        return Some(
            bytes[at + 2..]
                .windows(2)
                .position(|p| p == b"*/")
                .map_or(bytes.len(), |n| at + 2 + n + 2),
        );
    }
    if bytes[at..].starts_with(b"//") || bytes[at] == b'#' && bytes.get(at + 1) != Some(&b'[') {
        return Some(
            bytes[at..]
                .iter()
                .position(|b| matches!(b, b'\r' | b'\n'))
                .map_or(bytes.len(), |n| at + n),
        );
    }
    None
}

/// The bracket closing the `(`, `[` or `{` at `open`, skipping strings/comments.
pub fn closing(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut i = open;

    while i < bytes.len() {
        if let Some(end) = comment_end(bytes, i) {
            i = end;
            continue;
        }
        match bytes[i] {
            b'\'' | b'"' => {
                i = string_end(bytes, i);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }

    None
}

/// `range` without the whitespace around it.
pub fn trimmed(text: &str, range: Range<usize>) -> Range<usize> {
    let slice = &text[range.clone()];
    let start = range.start + (slice.len() - slice.trim_start().len());
    let end = range.end - (slice.len() - slice.trim_end().len());
    start..end.max(start)
}

/// Pieces of `range` between commas that aren't inside brackets or strings.
pub fn split(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = range.start;
    let mut i = range.start;

    while i < range.end {
        if let Some(end) = comment_end(bytes, i) {
            i = end.min(range.end);
            continue;
        }
        match bytes[i] {
            b'\'' | b'"' => {
                i = string_end(bytes, i).min(range.end);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                parts.push(start..i);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }

    if !text[start..range.end].trim().is_empty() {
        parts.push(start..range.end);
    }

    parts
}

struct Import {
    fqcn: String,
    alias: String,
}

pub struct UseEdit {
    pub offset: usize,
    pub text: String,
    pub short_name: String,
}

fn short_name(fqcn: &str) -> &str {
    fqcn.rsplit('\\').next().unwrap_or(fqcn)
}

/// Same length as `text`: comments and string contents become spaces, line breaks stay.
fn mask_all(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = bytes.to_vec();
    let blank = |out: &mut Vec<u8>, range: Range<usize>| {
        for byte in &mut out[range] {
            if *byte != b'\n' && *byte != b'\r' {
                *byte = b' ';
            }
        }
    };

    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        let next = bytes.get(i + 1).copied();
        match byte {
            b'/' if next == Some(b'*') => {
                let end = text[i + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |at| i + 2 + at + 2);
                blank(&mut out, i..end);
                i = end;
            }
            b'/' if next == Some(b'/') => {
                let end = text[i..].find('\n').map_or(bytes.len(), |at| i + at);
                blank(&mut out, i..end);
                i = end;
            }
            b'#' if next != Some(b'[') => {
                let end = text[i..].find('\n').map_or(bytes.len(), |at| i + at);
                blank(&mut out, i..end);
                i = end;
            }
            b'\'' | b'"' => {
                let mut end = i + 1;
                while end < bytes.len() && bytes[end] != byte {
                    end += if bytes[end] == b'\\' { 2 } else { 1 };
                }
                let end = end.min(bytes.len());
                blank(&mut out, i + 1..end);
                i = (end + 1).min(bytes.len());
            }
            _ => i += 1,
        }
    }

    String::from_utf8(out).unwrap_or_default()
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(byte, b'_' | b'\\' | b'$' | b'?' | b'|' | b'&')
        || byte >= 0x80
}

fn import_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && (bytes[0].is_ascii_alphabetic() || bytes[0] == b'_' || bytes[0] >= 128)
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b >= 128)
}
fn push_import(imports: &mut Vec<Import>, item: &str, prefix: &str) -> bool {
    let item = item.trim();
    if item.is_empty() {
        return true;
    }
    let words = item.split_whitespace().collect::<Vec<_>>();
    if words.first().is_some_and(|word| {
        word.eq_ignore_ascii_case("function") || word.eq_ignore_ascii_case("const")
    }) {
        return true;
    }
    let (name, alias) = match words.as_slice() {
        [name] => (*name, None),
        [name, keyword, alias] if keyword.eq_ignore_ascii_case("as") => (*name, Some(*alias)),
        _ => return false,
    };
    let fqcn = format!("{prefix}{}", name.trim_start_matches('\\'));
    let alias = alias.unwrap_or_else(|| short_name(&fqcn));
    if !fqcn.split('\\').all(import_identifier) || !import_identifier(alias) {
        return false;
    }
    imports.push(Import {
        alias: alias.into(),
        fqcn,
    });
    true
}

fn keyword_prefix<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let (head, tail) = (line.get(..keyword.len())?, line.get(keyword.len()..)?);
    (head.eq_ignore_ascii_case(keyword) && tail.chars().next().is_none_or(char::is_whitespace))
        .then_some(tail)
}
/// Named type declarations occupy the same case-insensitive PHP alias space as
/// class imports. Quoted examples, comments and `Foo::class` are not declarations.
fn declares_class_in(masked: &str, name: &str) -> bool {
    let bytes = masked.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if !(bytes[at].is_ascii_alphabetic() || bytes[at] == b'_') {
            at += 1;
            continue;
        }
        let start = at;
        at += 1;
        while at < bytes.len()
            && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_' || bytes[at] >= 128)
        {
            at += 1;
        }
        let word = &masked[start..at];
        if !["class", "interface", "trait", "enum"]
            .iter()
            .any(|keyword| word.eq_ignore_ascii_case(keyword))
            || masked[..start].trim_end().ends_with("::")
        {
            continue;
        }
        if !bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            continue;
        }
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        let begin = at;
        while at < bytes.len()
            && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_' || bytes[at] >= 128)
        {
            at += 1;
        }
        if masked[begin..at].eq_ignore_ascii_case(name) {
            return true;
        }
    }
    false
}
pub(crate) fn declares_class(source: &str, name: &str) -> bool {
    declares_class_in(&mask_all(source), name)
}

/// Imports with the byte offset where each statement's line ends, and the
/// offset after the `namespace` line.
struct Header {
    imports: Vec<(Import, usize)>,
    namespace: Option<(String, usize)>,
    ambiguous: bool,
}

fn line_end(masked: &str, from: usize) -> usize {
    masked[from..]
        .find('\n')
        .map_or(masked.len(), |at| from + at + 1)
}

fn statement_insertion(masked: &str, end: usize) -> usize {
    let line = line_end(masked, end.saturating_sub(1));
    if masked[end..line].trim().is_empty() {
        line
    } else {
        end
    }
}
fn scan_header(masked: &str) -> Header {
    let mut header = Header {
        imports: Vec::new(),
        namespace: None,
        ambiguous: false,
    };
    let bytes = masked.as_bytes();
    let mut at = masked.find("<?php").map_or(0, |at| at + 5);
    let mut depth = 0usize;
    let mut code_seen = false;
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if matches!(bytes[at], b'\'' | b'"') {
            at = string_end(bytes, at);
            continue;
        }
        if bytes[at] == b'{' {
            depth += 1;
            at += 1;
            continue;
        }
        if bytes[at] == b'}' {
            depth = depth.saturating_sub(1);
            at += 1;
            continue;
        }
        if !bytes[at].is_ascii_alphabetic() && bytes[at] != b'_' && bytes[at] < 128 {
            at += 1;
            continue;
        }
        let begin = at;
        while at < bytes.len()
            && (bytes[at].is_ascii_alphanumeric()
                || matches!(bytes[at], b'_' | b'\\')
                || bytes[at] >= 128)
        {
            at += 1;
        }
        if depth != 0 {
            continue;
        }
        let word = &masked[begin..at];
        if word.eq_ignore_ascii_case("namespace") {
            let Some(end) = masked[at..].find([';', '{']).map(|n| at + n) else {
                header.ambiguous = true;
                break;
            };
            let name = masked[at..end].trim();
            if code_seen
                || header.namespace.is_some()
                || bytes[end] == b'{'
                || !name.split('\\').all(import_identifier)
            {
                header.ambiguous = true;
            }
            header.namespace = Some((name.into(), statement_insertion(masked, end + 1)));
            at = end + 1;
            continue;
        }
        if word.eq_ignore_ascii_case("use") {
            let next = masked[at..].trim_start();
            // A closure capture is not a namespace import.
            if next.starts_with('(') {
                continue;
            }
            let Some(end) = masked[at..].find(';').map(|n| at + n) else {
                header.ambiguous = true;
                break;
            };
            if code_seen {
                header.ambiguous = true;
            }
            let statement = masked[at..end].trim();
            let mut imports = Vec::new();
            if keyword_prefix(statement, "function").is_none()
                && keyword_prefix(statement, "const").is_none()
            {
                if let Some((prefix, rest)) = statement.split_once('{') {
                    if !rest.trim_end().ends_with('}')
                        || rest[..rest.trim_end().len() - 1].contains(['{', '}'])
                    {
                        header.ambiguous = true;
                    }
                    let prefix = prefix.trim().trim_start_matches('\\');
                    for item in rest.trim_end().trim_end_matches('}').split(',') {
                        header.ambiguous |= !push_import(&mut imports, item, prefix);
                    }
                } else {
                    for item in statement.split(',') {
                        header.ambiguous |= !push_import(&mut imports, item, "");
                    }
                }
            }
            let insertion = statement_insertion(masked, end + 1);
            header
                .imports
                .extend(imports.into_iter().map(|import| (import, insertion)));
            at = end + 1;
            continue;
        }
        if word.eq_ignore_ascii_case("declare") && !code_seen {
            let Some(end) = declaration_statement_end(masked, at) else {
                header.ambiguous = true;
                break;
            };
            at = end;
            continue;
        }
        code_seen = true;
    }
    header
}

pub fn insert_use(text: &str, fqcn: &str) -> Option<UseEdit> {
    let masked = mask_all(text);
    let header = scan_header(&masked);
    if header.ambiguous {
        return None;
    }
    let fqcn = fqcn.trim_start_matches('\\');
    let short = short_name(fqcn);

    if let Some((import, _)) = header
        .imports
        .iter()
        .find(|(import, _)| import.fqcn.eq_ignore_ascii_case(fqcn))
    {
        if declares_class_in(&masked, &import.alias)
            || header
                .imports
                .iter()
                .filter(|(other, _)| other.alias.eq_ignore_ascii_case(&import.alias))
                .count()
                != 1
        {
            return None;
        }
        return Some(UseEdit {
            offset: 0,
            text: String::new(),
            short_name: import.alias.clone(),
        });
    }

    if declares_class_in(&masked, short)
        || header
            .imports
            .iter()
            .any(|(import, _)| import.alias.eq_ignore_ascii_case(short))
    {
        return None;
    }

    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let line = format!("use {fqcn};{eol}");
    let (offset, text) = if let Some((_, end)) = header.imports.last() {
        (*end, line)
    } else if let Some((_, end)) = &header.namespace {
        (*end, format!("{eol}{line}"))
    } else {
        let opening = masked.find("<?php")? + 5;
        let first_line = line_end(&masked, opening);
        let start = if masked[opening..first_line].trim().is_empty() {
            first_line
        } else {
            opening
        };
        match declare_end(&masked, start) {
            Some(end) => (end, format!("{eol}{line}")),
            None => (
                start,
                if start == opening {
                    format!("{eol}{line}")
                } else {
                    line
                },
            ),
        }
    };

    Some(UseEdit {
        offset,
        text,
        short_name: short.to_owned(),
    })
}

/// A controller literal with namespace separators denotes an absolute PHP class
/// name at runtime, independently of the namespace of its route file. Reuse a
/// proven import when possible; otherwise emit an explicit absolute reference.
/// Legacy short names retain the existing imported/configured-namespace subset.
pub(crate) fn controller_reference(
    text: &str,
    class: &str,
    controllers_namespace: &str,
) -> Option<String> {
    let absolute = class.starts_with('\\');
    let target = class.trim_start_matches('\\');
    if !absolute
        && ["self", "static", "parent"]
            .iter()
            .any(|name| target.eq_ignore_ascii_case(name))
    {
        return None;
    }
    if !target.split('\\').all(import_identifier) || class.starts_with("\\\\") {
        return None;
    }
    let header = scan_header(&mask_all(text));
    if header.ambiguous {
        return None;
    }
    let safe_alias = |alias: &str| {
        !declares_class(text, alias)
            && header
                .imports
                .iter()
                .filter(|(item, _)| item.alias.eq_ignore_ascii_case(alias))
                .count()
                == 1
    };
    if absolute || target.contains('\\') {
        // Full imports take precedence over namespace-prefix imports.
        if let Some((item, _)) = header
            .imports
            .iter()
            .find(|(item, _)| item.fqcn.eq_ignore_ascii_case(target) && safe_alias(&item.alias))
        {
            return Some(item.alias.clone());
        }
        if let Some((item, _)) = header
            .imports
            .iter()
            .filter(|(item, _)| safe_alias(&item.alias))
            .filter(|(item, _)| {
                target
                    .get(..item.fqcn.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(&item.fqcn))
                    && target.as_bytes().get(item.fqcn.len()) == Some(&b'\\')
            })
            .max_by_key(|(item, _)| item.fqcn.len())
        {
            return Some(format!("{}{}", item.alias, &target[item.fqcn.len()..]));
        }
        return Some(format!("\\{target}"));
    }
    if let Some((item, _)) = header
        .imports
        .iter()
        .find(|(item, _)| item.alias.eq_ignore_ascii_case(target) && safe_alias(&item.alias))
    {
        return Some(item.alias.clone());
    }
    header
        .namespace
        .filter(|(namespace, _)| {
            namespace.eq_ignore_ascii_case(controllers_namespace.trim_matches('\\'))
        })
        .map(|_| target.to_owned())
}

/// Offset after the line of a `declare(...);` statement that opens the file at `from`.
fn declare_end(masked: &str, from: usize) -> Option<usize> {
    let start = skip_whitespace(masked, from);
    let word = read_word(masked, start);
    if !masked[word.clone()].eq_ignore_ascii_case("declare") {
        return None;
    }

    let end = declaration_statement_end(masked, word.end)?;
    Some(statement_insertion(masked, end))
}

fn declaration_statement_end(masked: &str, after_word: usize) -> Option<usize> {
    let open = skip_whitespace(masked, after_word);
    if masked.as_bytes().get(open) != Some(&b'(') {
        return None;
    }
    let close = closing(masked, open)?;
    let end = skip_whitespace(masked, close + 1);
    (masked.as_bytes().get(end) == Some(&b';')).then_some(end + 1)
}

fn skip_whitespace(masked: &str, mut at: usize) -> usize {
    let bytes = masked.as_bytes();
    while at < bytes.len() && bytes[at].is_ascii_whitespace() {
        at += 1;
    }
    at
}

fn read_word(masked: &str, at: usize) -> Range<usize> {
    let bytes = masked.as_bytes();
    let mut end = at;
    while end < bytes.len() && is_word_byte(bytes[end]) {
        end += 1;
    }
    at..end
}

/// Arguments of the call whose `(` is at `open`.
pub fn arguments(text: &str, open: usize) -> Vec<Range<usize>> {
    match closing(text, open) {
        Some(close) => split(text, open + 1..close),
        None => Vec::new(),
    }
}

/// Contents and offset of `range` when it is exactly one string literal.
pub fn literal(text: &str, range: Range<usize>) -> Option<(&str, usize)> {
    let slice = text.get(range.clone())?;
    let range = if slice.contains(['/', '#']) {
        let masked = mask_comments(slice);
        let content = trimmed(&masked, 0..masked.len());
        range.start + content.start..range.start + content.end
    } else {
        trimmed(text, range)
    };
    let bytes = text.as_bytes();
    let quote = *bytes.get(range.start)?;

    if (quote != b'\'' && quote != b'"')
        || string_end(bytes, range.start) != range.end
        || range.len() < 2
    {
        return None;
    }

    Some((&text[range.start + 1..range.end - 1], range.start + 1))
}

/// `App\Foo::class` of `range`: `App\Foo`.
pub fn class_constant(text: &str, range: Range<usize>) -> Option<&str> {
    let range = trimmed(text, range);
    text[range]
        .strip_suffix("::class")
        .map(|class| class.trim_start_matches('\\'))
}

/// Index of a top-level `=>` in `range`.
fn arrow(text: &str, range: Range<usize>) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut i = range.start;

    while i < range.end {
        if let Some(end) = comment_end(bytes, i) {
            i = end.min(range.end);
            continue;
        }
        match bytes[i] {
            b'\'' | b'"' => {
                i = string_end(bytes, i).min(range.end);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'=' if depth == 0 && bytes.get(i + 1) == Some(&b'>') => return Some(i),
            _ => {}
        }
        i += 1;
    }

    None
}

pub struct Item {
    pub key: Option<Range<usize>>,
    pub value: Range<usize>,
}

/// The bracket opening the array literal `range` is, if it is one.
pub fn array_open(text: &str, range: Range<usize>) -> Option<usize> {
    let range = trimmed(text, range);
    let slice = &text[range.clone()];

    if slice.starts_with('[') {
        return Some(range.start);
    }

    let head = slice.get(..5)?;
    let rest = slice[5..].trim_start();
    (head.eq_ignore_ascii_case("array") && rest.starts_with('(')).then(|| range.end - rest.len())
}

/// Elements of the array literal opened by the bracket at `open`.
pub fn items(text: &str, open: usize) -> Vec<Item> {
    let Some(close) = closing(text, open) else {
        return Vec::new();
    };

    split(text, open + 1..close)
        .into_iter()
        .map(|part| match arrow(text, part.clone()) {
            Some(at) => Item {
                key: Some(part.start..at),
                value: at + 2..part.end,
            },
            None => Item {
                key: None,
                value: part,
            },
        })
        .collect()
}

/// Value of `key` in the array literal opened at `open`, when it is a string.
pub fn string_value<'a>(text: &'a str, open: usize, key: &str) -> Option<(&'a str, usize)> {
    items(text, open)
        .into_iter()
        .find(|item| {
            item.key
                .clone()
                .and_then(|k| literal(text, k))
                .is_some_and(|(name, _)| name == key)
        })
        .and_then(|item| literal(text, item.value))
}

/// Strings of the array literal opened at `open` (keys when `keys`).
pub fn strings(text: &str, open: usize, keys: bool) -> Vec<String> {
    items(text, open)
        .into_iter()
        .filter_map(|item| {
            let range = if keys { item.key? } else { item.value };
            literal(text, range).map(|(s, _)| s.to_owned())
        })
        .collect()
}

/// The array a file returns: `return [` or `return array(`.
pub fn returned_array(text: &str) -> Option<usize> {
    text.match_indices("return").find_map(|(at, _)| {
        if at > 0 && is_identifier(text.as_bytes()[at - 1]) {
            return None;
        }

        let trimmed = text[at + 6..].trim_start();
        let start = text.len() - trimmed.len();
        if start == at + 6 && !trimmed.starts_with('[') {
            return None;
        }

        array_open(text, start..text.len())
    })
}

/// A key of a nested array: its dotted path, where the key name starts and
/// its value when that is a plain string.
pub struct Leaf {
    pub path: String,
    pub offset: usize,
    pub width: usize,
    pub value: Option<String>,
    /// Source expression range, independent of literal-value inference.
    pub expression: Range<usize>,
    /// The value is an array literal.
    pub array: bool,
}

/// Dotted paths of every string key of the array opened at `open`.
pub fn array_paths(text: &str, open: usize, prefix: &str, out: &mut Vec<Leaf>, depth: usize) {
    if depth > 8 {
        return;
    }

    for item in items(text, open) {
        let Some((key, offset)) = item.key.and_then(|k| literal(text, k)) else {
            continue;
        };
        if key.is_empty() {
            continue;
        }

        let path = if prefix.is_empty() {
            key.to_owned()
        } else {
            format!("{prefix}.{key}")
        };
        let value = literal(text, item.value.clone()).map(|(v, _)| v.to_owned());
        let expression = trimmed(text, item.value.clone());
        let nested = array_open(text, item.value);
        out.push(Leaf {
            path: path.clone(),
            offset,
            width: key.len(),
            value,
            expression,
            array: nested.is_some(),
        });

        if let Some(nested) = nested {
            array_paths(text, nested, &path, out, depth + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_use_goes_after_the_last_import() {
        let text = "<?php\nnamespace App;\n\nuse A\\B;\nuse A\\C;\n\nclass X {}\n";
        let edit = insert_use(text, "Foo\\Bar").unwrap();
        assert_eq!(edit.text, "use Foo\\Bar;\n");
        assert_eq!(
            &text[..edit.offset],
            "<?php\nnamespace App;\n\nuse A\\B;\nuse A\\C;\n"
        );
        assert_eq!(edit.short_name, "Bar");
    }

    #[test]
    fn insert_use_refuses_a_short_name_clash() {
        assert!(insert_use("<?php\nuse Other\\Bar;\n", "Foo\\Bar").is_none());
    }

    #[test]
    fn insert_use_handles_namespace_declare_crlf_and_existing_imports() {
        let text = "<?php\nnamespace App;\n\nclass X {}\n";
        let edit = insert_use(text, "Foo\\Bar").unwrap();
        assert_eq!(edit.text, "\nuse Foo\\Bar;\n");
        assert_eq!(&text[..edit.offset], "<?php\nnamespace App;\n");

        let text = "<?php\ndeclare(strict_types=1);\n\nclass X {}\n";
        let edit = insert_use(text, "Foo\\Bar").unwrap();
        assert_eq!(&text[..edit.offset], "<?php\ndeclare(strict_types=1);\n");

        let text = "<?php\r\nnamespace App;\r\n\r\nuse A\\B;\r\n\r\nclass X {}\r\n";
        assert_eq!(
            insert_use(text, "Foo\\Bar").unwrap().text,
            "use Foo\\Bar;\r\n"
        );

        let edit = insert_use("<?php\nuse Foo\\Bar;\n", "\\Foo\\Bar").unwrap();
        assert_eq!(edit.text, "");
    }

    #[test]
    fn masks_comments_but_not_strings() {
        let text = "a('//x'); // c\n# d\n#[Attr]\n/* e\nf */ g";
        let masked = mask_comments(text);
        assert_eq!(masked.len(), text.len());
        assert_eq!(
            masked.lines().map(str::trim_end).collect::<Vec<_>>(),
            ["a('//x');", "", "#[Attr]", "", "     g"]
        );
    }

    #[test]
    fn splits_arguments_at_the_top_level() {
        let text = "f('a,b', [1, 2], g(3, 4), 'c')";
        let args: Vec<_> = arguments(text, 1).into_iter().map(|r| &text[r]).collect();
        assert_eq!(args, ["'a,b'", " [1, 2]", " g(3, 4)", " 'c'"]);
    }

    #[test]
    fn reads_array_literals() {
        let text = "['as' => 'admin.', 'uses' => 'A@b', 'only' => ['index', 'show'], 'x', array('k' => 1)]";
        assert_eq!(string_value(text, 0, "as").map(|(v, _)| v), Some("admin."));
        assert_eq!(string_value(text, 0, "uses").map(|(v, _)| v), Some("A@b"));
        let only = items(text, 0)
            .into_iter()
            .find(|i| i.key.clone().is_some_and(|k| text[k].contains("only")))
            .unwrap();
        assert_eq!(
            strings(text, array_open(text, only.value).unwrap(), false),
            ["index", "show"]
        );
    }

    #[test]
    fn collects_nested_paths_of_a_returned_array() {
        let text = "<?php\n\nreturn [\n    'name' => env('APP_NAME', 'Laravel'),\n    'db' => [\n        'host' => 'localhost',\n    ],\n];\n";
        let open = returned_array(text).unwrap();
        let mut leaves = Vec::new();
        array_paths(text, open, "app", &mut leaves, 0);
        let paths: Vec<_> = leaves
            .iter()
            .map(|l| (l.path.as_str(), l.value.as_deref()))
            .collect();
        assert_eq!(
            paths,
            [
                ("app.name", None),
                ("app.db", None),
                ("app.db.host", Some("localhost"))
            ]
        );
        assert_eq!(
            &text[leaves[2].offset..leaves[2].offset + leaves[2].width],
            "host"
        );
    }

    #[test]
    fn survives_half_typed_code() {
        let text = "<?php return ['a' => ['b' => 'c', 'd' => f('e', [ ";
        for end in 0..=text.len() {
            let prefix = &text[..end];
            let _ = mask_comments(prefix);
            if let Some(open) = returned_array(prefix) {
                array_paths(prefix, open, "", &mut Vec::new(), 0);
            }
        }
    }
    #[test]
    fn argument_delimiters_and_literal_spans_ignore_php_comment_trivia() {
        let source = "f(/* 😀 ) , => */ 'a/*notcomment*/', // ) ,\r\n 'b#notcomment' /* , */)";
        let open = source.find('(').unwrap();
        assert_eq!(closing(source, open), Some(source.len() - 1));
        let parts = arguments(source, open);
        assert_eq!(parts.len(), 2);
        for (part, expected) in parts.into_iter().zip(["a/*notcomment*/", "b#notcomment"]) {
            let (value, offset) = literal(source, part).unwrap();
            assert_eq!(value, expected);
            assert_eq!(&source[offset..offset + expected.len()], expected);
        }
        let attributed = "f(#[Example(') ,')] fn()=>1, 'last')";
        assert_eq!(closing(attributed, 1), Some(attributed.len() - 1));
        assert_eq!(arguments(attributed, 1).len(), 2);
    }
    #[test]
    fn array_arrows_in_comments_do_not_define_keys_and_computed_literals_stay_refused() {
        let source = "[/* => , 'fake' */ 'real' /* => */ => /* value */ 'text' /* , */]";
        let items = items(source, 0);
        assert_eq!(items.len(), 1);
        assert_eq!(
            literal(source, items[0].key.clone().unwrap()).unwrap().0,
            "real"
        );
        assert_eq!(literal(source, items[0].value.clone()).unwrap().0, "text");
        let computed = "'one' /* comment */ . 'two'";
        assert!(literal(computed, 0..computed.len()).is_none());
        assert!(closing("f(/* unterminated )", 1).is_none());
    }
}

#[cfg(test)]
mod import_safety_tests {
    use super::*;
    fn applied(source: &str, target: &str) -> String {
        let edit = insert_use(source, target).expect("safe import");
        let mut result = source.to_owned();
        result.insert_str(edit.offset, &edit.text);
        result
    }
    #[test]
    fn named_declarations_reserve_case_insensitive_aliases_but_examples_do_not() {
        for declaration in [
            "class rule {}",
            "CLASS RULE {}",
            "interface RuLe {}",
            "trait rUlE {}",
            "enum RULE {}",
        ] {
            let source = format!("<?php\nnamespace App;\n{declaration}\n");
            assert!(declares_class(&source, "Rule"), "{source}");
            assert!(
                insert_use(&source, "Illuminate\\Validation\\Rule").is_none(),
                "{source}"
            );
        }
        for source in [
            "<?php\nnamespace App;\n$x='class Rule {}';\n",
            "<?php\nnamespace App;\n// class Rule {}\n",
            "<?php\nnamespace App;\n$x=Rule::class;\n",
            "<?php\nnamespace App;\nclass Ruleé {}\n",
        ] {
            assert!(!declares_class(source, "Rule"), "{source}");
            assert!(
                insert_use(source, "Illuminate\\Validation\\Rule").is_some(),
                "{source}"
            );
        }
    }
    #[test]
    fn uppercase_multiline_grouped_and_case_variant_imports_reuse_the_exact_alias() {
        for source in [
            "<?php\nNAMESPACE App;\nUSE Illuminate\\Validation\\Rule AS ValidationRule;\nclass A {}\n",
            "<?php\nnamespace App;\nuse illuminate\\validation\\rule as ValidationRule;\nclass A {}\n",
            "<?php\nnamespace App;\nUSE\n Illuminate\\Validation\\Rule AS\n ValidationRule;\nclass A {}\n",
            "<?php\nnamespace App;\nUSE Illuminate\\Validation\\{Rule AS ValidationRule, Rules\\File};\nclass A {}\n",
        ] {
            let edit = insert_use(source, "Illuminate\\Validation\\Rule").unwrap();
            assert!(edit.text.is_empty(), "{}", edit.text);
            assert_eq!(edit.short_name, "ValidationRule");
        }
        for source in [
            "<?php\nUSE Other\\Thing AS rUlE;\n",
            "<?php\nuse A\\B; USE Other\\Thing AS RULE;\n",
        ] {
            assert!(insert_use(source, "Illuminate\\Validation\\Rule").is_none());
        }
    }
    #[test]
    fn inline_headers_insert_before_code_and_ambiguous_scopes_are_withheld() {
        for source in [
            "<?php namespace App; class A {public function run(){return Rule::in(['a']);}}",
            "<?php\nnamespace App; use Foo\\Bar; class A {public function run(){return Rule::in(['a']);}}",
            "<?php $rule=Rule::in(['a']);",
            "<?php declare(strict_types=1); $rule=Rule::in(['a']);",
        ] {
            let result = applied(source, "Illuminate\\Validation\\Rule");
            assert!(
                result.find("use Illuminate\\Validation\\Rule;").unwrap()
                    < result.find("Rule::in").unwrap(),
                "{result}"
            );
        }
        for source in [
            "<?php\nnamespace A; class One {}\nnamespace B; class Two {}",
            "<?php\nnamespace A {class One {}}",
        ] {
            assert!(insert_use(source, "Foo\\Bar").is_none(), "{source}");
        }
    }
    #[test]
    fn block_and_alternate_declare_scopes_do_not_receive_namespace_imports() {
        for text in [
            "<?php declare(ticks=1) { consume(); }",
            "<?php declare(ticks=1): consume(); enddeclare;",
        ] {
            assert!(
                insert_use(text, "Illuminate\\Validation\\Rule").is_none(),
                "{text}"
            );
        }
    }

    #[test]
    fn aliases_may_coexist_with_a_different_local_short_name_but_not_the_alias_itself() {
        let source = "<?php\nnamespace App;\nuse Illuminate\\Validation\\Rule as ValidationRule;\nclass Rule {}";
        let edit = insert_use(source, "Illuminate\\Validation\\Rule").unwrap();
        assert_eq!(edit.short_name, "ValidationRule");
        let collision = source.replace("class Rule", "class validationrule");
        assert!(insert_use(&collision, "Illuminate\\Validation\\Rule").is_none());
    }
    #[test]
    #[ignore = "requires php executable; lints actual rendered import edits"]
    fn actual_import_edits_lint_with_php() {
        for source in [
            "<?php namespace App; class A {public function run(){return Rule::in(['a']);}}",
            "<?php\nnamespace App; use Foo\\Bar; class A {public function run(){return Rule::in(['a']);}}",
            "<?php $rule=Rule::in(['a']);",
            "<?php declare(strict_types=1); $rule=Rule::in(['a']);",
        ] {
            let result = applied(source, "Illuminate\\Validation\\Rule");
            let mut child = std::process::Command::new("php")
                .arg("-l")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(result.as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{result}\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
