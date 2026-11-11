//! Binds lockfile/resolved coordinates back to the manifest line that declares them.
//! Only exact-name literal declarations are bound; aliases and repeated declarations are refused.
use crate::{Declaration, Kind, Row, kind, manifest, npm_graph};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};

pub(crate) const MAX_NODES: usize = 2000;
pub(crate) const MAX_DEPTH: usize = 6;
const NPM_SECTIONS: [&str; 4] = ["dependencies", "devDependencies", "optionalDependencies", "peerDependencies"];

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Decl {
    Direct(u32),
    Absent,
    Refused(&'static str),
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn line_of(text: &str, at: usize) -> u32 {
    text[..at.min(text.len())].bytes().filter(|b| *b == b'\n').count() as u32
}

fn one(lines: Vec<u32>) -> Decl {
    match lines.as_slice() {
        [] => Decl::Absent,
        [line] => Decl::Direct(*line),
        _ => Decl::Refused("declared more than once"),
    }
}

fn npm(text: &str, name: &str) -> Decl {
    let Some(root) = manifest::object_members(text, 0) else { return Decl::Refused("manifest not parseable") };
    let mut hits = Vec::new();
    for (section, _, at) in root.iter().filter(|(k, _, _)| NPM_SECTIONS.contains(&k.as_str())) {
        let Some(entries) = manifest::object_members(text, *at) else { return Decl::Refused("manifest not parseable") };
        let values: Option<Value> = serde_json::from_str(&text[*at..]).ok().or_else(|| {
            let mut it = serde_json::Deserializer::from_str(&text[*at..]).into_iter::<Value>();
            it.next()?.ok()
        });
        for (key, span, _) in entries {
            let spec = values.as_ref().and_then(|v| v.get(&key)).and_then(Value::as_str).unwrap_or("");
            if let Some(target) = spec.strip_prefix("npm:") {
                let real = target.rsplit_once('@').map_or(target, |(n, _)| n);
                if key == name || real == name {
                    return Decl::Refused("npm: alias");
                }
                continue;
            }
            if key == name {
                hits.push(line_of(text, span.0));
            }
        }
        let _ = section;
    }
    one(hits)
}

fn pep508_name(s: &str) -> Option<&str> {
    let end = s.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))).unwrap_or(s.len());
    let rest = s[end..].trim_start();
    (end > 0 && (rest.is_empty() || rest.starts_with(['<', '>', '=', '!', '~', ';', '[', '(', '@', ','])))
        .then(|| &s[..end])
}

fn pyproject(text: &str, name: &str) -> Decl {
    let want = manifest::normalise_pypi(name);
    let mut hits = Vec::new();
    let (mut table, mut in_array) = (String::new(), false);
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.starts_with('[') && !in_array {
            table = line.trim_matches(['[', ']']).trim().to_owned();
            continue;
        }
        let poetry = table.starts_with("tool.poetry") && table.ends_with("dependencies") || table.contains("tool.poetry.group.") && table.ends_with(".dependencies");
        if poetry {
            if let Some((key, _)) = line.split_once('=') {
                if manifest::normalise_pypi(key.trim().trim_matches('"')) == want {
                    hits.push(index as u32);
                }
            }
            continue;
        }
        let listed = table == "dependency-groups" || table == "project.optional-dependencies" || table == "project";
        if !listed {
            continue;
        }
        let mut body = line;
        if !in_array {
            let Some((key, rest)) = line.split_once('=') else { continue };
            let key = key.trim();
            if table == "project" && key != "dependencies" {
                continue;
            }
            if !rest.trim_start().starts_with('[') {
                continue;
            }
            in_array = true;
            body = rest.trim_start().trim_start_matches('[');
        }
        let mut rest = body;
        while let Some(open) = rest.find('"') {
            let Some(close) = rest[open + 1..].find('"') else { break };
            let item = &rest[open + 1..open + 1 + close];
            if pep508_name(item).is_some_and(|n| manifest::normalise_pypi(n) == want) {
                hits.push(index as u32);
            }
            rest = &rest[open + close + 2..];
        }
        if body.contains(']') {
            in_array = false;
        }
    }
    one(hits)
}

fn tag<'a>(block: &'a str, name: &str) -> Option<&'a str> {
    let start = block.find(&format!("<{name}>"))? + name.len() + 2;
    let end = block[start..].find(&format!("</{name}>"))?;
    Some(block[start..start + end].trim())
}

