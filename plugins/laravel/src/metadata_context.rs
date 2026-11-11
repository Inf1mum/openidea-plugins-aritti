//! Literal array/return contexts built from the same bounded PHP lexer as receiver inference.
use super::*;
use crate::metadata::Site;

#[derive(Clone, Debug)]
pub(crate) struct ArrayRole {
    pub key: Option<String>,
    pub has_key: bool,
    pub is_key: bool,
}
/// Decode only deterministic PHP string escapes, never interpolation or expressions.
pub(crate) struct Decoded {
    pub value: String,
    pub boundaries: BTreeMap<usize, usize>,
}
impl Decoded {
    pub fn decoded_at(&self, raw: usize) -> Option<usize> {
        self.boundaries
            .iter()
            .find_map(|(decoded, source)| (*source == raw).then_some(*decoded))
    }
    pub fn raw_at(&self, decoded: usize) -> Option<usize> {
        self.boundaries.get(&decoded).copied()
    }
}
pub(crate) fn decoded(value: &str, quote: u8) -> Option<String> {
    decoded_offsets(value, quote).map(|decoded| decoded.value)
}
pub(crate) fn decoded_offsets(value: &str, quote: u8) -> Option<Decoded> {
    fn emit(
        out: &mut Vec<u8>,
        map: &mut Vec<(usize, usize)>,
        bytes: &[u8],
        start: usize,
        end: usize,
        identity: bool,
    ) {
        let before = out.len();
        map.push((before, start));
        if identity {
            for i in 1..bytes.len() {
                if bytes[i] & 0b11000000 != 0b10000000 {
                    map.push((before + i, start + i));
                }
            }
        }
        out.extend(bytes);
        map.push((out.len(), end));
    }
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut map = vec![(0, 0)];
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        let ch = value[at..].chars().next()?;
        at += ch.len_utf8();
        if ch == '$' && quote == b'"' {
            return None;
        }
        if ch != '\\' {
            emit(&mut out, &mut map, &bytes[start..at], start, at, true);
            continue;
        }
        let next = value[at..].chars().next()?;
        at += next.len_utf8();
        if quote == b'\'' {
            if matches!(next, '\\' | '\'') {
                emit(&mut out, &mut map, &[next as u8], start, at, false);
            } else {
                emit(&mut out, &mut map, &bytes[start..at], start, at, true);
            }
            continue;
        }
        let mut escaped = Vec::new();
        let mut identity = false;
        match next {
            '\\' | '"' | '$' => escaped.push(next as u8),
            'n' => escaped.push(b'\n'),
            'r' => escaped.push(b'\r'),
            't' => escaped.push(b'\t'),
            'v' => escaped.push(11),
            'e' => escaped.push(27),
            'f' => escaped.push(12),
            '0'..='7' => {
                let mut value = next.to_digit(8)?;
                for _ in 0..2 {
                    let Some(next) = bytes.get(at).filter(|byte| matches!(byte, b'0'..=b'7'))
                    else {
                        break;
                    };
                    value = value * 8 + u32::from(*next - b'0');
                    at += 1;
                }
                escaped.push((value & 255) as u8);
            }
            'x' | 'X' => {
                let mut value = 0;
                let mut digits = 0;
                for _ in 0..2 {
                    let Some(next) = bytes
                        .get(at)
                        .and_then(|byte| char::from(*byte).to_digit(16))
                    else {
                        break;
                    };
                    at += 1;
                    value = value * 16 + next;
                    digits += 1;
                }
                if digits == 0 {
                    identity = true;
                } else {
                    escaped.push(value as u8);
                }
            }
            'u' if bytes.get(at) == Some(&b'{') => {
                at += 1;
                let begin = at;
                while bytes.get(at) != Some(&b'}') {
                    let ch = *bytes.get(at)?;
                    if !ch.is_ascii_hexdigit() || at - begin >= 8 {
                        return None;
                    }
                    at += 1;
                }
                let point = u32::from_str_radix(&value[begin..at], 16).ok()?;
                at += 1;
                let mut buffer = [0; 4];
                escaped.extend(char::from_u32(point)?.encode_utf8(&mut buffer).as_bytes());
            }
            _ => {
                identity = true;
            }
        }
        if identity {
            emit(&mut out, &mut map, &bytes[start..at], start, at, true);
        } else {
            emit(&mut out, &mut map, &escaped, start, at, false);
        }
    }
    let value = String::from_utf8(out).ok()?;
    let boundaries = map
        .into_iter()
        .filter(|(at, _)| value.is_char_boundary(*at))
        .collect();
    Some(Decoded { value, boundaries })
}

