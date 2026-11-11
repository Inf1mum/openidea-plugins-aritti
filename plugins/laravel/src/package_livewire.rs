//! Version-gated Livewire surface and explicit Volt functional declarations.
use crate::{
    Index,
    index::{Entry, Names},
};
use openidea_plugin::{
    Position, Range,
    providers::{CompletionItem, CompletionKind, Document},
    text,
};
pub(crate) fn completion(index: &Index, doc: &Document, at: Position) -> Vec<CompletionItem> {
    if doc.language != "blade" || doc.text.len() > 256 * 1024 {
        return vec![];
    }
    let Some(major) = index.metadata.package_major("livewire/livewire") else {
        return vec![];
    };
    if !(2..=4).contains(&major) {
        return vec![];
    }
    let Some(at) = text::offset(&doc.text, at) else {
        return vec![];
    };
    let slot = slots(index, doc, at);
    if !slot.is_empty() {
        return slot;
    }
    let visible = crate::refs::without_comments(&doc.text);
    let Some(open) = visible[..at].rfind('<') else {
        return vec![];
    };
    let header = &visible[open..at];
    if header.contains('>') || header.bytes().filter(|b| matches!(b, b'\'' | b'"')).count() % 2 != 0
    {
        return vec![];
    }
    let typed = header.rsplit(char::is_whitespace).next().unwrap_or("");
    if !typed.starts_with("wire:") {
        return vec![];
    }
    let start = at - typed.len();
    let mut names = vec![
        "wire:click",
        "wire:submit",
        "wire:model",
        "wire:loading",
        "wire:target",
        "wire:poll",
        "wire:init",
        "wire:key",
        "wire:ignore",
        "wire:dirty",
        "wire:offline",
    ];
    if major >= 3 {
        names.extend([
            "wire:navigate",
            "wire:replace",
            "wire:current",
            "wire:transition",
            "wire:confirm",
            "wire:stream",
            "wire:cloak",
            "wire:model.live",
            "wire:model.blur",
            "wire:model.change",
        ]);
    }
    if major >= 4 {
        names.extend([
            "wire:show",
            "wire:text",
            "wire:bind:",
            "wire:sort",
            "wire:sort:item",
            "wire:sort:handle",
            "wire:intersect",
            "wire:island",
            "wire:click.async",
            "wire:click.renderless",
            "wire:model.deep",
        ]);
    }
    names
        .into_iter()
        .filter(|name| name.starts_with(typed))
        .map(|name| {
            let mut item = CompletionItem::new(name, CompletionKind::Property);
            item.detail = Some(crate::ui_text::format(
                "package.detail.livewire_version",
                "Livewire {version}",
                &[("version", &major.to_string())],
            ));
            item.replace = Some(Range::new(
                text::position(&doc.text, start),
                text::position(&doc.text, at),
            ));
            item
        })
        .collect()
}
/// Extract only explicit functional state/action declarations with proven Volt
/// imports. Expressions and arbitrary user closures are never evaluated.
pub(crate) fn volt(source: &str, path: &str) -> (Vec<String>, Names) {
    let Some(code) = volt_code(source) else {
        return (vec![], Names::new());
    };
    let mut props = Vec::new();
    let mut methods = Names::new();
    let binding = php_source(source).unwrap_or(source);
    let calls: Vec<_> = code.match_indices("state").collect();
    if calls.len() > 64 || source.len().saturating_mul(calls.len()) > 4 * 1024 * 1024 {
        return (props, methods);
    }
    for (at, _) in calls {
        let depth = code[..at].bytes().fold(0usize, |d, b| match b {
            b'{' => d + 1,
            b'}' => d.saturating_sub(1),
            _ => d,
        });
        if depth != 0 {
            continue;
        }
        if at > 0 && crate::php::is_identifier(code.as_bytes()[at - 1]) {
            continue;
        }
        let rest = code[at + 5..].trim_start();
        if !rest.starts_with('(') {
            continue;
        }
        let open = code.len() - rest.len();
        let call = crate::eloquent_members::metadata_inference::call_identity(binding, open);
        if call.and_then(|c| c.function_fqn).as_deref() != Some("Livewire\\Volt\\state") {
            continue;
        }
        let args = crate::php::arguments(source, open);
        for range in &args {
            let raw = source[range.clone()].trim();
            if let Some((name, tail)) = raw.split_once(':') {
                if !tail.starts_with(':') && identifier(name) {
                    props.push(name.into());
                }
            }
        }
        let Some(arg) = args.first() else {
            continue;
        };
        if let Some(array) = crate::php::array_open(source, arg.clone()) {
            for item in crate::php::items(source, array) {
                let named = item
                    .key
                    .clone()
                    .and_then(|r| crate::php::literal(source, r))
                    .map(|k| k.0);
                let positional = item
                    .key
                    .as_ref()
                    .is_none_or(|r| source[r.clone()].trim().parse::<usize>().is_ok());
                let name = named.or_else(|| {
                    positional
                        .then(|| crate::php::literal(source, item.value).map(|p| p.0))
                        .flatten()
                });
                if let Some(name) = name.filter(|n| identifier(n)) {
                    props.push(name.into());
                }
            }
        } else if let Some((name, _)) = crate::php::literal(source, arg.clone()) {
            if identifier(name) {
                props.push(name.into());
            }
        }
    }
    // In an explicitly mounted Volt file, top-level closure variables are
    // actions even when the component has no state declarations.
    let variables = code.match_indices('$').count();
    if variables > 128 || variables.saturating_mul(source.len()) > 4 * 1024 * 1024 {
        return (props, methods);
    }
    let bytes = code.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b'$' if depth == 0 => {
                let start = i + 1;
                let mut end = start;
                while bytes
                    .get(end)
                    .is_some_and(|b| crate::php::is_identifier(*b))
                {
                    end += 1;
                }
                let name = &source[start..end];
                let rest = code[end..].trim_start();
                if identifier(name) && rest.starts_with('=') {
                    let expr = rest[1..].trim_start();
                    let direct = expr.starts_with("fn(")
                        || expr.starts_with("fn (")
                        || expr.starts_with("function(")
                        || expr.starts_with("function (");
                    if direct || wrapped(source, &code, expr, "Livewire\\Volt\\action") {
                        methods.insert(
                            name.into(),
                            Entry {
                                path: path.into(),
                                at: text::position(source, start),
                                width: name.encode_utf16().count() as u32,
                                detail: Some("Volt action".into()),
                            },
                        );
                    }
                }
                i = end.saturating_sub(1);
            }
            _ => {}
        }
        i += 1;
    }
    props.sort();
    props.dedup();
    (props, methods)
}
pub(super) fn php_source(source: &str) -> Option<&str> {
    let first = crate::blade_php::ranges(source)?
        .into_iter()
        .find(|r| source[r.clone()].starts_with("<?php"))?;
    if !source[..first.start].trim().is_empty() {
        return None;
    }
    source.get(..first.end)
}
pub(super) fn volt_code(source: &str) -> Option<String> {
    let prefix = php_source(source)?;
    let mut code = crate::package_facts::code(prefix)?.into_bytes();
    code.extend(
        source[prefix.len()..]
            .bytes()
            .map(|b| if matches!(b, b'\n' | b'\r') { b } else { b' ' }),
    );
    String::from_utf8(code).ok()
}

