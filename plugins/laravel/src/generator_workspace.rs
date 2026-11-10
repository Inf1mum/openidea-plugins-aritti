//! Project-owned previews bridged to the host's atomic editor FileAction API.
use crate::generator_plan::{self, Draft, Plan};
use openidea_plugin::{
    Error, Invocation, Project,
    file_actions::{FileAction, FileOperation},
    host,
    providers::Document,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) const VIEW: &str = "laravel-generators";
pub(crate) fn html() -> String {
    let revision = crate::artisan_metadata::hash(include_str!(
        "../../../crates/ui-kit/web/command-workspace.css"
    ));
    let html = include_str!("../web/generators.html").replace(
        "./command-workspace.css",
        &format!("./command-workspace.css?v={revision}"),
    );
    format!(
        "<link rel=\"stylesheet\" href=\"./components.css\"><style>{}</style>{}",
        include_str!("../web/generators.css"),
        html
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn setup() -> (MockHost, State, Document) {
        let h = MockHost::install();
        h.add_dir("app");
        h.add_file(
            "composer.json",
            r#"{"autoload":{"psr-4":{"App\\":"app/","Database\\Factories\\":"database/factories/","Database\\Seeders\\":"database/seeders/"}}}"#,
        );
        h.add_file("routes/web.php", "<?php // anchor");
        let doc = Document {
            path: "/project/routes/web.php".into(),
            text: "<?php // anchor".into(),
            language: "php".into(),
            version: 1,
        };
        h.open_editor(&doc.path, "php", &doc.text, &[]);
        (h, State::default(), doc)
    }
    #[test]
    fn composer_parent_prefix_and_owned_package_manifest_resolve_generated_classes() {
        let (h, _, _) = setup();
        let mut draft: Draft = serde_json::from_value(
            json!({"name":"Post","table":"posts","migrationStamp":"2026_11_10_120000"}),
        )
        .unwrap();
        draft.name = "Post".into();
        draft.namespace = "App\\Domain".into();
        draft.source_root = "app/Domain".into();
        let project = h.invocation().project().unwrap();
        assert!(mapping(&project, &draft).is_ok());
        h.add_dir("packages");
        h.add_dir("packages/billing");
        h.add_dir("packages/billing/src");
        h.add_file(
            "packages/billing/composer.json",
            r#"{"autoload":{"psr-4":{"Vendor\\Billing\\":"src/"}}}"#,
        );
        draft.namespace = "Vendor\\Billing".into();
        draft.source_root = "packages/billing/src".into();
        assert!(mapping(&project, &draft).is_ok());
        let before = mapping(&project, &draft).unwrap();
        h.add_file(
            "packages/billing/src/composer.json",
            r#"{"name":"owned/nested"}"#,
        );
        assert_ne!(
            mapping(&project, &draft).unwrap(),
            before,
            "new manifests invalidate prepared evidence"
        );
        h.add_project("/project/packages/billing");
        assert!(mapping(&project, &draft).is_err());
    }
    #[test]
    fn composer_conflicts_and_escaping_relative_roots_are_rejected() {
        assert_eq!(
            composer_relative("packages/billing", "../shared/src/").unwrap(),
            "packages/shared/src"
        );
        assert!(composer_relative("packages/billing", "../../../escape").is_err());
        let (h, _, _) = setup();
        h.add_dir("app/Domain");
        h.add_file(
            "app/composer.json",
            r#"{"autoload":{"psr-4":{"App\\":"different/"}}}"#,
        );
        let mut draft: Draft = serde_json::from_value(
            json!({"name":"Post","table":"posts","migrationStamp":"2026_11_10_120000"}),
        )
        .unwrap();
        draft.name = "Post".into();
        assert!(mapping(&h.invocation().project().unwrap(), &draft).is_err());
    }
    #[test]
    fn preview_uses_configured_destinations_composer_proof_and_availability() {
        let (h, mut state, _) = setup();
        h.add_file("composer.json",r#"{"autoload":{"psr-4":{"App\\":"app/","Domain\\":"src/","Database\\Factories\\":"database/factories/"}}}"#);
        h.add_file(".openidea/laravel-generators.json",r#"{"version":1,"generators":{"model":{"directory":"src/Entities","namespace":"Domain\\Entities"},"controller":{"availableInApplication":false}}}"#);
        let preview = state.handle(&h.invocation(), &request("preview")).unwrap();
        assert!(preview["files"].as_array().unwrap().iter().any(|file| {
            file["path"] == "src/Entities/Post.php"
                && file["text"]
                    .as_str()
                    .unwrap()
                    .contains("namespace Domain\\Entities;")
        }));
        let mut unavailable: Value = serde_json::from_str(&request("preview")).unwrap();
        unavailable["draft"]["related"] = json!(["controller"]);
        assert!(
            state
                .handle(&h.invocation(), &unavailable.to_string())
                .is_err()
        );
        h.add_file(
            "composer.json",
            r#"{"autoload":{"psr-4":{"Domain\\":"wrong/"}}}"#,
        );
        assert!(state.handle(&h.invocation(), &request("preview")).is_err());
    }
    fn request(op: &str) -> String {
        json!({"op":op,"draft":{"name":"Post","table":"posts","migrationStamp":"2026_11_10_120000","fields":[{"name":"title","kind":"string"}],"related":["migration","factory"]}}).to_string()
    }
    fn model_preset(h: &MockHost, state: &mut State) {
        h.add_file("app/Models/UserProfile.php","<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class UserProfile extends Model {}");
        h.add_file(".openidea/laravel-generators.json",r#"{"version":1,"presets":[{"id":"model_repo","label":"Model repository","parameters":[{"name":"model","label":"Model","type":"model"}],"calculated":[{"name":"table","label":"Table name","expression":"${model|className|snake|plural}"}],"files":[{"path":"app/Repositories/${table|studly}.php","template":"model_repo.php"}]}]}"#);
        h.add_file(
            ".openidea/laravel-templates/model_repo.php",
            "<?php // ${table|php}",
        );
        let index = crate::index::Index::build(&h.invocation().project().unwrap());
        state.set_models(&h.invocation(), &index).unwrap();
    }
    fn model_request(op: &str, model: &str) -> String {
        json!({"op":op,"preset":"model_repo","values":{"model":model},"overrides":{"table":"manual_records"}}).to_string()
    }
    #[test]
    fn indexed_model_choices_and_live_values_use_owned_inventory_and_explicit_overrides() {
        let (h, mut state, _) = setup();
        model_preset(&h, &mut state);
        let response = state.handle(&h.invocation(), r#"{"op":"state"}"#).unwrap();
        assert_eq!(response["models"][0]["class"], "App\\Models\\UserProfile");
        assert_eq!(response["modelsComplete"], true);
        let live = state
            .handle(
                &h.invocation(),
                &model_request("evaluate-preset", "App\\Models\\UserProfile"),
            )
            .unwrap();
        assert_eq!(live["values"]["table"], "manual_records");
        let preview = state
            .handle(
                &h.invocation(),
                &model_request("preview-preset", "App\\Models\\UserProfile"),
            )
            .unwrap();
        assert_eq!(
            preview["files"][0]["path"],
            "app/Repositories/ManualRecords.php"
        );
        assert!(h.process_runs().is_empty());
        assert!(h.terminals().is_empty());
        assert!(
            state
                .handle(
                    &h.invocation(),
                    &model_request("evaluate-preset", "App\\Models\\Unknown")
                )
                .is_err()
        );
        h.add_project("/project/app/Models");
        assert!(
            state
                .handle(
                    &h.invocation(),
                    &model_request("preview-preset", "App\\Models\\UserProfile")
                )
                .is_err()
        );
    }
    #[test]
    fn live_evaluation_does_not_read_or_execute_model_sources_but_preparation_revalidates_them() {
        let (h, mut state, _) = setup();
        model_preset(&h, &mut state);
        h.add_file(
            "app/Models/UserProfile.php",
            "<?php namespace App\\Models; class Renamed {}",
        );
        let live = state
            .handle(
                &h.invocation(),
                &model_request("evaluate-preset", "App\\Models\\UserProfile"),
            )
            .unwrap();
        assert_eq!(live["values"]["table"], "manual_records");
        assert!(
            state
                .handle(
                    &h.invocation(),
                    &model_request("prepare-preset", "App\\Models\\UserProfile")
                )
                .is_err()
        );
        assert!(h.process_runs().is_empty());
    }
    #[test]
    fn changing_selected_model_source_invalidates_prepared_calculated_generation() {
        let (h, mut state, doc) = setup();
        model_preset(&h, &mut state);
        state
            .handle(
                &h.invocation(),
                &model_request("prepare-preset", "App\\Models\\UserProfile"),
            )
            .unwrap();
        let action = state.actions(&doc).remove(0);
        h.add_file("app/Models/UserProfile.php","<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class UserProfile extends Model { protected $table = 'profiles'; }");
        assert!(state.resolve(&doc, action).is_err());
    }
    #[test]
    fn preparation_returns_one_undoable_nonoverwriting_transaction_without_writes() {
        let (h, mut state, doc) = setup();
        let response = state.handle(&h.invocation(), &request("prepare")).unwrap();
        assert_eq!(response["prepared"], true);
        let lazy = state.actions(&doc).remove(0);
        assert!(lazy.operations.is_empty());
        h.answer_next_prompt(Some(&["yes"]));
        let ready = state.resolve(&doc, lazy.clone()).unwrap();
        assert_eq!(ready.operations.len(), 3);
        assert!(ready.operations.iter().all(|o| matches!(
            o,
            FileOperation::Create {
                overwrite: false,
                ..
            }
        )));
        assert!(h.process_runs().is_empty());
        assert!(h.terminals().is_empty());
        assert!(
            h.invocation()
                .project()
                .unwrap()
                .read_file("app/Models/Post.php")
                .is_err()
        );
        assert!(state.resolve(&doc, lazy).is_err());
    }
    #[test]
    fn collision_and_composer_changes_invalidate_prepared_generation() {
        let (h, mut state, doc) = setup();
        state.handle(&h.invocation(), &request("prepare")).unwrap();
        let lazy = state.actions(&doc).remove(0);
        h.add_file("app/Models/Post.php", "existing");
        assert!(state.resolve(&doc, lazy).is_err());
        let (h, mut state, doc) = setup();
        state.handle(&h.invocation(), &request("prepare")).unwrap();
        let lazy = state.actions(&doc).remove(0);
        h.add_file(
            "composer.json",
            r#"{"autoload":{"psr-4":{"App\\":"other/"}}}"#,
        );
        assert!(state.resolve(&doc, lazy).is_err());
    }
    #[test]
    fn cancel_stale_anchor_and_discard_never_write() {
        let (h, mut state, doc) = setup();
        state.handle(&h.invocation(), &request("prepare")).unwrap();
        let lazy = state.actions(&doc).remove(0);
        let mut changed = doc.clone();
        changed.text.push(' ');
        assert!(state.resolve(&changed, lazy.clone()).is_err());
        assert_eq!(state.resolve(&doc, lazy), Err(Error::Cancelled));
        state
            .handle(&h.invocation(), r#"{"op":"discard"}"#)
            .unwrap();
        assert!(state.actions(&doc).is_empty());
        assert!(h.process_runs().is_empty());
    }
    #[test]
    fn foreign_project_cannot_apply_another_project_plan() {
        let (h, mut state, doc) = setup();
        state.handle(&h.invocation(), &request("prepare")).unwrap();
        h.add_project("/project/routes");
        assert!(state.actions(&doc).is_empty());
    }
    #[test]
    fn custom_template_changes_cannot_apply_an_old_preview() {
        let (h, mut state, doc) = setup();
        h.add_file(".openidea/laravel-generators.json",r#"{"version":1,"presets":[{"id":"simple","label":"Simple","files":[{"path":"app/Generated.php","template":"simple.php"}]}]}"#);
        h.add_file(
            ".openidea/laravel-templates/simple.php",
            "<?php // reviewed",
        );
        let reply = state
            .handle(
                &h.invocation(),
                r#"{"op":"prepare-preset","preset":"simple","values":{}}"#,
            )
            .unwrap();
        assert_eq!(reply["prepared"], true);
        let lazy = state.actions(&doc).remove(0);
        h.add_file(".openidea/laravel-templates/simple.php", "<?php // changed");
        assert!(state.resolve(&doc, lazy).is_err());
        assert!(
            h.invocation()
                .project()
                .unwrap()
                .read_file("app/Generated.php")
                .is_err()
        );
    }
}
#[derive(Default)]
pub(crate) struct State {
    next: u64,
    projects: BTreeMap<String, Session>,
    presets: crate::generator_preset_actions::State,
    existing: crate::existing_model_fields::State,
}
#[derive(Default)]
struct Session {
    draft: Option<Draft>,
    input: Option<Input>,
    pending: Option<Pending>,
    models: crate::eloquent_members::Models,
    models_complete: bool,
}
#[derive(Clone)]
enum Input {
    Model {
        draft: Draft,
        module: Option<String>,
    },
    Preset {
        preset: String,
        values: BTreeMap<String, Value>,
        module: Option<String>,
        overrides: BTreeMap<String, Value>,
    },
}
struct Pending {
    key: String,
    plan: Plan,
    evidence: String,
    anchor: String,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
enum Request {
    State,
    Artisan,
    NewModel,
    PreviewFields {
        draft: crate::existing_model_fields::Draft,
    },
    PrepareFields {
        draft: crate::existing_model_fields::Draft,
    },
    InspectConfig {
        scope: String,
    },
    PreviewConfig {
        scope: String,
        config: Value,
        templates: BTreeMap<String, String>,
        #[serde(default, rename = "removeTemplates")]
        remove_templates: Vec<String>,
    },
    PrepareConfig {
        scope: String,
        config: Value,
        templates: BTreeMap<String, String>,
        #[serde(default, rename = "removeTemplates")]
        remove_templates: Vec<String>,
    },
    Preview {
        draft: Draft,
        #[serde(default)]
        module: Option<String>,
    },
    Prepare {
        draft: Draft,
        #[serde(default)]
        module: Option<String>,
    },
    PreviewPreset {
        preset: String,
        values: BTreeMap<String, Value>,
        #[serde(default)]
        module: Option<String>,
        #[serde(default)]
        overrides: BTreeMap<String, Value>,
    },
    PreparePreset {
        preset: String,
        values: BTreeMap<String, Value>,
        #[serde(default)]
        module: Option<String>,
        #[serde(default)]
        overrides: BTreeMap<String, Value>,
    },
    EvaluatePreset {
        preset: String,
        values: BTreeMap<String, Value>,
        #[serde(default)]
        module: Option<String>,
        #[serde(default)]
        overrides: BTreeMap<String, Value>,
    },
    Discard,
}

fn owner(ctx: &Invocation) -> Result<(Project, String), Error> {
    let p = ctx
        .project()
        .ok_or_else(|| Error::Invalid("Open a Laravel project first".into()))?;
    let root = p.root()?;
    if !host::projects()?
        .iter()
        .any(|p| p.root().ok().as_deref() == Some(&root))
    {
        return Err(Error::Stale);
    }
    Ok((p, root))
}
fn owned(root: &str, path: &str) -> bool {
    host::project_of(path)
        .and_then(|p| p.root().ok())
        .as_deref()
        == Some(root)
}
#[derive(Clone)]
struct ComposerContext {
    path: String,
    directory: String,
    value: Value,
}
fn composer_relative(base: &str, path: &str) -> Result<String, Error> {
    if path.starts_with('/')
        || path.contains(['\\', ':', '\0'])
        || path.chars().any(char::is_control)
    {
        return Err(Error::Invalid(
            "Composer source roots must remain slash-normalized project paths".into(),
        ));
    }
    let mut parts = base
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(Error::Invalid(
                        "Composer source root escapes this project".into(),
                    ));
                }
            }
            part => {
                if !generator_plan::relative(part) {
                    return Err(Error::Invalid(
                        "Invalid Composer source-root segment".into(),
                    ));
                }
                parts.push(part.into());
            }
        }
    }
    Ok(parts.join("/"))
}
fn composer_candidates(roots: &[&str]) -> Result<Vec<String>, Error> {
    let mut paths = std::collections::BTreeSet::from(["composer.json".to_owned()]);
    for root in roots {
        if !generator_plan::relative(root) {
            return Err(Error::Invalid(
                "Generator source roots must be project-relative paths".into(),
            ));
        }
        let mut parts = root.split('/').collect::<Vec<_>>();
        if parts.len() > 16 {
            return Err(Error::Invalid(
                "Generator source root exceeds Composer-discovery depth".into(),
            ));
        }
        while !parts.is_empty() {
            paths.insert(format!("{}/composer.json", parts.join("/")));
            parts.pop();
        }
    }
    if paths.len() > 64 {
        return Err(Error::Invalid(
            "Too many Composer namespace contexts".into(),
        ));
    }
    Ok(paths.into_iter().collect())
}
fn composer_contexts(
    project: &Project,
    roots: &[&str],
) -> Result<(Vec<ComposerContext>, String), Error> {
    let root = project.root()?;
    let mut contexts = Vec::new();
    let mut evidence = BTreeMap::<String, Option<String>>::new();
    let mut bytes = 0usize;
    for path in composer_candidates(roots)? {
        let absolute = format!("{root}/{path}");
        if !owned(&root, &absolute) {
            return Err(Error::Stale);
        }
        match project.stat(&path) {
            Err(Error::NotFound(_)) => {
                evidence.insert(path, None);
                continue;
            }
            Ok(stat) if stat.kind == openidea_plugin::FileKind::File => {}
            Ok(_) => {
                return Err(Error::Invalid(format!(
                    "Composer metadata must be an ordinary file: {path}"
                )));
            }
            Err(error) => return Err(error),
        }
        let directory = path
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("")
            .to_owned();
        let mut parts = directory
            .split('/')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        while !parts.is_empty() {
            let parent = parts.join("/");
            if project.stat(&parent)?.kind != openidea_plugin::FileKind::Directory {
                return Err(Error::Invalid(
                    "Composer metadata cannot cross a symlink or non-directory".into(),
                ));
            }
            parts.pop();
        }
        let source = project.read_file(&path)?;
        bytes += source.len();
        if bytes > 2 << 20 {
            return Err(Error::Invalid(
                "Composer namespace evidence exceeds 2 MiB".into(),
            ));
        }
        let value = crate::strict_json::parse(&source, 128 << 10)?;
        if !value.is_object() {
            return Err(Error::Invalid(format!(
                "Composer metadata must be an object: {path}"
            )));
        }
        evidence.insert(path.clone(), Some(source));
        contexts.push(ComposerContext {
            path,
            directory,
            value,
        });
    }
    Ok((
        contexts,
        serde_json::to_string(&evidence).map_err(|_| Error::Stale)?,
    ))
}
fn mapped_class(
    project: &Project,
    contexts: &[ComposerContext],
    class: &str,
    path: &str,
    allow_dev: bool,
) -> Result<(), Error> {
    if !generator_plan::qualified(class) || !generator_plan::relative(path) {
        return Err(Error::Invalid("Invalid generated class or path".into()));
    }
    let mut matches = Vec::<(usize, String, std::collections::BTreeSet<String>)>::new();
    for context in contexts {
        for section in if allow_dev {
            vec!["autoload", "autoload-dev"]
        } else {
            vec!["autoload"]
        } {
            let Some(mappings) = context.value[section]["psr-4"].as_object() else {
                continue;
            };
            for (prefix, roots) in mappings {
                if !prefix.is_empty()
                    && (!prefix.ends_with('\\')
                        || !generator_plan::qualified(prefix.trim_end_matches('\\')))
                {
                    continue;
                }
                let Some(suffix) = class.strip_prefix(prefix) else {
                    continue;
                };
                let roots = match roots {
                    Value::String(root) => vec![root.as_str()],
                    Value::Array(roots) => roots
                        .iter()
                        .map(|root| {
                            root.as_str().ok_or_else(|| {
                                Error::Invalid(format!("Invalid PSR-4 root in {}", context.path))
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    _ => {
                        return Err(Error::Invalid(format!(
                            "Invalid PSR-4 mapping for {prefix}"
                        )));
                    }
                };
                if roots.is_empty() || roots.len() > 32 {
                    return Err(Error::Invalid("Choose 1–32 declared PSR-4 roots".into()));
                }
                let mut destinations = std::collections::BTreeSet::new();
                for root in roots {
                    let directory = composer_relative(&context.directory, root)?;
                    let file = format!(
                        "{}{}.php",
                        if directory.is_empty() {
                            String::new()
                        } else {
                            format!("{directory}/")
                        },
                        suffix.replace('\\', "/")
                    );
                    if !generator_plan::relative(&file) {
                        return Err(Error::Invalid(
                            "Composer mapping produced an invalid destination".into(),
                        ));
                    }
                    destinations.insert(file);
                }
                matches.push((prefix.len(), context.path.clone(), destinations));
            }
        }
    }
    let longest = matches
        .iter()
        .map(|(length, _, _)| *length)
        .max()
        .ok_or_else(|| {
            Error::Invalid(format!("No owned Composer PSR-4 declaration maps {class}"))
        })?;
    let matching = matches
        .into_iter()
        .filter(|(length, _, _)| *length == longest)
        .collect::<Vec<_>>();
    for (left, (_, manifest, roots)) in matching.iter().enumerate() {
        for (_, other_manifest, other_roots) in matching.iter().skip(left + 1) {
            if manifest != other_manifest && roots.is_disjoint(other_roots) {
                return Err(Error::Invalid(format!(
                    "Conflicting Composer package mappings for {class}; resolve their namespaces before generating"
                )));
            }
        }
    }
    let destinations = matching
        .into_iter()
        .flat_map(|(_, _, roots)| roots)
        .collect::<std::collections::BTreeSet<_>>();
    if !destinations.contains(path) {
        return Err(Error::Invalid(format!(
            "Composer maps {class} to a different source root; expected generated path {path}"
        )));
    }
    let project_root = project.root()?;
    for other in destinations.iter().filter(|other| other.as_str() != path) {
        let absolute = format!("{project_root}/{other}");
        if !owned(&project_root, &absolute) {
            return Err(Error::Stale);
        }
        match project.stat(other) {
            Err(Error::NotFound(_)) => {}
            Ok(_) => {
                return Err(Error::Invalid(format!(
                    "Another Composer source root already contains {class}: {other}"
                )));
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
/// Reuse the wizard's owned ancestor/package PSR-4 proof for related actions.
pub(crate) fn related_mapping(project: &Project, class: &str, path: &str, allow_dev: bool) -> Result<String, Error> {
    let directory = path.rsplit_once('/').map(|(directory, _)| directory).ok_or(Error::Stale)?;
    let (contexts, evidence) = composer_contexts(project, &[directory])?;
    mapped_class(project, &contexts, class, path, allow_dev)?;
    Ok(evidence)
}
fn mapping(project: &Project, draft: &Draft) -> Result<String, Error> {
    let kinds = std::iter::once("model")
        .chain(draft.related.iter().map(String::as_str))
        .filter(|kind| *kind != "migration")
        .collect::<Vec<_>>();
    let roots = kinds
        .iter()
        .map(|kind| draft.directory(kind))
        .collect::<Vec<_>>();
    let references = roots.iter().map(String::as_str).collect::<Vec<_>>();
    let (contexts, evidence) = composer_contexts(project, &references)?;
    for kind in kinds {
        let name = draft.class_name(kind);
        mapped_class(
            project,
            &contexts,
            &format!("{}\\{name}", draft.namespace_for(kind)),
            &format!("{}/{name}.php", draft.directory(kind)),
            matches!(kind, "factory" | "seeder"),
        )?;
    }
    Ok(evidence)
}

fn validate(project: &Project, root: &str, plan: &Plan) -> Result<(), Error> {
    for file in &plan.files {
        let absolute = format!("{root}/{}", file.path);
        if !generator_plan::relative(&file.path) || !owned(root, &absolute) {
            return Err(Error::Stale);
        }
        match project.stat(&file.path) {
            Err(Error::NotFound(_)) => {}
            Ok(_) => {
                return Err(Error::Invalid(format!(
                    "Target already exists: {}",
                    file.path
                )));
            }
            Err(e) => return Err(e),
        }
        // Require every existing ancestor to be an ordinary directory. This also
        // excludes symlink roots even on hosts whose project ownership follows them.
        let mut parts = file.path.split('/').collect::<Vec<_>>();
        parts.pop();
        while !parts.is_empty() {
            let parent = parts.join("/");
            match project.stat(&parent) {
                Ok(stat) if stat.kind == openidea_plugin::FileKind::Directory => {}
                Ok(_) => {
                    return Err(Error::Invalid(format!(
                        "Target parent is not a directory: {parent}"
                    )));
                }
                Err(Error::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
            parts.pop();
        }
    }
    Ok(())
}
fn model_catalog(models: &crate::eloquent_members::Models, root: &str) -> Vec<Value> {
    crate::eloquent_members::model_inventory(models)
        .into_iter()
        .filter(|(_, path, _)| owned(root, path))
        .take(512)
        .map(|(class, path, table)| json!({"class":class,"path":path,"table":table}))
        .collect()
}
fn indexed_model_inputs(
    bundle: &crate::generator_presets::Bundle,
    models: &crate::eloquent_members::Models,
    preset: &str,
    values: &BTreeMap<String, Value>,
    root: &str,
) -> Result<Vec<String>, Error> {
    let chosen = bundle
        .model_inputs(preset, values)
        .map_err(Error::Invalid)?;
    let catalog = model_catalog(models, root);
    for model in &chosen {
        if !catalog
            .iter()
            .any(|entry| entry["class"].as_str() == Some(model.trim_start_matches('\\')))
        {
            return Err(Error::Invalid(format!(
                "Choose a currently indexed model: {model}"
            )));
        }
    }
    Ok(chosen)
}
fn fresh_model_evidence(
    project: &Project,
    models: &crate::eloquent_members::Models,
    chosen: &[String],
) -> Result<String, Error> {
    if chosen.is_empty() {
        return Ok(String::new());
    }
    let root = project.root()?;
    let inventory = crate::eloquent_members::model_inventory(models);
    if inventory.len() > 512 {
        return Err(Error::Invalid(
            "Model inventory exceeds the exact generation snapshot budget".into(),
        ));
    }
    let mut files = BTreeMap::new();
    let mut fresh = crate::eloquent_members::Models::default();
    let mut total = 0usize;
    for (_, path, _) in inventory {
        if !owned(&root, &path) {
            continue;
        }
        if files.contains_key(&path) {
            continue;
        }
        let source = project.read_file(&path)?;
        total += source.len();
        if source.len() > 128 << 10 || total > 8 << 20 {
            return Err(Error::Invalid(
                "Model sources exceed generation snapshot limits".into(),
            ));
        }
        crate::eloquent_members::cache(&mut fresh, &source, &path);
        files.insert(path, source);
    }
    let current = crate::eloquent_members::model_inventory(&fresh);
    for name in chosen {
        if !current
            .iter()
            .any(|(full, path, _)| full == name.trim_start_matches('\\') && owned(&root, path))
        {
            return Err(Error::Stale);
        }
    }
    serde_json::to_string(&files).map_err(|_| Error::Stale)
}
fn build(
    project: &Project,
    input: &Input,
    models: &crate::eloquent_members::Models,
) -> Result<(Plan, String), Error> {
    let bundle = crate::generator_presets::load(project)?;
    let fingerprint = bundle.fingerprint();
    match input {
        Input::Model { draft, module } => {
            let mut draft = draft.clone();
            bundle.apply_settings(&mut draft);
            if let Some(module) = module {
                bundle
                    .apply_module(&mut draft, module)
                    .map_err(Error::Invalid)?;
            }
            bundle
                .apply_generator_options(&mut draft, module.as_deref())
                .map_err(Error::Invalid)?;
            let composer = mapping(project, &draft)?;
            let plan = generator_plan::plan(&draft).map_err(Error::Invalid)?;
            Ok((plan, format!("{fingerprint}\n{composer}")))
        }
        Input::Preset {
            preset,
            values,
            module,
            overrides,
        } => {
            let chosen = indexed_model_inputs(&bundle, models, preset, values, &project.root()?)?;
            let evidence = fresh_model_evidence(project, models, &chosen)?;
            Ok((
                bundle
                    .render_with_overrides(preset, values, module.as_deref(), overrides)
                    .map_err(Error::Invalid)?,
                format!("{fingerprint}\n{evidence}"),
            ))
        }
    }
}
fn snapshot(plan: &Plan) -> Value {
    json!({"files":plan.files,"canPrepare":true})
}
impl State {
    /// Index-only inventory snapshot; source contents are read only for explicit
    /// preview/preparation/application, never during view rendering.
    pub(crate) fn set_models(
        &mut self,
        ctx: &Invocation,
        index: &crate::index::Index,
    ) -> Result<(), Error> {
        let (_, root) = owner(ctx)?;
        if !self.projects.contains_key(&root) && self.projects.len() >= 8 {
            return Err(Error::Invalid("Too many generator project sessions".into()));
        }
        let session = self.projects.entry(root).or_default();
        session.models = if index.partial {
            Default::default()
        } else {
            index.eloquent_models.clone()
        };
        session.models_complete = !index.partial;
        Ok(())
    }
    pub(crate) fn seed_existing(
        &mut self,
        ctx: &Invocation,
        index: &crate::index::Index,
        path: &str,
    ) -> Result<(), Error> {
        self.existing.seed(ctx, index, path)?;
        ui_kit::web::reveal_view(VIEW, include_str!("ui_config.json"))
    }

    pub(crate) fn seed_model(&mut self, ctx: &Invocation, draft: Draft) -> Result<(), Error> {
        let (_, root) = owner(ctx)?;
        generator_plan::plan(&draft).map_err(Error::Invalid)?;
        if !self.projects.contains_key(&root) && self.projects.len() >= 8 {
            return Err(Error::Invalid(
                "Too many generator projects; reload to retire inactive sessions".into(),
            ));
        }
        let session = self.projects.entry(root).or_default();
        session.pending = None;
        session.input = None;
        session.draft = Some(draft);
        ui_kit::web::reveal_view(VIEW, include_str!("ui_config.json"))
    }
    pub(crate) fn handle(&mut self, ctx: &Invocation, message: &str) -> Result<Value, Error> {
        if message.len() > 128 * 1024 {
            return Err(Error::Invalid("Generator request is too large".into()));
        }
        let request: Request =
            serde_json::from_str(message).map_err(|e| Error::Invalid(e.to_string()))?;
        let (project, root) = owner(ctx)?;
        let request = match request {
            Request::NewModel => {
                self.existing.close(ctx)?;
                return Ok(json!({"existing":null}));
            }
            Request::PreviewFields { draft } => return self.existing.handle(ctx, draft, false),
            Request::PrepareFields { draft } => return self.existing.handle(ctx, draft, true),
            Request::InspectConfig { scope } => {
                return match scope.as_str() {
                    "project" => crate::generator_preset_actions::inspect(ctx),
                    "user" => crate::generator_presets::user_document(),
                    _ => Err(Error::Invalid("Unknown preset scope".into())),
                };
            }
            Request::PreviewConfig {
                scope,
                config,
                templates,
                remove_templates,
            } => {
                return match scope.as_str() {
                    "project" => self
                        .presets
                        .preview(ctx, config, templates, remove_templates),
                    "user" => {
                        crate::generator_presets::from_parts(config.clone(), templates.clone())?;
                        let before = crate::generator_presets::user_document()?;
                        Ok(
                            json!({"files":[{"path":"User-level generator settings","before":serde_json::to_string_pretty(&before).unwrap_or_default(),"after":serde_json::to_string_pretty(&json!({"config":config,"templates":templates})).unwrap_or_default(),"operation":"edit"}]}),
                        )
                    }
                    _ => Err(Error::Invalid("Unknown preset scope".into())),
                };
            }
            Request::PrepareConfig {
                scope,
                config,
                templates,
                remove_templates,
            } => {
                return match scope.as_str() {
                    "project" => self
                        .presets
                        .prepare(ctx, config, templates, remove_templates),
                    "user" => crate::generator_presets::save_user(config, templates),
                    _ => Err(Error::Invalid("Unknown preset scope".into())),
                };
            }
            request => request,
        };
        if !self.projects.contains_key(&root) && self.projects.len() >= 8 {
            return Err(Error::Invalid(
                "Too many generator projects; reload to retire inactive sessions".into(),
            ));
        }
        let session = self.projects.entry(root.clone()).or_default();
        if let Request::EvaluatePreset {
            preset,
            values,
            module,
            overrides,
        } = &request
        {
            let bundle = crate::generator_presets::load(&project)?;
            indexed_model_inputs(&bundle, &session.models, preset, values, &root)?;
            let mut result = bundle
                .evaluate(preset, values, module.as_deref(), overrides)
                .map_err(Error::Invalid)?;
            result["preset"] = preset.clone().into();
            return Ok(result);
        }
        if matches!(request, Request::Artisan) {
            ui_kit::web::reveal_view("laravel-artisan", include_str!("ui_config.json"))?;
            return Ok(
                json!({"instruction":"Choose a make:* command in Artisan; load command metadata explicitly if needed."}),
            );
        }
        if matches!(request, Request::State) {
            let bundle = crate::generator_presets::load(&project)?;
            return Ok(
                json!({"draft":session.draft,"existing":self.existing.state(ctx)?,"catalog":bundle.catalog(),"models":model_catalog(&session.models,&root),"modelsComplete":session.models_complete,"project":root,"prepared":session.pending.is_some(),"instruction":"Preview files, then prepare and apply the editor action. Native generation runs no PHP."}),
            );
        }
        if matches!(request, Request::Discard) {
            session.pending = None;
            self.presets.discard(ctx)?;
            self.existing.discard(ctx)?;
            return Ok(json!({"prepared":false}));
        }
        let (input, prepare) = match request {
            Request::Preview { draft, module } => (Input::Model { draft, module }, false),
            Request::Prepare { draft, module } => (Input::Model { draft, module }, true),
            Request::PreviewPreset {
                preset,
                values,
                module,
                overrides,
            } => (
                Input::Preset {
                    preset,
                    values,
                    module,
                    overrides,
                },
                false,
            ),
            Request::PreparePreset {
                preset,
                values,
                module,
                overrides,
            } => (
                Input::Preset {
                    preset,
                    values,
                    module,
                    overrides,
                },
                true,
            ),
            _ => unreachable!(),
        };
        session.pending = None;
        let (plan, evidence) = build(&project, &input, &session.models)?;
        validate(&project, &root, &plan)?;
        if let Input::Model { draft, .. } = &input {
            session.draft = Some(draft.clone());
        }
        session.input = Some(input);
        if !prepare {
            return Ok(snapshot(&plan));
        }
        let editors = host::open_editors()?;
        if plan.files.iter().any(|f| {
            editors
                .iter()
                .any(|e| e.path == format!("{root}/{}", f.path))
        }) {
            return Err(Error::Invalid(
                "A generated target is open in the editor".into(),
            ));
        }
        let anchor = editors
            .iter()
            .find(|e| e.active && !e.dirty && e.language == "php" && owned(&root, &e.path))
            .or_else(|| {
                editors
                    .iter()
                    .find(|e| !e.dirty && e.language == "php" && owned(&root, &e.path))
            })
            .map(|e| e.path.clone())
            .unwrap_or_else(|| format!("{root}/routes/web.php"));
        if !owned(&root, &anchor) || project.read_file(&anchor).is_err() {
            return Err(Error::Invalid("Open an existing saved PHP file in this project to anchor the undoable generation action".into()));
        }
        self.next = self.next.checked_add(1).ok_or(Error::Stale)?;
        let key = format!("generator-apply:{}", self.next);
        host::open_editor(&anchor, None)?;
        session.pending = Some(Pending {
            key,
            plan,
            evidence,
            anchor,
        });
        Ok(
            json!({"prepared":true,"instruction":"In the opened PHP editor, choose Apply prepared Laravel generation… from editor actions. One undo reverses all created files."}),
        )
    }

    pub(crate) fn actions(&self, doc: &Document) -> Vec<FileAction> {
        let mut actions = self.presets.actions(doc);
        actions.extend(self.existing.actions(doc));
        actions.extend(
            self.projects
                .iter()
                .find_map(|(root, s)| {
                    s.pending
                        .as_ref()
                        .filter(|p| p.anchor == doc.path && owned(root, &doc.path))
                        .map(|p| FileAction {
                            title: "Apply prepared Laravel generation…".into(),
                            kind: Some("refactor".into()),
                            data: Some(p.key.clone()),
                            ..Default::default()
                        })
                })
                .into_iter()
                .collect::<Vec<_>>(),
        );
        actions
    }
    pub(crate) fn resolve(
        &mut self,
        doc: &Document,
        action: FileAction,
    ) -> Result<FileAction, Error> {
        if action
            .data
            .as_deref()
            .is_some_and(|key| key.starts_with("existing-model-fields:"))
        {
            return self.existing.resolve(doc, action);
        }
        if action
            .data
            .as_deref()
            .is_some_and(|key| key.starts_with("generator-presets:"))
        {
            return self.presets.resolve(doc, action);
        }
        let root = host::project_of(&doc.path)
            .and_then(|p| p.root().ok())
            .ok_or(Error::Stale)?;
        let project = host::project_of(&doc.path).ok_or(Error::Stale)?;
        let session = self.projects.get_mut(&root).ok_or(Error::Stale)?;
        let pending = session.pending.as_ref().ok_or(Error::Stale)?;
        if action.data.as_deref() != Some(&pending.key)
            || pending.anchor != doc.path
            || project.read_file(&doc.path)? != doc.text
        {
            return Err(Error::Stale);
        }
        let input = session.input.as_ref().ok_or(Error::Stale)?;
        let (current, evidence) = build(&project, input, &session.models)?;
        if evidence != pending.evidence || current != pending.plan {
            return Err(Error::Stale);
        }
        validate(&project, &root, &pending.plan)?;
        let preview = pending
            .plan
            .files
            .iter()
            .map(|f| format!("Create {}\n{}", f.path, f.text))
            .collect::<Vec<_>>()
            .join("\n\n");
        if !ui_kit::prompts::confirm_exact("Apply prepared Laravel generation?", &preview)? {
            return Err(Error::Cancelled);
        }
        let (current, evidence) = build(&project, input, &session.models)?;
        if evidence != pending.evidence
            || current != pending.plan
            || project.read_file(&doc.path)? != doc.text
        {
            return Err(Error::Stale);
        }
        validate(&project, &root, &pending.plan)?;
        let pending = session.pending.take().ok_or(Error::Stale)?;
        let open = pending
            .plan
            .files
            .iter()
            .find(|f| f.path.contains("/Models/"))
            .map(|f| format!("{root}/{}", f.path));
        Ok(FileAction {
            title: "Generate Laravel model and related files".into(),
            operations: pending
                .plan
                .files
                .into_iter()
                .map(|f| FileOperation::Create {
                    path: format!("{root}/{}", f.path),
                    text: f.text,
                    overwrite: false,
                })
                .collect(),
            open,
            ..Default::default()
        })
    }
}
