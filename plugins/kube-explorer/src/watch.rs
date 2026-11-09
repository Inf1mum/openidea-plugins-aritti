//! Explicit instance-owned LIST→WATCH; positive observations, never complete inventory.
use crate::{ALL_NAMESPACES, config, forward};
use openidea_plugin::host::{Child, Chunk, Command};
use openidea_plugin::views::{Body, Button, Field, FieldValue, View, ViewEvent};
use openidea_plugin::{Error, Invocation, PickItem, host};
use serde_json::Value;
use std::collections::BTreeMap;
use std::hash::BuildHasher;
use std::time::{Duration, Instant};
pub(crate) const VIEW: &str = "resource-watch";
const MAX_EVENT: usize = 64 << 10;
const MAX_PENDING: usize = 128 << 10;
const MAX_RESOURCES: usize = 128;
const TTL: Duration = Duration::from_secs(60);
const KINDS: &[(&str, &str, &str)] = &[
    ("pods", "Pod", "v1"),
    ("services", "Service", "v1"),
    ("deployments", "Deployment", "apps/v1"),
];
type Choices = (Option<String>, Option<String>, Option<String>);
fn choices(context: &Option<String>, namespace: &Option<String>) -> Choices {
    (context.clone(), namespace.clone(), config::kubeconfig())
}
fn capture(ctx: &Invocation, context: Option<&str>, namespace: Option<&str>) -> Result<forward::Scope, Error> {
    let (project, _) = forward::owner(Some(ctx))?;
    forward::Scope::capture_for_project(context, namespace, &project).map_err(|error| match error {
        Error::Denied(_) | Error::Cancelled => error,
        _ => Error::Failed("Selected Kubernetes scope could not be inspected; raw tool errors are withheld.".into()),
    })
}
#[derive(Clone)]
struct Spec {
    root: String,
    kind: usize,
    scope: forward::Scope,
    choices: Choices,
}
impl Spec {
    fn args(&self) -> Vec<String> {
        self.scope.args(&[
            "get",
            KINDS[self.kind].0,
            "--watch",
            "--output-watch-events",
            "-o",
            "json",
            "--request-timeout=10m",
        ])
    }
    fn label(&self) -> String {
        format!("{} · {}", self.scope.label(), KINDS[self.kind].0)
    }
}
struct Pending {
    spec: Spec,
    at: Instant,
}
struct History {
    label: String,
    names: Vec<String>,
    since: Instant,
}
struct Session {
    spec: Spec,
    child: Option<Child>,
    pending: String,
    names: BTreeMap<String, String>,
    history: Option<History>,
    status: String,
    events: u64,
    started: Instant,
    exit: Option<i32>,
    stderr_seen: bool,
}
impl Session {
    fn retire(&mut self, status: &str) {
        self.child = None;
        self.pending.clear();
        self.status = format!("{status} Last accepted metadata is historical, not a completeness/absence proof.");
    }
    fn apply(&mut self, value: &Value) -> Result<(), &'static str> {
        let event = value
            .get("type")
            .and_then(Value::as_str)
            .ok_or("Unsupported watch envelope; last accepted observations retained.")?;
        if event == "ERROR" {
            return Err(if value["object"]["code"] == 410 {
                "Watch history expired (410); historical observations retained. Restart must re-LIST; no automatic reconnect."
            } else {
                "Watch server error; historical observations retained. Restart requires fresh review."
            });
        }
        let object = &value["object"];
        let (_, kind, api) = KINDS[self.spec.kind];
        if object["kind"] != kind || object["apiVersion"] != api {
            return Err("Watch object kind/API mismatch; observations retained.");
        }
        let version = object["metadata"]["resourceVersion"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control));
        if version.is_none() {
            return Err("Missing/bounded opaque resourceVersion required; observations retained.");
        }
        if event == "BOOKMARK" {
            self.events = self.events.saturating_add(1);
            return Ok(());
        }
        if !["ADDED", "MODIFIED", "DELETED"].contains(&event) {
            return Err("Unknown watch event type; observations retained.");
        }
        let name = object["metadata"]["name"]
            .as_str()
            .filter(|n| crate::is_dns_subdomain(n))
            .ok_or("Invalid watch resource name; observations retained.")?;
        let uid = object["metadata"]["uid"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 128 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
            .ok_or("Invalid watch UID; observations retained.")?;
        if object["metadata"]["namespace"].as_str() != self.spec.args().iter().find_map(|arg| arg.strip_prefix("--namespace=")) {
            return Err("Watch namespace mismatch; observations retained.");
        }
        if event == "DELETED" {
            if self.names.get(name).is_some_and(|known| known == uid) {
                self.names.remove(name);
            }
        } else {
            if !self.names.contains_key(name) && self.names.len() >= MAX_RESOURCES {
                return Err("128-resource observation cap reached; historical observations retained.");
            }
            self.names.insert(name.into(), uid.into());
        }
        self.events = self.events.saturating_add(1);
        Ok(())
    }
    fn ingest(&mut self, chunk: Chunk) -> Result<bool, &'static str> {
        self.stderr_seen |= !chunk.stderr.is_empty();
        if self.pending.len().saturating_add(chunk.stdout.len()) > MAX_PENDING {
            return Err("Watch pending JSON cap reached; observations retained.");
        }
        self.pending.push_str(&chunk.stdout);
        if !stream_depth(&self.pending) {
            return Err("Watch JSON nesting exceeds the structural limit; observations retained.");
        }
        loop {
            let trimmed = self.pending.trim_start();
            if trimmed.len() != self.pending.len() {
                self.pending = trimmed.to_owned();
            }
            if self.pending.is_empty() {
                break;
            }
            let mut stream = serde_json::Deserializer::from_str(&self.pending).into_iter::<Value>();
            match stream.next() {
                Some(Ok(value)) => {
                    let used = stream.byte_offset();
                    drop(stream);
                    if used > MAX_EVENT {
                        return Err("Watch event exceeds 64 KiB; observations retained.");
                    }
                    // Reuse bounded duplicate/alias/tag/depth/node rejection; JSON
                    // syntax was established above. Never display raw object data.
                    crate::kubeconfig_import::bounded_structure(&self.pending[..used])
                        .map_err(|_| "Watch event has duplicate/unsupported/bounded structure; observations retained.")?;
                    self.pending.drain(..used);
                    self.apply(&value)?;
                }
                Some(Err(error)) if error.is_eof() => break,
                Some(Err(_)) => return Err("Malformed watch JSON; observations retained."),
                None => break,
            }
        }
        if self.pending.len() > MAX_EVENT {
            return Err("Incomplete watch event exceeds 64 KiB; observations retained.");
        }
        if let Some(exit) = chunk.status {
            self.exit = Some(exit);
            self.status = format!("Exited {exit}; draining available output manually. Initial completeness remains unknown.");
        }
        if self.exit.is_some() && chunk.stdout.is_empty() && chunk.stderr.is_empty() {
            self.status = if self.pending.is_empty() {
                format!(
                    "Exited {}; observations are historical, not complete inventory.",
                    self.exit.unwrap()
                )
            } else {
                "Exited with incomplete JSON; last accepted observations are historical.".into()
            };
            self.child = None;
            self.pending.clear();
            return Ok(true);
        }
        Ok(false)
    }
    fn poll(&mut self) {
        for _ in 0..4 {
            let Some(child) = self.child.as_ref() else { return };
            let chunk = match child.poll(16 << 10) {
                Ok(chunk) => chunk,
                Err(_) => {
                    self.retire("Watch polling failed; owned handle released; observations historical.");
                    return;
                }
            };
            let empty = chunk.stdout.is_empty() && chunk.stderr.is_empty();
            match self.ingest(chunk) {
                Ok(true) => return,
                Err(message) => {
                    self.retire(message);
                    return;
                }
                _ => {}
            }
            if empty {
                return;
            }
        }
        if self.exit.is_none() {
            self.status = "Running; bounded manual drain reached. More output may remain; no loss-free backpressure claim.".into();
        }
    }
}
// Bound recursive serde_json allocation before parsing, including incomplete
// events. Braces inside escaped strings do not change structural depth.
fn stream_depth(text: &str) -> bool {
    let mut stack = Vec::new();
    let mut quoted = false;
    let mut escaped = false;
    for byte in text.bytes() {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b'{' | b'[' => {
                    stack.push(byte);
                    if stack.len() > 16 {
                        return false;
                    }
                }
                b'}' if stack.pop() != Some(b'{') => return false,
                b']' if stack.pop() != Some(b'[') => return false,
                _ => {}
            }
        }
    }
    true
}
#[derive(Default)]
pub(crate) struct Watch {
    pending: Option<Pending>,
    session: Option<Session>,
    token: String,
    serial: u64,
}
impl Watch {
    fn rotate(&mut self) {
        self.serial = self.serial.wrapping_add(1);
        let r = std::collections::hash_map::RandomState::new();
        self.token = format!(
            "{:016x}{:016x}",
            r.hash_one((self.serial, "watch")),
            r.hash_one((self.serial, "controls"))
        );
    }
    pub(crate) fn clear(&mut self) {
        self.pending = None;
        self.session = None;
        self.rotate();
    }
    pub(crate) fn settings_changed(&mut self, context: &Option<String>, namespace: &Option<String>) {
        let current = choices(context, namespace);
        let mut changed = false;
        if self.pending.as_ref().is_some_and(|p| p.spec.choices != current) {
            self.pending = None;
            changed = true;
        }
        if let Some(session) = &mut self.session {
            if session.spec.choices != current && session.child.is_some() {
                session.retire("Scope/settings changed; handle released, observations historical. Restart requires fresh review.");
                changed = true;
            }
        }
        if changed {
            self.rotate();
            host::refresh_view(VIEW);
        }
    }
    pub(crate) fn closed_roots(&mut self) {
        if self.pending.is_none() && self.session.is_none() {
            return;
        }
        let Ok(projects) = host::projects() else {
            self.clear();
            host::refresh_view(VIEW);
            return;
        };
        let open = |root: &str| projects.iter().any(|p| p.root().ok().as_deref() == Some(root));
        let mut changed = false;
        if self.pending.as_ref().is_some_and(|p| !open(&p.spec.root)) {
            self.pending = None;
            changed = true;
        }
        if self.session.as_ref().is_some_and(|s| !open(&s.spec.root)) {
            self.session = None;
            changed = true;
        }
        if changed {
            self.rotate();
            host::refresh_view(VIEW);
        }
    }
    pub(crate) fn prepare(
        &mut self,
        ctx: &Invocation,
        context: &Option<String>,
        namespace: &Option<String>,
        restart: bool,
    ) -> Result<(), Error> {
        self.closed_roots();
        let (_, root) = forward::owner(Some(ctx))?;
        if namespace.as_deref() == Some(ALL_NAMESPACES) {
            return Err("Select one namespace before watching.".into());
        }
        if self.session.as_ref().is_some_and(|s| s.child.is_some() && s.spec.root != root) {
            return Err("An owned watch is running in another project; stop it from that project first.".into());
        }
        self.settings_changed(context, namespace);
        self.pending = None;
        self.rotate();
        let kind = if restart {
            self.session.as_ref().filter(|s| s.spec.root == root).ok_or(Error::Stale)?.spec.kind
        } else {
            let items: Vec<_> = KINDS
                .iter()
                .enumerate()
                .map(|(i, (name, _, _))| PickItem::new(&i.to_string(), name))
                .collect();
            host::pick("Watch resources in the selected namespace", &items)?
                .ok_or(Error::Cancelled)?
                .parse::<usize>()
                .ok()
                .filter(|i| *i < KINDS.len())
                .ok_or(Error::Stale)?
        };
        let scope = capture(ctx, context.as_deref(), namespace.as_deref())?;
        let spec = Spec {
            root,
            kind,
            scope,
            choices: choices(context, namespace),
        };
        let preview = format!(
            "Project: {}\nScope: {}\nProgram: kubectl\nExact argv: {:?}\n\nApproval only prepares; separate Launch Reviewed Watch uses a fresh project invocation. This read-only command may authenticate/connect. kubectl owns LIST→WATCH. Initial completeness is UNKNOWN; only positive name/UID observations are kept, never authoritative Services inventory or absence. Manual bounded polling, 128 observations/64 KiB event; caps/errors preserve dated accepted metadata. Raw object/stderr values are not displayed. Stop releases only its Child; Restart re-LISTs after new review. Instance idle reclaim (~5 minutes), window/plugin stop may terminate it before the 10-minute request timeout. No durable job, automatic reconnect, keepalive or loss-free buffering claim.",
            spec.root,
            spec.label(),
            spec.args()
        );
        if !ui_kit::prompts::confirm_exact("Review managed resource watch?", &preview)? {
            host::refresh_view(VIEW);
            return Ok(());
        }
        if forward::owner(Some(ctx))?.1 != spec.root
            || choices(context, namespace) != spec.choices
            || capture(ctx, context.as_deref(), namespace.as_deref())? != spec.scope
        {
            return Err(Error::Stale);
        }
        self.pending = Some(Pending { spec, at: Instant::now() });
        self.rotate();
        host::refresh_view(VIEW);
        host::reveal_view(VIEW)
    }
    pub(crate) fn view(&mut self, ctx: &Invocation, context: &Option<String>, namespace: &Option<String>) -> Result<View, Error> {
        self.closed_roots();
        let Ok((_, root)) = forward::owner(Some(ctx)) else {
            return Ok(View::message(
                "Open the owning project with project.read to inspect its resource watch.",
            ));
        };
        let belongs =
            self.pending.as_ref().is_some_and(|p| p.spec.root == root) || self.session.as_ref().is_some_and(|s| s.spec.root == root);
        if !belongs {
            return Ok(View::message(
                "No resource watch for this project. Run Kube Explorer: Review Managed Resource Watch… explicitly.",
            ));
        }
        self.settings_changed(context, namespace);
        let key = |name: &str| format!("{name}@{}", self.token);
        let mut fields = vec![];
        let mut view = View::new(Body::Message(String::new()));
        if let Some(p) = self.pending.as_ref().filter(|p| p.spec.root == root) {
            fields.push(Field::new(
                "prepared",
                "Reviewed watch",
                FieldValue::readonly_text(
                    &format!(
                        "{}\n{}\n{:?}\nApproval expires after 60 seconds; no process started by approval.",
                        p.spec.root,
                        p.spec.label(),
                        p.spec.args()
                    ),
                    true,
                ),
            ));
            view = view
                .button(Button::new(&key("launch"), "Launch Reviewed Watch").enabled(p.at.elapsed() < TTL))
                .button(Button::new(&key("cancel"), "Cancel Review"));
        }
        if let Some(s) = self.session.as_ref().filter(|s| s.spec.root == root) {
            fields.push(Field::new(
                "scope",
                "Captured scope",
                FieldValue::readonly_text(
                    &format!(
                        "{}\n{}\n{}s old; events accepted {}",
                        s.spec.root,
                        s.spec.label(),
                        s.started.elapsed().as_secs(),
                        s.events
                    ),
                    true,
                ),
            ));
            fields.push(Field::new(
                "status",
                "Status",
                FieldValue::readonly_text(
                    &format!(
                        "{}{}",
                        s.status,
                        if s.stderr_seen {
                            "\nStderr received; raw values withheld."
                        } else {
                            ""
                        }
                    ),
                    true,
                ),
            ));
            let names = s.names.keys().take(32).cloned().collect::<Vec<_>>().join("\n");
            fields.push(Field::new(
                "observed",
                "Positive observed names (initial completeness UNKNOWN)",
                FieldValue::readonly_text(
                    &format!(
                        "Showing up to 32 of {} observed names, cap 128. Never an absence proof.\n{names}",
                        s.names.len()
                    ),
                    true,
                ),
            ));
            if let Some(h) = &s.history {
                fields.push(Field::new(
                    "history",
                    "Previous session historical observations (not merged)",
                    FieldValue::readonly_text(
                        &format!(
                            "{} · {}s since prior watch start\n{}",
                            h.label,
                            h.since.elapsed().as_secs(),
                            h.names.iter().take(16).cloned().collect::<Vec<_>>().join("\n")
                        ),
                        true,
                    ),
                ));
            }
            view = view
                .button(Button::new(&key("poll"), "Refresh Watch Status").enabled(s.child.is_some()))
                .button(Button::new(&key("stop"), "Stop Owned Watch").enabled(s.child.is_some()))
                .button(Button::new(&key("restart"), "Review Restart…"));
        }
        view.body = Body::Form(fields);
        Ok(view.status("Instance-owned read-only watch · initial completeness unknown · manual polling · no background durability"))
    }
    pub(crate) fn handle(
        &mut self,
        event: ViewEvent,
        ctx: &Invocation,
        context: &Option<String>,
        namespace: &Option<String>,
    ) -> Result<(), Error> {
        self.closed_roots();
        let ViewEvent::Button(id) = event else { return Err(Error::Stale) };
        let (action, token) = id.rsplit_once('@').ok_or(Error::Stale)?;
        let (_, root) = forward::owner(Some(ctx))?;
        if token != self.token
            || !(self.pending.as_ref().is_some_and(|p| p.spec.root == root) || self.session.as_ref().is_some_and(|s| s.spec.root == root))
        {
            return Err(Error::Stale);
        }
        self.settings_changed(context, namespace);
        if token != self.token {
            return Err(Error::Stale);
        }
        let result = (|| {
            match action {
                "cancel" => self.pending = None,
                "launch" => {
                    let p = self.pending.take().ok_or(Error::Stale)?;
                    if p.at.elapsed() >= TTL
                        || p.spec.root != root
                        || p.spec.choices != choices(context, namespace)
                        || capture(ctx, context.as_deref(), namespace.as_deref())? != p.spec.scope
                    {
                        return Err(Error::Stale);
                    }
                    let (project, fresh_root) = forward::owner(Some(ctx))?;
                    if fresh_root != root
                        || p.at.elapsed() >= TTL
                        || !p.spec.scope.configuration_current()
                        || p.spec.choices != choices(context, namespace)
                        || self.session.as_ref().is_some_and(|s| s.child.is_some() && s.spec.root != root)
                    {
                        return Err(Error::Stale);
                    }
                    let child = host::start(&Command::new("kubectl").args(&p.spec.args()), Some(&project))?;
                    child.close_stdin()?;
                    let history = self.session.as_ref().filter(|s| s.spec.root == root).and_then(|s| {
                        if s.names.is_empty() {
                            s.history.as_ref().map(|h| History {
                                label: h.label.clone(),
                                names: h.names.clone(),
                                since: h.since,
                            })
                        } else {
                            Some(History {
                                label: format!("{}; previous session, historical only", s.spec.label()),
                                names: s.names.keys().cloned().collect(),
                                since: s.started,
                            })
                        }
                    });
                    self.session = Some(Session {
                        spec: p.spec,
                        child: Some(child),
                        pending: String::new(),
                        names: BTreeMap::new(),
                        history,
                        status: "Running; refresh manually. Initial completeness UNKNOWN.".into(),
                        events: 0,
                        started: Instant::now(),
                        exit: None,
                        stderr_seen: false,
                    });
                }
                "poll" => {
                    let s = self.session.as_mut().filter(|s| s.spec.root == root).ok_or(Error::Stale)?;
                    if capture(ctx, context.as_deref(), namespace.as_deref()).ok().as_ref() != Some(&s.spec.scope) {
                        s.retire("Scope could not be revalidated; handle released, observations historical.");
                        return Err(Error::Stale);
                    }
                    s.poll();
                }
                "stop" => {
                    let s = self.session.as_mut().filter(|s| s.spec.root == root).ok_or(Error::Stale)?;
                    if let Some(child) = &s.child {
                        child.kill()?;
                    }
                    s.retire("Stopped owned watch; pending output discarded; accepted observations historical.");
                    self.pending = None;
                }
                "restart" => return self.prepare(ctx, context, namespace, true),
                _ => return Err(Error::Stale),
            }
            Ok(())
        })();
        self.rotate();
        host::refresh_view(VIEW);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KubeExplorer;
    use openidea_plugin::{Plugin, ProcessOutput, mock::MockHost, views::Views};
    use serde_json::json;
    fn setup() -> (MockHost, KubeExplorer) {
        let host = MockHost::install().with_capabilities(&["project.read", "process:kubectl", "storage"]);
        host.on_process("kubectl", |args,_| {
            assert!(args.iter().any(|a|a=="config"), "watch itself must use managed start, not synchronous run_process");
            Ok(ProcessOutput {status:Some(0),stdout:r#"{"contexts":[{"name":"dev","context":{"cluster":"local","namespace":"shop"}}],"clusters":[{"name":"local","cluster":{"server":"https://dev.invalid"}}]}"#.into(),stderr:String::new()})
        });
        (
            host,
            KubeExplorer {
                context: Some("dev".into()),
                namespace: Some("shop".into()),
                ..Default::default()
            },
        )
    }
    fn prepare(k: &mut KubeExplorer, h: &MockHost) {
        h.answer_next_prompt(Some(&["0"]));
        h.answer_next_prompt(Some(&[]));
        k.run_command("watch-resources", &h.invocation(), None).unwrap();
    }
    fn press(k: &mut KubeExplorer, h: &MockHost, action: &str) -> Result<(), Error> {
        let id = format!("{action}@{}", k.watch.token);
        k.handle(VIEW, ViewEvent::Button(id), &h.invocation())
    }
    fn launch(k: &mut KubeExplorer, h: &MockHost) {
        prepare(k, h);
        press(k, h, "launch").unwrap();
    }
    fn event(kind: &str, name: &str, uid: &str) -> String {
        json!({"type":kind,"object":{"apiVersion":"v1","kind":"Pod","metadata":{"name":name,"uid":uid,"namespace":"shop","resourceVersion":"opaque/rv-1"},"spec":{"ignoredFloat":0.5,"sensitive":"must-not-display"}}}).to_string()
    }
    fn ingest(k: &mut KubeExplorer, output: &str) -> Result<bool, &'static str> {
        k.watch.session.as_mut().unwrap().ingest(Chunk {
            stdout: output.into(),
            ..Default::default()
        })
    }
    #[test]
    fn public_review_is_inert_launch_is_fresh_scoped_single_use_and_render_never_polls() {
        let (h, mut k) = setup();
        prepare(&mut k, &h);
        assert!(k.watch.session.is_none());
        assert!(k.watch.pending.is_some());
        let token = k.watch.token.clone();
        let before = h.process_runs().len();
        k.render(VIEW, None, &h.invocation()).unwrap();
        assert_eq!(h.process_runs().len(), before);
        assert!(k.watch.pending.as_ref().unwrap().spec.args().ends_with(&[
            "get".into(),
            "pods".into(),
            "--watch".into(),
            "--output-watch-events".into(),
            "-o".into(),
            "json".into(),
            "--request-timeout=10m".into()
        ]));
        assert!(k.watch.pending.as_ref().unwrap().spec.args().contains(&"--namespace=shop".into()));
        assert!(
            !k.watch
                .pending
                .as_ref()
                .unwrap()
                .spec
                .args()
                .iter()
                .any(|a| a.starts_with("--resource-version"))
        );
        press(&mut k, &h, "launch").unwrap();
        assert!(k.watch.pending.is_none());
        let session = k.watch.session.as_ref().unwrap();
        assert!(session.child.is_some());
        assert!(
            session.child.as_ref().unwrap().write("not accepted").is_err(),
            "managed stdin is closed"
        );
        let before = h.process_runs().len();
        k.render(VIEW, None, &h.invocation()).unwrap();
        assert_eq!(h.process_runs().len(), before);
        assert!(
            k.handle(VIEW, ViewEvent::Button(format!("launch@{token}")), &h.invocation())
                .is_err()
        );
    }
    #[test]
    fn cancel_expiry_foreign_root_and_changed_local_identity_cannot_launch() {
        let (h, mut k) = setup();
        h.answer_next_prompt(Some(&["0"]));
        h.answer_next_prompt(None);
        k.run_command("watch-resources", &h.invocation(), None).unwrap();
        assert!(k.watch.pending.is_none());
        prepare(&mut k, &h);
        let token = k.watch.token.clone();
        h.add_project("/project");
        h.set_project_root(Some("/other"));
        assert!(
            k.handle(VIEW, ViewEvent::Button(format!("launch@{token}")), &h.invocation())
                .is_err()
        );
        assert!(k.watch.session.is_none());
        h.set_project_root(Some("/project"));
        k.watch.pending.as_mut().unwrap().at = Instant::now() - TTL;
        assert!(press(&mut k, &h, "launch").is_err());
        assert!(k.watch.pending.is_none());
        prepare(&mut k, &h);
        h.on_process("kubectl",|_,_|Ok(ProcessOutput{status:Some(0),stdout:r#"{"contexts":[{"name":"dev","context":{"cluster":"local","namespace":"shop"}}],"clusters":[{"name":"local","cluster":{"server":"https://changed.invalid"}}]}"#.into(),stderr:String::new()}));
        assert!(press(&mut k, &h, "launch").is_err());
        assert!(k.watch.session.is_none());
    }
    #[test]
    fn fragmented_concatenated_events_keep_only_metadata_and_uid_deletion_is_safe() {
        let (h, mut k) = setup();
        launch(&mut k, &h);
        let raw = event("ADDED", "web", "uid-a");
        let split = raw.len() / 2;
        ingest(&mut k, &raw[..split]).unwrap();
        assert!(k.watch.session.as_ref().unwrap().names.is_empty());
        ingest(&mut k, &format!("{}\n{}", &raw[split..], event("ADDED", "other", "uid-b"))).unwrap();
        assert_eq!(k.watch.session.as_ref().unwrap().names.len(), 2);
        ingest(&mut k, &event("ADDED", "web", "uid-new")).unwrap();
        ingest(&mut k, &event("DELETED", "web", "uid-a")).unwrap();
        assert_eq!(
            k.watch.session.as_ref().unwrap().names.get("web").map(String::as_str),
            Some("uid-new")
        );
        let view = k.render(VIEW, None, &h.invocation()).unwrap();
        let text = format!("{:?}", view.body);
        assert!(!text.contains("must-not-display"));
        assert!(text.contains("UNKNOWN"));
        ingest(&mut k, &event("DELETED", "web", "uid-new")).unwrap();
        assert!(!k.watch.session.as_ref().unwrap().names.contains_key("web"));
    }
    #[test]
    fn namespace_kind_duplicate_and_caps_refuse_events_without_losing_accepted_names() {
        let (h, mut k) = setup();
        launch(&mut k, &h);
        ingest(&mut k, &event("ADDED", "good", "uid-a")).unwrap();
        for raw in [
            event("ADDED", "bad", "uid-b").replace("\"shop\"", "\"foreign\""),
            event("ADDED", "bad", "uid-b").replace("\"Pod\"", "\"Service\""),
            event("ADDED", "bad", "uid-b").replace("\"type\":\"ADDED\"", "\"type\":\"ADDED\",\"type\":\"DELETED\""),
        ] {
            k.watch.session.as_mut().unwrap().pending.clear();
            assert!(ingest(&mut k, &raw).is_err());
            assert!(k.watch.session.as_ref().unwrap().names.contains_key("good"));
        }
        k.watch.session.as_mut().unwrap().pending.clear();
        assert!(ingest(&mut k, &"x".repeat(MAX_PENDING + 1)).is_err());
        k.watch.session.as_mut().unwrap().pending.clear();
        for n in 1..MAX_RESOURCES {
            ingest(&mut k, &event("ADDED", &format!("pod-{n}"), &format!("uid-{n}"))).unwrap();
        }
        assert!(ingest(&mut k, &event("ADDED", "overflow", "uid-overflow")).is_err());
        assert_eq!(k.watch.session.as_ref().unwrap().names.len(), MAX_RESOURCES);
    }
    #[test]
    fn bookmarks_410_and_exit_draining_never_claim_complete_inventory() {
        let (h, mut k) = setup();
        launch(&mut k, &h);
        ingest(&mut k, &event("ADDED", "good", "uid-a")).unwrap();
        ingest(
            &mut k,
            r#"{"type":"BOOKMARK","object":{"kind":"Pod","apiVersion":"v1","metadata":{"resourceVersion":"opaque/not-numeric"}}}"#,
        )
        .unwrap();
        assert!(
            ingest(
                &mut k,
                r#"{"type":"ERROR","object":{"kind":"Status","code":410,"message":"private-server-message"}}"#
            )
            .unwrap_err()
            .contains("410")
        );
        let s = k.watch.session.as_mut().unwrap();
        assert!(s.names.contains_key("good"));
        s.pending.clear();
        assert!(
            !s.ingest(Chunk {
                stdout: event("MODIFIED", "good", "uid-a"),
                status: Some(0),
                ..Default::default()
            })
            .unwrap()
        );
        assert!(s.child.is_some());
        assert!(
            s.ingest(Chunk {
                status: Some(0),
                ..Default::default()
            })
            .unwrap()
        );
        assert!(s.child.is_none());
        assert!(s.status.contains("historical"));
        assert!(!format!("{:?}", k.render(VIEW, None, &h.invocation()).unwrap().body).contains("private-server-message"));
    }
    #[test]
    fn stop_releases_only_owned_child_restart_requires_new_review_and_preserves_separate_history() {
        let (h, mut k) = setup();
        launch(&mut k, &h);
        ingest(&mut k, &event("ADDED", "old", "uid-old")).unwrap();
        let spare = host::start(&Command::new("kubectl"), None).unwrap();
        press(&mut k, &h, "stop").unwrap();
        assert!(k.watch.session.as_ref().unwrap().child.is_none());
        assert!(spare.poll(4096).unwrap().status.is_none());
        h.answer_next_prompt(None);
        press(&mut k, &h, "restart").unwrap();
        assert!(k.watch.pending.is_none());
        h.answer_next_prompt(Some(&[]));
        press(&mut k, &h, "restart").unwrap();
        assert!(k.watch.session.as_ref().unwrap().child.is_none());
        assert!(k.watch.pending.is_some());
        press(&mut k, &h, "launch").unwrap();
        let s = k.watch.session.as_ref().unwrap();
        assert!(s.names.is_empty());
        assert_eq!(s.history.as_ref().unwrap().names, ["old"]);
    }
    #[test]
    fn failed_replacement_keeps_previous_owned_watch_and_snapshots() {
        let (h, mut k) = setup();
        launch(&mut k, &h);
        ingest(&mut k, &event("ADDED", "old", "uid-old")).unwrap();
        let spare: Vec<_> = (0..7).map(|_| host::start(&Command::new("kubectl"), None).unwrap()).collect();
        h.answer_next_prompt(Some(&[]));
        press(&mut k, &h, "restart").unwrap();
        assert!(press(&mut k, &h, "launch").is_err());
        let s = k.watch.session.as_ref().unwrap();
        assert!(s.child.is_some());
        assert!(s.names.contains_key("old"));
        assert!(k.watch.pending.is_none());
        assert!(spare[0].poll(4096).unwrap().status.is_none());
    }
    #[test]
    fn foreign_render_preserves_owned_watch_but_scope_change_and_deactivation_release_it() {
        let (h, mut k) = setup();
        launch(&mut k, &h);
        ingest(&mut k, &event("ADDED", "private-name", "uid-a")).unwrap();
        h.add_project("/project");
        h.set_project_root(Some("/other"));
        let before = h.process_runs().len();
        assert!(matches!(k.render(VIEW, None, &h.invocation()).unwrap().body, Body::Message(_)));
        assert!(k.watch.session.as_ref().unwrap().child.is_some());
        assert_eq!(h.process_runs().len(), before);
        h.set_project_root(Some("/project"));
        k.namespace = Some("new".into());
        k.render(VIEW, None, &h.invocation()).unwrap();
        assert!(k.watch.session.as_ref().unwrap().child.is_none());
        assert!(k.watch.session.as_ref().unwrap().names.contains_key("private-name"));
        k.deactivate();
        assert!(k.watch.session.is_none());
    }
    #[test]
    fn deep_or_node_heavy_json_is_rejected_before_observations_change() {
        let (h, mut k) = setup();
        launch(&mut k, &h);
        ingest(&mut k, &event("ADDED", "good", "uid-a")).unwrap();
        let deep = format!("{{\"type\":\"ADDED\",\"object\":{}0{}}}", "[".repeat(40), "]".repeat(40));
        assert!(ingest(&mut k, &deep).is_err());
        assert!(k.watch.session.as_ref().unwrap().names.contains_key("good"));
        k.watch.session.as_mut().unwrap().pending.clear();
        let many=json!({"type":"ADDED","object":{"apiVersion":"v1","kind":"Pod","metadata":{"name":"extra","uid":"uid-b","namespace":"shop","resourceVersion":"rv"},"ignored":vec![0;5000]}}).to_string();
        assert!(ingest(&mut k, &many).is_err());
        assert!(!k.watch.session.as_ref().unwrap().names.contains_key("extra"));
        assert!(stream_depth(r#"{"text":"escaped \" }[[["}"#));
    }
    #[test]
    fn all_namespace_and_changed_review_choices_refuse_without_watch_start() {
        let (h, mut k) = setup();
        k.namespace = Some(ALL_NAMESPACES.into());
        assert!(k.run_command("watch-resources", &h.invocation(), None).is_err());
        assert!(h.process_runs().is_empty());
        k.namespace = Some("shop".into());
        prepare(&mut k, &h);
        k.namespace = Some("other".into());
        assert!(press(&mut k, &h, "launch").is_err());
        assert!(k.watch.pending.is_none());
        assert!(k.watch.session.is_none());
    }
    #[test]
    fn local_scope_tool_errors_do_not_publish_raw_credential_text() {
        let (h, mut k) = setup();
        h.on_process("kubectl", |_, _| {
            Ok(ProcessOutput {
                status: Some(1),
                stdout: String::new(),
                stderr: "parser secret-token-value auth-url-value".into(),
            })
        });
        h.answer_next_prompt(Some(&["0"]));
        let error = k.run_command("watch-resources", &h.invocation(), None).unwrap_err().to_string();
        assert!(!error.contains("secret-token-value") && !error.contains("auth-url-value"));
        assert!(k.watch.pending.is_none() && k.watch.session.is_none());
    }
    #[test]
    fn relative_kubeconfig_inspection_uses_the_same_project_cwd_as_watch_spawn() {
        let (h, mut k) = setup();
        h.set_setting("kubeconfig", "\"configs/relative-kubeconfig\"");
        prepare(&mut k, &h);
        assert!(h.process_runs().iter().all(|r| r.cwd.as_deref() == Some("/project")));
        assert!(
            k.watch
                .pending
                .as_ref()
                .unwrap()
                .spec
                .args()
                .contains(&"--kubeconfig=configs/relative-kubeconfig".into())
        );
        press(&mut k, &h, "launch").unwrap();
        assert!(h.process_runs().iter().all(|r| r.cwd.as_deref() == Some("/project")));
        assert_eq!(k.watch.session.as_ref().unwrap().spec.root, "/project");
    }
    #[test]
    fn closed_project_disposes_the_owned_child_and_metadata_without_polling() {
        let (h, mut k) = setup();
        launch(&mut k, &h);
        ingest(&mut k, &event("ADDED", "private-name", "uid-a")).unwrap();
        let before = h.process_runs().len();
        h.set_project_root(None);
        assert!(matches!(k.render(VIEW, None, &h.invocation()).unwrap().body, Body::Message(_)));
        assert!(k.watch.session.is_none());
        assert!(k.watch.pending.is_none());
        assert_eq!(h.process_runs().len(), before);
    }
    #[test]
    fn actual_mock_child_malformed_output_and_revoked_scope_keep_dated_last_good() {
        let (h, mut k) = setup();
        launch(&mut k, &h);
        ingest(&mut k, &event("ADDED", "good", "uid-a")).unwrap();
        // SDK start's real mock resource emits a synthetic startup banner; the
        // public manual poll must reject it rather than overwrite useful data.
        press(&mut k, &h, "poll").unwrap();
        let s = k.watch.session.as_ref().unwrap();
        assert!(s.child.is_none());
        assert!(s.names.contains_key("good"));
        assert!(s.status.contains("Malformed"));
        prepare(&mut k, &h);
        press(&mut k, &h, "launch").unwrap();
        let h = h.with_capabilities(&["project.read", "storage"]);
        assert!(press(&mut k, &h, "poll").is_err());
        assert!(k.watch.session.as_ref().unwrap().child.is_none());
    }
}
