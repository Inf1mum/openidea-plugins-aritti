//! Static registered Livewire and Laravel view namespaces remain distinct.
//! All inputs are captured in the native preview evidence; no application code runs.
use super::*;
use crate::eloquent_members::metadata_inference as inference;
use std::ops::Range;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Registration {
    pub view_path: Option<String>,
    pub class_namespace: Option<String>,
    pub class_path: Option<String>,
    pub class_view_path: Option<String>,
}
#[derive(Default)]
pub(super) struct Catalog {
    entries: BTreeMap<String, Registration>,
    views: BTreeMap<String, BTreeSet<String>>,
    invalid: BTreeSet<String>,
}
fn error() -> Error {
    invalid(
        "Livewire namespace generation requires an unambiguous literal registered namespace and matching Laravel view mapping",
    )
}
fn tag(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
fn normalized(value: &str, root: &str) -> Option<String> {
    let value = value.strip_prefix(&format!("{root}/")).unwrap_or(value);
    if value.starts_with('/') || value.contains(['\\', '\0']) {
        return None;
    }
    let mut parts = vec![];
    for part in value.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    let path = parts.join("/");
    generator_plan::relative(&path).then_some(path)
}
fn path(
    source: &str,
    range: Range<usize>,
    origin: &str,
    root: &str,
    depth: usize,
) -> Option<String> {
    if depth > 8 {
        return None;
    }
    let range = php::trimmed(source, range);
    let masked = crate::package_facts::code(source)?;
    let bytes = source.as_bytes();
    let mut pieces = vec![];
    let mut start = range.start;
    let mut at = start;
    while at < range.end {
        if matches!(bytes[at], b'\'' | b'"') {
            at = php::string_end(bytes, at);
            continue;
        }
        if bytes[at] == b'(' {
            at = php::closing(&masked, at)? + 1;
            continue;
        }
        if bytes[at] == b'.' {
            pieces.push(start..at);
            start = at + 1;
        }
        at += 1;
    }
    if !pieces.is_empty() {
        pieces.push(start..range.end);
        let mut out = String::new();
        for piece in pieces {
            out.push_str(&path(source, piece, origin, root, depth + 1)?);
        }
        return (out.len() <= 4096).then_some(out);
    }
    if php::literal(source, range.clone()).is_some() {
        return php_string(source, range).ok();
    }
    let raw = masked[range.clone()].trim();
    if raw.eq_ignore_ascii_case("__DIR__") {
        return Some(format!("{root}/{}", origin.rsplit_once('/')?.0));
    }
    if raw.eq_ignore_ascii_case("__FILE__") {
        return Some(format!("{root}/{origin}"));
    }
    let open = range.start + masked[range.clone()].find('(')?;
    let call = inference::call_identity(source, open)?;
    if call.class.is_some() || php::closing(&masked, open)? + 1 != range.end {
        return None;
    }
    let args = php::arguments(source, open);
    let name = ["dirname", "base_path", "resource_path", "app_path"]
        .into_iter()
        .find(|name| inference::global_helper(source, open, name))?;
    if name == "dirname" {
        if args.is_empty() || args.len() > 2 {
            return None;
        }
        let mut p = path(source, args[0].clone(), origin, root, depth + 1)?;
        let n = if args.len() == 2 {
            source[args[1].clone()].trim().parse::<usize>().ok()?
        } else {
            1
        };
        if n == 0 || n > 8 {
            return None;
        }
        for _ in 0..n {
            p = p.rsplit_once('/')?.0.into();
        }
        return Some(p);
    }
    let base = match name {
        "base_path" => root.to_owned(),
        "resource_path" => format!("{root}/resources"),
        "app_path" => format!("{root}/app"),
        _ => return None,
    };
    if args.len() > 1 {
        return None;
    }
    let suffix = if args.is_empty() {
        String::new()
    } else {
        php_string(source, args[0].clone()).ok()?
    };
    Some(if suffix.is_empty() {
        base
    } else {
        format!("{base}/{suffix}")
    })
}
fn owned_path(
    sources: &mut Sources,
    source: &str,
    range: Range<usize>,
    origin: &str,
) -> Option<String> {
    let p = normalized(
        &path(source, range, origin, &sources.root, 0)?,
        &sources.root,
    )?;
    let absolute = format!("{}/{p}", sources.root);
    if !crate::owns_current_path(sources.project, &absolute) {
        return None;
    }
    let mut prefix = String::new();
    for part in p.split('/') {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        if sources
            .project
            .stat(&prefix)
            .is_ok_and(|s| s.kind == openidea_plugin::FileKind::Symlink)
        {
            return None;
        }
    }
    Some(p)
}
fn args(source: &str, open: usize, names: &[&str]) -> Option<BTreeMap<String, Range<usize>>> {
    let mut out = BTreeMap::new();
    let mut named = false;
    for (i, r) in php::arguments(source, open).into_iter().enumerate() {
        let r = php::trimmed(source, r);
        let raw = &source[r.clone()];
        let pair = raw.find(':').filter(|at| {
            raw[..*at]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        });
        let (key, value) = if let Some(at) = pair {
            named = true;
            (
                raw[..at].to_owned(),
                php::trimmed(source, r.start + at + 1..r.end),
            )
        } else {
            if named {
                return None;
            }
            (names.get(i)?.to_string(), r)
        };
        if !names.contains(&key.as_str()) || out.insert(key, value).is_some() {
            return None;
        }
    }
    Some(out)
}
fn boot_call(source: &str, open: usize) -> bool {
    let Some(masked) = crate::package_facts::code(source) else {
        return false;
    };
    let mut found = false;
    for (at, _) in masked.match_indices("function") {
        if at > 0 && php::is_identifier(masked.as_bytes()[at - 1]) {
            continue;
        }
        let rest = masked[at + 8..].trim_start();
        let Some(rest) = rest.strip_prefix("boot") else {
            continue;
        };
        if !rest.trim_start().starts_with('(') {
            continue;
        }
        let Some(relative) = masked[at..].find('{') else {
            continue;
        };
        let body = at + relative;
        let Some(end) = php::closing(&masked, body) else {
            continue;
        };
        if body < open && open < end {
            let prefix = &masked[body + 1..open];
            let depth = prefix.bytes().fold(0i32, |d, b| match b {
                b'{' => d + 1,
                b'}' => d - 1,
                _ => d,
            });
            if depth != 0 {
                return false;
            }
            let words: Vec<_> = prefix
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .collect();
            if words.iter().any(|w| {
                [
                    "if", "else", "foreach", "for", "while", "switch", "try", "return", "throw",
                    "function", "fn",
                ]
                .contains(w)
            }) {
                return false;
            }
            found = true;
        }
    }
    found
}
impl Catalog {
    fn register(&mut self, name: String, value: Registration) {
        if !tag(&name) {
            return;
        }
        let old = self.entries.entry(name.clone()).or_default();
        for (target, source) in [
            (&mut old.view_path, value.view_path),
            (&mut old.class_namespace, value.class_namespace),
            (&mut old.class_path, value.class_path),
            (&mut old.class_view_path, value.class_view_path),
        ] {
            if let Some(source) = source {
                if target.as_ref().is_some_and(|old| old != &source) {
                    self.invalid.insert(name.clone());
                } else {
                    *target = Some(source);
                }
            }
        }
    }
    pub fn selected(&self, name: Option<&str>) -> Result<Option<&Registration>, Error> {
        let Some(name) = name else { return Ok(None) };
        if self.invalid.contains(name) {
            return Err(error());
        }
        self.entries.get(name).map(Some).ok_or_else(error)
    }
    pub fn view_prefix(&self, directory: &str, settings: &Settings) -> Result<String, Error> {
        if directory == settings.application_view_root {
            return Ok(String::new());
        }
        if let Some(tail) = directory.strip_prefix(&format!("{}/", settings.application_view_root))
        {
            return Ok(tail.replace('/', "."));
        }
        let mut matches = vec![];
        for (namespace, roots) in &self.views {
            for root in roots {
                let suffix = if root == directory {
                    Some("")
                } else {
                    directory.strip_prefix(&format!("{root}/"))
                };
                if let Some(suffix) = suffix {
                    matches.push((
                        root.len(),
                        format!("{namespace}::{}", suffix.replace('/', ".")),
                    ));
                }
            }
        }
        matches.sort();
        let Some(max) = matches.last().map(|v| v.0) else {
            return Err(error());
        };
        let names: BTreeSet<_> = matches
            .into_iter()
            .filter(|v| v.0 == max)
            .map(|v| v.1)
            .collect();
        if names.len() != 1 {
            return Err(error());
        }
        Ok(names.into_iter().next().unwrap())
    }
}
pub(super) fn capture(
    sources: &mut Sources,
    settings: &Settings,
    config: &ConfigSources,
    package: &str,
    major: u32,
    need_providers: bool,
) -> Result<Catalog, Error> {
    let mut catalog = Catalog::default();
    if major == 4 {
        if let Some((source, range)) = config.entry(&["component_namespaces"])? {
            let array = php::array_open(source, range).ok_or_else(error)?;
            for item in php::items(source, array) {
                let key = php_string(source, item.key.ok_or_else(error)?)?;
                let value = owned_path(sources, source, item.value, "config/livewire.php")
                    .ok_or_else(error)?;
                catalog.register(
                    key.clone(),
                    Registration {
                        view_path: Some(value.clone()),
                        ..Default::default()
                    },
                );
                catalog.views.entry(key).or_default().insert(value);
            }
        }
    }
    if !need_providers {
        return Ok(catalog);
    }
    if sources.exists("bootstrap/cache/config.php")? {
        return Err(error());
    }
    let providers = provider_sources(sources, settings)?;
    for (origin, source) in providers {
        let Some(fact) = inference::class_fact(&source, &origin) else {
            continue;
        };
        if fact.base.as_deref() != Some("Illuminate\\Support\\ServiceProvider")
            || fact.class_kind != "class"
            || fact.adaptations
            || fact.methods.contains("loadviewsfrom")
        {
            continue;
        }
        let Some(code) = crate::package_facts::code(&source) else {
            continue;
        };
        for (open, b) in code.bytes().enumerate() {
            if b != b'(' {
                continue;
            }
            let Some(call) = inference::call_identity(&source, open) else {
                continue;
            };
            if !boot_call(&source, open) {
                if major == 4
                    && call.class.as_deref() == Some("Livewire\\Livewire")
                    && call.method.eq_ignore_ascii_case("addNamespace")
                {
                    if let Some(name) = args(
                        &source,
                        open,
                        &[
                            "namespace",
                            "viewPath",
                            "classNamespace",
                            "classPath",
                            "classViewPath",
                        ],
                    )
                    .and_then(|a| {
                        a.get("namespace")
                            .and_then(|r| php_string(&source, r.clone()).ok())
                    }) {
                        catalog.invalid.insert(name);
                    }
                }
                continue;
            }

            if major == 4
                && call.class.as_deref() == Some("Livewire\\Livewire")
                && call.method.eq_ignore_ascii_case("addNamespace")
            {
                let Some(a) = args(
                    &source,
                    open,
                    &[
                        "namespace",
                        "viewPath",
                        "classNamespace",
                        "classPath",
                        "classViewPath",
                    ],
                ) else {
                    continue;
                };
                let Some(name) = a
                    .get("namespace")
                    .and_then(|r| php_string(&source, r.clone()).ok())
                else {
                    continue;
                };
                let mut reg = Registration::default();
                let mut valid = true;
                for (key, target) in [
                    ("viewPath", &mut reg.view_path),
                    ("classPath", &mut reg.class_path),
                    ("classViewPath", &mut reg.class_view_path),
                ] {
                    if let Some(r) = a.get(key) {
                        match owned_path(sources, &source, r.clone(), &origin) {
                            Some(p) => *target = Some(p),
                            None => valid = false,
                        }
                    }
                }
                if let Some(r) = a.get("classNamespace") {
                    reg.class_namespace = php_string(&source, r.clone())
                        .ok()
                        .filter(|s| generator_plan::qualified(s));
                    if reg.class_namespace.is_none() {
                        valid = false;
                    }
                }
                if valid {
                    catalog.register(name, reg);
                } else {
                    catalog.invalid.insert(name);
                }
            } else if call.class.as_deref() == Some(fact.full.as_str())
                && call.method.eq_ignore_ascii_case("loadViewsFrom")
                && code[..open].trim_end().ends_with("$this->loadViewsFrom")
            {
                let Some(a) = args(&source, open, &["path", "namespace"]) else {
                    continue;
                };
                let Some(name) = a
                    .get("namespace")
                    .and_then(|r| php_string(&source, r.clone()).ok())
                    .filter(|s| tag(s))
                else {
                    continue;
                };
                let Some(value) = a
                    .get("path")
                    .and_then(|r| owned_path(sources, &source, r.clone(), &origin))
                else {
                    continue;
                };
                catalog.views.entry(name).or_default().insert(value);
            }
        }
    }
    // A verified installed Finder/manager signature is required before adapting namespace calls.
    if major == 4 && !catalog.entries.is_empty() {
        let finder = sources.read(&format!("{package}/src/Finder/Finder.php"), 128 << 10)?;
        let manager = sources.read(&format!("{package}/src/LivewireManager.php"), 128 << 10)?;
        if !finder.contains("function addNamespace($namespace, $viewPath = null, $classNamespace = null, $classPath = null, $classViewPath = null)")||!manager.contains("function addNamespace($namespace, $viewPath = null, $classNamespace = null, $classPath = null, $classViewPath = null)"){return Err(error());}
    }
    let _ = settings;
    Ok(catalog)
}
pub(super) fn view_key(prefix: &str, slugs: &[String]) -> String {
    if prefix.is_empty() {
        slugs.join(".")
    } else if prefix.ends_with("::") {
        format!("{prefix}{}", slugs.join("."))
    } else {
        format!("{prefix}.{}", slugs.join("."))
    }
}
fn optional(sources: &mut Sources, path: &str, limit: usize) -> Result<Option<String>, Error> {
    let mut prefix = String::new();
    for part in path.split('/') {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        if sources
            .project
            .stat(&prefix)
            .is_ok_and(|stat| stat.kind == openidea_plugin::FileKind::Symlink)
        {
            return Err(Error::Stale);
        }
    }
    if sources.exists(path)? {
        sources.read(path, limit).map(Some)
    } else {
        Ok(None)
    }
}
fn roots(value: &Value, base: &str, found: &mut Vec<(String, String)>) {
    if let Some(map) = value.pointer("/autoload/psr-4").and_then(Value::as_object) {
        for (namespace, values) in map {
            if !namespace.ends_with('\\')
                || !generator_plan::qualified(namespace.trim_end_matches('\\'))
            {
                continue;
            }
            let values: Vec<_> = if let Some(s) = values.as_str() {
                vec![s]
            } else {
                values
                    .as_array()
                    .map(|a| a.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default()
            };
            for v in values {
                let raw = if base.is_empty() {
                    v.into()
                } else {
                    format!("{base}/{v}")
                };
                if let Some(path) = normalized(&raw, "") {
                    found.push((namespace.clone(), path));
                }
            }
        }
    }
}
fn declared(value: &Value, names: &mut BTreeSet<String>) {
    if let Some(a) = value
        .pointer("/extra/laravel/providers")
        .and_then(Value::as_array)
    {
        for p in a.iter().filter_map(Value::as_str) {
            if generator_plan::qualified(p) {
                names.insert(p.trim_start_matches('\\').into());
            }
        }
    }
}
fn php_providers(source: &str, array: usize, names: &mut BTreeSet<String>) {
    for item in php::items(source, array) {
        if item.key.is_some() {
            continue;
        }
        let raw = source[item.value].trim();
        if let Some(class) = raw
            .strip_suffix("::class")
            .and_then(|c| inference::class_identity(source, c.trim()))
        {
            names.insert(class);
        }
    }
}

/// Automatic module providers require an installed, registered loader and its literal
/// default file-activator contract. Custom loaders/activators/cache manifests are withheld.
fn module_runtime(
    sources: &mut Sources,
    settings: &Settings,
    names: &BTreeSet<String>,
    base: &str,
) -> Result<bool, Error> {
    if !names.contains("Nwidart\\Modules\\LaravelModulesServiceProvider") {
        return Ok(false);
    }
    let Ok((package, _)) = sources.package("nwidart/laravel-modules", &[12], settings) else {
        return Ok(false);
    };
    if sources.exists("bootstrap/cache/modules.php")? {
        return Ok(false);
    }
    for (file, needles) in [
        (
            "src/Laravel/Module.php",
            vec![
                "function registerProviders()",
                "->load($this->get('providers', []))",
            ],
        ),
        (
            "src/Module.php",
            vec![
                "$this->registerProviders();",
                "$this->activator->hasStatus($this, true)",
            ],
        ),
        (
            "src/Activators/FileActivator.php",
            vec![
                "$this->config('statuses-file')",
                "$this->modulesStatuses[$name] === $status",
            ],
        ),
        (
            "src/LaravelModulesServiceProvider.php",
            vec!["$this->registerModules();"],
        ),
    ] {
        let source = sources.read(&format!("{package}/{file}"), 128 << 10)?;
        let masked = php::mask_comments(&source);
        if !needles.iter().all(|needle| masked.contains(needle)) {
            return Ok(false);
        }
    }
    let config = ConfigSources {
        project: optional(sources, "config/modules.php", 128 << 10)?,
        defaults: sources.read(&format!("{package}/config/config.php"), 128 << 10)?,
    };
    let Some((source, r)) = config.entry(&["activator"])? else {
        return Ok(false);
    };
    if php_string(source, r)? != "file" {
        return Ok(false);
    }
    let Some((source, r)) = config.entry(&["activators", "file", "class"])? else {
        return Ok(false);
    };
    let class = source[r]
        .trim()
        .strip_suffix("::class")
        .and_then(|raw| inference::class_identity(source, raw.trim()));
    if class.as_deref() != Some("Nwidart\\Modules\\Activators\\FileActivator") {
        return Ok(false);
    }
    let Some((source, r)) = config.entry(&["activators", "file", "statuses-file"])? else {
        return Ok(false);
    };
    if config_path(source, r, &sources.root)? != "modules_statuses.json" {
        return Ok(false);
    }
    let Some((source, r)) = config.entry(&["paths", "modules"])? else {
        return Ok(false);
    };
    let modules = config_path(source, r, &sources.root)?;
    Ok(base
        .rsplit_once('/')
        .is_some_and(|(parent, _)| parent == modules))
}
/// Registered production providers only. Discovery authority files are part of preview evidence,
/// including absent inputs, so a newly added registration invalidates a confirmed preview.
fn provider_sources(
    sources: &mut Sources,
    settings: &Settings,
) -> Result<BTreeMap<String, String>, Error> {
    let mut mappings = vec![];
    let mut names = BTreeSet::new();
    let mut disabled = BTreeSet::new();
    if let Some(source) = optional(sources, "composer.json", 128 << 10)? {
        let v = crate::strict_json::parse(&source, 128 << 10)?;
        roots(&v, "", &mut mappings);
        if let Some(a) = v
            .pointer("/extra/laravel/dont-discover")
            .and_then(Value::as_array)
        {
            disabled.extend(a.iter().filter_map(Value::as_str).map(str::to_owned));
        }
    }
    let descriptor = format!("{}/composer/installed.json", settings.vendor_root);
    if let Some(source) = optional(sources, &descriptor, 2 << 20)? {
        let v = crate::strict_json::parse(&source, 2 << 20)?;
        let packages = v
            .get("packages")
            .and_then(Value::as_array)
            .or_else(|| v.as_array());
        if let Some(packages) = packages {
            if packages.len() > 512 {
                return Err(error());
            }
            for p in packages {
                let Some(name) = p["name"].as_str() else {
                    continue;
                };
                if disabled.contains("*") || disabled.contains(name) {
                    continue;
                }
                let base = if let Some(path) = p["install-path"].as_str() {
                    normalized(&format!("{}/composer/{path}", settings.vendor_root), "")
                } else {
                    normalized(&format!("{}/{name}", settings.vendor_root), "")
                };
                if let Some(base) = base {
                    roots(p, &base, &mut mappings);
                    declared(p, &mut names);
                }
            }
        }
    }
    if let Some(manifest) = optional(sources, "bootstrap/cache/packages.php", 128 << 10)? {
        let array = php::returned_array(&manifest).ok_or_else(error)?;
        let mut registered = BTreeSet::new();
        for package in php::items(&manifest, array) {
            let Some(branch) = php::array_open(&manifest, package.value) else {
                return Err(error());
            };
            for field in php::items(&manifest, branch) {
                if field
                    .key
                    .and_then(|r| php_string(&manifest, r).ok())
                    .as_deref()
                    != Some("providers")
                {
                    continue;
                }
                let values = php::array_open(&manifest, field.value).ok_or_else(error)?;
                for item in php::items(&manifest, values) {
                    let name = php_string(&manifest, item.value)?;
                    if !generator_plan::qualified(&name) {
                        return Err(error());
                    }
                    registered.insert(name);
                }
            }
        }
        names = registered;
    }
    for file in ["bootstrap/providers.php", "config/app.php"] {
        if let Some(source) = optional(sources, file, 128 << 10)? {
            let array = if file.starts_with("bootstrap/") {
                php::returned_array(&source)
            } else {
                config_entry(&source, &["providers"])
                    .ok()
                    .flatten()
                    .and_then(|r| php::array_open(&source, r))
            };
            if let Some(array) = array {
                php_providers(&source, array, &mut names);
            }
        }
    }
    // nWidart module registration requires both an enabled status and literal module descriptor.
    // The selected source/view roots identify a candidate; they do not themselves register providers.
    let source_parts: Vec<_> = settings.source_root.split('/').collect();
    let view_parts: Vec<_> = settings.view_root.split('/').collect();
    let common = source_parts
        .iter()
        .zip(&view_parts)
        .take_while(|(a, b)| a == b)
        .count();
    if common > 0 && common < source_parts.len() {
        let base = source_parts[..common].join("/");
        let module_file = format!("{base}/module.json");
        if let Some(module) = optional(sources, &module_file, 128 << 10)? {
            let v = crate::strict_json::parse(&module, 128 << 10)?;
            let status = optional(sources, "modules_statuses.json", 128 << 10)?
                .and_then(|s| crate::strict_json::parse(&s, 128 << 10).ok());
            let enabled = v["name"].as_str().is_some_and(|name| {
                status
                    .as_ref()
                    .and_then(|v| v.get(name))
                    .and_then(Value::as_bool)
                    == Some(true)
            });
            if enabled && module_runtime(sources, settings, &names, &base)? {
                // Module-local Composer files do not themselves install a PSR-4 loader.
                // Only root/installed mappings already proven above may resolve these providers.
                if let Some(providers) = v["providers"].as_array() {
                    for p in providers.iter().filter_map(Value::as_str) {
                        if generator_plan::qualified(p) {
                            names.insert(p.into());
                        }
                    }
                }
            }
        }
    }
    if names.len() > 128 || mappings.len() > 1024 {
        return Err(error());
    }
    let mut found = BTreeMap::new();
    let mut bytes = 0usize;
    let mut checked = 0usize;
    for name in names {
        let mut candidates = BTreeMap::new();
        for (namespace, directory) in &mappings {
            let Some(tail) = name.strip_prefix(namespace) else {
                continue;
            };
            checked += 1;
            if checked > 256 {
                return Err(error());
            }
            let file = format!("{directory}/{}.php", tail.replace('\\', "/"));
            if let Some(source) = optional(sources, &file, 128 << 10)? {
                if inference::class_fact(&source, &file).is_some_and(|f| f.full == name) {
                    candidates.insert(file, source);
                }
            }
        }
        if candidates.len() == 1 {
            let (file, source) = candidates.into_iter().next().unwrap();
            bytes += source.len();
            if bytes > 2 << 20 {
                return Err(error());
            }
            found.insert(file, source);
        }
    }
    Ok(found)
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    pub(crate) fn fixture() -> (MockHost, Settings, Request) {
        let h = MockHost::install();
        h.add_file(
            "composer.json",
            r#"{"autoload":{"psr-4":{"App\\":"app/","Modules\\Blog\\":"Modules/Blog/app/"}}}"#,
        );
        h.add_file("composer.lock",r#"{"packages":[{"name":"laravel/framework","version":"v13.3.3"},{"name":"livewire/livewire","version":"v4.4.8"}]}"#);
        h.add_file(
            "vendor/laravel/framework/src/Illuminate/Foundation/Application.php",
            "<?php class Application {const VERSION = '13.3.3';}",
        );
        h.add_file(
            "vendor/livewire/livewire/composer.json",
            r#"{"name":"livewire/livewire"}"#,
        );
        h.add_file(
            "vendor/livewire/livewire/src/Component.php",
            "<?php namespace Livewire; abstract class Component {}",
        );
        h.add_file("vendor/livewire/livewire/config/livewire.php",r#"<?php return ['component_locations'=>[resource_path('views/components')],'component_namespaces'=>[],'class_namespace'=>'App\\Livewire','view_path'=>resource_path('views/livewire'),'make_command'=>['type'=>'class','emoji'=>false]];"#);
        for file in ["Finder/Finder.php", "LivewireManager.php"] {
            h.add_file(&format!("vendor/livewire/livewire/src/{file}"),r#"<?php class Adapter {private const ZAP = "\u{26A1}"; function addNamespace($namespace, $viewPath = null, $classNamespace = null, $classPath = null, $classViewPath = null) {}}"#);
        }
        let root = "vendor/livewire/livewire/src/Features/SupportConsoleCommands/Commands";
        h.add_file(&format!("{root}/livewire.stub"),"<?php namespace [namespace]; use Livewire\\Component; class [class] extends Component { public function render() { return view('[view]'); } }");
        h.add_file(&format!("{root}/livewire.view.stub"), "<div>[quote]</div>");
        h.add_file(
            &format!("{root}/livewire-sfc.stub"),
            "<?php use Livewire\\Component; new class extends Component {}; ?> <div>[quote]</div>",
        );
        h.add_file(
            &format!("{root}/livewire-mfc-class.stub"),
            "<?php use Livewire\\Component; new class extends Component {};",
        );
        h.add_file(
            &format!("{root}/livewire-mfc-view.stub"),
            "<div>[quote]</div>",
        );
        h.add_file(
            "bootstrap/providers.php",
            "<?php return [\\Modules\\Blog\\Providers\\BlogServiceProvider::class];",
        );
        h.add_file("Modules/Blog/app/Providers/BlogServiceProvider.php",&provider("Livewire::addNamespace(namespace:'blog', classNamespace:'Modules\\\\Blog\\\\Livewire', classPath:base_path('Modules/Blog/app/Livewire'), classViewPath:base_path('Modules/Blog/resources/views/livewire')); $this->loadViewsFrom(base_path('Modules/Blog/resources/views'),'blog-views');"));
        let settings = Settings {
            namespace: "Modules\\Blog".into(),
            source_root: "Modules/Blog/app".into(),
            view_root: "Modules/Blog/resources/views".into(),
            ..Settings::default()
        };
        let request=serde_json::from_value(json!({"kind":"livewire-component","name":"blog::reports.daily","componentType":"class"})).unwrap();
        (h, settings, request)
    }
    fn provider(body: &str) -> String {
        format!(
            "<?php namespace Modules\\Blog\\Providers; use Illuminate\\Support\\ServiceProvider; use Livewire\\Livewire; class BlogServiceProvider extends ServiceProvider {{public function boot(){{{body}}}}}"
        )
    }
    #[test]
    fn registered_class_module_uses_distinct_livewire_and_laravel_namespaces() {
        let (h, s, r) = fixture();
        let p = h.invocation().project().unwrap();
        let (plan, evidence) = super::super::plan(&p, &r, &s).unwrap();
        assert_eq!(plan.files.len(), 2);
        assert_eq!(
            plan.files[0].path,
            "Modules/Blog/app/Livewire/Reports/Daily.php"
        );
        assert!(
            plan.files[0]
                .text
                .contains("namespace Modules\\Blog\\Livewire\\Reports;")
        );
        assert!(
            plan.files[0]
                .text
                .contains("view('blog-views::livewire.reports.daily')")
        );
        assert_eq!(
            plan.files[1].path,
            "Modules/Blog/resources/views/livewire/reports/daily.blade.php"
        );
        assert!(evidence.contains("bootstrap/providers.php"));
        assert!(evidence.contains("BlogServiceProvider.php"));
        h.add_file("Modules/Blog/app/Providers/BlogServiceProvider.php",&provider("Livewire::addNamespace(namespace:'blog',classNamespace:'Modules\\\\Blog\\\\Livewire',classPath:base_path('Modules/Blog/app/Livewire'),classViewPath:base_path('Modules/Blog/resources/views/livewire')); $this->loadViewsFrom(base_path('Modules/Blog/resources/views'),'another-view');"));
        let (next, proof) = super::super::plan(&p, &r, &s).unwrap();
        assert_ne!(proof, evidence);
        assert!(
            next.files[0]
                .text
                .contains("another-view::livewire.reports.daily")
        );
    }
    #[test]
    fn config_namespaces_drive_single_and_multi_file_finder_paths() {
        let (h, s, mut r) = fixture();
        h.add_file("bootstrap/providers.php", "<?php return [];");
        h.add_file("config/livewire.php","<?php return ['component_namespaces'=>['blog'=>base_path('Modules/Blog/resources/views/components')]];");
        for (format, ending) in [
            ("single-file", "reports/daily.blade.php"),
            ("multi-file", "reports/daily/daily.php"),
        ] {
            r.component_type = Some(format.into());
            let (plan, _) = super::super::plan(&h.invocation().project().unwrap(), &r, &s).unwrap();
            assert!(plan.files[0].path.ends_with(ending), "{:?}", plan.files);
            assert!(
                plan.files[0]
                    .path
                    .starts_with("Modules/Blog/resources/views/components/")
            );
        }
    }
    #[test]
    fn namespace_proof_refuses_dynamic_foreign_conditional_and_unregistered_sources() {
        for body in [
            "Livewire::addNamespace(namespace:'blog',viewPath:$dynamic);",
            "if ($enabled) {Livewire::addNamespace(namespace:'blog',viewPath:base_path('Modules/Blog/resources/views/components'));}",
            "\\Other\\Livewire::addNamespace(namespace:'blog',viewPath:base_path('Modules/Blog/resources/views/components'));",
            "Livewire::addNamespace(namespace:'blog',classNamespace:'Modules\\\\Blog\\\\Livewire',classPath:base_path('app/Other'),classViewPath:base_path('Modules/Blog/resources/views/livewire')); $this->loadViewsFrom(base_path('Modules/Blog/resources/views'),'blog-views');",
            "Livewire::addNamespace(namespace:'blog',classNamespace:'Modules\\\\Blog\\\\Livewire',classPath:base_path('Modules/Blog/app/Livewire'),classViewPath:base_path('Modules/Blog/resources/views/livewire'));",
        ] {
            let (h, s, r) = fixture();
            h.add_file(
                "Modules/Blog/app/Providers/BlogServiceProvider.php",
                &provider(body),
            );
            assert!(
                super::super::plan(&h.invocation().project().unwrap(), &r, &s).is_err(),
                "{body}"
            );
        }
        let (h, s, r) = fixture();
        h.add_file("bootstrap/providers.php", "<?php return [];");
        h.add_file("composer.json",r#"{"autoload":{"psr-4":{"Modules\\Blog\\":"Modules/Blog/app/"}},"extra":{"laravel":{"providers":["Modules\\Blog\\Providers\\BlogServiceProvider"]}}}"#);
        assert!(super::super::plan(&h.invocation().project().unwrap(), &r, &s).is_err());
    }
    #[test]
    fn configured_legacy_module_class_views_require_registered_laravel_namespace() {
        let (h, s, mut r) = fixture();
        h.add_file("composer.lock",r#"{"packages":[{"name":"laravel/framework","version":"v13.3.3"},{"name":"livewire/livewire","version":"v3.8.11"}]}"#);
        h.add_file("config/livewire.php",r#"<?php return ['class_namespace'=>'Modules\\Blog\\Livewire','view_path'=>base_path('Modules/Blog/resources/views/livewire')];"#);
        h.add_file(
            "Modules/Blog/app/Providers/BlogServiceProvider.php",
            &provider(
                "$this->loadViewsFrom(base_path('Modules/Blog/resources/views'),'blog-views');",
            ),
        );
        r.name = "reports.daily".into();
        let (plan, _) = super::super::plan(&h.invocation().project().unwrap(), &r, &s).unwrap();
        assert!(
            plan.files[0]
                .text
                .contains("blog-views::livewire.reports.daily")
        );
        r.name = "blog::reports.daily".into();
        assert!(super::super::plan(&h.invocation().project().unwrap(), &r, &s).is_err());
    }

    #[test]
    fn enabled_module_descriptor_is_required_and_fingerprinted() {
        let (h, s, r) = fixture();
        h.add_file("bootstrap/providers.php", "<?php return [];");

        h.add_file(
            "Modules/Blog/composer.json",
            r#"{"autoload":{"psr-4":{"Modules\\Blog\\":"app/"}}}"#,
        );
        h.add_file(
            "Modules/Blog/module.json",
            r#"{"name":"Blog","providers":["Modules\\Blog\\Providers\\BlogServiceProvider"]}"#,
        );
        h.add_file("modules_statuses.json", r#"{"Blog":false}"#);
        assert!(super::super::plan(&h.invocation().project().unwrap(), &r, &s).is_err());
        h.add_file("modules_statuses.json", r#"{"Blog":true}"#);
        assert!(
            super::super::plan(&h.invocation().project().unwrap(), &r, &s).is_err(),
            "module metadata without an installed registered runtime is not authority"
        );
        h.add_file("composer.lock",r#"{"packages":[{"name":"laravel/framework","version":"v13.3.3"},{"name":"livewire/livewire","version":"v4.4.8"},{"name":"nwidart/laravel-modules","version":"v12.0.4"}]}"#);
        h.add_file("vendor/composer/installed.json",r#"{"packages":[{"name":"nwidart/laravel-modules","extra":{"laravel":{"providers":["Nwidart\\Modules\\LaravelModulesServiceProvider"]}}}]}"#);
        h.add_file(
            "vendor/nwidart/laravel-modules/composer.json",
            r#"{"name":"nwidart/laravel-modules"}"#,
        );
        for (file, source) in [
            (
                "src/Laravel/Module.php",
                "<?php function registerProviders() { $this->repository->load($this->get('providers', [])); }",
            ),
            (
                "src/Module.php",
                "<?php $this->registerProviders(); $this->activator->hasStatus($this, true);",
            ),
            (
                "src/Activators/FileActivator.php",
                "<?php $this->config('statuses-file'); $this->modulesStatuses[$name] === $status;",
            ),
            (
                "src/LaravelModulesServiceProvider.php",
                "<?php $this->registerModules();",
            ),
            (
                "config/config.php",
                r#"<?php use Nwidart\Modules\Activators\FileActivator; return ['paths'=>['modules'=>base_path('Modules')],'activator'=>'file','activators'=>['file'=>['class'=>FileActivator::class,'statuses-file'=>base_path('modules_statuses.json')]]];"#,
            ),
        ] {
            h.add_file(&format!("vendor/nwidart/laravel-modules/{file}"), source);
        }
        let (plan, evidence) =
            super::super::plan(&h.invocation().project().unwrap(), &r, &s).unwrap();
        assert!(evidence.contains("modules_statuses.json"));
        assert!(evidence.contains("module.json"));
        assert!(
            plan.files[0]
                .text
                .contains("blog-views::livewire.reports.daily")
        );
        h.add_project("/project/Modules/Blog");
        assert!(
            super::super::plan(&h.invocation().project().unwrap(), &r, &s).is_err(),
            "nested project provider/output authority cannot leak"
        );
    }
    #[test]
    fn repeated_namespace_registrations_must_agree_and_cannot_hide_dynamic_overrides() {
        let (h, s, mut r) = fixture();
        r.component_type = Some("single-file".into());
        h.add_file("config/livewire.php","<?php return ['component_namespaces'=>['blog'=>base_path('Modules/Blog/resources/views/components')]];");
        h.add_file("Modules/Blog/app/Providers/BlogServiceProvider.php",&provider("Livewire::addNamespace(namespace:'blog',viewPath:base_path('Modules/Blog/resources/views/other'));"));
        assert!(super::super::plan(&h.invocation().project().unwrap(), &r, &s).is_err());
        h.add_file(
            "Modules/Blog/app/Providers/BlogServiceProvider.php",
            &provider("Livewire::addNamespace(namespace:'blog',viewPath:$dynamic);"),
        );
        assert!(super::super::plan(&h.invocation().project().unwrap(), &r, &s).is_err());
        h.add_file("Modules/Blog/app/Providers/BlogServiceProvider.php",&provider("if ($enabled) Livewire::addNamespace(namespace:'blog',viewPath:base_path('Modules/Blog/resources/views/components'));"));
        assert!(super::super::plan(&h.invocation().project().unwrap(), &r, &s).is_err());
    }
    #[test]
    #[ignore = "requires LARAVEL_NATIVE_PACKAGE_FIXTURE with installed Livewire 4; executes only fixture contract probes"]
    fn installed_finder_and_render_contract_for_registered_module_namespace() {
        let fixture_root =
            std::env::var("LARAVEL_NATIVE_PACKAGE_FIXTURE").expect("installed fixture");
        let (h, s, r) = fixture();
        let package = "vendor/livewire/livewire";
        for tail in [
            "src/Finder/Finder.php",
            "src/LivewireManager.php",
            "src/Features/SupportConsoleCommands/Commands/livewire.stub",
            "src/Features/SupportConsoleCommands/Commands/livewire.view.stub",
            "src/Features/SupportConsoleCommands/Commands/livewire-sfc.stub",
            "src/Features/SupportConsoleCommands/Commands/livewire-mfc-class.stub",
            "src/Features/SupportConsoleCommands/Commands/livewire-mfc-view.stub",
        ] {
            let text = std::fs::read_to_string(format!("{fixture_root}/{package}/{tail}")).unwrap();
            h.add_file(&format!("{package}/{tail}"), &text);
        }
        let (plan, _) = super::super::plan(&h.invocation().project().unwrap(), &r, &s).unwrap();
        let target = std::env::temp_dir().join(format!(
            "laravel-native-livewire-namespaces-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&target);
        std::fs::create_dir_all(&target).unwrap();
        for output in &plan.files {
            let dest = target.join(&output.path);
            std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
            std::fs::write(dest, &output.text).unwrap();
        }
        let script = r#"<?php
require $argv[1].'/vendor/autoload.php';
$root=$argv[2];
$app=new \Illuminate\Foundation\Application($root);
$app->instance('config',new \Illuminate\Config\Repository(['view'=>['paths'=>[$root.'/resources/views'],'compiled'=>$root.'/compiled'],'livewire'=>['make_command'=>['emoji'=>false]]]));
$app->register(\Illuminate\Filesystem\FilesystemServiceProvider::class);
$app->register(\Illuminate\View\ViewServiceProvider::class);
$app['view']->addNamespace('blog-views',$root.'/Modules/Blog/resources/views');
$finder=new \Livewire\Finder\Finder;
$finder->addNamespace('blog',classNamespace:'Modules\\Blog\\Livewire',classPath:$root.'/Modules/Blog/app/Livewire',classViewPath:$root.'/Modules/Blog/resources/views/livewire');
$paths=$finder->resolveClassComponentFilePaths('blog::reports.daily');
if ($paths['class']!==$root.'/Modules/Blog/app/Livewire/Reports/Daily.php'||$paths['view']!==$root.'/Modules/Blog/resources/views/livewire/reports/daily.blade.php')throw new \Exception('Finder class/view path mismatch');
require $paths['class'];
if ($finder->resolveClassComponentClassName('blog::reports.daily')!==\Modules\Blog\Livewire\Reports\Daily::class)throw new \Exception('Finder runtime component tag mismatch');
$view=(new \Modules\Blog\Livewire\Reports\Daily)->render();
if (!$view instanceof \Illuminate\Contracts\View\View||$view->getName()!=='blog-views::livewire.reports.daily'||!$app['view']->exists($view->getName()))throw new \Exception('separate Laravel namespace render contract mismatch');
$finder->addNamespace('blog-pages',viewPath:$root.'/Modules/Blog/resources/views/components');
if ($finder->resolveSingleFileComponentPathForCreation('blog-pages::reports.daily',false)!==$root.'/Modules/Blog/resources/views/components/reports/daily.blade.php')throw new \Exception('SFC Finder path mismatch');
if ($finder->resolveMultiFileComponentPathForCreation('blog-pages::reports.daily',false)!==$root.'/Modules/Blog/resources/views/components/reports/daily')throw new \Exception('MFC Finder path mismatch');
echo 'Finder class/SFC/MFC paths and distinct Laravel view render contract pass';
"#;
        let path = target.join("probe.php");
        std::fs::write(&path, script).unwrap();
        let result = std::process::Command::new(
            std::env::var("LARAVEL_NATIVE_PHP").unwrap_or_else(|_| "php".into()),
        )
        .arg(&path)
        .arg(&fixture_root)
        .arg(&target)
        .output()
        .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(String::from_utf8_lossy(&result.stdout).contains("contract pass"));
        let _ = std::fs::remove_dir_all(target);
    }
}
