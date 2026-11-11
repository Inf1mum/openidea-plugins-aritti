//! `pyproject.toml`: PEP 621 `[project]` dependencies and PEP 735 `[dependency-groups]`.
//! Only exact `==` pins name an installed version; every other specifier is skipped quietly.
//! Marker-guarded requirements (PEP 508) are still audited and reported as "marker not evaluated".

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::manifest::{Dep, Ecosystem, MARKER, Note, normalise_pypi};

const MAX_ITEMS: usize = 2000;
const MAX_GROUPS: usize = 200;
const MAX_DEPTH: usize = 16;

enum Item {
    Req(usize),
    Include(String),
}

struct Req {
    name: String,
    name_at: usize,
    version: String,
    marker: bool,
}

/// Content range and end offset of the string starting at `i`; `None` when it is escaped or multi-line (unreliable).
fn string_at(text: &str, i: usize) -> Option<((usize, usize), usize)> {
    let quote = *text.as_bytes().get(i)?;
    if text[i..].starts_with("\"\"\"") || text[i..].starts_with("'''") {
        return None;
    }
    let mut j = i + 1;
    while let Some(&c) = text.as_bytes().get(j) {
        match c {
            b'\n' => return None,
            b'\\' if quote == b'"' => return None,
            c if c == quote => return Some(((i + 1, j), j + 1)),
            _ => j += 1,
        }
    }

    None
}

/// End offset of the multi-line string or comment starting at `i`, when one starts there.
fn skip_opaque(text: &str, i: usize) -> Option<usize> {
    let rest = &text[i..];
    for fence in ["\"\"\"", "'''"] {
        if rest.starts_with(fence) {
            return Some(rest[3..].find(fence).map_or(text.len(), |p| i + 3 + p + 3));
        }
    }
    rest.starts_with('#').then(|| rest.find('\n').map_or(text.len(), |p| i + p))
}

struct Array {
    section: String,
    key: String,
    items: Vec<(Option<(usize, usize)>, Option<String>)>,
}

/// Items of the array starting at `start` (`[`); strings carry their content range, `{include-group = ".."}` the group.
fn array(text: &str, start: usize) -> (Vec<(Option<(usize, usize)>, Option<String>)>, usize) {
    let b = text.as_bytes();
    let mut items = Vec::new();
    let mut i = start + 1;
    let mut depth = 0usize;
    while i < b.len() {
        if let Some(end) = skip_opaque(text, i) {
            i = end.max(i + 1);
            continue;
        }
        match b[i] {
            b'"' | b'\'' if depth == 0 => match string_at(text, i) {
                Some((range, end)) => {
                    items.push((Some(range), None));
                    i = end;
                }
                None => i += 1,
            },
            b'"' | b'\'' => i = string_at(text, i).map_or(i + 1, |(_, end)| end),
            b'{' | b'[' => {
                if depth == 0 && b[i] == b'{' {
                    let close = text[i..].find('}').map_or(text.len(), |p| i + p);
                    let body = &text[i + 1..close];
                    let group = body
                        .split_once("include-group")
                        .and_then(|(_, rest)| rest.trim_start().strip_prefix('='))
                        .map(str::trim)
                        .and_then(|v| v.strip_prefix(['"', '\'']))
                        .and_then(|v| v.split(['"', '\'']).next());
                    items.push((None, group.map(str::to_owned)));
                }
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b']' if depth == 0 => return (items, i + 1),
            b']' => {
                depth -= 1;
                i += 1;
            }
            _ => i += 1,
        }
        if items.len() > MAX_ITEMS {
            break;
        }
    }

    (items, i)
}

/// Every `key = [ ... ]` of the document with the table it appears in.
fn arrays(text: &str) -> Vec<Array> {
    let mut found = Vec::new();
    let mut section = String::new();
    let mut i = 0;
    while i < text.len() {
        let line_end = text[i..].find('\n').map_or(text.len(), |p| i + p);
        let line = &text[i..line_end];
        let trimmed = line.trim_start();
        let next = line_end + 1;
        if trimmed.starts_with('[') {
            section = trimmed
                .trim_start_matches('[')
                .split(']')
                .next()
                .unwrap_or_default()
                .chars()
                .filter(|c| !c.is_whitespace() && *c != '"' && *c != '\'')
                .collect();
            i = next;
            continue;
        }
        let Some((key, _)) = trimmed.split_once('=') else {
            i = next;
            continue;
        };
        let value_at = i + (line.len() - trimmed.len()) + key.len() + 1;
        let value_at = value_at + (text[value_at..].len() - text[value_at..].trim_start_matches([' ', '\t']).len());
        let key = key.trim().trim_matches(['"', '\'']).to_owned();
        match text.as_bytes().get(value_at) {
            Some(b'[') => {
                let (items, end) = array(text, value_at);
                if found.len() < MAX_GROUPS * 2 {
                    found.push(Array { section: section.clone(), key, items });
                }
                i = text[end.min(text.len())..].find('\n').map_or(text.len(), |p| end + p + 1);
            }
            _ => i = skip_opaque(text, value_at).map_or(next, |end| end.max(next)),
        }
    }

    found
}

fn is_env_var(word: &str) -> bool {
    matches!(
        word,
        "python_version" | "python_full_version" | "os_name" | "sys_platform" | "platform_release" | "platform_system"
            | "platform_version" | "platform_machine" | "platform_python_implementation" | "implementation_name"
            | "implementation_version" | "extra" | "extras" | "dependency_groups"
    )
}

struct Marker<'a> {
    b: &'a [u8],
    i: usize,
}

