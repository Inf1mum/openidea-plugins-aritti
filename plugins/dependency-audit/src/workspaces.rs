//! npm/yarn `workspaces` and `pnpm-workspace.yaml` globs, used to recognise local workspace members.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

const MAX_GLOBS: usize = 100;

/// `workspaces` as an array, or yarn's `{ "packages": [...] }` object.
pub(crate) fn package_json_globs(text: &str) -> Vec<String> {
    let Ok(root) = serde_json::from_str::<Value>(text) else { return Vec::new() };
    let list = match root.get("workspaces") {
        Some(Value::Object(object)) => object.get("packages"),
        other => other,
    };

    list.and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).take(MAX_GLOBS).map(str::to_owned).collect())
        .unwrap_or_default()
}

/// Literal `packages:` sequence items of `pnpm-workspace.yaml` (block style, optionally quoted).
pub(crate) fn pnpm_globs(text: &str) -> Vec<String> {
    let mut globs = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let without_comment = line.split(" #").next().unwrap_or(line);
        if !line.starts_with([' ', '\t', '-']) && !line.trim().is_empty() {
            inside = without_comment.trim_end() == "packages:";

            continue;
        }
        let Some(item) = inside.then(|| without_comment.trim().strip_prefix('-')).flatten() else { continue };
        let item = item.trim().trim_matches(['"', '\'']);
        if !item.is_empty() && globs.len() < MAX_GLOBS {
            globs.push(item.to_owned());
        }
    }

    globs
}

fn segment_matches(pattern: &str, name: &str) -> bool {
    if name.starts_with('.') && pattern != name {
        return false;
    }
    match pattern.split_once('*') {
        None => pattern == name,
        Some((head, tail)) => {
            name.len() >= head.len() + tail.len() && name.starts_with(head) && name.ends_with(tail) && !tail.contains('*')
        }
    }
}

fn path_matches(pattern: &[&str], path: &[&str]) -> bool {
    match (pattern.first(), path.first()) {
        (None, None) => true,
        (Some(&"**"), _) => path_matches(&pattern[1..], path) || (!path.is_empty() && path_matches(pattern, &path[1..])),
        (Some(p), Some(n)) => segment_matches(p, n) && path_matches(&pattern[1..], &path[1..]),
        _ => false,
    }
}

fn segments(value: &str) -> Vec<&str> {
    value.split('/').filter(|s| !s.is_empty() && *s != ".").collect()
}

/// Whether `dir` (relative to the workspace root) is selected: some positive glob matches and no `!` glob does.
pub(crate) fn is_member(globs: &[String], dir: &str) -> bool {
    let path = segments(dir);
    if path.is_empty() {
        return false;
    }
    let matches = |glob: &str| path_matches(&segments(glob), &path);
    globs.iter().filter(|g| !g.starts_with('!')).any(|g| matches(g)) && !globs.iter().filter_map(|g| g.strip_prefix('!')).any(matches)
}

/// Directory (with trailing `/`) of a manifest path.
fn directory(path: &str) -> &str {
    &path[..path.rfind('/').map_or(0, |i| i + 1)]
}

/// Names of workspace members per workspace root directory.
/// `package_jsons` maps each `package.json` path to its text, `pnpm` maps a directory to its `pnpm-workspace.yaml` text.
pub(crate) fn members(package_jsons: &HashMap<String, String>, pnpm: &HashMap<String, String>) -> HashMap<String, HashSet<String>> {
    let mut roots: HashMap<String, Vec<String>> = HashMap::new();
    for (path, text) in package_jsons {
        let globs = package_json_globs(text);
        if !globs.is_empty() {
            roots.insert(directory(path).to_owned(), globs);
        }
    }
    for (dir, text) in pnpm {
        let globs = pnpm_globs(text);
        if !globs.is_empty() {
            roots.insert(dir.clone(), globs);
        }
    }

    let mut found: HashMap<String, HashSet<String>> = HashMap::new();
    for (root, globs) in &roots {
        for (path, text) in package_jsons {
            let Some(relative) = directory(path).strip_prefix(root.as_str()) else { continue };
            if !is_member(globs, relative) {
                continue;
            }
            if let Some(name) = serde_json::from_str::<Value>(text).ok().and_then(|v| v.get("name")?.as_str().map(str::to_owned)) {
                found.entry(root.clone()).or_default().insert(name);
            }
        }
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn globs(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reads_npm_yarn_and_pnpm_globs() {
        assert_eq!(package_json_globs(r#"{"workspaces":["packages/*","./apps/web"]}"#), ["packages/*", "./apps/web"]);
        assert_eq!(package_json_globs(r#"{"workspaces":{"packages":["libs/*"]}}"#), ["libs/*"]);
        assert!(package_json_globs(r#"{"workspaces":"x"}"#).is_empty());
        let yaml = "packages:\n  - 'my-app'  # app\n  - \"packages/*\"\n  - '!**/test/**'\ncatalog:\n  - x\n";
        assert_eq!(pnpm_globs(yaml), ["my-app", "packages/*", "!**/test/**"]);
    }

    #[test]
    fn matches_literal_star_and_double_star_globs_with_exclusions() {
        let g = globs(&["./packages/*", "apps/web", "components/**", "!**/test/**"]);
        for member in ["packages/a/", "apps/web/", "components/x/y/", "packages//b"] {
            assert!(is_member(&g, member), "{member}");
        }
        for other in ["packages/a/b/", "apps/api/", "components/x/test/y/", "packages/.cache/", ""] {
            assert!(!is_member(&g, other), "{other}");
        }
    }

    #[test]
    fn collects_member_names_per_root() {
        let json = |name: &str| format!(r#"{{"name":"{name}"}}"#);
        let mut files = HashMap::new();
        files.insert("/r/package.json".to_owned(), r#"{"name":"root","workspaces":["packages/*"]}"#.to_owned());
        files.insert("/r/packages/a/package.json".to_owned(), json("a"));
        files.insert("/r/other/b/package.json".to_owned(), json("b"));
        let found = members(&files, &HashMap::new());
        assert_eq!(found["/r/"], HashSet::from(["a".to_owned()]));
    }
}
