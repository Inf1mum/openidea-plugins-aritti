//! Current dotenv declaration/reference tokens. No lookup, discovery or runtime
//! environment resolution; key identity comes only from the established lexer.
use crate::{DotEnv, parse};
use openidea_plugin::semantic_tokens::{SemanticToken, SemanticTokens, TokenKind, TokenModifier};
use openidea_plugin::{Position, Range, providers::Document};
use std::collections::HashSet;
const MAX_TEXT: usize = 256 << 10;
const MAX_TOKENS: usize = 4096;
impl SemanticTokens for DotEnv {
    fn semantic_tokens(&mut self, doc: &Document) -> Vec<SemanticToken> {
        if doc.language != "dotenv" || doc.text.len() > MAX_TEXT {
            return Vec::new();
        }
        let lexical = parse::lex(&doc.text);
        let declared: HashSet<_> = parse::properties(&doc.text, &lexical)
            .into_iter()
            .map(|p| (p.key.start, p.key.end))
            .collect();
        let mut result = Vec::new();
        let mut cursor = 0;
        let mut position = Position::new(0, 0);
        for token in lexical {
            let declaration = token.kind == parse::Kind::Key
                && declared.contains(&(token.range.start, token.range.end));
            if !declaration && token.kind != parse::Kind::NestedKey {
                continue;
            }
            if token.range.start >= token.range.end || token.range.start < cursor {
                continue;
            }
            // Ordered lexer spans let UTF16 conversion visit source once, even
            // with thousands of interpolation references on one long line.
            let mut advance = |to: usize| {
                for c in doc.text[cursor..to].chars() {
                    if c == '\n' {
                        position.line += 1;
                        position.character = 0;
                    } else {
                        position.character += c.len_utf16() as u32;
                    }
                }
                cursor = to;
                position
            };
            let start = advance(token.range.start);
            let end = advance(token.range.end);
            if start.line != end.line {
                continue;
            }
            let mut token = SemanticToken::new(Range::new(start, end), TokenKind::Variable);
            if declaration {
                token = token.modifier(TokenModifier::Declaration);
            }
            result.push(token);
            if result.len() == MAX_TOKENS {
                break;
            }
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, text};
    fn doc(text: &str) -> Document {
        Document {
            path: "/unowned/.env".into(),
            language: "dotenv".into(),
            text: text.into(),
            version: 1,
        }
    }
    #[test]
    fn current_declarations_and_double_quoted_refs_are_real_variable_spans() {
        let host = MockHost::install().with_capabilities(&[]);
        let mut plugin = DotEnv::default();
        let source = "A=one\nB=\"😀${A}\"\nC='${LITERAL}'\n# ${COMMENT}\n";
        let tokens = plugin.semantic_tokens(&doc(source));
        let words: Vec<_> = tokens
            .iter()
            .map(|t| {
                assert_eq!(t.kind, TokenKind::Variable);
                let a = text::offset(source, t.range.start).unwrap();
                let b = text::offset(source, t.range.end).unwrap();
                &source[a..b]
            })
            .collect();
        assert_eq!(words, ["A", "B", "A", "C"]);
        assert!(tokens[0].modifiers.contains(&TokenModifier::Declaration));
        assert!(tokens[2].modifiers.is_empty());
        assert_eq!(
            tokens[2].range,
            Range::new(Position::new(1, 7), Position::new(1, 8))
        );
        assert!(host.requests().is_empty());
        assert!(host.process_runs().is_empty());
        assert!(plugin.projects.is_empty());
        let mut changed = doc("RENAMED=one\n");
        changed.version += 1;
        assert_eq!(plugin.semantic_tokens(&changed).len(), 1);
        assert!(plugin.projects.is_empty());
    }
    #[test]
    fn bounds_and_foreign_languages_are_quiet_with_nonempty_sorted_ranges() {
        let mut plugin = DotEnv::default();
        let mut foreign = doc("A=1");
        foreign.language = "json".into();
        assert!(plugin.semantic_tokens(&foreign).is_empty());
        assert!(
            plugin
                .semantic_tokens(&doc(&"x".repeat(MAX_TEXT + 1)))
                .is_empty()
        );
        let text = format!("A=\"{}\"", "${A}".repeat(MAX_TOKENS + 10));
        let tokens = plugin.semantic_tokens(&doc(&text));
        assert_eq!(tokens.len(), MAX_TOKENS);
        assert!(tokens.iter().all(|t| t.range.start.line == t.range.end.line
            && t.range.start.character < t.range.end.character));
        assert!(
            tokens
                .windows(2)
                .all(|w| (w[0].range.end.line, w[0].range.end.character)
                    <= (w[1].range.start.line, w[1].range.start.character))
        );
    }
}