fn pom(text: &str, name: &str) -> Decl {
    let Some((group, artifact)) = name.split_once(':') else { return Decl::Absent };
    let mut hits = Vec::new();
    let mut from = 0;
    while let Some(open) = text[from..].find("<dependency>") {
        let start = from + open;
        let Some(len) = text[start..].find("</dependency>") else { break };
        let block = &text[start..start + len];
        if tag(block, "artifactId") == Some(artifact) && tag(block, "groupId") == Some(group) {
            hits.push(line_of(text, start));
        }
        from = start + len;
    }
    one(hits)
}

fn quoted_lines(text: &str, needles: &[String]) -> Decl {
    one(text.lines().enumerate().filter(|(_, l)| needles.iter().any(|n| l.contains(n.as_str()))).map(|(i, _)| i as u32).collect())
}

fn gradle(text: &str, name: &str) -> Decl {
    let needles = ['"', '\''].iter().flat_map(|q| [format!("{q}{name}:"), format!("{q}{name}{q}")]).collect::<Vec<_>>();
    quoted_lines(text, &needles)
}

fn sbt(text: &str, name: &str) -> Decl {
    let Some((group, artifact)) = name.split_once(':') else { return Decl::Absent };
    quoted_lines(text, &[format!("\"{group}\" % \"{artifact}\" %")])
}

pub(crate) fn declared(manifest_path: &str, text: &str, name: &str) -> Decl {
    match (kind(manifest_path), file_name(manifest_path)) {
        (Some(Kind::PackageJson), _) => npm(text, name),
        (Some(Kind::Pyproject), _) => pyproject(text, name),
        (Some(Kind::Pom), _) => pom(text, name),
        (Some(Kind::Gradle), _) => gradle(text, name),
        (_, "build.sbt") => sbt(text, name),
        _ => Decl::Absent,
    }
}

struct Graph {
    nodes: Vec<(String, String)>,
    edges: Vec<Vec<usize>>,
    roots: Vec<usize>,
}

fn npm_graph(text: &str) -> Option<Graph> {
    let root: Value = serde_json::from_str(text).ok()?;
    if !matches!(root.get("lockfileVersion")?.as_u64()?, 2 | 3) {
        return None;
    }
    let packages = root.get("packages")?.as_object().filter(|p| p.len() <= MAX_NODES)?;
    let keys: Vec<&String> = packages
        .iter()
        .filter(|(k, v)| k.contains("node_modules/") && v.get("link").is_none() && v.get("version").is_some())
        .map(|(k, _)| k)
        .collect();
    let index: HashMap<&str, usize> = keys.iter().enumerate().map(|(i, k)| (k.as_str(), i)).collect();
    let mut graph = Graph { nodes: Vec::new(), edges: Vec::new(), roots: Vec::new() };
    for (i, key) in keys.iter().enumerate() {
        let entry = &packages[key.as_str()];
        let name = key.rsplit("node_modules/").next()?.to_owned();
        graph.nodes.push((name, entry["version"].as_str()?.to_owned()));
        if key.matches("node_modules/").count() == 1 {
            graph.roots.push(i);
        }
        let mut out = Vec::new();
        for field in ["dependencies", "optionalDependencies"] {
            for dep in entry.get(field).and_then(Value::as_object).into_iter().flat_map(|m| m.keys()) {
                if let Some(target) = npm_graph::resolve(packages, key, dep).and_then(|k| index.get(k.as_str()).copied()) {
                    out.push(target);
                }
            }
        }
        graph.edges.push(out);
    }
    Some(graph)
}

