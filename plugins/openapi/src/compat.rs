//! A breaking-change report between an old and a new spec, in the spirit of openapi-diff and
//! oasdiff. It is NOT oasdiff parity; the rules below are the complete list.
//!
//! Operations and paths: path removed (breaking), operation removed (breaking), path or operation
//! added (non-breaking), operation newly deprecated (info).
//! Parameters, keyed by (in, name): required parameter removed (breaking), optional parameter
//! removed (info), one removed and one added with the same location and schema (renamed, breaking),
//! required parameter added (breaking), optional parameter added (non-breaking), made required
//! (breaking), made optional (non-breaking), schema compared as a request.
//! Request body: required body added (breaking), made required (breaking), made optional
//! (non-breaking), body removed (info), media type removed (breaking), schema compared as a request.
//! Responses: 2xx/3xx/default code removed (breaking), 4xx/5xx code removed (info), code added
//! (non-breaking), media type removed (breaking), schema compared as a response.
//! Schemas (`properties`, `items`, `allOf` flattened): type or format
//! changed (breaking); enum value removed (request breaking, response info); enum value added
//! (response breaking, request non-breaking); property removed (response breaking, request info);
//! property added (request required: breaking, otherwise non-breaking); property made required
//! (request breaking, response non-breaking); property no longer required (response breaking,
//! request non-breaking).
//! Schema keywords, per the oasdiff checker rules (github.com/oasdiff/oasdiff, `checker/rules.go`
//! and `docs/NULLABILITY.md`, severities as registered there; oasdiff WARN, "effect unknown", is
//! reported here as info):
//! - `minimum`, `exclusiveMinimum`, `minLength`, `minItems` (and the `max*` mirrors): in a request
//!   setting or tightening is breaking, loosening non-breaking (oasdiff `request-property-max-set`,
//!   `-max-decreased`, `-min-increased`, `-max-length-set`, `-min-items-set` ... are ERR, the
//!   increases/decreases that widen are INFO); in a response loosening is breaking (`response-property-
//!   max-increased`, `-min-decreased`, `-max-length-increased`, `-max-length-unset`, `-min-items-
//!   unset`, `-min-length-decreased` are ERR), tightening non-breaking. `minLength`/`minItems` default
//!   to 0, `minimum` and `maximum` have no default, and only numeric exclusive bounds (3.1) are read.
//!   Unsetting `maximum`/`maxItems` in a response has no oasdiff rule and is not reported.
//! - `pattern`: added in a request (ERR) or removed in a response (ERR) is breaking; removed in a
//!   request (INFO) or added in a response (INFO) non-breaking; changed is info (oasdiff WARN).
//! - `uniqueItems`: set in a request (ERR) or unset in a response (ERR) breaking; the reverse is
//!   non-breaking.
//! - `multipleOf`: set in a request / unset in a response breaking; unset in a request, or the old
//!   value being a multiple of the new one (generalized), non-breaking in a request; a change that
//!   is not a generalization is breaking in both directions (oasdiff `-multiple-of-changed`, ERR);
//!   a response specialized (new a multiple of old) is non-breaking.
//! - `nullable` (3.0), `type: [x, "null"]` (3.1): request became nullable non-breaking, became not
//!   nullable breaking; response became nullable breaking, became not nullable info (NULLABILITY.md
//!   table). The `oneOf`-with-null form is not recognised.
//! - `type` lists: widened (request non-breaking, response breaking), narrowed (request breaking,
//!   response info), as `*-list-of-types-widened`/`-narrowed`; `"null"` is ignored here.
//! - `oneOf`/`anyOf` members (matched by `$ref`, else by identical JSON; only when both sides have
//!   the keyword): member removed breaking in a request and info in a response, member added
//!   non-breaking in a request and breaking in a response (`request-property-one-of-removed` ERR,
//!   `response-property-one-of-added` ERR, and the `any-of` twins). Members are not diffed deeply.
//! - `readOnly`/`writeOnly` on a property: request readOnly set is info (WARN), unset is breaking
//!   when the property is required and non-breaking otherwise (`request-required-property-became-
//!   non-read-only` ERR); response writeOnly set on a required property is breaking (ERR) and info
//!   otherwise, writeOnly unset and the readOnly changes are info. A request constraint change on a
//!   `readOnly` schema and a response one on a `writeOnly` schema is info (oasdiff BREAKING-CHANGES.md,
//!   "the spec shows the change cannot break anyone").
//! - `deprecated` added to or removed from a schema: info (`request-property-deprecated` INFO).
//! - Security, on the effective requirements of an operation (its own `security`, else the root one):
//!   a scheme removed is breaking (`api-security-removed` ERR), a scope added is breaking
//!   (`api-security-scope-added` ERR), anonymous access (`{}` or no security) removed is breaking
//!   (`api-security-anonymous-access-removed` ERR); scheme added, scope removed and anonymous access
//!   added are info (INFO). Components' security scheme definitions are not compared.
//! - `additionalProperties` has no rule in the oasdiff checker list and is not analysed.
//! Only local `#/` references are followed; others are reported as info and skipped.

use crate::spec::{self, Key, METHODS};
use openidea_plugin::text::position;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use yaml_rust2::{Yaml, YamlLoader};

const MAX_DEPTH: usize = 24;
const MAX_REF_HOPS: usize = 16;
const MAX_STEPS: usize = 50_000;
pub const MAX_FINDINGS: usize = 1000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Breaking,
    NonBreaking,
    Info,
}

impl Severity {
    fn label(self) -> &'static str {
        match self {
            Severity::Breaking => "breaking",
            Severity::NonBreaking => "non-breaking",
            Severity::Info => "info",
        }
    }
}

pub struct Finding {
    pub severity: Severity,
    pub rule: &'static str,
    pub message: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Dir {
    Request,
    Response,
}

#[derive(Clone)]
struct Node<'a> {
    v: &'a Value,
    path: String,
}

impl<'a> Node<'a> {
    fn child(&self, key: &str) -> Option<Node<'a>> {
        self.v.get(key).map(|v| Node { v, path: join(&self.path, key) })
    }
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() { key.to_owned() } else { format!("{path}.{key}") }
}

/// Spec-provided text goes into the report only after this: no control characters, backticks or
/// excess length.
fn clean(text: &str) -> String {
    let cleaned: String = text.chars().map(|c| if c.is_control() { ' ' } else if c == '`' { '’' } else { c }).take(200).collect();
    cleaned
}

struct Side<'a> {
    root: &'a Value,
    keys: HashMap<&'a str, &'a Key>,
    text: &'a str,
    file: &'a str,
}

impl<'a> Side<'a> {
    fn loc(&self, path: &str) -> String {
        let mut path = path;
        loop {
            if let Some(key) = self.keys.get(path) {
                let at = position(self.text, key.at);
                return format!("{}:{}:{}", self.file, at.line + 1, at.character + 1);
            }
            match path.rsplit_once('.') {
                Some((parent, _)) => path = parent,
                None => return self.file.to_owned(),
            }
        }
    }

