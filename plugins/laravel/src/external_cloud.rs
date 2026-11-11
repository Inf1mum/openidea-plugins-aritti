//! Read-only Laravel Cloud environment snapshots. API contracts verified against
//! https://laravel.com/cloud/docs/api/environments/get-environment
//! https://laravel.com/cloud/docs/api/deployments/list-deployments
//! https://laravel.com/cloud/docs/api/deployments/get-deployment-logs
//! https://laravel.com/cloud/docs/api/database-clusters/list-database-clusters
//! https://laravel.com/cloud/docs/api/database-clusters/get-database-cluster
//! Host secrets have no readable value/version in API 1.4. Requests resolve the
//! declared secret in the host; refresh/disconnect invalidate our local epoch.
use super::*;
use openidea_plugin::{PickItem, PluginDataSource, views::Body};
const API: &str = "https://cloud.laravel.com/api";
const INTERVAL: u64 = 5_000;
const LOG_INTERVAL: u64 = 30_000;
#[derive(Default)]
pub(super) struct Cloud {
    pub views: BTreeMap<String, CloudView>,
    pub selected: BTreeMap<String, String>,
    pub epoch: u64,
}
#[derive(Clone)]
pub(super) struct CloudView {
    app: String,
    app_slug: String,
    app_revision: u64,
    env_revision: u64,
    epoch: u64,
    revision: u64,
    environment: Option<String>,
    snapshot: Option<Environment>,
    revealed: Option<usize>,
    show_logs: bool,
    logs: Vec<Value>,
    last_poll: u64,
    last_logs: u64,
    dirty_after: Option<u64>,
    list_pending: bool,
    paused: bool,
}
#[derive(Clone)]
struct Environment {
    name: String,
    slug: String,
    status: String,
    domain: String,
    php: String,
    branch: String,
    date: String,
    deployment: Option<String>,
    deployment_status: String,
    failure: String,
    organization: String,
    variables: Vec<(String, String)>,
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_millis().min(u64::MAX as u128) as u64)
}
fn optional(value: &Value, key: &str, max: usize) -> Result<String, Error> {
    if value[key].is_null() {
        Ok(String::new())
    } else {
        value[key]
            .as_str()
            .ok_or_else(|| Error::Invalid("Cloud field is not text.".into()))
            .and_then(|v| bounded(v, max))
    }
}
fn resource<'a>(value: &'a Value, kind: &str, expected: &str) -> Result<&'a Value, Error> {
    let data = &value["data"];
    if data["type"] != kind || id(data)? != expected {
        return Err(Error::Stale);
    }
    Ok(data)
}
fn relation(value: &Value, key: &str, expected: &str) -> Result<(), Error> {
    let related = &value["relationships"][key]["data"];
    if !related.is_null() && related["id"] != expected {
        return Err(Error::Stale);
    }
    Ok(())
}
fn environment_url(value: &str) -> Option<String> {
    let host = value.strip_prefix("https://").unwrap_or(value);
    if host.is_empty()
        || host.len() > 253
        || !host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        || host
            .split('.')
            .any(|part| part.is_empty() || part.starts_with('-') || part.ends_with('-'))
    {
        return None;
    }
    Some(format!("https://{host}"))
}
fn pending(status: &str) -> bool {
    matches!(
        status,
        "deploying"
            | "pending"
            | "build.pending"
            | "build.created"
            | "build.queued"
            | "build.running"
            | "build.succeeded"
            | "deployment.pending"
            | "deployment.created"
            | "deployment.queued"
            | "deployment.running"
    )
}

