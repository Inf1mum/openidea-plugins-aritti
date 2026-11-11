//! Explicit model-instance facts, separate from schema data and source inference.
//! Reads only consume a current captured snapshot; they never bootstrap PHP.
use super::{State, execute, key, owner, stamp, target_binding};
use crate::artisan_runner::Runner;
use openidea_plugin::{Error, Invocation, Project};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn scalar(value: &Value, limit: usize) -> Result<String, Error> {
    value
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= limit && !s.chars().any(char::is_control))
        .map(str::to_owned)
        .ok_or(Error::Stale)
}
fn class(value: &str) -> bool {
    value.len() <= 512
        && value.split('\\').all(|part| {
            !part.is_empty()
                && !part.as_bytes()[0].is_ascii_digit()
                && part.bytes().all(crate::php::is_identifier)
        })
}
fn relative(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains(['\\', '\0'])
        && value.split('/').all(|s| !matches!(s, "" | "." | ".."))
        && !value.chars().any(char::is_control)
}
pub(super) fn parse(raw: &str) -> Result<Vec<Value>, Error> {
    let value = crate::strict_json::parse(raw, 256 * 1024)?;
    let default = scalar(&value["defaultConnection"], 128)?;
    let models = value["models"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 32)
        .ok_or(Error::Stale)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut rows = Vec::new();
    for model in models {
        let full = scalar(&model["class"], 512)?;
        let file = scalar(&model["file"], 1024)?;
        if !class(&full) || !relative(&file) || !seen.insert(full.clone()) {
            return Err(Error::Stale);
        }
        let source_hash = scalar(&model["sourceHash"], 16)?;
        if source_hash.len() != 16 || !source_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Stale);
        }
        let dependencies = model["dependencies"]
            .as_array()
            .filter(|v| v.len() <= 64)
            .ok_or(Error::Stale)?;
        let mut dependency_paths = std::collections::BTreeSet::new();
        let mut safe_dependencies = Vec::new();
        for dependency in dependencies {
            let file = scalar(&dependency["file"], 1024)?;
            let hash = scalar(&dependency["hash"], 16)?;
            if !relative(&file)
                || hash.len() != 16
                || !hash.bytes().all(|b| b.is_ascii_hexdigit())
                || !dependency_paths.insert(file.clone())
            {
                return Err(Error::Stale);
            }
            safe_dependencies.push(json!({"file":file,"hash":hash}));
        }
        let available = model["available"].as_bool().ok_or(Error::Stale)?;
        let mut safe = json!({"class":full,"file":file,"available":available,"sourceHash":source_hash,"dependencies":safe_dependencies});
        let details = if available {
            let connection = scalar(&model["connection"], 128)?;
            let table = scalar(&model["table"], 512)?;
            safe["connection"] = json!(connection);
            safe["table"] = json!(table);
            format!("{connection} · {table}")
        } else {
            safe["reason"] = json!("Framework could not resolve this exact owned model.");
            "Unavailable · exact model could not be resolved".into()
        };
        rows.push(json!({"key":format!("runtime:model-connections:{}",rows.len()),"cells":["Models",full,details],"activationEnabled":false,"modelConnection":safe,"modelDefaultConnection":default}));
    }
    Ok(rows)
}
impl State {
    pub(crate) fn capture_model_connections(
        &mut self,
        ctx: &Invocation,
        runner: Runner,
        models: &[(String, String)],
    ) -> Result<(), Error> {
        self.capture_model_connections_with(ctx, runner, models, |project, args| {
            execute(project, runner, args)
        })
    }
    fn capture_model_connections_with(
        &mut self,
        ctx: &Invocation,
        runner: Runner,
        models: &[(String, String)],
        run: impl FnOnce(&Project, &[String]) -> Result<String, Error>,
    ) -> Result<(), Error> {
        let (project, root) = owner(ctx)?;
        if models.is_empty() || models.len() > 32 {
            return Err(crate::ui_text::text(
                "final.runtime_model_connections.error0",
                "Select between 1 and 32 owned model classes.",
            )
            .into());
        }
        let mut evidence = BTreeMap::new();
        let mut inputs = Vec::new();
        let mut total = 0;
        for (full, path) in models {
            let file = path
                .strip_prefix(&format!("{root}/"))
                .filter(|file| relative(file))
                .ok_or(Error::Stale)?;
            if !class(full)
                || !crate::owns_current_path(&project, path)
                || evidence.contains_key(full)
            {
                return Err(Error::Stale);
            }
            let source = project.read_file(path)?;
            total += source.len();
            if source.len() > 512 * 1024 || total > 8 * 1024 * 1024 {
                return Err(crate::ui_text::text(
                    "final.runtime_model_connections.error1",
                    "Model source capture exceeds its bounds.",
                )
                .into());
            }
            evidence.insert(
                full.clone(),
                (file.to_owned(), crate::artisan_metadata::hash(&source)),
            );
            inputs.push(json!({"class":full,"file":file}));
        }
        self.capture_options_with(
            ctx,
            runner,
            "model-connections",
            json!({"models":inputs}),
            |project, args| {
                let raw = run(project, args)?;
                let mut value = crate::strict_json::parse(&raw, 256 * 1024)?;
                let output = value["models"]
                    .as_array_mut()
                    .filter(|v| v.len() == evidence.len())
                    .ok_or(Error::Stale)?;
                let mut returned = std::collections::BTreeSet::new();
                for model in output {
                    let full = model["class"].as_str().ok_or(Error::Stale)?.to_owned();
                    let (file, hash) = evidence.get(&full).ok_or(Error::Stale)?;
                    if !returned.insert(full) || model["file"] != file.as_str() {
                        return Err(Error::Stale);
                    }
                    let path = format!("{root}/{file}");
                    if !crate::owns_current_path(project, &path)
                        || crate::artisan_metadata::hash(&project.read_file(&path)?) != *hash
                    {
                        return Err(Error::Stale);
                    }
                    let paths = model["dependencies"]
                        .as_array()
                        .filter(|v| v.len() <= 64)
                        .ok_or(Error::Stale)?;
                    let mut dependencies = Vec::new();
                    let mut total = 0;
                    let mut seen = std::collections::BTreeSet::new();
                    for dependency in paths {
                        let file = dependency["file"]
                            .as_str()
                            .filter(|f| relative(f))
                            .ok_or(Error::Stale)?;
                        let reported = dependency["hash"].as_str().ok_or(Error::Stale)?;
                        if !seen.insert(file) {
                            return Err(Error::Stale);
                        }
                        let path = format!("{root}/{file}");
                        if !crate::owns_current_path(project, &path) {
                            return Err(Error::Stale);
                        }
                        let source = project.read_file(&path)?;
                        total += source.len();
                        if source.len() > 512 * 1024 || total > 8 * 1024 * 1024 {
                            return Err(Error::Stale);
                        }
                        let current = crate::artisan_metadata::hash(&source);
                        if current != reported {
                            return Err(Error::Stale);
                        }
                        dependencies.push(json!({"file":file,"hash":current}));
                    }
                    model["dependencies"] = json!(dependencies);
                    model["sourceHash"] = json!(hash);
                }
                Ok(value.to_string())
            },
        )
    }
    pub(crate) fn model_connections_for_project(
        &self,
        project: &Project,
        runner: Runner,
    ) -> Result<Value, Error> {
        let root = project.root()?;
        if !crate::hover::canonical(&root)
            || !openidea_plugin::host::projects()?
                .iter()
                .any(|p| p.root().ok().as_deref() == Some(&root))
        {
            return Err(Error::Stale);
        }
        let target = target_binding(project, runner)?;
        let Some(snapshot) = self.snapshots.get(&key(&root, runner, "model-connections")) else {
            return Ok(
                json!({"models":[],"defaultConnection":null,"capturedAt":null,"runner":runner.name()}),
            );
        };
        if snapshot.target != target || snapshot.identity != stamp(project)? {
            return Err(Error::Stale);
        }
        let mut models = Vec::new();
        for row in &snapshot.rows {
            let model = &row["modelConnection"];
            let file = model["file"]
                .as_str()
                .filter(|f| relative(f))
                .ok_or(Error::Stale)?;
            let path = format!("{root}/{file}");
            if !crate::owns_current_path(project, &path)
                || crate::artisan_metadata::hash(&project.read_file(&path)?)
                    != model["sourceHash"].as_str().ok_or(Error::Stale)?
            {
                return Err(Error::Stale);
            }
            for dependency in model["dependencies"].as_array().ok_or(Error::Stale)? {
                let file = dependency["file"]
                    .as_str()
                    .filter(|f| relative(f))
                    .ok_or(Error::Stale)?;
                let path = format!("{root}/{file}");
                if !crate::owns_current_path(project, &path)
                    || crate::artisan_metadata::hash(&project.read_file(&path)?)
                        != dependency["hash"].as_str().ok_or(Error::Stale)?
                {
                    return Err(Error::Stale);
                }
            }
            let mut model = model.clone();
            model["file"] = json!(path);
            models.push(model);
        }
        Ok(
            json!({"models":models,"defaultConnection":snapshot.rows.first().map(|r|&r["modelDefaultConnection"]),"capturedAt":snapshot.captured,"runner":runner.name()}),
        )
    }
    /// Application-default provenance is independent from per-model hashes.
    /// Callers must separately prove the current model uses this default.
    pub(crate) fn captured_default_connection(
        &self,
        project: &Project,
        runner: Runner,
    ) -> Result<Option<String>, Error> {
        let root = project.root()?;
        if !crate::hover::canonical(&root)
            || !openidea_plugin::host::projects()?
                .iter()
                .any(|p| p.root().ok().as_deref() == Some(&root))
        {
            return Err(Error::Stale);
        }
        let Some(snapshot) = self.snapshots.get(&key(&root, runner, "model-connections")) else {
            return Ok(None);
        };
        if snapshot.target != target_binding(project, runner)?
            || snapshot.identity != stamp(project)?
        {
            return Err(Error::Stale);
        }
        let mut default = None;
        for row in &snapshot.rows {
            let Some(value) = row["modelDefaultConnection"].as_str() else {
                return Ok(None);
            };
            if default.as_deref().is_some_and(|existing| existing != value) {
                return Err(Error::Stale);
            }
            default = Some(scalar(&json!(value), 128)?);
        }
        Ok(default)
    }
    pub(crate) fn helper_schema(
        &self,
        project: &Project,
        runner: Runner,
        full: &str,
        path: &str,
    ) -> Result<Option<Value>, Error> {
        let capture = self.model_connections_for_project(project, runner)?;
        let Some(model) = capture["models"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|model| {
                model["class"] == full && model["file"] == path && model["available"] == true
            })
        else {
            return Ok(None);
        };
        let schema = self.schema_for_generation(project, runner)?;
        if schema["connection"] != model["connection"] || schema["connection"].is_null() {
            return Ok(None);
        }
        let table = model["table"].as_str().ok_or(Error::Stale)?;
        let matches: Vec<_> = schema["tables"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|candidate| {
                let Some(name) = candidate["name"].as_str() else {
                    return false;
                };
                let namespace = candidate["schema"].as_str().unwrap_or("");
                if table.contains('.') {
                    format!("{namespace}.{name}") == table
                } else {
                    name == table
                }
            })
            .collect();
        if matches.len() != 1 {
            return Ok(None);
        }
        let table = matches[0];
        Ok(Some(
            json!({"model":full,"file":path,"connection":model["connection"],"table":table["name"],"schema":table["schema"],"columns":table["columns"],"indexes":table["indexes"],"capturedAt":schema["capturedAt"],"modelCapturedAt":capture["capturedAt"],"runner":runner.name()}),
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn fixture() -> (MockHost, State, Vec<(String, String)>) {
        let h = MockHost::install();
        h.add_file("artisan", "<?php");
        h.add_file("composer.json", "{}");
        h.add_file(
            "app/Models/Invoice.php",
            "<?php namespace App\\Models; class Invoice {}",
        );
        (
            h,
            State::default(),
            vec![(
                "App\\Models\\Invoice".into(),
                "/project/app/Models/Invoice.php".into(),
            )],
        )
    }
    fn payload() -> Value {
        json!({"defaultConnection":"sqlite","models":[{"class":"App\\Models\\Invoice","file":"app/Models/Invoice.php","available":true,"connection":"tenant","table":"invoices","dependencies":[{"file":"app/Models/Invoice.php","hash":crate::artisan_metadata::hash("<?php namespace App\\Models; class Invoice {}")}],"password":"private"}]})
    }
    #[test]
    fn captured_dynamic_connection_joins_only_exact_schema_without_running_php_on_reads() {
        let (h, mut state, models) = fixture();
        let ctx = h.invocation();
        state
            .capture_model_connections_with(&ctx, Runner::Php, &models, |_, args| {
                assert_eq!(args[2], "model-connections");
                Ok(payload().to_string())
            })
            .unwrap();
        state.capture_options_with(&ctx,Runner::Php,"schema",json!({"connection":"tenant"}),|_,_|Ok(json!({"connection":"tenant","tables":[{"name":"invoices","schema":"","columns":[{"name":"id","type":"integer","nullable":false}],"indexes":[]}]}).to_string())).unwrap();
        let project = ctx.project().unwrap();
        let facts = state
            .helper_schema(&project, Runner::Php, &models[0].0, &models[0].1)
            .unwrap()
            .unwrap();
        assert_eq!(facts["connection"], "tenant");
        assert_eq!(facts["columns"][0]["name"], "id");
        assert!(!facts.to_string().contains("private"));
        assert!(
            state
                .helper_schema(&project, Runner::Php, "App\\Models\\Other", &models[0].1)
                .unwrap()
                .is_none()
        );
        assert!(h.process_runs().is_empty());
    }
    #[test]
    fn captured_default_survives_model_only_changes_but_not_application_binding_changes() {
        let (h, mut state, models) = fixture();
        let ctx = h.invocation();
        let project = ctx.project().unwrap();
        assert_eq!(
            state
                .captured_default_connection(&project, Runner::Php)
                .unwrap(),
            None
        );
        state
            .capture_model_connections_with(&ctx, Runner::Php, &models, |_, _| {
                Ok(payload().to_string())
            })
            .unwrap();
        assert_eq!(
            state
                .captured_default_connection(&project, Runner::Php)
                .unwrap()
                .as_deref(),
            Some("sqlite")
        );
        h.add_file(
            "app/Models/Invoice.php",
            "<?php namespace App\\Models; class Invoice {protected $fillable=['title'];}",
        );
        assert!(
            state
                .model_connections_for_project(&project, Runner::Php)
                .is_err()
        );
        assert_eq!(
            state
                .captured_default_connection(&project, Runner::Php)
                .unwrap()
                .as_deref(),
            Some("sqlite")
        );
        h.add_file(
            "config/database.php",
            "<?php return ['default'=>'changed'];",
        );
        assert!(
            state
                .captured_default_connection(&project, Runner::Php)
                .is_err()
        );
        assert!(h.process_runs().is_empty() && h.requests().is_empty());
    }
    #[test]
    fn source_changes_unknown_classes_duplicate_rows_and_stale_inputs_do_not_establish_facts() {
        let (h, mut state, models) = fixture();
        let ctx = h.invocation();
        let mut bad = payload();
        bad["models"][0]["class"] = json!("App\\Models\\Other");
        assert!(
            state
                .capture_model_connections_with(&ctx, Runner::Php, &models, |_, _| Ok(
                    bad.to_string()
                ))
                .is_err()
        );
        state
            .capture_model_connections_with(&ctx, Runner::Php, &models, |_, _| {
                Ok(payload().to_string())
            })
            .unwrap();
        h.add_file("app/Models/Invoice.php", "<?php changed");
        assert!(
            state
                .model_connections_for_project(&ctx.project().unwrap(), Runner::Php)
                .is_err()
        );
    }
    #[test]
    fn missing_capture_and_unavailable_model_do_not_guess_default_connections() {
        let (h, mut state, models) = fixture();
        let ctx = h.invocation();
        assert!(
            state
                .model_connections_for_project(&ctx.project().unwrap(), Runner::Php)
                .unwrap()["models"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let mut p = payload();
        p["models"][0] = json!({"class":models[0].0,"file":"app/Models/Invoice.php","available":false,"dependencies":[],"reason":"SECRET DSN"});
        state
            .capture_model_connections_with(&ctx, Runner::Php, &models, |_, _| Ok(p.to_string()))
            .unwrap();
        let value = state
            .model_connections_for_project(&ctx.project().unwrap(), Runner::Php)
            .unwrap();
        assert!(!value.to_string().contains("SECRET"));
        assert!(
            state
                .helper_schema(
                    &ctx.project().unwrap(),
                    Runner::Php,
                    &models[0].0,
                    &models[0].1
                )
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn native_helper_action_uses_proven_dynamic_connection_database_authority_and_migration_supplement()
     {
        use openidea_plugin::{file_actions::FileOperation, host, providers::Document};
        let (h, _, models) = fixture();
        let source = "<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class Invoice extends Model {}";
        h.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"laravel/framework","version":"v13.40.0"}]}"#,
        );
        h.add_file("app/Models/Invoice.php", source);
        h.add_file("database/migrations/001.php","<?php Schema::create('invoices',function($t){$t->string('title');$t->boolean('migration_only');});");
        let ctx = h.invocation();
        let project = ctx.project().unwrap();
        let mut main = crate::Main::default();
        let mut evidence = payload();
        evidence["models"][0]["dependencies"][0]["hash"] =
            json!(crate::artisan_metadata::hash(source));
        main.runtime_inspection
            .capture_model_connections_with(&ctx, Runner::Php, &models, |_, _| {
                Ok(evidence.to_string())
            })
            .unwrap();
        main.runtime_inspection.capture_options_with(&ctx,Runner::Php,"schema",json!({"connection":"tenant"}),|_,_|Ok(json!({"connection":"tenant","tables":[{"name":"invoices","schema":"","columns":[{"name":"title","type":"integer","nullable":true},{"name":"database_only","type":"text","nullable":false}],"indexes":[]}]}).to_string())).unwrap();
        let doc = Document {
            path: models[0].1.clone(),
            text: source.into(),
            language: "php".into(),
            version: 1,
        };
        for mode in ["both", "database"] {
            let parameters = crate::model_actions::parameters::Parameters {
                source: mode.into(),
                ..Default::default()
            };
            host::storage::set(
                "laravel.helper-parameters:/project",
                &serde_json::to_string(&parameters).unwrap(),
            )
            .unwrap();
            let index = crate::Index::build(&project);
            let action = crate::model_actions::actions(&index, &doc)
                .into_iter()
                .find(|a| a.data.as_deref() == Some("model-native:helpers:1"))
                .unwrap();
            h.answer_next_prompt(Some(&["yes"]));
            let resolved =
                crate::model_actions::resolve_current(&main, &index, &doc, action).unwrap();
            let text = resolved
                .operations
                .iter()
                .find_map(|op| match op {
                    FileOperation::Create { text, .. } => Some(text),
                    _ => None,
                })
                .unwrap();
            assert!(text.contains("@property int|string|null $title"), "{text}");
            assert!(text.contains("@property string $database_only"), "{text}");
            assert_eq!(text.contains("$migration_only"), mode == "both");
            assert!(!text.contains("@property string $title"));
        }
        // SDK mock offers queued answers, not a prompt mutation callback. A stale
        // capture before resolve is still rejected instead of falling back.
        h.add_file(
            "app/Models/Invoice.php",
            &format!("{source} // source changed"),
        );
        let index = crate::Index::build(&project);
        let mut doc = doc;
        doc.text = project.read_file(&doc.path).unwrap();
        let action = crate::model_actions::actions(&index, &doc)
            .into_iter()
            .find(|a| a.data.as_deref() == Some("model-native:helpers:1"))
            .unwrap();
        assert!(crate::model_actions::resolve_current(&main, &index, &doc, action).is_err());
        assert!(h.process_runs().is_empty());
    }

    #[test]
    fn runtime_dependency_fingerprint_must_match_the_owned_read_after_execution() {
        let (h, mut state, models) = fixture();
        let mut value = payload();
        value["models"][0]["dependencies"][0]["hash"] = json!("0000000000000000");
        assert!(
            state
                .capture_model_connections_with(&h.invocation(), Runner::Php, &models, |_, _| Ok(
                    value.to_string()
                ))
                .is_err()
        );
    }

    #[test]
    fn actual_saved_event_named_database_helpers_retain_schema_but_unknown_changed_bindings_never_refresh_php()
     {
        use openidea_plugin::{Event, Plugin, host};
        let (h, _, models) = fixture();
        let h = h.with_capabilities(&["project.read", "project.write", "editor.read", "storage"]);
        h.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        h.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"laravel/framework","version":"v13.40.0"}]}"#,
        );
        let before = "<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class Invoice extends Model { protected $connection='tenant'; protected $table='invoices'; }";
        h.add_file("app/Models/Invoice.php", before);
        let ctx = h.invocation();
        let project = ctx.project().unwrap();
        let mut main = crate::Main::default();
        let mut binding = payload();
        binding["models"][0]["dependencies"][0]["hash"] =
            json!(crate::artisan_metadata::hash(before));
        main.runtime_inspection
            .capture_model_connections_with(&ctx, Runner::Php, &models, |_, _| {
                Ok(binding.to_string())
            })
            .unwrap();
        main.runtime_inspection.capture_options_with(&ctx,Runner::Php,"schema",json!({"connection":"tenant"}),|_,_|Ok(json!({"connection":"tenant","tables":[{"name":"invoices","schema":"","columns":[{"name":"enabled","type":"integer","nullable":true}],"indexes":[]}]}).to_string())).unwrap();
        let params = crate::model_actions::parameters::Parameters {
            source: "database".into(),
            on_save: true,
            ..Default::default()
        };
        host::storage::set(
            "laravel.helper-parameters:/project",
            &serde_json::to_string(&params).unwrap(),
        )
        .unwrap();
        let changed = before.replace(" }", " protected $casts=['enabled'=>'bool']; }");
        h.add_file("app/Models/Invoice.php", &changed);
        main.handle_events(&[Event::FileSaved(models[0].1.clone())], &ctx);crate::tests::finish_indexing(&mut main);
        let path = ".openidea/helpers/App_Models_Invoice.php";
        let generated = project.read_file(path).unwrap();
        assert!(
            generated.contains("@property bool|null $enabled"),
            "{generated}"
        );
        assert_eq!(
            main.runtime_inspection
                .schema_for_generation(&project, Runner::Php)
                .unwrap()["connection"],
            "tenant"
        );
        assert_eq!(project.read_file(&models[0].1).unwrap(), changed);
        main.handle_events(&[Event::FileSaved(format!("/project/{path}"))], &ctx);crate::tests::finish_indexing(&mut main);
        assert_eq!(
            main.runtime_inspection
                .schema_for_generation(&project, Runner::Php)
                .unwrap()["connection"],
            "tenant"
        );
        let unknown=changed.replace(" protected $casts", " public function getConnectionName(){ return resolve_tenant_name(); } protected $casts").replace("'enabled'=>'bool'","'enabled'=>'string'");
        h.add_file("app/Models/Invoice.php", &unknown);
        main.handle_events(&[Event::FileSaved(models[0].1.clone())], &ctx);crate::tests::finish_indexing(&mut main);
        assert_eq!(project.read_file(path).unwrap(), generated);
        assert!(
            main.runtime_inspection
                .helper_schema(&project, Runner::Php, &models[0].0, &models[0].1)
                .is_err()
        );
        assert!(h.process_runs().is_empty());
    }

    #[test]
    fn actual_saved_event_default_database_helpers_require_current_default_policy_without_bootstrapping()
     {
        use openidea_plugin::{Event, Plugin, host};
        let (h, _, models) = fixture();
        let h = h.with_capabilities(&["project.read", "project.write", "editor.read", "storage"]);
        h.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        h.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"laravel/framework","version":"v13.40.0"}]}"#,
        );
        let before = "<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class Invoice extends Model { protected $table='invoices'; }";
        h.add_file("app/Models/Invoice.php", before);
        let ctx = h.invocation();
        let project = ctx.project().unwrap();
        let mut main = crate::Main::default();
        let mut binding = payload();
        binding["models"][0]["connection"] = json!("sqlite");
        binding["models"][0]["dependencies"][0]["hash"] =
            json!(crate::artisan_metadata::hash(before));
        main.runtime_inspection
            .capture_model_connections_with(&ctx, Runner::Php, &models, |_, _| {
                Ok(binding.to_string())
            })
            .unwrap();
        main.runtime_inspection.capture_options_with(&ctx,Runner::Php,"schema",json!({"connection":"sqlite"}),|_,_|Ok(json!({"connection":"sqlite","tables":[{"name":"invoices","schema":"","columns":[{"name":"enabled","type":"integer","nullable":true}],"indexes":[]}]}).to_string())).unwrap();
        let params = crate::model_actions::parameters::Parameters {
            source: "database".into(),
            on_save: true,
            ..Default::default()
        };
        host::storage::set(
            "laravel.helper-parameters:/project",
            &serde_json::to_string(&params).unwrap(),
        )
        .unwrap();
        let changed = before.replace(" }", " protected $casts=['enabled'=>'bool']; }");
        h.add_file("app/Models/Invoice.php", &changed);
        main.handle_events(&[Event::FileSaved(models[0].1.clone())], &ctx);crate::tests::finish_indexing(&mut main);
        let path = ".openidea/helpers/App_Models_Invoice.php";
        let generated = project.read_file(path).unwrap();
        assert!(
            generated.contains("@property bool|null $enabled"),
            "{generated}"
        );
        assert_eq!(
            main.runtime_inspection
                .schema_for_generation(&project, Runner::Php)
                .unwrap()["connection"],
            "sqlite"
        );
        assert_eq!(project.read_file(&models[0].1).unwrap(), changed);
        main.handle_events(&[Event::FileSaved(format!("/project/{path}"))], &ctx);crate::tests::finish_indexing(&mut main);
        assert_eq!(
            main.runtime_inspection
                .schema_for_generation(&project, Runner::Php)
                .unwrap()["connection"],
            "sqlite"
        );
        let unknown=changed.replace(" protected $casts", " public function getConnectionName(){ return resolve_tenant_name(); } protected $casts").replace("'enabled'=>'bool'","'enabled'=>'string'");
        h.add_file("app/Models/Invoice.php", &unknown);
        main.handle_events(&[Event::FileSaved(models[0].1.clone())], &ctx);crate::tests::finish_indexing(&mut main);
        assert_eq!(project.read_file(path).unwrap(), generated);
        assert!(
            main.runtime_inspection
                .helper_schema(&project, Runner::Php, &models[0].0, &models[0].1)
                .is_err()
        );
        assert!(h.process_runs().is_empty());
    }
}
