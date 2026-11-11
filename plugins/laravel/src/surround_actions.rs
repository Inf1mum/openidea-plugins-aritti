//! Selection-based Laravel dd()/collect() surround actions.
use crate::{actions::Rewrite, package_facts, php};
use openidea_plugin::{Range, providers::Document, text};
fn runtime_scope(source: &str, at: usize) -> bool {
    let Some(code) = package_facts::code(source) else {
        return false;
    };
    let mut stack = Vec::new();
    for (offset, byte) in code.bytes().enumerate().take(at) {
        match byte {
            b'(' | b'[' | b'{' => stack.push((byte, offset)),
            b')' | b']' | b'}' => {
                stack.pop();
            }
            _ => {}
        }
    }
    let mut in_class = false;
    let mut in_function = false;
    for (byte, open) in stack {
        let before = code[..open].trim_end();
        let boundary = before.rfind([';', '{', '}']).map_or(0, |n| n + 1);
        let header = before[boundary..].to_ascii_lowercase();
        if byte == b'[' && before.ends_with('#') {
            return false;
        }
        if byte == b'(' {
            let name_start = before
                .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
                .map_or(0, |n| n + 1);
            if before[..name_start]
                .trim_end()
                .trim_end_matches('&')
                .trim_end()
                .ends_with("function")
                || before.ends_with("function")
                || before.ends_with("fn")
            {
                return false;
            }
        }
        if byte == b'{' {
            if header
                .split_whitespace()
                .any(|word| matches!(word, "class" | "enum" | "interface" | "trait"))
            {
                in_class = true;
            }
            if header.contains("function ") || header.contains("function(") {
                in_function = true;
            }
        }
    }
    let before = &code[..at];
    let statement = before[before.rfind([';', '{', '}']).map_or(0, |n| n + 1)..]
        .trim_start()
        .trim_start_matches("<?php")
        .trim_start()
        .to_ascii_lowercase();
    !(in_class && !in_function
        || statement.starts_with("const ")
        || statement.starts_with("static "))
}

