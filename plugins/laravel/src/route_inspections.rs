//! Current route syntax plus closed cached controller declarations; no PHP runs.
use crate::{Index, routes};
use openidea_plugin::{Diagnostic, Severity, host, providers::Document};
use std::collections::{BTreeMap, BTreeSet};
fn known(index: &Index, root: &str, path: &str) -> bool {
    let Some(relative) = path.strip_prefix(root).and_then(|s| s.strip_prefix('/')) else {
        return false;
    };
    relative.starts_with("routes/")
        || index.route_files.contains(path)
        || index.route_defs.iter().any(|entry| entry.path == path)
        || index
            .metadata
            .route_files
            .iter()
            .any(|pattern| pattern.trim_start_matches(&format!("{root}/")) == relative)
}
pub(crate) fn current(index: &Index, doc: &Document) -> Vec<Diagnostic> {
    let run = || -> Option<Vec<Diagnostic>> {
        if doc.language != "php"
            || index.partial
            || doc.text.len() > 128 * 1024
            || !host::setting_bool("reportUnknownRouteControllers").unwrap_or(true)
        {
            return None;
        }
        let project = crate::project_for(&doc.path)?;
        let root = project.root().ok()?;
        if !crate::owns_current_path(&project, &doc.path) || !known(index, &root, &doc.path) {
            return None;
        }
        let definitions = index.parse_routes(&doc.path,&doc.text);
        let resources = routes::resource_offsets(&doc.text);
        let identities = crate::eloquent_members::class_inventory(&index.eloquent_models);
        let namespace = host::setting_string("controllersNamespace")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "App\\Http\\Controllers".into());
        let mut missing: BTreeMap<usize, (std::ops::Range<usize>, String, BTreeSet<String>, bool)> =
            BTreeMap::new();
        let mut found = Vec::new();
        let code = crate::package_facts::code(&doc.text)?;
        let namespace_group = code.contains("::namespace") || code.contains("->namespace");
        for def in definitions {
            let Some((class, method)) = &def.controller else {
                continue;
            };
            let spec = def.action.split_once('@').map(|p| p.0).unwrap_or(class);
            // External string namespace scopes need a separately proven mount
            // context. PHP class-constant aliases remain source-resolved below.
            if namespace_group && resources.contains(&def.offset) {
                continue;
            }
            let full = if spec.contains('\\') {
                if namespace_group && !spec.starts_with('\\') {
                    continue;
                }
                crate::actions::controller_literal(spec, '\'')?
                    .trim_start_matches('\\')
                    .to_owned()
            } else {
                let Some(full) = crate::eloquent_members::source_class_name(&doc.text, spec) else {
                    continue;
                };
                full
            };
            if index.metadata.helper_ignored(&full) {
                continue;
            }
            let entries = identities
                .iter()
                .filter(|identity| identity.full.eq_ignore_ascii_case(&full))
                .collect::<Vec<_>>();
            if entries.is_empty() {
                if !full.starts_with(&format!("{}\\", namespace.trim_end_matches('\\')))
                    || index.bindings.contains_key(&full)
                    || index.bindings.contains_key(spec)
                {
                    continue;
                }
                let suffix =
                    full.strip_prefix(&format!("{}\\", namespace.trim_end_matches('\\')))?;
                let expected = format!(
                    "{root}/app/Http/Controllers/{}.php",
                    suffix.replace('\\', "/")
                );
                if !crate::owns_current_path(&project, &expected) {
                    continue;
                }
                found.push(crate::problem(
                    &doc.text,
                    def.offset..def.offset + def.width,
                    "laravel.unknown-route-controller",
                    Severity::Warning,
                    crate::ui_text::format(
                        "routeInspection.unknownController",
                        "Controller \"{class}\" does not exist.",
                        &[("class", class)],
                    ),
                ));
                continue;
            }
            if entries.len() != 1
                || entries[0].ambiguous
                || !crate::owns_current_path(&project, &entries[0].path)
            {
                continue;
            }
            if crate::eloquent_members::semantic::method_presence(
                index,
                doc,
                &full,
                method,
                &|path| crate::owns_current_path(&project, path),
            ) == Some(false)
            {
                let is_resource = resources.contains(&def.offset);
                if is_resource
                    && !host::setting_bool("reportMissingResourceMethods").unwrap_or(true)
                {
                    continue;
                }
                let item = missing.entry(def.offset).or_insert_with(|| {
                    (
                        def.offset..def.offset + def.width,
                        class.clone(),
                        BTreeSet::new(),
                        is_resource,
                    )
                });
                item.2.insert(method.clone());
            }
        }
        for (_, (range, class, methods, resource)) in missing {
            let names = methods.into_iter().collect::<Vec<_>>();
            let code = if resource {
                "laravel.missing-resource-methods"
            } else {
                "laravel.unknown-route-method"
            };
            let message = if resource {
                crate::ui_text::format(
                    "routeInspection.missingResource",
                    "{class} lacks the resource methods {methods}.",
                    &[("class", &class), ("methods", &names.join(", "))],
                )
            } else {
                crate::ui_text::format(
                    "routeInspection.missingMethod",
                    "{class} has no method \"{method}\".",
                    &[("class", &class), ("method", &names.join(", "))],
                )
            };
            found.push(crate::problem(
                &doc.text,
                range,
                code,
                Severity::Warning,
                message,
            ));
        }
        Some(found)
    };
    run().unwrap_or_default()
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, text};
    fn setup() -> (MockHost, Index) {
        let host = MockHost::install();
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        host.add_file("artisan", "<?php");
        host.add_file("ide.json",r#"{"routing":{"rootFiles":["custom/api/routes.php"],"routeFacadeMethods":[{"name":"tenant","urlIndex":1,"controllerIndex":2,"httpMethod":"GET"}]}}"#);
        host.add_file("app/Http/Controllers/PostController.php","<?php\nnamespace App\\Http\\Controllers;\nclass PostController {\npublic function index() {}\npublic function SHOW() {}\n}\n");
        let index = Index::build(&host.invocation().project().unwrap());
        (host, index)
    }
    fn doc(source: &str) -> Document {
        Document {
            path: "/project/custom/api/routes.php".into(),
            language: "php".into(),
            version: 1,
            text: source.into(),
        }
    }
    #[test]
    fn metadata_root_current_overlay_resource_single_missing_uses_resource_code_and_toggle() {
        let (host, index) = setup();
        let source = "<?php /* 😀 */ use App\\Http\\Controllers\\PostController; Route::resource('posts',PostController::class)->only(['index','create']);";
        let found = current(&index, &doc(source));
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].code.as_deref(),
            Some("laravel.missing-resource-methods")
        );
        assert_eq!(
            found[0].range.start,
            text::position(source, source.find("posts'").unwrap())
        );
        host.set_setting("reportMissingResourceMethods", "false");
        assert!(current(&index, &doc(source)).is_empty());
    }
    #[test]
    fn custom_route_methods_and_method_case_use_source_proof() {
        let (_host, index) = setup();
        let source = "<?php use App\\Http\\Controllers\\PostController; Route::tenant('/x',[PostController::class,'missing']); Route::get('/show',[PostController::class,'show']);";
        let found = current(&index, &doc(source));
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].code.as_deref(),
            Some("laravel.unknown-route-method")
        );
    }
    #[test]
    fn ambiguous_dynamic_foreign_and_unregistered_sources_do_not_report() {
        let (host, index) = setup();
        let source = "<?php use App\\Http\\Controllers\\PostController; Route::resource('posts',PostController::class);";
        let mut d = doc(source);
        d.path = "/project/unregistered.php".into();
        assert!(current(&index, &d).is_empty());
        host.add_project("/project/app/Http/Controllers");
        assert!(current(&index, &doc(source)).is_empty());
    }
}
