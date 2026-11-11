//! Shortest bounded chains from each workspace member through package-lock v2/v3, yarn berry and pnpm lockfiles.
//! Peer dependencies are edges only for package-lock v3 (npm >= 7 installs them and v3 lists them on installed records).
use crate::Kind;
use crate::binding::{MAX_DEPTH, MAX_NODES};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};

const MAX_MEMBERS: usize = 50;

struct Graph {
    nodes: Vec<(String, String)>,
    edges: Vec<Vec<usize>>,
    members: Vec<(String, Vec<usize>)>,
}

/// `(member directory relative to the lockfile, [first dependency, ..., target])` per member reaching the coordinate.
pub(crate) fn chains(kind: Kind, text: &str, name: &str, version: &str) -> Vec<(String, Vec<(String, String)>)> {
    let graph = match kind {
        Kind::PackageLock => npm(text),
        Kind::YarnLock => yarn(text),
        Kind::PnpmLock => pnpm(text),
        _ => None,
    };
    let Some(graph) = graph else { return Vec::new() };
    let mut found = Vec::new();
    for (dir, starts) in graph.members.iter().take(MAX_MEMBERS) {
        let mut parent: HashMap<usize, Option<usize>> = HashMap::new();
        let mut queue = VecDeque::new();
        for &start in starts {
            if parent.insert(start, None).is_none() {
                queue.push_back((start, 1));
            }
        }
        while let Some((node, depth)) = queue.pop_front() {
            if graph.nodes[node].0 == name && graph.nodes[node].1 == version {
                let mut path = vec![node];
                while let Some(Some(p)) = parent.get(path.last().unwrap()) {
                    path.push(*p);
                }
                path.reverse();
                found.push((dir.clone(), path.into_iter().map(|i| graph.nodes[i].clone()).collect()));
                break;
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
    }

    found
}

fn npm(text: &str) -> Option<Graph> {
    let root: Value = serde_json::from_str(text).ok()?;
    let lock_version = root.get("lockfileVersion")?.as_u64()?;
    if !matches!(lock_version, 2 | 3) {
        return None;
    }
    let packages = root.get("packages")?.as_object().filter(|p| p.len() <= MAX_NODES)?;
    let is_member = |key: &str| !key.is_empty() && !key.contains("node_modules/");
    let installed = |key: &str, entry: &Value| entry.get("link").is_none() && (is_member(key) || key.contains("node_modules/"));
    let mut index = HashMap::new();
    let mut nodes = Vec::new();
    let mut keys = Vec::new();
    for (key, entry) in packages.iter().filter(|(k, e)| installed(k, e)) {
        let Some(version) = entry.get("version").and_then(Value::as_str) else { continue };
        let name = entry.get("name").and_then(Value::as_str).or_else(|| key.contains("node_modules/").then(|| key.rsplit("node_modules/").next()).flatten());
        let Some(name) = name else { continue };
        index.insert(key.as_str(), nodes.len());
        nodes.push((name.to_owned(), version.to_owned()));
        keys.push(key.as_str());
    }
    let targets = |key: &str, fields: &[&str]| -> Vec<usize> {
        let mut out = Vec::new();
        for field in fields {
            for dep in packages[key].get(field).and_then(Value::as_object).into_iter().flat_map(|m| m.keys()) {
                if let Some(i) = crate::npm_graph::resolve_linked(packages, key, dep).and_then(|t| index.get(t.as_str()).copied()) {
                    out.push(i);
                }
            }
        }
        out
    };
    let mut traversed = vec!["dependencies", "optionalDependencies"];
    if lock_version == 3 {
        traversed.push("peerDependencies");
    }
    let edges = keys.iter().map(|k| targets(k, &traversed)).collect();
    let mut declared = traversed.clone();
    declared.push("devDependencies");
    let members = packages
        .iter()
        .filter(|(k, e)| is_member(k) && e.get("link").is_none())
        .map(|(k, _)| (k.clone(), targets(k, &declared)))
        .collect();

    Some(Graph { nodes, edges, members })
}

fn unquote(value: &str) -> &str {
    value.trim().trim_matches(['"', '\''])
}

fn yarn(text: &str) -> Option<Graph> {
    struct Block {
        specs: Vec<String>,
        name: String,
        version: String,
        workspace: Option<String>,
        deps: Vec<(String, String)>,
    }
    let mut blocks: Vec<Block> = Vec::new();
    let mut in_deps = false;
    for line in text.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        if !line.starts_with(' ') {
            in_deps = false;
            let header = line.strip_suffix(':')?;
            let specs: Vec<String> = unquote(header).split(", ").map(|s| unquote(s).to_owned()).collect();
            blocks.push(Block { specs, name: String::new(), version: String::new(), workspace: None, deps: Vec::new() });
            continue;
        }
        let block = blocks.last_mut()?;
        let indent = line.len() - line.trim_start().len();
        let line = line.trim();
        if indent == 2 {
            in_deps = matches!(line, "dependencies:" | "optionalDependencies:");
            if let Some(v) = line.strip_prefix("version: ") {
                block.version = unquote(v).to_owned();
            } else if let Some(r) = line.strip_prefix("resolution: ") {
                let r = unquote(r);
                block.name = r.char_indices().skip(1).find(|(_, c)| *c == '@').map_or(r, |(i, _)| &r[..i]).to_owned();
                block.workspace = r.split_once("@workspace:").map(|(_, dir)| dir.to_owned());
            }
        } else if indent == 4 && in_deps {
            if let Some((name, range)) = line.split_once(": ") {
                block.deps.push((unquote(name).to_owned(), unquote(range).to_owned()));
            }
        }
    }
    blocks.retain(|b| !b.version.is_empty() && !b.name.is_empty());
    if blocks.is_empty() || blocks.len() > MAX_NODES {
        return None;
    }
    let specs: HashMap<&str, usize> = blocks.iter().enumerate().flat_map(|(i, b)| b.specs.iter().map(move |s| (s.as_str(), i))).collect();
    let edges: Vec<Vec<usize>> = blocks
        .iter()
        .map(|b| {
            b.deps
                .iter()
                .filter_map(|(n, r)| specs.get(format!("{n}@{r}").as_str()).or_else(|| specs.get(format!("{n}@npm:{r}").as_str())).copied())
                .collect()
        })
        .collect();
    let members = blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| Some((b.workspace.clone().filter(|d| d != "."), i)))
        .filter_map(|(dir, i)| Some((dir?, edges[i].clone())))
        .collect();

    Some(Graph { nodes: blocks.into_iter().map(|b| (b.name, b.version)).collect(), edges, members })
}

