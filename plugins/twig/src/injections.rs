//! Physical, closed HTML raw-text bodies only. Twig keeps ownership of holes.
use openidea_plugin::{
    Range,
    injections::{Document, Injection, Injections},
    text::position,
};

impl Injections for crate::Twig {
    fn injections(&mut self, doc: &Document) -> Vec<Injection> {
        if doc.language != "twig"
            || !doc.path.ends_with(".twig")
            || doc.text.len() > crate::MAX_SYNTAX_BYTES
        {
            return vec![];
        }
        bodies(&doc.text)
            .into_iter()
            .map(|(start, end, language)| Injection {
                range: Range::new(position(&doc.text, start), position(&doc.text, end)),
                language: language.into(),
            })
            .collect()
    }
}

fn bodies(text: &str) -> Vec<(usize, usize, &'static str)> {
    let bytes = text.as_bytes();
    let mut lexer = crate::Lexer {
        text,
        bytes,
        out: crate::Lexed::default(),
    };
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() && out.len() < 128 {
        if [b"{{", b"{%", b"{#"]
            .iter()
            .any(|s| bytes[i..].starts_with(*s))
        {
            i = lexer.twig(i).max(i + 1);
            continue;
        }
        if bytes[i..].starts_with(b"<!--") {
            let Some(end) = text[i + 4..].find("-->") else {
                break;
            };
            i += 4 + end + 3;
            continue;
        }
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let Some(end) = tag_end(bytes, i) else {
            break;
        };
        let header = &text[i + 1..end];
        let name_end = header.bytes().take_while(u8::is_ascii_alphabetic).count();
        let name = &header[..name_end];
        let Some(raw) = ["script", "style", "textarea", "title"]
            .into_iter()
            .find(|n| name.eq_ignore_ascii_case(n))
        else {
            i = end + 1;
            continue;
        };
        let Some((close, after)) = closing(bytes, end + 1, raw) else {
            break;
        };
        let body = &text[end + 1..close];
        if header[name_end..].trim().is_empty()
            && !body.trim().is_empty()
            && !["{{", "{%", "{#", "<?", "<!--", "<script", "<style"]
                .iter()
                .any(|s| body.to_ascii_lowercase().contains(s))
        {
            if let Some(language) = match raw {
                "script" => Some("javascript"),
                "style" => Some("css"),
                _ => None,
            } {
                out.push((end + 1, close, language));
            }
        }
        i = after;
    }
    out
}
fn tag_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut quote = None;
    for (i, &b) in bytes.iter().enumerate().skip(start + 1) {
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
        } else if matches!(b, b'\'' | b'"') {
            quote = Some(b);
        } else if b == b'>' {
            return Some(i);
        }
    }
    None
}
fn closing(bytes: &[u8], from: usize, name: &str) -> Option<(usize, usize)> {
    let needle = format!("</{name}");
    for i in from..bytes.len() {
        if bytes
            .get(i..i + needle.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(needle.as_bytes()))
        {
            let mut end = i + needle.len();
            while bytes.get(end).is_some_and(u8::is_ascii_whitespace) {
                end += 1;
            }
            if bytes.get(end) == Some(&b'>') {
                return Some((i, end + 1));
            }
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn islands_preserve_physical_utf16_and_crlf() {
        let text = "😀\r\n{% block body %}<SCRIPT>const café = 1;</SCRIPT><style>p {color:red}</style>{% endblock %}";
        let doc = Document {
            path: "/p/a.twig".into(),
            language: "twig".into(),
            text: text.into(),
            version: 1,
        };
        let found = crate::Twig::default().injections(&doc);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].language, "javascript");
        assert_eq!(
            found[0].range.start,
            position(text, text.find("const").unwrap())
        );
        assert_eq!(found[1].language, "css");
    }
    #[test]
    fn holes_comments_raw_attributes_and_unclosed_bodies_stay_quiet() {
        for text in [
            "<script>{{ value }}</script>",
            "<style>{% if x %}p{}{% endif %}</style>",
            "{# <script>x()</script> #}",
            "<!-- <style>p{}</style> -->",
            "{% verbatim %}<script>x()</script>{% endverbatim %}",
            "<script type='module'>x()</script>",
            "<script>x()",
            "<textarea><script>x()</script></textarea>",
            "{{ '<script>x()</script>' }}",
        ] {
            assert!(bodies(text).is_empty(), "{text}");
        }
        assert_eq!(
            bodies("<script>{{ x }}</script><style>p{}</style>").len(),
            1
        );
    }
}
