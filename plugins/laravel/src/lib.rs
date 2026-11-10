//! Laravel: completion, navigation, problems, gutter markers and a tool
//! window for routes, views, config, translations, env keys, middleware,
//! controllers and assets in PHP and Blade files.
//!
//! The features the free Laravel plugin for IntelliJ (Daniel Espendiller,
//! MIT) has are ported from its public source; the rest follows Laravel's own
//! documentation. See README.md for what was and wasn't ported.

mod actions;
mod about;
mod artisan_metadata;
mod artisan_arguments;
mod artisan_workspace;
mod artisan_execution;
mod artisan_output;
mod artisan_docker;
mod artisan_runner;
mod blade;
mod blade_php;
mod bulk;
mod class_actions;
mod components;
mod config_arrays;
mod controllers;
mod console_editor;
mod directives;
mod eloquent_checks;
mod eloquent_members;
mod function_checks;
mod generate;
mod generator_plan;
mod generator_presets;
mod generator_preset_actions;
mod external_services;
mod generator_workspace;
mod existing_model_fields;
mod vite;
mod hover;
mod view_creation;
mod view_extract;
mod request_creation;
mod request_promotion;
mod source_matches;
mod semantic_refs;
mod strict_json;
mod index;
mod indexer;
mod js;
mod lang_actions;
mod live;
mod migrations;
mod model_overview;
mod model_actions;
mod model_from_schema;
mod vendor_overrides;
mod metadata;
mod managed_processes;
mod package_facts;
mod project_wizard;
mod php;
mod refs;
mod routes;
mod route_runtime;
mod route_search;
mod view_rename;
mod route_editor;
mod runtime_inspection;
mod mcp;
mod unused;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use openidea_plugin::providers::{CodeAction, CompletionItem, CompletionKind, Document, LineMarker, Providers};
use openidea_plugin::views::{Body, Button, TreeItem, View, ViewEvent, Viewport, Views};
use openidea_plugin::{Context, Diagnostic, Event, EventKind, Location, Position, Range, Severity, TextEdit};
use openidea_plugin::{Error, Invocation, LogLevel, NotificationLevel, Plugin, Project, host, text};
use ui_kit::routes::{Handler, Route, RouteEvent, RouteList, copy_as_curl, create_http_file_for_project};

pub use index::Index;
use index::{AddName, Entry, Lines, Names};
use refs::{Kind, Reference};

const VIEW: &str = "laravel";
const PLUGIN_ID: &str = "aritti.laravel";
const DEFAULT_BASE_URL: &str = "http://localhost:8000";
const SOURCE: &str = "Laravel";
const CONTROLLERS_DIRECTORY: &str = "app/Http/Controllers/";

/// Whether a saved file (project-relative) feeds the index.
fn affects_index(relative: &str) -> bool {
    if matches!(relative, "ide-blade.json" | ".idea/ide-blade.json" | "ide.json" | ".idea/ide.json")
        || relative.starts_with(".openidea/laravel-")
        || relative.starts_with("database/schema/")
        || relative.starts_with("public/build/")
        || relative.starts_with("vite.config.") {
        return true;
    }

    let directory = [
        "routes/",
        "config/",
        "lang/",
        "resources/lang/",
        "resources/views/",
        "app/Http/Controllers/",
        "app/Mail/",
        "app/View/Components/",
        "app/Livewire/",
        "app/Http/Livewire/",
        "app/Policies/",
        "app/Models/",
        "database/migrations/",
        "app/Providers/",
        "database/factories/",
        "resources/js/Pages/",
        "resources/js/pages/",
    ];
    (relative.starts_with("app/") && relative.ends_with(".php"))
        || (relative.starts_with("database/seeders/") && relative.ends_with(".php"))
        || directory.iter().any(|d| relative.starts_with(d))
        || matches!(
            relative,
            "composer.json" | "composer.lock" | ".env" | ".env.example" | "bootstrap/app.php" | "app/Http/Kernel.php" | "app/Http/routes.php"
        )
}

fn under(path: &str, root: &str) -> bool {
    path.strip_prefix(root).is_some_and(|rest| rest.starts_with('/'))
}

fn project_for(path: &str) -> Option<Project> {
    host::projects()
        .ok()?
        .into_iter()
        .filter(|p| p.root().is_ok_and(|root| under(path, &root)))
        .max_by_key(|p| p.root().map_or(0, |r| r.len()))
}

/// The language the plugin reads a file as: Blade files are PHP files to the
/// IDE when the Blade plugin isn't installed.
fn language_of(path: &str, language: &str) -> Option<&'static str> {
    if path.ends_with(".blade.php") || language == "blade" {
        return Some("blade");
    }
    path.ends_with(".php").then_some("php")
}

fn range_of(text: &str, start: usize, end: usize) -> Range {
    Range::new(text::position(text, start), text::position(text, end))
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn location(entry: &Entry) -> Option<Location> {
    if entry.path.is_empty() {
        return None;
    }

    let end = Position::new(entry.at.line, entry.at.character + entry.width);
    Some(Location { path: entry.path.clone(), range: Range::new(entry.at, end) })
}

/// Current bounded source facts, not saved offsets projected onto a dirty file.
/// Retain current innermost ownership even when a cached view path still exists.
fn current_blade_view<'a>(index: &'a Index, project: &Project, view: &str) -> Option<(&'a Entry, String, blade::Facts)> {
    let entry = index.views.get(view)?;
    let root = project.root().ok()?;
    if host::project_of(&entry.path).and_then(|owner| owner.root().ok()).as_deref() != Some(root.as_str()) { return None; }
    let metadata = project.stat(&entry.path).ok()?;
    if metadata.kind != openidea_plugin::FileKind::File || metadata.size > 128 * 1024 { return None; }
    let source = project.read_file(&entry.path).ok().filter(|source| source.len() <= 128 * 1024)?;
    let facts = blade::facts(&source);
    Some((entry, source, facts))
}

fn owns_current_path(project: &Project, path: &str) -> bool {
    let Ok(root) = project.root() else { return false; };
    host::project_of(path).and_then(|owner| owner.root().ok()).as_deref() == Some(root.as_str())
}
fn scoped_location(project: &Project, entry: &Entry) -> Option<Location> {
    if !owns_current_path(project, &entry.path) { return None; }
    location(entry)
}

/// Sections, yields and stacks that the layouts above a view define.
fn layout_names(index: &Index, project: &Project, doc: &Document, kind: Kind) -> Names {
    let mut names = Names::new();
    let facts=blade::facts(&doc.text);
    let Some(parent) = index.layout_parent(&facts) else {
        return names;
    };

    for view in std::iter::once(parent).chain(index.parents(parent)) {
        let Some((entry, text, facts)) = current_blade_view(index, project, view) else { continue; };
        let lines = Lines::new(&text);

        let found: Vec<&blade::Named> = match kind {
            Kind::Stack => facts.stacks.iter().collect(),
            _ => facts.sections.iter().chain(&facts.yields).collect(),
        };
        for (name, offset) in found {
            names.add(name, Some(view.to_owned()), &entry.path, lines.position(*offset), lines.width(*offset, name.len()));
        }
    }

    names
}

/// `Class@method` of every controller.
fn actions(index: &Index) -> Names {
    let mut names = Names::new();
    for (class, methods) in &index.methods {
        for (method, entry) in methods {
            if method == "__invoke" {names.insert(class.clone(),entry.clone());}
            names.insert(format!("{class}@{method}"), entry.clone());
        }
    }
    names
}

/// Names a reference can name, for completion and navigation.
fn candidates<'a>(index: &'a Index, doc: &Document, reference: &Reference) -> Cow<'a, Names> {
    match reference.kind {
        Kind::Route => Cow::Borrowed(&index.routes),
        Kind::View => Cow::Borrowed(&index.views),
        Kind::Config => Cow::Borrowed(&index.config),
        Kind::Translation => Cow::Borrowed(&index.translations),
        Kind::Env => Cow::Borrowed(&index.env),
        Kind::Asset => Cow::Borrowed(&index.assets),
        Kind::Middleware => Cow::Borrowed(&index.middleware),
        Kind::Controller => Cow::Borrowed(&index.controllers),
        Kind::Component => Cow::Borrowed(&index.components),
        Kind::Livewire => Cow::Borrowed(&index.livewire),
        Kind::Vite => Cow::Borrowed(&index.vite),
        Kind::Inertia => Cow::Borrowed(&index.inertia_pages),
        Kind::Rule => Cow::Borrowed(&index.rules),
        Kind::Ability => Cow::Borrowed(&index.abilities),
        Kind::Column => {
            let table = match reference.class.as_deref() {
                Some(table) if table.starts_with("table:") => Some(table["table:".len()..].to_owned()),
                Some(spec) => resolve_model(index, spec, &doc.path)
                    .map(|class| index.models.get(&class).cloned().unwrap_or_else(|| index::table_name(&class))),
                None => index.model_files.get(&doc.path).cloned(),
            };
            Cow::Owned(table.and_then(|t| index.columns.get(&t)).cloned().unwrap_or_default())
        }
        Kind::Binding => Cow::Borrowed(&index.bindings),
        Kind::Action => Cow::Owned(actions(index)),
        Kind::Method => {
            let methods = reference.class.as_deref().and_then(|class| index.controller_methods(class));
            Cow::Owned(methods.cloned().unwrap_or_default())
        }
        Kind::Section | Kind::Stack => match project_for(&doc.path) {
            Some(project) => Cow::Owned(layout_names(index, &project, doc, reference.kind)),
            None => Cow::Owned(Names::new()),
        },
    }
}

/// The model a chain such as `Post>comments>published` ends on: `Post` followed by relation or scope calls.
/// An empty start is the model of the current file. `None` when a call is neither.
fn resolve_model(index: &Index, spec: &str, path: &str) -> Option<String> {
    let mut parts = spec.split('>');
    let first = parts.next()?;
    let mut model = if first.is_empty() { file_name(path).strip_suffix(".php")?.to_owned() } else { first.to_owned() };

    for hop in parts {
        if index.scopes.get(&model).is_some_and(|scopes| scopes.contains(hop)) {
            continue;
        }

        model = index.relations.get(&model)?.get(hop)?.clone();
    }

    Some(model)
}

fn completion_kind(kind: Kind) -> CompletionKind {
    match kind {
        Kind::Route => CompletionKind::Reference,
        Kind::View | Kind::Asset => CompletionKind::File,
        Kind::Config => CompletionKind::Property,
        Kind::Translation => CompletionKind::Text,
        Kind::Env | Kind::Section | Kind::Stack => CompletionKind::Constant,
        Kind::Middleware => CompletionKind::Module,
        Kind::Controller | Kind::Component | Kind::Livewire => CompletionKind::Class,
        Kind::Vite => CompletionKind::File,
        Kind::Inertia => CompletionKind::File,
        Kind::Column => CompletionKind::Field,
        Kind::Rule => CompletionKind::Keyword,
        Kind::Ability => CompletionKind::Value,
        Kind::Action | Kind::Method => CompletionKind::Method,
        Kind::Binding => CompletionKind::Class,
    }
}

/// A name that is built at run time (`'posts.' . $x`), a wildcard (`'admin.*'`)
/// or still being typed (`'app.'`): not checked.
fn is_dynamic(name: &str) -> bool {
    name.is_empty() || name.contains(['{', '$', '*']) || name.ends_with('.')
}

fn problem(text: &str, range: std::ops::Range<usize>, code: &str, severity: Severity, message: String) -> Diagnostic {
    Diagnostic {
        range: range_of(text, range.start, range.end),
        severity,
        message,
        code: Some(code.into()),
        source: Some(SOURCE.into()),
        unnecessary: false,
        deprecated: false,
        related: Vec::new(),
    }
}