    fn deref(&self, mut node: Node<'a>) -> Result<Node<'a>, String> {
        for _ in 0..MAX_REF_HOPS {
            let Some(target) = node.v.get("$ref").and_then(Value::as_str) else {
                return Ok(node);
            };
            let Some(pointer) = target.strip_prefix("#/") else {
                return Err(format!("`$ref` {} is not a local reference and was not followed", clean(target)));
            };
            let segments: Vec<String> = pointer.split('/').map(|s| s.replace("~1", "/").replace("~0", "~")).collect();
            let mut current = self.root;
            for segment in &segments {
                let next = match current {
                    Value::Object(map) => map.get(segment),
                    Value::Array(list) => segment.parse::<usize>().ok().and_then(|i| list.get(i)),
                    _ => None,
                };
                current = next.ok_or_else(|| format!("`$ref` {} does not resolve", clean(target)))?;
            }
            node = Node { v: current, path: segments.join(".") };
        }

        Err("`$ref` chain is too long".into())
    }

    fn eff(&self, node: &Node<'a>, depth: usize, out: &mut Eff<'a>) {
        let Ok(node) = self.deref(node.clone()) else { return };
        if let Some(Value::Object(props)) = node.v.get("properties") {
            for (name, v) in props {
                out.props.entry(name.clone()).or_insert_with(|| Node { v, path: join(&join(&node.path, "properties"), name) });
            }
        }
        if let Some(Value::Array(required)) = node.v.get("required") {
            out.required.extend(required.iter().filter_map(Value::as_str).map(str::to_owned));
        }
        out.ty = out.ty.or_else(|| node.v.get("type"));
        out.format = out.format.or_else(|| node.v.get("format"));
        if out.enums.is_none() {
            out.enums = node.v.get("enum").and_then(Value::as_array);
        }
        if out.items.is_none() {
            out.items = node.child("items");
        }
        for key in KEYWORDS {
            if let Some(v) = node.v.get(key) {
                out.kw.entry(key).or_insert(v);
            }
        }
        if depth < 8 {
            if let Some(Value::Array(parts)) = node.v.get("allOf") {
                for (i, v) in parts.iter().enumerate() {
                    self.eff(&Node { v, path: join(&join(&node.path, "allOf"), &i.to_string()) }, depth + 1, out);
                }
            }
        }
    }
}

#[derive(Default)]
struct Eff<'a> {
    props: BTreeMap<String, Node<'a>>,
    required: BTreeSet<String>,
    ty: Option<&'a Value>,
    format: Option<&'a Value>,
    enums: Option<&'a Vec<Value>>,
    items: Option<Node<'a>>,
    kw: BTreeMap<&'static str, &'a Value>,
}

const KEYWORDS: [&str; 14] = [
    "minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum", "minLength", "maxLength", "pattern", "minItems", "maxItems",
    "uniqueItems", "multipleOf", "nullable", "readOnly", "writeOnly",
];

fn types(v: Option<&Value>) -> BTreeSet<String> {
    match v {
        Some(Value::String(t)) => BTreeSet::from([t.clone()]),
        Some(Value::Array(list)) => list.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
        _ => BTreeSet::new(),
    }
}

fn nullable(e: &Eff) -> bool {
    e.kw.get("nullable").and_then(|v| v.as_bool()) == Some(true) || types(e.ty).contains("null")
}

fn flag(v: Option<&&Value>) -> bool {
    v.and_then(|v| v.as_bool()) == Some(true)
}

fn is_multiple(a: f64, b: f64) -> bool {
    if b == 0.0 {
        return false;
    }
    let ratio = a / b;
    (ratio - ratio.round()).abs() < 1e-9
}

struct Diff<'a> {
    old: Side<'a>,
    new: Side<'a>,
    out: Vec<Finding>,
    seen: HashSet<(String, String, Dir)>,
    steps: usize,
    truncated: bool,
}

impl<'a> Diff<'a> {
    fn add(&mut self, severity: Severity, rule: &'static str, message: String, old: Option<&str>, new: Option<&str>) {
        let old = old.map(|p| self.old.loc(p));
        let new = new.map(|p| self.new.loc(p));
        self.out.push(Finding { severity, rule, message, old, new });
    }

    fn note(&mut self, message: String, old: &str, new: &str) {
        let message = format!("{message}; compatibility of that part is unknown");
        self.add(Severity::Info, "reference-not-followed", message, Some(old), Some(new));
    }

    fn step(&mut self) -> bool {
        self.steps += 1;
        if self.steps > MAX_STEPS {
            self.truncated = true;
        }
        !self.truncated
    }

    fn paths(&mut self) {
        let (Some(old), Some(new)) = (self.old.root.get("paths").and_then(Value::as_object), self.new.root.get("paths").and_then(Value::as_object)) else {
            return;
        };
        for (path, o) in old {
            let key = join("paths", path);
            let Some(n) = new.get(path) else {
                self.add(Severity::Breaking, "path-removed", format!("path `{}` was removed", clean(path)), Some(&key), None);
                continue;
            };
            self.path_item(path, Node { v: o, path: key.clone() }, Node { v: n, path: key });
        }
        for path in new.keys().filter(|p| !old.contains_key(*p)) {
            let key = join("paths", path);
            self.add(Severity::NonBreaking, "path-added", format!("path `{}` was added", clean(path)), None, Some(&key));
        }
    }

    fn path_item(&mut self, path: &str, o: Node<'a>, n: Node<'a>) {
        let (o, n) = match (self.old.deref(o.clone()), self.new.deref(n.clone())) {
            (Ok(o), Ok(n)) => (o, n),
            (a, b) => {
                for message in [a.err(), b.err()].into_iter().flatten() {
                    self.note(message, &o.path, &n.path);
                }
                return;
            }
        };
        for method in METHODS {
            let label = format!("{} {}", method.to_uppercase(), clean(path));
            match (o.child(method), n.child(method)) {
                (Some(old), None) => self.add(Severity::Breaking, "operation-removed", format!("operation `{label}` was removed"), Some(&old.path), None),
                (None, Some(new)) => self.add(Severity::NonBreaking, "operation-added", format!("operation `{label}` was added"), None, Some(&new.path)),
                (Some(old), Some(new)) => self.operation(&label, &o, old, &n, new),
                (None, None) => {}
            }
        }
    }