fn wrapped(source: &str, code: &str, expr: &str, expected: &str) -> bool {
    let Some(relative) = expr.find('(') else {
        return false;
    };
    let name = expr[..relative].trim();
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'\\'))
    {
        return false;
    }
    let open = code.len() - expr.len() + relative;
    php_source(source)
        .and_then(|binding| {
            crate::eloquent_members::metadata_inference::call_identity(binding, open)
        })
        .and_then(|c| c.function_fqn)
        .as_deref()
        == Some(expected)
}
pub(super) fn computed(source: &str, path: &str) -> Names {
    let Some(code) = volt_code(source) else {
        return Names::new();
    };
    let mut out = Names::new();
    let mut depth = 0usize;
    let vars = code.match_indices('$').count();
    if vars > 64 || vars.saturating_mul(source.len()) > 4 * 1024 * 1024 {
        return out;
    }
    for (at, b) in code.bytes().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b'$' if depth == 0 => {
                let name = code[at + 1..]
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .next()
                    .unwrap_or("");
                let tail = code[at + 1 + name.len()..].trim_start();
                if let Some(expr) = tail.strip_prefix('=').map(str::trim_start) {
                    if identifier(name) && wrapped(source, &code, expr, "Livewire\\Volt\\computed")
                    {
                        out.insert(
                            name.into(),
                            Entry {
                                path: path.into(),
                                at: text::position(source, at + 1),
                                width: name.encode_utf16().count() as u32,
                                detail: Some("Volt computed property".into()),
                            },
                        );
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn identifier(name: &str) -> bool {
    name.bytes()
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn volt_import_identity_and_closure_depth() {
        let source = "<?php use function Livewire\\Volt\\state; state(['count'=>0,'label'=>'x']); $increment=fn()=>1; function helper(){ $invented=fn()=>2; } ?>";
        let (props, methods) = volt(source, "/p/c.blade.php");
        assert_eq!(props, ["count", "label"]);
        assert!(methods.contains_key("increment"));
        assert!(!methods.contains_key("invented"));
        assert!(
            volt(
                &source.replace("Livewire\\Volt\\state", "Vendor\\state"),
                "x"
            )
            .0
            .is_empty()
        );
    }
    #[test]
    fn version_surface_does_not_leak_future_attributes() {
        let _host=openidea_plugin::mock::MockHost::install();
        let mut index = Index::default();
        let doc = Document {
            path: "/p/c.blade.php".into(),
            language: "blade".into(),
            version: 1,
            text: "<div wire:".into(),
        };
        for major in [2, 3, 4] {
            index
                .metadata
                .enabled_packages
                .insert("livewire/livewire".into(), format!("{major}.0.0"));
            let found = completion(&index, &doc, text::position(&doc.text, doc.text.len()));
            assert!(found.iter().any(|i| i.label == "wire:click"));
            assert_eq!(found.iter().any(|i| i.label == "wire:show"), major == 4);
            assert_eq!(found.iter().any(|i| i.label == "wire:navigate"), major >= 3);
        }
    }
}
fn test_target(
    index: &Index,
    doc: &Document,
    at: usize,
) -> Option<(std::ops::Range<usize>, String, bool, bool)> {
    if doc.language != "php"
        || index
            .metadata
            .package_version("livewire/livewire")
            .is_none()
    {
        return None;
    }
    let (literal, site) = crate::refs::literal_site_at("php", &doc.text, at)?;
    if site.arg != 0 || site.depth != 0 {
        return None;
    }
    let method = site.callee.strip_prefix("->")?;
    let actions = method == "call";
    if !actions && !["set", "toggle", "assertSet", "assertNotSet", "assertCount"].contains(&method)
    {
        return None;
    }
    let code = crate::package_facts::code(&doc.text)?;
    let mut end = site.callee_start;
    for _ in 0..16 {
        let before = code[..end].trim_end();
        if !before.ends_with(')') {
            return None;
        }
        let close = before.len() - 1;
        let open = code[..close]
            .match_indices('(')
            .rev()
            .find_map(|(open, _)| {
                (crate::php::closing(&doc.text, open) == Some(close)).then_some(open)
            })?;
        let call = crate::eloquent_members::metadata_inference::call_identity(&doc.text, open)?;
        let pest = call.function_fqn.as_deref() == Some("Pest\\Livewire\\livewire")
            && index
                .metadata
                .package_version("pestphp/pest-plugin-livewire")
                .is_some();
        let core = call.class.as_deref() == Some("Livewire\\Livewire") && call.method == "test";
        if pest || core {
            let args = crate::php::arguments(&doc.text, open);
            let arg = args.first()?.clone();
            let target = if let Some(class) = crate::php::class_constant(&doc.text, arg.clone()) {
                let full =
                    crate::eloquent_members::metadata_inference::class_identity(&doc.text, class)?;
                format!("class:{full}")
            } else {
                let (name, _) = crate::php::literal(&doc.text, arg)?;
                format!("name:{name}")
            };
            return Some((
                literal.start..literal.end,
                target,
                actions,
                method.starts_with("assert"),
            ));
        }
        let prefix = code[..open].trim_end();
        let arrow = prefix.rfind("->")?;
        let method = &prefix[arrow + 2..];
        if ![
            "set",
            "call",
            "assertSet",
            "assertNotSet",
            "assertSee",
            "assertHasErrors",
            "assertOk",
            "assertStatus",
            "toggle",
        ]
        .contains(&method)
        {
            return None;
        }
        end = arrow;
    }
    None
}
pub(super) fn test_names(
    index: &Index,
    doc: &Document,
    target: &str,
    actions: bool,
    readable: bool,
) -> Names {
    let mut names = Names::new();
    let Some(project) = crate::project_for(&doc.path) else {
        return names;
    };
    let mut found = Vec::new();
    let started = std::time::Instant::now();
    let mut bytes = 0;
    let mut files = 0;
    for (name, entry) in index.livewire.iter().take(128) {
        if !crate::owns_current_path(&project, &entry.path) {
            continue;
        }
        if project
            .stat(&entry.path)
            .ok()
            .is_none_or(|s| s.size > 128 * 1024)
        {
            continue;
        }
        if let Some(wanted) = target.strip_prefix("name:") {
            if wanted != name {
                continue;
            }
        }
        if let Some(wanted) = target.strip_prefix("class:") {
            if !index.packages.roots.iter().any(|root| {
                wanted.strip_prefix(&root.namespace).is_some_and(|tail| {
                    entry.path.ends_with(&format!(
                        "/{}/{}.php",
                        root.directory,
                        tail.replace('\\', "/")
                    ))
                })
            }) {
                continue;
            }
        }
        files += 1;
        if files > 32
            || bytes > 2 * 1024 * 1024
            || started.elapsed() > std::time::Duration::from_millis(100)
        {
            return Names::new();
        }
        let source = if entry.path == doc.path {
            doc.text.clone()
        } else {
            let Ok(source) = project.read_file(&entry.path) else {
                continue;
            };
            source
        };
        bytes += source.len();
        if let Some(wanted) = target.strip_prefix("class:") {
            if crate::eloquent_members::declaration(&source).as_deref() != Some(wanted) {
                continue;
            }
        }
        found.push((name, entry, source));
    }
    if found.len() != 1 {
        return names;
    }
    let (name, entry, source) = found.pop().unwrap();
    if entry.path.ends_with(".blade.php") || index.packages.livewire_views.contains_key(name) {
        let class_based = super::livewire_views::class(&source, &entry.path);
        let is_class = class_based.is_some();
        if !is_class && index.metadata.package_version("livewire/volt").is_none() {
            return names;
        }
        let (props, methods) = class_based.unwrap_or_else(|| volt(&source, &entry.path));
        if actions {
            return methods;
        }
        if props.len() > 64 || props.len().saturating_mul(source.len()) > 4 * 1024 * 1024 {
            return names;
        }
        let structural = is_class
            .then(|| crate::package_facts::code(&source))
            .flatten();
        for prop in props {
            let property = if is_class {
                structural
                    .as_ref()
                    .and_then(|code| code.find(&format!("${prop}")))
            } else {
                source.find(&format!("'{prop}'"))
            };
            if let Some(start) = property {
                names.insert(
                    prop.clone(),
                    Entry {
                        path: entry.path.clone(),
                        at: text::position(&source, start + 1),
                        width: prop.encode_utf16().count() as u32,
                        detail: Some(format!("Volt {name} state")),
                    },
                );
            }
        }
        if readable {
            names.extend(computed(&source, &entry.path));
        }
        return names;
    }
    let Some(fact) = component_class(index, &project, &source) else {
        return names;
    };
    let Some(code) = crate::package_facts::code(&source) else {
        return names;
    };
    if actions {
        let Some(declaration) = text::offset(&source, fact.entry.at) else {
            return names;
        };
        let Some(open) = code[declaration..].find('{').map(|n| n + declaration) else {
            return names;
        };
        let Some(close) = crate::php::closing(&source, open) else {
            return names;
        };
        let calls: Vec<_> = code[open..close].match_indices("function ").collect();
        if calls.len() > 64 || calls.len().saturating_mul(source.len()) > 4 * 1024 * 1024 {
            return names;
        }
        for (relative, _) in calls {
            let at = open + relative;
            let depth = code[open..at].bytes().fold(0usize, |d, b| match b {
                b'{' => d + 1,
                b'}' => d.saturating_sub(1),
                _ => d,
            });
            if depth != 1 {
                continue;
            }
            let start = at + 9;
            let len = code[start..]
                .bytes()
                .take_while(|b| crate::php::is_identifier(*b))
                .count();
            let name = &code[start..start + len];
            let bound = code[..at].rfind([';', '{', '}']).map_or(0, |n| n + 1);
            let modifiers = &code[bound..at];
            if name.is_empty()
                || name.starts_with("__")
                || ["protected", "private", "static"]
                    .iter()
                    .any(|m| modifiers.split_whitespace().any(|s| s == *m))
            {
                continue;
            }
            names.insert(
                name.into(),
                Entry {
                    path: entry.path.clone(),
                    at: text::position(&source, start),
                    width: name.encode_utf16().count() as u32,
                    detail: Some("Livewire action".into()),
                },
            );
        }
    } else {
        for prop in crate::components::livewire_props(&source) {
            if let Some(at) = code.find(&format!("${}", prop.name)) {
                names.insert(
                    prop.name.clone(),
                    Entry {
                        path: entry.path.clone(),
                        at: text::position(&source, at + 1),
                        width: prop.name.encode_utf16().count() as u32,
                        detail: Some("Livewire property".into()),
                    },
                );
            }
        }
    }
    names
}
fn component_class(
    index: &Index,
    project: &openidea_plugin::Project,
    source: &str,
) -> Option<crate::metadata::ClassFact> {
    let Some(fact) = crate::eloquent_members::metadata_inference::class_fact(source, "") else {
        return None;
    };
    if fact.class_kind != "class" {
        return None;
    }
    let mut pending = fact.parents.clone();
    let mut seen = std::collections::BTreeSet::new();
    let mut count = 0;
    while let Some(parent) = pending.pop() {
        if parent == "Livewire\\Component" {
            return Some(fact);
        }
        if !seen.insert(parent.to_ascii_lowercase()) {
            continue;
        }
        count += 1;
        if count > 8 {
            return None;
        }
        let Some(cached) = index.metadata.class_facts.get(&parent.to_ascii_lowercase()) else {
            continue;
        };
        if !crate::owns_current_path(project, &cached.path)
            || project
                .stat(&cached.path)
                .ok()
                .is_none_or(|s| s.size > 128 * 1024)
        {
            continue;
        }
        let Ok(current) = project.read_file(&cached.path) else {
            continue;
        };
        if crate::artisan_metadata::hash(&current) != cached.source_hash {
            continue;
        }
        pending.extend(cached.parents.iter().cloned());
    }
    None
}

pub(super) fn test_completion(index: &Index, doc: &Document, at: Position) -> Vec<CompletionItem> {
    let run = || {
        let at = text::offset(&doc.text, at)?;
        let (range, target, actions, readable) = test_target(index, doc, at)?;
        let names = test_names(index, doc, &target, actions, readable);
        let typed = &doc.text[range.start..at];
        Some(
            names
                .into_iter()
                .filter(|(name, _)| name.starts_with(typed))
                .map(|(name, entry)| {
                    let mut item = CompletionItem::new(&name, CompletionKind::Property);
                    item.detail = super::presentation_detail(entry.detail);
                    item.replace = Some(Range::new(
                        text::position(&doc.text, range.start),
                        text::position(&doc.text, range.end),
                    ));
                    item
                })
                .collect(),
        )
    };
    run().unwrap_or_default()
}
pub(super) fn test_definition(
    index: &Index,
    doc: &Document,
    at: Position,
) -> Vec<openidea_plugin::Location> {
    let run = || {
        let at = text::offset(&doc.text, at)?;
        let (range, target, actions, readable) = test_target(index, doc, at)?;
        let names = test_names(index, doc, &target, actions, readable);
        let entry = names.get(&doc.text[range])?;
        crate::scoped_location(&crate::project_for(&doc.path)?, entry)
    };
    run().into_iter().collect()
}
#[cfg(test)]
mod provider_tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, providers::Providers};
    #[test]
    fn pest_livewire_and_core_testing_completion_bind_to_current_component() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("artisan", "<?php");
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        host.add_file("composer.lock",r#"{"packages":[{"name":"livewire/livewire","version":"4.4.8"}],"packages-dev":[{"name":"pestphp/pest-plugin-livewire","version":"4.1.0"}]}"#);
        host.add_file("app/Livewire/Counter.php","<?php\nnamespace App\\Livewire;\nclass Counter extends \\Livewire\\Component {\n public int $count=0;\n public function increment() {}\n private function hidden() {}\n}");
        let project = host.invocation().project().unwrap();
        let index = crate::Index::build(&project);
        assert!(index.livewire.contains_key("counter"));
        let mut main = crate::Main::default();
        main.indexes.insert("/project".into(), index);
        for (source, wanted) in [
            (
                "<?php use function Pest\\Livewire\\livewire; use App\\Livewire\\Counter; livewire(Counter::class)->call('inc');",
                "increment",
            ),
            (
                "<?php use Livewire\\Livewire; Livewire::test('counter')->set('cou');",
                "count",
            ),
        ] {
            let needle = if wanted == "count" { "cou" } else { "inc" };
            let at = source.rfind(needle).unwrap() + 3;
            let doc = Document {
                path: "/project/tests/Feature/CounterTest.php".into(),
                language: "php".into(),
                version: 1,
                text: source.into(),
            };
            let found = main.completion(&doc, text::position(source, at), None);
            assert!(
                found.iter().any(|i| i.label == wanted),
                "{source}: {:?}",
                found.iter().map(|i| &i.label).collect::<Vec<_>>()
            );
            assert!(!found.iter().any(|i| i.label == "hidden"));
            let mut full = source.to_owned();
            let start = full.rfind(needle).unwrap();
            full.replace_range(start..start + needle.len(), wanted);
            let doc = Document {
                text: full.clone(),
                ..doc
            };
            assert!(
                !main
                    .definition(&doc, text::position(&full, full.rfind(wanted).unwrap() + 2))
                    .is_empty()
            );
        }
        let source = "<?php use function Vendor\\livewire; use App\\Livewire\\Counter; livewire(Counter::class)->call('inc');";
        let doc = Document {
            path: "/project/tests/Feature/CounterTest.php".into(),
            language: "php".into(),
            version: 1,
            text: source.into(),
        };
        assert!(
            !main
                .completion(
                    &doc,
                    text::position(source, source.rfind("inc").unwrap() + 3),
                    None
                )
                .iter()
                .any(|i| i.label == "increment")
        );
    }
}
fn slots(index: &Index, doc: &Document, at: usize) -> Vec<CompletionItem> {
    let run = || {
        if index.metadata.package_major("livewire/livewire") != Some(4) {
            return None;
        }
        let visible = crate::refs::without_comments(&doc.text);
        let open = visible[..at].rfind("<x-slot:")?;
        let start = open + 8;
        let typed = &visible[start..at];
        if !typed
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return None;
        }
        let mut stack = Vec::new();
        for reference in crate::refs::tag_references(&visible) {
            if reference.end > open {
                continue;
            }
            let prefix = &visible[..reference.start];
            let closing = prefix.ends_with("</livewire:") || prefix.ends_with("</x-");
            let name = &visible[reference.start..reference.end];
            if closing {
                if let Some(pos) = stack
                    .iter()
                    .rposition(|(_, n): &(crate::refs::Kind, String)| n == name)
                {
                    stack.truncate(pos);
                }
                continue;
            }
            let Some(end) = visible[reference.end..]
                .find('>')
                .map(|n| reference.end + n)
            else {
                continue;
            };
            if !visible[..end].trim_end().ends_with('/') {
                stack.push((reference.kind, name.into()));
            }
        }
        let (kind, name) = stack.pop()?;
        if kind != crate::refs::Kind::Livewire {
            return None;
        }
        let project = crate::project_for(&doc.path)?;
        let entry = index.livewire.get(&name)?;
        if !crate::owns_current_path(&project, &entry.path)
            || project.stat(&entry.path).ok()?.size > 128 * 1024
        {
            return None;
        }
        let source = project.read_file(&entry.path).ok()?;
        let view = if let Some(view) = index.packages.livewire_views.get(&name) {
            if !crate::owns_current_path(&project, &view.path)
                || project.stat(&view.path).ok()?.size > 128 * 1024
            {
                return None;
            }
            project.read_file(&view.path).ok()?
        } else if entry.path.ends_with(".blade.php") {
            source
        } else {
            let code = crate::package_facts::code(&source)?;
            let render = code.find("function render")?;
            let body = code[render..].find('{')? + render;
            let end = crate::php::closing(&source, body)?;
            let source_doc = Document {
                path: entry.path.clone(),
                language: "php".into(),
                text: source.clone(),
                version: 0,
            };
            let refs: Vec<_> = crate::refs::references("php", &source)
                .into_iter()
                .filter(|r| {
                    r.kind == crate::refs::Kind::View
                        && r.start > body
                        && r.end < end
                        && crate::view_rename::binding_safe(index, &source_doc, r)
                })
                .collect();
            if refs.len() != 1 {
                return None;
            }
            let view_name = &source[refs[0].start..refs[0].end];
            let entry = index.views.get(view_name)?;
            if !crate::owns_current_path(&project, &entry.path)
                || project.stat(&entry.path).ok()?.size > 128 * 1024
            {
                return None;
            }
            project.read_file(&entry.path).ok()?
        };
        let props = crate::components::view_props(&view);
        Some(
            crate::components::slot_names(&view, &props)
                .into_iter()
                .filter(|name| name.starts_with(typed))
                .map(|name| {
                    let mut item = CompletionItem::new(&name, CompletionKind::Property);
                    item.detail = Some(crate::ui_text::t(
                        "package.detail.livewire_slot",
                        "Livewire 4 named slot",
                    ));
                    item.replace = Some(Range::new(
                        text::position(&doc.text, start),
                        text::position(&doc.text, at),
                    ));
                    item
                })
                .collect(),
        )
    };
    run().unwrap_or_default()
}
#[cfg(test)]
mod slot_tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, providers::Providers};
    #[test]
    fn livewire_four_named_slots_follow_the_components_proven_render_view() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("artisan", "<?php");
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        host.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"livewire/livewire","version":"4.4.8"}]}"#,
        );
        host.add_file("app/Livewire/Panel.php","<?php\nnamespace App\\Livewire;\nclass Panel extends \\Livewire\\Component {function render(){return view('livewire.panel');}}");
        host.add_file(
            "resources/views/livewire/panel.blade.php",
            "<div>{{ $header }} {{ $slot }}</div>",
        );
        let index = crate::Index::build(&host.invocation().project().unwrap());
        let mut main = crate::Main::default();
        main.indexes.insert("/project".into(), index);
        let source = "<livewire:panel><x-slot:he";
        let doc = Document {
            path: "/project/resources/views/a.blade.php".into(),
            language: "blade".into(),
            version: 1,
            text: source.into(),
        };
        assert!(
            main.completion(&doc, text::position(source, source.len()), None)
                .iter()
                .any(|i| i.label == "header")
        );
        let index = main.indexes.get_mut("/project").unwrap();
        index
            .metadata
            .enabled_packages
            .insert("livewire/livewire".into(), "3.9.0".into());
        assert!(
            !main
                .completion(&doc, text::position(source, source.len()), None)
                .iter()
                .any(|i| i.label == "header")
        );
    }
}
#[cfg(test)]
mod installed_pest_tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, providers::Providers};
    #[test]
    #[ignore = "requires disposable installed Pest Livewire fixture"]
    fn installed_pest_export_and_livewire_testing_methods_match_provider_contract() {
        let folder = std::env::var("OPENIDEA_LARAVEL_PEST_FIXTURE").expect("fixture path");
        let base = std::path::Path::new(&folder);
        let source = std::fs::read_to_string(
            base.join("vendor/pestphp/pest-plugin-livewire/src/Autoload.php"),
        )
        .unwrap();
        let structural = crate::package_facts::code(&source).unwrap();
        assert!(structural.contains("namespace Pest\\Livewire;"));
        assert!(structural.contains("function livewire("));
        let source = std::fs::read_to_string(
            base.join("vendor/livewire/livewire/src/Features/SupportTesting/Testable.php"),
        )
        .unwrap();
        let structural = crate::package_facts::code(&source).unwrap();
        assert!(structural.contains("function call("));
        assert!(structural.contains("function set("));
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("artisan", "<?php");
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        host.add_file(
            "composer.lock",
            &std::fs::read_to_string(base.join("composer.lock")).unwrap(),
        );
        host.add_file("app/Livewire/Counter.php","<?php\nnamespace App\\Livewire;\nclass Counter extends \\Livewire\\Component {\npublic function increment() {}\n}");
        let index = crate::Index::build(&host.invocation().project().unwrap());
        assert_eq!(
            index
                .metadata
                .package_version("pestphp/pest-plugin-livewire"),
            Some("v4.1.0")
        );
        let mut main = crate::Main::default();
        main.indexes.insert("/project".into(), index);
        let source = "<?php use function Pest\\Livewire\\livewire; use App\\Livewire\\Counter; livewire(Counter::class)->call('inc');";
        let doc = Document {
            path: "/project/tests/CounterTest.php".into(),
            language: "php".into(),
            version: 1,
            text: source.into(),
        };
        assert!(
            main.completion(
                &doc,
                text::position(source, source.rfind("inc").unwrap() + 3),
                None
            )
            .iter()
            .any(|i| i.label == "increment")
        );
    }
}
#[cfg(test)]
mod functional_tests {
    use super::*;
    #[test]
    fn functional_forms_support_named_and_list_state_wrapped_actions_and_computed_reads() {
        let source = "<?php use function Livewire\\Volt\\{state,action,computed}; state(['first','second','count'=>fn()=>2]); state(title: query()); $save=action(fn()=>1); $total=computed(fn()=>3); ?> $invented = fn()=>false;";
        let (props, methods) = volt(source, "/p/x.blade.php");
        assert_eq!(props, ["count", "first", "second", "title"]);
        assert!(methods.contains_key("save"));
        assert!(!methods.contains_key("invented"));
        assert!(!methods.contains_key("total"));
        assert!(computed(source, "/p/x.blade.php").contains_key("total"));
        let source = "<?php $save=fn()=>1; ?>";
        assert!(volt(source, "x").1.contains_key("save"));
        let source = "<?php use function Vendor\\action; $fake=action(fn()=>1); ?>";
        assert!(!volt(source, "x").1.contains_key("fake"));
    }
}
