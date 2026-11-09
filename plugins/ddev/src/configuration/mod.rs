//! Offline schema-backed assistance for project and global DDEV YAML.
mod schema;
#[cfg(test)]
mod tests;
mod yaml;

use crate::Main;
use openidea_plugin::diagnostics::DiagnosticsProvider;
use openidea_plugin::hover::{Hover, HoverProvider};
use openidea_plugin::providers::{CompletionItem, CompletionKind, Document, Providers};
use openidea_plugin::text::{offset, position};
use openidea_plugin::{Diagnostic, Position, Range, Severity};
use serde_json::Value;
use yaml::Node;
const MARKER: &str = "__openidea_ddev_completion_7e4f__";

pub fn initialize() {
    let _ = schema::project();
    let _ = schema::global();
}
fn schema_for(path: &str) -> Option<&'static schema::Schema> {
    let windows = path.starts_with("\\\\")
        || path.as_bytes().get(1) == Some(&b':')
            && path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic);
    let path = path.replace('\\', "/");
    let path = if windows {
        path.to_ascii_lowercase()
    } else {
        path
    };
    let (dir, file) = path.rsplit_once('/')?;
    if dir != ".ddev" && !dir.ends_with("/.ddev") {
        return None;
    }
    if file == "config.yaml" || file.starts_with("config.") && file.ends_with(".yaml") {
        Some(schema::project())
    } else if file == "global_config.yaml"
        || file.starts_with("global_config.") && file.ends_with(".yaml")
    {
        Some(schema::global())
    } else {
        None
    }
}
fn span(text: &str, start: usize, end: usize) -> Range {
    let start = start.min(text.len());
    let mut end = end.min(text.len()).max(start);
    if end == start && start < text.len() {
        end += text[start..].chars().next().unwrap().len_utf8();
    }
    Range::new(position(text, start), position(text, end))
}
fn pointer(path: &str) -> Vec<String> {
    path.strip_prefix('/').map_or(vec![], |s| {
        s.split('/')
            .map(|s| s.replace("~1", "/").replace("~0", "~"))
            .collect()
    })
}

