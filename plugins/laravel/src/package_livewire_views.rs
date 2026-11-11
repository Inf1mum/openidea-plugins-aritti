//! Livewire 4 Finder/config-backed single- and multi-file components. Only owned
//! anonymous classes bound to Livewire\Component become source facts.
use super::*;
fn returned(source: &str) -> Option<usize> {
    let code = crate::package_facts::code(source)?;
    for (at, _) in code.match_indices("return") {
        if at > 0 && php::is_identifier(code.as_bytes()[at - 1]) {
            continue;
        }
        if code
            .as_bytes()
            .get(at + 6)
            .is_some_and(|b| php::is_identifier(*b))
        {
            continue;
        }
        let depth = code[..at].bytes().fold(0usize, |d, b| match b {
            b'{' => d + 1,
            b'}' => d.saturating_sub(1),
            _ => d,
        });
        if depth != 0 {
            continue;
        }
        let start = at + 6;
        let rest = source[start..].trim_start();
        let open = source.len() - rest.len();
        let array = php::array_open(source, open..source.len())?;
        let close = php::closing(source, array)?;
        if !source[close + 1..].trim_start().starts_with(';') {
            return None;
        }
        return Some(array);
    }
    None
}
fn field(source: &str, key: &str) -> Option<std::ops::Range<usize>> {
    let array = returned(source)?;
    php::items(source, array)
        .into_iter()
        .find(|item| {
            item.key
                .clone()
                .and_then(|r| php::literal(source, r))
                .is_some_and(|(k, _)| k == key)
        })
        .map(|i| i.value)
}
fn root_value(source: &str, range: std::ops::Range<usize>, root: &str) -> Option<String> {
    if let Some((path, _)) = literal(source, range.clone()) {
        let path = path.strip_prefix(&format!("{root}/")).unwrap_or(path);
        return clean(path).then(|| path.into());
    }
    let raw = source[range.clone()].trim();
    let helper = raw.split('(').next()?.trim();
    let base = match helper {
        "resource_path" => "resources",
        "base_path" => "",
        "app_path" => "app",
        _ => return None,
    };
    let open = range.start + source[range.clone()].find('(')?;
    if php::closing(source, open) != Some(php::trimmed(source, range.clone()).end - 1) {
        return None;
    }
    let call = crate::eloquent_members::metadata_inference::call_identity(source, open)?;
    if call.class.is_some() || call.function_fqn.as_deref().is_some_and(|f| f != helper) {
        return None;
    }
    let args = php::arguments(source, open);
    if args.len() != 1 {
        return None;
    }
    let (path, _) = literal(source, args[0].clone())?;
    normalize(base, path)
}
fn zap(value: &str) -> String {
    value
        .replace("⚡\u{fe0e}", "")
        .replace("⚡\u{fe0f}", "")
        .replace('⚡', "")
}
fn bounds(source: &str) -> Option<(String, usize, usize)> {
    let code = super::livewire::volt_code(source)?;
    let mut candidates = Vec::new();
    for (at, _) in code.match_indices("new") {
        if at > 0 && php::is_identifier(code.as_bytes()[at - 1]) {
            continue;
        }
        let mut start = at + 3;
        while code
            .as_bytes()
            .get(start)
            .is_some_and(u8::is_ascii_whitespace)
        {
            start += 1;
        }
        while code[start..].starts_with("#[") {
            start = php::closing(source, start + 1)? + 1;
            while code
                .as_bytes()
                .get(start)
                .is_some_and(u8::is_ascii_whitespace)
            {
                start += 1;
            }
        }
        if !code[start..]
            .strip_prefix("class")
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
        {
            continue;
        }
        let body = code[start..].find('{')? + start;
        let header = &code[start + 5..body];
        let raw = header.trim_start().strip_prefix("extends")?;
        if !raw.starts_with(char::is_whitespace) {
            continue;
        }
        let parent = raw.trim();
        if !parent
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'\\'))
        {
            continue;
        }
        if crate::eloquent_members::metadata_inference::class_identity(
            super::livewire::php_source(source)?,
            parent,
        )
        .as_deref()
            != Some("Livewire\\Component")
        {
            continue;
        }
        let end = php::closing(source, body)?;
        candidates.push((body, end));
    }
    if candidates.len() != 1 {
        return None;
    }
    let (body, end) = candidates.pop()?;
    Some((code, body, end))
}
pub(super) fn class(source: &str, path: &str) -> Option<(Vec<String>, Names)> {
    let (code, body, end) = bounds(source)?;
    let props = crate::components::livewire_props(source)
        .into_iter()
        .map(|p| p.name)
        .collect();
    let mut methods = Names::new();
    let mut depth = 0usize;
    for (at, b) in code[body..end + 1].bytes().enumerate() {
        let at = body + at;
        match b {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b'f' if depth == 1 && code[at..].starts_with("function ") => {
                let start = at + 9;
                let length = code[start..]
                    .bytes()
                    .take_while(|b| php::is_identifier(*b))
                    .count();
                let name = &code[start..start + length];
                let bound = code[..at]
                    .rfind([';', '{', '}'])
                    .map_or(body + 1, |i| i + 1);
                let modifiers = &code[bound..at];
                if name.is_empty()
                    || name.starts_with("__")
                    || modifiers
                        .split_whitespace()
                        .any(|m| ["private", "protected", "static"].contains(&m))
                {
                    continue;
                }
                methods.insert(
                    name.into(),
                    entry(path, source, start, name, "Livewire view component action"),
                );
            }
            _ => {}
        }
    }
    Some((props, methods))
}
pub(super) fn apply(
    read: &mut Read<'_>,
    index: &mut Index,
    roots: &[ComposerRoot],
    facts: &mut Facts,
) {
    let Some(major) = index
        .metadata
        .package_major("livewire/livewire")
        .filter(|v| (2..=4).contains(v))
    else {
        return;
    };
    configured_classes(read, index, roots, facts);
    if major != 4 {
        return;
    }

    let Some((finder, source)) = source_for(
        read,
        roots,
        "Livewire\\Finder\\Finder",
        Some("livewire/livewire"),
    ) else {
        return;
    };
    let Some(code) = crate::package_facts::code(&source) else {
        return;
    };
    if !code.contains("function resolveSingleFileComponentPath(")
        || !code.contains("function resolveMultiFileComponentPath(")
    {
        return;
    }
    index.dependency_files.insert(finder);
    let Some(mapping) = roots
        .iter()
        .find(|r| r.package == "livewire/livewire" && r.namespace == "Livewire\\")
    else {
        return;
    };
    let Some(default_path) = normalize(&mapping.directory, "../config/livewire.php") else {
        return;
    };
    let Some(defaults) = read.read(&default_path) else {
        return;
    };
    index.dependency_files.insert(default_path);
    index.dependency_files.insert("config/livewire.php".into());
    if !owned(read.project, &format!("{}/config/livewire.php", read.root)) {
        facts.issues.push(
            "Livewire config belongs to another project; component inventory is partial".into(),
        );
        return;
    }
    let project = read.read("config/livewire.php");
    if project
        .as_ref()
        .is_some_and(|source| returned(source).is_none())
    {
        facts.issues.push(
            "Livewire config is dynamically constructed; component inventory is partial".into(),
        );
        return;
    }
    let mut locations = Vec::new();
    for (key, namespaced) in [
        ("component_locations", false),
        ("component_namespaces", true),
    ] {
        let selected = project
            .as_ref()
            .and_then(|s| field(s, key).map(|r| (s.as_str(), r)))
            .or_else(|| field(&defaults, key).map(|r| (defaults.as_str(), r)));
        let Some((source, value)) = selected else {
            continue;
        };
        let Some(array) = php::array_open(source, value) else {
            facts.issues.push(format!(
                "Livewire {key} is dynamic; view component inventory is partial"
            ));
            continue;
        };
        let items = php::items(source, array);
        if items.len() > 64 {
            read.exhausted = true;
            return;
        }
        for item in items {
            let namespace = if namespaced {
                let Some((name, _)) = item.key.and_then(|r| literal(source, r)) else {
                    continue;
                };
                if !crate::refs::valid_tag_name(name, false) || name.contains("::") {
                    continue;
                }
                Some(name.to_owned())
            } else {
                None
            };
            if let Some(path) = root_value(source, item.value, &read.root) {
                locations.push((namespace, path));
            } else {
                facts.issues.push(format!("Livewire {key} contains an unresolved root; view component inventory is partial"));
            }
        }
    }
    facts.livewire_view_roots = locations.clone();
    for (namespace, root) in locations {
        index.dependency_roots.insert(root.clone());
        for path in read.find(&format!("{root}/**/*.php")) {
            let relative = path
                .strip_prefix(&format!("{}/", read.root))
                .unwrap_or(&path);
            let Some(inside) = relative.strip_prefix(&format!("{root}/")) else {
                continue;
            };
            let Some(source) = read.read(&path) else {
                continue;
            };
            let full = if path.starts_with('/') {
                path.clone()
            } else {
                format!("{}/{path}", read.root)
            };
            let Some((props, methods)) = class(&source, &full) else {
                continue;
            };
            let (name, view) = if let Some(name) = inside.strip_suffix(".blade.php") {
                (zap(name), full.clone())
            } else {
                let Some(stem) = inside.strip_suffix(".php") else {
                    continue;
                };
                let Some((directory, file)) = stem.rsplit_once('/') else {
                    continue;
                };
                if zap(directory.rsplit('/').next().unwrap_or("")) != zap(file) {
                    continue;
                }
                let view = format!("{}/{root}/{stem}.blade.php", read.root);
                if read.read(&view).is_none() {
                    continue;
                }
                (zap(directory), view)
            };
            let name = name.replace('/', ".");
            let name = namespace
                .as_ref()
                .map_or(name.clone(), |ns| format!("{ns}::{name}"));
            if !crate::refs::valid_tag_name(&name, false) {
                continue;
            }
            if index
                .livewire
                .get(&name)
                .is_some_and(|entry| entry.path != full)
            {
                continue;
            }
            let entry = Entry {
                path: full.clone(),
                at: Position::new(0, 0),
                width: 0,
                detail: Some("Livewire 4 view component".into()),
            };
            index.livewire.entry(name.clone()).or_insert(entry);
            facts.livewire_views.insert(
                name.clone(),
                Entry {
                    path: view,
                    at: Position::new(0, 0),
                    width: 0,
                    detail: Some("Livewire view source".into()),
                },
            );
            facts.livewire_view_props.insert(name.clone(), props);
            facts.livewire_view_methods.insert(name, methods);
        }
    }
}

