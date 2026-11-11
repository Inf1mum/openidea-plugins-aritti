//! Isolated released-host acceptance for the Laravel plugin: real Wasm, real host
//! dispatch, private config/data/plugin stores and a disposable copy of a Laravel
//! application. It never opens a desktop window and never touches the user's installation.
use ide_core::{FileEdit, Pos, Range, TextEdit, WorkspaceEdit};
use ide_host::{
    Host, HostConfig,
    plugins::{Consent, HostWorkspace, PluginEvent, PluginHost},
};
use ide_plugin::{FileOperation, Invocation, Limits, ProviderDocument, ViewEvent};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
    time::Instant,
};

const ID: &str = "aritti.laravel";
const SKIP_COPY: &[&str] = &[".git", "node_modules", "storage", "target", "dist", "__pycache__"];

fn copy_tree(source: &Path, target: &Path, skip: &[&str]) -> std::io::Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        if skip.iter().any(|skipped| name == *skipped) {
            continue;
        }
        let kind = entry.file_type()?;
        let destination = target.join(&name);
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            copy_tree(&entry.path(), &destination, skip)?;
        } else {
            std::fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}

fn cursor(marked: &str) -> (String, Pos) {
    let offset = marked.find('^').unwrap_or(0);
    let text = marked.replacen('^', "", 1);
    let before = &text[..offset];
    let line = before.matches('\n').count() as u32;
    let col = before.rsplit('\n').next().unwrap().encode_utf16().count() as u32;
    (text, Pos { line, col })
}

struct Report {
    checks: Vec<Value>,
    failed: bool,
}

impl Report {
    fn check(&mut self, name: &str, passed: bool, detail: impl Into<String>) {
        let detail = detail.into();
        println!("{} {name}: {detail}", if passed { "PASS" } else { "FAIL" });
        self.failed |= !passed;
        self.checks.push(json!({"check": name, "passed": passed, "detail": detail}));
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let repo = PathBuf::from(args.next().ok_or("usage: laravel-smoke REPO OUTPUT FIXTURE_APP")?).canonicalize()?;
    let output = std::path::absolute(PathBuf::from(args.next().ok_or("missing output directory")?))?;
    let fixture = PathBuf::from(args.next().ok_or("missing fixture application")?).canonicalize()?;
    if output.starts_with(&repo) || !fixture.join("artisan").is_file() {
        return Err("output overlaps the repository or fixture is not a Laravel application".into());
    }
    std::fs::create_dir_all(&output)?;
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let project = base.join("app-under-test");
    copy_tree(&fixture, &project, SKIP_COPY)?;
    let mut report = Report { checks: vec![], failed: false };

    let applied = Arc::new(Mutex::new(Vec::<u32>::new()));
    let sink_applied = applied.clone();
    let host = Arc::new(Host::new(HostConfig {
        config_dir: base.join("config"),
        data_dir: base.join("data"),
        local_data_dir: base.join("local"),
        languages: Arc::new(ide_language::LanguageRegistry::load(None)?),
        syntax: Arc::default(),
        sink_for: Box::new(move |_| {
            let events = sink_applied.clone();
            Arc::new(move |event: ide_core::IdeEvent| {
                if let ide_core::IdeEvent::WorkspaceEditApplied { id, .. } = event {
                    events.lock().unwrap().push(id);
                }
            })
        }),
        owner: |w| w,
        runtime: tokio::runtime::Handle::current(),
        project_environment: false,
    }));
    host.trust.decide(&project, true, false)?;
    host.open_project("laravel-smoke", &project)?;
    let controller = Arc::new(Mutex::new(Weak::<PluginHost>::new()));
    let prompt_controller = controller.clone();
    let plugins = Arc::new(PluginHost::new(
        base.join("plugins"),
        Arc::new(move |_: Option<&str>, event| {
            if let PluginEvent::PromptRequested { prompt_id, .. } = event {
                if let Some(plugins) = prompt_controller.lock().unwrap().upgrade() {
                    plugins.answer_prompt(&prompt_id, Some(vec!["ok".into()]));
                }
            }
        }),
        Limits::default(),
    )?);
    *controller.lock().unwrap() = Arc::downgrade(&plugins);
    plugins.set_workspace(Some(Arc::new(HostWorkspace(Arc::downgrade(&host)))));
    host.set_plugin_host(&plugins);
    plugins.load_all();
    plugins.startup_finished();

    let source = repo.join("plugins/laravel");
    let wasm = source.join("dist/plugin.wasm");
    if !wasm.is_file() {
        return Err("build the plugin first: openidea-plugin build plugins/laravel".into());
    }
    let staged = base.join("packages/laravel");
    copy_tree(&source, &staged, SKIP_COPY)?;
    std::fs::create_dir_all(staged.join("dist"))?;
    std::fs::copy(&wasm, staged.join("dist/plugin.wasm"))?;
    let optional = ["project.write", "storage", "editor.read"].map(String::from).to_vec();
    let consent = Consent { optional, ..Default::default() };
    let installed = plugins.install_from_dir(&staged, &consent);
    report.check("install", installed.is_ok(), format!("{:?}", installed.as_ref().err()));
    if installed.is_err() {
        std::fs::write(output.join("laravel-results.json"), serde_json::to_vec_pretty(&report.checks)?)?;
        std::process::exit(1);
    }

    let invocation = Invocation {
        client: Some("laravel-smoke".into()),
        workspace_root: Some(project.clone()),
        ..Default::default()
    };
    let first = Instant::now();
    let rendered = plugins.render_view(ID, "laravel", None, invocation.clone());
    report.check(
        "activation and first render within the per-call fuel budget",
        rendered.is_ok(),
        format!("{:?} in {} ms", rendered.as_ref().err(), first.elapsed().as_millis()),
    );

    let tick = json!({"op": "index-tick"}).to_string();
    let (mut max_ms, mut ticks, mut ready) = (0u128, 0usize, false);
    let mut failure = None;
    for step in 0..30_000usize {
        let started = Instant::now();
        let result = plugins.view_event(ID, "laravel", ViewEvent::Message { text: tick.clone() }, invocation.clone());
        max_ms = max_ms.max(started.elapsed().as_millis());
        ticks = step + 1;
        if let Err(error) = result {
            failure = Some(error.to_string());
            break;
        }
        if step % 20 == 0 {
            if let Ok(model) = plugins.render_view(ID, "laravel", None, invocation.clone()) {
                if !serde_json::to_string(&model)?.contains("indexing tasks") {
                    ready = true;
                    break;
                }
            }
        }
    }
    report.check(
        "index builds to completion on the real host",
        ready && failure.is_none(),
        format!("ready={ready} ticks={ticks} max_callback_ms={max_ms} error={failure:?}"),
    );

    let model = plugins.render_view(ID, "laravel", None, invocation.clone())?;
    let model_json = serde_json::to_string(&model)?;
    std::fs::write(output.join("laravel-view.json"), &model_json)?;
    let documents = &host.project("laravel-smoke")?.documents;
    let probe = |relative: &str, marked: &str| -> Result<(ProviderDocument, Pos), Box<dyn std::error::Error>> {
        let (text, at) = cursor(marked);
        let path = project.join(relative);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, &text)?;
        let info = documents.open(&path)?;
        Ok((ProviderDocument { path: info.path.to_path_buf(), language: "php".into(), version: info.version as u64, text: info.text.clone() }, at))
    };