impl DiagnosticsProvider for Main {
    fn diagnostics(&mut self, doc: &Document) -> Vec<Diagnostic> {
        let Some(schema) = schema_for(&doc.path) else {
            return vec![];
        };
        if doc.text.len() > yaml::MAX_BYTES {
            return vec![
                Diagnostic::new(
                    Range::default(),
                    Severity::Info,
                    "DDEV configuration assistance is limited to files up to 256 KiB.",
                )
                .code("ddev-config-size"),
            ];
        }
        let tree = match yaml::parse(&doc.text) {
            Ok(t) => t,
            Err(e) => {
                return vec![
                    Diagnostic::new(span(&doc.text, e.start, e.end), Severity::Error, e.message)
                        .code("ddev-yaml"),
                ];
            }
        };
        let mut out: Vec<_> = tree
            .duplicates
            .iter()
            .take(100)
            .map(|e| {
                Diagnostic::new(span(&doc.text, e.start, e.end), Severity::Error, &e.message)
                    .code("ddev-duplicate-key")
            })
            .collect();
        if out.len() >= 100 {
            return out;
        }
        for error in schema.validator.iter_errors(&tree.root.value).take(100) {
            let path = pointer(&error.instance_path().to_string());
            let node = tree.root.at(&path).unwrap_or(&tree.root);
            if let jsonschema::error::ValidationErrorKind::AdditionalProperties { unexpected } =
                error.kind()
            {
                for key in unexpected.iter().take(100_usize.saturating_sub(out.len())) {
                    let (start, end) = node
                        .keys
                        .get(key)
                        .copied()
                        .unwrap_or((node.start, node.end));
                    out.push(
                        Diagnostic::new(
                            span(&doc.text, start, end),
                            Severity::Warning,
                            format!(
                                "Unknown DDEV configuration key '{key}' ({} schema).",
                                schema::VERSION
                            ),
                        )
                        .code("ddev-unknown-property"),
                    );
                }
            } else {
                let message: String = error.to_string().chars().take(600).collect();
                out.push(
                    Diagnostic::new(
                        span(&doc.text, node.start, node.end),
                        Severity::Warning,
                        format!("{} schema: {message}", schema::VERSION),
                    )
                    .code("ddev-schema"),
                );
            }
            if out.len() >= 100 {
                break;
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
struct Edit {
    start: usize,
    end: usize,
    quoted: Option<char>,
    value_only: bool,
    colon: bool,
}
/// Find the editable token without interpreting comment or quoted punctuation
/// as a mapping delimiter. The full YAML parser decides its actual schema path.
fn edit(text: &str, cursor: usize) -> Option<Edit> {
    let start = text[..cursor].rfind('\n').map_or(0, |n| n + 1);
    let end = text[cursor..].find('\n').map_or(text.len(), |n| cursor + n);
    let line = &text[start..end];
    let at = cursor - start;
    let mut boundary = 0;
    let mut last = None;
    let mut quote = None;
    let mut quote_start = 0;
    let mut escape = false;
    let mut comment = None;
    let mapping_colon = |pos: usize| {
        line[pos + 1..]
            .chars()
            .next()
            .is_none_or(|n| n.is_whitespace() || "[]{},'\"".contains(n))
            || line[..pos].ends_with(['\"', '\''])
    };
    let chars: Vec<_> = line.char_indices().collect();
    let mut i = 0;
    let mut quoted_token = None;
    while i < chars.len() {
        let (pos, c) = chars[i];
        if let Some(q) = quote {
            if escape {
                escape = false;
                i += 1;
                continue;
            }
            if c == '\\' && q == '"' {
                escape = true;
                i += 1;
                continue;
            }
            if c == q {
                if q == '\'' && chars.get(i + 1).is_some_and(|(_, c)| *c == q) {
                    i += 2;
                    continue;
                }
                if quote_start <= at && at <= pos + 1 {
                    quoted_token = Some((quote_start, pos + 1, q));
                }
                quote = None;
            }
            i += 1;
            continue;
        }
        if c == '#' && (pos == 0 || line[..pos].chars().last().is_some_and(char::is_whitespace)) {
            comment = Some(pos);
            break;
        }
        if c == '\'' || c == '"' {
            quote = Some(c);
            quote_start = pos;
            i += 1;
            continue;
        }
        let colon = c == ':' && mapping_colon(pos);
        if pos < at && (colon || "[{,".contains(c)) {
            boundary = pos + 1;
            last = Some(c);
        }
        i += 1;
    }
    if comment.is_some_and(|p| at >= p) {
        return None;
    }
    if let Some(q) = quote
        && quote_start <= at
    {
        quoted_token = Some((quote_start, comment.unwrap_or(line.len()), q));
    }
    let limit = comment.unwrap_or(line.len());
    let (token_start, token_end, quoted) = if let Some((a, b, q)) = quoted_token {
        (a, b, Some(q))
    } else {
        let mut a = boundary;
        while a < at && line[a..].starts_with(char::is_whitespace) {
            a += line[a..].chars().next()?.len_utf8();
        }
        if boundary == 0
            && line[a..].starts_with('-')
            && line[a + 1..].chars().next().is_none_or(char::is_whitespace)
        {
            a += 1;
            while a < at && line[a..].starts_with(char::is_whitespace) {
                a += line[a..].chars().next()?.len_utf8();
            }
        }
        let mut b = limit;
        for (pos, c) in line[a..limit].char_indices() {
            let pos = a + pos;
            if ",]}".contains(c) || c == ':' && mapping_colon(pos) {
                b = pos;
                break;
            }
        }
        b = a + line[a..b].trim_end().len();
        if a > at || b < at {
            return None;
        }
        (a, b, None)
    };
    let colon = line[token_end..limit].trim_start().starts_with(':');
    Some(Edit {
        start: start + token_start,
        end: start + token_end,
        quoted,
        value_only: last == Some(':'),
        colon,
    })
}
fn completion_tree(text: &str) -> Result<yaml::Tree, yaml::Issue> {
    match yaml::parse(text) {
        Ok(tree) => Ok(tree),
        Err(original) => {
            let mut closers = vec![];
            let mut quote = None;
            let mut escape = false;
            let mut comment = false;
            for c in text.chars() {
                if comment {
                    if c == '\n' {
                        comment = false;
                    } else {
                        continue;
                    }
                }
                if let Some(q) = quote {
                    if escape {
                        escape = false;
                        continue;
                    }
                    if c == '\\' && q == '"' {
                        escape = true;
                        continue;
                    }
                    if c == q {
                        quote = None;
                    }
                    continue;
                }
                match c {
                    '#' => comment = true,
                    '\'' | '"' => quote = Some(c),
                    '{' => closers.push('}'),
                    '[' => closers.push(']'),
                    '}' | ']' if closers.pop() != Some(c) => return Err(original),
                    _ => {}
                }
            }
            if closers.is_empty() || quote.is_some() {
                return Err(original);
            }
            let mut repaired = text.to_owned();
            repaired.push('\n');
            repaired.extend(closers.into_iter().rev());
            yaml::parse(&repaired).map_err(|_| original)
        }
    }
}
fn marker_path(node: &Node, path: &mut Vec<String>) -> Option<(Vec<String>, bool)> {
    if node.keys.contains_key(MARKER) {
        return Some((path.clone(), true));
    }
    if node.value.as_str() == Some(MARKER) {
        return Some((path.clone(), false));
    }
    for (name, child) in &node.fields {
        path.push(name.clone());
        let found = marker_path(child, path);
        path.pop();
        if found.is_some() {
            return found;
        }
    }
    for (i, child) in node.items.iter().enumerate() {
        path.push(i.to_string());
        let found = marker_path(child, path);
        path.pop();
        if found.is_some() {
            return found;
        }
    }
    None
}
fn insert_value(value: &Value, quote: Option<char>) -> String {
    let Some(text) = value.as_str() else {
        return value.to_string();
    };
    if quote == Some('\'') {
        return format!("'{}'", text.replace('\'', "''"));
    }
    if quote == Some('"')
        || !matches!(
            yaml_rust2::Yaml::from_str(text),
            yaml_rust2::Yaml::String(_)
        )
        || text.contains(['#', ':', '\n', ',', '[', ']', '{', '}'])
    {
        return serde_json::to_string(text).unwrap();
    }
    text.to_owned()
}
impl Providers for Main {
    fn completion(
        &mut self,
        doc: &Document,
        at: Position,
        _trigger: Option<&str>,
    ) -> Vec<CompletionItem> {
        let Some(schema) = schema_for(&doc.path) else {
            return vec![];
        };
        if doc.text.len() > yaml::MAX_BYTES || doc.text.contains(MARKER) {
            return vec![];
        }
        let Some(cursor) = offset(&doc.text, at) else {
            return vec![];
        };
        let Some(edit) = edit(&doc.text, cursor) else {
            return vec![];
        };
        let prefix = &doc.text[edit.start..cursor];
        let prefix = if edit.quoted.is_some() {
            prefix.get(1..).unwrap_or("")
        } else {
            prefix
        };
        let needs_space = edit.value_only && doc.text[..edit.start].ends_with(':');
        for key_attempt in [true, false] {
            if key_attempt && edit.value_only {
                continue;
            }
            let replacement = if key_attempt && !edit.colon {
                format!("{MARKER}: null")
            } else if needs_space {
                format!(" {MARKER}")
            } else {
                MARKER.into()
            };
            let mut patched = doc.text.clone();
            patched.replace_range(edit.start..edit.end, &replacement);
            let Ok(tree) = completion_tree(&patched) else {
                continue;
            };
            let Some((path, key)) = marker_path(&tree.root, &mut vec![]) else {
                continue;
            };
            let schemas = schema.at(&path, &tree.root.value);
            let mut items = vec![];
            if key {
                let props = schema.properties(&schemas);
                if props.is_empty() {
                    continue;
                }
                let parent = tree.root.at(&path).unwrap();
                for (name, variants) in props {
                    if !name.starts_with(prefix) || parent.fields.contains_key(name) {
                        continue;
                    }
                    let mut item = CompletionItem::new(name, CompletionKind::Property);
                    item.insert_text = if edit.colon {
                        name.into()
                    } else {
                        format!("{name}: ")
                    };
                    item.documentation =
                        Some(schema.documentation(name, &schema.variants(&variants, &Value::Null)));
                    item.detail = variants
                        .iter()
                        .find_map(|v| v.get("description").and_then(Value::as_str))
                        .map(str::to_owned);
                    item.replace = Some(Range::new(
                        position(&doc.text, edit.start),
                        position(&doc.text, edit.end),
                    ));
                    items.push(item);
                }
            } else {
                for value in schema.values(&schemas) {
                    let label = value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string());
                    if !label.starts_with(prefix) {
                        continue;
                    }
                    // Empty strings are valid enum values but need a visible
                    // label in the completion popup.
                    let label = if label.is_empty() {
                        "\"\"".into()
                    } else {
                        label
                    };
                    let mut item = CompletionItem::new(&label, CompletionKind::Value);
                    item.insert_text = format!(
                        "{}{}",
                        if needs_space { " " } else { "" },
                        insert_value(&value, edit.quoted)
                    );
                    item.documentation =
                        Some(schema.documentation(path.last().map_or("value", |s| s), &schemas));
                    item.replace = Some(if edit.start == edit.end {
                        Range::new(at, at)
                    } else {
                        span(&doc.text, edit.start, edit.end)
                    });
                    items.push(item);
                }
            }
            return items;
        }
        vec![]
    }
}

fn hover_path(
    node: &Node,
    at: usize,
    path: &mut Vec<String>,
) -> Option<(Vec<String>, usize, usize)> {
    for (key, (start, end)) in &node.keys {
        if *start <= at && at < *end {
            let mut p = path.clone();
            p.push(key.clone());
            return Some((p, *start, *end));
        }
    }
    for (key, child) in &node.fields {
        path.push(key.clone());
        if let Some(found) = hover_path(child, at, path) {
            return Some(found);
        }
        path.pop();
    }
    for (i, child) in node.items.iter().enumerate() {
        path.push(i.to_string());
        if let Some(found) = hover_path(child, at, path) {
            return Some(found);
        }
        path.pop();
    }
    (node.start <= at && at < node.end && !node.value.is_object() && !node.value.is_array())
        .then(|| (path.clone(), node.start, node.end))
}
impl HoverProvider for Main {
    fn hover(&mut self, doc: &Document, at: Position) -> Option<Hover> {
        let schema = schema_for(&doc.path)?;
        let cursor = offset(&doc.text, at)?;
        let tree = yaml::parse(&doc.text).ok()?;
        let (path, start, end) = hover_path(&tree.root, cursor, &mut vec![])?;
        let schemas = schema.at(&path, &tree.root.value);
        if schemas.is_empty() {
            return None;
        }
        Some(
            Hover::new(schema.documentation(path.last()?, &schemas))
                .range(span(&doc.text, start, end)),
        )
    }
}
