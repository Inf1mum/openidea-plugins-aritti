//! Bounded, declarative project metadata. No PHP or metadata expressions execute.
use crate::index::{AddName, Index, Names};
use openidea_plugin::{
    Location, Position, Range as TextRange,
    host::{self, Project},
    providers::{CompletionItem, CompletionKind},
    text,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
#[path="metadata_ui.rs"]
mod ui;
pub(crate) fn presentation_issue(index:&Index,issue:&str)->String{ui::issue(index,issue)}
#[path = "metadata_artisan.rs"]
mod artisan;
#[path = "metadata_class_cache.rs"]
mod class_cache;
#[path = "metadata_config_parents.rs"]
mod config_parents;
#[path = "metadata_domains.rs"]
mod domains;
#[path = "metadata_fields.rs"]
mod fields;
#[path = "metadata_pipe.rs"]
mod pipe;
#[path = "metadata_response.rs"]
mod response;
#[path = "metadata_strings.rs"]
mod strings;
#[path = "metadata_validation.rs"]
mod validation;
#[path = "metadata_transformers.rs"]
mod transformers;

#[path = "metadata_evidence.rs"]
pub(crate) mod evidence;
#[derive(Clone, Default, Debug)]
pub(crate) struct Metadata {
    pub(crate) source_evidence: evidence::SourceEvidence,
    pub(crate) source_evidence_incomplete: bool,
    pub(crate) source_queries: BTreeMap<(String,u32), Option<Vec<String>>>,
    pub view_roots: Vec<(Option<String>, String)>,
    pub inertia_roots: Vec<String>,
    pub(crate) inertia_paths: Vec<PageRoot>,
    pub(crate) inertia_ignore_settings: bool,
    pub(crate) livewire_namespaces: Vec<LivewireNamespace>,
    installed_packages: BTreeMap<String, String>,
    pub route_files: Vec<String>,
    pub(crate) additional_gates: Names,
    pub(crate) additional_middleware: Names,
    pub(crate) route_methods: BTreeMap<String, RouteMethod>,
    model_tables: BTreeMap<String, (String, String)>,
    pub schema_directories: Vec<String>,
    pub schema_files: Vec<String>,
    pub column_methods: Vec<Value>,
    pub framework_version: Option<(u32, u32, u32)>,
    pub(crate) php_version: Option<(u32, u32, u32)>,
    pub directives: BTreeSet<String>,
    pub(crate) blade_components: crate::components::Metadata,
    pub passport: Names,
    pub enabled_packages: BTreeMap<String, String>,
    pub disabled_packages: BTreeSet<String>,
    pub issues: Vec<String>,
    pub(crate) mixins: Vec<(String, String)>,
    pub(crate) project_root: String,
    pub(crate) mixin_sources: BTreeMap<String, String>,
    pub(crate) ambiguous_forward_classes: BTreeSet<String>,
    pub(crate) forward_methods: Vec<ForwardMethod>,
    pub(crate) forward_classes: BTreeMap<String, ForwardClass>,
    pub(crate) forwarded: BTreeMap<(String, String), Forwarded>,
    ignored_classes: BTreeSet<String>,
    ignored_namespaces: BTreeSet<String>,
    inputs: BTreeSet<String>,
    source_hashes: BTreeMap<String, String>,
    external_json: BTreeMap<String, Option<Value>>,
    rules: Vec<Rule>,
    pub(crate) class_facts: BTreeMap<String, ClassFact>,
    pub(crate) ambiguous_classes: BTreeSet<String>,
    pub(crate) domain_names: BTreeMap<String, Names>,
    pub(crate) domain_options: Vec<(String, String, Value)>,
    pub(crate) domain_roots: BTreeSet<String>,
    pub(crate) artisan_snapshot: Option<Value>,
    config_pending: Vec<config_parents::Pending>,
    config_parents: BTreeMap<String, Vec<config_parents::Parent>>,
}
#[derive(Clone, Debug)]
pub(crate) struct ClassFact {
    pub full: String,
    pub path: String,
    pub parents: Vec<String>,
    pub traits: Vec<String>,
    pub properties: BTreeMap<String, PropertyFact>,
    pub entry: crate::index::Entry,
    pub references: Vec<String>,
    pub source_size: usize,
    pub cast_enum: bool,
    pub base: Option<String>,
    pub methods: BTreeSet<String>,
    pub class_kind: String,
    pub abstract_type: bool,
    pub adaptations: bool,
    pub source_hash: String,
}
#[derive(Clone, Debug)]
pub(crate) struct PropertyFact {
    pub visibility: String,
    pub entry: crate::index::Entry,
    pub class_target: Option<String>,
}
/// Documented routing.routeFacadeMethods entry; argument indices remain 1-based.
#[derive(Clone, Debug)]
pub(crate) struct RouteMethod {
    pub name: String,
    pub url_index: Option<usize>,
    pub controller_index: Option<usize>,
    pub http_method: String,
}
#[derive(Clone, Debug)]
pub(crate) struct PageRoot {
    pub path: String,
    pub prefix: String,
    pub suffix: String,
}
#[derive(Clone, Debug)]
pub(crate) struct LivewireNamespace {
    pub source: String,
    pub class_namespace: Option<String>,
    pub view_path: Option<String>,
    pub tag_prefix: String,
}
#[derive(Clone, Debug)]
pub(crate) struct ForwardMethod {
    pub source: String,
    pub base: String,
    pub name: String,
    pub target: String,
    pub additional: Vec<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct ForwardClass {
    pub full: String,
    pub path: String,
    pub parents: Vec<String>,
    pub constructor: Option<Vec<String>>,
    pub constructor_blocked: bool,
    pub declared_methods: BTreeSet<String>,
    pub entry: crate::index::Entry,
}
#[derive(Clone, Debug)]
pub(crate) struct Forwarded {
    pub sources: Vec<String>,
    pub parameters: Vec<String>,
    pub target: String,
    pub entry: crate::index::Entry,
}
pub(crate) fn identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80)
        && bytes.all(crate::php::is_identifier)
}
fn class_name(value: &Value) -> Option<String> {
    let name = value.as_str()?.trim_matches('\\');
    (!name.is_empty() && name.split('\\').all(identifier)).then(|| name.to_owned())
}
fn metadata_type(value: &Value) -> Option<String> {
    let name = value.as_str()?;
    let primitive = matches!(
        name,
        "string"
            | "int"
            | "float"
            | "bool"
            | "array"
            | "mixed"
            | "object"
            | "callable"
            | "iterable"
    );
    if primitive {
        Some(name.into())
    } else {
        class_name(value).map(|s| format!("\\{s}"))
    }
}
#[derive(Clone, Debug)]
struct Rule {
    transformers: transformers::Pipeline,
    category: String,
    conditions: Vec<Condition>,
    strings: Names,
    prefix: Option<String>,
    source: String,
    options: Value,
}
#[derive(Clone, Debug)]
struct Condition {
    place: String,
    keys: Vec<String>,
    short_classes: Vec<String>,
    parent_classes: Vec<String>,
    short_constructors: Vec<String>,
    parent_constructors: Vec<String>,
    functions: Vec<String>,
    function_fqns: Vec<String>,
    methods: Vec<String>,
    classes: Vec<String>,
    constructors: Vec<String>,
    parameters: Vec<usize>,
    parameter_names: Vec<String>,
}
const LIMIT: usize = 256 << 10;
fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
fn list<'a>(v: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    v.get(key).and_then(Value::as_array).into_iter().flatten()
}
/// Slash-prefixed paths are project-relative, other paths are metadata-relative.
/// Parent traversal is allowed only while the resolved path remains project-owned.
pub(crate) fn resolve_path(source: &str, path: &str) -> Option<String> {
    if path.is_empty() || path.contains(['\\', '\0', '\n', '*', '?', '[', ']']) {
        return None;
    }
    let mut parts: Vec<&str> = if path.starts_with('/') {
        Vec::new()
    } else {
        source
            .rsplit_once('/')
            .map(|(d, _)| d.split('/').collect())
            .unwrap_or_default()
    };
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}
impl Metadata {
    pub(crate) fn load(project: &Project) -> Self {
        let mut result = Self::default();
        let Ok(root) = project.root() else {
            return result;
        };
        result.project_root = root.clone();
        // Package defaults first; explicit project metadata is loaded last.
        // Hidden .idea is addressed explicitly because host discovery skips it.
        let mut sources = match project.find_files("**/ide.json",129) {
            Ok(sources) => { let mut owned:Vec<_>=sources.iter().filter(|path|crate::owns_current_path(project,path)).cloned().collect(); owned.sort();owned.dedup();result.source_queries.insert(("**/ide.json".into(),129),Some(owned)); sources },
            Err(_) => { result.source_queries.insert(("**/ide.json".into(),129),None);result.source_evidence_incomplete = true; Vec::new() }
        };
        sources.sort();
        sources.dedup();
        if sources.len() > 128 {
            result
                .issues
                .push("Metadata file budget exceeded; only 128 source files are loaded".into());
            sources.truncate(128);
        }
        let mut sources: Vec<String> = sources
            .into_iter()
            .filter_map(|p| p.strip_prefix(&format!("{root}/")).map(str::to_owned))
            .filter(|p| {
                p != "ide.json"
                    && p != ".idea/ide.json"
                    && !p
                        .split('/')
                        .any(|s| matches!(s, "node_modules" | "dist" | "target"))
            })
            .collect();
        sources.extend(["ide.json".into(), ".idea/ide.json".into()]);
        for source in sources {
            if let Some(contents) = evidence::read(project,&root,&source,LIMIT,&mut result.source_evidence,&mut result.source_evidence_incomplete) {
                let extra = std::cell::RefCell::new((evidence::SourceEvidence::new(),false));
                result.extend(&source,&contents,|path| {
                    let mut capture = extra.borrow_mut();
                    let (map,incomplete) = &mut *capture;
                    evidence::read(project,&root,path,LIMIT,map,incomplete)
                });
                let (map,incomplete) = extra.into_inner(); evidence::merge(&mut result.source_evidence,&mut result.source_evidence_incomplete,map);result.source_evidence_incomplete |= incomplete;
            } else if !result.source_evidence.contains_key(&source) && project.stat(&source).is_ok_and(|stat|stat.size>LIMIT as u64) {
                result.issues.push(format!("{source}: metadata exceeds size limit"));
            }
        }
        if let Some(lock) = evidence::read(project,&root,"composer.lock",4<<20,&mut result.source_evidence,&mut result.source_evidence_incomplete) {
            if let Ok(value) = crate::strict_json::parse(&lock,4<<20) {
                result.framework_version = installed_framework_value(&value);
                result.php_version = value.pointer("/platform-overrides/php").and_then(Value::as_str).and_then(project_version);
                for section in ["packages","packages-dev"] {
                    for package in list(&value,section) {
                        if let (Some(name),Some(version)) = (package["name"].as_str(),package["version"].as_str()) { result.installed_packages.insert(name.into(),version.into()); }
                    }
                }
            }
        }
        if let Some(source) = evidence::read(project,&root,"composer.json",512*1024,&mut result.source_evidence,&mut result.source_evidence_incomplete) {
            if let Ok(value) = crate::strict_json::parse(&source,512*1024) {
                if let Some(version) = value.pointer("/config/platform/php").and_then(Value::as_str).and_then(project_version) { result.php_version = Some(version); }
            }
        }
        if !result.package_enabled("laravel/passport", true) {
            result.passport.clear();
        }
        for entry in result
            .additional_gates
            .values_mut()
            .chain(result.additional_middleware.values_mut())
        {
            entry.path = format!("{root}/{}", entry.path);
        }
        for entry in result.passport.values_mut() {
            entry.path = format!("{root}/{}", entry.path);
        }
        for rule in &mut result.rules {
            for entry in rule.strings.values_mut() {
                if !entry.path.is_empty() {
                    let absolute = format!("{root}/{}", entry.path);
                    if host::project_of(&absolute)
                        .and_then(|owner| owner.root().ok())
                        .as_deref()
                        == Some(root.as_str())
                        && project
                            .stat(&entry.path)
                            .is_ok_and(|stat| stat.kind == openidea_plugin::FileKind::File)
                    {
                        entry.path = absolute;
                    } else {
                        result.issues.push(format!("{}: stringsWithLocation target {} is missing or outside current project ownership",rule.source,entry.path));
                        entry.path.clear();
                    }
                }
            }
        }
        domains::cache(project, &mut result);
        artisan::cache(project, &mut result);
        result
    }
    fn extend(&mut self, source: &str, contents: &str, read: impl Fn(&str) -> Option<String>) {
        self.inputs.insert(source.to_owned());
        self.source_hashes
            .insert(source.into(), crate::artisan_metadata::hash(contents));
        let value = match crate::strict_json::parse(contents, LIMIT) {
            Ok(v) if v.is_object() => v,
            _ => {
                self.issues.push(format!(
                    "{source}: invalid, duplicate-key or oversized metadata JSON"
                ));
                return;
            }
        };
        self.issues.extend(validation::issues(source, &value));
        for (domain, field, target) in [
            ("auth", "additionalGates", &mut self.additional_gates),
            (
                "routing",
                "additionalMiddleware",
                &mut self.additional_middleware,
            ),
        ] {
            for item in list(&value[domain], field) {
                let Some(name) = item.as_str() else { continue };
                if name.trim().is_empty() || name.len() > 1024 || name.chars().any(char::is_control)
                {
                    self.issues
                        .push(format!("{source}: invalid {domain}.{field} name"));
                    continue;
                }
                target.remove(name);
                target.add(
                    name,
                    Some(format!("ide.json {domain}.{field}")),
                    source,
                    Position::new(0, 0),
                    0,
                );
            }
        }
        for item in list(&value["eloquent"], "modelTables") {
            let (Some(model), Some(table)) = (class_name(&item["model"]), item["table"].as_str())
            else {
                continue;
            };
            if table.trim().is_empty() || table.len() > 256 || table.chars().any(char::is_control) {
                self.issues.push(format!(
                    "{source}: invalid eloquent.modelTables table for {model}"
                ));
                continue;
            }
            let model = model.to_ascii_lowercase();
            if self
                .model_tables
                .get(&model)
                .is_some_and(|(old, _)| old != table)
            {
                self.issues.push(format!("{source}: conflicting eloquent.modelTables override for {model}; later metadata source wins"));
            }
            self.model_tables
                .insert(model, (table.into(), source.into()));
        }
        for item in list(&value["routing"], "routeFacadeMethods") {
            let Some(name) = item["name"].as_str().filter(|name| identifier(name)) else {
                continue;
            };
            let index = |key: &str| {
                item.get(key).map(|value| {
                    value
                        .as_u64()
                        .filter(|i| *i > 0 && *i <= 256)
                        .map(|i| i as usize)
                })
            };
            let (url, controller) = (index("urlIndex"), index("controllerIndex"));
            if url.as_ref().is_some_and(Option::is_none)
                || controller.as_ref().is_some_and(Option::is_none)
            {
                continue;
            }
            let http_method = item
                .get("httpMethod")
                .and_then(Value::as_str)
                .unwrap_or("GET");
            if http_method.is_empty()
                || http_method.len() > 32
                || !http_method
                    .bytes()
                    .all(|b| b.is_ascii_alphabetic() || b == b'|')
            {
                self.issues.push(format!(
                    "{source}: invalid routing.routeFacadeMethods httpMethod for {name}"
                ));
                continue;
            }
            let method = RouteMethod {
                name: name.into(),
                url_index: url.flatten(),
                controller_index: controller.flatten(),
                http_method: http_method.to_ascii_uppercase(),
            };
            self.route_methods.insert(name.to_ascii_lowercase(), method);
        }

        for item in list(&value["helperCode"]["eloquent"], "ignoreClasses") {
            if let Some(name) = class_name(item) {
                self.ignored_classes.insert(name.to_ascii_lowercase());
            } else {
                self.issues
                    .push(format!("{source}: invalid helperCode ignoreClasses entry"));
            }
        }
        for item in list(&value["helperCode"]["eloquent"], "ignoreNamespaces") {
            if let Some(name) = class_name(item) {
                self.ignored_namespaces.insert(name.to_ascii_lowercase());
            } else {
                self.issues.push(format!(
                    "{source}: invalid helperCode ignoreNamespaces entry"
                ));
            }
        }
        for item in list(&value["helperCode"], "classMixins") {
            match (class_name(&item["classFqn"]), class_name(&item["mixinFqn"])) {
                (Some(target), Some(donor)) if !target.eq_ignore_ascii_case(&donor) => {
                    if self.mixins.len() >= 64 {
                        self.issues
                            .push(format!("{source}: classMixins budget exceeded"));
                        continue;
                    }
                    self.mixin_sources
                        .insert(target.to_ascii_lowercase(), source.into());
                    self.mixins.push((target, donor))
                }
                _ => self
                    .issues
                    .push(format!("{source}: invalid helperCode classMixins entry")),
            }
        }
        for item in list(&value["helperCode"], "methodsWithConstructorParameters") {
            let Some(base) = class_name(&item["baseFqn"]) else {
                self.issues
                    .push(format!("{source}: invalid forwarding baseFqn"));
                continue;
            };
            for method in list(item, "methods") {
                let name = method["name"].as_str().filter(|s| identifier(s));
                let target = metadata_type(&method["returnType"]);
                let mut additional = Vec::new();
                let mut valid = true;
                for parameter in list(method, "additionalParameters") {
                    match (
                        parameter["name"].as_str().filter(|s| identifier(s)),
                        metadata_type(&parameter["type"]),
                    ) {
                        (Some(name), Some(kind)) => additional.push(format!("{kind} ${name}")),
                        _ => valid = false,
                    }
                }
                match (name, target) {
                    (Some(name), Some(target)) if valid => {
                        if self.forward_methods.len() >= 128 {
                            self.issues.push(format!(
                                "{source}: constructor forwarding method budget exceeded"
                            ));
                            continue;
                        }
                        self.forward_methods.push(ForwardMethod {
                            source: source.into(),
                            base: base.clone(),
                            name: name.into(),
                            target: target.trim_start_matches('\\').into(),
                            additional,
                        })
                    }
                    _ => self
                        .issues
                        .push(format!("{source}: invalid constructor forwarding method")),
                }
            }
        }
        let mut view_roots = Vec::new();
        for entry in list(&value["view"], "paths") {
            let path = entry
                .as_str()
                .or_else(|| entry.get("path").and_then(Value::as_str));
            if let Some(path) = path.and_then(|p| resolve_path(source, p)) {
                view_roots.push((
                    entry
                        .get("namespace")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned),
                    path,
                ));
            } else {
                self.issues
                    .push(format!("{source}: invalid view.paths entry"));
            }
        }
        view_roots.append(&mut self.view_roots);
        self.view_roots = view_roots;
        for entry in list(&value["routing"], "rootFiles") {
            if let Some(path) = entry.as_str().and_then(|p| resolve_path(source, p)) {
                self.route_files.push(path);
            } else {
                self.issues
                    .push(format!("{source}: invalid routing.rootFiles entry"));
            }
        }
        for (key, target) in [
            ("schemaDirectories", &mut self.schema_directories),
            ("schemaFilePaths", &mut self.schema_files),
        ] {
            for entry in list(&value["database"], key) {
                if let Some(path) = entry.as_str().and_then(|p| resolve_path(source, p)) {
                    target.push(path);
                } else {
                    self.issues
                        .push(format!("{source}: invalid database.{key} entry"));
                }
            }
        }
        self.column_methods
            .extend(list(&value["database"], "columnMigrationMethods").cloned());
        for key in ["directives", "ifDirectives", "scriptDirectives"] {
            for entry in list(&value["blade"], key) {
                if let Some(name) = entry.get("name").and_then(Value::as_str).filter(|s| {
                    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                }) {
                    self.directives.insert(name.into());
                    if key == "ifDirectives" {
                        for prefix in ["else", "unless", "end"] {
                            self.directives.insert(format!("{prefix}{name}"));
                        }
                    }
                    if key == "scriptDirectives" {
                        self.directives.insert(format!("end{name}"));
                    }
                    if entry.get("prefix").is_some() || entry.get("suffix").is_some() {
                        self.issues.push(format!(
                            "{source}: {name} injection prefix/suffix is not supported"
                        ));
                    }
                }
            }
        }
        let components =
            crate::components::metadata(source, &value, &|path| resolve_path(source, path));
        self.issues
            .extend(self.blade_components.prepend(components));
        let packages = &value["packages"];
        if let Some(map) = packages["enabledPackages"].as_object() {
            for (name, version) in map {
                if let Some(version) = version
                    .as_str()
                    .filter(|version| !version.trim().is_empty())
                {
                    if self.disabled_packages.contains(name) {
                        self.issues.push(format!("{source}: conflicting package override {name}; explicit enable supersedes an earlier source disable"));
                        self.disabled_packages.remove(name);
                    }
                    self.enabled_packages.insert(name.clone(), version.into());
                } else {
                    self.issues
                        .push(format!("{source}: invalid version override for {name}"));
                }
            }
        }
        for name in strings(&packages["disabledPackages"]) {
            if self.enabled_packages.remove(&name).is_some() {
                self.issues.push(format!("{source}: conflicting package override {name}; explicit disable wins in this source"));
            }
            self.disabled_packages.insert(name);
        }
        let inertia = if packages.get("inertia").is_some() {
            &packages["inertia"]
        } else {
            &value["inertia"]
        };
        if value.get("inertia").is_some() {
            self.issues.push(format!(
                "{source}: deprecated inertia metadata; use packages.inertia"
            ));
        }
        if let Some(ignore) = inertia["ignoreSettingsPaths"].as_bool() {
            self.inertia_ignore_settings = ignore;
        }
        let mut inertia_paths = Vec::new();
        for entry in list(inertia, "pagePaths") {
            if ["prefix", "suffix"]
                .iter()
                .any(|key| entry.get(*key).is_some_and(|v| !v.is_string()))
            {
                continue;
            }
            if let Some(path) = entry["path"]
                .as_str()
                .and_then(|path| resolve_path(source, path))
            {
                self.inertia_roots.push(path.clone());
                inertia_paths.push(PageRoot {
                    path,
                    prefix: entry["prefix"].as_str().unwrap_or("").into(),
                    suffix: entry["suffix"].as_str().unwrap_or("").into(),
                });
            } else {
                self.issues
                    .push(format!("{source}: invalid packages.inertia.pagePaths path"));
            }
        }
        inertia_paths.append(&mut self.inertia_paths);
        self.inertia_paths = inertia_paths;
        let mut livewire_namespaces = Vec::new();
        for entry in list(&packages["livewire"], "namespaces") {
            if entry.get("tagPrefix").is_some_and(|v| !v.is_string()) {
                continue;
            }
            let class_namespace = entry.get("classNamespace").and_then(class_name);
            let view_path = entry["viewPath"]
                .as_str()
                .and_then(|path| resolve_path(source, path));
            let prefix = entry["tagPrefix"].as_str().unwrap_or("");
            if class_namespace.is_none() && view_path.is_none()
                || !prefix
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
            {
                self.issues.push(format!("{source}: invalid packages.livewire.namespaces entry; need classNamespace or owned viewPath and a literal tagPrefix"));
                continue;
            }
            if entry.get("classNamespace").is_some() && class_namespace.is_none()
                || entry.get("viewPath").is_some() && view_path.is_none()
            {
                self.issues
                    .push(format!("{source}: invalid Livewire namespace/path"));
                continue;
            }
            livewire_namespaces.push(LivewireNamespace {
                source: source.into(),
                class_namespace,
                view_path,
                tag_prefix: prefix.into(),
            });
        }
        livewire_namespaces.append(&mut self.livewire_namespaces);
        self.livewire_namespaces = livewire_namespaces;
        if !self.disabled_packages.contains("laravel/passport") {
            if let Some(scopes) = packages["passport"].get("scopes") {
                self.add_scopes(source, source, scopes);
            }
            for entry in list(&packages["passport"], "scopeFiles") {
                if let Some(path) = entry.as_str().and_then(|p| resolve_path(source, p)) {
                    self.inputs.insert(path.clone());
                    let external =
                        read(&path).and_then(|s| crate::strict_json::parse(&s, LIMIT).ok());
                    self.external_json.insert(path.clone(), external.clone());
                    match external {
                        Some(v) => self.add_scopes(source, &path, &v),
                        None => self.issues.push(format!(
                            "{source}: unreadable or invalid Passport scope file {path}"
                        )),
                    }
                }
            }
        }
        for entry in list(&value, "completions") {
            let Some(category) = entry.get("complete").and_then(Value::as_str) else {
                self.issues
                    .push(format!("{source}: completion missing complete"));
                continue;
            };
            if !matches!(
                category,
                "routeName"
                    | "routeMiddleware"
                    | "configKey"
                    | "viewName"
                    | "translationKey"
                    | "validationRule"
                    | "validationRules"
                    | "staticStrings"
                    | "gate"
                    | "policy"
                    | "authRule"
                    | "assetPath"
                    | "environmentVariable"
                    | "inertiaPage"
                    | "livewireComponent"
                    | "bladeComponent"
                    | "viteAsset"
                    | "passportScopeId"
                    | "classFields"
                    | "eloquentFields"
                    | "eloquentCasts"
                    | "artisanCommandName"
                    | "artisanCommandParameter"
            ) && !domains::supported(category)
            {
                self.issues.push(format!(
                    "{source}: unsupported completion category {category}"
                ));
                continue;
            }
            let options = &entry["options"];
            let valid_keys: &[&str] = match category {
                "configKey" => &["prefix", "recursive"],
                "staticStrings" => &["strings", "stringsWithLocation", "dataFile"],
                "classFields" => &["fetchClass", "fieldsFilter", "deep", "delimiter"],
                "eloquentFields" => &["eloquentFields"],
                "directoryFiles" => &[
                    "directory",
                    "directories",
                    "prefix",
                    "recursive",
                    "suffixToClear",
                ],
                _ => &[],
            };
            if options.is_object()
                && options
                    .as_object()
                    .unwrap()
                    .keys()
                    .any(|key| !valid_keys.contains(&key.as_str()))
                || !options.is_null() && !options.is_object()
            {
                self.issues.push(format!(
                    "{source}: unsupported or malformed {category} completion options"
                ));
                continue;
            }
            let option_strings = [
                "prefix",
                "dataFile",
                "fetchClass",
                "delimiter",
                "directory",
                "suffixToClear",
            ];
            if option_strings
                .iter()
                .any(|key| options.get(*key).is_some_and(|value| !value.is_string()))
                || ["deep", "recursive"]
                    .iter()
                    .any(|key| options.get(*key).is_some_and(|value| !value.is_boolean()))
                || ["strings", "directories"].iter().any(|key| {
                    options.get(*key).is_some_and(|value| {
                        value
                            .as_array()
                            .is_none_or(|values| values.iter().any(|value| !value.is_string()))
                    })
                })
            {
                self.issues.push(format!(
                    "{source}: malformed {category} completion option shape"
                ));
                continue;
            }
            let option_issues = if fields::supported(category) {
                fields::validate(category, options)
            } else {
                domains::validate(category, options)
            };
            if !option_issues.is_empty() {
                self.issues.extend(
                    option_issues
                        .into_iter()
                        .map(|issue| format!("{source}: {issue}")),
                );
                continue;
            }
            let mut conditions = Vec::new();
            for condition in list(entry, "condition") {
                if condition.as_object().is_none_or(|o| {
                    o.keys().any(|k| {
                        !matches!(
                            k.as_str(),
                            "functionNames"
                                | "functionFqn"
                                | "parameterNames"
                                | "methodNames"
                                | "classFqn"
                                | "classNames"
                                | "classParentFqn"
                                | "newClassNames"
                                | "newClassParentFqn"
                                | "keys"
                                | "newClassFqn"
                                | "parameters"
                                | "place"
                        )
                    })
                }) {
                    self.issues
                        .push(format!("{source}: unsupported completion condition"));
                    continue;
                }
                let string_arrays = [
                    "functionNames",
                    "functionFqn",
                    "parameterNames",
                    "methodNames",
                    "classFqn",
                    "classNames",
                    "classParentFqn",
                    "newClassFqn",
                    "newClassNames",
                    "newClassParentFqn",
                    "keys",
                ];
                if string_arrays.iter().any(|key| {
                    condition.get(*key).is_some_and(|value| {
                        value
                            .as_array()
                            .is_none_or(|items| items.iter().any(|item| !item.is_string()))
                    })
                }) || condition.get("parameters").is_some_and(|value| {
                    value.as_array().is_none_or(|items| {
                        items
                            .iter()
                            .any(|item| item.as_u64().is_none_or(|n| n == 0 || n > 256))
                    })
                }) || condition
                    .get("place")
                    .is_some_and(|value| !value.is_string())
                {
                    self.issues
                        .push(format!("{source}: malformed completion condition filter"));
                    continue;
                }
                let place = condition
                    .get("place")
                    .and_then(Value::as_str)
                    .unwrap_or("parameter");
                if !matches!(
                    place,
                    "parameter"
                        | "arrayKey"
                        | "arrayOnlyKey"
                        | "arrayValue"
                        | "arrayValueWithKey"
                        | "arrayValueWithKeyArray"
                        | "arrayInArrayKey"
                        | "arrayInArrayOnlyKey"
                        | "arrayInArrayValue"
                        | "returnString"
                        | "returnArrayValue"
                        | "returnArrayKey"
                        | "returnArrayOnlyKey"
                        | "returnArrayValueWithKey"
                        | "returnInArrayValueWithKey"
                ) {
                    self.issues
                        .push(format!("{source}: unsupported completion place {place}"));
                    continue;
                }
                let functions = strings(&condition["functionNames"]);
                let function_fqns = strings(&condition["functionFqn"]);
                let parameter_names = strings(&condition["parameterNames"]);
                if condition.get("parameterNames").is_some() && parameter_names.is_empty()
                    || parameter_names.iter().any(|name| !identifier(name))
                {
                    self.issues
                        .push(format!("{source}: invalid parameterNames identifier"));
                    continue;
                }
                let methods = strings(&condition["methodNames"]);
                let classes = strings(&condition["classFqn"]);
                let constructors = strings(&condition["newClassFqn"]);
                let parameters = condition["parameters"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_u64)
                    .filter(|n| *n > 0 && *n < 256)
                    .map(|n| n as usize)
                    .collect::<Vec<_>>();
                if condition.get("parameters").is_some() && parameters.is_empty() {
                    self.issues
                        .push(format!("{source}: invalid completion parameter positions"));
                    continue;
                }
                // A method condition without exact class ownership must not match unrelated classes.
                let short_classes = strings(&condition["classNames"]);
                let parent_classes = strings(&condition["classParentFqn"]);
                let short_constructors = strings(&condition["newClassNames"]);
                let parent_constructors = strings(&condition["newClassParentFqn"]);
                let returning = place.starts_with("return");
                if returning && methods.is_empty()
                    || !returning
                        && functions.is_empty()
                        && function_fqns.is_empty()
                        && constructors.is_empty()
                        && short_constructors.is_empty()
                        && parent_constructors.is_empty()
                        && (methods.is_empty()
                            || classes.is_empty()
                                && short_classes.is_empty()
                                && parent_classes.is_empty())
                {
                    self.issues.push(format!("{source}: completion requires functionNames or exact classFqn + methodNames"));
                    continue;
                }
                conditions.push(Condition {
                    place: place.into(),
                    keys: strings(&condition["keys"]),
                    short_classes,
                    parent_classes,
                    short_constructors,
                    parent_constructors,
                    functions,
                    function_fqns,
                    parameter_names,
                    methods,
                    classes,
                    constructors,
                    parameters,
                });
            }
            let mut names = Names::new();
            for item in list(&entry["options"], "strings") {
                if let Some(name) = item.as_str() {
                    names.add(name, Some("ide.json".into()), "", Position::new(0, 0), 0);
                }
            }
            if let Some(locations) = entry["options"].get("stringsWithLocation") {
                strings::locations(
                    source,
                    locations,
                    &mut names,
                    &mut self.issues,
                    &mut self.inputs,
                );
            }
            if let Some(file) = entry["options"].get("dataFile").and_then(Value::as_str) {
                let resolved = resolve_path(source, file);
                if let Some(path) = &resolved {
                    self.inputs.insert(path.clone());
                }
                let external = resolved
                    .as_ref()
                    .and_then(|path| read(path))
                    .and_then(|data| crate::strict_json::parse(&data, LIMIT).ok());
                if let Some(path) = &resolved {
                    self.external_json.insert(path.clone(), external.clone());
                }
                match external {
                    Some(data) if data.is_array() => {
                        for (position, value) in data.as_array().unwrap().iter().enumerate() {
                            if let Some(name) = value.as_str() {
                                names.add(
                                    name,
                                    Some("ide.json dataFile".into()),
                                    "",
                                    Position::new(0, 0),
                                    0,
                                );
                            } else {
                                self.issues.push(format!("{source}: staticStrings dataFile {file}[{position}] must be a string"));
                            }
                        }
                    }
                    Some(data) if data.is_object()=>strings::locations(source,&data,&mut names,&mut self.issues,&mut self.inputs),
                    _ => self.issues.push(format!(
                        "{source}: invalid staticStrings dataFile {file}; expected an array of strings or a location map"
                    )),
                }
            }

            if entry["options"].as_object().is_some_and(|o| {
                o.keys().any(|k| {
                    !matches!(
                        k.as_str(),
                        "strings"
                            | "stringsWithLocation"
                            | "dataFile"
                            | "prefix"
                            | "fetchClass"
                            | "fieldsFilter"
                            | "eloquentFields"
                            | "deep"
                            | "delimiter"
                            | "directory"
                            | "directories"
                            | "recursive"
                            | "suffixToClear"
                    )
                })
            }) {
                self.issues
                    .push(format!("{source}: unsupported completion options"));
                continue;
            }
            if entry.get("prefix").is_some()
                || entry.get("suffix").is_some()
                || entry.get("transform").is_some()
            {
                // Shape validation above reports these unsupported properties.
                continue;
            }
            let transformers = match transformers::Pipeline::parse(entry.get("transformers")) {
                Ok(pipeline)=>pipeline,
                Err(issue)=>{self.issues.push(format!("{source}: {issue}"));continue;}
            };
            // Pinned ConfigCompletionRegistrar's explicit-prefix branch creates
            // ConfigRootBasedCompletion directly and ignores model transformers.
            let transformers = if !matches!(category,"configKey"|"translationKey"|"viewName"|"staticStrings"|"directoryFiles"|"passportScopeId")
                || category == "configKey" && entry["options"]["prefix"].is_string() {
                transformers::Pipeline::default()
            } else { transformers };
            // DirectoryFiles uses suffixToClear only when the effective custom
            // factory pipeline is empty (including unknown-only expressions).
            let mut effective_options = entry["options"].clone();
            if category == "directoryFiles" && !transformers.is_empty() {
                if let Some(options)=effective_options.as_object_mut(){options.remove("suffixToClear");}
            }
            if !conditions.is_empty() {
                if domains::supported(category) {
                    self.domain_options.push((
                        source.into(),
                        category.into(),
                        effective_options.clone(),
                    ));
                }
                self.rules.push(Rule {
                    transformers,
                    category: category.into(),
                    source: source.into(),
                    options: effective_options,
                    conditions,
                    strings: names,
                    prefix: (category == "configKey")
                        .then(|| entry["options"]["prefix"].as_str().map(str::to_owned))
                        .flatten(),
                });
            }
        }
    }
    fn add_scopes(&mut self, source: &str, path: &str, value: &Value) {
        let Some(map) = value.as_object() else {
            self.issues.push(format!(
                "{source}: Passport scopes in {path} must be an object of scope descriptions"
            ));
            return;
        };
        for (name, description) in map {
            if let Some(description) = description.as_str() {
                if name.is_empty() || name.chars().any(char::is_control) {
                    self.issues
                        .push(format!("{source}: invalid Passport scope name in {path}"));
                    continue;
                }
                self.passport.remove(name);
                self.passport
                    .add(name, Some(description.into()), path, Position::new(0, 0), 0);
            } else {
                self.issues.push(format!(
                    "{source}: Passport scope {name} in {path} must have a string description"
                ));
            }
        }
    }
    /// Explicit FQN overrides; source order is package defaults then project metadata.
    pub(crate) fn needs_class_facts(&self) -> bool {
        !self.config_pending.is_empty()
            || self.rules.iter().any(|rule| {
                fields::supported(&rule.category)
                    || rule.conditions.iter().any(|condition| {
                        !condition.parent_classes.is_empty()
                            || !condition.parent_constructors.is_empty()
                    })
            })
    }
    pub(crate) fn model_table(&self, full: &str) -> Option<&str> {
        self.model_tables
            .get(&full.trim_matches('\\').to_ascii_lowercase())
            .map(|(table, _)| table.as_str())
    }
    pub(crate) fn helper_ignored(&self, full: &str) -> bool {
        let full = full.trim_matches('\\').to_ascii_lowercase();
        self.ignored_classes.contains(&full)
            || self
                .ignored_namespaces
                .iter()
                .any(|namespace| full == *namespace || full.starts_with(&format!("{namespace}\\")))
    }
    /// Invalidate declared metadata dependencies even outside conventional Laravel roots.
    pub(crate) fn affects(&self, relative: &str) -> bool {
        relative == "ide.json"
            || relative == ".idea/ide.json"
            || relative.ends_with("/ide.json")
            || self.inputs.contains(relative)
            || !self.domain_options.is_empty()
                && (relative.ends_with(".css")
                    || relative.ends_with(".svg")
                    || relative.ends_with("/module.json")
                    || relative == "config/blade-icons.php")
            || self.domain_roots.iter().any(|root| {
                root.is_empty() || relative == root || relative.starts_with(&format!("{root}/"))
            })
            || self
                .livewire_namespaces
                .iter()
                .filter_map(|n| n.view_path.as_ref())
                .any(|root| relative == root || relative.starts_with(&format!("{root}/")))
            || self.route_files.iter().any(|path| path == relative)
            || self.schema_files.iter().any(|path| path == relative)
            || self
                .view_roots
                .iter()
                .map(|(_, path)| path)
                .chain(self.inertia_roots.iter())
                .chain(self.schema_directories.iter())
                .any(|root| relative == root || relative.starts_with(&format!("{root}/")))
    }
    pub(crate) fn package_version(&self, name: &str) -> Option<&str> {
        if self.disabled_packages.contains(name) {
            return None;
        }
        self.enabled_packages
            .get(name)
            .or_else(|| self.installed_packages.get(name))
            .map(String::as_str)
    }
    pub(crate) fn package_major(&self, name: &str) -> Option<u32> {
        self.package_version(name)?
            .trim_start_matches('v')
            .split('.')
            .next()?
            .parse()
            .ok()
    }
    pub(crate) fn package_enabled(&self, name: &str, detected: bool) -> bool {
        !self.disabled_packages.contains(name)
            && (self.enabled_packages.contains_key(name)
                || self.installed_packages.contains_key(name)
                || detected)
    }
}