impl Marker<'_> {
    fn ws(&mut self) {
        while self.b.get(self.i).is_some_and(u8::is_ascii_whitespace) {
            self.i += 1;
        }
    }

    fn word(&mut self, word: &str) -> bool {
        self.ws();
        let end = self.i + word.len();
        let boundary = self.b.get(end).is_none_or(|c| !(c.is_ascii_alphanumeric() || *c == b'_'));
        let found = self.b.get(self.i..end) == Some(word.as_bytes()) && boundary;
        if found {
            self.i = end;
        }

        found
    }

    fn operand(&mut self) -> bool {
        self.ws();
        match self.b.get(self.i) {
            Some(&q) if q == b'"' || q == b'\'' => {
                let Some(len) = self.b[self.i + 1..].iter().position(|&c| c == q) else { return false };
                self.i += len + 2;

                true
            }
            Some(c) if c.is_ascii_lowercase() => {
                let start = self.i;
                while self.b.get(self.i).is_some_and(|c| c.is_ascii_lowercase() || *c == b'_') {
                    self.i += 1;
                }

                std::str::from_utf8(&self.b[start..self.i]).is_ok_and(is_env_var)
            }
            _ => false,
        }
    }

    fn operator(&mut self) -> bool {
        self.ws();
        for op in ["===", "==", "<=", ">=", "!=", "~=", "<", ">"] {
            if self.b[self.i..].starts_with(op.as_bytes()) {
                self.i += op.len();

                return true;
            }
        }
        if self.word("not") {
            return self.word("in");
        }

        self.word("in")
    }

    fn expression(&mut self, depth: usize) -> bool {
        self.ws();
        if self.b.get(self.i) == Some(&b'(') {
            self.i += 1;
            if depth >= MAX_DEPTH || !self.or(depth + 1) {
                return false;
            }
            self.ws();
            if self.b.get(self.i) != Some(&b')') {
                return false;
            }
            self.i += 1;

            return true;
        }

        self.operand() && self.operator() && self.operand()
    }

    fn and(&mut self, depth: usize) -> bool {
        self.expression(depth) && {
            while self.word("and") {
                if !self.expression(depth) {
                    return false;
                }
            }

            true
        }
    }

    fn or(&mut self, depth: usize) -> bool {
        self.and(depth) && {
            while self.word("or") {
                if !self.and(depth) {
                    return false;
                }
            }

            true
        }
    }
}

