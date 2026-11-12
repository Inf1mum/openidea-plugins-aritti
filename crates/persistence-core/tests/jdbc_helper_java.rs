//! Opt-in: runs the real `plugins/jpa-assist/helper/Helper.java` against H2 through the Rust client.
//! `JPA_JDBC_JAVA=1 cargo test --test jdbc_helper_java`. Needs `java` (JDK 17+) on PATH and an H2 jar:
//! `JPA_JDBC_H2_JAR`, or the newest `~/.m2/repository/com/h2database/h2/*/h2-*.jar`. Skips (passes) otherwise.
use persistence_core::introspect::{Client, Connection, DriverSpec, ErrorCode, LaunchError, RunOutput, Runner};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const PASSWORD: &str = "S3NT1NEL-pw";

fn manifest(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn h2_jar() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("JPA_JDBC_H2_JAR") {
        return Some(PathBuf::from(path));
    }
    let root = PathBuf::from(std::env::var("HOME").ok()?).join(".m2/repository/com/h2database/h2");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(root).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    versions.sort();
    versions.into_iter().rev().find_map(|dir| {
        let jar = dir.join(format!("h2-{}.jar", dir.file_name()?.to_str()?));
        jar.is_file().then_some(jar)
    })
}

struct JavaRunner;

impl Runner for JavaRunner {
    fn run(&mut self, stdin: &str, _timeout_ms: u64) -> Result<RunOutput, LaunchError> {
        let mut child = Command::new("java")
            .args(["-Dfile.encoding=UTF-8", "-Dstdout.encoding=UTF-8", "-Duser.language=en", "-Xmx256m", "-Djava.awt.headless=true", "-XX:TieredStopAtLevel=1"])
            .arg(manifest("../../plugins/jpa-assist/helper/Helper.java"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| LaunchError::NotStarted(e.to_string()))?;
        child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
        let output = child.wait_with_output().map_err(|e| LaunchError::NotStarted(e.to_string()))?;
        Ok(RunOutput {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

fn run_script(jar: &Path, db: &Path, script: &Path) {
    let status = Command::new("java")
        .args(["-cp"])
        .arg(jar)
        .args(["org.h2.tools.RunScript", "-url", &format!("jdbc:h2:file:{}", db.display()), "-user", "sa", "-script"])
        .arg(script)
        .status()
        .unwrap();
    assert!(status.success(), "H2 RunScript failed");
}

fn connection(jar: &Path, url: String) -> Connection {
    Connection {
        url,
        user: Some("sa".into()),
        driver: DriverSpec { jars: vec![jar.display().to_string()], class: "org.h2.Driver".into() },
        props: BTreeMap::new(),
    }
}

fn raw(request: Value) -> Value {
    let output = JavaRunner.run(&request.to_string(), 60_000).unwrap();
    assert!(output.stdout.ends_with('\n') && output.stdout.trim().lines().count() == 1, "one line, got {:?} / stderr {:?}", output.stdout, output.stderr);
    serde_json::from_str(output.stdout.trim()).unwrap()
}

#[test]
fn helper_against_h2() {
    if std::env::var("JPA_JDBC_JAVA").as_deref() != Ok("1") {
        eprintln!("skipped: set JPA_JDBC_JAVA=1");
        return;
    }
    let Some(jar) = h2_jar() else {
        eprintln!("skipped: no H2 jar (set JPA_JDBC_H2_JAR)");
        return;
    };
    let dir = std::env::temp_dir().join(format!("jpa-jdbc-h2-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("demo");
    run_script(&jar, &db, &manifest("../../plugins/jpa-assist/parity/spike/h2-init.sql"));
    run_script(&jar, &db, &manifest("../../plugins/jpa-assist/parity/spike/h2-big.sql"));
    let url = format!("jdbc:h2:file:{}", db.display());

    let mut client = Client::new(JavaRunner, connection(&jar, url.clone()), None);

    let ping = client.ping().unwrap();
    assert!(ping.java.jdk);

    let info = client.test_connection().unwrap();
    assert_eq!(info.product_name, "H2");
    assert!(info.read_only, "ACCESS_MODE_DATA=r is forced for file databases");

    let schemas = client.list_schemas().unwrap();
    assert!(schemas.schemas.iter().any(|s| s.name.as_deref() == Some("SHOP") && !s.system));
    assert!(schemas.schemas.iter().any(|s| s.name.as_deref() == Some("INFORMATION_SCHEMA") && s.system));

    let shop = client.list_all_tables(Some("shop"), None).unwrap();
    let names: Vec<&str> = shop.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["CUSTOMER", "LINE", "ORDERS", "V_CUST"]);
    assert_eq!(shop.iter().find(|t| t.name == "V_CUST").unwrap().kind, "VIEW");
    assert_eq!(shop[0].remarks.as_deref(), Some("customers"));

    client.max_bytes = 4096;
    let page = client.list_tables_page(Some("BIG"), None, 0, 1000).unwrap();
    assert!(page.truncated && page.next.is_some() && page.value.len() < 600, "a 4 KiB cap must truncate 600 tables, got {}", page.value.len());
    let everything = client.list_all_tables(Some("BIG"), None).unwrap();
    assert_eq!(everything.len(), 600);
    client.max_bytes = persistence_core::introspect::DEFAULT_MAX_BYTES;

    let described = client
        .describe_all(Some("SHOP"), &["CUSTOMER".into(), "ORDERS".into(), "LINE".into(), "NO_SUCH".into(), "V_CUST".into()])
        .unwrap();
    let customer = &described[0];
    assert_eq!(customer.primary_key.as_ref().unwrap().columns, vec!["ID"]);
    assert_eq!(customer.columns.as_ref().unwrap()[0].auto_increment.as_deref(), Some("YES"));
    assert_eq!(customer.columns.as_ref().unwrap()[2].default.as_deref(), Some("'anon'"));
    let orders = &described[1];
    let fk = &orders.foreign_keys.as_ref().unwrap()[0];
    assert_eq!((fk.name.as_deref(), fk.ref_table.as_str(), fk.update_rule, fk.delete_rule), (Some("FK_CUST"), "CUSTOMER", Some(1), Some(0)));
    assert_eq!(described[2].primary_key.as_ref().unwrap().columns, vec!["ORDER_ID", "N"]);
    assert!(!described[3].found);
    assert_eq!(described[4].kind.as_deref(), Some("VIEW"));

    let h2_conn = connection(&jar, url.clone());
    let base = |op: &str, conn: &Connection, args: Value| json!({"v":1,"id":"x","op":op,"deadlineMs":20000,"conn":conn,"secrets":{"password":""},"args":args});
    let error_code = |response: &Value| response["error"]["code"].as_str().unwrap_or("").to_owned();

    let mut bad_password = Client::new(JavaRunner, h2_conn.clone(), Some(PASSWORD.into()));
    let err = bad_password.test_connection().unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthFailed, "{err}");
    assert!(!err.message.contains(PASSWORD));

    let mut missing_schema = Client::new(JavaRunner, h2_conn.clone(), None);
    assert_eq!(missing_schema.list_all_tables(Some("NOPE"), None).unwrap_err().code, ErrorCode::SchemaNotFound);

    let mut no_jar = connection(&jar, url.clone());
    no_jar.driver.jars = vec!["/nonexistent/h2.jar".into()];
    assert_eq!(Client::new(JavaRunner, no_jar, None).test_connection().unwrap_err().code, ErrorCode::DriverNotFound);

    let mut wrong_class = connection(&jar, url.clone());
    wrong_class.driver.class = "java.lang.Runtime".into();
    assert_eq!(Client::new(JavaRunner, wrong_class, None).test_connection().unwrap_err().code, ErrorCode::BadRequest);

    let mut foreign_scheme = connection(&jar, "jdbc:sqlite:/tmp/x.db".into());
    assert_eq!(Client::new(JavaRunner, foreign_scheme.clone(), None).test_connection().unwrap_err().code, ErrorCode::UrlUnsupported);
    foreign_scheme.url = "jdbc:postgresql://127.0.0.1:1/db".to_owned();
    assert_eq!(
        Client::new(JavaRunner, foreign_scheme, None).test_connection().unwrap_err().code,
        ErrorCode::UrlUnsupported,
        "the H2 driver returns null for a PostgreSQL URL"
    );

    for hostile in [
        format!("{url};INIT=CREATE TABLE PWNED(A INT)"),
        "jdbc:h2:mem:t".to_owned(),
        "jdbc:postgresql://h/db?socketFactory=x.Y".to_owned(),
        "jdbc:postgresql://h/db?socket%46actory=x.Y".to_owned(),
        "jdbc:mysql://h/db?allowLoadLocalInfile=true".to_owned(),
        "jdbc:mysql://h/db?autoDeserialize=true&queryInterceptors=x.Y".to_owned(),
        "jdbc:postgresql://h/db?password=hunter22".to_owned(),
        "jdbc:sqlserver://h;databaseName=d;authenticationScheme=JavaKerberos".to_owned(),
        "jdbc:oracle:thin:@(DESCRIPTION=(ADDRESS=(PROTOCOL=TCP)(HOST=h)))".to_owned(),
    ] {
        let response = raw(base("testConnection", &connection(&jar, hostile.clone()), json!({})));
        let code = error_code(&response);
        assert!(code == "UrlRejected" || code == "UrlUnsupported", "{hostile} -> {response}");
        assert_ne!(code, "UrlUnsupported", "{hostile} must be rejected, not just unsupported");
    }
    let mut props = connection(&jar, url.clone());
    props.props.insert("INIT".into(), "RUNSCRIPT FROM 'x'".into());
    assert_eq!(error_code(&raw(base("testConnection", &props, json!({})))), "UrlRejected");

    let leaky_url = format!("jdbc:h2:file:{}/{PASSWORD}-missing", dir.display());
    let output = JavaRunner
        .run(&json!({"v":1,"id":"x","op":"testConnection","conn":connection(&jar, leaky_url),"secrets":{"password":PASSWORD}}).to_string(), 60_000)
        .unwrap();
    assert!(output.stdout.contains("not found"), "expected the H2 missing-database error, got {}", output.stdout);
    assert!(!output.stdout.contains(PASSWORD) && !output.stderr.contains(PASSWORD), "the password must be scrubbed from the response and absent from stderr");

    let mut slow = Client::new(JavaRunner, h2_conn.clone(), None);
    slow.deadline_ms = 1;
    assert_eq!(slow.test_connection().unwrap_err().code, ErrorCode::DeadlineExceeded);

    assert_eq!(raw(json!({"v":2,"id":"x","op":"ping"}))["error"]["code"], "ProtocolVersion");
    assert_eq!(raw(json!({"v":1,"id":"x","op":"dropEverything","conn":h2_conn}))["error"]["code"], "BadRequest");
    let too_many: Vec<String> = (0..26).map(|i| format!("T{i}")).collect();
    assert_eq!(raw(base("describeTables", &h2_conn, json!({"schema":"SHOP","tables":too_many})))["error"]["code"], "BadRequest");
    let output = JavaRunner.run("{not json", 10_000).unwrap();
    assert!(output.stdout.contains("\"BadRequest\"") && output.stdout.ends_with('\n'));

    std::fs::remove_dir_all(&dir).ok();
}