pub(crate) fn prepare_parent_configs(
    project: &Project,
    metadata: &mut Metadata,
    sources: &BTreeMap<String, String>,
    roots: &[crate::package_integrations::ComposerRoot],
) {
    config_parents::prepare(project, metadata, sources, roots);
}
pub(crate) fn finish_parent_configs(project: &Project, metadata: &mut Metadata) {
    config_parents::finish(project, metadata);
}
pub(crate) fn merge_parent_config_inventory(project: &Project, index: &mut Index) {
    config_parents::merge_inventory(project, index);
}
pub(crate) fn config_parent_markers(
    index: &Index,
    project: &Project,
    doc: &openidea_plugin::providers::Document,
) -> Vec<openidea_plugin::providers::LineMarker> {
    config_parents::markers(index, project, doc)
}
pub(crate) fn config_parent_definition(
    index: &Index,
    project: &Project,
    doc: &openidea_plugin::providers::Document,
    at: usize,
) -> Vec<Location> {
    config_parents::definition(index, project, doc, at)
}
pub(crate) fn cache_package_icons(
    project: &Project,
    metadata: &mut Metadata,
    sources: &BTreeMap<String, String>,
) {
    domains::cache_package_icons(project, metadata, sources);
}
pub(crate) fn complete_class_cache(
    project: &Project,
    metadata: &mut Metadata,
    roots: &[crate::package_integrations::ComposerRoot],
) {
    class_cache::complete(project, metadata, roots);
}
pub(crate) fn pipe_reference(
    index: &Index,
    doc: &openidea_plugin::providers::Document,
    at: usize,
) -> Option<crate::refs::Reference> {
    pipe::reference(index, doc, at)
}
pub(crate) fn response_completion(
    index: &Index,
    doc: &openidea_plugin::providers::Document,
    at: usize,
) -> Vec<CompletionItem> {
    response::completion(index, doc, at)
}
pub(crate) fn refresh_artisan(project: &Project, metadata: &mut Metadata) {
    artisan::cache(project, metadata);
}
fn issue_owner(issue: &str) -> Option<&str> {
    let (prefix, _) = issue.split_once(": ")?;
    (prefix.rsplit('/').next() == Some("ide.json")).then_some(prefix)
}
/// Cached metadata issues, attributed only to their owning metadata source.
pub(crate) fn diagnostics(
    index: &Index,
    path: &str,
    root: &str,
) -> Vec<openidea_plugin::Diagnostic> {
    let Some(relative) = path.strip_prefix(&format!("{root}/")) else {
        return Vec::new();
    };
    if !relative.ends_with("ide.json") {
        return Vec::new();
    }
    index
        .metadata
        .issues
        .iter()
        .filter(|issue| issue_owner(issue).is_none_or(|owner| owner == relative))
        .take(200)
        .map(|issue| {
            let bounded: String = presentation_issue(index,issue).chars().take(1024).collect();
            let mut diagnostic = openidea_plugin::Diagnostic::new(
                TextRange::new(Position::new(0, 0), Position::new(0, 0)),
                openidea_plugin::Severity::Warning,
                bounded,
            );
            diagnostic.code = Some("laravel-metadata".into());
            diagnostic
        })
        .collect()
}

