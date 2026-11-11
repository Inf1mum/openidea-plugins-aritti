//! Exact reader for the literal block-style `twig.paths` mapping of `config/packages/twig.yaml`.
//! Anything else (anchors, flow style, tabs, nesting, duplicates, parameters other than a leading
//! `%kernel.project_dir%/`) refuses the whole block. `when@env` and `packages/<env>/` overrides are not read.

const MAX_BYTES: usize = 16 << 10;
const MAX_ENTRIES: usize = 32;

/// (project-relative directory, namespace); `None` namespace is the main namespace.
pub fn paths(text: &str) -> Vec<(String, Option<String>)> {
    parse(text).unwrap_or_default()
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn strip_comment(line: &str) -> &str {
    let mut quote = None;
    let mut prev = ' ';
    for (i, c) in line.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '\'' || c == '"' => quote = Some(c),
            None if c == '#' && prev == ' ' => return line[..i].trim_end(),
            None => {}
        }
        prev = c;
    }
    line.trim_end()
}

fn scalar(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if let Some(first) = raw.chars().next() {
        if first == '\'' || first == '"' {
            let inner = raw.strip_prefix(first)?.strip_suffix(first)?;
            if inner.contains([first, '\\']) {
                return None;
            }
            return Some(inner.to_owned());
        }
        if "&*{}[]|>!%@`,?-".contains(first) && !(first == '%' || first == '-') {
            return None;
        }
    }
    if raw.contains([':', '#', '{', '}', '[', ']', '&', '*', '\'', '"', '\\']) && !raw.starts_with('%') {
        return None;
    }
    Some(raw.to_owned())
}

fn split_entry(line: &str) -> Option<(String, String)> {
    let (key, value) = if let Some(q) = line.chars().next().filter(|c| *c == '\'' || *c == '"') {
        let end = line[1..].find(q)? + 1;
        let rest = line[end + 1..].strip_prefix(':')?;
        (line[..=end].to_owned(), rest.to_owned())
    } else {
        let (k, v) = line.split_once(':')?;
        (k.to_owned(), v.to_owned())
    };
    if !value.is_empty() && !value.starts_with(' ') {
        return None;
    }
    Some((scalar(&key)?, value.trim().to_owned()))
}

fn parse(text: &str) -> Option<Vec<(String, Option<String>)>> {
    if text.len() > MAX_BYTES || text.contains(['\t', '\r']) {
        return None;
    }
    let mut lines = text.lines().map(strip_comment).filter(|l| !l.trim().is_empty() && !l.starts_with("---"));
    lines.find(|l| *l == "twig:")?;
    let mut child = None;
    let mut entry_indent = None;
    let mut in_paths = false;
    let mut found = false;
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    for line in lines {
        let depth = indent(line);
        if depth == 0 {
            break;
        }
        let child_depth = *child.get_or_insert(depth);
        let body = line.trim_start();
        if depth == child_depth {
            in_paths = false;
            let (key, value) = split_entry(body)?;
            if key == "paths" {
                if found || !value.is_empty() {
                    return None;
                }
                found = true;
                in_paths = true;
            }
            continue;
        }
        if depth < child_depth {
            return None;
        }
        if !in_paths {
            continue;
        }
        if *entry_indent.get_or_insert(depth) != depth {
            return None;
        }
        let (dir, ns) = split_entry(body)?;
        let dir = dir.strip_prefix("%kernel.project_dir%/").unwrap_or(&dir).to_owned();
        if dir.contains('%') || out.iter().any(|(d, _)| *d == dir) || out.len() >= MAX_ENTRIES {
            return None;
        }
        let namespace = match ns.as_str() {
            "" | "~" | "null" => None,
            value => {
                let value = scalar(value)?;
                if value.contains('%') {
                    return None;
                }
                Some(value)
            }
        };
        out.push((dir, namespace));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::paths;

    fn some(dir: &str, ns: Option<&str>) -> (String, Option<String>) {
        (dir.to_owned(), ns.map(str::to_owned))
    }

    #[test]
    fn reads_the_documented_block_mapping() {
        let text = "twig:\n    default_path: '%kernel.project_dir%/templates'\n    paths:\n        'email/default/templates': ~\n        'backend/templates': 'admin'\n        \"%kernel.project_dir%/lib/ui\": ui # note\n        plain/dir: null\n    strict_variables: true\nwhen@test:\n    twig:\n        paths: {}\n";
        assert_eq!(
            paths(text),
            vec![some("email/default/templates", None), some("backend/templates", Some("admin")), some("lib/ui", Some("ui")), some("plain/dir", None)]
        );
    }

    #[test]
    fn refuses_anchors_flow_parameters_nesting_duplicates_and_tabs() {
        for body in [
            "paths: { a: b }",
            "paths:\n        a: &x b",
            "paths:\n        a: *x",
            "paths:\n        a: '%env(NS)%'",
            "paths:\n        '%kernel.cache_dir%/a': b",
            "paths:\n        '%env(DIR)%': b",
            "paths:\n        a: b\n        a: c",
            "paths:\n        a: b\n          c: d",
            "paths:\n        a: b\n      c: d",
            "paths:\n        a: [b]",
            "paths:\n        a: |\n          b",
            "paths:\n        a: b\n    paths:\n        c: d",
            "paths:\n\ta: b",
        ] {
            assert!(paths(&format!("twig:\n    {body}\n")).is_empty(), "{body}");
        }
        assert!(paths("framework:\n    paths:\n        a: b\n").is_empty());
        assert!(paths(&format!("twig:\n    paths:\n        a: b\n#{}", "x".repeat(20 << 10))).is_empty());
    }
}