    fn params(side: &Side<'a>, item: &Node<'a>, op: &Node<'a>, notes: &mut Vec<String>) -> BTreeMap<(String, String), Node<'a>> {
        let mut out = BTreeMap::new();
        for owner in [item, op] {
            let Some(Value::Array(list)) = owner.v.get("parameters") else { continue };
            for (i, v) in list.iter().enumerate() {
                let base = Node { v, path: join(&join(&owner.path, "parameters"), &i.to_string()) };
                match side.deref(base) {
                    Ok(p) => {
                        let place = p.v.get("in").and_then(Value::as_str).unwrap_or_default().to_owned();
                        let name = p.v.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();
                        out.insert((place, name), p);
                    }
                    Err(message) => notes.push(message),
                }
            }
        }
        out
    }

    fn operation(&mut self, label: &str, oi: &Node<'a>, o: Node<'a>, ni: &Node<'a>, n: Node<'a>) {
        if !self.step() {
            return;
        }
        let deprecated = |node: &Node| node.v.get("deprecated").and_then(Value::as_bool) == Some(true);
        if !deprecated(&o) && deprecated(&n) {
            self.add(Severity::Info, "operation-deprecated", format!("operation `{label}` was deprecated"), Some(&o.path), Some(&n.path));
        }

        self.security(&o, &n, label);

        let mut notes = Vec::new();
        let old_params = Self::params(&self.old, oi, &o, &mut notes);
        let new_params = Self::params(&self.new, ni, &n, &mut notes);
        for message in notes {
            self.note(message, &o.path, &n.path);
        }
        self.parameters(label, &old_params, &new_params);
        self.request_body(label, &old_params, &o, &new_params, &n);
        self.responses(label, &o, &n);
    }

    fn parameters(&mut self, label: &str, old: &BTreeMap<(String, String), Node<'a>>, new: &BTreeMap<(String, String), Node<'a>>) {
        let required = |key: &(String, String), p: &Node| key.0 == "path" || p.v.get("required").and_then(Value::as_bool) == Some(true);
        let schema_of = |p: &Node<'a>| p.child("schema").unwrap_or_else(|| p.clone());
        let shape = |p: &Node<'a>| p.v.get("schema").unwrap_or(p.v).get("type").cloned();
        let removed: Vec<_> = old.iter().filter(|(k, _)| k.0 != "body" && !new.contains_key(*k)).collect();
        let mut added: Vec<_> = new.iter().filter(|(k, _)| k.0 != "body" && !old.contains_key(*k)).collect();
        for (key, p) in removed {
            let matches: Vec<usize> = added.iter().enumerate().filter(|(_, (k, a))| k.0 == key.0 && shape(a) == shape(p)).map(|(i, _)| i).collect();
            let name = clean(&key.1);
            if let [index] = matches[..] {
                let (new_key, new_node) = added.remove(index);
                let message = format!("{} parameter `{name}` of `{label}` was possibly renamed to `{}`", clean(&key.0), clean(&new_key.1));
                self.add(Severity::Breaking, "parameter-renamed", message, Some(&p.path), Some(&new_node.path));
            } else if required(key, p) {
                self.add(Severity::Breaking, "parameter-removed", format!("required {} parameter `{name}` of `{label}` was removed", clean(&key.0)), Some(&p.path), None);
            } else {
                self.add(Severity::Info, "parameter-removed", format!("optional {} parameter `{name}` of `{label}` was removed", clean(&key.0)), Some(&p.path), None);
            }
        }
        for (key, p) in added {
            let name = clean(&key.1);
            if required(key, p) {
                self.add(Severity::Breaking, "parameter-added-required", format!("required {} parameter `{name}` was added to `{label}`", clean(&key.0)), None, Some(&p.path));
            } else {
                self.add(Severity::NonBreaking, "parameter-added", format!("optional {} parameter `{name}` was added to `{label}`", clean(&key.0)), None, Some(&p.path));
            }
        }
        for (key, o) in old.iter().filter(|(k, _)| k.0 != "body") {
            let Some(n) = new.get(key) else { continue };
            let name = clean(&key.1);
            match (required(key, o), required(key, n)) {
                (false, true) => self.add(Severity::Breaking, "parameter-made-required", format!("{} parameter `{name}` of `{label}` became required", clean(&key.0)), Some(&o.path), Some(&n.path)),
                (true, false) => self.add(Severity::NonBreaking, "parameter-made-optional", format!("{} parameter `{name}` of `{label}` became optional", clean(&key.0)), Some(&o.path), Some(&n.path)),
                _ => {}
            }
            self.schema(schema_of(o), schema_of(n), Dir::Request, &format!("{label} parameter `{name}`"), 0);
        }
    }

    fn bodies(side: &Side<'a>, params: &BTreeMap<(String, String), Node<'a>>, op: &Node<'a>, notes: &mut Vec<String>) -> Option<(bool, BTreeMap<String, Node<'a>>)> {
        if let Some((_, p)) = params.iter().find(|(k, _)| k.0 == "body") {
            let required = p.v.get("required").and_then(Value::as_bool) == Some(true);
            return Some((required, p.child("schema").map(|s| ("body".to_owned(), s)).into_iter().collect()));
        }
        let body = match side.deref(op.child("requestBody")?) {
            Ok(body) => body,
            Err(message) => {
                notes.push(message);
                return None;
            }
        };
        let required = body.v.get("required").and_then(Value::as_bool) == Some(true);
        Some((required, Self::media(&body)))
    }

    fn media(holder: &Node<'a>) -> BTreeMap<String, Node<'a>> {
        match holder.child("content").map(|c| (c.v, c.path)) {
            Some((Value::Object(map), path)) => map
                .iter()
                .map(|(mime, m)| (mime.clone(), Node { v: m, path: join(&join(&path, mime), "schema") }))
                .filter(|(_, n)| n.v.get("schema").is_some())
                .map(|(mime, n)| {
                    let schema = n.v.get("schema").unwrap();
                    (mime, Node { v: schema, path: n.path })
                })
                .collect(),
            _ => holder.child("schema").map(|s| ("*/*".to_owned(), s)).into_iter().collect(),
        }
    }

    fn request_body(&mut self, label: &str, op: &BTreeMap<(String, String), Node<'a>>, o: &Node<'a>, np: &BTreeMap<(String, String), Node<'a>>, n: &Node<'a>) {
        let mut notes = Vec::new();
        let old = Self::bodies(&self.old, op, o, &mut notes);
        let new = Self::bodies(&self.new, np, n, &mut notes);
        for message in notes {
            self.note(message, &o.path, &n.path);
        }
        match (old, new) {
            (None, Some((true, _))) => self.add(Severity::Breaking, "request-body-added-required", format!("`{label}` gained a required request body"), Some(&o.path), Some(&n.path)),
            (None, Some(_)) => self.add(Severity::NonBreaking, "request-body-added", format!("`{label}` gained an optional request body"), Some(&o.path), Some(&n.path)),
            (Some(_), None) => self.add(Severity::Info, "request-body-removed", format!("`{label}` no longer declares a request body"), Some(&o.path), Some(&n.path)),
            (Some((or, om)), Some((nr, nm))) => {
                match (or, nr) {
                    (false, true) => self.add(Severity::Breaking, "request-body-made-required", format!("request body of `{label}` became required"), Some(&o.path), Some(&n.path)),
                    (true, false) => self.add(Severity::NonBreaking, "request-body-made-optional", format!("request body of `{label}` became optional"), Some(&o.path), Some(&n.path)),
                    _ => {}
                }
                self.media_types(label, "request", Dir::Request, om, nm);
            }
            (None, None) => {}
        }
    }

    fn media_types(&mut self, label: &str, what: &str, dir: Dir, old: BTreeMap<String, Node<'a>>, new: BTreeMap<String, Node<'a>>) {
        for (mime, o) in &old {
            let Some(n) = new.get(mime) else {
                self.add(Severity::Breaking, "media-type-removed", format!("{what} media type `{}` of `{label}` was removed", clean(mime)), Some(&o.path), None);
                continue;
            };
            self.schema(o.clone(), n.clone(), dir, &format!("{label} {what} `{}`", clean(mime)), 0);
        }
        for (mime, n) in new.iter().filter(|(m, _)| !old.contains_key(*m)) {
            self.add(Severity::NonBreaking, "media-type-added", format!("{what} media type `{}` was added to `{label}`", clean(mime)), None, Some(&n.path));
        }
    }

    fn responses(&mut self, label: &str, o: &Node<'a>, n: &Node<'a>) {
        let (Some(old), Some(new)) = (o.child("responses"), n.child("responses")) else { return };
        let (Some(oldmap), Some(newmap)) = (old.v.as_object(), new.v.as_object()) else { return };
        for (code, ov) in oldmap {
            let on = Node { v: ov, path: join(&old.path, code) };
            let Some(nv) = newmap.get(code) else {
                let hard = !(code.starts_with('4') || code.starts_with('5'));
                let severity = if hard { Severity::Breaking } else { Severity::Info };
                self.add(severity, "response-removed", format!("response `{}` of `{label}` was removed", clean(code)), Some(&on.path), None);
                continue;
            };
            let nn = Node { v: nv, path: join(&new.path, code) };
            let (on, nn) = match (self.old.deref(on.clone()), self.new.deref(nn.clone())) {
                (Ok(a), Ok(b)) => (a, b),
                (a, b) => {
                    for message in [a.err(), b.err()].into_iter().flatten() {
                        self.note(message, &on.path, &nn.path);
                    }
                    continue;
                }
            };
            self.media_types(&format!("{label} response {}", clean(code)), "response", Dir::Response, Self::media(&on), Self::media(&nn));
        }
        for code in newmap.keys().filter(|c| !oldmap.contains_key(*c)) {
            self.add(Severity::NonBreaking, "response-added", format!("response `{}` was added to `{label}`", clean(code)), None, Some(&join(&new.path, code)));
        }
    }

    fn keywords(&mut self, o: &Node<'a>, n: &Node<'a>, eo: &Eff<'a>, en: &Eff<'a>, dir: Dir, at: &str) {
        let (op, np) = (o.path.as_str(), n.path.as_str());
        let guarded = match dir {
            Dir::Request => flag(en.kw.get("readOnly")),
            Dir::Response => flag(en.kw.get("writeOnly")),
        };
        let num = |e: &Eff, k: &str| e.kw.get(k).and_then(|v| v.as_f64());
        let change = |this: &mut Self, what: &str, tightened: bool, shown: String| {
            let severity = match (dir, tightened) {
                (Dir::Request, true) | (Dir::Response, false) => Severity::Breaking,
                _ => Severity::NonBreaking,
            };
            let severity = if guarded && severity == Severity::Breaking { Severity::Info } else { severity };
            let rule = if tightened { "constraint-tightened" } else { "constraint-loosened" };
            this.add(severity, rule, format!("`{what}` of {at} was {} ({shown})", if tightened { "tightened" } else { "loosened" }), Some(op), Some(np));
        };
        // (keyword, upper bound, default when unset, unset loosens)
        for (key, upper, default, unset_loosens) in [
            ("maximum", true, None, false),
            ("exclusiveMaximum", true, None, false),
            ("maxLength", true, None, true),
            ("maxItems", true, None, false),
            ("minimum", false, None, false),
            ("exclusiveMinimum", false, None, false),
            ("minLength", false, Some(0.0), false),
            ("minItems", false, Some(0.0), false),
        ] {
            let (a, b) = (num(eo, key).or(default), num(en, key).or(default));
            let shown = format!("{} to {}", a.map_or("unset".into(), |v| v.to_string()), b.map_or("unset".into(), |v| v.to_string()));
            let verdict = match (a, b) {
                (Some(a), Some(b)) if a == b => None,
                (Some(a), Some(b)) => Some(if upper { b < a } else { b > a }),
                (None, Some(_)) => Some(true),
                (Some(_), None) if unset_loosens => Some(false),
                _ => None,
            };
            if let Some(tightened) = verdict {
                change(self, key, tightened, shown);
            }
        }
        let (pa, pb) = (eo.kw.get("pattern").and_then(|v| v.as_str()), en.kw.get("pattern").and_then(|v| v.as_str()));
        match (pa, pb) {
            (Some(a), Some(b)) if a != b => self.add(Severity::Info, "pattern-changed", format!("`pattern` of {at} changed from {} to {}; whether it is compatible is unknown", clean(a), clean(b)), Some(op), Some(np)),
            (None, Some(_)) => change(self, "pattern", true, "added".into()),
            (Some(_), None) => change(self, "pattern", false, "removed".into()),
            _ => {}
        }
        match (flag(eo.kw.get("uniqueItems")), flag(en.kw.get("uniqueItems"))) {
            (false, true) => change(self, "uniqueItems", true, "set".into()),
            (true, false) => change(self, "uniqueItems", false, "unset".into()),
            _ => {}
        }
        match (num(eo, "multipleOf"), num(en, "multipleOf")) {
            (None, Some(_)) => change(self, "multipleOf", true, "set".into()),
            (Some(_), None) => change(self, "multipleOf", false, "unset".into()),
            (Some(a), Some(b)) if a != b => {
                if is_multiple(a, b) {
                    change(self, "multipleOf", false, format!("{a} to {b}"));
                } else if is_multiple(b, a) && dir == Dir::Response {
                    change(self, "multipleOf", true, format!("{a} to {b}"));
                } else {
                    self.add(Severity::Breaking, "multiple-of-changed", format!("`multipleOf` of {at} changed from {a} to {b}"), Some(op), Some(np));
                }
            }
            _ => {}
        }
        match (nullable(eo), nullable(en)) {
            (false, true) => {
                let severity = if dir == Dir::Request { Severity::NonBreaking } else { Severity::Breaking };
                self.add(severity, "became-nullable", format!("{at} became nullable"), Some(op), Some(np));
            }
            (true, false) => {
                let severity = if dir == Dir::Request { Severity::Breaking } else { Severity::Info };
                self.add(severity, "became-not-nullable", format!("{at} is no longer nullable"), Some(op), Some(np));
            }
            _ => {}
        }
        for key in ["oneOf", "anyOf"] {
            let (Some(Value::Array(a)), Some(Value::Array(b))) = (o.v.get(key), n.v.get(key)) else { continue };
            let id = |m: &Value| m.get("$ref").and_then(Value::as_str).map_or_else(|| m.to_string(), str::to_owned);
            let (ia, ib): (Vec<String>, Vec<String>) = (a.iter().map(id).collect(), b.iter().map(id).collect());
            for member in ia.iter().filter(|m| !ib.contains(m)) {
                let severity = if dir == Dir::Request { Severity::Breaking } else { Severity::Info };
                self.add(severity, "composition-member-removed", format!("a `{key}` member of {at} was removed: {}", clean(member)), Some(op), Some(np));
            }
            for member in ib.iter().filter(|m| !ia.contains(m)) {
                let severity = if dir == Dir::Request { Severity::NonBreaking } else { Severity::Breaking };
                self.add(severity, "composition-member-added", format!("a `{key}` member of {at} was added: {}", clean(member)), Some(op), Some(np));
            }
        }
        let deprecated = |node: &Node| node.v.get("deprecated").and_then(Value::as_bool) == Some(true);
        match (deprecated(o), deprecated(n)) {
            (false, true) => self.add(Severity::Info, "schema-deprecated", format!("{at} was deprecated"), Some(op), Some(np)),
            (true, false) => self.add(Severity::Info, "schema-reactivated", format!("{at} is no longer deprecated"), Some(op), Some(np)),
            _ => {}
        }
    }

    fn access_flags(&mut self, po: &Node<'a>, pn: &Node<'a>, dir: Dir, required: bool, what: &str) {
        let read = |side: &Side<'a>, p: &Node<'a>, key: &str| {
            let mut e = Eff::default();
            side.eff(p, 0, &mut e);
            flag(e.kw.get(key))
        };
        let (op, np) = (po.path.as_str(), pn.path.as_str());
        for (key, rule_set, rule_unset) in [("readOnly", "read-only-set", "read-only-unset"), ("writeOnly", "write-only-set", "write-only-unset")] {
            let (was, is) = (read(&self.old, po, key), read(&self.new, pn, key));
            if was == is {
                continue;
            }
            let breaking = match (dir, key, is) {
                (Dir::Request, "readOnly", false) | (Dir::Response, "writeOnly", true) => required,
                _ => false,
            };
            let severity = if breaking {
                Severity::Breaking
            } else if dir == Dir::Request && key == "readOnly" && !is {
                Severity::NonBreaking
            } else {
                Severity::Info
            };
            let (rule, verb) = if is { (rule_set, "became") } else { (rule_unset, "is no longer") };
            self.add(severity, rule, format!("{what} {verb} {key}"), Some(op), Some(np));
        }
    }

    fn security(&mut self, o: &Node<'a>, n: &Node<'a>, label: &str) {
        let read = |side: &Side<'a>, op: &Node<'a>| -> (bool, BTreeMap<String, BTreeSet<String>>) {
            let list = op.v.get("security").or_else(|| side.root.get("security"));
            let Some(Value::Array(list)) = list else { return (true, BTreeMap::new()) };
            let mut schemes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            let mut anonymous = list.is_empty();
            for requirement in list {
                let Some(map) = requirement.as_object() else { continue };
                anonymous |= map.is_empty();
                for (name, scopes) in map {
                    let entry = schemes.entry(name.clone()).or_default();
                    entry.extend(scopes.as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned));
                }
            }
            (anonymous, schemes)
        };
        let ((anon_old, old), (anon_new, new)) = (read(&self.old, o), read(&self.new, n));
        let (op, np) = (o.path.as_str(), n.path.as_str());
        match (anon_old, anon_new) {
            (true, false) => self.add(Severity::Breaking, "security-anonymous-access-removed", format!("`{label}` no longer allows anonymous access"), Some(op), Some(np)),
            (false, true) => self.add(Severity::Info, "security-anonymous-access-added", format!("`{label}` now allows anonymous access"), Some(op), Some(np)),
            _ => {}
        }
        for name in old.keys().filter(|k| !new.contains_key(*k)) {
            self.add(Severity::Breaking, "security-scheme-removed", format!("security scheme `{}` was removed from `{label}`", clean(name)), Some(op), Some(np));
        }
        for name in new.keys().filter(|k| !old.contains_key(*k)) {
            self.add(Severity::Info, "security-scheme-added", format!("security scheme `{}` was added to `{label}`", clean(name)), Some(op), Some(np));
        }
        for (name, scopes) in &old {
            let Some(now) = new.get(name) else { continue };
            for scope in now.difference(scopes) {
                self.add(Severity::Breaking, "security-scope-added", format!("scope `{}` of security scheme `{}` is now required by `{label}`", clean(scope), clean(name)), Some(op), Some(np));
            }
            for scope in scopes.difference(now) {
                self.add(Severity::Info, "security-scope-removed", format!("scope `{}` of security scheme `{}` is no longer required by `{label}`", clean(scope), clean(name)), Some(op), Some(np));
            }
        }
    }

    fn schema(&mut self, o: Node<'a>, n: Node<'a>, dir: Dir, at: &str, depth: usize) {
        if depth > MAX_DEPTH || !self.step() {
            return;
        }
        let (o, n) = match (self.old.deref(o.clone()), self.new.deref(n.clone())) {
            (Ok(a), Ok(b)) => (a, b),
            (a, b) => {
                for message in [a.err(), b.err()].into_iter().flatten() {
                    self.note(message, &o.path, &n.path);
                }
                return;
            }
        };
        if !self.seen.insert((o.path.clone(), n.path.clone(), dir)) {
            return;
        }
        let (mut eo, mut en) = (Eff::default(), Eff::default());
        self.old.eff(&o, 0, &mut eo);
        self.new.eff(&n, 0, &mut en);
        let (op, np) = (o.path.as_str(), n.path.as_str());

        if let (Some(a), Some(b)) = (eo.ty, en.ty) {
            let (ta, tb) = (types(Some(a)), types(Some(b)));
            let (ta, tb): (BTreeSet<_>, BTreeSet<_>) = (ta.into_iter().filter(|t| t != "null").collect(), tb.into_iter().filter(|t| t != "null").collect());
            let listed = a.is_array() || b.is_array();
            if ta != tb {
                if listed && ta.is_subset(&tb) {
                    let severity = if dir == Dir::Request { Severity::NonBreaking } else { Severity::Breaking };
                    self.add(severity, "types-widened", format!("the types of {at} were widened from {} to {}", clean(&a.to_string()), clean(&b.to_string())), Some(op), Some(np));
                } else if listed && tb.is_subset(&ta) {
                    let severity = if dir == Dir::Request { Severity::Breaking } else { Severity::Info };
                    self.add(severity, "types-narrowed", format!("the types of {at} were narrowed from {} to {}", clean(&a.to_string()), clean(&b.to_string())), Some(op), Some(np));
                } else {
                    self.add(Severity::Breaking, "type-changed", format!("type of {at} changed from {} to {}", clean(&a.to_string()), clean(&b.to_string())), Some(op), Some(np));
                }
            }
        }
        self.keywords(&o, &n, &eo, &en, dir, at);
        if let (Some(a), Some(b)) = (eo.format, en.format) {
            if a != b {
                self.add(Severity::Breaking, "format-changed", format!("format of {at} changed from {} to {}", clean(&a.to_string()), clean(&b.to_string())), Some(op), Some(np));
            }
        }
        if let (Some(a), Some(b)) = (eo.enums, en.enums) {
            for value in a.iter().filter(|v| !b.contains(v)) {
                let severity = if dir == Dir::Request { Severity::Breaking } else { Severity::Info };
                self.add(severity, "enum-value-removed", format!("enum value {} of {at} was removed", clean(&value.to_string())), Some(op), Some(np));
            }
            for value in b.iter().filter(|v| !a.contains(v)) {
                let severity = if dir == Dir::Response { Severity::Breaking } else { Severity::NonBreaking };
                self.add(severity, "enum-value-added", format!("enum value {} was added to {at}", clean(&value.to_string())), Some(op), Some(np));
            }
        }

        for (name, p) in eo.props.iter().filter(|(k, _)| !en.props.contains_key(*k)) {
            let severity = if dir == Dir::Response { Severity::Breaking } else { Severity::Info };
            self.add(severity, "property-removed", format!("property `{}` of {at} was removed", clean(name)), Some(&p.path), Some(np));
        }
        for (name, p) in en.props.iter().filter(|(k, _)| !eo.props.contains_key(*k)) {
            if dir == Dir::Request && en.required.contains(name) {
                self.add(Severity::Breaking, "request-property-added-required", format!("required property `{}` was added to {at}", clean(name)), Some(op), Some(&p.path));
            } else {
                self.add(Severity::NonBreaking, "property-added", format!("property `{}` was added to {at}", clean(name)), Some(op), Some(&p.path));
            }
        }
        for (name, po) in &eo.props {
            let Some(pn) = en.props.get(name) else { continue };
            let (was, is) = (eo.required.contains(name), en.required.contains(name));
            let shown = clean(name);
            self.access_flags(po, pn, dir, is, &format!("property `{shown}` of {at}"));
            match (dir, was, is) {
                (Dir::Request, false, true) => self.add(Severity::Breaking, "request-property-made-required", format!("property `{shown}` of {at} became required"), Some(&po.path), Some(&pn.path)),
                (Dir::Request, true, false) => self.add(Severity::NonBreaking, "request-property-made-optional", format!("property `{shown}` of {at} is no longer required"), Some(&po.path), Some(&pn.path)),
                (Dir::Response, true, false) => self.add(Severity::Breaking, "response-property-made-optional", format!("property `{shown}` of {at} is no longer always returned"), Some(&po.path), Some(&pn.path)),
                (Dir::Response, false, true) => self.add(Severity::NonBreaking, "response-property-made-required", format!("property `{shown}` of {at} is now always returned"), Some(&po.path), Some(&pn.path)),
                _ => {}
            }
            self.schema(po.clone(), pn.clone(), dir, &format!("{at} › `{shown}`"), depth + 1);
        }
        if let (Some(a), Some(b)) = (eo.items, en.items) {
            self.schema(a, b, dir, &format!("{at} items"), depth + 1);
        }
    }
}

fn to_value(yaml: &Yaml, depth: usize) -> Result<Value, String> {
    if depth > 64 {
        return Err("The document is nested too deeply.".into());
    }
    Ok(match yaml {
        Yaml::Hash(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                let key = match k {
                    Yaml::String(s) | Yaml::Real(s) => s.clone(),
                    Yaml::Integer(i) => i.to_string(),
                    Yaml::Boolean(b) => b.to_string(),
                    _ => return Err("Unsupported mapping key.".into()),
                };
                out.insert(key, to_value(v, depth + 1)?);
            }
            Value::Object(out)
        }
        Yaml::Array(list) => Value::Array(list.iter().map(|v| to_value(v, depth + 1)).collect::<Result<_, _>>()?),
        Yaml::String(s) | Yaml::Real(s) => Value::String(s.clone()),
        Yaml::Integer(i) => Value::from(*i),
        Yaml::Boolean(b) => Value::Bool(*b),
        Yaml::Null => Value::Null,
        _ => return Err("YAML aliases are not supported by the compatibility report.".into()),
    })
}