/// Problems of a file: views, config keys and translation keys that don't
/// exist and, when asked for, routes that don't.
pub fn problems(index: &Index, language: &str, text: &str, check_routes: bool) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    let sure = !index.partial;

    for reference in refs::references(language, text).into_iter().filter(|r| r.closed) {
        let name = &text[reference.start..reference.end];
        if is_dynamic(name) {
            continue;
        }

        match reference.kind {
            Kind::View if !name.contains("::") && !index.views.is_empty() && !index.views.contains_key(name) && !refs::optional_view_fallback(language,text,reference.start) => {
                found.push(problem(
                    text,
                    reference.start..reference.end,
                    "missing-view",
                    Severity::Warning,
                    format!("View \"{name}\" does not exist."),
                ));
            }
            Kind::Config if missing_config(index, name) => {
                found.push(problem(
                    text,
                    reference.start..reference.end,
                    "missing-config",
                    Severity::Info,
                    format!("Config key \"{name}\" is not defined."),
                ));
            }
            Kind::Translation if missing_translation(index, name) => {
                found.push(problem(
                    text,
                    reference.start..reference.end,
                    "missing-translation",
                    Severity::Info,
                    format!("Translation key \"{name}\" is missing."),
                ));
            }
            Kind::Route if check_routes && !index.routes.contains_key(name) => {
                found.push(problem(
                    text,
                    reference.start..reference.end,
                    "missing-route",
                    Severity::Warning,
                    format!("Route \"{name}\" is not defined."),
                ));
            }
            Kind::Inertia if sure && unknown_inertia_page(index, name) && enabled("reportUnknownInertiaPages") => {
                found.push(problem(
                    text,
                    reference.start..reference.end,
                    "laravel.unknown-inertia-page",
                    Severity::Warning,
                    format!("Inertia page \"{name}\" does not exist."),
                ));
            }
            Kind::Livewire if sure && unknown_livewire(index, name) && enabled("reportUnknownLivewireDirectives") => {
                found.push(problem(
                    text,
                    reference.start..reference.end,
                    "laravel.unknown-livewire-directive",
                    Severity::Warning,
                    format!("Livewire component \"{name}\" does not exist."),
                ));
            }
            _ => {}
        }
    }

    if language == "blade" && sure {
        for reference in refs::tag_references(text) {
            let name = &text[reference.start..reference.end];
            let range = reference.start..reference.end;

            match reference.kind {
                Kind::Component if unknown_component(index, name) && enabled("reportUnknownComponents") => {
                    found.push(problem(
                        text,
                        range,
                        "laravel.unknown-component",
                        Severity::Warning,
                        format!("Blade component \"{name}\" does not exist."),
                    ));
                }
                Kind::Livewire if unknown_livewire(index, name) && enabled("reportUnknownLivewire") => {
                    found.push(problem(
                        text,
                        range,
                        "laravel.unknown-livewire",
                        Severity::Warning,
                        format!("Livewire component \"{name}\" does not exist."),
                    ));
                }
                _ => {}
            }
        }
    }

    if language == "php" && sure && enabled("reportUnknownFactories") {
        for range in refs::factory_classes(text) {
            let class = text[range.clone()].rsplit('\\').next().unwrap_or_default();
            if unknown_factory(index, class) {
                found.push(problem(text, range, "laravel.unknown-factory", Severity::Warning, format!("Model {class} has no factory.")));
            }
        }
    }

    found.sort_by_key(|d| (d.range.start.line, d.range.start.character));
    found
}

/// `db:seed` runs inside `Model::unguarded()`, so mass assignment there is not discarded.
const UNGUARDED_DIRECTORIES: &[&str] = &["/database/seeders/", "/database/seeds/", "/database/factories/"];

/// Mass assignment, prunable and `orderBy` problems of a PHP file.
fn eloquent_problems(project: &Project, path: &str, language: &str, text: &str) -> Vec<Diagnostic> {
    if language != "php" || !enabled("checkEloquent") {
        return Vec::new();
    }

    let Ok(root) = project.root() else {
        return Vec::new();
    };
    let read = |relative: &str| project.read_file(&format!("{root}/{relative}")).ok();

    let unguarded_run = UNGUARDED_DIRECTORIES.iter().any(|directory| path.contains(directory));

    eloquent_checks::findings(text, &read)
        .into_iter()
        .filter(|finding| !(unguarded_run && finding.code == "guarded-attribute-assignment"))
        .map(|finding| problem(text, finding.start..finding.end, finding.code, finding.severity, finding.message))
        .collect()
}

/// `env()` calls outside the configuration files.
fn function_problems(project: &Project, path: &str, language: &str, text: &str) -> Vec<Diagnostic> {
    if !enabled("checkFunctions") {
        return Vec::new();
    }

    let Ok(root) = project.root() else {
        return Vec::new();
    };
    let relative = path.strip_prefix(&format!("{root}/")).unwrap_or(path);

    function_checks::env_outside_config(relative, language, text)
        .into_iter()
        .map(|finding| problem(text, finding.start..finding.end, finding.code, finding.severity, finding.message))
        .collect()
}

/// `wire:click="method"` naming a method the view's own component class does not have.
fn livewire_method_problems(project: &Project, index: &Index, path: &str, language: &str, text: &str) -> Vec<Diagnostic> {
    if language != "blade" || index.partial || !enabled("reportUnknownLivewireMethods") {
        return Vec::new();
    }

    let Some(component) = index.view_of(path).and_then(|view| view.strip_prefix("livewire.")) else {
        return Vec::new();
    };
    let Some(entry) = index.livewire.get(component).filter(|entry| !entry.path.is_empty()) else {
        return Vec::new();
    };
    let Some(methods) = project.read_file(&entry.path).ok().and_then(|class| blade::livewire_methods(&class)) else {
        return Vec::new();
    };

    blade::wire_calls(text)
        .into_iter()
        .filter(|call| !methods.contains(&call.name))
        .map(|call| {
            let message = format!("Component class has no method \"{}\".", call.name);
            problem(text, call.start..call.end, "laravel.unknown-livewire-method", Severity::Warning, message)
        })
        .collect()
}

fn controllers_namespace() -> String {
    host::setting_string("controllersNamespace")
        .filter(|namespace| !namespace.is_empty())
        .unwrap_or_else(|| "App\\Http\\Controllers".into())
}

/// Controllers and methods that `routes/*.php` name and the project lacks.
fn route_controller_problems(project: &Project, index: &Index, path: &str, language: &str, text: &str) -> Vec<Diagnostic> {
    let Ok(root) = project.root() else {
        return Vec::new();
    };
    let inside_routes = path.strip_prefix(&format!("{root}/")).is_some_and(|relative| relative.starts_with("routes/"));
    if language != "php" || !inside_routes || index.partial || !enabled("reportUnknownRouteControllers") {
        return Vec::new();
    }

    let masked = php::mask_comments(text);
    if masked.contains("::controller(") || masked.contains("->controller(") {
        return Vec::new();
    }

    let namespace = controllers_namespace();
    let imports = routes::imports(&masked);
    let mut declared: BTreeMap<String, Option<Vec<String>>> = BTreeMap::new();
    let mut missing: BTreeMap<usize, (std::ops::Range<usize>, String, Vec<String>)> = BTreeMap::new();
    let mut found = Vec::new();

    for def in routes::parse(&masked) {
        let Some((class, method)) = &def.controller else {
            continue;
        };
        if method == "__invoke" {
            continue;
        }

        let candidates: Vec<&String> = index.methods.keys().filter(|full| full.rsplit('\\').next() == Some(class.as_str())).collect();
        let range = def.offset..def.offset + def.width;

        match candidates.as_slice() {
            [] => {
                let imported_from_project = imports.iter().any(|(name, from)| name == class && from.starts_with(&namespace));
                if imported_from_project && !index.partial {
                    found.push(problem(
                        text,
                        range,
                        "laravel.unknown-route-controller",
                        Severity::Warning,
                        format!("Controller \"{class}\" does not exist."),
                    ));
                }
            }
            [full] => {
                let methods = declared.entry((*full).clone()).or_insert_with(|| {
                    let file = &index.controllers.get(*full)?.path;
                    controllers::declared_methods(&php::mask_comments(&project.read_file(file).ok()?))
                });
                if methods.as_ref().is_some_and(|methods| !methods.contains(method)) {
                    missing.entry(def.offset).or_insert_with(|| (range, class.clone(), Vec::new())).2.push(method.clone());
                }
            }
            _ => {}
        }
    }

    for (range, class, methods) in missing.into_values() {
        match methods.as_slice() {
            [_, _, ..] if !enabled("reportMissingResourceMethods") => {}
            [method] => found.push(problem(
                text,
                range,
                "laravel.unknown-route-method",
                Severity::Warning,
                format!("{class} has no method \"{method}\"."),
            )),
            _ => {
                let message = format!("{class} lacks the resource methods {}.", methods.join(", "));
                found.push(problem(text, range, "laravel.missing-resource-methods", Severity::Warning, message));
            }
        }
    }
    found.sort_by_key(|diagnostic| diagnostic.range.start.line);

    found
}

fn enabled(setting: &str) -> bool {
    host::setting_bool(setting).unwrap_or(true)
}

fn unknown_component(index: &Index, name: &str) -> bool {
    !index.open_components && !name.contains("::") && !matches!(name, "slot" | "dynamic-component") && !index.components.contains_key(name)
}

fn unknown_livewire(index: &Index, name: &str) -> bool {
    !index.open_livewire
        && !name.contains("::")
        && !matches!(name, "is" | "styles" | "scripts" | "dynamic-component")
        && !index.livewire.contains_key(name)
        && !index.views.contains_key(&format!("livewire.{name}"))
}

fn unknown_inertia_page(index: &Index, name: &str) -> bool {
    !index.inertia_pages.is_empty() && !name.contains("::") && !name.ends_with('/') && !index.inertia_pages.contains_key(name)
}

fn unknown_factory(index: &Index, class: &str) -> bool {
    !matches!(class, "static" | "self" | "parent")
        && index.models.contains_key(class)
        && !index.factories.contains(&format!("{class}Factory"))
        && !index.factory_overrides.contains(class)
}

/// A key below an array (or config file) of the project that doesn't have it.
fn missing_config(index: &Index, name: &str) -> bool {
    if index.config.contains_key(name) {
        return false;
    }

    let Some((parent, _)) = name.rsplit_once('.') else {
        return false;
    };
    index.config.get(parent).is_some_and(|entry| !index.config_incomplete.contains(&entry.path) && matches!(entry.detail.as_deref(), Some("array" | "config file")))
}

/// `file.key` where the project has a lang file for `file` but not the key.
fn missing_translation(index: &Index, name: &str) -> bool {
    if name.contains("::") || index.translations.contains_key(name) {
        return false;
    }

    let Some((file, _)) = name.split_once('.') else {
        return false;
    };
    index.translation_files.contains_key(file)
}

#[derive(Default)]
pub struct Main {
    artisan_terminals: generate::Terminals,
    view_extractions:view_extract::State,
    request_creations:request_creation::State,
    request_promotions:request_promotion::State,
    /// By project root.
    indexes: BTreeMap<String, Index>,
    /// Files with published problems, checked again when the index changes.
    checked: BTreeSet<String>,
    /// Project root and time `migrate-fresh` was first asked for.
    fresh_asked: Option<(String, Instant)>,
    route_list: Option<RouteList>,
    runtime_routes: BTreeMap<String, route_runtime::Snapshot>,
    artisan_workspace: artisan_workspace::State,
    generator_workspace: generator_workspace::State,
    project_wizard: project_wizard::State,
    managed_processes: managed_processes::State,
    external_services: external_services::State,
    external_details: BTreeMap<String, String>,
    model_tree_expanded: BTreeMap<String, BTreeMap<String, bool>>,
    runtime_inspection: runtime_inspection::State,
}