fn toml_graph(text: &str, uv: bool) -> Option<Graph> {
    let mut records: Vec<(String, String, Vec<String>)> = Vec::new();
    let (mut in_deps, mut in_list) = (false, false);
    for raw in text.lines() {
        let line = raw.trim();
        if line == "[[package]]" {
            records.push((String::new(), String::new(), Vec::new()));
            (in_deps, in_list) = (false, false);
            continue;
        }
        let Some(record) = records.last_mut() else { continue };
        if line.starts_with('[') {
            in_deps = if uv {
                matches!(line, "[package.optional-dependencies]" | "[package.dev-dependencies]")
            } else {
                line == "[package.dependencies]"
            };
            in_list = false;
            continue;
        }
        if in_list {
            if let Some(name) = line.split("name = \"").nth(1).and_then(|s| s.split('"').next()) {
                record.2.push(name.to_owned());
            }
            in_list = !line.starts_with(']');
            continue;
        }
        if in_deps {
            if uv {
                for item in line.split("{ name = \"").skip(1) {
                    record.2.push(item.split('"').next().unwrap_or("").to_owned());
                }
            } else if let Some((key, _)) = line.split_once('=') {
                let key = key.trim().trim_matches('"');
                if !key.starts_with(['{', ']', '#']) {
                    record.2.push(key.to_owned());
                }
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("name = \"") {
            record.0 = manifest::normalise_pypi(value.trim_end_matches('"'));
        } else if let Some(value) = line.strip_prefix("version = \"") {
            record.1 = value.trim_end_matches('"').to_owned();
        } else if uv && line.starts_with("dependencies = [") {
            for item in line.split("name = \"").skip(1) {
                record.2.push(item.split('"').next().unwrap_or("").to_owned());
            }
            in_list = !line.contains(']');
        }
    }
    if records.is_empty() || records.len() > MAX_NODES {
        return None;
    }
    let index: HashMap<String, usize> = records.iter().enumerate().map(|(i, r)| (r.0.clone(), i)).collect();
    let edges = records.iter().map(|r| r.2.iter().filter_map(|d| index.get(&manifest::normalise_pypi(d)).copied()).collect()).collect();
    let roots = (0..records.len()).collect();
    Some(Graph { nodes: records.into_iter().map(|r| (r.0, r.1)).collect(), edges, roots })
}

/// Shortest bounded chain `[direct dependency, ..., target]` as `name@version`, or `None`.
pub(crate) fn chain(lock_path: &str, lock_text: &str, name: &str, version: &str, is_direct: &dyn Fn(&str) -> bool) -> Option<Vec<(String, String)>> {
    let graph = match kind(lock_path)? {
        Kind::PackageLock => npm_graph(lock_text)?,
        Kind::UvLock => toml_graph(lock_text, true)?,
        Kind::PoetryLock => toml_graph(lock_text, false)?,
        _ => return None,
    };
    let pypi = !matches!(kind(lock_path), Some(Kind::PackageLock));
    let name = if pypi { manifest::normalise_pypi(name) } else { name.to_owned() };
    let mut parent: HashMap<usize, Option<usize>> = HashMap::new();
    let mut queue = VecDeque::new();
    for &root in &graph.roots {
        if is_direct(&graph.nodes[root].0) && parent.insert(root, None).is_none() {
            queue.push_back((root, 1));
        }
    }
    while let Some((node, depth)) = queue.pop_front() {
        if graph.nodes[node].0 == name && graph.nodes[node].1 == version {
            let mut path = vec![node];
            while let Some(Some(p)) = parent.get(path.last()?) {
                path.push(*p);
            }
            path.reverse();
            return Some(path.into_iter().map(|i| graph.nodes[i].clone()).collect());
        }
        if depth >= MAX_DEPTH {
            continue;
        }
        for &next in &graph.edges[node] {
            if !parent.contains_key(&next) {
                parent.insert(next, Some(node));
                queue.push_back((next, depth + 1));
            }
        }
    }
    None
}

pub(crate) const DECLARED: &str = "Declared in";

fn short(path: &str) -> &str {
    file_name(path)
}

/// Adds navigable declaration sources to rows. `texts` maps file path to its current text.
pub(crate) fn annotate(rows: &mut [Row], texts: &HashMap<String, String>) {
    for row in rows {
        let name = row.name.clone();
        let sources = std::mem::take(&mut row.sources);
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for mut source in sources {
            let sibling = |file: &str| format!("{}{file}", &source.path[..source.path.len() - file_name(&source.path).len()]);
            match kind(&source.path) {
                Some(Kind::Pom | Kind::Gradle) | None if source.path.ends_with("build.sbt") || matches!(kind(&source.path), Some(Kind::Pom | Kind::Gradle)) => {
                    if let Some(text) = texts.get(&source.path) {
                        match declared(&source.path, text, &name) {
                            Decl::Direct(line) => {
                                source.line = line;
                                source.evidence = format!("{} · {DECLARED} {}:{} (direct, literal)", source.evidence, short(&source.path), line + 1);
                            }
                            Decl::Refused(why) => source.evidence = format!("{} · not bound to one declaration: {why}", source.evidence),
                            Decl::Absent => {}
                        }
                    }
                    out.push(source);
                }
                Some(Kind::PackageLock | Kind::YarnLock | Kind::PnpmLock | Kind::UvLock | Kind::PoetryLock) => {
                    let manifest_path = sibling(if matches!(kind(&source.path), Some(Kind::UvLock | Kind::PoetryLock)) { "pyproject.toml" } else { "package.json" });
                    let lock_path = source.path.clone();
                    let evidence = source.evidence.clone();
                    out.push(source);
                    let Some(text) = texts.get(&manifest_path) else { continue };
                    if !seen.insert(manifest_path.clone()) {
                        continue;
                    }
                    match declared(&manifest_path, text, &name) {
                        Decl::Direct(line) => out.push(Declaration { path: manifest_path.clone(), line, evidence: format!("{DECLARED} {}:{} (direct dependency, exact name, literal declaration); resolved by {}", short(&manifest_path), line + 1, short(&lock_path)) }),
                        Decl::Refused(why) => out.push(Declaration { path: manifest_path.clone(), line: 0, evidence: format!("Not bound to a declaration in {}: {why}", short(&manifest_path)) }),
                        Decl::Absent => {
                            let Some(lock_text) = texts.get(&lock_path) else { continue };
                            let direct = |n: &str| matches!(declared(&manifest_path, text, n), Decl::Direct(_));
                            let Some(path) = chain(&lock_path, lock_text, &name, &row.version, &direct) else { continue };
                            let Decl::Direct(line) = declared(&manifest_path, text, &path[0].0) else { continue };
                            let via = path.iter().map(|(n, v)| format!("{n}@{v}")).collect::<Vec<_>>().join(" > ");
                            let _ = evidence;
                            out.push(Declaration { path: manifest_path.clone(), line, evidence: format!("{DECLARED} {}:{} (transitive via {via})", short(&manifest_path), line + 1) });
                        }
                    }
                }
                _ => out.push(source),
            }
        }
        row.sources = out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_json_direct_ambiguous_and_alias() {
        let text = "{\n \"dependencies\": {\n  \"lodash\": \"^4\",\n  \"x\": \"npm:y@1\"\n },\n \"devDependencies\": {\n  \"mocha\": \"1\"\n }\n}";
        assert_eq!(declared("package.json", text, "lodash"), Decl::Direct(2));
        assert_eq!(declared("package.json", text, "y"), Decl::Refused("npm: alias"));
        assert_eq!(declared("package.json", text, "nope"), Decl::Absent);
        let twice = "{\"dependencies\":{\"a\":\"1\"},\"devDependencies\":{\"a\":\"2\"}}";
        assert_eq!(declared("package.json", twice, "a"), Decl::Refused("declared more than once"));
    }

    #[test]
    fn pyproject_binds_requirements_and_poetry_keys_but_not_prose() {
        let text = "[project]\ndescription = \"requests is nice\"\ndependencies = [\n  \"Requests>=2\",\n  \"flask\",\n]\n[tool.poetry.dependencies]\nDjango = \"^4\"\n";
        assert_eq!(declared("pyproject.toml", text, "requests"), Decl::Direct(3));
        assert_eq!(declared("pyproject.toml", text, "django"), Decl::Direct(7));
        let twice = "[project]\ndependencies = [\"a>1\"]\n[dependency-groups]\ndev = [\"a\"]\n";
        assert_eq!(declared("pyproject.toml", twice, "a"), Decl::Refused("declared more than once"));
    }

    #[test]
    fn maven_gradle_sbt_literals() {
        let pom = "<dependencies>\n<dependency>\n<groupId>g</groupId>\n<artifactId>a</artifactId>\n<version>1</version>\n</dependency>\n</dependencies>";
        assert_eq!(declared("pom.xml", pom, "g:a"), Decl::Direct(1));
        assert_eq!(declared("pom.xml", &format!("{pom}{pom}"), "g:a"), Decl::Refused("declared more than once"));
        assert_eq!(declared("build.gradle", "dependencies {\n implementation 'g:a:1'\n}", "g:a"), Decl::Direct(1));
        assert_eq!(declared("build.sbt", "libraryDependencies += \"g\" % \"a\" % \"1\"\n", "g:a"), Decl::Direct(0));
    }

    const LOCK: &str = r#"{"lockfileVersion":3,"packages":{"":{},
      "node_modules/a":{"version":"1.0.0","dependencies":{"b":"^1"}},
      "node_modules/b":{"version":"2.0.0","dependencies":{"c":"^1"}},
      "node_modules/c":{"version":"3.0.0"}}}"#;

    #[test]
    fn npm_chain_is_shortest_and_bounded() {
        let direct = |n: &str| n == "a";
        let path = chain("package-lock.json", LOCK, "c", "3.0.0", &direct).unwrap();
        assert_eq!(path.iter().map(|p| p.0.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        assert!(chain("package-lock.json", LOCK, "c", "9", &direct).is_none());
    }

    #[test]
    fn uv_chain_and_annotate_adds_navigable_declaration() {
        let uv = "[[package]]\nname = \"app\"\nversion = \"0\"\ndependencies = [\n    { name = \"Req\" },\n]\n\n[[package]]\nname = \"req\"\nversion = \"1\"\ndependencies = [\n    { name = \"urllib3\" },\n]\n\n[[package]]\nname = \"urllib3\"\nversion = \"1.2\"\n";
        let pyproject = "[project]\ndependencies = [\n \"req\",\n]\n";
        let mut texts = HashMap::new();
        texts.insert("/p/uv.lock".to_owned(), uv.to_owned());
        texts.insert("/p/pyproject.toml".to_owned(), pyproject.to_owned());
        let mut rows = vec![Row {
            path: "/p/uv.lock".into(), line: 0, name: "urllib3".into(), version: "1.2".into(), fix: None, advisories: vec![], ecosystem: Some("PyPI".into()),
            sources: vec![Declaration { path: "/p/uv.lock".into(), line: 15, evidence: "Lockfile entry".into() }], details: vec![],
        }];
        annotate(&mut rows, &texts);
        let bound = &rows[0].sources[1];
        assert_eq!((bound.path.as_str(), bound.line), ("/p/pyproject.toml", 2));
        assert!(bound.evidence.contains("pyproject.toml:3") && bound.evidence.contains("req@1 > urllib3@1.2"));
    }

    #[test]
    fn uv_lock_real_shapes_optional_dev_and_extras() {
        let uv = "[[package]]\nname = \"validate-pyproject\"\nversion = \"0.25\"\nsource = { registry = \"https://pypi.org/simple\" }\ndependencies = [\n    { name = \"fastjsonschema\" },\n]\n\n[package.optional-dependencies]\nall = [\n    { name = \"packaging\" },\n    { name = \"tomli\", marker = \"python_full_version < '3.11'\" },\n]\n\n[package.metadata]\nrequires-dist = [\n    { name = \"ghost\", specifier = \">=1\" },\n]\n\n[[package]]\nname = \"uv\"\nversion = \"0.1\"\nsource = { editable = \".\" }\n\n[package.dev-dependencies]\ncheck = [\n    { name = \"validate-pyproject\", extra = [\"all\"] },\n]\n\n[package.metadata.requires-dev]\ncheck = [\n    { name = \"ghost\", specifier = \">=1\" },\n]\n\n[[package]]\nname = \"packaging\"\nversion = \"24\"\n\n[[package]]\nname = \"ghost\"\nversion = \"9\"\n";
        let direct = |n: &str| n == "uv";
        let got = chain("/p/uv.lock", uv, "packaging", "24", &direct).unwrap();
        assert_eq!(got.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["uv", "validate-pyproject", "packaging"]);
        assert!(chain("/p/uv.lock", uv, "ghost", "9", &direct).is_none());
    }

    #[test]
    fn poetry_lock_real_shapes_inline_tables_arrays_and_extras() {
        let poetry = "[[package]]\nname = \"Root\"\nversion = \"1\"\n\n[package.dependencies]\nidna = \">=2.8\"\ntyping_extensions = {version = \">=4.16.0\", markers = \"python_version < \\\"3.15\\\"\"}\nexceptiongroup = [\n    {version = \">=1\", markers = \"python_version < \\\"3.11\\\"\"},\n]\n\n[package.extras]\ntrio = [\"ghost (>=0.32.0)\"]\n\n[[package]]\nname = \"typing-extensions\"\nversion = \"4.1\"\n\n[[package]]\nname = \"exceptiongroup\"\nversion = \"1.2\"\n\n[[package]]\nname = \"ghost\"\nversion = \"9\"\n";
        let direct = |n: &str| n == "root";
        for (n, v) in [("typing_extensions", "4.1"), ("exceptiongroup", "1.2")] {
            assert_eq!(chain("/p/poetry.lock", poetry, n, v, &direct).unwrap().len(), 2, "{n}");
        }
        assert!(chain("/p/poetry.lock", poetry, "ghost", "9", &direct).is_none());
    }
}