fn expression(source: &str, selection: std::ops::Range<usize>) -> Option<&str> {
    if selection.is_empty()
        || selection.end > source.len()
        || selection.len() > 32 * 1024
        || source.contains("<<<")
        || source.contains("?>")
        || source.contains('`')
    {
        return None;
    }
    if !runtime_scope(source, selection.start) {
        return None;
    }
    let raw = source.get(selection.clone())?;
    let selected = raw.trim();
    if selected.is_empty() || raw != selected {
        return None;
    }
    if selection.start < source.find("<?php")?.checked_add(5)? || selected.contains("<?") {
        return None;
    }
    let mask = php::mask_comments(source);
    if mask.get(selection.clone())?.trim().is_empty() {
        return None;
    }
    let bytes = mask.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if matches!(bytes[at], b'\'' | b'"') {
            let end = php::string_end(bytes, at);
            if at < selection.start && selection.start < end
                || at < selection.end && selection.end < end
            {
                return None;
            }
            at = end;
        } else {
            at += 1;
        }
    }
    if source
        .as_bytes()
        .get(selection.start.wrapping_sub(1))
        .is_some_and(|b| php::is_identifier(*b) || *b == b'$' || *b >= 128)
        || source
            .as_bytes()
            .get(selection.end)
            .is_some_and(|b| php::is_identifier(*b) || *b == b'$' || *b >= 128)
    {
        return None;
    }
    let code = package_facts::code(selected)?;
    let mut stack = Vec::new();
    let bytes = code.as_bytes();
    for (at, byte) in bytes.iter().copied().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => stack.push(byte),
            b')' => {
                if stack.pop() != Some(b'(') {
                    return None;
                }
            }
            b']' => {
                if stack.pop() != Some(b'[') {
                    return None;
                }
            }
            b'}' => {
                if stack.pop() != Some(b'{') {
                    return None;
                }
            }
            b';' | b',' if stack.is_empty() => return None,
            b'=' if stack.is_empty() && bytes.get(at + 1) == Some(&b'>') => return None,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return None;
    }
    let first = code
        .trim_start()
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .unwrap_or("");
    if [
        "return",
        "echo",
        "print",
        "throw",
        "function",
        "fn",
        "class",
        "interface",
        "trait",
        "enum",
        "namespace",
        "use",
        "if",
        "foreach",
        "while",
        "for",
        "switch",
        "case",
        "yield",
        "include",
        "require",
        "global",
        "unset",
    ]
    .iter()
    .any(|keyword| first.eq_ignore_ascii_case(keyword))
    {
        return None;
    }
    if code
        .split_whitespace()
        .any(|word| matches!(word, "as" | "const" | "declare"))
    {
        return None;
    }
    let tail = code.trim_end();
    if tail.as_bytes().last().is_some_and(|b| {
        matches!(
            b,
            b'+' | b'-'
                | b'*'
                | b'/'
                | b'='
                | b'&'
                | b'|'
                | b'?'
                | b':'
                | b'.'
                | b'!'
                | b'<'
                | b'>'
                | b'$'
                | b'\\'
        )
    }) {
        return None;
    }
    if code.trim().is_empty() && !selected.starts_with(['\'', '"']) {
        return None;
    }
    Some(selected)
}
pub(crate) fn rewrites(doc: &Document, selection: Range) -> Vec<Rewrite> {
    if doc.language != "php"
        || doc.text.len() > 256 * 1024
        || !doc.text.trim_start().starts_with("<?php")
    {
        return vec![];
    }
    let Some(start) = text::offset(&doc.text, selection.start) else {
        return vec![];
    };
    let Some(end) = text::offset(&doc.text, selection.end) else {
        return vec![];
    };
    let Some(selected) = expression(&doc.text, start..end) else {
        return vec![];
    };
    let code = package_facts::code(&doc.text)
        .unwrap_or_default()
        .to_ascii_lowercase();
    ["dd", "collect"]
        .into_iter()
        .filter(|helper| {
            !code.match_indices("function").any(|(at, _)| {
                if at > 0 && php::is_identifier(code.as_bytes()[at - 1]) {
                    return false;
                }
                let tail = &code[at + 8..];
                if !tail.as_bytes().first().is_some_and(u8::is_ascii_whitespace) {
                    return false;
                }
                tail.trim_start()
                    .trim_start_matches('&')
                    .trim_start()
                    .strip_prefix(helper)
                    .is_some_and(|rest| rest.trim_start().starts_with('('))
            })
        })
        .map(|helper| Rewrite {
            title: crate::ui_text::format(
                "surround.action",
                "Surround with {helper}()",
                &[("helper", helper)],
            ),
            kind: "refactor.rewrite",
            edits: vec![(start..end, format!("\\{helper}({selected})"))],
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn marked(marked: &str) -> (Document, Range) {
        let start = marked.find('«').unwrap();
        let end = marked.find('»').unwrap() - 2;
        let source = marked.replace(['«', '»'], "");
        let range = Range::new(text::position(&source, start), text::position(&source, end));
        (
            Document {
                path: "/project/app/Use.php".into(),
                language: "php".into(),
                version: 1,
                text: source,
            },
            range,
        )
    }
    #[test]
    fn selected_expression_preserves_unicode_comments_and_only_wraps_requested_range() {
        let _host = openidea_plugin::mock::MockHost::install();
        let (doc, range) = marked("<?php /* 😀 */ return «$user->items(['label' => 'é'])»;");
        let found = rewrites(&doc, range);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].edits[0].1, "\\dd($user->items(['label' => 'é']))");
        assert_eq!(
            &doc.text[found[0].edits[0].0.clone()],
            "$user->items(['label' => 'é'])"
        );
    }
    #[test]
    fn full_quoted_literals_are_supported_but_quoted_substrings_and_statement_selections_are_withheld()
     {
        let _host=openidea_plugin::mock::MockHost::install();
        for source in ["<?php return «'hello'»;", "<?php return «[1, 2, 3]»;"] {
            let (doc, range) = marked(source);
            assert_eq!(rewrites(&doc, range).len(), 2, "{source}");
        }
        for source in [
            "<?php return '«hello»';",
            "<?php «return $value;»",
            "<?php return $«value»;",
            "<?php consume(«$a,$b»);",
            "<?php return «$value +»;",
            "<?php /* «word» */",
            "<?php function run(«Service» $value) {}",
            "<?php class A { public $value = «1»; }",
            "<?php const VALUE = «1»;",
            "<?php #[Attr(«1»)] class A {}",
        ] {
            let (doc, range) = marked(source);
            assert!(rewrites(&doc, range).is_empty(), "{source}");
        }
    }
    #[test]
    fn global_helpers_avoid_imported_aliases_and_local_declarations_are_withheld() {
        let _host = openidea_plugin::mock::MockHost::install();
        let (doc, range) = marked("<?php use function Vendor\\dd; return «$value»;");
        assert_eq!(rewrites(&doc, range)[0].edits[0].1, "\\dd($value)");
        let (doc, range) = marked("<?php function dd($value){} return «$value»;");
        assert_eq!(rewrites(&doc, range).len(), 1);
    }
    #[test]
    fn localized_action_titles_do_not_translate_php_helpers_or_selected_code() {
        let host = openidea_plugin::mock::MockHost::install();
        host.set_setting("uiLanguage", "\"ja\"");
        let (doc, range) = marked("<?php return «$value»;");
        let found = rewrites(&doc, range);
        assert_ne!(found[0].title, "Surround with dd()");
        assert!(found[0].title.contains("dd"));
        assert_eq!(found[0].edits[0].1, "\\dd($value)");
    }

    #[test]
    #[ignore = "requires php executable; lints actual selected-expression transforms"]
    fn actual_surround_edits_lint() {
        let _host = openidea_plugin::mock::MockHost::install();
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let (doc, range) = marked("<?php /* 😀 */ function run($value) {return «$value + 1»;}");
        for action in rewrites(&doc, range) {
            let mut rendered = doc.text.clone();
            for (range, replacement) in action.edits {
                rendered.replace_range(range, &replacement);
            }
            let mut child = Command::new("php")
                .arg("-l")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(rendered.as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{rendered}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
