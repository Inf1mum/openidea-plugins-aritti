//! Source-context quick fixes for strict JSON specs. An exact token scanner
//! (member extents) emits one TextEdit; comments, duplicate keys, trailing
//! commas or any other non-strict JSON make the whole document refused.
use crate::operation_actions::{Body, METHODS, RESPONSES, operation_skeleton, parameter_skeleton, render};
use crate::*;

struct Node {
    start: usize,
    end: usize,
    kind: Kind,
}

enum Kind {
    Object(Vec<(String, usize, Node)>),
    Array(Vec<Node>),
    Scalar,
}

struct Scanner<'a> {
    text: &'a str,
    pos: usize,
    depth: usize,
}

impl Scanner<'_> {
    fn skip_ws(&mut self) {
        while matches!(self.text.as_bytes().get(self.pos), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        self.skip_ws();
        if self.text.as_bytes().get(self.pos) == Some(&byte) {
            self.pos += 1;
            return true;
        }
        false
    }

    fn string(&mut self) -> Option<String> {
        let bytes = self.text.as_bytes();
        let open = self.pos;
        if bytes.get(open) != Some(&b'"') {
            return None;
        }
        let mut i = open + 1;
        while let Some(&b) = bytes.get(i) {
            match b {
                b'"' => {
                    self.pos = i + 1;
                    return Some(json::unescape(&self.text[open + 1..i]));
                }
                b'\\' => i += 2,
                0..=0x1f => return None,
                _ => i += 1,
            }
        }
        None
    }

    fn value(&mut self) -> Option<Node> {
        self.skip_ws();
        let start = self.pos;
        let kind = match *self.text.as_bytes().get(start)? {
            b'{' => {
                self.enter()?;
                self.pos += 1;
                let mut members: Vec<(String, usize, Node)> = Vec::new();
                if !self.eat(b'}') {
                    loop {
                        self.skip_ws();
                        let key_start = self.pos;
                        let key = self.string()?;
                        if members.iter().any(|(k, ..)| *k == key) || !self.eat(b':') {
                            return None;
                        }
                        members.push((key, key_start, self.value()?));
                        if self.eat(b'}') {
                            break;
                        }
                        if !self.eat(b',') {
                            return None;
                        }
                    }
                }
                self.depth -= 1;
                Kind::Object(members)
            }
            b'[' => {
                self.enter()?;
                self.pos += 1;
                let mut items = Vec::new();
                if !self.eat(b']') {
                    loop {
                        items.push(self.value()?);
                        if self.eat(b']') {
                            break;
                        }
                        if !self.eat(b',') {
                            return None;
                        }
                    }
                }
                self.depth -= 1;
                Kind::Array(items)
            }
            b'"' => {
                self.string()?;
                Kind::Scalar
            }
            _ => {
                let rest = &self.text[start..];
                let len = rest.find(|c: char| !(c.is_ascii_alphanumeric() || "+-.".contains(c)))?.min(rest.len());
                let token = &rest[..len];
                let literal = matches!(token, "true" | "false" | "null");
                if !literal && token.parse::<f64>().is_err() || token.starts_with('+') || token.starts_with('.') {
                    return None;
                }
                self.pos += len;
                Kind::Scalar
            }
        };
        Some(Node { start, end: self.pos, kind })
    }

    fn enter(&mut self) -> Option<()> {
        self.depth += 1;
        (self.depth <= 64).then_some(())
    }
}

fn parse(text: &str) -> Option<Node> {
    let mut scanner = Scanner { text, pos: 0, depth: 0 };
    let root = scanner.value()?;
    scanner.skip_ws();
    (scanner.pos == text.len() && matches!(root.kind, Kind::Object(_))).then_some(root)
}

fn member<'a>(node: &'a Node, name: &str) -> Option<&'a Node> {
    match &node.kind {
        Kind::Object(members) => members.iter().find(|(k, ..)| k == name).map(|(.., v)| v),
        _ => None,
    }
}

fn line_indent(text: &str, at: usize) -> &str {
    let line = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let rest = &text[line..];
    &rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()]
}

fn unit(text: &str) -> String {
    text.lines()
        .map(|l| &l[..l.len() - l.trim_start_matches([' ', '\t']).len()])
        .find(|i| !i.is_empty())
        .unwrap_or("  ")
        .to_string()
}

