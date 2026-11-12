use super::*;
use openidea_plugin::mock::MockHost;
use openidea_plugin::ProcessOutput;
use persistence_core::connection::{SchemaFilter, parse_profiles as parse};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::rc::Rc;

const SENTINEL: &str = "S3NT1NEL-pw";
const JAR: &str = "/m2/postgresql-42.7.4.jar";

enum A {
    Ok(Value),
    Paged(Value, Option<u64>),
    Err(&'static str, &'static str),
}

fn envelope(request: &Value, answer: A) -> String {
    let id = &request["id"];
    let body = match answer {
        A::Ok(result) => json!({"v":1,"id":id,"ok":true,"truncated":false,"next":null,"result":result}),
        A::Paged(result, next) => json!({"v":1,"id":id,"ok":true,"truncated":next.is_some(),"next":next,"result":result}),
        A::Err(code, message) => json!({"v":1,"id":id,"ok":false,"error":{"code":code,"message":message,"sqlState":null,"vendorCode":0,"retryable":false}}),
    };
    format!("{body}\n")
}

type Seen = Rc<RefCell<Vec<(Vec<String>, String)>>>;

fn serve(host: &MockHost, answer: impl Fn(&Value) -> A + 'static) -> Seen {
    let seen: Seen = Rc::default();
    let log = seen.clone();
    host.on_process("java", move |args, stdin| {
        let stdin = stdin.unwrap_or("");
        log.borrow_mut().push((args.to_vec(), stdin.to_owned()));
        let request: Value = serde_json::from_str(stdin).unwrap();
        Ok(ProcessOutput { status: Some(0), stdout: envelope(&request, answer(&request)), stderr: String::new() })
    });
    seen
}

fn ops(seen: &Seen) -> Vec<String> {
    seen.borrow().iter().map(|(_, stdin)| serde_json::from_str::<Value>(stdin).unwrap()["op"].as_str().unwrap().to_owned()).collect()
}

fn table(schema: &str, name: &str, kind: &str) -> Value {
    json!({"catalog":null,"schema":schema,"name":name,"type":kind,"remarks":null})
}

fn page_of(names: &[String], request: &Value) -> A {
    let offset = request["args"]["offset"].as_u64().unwrap_or(0) as usize;
    let limit = request["args"]["limit"].as_u64().unwrap_or(200) as usize;
    let schema = request["args"]["schema"].as_str().unwrap_or("public");
    let slice: Vec<Value> = names.iter().skip(offset).take(limit).map(|n| table(schema, n, if n.starts_with("v_") { "VIEW" } else { "TABLE" })).collect();
    let next = (offset + limit < names.len()).then_some((offset + limit) as u64);
    A::Paged(json!({"tables": slice}), next)
}

fn pg(request: &Value) -> A {
    match request["op"].as_str().unwrap() {
        "ping" => A::Ok(json!({"helperVersion":"1.0.0","protocol":1,"java":{"version":"21.0.4","vendor":"Test","jdk":true},"os":"Linux"})),
        "testConnection" => A::Ok(json!({"productName":"PostgreSQL","productVersion":"16.2","driverName":"PostgreSQL JDBC Driver","driverVersion":"42.7.4","catalog":"app","schema":"public","readOnly":true})),
        "listSchemas" => A::Ok(json!({"catalogs":["app"],"schemas":[
            {"catalog":"app","name":"public","isDefault":true,"system":false},
            {"catalog":"app","name":"audit","isDefault":false,"system":false},
            {"catalog":"app","name":"pg_catalog","isDefault":false,"system":true}],"current":{"catalog":"app","schema":"public"}})),
        "listTables" => page_of(&["customers".to_owned(), "orders".to_owned(), "v_totals".to_owned()], request),
        "describeTables" => {
            let tables: Vec<Value> = request["args"]["tables"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| {
                    let name = n.as_str().unwrap();
                    let kind = if name.starts_with("v_") { "VIEW" } else { "TABLE" };
                    json!({"name":name,"schema":request["args"]["schema"],"found":true,"type":kind,
                        "columns":[{"name":"id","typeName":"int8","dataType":-5,"nullable":0,"position":1},{"name":"doc","typeName":"jsonb","dataType":1111,"nullable":1,"position":2}],
                        "primaryKey":{"name":"pk","columns":["id"]},"foreignKeys":[],"indexes":[]})
                })
                .collect();
            A::Ok(json!({"tables": tables}))
        }
        other => panic!("unexpected op {other}"),
    }
}

fn profile() -> Profile {
    Profile {
        name: "dev".into(),
        dbms: Dbms::PostgreSql,
        url_template: "jdbc:postgresql://db.example:5432/app".into(),
        user: Some("app".into()),
        driver_jar: Some(JAR.into()),
        schema_filter: SchemaFilter::default(),
        spring_source: None,
    }
}

fn setup(profiles: &[Profile]) -> MockHost {
    let host = MockHost::install().with_capabilities(&["project.read", "storage", "process:java"]);
    host::storage::set(PROFILES_KEY, &profiles_json(profiles)).unwrap();
    host
}

fn say(host: &MockHost, answers: &[&str]) {
    for answer in answers {
        host.answer_next_prompt(Some(&[answer]));
    }
}

fn many(host: &MockHost, answers: &[&str]) {
    host.answer_next_prompt(Some(answers));
}

fn last(host: &MockHost) -> (NotificationLevel, String) {
    let n = host.notifications().pop().expect("a notification");
    (n.level, n.body)
}

