//! Explicit read-only Sentry and Laravel Cloud snapshots. Rendering does no HTTP.
//! Contracts: https://docs.sentry.io/api/events/list-an-organizations-issues/
//! https://docs.sentry.io/api/events/retrieve-an-issue/
//! https://docs.sentry.io/api/events/retrieve-an-issue-event/
//! https://docs.sentry.io/api/pagination/
//! https://laravel.com/cloud/docs/api/applications/list-applications
//! https://laravel.com/cloud/docs/api/environments/list-environments
//! Tokens are declared host secrets, never read or sent through a webview.
use openidea_plugin::{Error, HttpRequest, Invocation, Project, host};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const MAX_BODY: usize = 2 << 20;
const MAX_ROWS: usize = 100;
const MAX_PAGES: usize = 20;
const MAX_TOTAL: usize = MAX_ROWS * MAX_PAGES;
const SENTRY_SECRET: &str = "sentry-token";
const CLOUD_SECRET: &str = "cloud-token";
#[path = "sentry_oauth.rs"]
mod oauth;
#[path = "external_cloud.rs"]
mod cloud;
pub(crate) use cloud::ui_error as cloud_ui_error;

#[derive(Default)]
pub(crate) struct State {
    projects: BTreeMap<String, Session>,
    sentry_device: oauth::DeviceState,
    cloud: cloud::Cloud,
}
#[derive(Default)]
struct Session {
    organization: String,
    issues: Vec<Value>,
    applications: Vec<Value>,
    environments: Vec<Value>,
    issues_page: Page,
    applications_page: Page,
    environments_page: Page,
    environment_application: Option<String>,
}
#[derive(Clone, Default, PartialEq, Eq)]
struct Page {
    revision: u64,
    next: Option<String>,
    seen: std::collections::BTreeSet<String>,
    pages: usize,
}
impl Page {
    fn first(previous: &Self, url: String, next: Option<String>) -> Self {
        Self {
            revision: previous.revision.wrapping_add(1),
            next,
            seen: [url].into(),
            pages: 1,
        }
    }
    fn request(&self) -> Result<String, Error> {
        if self.pages >= MAX_PAGES {
            return Err(
                "External inventory reached 20 pages; refresh or narrow the service scope.".into(),
            );
        }
        let next = self
            .next
            .clone()
            .ok_or("No more captured pages are available.")?;
        if self.seen.contains(&next) {
            return Err("External pagination repeated a previous page.".into());
        }
        Ok(next)
    }
    fn advance(&self, url: String, next: Option<String>) -> Result<Self, Error> {
        let mut result = self.clone();
        result.seen.insert(url);
        if next.as_ref().is_some_and(|n| result.seen.contains(n)) {
            return Err("External pagination repeated a previous page.".into());
        }
        result.next = next;
        result.pages += 1;
        Ok(result)
    }
}
fn resource_key(prefix: &str, root: &str, revision: u64, id: &str) -> String {
    format!(
        "{prefix}:{}:{revision}:{id}",
        crate::artisan_metadata::hash(root)
    )
}
fn selected<'a>(
    rows: &'a [Value],
    key: &str,
    prefix: &str,
    root: &str,
    revision: u64,
) -> Result<&'a Value, Error> {
    rows.iter()
        .find(|row| {
            row["id"]
                .as_str()
                .is_some_and(|id| key == resource_key(prefix, root, revision, id))
        })
        .ok_or(Error::Stale)
}
fn query_pairs<'a>(url: &'a str, base: &str) -> Result<Vec<(&'a str, &'a str)>, Error> {
    let tail = url
        .strip_prefix(base)
        .and_then(|s| s.strip_prefix('?'))
        .ok_or("External pagination changed origin or resource.")?;
    if tail.len() > 2048 {
        return Err("External pagination query is too large.".into());
    }
    let mut pairs = Vec::new();
    let mut keys = std::collections::BTreeSet::new();
    for part in tail.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = part
            .split_once('=')
            .ok_or("Malformed external pagination query.")?;
        if !keys.insert(key) {
            return Err("Duplicate external pagination parameter.".into());
        }
        pairs.push((key, value));
    }
    Ok(pairs)
}
fn sentry_next(headers: &[(String, String)], base: &str) -> Result<Option<String>, Error> {
    let mut next = None;
    for (_, header) in headers
        .iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case("link"))
    {
        if header.len() > 8192 {
            return Err("External pagination header exceeds its limit.".into());
        }
        for part in header.split(',') {
            let mut fields = part.trim().split(';');
            let url = fields.next().unwrap_or("").trim();
            let params: Vec<_> = fields.map(str::trim).collect();
            if !params.contains(&"rel=\"next\"") {
                continue;
            }
            if !params.contains(&"results=\"true\"") {
                continue;
            }
            if next.is_some() {
                return Err("Duplicate Sentry next-page relation.".into());
            }
            let url = url
                .strip_prefix('<')
                .and_then(|s| s.strip_suffix('>'))
                .ok_or("Malformed Sentry next-page URL.")?;
            let pairs = query_pairs(url, base)?;
            let mut cursor = None;
            for (key, value) in pairs {
                match key {
                    "cursor" => {
                        let value = value.replace("%3A", ":").replace("%3a", ":");
                        let pieces: Vec<_> = value.split(':').collect();
                        if pieces.len() != 3
                            || pieces.iter().any(|v| {
                                v.is_empty()
                                    || v.len() > 20
                                    || !v.bytes().all(|b| b.is_ascii_digit())
                            })
                            || !matches!(pieces[2], "0" | "1")
                        {
                            return Err("Invalid Sentry cursor.".into());
                        }
                        cursor = Some(value);
                    }
                    "limit" if value == "100" => {}
                    _ => return Err("Unsupported Sentry pagination parameter.".into()),
                }
            }
            let cursor = cursor.ok_or("Sentry next-page URL has no cursor.")?;
            next = Some(format!("{base}?limit=100&cursor={cursor}"));
        }
    }
    Ok(next)
}
fn cloud_next(value: &Value, base: &str) -> Result<Option<String>, Error> {
    let Some(next) = value.get("links").and_then(|v| v.get("next")) else {
        return Ok(None);
    };
    if next.is_null() {
        return Ok(None);
    }
    let url = next.as_str().ok_or("Invalid Cloud next-page link.")?;
    let pairs = query_pairs(url, base)?;
    let mut page = None;
    for (key, value) in pairs {
        if key != "page" || value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err("Unsupported Cloud pagination parameter.".into());
        }
        let n = value.parse::<u32>().map_err(|_| Error::Stale)?;
        if n == 0 || n > 1_000_000 {
            return Err("Invalid Cloud page number.".into());
        }
        page = Some(n);
    }
    Ok(Some(format!(
        "{base}?page={}",
        page.ok_or("Missing Cloud page number.")?
    )))
}
fn merge(previous: &[Value], fresh: Vec<Value>) -> Result<Vec<Value>, Error> {
    let mut result = previous.to_vec();
    for row in fresh {
        if let Some(existing) = result.iter_mut().find(|r| r["id"] == row["id"]) {
            *existing = row;
        } else {
            result.push(row);
        }
    }
    if result.len() > MAX_TOTAL {
        return Err("External inventory exceeds 2000 resources.".into());
    }
    Ok(result)
}
fn owner(ctx: &Invocation) -> Result<(Project, String), Error> {
    let project = ctx.project().ok_or("Open a Laravel project first.")?;
    let root = project.root()?;
    if !crate::hover::canonical(&root)
        || !host::projects()?
            .iter()
            .any(|p| p.root().ok().as_deref() == Some(root.as_str()))
    {
        return Err(Error::Stale);
    }
    Ok((project, root))
}
fn segment(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 200
        && text
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
}
fn bounded(text: &str, max: usize) -> Result<String, Error> {
    if text.len() > max || text.contains('\0') {
        return Err(Error::Invalid(
            "External service returned an oversized value.".into(),
        ));
    }
    Ok(text.into())
}
fn field(value: &Value, name: &str) -> Result<String, Error> {
    bounded(
        value[name]
            .as_str()
            .ok_or_else(|| Error::Invalid(format!("External response is missing {name}.")))?,
        4096,
    )
}
fn id(value: &Value) -> Result<String, Error> {
    let id = field(value, "id")?;
    if !segment(&id) {
        return Err(Error::Invalid(
            "External response has an invalid resource ID.".into(),
        ));
    }
    Ok(id)
}
fn get(url: String, secret: &str) -> Result<(Value, Vec<(String, String)>), Error> {
    if secret == SENTRY_SECRET {
        oauth::ensure_access()?;
    }
    if !host::secret_is_set(secret) {
        return Err(Error::Invalid(crate::ui_text::format(
            "runtime.boundary.afa74253ff",
            "Set {secret} in Settings › Plugins › Laravel › Secrets first. Tokens stay in the host credential store.",
            &[("secret", &(secret).to_string())],
        )));
    }
    let mut request = HttpRequest::get(&url)
        .header("Authorization", &format!("Bearer {{{{secret:{secret}}}}}"))
        .header("Accept", "application/json");
    request.timeout_ms = Some(15_000);
    let response = host::fetch(&request).map_err(|error| match error {
        Error::Denied(_) => error,
        _ => Error::Failed(
            crate::ui_text::text(
                "runtime.boundary.dfc3b48bc3",
                "External service request failed; retry explicitly.",
            )
            .into(),
        ),
    })?;
    if response.status != 200 {
        // Error bodies may echo credentials or sensitive server configuration.
        return Err(Error::Failed(
            match response.status {
                401 => crate::ui_text::text(
                    "runtime.boundary.89dddd18a6",
                    "External service rejected the token; update it in plugin Secrets.",
                ),
                403 => crate::ui_text::text(
                    "runtime.boundary.8ef0cf8d91",
                    "External service denied this resource or token scope.",
                ),
                429 => crate::ui_text::text(
                    "runtime.boundary.8f08acc492",
                    "External service rate limit reached; retry later.",
                ),
                _ => crate::ui_text::text(
                    "runtime.boundary.6967c42f8f",
                    "External service returned an unsuccessful response.",
                ),
            }
            .into(),
        ));
    }
    if response.body.len() > MAX_BODY {
        return Err(Error::Invalid(
            crate::ui_text::text(
                "runtime.boundary.af3377fc90",
                "External response exceeds 2 MiB.",
            )
            .into(),
        ));
    }
    let value = crate::strict_json::parse(
        std::str::from_utf8(&response.body).map_err(|_| {
            Error::Invalid(
                crate::ui_text::text(
                    "runtime.boundary.d2892ca2a6",
                    "External response is not UTF-8.",
                )
                .into(),
            )
        })?,
        MAX_BODY,
    )
    .map_err(|_| {
        Error::Invalid(
            crate::ui_text::text(
                "runtime.boundary.ffcaefadd6",
                "External response is not valid unambiguous JSON.",
            )
            .into(),
        )
    })?;
    Ok((value, response.headers))
}
fn sentry_rows(value: &Value) -> Result<Vec<Value>, Error> {
    let values = value
        .as_array()
        .filter(|rows| rows.len() <= MAX_ROWS)
        .ok_or("Expected at most 100 Sentry issues.")?;
    let mut ids = std::collections::BTreeSet::new();
    values
        .iter()
        .map(|issue| {
            let id = id(issue)?;
            if !ids.insert(id.clone()) {
                return Err(Error::Invalid("Duplicate Sentry issue ID.".into()));
            }
            Ok(
                json!({"id":id,"title":field(issue,"title")?,"status":field(issue,"status")?,
            "project":field(&issue["project"],"slug")?,
            "lastSeen":issue["lastSeen"].as_str().map(|s|bounded(s,128)).transpose()?}),
            )
        })
        .collect()
}
fn cloud_rows(value: &Value, kind: &str) -> Result<Vec<Value>, Error> {
    let values = value["data"]
        .as_array()
        .filter(|rows| rows.len() <= MAX_ROWS)
        .ok_or("Expected a Cloud JSON:API resource page of at most 100 records.")?;
    let mut ids = std::collections::BTreeSet::new();
    values
        .iter()
        .map(|resource| {
            if resource["type"].as_str() != Some(kind) {
                return Err(Error::Invalid("Cloud resource has the wrong type.".into()));
            }
            let id = id(resource)?;
            if !ids.insert(id.clone()) {
                return Err(Error::Invalid("Duplicate Cloud resource ID.".into()));
            }
            let attrs = &resource["attributes"];
            Ok(
                json!({"id":id,"name":field(attrs,"name")?,"slug":field(attrs,"slug")?,
            "status":attrs["status"].as_str().map(|s|bounded(s,128)).transpose()?,
            "region":attrs["region"].as_str().map(|s|bounded(s,128)).transpose()?}),
            )
        })
        .collect()
}
fn org_key(root: &str) -> String {
    format!(
        "external:sentry-org:{}",
        crate::artisan_metadata::hash(root)
    )
}
impl State {
    /// Configuration is project-specific and contains no token. No network call.
    pub(crate) fn configure_sentry(&mut self, ctx: &Invocation) -> Result<(), Error> {
        self.configure_sentry_ui_inner(ctx).map_err(ui_error)
    }
    fn configure_sentry_ui_inner(&mut self, ctx: &Invocation) -> Result<(), Error> {
        let (_, root) = owner(ctx)?;
        let existing = host::storage::get(&org_key(&root))?.unwrap_or_default();
        let Some(organization) = host::input(
            crate::ui_text::text(
                "runtime.boundary.70cf2bc29d",
                "Sentry organization slug (sentry.io)",
            ),
            Some(&existing),
        )?
        else {
            return Err(Error::Cancelled);
        };
        if !segment(&organization) {
            return Err(Error::Invalid(
                crate::ui_text::text("runtime.boundary.01a3fd0780","Enter a Sentry organization slug using letters, digits, hyphens or underscores.")
                    .into(),
            ));
        }
        let (_, current) = owner(ctx)?;
        if root != current {
            return Err(Error::Stale);
        }
        host::storage::set(&org_key(&root), &organization)?;
        let session = self.projects.entry(root).or_default();
        session.organization = organization;
        session.issues.clear();
        session.issues_page = Page::first(&session.issues_page, String::new(), None);
        Ok(())
    }
    pub(crate) fn refresh_sentry(&mut self, ctx: &Invocation) -> Result<(), Error> {
        self.refresh_sentry_ui_inner(ctx).map_err(ui_error)
    }
    fn refresh_sentry_ui_inner(&mut self, ctx: &Invocation) -> Result<(), Error> {
        let (_, root) = owner(ctx)?;
        let organization = host::storage::get(&org_key(&root))?
            .filter(|s| segment(s))
            .ok_or("Configure this project's Sentry organization first.")?;
        let base = format!("https://sentry.io/api/0/organizations/{organization}/issues/");
        let url = format!("{base}?limit=100");
        let (value, headers) = get(url.clone(), SENTRY_SECRET)?;
        let issues = sentry_rows(&value)?;
        let next = sentry_next(&headers, &base)?;
        if owner(ctx)?.1 != root
            || host::storage::get(&org_key(&root))?.as_deref() != Some(&organization)
        {
            return Err(Error::Stale);
        }
        let session = self.projects.entry(root).or_default();
        session.organization = organization;
        session.issues = issues;
        session.issues_page = Page::first(&session.issues_page, url, next);
        Ok(())
    }
    pub(crate) fn load_more_sentry(&mut self, ctx: &Invocation) -> Result<(), Error> {
        self.load_more_sentry_ui_inner(ctx).map_err(ui_error)
    }
    fn load_more_sentry_ui_inner(&mut self, ctx: &Invocation) -> Result<(), Error> {
        let (_, root) = owner(ctx)?;
        let session = self.projects.get(&root).ok_or(Error::Stale)?;
        let org = session.organization.clone();
        let before = session.issues_page.clone();
        if !segment(&org) || host::storage::get(&org_key(&root))?.as_deref() != Some(&org) {
            return Err(Error::Stale);
        }
        let url = before.request()?;
        let base = format!("https://sentry.io/api/0/organizations/{org}/issues/");
        // Only our reconstructed URL is stored, never a cross-origin server URL.
        let (value, headers) = get(url.clone(), SENTRY_SECRET)?;
        let fresh = sentry_rows(&value)?;
        let page = before.advance(url, sentry_next(&headers, &base)?)?;
        if owner(ctx)?.1 != root || host::storage::get(&org_key(&root))?.as_deref() != Some(&org) {
            return Err(Error::Stale);
        }
        let session = self.projects.get_mut(&root).ok_or(Error::Stale)?;
        if session.issues_page != before || session.organization != org {
            return Err(Error::Stale);
        }
        session.issues = merge(&session.issues, fresh)?;
        session.issues_page = page;
        Ok(())
    }
    pub(crate) fn refresh_cloud(&mut self, ctx: &Invocation) -> Result<(), Error> {
        self.refresh_cloud_ui_inner(ctx).map_err(ui_error)
    }
    fn refresh_cloud_ui_inner(&mut self, ctx: &Invocation) -> Result<(), Error> {
        let (_, root) = owner(ctx)?;
        let base = "https://cloud.laravel.com/api/applications";
        let (value, _) = get(base.into(), CLOUD_SECRET)?;
        let applications = cloud_rows(&value, "applications")?;
        let next = cloud_next(&value, base)?;
        if owner(ctx)?.1 != root {
            return Err(Error::Stale);
        }
        let session = self.projects.entry(root.clone()).or_default();
        session.applications = applications;
        session.applications_page = Page::first(&session.applications_page, base.into(), next);
        session.environments.clear();
        session.environment_application = None;
        session.environments_page = Page::first(&session.environments_page, String::new(), None);
        self.cloud.views.remove(&root);
        self.cloud.epoch=self.cloud.epoch.wrapping_add(1);
        self.cloud_restore_selection(ctx)
    }
    pub(crate) fn load_more_cloud(&mut self, ctx: &Invocation) -> Result<(), Error> {
        self.load_more_cloud_ui_inner(ctx).map_err(ui_error)
    }
    fn load_more_cloud_ui_inner(&mut self, ctx: &Invocation) -> Result<(), Error> {
        let (_, root) = owner(ctx)?;
        let before = self
            .projects
            .get(&root)
            .ok_or(Error::Stale)?
            .applications_page
            .clone();
        let url = before.request()?;
        let (value, _) = get(url.clone(), CLOUD_SECRET)?;
        let fresh = cloud_rows(&value, "applications")?;
        let page = before.advance(
            url,
            cloud_next(&value, "https://cloud.laravel.com/api/applications")?,
        )?;
        if owner(ctx)?.1 != root {
            return Err(Error::Stale);
        }
        let session = self.projects.get_mut(&root).ok_or(Error::Stale)?;
        if session.applications_page != before {
            return Err(Error::Stale);
        }
        session.applications = merge(&session.applications, fresh)?;
        session.applications_page = page;
        self.cloud_restore_selection(ctx)
    }
    /// Row key binds the captured application to project and snapshot revision.
    pub(crate) fn cloud_environments(&mut self, ctx: &Invocation, key: &str) -> Result<(), Error> {
        self.cloud_environments_ui_inner(ctx, key).map_err(ui_error)
    }
    fn cloud_environments_ui_inner(&mut self, ctx: &Invocation, key: &str) -> Result<(), Error> {
        let (_, root) = owner(ctx)?;
        let session = self.projects.get(&root).ok_or(Error::Stale)?;
        let snapshot = selected(
            &session.applications,
            key,
            "cloud-app",
            &root,
            session.applications_page.revision,
        )?
        .clone();
        let app = id(&snapshot)?;
        let revision = session.applications_page.revision;
        let base = format!("https://cloud.laravel.com/api/applications/{app}/environments");
        let (value, _) = get(base.clone(), CLOUD_SECRET)?;
        let environments = cloud_rows(&value, "environments")?;
        let next = cloud_next(&value, &base)?;
        if owner(ctx)?.1 != root {
            return Err(Error::Stale);
        }
        let session = self.projects.get_mut(&root).ok_or(Error::Stale)?;
        if session.applications_page.revision != revision
            || !session.applications.contains(&snapshot)
        {
            return Err(Error::Stale);
        }
        session.environment_application = Some(app.clone());
        session.environments = environments
            .into_iter()
            .map(|mut row| {
                row["application"] = json!(app);
                row
            })
            .collect();
        session.environments_page = Page::first(&session.environments_page, base, next);
        self.cloud_application_selected(&root);
        Ok(())
    }
    pub(crate) fn load_more_environments(&mut self, ctx: &Invocation) -> Result<(), Error> {
        self.load_more_environments_ui_inner(ctx).map_err(ui_error)
    }
    fn load_more_environments_ui_inner(&mut self, ctx: &Invocation) -> Result<(), Error> {
        let (_, root) = owner(ctx)?;
        let session = self.projects.get(&root).ok_or(Error::Stale)?;
        let app = session
            .environment_application
            .clone()
            .ok_or(Error::Stale)?;
        let application = selected(
            &session.applications,
            &resource_key("cloud-app", &root, session.applications_page.revision, &app),
            "cloud-app",
            &root,
            session.applications_page.revision,
        )?
        .clone();
        let before = session.environments_page.clone();
        let app_revision = session.applications_page.revision;
        let url = before.request()?;
        let (value, _) = get(url.clone(), CLOUD_SECRET)?;
        let fresh = cloud_rows(&value, "environments")?
            .into_iter()
            .map(|mut row| {
                row["application"] = json!(app);
                row
            })
            .collect();
        let base = format!("https://cloud.laravel.com/api/applications/{app}/environments");
        let page = before.advance(url, cloud_next(&value, &base)?)?;
        if owner(ctx)?.1 != root {
            return Err(Error::Stale);
        }
        let session = self.projects.get_mut(&root).ok_or(Error::Stale)?;
        if session.environments_page != before
            || session.environment_application.as_deref() != Some(&app)
            || session.applications_page.revision != app_revision
            || !session.applications.contains(&application)
        {
            return Err(Error::Stale);
        }
        session.environments = merge(&session.environments, fresh)?;
        session.environments_page = page;
        Ok(())
    }
    pub(crate) fn pagination(&self, ctx: &Invocation) -> Value {
        let Ok((_, root)) = owner(ctx) else {
            return json!({});
        };
        let Some(s) = self.projects.get(&root) else {
            return json!({});
        };
        let more = |p: &Page| p.next.is_some() && p.pages < MAX_PAGES;
        json!({"sentry":more(&s.issues_page),"cloud":more(&s.applications_page),"environments":more(&s.environments_page),
            "sentryCount":s.issues.len(),"cloudCount":s.applications.len(),"environmentCount":s.environments.len(),
            "sentryPages":s.issues_page.pages,"cloudPages":s.applications_page.pages,"environmentPages":s.environments_page.pages,"maxPages":MAX_PAGES,
            "sentryLimited":s.issues_page.next.is_some()&&s.issues_page.pages>=MAX_PAGES,
            "cloudLimited":s.applications_page.next.is_some()&&s.applications_page.pages>=MAX_PAGES,
            "environmentsLimited":s.environments_page.next.is_some()&&s.environments_page.pages>=MAX_PAGES})
    }
    /// Unlink this project's org, retaining the shared credential for other projects.
    pub(crate) fn forget_sentry(&mut self, ctx: &Invocation) -> Result<(), Error> {
        self.forget_sentry_ui_inner(ctx).map_err(ui_error)
    }
    fn forget_sentry_ui_inner(&mut self, ctx: &Invocation) -> Result<(), Error> {
        let (_, root) = owner(ctx)?;
        host::storage::delete(&org_key(&root))?;
        if let Some(session) = self.projects.get_mut(&root) {
            session.organization.clear();
            session.issues.clear();
            session.issues_page = Page::first(&session.issues_page, String::new(), None);
        }
        Ok(())
    }
    /// Fetch only a previously captured issue; output is reduced to safe display fields.
    pub(crate) fn sentry_details(&self, ctx: &Invocation, key: &str) -> Result<String, Error> {
        self.sentry_details_ui_inner(ctx, key).map_err(ui_error)
    }
    fn sentry_details_ui_inner(&self, ctx: &Invocation, key: &str) -> Result<String, Error> {
        let (_, root) = owner(ctx)?;
        let session = self
            .projects
            .get(&root)
            .filter(|s| segment(&s.organization))
            .ok_or(Error::Stale)?;
        let selected = selected(
            &session.issues,
            key,
            "sentry-issue",
            &root,
            session.issues_page.revision,
        )?;
        let issue = id(selected)?;
        let organization = &session.organization;
        if host::storage::get(&org_key(&root))?.as_deref() != Some(organization) {
            return Err(Error::Stale);
        }
        let (details, _) = get(
            format!("https://sentry.io/api/0/organizations/{organization}/issues/{issue}/"),
            SENTRY_SECRET,
        )?;
        if id(&details)? != issue
            || owner(ctx)?.1 != root
            || host::storage::get(&org_key(&root))?.as_deref() != Some(organization)
        {
            return Err(Error::Stale);
        }
        let (event, _) = get(
            format!(
                "https://sentry.io/api/0/organizations/{organization}/issues/{issue}/events/latest/"
            ),
            SENTRY_SECRET,
        )?;
        let mut frames = vec![];
        if let Some(entries) = event["entries"].as_array() {
            for entry in entries.iter().take(100) {
                if entry["type"] != "exception" {
                    continue;
                }
                if let Some(values) = entry["data"]["values"].as_array() {
                    for exception in values.iter().take(16) {
                        if let Some(stack) = exception["stacktrace"]["frames"].as_array() {
                            for frame in stack.iter().take(256usize.saturating_sub(frames.len())) {
                                frames.push(json!({"file":frame["filename"].as_str().map(|s|bounded(s,4096)).transpose()?,
                                    "function":frame["function"].as_str().map(|s|bounded(s,4096)).transpose()?,
                                    "line":frame["lineNo"].as_u64()}));
                            }
                        }
                    }
                }
            }
        }
        if owner(ctx)?.1 != root
            || host::storage::get(&org_key(&root))?.as_deref() != Some(organization)
        {
            return Err(Error::Stale);
        }
        Ok(
            serde_json::to_string_pretty(&json!({"organization":organization,"issue":issue,
            "title":field(&details,"title")?,"status":field(&details,"status")?,"frames":frames,
            "note":"Remote frame names are display text, not trusted local navigation targets."}))
            .map_err(|e| Error::Failed(e.to_string()))?,
        )
    }
    /// Project snapshots only; rendering never requests credentials or fetches data.
    pub(crate) fn rows(&self, ctx: &Invocation) -> Vec<Value> {
        let Ok((_, root)) = owner(ctx) else {
            return vec![];
        };
        let mut rows = vec![
            json!({"key":"external-info","cells":[crate::ui_text::text("runtime.boundary.8f3509b64e","Connections"),crate::ui_text::text("runtime.boundary.b0881ba10e","Tokens are configured in plugin Secrets"),crate::ui_text::text("runtime.boundary.f6609b636c","Explicit refresh · Load More requests one page (20-page limit)")],"activationEnabled":false}),
        ];
        let Some(session) = self.projects.get(&root) else {
            return rows;
        };
        if [
            &session.issues_page,
            &session.applications_page,
            &session.environments_page,
        ]
        .iter()
        .any(|page| page.next.is_some() && page.pages >= MAX_PAGES)
        {
            rows[0]["cells"][2] = json!(crate::ui_text::text(
                "runtime.boundary.4b31731ade",
                "Load limit reached (20 pages); results are incomplete. Refresh to restart."
            ));
        }
        if host::secret_is_set(SENTRY_SECRET)
            && host::storage::get(&org_key(&root))
                .ok()
                .flatten()
                .as_deref()
                == Some(&session.organization)
        {
            rows.extend(session.issues.iter().map(|issue|json!({"key":resource_key("sentry-issue",&root,session.issues_page.revision,issue["id"].as_str().unwrap_or_default()),"cells":["Sentry",issue["title"],format!("{} · {}",issue["project"].as_str().unwrap_or_default(),issue["status"].as_str().unwrap_or_default())],"activationEnabled":false})));
        }
        if host::secret_is_set(CLOUD_SECRET) {
            rows.extend(session.applications.iter().map(|app|json!({"key":resource_key("cloud-app",&root,session.applications_page.revision,app["id"].as_str().unwrap_or_default()),"cells":[crate::ui_text::text("runtime.boundary.d29443bb01","Cloud application"),app["name"],app["slug"]],"activationEnabled":false})));
            rows.extend(session.environments.iter().map(|env|json!({"key":resource_key("cloud-env",&root,session.environments_page.revision,env["id"].as_str().unwrap_or_default()),"cells":[crate::ui_text::text("runtime.boundary.5369a0cbef","Cloud environment"),env["name"],env["status"]],"activationEnabled":false})));
        }
        rows
    }
    /// Declared tokens are global to the plugin; disconnect clears every cached project.
    /// Nonsecret organization preferences survive reconnect; forget_sentry unlinks one project.
    pub(crate) fn disconnect_sentry(&mut self) -> Result<(), Error> {
        self.disconnect_sentry_ui_inner().map_err(ui_error)
    }
    fn disconnect_sentry_ui_inner(&mut self) -> Result<(), Error> {
        host::secret_delete(SENTRY_SECRET)?;
        oauth::disconnect(&mut self.sentry_device)?;
        for session in self.projects.values_mut() {
            session.issues.clear();
            session.issues_page = Page::first(&session.issues_page, String::new(), None);
        }
        Ok(())
    }
    pub(crate) fn disconnect_cloud(&mut self) -> Result<(), Error> {
        self.disconnect_cloud_ui_inner().map_err(ui_error)
    }
    fn disconnect_cloud_ui_inner(&mut self) -> Result<(), Error> {
        host::secret_delete(CLOUD_SECRET)?;
        for session in self.projects.values_mut() {
            session.applications.clear();
            session.environments.clear();
            session.environment_application = None;
            session.applications_page =
                Page::first(&session.applications_page, String::new(), None);
            session.environments_page =
                Page::first(&session.environments_page, String::new(), None);
        }
        let mut roots=self.projects.keys().cloned().collect::<std::collections::BTreeSet<_>>();
        roots.extend(host::projects().unwrap_or_default().into_iter().filter_map(|project|project.root().ok()));
        for root in roots{let _=host::storage::delete(&format!("external:cloud-application:{}",crate::artisan_metadata::hash(&root)));}
        self.cloud.selected.clear();
        self.cloud.views.clear();
        self.cloud.epoch=self.cloud.epoch.wrapping_add(1);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{HttpResponse, mock::MockHost};
    #[test]
    fn parsers_bound_and_validate_resource_identity_without_trusting_links() {
        assert!(!segment("../other"));
        assert!(!segment("org?project=other"));
        assert!(sentry_rows(&json!([{"id":"1","title":"Issue","status":"unresolved","project":{"slug":"app"},"permalink":"https://evil.example"}])).is_ok());
        assert!(
            sentry_rows(
                &json!([{"id":"../1","title":"Issue","status":"x","project":{"slug":"app"}}])
            )
            .is_err()
        );
        assert!(cloud_rows(&json!({"data":[{"id":"app_1","type":"applications","attributes":{"name":"App","slug":"app"}}]}),"applications").is_ok());
        assert!(cloud_rows(&json!({"data":[{"id":"app_1","type":"environments","attributes":{"name":"App","slug":"app"}}]}),"applications").is_err());
        assert!(
            sentry_rows(&json!(vec![
                json!({"id":"1","title":"x","status":"x","project":{"slug":"p"}});
                101
            ]))
            .is_err()
        );
    }
    #[test]
    fn fetch_uses_host_secret_and_never_surfaces_server_error_bodies() {
        let h = MockHost::install().with_capabilities(&["net:sentry.io", "secrets", "storage"]);
        h.set_secret(SENTRY_SECRET, "private-token");
        h.on_fetch(|r| {
            assert!(
                r.headers
                    .iter()
                    .any(|(k, v)| k == "Authorization" && v == "Bearer private-token")
            );
            Ok(HttpResponse {
                status: 401,
                headers: vec![],
                body: b"private-token".to_vec(),
            })
        });
        let error = get(
            "https://sentry.io/api/0/organizations/app/issues/?limit=100".into(),
            SENTRY_SECRET,
        )
        .err()
        .unwrap()
        .to_string();
        assert!(!error.contains("private-token"));
        assert!(error.contains("rejected"));
    }
    #[test]
    fn malformed_duplicate_and_oversized_responses_are_refused() {
        let h = MockHost::install().with_capabilities(&["net:cloud.laravel.com", "secrets"]);
        h.set_secret(CLOUD_SECRET, "private-token");
        h.on_fetch(|_| {
            Ok(HttpResponse {
                status: 200,
                headers: vec![],
                body: br#"{"data":[],"data":[]}"#.to_vec(),
            })
        });
        assert!(
            get(
                "https://cloud.laravel.com/api/applications".into(),
                CLOUD_SECRET
            )
            .is_err()
        );
        h.on_fetch(|_| {
            Ok(HttpResponse {
                status: 200,
                headers: vec![],
                body: vec![b' '; MAX_BODY + 1],
            })
        });
        assert!(
            get(
                "https://cloud.laravel.com/api/applications".into(),
                CLOUD_SECRET
            )
            .is_err()
        );
    }

    #[test]
    fn explicit_snapshots_details_and_disconnect_are_project_scoped() {
        let h = MockHost::install().with_capabilities(&[
            "project.read",
            "storage",
            "secrets",
            "net:sentry.io",
            "net:cloud.laravel.com",
        ]);
        h.declare_secret("sentry-refresh-token");
        h.set_secret(SENTRY_SECRET, "sentry-private");
        h.set_secret(CLOUD_SECRET, "cloud-private");
        h.answer_next_prompt(Some(&["acme"]));
        let mut state = State::default();
        state.configure_sentry(&h.invocation()).unwrap();
        assert!(h.requests().is_empty());
        h.on_fetch(|request| {
            let value = match request.url.as_str() {
                "https://sentry.io/api/0/organizations/acme/issues/?limit=100" => json!([
                    {"id":"42","title":"Failure","status":"unresolved","project":{"slug":"shop"}}
                ]),
                "https://sentry.io/api/0/organizations/acme/issues/42/" => json!({
                    "id":"42","title":"Failure","status":"unresolved","sensitive":"never-display"
                }),
                "https://sentry.io/api/0/organizations/acme/issues/42/events/latest/" => json!({
                    "entries":[{"type":"exception","data":{"values":[{"stacktrace":{"frames":[
                        {"filename":"/remote/app.php","function":"handle","lineNo":5,"vars":{"secret":"never-display"}}
                    ]}}]}}]
                }),
                "https://cloud.laravel.com/api/applications" => json!({"data":[
                    {"id":"app_1","type":"applications","attributes":{"name":"Shop","slug":"shop"}}
                ]}),
                "https://cloud.laravel.com/api/applications/app_1/environments" => json!({"data":[
                    {"id":"env_1","type":"environments","attributes":{"name":"Production","slug":"production","status":"running"}}
                ]}),
                other => panic!("Unexpected request: {other}"),
            };
            Ok(HttpResponse { status: 200, headers: vec![], body: value.to_string().into_bytes() })
        });
        state.refresh_sentry(&h.invocation()).unwrap();
        state.refresh_cloud(&h.invocation()).unwrap();
        state
            .cloud_environments(
                &h.invocation(),
                &resource_key("cloud-app", "/project", 1, "app_1"),
            )
            .unwrap();
        let detail = state
            .sentry_details(
                &h.invocation(),
                &resource_key("sentry-issue", "/project", 2, "42"),
            )
            .unwrap();
        assert!(detail.contains("/remote/app.php"));
        assert!(!detail.contains("never-display"));
        let before = h.requests().len();
        let rows = state.rows(&h.invocation());
        assert_eq!(rows.len(), 4);
        assert!(rows.iter().all(|row| row["activationEnabled"] == false));
        assert_eq!(h.requests().len(), before);
        assert!(
            state
                .cloud_environments(&h.invocation(), "cloud-app:other")
                .is_err()
        );
        assert!(
            state
                .sentry_details(&h.invocation(), "sentry-issue:other")
                .is_err()
        );
        h.set_project_root(Some("/other-project"));
        assert_eq!(state.rows(&h.invocation()).len(), 1);
        assert!(
            state
                .cloud_environments(
                    &h.invocation(),
                    &resource_key("cloud-app", "/project", 1, "app_1")
                )
                .is_err()
        );
        assert_eq!(h.requests().len(), before);
        h.set_project_root(Some("/project"));
        state.disconnect_sentry().unwrap();
        state.disconnect_cloud().unwrap();
        assert!(!host::secret_is_set(SENTRY_SECRET));
        assert!(!host::secret_is_set(CLOUD_SECRET));
        assert_eq!(state.rows(&h.invocation()).len(), 1);
    }
    #[test]
    fn failed_refresh_keeps_previous_snapshot_and_missing_token_never_fetches() {
        let h = MockHost::install().with_capabilities(&[
            "project.read",
            "storage",
            "secrets",
            "net:cloud.laravel.com",
        ]);
        let mut state = State::default();
        assert!(state.refresh_cloud(&h.invocation()).is_err());
        assert!(h.requests().is_empty());
        h.set_secret(CLOUD_SECRET, "cloud-private");
        h.on_fetch(|_| {
            Ok(HttpResponse {
                status: 200,
                headers: vec![],
                body: json!({"data":[
                    {"id":"app_1","type":"applications","attributes":{"name":"Shop","slug":"shop"}}
                ]})
                .to_string()
                .into_bytes(),
            })
        });
        state.refresh_cloud(&h.invocation()).unwrap();
        let previous = state.rows(&h.invocation());
        h.on_fetch(|_| {
            Ok(HttpResponse {
                status: 429,
                headers: vec![],
                body: b"cloud-private".to_vec(),
            })
        });
        assert!(state.refresh_cloud(&h.invocation()).is_err());
        assert_eq!(state.rows(&h.invocation()), previous);
    }

    #[test]
    fn pagination_links_are_reconstructed_without_cross_origin_or_resource_forwarding() {
        let sentry = "https://sentry.io/api/0/organizations/acme/issues/";
        let header = |url: &str| {
            vec![(
                "Link".into(),
                format!("<{url}>; rel=\"next\"; results=\"true\"; cursor=\"0:100:0\""),
            )]
        };
        assert_eq!(
            sentry_next(&header(&format!("{sentry}?&cursor=0%3A100%3A0")), sentry).unwrap(),
            Some(format!("{sentry}?limit=100&cursor=0:100:0"))
        );
        for url in [
            "https://evil.example/?cursor=0:100:0",
            "https://sentry.io.evil.example/api/0/organizations/acme/issues/?cursor=0:100:0",
            "https://sentry.io/api/0/organizations/other/issues/?cursor=0:100:0",
            "https://sentry.io/api/0/organizations/acme/issues/?cursor=0:100:0&token=secret",
            "https://sentry.io/api/0/organizations/acme/issues/?cursor=0:100:0&cursor=0:200:0",
            "https://sentry.io/api/0/organizations/acme/issues/?cursor=0:100:0#evil",
        ] {
            assert!(sentry_next(&header(url), sentry).is_err(), "{url}");
        }
        assert_eq!(
            sentry_next(
                &[(
                    "link".into(),
                    format!("<{sentry}?cursor=0:100:0>; rel=\"next\"; results=\"false\"")
                )],
                sentry
            )
            .unwrap(),
            None
        );
        let cloud = "https://cloud.laravel.com/api/applications";
        assert_eq!(
            cloud_next(&json!({"links":{"next":format!("{cloud}?page=2")}}), cloud).unwrap(),
            Some(format!("{cloud}?page=2"))
        );
        for url in [
            "https://evil.example/api/applications?page=2",
            "https://cloud.laravel.com/api/applications/other/environments?page=2",
            "https://cloud.laravel.com/api/applications?page=0",
            "https://cloud.laravel.com/api/applications?page=2&token=secret",
            "https://cloud.laravel.com/api/applications?page=2&page=3",
        ] {
            assert!(
                cloud_next(&json!({"links":{"next":url}}), cloud).is_err(),
                "{url}"
            );
        }
        assert!(
            Page::first(&Page::default(), "base".into(), Some("base".into()))
                .request()
                .is_err()
        );
        let mut page = Page::first(&Page::default(), "base".into(), Some("next".into()));
        assert!(page.advance("next".into(), Some("next".into())).is_err());
        page.pages = MAX_PAGES;
        assert!(page.request().is_err());
    }
    #[test]
    fn explicit_pagination_merges_resources_preserves_keys_and_rejects_stale_rows() {
        let h = MockHost::install().with_capabilities(&[
            "project.read",
            "storage",
            "secrets",
            "net:sentry.io",
            "net:cloud.laravel.com",
        ]);
        h.set_secret(SENTRY_SECRET, "sentry-private");
        h.set_secret(CLOUD_SECRET, "cloud-private");
        h.answer_next_prompt(Some(&["acme"]));
        let mut state = State::default();
        state.configure_sentry(&h.invocation()).unwrap();
        h.on_fetch(|r|{
            let (value,headers)=match r.url.as_str() {
                "https://sentry.io/api/0/organizations/acme/issues/?limit=100"=>(json!([{"id":"1","title":"First","status":"unresolved","project":{"slug":"app"}}]),vec![("Link".into(),"<https://sentry.io/api/0/organizations/acme/issues/?cursor=0:100:0>; rel=\"next\"; results=\"true\"".into())]),
                "https://sentry.io/api/0/organizations/acme/issues/?limit=100&cursor=0:100:0"=>(json!([{"id":"1","title":"Updated","status":"resolved","project":{"slug":"app"}},{"id":"2","title":"Second","status":"unresolved","project":{"slug":"app"}}]),vec![]),
                "https://cloud.laravel.com/api/applications"=>(json!({"data":[{"id":"a","type":"applications","attributes":{"name":"First app","slug":"a"}}],"links":{"next":"https://cloud.laravel.com/api/applications?page=2"}}),vec![]),
                "https://cloud.laravel.com/api/applications?page=2"=>(json!({"data":[{"id":"b","type":"applications","attributes":{"name":"Second app","slug":"b"}}],"links":{"next":null}}),vec![]),
                "https://cloud.laravel.com/api/applications/b/environments"=>(json!({"data":[{"id":"e1","type":"environments","attributes":{"name":"Staging","slug":"staging"}}],"links":{"next":"https://cloud.laravel.com/api/applications/b/environments?page=2"}}),vec![]),
                "https://cloud.laravel.com/api/applications/b/environments?page=2"=>(json!({"data":[{"id":"e2","type":"environments","attributes":{"name":"Production","slug":"production"}}],"links":{"next":null}}),vec![]),
                _=>panic!("unexpected request {}",r.url),
            };Ok(HttpResponse{status:200,headers,body:value.to_string().into_bytes()})
        });
        state.refresh_sentry(&h.invocation()).unwrap();
        state.refresh_cloud(&h.invocation()).unwrap();
        assert_eq!(state.pagination(&h.invocation())["sentry"], true);
        assert_eq!(state.pagination(&h.invocation())["cloud"], true);
        let first_key = state
            .rows(&h.invocation())
            .into_iter()
            .find(|r| r["cells"][0] == "Sentry")
            .unwrap()["key"]
            .as_str()
            .unwrap()
            .to_owned();
        state.load_more_sentry(&h.invocation()).unwrap();
        state.load_more_cloud(&h.invocation()).unwrap();
        assert_eq!(state.pagination(&h.invocation())["sentryCount"], 2);
        assert_eq!(state.pagination(&h.invocation())["cloudCount"], 2);
        assert!(
            state
                .rows(&h.invocation())
                .iter()
                .any(|r| r["key"] == first_key && r["cells"][1] == "Updated")
        );
        let app_key = resource_key("cloud-app", "/project", 1, "b");
        state.cloud_environments(&h.invocation(), &app_key).unwrap();
        assert_eq!(state.pagination(&h.invocation())["environments"], true);
        state.load_more_environments(&h.invocation()).unwrap();
        assert_eq!(state.pagination(&h.invocation())["environmentCount"], 2);
        let count = h.requests().len();
        assert!(state.load_more_sentry(&h.invocation()).is_err());
        assert!(state.load_more_cloud(&h.invocation()).is_err());
        assert!(state.load_more_environments(&h.invocation()).is_err());
        assert_eq!(h.requests().len(), count);
        state.refresh_cloud(&h.invocation()).unwrap();
        let count = h.requests().len();
        assert!(state.cloud_environments(&h.invocation(), &app_key).is_err());
        assert_eq!(h.requests().len(), count);
        state.refresh_sentry(&h.invocation()).unwrap();
        let count = h.requests().len();
        assert!(state.sentry_details(&h.invocation(), &first_key).is_err());
        assert_eq!(h.requests().len(), count);
        state.forget_sentry(&h.invocation()).unwrap();
        assert!(host::secret_is_set(SENTRY_SECRET));
        assert!(host::storage::get(&org_key("/project")).unwrap().is_none());
        assert!(state.refresh_sentry(&h.invocation()).is_err());
    }
    #[test]
    fn invalid_next_page_and_failed_load_more_leave_the_snapshot_unchanged() {
        let h = MockHost::install().with_capabilities(&[
            "project.read",
            "storage",
            "secrets",
            "net:cloud.laravel.com",
        ]);
        h.set_secret(CLOUD_SECRET, "private");
        let mut state = State::default();
        h.on_fetch(|_|Ok(HttpResponse{status:200,headers:vec![],body:json!({"data":[{"id":"a","type":"applications","attributes":{"name":"A","slug":"a"}}],"links":{"next":"https://cloud.laravel.com/api/applications?page=2"}}).to_string().into_bytes()}));
        state.refresh_cloud(&h.invocation()).unwrap();
        let before = state.rows(&h.invocation());
        let page = state.pagination(&h.invocation());
        h.on_fetch(|_|Ok(HttpResponse{status:200,headers:vec![],body:json!({"data":[{"id":"b","type":"applications","attributes":{"name":"B","slug":"b"}}],"links":{"next":"https://attacker.example/steal"}}).to_string().into_bytes()}));
        assert!(state.load_more_cloud(&h.invocation()).is_err());
        assert_eq!(state.rows(&h.invocation()), before);
        assert_eq!(state.pagination(&h.invocation()), page);
        assert!(h.requests().iter().all(|r| {
            r.url
                .starts_with("https://cloud.laravel.com/api/applications")
        }));
        h.on_fetch(|_| Err(Error::Failed("private credential echoed".into())));
        let error = state
            .load_more_cloud(&h.invocation())
            .err()
            .unwrap()
            .to_string();
        assert!(!error.contains("credential echoed"));
        assert_eq!(state.rows(&h.invocation()), before);
        h.set_project_root(Some("/other"));
        let count = h.requests().len();
        assert!(state.load_more_cloud(&h.invocation()).is_err());
        assert_eq!(h.requests().len(), count);
    }
}

// Exact authored failures are localized only at native UI delivery boundaries.
fn ui_error(error: Error) -> Error {
    let translate = |message: String| -> String {
        match message.as_str() {
            "External inventory reached 20 pages; refresh or narrow the service scope." => {
                crate::ui_text::t("runtime.error.9ee7deed4647", &message)
            }
            "No more captured pages are available." => {
                crate::ui_text::t("runtime.error.9162d6217c66", &message)
            }
            "External pagination repeated a previous page." => {
                crate::ui_text::t("runtime.error.f0fd4a407a7f", &message)
            }
            "External pagination changed origin or resource." => {
                crate::ui_text::t("runtime.error.3a2044b9842d", &message)
            }
            "External pagination query is too large." => {
                crate::ui_text::t("runtime.error.ba8f1dc6c4db", &message)
            }
            "Malformed external pagination query." => {
                crate::ui_text::t("runtime.error.3a70f7e87e45", &message)
            }
            "Duplicate external pagination parameter." => {
                crate::ui_text::t("runtime.error.930fbb2d9ee3", &message)
            }
            "External pagination header exceeds its limit." => {
                crate::ui_text::t("runtime.error.4e08efa0c1a7", &message)
            }
            "Duplicate Sentry next-page relation." => {
                crate::ui_text::t("runtime.error.5d6c8bc6884a", &message)
            }
            "Malformed Sentry next-page URL." => {
                crate::ui_text::t("runtime.error.4ecd744da350", &message)
            }
            "Invalid Sentry cursor." => crate::ui_text::t("runtime.error.b3f41c412600", &message),
            "Unsupported Sentry pagination parameter." => {
                crate::ui_text::t("runtime.error.a13f680ab228", &message)
            }
            "Sentry next-page URL has no cursor." => {
                crate::ui_text::t("runtime.error.6ebdfff4b7aa", &message)
            }
            "Invalid Cloud next-page link." => {
                crate::ui_text::t("runtime.error.a3539abe2d0a", &message)
            }
            "Unsupported Cloud pagination parameter." => {
                crate::ui_text::t("runtime.error.7962dfcd7479", &message)
            }
            "Invalid Cloud page number." => {
                crate::ui_text::t("runtime.error.4c369b73cb63", &message)
            }
            "Missing Cloud page number." => {
                crate::ui_text::t("runtime.error.ef41ed97797d", &message)
            }
            "External inventory exceeds 2000 resources." => {
                crate::ui_text::t("runtime.error.c7994b5e25c6", &message)
            }
            "Open a Laravel project first." => crate::ui_text::t("process.error0", &message),
            "External service returned an oversized value." => {
                crate::ui_text::t("runtime.error.e03b9b37c620", &message)
            }
            "External response has an invalid resource ID." => {
                crate::ui_text::t("runtime.error.f99bd0952800", &message)
            }
            "Expected at most 100 Sentry issues." => {
                crate::ui_text::t("runtime.error.3645d6cb72e8", &message)
            }
            "Duplicate Sentry issue ID." => {
                crate::ui_text::t("runtime.error.ed8c2501a782", &message)
            }
            "Expected a Cloud JSON:API resource page of at most 100 records." => {
                crate::ui_text::t("runtime.error.40162270f212", &message)
            }
            "Cloud resource has the wrong type." => {
                crate::ui_text::t("runtime.error.014426ef869b", &message)
            }
            "Duplicate Cloud resource ID." => {
                crate::ui_text::t("runtime.error.a7cf2211284a", &message)
            }
            "Configure this project's Sentry organization first." => {
                crate::ui_text::t("runtime.error.e77590a6735c", &message)
            }
            _ => crate::ui_text::owned_error(&message, UI_ERROR_TEMPLATES).unwrap_or(message),
        }
    };
    match error {
        Error::Invalid(message) => Error::Invalid(translate(message)),
        Error::Failed(message) => Error::Failed(translate(message)),
        other => other,
    }
}

// Explicit formats emitted by this module; only UI delivery uses locale state.
const UI_ERROR_TEMPLATES: &[(&str, &str)] = &[(
    "final.external_services.missingField",
    "External response is missing {name}.",
)];
