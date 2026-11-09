//! Cheap current-text literal/context proofs, not PHP type or runtime identity.
use crate::{Document, Reference, refs};
use std::ops::Range;
pub(crate) const MAX_TEXT: usize = 128 << 10;

/// Literal ranges in actual Twig code; raw bodies and comments stay opaque.
pub(crate) fn twig_strings(text: &str) -> Option<Vec<Range<usize>>> {
    if text.len() > MAX_TEXT {
        return None;
    }
    let mut out = Vec::new();
    let mut cursor = 0;
    let mut count = 0;
    while cursor < text.len() {
        let Some(next) = text[cursor..].find(['{', '<']) else { break };
        let start = cursor + next;
        if text[start..].starts_with("<!--") {
            cursor = start + 4 + text[start + 4..].find("-->")? + 3;
            continue;
        }
        if text[start..].starts_with("{#") {
            cursor = start + 2 + text[start + 2..].find("#}")? + 2;
            continue;
        }
        let tag = text[start..].starts_with("{%");
        if !tag && !text[start..].starts_with("{{") {
            cursor = start + 1;
            continue;
        }
        count += 1;
        if count > 4096 {
            return None;
        }
        let end = crate::file_actions::tag_end(text, start + 2, if tag { "%}" } else { "}}" })?;
        cursor = end + 2;
        let body = text[start + 2..end].trim_matches(['-', '~', ' ', '\t', '\r', '\n']);
        if tag && matches!(body, "raw" | "verbatim") {
            let expected = format!("end{body}");
            let mut from = cursor;
            let mut close = None;
            while let Some(next) = text[from..].find("{%") {
                let at = from + next;
                let end = at + 2 + text[at + 2..].find("%}")?;
                if text[at + 2..end].trim_matches(['-', '~', ' ', '\t', '\r', '\n']) == expected {
                    close = Some(end + 2);
                    break;
                }
                from = at + 2;
            }
            cursor = close?;
            continue;
        }
        let mut at = start + 2;
        while at < end {
            let b = text.as_bytes()[at];
            if b == b'#' {
                at = text[at..end].find('\n').map_or(end, |n| at + n + 1);
                continue;
            }
            if matches!(b, b'\'' | b'"') {
                let open = at;
                at += 1;
                while at < end && text.as_bytes()[at] != b {
                    if text.as_bytes()[at] == b'\\' {
                        at += 1;
                    }
                    if at < end {
                        at += text[at..].chars().next()?.len_utf8();
                    }
                }
                if at >= end {
                    return None;
                }
                out.push(open + 1..at);
                at += 1;
                if out.len() > 4096 {
                    return None;
                }
            } else {
                at += text[at..].chars().next()?.len_utf8();
            }
        }
    }
    Some(out)
}