fn assert_no_secret(host: &MockHost, seen: &Seen) {
    for n in host.notifications() {
        assert!(!n.title.contains(SENTINEL) && !n.body.contains(SENTINEL), "notification leaks the password: {}", n.body);
    }
    for (key, value) in host.storage() {
        assert!(!value.contains(SENTINEL), "storage {key} holds the password");
    }
    for (_, message) in host.logs() {
        assert!(!message.contains(SENTINEL));
    }
    for (args, _) in seen.borrow().iter() {
        assert!(args.iter().all(|a| !a.contains(SENTINEL)), "argv leaks the password");
    }
    let dir = host::fs::plugin_dir().unwrap();
    for file in [DRIVERS_FILE, HELPER_FILE] {
        if let Ok(text) = host::fs::read_file(&format!("{dir}/{file}")) {
            assert!(!text.contains(SENTINEL));
        }
    }
}

// ---- Client-level, fake runner ----

struct Fake {
    answer: Box<dyn FnMut(&Value) -> A>,
    requests: Vec<Value>,
}

impl Runner for Fake {
    fn run(&mut self, stdin: &str, _timeout_ms: u64) -> Result<RunOutput, LaunchError> {
        let request: Value = serde_json::from_str(stdin).unwrap();
        let reply = (self.answer)(&request);
        self.requests.push(request.clone());
        Ok(RunOutput { code: 0, stdout: envelope(&request, reply), stderr: String::new() })
    }
}

fn fake(answer: impl FnMut(&Value) -> A + 'static) -> Client<Fake> {
    let connection = Connection {
        url: "jdbc:postgresql://db/app".into(),
        user: Some("app".into()),
        driver: DriverSpec { jars: vec![JAR.into()], class: "org.postgresql.Driver".into() },
        props: BTreeMap::new(),
    };
    Client::new(Fake { answer: Box::new(answer), requests: vec![] }, connection, Some(SENTINEL.into()))
}

#[test]
fn probe_runs_ping_then_connect_and_stops_when_cancelled_between_launches() {
    let mut client = fake(pg);
    let (ping, info) = probe(&mut client, &|| false).unwrap();
    assert_eq!((ping.java.version.as_str(), info.product_name.as_str()), ("21.0.4", "PostgreSQL"));
    assert_eq!(client.into_runner().requests.len(), 2);

    let mut client = fake(pg);
    assert_eq!(probe(&mut client, &|| true).unwrap_err().code, ErrorCode::Cancelled);
    assert_eq!(client.into_runner().requests.len(), 1, "no connection is attempted after cancel");

    let mut old = fake(|_| A::Ok(json!({"helperVersion":"1.0.0","protocol":1,"java":{"version":"11.0.2","vendor":"x","jdk":true},"os":"x"})));
    assert_eq!(probe(&mut old, &|| false).unwrap_err().code, ErrorCode::JvmTooOld);
}

fn many_tables(count: usize, matching: &'static str) -> impl FnMut(&Value) -> A {
    move |request| {
        let pattern = request["args"]["pattern"].as_str().unwrap_or("");
        let names: Vec<String> = if pattern.is_empty() { (0..count).map(|i| format!("t{i:04}")).collect() } else { (0..3).map(|i| format!("{matching}{i}")).collect() };
        page_of(&names, request)
    }
}

#[test]
fn five_hundred_tables_list_without_asking_and_five_hundred_one_require_a_filter() {
    let mut client = fake(many_tables(500, "ord"));
    let (listed, pattern) = list_with_filter(&mut client, &[Some("public".into())], None, &mut |_| panic!("no filter needed")).unwrap();
    assert_eq!((listed.len(), pattern), (500, None));
    assert!(client.into_runner().requests.len() >= 3, "paged listing follows the cursor");

    let asked = RefCell::new(Vec::new());
    let mut client = fake(many_tables(501, "ord"));
    let (listed, pattern) = list_with_filter(&mut client, &[Some("public".into())], None, &mut |count| {
        asked.borrow_mut().push(count);
        Ok(Some("ord%".into()))
    })
    .unwrap();
    assert_eq!(asked.into_inner(), vec![501]);
    assert_eq!((listed.len(), pattern.as_deref()), (3, Some("ord%")));
}

#[test]
fn a_filter_that_still_matches_too_many_fails_instead_of_truncating() {
    let mut client = fake(|request| {
        let names: Vec<String> = (0..501).map(|i| format!("t{i}")).collect();
        page_of(&names, request)
    });
    let mut asked = 0;
    let result = list_with_filter(&mut client, &[None], None, &mut |_| {
        asked += 1;
        Ok(Some("t%".into()))
    });
    assert!(matches!(result, Err(ListError::StillTooMany(501))), "{result:?}");
    assert_eq!(asked, FILTER_ATTEMPTS);

    let mut client = fake(many_tables(501, "x"));
    assert!(matches!(list_with_filter(&mut client, &[None], None, &mut |_| Ok(None)), Err(ListError::Declined)));
    let mut client = fake(many_tables(501, "x"));
    assert!(matches!(list_with_filter(&mut client, &[None], None, &mut |_| Ok(Some("  ".into()))), Err(ListError::Declined)));
}

#[test]
fn views_do_not_count_toward_the_snapshot_bound() {
    let mut client = fake(|request| {
        let mut names: Vec<String> = (0..500).map(|i| format!("t{i}")).collect();
        names.extend((0..10).map(|i| format!("v_{i}")));
        page_of(&names, request)
    });
    let (listed, _) = list_with_filter(&mut client, &[None], None, &mut |_| panic!("views are not tables")).unwrap();
    assert_eq!(listed.len(), 510);
}