fn load(file: &str, text: &str) -> Result<Value, String> {
    if spec::is_json(text) {
        return serde_json::from_str(text).map_err(|e| format!("{file} is not valid JSON: {e}"));
    }
    let docs = YamlLoader::load_from_str(text).map_err(|e| format!("{file} is not valid YAML: {e}"))?;
    match docs.as_slice() {
        [doc] => to_value(doc, 0).map_err(|e| format!("{file}: {e}")),
        _ => Err(format!("{file} must contain exactly one YAML document.")),
    }
}

pub fn compare(old_file: &str, old_text: &str, new_file: &str, new_text: &str) -> Result<Vec<Finding>, String> {
    let old_root = load(old_file, old_text)?;
    let new_root = load(new_file, new_text)?;
    let (old_keys, new_keys) = (spec::scan(old_text), spec::scan(new_text));
    let mut diff = Diff {
        old: Side { root: &old_root, keys: spec::exact(&old_keys), text: old_text, file: old_file },
        new: Side { root: &new_root, keys: spec::exact(&new_keys), text: new_text, file: new_file },
        out: Vec::new(),
        seen: HashSet::new(),
        steps: 0,
        truncated: false,
    };
    diff.paths();
    if diff.truncated {
        diff.add(Severity::Info, "analysis-truncated", format!("the analysis stopped after {MAX_STEPS} steps; the report is incomplete"), None, None);
    }

    Ok(diff.out)
}