fn refresh_laravel() {
    host::refresh_view(VIEW);
    let _ = host::post_view_message(VIEW, r#"{"channel":"ui-kit-refresh"}"#);
}

impl Main {
    fn index_project(&mut self, project: &Project) {
        let Ok(root) = project.root() else { return };
        self.runtime_inspection.invalidate(&root);

        if !index::is_laravel(project) {
            if self.indexes.remove(&root).is_some() { refresh_laravel(); }
            return;
        }

        let index = Index::build(project);
        if index.partial {
            host::log(LogLevel::Warn, &format!("index of {root} is partial: out of time"));
        }
        self.indexes.insert(root.clone(), index);
        let _ = host::set_context("indexed", true);

        let checked: Vec<String> = self.checked.iter().filter(|path| under(path, &root)).cloned().collect();
        for path in checked {
            self.check(project, &path);
        }
        refresh_laravel();
    }

    fn index_all(&mut self) {
        for project in host::projects().unwrap_or_default() {
            self.index_project(&project);
        }
    }

    fn index_for(&self, path: &str) -> Option<&Index> {
        // Cached ancestry is not current ownership. A newly opened nested app
        // may have no cache yet; a closed/denied scope must not expose its old
        // inventory. Unknown ownership withholds results, never deletes caches.
        let root = project_for(path)?.root().ok()?;
        self.indexes.get(&root)
    }

    /// Publishes the problems of a file (none when they are switched off).
    fn check(&mut self, project: &Project, path: &str) {
        let Some(language) = language_of(path, "") else {
            return;
        };
        let Some(index) = self.index_for(path) else {
            return;
        };

        let enabled = host::setting_bool("diagnostics").unwrap_or(true);
        let found = match project.read_file(path) {
            Ok(text) if enabled && !index.partial => {
                // Cheap cached-inventory checks belong exclusively to DiagnosticsProvider.
                let mut found = Vec::new();
                found.extend(eloquent_problems(project, path, language, &text));
                found.extend(function_problems(project, path, language, &text));
                found.extend(livewire_method_problems(project, index, path, language, &text));
                found.extend(route_controller_problems(project, index, path, language, &text));
                found.sort_by_key(|d| (d.range.start.line, d.range.start.character));
                found
            }
            _ => Vec::new(),
        };

        if host::publish_diagnostics(path, &found).is_ok() {
            self.checked.insert(path.to_owned());
        }
    }

    fn artisan(&mut self, ctx: &Invocation, command: &str) -> Result<Option<String>, Error> {
        let project = ctx.project().ok_or("No project is open.")?;
        let task = host::begin_task(&format!("artisan {command}"), false);
        let args = ["artisan".to_owned(), command.to_owned(), "--no-interaction".to_owned(), "--no-ansi".to_owned()];

        let output = match project.run_process("php", &args, None) {
            Err(Error::Denied(_)) => {
                return Err("Allow the process:php capability in Settings › Plugins.".into());
            }
            result => result?,
        };

        if output.status != Some(0) {
            return Err(artisan_output::failure(command, output.status, &output.stdout, &output.stderr).into());
        }

        task.report(1, Some(2), Some("Indexing"));
        if let Ok(root)=project.root(){self.runtime_routes.remove(&root);}
        self.index_project(&project);
        host::notify(NotificationLevel::Info, SOURCE, &format!("Ran artisan {command} and reindexed the project."));
        Ok(None)
    }

    fn generator(&mut self, id: &str, ctx: &Invocation) -> Result<Option<String>, Error> {
        let project = ctx.project().ok_or("No project is open.")?;
        let root = project.root()?;
        if id == "new-model" {
            ui_kit::web::reveal_view(generator_workspace::VIEW, include_str!("ui_config.json"))?;
            return Ok(None);
        }
        if id == "make" && self.artisan_workspace.open_generator(ctx, None)? {
            return Ok(None);
        }
        let index = self.indexes.get(&root);

        match id {
            "make" => {
                let mut composer = project.read_file("composer.json").unwrap_or_default();
                if index.is_some_and(|i| !i.livewire.is_empty()) {
                    composer.push_str("\"livewire/livewire\"");
                }
                generate::make(&self.artisan_terminals, ctx, index, None, &composer)
            }
            "new-model" => generate::make(&self.artisan_terminals, ctx, index, Some("model"), ""),
            "generate-for-model" => {
                let runner=self.artisan_workspace.runner(ctx)?;
                let runtime=self.runtime_inspection.schema_for_generation(&project,runner).ok().map(|snapshot|(runner,snapshot));
                generate::for_model(&self.artisan_terminals, ctx, index, runtime)
            },
            _ => generate::create_missing(&self.artisan_terminals, ctx, index),
        }
    }

    fn unused_views(&self, ctx: &Invocation) -> Result<Option<String>, Error> {
        let project = ctx.project().ok_or("No project is open.")?;
        let root = project.root()?;
        let index = self.indexes.get(&root).ok_or("This project is not a Laravel application.")?;
        let _task = host::begin_task("Finding unused views", false);

        let mut sources: Vec<(&str, String)> = Vec::new();
        for glob in ["app/**/*.php", "routes/**/*.php", "resources/views/**/*.php", "database/**/*.php", "src/**/*.php"] {
            for path in project.find_files(glob, 5000).unwrap_or_default() {
                if let Ok(text) = project.read_file(&path) {
                    sources.push((if path.ends_with(".blade.php") { "blade" } else { "php" }, text));
                }
            }
        }

        let views: BTreeSet<&str> = index.views.keys().map(String::as_str).collect();
        let unused = unused::unused_views(&views, &sources);
        if unused.is_empty() {
            host::notify(NotificationLevel::Info, SOURCE, "Every view is used.");
            return Ok(None);
        }

        let choices: Vec<openidea_plugin::PickItem> = unused
            .iter()
            .map(|view| {
                let item = openidea_plugin::PickItem::new(&view.name, &view.name);
                if view.maybe { item.description("maybe used: the project builds view names at run time") } else { item }
            })
            .collect();
        let title = format!("{} unused views (choose one to open)", unused.len());
        let Some(name) = host::pick(&title, &choices)? else {
            return Ok(None);
        };

        if let Some(entry) = index.views.get(&name) {
            host::open_location(&entry.path, 0, 0)?;
        }
        Ok(None)
    }

    fn summary(&self) -> String {
        if self.indexes.is_empty() {
            return "No Laravel project is open.".into();
        }

        let lines: Vec<String> = self
            .indexes
            .iter()
            .map(|(root, i)| {
                let partial = if i.partial { " (index incomplete)" } else { "" };
                format!(
                    "{root}: {} routes, {} views, {} config keys, {} translations{partial}",
                    i.route_defs.len(),
                    i.views.len(),
                    i.config.len(),
                    i.translations.len(),
                )
            })
            .collect();
        lines.join("\n")
    }
}

impl Plugin for Main {
    fn activate(&mut self, _ctx: &Context) -> Result<(), Error> {
        for kind in [EventKind::FileSaved, EventKind::FileOpened, EventKind::TerminalExited] {
            host::subscribe(kind)?;
        }
        host::subscribe_paths("files-changed", &["**/*.php", "**/*.json", "**/*.sql", "**/*.js", "**/*.ts", "**/*.mjs", "**/*.mts", ".env*", "resources/**/*"])?;
        self.index_all();
        Ok(())
    }

    fn run_command(&mut self, id: &str, ctx: &Invocation, arg: Option<&str>) -> Result<Option<String>, Error> {
        match id {
            "new-project" => self.project_wizard.run(ctx),
            "sentry-configure" => { self.external_services.configure_sentry(ctx)?; refresh_laravel(); Ok(None) }
            "sentry-refresh" => { self.external_services.refresh_sentry(ctx)?; refresh_laravel(); Ok(None) }
            "sentry-more" => { self.external_services.load_more_sentry(ctx)?; refresh_laravel(); Ok(None) }
            "cloud-more" => { self.external_services.load_more_cloud(ctx)?; refresh_laravel(); Ok(None) }
            "cloud-environments-more" => { self.external_services.load_more_environments(ctx)?; refresh_laravel(); Ok(None) }
            "sentry-forget" => {
                self.external_services.forget_sentry(ctx)?;
                if let Some(root) = ctx.project().and_then(|project|project.root().ok()) { self.external_details.remove(&root); }
                refresh_laravel(); Ok(None)
            }
            "cloud-refresh" => { self.external_services.refresh_cloud(ctx)?; refresh_laravel(); Ok(None) }
            "sentry-disconnect" => { self.external_services.disconnect_sentry()?; self.external_details.clear(); refresh_laravel(); Ok(None) }
            "cloud-disconnect" => { self.external_services.disconnect_cloud()?; self.external_details.clear(); refresh_laravel(); Ok(None) }
            "cloud-environments" => { self.external_services.cloud_environments(ctx, arg.ok_or("Select a captured Cloud application.")?)?; refresh_laravel(); Ok(None) }
            "sentry-details" => {
                let root = ctx.project().ok_or(Error::Stale)?.root()?;
                let details = self.external_services.sentry_details(ctx, arg.ok_or("Select a captured Sentry issue.")?)?;
                self.external_details.insert(root, details);
                refresh_laravel(); Ok(None)
            }
            "external-list" => {
                let root = ctx.project().ok_or(Error::Stale)?.root()?;
                self.external_details.remove(&root); refresh_laravel(); Ok(None)
            }
            "runtime-schema" => {
                self.runtime_inspection.capture_selected_schema(ctx,self.artisan_workspace.runner(ctx)?)?;
                refresh_laravel();
                Ok(None)
            }
            "runtime-register-database" => {
                self.runtime_inspection.register_database_source(ctx, self.artisan_workspace.runner(ctx)?)?;
                Ok(None)
            }
            "helper-parameters" => {
                model_actions::configure(ctx)?;
                refresh_laravel();
                Ok(None)
            }
            "regenerate-model-helpers" => {
                model_actions::begin_project(ctx)?;
                Ok(None)
            }
            "model-from-database" => {
                let schema = self.runtime_inspection.schema_snapshot(ctx, self.artisan_workspace.runner(ctx)?)?;
                if let Some(draft) = model_from_schema::run(ctx, &schema)? {
                    self.generator_workspace.seed_model(ctx, draft)?;
                }
                Ok(None)
            }
            "model-open-source" => {
                let project = ctx.project().ok_or(Error::Stale)?;
                let index = self.view_index(ctx).ok_or(Error::Stale)?;
                model_overview::open(index, &project, arg.ok_or(Error::Stale)?)?;
                Ok(None)
            }
            "model-create-fields" => {
                let project = ctx.project().ok_or(Error::Stale)?;
                let root = project.root()?;
                if self.view_index(ctx).is_none() { return Err(Error::Stale); }
                let index = self.indexes.get(&root).ok_or(Error::Stale)?;
                let key = match arg {
                    Some(key) => key.to_owned(),
                    None => {
                        let choices: Vec<_> = model_overview::rows(index, &project).into_iter().filter(|row|row["cells"][1] == "Model").filter_map(|row|Some(openidea_plugin::PickItem::new(row["key"].as_str()?, row["cells"][0].as_str()?))).collect();
                        let Some(key) = host::pick("Add fields to model", &choices)? else { return Ok(None) };
                        key
                    }
                };
                let path = model_overview::source_path(index, &project, &key)?;
                self.generator_workspace.seed_existing(ctx, index, &path)?;
                Ok(None)
            }
            "runtime-open-frame" => {
                self.runtime_inspection.open_log_frame(ctx, self.artisan_workspace.runner(ctx)?, arg.ok_or("Select a captured Errors row first.")?)?;
                Ok(None)
            }
            "console-command-open" => {
                let name=console_editor::resolve(ctx,arg.ok_or(Error::Stale)?)?;
                if !self.artisan_workspace.select_command(ctx,&name)? {
                    ui_kit::web::reveal_view(artisan_workspace::VIEW,include_str!("ui_config.json"))?;
                    host::notify(NotificationLevel::Info,SOURCE,&format!("Refresh Artisan command metadata to configure {name}."));
                }
                Ok(None)
            }
            "route-search" => {
                let project = ctx.project().ok_or(Error::Stale)?;
                let index = Index::build(&project);
                if index.partial {return Err(Error::Failed("Laravel route index is incomplete; reindex before searching.".into()));}
                let initial = self.search_routes_from_index(ctx,&index);
                route_search::run(ctx, initial, || { let current=Index::build(&project); if current.partial {return Err(Error::Stale);} Ok(self.search_routes_from_index(ctx,&current)) })?;
                Ok(None)
            }
            "runtime-database-list" => {
                let root = ctx.project().ok_or(Error::Stale)?.root()?;
                let ids = self.runtime_inspection.database_sources(ctx)?;
                if ids.is_empty() { host::notify(NotificationLevel::Info, SOURCE, "No registered Laravel database sources. Add a Database Connection from Models first."); return Ok(None); }
                let choices: Vec<_> = ids.iter().map(|id|openidea_plugin::PickItem::new(id,id)).collect();
                let Some(id) = host::pick("Laravel database connections", &choices)? else { return Ok(None) };
                if !ids.contains(&id) {return Err(Error::Stale);}
                let Some(action) = host::pick("Database connection action", &[openidea_plugin::PickItem::new("connect", "Connect / enter credentials…"),openidea_plugin::PickItem::new("remove", "Remove connection and saved credentials…")])? else { return Ok(None) };
                if ctx.project().ok_or(Error::Stale)?.root()? != root || !host::projects()?.iter().any(|p|p.root().ok().as_deref()==Some(&root)) {return Err(Error::Stale);}
                self.runtime_inspection.database_source_action(ctx,&action,Some(&id))?;
                refresh_laravel(); Ok(None)
            }
            "runtime-database-connect" | "runtime-database-remove" => {
                let action = if id == "runtime-database-connect" {"connect"} else {"remove"};
                if let Some(key) = arg.filter(|key|key.starts_with("runtime:database:")) {
                    self.runtime_inspection.database_source_row_action(ctx,action,key)?;
                } else { self.runtime_inspection.database_source_action(ctx,action,arg)?; }
                refresh_laravel(); Ok(None)
            }
            "route-editor-open" => {
                let index = self.view_index(ctx).ok_or(Error::Stale)?;
                route_editor::open(index, ctx, arg.ok_or(Error::Stale)?)?;
                Ok(None)
            }
            "route-editor-action" => {
                let index = self.view_index(ctx).ok_or(Error::Stale)?;
                route_editor::action(index, ctx, arg.ok_or(Error::Stale)?, DEFAULT_BASE_URL)?;
                Ok(None)
            }
            "find-literal-source-matches" => crate::source_matches::run(ctx),
            "routes-runtime-refresh" => {
                let project=ctx.project().ok_or("Open a Laravel project first.")?;
                let root=project.root()?;
                if self.runtime_routes.len() >= 8 && !self.runtime_routes.contains_key(&root) { return Err(Error::Invalid("Too many runtime route snapshots; reindex to clear them.".into())); }
                self.index_project(&project);
                let index=self.view_index(ctx).ok_or("This project is not indexed as Laravel.")?;
                let partial=index.partial;
                let snapshot=route_runtime::capture(ctx,index)?;
                let count=snapshot.routes.len();
                let open: BTreeSet<_>=host::projects()?.iter().filter_map(|p|p.root().ok()).collect();
                self.runtime_routes.retain(|root,_|open.contains(root));
                self.runtime_routes.insert(root,snapshot);
                refresh_laravel();
                host::notify(NotificationLevel::Info,SOURCE,&format!("Rebuilt the source index{} and loaded {count} registered routes from Artisan.",if partial {" (incomplete)"} else {""}));
                Ok(None)
            }
            "reindex" => {
                self.runtime_routes.clear();
                let _task = host::begin_task("Indexing Laravel projects", false);
                self.index_all();
                host::notify(NotificationLevel::Info, SOURCE, &self.summary());
                Ok(None)
            }
            "optimize-clear" => self.artisan(ctx, "optimize:clear"),
            "cache-clear" => self.artisan(ctx, "cache:clear"),
            "config-clear" => self.artisan(ctx, "config:clear"),
            "route-clear" => self.artisan(ctx, "route:clear"),
            "view-clear" => self.artisan(ctx, "view:clear"),
            "migrate" => self.artisan(ctx, "migrate"),
            "storage-link" => self.artisan(ctx, "storage:link"),
            "db-seed" => self.artisan(ctx, "db:seed"),
            "route-copy-curl" | "route-create-http" => {
                let Some(mut route) = arg.and_then(|key| self.route_at(key, ctx)) else {
                    return Ok(None);
                };
                if route.method.contains('|') {
                    let choices:Vec<_>=route.method.split('|').map(|method|openidea_plugin::PickItem::new(method,method)).collect();
                    let Some(method)=host::pick("HTTP method",&choices)? else {return Ok(None);};
                    if !route.method.split('|').any(|known|known==method) {return Err(Error::Invalid("Unknown HTTP method.".into()));}
                    route.method=method;
                }
                match id {
                    "route-copy-curl" => copy_as_curl(&route, DEFAULT_BASE_URL)?,
                    _ => create_http_file_for_project(&route, DEFAULT_BASE_URL, None, &ctx.project().ok_or(Error::Stale)?)?,
                }
                Ok(None)
            }
            "migrate-fresh" => {
                let root = ctx.project().and_then(|project| project.root().ok()).unwrap_or_default();
                let confirmed = self
                    .fresh_asked
                    .take()
                    .is_some_and(|(asked_root, asked)| asked_root == root && asked.elapsed() < Duration::from_secs(30));
                if !confirmed {
                    self.fresh_asked = Some((root, Instant::now()));
                    host::notify(
                        NotificationLevel::Warning,
                        SOURCE,
                        "Fresh migration drops all tables. Run Laravel: Fresh Migration (drops all tables) again within 30 seconds to confirm.",
                    );
                    return Ok(None);
                }
                self.artisan(ctx, "migrate:fresh")
            }
            "runtime-about" | "runtime-events" | "runtime-schedule" | "runtime-queues" | "runtime-queue-status" | "runtime-workers" => {
                let section=id.strip_prefix("runtime-").ok_or(Error::Stale)?;
                self.runtime_inspection.capture(ctx,self.artisan_workspace.runner(ctx)?,section)?;
                refresh_laravel();
                Ok(None)
            },
            "runtime-inspect-queue" => {
                let Some(connection)=host::input("Queue connection (blank for all configured connections)",None)? else{return Ok(None)};
                let Some(queue)=host::input("Queue name (blank for configured defaults)",None)? else{return Ok(None)};
                self.runtime_inspection.capture_queue_status(ctx,self.artisan_workspace.runner(ctx)?,Some(connection.trim()).filter(|value|!value.is_empty()),Some(queue.trim()).filter(|value|!value.is_empty()))?;
                refresh_laravel();
                Ok(None)
            },
            "runtime-logs" => {
                let files=self.runtime_inspection.log_files(ctx)?;
                if files.is_empty(){host::notify(NotificationLevel::Info,SOURCE,"No owned log files found under storage/logs.");return Ok(None);}
                let choices:Vec<_>=files.iter().map(|file|openidea_plugin::PickItem::new(file,file)).collect();
                if let Some(file)=host::pick("Read Laravel log",&choices)? {
                    self.runtime_inspection.capture_log(ctx,self.artisan_workspace.runner(ctx)?,&file)?;
                    refresh_laravel();
                } Ok(None)
            },
            "runtime-retry-failed" => {
                let key=arg.ok_or("Select a captured failed-job row and choose Retry Failed Job.")?;
                let result=self.runtime_inspection.retry_failed(ctx,self.artisan_workspace.runner(ctx)?,key)?;
                host::notify(NotificationLevel::Info,SOURCE,&result);
                refresh_laravel();
                Ok(None)
            },
            "runtime-runner" => {
                if let Some(runner)=host::pick("Runtime PHP runner",&[openidea_plugin::PickItem::new("php","Local PHP"),openidea_plugin::PickItem::new("docker","Docker")])? {
                    let runner:artisan_runner::Runner=serde_json::from_value(serde_json::json!(runner)).map_err(|_|Error::Stale)?;
                    self.artisan_workspace.handle(ctx,&serde_json::json!({"op":"runner","runner":runner}).to_string(),&self.artisan_terminals)?;
                    refresh_laravel();
                } Ok(None)
            },
            "unused-views" => self.unused_views(ctx),
            "artisan-run" => { ui_kit::web::reveal_view(artisan_workspace::VIEW, include_str!("ui_config.json"))?; Ok(None) },
            "artisan-run-terminal" => generate::run_artisan(&self.artisan_terminals, ctx),
            "artisan-refresh-metadata" => artisan_metadata::refresh(ctx),
            "artisan-refresh-docker-metadata" => artisan_docker::refresh(ctx),
            "artisan-choose-docker-target" => {let result=artisan_docker::discover(ctx)?;refresh_laravel();Ok(result)},
            "artisan-clear-docker-target" => {let result=artisan_docker::clear_discovered_target(ctx)?;refresh_laravel();Ok(result)},
            "artisan-browse-docker-metadata" => artisan_docker::browse(ctx),
            "make" | "new-model" | "generate-for-model" | "create-missing" => self.generator(id, ctx),
            "remove-phpdocs" => {
                let root = ctx.project().and_then(|project| project.root().ok()).unwrap_or_default();
                bulk::remove_phpdocs(ctx, self.indexes.get(&root))
            }
            "reveal" => {
                host::reveal_view(VIEW)?;
                Ok(None)
            }
            "route-list" => {
                generate::run(&self.artisan_terminals, ctx, "route:list", Vec::new())?;
                Ok(None)
            }
            _ => Err(Error::NotFound(format!("command {id}"))),
        }
    }

    fn handle_events(&mut self, events: &[Event], ctx: &Invocation) {
        for event in events {
            if let Some(exit) = event.terminal_exited() {
                if let Err(error) = self.project_wizard.complete(&exit) {
                    host::notify(NotificationLevel::Error, SOURCE, &error.to_string());
                }
            }
            if let Some(exit) = event.terminal_exited()
                && let Some(root) = self.artisan_terminals.complete(&exit, ctx)
                && let Some(project) = host::projects().unwrap_or_default().into_iter().find(|p|p.root().ok().as_deref()==Some(&root)) {
                self.runtime_routes.remove(&root);
                self.index_project(&project);
                refresh_laravel();
            }
        }
        let mut saved: BTreeSet<&str> = BTreeSet::new();
        let mut opened: BTreeSet<&str> = BTreeSet::new();

        for event in events {
            match event {
                Event::FilesChanged(batch) => {
                    saved.extend(batch.created.iter().chain(&batch.modified).chain(&batch.removed).map(String::as_str));
                }
                Event::FileSaved(path) => {
                    saved.insert(path);
                }
                Event::FileOpened(path) => {
                    opened.insert(path);
                }
                _ => {}
            }
        }

        for project in host::projects().unwrap_or_default() {
            let Ok(root) = project.root() else { continue };
            let relative = |path: &&str| path.strip_prefix(&format!("{root}/")).map(str::to_owned);

            let changed = saved.iter().filter_map(relative).any(|r| affects_index(&r) || self.indexes.get(&root).is_some_and(|index| index.affects(&r)));
            let unknown = !self.indexes.contains_key(&root) && opened.iter().any(|path| under(path, &root));
            if saved.iter().any(|path|under(path,&root) && (path.ends_with(".php") || path.rsplit('/').next().is_some_and(|name|name.starts_with(".env") || matches!(name,"composer.json"|"composer.lock")))) { self.runtime_routes.remove(&root); self.runtime_inspection.invalidate(&root); }
            if changed || unknown {
                self.index_project(&project);
            }

            for path in saved.iter().chain(&opened).filter(|path| under(path, &root)) {
                self.check(&project, path);
            }
        }
    }
}

fn component_view_text(index: &Index, project: &Project, component: &str) -> Option<String> {
    let anonymous = components::metadata_view(index,component)
        .or_else(||index.components.get(component).filter(|entry|entry.path.ends_with(".blade.php")))
        .or_else(||index.views.get(&format!("components.{component}")))
        .or_else(||index.views.get(&format!("components.{component}.index")))?;
    if !owns_current_path(project, &anonymous.path) { return None; }
    project.read_file(&anonymous.path).ok()
}

/// Attributes and slots of the component named by the tag around the caret.
fn component_items(index: &Index, doc: &Document, offset: usize, context: &components::Context) -> Vec<CompletionItem> {
    let Some(project) = project_for(&doc.path) else {
        return Vec::new();
    };

    let (component, kind, start, slots) = match context {
        components::Context::Attribute { kind, component, start } => (component, *kind, *start, false),
        components::Context::Slot { component, start } => (component, Kind::Component, *start, true),
    };
    let replace = range_of(&doc.text, start, offset);

    let (props, view) = if kind == Kind::Livewire {
        let class =
            index.livewire.get(component).filter(|entry| !entry.path.is_empty() && owns_current_path(&project, &entry.path)).and_then(|entry| project.read_file(&entry.path).ok());
        (class.map(|class| components::livewire_props(&class)).unwrap_or_default(), None)
    } else {
        let view = component_view_text(index, &project, component);
        let class = index
            .components
            .get(component)
            .filter(|entry| entry.path.ends_with(".php") && !entry.path.ends_with(".blade.php") && owns_current_path(&project, &entry.path))
            .and_then(|entry| project.read_file(&entry.path).ok());
        let props = match &class {
            Some(class) => components::constructor_props(class),
            None => view.as_deref().map(components::view_props).unwrap_or_default(),
        };
        let view =
            view.or_else(|| index.views.get(&format!("components.{component}")).filter(|entry| owns_current_path(&project, &entry.path)).and_then(|entry| project.read_file(&entry.path).ok()));
        (props, view)
    };

    if slots {
        return view
            .map(|view| components::slot_names(&view, &props))
            .unwrap_or_default()
            .into_iter()
            .map(|name| {
                let mut item = CompletionItem::new(&components::attribute_name(&name), CompletionKind::Property);
                item.detail = Some("slot".into());
                item.replace = Some(replace);
                item
            })
            .collect();
    }

    props
        .into_iter()
        .map(|prop| {
            let mut item = CompletionItem::new(&components::attribute_name(&prop.name), CompletionKind::Property);
            item.detail = prop.default.map(|default| format!("default {default}"));
            item.replace = Some(replace);
            item
        })
        .collect()
}

/// The class name under the caret in the providers of `config/app.php` or `bootstrap/providers.php`.
fn provider_token(path: &str, text: &str, offset: usize) -> Option<Range> {
    let whole_file = path.ends_with("bootstrap/providers.php");
    if !whole_file && !path.ends_with("config/app.php") {
        return None;
    }

    if !whole_file {
        let key = text.find("'providers'")?;
        let open = text[key..].find('[').map(|i| key + i)?;
        let close = php::closing(text, open)?;
        if offset <= open || offset > close {
            return None;
        }
    }

    let bytes = text.as_bytes();
    let class_byte = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'\\');
    let mut start = offset.min(bytes.len());
    while start > 0 && class_byte(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset.min(bytes.len());
    while end < bytes.len() && class_byte(bytes[end]) {
        end += 1;
    }

    let before = text[..start].trim_end();
    let valid = start < end && bytes[start] != b'\\' || start + 1 < end;
    (valid && (before.ends_with('[') || before.ends_with(',')) && !text[..start].ends_with('\'')).then(|| range_of(text, start, end))
}

fn provider_items(index: &Index, doc: &Document, token: Range) -> Vec<CompletionItem> {
    let end = text::offset(&doc.text, token.end).unwrap_or(0);
    let followed = doc.text[end..].starts_with("::class");

    index
        .providers
        .keys()
        .map(|name| {
            let mut item = CompletionItem::new(name, CompletionKind::Class);
            item.replace = Some(token);
            if !followed {
                item.insert_text = format!("{name}::class");
            }
            item
        })
        .collect()
}

fn directive_items(index: Option<&Index>, replace: Range, ranking: &directives::Ranking) -> Vec<CompletionItem> {
    let livewire = index.is_none_or(|index| index.metadata.package_enabled("livewire/livewire", true));
    let core = directives::CORE.iter().filter(|name| livewire || !matches!(**name, "livewire" | "livewireStyles" | "livewireScripts" | "livewireScriptConfig")).map(|name| (*name, None, true));
    let custom = index.into_iter().flat_map(|index| index.directives.iter()).map(|(name, entry)| (name.as_str(), entry.detail.clone(), false));

    let mut seen = BTreeSet::new();
    let mut items: Vec<_> = custom
        .chain(core)
        .filter(|(name, _, _)| seen.insert(*name))
        .map(|(name, detail, is_core)| {
            let mut item = CompletionItem::new(name, CompletionKind::Keyword);
            item.detail = detail;
            item.sort_text = Some(ranking.key(name, is_core));
            item.replace = Some(replace);
            if is_core && let Some((_, body)) = directives::SNIPPETS.iter().find(|(n, _)| *n == name) {
                item.snippet = true;
                item.insert_text = if ranking.repairs_opening(name) {
                    format!("{}$0", body.lines().next().unwrap_or(name))
                } else {
                    (*body).to_owned()
                };
            }
            item
        })
        .collect();
    items.sort_by(|a, b| a.sort_text.cmp(&b.sort_text));
    items
}

impl Main {
    fn script_completion(&self, doc: &Document, at: Position) -> Vec<CompletionItem> {
        if !js::mentions_route(&doc.text) {
            return Vec::new();
        }

        let Some(offset) = text::offset(&doc.text, at) else {
            return Vec::new();
        };
        let Some(name) = js::route_name_at(&doc.text, offset) else {
            return Vec::new();
        };
        let Some(index) = self.index_for(&doc.path) else {
            return Vec::new();
        };

        let replace = range_of(&doc.text, name.start, name.end);
        index
            .routes
            .iter()
            .map(|(name, entry)| {
                let mut item = CompletionItem::new(name, CompletionKind::Reference);
                item.detail = entry.detail.clone();
                item.replace = Some(replace);
                item
            })
            .collect()
    }

    fn script_definition(&self, doc: &Document, at: Position) -> Vec<Location> {
        if let (Some(project), Some(index)) = (project_for(&doc.path), self.index_for(&doc.path)) {
            let wayfinder = package_facts::wayfinder_definition(&project, index, doc, at);
            if !wayfinder.is_empty() { return wayfinder; }
        }
        if !js::mentions_route(&doc.text) {
            return Vec::new();
        }

        let Some(offset) = text::offset(&doc.text, at) else {
            return Vec::new();
        };
        let Some(name) = js::route_name_at(&doc.text, offset) else {
            return Vec::new();
        };
        let Some(index) = self.index_for(&doc.path) else {
            return Vec::new();
        };

        let Some(project) = project_for(&doc.path) else { return Vec::new(); };
        index.routes.get(&doc.text[name]).and_then(|entry| scoped_location(&project, entry)).into_iter().collect()
    }
}

impl Providers for Main {
    fn completion(&mut self, doc: &Document, at: Position, _trigger: Option<&str>) -> Vec<CompletionItem> {
        if js::is_script(&doc.path, &doc.language) {
            return self.script_completion(doc, at);
        }

        let Some(language) = language_of(&doc.path, &doc.language) else {
            return Vec::new();
        };
        let Some(offset) = text::offset(&doc.text, at) else {
            return Vec::new();
        };
        if language == "blade" && blade_php::opaque_at(&doc.text, offset) { return Vec::new(); }
        if language == "blade" && let Some((start, end)) = directives::name_at(&doc.text, offset) {
            let custom = self.index_for(&doc.path).into_iter().flat_map(|index| index.directives.keys().map(String::as_str)).collect();
            let Some(ranking) = directives::ranking(&doc.text, offset, start, end, &custom) else {
                return Vec::new();
            };
            return directive_items(self.index_for(&doc.path), range_of(&doc.text, start, end), &ranking);
        }
        let Some(index) = self.index_for(&doc.path) else {
            return Vec::new();
        };
        if language=="blade" {let bare=components::bare_completion(index,doc,at);if !bare.is_empty(){return bare;}}
        let scopes = package_facts::passport_completion(index, doc, at);
        if !scopes.is_empty() { return scopes; }
        if language == "php" {
            let custom = metadata::completion(index, &doc.text, offset);
            if !custom.is_empty() { return custom; }
        }
        if language == "php" {
            let root = project_for(&doc.path).and_then(|p| p.root().ok());
            let items = eloquent_members::completion_checked(index, doc, at, &|path| root.is_some() && project_for(path).and_then(|p|p.root().ok()) == root);
            if !items.is_empty() { return items; }
        }
        if language == "php" && let Some(token) = provider_token(&doc.path, &doc.text, offset) {
            return provider_items(index, doc, token);
        }
        if language == "blade"
            && let Some(context) = components::context_with_metadata(index,&doc.text, offset)
        {
            return component_items(index, doc, offset, &context);
        }

        let Some(reference) = refs::reference_at(language, &doc.text, offset).or_else(||(language=="php").then(||metadata::route_reference(&index.metadata,&doc.text,offset)).flatten()) else {
            return Vec::new();
        };

        let replace = range_of(&doc.text, reference.start, reference.end);
        candidates(index, doc, &reference)
            .iter()
            .map(|(name, entry)| {
                let mut item = CompletionItem::new(name, completion_kind(reference.kind));
                item.detail = entry.detail.clone();
                item.replace = Some(replace);
                item
            })
            .collect()
    }

    fn definition(&mut self, doc: &Document, at: Position) -> Vec<Location> {
        if js::is_script(&doc.path, &doc.language) {
            return self.script_definition(doc, at);
        }

        let Some(language) = language_of(&doc.path, &doc.language) else {
            return Vec::new();
        };
        let Some(index) = self.index_for(&doc.path) else {
            return Vec::new();
        };
        let Some(offset) = text::offset(&doc.text, at) else {
            return Vec::new();
        };
        if language=="blade" {let bare=components::bare_definition(index,doc,at);if !bare.is_empty(){return bare;}}
        let navigation = indexer::navigation(&doc.path, &doc.text);
        let resolver = ui_kit::navigation::Resolver::new(
            index
                .navigation
                .iter()
                .filter(|(path, _)| **path != doc.path)
                .map(|(path, g)| (path.as_str(), g))
                .chain(std::iter::once((doc.path.as_str(), &navigation))),
        );
        let Some(project) = project_for(&doc.path) else { return Vec::new() };
        let scopes = package_facts::passport_definition(index, doc, at);
        if !scopes.is_empty() { return scopes; }
        if language == "php" {
            let custom = metadata::definition(index, &doc.text, offset);
            let owned: Vec<_> = custom.into_iter().filter(|location| owns_current_path(&project, &location.path)).collect();
            if !owned.is_empty() { return owned; }
        }
        if language == "php" && let Some(entry) = eloquent_members::definition_checked(index, doc, at, &|path| project_for(path).and_then(|p|p.root().ok()) == project.root().ok()) {
            return scoped_location(&project, &entry).into_iter().collect();
        }
        let related = resolver
            .checked(|path| {
                if path == doc.path {
                    return Some(navigation.clone());
                }
                if !owns_current_path(&project, path) { return None; }
                project.read_file(path).ok().filter(|text| text.len() <= 512 * 1024).map(|text| indexer::navigation(path, &text))
            })
            .definition(&navigation, at);
        if !related.is_empty() {
            return related.into_iter().filter(|target| owns_current_path(&project, &target.path)).collect();
        }

        if language == "php"
            && let Some(token) = provider_token(&doc.path, &doc.text, offset)
        {
            let start = text::offset(&doc.text, token.start).unwrap_or(0);
            let end = text::offset(&doc.text, token.end).unwrap_or(0);
            return index.bindings.get(doc.text[start..end].trim_start_matches('\\')).and_then(|entry| scoped_location(&project, entry)).into_iter().collect();
        }
        let Some(reference) = refs::reference_at(language, &doc.text, offset).or_else(||(language=="php").then(||metadata::route_reference(&index.metadata,&doc.text,offset)).flatten()) else {
            return Vec::new();
        };

        let name = &doc.text[reference.start..reference.end];
        let names = candidates(index, doc, &reference);
        let name = if reference.kind == Kind::Binding { name.replace("\\\\", "\\") } else { name.to_owned() };
        names.get(&name).and_then(|entry| scoped_location(&project, entry)).into_iter().collect()
    }

    /// Controller ↔ route, `view()` → view file, view → controllers and views
    /// using it, section ↔ parent section, push ↔ stack.
    fn line_markers(&mut self, doc: &Document) -> Vec<LineMarker> {
        let Some(language) = language_of(&doc.path, &doc.language) else {
            return Vec::new();
        };
        let Some(index) = self.index_for(&doc.path) else {
            return Vec::new();
        };
        let Some(project) = project_for(&doc.path) else {
            return Vec::new();
        };

        let markers = match language {
            "php" => {
                let navigation = indexer::navigation(&doc.path, &doc.text);
                let resolver = ui_kit::navigation::Resolver::new(
                    index
                        .navigation
                        .iter()
                        .filter(|(path, _)| **path != doc.path)
                        .map(|(path, g)| (path.as_str(), g))
                        .chain(std::iter::once((doc.path.as_str(), &navigation))),
                );
                let mut markers = php_markers(index, &project, doc);
                markers.extend(
                    resolver
                        .checked(|path| {
                            if path == doc.path {
                                return Some(navigation.clone());
                            }
                            if !owns_current_path(&project, path) { return None; }
                            project
                                .read_file(path)
                                .ok()
                                .filter(|text| text.len() <= 512 * 1024)
                                .map(|text| indexer::navigation(path, &text))
                        })
                        .markers(&navigation),
                );
                markers
            }
            _ => blade_markers(index, &project, doc),
        };
        markers.into_iter().filter(|marker| marker.target.as_ref().is_none_or(|target| owns_current_path(&project, &target.path))).collect()
    }

    fn code_actions(&mut self, doc: &Document, selection: Range) -> Vec<CodeAction> {
        let Some(language) = language_of(&doc.path, &doc.language) else {
            return Vec::new();
        };
        let Some(offset) = text::offset(&doc.text, selection.start) else {
            return Vec::new();
        };

        let mut found = Vec::new();
        if let Some(index) = self.index_for(&doc.path) {
            found.extend(missing_key_actions(index, doc, language, offset));
        }

        if language == "php" && enabled("checkEloquent") {
            let masked = php::mask_comments(&doc.text);
            found.extend(eloquent_checks::order_by(&masked).into_iter().filter(|f| f.start <= offset && offset <= f.end).filter_map(
                |finding| {
                    Some(CodeAction {
                        title: format!("Replace with {}", finding.replacement.as_deref()?),
                        kind: Some("quickfix".into()),
                        preferred: true,
                        edits: vec![(
                            doc.path.clone(),
                            vec![TextEdit { range: range_of(&doc.text, finding.start, finding.end), text: finding.replacement? }],
                        )],
                        data: None,
                    })
                },
            ));
        }

        if language == "php"
            && let Some((range, text)) = unused::remove_model_phpdocs(&doc.text, offset)
        {
            found.push(CodeAction {
                title: "Remove IDE helper PHPDoc from model".into(),
                kind: Some("refactor.rewrite".into()),
                preferred: false,
                edits: vec![(doc.path.clone(), vec![TextEdit { range: range_of(&doc.text, range.start, range.end), text }])],
                data: None,
            });
        }

        let controllers_namespace = host::setting_string("controllersNamespace")
            .filter(|namespace| !namespace.is_empty())
            .unwrap_or_else(|| "App\\Http\\Controllers".into());
        let version = self.index_for(&doc.path).and_then(|index| index.metadata.framework_version);
        let features = actions::FrameworkFeatures {
            rule_array: version.is_some_and(|version| version >= (12,0,0)),
            scope_attribute: version.is_some_and(|version| version >= (12,6,0)) && host::setting_bool("scopeAttributeIntentions").unwrap_or(true),
            scope_model: self.index_for(&doc.path).is_some_and(|index|eloquent_members::scope_model_source(index,doc)),
            blade_class: version.is_some_and(|version|version >= (9,0,0)),
        };
        let mut rewrites = actions::rewrites_with_framework_features(&doc.path, language, &doc.text, offset, &controllers_namespace, features);
        if language == "php"
            && let Some(index) = self.index_for(&doc.path)
        {
            rewrites.extend(lang_actions::column_rewrites(index, &doc.path, &doc.text, offset));
        }
        found.extend(rewrites.into_iter().map(|rewrite| CodeAction {
            title: rewrite.title,
            kind: Some(rewrite.kind.into()),
            preferred: false,
            edits: vec![(
                doc.path.clone(),
                rewrite.edits.into_iter().map(|(range, text)| TextEdit { range: range_of(&doc.text, range.start, range.end), text,
                        }).collect(),
            )],
            data: None,
        }));

        if let (Some(index), Some(project)) = (self.index_for(&doc.path), project_for(&doc.path)) {
            found.extend(lang_actions::extract_string(index, &project, doc, language, selection));
        }

        found
    }

    fn resolve_code_action(&mut self, _doc: &Document, mut action: CodeAction) -> Result<CodeAction, Error> {
        let Some(data) = action.data.clone() else {
            return Ok(action);
        };
        let (file, key) = data.split_once('\n').ok_or("Malformed code action.")?;
        let project = project_for(file).ok_or("The file is not in an open project.")?;
        let text = project.read_file(file)?;

        let edit = lang_actions::entry_edit(&text, key, key).ok_or("The lang file doesn't end its array.")?;

        action.edits = vec![(file.to_owned(), vec![edit])];
        Ok(action)
    }
}

/// Adds a missing translation key to the main locale's lang file.
fn missing_key_actions(index: &Index, doc: &Document, language: &str, offset: usize) -> Vec<CodeAction> {
    let Some(reference) = refs::reference_at(language, &doc.text, offset) else {
        return Vec::new();
    };

    let name = &doc.text[reference.start..reference.end];
    if reference.kind != Kind::Translation || is_dynamic(name) || !missing_translation(index, name) {
        return Vec::new();
    }

    let Some((file_key, key)) = name.split_once('.') else {
        return Vec::new();
    };
    if key.contains('.') {
        return Vec::new();
    }

    let files = index.translation_files.get(file_key).map(Vec::as_slice).unwrap_or_default();
    files
        .iter()
        .take(1)
        .map(|file| CodeAction {
            title: format!("Add translation key \"{name}\" to {}/{}", file.rsplit('/').nth(1).unwrap_or_default(), file_name(file)),
            kind: Some("quickfix".into()),
            preferred: false,
            edits: Vec::new(),
            data: Some(format!("{file}\n{key}")),
        })
        .collect()
}

fn php_markers(index: &Index, project: &Project, doc: &Document) -> Vec<LineMarker> {
    let mut markers = Vec::new();
    let line_of = |offset: usize| text::position(&doc.text, offset).line;

    for reference in refs::references("php", &doc.text).into_iter().filter(|r| r.closed) {
        let name = &doc.text[reference.start..reference.end];
        let (target, label) = match reference.kind {
            Kind::View => (index.views.get(name), "View"),
            Kind::Controller => (index.controllers.get(name), "Controller"),
            _ => continue,
        };

        if let Some(target) = target.and_then(location) {
            markers.push(LineMarker {
                line: line_of(reference.start),
                icon: None,
                tooltip: format!("{label} {name}"),
                target: Some(target),
            });
        }
    }

    let Ok(root) = project.root() else {
        return markers;
    };
    let Some(inside) = doc.path.strip_prefix(&format!("{root}/{CONTROLLERS_DIRECTORY}")) else {
        return markers;
    };
    let masked = php::mask_comments(&doc.text);
    let Some(controller) = controllers::parse(inside, &masked) else {
        return markers;
    };

    let short = controller.class.rsplit('\\').next().unwrap_or_default();
    for (method, offset) in &controller.methods {
        let route = index.route_defs.iter().find(|r| r.def.controller.as_ref().is_some_and(|(class, m)| class == short && m == method));
        let Some(route) = route else { continue };

        let end = Position::new(route.at.line, route.at.character + route.width);
        let tooltip = format!("Route {} {}", route.def.methods, route.def.uri);
        let target = Location { path: route.path.clone(), range: Range::new(route.at, end) };
        markers.push(LineMarker { line: line_of(*offset), icon: Some("run".into()), tooltip, target: Some(target) });
    }

    markers
}

/// Where `name` is defined by `pick` in the view's file.
fn locate(index: &Index, project: &Project, view: &str, pick: fn(&blade::Facts) -> Vec<&blade::Named>, name: &str) -> Option<Location> {
    let (entry, source, facts) = current_blade_view(index, project, view)?;
    let (_, offset) = pick(&facts).into_iter().find(|(n, _)| n == name)?;
    let lines = Lines::new(&source);
    let start = lines.position(*offset);
    let width = lines.width(*offset, name.len()) as u32;
    Some(Location { path: entry.path.clone(), range: Range::new(start, Position::new(start.line, start.character + width)) })
}

fn sections_and_yields(facts: &blade::Facts) -> Vec<&blade::Named> {
    facts.sections.iter().chain(&facts.yields).collect()
}

fn only_sections(facts: &blade::Facts) -> Vec<&blade::Named> {
    facts.sections.iter().collect()
}

fn only_stacks(facts: &blade::Facts) -> Vec<&blade::Named> {
    facts.stacks.iter().collect()
}

fn only_pushes(facts: &blade::Facts) -> Vec<&blade::Named> {
    facts.pushes.iter().collect()
}

fn blade_markers(index: &Index, project: &Project, doc: &Document) -> Vec<LineMarker> {
    let mut markers = Vec::new();
    let line_of = |offset: usize| text::position(&doc.text, offset).line;
    let mut marker = |line: u32, icon: &str, tooltip: String, target: Option<Location>| {
        markers.push(LineMarker { line, icon: Some(icon.into()), tooltip, target });
    };

    let facts = blade::facts(&doc.text);
    let view = index.view_of(&doc.path);

    if let Some(view) = view {
        let users = index.users_of(view);
        if let Some(first) = users.first().and_then(|v| index.views.get(*v)) {
            let target = Location { path: first.path.clone(), range: Range::new(Position::new(0, 0), Position::new(0, 0)) };
            marker(0, "implemented", format!("Used by {}", users.join(", ")), Some(target));
        }

        let renders: Vec<&index::Render> = index.renders.iter().filter(|r| r.view == view).collect();
        if let Some(first) = renders.first() {
            let callers: Vec<&str> = renders.iter().map(|r| r.caller.as_str()).collect();
            let target = Location { path: first.path.clone(), range: Range::new(first.at, first.at) };
            marker(0, "navigate", format!("Rendered by {}", callers.join(", ")), Some(target));
        }
    }

    let parents: Vec<&str> = index.layout_parent(&facts).map(|p| std::iter::once(p).chain(index.parents(p)).collect()).unwrap_or_default();
    let descendants: Vec<&str> = match view {
        Some(view) => index.blade.keys().map(String::as_str).filter(|v| index.parents(v).contains(&view)).collect(),
        None => Vec::new(),
    };

    for (name, offset) in &facts.sections {
        let line = line_of(*offset);
        let parent = parents.iter().find_map(|p| locate(index, project, p, sections_and_yields, name));
        if let Some(parent) = parent {
            marker(line, "overriding", format!("Parent section \"{name}\""), Some(parent));
        }
    }

    for (name, offset) in facts.sections.iter().chain(&facts.yields) {
        let child = descendants.iter().find_map(|d| locate(index, project, d, only_sections, name));
        if let Some(child) = child {
            marker(line_of(*offset), "overridden", format!("Section \"{name}\" is filled by another view"), Some(child));
        }
    }

    for (name, offset) in &facts.pushes {
        let stack = parents.iter().find_map(|p| locate(index, project, p, only_stacks, name));
        if let Some(stack) = stack {
            marker(line_of(*offset), "implementing", format!("Stack \"{name}\""), Some(stack));
        }
    }

    for (name, offset) in &facts.stacks {
        let push = descendants.iter().find_map(|d| locate(index, project, d, only_pushes, name));
        if let Some(push) = push {
            marker(line_of(*offset), "implemented", format!("Stack \"{name}\" is pushed to by another view"), Some(push));
        }
    }

    markers
}

/// Tool window node keys: `group:<kind>`, `<kind>:<name>`, `route:<number>`.
const GROUPS: &[(&str, &str, &str)] = &[
    ("route", "Routes", "web"),
    ("view", "Views", "file-html"),
    ("config", "Config", "file-config"),
    ("translation", "Translations", "text"),
    ("middleware", "Middleware", "class"),
    ("env", "Env", "file-dotenv"),
];

fn group_names<'a>(index: &'a Index, group: &str) -> Option<&'a Names> {
    match group {
        "view" => Some(&index.views),
        "component" => Some(&index.components),
        "livewire" => Some(&index.livewire),
        "config" => Some(&index.config),
        "translation" => Some(&index.translations),
        "middleware" => Some(&index.middleware),
        "env" => Some(&index.env),
        _ => None,
    }
}