/// One edit inserting `prefix + body` as a new member (prefix = `"key": `) or array item.
fn insert(text: &str, container: &Node, prefix: &str, body: &Body) -> Option<TextEdit> {
    let unit = unit(text);
    let (first_start, last_end) = match &container.kind {
        Kind::Object(m) => (m.first().map(|(_, k, _)| *k), m.last().map(|(.., v)| v.end)),
        Kind::Array(a) => (a.first().map(|v| v.start), a.last().map(|v| v.end)),
        Kind::Scalar => return None,
    };
    let (from, to, new) = match (first_start, last_end) {
        (Some(first), Some(end)) => {
            let indent = line_indent(text, first);
            if text[text[..first].rfind('\n').map_or(0, |i| i + 1)..first].trim().is_empty() && text[container.start..first].contains('\n') {
                (end, end, format!(",\n{indent}{prefix}{}", render(body, indent, &unit, true)))
            } else {
                (end, end, format!(", {prefix}{}", render(body, "", &unit, false)))
            }
        }
        _ if text.contains('\n') => {
            let indent = line_indent(text, container.start);
            let inner = format!("{indent}{unit}");
            (container.start + 1, container.end - 1, format!("\n{inner}{prefix}{}\n{indent}", render(body, &inner, &unit, true)))
        }
        _ => (container.start + 1, container.end - 1, format!("{prefix}{}", render(body, "", &unit, false))),
    };
    Some(TextEdit { range: spring_text::range(text, from, to), text: new })
}

fn code_action(doc: &Document, title: String, edit: TextEdit) -> CodeAction {
    CodeAction { title, kind: Some("quickfix".into()), preferred: false, edits: vec![(doc.path.clone(), vec![edit])], data: None }
}

