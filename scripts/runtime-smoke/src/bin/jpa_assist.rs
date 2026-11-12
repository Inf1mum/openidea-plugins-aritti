//! Released-host headless acceptance for JPA Assist: real freshly built Wasm, real host dispatch, private
//! config/data/plugin stores and a DISPOSABLE COPY of a parity fixture. No desktop window, no network.
//! It scripts every `host::pick/input/confirm` prompt through the host's `PluginEvent::PromptRequested` /
//! `PluginHost::answer_prompt` pair, applies the resolved FileAction through the native workspace edit engine and
//! undoes it with native grouped Undo. This is L1 evidence only (see parity/CLOSURE_PLAN.md); it is not a parity claim.
//!
//! usage: jpa_assist REPO OUTPUT [FIXTURE_DIR] [WASM]
use ide_core::{FileEdit, Pos, Range, TextEdit, WorkspaceEdit};
use ide_host::{
    Host, HostConfig,
    plugins::{Consent, HostWorkspace, PluginEvent, PluginHost},
};
use ide_plugin::{Invocation, Limits, Prompt, ProviderDocument, ViewEvent, WorkbenchEvent};
#[path = "jpa_assist/scenario.rs"]
mod scenario;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
};

const ID: &str = "aritti.jpa-assist";
const CLIENT: &str = "jpa-smoke";
const SKIP_COPY: &[&str] = &[".git", "node_modules", "target", "__pycache__"];
const STAGE_SKIP: &[&str] = &[".git", "node_modules", "target", "__pycache__", "parity", "src", "tests", "dist"];

type R<T> = Result<T, Box<dyn std::error::Error>>;

fn copy_tree(source: &Path, target: &Path, skip: &[&str]) -> std::io::Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        if skip.iter().any(|s| name == *s) || entry.file_type()?.is_symlink() {
            continue;
        }
        let to = target.join(&name);
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &to, skip)?;
        } else {
            std::fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn files_under(root: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(root).into_iter().flatten().flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn tree_hash(root: &Path) -> String {
    let mut files = vec![];
    files_under(root, &mut files);
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.strip_prefix(root).unwrap().to_string_lossy().as_bytes());
        hasher.update(std::fs::read(&file).unwrap_or_default());
    }
    format!("{:x}", hasher.finalize())
}

#[derive(Clone, Debug)]
pub(crate) enum Answer {
    Pick(String),
    PickLabel(String),
    PickMany(Vec<String>),
    PickFirstEndingWith(String),
    Input(String),
    Confirm(bool),
    Default,
    Cancel,
}

#[derive(Default)]
struct Script {
    answers: VecDeque<Answer>,
    seen: Vec<Value>,
    problems: Vec<String>,
    hook: Option<(usize, Box<dyn FnOnce() + Send>)>,
}

struct Report {
    checks: Vec<Value>,
    failed: bool,
}

impl Report {
    fn check(&mut self, name: &str, passed: bool, detail: impl Into<String>) {
        let detail = detail.into();
        println!("{} {name}: {}", if passed { "PASS" } else { "FAIL" }, detail.chars().take(300).collect::<String>());
        self.failed |= !passed;
        self.checks.push(json!({"check": name, "passed": passed, "detail": detail}));
    }
}

