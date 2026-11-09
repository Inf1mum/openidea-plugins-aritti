//! Decoded annotation literals with original raw spans. Structural Java Unicode
//! escapes/text-block preprocessing remain compiler-owned rather than guessed.
use super::*;
fn decode(raw: &str, kotlin: bool, quote: char) -> Option<String> {
    let inner = raw.strip_prefix(quote)?.strip_suffix(quote)?;
    if quote == '"' && raw.starts_with("\"\"\"") {
        if !kotlin {
            return None;
        }
        let inner = raw.strip_prefix("\"\"\"")?.strip_suffix("\"\"\"")?;
        return (!inner.contains('$')).then(|| inner.into());
    }
    let mut chars = inner.chars().peekable();
    let mut units = Vec::new();
    while let Some(c) = chars.next() {
        if c == '\n' || c == '\r' {
            return None;
        }
        if c == '$' && kotlin && chars.peek().is_some_and(|c| *c == '{' || *c == '_' || c.is_alphabetic()) {
            return None;
        }
        if c != '\\' {
            units.extend(c.encode_utf16(&mut [0; 2]).iter().copied());
            continue;
        }
        let escaped = chars.next()?;
        if escaped == 'u' {
            if !kotlin {
                while chars.peek() == Some(&'u') {
                    chars.next();
                }
            }
            let mut value = 0u16;
            for _ in 0..4 {
                value = value.checked_mul(16)?.checked_add(chars.next()?.to_digit(16)? as u16)?;
            }
            if !kotlin && matches!(value, 0x22 | 0x5c | 0xa | 0xd) || quote == '\'' && value == 0x27 {
                return None;
            }
            units.push(value);
            continue;
        }
        let value = match escaped {
            'b' => '\u{8}',
            't' => '\t',
            'n' => '\n',
            'r' => '\r',
            'f' if !kotlin => '\u{c}',
            's' if !kotlin => ' ',
            '\\' => '\\',
            '\'' => '\'',
            '"' => '"',
            '$' if kotlin => '$',
            v @ '0'..='7' if !kotlin => {
                let mut value = v.to_digit(8)?;
                let count = if v <= '3' { 2 } else { 1 };
                for _ in 0..count {
                    let Some(next) = chars.peek().copied().filter(|c| matches!(c, '0'..='7')) else {
                        break;
                    };
                    chars.next();
                    value = value * 8 + next.to_digit(8)?;
                }
                units.push(value as u16);
                continue;
            }
            _ => return None,
        };
        units.push(value as u16);
    }
    String::from_utf16(&units).ok()
}
/// Keep established ordinary literal decoding while refusing source-level
/// Unicode translation that could change comments, delimiters or identifiers.
pub(super) fn java_source_supported(text:&str)->bool {
    if !text.contains("\\u"){return true;}
    let mut ranges=Vec::new();
    for token in java::tokens(text) {
        if token.s.contains("\\u") && (token.s.starts_with('\"') || token.s.starts_with('\'')) {
            let quote=token.s.chars().next().unwrap();
            if decode(token.s,false,quote).is_none(){return false;}
            ranges.push(token.at..token.end());
        }
    }
    let mut current=0;
    for (at,_) in text.match_indices("\\u") {
        while current<ranges.len() && ranges[current].end<=at {current+=1;}
        if ranges.get(current).is_none_or(|r|at<r.start || at+2>r.end){return false;}
    }
    true
}
pub(super) fn values(doc: &Document, a: &Annotation, name: &str) -> Option<Vec<Str>> {
    let arg = a.arg(name)?;
    let mut out = Vec::new();
    for token in java::tokens(&arg.text) {
        if matches!(token.s, "{" | "}" | "[" | "]" | ",") {
            continue;
        }
        let value = decode(token.s, doc.language == "kotlin", '"')?;
        let quote = if token.s.starts_with("\"\"\"") { 3 } else { 1 };
        out.push(Str { value, at: arg.at + token.at + quote, end: arg.at + token.end() - quote });
    }
    Some(out)
}
pub(super) fn characters(doc: &Document, a: &Annotation, name: &str) -> Option<Vec<(String, std::ops::Range<usize>)>> {
    let arg = a.arg(name)?;
    let mut out = Vec::new();
    for token in java::tokens(&arg.text) {
        if matches!(token.s, "{" | "}" | "[" | "]" | ",") {
            continue;
        }
        let value = decode(token.s, doc.language == "kotlin", '\'')?;
        if value.encode_utf16().count() != 1 {
            return None;
        }
        out.push((value, arg.at + token.at + 1..arg.at + token.end() - 1));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escaped_literals_decode_without_losing_compiler_boundaries() {
        assert_eq!(decode(r#""\u0061\141\t""#, false, '"').as_deref(), Some("aa\t"));
        assert_eq!(decode(r#""\uD83D\uDE00""#, false, '"').as_deref(), Some("😀"));
        assert_eq!(decode(r#""\$name""#, true, '"').as_deref(), Some("$name"));
        assert_eq!(decode(r#""$name""#, true, '"'), None);
        assert_eq!(decode(r#""${value}""#, true, '"'), None);
        assert_eq!(decode(r#""\u0022""#, false, '"'), None);
        assert_eq!(decode(r#""\u005c""#, false, '"'), None);
        assert_eq!(decode("\"\"\"\ntext\n\"\"\"", false, '"'), None);
        assert_eq!(decode(r#"'\t'"#, true, '\'').as_deref(), Some("\t"));
        assert_eq!(decode(r#""\q""#, false, '"'), None);
    }
}
