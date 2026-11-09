//! One owned port-forward process, with pinned cluster scope and bounded output.
use super::*;
use openidea_plugin::Project;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Scope {
    config: Option<String>,
    context: String,
    namespace: String,
    identity: serde_json::Value,
}

impl Scope {
    pub(super) fn args(&self, rest: &[&str]) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(config) = &self.config {
            args.push(format!("--kubeconfig={config}"));
        }
        args.push(format!("--context={}", self.context));
        args.push(format!("--namespace={}", self.namespace));
        args.extend(rest.iter().map(|arg| (*arg).to_owned()));
        args
    }

    fn read(args: &[String]) -> Result<String, Error> { Self::read_in(args, None) }
    fn read_in(args: &[String], project: Option<&Project>) -> Result<String, Error> {
        let out = match project { Some(project) => project.run_process(KUBECTL, args, None)?, None => host::run_process(KUBECTL, args, None)? };
        if out.status != Some(0) {
            return Err(Error::Failed(format!(
                "Cannot resolve forwarding scope: {}",
                out.stderr.trim()
            )));
        }
        Ok(out.stdout.trim().to_owned())
    }

    pub(super) fn capture(context: Option<&str>, namespace: Option<&str>) -> Result<Self, Error> { Self::capture_in(context, namespace, None) }
    pub(super) fn capture_for_project(context: Option<&str>, namespace: Option<&str>, project: &Project) -> Result<Self, Error> {
        Self::capture_in(context, namespace, Some(project)).map_err(|error| match error {
            Error::Denied(_) | Error::Cancelled => error,
            _ => Error::Failed("Selected Kubernetes scope could not be inspected; raw tool errors withheld.".into()),
        })
    }
    fn capture_in(context: Option<&str>, namespace: Option<&str>, project: Option<&Project>) -> Result<Self, Error> {
        let config = config::kubeconfig();
        let mut args: Vec<String> = config
            .iter()
            .map(|path| format!("--kubeconfig={path}"))
            .collect();
        let context = match context {
            Some(context) => context.to_owned(),
            None => {
                args.extend(["config".into(), "current-context".into()]);
                Self::read_in(&args, project)?
            }
        };
        if context.is_empty() || context.len() > 4096 || context.chars().any(char::is_control) {
            return Err("Choose an identifiable Kubernetes context before forwarding.".into());
        }
        let mut scope = Self {
            config,
            context,
            namespace: String::new(),
            identity: serde_json::Value::Null,
        };
        // Read only the redacted, selected local configuration. Never request --raw,
        // retain user credentials, or send this metadata to a remote service.
        let mut args = scope.args(&["config", "view", "--minify", "-o", "json"]);
        args.retain(|arg| !arg.starts_with("--namespace="));
        let json = Self::read_in(&args, project)?;
        if json.len() > 128 << 10 {
            return Err("Selected kubeconfig metadata exceeds the inspection limit.".into());
        }
        let config: serde_json::Value = serde_json::from_str(&json).map_err(|_| {
            Error::Failed("Cannot inspect the selected context's cluster identity.".into())
        })?;
        let contexts = config["contexts"]
            .as_array()
            .filter(|a| a.len() == 1)
            .ok_or("The selected context is ambiguous.")?;
        let clusters = config["clusters"]
            .as_array()
            .filter(|a| a.len() == 1)
            .ok_or("The selected cluster is ambiguous.")?;
        let cluster_name = contexts[0]["context"]["cluster"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("The selected context has no cluster name.")?;
        if contexts[0]["name"].as_str() != Some(scope.context.as_str())
            || clusters[0]["name"].as_str() != Some(cluster_name)
        {
            return Err("The selected context and cluster do not match.".into());
        }
        let server = clusters[0]["cluster"]["server"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("The selected cluster has no server address.")?;
        if server.len() > 4096 || server.chars().any(char::is_control) {
            return Err("The selected server address is invalid.".into());
        }
        // Only noncredential identity fields are retained. TLS/proxy/CA metadata is
        // compared too, while auth material from users is deliberately excluded.
        let cluster = &clusters[0]["cluster"];
        scope.identity = serde_json::json!({"cluster":clusters[0]["name"],"server":server,"user":contexts[0]["context"]["user"],
            "tls":cluster["tls-server-name"],"insecure":cluster["insecure-skip-tls-verify"],"ca":cluster["certificate-authority"],"proxy":cluster["proxy-url"]});
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        scope.identity.to_string().hash(&mut fingerprint);
        scope.identity = fingerprint.finish().into();
        scope.namespace = namespace.map(str::to_owned).unwrap_or_else(|| {
            contexts[0]["context"]["namespace"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or("default")
                .to_owned()
        });
        if !is_dns_label(&scope.namespace) {
            return Err("Choose a valid namespace before forwarding.".into());
        }
        Ok(scope)
    }

    pub(super) fn configuration_current(&self) -> bool {
        self.config == config::kubeconfig()
    }
    #[cfg(test)]
    pub(super) fn test_log_scope() -> Self {
        Self {
            config: None,
            context: "dev".into(),
            namespace: "shop".into(),
            identity: serde_json::Value::Null,
        }
    }
    pub(super) fn label(&self) -> String {
        format!("{} / {}", self.context, self.namespace)
    }
}

pub(super) type Selection = (Option<String>, Option<String>, Option<(String, String)>);

pub(super) fn owner(ctx: Option<&Invocation>) -> Result<(Project, String), Error> {
    let project = ctx
        .and_then(Invocation::project)
        .ok_or("Scoped Kubernetes tools need an open project and project.read permission.")?;
    let root = project.root()?;
    let absolute = root.starts_with('/')
        || root.as_bytes().get(1) == Some(&b':') && root.as_bytes().get(2) == Some(&b'/');
    if !absolute
        || root.len() > 4096
        || root == "/"
        || root.len() == 3 && root.as_bytes().get(1) == Some(&b':')
        || root.contains(['\\'])
        || root.chars().any(char::is_control)
        || root.contains("//")
        || root.split('/').any(|s| matches!(s, "." | ".."))
    {
        return Err("Scoped Kubernetes tools need an identifiable concrete project root.".into());
    }
    Ok((project, root))
}

#[derive(Clone)]
pub(super) struct Draft {
    scope: Scope,
    selection: Selection,
    pub(super) owner_root: Option<String>,
    pub target: String,
    pub ports: String,
    kind: String,
    row: String,
}

#[derive(Clone)]
struct Spec {
    scope: Scope,
    owner_root: Option<String>,
    target: String,
    ports: Vec<String>,
    selection: Selection,
    kind: String,
    row: String,
}

/// Approval is deliberately inert until a separate token-bearing native event.
pub(super) struct Prepared {
    spec: Spec,
    token: String,
    created: std::time::Instant,
    restart: bool,
}
const APPROVAL_TTL: std::time::Duration = std::time::Duration::from_secs(60);

impl Spec {
    fn args(&self) -> Vec<String> {
        let mut rest = vec!["port-forward", self.target.as_str(), "--address=127.0.0.1"];
        rest.extend(self.ports.iter().map(String::as_str));
        self.scope.args(&rest)
    }
    fn label(&self) -> String {
        format!(
            "{} · {} · {}",
            self.scope.label(),
            self.target,
            self.ports.join(" ")
        )
    }
    fn preview(&self) -> String {
        format!(
            "Project: {}\n{}\n\nProgram: kubectl\nExact arguments: {:?}\n\nLoopback only. Approval only prepares this command; a separate Launch Reviewed button event starts it after checking that event’s project. Listener availability is not confirmed by spawning. Managed ownership is this plugin instance/project; idle reclaim (normally about five idle minutes), invoking window close or plugin stop ends its process. Observed root, namespace, context, selection or kubeconfig changes retire the managed session; restart then requires a fresh draft. A kubeconfig/credential change after the final check remains outside atomic control.",
            self.owner_root
                .as_deref()
                .unwrap_or("unbound terminal mode"),
            self.label(),
            self.args()
        )
    }
}

pub(super) struct Session {
    spec: Spec,
    child: Option<host::Child>,
    output: String,
    state: String,
    exit_status: Option<i32>,
    retired: bool,
}

impl Session {
    fn launch(spec: Spec, project: &Project) -> Self {
        let command = host::Command::new(KUBECTL).args(spec.args());
        match host::start(&command, Some(project)) {
            Ok(child) => Self {
                spec,
                child: Some(child),
                output: String::new(),
                state: "Running; listener availability not confirmed".into(),
                exit_status: None,
                retired: false,
            },
            Err(error) => Self {
                spec,
                child: None,
                output: error.to_string(),
                state: "Start failed".into(),
                exit_status: None,
                retired: false,
            },
        }
    }
    fn append(&mut self, text: &str) {
        self.output.push_str(text);
        const MAX: usize = 16 << 10;
        if self.output.len() > MAX {
            const MARKER: &str = "[Earlier process output omitted]\n";
            let mut at = self.output.len() - (MAX - MARKER.len());
            while !self.output.is_char_boundary(at) {
                at += 1;
            }
            self.output.drain(..at);
            self.output.insert_str(0, MARKER);
        }
    }
    fn poll(&mut self) {
        // Exit status and pending output are independent in API1.3. Drain a bounded
        // amount, retaining the handle for the next refresh if this batch is full.
        for _ in 0..4 {
            let Some(child) = &self.child else { return };
            match child.poll(4096) {
                Ok(chunk) => {
                    let empty = chunk.stdout.is_empty() && chunk.stderr.is_empty();
                    self.append(&chunk.stdout);
                    self.append(&chunk.stderr);
                    if let Some(status) = chunk.status {
                        self.exit_status = Some(status);
                        self.state =
                            format!("Exited with status {status}; draining available output");
                        if empty {
                            self.state = format!("Exited with status {status}");
                            self.child = None;
                            return;
                        }
                    } else {
                        return;
                    }
                }
                Err(error) => {
                    self.append(&format!("\n{error}"));
                    self.state = "Process polling failed; handle released".into();
                    self.child = None;
                    return;
                }
            }
        }
    }
    fn stop(&mut self) -> Result<(), Error> {
        if let Some(child) = &self.child {
            child.kill()?;
        }
        self.poll();
        self.child = None;
        self.state = "Stopped".into();
        Ok(())
    }
    pub fn running(&self) -> bool {
        self.child.is_some() && self.exit_status.is_none()
    }
    fn retire(&mut self, reason: &str) {
        if self.retired {
            return;
        }
        if let Some(child) = &self.child {
            if let Err(error) = child.kill() {
                self.append(&format!(
                    "\nStop request failed; releasing owned handle: {error}"
                ));
            }
        }
        self.poll();
        self.child = None; // Dropping Child releases/kills this handle only.
        self.retired = true;
        self.state = format!("Retired: {reason}; owned process handle released");
    }
}

fn ports(text: &str) -> Result<Vec<String>, Error> {
    if text.len() > 1024 {
        return Err("Too many port mappings.".into());
    }
    let mut result = Vec::new();
    let mut locals = BTreeSet::new();
    let mut mappings = BTreeSet::new();
    let number = |s: &str| {
        s.parse::<u16>()
            .ok()
            .filter(|n| *n > 0)
            .filter(|_| s.bytes().all(|b| b.is_ascii_digit()))
    };
    for token in text.split_whitespace() {
        let (local, remote) = token.split_once(':').unwrap_or((token, token));
        let Some(remote) = number(remote) else {
            return Err("Use numeric ports from 1 to 65535, as local:remote or :remote.".into());
        };
        let local = if local.is_empty() {
            String::new()
        } else {
            let Some(local) = number(local) else {
                return Err("Use numeric local ports from 1 to 65535.".into());
            };
            if !locals.insert(local) {
                return Err("A local port can be mapped only once.".into());
            }
            local.to_string()
        };
        let mapping = format!("{local}:{remote}");
        if !mappings.insert(mapping.clone()) {
            return Err("Duplicate port mapping.".into());
        }
        result.push(mapping);
        if result.len() > 16 {
            return Err("At most 16 port mappings are supported.".into());
        }
    }
    if result.is_empty() {
        return Err("Enter at least one port mapping.".into());
    }
    Ok(result)
}

impl KubeExplorer {
    pub(super) fn retire_forward(&mut self, reason: &str) {
        let changed = self.forward.is_some() || self.prepared_forward.is_some() || self.show_forward_session
            || self.forward_session.as_ref().is_some_and(|s| !s.retired);
        if let Some(session) = &mut self.forward_session {
            session.retire(reason);
        }
        self.forward = None;
        self.prepared_forward = None;
        self.show_forward_session = false;
        if changed { crate::refresh_panel(); }
    }
    pub(super) fn expire_forward_context(&mut self, ctx: Option<&Invocation>) {
        if self.forward_session.is_none()
            && self.prepared_forward.is_none()
            && self.forward.as_ref().is_none_or(|d| d.owner_root.is_none())
        {
            return;
        }
        let current = owner(ctx).ok().map(|(_, root)| root);
        if self.prepared_forward.as_ref().is_some_and(|p| {
            p.created.elapsed() >= APPROVAL_TTL
                || p.spec.owner_root != current
                || p.spec.selection != self.forward_selection()
                || p.spec.scope.config != config::kubeconfig()
        }) {
            self.prepared_forward = None;
            crate::refresh_panel();
        }
        let changed = self.forward_session.as_ref().is_some_and(|s| {
            !s.retired
                && (s.spec.owner_root != current
                    || s.spec.selection != self.forward_selection()
                    || s.spec.scope.config != config::kubeconfig())
        });
        if changed {
            self.retire_forward("Project, selection or Kubernetes scope changed");
        }
        if self
            .forward
            .as_ref()
            .is_some_and(|d| d.owner_root.is_some() && d.owner_root != current)
        {
            self.forward = None;
            self.show_forward_session = false;
            crate::refresh_panel();
        }
    }
    pub(super) fn forward_selection(&self) -> Selection {
        (
            self.context.clone(),
            self.namespace.clone(),
            self.selected.clone(),
        )
    }

    pub(super) fn prepare_forward(
        &self,
        kind: &str,
        row: &str,
        ctx: Option<&Invocation>,
    ) -> Result<Draft, Error> {
        if !FORWARDABLE.iter().any(|(allowed, _)| *allowed == kind) {
            return Err("This resource kind does not support the port-forward form.".into());
        }
        let (namespace, name) = self.split_scope(kind, row)?;
        if !is_dns_subdomain(&name) {
            return Err("Choose a valid resource name before forwarding.".into());
        }
        let binding = owner(ctx).ok();
        let scope = match &binding {
            Some((project,_)) => Scope::capture_for_project(self.context.as_deref(), namespace.as_deref(), project)?,
            None => Scope::capture(self.context.as_deref(), namespace.as_deref())?,
        };
        Ok(Draft {
            scope,
            selection: self.forward_selection(),
            owner_root: binding.map(|(_,root)|root),
            target: format!("{kind}/{name}"),
            ports: String::new(),
            kind: kind.into(),
            row: row.into(),
        })
    }

    fn check_forward(
        &self,
        scope: &Scope,
        selection: &Selection,
        kind: &str,
        row: &str,
        owner_root: Option<&str>,
        ctx: Option<&Invocation>,
    ) -> Result<(), Error> {
        let project = if let Some(root) = owner_root {
            let (project,current) = owner(ctx)?;
            if current != root { return Err(Error::Stale); }
            Some(project)
        } else { None };
        if &self.forward_selection() != selection {
            return Err(Error::Stale);
        }
        let (namespace, _) = self.split_scope(kind, row)?;
        let current = match &project {
            Some(project) => Scope::capture_for_project(self.context.as_deref(), namespace.as_deref(), project)?,
            None => Scope::capture(self.context.as_deref(), namespace.as_deref())?,
        };
        if &current != scope {
            return Err(Error::Stale);
        }
        Ok(())
    }

    pub(super) fn forward_ports(&self, draft: &Draft, jsonpath: &str) -> Result<String, Error> {
        let name = draft
            .target
            .split_once('/')
            .map(|(_, name)| name)
            .ok_or("Invalid forwarding target.")?;
        let args = draft.scope.args(&["get", &draft.kind, name, "-o", jsonpath, REQUEST_TIMEOUT]);
        if let Some(root) = &draft.owner_root {
            let project = host::projects()?.into_iter().find(|p|p.root().ok().as_deref()==Some(root)).ok_or(Error::Stale)?;
            Scope::read_in(&args,Some(&project)).map_err(|error|match error {
                Error::Denied(_) | Error::Cancelled => error,
                _ => Error::Failed("Forwarding resource metadata could not be inspected; raw tool errors withheld.".into()),
            })
        } else { Scope::read(&args) }
    }

    pub(super) fn start_terminal_forward(&mut self, ctx: Option<&Invocation>) -> Result<(), Error> {
        self.prepared_forward = None;
        let Some(draft) = self.forward.clone() else {
            return Ok(());
        };
        let spec = Spec {
            scope: draft.scope,
            owner_root: draft.owner_root,
            target: draft.target,
            ports: ports(&draft.ports)?,
            selection: draft.selection,
            kind: draft.kind,
            row: draft.row,
        };
        self.check_forward(
            &spec.scope,
            &spec.selection,
            &spec.kind,
            &spec.row,
            spec.owner_root.as_deref(),
            ctx,
        )?;
        if !ui_kit::prompts::confirm_exact(
            "Start forwarding in terminal?",
            &format!(
                "{}\n\nArguments: {:?}\n\nLoopback only. Stop it from the terminal; managed-session controls do not control this process.",
                spec.label(),
                spec.args()
            ),
        )? {
            return Ok(());
        }
        self.check_forward(
            &spec.scope,
            &spec.selection,
            &spec.kind,
            &spec.row,
            spec.owner_root.as_deref(),
            ctx,
        )?;
        host::open_terminal(
            &format!("Port Forward: {}", spec.target),
            KUBECTL,
            &spec.args(),
        )
    }

    fn stage_forward(&mut self, spec: Spec, restart: bool)->Result<(),Error> {
        self.ensure_panel_replace()?;
        self.clear_panel_data();
        self.retire_tree_callbacks();
        self.new_rich_callback();
        self.forward_nonce = self.forward_nonce.wrapping_add(1);
        // Correlation token, not an authorization or encrypted window identity.
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (
            self.forward_nonce,
            std::time::SystemTime::now(),
            spec.preview(),
        )
            .hash(&mut hash);
        self.prepared_forward = Some(Prepared {
            spec,
            token: format!(
                "launch-reviewed-forward-{:016x}-{:016x}",
                self.forward_nonce,
                hash.finish()
            ),
            created: std::time::Instant::now(),
            restart,
        });
        crate::refresh_views();
        Ok(())
    }

    pub(super) fn prepared_forward_view(&self) -> Option<View> {
        self.prepared_forward.as_ref().map(|p| View::message(&format!(
            "PREPARED ONLY: approval has not started or replaced a process.\n{}\n\nChoose the separate Launch button within 60 seconds. Its new invocation must match this reviewed project and scope. A switch after that event begins remains outside atomic API1.3 control.", p.spec.preview()))
            .button(Button::new(&p.token, if p.restart { "Launch Reviewed Restart" } else { "Launch Reviewed Forward" }))
            .button(Button::new("cancel-reviewed-forward", "Cancel Prepared Forward")))
    }

    fn commit_forward(&mut self, token: &str, ctx: Option<&Invocation>) -> Result<(), Error> {
        // Consume even on stale/error so approvals cannot be replayed.
        let pending = self.prepared_forward.take().ok_or(Error::Stale)?;
        if pending.token != token || pending.created.elapsed() >= APPROVAL_TTL {
            return Err(Error::Stale);
        }
        let spec = pending.spec;
        self.check_forward(
            &spec.scope,
            &spec.selection,
            &spec.kind,
            &spec.row,
            spec.owner_root.as_deref(),
            ctx,
        )?;
        if !pending.restart {
            let draft = self.forward.as_ref().ok_or(Error::Stale)?;
            if ports(&draft.ports)? != spec.ports || draft.target != spec.target {
                return Err(Error::Stale);
            }
        } else {
            let previous = self.forward_session.as_ref().ok_or(Error::Stale)?;
            if previous.retired || previous.running() || previous.spec.args() != spec.args() {
                return Err(Error::Stale);
            }
        }
        let (project, _) = owner(ctx)?;
        let history = if let Some(previous) = &mut self.forward_session {
            previous.stop()?;
            if previous.spec.owner_root == spec.owner_root {
                previous.output.clone()
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        let mut next = Session::launch(spec, &project);
        if !next.running() && !history.is_empty() {
            let failure = std::mem::take(&mut next.output);
            next.append(&format!(
                "[Previous owned session output]\n{history}\n[New start failure]\n{failure}"
            ));
        }
        self.retire_tree_callbacks();
        self.new_rich_callback();
        self.forward_session = Some(next);
        self.show_forward_session = true;
        crate::refresh_views();
        Ok(())
    }

    pub(super) fn start_managed_forward(&mut self, ctx: &Invocation) -> Result<(), Error> {
        self.prepared_forward = None;
        let Some(draft) = self.forward.clone() else {
            return Ok(());
        };
        let (_, root) = owner(Some(ctx))?;
        if draft.owner_root.as_deref() != Some(&root) {
            return Err(Error::Stale);
        }
        let ports = ports(&draft.ports)?;
        self.check_forward(
            &draft.scope,
            &draft.selection,
            &draft.kind,
            &draft.row,
            draft.owner_root.as_deref(),
            Some(ctx),
        )?;
        let spec = Spec {
            scope: draft.scope,
            owner_root: draft.owner_root,
            target: draft.target,
            ports,
            selection: draft.selection,
            kind: draft.kind,
            row: draft.row,
        };
        let replacing = self.forward_session.as_ref().is_some_and(Session::running);
        if !ui_kit::prompts::confirm_exact(
            "Review managed port forward?",
            &format!(
                "{}{}",
                spec.preview(),
                if replacing {
                    " The previous owned forwarding will be stopped first."
                } else {
                    ""
                }
            ),
        )? {
            return Ok(());
        }
        // ctx.project remains the original native call snapshot after the modal.
        // Do not launch, stop or claim a fresh active-project check in this call.
        self.stage_forward(spec, false)?;
        Ok(())
    }

    pub(super) fn forward_status(&mut self, ctx: Option<&Invocation>) -> View {
        self.expire_forward_context(ctx);
        let current = owner(ctx).ok().map(|(_, root)| root);
        if self
            .forward_session
            .as_ref()
            .is_some_and(|s| s.spec.owner_root != current || current.is_none())
        {
            return View::message("No owned forwarding session for this project. Open a project with project.read permission to manage its session.").button(Button::new("back", "Back"));
        }
        let selection = self.forward_selection();
        let Some(session) = &mut self.forward_session else {
            return View::message("No owned forwarding session.");
        };
        session.poll();
        View::message(&format!(
            "Project: {}\n{}\n{}\n\n{}\n\nCaptured scope, not continuously monitored. Only this plugin instance/project's process is controlled. Window close, scope change or idle reclamation ends it. Same-project windows share this instance; API1.3 exposes no stable window identity here.",
            session.spec.owner_root.as_deref().unwrap_or("unknown"),
            session.spec.label(),
            session.state,
            session.output
        ))
        .button(Button::new("back", "Back"))
        .button(Button::new("forward-status", "Refresh status"))
        .button(Button::new("stop-forward", "Stop").enabled(session.running()))
        .button(Button::new("restart-forward", "Restart").enabled(!session.running() && !session.retired && session.spec.selection == selection))
    }

    pub(super) fn forward_button(
        &mut self,
        button: &str,
        ctx: Option<&Invocation>,
    ) -> Result<bool, Error> {
        if button.starts_with("launch-reviewed-forward-") {
            self.expire_forward_context(ctx);
            self.commit_forward(button, ctx)?;
            return Ok(true);
        }
        if button == "cancel-reviewed-forward" {
            self.prepared_forward = None;
            crate::refresh_views();
            return Ok(true);
        }
        if !matches!(
            button,
            "forward-status" | "stop-forward" | "restart-forward"
        ) {
            return Ok(false);
        }
        self.expire_forward_context(ctx);
        if let Some(session) = &self.forward_session {
            if session.spec.owner_root.as_deref() != Some(owner(ctx)?.1.as_str()) {
                return Err(Error::Stale);
            }
        }
        match button {
            "forward-status" => {
                self.ensure_panel_replace()?;
                self.clear_panel_data();self.prepared_forward=None;self.new_rich_callback();
                if let Some(session) = &self.forward_session {
                    if !session.retired
                        && self
                            .check_forward(
                                &session.spec.scope,
                                &session.spec.selection,
                                &session.spec.kind,
                                &session.spec.row,
                                session.spec.owner_root.as_deref(),
                                ctx,
                            )
                            .is_err()
                    {
                        self.retire_forward("Pinned cluster scope could no longer be confirmed");
                    }
                }
                self.show_forward_session = true;
            }
            "stop-forward" => {
                self.prepared_forward = None;
                if let Some(session) = &mut self.forward_session {
                    session.stop()?;
                }
                self.show_forward_session = true;
            }
            "restart-forward" => {
                self.prepared_forward = None;
                let Some(session) = &self.forward_session else {
                    return Ok(true);
                };
                if session.retired {
                    return Err(Error::Stale);
                }
                if session.running() {
                    return Err("Stop the owned forwarding before restarting it.".into());
                }
                let spec = session.spec.clone();
                self.check_forward(
                    &spec.scope,
                    &spec.selection,
                    &spec.kind,
                    &spec.row,
                    spec.owner_root.as_deref(),
                    ctx,
                )?;
                if !ui_kit::prompts::confirm_exact("Review port forward restart?", &spec.preview())?
                {
                    return Ok(true);
                }
                self.stage_forward(spec, true)?;
            }
            _ => return Ok(false),
        }
        crate::refresh_views();
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn setup() -> (MockHost, KubeExplorer) {
        let host = MockHost::install().with_capabilities(&["process:kubectl", "project.read"]);
        host.on_process(KUBECTL, |_, _| Ok(openidea_plugin::ProcessOutput { status: Some(0), stdout: r#"{"contexts":[{"name":"dev","context":{"cluster":"local","namespace":"team-a"}}],"clusters":[{"name":"local","cluster":{"server":"https://dev.invalid"}}]}"#.into(), stderr: String::new() }));
        let mut kube = KubeExplorer {
            context: Some("dev".into()),
            namespace: Some("team-a".into()),
            selected: Some(("pods".into(), "web".into())),
            ..Default::default()
        };
        let mut draft = kube
            .prepare_forward("pods", "web", Some(&host.invocation()))
            .unwrap();
        draft.ports = "8080:80".into();
        kube.forward = Some(draft);
        kube.new_rich_callback();
        (host, kube)
    }
    fn start(host: &MockHost, kube: &mut KubeExplorer) {
        host.answer_next_prompt(Some(&[]));
        kube.start_managed_forward(&host.invocation()).unwrap();
        commit(host, kube);
    }
    fn commit(host: &MockHost, kube: &mut KubeExplorer) {
        let token = kube.prepared_forward.as_ref().unwrap().token.clone();
        kube.forward_button(&token, Some(&host.invocation()))
            .unwrap();
    }

    fn approve(host: &MockHost, kube: &mut KubeExplorer) -> String {
        host.answer_next_prompt(Some(&[]));
        kube.start_managed_forward(&host.invocation()).unwrap();
        kube.prepared_forward.as_ref().unwrap().token.clone()
    }

    #[test]
    fn panel_navigation_preserves_same_subject_child_but_retires_hidden_forward_review() {
        let (host,mut kube)=setup();start(&host,&mut kube);let ticket=approve(&host,&mut kube);
        let key=kube.test_resource_key("res:pods/web",&host.invocation());
        kube.run_command("resource-describe",&host.invocation(),Some(&key)).unwrap();
        assert!(kube.details.is_some()&&kube.prepared_forward.is_none());
        assert!(kube.forward_session.as_ref().unwrap().running());
        assert!(matches!(kube.handle(PANEL,ViewEvent::Button(ticket),&host.invocation()),Err(Error::Stale)));
        assert!(kube.forward_session.as_ref().unwrap().running());
    }
    #[test]
    fn old_tree_group_selection_cannot_indirectly_stop_a_new_forwarding_session() {
        let (host,mut kube)=setup();start(&host,&mut kube);
        let Body::Tree(items)=kube.render(TREE,None,&host.invocation()).unwrap().body else{panic!("tree")};
        let old=items.iter().find(|item|item.label.starts_with("Namespace:")).unwrap().key.clone();
        kube.forward.as_mut().unwrap().ports="9090:90".into();start(&host,&mut kube);
        let result=kube.handle(TREE,ViewEvent::Select(old),&host.invocation());
        assert!(kube.forward_session.as_ref().unwrap().running(),"old group selection stopped the replacement child");
        assert!(matches!(result,Err(Error::Stale)));
        assert!(matches!(kube.handle(TREE,ViewEvent::Select("ns".into()),&host.invocation()),Err(Error::Stale)));
        assert!(kube.forward_session.as_ref().unwrap().running());
        let Body::Tree(items)=kube.render(TREE,None,&host.invocation()).unwrap().body else{panic!("fresh tree")};
        let current=items.iter().find(|item|item.label.starts_with("Namespace:")).unwrap().key.clone();
        kube.handle(TREE,ViewEvent::Select(current),&host.invocation()).unwrap();
        assert!(!kube.forward_session.as_ref().unwrap().running(),"fresh selection keeps the existing retirement policy");
    }
    #[test]
    fn old_session_stop_callback_cannot_stop_a_new_same_owner_session() {
        let (host,mut kube)=setup();start(&host,&mut kube);
        let view=kube.render_tree_in(Some(&host.invocation())).unwrap();
        let old_stop=view.toolbar.iter().find(|button|button.label=="Stop").unwrap().id.clone();
        kube.forward.as_mut().unwrap().ports="9090:90".into();start(&host,&mut kube);
        assert!(kube.forward_session.as_ref().unwrap().running());
        assert!(matches!(kube.handle(PANEL,ViewEvent::Button(old_stop),&host.invocation()),Err(Error::Stale)));
        assert!(kube.forward_session.as_ref().unwrap().running());
        assert_eq!(kube.forward_session.as_ref().unwrap().child.as_ref().unwrap().poll(32).unwrap().status,None);
        let view=kube.render_tree_in(Some(&host.invocation())).unwrap();
        let current=view.toolbar.iter().find(|button|button.label=="Stop").unwrap().id.clone();
        kube.handle(PANEL,ViewEvent::Button(current),&host.invocation()).unwrap();
        assert!(!kube.forward_session.as_ref().unwrap().running());
    }
    #[test]
    fn old_forward_draft_fields_and_terminal_review_cannot_retarget_a_new_draft() {
        let (host,mut kube)=setup();
        host.on_process(KUBECTL,|args,_|Ok(openidea_plugin::ProcessOutput{status:Some(0),stderr:String::new(),stdout:if args.iter().any(|arg|arg.starts_with("jsonpath=")){"80".into()}else{r#"{"contexts":[{"name":"dev","context":{"cluster":"local","namespace":"team-a"}}],"clusters":[{"name":"local","cluster":{"server":"https://dev.invalid"}}]}"#.into()}}));
        let view=kube.render_tree_in(Some(&host.invocation())).unwrap();
        let Body::Form(fields)=view.body else{panic!("forward draft")};
        let old_ports=fields.iter().find(|field|field.label.starts_with("Ports")).unwrap().key.clone();
        let old_start=view.toolbar.iter().find(|button|button.label=="Start in Terminal").unwrap().id.clone();
        kube.start_port_forward("pods","web",Some(&host.invocation())).unwrap();let before=host.process_runs().len();
        assert!(matches!(kube.handle(PANEL,ViewEvent::Field(old_ports,"19090:80".into()),&host.invocation()),Err(Error::Stale)));
        assert!(matches!(kube.handle(PANEL,ViewEvent::Button(old_start),&host.invocation()),Err(Error::Stale)));
        assert_eq!(host.process_runs().len(),before);assert!(host.terminals().is_empty());
        kube.handle(PANEL,ViewEvent::Field(kube.test_control_key("ports"),"19090:80".into()),&host.invocation()).unwrap();
        host.answer_next_prompt(Some(&[]));
        kube.handle(PANEL,ViewEvent::Button(kube.test_control_key("start-forward")),&host.invocation()).unwrap();
        assert_eq!(host.terminals().len(),1);assert!(host.terminals()[0].args.iter().any(|arg|arg=="19090:80"));
    }
    #[test]
    fn approval_only_prepares_and_separate_token_event_launches_once() {
        let (host, mut kube) = setup();
        let token = approve(&host, &mut kube);
        assert!(token.len() <= 128);
        assert!(kube.forward_session.is_none());
        let view = kube.render(PANEL, None, &host.invocation()).unwrap();
        assert!(
            matches!(view.body, Body::Message(text) if text.contains("PREPARED ONLY") && text.contains("8080:80"))
        );
        assert!(
            view.toolbar
                .iter()
                .any(|b| b.id == token && b.label == "Launch Reviewed Forward")
        );
        kube.handle(PANEL, ViewEvent::Button(token.clone()), &host.invocation())
            .unwrap();
        assert!(kube.forward_session.as_ref().unwrap().running());
        assert!(kube.prepared_forward.is_none());
        assert!(matches!(
            kube.forward_button(&token, Some(&host.invocation())),
            Err(Error::Stale)
        ));
        assert!(kube.forward_session.as_ref().unwrap().running());
    }

    #[test]
    fn owned_relative_configuration_discovery_review_and_spawn_share_the_project_cwd() {
        let (host,mut kube)=setup();host.set_setting("kubeconfig","\"configs/relative-kubeconfig\"");
        let mut draft=kube.prepare_forward("pods","web",Some(&host.invocation())).unwrap();draft.ports="8080:80".into();
        kube.forward_ports(&draft,"jsonpath={.spec.containers[*].ports[*].containerPort}").unwrap();
        kube.forward=Some(draft);start(&host,&mut kube);
        let scoped:Vec<_>=host.process_runs().into_iter().filter(|r|r.args.iter().any(|a|a=="--kubeconfig=configs/relative-kubeconfig")).collect();
        assert!(scoped.len()>=4);assert!(scoped.iter().all(|r|r.cwd.as_deref()==Some("/project")),"{scoped:?}");
    }
    #[test]
    fn owned_port_discovery_refuses_closed_owner_without_global_fallback() {
        let (host,kube)=setup();let draft=kube.forward.as_ref().unwrap().clone();
        let before=host.process_runs().len();host.set_project_root(None);
        assert!(matches!(kube.forward_ports(&draft,"jsonpath={.spec.ports[*].port}"),Err(Error::Stale)));
        assert_eq!(host.process_runs().len(),before);
    }
    #[test]
    fn owned_port_discovery_withholds_raw_tool_errors_and_unbound_terminal_scope_stays_separate() {
        let (host,kube)=setup();let draft=kube.forward.as_ref().unwrap().clone();
        host.on_process(KUBECTL,|_,_|Ok(openidea_plugin::ProcessOutput{status:Some(1),stdout:String::new(),stderr:"credential-private-parser-error".into()}));
        let error=kube.forward_ports(&draft,"jsonpath={.spec.ports[*].port}").unwrap_err().to_string();
        assert!(!error.contains("credential-private"));
        assert_eq!(host.process_runs().last().unwrap().cwd.as_deref(),Some("/project"));
        let (host,kube)=setup();let draft=kube.prepare_forward("pods","web",None).unwrap();
        assert!(draft.owner_root.is_none());
        assert!(host.process_runs().last().unwrap().cwd.is_none(),"explicit unbound terminal mode retains its separate legacy inspection path");
    }
    #[test]
    fn separate_launch_invocation_with_different_project_refuses() {
        let (host, mut kube) = setup();
        let token = approve(&host, &mut kube);
        // This is a NEW event, not evidence that a native Invocation changes in a modal.
        host.set_project_root(Some("/other-project"));
        let fresh_event = host.invocation();
        assert!(matches!(
            kube.handle(PANEL, ViewEvent::Button(token), &fresh_event),
            Err(Error::Stale)
        ));
        assert!(kube.forward_session.is_none());
        assert!(kube.prepared_forward.is_none());
    }

    #[test]
    fn stale_prepare_event_cannot_replace_or_consume_current_prepared_ticket() {
        let (host,mut kube)=setup();let old=kube.test_control_key("start-managed-forward");
        let token=approve(&host,&mut kube);
        assert!(matches!(kube.handle(PANEL,ViewEvent::Button(old),&host.invocation()),Err(Error::Stale)));
        assert!(kube.forward_session.is_none());
        assert_eq!(kube.prepared_forward.as_ref().unwrap().token,token);
        kube.handle(PANEL,ViewEvent::Button(token),&host.invocation()).unwrap();
        assert!(kube.forward_session.as_ref().unwrap().running());
    }

    #[test]
    fn prepared_approval_expires_without_sleep_and_is_consumed() {
        let (host, mut kube) = setup();
        let token = approve(&host, &mut kube);
        kube.prepared_forward.as_mut().unwrap().created = std::time::Instant::now() - APPROVAL_TTL;
        assert!(matches!(
            kube.forward_button(&token, Some(&host.invocation())),
            Err(Error::Stale)
        ));
        assert!(kube.prepared_forward.is_none() && kube.forward_session.is_none());
    }

    #[test]
    fn cancellation_or_wrong_nonce_never_replaces_running_process() {
        let (host, mut kube) = setup();
        start(&host, &mut kube);
        let token = approve(&host, &mut kube);
        assert!(kube.forward_session.as_ref().unwrap().running());
        kube.forward_button("cancel-reviewed-forward", Some(&host.invocation()))
            .unwrap();
        assert!(
            kube.forward_button(&token, Some(&host.invocation()))
                .is_err()
        );
        assert!(kube.forward_session.as_ref().unwrap().running());
        let _ = approve(&host, &mut kube);
        assert!(
            kube.forward_button("launch-reviewed-forward-foreign", Some(&host.invocation()))
                .is_err()
        );
        assert!(kube.prepared_forward.is_none());
        assert!(kube.forward_session.as_ref().unwrap().running());
    }

    #[test]
    fn stale_draft_ports_event_cannot_change_current_prepared_command() {
        let (host,mut kube)=setup();let old=kube.test_control_key("ports");
        let token=approve(&host,&mut kube);
        assert!(matches!(kube.handle(PANEL,ViewEvent::Field(old,"9090:90".into()),&host.invocation()),Err(Error::Stale)));
        assert_eq!(kube.prepared_forward.as_ref().unwrap().token,token);
        assert_eq!(kube.forward.as_ref().unwrap().ports,"8080:80");
        assert!(kube.forward_session.is_none());
    }

    #[test]
    fn changed_fingerprint_consumes_approval_without_stopping_previous_handle() {
        let (host, mut kube) = setup();
        start(&host, &mut kube);
        let token = approve(&host, &mut kube);
        host.on_process(KUBECTL, |_, _| Ok(openidea_plugin::ProcessOutput {
            status: Some(0), stderr: String::new(),
            stdout: r#"{"contexts":[{"name":"dev","context":{"cluster":"other","namespace":"team-a"}}],"clusters":[{"name":"other","cluster":{"server":"https://other.invalid"}}]}"#.into(),
        }));
        assert!(matches!(
            kube.forward_button(&token, Some(&host.invocation())),
            Err(Error::Stale)
        ));
        assert!(kube.prepared_forward.is_none());
        assert!(kube.forward_session.as_ref().unwrap().running());
    }

    #[test]
    fn restart_is_staged_and_requires_its_own_fresh_launch_event() {
        let (host, mut kube) = setup();
        start(&host, &mut kube);
        kube.forward_button("stop-forward", Some(&host.invocation()))
            .unwrap();
        host.answer_next_prompt(Some(&[]));
        kube.forward_button("restart-forward", Some(&host.invocation()))
            .unwrap();
        assert_eq!(kube.forward_session.as_ref().unwrap().state, "Stopped");
        let view = kube.prepared_forward_view().unwrap();
        let token = view
            .toolbar
            .iter()
            .find(|b| b.label == "Launch Reviewed Restart")
            .unwrap()
            .id
            .clone();
        kube.forward_button(&token, Some(&host.invocation()))
            .unwrap();
        assert!(kube.forward_session.as_ref().unwrap().running());
    }

    #[test]
    fn replacing_prepare_changes_nonce_and_deactivation_invalidates_it() {
        let (host, mut kube) = setup();
        let first = approve(&host, &mut kube);
        let second = approve(&host, &mut kube);
        assert_ne!(first, second);
        kube.deactivate();
        assert!(kube.prepared_forward.is_none());
        assert!(
            kube.forward_button(&second, Some(&host.invocation()))
                .is_err()
        );
        assert!(kube.forward_session.is_none());
    }

    #[test]
    fn services_parent_collapse_preserves_current_session_and_reviewed_launch() {
        let (host, mut kube) = setup();
        start(&host, &mut kube);
        let token = approve(&host, &mut kube);
        let before = host.process_runs().len();
        kube.handle(
            TREE,
            ViewEvent::Collapse(kube.test_node_key("kube:cluster",&host.invocation())),
            &host.invocation(),
        )
        .unwrap();
        kube.handle(
            TREE,
            ViewEvent::Expand(kube.test_node_key("kube:cluster",&host.invocation())),
            &host.invocation(),
        )
        .unwrap();
        assert!(kube.forward_session.as_ref().unwrap().running());
        assert_eq!(kube.prepared_forward.as_ref().unwrap().token, token);
        assert_eq!(host.process_runs().len(), before);
        commit(&host, &mut kube);
        assert!(kube.forward_session.as_ref().unwrap().running());
    }

    #[test]
    fn services_selection_retirement_invalidates_open_packaged_panel() {
        let (host, mut kube) = setup();
        start(&host, &mut kube);
        let before = host.view_messages().len();
        kube.handle(TREE, ViewEvent::Select(kube.test_node_key("ns",&host.invocation())), &host.invocation()).unwrap();
        assert!(kube.forward_session.as_ref().unwrap().retired);
        let emitted = host.view_messages();
        assert!(emitted[before..].iter().any(|(view, text)| view == PANEL && text == r#"{"channel":"ui-kit-refresh"}"#));
        assert!(emitted[before..].iter().all(|(view, _)| view == PANEL), "invalidate only the owned resource panel");
        assert!(host.refreshed_views().iter().any(|view| view == PANEL));
        let panel = kube.render(PANEL, None, &host.invocation()).unwrap();
        assert!(!matches!(panel.body, Body::Message(text) if text.contains("Running;")));
    }

    #[test]
    fn stable_packaged_render_has_no_invalidation_feedback_loop() {
        let (host, mut kube) = setup();
        start(&host, &mut kube);
        for sequence in [1, 2] {
            let before=host.view_messages().len();
            let request=serde_json::json!({"channel":"ui-kit","frameId":"0123456789abcdef0123456789abcdef","sequence":sequence,"project":"/project","op":"render"});
            kube.handle(PANEL,ViewEvent::Message(request.to_string()),&host.invocation()).unwrap();
            let messages=host.view_messages();
            assert_eq!(messages.len(),before+1,"stable render must only reply, not invalidate again");
            let reply:serde_json::Value=serde_json::from_str(&messages[before].1).unwrap();
            assert_eq!(reply["channel"],"ui-kit");assert!(reply.get("model").is_some());
        }
    }

    #[test]
    fn native_services_button_opening_draft_invalidates_existing_panel() {
        let (host, mut kube) = setup();
        host.on_process(KUBECTL, |args, _| Ok(openidea_plugin::ProcessOutput {
            status:Some(0),stderr:String::new(),stdout:if args.iter().any(|a|a.starts_with("jsonpath=")) {"80".into()} else {r#"{"contexts":[{"name":"dev","context":{"cluster":"local","namespace":"team-a"}}],"clusters":[{"name":"local","cluster":{"server":"https://dev.invalid"}}]}"#.into()}
        }));
        kube.handle(TREE,ViewEvent::Select(kube.test_resource_key("res:pods/web",&host.invocation())),&host.invocation()).unwrap();
        let before=host.view_messages().len();
        kube.handle(TREE,ViewEvent::Button(kube.test_control_key("port-forward")),&host.invocation()).unwrap();
        assert!(host.view_messages()[before..].iter().any(|(view,text)|view==PANEL && text==r#"{"channel":"ui-kit-refresh"}"#));
        assert!(matches!(kube.render(PANEL,None,&host.invocation()).unwrap().body,Body::Form(_)));
    }

    #[test]
    fn mappings_are_numeric_bounded_and_cannot_inject_flags() {
        assert_eq!(
            ports("8080:80 443 :9090").unwrap(),
            ["8080:80", "443:443", ":9090"]
        );
        for bad in [
            "",
            "--address=0.0.0.0",
            "80 --address=0.0.0.0",
            "0:80",
            "80:0",
            "65536:80",
            "80:65536",
            "80:80:90",
            "80:80 080:90",
            ":80 :80",
            "80:http",
        ] {
            assert!(ports(bad).is_err(), "{bad}");
        }
        assert!(
            ports(
                &(1..=17)
                    .map(|n| format!("{n}:80"))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
            .is_err()
        );
    }

    #[test]
    fn cancelled_start_and_replacement_do_not_spawn_or_stop_owned_session() {
        let (host, mut kube) = setup();
        kube.start_managed_forward(&host.invocation()).unwrap();
        assert!(kube.forward_session.is_none());
        start(&host, &mut kube);
        let first = kube.forward_session.as_ref().unwrap().spec.args();
        kube.forward.as_mut().unwrap().ports = "9090:90".into();
        kube.start_managed_forward(&host.invocation()).unwrap();
        let session = kube.forward_session.as_ref().unwrap();
        assert_eq!(session.spec.args(), first);
        assert!(session.running());
        assert_eq!(
            session.child.as_ref().unwrap().poll(32).unwrap().status,
            None
        );
    }

    #[test]
    fn lifecycle_is_pinned_loopback_only_and_handles_stop_restart_and_exit() {
        let (host, mut kube) = setup();
        start(&host, &mut kube);
        assert_eq!(
            kube.forward_session.as_ref().unwrap().spec.args(),
            [
                "--context=dev",
                "--namespace=team-a",
                "port-forward",
                "pods/web",
                "--address=127.0.0.1",
                "8080:80"
            ]
        );
        assert!(
            matches!(kube.forward_status(Some(&host.invocation())).body,Body::Message(text) if text.contains("availability not confirmed"))
        );
        kube.forward_button("stop-forward", Some(&host.invocation()))
            .unwrap();
        assert!(!kube.forward_session.as_ref().unwrap().running());
        assert_eq!(kube.forward_session.as_ref().unwrap().state, "Stopped");
        host.answer_next_prompt(Some(&[]));
        kube.forward_button("restart-forward", Some(&host.invocation()))
            .unwrap();
        commit(&host, &mut kube);
        let session = kube.forward_session.as_mut().unwrap();
        assert!(session.running());
        session.child.as_ref().unwrap().kill().unwrap();
        session.poll();
        assert!(!session.running());
        assert!(session.state.contains("Exited with status -1"));
    }

    #[test]
    fn stale_selection_and_namespace_prevent_start_and_restart() {
        let (host, mut kube) = setup();
        kube.namespace = Some("other".into());
        assert!(matches!(
            kube.start_managed_forward(&host.invocation()),
            Err(Error::Stale)
        ));
        assert!(kube.forward_session.is_none());
        kube.namespace = Some("team-a".into());
        start(&host, &mut kube);
        kube.forward_button("stop-forward", Some(&host.invocation()))
            .unwrap();
        kube.context = Some("prod".into());
        host.answer_next_prompt(Some(&[]));
        assert!(matches!(
            kube.forward_button("restart-forward", Some(&host.invocation())),
            Err(Error::Stale)
        ));
        assert!(!kube.forward_session.as_ref().unwrap().running());
    }

    #[test]
    fn changes_to_implicit_context_or_kubeconfig_invalidate_drafts() {
        use std::{cell::RefCell, rc::Rc};
        let host =
            MockHost::install().with_capabilities(&["process:kubectl", "storage", "project.read"]);
        let current = Rc::new(RefCell::new("dev".to_owned()));
        let seen = current.clone();
        host.on_process(KUBECTL, move |args, _| {
            Ok(openidea_plugin::ProcessOutput {
                status: Some(0),
                stdout: if args.iter().any(|a| a == "current-context") { seen.borrow().clone() } else { serde_json::json!({"contexts":[{"name":seen.borrow().as_str(),"context":{"cluster":"local","namespace":"team-a"}}],"clusters":[{"name":"local","cluster":{"server":"https://dev.invalid"}}]}).to_string() },
                stderr: String::new(),
            })
        });
        let mut kube = KubeExplorer {
            selected: Some(("pods".into(), "web".into())),
            ..Default::default()
        };
        let mut draft = kube
            .prepare_forward("pods", "web", Some(&host.invocation()))
            .unwrap();
        draft.ports = "8080:80".into();
        kube.forward = Some(draft);
        *current.borrow_mut() = "prod".into();
        assert!(matches!(
            kube.start_managed_forward(&host.invocation()),
            Err(Error::Stale)
        ));
        assert!(kube.forward_session.is_none());
        *current.borrow_mut() = "dev".into();
        host.set_setting("kubeconfig", "\"/other/config\"");
        assert!(matches!(
            kube.start_managed_forward(&host.invocation()),
            Err(Error::Stale)
        ));
        assert!(kube.forward_session.is_none());
    }

    #[test]
    fn output_is_bounded_and_permission_failure_is_not_running() {
        let (host, mut kube) = setup();
        start(&host, &mut kube);
        let session = kube.forward_session.as_mut().unwrap();
        session.append(&"🦀".repeat(100_000));
        assert!(session.output.len() < 17 << 10);
        assert!(
            session
                .output
                .starts_with("[Earlier process output omitted]")
        );
        drop(kube);
        drop(host);
        let (host, mut kube) = setup();
        let host = host.with_capabilities(&[]);
        host.answer_next_prompt(Some(&[]));
        assert!(matches!(
            kube.start_managed_forward(&host.invocation()),
            Err(Error::Denied(_))
        ));
        assert!(kube.forward_session.is_none());
    }

    #[test]
    fn same_context_and_path_cannot_silently_change_cluster_server() {
        use std::{cell::RefCell, rc::Rc};
        let (host, mut kube) = setup();
        let server = Rc::new(RefCell::new("https://other.invalid".to_owned()));
        let value = server.clone();
        host.on_process(KUBECTL, move |_, _| {
            Ok(openidea_plugin::ProcessOutput {
                status: Some(0),
                stderr: String::new(),
                stdout: serde_json::json!({
                    "contexts":[{"name":"dev","context":{"cluster":"local","namespace":"team-a"}}],
                    "clusters":[{"name":"local","cluster":{"server":value.borrow().as_str()}}]
                })
                .to_string(),
            })
        });
        assert!(matches!(
            kube.start_managed_forward(&host.invocation()),
            Err(Error::Stale)
        ));
        assert!(kube.forward_session.is_none());
        assert!(matches!(
            kube.start_terminal_forward(Some(&host.invocation())),
            Err(Error::Stale)
        ));
        assert!(host.terminals().is_empty());
    }

    #[test]
    fn malformed_cluster_identity_cannot_become_a_forwarding_scope() {
        let (host, mut kube) = setup();
        host.on_process(KUBECTL, |_, _| {
            Ok(openidea_plugin::ProcessOutput {
                status: Some(0),
                stderr: String::new(),
                stdout: r#"{"contexts":[{"name":"dev","context":{}}],"clusters":[{"cluster":{"server":"https://dev.invalid"}}]}"#.into(),
            })
        });
        assert!(
            kube.start_terminal_forward(Some(&host.invocation()))
                .is_err()
        );
        assert!(kube.start_managed_forward(&host.invocation()).is_err());
        assert!(host.terminals().is_empty());
        assert!(kube.forward_session.is_none());
    }

    #[test]
    fn terminal_mode_remains_available_with_pinned_scope_and_confirmation() {
        let (host, mut kube) = setup();
        kube.start_terminal_forward(Some(&host.invocation()))
            .unwrap();
        assert!(host.terminals().is_empty());
        host.answer_next_prompt(Some(&[]));
        kube.start_terminal_forward(Some(&host.invocation()))
            .unwrap();
        assert_eq!(
            host.terminals()[0].args,
            [
                "--context=dev",
                "--namespace=team-a",
                "port-forward",
                "pods/web",
                "--address=127.0.0.1",
                "8080:80"
            ]
        );
        assert!(kube.forward_session.is_none());
        let form = kube.render_forward("pods/web", "8080:80");
        assert!(
            form.toolbar
                .iter()
                .any(|b| b.id == "start-forward" && b.label == "Start in Terminal")
        );
        assert!(form.toolbar.iter().any(|b| b.id == "start-managed-forward"));
        assert!(matches!(form.body, Body::Form(fields) if fields.len() == 2));
    }

    #[test]
    fn project_switch_retires_handle_and_hides_previous_output() {
        let (host, mut kube) = setup();
        start(&host, &mut kube);
        kube.forward_session
            .as_mut()
            .unwrap()
            .append("private previous project output");
        let unrelated = host::start(&host::Command::new(KUBECTL), None).unwrap();
        host.set_project_root(Some("/different-project"));
        let view = kube.forward_status(Some(&host.invocation()));
        assert!(
            matches!(view.body, Body::Message(text) if !text.contains("private previous project output"))
        );
        let session = kube.forward_session.as_ref().unwrap();
        assert!(session.retired && session.child.is_none());
        assert!(kube.forward.is_none());
        unrelated.write("still owned elsewhere").unwrap();
        assert_eq!(unrelated.poll(4096).unwrap().status, None);
        assert!(
            kube.forward_button("restart-forward", Some(&host.invocation()))
                .is_err()
        );
    }

    #[test]
    fn unbound_or_changed_project_cannot_launch_managed_draft() {
        let (host, mut kube) = setup();
        host.set_project_root(Some("/different-project"));
        host.answer_next_prompt(Some(&[]));
        assert!(matches!(
            kube.start_managed_forward(&host.invocation()),
            Err(Error::Stale)
        ));
        assert!(kube.forward_session.is_none());
        host.set_project_root(None);
        let mut draft = kube
            .prepare_forward("pods", "web", Some(&host.invocation()))
            .unwrap();
        draft.ports = "8080:80".into();
        kube.forward = Some(draft);
        assert!(kube.start_managed_forward(&host.invocation()).is_err());
        host.answer_next_prompt(Some(&[]));
        kube.start_terminal_forward(Some(&host.invocation()))
            .unwrap();
        assert_eq!(host.terminals().len(), 1);
    }

    #[test]
    fn actual_selection_event_retires_session_and_cannot_revive_it() {
        let (host, mut kube) = setup();
        start(&host, &mut kube);
        kube.handle(
            TREE,
            ViewEvent::Select(kube.test_resource_key("res:pods/other", &host.invocation())),
            &host.invocation(),
        )
        .unwrap();
        assert!(kube.forward_session.as_ref().unwrap().retired);
        kube.handle(
            TREE,
            ViewEvent::Select(kube.test_resource_key("res:pods/web", &host.invocation())),
            &host.invocation(),
        )
        .unwrap();
        assert!(matches!(
            kube.forward_button("restart-forward", Some(&host.invocation())),
            Err(Error::Stale)
        ));
        assert!(kube.forward_session.as_ref().unwrap().child.is_none());
    }

    #[test]
    fn context_and_namespace_activation_retire_managed_lease() {
        for key in ["ctx/prod", "ns/other"] {
            let (host, mut kube) = setup();
            start(&host, &mut kube);
            kube.handle(TREE, ViewEvent::Activate(kube.test_choice_key(key,&host.invocation())), &host.invocation())
                .unwrap();
            let session = kube.forward_session.as_ref().unwrap();
            assert!(session.retired && session.child.is_none(), "{key}");
            assert!(kube.forward.is_none());
        }
    }

    #[test]
    fn malformed_roots_never_become_managed_owners() {
        let (host, _) = setup();
        for root in [
            "",
            "/",
            "relative",
            "/root/../other",
            "/root//child",
            "C:/",
            "/root\nchild",
            "C:\\project",
        ] {
            host.set_project_root(Some(root));
            assert!(owner(Some(&host.invocation())).is_err(), "{root:?}");
        }
        for root in ["/owned/project", "C:/owned/project"] {
            host.set_project_root(Some(root));
            assert_eq!(owner(Some(&host.invocation())).unwrap().1, root);
        }
    }

    #[test]
    fn config_setting_and_actor_stop_release_owned_processes() {
        let (host, mut kube) = setup();
        start(&host, &mut kube);
        host.set_setting("kubeconfig", "\"/other/config\"");
        kube.expire_forward_context(Some(&host.invocation()));
        assert!(kube.forward_session.as_ref().unwrap().retired);
        assert!(kube.forward_session.as_ref().unwrap().child.is_none());
        host.set_setting("kubeconfig", "\"\"");
        let mut draft = kube
            .prepare_forward("pods", "web", Some(&host.invocation()))
            .unwrap();
        draft.ports = "8080:80".into();
        kube.forward = Some(draft);
        start(&host, &mut kube);
        kube.deactivate();
        assert!(kube.forward_session.as_ref().unwrap().retired);
        let children: Vec<_> = (0..8)
            .map(|_| host::start(&host::Command::new(KUBECTL), None).unwrap())
            .collect();
        assert_eq!(children.len(), 8);
    }

    #[test]
    fn exited_process_keeps_handle_until_bounded_available_output_drains() {
        let (host, mut kube) = setup();
        start(&host, &mut kube);
        let session = kube.forward_session.as_mut().unwrap();
        let child = session.child.as_ref().unwrap();
        child
            .write(&format!("{}TAIL", "🦀".repeat(30_000)))
            .unwrap();
        child.kill().unwrap();
        session.poll();
        assert!(!session.running());
        assert!(session.child.is_some());
        for _ in 0..16 {
            session.poll();
        }
        assert!(session.child.is_none());
        assert!(session.output.ends_with("TAIL"));
        assert!(session.output.len() <= 16 << 10);
        assert!(
            session
                .output
                .starts_with("[Earlier process output omitted]")
        );
    }

    #[test]
    fn retired_previous_project_does_not_erase_new_owned_draft() {
        let (host, mut kube) = setup();
        start(&host, &mut kube);
        host.set_project_root(Some("/new-project"));
        kube.expire_forward_context(Some(&host.invocation()));
        let mut draft = kube
            .prepare_forward("pods", "web", Some(&host.invocation()))
            .unwrap();
        draft.ports = "9090:90".into();
        kube.forward = Some(draft);
        kube.expire_forward_context(Some(&host.invocation()));
        assert_eq!(
            kube.forward.as_ref().unwrap().owner_root.as_deref(),
            Some("/new-project")
        );
        start(&host, &mut kube);
        assert_eq!(
            kube.forward_session
                .as_ref()
                .unwrap()
                .spec
                .owner_root
                .as_deref(),
            Some("/new-project")
        );
    }

    #[test]
    fn changed_cluster_mapping_retires_only_on_explicit_scope_refresh() {
        let (host, mut kube) = setup();
        start(&host, &mut kube);
        host.on_process(KUBECTL, |_, _| Ok(openidea_plugin::ProcessOutput {
            status: Some(0), stderr: String::new(),
            stdout: r#"{"contexts":[{"name":"dev","context":{"cluster":"other","namespace":"team-a"}}],"clusters":[{"name":"other","cluster":{"server":"https://other.invalid"}}]}"#.into(),
        }));
        let before = host.process_runs().len();
        kube.forward_status(Some(&host.invocation()));
        assert_eq!(
            host.process_runs().len(),
            before,
            "render must not discover cluster scope"
        );
        assert!(!kube.forward_session.as_ref().unwrap().retired);
        kube.forward_button("forward-status", Some(&host.invocation()))
            .unwrap();
        assert!(kube.forward_session.as_ref().unwrap().retired);
        assert!(kube.forward_session.as_ref().unwrap().child.is_none());
    }

    #[test]
    fn failed_new_project_launch_does_not_copy_other_project_history() {
        let (host, mut kube) = setup();
        start(&host, &mut kube);
        kube.forward_session
            .as_mut()
            .unwrap()
            .append("old project private history");
        host.set_project_root(Some("/new-project"));
        kube.expire_forward_context(Some(&host.invocation()));
        let mut draft = kube
            .prepare_forward("pods", "web", Some(&host.invocation()))
            .unwrap();
        draft.ports = "9090:90".into();
        kube.forward = Some(draft);
        // Fill the host's owned handle slots so only launch fails, after approval.
        let _other: Vec<_> = (0..8)
            .map(|_| host::start(&host::Command::new(KUBECTL), None).unwrap())
            .collect();
        start(&host, &mut kube);
        let session = kube.forward_session.as_ref().unwrap();
        assert_eq!(session.state, "Start failed");
        assert!(!session.output.contains("old project private history"));
    }

    #[test]
    fn full_preview_identifies_owner_and_exact_argv_without_readiness_claim() {
        let (host, mut kube) = setup();
        start(&host, &mut kube);
        let session = kube.forward_session.as_ref().unwrap();
        let preview = session.spec.preview();
        for expected in [
            "Project:",
            "Program: kubectl",
            "--context=dev",
            "--namespace=team-a",
            "pods/web",
            "127.0.0.1",
            "8080:80",
            "idle reclaim",
            "not confirmed",
        ] {
            assert!(preview.contains(expected), "{expected}");
        }
        assert!(
            kube.prepare_forward("secrets", "cert", Some(&host.invocation()))
                .is_err()
        );
    }

    #[test]
    fn dropping_the_instance_releases_the_owned_process_slot() {
        let (host, mut kube) = setup();
        start(&host, &mut kube);
        drop(kube);
        let cmd = host::Command::new(KUBECTL);
        let children: Vec<_> = (0..8).map(|_| host::start(&cmd, None).unwrap()).collect();
        assert_eq!(children.len(), 8);
    }
}
