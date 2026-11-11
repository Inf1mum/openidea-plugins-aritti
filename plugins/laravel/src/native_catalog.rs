//! Native catalog planning from the project's installed Laravel stubs. Preview
//! reads bounded owned source files; it never executes PHP, Artisan or Composer.
use crate::{
    generator_plan::{self, Output, Plan},
    php,
};
use openidea_plugin::{Error, Project, host};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[path = "native_livewire_namespaces.rs"]
mod livewire_namespaces;
#[cfg(test)]
pub(crate) use livewire_namespaces::tests::fixture as namespaced_fixture;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Request {
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub user_model: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub migration_stamp: Option<String>,
    #[serde(default)]
    pub table: Option<String>,
    #[serde(default)]
    pub migration_operation: Option<String>,
    #[serde(default)]
    pub enum_type: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub view: Option<String>,
    #[serde(default)]
    pub queued: bool,
    #[serde(default)]
    pub api: bool,
    #[serde(default)]
    pub implicit: bool,
    #[serde(default)]
    pub pivot: bool,
    /// Omitted retains installed observer methods; an explicit list selects them.
    #[serde(default)]
    pub methods: Option<Vec<String>>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub component_type: Option<String>,
    #[serde(default)]
    pub emoji: Option<bool>,
    #[serde(default)]
    pub frontend: Option<String>,
    #[serde(default)]
    pub page_root: Option<String>,
    #[serde(default)]
    pub typescript: Option<bool>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Dependency {
    pub name: String,
    pub class: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub(crate) struct Settings {
    pub namespace: String,
    pub application_namespace: String,
    pub source_root: String,
    pub migration_root: String,
    pub view_root: String,
    pub application_view_root: String,
    pub test_root: String,
    pub test_namespace: String,
    pub test_case: String,
    pub vendor_root: String,
    pub type_hints: bool,
    pub constructor_promotion: bool,
    pub kinds: BTreeMap<String, KindSettings>,
    /// Bodies captured by the owning preset bundle, including internally scoped
    /// user templates. The caller rebuilds this map when checking plan evidence.
    pub templates: BTreeMap<String, String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            namespace: "App".into(),
            application_namespace: "App".into(),
            source_root: "app".into(),
            migration_root: "database/migrations".into(),
            view_root: "resources/views".into(),
            application_view_root: "resources/views".into(),
            test_root: "tests".into(),
            test_namespace: "Tests".into(),
            test_case: "Tests\\TestCase".into(),
            vendor_root: "vendor".into(),
            type_hints: true,
            constructor_promotion: true,
            kinds: BTreeMap::new(),
            templates: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub(crate) struct KindSettings {
    pub directory: Option<String>,
    pub namespace: Option<String>,
    pub base_class: Option<String>,
    pub postfix: Option<String>,
    pub template: Option<String>,
}
struct Kind {
    id: &'static str,
    label: &'static str,
    directory: &'static str,
    postfix: &'static str,
    package: Option<&'static str>,
}
const KINDS: &[Kind] = &[
    Kind {
        id: "inertia-page",
        label: "Inertia page",
        directory: "@inertia",
        postfix: "",
        package: Some("inertiajs/inertia-laravel"),
    },
    Kind {
        id: "model",
        label: "Model",
        directory: "Models",
        package: None,
        postfix: "",
    },
    Kind {
        id: "controller",
        label: "Controller",
        directory: "Http/Controllers",
        package: None,
        postfix: "Controller",
    },
    Kind {
        id: "resource-controller",
        label: "Resource controller",
        directory: "Http/Controllers",
        package: None,
        postfix: "Controller",
    },
    Kind {
        id: "channel",
        label: "Broadcast channel",
        directory: "Broadcasting",
        package: None,
        postfix: "",
    },
    Kind {
        id: "command",
        label: "Console command",
        directory: "Console/Commands",
        package: None,
        postfix: "Command",
    },
    Kind {
        id: "event",
        label: "Event",
        directory: "Events",
        package: None,
        postfix: "",
    },
    Kind {
        id: "exception",
        label: "Exception",
        directory: "Exceptions",
        package: None,
        postfix: "Exception",
    },
    Kind {
        id: "http-exception",
        label: "HTTP exception",
        directory: "Exceptions",
        package: None,
        postfix: "Exception",
    },
    Kind {
        id: "factory",
        label: "Database factory",
        directory: "@factory",
        package: None,
        postfix: "Factory",
    },
    Kind {
        id: "feature-test",
        label: "Feature test",
        directory: "@feature",
        package: None,
        postfix: "Test",
    },
    Kind {
        id: "request",
        label: "Form request",
        directory: "Http/Requests",
        package: None,
        postfix: "Request",
    },
    Kind {
        id: "job",
        label: "Job",
        directory: "Jobs",
        package: None,
        postfix: "",
    },
    Kind {
        id: "resource",
        label: "JSON resource",
        directory: "Http/Resources",
        package: None,
        postfix: "Resource",
    },
    Kind {
        id: "listener",
        label: "Event listener",
        directory: "Listeners",
        package: None,
        postfix: "Listener",
    },
    Kind {
        id: "mail",
        label: "Mailable",
        directory: "Mail",
        package: None,
        postfix: "",
    },
    Kind {
        id: "middleware",
        label: "Middleware",
        directory: "Http/Middleware",
        package: None,
        postfix: "",
    },
    Kind {
        id: "migration",
        label: "Database migration",
        directory: "@migration",
        package: None,
        postfix: "",
    },
    Kind {
        id: "notification",
        label: "Notification",
        directory: "Notifications",
        package: None,
        postfix: "",
    },
    Kind {
        id: "observer",
        label: "Model observer",
        directory: "Observers",
        package: None,
        postfix: "Observer",
    },
    Kind {
        id: "policy",
        label: "Policy",
        directory: "Policies",
        package: None,
        postfix: "Policy",
    },
    Kind {
        id: "resource-collection",
        label: "JSON resource collection",
        directory: "Http/Resources",
        package: None,
        postfix: "Collection",
    },
    Kind {
        id: "seeder",
        label: "Database seeder",
        directory: "@seeder",
        package: None,
        postfix: "Seeder",
    },
    Kind {
        id: "provider",
        label: "Service provider",
        directory: "Providers",
        package: None,
        postfix: "ServiceProvider",
    },
    Kind {
        id: "unit-test",
        label: "Unit test",
        directory: "@unit",
        package: None,
        postfix: "Test",
    },
    Kind {
        id: "rule",
        label: "Validation rule",
        directory: "Rules",
        package: None,
        postfix: "",
    },
    Kind {
        id: "view",
        label: "Blade view",
        directory: "@view",
        package: None,
        postfix: "",
    },
    Kind {
        id: "view-composer",
        label: "View composer",
        directory: "View/Composers",
        package: None,
        postfix: "Composer",
    },
    Kind {
        id: "blade-component",
        label: "Blade component",
        directory: "View/Components",
        package: None,
        postfix: "",
    },
    Kind {
        id: "class",
        label: "Class",
        directory: "",
        package: None,
        postfix: "",
    },
    Kind {
        id: "interface",
        label: "Interface",
        directory: "Contracts",
        package: None,
        postfix: "",
    },
    Kind {
        id: "trait",
        label: "Trait",
        directory: "Concerns",
        package: None,
        postfix: "",
    },
    Kind {
        id: "enum",
        label: "Enum",
        directory: "Enums",
        package: None,
        postfix: "",
    },
    Kind {
        id: "cast",
        label: "Eloquent cast",
        directory: "Casts",
        package: None,
        postfix: "",
    },
    Kind {
        id: "scope",
        label: "Eloquent scope",
        directory: "Models/Scopes",
        package: None,
        postfix: "Scope",
    },
    Kind {
        id: "service",
        label: "Service",
        directory: "Services",
        package: None,
        postfix: "Service",
    },
    Kind {
        id: "facade",
        label: "Facade",
        directory: "Facades",
        package: None,
        postfix: "",
    },
    Kind {
        id: "livewire-component",
        label: "Livewire component",
        directory: "Livewire",
        postfix: "",
        package: Some("livewire/livewire"),
    },
    Kind {
        id: "livewire-form",
        label: "Livewire form",
        directory: "Livewire/Forms",
        postfix: "Form",
        package: Some("livewire/livewire"),
    },
    Kind {
        id: "dusk-test",
        label: "Dusk browser test",
        directory: "@dusk-test",
        postfix: "Test",
        package: Some("laravel/dusk"),
    },
    Kind {
        id: "dusk-page",
        label: "Dusk page",
        directory: "@dusk-page",
        postfix: "Page",
        package: Some("laravel/dusk"),
    },
    Kind {
        id: "dusk-component",
        label: "Dusk component",
        directory: "@dusk-component",
        postfix: "",
        package: Some("laravel/dusk"),
    },
    Kind {
        id: "tenant-migration",
        label: "Tenant database migration",
        directory: "@migration",
        postfix: "",
        package: Some("stancl/tenancy"),
    },
];
pub(crate) fn catalog() -> Value {
    json!(KINDS.iter().map(catalog_row).collect::<Vec<_>>())
}
pub(crate) fn kind_ids() -> Vec<&'static str> {
    KINDS.iter().map(|kind| kind.id).collect()
}
fn catalog_row(kind: &Kind) -> Value {
    json!({"id":kind.id,"label":kind.label,"postfix":kind.postfix,"model":matches!(kind.id,"factory"|"observer"|"policy"|"resource-controller"),"event":kind.id=="listener","methods":kind.id=="observer","package":kind.package,"installedFrameworkRequired":true})
}
/// Menu gating uses owned installed package identities. The planner repeats all
/// source/config checks and records exact evidence before any apply action.
pub(crate) fn catalog_for_project(project: &Project, settings: &Settings) -> Result<Value, Error> {
    let mut sources = Sources {
        project,
        root: project.root()?,
        files: BTreeMap::new(),
    };
    let mut rows = Vec::new();
    for kind in KINDS {
        let mut row = catalog_row(kind);
        row["label"] =
            crate::ui_text::t(&format!("project.native.kind.{}", kind.id), kind.label).into();
        if let Some(package) = kind.package {
            let majors: &[u32] = match package {
                "livewire/livewire" => &[2, 3, 4],
                "laravel/dusk" => &[7, 8],
                "stancl/tenancy" => &[3],
                "inertiajs/inertia-laravel" => &[1, 2, 3],
                _ => continue,
            };
            let Ok((_, major)) = sources.package(package, majors, settings) else {
                continue;
            };
            if kind.id == "livewire-form" && major == 2 {
                continue;
            }
            row["packageMajor"] = major.into();
            if kind.id == "inertia-page" {
                let Ok((roots, frontends)) = inertia_choices(&mut sources, settings) else {
                    continue;
                };
                row["pageRoots"] = json!(roots);
                row["frontendVariants"] = json!(frontends);
            }
            if kind.id == "livewire-component" {
                row["componentFormats"] = json!(if major == 4 {
                    vec!["class", "inline", "single-file", "multi-file"]
                } else {
                    vec!["class", "inline"]
                });
                row["emojiAvailable"] = (major == 4).into();
                row["defaultComponentType"] = if major < 4 {
                    json!("class")
                } else {
                    Value::Null
                };
                row["defaultEmoji"] = if major < 4 { json!(false) } else { Value::Null };
                if major == 4 {
                    let package = format!("{}/livewire/livewire", settings.vendor_root);
                    if let Ok(config) = livewire_config(&mut sources, &package) {
                        if let Ok(Some((source, range))) = config.entry(&["make_command", "type"]) {
                            if let Ok(value) = php_string(source, range) {
                                row["defaultComponentType"] = match value.as_str() {
                                    "sfc" => json!("single-file"),
                                    "mfc" => json!("multi-file"),
                                    "class" => json!("class"),
                                    _ => Value::Null,
                                };
                            }
                        }
                        if let Ok(Some((source, range))) = config.entry(&["make_command", "emoji"])
                        {
                            row["defaultEmoji"] = match php::mask_comments(&source[range]).trim() {
                                "true" => json!(true),
                                "false" => json!(false),
                                _ => Value::Null,
                            };
                        }
                    }
                }
            }
        }
        rows.push(row);
    }
    Ok(json!(rows))
}

struct Sources<'a> {
    project: &'a Project,
    root: String,
    files: BTreeMap<String, String>,
}
impl<'a> Sources<'a> {
    fn exists(&mut self, relative: &str) -> Result<bool, Error> {
        let normalized = relative.replace(['\u{fe0e}', '\u{fe0f}'], "");
        if !generator_plan::relative(&normalized) {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.2ee3ede486",
                "Package source paths must stay inside the project",
            )));
        }
        let absolute = format!("{}/{relative}", self.root.trim_end_matches('/'));
        if host::project_of(&absolute)
            .and_then(|p| p.root().ok())
            .as_deref()
            != Some(&self.root)
        {
            return Err(Error::Stale);
        }
        let exists = self.project.exists(relative)?;
        self.files
            .insert(format!("<exists>{relative}"), exists.to_string());
        Ok(exists)
    }
    fn package(
        &mut self,
        name: &str,
        majors: &[u32],
        settings: &Settings,
    ) -> Result<(String, u32), Error> {
        let lock = if let Some(text) = self.files.get("composer.lock") {
            text.clone()
        } else {
            self.read("composer.lock", 2 << 20)?
        };
        let value = crate::strict_json::parse(&lock, 2 << 20)?;
        let found: Vec<_> = ["packages", "packages-dev"]
            .into_iter()
            .flat_map(|key| value[key].as_array().into_iter().flatten())
            .filter(|package| package["name"] == name)
            .collect();
        if found.len() != 1 {
            return Err(invalid(&crate::ui_text::format(
                "project.native.format.b34a4c76ac",
                "Install one locked {name} package before using this native generator",
                &[("name", &name)],
            )));
        }
        let major = found[0]["version"]
            .as_str()
            .and_then(|version| {
                version
                    .trim_start_matches('v')
                    .split('.')
                    .next()?
                    .parse::<u32>()
                    .ok()
            })
            .filter(|major| majors.contains(major))
            .ok_or_else(|| {
                invalid(&crate::ui_text::format(
                    "project.native.format.4f12be8d77",
                    "The installed {name} version has no verified native stub adapter",
                    &[("name", &name)],
                ))
            })?;
        let root = format!("{}/{name}", settings.vendor_root);
        let package = self.read(&format!("{root}/composer.json"), 128 << 10)?;
        let package = crate::strict_json::parse(&package, 128 << 10)?;
        if package["name"] != name {
            return Err(Error::Stale);
        }
        Ok((root, major))
    }
    fn optional(&mut self, relative: &str, limit: usize) -> Result<Option<String>, Error> {
        match self.read(relative, limit) {
            Ok(text) => Ok(Some(text)),
            Err(Error::NotFound(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }
    fn read(&mut self, relative: &str, limit: usize) -> Result<String, Error> {
        if !generator_plan::relative(relative)
            && !relative
                .strip_prefix("node_modules/@inertiajs/")
                .is_some_and(generator_plan::relative)
        {
            return Err(Error::Invalid(crate::ui_text::t(
                "project.native.error.d340cb48b1",
                "Native catalog source path must stay inside the project",
            )));
        }
        let absolute = format!("{}/{relative}", self.root.trim_end_matches('/'));
        if host::project_of(&absolute)
            .and_then(|p| p.root().ok())
            .as_deref()
            != Some(&self.root)
        {
            return Err(Error::Stale);
        }
        if self.project.stat(relative)?.kind != openidea_plugin::FileKind::File {
            return Err(Error::Invalid(crate::ui_text::t(
                "project.native.error.ee8b69de42",
                "Native catalog inputs must be regular files, not symlinks",
            )));
        }
        let text = self.project.read_file(relative)?;
        if text.len() > limit || text.contains('\0') {
            return Err(Error::Invalid(crate::ui_text::format(
                "project.native.format.edea3e4f98",
                "Native catalog source {relative} exceeds supported limits",
                &[("relative", &relative)],
            )));
        }
        self.files.insert(relative.into(), text.clone());
        Ok(text)
    }
    fn framework(&mut self, settings: &Settings) -> Result<(String, u32), Error> {
        let lock = self.read("composer.lock", 2 << 20)?;
        let parsed = crate::strict_json::parse(&lock, 2 << 20)?;
        let packages = parsed["packages"].as_array().ok_or_else(|| {
            Error::Invalid(crate::ui_text::t(
                "project.native.error.11ddd1728c",
                "Install a locked laravel/framework dependency before native generation",
            ))
        })?;
        let packages: Vec<_> = packages
            .iter()
            .filter(|package| package["name"] == "laravel/framework")
            .collect();
        if packages.len() != 1 {
            return Err(Error::Invalid(crate::ui_text::t(
                "project.native.error.db6c9a66c4",
                "Exactly one locked laravel/framework package is required",
            )));
        }
        let version = packages[0]["version"]
            .as_str()
            .ok_or(Error::Stale)?
            .trim_start_matches('v');
        let major = version
            .split('.')
            .next()
            .and_then(|major| major.parse::<u32>().ok())
            .filter(|major| (10..=13).contains(major))
            .ok_or_else(|| {
                Error::Invalid(crate::ui_text::t(
                    "project.native.error.2c363cf178",
                    "Native catalog currently supports installed Laravel 10–13 stubs",
                ))
            })?;
        let root = format!("{}/laravel/framework/src/Illuminate", settings.vendor_root);
        let application = self.read(&format!("{root}/Foundation/Application.php"), 128 << 10)?;
        let marker = "const VERSION = ";
        let rest = application
            .split_once(marker)
            .map(|(_, rest)| rest.trim_start())
            .ok_or(Error::Stale)?;
        let quote = rest
            .as_bytes()
            .first()
            .copied()
            .filter(|quote| matches!(quote, b'\'' | b'"'))
            .ok_or(Error::Stale)?;
        let end = rest[1..].find(char::from(quote)).ok_or(Error::Stale)? + 1;
        if &rest[1..end] != version {
            return Err(Error::Invalid(crate::ui_text::t(
                "project.native.error.ea9243c610",
                "Installed Laravel sources disagree with composer.lock; reinstall dependencies before generating",
            )));
        }
        Ok((root, major))
    }
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

fn config_entry(source: &str, keys: &[&str]) -> Result<Option<std::ops::Range<usize>>, Error> {
    let mut array = php::returned_array(source).ok_or_else(|| {
        invalid(&crate::ui_text::t(
            "project.native.error.557c522b5d",
            "A dynamically built package config needs explicit native generator settings",
        ))
    })?;
    for (index, key) in keys.iter().enumerate() {
        let matches: Vec<_> = php::items(source, array)
            .into_iter()
            .filter(|item| {
                item.key
                    .as_ref()
                    .and_then(|range| php::literal(source, range.clone()))
                    .is_some_and(|(name, _)| name == *key)
            })
            .collect();
        if matches.len() > 1 {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.355b4e0b8c",
                "Duplicate package config keys are ambiguous",
            )));
        }
        let Some(item) = matches.into_iter().next() else {
            return Ok(None);
        };
        if index + 1 == keys.len() {
            return Ok(Some(item.value));
        }
        array = php::array_open(source, item.value).ok_or_else(|| {
            invalid(&crate::ui_text::t(
                "project.native.error.a50e5118cc",
                "A dynamic package config branch needs an explicit generator choice",
            ))
        })?;
    }
    Ok(None)
}
fn php_string(source: &str, range: std::ops::Range<usize>) -> Result<String, Error> {
    let (raw, offset) = php::literal(source, range).ok_or_else(|| {
        invalid(&crate::ui_text::t(
            "project.native.error.9170949dc7",
            "Use literal package paths/namespaces or explicit generator settings",
        ))
    })?;
    if raw.contains('$') && source.as_bytes().get(offset - 1) == Some(&b'"') {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.784df03cc6",
            "Interpolated package config strings require an explicit generator setting",
        )));
    }
    let mut output = String::new();
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let next = chars.next().ok_or(Error::Stale)?;
            if matches!(next, '\\' | '\'' | '"') {
                output.push(next);
            } else {
                output.push(ch);
                output.push(next);
            }
        } else {
            output.push(ch);
        }
    }
    Ok(output)
}
fn config_path(source: &str, range: std::ops::Range<usize>, root: &str) -> Result<String, Error> {
    let range = php::trimmed(source, range);
    let fragment = &source[range.clone()];
    let path = if php::literal(source, range.clone()).is_some() {
        let value = php_string(source, range)?;
        value
            .strip_prefix(&format!("{root}/"))
            .unwrap_or(&value)
            .to_owned()
    } else {
        let mut result = None;
        for (helper, prefix) in [
            ("resource_path", "resources"),
            ("database_path", "database"),
            ("app_path", "app"),
            ("base_path", ""),
        ] {
            if let Some(tail) = fragment.strip_prefix(helper) {
                let tail = tail.trim_start();
                if tail.starts_with('(') && php::closing(tail, 0) == Some(tail.len() - 1) {
                    let args = php::arguments(tail, 0);
                    let suffix = if args.is_empty() {
                        String::new()
                    } else if args.len() == 1 {
                        php_string(tail, args[0].clone())?
                    } else {
                        return Err(invalid(&crate::ui_text::t(
                            "project.native.error.66c54d15e4",
                            "Package path helpers accept one literal argument",
                        )));
                    };
                    result = Some(if suffix.is_empty() {
                        prefix.into()
                    } else if prefix.is_empty() {
                        suffix
                    } else {
                        format!("{prefix}/{suffix}")
                    });
                    break;
                }
            }
        }
        result.ok_or_else(|| {
            invalid(&crate::ui_text::t(
                "project.native.error.39e36266b4",
                "Dynamic package paths require explicit native generator settings",
            ))
        })?
    };
    if !generator_plan::relative(&path) {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.01b477fff7",
            "Package generation paths must remain inside the project",
        )));
    }
    Ok(path)
}
struct ConfigSources {
    project: Option<String>,
    defaults: String,
}
impl ConfigSources {
    fn entry<'a>(
        &'a self,
        keys: &[&str],
    ) -> Result<Option<(&'a str, std::ops::Range<usize>)>, Error> {
        for source in self.project.iter().chain(std::iter::once(&self.defaults)) {
            if let Some(range) = config_entry(source, keys)? {
                return Ok(Some((source, range)));
            }
        }
        Ok(None)
    }
}
fn livewire_config(sources: &mut Sources, package: &str) -> Result<ConfigSources, Error> {
    Ok(ConfigSources {
        project: sources.optional("config/livewire.php", 128 << 10)?,
        defaults: sources.read(&format!("{package}/config/livewire.php"), 128 << 10)?,
    })
}
fn legacy_template(body: &str, values: &BTreeMap<String, String>) -> Result<String, Error> {
    if body.len() > 128 << 10 || body.contains('\0') {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.e835674e32",
            "Package template exceeds supported limits",
        )));
    }
    let mut result = body.to_owned();
    for (key, value) in values {
        result = result.replace(&format!("[{key}]"), value);
        match key.as_str() {
            "namespace" => result = result.replace("DummyNamespace", value),
            "class" => result = result.replace("DummyClass", value),
            _ => {}
        }
    }
    let mut rest = result.as_str();
    while let Some(at) = rest.find('[') {
        let suffix = &rest[at + 1..];
        let Some(end) = suffix.find(']') else { break };
        let name = &suffix[..end];
        if !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_alphabetic()) {
            return Err(invalid(&crate::ui_text::format(
                "project.native.format.c0314ab2b4",
                "Unsupported package template placeholder [{name}]",
                &[("name", &name)],
            )));
        }
        rest = &suffix[end + 1..];
    }
    if result.contains("DummyNamespace") || result.contains("DummyClass") {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.620593d5eb",
            "Unresolved package template placeholders",
        )));
    }
    Ok(result)
}
fn package_output(
    sources: &Sources,
    request: &Request,
    settings: &Settings,
    files: Vec<Output>,
) -> Result<(Plan, String), Error> {
    let mut paths = BTreeSet::new();
    for file in &files {
        if !generator_plan::relative(&file.path)
            || file.text.len() > 128 << 10
            || !paths.insert(file.path.to_ascii_lowercase())
        {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.89766de97a",
                "Package outputs conflict or exceed supported limits",
            )));
        }
        let absolute = format!("{}/{}", sources.root, file.path);
        if host::project_of(&absolute)
            .and_then(|p| p.root().ok())
            .as_deref()
            != Some(&sources.root)
        {
            return Err(Error::Stale);
        }
    }
    let evidence =
        serde_json::to_string(&(request, settings, &sources.files)).map_err(|_| Error::Stale)?;
    Ok((Plan { files }, evidence))
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InertiaRoot {
    pub path: String,
    pub prefix: String,
    pub suffix: String,
}
fn inertia_choices(
    sources: &mut Sources<'_>,
    settings: &Settings,
) -> Result<(Vec<InertiaRoot>, Vec<String>), Error> {
    sources.package("inertiajs/inertia-laravel", &[1, 2, 3], settings)?;
    let mut metadata_files = sources.project.find_files("**/ide.json", 129)?;
    if metadata_files.len() > 128 {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.607c44d2c4",
            "Inertia metadata file budget exceeded",
        )));
    }
    metadata_files.extend([
        format!("{}/ide.json", sources.root),
        format!("{}/.idea/ide.json", sources.root),
    ]);
    metadata_files.sort();
    metadata_files.dedup();
    for path in metadata_files {
        let relative = path
            .strip_prefix(&format!("{}/", sources.root))
            .unwrap_or(&path);
        if sources.exists(relative)? {
            sources.read(relative, 1 << 20)?;
        }
    }
    let metadata = crate::metadata::Metadata::load(sources.project);
    if !metadata.package_enabled("inertiajs/inertia-laravel", true) {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.1969501fb5",
            "Inertia is disabled by project metadata",
        )));
    }
    let mut roots: Vec<_> = metadata
        .inertia_paths
        .into_iter()
        .map(|r| InertiaRoot {
            path: r.path,
            prefix: r.prefix,
            suffix: r.suffix,
        })
        .collect();
    if !metadata.inertia_ignore_settings {
        for path in ["resources/js/Pages", "resources/js/pages"] {
            if sources.exists(path)? && !roots.iter().any(|r| r.path == path) {
                roots.push(InertiaRoot {
                    path: path.into(),
                    prefix: String::new(),
                    suffix: String::new(),
                });
            }
        }
    }
    roots.retain(|r| generator_plan::relative(&r.path));
    let package = crate::strict_json::parse(&sources.read("package.json", 256 << 10)?, 256 << 10)?;
    let mut frontends = Vec::new();
    for (id, name) in [
        ("vue", "@inertiajs/vue3"),
        ("react", "@inertiajs/react"),
        ("svelte", "@inertiajs/svelte"),
    ] {
        if ["dependencies", "devDependencies"]
            .iter()
            .any(|key| package[*key].get(name).is_some_and(Value::is_string))
        {
            let path = format!("node_modules/{name}/package.json");
            if let Some(text) = sources.optional(&path, 128 << 10)? {
                let installed = crate::strict_json::parse(&text, 128 << 10)?;
                if installed["name"] == name
                    && installed["version"]
                        .as_str()
                        .and_then(|v| v.split('.').next())
                        .is_some_and(|v| matches!(v, "1" | "2" | "3"))
                {
                    frontends.push(id.into());
                }
            }
        }
    }
    if roots.is_empty() || frontends.is_empty() {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.82f1c875ab",
            "Configure an owned Inertia page root and install a supported frontend adapter before generation",
        )));
    }
    Ok((roots, frontends))
}
fn inertia_plan(
    project: &Project,
    request: &Request,
    settings: &Settings,
) -> Result<(Plan, String), Error> {
    if request.component_type.is_some()
        || request.emoji.is_some()
        || request.model.is_some()
        || request.event.is_some()
        || request.methods.is_some()
        || !request.dependencies.is_empty()
        || request.queued
        || request.api
        || request.implicit
        || request.pivot
    {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.4253793eb7",
            "These options do not apply to Inertia pages",
        )));
    }
    let mut sources = Sources {
        project,
        root: project.root()?,
        files: BTreeMap::new(),
    };
    let (roots, frontends) = inertia_choices(&mut sources, settings)?;
    let root = if let Some(selected) = &request.page_root {
        let matches: Vec<_> = roots.iter().filter(|r| &r.path == selected).collect();
        if matches.len() != 1 {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.361fbf2a99",
                "Choose one unambiguous configured Inertia page root",
            )));
        }
        matches[0]
    } else {
        if roots.len() != 1 {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.73c7bd34fc",
                "Choose an Inertia page root when several are configured",
            )));
        }
        &roots[0]
    };
    let frontend = if let Some(selected) = &request.frontend {
        if !frontends.contains(selected) {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.b220a2afa7",
                "Install the selected Inertia frontend adapter",
            )));
        }
        selected.as_str()
    } else {
        if frontends.len() != 1 {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.93dd152546",
                "Choose a frontend when several Inertia adapters are installed",
            )));
        }
        &frontends[0]
    };
    let logical = request
        .name
        .strip_prefix(&root.prefix)
        .and_then(|name| name.strip_suffix(&root.suffix))
        .ok_or_else(|| {
            invalid(&crate::ui_text::t(
                "project.native.error.99b9b9ef49",
                "The page name must match the configured Inertia prefix and suffix",
            ))
        })?;
    if !generator_plan::relative(logical) || logical.split('/').any(|p| p.contains('.')) {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.0486b5a54b",
            "Use an extensionless relative Inertia page name",
        )));
    }
    let typed = request.typescript.unwrap_or(true);
    let (extension,text)=match frontend {
        "vue"=>("vue",format!("<script setup{}>\nimport {{ Head }} from '@inertiajs/vue3';\n</script>\n\n<template>\n    <Head title=\"Page\" />\n    <main></main>\n</template>\n",if typed{" lang=\"ts\""}else{""})),
        "react"=>(if typed{"tsx"}else{"jsx"},"import { Head } from '@inertiajs/react';\n\nexport default function Page() {\n    return (\n        <>\n            <Head title=\"Page\" />\n            <main />\n        </>\n    );\n}\n".into()),
        "svelte"=>("svelte",format!("<script{}>\n</script>\n\n<svelte:head>\n    <title>Page</title>\n</svelte:head>\n<main></main>\n",if typed{" lang=\"ts\""}else{""})),
        _=>return Err(Error::Stale)
    };
    let path = format!("{}/{logical}.{extension}", root.path);
    if !generator_plan::relative(&path) || sources.exists(&path)? {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.07fbc4cdc7",
            "The Inertia page already exists or its destination is invalid",
        )));
    }
    let override_options = settings
        .kinds
        .get("inertia-page")
        .cloned()
        .unwrap_or_default();
    if override_options
        .directory
        .as_deref()
        .is_some_and(|p| p != root.path)
        || override_options.namespace.is_some()
        || override_options.base_class.is_some()
        || override_options.postfix.is_some()
        || override_options.template.is_some()
    {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.fb75d67aa4",
            "Use configured Inertia page roots; custom page templates require an explicit preset",
        )));
    }
    Ok((
        Plan {
            files: vec![Output { path, text }],
        },
        serde_json::to_string(&sources.files).map_err(|_| Error::Stale)?,
    ))
}