pub fn render(old_file: &str, new_file: &str, findings: &[Finding]) -> String {
    let code = |value: &str| format!("`{}`", clean(value));
    let count = |s: Severity| findings.iter().filter(|f| f.severity == s).count();
    let mut report = format!(
        "# OpenAPI compatibility report\n\nOld: {}\n\nNew: {}\n\n{} breaking, {} non-breaking, {} info. Rules are a subset of openapi-diff/oasdiff; see the plugin change notes.\n\n",
        code(old_file),
        code(new_file),
        count(Severity::Breaking),
        count(Severity::NonBreaking),
        count(Severity::Info),
    );
    if findings.is_empty() {
        report.push_str("No differences found by the implemented rules.\n");
    }
    for severity in [Severity::Breaking, Severity::NonBreaking, Severity::Info] {
        let group: Vec<_> = findings.iter().filter(|f| f.severity == severity).collect();
        if group.is_empty() {
            continue;
        }
        report.push_str(&format!("## {}\n\n", severity.label()));
        for finding in group.iter().take(MAX_FINDINGS) {
            report.push_str(&format!("- [{}] {}", finding.rule, clean(&finding.message)));
            if let Some(old) = &finding.old {
                report.push_str(&format!(" · old {}", code(old)));
            }
            if let Some(new) = &finding.new {
                report.push_str(&format!(" · new {}", code(new)));
            }
            report.push('\n');
        }
        if group.len() > MAX_FINDINGS {
            report.push_str(&format!("\nOnly the first {MAX_FINDINGS} of {} are shown.\n", group.len()));
        }
        report.push('\n');
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(paths: &str, schemas: &str) -> String {
        format!(r#"{{"openapi":"3.0.0","paths":{paths},"components":{{"schemas":{schemas}}}}}"#)
    }

    fn rules(old: &str, new: &str) -> Vec<(Severity, &'static str)> {
        compare("/old.json", old, "/new.json", new).unwrap().iter().map(|f| (f.severity, f.rule)).collect()
    }

    fn has(old: &str, new: &str, severity: Severity, rule: &str) {
        let found = rules(old, new);
        assert!(found.iter().any(|f| *f == (severity, rule)), "{rule} missing in {found:?}");
    }

    fn op(extra: &str) -> String {
        spec(&op_paths(extra), "{}")
    }

    fn op_paths(extra: &str) -> String {
        format!(r#"{{"/p":{{"get":{{{extra}"responses":{{"200":{{"description":"ok"}}}}}}}}}}"#)
    }

    fn with_param(p: &str) -> String {
        op(&format!(r#""parameters":[{p}],"#))
    }

    fn body(required: bool, schema: &str) -> String {
        let doc = format!(r#"{{"/p":{{"post":{{"requestBody":{{"required":{required},"content":{{"application/json":{{"schema":{schema}}}}}}},"responses":{{}}}}}}}}"#);
        spec(&doc, "{}")
    }

    fn response(code: &str, schema: &str) -> String {
        let doc = format!(r#"{{"/p":{{"get":{{"responses":{{"{code}":{{"description":"d","content":{{"application/json":{{"schema":{schema}}}}}}}}}}}}}}}"#);
        spec(&doc, "{}")
    }

    #[test]
    fn path_and_operation_removed() {
        let old = spec(r#"{"/a":{"get":{},"post":{}},"/b":{"get":{}}}"#, "{}");
        let new = spec(r#"{"/a":{"get":{}}}"#, "{}");
        has(&old, &new, Severity::Breaking, "path-removed");
        has(&old, &new, Severity::Breaking, "operation-removed");
        has(&new, &old, Severity::NonBreaking, "path-added");
        has(&new, &old, Severity::NonBreaking, "operation-added");
    }

    #[test]
    fn deprecation_is_info() {
        has(&op(""), &op(r#""deprecated":true,"#), Severity::Info, "operation-deprecated");
    }

    #[test]
    fn parameter_removed_required_vs_optional() {
        let required = with_param(r#"{"name":"q","in":"query","required":true,"schema":{"type":"string"}}"#);
        let optional = with_param(r#"{"name":"q","in":"query","schema":{"type":"string"}}"#);
        let none = with_param(r#"{"name":"other","in":"query","schema":{"type":"integer"}}"#);
        has(&required, &none, Severity::Breaking, "parameter-removed");
        has(&optional, &none, Severity::Info, "parameter-removed");
    }

    #[test]
    fn parameter_renamed() {
        let old = with_param(r#"{"name":"q","in":"query","schema":{"type":"string"}}"#);
        let new = with_param(r#"{"name":"query","in":"query","schema":{"type":"string"}}"#);
        assert_eq!(rules(&old, &new), [(Severity::Breaking, "parameter-renamed")]);
    }

    #[test]
    fn parameter_added_and_requiredness() {
        let optional = with_param(r#"{"name":"q","in":"query","schema":{"type":"string"}}"#);
        let required = with_param(r#"{"name":"q","in":"query","required":true,"schema":{"type":"string"}}"#);
        has(&op(""), &required, Severity::Breaking, "parameter-added-required");
        has(&op(""), &optional, Severity::NonBreaking, "parameter-added");
        has(&optional, &required, Severity::Breaking, "parameter-made-required");
        has(&required, &optional, Severity::NonBreaking, "parameter-made-optional");
    }

    #[test]
    fn parameter_schema_type_change_and_local_ref() {
        let old = format!(
            r##"{{"openapi":"3.0.0","paths":{{"/p":{{"get":{{"parameters":[{{"$ref":"#/components/parameters/Q"}}],"responses":{{}}}}}}}},"components":{{"parameters":{{"Q":{{"name":"q","in":"query","schema":{{"type":"string"}}}}}}}}}}"##
        );
        let new = old.replace(r#""type":"string""#, r#""type":"integer""#);
        has(&old, &new, Severity::Breaking, "type-changed");
    }

    #[test]
    fn request_body_rules() {
        let schema = r#"{"type":"object"}"#;
        has(&op(""), &body(true, schema).replace("post", "get"), Severity::Breaking, "request-body-added-required");
        has(&body(false, schema), &body(true, schema), Severity::Breaking, "request-body-made-required");
        has(&body(true, schema), &body(false, schema), Severity::NonBreaking, "request-body-made-optional");
        let other = body(true, schema).replace("application/json", "text/plain");
        has(&body(true, schema), &other, Severity::Breaking, "media-type-removed");
    }

    #[test]
    fn response_code_removed() {
        let old = spec(r#"{"/p":{"get":{"responses":{"200":{"description":"a"},"404":{"description":"b"}}}}}"#, "{}");
        let new = spec(r#"{"/p":{"get":{"responses":{"201":{"description":"a"}}}}}"#, "{}");
        has(&old, &new, Severity::Breaking, "response-removed");
        has(&old, &new, Severity::Info, "response-removed");
        has(&old, &new, Severity::NonBreaking, "response-added");
    }

    #[test]
    fn property_removed_depends_on_direction() {
        let old = r#"{"type":"object","properties":{"a":{"type":"string"},"b":{"type":"string"}}}"#;
        let new = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        has(&response("200", old), &response("200", new), Severity::Breaking, "property-removed");
        has(&body(true, old), &body(true, new), Severity::Info, "property-removed");
    }

    #[test]
    fn property_type_changed_through_ref() {
        let doc = |t: &str| spec(r##"{"/p":{"get":{"responses":{"200":{"description":"d","content":{"application/json":{"schema":{"$ref":"#/components/schemas/Pet"}}}}}}}}"##, &format!(r#"{{"Pet":{{"type":"object","properties":{{"age":{{"type":"{t}"}}}}}}}}"#));
        has(&doc("integer"), &doc("string"), Severity::Breaking, "type-changed");
    }

    #[test]
    fn enum_values() {
        let old = r#"{"type":"string","enum":["a","b"]}"#;
        let new = r#"{"type":"string","enum":["a","c"]}"#;
        has(&body(true, old), &body(true, new), Severity::Breaking, "enum-value-removed");
        has(&body(true, old), &body(true, new), Severity::NonBreaking, "enum-value-added");
        has(&response("200", old), &response("200", new), Severity::Breaking, "enum-value-added");
        has(&response("200", old), &response("200", new), Severity::Info, "enum-value-removed");
    }

    #[test]
    fn required_property_changes() {
        let plain = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let required = r#"{"type":"object","required":["a"],"properties":{"a":{"type":"string"}}}"#;
        let added = r#"{"type":"object","required":["z"],"properties":{"a":{"type":"string"},"z":{"type":"string"}}}"#;
        has(&body(true, plain), &body(true, required), Severity::Breaking, "request-property-made-required");
        has(&body(true, plain), &body(true, added), Severity::Breaking, "request-property-added-required");
        has(&response("200", required), &response("200", plain), Severity::Breaking, "response-property-made-optional");
        has(&response("200", plain), &response("200", added), Severity::NonBreaking, "property-added");
    }

    #[test]
    fn remote_refs_are_not_followed() {
        let remote = body(true, r##"{"$ref":"other.yaml#/X"}"##);
        has(&remote, &remote, Severity::Info, "reference-not-followed");
    }

    #[test]
    fn ref_cycles_terminate() {
        let doc = |t: &str| spec(r##"{"/p":{"get":{"responses":{"200":{"description":"d","content":{"application/json":{"schema":{"$ref":"#/components/schemas/N"}}}}}}}}"##, &format!(r##"{{"N":{{"type":"object","properties":{{"next":{{"$ref":"#/components/schemas/N"}},"v":{{"type":"{t}"}}}}}}}}"##));
        let found = rules(&doc("string"), &doc("integer"));
        assert_eq!(found, [(Severity::Breaking, "type-changed")]);
    }

    #[test]
    fn yaml_specs_and_locations() {
        let old = "openapi: 3.0.0\npaths:\n  /a:\n    get:\n      responses:\n        200:\n          description: ok\n";
        let new = "openapi: 3.0.0\npaths:\n  /b:\n    get:\n      responses:\n        200:\n          description: ok\n";
        let found = compare("/o.yaml", old, "/n.yaml", new).unwrap();
        assert_eq!(found[0].rule, "path-removed");
        assert_eq!(found[0].old.as_deref(), Some("/o.yaml:3:3"));
        assert!(render("/o.yaml", "/n.yaml", &found).contains("[path-removed]"));
    }

    #[test]
    fn report_escapes_spec_text() {
        let old = spec(r#"{"/a`x\ny":{"get":{}}}"#, "{}");
        let found = compare("/o.json", &old, "/n.json", &spec("{}", "{}")).unwrap();
        let report = render("/o.json", "/n.json", &found);
        assert!(!report.contains("a`x"));
        assert!(!report.contains("x\ny"));
    }

    #[test]
    fn invalid_documents_are_refused() {
        assert!(compare("/o.json", "{", "/n.json", "{}").is_err());
    }
    fn schema_req(s: &str) -> String {
        body(true, s)
    }

    #[test]
    fn constraints_by_direction() {
        let (a, b) = (r#"{"type":"integer","maximum":10,"minimum":1}"#, r#"{"type":"integer","maximum":5,"minimum":2}"#);
        has(&schema_req(a), &schema_req(b), Severity::Breaking, "constraint-tightened");
        has(&schema_req(b), &schema_req(a), Severity::NonBreaking, "constraint-loosened");
        has(&response("200", b), &response("200", a), Severity::Breaking, "constraint-loosened");
        has(&response("200", a), &response("200", b), Severity::NonBreaking, "constraint-tightened");
        let s = r#"{"type":"string"}"#;
        has(&schema_req(s), &schema_req(r#"{"type":"string","maxLength":3}"#), Severity::Breaking, "constraint-tightened");
        has(&schema_req(s), &schema_req(r#"{"type":"string","minLength":3}"#), Severity::Breaking, "constraint-tightened");
        has(&response("200", r#"{"type":"string","maxLength":3}"#), &response("200", s), Severity::Breaking, "constraint-loosened");
        has(&schema_req(s), &schema_req(r#"{"type":"string","exclusiveMaximum":3}"#), Severity::Breaking, "constraint-tightened");
    }

    #[test]
    fn item_and_collection_constraints() {
        let a = r#"{"type":"array","items":{"type":"string"}}"#;
        has(&schema_req(a), &schema_req(r#"{"type":"array","items":{"type":"string"},"maxItems":2}"#), Severity::Breaking, "constraint-tightened");
        has(&schema_req(a), &schema_req(r#"{"type":"array","items":{"type":"string"},"minItems":1}"#), Severity::Breaking, "constraint-tightened");
        has(&schema_req(a), &schema_req(r#"{"type":"array","items":{"type":"string"},"uniqueItems":true}"#), Severity::Breaking, "constraint-tightened");
        has(&response("200", r#"{"type":"array","items":{"type":"string"},"minItems":1}"#), &response("200", a), Severity::Breaking, "constraint-loosened");
        has(&response("200", r#"{"type":"array","items":{"type":"string"},"uniqueItems":true}"#), &response("200", a), Severity::Breaking, "constraint-loosened");
    }

    #[test]
    fn pattern_and_multiple_of() {
        let s = r#"{"type":"string"}"#;
        let p = r#"{"type":"string","pattern":"^a"}"#;
        has(&schema_req(s), &schema_req(p), Severity::Breaking, "constraint-tightened");
        has(&response("200", p), &response("200", s), Severity::Breaking, "constraint-loosened");
        has(&response("200", s), &response("200", p), Severity::NonBreaking, "constraint-tightened");
        has(&schema_req(p), &schema_req(r#"{"type":"string","pattern":"^b"}"#), Severity::Info, "pattern-changed");
        let m = |v: u32| format!(r#"{{"type":"integer","multipleOf":{v}}}"#);
        has(&schema_req(&m(10)), &schema_req(&m(5)), Severity::NonBreaking, "constraint-loosened");
        has(&schema_req(&m(5)), &schema_req(&m(10)), Severity::Breaking, "multiple-of-changed");
        has(&schema_req(r#"{"type":"integer"}"#), &schema_req(&m(2)), Severity::Breaking, "constraint-tightened");
    }

    #[test]
    fn nullability_by_direction() {
        let plain = r#"{"type":"string"}"#;
        let nullable = r#"{"type":"string","nullable":true}"#;
        has(&schema_req(plain), &schema_req(nullable), Severity::NonBreaking, "became-nullable");
        has(&schema_req(nullable), &schema_req(plain), Severity::Breaking, "became-not-nullable");
        has(&response("200", plain), &response("200", nullable), Severity::Breaking, "became-nullable");
        has(&response("200", nullable), &response("200", plain), Severity::Info, "became-not-nullable");
        let list = r#"{"type":["string","null"]}"#;
        has(&response("200", plain), &response("200", list), Severity::Breaking, "became-nullable");
        assert!(!rules(&response("200", plain), &response("200", list)).iter().any(|r| r.1 == "type-changed"));
    }

    #[test]
    fn type_lists() {
        let (one, two) = (r#"{"type":"string"}"#, r#"{"type":["string","integer"]}"#);
        has(&schema_req(one), &schema_req(two), Severity::NonBreaking, "types-widened");
        has(&schema_req(two), &schema_req(one), Severity::Breaking, "types-narrowed");
        has(&response("200", one), &response("200", two), Severity::Breaking, "types-widened");
        has(&response("200", two), &response("200", one), Severity::Info, "types-narrowed");
    }

    #[test]
    fn one_of_and_any_of_members() {
        for key in ["oneOf", "anyOf"] {
            let two = format!(r#"{{"{key}":[{{"type":"string"}},{{"type":"integer"}}]}}"#);
            let one = format!(r#"{{"{key}":[{{"type":"string"}}]}}"#);
            has(&schema_req(&two), &schema_req(&one), Severity::Breaking, "composition-member-removed");
            has(&schema_req(&one), &schema_req(&two), Severity::NonBreaking, "composition-member-added");
            has(&response("200", &one), &response("200", &two), Severity::Breaking, "composition-member-added");
            has(&response("200", &two), &response("200", &one), Severity::Info, "composition-member-removed");
        }
    }

    #[test]
    fn read_only_and_write_only() {
        let obj = |extra: &str, req: &str| format!(r#"{{"type":"object","required":[{req}],"properties":{{"a":{{"type":"string"{extra}}}}}}}"#);
        let (plain, ro, wo) = (obj("", r#""a""#), obj(r#","readOnly":true"#, r#""a""#), obj(r#","writeOnly":true"#, r#""a""#));
        has(&schema_req(&ro), &schema_req(&plain), Severity::Breaking, "read-only-unset");
        has(&schema_req(&obj(r#","readOnly":true"#, "")), &schema_req(&obj("", "")), Severity::NonBreaking, "read-only-unset");
        has(&schema_req(&plain), &schema_req(&ro), Severity::Info, "read-only-set");
        has(&response("200", &plain), &response("200", &wo), Severity::Breaking, "write-only-set");
        has(&response("200", &obj("", "")), &response("200", &obj(r#","writeOnly":true"#, "")), Severity::Info, "write-only-set");
        let tight = |extra: &str| format!(r#"{{"type":"object","properties":{{"a":{{"type":"integer"{extra}}}}}}}"#);
        has(&schema_req(&tight(r#","readOnly":true"#)), &schema_req(&tight(r#","readOnly":true,"maximum":3"#)), Severity::Info, "constraint-tightened");
    }

    #[test]
    fn deprecated_schema_is_info() {
        let (a, b) = (r#"{"type":"string"}"#, r#"{"type":"string","deprecated":true}"#);
        has(&schema_req(a), &schema_req(b), Severity::Info, "schema-deprecated");
    }

    #[test]
    fn security_requirements() {
        let sec = |s: &str| spec(&format!(r#"{{"/p":{{"get":{{"security":{s},"responses":{{}}}}}}}}"#), "{}");
        has(&sec("[]"), &sec(r#"[{"k":[]}]"#), Severity::Breaking, "security-anonymous-access-removed");
        has(&sec(r#"[{"k":[]}]"#), &sec(r#"[{"k":["w"]}]"#), Severity::Breaking, "security-scope-added");
        has(&sec(r#"[{"k":["w"]}]"#), &sec(r#"[{"k":[]}]"#), Severity::Info, "security-scope-removed");
        has(&sec(r#"[{"k":[]}]"#), &sec(r#"[{"j":[]}]"#), Severity::Breaking, "security-scheme-removed");
        has(&sec(r#"[{"k":[]}]"#), &sec(r#"[{"k":[]},{"j":[]}]"#), Severity::Info, "security-scheme-added");
        has(&sec(r#"[{"k":[]}]"#), &sec(r#"[{"k":[]},{}]"#), Severity::Info, "security-anonymous-access-added");
    }

}
