//! JSON resource assistance using the same built-in kind model as YAML. A
//! bounded structural scan keeps physical key/value ranges and duplicate keys.
use openidea_plugin::providers::{CompletionItem, CompletionKind, Document};
use openidea_plugin::{Diagnostic, Position, Range, Severity, text};
use serde_json::Value;
#[derive(Clone)]
struct Entry {
    path: Vec<String>,
    key: String,
    key_start: usize,
    key_end: usize,
    start: usize,
    end: usize,
}
struct Scan<'a> {
    source: &'a str,
    at: usize,
    entries: Vec<Entry>,
    duplicates: Vec<(usize, usize)>,
    nodes: usize,
}
impl Scan<'_> {
    fn space(&mut self) {
        while self
            .source
            .as_bytes()
            .get(self.at)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.at += 1;
        }
    }
    fn string(&mut self) -> Option<(String, usize, usize)> {
        let start = self.at;
        self.at += 1;
        while let Some(&c) = self.source.as_bytes().get(self.at) {
            self.at += 1;
            if c == b'\\' {
                self.at += 1;
            } else if c == b'"' {
                return Some((
                    serde_json::from_str(&self.source[start..self.at]).ok()?,
                    start,
                    self.at,
                ));
            }
        }
        None
    }
    fn value(&mut self, path: &mut Vec<String>, depth: usize) -> Option<()> {
        self.nodes += 1;
        if depth > 32 || self.nodes > 8192 {
            return None;
        }
        self.space();
        match self.source.as_bytes().get(self.at)? {
            b'{' => {
                self.at += 1;
                self.space();
                let mut keys = std::collections::BTreeSet::new();
                if self.source.as_bytes().get(self.at) == Some(&b'}') {
                    self.at += 1;
                    return Some(());
                }
                loop {
                    self.space();
                    let (key, key_start, key_end) = self.string()?;
                    self.space();
                    if self.source.as_bytes().get(self.at) != Some(&b':') {
                        return None;
                    }
                    self.at += 1;
                    self.space();
                    let start = self.at;
                    path.push(key.clone());
                    self.value(path, depth + 1)?;
                    path.pop();
                    if !keys.insert(key.clone()) {
                        self.duplicates.push((key_start, key_end));
                    }
                    self.entries.push(Entry {
                        path: path.clone(),
                        key,
                        key_start,
                        key_end,
                        start,
                        end: self.at,
                    });
                    self.space();
                    match self.source.as_bytes().get(self.at)? {
                        b'}' => {
                            self.at += 1;
                            break;
                        }
                        b',' => self.at += 1,
                        _ => return None,
                    }
                }
            }
            b'[' => {
                self.at += 1;
                self.space();
                if self.source.as_bytes().get(self.at) == Some(&b']') {
                    self.at += 1;
                    return Some(());
                }
                let mut i = 0;
                loop {
                    path.push(i.to_string());
                    self.value(path, depth + 1)?;
                    path.pop();
                    i += 1;
                    self.space();
                    match self.source.as_bytes().get(self.at)? {
                        b']' => {
                            self.at += 1;
                            break;
                        }
                        b',' => self.at += 1,
                        _ => return None,
                    }
                }
            }
            b'"' => {
                self.string()?;
            }
            _ => {
                while self.source.as_bytes().get(self.at).is_some_and(|c| {
                    !matches!(c, b',' | b'}' | b']' | b' ' | b'\t' | b'\r' | b'\n')
                }) {
                    self.at += 1;
                }
            }
        }
        Some(())
    }
}
// Replace JSONC trivia with equal-byte-length spaces. Physical offsets remain
// authoritative in the original source, including Unicode inside comments.
fn normalized_with_comments(
    source: &str,
    trailing_commas: bool,
) -> Option<(String, Vec<std::ops::Range<usize>>)> {
    if source.len() > 256 << 10 {
        return None;
    }
    let original = source.as_bytes();
    let mut out = original.to_vec();
    let mut comments = Vec::new();
    let mut at = 0;
    let mut string = false;
    if out.starts_with(&[0xef, 0xbb, 0xbf]) {
        out[..3].fill(b' ');
    }
    while at < out.len() {
        if string {
            if out[at] == b'\\' {
                at += 2;
                continue;
            }
            if out[at] == b'"' {
                string = false;
            }
            at += 1;
            continue;
        }
        if out[at] == b'"' {
            string = true;
            at += 1;
            continue;
        }
        if out.get(at..at + 2) == Some(b"//") {
            let start = at;
            while at < out.len() && !matches!(out[at], b'\r' | b'\n') {
                out[at] = b' ';
                at += 1;
            }
            comments.push(start..at);
            continue;
        }
        if out.get(at..at + 2) == Some(b"/*") {
            let start = at;
            at += 2;
            while at + 1 < out.len() && out.get(at..at + 2) != Some(b"*/") {
                at += 1;
            }
            if at + 1 >= out.len() {
                return None;
            }
            at += 2;
            for byte in &mut out[start..at] {
                if !matches!(*byte, b'\r' | b'\n') {
                    *byte = b' ';
                }
            }
            comments.push(start..at);
            continue;
        }
        at += 1;
    }
    at = 0;
    string = false;
    while trailing_commas && at < out.len() {
        if string {
            if out[at] == b'\\' {
                at += 2;
                continue;
            }
            if out[at] == b'"' {
                string = false;
            }
            at += 1;
            continue;
        }
        if out[at] == b'"' {
            string = true;
        } else if out[at] == b',' {
            let mut next = at + 1;
            while out.get(next).is_some_and(u8::is_ascii_whitespace) {
                next += 1;
            }
            if matches!(out.get(next), Some(b'}' | b']')) {
                out[at] = b' ';
            }
        }
        at += 1;
    }
    Some((String::from_utf8(out).ok()?, comments))
}
pub(crate) fn normalized(source: &str) -> Option<String> {
    Some(normalized_with_comments(source, true)?.0)
}
pub(crate) fn parsed(source: &str) -> Option<Value> {
    let (value, _, duplicates) = scan(source)?;
    duplicates.is_empty().then_some(value)
}

