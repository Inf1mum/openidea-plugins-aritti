//! What the running application's Actuator endpoints said, parsed into a
//! small model (Spring Boot 2.x and 3.x JSON shapes) that is kept in memory
//! and in storage.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use spring_text::config::canonical;

/// The endpoints read, in order; the first one tells whether the application is up.
pub const ENDPOINTS: [&str; 9] = ["beans", "env", "conditions", "health", "mappings", "configprops", "scheduledtasks", "loggers", "caches"];
/// What Actuator shows for a value it hides.
pub const MASK: &str = "******";
pub const HEALTH_DEPTH: usize = 4;
pub const HEALTH_NODES: usize = 200;

/// Control characters dropped, length bounded.
fn health_text(s: &str, max: usize) -> String {
    let clean: String = s.chars().filter(|c| !c.is_control()).collect();
    if clean.chars().count() <= max {
        return clean;
    }
    format!("{}…", clean.chars().take(max).collect::<String>())
}

fn sensitive_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    ["password", "passwd", "secret", "token", "key", "credential", "auth"].iter().any(|w| k.contains(w))
}

/// `scheme://user:pw@host` becomes `scheme://******@host`.
fn mask_userinfo(v: &str) -> String {
    let Some(i) = v.find("://") else { return v.to_owned() };
    let rest = &v[i + 3..];
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    match authority.rfind('@') {
        Some(at) => format!("{}{MASK}{}", &v[..i + 3], &rest[at..]),
        None => v.to_owned(),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub url: String,
    /// Seconds since the Unix epoch.
    pub fetched_at: u64,
    /// Endpoints that answered nothing usable, with why (`conditions (404)`).
    pub missing: Vec<String>,
    /// Legacy subset flag; the current persistence writer never trims automatically.
    pub trimmed: bool,
    pub beans: Vec<Bean>,
    pub profiles: Vec<String>,
    /// Property sources, highest precedence first.
    pub sources: Vec<Source>,
    pub conditions: Vec<Condition>,
    /// `""` is the application, `db`, `db/primary`… its components.
    pub health: Vec<(String, String)>,
    /// `(component path, key, value)`: bounded, escaped and masked at parse time. Absent in schema 1 caches.
    #[serde(default)]
    pub health_details: Vec<(String, String, String)>,
    pub mappings: Vec<Mapping>,
    pub configprops: Vec<PropsBean>,
    pub scheduled: Vec<Scheduled>,
    /// Only `ROOT` and the loggers with a configured level.
    pub loggers: Vec<Logger>,
    pub caches: Vec<Cache>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Bean {
    pub context: String,
    pub name: String,
    pub scope: String,
    /// Fully qualified, `$` nesting as `.`, without the CGLIB suffix.
    pub ty: String,
    pub deps: Vec<String>,
    /// True only when the fetched dependency JSON was an array of strings. Legacy caches cannot prove this.
    #[serde(default)]
    pub dependencies_verified: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub name: String,
    pub props: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    /// `com.acme.MailConfig`, `DataSourceAutoConfiguration#dataSource`…
    pub key: String,
    pub matched: bool,
    pub messages: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Mapping {
    /// The dispatcher servlet or handler.
    pub group: String,
    pub predicate: String,
    pub handler: String,
    /// The handler method's class and name, when it is one.
    pub method: Option<(String, String)>,
    /// Exact method/path pairs reported by requestMappingConditions; legacy snapshots have none.
    #[serde(default)]
    pub requests: Vec<(String, String)>,
    #[serde(default)]
    pub examples: crate::request_conditions::RequestExamples,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PropsBean {
    pub prefix: String,
    pub ty: String,
    pub props: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Scheduled {
    /// `cron`, `fixedDelay`, `fixedRate`, `custom`.
    pub kind: String,
    /// `com.acme.Jobs.cleanup`.
    pub target: String,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Logger {
    pub name: String,
    pub configured: String,
    pub effective: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Cache {
    pub manager: String,
    pub name: String,
    pub target: String,
}

fn mapping_requests(conditions: &Value) -> Vec<(String, String)> {
    let Some(methods) = conditions.get("methods").and_then(Value::as_array) else { return Vec::new() };
    let Some(paths) = conditions.get("patterns").and_then(Value::as_array) else { return Vec::new() };
    if methods.is_empty() || paths.is_empty() || methods.len().saturating_mul(paths.len()) > 32 { return Vec::new() }
    let mut result = Vec::new();
    for method in methods {
        let Some(method) = method.as_str().filter(|m| matches!(*m, "GET"|"HEAD"|"POST"|"PUT"|"PATCH"|"DELETE"|"OPTIONS"|"TRACE")) else { return Vec::new() };
        for path in paths {
            let Some(path) = path.as_str().filter(|p| p.starts_with('/') && !p.starts_with("//") && p.len() <= 2048 && !p.chars().any(|c| c.is_control() || c.is_whitespace())) else { return Vec::new() };
            if !result.iter().any(|pair| pair == &(method.to_owned(),path.to_owned())) { result.push((method.into(),path.into())); }
        }
    }
    result
}

fn entries(v: &Value) -> impl Iterator<Item = (&String, &Value)> {
    v.as_object().into_iter().flatten()
}

fn items(v: &Value) -> impl Iterator<Item = &Value> {
    v.as_array().into_iter().flatten()
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn type_name(raw: &str) -> String {
    raw.split("$$").next().unwrap_or(raw).replace('$', ".")
}

/// `{a: {b: 1}, c: [x]}` → `a.b = 1`, `c[0] = x`.
fn flatten(prefix: &str, v: &Value, out: &mut Vec<(String, String)>) {
    match v {
        Value::Object(map) => {
            for (k, v) in map {
                let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                flatten(&key, v, out);
            }
        }
        Value::Array(list) => {
            for (i, v) in list.iter().enumerate() {
                flatten(&format!("{prefix}[{i}]"), v, out);
            }
        }
        other => out.push((prefix.to_owned(), text(other))),
    }
}

impl Snapshot {
    /// Reads one endpoint's JSON.
    pub fn read(&mut self, endpoint: &str, json: &Value) {
        let contexts = || entries(&json["contexts"]);
        match endpoint {
            "beans" => {
                for (context, c) in contexts() {
                    for (name, b) in entries(&c["beans"]) {
                        self.beans.push(Bean {
                            context: context.clone(),
                            name: name.clone(),
                            scope: text(&b["scope"]),
                            ty: type_name(&text(&b["type"])),
                            deps: items(&b["dependencies"]).map(text).collect(),
                            dependencies_verified: b["dependencies"]
                                .as_array()
                                .is_some_and(|dependencies| dependencies.iter().all(Value::is_string)),
                        });
                    }
                }
            }
            "env" => {
                self.profiles = items(&json["activeProfiles"]).map(text).collect();
                for s in items(&json["propertySources"]) {
                    let props = entries(&s["properties"]).map(|(k, p)| (k.clone(), text(&p["value"]))).collect();
                    self.sources.push(Source { name: text(&s["name"]), props });
                }
            }
            "conditions" => {
                let messages = |list: &Value| -> Vec<String> {
                    items(list)
                        .map(|o| {
                            let m = text(&o["message"]);
                            if m.is_empty() { text(&o["condition"]) } else { m }
                        })
                        .collect()
                };
                for (_, c) in contexts() {
                    for (key, list) in entries(&c["positiveMatches"]) {
                        self.conditions.push(Condition { key: key.clone(), matched: true, messages: messages(list) });
                    }
                    for (key, o) in entries(&c["negativeMatches"]) {
                        self.conditions.push(Condition { key: key.clone(), matched: false, messages: messages(&o["notMatched"]) });
                    }
                }
            }
            "health" => self.read_health("", json),
            "mappings" => {
                for (_, c) in contexts() {
                    let m = &c["mappings"];
                    for (group, list) in entries(&m["dispatcherServlets"]).chain(entries(&m["dispatcherHandlers"])) {
                        for h in items(list) {
                            let method = &h["details"]["handlerMethod"];
                            self.mappings.push(Mapping {
                                examples: crate::request_conditions::read(&h["details"]["requestMappingConditions"]),
                                requests: mapping_requests(&h["details"]["requestMappingConditions"]),
                                group: group.clone(),
                                predicate: text(&h["predicate"]),
                                handler: text(&h["handler"]),
                                method: method["className"].as_str().map(|class| (type_name(class), text(&method["name"]))),
                            });
                        }
                    }
                }
            }
            "configprops" => {
                for (_, c) in contexts() {
                    for (key, b) in entries(&c["beans"]) {
                        let prefix = text(&b["prefix"]);
                        let ty = key.strip_prefix(&format!("{prefix}-")).unwrap_or(key);
                        let mut props = Vec::new();
                        flatten("", &b["properties"], &mut props);
                        self.configprops.push(PropsBean { prefix, ty: type_name(ty), props });
                    }
                }
            }
            "scheduledtasks" => {
                for (kind, list) in entries(json) {
                    for t in items(list) {
                        let detail = match (t.get("expression"), t.get("interval"), t.get("trigger")) {
                            (Some(e), _, _) => text(e),
                            (_, Some(i), _) => format!("every {} ms", text(i)),
                            (_, _, Some(t)) => text(t),
                            _ => String::new(),
                        };
                        self.scheduled.push(Scheduled { kind: kind.clone(), target: text(&t["runnable"]["target"]), detail });
                    }
                }
            }
            "loggers" => {
                for (name, l) in entries(&json["loggers"]) {
                    let configured = text(&l["configuredLevel"]);
                    if name == "ROOT" || !configured.is_empty() {
                        self.loggers.push(Logger { name: name.clone(), configured, effective: text(&l["effectiveLevel"]) });
                    }
                }
            }
            "caches" => {
                for (manager, m) in entries(&json["cacheManagers"]) {
                    for (name, c) in entries(&m["caches"]) {
                        self.caches.push(Cache { manager: manager.clone(), name: name.clone(), target: text(&c["target"]) });
                    }
                }
            }
            _ => {}
        }
    }

    /// Boot 2.2+ nests `components`; older versions put them under `details`.
    /// At most `HEALTH_DEPTH` levels and `HEALTH_NODES` components plus detail rows are kept.
    fn read_health(&mut self, path: &str, v: &Value) {
        let Some(status) = v["status"].as_str() else { return };
        if self.health.len() + self.health_details.len() >= HEALTH_NODES {
            return;
        }
        self.health.push((path.to_owned(), health_text(status, 64)));
        let depth = if path.is_empty() { 0 } else { path.split('/').count() };
        let mut children: Vec<(&String, &Value)> = entries(&v["components"]).collect();
        let is_component = |c: &Value| c.get("status").is_some_and(Value::is_string);
        for (name, c) in entries(&v["details"]) {
            if is_component(c) {
                children.push((name, c));
            } else {
                self.read_health_detail(path, &health_text(name, 64), c, 0);
            }
        }
        if depth >= HEALTH_DEPTH {
            return;
        }
        for (name, c) in children {
            let name = health_text(&name.replace('/', "_"), 64);
            let child = if path.is_empty() { name } else { format!("{path}/{name}") };
            self.read_health(&child, c);
        }
    }

    fn read_health_detail(&mut self, path: &str, key: &str, v: &Value, level: usize) {
        if self.health.len() + self.health_details.len() >= HEALTH_NODES {
            return;
        }
        match v {
            Value::Object(o) if level < 2 => {
                for (k, c) in o {
                    self.read_health_detail(path, &format!("{key}.{}", health_text(k, 64)), c, level + 1);
                }
            }
            Value::Null => {}
            _ => {
                let raw = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let value = if sensitive_key(key) { MASK.to_owned() } else { health_text(&mask_userinfo(&raw), 200) };
                self.health_details.push((path.to_owned(), health_text(key, 128), value));
            }
        }
    }

    /// The value `key` has at runtime and the property source it comes from,
    /// matched with relaxed binding (`SPRING_DATASOURCE_URL` is `spring.datasource.url`).
    pub fn effective(&self, key: &str) -> Option<(&str, &str)> {
        let relaxed = canonical(key);
        let env_form = relaxed.replace(['.', '[', ']'], "");
        self.sources.iter().find_map(|s| {
            let env = s.name.contains("systemEnvironment");
            s.props
                .iter()
                .find(|(k, _)| if env { canonical(k).replace(['.', '[', ']'], "") == env_form } else { canonical(k) == relaxed })
                .map(|(_, v)| (v.as_str(), s.name.as_str()))
        })
    }

    /// Legacy conversion helper. Current persistence stores the full model or rejects it.
    pub fn trimmed(&self) -> Snapshot {
        Snapshot {
            url: self.url.clone(),
            fetched_at: self.fetched_at,
            missing: self.missing.clone(),
            trimmed: true,
            beans: self.beans.clone(),
            profiles: self.profiles.clone(),
            sources: self.sources.clone(),
            ..Snapshot::default()
        }
    }

    /// Whether the snapshot holds what `endpoint` said.
    pub fn has(&self, endpoint: &str) -> bool {
        let kept = !self.trimmed || matches!(endpoint, "beans" | "env");
        kept && !self.missing.iter().any(|m| m.split(' ').next() == Some(endpoint))
    }
}

/// Whether a property source is the configuration file named `file`
/// (`Config resource 'class path resource [application.yml]' via …` in Boot 3,
/// `applicationConfig: [classpath:/application.yml]` in Boot 2).
pub fn is_file_source(source: &str, file: &str) -> bool {
    source.contains(&format!("[{file}]")) || source.contains(&format!("/{file}]"))
}

/// Whether a property source is a configuration file at all.
pub fn is_config_source(source: &str) -> bool {
    source.starts_with("Config resource") || source.starts_with("applicationConfig")
}

/// A property source's name, short enough for a tooltip: a config file's
/// location (`application.yml`), else the name.
pub fn short_source(source: &str) -> &str {
    if is_config_source(source)
        && let Some(open) = source.find('[')
        && let Some(close) = source[open..].find(']')
    {
        return &source[open + 1..open + close];
    }
    source
}

/// Known root/container shapes consumed by the retained adapters. Empty typed
/// collections are observations; an absent/wrong container is not proof of absence.
pub(crate) fn supported_shape(endpoint: &str, json: &Value) -> bool {
    if !json.is_object() {
        return false;
    }
    let contexts = |field: &str| {
        json.get("contexts").and_then(Value::as_object).is_some_and(|contexts| {
            contexts.values().all(|context| context.is_object() && context.get(field).is_some_and(Value::is_object))
        })
    };
    match endpoint {
        "beans" => {
            contexts("beans")
                && json["contexts"].as_object().is_some_and(|contexts| {
                    contexts.values().all(|context| context["beans"].as_object().is_some_and(|beans| beans.values().all(Value::is_object)))
                })
        }
        "env" => json.get("propertySources").and_then(Value::as_array).is_some_and(|sources| {
            sources.iter().all(|source| {
                source.is_object()
                    && source.get("name").is_some_and(Value::is_string)
                    && source.get("properties").is_some_and(Value::is_object)
            })
        }),
        "health" => json.get("status").is_some_and(Value::is_string),
        "conditions" => json.get("contexts").and_then(Value::as_object).is_some_and(|contexts| {
            contexts.values().all(|context| {
                context.is_object()
                    && (context.get("positiveMatches").is_some() || context.get("negativeMatches").is_some())
                    && ["positiveMatches", "negativeMatches"].iter().all(|key| context.get(*key).is_none_or(Value::is_object))
            })
        }),
        "mappings" => contexts("mappings"),
        "configprops" => {
            contexts("beans")
                && json["contexts"].as_object().is_some_and(|contexts| {
                    contexts.values().all(|context| {
                        context["beans"].as_object().is_some_and(|beans| {
                            beans.values().all(|bean| bean.is_object() && bean.get("properties").is_some_and(Value::is_object))
                        })
                    })
                })
        }
        "scheduledtasks" => {
            let keys = ["cron", "fixedDelay", "fixedRate", "custom"];
            keys.iter().any(|key| json.get(*key).is_some())
                && keys
                    .iter()
                    .all(|key| json.get(*key).is_none_or(|value| value.as_array().is_some_and(|tasks| tasks.iter().all(Value::is_object))))
        }
        "loggers" => json.get("loggers").and_then(Value::as_object).is_some_and(|loggers| loggers.values().all(Value::is_object)),
        "caches" => json.get("cacheManagers").and_then(Value::as_object).is_some_and(|managers| {
            managers.values().all(|manager| manager.is_object() && manager.get("caches").is_some_and(Value::is_object))
        }),
        _ => false,
    }
}

#[cfg(test)]
mod health_tests {
    use super::*;
    use serde_json::json;

    fn parse(v: Value) -> Snapshot {
        let mut s = Snapshot::default();
        s.read_health("", &v);
        s
    }

    #[test]
    fn keeps_details_and_masks_secrets_before_storage() {
        let s = parse(json!({"status":"UP","components":{"db":{"status":"UP","details":{
            "database":"PostgreSQL","apiToken":"abc","url":"jdbc:x://bob:pw@h:1/d","pool":{"password":"p","size":3}}}}}));
        let d: std::collections::HashMap<_, _> = s.health_details.iter().map(|(_, k, v)| (k.as_str(), v.as_str())).collect();
        assert_eq!(d["database"], "PostgreSQL");
        assert_eq!(d["apiToken"], MASK);
        assert_eq!(d["pool.password"], MASK);
        assert_eq!(d["pool.size"], "3");
        assert_eq!(d["url"], "jdbc:x://******@h:1/d");
        assert!(!serde_json::to_string(&s).unwrap().contains("pw@"));
    }

    #[test]
    fn legacy_details_children_stay_components() {
        let s = parse(json!({"status":"UP","details":{"db":{"status":"DOWN"},"free":10}}));
        assert_eq!(s.health, vec![("".into(), "UP".into()), ("db".into(), "DOWN".into())]);
        assert_eq!(s.health_details, vec![("".into(), "free".into(), "10".into())]);
    }

    #[test]
    fn depth_nodes_and_value_length_are_bounded() {
        let mut deep = json!({"status":"UP"});
        for _ in 0..8 {
            deep = json!({"status":"UP","components":{"c":deep}});
        }
        assert_eq!(parse(deep).health.len(), HEALTH_DEPTH + 1);
        let wide: serde_json::Map<String, Value> = (0..500).map(|i| (format!("c{i}"), json!({"status":"UP"}))).collect();
        assert_eq!(parse(json!({"status":"UP","components":wide})).health.len(), HEALTH_NODES);
        let s = parse(json!({"status":"UP","details":{"note":"é\u{7}".repeat(500)}}));
        assert!(s.health_details[0].2.chars().count() <= 201 && !s.health_details[0].2.contains('\u{7}'));
    }
}
