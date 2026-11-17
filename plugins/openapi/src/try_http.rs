//! Bounded local request flow. Native host API 1.4 is required for redirect enforcement.
use crate::{OpenApi, json, spec};
use openidea_plugin::{
    Context, Error, HttpRequest, Invocation, PickItem, host,
    views::{Body, Button, Field, FieldValue, View},
};
use serde_json::Value;
use yaml_rust2::Yaml;
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};
const MAX_SPEC: usize = 128 << 10;
const MAX_BODY: usize = 256 << 10;
const DISPLAY: usize = 64 << 10;
const HOST: &str = "127.0.0.1";
#[derive(Default)]
pub(crate) struct State {
    safe: bool,
    granted: bool,
    ask: bool,
    scope_ok: bool,
    next_nonce: u64,
    pending: Option<u64>,
    preview: Option<Prepared>,
    response: Option<Reply>,
    outcome: Option<Outcome>,
}
#[derive(Clone, Debug)]
struct Parameter {
    name: String,
    path: bool,
    required: bool,
    kind: String,
    choices: Vec<String>,
}
#[derive(Clone, Debug)]
struct Operation {
    method: String,
    path: String,
    base: String,
    parameters: Vec<Parameter>,
}
#[derive(Clone)]
struct Binding {
    root: String,
    path: String,
    text: String,
    version: u64,
}
struct Prepared {
    source: Binding,
    operation: Operation,
    values: Vec<String>,
    url: String,
    review: String,
    nonce: u64,
    at: Instant,
}
struct Reply {
    source: Binding,
    text: String,
}
struct Outcome {
    source: Binding,
    reason: &'static str,
}
fn failure_reason(error: &Error) -> &'static str {
    match error {
        Error::Denied(_) => "Request denied by the host: it needs the net:127.0.0.1 permission (net:ask for other hosts). Allow it in Settings › Plugins › OpenAPI.",
        Error::Invalid(_) => "Request or response is unsupported or exceeds the local limits. No response is shown.",
        _ => "Request failed or did not complete. No response is shown. Prepare a new request to retry.",
    }
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn safe_api(version: &str) -> bool {
    let mut parts = version.split('.');
    let major = parts.next();
    let minor = parts.next();
    let patch = parts.next();
    let numeric = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    major == Some("1")
        && minor.is_some_and(|s| numeric(s) && s.parse::<u32>().is_ok_and(|v| v >= 4))
        && patch.is_none_or(numeric)
        && parts.next().is_none()
}
fn plain(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
fn no_auth(value: Option<&Value>) -> bool {
    value.is_none_or(|v| v.as_array().is_some_and(Vec::is_empty))
}
fn remote_authority(authority: &str) -> bool {
    let (host, port) = authority.split_once(':').map_or((authority, None), |(h, p)| (h, Some(p)));
    let port_ok = port.is_none_or(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && p.parse::<u16>().is_ok_and(|n| n > 0));
    let host_ok = host.len() <= 253
        && host.split('.').all(|l| !l.is_empty() && !l.starts_with('-') && !l.ends_with('-') && l.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'));
    let labels: Vec<&str> = host.split('.').collect();
    let tld = labels.last().copied().unwrap_or("");
    let numeric_label = |l: &str| l.bytes().all(|b| b.is_ascii_digit()) || l.strip_prefix("0x").is_some_and(|h| h.bytes().all(|b| b.is_ascii_hexdigit()));
    // ponytail: name-only guard; a public name can still resolve to a private address (DNS rebinding), so the net:ask prompt at send time is the backstop. Resolving and pinning the IP needs a host API.
    let public_name = labels.len() > 1
        && (tld.starts_with("xn--") || tld.bytes().all(|b| b.is_ascii_lowercase()))
        && !matches!(tld, "localhost" | "local" | "internal" | "lan")
        && !host.ends_with(".home.arpa")
        && !labels.iter().any(|l| numeric_label(l));
    port_ok && host_ok && public_name && host != "localhost"
}
/// With `ask` (net:ask granted), a literal `https://host[:port]` server is also accepted; the host asks the user
/// at send time, so the spec never gains network reach the user has not approved.
fn literal_base(url: &str, ask: bool) -> Result<String, Error> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .ok_or_else(|| invalid("Use a literal HTTP(S) loopback server."))?;
    if url.len() > 1024 || url.chars().any(char::is_control) || rest.contains(['?', '#', '@', '\\', '%', '{', '}']) {
        return Err(invalid("Unsupported server URL form."));
    }
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let remote = ask && url.starts_with("https://") && !authority.starts_with("127.0.0.1:") && remote_authority(authority);
    if remote {
        if !path.bytes().all(|b| b.is_ascii_alphanumeric() || b"/_-.~".contains(&b)) || path.split('/').any(|p| matches!(p, "." | "..")) {
            return Err(invalid("Server paths must be literal unencoded segments."));
        }
        return Ok(url.trim_end_matches('/').into());
    }
    let port = authority.strip_prefix("127.0.0.1:").ok_or_else(|| invalid("Only literal 127.0.0.1 with an explicit port, or an https host when net:ask is granted, is supported."))?;
    if port.is_empty()
        || !port.bytes().all(|b| b.is_ascii_digit())
        || port.starts_with('0')
        || port.parse::<u16>().ok().is_none_or(|p| p == 0)
    {
        return Err(invalid("Invalid explicit loopback port."));
    }
    if !path.bytes().all(|b| b.is_ascii_alphanumeric() || b"/_-.~".contains(&b)) || path.split('/').any(|p| matches!(p, "." | "..")) {
        return Err(invalid("Server paths must be literal unencoded segments."));
    }
    Ok(url.trim_end_matches('/').into())
}
fn schema(value: &Value, path: bool) -> Result<Parameter, Error> {
    if !value.is_object() || value.get("$ref").is_some() {
        return Err(invalid("Referenced or unknown parameters are unsupported."));
    }
    let name = value["name"].as_str().filter(|s| plain(s)).ok_or_else(|| invalid("Use plain scalar parameter names."))?;
    if value.get("required").is_some_and(|v| !v.is_boolean()) {
        return Err(invalid("Required must be a literal boolean."));
    }
    let required = value.get("required").and_then(Value::as_bool).unwrap_or(false);
    if path && !required {
        return Err(invalid("Path parameters must be explicitly required."));
    }
    if value.get("style").is_some_and(|s| s != if path { "simple" } else { "form" })
        || value.get("explode").is_some_and(|e| !e.is_boolean())
    {
        return Err(invalid("Unsupported parameter serialization."));
    }
    let s = &value["schema"];
    let kind = s["type"]
        .as_str()
        .filter(|s| matches!(*s, "string" | "integer" | "number" | "boolean"))
        .ok_or_else(|| invalid("Only declared scalar parameter types are supported."))?;
    if s.get("$ref").is_some()
        || s.get("nullable").is_some()
        || s.get("oneOf").is_some()
        || s.get("anyOf").is_some()
        || s.get("allOf").is_some()
        || value.get("content").is_some()
    {
        return Err(invalid("Unsupported parameter schema."));
    }
    let choices = if let Some(e) = s.get("enum") {
        let e = e.as_array().filter(|e| !e.is_empty() && e.len() <= 32).ok_or_else(|| invalid("Oversized enum."))?;
        e.iter()
            .map(|v| match v {
                Value::String(s) if kind == "string" => Ok(s.clone()),
                Value::Number(n) if kind == "number" || kind == "integer" && n.as_i64().is_some() => Ok(v.to_string()),
                Value::Bool(_) if kind == "boolean" => Ok(v.to_string()),
                _ => Err(invalid("Only scalar enum values are supported.")),
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![]
    };
    if choices.iter().any(|s| s.len() > 256 || s.chars().any(char::is_control)) {
        return Err(invalid("Invalid enum value."));
    }
    Ok(Parameter { name: name.into(), path, required, kind: kind.into(), choices })
}
/// The JSON value of a YAML source, so YAML runs every JSON check unchanged. Duplicate keys
/// are refused by the loader; a quoted and an unquoted key that spell the same JSON name are refused here.
fn yaml_value(yaml: &Yaml) -> Result<Value, Error> {
    Ok(match yaml {
        Yaml::Hash(map) => {
            let mut out = serde_json::Map::new();
            for (key, value) in map {
                let key = match key {
                    Yaml::String(s) => s.clone(),
                    Yaml::Integer(i) => i.to_string(),
                    _ => return Err(invalid("Use string or integer YAML keys.")),
                };
                if out.insert(key, yaml_value(value)?).is_some() {
                    return Err(invalid("Duplicate or ambiguous YAML members."));
                }
            }
            Value::Object(out)
        }
        Yaml::Array(items) => Value::Array(items.iter().map(yaml_value).collect::<Result<_, _>>()?),
        Yaml::String(s) => Value::String(s.clone()),
        Yaml::Integer(i) => Value::from(*i),
        Yaml::Real(_) => yaml.as_f64().and_then(serde_json::Number::from_f64).map(Value::Number).ok_or_else(|| invalid("Unsupported YAML number."))?,
        Yaml::Boolean(b) => Value::Bool(*b),
        Yaml::Null => Value::Null,
        _ => return Err(invalid("Unsupported YAML value.")),
    })
}
fn operations(text: &str, ask: bool) -> Result<Vec<Operation>, Error> {
    if text.len() > MAX_SPEC {
        return Err(invalid("Spec exceeds 128 KiB."));
    }
    let root: Value = if spec::is_json(text) {
        let root = serde_json::from_str(text).map_err(|_| invalid("Use a valid JSON OpenAPI source."))?;
        let mut keys = BTreeSet::new();
        for key in json::keys(text) {
            if !keys.insert(key.key) {
                return Err(invalid("Duplicate or ambiguous JSON members."));
            }
        }
        root
    } else {
        // One document without anchors, aliases, tags or merge keys, bounded before loading.
        let yaml = crate::yaml_pointer::model(text)
            .ok_or_else(|| invalid("Use a valid YAML OpenAPI source: one document, no anchors, aliases, tags, merge keys or duplicate keys."))?;
        yaml_value(&yaml)?
    };
    let version = root["openapi"].as_str().unwrap_or("");
    if !version
        .rsplit_once('.')
        .is_some_and(|(prefix, patch)| matches!(prefix, "3.0" | "3.1") && !patch.is_empty() && patch.bytes().all(|c| c.is_ascii_digit()))
    {
        return Err(invalid("Only OpenAPI 3.0/3.1 is supported."));
    }
    if !no_auth(root.get("security")) {
        return Err(invalid("Authenticated operations are unsupported."));
    }
    let servers = root["servers"].as_array().filter(|a| a.len() == 1).ok_or_else(|| invalid("Declare exactly one literal root server."))?;
    if servers[0].get("variables").is_some() {
        return Err(invalid("Server variables are unsupported."));
    }
    let base = literal_base(servers[0]["url"].as_str().ok_or_else(|| invalid("Missing literal server URL."))?, ask)?;
    let paths = root["paths"].as_object().ok_or_else(|| invalid("Missing paths object."))?;
    if paths.len() > 64 {
        return Err(invalid("Too many paths."));
    }
    let mut out = Vec::new();
    for (path, item) in paths {
        if !path.starts_with('/')
            || path.len() > 512
            || path.contains(['?', '#', '%', '\\'])
            || !path.bytes().all(|b| b.is_ascii_alphanumeric() || b"/_-.~{}".contains(&b))
            || path.split('/').any(|p| matches!(p, "." | ".."))
        {
            return Err(invalid("Unsupported path spelling."));
        }
        if !item.is_object() || item.get("$ref").is_some() || item.get("servers").is_some() {
            return Err(invalid("Referenced/overridden path ownership is unsupported."));
        }
        for method in ["get", "head"] {
            let Some(op) = item.get(method) else {
                continue;
            };
            if !op.is_object() || op.get("servers").is_some() || op.get("requestBody").is_some() || !no_auth(op.get("security")) {
                continue;
            }
            let mut parameters = Vec::new();
            let mut names = BTreeSet::new();
            for list in [item.get("parameters"), op.get("parameters")].into_iter().flatten() {
                let list = list.as_array().ok_or_else(|| invalid("Parameters must be an array."))?;
                for p in list {
                    let is_path = match p["in"].as_str() {
                        Some("path") => true,
                        Some("query") => false,
                        _ => return Err(invalid("Header/cookie/auth parameters are unsupported.")),
                    };
                    let p = schema(p, is_path)?;
                    if !names.insert((p.path, p.name.clone())) {
                        return Err(invalid("Duplicate or inherited override parameters are unsupported."));
                    }
                    parameters.push(p);
                }
            }
            if parameters.len() > 16 {
                return Err(invalid("Too many parameters."));
            }
            let mut placeholders = BTreeSet::new();
            for segment in path.split('/') {
                if segment.contains(['{', '}']) {
                    let key = segment
                        .strip_prefix('{')
                        .and_then(|s| s.strip_suffix('}'))
                        .filter(|s| plain(s))
                        .ok_or_else(|| invalid("Templates must occupy a whole path segment."))?;
                    if !placeholders.insert(key.to_owned()) {
                        return Err(invalid("Repeated path placeholder."));
                    }
                }
            }
            if placeholders != parameters.iter().filter(|p| p.path).map(|p| p.name.clone()).collect() {
                return Err(invalid("Path placeholder/parameter membership mismatch."));
            }
            out.push(Operation { method: method.to_uppercase(), path: path.clone(), base: base.clone(), parameters });
        }
    }
    if out.is_empty() || out.len() > 64 {
        return Err(invalid("No bounded no-auth GET/HEAD operation is available."));
    }
    Ok(out)
}
fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
fn url(op: &Operation, values: &[String]) -> Result<String, Error> {
    if values.len() != op.parameters.len() {
        return Err(invalid("Parameter binding mismatch."));
    }
    let mut path = op.path.clone();
    let mut query = Vec::new();
    for (p, value) in op.parameters.iter().zip(values) {
        if value.len() > 256
            || value.chars().any(char::is_control)
            || p.required && value.is_empty()
            || p.path && matches!(value.as_str(), "." | "..")
        {
            return Err(invalid("Invalid parameter value."));
        }
        if value.is_empty() {
            continue;
        }
        if !p.choices.is_empty() && !p.choices.contains(value) {
            return Err(invalid("Value is outside its declared enum."));
        }
        let valid = match p.kind.as_str() {
            "integer" => value.parse::<i64>().is_ok(),
            "number" => value.parse::<f64>().is_ok_and(f64::is_finite),
            "boolean" => matches!(value.as_str(), "true" | "false"),
            _ => true,
        };
        if !valid {
            return Err(invalid("Value does not match its declared scalar type."));
        }
        if p.path {
            path = path.replace(&format!("{{{}}}", p.name), &encode(value));
        } else {
            query.push(format!("{}={}", encode(&p.name), encode(value)));
        }
    }
    let request = format!("{}{}{}", op.base, path, if query.is_empty() { String::new() } else { format!("?{}", query.join("&")) });
    if request.len() > 4096 {
        return Err(invalid("URL exceeds 4096 bytes."));
    }
    Ok(request)
}
fn binding(ctx: &Invocation) -> Result<Binding, Error> {
    let project = ctx.project().ok_or_else(|| invalid("Open a concrete project first."))?;
    let root = project.root()?;
    let editor = ctx.editor().ok_or_else(|| invalid("Focus a JSON or YAML OpenAPI source file."))?;
    let path = editor.path();
    let json_source = path.ends_with(".json") && editor.language() == "json";
    let yaml_source = crate::is_yaml(&path) && editor.language() == "yaml";
    if !json_source && !yaml_source {
        return Err(invalid("Focus a JSON or YAML OpenAPI source first."));
    }
    let actual = host::project_of(&path).ok_or(Error::Stale)?.root()?;
    if actual != root
        || !path.starts_with(&format!("{}/", root.trim_end_matches('/')))
        || path.split('/').any(|s| matches!(s, "." | ".."))
        || !crate::is_spec_file(&path)
    {
        return Err(Error::Stale);
    }
    let doc = editor.document()?;
    if doc.text.len() > MAX_SPEC {
        return Err(invalid("Spec exceeds 128 KiB."));
    }
    Ok(Binding { root, path, text: doc.text, version: doc.version })
}
fn revalidate(ctx: &Invocation, old: &Binding) -> Result<(), Error> {
    let now = binding(ctx)?;
    if now.root != old.root || now.path != old.path || now.version != old.version || now.text != old.text {
        return Err(Error::Stale);
    }
    Ok(())
}
impl State {
    fn guard(&self) -> Result<(), Error> {
        if !self.safe {
            return Err(invalid("Execution requires host API 1.4 with per-hop redirect grant enforcement."));
        }
        if !self.scope_ok {
            return Err(invalid(
                "Try it out works with net:127.0.0.1 or net:ask only. Remove other network permissions for this plugin.",
            ));
        }
        if !self.granted && !self.ask {
            return Err(ui_kit::denied::map(Error::Denied(String::new()), "Try it out", "net:127.0.0.1", "OpenAPI"));
        }
        Ok(())
    }
    fn begin(&mut self) -> Result<u64, Error> {
        self.pending = None;
        self.preview = None;
        self.response = None;
        self.outcome = None;
        self.next_nonce = self.next_nonce.checked_add(1).ok_or_else(|| invalid("Request nonce exhausted; reload the plugin."))?;
        self.pending = Some(self.next_nonce);
        Ok(self.next_nonce)
    }
    fn consume(&mut self, nonce: u64) -> Result<(), Error> {
        if self.pending.take() != Some(nonce) {
            return Err(Error::Stale);
        }
        Ok(())
    }
    pub fn activate(&mut self, context: &Context) {
        self.pending = None;
        self.preview = None;
        self.response = None;
        self.outcome = None;
        self.next_nonce = self.next_nonce.saturating_add(1);
        self.safe = safe_api(&context.api_version) && self.next_nonce < u64::MAX;
        self.granted = context.has("net:127.0.0.1");
        self.ask = context.has("net:ask");
        self.scope_ok = context
            .capabilities
            .iter()
            .filter(|c| c.starts_with("net:") || c.starts_with("net.http:") || c.starts_with("net-http:"))
            .all(|c| c == "net:127.0.0.1" || c == "net:ask");
    }
    pub fn view(&self, ctx: &Invocation) -> View {
        if self.guard().is_err() {
            return View::message(
                "Local execution is unavailable: a compatible API 1.4 host and only the literal loopback grant are required.",
            );
        }
        if let Some(preview) = &self.preview {
            if preview.at.elapsed() > Duration::from_secs(120) || revalidate(ctx, &preview.source).is_err() {
                return View::message("Prepared request does not match the current project/editor/source. Prepare it again.");
            }
            return View::new(Body::Form(vec![Field {
                key: "request".into(),
                label: "Read-only request preview".into(),
                value: FieldValue::readonly_text(&preview.review, true),
            }]))
            .status("Not sent. Send approves only this exact request.")
            .button(Button::new(&format!("send:{}", preview.nonce), &format!("Send {} once", preview.operation.method)).icon("send"));
        }
        if let Some(outcome) = &self.outcome {
            if revalidate(ctx, &outcome.source).is_err() {
                return View::message("Request outcome belongs to another project/editor/source version.");
            }
            return View::message(outcome.reason);
        }
        if let Some(response) = &self.response {
            if revalidate(ctx, &response.source).is_err() {
                return View::message("Captured response belongs to another project/editor/source version.");
            }
            return View::new(Body::Form(vec![Field {
                key: "response".into(),
                label: "HTTP response (plain text)".into(),
                value: FieldValue::readonly_text(&response.text, true),
            }]));
        }
        View::message("No prepared request or response. Prepare a local GET/HEAD request from the current JSON or YAML OpenAPI source.")
    }
}
impl OpenApi {
    /// Prompts produce a readonly intent only. This command never fetches.
    pub(crate) fn try_local(&mut self, ctx: &Invocation) -> Result<Option<String>, Error> {
        self.try_http.guard()?; // No host calls at all on the released API1.3 branch.
        let nonce = self.try_http.begin()?;
        let result = (|| {
            let source = binding(ctx)?;
            let operations = operations(&source.text, self.try_http.ask)?;
            let choices: Vec<_> = operations
                .iter()
                .enumerate()
                .map(|(i, op)| PickItem { key: i.to_string(), label: format!("{} {}", op.method, op.path), description: None })
                .collect();
            let selected = host::pick("Prepare a declared local GET/HEAD request", &choices)?.ok_or(Error::Cancelled)?;
            let op = selected.parse::<usize>().ok().and_then(|i| operations.get(i)).ok_or(Error::Stale)?;
            let mut values = Vec::new();
            for p in &op.parameters {
                values.push(
                    host::input(
                        &format!(
                            "{} parameter {}{}",
                            if p.path { "Path" } else { "Query" },
                            p.name,
                            if p.required { " (required)" } else { " (blank omits)" }
                        ),
                        None,
                    )?
                    .ok_or(Error::Cancelled)?,
                );
            }
            let concrete = url(op, &values)?;
            revalidate(ctx, &source)?;
            if self.try_http.pending != Some(nonce) {
                return Err(Error::Stale);
            }
            let review = format!(
                "Request ticket: {nonce}\nProject: {}\nSource: {} (version {})\n\n{} {}\n\nNo Authorization/Cookie headers, request body, secret-store values or user headers are added. Only declared scalar path/query inputs are sent.\nTimeout: 5 seconds. Host transfer cap: 8 MiB; accepts at most 256 KiB and displays at most 64 KiB.\nAPI 1.4 may follow up to 10 redirects within granted 127.0.0.1 (including other ports/paths) or hosts you allowed. Other hosts and HTTPS-to-HTTP downgrades are denied before connection. A host not yet allowed asks you at send time. Host proxy/TLS settings still apply.\n\nThis immutable preview has not sent anything. Send approves this concrete request once, in the current project/editor/source.",
                source.root, source.path, source.version, op.method, concrete
            );
            if review.len() > 16 << 10 {
                return Err(invalid("Exact preview exceeds 16 KiB; reduce the request."));
            }
            self.try_http.preview =
                Some(Prepared { source, operation: op.clone(), values, url: concrete, review, nonce, at: Instant::now() });
            crate::ui_group::reveal_view("try-response")?;
            Ok(None)
        })();
        if result.is_err() && self.try_http.pending == Some(nonce) {
            self.try_http.pending = None;
            self.try_http.preview = None;
        }
        crate::ui_group::refresh_view("try-response");
        result
    }
    /// A new native ViewEvent supplies current window project/editor context.
    /// No picker/input/confirmation/modal is permitted after this entry point.
    pub(crate) fn send_prepared(&mut self, ctx: &Invocation, key: &str) -> Result<(), Error> {
        self.try_http.guard()?;
        let nonce = key.strip_prefix("send:").and_then(|v| v.parse::<u64>().ok()).ok_or(Error::Stale)?;
        if self.try_http.preview.as_ref().is_none_or(|p| p.nonce != nonce) || self.try_http.pending != Some(nonce) {
            return Err(Error::Stale);
        }
        let prepared = self.try_http.preview.take().ok_or(Error::Stale)?;
        self.try_http.consume(nonce)?;
        self.try_http.response = None;
        self.try_http.outcome = None;
        let result = (|| {
            if prepared.at.elapsed() > Duration::from_secs(120) {
                return Err(Error::Stale);
            }
            revalidate(ctx, &prepared.source)?;
            if url(&prepared.operation, &prepared.values)? != prepared.url {
                return Err(Error::Stale);
            }
            let response = host::fetch(&HttpRequest {
                method: prepared.operation.method.clone(),
                url: prepared.url.clone(),
                headers: vec![("Accept".into(), "application/json, text/plain, */*".into())],
                body: None,
                timeout_ms: Some(5000),
            })?;
            revalidate(ctx, &prepared.source)?;
            if self.try_http.next_nonce != nonce || !self.try_http.safe {
                return Err(Error::Stale);
            }
            if response.body.len() > MAX_BODY {
                return Err(invalid("Response exceeds the 256 KiB acceptance limit (host transfer cap remains 8 MiB)."));
            }
            let body =
                String::from_utf8(response.body).map_err(|_| invalid("Binary response is unsupported by this text-only response view."))?;
            let mut shown = String::new();
            for c in body.chars() {
                if shown.len() + c.len_utf8() > DISPLAY {
                    break;
                }
                shown.push(c);
            }
            let truncated = shown.len() < body.len();
            let text = format!(
                "{} {}\nHTTP {}\nSource: {} (version {})\nTicket: {nonce}\n\n{}{}",
                prepared.operation.method,
                prepared.url,
                response.status,
                prepared.source.path,
                prepared.source.version,
                shown,
                if truncated { "\n[Display truncated at 64 KiB]" } else { "" }
            );
            self.try_http.response = Some(Reply { source: prepared.source.clone(), text });
            Ok(())
        })();
        if let Err(error) = &result {
            if self.try_http.next_nonce == nonce && self.try_http.safe && revalidate(ctx, &prepared.source).is_ok() {
                self.try_http.outcome = Some(Outcome { source: prepared.source, reason: failure_reason(error) });
            }
        }
        // Only a fresh render may expose the captured reply/outcome in a matching scope.
        // Do not reveal/steal focus into a project the user switched to in flight.
        crate::ui_group::refresh_view("try-response");
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{
        HttpResponse, Plugin,
        mock::MockHost,
        providers::{Document, Providers},
        views::{ViewEvent, Views},
    };
    const SPEC: &str = r#"{"openapi":"3.1.0","info":{"title":"Owned test","version":"1"},"servers":[{"url":"http://127.0.0.1:32109/api"}],"paths":{"/pets/{id}":{"get":{"parameters":[{"in":"path","name":"id","required":true,"schema":{"type":"string"}},{"in":"query","name":"q","schema":{"type":"string"}}],"responses":{"200":{"description":"OK"}}}}}}"#;
    fn setup() -> (MockHost, OpenApi) {
        let h = MockHost::install().with_capabilities(&["project.read", "editor.read", "events", "net:127.0.0.1"]);
        h.open_editor("/project/openapi.json", "json", SPEC, &[]);
        let mut p = OpenApi::default();
        p.activate(&Context { api_version: "1.4".into(), capabilities: vec!["net:127.0.0.1".into()], ..Context::default() }).unwrap();
        (h, p)
    }
    fn answers(h: &MockHost) {
        h.answer_next_prompt(Some(&["0"]));
        h.answer_next_prompt(Some(&["snow ☃/pet"]));
        h.answer_next_prompt(Some(&["x&y=z"]));
    }
    fn prepare(h: &MockHost, p: &mut OpenApi) -> String {
        answers(h);
        p.run_command("try-local-read", &h.invocation(), None).unwrap();
        format!("send:{}", p.try_http.preview.as_ref().unwrap().nonce)
    }
    fn send(h: &MockHost, p: &mut OpenApi, key: &str) -> Result<(), Error> {
        p.handle("try-response", ViewEvent::Button(key.into()), &h.invocation())
    }
    #[test]
    fn preparation_is_readonly_and_fresh_send_fetches_exact_encoded_request_once() {
        let (h, mut p) = setup();
        h.on_fetch(|r| {
            assert_eq!(r.method, "GET");
            assert_eq!(r.url, "http://127.0.0.1:32109/api/pets/snow%20%E2%98%83%2Fpet?q=x%26y%3Dz");
            assert_eq!(r.timeout_ms, Some(5000));
            assert!(r.body.is_none());
            assert_eq!(r.headers, [("Accept".into(), "application/json, text/plain, */*".into())]);
            Ok(HttpResponse {
                status: 404,
                headers: vec![("Set-Cookie".into(), "hidden=test".into())],
                body: b"<script>text only</script>".to_vec(),
            })
        });
        let key = prepare(&h, &mut p);
        assert!(h.requests().is_empty());
        let view = p.render("try-response", None, &h.invocation()).unwrap();
        assert_eq!(view.toolbar[0].id, key);
        let Body::Form(fields) = view.body else { panic!() };
        let FieldValue::Other { kind, data } = &fields[0].value else { panic!() };
        assert_eq!(kind, "readonly-text");
        assert!(data.contains("Not sent") || data.contains("has not sent anything"));
        assert!(data.contains("%E2%98%83"));
        send(&h, &mut p, &key).unwrap();
        assert_eq!(h.requests().len(), 1);
        assert!(send(&h, &mut p, &key).is_err());
        assert_eq!(h.requests().len(), 1);
        let Body::Form(fields) = p.render("try-response", None, &h.invocation()).unwrap().body else { panic!() };
        let FieldValue::Other { data, .. } = &fields[0].value else { panic!() };
        assert!(data.contains("HTTP 404") && data.contains("<script>text only</script>"));
        assert!(!data.contains("hidden=test"));
    }
    #[test]
    fn grouped_request_response_preserves_readonly_review_exact_send_and_hidden_tab_guards() {
        let (h, mut p) = setup();
        h.on_fetch(|_| Ok(HttpResponse {status:200,body:b"response body".to_vec(),..Default::default()}));
        let key = prepare(&h, &mut p);
        assert_eq!(h.revealed_views().last().unwrap(), crate::ENDPOINTS);
        let request = |p: &mut OpenApi, op: serde_json::Value| {
            let mut op=op;op["channel"]=serde_json::json!("ui-kit");op["sequence"]=serde_json::json!(1);op["project"]=serde_json::json!("/project");
            p.handle(crate::ENDPOINTS, ViewEvent::Message(op.to_string()), &h.invocation()).unwrap();
            serde_json::from_str::<serde_json::Value>(&h.view_messages().last().unwrap().1).unwrap()
        };
        let initial=request(&mut p,serde_json::json!({"op":"render"}));
        assert_eq!(initial["model"]["activeView"],"try-response");
        assert_eq!(initial["model"]["viewTabs"].as_array().unwrap().len(),3);
        assert!(initial["model"]["body"].to_string().contains("readonly-text"));
        assert!(h.requests().is_empty());
        let switched=request(&mut p,serde_json::json!({"op":"select-view","target":"endpoints","targetView":"try-response"}));
        assert_eq!(switched["model"]["activeView"],"endpoints");
        let stale=request(&mut p,serde_json::json!({"op":"event","targetView":"try-response","event":{"kind":"button","key":key}}));
        assert!(stale.get("error").is_some());assert!(h.requests().is_empty());
        request(&mut p,serde_json::json!({"op":"select-view","target":"try-response","targetView":"endpoints"}));
        let sent=request(&mut p,serde_json::json!({"op":"event","targetView":"try-response","event":{"kind":"button","key":key}}));
        assert!(sent.get("error").is_none(),"{sent}");
        assert_eq!(h.requests().len(),1);assert!(sent["model"]["body"].to_string().contains("response body"));
        let replay=request(&mut p,serde_json::json!({"op":"event","targetView":"try-response","event":{"kind":"button","key":key}}));
        assert!(replay.get("error").is_some());assert_eq!(h.requests().len(),1);
    }

    #[test]
    fn try_it_refuses_private_hosts() {
        for u in ["https://169.254.169.254", "https://10.0.0.5/v1", "https://localhost", "https://intranet", "https://localhost:8443", "https://2130706433", "https://evil.localhost", "https://a.b.localhost", "https://0x7f.0.0.0x1", "https://127.0.0.0x1", "https://localhost.", "https://[::1]", "https://metadata.google.internal", "https://printer.local", "https://x.home.arpa"] {
            assert!(literal_base(u, true).is_err(), "{u}");
        }
        assert!(literal_base("https://api.example.com", true).is_ok());
    }

    #[test]
    fn net_ask_allows_https_servers_and_refusal_shows_no_response() {
        let remote = SPEC.replace("http://127.0.0.1:32109/api", "https://api.example.test/v1");
        assert!(literal_base("https://api.example.test:8443/v1", true).is_ok());
        assert!(literal_base("https://api.example.test:8443/v1", false).is_err());
        for u in ["https://user@api.example.test", "https://API.example.test", "https://a..b", "https://[::1]:1", "https://x:0", "http://api.example.test"] {
            assert!(literal_base(u, true).is_err(), "{u}");
        }
        let h = MockHost::install().with_capabilities(&["project.read", "editor.read", "events", "net:ask"]);
        h.open_editor("/project/openapi.json", "json", &remote, &[]);
        let mut p = OpenApi::default();
        p.activate(&Context { api_version: "1.5".into(), capabilities: vec!["net:ask".into()], ..Context::default() }).unwrap();
        h.on_fetch(|r| {
            assert!(r.url.starts_with("https://api.example.test/v1/pets/"));
            Err(Error::Denied("net:api.example.test".into()))
        });
        let key = prepare(&h, &mut p);
        assert!(h.requests().is_empty());
        assert!(send(&h, &mut p, &key).is_err());
        let Body::Message(text) = p.render("try-response", None, &h.invocation()).unwrap().body else { panic!() };
        assert!(text.contains("denied"));
    }
    #[test]
    fn try_it_out_denied_names_permission() {
        let remote = SPEC.replace("http://127.0.0.1:32109/api", "https://api.example.test/v1");
        let h = MockHost::install().with_capabilities(&["project.read", "editor.read", "events", "net:ask"]);
        h.open_editor("/project/openapi.json", "json", &remote, &[]);
        let mut p = OpenApi::default();
        p.activate(&Context { api_version: "1.5".into(), capabilities: vec!["net:ask".into()], ..Context::default() }).unwrap();
        h.on_fetch(|_| Err(Error::Denied("net:api.example.test".into())));
        let key = prepare(&h, &mut p);
        assert!(send(&h, &mut p, &key).is_err());
        let Body::Message(text) = p.render("try-response", None, &h.invocation()).unwrap().body else { panic!() };
        assert!(text.contains("needs the net:127.0.0.1 permission"), "{text}");
        assert!(text.contains("net:ask"), "{text}");
        assert!(text.contains("Settings › Plugins › OpenAPI"), "{text}");
    }
    #[test]
    fn api13_default_denied_or_broader_grants_refuse_before_fetch() {
        let (h, mut p) = setup();
        for api in ["1.3", "1.2", "", "2.0", "bogus", "1.+4", "1.4.", "1.4.preview", "1.4.0.extra"] {
            p.activate(&Context { api_version: api.into(), capabilities: vec!["net:127.0.0.1".into()], ..Context::default() }).unwrap();
            assert!(p.try_local(&h.invocation()).is_err());
            assert!(p.send_prepared(&h.invocation(), "send:1").is_err());
        }
        assert!(OpenApi::default().try_local(&h.invocation()).is_err());
        for cap in ["net:*", "net:localhost", "net.http:127.0.0.1"] {
            p.activate(&Context {
                api_version: "1.4".into(),
                capabilities: vec!["net:127.0.0.1".into(), cap.into()],
                ..Context::default()
            })
            .unwrap();
            assert!(p.try_local(&h.invocation()).is_err());
        }
        p.activate(&Context { api_version: "1.4".into(), ..Context::default() }).unwrap();
        let error = p.try_local(&h.invocation()).unwrap_err().to_string();
        assert!(error.contains("needs the net:127.0.0.1 permission"), "{error}");
        assert!(h.requests().is_empty());
    }
    #[test]
    fn cancelled_preparation_and_no_send_never_execute() {
        for stage in 0..3 {
            let (h, mut p) = setup();
            if stage > 0 {
                h.answer_next_prompt(Some(&["0"]));
            }
            if stage > 1 {
                h.answer_next_prompt(Some(&["id"]));
            }
            assert_eq!(p.run_command("try-local-read", &h.invocation(), None).unwrap(), None);
            assert!(p.try_http.pending.is_none());
            assert!(p.try_http.preview.is_none());
            assert!(h.requests().is_empty());
        }
        let (h, mut p) = setup();
        prepare(&h, &mut p);
        assert!(h.requests().is_empty());
    }
    #[test]
    fn fresh_send_rejects_project_or_editor_switch_even_with_old_project_still_open() {
        for kind in 0..3 {
            let (h, mut p) = setup();
            let key = prepare(&h, &mut p);
            match kind {
                0 => {
                    h.add_project("/project");
                    h.set_project_root(Some("/other"));
                    h.open_editor("/other/openapi.json", "json", SPEC, &[]);
                }
                1 => h.open_editor("/project/other.json", "json", SPEC, &[]),
                _ => h.open_editor("/project/openapi.json", "json", &SPEC.replace("32109", "32110"), &[]),
            }
            assert!(matches!(p.render("try-response", None, &h.invocation()).unwrap().body, Body::Message(_)));
            assert!(send(&h, &mut p, &key).is_err());
            assert!(h.requests().is_empty());
            assert!(p.try_http.pending.is_none());
        }
    }
    #[test]
    fn nonce_replay_expiry_reactivation_and_forged_fields_cannot_authorize_requests() {
        let (h, mut p) = setup();
        let key = prepare(&h, &mut p);
        assert!(send(&h, &mut p, "send:999999").is_err());
        assert!(p.try_http.preview.is_some());
        assert!(
            p.handle("try-response", ViewEvent::Field("request".into(), "http://evil.test".into()), &h.invocation())
                .is_err()
        );
        p.try_http.preview.as_mut().unwrap().at = Instant::now() - Duration::from_secs(121);
        assert!(send(&h, &mut p, &key).is_err());
        assert!(h.requests().is_empty());
        let key = prepare(&h, &mut p);
        p.try_http.activate(&Context { api_version: "1.4".into(), capabilities: vec!["net:127.0.0.1".into()], ..Context::default() });
        assert!(send(&h, &mut p, &key).is_err());
        assert!(h.requests().is_empty());
    }
    #[test]
    fn host_url_versions_auth_overrides_and_duplicate_members_are_bounded() {
        for u in [
            "http://localhost:32109",
            "http://127.0.0.1.evil:32109",
            "http://u@127.0.0.1:32109",
            "http://127.0.0.1:0",
            "http://127.0.0.1:08080",
            "http://127.0.0.1:65536",
            "http://127.0.0.1",
            "http://127.0.0.1:32109/a/../b",
            "http://127.0.0.1:32109/a%2f",
            "http://[::1]:32109",
        ] {
            assert!(literal_base(u, false).is_err(), "{u}");
        }
        for source in [
            SPEC.replace("3.1.0", "3.2.0"),
            SPEC.replace("3.1.0", "3.1.invalid"),
            SPEC.replace("\"info\":", "\"openapi\":\"3.0.0\",\"info\":"),
            SPEC.replace("\"paths\":", "\"security\":[{\"bearer\":[]}],\"paths\":"),
            SPEC.replace("\"get\":", "\"post\":"),
            SPEC.replace("\"in\":\"query\"", "\"in\":\"header\""),
        ] {
            assert!(operations(&source, false).is_err());
        }
    }
    #[test]
    fn parameter_membership_encoding_and_scalar_validation_do_not_escape_source_scope() {
        let op = operations(SPEC, false).unwrap().remove(0);
        assert!(url(&op, &["..".into(), "".into()]).is_err());
        assert!(url(&op, &["a\r\nX:y".into(), "".into()]).is_err());
        for source in [
            SPEC.replace("{id}", "{other}"),
            SPEC.replace("{id}", "x{id}"),
            SPEC.replace("\"type\":\"string\"", "\"type\":\"string\",\"enum\":[1]"),
            SPEC.replace("\"type\":\"string\"", "\"type\":\"string\",\"enum\":[]"),
        ] {
            assert!(operations(&source, false).is_err());
        }
        assert_eq!(url(&op, &["%2e%2e".into(), "".into()]).unwrap(), "http://127.0.0.1:32109/api/pets/%252e%252e");
    }
    #[test]
    fn response_error_limits_and_capture_scope_do_not_reuse_or_expose_other_source() {
        for mode in 0..6 {
            let (h, mut p) = setup();
            h.on_fetch(move |_| match mode {
                0 => Ok(HttpResponse { status: 200, headers: vec![], body: vec![b'x'; MAX_BODY + 1] }),
                1 => Ok(HttpResponse { status: 200, headers: vec![], body: vec![0xff] }),
                2 => Err(Error::Failed("proxy-secret: owned timeout".into())),
                3 => Err(Error::Cancelled),
                4 => Err(Error::Denied("proxy-secret: redirected host".into())),
                _ => Err(Error::Failed("permission denied: proxy-secret (not a typed denial)".into())),
            });
            let key = prepare(&h, &mut p);
            assert!(send(&h, &mut p, &key).is_err());
            assert!(p.try_http.response.is_none() && p.try_http.preview.is_none());
            let reason = p.try_http.outcome.as_ref().unwrap().reason;
            assert!(reason.len() <= 160 && !reason.contains("proxy-secret"));
            assert_eq!(reason.contains("denied by the host"), mode == 4);
            assert_eq!(reason.contains("unsupported"), mode < 2);
            match p.render("try-response", None, &h.invocation()).unwrap().body {
                Body::Message(message) => assert_eq!(message, reason),
                _ => panic!("failed Send must retain a safe visible outcome"),
            }
            assert_eq!(h.requests().len(), 1);
        }
        let (h, mut p) = setup();
        h.on_fetch(|_| Ok(HttpResponse { status: 200, headers: vec![], body: vec![b'x'; DISPLAY + 10] }));
        let key = prepare(&h, &mut p);
        send(&h, &mut p, &key).unwrap();
        assert!(p.try_http.response.as_ref().unwrap().text.contains("Display truncated"));
        h.open_editor("/project/other.json", "json", SPEC, &[]);
        assert!(matches!(p.try_http.view(&h.invocation()).body, Body::Message(_)));
        h.set_project_root(Some("/other"));
        assert!(matches!(p.try_http.view(&h.invocation()).body, Body::Message(_)));
    }
    #[test]
    fn head_is_literal_and_render_or_typing_cannot_execute() {
        let (h, mut p) = setup();
        h.open_editor("/project/openapi.json", "json", &SPEC.replace("\"get\":", "\"head\":"), &[]);
        h.on_fetch(|r| {
            assert_eq!(r.method, "HEAD");
            Ok(HttpResponse { status: 204, headers: vec![], body: vec![] })
        });
        let doc = Document { path: "/project/openapi.json".into(), language: "json".into(), text: SPEC.into(), version: 2 };
        p.completion(&doc, openidea_plugin::Position::new(0, 1), None);
        p.render("try-response", None, &h.invocation()).unwrap();
        assert!(h.requests().is_empty());
        let key = prepare(&h, &mut p);
        assert!(h.requests().is_empty());
        send(&h, &mut p, &key).unwrap();
        assert_eq!(h.requests().len(), 1);
    }
    #[test]
    fn failed_send_outcome_hides_after_editor_root_text_or_version_drift() {
        for drift in 0..4 {
            let (h, mut p) = setup();
            h.on_fetch(|_| Err(Error::Failed("private-proxy-data".into())));
            let key = prepare(&h, &mut p);
            assert!(send(&h, &mut p, &key).is_err());
            let reason = p.try_http.outcome.as_ref().unwrap().reason;
            match drift {
                0 => h.open_editor("/project/other.json", "json", SPEC, &[]),
                1 => { h.add_project("/project"); h.set_project_root(Some("/other")); },
                2 => h.type_in_editor(""), // Same bytes, newer source version.
                _ => h.type_in_editor(" "),
            }
            match p.render("try-response", None, &h.invocation()).unwrap().body {
                Body::Message(message) => {
                    assert_ne!(message, reason);
                    assert!(!message.contains("private-proxy-data"));
                    assert!(message.contains("another project/editor/source version"));
                }
                _ => panic!("foreign/stale scope must not expose a retained outcome"),
            }
            assert_eq!(h.requests().len(), 1);
        }
    }
    #[test]
    fn failure_replaces_old_success_preserves_foreign_controls_and_resets_on_prepare_or_activate() {
        let (h, mut p) = setup();
        h.on_fetch(|_| Ok(HttpResponse { status: 200, headers: vec![], body: b"old-success-body".to_vec() }));
        let old_key = prepare(&h, &mut p);
        send(&h, &mut p, &old_key).unwrap();
        assert!(p.try_http.response.as_ref().unwrap().text.contains("old-success-body"));
        h.on_fetch(|_| Err(Error::Failed("private-proxy-data".into())));
        let key = prepare(&h, &mut p);
        assert!(send(&h, &mut p, &old_key).is_err());
        assert!(p.try_http.preview.is_some() && p.try_http.outcome.is_none());
        assert!(send(&h, &mut p, &key).is_err());
        assert!(p.try_http.response.is_none());
        let reason = p.try_http.outcome.as_ref().unwrap().reason;
        assert!(send(&h, &mut p, "send:unknown").is_err());
        assert!(p.handle("try-response", ViewEvent::Field("request".into(), "foreign control".into()), &h.invocation()).is_err());
        assert_eq!(p.try_http.outcome.as_ref().unwrap().reason, reason);
        match p.render("try-response", None, &h.invocation()).unwrap().body {
            Body::Message(message) => {
                assert_eq!(message, reason);
                assert!(!message.contains("old-success-body") && !message.contains("HTTP 200") && !message.contains("private-proxy-data"));
            }
            _ => panic!("failed request must not retain a successful body"),
        }
        assert_eq!(h.requests().len(), 2);
        assert_eq!(p.run_command("try-local-read", &h.invocation(), None).unwrap(), None); // Cancel at the picker.
        assert!(p.try_http.outcome.is_none() && p.try_http.pending.is_none());
        let key = prepare(&h, &mut p);
        assert!(send(&h, &mut p, &key).is_err());
        assert!(p.try_http.outcome.is_some());
        p.activate(&Context { api_version: "1.4".into(), capabilities: vec!["net:127.0.0.1".into()], ..Context::default() }).unwrap();
        assert!(p.try_http.outcome.is_none() && p.try_http.response.is_none());
        assert_eq!(h.requests().len(), 3);
    }
    const YAML_SPEC: &str = "openapi: 3.1.0\ninfo:\n  title: Owned test\n  version: '1'\nservers:\n  - url: http://127.0.0.1:32109/api\npaths:\n  /pets/{id}:\n    get:\n      parameters:\n        - in: path\n          name: id\n          required: true\n          schema:\n            type: string\n        - in: query\n          name: q\n          schema: {type: string}\n      responses:\n        200:\n          description: OK\n";
    fn model(text: &str, ask: bool) -> Result<String, String> {
        operations(text, ask).map(|ops| format!("{ops:?}")).map_err(|e| e.to_string())
    }
    #[test]
    fn yaml_spec_yields_the_json_request_model_and_sends_the_same_request() {
        assert_eq!(model(YAML_SPEC, false).unwrap(), model(SPEC, false).unwrap());
        let sent = |path: &str, language: &str, text: &str| {
            let (h, mut p) = setup();
            h.open_editor(path, language, text, &[]);
            h.on_fetch(|_| Ok(HttpResponse { status: 200, headers: vec![], body: b"ok".to_vec() }));
            let key = prepare(&h, &mut p);
            assert!(h.requests().is_empty());
            send(&h, &mut p, &key).unwrap();
            let r = h.requests().remove(0);
            (r.method, r.url, r.headers, r.body, r.timeout_ms)
        };
        let json = sent("/project/openapi.json", "json", SPEC);
        assert_eq!(json, sent("/project/openapi.yaml", "yaml", YAML_SPEC));
        assert_eq!(json, sent("/project/api.yml", "yaml", YAML_SPEC));
        for (path, language) in [("/project/openapi.yaml", "json"), ("/project/openapi.json", "yaml"), ("/project/openapi.txt", "yaml")] {
            let (h, mut p) = setup();
            h.open_editor(path, language, YAML_SPEC, &[]);
            assert!(p.try_local(&h.invocation()).is_err(), "{path} {language}");
            assert!(p.try_http.preview.is_none() && h.requests().is_empty());
        }
    }
    #[test]
    fn yaml_and_json_refuse_the_same_specs() {
        // JSON is YAML flow syntax: a leading comment routes the very same document through the YAML loader.
        let as_yaml = |json: &str| format!("# yaml\n{json}");
        let remote = |server: &str| SPEC.replace("http://127.0.0.1:32109/api", server);
        let mut variants = vec![
            SPEC.into(),
            SPEC.replace("3.1.0", "3.2.0"),
            SPEC.replace("3.1.0", "3.1.invalid"),
            SPEC.replace("\"3.1.0\"", "3.1"),
            SPEC.replace("\"info\":", "\"openapi\":\"3.0.0\",\"info\":"),
            SPEC.replace("\"paths\":", "\"security\":[{\"bearer\":[]}],\"paths\":"),
            SPEC.replace("\"get\":", "\"post\":"),
            SPEC.replace("\"get\":", "\"head\":"),
            SPEC.replace("\"in\":\"query\"", "\"in\":\"header\""),
            SPEC.replace("{id}", "{other}"),
            SPEC.replace("{id}", "x{id}"),
            SPEC.replace("\"required\":true", "\"required\":\"true\""),
            SPEC.replace("\"type\":\"string\"", "\"type\":\"string\",\"enum\":[1]"),
            SPEC.replace("\"type\":\"string\"", "\"type\":\"string\",\"enum\":[]"),
            SPEC.replace("\"type\":\"string\"", "\"type\":\"integer\",\"enum\":[1,2.5]"),
            SPEC.replace("\"type\":\"string\"", "\"type\":\"number\",\"enum\":[1,2.5]"),
            SPEC.replace("\"url\":", "\"variables\":{},\"url\":"),
            SPEC.replace("\"servers\":[{", "\"servers\":[{\"url\":\"http://127.0.0.1:1\"},{"),
        ];
        for server in [
            "https://api.example.test/v1", "https://169.254.169.254", "https://10.0.0.5/v1", "https://localhost", "https://intranet",
            "https://localhost:8443", "https://2130706433", "https://evil.localhost", "https://0x7f.0.0.0x1", "https://[::1]",
            "https://metadata.google.internal", "https://printer.local", "https://x.home.arpa", "https://user@api.example.test",
            "http://api.example.test", "http://localhost:32109", "http://127.0.0.1.evil:32109", "http://127.0.0.1:0",
            "http://127.0.0.1:32109/a/../b", "http://127.0.0.1:32109/a%2f",
        ] {
            variants.push(remote(server));
        }
        let mut accepted = 0;
        for json in &variants {
            for ask in [false, true] {
                let expected = model(json, ask).ok();
                assert_eq!(model(&as_yaml(json), ask).ok(), expected, "{json} ask={ask}");
                accepted += usize::from(expected.is_some());
            }
        }
        assert!(accepted >= 5, "the parity cases must include accepted specs");
        let dup_json = SPEC.replace("\"info\":", "\"servers\":[{\"url\":\"https://api.example.test\"}],\"info\":");
        assert!(model(&dup_json, true).is_err());
        assert!(model(&as_yaml(&dup_json), true).is_err());
        for yaml in [
            YAML_SPEC.replace("openapi: 3.1.0", "openapi: 3.1.0\nopenapi: 3.0.0"),
            YAML_SPEC.replace("        200:", "        200:\n          description: OK\n        '200':"),
            YAML_SPEC.replace("servers:\n  - url:", "servers:\n  - &s\n    url:"),
            YAML_SPEC.replace("            type: string", "            type: !!str string"),
            YAML_SPEC.replace("info:", "x: &a {}\ny: *a\ninfo:"),
            YAML_SPEC.replace("            type: string", "            <<: {type: string}"),
            format!("{YAML_SPEC}---\n{YAML_SPEC}"),
            YAML_SPEC.replace("version: '1'", "version: '1'\n  1.5: real key"),
            YAML_SPEC.replace("openapi: 3.1.0", "openapi: 3.1.0\nx-pad: '{}'", ).replace("{}", &"x".repeat(MAX_SPEC)),
        ] {
            assert!(operations(&yaml, true).is_err(), "{yaml}");
        }
    }
}
