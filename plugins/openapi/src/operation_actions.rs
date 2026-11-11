//! Source-context "Add operation" / "Add response" for YAML block style.
//! Inserts a minimal skeleton at the exact end of an existing block mapping;
//! refuses flow style, anchors/aliases, multi-document and tab indentation.
use crate::*;

const METHODS: [&str; 4] = ["get", "post", "put", "delete"];
const RESPONSES: [&str; 4] = ["200", "400", "404", "default"];

struct Block {
    indent: usize,
    insert_at: usize,
    keys: Vec<String>,
    needs_newline: bool,
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            out.push(u8::from_str_radix(text.get(i + 1..i + 3)?, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn unquote(key: &str) -> &str {
    key.trim_matches(|c| c == '\'' || c == '"')
}

fn plain_document(text: &str) -> bool {
    let mut markers = 0;
    for line in text.lines() {
        if line.starts_with("...") || line.starts_with("---") {
            markers += 1;
            if markers > 1 || line.starts_with("...") {
                return false;
            }
        }
        if line.starts_with('\t') || line.trim_start().starts_with('\t') {
            return false;
        }
        if line.split_whitespace().any(|t| t.starts_with('&') || t.starts_with('*') || t == "<<:") {
            return false;
        }
    }
    true
}

fn block(text: &str, caret: usize, name: &str) -> Option<Block> {
    let line_start = text[..caret.min(text.len())].rfind('\n').map_or(0, |i| i + 1);
    let mut lines = text[line_start..].split_inclusive('\n');
    let header = lines.next()?.trim_end_matches(['\n', '\r']);
    let key = header.trim_start().strip_suffix(':')?;
    if unquote(key) != name || key.ends_with(char::is_whitespace) {
        return None;
    }
    let indent = header.len() - header.trim_start().len();
    let mut pos = line_start;
    pos += text[line_start..].split_inclusive('\n').next()?.len();
    let mut found: Option<Block> = None;
    for raw in lines {
        let line = raw.trim_end_matches(['\n', '\r']);
        let trimmed = line.trim_start();
        let end = pos + raw.len();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            pos = end;
            continue;
        }
        let here = line.len() - trimmed.len();
        if here <= indent {
            break;
        }
        let block = found.get_or_insert_with(|| Block { indent: here, insert_at: end, keys: Vec::new(), needs_newline: false });
        if here < block.indent || (here == block.indent && block.keys.is_empty() && (trimmed.starts_with(['{', '[', '-', '?', '|', '>']))) {
            return None;
        }
        if here == block.indent {
            let key = trimmed.split(':').next().unwrap_or("");
            if !trimmed.contains(':') {
                return None;
            }
            block.keys.push(unquote(key.trim()).to_string());
        }
        block.insert_at = end;
        block.needs_newline = !raw.ends_with('\n');
        pos = end;
    }
    found
}

fn action(doc: &Document, title: String, block: &Block, body: &str) -> CodeAction {
    let pad = " ".repeat(block.indent);
    let mut text = if block.needs_newline { "\n".to_string() } else { String::new() };
    for line in body.lines() {
        text.push_str(&format!("{pad}{line}\n"));
    }
    let range = spring_text::range(&doc.text, block.insert_at, block.insert_at);
    CodeAction {
        title,
        kind: Some("quickfix".into()),
        preferred: false,
        edits: vec![(doc.path.clone(), vec![TextEdit { range, text }])],
        data: None,
    }
}

pub(crate) fn actions(doc: &Document, selection: Range) -> Vec<CodeAction> {
    if spec::is_json(&doc.text) || !plain_document(&doc.text) {
        return Vec::new();
    }
    let Some(at) = offset(&doc.text, selection.start) else {
        return Vec::new();
    };
    let Some(proof) = pointer_status::Proof::parse(&doc.text) else {
        return Vec::new();
    };
    let Some(pointer) = proof.copy_pointer_at(at) else {
        return Vec::new();
    };
    let Some(pointer) = pointer.strip_prefix("#/paths/").and_then(percent_decode) else {
        return Vec::new();
    };
    let tokens: Vec<&str> = pointer.split('/').collect();
    let decode = |t: &str| t.replace("~1", "/").replace("~0", "~");
    let mut out = Vec::new();
    match tokens.as_slice() {
        [path] => {
            let name = decode(path);
            let Some(block) = block(&doc.text, at, &name) else {
                return out;
            };
            if block.keys.iter().any(|k| k == "$ref") {
                return out;
            }
            for method in METHODS {
                if block.keys.iter().any(|k| k == method) {
                    continue;
                }
                let body = format!("{method}:\n  summary: TODO\n  responses:\n    '200':\n      description: OK");
                out.push(action(doc, format!("Add {method} operation"), &block, &body));
            }
        }
        [_, method, "responses"] if METHODS.contains(method) => {
            let Some(block) = block(&doc.text, at, "responses") else {
                return out;
            };
            for code in RESPONSES {
                if block.keys.iter().any(|k| k == code) {
                    continue;
                }
                out.push(action(doc, format!("Add {code} response"), &block, &format!("'{code}':\n  description: TODO")));
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    const SPEC: &str = "openapi: 3.0.3\ninfo:\n  title: T\n  version: '1'\npaths:\n  /pets/{id}:\n    get:\n      responses:\n        200:\n          description: ok\n    # note\n  /other:\n    $ref: '#/x'\n";
    fn doc(text: &str) -> Document {
        Document { path: "/p/openapi.yaml".into(), language: "yaml".into(), text: text.into(), version: 1 }
    }
    fn acts(text: &str, needle: &str) -> Vec<CodeAction> {
        let caret = position(text, text.find(needle).unwrap());
        actions(&doc(text), Range::new(caret, caret))
    }
    fn apply(text: &str, action: &CodeAction) -> String {
        let edit = &action.edits[0].1[0];
        let at = offset(text, edit.range.start).unwrap();
        format!("{}{}{}", &text[..at], edit.text, &text[at..])
    }
    #[test]
    fn adds_operation_at_end_of_path_item_and_reparses() {
        let found = acts(SPEC, "/pets");
        let titles: Vec<_> = found.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, ["Add post operation", "Add put operation", "Add delete operation"]);
        let updated = apply(SPEC, &found[0]);
        assert!(updated.contains("          description: ok\n    post:\n      summary: TODO"));
        let proof = pointer_status::Proof::parse(&updated).unwrap();
        assert_eq!(proof.status("/paths/~1pets~1{id}/post/responses/200/description"), pointer_status::Status::Present);
        assert_eq!(proof.status("/paths/~1pets~1{id}/post/summary"), pointer_status::Status::Present);
        assert!(spec::parse(&updated).is_some());
    }
    #[test]
    fn adds_response_to_existing_responses_and_skips_int_key() {
        let found = acts(SPEC, "responses");
        let titles: Vec<_> = found.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, ["Add 400 response", "Add 404 response", "Add default response"]);
        let updated = apply(SPEC, &found[2]);
        let proof = pointer_status::Proof::parse(&updated).unwrap();
        assert_eq!(proof.status("/paths/~1pets~1{id}/get/responses/default/description"), pointer_status::Status::Present);
    }
    #[test]
    fn appends_with_newline_at_eof_without_trailing_newline() {
        let text = "openapi: 3.0.3\npaths:\n  /a:\n    get:\n      summary: s";
        let found = acts(text, "/a");
        let updated = apply(text, &found[0]);
        assert!(updated.contains("summary: s\n    post:\n"));
        assert!(pointer_status::Proof::parse(&updated).is_some());
    }
    #[test]
    fn refuses_flow_anchors_ref_multidoc_and_json() {
        assert!(acts(SPEC, "/other").is_empty());
        for text in [
            "openapi: 3.0.3\npaths:\n  /a: {get: {}}\n",
            "openapi: 3.0.3\npaths:\n  /a:\n    get: &g\n      summary: s\n",
            "---\nopenapi: 3.0.3\npaths:\n  /a:\n    get:\n      summary: s\n---\nx: 1\n",
        ] {
            assert!(acts(text, "/a").is_empty(), "{text}");
        }
        let json = r#"{"openapi":"3.0.3","paths":{"/a":{}}}"#;
        assert!(acts(json, "/a").is_empty());
    }
}
