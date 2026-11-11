//! Package capabilities come from installed Composer descriptors and declared,
//! currently-owned source files. No package PHP is executed while indexing.
use crate::{
    Index,
    index::{AddName, Entry, Names},
    php,
};
use openidea_plugin::{
    Location, Position, Project, Range, host,
    providers::{CompletionItem, CompletionKind, Document},
    text,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};
#[path = "package_component_fixes.rs"]
mod component_fixes;
#[path = "package_dusk.rs"]
mod dusk;
#[path = "package_livewire_fixes.rs"]
mod fixes;
pub(crate) use fixes::methods_for_view as livewire_methods_for_view;
#[path = "package_livewire.rs"]
mod livewire;
#[path = "package_livewire_values.rs"]
mod livewire_values;
#[path = "package_livewire_views.rs"]
mod livewire_views;
#[path = "package_native.rs"]
mod native;
pub(crate) use native::props as native_component_props;
pub(crate) fn actions(
    index: &Index,
    doc: &Document,
    selection: Range,
) -> Vec<openidea_plugin::file_actions::FileAction> {
    let mut actions = fixes::actions(index, doc, selection);
    actions.extend(component_fixes::actions(index, doc, selection));
    actions
}
pub(crate) fn resolve(
    index: &Index,
    doc: &Document,
    action: openidea_plugin::file_actions::FileAction,
) -> Result<openidea_plugin::file_actions::FileAction, openidea_plugin::Error> {
    if action
        .data
        .as_deref()
        .is_some_and(|s| s.starts_with("livewire-action:component:"))
    {
        component_fixes::resolve(index, doc, action)
    } else {
        fixes::resolve(index, doc, action)
    }
}
const SOURCE_LIMIT: usize = 256 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ComposerRoot {
    pub namespace: String,
    pub directory: String,
    pub package: String,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Facts {
    pub source_evidence: BTreeMap<String,Option<(usize,String)>>,
    pub source_evidence_incomplete: bool,
    pub source_queries:BTreeMap<(String,u32),Option<Vec<String>>>,
    pub roots: Vec<ComposerRoot>,
    pub provider_files: BTreeSet<String>,
    pub provider_sources: BTreeMap<String, String>,
    pub pennant: Names,
    pub folio: Names,
    pub dusk_pages: Names,
    pub dusk_components: Names,
    pub dusk_selectors: BTreeMap<String, Names>,
    pub native: Names,
    pub native_partial: bool,
    pub native_element_props: BTreeMap<String, Names>,
    pub envoy: Names,
    pub volt: Names,
    pub volt_props: BTreeMap<String, Vec<String>>,
    pub volt_methods: BTreeMap<String, Names>,
    pub volt_computed: BTreeMap<String, Names>,
    pub livewire_views: Names,
    pub livewire_view_roots: Vec<(Option<String>, String)>,
    pub livewire_class_namespace: Option<String>,
    pub livewire_view_props: BTreeMap<String, Vec<String>>,
    pub livewire_view_methods: BTreeMap<String, Names>,
    pub issues: Vec<String>,
}
struct Read<'a> {
    project: &'a Project,
    root: String,
    started: Instant,
    files: usize,
    probes: usize,
    bytes: usize,
    cache: BTreeMap<String, Option<String>>,
    exhausted: bool,
    source_evidence: BTreeMap<String,Option<(usize,String)>>,
    source_evidence_incomplete: bool,
    source_queries:BTreeMap<(String,u32),Option<Vec<String>>>,
}
impl<'a> Read<'a> {
    fn new(project: &'a Project) -> Option<Self> {
        Some(Self {
            project,
            root: project.root().ok()?,
            started: Instant::now(),
            files: 0,
            probes: 0,
            bytes: 0,
            cache: BTreeMap::new(),
            exhausted: false,
            source_evidence:BTreeMap::new(),
            source_evidence_incomplete:false,
            source_queries:BTreeMap::new(),
        })
    }
    fn active(&mut self) -> bool {
        if self.started.elapsed() > Duration::from_millis(1000) {
            self.exhausted = true;
        }
        !self.exhausted
    }
    fn read(&mut self, path: &str) -> Option<String> {
        if !self.active() {
            return None;
        }
        let relative = path
            .strip_prefix(&format!("{}/", self.root))
            .unwrap_or(path);
        if let Some(source) = self.cache.get(relative) {
            return source.clone();
        }
        if self.files >= 128 {
            self.exhausted = true;
            return None;
        }
        self.files += 1;
        let source = self.capture(relative,SOURCE_LIMIT);
        self.bytes += source.as_ref().map_or(0, String::len);
        if self.bytes > 4 * 1024 * 1024 {
            self.exhausted = true;
            return None;
        }
        self.cache.insert(relative.into(), source.clone());
        source
    }
    fn exists(&mut self, path: &str) -> bool {
        if !self.active() {
            return false;
        }
        let relative = path
            .strip_prefix(&format!("{}/", self.root))
            .unwrap_or(path);
        if let Some(cached) = self.cache.get(relative) {
            return cached.is_some();
        }
        if self.probes >= 512 {
            self.exhausted = true;
            return false;
        }
        self.probes += 1;
        let full = format!("{}/{relative}", self.root);
        let exists = if clean(relative)&&owned(self.project,&full) {
            match self.project.stat(&full) {
                Ok(stat) if stat.kind==openidea_plugin::FileKind::File&&stat.size<=SOURCE_LIMIT as u64=>true,
                Err(openidea_plugin::Error::NotFound(_))=>{self.evidence(relative,None);false},
                _=>{self.source_evidence_incomplete=true;false}
            }
        }else{self.source_evidence_incomplete=true;false};
        if !exists {
            self.cache.insert(relative.into(), None);
        }
        exists
    }
    fn evidence(&mut self,relative:&str,evidence:Option<(usize,String)>){
        if self.source_evidence.get(relative).is_some_and(|old|old!=&evidence){self.source_evidence_incomplete=true;}
        self.source_evidence.insert(relative.into(),evidence);
    }
    fn capture(&mut self,relative:&str,limit:usize)->Option<String>{
        let full=format!("{}/{relative}",self.root);
        if !clean(relative)||!owned(self.project,&full){self.source_evidence_incomplete=true;return None;}
        match self.project.stat(&full){
            Err(openidea_plugin::Error::NotFound(_))=>{self.evidence(relative,None);None},
            Ok(stat) if stat.kind==openidea_plugin::FileKind::File&&stat.size<=limit as u64=>match self.project.read_file(&full){
                Ok(source) if source.len()<=limit=>{self.evidence(relative,Some((source.len(),crate::artisan_metadata::hash(&source))));Some(source)},
                _=>{self.source_evidence_incomplete=true;None}
            },
            _=>{self.source_evidence_incomplete=true;None}
        }
    }
    fn find(&mut self, glob: &str) -> Vec<String> {
        if !self.active() {
            return vec![];
        }
        let result=self.project.find_files(glob,513);
        let query=result.as_ref().ok().map(|paths|{let mut paths:Vec<_>=paths.iter().filter(|path|owned(self.project,path)).cloned().collect();paths.sort();paths.dedup();paths});
        if self.source_queries.get(&(glob.into(),513)).is_some_and(|old|old!=&query){self.source_evidence_incomplete=true;}
        self.source_queries.insert((glob.into(),513),query);
        match result {
            Ok(files) if files.len() < 513 => files,
            Ok(_) => {
                self.exhausted = true;
                vec![]
            }
            Err(_) => {self.source_evidence_incomplete=true;vec![]},
        }
    }
}
fn owned(project: &Project, path: &str) -> bool {
    host::project_of(path).and_then(|p| p.root().ok()) == project.root().ok()
}
fn clean(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', '\0', '\n', '\r'])
        && path
            .split('/')
            .all(|s| !s.is_empty() && !matches!(s, "." | ".."))
}
fn normalize(base: &str, path: &str) -> Option<String> {
    if path.starts_with('/') || path.contains(['\\', '\0', '\n', '\r']) {
        return None;
    }
    let mut parts: Vec<_> = base.split('/').filter(|p| !p.is_empty()).collect();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    let path = parts.join("/");
    clean(&path).then_some(path)
}
fn class_name(name: &str) -> Option<String> {
    let name = name.trim_matches('\\');
    (!name.is_empty()
        && name
            .split('\\')
            .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')))
    .then(|| name.into())
}
fn strings(value: &Value) -> Vec<&str> {
    value
        .as_str()
        .into_iter()
        .chain(
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        )
        .collect()
}
fn add_roots(out: &mut Vec<ComposerRoot>, package: &str, base: &str, value: &Value) {
    if let Some(map) = value.pointer("/autoload/psr-4").and_then(Value::as_object) {
        for (namespace, dirs) in map {
            let Some(namespace) = class_name(namespace) else {
                continue;
            };
            for dir in strings(dirs) {
                if let Some(directory) = normalize(base, dir) {
                    out.push(ComposerRoot {
                        namespace: format!("{namespace}\\"),
                        directory,
                        package: package.into(),
                    });
                }
            }
        }
    }
}
fn declared_roots(
    read: &mut Read<'_>,
    index: &mut Index,
) -> (Vec<ComposerRoot>, Vec<(String, String)>) {
    let mut roots = Vec::new();
    let mut providers = Vec::new();
    if let Some(source) = read.read("composer.json") {
        if let Ok(value) = crate::strict_json::parse(&source, SOURCE_LIMIT) {
            add_roots(&mut roots, "project", "", &value);
            if let Some(dev) = value.get("autoload-dev") {
                add_roots(
                    &mut roots,
                    "project",
                    "",
                    &serde_json::json!({"autoload":dev}),
                );
            }
            for provider in strings(
                value
                    .pointer("/extra/laravel/providers")
                    .unwrap_or(&Value::Null),
            ) {
                if let Some(provider) = class_name(provider) {
                    providers.push(("project".into(), provider));
                }
            }
        }
    }
    index
        .dependency_files
        .insert("vendor/composer/installed.json".into());
    let installed = format!("{}/vendor/composer/installed.json", read.root);
    // installed.json is a descriptor, not executable source. It has a separate
    // bounded 2MiB limit so normal dependency inventories are not truncated.
    let captured_installed=read.capture("vendor/composer/installed.json",2*1024*1024);
    if owned(read.project, &installed) {
        if let Ok(stat) = read.project.stat(&installed) {
            if stat.size <= 2 * 1024 * 1024 {
                if let Some(source) = captured_installed {
                    if let Ok(value) = crate::strict_json::parse(&source, 2 * 1024 * 1024) {
                        if let Some(packages) = value
                            .get("packages")
                            .and_then(Value::as_array)
                            .or_else(|| value.as_array())
                        {
                            if packages.len() > 512 {
                                read.exhausted = true;
                            } else {
                                for package in packages {
                                    let Some(name) = package.get("name").and_then(Value::as_str)
                                    else {
                                        continue;
                                    };
                                    if !clean(name) || !index.metadata.package_enabled(name, true) {
                                        continue;
                                    }
                                    let base = package
                                        .get("install-path")
                                        .and_then(Value::as_str)
                                        .and_then(|p| normalize("vendor/composer", p))
                                        .or_else(|| normalize("vendor", name));
                                    let Some(base) = base else {
                                        continue;
                                    };
                                    add_roots(&mut roots, name, &base, package);
                                    for provider in strings(
                                        package
                                            .pointer("/extra/laravel/providers")
                                            .unwrap_or(&Value::Null),
                                    ) {
                                        if let Some(provider) = class_name(provider) {
                                            providers.push((name.into(), provider));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    roots.sort_by(|a, b| (&a.namespace, &a.directory).cmp(&(&b.namespace, &b.directory)));
    roots.dedup_by(|a, b| a.namespace == b.namespace && a.directory == b.directory);
    if roots.len() > 1024 || providers.len() > 128 {
        read.exhausted = true;
        roots.clear();
        providers.clear();
    }
    (roots, providers)
}
fn source_for(
    read: &mut Read<'_>,
    roots: &[ComposerRoot],
    full: &str,
    package: Option<&str>,
) -> Option<(String, String)> {
    let mut found = BTreeMap::new();
    for root in roots {
        if package.is_some_and(|p| p != root.package) {
            continue;
        }
        let Some(tail) = full.strip_prefix(&root.namespace) else {
            continue;
        };
        let path = format!("{}/{}.php", root.directory, tail.replace('\\', "/"));
        if !read.exists(&path) {
            continue;
        }
        let Some(source) = read.read(&path) else {
            continue;
        };
        if crate::eloquent_members::declaration(&source).as_deref() == Some(full) {
            found.insert(path, source);
        }
    }
    if found.len() == 1 {
        found.into_iter().next()
    } else {
        None
    }
}
fn class_call(source: &str, open: usize, full: &str) -> bool {
    crate::eloquent_members::metadata_inference::call_identity(source, open)
        .and_then(|i| i.class)
        .is_some_and(|name| name == full)
}
fn literal(source: &str, range: std::ops::Range<usize>) -> Option<(&str, usize)> {
    let (name, start) = php::literal(source, range)?;
    (!name.is_empty() && !name.contains(['$', '\\', '\n', '\r', '\0'])).then_some((name, start))
}
fn static_calls(source: &str, method: &str) -> Vec<usize> {
    let Some(code) = crate::package_facts::code(source) else {
        return vec![];
    };
    let needle = format!("::{method}");
    code.match_indices(&needle)
        .filter_map(|(at, _)| {
            let rest = code[at + needle.len()..].trim_start();
            rest.starts_with('(').then(|| code.len() - rest.len())
        })
        .collect()
}
fn vendor_directives(
    read: &mut Read<'_>,
    index: &mut Index,
    roots: &[ComposerRoot],
    providers: &[(String, String)],
    facts: &mut Facts,
) {
    for (package, provider) in providers {
        if !read.active() {
            break;
        }
        let Some((path, source)) = source_for(read, roots, provider, Some(package)) else {
            continue;
        };
        index.dependency_files.insert(path.clone());
        facts.provider_files.insert(path.clone());
        facts.provider_sources.insert(path.clone(), source.clone());
        let full = format!("{}/{path}", read.root);
        crate::eloquent_members::cache_metadata(&mut index.metadata, &source, &full);
        for method in ["directive", "if"] {
            for open in static_calls(&source, method) {
                if !class_call(&source, open, "Illuminate\\Support\\Facades\\Blade") {
                    continue;
                }
                let args = php::arguments(&source, open);
                let Some((name, start)) = args.first().and_then(|r| literal(&source, r.clone()))
                else {
                    continue;
                };
                if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                    continue;
                }
                let width = name.encode_utf16().count();
                let names = if method == "if" {
                    vec![
                        name.into(),
                        format!("else{name}"),
                        format!("unless{name}"),
                        format!("end{name}"),
                    ]
                } else {
                    vec![name.into()]
                };
                for name in names {
                    index.directives.add(
                        &name,
                        Some(format!("declared Composer provider {package}")),
                        &full,
                        text::position(&source, start),
                        width,
                    );
                }
            }
        }
    }
}
/// Called after the main project inventories have been built. Package source
/// facts remain source-only and never discover or execute application PHP.
pub(crate) fn apply(project: &Project, index: &mut Index) {
    let Some(mut read) = Read::new(project) else {
        return;
    };
    let mut facts = Facts::default();
    let (roots, providers) = declared_roots(&mut read, index);
    facts.roots = roots.clone();
    vendor_directives(&mut read, index, &roots, &providers, &mut facts);
    application_facts(&mut read, index, &mut facts);
    dusk::apply(&mut read, index, &roots, &mut facts);
    twig_views(&mut read, index);
    native_elements(&mut read, index, &roots, &mut facts);
    native::apply(&mut read, index, &roots, &mut facts);
    volt_components(&mut read, index, &mut facts);
    livewire_views::apply(&mut read, index, &roots, &mut facts);
    if read.exhausted {
        facts
            .issues
            .push("Package source discovery reached its bounded descriptor/source budget".into());
        index.partial = true;
    }
    index.dependency_files.extend(read.cache.keys().cloned());
    index.metadata.issues.extend(facts.issues.iter().cloned());
    crate::metadata::cache_package_icons(project, &mut index.metadata, &facts.provider_sources);
    declared_relations_from(project, index, &facts.roots,&facts.provider_sources);
    facts.source_queries.extend(read.source_queries);
    index.source_queries.extend(facts.source_queries.clone());
    facts.source_evidence.extend(read.source_evidence);
    facts.source_evidence_incomplete|=read.source_evidence_incomplete;
    if facts.source_evidence.iter().any(|(path,value)|index.source_evidence.get(path).is_some_and(|old|old!=value)){facts.source_evidence_incomplete=true;}
    index.source_evidence.extend(facts.source_evidence.clone());
    index.source_evidence_incomplete|=facts.source_evidence_incomplete;
    index.packages = facts;
}

fn enabled(index: &Index, package: &str) -> bool {
    index.metadata.package_version(package).is_some()
}
fn app_sources(read: &mut Read<'_>, index: &mut Index) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for folder in ["app/Providers", "app/Features"] {
        index.dependency_roots.insert(folder.into());
        for path in read.find(&format!("{folder}/**/*.php")) {
            if let Some(source) = read.read(&path) {
                out.push((path, source));
            }
        }
    }
    out
}
fn entry(path: &str, source: &str, start: usize, name: &str, detail: &str) -> Entry {
    Entry {
        path: path.into(),
        at: text::position(source, start),
        width: name.encode_utf16().count() as u32,
        detail: Some(detail.into()),
    }
}
fn application_facts(read: &mut Read<'_>, index: &mut Index, facts: &mut Facts) {
    let pennant = enabled(index, "laravel/pennant");
    let folio = enabled(index, "laravel/folio");
    let mut mounts = BTreeSet::new();
    if pennant || folio {
        for (path, source) in app_sources(read, index) {
            let full = if path.starts_with('/') {
                path.clone()
            } else {
                format!("{}/{path}", read.root)
            };
            if pennant {
                for open in static_calls(&source, "define") {
                    if !class_call(&source, open, "Laravel\\Pennant\\Feature") {
                        continue;
                    }
                    if let Some((name, start)) = php::arguments(&source, open)
                        .first()
                        .and_then(|r| literal(&source, r.clone()))
                    {
                        facts.pennant.entry(name.into()).or_insert_with(|| {
                            entry(&full, &source, start, name, "Pennant feature")
                        });
                    }
                }
            }
            if folio {
                for open in static_calls(&source, "path") {
                    if !class_call(&source, open, "Laravel\\Folio\\Folio") {
                        continue;
                    }
                    let args = php::arguments(&source, open);
                    let Some(arg) = args.first() else {
                        continue;
                    };
                    let raw = source[arg.clone()].trim();
                    let Some(inner) = raw
                        .strip_prefix("resource_path(")
                        .and_then(|v| v.strip_suffix(')'))
                    else {
                        continue;
                    };
                    let base = arg.start + source[arg.clone()].find(inner).unwrap_or(0);
                    let Some((value, _)) = literal(&source, base..base + inner.len()) else {
                        continue;
                    };
                    if let Some(path) = normalize("resources", value) {
                        mounts.insert(path);
                    }
                }
            }
        }
    }
    for mount in mounts {
        index.dependency_roots.insert(mount.clone());
        for path in read.find(&format!("{mount}/**/*.blade.php")) {
            let Some(source) = read.read(&path) else {
                continue;
            };
            let Some(binding) = livewire::php_source(&source) else {
                continue;
            };
            let Some(code) = crate::package_facts::code(binding) else {
                continue;
            };
            for (at, _) in code.match_indices("name") {
                if at > 0 && php::is_identifier(code.as_bytes()[at - 1]) {
                    continue;
                }
                let rest = code[at + 4..].trim_start();
                if !rest.starts_with('(') {
                    continue;
                }
                let open = code.len() - rest.len();
                let Some(call) =
                    crate::eloquent_members::metadata_inference::call_identity(binding, open)
                else {
                    continue;
                };
                if call.function_fqn.as_deref() != Some("Laravel\\Folio\\name") {
                    continue;
                }
                if let Some((name, start)) = php::arguments(&source, open)
                    .first()
                    .and_then(|r| literal(&source, r.clone()))
                {
                    let full = if path.starts_with('/') {
                        path.clone()
                    } else {
                        format!("{}/{path}", read.root)
                    };
                    let e = entry(&full, &source, start, name, "Folio named page");
                    facts.folio.entry(name.into()).or_insert(e.clone());
                    index.routes.entry(name.into()).or_insert(e);
                }
            }
        }
    }
    if enabled(index, "laravel/envoy") {
        index.dependency_files.insert("Envoy.blade.php".into());
        if let Some(source) = read.read("Envoy.blade.php") {
            let full = format!("{}/Envoy.blade.php", read.root);
            for (name, start) in envoy_declarations(&source) {
                facts
                    .envoy
                    .entry(name.into())
                    .or_insert_with(|| entry(&full, &source, start, name, "Envoy task/story"));
            }
        }
    }
}
fn envoy_declarations(source: &str) -> Vec<(&str, usize)> {
    let visible = crate::refs::without_comments(source);
    let mut out = Vec::new();
    for directive in ["@task", "@story"] {
        for (at, _) in visible.match_indices(directive) {
            let line = visible[..at].rsplit('\n').next().unwrap_or("");
            if !line.trim().is_empty() {
                continue;
            }
            let rest = visible[at + directive.len()..].trim_start();
            if !rest.starts_with('(') {
                continue;
            }
            let open = visible.len() - rest.len();
            if let Some((name, start)) = php::arguments(source, open)
                .first()
                .and_then(|r| literal(source, r.clone()))
            {
                out.push((name, start));
            }
        }
    }
    out
}
fn php_site<'a>(
    index: &'a Index,
    doc: &Document,
    at: usize,
) -> Option<(std::ops::Range<usize>, &'a Names)> {
    if let Some(site) = dusk::site(index, doc, at) {
        return Some(site);
    }
    let (literal, site) = crate::refs::literal_site_at(&doc.language, &doc.text, at)?;
    if site.arg != 0 || site.depth != 0 {
        return None;
    }
    let open = doc.text[site.callee_start..].find('(')? + site.callee_start;
    let call = crate::eloquent_members::metadata_inference::call_identity(&doc.text, open)?;
    if enabled(index, "laravel/pennant")
        && call.class.as_deref() == Some("Laravel\\Pennant\\Feature")
        && [
            "active",
            "inactive",
            "value",
            "activate",
            "deactivate",
            "forget",
            "purge",
            "activeForEveryone",
            "inactiveForEveryone",
        ]
        .contains(&call.method.as_str())
    {
        return Some((literal.start..literal.end, &index.packages.pennant));
    }
    None
}
fn twig_site<'a>(
    index: &'a Index,
    doc: &Document,
    at: usize,
) -> Option<(std::ops::Range<usize>, &'a Names)> {
    if !doc.path.ends_with(".twig") || !enabled(index, "rcrowe/twigbridge") || at > doc.text.len() {
        return None;
    }
    let prefix = &doc.text[..at];
    if prefix
        .rfind("{#")
        .is_some_and(|open| prefix.rfind("#}").is_none_or(|close| close < open))
        || prefix.rfind("{% verbatim %}").is_some_and(|open| {
            prefix
                .rfind("{% endverbatim %}")
                .is_none_or(|close| close < open)
        })
    {
        return None;
    }
    let open = prefix
        .rfind("{{")
        .into_iter()
        .chain(prefix.rfind("{% "))
        .max()?;
    if prefix[open..].contains("}}") || prefix[open..].contains("%}") {
        return None;
    }
    let (quote_at, quote) = prefix
        .char_indices()
        .rev()
        .find(|(_, c)| matches!(c, '\'' | '"'))?;
    let context = prefix[..quote_at].trim_end();
    let names = if ["extends", "include", "embed", "import", "from"]
        .iter()
        .any(|tag| {
            context.ends_with(tag)
                && context
                    .rfind("{%")
                    .is_some_and(|o| !context[o..].contains("%}"))
        }) {
        &index.views
    } else if context.ends_with("route(") || context.ends_with("link_to_route(") {
        &index.routes
    } else if context.ends_with("trans(") || context.ends_with("trans_choice(") {
        &index.translations
    } else if context.ends_with("config_get(") || context.ends_with("config_has(") {
        &index.config
    } else {
        return None;
    };
    let end = doc.text[quote_at + 1..]
        .find(quote)
        .map(|n| quote_at + 1 + n)
        .unwrap_or(at);
    if at < quote_at + 1 || at > end || doc.text[quote_at + 1..at].contains(['\\', '\n', '\r']) {
        return None;
    }
    Some((quote_at + 1..end, names))
}
fn callback_site<'a>(
    index: &'a Index,
    doc: &Document,
    at: usize,
) -> Option<(std::ops::Range<usize>, &'a Names)> {
    if doc.text.len() > SOURCE_LIMIT {
        return None;
    }
    if let Some(site) = twig_site(index, doc, at) {
        return Some(site);
    }
    if doc.path.ends_with("/Envoy.blade.php") && enabled(index, "laravel/envoy") {
        let visible = crate::refs::without_comments(&doc.text);
        let before = &visible[..at];
        let line = before.rsplit('\n').next()?;
        let start = at - line.len() + line.len() - line.trim_start().len();
        let typed = &doc.text[start..at];
        if typed
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ':' | '.'))
            && before
                .match_indices("@story")
                .filter(|(s, _)| {
                    before[..*s]
                        .rsplit('\n')
                        .next()
                        .is_some_and(|line| line.trim().is_empty())
                })
                .last()
                .is_some_and(|(s, _)| !before[s..].contains("@endstory"))
        {
            return Some((start..at, &index.packages.envoy));
        }
    }
    php_site(index, doc, at)
}
fn current(read: &mut Read<'_>, name: &str, entry: &Entry) -> bool {
    if entry.path.is_empty() {
        return true;
    }
    if !owned(read.project, &entry.path) {
        return false;
    }
    if entry.width == 0 {
        return read.project.stat(&entry.path).is_ok();
    }
    let Some(source) = read.read(&entry.path) else {
        return false;
    };
    let Some(at) = text::offset(&source, entry.at) else {
        return false;
    };
    source[at..].starts_with(name) && entry.width == name.encode_utf16().count() as u32
}
pub(crate) fn completion(index: &Index, doc: &Document, at: Position) -> Vec<CompletionItem> {
    let values = livewire_values::completion(index, doc, at);
    if !values.is_empty() {
        return values;
    }
    let tests = livewire::test_completion(index, doc, at);
    if !tests.is_empty() {
        return tests;
    }
    let wire = livewire::completion(index, doc, at);
    if !wire.is_empty() {
        return wire;
    }
    let Some(offset) = text::offset(&doc.text, at) else {
        return vec![];
    };
    let Some((range, names)) = callback_site(index, doc, offset) else {
        return vec![];
    };
    let typed = &doc.text[range.start..offset];
    let Some(project) = crate::project_for(&doc.path) else {
        return vec![];
    };
    let Some(mut read) = Read::new(&project) else {
        return vec![];
    };
    names
        .iter()
        .filter(|(name, _)| name.starts_with(typed))
        .take(256)
        .filter_map(|(name, e)| {
            if !current(&mut read, name, e) {
                return None;
            }
            let mut item = CompletionItem::new(name, CompletionKind::Value);
            item.detail = presentation_detail(e.detail.clone());
            item.replace = Some(Range::new(
                text::position(&doc.text, range.start),
                text::position(&doc.text, range.end),
            ));
            Some(item)
        })
        .collect()
}
pub(crate) fn definition(index: &Index, doc: &Document, at: Position) -> Vec<Location> {
    let native_attribute = native::attribute_definition(index, doc, at);
    if !native_attribute.is_empty() {
        return native_attribute;
    }

    if doc.language == "blade" {
        let directive = || {
            let offset = text::offset(&doc.text, at)?;
            let (start, end) = crate::directives::name_at(&doc.text, offset)?;
            let visible = crate::refs::without_comments(&doc.text);
            if visible.get(start..end) != doc.text.get(start..end) {
                return None;
            }
            let name = &doc.text[start..end];
            let e = index.directives.get(name)?;
            let project = crate::project_for(&doc.path)?;
            let mut read = Read::new(&project)?;
            if e.width > 0 {
                let source = read.read(&e.path)?;
                let begin = text::offset(&source, e.at)?;
                let finish =
                    text::offset(&source, Position::new(e.at.line, e.at.character + e.width))?;
                let actual = &source[begin..finish];
                if actual != name
                    && !(["end", "else", "unless"]
                        .iter()
                        .any(|p| name.strip_prefix(p) == Some(actual)))
                {
                    return None;
                }
            }
            crate::scoped_location(&project, e)
        };
        if let Some(location) = directive() {
            return vec![location];
        }
    }

    let values = livewire_values::definition(index, doc, at);
    if !values.is_empty() {
        return values;
    }
    let tests = livewire::test_definition(index, doc, at);
    if !tests.is_empty() {
        return tests;
    }
    let run = || {
        let offset = text::offset(&doc.text, at)?;
        let (range, names) = callback_site(index, doc, offset)?;
        let name = &doc.text[range];
        let e = names.get(name)?;
        let project = crate::project_for(&doc.path)?;
        let mut read = Read::new(&project)?;
        if !current(&mut read, name, e) {
            return None;
        }
        crate::scoped_location(&project, e)
    };
    run().into_iter().collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, providers::Providers};
    fn fixture(host: &MockHost) {
        host.add_file("artisan", "<?php");
        host.add_file("composer.json",r#"{"require":{"laravel/framework":"^13","laravel/pennant":"^1","laravel/folio":"^1","laravel/envoy":"^2","rcrowe/twigbridge":"^0.14"},"autoload":{"psr-4":{"App\\":"app/"}}}"#);
        host.add_file("composer.lock",r#"{"packages":[{"name":"laravel/pennant","version":"v1.27.0"},{"name":"laravel/folio","version":"v1.3.0"},{"name":"laravel/envoy","version":"v2.13.0"},{"name":"rcrowe/twigbridge","version":"v0.14.8"}]}"#);
        host.add_file("vendor/composer/installed.json",r#"{"packages":[{"name":"acme/ui","install-path":"../acme/ui","autoload":{"psr-4":{"Acme\\UI\\":"src/"}},"extra":{"laravel":{"providers":["Acme\\UI\\Provider"]}}}]}"#);
        host.add_file("vendor/acme/ui/src/Provider.php","<?php namespace Acme\\UI; use Illuminate\\Support\\Facades\\Blade; class Provider { function boot(){ Blade::directive('markdown_2', fn($e)=>$e); Blade::if('subscribed', fn()=>true); } }");
        host.add_file("app/Providers/AppServiceProvider.php","<?php namespace App\\Providers; use Laravel\\Pennant\\Feature; use Laravel\\Folio\\Folio; class AppServiceProvider { function boot(){ Feature::define('purchase-button', fn()=>true); Folio::path(resource_path('views/pages')); } }");
        host.add_file(
            "resources/views/pages/users.blade.php",
            "<?php use function Laravel\\Folio\\name; name('users.index'); ?>Hello",
        );
        host.add_file("resources/views/welcome.twig", "hello");
        host.add_file(
            "Envoy.blade.php",
            "@task('deploy')\necho deploying\n@endtask\n@story('release')\ndeploy\n@endstory",
        );
    }
    #[test]
    fn declared_vendor_and_application_sources_reach_real_providers() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        fixture(&host);
        let project = host.invocation().project().unwrap();
        let index = Index::build(&project);
        assert!(index.directives.contains_key("markdown_2"));
        assert!(index.directives.contains_key("endsubscribed"));
        assert!(index.packages.pennant.contains_key("purchase-button"));
        assert!(index.routes.contains_key("users.index"));
        assert!(index.packages.envoy.contains_key("deploy"));
        let mut main = crate::Main::default();
        main.indexes.insert("/project".into(), index);
        for (path, language, source, label) in [
            (
                "app/test.php",
                "php",
                "<?php use Laravel\\Pennant\\Feature; Feature::active('pur');",
                "purchase-button",
            ),
            (
                "resources/views/a.twig",
                "twig",
                "{% extends 'wel' %}",
                "welcome",
            ),
            (
                "Envoy.blade.php",
                "blade",
                "@story('release')\nde",
                "deploy",
            ),
        ] {
            let doc = Document {
                path: format!("/project/{path}"),
                language: language.into(),
                text: source.into(),
                version: 1,
            };
            let at = source
                .find("pur")
                .map(|p| p + 3)
                .or_else(|| source.find("wel").map(|p| p + 3))
                .unwrap_or(source.len());
            assert!(
                main.completion(&doc, text::position(source, at), None)
                    .iter()
                    .any(|i| i.label == label),
                "{source}"
            );
        }
    }
    #[test]
    fn imports_comments_and_foreign_provider_roots_do_not_create_facts() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        fixture(&host);
        host.add_file("app/Providers/AppServiceProvider.php","<?php use App\\Feature; Feature::define('fake',fn()=>true); // \\Laravel\\Pennant\\Feature::define('comment',fn()=>true)");
        host.add_project("/project/vendor/acme");
        let index = Index::build(&host.invocation().project().unwrap());
        assert!(!index.directives.contains_key("markdown_2"));
        assert!(index.packages.pennant.is_empty());
        assert!(!index.routes.contains_key("users.index"));
    }
}

fn twig_views(read: &mut Read<'_>, index: &mut Index) {
    if !enabled(index, "rcrowe/twigbridge") {
        return;
    }
    let mut roots = index.metadata.view_roots.clone();
    roots.push((None, "resources/views".into()));
    for (namespace, root) in roots {
        if !clean(&root) {
            continue;
        }
        index.dependency_roots.insert(root.clone());
        for path in read.find(&format!("{root}/**/*.twig")) {
            let relative = path
                .strip_prefix(&format!("{}/", read.root))
                .unwrap_or(&path);
            let Some(local) = relative
                .strip_prefix(&format!("{root}/"))
                .and_then(|s| s.strip_suffix(".twig"))
            else {
                continue;
            };
            if read.read(&path).is_none() {
                continue;
            }
            let name = local.replace('/', ".");
            let name = namespace
                .as_ref()
                .map_or(name.clone(), |ns| format!("{ns}::{name}"));
            let full = if path.starts_with('/') {
                path.clone()
            } else {
                format!("{}/{path}", read.root)
            };
            index.views.entry(name).or_insert(Entry {
                path: full,
                at: Position::new(0, 0),
                width: 0,
                detail: Some("TwigBridge view".into()),
            });
        }
    }
}
fn native_elements(
    read: &mut Read<'_>,
    index: &mut Index,
    roots: &[ComposerRoot],
    facts: &mut Facts,
) {
    if index.metadata.package_major("nativephp/mobile") != Some(4) {
        return;
    }
    // Only manifests explicitly declared by Composer-installed NativePHP plugins
    // are read; a similarly named JSON file elsewhere is never a declaration.
    let installed = format!("{}/vendor/composer/installed.json", read.root);
    if !owned(read.project, &installed) {
        return;
    }
    let Some(source) = read.capture("vendor/composer/installed.json",2*1024*1024) else {
        return;
    };
    let Ok(value) = crate::strict_json::parse(&source, 2 * 1024 * 1024) else {
        return;
    };
    let Some(packages) = value
        .get("packages")
        .and_then(Value::as_array)
        .or_else(|| value.as_array())
    else {
        return;
    };
    for package in packages.iter().take(512) {
        let Some(name) = package.get("name").and_then(Value::as_str) else {
            continue;
        };
        if !index.metadata.package_enabled(name, true) {
            continue;
        }
        let Some(manifest) = package
            .pointer("/extra/nativephp/manifest")
            .and_then(Value::as_str)
        else {
            continue;
        };
        let Some(base) = package
            .get("install-path")
            .and_then(Value::as_str)
            .and_then(|p| normalize("vendor/composer", p))
        else {
            continue;
        };
        let Some(path) = normalize(&base, manifest) else {
            continue;
        };
        if !path.starts_with(&format!("{base}/")) {
            continue;
        }
        index.dependency_files.insert(path.clone());
        let Some(source) = read.read(&path) else {
            continue;
        };
        let Ok(manifest) = crate::strict_json::parse(&source, SOURCE_LIMIT) else {
            continue;
        };
        let Some(components) = manifest.get("components").and_then(Value::as_array) else {
            continue;
        };
        for component in components.iter().take(128) {
            let (Some(kind), Some(class)) = (
                component.get("type").and_then(Value::as_str),
                component.get("blade").and_then(Value::as_str),
            ) else {
                continue;
            };
            if !kind
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            {
                continue;
            }
            let (resolved, element_only) = if let Some(found) = source_for(read, roots, class, None)
            {
                (Some(found), false)
            } else {
                (
                    component
                        .get("element")
                        .and_then(Value::as_str)
                        .and_then(|element| source_for(read, roots, element, None)),
                    true,
                )
            };
            let Some((path, source)) = resolved else {
                facts.native_partial = true;
                continue;
            };
            index.dependency_files.insert(path.clone());
            let tag = format!("native:{}", kind.replace('_', "-"));
            let full = format!("{}/{path}", read.root);
            if element_only {
                facts
                    .native_element_props
                    .insert(tag.clone(), native::element_props(&source, &full));
            }
            facts.native.entry(tag).or_insert(Entry {
                path: full,
                at: Position::new(0, 0),
                width: 0,
                detail: Some(format!(
                    "NativePHP {name} element {}",
                    crate::eloquent_members::declaration(&source).unwrap_or_else(|| class.into())
                )),
            });
            crate::eloquent_members::cache_metadata(
                &mut index.metadata,
                &source,
                &format!("{}/{path}", read.root),
            );
        }
    }
}
fn volt_components(read: &mut Read<'_>, index: &mut Index, facts: &mut Facts) {
    if !enabled(index, "livewire/volt") {
        return;
    }
    let mut mounts = BTreeSet::new();
    for (_, source) in app_sources(read, index) {
        for open in static_calls(&source, "mount") {
            if !class_call(&source, open, "Livewire\\Volt\\Volt") {
                continue;
            }
            for arg in php::arguments(&source, open) {
                let ranges = if let Some(array) = php::array_open(&source, arg.clone()) {
                    php::items(&source, array)
                        .into_iter()
                        .filter(|item| item.key.is_none())
                        .map(|item| item.value)
                        .collect()
                } else {
                    vec![arg]
                };
                for range in ranges {
                    let raw = source[range.clone()].trim();
                    let Some(inner) = raw
                        .strip_prefix("resource_path(")
                        .and_then(|s| s.strip_suffix(')'))
                    else {
                        continue;
                    };
                    let at = range.start + source[range.clone()].find(inner).unwrap_or(0);
                    if let Some((value, _)) = literal(&source, at..at + inner.len()) {
                        if let Some(path) = normalize("resources", value) {
                            mounts.insert(path);
                        }
                    }
                }
            }
        }
    }
    for mount in mounts {
        index.dependency_roots.insert(mount.clone());
        for path in read.find(&format!("{mount}/**/*.blade.php")) {
            let relative = path
                .strip_prefix(&format!("{}/", read.root))
                .unwrap_or(&path);
            let Some(local) = relative
                .strip_prefix(&format!("{mount}/"))
                .and_then(|s| s.strip_suffix(".blade.php"))
            else {
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
            let (props, methods) = livewire::volt(&source, &full);
            let name = local.replace('/', ".");
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
                detail: Some("Volt component".into()),
            };
            facts.volt.insert(name.clone(), entry.clone());
            facts.volt_props.insert(name.clone(), props);
            facts.volt_methods.insert(name.clone(), methods);
            facts
                .volt_computed
                .insert(name.clone(), livewire::computed(&source, &full));
            index.livewire.entry(name).or_insert(entry);
        }
    }
}
#[cfg(test)]
mod installed_tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, providers::Providers};
    /// Run with OPENIDEA_LARAVEL_PACKAGE_FIXTURE pointing at a disposable Composer
    /// install. This copies descriptor-declared sources into MockHost; package PHP
    /// and Composer lifecycle scripts are never executed by the test.
    #[test]
    #[ignore = "requires disposable installed Composer package fixture"]
    fn installed_composer_package_differential() {
        let folder = std::env::var("OPENIDEA_LARAVEL_PACKAGE_FIXTURE").expect("fixture path");
        let base = std::path::Path::new(&folder);
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        for name in [
            "composer.json",
            "composer.lock",
            "vendor/composer/installed.json",
        ] {
            host.add_file(name, &std::fs::read_to_string(base.join(name)).unwrap());
        }
        host.add_file("artisan", "<?php");
        let installed: Value = serde_json::from_str(
            &std::fs::read_to_string(base.join("vendor/composer/installed.json")).unwrap(),
        )
        .unwrap();
        let mut roots = Vec::new();
        let packages = installed["packages"].as_array().unwrap();
        for p in packages {
            let name = p["name"].as_str().unwrap();
            let Some(dir) = p["install-path"]
                .as_str()
                .and_then(|p| normalize("vendor/composer", p))
            else {
                continue;
            };
            add_roots(&mut roots, name, &dir, p);
        }
        let load_class = |full: &str| {
            for root in &roots {
                if let Some(local) = full.strip_prefix(&root.namespace) {
                    let path = format!("{}/{}.php", root.directory, local.replace('\\', "/"));
                    if let Ok(source) = std::fs::read_to_string(base.join(&path)) {
                        host.add_file(&path, &source);
                    }
                }
            }
        };
        for p in packages {
            for provider in strings(
                p.pointer("/extra/laravel/providers")
                    .unwrap_or(&Value::Null),
            ) {
                load_class(provider);
            }
            if let Some(manifest) = p
                .pointer("/extra/nativephp/manifest")
                .and_then(Value::as_str)
            {
                let dir =
                    normalize("vendor/composer", p["install-path"].as_str().unwrap()).unwrap();
                let path = normalize(&dir, manifest).unwrap();
                let source = std::fs::read_to_string(base.join(&path)).unwrap();
                host.add_file(&path, &source);
                let value: Value = serde_json::from_str(&source).unwrap();
                for c in value["components"].as_array().unwrap() {
                    if let Some(class) = c["blade"].as_str() {
                        load_class(class);
                    }
                    if let Some(element) = c["element"].as_str() {
                        load_class(element);
                    }
                }
            }
        }
        load_class("Native\\Mobile\\Edge\\ComponentRegistry");
        host.add_file("app/Providers/NativeFixtureProvider.php","<?php namespace App\\Providers; use Native\\Mobile\\Edge\\ComponentRegistry; class NativeFixtureProvider {function boot(){ComponentRegistry::components(['user-card'=>\\App\\NativeComponents\\UserCard::class,'button'=>\\App\\NativeComponents\\UserCard::class]);ComponentRegistry::register('dynamic-card',$class);}}");
        host.add_file("app/NativeComponents/UserCard.php","<?php namespace App\\NativeComponents; class UserCard extends \\Native\\Mobile\\Edge\\NativeComponent { public function __construct(public string $userName) {} }");
        host.add_file("app/NativeComponents/AutoCard.php","<?php namespace App\\NativeComponents; class AutoCard extends \\Native\\Mobile\\Edge\\NativeComponent {}");
        host.add_file("app/Providers/PackageFixtureProvider.php","<?php namespace App\\Providers; use Laravel\\Pennant\\Feature; use Laravel\\Folio\\Folio; use Livewire\\Volt\\Volt; class PackageFixtureProvider {function boot(){Feature::define('fixture-feature',fn()=>true);Folio::path(resource_path('views/pages'));Volt::mount([resource_path('views/livewire')]);}}");
        host.add_file(
            "resources/views/pages/users.blade.php",
            "<?php use function Laravel\\Folio\\name; name('fixture.users'); ?>users",
        );
        host.add_file("resources/views/livewire/counter.blade.php","<?php use function Livewire\\Volt\\state; state(['count'=>0]); $increment=fn()=>1; ?><button wire:click=\"increment\">counter</button>");
        let project = host.invocation().project().unwrap();
        let index = Index::build(&project);
        assert!(index.packages.pennant.contains_key("fixture-feature"));
        assert!(index.routes.contains_key("fixture.users"));
        assert_eq!(
            index
                .packages
                .volt_props
                .get("counter")
                .map(|v| v.as_slice()),
            Some(["count".to_string()].as_slice()),
            "partial={} issues={:?} livewire={:?}",
            index.partial,
            index.packages.issues,
            index.livewire
        );
        assert!(index.packages.volt_methods["counter"].contains_key("increment"));
        assert!(index.directives.contains_key("nativeError"));
        assert!(index.packages.native.contains_key("native:button"));
        assert!(index.packages.native.contains_key("native:text"));
        assert!(index.packages.native_element_props["native:text"].contains_key("font-size"));
        assert!(index.packages.native.contains_key("native:user-card"));
        assert!(index.packages.native.contains_key("native:auto-card"));
        assert!(index.packages.native_partial);
        assert!(!index.packages.native.contains_key("native:dynamic-card"));
        assert!(
            index.packages.native["native:button"]
                .path
                .contains("vendor/")
        );
        let mut main = crate::Main::default();
        main.indexes.insert("/project".into(), index);
        let source = "<native:but";
        let doc = Document {
            path: "/project/resources/views/page.blade.php".into(),
            language: "blade".into(),
            text: source.into(),
            version: 1,
        };
        let found = main.completion(&doc, text::position(source, source.len()), None);
        assert!(found.iter().any(|i| i.label == "native:button"));
        let source = "<native:user-card us";
        let custom = Document {
            text: source.into(),
            ..doc.clone()
        };
        assert!(
            main.completion(&custom, text::position(source, source.len()), None)
                .iter()
                .any(|i| i.label == "user-name")
        );
        let source = "<native:text font-si";
        let element = Document {
            text: source.into(),
            ..doc.clone()
        };
        assert!(
            main.completion(&element, text::position(source, source.len()), None)
                .iter()
                .any(|i| i.label == "font-size")
        );
        let source = "<native:text font-size=\"12\"/>";
        let element = Document {
            text: source.into(),
            ..element
        };
        assert!(
            !main
                .definition(
                    &element,
                    text::position(source, source.find("font-size").unwrap() + 3)
                )
                .is_empty()
        );
        let source = "<native:button/>";
        let doc = Document {
            text: source.into(),
            ..doc
        };
        assert!(
            main.definition(
                &doc,
                text::position(source, source.find("button").unwrap() + 2)
            )
            .iter()
            .any(|d| d.path.ends_with("/Button.php"))
        );
    }
}
/// UI-only localization boundary. Cached source facts and parsers remain pure.
pub(crate) fn presentation_detail(detail: Option<String>) -> Option<String> {
    let detail = detail?;
    let exact = match detail.as_str() {
        "Pennant feature" => Some(("pennant", "Pennant feature")),
        "Folio named page" => Some(("folio", "Folio named page")),
        "Envoy task/story" => Some(("envoy", "Envoy task or story")),
        "Dusk scoped selector" => Some(("dusk_selector", "Dusk scoped selector")),
        "Dusk page/component" => Some(("dusk_component", "Dusk page or component")),
        "Livewire action" => Some(("livewire_action", "Livewire action")),
        "Livewire property" => Some(("livewire_property", "Livewire property")),
        "Volt action" => Some(("volt_action", "Volt action")),
        "Volt computed property" => Some(("volt_computed", "Volt computed property")),
        "Volt component" => Some(("volt_component", "Volt component")),
        "Livewire view component action" => {
            Some(("livewire_view_action", "Livewire view component action"))
        }
        "Livewire configured class namespace" => Some((
            "livewire_class_namespace",
            "Livewire configured class namespace",
        )),
        "Livewire 4 view component" => Some(("livewire_view", "Livewire 4 view component")),
        "TwigBridge view" => Some(("twig_view", "TwigBridge view")),
        "ide.json component class" => {
            Some(("metadata_class", "Component class declared by ide.json"))
        }
        "ide.json anonymous component" => Some(("metadata_anonymous", "Anonymous component declared by ide.json")),
        "ide.json declared component" => {
            Some(("metadata_component", "Component declared by ide.json"))
        }
        _ => None,
    };
    if let Some((key, english)) = exact {
        return Some(crate::ui_text::t(&format!("package.detail.{key}"), english));
    }
    if let Some(name) = detail.strip_prefix("NativePHP child component ") {
        return Some(crate::ui_text::format(
            "package.detail.native_child",
            "NativePHP child component {class}",
            &[("class", name)],
        ));
    }
    if let Some((package, class)) = detail
        .strip_prefix("NativePHP ")
        .and_then(|s| s.split_once(" element "))
    {
        return Some(crate::ui_text::format(
            "package.detail.native_element",
            "NativePHP {package} element {class}",
            &[("package", package), ("class", class)],
        ));
    }
    if let Some(package) = detail.strip_prefix("declared Composer provider ") {
        return Some(crate::ui_text::format(
            "package.detail.composer_provider",
            "Composer provider: {package}",
            &[("package", package)],
        ));
    }
    if let Some(name) = detail
        .strip_prefix("Volt ")
        .and_then(|s| s.strip_suffix(" state"))
    {
        return Some(crate::ui_text::format(
            "package.detail.volt_state",
            "Volt {component} state",
            &[("component", name)],
        ));
    }
    Some(detail)
}

/// Load only package classes named by an installed descriptor and cached project
/// model sources. The separate budget prevents framework/UI discovery starving
/// relation proofs in large package inventories.
pub(crate) fn declared_relations(project:&Project,index:&mut Index,roots:&[ComposerRoot]) {
    let providers=index.packages.provider_sources.clone();declared_relations_from(project,index,roots,&providers);
}
fn declared_relations_from(project:&Project,index:&mut Index,roots:&[ComposerRoot],providers:&BTreeMap<String,String>){
    if !roots.iter().any(|root|matches!(root.package.as_str(),"wendelladriel/laravel-lift"|"october/rain"|"staudenmeir/laravel-adjacency-list")){return;}
    use crate::eloquent_members::package_relations::{Capabilities,collect,methods};
    let Some(mut read)=Read::new(project)else{return};
    let mut caps=Capabilities::default();
    let mut proof=|package:&str,full:&str| -> Option<(String,String)> {
        source_for(&mut read,roots,full,Some(package)).map(|(path,source)|(format!("{}/{}",read.root,path),source))
    };
    if let (Some((lift,lift_source)),Some((manage,manage_source)))=(proof("wendelladriel/laravel-lift","WendellAdriel\\Lift\\Lift"),proof("wendelladriel/laravel-lift","WendellAdriel\\Lift\\Concerns\\ManageRelations")) {
        if lift_source.contains("ManageRelations")&&manage_source.contains("resolveRelationUsing") {
            for kind in ["HasOne","HasMany","HasOneThrough","HasManyThrough","BelongsTo","BelongsToMany","MorphOne","MorphMany","MorphToMany","MorphedByMany"] {
                let full=format!("WendellAdriel\\Lift\\Attributes\\Relations\\{kind}");
                if let Some((path,source))=proof("wendelladriel/laravel-lift",&full) {
                    let name_position=if matches!(kind,"HasOneThrough"|"HasManyThrough"|"MorphOne"|"MorphMany"|"MorphToMany"|"MorphedByMany"){2}else{1};
                    let many=matches!(kind,"HasMany"|"HasManyThrough"|"BelongsToMany"|"MorphMany"|"MorphToMany"|"MorphedByMany");
                    if source.contains("implements RelationAttribute")&&crate::eloquent_members::package_relations::literal_lift_naming(&source,many)&&crate::eloquent_members::package_relations::constructor_parameters(&source).is_some_and(|p|p.first().map(String::as_str)==Some("related")&&p.get(name_position).map(String::as_str)==Some("name")) {caps.lift.insert(full,vec![lift.clone(),manage.clone(),path]);}
                }
            }
        }
    }
    if let(Some((model,model_source)),Some((relations,relations_source)))=(proof("october/rain","October\\Rain\\Database\\Model"),proof("october/rain","October\\Rain\\Database\\Concerns\\HasRelationships")) {
        if crate::eloquent_members::metadata_inference::class_fact(&model_source,&model).is_some_and(|fact|fact.parents.iter().any(|parent|parent=="Illuminate\\Database\\Eloquent\\Model"))&&relations_source.contains("function hasRelation(") {
            crate::eloquent_members::cache(&mut index.eloquent_models,&model_source,&model);
            caps.october=vec![model,relations];
        }
    }
    for (outer,inner,graph) in [("HasRecursiveRelationships","HasAdjacencyList",false),("HasGraphRelationships","HasGraphAdjacencyList",true)] {
        let full=format!("Staudenmeir\\LaravelAdjacencyList\\Eloquent\\{outer}");
        if let(Some((outer_path,outer_source)),Some((inner_path,inner_source)))=(proof("staudenmeir/laravel-adjacency-list",&full),proof("staudenmeir/laravel-adjacency-list",&format!("Staudenmeir\\LaravelAdjacencyList\\Eloquent\\Traits\\{inner}"))) {
            if !outer_source.contains(&format!("use {inner};")){continue;}
            let methods=methods(&inner_source,&inner_path);let mut declared=Vec::new();
            for name in ["ancestors","ancestorsAndSelf","bloodline","children","childrenAndSelf","descendants","descendantsAndSelf","parent","parentAndSelf","parents","parentsAndSelf","rootAncestor","rootAncestorOrSelf","siblings","siblingsAndSelf"] {
                let Some(entry)=methods.get(name)else{continue};
                let (kind,many)=match name {"parent"=>("belongsTo".to_owned(),false),"children"=>(if graph{"belongsToMany"}else{"hasMany"}.to_owned(),true),"parents"=>("belongsToMany".to_owned(),true),_=>{
                    let class=match name{"ancestors"|"ancestorsAndSelf"|"parentsAndSelf"|"parentAndSelf"=>"Ancestors","rootAncestor"=>"RootAncestor","rootAncestorOrSelf"=>"RootAncestorOrSelf","bloodline"=>"Bloodline","siblings"|"siblingsAndSelf"=>"Siblings",_=>"Descendants"};
                    (format!("Staudenmeir\\LaravelAdjacencyList\\Eloquent\\Relations\\{}{class}",if graph{"Graph\\"}else{""}),!matches!(name,"rootAncestor"|"rootAncestorOrSelf"))
                }};
                declared.push((name.into(),kind,many,entry.clone(),vec![outer_path.clone(),inner_path.clone()]));
            }
            caps.recursive.insert(full,declared);
        }
    }
    // Default names depend on installed English rules, not a suffix heuristic.
    if !caps.lift.is_empty(){
    let inflectible=proof("doctrine/inflector","Doctrine\\Inflector\\Rules\\English\\Inflectible");
    let uninflected=proof("doctrine/inflector","Doctrine\\Inflector\\Rules\\English\\Uninflected");
    let pluralizer=proof("illuminate/support","Illuminate\\Support\\Pluralizer").or_else(||proof("laravel/framework","Illuminate\\Support\\Pluralizer"));
    let str_source=proof("illuminate/support","Illuminate\\Support\\Str").or_else(||proof("laravel/framework","Illuminate\\Support\\Str"));
    if let(Some((rules_path,rules)),Some((uninflected_path,uninflected)),Some((pluralizer_path,pluralizer)),Some((str_path,str_source)))=(inflectible,uninflected,pluralizer,str_source){
        if pluralizer.contains("$language = 'english'")&&pluralizer.contains("'recommended'")&&pluralizer.contains("'related'")&&str_source.contains("lcfirst(static::studly($value))"){
            caps.inflector=crate::eloquent_members::package_relations::inflector::Inflector::parse(&rules,&uninflected,vec![rules_path,uninflected_path,pluralizer_path,str_path]);
        }
    }
    }
    drop(proof);
    if caps.inflector.is_some()&&providers.values().any(|source|pluralizer_override(source)){caps.inflector=None;}
    if caps.inflector.is_some(){for (_,source) in app_sources(&mut read,index){
        if pluralizer_override(&source) {caps.inflector=None;}
    }
    }
    let classes=crate::eloquent_members::class_inventory(&index.eloquent_models);
    let parents:BTreeMap<_,_>=classes.iter().filter(|c|!c.ambiguous).map(|c|(c.full.to_ascii_lowercase(),c.parent.to_ascii_lowercase())).collect();
    let mut facts=Vec::new();let mut sources=Vec::new();
    for class in classes.iter().filter(|class|!class.ambiguous&&!class.is_trait) {
        if !roots.iter().filter(|r|r.package=="project").any(|r|class.full.starts_with(&r.namespace)){continue;}
        let mut ancestor=class.full.to_ascii_lowercase();let mut october=false;
        for _ in 0..8 {if ancestor=="october\\rain\\database\\model"{october=true;break;}let Some(parent)=parents.get(&ancestor)else{break};ancestor=parent.clone();}
        let package_trait=class.traits.iter().any(|name|name=="WendellAdriel\\Lift\\Lift"||caps.recursive.contains_key(name));
        if !october&&!package_trait{continue;}
        let Some(source)=read.read(&class.path)else{continue};
        if pluralizer_override(&source){caps.inflector=None;}
        sources.push((class.path.clone(),source,october));
    }
    for(path,source,october)in sources{facts.extend(collect(&source,&path,&caps,october));}
    if !facts.is_empty(){
        let mut guard_read=Read::new(project).unwrap();
        let native=declared_method_graph(&mut guard_read,roots,"Illuminate\\Database\\Eloquent\\Model");
        let lift=if facts.iter().any(|fact|caps.lift.values().any(|paths|paths.iter().any(|path|fact.evidence_paths.contains(path)))){declared_method_graph(&mut guard_read,roots,"WendellAdriel\\Lift\\Lift")}else{None};
        let october=if facts.iter().any(|fact|caps.october.iter().any(|path|fact.evidence_paths.contains(path))){declared_method_graph(&mut guard_read,roots,"October\\Rain\\Database\\Model")}else{None};
        facts.retain_mut(|fact|{
            let Some((native_methods,native_paths))=native.as_ref()else{return false};
            let mut evidence=native_paths.clone();let name=fact.name.to_ascii_lowercase();if native_methods.contains(&name){return false;}
            if caps.lift.values().any(|paths|paths.iter().any(|path|fact.evidence_paths.contains(path))){let Some((methods,paths))=lift.as_ref()else{return false};if methods.contains(&name){return false;}evidence.extend(paths.iter().cloned());}
            if caps.october.iter().any(|path|fact.evidence_paths.contains(path)){let Some((methods,paths))=october.as_ref()else{return false};if methods.contains(&name){return false;}evidence.extend(paths.iter().cloned());}
            fact.evidence_paths.extend(evidence);true
        });
        read.source_evidence_incomplete|=guard_read.source_evidence_incomplete;
        for(path,value)in guard_read.source_evidence{read.evidence(&path,value);}
        index.dependency_files.extend(guard_read.cache.keys().cloned());
    }
    crate::eloquent_members::apply_declared_relations(&mut index.eloquent_models,&facts);
    for path in read.cache.keys(){index.dependency_files.insert(path.clone());}
    index.source_queries.extend(read.source_queries);
    if read.source_evidence.iter().any(|(path,value)|index.source_evidence.get(path).is_some_and(|old|old!=value)){read.source_evidence_incomplete=true;}
    index.source_evidence.extend(read.source_evidence);
    index.source_evidence_incomplete|=read.source_evidence_incomplete;
    if read.exhausted{index.partial=true;}
}
/// Read one exact installed class through the descriptor-proven source roots.
/// Adapters retain evidence even for unsuccessful candidate paths.
pub(crate) fn snapshot_class(project:&Project,index:&mut Index,package:&str,full:&str)->Option<(String,String)>{
    let mut read=Read::new(project)?;
    let result=source_for(&mut read,&index.packages.roots,full,Some(package));
    index.dependency_files.extend(read.cache.keys().cloned());
    if read.source_evidence.iter().any(|(path,value)|index.source_evidence.get(path).is_some_and(|old|old!=value)){read.source_evidence_incomplete=true;}
    index.source_evidence.extend(read.source_evidence);
    index.source_evidence_incomplete|=read.source_evidence_incomplete;
    if read.exhausted{index.partial=true;}
    result
}
#[cfg(test)]mod evidence_tests{
 use super::*;use openidea_plugin::mock::MockHost;
 #[test]fn pluralizer_override_requires_owned_binding_and_retains_explicit_english(){
  assert!(pluralizer_override("<?php use Illuminate\\Support\\Pluralizer;Pluralizer::useLanguage('french');"));
  assert!(!pluralizer_override("<?php use Illuminate\\Support\\Pluralizer;Pluralizer::useLanguage('english');"));
  assert!(!pluralizer_override("<?php use Vendor\\Pluralizer;Pluralizer::useLanguage('french');"));
  assert!(!pluralizer_override("<?php // Pluralizer::useLanguage('french');"));
 }
 #[test]fn uninstalled_relation_packages_do_not_scan_cached_application_classes(){
  let host=MockHost::install().with_capabilities(&[]);let project=host.invocation().project().unwrap();let mut index=Index::default();
  for number in 0..130{crate::eloquent_members::cache(&mut index.eloquent_models,&format!("<?php namespace App;class Model{number} extends \\Illuminate\\Database\\Eloquent\\Model{{}}"),&format!("/project/app/Model{number}.php"));}
  let roots=vec![ComposerRoot{namespace:"App\\".into(),directory:"app".into(),package:"project".into()}];declared_relations(&project,&mut index,&roots);
  assert!(!index.partial);assert!(!index.source_evidence_incomplete);assert!(index.source_evidence.is_empty());assert!(index.source_queries.is_empty());
 }
 #[test]fn direct_package_reads_capture_present_missing_queries_and_opaque_separately(){
  let host=MockHost::install().with_capabilities(&["project.read"]);host.add_file("app/Provider.php","<?php class Provider {}");
  let project=host.invocation().project().unwrap();let mut read=Read::new(&project).unwrap();
  assert!(read.read("app/Provider.php").is_some());assert!(read.read("missing.php").is_none());
  assert_eq!(read.source_evidence["app/Provider.php"],Some(("<?php class Provider {}".len(),crate::artisan_metadata::hash("<?php class Provider {}"))));
  assert_eq!(read.source_evidence["missing.php"],None);assert!(!read.source_evidence_incomplete);
  read.find("app/**/*.php");assert_eq!(read.source_queries[&("app/**/*.php".into(),513)].as_ref().unwrap().len(),1);
  host.add_file("large.php",&"x".repeat(SOURCE_LIMIT+1));assert!(read.read("large.php").is_none());assert!(read.source_evidence_incomplete);assert!(!read.source_evidence.contains_key("large.php"));
 }
}