pub(crate) fn encoded(value: &str, quote: u8) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch == '\\' || ch == char::from(quote) || ch == '$' && quote == b'"' {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}
fn literal(source: &Source, index: usize) -> Option<(std::ops::Range<usize>, bool, u8)> {
    let token = source.tokens.get(index)?;
    if token.kind != Kind::String {
        return None;
    }
    let quote = *source.text.as_bytes().get(token.start)?;
    if !matches!(quote, b'\'' | b'"') {
        return None;
    }
    let closed =
        token.end > token.start + 1 && source.text.as_bytes().get(token.end - 1) == Some(&quote);
    let range = token.start + 1..token.end - usize::from(closed);
    decoded(&source.text[range.clone()], quote)?;
    Some((range, closed, quote))
}
fn array_open(source: &Source, index: usize) -> bool {
    if source.is(index, "(") {
        return index > 0 && source.is(index - 1, "array");
    }
    source.is(index, "[")
        && (index == 0
            || index == 3 && source.is(0, "<") && source.is(1, "?") && source.is(2, "php")
            || matches!(
                source.tokens[index - 1].value,
                "(" | "[" | "{" | "=" | "=>" | "," | ":" | "return" | ";"
            ))
}
fn role(source: &Source, open: usize, target: usize, target_end: usize) -> Option<ArrayRole> {
    let end = source
        .pairs
        .get(&open)
        .copied()
        .unwrap_or(source.tokens.len());
    let mut start = open + 1;
    let mut cursor = start;
    let mut arrow = None;
    let mut finish = end;
    while cursor < end {
        if source.is(cursor, ",") {
            if cursor < target {
                start = cursor + 1;
                arrow = None;
            } else {
                finish = cursor;
                break;
            }
        } else if source.is(cursor, "=>") {
            arrow = Some(cursor);
        }
        if let Some(close) = source.pairs.get(&cursor).filter(|&&close| close > cursor) {
            cursor = *close + 1;
        } else {
            cursor += 1;
        }
    }
    let (is_key, has_key, key) = match arrow {
        Some(arrow) if arrow == target_end + 1 && start == target => (true, true, None),
        Some(arrow) if arrow + 1 == target && target_end + 1 == finish => {
            let key = (arrow == start + 1)
                .then(|| literal(source, start))
                .flatten()
                .and_then(|(range, _, quote)| decoded(&source.text[range], quote));
            (false, true, key)
        }
        None if start == target && target_end + 1 == finish => (false, false, None),
        _ => return None,
    };
    Some(ArrayRole {
        key,
        has_key,
        is_key,
    })
}