fn route_list_of(index: &Index) -> Vec<Route> {
    index
        .route_defs
        .iter()
        .map(|entry| {
            let def = &entry.def;
            let handler = match &def.controller {
                Some((class, method)) => Handler { class: class.clone(), member: Some(method.clone()) },
                None => Handler { class: def.action.clone(), member: None },
            };
            let end = Position::new(entry.at.line, entry.at.character + entry.width);
            Route {
                method: def.methods.split('|').filter(|method|!(*method=="HEAD" && def.methods.split('|').any(|m|m=="GET"))).collect::<Vec<_>>().join("|"),
                path: def.uri.clone(),
                handler,
                name: def.name.clone(),
                location: Some(Location { path: entry.path.clone(), range: Range::new(entry.at, end) }),
            }
        })
        .collect()
}

fn entry_for<'a>(index: &'a Index, key: &str) -> Option<(&'a str, Position, u32)> {
    if let Some(encoded) = key.strip_prefix("translation-locale:") {
        let (locale, name): (String, String) = serde_json::from_str(encoded).ok()?;
        let entry = index.translations_by_locale.get(&locale)?.get(&name)?;
        return Some((&entry.path, entry.at, entry.width));
    }
    let (group, name) = key.split_once(':')?;
    if group == "route" {
        let route = index.route_defs.get(name.parse::<usize>().ok()?)?;
        return Some((&route.path, route.at, route.width));
    }

    let entry = group_names(index, group)?.get(name)?;
    Some((&entry.path, entry.at, entry.width))
}

