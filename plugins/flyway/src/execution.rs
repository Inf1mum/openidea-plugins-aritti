//! Finite, instance-owned commands. Never execute from render or replay a log tab.
use crate::*;
use openidea_plugin::host::{Child, Chunk, Command};
use openidea_plugin::views::{Field, FieldValue};
use std::time::Instant;
pub const VIEW: &str = "command-output";
const MAX_OUTPUT: usize = 16 << 10;
const MAX_INFO: usize = 1 << 20;
#[derive(Clone, Default)]
pub(crate) struct Redactor {
    literals: Vec<String>,
    suppress: bool,
}
fn sensitive(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "password",
        "passwd",
        "passphrase",
        "secret",
        "token",
        "credential",
        "authorization",
        "username",
        "user",
        "url",
        "privatekey",
        "accesskey",
    ]
    .iter()
    .any(|word| key.contains(word))
}
impl Redactor {
    fn add(&mut self, value: &str) {
        let value = value.trim().trim_matches(['\'', '"']);
        if value.is_empty() {
            return;
        }
        if value.len() > 4096 || self.literals.len() >= 128 {
            self.suppress = true;
            return;
        }
        if !self.literals.iter().any(|existing| existing == value) {
            self.literals.push(value.into());
            self.literals.sort_by_key(|value| std::cmp::Reverse(value.len()));
        }
    }
    fn source(&mut self, text: &str) {
        for line in text.lines() {
            // Java properties keep an inline # as value text; YAML/TOML may
            // treat it as a comment. Retain both bounded candidates instead of
            // leaking the suffix of a known properties credential.
            if let Some((key,value))=line.split_once('=').or_else(||line.split_once(':')) {
                if sensitive(key) {
                    let value=value.trim();
                    self.add(value);
                    if value.contains('\\') || value.contains("${") || value.contains("#{") || value.starts_with(['[','{']) {self.suppress=true;}
                }
            }
            let line = config::strip_comment(line).trim();
            if let Some((key, value)) = line.split_once('=').or_else(|| line.split_once(':')) {
                if sensitive(key) {
                    let value = value.trim();
                    if value.starts_with(['|', '>'])
                        || value.starts_with("\"\"\"")
                        || value.starts_with("'''")
                        || value.contains("${")
                        || value.contains("#{")
                    {
                        self.suppress = true;
                    }
                    self.add(value);
                    self.add(&openidea_plugin::text::unescape_json(value.trim().trim_matches('"')));
                }
            }
        }
    }
    pub fn literals(&self, text: &str) -> String {
        let mut out = String::new();
        let mut at = 0;
        while at < text.len() {
            if let Some(literal) = self.literals.iter().find(|literal| text[at..].starts_with(literal.as_str())) {
                out.push_str("[redacted]");
                at += literal.len();
            } else {
                let ch = text[at..].chars().next().unwrap();
                out.push(ch);
                at += ch.len_utf8();
            }
        }
        out
    }
    pub fn label(&self, text: &str) -> String {
        if self.suppress { "[label hidden: source secret inventory incomplete]".into() } else { self.literals(text) }
    }
    fn line(&self, line: &str) -> String {
        let lower = line.to_ascii_lowercase();
        if self.suppress
            || line.chars().any(|ch| ch.is_control() && ch != '\t')
            || lower.contains("://")
            || lower.contains("jdbc:")
            || [
                "password",
                "passwd",
                "token",
                "secret",
                "credential",
                "authorization",
                "authentication",
                "connection",
                "datasource",
                "user=",
                "user:",
            ]
            .iter()
            .any(|word| lower.contains(word))
        {
            "[connection/auth/output line suppressed]".into()
        } else {
            self.literals(line)
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
enum Observed {
    Absent,
    Source(String),
    Unavailable,
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Proof {
    root: String,
    runner: String,
    args: Vec<String>,
    mode: String,
    locations: String,
    files: Vec<(String, Observed)>,
    unknown: bool,
}
impl Proof {
    pub(crate) fn settings_current(&self, root: &str) -> bool {
        let (runner, args) = runner_args("info", true);
        self.root == root && self.runner == runner && self.args == args
            && self.mode == host::setting_string("executionMode").unwrap_or_else(|| "terminal".into())
            && self.locations == host::setting_string("locations").unwrap_or_default()
    }
    pub(crate) fn affected(&self, path: &str) -> bool {
        !path.is_empty() && (self.files.iter().any(|(source,_)| source == path || source.starts_with(&format!("{}/",path.trim_end_matches('/'))))
            || (owned(path,&self.root) && (path.ends_with(".sql") || is_flyway_config(path) || is_config_file(path))))
    }
}
pub(crate) fn info_scope(project: &Project) -> Result<Proof, Error> { Ok(prepare(project, "info", true)?.proof) }
pub(crate) struct Prepared {
    proof: Proof,
    pub redactor: Redactor,
    pub provenance: String,
}
fn owned(path: &str, root: &str) -> bool {
    path.len() <= 4096
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && !path.split('/').any(|part| part == "." || part == "..")
        && path.strip_prefix(root).is_some_and(|tail| tail.starts_with('/'))
}
/// Positive project-local configFiles facts only. These are observed inputs,
/// never a claim that Flyway's runtime environment/loader used only these files.
fn explicit_sources(root: &str, runner: &str, args: &[String]) -> (BTreeSet<String>, bool) {
    let mut paths = BTreeSet::new();
    let mut unknown = false;
    let redirected = args.iter().any(|arg| arg.starts_with("-workingDirectory=") || arg.starts_with("-Dflyway.workingDirectory="));
    for arg in args {
        let value = if runner == "mvn" { arg.strip_prefix("-Dflyway.configFiles=") } else { arg.strip_prefix("-configFiles=") };
        let Some(value) = value else { continue };
        let files: Vec<_> = value.split(',').collect();
        if files.len() > 16 { unknown = true; continue; }
        for file in files {
            if file.is_empty() || file == "-" || file.len() > 1024
                || file.chars().any(|c| c.is_control() || matches!(c, '\\' | '$' | '{' | '}' | '*' | '?' | '[' | ']' | '\'' | '"'))
                || file.strip_prefix('/').unwrap_or(file).split('/').any(|part| part.is_empty() || matches!(part, "." | ".."))
            {
                unknown = true;
                continue;
            }
            let path = if file.starts_with('/') {
                file.to_owned()
            } else {
                if redirected { unknown = true; continue; }
                format!("{root}/{file}")
            };
            if !owned(&path, root) || host::project_of(&path).and_then(|p| p.root().ok()).as_deref() != Some(root) {
                unknown = true;
                continue;
            }
            if paths.len() == 16 && !paths.contains(&path) {
                unknown = true;
                continue;
            }
            paths.insert(path);
        }
    }
    (paths, unknown || redirected)
}

fn prepare(project: &Project, goal: &str, json: bool) -> Result<Prepared, Error> {
    let root = project.root()?;
    if root.is_empty()
        || root.len() > 4096
        || !host::projects()?.iter().any(|project| project.root().ok().as_deref() == Some(root.as_str()))
    {
        return Err(Error::Stale);
    }
    let extra = host::setting_string("arguments").unwrap_or_default();
    if extra.len() > 8192 || extra.chars().any(char::is_control) {
        return Err("Arguments exceed the supported bound or contain controls.".into());
    }
    let (runner, args) = runner_args(goal, json);
    if args.len() > 128 || args.iter().skip(if json { 2 } else { 1 }).any(|arg| !arg.starts_with('-') || arg.len() > 4096) {
        return Err("Extra arguments must be separate option tokens, not additional goals; shell quoting is not interpreted.".into());
    }
    let mut redactor = Redactor::default();
    let mut shown = Vec::new();
    for arg in &args {
        if let Some((key, value)) = arg.split_once('=') {
            if sensitive(key) {
                redactor.add(value);
            }
            shown.push(format!("{key}=<value hidden>"));
        } else if arg.starts_with('-') {
            shown.push("<option token hidden>".into());
        } else {
            shown.push(arg.clone());
        }
    }
    let mut paths: BTreeSet<_> = config::CONFIG_FILES.iter().map(|file| format!("{root}/{file}")).collect();
    // Modern Flyway may automatically load personal overlays beside its
    // project configuration. Include their absence/content in the proof too.
    paths.extend([format!("{root}/flyway.user.toml"), format!("{root}/conf/flyway.user.toml")]);
    let (explicit, mut unknown) = explicit_sources(&root, runner, &args);
    paths.extend(explicit);
    match project.find_files("**/src/main/resources/application*", 33) {
        Ok(found) => {
            unknown |= found.len() > 32;
            for path in found.into_iter().take(32).filter(|path| is_config_file(path)) {
                if !owned(&path, &root) {
                    return Err(Error::Stale);
                }
                paths.insert(path);
            }
        }
        Err(_) => unknown = true,
    }
    let mut files = Vec::new();
    let mut lines = Vec::new();
    for path in paths {
        // Conventional and discovered sources need the same innermost owner
        // check as explicit configFiles, including currently absent paths.
        if !owned(&path, &root)
            || host::project_of(&path).and_then(|owner| owner.root().ok()).as_deref() != Some(root.as_str())
        {
            unknown = true;
            lines.push(format!("{path}: foreign/unavailable owner (not read)"));
            continue;
        }
        match project.read_file(&path) {
            Ok(text) if text.len() <= 64 * 1024 => {
                redactor.source(&text);
                files.push((path.clone(), Observed::Source(text.clone())));
                lines.push(format!("{path}: source observed (values hidden)"));
            }
            Err(Error::NotFound(_)) => {
                files.push((path.clone(), Observed::Absent));
                lines.push(format!("{path}: absent"));
            }
            _ => {
                unknown = true;
                redactor.suppress = true;
                files.push((path.clone(), Observed::Unavailable));
                lines.push(format!("{path}: unavailable/oversized"));
            }
        }
    }
    redactor.suppress |= unknown;
    let mode = host::setting_string("executionMode").unwrap_or_else(|| "terminal".into());
    if !matches!(mode.as_str(), "terminal" | "managed") {
        return Err("Unknown execution mode.".into());
    }
    let provenance = format!(
        "Invoking project: {root}\nApproved runner: {runner}\nArgv tokens (values hidden): {}\nObserved sources:\n{}\n{}\nEffective database target UNKNOWN. CLI configuration, Maven/plugin settings, inherited environment, selected environments, external/imported files and runtime precedence are unresolved. Observed Spring/plugin locations are editor hints and are not passed as CLI configuration. No source value is presented as effective connection truth.",
        shown.join(" "),
        lines.join("\n"),
        if unknown {
            "Source inventory incomplete; unknown inputs remain."
        } else {
            "Bounded known-source inventory observed; not a complete runtime loader model."
        }
    );
    let provenance = redactor.literals(&provenance);
    Ok(Prepared { proof: Proof { root, runner: runner.into(), args, mode, locations:host::setting_string("locations").unwrap_or_default(), files, unknown }, redactor, provenance })
}
#[derive(Default)]
struct Output {
    text: String,
    pending: String,
    dropping: bool,
    capped: bool,
}
impl Output {
    fn push(&mut self, text: &str, redactor: &Redactor, done: bool) {
        for ch in text.chars() {
            if ch == '\n' {
                let line = if self.dropping { "[oversized output line suppressed]".into() } else { redactor.line(&self.pending) };
                if self.text.len() + line.len() + 1 <= MAX_OUTPUT {
                    self.text.push_str(&line);
                    self.text.push('\n');
                } else {
                    self.capped = true;
                }
                self.pending.clear();
                self.dropping = false;
            } else if !self.dropping {
                if self.pending.len() + ch.len_utf8() > 4096 {
                    self.pending.clear();
                    self.dropping = true;
                } else {
                    self.pending.push(ch);
                }
            }
        }
        if done && (!self.pending.is_empty() || self.dropping) {
            self.push("\n", redactor, false);
        }
    }
}
struct Session {
    proof: Proof,
    root: String,
    goal: String,
    json: bool,
    child: Option<Child>,
    redactor: Redactor,
    provenance: String,
    status: String,
    output: Output,
    stderr: Output,
    raw: String,
    raw_capped: bool,
    started: Instant,
}
#[derive(Default)]
pub(crate) struct Execution {
    session: Option<Session>,
}
pub(crate) struct Finished {
    pub proof: Proof,
    pub root: String,
    pub goal: String,
    pub output: openidea_plugin::ProcessOutput,
    pub redactor: Redactor,
    pub info_error: Option<&'static str>,
}
impl Execution {
    pub fn start(&mut self, ctx: &Invocation, goal: &str, json: bool, force_managed: bool) -> Result<(), Error> {
        if self.session.as_ref().is_some_and(|session| session.child.is_some()) {
            return Err("A managed command is already running; inspect/cancel it from its owning project.".into());
        }
        let project = ctx.project().ok_or("Open a project first.")?;
        let prepared = prepare(&project, goal, json)?;
        let managed = force_managed || prepared.proof.mode == "managed";
        let impact = match goal {
            "clean" => "CLEAN DROPS ALL OBJECTS in the schemas selected by the CLI.",
            "migrate" => "Migrate may change the database schema/data.",
            "repair" => "Repair changes Flyway schema history.",
            _ => "This explicit command may connect to the CLI-selected database.",
        };
        let preview = format!(
            "{}\n\n{impact}\n{}\n{}",
            prepared.provenance,
            if managed {
                "Managed finite command: start/status/cancel in Flyway Command Output. It belongs to this plugin instance and may be terminated by window/plugin shutdown or roughly five-minute idle reclamation. Refresh Status polls it; no durable job claim."
            } else {
                "Open the approved command in the invoking project's terminal; native terminal controls own its status/cancellation and raw output."
            },
            "Cancellation/termination is not a database rollback. Runtime target remains unknown; review external CLI/environment settings before proceeding."
        );
        if !ui_kit::prompts::confirm_exact(&format!("Flyway: {goal}"), &preview)? {
            return Err(Error::Cancelled);
        }
        if ctx.project().and_then(|project| project.root().ok()).as_deref() != Some(prepared.proof.root.as_str())
            || prepare(&project, goal, json)?.proof != prepared.proof
        {
            return Err(Error::Stale);
        }
        if managed {
            let result = host::start(&Command::new(&prepared.proof.runner).args(&prepared.proof.args), Some(&project));
            let mut session = Session {
                proof: prepared.proof.clone(),
                root: prepared.proof.root,
                goal: goal.into(),
                json,
                child: None,
                redactor: prepared.redactor,
                provenance: prepared.provenance,
                status: String::new(),
                output: Output::default(),
                stderr: Output::default(),
                raw: String::new(),
                raw_capped: false,
                started: Instant::now(),
            };
            match result {
                Ok(child) => {
                    if let Err(error) = child.close_stdin() {
                        session.status = session.redactor.line(&format!("Failed to close command input: {error}"));
                        self.session = Some(session);
                        refresh_migrations();
                        return Err(Error::Failed("Command input initialization failed; owned process handle closed.".into()));
                    }
                    session.child = Some(child);
                    session.status = "Running — Refresh Status to poll output.".into();
                }
                Err(error) => {
                    session.status = session.redactor.line(&format!("Command could not start: {error}"));
                    self.session = Some(session);
                    ui_kit::web::refresh_view(VIEW, include_str!("ui_config.json"));
                    refresh_migrations();
                    return match error {
                        Error::Denied(_) => Err(error),
                        _ => Err(Error::Failed("Managed command could not start; inspect scoped Command Output.".into())),
                    };
                }
            }
            self.session = Some(session);
            ui_kit::web::reveal_view(VIEW, include_str!("ui_config.json"))?;
        } else {
            let native = !json && matches!(goal, "info" | "validate")
                && crate::native_runs::configurations(&prepared.proof.root).iter().any(|config|
                    config.id == format!("native-{goal}") && config.program == prepared.proof.runner && config.args == prepared.proof.args);
            let launch = if native {
                host::start_run(&format!("native-{goal}"), &[], false).map(|_| ())
            } else {
                host::open_terminal(&format!("Flyway {goal}"), &prepared.proof.runner, &prepared.proof.args)
            };
            if let Err(error) = launch {
                return match error {
                    Error::Denied(_) => Err(error),
                    _ => Err(Error::Failed(prepared.redactor.line(&error.to_string()))),
                };
            }
        }
        ui_kit::web::refresh_view(VIEW, include_str!("ui_config.json"));
        if managed { refresh_migrations(); }
        Ok(())
    }
    pub(crate) fn migration_status(&self, root: &str) -> Option<String> {
        self.session.as_ref().filter(|session|session.root==root).map(|session|format!("{}: {}",session.goal,session.status))
    }
    fn session(&mut self, root: &str) -> Result<&mut Session, Error> {
        self.session.as_mut().filter(|session| session.root == root).ok_or(Error::Stale)
    }
    pub fn poll(&mut self, root: &str) -> Result<Option<Finished>, Error> {
        let session = self.session(root)?;
        let Some(child) = &session.child else {
            return Ok(None);
        };
        let chunk = match child.poll(16 * 1024) {
            Ok(chunk) => chunk,
            Err(_) => {
                session.child = None;
                session.status = "Process status unavailable; handle closed. Runtime outcome unknown.".into();
                refresh_migrations();
                return Ok(None);
            }
        };
        let previous = session.status.clone();
        let result = Self::ingest(session, chunk);
        if session.status != previous {refresh_migrations();}
        Ok(result)
    }
    fn ingest(session: &mut Session, chunk: Chunk) -> Option<Finished> {
        if session.json {
            if session.raw.len() + chunk.stdout.len() > MAX_INFO {
                session.raw_capped = true;
            } else if !session.raw_capped {
                session.raw.push_str(&chunk.stdout);
            }
        }
        let drained = chunk.stdout.is_empty() && chunk.stderr.is_empty() && chunk.status.is_some();
        session.output.push(&chunk.stdout, &session.redactor, drained);
        session.stderr.push(&chunk.stderr, &session.redactor, drained);
        let status = chunk.status?;
        if !drained {
            session.status = format!("Process exited {status}; Refresh Status to finish draining bounded output.");
            return None;
        }
        session.status = format!(
            "Finished with exit {status} after {}s. Database outcome is not inferred from local source.",
            session.started.elapsed().as_secs()
        );
        session.child = None;
        if session.json {
            let stdout = if session.raw_capped { String::new() } else { std::mem::take(&mut session.raw) };
            Some(Finished {
                proof: session.proof.clone(),
                root: session.root.clone(),
                goal: session.goal.clone(),
                output: openidea_plugin::ProcessOutput {
                    status: Some(status),
                    stdout,
                    stderr: if session.stderr.text.trim().is_empty() { session.output.text.clone() } else { session.stderr.text.clone() },
                },
                redactor: session.redactor.clone(),
                info_error: session
                    .raw_capped
                    .then_some("Flyway Info output exceeded the 1 MiB JSON buffer; no complete result is available."),
            })
        } else {
            None
        }
    }
    pub fn cancel(&mut self, root: &str) -> Result<(), Error> {
        let session = self.session(root)?;
        let Some(child) = &session.child else {
            return Err("No managed command is running.".into());
        };
        child.kill()?;
        session.child = None;
        session.raw.clear();
        session.output.push("", &session.redactor, true);
        session.stderr.push("", &session.redactor, true);
        session.status = "Cancelled owned process; database outcome/rollback are unknown.".into();
        refresh_migrations();
        Ok(())
    }
    pub fn view(&self, root: &str) -> View {
        let Some(session) = self.session.as_ref().filter(|session| session.root == root) else {
            return View::message("No command output for this project. Run a Flyway command explicitly; rendering never starts one.");
        };
        View::new(Body::Form(vec![
            Field::new("execution-status", "Status", FieldValue::readonly_text(&session.status, true)),
            Field::new(
                "execution-provenance",
                "Invoked source provenance (not effective target)",
                FieldValue::readonly_text(&session.provenance, true),
            ),
            Field::new(
                "execution-log",
                "Redacted bounded output",
                FieldValue::readonly_text(
                    &format!(
                        "[stdout]\n{}\n[stderr]\n{}{}",
                        session.output.text,
                        session.stderr.text,
                        if session.output.capped || session.stderr.capped { "\n[output capped]" } else { "" }
                    ),
                    true,
                ),
            ),
        ]))
        .button(Button::new("execution-status", "Refresh Status").icon("refresh").enabled(session.child.is_some()))
        .button(Button::new("execution-cancel", "Cancel Command").enabled(session.child.is_some()))
        .status("Instance-lifetime finite command · output classification is conservative, not universal · cancellation is not rollback")
    }
    pub fn closed_roots(&mut self) {
        if !self.session.as_ref().is_some_and(|session| session.child.is_some()) {
            return;
        }
        let roots = match host::projects().and_then(|projects| {
            projects.into_iter().map(|project| project.root()).collect::<Result<Vec<_>, _>>()
        }) {
            Ok(roots) => roots,
            Err(_) => {
                if let Some(session) = &mut self.session {
                    let status = "Project membership unavailable; owned process retained. Retry from its owning project; runtime outcome unknown.";
                    if session.status != status {
                        session.status = status.into();
                        ui_kit::web::refresh_view(VIEW, include_str!("ui_config.json"));
                        refresh_migrations();
                    }
                }
                return;
            }
        };
        let closed = self.session.as_ref().is_some_and(|session| {
            session.child.is_some() && !roots.iter().any(|root| root == &session.root)
        });
        if !closed {
            if let Some(session) = &mut self.session {
                if session.status.starts_with("Project membership unavailable;") {
                    session.status = "Project membership revalidated; Refresh Status to poll the owned process. Runtime outcome unknown.".into();
                    ui_kit::web::refresh_view(VIEW, include_str!("ui_config.json"));
                    refresh_migrations();
                }
            }
        }
        if closed {
            if let Some(session) = &mut self.session {
                if let Some(child) = session.child.take() {
                    let _ = child.kill();
                }
                session.raw.clear();
                session.status = "Project closed; owned process handle closed; database outcome unknown.".into();
                refresh_migrations();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn managed() -> MockHost {
        let host = MockHost::install().with_capabilities(&["project.read", "events", "process:flyway", "storage"]);
        host.set_setting("executionMode", "\"managed\"");
        host
    }
    fn start(execution: &mut Execution, host: &MockHost, json: bool) {
        host.answer_next_prompt(Some(&["yes"]));
        execution.start(&host.invocation(), if json { "info" } else { "validate" }, json, false).unwrap();
    }
    #[test]
    fn interactive_readonly_launch_uses_native_run_after_review_without_terminal_fallback() {
        let host = MockHost::install().with_capabilities(&["project.read", "events", "process:flyway", "storage"]);
        host.add_file("flyway.toml", "");
        host.set_run_configurations(crate::native_runs::configurations("/project"));
        let mut execution = Execution::default();
        host.answer_next_prompt(None);
        assert!(execution.start(&host.invocation(), "validate", false, false).is_err());
        assert!(host.configuration_runs().is_empty());
        host.answer_next_prompt(Some(&["yes"]));
        execution.start(&host.invocation(), "validate", false, false).unwrap();
        assert_eq!(host.configuration_runs()[0].1, "native-validate");
        assert!(host.configuration_runs()[0].2.is_empty());
        assert!(host.terminals().is_empty());
        host.set_run_configurations(Vec::new());
        host.answer_next_prompt(Some(&["yes"]));
        assert!(execution.start(&host.invocation(), "validate", false, false).is_err());
        assert!(host.terminals().is_empty());
    }
    fn invalidations(host: &MockHost) -> usize {
        host.view_messages().iter().filter(|(_,text)| serde_json::from_str::<serde_json::Value>(text).ok().is_some_and(|value| value["channel"] == "ui-kit-refresh" && value["targetView"] == "migrations")).count()
    }
    fn finished_info(host: &MockHost) -> Finished {
        Finished {proof:info_scope(&host.invocation().project().unwrap()).unwrap(),root:"/project".into(),goal:"info".into(),
            output:openidea_plugin::ProcessOutput {status:Some(0),stdout:r#"{"migrations":[]}"#.into(),stderr:String::new()},redactor:Redactor::default(),info_error:None}
    }
    #[test]
    fn completed_info_callback_rejects_conflicting_or_duplicate_migration_objects() {
        for stdout in [r#"{"migrations":[]} {"migrations":[{"version":"1","state":"Pending"}]}"#,r#"{"migrations":[],"migrations":[{"version":"1","state":"Pending"}]}"#] {
            let host=managed();let mut plugin=Flyway::default();let mut finished=finished_info(&host);
            finished.output.stdout=stdout.into();
            plugin.accept_finished(&host.invocation().project().unwrap(),finished).unwrap();
            let view=&plugin.info_cache.as_ref().unwrap().1;
            assert!(matches!(&view.body,Body::Message(message) if message.contains("unambiguous")),"{view:?}");
            assert!(plugin.targets.is_empty());
            assert!(host.process_runs().is_empty()&&host.requests().is_empty());
        }
    }

    #[test]
    fn accepted_info_and_managed_start_cancel_refresh_native_view_without_render_loop() {
        let host=managed();let mut plugin=Flyway::default();
        plugin.accept_finished(&host.invocation().project().unwrap(),finished_info(&host)).unwrap();
        assert_eq!(invalidations(&host),1);
        host.on_process("flyway",|_,_|panic!("render cannot execute"));
        for _ in 0..3 {plugin.render("migrations",None,&host.invocation()).unwrap();}
        assert_eq!(invalidations(&host),1);
        start(&mut plugin.execution,&host,false);assert_eq!(invalidations(&host),2);
        assert!(plugin.render("migrations",None,&host.invocation()).unwrap().status.unwrap().contains("Running"));
        plugin.run_command("cancel",&host.invocation(),None).unwrap();assert_eq!(invalidations(&host),3);
        assert!(plugin.render("migrations",None,&host.invocation()).unwrap().status.unwrap().contains("Cancelled"));
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn foreign_missing_denied_render_neither_exposes_nor_kills_another_owned_job() {
        for case in 0..3 {
            let host=managed();let mut plugin=Flyway::default();start(&mut plugin.execution,&host,false);
            let host=match case {
                0=>{host.add_project("/project");host.set_project_root(Some("/foreign"));host},
                1=>{host.add_project("/project");host.set_project_root(None);host},
                _=>host.with_capabilities(&["events", "storage"]),
            };
            let view=plugin.render("migrations",None,&host.invocation()).unwrap();
            assert!(matches!(view.body,Body::Message(_)));
            assert!(!format!("{view:?}").contains("Running"));
            assert!(plugin.execution.session.as_ref().unwrap().child.is_some(),"render cannot close foreign/missing/denied owner's child");
            assert_eq!(invalidations(&host),1);
        }
    }
    #[test]
    fn changed_config_or_runner_before_info_completion_withholds_the_result() {
        for case in 0..2 {
            let host=managed();let mut plugin=Flyway::default();let finished=finished_info(&host);
            if case==0 {host.add_file("flyway.conf","flyway.locations=filesystem:changed");} else {host.set_setting("runner","\"mvn\"");}
            assert!(plugin.accept_finished(&host.invocation().project().unwrap(),finished).is_err());
            assert!(plugin.info_cache.is_none());assert_eq!(invalidations(&host),1);
        }
    }
    #[test]
    fn settings_drift_hides_old_info_and_navigation_without_render_discovery_or_cli() {
        let host=managed();let mut plugin=Flyway::default();plugin.accept_finished(&host.invocation().project().unwrap(),finished_info(&host)).unwrap();
        host.set_setting("arguments","\"-password=UNDISPLAYED_SECRET\"");
        let view=plugin.render("migrations",None,&host.invocation()).unwrap();
        assert!(matches!(view.body,Body::Message(ref text) if text.contains("No explicit")));
        assert!(!format!("{view:?}").contains("UNDISPLAYED_SECRET"));
        assert!(plugin.info_cache.is_some(),"a render only withholds; it does not erase another frame's snapshot");
        assert_eq!(invalidations(&host),1);assert!(host.process_runs().is_empty());
    }
    #[test]
    fn changed_plugin_location_hint_withholds_captured_rows_without_discovery() {
        let host=managed();let mut plugin=Flyway::default();plugin.accept_finished(&host.invocation().project().unwrap(),finished_info(&host)).unwrap();
        host.set_setting("locations","\"filesystem:other\"");
        assert!(matches!(plugin.render("migrations",None,&host.invocation()).unwrap().body,Body::Message(_)));
        assert_eq!(invalidations(&host),1);assert!(host.process_runs().is_empty());
    }
    #[test]
    fn observed_config_or_parent_deletion_retires_info_not_owned_child() {
        for path in ["/project/flyway.conf","/project"] {
            let host=managed();let mut plugin=Flyway::default();plugin.accept_finished(&host.invocation().project().unwrap(),finished_info(&host)).unwrap();
            start(&mut plugin.execution,&host,false);
            plugin.handle_events(&[Event::FilesChanged(openidea_plugin::FilesChanged {removed:vec![path.into()],..Default::default()})],&host.invocation());
            assert!(plugin.info_cache.is_none());assert!(plugin.execution.session.as_ref().unwrap().child.is_some());
            assert_eq!(invalidations(&host),3);
        }
    }
    #[test]
    fn public_status_draining_and_finish_refresh_changed_managed_result_only() {
        let host=managed();let mut plugin=Flyway::default();start(&mut plugin.execution,&host,false);
        plugin.execution.session.as_ref().unwrap().child.as_ref().unwrap().kill().unwrap();
        plugin.run_command("status",&host.invocation(),None).unwrap();
        assert!(plugin.execution.migration_status("/project").unwrap().contains("draining"));
        assert_eq!(invalidations(&host),2);
        plugin.run_command("status",&host.invocation(),None).unwrap();
        assert!(plugin.execution.migration_status("/project").unwrap().contains("Finished"));
        assert_eq!(invalidations(&host),3);
        plugin.run_command("status",&host.invocation(),None).unwrap();
        assert_eq!(invalidations(&host),3,"no snapshot/status change, no redundant native invalidation");
        assert!(plugin.info_cache.is_none());
    }
    #[test]
    fn external_generation_command_refreshes_native_preview_without_cli_or_file_write() {
        let host=managed();let mut plugin=Flyway::default();
        plugin.run_command("new-migration",&host.invocation(),None).unwrap();
        assert_eq!(invalidations(&host),1);
        assert!(matches!(plugin.render("migrations",None,&host.invocation()).unwrap().body,Body::Form(_)));
        assert_eq!(invalidations(&host),1);assert!(host.process_runs().is_empty());
    }
    #[test]
    fn foreign_completed_info_does_not_replace_owning_project_cache() {
        let host=managed();let mut plugin=Flyway::default();plugin.accept_finished(&host.invocation().project().unwrap(),finished_info(&host)).unwrap();
        let mut foreign=finished_info(&host);foreign.root="/foreign".into();
        assert!(plugin.accept_finished(&host.invocation().project().unwrap(),foreign).is_err());
        assert_eq!(invalidations(&host),1);assert_eq!(plugin.info_cache.as_ref().unwrap().0,"/project");
    }
    #[test]
    fn render_is_cached_and_never_invokes_flyway_or_a_database() {
        let host = managed();
        host.on_process("flyway", |_, _| panic!("render must not execute"));
        let mut plugin = Flyway::default();
        for _ in 0..2 {
            let view = plugin.render("migrations", None, &host.invocation()).unwrap();
            assert!(matches!(view.body,Body::Message(message) if message.contains("No explicit")));
        }
        assert!(host.process_runs().is_empty());
        assert!(host.terminals().is_empty());
    }
    #[test]
    fn provenance_is_literal_source_evidence_not_runtime_target_and_proof_is_exact() {
        let host = managed();
        host.add_file("flyway.conf", "flyway.url=jdbc:postgresql://u:p@db/acme\nflyway.user=admin\nflyway.password=topsecret\n");
        host.set_setting("arguments", "\"-password=clisecret -environment=dev\"");
        let project = host.invocation().project().unwrap();
        let prepared = prepare(&project, "validate", false).unwrap();
        for secret in ["jdbc:postgresql", "topsecret", "admin", "clisecret"] {
            assert!(!prepared.provenance.contains(secret));
        }
        assert!(prepared.provenance.contains("Effective database target UNKNOWN"));
        assert!(prepared.provenance.contains("/project/flyway.conf"));
        host.add_file("flyway.conf", "flyway.password=changed\n");
        assert!(prepare(&project, "validate", false).unwrap().proof != prepared.proof);
        assert_eq!(prepared.redactor.literals("a topsecret value"), "a [redacted] value");
    }
    #[test]
    fn nested_projects_do_not_supply_outer_command_source_proof() {
        for nested in ["/project/conf", "/project/src/main/resources"] {
            let host = managed();
            let relative = if nested.ends_with("conf") { "conf/flyway.conf" } else { "src/main/resources/application.properties" };
            host.add_file(relative, "flyway.password=nested-only-secret");
            let before = prepare(&host.invocation().project().unwrap(), "validate", false).unwrap().proof;
            host.add_project(nested);
            let prepared = prepare(&host.invocation().project().unwrap(), "validate", false).unwrap();
            assert!(prepared.proof.unknown, "{nested}");
            assert!(prepared.proof != before, "new nested ownership must change the approval proof");
            assert!(prepared.redactor.suppress);
            assert!(!prepared.proof.files.iter().any(|(path, _)| path.starts_with(&format!("{nested}/"))), "foreign sources must not be accepted into the outer proof");
            assert!(!prepared.redactor.literals.iter().any(|value| value == "nested-only-secret"));
            assert!(prepared.provenance.contains("foreign/unavailable owner"));
            assert!(host.process_runs().is_empty() && host.terminals().is_empty());
        }
    }
    #[test]
    fn explicit_cli_and_maven_configs_and_personal_overlay_participate_in_proof_and_redaction() {
        let host = managed();
        host.add_file("config/selected.toml", "[environments.dev]\npassword = \"custom-secret\"\n");
        host.add_file("flyway.user.toml", "[environments.dev]\npassword = \"personal-secret\"\n");
        let project = host.invocation().project().unwrap();
        for (runner, option) in [("flyway", "-configFiles=config/selected.toml"), ("mvn", "-Dflyway.configFiles=config/selected.toml")] {
            host.set_setting("runner", &serde_json::to_string(runner).unwrap());
            host.set_setting("arguments", &serde_json::to_string(option).unwrap());
            let before = prepare(&project, "validate", false).unwrap();
            assert!(before.provenance.contains("/project/config/selected.toml"));
            assert!(before.provenance.contains("/project/flyway.user.toml"));
            assert_eq!(before.redactor.literals("custom-secret personal-secret"), "[redacted] [redacted]");
            host.add_file("config/selected.toml", "password=changed-secret\n");
            assert!(before.proof != prepare(&project, "validate", false).unwrap().proof);
            host.add_file("config/selected.toml", "password=custom-secret\n");
        }
        let before = prepare(&project, "validate", false).unwrap();
        host.add_file("flyway.user.toml", "password=new-personal-secret\n");
        assert!(before.proof != prepare(&project, "validate", false).unwrap().proof);
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn unresolved_explicit_sources_do_not_read_outside_owner_and_suppress_output() {
        let host = managed();
        host.add_project("/project/nested");
        host.add_file("nested/custom.conf", "password=foreign-secret");
        let project = host.invocation().project().unwrap();
        for arg in ["-configFiles=-", "-configFiles=/outside/secret.conf", "-configFiles=../secret.conf", "-configFiles=${CONFIG}", "-configFiles=nested/custom.conf", "-workingDirectory=elsewhere -configFiles=custom.conf", "-configFiles=/project//custom.conf"] {
            host.set_setting("arguments", &serde_json::to_string(arg).unwrap());
            let prepared = prepare(&project, "validate", false).unwrap();
            assert!(prepared.proof.unknown, "{arg}");
            assert!(prepared.redactor.suppress, "{arg}");
            assert_eq!(prepared.redactor.line("unclassified potentially sensitive output"), "[connection/auth/output line suppressed]");
            assert!(!prepared.proof.files.iter().any(|(path,_)| path.contains("nested/") || path.contains("outside/")), "{arg}");
        }
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn explicit_config_list_is_bounded_and_observed_paths_invalidate_saved_info() {
        let host = managed();
        let project = host.invocation().project().unwrap();
        let args = vec!["-configFiles=one.conf,/project/two.toml".to_owned()];
        let (paths, unknown) = explicit_sources("/project", "flyway", &args);
        assert!(!unknown);
        assert_eq!(paths, BTreeSet::from(["/project/one.conf".into(), "/project/two.toml".into()]));
        let many = (0..17).map(|i|format!("{i}.conf")).collect::<Vec<_>>().join(",");
        assert!(explicit_sources("/project", "flyway", &[format!("-configFiles={many}")]).1);
        host.add_file("chosen.conf", "flyway.password=bounded-secret");
        host.set_setting("arguments", "\"-configFiles=chosen.conf\"");
        let proof = prepare(&project, "validate", false).unwrap().proof;
        assert!(proof.affected("/project/chosen.conf"));
        assert!(proof.affected("/project/flyway.user.toml"));
    }
    #[test]
    fn known_properties_secret_hash_suffix_is_not_mistaken_for_a_comment() {
        let host=managed();
        host.add_file("flyway.conf","flyway.password=prefix#privateSuffix\n");
        let prepared=prepare(&host.invocation().project().unwrap(),"validate",false).unwrap();
        let mut output=Output::default();
        output.push("prefix#privateSuffix\n",&prepared.redactor,true);
        assert!(!output.text.contains("privateSuffix"),"{}",output.text);
        assert!(!prepared.redactor.label("prefix#privateSuffix").contains("privateSuffix"));
        host.add_file("flyway.conf","flyway.password=prefix\\#privateSuffix\n");
        let prepared=prepare(&host.invocation().project().unwrap(),"validate",false).unwrap();
        assert!(prepared.redactor.suppress,"escaped source secret decoding is unresolved");
        assert!(!prepared.redactor.line("prefix#privateSuffix").contains("privateSuffix"));
        assert!(host.process_runs().is_empty()&&host.requests().is_empty());
    }

    #[test]
    fn streaming_redaction_buffers_split_literals_and_suppresses_urls_controls_and_auth_lines() {
        let mut redactor = Redactor::default();
        redactor.add("topsecret");
        redactor.add("a");
        let mut output = Output::default();
        output.push("topsec", &redactor, false);
        assert!(output.text.is_empty());
        output.push("ret\npassword=unrecognized\njdbc:vendor://u:p@host/db\n\u{1b}[31msensitive\n", &redactor, true);
        assert!(!output.text.contains("topsecret"));
        assert!(!output.text.contains("unrecognized"));
        assert!(!output.text.contains("u:p@host"));
        assert!(output.text.contains("suppressed"));
        assert!(redactor.literals("aaaa").len() < 100, "replacement markers are never recursively reprocessed");
        output.push(&"x".repeat(5000), &redactor, true);
        assert!(output.text.contains("oversized"));
    }
    #[test]
    fn managed_child_has_owned_status_cancel_busy_and_project_guards() {
        let host = managed();
        let mut execution = Execution::default();
        start(&mut execution, &host, false);
        assert!(execution.start(&host.invocation(), "migrate", false, false).is_err());
        assert!(execution.cancel("/other").is_err());
        execution.poll("/project").unwrap();
        assert!(execution.session.as_ref().unwrap().output.text.contains("started: flyway"));
        execution.cancel("/project").unwrap();
        assert!(execution.session.as_ref().unwrap().child.is_none());
        assert!(execution.session.as_ref().unwrap().status.contains("rollback"));
        host.set_project_root(Some("/other"));
        assert!(matches!(execution.view("/other").body, Body::Message(_)));
    }
    #[test]
    fn status_drains_final_chunks_before_marking_json_complete_and_caps_raw_output() {
        let host = managed();
        let mut execution = Execution::default();
        start(&mut execution, &host, true);
        let session = execution.session.as_mut().unwrap();
        let first = Chunk { stdout: "{\"migrations\":[".into(), stderr: "password=hidden\n".into(), status: Some(0) };
        assert!(Execution::ingest(session, first).is_none());
        assert!(session.child.is_some());
        assert!(Execution::ingest(session, Chunk { stdout: "]}".into(), status: Some(0), ..Default::default() }).is_none());
        let finished = Execution::ingest(session, Chunk { status: Some(0), ..Default::default() }).unwrap();
        assert!(info::parse(&finished.output.stdout).unwrap().is_empty());
        assert!(!finished.output.stderr.contains("hidden"));
        assert!(session.child.is_none());
        start(&mut execution, &host, true);
        let session = execution.session.as_mut().unwrap();
        Execution::ingest(session, Chunk { stdout: "x".repeat(MAX_INFO + 1), ..Default::default() });
        let finished = Execution::ingest(session, Chunk { status: Some(0), ..Default::default() }).unwrap();
        assert!(finished.output.stdout.is_empty());
    }
    #[test]
    fn unavailable_project_inventory_does_not_claim_closed_or_cancel_owned_child() {
        let host = managed();
        let mut execution = Execution::default();
        start(&mut execution, &host, false);
        let host = host.with_capabilities(&["events", "process:flyway", "storage"]);
        execution.closed_roots();
        let session = execution.session.as_ref().unwrap();
        assert!(session.child.is_some(), "inventory failure is not evidence of project closure");
        assert!(session.status.contains("unavailable"));
        assert!(!session.status.contains("Project closed"));
        let host = host.with_capabilities(&["project.read", "events", "process:flyway", "storage"]);
        execution.closed_roots();
        assert!(execution.session.as_ref().unwrap().child.is_some());
        assert!(execution.session.as_ref().unwrap().status.contains("revalidated"));
        execution.cancel("/project").unwrap();
        assert!(execution.session.as_ref().unwrap().child.is_none());
    }
    #[test]
    fn failed_start_and_closed_project_remain_scoped_and_do_not_claim_database_success() {
        let host = MockHost::install().with_capabilities(&["project.read", "events", "storage"]);
        host.set_setting("executionMode", "\"managed\"");
        let mut execution = Execution::default();
        host.answer_next_prompt(Some(&["yes"]));
        assert!(matches!(execution.start(&host.invocation(), "validate", false, false), Err(Error::Denied(_))));
        assert!(execution.session.as_ref().unwrap().status.contains("could not start"));
        let host = managed();
        let mut execution = Execution::default();
        start(&mut execution, &host, false);
        host.set_project_root(Some("/other"));
        execution.closed_roots();
        assert!(execution.session.as_ref().unwrap().child.is_none());
        assert!(execution.session.as_ref().unwrap().status.contains("outcome unknown"));
    }
    #[test]
    fn cancel_and_extra_goals_never_execute_and_unknown_sources_are_labelled() {
        let host = managed();
        let mut execution = Execution::default();
        assert!(matches!(execution.start(&host.invocation(), "clean", false, false), Err(Error::Cancelled)));
        assert!(execution.session.is_none());
        host.set_setting("arguments", "\"clean\"");
        assert!(prepare(&host.invocation().project().unwrap(), "validate", false).is_err());
        host.set_setting("arguments", "\"\"");
        host.add_file("flyway.conf", &"x".repeat(64 * 1024 + 1));
        let prepared = prepare(&host.invocation().project().unwrap(), "validate", false).unwrap();
        assert!(prepared.proof.unknown);
        assert!(prepared.provenance.contains("incomplete"));
        assert!(prepared.redactor.suppress);
        for value in ["${DB_PASSWORD}", "|", "\"\"\"multiline"] {
            let mut redactor = Redactor::default();
            redactor.source(&format!("password={value}"));
            assert!(redactor.suppress);
        }
    }
}