fn parse_environment(
    value: &Value,
    deployments: &Value,
    org: &Value,
    environment: &str,
    app: &str,
) -> Result<Environment, Error> {
    let env = resource(value, "environments", environment)?;
    relation(env, "application", app)?;
    let attrs = &env["attributes"];
    let organization = field(&org["data"]["attributes"], "slug")?;
    if org["data"]["type"] != "organizations" || !segment(&organization) {
        return Err(Error::Stale);
    }
    let slug = field(attrs, "slug")?;
    if !segment(&slug) {
        return Err(Error::Stale);
    }
    let variables = attrs["environment_variables"]
        .as_array()
        .ok_or("Cloud environment variables were not returned as an array.")?;
    let mut vars = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    if variables.len() > 512 {
        return Err("Cloud environment has too many variables.".into());
    }
    for variable in variables {
        let key = field(variable, "key")?;
        if key.is_empty() || !names.insert(key.clone()) {
            return Err("Cloud variable names are missing or duplicated.".into());
        }
        let value = variable["value"]
            .as_str()
            .ok_or("Cloud variable value is not text.")?;
        if value.len() > 64 * 1024 || value.contains('\0') {
            return Err("Cloud variable value exceeds its bound.".into());
        }
        vars.push((key, value.into()));
    }
    let deployments = deployments["data"]
        .as_array()
        .filter(|rows| rows.len() <= 100)
        .ok_or("Expected a bounded Cloud deployment list.")?;
    let mut result = Environment {
        name: field(attrs, "name")?,
        slug,
        status: field(attrs, "status")?,
        domain: optional(attrs, "vanity_domain", 2048)?,
        php: optional(attrs, "php_major_version", 64)?,
        branch: String::new(),
        date: String::new(),
        deployment: None,
        deployment_status: String::new(),
        failure: String::new(),
        organization,
        variables: vars,
    };
    if let Some(deployment) = deployments.first() {
        if deployment["type"] != "deployments" {
            return Err(Error::Stale);
        }
        relation(deployment, "environment", environment)?;
        let attrs = &deployment["attributes"];
        result.deployment = Some(id(deployment)?);
        result.deployment_status = field(attrs, "status")?;
        result.branch = optional(attrs, "branch_name", 1024)?;
        result.date = optional(attrs, "finished_at", 128)?;
        result.failure = optional(attrs, "failure_reason", 8192)?;
        if result.php.is_empty() {
            result.php = optional(attrs, "php_major_version", 64)?;
        }
    }
    Ok(result)
}
fn redact(value: &str, variables: &[(String, String)]) -> String {
    let mut prefixes: BTreeMap<char, Vec<&str>> = BTreeMap::new();
    for (_, secret) in variables {
        if let Some(first) = secret.chars().next() {
            prefixes.entry(first).or_default().push(secret);
        }
    }
    for candidates in prefixes.values_mut() {
        candidates.sort_by_key(|value| std::cmp::Reverse(value.len()));
        candidates.dedup();
    }
    let mut result = String::new();
    let mut at = 0;
    while at < value.len() {
        let tail = &value[at..];
        let first = tail.chars().next().unwrap();
        if let Some(secret) = prefixes
            .get(&first)
            .and_then(|values| values.iter().find(|secret| tail.starts_with(**secret)))
        {
            result.push_str("[redacted]");
            at += secret.len();
        } else {
            result.push(first);
            at += first.len_utf8();
        }
    }
    result
}