fn pnpm(text: &str) -> Option<Graph> {
    let mut records: Vec<(&str, String, Vec<(String, String)>)> = Vec::new();
    let mut top = "";
    let mut in_deps = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        let tracked = matches!(top, "importers" | "packages" | "snapshots");
        match indent {
            0 => {
                top = line.trim_end_matches(':');
                in_deps = false;
            }
            2 if tracked => {
                in_deps = false;
                if let Some(key) = line.strip_suffix(" {}").unwrap_or(line).strip_suffix(':') {
                    records.push((top, unquote(key).to_owned(), Vec::new()));
                }
            }
            4 if tracked => in_deps = matches!(line, "dependencies:" | "optionalDependencies:") || (top == "importers" && line == "devDependencies:"),
            6 if in_deps => {
                let (name, version) = line.split_once(": ").unwrap_or((line.trim_end_matches(':'), ""));
                records.last_mut()?.2.push((unquote(name).to_owned(), unquote(version).to_owned()));
            }
            8 if in_deps => {
                if let Some(v) = line.strip_prefix("version: ") {
                    if let Some(dep) = records.last_mut()?.2.last_mut().filter(|d| d.1.is_empty()) {
                        dep.1 = unquote(v).to_owned();
                    }
                }
            }
            _ => {}
        }
    }
    let plain = |v: &str| -> Option<String> {
        let v = v.split('(').next()?;
        (!v.is_empty() && !v.contains([':', '@'])).then(|| v.to_owned())
    };
    let coordinate = |key: &str| -> Option<(String, String)> {
        let key = key.trim_start_matches('/').split('(').next()?;
        let (name, version) = key.char_indices().skip(1).filter(|(_, c)| *c == '@').last().map(|(i, _)| (&key[..i], &key[i + 1..])).or_else(|| key.rsplit_once('/'))?;
        Some((name.to_owned(), version.to_owned()))
    };
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    let mut nodes = Vec::new();
    for (_, key, _) in records.iter().filter(|(t, _, _)| *t != "importers") {
        if let Some(c) = coordinate(key) {
            index.entry(c.clone()).or_insert_with(|| {
                nodes.push(c);
                nodes.len() - 1
            });
        }
    }
    if nodes.is_empty() || nodes.len() > MAX_NODES {
        return None;
    }
    let resolve = |deps: &[(String, String)]| -> Vec<usize> {
        deps.iter().filter_map(|(n, v)| index.get(&(n.clone(), plain(v)?)).copied()).collect()
    };
    let mut edges = vec![Vec::new(); nodes.len()];
    let mut members = Vec::new();
    for (top, key, deps) in &records {
        if *top == "importers" {
            if key != "." {
                members.push((key.clone(), resolve(deps)));
            }
        } else if let Some(&i) = coordinate(key).and_then(|c| index.get(&c)) {
            let more = resolve(deps);
            edges[i].extend(more);
        }
    }

    Some(Graph { nodes, edges, members })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(path: &[(String, String)]) -> Vec<&str> {
        path.iter().map(|(n, _)| n.as_str()).collect()
    }

    #[test]
    fn npm_follows_workspace_links_and_installs_nested_under_members() {
        let lock = r#"{"lockfileVersion":3,"packages":{
          "":{"name":"root","workspaces":["packages/*"]},
          "node_modules/a":{"resolved":"packages/a","link":true},
          "node_modules/b":{"resolved":"packages/b","link":true},
          "packages/a":{"name":"a","version":"1.0.0","dependencies":{"b":"*","x":"^1"}},
          "packages/b":{"name":"b","version":"1.0.0","dependencies":{"y":"^1"},"devDependencies":{"d":"1"}},
          "node_modules/x":{"version":"1.0.0","dependencies":{"z":"^1"}},
          "node_modules/y":{"version":"2.0.0"},
          "node_modules/d":{"version":"1.0.0"},
          "node_modules/z":{"version":"3.0.0"},
          "packages/a/node_modules/z":{"version":"4.0.0"}}}"#;
        let got = chains(Kind::PackageLock, lock, "z", "3.0.0");
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].0.as_str(), names(&got[0].1)), ("packages/a", vec!["x", "z"]));
        let through_link = chains(Kind::PackageLock, lock, "y", "2.0.0");
        let mut dirs: Vec<_> = through_link.iter().map(|(d, p)| (d.as_str(), names(p))).collect();
        dirs.sort();
        assert_eq!(dirs, [("packages/a", vec!["b", "y"]), ("packages/b", vec!["y"])]);
        assert_eq!(chains(Kind::PackageLock, lock, "d", "1.0.0").iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), ["packages/b"]);
        assert!(chains(Kind::PackageLock, lock, "z", "4.0.0").iter().all(|c| c.0 == "packages/a" && c.1.len() == 1));
    }

    #[test]
    fn peer_dependencies_are_edges_only_for_lockfile_v3() {
        let lock = |v: u8| format!(r#"{{"lockfileVersion":{v},"packages":{{"":{{}},"packages/a":{{"name":"a","version":"1","dependencies":{{"h":"1"}}}},"node_modules/h":{{"version":"1","peerDependencies":{{"p":"*"}}}},"node_modules/p":{{"version":"2"}}}}}}"#);
        assert_eq!(chains(Kind::PackageLock, &lock(3), "p", "2").len(), 1);
        assert!(chains(Kind::PackageLock, &lock(2), "p", "2").is_empty());
    }

    #[test]
    fn yarn_berry_workspace_entries_real_shape() {
        let lock = "__metadata:\n  version: 8\n\n\"@yarnpkg/builder@workspace:^, @yarnpkg/builder@workspace:packages/yarnpkg-builder\":\n  version: 0.0.0-use.local\n  resolution: \"@yarnpkg/builder@workspace:packages/yarnpkg-builder\"\n  dependencies:\n    \"@types/semver\": \"npm:^7.1.0\"\n    esbuild: \"npm:esbuild-wasm@^0.23.0\"\n    typescript: \"catalog:\"\n  languageName: unknown\n  linkType: soft\n\n\"@types/semver@npm:^7.1.0\":\n  version: 7.5.0\n  resolution: \"@types/semver@npm:7.5.0\"\n  dependencies:\n    leaf: \"npm:^1\"\n\n\"esbuild@npm:esbuild-wasm@^0.23.0\":\n  version: 0.23.1\n  resolution: \"esbuild-wasm@npm:0.23.1\"\n\n\"leaf@npm:^1\":\n  version: 1.0.0\n  resolution: \"leaf@npm:1.0.0\"\n\n\"app@workspace:.\":\n  version: 0.0.0-use.local\n  resolution: \"app@workspace:.\"\n  dependencies:\n    leaf: \"npm:^1\"\n";
        let got = chains(Kind::YarnLock, lock, "leaf", "1.0.0");
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].0.as_str(), names(&got[0].1)), ("packages/yarnpkg-builder", vec!["@types/semver", "leaf"]));
        assert_eq!(chains(Kind::YarnLock, lock, "esbuild-wasm", "0.23.1")[0].1.len(), 1);
    }

    #[test]
    fn pnpm_importers_v9_and_v6_shapes() {
        let v9 = "lockfileVersion: '9.0'\n\nimporters:\n\n  .:\n    dependencies:\n      root-only:\n        specifier: 1.0.0\n        version: 1.0.0\n\n  packages/a:\n    dependencies:\n      '@s/x':\n        specifier: ^1\n        version: 1.0.0(peer@2.0.0)\n      sib:\n        specifier: workspace:*\n        version: link:../b\n    devDependencies:\n      y:\n        specifier: ^2\n        version: 2.0.0\n\npackages:\n\n  '@s/x@1.0.0':\n    resolution: {integrity: x}\n\nsnapshots:\n\n  '@s/x@1.0.0(peer@2.0.0)':\n    dependencies:\n      leaf: 3.0.0\n\n  leaf@3.0.0: {}\n\n  y@2.0.0: {}\n\n  root-only@1.0.0: {}\n";
        let got = chains(Kind::PnpmLock, v9, "leaf", "3.0.0");
        assert_eq!((got[0].0.as_str(), names(&got[0].1)), ("packages/a", vec!["@s/x", "leaf"]));
        assert_eq!(chains(Kind::PnpmLock, v9, "y", "2.0.0")[0].1.len(), 1);
        assert!(chains(Kind::PnpmLock, v9, "root-only", "1.0.0").is_empty());
        let v6 = "lockfileVersion: '6.0'\n\nimporters:\n  packages/a:\n    dependencies:\n      lodash:\n        specifier: ^4\n        version: 4.17.15\n\npackages:\n\n  /lodash@4.17.15:\n    resolution: {integrity: x}\n    dependencies:\n      inner: 1.0.0\n    dev: false\n\n  /inner@1.0.0:\n    dev: false\n";
        assert_eq!(names(&chains(Kind::PnpmLock, v6, "inner", "1.0.0")[0].1), ["lodash", "inner"]);
    }
}
