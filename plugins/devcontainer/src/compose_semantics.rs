//! Compose Specification checks on a parsed Compose file (spec.md: services, ports, volumes,
//! depends_on, env_file, extends). Only literal values are judged; `$` interpolation is skipped.
use serde_json::{Map, Value};

#[derive(Default)]
pub(crate) struct Facts {
    pub messages: Vec<String>,
    pub paths: Vec<(&'static str, String)>,
    pub depends: Vec<(String, String)>,
}

fn dynamic(s: &str) -> bool {
    s.contains('$')
}

fn port_part_ok(part: &str) -> bool {
    let number = |s: &str| !s.is_empty() && s.len() <= 5 && s.bytes().all(|b| b.is_ascii_digit());
    match part.split_once('-') {
        Some((a, b)) => number(a) && number(b) && a.parse::<u32>().ok() <= b.parse::<u32>().ok(),
        None => number(part),
    }
}

/// `[[IP:]HOST:]CONTAINER[/PROTOCOL]`; quiet for IPv6 forms.
fn port_ok(spec: &str) -> bool {
    if dynamic(spec) || spec.contains('[') {
        return true;
    }
    let (ports, protocol) = match spec.split_once('/') {
        Some((p, proto)) => (p, Some(proto)),
        None => (spec, None),
    };
    if protocol == Some("") {
        return false;
    }
    let parts: Vec<&str> = ports.split(':').collect();
    match parts.as_slice() {
        [container] => port_part_ok(container),
        [host, container] => port_part_ok(host) && port_part_ok(container),
        [_ip, host, container] => (host.is_empty() || port_part_ok(host)) && port_part_ok(container),
        _ => true,
    }
}

fn volume_ok(spec: &str) -> bool {
    if dynamic(spec) || spec.contains('\\') || spec.as_bytes().get(1) == Some(&b':') {
        return true;
    }
    let parts: Vec<&str> = spec.split(':').collect();
    match parts.as_slice() {
        [_] => true,
        [source, target] | [source, target, _] => !source.is_empty() && target.starts_with('/'),
        _ => false,
    }
}

fn service(name: &str, def: &Map<String, Value>, sole_file: bool, facts: &mut Facts) {
    let mut say = |m: String| facts.messages.push(format!("service `{name}` {m}"));
    let borrowed = def.contains_key("extends") || def.contains_key("provider");
    if sole_file && !borrowed && !def.contains_key("image") && !def.contains_key("build") {
        say("declares neither `image` nor `build`.".into());
    }
    for item in def.get("ports").and_then(Value::as_array).into_iter().flatten() {
        match item {
            Value::String(s) if !port_ok(s) => say(format!("has the invalid `ports` entry `{s}`; expected [[IP:]HOST:]CONTAINER[/PROTOCOL].")),
            Value::Object(o) if !o.contains_key("target") => say("has a long-syntax `ports` entry without `target`.".into()),
            Value::Bool(_) | Value::Array(_) | Value::Null => say("has a `ports` entry that is neither a string, number nor mapping.".into()),
            _ => {}
        }
    }
    for item in def.get("volumes").and_then(Value::as_array).into_iter().flatten() {
        match item {
            Value::String(s) if !volume_ok(s) => say(format!("has the invalid `volumes` entry `{s}`; expected VOLUME:CONTAINER_PATH[:ACCESS_MODE] with an absolute container path.")),
            Value::Object(o) if !o.contains_key("target") => say("has a long-syntax `volumes` entry without `target`.".into()),
            Value::String(_) | Value::Object(_) => {}
            _ => say("has a `volumes` entry that is neither a string nor a mapping.".into()),
        }
    }
    match def.get("depends_on") {
        Some(Value::Array(a)) => facts.depends.extend(a.iter().filter_map(Value::as_str).filter(|s| !dynamic(s)).map(|t| (name.into(), t.into()))),
        Some(Value::Object(o)) => facts.depends.extend(o.iter().filter(|(_, v)| v.get("required") != Some(&Value::Bool(false))).filter(|(k, _)| !dynamic(k)).map(|(k, _)| (name.into(), k.clone()))),
        _ => {}
    }
    match def.get("env_file") {
        Some(Value::String(s)) => facts.paths.push(("env_file", s.clone())),
        Some(Value::Array(a)) => {
            for item in a {
                match item {
                    Value::String(s) => facts.paths.push(("env_file", s.clone())),
                    Value::Object(o) if o.get("required") != Some(&Value::Bool(false)) => {
                        if let Some(Value::String(p)) = o.get("path") {
                            facts.paths.push(("env_file", p.clone()));
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    if let Some(Value::String(file)) = def.get("extends").and_then(|e| e.get("file")) {
        facts.paths.push(("extends.file", file.clone()));
    }
}

/// `sole_file`: the Compose file is not combined with others, so a service without `image`/`build` is provably incomplete.
pub(crate) fn inspect(root: &Value, sole_file: bool) -> Facts {
    let mut facts = Facts::default();
    for (name, def) in root.get("services").and_then(Value::as_object).into_iter().flatten() {
        if let Some(def) = def.as_object() {
            service(name, def, sole_file, &mut facts);
        }
    }
    facts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(yaml: &str, sole: bool) -> Facts {
        inspect(&crate::compose::parse_source("c.yaml", yaml).unwrap(), sole)
    }

    #[test]
    fn spec_example_ports_are_quiet() {
        let ports = ["3000", "3000-3005", "8000:8000", "9090-9091:8080-8081", "8000-9000:80", "127.0.0.1:8001:8001", "127.0.0.1:5000-5010:5000-5010", "::1:6000:6000", "[::1]:6001:6001", "6060:6060/udp", "127.0.0.1:8080:80/tcp", "127.0.0.1::80", "${PORT}:80"];
        let yaml = format!("services:\n  a:\n    image: x\n    ports:\n{}", ports.map(|p| format!("      - \"{p}\"\n")).concat());
        assert!(run(&yaml, true).messages.is_empty());
    }

    #[test]
    fn malformed_ports_volumes_and_missing_image_warn() {
        let facts = run("services:\n  a:\n    ports: [\"80:\", \"9000-8000\", \"a:80\", \"80/\"]\n    volumes: [\"data:app\", \"./x:/x:ro\", \"vol:/b:ro:z\"]\n", true);
        assert_eq!(facts.messages.len(), 7);
        assert!(run("services:\n  a:\n    image: x\n  b:\n    build: .\n  c:\n    extends: {service: a}\n", true).messages.is_empty());
        assert!(run("services:\n  a:\n    ports: [80]\n", false).messages.is_empty());
    }

    #[test]
    fn collects_depends_on_env_file_and_extends_file() {
        let facts = run("services:\n  a:\n    image: x\n    depends_on: [db]\n    env_file:\n      - .env\n      - path: opt.env\n        required: false\n    extends:\n      file: base.yml\n      service: s\n  b:\n    image: x\n    depends_on:\n      cache:\n        condition: service_healthy\n", true);
        assert_eq!(facts.depends, [("a".into(), "db".into()), ("b".into(), "cache".into())]);
        assert_eq!(facts.paths, [("env_file", ".env".to_owned()), ("extends.file", "base.yml".to_owned())]);
    }
}