fn logs(value: &Value, variables: &[(String, String)]) -> Result<Vec<Value>, Error> {
    let mut result = Vec::new();
    let mut size = 0usize;
    for phase in ["build", "deploy"] {
        let data = &value["data"][phase];
        let available = data["available"]
            .as_bool()
            .ok_or("Cloud log phase availability was not returned.")?;
        if !available {
            result.push(json!({"phase":phase,"available":false}));
            continue;
        }
        let steps = data["steps"]
            .as_array()
            .filter(|steps| steps.len() <= 256)
            .ok_or("Expected bounded Cloud log steps.")?;
        if steps.is_empty() {
            result.push(json!({"phase":phase,"available":false}));
        }
        for step in steps {
            let name = field(step, "step")?;
            let description = field(step, "description")?;
            let status = field(step, "status")?;
            let time = optional(step, "time", 128)?;
            let output = step["output"].as_str().unwrap_or("");
            size = size.saturating_add(output.len() + description.len());
            if size > 512 * 1024 {
                return Err("Cloud logs exceed the display bound.".into());
            }
            result.push(json!({"phase":phase,"step":redact(&name,variables),"description":redact(&description,variables),"status":redact(&status,variables),"time":redact(&time,variables),"output":redact(output,variables)}));
        }
    }
    Ok(result)
}
impl CloudView {
    fn scope(&self, root: &str) -> String {
        format!(
            "cloud:{}:{}:{}:{}:{}",
            crate::artisan_metadata::hash(root),
            self.epoch,
            self.app_revision,
            self.env_revision,
            self.revision
        )
    }
    fn unfinished(&self) -> bool {
        self.snapshot
            .as_ref()
            .is_some_and(|env| pending(&env.status) || pending(&env.deployment_status))
    }
    fn active(&self) -> bool {
        !self.paused && (self.dirty_after.is_some() || self.list_pending || self.unfinished())
    }
}
impl State {
    pub(super) fn cloud_application_selected(&mut self, root: &str) {
        let Some(session) = self.projects.get(root) else {
            return;
        };
        let Some(app) = session.environment_application.as_ref() else {
            return;
        };
        let Some(row) = session.applications.iter().find(|row| row["id"] == *app) else {
            return;
        };
        if !self.cloud.views.contains_key(root) && self.cloud.views.len() >= 8 {
            if let Some(other) = self
                .cloud
                .views
                .keys()
                .find(|other| other.as_str() != root)
                .cloned()
            {
                self.cloud.views.remove(&other);
            }
        }
        self.cloud.selected.insert(root.into(), app.clone());
        let _ = host::storage::set(
            &format!(
                "external:cloud-application:{}",
                crate::artisan_metadata::hash(root)
            ),
            app,
        );
        self.cloud.epoch = self.cloud.epoch.wrapping_add(1);
        self.cloud.views.insert(
            root.into(),
            CloudView {
                app: app.clone(),
                app_slug: row["slug"].as_str().unwrap_or_default().into(),
                app_revision: session.applications_page.revision,
                env_revision: session.environments_page.revision,
                epoch: self.cloud.epoch,
                revision: 0,
                environment: None,
                snapshot: None,
                revealed: None,
                show_logs: false,
                logs: vec![],
                last_poll: 0,
                last_logs: 0,
                dirty_after: None,
                list_pending: session.environments.iter().any(|row| {
                    row["status"].as_str().is_some_and(pending) || row["pendingDeployment"] == true
                }),
                paused: false,
            },
        );
    }
    pub(super) fn cloud_restore_selection(&mut self, ctx: &Invocation) -> Result<(), Error> {
        let (_, root) = owner(ctx)?;
        let preferred = self
            .cloud
            .selected
            .get(&root)
            .cloned()
            .or_else(|| {
                host::storage::get(&format!(
                    "external:cloud-application:{}",
                    crate::artisan_metadata::hash(&root)
                ))
                .ok()
                .flatten()
            })
            .filter(|id| segment(id));
        let Some(preferred) = preferred else {
            return Ok(());
        };
        let session = self.projects.get(&root).ok_or(Error::Stale)?;
        if session.environment_application.is_some()
            || !session
                .applications
                .iter()
                .any(|app| app["id"] == preferred)
        {
            return Ok(());
        }
        let key = resource_key(
            "cloud-app",
            &root,
            session.applications_page.revision,
            &preferred,
        );
        self.cloud_environments(ctx, &key)
    }
    fn cloud_current(&self, ctx: &Invocation, view: &CloudView) -> Result<String, Error> {
        let (_, root) = owner(ctx)?;
        if !host::secret_is_set(CLOUD_SECRET) {
            return Err(Error::Stale);
        }
        let session = self.projects.get(&root).ok_or(Error::Stale)?;
        if session.applications_page.revision != view.app_revision
            || session.environment_application.as_deref() != Some(&view.app)
            || session.environments_page.revision != view.env_revision
            || self.cloud.views.get(&root).is_none_or(|current| {
                current.epoch != view.epoch
                    || current.revision != view.revision
                    || current.environment != view.environment
            })
        {
            return Err(Error::Stale);
        }
        if let Some(environment) = &view.environment {
            if !session
                .environments
                .iter()
                .any(|row| row["id"] == *environment && row["application"] == view.app)
            {
                return Err(Error::Stale);
            }
        }
        Ok(root)
    }
    fn cloud_capture(&mut self, ctx: &Invocation, view: CloudView, at: u64) -> Result<(), Error> {
        let root = self.cloud_current(ctx, &view)?;
        let environment = view.environment.as_deref().ok_or(Error::Stale)?;
        let (value, _) = get(format!("{API}/environments/{environment}"), CLOUD_SECRET)?;
        self.cloud_current(ctx, &view)?;
        let (deployments, _) = get(
            format!("{API}/environments/{environment}/deployments"),
            CLOUD_SECRET,
        )?;
        self.cloud_current(ctx, &view)?;
        let (org, _) = get(format!("{API}/meta/organization"), CLOUD_SECRET)?;
        let snapshot = parse_environment(&value, &deployments, &org, environment, &view.app)?;
        if view
            .snapshot
            .as_ref()
            .is_some_and(|old| old.organization != snapshot.organization)
        {
            self.cloud.views.remove(&root);
            return Err(Error::Stale);
        }
        self.cloud_current(ctx, &view)?;
        let mut next = view.clone();
        next.last_poll = at;
        // Variable row identities must never survive a refreshed/reordered value set.
        next.revision = next.revision.wrapping_add(1);
        next.dirty_after = None;
        next.paused = false;
        next.revealed = None;
        if next
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.deployment.as_ref())
            != snapshot.deployment.as_ref()
        {
            next.logs.clear();
            next.last_logs = 0;
        }
        next.snapshot = Some(snapshot);
        self.cloud.views.insert(root, next);
        Ok(())
    }
    /// Select only an environment captured under the current selected application.
    pub(crate) fn cloud_select_environment(
        &mut self,
        ctx: &Invocation,
        key: &str,
    ) -> Result<(), Error> {
        let (_, root) = owner(ctx)?;
        let session = self.projects.get(&root).ok_or(Error::Stale)?;
        let row = selected(
            &session.environments,
            key,
            "cloud-env",
            &root,
            session.environments_page.revision,
        )?;
        let environment = id(row)?;
        if row["application"]
            != session
                .environment_application
                .as_deref()
                .ok_or(Error::Stale)?
        {
            return Err(Error::Stale);
        }
        if !self.cloud.views.contains_key(&root) {
            self.cloud_application_selected(&root);
        }
        let mut view = self.cloud.views.get(&root).cloned().ok_or(Error::Stale)?;
        view.environment = Some(environment);
        view.snapshot = None;
        view.revision = view.revision.wrapping_add(1);
        view.revealed = None;
        view.logs.clear();
        view.show_logs = false;
        view.last_logs = 0;
        view.paused = false;
        self.cloud.views.insert(root, view.clone());
        self.cloud_capture(ctx, view, now())
    }
    pub(crate) fn cloud_clear_view(&mut self, ctx: &Invocation) -> Result<(), Error> {
        let (_, root) = owner(ctx)?;
        self.cloud.views.remove(&root);
        self.cloud.epoch = self.cloud.epoch.wrapping_add(1);
        Ok(())
    }
    pub(crate) fn cloud_buttons(&self, ctx: &Invocation) -> Vec<openidea_plugin::views::Button> {
        use openidea_plugin::views::Button;
        let Ok((_, root)) = owner(ctx) else {
            return vec![];
        };
        let Some(view) = self.cloud.views.get(&root) else {
            return vec![];
        };
        if self.cloud_current(ctx, view).is_err() {
            return vec![];
        }
        let snapshot = view.snapshot.as_ref();
        let has_environment = view.environment.is_some();
        let has_variables = snapshot.is_some_and(|snapshot| !snapshot.variables.is_empty());
        vec![
            Button::new(
                "cloud-details",
                &crate::ui_text::t("cloud.overviewButton", "Environment overview"),
            )
            .enabled(has_environment),
            Button::new(
                "cloud-selected-refresh",
                &crate::ui_text::t("cloud.refreshButton", "Refresh environment"),
            )
            .enabled(has_environment),
            Button::new(
                "cloud-logs",
                &crate::ui_text::t("cloud.logsButton", "Build / deploy logs"),
            )
            .enabled(snapshot.is_some_and(|snapshot| snapshot.deployment.is_some())),
            Button::new(
                "cloud-open-domain",
                &crate::ui_text::t("cloud.openDomainButton", "Open environment"),
            )
            .enabled(snapshot.is_some_and(|snapshot| environment_url(&snapshot.domain).is_some())),
            Button::new(
                "cloud-manage",
                &crate::ui_text::t("cloud.manageButton", "Manage environment"),
            )
            .enabled(snapshot.is_some_and(|snapshot| {
                segment(&snapshot.organization)
                    && segment(&snapshot.slug)
                    && segment(&view.app_slug)
            })),
            Button::new(
                "cloud-variable-reveal",
                &crate::ui_text::t("cloud.revealButton", "Reveal variable…"),
            )
            .enabled(has_variables),
            Button::new(
                "cloud-variable-copy",
                &crate::ui_text::t("cloud.copyButton", "Copy variable…"),
            )
            .enabled(has_variables),
            Button::new(
                "cloud-hide-variables",
                &crate::ui_text::t("cloud.hideButton", "Hide variables"),
            )
            .enabled(view.revealed.is_some()),
            Button::new(
                "cloud-import-database",
                &crate::ui_text::t("cloud.importButton", "Import database connection…"),
            )
            .enabled(snapshot.is_some()),
        ]
    }
    pub(crate) fn cloud_dirty(&mut self, root: &str) {
        if let Some(view) = self.cloud.views.get_mut(root) {
            view.dirty_after = Some(now().saturating_add(INTERVAL));
            view.revision = view.revision.wrapping_add(1);
            view.revealed = None;
            view.paused = false;
        }
    }
    pub(crate) fn cloud_tick(&mut self, ctx: &Invocation, message: &str) -> Result<Value, Error> {
        self.cloud_tick_at(ctx, message, now())
    }
    fn cloud_tick_at(&mut self, ctx: &Invocation, message: &str, at: u64) -> Result<Value, Error> {
        let request = crate::strict_json::parse(message, 8192)?;
        if request["op"] != "cloud-tick" {
            return Err(Error::Stale);
        }
        let (_, root) = owner(ctx)?;
        let view = self.cloud.views.get(&root).cloned().ok_or(Error::Stale)?;
        if request["scope"] != view.scope(&root) {
            return Err(Error::Stale);
        }
        self.cloud_current(ctx, &view)?;
        if !view.active()
            || at.saturating_sub(view.last_poll) < INTERVAL
            || view.dirty_after.is_some_and(|due| at < due)
        {
            return Ok(json!({"pending":view.active()}));
        }
        let result =
            if view.dirty_after.is_none() && !view.list_pending && view.environment.is_some() {
                self.cloud_capture(ctx, view.clone(), at)
            } else {
                let session = self.projects.get(&root).ok_or(Error::Stale)?;
                let key = resource_key(
                    "cloud-app",
                    &root,
                    session.applications_page.revision,
                    &view.app,
                );
                self.cloud_environments(ctx, &key).and_then(|_| {
                    if let Some(environment) = &view.environment {
                        let session = self.projects.get(&root).ok_or(Error::Stale)?;
                        if session
                            .environments
                            .iter()
                            .any(|row| row["id"] == *environment)
                        {
                            let key = resource_key(
                                "cloud-env",
                                &root,
                                session.environments_page.revision,
                                environment,
                            );
                            self.cloud_select_environment(ctx, &key)?;
                            let current = self.cloud.views.get_mut(&root).ok_or(Error::Stale)?;
                            if current
                                .snapshot
                                .as_ref()
                                .and_then(|snapshot| snapshot.deployment.as_ref())
                                == view
                                    .snapshot
                                    .as_ref()
                                    .and_then(|snapshot| snapshot.deployment.as_ref())
                            {
                                current.logs = view.logs.clone();
                                current.last_logs = view.last_logs;
                            }
                            current.show_logs = view.show_logs;
                            current.last_poll = at;
                            return Ok(());
                        }
                    }
                    Ok(())
                })
            };
        if result.is_ok() {
            if let Some(current) = self.cloud.views.get_mut(&root) {
                current.last_poll = at;
            }
        }
        if let Err(error) = result {
            if let Some(view) = self.cloud.views.get_mut(&root) {
                view.paused = true;
            }
            return Err(error);
        }
        if self.cloud.views.get(&root).is_some_and(|view| {
            view.show_logs && view.unfinished() && at.saturating_sub(view.last_logs) >= LOG_INTERVAL
        }) {
            self.cloud_logs_at(ctx, at)?;
        }
        Ok(json!({"pending":self.cloud.views.get(&root).is_some_and(CloudView::active)}))
    }
    fn cloud_logs_at(&mut self, ctx: &Invocation, at: u64) -> Result<(), Error> {
        let (_, root) = owner(ctx)?;
        let view = self.cloud.views.get(&root).cloned().ok_or(Error::Stale)?;
        self.cloud_current(ctx, &view)?;
        let snapshot = view.snapshot.as_ref().ok_or(Error::Stale)?;
        let deployment = snapshot
            .deployment
            .as_ref()
            .ok_or("No captured Cloud deployment has logs.")?;
        if at.saturating_sub(view.last_logs) < LOG_INTERVAL {
            return Ok(());
        }
        // Record the attempt too; repeated failures must not bypass the throttle.
        self.cloud
            .views
            .get_mut(&root)
            .ok_or(Error::Stale)?
            .last_logs = at;
        let (value, _) = get(format!("{API}/deployments/{deployment}/logs"), CLOUD_SECRET)?;
        let rows = logs(&value, &snapshot.variables)?;
        self.cloud_current(ctx, &view)?;
        let current = self.cloud.views.get_mut(&root).ok_or(Error::Stale)?;
        current.logs = rows;
        current.show_logs = true;
        Ok(())
    }
    pub(crate) fn cloud_action(
        &mut self,
        ctx: &Invocation,
        action: &str,
        key: Option<&str>,
    ) -> Result<(), Error> {
        let (_, root) = owner(ctx)?;
        let view = self.cloud.views.get(&root).cloned().ok_or(Error::Stale)?;
        self.cloud_current(ctx, &view)?;
        match action {
            "cloud-details" => {
                let current = self.cloud.views.get_mut(&root).ok_or(Error::Stale)?;
                current.show_logs = false;
                current.revealed = None;
                Ok(())
            }
            "cloud-selected-refresh" => self.cloud_capture(ctx, view, now()),
            "cloud-logs" => {
                self.cloud
                    .views
                    .get_mut(&root)
                    .ok_or(Error::Stale)?
                    .show_logs = true;
                self.cloud_logs_at(ctx, now())
            }
            "cloud-hide-variables" => {
                self.cloud
                    .views
                    .get_mut(&root)
                    .ok_or(Error::Stale)?
                    .revealed = None;
                Ok(())
            }
            "cloud-open-domain" => {
                let snapshot = view.snapshot.as_ref().ok_or(Error::Stale)?;
                host::open_url(&environment_url(&snapshot.domain).ok_or(Error::Stale)?)
            }
            "cloud-manage" => {
                let snapshot = view.snapshot.as_ref().ok_or(Error::Stale)?;
                if !segment(&view.app_slug)
                    || !segment(&snapshot.slug)
                    || !segment(&snapshot.organization)
                {
                    return Err(Error::Stale);
                }
                host::open_url(&format!(
                    "https://cloud.laravel.com/{}/{}/{}",
                    snapshot.organization, view.app_slug, snapshot.slug
                ))
            }
            "cloud-variable-reveal" | "cloud-variable-copy" => {
                let snapshot = view.snapshot.as_ref().ok_or(Error::Stale)?;
                let choices = snapshot
                    .variables
                    .iter()
                    .enumerate()
                    .map(|(i, (name, _))| {
                        PickItem::new(&format!("{}:variable:{i}", view.scope(&root)), name)
                    })
                    .collect::<Vec<_>>();
                let chosen = if let Some(key) = key {
                    key.to_owned()
                } else {
                    let Some(selected) = host::pick(
                        &crate::ui_text::t(
                            "cloud.chooseVariable",
                            "Choose a captured Cloud variable",
                        ),
                        &choices,
                    )?
                    else {
                        return Ok(());
                    };
                    selected
                };
                let index = choices
                    .iter()
                    .position(|item| item.key == chosen)
                    .ok_or(Error::Stale)?;
                self.cloud_current(ctx, &view)?;
                if action == "cloud-variable-copy" {
                    host::copy_to_clipboard(&snapshot.variables[index].1)
                } else {
                    self.cloud
                        .views
                        .get_mut(&root)
                        .ok_or(Error::Stale)?
                        .revealed = Some(index);
                    Ok(())
                }
            }
            _ => Err(Error::Stale),
        }
    }
    /// Rendering never makes HTTP calls; values remain masked until explicit reveal.
    pub(crate) fn cloud_body(&self, ctx: &Invocation) -> Option<Body> {
        let (_, root) = owner(ctx).ok()?;
        let view = self.cloud.views.get(&root)?;
        self.cloud_current(ctx, view).ok()?;
        let mut rows = Vec::new();
        if let Some(snapshot) = &view.snapshot {
            for (key, title, value) in [
                (
                    "name",
                    crate::ui_text::t("cloud.environment", "Environment"),
                    snapshot.name.clone(),
                ),
                (
                    "domain",
                    crate::ui_text::t("cloud.domain", "Domain"),
                    snapshot.domain.clone(),
                ),
                (
                    "status",
                    crate::ui_text::t("cloud.status", "Status"),
                    snapshot.status.clone(),
                ),
                (
                    "deployment",
                    crate::ui_text::t("cloud.deploymentStatus", "Deployment status"),
                    snapshot.deployment_status.clone(),
                ),
                (
                    "failure",
                    crate::ui_text::t("cloud.failure", "Failure reason"),
                    redact(&snapshot.failure, &snapshot.variables),
                ),
                (
                    "branch",
                    crate::ui_text::t("cloud.branch", "Branch"),
                    snapshot.branch.clone(),
                ),
                (
                    "date",
                    crate::ui_text::t("cloud.deploymentDate", "Deployment date"),
                    snapshot.date.clone(),
                ),
                (
                    "php",
                    crate::ui_text::t("cloud.php", "PHP version"),
                    snapshot.php.clone(),
                ),
            ] {
                rows.push(json!({"key":format!("cloud-detail:{key}"),"cells":[crate::ui_text::t("cloud.details","Cloud details"),title,if value.is_empty(){crate::ui_text::t("cloud.notReturned","Not returned by Cloud")}else{redact(&value,&snapshot.variables)}],"activationEnabled":false}));
            }
            for (i, (name, value)) in snapshot.variables.iter().enumerate() {
                rows.push(json!({"key":format!("{}:variable:{i}",view.scope(&root)),"cells":[crate::ui_text::t("cloud.variables","Variables"),name,if view.revealed==Some(i){value.as_str()}else{"••••••••"}],"activationEnabled":false}));
            }
            if view.show_logs {
                for (i, step) in view.logs.iter().enumerate() {
                    if step["available"] == false {
                        rows.push(json!({"key":format!("cloud-log:{i}"),"cells":[if step["phase"]=="build"{crate::ui_text::t("cloud.buildLogs","Build logs")}else{crate::ui_text::t("cloud.deployLogs","Deploy logs")},step["phase"],crate::ui_text::t("cloud.logsUnavailable","No log output is available for this phase.")],"activationEnabled":false}));
                        continue;
                    }
                    rows.push(json!({"key":format!("cloud-log:{i}"),"cells":[if step["phase"]=="build"{crate::ui_text::t("cloud.buildLogs","Build logs")}else{crate::ui_text::t("cloud.deployLogs","Deploy logs")},format!("{} · {} · {}",step["step"].as_str().unwrap_or(""),step["status"].as_str().unwrap_or(""),step["time"].as_str().unwrap_or("")),format!("{}\n{}",step["description"].as_str().unwrap_or(""),step["output"].as_str().unwrap_or(""))],"activationEnabled":false}));
                }
            }
        } else {
            rows = self.rows(ctx);
        }
        if view.paused {
            rows.push(json!({"key":"cloud-paused","cells":[crate::ui_text::t("cloud.details","Cloud details"),crate::ui_text::t("cloud.refreshPaused","Automatic refresh paused"),crate::ui_text::t("cloud.retryRefresh","Refresh this environment explicitly to retry.")],"activationEnabled":false}));
        }
        let poll=view.active().then(||json!({"intervalMs":INTERVAL,"message":{"op":"cloud-tick","scope":view.scope(&root)}}));
        Some(Body::Other{kind:"resources".into(),data:json!({"columns":[crate::ui_text::t("column.service","Service"),crate::ui_text::t("shared.resource","Resource"),crate::ui_text::t("column.status","Status")],"items":rows,"backgroundPoll":poll,"searchPlaceholder":crate::ui_text::t("cloud.filter","Filter captured Cloud details…")}).to_string()})
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Database {
    cluster: String,
    schema: String,
    definition: PluginDataSource,
    credential: Option<String>,
}
fn database_candidates(value: &Value) -> Result<Vec<Database>, Error> {
    let data = value["data"]
        .as_array()
        .filter(|rows| rows.len() <= 100)
        .ok_or("Expected a bounded Cloud database cluster page.")?;
    let included = value["included"]
        .as_array()
        .filter(|rows| rows.len() <= 2000)
        .ok_or("Cloud database schemas were not included.")?;
    let mut schemas = BTreeMap::new();
    for schema in included
        .iter()
        .filter(|schema| schema["type"] == "databaseSchemas")
    {
        let id = id(schema)?;
        if schemas.insert(id, schema).is_some() {
            return Err(Error::Stale);
        }
    }
    let mut result = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut pairs = std::collections::BTreeSet::new();
    let mut credential_bytes = 0usize;
    for cluster in data {
        if cluster["type"] != "databases" {
            return Err(Error::Stale);
        }
        let cluster_id = id(cluster)?;
        if !seen.insert(cluster_id.clone()) {
            return Err(Error::Stale);
        }
        let attrs = &cluster["attributes"];
        if attrs["status"] != "available" {
            continue;
        }
        let connection = &attrs["connection"];
        let driver = match connection["driver"]
            .as_str()
            .or_else(|| connection["protocol"].as_str())
        {
            Some("mysql" | "mariadb") => "mysql",
            Some("postgres" | "postgresql" | "pgsql") => "postgres",
            _ => continue,
        };
        let hostname = field(connection, "hostname")?;
        if hostname.is_empty()
            || (hostname.contains(['/', '@', '?', '#'])
                || hostname.contains(':') && hostname.parse::<std::net::IpAddr>().is_err())
            || hostname.chars().any(char::is_control)
        {
            return Err(Error::Stale);
        }
        let port = connection["port"]
            .as_u64()
            .or_else(|| {
                connection["port"]
                    .as_str()
                    .and_then(|port| port.parse().ok())
            })
            .and_then(|port| u16::try_from(port).ok())
            .filter(|port| *port > 0)
            .ok_or(Error::Stale)?;
        let user = field(connection, "username")?;
        let credential = if connection["password"].is_null() {
            None
        } else {
            let password = connection["password"]
                .as_str()
                .filter(|v| v.len() <= 64 * 1024 && !v.contains('\0'))
                .ok_or(Error::Stale)?;
            Some(password.to_owned())
        };
        let related = cluster["relationships"]["databases"]["data"]
            .as_array()
            .or_else(|| cluster["relationships"]["schemas"]["data"].as_array())
            .filter(|rows| rows.len() <= 2000)
            .ok_or(Error::Stale)?;
        for related in related {
            if related["type"] != "databaseSchemas" {
                return Err(Error::Stale);
            }
            let schema_id = id(related)?;
            if !pairs.insert((cluster_id.clone(), schema_id.clone())) {
                return Err(Error::Stale);
            }
            credential_bytes =
                credential_bytes.saturating_add(credential.as_ref().map_or(0, String::len));
            if result.len() >= 2000 || credential_bytes > 2 * 1024 * 1024 {
                return Err("Cloud database credentials exceed the capture bound.".into());
            }
            let schema = schemas.get(&schema_id).ok_or(Error::Stale)?;
            relation(schema, "database", &cluster_id)?;
            let database = field(&schema["attributes"], "name")?;
            let name = format!(
                "Cloud · {} / {} [{}]",
                field(attrs, "name")?,
                database,
                schema_id
            );
            if name.len() > 512 {
                return Err(Error::Stale);
            }
            result.push(Database {
                cluster: cluster_id.clone(),
                schema: schema_id,
                definition: PluginDataSource {
                    id: format!("laravel-{}", crate::artisan_metadata::hash(&name)),
                    name,
                    driver: driver.into(),
                    host: Some(hostname.clone()),
                    port: Some(port),
                    database: Some(database),
                    user: Some(user.clone()),
                    url: None,
                    options: vec![],
                },
                credential: credential.clone(),
            });
        }
    }
    Ok(result)
}
fn database_next(value: &Value) -> Result<Option<String>, Error> {
    let Some(next) = value["links"].get("next").filter(|next| !next.is_null()) else {
        return Ok(None);
    };
    let pairs = query_pairs(
        next.as_str().ok_or(Error::Stale)?,
        &format!("{API}/databases/clusters"),
    )?;
    let mut page = None;
    for (key, value) in pairs {
        match key {
            "include" if value == "databases" => {}
            "page" => {
                let n = value
                    .parse::<u32>()
                    .ok()
                    .filter(|n| *n > 0 && *n <= 1_000_000)
                    .ok_or(Error::Stale)?;
                page = Some(n);
            }
            _ => return Err(Error::Stale),
        }
    }
    Ok(Some(format!(
        "{API}/databases/clusters?include=databases&page={}",
        page.ok_or(Error::Stale)?
    )))
}
impl State {
    pub(crate) fn cloud_import_database(
        &self,
        ctx: &Invocation,
        runtime: &crate::runtime_inspection::State,
    ) -> Result<(), Error> {
        let (project, root) = owner(ctx)?;
        if project.data_sources()?.len() >= 256 {
            return Err(Error::Invalid(crate::ui_text::t(
                "cloud.databaseSourceLimit",
                "At most 256 database sources can be registered for this project.",
            )));
        }
        let view = self.cloud.views.get(&root).cloned().ok_or(Error::Stale)?;
        self.cloud_current(ctx, &view)?;
        let mut url = format!("{API}/databases/clusters?include=databases");
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..20 {
            if !seen.insert(url.clone()) {
                return Err(Error::Stale);
            }
            let (value, _) = get(url.clone(), CLOUD_SECRET)?;
            self.cloud_current(ctx, &view)?;
            let candidates = database_candidates(&value)?;
            let next = database_next(&value)?;
            let mut choices = candidates
                .iter()
                .enumerate()
                .map(|(i, database)| {
                    PickItem::new(&format!("database:{i}"), &database.definition.name).description(
                        &format!(
                            "{} · {}:{}",
                            database.definition.driver,
                            database.definition.host.as_deref().unwrap_or(""),
                            database.definition.port.unwrap_or(0)
                        ),
                    )
                })
                .collect::<Vec<_>>();
            if next.is_some() {
                choices.push(PickItem::new(
                    "more",
                    &crate::ui_text::t("cloud.moreDatabaseClusters", "More database clusters…"),
                ));
            }
            if choices.is_empty() {
                return Err("No available Cloud database schemas in this captured page.".into());
            }
            let Some(chosen) = host::pick(
                &crate::ui_text::t(
                    "cloud.chooseDatabase",
                    "Choose a captured Cloud database schema",
                ),
                &choices,
            )?
            else {
                return Ok(());
            };
            self.cloud_current(ctx, &view)?;
            if chosen == "more" {
                url = next.ok_or(Error::Stale)?;
                continue;
            }
            let candidate = chosen
                .strip_prefix("database:")
                .and_then(|n| n.parse::<usize>().ok())
                .and_then(|index| candidates.get(index))
                .ok_or(Error::Stale)?;
            return runtime.register_external_source(
                ctx,
                &candidate.definition,
                candidate.credential.as_deref(),
                || {
                    self.cloud_current(ctx, &view)?;
                    let (value, _) = get(
                        format!(
                            "{API}/databases/clusters/{}?include=databases",
                            candidate.cluster
                        ),
                        CLOUD_SECRET,
                    )?;
                    let mut list = value.clone();
                    list["data"] = json!([value["data"]]);
                    let current = database_candidates(&list)?
                        .into_iter()
                        .find(|database| database.schema == candidate.schema)
                        .ok_or(Error::Stale)?;
                    if &current != candidate {
                        return Err(Error::Stale);
                    }
                    self.cloud_current(ctx, &view)?;
                    Ok(())
                },
            );
        }
        Err("Cloud database discovery reached its page limit.".into())
    }
}

/// Native delivery adapter; pure resource parsers never consult locale state.
pub(crate) fn ui_error(error: Error) -> Error {
    let error = super::ui_error(error);
    fn message(message: String) -> String {
        match message.as_str() {
            "Cloud database credentials exceed the capture bound." => {
                crate::ui_text::t("cloud.error.43421a24bf", &message)
            }
            "Cloud database discovery reached its page limit." => {
                crate::ui_text::t("cloud.error.128344a5e0", &message)
            }
            "Cloud database schemas were not included." => {
                crate::ui_text::t("cloud.error.7debfd4c39", &message)
            }
            "Cloud environment has too many variables." => {
                crate::ui_text::t("cloud.error.def08f7396", &message)
            }
            "Cloud environment variables were not returned as an array." => {
                crate::ui_text::t("cloud.error.2980eef47a", &message)
            }
            "Cloud field is not text." => crate::ui_text::t("cloud.error.d8c681256d", &message),
            "Cloud logs exceed the display bound." => {
                crate::ui_text::t("cloud.error.a937b04059", &message)
            }
            "Cloud variable names are missing or duplicated." => {
                crate::ui_text::t("cloud.error.2d93a99cbd", &message)
            }
            "Cloud variable value exceeds its bound." => {
                crate::ui_text::t("cloud.error.d31cf1032a", &message)
            }
            "Cloud variable value is not text." => {
                crate::ui_text::t("cloud.error.dcf1b20fd9", &message)
            }
            "Expected a bounded Cloud database cluster page." => {
                crate::ui_text::t("cloud.error.91fef10e9a", &message)
            }
            "Expected a bounded Cloud deployment list." => {
                crate::ui_text::t("cloud.error.d87a106243", &message)
            }
            "Expected bounded Cloud log steps." => {
                crate::ui_text::t("cloud.error.99b96ac498", &message)
            }
            "No available Cloud database schemas in this captured page." => {
                crate::ui_text::t("cloud.error.11946f3e44", &message)
            }
            "No captured Cloud deployment has logs." => {
                crate::ui_text::t("cloud.error.7f3b999543", &message)
            }
            "Cloud log phase availability was not returned." => {
                crate::ui_text::t("cloud.logAvailabilityMissing", &message)
            }
            _ => message,
        }
    }
    match error {
        Error::Invalid(value) => Error::Invalid(message(value)),
        Error::Failed(value) => Error::Failed(message(value)),
        _ => error,
    }
}

#[cfg(test)]
#[path = "external_cloud_tests.rs"]
mod tests;