fn view_inventory(index: &Index, project: &Project) -> Body {
    let root=project.root().unwrap_or_default();
    let items: Vec<serde_json::Value> = index.views.iter().filter(|(_,entry)|owns_current_path(project,&entry.path)).map(|(name, entry)| {
        let namespace=name.split_once("::").map_or("app",|(namespace,_)|namespace);
        let file=entry.path.strip_prefix(&root).unwrap_or(&entry.path).trim_start_matches(['/', '\\']);
        let folder=index.view_folders.get(name).map(String::as_str).unwrap_or("(root)");
        serde_json::json!({"key":format!("view:{name}"),"cells":[name,file,folder,namespace],"icon":"file-html"})
    }).collect();
    Body::Other { kind:"resources".into(),data:serde_json::json!({"columns":["View","File","Folder","Namespace"],"columnStyles":["code","code","muted","locale"],"columnWeights":[3,4,2,1.5],"showRowIcons":true,"searchPlaceholder":"Filter views…","facetLabels":{"2":"All folders","3":"All namespaces"},"facets":[2,3],"items":items}).to_string() }
}

fn component_inventory(index: &Index, project: &Project) -> Body {
    let root = project.root().unwrap_or_default();
    let items: Vec<serde_json::Value> = [("component", &index.components), ("livewire", &index.livewire)]
        .into_iter().flat_map(|(kind, names)| names.iter().map(move |(name, entry)| (kind, name, entry)))
        .filter(|(_, _, entry)| !entry.path.is_empty() && owns_current_path(project, &entry.path))
        .map(|(kind, name, entry)| {
            let file = entry.path.strip_prefix(&root).unwrap_or(&entry.path).trim_start_matches(['/', '\\']);
            let (label, category, icon) = if kind == "livewire" {
                (format!("<livewire:{name}>"), "Livewire", "class")
            } else if entry.detail.as_deref() == Some("Blade::component") {
                (format!("<x-{name}>"), "Blade alias", "file-html")
            } else if entry.path.ends_with(".blade.php") {
                (format!("<x-{name}>"), "Blade anonymous", "file-html")
            } else {
                (format!("<x-{name}>"), "Blade class", "class")
            };
            serde_json::json!({"key":format!("{kind}:{name}"),"cells":[label,category,file],"icon":icon})
        }).collect();
    Body::Other { kind: "resources".into(), data: serde_json::json!({
        "columns":["Component","Type","File"],"columnStyles":["code","text","muted"],
        "columnWeights":[3,1.5,5],"showRowIcons":true,"searchPlaceholder":"Filter components…",
        "facets":[1],"facetLabels":{"1":"All types"},"items":items
    }).to_string() }
}

