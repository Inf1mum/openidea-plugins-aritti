use std::ops::Range;

use crate::lex;

pub const UNCLOSED_TAG: &str = "unclosed-tag";
pub const UNEXPECTED_END_TAG: &str = "unexpected-end-tag";
pub const UNTERMINATED: &str = "unterminated-delimiter";
pub const BRACKET: &str = "unbalanced-bracket";

const PAIRED: &[&str] = &["apply", "autoescape", "block", "cache", "embed", "for", "if", "macro", "sandbox", "set", "verbatim", "with"];

pub struct Finding {
    pub code: &'static str,
    pub range: Range<usize>,
    pub message: String,
}

pub fn check(text: &str) -> Vec<Finding> {
    let lexed = lex(text);
    let mut findings = Vec::new();
    let mut open: Vec<(&str, &Range<usize>)> = Vec::new();

    for tag in &lexed.tags {
        if let Some(name) = tag.name.strip_prefix("end") {
            if !PAIRED.contains(&name) {
                continue;
            }

            let Some(at) = open.iter().rposition(|(n, _)| *n == name) else {
                findings.push(Finding {
                    code: UNEXPECTED_END_TAG,
                    range: tag.range.clone(),
                    message: format!("{{% end{name} %}} has no matching {{% {name} %}}."),
                });
                continue;
            };

            for (name, range) in open.split_off(at + 1) {
                findings.push(unclosed(name, range));
            }
            open.pop();
            continue;
        }

        if tag.paired && PAIRED.contains(&tag.name.as_str()) {
            open.push((tag.name.as_str(), &tag.range));
        }
    }

    findings.extend(open.into_iter().map(|(name, range)| unclosed(name, range)));
    findings.extend(lexed.unterminated.iter().map(|range| Finding {
        code: UNTERMINATED,
        range: range.clone(),
        message: format!("\"{}\" is never closed.", &text[range.clone()]),
    }));
    findings
}

/// Brackets that do not pair inside `{{ … }}` and `{% … %}`; strings, comments and `verbatim` are skipped.
/// A block that never ends is left to the unterminated-delimiter check.
pub fn brackets(text: &str) -> Vec<Finding> {
    let bytes = text.as_bytes();
    let mut findings = Vec::new();
    let mut i = 0;

    let syntax=crate::delimiters::Syntax::current();
    while i < bytes.len() {
        if bytes[i..].starts_with(syntax.values[4].as_bytes()) {
            i=text[i+syntax.values[4].len()..].find(&syntax.values[5]).map_or(bytes.len(),|end|i+syntax.values[4].len()+end+syntax.values[5].len());continue;
        }
        let (is_tag,open_len,closer)=if bytes[i..].starts_with(syntax.values[0].as_bytes()) {(true,syntax.values[0].len(),syntax.values[1].as_str())}
            else if bytes[i..].starts_with(syntax.values[2].as_bytes()){(false,syntax.values[2].len(),syntax.values[3].as_str())}
            else {i+=1;continue;};
        let start = i;
        let mut stack: Vec<(u8, usize)> = Vec::new();
        let mut j = i + open_len;
        let mut ended = false;
        let mut reported = false;

        while j < bytes.len() {
            if stack.is_empty() && bytes[j..].starts_with(closer.as_bytes()) {ended=true;break;}
            let b = bytes[j];
            match b {
                b'\'' | b'"' => {
                    j += 1;
                    while j < bytes.len() && bytes[j] != b {
                        j += if bytes[j] == b'\\' { 2 } else { 1 };
                    }
                }
                b'(' | b'[' | b'{' => stack.push((b, j)),
                b')' | b']' | b'}' => {
                    let wanted = match b {
                        b')' => b'(',
                        b']' => b'[',
                        _ => b'{',
                    };
                    match stack.last() {
                        Some((open, _)) if *open == wanted => {
                            stack.pop();
                        }
                        Some((open, at)) => {
                            let message = format!("\"{}\" is closed by \"{}\".", *open as char, b as char);
                            findings.push(Finding { code: BRACKET, range: *at..*at + 1, message });
                            reported = true;
                            break;
                        }
                        None if bytes[j..].starts_with(closer.as_bytes()) => {
                            ended = true;
                            break;
                        }
                        None => {
                            findings.push(Finding { code: BRACKET, range: j..j + 1, message: format!("Unexpected \"{}\".", b as char) });
                            reported = true;
                            break;
                        }
                    }
                }
                b'%' | b'#' if stack.is_empty() && bytes[j..].starts_with(closer.as_bytes()) => {
                    ended = true;
                    break;
                }
                _ => {}
            }
            j += 1;
        }

        if ended {
            let body = text[start + open_len..j].trim_matches(['-', '~', ' ']);
            let skips = is_tag && (body == "verbatim" || body == "raw");
            i = if skips { text[j..].find(&format!("{} end",syntax.values[0])).map_or(bytes.len(), |end| j + end) } else { j + closer.len() };
            continue;
        }

        if !reported {
            if let Some((open, at)) = stack.last().filter(|_| j < bytes.len() || !closer.is_empty()) {
                if text[start..].contains(closer) {
                    findings.push(Finding { code: BRACKET, range: *at..*at + 1, message: format!("\"{}\" is never closed.", *open as char) });
                }
            }
        }
        i = start + open_len;
    }

    findings
}