pub(crate) fn actions(doc: &Document, tokens: &[&str]) -> Vec<CodeAction> {
    let text = doc.text.as_str();
    let Some(root) = parse(text) else {
        return Vec::new();
    };
    let Some(paths) = member(&root, "paths") else {
        return Vec::new();
    };
    let mut target = paths;
    for token in tokens {
        let Some(next) = member(target, token) else {
            return Vec::new();
        };
        target = next;
    }
    let Kind::Object(members) = &target.kind else {
        return Vec::new();
    };
    let has = |name: &str| members.iter().any(|(k, ..)| k == name);
    let mut out = Vec::new();
    let mut push = |title: String, prefix: String, body: Body, node: &Node| {
        if let Some(edit) = insert(text, node, &prefix, &body) {
            out.push(code_action(doc, title, edit));
        }
    };
    let parameters = |push: &mut dyn FnMut(String, String, Body, &Node)| match member(target, "parameters") {
        None => push("Add query parameter".into(), "\"parameters\": ".into(), Body::Array(vec![parameter_skeleton()]), target),
        Some(list) if matches!(list.kind, Kind::Array(_)) => push("Add query parameter".into(), String::new(), parameter_skeleton(), list),
        Some(_) => {}
    };
    match tokens {
        [] => {
            let name = (1..).map(|n| if n == 1 { "/new-path".to_string() } else { format!("/new-path-{n}") }).find(|c| !has(c)).unwrap_or_default();
            push("Add path".into(), format!("{name:?}: "), Body::Object(vec![("get", operation_skeleton())]), target);
        }
        [_] => {
            if has("$ref") {
                return Vec::new();
            }
            for method in METHODS.iter().filter(|m| !has(m)) {
                push(format!("Add {method} operation"), format!("{method:?}: "), operation_skeleton(), target);
            }
            parameters(&mut push);
        }
        [_, method] if METHODS.contains(method) => parameters(&mut push),
        [_, method, "responses"] if METHODS.contains(method) => {
            for code in RESPONSES.iter().filter(|c| !has(c)) {
                push(format!("Add {code} response"), format!("{code:?}: "), Body::Object(vec![("description", Body::Str("TODO"))]), target);
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "{\n  \"openapi\": \"3.0.3\",\n  \"paths\": {\n    \"/pets\": {\n      \"get\": {\n        \"responses\": {\n          \"200\": {\n            \"description\": \"ok\"\n          }\n        }\n      }\n    }\n  }\n}\n";

    fn acts(text: &str, needle: &str) -> Vec<CodeAction> {
        let doc = Document { path: "/p/openapi.json".into(), language: "json".into(), text: text.into(), version: 1 };
        let caret = position(text, text.find(needle).unwrap() + 1);
        operation_actions::actions(&doc, Range::new(caret, caret))
    }

    fn apply(text: &str, action: &CodeAction) -> String {
        let edit = &action.edits[0].1[0];
        let (from, to) = (offset(text, edit.range.start).unwrap(), offset(text, edit.range.end).unwrap());
        format!("{}{}{}", &text[..from], edit.text, &text[to..])
    }

    fn titles(found: &[CodeAction]) -> Vec<&str> {
        found.iter().map(|a| a.title.as_str()).collect()
    }

    fn valid(text: &str) -> serde_json::Value {
        serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
    }

    #[test]
    fn adds_operation_and_response_with_matching_indentation() {
        let found = acts(SPEC, "\"/pets\"");
        assert_eq!(titles(&found), ["Add post operation", "Add put operation", "Add delete operation", "Add query parameter"]);
        let updated = apply(SPEC, &found[0]);
        assert!(updated.contains("      },\n      \"post\": {\n        \"summary\": \"TODO\""), "{updated}");
        assert_eq!(valid(&updated)["paths"]["/pets"]["post"]["responses"]["200"]["description"], "OK");

        let found = acts(SPEC, "\"responses\"");
        assert_eq!(titles(&found), ["Add 400 response", "Add 404 response", "Add default response"]);
        let updated = apply(SPEC, &found[2]);
        assert_eq!(valid(&updated)["paths"]["/pets"]["get"]["responses"]["default"]["description"], "TODO");
    }

    #[test]
    fn adds_path_with_unique_name_and_parameters() {
        let found = acts(SPEC, "\"paths\"");
        assert_eq!(titles(&found), ["Add path"]);
        let once = apply(SPEC, &found[0]);
        assert!(valid(&once)["paths"]["/new-path"]["get"]["responses"]["200"].is_object());
        let twice = apply(&once, &acts(&once, "\"paths\"")[0]);
        assert!(valid(&twice)["paths"]["/new-path-2"].is_object());

        let found = acts(SPEC, "\"get\"");
        assert_eq!(titles(&found), ["Add query parameter"]);
        let updated = apply(SPEC, &found[0]);
        let param = &valid(&updated)["paths"]["/pets"]["get"]["parameters"][0];
        assert_eq!((param["name"].as_str(), param["in"].as_str(), param["schema"]["type"].as_str()), (Some("TODO"), Some("query"), Some("string")));
        let again = apply(&updated, &acts(&updated, "\"get\"")[0]);
        assert_eq!(valid(&again)["paths"]["/pets"]["get"]["parameters"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn handles_compact_and_empty_objects() {
        let compact = r#"{"openapi":"3.0.3","paths":{"/a":{"get":{}}}}"#;
        let updated = apply(compact, &acts(compact, "\"/a\"")[0]);
        assert_eq!(valid(&updated)["paths"]["/a"]["post"]["summary"], "TODO");
        let empty = "{\n  \"openapi\": \"3.0.3\",\n  \"paths\": {}\n}\n";
        let updated = apply(empty, &acts(empty, "\"paths\"")[0]);
        assert!(valid(&updated)["paths"]["/new-path"].is_object(), "{updated}");
        let empty_item = "{\"openapi\":\"3.0.3\",\"paths\":{\"/a\":{}}}";
        let updated = apply(empty_item, &acts(empty_item, "\"/a\"")[0]);
        assert!(valid(&updated)["paths"]["/a"]["get"].is_object());
    }

    #[test]
    fn refuses_duplicates_comments_trailing_commas_and_refs() {
        for text in [
            "{\"openapi\":\"3.0.3\",\"paths\":{\"/a\":{},\"/a\":{}}}",
            "{\n  // c\n  \"openapi\":\"3.0.3\",\"paths\":{\"/a\":{}}}",
            "{\"openapi\":\"3.0.3\",\"paths\":{\"/a\":{},}}",
            "{\"openapi\":\"3.0.3\",\"paths\":{\"/a\":{}}} x",
            "{\"openapi\":\"3.0.3\",\"paths\":{\"/a\":{\"$ref\":\"#/x\"}}}",
        ] {
            assert!(acts(text, "\"/a\"").is_empty(), "{text}");
        }
    }
}