fn config_inventory(index: &Index, root: &str) -> Body {
    let items: Vec<serde_json::Value> = index.config.iter().map(|(name, entry)| {
        let value = index.config_expressions.get(name).map(String::as_str).unwrap_or_else(|| entry.detail.as_deref().unwrap_or("Expression unavailable"));
        serde_json::json!({"key":format!("config:{name}"),"cells":[name,value,entry.path.strip_prefix(root).unwrap_or(&entry.path).trim_start_matches(['/', '\\'])],"icon":"file-config"})
    }).collect();
    Body::Other { kind:"resources".into(), data:serde_json::json!({"columns":["Key","Value","File"],"columnStyles":["code","code","muted"],"showRowIcons":true,"searchPlaceholder":"Filter config…","facetLabels":{"2":"All files"},"facets":[2],"items":items}).to_string() }
}

fn translation_inventory(index: &Index, root: &str) -> Body {
    let items: Vec<serde_json::Value> = index.translations_by_locale.iter().flat_map(|(locale, names)| {
        names.iter().map(move |(name, entry)| serde_json::json!({
            "key": format!("translation-locale:{}", serde_json::json!([locale, name])),
            "cells": [name, locale, entry.detail.as_deref().unwrap_or(""), entry.path.strip_prefix(root).unwrap_or(&entry.path).trim_start_matches(['/', '\\'])],
            "icon": "text"
        }))
    }).collect();
    Body::Other { kind: "resources".into(), data: serde_json::json!({"columns":["Key","Locale","Value","File"],"columnStyles":["code","locale","text","muted"],"showRowIcons":true,"searchPlaceholder":"Filter translations…","facetLabels":{"1":"All locales"},"facets":[1],"items":items}).to_string() }
}

