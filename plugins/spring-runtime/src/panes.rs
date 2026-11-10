//! Read-only observations. Source navigation is a unique qualified-name candidate only.
use super::*;
use openidea_plugin::Range;
use openidea_plugin::details::{Details, Pane, Tab};
use openidea_plugin::inlay_hints::{InlayHint, InlayHints};
const VIEW: &str = "runtime-snapshot";
const MAX_ROWS: usize = 512;
#[derive(Clone)]
struct Record {
    title: String,
    endpoint: &'static str,
    body: String,
    target: Option<Target>,
}
#[derive(Default)]
pub(crate) struct State {
    epoch: u64,
    scope: Option<Scope>,
    at: u64,
    rows: BTreeMap<String, Record>,
    selected: Option<String>,
}
impl State {
    pub(crate) fn clear(&mut self) {
        self.scope = None;
        self.rows.clear();
        self.selected = None;
    }
}
fn qualified(name: &str) -> bool {
    name.len() <= 4096
        && name.contains('.')
        && name.split('.').all(|p| {
            !p.is_empty()
                && p.bytes().enumerate().all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || b == b'$' || i > 0 && b.is_ascii_digit())
        })
}
fn source_owned(path: &str, root: &str) -> bool {
    path.len() <= 4096
        && !path.chars().any(char::is_control)
        && !path.contains('\\')
        && !path.split('/').any(|part| matches!(part, "." | ".."))
        && path.strip_prefix(root.trim_end_matches('/')).is_some_and(|tail| tail.starts_with('/'))
        && host::project_of(path).and_then(|project| project.root().ok()).as_deref() == Some(root)
}
fn known(value: &str) -> &str {
    if value.is_empty() { "unknown / not reported" } else { value }
}
fn candidate(ty: &str, method: Option<String>) -> Option<Target> {
    qualified(ty).then(|| Target::Type(ty.into(), method))
}
fn row(title: &str, endpoint: &'static str, body: String, target: Option<Target>) -> Record {
    Record { title: presentation(title, 256), endpoint, body: presentation(&body, 8192), target }
}
fn endpoint_presentation(endpoint: &str) -> (&str, &str) {
    match endpoint {
        "beans" => ("Beans", "spring-boot"),
        "health" => ("Health", "info"),
        "env" => ("Environment", "settings"),
        "conditions" => ("Auto-configuration", "spring-config"),
        "mappings" => ("Mappings", "web"),
        "configprops" => ("Configuration Properties", "property"),
        "scheduledtasks" => ("Scheduled Tasks", "clock"),
        "loggers" => ("Loggers", "settings"),
        "caches" => ("Caches", "database"),
        _ => (endpoint, "info"),
    }
}
fn records(s: &Snapshot) -> Vec<Record> {
    let mut out = Vec::new();
    macro_rules! add {
        ($value:expr) => {
            if out.len() < MAX_ROWS {
                out.push($value);
            }
        };
    }
    // Round-robin endpoint sections are independently capped, so one huge endpoint
    // cannot hide every other observed endpoint.
    for b in s.beans.iter().take(64) {
        add!(row(
            &b.name,
            "beans",
            format!(
                "Bean: {}\nContext: {}\nReported type: {}\nScope: {}\nDependencies (names, not resolved owners): {}",
                known(&b.name),
                known(&b.context),
                known(&b.ty),
                known(&b.scope),
                if b.deps.is_empty() {
                    "none reported".into()
                } else {
                    b.deps.iter().take(64).map(|v| presentation(v, 256)).collect::<Vec<_>>().join(", ")
                }
            ),
            candidate(&b.ty, None)
        ));
    }
    for (path, status) in s.health.iter().take(32) {
        add!(row(
            if path.is_empty() { "Application health" } else { path },
            "health",
            format!(
                "Component: {}\nReported status: {}\nHealth details beyond status are not retained in this model.",
                if path.is_empty() { "application" } else { path },
                known(status)
            ),
            None
        ));
    }
    for source in s.sources.iter().take(32) {
        add!(row(
            &source.name,
            "env",
            format!(
                "Property source: {}\nProperty count: {}\nProfiles: {}\nValues are omitted from this detail pane. Source-file identity and current effective values are unverified.",
                known(&source.name),
                source.props.len(),
                s.profiles.iter().take(32).map(|v| presentation(v, 128)).collect::<Vec<_>>().join(", ")
            ),
            None
        ));
    }
    for c in s.conditions.iter().take(64) {
        add!(row(
            &c.key,
            "conditions",
            format!(
                "Condition key: {}\nReported match: {}\nMessages:\n{}\n\nCondition keys are not resolved Java owners.",
                known(&c.key),
                c.matched,
                c.messages.iter().take(32).map(|m| presentation(m, 256)).collect::<Vec<_>>().join("\n")
            ),
            None
        ));
    }
    for m in s.mappings.iter().take(64) {
        add!(row(
            &m.predicate,
            "mappings",
            format!(
                "Group: {}\nPredicate: {}\nHandler: {}\nDeclared method: {}\nOverload signature / current source identity: unknown.",
                known(&m.group),
                known(&m.predicate),
                known(&m.handler),
                m.method.as_ref().map(|(c, m)| format!("{c}#{m}")).unwrap_or_else(|| "not reported".into())
            ),
            m.method.as_ref().and_then(|(c, m)| candidate(c, Some(m.clone())))
        ));
    }
    for p in s.configprops.iter().take(64) {
        add!(row(
            &p.prefix,
            "configprops",
            format!(
                "Prefix: {}\nReported type: {}\nRetained property count: {}\nValues omitted. Binding to the current source declaration is unverified.",
                known(&p.prefix),
                known(&p.ty),
                p.props.len()
            ),
            candidate(&p.ty, None)
        ));
    }
    for t in s.scheduled.iter().take(64) {
        add!(row(
            &t.target,
            "scheduledtasks",
            format!(
                "Target: {}\nKind: {}\nSchedule: {}\nTarget ownership and current execution state: unknown.",
                known(&t.target),
                known(&t.kind),
                known(&t.detail)
            ),
            None
        ));
    }
    for l in s.loggers.iter().take(64) {
        add!(row(
            &l.name,
            "loggers",
            format!(
                "Logger: {}\nConfigured level: {}\nEffective level at fetch: {}\nCurrent level: unknown.",
                known(&l.name),
                known(&l.configured),
                known(&l.effective)
            ),
            None
        ));
    }
    for c in s.caches.iter().take(64) {
        add!(row(
            &c.name,
            "caches",
            format!(
                "Cache: {}\nManager: {}\nReported target: {}\nContents, statistics and current state: unknown.",
                known(&c.name),
                known(&c.manager),
                known(&c.target)
            ),
            None
        ));
    }
    out
}
impl Runtime {
    pub(crate) fn render_panes(&mut self, ctx: &Invocation) -> Result<View, Error> {
        let custom = ui_kit::web::is_rendering();
        if !custom { self.panes.clear(); }
        let Some(project) = ctx.project() else {
            self.panes.clear();
            return Ok(View::message("Open a project to inspect its cached snapshot."));
        };
        let scope = match Scope::current(&project).and_then(|scope| { controls::verify(&project, &scope)?; Ok(scope) }) {
            Ok(scope) => scope,
            Err(error) => { self.panes.clear(); return Ok(View::message(&error.to_string())); }
        };
        self.load_scope(scope.clone());
        let Some(snap) = &self.snapshot else {
            return Ok(View::message("No cached snapshot. Refresh in Spring Runtime first; this view never fetches."));
        };
        let at = snap.fetched_at;
        if self.panes.scope.as_ref() != Some(&scope) || self.panes.at != at || self.panes.rows.is_empty() {
            self.panes.clear();
            self.panes.epoch = self.panes.epoch.checked_add(1).ok_or(Error::Stale)?;
            self.panes.scope = Some(scope);
            self.panes.at = at;
            for (i, record) in records(snap).into_iter().enumerate() {
                self.panes.rows.insert(format!("snapshot:{}:{i}", self.panes.epoch), record);
            }
        }
        let mut items: Vec<_> = self.panes.rows.iter().map(|(key, record)| {
            let (section, icon) = endpoint_presentation(record.endpoint);
            TreeItem::new(key, &record.title).detail(section).icon(icon)
        }).collect();
        items.sort_by_key(|item| item.key.rsplit(':').next().and_then(|index|index.parse::<usize>().ok()).unwrap_or(usize::MAX));
        let body = if custom {
            let mut rows = Vec::new();
            for item in items {
                // Observation/provenance are lazy and bounded, as in the native
                // detail provider: only the authoritative selected row gets them.
                let details: Vec<_> = if self.panes.selected.as_deref() == Some(item.key.as_str()) {
                    self.detail(VIEW, &item.key, ctx)?.into_iter().filter_map(|tab| {
                        match tab.body { Pane::Text(value) => Some(serde_json::json!({"label":tab.label,"value":value})), _ => None }
                    }).collect()
                } else { Vec::new() };
                let navigable = self.panes.rows.get(&item.key).is_some_and(|record|record.target.is_some());
                rows.push(serde_json::json!({"key":item.key,"label":item.label,"detail":item.detail,"icon":item.icon,
                    "details":details,"activationLabel":"Open Source","activationIcon":"file","activationEnabled":navigable}));
            }
            Body::Other { kind: "tree".into(), data: serde_json::json!({"items":rows}).to_string() }
        } else { Body::Tree(items) };
        Ok(View::new(body)
            .button(Button::new("runtime-overview", "Overview").icon("spring-boot"))
            .button(Button::new("runtime-graph", "Bean Graph").icon("hierarchy"))
            .button(Button::new("runtime-controls", "Refresh Controls").icon("settings"))
            .status(&format!("Cached observations · {} · Unix {} · bounded sections (up to 64 rows each; health/env 32) · values omitted · Show Details for Observation / Provenance; activate to navigate to a unique source candidate",age(at),at)))
    }
    pub(crate) fn select_pane(&mut self, key: &str, ctx: &Invocation) -> Result<(), Error> {
        self.pane_record(key, ctx)?;
        self.panes.selected = Some(key.to_owned());
        Ok(())
    }
    fn pane_record(&self, key: &str, ctx: &Invocation) -> Result<&Record, Error> {
        let scope = Scope::current(&ctx.project().ok_or(Error::Stale)?)?;
        if self.scope.as_ref() != Some(&scope)
            || self.panes.scope.as_ref() != Some(&scope)
            || self.snapshot.as_ref().is_none_or(|s| s.fetched_at != self.panes.at)
        {
            return Err(Error::Stale);
        }
        self.panes.rows.get(key).ok_or(Error::Stale)
    }
    pub(crate) fn activate_pane(&mut self, key: &str, ctx: &Invocation) -> Result<(), Error> {
        let target = self.pane_record(key, ctx)?.target.clone();
        let Some(Target::Type(class, method)) = target else {
            return Ok(());
        };
        let project = ctx.project().ok_or(Error::Stale)?;
        let scope = Scope::current(&project)?;
        self.java.reset();
        let index = self.java.get(&project, COMMAND_BUDGET);
        if !index.fresh() || index.iter().any(|(_, types)| types.iter().any(|t| t.fqn == "\0incomplete")) {
            return Err(Error::Failed("Source index incomplete or stale; no candidate opened.".into()));
        }
        let hits: Vec<_> =
            index.iter().filter(|(path, _)| source_owned(path, &scope.root))
                .flat_map(|(path, types)| types.iter().filter(|t| t.fqn == class).map(move |t| (path, t))).take(2).collect();
        let [(path, ty)] = hits.as_slice() else {
            return Err(Error::Failed("No unique exact qualified source candidate; no file opened.".into()));
        };
        let (line, column) = if let Some(method) = method {
            let methods: Vec<_> = ty.methods.iter().filter(|m| m.0 == method).take(2).collect();
            let [m] = methods.as_slice() else {
                return Err(Error::Failed("Method missing or overloaded; no source candidate opened.".into()));
            };
            (m.1, m.2)
        } else {
            (ty.line, ty.column)
        };
        if Scope::current(&project)? != scope || !source_owned(path, &scope.root) { return Err(Error::Stale); }
        host::open_location(path, line, column)
    }
}
impl Details for Runtime {
    fn detail(&mut self, view: &str, key: &str, ctx: &Invocation) -> Result<Vec<Tab>, Error> {
        if view != VIEW {
            return Ok(Vec::new());
        }
        let Ok(record) = self.pane_record(key, ctx) else {
            return Ok(vec![Tab::new(
                "expired",
                "Snapshot changed",
                Pane::Message("Selection expired. Refresh/select a current cached row.".into()),
            )]);
        };
        let scope = self.panes.scope.as_ref().ok_or(Error::Stale)?;
        let provenance = format!(
            "Project: {}\nActuator URL: {}\nAuthentication mode: {} (credential not shown)\nEndpoint: {}\nFetched Unix: {} ({})\n\nRead-only cached Actuator observation. Not breakpoint evaluation or current application state. Current source/runtime build identity: unknown.\nSource navigation: {}\nLast refresh error: {}\nMissing endpoints: {}",
            scope.root,
            scope.url,
            scope.auth,
            record.endpoint,
            self.panes.at,
            age(self.panes.at),
            if record.target.is_some() {
                "exact qualified-name candidate only; requires unique class and non-overloaded method"
            } else {
                "no established source owner"
            },
            self.error.as_deref().unwrap_or("none recorded"),
            self.snapshot.as_ref().map(|s| s.missing.join(", ")).unwrap_or_default()
        );
        Ok(vec![
            Tab::new("observation", "Observation", Pane::Text(record.body.clone())),
            Tab::new("provenance", "Provenance", Pane::Text(presentation(&provenance, 16384))),
        ])
    }
}
impl InlayHints for Runtime {
    fn inlay_hints(&mut self, doc: &Document, range: Range) -> Vec<InlayHint> {
        if doc.text.len() > 128 << 10
            || !matches!(doc.language.as_str(), "properties" | "yaml")
            || !config::is_config_file(&doc.path)
            || !host::setting_bool("snapshotInlays").unwrap_or(true)
        {
            return Vec::new();
        }
        let Some(scope) = spring_text::index::project_of(&doc.path).and_then(|p| Scope::current(&p).ok()) else {
            return Vec::new();
        };
        if !self.loaded || self.scope.as_ref() != Some(&scope) {
            return Vec::new();
        }
        let Some(s) = &self.snapshot else {
            return Vec::new();
        };
        if !s.has("env") {
            return Vec::new();
        }
        if s.sources.len() > 256 || s.sources.iter().map(|source| source.props.len()).sum::<usize>() > 8192 {
            return Vec::new();
        }
        let mut lookup: BTreeMap<&str, Option<(&str, &str)>> = BTreeMap::new();
        for source in &s.sources {
            let mut local: BTreeMap<&str, Option<&str>> = BTreeMap::new();
            for (key, value) in &source.props {
                if key.len() > 4096 {
                    continue;
                }
                local.entry(key).and_modify(|value| *value = None).or_insert(Some(value));
            }
            for (key, value) in local {
                lookup.entry(key).or_insert_with(|| value.map(|value| (source.name.as_str(), value)));
            }
        }
        config::keys(&doc.path,&doc.text).into_iter().take(200).filter_map(|key|{
            if key.key.len()>4096 {return None;}
            let p=position(&doc.text,key.end);if p<range.start || p>range.end{return None;}
            // Exact key only: no relaxed aliases, environment transformations,
            // inferred profile selection or current source-file origin.
            let (source,value)=lookup.get(key.key.as_str()).copied().flatten()?;
            Some(InlayHint::new(p,if value==MASK{"snapshot: redacted"}else{"snapshot: observed"}).padding_left().tooltip(presentation(&format!("Cached /env observation for exact key {}\nProperty source: {}\nFetched Unix {} ({})\nActuator: {}\nValues omitted. Active profile, source-file/runtime identity and current effective value are unverified.",key.key,presentation(source,1024),s.fetched_at,age(s.fetched_at),scope.url),4096)))
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, text::offset};
    fn setup() -> (MockHost, Runtime) {
        let host = MockHost::install();
        let scope = Scope::current(&host.invocation().project().unwrap()).unwrap();
        let snapshot = Snapshot {
            url: scope.url.clone(),
            fetched_at: 123,
            beans: vec![Bean {
                name: "service".into(),
                context: "app".into(),
                ty: "com.acme.Service".into(),
                scope: "singleton".into(),
                deps: vec!["other".into()],
                dependencies_verified:true,
            }],
            sources: vec![Source {
                name: "test source".into(),
                props: vec![("demo.key".into(), "private-value".into()), ("demo.secret".into(), MASK.into())],
            }],
            ..Snapshot::default()
        };
        let runtime = Runtime { snapshot: Some(snapshot), scope: Some(scope), loaded: true, ..Runtime::default() };
        (host, runtime)
    }
    fn doc(text: &str, language: &str) -> Document {
        Document {
            path: format!("/project/src/main/resources/application.{}", if language == "yaml" { "yml" } else { "properties" }),
            language: language.into(),
            version: 5,
            text: text.into(),
        }
    }
    fn all(doc: &Document) -> Range {
        spring_text::range(&doc.text, 0, doc.text.len())
    }
    #[test]
    fn command_and_hub_button_reveal_only_the_supporting_native_view() {
        let (host, mut runtime) = setup();
        let ctx = host.invocation();
        runtime.run_command("snapshot-details", &ctx, None).unwrap();
        runtime.handle("runtime", ViewEvent::Button("snapshot-details".into()), &ctx).unwrap();
        assert_eq!(host.revealed_views(), vec!["runtime", "runtime"]);
        assert_eq!(host.fetches(), 0);
        host.set_project_root(None);
        assert!(runtime.run_command("snapshot-details", &host.invocation(), None).is_err());
    }
    #[test]
    fn grouped_observations_keep_lazy_details_and_source_navigation_through_the_real_bridge() {
        let (host, mut runtime) = setup();
        host.add_file("src/Service.java", "package com.acme; class Service {}");
        crate::reveal_view(VIEW).unwrap();
        let request = |runtime: &mut Runtime, operation: Value| -> Value {
            let mut operation = operation;
            operation["channel"] = serde_json::json!("ui-kit");
            operation["sequence"] = serde_json::json!(1);
            operation["project"] = serde_json::json!("/project");
            operation["targetView"] = serde_json::json!(VIEW);
            runtime.handle(crate::VIEW, ViewEvent::Message(operation.to_string()), &host.invocation()).unwrap();
            serde_json::from_str(&host.view_messages().last().unwrap().1).unwrap()
        };
        let rendered = request(&mut runtime, serde_json::json!({"op":"render"}));
        assert_eq!(rendered["model"]["activeView"], VIEW);
        let rows = rendered["model"]["body"]["items"].as_array().unwrap();
        let key = rows[0]["key"].as_str().unwrap().to_owned();
        assert!(rows.iter().all(|row|row["details"].as_array().unwrap().is_empty()), "no eager large per-row detail payload");
        let selected = request(&mut runtime, serde_json::json!({"op":"event","event":{"kind":"select","key":key}}));
        assert!(selected.get("error").is_none(), "{selected}");
        let rows = selected["model"]["body"]["items"].as_array().unwrap();
        assert_eq!(rows[0]["key"], key, "bridge validation render keeps the current row identity");
        let details = rows[0]["details"].as_array().unwrap();
        assert_eq!(details.len(), 2);
        assert_eq!(details[0]["label"], "Observation");
        assert!(details[0]["value"].as_str().unwrap().contains("com.acme.Service"));
        assert_eq!(details[1]["label"], "Provenance");
        assert!(details[1]["value"].as_str().unwrap().contains("Endpoint: beans"));
        assert!(rows[1]["details"].as_array().unwrap().is_empty());
        assert!(!selected.to_string().contains("private-value"));
        let activated = request(&mut runtime, serde_json::json!({"op":"event","event":{"kind":"activate","key":key}}));
        assert!(activated.get("error").is_none(), "{activated}");
        assert_eq!(host.opened_locations().len(), 1);
        runtime.panes.clear();
        let stale = request(&mut runtime, serde_json::json!({"op":"event","event":{"kind":"activate","key":key}}));
        assert!(stale.get("error").is_some());
        assert_eq!(host.opened_locations().len(), 1);
        assert_eq!(host.fetches(), 0);
    }

    #[test]
    fn details_are_cached_bounded_and_explicit_about_provenance_unknowns() {
        let (host, mut runtime) = setup();
        let ctx = host.invocation();
        let view = runtime.render_panes(&ctx).unwrap();
        let Body::Tree(items) = view.body else { panic!() };
        assert_eq!(items.len(), 2);
        let tabs = runtime.detail(VIEW, &items[0].key, &ctx).unwrap();
        assert_eq!(tabs.len(), 2);
        let Pane::Text(observation) = &tabs[0].body else { panic!() };
        assert!(observation.contains("com.acme.Service"));
        assert!(observation.contains("not resolved owners"));
        let Pane::Text(provenance) = &tabs[1].body else { panic!() };
        for expected in ["/project", "Endpoint: beans", "Unix: 123", "current application state", "identity: unknown"] {
            assert!(provenance.contains(expected), "{expected}: {provenance}");
        }
        let tabs = runtime.detail(VIEW, &items[1].key, &ctx).unwrap();
        assert!(!format!("{tabs:?}").contains("private-value"));
        assert_eq!(host.fetches(), 0);
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn stale_keys_and_cross_project_or_url_scope_never_show_previous_rows() {
        for mode in 0..4 {
            let (host, mut runtime) = setup();
            let ctx = host.invocation();
            let view = runtime.render_panes(&ctx).unwrap();
            let Body::Tree(items) = view.body else { panic!() };
            let key = items[0].key.clone();
            match mode {
                0 => {
                    runtime.render_panes(&ctx).unwrap();
                }
                1 => host.set_project_root(Some("/other")),
                2 => host.set_setting("actuatorUrl", "\"http://localhost:9090/actuator\""),
                _ => runtime.panes.clear(),
            }
            let tabs = runtime.detail(VIEW, &key, &host.invocation()).unwrap();
            assert!(matches!(tabs[0].body, Pane::Message(_)));
            assert!(runtime.activate_pane(&key, &host.invocation()).is_err());
            assert!(host.opened_locations().is_empty());
        }
    }
    #[test]
    fn source_navigation_uses_only_unique_fqn_and_nonoverloaded_method() {
        for mode in 0..4 {
            let (host, mut runtime) = setup();
            host.add_file("src/Service.java", "package com.acme; class Service {void run(){} }");
            let ctx = host.invocation();
            if mode == 1 {
                host.add_file("other/Service.java", "package com.acme; class Service {}");
            }
            if mode == 2 {
                host.add_file("src/Service.java", "package other; class Service {}");
            }
            if mode == 3 {
                host.add_file("src/Service.java", "package com.acme; class Service {void run(){} void run(int x){} }");
                runtime.snapshot.as_mut().unwrap().mappings.push(Mapping {
                    predicate: "GET /".into(),
                    method: Some(("com.acme.Service".into(), "run".into())),
                    ..Mapping::default()
                });
            }
            runtime.render_panes(&ctx).unwrap();
            let key =
                runtime.panes.rows.iter().find(|(_, r)| r.endpoint == if mode == 3 { "mappings" } else { "beans" }).unwrap().0.clone();
            let result = runtime.activate_pane(&key, &ctx);
            assert_eq!(result.is_ok(), mode == 0, "mode {mode}");
            assert_eq!(host.opened_locations().len(), usize::from(mode == 0));
            assert_eq!(host.fetches(), 0);
        }
    }
    #[test]
    fn source_candidate_never_belongs_to_another_open_nested_project() {
        for outer in [false, true] {
            let (host, mut runtime) = setup();
            host.add_project("/project/nested");
            host.add_file("/project/nested/Service.java", "package com.acme; class Service {}");
            if outer { host.add_file("/project/src/Service.java", "package com.acme; class Service {}"); }
            let ctx = host.invocation();
            runtime.render_panes(&ctx).unwrap();
            let key = runtime.panes.rows.iter().find(|(_, row)| row.endpoint == "beans").unwrap().0.clone();
            assert_eq!(runtime.activate_pane(&key, &ctx).is_ok(), outer);
            assert_eq!(host.opened_locations().len(), usize::from(outer));
            assert!(host.opened_locations().iter().all(|location| !location.0.contains("/nested/")));
            assert_eq!(host.fetches(), 0);
        }
    }
    #[test]
    fn oversized_source_does_not_silently_hide_ambiguous_owner() {
        let (host, mut runtime) = setup();
        host.add_file("src/Service.java", "package com.acme; class Service {}");
        host.add_file("src/Large.java", &" ".repeat((512 << 10) + 1));
        let ctx = host.invocation();
        runtime.render_panes(&ctx).unwrap();
        let key = runtime.panes.rows.keys().next().unwrap().clone();
        assert!(runtime.activate_pane(&key, &ctx).is_err());
        assert!(host.opened_locations().is_empty());
    }
    #[test]
    fn inlays_use_cached_exact_keys_and_report_age_without_values_or_active_claims() {
        let (host, mut runtime) = setup();
        let d = doc("# 😀\ndemo.key=local\ndemo.secret=local\nDEMO_KEY=no\n", "properties");
        let hints = runtime.inlay_hints(&d, all(&d));
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0].label, "snapshot: observed");
        assert_eq!(hints[1].label, "snapshot: redacted");
        assert_eq!(offset(&d.text, hints[0].position), d.text.find("=local"));
        let text = format!("{hints:?}");
        assert!(text.contains("Unix 123"));
        assert!(text.contains("unverified"));
        assert!(!text.contains("private-value"));
        assert_eq!(host.fetches(), 0);
        let yaml = doc("# 😀\ndemo:\n  key: local\n", "yaml");
        let hints = runtime.inlay_hints(&yaml, all(&yaml));
        assert_eq!(hints.len(), 1);
        assert_eq!(offset(&yaml.text, hints[0].position), yaml.text.find(": local"));
    }
    #[test]
    fn cold_disabled_missing_ambiguous_and_other_scope_inlays_are_quiet() {
        for mode in 0..7 {
            let (host, mut runtime) = setup();
            let d = doc("demo.key=local", "properties");
            match mode {
                0 => runtime.loaded = false,
                1 => host.set_setting("snapshotInlays", "false"),
                2 => runtime.snapshot.as_mut().unwrap().missing.push("env (404)".into()),
                3 => runtime.snapshot.as_mut().unwrap().sources[0].props.push(("demo.key".into(), "duplicate".into())),
                4 => host.set_project_root(Some("/other")),
                5 => runtime.scope = None,
                _ => runtime.snapshot.as_mut().unwrap().sources[0].props.resize(8193, ("overflow".into(), "".into())),
            }
            assert!(runtime.inlay_hints(&d, all(&d)).is_empty(), "mode {mode}");
            assert_eq!(host.fetches(), 0);
        }
    }
    #[test]
    fn every_endpoint_has_readonly_observations_and_record_limits() {
        let mut s = Snapshot::default();
        s.beans = vec![Bean::default(); 100];
        s.health = vec![("".into(), "".into()); 100];
        s.sources = vec![Source::default(); 100];
        s.conditions = vec![Condition::default(); 100];
        s.mappings = vec![Mapping::default(); 100];
        s.configprops = vec![PropsBean::default(); 100];
        s.scheduled = vec![Scheduled::default(); 100];
        s.loggers = vec![Logger::default(); 100];
        s.caches = vec![Cache::default(); 100];
        let rows = records(&s);
        assert_eq!(rows.len(), 512);
        for endpoint in ENDPOINTS {
            assert!(rows.iter().any(|r| r.endpoint == endpoint), "{endpoint}");
        }
        assert!(rows.iter().all(|r| r.body.len() <= 8192));
        assert!(rows.iter().all(|r| r.target.is_none()));
    }
}