#[test]
fn describe_selection_chunks_by_schema_reports_progress_and_honours_cancel() {
    let selection: Vec<(Option<String>, String)> = (0..30).map(|i| (Some("a".to_owned()), format!("t{i}"))).chain((0..3).map(|i| (Some("b".to_owned()), format!("u{i}")))).collect();
    let progress = RefCell::new(Vec::new());
    let mut client = fake(pg);
    let described = describe_selection(&mut client, &selection, &|| false, &|done, total| progress.borrow_mut().push((done, total))).unwrap();
    assert_eq!(described.len(), 33);
    assert!(described.iter().all(|m| m.schema.is_some()));
    let sizes: Vec<usize> = client.into_runner().requests.iter().map(|r| r["args"]["tables"].as_array().unwrap().len()).collect();
    assert_eq!(sizes, vec![25, 5, 3]);
    assert_eq!(progress.into_inner(), vec![(0, 33), (25, 33), (30, 33)]);

    let flag = Rc::new(std::cell::Cell::new(false));
    let seen = flag.clone();
    let mut client = fake(move |request| {
        seen.set(true);
        pg(request)
    });
    let result = describe_selection(&mut client, &selection, &|| flag.get(), &|_, _| {});
    assert_eq!(result.unwrap_err().code, ErrorCode::Cancelled);
    assert_eq!(client.into_runner().requests.len(), 1, "cancel stops before the next chunk");
}

// ---- command level, MockHost ----

#[test]
fn test_command_reports_the_server_and_never_leaks_the_password() {
    let host = setup(&[profile()]);
    let seen = serve(&host, pg);
    say(&host, &["confirm"]);
    say(&host, &[SENTINEL]);
    run("jdbc-test", &host.invocation()).unwrap();

    let (level, body) = last(&host);
    assert_eq!(level, NotificationLevel::Info);
    assert!(body.contains("PostgreSQL 16.2") && body.contains("Java 21.0.4"), "{body}");
    assert_eq!(ops(&seen), vec!["ping", "testConnection"]);
    let connect: Value = serde_json::from_str(&seen.borrow()[1].1).unwrap();
    assert_eq!(connect["secrets"]["password"], SENTINEL);
    assert_eq!(connect["conn"]["driver"], json!({"jars":[JAR],"class":"org.postgresql.Driver"}));
    assert!(!seen.borrow()[0].1.contains(SENTINEL), "ping carries no secrets");
    assert_no_secret(&host, &seen);

    // The URL was confirmed once; the second run only asks for the password.
    say(&host, &[SENTINEL]);
    run("jdbc-test", &host.invocation()).unwrap();
    assert_eq!(ops(&seen).len(), 4);
    assert_no_secret(&host, &seen);
    assert_eq!(host.storage().get(PROFILES_KEY).map(|t| parse(Some(t)).unwrap().len()), Some(1));
}

#[test]
fn the_helper_is_installed_once_into_the_plugin_directory_and_run_with_safe_flags() {
    let host = setup(&[profile()]);
    let seen = serve(&host, pg);
    say(&host, &["confirm", SENTINEL]);
    run("jdbc-test", &host.invocation()).unwrap();
    let (args, _) = seen.borrow()[0].clone();
    let dir = host::fs::plugin_dir().unwrap();
    assert_eq!(args.last().unwrap(), &format!("{dir}/{HELPER_FILE}"));
    assert!(args.contains(&"-Duser.language=en".to_owned()) && args.contains(&"-Xmx256m".to_owned()));
    assert_eq!(host::fs::read_bytes(args.last().unwrap()).unwrap(), HELPER.as_bytes());
}

#[test]
fn a_changed_target_asks_for_confirmation_again_and_a_refusal_runs_nothing() {
    let host = setup(&[profile()]);
    let seen = serve(&host, pg);
    host.answer_next_prompt(None);
    run("jdbc-test", &host.invocation()).unwrap();
    assert!(seen.borrow().is_empty(), "no launch and no password prompt without confirmation");

    say(&host, &["confirm", SENTINEL]);
    run("jdbc-test", &host.invocation()).unwrap();
    assert_eq!(ops(&seen).len(), 2);

    let mut moved = profile();
    moved.url_template = "jdbc:postgresql://other.example:5432/app".into();
    host::storage::set(PROFILES_KEY, &profiles_json(&[moved])).unwrap();
    host.answer_next_prompt(None);
    run("jdbc-test", &host.invocation()).unwrap();
    assert_eq!(ops(&seen).len(), 2, "the new URL was not confirmed");
}

#[test]
fn dismissing_the_password_prompt_cancels_before_any_launch() {
    let host = setup(&[profile()]);
    let seen = serve(&host, pg);
    say(&host, &["confirm"]);
    host.answer_next_prompt(None);
    run("jdbc-test", &host.invocation()).unwrap();
    assert!(seen.borrow().is_empty());
}

#[test]
fn an_empty_password_is_allowed_and_sent_as_empty() {
    let host = setup(&[profile()]);
    let seen = serve(&host, pg);
    say(&host, &["confirm", ""]);
    run("jdbc-test", &host.invocation()).unwrap();
    let connect: Value = serde_json::from_str(&seen.borrow()[1].1).unwrap();
    assert_eq!(connect["secrets"]["password"], "");
}