fn package_plan(
    project: &Project,
    request: &Request,
    settings: &Settings,
    kind: &Kind,
) -> Result<(Plan, String), Error> {
    if request.queued
        || request.api
        || request.implicit
        || request.pivot
        || request.methods.is_some()
        || !request.dependencies.is_empty()
        || request.model.is_some()
        || request.event.is_some()
        || request.command.is_some()
    {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.9900cc7ff6",
            "These options do not apply to the package generator",
        )));
    }
    if !generator_plan::qualified(&settings.namespace)
        || !generator_plan::qualified(&settings.test_namespace)
        || [
            &settings.source_root,
            &settings.view_root,
            &settings.application_view_root,
            &settings.test_root,
            &settings.vendor_root,
        ]
        .iter()
        .any(|path| !generator_plan::relative(path))
    {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.44d698c72b",
            "Package generator roots/namespaces are invalid",
        )));
    }
    let mut sources = Sources {
        project,
        root: project.root()?,
        files: BTreeMap::new(),
    };
    let _framework = sources.framework(settings)?;
    let overrides = settings.kinds.get(kind.id).cloned().unwrap_or_default();
    if overrides.template.is_some() || overrides.base_class.is_some() {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.3c78871b9d",
            "Package stub/base overrides need an explicit custom preset; only verified installed package stubs are adapted here",
        )));
    }
    if kind.id == "tenant-migration" {
        let (package, _) = sources.package("stancl/tenancy", &[3], settings)?;
        let defaults = sources.read(&format!("{package}/assets/config.php"), 128 << 10)?;
        let config = ConfigSources {
            project: sources.optional("config/tenancy.php", 128 << 10)?,
            defaults,
        };
        let directory = if let Some(path) = overrides.directory {
            path
        } else {
            let (source, range) = config
                .entry(&["migration_parameters", "--path"])?
                .ok_or_else(|| {
                    invalid(&crate::ui_text::t(
                        "project.native.error.c8d632104d",
                        "Tenant migration paths are not configured",
                    ))
                })?;
            let array=php::array_open(source,range).ok_or_else(||invalid(&crate::ui_text::t("project.native.error.2c8e5466cf", "Select one explicit tenant migration directory for a dynamic path configuration")))?;
            let paths = php::items(source, array);
            if paths.len() != 1 {
                return Err(invalid(&crate::ui_text::t(
                    "project.native.error.5c9c271e49",
                    "Configure one native tenant migration destination when the package has multiple paths",
                )));
            }
            config_path(source, paths[0].value.clone(), &sources.root)?
        };
        if !generator_plan::relative(&directory) {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.f615230f08",
                "Tenant migrations must stay inside the project",
            )));
        }
        let mut adapted = request.clone();
        adapted.kind = "migration".into();
        let mut settings = settings.clone();
        settings.kinds.insert(
            "migration".into(),
            KindSettings {
                directory: Some(directory),
                namespace: None,
                base_class: None,
                postfix: overrides.postfix,
                template: None,
            },
        );
        let (plan, base_evidence) = plan(project, &adapted, &settings)?;
        let evidence =
            serde_json::to_string(&(base_evidence, sources.files)).map_err(|_| Error::Stale)?;
        return Ok((plan, evidence));
    }
    if request.table.is_some()
        || request.migration_stamp.is_some()
        || request.migration_operation.is_some()
        || request.enum_type.is_some()
    {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.41602184a1",
            "Migration/enum options do not apply to this package generator",
        )));
    }
    if kind.id.starts_with("dusk-") {
        if request.component_type.is_some() || request.emoji.is_some() || request.view.is_some() {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.9009df7998",
                "Component options do not apply to Dusk generators",
            )));
        }
        let (package, _) = sources.package("laravel/dusk", &[7, 8], settings)?;
        let suffix = match kind.id {
            "dusk-test" => "",
            "dusk-page" => "/Pages",
            "dusk-component" => "/Components",
            _ => return Err(Error::Stale),
        };
        let directory = overrides
            .directory
            .unwrap_or_else(|| format!("{}/Browser{suffix}", settings.test_root));
        let namespace = overrides.namespace.unwrap_or_else(|| {
            format!(
                "{}\\Browser{}",
                settings.test_namespace,
                suffix.replace('/', "\\")
            )
        });
        let name = qualified(&request.name)?;
        let (subdir, leaf) = name.rsplit_once('\\').unwrap_or(("", name.as_str()));
        let postfix = overrides.postfix.as_deref().unwrap_or(kind.postfix);
        let class = if leaf.ends_with(postfix) {
            leaf.into()
        } else {
            format!("{leaf}{postfix}")
        };
        let namespace = if subdir.is_empty() {
            namespace
        } else {
            format!("{namespace}\\{subdir}")
        };
        if !generator_plan::qualified(&namespace) || !generator_plan::identifier(&class) {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.16f27d2c37",
                "Dusk class namespace/name is invalid",
            )));
        }
        let stub = match kind.id {
            "dusk-test" => "test.stub",
            "dusk-page" => "page.stub",
            _ => "component.stub",
        };
        let body = sources.read(&format!("{package}/src/Console/stubs/{stub}"), 128 << 10)?;
        let mut body = legacy_template(
            &body,
            &BTreeMap::from([
                ("namespace".into(), namespace),
                ("class".into(), class.clone()),
            ]),
        )?;
        if kind.id == "dusk-test" {
            body = body.replace(
                "use Tests\\DuskTestCase;",
                &format!(
                    "use {}\\DuskTestCase;",
                    settings.test_namespace.trim_start_matches('\\')
                ),
            );
            sources.optional(
                &format!("{}/DuskTestCase.php", settings.test_root),
                128 << 10,
            )?;
        }
        if kind.id == "dusk-page" {
            let existing = sources.optional(
                &format!("{}/Browser/Pages/Page.php", settings.test_root),
                128 << 10,
            )?;
            if let Some(source) = &existing {
                let masked = php::mask_comments(source);
                if !php::declares_class(&masked, "Page")
                    || !masked.contains(&format!(
                        "namespace {}\\Browser\\Pages;",
                        settings.test_namespace.trim_start_matches('\\')
                    ))
                {
                    return Err(invalid(&crate::ui_text::t(
                        "project.native.error.2c13c46c77",
                        "The existing Dusk Page base needs a matching class/namespace declaration",
                    )));
                }
            }
            let base = if existing.is_some() {
                format!("{}\\Browser\\Pages\\Page", settings.test_namespace)
            } else {
                "Laravel\\Dusk\\Page".into()
            };
            body = body.replace(
                "extends Page",
                &format!("extends \\{}", base.trim_start_matches('\\')),
            );
            sources.read(&format!("{package}/src/Page.php"), 128 << 10)?;
        }
        if kind.id == "dusk-component" {
            sources.read(&format!("{package}/src/Component.php"), 128 << 10)?;
        }
        if !settings.type_hints {
            body = without_types(&body, &BTreeSet::new())?;
        }
        body = normalize_imports(&body, &class)?;
        let path = if subdir.is_empty() {
            format!("{directory}/{class}.php")
        } else {
            format!("{directory}/{}/{class}.php", subdir.replace('\\', "/"))
        };
        return package_output(
            &sources,
            request,
            settings,
            vec![Output { path, text: body }],
        );
    }
    let (package, major) = sources.package("livewire/livewire", &[2, 3, 4], settings)?;
    sources.read(&format!("{package}/src/Component.php"), 128 << 10)?;
    let config = livewire_config(&mut sources, &package)?;
    let command_root = if major == 2 {
        format!("{package}/src/Commands")
    } else {
        format!("{package}/src/Features/SupportConsoleCommands/Commands")
    };
    if kind.id == "livewire-form" && major == 2 {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.5f3c176057",
            "Livewire form objects require Livewire 3 or 4",
        )));
    }
    let (tag_namespace, local_name) = if let Some((namespace, local)) = request.name.split_once("::") {
        if major != 4 || kind.id != "livewire-component" || local.contains("::") { return Err(Error::Stale); }
        (Some(namespace), local)
    } else { (None, request.name.as_str()) };
    let selected_module = settings.namespace.trim_start_matches('\\') != settings.application_namespace.trim_start_matches('\\');
    let namespaces = livewire_namespaces::capture(&mut sources, settings, &config, &package, major, tag_namespace.is_some() || selected_module)?;
    let registration = namespaces.selected(tag_namespace)?;
    let pieces: Vec<_> = local_name.split(['.', '/', '\\']).collect();
    if pieces.is_empty()
        || pieces.iter().any(|part| {
            part.is_empty()
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
    {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.a9b075f59c",
            "Use a relative dotted or qualified Livewire component name",
        )));
    }
    let studly: Vec<String> = pieces
        .iter()
        .map(|part| {
            part.split(['_', '-'])
                .filter(|part| !part.is_empty())
                .map(|part| {
                    let mut part = part.to_owned();
                    part[..1].make_ascii_uppercase();
                    part
                })
                .collect()
        })
        .collect();
    if studly.iter().any(|part| !generator_plan::identifier(part)) {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.d4161fcca5",
            "Livewire class names must be valid identifiers",
        )));
    }
    let slugs: Vec<_> = studly
        .iter()
        .map(|part| snake(part).replace('_', "-"))
        .collect();
    let mut class = studly.last().cloned().ok_or(Error::Stale)?;
    let postfix = overrides.postfix.as_deref().unwrap_or(kind.postfix);
    if !postfix.is_empty() && !class.ends_with(postfix) {
        if registration.is_some() { return Err(Error::Stale); }
        class.push_str(postfix);
    }
    if !generator_plan::identifier(&class) {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.90ea7a7f7f",
            "Invalid Livewire class postfix",
        )));
    }
    let subtype = if kind.id == "livewire-form" {
        "class".into()
    } else if let Some(format) = &request.component_type {
        format.clone()
    } else if major == 4 {
        let (source, range) = config
            .entry(&["make_command", "type"])?
            .ok_or(Error::Stale)?;
        match php_string(source, range)?.as_str() {
            "sfc" => "single-file",
            "mfc" => "multi-file",
            "class" => "class",
            _ => {
                return Err(invalid(&crate::ui_text::t(
                    "project.native.error.6590591184",
                    "Unknown configured Livewire component format",
                )));
            }
        }
        .into()
    } else {
        "class".into()
    };
    if !["class", "inline", "single-file", "multi-file"].contains(&subtype.as_str())
        || major < 4 && matches!(subtype.as_str(), "single-file" | "multi-file")
    {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.4455e582ef",
            "This component format is unavailable in the installed Livewire version",
        )));
    }
    let quote = "Generated Livewire component.";
    let values = |namespace: String, view: String| {
        BTreeMap::from([
            ("namespace".into(), namespace),
            ("class".into(), class.clone()),
            ("view".into(), view),
            ("quote".into(), quote.into()),
        ])
    };
    if matches!(subtype.as_str(), "single-file" | "multi-file") {
        let mut locations = if let Some(registration) = registration {
            vec![registration.view_path.clone().ok_or(Error::Stale)?]
        } else {
        let (source, range) = config
            .entry(&["component_locations"])?
            .ok_or(Error::Stale)?;
        let array = php::array_open(source, range).ok_or_else(|| {
            invalid(&crate::ui_text::t(
                "project.native.error.35c595bd28",
                "Dynamic Livewire component locations need a verified Finder namespace adapter",
            ))
        })?;
        let locations = php::items(source, array)
            .into_iter()
            .map(|entry| config_path(source, entry.value, &sources.root))
            .collect::<Result<Vec<_>, _>>()?;
            locations
        };
        if locations.is_empty() {
            locations.push(format!("{}/components", settings.application_view_root));
        }
        let module = settings.namespace.trim_start_matches('\\')
            != settings.application_namespace.trim_start_matches('\\');
        let within_module = |path: &str| {
            path == settings.view_root || path.starts_with(&format!("{}/", settings.view_root))
        };
        let directory = if let Some(directory) = overrides.directory {
            directory
        } else if module {
            locations
                .iter()
                .find(|path| within_module(path))
                .cloned()
                .ok_or_else(|| {
                    invalid(&crate::ui_text::t(
                        "project.native.error.2d18ca972f",
                        "The selected module has no registered literal Livewire component location",
                    ))
                })?
        } else {
            locations[0].clone()
        };
        if !locations.contains(&directory) || module && !within_module(&directory) {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.99c27eac33",
                "The component directory is not a registered location for the selected application/module",
            )));
        }
        let finder = sources.read(&format!("{package}/src/Finder/Finder.php"), 128 << 10)?;
        if !finder.contains("const ZAP = \"\\u{26A1}\"") {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.309e8bad82",
                "The installed Livewire Finder uses an unverified component filename convention",
            )));
        }
        let emoji = if let Some(emoji) = request.emoji {
            emoji
        } else {
            let (source, range) = config
                .entry(&["make_command", "emoji"])?
                .ok_or(Error::Stale)?;
            match php::mask_comments(&source[range]).trim() {
                "true" => true,
                "false" => false,
                _ => {
                    return Err(invalid(&crate::ui_text::t(
                        "project.native.error.978c9927d3",
                        "A dynamic emoji setting needs an explicit generator choice",
                    )));
                }
            }
        };
        let prefix = if emoji { "⚡" } else { "" };
        let slug = slugs.last().ok_or(Error::Stale)?;
        let leading = slugs[..slugs.len() - 1].join("/");
        for location in &locations {
            let location = if leading.is_empty() {
                location.clone()
            } else {
                format!("{location}/{leading}")
            };
            for prefix in ["", "⚡", "⚡\u{fe0e}", "⚡\u{fe0f}"] {
                if sources.exists(&format!("{location}/{prefix}{slug}.blade.php"))?
                    || sources.exists(&format!("{location}/{prefix}{slug}"))?
                {
                    return Err(invalid(&crate::ui_text::t(
                        "project.native.error.a5dcd2687f",
                        "A view-based Livewire component already uses this name; conversion/overwrite is not implicit",
                    )));
                }
            }
        }
        let base = if leading.is_empty() {
            directory
        } else {
            format!("{directory}/{leading}")
        };
        let mut files = Vec::new();
        if subtype == "single-file" {
            let body = sources.read(&format!("{command_root}/livewire-sfc.stub"), 128 << 10)?;
            files.push(Output {
                path: format!("{base}/{prefix}{slug}.blade.php"),
                text: legacy_template(&body, &values(String::new(), String::new()))?,
            });
        } else {
            let folder = format!("{base}/{prefix}{slug}");
            for (stub, extension) in [
                ("livewire-mfc-class.stub", "php"),
                ("livewire-mfc-view.stub", "blade.php"),
            ] {
                let body = sources.read(&format!("{command_root}/{stub}"), 128 << 10)?;
                files.push(Output {
                    path: format!("{folder}/{slug}.{extension}"),
                    text: legacy_template(&body, &values(String::new(), String::new()))?,
                });
            }
        }
        return package_output(&sources, request, settings, files);
    }
    if request.emoji == Some(true) {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.30fd738cd6",
            "Emoji prefixes apply only to Livewire 4 view-based components",
        )));
    }
    let default_namespace = if kind.id == "livewire-form" {
        format!("{}\\Livewire\\Forms", settings.namespace)
    } else if let Some(registration) = registration {
        registration.class_namespace.clone().ok_or(Error::Stale)?
    } else if let Some((source, range)) = config.entry(&["class_namespace"])? {
        qualified(&php_string(source, range)?)?
    } else {
        format!(
            "{}\\{}",
            settings.namespace,
            if major == 2 {
                "Http\\Livewire"
            } else {
                "Livewire"
            }
        )
    };
    let namespace = overrides
        .namespace
        .unwrap_or_else(|| default_namespace.clone())
        .trim_start_matches('\\')
        .to_owned();
    if kind.id == "livewire-component" && namespace != default_namespace {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.d65180f880",
            "The component namespace is outside the configured Livewire class namespace",
        )));
    }
    if namespace != settings.namespace.trim_start_matches('\\')
        && !namespace.starts_with(&format!(
            "{}\\",
            settings.namespace.trim_start_matches('\\')
        ))
    {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.ac34d29255",
            "Livewire class configuration does not match the selected application/module namespace",
        )));
    }
    let directory = if let Some(registration) = registration {
        let registered = registration.class_path.clone().ok_or(Error::Stale)?;
        let prefix = settings.namespace.trim_start_matches('\\');
        let expected = if namespace == prefix { settings.source_root.clone() } else {
            format!("{}/{}", settings.source_root, namespace.strip_prefix(&format!("{prefix}\\")).ok_or(Error::Stale)?.replace('\\',"/"))
        };
        if registered != expected { return Err(Error::Stale); }
        if overrides.directory.as_ref().is_some_and(|directory| directory != &registered) { return Err(Error::Stale); }
        registered
    } else if let Some(directory) = overrides.directory {
        directory
    } else {
        let suffix = if namespace == settings.namespace.trim_start_matches('\\') {
            ""
        } else {
            namespace.strip_prefix(&format!("{}\\",settings.namespace.trim_start_matches('\\'))).ok_or_else(||invalid(&crate::ui_text::t("project.native.error.120e81ed45", "Configure an explicit directory for a Livewire namespace outside the application mapping")))?
        };
        if suffix.is_empty() {
            settings.source_root.clone()
        } else {
            format!("{}/{}", settings.source_root, suffix.replace('\\', "/"))
        }
    };
    let namespace = if studly.len() > 1 {
        format!("{namespace}\\{}", studly[..studly.len() - 1].join("\\"))
    } else {
        namespace
    };
    let class_directory = if studly.len() > 1 {
        format!("{directory}/{}", studly[..studly.len() - 1].join("/"))
    } else {
        directory
    };
    if !generator_plan::qualified(&namespace) {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.510d82e3cf",
            "Livewire namespace is invalid",
        )));
    }
    if kind.id == "livewire-form" {
        let body = sources.read(&format!("{command_root}/livewire.form.stub"), 128 << 10)?;
        let body = legacy_template(&body, &values(namespace, String::new()))?;
        let body = normalize_imports(&body, &class)?;
        sources.read(&format!("{package}/src/Form.php"), 128 << 10)?;
        return package_output(
            &sources,
            request,
            settings,
            vec![Output {
                path: format!("{class_directory}/{class}.php"),
                text: body,
            }],
        );
    }
    if subtype == "inline" {
        if class == "Component" {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.2351575f01",
                "A Livewire component cannot use the imported base class name Component",
            )));
        }
        let body = sources.read(&format!("{command_root}/livewire.inline.stub"), 128 << 10)?;
        return package_output(
            &sources,
            request,
            settings,
            vec![Output {
                path: format!("{class_directory}/{class}.php"),
                text: legacy_template(&body, &values(namespace, String::new()))?,
            }],
        );
    }
    let view_directory = if let Some(registration) = registration {
        registration.class_view_path.clone().ok_or(Error::Stale)?
    } else if let Some((source, range)) = config.entry(&["view_path"])? {
        config_path(source, range, &sources.root)?
    } else {
        format!("{}/livewire", settings.view_root)
    };
    if settings.namespace.trim_start_matches('\\')
        != settings.application_namespace.trim_start_matches('\\')
        && view_directory != settings.view_root
        && !view_directory.starts_with(&format!("{}/", settings.view_root))
    {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.c8da242c6c",
            "Livewire view configuration does not match the selected module's view root",
        )));
    }
    let view_prefix = namespaces.view_prefix(&view_directory,settings)?;
    let expected_view = livewire_namespaces::view_key(&view_prefix,&slugs);
    let view_key = request.view.clone().unwrap_or_else(|| expected_view.clone());
    if registration.is_some() && view_key != expected_view { return Err(Error::Stale); }
    if let Some((namespace, local)) = view_key.split_once("::") {
        let application_override = format!("{}/vendor/{}/{}.blade.php", settings.application_view_root, namespace, local.replace('.',"/"));
        if sources.exists(&application_override)? { return Err(Error::Stale); }
    }
    if let Some((namespace, local)) = view_key.split_once("::") { view_name(namespace)?; view_name(local)?; } else { view_name(&view_key)?; }
    let stub = if subtype == "inline" {
        "livewire.inline.stub"
    } else {
        "livewire.stub"
    };
    let body = sources.read(&format!("{command_root}/{stub}"), 128 << 10)?;
    let body = legacy_template(&body, &values(namespace, view_key))?;
    let body = if subtype == "inline" {
        if class == "Component" {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.2351575f01",
                "A Livewire component cannot use the imported base class name Component",
            )));
        }
        body
    } else {
        normalize_imports(&body, &class)?
    };
    let mut files = vec![Output {
        path: format!("{class_directory}/{class}.php"),
        text: body,
    }];
    if subtype == "class" {
        let body = sources.read(&format!("{command_root}/livewire.view.stub"), 128 << 10)?;
        files.push(Output {
            path: format!("{view_directory}/{}.blade.php", slugs.join("/")),
            text: legacy_template(&body, &values(String::new(), String::new()))?,
        });
    }
    package_output(&sources, request, settings, files)
}
fn qualified(value: &str) -> Result<String, Error> {
    if generator_plan::qualified(value) {
        Ok(value.trim_start_matches('\\').into())
    } else {
        Err(invalid(&crate::ui_text::t(
            "project.native.error.b2299c8e07",
            "Enter a qualified PHP class name",
        )))
    }
}
fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}
fn short(value: &str) -> &str {
    value.rsplit('\\').next().unwrap_or(value)
}
fn snake(value: &str) -> String {
    let mut result = String::new();
    let bytes = value.as_bytes();
    for (at, byte) in bytes.iter().copied().enumerate() {
        if byte.is_ascii_uppercase()
            && at > 0
            && (bytes[at - 1].is_ascii_lowercase()
                || bytes[at - 1].is_ascii_digit()
                || bytes[at - 1].is_ascii_uppercase()
                    && bytes.get(at + 1).is_some_and(u8::is_ascii_lowercase))
        {
            result.push('_');
        }
        result.push(char::from(byte.to_ascii_lowercase()));
    }
    result
}
fn view_name(name: &str) -> Result<String, Error> {
    if name.is_empty()
        || name.len() > 200
        || name.split('.').any(|part| {
            part.is_empty()
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        })
    {
        Err(invalid(&crate::ui_text::t(
            "project.native.error.8613afc3ca",
            "Use a dotted Blade view name without path traversal",
        )))
    } else {
        Ok(name.into())
    }
}
fn default_destination(kind: &Kind, settings: &Settings) -> (String, String) {
    match kind.directory {
        "@factory" => ("database/factories".into(), "Database\\Factories".into()),
        "@seeder" => ("database/seeders".into(), "Database\\Seeders".into()),
        "@feature" => (
            format!("{}/Feature", settings.test_root),
            format!("{}\\Feature", settings.test_namespace),
        ),
        "@unit" => (
            format!("{}/Unit", settings.test_root),
            format!("{}\\Unit", settings.test_namespace),
        ),
        "@view" => (settings.view_root.clone(), String::new()),
        "@migration" => (settings.migration_root.clone(), String::new()),
        "" => (settings.source_root.clone(), settings.namespace.clone()),
        directory => (
            format!("{}/{directory}", settings.source_root),
            format!("{}\\{}", settings.namespace, directory.replace('/', "\\")),
        ),
    }
}
fn stamp(value: &str) -> bool {
    value.len() == 17
        && value.bytes().enumerate().all(|(at, byte)| {
            if matches!(at, 4 | 7 | 10) {
                byte == b'_'
            } else {
                byte.is_ascii_digit()
            }
        })
}
fn replacement(
    stub: &str,
    values: &BTreeMap<String, String>,
    blade: bool,
) -> Result<String, Error> {
    if stub.len() > 128 << 10 || stub.contains("<<<") {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.3b4fb2945e",
            "Stub is too large or uses unsupported heredoc syntax",
        )));
    }
    let mut rest = stub;
    let mut result = String::new();
    while let Some(at) = rest.find("{{") {
        result.push_str(&rest[..at]);
        let end = rest[at + 2..].find("}}").ok_or_else(|| {
            invalid(&crate::ui_text::t(
                "project.native.error.0d561e30e5",
                "Unclosed stub placeholder",
            ))
        })? + at
            + 2;
        let token = rest[at + 2..end].trim();
        if let Some(value) = values.get(token) {
            result.push_str(value);
        } else if blade {
            result.push_str(&rest[at..end + 2]);
        } else {
            return Err(invalid(&crate::ui_text::format(
                "project.native.format.488efd928c",
                "Unsupported installed/custom stub placeholder {token}",
                &[("token", &token)],
            )));
        }
        rest = &rest[end + 2..];
    }
    result.push_str(rest);
    for placeholder in [
        "DummyNamespace",
        "DummyClass",
        "DummyRootNamespace",
        "DummyModel",
    ] {
        if result.contains(placeholder) {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.3e12aa7e4e",
                "Legacy Dummy placeholders require an explicit modern template",
            )));
        }
    }
    Ok(result)
}
fn normalize_imports(source: &str, class: &str) -> Result<String, Error> {
    let mask = masked(source)?;
    let header = mask
        .find(&format!("class {class}"))
        .or_else(|| mask.find(&format!("interface {class}")))
        .or_else(|| mask.find(&format!("trait {class}")))
        .or_else(|| mask.find(&format!("enum {class}")))
        .unwrap_or(mask.len());
    let mut names = BTreeMap::<String, String>::new();
    let mut result = String::new();
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim();
        let mut duplicate = false;
        if offset < header && trimmed.starts_with("use ") && trimmed.ends_with(';') {
            let import = &trimmed[4..trimmed.len() - 1];
            let (fqn, alias) = import
                .split_once(" as ")
                .map_or((import, short(import)), |(fqn, alias)| (fqn, alias));
            if !generator_plan::qualified(fqn) || !generator_plan::identifier(alias) {
                return Err(invalid(&crate::ui_text::t(
                    "project.native.error.4eafc680ea",
                    "Grouped or unsupported custom imports need an explicit resolved template",
                )));
            }
            if alias.eq_ignore_ascii_case(class) {
                return Err(invalid(&crate::ui_text::t(
                    "project.native.error.3fba3c11e2",
                    "The generated class name conflicts with an imported class; change its name/postfix",
                )));
            }
            let key = alias.to_ascii_lowercase();
            if let Some(previous) = names.get(&key) {
                if previous.eq_ignore_ascii_case(fqn) {
                    duplicate = true;
                } else {
                    return Err(invalid(&crate::ui_text::t(
                        "project.native.error.062425625a",
                        "Imported class aliases conflict; select an explicit custom template",
                    )));
                }
            } else {
                names.insert(key, fqn.into());
            }
        }
        if !duplicate {
            result.push_str(line);
        }
        offset += line.len();
    }
    Ok(result)
}
fn masked(source: &str) -> Result<String, Error> {
    if source.contains("<<<") {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.8928b5a66d",
            "Heredoc source transformations are unsupported",
        )));
    }
    let text = php::mask_comments(source);
    let mut bytes = text.as_bytes().to_vec();
    let mut at = 0;
    while at < bytes.len() {
        if matches!(bytes[at], b'\'' | b'"' | b'`') {
            let end = php::string_end(&bytes, at).min(bytes.len());
            for byte in &mut bytes[at..end] {
                if *byte != b'\n' {
                    *byte = b' ';
                }
            }
            at = end;
        } else {
            at += 1;
        }
    }
    String::from_utf8(bytes).map_err(|_| Error::Stale)
}
#[derive(Clone)]
struct Method {
    name: String,
    start: usize,
    open: usize,
    close: usize,
    end: usize,
}
fn methods(source: &str) -> Result<Vec<Method>, Error> {
    let mask = masked(source)?;
    let bytes = mask.as_bytes();
    let mut at = 0;
    let mut methods = Vec::new();
    while at < bytes.len() {
        if bytes[at..].starts_with(b"function")
            && (at == 0 || !php::is_identifier(bytes[at - 1]))
            && bytes.get(at + 8).is_none_or(|b| !php::is_identifier(*b))
        {
            let start = at;
            at += 8;
            while bytes
                .get(at)
                .is_some_and(|b| b.is_ascii_whitespace() || *b == b'&')
            {
                at += 1;
            }
            let name_start = at;
            while bytes.get(at).is_some_and(|b| php::is_identifier(*b)) {
                at += 1;
            }
            let name = mask[name_start..at].to_owned();
            while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                at += 1;
            }
            if bytes.get(at) != Some(&b'(') {
                return Err(invalid(&crate::ui_text::t(
                    "project.native.error.c0fd140e08",
                    "Unsupported function declaration in template",
                )));
            }
            let open = at;
            let close = php::closing(&mask, open).ok_or(Error::Stale)?;
            let mut body = close + 1;
            while bytes.get(body).is_some_and(|b| *b != b'{' && *b != b';') {
                if bytes[body] == b'$' {
                    return Err(invalid(&crate::ui_text::t(
                        "project.native.error.04f3282e31",
                        "Unsupported closure use syntax in transformed template",
                    )));
                }
                body += 1;
            }
            let end = match bytes.get(body) {
                Some(b'{') => php::closing(&mask, body).ok_or(Error::Stale)? + 1,
                Some(b';') => body + 1,
                _ => return Err(Error::Stale),
            };
            methods.push(Method {
                name,
                start,
                open,
                close,
                end,
            });
            at = close + 1;
        } else {
            at += 1;
        }
    }
    Ok(methods)
}
fn without_types(source: &str, preserve_returns: &BTreeSet<String>) -> Result<String, Error> {
    let mask = masked(source)?;
    let mut edits = Vec::<(std::ops::Range<usize>, String)>::new();
    for method in methods(source)? {
        for range in php::split(&mask, method.open + 1..method.close) {
            let raw = &mask[range.clone()];
            let Some(dollar) = raw.find('$') else {
                if raw.trim().is_empty() {
                    continue;
                }
                return Err(invalid(&crate::ui_text::t(
                    "project.native.error.6349303082",
                    "Unsupported parameter syntax",
                )));
            };
            let prefix = &raw[..dollar];
            if prefix.contains(['#', '[', ']', '=', ':'])
                || prefix
                    .split_whitespace()
                    .any(|word| ["public", "protected", "private", "readonly"].contains(&word))
            {
                return Err(invalid(&crate::ui_text::t(
                    "project.native.error.e39e3bab79",
                    "Type-hint removal does not rewrite attributed/promoted custom parameters",
                )));
            }
            let trimmed = prefix.trim_end();
            let suffix = if trimmed.ends_with("&...") {
                4
            } else if trimmed.ends_with("...") {
                3
            } else if trimmed.ends_with('&') {
                1
            } else {
                0
            };
            let type_end = trimmed.len().saturating_sub(suffix);
            let start = prefix.len() - prefix.trim_start().len();
            if start < type_end {
                let ty = prefix[start..type_end].trim();
                if !ty.chars().all(|c| {
                    c.is_ascii_alphanumeric()
                        || matches!(c, '_' | '\\' | '?' | '|' | '&' | '(' | ')' | ' ')
                }) {
                    return Err(invalid(&crate::ui_text::t(
                        "project.native.error.41ad4d702e",
                        "Unsupported parameter type in custom template",
                    )));
                }
                edits.push((range.start + start..range.start + type_end, String::new()));
            }
        }
        if !preserve_returns.contains(&method.name) {
            let tail = &mask[method.close + 1..method.end];
            let end = tail.find(['{', ';']).ok_or(Error::Stale)?;
            if let Some(colon) = tail[..end].find(':') {
                let content = &tail[colon + 1..end];
                if !content.chars().all(|c| {
                    c.is_ascii_alphanumeric()
                        || c.is_whitespace()
                        || matches!(c, '_' | '\\' | '?' | '|' | '&' | '(' | ')')
                }) {
                    return Err(invalid(&crate::ui_text::t(
                        "project.native.error.b1799cd4f4",
                        "Unsupported return type in template",
                    )));
                }
                let trim = tail[..end].trim_end().len();
                edits.push((
                    method.close + 1 + colon..method.close + 1 + trim,
                    String::new(),
                ));
            }
        }
    }
    edits.sort_by_key(|(range, _)| range.start);
    let mut result = source.to_owned();
    for (range, replacement) in edits.into_iter().rev() {
        result.replace_range(range, &replacement);
    }
    Ok(result)
}
fn declaration_start(mask: &str, function: usize) -> usize {
    let bytes = mask.as_bytes();
    let mut start = function;
    let mut cursor = function;
    loop {
        while cursor > 0 && bytes[cursor - 1].is_ascii_whitespace() {
            cursor -= 1;
        }
        let end = cursor;
        while cursor > 0 && php::is_identifier(bytes[cursor - 1]) {
            cursor -= 1;
        }
        if [
            "public",
            "protected",
            "private",
            "static",
            "final",
            "abstract",
        ]
        .contains(&&mask[cursor..end])
        {
            start = cursor;
        } else {
            break;
        }
    }
    let line = mask[..start].rfind('\n').map_or(0, |at| at + 1);
    if mask[line..start].trim().is_empty() {
        line
    } else {
        start
    }
}
fn select_observer(source: &str, selected: &[String]) -> Result<String, Error> {
    let mask = masked(source)?;
    let methods = methods(source)?;
    let names: BTreeSet<_> = methods
        .iter()
        .filter(|method| method.name != "__construct")
        .map(|method| method.name.clone())
        .collect();
    if selected.iter().any(|name| !names.contains(name))
        || selected.iter().collect::<BTreeSet<_>>().len() != selected.len()
    {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.75e0d7e184",
            "Select distinct observer methods present in the installed stub",
        )));
    }
    let mut output = source.to_owned();
    for method in methods
        .into_iter()
        .rev()
        .filter(|method| method.name != "__construct" && !selected.contains(&method.name))
    {
        let mut start = declaration_start(&mask, method.start);
        let prefix = output[..start].trim_end();
        if prefix.ends_with(']') {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.5962df9e8e",
                "Observer selection does not remove custom attributed methods",
            )));
        }
        if prefix.ends_with("*/") {
            let doc = prefix.rfind("/**").ok_or(Error::Stale)?;
            let before = output[..doc].rfind('\n').map_or(0, |at| at + 1);
            if output[before..doc].trim().is_empty() {
                start = before;
            }
        }
        output.replace_range(start..method.end, "");
    }
    Ok(output)
}
fn custom_base(source: &str, class: &str, base: &str) -> Result<String, Error> {
    let base = qualified(base)?;
    let mask = masked(source)?;
    let marker = format!("class {class}");
    let at = mask.find(&marker).ok_or_else(|| {
        invalid(&crate::ui_text::t(
            "project.native.error.d5aa7a10ca",
            "Base-class settings require a class template",
        ))
    })?;
    let brace = mask[at..].find('{').ok_or(Error::Stale)? + at;
    let header = &mask[at..brace];
    let mut output = source.to_owned();
    if let Some(extends) = header.find(" extends ") {
        let begin = at + extends + 9;
        let end = mask[begin..brace]
            .find(|c: char| c.is_whitespace())
            .map_or(brace, |n| begin + n);
        output.replace_range(begin..end, &format!("\\{base}"));
    } else {
        let insert = header
            .find(" implements ")
            .map_or(at + marker.len(), |n| at + n);
        output.insert_str(insert, &format!(" extends \\{base}"));
    }
    Ok(output)
}
fn constructor(
    source: &str,
    dependencies: &[Dependency],
    settings: &Settings,
) -> Result<String, Error> {
    if dependencies.is_empty() {
        return Ok(source.into());
    }
    if dependencies.len() > 32 {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.94a1d07f62",
            "At most 32 constructor dependencies are supported",
        )));
    }
    let mut names = BTreeSet::new();
    for dep in dependencies {
        if !generator_plan::identifier(&dep.name)
            || !generator_plan::qualified(&dep.class)
            || !names.insert(&dep.name)
        {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.5bf92e4971",
                "Constructor dependencies need unique variable names and qualified class names",
            )));
        }
    }
    let mut props = String::new();
    let mut params = Vec::new();
    let mut body = String::new();
    for dep in dependencies {
        let ty = if settings.type_hints {
            format!("\\{} ", dep.class.trim_start_matches('\\'))
        } else {
            String::new()
        };
        if settings.constructor_promotion {
            params.push(format!("        public {ty}${}", dep.name));
        } else {
            props.push_str(&format!("    public {ty}${};\n", dep.name));
            params.push(format!("        {ty}${}", dep.name));
            body.push_str(&format!("        $this->{} = ${};\n", dep.name, dep.name));
        }
    }
    let new = format!(
        "{props}\n    public function __construct(\n{}\n    )\n    {{\n{body}    }}",
        params.join(",\n")
    );
    let mask = masked(source)?;
    let existing = methods(source)?
        .into_iter()
        .find(|method| method.name == "__construct");
    let mut output = source.to_owned();
    if let Some(method) = existing {
        let brace = mask[method.close + 1..method.end]
            .find('{')
            .ok_or(Error::Stale)?
            + method.close
            + 1;
        if !mask[brace + 1..method.end - 1].trim().is_empty()
            || !mask[method.open + 1..method.close].trim().is_empty()
        {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.4dc7c6beb3",
                "Configured dependencies cannot replace a nonempty constructor",
            )));
        }
        let start = declaration_start(&mask, method.start);
        output.replace_range(start..method.end, &new);
    } else {
        let at = mask.find("class ").ok_or_else(|| {
            invalid(&crate::ui_text::t(
                "project.native.error.250514ec76",
                "Constructor settings require a class",
            ))
        })?;
        let brace = mask[at..].find('{').ok_or(Error::Stale)? + at;
        output.insert_str(brace + 1, &format!("\n{new}\n"));
    }
    Ok(output)
}