impl Main {
    fn tree(&self, index: &Index, group: &str) -> Vec<TreeItem> {
        let icon = GROUPS.iter().find(|(id, _, _)| *id == group).map_or("file", |(_, _, icon)| *icon);
        group_names(index, group).into_iter().flat_map(|names| names.iter()).map(|(name, entry)| {
            let leaf = TreeItem::new(&format!("{group}:{name}"), name).icon(icon);
            match &entry.detail {
                Some(detail) if !detail.is_empty() => leaf.detail(detail),
                _ => leaf,
            }
        }).collect()
    }

    fn route_list(&mut self) -> &mut RouteList {
        self.route_list.get_or_insert_with(|| RouteList::new("laravel.routes", PLUGIN_ID))
    }

    fn active_routes(&self, ctx: &Invocation) -> Vec<Route> {
        let Some(index)=self.view_index(ctx) else{return vec![];};
        let project=ctx.project();
        if let Some(snapshot)=project.as_ref().and_then(|p|p.root().ok()).and_then(|root|self.runtime_routes.get(&root)) {
            return snapshot.routes.iter().cloned().map(|mut route|{if route.location.as_ref().is_some_and(|l|!project.as_ref().is_some_and(|p|owns_current_path(p,&l.path))){route.location=None;}route}).collect();
        }
        route_list_of(index)
    }
    fn search_routes_from_index(&self, ctx: &Invocation, index:&Index) -> Vec<Route> {
        let Some(project) = ctx.project() else {return vec![]};
        if index.partial {return vec![]}
        if let Some(snapshot) = project.root().ok().and_then(|root|self.runtime_routes.get(&root)) {
            return route_runtime::endpoint_routes(snapshot,index,&project);
        }
        route_list_of(index).into_iter().zip(&index.route_defs).filter_map(|(mut route,entry)| {
            if !owns_current_path(&project,&entry.path) {return None}
            if let Some(uri) = &entry.endpoint_uri {route.path=uri.clone();}
            Some(route)
        }).collect()
    }
    fn route_at(&self, key: &str, ctx: &Invocation) -> Option<Route> {
        let number: usize = key.strip_prefix("route:")?.parse().ok()?;
        self.active_routes(ctx).into_iter().nth(number)
    }

    fn web_config(&self, ctx: &Invocation) -> String {
        let mut config: serde_json::Value = serde_json::from_str(include_str!("ui_config.json")).expect("packaged Laravel UI config");
        let runtime = self.artisan_workspace.runner(ctx).ok().and_then(|runner| self.runtime_inspection.cached(ctx, runner).ok());
        let services = self.external_services.rows(ctx);
        for action in config["actions"].as_array_mut().expect("Laravel actions") {
            let keys: Option<Vec<serde_json::Value>> = match action["id"].as_str() {
                Some("runtime-database-connect" | "runtime-database-remove") => Some(self.runtime_inspection.database_source_rows(ctx).unwrap_or_default().into_iter().filter_map(|row|row.get("key").cloned()).collect()),
                Some("runtime-open-frame") => Some(runtime.as_ref().into_iter().flat_map(|snapshot| snapshot["snapshots"].as_array().into_iter().flatten()).flat_map(|snapshot| snapshot["rows"].as_array().into_iter().flatten()).filter(|row| row.get("error").is_some()).filter_map(|row|row.get("key").cloned()).collect()),
                Some("sentry-details") => Some(services.iter().filter(|row|row["key"].as_str().is_some_and(|key|key.starts_with("sentry-issue:"))).filter_map(|row|row.get("key").cloned()).collect()),
                Some("cloud-environments") => Some(services.iter().filter(|row|row["key"].as_str().is_some_and(|key|key.starts_with("cloud-app:"))).filter_map(|row|row.get("key").cloned()).collect()),
                Some("model-open-source" | "model-create-fields") => Some(ctx.project().and_then(|project| self.view_index(ctx).map(|index| model_overview::rows(index, &project))).unwrap_or_default().into_iter().filter(|row|row["cells"][1] == "Model").filter_map(|row|row.get("key").cloned()).collect()),
                _ => None,
            };
            if let Some(keys) = keys { action["keys"] = keys.into(); }
        }
        if let Some(index) = self.view_index(ctx) {
            for tab in config["group"]["tabs"].as_array_mut().expect("Laravel tabs") {
                let count = match tab["id"].as_str().unwrap_or_default() {
                    VIEW => Some(self.active_routes(ctx).len()),
                    "laravel-components" => ctx.project().map(|project| [&index.components, &index.livewire].into_iter().flat_map(|names| names.values()).filter(|entry| !entry.path.is_empty() && owns_current_path(&project, &entry.path)).count()),
                    "laravel-views" => Some(index.views.values().filter(|entry|ctx.project().as_ref().is_some_and(|project|owns_current_path(project,&entry.path))).count()),
                    "laravel-config" => Some(index.config.len()),
                    "laravel-translations" => Some(index.translations_by_locale.values().map(Names::len).sum()),
                    "laravel-middleware" => Some(index.middleware.len()),
                    "laravel-env" => Some(index.env.len()),
                    _ => None,
                };
                if let Some(count) = count { tab["count"] = count.into(); }
            }
        }
        config.to_string()
    }

    fn view_index(&self, ctx: &Invocation) -> Option<&Index> {
        let root = ctx.project()?.root().ok()?;
        if root.is_empty() || root.chars().any(char::is_control) || root.split(['/', '\\']).any(|part|matches!(part,"."|".."))
            || !host::projects().ok()?.iter().any(|project|project.root().ok().as_deref()==Some(root.as_str())) {return None;}
        self.indexes.get(&root)
    }
}