fn answer_for(script: &mut Script, prompt: &Prompt) -> Option<Vec<String>> {
    let index = script.seen.len();
    script.seen.push(serde_json::to_value(prompt).unwrap_or(Value::Null));
    let hook = match script.hook.take() {
        Some((at, hook)) if at == index => Some(hook),
        other => {
            script.hook = other;
            None
        }
    };
    if let Some(hook) = hook {
        hook();
    }
    let Some(next) = script.answers.pop_front() else {
        script.problems.push(format!("prompt {index} had no scripted answer: {prompt:?}"));
        return None;
    };
    match (next.clone(), prompt) {
        (Answer::Cancel, _) => None,
        (Answer::Pick(key), Prompt::QuickPick { items, .. }) if items.iter().any(|i| i.key == key) => Some(vec![key]),
        (Answer::PickLabel(text), Prompt::QuickPick { items, .. }) if items.iter().any(|i| i.label.contains(&text)) => {
            items.iter().find(|i| i.label.contains(&text)).map(|i| vec![i.key.clone()])
        }
        (Answer::PickMany(wanted), Prompt::QuickPick { items, multiple: true, .. }) => {
            let resolved: Vec<String> = wanted.iter().filter_map(|w| items.iter().find(|i| &i.key == w || &i.label == w).map(|i| i.key.clone())).collect();
            (resolved.len() == wanted.len()).then_some(resolved)
        }
        (Answer::PickFirstEndingWith(suffix), Prompt::QuickPick { items, .. }) => {
            items.iter().find(|i| i.label.ends_with(&suffix)).map(|i| vec![i.key.clone()])
        }
        (Answer::Input(text), Prompt::Input { .. }) => Some(vec![text]),
        (Answer::Default, Prompt::Input { value: Some(value), .. }) => Some(vec![value.clone()]),
        (Answer::Confirm(true), Prompt::Confirm { .. }) => Some(vec!["ok".into()]),
        (Answer::Confirm(false), Prompt::Confirm { .. }) => None,
        _ => {
            script.problems.push(format!("prompt {index} {prompt:?} does not accept scripted {next:?}"));
            None
        }
    }
}

fn provider_doc(host: &Host, path: &Path, language: &str) -> R<ProviderDocument> {
    let info = host.project(CLIENT)?.documents.open(path)?;
    Ok(ProviderDocument { path: info.path.to_path_buf(), language: language.into(), version: info.version as u64, text: info.text.clone() })
}

fn language_of(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("kt") => "kotlin",
        Some("xml") => "xml",
        _ => "java",
    }
}

fn golden_rows(text: &str, scenario: &str) -> BTreeSet<(String, String, u32)> {
    let mut rows = BTreeSet::new();
    let mut inside = false;
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("## Scenario: ") {
            inside = name.trim() == scenario;
            continue;
        }
        if !inside || !line.starts_with("| `") {
            continue;
        }
        let cells: Vec<_> = line.split('|').map(|c| c.trim().trim_matches('`').to_owned()).collect();
        if let (Some(rule), Some(file), Some(Ok(number))) = (cells.get(1), cells.get(2), cells.get(3).map(|c| c.parse::<u32>())) {
            rows.insert((rule.clone(), file.clone(), number));
        }
    }
    rows
}

fn class_line(text: &str) -> u32 {
    text.lines().position(|l| l.contains("class ") && !l.trim_start().starts_with(['/', '*', 'i'])).unwrap_or(0) as u32
}

/// Independent oracle: inserts `@{constraint}` above the field and the validation import after the last import.
fn expected_validation(before: &str, attribute: &str, constraint: &str) -> Option<String> {
    let import = format!("import jakarta.validation.constraints.{constraint};\n");
    let lines: Vec<&str> = before.split_inclusive('\n').collect();
    let field = lines.iter().position(|l| l.trim_end().ends_with(&format!(" {attribute};")) && l.trim_start().starts_with("private"))?;
    let last_import = lines.iter().rposition(|l| l.starts_with("import "))?;
    let indent: String = lines[field].chars().take_while(|c| *c == ' ').collect();
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i == field {
            out.push_str(&format!("{indent}@{constraint}\n"));
        }
        out.push_str(line);
        if i == last_import {
            out.push_str(&import);
        }
    }
    Some(out)
}

const HOST_REV: &str = "e702e4da7b0333e81bc818e26be72db1d157ec5d";
const PLUGIN_SOURCE: &str = "plugins/jpa-assist";

/// A private released host with the freshly built wasm installed; everything a scenario or the smoke set needs.
pub(crate) struct Rt {
    pub host: Arc<Host>,
    pub plugins: Arc<PluginHost>,
    pub script: Arc<Mutex<Script>>,
    pub clipboard: Arc<Mutex<Vec<String>>>,
    pub applied: Arc<Mutex<Vec<u32>>>,
    pub install_error: Option<String>,
}