/// PEP 508 marker grammar, bounded in length and nesting.
pub(crate) fn valid_marker(marker: &str) -> bool {
    let mut parser = Marker { b: marker.as_bytes(), i: 0 };
    marker.len() <= 1024 && parser.or(0) && {
        parser.ws();
        parser.i == marker.len()
    }
}

/// `name[extras] ==version ; marker`; anything that is not an exact PyPI pin yields `None`.
fn requirement(spec: &str, base: usize) -> Option<Req> {
    let lead = spec.len() - spec.trim_start().len();
    let spec = &spec[lead..];
    let name_len = spec.find(|c: char| !(c.is_ascii_alphanumeric() || "-_.".contains(c))).unwrap_or(spec.len());
    let name = &spec[..name_len];
    if !name.starts_with(|c: char| c.is_ascii_alphanumeric()) || !name.ends_with(|c: char| c.is_ascii_alphanumeric()) {
        return None;
    }
    let mut rest = spec[name_len..].trim_start();
    if let Some(extras) = rest.strip_prefix('[') {
        rest = extras[extras.find(']')? + 1..].trim_start();
    }
    let (versions, marker) = match rest.split_once(';') {
        Some((versions, marker)) => (versions, Some(marker.trim())),
        None => (rest, None),
    };
    let marker = match marker {
        Some(marker) if !valid_marker(marker) => return None,
        Some(_) => true,
        None => false,
    };
    let versions = versions.trim();
    let versions = versions.strip_prefix('(').and_then(|v| v.strip_suffix(')')).unwrap_or(versions);
    let version = versions.trim().trim_end_matches(',').trim().strip_prefix("==")?.trim();
    let exact = version.starts_with(|c: char| c.is_ascii_digit())
        && version.chars().all(|c| c.is_ascii_alphanumeric() || ".-_+!".contains(c));

    exact.then(|| Req { name: name.to_owned(), name_at: base + lead, version: version.to_owned(), marker })
}

fn groups_reach(
    groups: &HashMap<String, Vec<Item>>,
    name: &str,
    stack: &mut Vec<String>,
    done: &mut HashSet<String>,
    out: &mut BTreeSet<usize>,
) -> bool {
    if stack.iter().any(|s| s == name) || stack.len() >= MAX_DEPTH {
        return false;
    }
    if done.contains(name) {
        return true;
    }
    let Some(entries) = groups.get(name) else { return false };
    stack.push(name.to_owned());
    for entry in entries {
        match entry {
            Item::Req(index) => {
                out.insert(*index);
            }
            Item::Include(included) => {
                if !groups_reach(groups, &normalise_pypi(included), stack, done, out) {
                    stack.pop();

                    return false;
                }
            }
        }
    }
    stack.pop();
    done.insert(name.to_owned());

    true
}

