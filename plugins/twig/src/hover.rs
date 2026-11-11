//! Current lexer/source facts only. Symfony owns configured framework references.
use crate::*;
use openidea_plugin::hover::{Hover, HoverProvider};
const TAGS: &[&str] = &[
    "apply",
    "autoescape",
    "block",
    "deprecated",
    "do",
    "embed",
    "extends",
    "flush",
    "for",
    "from",
    "if",
    "import",
    "include",
    "macro",
    "sandbox",
    "set",
    "use",
    "verbatim",
    "with",
];
const FILTERS: &[&str] = &[
    "abs",
    "batch",
    "capitalize",
    "column",
    "convert_encoding",
    "date",
    "date_modify",
    "default",
    "e",
    "escape",
    "filter",
    "first",
    "format",
    "join",
    "json_encode",
    "keys",
    "last",
    "length",
    "lower",
    "map",
    "merge",
    "nl2br",
    "number_format",
    "raw",
    "reduce",
    "replace",
    "reverse",
    "round",
    "slice",
    "sort",
    "spaceless",
    "split",
    "striptags",
    "title",
    "trim",
    "upper",
    "url_encode",
];
fn escape(s: &str) -> String {
    s.chars()
        .take(512)
        .flat_map(|c| {
            if c.is_control() {
                vec![' ']
            } else if "\\`*_{}[]<>#|!".contains(c) {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect()
}
fn documentation(kind: &str, name: &str) -> String {
    let page = if name == "e" { "escape" } else { name };
    let summary = match (kind, page) {
        ("filter", "default") => {
            "Uses a fallback for undefined or empty values; false counts as empty."
        }
        ("filter", "escape") => {
            "Escapes output for a selected context; e is an alias. Runtime escapers and autoescape configuration are not inferred."
        }
        ("tag", "include") => {
            "Renders another template. The only modifier excludes the calling context; configured loader/path resolution belongs to Symfony."
        }
        ("tag", "macro") => {
            "Declares a reusable template fragment with arguments. Caller variables are not implicitly available inside a macro."
        }
        _ => {
            "Language documentation reference; installed version, custom overrides and runtime behavior are not resolved."
        }
    };
    format!(
        "**Twig {kind} documentation** `{name}`\n\n{summary}\n\n[Twig3 documentation](https://twig.symfony.com/doc/3.x/{kind}s/{page}.html)\n\nTwig owns language/source documentation. Symfony owns routes, services, translations, configured template namespaces and framework extension resolution."
    )
}
impl HoverProvider for Twig {
    fn hover(&mut self, doc: &Document, at: openidea_plugin::Position) -> Option<Hover> {
        let _syntax=crate::delimiters::Syntax::scope()?;
        if doc.language != "twig"
            || !doc.path.ends_with(".twig")
            || doc.text.len() > 128 << 10
            || !host::setting_bool("hover").unwrap_or(true)
        {
            return None;
        }
        let at = offset(&doc.text, at)?;
        let parsed = lex(&doc.text);
        if parsed.tags.len() > MAX_STRUCTURE_TAGS
            || parsed.tokens.len() > MAX_SYNTAX_TOKENS
            || !parsed.unterminated.is_empty()
        {
            return None;
        }
        let (index, token) = parsed
            .tokens
            .iter()
            .enumerate()
            .find(|(_, t)| t.range.start <= at && at < t.range.end)?;
        let word = &doc.text[token.range.clone()];
        let span = EditorRange::new(
            position(&doc.text, token.range.start),
            position(&doc.text, token.range.end),
        );
        if token.kind == Highlight::Keyword {
            if let Some(tag) = parsed.tags.iter().find(|tag| {
                tag.range.start <= token.range.start && token.range.end <= tag.range.end
            }) {
                let first = parsed.tokens.iter().find(|t| {
                    tag.range.start <= t.range.start
                        && t.range.end <= tag.range.end
                        && t.kind == Highlight::Keyword
                })?;
                let name = if matches!(word, "else" | "elseif") {
                    "if"
                } else {
                    word.strip_prefix("end").unwrap_or(word)
                };
                if first.range == token.range && TAGS.contains(&name) {
                    return Some(Hover::new(documentation("tag", name)).range(span));
                }
            }
        }
        if index > 0
            && &doc.text[parsed.tokens[index - 1].range.clone()] == "|"
            && FILTERS.contains(&word)
        {
            return Some(Hover::new(documentation("filter", word)).range(span));
        }
        let declaration = parsed
            .tags
            .iter()
            .any(|tag| tag.name == "macro" && tag.argument.as_ref() == Some(&token.range));
        let self_call = token.kind == Highlight::FunctionCall
            && index >= 2
            && &doc.text[parsed.tokens[index - 1].range.clone()] == "."
            && &doc.text[parsed.tokens[index - 2].range.clone()] == "_self"
            && (index < 3 || &doc.text[parsed.tokens[index - 3].range.clone()] != ".");
        if (declaration || self_call)
            && checks::check(&doc.text).is_empty()
            && checks::brackets(&doc.text).is_empty()
        {
            let declarations: Vec<_> = navigate::macros(&doc.text)
                .into_iter()
                .filter(|m| m.name == word)
                .collect();
            if let [m] = declarations.as_slice() {
                if m.arguments.len() > 512 {
                    return None;
                }
                return Some(Hover::new(format!("**Current-template macro** `{}`\n\nDeclared arguments: {}\n\nOne unique current-source declaration. Default expressions are displayed as source, not evaluated. Imported/external macro ownership, argument types, call validity and runtime execution remain unresolved. No project files are read.",escape(&m.name),escape(&m.arguments))).range(span));
            }
        }
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{Position, mock::MockHost};
    fn doc(text: &str) -> Document {
        Document {
            path: "/project/templates/page.twig".into(),
            language: "twig".into(),
            version: 8,
            text: text.into(),
        }
    }
    fn at(text: &str, needle: &str) -> Position {
        position(text, text.rfind(needle).unwrap())
    }
    #[test]
    fn tag_filter_and_current_macro_docs_are_contextual_utf16_and_source_only() {
        let h = MockHost::install();
        let text = "😀\r\n{% macro badge(label='item') %}{{ label }}{% endmacro %}\r\n{% include 'header.twig' only %}{{ name|default('none') }}{{ _self.badge('x') }}";
        let d = doc(text);
        let mut twig = Twig::default();
        for (word, claim) in [
            ("include", "Twig tag documentation"),
            ("default", "Twig filter documentation"),
            ("badge", "Current-template macro"),
        ] {
            let p = at(text, word);
            let hover = twig.hover(&d, p).unwrap();
            assert!(hover.markdown.contains(claim));
            assert_eq!(hover.range.unwrap().start, p);
        }
        assert!(h.process_runs().is_empty());
        assert_eq!(h.fetches(), 0);
        assert!(h.requests().is_empty());
    }
    #[test]
    fn symfony_framework_names_strings_comments_raw_member_calls_and_unknowns_are_quiet() {
        let h = MockHost::install();
        h.add_file("symfony.lock", "{}");
        let mut twig = Twig::default();
        for text in [
            "{{ path('home') }}",
            "{{ text|trans }}",
            "{# default #}",
            "{% verbatim %}{{ name|default('x') }}{% endverbatim %}",
            "{{ 'default' }}",
            "<p>include</p>",
            "{{ object.default() }}",
            "{{ x|unknown }}",
        ] {
            let word = if text.contains("default") {
                "default"
            } else if text.contains("path") {
                "path"
            } else if text.contains("trans") {
                "trans"
            } else if text.contains("unknown") {
                "unknown"
            } else {
                "include"
            };
            assert!(twig.hover(&doc(text), at(text, word)).is_none(), "{text}");
        }
        let text = "{{ name|default('x') }}";
        assert!(
            twig.hover(&doc(text), at(text, "default")).is_some(),
            "core language docs do not compete with framework reference hover"
        );
    }
    #[test]
    fn duplicate_external_and_shadowed_self_macros_do_not_get_arbitrary_signatures() {
        let _h = MockHost::install();
        let mut twig = Twig::default();
        for text in [
            "{% macro badge(x) %}{% endmacro %}{% macro badge(y) %}{% endmacro %}{{ _self.badge() }}",
            "{% import 'forms.twig' as forms %}{{ forms.badge() }}",
            "{% macro badge(x) %}{% endmacro %}{{ object._self.badge() }}",
        ] {
            assert!(
                twig.hover(&doc(text), at(text, "badge")).is_none(),
                "{text}"
            );
        }
    }
    #[test]
    fn disabled_and_oversized_source_do_not_parse_hover() {
        let h = MockHost::install();
        let mut twig = Twig::default();
        let d = doc("{{ name|default('x') }}");
        h.set_setting("hover", "false");
        assert!(twig.hover(&d, at(&d.text, "default")).is_none());
        h.set_setting("hover", "true");
        let huge = doc(&format!("{}{}", d.text, " ".repeat(128 << 10)));
        assert!(twig.hover(&huge, at(&huge.text, "default")).is_none());
    }
}