#[test]
fn every_helper_error_code_reaches_the_user_as_its_taxonomy_message() {
    let cases: [(&'static str, ErrorCode); 16] = [
        ("DriverNotFound", ErrorCode::DriverNotFound),
        ("DriverLoadFailed", ErrorCode::DriverLoadFailed),
        ("UrlUnsupported", ErrorCode::UrlUnsupported),
        ("UrlRejected", ErrorCode::UrlRejected),
        ("ConnectTimeout", ErrorCode::ConnectTimeout),
        ("NetworkUnreachable", ErrorCode::NetworkUnreachable),
        ("TlsFailed", ErrorCode::TlsFailed),
        ("AuthFailed", ErrorCode::AuthFailed),
        ("PermissionDenied", ErrorCode::PermissionDenied),
        ("SchemaNotFound", ErrorCode::SchemaNotFound),
        ("ResultTooLarge", ErrorCode::ResultTooLarge),
        ("DeadlineExceeded", ErrorCode::DeadlineExceeded),
        ("ProtocolVersion", ErrorCode::ProtocolVersion),
        ("BadRequest", ErrorCode::BadRequest),
        ("SqlError", ErrorCode::SqlError),
        ("Internal", ErrorCode::Internal),
    ];
    for (code, expected) in cases {
        let host = setup(&[profile()]);
        let seen = serve(&host, move |request| if request["op"] == "ping" { pg(request) } else { A::Err(code, "boom") });
        say(&host, &["confirm", SENTINEL]);
        run("jdbc-test", &host.invocation()).unwrap();
        let (level, body) = last(&host);
        let error = HelperError { code: expected, message: "boom".into(), sql_state: None, vendor_code: 0, retryable: false };
        assert_eq!((level, body), (NotificationLevel::Error, error_message(&error)), "{code}");
        assert_eq!(ops(&seen), vec!["ping", "testConnection"], "{code}: no automatic retry");
        assert_no_secret(&host, &seen);
    }
}

#[test]
fn launch_failures_map_to_jvm_budget_cancel_and_protocol_messages() {
    let run_with = |setup_host: &dyn Fn(&MockHost), expected: ErrorCode| {
        let host = setup(&[profile()]);
        setup_host(&host);
        say(&host, &["confirm", SENTINEL]);
        run("jdbc-test", &host.invocation()).unwrap();
        let (level, body) = last(&host);
        assert_eq!(level == NotificationLevel::Info, expected == ErrorCode::Cancelled);
        assert!(!body.contains(SENTINEL));
        body
    };
    let missing = run_with(&|h| h.on_process("java", |_, _| Ok(ProcessOutput { status: Some(127), stdout: String::new(), stderr: "java: not found".into() })), ErrorCode::JvmMissing);
    assert!(missing.starts_with("Java was not found"));
    let jre = run_with(&|h| h.on_process("java", |_, _| Ok(ProcessOutput { status: Some(1), stdout: String::new(), stderr: "Module jdk.compiler not in boot Layer".into() })), ErrorCode::JdkRequired);
    assert!(jre.contains("no compiler"));
    let budget = run_with(&|h| h.on_process("java", |_, _| Err(Error::Failed("java timed out after 5s".into()))), ErrorCode::HostBudget);
    assert!(budget.contains("time the IDE allows"));
    let cancel = run_with(&|h| h.on_process("java", |_, _| Err(Error::Cancelled)), ErrorCode::Cancelled);
    assert!(cancel.starts_with("Cancelled"));
    let spawn = run_with(&|h| h.on_process("java", |_, _| Err(Error::NotFound("java".into()))), ErrorCode::JvmMissing);
    assert!(spawn.starts_with("Java was not found"));
    let garbage = run_with(&|h| h.on_process("java", |_, _| Ok(ProcessOutput { status: Some(0), stdout: "not json\n".into(), stderr: format!("boom {SENTINEL}") })), ErrorCode::HelperProtocol);
    assert!(garbage.contains("did not answer as expected"));
}

#[test]
fn a_denied_process_grant_explains_what_to_allow_and_launches_nothing() {
    let host = MockHost::install().with_capabilities(&["project.read", "storage"]);
    host::storage::set(PROFILES_KEY, &profiles_json(&[profile()])).unwrap();
    say(&host, &["confirm", SENTINEL]);
    run("jdbc-test", &host.invocation()).unwrap();
    let (level, body) = last(&host);
    assert_eq!(level, NotificationLevel::Error);
    assert!(body.contains("process:java") && body.contains("Settings"), "{body}");
    assert!(host.process_runs().is_empty());
    assert!(!body.contains(SENTINEL));
}

#[test]
fn missing_driver_jar_and_invalid_profiles_stop_before_prompting() {
    let mut no_jar = profile();
    no_jar.driver_jar = None;
    let host = setup(&[no_jar]);
    let seen = serve(&host, pg);
    run("jdbc-test", &host.invocation()).unwrap();
    assert!(last(&host).1.contains("No PostgreSQL driver jar is set"));
    assert!(seen.borrow().is_empty());

    let stored = r#"{"version":1,"profiles":[{"name":"x","dbms":"postgresql","url_template":"jdbc:postgresql://u:p@h/d","user":null}]}"#;
    host::storage::set(PROFILES_KEY, stored).unwrap();
    run("jdbc-test", &host.invocation()).unwrap();
    assert!(last(&host).1.contains("remove the user name"));
    assert_eq!(host.storage()[PROFILES_KEY], stored, "a bad stored list is left untouched");
}

#[test]
fn no_profiles_points_at_manage() {
    let host = setup(&[]);
    run("jdbc-browse", &host.invocation()).unwrap();
    assert!(last(&host).1.contains("Manage Database Connections"));
}

// ---- manage ----

fn answer_manual_pg(host: &MockHost) {
    say(host, &["manual", "postgresql", "db.example", "5432", "app", "app", "manual", JAR, "dev"]);
}

#[test]
fn manual_connection_is_saved_without_a_password_and_remembers_the_driver() {
    let host = setup(&[]);
    answer_manual_pg(&host);
    run("jdbc-manage", &host.invocation()).unwrap();
    let profiles = parse(host.storage().get(PROFILES_KEY).map(String::as_str)).unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].url_template, "jdbc:postgresql://db.example:5432/app");
    assert_eq!(profiles[0].driver_jar.as_deref(), Some(JAR));
    let drivers = load_drivers().unwrap();
    assert_eq!(drivers.get(&Dbms::PostgreSql).map(String::as_str), Some(JAR));
    assert!(last(&host).1.contains("password is asked then and never saved"));

    // Saving the same name asks before replacing.
    answer_manual_pg(&host);
    host.answer_next_prompt(None);
    run("jdbc-manage", &host.invocation()).unwrap();
    assert_eq!(parse(host.storage().get(PROFILES_KEY).map(String::as_str)).unwrap().len(), 1);
}