fn configured_classes(
    read: &mut Read<'_>,
    index: &mut Index,
    roots: &[ComposerRoot],
    facts: &mut Facts,
) {
    let Some(mapping) = roots
        .iter()
        .find(|r| r.package == "livewire/livewire" && r.namespace == "Livewire\\")
    else {
        return;
    };
    let Some(default_path) = normalize(&mapping.directory, "../config/livewire.php") else {
        return;
    };
    let Some(defaults) = read.read(&default_path) else {
        return;
    };
    index.dependency_files.insert(default_path);
    index.dependency_files.insert("config/livewire.php".into());
    if !owned(read.project, &format!("{}/config/livewire.php", read.root)) {
        facts.issues.push(
            "Livewire config belongs to another project; component inventory is partial".into(),
        );
        return;
    }
    let project = read.read("config/livewire.php");
    if project
        .as_ref()
        .is_some_and(|source| returned(source).is_none())
    {
        facts.issues.push(
            "Livewire config is dynamically constructed; component inventory is partial".into(),
        );
        return;
    }
    let selected = project
        .as_ref()
        .and_then(|s| field(s, "class_namespace").map(|r| (s.as_str(), r)))
        .or_else(|| field(&defaults, "class_namespace").map(|r| (defaults.as_str(), r)));
    let Some((source, value)) = selected else {
        return;
    };
    let Some((name, at)) = php::literal(source, value) else {
        facts.issues.push(
            "Livewire class_namespace is dynamic; class component inventory is partial".into(),
        );
        return;
    };
    let Some(namespace) = crate::eloquent_members::metadata_inference::decode_literal(
        name,
        source.as_bytes()[at - 1],
    )
    .and_then(|s| class_name(&s)) else {
        return;
    };
    facts.livewire_class_namespace = Some(namespace.clone());
    if matches!(namespace.as_str(), "App\\Livewire" | "App\\Http\\Livewire") {
        return;
    }
    index.livewire.retain(|_, entry| {
        entry.detail.is_some()
            || !(entry
                .path
                .starts_with(&format!("{}/app/Livewire/", read.root))
                || entry
                    .path
                    .starts_with(&format!("{}/app/Http/Livewire/", read.root)))
    });
    for root in roots.iter().filter(|r| r.package == "project") {
        let directory = if namespace == root.namespace.trim_end_matches('\\') {
            Some(root.directory.clone())
        } else {
            namespace
                .strip_prefix(&root.namespace)
                .map(|tail| format!("{}/{}", root.directory, tail.replace('\\', "/")))
        };
        let Some(directory) = directory else {
            continue;
        };
        index.dependency_roots.insert(directory.clone());
        for path in read.find(&format!("{directory}/**/*.php")) {
            let Some(source) = read.read(&path) else {
                continue;
            };
            let Some(full) = crate::eloquent_members::declaration(&source) else {
                continue;
            };
            let Some(local) = full.strip_prefix(&format!("{namespace}\\")) else {
                continue;
            };
            let Some(code) = crate::package_facts::code(&source) else {
                continue;
            };
            let Some(at) = code.find("extends ") else {
                continue;
            };
            let parent = code[at + 8..]
                .trim_start()
                .split(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, '_' | '\\'))
                .next()
                .unwrap_or("");
            if crate::eloquent_members::metadata_inference::class_identity(&source, parent)
                .as_deref()
                != Some("Livewire\\Component")
            {
                continue;
            }
            let Some((verified, _)) = source_for(read, roots, &full, Some("project")) else {
                continue;
            };
            index.dependency_files.insert(verified.clone());
            let name = local
                .split('\\')
                .map(|part| crate::index::separated(part, '-'))
                .collect::<Vec<_>>()
                .join(".");
            index.livewire.entry(name).or_insert(Entry {
                path: format!("{}/{verified}", read.root),
                at: Position::new(0, 0),
                width: 0,
                detail: Some("Livewire configured class namespace".into()),
            });
        }
    }
}