pub(crate) fn site(text: &str, at: usize) -> Option<Site> {
    if at > text.len()
        || !text.is_char_boundary(at)
        || ["?>", "<<<", "`"]
            .iter()
            .any(|needle| text[..at].contains(needle))
    {
        return None;
    }
    let source = Source::new(text)?;
    binding_names(&source)?;
    let index = source
        .tokens
        .iter()
        .position(|token| token.kind == Kind::String && token.start < at && at <= token.end)?;
    let (range, closed, quote) = literal(&source, index)?;
    if at > range.end {
        return None;
    }
    let mut stack = Vec::new();
    for (i, token) in source.tokens.iter().enumerate().take(index) {
        if matches!(token.value, "(" | "[" | "{") {
            stack.push(i);
        }
        if matches!(token.value, ")" | "]" | "}") {
            stack.pop()?;
        }
    }
    let mut target = index;
    let mut target_end = index;
    let mut arrays = Vec::new();
    while let Some(open) = stack
        .last()
        .copied()
        .filter(|open| array_open(&source, *open))
    {
        if arrays.len() >= 2 {
            return None;
        }
        arrays.push(role(&source, open, target, target_end)?);
        stack.pop();
        target = if source.is(open, "(") {
            open.checked_sub(1)?
        } else {
            open
        };
        target_end = source
            .pairs
            .get(&open)
            .copied()
            .unwrap_or(source.tokens.len().saturating_sub(1));
    }
    arrays.reverse();
    let after = target_end + 1;
    let rhs = if source.is(after, "|>") {
        Some(after + 1)
    } else if source.is(after, "|") && source.is(after + 1, ">") {
        Some(after + 2)
    } else {
        None
    };
    if let Some(rhs) = rhs {
        if target > 0 {
            let previous = source.tokens[target - 1].value;
            let opening =
                target == 3 && source.is(0, "<") && source.is(1, "?") && source.is(2, "php");
            let named = previous == ":"
                && stack.last().is_some_and(|open| {
                    source.is(*open, "(")
                        && crate::metadata::argument_context(
                            &crate::php::mask_comments(text),
                            source.tokens[*open].start,
                            source.tokens[target].start,
                        )
                        .is_some()
                });
            if !opening
                && !named
                && !matches!(previous, "=" | "return" | "(" | "," | ";" | "{" | "=>")
            {
                return None;
            }
        }
        let open = if source
            .tokens
            .get(rhs)
            .is_some_and(|token| token.kind == Kind::Name)
            && source.is(rhs + 1, "(")
        {
            rhs + 1
        } else if source
            .tokens
            .get(rhs)
            .is_some_and(|token| token.kind == Kind::Name)
            && source.is(rhs + 1, "::")
            && source
                .tokens
                .get(rhs + 2)
                .is_some_and(|token| token.kind == Kind::Name)
            && source.is(rhs + 3, "(")
        {
            rhs + 3
        } else if source
            .tokens
            .get(rhs)
            .is_some_and(|token| token.kind == Kind::Var)
            && source.is(rhs + 1, "->")
            && source
                .tokens
                .get(rhs + 2)
                .is_some_and(|token| token.kind == Kind::Name)
            && source.is(rhs + 3, "(")
        {
            rhs + 3
        } else {
            return None;
        };
        let close = *source.pairs.get(&open)?;
        if !(source.is(open + 1, "...") && close == open + 2
            || source.is(open + 1, ".")
                && source.is(open + 2, ".")
                && source.is(open + 3, ".")
                && close == open + 4)
        {
            return None;
        }
        if source
            .tokens
            .get(close + 1)
            .is_some_and(|token| !matches!(token.value, ";" | "," | ")" | "]" | "}" | "|>" | "|"))
        {
            return None;
        }
        let identity = call_identity(text, source.tokens[open].start);
        let unresolved = identity.is_none();
        let identity = identity.unwrap_or(CallIdentity {
            class: None,
            method: source.tokens[open - 1].value.into(),
            constructor: false,
            function_names: Vec::new(),
            function_fqn: None,
        });
        if identity.constructor {
            return None;
        }
        return Some(Site {
            open: source.tokens[open].start,
            identity,
            argument: 1,
            parameter_name: None,
            literal: range,
            arrays,
            return_context: false,
            closed,
            quote,
            unresolved,
            doc_path: None,
            model: None,
            containing: None,
            pipe: true,
        });
    }
    // Return expressions belong to a declared method, never a nested closure.
    if target > 0 && source.is(target - 1, "return") {
        let end = target_end + 1;
        if source
            .tokens
            .get(end)
            .is_some_and(|token| !matches!(token.value, ";" | "}"))
        {
            return None;
        }
        let names = binding_names(&source)?;
        let (full, _, body, _) = names.class.as_ref()?;
        let function = source
            .functions()
            .into_iter()
            .filter(|function| function.body < index && index < function.end)
            .max_by_key(|function| function.body)?;
        if source.paths[function.start] != vec![*body] {
            return None;
        }
        let method = source.tokens[function.name?].value.to_owned();
        return Some(Site {
            open: source.tokens[target - 1].start,
            identity: CallIdentity {
                class: Some(full.clone()),
                method,
                constructor: false,
                function_names: Vec::new(),
                function_fqn: None,
            },
            argument: 0,
            parameter_name: None,
            literal: range,
            arrays,
            return_context: true,
            closed,
            quote,
            unresolved: false,
            doc_path: None,
            model: None,
            containing: None,
            pipe: false,
        });
    }
    let open = stack.last().copied()?;
    if !source.is(open, "(") || array_open(&source, open) {
        return None;
    }
    let after = target_end + 1;
    if source
        .tokens
        .get(after)
        .is_some_and(|token| !matches!(token.value, "," | ")"))
    {
        return None;
    }
    let (argument, parameter_name) = crate::metadata::argument_context(
        &crate::php::mask_comments(text),
        source.tokens[open].start,
        source.tokens[target].start,
    )?;
    let identity = call_identity(text, source.tokens[open].start);
    let unresolved = identity.is_none();
    let identity = if let Some(identity) = identity {
        identity
    } else {
        if open < 2
            || !matches!(source.tokens[open - 2].value, "->" | "?->" | "::")
            || source.tokens[open - 1].kind != Kind::Name
        {
            return None;
        }
        CallIdentity {
            class: None,
            method: source.tokens[open - 1].value.into(),
            constructor: false,
            function_names: Vec::new(),
            function_fqn: None,
        }
    };
    Some(Site {
        open: source.tokens[open].start,
        identity,
        argument,
        parameter_name,
        literal: range,
        arrays,
        return_context: false,
        closed,
        quote,
        unresolved,
        doc_path: None,
        model: None,
        containing: None,
        pipe: false,
    })
}