#[test]
fn manual_urls_with_credentials_in_memory_h2_or_h2_settings_are_refused_with_a_reason() {
    for (url, reason) in [
        ("jdbc:h2:file:/data/x;PASSWORD=S3NT1NEL-pw", "user name and password"),
        ("jdbc:h2:mem:test", "in-memory"),
        ("jdbc:h2:file:/d/x;INIT=RUNSCRIPT FROM 'x'", "settings"),
    ] {
        let host = setup(&[]);
        say(&host, &["manual", "h2", url]);
        run("jdbc-manage", &host.invocation()).unwrap();
        let (level, body) = last(&host);
        assert_eq!(level, NotificationLevel::Error);
        assert!(body.contains(reason) && !body.contains(SENTINEL), "{url}: {body}");
        assert!(parse(host.storage().get(PROFILES_KEY).map(String::as_str)).unwrap().is_empty());
    }
    let host = setup(&[]);
    say(&host, &["manual", "postgresql", "u:S3NT1NEL-pw@db", "5432", "a"]);
    run("jdbc-manage", &host.invocation()).unwrap();
    assert!(last(&host).1.contains("remove the user name and password"));
    assert!(!last(&host).1.contains(SENTINEL));

    let host = setup(&[]);
    say(&host, &["manual", "postgresql", "db", "5432", "a", "u", "manual", "relative.jar"]);
    run("jdbc-manage", &host.invocation()).unwrap();
    assert!(last(&host).1.contains("must be absolute"));
}

const APPLICATION: &str = "spring.datasource.url=jdbc:postgresql://db.internal:5432/shop\nspring.datasource.username=app\nspring.datasource.password=S3NT1NEL-pw\n";

#[test]
fn spring_discovery_creates_a_profile_and_the_literal_password_is_read_at_connect_time_only() {
    let host = setup(&[]);
    host.add_file("src/main/resources/application.properties", APPLICATION);
    say(&host, &["spring", "0", "shop", "app", "manual", JAR]);
    run("jdbc-manage", &host.invocation()).unwrap();
    let stored = host.storage()[PROFILES_KEY].clone();
    assert!(!stored.contains(SENTINEL), "{stored}");
    let profiles = parse(Some(&stored)).unwrap();
    let source = profiles[0].spring_source.clone().unwrap();
    assert_eq!((source.prefix.as_str(), profiles[0].user.as_deref()), ("spring.datasource", Some("app")));

    let seen = serve(&host, pg);
    say(&host, &["confirm"]);
    run("jdbc-test", &host.invocation()).unwrap();
    let connect: Value = serde_json::from_str(&seen.borrow()[1].1).unwrap();
    assert_eq!(connect["secrets"]["password"], SENTINEL, "read from the config at call time, no prompt");
    assert!(last(&host).1.contains("works"));
    assert!(!host.storage().values().any(|v| v.contains(SENTINEL)));
    assert_no_secret(&host, &seen);

    host.add_file("src/main/resources/application.properties", "spring.datasource.url=jdbc:postgresql://db.internal:5432/shop\nspring.datasource.password=${DB_PASSWORD}\n");
    say(&host, &["typed-pw"]);
    run("jdbc-test", &host.invocation()).unwrap();
    let connect: Value = serde_json::from_str(&seen.borrow()[3].1).unwrap();
    assert_eq!(connect["secrets"]["password"], "typed-pw", "a placeholder falls back to the prompt");
}

#[test]
fn spring_discovery_skips_disabled_databases_and_placeholder_urls_with_reasons() {
    let host = setup(&[]);
    host.add_file(
        "src/main/resources/application.properties",
        "spring.datasource.url=jdbc:sqlserver://h;databaseName=x\nspring.datasource.username=u\n",
    );
    say(&host, &["spring"]);
    run("jdbc-manage", &host.invocation()).unwrap();
    let body = last(&host).1;
    assert!(body.contains("No usable data source") && body.contains("SQL Server is not enabled yet"), "{body}");

    host.add_file("src/main/resources/application.properties", "spring.datasource.url=jdbc:postgresql://${HOST}/x\n");
    say(&host, &["spring"]);
    run("jdbc-manage", &host.invocation()).unwrap();
    assert!(last(&host).1.contains("not a literal"));

    host.add_file("src/main/resources/application.properties", "spring.datasource.url=jdbc:h2:mem:test\n");
    say(&host, &["spring", "0"]);
    run("jdbc-manage", &host.invocation()).unwrap();
    assert!(last(&host).1.contains("in-memory"));
}

#[test]
fn set_filter_reset_qualify_toggle_and_delete_edit_the_stored_profile() {
    let mut p = profile();
    p.schema_filter = SchemaFilter { schemas: vec!["public".into()], table_pattern: Some("o%".into()), qualify_schema: false };
    let host = setup(&[p]);
    say(&host, &["filter"]);
    run("jdbc-manage", &host.invocation()).unwrap();
    assert!(parse(host.storage().get(PROFILES_KEY).map(String::as_str)).unwrap()[0].schema_filter.is_empty());
    say(&host, &["qualify"]);
    run("jdbc-manage", &host.invocation()).unwrap();
    assert!(parse(host.storage().get(PROFILES_KEY).map(String::as_str)).unwrap()[0].schema_filter.qualify_schema);
    say(&host, &["delete", "ok"]);
    run("jdbc-manage", &host.invocation()).unwrap();
    assert!(parse(host.storage().get(PROFILES_KEY).map(String::as_str)).unwrap().is_empty());
    say(&host, &["driver", "mariadb", "manual", "/m2/mariadb.jar"]);
    run("jdbc-manage", &host.invocation()).unwrap();
    assert_eq!(load_drivers().unwrap().get(&Dbms::MariaDb).map(String::as_str), Some("/m2/mariadb.jar"));
}