/// A registered-namespace component whose planned files carry exactly the
/// requested local name; the namespace roots were already proven by the native plan.
pub(super) fn generated_namespaced(name: &str, plan: &crate::generator_plan::Plan) -> bool {
    let Some((_, local)) = name.split_once("::") else {
        return false;
    };
    let parts = local.split('.').collect::<Vec<_>>();
    let studly = |part: &str| {
        part.split(['-', '_'])
            .map(|word| {
                let mut chars = word.chars();
                chars.next().map_or_else(String::new, |first| {
                    format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
                })
            })
            .collect::<String>()
    };
    let class_tail = parts.iter().map(|part| studly(part)).collect::<Vec<_>>();
    let class_path = format!("/{}.php", class_tail.join("/"));
    let class_name = format!("\\{}", class_tail.join("\\"));
    let view_path = format!("/{}.blade.php", parts.join("/"));
    let multi_file_path = format!(
        "/{}/{}.php",
        parts.join("/"),
        parts.last().copied().unwrap_or_default()
    );
    plan.files.iter().any(|file| {
        let is_component_class = crate::eloquent_members::declaration(&file.text)
            .is_some_and(|class| class.ends_with(&class_name));
        (is_component_class && file.path.ends_with(&class_path))
            || zap(&file.path).ends_with(&view_path)
            || file.path.ends_with(&multi_file_path)
    })
}
pub(super) fn generated(index: &Index, name: &str, plan: &crate::generator_plan::Plan) -> bool {
    for file in &plan.files {
        if let Some(class) = crate::eloquent_members::declaration(&file.text) {
            let valid_parent = crate::package_facts::code(&file.text)
                .and_then(|code| {
                    code.find("extends ").map(|at| {
                        code[at + 8..]
                            .trim_start()
                            .split(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, '_' | '\\'))
                            .next()
                            .unwrap_or("")
                            .to_owned()
                    })
                })
                .and_then(|parent| {
                    crate::eloquent_members::metadata_inference::class_identity(&file.text, &parent)
                })
                .as_deref()
                == Some("Livewire\\Component");
            if !valid_parent {
                continue;
            }
            if let Some(namespace) = index.packages.livewire_class_namespace.as_ref() {
                if let Some(local) = class.strip_prefix(&format!("{namespace}\\")) {
                    let actual = local
                        .split('\\')
                        .map(|part| crate::index::separated(part, '-'))
                        .collect::<Vec<_>>()
                        .join(".");
                    if actual == name
                        && index
                            .packages
                            .roots
                            .iter()
                            .filter(|r| r.package == "project")
                            .any(|root| {
                                class.strip_prefix(&root.namespace).is_some_and(|tail| {
                                    file.path
                                        == format!(
                                            "{}/{}.php",
                                            root.directory,
                                            tail.replace('\\', "/")
                                        )
                                })
                            })
                    {
                        return true;
                    }
                }
            }
        }
        if class(&file.text, &file.path).is_none() {
            continue;
        }
        for (namespace, root) in &index.packages.livewire_view_roots {
            if namespace.is_some() {
                continue;
            }
            let Some(local) = file.path.strip_prefix(&format!("{root}/")) else {
                continue;
            };
            let actual = if let Some(local) = local.strip_suffix(".blade.php") {
                zap(local).replace('/', ".")
            } else if let Some(stem) = local.strip_suffix(".php") {
                let Some((directory, base)) = stem.rsplit_once('/') else {
                    continue;
                };
                if zap(directory.rsplit('/').next().unwrap_or("")) != zap(base)
                    || !plan
                        .files
                        .iter()
                        .any(|f| f.path == format!("{root}/{stem}.blade.php"))
                {
                    continue;
                }
                zap(directory).replace('/', ".")
            } else {
                continue;
            };
            if actual == name {
                return true;
            }
        }
    }
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn anonymous_component_proof_preserves_php_scope_and_ignores_template_data() {
        let source = "<?php use Livewire\\Component; new class extends Component {\npublic int $count=0;\npublic function increment() {}\n}; ?>\nIt's visible\npublic string $invented;\nfunction ghost() {}";
        let (props, methods) = class(source, "/p/c.blade.php").unwrap();
        assert_eq!(props, ["count"]);
        assert!(methods.contains_key("increment"));
        assert!(!methods.contains_key("ghost"));
        assert!(
            class(
                &source.replace("use Livewire\\Component", "use App\\Component"),
                "x"
            )
            .is_none()
        );
    }
}

pub(super) fn absence(source: &str) -> Option<(Vec<String>, usize)> {
    let (code, body, end) = bounds(source)?;
    let region = &code[body..end + 1];
    if !crate::blade::stable_attributes(super::livewire::php_source(source)?, &code, body..end + 1)
        || region.contains("__call")
    {
        return None;
    }
    let mut depth = 0usize;
    let mut methods = Vec::new();
    for (relative, b) in region.bytes().enumerate() {
        let at = body + relative;
        match b {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b'u' if depth == 1 && code[at..].starts_with("use ") => return None,
            b'f' if depth == 1 && code[at..].starts_with("function ") => {
                let name = code[at + 9..]
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .next()
                    .unwrap_or("");
                if !name.is_empty() {
                    methods.push(name.into());
                }
            }
            _ => {}
        }
    }
    Some((methods, end))
}
