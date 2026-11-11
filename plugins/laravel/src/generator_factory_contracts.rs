//! Native convention proof for ordinary Laravel factories with no $model.
//! This never executes project PHP or trusts PHPDoc as runtime model binding.
use crate::{
    eloquent_members::{self, ClassIdentity},
    index::Index,
};
use openidea_plugin::{Error, FileKind, Project};
use std::collections::BTreeMap;
const FACTORIES: &[&str] = &[
    include_str!("../tests/native-factory-profiles/factory-10.php"),
    include_str!("../tests/native-factory-profiles/factory-11.php"),
    include_str!("../tests/native-factory-profiles/factory-13.php"),
];
const APPLICATIONS: &[&str] = &[
    include_str!("../tests/native-factory-profiles/application-10.php"),
    include_str!("../tests/native-factory-profiles/application-12.php"),
];
fn method(source: &str, name: &str, profiles: &[&str]) -> bool {
    let Some(actual) = eloquent_members::class_method_tokens(source, name) else {
        return false;
    };
    profiles.iter().any(|profile| {
        eloquent_members::class_method_tokens(profile, name).as_ref() == Some(&actual)
    })
}
fn binding(source: &str, short: &str, expected: &str) -> bool {
    eloquent_members::source_class_name(source, short).as_deref() == Some(expected)
}
pub(super) fn native_methods(factory: &str, application: &str) -> bool {
    if eloquent_members::declaration(factory).as_deref()
        != Some("Illuminate\\Database\\Eloquent\\Factories\\Factory")
        || eloquent_members::declaration(application).as_deref()
            != Some("Illuminate\\Foundation\\Application")
    {
        return false;
    }
    if !method(factory, "modelName", FACTORIES)
        || !method(factory, "appNamespace", FACTORIES)
        || !method(application, "getNamespace", APPLICATIONS)
        || !method(application, "path", APPLICATIONS)
    {
        return false;
    }
    if !binding(factory, "Str", "Illuminate\\Support\\Str")
        || !binding(factory, "Container", "Illuminate\\Container\\Container")
        || !binding(
            factory,
            "Application",
            "Illuminate\\Contracts\\Foundation\\Application",
        )
        || !binding(factory, "Throwable", "Throwable")
        || !binding(application, "RuntimeException", "RuntimeException")
    {
        return false;
    }
    let tokens = eloquent_members::class_method_tokens(factory, "modelName").unwrap();
    if tokens.iter().any(|token| token == "UseModel")
        && (!binding(
            factory,
            "UseModel",
            "Illuminate\\Database\\Eloquent\\Factories\\Attributes\\UseModel",
        ) || !binding(factory, "ReflectionClass", "ReflectionClass"))
    {
        return false;
    }
    eloquent_members::class_string_property(factory, "namespace").as_deref()
        == Some("Database\\Factories\\")
}
#[derive(Clone)]
pub(super) struct Convention {
    root: String,
    app: String,
    mappings: Vec<(String, Vec<String>)>,
    pub evidence: String,
}
fn relative(path: &str) -> Option<String> {
    super::super::composer_relative("", path).ok()
}
fn paths(value: &serde_json::Value) -> Option<Vec<String>> {
    match value {
        serde_json::Value::String(path) => Some(vec![relative(path)?]),
        serde_json::Value::Array(paths) => {
            paths.iter().map(|path| relative(path.as_str()?)).collect()
        }
        _ => None,
    }
}
fn unsafe_naming(source: &str) -> bool {
    let masked = super::sanitized_code(source);
    let words = masked
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .filter(|word| !word.is_empty())
        .map(|word| word.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if words.iter().any(|word| {
        [
            "guessmodelnamesusing",
            "guessfactorynamesusing",
            "usenamespace",
            "useapppath",
            "setinstance",
            "class_alias",
            "spl_autoload_register",
            "reflectionproperty",
        ]
        .contains(&word.as_str())
    }) {
        return true;
    }
    let compact = masked
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    [
        "::$namespace=",
        "::$modelNameResolver=",
        "::$modelNameResolvers=",
        "->namespace=",
    ]
    .iter()
    .any(|pattern| compact.contains(pattern))
}
impl Convention {
    pub(super) fn capture(
        project: &Project,
        index: &Index,
        classes: &[ClassIdentity],
        captured: &BTreeMap<String, String>,
    ) -> Option<Self> {
        let root = project.root().ok()?;
        let composer_source = super::read(project, &format!("{root}/composer.json"))?;
        let composer = crate::strict_json::parse(&composer_source, 128 << 10).ok()?;
        let vendor = relative(
            composer["config"]["vendor-dir"]
                .as_str()
                .unwrap_or("vendor"),
        )?;
        let factory_path = format!(
            "{root}/{vendor}/laravel/framework/src/Illuminate/Database/Eloquent/Factories/Factory.php"
        );
        let application_path =
            format!("{root}/{vendor}/laravel/framework/src/Illuminate/Foundation/Application.php");
        let factory = super::read(project, &factory_path)?;
        let application = super::read(project, &application_path)?;
        if !native_methods(&factory, &application) {
            return None;
        }
        let autoload = composer["autoload"]["psr-4"].as_object()?;
        // Application::getNamespace checks these ordered Composer paths against its
        // native app directory. Do not invent an App prefix for a custom namespace.
        let app = autoload.iter().find_map(|(namespace, value)| {
            paths(value)
                .filter(|paths| paths.iter().any(|path| path == "app"))
                .map(|_| namespace.clone())
        })?;
        if !app.ends_with('\\') || !crate::generator_plan::qualified(app.trim_end_matches('\\')) {
            return None;
        }
        if project.stat("app").ok()?.kind != FileKind::Directory {
            return None;
        }
        let mut mappings = Vec::new();
        for scope in ["autoload", "autoload-dev"] {
            if let Some(map) = composer[scope]["psr-4"].as_object() {
                for (namespace, value) in map {
                    if !namespace.is_empty()
                        && (!namespace.ends_with('\\')
                            || !crate::generator_plan::qualified(namespace.trim_end_matches('\\')))
                    {
                        return None;
                    }
                    mappings.push((namespace.clone(), paths(value)?));
                }
            }
        }
        if classes
            .iter()
            .any(|class| class.parent == "Illuminate\\Foundation\\Application")
        {
            return None;
        }
        let framework = format!("{root}/{vendor}/laravel/framework/");
        let mut policy = captured
            .iter()
            .filter(|(path, _)| !path.starts_with(&framework))
            .map(|(path, source)| (path.clone(), source.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut total = policy.values().map(String::len).sum::<usize>();
        let mut paths = index
            .navigation
            .keys()
            .filter(|path| path.ends_with(".php") && !path.starts_with(&framework))
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        paths.extend(
            index
                .source_evidence
                .keys()
                .filter(|path| {
                    path.ends_with(".php")
                        && !path.starts_with(&format!("{vendor}/laravel/framework/"))
                })
                .map(|path| {
                    if path.starts_with('/') {
                        path.clone()
                    } else {
                        format!("{root}/{path}")
                    }
                }),
        );
        if paths.len() > 1024 {
            return None;
        }
        for path in paths {
            if policy.contains_key(&path) {
                continue;
            }
            if !super::super::owned(&root, &path) {
                return None;
            }
            match project.read_file(&path) {
                Ok(source) => {
                    total += source.len();
                    if source.len() > 512 << 10 || total > 4 << 20 {
                        return None;
                    }
                    policy.insert(path, source);
                }
                Err(Error::NotFound(_)) => {}
                Err(_) => return None,
            }
        }
        if policy.values().any(|source| unsafe_naming(source)) {
            return None;
        }
        let signatures = policy
            .into_iter()
            .map(|(path, source)| (path, (source.len(), crate::artisan_metadata::hash(&source))))
            .collect::<BTreeMap<_, _>>();
        let evidence = serde_json::to_string(&(
            composer_source,
            factory_path,
            factory.len(),
            crate::artisan_metadata::hash(&factory),
            application_path,
            application.len(),
            crate::artisan_metadata::hash(&application),
            signatures,
        ))
        .ok()?;
        Some(Self {
            root,
            app,
            mappings,
            evidence,
        })
    }
    fn mapped(&self, project: &Project, class: &str, path: &str) -> bool {
        let Some(longest) = self
            .mappings
            .iter()
            .filter(|(prefix, _)| class.starts_with(prefix))
            .map(|(prefix, _)| prefix.len())
            .max()
        else {
            return false;
        };
        for (prefix, roots) in self
            .mappings
            .iter()
            .filter(|(prefix, _)| prefix.len() == longest && class.starts_with(prefix))
        {
            for root in roots {
                let relative = class[prefix.len()..].replace('\\', "/");
                let candidate = format!(
                    "{}/{}/{}.php",
                    self.root,
                    root.trim_end_matches('/'),
                    relative
                );
                match project.stat(&candidate) {
                    Ok(info) if info.kind == FileKind::File => return candidate == path,
                    Ok(_) => return false,
                    Err(Error::NotFound(_)) => {}
                    Err(_) => return false,
                }
            }
        }
        false
    }
    pub(super) fn target(
        &self,
        project: &Project,
        factory: &ClassIdentity,
        classes: &[ClassIdentity],
    ) -> Option<String> {
        if !self.mapped(project, &factory.full, &factory.path) {
            return None;
        }
        let tail = factory
            .full
            .strip_prefix("Database\\Factories\\")?
            .strip_suffix("Factory")?;
        if tail.is_empty() {
            return None;
        }
        let preferred = format!("{}Models\\{tail}", self.app);
        let fallback = format!("{}{}", self.app, tail.rsplit('\\').next()?);
        for class in [preferred, fallback] {
            let identities = classes
                .iter()
                .filter(|identity| identity.full == class)
                .collect::<Vec<_>>();
            if identities.len() != 1 {
                if identities.is_empty() {
                    continue;
                }
                return None;
            }
            let identity = identities[0];
            if identity.ambiguous
                || identity.is_trait
                || identity.is_abstract
                || !self.mapped(project, &class, &identity.path)
            {
                return None;
            }
            return Some(class);
        }
        None
    }
}
#[cfg(test)]
pub(super) fn fixture_sources() -> (String, String) {
    let factory = format!(
        "<?php namespace Illuminate\\Database\\Eloquent\\Factories;use Illuminate\\Support\\Str;use Illuminate\\Container\\Container;use Illuminate\\Contracts\\Foundation\\Application;use Throwable;{}",
        FACTORIES[1]
            .split_once("class FactoryContract")
            .unwrap()
            .1
            .replacen('{', "abstract class Factory {", 1)
    );
    let app = format!(
        "<?php namespace Illuminate\\Foundation;use RuntimeException;class Application{}",
        APPLICATIONS[1]
            .split_once("class ApplicationContract")
            .unwrap()
            .1
    );
    (factory, app)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_profile_rejects_changed_logic_imports_and_default_namespace() {
        let factory = format!(
            "<?php namespace Illuminate\\Database\\Eloquent\\Factories;use Illuminate\\Support\\Str;use Illuminate\\Container\\Container;use Illuminate\\Contracts\\Foundation\\Application;use Throwable;{}",
            FACTORIES[1]
                .split_once("class FactoryContract")
                .unwrap()
                .1
                .replacen('{', "abstract class Factory {", 1)
        );
        let app = format!(
            "<?php namespace Illuminate\\Foundation;use RuntimeException;class Application{}",
            APPLICATIONS[1]
                .split_once("class ApplicationContract")
                .unwrap()
                .1
        );
        assert!(native_methods(&factory, &app));
        assert!(!native_methods(
            &factory.replace("return $resolver($this);", "return 'Wrong';"),
            &app
        ));
        assert!(!native_methods(
            &factory.replace("use Illuminate\\Support\\Str;", "use Wrong\\Str;"),
            &app
        ));
        assert!(!native_methods(
            &factory.replace("Database\\\\Factories\\\\", "Other\\\\Factories\\\\"),
            &app
        ));
    }
    #[test]
    #[ignore = "requires explicitly installed Laravel10–13 native package fixtures"]
    fn installed_native_factory_convention_profiles_match_first_and_latest_versions() {
        for root in [
            "/tmp/laravel-native-compat-10-first",
            "/tmp/laravel-native-compat-10-latest",
            "/tmp/laravel-native-compat-11-first",
            "/tmp/laravel-native-compat-11-latest",
            "/tmp/laravel-native-compat-12-first",
            "/tmp/laravel-native-compat-12-latest",
            "/tmp/openidea-package-parity-fixture",
        ] {
            let factory=std::fs::read_to_string(format!("{root}/vendor/laravel/framework/src/Illuminate/Database/Eloquent/Factories/Factory.php")).unwrap();
            let app = std::fs::read_to_string(format!(
                "{root}/vendor/laravel/framework/src/Illuminate/Foundation/Application.php"
            ))
            .unwrap();
            assert!(native_methods(&factory, &app), "{root}");
        }
    }
}