// ---- browse ----

#[test]
fn browse_reads_schemas_and_tables_summarises_the_snapshot_and_saves_the_filter() {
    let host = setup(&[profile()]);
    let seen = serve(&host, pg);
    say(&host, &["confirm", SENTINEL]);
    many(&host, &["public"]);
    many(&host, &[ALL_TABLES]);
    run("jdbc-browse", &host.invocation()).unwrap();

    let (level, body) = last(&host);
    assert_eq!(level, NotificationLevel::Info);
    assert!(body.contains("2 tables (1 views, 0 not found), 4 columns"), "{body}");
    assert!(body.contains("jsonb") && body.contains("not proven absent"), "{body}");
    assert_eq!(ops(&seen), vec!["listSchemas", "listTables", "describeTables"]);
    let stored = parse(host.storage().get(PROFILES_KEY).map(String::as_str)).unwrap();
    assert_eq!(stored[0].schema_filter.schemas, vec!["public"]);
    assert_no_secret(&host, &seen);

    // The saved schemas are reused: only confirmation-free password and the table picker remain.
    say(&host, &[SENTINEL]);
    many(&host, &["0", "1"]);
    run("jdbc-browse", &host.invocation()).unwrap();
    assert_eq!(ops(&seen).len(), 6);
    assert!(last(&host).1.contains("2 tables (0 views"), "{}", last(&host).1);
}

#[test]
fn browse_requires_a_filter_above_five_hundred_tables_and_never_truncates() {
    let host = setup(&[profile()]);
    let seen = serve(&host, |request| match request["op"].as_str().unwrap() {
        "listTables" => {
            let pattern = request["args"]["pattern"].as_str().unwrap_or("");
            let names: Vec<String> = if pattern.is_empty() { (0..501).map(|i| format!("t{i:03}")).collect() } else { vec!["ord_a".into(), "ord_b".into()] };
            page_of(&names, request)
        }
        _ => pg(request),
    });
    say(&host, &["confirm", SENTINEL]);
    many(&host, &["public"]);
    say(&host, &["ord%"]);
    many(&host, &[ALL_TABLES]);
    run("jdbc-browse", &host.invocation()).unwrap();

    let requests: Vec<Value> = seen.borrow().iter().map(|(_, s)| serde_json::from_str(s).unwrap()).collect();
    let described: Vec<&Value> = requests.iter().filter(|r| r["op"] == "describeTables").collect();
    assert_eq!(described.len(), 1);
    assert_eq!(described[0]["args"]["tables"], json!(["ord_a", "ord_b"]));
    assert!(requests.iter().filter(|r| r["op"] == "listTables").count() >= 4, "the unfiltered listing was paged fully, then the filtered one ran");
    assert!(last(&host).1.contains("2 tables"));
    let stored = parse(host.storage().get(PROFILES_KEY).map(String::as_str)).unwrap();
    assert_eq!(stored[0].schema_filter.table_pattern.as_deref(), Some("ord%"));

    // Dismissing the filter prompt stops before describing anything.
    let before = seen.borrow().len();
    host::storage::set(PROFILES_KEY, &profiles_json(&[profile()])).unwrap();
    say(&host, &[SENTINEL]);
    many(&host, &["public"]);
    host.answer_next_prompt(None);
    run("jdbc-browse", &host.invocation()).unwrap();
    assert!(!ops(&seen)[before..].contains(&"describeTables".to_owned()));
}

#[test]
fn same_named_tables_in_two_schemas_are_an_error_unless_qualification_is_on() {
    let build = |qualify: bool| {
        let mut p = profile();
        p.schema_filter.qualify_schema = qualify;
        let host = setup(&[p]);
        let seen = serve(&host, |request| match request["op"].as_str().unwrap() {
            "listTables" => page_of(&["customers".to_owned()], request),
            _ => pg(request),
        });
        say(&host, &["confirm", SENTINEL]);
        many(&host, &["public", "audit"]);
        many(&host, &[ALL_TABLES]);
        run("jdbc-browse", &host.invocation()).unwrap();
        (last(&host), host, seen)
    };
    let ((level, body), _, _) = build(false);
    assert_eq!(level, NotificationLevel::Error);
    assert!(body.contains("`customers` exists in more than one selected schema"), "{body}");
    let ((level, body), _, _) = build(true);
    assert_eq!(level, NotificationLevel::Info);
    assert!(body.contains("2 tables"), "{body}");
}

#[test]
fn browse_surfaces_helper_errors_from_each_stage() {
    for failing in ["listSchemas", "listTables", "describeTables"] {
        let host = setup(&[profile()]);
        let seen = serve(&host, move |request| if request["op"] == failing { A::Err("PermissionDenied", "no catalog access") } else { pg(request) });
        say(&host, &["confirm", SENTINEL]);
        many(&host, &["public"]);
        many(&host, &[ALL_TABLES]);
        run("jdbc-browse", &host.invocation()).unwrap();
        let (level, body) = last(&host);
        assert_eq!(level, NotificationLevel::Error, "{failing}");
        assert!(body.contains("may not read this metadata"), "{failing}: {body}");
        assert_no_secret(&host, &seen);
    }
}

#[test]
fn cancelling_the_host_call_during_browse_reports_cancelled() {
    let host = setup(&[profile()]);
    host.on_process("java", |_, _| Err(Error::Cancelled));
    say(&host, &["confirm", SENTINEL]);
    run("jdbc-browse", &host.invocation()).unwrap();
    let (level, body) = last(&host);
    assert_eq!((level, body.starts_with("Cancelled")), (NotificationLevel::Info, true));
}