/// Revalidate changed metadata text against cached owned external snapshots.
/// Editor callbacks do not read source files or bootstrap project code.
pub(crate) fn diagnostics_current(
    index: &Index,
    path: &str,
    root: &str,
    contents: &str,
) -> Vec<openidea_plugin::Diagnostic> {
    let Some(relative) = path.strip_prefix(&format!("{root}/")) else {
        return Vec::new();
    };
    if !relative.ends_with("ide.json") {
        return Vec::new();
    }
    if contents.len() > LIMIT {
        let overlay = Index {
            metadata: Metadata {
                issues: vec![format!(
                    "{relative}: metadata exceeds the {LIMIT}-byte size limit"
                )],
                ..Metadata::default()
            },
            ..Index::default()
        };
        return diagnostics(&overlay, path, root);
    }
    if index
        .metadata
        .source_hashes
        .get(relative)
        .is_some_and(|hash| *hash == crate::artisan_metadata::hash(contents))
    {
        return diagnostics(index, path, root);
    }
    let missing = std::cell::RefCell::new(BTreeSet::new());
    let mut current = Metadata {
        project_root: root.into(),
        ..Metadata::default()
    };
    current.extend(relative, contents, |dependency| {
        let absolute = format!("{root}/{dependency}");
        if host::project_of(&absolute)
            .and_then(|project| project.root().ok())
            .as_deref()
            != Some(root)
        {
            return None;
        }
        match index.metadata.external_json.get(dependency) {
            Some(Some(value)) => serde_json::to_string(value).ok(),
            Some(None) => None,
            None => {
                missing.borrow_mut().insert(dependency.to_owned());
                None
            }
        }
    });
    let missing = missing.into_inner();
    current
        .issues
        .retain(|issue| !missing.iter().any(|dependency| issue.contains(dependency)));
    for dependency in missing {
        current.issues.push(format!("{relative}: external metadata {dependency} is not indexed; save/reindex to validate its contents"));
    }
    current.issues.extend(
        index
            .metadata
            .issues
            .iter()
            .filter(|issue| issue_owner(issue).is_none())
            .cloned(),
    );
    current.issues.sort();
    current.issues.dedup();
    let overlay = Index {
        metadata: current,
        ..Index::default()
    };
    diagnostics(&overlay, path, root)
}

fn project_version(value: &str) -> Option<(u32, u32, u32)> {
    let mut parts = value.trim_start_matches('v').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().map(str::parse).transpose().ok()?.unwrap_or(0);
    parts.next().is_none().then_some((major, minor, patch))
}
/// Only an exact installed stable lock version is evidence; constraints are not.
#[cfg(test)]
fn installed_framework(lock: &str) -> Option<(u32, u32, u32)> {
    let value = crate::strict_json::parse(lock, 4 << 20).ok()?;
    installed_framework_value(&value)
}
fn installed_framework_value(value: &Value) -> Option<(u32, u32, u32)> {
    let found = list(value, "packages")
        .filter(|p| p["name"].as_str() == Some("laravel/framework"))
        .collect::<Vec<_>>();
    if found.len() != 1 {
        return None;
    }
    let mut parts = found[0]["version"]
        .as_str()?
        .trim_start_matches('v')
        .split('.');
    let version = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(version)
}

pub(crate) struct Site {
    pub(crate) open: usize,
    pub(crate) identity: crate::eloquent_members::metadata_inference::CallIdentity,
    pub(crate) argument: usize,
    pub(crate) parameter_name: Option<String>,
    pub(crate) literal: Range<usize>,
    pub(crate) arrays: Vec<crate::eloquent_members::metadata_inference::ArrayRole>,
    pub(crate) return_context: bool,
    pub(crate) closed: bool,
    pub(crate) quote: u8,
    pub(crate) unresolved: bool,
    pub(crate) doc_path: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) containing: Option<ClassFact>,
    pub(crate) pipe: bool,
}
fn parameter_prefix(text: &str) -> Option<Option<String>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Some(None);
    }
    let name = trimmed.strip_suffix(':')?.trim();
    identifier(name).then(|| Some(name.to_owned()))
}
pub(crate) fn argument_context(
    masked: &str,
    open: usize,
    literal: usize,
) -> Option<(usize, Option<String>)> {
    let b = masked.as_bytes();
    let mut i = open + 1;
    let mut start = i;
    let mut ordinal = 1;
    let mut names = BTreeSet::new();
    let mut had_named = false;
    while i < literal {
        match b[i] {
            b'\'' | b'"' => {
                i = crate::php::string_end(b, i).min(b.len());
                continue;
            }
            b'(' | b'[' | b'{' => {
                i = crate::php::closing(masked, i)? + 1;
                continue;
            }
            b',' => {
                let prefix = masked[start..i].trim_start();
                let length = prefix
                    .bytes()
                    .take_while(|b| crate::php::is_identifier(*b))
                    .count();
                let name = &prefix[..length];
                if identifier(name) && prefix[length..].trim_start().starts_with(':') {
                    if !names.insert(name.to_owned()) {
                        return None;
                    }
                    had_named = true;
                } else if had_named {
                    return None;
                }
                ordinal += 1;
                start = i + 1;
            }
            b'.' if b[i..].starts_with(b"...") => return None,
            _ => {}
        }
        i += 1;
    }
    if i != literal {
        return None;
    }
    let name = parameter_prefix(&masked[start..literal])?;
    if name.as_ref().is_some_and(|name| names.contains(name)) || name.is_none() && had_named {
        return None;
    }
    Some((ordinal, name))
}
/// Direct literal arguments only; comments, nested arrays and interpolation do not
/// manufacture call ownership. Receiver classes are resolved only from imports,
/// explicit parameter types and straight-line literal new bindings.
fn site(text: &str, at: usize) -> Option<Site> {
    crate::eloquent_members::metadata_inference::completion_site(text, at)
        .filter(|site| !site.unresolved)
}
fn document_site(
    index: &Index,
    doc: &openidea_plugin::providers::Document,
    at: usize,
) -> Option<Site> {
    let owns = |path: &str| {
        index.metadata.project_root.is_empty()
            || host::project_of(path)
                .and_then(|owner| owner.root().ok())
                .as_deref()
                == Some(index.metadata.project_root.as_str())
    };
    if !owns(&doc.path) {
        return None;
    }
    let mut site = crate::eloquent_members::metadata_inference::completion_site(&doc.text, at)?;
    site.doc_path = Some(doc.path.clone());
    if site.unresolved {
        site.identity.class = Some(crate::eloquent_members::semantic::receiver_class(
            index, doc, site.open, &owns,
        )?);
        site.unresolved = false;
    }
    if !site.return_context
        && index.metadata.rules.iter().any(|rule| {
            rule.category == "eloquentFields"
                && rule.options["eloquentFields"]["fetch"]
                    .as_str()
                    .unwrap_or("current")
                    == "current"
        })
    {
        site.model =
            crate::eloquent_members::semantic::receiver_model(index, doc, site.open, &owns);
    }
    Some(site)
}

