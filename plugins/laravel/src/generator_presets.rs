//! Native, non-executable generation presets. `${name|transform}` substitutes a
//! declared parameter; `$${` emits a literal `${`. No PHP or shell is evaluated.
#[path = "generator_date.rs"]
mod date;
use crate::generator_plan::{self, Draft, Output, Plan};
use openidea_plugin::{Error, Project, host};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const CONFIG: &str = ".openidea/laravel-generators.json";
const TEMPLATE_ROOT: &str = ".openidea/laravel-templates/";
const LIMIT: usize = 128 << 10;
const USER_KEY: &str = "laravel.generators.user.v1";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Config {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    schema: Option<String>,
    version: u32,
    #[serde(default)]
    presets: Vec<Preset>,
    #[serde(default)]
    partials: Vec<String>,
    #[serde(default)]
    generators: BTreeMap<String, GeneratorOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    application: Option<Application>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    allow_application: Option<bool>,
    #[serde(default)]
    modules: Vec<Module>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    settings: Option<generator_plan::GenerationSettings>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            schema: None,
            version: 1,
            presets: Vec::new(),
            partials: Vec::new(),
            generators: BTreeMap::new(),
            application: None,
            allow_application: None,
            modules: Vec::new(),
            settings: None,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Preset {
    id: String,
    label: String,
    #[serde(default)]
    parameters: Vec<Parameter>,
    #[serde(default)]
    calculated: Vec<Calculated>,
    files: Vec<TemplateFile>,
    #[serde(default = "editable_default")]
    available_in_application: bool,
    #[serde(default = "editable_default")]
    available_in_modules: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    module_ids: Option<Vec<String>>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Parameter {
    name: String,
    label: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    choices: Vec<String>,
    #[serde(default)]
    default: Option<Value>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Calculated {
    name: String,
    expression: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(default)]
    hidden: bool,
    #[serde(default = "editable_default")]
    editable: bool,
}
fn editable_default() -> bool {
    true
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TemplateFile {
    path: String,
    template: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Module {
    id: String,
    label: String,
    namespace: String,
    source_root: String,
    migration_root: String,
    view_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    factory_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    factory_namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seeder_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seeder_namespace: Option<String>,
    #[serde(default)]
    generators: BTreeMap<String, GeneratorOptions>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
struct Application {
    namespace: String,
    source_root: String,
    migration_root: String,
    view_root: String,
    test_root: String,
    test_namespace: String,
}
impl Default for Application {
    fn default() -> Self {
        Self {
            namespace: "App".into(),
            source_root: "app".into(),
            migration_root: "database/migrations".into(),
            view_root: "resources/views".into(),
            test_root: "tests".into(),
            test_namespace: "Tests".into(),
        }
    }
}
fn generation_kinds() -> BTreeSet<String> {
    generator_plan::KINDS
        .iter()
        .map(|kind| (*kind).into())
        .chain(
            crate::native_catalog::kind_ids()
                .into_iter()
                .map(str::to_owned),
        )
        .chain(
            [
                "nova-resource",
                "filament-resource",
                "filament-schema",
                "filament-table",
                "filament-relation-manager",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .collect()
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneratorOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    directory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    available_in_application: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    available_in_modules: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    base_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    postfix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label_mode: Option<crate::package_generators::LabelMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    separate: Option<bool>,
}
fn validate_generators(options: &BTreeMap<String, GeneratorOptions>) -> Result<(), String> {
    for (kind, option) in options {
        if !generation_kinds().contains(kind) {
            return Err(format!("Unknown native generator kind {kind}"));
        }
        if option
            .base_class
            .as_ref()
            .is_some_and(|base| !generator_plan::qualified(base) || base.starts_with('\\'))
            || option
                .postfix
                .as_ref()
                .is_some_and(|postfix| !generator_plan::identifier(&format!("Generated{postfix}")))
            || option
                .template
                .as_ref()
                .is_some_and(|template| !generator_plan::relative(template))
        {
            return Err("Generator base class, postfix or template is invalid".into());
        }
        if (option.label_mode.is_some()
            || option.label_prefix.is_some()
            || option.separate.is_some())
            && !matches!(
                kind.as_str(),
                "nova-resource"
                    | "filament-resource"
                    | "filament-schema"
                    | "filament-table"
                    | "filament-relation-manager"
            )
        {
            return Err("Label/schema options apply only to Nova and Filament generators".into());
        }
        if option.separate.is_some() && kind != "filament-resource" {
            return Err(
                "Separate schema/table settings apply only to Filament resource generation".into(),
            );
        }
        if option.base_class.is_some()
            && matches!(kind.as_str(), "filament-schema" | "filament-table")
        {
            return Err(
                "Standalone Filament schema/table helpers do not extend a resource base class"
                    .into(),
            );
        }
        if option
            .label_prefix
            .as_ref()
            .is_some_and(|prefix| prefix.len() > 128 || prefix.chars().any(char::is_control))
        {
            return Err("Package field label prefix is invalid".into());
        }
        if let Some(directory) = &option.directory {
            let sample = directory
                .replace("${sourceRoot}", "app")
                .replace("${moduleRoot}", "modules/Blog");
            if directory.len() > 4096 || !generator_plan::relative(&sample) {
                return Err("Generator directories require safe relative paths with optional sourceRoot/moduleRoot placeholders".into());
            }
        }
        if let Some(namespace) = &option.namespace {
            if kind == "migration"
                || namespace.len() > 4096
                || !generator_plan::qualified(&namespace.replace("${namespace}", "App"))
                || namespace.starts_with('\\')
            {
                return Err("Generator namespaces require a qualified class namespace with optional namespace placeholder".into());
            }
        }
    }
    Ok(())
}
#[derive(Clone, Debug)]
pub(crate) struct Bundle {
    config: Config,
    templates: BTreeMap<String, String>,
    evidence: String,
    current_time: Option<String>,
}

fn variable(name: &str) -> bool {
    name.len() <= 96 && generator_plan::identifier(name)
}
fn label(label: &str) -> bool {
    !label.trim().is_empty() && label.len() <= 200 && !label.chars().any(char::is_control)
}
fn value_ok(parameter: &Parameter, value: &Value) -> bool {
    match parameter.kind.as_str() {
        "boolean" => value.is_boolean(),
        "text" => value
            .as_str()
            .is_some_and(|s| s.len() <= 4096 && !s.contains('\0')),
        "model" => value.as_str().is_some_and(generator_plan::qualified),
        "choice" => value
            .as_str()
            .is_some_and(|s| parameter.choices.iter().any(|choice| choice == s)),
        _ => false,
    }
}

pub(crate) fn parse(text: &str) -> Result<Config, String> {
    let json = crate::strict_json::parse(text, LIMIT).map_err(|e| e.to_string())?;
    if json
        .get("$schema")
        .is_some_and(|schema| !schema.is_string())
    {
        return Err("Schema references must be strings when supplied".into());
    }
    if json
        .get("presets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|preset| {
            preset
                .get("parameters")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|parameter| parameter.get("default").is_some_and(Value::is_null))
        })
    {
        return Err(
            "Parameter defaults must match their input type; null is not an input value".into(),
        );
    }
    let config: Config = serde_json::from_value(json).map_err(|e| e.to_string())?;
    if let Some(settings) = &config.settings {
        settings.validate()?;
    }
    if config.version != 1 {
        return Err("Unsupported generator config version; expected version 1".into());
    }
    if config
        .schema
        .as_ref()
        .is_some_and(|schema| schema.len() > 4096 || schema.chars().any(char::is_control))
    {
        return Err("Schema references must be bounded strings without control characters".into());
    }
    if config.presets.len() > 64 || config.modules.len() > 64 {
        return Err("At most 64 presets and 64 modules are supported".into());
    }
    if config.partials.len() > 64
        || config
            .partials
            .iter()
            .any(|name| !generator_plan::relative(name))
        || config.partials.iter().collect::<BTreeSet<_>>().len() != config.partials.len()
    {
        return Err("Shared templates need at most 64 unique relative names".into());
    }
    if let Some(application) = &config.application {
        if !generator_plan::qualified(&application.namespace)
            || application.namespace.starts_with('\\')
            || !generator_plan::qualified(&application.test_namespace)
            || application.test_namespace.starts_with('\\')
            || [
                &application.source_root,
                &application.migration_root,
                &application.view_root,
                &application.test_root,
            ]
            .into_iter()
            .any(|path| !generator_plan::relative(path))
        {
            return Err(
                "Application namespaces and directories must be explicit qualified/relative names"
                    .into(),
            );
        }
    }
    validate_generators(&config.generators)?;
    let mut ids = BTreeSet::new();
    for preset in &config.presets {
        if !variable(&preset.id) || !label(&preset.label) || !ids.insert(&preset.id) {
            return Err("Preset ids must be unique identifiers with a nonempty label".into());
        }
        if preset.module_ids.as_ref().is_some_and(|ids| {
            ids.len() > 64
                || ids.iter().any(|id| !variable(id))
                || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
        }) {
            return Err("Preset moduleIds must contain at most 64 unique module ids".into());
        }
        if preset.parameters.len() > 64
            || preset.calculated.len() > 64
            || preset.files.is_empty()
            || preset.files.len() > 64
        {
            return Err("A preset needs 1–64 files and at most 64 inputs/calculations".into());
        }
        let mut names = BTreeSet::new();
        for parameter in &preset.parameters {
            if (parameter.name == "CURRENT_TIME" || !variable(&parameter.name))
                || !label(&parameter.label)
                || !names.insert(&parameter.name)
                || !["text", "boolean", "model", "choice"].contains(&parameter.kind.as_str())
                || parameter.kind == "choice"
                    && (parameter.choices.is_empty()
                        || parameter.choices.len() > 128
                        || parameter.choices.iter().collect::<BTreeSet<_>>().len()
                            != parameter.choices.len())
                || parameter.kind != "choice" && !parameter.choices.is_empty()
                || parameter
                    .choices
                    .iter()
                    .any(|choice| choice.len() > 4096 || choice.contains('\0'))
                || parameter
                    .default
                    .as_ref()
                    .is_some_and(|value| !value_ok(parameter, value))
            {
                return Err(format!("Invalid input declaration {}", parameter.name));
            }
        }
        for calc in &preset.calculated {
            if (calc.name == "CURRENT_TIME" || !variable(&calc.name))
                || !names.insert(&calc.name)
                || calc.expression.len() > 4096
                || !calc.expression.starts_with("${")
                || !calc.expression.ends_with('}')
                || calc.expression[2..calc.expression.len() - 1].contains(['{', '}'])
                || calc.label.as_ref().is_some_and(|value| !label(value))
            {
                return Err(
                    "Calculated inputs need a unique name and exactly one substitution expression"
                        .into(),
                );
            }
        }
        for file in &preset.files {
            if !generator_plan::relative(&file.template)
                || file.path.is_empty()
                || file.path.len() > 4096
            {
                return Err(
                    "Template names must be relative paths and output paths must be bounded".into(),
                );
            }
        }
    }
    let mut modules = BTreeSet::new();
    for module in &config.modules {
        validate_generators(&module.generators)?;
        if module
            .generators
            .values()
            .any(|option| option.available_in_application.is_some())
        {
            return Err("Module generator overrides cannot set Application availability".into());
        }
        if module
            .factory_root
            .as_ref()
            .into_iter()
            .chain(module.seeder_root.as_ref())
            .any(|root| !generator_plan::relative(root))
            || module
                .factory_namespace
                .as_ref()
                .into_iter()
                .chain(module.seeder_namespace.as_ref())
                .any(|namespace| {
                    !generator_plan::qualified(namespace) || namespace.starts_with('\\')
                })
        {
            return Err("Invalid module factory/seeder layout".into());
        }
        if !variable(&module.id)
            || !modules.insert(&module.id)
            || !label(&module.label)
            || !generator_plan::qualified(&module.namespace)
            || [
                &module.source_root,
                &module.migration_root,
                &module.view_root,
            ]
            .iter()
            .any(|root| !generator_plan::relative(root))
        {
            return Err("Modules require unique identifiers, an explicit namespace and relative source/migration/view roots".into());
        }
    }
    Ok(config)
}

fn owned(project: &Project, root: &str, relative: &str) -> Result<(), Error> {
    let path = format!("{}/{relative}", root.trim_end_matches('/'));
    if host::project_of(&path)
        .and_then(|owner| owner.root().ok())
        .as_deref()
        != Some(root)
    {
        return Err(Error::Stale);
    }
    // Ownership is checked independently of whether the target currently exists.
    if project.root()?.as_str() != root {
        return Err(Error::Stale);
    }
    Ok(())
}

fn load_project(project: &Project) -> Result<Bundle, Error> {
    let root = project.root()?;
    owned(project, &root, CONFIG)?;
    let text = match project.read_file(CONFIG) {
        Ok(text) => text,
        Err(Error::NotFound(_)) => {
            return Ok(Bundle {
                config: Config::default(),
                templates: BTreeMap::new(),
                evidence: String::new(),
                current_time: None,
            });
        }
        Err(error) => return Err(error),
    };
    let config = parse(&text).map_err(Error::Invalid)?;
    let mut templates = BTreeMap::new();
    let mut total = text.len();
    for name in template_names(&config) {
        let relative = format!("{TEMPLATE_ROOT}{name}");
        owned(project, &root, &relative)?;
        let body = project.read_file(&relative)?;
        total += body.len();
        if body.len() > LIMIT || total > 2 << 20 {
            return Err(Error::Invalid(
                "Preset templates exceed the 128 KiB/file or 2 MiB bundle limit".into(),
            ));
        }
        templates.insert(name, body);
    }
    // Exact serialized evidence, not a lossy hash: callers can compare it around
    // a review without overlooking template edits that preserve rendered output.
    let evidence =
        serde_json::to_string(&(text, &templates)).map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(Bundle {
        config,
        templates,
        evidence,
        current_time: None,
    })
}

pub(crate) fn template_names(config: &Config) -> BTreeSet<String> {
    config
        .presets
        .iter()
        .flat_map(|preset| preset.files.iter().map(|file| file.template.clone()))
        .chain(config.partials.iter().cloned())
        .chain(
            config
                .generators
                .values()
                .filter_map(|option| option.template.clone()),
        )
        .chain(config.modules.iter().flat_map(|module| {
            module
                .generators
                .values()
                .filter_map(|option| option.template.clone())
        }))
        .collect()
}

/// Validate private/user definitions using exactly the same native format as
/// project definitions. Template contents remain inert until explicitly used.
pub(crate) fn from_parts(
    config: Value,
    templates: BTreeMap<String, String>,
) -> Result<Bundle, Error> {
    let text = serde_json::to_string(&config).map_err(|e| Error::Invalid(e.to_string()))?;
    let config = parse(&text).map_err(Error::Invalid)?;
    if templates.len() > 256
        || templates.iter().any(|(name, body)| {
            !generator_plan::relative(name) || body.len() > LIMIT || body.contains('\0')
        })
        || text.len() + templates.values().map(String::len).sum::<usize>() > 2 << 20
    {
        return Err(Error::Invalid(
            "User templates must be bounded relative names without NUL (128 KiB/file, 2 MiB total)"
                .into(),
        ));
    }
    if template_names(&config)
        .iter()
        .any(|name| !templates.contains_key(name))
    {
        return Err(Error::Invalid(
            "A generator refers to a missing template".into(),
        ));
    }
    let evidence =
        serde_json::to_string(&(&text, &templates)).map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(Bundle {
        config,
        templates,
        evidence,
        current_time: None,
    })
}

pub(crate) fn user_document() -> Result<Value, Error> {
    match host::storage::get(USER_KEY)? {
        None => Ok(json!({"config":{"version":1,"presets":[],"modules":[]},"templates":{}})),
        Some(text) => {
            let value = crate::strict_json::parse(&text, 3 << 20)?;
            let config = value.get("config").cloned().ok_or(Error::Stale)?;
            let templates: BTreeMap<String, String> =
                serde_json::from_value(value.get("templates").cloned().ok_or(Error::Stale)?)
                    .map_err(|_| Error::Stale)?;
            from_parts(config, templates)?;
            Ok(value)
        }
    }
}

/// User-level presets are private plugin storage, not project writes. The exact
/// old/new document is reviewed; a concurrent settings change aborts the save.
pub(crate) fn save_user(
    config: Value,
    templates: BTreeMap<String, String>,
) -> Result<Value, Error> {
    let bundle = from_parts(config, templates)?;
    let next = json!({"config":bundle.config,"templates":bundle.templates});
    let previous = host::storage::get(USER_KEY)?;
    let empty = crate::ui_text::t("generator.user.noPresets", "(no user presets)");
    let before = previous.as_deref().unwrap_or(&empty);
    let after = serde_json::to_string_pretty(&next).map_err(|e| Error::Invalid(e.to_string()))?;
    if !ui_kit::prompts::confirm_exact(
        &crate::ui_text::t("generator.user.confirm", "Save user Laravel generators?"),
        &crate::ui_text::format(
            "generator.user.review",
            "These presets apply across projects. Project definitions with the same id take precedence.\n\nPrevious user definitions:\n{before}\n\nReplacement user definitions:\n{after}\n\nSave in private plugin storage?",
            &[("before", before), ("after", &after)],
        ),
    )? {
        return Err(Error::Cancelled);
    }
    if host::storage::get(USER_KEY)? != previous {
        return Err(Error::Stale);
    }
    host::storage::set(USER_KEY, &next.to_string())?;
    Ok(json!({"saved":true,"scope":"user"}))
}

/// Project presets take precedence by id; user template keys are internally
/// namespaced so shared relative filenames cannot change project definitions.
pub(crate) fn load(project: &Project) -> Result<Bundle, Error> {
    let mut project = load_project(project)?;
    let value = match user_document() {
        Ok(value) => value,
        Err(Error::Denied(_)) => return Ok(project),
        Err(error) => return Err(error),
    };
    let mut user = from_parts(
        value["config"].clone(),
        serde_json::from_value(value["templates"].clone()).map_err(|_| Error::Stale)?,
    )?;
    for option in user.config.generators.values_mut().chain(
        user.config
            .modules
            .iter_mut()
            .flat_map(|module| module.generators.values_mut()),
    ) {
        if let Some(template) = &mut option.template {
            *template = format!("@user/{template}");
        }
    }
    if project.config.application.is_none() {
        project.config.application = user.config.application.take();
    }
    if project.config.allow_application.is_none() {
        project.config.allow_application = user.config.allow_application;
    }
    for (kind, option) in &user.config.generators {
        project
            .config
            .generators
            .entry(kind.clone())
            .or_insert_with(|| option.clone());
    }
    if project.config.settings.is_none() {
        project.config.settings = user.config.settings.take();
    }
    project.evidence =
        serde_json::to_string(&(&project.evidence, &user.evidence)).map_err(|_| Error::Stale)?;
    let project_ids: BTreeSet<_> = project
        .config
        .presets
        .iter()
        .map(|preset| preset.id.clone())
        .collect();
    for mut preset in user
        .config
        .presets
        .drain(..)
        .filter(|preset| !project_ids.contains(&preset.id))
    {
        for file in &mut preset.files {
            file.template = format!("@user/{}", file.template);
        }
        project.config.presets.push(preset);
    }
    let project_modules: BTreeSet<_> = project
        .config
        .modules
        .iter()
        .map(|module| module.id.clone())
        .collect();
    project.config.modules.extend(
        user.config
            .modules
            .into_iter()
            .filter(|module| !project_modules.contains(&module.id)),
    );
    project.config.partials.extend(
        user.config
            .partials
            .into_iter()
            .map(|name| format!("@user/{name}")),
    );
    project.templates.extend(
        user.templates
            .into_iter()
            .map(|(name, body)| (format!("@user/{name}"), body)),
    );
    Ok(project)
}

pub(crate) fn capture_current_time() -> Result<String, String> {
    date::captured_now()
}
impl Bundle {
    pub(crate) fn with_current_time(mut self, current_time: &str) -> Result<Self, String> {
        date::validate(current_time)?;
        self.current_time = Some(current_time.into());
        Ok(self)
    }

    pub(crate) fn migration_root_for_source(&self, path: &str) -> String {
        self.config
            .modules
            .iter()
            .filter(|module| path.starts_with(&format!("{}/", module.source_root)))
            .max_by_key(|module| module.source_root.len())
            .map(|module| module.migration_root.clone())
            .unwrap_or_else(|| "database/migrations".into())
    }
    pub(crate) fn apply_settings(&self, draft: &mut Draft) {
        if draft.use_project_settings {
            if let Some(settings) = &self.config.settings {
                draft.settings = settings.clone();
            } else {
                draft.settings.controller_base = format!(
                    "{}\\Http\\Controllers\\Controller",
                    self.config
                        .application
                        .clone()
                        .unwrap_or_default()
                        .namespace
                );
            }
        }
    }
    pub(crate) fn fingerprint(&self) -> String {
        format!(
            "{}\nclock:{}",
            self.evidence,
            self.current_time.as_deref().unwrap_or("")
        )
    }
    pub(crate) fn catalog(&self) -> Value {
        let all = generation_kinds();
        let application: BTreeMap<_, _> = all
            .iter()
            .map(|kind| (kind, self.available(kind, None)))
            .collect();
        let modules: BTreeMap<_, _> = self
            .config
            .modules
            .iter()
            .map(|module| {
                (
                    module.id.clone(),
                    all.iter()
                        .map(|kind| (kind, self.available(kind, Some(&module.id))))
                        .collect::<BTreeMap<_, _>>(),
                )
            })
            .collect();
        let package_kinds = [
            "nova-resource",
            "filament-resource",
            "filament-schema",
            "filament-table",
            "filament-relation-manager",
        ];
        let package_application = package_kinds
            .iter()
            .filter_map(|kind| {
                self.package_defaults(kind, None)
                    .ok()
                    .map(|defaults| ((*kind).to_string(), defaults))
            })
            .collect::<BTreeMap<_, _>>();
        let package_modules = self
            .config
            .modules
            .iter()
            .map(|module| {
                (
                    module.id.clone(),
                    package_kinds
                        .iter()
                        .filter_map(|kind| {
                            self.package_defaults(kind, Some(&module.id))
                                .ok()
                                .map(|defaults| ((*kind).to_string(), defaults))
                        })
                        .collect::<BTreeMap<_, _>>(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        json!({"version":1,"presets":self.config.presets,"modules":self.config.modules,"settings":self.config.settings,"generators":self.config.generators,"application":self.config.application.clone().unwrap_or_default(),"nativeCatalog":crate::native_catalog::catalog(),"packageDefaults":{"application":package_application,"modules":package_modules},"availability":{"application":application,"modules":modules}})
    }
    pub(crate) fn available(&self, kind: &str, module_id: Option<&str>) -> bool {
        if module_id.is_none() && self.config.allow_application == Some(false) {
            return false;
        }
        let base = self.config.generators.get(kind);
        match module_id {
            None => base
                .and_then(|option| option.available_in_application)
                .unwrap_or(true),
            Some(id) => self
                .config
                .modules
                .iter()
                .find(|module| module.id == id)
                .is_some_and(|module| {
                    module
                        .generators
                        .get(kind)
                        .and_then(|option| option.available_in_modules)
                        .or_else(|| base.and_then(|option| option.available_in_modules))
                        .unwrap_or(true)
                }),
        }
    }
    pub(crate) fn package_defaults(
        &self,
        kind: &str,
        module_id: Option<&str>,
    ) -> Result<Value, String> {
        let module = module_id
            .map(|id| {
                self.config
                    .modules
                    .iter()
                    .find(|module| module.id == id)
                    .ok_or("Unknown module")
            })
            .transpose()?;
        let base = self.config.generators.get(kind);
        let local = module.and_then(|module| module.generators.get(kind));
        Ok(
            json!({"labelMode":local.and_then(|option|option.label_mode.as_ref()).or_else(||base.and_then(|option|option.label_mode.as_ref())).cloned().unwrap_or_default(),"labelPrefix":local.and_then(|option|option.label_prefix.as_ref()).or_else(||base.and_then(|option|option.label_prefix.as_ref())).cloned().unwrap_or_else(||"fields.".into()),"separate":local.and_then(|option|option.separate).or_else(||base.and_then(|option|option.separate)).unwrap_or(true)}),
        )
    }
    pub(crate) fn module_ids(&self) -> Vec<String> {
        self.config.modules.iter().map(|module| module.id.clone()).collect()
    }
    pub(crate) fn native_settings(
        &self,
        module_id: Option<&str>,
    ) -> Result<crate::native_catalog::Settings, String> {
        let app = self.config.application.clone().unwrap_or_default();
        let mut settings = crate::native_catalog::Settings::default();
        settings.namespace = app.namespace.clone();
        settings.application_namespace = app.namespace;
        settings.source_root = app.source_root;
        settings.migration_root = app.migration_root;
        settings.application_view_root = app.view_root.clone();
        settings.view_root = app.view_root;
        settings.test_root = app.test_root;
        settings.test_case = format!("{}\\TestCase", app.test_namespace);
        settings.test_namespace = app.test_namespace;
        let module = module_id
            .map(|id| {
                self.config
                    .modules
                    .iter()
                    .find(|module| module.id == id)
                    .ok_or("Unknown module")
            })
            .transpose()?;
        if let Some(module) = module {
            settings.namespace = module.namespace.clone();
            settings.source_root = module.source_root.clone();
            settings.migration_root = module.migration_root.clone();
            settings.view_root = module.view_root.clone();
            let base = module
                .source_root
                .strip_suffix("/app")
                .unwrap_or(&module.source_root);
            settings.test_root = format!("{base}/tests");
            settings.test_namespace = format!("{}\\Tests", module.namespace);
        }
        if let Some(saved) = &self.config.settings {
            settings.type_hints = saved.type_hints;
            settings.constructor_promotion = saved.constructor_promotion;
        }
        for kind in generation_kinds() {
            let base = self.config.generators.get(&kind);
            let local = module.and_then(|module| module.generators.get(&kind));
            let pick = |get: fn(&GeneratorOptions) -> &Option<String>| {
                local
                    .and_then(|option| get(option).clone())
                    .or_else(|| base.and_then(|option| get(option).clone()))
            };
            let root = module
                .map(|module| {
                    module
                        .source_root
                        .strip_suffix("/app")
                        .unwrap_or(&module.source_root)
                })
                .unwrap_or("");
            let directory = pick(|option| &option.directory).map(|value| {
                value
                    .replace("${sourceRoot}", &settings.source_root)
                    .replace("${moduleRoot}", root)
            });
            let namespace = pick(|option| &option.namespace)
                .map(|value| value.replace("${namespace}", &settings.namespace));
            settings.kinds.insert(
                kind,
                crate::native_catalog::KindSettings {
                    directory,
                    namespace,
                    base_class: pick(|option| &option.base_class),
                    postfix: pick(|option| &option.postfix),
                    template: pick(|option| &option.template),
                },
            );
        }
        if let Some(module) = module {
            let root = module
                .source_root
                .strip_suffix("/app")
                .unwrap_or(&module.source_root);
            for (kind, folder, suffix) in [
                ("factory", "factories", "Factories"),
                ("seeder", "seeders", "Seeders"),
            ] {
                let options = settings.kinds.entry(kind.into()).or_default();
                if options.directory.is_none() {
                    options.directory = if kind == "factory" {
                        module.factory_root.clone()
                    } else {
                        module.seeder_root.clone()
                    }
                    .or_else(|| Some(format!("{root}/database/{folder}")));
                }
                if options.namespace.is_none() {
                    options.namespace = if kind == "factory" {
                        module.factory_namespace.clone()
                    } else {
                        module.seeder_namespace.clone()
                    }
                    .or_else(|| Some(format!("{}\\Database\\{suffix}", module.namespace)));
                }
            }
        }
        let mut defaults = self.config.settings.clone().unwrap_or_default();
        if self.config.settings.is_none() {
            defaults.controller_base = format!(
                "{}\\Http\\Controllers\\Controller",
                settings.application_namespace
            );
        }
        for (kind, base, postfix) in [
            ("model", Some(defaults.model_base), defaults.model_postfix),
            (
                "controller",
                Some(defaults.controller_base.clone()),
                defaults.controller_postfix.clone(),
            ),
            (
                "resource-controller",
                Some(defaults.controller_base),
                defaults.controller_postfix,
            ),
            ("policy", defaults.policy_base, defaults.policy_postfix),
            (
                "request",
                Some(defaults.request_base),
                defaults.request_postfix,
            ),
            (
                "resource",
                Some(defaults.resource_base),
                defaults.resource_postfix,
            ),
            (
                "factory",
                Some(defaults.factory_base),
                defaults.factory_postfix,
            ),
            (
                "seeder",
                Some(defaults.seeder_base),
                defaults.seeder_postfix,
            ),
        ] {
            let option = settings.kinds.entry(kind.into()).or_default();
            let stock = match kind {
                "model" => Some("Illuminate\\Database\\Eloquent\\Model".to_string()),
                "controller" | "resource-controller" => Some(format!(
                    "{}\\Http\\Controllers\\Controller",
                    settings.application_namespace
                )),
                "request" => Some("Illuminate\\Foundation\\Http\\FormRequest".into()),
                "resource" => Some("Illuminate\\Http\\Resources\\Json\\JsonResource".into()),
                "factory" => Some("Illuminate\\Database\\Eloquent\\Factories\\Factory".into()),
                "seeder" => Some("Illuminate\\Database\\Seeder".into()),
                _ => None,
            };
            if option.base_class.is_none() && base != stock {
                option.base_class = base;
            }
            if option.postfix.is_none() {
                option.postfix = Some(postfix);
            }
        }
        settings.templates = self.templates.clone();
        Ok(settings)
    }
    pub(crate) fn render_package_output(
        &self,
        kind: &str,
        module_id: Option<&str>,
        generated_class: &str,
        model: &str,
        table: &str,
        generated: &str,
        type_hints: bool,
    ) -> Result<String, String> {
        if !generator_plan::qualified(generated_class) || !generator_plan::qualified(model) {
            return Err("Generated/model identities must be qualified classes".into());
        }
        let module = module_id
            .map(|id| {
                self.config
                    .modules
                    .iter()
                    .find(|module| module.id == id)
                    .ok_or("Unknown module")
            })
            .transpose()?;
        let template = module
            .and_then(|module| module.generators.get(kind))
            .and_then(|options| options.template.as_ref())
            .or_else(|| {
                self.config
                    .generators
                    .get(kind)
                    .and_then(|options| options.template.as_ref())
            });
        let Some(template) = template else {
            return Ok(generated.into());
        };
        let expanded = expand_template(
            template,
            &self.templates,
            &template_names(&self.config),
            &BTreeMap::from([("typeHints".into(), Value::Bool(type_hints))]),
            &mut Vec::new(),
            &mut ExpansionBudget::default(),
        )?;
        if table.is_empty() && expanded.contains("${table}") {
            return Err(
                "A package template table slot requires a captured or indexed table name".into(),
            );
        }
        let (namespace, class) = generated_class
            .trim_start_matches('\\')
            .rsplit_once('\\')
            .ok_or("Generated class requires namespace")?;
        generator_plan::render_generated_template(
            &expanded,
            generated,
            namespace,
            class,
            model.trim_start_matches('\\'),
            table,
        )
    }
    pub(crate) fn render_related_output(
        &self,
        draft: &Draft,
        kind: &str,
        module_id: Option<&str>,
        generated_class: &str,
        model: &str,
        generated: &str,
    ) -> Result<String, String> {
        if !generator_plan::qualified(generated_class) || !generator_plan::qualified(model) {
            return Err("Generated/model identities must be qualified classes".into());
        }
        let mut context = draft.clone();
        context.related = [kind.into()].into_iter().collect();
        let templates = self.generated_templates(&context, module_id)?;
        let Some(template) = templates.get(kind) else {
            return Ok(generated.to_string());
        };
        let (namespace, class) = generated_class
            .trim_start_matches('\\')
            .rsplit_once('\\')
            .ok_or("Generated class requires namespace")?;
        generator_plan::render_generated_template(
            template,
            generated,
            namespace,
            class,
            model.trim_start_matches('\\'),
            &draft.table,
        )
    }
    pub(crate) fn apply_application(&self, draft: &mut Draft) {
        let application = self.config.application.clone().unwrap_or_default();
        draft.namespace = application.namespace;
        draft.source_root = application.source_root;
        draft.migration_root = application.migration_root;
    }
    pub(crate) fn generated_templates(
        &self,
        draft: &Draft,
        module_id: Option<&str>,
    ) -> Result<BTreeMap<String, String>, String> {
        let module = module_id
            .map(|id| {
                self.config
                    .modules
                    .iter()
                    .find(|module| module.id == id)
                    .ok_or("Unknown module")
            })
            .transpose()?;
        let values = BTreeMap::from([
            ("timestamps".into(), Value::Bool(draft.timestamps)),
            ("softDeletes".into(), Value::Bool(draft.soft_deletes)),
            ("typeHints".into(), Value::Bool(draft.settings.type_hints)),
        ]);
        let registered = template_names(&self.config);
        let mut budget = ExpansionBudget::default();
        let mut result = BTreeMap::new();
        for kind in std::iter::once("model").chain(draft.related.iter().map(String::as_str)) {
            let name = module
                .and_then(|module| module.generators.get(kind))
                .and_then(|option| option.template.as_ref())
                .or_else(|| {
                    self.config
                        .generators
                        .get(kind)
                        .and_then(|option| option.template.as_ref())
                });
            if let Some(name) = name {
                result.insert(
                    kind.into(),
                    expand_template(
                        name,
                        &self.templates,
                        &registered,
                        &values,
                        &mut Vec::new(),
                        &mut budget,
                    )?,
                );
            }
        }
        Ok(result)
    }
    pub(crate) fn apply_generator_options(
        &self,
        draft: &mut Draft,
        module_id: Option<&str>,
    ) -> Result<(), String> {
        let selected = std::iter::once("model".to_string())
            .chain(draft.related.iter().cloned())
            .collect::<BTreeSet<_>>();
        self.apply_selected_options(draft, module_id, &selected)
    }
    /// Existing-model actions generate one related class; model availability
    /// does not gate them because the model already exists.
    pub(crate) fn apply_related_generator_options(
        &self,
        draft: &mut Draft,
        kind: &str,
        module_id: Option<&str>,
    ) -> Result<(), String> {
        if !matches!(kind, "factory" | "resource") {
            return Err("Unsupported related generator".into());
        }
        self.apply_selected_options(draft, module_id, &[kind.to_owned()].into_iter().collect())
    }
    fn apply_selected_options(
        &self,
        draft: &mut Draft,
        module_id: Option<&str>,
        selected: &BTreeSet<String>,
    ) -> Result<(), String> {
        let module = module_id
            .map(|id| {
                self.config
                    .modules
                    .iter()
                    .find(|module| module.id == id)
                    .ok_or("Unknown module")
            })
            .transpose()?;
        for kind in selected {
            if !self.available(kind, module_id) {
                return Err(format!(
                    "Generator {kind} is unavailable in the selected application/module"
                ));
            }
        }
        for kind in generator_plan::KINDS
            .iter()
            .filter(|kind| selected.contains(**kind))
        {
            let base = self.config.generators.get(*kind);
            let local = module.and_then(|module| module.generators.get(*kind));
            let directory = local
                .and_then(|option| option.directory.as_ref())
                .or_else(|| base.and_then(|option| option.directory.as_ref()));
            let namespace = local
                .and_then(|option| option.namespace.as_ref())
                .or_else(|| base.and_then(|option| option.namespace.as_ref()));
            let selected_base = local
                .and_then(|option| option.base_class.as_ref())
                .or_else(|| base.and_then(|option| option.base_class.as_ref()));
            let selected_postfix = local
                .and_then(|option| option.postfix.as_ref())
                .or_else(|| base.and_then(|option| option.postfix.as_ref()));
            if let Some(value) = selected_base {
                match *kind {
                    "model" => draft.settings.model_base = value.clone(),
                    "controller" => draft.settings.controller_base = value.clone(),
                    "policy" => draft.settings.policy_base = Some(value.clone()),
                    "request" => draft.settings.request_base = value.clone(),
                    "resource" => draft.settings.resource_base = value.clone(),
                    "factory" => draft.settings.factory_base = value.clone(),
                    "seeder" => draft.settings.seeder_base = value.clone(),
                    "migration" => {
                        return Err(
                            "Anonymous migration bases are provided by the migration template"
                                .into(),
                        );
                    }
                    _ => {}
                }
            }
            if let Some(value) = selected_postfix {
                match *kind {
                    "model" => draft.settings.model_postfix = value.clone(),
                    "controller" => draft.settings.controller_postfix = value.clone(),
                    "policy" => draft.settings.policy_postfix = value.clone(),
                    "request" => draft.settings.request_postfix = value.clone(),
                    "resource" => draft.settings.resource_postfix = value.clone(),
                    "factory" => draft.settings.factory_postfix = value.clone(),
                    "seeder" => draft.settings.seeder_postfix = value.clone(),
                    "migration" if !value.is_empty() => {
                        return Err("Anonymous migrations do not have a class postfix".into());
                    }
                    _ => {}
                }
            }
            if directory.is_none() && namespace.is_none() {
                continue;
            }
            let module_root = module
                .map(|module| {
                    module
                        .source_root
                        .strip_suffix("/app")
                        .unwrap_or(&module.source_root)
                })
                .unwrap_or("");
            let directory = directory
                .map(|directory| {
                    directory
                        .replace("${sourceRoot}", &draft.source_root)
                        .replace("${moduleRoot}", module_root)
                })
                .unwrap_or_else(|| draft.directory(kind));
            let namespace = namespace
                .map(|namespace| namespace.replace("${namespace}", &draft.namespace))
                .unwrap_or_else(|| {
                    if *kind == "migration" {
                        String::new()
                    } else {
                        draft.namespace_for(kind)
                    }
                });
            if !generator_plan::relative(&directory)
                || *kind != "migration"
                    && (!generator_plan::qualified(&namespace) || namespace.starts_with('\\'))
            {
                return Err(format!(
                    "Invalid effective {kind} directory/namespace for selected application/module"
                ));
            }
            draft.destinations.insert(
                (*kind).into(),
                generator_plan::Destination {
                    directory,
                    namespace,
                },
            );
        }
        Ok(())
    }
    pub(crate) fn apply_module(&self, draft: &mut Draft, id: &str) -> Result<(), String> {
        let module = self
            .config
            .modules
            .iter()
            .find(|module| module.id == id)
            .ok_or("Unknown module")?;
        draft.namespace = module.namespace.clone();
        draft.source_root = module.source_root.clone();
        draft.migration_root = module.migration_root.clone();
        let base = module
            .source_root
            .strip_suffix("/app")
            .unwrap_or(&module.source_root);
        draft.layout.factory_root = module
            .factory_root
            .clone()
            .unwrap_or_else(|| format!("{base}/database/factories"));
        draft.layout.factory_namespace = module
            .factory_namespace
            .clone()
            .unwrap_or_else(|| format!("{}\\Database\\Factories", module.namespace));
        draft.layout.seeder_root = module
            .seeder_root
            .clone()
            .unwrap_or_else(|| format!("{base}/database/seeders"));
        draft.layout.seeder_namespace = module
            .seeder_namespace
            .clone()
            .unwrap_or_else(|| format!("{}\\Database\\Seeders", module.namespace));
        Ok(())
    }
    fn resolved_values(
        &self,
        id: &str,
        inputs: &BTreeMap<String, Value>,
        module_id: Option<&str>,
        overrides: &BTreeMap<String, Value>,
    ) -> Result<BTreeMap<String, Value>, String> {
        let preset = self
            .config
            .presets
            .iter()
            .find(|preset| preset.id == id)
            .ok_or("Unknown generator preset")?;
        if inputs.keys().any(|key| {
            !preset
                .parameters
                .iter()
                .any(|parameter| &parameter.name == key)
        }) {
            return Err("Unknown or calculated generator input supplied".into());
        }
        if module_id.is_none()
            && (!preset.available_in_application || self.config.allow_application == Some(false))
            || module_id.is_some()
                && (!preset.available_in_modules
                    || preset
                        .module_ids
                        .as_ref()
                        .is_some_and(|ids| !ids.iter().any(|id| Some(id.as_str()) == module_id)))
        {
            return Err("Preset is unavailable in the selected application/module".into());
        }
        let mut values = BTreeMap::new();
        if let Some(current_time) = &self.current_time {
            values.insert("CURRENT_TIME".into(), Value::String(current_time.clone()));
        }
        for parameter in &preset.parameters {
            let value = inputs
                .get(&parameter.name)
                .or(parameter.default.as_ref())
                .ok_or_else(|| format!("Missing input {}", parameter.label))?;
            if !value_ok(parameter, value) {
                return Err(format!("Invalid input {}", parameter.label));
            }
            values.insert(parameter.name.clone(), value.clone());
        }
        if let Some(id) = module_id {
            let module = self
                .config
                .modules
                .iter()
                .find(|module| module.id == id)
                .ok_or("Unknown module")?;
            for (key, value) in [
                ("module.namespace", &module.namespace),
                ("module.sourceRoot", &module.source_root),
                ("module.migrationRoot", &module.migration_root),
                ("module.viewRoot", &module.view_root),
            ] {
                values.insert(key.into(), Value::String(value.clone()));
            }
        }
        if overrides.keys().any(|key| {
            !preset
                .calculated
                .iter()
                .any(|calc| calc.name == *key && !calc.hidden && calc.editable)
        }) {
            return Err("Unknown, hidden or read-only calculated override supplied".into());
        }
        for calc in &preset.calculated {
            // Calculations consume previous values only: forward references and
            // cycles are rejected instead of repeatedly evaluating expressions.
            let expression = &calc.expression[2..calc.expression.len() - 1];
            let calculated = expression_value(expression, &values)?;
            if calculated.as_str().is_some_and(|value| value.len() > 4096) {
                return Err("Calculated values must be at most 4096 bytes".into());
            }
            let value = if let Some(value) = overrides.get(&calc.name) {
                let valid = match (&calculated, value) {
                    (Value::String(_), Value::String(value)) => {
                        value.len() <= 4096 && !value.contains('\0')
                    }
                    (Value::Bool(_), Value::Bool(_)) => true,
                    (Value::Number(_), Value::Number(_)) => true,
                    _ => false,
                };
                if !valid {
                    return Err(format!("Invalid calculated override {}", calc.name));
                }
                value.clone()
            } else {
                calculated
            };
            values.insert(calc.name.clone(), value);
        }
        if values
            .values()
            .map(|value| value.as_str().map(str::len).unwrap_or(32))
            .sum::<usize>()
            > LIMIT
        {
            return Err("Resolved parameter values exceed the live-preview budget".into());
        }
        Ok(values)
    }
    pub(crate) fn model_inputs(
        &self,
        id: &str,
        inputs: &BTreeMap<String, Value>,
    ) -> Result<Vec<String>, String> {
        let preset = self
            .config
            .presets
            .iter()
            .find(|preset| preset.id == id)
            .ok_or("Unknown generator preset")?;
        preset
            .parameters
            .iter()
            .filter(|parameter| parameter.kind == "model")
            .map(|parameter| {
                inputs
                    .get(&parameter.name)
                    .or(parameter.default.as_ref())
                    .and_then(Value::as_str)
                    .filter(|value| generator_plan::qualified(value))
                    .map(str::to_owned)
                    .ok_or_else(|| format!("Choose an indexed model for {}", parameter.label))
            })
            .collect()
    }
    pub(crate) fn evaluate(
        &self,
        id: &str,
        inputs: &BTreeMap<String, Value>,
        module_id: Option<&str>,
        overrides: &BTreeMap<String, Value>,
    ) -> Result<Value, String> {
        let values = self.resolved_values(id, inputs, module_id, overrides)?;
        let preset = self
            .config
            .presets
            .iter()
            .find(|preset| preset.id == id)
            .ok_or("Unknown generator preset")?;
        let calculated=preset.calculated.iter().map(|calc|json!({"name":calc.name,"label":calc.label.as_deref().unwrap_or(&calc.name),"hidden":calc.hidden,"editable":calc.editable&&!calc.hidden,"value":values.get(&calc.name),"overridden":overrides.contains_key(&calc.name)})).collect::<Vec<_>>();
        Ok(json!({"values":values,"calculated":calculated}))
    }
    pub(crate) fn render(
        &self,
        id: &str,
        inputs: &BTreeMap<String, Value>,
        module_id: Option<&str>,
    ) -> Result<Plan, String> {
        self.render_with_overrides(id, inputs, module_id, &BTreeMap::new())
    }
    pub(crate) fn render_with_overrides(
        &self,
        id: &str,
        inputs: &BTreeMap<String, Value>,
        module_id: Option<&str>,
        overrides: &BTreeMap<String, Value>,
    ) -> Result<Plan, String> {
        let values = self.resolved_values(id, inputs, module_id, overrides)?;
        let preset = self
            .config
            .presets
            .iter()
            .find(|preset| preset.id == id)
            .ok_or("Unknown generator preset")?;
        let mut paths = BTreeSet::new();
        let mut files = Vec::new();
        let mut total = 0;
        let mut budget = ExpansionBudget::default();
        for file in &preset.files {
            let path = substitute(&file.path, &values, true)?;
            let folded = path.to_ascii_lowercase();
            if !generator_plan::relative(&path)
                || paths.iter().any(|other: &String| {
                    folded.starts_with(&format!("{other}/"))
                        || other.starts_with(&format!("{folded}/"))
                })
                || !paths.insert(folded)
            {
                return Err(format!("Invalid or conflicting generated path {path}"));
            }
            let template = expand_template(
                &file.template,
                &self.templates,
                &template_names(&self.config),
                &values,
                &mut Vec::new(),
                &mut budget,
            )?;
            let text = substitute(&template, &values, false)?;
            total += text.len();
            if text.len() > LIMIT || total > 2 << 20 {
                return Err("Rendered output exceeds generation limits".into());
            }
            files.push(Output { path, text });
        }
        Ok(Plan { files })
    }
}

#[derive(Default)]
struct ExpansionBudget {
    directives: usize,
    source_bytes: usize,
}
// Parse directives before substituting values so parameter text cannot become code.
fn expand_template(
    name: &str,
    templates: &BTreeMap<String, String>,
    registered: &BTreeSet<String>,
    values: &BTreeMap<String, Value>,
    stack: &mut Vec<String>,
    work: &mut ExpansionBudget,
) -> Result<String, String> {
    if stack.len() >= 8 || stack.iter().any(|item| item == name) {
        return Err("Shared template recursion or nesting limit exceeded".into());
    }
    if !registered.contains(name) {
        return Err("Include requires a registered shared template".into());
    }
    let body = templates
        .get(name)
        .ok_or("Shared template was not loaded")?;
    work.source_bytes += body.len();
    if work.source_bytes > 2 << 20 {
        return Err("Template expansion source-work limit exceeded".into());
    }
    stack.push(name.into());
    let result = expand_sections(
        body,
        name.starts_with("@user/"),
        templates,
        registered,
        values,
        stack,
        work,
    );
    stack.pop();
    result
}
fn expand_sections(
    body: &str,
    user: bool,
    templates: &BTreeMap<String, String>,
    registered: &BTreeSet<String>,
    values: &BTreeMap<String, Value>,
    stack: &mut Vec<String>,
    work: &mut ExpansionBudget,
) -> Result<String, String> {
    let mut output = String::new();
    let mut rest = body;
    // Each frame retains parent activity and the branch predicate.
    let mut branches: Vec<(bool, bool, bool)> = Vec::new();
    let mut active = true;
    while let Some(offset) = rest.find("${") {
        // $${...} is an escaped ordinary placeholder, including directive text.
        if offset > 0 && rest.as_bytes()[offset - 1] == b'$' {
            let end = rest[offset + 2..]
                .find('}')
                .map_or(rest.len(), |end| end + offset + 3);
            if active {
                output.push_str(&rest[..end]);
            }
            rest = &rest[end..];
            continue;
        }
        if active {
            output.push_str(&rest[..offset]);
        }
        let end = rest[offset + 2..]
            .find('}')
            .ok_or("Unclosed template placeholder")?
            + offset
            + 2;
        let token = &rest[offset + 2..end];
        work.directives += 1;
        if work.directives > 4096 {
            return Err("Template expansion work limit exceeded".into());
        }
        if let Some(condition) = token.strip_prefix("#if ") {
            let condition = condition.trim();
            let (invert, condition) = condition
                .strip_prefix('!')
                .map_or((false, condition), |name| (true, name));
            if !variable(condition) {
                return Err("Conditions require a boolean variable".into());
            }
            let predicate = values
                .get(condition)
                .and_then(Value::as_bool)
                .ok_or("Conditions require a declared boolean value")?
                ^ invert;
            if branches.len() >= 8 {
                return Err("Conditional nesting limit exceeded".into());
            }
            branches.push((active, predicate, false));
            active = active && predicate;
        } else if token == "#else" {
            let (parent, predicate, seen) = branches.last_mut().ok_or("Unexpected else")?;
            if *seen {
                return Err("Duplicate else".into());
            }
            *seen = true;
            active = *parent && !*predicate;
        } else if token == "/if" {
            active = branches.pop().ok_or("Unexpected conditional end")?.0;
        } else if let Some(name) = token.strip_prefix("#include ") {
            let name = name.trim();
            if !generator_plan::relative(name) || name.starts_with("@user/") {
                return Err("Include requires a scoped relative template name".into());
            }
            let name = if user {
                format!("@user/{name}")
            } else {
                name.to_string()
            };
            // Validate even skipped branches: missing, cyclic and malformed includes
            // cannot remain dormant until a boolean changes.
            let included = expand_template(&name, templates, registered, values, stack, work)?;
            if active {
                output.push_str(&included);
            }
        } else if token.starts_with('#') || token.starts_with('/') {
            return Err("Unknown template directive".into());
        } else if active {
            output.push_str(&rest[offset..=end]);
        }
        if output.len() > LIMIT {
            return Err("Expanded template exceeds 128 KiB".into());
        }
        rest = &rest[end + 1..];
    }
    if !branches.is_empty() {
        return Err("Unclosed conditional section".into());
    }
    if active {
        output.push_str(rest);
    }
    if output.len() > LIMIT {
        return Err("Expanded template exceeds 128 KiB".into());
    }
    Ok(output)
}

fn word_ranges(value: &str) -> Result<Vec<std::ops::Range<usize>>, String> {
    if value.is_empty() || value.len() > 4096 {
        return Err("Name transformations need 1–4096 bytes".into());
    }
    let chars = value.char_indices().collect::<Vec<_>>();
    let mut ranges = Vec::new();
    let mut start = None;
    for (index, (offset, ch)) in chars.iter().copied().enumerate() {
        if ch.is_whitespace() || matches!(ch, '_' | '-' | '.') {
            if let Some(start) = start.take() {
                ranges.push(start..offset);
            }
            continue;
        }
        if !ch.is_alphanumeric() {
            return Err("Name transforms accept letters, numbers, whitespace, underscores, hyphens and dots".into());
        }
        let boundary = start.is_some()
            && ch.is_uppercase()
            && index > 0
            && (chars[index - 1].1.is_lowercase()
                || chars[index - 1].1.is_numeric()
                || chars[index - 1].1.is_uppercase()
                    && chars
                        .get(index + 1)
                        .is_some_and(|(_, next)| next.is_lowercase()));
        if boundary {
            if let Some(old) = start.replace(offset) {
                ranges.push(old..offset);
            }
        } else if start.is_none() {
            start = Some(offset);
        }
    }
    if let Some(start) = start {
        ranges.push(start..value.len());
    }
    if ranges.is_empty() {
        return Err("Name transformation produced no name".into());
    }
    Ok(ranges)
}
pub(crate) fn snake_name(value: &str) -> Result<String, String> {
    Ok(words(value)?.join("_"))
}
fn words(value: &str) -> Result<Vec<String>, String> {
    Ok(word_ranges(value)?
        .into_iter()
        .map(|range| value[range].to_lowercase())
        .collect())
}
fn studly(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
const IRREGULAR: &[(&str, &str)] = &[
    ("person", "people"),
    ("child", "children"),
    ("man", "men"),
    ("woman", "women"),
    ("mouse", "mice"),
    ("goose", "geese"),
    ("tooth", "teeth"),
    ("foot", "feet"),
    ("ox", "oxen"),
    ("analysis", "analyses"),
    ("basis", "bases"),
    ("crisis", "crises"),
    ("thesis", "theses"),
    ("index", "indices"),
    ("matrix", "matrices"),
    ("vertex", "vertices"),
    ("quiz", "quizzes"),
    ("leaf", "leaves"),
    ("life", "lives"),
    ("knife", "knives"),
    ("wife", "wives"),
    ("wolf", "wolves"),
    ("calf", "calves"),
    ("half", "halves"),
    ("loaf", "loaves"),
    ("shelf", "shelves"),
    ("thief", "thieves"),
    ("self", "selves"),
    ("elf", "elves"),
    ("status", "statuses"),
    ("bus", "buses"),
    ("alias", "aliases"),
    ("axis", "axes"),
    ("testis", "testes"),
    ("gas", "gases"),
    ("bias", "biases"),
    ("atlas", "atlases"),
    ("lens", "lenses"),
    ("canvas", "canvases"),
    ("movie", "movies"),
    ("tie", "ties"),
    ("cookie", "cookies"),
    ("pie", "pies"),
];
fn inflect_word(word: &str, plural: bool) -> String {
    if [
        "sheep",
        "fish",
        "series",
        "species",
        "deer",
        "news",
        "equipment",
        "information",
        "rice",
        "money",
        "offspring",
        "salmon",
        "trout",
        "aircraft",
        "bison",
        "moose",
        "means",
        "metadata",
        "data",
    ]
    .contains(&word)
    {
        return word.into();
    }
    for (singular, multiple) in IRREGULAR {
        if word == *singular || word == *multiple {
            return if plural { *multiple } else { *singular }.into();
        }
    }
    if plural {
        let previous = word.chars().rev().nth(1);
        if word.ends_with('y') && previous.is_some_and(|ch| !"aeiouáéíóúàèìòùäëïöü".contains(ch))
        {
            return format!("{}ies", &word[..word.len() - 1]);
        }
        if word.ends_with("ss")
            || word.ends_with(['x', 'z'])
            || word.ends_with("ch")
            || word.ends_with("sh")
        {
            return format!("{word}es");
        }
        if word.ends_with('s') {
            return word.into();
        }
        format!("{word}s")
    } else {
        if word.ends_with("ies") && word.len() > 3 {
            return format!("{}y", &word[..word.len() - 3]);
        }
        if ["sses", "xes", "zes", "ches", "shes"]
            .iter()
            .any(|suffix| word.ends_with(suffix))
        {
            return word[..word.len() - 2].into();
        }
        if word.ends_with('s') && !word.ends_with("ss") {
            return word[..word.len() - 1].into();
        }
        word.into()
    }
}
pub(crate) fn inflect(value: &str, plural: bool) -> Result<String, String> {
    let ranges = word_ranges(value)?;
    let last = ranges.last().ok_or("Name contains no words")?;
    let word = &value[last.clone()];
    let inflected = inflect_word(&word.to_lowercase(), plural);
    let letters = word
        .chars()
        .filter(|ch| ch.is_alphabetic())
        .collect::<Vec<_>>();
    let transformed = if !letters.is_empty() && letters.iter().all(|ch| ch.is_uppercase()) {
        inflected.to_uppercase()
    } else if word.chars().next().is_some_and(char::is_uppercase) {
        studly(&inflected)
    } else {
        inflected
    };
    Ok(format!(
        "{}{}{}",
        &value[..last.start],
        transformed,
        &value[last.end..]
    ))
}

/// Only an escaped pipe is special: PHP FQN backslashes and other argument
/// bytes remain literal. Work is bounded before transformation or allocation.
fn expression_parts(expression: &str) -> Result<Vec<String>, String> {
    if expression.len() > 4096 {
        return Err("Expressions exceed 4096 bytes".into());
    }
    let mut parts = vec![String::new()];
    let mut chars = expression.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'|') {
            chars.next();
            parts.last_mut().unwrap().push('|');
        } else if ch == '|' {
            if parts.len() >= 65 {
                return Err("Expressions allow at most 64 transforms".into());
            }
            parts.push(String::new());
        } else {
            parts.last_mut().unwrap().push(ch);
        }
    }
    Ok(parts)
}
fn first_case(text: &str, upper: bool) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let first = if upper {
        first.to_uppercase().collect::<String>()
    } else {
        first.to_lowercase().collect()
    };
    format!("{first}{}", chars.as_str())
}
fn expression_value(expression: &str, values: &BTreeMap<String, Value>) -> Result<Value, String> {
    let parts = expression_parts(expression)?;
    let name = &parts[0];
    let mut value = values
        .get(name)
        .cloned()
        .ok_or_else(|| format!("Unknown input {name}"))?;
    for transform in parts.iter().skip(1) {
        let (kind, argument) = transform
            .split_once(':')
            .map(|(kind, arg)| (kind, Some(arg)))
            .unwrap_or((transform.as_str(), None));
        if argument.is_some()
            && !matches!(
                kind,
                "trim"
                    | "trimStart"
                    | "trimEnd"
                    | "replace"
                    | "removePrefix"
                    | "removeSuffix"
                    | "convertToClassName"
                    | "format"
            )
        {
            return Err(format!("Transform {kind} does not accept arguments"));
        }
        let text = value
            .as_str()
            .ok_or("Name transforms require a string input")?;
        value = Value::String(match kind {
            "upperCase" => text.to_uppercase(),
            "lowerCase" => text.to_lowercase(),
            "upperFirst" => first_case(text, true),
            "lowerFirst" => first_case(text, false),
            "className" => text
                .trim_start_matches('\\')
                .rsplit('\\')
                .next()
                .filter(|name| generator_plan::identifier(name))
                .ok_or("Invalid class name")?
                .into(),
            "namespace" | "namespaceString" => {
                if text.is_empty() && kind == "namespaceString" {
                    String::new()
                } else {
                    if !generator_plan::qualified(text) {
                        return Err("Invalid qualified class name".into());
                    }
                    let namespace = text
                        .trim_start_matches('\\')
                        .rsplit_once('\\')
                        .map(|(namespace, _)| namespace)
                        .unwrap_or("");
                    if kind == "namespaceString" && !namespace.is_empty() {
                        format!("\nnamespace {namespace};\n")
                    } else {
                        namespace.into()
                    }
                }
            }
            "useClassString" => {
                if text.is_empty() {
                    String::new()
                } else {
                    if !generator_plan::qualified(text) {
                        return Err("Invalid qualified class name".into());
                    }
                    format!("use {};", text.trim_start_matches('\\'))
                }
            }
            "snake" | "snakeCase" => words(text)?.join("_"),
            "upperSnakeCase" => words(text)?.join("_").to_uppercase(),
            "kebab" | "kebabCase" => words(text)?.join("-"),
            "dotCase" => words(text)?.join("."),
            "wordsCase" => words(text)?.join(" "),
            "titleCase" => word_ranges(text)?
                .into_iter()
                .map(|range| {
                    let word = &text[range];
                    if word
                        .chars()
                        .filter(|ch| ch.is_alphabetic())
                        .all(char::is_uppercase)
                    {
                        word.into()
                    } else {
                        studly(&word.to_lowercase())
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
            "sentenceCase" => first_case(&words(text)?.join(" "), true),
            "studly" | "upperCamelCase" => words(text)?.iter().map(|word| studly(word)).collect(),
            "camel" | "lowerCamelCase" => {
                let words = words(text)?;
                format!(
                    "{}{}",
                    words[0],
                    words[1..]
                        .iter()
                        .map(|word| studly(word))
                        .collect::<String>()
                )
            }
            "trim" | "trimStart" | "trimEnd" => {
                let trim = |ch: char| {
                    argument
                        .map(|chars| chars.contains(ch))
                        .unwrap_or_else(|| ch.is_whitespace())
                };
                match kind {
                    "trimStart" => text.trim_start_matches(trim),
                    "trimEnd" => text.trim_end_matches(trim),
                    _ => text.trim_matches(trim),
                }
                .into()
            }
            "replace" => {
                let (old, new) = argument
                    .and_then(|arg| arg.split_once(','))
                    .ok_or("replace requires old,new")?;
                let matches = if old.is_empty() {
                    text.chars().count() + 1
                } else {
                    text.matches(old).count()
                };
                let bytes = text
                    .len()
                    .checked_add(
                        matches
                            .checked_mul(new.len())
                            .ok_or("Replacement exceeds value budget")?,
                    )
                    .ok_or("Replacement exceeds value budget")?;
                if bytes > LIMIT {
                    return Err("Replacement exceeds value budget".into());
                }
                text.replace(old, new)
            }
            "removePrefix" => text
                .strip_prefix(argument.ok_or("removePrefix requires an argument")?)
                .unwrap_or(text)
                .into(),
            "removeSuffix" => text
                .strip_suffix(argument.ok_or("removeSuffix requires an argument")?)
                .unwrap_or(text)
                .into(),
            "convertToClassName" => {
                let replacement = argument.unwrap_or("_");
                if replacement.chars().count() != 1 {
                    return Err("convertToClassName replacement must be one character".into());
                }
                let mut converted = String::new();
                for ch in text.chars() {
                    if ch.is_alphanumeric() || ch == '_' {
                        converted.push(ch)
                    } else {
                        converted.push_str(replacement)
                    }
                }
                converted
                    .trim_start_matches(|ch: char| ch == '_' || ch.is_whitespace())
                    .into()
            }
            "plural" | "singular" => inflect(text, kind == "plural")?,
            "format" => date::format(text, argument.ok_or("format requires a date pattern")?)?,
            "php" => format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'")),
            _ => return Err(format!("Unsupported transform {kind}")),
        });
        if value
            .as_str()
            .is_some_and(|value| value.len() > LIMIT || value.contains('\0'))
        {
            return Err(
                "Calculated transformation exceeds the bounded value limit or contains NUL".into(),
            );
        }
    }
    Ok(value)
}
fn substitute(
    template: &str,
    values: &BTreeMap<String, Value>,
    path: bool,
) -> Result<String, String> {
    let mut rest = template;
    let mut result = String::new();
    while let Some(at) = rest.find("${") {
        if at > 0 && rest.as_bytes()[at - 1] == b'$' {
            result.push_str(&rest[..at - 1]);
            result.push_str("${");
            rest = &rest[at + 2..];
            continue;
        }
        result.push_str(&rest[..at]);
        let end = rest[at + 2..]
            .find('}')
            .ok_or("Unclosed template substitution")?
            + at
            + 2;
        let expression = &rest[at + 2..end];
        let value = expression_value(expression, values)?;
        let text = match value {
            Value::String(s) => s,
            Value::Bool(value) => value.to_string(),
            _ => return Err("Only string/boolean values can be substituted".into()),
        };
        if !path
            && !expression.ends_with("|php")
            && !expression.ends_with("|namespaceString")
            && !expression.ends_with("|useClassString")
            && text
                .chars()
                .any(|c| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '\\' | '/' | '-' | '.')))
        {
            return Err("Arbitrary text needs the php transform for an escaped PHP literal".into());
        }
        result.push_str(&text);
        rest = &rest[end + 1..];
        if result.len() > LIMIT {
            return Err("Template expansion exceeds 128 KiB".into());
        }
    }
    result.push_str(rest);
    if result.contains('\0') {
        return Err("Templates cannot contain NUL".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn documented_mutators_compose_unicode_acronyms_fqn_helpers_and_escaped_arguments() {
        for (input, expression, expected) in [
            ("OrderHTTPKey", "value|upperCase", "ORDERHTTPKEY"),
            ("ORDERKey", "value|lowerCase", "orderkey"),
            (
                "HTTPResponse_order-item",
                "value|snakeCase",
                "http_response_order_item",
            ),
            ("OrderHTTPKey", "value|upperSnakeCase", "ORDER_HTTP_KEY"),
            ("OrderHTTPKey", "value|dotCase", "order.http.key"),
            ("OrderHTTPKey", "value|wordsCase", "order http key"),
            ("OrderHTTPKey", "value|titleCase", "Order HTTP Key"),
            ("OrderHTTPKey", "value|sentenceCase", "Order http key"),
            ("order_http_key", "value|upperCamelCase", "OrderHttpKey"),
            ("OrderHTTPKey", "value|lowerCamelCase", "orderHttpKey"),
            ("éclairAPI", "value|upperFirst", "ÉclairAPI"),
            ("ÉclairAPI", "value|lowerFirst", "éclairAPI"),
            ("\u{2003} message \t", "value|trim", "message"),
            ("||message||", r"value|trim:\|", "message"),
            ("__message__", "value|trimStart:_", "message__"),
            ("__message__", "value|trimEnd:_", "__message"),
            ("foo|foo", r"value|replace:foo,bar|replace:\|,_", "bar_bar"),
            (
                "PrefixOrderSuffix",
                "value|removePrefix:Prefix|removeSuffix:Suffix",
                "Order",
            ),
            (
                "__ hello%éclair",
                "value|convertToClassName",
                "hello_éclair",
            ),
            ("report%item", "value|convertToClassName:-", "report-item"),
            (
                "\\Domain\\Sales\\Order",
                "value|namespaceString",
                "\nnamespace Domain\\Sales;\n",
            ),
            (
                "\\Domain\\Sales\\Order",
                "value|useClassString",
                "use Domain\\Sales\\Order;",
            ),
            ("Order", "value|namespaceString", ""),
            ("", "value|useClassString", ""),
        ] {
            let values = BTreeMap::from([("value".into(), json!(input))]);
            assert_eq!(
                expression_value(expression, &values).unwrap(),
                expected,
                "{expression}"
            );
        }
        let values = BTreeMap::from([("value".into(), json!("A\\B; execute()"))]);
        assert!(expression_value("value|useClassString", &values).is_err());
        for expression in [
            "value|replace:missing",
            "value|upperCase:unexpected",
            "value|format:unknown",
            "value|removePrefix",
            "value|convertToClassName:two",
        ] {
            assert!(
                expression_value(expression, &values).is_err(),
                "{expression}"
            );
        }
    }
    #[test]
    fn package_options_reject_inapplicable_settings_and_capture_principal_templates() {
        for generators in [
            json!({"nova-resource":{"separate":true}}),
            json!({"filament-table":{"baseClass":"App\\Resource"}}),
            json!({"filament-schema":{"separate":false}}),
        ] {
            assert!(
                from_parts(
                    json!({"version":1,"generators":generators}),
                    BTreeMap::new()
                )
                .is_err()
            );
        }
        let bundle=from_parts(json!({"version":1,"partials":["shared.php"],"generators":{"filament-resource":{"template":"resource.php","separate":false}}}),BTreeMap::from([("resource.php".into(),"${#include shared.php}\n${generated}".into()),("shared.php".into(),"// reviewed resource template".into())])).unwrap();
        let generated = "<?php namespace App\\Filament; class PostResource {}";
        let output = bundle
            .render_package_output(
                "filament-resource",
                None,
                "App\\Filament\\PostResource",
                "App\\Models\\Post",
                "posts",
                generated,
                true,
            )
            .unwrap();
        assert_eq!(
            output,
            format!("// reviewed resource template\n{generated}")
        );
        assert_eq!(
            bundle
                .render_package_output(
                    "nova-resource",
                    None,
                    "App\\Nova\\Post",
                    "App\\Models\\Post",
                    "posts",
                    generated,
                    true
                )
                .unwrap(),
            generated
        );
    }
    #[test]
    fn captured_current_time_is_pure_bound_evidence_and_rendered_consistently() {
        let bundle=from_parts(json!({"version":1,"presets":[{"id":"dated","label":"Dated","calculated":[{"name":"stamp","expression":"${CURRENT_TIME|format:yyyyMMdd_HHmmss}"}],"files":[{"path":"app/Generated_${stamp}.php","template":"dated.php"}]}]}),BTreeMap::from([("dated.php".into(),"<?php // ${CURRENT_TIME|format:yyyy-MM-dd} stamp ${stamp}".into())])).unwrap();
        assert!(
            bundle.render("dated", &BTreeMap::new(), None).is_err(),
            "a pure bundle never reads the clock"
        );
        let first = bundle
            .clone()
            .with_current_time("2024-02-29 23:59:58")
            .unwrap();
        let other = bundle.with_current_time("2024-03-01 00:00:00").unwrap();
        let plan = first.render("dated", &BTreeMap::new(), None).unwrap();
        assert_eq!(plan.files[0].path, "app/Generated_20240229_235958.php");
        assert_eq!(
            plan.files[0].text,
            "<?php // 2024-02-29 stamp 20240229_235958"
        );
        assert_eq!(first.render("dated", &BTreeMap::new(), None).unwrap(), plan);
        assert_ne!(first.fingerprint(), other.fingerprint());
        let values = BTreeMap::from([("CURRENT_TIME".into(), json!("2024-02-29 23:59:58"))]);
        assert_eq!(
            expression_value("CURRENT_TIME|format:dd.MM.yyyy 'at' HH:mm", &values).unwrap(),
            "29.02.2024 at 23:59"
        );
    }
    #[test]
    fn compound_inflection_preserves_case_separators_and_utf8() {
        for (input, plural, expected) in [
            ("UserPerson", true, "UserPeople"),
            ("user_child", true, "user_children"),
            ("USER_ACCOUNTS", false, "USER_ACCOUNT"),
            ("AccountPerson", true, "AccountPeople"),
            ("API.Person", true, "API.People"),
            ("ÉquipePerson", true, "ÉquipePeople"),
            ("café_person", true, "café_people"),
            ("Person", false, "Person"),
        ] {
            assert_eq!(inflect(input, plural).unwrap(), expected);
        }
        let values = BTreeMap::from([("model".into(), json!("App\\Models\\AccountPerson"))]);
        assert_eq!(
            substitute("${model|className|snake|plural}", &values, false).unwrap(),
            "account_people"
        );
        assert_eq!(studly("équipe"), "Équipe");
        assert!(inflect("user/person", true).is_err());
    }
    fn section_bundle(body: &str, partial: &str) -> Bundle {
        from_parts(json!({"version":1,"partials":["header.php"],"presets":[{"id":"sample","label":"Sample","parameters":[{"name":"save","label":"Save","type":"boolean","default":false},{"name":"name","label":"Name","type":"text","default":"Post"}],"files":[{"path":"app/Post.php","template":"model.php"}]}]}),BTreeMap::from([("model.php".into(),body.into()),("header.php".into(),partial.into())])).unwrap()
    }
    #[test]
    fn registered_shared_templates_and_boolean_sections_are_inert_and_bounded() {
        let bundle = section_bundle(
            "${#include header.php}\n${#if save}public function save() {}${#else}// no save${/if}\n$${#include untouched}",
            "<?php // ${name}",
        );
        let render = |save| {
            bundle
                .render(
                    "sample",
                    &BTreeMap::from([("save".into(), json!(save))]),
                    None,
                )
                .unwrap()
                .files[0]
                .text
                .clone()
        };
        assert_eq!(
            render(false),
            "<?php // Post\n// no save\n${#include untouched}"
        );
        assert!(render(true).contains("public function save() {}"));
        assert_eq!(
            section_bundle("$${", "unused")
                .render("sample", &BTreeMap::new(), None)
                .unwrap()
                .files[0]
                .text,
            "${"
        );
        // Substituted data is never reinterpreted as directives.
        let bundle = section_bundle("<?php // ${name|php}", "unused");
        let rendered = bundle
            .render(
                "sample",
                &BTreeMap::from([("name".into(), json!("${#include secret}"))]),
                None,
            )
            .unwrap();
        assert!(rendered.files[0].text.contains("${#include secret}"));
        for body in [
            "${#if name}bad${/if}",
            "${#if save}${#else}${#else}${/if}",
            "${#if save}unclosed",
            "${/if}",
            "${#if save}${#include ../escape}${/if}",
            "${#if save}${#include missing.php}${/if}",
            "${#unknown}",
            "${#include @user/header.php}",
        ] {
            assert!(
                section_bundle(body, "header")
                    .render("sample", &BTreeMap::new(), None)
                    .is_err(),
                "{body}"
            );
        }
        assert!(
            section_bundle(
                "${#if save}${#include header.php}${/if}",
                "${#include header.php}"
            )
            .render("sample", &BTreeMap::new(), None)
            .is_err()
        );
        assert!(
            section_bundle(&"${#include header.php}".repeat(4097), "x")
                .render("sample", &BTreeMap::new(), None)
                .is_err()
        );
        assert!(
            section_bundle(
                "${#include header.php}${#include header.php}",
                &"x".repeat(LIMIT)
            )
            .render("sample", &BTreeMap::new(), None)
            .is_err()
        );
    }
    #[test]
    #[ignore = "requires local PHP executable; syntax checks without executing generated PHP"]
    fn generated_shared_conditional_php_lints() {
        let bundle = section_bundle(
            "${#include header.php}\nclass ${name} {\n${#if save}public function save(): bool { return true; }${#else}public function skip(): void {}${/if}\n}\n",
            "<?php\ndeclare(strict_types=1);\nnamespace App\\Models;",
        );
        for save in [false, true] {
            let file = std::env::temp_dir().join(format!(
                "laravel-preset-lint-{}-{save}.php",
                std::process::id()
            ));
            let generated = bundle
                .render(
                    "sample",
                    &BTreeMap::from([("save".into(), json!(save))]),
                    None,
                )
                .unwrap();
            std::fs::write(&file, &generated.files[0].text).unwrap();
            let result = std::process::Command::new("php")
                .arg("-l")
                .arg(&file)
                .output()
                .unwrap();
            std::fs::remove_file(&file).unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        let dated = from_parts(json!({"version":1,"presets":[{"id":"service","label":"Service","parameters":[{"name":"model","label":"Model","type":"model","default":"Domain\\Sales\\Order"},{"name":"owner","label":"Owner","type":"model","default":"App\\Generated\\OrderService"}],"files":[{"path":"app/Generated/${owner|className}.php","template":"service.php"}]}]}), BTreeMap::from([("service.php".into(), "<?php\n${owner|namespaceString}\n${model|useClassString}\nfinal class ${owner|className} { public const CREATED = ${CURRENT_TIME|format:yyyy-MM-dd'T'HH:mm:ssXXX|php}; public const LABEL = ${model|className|plural|titleCase|php}; }".into())])).unwrap().with_current_time("2024-02-29 23:59:58").unwrap();
        let output = dated.render("service", &BTreeMap::new(), None).unwrap();
        let file = std::env::temp_dir().join(format!(
            "laravel-preset-date-fqn-lint-{}.php",
            std::process::id()
        ));
        std::fs::write(&file, &output.files[0].text).unwrap();
        let result = std::process::Command::new("php")
            .arg("-l")
            .arg(&file)
            .output()
            .unwrap();
        std::fs::remove_file(file).unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    #[test]
    fn shared_template_source_work_is_bounded_even_when_output_is_skipped() {
        let body = format!(
            "${{#if save}}{}${{/if}}",
            "${#include header.php}".repeat(17)
        );
        assert!(
            section_bundle(&body, &"x".repeat(LIMIT))
                .render("sample", &BTreeMap::new(), None)
                .is_err()
        );
    }
    #[test]
    fn project_shared_template_changes_invalidate_bundle_fingerprint() {
        let host = openidea_plugin::mock::MockHost::install();
        host.add_file(CONFIG,r#"{"version":1,"partials":["header.php"],"presets":[{"id":"sample","label":"Sample","files":[{"path":"app/Post.php","template":"post.php"}]}]}"#);
        host.add_file(
            ".openidea/laravel-templates/post.php",
            "${#include header.php}",
        );
        host.add_file(
            ".openidea/laravel-templates/header.php",
            "<?php // original",
        );
        let project = host.invocation().project().unwrap();
        let before = load(&project).unwrap();
        assert_eq!(
            before
                .render("sample", &BTreeMap::new(), None)
                .unwrap()
                .files[0]
                .text,
            "<?php // original"
        );
        host.add_file(".openidea/laravel-templates/header.php", "<?php // changed");
        assert_ne!(before.fingerprint(), load(&project).unwrap().fingerprint());
    }
    #[test]
    fn includes_preserve_project_user_scope_and_cannot_read_unregistered_templates() {
        let mut bundle = section_bundle("${#include header.php}", "project");
        bundle
            .templates
            .insert("extra.php".into(), "unregistered".into());
        bundle
            .templates
            .insert("model.php".into(), "${#include extra.php}".into());
        assert!(bundle.render("sample", &BTreeMap::new(), None).is_err());
        bundle.config.presets[0].files[0].template = "@user/model.php".into();
        bundle.config.partials.push("@user/header.php".into());
        bundle
            .templates
            .insert("@user/model.php".into(), "${#include header.php}".into());
        bundle
            .templates
            .insert("@user/header.php".into(), "user".into());
        assert_eq!(
            bundle
                .render("sample", &BTreeMap::new(), None)
                .unwrap()
                .files[0]
                .text,
            "user"
        );
    }
    #[test]
    fn per_kind_destinations_keep_generated_references_coherent_in_application_and_module() {
        let bundle=from_parts(json!({"version":1,"generators":{"model":{"directory":"${sourceRoot}/Entities","namespace":"${namespace}\\Entities"},"controller":{"directory":"${sourceRoot}/Actions","namespace":"${namespace}\\Actions"},"factory":{"directory":"custom/factories","namespace":"Custom\\Factories"},"seeder":{"directory":"custom/seeders","namespace":"Custom\\Seeders"},"migration":{"directory":"custom/migrations"}},"modules":[{"id":"blog","label":"Blog","namespace":"Modules\\Blog","sourceRoot":"modules/Blog/app","migrationRoot":"modules/Blog/database/migrations","viewRoot":"modules/Blog/resources/views","generators":{"factory":{"directory":"${moduleRoot}/Factories","namespace":"${namespace}\\Factories"},"seeder":{"directory":"${moduleRoot}/Seeders","namespace":"${namespace}\\Seeders"}}}]}),BTreeMap::new()).unwrap();
        for module in [None, Some("blog")] {
            let mut draft:Draft=serde_json::from_value(json!({"name":"Post","table":"posts","migrationStamp":"2026_11_10_120000","related":["controller","migration","factory","seeder"]})).unwrap();
            if let Some(id) = module {
                bundle.apply_module(&mut draft, id).unwrap();
            }
            bundle.apply_generator_options(&mut draft, module).unwrap();
            let plan = generator_plan::plan(&draft).unwrap();
            let prefix = if module.is_some() {
                "Modules\\Blog"
            } else {
                "App"
            };
            let root = if module.is_some() {
                "modules/Blog/app"
            } else {
                "app"
            };
            assert!(plan.files.iter().any(|file| {
                file.path == format!("{root}/Entities/Post.php")
                    && file
                        .text
                        .contains(&format!("namespace {prefix}\\Entities;"))
            }));
            assert!(plan.files.iter().any(|file| file.path
                == format!("{root}/Actions/PostController.php")
                && file.text.contains(&format!("namespace {prefix}\\Actions;"))));
            let factory = plan
                .files
                .iter()
                .find(|file| file.path.ends_with("/PostFactory.php"))
                .unwrap();
            let seeder = plan
                .files
                .iter()
                .find(|file| file.path.ends_with("/PostSeeder.php"))
                .unwrap();
            assert!(
                factory
                    .text
                    .contains(&format!("\\{prefix}\\Entities\\Post::class"))
            );
            assert!(
                seeder
                    .text
                    .contains(&format!("\\{prefix}\\Entities\\Post::factory()"))
            );
            let expected = if module.is_some() {
                "Modules\\Blog\\Factories"
            } else {
                "Custom\\Factories"
            };
            assert!(plan.files.iter().any(|file| {
                file.path.ends_with("/Post.php")
                    && file
                        .text
                        .contains(&format!("\\{expected}\\PostFactory::new()"))
            }));
            assert!(
                plan.files
                    .iter()
                    .any(|file| file.path.starts_with("custom/migrations/"))
            );
        }
    }
    #[test]
    fn native_and_preset_availability_are_enforced_for_application_and_specific_modules() {
        let config = json!({"version":1,"generators":{"model":{"availableInApplication":false},"factory":{"availableInModules":false}},"modules":[{"id":"blog","label":"Blog","namespace":"Modules\\Blog","sourceRoot":"modules/Blog/app","migrationRoot":"modules/Blog/database/migrations","viewRoot":"modules/Blog/resources/views","generators":{"factory":{"availableInModules":true}}},{"id":"shop","label":"Shop","namespace":"Modules\\Shop","sourceRoot":"modules/Shop/app","migrationRoot":"modules/Shop/database/migrations","viewRoot":"modules/Shop/resources/views"}],"presets":[{"id":"module_only","label":"Module only","availableInApplication":false,"moduleIds":["blog"],"files":[{"path":"${module.sourceRoot}/Custom.php","template":"custom.php"}]},{"id":"app_only","label":"Application only","availableInModules":false,"files":[{"path":"app/Custom.php","template":"custom.php"}]}]});
        let bundle = from_parts(
            config,
            BTreeMap::from([("custom.php".into(), "<?php".into())]),
        )
        .unwrap();
        let mut draft:Draft=serde_json::from_value(json!({"name":"Post","table":"posts","migrationStamp":"2026_11_10_120000","related":["factory"]})).unwrap();
        assert!(bundle.apply_generator_options(&mut draft, None).is_err());
        assert!(
            bundle
                .apply_generator_options(&mut draft, Some("shop"))
                .is_err()
        );
        assert!(
            bundle
                .apply_generator_options(&mut draft, Some("blog"))
                .is_ok()
        );
        assert!(
            bundle
                .render("module_only", &BTreeMap::new(), None)
                .is_err()
        );
        assert!(
            bundle
                .render("module_only", &BTreeMap::new(), Some("shop"))
                .is_err()
        );
        assert!(
            bundle
                .render("module_only", &BTreeMap::new(), Some("blog"))
                .is_ok()
        );
        assert!(bundle.render("app_only", &BTreeMap::new(), None).is_ok());
        assert!(
            bundle
                .render("app_only", &BTreeMap::new(), Some("blog"))
                .is_err()
        );
        assert_eq!(
            bundle.catalog()["availability"]["application"]["model"],
            false
        );
        assert_eq!(
            bundle.catalog()["availability"]["modules"]["blog"]["factory"],
            true
        );
        assert_eq!(
            bundle.catalog()["availability"]["modules"]["shop"]["factory"],
            false
        );
    }
    #[test]
    fn settings_for_unselected_unavailable_kinds_do_not_block_other_native_outputs() {
        let bundle=from_parts(json!({"version":1,"generators":{"factory":{"directory":"${moduleRoot}/Factories","availableInApplication":false}}}),BTreeMap::new()).unwrap();
        let mut draft: Draft = serde_json::from_value(
            json!({"name":"Post","table":"posts","migrationStamp":"2026_11_10_120000"}),
        )
        .unwrap();
        bundle.apply_generator_options(&mut draft, None).unwrap();
        assert!(draft.destinations.is_empty());
        draft.related.insert("factory".into());
        assert!(bundle.apply_generator_options(&mut draft, None).is_err());
    }
    #[test]
    fn per_kind_settings_reject_unknown_kinds_escapes_and_invalid_namespaces() {
        for option in [
            json!({"unknown":{}}),
            json!({"model":{"directory":"../escape"}}),
            json!({"model":{"directory":"${unknown}/Entities"}}),
            json!({"model":{"namespace":"${unknown}\\Entities"}}),
            json!({"migration":{"namespace":"App\\Migrations"}}),
            json!({"model":{"namespace":"\\App\\Entities"}}),
        ] {
            assert!(parse(&json!({"version":1,"generators":option}).to_string()).is_err());
        }
        assert!(parse(&json!({"version":1,"presets":[{"id":"bad","label":"Bad","moduleIds":["blog","blog"],"files":[{"path":"app/One.php","template":"one.php"}]}]}).to_string()).is_err());
        let bundle = from_parts(
            json!({"version":1,"generators":{"model":{"directory":"${moduleRoot}/Entities"}}}),
            BTreeMap::new(),
        )
        .unwrap();
        let mut draft: Draft = serde_json::from_value(
            json!({"name":"Post","table":"posts","migrationStamp":"2026_11_10_120000"}),
        )
        .unwrap();
        assert!(bundle.apply_generator_options(&mut draft, None).is_err());
    }
    fn live_bundle() -> Bundle {
        from_parts(json!({"version":1,"presets":[{"id":"repository","label":"Repository","parameters":[{"name":"model","label":"Model","type":"model"}],"calculated":[{"name":"table","label":"Table name","expression":"${model|className|snake|plural}"},{"name":"output","expression":"${table|studly}","editable":false},{"name":"internal","expression":"${table|singular}","hidden":true}],"files":[{"path":"app/${output}.php","template":"repository.php"}]}]}),BTreeMap::from([("repository.php".into(),"<?php // ${table|php}".into())])).unwrap()
    }
    #[test]
    fn live_calculations_follow_inputs_until_explicit_override_and_resume_after_reset() {
        let bundle = live_bundle();
        let mut inputs = BTreeMap::from([("model".into(), json!("App\\Models\\OrderLine"))]);
        let automatic = bundle
            .evaluate("repository", &inputs, None, &BTreeMap::new())
            .unwrap();
        assert_eq!(automatic["values"]["table"], "order_lines");
        assert_eq!(automatic["calculated"][0]["label"], "Table name");
        assert_eq!(automatic["calculated"][2]["hidden"], true);
        let overrides = BTreeMap::from([("table".into(), json!("custom_records"))]);
        let overridden = bundle
            .evaluate("repository", &inputs, None, &overrides)
            .unwrap();
        assert_eq!(overridden["values"]["output"], "CustomRecords");
        assert_eq!(overridden["calculated"][0]["overridden"], true);
        inputs.insert("model".into(), json!("App\\Models\\UserProfile"));
        assert_eq!(
            bundle
                .evaluate("repository", &inputs, None, &overrides)
                .unwrap()["values"]["table"],
            "custom_records"
        );
        assert_eq!(
            bundle
                .evaluate("repository", &inputs, None, &BTreeMap::new())
                .unwrap()["values"]["table"],
            "user_profiles"
        );
        let plan = bundle
            .render_with_overrides("repository", &inputs, None, &overrides)
            .unwrap();
        assert_eq!(plan.files[0].path, "app/CustomRecords.php");
        assert!(plan.files[0].text.contains("'custom_records'"));
    }
    #[test]
    fn hidden_readonly_unknown_and_wrong_typed_calculated_overrides_are_refused() {
        let bundle = live_bundle();
        let inputs = BTreeMap::from([("model".into(), json!("App\\Models\\Post"))]);
        for overrides in [
            BTreeMap::from([("internal".into(), json!("x"))]),
            BTreeMap::from([("output".into(), json!("x"))]),
            BTreeMap::from([("unknown".into(), json!("x"))]),
            BTreeMap::from([("table".into(), json!(true))]),
            BTreeMap::from([("table".into(), json!({"execute":"php"}))]),
        ] {
            assert!(
                bundle
                    .evaluate("repository", &inputs, None, &overrides)
                    .is_err()
            );
            assert!(
                bundle
                    .render_with_overrides("repository", &inputs, None, &overrides)
                    .is_err()
            );
        }
    }
    #[test]
    fn live_expression_growth_is_bounded_before_returning_preview_values() {
        let mut values = BTreeMap::from([("name".into(), json!("'".repeat(4096)))]);
        let expression = format!("name{}", "|php".repeat(32));
        assert!(expression_value(&expression, &values).is_err());
        values.insert("name".into(), json!("Post"));
        assert_eq!(
            expression_value("name|snake|plural", &values).unwrap(),
            json!("posts")
        );
    }
    fn bundle() -> Bundle {
        let config=parse(r#"{"version":1,"modules":[{"id":"blog","label":"Blog","namespace":"Modules\\Blog","sourceRoot":"Modules/Blog/app","migrationRoot":"Modules/Blog/database/migrations","viewRoot":"Modules/Blog/resources/views"}],"presets":[{"id":"repository","label":"Repository","parameters":[{"name":"model","label":"Model","type":"model"},{"name":"description","label":"Description","type":"text","default":"John's repository"},{"name":"enabled","label":"Enabled","type":"boolean","default":true}],"calculated":[{"name":"class","expression":"${model|className}"},{"name":"table","expression":"${class|snake|plural}"}],"files":[{"path":"${module.sourceRoot}/Repositories/${class}Repository.php","template":"repository.php"},{"path":"${module.viewRoot}/${table}.blade.php","template":"view.blade.php"}]}]}"#).unwrap();
        Bundle{config,templates:BTreeMap::from([("repository.php".into(),"<?php namespace ${module.namespace}\\Repositories; class ${class}Repository { public $label = ${description|php}; public $enabled = ${enabled}; }".into()),("view.blade.php".into(),"<div>{{ $title }}</div> $${literal}".into())]),evidence:"fixture".into(),current_time:None}
    }
    #[test]
    fn renders_multiple_files_calculations_escaped_literals_and_modules() {
        let bundle = bundle();
        let values = BTreeMap::from([("model".into(), json!("App\\Models\\BlogPost"))]);
        let plan = bundle.render("repository", &values, Some("blog")).unwrap();
        assert_eq!(
            plan.files[0].path,
            "Modules/Blog/app/Repositories/BlogPostRepository.php"
        );
        assert!(plan.files[0].text.contains("'John\\'s repository'"));
        assert!(plan.files[0].text.contains("public $enabled = true"));
        assert_eq!(
            plan.files[1].path,
            "Modules/Blog/resources/views/blog_posts.blade.php"
        );
        assert_eq!(plan.files[1].text, "<div>{{ $title }}</div> ${literal}");
    }
    #[test]
    fn rejects_unknown_inputs_missing_module_and_path_traversal() {
        let mut bundle = bundle();
        let mut values = BTreeMap::from([("model".into(), json!("Post"))]);
        assert!(bundle.render("repository", &values, None).is_err());
        values.insert("table".into(), json!("override"));
        assert!(bundle.render("repository", &values, Some("blog")).is_err());
        values.remove("table");
        bundle.config.presets[0].files[0].path = "../escape.php".into();
        assert!(bundle.render("repository", &values, Some("blog")).is_err());
    }
    #[test]
    fn rejects_duplicate_keys_versions_and_executable_expressions() {
        for config in [
            r#"{"version":1,"version":1}"#,
            r#"{"version":2}"#,
            r#"{"version":1,"extra":true}"#,
        ] {
            assert!(parse(config).is_err());
        }
        let mut bundle = bundle();
        bundle.config.presets[0].calculated[0].expression = "${model|shell}".into();
        assert!(
            bundle
                .render(
                    "repository",
                    &BTreeMap::from([("model".into(), json!("Post"))]),
                    Some("blog")
                )
                .is_err()
        );
    }
    #[test]
    fn duplicate_paths_and_calculation_cycles_are_refused() {
        let mut bundle = bundle();
        bundle.config.presets[0].files[1].path = bundle.config.presets[0].files[0].path.clone();
        let values = BTreeMap::from([("model".into(), json!("Post"))]);
        assert!(bundle.render("repository", &values, Some("blog")).is_err());
        bundle.config.presets[0].calculated[0].expression = "${table}".into();
        assert!(bundle.render("repository", &values, Some("blog")).is_err());
    }
    #[test]
    fn transformations_are_bounded_and_acronyms_work() {
        let values = BTreeMap::from([
            ("name".into(), json!("HTTPResponse")),
            ("class".into(), json!("App\\Models\\Person")),
        ]);
        assert_eq!(
            substitute("${name|snake}", &values, false).unwrap(),
            "http_response"
        );
        assert_eq!(
            substitute("${class|className|snake|plural}", &values, false).unwrap(),
            "people"
        );
        assert!(substitute("${name|eval}", &values, false).is_err());
        assert!(substitute("${missing}", &values, false).is_err());
    }
    #[test]
    fn text_cannot_accidentally_become_php_source() {
        let values = BTreeMap::from([("text".into(), json!("'; system('id'); //"))]);
        assert!(substitute("${text}", &values, false).is_err());
        assert_eq!(
            substitute("${text|php}", &values, false).unwrap(),
            "'\\'; system(\\'id\\'); //'"
        );
    }

    #[test]
    fn project_bundle_evidence_tracks_templates_and_respects_nested_owners() {
        let mock = openidea_plugin::mock::MockHost::install();
        let project = mock.invocation().project().unwrap();
        assert_eq!(load(&project).unwrap().catalog()["presets"], json!([]));
        let config = r#"{"version":1,"presets":[{"id":"simple","label":"Simple","files":[{"path":"app/Generated.php","template":"simple.php"}]}]}"#;
        mock.add_file(CONFIG, config);
        mock.add_file(".openidea/laravel-templates/simple.php", "<?php // first");
        let first = load(&project).unwrap();
        assert_eq!(
            first
                .render("simple", &BTreeMap::new(), None)
                .unwrap()
                .files[0]
                .text,
            "<?php // first"
        );
        mock.add_file(".openidea/laravel-templates/simple.php", "<?php // changed");
        assert_ne!(first.fingerprint(), load(&project).unwrap().fingerprint());
        mock.add_project("/project/.openidea/laravel-templates");
        assert!(
            load(&project).is_err(),
            "a nested open project's templates cannot be read"
        );
    }

    #[test]
    fn module_selection_applies_explicit_native_model_roots() {
        let bundle = bundle();
        let mut draft: Draft = serde_json::from_value(
            json!({"name":"Post","table":"posts","migrationStamp":"2026_11_10_120000"}),
        )
        .unwrap();
        bundle.apply_module(&mut draft, "blog").unwrap();
        assert_eq!(draft.namespace, "Modules\\Blog");
        assert_eq!(draft.source_root, "Modules/Blog/app");
        assert_eq!(draft.migration_root, "Modules/Blog/database/migrations");
        assert!(bundle.apply_module(&mut draft, "unknown").is_err());
    }

    #[test]
    fn schema_reference_is_optional_bounded_and_never_fetched() {
        assert!(parse(r#"{"$schema":"./laravel-generators.schema.json","version":1}"#).is_ok());
        assert!(parse(r#"{"$schema":10,"version":1}"#).is_err());
        assert!(parse(r#"{"$schema":null,"version":1}"#).is_err());
        let config = json!({"version":1,"$schema":"x".repeat(4097)});
        assert!(parse(&config.to_string()).is_err());
        let schema: Value =
            serde_json::from_str(include_str!("../schemas/laravel-generators.schema.json"))
                .unwrap();
        assert_eq!(schema["properties"]["version"]["const"], 1);
        assert_eq!(schema["additionalProperties"], false);
    }

    #[test]
    fn user_storage_is_reviewed_and_project_ids_override_without_template_leaks() {
        let mock = openidea_plugin::mock::MockHost::install();
        let project = mock.invocation().project().unwrap();
        let config = json!({"version":1,"presets":[{"id":"service","label":"Service","files":[{"path":"app/Service.php","template":"service.php"}]}]});
        let templates = BTreeMap::from([("service.php".into(), "<?php // user".into())]);
        assert_eq!(
            save_user(config.clone(), templates.clone()),
            Err(Error::Cancelled)
        );
        assert_eq!(user_document().unwrap()["config"]["presets"], json!([]));
        mock.answer_next_prompt(Some(&["yes"]));
        save_user(config.clone(), templates).unwrap();
        let user = load(&project).unwrap();
        assert_eq!(
            user.render("service", &BTreeMap::new(), None)
                .unwrap()
                .files[0]
                .text,
            "<?php // user"
        );
        mock.add_file(CONFIG, &config.to_string());
        mock.add_file(
            ".openidea/laravel-templates/service.php",
            "<?php // project",
        );
        let local = load(&project).unwrap();
        assert_eq!(local.catalog()["presets"].as_array().unwrap().len(), 1);
        assert_eq!(
            local
                .render("service", &BTreeMap::new(), None)
                .unwrap()
                .files[0]
                .text,
            "<?php // project"
        );
        assert_ne!(user.fingerprint(), local.fingerprint());
        assert!(mock.process_runs().is_empty());
    }
}
#[test]
fn module_roots_cover_database_classes_and_saved_settings_apply_without_overriding_explicit_drafts()
{
    let bundle=from_parts(json!({"version":1,"settings":{"controllerBase":"App\\Http\\Controllers\\BaseController","factoryPostfix":"Maker"},"modules":[{"id":"blog","label":"Blog","namespace":"Modules\\Blog","sourceRoot":"modules/Blog/app","migrationRoot":"modules/Blog/database/migrations","viewRoot":"modules/Blog/resources/views"}]}),BTreeMap::new()).unwrap();
    let mut draft: Draft = serde_json::from_value(
        json!({"name":"Post","table":"posts","migrationStamp":"2026_11_10_120000"}),
    )
    .unwrap();
    bundle.apply_settings(&mut draft);
    bundle.apply_module(&mut draft, "blog").unwrap();
    assert_eq!(draft.settings.factory_postfix, "Maker");
    assert_eq!(draft.layout.factory_root, "modules/Blog/database/factories");
    assert_eq!(
        draft.layout.seeder_namespace,
        "Modules\\Blog\\Database\\Seeders"
    );
    assert_eq!(
        bundle.migration_root_for_source("modules/Blog/app/Models/Post.php"),
        "modules/Blog/database/migrations"
    );
    draft.use_project_settings = false;
    draft.settings.factory_postfix = "Explicit".into();
    bundle.apply_settings(&mut draft);
    assert_eq!(draft.settings.factory_postfix, "Explicit");
}