fn unclosed(name: &str, range: &Range<usize>) -> Finding {
    Finding {
        code: UNCLOSED_TAG,
        range: range.clone(),
        message: format!("Tag \"{name}\" is never closed: missing {{% end{name} %}}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(text: &str) -> Vec<(&'static str, String)> {
        check(text).into_iter().map(|f| (f.code, text[f.range].to_owned())).collect()
    }

    #[test]
    fn balanced_template_is_clean() {
        let text = "{% extends 'a' %}{% block x %}{% if a %}{% else %}{% endif %}{% endblock %}{% block t 'T' %}{% set a = 1 %}{% verbatim %}{% if %}{% endverbatim %}";
        assert!(codes(text).is_empty());
    }

    #[test]
    fn reports_unclosed_and_unexpected_end_tags() {
        assert_eq!(codes("{% for a in b %}{% if a %}{% endfor %}"), [("unclosed-tag", "{% if a %}".into())]);
        assert_eq!(codes("{% endif %}"), [("unexpected-end-tag", "{% endif %}".into())]);
        assert_eq!(codes("{% set a %}x"), [("unclosed-tag", "{% set a %}".into())]);
    }

    #[test]
    fn reports_unpaired_brackets_inside_blocks_only() {
        let found = |text: &str| brackets(text).into_iter().map(|f| (f.code, text[f.range].to_owned())).collect::<Vec<_>>();
        assert!(found("{{ f(a, [1, 2], {'k': {'n': 1}}) }}{% if (a and b) %}x{% endif %}").is_empty());
        assert!(found("{{ ')' ~ \"(\" }}{# ( #}{% verbatim %}{{ ( }}{% endverbatim %}<p>(</p>").is_empty());
        assert_eq!(found("{{ f(a] }}"), [("unbalanced-bracket", "(".into())]);
        assert_eq!(found("{{ a) }}"), [("unbalanced-bracket", ")".into())]);
        assert_eq!(found("{% if (a %}x"), [("unbalanced-bracket", "(".into())]);
        assert!(found("<p>{{ name </p>").is_empty());
    }

    #[test]
    fn reports_unterminated_delimiters() {
        assert_eq!(codes("<p>{{ name </p>"), [("unterminated-delimiter", "{{".into())]);
        assert_eq!(codes("{# note"), [("unterminated-delimiter", "{#".into())]);
        assert!(codes("{{ '}}' }}").is_empty());
    }
}