impl Views for Main {
    fn render(&mut self, view: &str, _viewport: Option<Viewport>, ctx: &Invocation) -> Result<View, Error> {
        (|| -> Result<View, Error> {
            if view == "laravel-about" {
                let project = ctx.project().ok_or("Open a Laravel project first.")?;
                let runner = self.artisan_workspace.runner(ctx)?;
                let cached = self.runtime_inspection.cached(ctx, runner).ok();
                return Ok(View::new(about::body(&project, cached.as_ref())?)
                    .button(Button::new("runtime-about", "Refresh App Info").icon("refresh"))
                    .button(Button::new("runtime-runner", if runner == artisan_runner::Runner::Docker {"Docker"} else {"Local PHP"}).icon("terminal")));
            }
            if view == artisan_workspace::VIEW {
                return Ok(View::new(Body::Html(artisan_workspace::html())));
            }
            if view == generator_workspace::VIEW {
                return Ok(View::new(Body::Html(generator_workspace::html())));
            }
            if view == managed_processes::VIEW {
                return Ok(View::new(Body::Html(managed_processes::html())));
            }
            if view == "laravel-external" {
                let root = ctx.project().ok_or("Open a Laravel project first.")?.root()?;
                let pagination = self.external_services.pagination(ctx);
                let body = if let Some(details) = self.external_details.get(&root) {
                    Body::Message(details.clone())
                } else {
                    Body::Other { kind: "resources".into(), data: serde_json::json!({"columns":["Service","Resource","Status"],"items": self.external_services.rows(ctx),"searchPlaceholder":"Filter captured external resources…"}).to_string() }
                };
                return Ok(View::new(body)
                    .button(Button::new("sentry-configure", "Configure Sentry…").icon("settings"))
                    .button(Button::new("sentry-refresh", "Refresh Sentry").icon("refresh"))
                    .button(Button::new("cloud-refresh", "Refresh Cloud").icon("refresh"))
                    .button(Button::new("sentry-more", "More Sentry Issues").icon("page-last").enabled(pagination["sentry"] == true))
                    .button(Button::new("cloud-more", "More Cloud Applications").icon("page-last").enabled(pagination["cloud"] == true))
                    .button(Button::new("cloud-environments-more", "More Environments").icon("page-last").enabled(pagination["environments"] == true))
                    .button(Button::new("external-list", "Resource List").icon("list")));
            }
            if view == "laravel-models" {
                let project = ctx.project().ok_or("Open a Laravel project first.")?;
                let Some(index) = self.view_index(ctx) else { return Ok(View::new(Body::Message("Index this Laravel project to browse models.".into()))); };
                let schema = self.runtime_inspection.schema_snapshot(ctx, self.artisan_workspace.runner(ctx)?).ok();
                let expanded = self.model_tree_expanded.get(&project.root()?).cloned().unwrap_or_default();
                return Ok(View::new(model_overview::inventory_with_schema(index, &project, schema.as_ref(), &expanded))
                    .button(Button::new("new-model", "New Model…").icon("add"))
                    .button(Button::new("helper-parameters", "Helper Parameters…").icon("settings"))
                    .button(Button::new("regenerate-model-helpers", "Regenerate Helpers…").icon("refresh"))
                    .button(Button::new("runtime-schema", "Refresh Database Metadata").icon("database"))
                    .button(Button::new("model-from-database", "Model from Table…").icon("class"))
                    .button(Button::new("runtime-register-database", "Add Database Connection…").icon("database"))
                    .button(Button::new("runtime-database-list", "Database Connections…").icon("database"))
                    .button(Button::new("reindex", "Reindex").icon("refresh")));
            }
            if view == "laravel-runtime" {
                let runner = self.artisan_workspace.runner(ctx)?;
                return Ok(View::new(self.runtime_inspection.body(ctx,runner)?)
                    .button(Button::new("runtime-database-list","Database Connections…").icon("database"))
                    .button(Button::new("runtime-about","Application").icon("info"))
                    .button(Button::new("runtime-events","Events").icon("text"))
                    .button(Button::new("runtime-schedule","Schedule").icon("calendar"))
                    .button(Button::new("runtime-queues","Failed Jobs").icon("warning"))
                    .button(Button::new("runtime-queue-status","Queue Status").icon("services"))
                    .button(Button::new("runtime-workers","Workers").icon("services"))
                    .button(Button::new("runtime-logs","Logs").icon("text"))
                    .button(Button::new("runtime-runner",if runner==artisan_runner::Runner::Docker{"Docker"}else{"Local PHP"}).icon("terminal"))
                    .status(&self.runtime_inspection.status(ctx,runner)?));
            }
            let reindex = Button::new("reindex", "Reindex").icon("refresh");
            let group = match view {
                VIEW => "route",
                "laravel-views" => "view",
                "laravel-components" => "components",
                "laravel-config" => "config",
                "laravel-translations" => "translation",
                "laravel-middleware" => "middleware",
                "laravel-env" => "env",
                _ => return Err(Error::NotFound(view.into())),
            };
            let routes_active = group == "route";
            let Some(index) = self.view_index(ctx) else {
                return Ok(View::message("This project is not a Laravel application.").button(reindex));
            };

            let partial = index.partial;
            let config_incomplete = index.config_incomplete.len();
            let config_coverage = index.config_incomplete.iter().take(3).map(|path| path.rsplit('/').next().unwrap_or(path)).collect::<Vec<_>>().join(", ");
            let toolbar = |body| {
                let mut view = View::new(body);
                if routes_active {
                    view = view.button(Button::new("route-search", "Search Routes…").icon("search"));
                    view = view.button(Button::new("routes-runtime-refresh", "Refresh Routes (PHP)").icon("refresh"));
                    view = view.button(Button::new("route-list", "Route List").icon("terminal"));
                } else {
                    view = view.button(reindex);
                }
                let cache_action = match group {
                    "route" => Some(("route-clear", "Clear Route Cache")),
                    "view" => Some(("view-clear", "Clear Compiled Views")),
                    "config" | "env" => Some(("config-clear", "Clear Config Cache")),
                    _ => None,
                };
                if let Some((id, label)) = cache_action {
                    view = view.button(Button::new(id, label).icon("build-project"));
                }
                view
            };
            let mut view = if routes_active {
                let routes = self.active_routes(ctx);
                let list = self.route_list();
                list.set_routes(routes);
                list.render(toolbar(Body::Message(String::new())))
            } else if group == "components" {
                toolbar(component_inventory(index, &ctx.project().ok_or(Error::Stale)?))
            } else if group == "view" {
                toolbar(view_inventory(index, &ctx.project().ok_or(Error::Stale)?))
            } else if group == "config" {
                toolbar(config_inventory(index, &ctx.project().and_then(|p| p.root().ok()).unwrap_or_default()))
            } else if group == "translation" {
                toolbar(translation_inventory(index, &ctx.project().and_then(|p| p.root().ok()).unwrap_or_default()))
            } else {
                toolbar(Body::Tree(self.tree(index, group)))
            };
            if routes_active {
                let provenance=ctx.project().and_then(|p|p.root().ok()).and_then(|root|self.runtime_routes.get(&root)).map(route_runtime::Snapshot::provenance).unwrap_or_else(||"Static source index · Refresh Routes (PHP) for registered routes".into());
                view.status=Some(format!("{} · {provenance}",view.status.as_deref().unwrap_or("No routes")));
            }
            if group == "config" && config_incomplete > 0 {
                view = view.status(&format!("Config coverage incomplete: {config_coverage}{} (dynamic arrays, unreadable sources or indexing limits){}",if config_incomplete > 3 {format!(", +{} more",config_incomplete-3)} else {String::new()},if partial {"; project indexing also timed out"} else {""}));
            }
            if partial && view.status.is_none() {
                view = view.status("Index incomplete (time limit reached)");
            }
            Ok(view)
        })().map(ui_kit::actions::available)
    }

    fn handle(&mut self, view: &str, event: ViewEvent, ctx: &Invocation) -> Result<(), Error> {
        let config = self.web_config(ctx);
        if ui_kit::web::handle(self, view, &event, ctx, &config)? {
            return Ok(());
        }

        if view == managed_processes::VIEW {
            if let ViewEvent::Message(message) = event {
                let reply = match self.managed_processes.handle(ctx, &message) {
                    Ok(value) => value,
                    Err(error) => serde_json::json!({"error": error.to_string()}),
                };
                ui_kit::web::post_view_message(view, &reply.to_string(), &config)?;
            }
            return Ok(());
        }
        if view == generator_workspace::VIEW {
            if let ViewEvent::Message(message) = event {
                let result = (|| {
                    let project = ctx.project().ok_or(Error::Stale)?;
                    let root = project.root()?;
                    if !self.indexes.contains_key(&root) { self.index_project(&project); }
                    let index = self.indexes.get(&root).ok_or(Error::Stale)?;
                    self.generator_workspace.set_models(ctx,index)?;
                    self.generator_workspace.handle(ctx, &message)
                })();
                let reply = match result {
                    Ok(value) => value,
                    Err(error) => serde_json::json!({"error": error.to_string()}),
                };
                ui_kit::web::post_view_message(view, &reply.to_string(), &config)?;
            }
            return Ok(());
        }
        if view == artisan_workspace::VIEW {
            if let ViewEvent::Message(message) = event {
                let reply = match self.artisan_workspace.handle(ctx, &message, &self.artisan_terminals) {
                    Ok(value) => value,
                    Err(error) => {
                        let preview = serde_json::from_str::<serde_json::Value>(&message).ok().is_some_and(|request| request["op"] == "preview");
                        if !preview && !matches!(error, Error::Cancelled) {
                            host::notify(NotificationLevel::Error, "Artisan", &error.to_string());
                        }
                        serde_json::json!({"error":error.to_string()})
                    },
                };
                if reply["completed"] == true && let Some(project) = ctx.project() { if let Ok(root)=project.root(){self.runtime_routes.remove(&root);} self.index_project(&project); }
                ui_kit::web::post_view_message(view, &reply.to_string(), &config)?;
            }
            return Ok(());
        }

        if view == VIEW {
            match self.route_list().handle(&event) {
                RouteEvent::Changed => {
                    host::refresh_view(view);
                    return Ok(());
                }
                RouteEvent::Open(route) => {
                    if !self.active_routes(ctx).iter().any(|current| current == &route) {
                        refresh_laravel();
                        return Err(Error::Stale);
                    }
                    if let Some(location) = route.location {
                        host::open_location(&location.path, location.range.start.line, location.range.start.character)?;
                    }
                    return Ok(());
                }
                RouteEvent::Ignored => {}
            }
        }

        let model_expand = matches!(&event, ViewEvent::Expand(_));
        match event {
            ViewEvent::Expand(key) | ViewEvent::Collapse(key) if view == "laravel-models" => {
                let current = self.render(view, None, ctx)?;
                let Body::Other { data, .. } = current.body else { return Err(Error::Stale) };
                let body: serde_json::Value = serde_json::from_str(&data).map_err(|_| Error::Stale)?;
                if !body["items"].as_array().is_some_and(|items| items.iter().any(|item|item["key"] == key && item["expandable"] == true)) { return Err(Error::Stale); }
                let root = ctx.project().ok_or(Error::Stale)?.root()?;
                self.model_tree_expanded.entry(root).or_default().insert(key, model_expand);
                host::refresh_view(view);
            }
            ViewEvent::Activate(key) => {
                if view == "laravel-models" {
                    let index = self.view_index(ctx).ok_or(Error::Stale)?;
                    let project = ctx.project().ok_or(Error::Stale)?;
                    return model_overview::open(index, &project, &key);
                }
                let target = self.view_index(ctx).and_then(|index| entry_for(index, &key));
                if let Some((path, at, _)) = target.filter(|(path, _, _)| !path.is_empty() && ctx.project().as_ref().is_some_and(|project| owns_current_path(project, path))) {
                    host::open_location(path, at.line, at.character)?;
                }
            }
            ViewEvent::Button(id) => {
                self.run_command(&id, ctx, None)?;
                host::refresh_view(view);
            }
            _ => {}
        }
        Ok(())
    }
}

impl openidea_plugin::inlay_hints::InlayHints for Main {
    fn inlay_hints(&mut self, doc: &Document, range: Range) -> Vec<openidea_plugin::inlay_hints::InlayHint> {
        if !host::setting_bool("routeHints").unwrap_or(true) { return Vec::new(); }
        let Some(index) = self.index_for(&doc.path) else { return Vec::new() };
        let Some(project) = project_for(&doc.path) else { return Vec::new() };
        route_editor::hints(index, doc, range, &|path| owns_current_path(&project, path))
    }
}

impl openidea_plugin::language_features::LanguageFeatures for Main {
    fn signature_help(&mut self, doc: &Document, at: Position) -> Option<openidea_plugin::language_features::SignatureHelp> {
        let index = self.index_for(&doc.path)?;
        let project = project_for(&doc.path)?;
        eloquent_members::forwarded_signature_help_checked(index, doc, at, &|path| owns_current_path(&project, path))
    }
    fn code_lenses(&mut self, doc: &Document) -> Vec<openidea_plugin::language_features::CodeLens> {
        let Some(project) = project_for(&doc.path) else { return Vec::new() };
        let owns=|path:&str| owns_current_path(&project,path);
        let mut lenses=console_editor::lenses(doc,&owns);
        if host::setting_bool("routeHints").unwrap_or(true) && let Some(index)=self.index_for(&doc.path) {lenses.extend(route_editor::lenses(index,doc,&owns));}
        lenses
    }
    fn gutter_actions(&mut self, doc: &Document) -> Vec<openidea_plugin::language_features::GutterAction> {
        let Some(project) = project_for(&doc.path) else { return Vec::new() };
        let owns=|path:&str| owns_current_path(&project,path);
        let mut gutters=console_editor::gutters(doc,&owns);
        if let Some(index)=self.index_for(&doc.path) {gutters.extend(route_editor::gutters(index,doc,&owns));}
        gutters
    }
}

openidea_plugin::register_plugin!(Main, providers, views, indexing, endpoints, file_actions, hover, diagnostics, mcp, inlay_hints, language_features);

#[cfg(test)]
mod tests;

impl openidea_plugin::endpoints::EndpointsProvider for Main {
    fn endpoints(&mut self, ctx: &Invocation) -> Result<Vec<openidea_plugin::endpoints::Endpoint>, Error> {
        let Some(project) = ctx.project() else {
            return Ok(Vec::new());
        };
        if !index::is_laravel(&project) {return Ok(Vec::new());}
        let current = Index::build(&project);
        let index = &current;
        if index.partial {
            return Err(Error::Failed("Laravel route index is incomplete; refresh after indexing.".into()));
        }
        if let Some(snapshot) = project.root().ok().and_then(|root|self.runtime_routes.get(&root)) {
            let routes = route_runtime::endpoint_routes(snapshot,index,&project);
            return Ok(ui_kit::routes::endpoints(&routes,"Laravel"));
        }
        let mut routes = route_list_of(index);
        for (route, entry) in routes.iter_mut().zip(&index.route_defs) {
            route.method = entry.def.methods.clone();
        }
        let mut endpoints = Vec::new();
        for (mut route, entry) in routes.into_iter().zip(&index.route_defs) {
            let Some(uri) = &entry.endpoint_uri else { continue };
            route.path = uri.clone();
            let mut published = ui_kit::routes::endpoints(&[route], "Laravel");
            for endpoint in &mut published {
                endpoint.handler = Some(match &entry.def.controller {
                    Some((class, method)) => format!("{class}@{method}"),
                    None => entry.def.action.clone(),
                });
            }
            endpoints.extend(published);
        }
        Ok(endpoints)
    }
}
