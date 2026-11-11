//! Conservative source anchors for an already-owned Blade compiler origin.
//! Generated scaffolding alone never proves a source line.
#[derive(Clone)]
struct Token<'a> {
    value: &'a str,
}
fn tokens(text: &str) -> Option<Vec<Token<'_>>> {
    let b = text.as_bytes();
    let mut at = 0;
    let mut out = Vec::new();
    while at < b.len() {
        if out.len() > 65536 {
            return None;
        }
        if b[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if text[at..].starts_with("{{--") {
            at += text[at..].find("--}}").map_or(b.len() - at, |n| n + 4);
            continue;
        }
        if text[at..].starts_with("/*") {
            at += text[at..].find("*/").map_or(b.len() - at, |n| n + 2);
            continue;
        }
        if text[at..].starts_with("//") {
            at += text[at..].find('\n').unwrap_or(b.len() - at);
            continue;
        }
        let start = at;
        if matches!(b[at], b'\'' | b'"' | b'`') {
            let q = b[at];
            at += 1;
            let mut closed = false;
            while at < b.len() {
                if b[at] == b'\\' {
                    at = (at + 2).min(b.len());
                    continue;
                }
                if b[at] == q {
                    at += 1;
                    closed = true;
                    break;
                }
                at += 1;
            }
            if !closed {
                return None;
            }
        } else if b[at].is_ascii_alphanumeric() || matches!(b[at], b'_' | b'$') {
            at += 1;
            while at < b.len() && (b[at].is_ascii_alphanumeric() || matches!(b[at], b'_' | b'$')) {
                at += 1;
            }
        } else {
            at += text[at..].chars().next()?.len_utf8();
        }
        out.push(Token {
            value: &text[start..at],
        });
    }
    Some(out)
}
fn count(haystack: &[Token<'_>], needle: &[Token<'_>], budget: &mut usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    let cost = haystack.len().saturating_mul(needle.len());
    if cost > *budget {
        return None;
    }
    *budget -= cost;
    Some(
        haystack
            .windows(needle.len())
            .filter(|part| part.iter().zip(needle).all(|(a, b)| a.value == b.value))
            .take(2)
            .count(),
    )
}
fn close(text: &str, start: usize, open: u8, end: u8) -> Option<usize> {
    let b = text.as_bytes();
    let mut at = start;
    let mut depth = 1;
    while at < b.len() {
        if matches!(b[at], b'\'' | b'"' | b'`') {
            let q = b[at];
            at += 1;
            while at < b.len() {
                if b[at] == b'\\' {
                    at = (at + 2).min(b.len());
                    continue;
                }
                if b[at] == q {
                    at += 1;
                    break;
                }
                at += 1;
            }
            continue;
        }
        if b[at] == open {
            depth += 1;
        }
        if b[at] == end {
            depth -= 1;
            if depth == 0 {
                return Some(at);
            }
        }
        at += 1;
    }
    None
}
fn candidates(source: &str) -> Vec<(usize, &str)> {
    let mut result = Vec::new();
    let mut at = 0;
    let b = source.as_bytes();
    while at < b.len() && result.len() < 2048 {
        if source[at..].starts_with("{{--") {
            at += source[at..].find("--}}").map_or(b.len() - at, |n| n + 4);
            continue;
        }
        if source[at..].starts_with("@verbatim") {
            at += source[at..]
                .find("@endverbatim")
                .map_or(b.len() - at, |n| n + 12);
            continue;
        }
        if source[at..].starts_with("@@") {
            at += 2;
            continue;
        }
        let echo = if source[at..].starts_with("{{")
            && at.checked_sub(1).and_then(|i| b.get(i)) != Some(&b'@')
        {
            Some((2, "}}"))
        } else if source[at..].starts_with("{!!") {
            Some((3, "!!}"))
        } else {
            None
        };
        if let Some((prefix, suffix)) = echo {
            let start = at + prefix;
            if let Some(end) = source[start..].find(suffix) {
                result.push((start, &source[start..start + end]));
                at = start + end + suffix.len();
                continue;
            }
        }
        if b[at] == b'@' {
            let mut end = at + 1;
            while b.get(end).is_some_and(u8::is_ascii_alphabetic) {
                end += 1;
            }
            let name = &source[at + 1..end];
            while b.get(end).is_some_and(u8::is_ascii_whitespace) {
                end += 1;
            }
            if b.get(end) == Some(&b'(')
                && matches!(
                    name,
                    "if" | "elseif"
                        | "unless"
                        | "while"
                        | "for"
                        | "foreach"
                        | "forelse"
                        | "switch"
                        | "case"
                        | "include"
                        | "includeIf"
                        | "includeWhen"
                        | "includeUnless"
                        | "each"
                        | "json"
                        | "js"
                        | "error"
                        | "isset"
                        | "empty"
                        | "can"
                        | "cannot"
                        | "canany"
                        | "php"
                )
            {
                let start = end + 1;
                if let Some(finish) = close(source, start, b'(', b')') {
                    let expression = &source[start..finish];
                    result.push((start, expression));
                    let args = crate::php::arguments(source, end);
                    if args.len() > 1 {
                        for arg in args {
                            result.push((arg.start, &source[arg]));
                        }
                    }
                    if matches!(name, "foreach" | "forelse") {
                        if let Some(as_at) = expression.find(" as ") {
                            result.push((start, &expression[..as_at]));
                        }
                    }
                    at = finish + 1;
                    continue;
                }
            }
        }
        at += source[at..].chars().next().map_or(1, char::len_utf8);
    }
    result
}
pub(super) fn line(compiled: &str, source: &str, line: u32) -> Option<u32> {
    if compiled.len() > 1024 * 1024 || source.len() > 1024 * 1024 {
        return None;
    }
    let target = compiled.lines().nth(line.checked_sub(1)? as usize)?.trim();
    if target.is_empty() {
        return None;
    }
    // Unchanged PHP/markup lines are also valid anchors, but only when unique.
    if compiled
        .lines()
        .filter(|candidate| candidate.trim() == target)
        .count()
        == 1
    {
        let matches: Vec<_> = source
            .lines()
            .enumerate()
            .filter(|(_, s)| s.trim() == target)
            .collect();
        if matches.len() == 1 {
            return Some(matches[0].0 as u32);
        }
    }
    let compiled_tokens = tokens(compiled)?;
    let source_tokens = tokens(source)?;
    let target_tokens = tokens(target)?;
    let mut matches = std::collections::BTreeSet::new();
    let mut budget = 16_000_000usize;
    for (at, expression) in candidates(source) {
        let Some(expression) = tokens(expression) else {
            continue;
        };
        if expression.is_empty() || expression.len() > 512 {
            continue;
        }
        if count(&target_tokens, &expression, &mut budget)? == 1
            && count(&compiled_tokens, &expression, &mut budget)? == 1
            && count(&source_tokens, &expression, &mut budget)? == 1
        {
            matches.insert(source[..at].bytes().filter(|b| *b == b'\n').count() as u32);
        }
    }
    (matches.len() == 1).then(|| *matches.first().unwrap())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unique_emitted_expressions_map_inline_echo_directives_and_include_arguments() {
        assert_eq!(
            line(
                "<?php if(is_ready($user)): ?>",
                "heading\n@if (is_ready($user))",
                1
            ),
            Some(1)
        );
        assert_eq!(
            line(
                "<?php echo e(render_name($user)); ?>",
                "heading\nHello {{ render_name($user) }}",
                1
            ),
            Some(1)
        );
        assert_eq!(
            line(
                "<?php echo $__env->make('partial', ['id' => $owner], array_diff_key(get_defined_vars(), ['__data'=>1]))->render(); ?>",
                "@include('partial', ['id' => $owner])",
                1
            ),
            Some(0)
        );
    }
    #[test]
    fn ambiguous_or_generated_scaffolding_retains_file_only() {
        assert_eq!(line("<?php if($ok): ?>", "@if($ok)\n@if($ok)", 1), None);
        assert_eq!(
            line("<?php endforeach; $__env->popLoop(); ?>", "@endforeach", 1),
            None
        );
        assert_eq!(
            line("<?php echo e($name); ?>", "{{-- {{ $name }} --}}", 1),
            None
        );
        assert_eq!(
            line(
                "<?php echo e($name); ?>",
                "@verbatim{{ $name }}@endverbatim",
                1
            ),
            None
        );
    }
}