fn boot(base: &Path, project: &Path, repo: &Path, wasm: &Path, settings_toml: Option<&str>, grants: &[String]) -> R<Rt> {
    std::fs::create_dir_all(base.join("config"))?;
    if let Some(text) = settings_toml {
        std::fs::write(base.join("config/settings.toml"), text)?;
    }
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
    host.trust.decide(project, true, false)?;
    host.open_project(CLIENT, project)?;

    let script = Arc::new(Mutex::new(Script::default()));
    let controller = Arc::new(Mutex::new(Weak::<PluginHost>::new()));
    let clipboard = Arc::new(Mutex::new(Vec::<String>::new()));
    let (sink_script, sink_controller, sink_clipboard) = (script.clone(), controller.clone(), clipboard.clone());
    let plugins = Arc::new(PluginHost::new(
        base.join("plugins"),
        Arc::new(move |_: Option<&str>, event| {
            if let PluginEvent::CopyToClipboard { text, .. } = &event {
                sink_clipboard.lock().unwrap().push(text.clone());
            }
            if let PluginEvent::PromptRequested { prompt_id, prompt, .. } = event {
                let answer = answer_for(&mut sink_script.lock().unwrap(), &prompt);
                if let Some(plugins) = sink_controller.lock().unwrap().upgrade() {
                    plugins.answer_prompt(&prompt_id, answer);
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

    let staged = base.join("packages/jpa-assist");
    copy_tree(&repo.join(PLUGIN_SOURCE), &staged, STAGE_SKIP)?;
    std::fs::create_dir_all(staged.join("dist"))?;
    std::fs::copy(wasm, staged.join("dist/plugin.wasm"))?;
    let consent = Consent { optional: ["clipboard", "editor.write"].map(String::from).into_iter().chain(grants.iter().cloned()).collect(), ..Default::default() };
    let installed = plugins.install_from_dir(&staged, &consent);
    Ok(Rt { host, plugins, script, clipboard, applied, install_error: installed.err().map(|e| e.to_string()) })
}

#[derive(Default)]
struct Cli {
    positional: Vec<String>,
    scenarios: Vec<String>,
    only: Vec<String>,
    receipt_dir: Option<PathBuf>,
    list: bool,
    validate: bool,
    smoke: bool,
}

fn parse_cli() -> R<Cli> {
    let mut cli = Cli::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scenario" => cli.scenarios.push(args.next().ok_or("--scenario needs a file or directory")?),
            "--only" => cli.only.extend(args.next().ok_or("--only needs ids")?.split(',').map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())),
            "--json-receipt-dir" => cli.receipt_dir = Some(PathBuf::from(args.next().ok_or("--json-receipt-dir needs a directory")?)),
            "--list" => cli.list = true,
            "--validate" => cli.validate = true,
            "--smoke" => cli.smoke = true,
            other if other.starts_with("--") => return Err(format!("unknown option {other}").into()),
            other => cli.positional.push(other.to_owned()),
        }
    }
    Ok(cli)
}

const USAGE: &str = "usage: jpa_assist REPO OUT [FIXTURE_DIR] [WASM] [--smoke] [--scenario FILE|DIR ...] [--only RS-001,...] [--json-receipt-dir DIR]\n       jpa_assist REPO --list|--validate [--scenario FILE|DIR ...] [--only ...]\nno --scenario and no --list/--validate runs the built-in 31-check smoke set only";

#[tokio::main]
async fn main() -> R<()> {
    let cli = parse_cli()?;
    let repo = PathBuf::from(cli.positional.first().ok_or(USAGE)?).canonicalize()?;
    let mut scenarios_requested = cli.scenarios.clone();
    if scenarios_requested.is_empty() && (cli.list || cli.validate || !cli.only.is_empty()) {
        scenarios_requested.push(repo.join("plugins/jpa-assist/parity/runner/scenarios").display().to_string());
    }
    if cli.list || cli.validate {
        let scenarios = scenario::load(&scenarios_requested, &cli.only)?;
        if cli.list {
            scenario::list(&scenarios);
        }
        if cli.validate {
            let errors = scenario::validate(&repo, &scenarios);
            for error in &errors {
                println!("INVALID {error}");
            }
            println!("validated {} scenarios: {} errors", scenarios.len(), errors.len());
            if !errors.is_empty() {
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    let output = std::path::absolute(PathBuf::from(cli.positional.get(1).ok_or(USAGE)?))?;
    let wasm = match cli.positional.get(3) {
        Some(path) => PathBuf::from(path),
        None => repo.join("plugins/jpa-assist/dist/plugin.wasm"),
    };
    if output.starts_with(&repo) || !wasm.is_file() {
        return Err("output overlaps the repository or the wasm is missing (build the plugin first)".into());
    }
    std::fs::create_dir_all(&output)?;
    let mut failed = false;
    if !scenarios_requested.is_empty() {
        let scenarios = scenario::load(&scenarios_requested, &cli.only)?;
        let errors = scenario::validate(&repo, &scenarios);
        if !errors.is_empty() {
            for error in &errors {
                println!("INVALID {error}");
            }
            return Err("scenario validation failed; nothing was run".into());
        }
        let receipts = cli.receipt_dir.clone().unwrap_or_else(|| output.join("receipts"));
        failed |= scenario::run_all(&repo, &output, &receipts, &wasm, &scenarios).await?;
    }
    if scenarios_requested.is_empty() || cli.smoke {
        let fixture = match cli.positional.get(2) {
            Some(path) => PathBuf::from(path).canonicalize()?,
            None => repo.join("plugins/jpa-assist/parity/fixtures/boot-jpa-java-jakarta"),
        };
        let smoke_out = if scenarios_requested.is_empty() { output.clone() } else { output.join("smoke") };
        std::fs::create_dir_all(&smoke_out)?;
        failed |= smoke(&repo, &smoke_out, &fixture, &wasm).await?;
    }
    if failed {
        std::process::exit(1);
    }
    Ok(())
}

/// The original 31-check released-host acceptance (kept unchanged in behaviour). Returns true when a check failed.
async fn smoke(repo: &Path, output: &Path, fixture: &Path, wasm: &Path) -> R<bool> {
    let output = output.to_path_buf();
    if !fixture.join("pom.xml").is_file() {
        return Err("fixture has no pom.xml".into());
    }
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let project = base.join("project");
    copy_tree(fixture, &project, SKIP_COPY)?;
    let pristine_hash = tree_hash(&project);
    let mut report = Report { checks: vec![], failed: false };

    let rt = boot(&base, &project, repo, wasm, None, &[])?;
    let Rt { host, plugins, script, clipboard, applied, .. } = rt;
    let wasm_sha = sha(&std::fs::read(wasm)?);
    report.check("install into the private released host", rt.install_error.is_none(), format!("wasm sha256 {wasm_sha}; {:?}", rt.install_error));
    let write_results = |report: &Report| -> R<()> {
        std::fs::write(
            output.join("jpa-assist-results.json"),
            serde_json::to_vec_pretty(&json!({
                "scope": "released OpenIDEA host (see runtime-smoke Cargo.toml rev), freshly built jpa-assist Wasm, disposable copy of a parity fixture; no GUI, no network, no process grants. L1 evidence only, not a parity claim.",
                "fixture": fixture, "wasmSha256": wasm_sha, "pristineTreeSha256": pristine_hash, "checks": report.checks,
            }))?,
        )?;
        Ok(())
    };
    if rt.install_error.is_some() {
        write_results(&report)?;
        return Ok(true);
    }

    let documents = host.project(CLIENT)?.documents.clone();
    let invocation = Invocation { client: Some(CLIENT.into()), workspace_root: Some(project.clone()), ..Default::default() };

    let mut all: Vec<PathBuf> = vec![];
    files_under(&project, &mut all);
    all.retain(|p| p.extension().is_some_and(|e| e == "java" || e == "kt" || e == "xml") && p.file_name().is_some_and(|n| n != "pom.xml"));
    let relative = |p: &Path| p.strip_prefix(&project).unwrap().to_string_lossy().replace('\\', "/");

    // (1) live diagnostics, two passes like the GOLDEN harness
    let mut found = BTreeSet::new();
    let mut detail_rows = vec![];
    for pass in 0..2 {
        found.clear();
        detail_rows.clear();
        for path in &all {
            let doc = provider_doc(&host, path, language_of(path))?;
            if pass == 0 {
                plugins.post_events(Some(CLIENT), vec![WorkbenchEvent::FileOpened(path.display().to_string())]);
                plugins.flush_events();
            }
            for (owner, diagnostics) in plugins.provide_diagnostics(&doc, &ide_core::CancellationToken::new()) {
                if owner != ID {
                    continue;
                }
                for d in diagnostics {
                    let row = (d.code.clone().unwrap_or_default(), relative(path), d.range.start.line + 1);
                    detail_rows.push(json!({"rule": row.0, "file": row.1, "line": row.2, "severity": format!("{:?}", d.severity), "message": d.message}));
                    found.insert(row);
                }
            }
        }
    }
    std::fs::write(output.join("diagnostics-actual.json"), serde_json::to_vec_pretty(&detail_rows)?)?;
    let golden_text = std::fs::read_to_string(fixture.join("GOLDEN.md"))?;
    let golden = golden_rows(&golden_text, "base");
    let missing: Vec<_> = golden.difference(&found).collect();
    let extra: Vec<_> = found.difference(&golden).collect();
    for sample in ["AttributeTypeDefects", "RelationDefects", "EntityCallbackDefects"] {
        let want: BTreeSet<_> = golden.iter().filter(|r| r.1.contains(sample)).collect();
        let got: BTreeSet<_> = found.iter().filter(|r| r.1.contains(sample)).collect();
        report.check(&format!("diagnostics for {sample} equal GOLDEN rule/file/line"), !want.is_empty() && want == got, format!("golden {} rows, live {} rows", want.len(), got.len()));
    }
    report.check(
        "diagnostics for the whole base scenario equal GOLDEN rule/file/line",
        missing.is_empty() && extra.is_empty(),
        format!("golden {} live {} missing {:?} extra {:?}", golden.len(), found.len(), missing, extra),
    );

    // other read-only provider surfaces on opened fixture documents
    let customer = project.join("src/main/java/com/example/shop/domain/Customer.java");
    let customer_doc = provider_doc(&host, &customer, "java")?;
    let markers = plugins.provide_line_markers(&customer_doc);
    report.check("line markers are provided on an entity", !markers.is_empty(), format!("{} markers", markers.len()));
    let repository = project.join("src/main/java/com/example/shop/repository/CustomerRepository.java");
    let repository_doc = provider_doc(&host, &repository, "java")?;
    let repository_markers = plugins.provide_line_markers(&repository_doc);
    report.check("line markers on a repository (informational)", true, format!("{} markers", repository_markers.len()));
    let hover_line = customer_doc.text.lines().position(|l| l.contains("private String name;")).unwrap_or(0) as u32;
    let hovers = plugins.provide_hover(&customer_doc, Pos { line: hover_line, col: 22 });
    report.check("hover is answered on an attribute", true, format!("{} hovers (informational: an empty answer is not a failure)", hovers.len()));
    let probe = project.join("src/main/java/com/example/shop/repository/ProbeRepository.java");
    let probe_text = "package com.example.shop.repository;\n\nimport com.example.shop.domain.Category;\nimport org.springframework.data.jpa.repository.JpaRepository;\n\npublic interface ProbeRepository extends JpaRepository<Category, Long> {\nfindBy\n}\n";
    std::fs::write(&probe, probe_text)?;
    let probe_doc = provider_doc(&host, &probe, "java")?;
    let labels: Vec<String> = plugins.provide_completion(&probe_doc, Pos { line: 6, col: 6 }, None).into_iter().map(|(_, i)| i.label).collect();
    report.check("derived-query completion lists entity properties", labels.iter().any(|l| l.contains("Name")), format!("{} items; first {:?}", labels.len(), labels.iter().take(6).collect::<Vec<_>>()));
    std::fs::remove_file(&probe)?;
    let actions = plugins.provide_code_actions(&customer_doc, Range::default());
    let titles: Vec<String> = actions.iter().filter(|(o, _)| o == ID).map(|(_, a)| a.title.clone()).collect();
    std::fs::write(output.join("code-action-titles-Customer.json"), serde_json::to_vec_pretty(&titles)?)?;
    report.check("code actions are offered on an entity", !titles.is_empty(), format!("{} actions: {titles:?}", titles.len()));

    // (d) native views and the Persistence tree model
    for view in ["structure", "designer", "source-diagram"] {
        let rendered = plugins.render_view(ID, view, None, invocation.clone());
        match rendered {
            Ok(model) => {
                let text = serde_json::to_string_pretty(&model)?;
                std::fs::write(output.join(format!("view-{view}.json")), &text)?;
                report.check(&format!("view {view} renders a model"), true, format!("{} bytes; mentions Customer: {}", text.len(), text.contains("Customer")));
            }
            Err(error) => report.check(&format!("view {view} renders a model"), false, error.to_string()),
        }
    }
    if let Ok(model) = plugins.render_view(ID, "structure", None, invocation.clone()) {
        let value = serde_json::to_value(&model)?;
        let nodes = value["body"]["value"].as_array().cloned().unwrap_or_default();
        let tree_entities: BTreeSet<String> = nodes.iter().filter(|n| n["key"].as_str().is_some_and(|k| k.starts_with("entity:"))).filter_map(|n| n["label"].as_str().map(str::to_owned)).collect();
        let source_entities: BTreeSet<String> = all
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "java") && std::fs::read_to_string(p).is_ok_and(|t| t.lines().any(|l| l.trim() == "@Entity")))
            .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .collect();
        report.check(
            "structure tree model lists only source @Entity classes (an @Entity interface is correctly absent)",
            tree_entities.is_subset(&source_entities),
            format!("{} tree entity nodes, {} source @Entity classes, missing {:?}, tree-only {:?}", tree_entities.len(), source_entities.len(), source_entities.difference(&tree_entities).collect::<Vec<_>>(), tree_entities.difference(&source_entities).collect::<Vec<_>>()),
        );
    }
    let _ = plugins.view_event(ID, "structure", ViewEvent::Button { id: "refresh".into() }, invocation.clone());

    // commands run headlessly too; the clipboard text is captured from PluginEvent::CopyToClipboard
    {
        let category = project.join("src/main/java/com/example/shop/domain/Category.java");
        let editor = ide_plugin::EditorContext { path: category.clone(), language: "java".into(), selections: vec![] };
        let with_editor = Invocation { editor: Some(editor), ..invocation.clone() };
        *script.lock().unwrap() = Script { answers: (0..4).map(|_| Answer::PickFirstEndingWith(String::new())).collect(), ..Script::default() };
        let result = plugins.run_command(ID, "copy-ddl", None, with_editor);
        let prompts = std::mem::take(&mut script.lock().unwrap().seen);
        std::fs::write(output.join("copy-ddl-prompts.json"), serde_json::to_vec_pretty(&prompts)?)?;
        let copied = clipboard.lock().unwrap().clone();
        std::fs::write(output.join("copy-ddl-clipboard.txt"), copied.join("\n----\n"))?;
        report.check(
            "Copy DDL command runs and its clipboard text is captured",
            result.is_ok() && copied.iter().any(|t| t.to_lowercase().contains("create table")),
            format!("{:?}; {} prompts; {} clipboard writes; first {:?}", result.as_ref().err(), prompts.len(), copied.len(), copied.first().map(|t| t.chars().take(120).collect::<String>())),
        );
    }

    // (2) one reviewed FileAction end to end
    let title = "Add validation constraint with preview…";
    let target = ["Category", "Product", "Customer"]
        .iter()
        .map(|name| project.join(format!("src/main/java/com/example/shop/domain/{name}.java")))
        .find(|path| {
            provider_doc(&host, path, "java")
                .map(|doc| plugins.provide_code_actions(&doc, Range::default()).iter().any(|(o, a)| o == ID && a.title == title))
                .unwrap_or(false)
        });
    let Some(target) = target else {
        report.check("validation FileAction is offered on a domain entity", false, "none of Category/Product/Customer offers it".to_owned());
        write_results(&report)?;
        return Ok(true);
    };
    let original_text = std::fs::read_to_string(&target)?;
    let original_bytes_sha = sha(original_text.as_bytes());
    report.check("validation FileAction is offered on a domain entity", true, relative(&target));

    let class_row = class_line(&original_text);
    let range = Range { start: Pos { line: class_row, col: 0 }, end: Pos { line: class_row, col: 0 } };
    let id = documents.open(&target)?.id;

    let run = |answers: Vec<Answer>, hook: Option<(usize, Box<dyn FnOnce() + Send>)>| -> R<(Result<ide_plugin::CodeAction, String>, Vec<Value>, Vec<String>)> {
        let doc = provider_doc(&host, &target, "java")?;
        let action = plugins
            .provide_code_actions(&doc, range)
            .into_iter()
            .find(|(o, a)| o == ID && a.title == title)
            .map(|(_, a)| a)
            .ok_or("action not offered")?;
        {
            let mut s = script.lock().unwrap();
            *s = Script { answers: answers.into(), seen: vec![], problems: vec![], hook };
        }
        let resolved = plugins.resolve_code_action(ID, &doc, action, invocation.clone()).map_err(|e| e.to_string());
        let mut s = script.lock().unwrap();
        Ok((resolved, std::mem::take(&mut s.seen), std::mem::take(&mut s.problems)))
    };
    let state = || -> (String, String, String) {
        (documents.text_of(&target).unwrap_or_default(), std::fs::read_to_string(&target).unwrap_or_default(), tree_hash(&project))
    };

    // discovery run: cancel at the first prompt to read the attribute choices
    let (_, seen, _) = run(vec![Answer::Cancel], None)?;
    std::fs::write(output.join("validation-prompt-discovery.json"), serde_json::to_vec_pretty(&seen)?)?;
    let attribute = seen
        .first()
        .and_then(|p| p["items"].as_array())
        .and_then(|items| items.iter().find(|i| i["label"].as_str().is_some_and(|l| l.ends_with(": String"))))
        .and_then(|i| i["key"].as_str().map(str::to_owned))
        .ok_or("no String attribute offered by the first prompt")?;
    let attribute: &str = &attribute;
    let happy = || vec![Answer::Pick(attribute.into()), Answer::Pick("NotBlank".into()), Answer::Input(String::new()), Answer::Confirm(true)];
    let expected = expected_validation(&original_text, attribute, "NotBlank").ok_or("oracle could not place the field")?;

    // happy path: scripted answers, exact preview, native apply
    let before_events = applied.lock().unwrap().len();
    let (resolved, seen, problems) = run(happy(), None)?;
    std::fs::write(output.join("validation-prompts.json"), serde_json::to_vec_pretty(&seen)?)?;
    let confirm = seen.iter().rev().find(|p| p["kind"] == "confirm").cloned().unwrap_or(Value::Null);
    let preview = confirm["message"].as_str().unwrap_or("").to_owned();
    std::fs::write(output.join("validation-preview.txt"), &preview)?;
    report.check("scripted prompts answered in order (pick, pick, input, confirm)", problems.is_empty() && seen.len() == 4, format!("{} prompts; problems {:?}", seen.len(), problems));
    report.check("confirm preview contains the full expected file text", preview.contains(&expected), format!("preview {} bytes, expected {} bytes", preview.len(), expected.len()));
    let resolved = resolved.map_err(|e| format!("happy path did not resolve: {e}"))?;
    report.check("action resolves to edits only (no file operations)", resolved.operations.is_empty() && resolved.edits.len() == 1, format!("{} operations, {} edit files", resolved.operations.len(), resolved.edits.len()));
    let mut edit = WorkspaceEdit {
        files: resolved.edits.into_iter().map(|(path, edits): (PathBuf, Vec<TextEdit>)| FileEdit::Edit { path: path.into(), edits, version: None }).collect(),
        ..Default::default()
    };
    documents.prepare_workspace_edit(&mut edit)?;
    documents.apply_workspace_edit(&edit)?;
    let (buffer, disk, _) = state();
    report.check("native workspace edit result bytes equal the independent oracle", buffer == expected, format!("buffer sha {} expected sha {}", sha(buffer.as_bytes()), sha(expected.as_bytes())));
    report.check("observation: the native engine writes the edit to disk itself (no separate Save needed)", disk == expected, format!("disk sha {} expected sha {}", sha(disk.as_bytes()), sha(expected.as_bytes())));
    documents.save(id)?;
    report.check("saved file equals the expected source", std::fs::read_to_string(&target)? == expected, format!("sha {}", sha(&std::fs::read(&target)?)));
    std::fs::write(output.join("validation-after.java"), &expected)?;
    let undo_id = applied.lock().unwrap().get(before_events).copied();
    match undo_id {
        Some(undo) => {
            let undone = documents.undo_workspace_edit(undo);
            let (buffer, _, _) = state();
            report.check("native grouped Undo restores the original bytes exactly", undone.is_ok() && buffer == original_text, format!("undo {undo}: {:?}; sha {} original {}", undone.err(), sha(buffer.as_bytes()), original_bytes_sha));
        }
        None => report.check("native grouped Undo restores the original bytes exactly", false, "no WorkspaceEditApplied event".to_owned()),
    }
    documents.save(id)?;
    report.check("tree hash is back to the pristine fixture after Undo and Save", tree_hash(&project) == pristine_hash, format!("{} vs {pristine_hash}", tree_hash(&project)));

    // cancel at every prompt
    for stop in 0..4 {
        let mut answers: Vec<Answer> = happy().into_iter().take(stop).collect();
        answers.push(Answer::Cancel);
        let events = applied.lock().unwrap().len();
        let (resolved, seen, problems) = run(answers, None)?;
        let (buffer, disk, tree) = state();
        let clean = buffer == original_text && disk == original_text && tree == pristine_hash && applied.lock().unwrap().len() == events;
        report.check(
            &format!("Cancel at prompt {} of 4 leaves the tree unchanged", stop + 1),
            clean && resolved.is_err() && problems.is_empty() && seen.len() == stop + 1,
            format!("resolve {:?}; prompts seen {}; problems {:?}", resolved.as_ref().err(), seen.len(), problems),
        );
    }

    // (3) stale source: edit the buffer between a prompt and the end of the action
    for (label, at) in [("after the first prompt", 1usize), ("at the final confirm", 3usize)] {
        let hook_documents = documents.clone();
        let hook: Box<dyn FnOnce() + Send> = Box::new(move || {
            let _ = hook_documents.apply_edits(id, &[TextEdit { range: Range::default(), new_text: "\n".into() }]);
        });
        let events = applied.lock().unwrap().len();
        let (resolved, seen, problems) = run(happy(), Some((at, hook)))?;
        let (buffer, disk, _) = state();
        let refused = match &resolved {
            Err(message) => message.to_lowercase().contains("stale") || message.to_lowercase().contains("changed"),
            Ok(_) => false,
        };
        report.check(
            &format!("stale buffer edit {label} is refused"),
            refused && applied.lock().unwrap().len() == events && disk == original_text && buffer == format!("\n{original_text}"),
            format!("resolve {:?}; prompts seen {}; problems {:?}; only the harness's own edit is in the buffer", resolved.as_ref().err(), seen.len(), problems),
        );
        documents.replace_all(id, &original_text)?;
        documents.save(id)?;
    }
    report.check("tree hash is pristine at the end of the run", tree_hash(&project) == pristine_hash, "disposable copy restored".to_owned());

    write_results(&report)?;
    Ok(report.failed)
}