pub(crate) fn reference(doc: &Document, at: usize) -> Option<Reference> {
    if doc.text.len() > MAX_TEXT || !doc.text.is_char_boundary(at) {
        return None;
    }
    let r = refs::reference_at(&doc.language, &doc.text, at)?;
    let (start, end, closed) = refs::literal_at(&doc.language, &doc.text, at)?;
    if !closed || r.start < start || r.end != end || r.start >= r.end {
        return None;
    }
    let content = &doc.text[start..end];
    if content.contains('\\') || content.chars().any(char::is_control) {
        return None;
    }
    let quote = *doc.text.as_bytes().get(start.checked_sub(1)?)?;
    match doc.language.as_str() {
        "php" => {
            if quote == b'"' && content.contains('$') {
                return None;
            }
            let masked = crate::php_text::mask(&doc.text);
            if masked.as_bytes().get(start - 1) != Some(&quote) || masked.as_bytes().get(end) != Some(&quote) {
                return None;
            }
        }
        "twig" => {
            if !twig_strings(&doc.text)?.contains(&(start..end)) || (quote == b'"' && content.contains("#{")) {
                return None;
            }
        }
        "yaml" => {
            if !doc.path.contains("/config/") {
                return None;
            }
            let line = doc.text[..start - 1].rsplit('\n').next()?;
            // Conservative for comments/flow syntax; no scalar interpretation.
            if line.contains('#') || line.contains(['|', '>']) {
                return None;
            }
        }
        // Existing XML text patterns do not establish qualified element/URI
        // ownership under nested prefix/default-namespace rebinding.
        "xml" => return None,
        _ => return None,
    }
    Some(r)
}
fn plain(value: &str) -> Option<String> {
    let v = value.trim();
    let q = v.as_bytes().first().copied().filter(|q| matches!(q, b'\'' | b'"'))?;
    if v.len() < 2 || v.as_bytes().last() != Some(&q) {
        return None;
    }
    let inner = &v[1..v.len() - 1];
    (!inner.is_empty()
        && !inner.contains(q as char)
        && !inner.contains(['\\', '$'])
        && !inner.contains("#{")
        && !inner.chars().any(char::is_control))
    .then(|| inner.into())
}
/// No explicit dynamic domain is silently converted to messages. Outer None is
/// unknown; the literal fallback is used only when no domain was supplied.
pub(crate) fn domain(doc: &Document, r: &Reference) -> Option<String> {
    source_domain(&doc.language, &doc.text, r)
}
pub(crate) fn source_domain(language: &str, text: &str, r: &Reference) -> Option<String> {
    let mut explicit = None;
    if language == "twig" {
        let after = text[r.end + 1..].trim_start().strip_prefix('|')?.trim_start().strip_prefix("trans")?;
        if after.as_bytes().first().is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_') {
            return None;
        }
        if after.trim_start().starts_with('(') {
            explicit = Some((text.len() - after.trim_start().len(), 1));
        }
    } else if language == "php" {
        let before = text[..r.start - 1].trim_end();
        if before.ends_with('(') {
            explicit = Some((before.len() - 1, 2));
        } else {
            return None;
        }
    } else {
        return None;
    }
    if let Some((open, position)) = explicit {
        // The existing argument splitter returns prefixes on incomplete input.
        // Do not interpret such a prefix as proof that a domain was omitted.
        let mut stack = Vec::new();
        let mut quote = None;
        let mut cursor = open;
        let mut closed = false;
        while cursor < text.len().min(open + 4000) {
            let b = text.as_bytes()[cursor];
            match quote {
                Some(_) if b == b'\\' => cursor += 1,
                Some(q) if b == q => quote = None,
                Some(_) => {}
                None if matches!(b, b'\'' | b'"') => quote = Some(b),
                None if matches!(b, b'(' | b'[' | b'{') => stack.push(b),
                None if matches!(b, b')' | b']' | b'}') => {
                    let expected = match b {
                        b')' => b'(',
                        b']' => b'[',
                        _ => b'{',
                    };
                    if stack.pop() != Some(expected) {
                        return None;
                    }
                    if stack.is_empty() {
                        closed = true;
                        break;
                    }
                }
                _ => {}
            }
            cursor += 1;
        }
        if !closed {
            return None;
        }
        let args = refs::arguments(&text, open);
        if language=="php" && args.iter().any(|argument| argument.trim().starts_with("...") || argument.contains("/*") || argument.contains("//")) {return None;}
        let named: Vec<_> = args.iter().filter_map(|argument| {
            let (name,value)=argument.trim().split_once(':')?;
            (name.trim()=="domain").then_some(value)
        }).collect();
        if named.len() > 1 {
            return None;
        }
        if let Some(named) = named.first() {
            return plain(named);
        }
        if let Some(value) = args.get(position) {
            return plain(value);
        }
    }
    if language == "twig" {
        let mut domains = Vec::new();
        let mut scopes = Vec::new();
        for (name, rest, _) in refs::twig_tags(&text) {
            if let Some(end) = name.strip_prefix("end") {
                if scopes.last() == Some(&end) {
                    scopes.pop();
                }
            } else if ["block", "macro", "embed", "if", "for", "with", "apply", "cache", "sandbox", "autoescape"].contains(&name) {
                scopes.push(name);
            }
            if name == "trans_default_domain" {
                if !scopes.is_empty() {
                    return None;
                }
                // Require a real literal from a recognized code region.
                let d = plain(rest.trim())?;
                let literal = rest.trim();
                let start = literal.as_ptr() as usize - text.as_ptr() as usize + 1;
                if !twig_strings(&text)?.contains(&(start..start + d.len())) {
                    continue;
                }
                domains.push(d);
            }
        }
        if domains.len() > 1 {
            return None;
        }
        if let Some(d) = domains.pop() {
            return Some(d);
        }
    }
    Some("messages".into())
}