    let view_name = {
        let mut names = std::fs::read_dir(project.join("resources/views"))?
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().to_string_lossy().strip_suffix(".blade.php").map(str::to_owned))
            .collect::<Vec<_>>();
        names.sort();
        names.into_iter().next().ok_or("fixture has no top-level Blade view")?
    };
    let (doc, at) = probe("app/Probe/ViewCompletion.php", "<?php\nreturn view('^');\n")?;
    let labels = plugins.provide_completion(&doc, at, None).into_iter().map(|(_, item)| item.label).collect::<Vec<_>>();
    report.check("completion lists project views", labels.iter().any(|label| *label == view_name), format!("{view_name}: {} items", labels.len()));

    let (doc, at) = probe("app/Probe/ConfigCompletion.php", "<?php\nreturn config('app.^');\n")?;
    let labels = plugins.provide_completion(&doc, at, None).into_iter().map(|(_, item)| item.label).collect::<Vec<_>>();
    report.check("completion lists config keys", labels.iter().any(|label| label.ends_with("name")), format!("{} items", labels.len()));

    let (doc, at) = probe("app/Probe/ViewDefinition.php", &format!("<?php\nreturn view('{view_name}^');\n"))?;
    let locations = plugins.provide_definition(&doc, at);
    let reaches_view = locations.iter().any(|location| location.path.display().to_string().ends_with(&format!("resources/views/{view_name}.blade.php")));
    report.check("go to definition reaches the Blade view", reaches_view, format!("{view_name}: {} locations", locations.len()));

    let (doc, _) = probe("app/Probe/MissingView.php", "<?php\nreturn view('probe.missing-screen');\n")?;
    let diagnostics = plugins.provide_diagnostics(&doc, &ide_core::CancellationToken::new());
    let count = diagnostics.iter().map(|(_, found)| found.len()).sum::<usize>();
    report.check("diagnostics flag a missing view", count > 0, format!("{count} diagnostics"));

    let at = Pos { line: 1, col: 14 };
    let actions = plugins.provide_code_actions(&doc, Range { start: at, end: at });
    let titles = actions.iter().map(|(_, action)| action.title.clone()).collect::<Vec<_>>();
    let create = actions.into_iter().find(|(owner, action)| owner == ID && action.title.to_lowercase().contains("create"));
    report.check("quick fix offers view creation", create.is_some(), format!("{titles:?}"));
    if let Some((_, action)) = create {
        let resolved = plugins.resolve_code_action(ID, &doc, action, invocation.clone());
        match resolved {
            Ok(resolved) => {
                let mut files = Vec::new();
                let mut created = Vec::new();
                for operation in &resolved.operations {
                    if let FileOperation::Create { path, text, .. } = operation {
                        files.push(FileEdit::Create { path: path.clone().into(), overwrite: false });
                        files.push(FileEdit::Edit {
                            path: path.clone().into(),
                            edits: vec![TextEdit { range: Range { start: Pos { line: 0, col: 0 }, end: Pos { line: 0, col: 0 } }, new_text: text.clone() }],
                            version: None,
                        });
                        created.push(path.clone());
                    }
                }
                report.check("quick fix resolves to a no-overwrite file creation", !created.is_empty(), format!("{created:?}"));
                if !created.is_empty() {
                    let mut edit = WorkspaceEdit { files, ..Default::default() };
                    let before = applied.lock().unwrap().len();
                    let result = documents.prepare_workspace_edit(&mut edit).and_then(|_| documents.apply_workspace_edit(&edit));
                    let exists = created.iter().all(|path| path.is_file());
                    report.check("native workspace edit creates the view", result.is_ok() && exists, format!("{:?}", result.err()));
                    let id = applied.lock().unwrap().get(before).copied();
                    if let Some(id) = id {
                        let undone = documents.undo_workspace_edit(id);
                        let gone = created.iter().all(|path| !path.exists());
                        report.check("native grouped Undo removes the created view", undone.is_ok() && gone, format!("{:?}", undone.err()));
                    } else {
                        report.check("native grouped Undo removes the created view", false, "no WorkspaceEditApplied event");
                    }
                }
            }
            Err(error) => report.check("quick fix resolves to a no-overwrite file creation", false, error.to_string()),
        }
    }


    if std::env::var("LARAVEL_SMOKE_STALL").is_ok() {
        let root = project.display().to_string();
        let frame = "0123456789abcdef0123456789abcdef";
        let target_owned = std::env::var("LARAVEL_SMOKE_TARGET").unwrap_or_else(|_| "laravel".into());
        let target = target_owned.as_str();
        let envelope = |sequence: u32, body: Value| {
            let mut value = json!({"channel": "ui-kit", "sequence": sequence, "frameId": frame, "project": root, "targetView": target});
            value.as_object_mut().unwrap().extend(body.as_object().unwrap().clone());
            value.to_string()
        };
        let rendered = plugins.view_event(ID, "laravel", ViewEvent::Message { text: envelope(1, json!({"op": "render"})) }, invocation.clone());
        report.check("hub render envelope is accepted", rendered.is_ok(), format!("{:?}", rendered.err()));
        let mut paths = vec![];
        for name in ["MissingView", "ViewCompletion", "ConfigCompletion"] {
            paths.push(project.join(format!("app/Probe/{name}.php")).display().to_string());
        }
        plugins.post_events(Some("laravel-smoke"), paths.iter().cloned().map(ide_plugin::WorkbenchEvent::FileOpened).collect());
        plugins.flush_events();
        let user = project.join("app/Models/User.php");
        let mut text = std::fs::read_to_string(&user)?;
        text.push_str("\n// external change\n");
        std::fs::write(&user, text)?;
        plugins.post_events(
            Some("laravel-smoke"),
            vec![ide_plugin::WorkbenchEvent::FilesChanged(ide_plugin::FilesChanged {
                created: vec![],
                modified: vec![user.display().to_string()],
                removed: vec![],
            })],
        );
        plugins.flush_events();
        let status = || -> String {
            plugins
                .render_view(ID, "laravel", None, invocation.clone())
                .ok()
                .and_then(|model| serde_json::to_value(&model).ok())
                .and_then(|value| value["status"].as_str().map(str::to_owned))
                .unwrap_or_default()
        };
        let mut trace = vec![status()];
        let mut done = false;
        for step in 0..4000u32 {
            let message = json!({"op": "index-tick", "_backgroundPoll": format!("{frame}:{step}")}).to_string();
            let result = plugins.view_event(ID, "laravel", ViewEvent::Message { text: envelope(0, json!({"op": "opaque-message", "message": message})) }, invocation.clone());
            if let Err(error) = result {
                trace.push(format!("tick {step}: {error}"));
                break;
            }
            if step % 25 == 0 {
                let current = status();
                if trace.last() != Some(&current) {
                    trace.push(format!("tick {step}: {current}"));
                }
                if !current.contains("indexing tasks") && step > 0 {
                    done = true;
                    break;
                }
            }
        }
        report.check("index recovers after an external change and recheck queue", done, format!("{trace:?}"));
    }
    std::fs::write(
        output.join("laravel-results.json"),
        serde_json::to_vec_pretty(&json!({
            "scope": "released API 1.4 host (rev 37c2a60), real freshly built Wasm, disposable copy of a Laravel application; no GUI, no process or network grants",
            "fixture": fixture,
            "checks": report.checks,
        }))?,
    )?;
    if report.failed {
        std::process::exit(1);
    }
    Ok(())
}
