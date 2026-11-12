//! Connection UX for the managed JDBC helper (P3): profiles, Spring data source discovery, the clear-text password
//! prompt, driver jars, Test connection and a schema/table browser. Read-only: nothing here writes to a database or
//! to the project, and snapshots are only summarised (wiring them into reverse engineering is P5).
//!
//! Everything talks to `persistence_core::introspect::Client` through the `Runner` trait; `HostRunner` launches
//! `java JdbcHelper.java` with `host::run_command`, tests use fake runners. The password lives in one local variable
//! for the length of a command: never in storage, notifications, logs or arguments (see the sentinel tests).
use crate::datasource_facts::{self, DataSource, Dialect};
use openidea_plugin::host::Command;
use openidea_plugin::{Error, Invocation, NotificationLevel, PickItem, Project, host};
use persistence_core::connection::{
    Dbms, Profile, SpringSource, connection_summary, count_snapshot, drivers_json, error_message, filter_required_message, needs_filter, parse_drivers,
    MAX_JAR_SEARCH, driver_jar_candidates, parse_profiles, profiles_json, snapshot_summary, upsert, valid_jar_path,
};
use persistence_core::introspect::{
    Client, Connection, ConnectionInfo, DriverSpec, ErrorCode, HelperError, LaunchError, MAX_DESCRIBE_TABLES, PingInfo, RunOutput, Runner, TableEntry, TableMeta,
};
use persistence_core::introspect_map::{MapError, MapOptions, to_snapshot};
use std::collections::BTreeMap;

const HELPER: &str = include_str!("../helper/Helper.java");
const HELPER_FILE: &str = "JdbcHelper.java";
const DRIVERS_FILE: &str = "jdbc-drivers.json";
pub(crate) const PROFILES_KEY: &str = "jdbc.profiles";
const CONFIRMED_KEY: &str = "jdbc.confirmed";
const MAX_CONFIRMED: usize = 200;
const TITLE: &str = "JPA Assist";
const TEST_DEADLINE_MS: u64 = 30_000;
const BROWSE_DEADLINE_MS: u64 = 20_000;
const FILTER_ATTEMPTS: usize = 3;
const ALL_TABLES: &str = "*";
const JAVA_FLAGS: [&str; 6] = ["-Dfile.encoding=UTF-8", "-Dstdout.encoding=UTF-8", "-Duser.language=en", "-Xmx256m", "-Djava.awt.headless=true", "-XX:TieredStopAtLevel=1"];

fn info(text: &str) {
    host::notify(NotificationLevel::Info, TITLE, text);
}

fn fail(text: &str) {
    host::notify(NotificationLevel::Error, TITLE, text);
}

fn report(error: &HelperError) {
    match error.code {
        ErrorCode::Cancelled => info(&error_message(error)),
        _ => fail(&error_message(error)),
    }
}

fn cancelled() -> HelperError {
    HelperError { code: ErrorCode::Cancelled, message: "cancelled".into(), sql_state: None, vendor_code: 0, retryable: false }
}

// ---- the real runner ----

/// Launches the helper once per request. The host budget (5 s, or 60 s inside a task) kills it; the runner's own
/// `timeout_ms` is therefore advisory and unused.
pub struct HostRunner {
    helper: String,
}

impl Runner for HostRunner {
    fn run(&mut self, stdin: &str, _timeout_ms: u64) -> Result<RunOutput, LaunchError> {
        let command = Command::new("java").args(JAVA_FLAGS).arg(&self.helper);
        match host::run_command(&command, None, Some(stdin)) {
            Ok(output) => Ok(RunOutput { code: output.status.unwrap_or(-1), stdout: output.stdout, stderr: output.stderr }),
            Err(Error::Denied(why)) => Err(LaunchError::Denied(why)),
            Err(Error::Cancelled) => Err(LaunchError::Cancelled),
            Err(Error::Failed(message)) if message.contains("timed out") => Err(LaunchError::Timeout),
            Err(other) => Err(LaunchError::NotStarted(other.to_string())),
        }
    }
}

/// Writes the bundled helper into the private plugin directory when it is missing or differs.
fn install_helper() -> Result<String, Error> {
    let path = format!("{}/{HELPER_FILE}", host::fs::plugin_dir()?);
    match host::fs::read_bytes(&path) {
        Ok(bytes) if bytes == HELPER.as_bytes() => {}
        _ => host::fs::write_bytes(&path, HELPER.as_bytes())?,
    }
    Ok(path)
}

// ---- storage ----

