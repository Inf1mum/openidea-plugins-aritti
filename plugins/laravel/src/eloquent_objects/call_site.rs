//! Signature help follows the nearest real call and top-level arguments only.
use super::super::{Kind, Source};
use std::collections::BTreeSet;
pub(in crate::eloquent_members) fn call_open(source: &Source, offset: usize) -> Option<usize> {
    for open in (0..source.tokens.len()).rev().filter(|i| {
        source.is(*i, "(")
            && source.tokens[*i].start < offset
            && source
                .pairs
                .get(i)
                .is_none_or(|end| source.tokens[*end].start >= offset)
    }) {
        let before = open.checked_sub(1)?;
        let token = &source.tokens[before];
        if token.kind == Kind::Name {
            if [
                "if", "elseif", "while", "for", "foreach", "switch", "catch", "match", "array",
                "function", "fn", "use",
            ]
            .iter()
            .any(|keyword| token.value.eq_ignore_ascii_case(keyword))
            {
                continue;
            }
            return Some(open);
        }
        if matches!(token.value, ")" | "]") {
            return Some(open);
        }
    }
    None
}
fn parameter_name(label: &str) -> Option<&str> {
    let (_, name) = label.split_once('$')?;
    let end = name
        .find(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .unwrap_or(name.len());
    (end > 0).then_some(&name[..end])
}
pub(in crate::eloquent_members) fn active_parameter(
    source: &Source,
    open: usize,
    offset: usize,
    parameters: &[String],
) -> Option<u32> {
    if parameters.is_empty() {
        return Some(0);
    }
    let mut starts = vec![open + 1];
    let mut cursor = open + 1;
    while cursor < source.tokens.len() && source.tokens[cursor].start < offset {
        if source.is(cursor, ",") {
            starts.push(cursor + 1);
            cursor += 1;
            continue;
        }
        if matches!(source.tokens[cursor].value, "(" | "[" | "{") {
            if let Some(end) = source.pairs.get(&cursor).filter(|end| **end > cursor) {
                if source.tokens[*end].start < offset {
                    cursor = end + 1;
                    continue;
                }
            }
            break;
        }
        cursor += 1;
    }
    let variadic = parameters.iter().position(|p| {
        p.split_once('$')
            .is_some_and(|(prefix, _)| prefix.contains("..."))
    });
    let mut named = BTreeSet::new();
    let mut saw_named = false;
    for (position, start) in starts.iter().enumerate() {
        let current = position + 1 == starts.len();
        let named_arg = source
            .tokens
            .get(*start)
            .filter(|token| token.kind == Kind::Name && source.is(*start + 1, ":"))
            .map(|token| token.value);
        if let Some(name) = named_arg {
            if !named.insert(name) {
                return None;
            }
            saw_named = true;
            let target = parameters
                .iter()
                .position(|p| parameter_name(p) == Some(name))
                .or(variadic)?;
            if current {
                return Some(target as u32);
            }
        } else if current {
            if saw_named
                && source
                    .tokens
                    .get(*start)
                    .is_some_and(|token| token.start < offset)
            {
                return None;
            }
            return Some(position.min(variadic.unwrap_or(parameters.len() - 1)) as u32);
        } else if saw_named {
            return None;
        }
    }
    None
}
