//! Explicit bounded Artisan snapshots. Rendering and agent reads never bootstrap PHP.
use crate::{artisan_metadata as metadata, artisan_runner::Runner};
use openidea_plugin::{Error, Invocation, Project, host, views::Body};
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[path = "runtime_database.rs"]
mod database;
#[path = "runtime_errors.rs"]
pub(super) mod errors;
#[path = "runtime_queue_tools.rs"]
mod queue_tools;
const MAX: usize = 4 * 1024 * 1024;
pub(super) const SECTIONS: &[(&str, &str, &str)] = &[
    ("about", "Application", "about"),
    ("events", "Events", "event:list"),
    ("schedule", "Schedule", "schedule:list"),
    ("queues", "Failed Jobs", "queue:failed"),
    ("queue-status", "Queues", "@queue-status"),
    ("workers", "Workers", "@workers"),
    ("logs", "Logs", "@logs"),
    ("schema", "Schema", "@schema"),
];
#[derive(Clone)]
struct Snapshot {
    identity: Value,
    target: Option<Value>,
    rows: Vec<Value>,
    captured: u64,
}
#[derive(Default)]
pub(super) struct State {
    snapshots: BTreeMap<String, Snapshot>,
}
fn key(root: &str, runner: Runner, section: &str) -> String {
    format!("{root}\n{}\n{section}", runner.name())
}
fn stamp(project: &Project) -> Result<Value, Error> {
    let mut identity = metadata::identity(project)?;
    let mut sources = vec![];
    for pattern in [
        "app/Providers/**/*.php",
        "app/Console/**/*.php",
        "app/Events/**/*.php",
        "app/Listeners/**/*.php",
        "app/Jobs/**/*.php",
        "routes/**/*.php",
        "config/**/*.php",
        "bootstrap/**/*.php",
    ] {
        let paths = project.find_files(pattern, 1024)?;
        if paths.len() >= 1024 {
            return Err(Error::Invalid(
                "Runtime input discovery reached its limit.".into(),
            ));
        }
        for path in paths {
            if !crate::owns_current_path(project, &path) {
                return Err(Error::Stale);
            }
            sources.push(path);
        }
    }
    let env = format!("{}/.env", project.root()?);
    if project.exists(&env)? {
        if !crate::owns_current_path(project, &env) {
            return Err(Error::Stale);
        }
        sources.push(env);
    }
    sources.sort();
    sources.dedup();
    let mut total = 0;
    let mut hashes = vec![];
    for path in sources {
        let text = project.read_file(&path)?;
        total += text.len();
        if total > 8 * 1024 * 1024 {
            return Err(Error::Invalid("Runtime inputs exceed 8 MiB.".into()));
        }
        hashes.push(json!([path, metadata::hash(&text)]));
    }
    identity["sources"] = json!(hashes);
    Ok(identity)
}
fn owner(ctx: &Invocation) -> Result<(Project, String), Error> {
    let project = ctx.project().ok_or("Open a Laravel project first.")?;
    let root = project.root()?;
    if !crate::hover::canonical(&root)
        || !host::projects()?
            .iter()
            .any(|p| p.root().ok().as_deref() == Some(&root))
    {
        return Err(Error::Stale);
    }
    Ok((project, root))
}
fn target_binding(project: &Project, runner: Runner) -> Result<Option<Value>, Error> {
    if runner == Runner::Php {
        return Ok(None);
    }
    let snapshot = crate::artisan_docker::snapshot(project)?;
    Ok(Some(
        json!({"identity":snapshot["identity"],"proof":snapshot["proof"]}),
    ))
}
fn string(value: &Value) -> Result<String, Error> {
    let text = if let Some(text) = value.as_str() {
        text.to_owned()
    } else if value.is_boolean() || value.is_number() || value.is_null() {
        value.to_string()
    } else {
        return Err(Error::Invalid("Unsupported runtime value.".into()));
    };
    if text.len() > 4096
        || text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
    {
        return Err(Error::Invalid(
            "Runtime field exceeds its display limit.".into(),
        ));
    }
    Ok(text)
}
pub(super) fn parse(section: &str, raw: &str) -> Result<Vec<Value>, Error> {
    if section == "schema" { return database::parse(raw); }
    if matches!(section, "queue-status" | "workers" | "logs") {
        return queue_tools::parse(section, raw);
    }
    let value = crate::strict_json::parse(raw, MAX)?;
    let title = SECTIONS
        .iter()
        .find(|(id, _, _)| *id == section)
        .ok_or(Error::Stale)?
        .1;
    let mut rows = vec![];
    let mut push = |resource: String, details: String| -> Result<(), Error> {
        if rows.len() >= 5000 {
            return Err(Error::Invalid("Runtime snapshot exceeds 5000 rows.".into()));
        }
        rows.push(json!({"key":format!("runtime:{section}:{}",rows.len()),"cells":[title,resource,details],"activationEnabled":false}));
        Ok(())
    };
    if section == "about" {
        let object = value
            .as_object()
            .ok_or("Expected application information JSON.")?;
        for (group, keys) in [
            (
                "environment",
                &[
                    "application_name",
                    "laravel_version",
                    "php_version",
                    "composer_version",
                    "environment",
                    "debug_mode",
                    "maintenance_mode",
                    "timezone",
                    "locale",
                ][..],
            ),
            ("cache", &["config", "events", "routes", "views"][..]),
            (
                "drivers",
                &[
                    "broadcasting",
                    "cache",
                    "database",
                    "mail",
                    "queue",
                    "session",
                ][..],
            ),
        ] {
            if let Some(values) = object.get(group).and_then(Value::as_object) {
                for name in keys {
                    if let Some(value) = values.get(*name) {
                        push(format!("{group}.{name}"), string(value)?)?;
                    }
                }
            }
        }
    } else {
        let items = value.as_array().filter(|items| items.len() <= 5000).ok_or(
            "Expected a bounded runtime JSON list; this Laravel version may not support --json.",
        )?;
        for item in items {
            if section == "events" {
                let event = string(item.get("event").ok_or("Missing event name.")?)?;
                let listeners = item["listeners"]
                    .as_array()
                    .filter(|items| items.len() <= 1000)
                    .ok_or("Missing event listeners.")?;
                if listeners.is_empty() {
                    push(event.clone(), "No listeners".into())?;
                }
                for listener in listeners {
                    push(event.clone(), string(listener)?)?;
                }
            } else if section == "schedule" {
                push(
                    string(item.get("command").ok_or("Missing scheduled command.")?)?,
                    format!(
                        "{} · {} · {}",
                        string(&item["expression"])?,
                        string(&item["next_due_date"])?,
                        string(&item["timezone"])?
                    ),
                )?;
            } else {
                push(
                    string(item.get("id").ok_or("Missing failed job identity.")?)?,
                    format!(
                        "{} · {} / {} · {}",
                        string(&item["class"])?,
                        string(&item["connection"])?,
                        string(&item["queue"])?,
                        string(&item["failed_at"])?
                    ),
                )?;
            }
        }
    }
    if serde_json::to_string(&rows).unwrap_or_default().len() > MAX {
        return Err(Error::Invalid("Runtime rows exceed 4 MiB.".into()));
    }
    Ok(rows)
}
fn execute(project: &Project, runner: Runner, args: &[String]) -> Result<String, Error> {
    runner.verify(project)?;
    let args = runner.argv(project, args, false)?;
    let task = host::begin_task("Inspect Laravel runtime", true);
    let child = host::start(
        &host::Command::new(runner.name()).args(&args),
        Some(project),
    )?;
    if let Err(error) = child.close_stdin() {
        let _ = child.kill();
        return Err(error);
    }
    let result = metadata::collect_limited(
        MAX,
        256,
        |timeout| child.wait(timeout),
        || task.is_cancelled(),
    );
    if result.is_err() {
        let _ = child.kill();
    }
    result
}
impl State {
    pub fn capture(
        &mut self,
        ctx: &Invocation,
        runner: Runner,
        section: &str,
    ) -> Result<(), Error> {
        self.capture_with(ctx, runner, section, |project, args| {
            execute(project, runner, args)
        })
    }
    fn capture_with(
        &mut self,
        ctx: &Invocation,
        runner: Runner,
        section: &str,
        run: impl FnOnce(&Project, &[String]) -> Result<String, Error>,
    ) -> Result<(), Error> {
        self.capture_options_with(ctx, runner, section, json!({}), run)
    }
    #[cfg(test)]
    pub(crate) fn capture_schema_fixture(&mut self,ctx:&Invocation,payload:Value)->Result<(),Error>{
        self.capture_options_with(ctx,Runner::Php,"schema",json!({}),|_,_|Ok(payload.to_string()))
    }
    fn capture_options_with(
        &mut self,
        ctx: &Invocation,
        runner: Runner,
        section: &str,
        options: Value,
        run: impl FnOnce(&Project, &[String]) -> Result<String, Error>,
    ) -> Result<(), Error> {
        let (project, root) = owner(ctx)?;
        let command = SECTIONS
            .iter()
            .find(|(id, _, _)| *id == section)
            .ok_or(Error::Stale)?
            .2;
        let before = stamp(&project)?;
        let docker = target_binding(&project, runner)?;
        let args: Vec<String> = if command.starts_with('@') {
            queue_tools::probe_args(section, &options)?
        } else {
            [
                "artisan",
                command,
                "--json",
                "--no-interaction",
                "--no-ansi",
            ]
            .map(str::to_owned)
            .to_vec()
        };
        let raw = run(&project, &args)?;
        let mut rows = parse(section, &raw)?;
        if section == "logs" {
            let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d|d.as_nanos());
            let capture = metadata::hash(&format!("{root}:{}:{nonce}:{}",runner.name(),metadata::hash(&raw)));
            for (index,row) in rows.iter_mut().enumerate() { row["key"] = json!(format!("runtime-log:{capture}:{index}")); }
        }
        if section == "queues" {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos());
            let capture = metadata::hash(&format!(
                "{root}:{}:{nonce}:{}",
                runner.name(),
                metadata::hash(&raw)
            ));
            for (index, row) in rows.iter_mut().enumerate() {
                row["key"] = json!(format!("failedjob:{capture}:{index}"));
            }
        }
        if owner(ctx)?.1 != root
            || stamp(&project)? != before
            || target_binding(&project, runner)? != docker
        {
            return Err(Error::Stale);
        }
        let cache_key = key(&root, runner, section);
        if self.snapshots.len() >= 32 && !self.snapshots.contains_key(&cache_key) {
            return Err(Error::Invalid(
                "Too many runtime snapshots; reload to retire them.".into(),
            ));
        }
        let captured = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let size = serde_json::to_string(&rows)
            .map_err(|error| Error::Invalid(error.to_string()))?
            .len()
            + before.to_string().len()
            + docker.as_ref().map_or(0, |target| target.to_string().len());
        let retained: usize = self
            .snapshots
            .iter()
            .filter(|(key, _)| **key != cache_key)
            .map(|(_, snapshot)| {
                snapshot
                    .rows
                    .iter()
                    .map(|row| row.to_string().len())
                    .sum::<usize>()
                    + snapshot.identity.to_string().len()
                    + snapshot
                        .target
                        .as_ref()
                        .map_or(0, |target| target.to_string().len())
            })
            .sum();
        if retained + size > 8 * 1024 * 1024 {
            return Err(Error::Invalid(
                "Runtime snapshot cache exceeds 8 MiB; previous snapshots retained.".into(),
            ));
        }
        self.snapshots.insert(
            cache_key,
            Snapshot {
                identity: before,
                target: docker,
                rows,
                captured,
            },
        );
        Ok(())
    }
    /// Revalidate a captured stack candidate before exposing a source location.
    /// Callers must pass the opaque captured row key, never a path from the webview.
    pub fn log_frame(&self, ctx:&Invocation, runner:Runner, row_key:&str, index:usize) -> Result<openidea_plugin::Location,Error> {
        let (project,root)=owner(ctx)?;
        let target=target_binding(&project,runner)?;
        let snapshot=self.snapshots.get(&key(&root,runner,"logs")).filter(|snapshot|snapshot.target==target).ok_or(Error::Stale)?;
        let row=snapshot.rows.iter().find(|row|row["key"].as_str()==Some(row_key)).ok_or(Error::Stale)?;
        let value=row["error"]["frames"].as_array().and_then(|frames|frames.get(index)).ok_or(Error::Stale)?;
        let frame=errors::Frame {path:value["path"].as_str().ok_or(Error::Stale)?.into(),line:value["line"].as_u64().and_then(|n|u32::try_from(n).ok()).ok_or(Error::Stale)?,call:String::new()};
        let (path,line)=errors::source(&project,&frame).ok_or(Error::Stale)?;
        let position=openidea_plugin::Position::new(line,0);
        Ok(openidea_plugin::Location{path,range:openidea_plugin::Range::new(position,position)})
    }
    /// Select from currently provable source frames; revalidate again after the prompt.
    pub fn open_log_frame(&self, ctx:&Invocation, runner:Runner, row_key:&str) -> Result<(),Error> {
        let (_,root)=owner(ctx)?;
        let snapshot=self.snapshots.get(&key(&root,runner,"logs")).ok_or(Error::Stale)?;
        let row=snapshot.rows.iter().find(|row|row["key"].as_str()==Some(row_key)).ok_or(Error::Stale)?;
        let frames=row["error"]["frames"].as_array().ok_or("Select a structured error with stack frames.")?;
        let choices:Vec<_>=frames.iter().enumerate().filter_map(|(index,_)| {
            let location=self.log_frame(ctx,runner,row_key,index).ok()?;
            Some(openidea_plugin::PickItem::new(&index.to_string(),&format!("{}:{}",location.path,location.range.start.line+1)))
        }).collect();
        if choices.is_empty(){return Err("No captured frame currently resolves to an owned source file.".into());}
        let Some(selected)=host::pick("Open Laravel error frame",&choices)? else{return Ok(())};
        if !choices.iter().any(|choice|choice.key==selected){return Err(Error::Stale);}
        let index=selected.parse::<usize>().map_err(|_|Error::Stale)?;
        let location=self.log_frame(ctx,runner,row_key,index)?;
        host::open_editor(&location.path,Some(location.range))
    }
    /// Read-only local project inventory, separate from explicit PHP runtime snapshots.
    pub fn dashboard_rows(&self, ctx:&Invocation) -> Result<Vec<Value>,Error> {
        let (project,root)=owner(ctx)?;
        let mut rows=vec![json!({"key":"runtime:dashboard:root","cells":["Dashboard","Project",root],"activationEnabled":false})];
        for relative in ["artisan","bootstrap/app.php","composer.json"] {
            let path=format!("{root}/{relative}");
            if crate::owns_current_path(&project,&path) && project.exists(&path)? {
                rows.push(json!({"key":format!("runtime:dashboard:{relative}"),"cells":["Dashboard",relative,"Present · source inventory"],"activationEnabled":false}));
            }
        }
        let path=format!("{root}/composer.json");
        if crate::owns_current_path(&project,&path) && project.exists(&path)? {
            let text=project.read_file(&path)?;
            if text.len()<=128*1024 {
                if let Ok(value)=crate::strict_json::parse(&text,128*1024) {
                    for package in ["php","laravel/framework","livewire/livewire","inertiajs/inertia-laravel","laravel/nova","filament/filament","laravel/passport","laravel/sanctum"] {
                        if let Some(version)=value["require"][package].as_str().filter(|s|s.len()<=256 && !s.chars().any(char::is_control)) {
                            rows.push(json!({"key":format!("runtime:dashboard:package:{package}"),"cells":["Dashboard",package,format!("{version} · declared Composer constraint; installed version not inferred")],"activationEnabled":false}));
                        }
                    }
                }
            }
        }
        Ok(rows)
    }
    pub fn invalidate(&mut self, root: &str) {
        self.snapshots
            .retain(|_, snapshot| snapshot.identity["root"] != root);
    }
    pub fn cached(&self, ctx: &Invocation, runner: Runner) -> Result<Value, Error> {
        let (project, root) = owner(ctx)?;
        let target = target_binding(&project, runner).ok().flatten();
        let snapshots:Vec<_>=SECTIONS.iter().filter_map(|(section,_,_)|self.snapshots.get(&key(&root,runner,section)).filter(|snapshot|snapshot.target==target).map(|snapshot|json!({"section":section,"capturedAt":snapshot.captured,"rows":snapshot.rows}))).collect();
        Ok(
            json!({"runner":runner.name(),"snapshots":snapshots,"provenance":"Explicit Artisan snapshots; refresh after environment/database changes"}),
        )
    }
    pub fn status(&self, ctx: &Invocation, runner: Runner) -> Result<String, Error> {
        let (project, root) = owner(ctx)?;
        let target = target_binding(&project, runner).ok().flatten();
        let captures: Vec<_> = SECTIONS
            .iter()
            .filter_map(|(section, _, _)| {
                self.snapshots
                    .get(&key(&root, runner, section))
                    .filter(|snapshot| snapshot.target == target)
            })
            .collect();
        let label = if runner == Runner::Docker {
            "Docker"
        } else {
            "Local PHP"
        };
        let Some(latest) = captures.iter().map(|snapshot| snapshot.captured).max() else {
            return Ok(format!(
                "{label} · No snapshots · Choose an inspection button to capture runtime information"
            ));
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(latest, |duration| duration.as_secs());
        Ok(format!(
            "{label} · {} captured sections · most recent capture {}s ago · refresh after environment/database changes",
            captures.len(),
            now.saturating_sub(latest)
        ))
    }
    pub fn body(&self, ctx: &Invocation, runner: Runner) -> Result<Body, Error> {
        let cached = self.cached(ctx, runner)?;
        let mut rows = self.dashboard_rows(ctx)?;
        rows.extend(self.database_source_rows(ctx).unwrap_or_default());
        rows.extend(cached["snapshots"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|snapshot| snapshot["rows"].as_array().into_iter().flatten().cloned())
            );
        Ok(Body::Other{kind:"resources".into(),data:json!({"columns":["Section","Resource","Details"],"columnStyles":["text","code","muted"],"columnWeights":[1,3,4],"facets":[0],"facetLabels":{"0":"All sections"},"searchPlaceholder":"Filter runtime information…","items":rows}).to_string()})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn setup() -> MockHost {
        let host = MockHost::install();
        host.add_file("artisan", "<?php");
        host.add_file("composer.json", "{}");
        host
    }
    #[test]
    fn log_frame_navigation_rejects_stale_keys_and_rechecks_current_sources() {
        let host=setup(); host.add_file("app/Broken.php","<?php\nthrow new Exception();");
        let ctx=host.invocation();let root=ctx.project().unwrap().root().unwrap();let mut state=State::default();
        let payload=json!({"file":"storage/logs/laravel.log","truncated":false,"lines":["[2026-11-10] local.ERROR: Broken",format!("#0 {root}/app/Broken.php(2): run()")]});
        state.capture_options_with(&ctx,Runner::Php,"logs",json!({}),|_,_|Ok(payload.to_string())).unwrap();
        let snapshot=&state.snapshots[&key(&root,Runner::Php,"logs")];let row=snapshot.rows.iter().find(|row|row.get("error").is_some()).unwrap();let captured=row["key"].as_str().unwrap().to_owned();
        assert_eq!(state.log_frame(&ctx,Runner::Php,&captured,0).unwrap().range.start.line,1);
        host.answer_next_prompt(Some(&["0"]));state.open_log_frame(&ctx,Runner::Php,&captured).unwrap();
        let editor=ctx.editor().unwrap();assert_eq!(editor.path(),format!("{root}/app/Broken.php"));assert_eq!(editor.selections().unwrap()[0].start.line,1);
        host.type_in_editor(" changed");let edited=host.editor_text();host.answer_next_prompt(None);state.open_log_frame(&ctx,Runner::Php,&captured).unwrap();assert_eq!(host.editor_text(),edited);
        assert!(state.log_frame(&ctx,Runner::Php,&captured,10).is_err());
        host.add_file("app/Broken.php","<?php");host.open_editor("app/Broken.php","php","<?php",&[]);assert!(state.log_frame(&ctx,Runner::Php,&captured,0).is_err());
        host.add_file("app/Broken.php","<?php\nthrow new Exception();");host.open_editor("app/Broken.php","php","<?php\nthrow new Exception();",&[]);
        state.capture_options_with(&ctx,Runner::Php,"logs",json!({}),|_,_|Ok(payload.to_string())).unwrap();
        assert!(state.log_frame(&ctx,Runner::Php,&captured,0).is_err());
    }
    #[test]
    fn dashboard_is_local_curated_inventory_not_runtime_or_credentials() {
        let host=setup();host.add_file("composer.json",r#"{"require":{"php":"^8.3","laravel/framework":"^13.0","password":"secret"},"password":"secret"}"#);
        let rows=State::default().dashboard_rows(&host.invocation()).unwrap();let text=json!(rows).to_string();
        assert!(text.contains("^13.0"));assert!(text.contains("installed version not inferred"));assert!(!text.contains("secret"));
    }
    #[test]
    fn readonly_endpoint_and_route_search_keep_explicit_runtime_snapshots() {
        use openidea_plugin::Plugin;
        let host=MockHost::install();host.add_file("artisan","<?php");let mut plugin=crate::tests::activated(&host);let ctx=host.invocation();let root=ctx.project().unwrap().root().unwrap();
        plugin.runtime_inspection.capture_options_with(&ctx,Runner::Php,"about",json!({}),|_,_|Ok(json!({"environment":{"php_version":"8.4"}}).to_string())).unwrap();
        plugin.runtime_inspection.capture_options_with(&ctx,Runner::Php,"schema",json!({}),|_,_|Ok(json!({"tables":[{"name":"posts","columns":[{"name":"id","type":"integer","nullable":false}],"indexes":[]}]}).to_string())).unwrap();
        let raw=r#"[{"method":"GET|HEAD","uri":"api/live","name":"live","action":"App\\Http\\Controllers\\PostController@index"}]"#;
        let snapshot=crate::route_runtime::capture_with(&ctx,plugin.view_index(&ctx).unwrap(),|_|Ok(raw.into())).unwrap();plugin.runtime_routes.insert(root.clone(),snapshot);
        let before=plugin.runtime_inspection.cached(&ctx,Runner::Php).unwrap();let provenance=plugin.runtime_routes[&root].provenance();
        let endpoints=openidea_plugin::endpoints::EndpointsProvider::endpoints(&mut plugin,&ctx).unwrap();assert!(endpoints.iter().any(|endpoint|endpoint.path=="/api/live"));
        assert_eq!(plugin.runtime_inspection.cached(&ctx,Runner::Php).unwrap(),before);assert_eq!(plugin.runtime_routes[&root].provenance(),provenance);
        host.answer_next_prompt(Some(&["live"]));host.answer_next_prompt(Some(&["0"]));plugin.run_command("route-search",&ctx,None).unwrap();
        assert_eq!(plugin.runtime_inspection.cached(&ctx,Runner::Php).unwrap(),before);assert_eq!(plugin.runtime_routes[&root].provenance(),provenance);assert!(host.process_runs().is_empty());
        assert!(ctx.editor().unwrap().path().ends_with("PostController.php"));
    }
    #[test]
    fn parses_runtime_sections_and_excludes_secret_payloads() {
        let rows=parse("about",r#"{"environment":{"php_version":"8.4","password":"secret"},"drivers":{"database":"sqlite","password":"secret"},"custom":{"secret":"secret"}}"#).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(!json!(rows).to_string().contains("secret"));
        let events = parse(
            "events",
            r#"[{"event":"App\\Events\\Created","listeners":["App\\Listeners\\Notify"]}]"#,
        )
        .unwrap();
        assert_eq!(events[0]["cells"][1], "App\\Events\\Created");
        assert_eq!(parse("schedule",r#"[{"command":"inspire","expression":"* * * * *","next_due_date":"2026-11-10","timezone":"UTC"}]"#).unwrap().len(),1);
        let failed=parse("queues",r#"[{"id":"id","class":"Job","connection":"redis","queue":"default","failed_at":"today","payload":"secret","exception":"secret"}]"#).unwrap();
        assert!(!json!(failed).to_string().contains("secret"));
        for section in ["events", "schedule", "queues"] {
            assert!(parse(section, "[]").unwrap().is_empty());
            assert!(parse(section, "{}").is_err());
        }
        assert!(parse("events", r#"[{"event":"X","listeners":null}]"#).is_err());
        assert!(parse("events", r#"[{"event":"X","event":"Y","listeners":[]}]"#).is_err());
    }
    #[test]
    fn capture_is_explicit_bounded_and_retains_previous_snapshot_on_failure() {
        let host = setup();
        let mut state = State::default();
        let ctx = host.invocation();
        assert!(
            state.cached(&ctx, Runner::Php).unwrap()["snapshots"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        state
            .capture_with(&ctx, Runner::Php, "events", |_, args| {
                assert_eq!(
                    args,
                    [
                        "artisan",
                        "event:list",
                        "--json",
                        "--no-interaction",
                        "--no-ansi"
                    ]
                );
                Ok(r#"[{"event":"E","listeners":["L"]}]"#.into())
            })
            .unwrap();
        let before = state.cached(&ctx, Runner::Php).unwrap();
        assert!(
            state
                .capture_with(&ctx, Runner::Php, "events", |_, _| Ok("not json".into()))
                .is_err()
        );
        assert_eq!(state.cached(&ctx, Runner::Php).unwrap(), before);
        assert!(
            state
                .capture_with(&ctx, Runner::Php, "events", |_, _| {
                    host.add_file("config/new.php", "<?php return []; ");
                    Ok("[]".into())
                })
                .is_err()
        );
        assert_eq!(state.cached(&ctx, Runner::Php).unwrap(), before);
        assert!(
            state.cached(&ctx, Runner::Docker).unwrap()["snapshots"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        host.add_project("/project");
        host.set_project_root(Some("/other"));
        assert!(
            state.cached(&host.invocation(), Runner::Php).unwrap()["snapshots"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        host.set_project_root(Some("/project"));
        state.invalidate("/project");
        assert!(
            state.cached(&ctx, Runner::Php).unwrap()["snapshots"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn runtime_denied_execution_does_not_replace_a_cached_snapshot() {
        let host = setup().with_capabilities(&["project.read"]);
        let mut state = State::default();
        assert!(matches!(
            state.capture(&host.invocation(), Runner::Php, "events"),
            Err(Error::Denied(_))
        ));
        assert!(state.snapshots.is_empty());
    }
}