pub(crate) fn plan(
    project: &Project,
    request: &Request,
    settings: &Settings,
) -> Result<(Plan, String), Error> {
    let kind = KINDS
        .iter()
        .find(|kind| kind.id == request.kind)
        .ok_or_else(|| {
            invalid(&crate::ui_text::t(
                "project.native.error.f02ee94e5d",
                "Unknown native generator kind",
            ))
        })?;
    if kind.id == "inertia-page" {
        return inertia_plan(project, request, settings);
    }
    if request.frontend.is_some() || request.page_root.is_some() || request.typescript.is_some() {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.d339114da6",
            "Inertia frontend options apply only to Inertia pages",
        )));
    }
    if kind.package.is_some() {
        return package_plan(project, request, settings, kind);
    }
    if request.component_type.is_some() || request.emoji.is_some() {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.e297e3dbe1",
            "Component format/emoji choices apply only to Livewire components",
        )));
    }
    if !generator_plan::qualified(&settings.namespace)
        || !generator_plan::qualified(&settings.application_namespace)
        || !generator_plan::qualified(&settings.test_namespace)
        || !generator_plan::qualified(&settings.test_case)
        || [
            &settings.source_root,
            &settings.migration_root,
            &settings.view_root,
            &settings.test_root,
            &settings.vendor_root,
        ]
        .iter()
        .any(|root| !generator_plan::relative(root))
    {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.276f87e37f",
            "Native generator roots/namespaces are invalid",
        )));
    }
    if settings.templates.len() > 512
        || settings
            .templates
            .values()
            .any(|body| body.len() > 128 << 10 || body.contains('\0'))
        || settings.templates.values().map(String::len).sum::<usize>() > 4 << 20
    {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.8cbe5b6502",
            "Captured native templates exceed supported limits",
        )));
    }
    if request.queued && !matches!(kind.id, "job" | "listener")
        || request.api && !matches!(kind.id, "controller" | "resource-controller")
        || request.implicit && kind.id != "rule"
        || request.pivot && kind.id != "model"
        || request.methods.is_some() && kind.id != "observer"
        || request.event.is_some() && kind.id != "listener"
        || request.table.is_some() && kind.id != "migration"
        || request.migration_operation.is_some()
            && (kind.id != "migration" || request.table.is_none())
        || request.enum_type.is_some() && kind.id != "enum"
        || request.command.is_some() && kind.id != "command"
        || request.view.is_some() && !matches!(kind.id, "view" | "blade-component" | "mail")
        || request.model.is_some()
            && !matches!(
                kind.id,
                "factory" | "observer" | "policy" | "resource-controller" | "facade"
            )
    {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.de779c62db",
            "An option does not apply to this generator",
        )));
    }
    if request
        .migration_operation
        .as_deref()
        .is_some_and(|mode| !matches!(mode, "create" | "update"))
        || request
            .enum_type
            .as_deref()
            .is_some_and(|ty| !matches!(ty, "string" | "int"))
    {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.9f9959b068",
            "Select a supported migration operation or enum backing type",
        )));
    }
    let overrides = settings.kinds.get(kind.id).cloned().unwrap_or_default();
    let (mut directory, mut namespace) = default_destination(kind, settings);
    if let Some(value) = &overrides.directory {
        directory = value.clone();
    }
    if let Some(value) = &overrides.namespace {
        namespace = value.clone();
    }
    namespace = namespace.trim_start_matches('\\').to_owned();
    if !generator_plan::relative(&directory)
        || !namespace.is_empty() && !generator_plan::qualified(&namespace)
    {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.e8348d8ede",
            "Configured generator destination is invalid",
        )));
    }
    let mut sources = Sources {
        project,
        root: project.root()?,
        files: BTreeMap::new(),
    };
    let (framework, major) = sources.framework(settings)?;
    let migration = kind.id == "migration";
    let blade = kind.id == "view";
    let postfix = overrides.postfix.as_deref().unwrap_or(kind.postfix);
    if !postfix.is_empty() && !generator_plan::identifier(postfix) {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.74aff68870",
            "Class postfix must be an identifier",
        )));
    }
    let relative_name = if blade {
        view_name(&request.name)?.replace('.', "/")
    } else {
        qualified(&request.name)?.replace('\\', "/")
    };
    let (subdir, leaf) = relative_name
        .rsplit_once('/')
        .map_or(("", relative_name.as_str()), |(left, right)| (left, right));
    if migration && !generator_plan::identifier(leaf) {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.63de22c060",
            "Migration names must be identifiers",
        )));
    }
    let class = if !postfix.is_empty() && !leaf.ends_with(postfix) {
        format!("{leaf}{postfix}")
    } else {
        leaf.into()
    };
    if !subdir.is_empty() {
        directory.push('/');
        directory.push_str(subdir);
        if !namespace.is_empty() {
            namespace.push('\\');
            namespace.push_str(&subdir.replace('/', "\\"));
        }
    }
    let output_path = if blade {
        format!("{directory}/{class}.blade.php")
    } else if migration {
        let stamp = request
            .migration_stamp
            .as_deref()
            .filter(|value| stamp(value))
            .ok_or_else(|| {
                invalid(&crate::ui_text::t(
                    "project.native.error.3eaa395de1",
                    "Supply one stable YYYY_MM_DD_HHMMSS migration stamp",
                ))
            })?;
        format!("{directory}/{stamp}_{class}.php")
    } else {
        format!("{directory}/{class}.php")
    };
    if !generator_plan::relative(&output_path) {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.28f377879b",
            "Generated path is invalid",
        )));
    }
    let model = request.model.as_deref().map(qualified).transpose()?;
    if kind.id == "factory" && model.is_none() {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.962355cde5",
            "Select an explicit model class for factory generation",
        )));
    }
    let user = qualified(
        request
            .user_model
            .as_deref()
            .unwrap_or(&format!("{}\\Models\\User", settings.namespace)),
    )?;
    let event = request.event.as_deref().map(qualified).transpose()?;
    let view = request.view.as_deref().map(view_name).transpose()?;
    let mut values = BTreeMap::from([
        ("namespace".into(), namespace.clone()),
        ("class".into(), class.clone()),
        (
            "rootNamespace".into(),
            format!(
                "{}\\",
                settings.application_namespace.trim_start_matches('\\')
            ),
        ),
        ("namespacedUserModel".into(), user.clone()),
        ("userModel".into(), short(&user).into()),
        ("user".into(), short(&user).into()),
        ("factoryNamespace".into(), namespace.clone()),
        (
            "factory".into(),
            class.strip_suffix("Factory").unwrap_or(&class).into(),
        ),
        (
            "factoryImport".into(),
            "use Illuminate\\Database\\Eloquent\\Factories\\HasFactory;".into(),
        ),
        ("subject".into(), snake(&class).replace('_', " ")),
        ("quote".into(), "Generated Laravel view.".into()),
        (
            "command".into(),
            request
                .command
                .clone()
                .unwrap_or_else(|| format!("app:{}", snake(leaf).replace('_', "-"))),
        ),
    ]);
    if values["command"].len() > 256
        || !values["command"].bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b':' | b'_' | b'-' | b' ' | b'{' | b'}' | b'=' | b'?' | b'*' | b'.'
                )
        })
    {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.02b5f2fea8",
            "Use a bounded literal Artisan command signature",
        )));
    }
    if let Some(model) = &model {
        let model_name: String = short(model).into();
        let mut variable = model_name.clone();
        variable[..1].make_ascii_lowercase();
        if kind.id == "policy" && variable == "user" {
            variable = "model".into();
        }
        let reserved = [
            "Controller",
            "Request",
            "Response",
            "Factory",
            "Model",
            "JsonResource",
            "ResourceCollection",
            "ServiceProvider",
            "Component",
            "View",
            "Closure",
            "ShouldQueue",
            "InteractsWithQueue",
            "Queueable",
            "SerializesModels",
            "Exception",
            "CastsAttributes",
            "Scope",
            "ValidationRule",
            "PotentiallyTranslatedString",
            "Envelope",
            "Content",
            "Attachment",
        ];
        let alias = if reserved
            .iter()
            .any(|reserved| reserved.eq_ignore_ascii_case(&model_name))
            || model_name.eq_ignore_ascii_case(&class)
            || model_name.eq_ignore_ascii_case(short(&user)) && !model.eq_ignore_ascii_case(&user)
        {
            format!("{model_name}Model")
        } else {
            model_name.clone()
        };
        values.extend([
            (
                "namespacedModel".into(),
                if alias == model_name {
                    model.clone()
                } else {
                    format!("{model} as {alias}")
                },
            ),
            ("model".into(), alias),
            ("modelVariable".into(), variable),
        ]);
    }
    if let Some(event) = &event {
        values.insert("eventNamespace".into(), event.clone());
        values.insert("event".into(), short(event).into());
    }
    let component_view = if kind.id == "blade-component" {
        Some(view.clone().unwrap_or_else(|| {
            format!(
                "components.{}",
                relative_name
                    .split('/')
                    .map(|part| snake(part).replace('_', "-"))
                    .collect::<Vec<_>>()
                    .join(".")
            )
        }))
    } else {
        None
    };
    if let Some(view) = component_view.as_ref().or(view.as_ref()) {
        values.insert(
            "view".into(),
            if kind.id == "blade-component" {
                format!("view({})", literal(view))
            } else {
                view.clone()
            },
        );
    }
    let stub = match kind.id {
        "controller" => {
            if request.api {
                "Routing/Console/stubs/controller.api.stub"
            } else {
                "Routing/Console/stubs/controller.plain.stub"
            }
        }
        "resource-controller" => {
            if model.is_some() {
                if request.api {
                    "Routing/Console/stubs/controller.model.api.stub"
                } else {
                    "Routing/Console/stubs/controller.model.stub"
                }
            } else if request.api {
                "Routing/Console/stubs/controller.api.stub"
            } else {
                "Routing/Console/stubs/controller.stub"
            }
        }
        "model" => {
            if request.pivot {
                "Foundation/Console/stubs/model.pivot.stub"
            } else {
                "Foundation/Console/stubs/model.stub"
            }
        }
        "channel" => "Foundation/Console/stubs/channel.stub",
        "command" => "Foundation/Console/stubs/console.stub",
        "event" => "Foundation/Console/stubs/event.stub",
        "exception" => "Foundation/Console/stubs/exception.stub",
        "factory" => "Database/Console/Factories/stubs/factory.stub",
        "feature-test" => "Foundation/Console/stubs/test.stub",
        "request" => "Foundation/Console/stubs/request.stub",
        "job" => {
            if request.queued {
                "Foundation/Console/stubs/job.queued.stub"
            } else {
                "Foundation/Console/stubs/job.stub"
            }
        }
        "resource" => "Foundation/Console/stubs/resource.stub",
        "listener" if major == 10 => match (event.is_some(), request.queued) {
            (true, true) => "Foundation/Console/stubs/listener-queued.stub",
            (true, false) => "Foundation/Console/stubs/listener.stub",
            (false, true) => "Foundation/Console/stubs/listener-queued-duck.stub",
            (false, false) => "Foundation/Console/stubs/listener-duck.stub",
        },
        "listener" => match (event.is_some(), request.queued) {
            (true, true) => "Foundation/Console/stubs/listener.typed.queued.stub",
            (true, false) => "Foundation/Console/stubs/listener.typed.stub",
            (false, true) => "Foundation/Console/stubs/listener.queued.stub",
            (false, false) => "Foundation/Console/stubs/listener.stub",
        },
        "mail" => {
            if view.is_some() {
                "Foundation/Console/stubs/view-mail.stub"
            } else {
                "Foundation/Console/stubs/mail.stub"
            }
        }
        "middleware" => "Routing/Console/stubs/middleware.stub",
        "migration" => {
            if request.table.is_some() {
                if request.migration_operation.as_deref() == Some("update") {
                    "Database/Migrations/stubs/migration.update.stub"
                } else {
                    "Database/Migrations/stubs/migration.create.stub"
                }
            } else {
                "Database/Migrations/stubs/migration.stub"
            }
        }
        "notification" => "Foundation/Console/stubs/notification.stub",
        "observer" => {
            if model.is_some() {
                "Foundation/Console/stubs/observer.stub"
            } else {
                "Foundation/Console/stubs/observer.plain.stub"
            }
        }
        "policy" => {
            if model.is_some() {
                "Foundation/Console/stubs/policy.stub"
            } else {
                "Foundation/Console/stubs/policy.plain.stub"
            }
        }
        "resource-collection" => "Foundation/Console/stubs/resource-collection.stub",
        "seeder" => "Database/Console/Seeds/stubs/seeder.stub",
        "provider" => "Foundation/Console/stubs/provider.stub",
        "unit-test" => "Foundation/Console/stubs/test.unit.stub",
        "rule" => {
            if request.implicit {
                "Foundation/Console/stubs/rule.implicit.stub"
            } else {
                "Foundation/Console/stubs/rule.stub"
            }
        }
        "view" => "Foundation/Console/stubs/view.stub",
        "blade-component" => "Foundation/Console/stubs/view-component.stub",
        "class" | "service" => "Foundation/Console/stubs/class.stub",
        "interface" => "Foundation/Console/stubs/interface.stub",
        "trait" => "Foundation/Console/stubs/trait.stub",
        "enum" => {
            if request.enum_type.is_some() {
                "Foundation/Console/stubs/enum.backed.stub"
            } else {
                "Foundation/Console/stubs/enum.stub"
            }
        }
        "cast" => "Foundation/Console/stubs/cast.stub",
        "scope" => "Foundation/Console/stubs/scope.stub",
        "http-exception" | "view-composer" | "facade" => "",
        _ => return Err(Error::Stale),
    };
    if let Some(table) = &request.table {
        if !generator_plan::identifier(table) {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.f784c96628",
                "Use a literal table identifier",
            )));
        }
        values.insert("table".into(), table.clone());
    }
    if let Some(ty) = &request.enum_type {
        values.insert("type".into(), ty.clone());
    }
    if kind.id == "resource-controller" && model.is_some() {
        values.insert(
            "namespacedRequests".into(),
            "Illuminate\\Http\\Request;".into(),
        );
        values.insert("storeRequest".into(), "Request".into());
        values.insert("updateRequest".into(), "Request".into());
    }
    let custom = overrides.template.as_ref();
    let mut template = if let Some(name) = custom {
        if let Some(body) = settings.templates.get(name) {
            if body.len() > 128 << 10 || body.contains('\0') {
                return Err(invalid(&crate::ui_text::t(
                    "project.native.error.893871a790",
                    "Captured template exceeds supported limits",
                )));
            }
            body.clone()
        } else {
            if !generator_plan::relative(name) {
                return Err(invalid(&crate::ui_text::t(
                    "project.native.error.33d38becb8",
                    "Template overrides must name a relative project template file",
                )));
            }
            sources.read(&format!(".openidea/laravel-templates/{name}"), 128 << 10)?
        }
    } else if !stub.is_empty() {
        let path = format!("{framework}/{stub}");
        match sources.optional(&path, 128 << 10)? {
            Some(template) => template,
            None if kind.id == "mail" && view.is_some() && major <= 11 => {
                let template = sources.read(
                    &format!("{framework}/Foundation/Console/stubs/mail.stub"),
                    128 << 10,
                )?;
                let marker = "view: 'view.name'";
                if template.matches(marker).count() != 1 {
                    return Err(invalid(&crate::ui_text::t(
                        "project.native.error.763aa84a34",
                        "Installed older Mailable stub has an unsupported Content shape",
                    )));
                }
                template.replace(marker, "view: '{{ view }}'")
            }
            None if major == 10 => match kind.id {
                // Laravel 10 predates these make commands. These are authored
                // PHP 8.1/Blade skeletons, not claimed to be installed stubs.
                "class" | "service" => {
                    "<?php\n\nnamespace {{ namespace }};\n\nclass {{ class }}\n{\n    //\n}\n"
                        .into()
                }
                "interface" => {
                    "<?php\n\nnamespace {{ namespace }};\n\ninterface {{ class }}\n{\n    //\n}\n"
                        .into()
                }
                "trait" => {
                    "<?php\n\nnamespace {{ namespace }};\n\ntrait {{ class }}\n{\n    //\n}\n"
                        .into()
                }
                "enum" => {
                    if request.enum_type.is_some() {
                        "<?php\n\nnamespace {{ namespace }};\n\nenum {{ class }}: {{ type }}\n{\n    //\n}\n".into()
                    } else {
                        "<?php\n\nnamespace {{ namespace }};\n\nenum {{ class }}\n{\n    //\n}\n"
                            .into()
                    }
                }
                "view" => "<div>\n    <!-- {{ quote }} -->\n</div>\n".into(),
                _ => return Err(Error::NotFound(path)),
            },
            None => return Err(Error::NotFound(path)),
        }
    } else {
        match kind.id {
            "http-exception" => {
                sources.read(
                    &format!(
                        "{}/symfony/http-kernel/Exception/HttpException.php",
                        settings.vendor_root
                    ),
                    128 << 10,
                )?;
                "<?php\n\nnamespace {{ namespace }};\n\nclass {{ class }} extends \\Symfony\\Component\\HttpKernel\\Exception\\HttpException\n{\n}\n".into()
            }
            "view-composer" => {
                sources.read(&format!("{framework}/View/View.php"), 128 << 10)?;
                "<?php\n\nnamespace {{ namespace }};\n\nclass {{ class }}\n{\n    public function compose(\\Illuminate\\View\\View $view): void\n    {\n        //\n    }\n}\n".into()
            }
            "facade" => {
                sources.read(
                    &format!("{framework}/Support/Facades/Facade.php"),
                    128 << 10,
                )?;
                "<?php\n\nnamespace {{ namespace }};\n\nclass {{ class }} extends \\Illuminate\\Support\\Facades\\Facade\n{\n    protected static function getFacadeAccessor()\n    {\n        return {{ accessor }};\n    }\n}\n".into()
            }
            _ => return Err(Error::Stale),
        }
    };
    if kind.id == "facade" {
        let accessor = request.model.as_deref().ok_or_else(|| {
            invalid(&crate::ui_text::t(
                "project.native.error.0737e8602f",
                "Choose an explicit service class for the facade accessor",
            ))
        })?;
        values.insert(
            "accessor".into(),
            format!("\\{}::class", qualified(accessor)?),
        );
    }
    if kind.id == "model" {
        values.insert(
            "factory".into(),
            format!(
                "/** @use HasFactory<\\Database\\Factories\\{class}Factory> */\n    use HasFactory;"
            ),
        );
    }
    if kind.id == "factory" {
        template = template
            .replace("{{ factory }}Factory", "{{ class }}")
            .replace("{{factory}}Factory", "{{class}}");
    }
    if kind.id == "feature-test" && settings.test_case != "Tests\\TestCase" {
        template = template.replace(
            "use Tests\\TestCase;",
            &format!("use {};", settings.test_case.trim_start_matches('\\')),
        );
    }
    let mut output = replacement(&template, &values, blade)?;
    if !blade {
        output = normalize_imports(&output, &class)?;
    }
    if let Some(methods) = &request.methods {
        output = select_observer(&output, methods)?;
    }
    if let Some(base) = &overrides.base_class {
        if !settings.type_hints {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.e64bd73476",
                "A custom base with disabled type hints needs a reviewed custom template preserving its method contracts",
            )));
        }
        output = custom_base(&output, &class, base)?;
    }
    if !settings.type_hints && !blade {
        let mut preserve = BTreeSet::new();
        if kind.id == "rule" {
            sources.read(
                &format!("{framework}/Contracts/Validation/ValidationRule.php"),
                128 << 10,
            )?;
            preserve.insert("validate".into());
        }
        output = without_types(&output, &preserve)?;
    }
    if !request.dependencies.is_empty() {
        if blade
            || migration
            || matches!(
                kind.id,
                "interface"
                    | "trait"
                    | "enum"
                    | "model"
                    | "factory"
                    | "provider"
                    | "exception"
                    | "http-exception"
            )
        {
            return Err(invalid(&crate::ui_text::t(
                "project.native.error.171ad3940e",
                "This generator does not accept new constructor dependencies",
            )));
        }
        output = constructor(&output, &request.dependencies, settings)?;
    }
    let mut files = vec![Output {
        path: output_path,
        text: output,
    }];
    if let Some(view) = component_view {
        let path = format!("{framework}/Foundation/Console/stubs/view.stub");
        let template = match sources.optional(&path, 128 << 10)? {
            Some(template) => template,
            None if major == 10 => "<div>\n    <!-- {{ quote }} -->\n</div>\n".into(),
            None => return Err(Error::NotFound(path)),
        };
        files.push(Output {
            path: format!(
                "{}/{}.blade.php",
                settings.view_root,
                view.replace('.', "/")
            ),
            text: replacement(&template, &values, true)?,
        });
    }
    if files
        .iter()
        .any(|file| file.text.len() > 128 << 10 || !generator_plan::relative(&file.path))
        || files
            .iter()
            .map(|file| file.path.to_ascii_lowercase())
            .collect::<BTreeSet<_>>()
            .len()
            != files.len()
    {
        return Err(invalid(&crate::ui_text::t(
            "project.native.error.5dcd7c5282",
            "Generated outputs exceed limits or conflict",
        )));
    }
    for file in &files {
        let path = format!("{}/{}", sources.root.trim_end_matches('/'), file.path);
        if host::project_of(&path)
            .and_then(|p| p.root().ok())
            .as_deref()
            != Some(&sources.root)
        {
            return Err(Error::Stale);
        }
    }
    let evidence =
        serde_json::to_string(&(request, settings, sources.files)).map_err(|_| Error::Stale)?;
    Ok((Plan { files }, evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const ROOT: &str = "vendor/laravel/framework/src/Illuminate";
    #[test]
    fn inertia_pages_preserve_configured_names_and_reject_ambiguity_and_overwrites() {
        let mock = fixture();
        mock.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"inertiajs/inertia-laravel","version":"v3.0.0"}]}"#,
        );
        mock.add_file(
            "vendor/inertiajs/inertia-laravel/composer.json",
            r#"{"name":"inertiajs/inertia-laravel"}"#,
        );
        mock.add_file("ide.json",r#"{"packages":{"inertia":{"ignoreSettingsPaths":true,"pagePaths":[{"path":"resources/pages","prefix":"Admin::","suffix":"Page"}]}}}"#);
        mock.add_file("package.json",r#"{"dependencies":{"@inertiajs/vue3":"^2","@inertiajs/react":"^2","@inertiajs/svelte":"^2"}}"#);
        for (frontend, package, extension) in [
            ("vue", "vue3", "vue"),
            ("react", "react", "tsx"),
            ("svelte", "svelte", "svelte"),
        ] {
            mock.add_file(
                &format!("node_modules/@inertiajs/{package}/package.json"),
                &format!(r#"{{"name":"@inertiajs/{package}","version":"2.0.0"}}"#),
            );
            let project = mock.invocation().project().unwrap();
            let req:Request=serde_json::from_value(json!({"kind":"inertia-page","name":"Admin::Reports/IndexPage","frontend":frontend})).unwrap();
            let (output, evidence) = plan(&project, &req, &Settings::default()).unwrap();
            assert_eq!(
                output.files[0].path,
                format!("resources/pages/Reports/Index.{extension}")
            );
            assert!(output.files[0].text.contains(if frontend == "svelte" {
                "svelte:head"
            } else {
                "Head"
            }));
            assert!(evidence.contains("ide.json"));
            let mut ambiguous = req.clone();
            ambiguous.frontend = None;
            if frontend != "vue" {
                assert!(plan(&project, &ambiguous, &Settings::default()).is_err());
            }
            let mut traversal = req.clone();
            traversal.name = "Admin::../SecretPage".into();
            assert!(plan(&project, &traversal, &Settings::default()).is_err());
        }
        let project = mock.invocation().project().unwrap();
        let req: Request = serde_json::from_value(
            json!({"kind":"inertia-page","name":"Admin::Reports/IndexPage","frontend":"vue"}),
        )
        .unwrap();
        let (_, before) = plan(&project, &req, &Settings::default()).unwrap();
        mock.add_file(
            "node_modules/@inertiajs/vue3/package.json",
            r#"{"name":"@inertiajs/vue3","version":"2.1.0"}"#,
        );
        assert_ne!(
            before,
            plan(&project, &req, &Settings::default()).unwrap().1
        );
        mock.add_file("resources/pages/Reports/Index.vue", "existing");
        assert!(plan(&project, &req, &Settings::default()).is_err());
    }

    #[test]
    #[ignore = "requires isolated installed Vue/React/Svelte adapter/compiler fixture"]
    fn installed_inertia_adapters_compile_native_pages() {
        let root = std::path::PathBuf::from(
            std::env::var("LARAVEL_INERTIA_ADAPTER_FIXTURE")
                .expect("LARAVEL_INERTIA_ADAPTER_FIXTURE"),
        );
        let mock = fixture();
        mock.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"inertiajs/inertia-laravel","version":"v3.7.0"}]}"#,
        );
        mock.add_file(
            "vendor/inertiajs/inertia-laravel/composer.json",
            r#"{"name":"inertiajs/inertia-laravel"}"#,
        );
        mock.add_file("ide.json",r#"{"packages":{"inertia":{"ignoreSettingsPaths":true,"pagePaths":[{"path":"resources/pages"}]}}}"#);
        mock.add_file(
            "package.json",
            &std::fs::read_to_string(root.join("package.json")).unwrap(),
        );
        for package in ["vue3", "react", "svelte"] {
            let path = format!("node_modules/@inertiajs/{package}/package.json");
            mock.add_file(&path, &std::fs::read_to_string(root.join(&path)).unwrap());
        }
        let project = mock.invocation().project().unwrap();
        let destination =
            std::env::temp_dir().join(format!("openidea-inertia-compile-{}", std::process::id()));
        std::fs::create_dir_all(&destination).unwrap();
        let runner = destination.join("compile.cjs");
        std::fs::write(&runner,r#"const fs=require('node:fs');const path=require('node:path');const root=process.argv[2],file=process.argv[3],frontend=process.argv[4];const code=fs.readFileSync(file,'utf8');
if(frontend==='vue'){const c=require(path.join(root,'node_modules/@vue/compiler-sfc'));const parsed=c.parse(code,{filename:file});if(parsed.errors.length)throw parsed.errors[0];c.compileScript(parsed.descriptor,{id:'native'});const result=c.compileTemplate({source:parsed.descriptor.template.content,filename:file,id:'native'});if(result.errors.length)throw result.errors[0];}
else if(frontend==='svelte'){require(path.join(root,'node_modules/svelte/compiler')).compile(code,{filename:file,generate:'server'});}
else{require(path.join(root,'node_modules/@babel/core')).transformSync(code,{filename:file,configFile:false,babelrc:false,plugins:[require(path.join(root,'node_modules/@babel/plugin-transform-react-jsx'))]});}
if(frontend!=='svelte'){const p=JSON.parse(fs.readFileSync(path.join(root,'node_modules/@inertiajs',frontend==='vue'?'vue3':'react','package.json'),'utf8'));const types=p.types||p.typings;if(!types)throw Error('adapter types missing');if(!fs.readFileSync(path.join(root,'node_modules/@inertiajs',frontend==='vue'?'vue3':'react',types),'utf8').includes('Head'))throw Error('adapter Head contract missing');}
"#).unwrap();
        for frontend in ["vue", "react", "svelte"] {
            for typed in [false, true] {
                let req:Request=serde_json::from_value(json!({"kind":"inertia-page","name":"Report","frontend":frontend,"typescript":typed})).unwrap();
                let (output, _) = plan(&project, &req, &Settings::default()).unwrap();
                let file = destination.join(output.files[0].path.rsplit('/').next().unwrap());
                std::fs::write(&file, &output.files[0].text).unwrap();
                let result = std::process::Command::new(
                    std::env::var("LARAVEL_NATIVE_NODE").unwrap_or_else(|_| "node".into()),
                )
                .args([
                    runner.as_os_str(),
                    root.as_os_str(),
                    file.as_os_str(),
                    std::ffi::OsStr::new(frontend),
                ])
                .output()
                .unwrap();
                assert!(
                    result.status.success(),
                    "{frontend} typed={typed}: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
            }
        }
        assert!(mock.process_runs().is_empty());
        assert!(mock.terminals().is_empty());
        std::fs::remove_dir_all(destination).unwrap();
    }

    fn php_binary() -> String {
        std::env::var("LARAVEL_NATIVE_PHP").unwrap_or_else(|_| "php".into())
    }
    fn fixture() -> MockHost {
        let mock = MockHost::install();
        mock.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"laravel/framework","version":"v13.3.3"}]}"#,
        );
        mock.add_file(
            &format!("{ROOT}/Foundation/Application.php"),
            "<?php class Application { const VERSION = '13.3.3'; }",
        );
        mock
    }
    fn request(kind: &str) -> Request {
        serde_json::from_value(json!({"kind":kind,"name":"Report"})).unwrap()
    }
    #[test]
    fn catalog_has_distinct_documented_native_workflows() {
        let value = catalog();
        let ids: BTreeSet<_> = value
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids.len(), KINDS.len());
        for name in [
            "http-exception",
            "view-composer",
            "resource-controller",
            "feature-test",
            "resource-collection",
            "blade-component",
        ] {
            assert!(ids.contains(name));
        }
    }
    #[test]
    fn installed_stub_settings_are_deterministic_and_inputs_are_inert() {
        let mock = fixture();
        mock.add_file(
            &format!("{ROOT}/Foundation/Console/stubs/class.stub"),
            "<?php\nnamespace {{ namespace }};\nclass {{ class }} {}\n",
        );
        let project = mock.invocation().project().unwrap();
        let mut settings = Settings::default();
        settings.kinds.insert(
            "service".into(),
            KindSettings {
                directory: Some("modules/Reports/Services".into()),
                namespace: Some("Modules\\Reports\\Services".into()),
                postfix: Some("Manager".into()),
                base_class: Some("App\\Services\\BaseService".into()),
                template: None,
            },
        );
        let req = request("service");
        let (first, evidence) = plan(&project, &req, &settings).unwrap();
        assert_eq!(
            first.files[0].path,
            "modules/Reports/Services/ReportManager.php"
        );
        assert!(
            first.files[0]
                .text
                .contains("extends \\App\\Services\\BaseService")
        );
        assert_eq!(plan(&project, &req, &settings).unwrap(), (first, evidence));
        assert!(mock.process_runs().is_empty());
        assert!(mock.terminals().is_empty());
    }
    #[test]
    fn framework_mismatch_unknown_placeholders_and_nested_sources_are_refused() {
        let mock = fixture();
        mock.add_file(
            &format!("{ROOT}/Foundation/Console/stubs/class.stub"),
            "<?php class {{ unknown }} {}",
        );
        let project = mock.invocation().project().unwrap();
        assert!(plan(&project, &request("class"), &Settings::default()).is_err());
        mock.add_file(
            &format!("{ROOT}/Foundation/Console/stubs/class.stub"),
            "<?php namespace {{ namespace }}; class {{ class }} {}",
        );
        mock.add_file(
            &format!("{ROOT}/Foundation/Application.php"),
            "<?php const VERSION = '12.0.0';",
        );
        assert!(plan(&project, &request("class"), &Settings::default()).is_err());
        mock.add_file(
            &format!("{ROOT}/Foundation/Application.php"),
            "<?php const VERSION = '13.3.3';",
        );
        mock.add_project("/project/vendor/laravel/framework");
        assert!(plan(&project, &request("class"), &Settings::default()).is_err());
    }
    #[test]
    fn observer_selection_and_type_settings_preserve_contracts_and_bodies() {
        let source = "<?php\nclass ReportObserver\n{\n    /** Created. */\n    public function created(Report $report): void { $report->log(message: 'keep'); }\n    /** Updated. */\n    public function updated(Report $report): void { $report->save(); }\n}\n";
        let selected = select_observer(source, &["created".into()]).unwrap();
        assert!(selected.contains("created"));
        assert!(!selected.contains("updated"));
        let untyped = without_types(&selected, &BTreeSet::new()).unwrap();
        assert!(untyped.contains("created( $report)"));
        assert!(untyped.contains("$report->log(message: 'keep')"));
        assert!(!untyped.contains(": void"));
        let rule = "<?php class Rule { public function validate(string $attribute, mixed $value, Closure $fail): void {} }";
        let transformed = without_types(rule, &BTreeSet::from(["validate".into()])).unwrap();
        assert!(transformed.contains("): void"));
        assert!(!transformed.contains("string $attribute"));
    }
    #[test]
    fn constructor_promotion_and_plain_properties_are_explicit() {
        let _locale_host = MockHost::install();
        let source = "<?php\nnamespace App;\nclass Report {\n    public function __construct() { /* empty */ }\n}\n";
        let dependencies = [Dependency {
            name: "repository".into(),
            class: "App\\Repositories\\Reports".into(),
        }];
        let mut settings = Settings::default();
        let promoted = constructor(source, &dependencies, &settings).unwrap();
        assert!(promoted.contains("public \\App\\Repositories\\Reports $repository"));
        assert!(!promoted.contains("$this->repository ="));
        settings.constructor_promotion = false;
        let plain = constructor(source, &dependencies, &settings).unwrap();
        assert!(plain.contains("public \\App\\Repositories\\Reports $repository;"));
        assert!(plain.contains("$this->repository = $repository;"));
        settings.type_hints = false;
        assert!(
            !constructor(source, &dependencies, &settings)
                .unwrap()
                .contains("\\App\\Repositories\\Reports")
        );
        assert!(
            constructor(
                "<?php class X {public function __construct() {run();}}",
                &dependencies,
                &settings
            )
            .is_err()
        );
    }
    #[test]
    fn component_returns_one_php_and_one_blade_plan() {
        let mock = fixture();
        mock.add_file(&format!("{ROOT}/Foundation/Console/stubs/view-component.stub"),"<?php namespace {{ namespace }}; class {{ class }} {public function render() {return {{ view }};}}");
        mock.add_file(
            &format!("{ROOT}/Foundation/Console/stubs/view.stub"),
            "<div><!-- {{ quote }} --></div>",
        );
        let mut req = request("blade-component");
        req.name = "Forms\\InputField".into();
        let (plan, _) = plan(
            &mock.invocation().project().unwrap(),
            &req,
            &Settings::default(),
        )
        .unwrap();
        assert_eq!(
            plan.files[0].path,
            "app/View/Components/Forms/InputField.php"
        );
        assert_eq!(
            plan.files[1].path,
            "resources/views/components/forms/input-field.blade.php"
        );
        assert!(
            plan.files[0]
                .text
                .contains("view('components.forms.input-field')")
        );
    }
    #[test]
    fn request_rejects_path_traversal_wrong_options_and_untyped_custom_bases() {
        let mock = fixture();
        let project = mock.invocation().project().unwrap();
        let mut req = request("service");
        req.name = "../escape".into();
        assert!(plan(&project, &req, &Settings::default()).is_err());
        let mut req = request("service");
        req.queued = true;
        assert!(plan(&project, &req, &Settings::default()).is_err());
        let mut settings = Settings::default();
        settings.kinds.insert(
            "service".into(),
            KindSettings {
                template: Some("../escape".into()),
                ..Default::default()
            },
        );
        assert!(plan(&project, &request("service"), &settings).is_err());
    }
    #[test]
    fn inline_methods_and_user_templates_preserve_class_boundaries() {
        let source = "<?php class Example { public function created(Model $model): void {} public function updated(Model $model): void {} }";
        let selected = select_observer(source, &["created".into()]).unwrap();
        assert!(selected.starts_with("<?php class Example {"));
        assert!(!selected.contains("updated"));
        let result = constructor(
            "<?php class Example {public function __construct() {}}",
            &[Dependency {
                name: "service".into(),
                class: "App\\Service".into(),
            }],
            &Settings::default(),
        )
        .unwrap();
        assert!(result.starts_with("<?php class Example {"));
        assert!(result.contains("public \\App\\Service $service"));
        let mock = fixture();
        let project = mock.invocation().project().unwrap();
        let mut settings = Settings::default();
        settings.kinds.insert(
            "service".into(),
            KindSettings {
                template: Some("@user/service.php".into()),
                ..Default::default()
            },
        );
        settings.templates.insert(
            "@user/service.php".into(),
            "<?php namespace {{namespace}}; class {{class}} {}".into(),
        );
        let (first, evidence) = plan(&project, &request("service"), &settings).unwrap();
        assert!(first.files[0].text.contains("class ReportService"));
        settings.templates.insert(
            "@user/service.php".into(),
            "<?php namespace {{namespace}}; final class {{class}} {}".into(),
        );
        assert_ne!(
            evidence,
            plan(&project, &request("service"), &settings).unwrap().1
        );
    }
    #[test]
    fn policy_user_model_and_import_aliases_are_not_duplicated() {
        let mock = fixture();
        mock.add_file(&format!("{ROOT}/Foundation/Console/stubs/policy.stub"),"<?php\nnamespace {{namespace}};\nuse {{namespacedModel}};\nuse {{namespacedUserModel}};\nclass {{class}} {public function view({{user}} $user, {{model}} ${{modelVariable}}): bool {return false;}}\n");
        let mut req = request("policy");
        req.model = Some("App\\Models\\User".into());
        let (plan, _) = plan(
            &mock.invocation().project().unwrap(),
            &req,
            &Settings::default(),
        )
        .unwrap();
        assert_eq!(
            plan.files[0].text.matches("use App\\Models\\User;").count(),
            1
        );
        assert!(plan.files[0].text.contains("User $user, User $model"));
        let output = normalize_imports(
            "<?php\nuse A\\Thing;\nuse B\\Thing;\nclass Example {}",
            "Example",
        );
        assert!(output.is_err());
    }
    fn read_fixture(mock: &MockHost, root: &std::path::Path) {
        for file in [
            "composer.lock",
            "vendor/laravel/framework/src/Illuminate/Foundation/Application.php",
            "vendor/laravel/framework/src/Illuminate/View/View.php",
            "vendor/laravel/framework/src/Illuminate/Support/Facades/Facade.php",
            "vendor/laravel/framework/src/Illuminate/Contracts/Validation/ValidationRule.php",
            "vendor/symfony/http-kernel/Exception/HttpException.php",
        ] {
            mock.add_file(file, &std::fs::read_to_string(root.join(file)).unwrap());
        }
        for dir in [
            "Foundation/Console/stubs",
            "Routing/Console/stubs",
            "Database/Console/Factories/stubs",
            "Database/Console/Seeds/stubs",
            "Database/Migrations/stubs",
        ] {
            let relative = format!("{ROOT}/{dir}");
            for entry in std::fs::read_dir(root.join(&relative)).unwrap() {
                let entry = entry.unwrap();
                if entry.path().extension().is_some_and(|ext| ext == "stub") {
                    mock.add_file(
                        &format!("{relative}/{}", entry.file_name().to_str().unwrap()),
                        &std::fs::read_to_string(entry.path()).unwrap(),
                    );
                }
            }
        }
    }
    fn read_package_fixture(mock: &MockHost, root: &std::path::Path) {
        read_fixture(mock, root);
        fn walk(mock: &MockHost, base: &std::path::Path, path: &std::path::Path) {
            for entry in std::fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    walk(mock, base, &entry.path());
                } else if entry.path().extension().is_some_and(|ext| ext == "stub") {
                    mock.add_file(
                        entry.path().strip_prefix(base).unwrap().to_str().unwrap(),
                        &std::fs::read_to_string(entry.path()).unwrap(),
                    );
                }
            }
        }
        for package in ["livewire/livewire", "laravel/dusk", "stancl/tenancy"] {
            let directory = root.join(format!("vendor/{package}"));
            if !directory.exists() {
                continue;
            }
            walk(mock, root, &directory);
            mock.add_file(
                &format!("vendor/{package}/composer.json"),
                &std::fs::read_to_string(directory.join("composer.json")).unwrap(),
            );
            for relative in [
                "config/livewire.php",
                "src/Component.php",
                "src/Form.php",
                "src/Page.php",
                "src/Finder/Finder.php",
                "assets/config.php",
            ] {
                let file = directory.join(relative);
                if file.is_file() {
                    mock.add_file(
                        &format!("vendor/{package}/{relative}"),
                        &std::fs::read_to_string(file).unwrap(),
                    );
                }
            }
        }
    }
    #[test]
    fn package_config_paths_and_template_tokens_are_literal_and_bounded() {
        let _locale_host = MockHost::install();
        let source = "<?php return ['class_namespace'=>'App\\\\Livewire','view_path'=>resource_path('views/widgets'),'make_command'=>['emoji'=>false]];";
        assert_eq!(
            php_string(
                source,
                config_entry(source, &["class_namespace"]).unwrap().unwrap()
            )
            .unwrap(),
            "App\\Livewire"
        );
        assert_eq!(
            config_path(
                source,
                config_entry(source, &["view_path"]).unwrap().unwrap(),
                "/project"
            )
            .unwrap(),
            "resources/views/widgets"
        );
        assert!(config_path("env('PATH')", 0..11, "/project").is_err());
        assert!(config_entry("<?php return buildConfig();", &["view_path"]).is_err());
        let values = BTreeMap::from([
            ("namespace".into(), "App\\Livewire".into()),
            ("class".into(), "Counter".into()),
            ("quote".into(), "Hello".into()),
        ]);
        let result = legacy_template(
            "namespace DummyNamespace; class DummyClass {} {{-- [quote] --}}",
            &values,
        )
        .unwrap();
        assert!(result.contains("namespace App\\Livewire; class Counter"));
        assert!(result.contains("Hello"));
        assert!(legacy_template("[unrecognized]", &values).is_err());
    }
    #[test]
    fn package_generators_require_installed_locked_sources() {
        let mock = fixture();
        let kind = KINDS
            .iter()
            .find(|kind| kind.id == "livewire-component")
            .unwrap();
        assert!(
            package_plan(
                &mock.invocation().project().unwrap(),
                &request(kind.id),
                &Settings::default(),
                kind
            )
            .is_err()
        );
    }
    #[test]
    fn livewire_module_generation_requires_actual_registered_namespace_or_location() {
        let mock = fixture();
        mock.add_file("composer.lock",r#"{"packages":[{"name":"laravel/framework","version":"v13.3.3"},{"name":"livewire/livewire","version":"v4.4.8"}]}"#);
        mock.add_file(
            "vendor/livewire/livewire/composer.json",
            r#"{"name":"livewire/livewire"}"#,
        );
        mock.add_file(
            "vendor/livewire/livewire/src/Component.php",
            "<?php namespace Livewire; abstract class Component {}",
        );
        mock.add_file("vendor/livewire/livewire/config/livewire.php",r#"<?php return ['component_locations'=>[resource_path('views/components')],'class_namespace'=>'App\\Livewire','view_path'=>resource_path('views/livewire'),'make_command'=>['type'=>'sfc','emoji'=>true]];"#);
        mock.add_file(
            "vendor/livewire/livewire/src/Finder/Finder.php",
            r#"<?php class Finder {private const ZAP = "\u{26A1}";}"#,
        );
        mock.add_file("vendor/livewire/livewire/src/Features/SupportConsoleCommands/Commands/livewire-sfc.stub","<?php use Livewire\\Component; new class extends Component {}; ?>\n<div>{{-- [quote] --}}</div>");
        let project = mock.invocation().project().unwrap();
        let mut settings = Settings {
            namespace: "Modules\\Blog".into(),
            source_root: "Modules/Blog/app".into(),
            view_root: "Modules/Blog/resources/views".into(),
            ..Default::default()
        };
        let req = request("livewire-component");
        assert!(
            plan(&project, &req, &settings).is_err(),
            "global Finder defaults cannot silently target a selected module"
        );
        settings.kinds.insert(
            "livewire-component".into(),
            KindSettings {
                directory: Some("Modules/Blog/resources/views/components".into()),
                ..Default::default()
            },
        );
        assert!(
            plan(&project, &req, &settings).is_err(),
            "an arbitrary native directory override does not register a Finder location"
        );
        mock.add_file("config/livewire.php",r#"<?php return ['component_locations'=>[base_path('Modules/Blog/resources/views/components')]];"#);
        let (generated, _) = plan(&project, &req, &settings).unwrap();
        assert_eq!(
            generated.files[0].path,
            "Modules/Blog/resources/views/components/⚡report.blade.php"
        );
        let mut class_req = req.clone();
        class_req.component_type = Some("class".into());
        assert!(
            plan(&project, &class_req, &settings).is_err(),
            "class components also need a configured matching namespace"
        );
        mock.add_file(
            "Modules/Blog/resources/views/components/⚡report.blade.php",
            "existing",
        );
        assert!(
            plan(&project, &req, &settings).is_err(),
            "cross-format/discovery names cannot be overwritten implicitly"
        );
    }

    #[test]
    #[ignore = "requires LARAVEL_NATIVE_PACKAGE_FIXTURE; lints package plans and checks framework classes/finder without browsers or application bootstrap"]
    fn installed_livewire_dusk_and_tenant_package_plans() {
        let fixture = std::env::var("LARAVEL_NATIVE_PACKAGE_FIXTURE").expect("set package fixture");
        let root_fixture = std::path::Path::new(&fixture);
        let mock = MockHost::install();
        read_package_fixture(&mock, root_fixture);
        let project = mock.invocation().project().unwrap();
        let lock: Value = serde_json::from_str(
            &std::fs::read_to_string(root_fixture.join("composer.lock")).unwrap(),
        )
        .unwrap();
        let installed: BTreeMap<_, _> = ["packages", "packages-dev"]
            .into_iter()
            .flat_map(|key| lock[key].as_array().into_iter().flatten())
            .filter_map(|package| {
                Some((
                    package["name"].as_str()?.to_owned(),
                    package["version"]
                        .as_str()?
                        .trim_start_matches('v')
                        .split('.')
                        .next()?
                        .parse::<u32>()
                        .ok()?,
                ))
            })
            .collect();
        let root = std::env::temp_dir().join(format!(
            "laravel-native-package-contracts-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let runner = root.join("contracts.php");
        std::fs::write(&runner,r#"<?php
namespace { require $argv[1].'/vendor/autoload.php'; }
namespace Tests { if (class_exists(\Laravel\Dusk\TestCase::class)) { abstract class DuskTestCase extends \Laravel\Dusk\TestCase { public function createApplication() { return new \Illuminate\Foundation\Application; } } } }
namespace {
 function check($value,$message) {if(!$value){throw new \RuntimeException($message);}}
 $kind=$argv[4];$before=get_declared_classes();ob_start();$generated=require $argv[2];ob_end_clean();$after=array_values(array_diff(get_declared_classes(),$before));
 if($kind==='livewire-component') {
   $matches=array_filter($after,fn($class)=>is_subclass_of($class,\Livewire\Component::class));check(count($matches)===1,'Livewire component inheritance');
   $component=new \ReflectionClass(array_values($matches)[0]);$format=$argv[6];
   if($component->hasMethod('render') && !$component->isAnonymous()) {
     if($format==='inline') {check(str_contains($component->newInstanceWithoutConstructor()->render(),'Generated Livewire component.'),'inline render body');}
     else { $app=new \Illuminate\Foundation\Application($argv[7]);$app->instance('config',new \Illuminate\Config\Repository(['view'=>['paths'=>[$argv[7].'/resources/views'],'compiled'=>$argv[7].'/compiled']]));$app->register(\Illuminate\Filesystem\FilesystemServiceProvider::class);$app->register(\Illuminate\View\ViewServiceProvider::class);$view=$component->newInstanceWithoutConstructor()->render();check($view instanceof \Illuminate\Contracts\View\View,'class render view contract');check($view->getName()==='livewire.native-package-example','class render view name'); }
   }
   if(class_exists(\Livewire\Finder\Finder::class) && ($format===''||$format==='single-file'||$format==='multi-file')) { $finder=new \Livewire\Finder\Finder;$finder->addLocation(viewPath:$argv[7].'/resources/views/components');$expected=$argv[7].'/'.$argv[5];if($format==='multi-file'){$expected=dirname($expected);$found=$finder->resolveMultiFileComponentPathForCreation('native-package-example',true);}else{$found=$finder->resolveSingleFileComponentPathForCreation('native-package-example',true);}check($found===$expected,'Livewire Finder generation path'); }
 }
 elseif($kind==='livewire-form') {check(is_subclass_of($argv[3],\Livewire\Form::class),'Livewire Form inheritance');}
 elseif($kind==='dusk-page') { $page=new $argv[3];check($page instanceof \Laravel\Dusk\Page,'Dusk Page inheritance');check($page->url()==='/','Dusk page URL stub');check($page->elements()['@element']==='#selector','Dusk page elements'); }
 elseif($kind==='dusk-component') { $component=new $argv[3];check($component instanceof \Laravel\Dusk\Component,'Dusk Component inheritance');check($component->selector()==='#selector','Dusk selector'); }
 elseif($kind==='dusk-test') {check(is_subclass_of($argv[3],\Laravel\Dusk\TestCase::class),'Dusk test inheritance');}
 elseif($kind==='tenant-migration') {check($generated instanceof \Illuminate\Database\Migrations\Migration,'tenant migration inheritance');}
}
"#).unwrap();
        for kind in KINDS.iter().filter(|kind| kind.package.is_some()) {
            let Some(major) = installed.get(kind.package.unwrap()) else {
                continue;
            };
            if kind.id == "livewire-form" && *major == 2 {
                assert!(
                    package_plan(&project, &request(kind.id), &Settings::default(), kind).is_err()
                );
                continue;
            }
            let formats = if kind.id == "livewire-component" {
                if *major == 4 {
                    vec![
                        None,
                        Some("class"),
                        Some("inline"),
                        Some("single-file"),
                        Some("multi-file"),
                    ]
                } else {
                    vec![None, Some("inline")]
                }
            } else {
                vec![None]
            };
            for (format_index, format) in formats.iter().enumerate() {
                for type_hints in [true, false] {
                    let mut req = request(kind.id);
                    req.name = "NativePackageExample".into();
                    req.component_type = format.map(String::from);
                    if kind.id == "tenant-migration" {
                        req.migration_stamp = Some("2026_11_10_123456".into());
                        req.table = Some("native_tenant_entries".into());
                    }
                    let settings = Settings {
                        type_hints,
                        ..Default::default()
                    };
                    let (plan, _) =
                        package_plan(&project, &req, &settings, kind).unwrap_or_else(|error| {
                            panic!("{} format={format:?} typed={type_hints}: {error}", kind.id)
                        });
                    for file in &plan.files {
                        let physical = root.join(&file.path);
                        std::fs::create_dir_all(physical.parent().unwrap()).unwrap();
                        std::fs::write(physical, &file.text).unwrap();
                    }
                    for (index, file) in plan.files.iter().enumerate() {
                        let path = root.join(format!(
                            "{}-{format_index}-{type_hints}-{index}.php",
                            kind.id
                        ));
                        std::fs::write(&path, &file.text).unwrap();
                        let lint = std::process::Command::new(php_binary())
                            .arg("-l")
                            .arg(&path)
                            .output()
                            .unwrap();
                        assert!(
                            lint.status.success(),
                            "{}: {}{}",
                            file.path,
                            String::from_utf8_lossy(&lint.stdout),
                            String::from_utf8_lossy(&lint.stderr)
                        );
                        if index > 0 {
                            continue;
                        }
                        let namespace = file
                            .text
                            .split_once("namespace ")
                            .map(|(_, rest)| rest.split(';').next().unwrap().trim())
                            .unwrap_or("");
                        let name = file
                            .path
                            .rsplit('/')
                            .next()
                            .unwrap()
                            .trim_end_matches(".php");
                        let fqcn = format!("{namespace}\\{name}");
                        let run = std::process::Command::new(php_binary())
                            .arg(&runner)
                            .arg(&fixture)
                            .arg(&path)
                            .arg(&fqcn)
                            .arg(kind.id)
                            .arg(&file.path)
                            .arg(format.unwrap_or(""))
                            .arg(&root)
                            .output()
                            .unwrap();
                        assert!(
                            run.status.success(),
                            "{} format={format:?} typed={type_hints}: {}{}",
                            kind.id,
                            String::from_utf8_lossy(&run.stdout),
                            String::from_utf8_lossy(&run.stderr)
                        );
                    }
                    if kind.id == "livewire-component"
                        && *major == 4
                        && matches!(format, None | Some("single-file") | Some("multi-file"))
                    {
                        assert!(
                            plan.files[0].path.contains('⚡'),
                            "Livewire4 default emoji prefix follows installed config"
                        );
                    }
                }
            }
        }
        assert!(mock.process_runs().is_empty());
        assert!(mock.terminals().is_empty());
    }
    #[test]
    #[ignore = "requires LARAVEL_NATIVE_GENERATOR_FIXTURE and local PHP; reads installed stubs, lints generated files, never executes app/PHP source"]
    fn official_installed_catalog_lints_every_kind_with_and_without_type_hints() {
        let fixture = std::env::var("LARAVEL_NATIVE_GENERATOR_FIXTURE")
            .expect("set LARAVEL_NATIVE_GENERATOR_FIXTURE");
        let mock = MockHost::install();
        read_fixture(&mock, std::path::Path::new(&fixture));
        let project = mock.invocation().project().unwrap();
        let root = std::env::temp_dir().join(format!(
            "laravel-native-catalog-lint-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        for kind in KINDS.iter().filter(|kind| kind.package.is_none()) {
            for type_hints in [true, false] {
                let mut req = request(kind.id);
                req.name = if kind.id == "view" {
                    "parity.catalog".into()
                } else if kind.id == "migration" {
                    "create_native_catalog_entries".into()
                } else {
                    "NativeCatalogExample".into()
                };
                if matches!(
                    kind.id,
                    "factory" | "observer" | "policy" | "resource-controller"
                ) {
                    req.model = Some("App\\Models\\User".into());
                }
                if kind.id == "facade" {
                    req.model = Some("App\\Services\\ExampleService".into());
                }
                if kind.id == "listener" {
                    req.event = Some("App\\Events\\ExampleEvent".into());
                }
                if kind.id == "migration" {
                    req.migration_stamp = Some("2026_11_10_123456".into());
                    req.table = Some("native_catalog_entries".into());
                }
                if kind.id == "observer" {
                    req.methods = Some(vec!["created".into()]);
                }
                let mut variants = vec![req.clone()];
                let mut alternate = req.clone();
                match kind.id {
                    "controller" | "resource-controller" => {
                        alternate.api = true;
                        variants.push(alternate);
                    }
                    "job" => {
                        alternate.queued = true;
                        variants.push(alternate);
                    }
                    "listener" => {
                        alternate.queued = true;
                        variants.push(alternate);
                        let mut plain = req.clone();
                        plain.event = None;
                        variants.push(plain);
                    }
                    "observer" | "policy" => {
                        alternate.model = None;
                        alternate.methods = None;
                        variants.push(alternate);
                    }
                    "mail" => {
                        alternate.view = Some("mail.example".into());
                        variants.push(alternate);
                    }
                    "migration" => {
                        alternate.migration_operation = Some("update".into());
                        variants.push(alternate);
                        let mut empty = req.clone();
                        empty.table = None;
                        variants.push(empty);
                    }
                    "rule" => {
                        alternate.implicit = true;
                        variants.push(alternate);
                    }
                    "model" => {
                        alternate.pivot = true;
                        variants.push(alternate);
                    }
                    "enum" => {
                        alternate.enum_type = Some("string".into());
                        variants.push(alternate);
                    }
                    "service" => {
                        alternate.dependencies = vec![Dependency {
                            name: "reports".into(),
                            class: "App\\Repositories\\Reports".into(),
                        }];
                        variants.push(alternate);
                    }
                    _ => {}
                }
                for (variant, req) in variants.iter().enumerate() {
                    let settings = Settings {
                        type_hints,
                        constructor_promotion: variant == 0,
                        ..Default::default()
                    };
                    let (plan, _) = plan(&project, req, &settings)
                        .unwrap_or_else(|error| panic!("{} typed={type_hints}: {error}", kind.id));
                    for (index, file) in plan.files.iter().enumerate() {
                        let output =
                            root.join(format!("{}-{type_hints}-{variant}-{index}.php", kind.id));
                        std::fs::write(&output, &file.text).unwrap();
                        let lint = std::process::Command::new(php_binary())
                            .arg("-l")
                            .arg(&output)
                            .output()
                            .unwrap();
                        assert!(
                            lint.status.success(),
                            "{} typed={type_hints}: {}{}",
                            kind.id,
                            String::from_utf8_lossy(&lint.stdout),
                            String::from_utf8_lossy(&lint.stderr)
                        );
                    }
                }
            }
        }
        assert!(mock.process_runs().is_empty());
        assert!(mock.terminals().is_empty());
    }

    #[test]
    #[ignore = "requires a package-only LARAVEL_NATIVE_GENERATOR_FIXTURE; loads installed framework/generated classes without application bootstrap or database"]
    fn installed_framework_contracts_and_generator_behavior() {
        let fixture = std::env::var("LARAVEL_NATIVE_GENERATOR_FIXTURE").expect("set fixture");
        let mock = MockHost::install();
        read_fixture(&mock, std::path::Path::new(&fixture));
        let project = mock.invocation().project().unwrap();
        let root = std::env::temp_dir().join(format!(
            "laravel-native-contracts-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let runner = root.join("contracts.php");
        std::fs::write(&runner,r#"<?php
namespace { require $argv[1].'/vendor/autoload.php'; }
namespace App\Models { class User extends \Illuminate\Foundation\Auth\User {} }
namespace App\Events { class ExampleEvent {} }
namespace App\Services { class ExampleService {} }
namespace App\Repositories { class Reports {} }
namespace App\Http\Controllers { class Controller extends \Illuminate\Routing\Controller {} }
namespace Tests { abstract class TestCase extends \Illuminate\Foundation\Testing\TestCase { public function createApplication() { return new \Illuminate\Foundation\Application; } } }
namespace {
    function check($value,$message) { if (!$value) { throw new \RuntimeException($message); } }
    $kind=$argv[4]; $class=$argv[3]; $generated=require $argv[2];
    if ($kind==='migration') { check($generated instanceof \Illuminate\Database\Migrations\Migration,'migration inheritance'); check(method_exists($generated,'up')&&method_exists($generated,'down'),'migration methods'); exit; }
    if ($kind==='interface') { check(interface_exists($class,false),'interface declaration'); exit; }
    if ($kind==='trait') { check(trait_exists($class,false),'trait declaration'); exit; }
    if ($kind==='enum') { check(enum_exists($class,false),'enum declaration'); exit; }
    check(class_exists($class,false),'class declaration'); $reflection=new \ReflectionClass($class);
    switch ($kind) {
      case 'request': $request=$reflection->newInstance(); check($request instanceof \Illuminate\Foundation\Http\FormRequest,'FormRequest inheritance'); check($request->authorize()===false,'new request authorization stays explicit'); check($request->rules()===[],'new request rules'); break;
      case 'policy': $method=$reflection->getMethod('create'); check($method->getNumberOfRequiredParameters()===1,'policy user parameter'); $lines=file($argv[2]); $body=implode('',array_slice($lines,$method->getStartLine()-1,$method->getEndLine()-$method->getStartLine()+1)); if (str_contains($body,'return false;')) { check($reflection->newInstance()->create(new \App\Models\User)===false,'completed policy default denies'); } break;
      case 'factory': check($reflection->isSubclassOf(\Illuminate\Database\Eloquent\Factories\Factory::class),'factory inheritance'); check($reflection->newInstanceWithoutConstructor()->definition()===[],'factory default definition'); break;
      case 'model': check($reflection->newInstance() instanceof \Illuminate\Database\Eloquent\Model,'model inheritance'); break;
      case 'middleware': $request=\Illuminate\Http\Request::create('/native-contract'); $response=$reflection->newInstance()->handle($request,fn($value)=>new \Illuminate\Http\Response('native-ok')); check($response->getContent()==='native-ok','middleware next closure'); break;
      case 'resource': check($reflection->newInstance(['id'=>17])->toArray(new \Illuminate\Http\Request)===['id'=>17],'resource wrapper behavior'); break;
      case 'resource-collection': check($reflection->newInstance([])->toArray(new \Illuminate\Http\Request)===[],'resource collection behavior'); break;
      case 'http-exception': check($reflection->newInstance(403,'native')->getStatusCode()===403,'HTTP exception status'); break;
      case 'exception': check($reflection->newInstance('native')->getMessage()==='native','exception message'); break;
      case 'rule': check($reflection->implementsInterface(\Illuminate\Contracts\Validation\ValidationRule::class),'validation rule contract'); break;
      case 'cast': check($reflection->implementsInterface(\Illuminate\Contracts\Database\Eloquent\CastsAttributes::class),'cast contract'); break;
      case 'scope': check($reflection->implementsInterface(\Illuminate\Database\Eloquent\Scope::class),'scope contract'); break;
      case 'facade': $accessor=$reflection->getMethod('getFacadeAccessor'); $accessor->setAccessible(true); check($accessor->invoke(null)===\App\Services\ExampleService::class,'facade service accessor'); break;
      case 'blade-component': check($reflection->isSubclassOf(\Illuminate\View\Component::class),'Blade component inheritance'); break;
    }
}
"#).unwrap();
        for kind in KINDS
            .iter()
            .filter(|kind| kind.id != "view" && kind.package.is_none())
        {
            for type_hints in [true, false] {
                let mut req = request(kind.id);
                req.name = "NativeContractExample".into();
                if matches!(
                    kind.id,
                    "factory" | "policy" | "observer" | "resource-controller"
                ) {
                    req.model = Some("App\\Models\\User".into());
                }
                if kind.id == "facade" {
                    req.model = Some("App\\Services\\ExampleService".into());
                }
                if kind.id == "listener" {
                    req.event = Some("App\\Events\\ExampleEvent".into());
                }
                if kind.id == "migration" {
                    req.migration_stamp = Some("2026_11_10_123456".into());
                    req.table = Some("native_contracts".into());
                }
                let settings = Settings {
                    type_hints,
                    ..Default::default()
                };
                let (plan, _) = plan(&project, &req, &settings)
                    .unwrap_or_else(|error| panic!("{} typed={type_hints}: {error}", kind.id));
                let file = &plan.files[0];
                let path = root.join(format!("{}-{type_hints}.php", kind.id));
                std::fs::write(&path, &file.text).unwrap();
                let mask = masked(&file.text).unwrap();
                let namespace = mask
                    .split_once("namespace ")
                    .map(|(_, rest)| rest.split(';').next().unwrap().trim())
                    .unwrap_or("");
                let name = file
                    .path
                    .rsplit('/')
                    .next()
                    .unwrap()
                    .trim_end_matches(".php");
                let fqcn = format!("{namespace}\\{name}");
                let output = std::process::Command::new(php_binary())
                    .arg(&runner)
                    .arg(&fixture)
                    .arg(&path)
                    .arg(&fqcn)
                    .arg(kind.id)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{} typed={type_hints}: {}{}",
                    kind.id,
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        assert!(mock.process_runs().is_empty());
        assert!(mock.terminals().is_empty());
    }
}