fn declared_method_graph(read:&mut Read<'_>,roots:&[ComposerRoot],seed:&str)->Option<(BTreeSet<String>,Vec<String>)>{
 let mut pending=vec![seed.to_owned()];let mut seen=BTreeSet::new();let mut methods=BTreeSet::new();let mut evidence=Vec::new();
 while let Some(full)=pending.pop(){
  if !seen.insert(full.to_ascii_lowercase()){continue;}if seen.len()>96{return None;}
  let(path,source)=source_for(read,roots,&full,None)?;let path=format!("{}/{path}",read.root);
  let(identity,parent,traits,declared)=crate::eloquent_members::declared_method_inventory(&source,&path)?;if identity!=full{return None;}
  evidence.push(path);methods.extend(declared.into_iter().map(|(name,_)|name.to_ascii_lowercase()));
  pending.extend(traits);if !parent.is_empty(){pending.push(parent);}
 }
 Some((methods,evidence))
}

fn pluralizer_override(source:&str)->bool{
 static_calls(source,"useLanguage").into_iter().any(|open|class_call(source,open,"Illuminate\\Support\\Pluralizer")&&php::arguments(source,open).first().and_then(|range|literal(source,range.clone())).map(|(value,_)|value)!=Some("english"))||crate::package_facts::code(source).is_some_and(|code|code.contains("::$uncountable")||code.contains("::uncountable"))
}