pub(crate) fn parse(path: &str, text: &str) -> (Vec<Dep>, Vec<Note>) {
    let mut reqs: Vec<Req> = Vec::new();
    let mut chosen: BTreeSet<usize> = BTreeSet::new();
    let mut groups: HashMap<String, Vec<Item>> = HashMap::new();
    let mut ambiguous: HashSet<String> = HashSet::new();
    for array in arrays(text) {
        let in_groups = array.section == "dependency-groups";
        let direct = (array.section == "project" && array.key == "dependencies") || array.section == "project.optional-dependencies";
        if !in_groups && !direct {
            continue;
        }
        let mut entries = Vec::new();
        for (range, include) in array.items {
            if let Some(group) = include {
                entries.push(Item::Include(group));
            } else if let Some((from, to)) = range
                && let Some(req) = requirement(&text[from..to], from)
            {
                reqs.push(req);
                entries.push(Item::Req(reqs.len() - 1));
            }
        }
        match in_groups {
            true => {
                let name = normalise_pypi(&array.key);
                if groups.insert(name.clone(), entries).is_some() {
                    ambiguous.insert(name);
                }
            }
            false => chosen.extend(entries.iter().filter_map(|e| match e {
                Item::Req(index) => Some(*index),
                Item::Include(_) => None,
            })),
        }
    }
    for name in groups.keys().cloned().collect::<Vec<_>>() {
        let mut out = BTreeSet::new();
        if !ambiguous.contains(&name) && groups_reach(&groups, &name, &mut Vec::new(), &mut HashSet::new(), &mut out) {
            chosen.extend(out);
        }
    }

    let mut deps = Vec::new();
    let mut notes = Vec::new();
    for index in chosen {
        let req = &reqs[index];
        deps.push(Dep {
            path: path.to_owned(),
            ecosystem: Ecosystem::PyPI,
            name: req.name.clone(),
            version: req.version.clone(),
            at: (req.name_at, req.name_at + req.name.len()),
            version_at: None,
        });
        if req.marker {
            notes.push(Note {
                path: path.to_owned(),
                ecosystem: Ecosystem::PyPI,
                name: req.name.clone(),
                spec: req.version.clone(),
                label: MARKER,
            });
        }
    }

    (deps, notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"[project]
name = "x"
description = """
[dependencies]
"""
dependencies = [
  "Django==4.2.1",
  "requests>=2",            # range: skipped
  "click[extra] == 8.0.0 ; python_version < '3.9' and (os_name == 'nt' or extra == 'x')",
  "bad @ https://example.com/b.whl",
  "weird==1.*",
  "broken==1.0 ; python_version <",
]

[project.optional-dependencies]
docs = ["sphinx==7.0.0"]

[dependency-groups]
test = ["pytest==8.0.0", {include-group = "lint"}]
lint = ["ruff==0.1.0"]
all = [{include-group = "test"}, "coverage==7.0.0"]
loop-a = [{include-group = "loop-b"}, "loopdep==1.0"]
loop-b = [{include-group = "loop-a"}]
dangling = [{include-group = "nope"}, "orphan==1.0"]
"#;

    #[test]
    fn reads_exact_pins_groups_includes_and_skips_cycles_and_unresolvable_groups() {
        let (deps, notes) = parse("/p/pyproject.toml", DOC);
        let mut found: Vec<_> = deps.iter().map(|d| (d.name.as_str(), d.version.as_str())).collect();
        found.sort();
        assert_eq!(
            found,
            [("Django", "4.2.1"), ("click", "8.0.0"), ("coverage", "7.0.0"), ("pytest", "8.0.0"), ("ruff", "0.1.0"), ("sphinx", "7.0.0")]
        );
        let django = deps.iter().find(|d| d.name == "Django").unwrap();
        assert_eq!(&DOC[django.at.0..django.at.1], "Django");
        assert_eq!(notes.len(), 1);
        assert_eq!((notes[0].name.as_str(), notes[0].label), ("click", MARKER));
    }

    #[test]
    fn marker_grammar_is_bounded_and_strict() {
        assert!(valid_marker("python_version >= '3.8' and sys_platform != \"win32\""));
        assert!(valid_marker("'a' not in platform_release or (extra == 'x')"));
        assert!(!valid_marker("python_version"));
        assert!(!valid_marker("3.4 < python_version < 3.9"));
        assert!(!valid_marker("unknown_var == '1'"));
        assert!(!valid_marker(&format!("{}os_name == 'a'{}", "(".repeat(40), ")".repeat(40))));
    }

    #[test]
    fn half_typed_documents_never_panic() {
        for end in 0..DOC.len().min(900) {
            if DOC.is_char_boundary(end) {
                let _ = parse("/p/pyproject.toml", &DOC[..end]);
            }
        }
    }
}
