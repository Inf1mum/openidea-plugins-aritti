//! Missing Livewire components reuse the installed package's native generator
//! plan and source evidence. The result is one reviewed no-overwrite FileAction.
use crate::{Index, generator_plan::Plan};
use openidea_plugin::{
    Error, Project, Range,
    file_actions::{FileAction, FileOperation},
    providers::Document,
    text,
};
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
struct Request {
    root: String,
    path: String,
    version: u64,
    text: String,
    at: usize,
    name: String,
}
fn selected(index: &Index, doc: &Document, at: usize) -> Option<String> {
    if index.partial
        || doc.language != "blade"
        || doc.text.len() > 128 * 1024
        || !index.metadata.livewire_namespaces.is_empty()
    {
        return None;
    }
    let mut references = crate::refs::references("blade", &doc.text);
    references.extend(crate::refs::tag_references(&doc.text));
    let reference = references.into_iter().find(|r| {
        r.kind == crate::refs::Kind::Livewire && r.closed && r.start <= at && at <= r.end
    })?;
    let name = doc.text.get(reference.start..reference.end)?;
    let valid = match name.split_once("::") {
        Some((namespace, local)) => {
            crate::refs::valid_tag_name(name, false)
                && !namespace.contains('.')
                && crate::view_creation::valid_name(local)
        }
        None => crate::view_creation::valid_name(name),
    };
    if index.livewire.contains_key(name) || !valid {
        return None;
    }
    Some(name.into())
}
fn generate(
    project: &Project,
    index: &Index,
    bundle: &crate::generator_presets::Bundle,
    module: Option<&str>,
    name: &str,
) -> Result<(Plan, String), Error> {
    let settings = bundle
        .native_settings(module)
        .map_err(Error::Invalid)
        .map_err(crate::generator_workspace::ui_error)?;
    let request =
        serde_json::from_value(serde_json::json!({"kind":"livewire-component","name":name}))
            .map_err(|_| Error::Stale)?;
    let (result, evidence) = crate::native_catalog::plan(project, &request, &settings)
        .map_err(crate::generator_workspace::ui_error)?;
    let destinations_match = if name.contains("::") {
        super::livewire_views::generated_namespaced(name, &result)
    } else {
        super::livewire_views::generated(index, name, &result)
    };
    if !destinations_match {
        return Err(Error::Invalid(crate::ui_text::text("final.packageComponent.invalidDestinations","Generated component destinations do not match the source-proven Livewire runtime namespace/roots").into()));
    }
    Ok((result, evidence))
}
fn plan(project: &Project, index: &Index, name: &str) -> Result<(Plan, String), Error> {
    let bundle = crate::generator_presets::load(project)?;
    let (result, evidence) = if name.contains("::") {
        let modules = bundle.module_ids();
        let mut found = None;
        for module in std::iter::once(None).chain(modules.iter().map(|id| Some(id.as_str()))) {
            let Ok(candidate) = generate(project, index, &bundle, module, name) else {
                continue;
            };
            if found.replace(candidate).is_some() {
                return Err(Error::Invalid(
                    crate::ui_text::text(
                        "final.packageComponent.ambiguousModule",
                        "More than one configured application or module can generate this namespaced Livewire component",
                    )
                    .into(),
                ));
            }
        }
        found.ok_or_else(|| {
            Error::Invalid(
                crate::ui_text::text(
                    "final.packageComponent.noModule",
                    "No configured application or module matches this registered Livewire namespace",
                )
                .into(),
            )
        })?
    } else {
        generate(project, index, &bundle, None, name)?
    };
    let root = project.root()?;
    if result.files.is_empty() || result.files.len() > 4 {
        return Err(Error::Stale);
    }
    for file in &result.files {
        let target = format!("{root}/{}", file.path);
        crate::view_creation::missing(project, &target)?;
        let parent = file.path.rsplit_once('/').ok_or(Error::Stale)?.0;
        let all = project.find_files(&format!("{parent}/**/*"), 513)?;
        if all.len() >= 513 {
            return Err(Error::Invalid(
                crate::ui_text::text(
                    "final.packageComponent.collisionBound",
                    "Component destination collision inventory exceeds its 512-path bound",
                )
                .into(),
            ));
        }
        if all
            .iter()
            .any(|p| p.eq_ignore_ascii_case(&target) || p.eq_ignore_ascii_case(&file.path))
        {
            return Err(Error::Invalid(
                crate::ui_text::text(
                    "final.packageComponent.caseCollision",
                    "A case-insensitive component destination already exists",
                )
                .into(),
            ));
        }
    }
    Ok((result, evidence))
}
pub(super) fn actions(index: &Index, doc: &Document, selection: Range) -> Vec<FileAction> {
    let run = || {
        let at = text::offset(&doc.text, selection.start)?;
        let name = selected(index, doc, at)?;
        let (project, root) = crate::view_creation::owner(doc, None).ok()?;
        plan(&project, index, &name).ok()?;
        let request = Request {
            root,
            path: doc.path.clone(),
            version: doc.version,
            text: doc.text.clone(),
            at,
            name: name.clone(),
        };
        Some(FileAction {
            title: crate::ui_text::format(
                "final.packageComponent.action",
                "Create Livewire component {name}",
                &[("name", &name)],
            ),
            kind: Some("quickfix".into()),
            data: Some(format!(
                "livewire-action:component:{}",
                serde_json::to_string(&request).ok()?
            )),
            ..Default::default()
        })
    };
    run().into_iter().collect()
}
pub(super) fn resolve(
    index: &Index,
    doc: &Document,
    action: FileAction,
) -> Result<FileAction, Error> {
    let raw = action
        .data
        .as_deref()
        .and_then(|d| d.strip_prefix("livewire-action:component:"))
        .filter(|s| s.len() < 256 * 1024)
        .ok_or(Error::Stale)?;
    let request: Request = serde_json::from_str(raw).map_err(|_| Error::Stale)?;
    let (project, root) = crate::view_creation::owner(doc, Some(&request.root))?;
    if request.path != doc.path
        || request.version != doc.version
        || request.text != doc.text
        || project.read_file(&doc.path)? != doc.text
    {
        return Err(Error::Stale);
    }
    if !crate::index::index_sources_current(&project,index){return Err(Error::Stale);}
    if selected(index, doc, request.at).as_deref() != Some(&request.name) {
        return Err(Error::Stale);
    }
    let (before, evidence) = plan(&project, index, &request.name)?;
    let mut preview = crate::ui_text::format(
        "final.packageComponent.review",
        "Project: {root}\nCreate Livewire component {name} using installed native package stubs\n\n",
        &[("root", &root), ("name", &request.name)],
    );
    for file in &before.files {
        preview.push_str(&crate::ui_text::format(
            "final.packageComponent.fileReview",
            "Create {path} (no overwrite)\nComplete source:\n{source}\n\n",
            &[
                ("path", &file.path),
                ("source", &ui_kit::prompts::readable_source(&file.text)),
            ],
        ));
    }
    if !ui_kit::prompts::confirm_exact(
        crate::ui_text::text(
            "final.packageComponent.confirm",
            "Create missing Livewire component?",
        ),
        &preview,
    )? {
        return Err(Error::Cancelled);
    }
    if !crate::index::index_sources_current(&project,index){return Err(Error::Stale);}
    let (after, after_evidence) = plan(&project, index, &request.name)?;
    if project.read_file(&doc.path)? != doc.text
        || selected(index, doc, request.at).as_deref() != Some(&request.name)
        || before != after
        || evidence != after_evidence
    {
        return Err(Error::Stale);
    }
    let operations = before
        .files
        .iter()
        .map(|f| FileOperation::Create {
            path: format!("{root}/{}", f.path),
            text: f.text.clone(),
            overwrite: false,
        })
        .collect();
    Ok(FileAction {
        title: crate::ui_text::format(
            "final.packageComponent.action",
            "Create Livewire component {name}",
            &[("name", &request.name)],
        ),
        operations,
        open: before.files.first().map(|f| format!("{root}/{}", f.path)),
        ..Default::default()
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{file_actions::FileActions, mock::MockHost};
    #[test]
    #[ignore = "requires disposable installed Composer package fixture"]
    fn installed_missing_component_uses_native_plan_atomic_review_and_rechecks_source() {
        let folder = std::env::var("OPENIDEA_LARAVEL_PACKAGE_FIXTURE").expect("fixture path");
        let base = std::path::Path::new(&folder);
        let host =
            MockHost::install().with_capabilities(&["project.read", "editor.read", "ui.prompt"]);
        for file in [
            "composer.json",
            "composer.lock",
            "vendor/composer/installed.json",
            "vendor/laravel/framework/src/Illuminate/Foundation/Application.php",
        ] {
            host.add_file(file, &std::fs::read_to_string(base.join(file)).unwrap());
        }
        fn copy(base: &std::path::Path, path: &std::path::Path, host: &MockHost) {
            for file in std::fs::read_dir(path).unwrap() {
                let path = file.unwrap().path();
                if path.is_dir() {
                    copy(base, &path, host);
                } else if let Ok(source) = std::fs::read_to_string(&path) {
                    host.add_file(path.strip_prefix(base).unwrap().to_str().unwrap(), &source);
                }
            }
        }
        copy(base, &base.join("vendor/livewire/livewire"), &host);
        host.add_file("artisan", "<?php");
        let source = "<livewire:missing-card/>";
        let doc = Document {
            path: "/project/resources/views/a.blade.php".into(),
            language: "blade".into(),
            version: 1,
            text: source.into(),
        };
        host.add_file("resources/views/a.blade.php", source);
        let project = host.invocation().project().unwrap();
        let index = Index::build(&project);
        assert!(
            selected(&index, &doc, source.find("missing").unwrap() + 2).is_some(),
            "selected missing component; partial={} refs={:?}",
            index.partial,
            crate::refs::references("blade", source)
        );
        plan(&project, &index, "missing-card").expect("installed native component plan");
        assert!(crate::index::index_sources_current(&project,&index),"fresh installed package sources must be reusable: {:?}",index.metadata.issues);

        let mut main = crate::Main::default();
        main.indexes.insert("/project".into(), index);
        let at = text::position(source, source.find("missing").unwrap() + 2);
        let selection = Range::new(at, at);
        let action = main
            .file_actions(&doc, selection)
            .into_iter()
            .find(|a| {
                a.data
                    .as_deref()
                    .is_some_and(|d| d.starts_with("livewire-action:component:"))
            })
            .expect("missing component quickfix");
        host.answer_next_prompt(Some(&["ok"]));
        let ready = main.resolve_file_action(&doc, action.clone()).unwrap();
        assert!(!ready.operations.is_empty());
        assert!(ready.operations.iter().all(|op| matches!(
            op,
            FileOperation::Create {
                overwrite: false,
                ..
            }
        )));
        assert!(ready.operations.iter().any(|op|matches!(op,FileOperation::Create{path,..}if path.ends_with("missing-card.blade.php"))));
        assert!(host.process_runs().is_empty());
        let changed = Document {
            version: 2,
            ..doc.clone()
        };
        assert_eq!(
            main.resolve_file_action(&changed, action.clone()),
            Err(Error::Stale)
        );
        for operation in &ready.operations {
            if let FileOperation::Create { path, text, .. } = operation {
                host.add_file(path, text);
            }
        }
        let refreshed = Index::build(&project);
        assert!(
            refreshed.livewire.contains_key("missing-card"),
            "new native SFC must resolve as Livewire"
        );
        main.indexes.insert("/project".into(), refreshed);
        assert!(!openidea_plugin::providers::Providers::definition(&mut main, &doc, at).is_empty());
        let (component_path, component_source) = ready
            .operations
            .iter()
            .find_map(|op| match op {
                FileOperation::Create { path, text, .. } if path.ends_with(".blade.php") => {
                    Some((path.clone(), text.clone()))
                }
                _ => None,
            })
            .unwrap();
        let component_source = component_source.replacen(
            "</div>",
            "<button wire:click=\"save\">Save</button></div>",
            1,
        );
        host.add_file(&component_path, &component_source);
        let refreshed = Index::build(&project);
        assert!(crate::index::index_sources_current(&project,&refreshed),"generated component source snapshot must remain current: {:?}",refreshed.metadata.issues);
        main.indexes.insert("/project".into(), refreshed);
        let component_doc = Document {
            path: component_path.clone(),
            language: "blade".into(),
            text: component_source.clone(),
            version: 1,
        };
        let action_at = text::position(
            &component_source,
            component_source.find("wire:click=\"save").unwrap() + 14,
        );
        let diagnostics = openidea_plugin::diagnostics::DiagnosticsProvider::diagnostics(
            &mut main,
            &component_doc,
        );
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.code.as_deref() == Some("laravel.unknown-livewire-method"))
                .count(),
            1
        );
        let method_action = main
            .file_actions(&component_doc, Range::new(action_at, action_at))
            .into_iter()
            .find(|a| {
                a.data.as_deref().is_some_and(|d| {
                    d.starts_with("livewire-action:")
                        && !d.starts_with("livewire-action:component:")
                })
            })
            .expect("missing anonymous-class action quickfix");
        host.answer_next_prompt(Some(&["ok"]));
        let method = main
            .resolve_file_action(&component_doc, method_action)
            .unwrap();
        assert_eq!(method.edits[0].0, component_path);
        assert!(
            method.edits[0].1[0]
                .text
                .contains("public function save(): void")
        );
        if let FileOperation::Create { path, .. } = &ready.operations[0] {
            host.add_file(path, "existing");
        }
        assert!(main.resolve_file_action(&doc, action).is_err());
        host.add_project("/project/vendor/livewire");
        assert!(!main.file_actions(&doc, selection).iter().any(|a| {
            a.data
                .as_deref()
                .is_some_and(|d| d.starts_with("livewire-action:component:"))
        }));
    }

    #[test]
    fn registered_module_namespace_component_is_offered_and_created_without_running_php() {
        let (host, _, _) = crate::native_catalog::namespaced_fixture();
        let host = host.with_capabilities(&["project.read", "ui.prompt"]);
        host.add_file("artisan", "<?php");
        host.add_file(".openidea/laravel-generators.json", r#"{"version":1,"modules":[{"id":"blog","label":"Blog","namespace":"Modules\\Blog","sourceRoot":"Modules/Blog/app","migrationRoot":"Modules/Blog/database/migrations","viewRoot":"Modules/Blog/resources/views"}]}"#);
        let source = "<livewire:blog::reports.daily/>";
        host.add_file("resources/views/a.blade.php", source);
        let doc = Document {
            path: "/project/resources/views/a.blade.php".into(),
            language: "blade".into(),
            version: 1,
            text: source.into(),
        };
        let project = host.invocation().project().unwrap();
        let index = Index::build(&project);
        let at = text::position(source, source.find("daily").unwrap());
        let actions = actions(&index, &doc, Range::new(at, at));
        assert_eq!(actions.len(), 1, "partial={} issues={:?}", index.partial, index.metadata.issues);
        let mut main = crate::Main::default();
        main.indexes.insert("/project".into(), index);
        host.answer_next_prompt(Some(&["ok"]));
        let ready = main.resolve_file_action(&doc, actions[0].clone()).unwrap();
        let created = ready
            .operations
            .iter()
            .filter_map(|operation| match operation {
                FileOperation::Create { path, overwrite: false, .. } => Some(path.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(created.len(), ready.operations.len());
        assert_eq!(
            created,
            [
                "/project/Modules/Blog/app/Livewire/Reports/Daily.php",
                "/project/Modules/Blog/resources/views/livewire/reports/daily.blade.php"
            ]
        );
        assert!(host.process_runs().is_empty());
    }

    #[test]
    fn namespaced_component_without_a_matching_configured_module_or_registration_is_refused() {
        for (module_config, name) in [(false, "blog::reports.daily"), (true, "other::reports.daily")] {
            let (host, _, _) = crate::native_catalog::namespaced_fixture();
            let host = host.with_capabilities(&["project.read", "ui.prompt"]);
            host.add_file("artisan", "<?php");
            if module_config {
                host.add_file(".openidea/laravel-generators.json", r#"{"version":1,"modules":[{"id":"blog","label":"Blog","namespace":"Modules\\Blog","sourceRoot":"Modules/Blog/app","migrationRoot":"Modules/Blog/database/migrations","viewRoot":"Modules/Blog/resources/views"}]}"#);
            }
            let project = host.invocation().project().unwrap();
            let index = Index::build(&project);
            assert!(plan(&project, &index, name).is_err(), "{name} module_config={module_config}");
        }
    }
}