fn scan(source: &str) -> Option<(Value, Vec<Entry>, Vec<(usize, usize)>)> {
    if source.len() > 256 << 10 {
        return None;
    }
    let normalized = normalized(source)?;
    let parsed: Value = serde_json::from_str(&normalized).ok()?;
    parsed.as_object()?;
    let mut scan = Scan {
        source: &normalized,
        at: 0,
        entries: vec![],
        duplicates: vec![],
        nodes: 0,
    };
    scan.value(&mut vec![], 0)?;
    Some((parsed, scan.entries, scan.duplicates))
}
fn scalar(entry: &Entry, source: &str) -> crate::manifest::Value {
    let raw = &source[entry.start..entry.end];
    crate::manifest::Value {
        text: serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.into()),
        start: entry.start,
        end: entry.end,
    }
}
fn facts(source: &str, value: &Value, entries: &[Entry]) -> Option<crate::manifest::Doc> {
    let api = value["apiVersion"].as_str()?;
    let kind = value["kind"].as_str()?;
    let mut doc = crate::manifest::Doc {
        range: 0..source.len(),
        ..Default::default()
    };
    for e in entries {
        let val = scalar(e, source);
        if e.path.is_empty() {
            doc.top_keys.push(e.key.clone());
            doc.top_entries
                .push((e.key.clone(), e.key_start..e.key_end, Some(val.clone())));
            match e.key.as_str() {
                "apiVersion" => {
                    doc.api_version = Some(crate::manifest::Value {
                        text: api.into(),
                        ..val
                    })
                }
                "kind" => {
                    doc.kind = Some(crate::manifest::Value {
                        text: kind.into(),
                        ..val
                    })
                }
                _ => {}
            }
        } else if e.path == ["spec"] {
            doc.spec_keys.push(crate::manifest::Value {
                text: e.key.clone(),
                start: e.key_start,
                end: e.key_end,
            });
            doc.spec_values.push((e.key.clone(), val));
        } else if e.path.len() == 2 && e.path[0] == "spec" {
            doc.spec_values
                .push((format!("{}.{}", e.path[1], e.key), val));
        } else if e.path == ["metadata"] {
            if e.key == "name" {
                doc.name = Some(val.clone());
            }
            if e.key == "generateName" {
                doc.has_generate_name = true;
            }
            doc.metadata_keys.push((
                e.key.clone(),
                crate::manifest::Value {
                    text: e.key.clone(),
                    start: e.key_start,
                    end: e.key_end,
                },
                Some(val),
            ));
        } else if e.path == ["metadata", "annotations"] {
            doc.annotation_keys.push((
                crate::manifest::Value {
                    text: e.key.clone(),
                    start: e.key_start,
                    end: e.key_end,
                },
                Some(val),
            ));
        }
    }
    doc.has_selector = !value["spec"]["selector"].is_null();
    doc.has_template = !value["spec"]["template"].is_null();
    Some(doc)
}
pub(crate) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    if !matches!(doc.language.as_str(), "json" | "jsonc")
        || crate::config::is_non_kubernetes(&doc.path)
    {
        return vec![];
    }
    let Some((value, entries, duplicates)) = scan(&doc.text) else {
        return vec![];
    };
    if value["apiVersion"].as_str().is_none() || value["kind"].as_str().is_none() {
        return vec![];
    }
    if !duplicates.is_empty() {
        return duplicates
            .into_iter()
            .map(|(a, b)| Diagnostic {
                code: Some("duplicate-key".into()),
                ..Diagnostic::new(
                    Range::new(text::position(&doc.text, a), text::position(&doc.text, b)),
                    Severity::Warning,
                    "Duplicate JSON property makes resource ownership ambiguous",
                )
            })
            .collect();
    }
    let Some(facts) = facts(&doc.text, &value, &entries) else {
        return vec![];
    };
    crate::kinds::check(&facts, &doc.text)
        .into_iter()
        .filter_map(|issue| {
            use crate::kinds::Problem;
            let (code, setting, severity) = match issue.problem {
                Problem::UnknownKey if issue.message.contains("bundled reference") => {
                    ("unknown-key", "checkKindSchema", Severity::Info)
                }
                Problem::UnknownKey => ("unknown-key", "checkKindSchema", Severity::Warning),
                Problem::UnknownKind => ("unknown-kind", "checkKindSchema", Severity::Info),
                Problem::ReadOnly => ("read-only-field", "checkReadOnlyFields", Severity::Info),
                Problem::Deprecated => ("deprecated-key", "checkDeprecatedKeys", Severity::Warning),
                Problem::MissingKey => ("missing-key", "checkRequiredKeys", Severity::Info),
                Problem::InvalidValue => ("invalid-value", "checkFieldValues", Severity::Info),
            };
            if openidea_plugin::host::setting_bool(setting) == Some(false) {
                return None;
            }
            Some(
                crate::checks::Finding {
                    severity,
                    ..crate::checks::Finding::new(issue.start, issue.end, code, issue.message)
                }
                .diagnostic(&doc.text),
            )
        })
        .collect()
}
fn keys(api: &str, kind: &str, path: &[String]) -> Vec<&'static str> {
    if path.is_empty() {
        return crate::kinds::completion_keys(api, kind, false);
    }
    if path == ["spec"] {
        return crate::kinds::completion_keys(api, kind, true);
    }
    if path == ["metadata"] {
        return vec![
            "name",
            "generateName",
            "namespace",
            "labels",
            "annotations",
            "finalizers",
            "ownerReferences",
        ];
    }
    vec![]
}
/// Insert a placeholder at the current key token and parse the resulting JSON
/// before suggesting. This admits an empty key slot without guessing nested
/// object ownership from braces inside strings.
pub(crate) fn completion(
    doc: &Document,
    at: Position,
    schemas: &crate::crd_schema::Cache,
) -> Vec<CompletionItem> {
    if !matches!(doc.language.as_str(), "json" | "jsonc") || doc.text.len() > 256 << 10 {
        return vec![];
    }
    let Some(caret) = text::offset(&doc.text, at) else {
        return vec![];
    };
    let bytes = doc.text.as_bytes();
    let mut start = caret;
    let mut end = caret;
    while start > 0
        && (bytes[start - 1].is_ascii_alphanumeric()
            || matches!(bytes[start - 1], b'"' | b'_' | b'-'))
    {
        start -= 1;
    }
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'"' | b'_' | b'-'))
    {
        end += 1;
    }
    let Some((normalized, comments)) = normalized_with_comments(&doc.text, false) else {
        return vec![];
    };
    if comments.iter().any(|r| r.start <= caret && caret < r.end) {
        return vec![];
    }
    let before = normalized[..start].trim_end();
    if before.ends_with(':') {
        let candidate = format!(
            "{}\"__kube_value__\"{}",
            &doc.text[..start],
            &doc.text[end..]
        );
        let Some((value, entries, duplicates)) = scan(&candidate) else {
            return vec![];
        };
        if !duplicates.is_empty() {
            return vec![];
        }
        let Some(entry) = entries
            .iter()
            .find(|e| candidate[e.start..e.end] == *"\"__kube_value__\"")
        else {
            return vec![];
        };
        let mut schema = schemas.schema(&Document {
            text: candidate.clone(),
            ..doc.clone()
        });
        for key in entry.path.iter().chain(std::iter::once(&entry.key)) {
            schema = schema.map(|s| s["properties"][key.as_str()].clone());
        }
        let choices = if let Some(schema) = schema {
            schema["enum"]
                .as_vec()
                .map(|v| {
                    v.iter()
                        .filter_map(|v| match v {
                            yaml_rust2::Yaml::String(s) => Some(Value::String(s.clone())),
                            yaml_rust2::Yaml::Boolean(b) => Some(Value::Bool(*b)),
                            yaml_rust2::Yaml::Integer(n) => Some(Value::Number((*n).into())),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| {
                    if schema["type"].as_str() == Some("boolean") {
                        vec![Value::Bool(true), Value::Bool(false)]
                    } else {
                        vec![]
                    }
                })
        } else {
            let kind = if value["apiVersion"].as_str() == Some("v1") {
                value["kind"].as_str().unwrap_or("")
            } else {
                ""
            };
            let path = entry.path.iter().map(String::as_str).collect::<Vec<_>>();
            let choices: &[&str] = match (kind, path.as_slice(), entry.key.as_str()) {
                ("Pod", ["spec"], "restartPolicy") => &["Always", "OnFailure", "Never"],
                ("Pod", ["spec"], "dnsPolicy") => {
                    &["ClusterFirstWithHostNet", "ClusterFirst", "Default", "None"]
                }
                ("Service", ["spec"], "type") => {
                    &["ClusterIP", "NodePort", "LoadBalancer", "ExternalName"]
                }
                _ => &[],
            };
            choices.iter().map(|s| Value::String((*s).into())).collect()
        };
        let replace = Range::new(
            text::position(&doc.text, start),
            text::position(&doc.text, end),
        );
        return choices
            .into_iter()
            .filter_map(|v| {
                let insert = serde_json::to_string(&v).ok()?;
                let label = v
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| insert.clone());
                let mut item = CompletionItem::new(&label, CompletionKind::EnumMember);
                item.insert_text = insert;
                item.replace = Some(replace);
                Some(item)
            })
            .collect();
    }
    if !before.ends_with(['{', ',']) {
        return vec![];
    }
    let has_colon = doc.text[end..].trim_start().starts_with(':');
    let token = if has_colon {
        "\"__kube_completion__\""
    } else {
        "\"__kube_completion__\":null"
    };
    let candidate = format!("{}{token}{}", &doc.text[..start], &doc.text[end..]);
    let Some((value, entries, duplicates)) = scan(&candidate) else {
        return vec![];
    };
    if !duplicates.is_empty() {
        return vec![];
    }
    let Some(api) = value["apiVersion"].as_str() else {
        return vec![];
    };
    let Some(kind) = value["kind"].as_str() else {
        return vec![];
    };
    let Some(entry) = entries.iter().find(|e| e.key == "__kube_completion__") else {
        return vec![];
    };
    let mut parent = &value;
    for key in &entry.path {
        parent = &parent[key];
    }
    let replace = Range::new(
        text::position(&doc.text, start),
        text::position(&doc.text, end),
    );
    let mut schema = schemas.schema(&Document {
        text: candidate.clone(),
        ..doc.clone()
    });
    for path in &entry.path {
        schema = schema.map(|s| s["properties"][path.as_str()].clone());
    }
    let choices = schema
        .and_then(|s| {
            s["properties"].as_hash().map(|p| {
                p.keys()
                    .filter_map(|k| k.as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
            })
        })
        .unwrap_or_else(|| {
            keys(api, kind, &entry.path)
                .into_iter()
                .map(str::to_owned)
                .collect()
        });
    choices
        .into_iter()
        .filter(|key| parent.get(key.as_str()).is_none())
        .map(|key| {
            let mut item = CompletionItem::new(&key, CompletionKind::Property);
            item.insert_text = if has_colon {
                format!("\"{key}\"")
            } else {
                format!("\"{key}\": $0")
            };
            item.snippet = !has_colon;
            item.replace = Some(replace);
            item.detail = Some(format!("Kubernetes {kind} field"));
            item
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn doc(source: &str) -> Document {
        Document {
            path: "/p/resource.json".into(),
            language: "json".into(),
            version: 1,
            text: source.into(),
        }
    }
    fn complete(marked: &str) -> Vec<CompletionItem> {
        let at = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        completion(
            &doc(&source),
            text::position(&source, at),
            &crate::crd_schema::Cache::default(),
        )
    }
    #[test]
    fn jsonc_comments_and_trailing_commas_preserve_source_ranges_and_cursor_ownership() {
        let _h = MockHost::install();
        let mut d =
            doc("{ /* 🐳 */ \"apiVersion\":\"v1\",\"kind\":\"Pod\",\"spec\":{\"typo\":3,},} ");
        d.language = "jsonc".into();
        let found = diagnostics(&d);
        let issue = found.iter().find(|i| i.message.contains("typo")).unwrap();
        let a = text::offset(&d.text, issue.range.start).unwrap();
        let b = text::offset(&d.text, issue.range.end).unwrap();
        assert_eq!(&d.text[a..b], "\"typo\"");
        assert!(
            complete(r#"{"apiVersion":"v1","kind":"Pod",/* 🐳 */ |}"#)
                .iter()
                .any(|i| i.label == "spec")
        );
        assert!(complete(r#"{"apiVersion":"v1","kind":"Pod",/* | */}"#).is_empty());
        assert!(normalized("{/* unfinished").is_none());
        assert_eq!(
            parsed(r#"{"url":"https://example.org/a/*path*/",}"#).unwrap()["url"],
            "https://example.org/a/*path*/"
        );
    }

    #[test]
    fn builtin_json_enums_and_unsupported_jsonc_are_quiet_or_typed() {
        let _h = MockHost::install();
        let found = complete(r#"{"apiVersion":"v1","kind":"Pod","spec":{"restartPolicy":"|"}}"#);
        assert_eq!(
            found.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            ["Always", "OnFailure", "Never"]
        );
        let mut d = doc("{ // JSONC comment\n \"apiVersion\":\"v1\",\"kind\":\"Pod\",} ");
        d.language = "jsonc".into();
        assert!(diagnostics(&d).is_empty());
    }

    #[test]
    fn builtin_json_keys_complete_with_structural_context_and_no_duplicate_colons() {
        let _h = MockHost::install();
        let root = complete(r#"{"apiVersion":"v1","kind":"Pod", |}"#);
        assert_eq!(
            root.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            ["metadata", "spec"]
        );
        assert!(
            complete(r#"{"apiVersion":"v1","kind":"Pod","spec":{ |}}"#)
                .iter()
                .any(|i| i.label == "containers")
        );
        let existing = complete(r#"{"apiVersion":"v1","kind":"Pod","spec":{"cont|ainers":[]}}"#);
        let item = existing.iter().find(|i| i.label == "containers").unwrap();
        assert_eq!(item.insert_text, "\"containers\"");
        assert!(!item.snippet);
        assert!(complete(r#"{"apiVersion":"v1","kind":"Pod","metadata":{"name":"|"}}"#).is_empty());
        assert!(complete(r#"{"apiVersion":"custom/v1","kind":"Custom", |}"#).is_empty());
    }
    #[test]
    fn json_diagnostics_reuse_builtin_model_and_physical_utf16_ranges() {
        let _h = MockHost::install();
        let d = doc(
            r#"{"apiVersion":"apps/v1","kind":"Deployment","metadata":{"name":"🐳","uid":"server"},"spec":{"replicas":-1,"typo":3}}"#,
        );
        let found = diagnostics(&d);
        assert!(
            found
                .iter()
                .any(|i| i.code.as_deref() == Some("read-only-field"))
        );
        assert!(found.iter().any(|i| i.message.contains("typo")));
        let issue = found.iter().find(|i| i.message.contains("typo")).unwrap();
        let a = text::offset(&d.text, issue.range.start).unwrap();
        let b = text::offset(&d.text, issue.range.end).unwrap();
        assert_eq!(&d.text[a..b], "\"typo\"");
    }
    #[test]
    fn duplicate_json_keys_are_diagnosed_without_guessing_last_key_ownership() {
        let _h = MockHost::install();
        let d = doc(r#"{"apiVersion":"v1","kind":"Pod","spec":{},"spec":{}}"#);
        let found = diagnostics(&d);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code.as_deref(), Some("duplicate-key"));
        assert!(diagnostics(&doc(r#"{"package":"demo","package":"other"}"#)).is_empty());
        assert!(diagnostics(&doc(r#"{"apiVersion":"v1","kind":"Pod","spec": "#)).is_empty());
    }
    #[test]
    fn json_provider_does_not_offer_yaml_modeline_actions() {
        let _h = MockHost::install();
        let mut kube = crate::KubeExplorer::default();
        let d = doc(r#"{"apiVersion":"v1","kind":"Pod"}"#);
        assert!(
            openidea_plugin::providers::Providers::code_actions(
                &mut kube,
                &d,
                Range::new(Position::new(0, 0), Position::new(0, 0))
            )
            .is_empty()
        );
    }
}