// ---- hygiene ----

#[test]
fn settings_schema_has_no_credential_setting_and_the_manifest_keeps_java_optional() {
    let schema: Value = serde_json::from_str(include_str!("../settings.schema.json")).unwrap();
    for key in schema["properties"].as_object().unwrap().keys() {
        let lower = key.to_ascii_lowercase();
        assert!(!lower.contains("password") && !lower.contains("secret"), "{key}");
    }
    let manifest = include_str!("../plugin.toml");
    let section = |name: &str| manifest.lines().find(|l| l.starts_with(name)).unwrap_or_default().to_owned();
    assert!(section("optional").contains("process:java") && !section("required").contains("process"));
    for id in ["jdbc-manage", "jdbc-test", "jdbc-browse"] {
        assert!(manifest.contains(&format!("id = \"{id}\"")), "{id}");
        assert!(include_str!("lib.rs").contains(&format!("\"{id}\"")), "{id} is dispatched");
    }
}

#[test]
fn the_bundled_helper_source_is_the_one_in_the_helper_directory() {
    assert!(HELPER.contains("public class Helper") && HELPER.contains("PROTOCOL = 1"));
}

// ---- opt-in: the real helper against real H2 files, through the real command flows ----

fn h2_jar() -> Option<std::path::PathBuf> {
    if let Ok(path) = std::env::var("JPA_JDBC_H2_JAR") {
        return Some(path.into());
    }
    let root = std::path::PathBuf::from(std::env::var("HOME").ok()?).join(".m2/repository/com/h2database/h2");
    let mut versions: Vec<_> = std::fs::read_dir(root).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    versions.sort();
    versions.into_iter().rev().find_map(|dir| {
        let jar = dir.join(format!("h2-{}.jar", dir.file_name()?.to_str()?));
        jar.is_file().then_some(jar)
    })
}