fn place_matches(condition: &Condition, site: &Site) -> bool {
    let depth = site.arrays.len();
    let role = site.arrays.last();
    let selected = |role: &crate::eloquent_members::metadata_inference::ArrayRole| {
        condition.keys.is_empty()
            || role
                .key
                .as_ref()
                .is_some_and(|key| condition.keys.contains(key))
    };
    if site.return_context != condition.place.starts_with("return") {
        return false;
    }
    match condition.place.as_str() {
        "parameter" | "returnString" => depth == 0,
        "arrayKey" | "returnArrayKey" => depth == 1 && role.is_some_and(|r| r.is_key || !r.has_key),
        "arrayOnlyKey" | "returnArrayOnlyKey" => depth == 1 && role.is_some_and(|r| r.is_key),
        "arrayValue" | "returnArrayValue" => depth == 1 && role.is_some_and(|r| !r.is_key),
        "arrayValueWithKey" | "returnArrayValueWithKey" => {
            depth == 1 && role.is_some_and(|r| !r.is_key && r.has_key && selected(r))
        }
        "arrayInArrayKey" => depth == 2 && role.is_some_and(|r| r.is_key || !r.has_key),
        "arrayInArrayOnlyKey" => depth == 2 && role.is_some_and(|r| r.is_key),
        "arrayInArrayValue" => depth == 2 && role.is_some_and(|r| !r.is_key),
        "arrayValueWithKeyArray" | "returnInArrayValueWithKey" => {
            depth == 2
                && role.is_some_and(|r| !r.is_key)
                && site
                    .arrays
                    .first()
                    .is_some_and(|r| !r.is_key && r.has_key && selected(r))
        }
        _ => false,
    }
}
fn matches(metadata: &Metadata, _source: &str, condition: &Condition, site: &Site) -> bool {
    if site.pipe
        && metadata
            .php_version
            .is_some_and(|version| version < (8, 5, 0))
    {
        return false;
    }
    if !place_matches(condition, site) {
        return false;
    }
    let identity = &site.identity;
    let full_matches = |expected: &String, full: &str| {
        expected.trim_start_matches('\\').eq_ignore_ascii_case(full)
    };
    let class_matches = |classes: &[String], short: &[String], parents: &[String], full: &str| {
        classes.iter().any(|expected| full_matches(expected, full))
            || short.iter().any(|expected| {
                full.rsplit('\\')
                    .next()
                    .is_some_and(|name| name.eq_ignore_ascii_case(expected))
            })
            || parents.iter().any(|parent| {
                crate::eloquent_members::metadata_inference::is_subtype_fact(
                    metadata,
                    site.containing.as_ref(),
                    full,
                    parent,
                )
            })
    };
    if site.return_context {
        return condition
            .methods
            .iter()
            .any(|method| method.eq_ignore_ascii_case(&identity.method))
            && (condition.parent_classes.is_empty()
                || identity.class.as_ref().is_some_and(|class| {
                    condition.parent_classes.iter().any(|parent| {
                        crate::eloquent_members::metadata_inference::is_subtype_fact(
                            metadata,
                            site.containing.as_ref(),
                            class,
                            parent,
                        )
                    })
                }));
    }
    if let Some(name) = &site.parameter_name {
        if !condition.parameter_names.is_empty() {
            if !condition.parameter_names.contains(name) {
                return false;
            }
        } else if !condition.parameters.is_empty() {
            return false;
        }
    } else if !condition.parameters.is_empty() {
        if !condition.parameters.contains(&site.argument) {
            return false;
        }
    } else if !condition.parameter_names.is_empty() {
        return false;
    }
    if identity.constructor {
        return identity.class.as_ref().is_some_and(|class| {
            class_matches(
                &condition.constructors,
                &condition.short_constructors,
                &condition.parent_constructors,
                class,
            )
        });
    }
    if let Some(class) = &identity.class {
        return class_matches(
            &condition.classes,
            &condition.short_classes,
            &condition.parent_classes,
            class,
        ) && condition
            .methods
            .iter()
            .chain(&condition.functions)
            .any(|name| name.eq_ignore_ascii_case(&identity.method));
    }
    if !condition.classes.is_empty()
        || !condition.short_classes.is_empty()
        || !condition.parent_classes.is_empty()
    {
        return false;
    }
    condition.functions.iter().any(|expected| {
        identity
            .function_names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(expected))
    }) || identity.function_fqn.as_ref().is_some_and(|full| {
        condition
            .function_fqns
            .iter()
            .any(|expected| full_matches(expected, full))
    })
}

fn inventory(index: &Index, rule: &Rule, source: &str, at: usize, site: &Site) -> Names {
    if matches!(
        rule.category.as_str(),
        "artisanCommandName" | "artisanCommandParameter"
    ) {
        return artisan::inventory(index, &rule.category, source, site);
    }
    if domains::supported(&rule.category) {
        return domains::inventory(
            index,
            &rule.category,
            &rule.options,
            &rule.source,
            source,
            at,
            site.identity.class.as_deref(),
        );
    }
    if fields::supported(&rule.category) {
        return fields::inventory(index, &rule.category, &rule.options, source, at, site);
    }
    match rule.category.as_str() {
        "routeName" => &index.routes,
        "routeMiddleware" => &index.middleware,
        "configKey" => &index.config,
        "viewName" => &index.views,
        "translationKey" => &index.translations,
        "validationRule" | "validationRules" => &index.rules,
        "staticStrings" => &rule.strings,
        "gate" | "policy" | "authRule" => &index.abilities,
        "assetPath" => &index.assets,
        "environmentVariable" => &index.env,
        "inertiaPage" => &index.inertia_pages,
        "livewireComponent" => &index.livewire,
        "bladeComponent" => &index.components,
        "viteAsset" => &index.vite,
        "passportScopeId" => &index.metadata.passport,
        _ => &rule.strings,
    }
    .clone()
}
/// Controller arguments of documented custom Route facade methods.
/// This feeds the same action/method inventories as standard route calls.
pub(crate) fn route_reference(
    metadata: &Metadata,
    source: &str,
    at: usize,
) -> Option<crate::refs::Reference> {
    use crate::{
        php,
        refs::{Kind, Reference},
    };
    if metadata.route_methods.is_empty()
        || source.len() > 128 * 1024
        || at > source.len()
        || !source.is_char_boundary(at)
    {
        return None;
    }
    if let Some(site) =
        site(source, at).filter(|site| site.arrays.is_empty() && !site.return_context)
    {
        let class = site.identity.class.as_deref()?;
        if !crate::eloquent_members::metadata_inference::route_calls(source)
            .iter()
            .any(|(_, method)| {
                *method < site.open
                    && source[*method..site.open]
                        .trim_end()
                        .eq_ignore_ascii_case(&site.identity.method)
            })
        {
            return None;
        }
        if !class.eq_ignore_ascii_case("Route")
            && !class.eq_ignore_ascii_case("Illuminate\\Support\\Facades\\Route")
        {
            return None;
        }
        let method = metadata
            .route_methods
            .get(&site.identity.method.to_ascii_lowercase())?;
        if site.parameter_name.is_none() && method.controller_index == Some(site.argument) {
            return Some(Reference {
                kind: Kind::Action,
                start: site.literal.start,
                end: site.literal.end,
                class: None,
                closed: source
                    .as_bytes()
                    .get(site.literal.end)
                    .is_some_and(|b| matches!(b, b'\'' | b'"')),
            });
        }
        return None;
    }
    let masked = php::mask_comments(source);
    for (_, start) in crate::eloquent_members::metadata_inference::route_calls(source) {
        let tail = &masked[start..];
        let Some(open) = tail.find('(').map(|i| i + start) else {
            continue;
        };
        let name = masked[start..open].trim().to_ascii_lowercase();
        let Some(method) = metadata.route_methods.get(&name) else {
            continue;
        };
        let Some(index) = method.controller_index else {
            continue;
        };
        let args = php::arguments(&masked, open);
        let Some(range) = args
            .get(index - 1)
            .cloned()
            .filter(|range| range.contains(&at))
        else {
            continue;
        };
        // Positional indices are not valid after an unpacked/named argument.
        if args.iter().take(index).any(|range| {
            let arg = masked[range.clone()].trim_start();
            arg.starts_with("...")
                || arg
                    .split_once(':')
                    .is_some_and(|(name, _)| identifier(name.trim()))
        }) {
            continue;
        }
        let Some(array) = php::array_open(&masked, range.clone()) else {
            continue;
        };
        if php::closing(&masked, array)
            .is_none_or(|end| end + 1 != php::trimmed(&masked, range).end)
        {
            continue;
        }
        let items = php::items(&masked, array);
        if items.len() != 2 || items.iter().any(|item| item.key.is_some()) {
            continue;
        }
        let Some(class) = php::class_constant(&masked, items[0].value.clone()) else {
            continue;
        };
        if class_name(&Value::String(class.into())).is_none() {
            continue;
        }
        let Some(class) =
            crate::eloquent_members::metadata_inference::class_identity(source, class)
        else {
            continue;
        };
        let Some((name, start)) = php::literal(&masked, items[1].value.clone()) else {
            continue;
        };
        if name.contains(['$', '\\']) || at < start || at > start + name.len() {
            continue;
        }
        return Some(Reference {
            kind: Kind::Method,
            start,
            end: start + name.len(),
            class: Some(class),
            closed: true,
        });
    }
    None
}

fn completion_range(rule: &Rule, source: &str, site: &Site, at: usize) -> Option<Range<usize>> {
    if matches!(rule.category.as_str(), "eloquentFields" | "classFields") {
        return fields::range(&rule.options, source, at, site);
    }
    if !matches!(
        rule.category.as_str(),
        "cssClassNames" | "validationRule" | "validationRules"
    ) {
        return Some(site.literal.clone());
    }
    let decoded = crate::eloquent_members::metadata_inference::decoded_offsets(
        source.get(site.literal.clone())?,
        site.quote,
    )?;
    let contents = &decoded.value;
    let relative = decoded.decoded_at(at.checked_sub(site.literal.start)?)?;
    let (from, to) = if rule.category == "cssClassNames" {
        let from = contents[..relative]
            .rfind(char::is_whitespace)
            .map_or(0, |i| i + contents[i..].chars().next().unwrap().len_utf8());
        let to = relative
            + contents[relative..]
                .find(char::is_whitespace)
                .unwrap_or(contents.len() - relative);
        (from, to)
    } else {
        if rule.category == "validationRule" && contents.contains('|') {
            return None;
        }
        if rule.category == "validationRules"
            && contents[..relative].split('|').any(|segment| {
                segment
                    .split_once(':')
                    .is_some_and(|(name, _)| matches!(name.trim(), "regex" | "not_regex"))
            })
        {
            return None;
        }
        let from = if rule.category == "validationRules" {
            contents[..relative].rfind('|').map_or(0, |i| i + 1)
        } else {
            0
        };
        if contents[from..relative].contains(':') {
            return None;
        }
        let to = relative
            + contents[relative..]
                .find(['|', ':'])
                .unwrap_or(contents.len() - relative);
        (from, to)
    };
    Some(site.literal.start + decoded.raw_at(from)?..site.literal.start + decoded.raw_at(to)?)
}