fn load_profiles() -> Result<Vec<Profile>, Error> {
    let text = host::storage::get(PROFILES_KEY)?;
    parse_profiles(text.as_deref()).map_err(Error::Invalid)
}

pub fn saved_profiles() -> Result<Vec<Profile>, Error> {
    load_profiles()
}

/// Shows one saved profile: never the password, which is not stored.
pub fn describe_saved(name: &str) {
    let text = match load_profiles().map(|profiles| profiles.into_iter().find(|p| p.name == name)) {
        Ok(Some(p)) => format!(
            "`{}`: {} {}, user {}, driver jar {}. The password is asked for when connecting.",
            p.name,
            p.dbms.label(),
            p.target(),
            p.user.as_deref().filter(|u| !u.is_empty()).unwrap_or("(none)"),
            p.driver_jar.as_deref().unwrap_or("from the driver registry"),
        ),
        _ => format!("Connection `{name}` is no longer saved."),
    };
    info(&text);
}

fn save_profiles(profiles: &[Profile]) -> Result<(), Error> {
    host::storage::set(PROFILES_KEY, &profiles_json(profiles))
}

fn load_drivers() -> Result<BTreeMap<Dbms, String>, Error> {
    let path = format!("{}/{DRIVERS_FILE}", host::fs::plugin_dir()?);
    let text = host::fs::read_file(&path).ok();
    parse_drivers(text.as_deref()).map_err(Error::Invalid)
}

fn save_drivers(drivers: &BTreeMap<Dbms, String>) -> Result<(), Error> {
    let path = format!("{}/{DRIVERS_FILE}", host::fs::plugin_dir()?);
    host::fs::write_file(&path, &drivers_json(drivers))
}

