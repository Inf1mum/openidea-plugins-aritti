//! Source-context "Add operation/response/path/parameter" for YAML block style
//! (JSON lives in json_actions).
//! Inserts a minimal skeleton at the exact end of an existing block mapping;
//! refuses flow style, anchors/aliases, multi-document and tab indentation.
use crate::*;

pub(crate) const METHODS: [&str; 4] = ["get", "post", "put", "delete"];
pub(crate) const RESPONSES: [&str; 4] = ["200", "400", "404", "default"];

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

pub(crate) enum Body {
    Str(&'static str),
    Object(Vec<(&'static str, Body)>),
    Array(Vec<Body>),
}

pub(crate) fn operation_skeleton() -> Body {
    Body::Object(vec![
        ("summary", Body::Str("TODO")),
        ("responses", Body::Object(vec![("200", Body::Object(vec![("description", Body::Str("OK"))]))])),
    ])
}

pub(crate) fn parameter_skeleton() -> Body {
    Body::Object(vec![
        ("name", Body::Str("TODO")),
        ("in", Body::Str("query")),
        ("schema", Body::Object(vec![("type", Body::Str("string"))])),
    ])
}

pub(crate) fn render(body: &Body, indent: &str, unit: &str, pretty: bool) -> String {
    let inner = format!("{indent}{unit}");
    let (open, close, items): (&str, &str, Vec<String>) = match body {
        Body::Str(s) => return format!("{s:?}"),
        Body::Object(m) => ("{", "}", m.iter().map(|(k, v)| format!("{k:?}: {}", render(v, &inner, unit, pretty))).collect()),
        Body::Array(a) => ("[", "]", a.iter().map(|v| render(v, &inner, unit, pretty)).collect()),
    };
    if pretty {
        let lines: Vec<String> = items.iter().map(|i| format!("{inner}{i}")).collect();
        return format!("{open}\n{}\n{indent}{close}", lines.join(",\n"));
    }
    format!("{open}{}{close}", items.join(", "))
}

fn path_tokens(pointer: &str) -> Option<Vec<String>> {
    if pointer == "#/paths" {
        return Some(Vec::new());
    }
    let decoded = pointer.strip_prefix("#/paths/").and_then(percent_decode)?;
    Some(decoded.split('/').map(|t| t.replace("~1", "/").replace("~0", "~")).collect())
}

fn list_block(text: &str, header_start: usize) -> Option<Block> {
    let mut lines = text[header_start..].split_inclusive('\n');
    let header = lines.next()?;
    let header_line = header.trim_end_matches(['\n', '\r']);
    if header_line.trim_start() != "parameters:" {
        return None;
    }
    let indent = header_line.len() - header_line.trim_start().len();
    let mut pos = header_start + header.len();
    let mut found: Option<Block> = None;
    for raw in lines {
        let line = raw.trim_end_matches(['\n', '\r']);
        let trimmed = line.trim_start();
        let end = pos + raw.len();
        pos = end;
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let here = line.len() - trimmed.len();
        let dash = trimmed.starts_with("- ") || trimmed == "-";
        if here < indent || (here == indent && !dash) {
            break;
        }
        let item_indent = found.as_ref().map_or(here, |b| b.indent);
        if !dash && here == item_indent || here < item_indent {
            return None;
        }
        let block = found.get_or_insert(Block { indent: here, insert_at: end, keys: Vec::new(), needs_newline: false });
        block.insert_at = end;
        block.needs_newline = !raw.ends_with('\n');
    }
    found
}

fn parameter_action(doc: &Document, at: usize, name: &str) -> Option<CodeAction> {
    let text = &doc.text;
    let item = block(text, at, name)?;
    if !item.keys.iter().any(|k| k == "parameters") {
        return Some(action(doc, "Add query parameter".into(), &item, "parameters:\n  - name: TODO\n    in: query\n    schema:\n      type: string"));
    }
    let header_line = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let mut pos = header_line + text[header_line..].split_inclusive('\n').next()?.len();
    for raw in text[pos..].split_inclusive('\n') {
        let line = raw.trim_end_matches(['\n', '\r']);
        if line.len() - line.trim_start().len() == item.indent && line.trim_end() == format!("{}parameters:", " ".repeat(item.indent)) {
            let list = list_block(text, pos)?;
            return Some(action(doc, "Add query parameter".into(), &list, "- name: TODO\n  in: query\n  schema:\n    type: string"));
        }
        pos += raw.len();
    }
    None
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
    let json = spec::is_json(&doc.text);
    if !json && !plain_document(&doc.text) {
        return Vec::new();
    }
    let Some(at) = offset(&doc.text, selection.start) else {
        return Vec::new();
    };
    let Some(proof) = pointer_status::Proof::parse(&doc.text) else {
        return Vec::new();
    };
    let Some(tokens) = proof.copy_pointer_at(at).as_deref().and_then(path_tokens) else {
        return Vec::new();
    };
    let tokens: Vec<&str> = tokens.iter().map(String::as_str).collect();
    if json {
        return json_actions::actions(doc, &tokens);
    }
    let mut out = Vec::new();
    match tokens.as_slice() {
        [] => {
            let Some(block) = block(&doc.text, at, "paths") else {
                return out;
            };
            let name = (1..)
                .map(|n| if n == 1 { "/new-path".to_string() } else { format!("/new-path-{n}") })
                .find(|c| !block.keys.contains(c))
                .unwrap_or_default();
            let body = format!("{name}:\n  get:\n    summary: TODO\n    responses:\n      '200':\n        description: OK");
            out.push(action(doc, "Add path".into(), &block, &body));
        }
        [name] => {
            let Some(block) = block(&doc.text, at, name) else {
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
            out.extend(parameter_action(doc, at, name));
        }
        [_, method] if METHODS.contains(method) => out.extend(parameter_action(doc, at, method)),
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
        assert_eq!(titles, ["Add post operation", "Add put operation", "Add delete operation", "Add query parameter"]);
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
        let json = "{\"openapi\":\"3.0.3\",\"paths\":{\"/a\":{}} // c\n}";
        assert!(acts(json, "/a").is_empty());
    }

    const PARAMS: &str = "openapi: 3.0.3\npaths:\n  /a:\n    get:\n      parameters:\n      - name: x\n        in: query\n      responses:\n        '200':\n          description: ok\n  /b:\n    parameters: []\n    get:\n      summary: s\n";

    #[test]
    fn adds_path_with_unique_name_and_reparses() {
        let found = acts(SPEC, "paths:");
        assert_eq!(found.iter().map(|a| a.title.as_str()).collect::<Vec<_>>(), ["Add path"]);
        let once = apply(SPEC, &found[0]);
        assert!(pointer_status::Proof::parse(&once).unwrap().status("/paths/~1new-path/get/responses/200/description") == pointer_status::Status::Present);
        let twice = apply(&once, &acts(&once, "paths:")[0]);
        assert!(pointer_status::Proof::parse(&twice).unwrap().status("/paths/~1new-path-2/get/summary") == pointer_status::Status::Present);
        assert!(spec::parse(&twice).is_some());
    }

    #[test]
    fn adds_parameter_creating_or_merging_block_lists_only() {
        let created = apply(SPEC, acts(SPEC, "/pets").iter().find(|a| a.title == "Add query parameter").unwrap());
        assert!(pointer_status::Proof::parse(&created).is_some(), "{created}");
        assert!(created.contains("    parameters:\n      - name: TODO"), "{created}");
        assert!(spec::parse(&created).is_some());
        let op = acts(PARAMS, "get:");
        let merged = apply(PARAMS, &op[0]);
        assert!(merged.contains("        in: query\n      - name: TODO\n        in: query\n        schema:\n          type: string\n      responses:"), "{merged}");
        let proof = pointer_status::Proof::parse(&merged).unwrap();
        let docs = yaml_rust2::YamlLoader::load_from_str(&merged).unwrap();
        let params = &docs[0]["paths"]["/a"]["get"]["parameters"];
        assert_eq!((params.as_vec().unwrap().len(), params[1]["name"].as_str(), params[1]["schema"]["type"].as_str()), (2, Some("TODO"), Some("string")));
        assert!(proof.status("/paths/~1a/get/responses/200/description") == pointer_status::Status::Present);
        assert!(acts(PARAMS, "/b").iter().all(|a| a.title != "Add query parameter"));
    }
}
