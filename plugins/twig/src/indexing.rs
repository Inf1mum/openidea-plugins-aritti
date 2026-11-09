//! Closed local for-loop binding scopes. Other tag families and dynamic context
//! access are deliberately not admitted; equal spellings do not imply identity.
use super::*;
use openidea_plugin::indexing::{Anchor, Document as IndexedDocument, DocumentIndexer, Edges};
struct Binding {
    name: String,
    at: Range<usize>,
    uses: Vec<Range<usize>>,
}
fn collect(text: &str) -> Option<Vec<Binding>> {
    if text.len() > 64 << 10 {
        return None;
    }
    let parsed = lex(text);
    if !parsed.unterminated.is_empty() || parsed.tags.len() > 512 || parsed.tokens.len() > 8192 {
        return None;
    }
    if parsed.tokens.iter().any(|t| {
        let raw = &text[t.range.clone()];
        matches!(raw, "_context" | "attribute" | "dump" | "include")
            || t.kind == Highlight::String && raw.contains("#{")
    }) {
        return None;
    }
    let mut bindings: Vec<Binding> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut cursor = 0;
    let uses =
        |from: usize, to: usize, stack: &[usize], bindings: &mut Vec<Binding>| {
            for token in parsed.tokens.iter().filter(|t| {
                from <= t.range.start && t.range.end <= to && t.kind == Highlight::Variable
            }) {
                let name = &text[token.range.clone()];
                if let Some(i) = stack
                    .iter()
                    .rev()
                    .copied()
                    .find(|i| bindings[*i].name == name)
                {
                    bindings[i].uses.push(token.range.clone());
                }
            }
        };
    for tag in &parsed.tags {
        uses(cursor, tag.range.start, &stack, &mut bindings);
        match tag.name.as_str() {
            "for" => {
                if stack.len() >= 32 {
                    return None;
                }
                let tokens: Vec<_> = parsed
                    .tokens
                    .iter()
                    .filter(|t| tag.range.start <= t.range.start && t.range.end <= tag.range.end)
                    .collect();
                let keyword = tokens
                    .iter()
                    .position(|t| &text[t.range.clone()] == "for")?;
                let name = tokens.get(keyword + 1)?;
                let word = &text[name.range.clone()];
                if name.kind != Highlight::Variable
                    || word == "loop"
                    || !word
                        .as_bytes()
                        .first()
                        .is_some_and(|b| is_identifier_start(*b))
                    || !word.bytes().all(is_identifier)
                    || &text[tokens.get(keyword + 2)?.range.clone()] != "in"
                {
                    return None;
                }
                if tokens
                    .iter()
                    .skip(keyword + 3)
                    .any(|t| matches!(&text[t.range.clone()], "if" | "recursive"))
                {
                    return None;
                }
                uses(
                    tokens.get(keyword + 2)?.range.end,
                    tag.range.end,
                    &stack,
                    &mut bindings,
                );
                stack.push(bindings.len());
                bindings.push(Binding {
                    name: word.into(),
                    at: name.range.clone(),
                    uses: vec![],
                });
            }
            "endfor" => {
                // A closing tag has no expressions or alternate binding syntax.
                let raw = &text[tag.range.clone()];
                if raw
                    .trim_start_matches("{%")
                    .trim_end_matches("%}")
                    .trim_matches([' ', '\t', '\r', '\n', '-', '~'])
                    != "endfor"
                {
                    return None;
                }
                stack.pop()?;
            }
            _ => return None,
        }
        cursor = tag.range.end;
    }
    uses(cursor, text.len(), &stack, &mut bindings);
    if !stack.is_empty() || bindings.iter().map(|b| 1 + b.uses.len()).sum::<usize>() > 4096 {
        return None;
    }
    Some(bindings)
}
impl DocumentIndexer for Twig {
    fn index(&mut self, doc: &IndexedDocument) -> Edges {
        let mut edges = Edges::default();
        if !doc.path.ends_with(".twig") {
            return edges;
        }
        let Some(bindings) = collect(&doc.text) else {
            return edges;
        };
        for b in bindings {
            let key = format!(
                "twig:loop:{}:{}:{}:{}",
                doc.path.len(),
                doc.path,
                b.at.start,
                b.name
            );
            let anchor = |r: Range<usize>| Anchor {
                key: key.clone(),
                range: EditorRange::new(position(&doc.text, r.start), position(&doc.text, r.end)),
            };
            edges.definitions.push(anchor(b.at));
            edges.references.extend(b.uses.into_iter().map(anchor));
        }
        edges
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn doc(text: &str) -> IndexedDocument {
        IndexedDocument {
            path: "/p/a.twig".into(),
            text: text.into(),
            symbols: vec![],
        }
    }
    #[test]
    fn nested_shadowing_and_iterable_use_outer_binding() {
        let text = "😀{% for item in items %}{{ item.name }}{{ object.item }}{% for item in item.children %}{{ item }}{% endfor %}{{ item }}{% endfor %}{{ item }}";
        let edges = Twig::default().index(&doc(text));
        assert_eq!(edges.definitions.len(), 2);
        assert_ne!(edges.definitions[0].key, edges.definitions[1].key);
        assert_eq!(
            edges
                .references
                .iter()
                .filter(|a| a.key == edges.definitions[0].key)
                .count(),
            3
        );
        assert_eq!(
            edges
                .references
                .iter()
                .filter(|a| a.key == edges.definitions[1].key)
                .count(),
            1
        );
        for a in edges.definitions.iter().chain(&edges.references) {
            assert_eq!(
                &text[offset(text, a.range.start).unwrap()..offset(text, a.range.end).unwrap()],
                "item"
            );
        }
    }
    #[test]
    fn dynamic_incomplete_and_assignment_scopes_do_not_publish() {
        for text in [
            "{% for item in items %}{{ item }}",
            "{% for item in items %}{% set item = other %}{{ item }}{% endfor %}",
            "{% for item in items %}{{ _context['item'] }}{% endfor %}",
            "{% for item in items if item.active %}{{ item }}{% endfor %}",
            "{% for item in items %}{{ \"#{item}\" }}{% endfor %}",
            "{% for item in items %}{% else %}{{ item }}{% endfor %}",
        ] {
            assert!(
                Twig::default().index(&doc(text)).definitions.is_empty(),
                "{text}"
            );
        }
    }
}
