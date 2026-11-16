//! DDEV's JSON log envelopes and the fields this plugin uses.
use openidea_plugin::Error;
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Service {
    pub name: String,
    pub url: Option<String>,
    /// Synthetic Mailpit/Mailhog links do not name a compose service.
    pub logs: bool,
    pub info: Value,
}

#[derive(Clone, Debug)]
pub struct Description {
    pub name: String,
    pub status: String,
    pub url: Option<String>,
    pub services: Vec<Service>,
    pub raw: Value,
}

/// JSON log records can precede the result (including debug output).
pub fn raw_output(stdout: &str) -> Result<Value, Error> {
    if let Ok(value) = serde_json::from_str::<Value>(stdout)
        && let Some(raw) = value.get("raw").filter(|r| r.is_object())
    {
        return Ok(raw.clone());
    }
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|v| v.get("raw").filter(|r| r.is_object()).cloned())
        .ok_or_else(|| "DDEV returned no JSON result with an object in 'raw'.".into())
}

/// Match upstream DdevErrorOutput: the last error/fatal record wins, and
/// informational JSON records are not presented as an error message.
pub fn error_message(stderr: &str) -> Option<String> {
    let mut message = None;
    let mut plain = vec![];
    for line in stderr.lines().map(str::trim).filter(|s| !s.is_empty()) {
        match serde_json::from_str::<Value>(line)
            .ok()
            .filter(Value::is_object)
        {
            Some(record) => {
                if matches!(text(&record, "level"), "error" | "fatal")
                    && let Some(value) = record.get("msg").and_then(Value::as_str)
                {
                    message = Some(value.to_owned());
                }
            }
            None => plain.push(line),
        }
    }
    message
        .or_else(|| (!plain.is_empty()).then(|| plain.join("\n")))
        .map(|text| text.chars().take(2000).collect())
}

pub fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

pub fn safe_name(name: &str) -> bool {
    name.as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        && safe_service_name(name)
}

/// Compose service identifiers allow leading '.', '_' and '-'. They remain
/// a single option value when passed with --service=<name>.
pub fn safe_service_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}

fn url(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        let value = text(v, key);
        let parsed = url::Url::parse(value).ok()?;
        (matches!(parsed.scheme(), "https" | "http")
            && parsed.host_str().is_some()
            && parsed.username().is_empty()
            && parsed.password().is_none())
        .then(|| value.to_owned())
    })
}

impl Description {
    pub fn parse(stdout: &str) -> Result<Self, Error> {
        let raw = raw_output(stdout)?;
        if text(&raw, "name").is_empty() || text(&raw, "status").is_empty() {
            return Err("DDEV describe returned no project name or status.".into());
        }
        let mut services: Vec<Service> = raw
            .get("services")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter(|(name, info)| safe_service_name(name) && info.is_object())
            .map(|(name, info)| Service {
                name: name.clone(),
                url: url(info, &["https_url", "http_url"]),
                logs: true,
                info: info.clone(),
            })
            .collect();
        for (name, keys) in [
            (
                "mailpit",
                ["mailpit_https_url", "mailpit_url", "mailpit_http_url"],
            ),
            (
                "mailhog",
                ["mailhog_https_url", "mailhog_url", "mailhog_http_url"],
            ),
        ] {
            if let Some(link) = url(&raw, &keys) {
                if let Some(existing) = services.iter_mut().find(|s| s.name == name) {
                    existing.url = Some(link);
                } else {
                    services.push(Service {
                        name: name.into(),
                        url: Some(link.clone()),
                        logs: false,
                        info: serde_json::json!({"url": link}),
                    });
                }
            }
        }
        services.sort_by(|a, b| a.name.cmp(&b.name));
        let project_url = url(&raw, &["primary_url", "httpsurl", "httpurl"]).or_else(|| {
            services
                .iter()
                .find(|s| s.name == "web")
                .and_then(|s| s.url.clone())
        });
        Ok(Self {
            name: text(&raw, "name").into(),
            status: text(&raw, "status").into(),
            url: project_url,
            services,
            raw,
        })
    }

    pub fn service(&self, key: &str) -> Option<&Service> {
        let name = key.strip_prefix("service:")?;
        self.services.iter().find(|s| s.name == name)
    }
}

pub fn pretty(value: &Value) -> String {
    fn redact(value: &mut Value) {
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    let key = key.to_ascii_lowercase();
                    if key.contains("password") || key.contains("secret") || key.contains("token") {
                        *value = Value::String("[redacted]".into());
                    } else {
                        redact(value);
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(redact),
            _ => {}
        }
    }
    let mut safe = value.clone();
    redact(&mut safe);
    serde_json::to_string_pretty(&safe).unwrap_or_default()
}