fn confirmed_keys() -> Vec<String> {
    host::storage::get(CONFIRMED_KEY).ok().flatten().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn remember_confirmed(key: String) -> Result<(), Error> {
    let mut keys = confirmed_keys();
    keys.retain(|k| *k != key);
    keys.push(key);
    let start = keys.len().saturating_sub(MAX_CONFIRMED);
    host::storage::set(CONFIRMED_KEY, &serde_json::to_string(&keys[start..]).unwrap_or_default())
}

// ---- entry point ----

pub fn run(id: &str, ctx: &Invocation) -> Result<Option<String>, Error> {
    match id {
        "jdbc-manage" => manage(ctx)?,
        "jdbc-test" => test_connection(ctx)?,
        "jdbc-browse" => browse(ctx)?,
        _ => return Err(Error::NotFound(format!("command {id}"))),
    }
    Ok(None)
}

fn pick_one(title: &str, items: Vec<PickItem>) -> Result<Option<String>, Error> {
    host::pick(title, &items)
}

fn pick_profile(title: &str) -> Result<Option<Profile>, Error> {
    let profiles = match load_profiles() {
        Ok(profiles) => profiles,
        Err(Error::Invalid(why)) => {
            fail(&format!("{why}. Nothing was changed; the stored list is left as it is."));
            return Ok(None);
        }
        Err(other) => return Err(other),
    };
    match profiles.len() {
        0 => {
            info("No saved database connections yet. Run JPA: Manage Database Connections to add one.");
            Ok(None)
        }
        1 => Ok(profiles.into_iter().next()),
        _ => {
            let items = profiles.iter().enumerate().map(|(i, p)| PickItem::new(&i.to_string(), &p.name).description(&format!("{}: {}", p.dbms.label(), p.target()))).collect();
            let Some(key) = pick_one(title, items)? else {
                return Ok(None);
            };
            Ok(key.parse::<usize>().ok().and_then(|i| profiles.into_iter().nth(i)))
        }
    }
}

// ---- connecting ----

fn driver_jar(profile: &Profile) -> Result<Option<String>, Error> {
    if let Some(jar) = &profile.driver_jar {
        return Ok(Some(jar.clone()));
    }
    Ok(load_drivers()?.get(&profile.dbms).cloned())
}

fn confirm_target(profile: &Profile, jar: &str) -> Result<bool, Error> {
    let key = profile.confirmation_key(jar);
    if confirmed_keys().contains(&key) {
        return Ok(true);
    }
    let body = format!(
        "JPA Assist will connect to:\n\nDatabase: {}\nTarget: {}\nUser: {}\nDriver jar: {jar}\n\nThis starts java with the driver jar you supplied. The driver code runs with your user rights and can reach any host this machine can reach; JPA Assist only reads metadata and never writes. Connect only if you trust this URL and this jar.",
        profile.dbms.label(),
        profile.target(),
        profile.user.as_deref().unwrap_or("(none)"),
    );
    if !host::confirm("Connect to this database?", &body)? {
        return Ok(false);
    }
    remember_confirmed(key)?;
    Ok(true)
}

fn spring_password(project: Option<Project>, source: &SpringSource) -> Option<String> {
    let project = project?;
    if project.root().ok()? != source.root {
        return None;
    }
    let text = project.read_file(&source.file).ok()?.to_string();
    datasource_facts::literal_password(&text, &source.file, &source.prefix)
}

/// Config first, else a prompt that says it shows the typed text (the host has no masked prompt).
fn password_for(project: Option<Project>, profile: &Profile) -> Result<Option<String>, Error> {
    if let Some(source) = &profile.spring_source {
        if let Some(password) = spring_password(project, source) {
            return Ok(Some(password));
        }
    }
    let prompt = format!(
        "Password for {} at {}. Your typing is VISIBLE in this box (the IDE has no masked prompt). It goes only to the local helper process and is never stored or logged. Leave empty for no password.",
        profile.user.as_deref().unwrap_or("(no user)"),
        profile.target()
    );
    host::input(&prompt, None)
}

/// Validates, confirms the target, gets the password and builds the client. `None`: stopped, already reported.
fn prepare(ctx: &Invocation, profile: &Profile) -> Result<Option<Client<HostRunner>>, Error> {
    prepare_with(ctx.project(), profile)
}

pub(crate) fn prepare_with(project: Option<Project>, profile: &Profile) -> Result<Option<Client<HostRunner>>, Error> {
    if let Err(why) = profile.validate() {
        fail(&format!("Connection `{}` cannot be used: {why}", profile.name));
        return Ok(None);
    }
    let Some(jar) = driver_jar(profile)? else {
        fail(&format!("No {} driver jar is set. Run JPA: Manage Database Connections and choose Set driver jar.", profile.dbms.label()));
        return Ok(None);
    };
    if !confirm_target(profile, &jar)? {
        return Ok(None);
    }
    let Some(password) = password_for(project, profile)? else {
        return Ok(None);
    };
    let helper = match install_helper() {
        Ok(path) => path,
        Err(why) => {
            fail(&format!("The database helper could not be written to the plugin directory: {why}"));
            return Ok(None);
        }
    };
    let connection = Connection {
        url: profile.url_template.clone(),
        user: profile.user.clone().filter(|u| !u.is_empty()),
        driver: DriverSpec { jars: vec![jar], class: profile.dbms.driver_class().to_owned() },
        props: BTreeMap::new(),
    };
    Ok(Some(Client::new(HostRunner { helper }, connection, Some(password).filter(|p| !p.is_empty()))))
}

// ---- Test connection ----

/// JVM probe, then a metadata-only connect. `cancelled` is polled between launches.
pub fn probe<R: Runner>(client: &mut Client<R>, cancelled_now: &dyn Fn() -> bool) -> Result<(PingInfo, ConnectionInfo), HelperError> {
    let ping = client.ping()?;
    if cancelled_now() {
        return Err(cancelled());
    }
    let connected = client.test_connection()?;
    Ok((ping, connected))
}

fn test_connection(ctx: &Invocation) -> Result<(), Error> {
    let Some(profile) = pick_profile("Test which connection?")? else {
        return Ok(());
    };
    let Some(mut client) = prepare(ctx, &profile)? else {
        return Ok(());
    };
    client.deadline_ms = TEST_DEADLINE_MS;
    let task = host::begin_task("Testing database connection", true);
    let outcome = probe(&mut client, &|| task.is_cancelled());
    drop(task);
    match outcome {
        Ok((ping, connected)) => info(&format!(
            "Connection `{}` works: {} Java {} ({}), helper {}.",
            profile.name,
            connection_summary(&connected),
            ping.java.version,
            ping.java.vendor,
            ping.helper_version
        )),
        Err(error) => report(&error),
    }
    Ok(())
}

// ---- Browse ----

#[derive(Debug)]
pub enum ListError {
    Helper(HelperError),
    Host(Error),
    /// The user dismissed the filter prompt.
    Declined,
    StillTooMany(usize),
}

/// Tables of the chosen schemas that match `pattern`. More than the snapshot bound is never cut off: `ask` must
/// return a narrower pattern (or `None` to stop) until the count fits; after `FILTER_ATTEMPTS` the listing fails.
pub fn list_with_filter<R: Runner>(
    client: &mut Client<R>,
    schemas: &[Option<String>],
    mut pattern: Option<String>,
    ask: &mut dyn FnMut(usize) -> Result<Option<String>, Error>,
) -> Result<(Vec<(Option<String>, TableEntry)>, Option<String>), ListError> {
    let mut attempts = 0;
    loop {
        let mut listed = Vec::new();
        for schema in schemas {
            let tables = client.list_all_tables(schema.as_deref(), pattern.as_deref()).map_err(ListError::Helper)?;
            listed.extend(tables.into_iter().map(|t| (schema.clone(), t)));
        }
        let tables = listed.iter().filter(|(_, t)| t.kind == "TABLE").count();
        if !needs_filter(tables) {
            return Ok((listed, pattern));
        }
        if attempts == FILTER_ATTEMPTS {
            return Err(ListError::StillTooMany(tables));
        }
        attempts += 1;
        match ask(tables).map_err(ListError::Host)? {
            Some(next) if !next.trim().is_empty() => pattern = Some(next.trim().to_owned()),
            _ => return Err(ListError::Declined),
        }
    }
}

/// Describes the selection in chunks of 25, polling `cancelled_now` and reporting `(done, total)` between them.
pub fn describe_selection<R: Runner>(
    client: &mut Client<R>,
    selection: &[(Option<String>, String)],
    cancelled_now: &dyn Fn() -> bool,
    progress: &dyn Fn(usize, usize),
) -> Result<Vec<TableMeta>, HelperError> {
    let mut by_schema: BTreeMap<Option<String>, Vec<String>> = BTreeMap::new();
    for (schema, name) in selection {
        by_schema.entry(schema.clone()).or_default().push(name.clone());
    }
    let mut described = Vec::new();
    for (schema, names) in by_schema {
        for chunk in names.chunks(MAX_DESCRIBE_TABLES) {
            if cancelled_now() {
                return Err(cancelled());
            }
            progress(described.len(), selection.len());
            let mut metas = client.describe_all(schema.as_deref(), chunk)?;
            for meta in &mut metas {
                if meta.schema.is_none() {
                    meta.schema.clone_from(&schema);
                }
            }
            described.extend(metas);
        }
    }
    Ok(described)
}

fn browse(ctx: &Invocation) -> Result<(), Error> {
    let Some(profile) = pick_profile("Browse which connection?")? else {
        return Ok(());
    };
    let Some(mut client) = prepare(ctx, &profile)? else {
        return Ok(());
    };
    client.deadline_ms = BROWSE_DEADLINE_MS;
    let task = host::begin_task("Reading database metadata", true);
    let result = browse_with(&profile, &mut client, &task);
    drop(task);
    let (profile, summary) = match result? {
        Some(done) => done,
        None => return Ok(()),
    };
    if let Err(why) = save_filter(profile) {
        fail(&format!("The connection summary is below, but the schema filter could not be saved: {why}"));
    }
    info(&summary);
    Ok(())
}

fn schema_candidates(list: &persistence_core::introspect::SchemaList) -> Vec<Option<String>> {
    let named: Vec<Option<String>> = list.schemas.iter().filter(|s| !s.system && s.name.is_some()).map(|s| s.name.clone()).collect();
    if named.is_empty() { vec![list.current.schema.clone()] } else { named }
}

fn choose_schemas(profile: &Profile, list: &persistence_core::introspect::SchemaList) -> Result<Option<Vec<Option<String>>>, Error> {
    let candidates = schema_candidates(list);
    let saved: Vec<Option<String>> = profile.schema_filter.schemas.iter().cloned().map(Some).collect();
    if !saved.is_empty() && saved.iter().all(|s| candidates.contains(s)) {
        return Ok(Some(saved));
    }
    if candidates.len() == 1 {
        return Ok(Some(candidates));
    }
    let items: Vec<PickItem> = list
        .schemas
        .iter()
        .filter(|s| candidates.contains(&s.name) && s.name.is_some())
        .map(|s| {
            let item = PickItem::new(s.name.as_deref().unwrap_or(""), s.name.as_deref().unwrap_or(""));
            if s.is_default { item.description("default schema") } else { item }
        })
        .collect();
    let Some(keys) = host::pick_many("Schemas to read", &items)? else {
        return Ok(None);
    };
    if keys.is_empty() {
        return Ok(None);
    }
    Ok(Some(keys.into_iter().map(Some).collect()))
}

fn choose_tables(listed: &[(Option<String>, TableEntry)]) -> Result<Option<Vec<(Option<String>, String)>>, Error> {
    let mut items = vec![PickItem::new(ALL_TABLES, &format!("All {} listed tables and views", listed.len()))];
    for (i, (schema, table)) in listed.iter().enumerate() {
        let owner = schema.as_deref().unwrap_or("");
        items.push(PickItem::new(&i.to_string(), &table.name).description(&format!("{owner} {}", table.kind.to_lowercase())));
    }
    let Some(keys) = host::pick_many("Tables to read", &items)? else {
        return Ok(None);
    };
    let all = keys.iter().any(|k| k == ALL_TABLES);
    let chosen: Vec<(Option<String>, String)> = if all {
        listed.iter().map(|(s, t)| (s.clone(), t.name.clone())).collect()
    } else {
        keys.iter().filter_map(|k| k.parse::<usize>().ok()).filter_map(|i| listed.get(i)).map(|(s, t)| (s.clone(), t.name.clone())).collect()
    };
    Ok(Some(chosen).filter(|c| !c.is_empty()))
}

/// `Ok(None)`: stopped and reported. `Ok(Some)`: the profile with the filter that was used, and the summary.
fn browse_with(profile: &Profile, client: &mut Client<HostRunner>, task: &host::Task) -> Result<Option<(Profile, String)>, Error> {
    let list = match client.list_schemas() {
        Ok(list) => list,
        Err(error) => {
            report(&error);
            return Ok(None);
        }
    };
    let Some(schemas) = choose_schemas(profile, &list)? else {
        return Ok(None);
    };
    let candidates = schema_candidates(&list);

    let mut ask = |count: usize| host::input(&format!("{}\nName filter, for example order% :", filter_required_message(count)), profile.schema_filter.table_pattern.as_deref());
    let (listed, pattern) = match list_with_filter(client, &schemas, profile.schema_filter.table_pattern.clone(), &mut ask) {
        Ok(done) => done,
        Err(ListError::Helper(error)) => {
            report(&error);
            return Ok(None);
        }
        Err(ListError::Host(error)) => return Err(error),
        Err(ListError::Declined) => return Ok(None),
        Err(ListError::StillTooMany(count)) => {
            fail(&format!("{} The filter you entered still leaves too many; narrow it further.", filter_required_message(count)));
            return Ok(None);
        }
    };
    if listed.is_empty() {
        info("No tables or views match the selected schemas and filter.");
        return Ok(None);
    }
    let Some(selection) = choose_tables(&listed)? else {
        return Ok(None);
    };

    let described = match describe_selection(client, &selection, &|| task.is_cancelled(), &|done, total| task.report(done as u32, Some(total as u32), None)) {
        Ok(described) => described,
        Err(error) => {
            report(&error);
            return Ok(None);
        }
    };

    let schema_names: Vec<String> = schemas.iter().flatten().cloned().collect();
    let complete = pattern.is_none() && selection.len() == listed.len() && schemas.iter().all(|s| candidates.contains(s)) && candidates.iter().all(|s| schemas.contains(s));
    let mapped = match profile.dbms.mapping_dialect() {
        None => None,
        Some(dialect) => match to_snapshot(dialect, &described, &MapOptions { listing_complete: complete, qualify_schema: profile.schema_filter.qualify_schema }) {
            Ok(mapped) => Some(mapped),
            Err(MapError::DuplicateTable { name }) => {
                fail(&format!(
                    "Table `{name}` exists in more than one selected schema. Select fewer schemas, or turn on Qualify table names with schema for this connection (JPA: Manage Database Connections)."
                ));
                return Ok(None);
            }
        },
    };
    let counts = count_snapshot(&described, mapped.as_ref());
    let summary = format!("`{}`: {}", profile.name, snapshot_summary(profile.dbms, &schema_names, &counts, complete && mapped.as_ref().is_none_or(|m| m.snapshot.complete)));

    let mut used = profile.clone();
    used.schema_filter.schemas = schema_names;
    used.schema_filter.table_pattern = pattern;
    Ok(Some((used, summary)))
}

fn save_filter(profile: Profile) -> Result<(), Error> {
    let mut profiles = load_profiles()?;
    if profiles.iter().any(|p| *p == profile) {
        return Ok(());
    }
    upsert(&mut profiles, profile).map_err(|e| Error::Invalid(e.to_string()))?;
    save_profiles(&profiles)
}

// ---- Manage ----

fn manage(ctx: &Invocation) -> Result<(), Error> {
    let items = vec![
        PickItem::new("spring", "New connection from Spring configuration").description("reads application*.properties|yml; the password is never kept"),
        PickItem::new("manual", "New connection"),
        PickItem::new("driver", "Set driver jar").description("absolute path of a JDBC driver jar; nothing is downloaded"),
        PickItem::new("filter", "Reset schema and table filter"),
        PickItem::new("qualify", "Toggle table names qualified with schema"),
        PickItem::new("delete", "Delete connection"),
    ];
    let Some(action) = pick_one("Database connections", items)? else {
        return Ok(());
    };
    match action.as_str() {
        "spring" => from_spring(ctx),
        "manual" => manual(ctx),
        "driver" => set_driver(ctx),
        "filter" => edit_profile("Reset the filter of which connection?", |p| p.schema_filter = Default::default(), "Filter reset."),
        "qualify" => edit_profile("Toggle qualification for which connection?", |p| p.schema_filter.qualify_schema = !p.schema_filter.qualify_schema, "Qualification toggled."),
        "delete" => delete_profile(),
        _ => Ok(()),
    }
}

fn dbms_of(dialect: &Dialect) -> Result<Dbms, String> {
    match dialect {
        Dialect::PostgreSql => Ok(Dbms::PostgreSql),
        Dialect::MySql => Ok(Dbms::MySql),
        Dialect::MariaDb => Ok(Dbms::MariaDb),
        Dialect::H2 => Ok(Dbms::H2),
        Dialect::SqlServer => Err(Dbms::SqlServer.disabled_reason()),
        Dialect::Oracle => Err(Dbms::Oracle.disabled_reason()),
        Dialect::Db2 => Err(Dbms::Db2.disabled_reason()),
        Dialect::Hsqldb => Err("HSQLDB is not supported.".into()),
        Dialect::Other(name) => Err(format!("`{name}` databases are not supported.")),
    }
}

/// The enabled data sources of the project as `(source, dbms, url)`, plus the reasons the others were left out.
pub fn usable_sources(sources: Vec<DataSource>) -> (Vec<(DataSource, Dbms, String)>, Vec<String>) {
    let mut usable = Vec::new();
    let mut skipped = Vec::new();
    for source in sources {
        let label = format!("{} ({})", source.name, source.source_file);
        let Some(url) = source.url.known().cloned() else {
            skipped.push(format!("{label}: the URL is not a literal ({}); create the connection manually.", unknown_reason(&source)));
            continue;
        };
        match source.dialect.known().map(dbms_of) {
            Some(Ok(dbms)) => usable.push((source, dbms, url)),
            Some(Err(why)) => skipped.push(format!("{label}: {why}")),
            None => skipped.push(format!("{label}: the database type is unknown.")),
        }
    }
    (usable, skipped)
}

fn unknown_reason(source: &DataSource) -> String {
    match &source.url {
        persistence_core::schema::Fact::Unknown(reason) => reason.clone(),
        persistence_core::schema::Fact::Known(_) => String::new(),
    }
}

fn from_spring(ctx: &Invocation) -> Result<(), Error> {
    let Some(project) = ctx.project() else {
        fail("Open a project first.");
        return Ok(());
    };
    let (usable, skipped) = usable_sources(datasource_facts::discover(&project));
    if usable.is_empty() {
        let why = if skipped.is_empty() { "No spring.datasource, quarkus.datasource or persistence.xml JDBC URL was found.".to_owned() } else { skipped.join("\n") };
        fail(&format!("No usable data source in this project.\n{why}"));
        return Ok(());
    }
    let items = usable
        .iter()
        .enumerate()
        .map(|(i, (source, dbms, url))| {
            let profile = source.profile.as_deref().map(|p| format!(" [{p}]")).unwrap_or_default();
            PickItem::new(&i.to_string(), &format!("{}{profile}", source.name)).description(&format!("{} {url}", dbms.label()))
        })
        .collect();
    let Some(key) = pick_one("Create a connection from which data source?", items)? else {
        return Ok(());
    };
    let Some((source, dbms, url)) = key.parse::<usize>().ok().and_then(|i| usable.into_iter().nth(i)) else {
        return Ok(());
    };
    if let Some(why) = url_problem(dbms, &url) {
        fail(&format!("This data source cannot be used: {why}"));
        return Ok(());
    }
    let default_name = match &source.profile {
        Some(profile) => format!("{}-{profile}", source.name),
        None => source.name.clone(),
    };
    let spring_source = SpringSource { root: project.root()?, file: source.source_file.clone(), prefix: source.key_prefix.clone() };
    let Some(name) = host::input("Connection name", Some(&default_name))? else {
        return Ok(());
    };
    let Some(user) = host::input("Database user (the password is asked at connect time)", source.username.as_deref())? else {
        return Ok(());
    };
    let Some(jar) = ask_driver_jar(dbms, None, ctx.project().as_ref())? else {
        return Ok(());
    };
    let profile = Profile {
        name: name.trim().to_owned(),
        dbms,
        url_template: url,
        user: Some(user.trim().to_owned()).filter(|u| !u.is_empty()),
        driver_jar: Some(jar),
        schema_filter: Default::default(),
        spring_source: Some(spring_source),
    };
    save_new(profile)
}

fn manual(ctx: &Invocation) -> Result<(), Error> {
    let items = Dbms::ENABLED.iter().map(|d| PickItem::new(d.key(), d.label())).collect();
    let Some(key) = pick_one("Database", items)? else {
        return Ok(());
    };
    let Some(dbms) = Dbms::from_key(&key) else {
        return Ok(());
    };
    let url = if dbms == Dbms::H2 {
        let Some(url) = host::input("H2 URL: jdbc:h2:file:<absolute path without .mv.db> or jdbc:h2:tcp://host:port/name", Some("jdbc:h2:file:"))? else {
            return Ok(());
        };
        url
    } else {
        let Some(server) = host::input("Host", Some("localhost"))? else {
            return Ok(());
        };
        let port_default = dbms.default_port().map(|p| p.to_string());
        let Some(port) = host::input("Port", port_default.as_deref())? else {
            return Ok(());
        };
        let Some(database) = host::input("Database name", None)? else {
            return Ok(());
        };
        format!("{}//{}:{}/{}", dbms.url_prefix(), server.trim(), port.trim(), database.trim())
    };
    if let Some(why) = url_problem(dbms, &url) {
        fail(&format!("The connection was not saved: {why}"));
        return Ok(());
    }
    let Some(user) = host::input("Database user (the password is asked at connect time)", None)? else {
        return Ok(());
    };
    let Some(jar) = ask_driver_jar(dbms, None, ctx.project().as_ref())? else {
        return Ok(());
    };
    let default_name = format!("{} {}", dbms.label(), persistence_core::connection::describe_target(url.trim()));
    let Some(name) = host::input("Connection name", Some(&default_name))? else {
        return Ok(());
    };
    save_new(Profile {
        name: name.trim().to_owned(),
        dbms,
        url_template: url.trim().to_owned(),
        user: Some(user.trim().to_owned()).filter(|u| !u.is_empty()),
        driver_jar: Some(jar),
        schema_filter: Default::default(),
        spring_source: None,
    })
}

/// Refuses an unusable URL right after it is entered, before more questions are asked.
fn url_problem(dbms: Dbms, url: &str) -> Option<String> {
    let probe = Profile { name: "check".into(), dbms, url_template: url.trim().to_owned(), user: None, driver_jar: None, schema_filter: Default::default(), spring_source: None };
    probe.validate().err().map(|e| e.to_string())
}

fn save_new(profile: Profile) -> Result<(), Error> {
    if let Err(why) = profile.validate() {
        fail(&format!("The connection was not saved: {why}"));
        return Ok(());
    }
    let mut profiles = match load_profiles() {
        Ok(profiles) => profiles,
        Err(Error::Invalid(why)) => {
            fail(&format!("{why}. Nothing was saved."));
            return Ok(());
        }
        Err(other) => return Err(other),
    };
    let exists = profiles.iter().any(|p| p.name.trim().eq_ignore_ascii_case(profile.name.trim()));
    if exists && !host::confirm("Replace connection?", &format!("A connection named `{}` exists. Replace it?", profile.name))? {
        return Ok(());
    }
    let name = profile.name.clone();
    if let Err(why) = upsert(&mut profiles, profile) {
        fail(&format!("The connection was not saved: {why}"));
        return Ok(());
    }
    save_profiles(&profiles)?;
    info(&format!("Saved connection `{name}`. Run JPA: Test Database Connection; the password is asked then and never saved."));
    Ok(())
}

const MANUAL_JAR: &str = "manual";

/// Project-local jars named for the family. Only asks the host (`project.find_files`, never reads a jar); the host
/// skips hidden and gitignored folders and build caches live outside the project, so absence proves nothing.
fn project_driver_jars(dbms: Dbms, project: Option<&Project>) -> Vec<String> {
    let (Some(project), Some(glob)) = (project, dbms.jar_glob()) else {
        return Vec::new();
    };
    let Ok(root) = project.root() else {
        return Vec::new();
    };
    let found = project.find_files(&glob, MAX_JAR_SEARCH).unwrap_or_default();
    driver_jar_candidates(dbms, &root, &found)
}

fn jar_pick_items(dbms: Dbms, jars: &[String]) -> Vec<PickItem> {
    let note = if jars.is_empty() {
        format!("No {} driver jar found in the project. Hidden and gitignored folders are skipped, and ~/.m2 and ~/.gradle are not searched (the plugin has no access to them).", dbms.label())
    } else {
        "Not listed: hidden and gitignored folders, ~/.m2 and ~/.gradle (the plugin has no access to them).".to_owned()
    };
    let mut items: Vec<PickItem> = jars.iter().enumerate().map(|(i, jar)| PickItem::new(&i.to_string(), jar.rsplit('/').next().unwrap_or(jar)).description(jar)).collect();
    items.push(PickItem::new(MANUAL_JAR, "Type a path").description(&note));
    items
}

/// Offers the jars found in the project; the choice only pre-fills the path prompt, it is never used unasked.
/// `None` means the user dismissed the list, `Some(None)` means "type a path".
fn offer_project_jars(dbms: Dbms, project: Option<&Project>) -> Result<Option<Option<String>>, Error> {
    let jars = project_driver_jars(dbms, project);
    let items = jar_pick_items(dbms, &jars);
    let Some(key) = pick_one(&format!("{} driver jar", dbms.label()), items)? else {
        return Ok(None);
    };
    Ok(Some(key.parse::<usize>().ok().and_then(|i| jars.into_iter().nth(i))))
}

/// Asks for an absolute jar path (the user-supplied driver) and remembers it per family in the plugin directory.
/// With no remembered jar the project's own jars are offered first; the chosen one still goes through the same prompt.
fn ask_driver_jar(dbms: Dbms, current: Option<&str>, project: Option<&Project>) -> Result<Option<String>, Error> {
    let cached = load_drivers()?.get(&dbms).cloned();
    let mut initial = current.map(str::to_owned).or(cached);
    if initial.is_none() {
        let Some(chosen) = offer_project_jars(dbms, project)? else {
            return Ok(None);
        };
        initial = chosen;
    }
    let prompt = format!("Absolute path of the {} JDBC driver jar (JPA Assist never downloads drivers)", dbms.label());
    let Some(path) = host::input(&prompt, initial.as_deref())? else {
        return Ok(None);
    };
    let path = path.trim().to_owned();
    if let Err(why) = valid_jar_path(&path) {
        fail(&format!("Driver jar not accepted: {why}"));
        return Ok(None);
    }
    let mut drivers = load_drivers()?;
    drivers.insert(dbms, path.clone());
    save_drivers(&drivers)?;
    Ok(Some(path))
}

fn set_driver(ctx: &Invocation) -> Result<(), Error> {
    let items = Dbms::ENABLED.iter().map(|d| PickItem::new(d.key(), d.label())).collect();
    let Some(key) = pick_one("Driver for which database?", items)? else {
        return Ok(());
    };
    let Some(dbms) = Dbms::from_key(&key) else {
        return Ok(());
    };
    if let Some(path) = ask_driver_jar(dbms, None, ctx.project().as_ref())? {
        info(&format!("{} driver jar set to {path}. Connections without their own jar use it.", dbms.label()));
    }
    Ok(())
}

fn edit_profile(title: &str, change: impl Fn(&mut Profile), done: &str) -> Result<(), Error> {
    let Some(mut profile) = pick_profile(title)? else {
        return Ok(());
    };
    let name = profile.name.clone();
    change(&mut profile);
    let mut profiles = load_profiles()?;
    if let Some(at) = profiles.iter().position(|p| p.name == name) {
        profiles[at] = profile;
        save_profiles(&profiles)?;
        info(&format!("`{name}`: {done}"));
    }
    Ok(())
}

fn delete_profile() -> Result<(), Error> {
    let Some(profile) = pick_profile("Delete which connection?")? else {
        return Ok(());
    };
    if !host::confirm("Delete connection?", &format!("Delete `{}`? The driver jar and the database are not touched.", profile.name))? {
        return Ok(());
    }
    let mut profiles = load_profiles()?;
    profiles.retain(|p| p.name != profile.name);
    save_profiles(&profiles)?;
    info(&format!("Deleted connection `{}`.", profile.name));
    Ok(())
}

#[cfg(test)]
#[path = "jdbc_connect_tests.rs"]
mod tests;
