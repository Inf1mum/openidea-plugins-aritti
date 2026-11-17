//! Finite, instance-owned commands. Never execute from render or replay a log tab; render only polls an owned child.
use crate::*;
use openidea_plugin::host::{Child, Chunk, Command};
use openidea_plugin::views::{Field, FieldValue};
use std::time::Instant;
pub const VIEW: &str = "command-output";
const MAX_OUTPUT: usize = 16 << 10;
const MAX_INFO: usize = 1 << 20;
#[derive(Clone, Default)]
pub(crate) struct Redactor {
    /// Secret values, longest first; `true` means whole-token matching only.
    literals: Vec<(String, bool)>,
    suppress: bool,
}
#[derive(Clone, Copy, PartialEq)]
enum Secret {
    /// Password-like: hidden as a plain substring from 2 characters, a whole token below.
    Always,
    /// Connection URL: plain substring, and its `user:pass@` userinfo is split out.
    Url,
    /// User names and token-like settings: skipped under 4 characters, whole tokens under 8.
    Name,
}
/// Credential keys. `user` counts in Flyway config, under a datasource/Flyway key, or as an
/// undotted leaf whose parent is unknown (YAML); `app.user` is not a credential.
fn secret(key: &str, flyway: bool) -> Option<Secret> {
    let key = key.trim().to_ascii_lowercase();
    if ["password", "passwd", "passphrase", "secret", "credential", "privatekey", "accesskey", "authorization", "licensekey"].iter().any(|word| key.contains(word)) {
        return Some(Secret::Always);
    }
    if key.contains("url") {
        return Some(Secret::Url);
    }
    let user = key.contains("user") && (flyway || key.contains("flyway") || key.contains("datasource") || !key.contains('.'));
    (user || key.contains("token")).then_some(Secret::Name)
}
fn word(ch: Option<char>) -> bool {
    ch.is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
}
/// Masks `scheme://user:pass@` userinfo wherever it appears.
fn mask_userinfo(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let (head, tail) = rest.split_at(at + 3);
        out.push_str(head);
        let end = tail.find(|ch: char| ch == '/' || ch.is_whitespace()).unwrap_or(tail.len());
        match tail[..end].rfind('@') {
            Some(userinfo) if userinfo > 0 => {
                out.push_str("[redacted]");
                rest = &tail[userinfo..];
            }
            _ => rest = tail,
        }
    }
    out.push_str(rest);
    out
}
impl Redactor {
    fn add(&mut self, value: &str, kind: Secret) {
        let value = value.trim().trim_matches(['\'', '"']);
        if kind == Secret::Url {
            if let Some((_, tail)) = value.split_once("://") {
                let authority = tail.split(['/', '?', '#']).next().unwrap_or_default();
                if let Some((userinfo, _)) = authority.rsplit_once('@') {
                    let (user, password) = userinfo.split_once(':').unwrap_or((userinfo, ""));
                    self.add(user, Secret::Name);
                    self.add(password, Secret::Always);
                }
            }
        }
        let length = value.chars().count();
        if value.is_empty() || kind == Secret::Name && length < 4 {
            return;
        }
        if value.len() > 4096 || self.literals.len() >= 128 {
            self.suppress = true;
            return;
        }
        // A one-character secret hidden as a substring would erase that letter everywhere.
        let whole = kind == Secret::Name && length < 8 || length < 2;
        match self.literals.iter_mut().find(|(existing, _)| existing == value) {
            Some(existing) => existing.1 &= whole,
            None => {
                self.literals.push((value.into(), whole));
                self.literals.sort_by_key(|(value, _)| std::cmp::Reverse(value.len()));
            }
        }
    }
    fn source(&mut self, text: &str, flyway: bool) {
        for line in text.lines() {
            // Java properties keep an inline # as value text; YAML/TOML may
            // treat it as a comment. Retain both bounded candidates instead of
            // leaking the suffix of a known properties credential.
            if let Some((key,value))=line.split_once('=').or_else(||line.split_once(':')) {
                if let Some(kind) = secret(key, flyway) {
                    let value=value.trim();
                    self.add(value, kind);
                    if value.contains('\\') || value.contains("${") || value.contains("#{") || value.starts_with(['[','{']) {self.suppress=true;}
                }
            }
            let line = config::strip_comment(line).trim();
            if let Some((key, value)) = line.split_once('=').or_else(|| line.split_once(':')) {
                if let Some(kind) = secret(key, flyway) {
                    let value = value.trim();
                    if value.starts_with(['|', '>'])
                        || value.starts_with("\"\"\"")
                        || value.starts_with("'''")
                        || value.contains("${")
                        || value.contains("#{")
                    {
                        self.suppress = true;
                    }
                    self.add(value, kind);
                    self.add(&openidea_plugin::text::unescape_json(value.trim().trim_matches('"')), kind);
                }
            }
        }
    }
    /// Replaces secret values (short names only as whole tokens, so `sa` leaves `sales` alone)
    /// and any URL userinfo.
    pub fn literals(&self, text: &str) -> String {
        let text = mask_userinfo(text);
        let mut out = String::new();
        let mut at = 0;
        while at < text.len() {
            let before = text[..at].chars().next_back();
            let found = self.literals.iter().find(|(literal, whole)| {
                text[at..].starts_with(literal.as_str())
                    && !(*whole && word(literal.chars().next()) && word(before))
                    && !(*whole && word(literal.chars().next_back()) && word(text[at + literal.len()..].chars().next()))
            });
            if let Some((literal, _)) = found {
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
            if let Some(kind) = secret(key, true) {
                redactor.add(value, kind);
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
                redactor.source(&text, !is_config_file(&path));
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
fn preview(goal: &str, prepared: &Prepared, managed: bool) -> String {
    let impact = match goal {
        "clean" => "CLEAN DROPS ALL OBJECTS in the schemas selected by the CLI.",
        "migrate" => "Migrate may change the database schema/data.",
        "repair" => "Repair changes Flyway schema history.",
        _ => "This explicit command may connect to the CLI-selected database.",
    };
    format!(
        "{impact}\nTarget database: {}\n\n{}\n\n{}\n{}",
        target_hint(&prepared.proof.files, &prepared.proof.args),
        prepared.provenance,
        if managed {
            "Runs in the background: follow it in Flyway Command Output with Refresh Status and Cancel. It stops if the window or plugin closes, or after about five idle minutes."
        } else {
            "Opens in the project's terminal; use the terminal to follow or stop it."
        },
        "Cancelling does not roll back the database. Check CLI and environment settings before proceeding."
    )
}
/// The confirmation's target hint for the status bar: `None` without a project Flyway config
/// file, `Some(None)` when the target is unknown, else the short `host/database` and full hint.
pub(crate) fn status_target(project: &Project) -> Option<Option<(String, String)>> {
    let prepared = prepare(project, "info", true).ok()?;
    let files = &prepared.proof.files;
    if !files.iter().any(|(path, observed)| matches!(observed, Observed::Source(_)) && crate::config::is_flyway_config(path)) {
        return None;
    }
    let hint = target_hint(files, &prepared.proof.args);
    if hint.starts_with("UNKNOWN") {
        return Some(None);
    }
    let url = hint.split(" (").next().unwrap_or_default();
    let short = url.split_once("://").map_or(url, |(_, rest)| rest).to_owned();
    Some(Some((short, hint)))
}
/// Best-known target from Flyway config files only, as a host/database hint without credentials or parameters.
fn target_hint(files: &[(String, Observed)], args: &[String]) -> String {
    if args.iter().any(|arg| arg.split_once('=').is_some_and(|(key, _)| key.to_ascii_lowercase().ends_with("url"))) {
        return "UNKNOWN (set by command-line arguments; value hidden)".into();
    }
    let mut urls = BTreeSet::new();
    for (path, observed) in files {
        let Observed::Source(text) = observed else { continue };
        if is_config_file(path) {
            continue;
        }
        urls.extend(config::settings(path, text).into_iter().filter(|(key, _)| key == "flyway.url").map(|(_, url)| url));
        if path.ends_with(".toml") {
            let mut table = "";
            for line in text.lines().map(|line| config::strip_comment(line).trim()) {
                if let Some(header) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
                    table = header.trim();
                } else if let Some((key, value)) = line.split_once('=').filter(|_| table.starts_with("environments.")) {
                    if key.trim() == "url" {
                        urls.insert(value.trim().trim_matches(['"', '\'']).to_owned());
                    }
                }
            }
        }
    }
    match urls.into_iter().collect::<Vec<_>>().as_slice() {
        [url] => safe_jdbc(url).map_or_else(
            || "UNKNOWN (the configured url is dynamic, holds credentials or is unsupported; value hidden)".into(),
            |hint| format!("{hint} (from project config files; CLI or environment settings can override it)"),
        ),
        [] => "UNKNOWN (no url in the project's Flyway config files)".into(),
        _ => "UNKNOWN (several urls are configured)".into(),
    }
}
fn safe_jdbc(value: &str) -> Option<String> {
    let base = value.split(['?', ';']).next()?;
    let (prefix, address) = ["jdbc:postgresql://", "jdbc:mysql://", "jdbc:mariadb://"].iter().find_map(|prefix| base.strip_prefix(prefix).map(|rest| (*prefix, rest)))?;
    let (authority, database) = address.split_once('/')?;
    if authority.is_empty() || database.is_empty() || base.len() > 1024
        || !authority.bytes().all(|b| b.is_ascii_alphanumeric() || b".-:[]".contains(&b))
        || !database.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b)) {
        return None;
    }
    Some(format!("{prefix}{authority}/{database} (parameters omitted)"))
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
        let preview = preview(goal, &prepared, managed);
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
                        _ => Err(Error::Failed("Command could not start; see Flyway Command Output.".into())),
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
    pub(crate) fn running(&self, root: &str) -> bool {
        self.session.as_ref().is_some_and(|session| session.root == root && session.child.is_some())
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
            session.status = format!("Process exited {status}; Refresh Status to read the remaining output.");
            return None;
        }
        session.status = format!(
            "Finished with exit {status} after {}s. Database outcome is not inferred from local source.",
            session.started.elapsed().as_secs()
        );
        session.child = None;
        if status == 0 && matches!(session.goal.as_str(), "migrate" | "repair" | "clean") {
            let refresh = openidea_plugin::NotificationAction::new("Refresh Info", "info");
            let _ = host::notify_with_actions(
                openidea_plugin::NotificationLevel::Info,
                "Flyway",
                &format!("Flyway {} finished. Refresh Info to see the database's migration state.", session.goal),
                &[refresh],
            );
        }
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
        session.status = "Cancelled. Database changes are not rolled back; the outcome is unknown.".into();
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
                "Command details (target not verified)",
                FieldValue::readonly_text(&session.provenance, true),
            ),
            Field::new(
                "execution-log",
                "Command output",
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
        .status("Output is kept until the plugin restarts · secrets are hidden · cancelling does not roll back the database")
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
    fn info_result_accepted_on_render() {
        let host = managed();
        let mut plugin = Flyway::default();
        host.answer_next_prompt(Some(&["yes"]));
        plugin.execution.start(&host.invocation(), "info", true, true).unwrap();
        plugin.render("migrations", None, &host.invocation()).unwrap();
        assert!(plugin.info_cache.is_none(), "still running");
        plugin.execution.session.as_ref().unwrap().child.as_ref().unwrap().kill().unwrap();
        plugin.render("migrations", None, &host.invocation()).unwrap();
        assert!(plugin.execution.session.as_ref().unwrap().child.is_none());
        assert!(plugin.info_cache.is_some(), "render polled the owned child and accepted its result");
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
        assert!(matches!(view.body,Body::Message(ref text) if text.contains("No local Flyway migrations")));
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
        assert!(plugin.execution.migration_status("/project").unwrap().contains("remaining output"));
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
            assert!(matches!(view.body,Body::Message(message) if message.contains("No local Flyway migrations")));
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
            assert!(!prepared.redactor.literals.iter().any(|(value, _)| value == "nested-only-secret"));
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
    fn short_user_value_not_redacted_in_words() {
        let host = managed();
        host.add_file("flyway.conf", "flyway.user=sa\nflyway.password=s3cretpw\nflyway.url=jdbc:postgresql://deploy:urlpass@db/app\n");
        host.add_file("src/main/resources/application.properties", "spring.flyway.token-validity=10\napp.user=bob-the-builder\nspring.datasource.username=dbadmin\nspring.datasource.password=ab\n");
        let redactor = prepare(&host.invocation().project().unwrap(), "validate", false).unwrap().redactor;
        assert_eq!(redactor.label("V10 sales report"), "V10 sales report", "user=sa and token-validity=10 are too short to redact");
        assert_eq!(redactor.label("bob-the-builder"), "bob-the-builder", "user outside datasource/flyway is not a credential");
        assert_eq!(redactor.label("x s3cretpw y"), "x [redacted] y");
        assert_eq!(redactor.label("s3cretpw_old s3cretpw123 Xs3cretpw"), "[redacted]_old [redacted]123 X[redacted]", "passwords are hidden even inside words");
        assert_eq!(redactor.label("cabinet"), "c[redacted]inet", "a password is hidden wherever it appears literally, at any length");
        assert_eq!(redactor.label("by dbadmin. dbadmins"), "by [redacted]. dbadmins", "a short user name is hidden as a whole token only");
        assert_eq!(redactor.label("urlpass deploy deployment"), "[redacted] [redacted] deployment", "URL userinfo: password as a substring, short user as a whole token");
        assert_eq!(redactor.label("see https://someone:hunter2@host/x"), "see https://[redacted]@host/x");
        let mut toml = Redactor::default();
        toml.source("[environments.dev]\nuser = \"deployer\"\npassword = \"pw\"\ntoken = \"0123456789abcdef\"\n", true);
        assert_eq!(toml.label("deployer pw"), "[redacted] [redacted]", "Flyway config users and short passwords stay secret");
        assert_eq!(toml.label("x0123456789abcdefy"), "x[redacted]y", "names of 8 or more characters are plain substrings");
        let mut one = Redactor::default();
        one.add("p", Secret::Always);
        assert_eq!(one.label("plan p"), "plan [redacted]", "a one-character secret is a whole token");
        let mut license = Redactor::default();
        license.source("flyway.licenseKey=FL01ABCDEF\n", false);
        assert_eq!(license.label("key FL01ABCDEF"), "key [redacted]");
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
        redactor.add("topsecret", Secret::Always);
        redactor.add("a", Secret::Always);
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
        assert!(execution.session.as_ref().unwrap().status.contains("rolled back"));
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
        let _host = host.with_capabilities(&["project.read", "events", "process:flyway", "storage"]);
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
    fn accepted_info_notes_migration_files_and_clears_them_when_stale() {
        let host = managed();
        host.add_file("src/main/resources/db/migration/V1__init.sql", "");
        host.add_file("src/main/resources/db/migration/V2__more.sql", "");
        host.add_file("src/main/resources/db/migration/V3__unknown.sql", "");
        let mut plugin = Flyway::default();
        let mut finished = finished_info(&host);
        finished.output.stdout = r#"{"migrations":[{"version":"1","description":"init","state":"Success","installedOnUTC":"2026-11-10 09:00"},{"version":"2","description":"more","state":"Pending"},{"version":"3","description":"unknown","state":"Odd"}]}"#.into();
        plugin.accept_finished(&host.invocation().project().unwrap(), finished).unwrap();
        let notes: Vec<_> = host.file_notes().into_iter().map(|((set, path), (note, _))| (set, path, note)).collect();
        assert_eq!(notes, [
            ("flyway-info".into(), "/project/src/main/resources/db/migration/V1__init.sql".into(), "applied 2026-11-10".into()),
            ("flyway-info".into(), "/project/src/main/resources/db/migration/V2__more.sql".into(), "pending".into()),
        ]);
        plugin.handle_events(&[Event::FileSaved("/project/src/main/resources/db/migration/V2__more.sql".into())], &host.invocation());
        assert!(host.file_notes().is_empty());
        plugin.accept_finished(&host.invocation().project().unwrap(), finished_info(&host)).unwrap();
        assert!(host.file_notes().is_empty(), "no Info rows, no notes");
    }
    #[test]
    fn status_item_follows_flyway_config_and_opens_the_panel() {
        let host = managed();
        let mut plugin = Flyway::default();
        plugin.run_command("show-migrations", &host.invocation(), None).unwrap();
        assert!(!host.status_items().contains_key("flyway-target"));
        host.add_file("flyway.toml", "[flyway]\nurl = \"jdbc:mysql://db.example/app\"\n");
        plugin.run_command("show-migrations", &host.invocation(), None).unwrap();
        let item = &host.status_items()["flyway-target"];
        assert_eq!(item.text, "Flyway: db.example/app");
        assert_eq!(item.command.as_deref(), Some("show-migrations"));
    }
    #[test]
    fn successful_managed_write_offers_refresh_info() {
        let host = managed();
        let mut execution = Execution::default();
        host.answer_next_prompt(Some(&["yes"]));
        execution.start(&host.invocation(), "migrate", false, false).unwrap();
        let session = execution.session.as_mut().unwrap();
        Execution::ingest(session, Chunk { status: Some(0), ..Default::default() });
        let sent = host.notifications_with_actions();
        assert_eq!(sent.len(), 1);
        assert_eq!((sent[0].actions[0].label.as_str(), sent[0].actions[0].command.as_str()), ("Refresh Info", "info"));
        host.answer_next_prompt(Some(&["yes"]));
        execution.start(&host.invocation(), "migrate", false, false).unwrap();
        Execution::ingest(execution.session.as_mut().unwrap(), Chunk { status: Some(1), ..Default::default() });
        assert_eq!(host.notifications_with_actions().len(), 1, "failures do not offer Refresh Info");
    }
    #[test]
    fn status_target_is_the_redacted_hint_only_with_flyway_config() {
        let host = managed();
        let project = host.invocation().project().unwrap();
        assert_eq!(status_target(&project), None);
        host.add_file("flyway.conf", "flyway.password=TOKEN_1\n");
        assert_eq!(status_target(&project), Some(None));
        host.add_file("flyway.conf", "flyway.url=jdbc:postgresql://db.example:5432/app?password=TOKEN_1\n");
        let (short, hint) = status_target(&project).unwrap().unwrap();
        assert_eq!(short, "db.example:5432/app");
        assert!(!hint.contains("TOKEN_1") && hint.starts_with("jdbc:postgresql://db.example:5432/app"));
    }
    #[test]
    fn confirmation_leads_with_impact_and_redacted_target_hint() {
        let host = managed();
        host.add_file("flyway.conf", "flyway.url=jdbc:postgresql://db.example:5432/app?password=TOKEN_1\nflyway.password=TOKEN_1\n");
        let prepared = prepare(&host.invocation().project().unwrap(), "clean", false).unwrap();
        let body = preview("clean", &prepared, true);
        assert!(body.starts_with("CLEAN DROPS ALL OBJECTS"), "{body}");
        assert!(body.lines().nth(1).unwrap().starts_with("Target database: jdbc:postgresql://db.example:5432/app (parameters omitted)"), "{body}");
        assert!(body.find("Target database").unwrap() < body.find("Invoking project").unwrap());
        assert!(!body.contains("TOKEN_1"), "{body}");
        let toml = vec![("/project/flyway.toml".to_owned(), Observed::Source("[environments.default]\nurl = \"jdbc:mysql://h/db\"\n".into()))];
        assert!(target_hint(&toml, &[]).starts_with("jdbc:mysql://h/db"));
        assert!(target_hint(&toml, &["validate".into(), "-url=jdbc:mysql://other/x".into()]).starts_with("UNKNOWN"));
        let credentials = vec![("/project/flyway.conf".to_owned(), Observed::Source("flyway.url=jdbc:postgresql://user:pw@h/db\n".into()))];
        assert!(target_hint(&credentials, &[]).starts_with("UNKNOWN") && !target_hint(&credentials, &[]).contains("pw"));
        assert!(target_hint(&[], &[]).starts_with("UNKNOWN"));
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
            redactor.source(&format!("password={value}"), true);
            assert!(redactor.suppress);
        }
    }
}