pub(crate) fn completion(index: &Index, source: &str, at: usize) -> Vec<CompletionItem> {
    if index.metadata.rules.is_empty() {
        return Vec::new();
    }
    let Some(site) = site(source, at) else {
        return Vec::new();
    };
    complete_site(index, source, at, site)
}
pub(crate) fn completion_document(
    index: &Index,
    doc: &openidea_plugin::providers::Document,
    at: usize,
) -> Vec<CompletionItem> {
    if index.metadata.rules.is_empty() {
        return Vec::new();
    }
    let Some(site) = document_site(index, doc, at) else {
        return Vec::new();
    };
    complete_site(index, &doc.text, at, site)
}
fn complete_site(index: &Index, source: &str, at: usize, mut site: Site) -> Vec<CompletionItem> {
    if index.metadata.needs_class_facts() {
        site.containing = crate::eloquent_members::metadata_inference::class_fact(
            source,
            site.doc_path.as_deref().unwrap_or(""),
        );
    }
    let mut out = BTreeMap::new();
    for rule in &index.metadata.rules {
        if let Some(condition) = rule
            .conditions
            .iter()
            .find(|condition| matches(&index.metadata, source, condition, &site))
        {
            let Some(replace) = completion_range(rule, source, &site, at) else {
                continue;
            };
            for (name, entry) in &inventory(index, rule, source, at, &site) {
                let name = if let Some(prefix) = &rule.prefix {
                    let Some(short) = name.strip_prefix(&format!("{prefix}.")) else {
                        continue;
                    };
                    if !rule.options["recursive"].as_bool().unwrap_or(false) && short.contains('.') { continue; }
                    short
                } else {
                    name.as_str()
                };
                let Some(name) = rule.transformers.forward(name) else {continue;};
                let mut item = CompletionItem::new(&name, CompletionKind::Value);
                item.detail = entry.detail.as_ref().map(|detail|match detail.as_str() {
                    "Ambiguous project sources" => crate::ui_text::t("metadata.detail.ambiguousSources","Ambiguous project sources"),
                    "Ambiguous icon registrations" => crate::ui_text::t("metadata.detail.ambiguousIcons","Ambiguous icon registrations"),
                    "source cast class or enum" => crate::ui_text::t("metadata.detail.castClass","Source cast class or enum"),
                    _ => detail.clone(),
                });
                let mut end = replace.end;
                let encoded =
                    crate::eloquent_members::metadata_inference::encode_literal(&name, site.quote);
                let key_conversion = matches!(
                    condition.place.as_str(),
                    "arrayKey" | "arrayInArrayKey" | "returnArrayKey"
                ) && site.arrays.last().is_some_and(|role| !role.has_key)
                    && replace == site.literal;
                if key_conversion {
                    end += usize::from(site.closed);
                    let escaped = encoded
                        .replace('\\', "\\\\")
                        .replace('$', "\\$")
                        .replace('}', "\\}");
                    item.insert_text = format!("{escaped}{} => $0", char::from(site.quote));
                    item.snippet = true;
                } else {
                    item.insert_text = encoded;
                }
                item.replace = Some(TextRange::new(
                    text::position(source, replace.start),
                    text::position(source, end),
                ));
                out.insert(name.to_owned(), item);
            }
        }
    }
    out.into_values().collect()
}
pub(crate) fn definition(index: &Index, source: &str, at: usize) -> Vec<Location> {
    if index.metadata.rules.is_empty() {
        return Vec::new();
    }
    let Some(site) = site(source, at) else {
        return Vec::new();
    };
    define_site(index, source, at, site)
}
pub(crate) fn definition_document(
    index: &Index,
    doc: &openidea_plugin::providers::Document,
    at: usize,
) -> Vec<Location> {
    if index.metadata.rules.is_empty() {
        return Vec::new();
    }
    let Some(site) = document_site(index, doc, at) else {
        return Vec::new();
    };
    define_site(index, &doc.text, at, site)
}
fn define_site(index: &Index, source: &str, at: usize, mut site: Site) -> Vec<Location> {
    if index.metadata.needs_class_facts() {
        site.containing = crate::eloquent_members::metadata_inference::class_fact(
            source,
            site.doc_path.as_deref().unwrap_or(""),
        );
    }
    let mut out = Vec::new();
    for rule in &index.metadata.rules {
        if rule
            .conditions
            .iter()
            .any(|c| matches(&index.metadata, source, c, &site))
        {
            let Some(range) = completion_range(rule, source, &site, at) else {
                continue;
            };
            let Some(name) = crate::eloquent_members::metadata_inference::decode_literal(
                &source[range],
                site.quote,
            ) else {
                continue;
            };
            let name = rule.transformers.back(&name);
            if let Some(entry) = inventory(index, rule, source, at, &site).get(
                &rule
                    .prefix
                    .as_ref()
                    .map_or_else(|| name.clone(), |p| format!("{p}.{name}")),
            ) {
                if !entry.path.is_empty() {
                    out.push(Location {
                        path: entry.path.clone(),
                        range: TextRange::new(
                            entry.at,
                            Position::new(entry.at.line, entry.at.character + entry.width),
                        ),
                    });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(source: &str) -> Metadata {
        let mut m = Metadata::default();
        m.extend("ide.json", source, |_| None);
        m
    }
    #[test]
    fn path_semantics_are_metadata_relative_and_never_escape() {
        assert_eq!(
            resolve_path(".idea/ide.json", "../views"),
            Some("views".into())
        );
        assert_eq!(
            resolve_path("vendor/pkg/ide.json", "/views"),
            Some("views".into())
        );
        assert_eq!(resolve_path("ide.json", "../outside"), None);
        assert_eq!(resolve_path("ide.json", "x/**/*.php"), None);
    }
    #[test]
    fn invalid_duplicates_do_not_partially_load() {
        let m = parse(r#"{"packages":{"enabledPackages":{"x":"1","x":"2"}}}"#);
        assert!(m.enabled_packages.is_empty());
        assert_eq!(m.issues.len(), 1);
    }
    #[test]
    fn package_disabling_wins_and_scope_files_are_bounded() {
        let mut m = Metadata::default();
        m.extend(".idea/ide.json",r#"{"packages":{"enabledPackages":{"laravel/passport":"12"},"disabledPackages":["laravel/passport"],"passport":{"scopes":{"read":"Read"}}}}"#,|_|None);
        assert!(!m.package_enabled("laravel/passport", true));
        assert!(m.passport.is_empty());
    }
    #[test]
    fn custom_function_completion_reuses_real_inventory_and_navigation() {
        let mut index = Index::default();
        index.metadata = parse(
            r#"{"completions":[{"complete":"routeName","condition":[{"functionNames":["tenantRoute"],"parameters":[2]}]}]}"#,
        );
        index.routes.add(
            "users.index",
            None,
            "/app/routes/web.php",
            Position::new(3, 2),
            11,
        );
        let source = "<?php tenantRoute(42, 'users.index');";
        let at = source.find("users.index").unwrap() + 3;
        assert_eq!(completion(&index, source, at)[0].label, "users.index");
        assert_eq!(
            definition(&index, source, at)[0].path,
            "/app/routes/web.php"
        );
        assert!(completion(&index, "<?php tenantRoute('x');", 20).is_empty());
    }
    #[test]
    fn static_methods_require_exact_class_and_literals_cannot_fake_calls() {
        let m = parse(
            r#"{"completions":[{"complete":"staticStrings","options":{"strings":["yes"]},"condition":[{"methodNames":["pick"],"classFqn":["\\Pkg\\Chooser"],"parameters":[1]}]}]}"#,
        );
        let mut index = Index::default();
        index.metadata = m;
        for source in [
            "<?php \\Pkg\\Chooser::pick('');",
            "<?php Chooser::pick('');",
            "<?php $x->pick('');",
            "<?php // \\Pkg\\Chooser::pick('');",
        ] {
            let at = source.find("''").unwrap() + 1;
            assert_eq!(
                !completion(&index, source, at).is_empty(),
                source.starts_with("<?php \\")
            );
        }
    }
    #[test]
    fn expressions_are_withheld_and_array_sites_remain_distinct_from_parameters() {
        for source in ["<?php f('x'.'unsafe');", r#"<?php f("$unsafe");"#] {
            assert!(site(source, source.find("unsafe").unwrap() + 2).is_none());
        }
        let source = "<?php f(['safe']);";
        assert_eq!(
            site(source, source.find("safe").unwrap() + 2)
                .unwrap()
                .arrays
                .len(),
            1
        );
        let source = "<?php f('$literal');";
        assert!(site(source, source.find("literal").unwrap() + 2).is_some());
    }
    #[test]
    fn unsupported_conditions_are_not_broadened() {
        let m = parse(
            r#"{"completions":[{"complete":"routeName","condition":[{"functionNames":["f"],"place":"futurePlace"}]}]}"#,
        );
        assert!(m.rules.is_empty());
        assert!(!m.issues.is_empty());
    }
    #[test]
    fn installed_versions_reject_ranges_prereleases_and_duplicate_frameworks() {
        assert_eq!(
            installed_framework(
                r#"{"packages":[{"name":"laravel/framework","version":"v12.7.1"}]}"#
            ),
            Some((12, 7, 1))
        );
        for version in ["^12.0", "12.1.0-dev", "dev-master"] {
            assert_eq!(
                installed_framework(&format!(
                    r#"{{"packages":[{{"name":"laravel/framework","version":"{version}"}}]}}"#
                )),
                None
            );
        }
    }
    #[test]
    fn index_loads_roots_scopes_schema_and_custom_completion_without_execution() {
        let host = openidea_plugin::mock::MockHost::install();
        host.add_file("ide.json",r#"{"view":{"paths":[{"path":"/modules/blog/views","namespace":"blog"}]},"routing":{"rootFiles":["/extra/routes.php"]},"packages":{"passport":{"scopeFiles":["/scopes.json"]},"inertia":{"pagePaths":[{"path":"/frontend/pages"}]}},"completions":[{"complete":"viewName","condition":[{"functionNames":["tenantView"]}]}]}"#);
        host.add_file("modules/blog/views/home.blade.php", "Hello");
        host.add_file(
            "extra/routes.php",
            "<?php Route::get('/extra', fn()=>1)->name('extra');",
        );
        host.add_file("frontend/pages/Show.vue", "<template/>");
        host.add_file("scopes.json", r#"{"read":"Read content"}"#);
        let index = Index::build(&openidea_plugin::host::projects().unwrap().remove(0));
        assert!(index.views.contains_key("blog::home"));
        assert!(index.routes.contains_key("extra"));
        assert!(index.inertia_pages.contains_key("Show"));
        assert_eq!(index.metadata.passport["read"].path, "/project/scopes.json");
        let source = "<?php tenantView('');";
        assert!(
            completion(&index, source, source.find("''").unwrap() + 1)
                .iter()
                .any(|i| i.label == "blog::home")
        );
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn constructors_and_config_prefix_use_documented_options() {
        let mut index = Index::default();
        index.metadata = parse(
            r#"{"completions":[{"complete":"staticStrings","options":{"strings":["latitude","longitude"]},"condition":[{"newClassFqn":["\\Geo\\Axis"],"parameters":[1]}]},{"complete":"configKey","options":{"prefix":"database.connections"},"condition":[{"functionNames":["connection"],"place":"parameter"}]}]}"#,
        );
        index.config.add(
            "database.connections.mysql",
            None,
            "/project/config/database.php",
            Position::new(0, 0),
            0,
        );
        index.config.add(
            "app.name",
            None,
            "/project/config/app.php",
            Position::new(0, 0),
            0,
        );
        let source = "<?php new \\Geo\\Axis('');";
        assert_eq!(
            completion(&index, source, source.find("''").unwrap() + 1).len(),
            2
        );
        let source = "<?php connection('mysql');";
        assert_eq!(
            completion(&index, source, source.find("mysql").unwrap())[0].label,
            "mysql"
        );
        assert_eq!(
            definition(&index, source, source.find("mysql").unwrap())[0].path,
            "/project/config/database.php"
        );
    }
    #[test]
    fn package_metadata_paths_are_relative_and_nested_projects_are_excluded() {
        let host = openidea_plugin::mock::MockHost::install();
        host.add_file(
            "vendor/acme/pkg/ide.json",
            r#"{"view":{"paths":[{"path":"views","namespace":"acme"}]}}"#,
        );
        host.add_file("vendor/acme/pkg/views/card.blade.php", "Card");
        host.add_project("/project/nested");
        host.add_file(
            "nested/ide.json",
            r#"{"view":{"paths":[{"path":"/foreign","namespace":"foreign"}]}}"#,
        );
        host.add_file("foreign/card.blade.php", "Foreign");
        let index = Index::build(&openidea_plugin::host::projects().unwrap().remove(0));
        assert!(index.views.contains_key("acme::card"));
        assert!(!index.views.contains_key("foreign::card"));
    }
    #[test]
    fn vite_config_and_manifest_add_owned_inputs_and_manifest_only_assets() {
        let host = openidea_plugin::mock::MockHost::install();
        host.add_file("vite.config.js","import laravel from 'laravel-vite-plugin'; export default {plugins:[laravel({input:['frontend/app.ts'],buildDirectory:'assets'})]};");
        host.add_file("frontend/app.ts", "export default 1;");
        host.add_file("public/assets/manifest.json",r#"{"resources/images/logo.svg":{"file":"assets/logo-hash.svg","src":"resources/images/logo.svg"}}"#);
        let index = Index::build(&openidea_plugin::host::projects().unwrap().remove(0));
        assert_eq!(
            index.vite["frontend/app.ts"].path,
            "/project/frontend/app.ts"
        );
        assert_eq!(
            index.vite["resources/images/logo.svg"].path,
            "/project/public/assets/manifest.json"
        );
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn dynamic_calls_heredocs_and_unterminated_escaped_strings_are_withheld() {
        for source in [
            "<?php $f('');",
            "<?php $x->f('');",
            "<?php $x = <<<TXT\nf('');\nTXT;",
        ] {
            let at = source.find("''").unwrap() + 1;
            assert!(site(source, at).is_none());
        }
        let source = "<?php f('tail\\";
        assert!(site(source, source.len()).is_none());
    }
    #[test]
    fn invalidation_tracks_external_inputs_and_declared_roots() {
        let mut metadata = Metadata::default();
        metadata.extend("ide.json",r#"{"view":{"paths":[{"path":"/modules/views"}]},"packages":{"passport":{"scopeFiles":["/data/scopes.json"]}},"completions":[{"complete":"staticStrings","options":{"dataFile":"/data/values.json"},"condition":[{"functionNames":["f"]}]}]}"#, |_|Some("[]".into()));
        for path in [
            "vendor/pkg/ide.json",
            "modules/views/new.blade.php",
            "data/scopes.json",
            "data/values.json",
        ] {
            assert!(metadata.affects(path), "{path}");
        }
        assert!(!metadata.affects("modules/views-other/a.php"));
        assert!(!metadata.affects("unrelated/a.json"));
    }
    #[test]
    fn nested_unknown_and_malformed_shapes_have_attributed_diagnostics() {
        let metadata = parse(
            r#"{"packages":{"livewire":{"namespaces":"bad","typo":true},"inertia":{"pagePaths":[{"path":"/pages","prefix":2,"extra":true}]},"enabledPackages":{"laravel/nova":5}},"routing":{"rootFiles":false,"unexpected":[]},"helperCode":{"eloquent":{"ignoreClasses":"bad","unknown":true}},"database":{"schemaDirectories":[4]}}"#,
        );
        for field in [
            "packages.livewire.namespaces",
            "packages.livewire.typo",
            "packages.inertia.pagePaths[0].prefix",
            "packages.inertia.pagePaths[0].extra",
            "packages.enabledPackages.laravel/nova",
            "routing.rootFiles",
            "routing.unexpected",
            "helperCode.eloquent.ignoreClasses",
            "helperCode.eloquent.unknown",
            "database.schemaDirectories[0]",
        ] {
            assert!(
                metadata
                    .issues
                    .iter()
                    .any(|i| i.starts_with("ide.json:") && i.contains(field)),
                "{field}: {:?}",
                metadata.issues
            );
        }
        assert!(metadata.livewire_namespaces.is_empty());
        assert!(metadata.inertia_paths.is_empty());
    }
    #[test]
    fn disabled_packages_remove_their_inventory_but_preserve_core_blade() {
        let host = openidea_plugin::mock::MockHost::install();
        host.add_file("composer.json",r#"{"require":{"laravel/framework":"^12","filament/filament":"^4","laravel/nova":"^5"}}"#);
        host.add_file("ide.json",r#"{"packages":{"disabledPackages":["livewire/livewire","inertiajs/inertia-laravel","filament/filament","laravel/nova"],"livewire":{"namespaces":[{"viewPath":"/module/views"}]},"inertia":{"pagePaths":[{"path":"/frontend"}]}}}"#);
        host.add_file(
            "app/Livewire/Counter.php",
            "<?php namespace App\\Livewire; class Counter extends Component {}",
        );
        host.add_file("resources/js/Pages/Home.vue", "<template/>");
        host.add_file("frontend/Other.vue", "<template/>");
        host.add_file("module/views/counter.blade.php", "Counter");
        host.add_file("resources/views/components/card.blade.php", "Card");
        let index = Index::build(&host::projects().unwrap().remove(0));
        assert!(index.livewire.is_empty());
        assert!(index.inertia_pages.is_empty());
        assert!(index.components.contains_key("card"));
        assert!(index.views.contains_key("components.card"));
        assert!(!index.open_components);
        for package in [
            "livewire/livewire",
            "inertiajs/inertia-laravel",
            "filament/filament",
            "laravel/nova",
        ] {
            assert!(!index.metadata.package_enabled(package, true));
        }
    }
    #[test]
    fn livewire_namespace_maps_psr4_classes_and_views_with_owned_precedence() {
        let host = openidea_plugin::mock::MockHost::install();
        host.add_file(
            "composer.json",
            r#"{"autoload":{"psr-4":{"Modules\\Blog\\":"modules/Blog/"}}}"#,
        );
        host.add_file("ide.json",r#"{"packages":{"enabledPackages":{"livewire/livewire":"4.0.1"},"livewire":{"namespaces":[{"classNamespace":"Modules\\Blog\\Livewire","viewPath":"/modules/Blog/views","tagPrefix":"blog"}]}}}"#);
        host.add_file("modules/Blog/Livewire/Forms/Editor.php","<?php namespace Modules\\Blog\\Livewire\\Forms; class Editor extends \\Livewire\\Component {}");
        host.add_file("modules/Blog/views/forms/editor.blade.php", "Editor");
        host.add_file("modules/Blog/views/status.blade.php", "Status");
        host.add_project("/project/modules/Blog/Livewire/Private");
        host.add_file(
            "modules/Blog/Livewire/Private/Secret.php",
            "<?php namespace Modules\\Blog\\Livewire\\Private; class Secret extends Component {}",
        );
        let index = Index::build(&host::projects().unwrap().remove(0));
        assert_eq!(
            index.livewire["blog::forms.editor"].path,
            "/project/modules/Blog/Livewire/Forms/Editor.php"
        );
        assert_eq!(
            index.livewire["blog::status"].path,
            "/project/modules/Blog/views/status.blade.php"
        );
        assert!(!index.livewire.contains_key("blog::private.secret"));
        assert_eq!(
            index.metadata.package_version("livewire/livewire"),
            Some("4.0.1")
        );
        assert_eq!(index.metadata.package_major("livewire/livewire"), Some(4));
        assert!(index.affects("modules/Blog/Livewire/NewComponent.php"));
    }
    #[test]
    fn package_versions_and_overrides_have_deterministic_source_precedence() {
        let host = openidea_plugin::mock::MockHost::install();
        host.add_file("composer.lock",r#"{"packages":[{"name":"livewire/livewire","version":"v3.6.2"},{"name":"filament/filament","version":"v4.0.0"}]}"#);
        host.add_file("vendor/acme/pkg/ide.json",r#"{"packages":{"disabledPackages":["livewire/livewire"],"enabledPackages":{"laravel/nova":"4.0.0"}}}"#);
        host.add_file("ide.json",r#"{"packages":{"enabledPackages":{"livewire/livewire":"4.0.1","laravel/nova":"5.0.0"}}}"#);
        host.add_file(
            ".idea/ide.json",
            r#"{"packages":{"disabledPackages":["laravel/nova"]}}"#,
        );
        let metadata = Metadata::load(&host::projects().unwrap().remove(0));
        assert_eq!(metadata.package_version("livewire/livewire"), Some("4.0.1"));
        assert_eq!(metadata.package_major("filament/filament"), Some(4));
        assert!(metadata.package_enabled("livewire/livewire", false));
        assert!(!metadata.package_enabled("laravel/nova", true));
        assert_eq!(metadata.package_version("laravel/nova"), None);
        assert!(
            metadata
                .issues
                .iter()
                .any(|i| i.contains("conflicting package override"))
        );
    }
    #[test]
    fn inertia_documented_paths_ignore_defaults_and_transform_names() {
        let host = openidea_plugin::mock::MockHost::install();
        host.add_file("ide.json",r#"{"packages":{"inertia":{"ignoreSettingsPaths":true,"pagePaths":[{"path":"/frontend/pages","prefix":"Tenant/","suffix":"Page"}]}}}"#);
        host.add_file("frontend/pages/Posts/Index.vue", "<template/>");
        host.add_file("resources/js/Pages/Default.vue", "<template/>");
        let index = Index::build(&host::projects().unwrap().remove(0));
        assert_eq!(
            index
                .inertia_pages
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["Tenant/Posts/IndexPage"]
        );
    }
    #[test]
    fn explicit_view_metadata_precedes_defaults_and_diagnoses_unmapped_livewire() {
        let host = openidea_plugin::mock::MockHost::install();
        host.add_file("ide.json",r#"{"view":{"paths":[{"path":"/custom/views"}]},"packages":{"livewire":{"namespaces":[{"classNamespace":"Unknown\\Livewire"}]}}}"#);
        host.add_file("custom/views/home.blade.php", "Custom");
        host.add_file("resources/views/home.blade.php", "Default");
        let index = Index::build(&host::projects().unwrap().remove(0));
        assert_eq!(
            index.views["home"].path,
            "/project/custom/views/home.blade.php"
        );
        assert!(
            index
                .metadata
                .issues
                .iter()
                .any(|issue| issue.contains("no owned Composer PSR-4 mapping"))
        );
    }
    fn custom_views(config: &str) -> Index {
        let mut index = Index::default();
        index.metadata = parse(config);
        index.views.add(
            "home",
            None,
            "/project/resources/views/home.blade.php",
            Position::new(0, 0),
            0,
        );
        index
    }
    fn completed(index: &Index, source: &str) -> bool {
        let at = source
            .rfind("'home'")
            .map(|at| at + 2)
            .expect("marked home literal");
        completion(index, source, at)
            .iter()
            .any(|i| i.label == "home")
    }
    #[test]
    fn named_argument_names_override_lexical_position_without_guessing_signature() {
        let index = custom_views(
            r#"{"completions":[{"complete":"viewName","condition":[{"functionNames":["renderView"],"parameters":[1],"parameterNames":["template"]}]}]}"#,
        );
        assert!(
            index.metadata.issues.is_empty(),
            "{:?}",
            index.metadata.issues
        );
        for source in [
            "<?php renderView(template: 'home');",
            "<?php renderView(data: [], template: 'home');",
            "<?php renderView('home');",
        ] {
            assert!(completed(&index, source), "{source}");
        }
        for source in [
            "<?php renderView(other: 'home');",
            "<?php renderView([], 'home');",
            "<?php renderView(...$args, template: 'home');",
            "<?php renderView(template:'old',template:'home');",
            "<?php renderView(data: [], 'home');",
        ] {
            assert!(!completed(&index, source), "{source}");
        }
        let source = "<?php renderView(data: [], template: 'home');";
        let at = source.find("'home'").unwrap() + 2;
        assert_eq!(
            definition(&index, source, at)[0].path,
            "/project/resources/views/home.blade.php"
        );
        let index = custom_views(
            r#"{"completions":[{"complete":"viewName","condition":[{"functionNames":["renderView"],"parameterNames":["template"]}]}]}"#,
        );
        assert!(completed(&index, "<?php renderView(template:'home');"));
        assert!(!completed(&index, "<?php renderView('home');"));
    }
    #[test]
    fn imported_static_and_constructor_classes_are_resolved_without_suffix_guessing() {
        let index = custom_views(
            r#"{"completions":[{"complete":"viewName","condition":[{"classFqn":["\\Pkg\\Chooser"],"methodNames":["pick"]},{"newClassFqn":["\\Pkg\\Chooser"]}]}]}"#,
        );
        for source in [
            "<?php use Pkg\\Chooser as C; C::pick('home');",
            "<?php use Pkg\\Chooser as C; new C('home');",
            "<?php namespace Pkg; Chooser::pick('home');",
            "<?php use Pkg\\{Chooser as C}; C::pick('home');",
        ] {
            assert!(completed(&index, source), "{source}");
        }
        for source in [
            "<?php use Other\\Chooser as C; C::pick('home');",
            "<?php use Pkg\\Chooser as C; use Other\\Chooser as C; C::pick('home');",
            "<?php $class::pick('home');",
            "<?php namespace A; use Pkg\\Chooser as C; namespace B; C::pick('home');",
        ] {
            assert!(!completed(&index, source), "{source}");
        }
    }
    #[test]
    fn function_import_aliases_and_fqns_use_the_proven_function_namespace() {
        let index = custom_views(
            r#"{"completions":[{"complete":"viewName","condition":[{"functionFqn":["\\Pkg\\renderView"],"parameterNames":["template"]}]}]}"#,
        );
        assert!(
            index.metadata.issues.is_empty(),
            "{:?}",
            index.metadata.issues
        );
        for source in [
            "<?php use function Pkg\\renderView as rv; rv(template:'home');",
            "<?php \\Pkg\\renderView(template:'home');",
            "<?php namespace Pkg; namespace\\renderView(template:'home');",
            "<?php use function Pkg\\{renderView as rv}; rv(template:'home');",
        ] {
            assert!(completed(&index, source), "{source}");
        }
        for source in [
            "<?php use function Other\\renderView as rv; rv(template:'home');",
            "<?php use function Pkg\\renderView as rv; use function Other\\renderView as rv; rv(template:'home');",
            "<?php namespace Unknown; renderView(template:'home');",
            "<?php use Pkg\\renderView as rv; rv(template:'home');",
        ] {
            assert!(!completed(&index, source), "{source}");
        }
    }
    #[test]
    fn instance_calls_use_typed_parameters_this_and_straight_line_new_bindings() {
        let index = custom_views(
            r#"{"completions":[{"complete":"viewName","condition":[{"classFqn":["\\Pkg\\Chooser"],"methodNames":["pick"]}]}]}"#,
        );
        for source in [
            "<?php function f(\\Pkg\\Chooser $p){$p->pick('home');}",
            "<?php use Pkg\\Chooser as C; function f(?C $p){$p?->pick('home');}",
            "<?php function f(\\Pkg\\Chooser|null $p){$p?->pick('home');}",
            "<?php use Pkg\\Chooser as C; $p=new C; $p->pick('home');",
            "<?php $p=new \\Pkg\\Chooser(); $p->pick('home');",
            "<?php use Pkg\\Chooser as C; (new C())->pick('home');",
            "<?php namespace Pkg; class Chooser {function f(){$this->pick('home');}}",
        ] {
            assert!(completed(&index, source), "{source}");
        }
        for source in [
            "<?php $p->pick('home');",
            "<?php function f(\\Other\\Chooser $p){$p->pick('home');}",
            "<?php function f(\\Pkg\\Chooser|\\Other\\Chooser $p){$p->pick('home');}",
            "<?php function f(\\Pkg\\Chooser $p){$p=unknown();$p->pick('home');}",
            "<?php function f(\\Pkg\\Chooser $p){mutate($p);$p->pick('home');}",
            "<?php function f(\\Pkg\\Chooser $p){if($condition){$p=new \\Other\\Chooser;}$p->pick('home');}",
            "<?php getPicker()->pick('home');",
            "<?php $p=new \\Pkg\\Chooser; $other=$p; $other->pick('home');",
            "<?php namespace Pkg; class Chooser {static function f(){$this->pick('home');}}",
        ] {
            assert!(!completed(&index, source), "{source}");
        }
    }
    #[test]
    fn qualified_function_imports_preserve_proven_identity() {
        let mut index = Index::default();
        index.metadata = parse(
            r#"{"completions":[{"complete":"staticStrings","options":{"strings":["home"]},"condition":[{"functionFqn":["\\Vendor\\Views\\render"]}]}]}"#,
        );
        for source in [
            "<?php namespace Site; use Vendor\\Views as V; V\\render('home');",
            "<?php namespace Site; use Vendor as V; V\\Views\\render('home');",
            "<?php namespace Site; use Vendor\\Views; Views\\render('home');",
            "<?php namespace Site; \\Vendor\\Views\\render('home');",
        ] {
            assert!(completed(&index, source), "{source}");
        }
        for source in [
            "<?php namespace Site; V\\render('home');",
            "<?php namespace Site; use Other\\Views as V; V\\render('home');",
            "<?php namespace Site; use Vendor\\Views as V; namespace\\V\\render('home');",
            "<?php namespace Site; use Vendor\\Views as V; use Other\\Views as V; V\\render('home');",
            "<?php namespace Site; use function Vendor\\Views\\render as V; V\\render('home');",
        ] {
            assert!(!completed(&index, source), "{source}");
        }
    }
    #[test]
    fn validation_list_replaces_only_current_name_with_utf16_ranges() {
        let mut index = Index::default();
        index.metadata = parse(
            r#"{"completions":[{"complete":"validationRules","condition":[{"functionNames":["rules"]}]}]}"#,
        );
        index.rules.add(
            "required",
            None,
            "/project/rules.php",
            Position::new(4, 0),
            8,
        );
        let source = "<?php /* 😀 */ rules('nullable|réq:3|email');";
        let start = source.find("réq").unwrap();
        let at = start + "ré".len();
        let out = completion(&index, source, at);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].replace,
            Some(TextRange::new(
                text::position(source, start),
                text::position(source, start + "réq".len())
            ))
        );
        let changed = format!(
            "{}required{}",
            &source[..start],
            &source[start + "réq".len()..]
        );
        assert_eq!(
            changed,
            "<?php /* 😀 */ rules('nullable|required:3|email');"
        );
        assert!(completion(&index, source, source.find(":3").unwrap() + 2).is_empty());
        let source = "<?php rules('nullable|required:3|email');";
        assert_eq!(
            definition(&index, source, source.find("required").unwrap() + 2)[0].path,
            "/project/rules.php"
        );
        index.metadata = parse(
            r#"{"completions":[{"complete":"validationRule","condition":[{"functionNames":["rules"]}]}]}"#,
        );
        assert!(completion(&index, source, source.find("required").unwrap() + 2).is_empty());
        let source = "<?php rules('réq:3');";
        assert_eq!(
            completion(&index, source, source.find("réq").unwrap() + 1).len(),
            1
        );
    }
    #[test]
    fn malformed_external_entries_are_diagnosed_without_discarding_valid_values() {
        let mut metadata = Metadata::default();
        metadata.extend("ide.json", r#"{"packages":{"passport":{"scopeFiles":["/scopes.json"]}},"completions":[{"complete":"staticStrings","options":{"dataFile":"/strings.json"},"condition":[{"functionNames":["f"]}]}]}"#, |path| {
            Some(if path == "scopes.json" {r#"{"read":"Read records","bad":4,"":"Empty"}"#} else {r#"["ok",3,{"value":"no"}]"#}.into())
        });
        assert!(metadata.passport.contains_key("read"));
        assert!(!metadata.passport.contains_key("bad"));
        assert_eq!(
            metadata.rules[0]
                .strings
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["ok"]
        );
        for expected in [
            "scope bad",
            "invalid Passport scope name",
            "strings.json[1]",
            "strings.json[2]",
        ] {
            assert!(
                metadata
                    .issues
                    .iter()
                    .any(|issue| issue.starts_with("ide.json:") && issue.contains(expected)),
                "{expected}: {:?}",
                metadata.issues
            );
        }
        metadata
            .issues
            .push("Metadata file budget exceeded; only 128 source files are loaded".into());
        let index = Index {
            metadata,
            ..Index::default()
        };
        // Parsing above remains host-free; diagnostic publication has a locale context.
        let _host = openidea_plugin::mock::MockHost::install();
        assert!(
            diagnostics(&index, "/project/ide.json", "/project")
                .iter()
                .any(|d| d.message.contains("budget exceeded"))
        );
    }
    #[test]
    fn validation_lists_with_regex_parameters_never_replace_internal_pipes() {
        let mut index = Index::default();
        index.metadata = parse(
            r#"{"completions":[{"complete":"validationRules","condition":[{"functionNames":["rules"]}]}]}"#,
        );
        index.rules.add(
            "required",
            None,
            "/project/rules.php",
            Position::new(0, 0),
            8,
        );
        for source in [
            "<?php rules('required|regex:/foo|ba/');",
            "<?php rules('required|not_regex:#foo|ba#');",
            "<?php rules('regex:/foo|ba/|email');",
            "<?php rules('required|regex:/foo|ba');",
            "<?php rules('required|regex:/foo\\|ba/');",
        ] {
            let at = source.find("ba").unwrap() + 1;
            assert!(completion(&index, source, at).is_empty(), "{source}");
            assert!(definition(&index, source, at).is_empty(), "{source}");
        }
        let source = "<?php rules('required|regex:/foo|ba/');";
        assert_eq!(
            completion(&index, source, source.find("required").unwrap() + 2).len(),
            1
        );
        assert_eq!(
            completion(&index, source, source.find("regex").unwrap() + 2).len(),
            1
        );
    }
    #[test]
    fn public_inventory_domains_have_precedence_and_precise_shape_diagnostics() {
        let mut metadata = parse(
            r#"{"auth":{"additionalGates":["package"]},"routing":{"additionalMiddleware":["tenant"],"routeFacadeMethods":[{"name":"apiGet","urlIndex":2,"controllerIndex":3}]},"eloquent":{"modelTables":[{"model":"\\App\\Models\\User","table":"legacy_users"}]}}"#,
        );
        assert!(metadata.additional_gates.contains_key("package"));
        assert!(metadata.additional_middleware.contains_key("tenant"));
        assert_eq!(metadata.route_methods["apiget"].http_method, "GET");
        assert_eq!(
            metadata.model_table("app\\models\\USER"),
            Some("legacy_users")
        );
        metadata.extend(".idea/ide.json",r#"{"eloquent":{"modelTables":[{"model":"App\\Models\\User","table":"project_users"}]}}"#,|_|None);
        assert_eq!(
            metadata.model_table("\\App\\Models\\User"),
            Some("project_users")
        );
        assert!(
            metadata
                .issues
                .iter()
                .any(|issue| issue.starts_with(".idea/ide.json:") && issue.contains("conflicting"))
        );
        let metadata = parse(
            r#"{"auth":{"additionalGates":[2,""]},"eloquent":{"modelTables":[{"model":"bad class","table":"x"},{"model":"App\\User"}]},"routing":{"additionalMiddleware":[false],"routeFacadeMethods":[{"name":"x","urlIndex":0},{"name":"y","controllerIndex":300},{"name":"bad()"},{"name":"z","httpMethod":"POST\nGET"}]}}"#,
        );
        for field in [
            "auth.additionalGates[0]",
            "eloquent.modelTables model",
            "eloquent.modelTables[1].table",
            "routing.additionalMiddleware[0]",
            "urlIndex",
            "controllerIndex",
            ".name",
            "httpMethod",
        ] {
            assert!(
                metadata.issues.iter().any(|issue| issue.contains(field)),
                "{field}: {:?}",
                metadata.issues
            );
        }
        assert!(metadata.route_methods.is_empty());
    }
    #[test]
    fn custom_route_references_require_static_owned_facade_identity() {
        let metadata = parse(
            r#"{"routing":{"routeFacadeMethods":[{"name":"apiGet","urlIndex":1,"controllerIndex":2}]}}"#,
        );
        for source in [
            "<?php Route::apiGet('/x','PostController@index');",
            "<?php use Illuminate\\Support\\Facades\\Route as R; R::apiGet('/x','PostController@index');",
            "<?php \\Illuminate\\Support\\Facades\\Route::apiGet('/x','PostController@index');",
        ] {
            let at = source.find("PostController@index").unwrap() + 5;
            let found = route_reference(&metadata, source, at).expect(source);
            assert_eq!(found.kind, crate::refs::Kind::Action);
            assert_eq!(&source[found.start..found.end], "PostController@index");
        }
        let source = "<?php use App\\Http\\Controllers\\PostController as P; use Illuminate\\Support\\Facades\\Route as R; R::apiGet('/x',[P::class,'index']);";
        let found =
            route_reference(&metadata, source, source.find("'index'").unwrap() + 2).unwrap();
        assert_eq!(found.kind, crate::refs::Kind::Method);
        assert_eq!(
            found.class.as_deref(),
            Some("App\\Http\\Controllers\\PostController")
        );
        for source in [
            "<?php use Other\\Route; Route::apiGet('/x','PostController@index');",
            "<?php class Route{} Route::apiGet('/x','PostController@index');",
            "<?php function f(\\Illuminate\\Support\\Facades\\Route $r){$r->apiGet('/x','PostController@index');}",
            "<?php Route::apiGet(...$args,'PostController@index');",
            "<?php Route::apiGet(url:'/x',handler:'PostController@index');",
            "<?php $x=\"Route::apiGet('/x','PostController@index');\";",
            "<?php Route::apiGet('/x',[$class,'index']);",
            "<?php Route::apiGet('/x',[P::class,'index']+$more);",
        ] {
            let at = source
                .find("PostController@index")
                .map(|p| p + 3)
                .unwrap_or_else(|| source.find("'index'").unwrap() + 2);
            assert!(route_reference(&metadata, source, at).is_none(), "{source}");
        }
    }
    #[test]
    fn documented_domains_reach_real_index_and_registered_providers() {
        use crate::tests::{activated, cursor, doc};
        use openidea_plugin::{mock::MockHost, providers::Providers};
        let host = MockHost::install();
        host.add_file("ide.json",r#"{"auth":{"additionalGates":["audit","view-reports"]},"routing":{"additionalMiddleware":["tenant"],"routeFacadeMethods":[{"name":"apiGet","urlIndex":1,"controllerIndex":2}]},"eloquent":{"modelTables":[{"model":"App\\Models\\LegacyUser","table":"external_users"}]}}"#);
        host.add_file("app/Models/LegacyUser.php","<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class LegacyUser extends Model {protected $table='old_users';}");
        host.add_file("database/migrations/2030_01_01_external.php","<?php use Illuminate\\Support\\Facades\\Schema; Schema::create('external_users', function($table){$table->string('external_key');});");
        host.add_file("routes/custom.php","<?php use Illuminate\\Support\\Facades\\Route as R; R::apiGet('/custom','PostController@show')->name('custom.show');");
        let mut plugin = activated(&host);
        let index = &plugin.indexes["/project"];
        assert!(index.routes.contains_key("custom.show"));
        assert_eq!(
            index.metadata.model_table("App\\Models\\LegacyUser"),
            Some("external_users")
        );
        assert_eq!(
            index
                .model_files
                .get("/project/app/Models/LegacyUser.php")
                .map(String::as_str),
            Some("external_users")
        );
        assert_eq!(index.abilities["audit"].path, "/project/ide.json");
        assert_eq!(
            index.abilities["view-reports"].path,
            "/project/app/Providers/AuthServiceProvider.php"
        );
        for (marked, expected) in [
            ("<?php Gate::allows('aud|');", "audit"),
            ("<?php Route::middleware('ten|');", "tenant"),
            ("<?php Route::apiGet('/x','Post|');", "PostController@index"),
            (
                "<?php use Illuminate\\Support\\Facades\\Route as R; use App\\Http\\Controllers\\PostController as P; R::apiGet('/x',[P::class,'sh|']);",
                "show",
            ),
            (
                "<?php \\App\\Models\\LegacyUser::where('ext|');",
                "external_key",
            ),
        ] {
            let (source, at) = cursor(marked);
            let found = plugin.completion(&doc("/project/test.php", "php", &source), at, None);
            assert!(
                found.iter().any(|item| item.label == expected),
                "{marked}: {found:?}"
            );
        }
        let (source, at) = cursor("<?php Route::apiGet('/x','App\\Http\\Controllers\\PostController@sh|ow');");
        assert_eq!(
            plugin.definition(&doc("/project/test.php", "php", &source), at)[0].path,
            "/project/app/Http/Controllers/PostController.php"
        );
        for marked in [
            "<?php use Other\\Route; Route::apiGet('/x','Post|');",
            "<?php class Route{} Route::apiGet('/x','Post|');",
        ] {
            let (source, at) = cursor(marked);
            assert!(
                plugin
                    .completion(&doc("/project/test.php", "php", &source), at, None)
                    .is_empty(),
                "{marked}"
            );
        }
    }
    #[test]
    fn documented_array_places_reach_registered_providers_and_preserve_utf16() {
        use crate::tests::{activated, cursor, doc};
        use openidea_plugin::{mock::MockHost, providers::Providers};
        let examples = [
            (
                "parameter",
                vec!["<?php choose('wel|');"],
                vec!["<?php choose(['wel|']);"],
            ),
            (
                "arrayKey",
                vec!["<?php choose(['wel|' => 1]);", "<?php choose(['wel|']);"],
                vec!["<?php choose('wel|');", "<?php choose(['x'=>'wel|']);"],
            ),
            (
                "arrayOnlyKey",
                vec!["<?php choose(['wel|' => 1]);"],
                vec!["<?php choose(['wel|']);"],
            ),
            (
                "arrayValue",
                vec!["<?php choose(['wel|']);", "<?php choose(['x'=>'wel|']);"],
                vec!["<?php choose(['wel|'=>1]);"],
            ),
            (
                "arrayValueWithKey",
                vec!["<?php choose(['route'=>'wel|']);"],
                vec![
                    "<?php choose(['wrong'=>'wel|']);",
                    "<?php choose(['route'=>['wel|']]);",
                ],
            ),
            (
                "arrayValueWithKeyArray",
                vec!["<?php choose(['route'=>['wel|']]);"],
                vec![
                    "<?php choose(['route'=>'wel|']);",
                    "<?php choose(['wrong'=>['wel|']]);",
                ],
            ),
            (
                "arrayInArrayKey",
                vec!["<?php choose([['wel|']]);", "<?php choose([['wel|'=>1]]);"],
                vec!["<?php choose(['wel|']);"],
            ),
            (
                "arrayInArrayOnlyKey",
                vec!["<?php choose([['wel|'=>1]]);"],
                vec!["<?php choose([['wel|']]);"],
            ),
            (
                "arrayInArrayValue",
                vec!["<?php choose([['wel|']]);"],
                vec![
                    "<?php choose([['wel|'=>1]]);",
                    "<?php choose([[['wel|']]]);",
                ],
            ),
        ];
        for (place, positive, negative) in examples {
            let host = MockHost::install();
            host.add_file("ide.json",&serde_json::json!({"completions":[{"complete":"viewName","condition":[{"functionNames":["choose"],"parameters":[2],"place":place,"keys":["route"]}]}]}).to_string());
            let mut plugin = activated(&host);
            for (valid, marked) in positive
                .into_iter()
                .map(|s| (true, s))
                .chain(negative.into_iter().map(|s| (false, s)))
            {
                let marked = marked.replacen("choose(", "choose(1,", 1);
                let (source, at) = cursor(&marked);
                let found = plugin.completion(&doc("/project/test.php", "php", &source), at, None);
                assert_eq!(
                    found.iter().any(|item| item.label == "welcome"),
                    valid,
                    "{place}: {marked}: {found:?}"
                );
            }
        }
        let host = MockHost::install();
        host.add_file("ide.json",r#"{"completions":[{"complete":"viewName","condition":[{"functionNames":["choose"],"place":"arrayKey"}]}]}"#);
        let mut plugin = activated(&host);
        let (source, at) = cursor("<?php /* 😀 */ choose(['wel|']);");
        let item = plugin
            .completion(&doc("/project/test.php", "php", &source), at, None)
            .into_iter()
            .find(|item| item.label == "welcome")
            .unwrap();
        assert!(item.snippet);
        assert_eq!(item.insert_text, "welcome' => $0");
        let range = item.replace.unwrap();
        assert_eq!(
            range.start,
            text::position(&source, source.find("wel").unwrap())
        );
        assert_eq!(
            range.end,
            text::position(&source, source.find("wel").unwrap() + 4)
        );
        let (source, at) = cursor("<?php choose(['welcome|'=>1]);");
        assert_eq!(
            plugin.definition(&doc("/project/test.php", "php", &source), at)[0].path,
            "/project/resources/views/welcome.blade.php"
        );
    }
    #[test]
    fn return_places_and_subtype_interface_conditions_use_owned_cached_facts() {
        use crate::tests::{activated, cursor, doc};
        use openidea_plugin::{mock::MockHost, providers::Providers};
        let host = MockHost::install();
        host.add_file(
            "app/Contracts/Renderable.php",
            "<?php namespace App\\Contracts; interface Renderable {}",
        );
        host.add_file(
            "app/Base/Component.php",
            "<?php namespace App\\Base; class Component implements \\App\\Contracts\\Renderable {}",
        );
        host.add_file(
            "app/Features/Card.php",
            "<?php namespace App\\Features; class Card extends \\App\\Base\\Component {}",
        );
        host.add_file("ide.json",r#"{"completions":[{"complete":"viewName","condition":[{"methodNames":["render"],"classParentFqn":["App\\Contracts\\Renderable"]},{"newClassParentFqn":["App\\Contracts\\Renderable"]},{"place":"returnString","methodNames":["template"],"classParentFqn":["App\\Contracts\\Renderable"]},{"place":"returnArrayValueWithKey","keys":["view"],"methodNames":["templates"]},{"place":"returnInArrayValueWithKey","keys":["view"],"methodNames":["nested"]}]}]}"#);
        let mut plugin = activated(&host);
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^12"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        let index = Index::build(&host.invocation().project().unwrap());
        plugin.indexes.insert("/project".into(), index);
        for marked in [
            "<?php use App\\Features\\Card as C; C::render('wel|');",
            "<?php use App\\Features\\Card as C; function f(C $card){$card->render('wel|');}",
            "<?php use App\\Features\\Card as C; new C('wel|');",
            "<?php namespace App\\Features; class Other extends \\App\\Base\\Component {function template(){return 'wel|';}}",
            "<?php class Other {function templates(){return ['view'=>'wel|'];}}",
            "<?php class Other {function nested(){return ['view'=>['wel|']];}}",
        ] {
            let (source, at) = cursor(marked);
            assert!(
                plugin
                    .completion(&doc("/project/test.php", "php", &source), at, None)
                    .iter()
                    .any(|item| item.label == "welcome"),
                "{marked}"
            );
        }
        for marked in [
            "<?php \\Other\\Card::render('wel|');",
            "<?php class Other {function template(){return 'wel|';}}",
            "<?php class Other {function templates(){return ['wrong'=>'wel|'];}}",
            "<?php namespace App\\Features; class Other extends \\App\\Base\\Component {function template(){return function(){return 'wel|';};}}",
            "<?php namespace App\\Features; class Other extends \\App\\Base\\Component {function template(){return fake('wel|');}}",
        ] {
            let (source, at) = cursor(marked);
            assert!(
                plugin
                    .completion(&doc("/project/test.php", "php", &source), at, None)
                    .is_empty(),
                "{marked}"
            );
        }
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }
    #[test]
    fn public_string_location_maps_support_data_objects_null_and_owned_lines() {
        use crate::tests::{activated, cursor, doc};
        use openidea_plugin::{mock::MockHost, providers::Providers};
        let host = MockHost::install();
        host.add_file("resources/file.txt", "zero\none\ntwo\n");
        host.add_file(
            "data/values.json",
            r#"{"file":"/resources/file.txt:3","empty":null,"bad":9,"escape":"/../foreign"}"#,
        );
        host.add_file("ide.json",r#"{"completions":[{"complete":"staticStrings","options":{"stringsWithLocation":{"inline":"/resources/file.txt:2"},"dataFile":"/data/values.json"},"condition":[{"functionNames":["keys"]}]}]}"#);
        let mut plugin = activated(&host);
        let (source, at) = cursor("<?php keys('f|');");
        let found = plugin.completion(&doc("/project/test.php", "php", &source), at, None);
        assert!(found.iter().any(|item| item.label == "file"));
        assert!(found.iter().any(|item| item.label == "empty"));
        assert!(
            !found
                .iter()
                .any(|item| item.label == "bad" || item.label == "escape")
        );
        for (name, line) in [("file", 2), ("inline", 1)] {
            let (source, at) = cursor(&format!("<?php keys('{name}|');"));
            let found = plugin.definition(&doc("/project/test.php", "php", &source), at);
            assert_eq!(found[0].path, "/project/resources/file.txt");
            assert_eq!(found[0].range.start.line, line);
        }
        let (source, at) = cursor("<?php keys('empty|');");
        assert!(
            plugin
                .definition(&doc("/project/test.php", "php", &source), at)
                .is_empty()
        );
        let issues = &plugin.indexes["/project"].metadata.issues;
        assert!(issues.iter().any(|issue| issue.contains("bad must be")));
        assert!(issues.iter().any(|issue| issue.contains("path for escape")));
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }
    #[test]
    fn class_and_eloquent_field_options_reach_real_provider_inventories() {
        use crate::tests::{activated, cursor, doc};
        use openidea_plugin::{mock::MockHost, providers::Providers};
        let host = MockHost::install();
        for (path, source) in [
            (
                "app/Data/BaseData.php",
                "<?php namespace App\\Data; class BaseData {public string $parent; protected string $token; private string $secret;}",
            ),
            (
                "app/Data/AttributeSet.php",
                "<?php namespace App\\Data; trait AttributeSet {public string $trait_field;}",
            ),
            (
                "app/Data/Profile.php",
                "<?php namespace App\\Data; class Profile extends BaseData {use AttributeSet; public string $name; protected string $slug; private string $private; function __construct(public string $promoted){}}",
            ),
            (
                "app/Repositories/BaseRepository.php",
                "<?php namespace App\\Repositories; class BaseRepository {}",
            ),
            (
                "app/Repositories/UserRepository.php",
                "<?php namespace App\\Repositories; class UserRepository extends BaseRepository {private $model=\\App\\Models\\User::class;}",
            ),
            (
                "app/Models/User.php",
                "<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class User extends Model {protected $casts=['active'=>'boolean']; function posts(){return $this->hasMany(Entry::class);}}",
            ),
            (
                "app/Models/Entry.php",
                "<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class Entry extends Model {protected $table='posts';}",
            ),
            (
                "app/Casts/Slug.php",
                "<?php namespace App\\Casts; class Slug implements \\Illuminate\\Contracts\\Database\\Eloquent\\CastsInboundAttributes {}",
            ),
            (
                "app/Enums/Status.php",
                "<?php namespace App\\Enums; enum Status:string {case Active='active';}",
            ),
        ] {
            host.add_file(path, source);
        }
        host.add_file("ide.json",r#"{"completions":[{"complete":"classFields","options":{"fieldsFilter":{"fetch":"all","modifier":["public","protected"]}},"condition":[{"functionNames":["from"],"classParentFqn":["App\\Data\\BaseData"],"place":"arrayKey"}]},{"complete":"classFields","options":{"fetchClass":"App\\Data\\Profile"},"condition":[{"functionNames":["fields"]}]},{"complete":"eloquentFields","options":{"eloquentFields":{"fetch":"classField","classField":"model","complete":"fieldsAndRelations"}},"condition":[{"methodNames":["sortBy"],"classParentFqn":["App\\Repositories\\BaseRepository"]}]},{"complete":"eloquentFields","options":{"eloquentFields":{"fetch":"classField","complete":"deepRelations"}},"condition":[{"methodNames":["deep"],"classParentFqn":["App\\Repositories\\BaseRepository"]}]},{"complete":"eloquentCasts","condition":[{"functionNames":["cast"]}]}]}"#);
        let mut plugin = activated(&host);
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^12"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        plugin.indexes.insert(
            "/project".into(),
            Index::build(&host.invocation().project().unwrap()),
        );
        let (source, at) = cursor("<?php use App\\Data\\Profile as P; P::from(['|']);");
        let labels: Vec<_> = plugin
            .completion(&doc("/project/test.php", "php", &source), at, None)
            .into_iter()
            .map(|item| item.label)
            .collect();
        for name in ["name", "slug", "promoted", "parent", "token", "trait_field"] {
            assert!(
                labels.iter().any(|label| label == name),
                "{name}: {labels:?}"
            );
        }
        assert!(
            !labels
                .iter()
                .any(|label| label == "secret" || label == "private")
        );
        let (source, at) = cursor("<?php fields('|');");
        let found = plugin.completion(&doc("/project/test.php", "php", &source), at, None);
        let labels: Vec<_> = found.into_iter().map(|item| item.label).collect();
        assert_eq!(labels, vec!["name", "promoted", "trait_field"]);
        let (source, at) = cursor("<?php fields('na|me');");
        assert_eq!(
            plugin.definition(&doc("/project/test.php", "php", &source), at)[0].path,
            "/project/app/Data/Profile.php"
        );
        let (source, at) =
            cursor("<?php use App\\Repositories\\UserRepository as R; R::sortBy('|');");
        let found = plugin.completion(&doc("/project/test.php", "php", &source), at, None);
        for name in ["email", "active", "posts"] {
            assert!(
                found.iter().any(|item| item.label == name),
                "{name}: {found:?}"
            );
        }
        let (source, at) = cursor(
            "<?php use App\\Repositories\\UserRepository as R; R::deep('posts:ti|tle,body');",
        );
        let found = plugin.completion(&doc("/project/test.php", "php", &source), at, None);
        let title = found
            .into_iter()
            .find(|item| item.label == "title")
            .unwrap();
        let start = source.find("title").unwrap();
        assert_eq!(
            title.replace,
            Some(TextRange::new(
                text::position(&source, start),
                text::position(&source, start + 5)
            ))
        );
        let (source, at) = cursor("<?php cast('|');");
        let found = plugin.completion(&doc("/project/test.php", "php", &source), at, None);
        for name in ["boolean", "App\\Casts\\Slug", "App\\Enums\\Status"] {
            assert!(
                found.iter().any(|item| item.label == name),
                "{name}: {found:?}"
            );
        }
        let custom = found
            .iter()
            .find(|item| item.label == "App\\Casts\\Slug")
            .unwrap();
        assert_eq!(custom.insert_text, "App\\\\Casts\\\\Slug");
        let (source, at) = cursor(r"<?php cast('App\\Casts\\Sl|ug');");
        assert_eq!(
            plugin.definition(&doc("/project/test.php", "php", &source), at)[0].path,
            "/project/app/Casts/Slug.php"
        );
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }
    #[test]
    fn malformed_place_class_and_option_filters_never_become_broader_rules() {
        for condition in [
            serde_json::json!({"functionNames":["f"],"keys":false,"place":"arrayValueWithKey"}),
            serde_json::json!({"functionNames":["f"],"classFqn":false}),
            serde_json::json!({"functionNames":["f"],"parameters":[0]}),
            serde_json::json!({"functionNames":["f"],"place":false}),
        ] {
            let config=serde_json::json!({"completions":[{"complete":"viewName","condition":[condition]}]}).to_string();
            let index = custom_views(&config);
            assert!(index.metadata.rules.is_empty(), "{config}");
            assert!(!index.metadata.issues.is_empty());
        }
        for options in [
            serde_json::json!({"fieldsFilter":{"modifier":"public"}}),
            serde_json::json!({"fetchClass":false}),
            serde_json::json!({"fieldsFilter":{"fetch":"unknown"}}),
        ] {
            let config=serde_json::json!({"completions":[{"complete":"classFields","options":options,"condition":[{"functionNames":["f"]}]}]}).to_string();
            let metadata = parse(&config);
            assert!(metadata.rules.is_empty(), "{config}");
            assert!(!metadata.issues.is_empty());
        }
    }
    #[test]
    fn artisan_domains_project_only_captured_metadata_without_read_time_execution() {
        use crate::tests::{activated, cursor, doc};
        use openidea_plugin::{mock::MockHost, providers::Providers};
        let host = MockHost::install();
        host.add_file("artisan", "<?php require 'bootstrap/app.php';");
        host.add_file("ide.json",r#"{"completions":[{"complete":"artisanCommandName","condition":[{"functionNames":["command"],"parameters":[1]}]},{"complete":"artisanCommandParameter","condition":[{"functionNames":["command"],"parameters":[2],"place":"arrayKey"}]}]}"#);
        let mut plugin = activated(&host);
        let project = host.invocation().project().unwrap();
        let identity = crate::artisan_metadata::identity(&project).unwrap();
        let commands=crate::artisan_metadata::parse(r#"{"commands":[{"name":"app:custom","description":"Project command","definition":{"arguments":[{"name":"name","description":"Name"}],"options":[{"name":"force","description":"Force"}]}}]}"#).unwrap();
        let snapshot = serde_json::json!({"schema":2,"identity":identity,"capturedAt":100,"commands":commands});
        host::storage::set(
            &format!(
                "artisan-metadata-v2:{}",
                crate::artisan_metadata::hash("/project")
            ),
            &snapshot.to_string(),
        )
        .unwrap();
        refresh_artisan(
            &project,
            &mut plugin.indexes.get_mut("/project").unwrap().metadata,
        );
        let (source, at) = cursor("<?php command('app:|');");
        assert!(
            plugin
                .completion(&doc("/project/test.php", "php", &source), at, None)
                .iter()
                .any(|item| item.label == "app:custom")
        );
        let (source, at) = cursor("<?php command('app:custom',['|']);");
        let found = plugin.completion(&doc("/project/test.php", "php", &source), at, None);
        assert!(found.iter().any(|item| item.label == "--force"));
        assert!(found.iter().any(|item| item.label == "name"));
        // A callback uses its cached projection; reading new storage or source is maintenance work.
        host.add_file("artisan", "<?php changed");
        assert!(
            plugin
                .completion(&doc("/project/test.php", "php", &source), at, None)
                .iter()
                .any(|item| item.label == "--force")
        );
        refresh_artisan(
            &project,
            &mut plugin.indexes.get_mut("/project").unwrap().metadata,
        );
        assert!(
            plugin
                .completion(&doc("/project/test.php", "php", &source), at, None)
                .is_empty()
        );
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }
    #[test]
    fn response_header_names_follow_real_helper_facade_and_constructor_bindings() {
        use crate::tests::{activated, cursor, doc};
        use openidea_plugin::{mock::MockHost, providers::Providers};
        let host = MockHost::install();
        let mut plugin = activated(&host);
        for marked in [
            "<?php /* 😀 */ response('',200,['Cache-|'=>'no-cache']);",
            "<?php response(headers:['Content-T|'=>'text/plain'],content:'',status:200);",
            "<?php namespace App; response('',200,['Cache-|'=>'no-cache']);",
            "<?php namespace App; use function response as reply; reply('',200,['Cache-|'=>'no-cache']);",
            "<?php use Illuminate\\Support\\Facades\\Response as R; R::make('',200,['Cache-|'=>'no-cache']);",
            "<?php response()->json([],200,['Cache-|'=>'no-cache']);",
            "<?php response()->noContent(204,['Cache-|'=>'no-cache']);",
            "<?php response()->view('welcome',[],200,['Cache-|'=>'no-cache']);",
            "<?php response()->file('/file',['Cache-|'=>'no-cache']);",
            "<?php use Illuminate\\Support\\Facades\\Response as R; R::jsonp('cb',[],200,['Cache-|'=>'no-cache']);",
            "<?php response('body')->withHeaders(['Cache-|'=>'no-cache']);",
            "<?php response('body')->header('Cache-|','no-cache');",
            "<?php new \\Illuminate\\Http\\Response('',200,['Cache-|'=>'no-cache']);",
            "<?php use Symfony\\Component\\HttpFoundation\\Response as R; new R('',200,['Cache-|'=>'no-cache']);",
        ] {
            let (source, at) = cursor(marked);
            let found = plugin.completion(&doc("/project/test.php", "php", &source), at, None);
            assert!(
                found.iter().any(|item| item.label == "Cache-Control"),
                "{marked}: {found:?}"
            );
        }
        for marked in [
            "<?php response('',200,['key'=>'Cache-|']);",
            "<?php response('', ['Cache-|'=>1]);",
            "<?php function response($a,$b,$c){} response('',200,['Cache-|'=>1]);",
            "<?php namespace App; function response($a,$b,$c){} response('',200,['Cache-|'=>1]);",
            "<?php use function Vendor\\response; response('',200,['Cache-|'=>1]);",
            "<?php \\Vendor\\response('',200,['Cache-|'=>1]);",
            "<?php use Vendor\\Response as R; R::make('',200,['Cache-|'=>1]);",
            "<?php class Response{} Response::make('',200,['Cache-|'=>1]);",
            "<?php new \\Vendor\\Response('',200,['Cache-|'=>1]);",
            "<?php response()->json([],['Cache-|'=>1]);",
            "<?php response()->header('Cache-|','no-cache');",
            "<?php response($dynamic)->withHeaders(['Cache-|'=>1]);",
            "<?php use function Vendor\\response; response()->json([],200,['Cache-|'=>1]);",
            "<?php use Illuminate\\Support\\Facades\\Response as R; class R{} R::json([],200,['Cache-|'=>1]);",
            "<?php $x=\"response('',200,['Cache-|'=>1]);\";",
        ] {
            let (source, at) = cursor(marked);
            assert!(
                plugin
                    .completion(&doc("/project/test.php", "php", &source), at, None)
                    .is_empty(),
                "{marked}"
            );
        }
        let (source, at) = cursor("<?php response('',200,['Cache-|']);");
        let found = plugin.completion(&doc("/project/test.php", "php", &source), at, None);
        let item = found
            .iter()
            .find(|item| item.label == "Cache-Control")
            .unwrap();
        assert_eq!(item.insert_text, "Cache-Control' => $0");
        assert!(item.snippet);
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }
    #[test]
    fn declared_vendor_ancestors_and_explicit_class_fields_are_cached_before_providers() {
        use crate::tests::{activated, cursor, doc};
        use openidea_plugin::{mock::MockHost, providers::Providers};
        let host = MockHost::install();
        host.add_file("vendor/composer/installed.json",r#"{"packages":[{"name":"acme/ui","install-path":"../acme/ui","autoload":{"psr-4":{"Acme\\":"src/"}}}]}"#);
        host.add_file("vendor/acme/ui/src/Ui/BaseData.php","<?php namespace Acme\\Ui; class BaseData implements \\Acme\\Contracts\\Renderable {public string $vendor_name;}");
        host.add_file(
            "vendor/acme/ui/src/Contracts/Renderable.php",
            "<?php namespace Acme\\Contracts; interface Renderable {}",
        );
        host.add_file(
            "app/Features/Card.php",
            "<?php namespace App\\Features; class Card extends \\Acme\\Ui\\BaseData {}",
        );
        host.add_file("ide.json",r#"{"completions":[{"complete":"viewName","condition":[{"methodNames":["render"],"classParentFqn":["Acme\\Contracts\\Renderable"]}]},{"complete":"classFields","options":{"fetchClass":"Acme\\Ui\\BaseData"},"condition":[{"functionNames":["fields"]}]}]}"#);
        let mut plugin = activated(&host);
        host.add_file("composer.json",r#"{"require":{"laravel/framework":"^12","acme/ui":"1"},"autoload":{"psr-4":{"App\\":"app/"}}}"#);
        plugin.indexes.insert(
            "/project".into(),
            Index::build(&host.invocation().project().unwrap()),
        );
        for (marked, expected) in [
            ("<?php \\App\\Features\\Card::render('wel|');", "welcome"),
            ("<?php fields('vendor_|');", "vendor_name"),
        ] {
            let (source, at) = cursor(marked);
            assert!(
                plugin
                    .completion(&doc("/project/test.php", "php", &source), at, None)
                    .iter()
                    .any(|item| item.label == expected),
                "{marked}"
            );
        }
        assert!(
            plugin.indexes["/project"]
                .metadata
                .affects("vendor/acme/ui/src/Ui/BaseData.php")
        );
        host.add_project("/project/vendor/acme/ui");
        plugin.indexes.insert(
            "/project".into(),
            Index::build(&host.invocation().project().unwrap()),
        );
        let (source, at) = cursor("<?php fields('vendor_|');");
        assert!(
            plugin
                .completion(&doc("/project/test.php", "php", &source), at, None)
                .is_empty()
        );
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }
    #[test]
    fn metadata_typed_receiver_overlay_preserves_proven_query_model_and_object_return_type() {
        use crate::tests::{activated, cursor, doc};
        use openidea_plugin::{mock::MockHost, providers::Providers};
        let host = MockHost::install();
        host.add_file("app/Models/User.php","<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class User extends Model {}");
        host.add_file(
            "app/Data/Profile.php",
            "<?php namespace App\\Data; class Profile {public string $name;}",
        );
        host.add_file("app/Repositories/ProfileRepository.php","<?php namespace App\\Repositories; class ProfileRepository {function data(): \\App\\Data\\Profile {return new \\App\\Data\\Profile;}}");
        host.add_file("ide.json",r#"{"completions":[{"complete":"eloquentFields","condition":[{"methodNames":["custom"],"classParentFqn":["Illuminate\\Database\\Eloquent\\Builder"]}]},{"complete":"classFields","condition":[{"methodNames":["pick"],"classFqn":["App\\Data\\Profile"]}]}]}"#);
        let mut plugin = activated(&host);
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^12"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        plugin.indexes.insert(
            "/project".into(),
            Index::build(&host.invocation().project().unwrap()),
        );
        for (marked, expected) in [
            (
                "<?php \\App\\Models\\User::query()->custom('em|');",
                "email",
            ),
            (
                "<?php function f(\\App\\Repositories\\ProfileRepository $repo){$repo->data()->pick('na|');}",
                "name",
            ),
        ] {
            let (source, at) = cursor(marked);
            let found = plugin.completion(&doc("/project/test.php", "php", &source), at, None);
            assert!(
                found.iter().any(|item| item.label == expected),
                "{marked}: {found:?}"
            );
        }
        for marked in [
            "<?php unknown()->custom('em|');",
            "<?php function f($repo){$repo->data()->pick('na|');}",
        ] {
            let (source, at) = cursor(marked);
            assert!(
                plugin
                    .completion(&doc("/project/test.php", "php", &source), at, None)
                    .is_empty(),
                "{marked}"
            );
        }
    }
    #[test]
    fn escaped_php_literals_preserve_decoded_segment_boundaries_and_navigation() {
        let mut index = Index::default();
        index.metadata = parse(
            r#"{"completions":[{"complete":"validationRules","condition":[{"functionNames":["rules"]}]}]}"#,
        );
        index.rules.add(
            "required",
            None,
            "/project/rules.php",
            Position::new(0, 0),
            8,
        );
        let source = r#"<?php /* 😀 */ rules("nullable\x7creq\x75ired:3\u{7c}email");"#;
        let start = source.find("req").unwrap();
        let at = start + 2;
        let out = completion(&index, source, at);
        assert_eq!(out.len(), 1);
        let end = source.find(":3").unwrap();
        assert_eq!(
            out[0].replace,
            Some(TextRange::new(
                text::position(source, start),
                text::position(source, end)
            ))
        );
        assert_eq!(definition(&index, source, at)[0].path, "/project/rules.php");
        let regex = r#"<?php rules("required\x7cregex:/foo|ba/");"#;
        assert!(completion(&index, regex, regex.find("ba").unwrap() + 1).is_empty());
        assert_eq!(
            crate::eloquent_members::metadata_inference::decode_literal(
                r"\xC3\xA9\u{1F600}\101",
                b'"'
            ),
            Some("é😀A".into())
        );
        assert_eq!(
            crate::eloquent_members::metadata_inference::decode_literal(r"\q", b'"'),
            Some(r"\q".into())
        );
        assert!(
            crate::eloquent_members::metadata_inference::decode_literal(r"\u{110000}", b'"')
                .is_none()
        );
        assert!(
            crate::eloquent_members::metadata_inference::decode_literal(r"$variable", b'"')
                .is_none()
        );
    }
    #[test]
    fn cyclic_and_colliding_type_bindings_do_not_satisfy_parent_conditions() {
        let mut index = custom_views(
            r#"{"completions":[{"complete":"viewName","condition":[{"methodNames":["pick"],"classParentFqn":["Pkg\\Base"]}]}]}"#,
        );
        crate::eloquent_members::metadata_inference::cache_class(
            &mut index.metadata,
            "<?php namespace Pkg; class A extends Base {}",
            "/project/A.php",
        );
        crate::eloquent_members::metadata_inference::cache_class(
            &mut index.metadata,
            "<?php namespace Pkg; class Base extends A {}",
            "/project/Base.php",
        );
        assert!(!completed(&index, "<?php \\Pkg\\A::pick('home');"));
        let source = "<?php use Pkg\\A as C; class C{} C::pick('home');";
        assert!(!completed(&index, source));
    }
    #[test]
    fn php85_pipe_first_class_callables_use_implicit_first_argument_and_native_inventories() {
        use crate::tests::{activated, cursor, doc};
        use openidea_plugin::{mock::MockHost, providers::Providers};
        let host = MockHost::install();
        host.add_file("ide.json",r#"{"completions":[{"complete":"viewName","condition":[{"functionNames":["choose"],"parameters":[1]},{"methodNames":["pick"],"classFqn":["Pkg\\Picker"],"parameters":[1]},{"functionNames":["chooseList"],"place":"arrayValueWithKey","keys":["route"],"parameters":[1]}]}]}"#);
        let mut plugin = activated(&host);
        for (marked, expected) in [
            ("<?php /* 😀 */ 'app.na|' |> config(...);", "app.name"),
            (
                "<?php use function config as settings; 'app.na|' |> settings(...);",
                "app.name",
            ),
            ("<?php 'wel|' |> \\view(...);", "welcome"),
            ("<?php 'posts.sh|' |> route(...);", "posts.show"),
            (
                "<?php use Illuminate\\Support\\Facades\\Config as C; 'app.na|' |> C::get(...);",
                "app.name",
            ),
            ("<?php 'wel|' |> choose(...);", "welcome"),
            (
                "<?php function f(\\Pkg\\Picker $picker){'wel|' |> $picker->pick(...);}",
                "welcome",
            ),
            (
                "<?php use Pkg\\Picker as P; 'wel|' |> P::pick(...);",
                "welcome",
            ),
            ("<?php ['route'=>'wel|'] |> chooseList(...);", "welcome"),
            ("<?php ['app.na|'=>1] |> config(...);", "app.name"),
        ] {
            let (source, at) = cursor(marked);
            let found = plugin.completion(&doc("/project/test.php", "php", &source), at, None);
            assert!(
                found.iter().any(|item| item.label == expected),
                "{marked}: {found:?}"
            );
        }
        let (source, at) = cursor("<?php /* 😀 */ 'app.na|me' |> config(...);");
        let found = plugin.completion(&doc("/project/test.php", "php", &source), at, None);
        let item = found.iter().find(|item| item.label == "app.name").unwrap();
        let start = source.find("app.name").unwrap();
        assert_eq!(
            item.replace,
            Some(TextRange::new(
                text::position(&source, start),
                text::position(&source, start + 8)
            ))
        );
        assert_eq!(
            plugin.definition(&doc("/project/test.php", "php", &source), at)[0].path,
            "/project/config/app.php"
        );
        for marked in [
            "<?php 'wel|' |> $dynamic(...);",
            "<?php 'wel|' |> fn($value)=>choose($value);",
            "<?php 'wel|' |> choose($value);",
            "<?php 'prefix'.'wel|' |> choose(...);",
            "<?php use function Vendor\\config; 'app.na|' |> config(...);",
            "<?php namespace App; function config($key){} 'app.na|' |> config(...);",
            "<?php use Vendor\\Config as C; 'app.na|' |> C::get(...);",
            "<?php function f($unknown){'wel|' |> $unknown->pick(...);}",
        ] {
            let (source, at) = cursor(marked);
            assert!(
                plugin
                    .completion(&doc("/project/test.php", "php", &source), at, None)
                    .is_empty(),
                "{marked}"
            );
        }
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^12"},"config":{"platform":{"php":"8.4.0"}}}"#,
        );
        plugin.indexes.insert(
            "/project".into(),
            Index::build(&host.invocation().project().unwrap()),
        );
        assert_eq!(
            plugin.indexes["/project"].metadata.php_version,
            Some((8, 4, 0))
        );
        let (source, at) = cursor("<?php 'wel|' |> choose(...);");
        assert!(
            plugin
                .completion(&doc("/project/test.php", "php", &source), at, None)
                .is_empty()
        );
        let (source, at) = cursor("<?php 'app.na|' |> config(...);");
        assert!(
            plugin
                .completion(&doc("/project/test.php", "php", &source), at, None)
                .is_empty()
        );
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }
    #[test]
    fn unicode_php_type_variable_and_field_names_keep_exact_utf16_locations() {
        use crate::tests::{activated, cursor, doc};
        use openidea_plugin::{mock::MockHost, providers::Providers};
        let host = MockHost::install();
        let declaration = "<?php namespace App\\Data; class Élève {public string $prénom😀; public function choisir($champ){}}";
        host.add_file("app/Data/Élève.php", declaration);
        host.add_file("ide.json",r#"{"completions":[{"complete":"classFields","condition":[{"methodNames":["choisir"],"classFqn":["App\\Data\\Élève"]}]}]}"#);
        let mut plugin = activated(&host);
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^12"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        plugin.indexes.insert(
            "/project".into(),
            Index::build(&host.invocation().project().unwrap()),
        );
        let (source, at) = cursor(
            "<?php use App\\Data\\Élève as E; function f(E $élève){$élève->choisir('pré|');}",
        );
        let found = plugin.completion(&doc("/project/test.php", "php", &source), at, None);
        assert!(
            found.iter().any(|item| item.label == "prénom😀"),
            "{found:?}"
        );
        let (source, at) = cursor(
            "<?php use App\\Data\\Élève as E; function f(E $élève){$élève->choisir('prénom😀|');}",
        );
        let found = plugin.definition(&doc("/project/test.php", "php", &source), at);
        let at = declaration.find("prénom😀").unwrap();
        assert_eq!(
            found[0].range,
            TextRange::new(
                text::position(declaration, at),
                text::position(declaration, at + "prénom😀".len())
            )
        );
    }
}
