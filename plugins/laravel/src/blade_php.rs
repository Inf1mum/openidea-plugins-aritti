//! Opaque source spans for the bounded Laravel Blade literal adapter, not PHP
//! execution or a complete Blade parser. Coordinates remain original UTF-8 bytes.
use std::ops::Range;
const MAX_TEXT: usize = 128 * 1024;
const MAX_SPANS: usize = 4096;

fn quoted_end(bytes: &[u8], from: usize) -> Option<usize> {
    let quote = bytes[from];
    let mut at = from + 1;
    while at < bytes.len() {
        if bytes[at] == b'\\' {
            at += 2;
            continue;
        }
        if bytes[at] == quote {
            return Some(at + 1);
        }
        if quote != b'\'' && (bytes[at..].starts_with(b"{$") || bytes[at..].starts_with(b"${")) {
            return None;
        }
        at += 1;
    }
    None
}
fn code_close(bytes: &[u8], from: usize, close: &[u8]) -> Option<usize> {
    let mut at = from;
    while at < bytes.len() {
        if bytes[at..].starts_with(close) {
            return Some(at);
        }
        if matches!(bytes[at], b'\'' | b'"' | b'`') {
            at = quoted_end(bytes, at)?;
        } else if bytes[at..].starts_with(b"/*") {
            at += 2;
            while at < bytes.len() && !bytes[at..].starts_with(b"*/") {
                at += 1;
            }
            if at == bytes.len() {
                return None;
            }
            at += 2;
        } else if bytes[at..].starts_with(b"//")
            || bytes[at] == b'#' && bytes.get(at + 1) != Some(&b'[')
        {
            at += if bytes[at] == b'#' { 1 } else { 2 };
            // PHP ?> remains active inside // and # line comments.
            while at < bytes.len()
                && !matches!(bytes[at], b'\r' | b'\n')
                && !bytes[at..].starts_with(close)
            {
                at += 1;
            }
        } else if bytes[at..].starts_with(b"<<<") {
            return None;
        } else {
            at += 1;
        }
    }
    None
}
fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    bytes
        .get(from..)?
        .windows(needle.len())
        .position(|v| v == needle)
        .map(|n| from + n)
}

pub(crate) fn ranges(text: &str) -> Option<Vec<Range<usize>>> {
    if text.len() > MAX_TEXT {
        return None;
    }
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let end = if bytes[at..].starts_with(b"{{--") {
            Some(find(bytes, at + 4, b"--}}").map_or(bytes.len(), |n| n + 4))
        } else if bytes[at..].starts_with(b"<?") {
            Some(code_close(bytes, at + 2, b"?>").map_or(bytes.len(), |n| n + 2))
        } else if bytes[at] == b'@'
            && (at == 0 || bytes[at - 1] != b'@')
            && bytes[at..].starts_with(b"@verbatim")
        {
            // Laravel stores these before compiling other Blade constructs.
            match find(bytes, at + 9, b"@endverbatim") {
                Some(end) => Some(end + 12),
                None if bytes.get(at + 9).is_none_or(|c| !c.is_ascii_alphabetic()) => {
                    Some(bytes.len())
                }
                _ => None,
            }
        } else if bytes[at] == b'@'
            && (at == 0 || bytes[at - 1] != b'@')
            && bytes[at..].starts_with(b"@php")
        {
            match find(bytes, at + 4, b"@endphp") {
                Some(raw) => {
                    // The compiler's raw delimiter inside a PHP quote/comment
                    // is ambiguous here; never invent a later template scope.
                    Some(if code_close(bytes, at + 4, b"@endphp") == Some(raw) {
                        raw + 7
                    } else {
                        bytes.len()
                    })
                }
                None => {
                    let mut next = at + 4;
                    while bytes.get(next).is_some_and(u8::is_ascii_whitespace) {
                        next += 1;
                    }
                    if bytes.get(at + 4).is_none_or(|c| !c.is_ascii_alphabetic())
                        && bytes.get(next) != Some(&b'(')
                    {
                        Some(bytes.len())
                    } else {
                        None
                    }
                }
            }
        } else {
            None
        };
        if let Some(end) = end {
            spans.push(at..end);
            if spans.len() > MAX_SPANS {
                return None;
            }
            at = end;
        } else {
            at += 1;
        }
    }
    Some(spans)
}
pub(crate) fn opaque_at(text: &str, at: usize) -> bool {
    at > text.len()
        || ranges(text).is_none_or(|spans| {
            spans
                .iter()
                .any(|r| r.contains(&at) || at == text.len() && r.end == at)
        })
}
pub(crate) fn masked(text: &str) -> Option<String> {
    let spans = ranges(text)?;
    let mut bytes = text.as_bytes().to_vec();
    for span in spans {
        for byte in &mut bytes[span] {
            if !matches!(*byte, b'\r' | b'\n') {
                *byte = b' ';
            }
        }
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_php_quotes_block_comments_attributes_and_line_comments_have_distinct_close_rules() {
        for source in [
            r#"<?php $x='?> fake'; ?> real"#,
            r#"<?php $x="?> fake"; ?> real"#,
            r#"<?php $x=`?> fake`; ?> real"#,
            r#"<?php /* ?> fake */ ?> real"#,
            r#"<?php #[Example("?> fake")] function f(){} ?> real"#,
        ] {
            assert!(opaque_at(source, source.find("fake").unwrap()));
            assert!(
                !opaque_at(source, source.rfind("real").unwrap()),
                "{source}"
            );
        }
        for source in [
            "<?php // comment ?> real",
            "<?php # comment ?> real",
            "<?php // comment\n?> real",
        ] {
            assert!(!opaque_at(source, source.find("real").unwrap()), "{source}");
        }
    }
    #[test]
    fn opaque_block_and_comment_masks_preserve_byte_coordinates_and_real_successors() {
        for source in [
            "😀\r\n<?php $x='é ?> fake'; ?> real",
            "{{-- <?php fake --}} real",
            "@verbatim @include('fake') <x-fake> @endverbatim real",
            "@php\n$x=\"@include('fake') <x-fake>\";\n@endphp real",
        ] {
            assert!(opaque_at(source, source.find("fake").unwrap()));
            assert!(
                !opaque_at(source, source.rfind("real").unwrap()),
                "{source}"
            );
            let masked = masked(source).unwrap();
            assert_eq!(masked.len(), source.len());
            assert_eq!(masked.matches('\n').count(), source.matches('\n').count());
            assert_eq!(masked.matches('\r').count(), source.matches('\r').count());
            assert!(!masked.contains("fake"));
        }
    }
    #[test]
    fn unknown_php_states_and_processing_bounds_never_invent_template_continuation() {
        for source in [
            "<?php $x='unclosed ?> real",
            "<?php /* unclosed ?> real",
            "<?php $x=<<<TXT\n?> real\nTXT;\n?>",
            r#"<?php $x="{$obj->call("?>")} real"; ?>"#,
            "@php $x='@endphp'; @endphp real",
        ] {
            assert!(opaque_at(source, source.find("real").unwrap()), "{source}");
        }
        assert!(ranges(&"x".repeat(MAX_TEXT + 1)).is_none());
        assert!(ranges(&"<? ?>".repeat(MAX_SPANS + 1)).is_none());
        assert!(opaque_at("<?php <x-fake", "<?php <x-fake".len()));
    }
}