#[test]
fn real_h2_through_the_command_flows() {
    use std::io::Write;
    use std::process::{Command as Proc, Stdio};
    if std::env::var("JPA_JDBC_JAVA").as_deref() != Ok("1") {
        eprintln!("skipped: set JPA_JDBC_JAVA=1");
        return;
    }
    let Some(jar) = h2_jar() else {
        eprintln!("skipped: no H2 jar (set JPA_JDBC_H2_JAR)");
        return;
    };
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dir = std::env::temp_dir().join(format!("jpa-p3-h2-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("demo");
    for script in ["h2-init.sql", "h2-big.sql"] {
        let status = Proc::new("java")
            .args(["-cp"])
            .arg(&jar)
            .args(["org.h2.tools.RunScript", "-url", &format!("jdbc:h2:file:{}", db.display()), "-user", "sa", "-script"])
            .arg(manifest.join("parity/spike").join(script))
            .status()
            .unwrap();
        assert!(status.success());
    }

    let h2 = Profile {
        name: "demo".into(),
        dbms: Dbms::H2,
        url_template: format!("jdbc:h2:file:{}", db.display()),
        user: Some("sa".into()),
        driver_jar: Some(jar.display().to_string()),
        schema_filter: SchemaFilter::default(),
        spring_source: None,
    };
    let host = setup(&[h2]);
    let helper = manifest.join("helper/Helper.java");
    host.on_process("java", move |_, stdin| {
        let mut child = Proc::new("java")
            .args(JAVA_FLAGS)
            .arg(&helper)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Failed(e.to_string()))?;
        child.stdin.take().unwrap().write_all(stdin.unwrap_or("").as_bytes()).unwrap();
        let out = child.wait_with_output().map_err(|e| Error::Failed(e.to_string()))?;
        Ok(ProcessOutput { status: out.status.code(), stdout: String::from_utf8_lossy(&out.stdout).into_owned(), stderr: String::from_utf8_lossy(&out.stderr).into_owned() })
    });

    say(&host, &["confirm", ""]);
    run("jdbc-test", &host.invocation()).unwrap();
    let (level, body) = last(&host);
    assert_eq!(level, NotificationLevel::Info, "{body}");
    assert!(body.contains("H2 2.") && body.contains("Java "), "{body}");

    say(&host, &[SENTINEL]);
    run("jdbc-test", &host.invocation()).unwrap();
    let (level, body) = last(&host);
    assert_eq!(level, NotificationLevel::Error);
    assert!(body.starts_with("Authentication failed"), "{body}");
    assert!(!body.contains(SENTINEL));

    say(&host, &[""]);
    many(&host, &["BIG"]);
    say(&host, &["T00%"]);
    many(&host, &[ALL_TABLES]);
    run("jdbc-browse", &host.invocation()).unwrap();
    let (level, body) = last(&host);
    assert_eq!(level, NotificationLevel::Info, "{body}");
    assert!(body.contains("100 tables") && body.contains("Type mapping for H2 is not available"), "{body}");

    say(&host, &[""]);
    many(&host, &["SHOP"]);
    host.answer_next_prompt(Some(&[ALL_TABLES]));
    let mut reset = parse(host.storage().get(PROFILES_KEY).map(String::as_str)).unwrap();
    reset[0].schema_filter = SchemaFilter::default();
    host::storage::set(PROFILES_KEY, &profiles_json(&reset)).unwrap();
    run("jdbc-browse", &host.invocation()).unwrap();
    let (_, body) = last(&host);
    assert!(body.contains("3 tables (1 views"), "{body}");
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- project-local driver jars ----

fn found_jars(host: &MockHost, dbms: Dbms) -> Vec<String> {
    project_driver_jars(dbms, host.invocation().project().as_ref())
}

#[test]
fn project_jars_are_matched_per_family_and_skip_sources_tests_and_hidden_folders() {
    let host = setup(&[]);
    for path in [
        "lib/postgresql-42.7.4.jar",
        "libs/postgresql-42.7.4-sources.jar",
        "libs/postgresql-42.7.4-javadoc.jar",
        "drivers/mysql-connector-j-8.3.0.jar",
        "drivers/mariadb-java-client-3.3.2.jar",
        "vendor/h2-2.2.224.jar",
        "src/test/lib/postgresql-1.0.jar",
        ".hidden/postgresql-2.0.jar",
        "build/libs/postgresql-43.0.jar",
    ] {
        host.add_file(path, "");
    }
    assert_eq!(found_jars(&host, Dbms::PostgreSql), ["/project/lib/postgresql-42.7.4.jar", "/project/build/libs/postgresql-43.0.jar"]);
    assert_eq!(found_jars(&host, Dbms::MySql), ["/project/drivers/mysql-connector-j-8.3.0.jar"]);
    assert_eq!(found_jars(&host, Dbms::MariaDb), ["/project/drivers/mariadb-java-client-3.3.2.jar"]);
    assert_eq!(found_jars(&host, Dbms::H2), ["/project/vendor/h2-2.2.224.jar"]);
    assert_eq!(found_jars(&host, Dbms::PostgreSql), found_jars(&host, Dbms::PostgreSql));
}

#[test]
fn hostile_jar_names_and_a_missing_project_yield_nothing() {
    let host = setup(&[]);
    host.add_file("lib/postgresql-1\n.jar", "");
    host.add_file("lib/postgresql-2\u{7}.jar", "");
    host.add_file(&format!("lib/postgresql-3{}.jar", "x".repeat(1100)), "");
    host.add_file("lib/postgresql-4.jar.txt", "");
    assert!(found_jars(&host, Dbms::PostgreSql).is_empty());
    assert!(project_driver_jars(Dbms::PostgreSql, None).is_empty());
    assert!(project_driver_jars(Dbms::Oracle, host.invocation().project().as_ref()).is_empty());
}

#[test]
fn the_search_is_bounded_and_the_list_is_capped() {
    let host = setup(&[]);
    for i in 0..30 {
        host.add_file(&format!("lib/d{i:02}/h2-2.{i}.jar"), "");
    }
    let jars = found_jars(&host, Dbms::H2);
    assert_eq!(jars.len(), persistence_core::connection::MAX_JAR_CANDIDATES);
    assert_eq!(jars[0], "/project/lib/d00/h2-2.0.jar");
}

#[test]
fn the_pick_list_names_the_path_and_says_what_is_not_searched() {
    let jars = vec!["/project/lib/postgresql-42.jar".to_owned()];
    let items = jar_pick_items(Dbms::PostgreSql, &jars);
    assert_eq!((items[0].key.as_str(), items[0].description.as_deref()), ("0", Some("/project/lib/postgresql-42.jar")));
    assert_eq!(items.last().unwrap().key, MANUAL_JAR);
    let empty = jar_pick_items(Dbms::PostgreSql, &[]);
    assert_eq!(empty.len(), 1);
    let note = empty[0].description.clone().unwrap();
    assert!(note.contains("No PostgreSQL driver jar found") && note.contains("gitignored") && note.contains("~/.m2") && note.contains("~/.gradle"), "{note}");
}

#[test]
fn choosing_a_found_jar_still_asks_for_the_path_and_validates_it() {
    let host = setup(&[]);
    host.add_file("lib/postgresql-42.7.4.jar", "");
    say(&host, &["driver", "postgresql", "0", "/typed/other.jar"]);
    run("jdbc-manage", &host.invocation()).unwrap();
    assert_eq!(load_drivers().unwrap().get(&Dbms::PostgreSql).map(String::as_str), Some("/typed/other.jar"), "the pick is not used without the prompt");

    let host = setup(&[]);
    host.add_file("lib/postgresql-42.7.4.jar", "");
    say(&host, &["driver", "postgresql", "0", "lib/postgresql-42.7.4.jar"]);
    run("jdbc-manage", &host.invocation()).unwrap();
    assert!(last(&host).1.contains("must be absolute"));
    assert!(load_drivers().unwrap().is_empty());

    let host = setup(&[]);
    host.add_file("lib/postgresql-42.7.4.jar", "");
    say(&host, &["driver", "postgresql"]);
    run("jdbc-manage", &host.invocation()).unwrap();
    assert!(load_drivers().unwrap().is_empty(), "dismissing the list selects nothing");
}

#[test]
fn a_remembered_jar_skips_the_project_search_and_a_found_jar_still_needs_the_connect_confirmation() {
    let mut no_jar = profile();
    no_jar.driver_jar = None;
    let host = setup(&[no_jar]);
    host.add_file("lib/postgresql-42.7.4.jar", "");
    let seen = serve(&host, pg);
    say(&host, &["driver", "postgresql", "0", "/project/lib/postgresql-42.7.4.jar"]);
    run("jdbc-manage", &host.invocation()).unwrap();
    assert!(seen.borrow().is_empty());

    host.answer_next_prompt(None);
    run("jdbc-test", &host.invocation()).unwrap();
    assert!(seen.borrow().is_empty(), "declining the confirm box launches nothing");

    say(&host, &["confirm", "pw"]);
    run("jdbc-test", &host.invocation()).unwrap();
    assert!(!seen.borrow().is_empty());

    say(&host, &["driver", "postgresql", "/project/lib/postgresql-43.jar"]);
    run("jdbc-manage", &host.invocation()).unwrap();
    assert_eq!(load_drivers().unwrap().get(&Dbms::PostgreSql).map(String::as_str), Some("/project/lib/postgresql-43.jar"), "no list when a jar is remembered");
}
