//! Managed JDBC helper, protocol v1: wire types and a client that talks to the helper through a `Runner`.
//!
//! Pure and host-free. A `Runner` launches `java Helper.java` (the plugin side, a later increment) and
//! returns its raw output; tests use a fake. Nothing here prints, logs or stores request or response text,
//! and every message surfaced to a caller is scrubbed of the password.
//! The wire contract is documented in `plugins/jpa-assist/parity/JDBC_HELPER.md` sections 2.3 and 10.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt;

pub const HELPER_PROTOCOL: u32 = 1;
pub const MAX_DESCRIBE_TABLES: usize = 25;
pub const MIN_JAVA_MAJOR: u32 = 17;
pub const DEFAULT_DEADLINE_MS: u64 = 50_000;
pub const DEFAULT_MAX_BYTES: u64 = 3_000_000;
/// The host kills a launch at its own budget; the runner gets the helper deadline plus this margin.
const RUNNER_MARGIN_MS: u64 = 3_000;
const MAX_PAGES: usize = 200;
const STDERR_TAIL: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    // Detected by the plugin around the launch.
    JvmMissing,
    JdkRequired,
    JvmTooOld,
    HelperProtocol,
    Cancelled,
    HostBudget,
    // Reported by the helper.
    ProtocolVersion,
    BadRequest,
    DriverNotFound,
    DriverLoadFailed,
    UrlUnsupported,
    UrlRejected,
    ConnectTimeout,
    NetworkUnreachable,
    TlsFailed,
    AuthFailed,
    PermissionDenied,
    SchemaNotFound,
    ResultTooLarge,
    DeadlineExceeded,
    SqlError,
    Internal,
    /// A code this plugin version does not know (a newer helper).
    #[serde(other)]
    Unrecognized,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperError {
    pub code: ErrorCode,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub sql_state: Option<String>,
    #[serde(default)]
    pub vendor_code: i64,
    #[serde(default)]
    pub retryable: bool,
}

impl HelperError {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), sql_state: None, vendor_code: 0, retryable: false }
    }
}

impl fmt::Display for HelperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DriverSpec {
    pub jars: Vec<String>,
    pub class: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Connection {
    pub url: String,
    pub user: Option<String>,
    pub driver: DriverSpec,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub props: BTreeMap<String, String>,
}

/// What a launch produced. The plugin-side runner fills this from `run_process`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchError {
    /// `process:java` denied, or the program could not be started.
    NotStarted(String),
    Cancelled,
    /// The host killed the call at its time budget before a response arrived.
    Timeout,
}

pub trait Runner {
    /// Runs the helper once: `stdin` is the single request line, `timeout_ms` the host-side budget.
    fn run(&mut self, stdin: &str, timeout_ms: u64) -> Result<RunOutput, LaunchError>;
}

// ---- typed results ----

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaInfo {
    pub version: String,
    #[serde(default)]
    pub vendor: String,
    pub jdk: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingInfo {
    pub helper_version: String,
    #[serde(default)]
    pub protocol: Option<u32>,
    pub java: JavaInfo,
    #[serde(default)]
    pub os: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub product_name: String,
    #[serde(default)]
    pub product_version: String,
    #[serde(default)]
    pub driver_name: String,
    #[serde(default)]
    pub driver_version: String,
    #[serde(default)]
    pub catalog: Option<String>,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub read_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaEntry {
    #[serde(default)]
    pub catalog: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub system: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct CurrentScope {
    #[serde(default)]
    pub catalog: Option<String>,
    #[serde(default)]
    pub schema: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct SchemaList {
    #[serde(default)]
    pub catalogs: Vec<Option<String>>,
    #[serde(default)]
    pub schemas: Vec<SchemaEntry>,
    pub current: CurrentScope,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct TableEntry {
    #[serde(default)]
    pub catalog: Option<String>,
    #[serde(default)]
    pub schema: Option<String>,
    pub name: String,
    /// Normalized by the helper: `TABLE`, `VIEW` or `MATERIALIZED VIEW`.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub remarks: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawColumn {
    pub name: String,
    pub type_name: String,
    #[serde(default)]
    pub data_type: i32,
    #[serde(default)]
    pub size: Option<i64>,
    #[serde(default)]
    pub scale: Option<i64>,
    /// `DatabaseMetaData.columnNoNulls` 0, `columnNullable` 1, `columnNullableUnknown` 2.
    #[serde(default)]
    pub nullable: Option<i32>,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub auto_increment: Option<String>,
    #[serde(default)]
    pub generated: Option<String>,
    #[serde(default)]
    pub remarks: Option<String>,
    #[serde(default)]
    pub position: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct RawPrimaryKey {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub columns: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawForeignKey {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub ref_catalog: Option<String>,
    #[serde(default)]
    pub ref_schema: Option<String>,
    pub ref_table: String,
    pub columns: Vec<String>,
    pub ref_columns: Vec<String>,
    #[serde(default)]
    pub update_rule: Option<i32>,
    #[serde(default)]
    pub delete_rule: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct RawIndexColumn {
    /// `None` for an expression key part on drivers that report no text for it.
    #[serde(default)]
    pub name: Option<String>,
    /// `A` ascending, `D` descending, absent when unsupported.
    #[serde(default)]
    pub order: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct RawIndex {
    pub name: String,
    pub unique: bool,
    pub columns: Vec<RawIndexColumn>,
    #[serde(default)]
    pub filter: Option<String>,
}

fn found_by_default() -> bool {
    true
}

/// One `describeTables` entry. A facet is `None` when its metadata call failed; the reason is in `facet_errors`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableMeta {
    pub name: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default = "found_by_default")]
    pub found: bool,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub remarks: Option<String>,
    #[serde(default)]
    pub columns: Option<Vec<RawColumn>>,
    #[serde(default)]
    pub primary_key: Option<RawPrimaryKey>,
    #[serde(default)]
    pub foreign_keys: Option<Vec<RawForeignKey>>,
    #[serde(default)]
    pub indexes: Option<Vec<RawIndex>>,
    #[serde(default)]
    pub facet_errors: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct TablesResult<T> {
    tables: Vec<T>,
}

/// A successful reply with its paging state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply<T> {
    pub value: T,
    pub truncated: bool,
    pub next: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    v: u32,
    #[serde(default)]
    id: Option<String>,
    ok: bool,
    #[serde(default)]
    truncated: bool,
    #[serde(default)]
    next: Option<u64>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<HelperError>,
}

pub struct Client<R: Runner> {
    runner: R,
    connection: Connection,
    password: Option<String>,
    pub deadline_ms: u64,
    pub max_bytes: u64,
    sequence: u64,
}

// The password must never reach a debug dump.
impl<R: Runner> fmt::Debug for Client<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("url", &self.connection.url)
            .field("password", &self.password.as_ref().map(|_| "***"))
            .finish_non_exhaustive()
    }
}

impl<R: Runner> Client<R> {
    pub fn new(runner: R, connection: Connection, password: Option<String>) -> Self {
        Self { runner, connection, password, deadline_ms: DEFAULT_DEADLINE_MS, max_bytes: DEFAULT_MAX_BYTES, sequence: 0 }
    }

    pub fn into_runner(self) -> R {
        self.runner
    }

    /// JVM and helper probe: maps an old or compiler-less Java to `JvmTooOld` / `JdkRequired`.
    pub fn ping(&mut self) -> Result<PingInfo, HelperError> {
        let reply: Reply<PingInfo> = self.call("ping", false, json!({}))?;
        let info = reply.value;
        if !info.java.jdk {
            return Err(HelperError::new(ErrorCode::JdkRequired, "the Java runtime has no compiler; install a JDK 17 or newer"));
        }
        match java_major(&info.java.version) {
            Some(major) if major >= MIN_JAVA_MAJOR => Ok(info),
            _ => Err(HelperError::new(ErrorCode::JvmTooOld, format!("Java {} found; 17 or newer is required", info.java.version))),
        }
    }

    pub fn test_connection(&mut self) -> Result<ConnectionInfo, HelperError> {
        Ok(self.call::<ConnectionInfo>("testConnection", true, json!({}))?.value)
    }

    pub fn list_schemas(&mut self) -> Result<SchemaList, HelperError> {
        Ok(self.call::<SchemaList>("listSchemas", true, json!({}))?.value)
    }

    pub fn list_tables_page(&mut self, schema: Option<&str>, pattern: Option<&str>, offset: u64, limit: u64) -> Result<Reply<Vec<TableEntry>>, HelperError> {
        let args = json!({"schema": schema, "pattern": pattern, "offset": offset, "limit": limit});
        let reply: Reply<TablesResult<TableEntry>> = self.call("listTables", true, args)?;
        Ok(Reply { value: reply.value.tables, truncated: reply.truncated, next: reply.next })
    }

    /// Follows `next` until the listing is complete.
    pub fn list_all_tables(&mut self, schema: Option<&str>, pattern: Option<&str>) -> Result<Vec<TableEntry>, HelperError> {
        let mut all = Vec::new();
        let mut offset = 0;
        for _ in 0..MAX_PAGES {
            let page = self.list_tables_page(schema, pattern, offset, 200)?;
            all.extend(page.value);
            if !page.truncated {
                return Ok(all);
            }
            offset = advance(offset, page.next)?;
        }
        Err(HelperError::new(ErrorCode::HelperProtocol, "table listing did not finish within the page limit"))
    }

    /// One `describeTables` call for at most 25 names; `offset` resumes inside `names` after a truncation.
    pub fn describe_tables_page(&mut self, schema: Option<&str>, names: &[String], offset: u64) -> Result<Reply<Vec<TableMeta>>, HelperError> {
        if names.is_empty() || names.len() > MAX_DESCRIBE_TABLES {
            return Err(HelperError::new(ErrorCode::BadRequest, format!("describeTables takes 1..={MAX_DESCRIBE_TABLES} names")));
        }
        let args = json!({"schema": schema, "tables": names, "offset": offset});
        let reply: Reply<TablesResult<TableMeta>> = self.call("describeTables", true, args)?;
        Ok(Reply { value: reply.value.tables, truncated: reply.truncated, next: reply.next })
    }

    /// Describes any number of tables: chunks of 25, following `next` inside a chunk.
    pub fn describe_all(&mut self, schema: Option<&str>, names: &[String]) -> Result<Vec<TableMeta>, HelperError> {
        let mut all = Vec::new();
        for chunk in names.chunks(MAX_DESCRIBE_TABLES) {
            let mut offset = 0;
            for _ in 0..MAX_PAGES {
                let page = self.describe_tables_page(schema, chunk, offset)?;
                all.extend(page.value);
                if !page.truncated {
                    break;
                }
                offset = advance(offset, page.next)?;
            }
        }
        Ok(all)
    }

    fn call<T: for<'de> Deserialize<'de>>(&mut self, op: &str, with_connection: bool, args: Value) -> Result<Reply<T>, HelperError> {
        self.sequence += 1;
        let id = format!("n{}", self.sequence);
        let mut request = json!({
            "v": HELPER_PROTOCOL,
            "id": id,
            "op": op,
            "deadlineMs": self.deadline_ms,
            "maxBytes": self.max_bytes,
            "args": args,
        });
        if with_connection {
            request["conn"] = serde_json::to_value(&self.connection).map_err(|e| HelperError::new(ErrorCode::BadRequest, e.to_string()))?;
            request["secrets"] = json!({"password": self.password.clone().unwrap_or_default()});
        }
        let line = request.to_string();

        let output = match self.runner.run(&line, self.deadline_ms + RUNNER_MARGIN_MS) {
            Ok(output) => output,
            Err(LaunchError::Cancelled) => return Err(HelperError::new(ErrorCode::Cancelled, "cancelled")),
            Err(LaunchError::Timeout) => return Err(HelperError::new(ErrorCode::HostBudget, "the helper did not answer within the host time budget")),
            Err(LaunchError::NotStarted(why)) => return Err(self.scrubbed(HelperError::new(ErrorCode::JvmMissing, format!("could not start java: {why}")))),
        };
        self.parse(&id, output)
    }

    fn parse<T: for<'de> Deserialize<'de>>(&self, id: &str, output: RunOutput) -> Result<Reply<T>, HelperError> {
        let stderr_tail = self.scrub(&tail(&output.stderr, STDERR_TAIL));
        let protocol = |why: &str| {
            let mut message = format!("{why} (exit {})", output.code);
            if !stderr_tail.trim().is_empty() {
                message.push_str(&format!("; stderr: {}", stderr_tail.trim()));
            }
            HelperError::new(ErrorCode::HelperProtocol, message)
        };

        let silent = output.stdout.trim().is_empty();
        if silent && (output.code == 127 || output.stderr.contains("Unable to locate a Java Runtime")) {
            return Err(HelperError::new(ErrorCode::JvmMissing, "java was not found; install a JDK 17 or newer and put it on PATH"));
        }
        if silent && output.stderr.contains("jdk.compiler") {
            return Err(HelperError::new(ErrorCode::JdkRequired, "the Java runtime has no compiler module; install a JDK 17 or newer"));
        }
        if silent {
            return Err(protocol("the helper wrote no response"));
        }
        let body = match output.stdout.strip_suffix('\n') {
            Some(body) => body,
            None => return Err(protocol("the helper response has no trailing newline")),
        };
        if body.contains('\n') {
            return Err(protocol("the helper wrote more than one line"));
        }
        let envelope: Envelope = match serde_json::from_str(body) {
            Ok(envelope) => envelope,
            Err(_) => return Err(protocol("the helper response is not valid protocol JSON")),
        };
        if envelope.v != HELPER_PROTOCOL {
            return Err(HelperError::new(ErrorCode::ProtocolVersion, format!("helper speaks protocol v{}, expected v{HELPER_PROTOCOL}", envelope.v)));
        }
        if envelope.id.as_deref() != Some(id) {
            return Err(protocol("the helper answered a different request"));
        }
        if !envelope.ok {
            return Err(match envelope.error {
                Some(error) => self.scrubbed(error),
                None => protocol("the helper reported failure without an error"),
            });
        }
        let Some(result) = envelope.result else {
            return Err(protocol("the helper reported success without a result"));
        };
        match serde_json::from_value(result) {
            Ok(value) => Ok(Reply { value, truncated: envelope.truncated, next: envelope.next }),
            Err(_) => Err(protocol("the helper result has an unexpected shape")),
        }
    }

    fn scrubbed(&self, mut error: HelperError) -> HelperError {
        error.message = self.scrub(&error.message);
        error
    }

    fn scrub(&self, text: &str) -> String {
        scrub_secrets(text, self.password.as_deref())
    }
}

fn advance(offset: u64, next: Option<u64>) -> Result<u64, HelperError> {
    match next {
        Some(next) if next > offset => Ok(next),
        _ => Err(HelperError::new(ErrorCode::HelperProtocol, "a truncated reply did not advance the cursor")),
    }
}

fn tail(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_owned()
}

/// Major version of a `java.version` string: `17.0.2` is 17, `1.8.0_292` is 8, `21-ea` is 21.
pub fn java_major(version: &str) -> Option<u32> {
    let mut parts = version.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty());
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        return parts.next()?.parse().ok();
    }
    Some(first)
}

/// Removes the password (when 4 or more characters, as shorter ones would mangle words), `password=`/`pwd=`
/// assignments and URL userinfo passwords.
pub fn scrub_secrets(text: &str, password: Option<&str>) -> String {
    let mut out = text.to_owned();
    if let Some(password) = password.filter(|p| p.len() >= 4) {
        out = out.replace(password, "***");
    }
    for key in ["password=", "pwd="] {
        out = mask_values(&out, key);
    }
    mask_userinfo(&out)
}

fn mask_values(text: &str, key: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some(found) = lower[cursor..].find(key) {
        let value_start = cursor + found + key.len();
        out.push_str(&text[cursor..value_start]);
        let value_len = text[value_start..].find(|c: char| matches!(c, '&' | ';' | '"' | '\'') || c.is_whitespace()).unwrap_or(text.len() - value_start);
        if value_len > 0 {
            out.push_str("***");
        }
        cursor = value_start + value_len;
    }
    out.push_str(&text[cursor..]);
    out
}

fn mask_userinfo(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(slashes) = rest.find("//") {
        let after = slashes + 2;
        out.push_str(&rest[..after]);
        rest = &rest[after..];
        let end = rest.find(|c: char| matches!(c, '/' | '?' | '"' | '\'') || c.is_whitespace()).unwrap_or(rest.len());
        let authority = &rest[..end];
        match (authority.rfind('@'), authority.find(':')) {
            (Some(at), Some(colon)) if colon < at => {
                out.push_str(&authority[..=colon]);
                out.push_str("***");
                out.push_str(&authority[at..]);
            }
            _ => out.push_str(authority),
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct Fake {
        queue: VecDeque<Result<RunOutput, LaunchError>>,
        requests: Vec<Value>,
        timeouts: Vec<u64>,
    }

    impl Runner for Fake {
        fn run(&mut self, stdin: &str, timeout_ms: u64) -> Result<RunOutput, LaunchError> {
            assert!(!stdin.contains('\n'), "request must be a single line");
            self.requests.push(serde_json::from_str(stdin).unwrap());
            self.timeouts.push(timeout_ms);
            self.queue.pop_front().expect("unexpected extra launch")
        }
    }

    const PASSWORD: &str = "S3NT1NEL-pw";

    fn client(replies: Vec<Result<RunOutput, LaunchError>>) -> Client<Fake> {
        let connection = Connection {
            url: "jdbc:postgresql://db.example:5432/app".into(),
            user: Some("app".into()),
            driver: DriverSpec { jars: vec!["/m2/postgresql.jar".into()], class: "org.postgresql.Driver".into() },
            props: BTreeMap::new(),
        };
        Client::new(Fake { queue: replies.into(), requests: vec![], timeouts: vec![] }, connection, Some(PASSWORD.into()))
    }

    fn out(stdout: &str) -> Result<RunOutput, LaunchError> {
        Ok(RunOutput { code: 0, stdout: stdout.to_owned(), stderr: String::new() })
    }

    fn ok(id: u32, result: Value) -> Result<RunOutput, LaunchError> {
        out(&format!("{}\n", json!({"v":1,"id":format!("n{id}"),"ok":true,"truncated":false,"next":null,"result":result,"timingMs":{"total":5}})))
    }

    fn page(id: u32, result: Value, next: Option<u64>) -> Result<RunOutput, LaunchError> {
        out(&format!("{}\n", json!({"v":1,"id":format!("n{id}"),"ok":true,"truncated":next.is_some(),"next":next,"result":result})))
    }

    fn fail(id: u32, code: &str, message: &str) -> Result<RunOutput, LaunchError> {
        out(&format!("{}\n", json!({"v":1,"id":format!("n{id}"),"ok":false,"error":{"code":code,"message":message,"sqlState":null,"vendorCode":0,"retryable":false}})))
    }

    fn ping_json(version: &str, jdk: bool) -> Value {
        json!({"helperVersion":"1.0.0","protocol":1,"java":{"version":version,"vendor":"Test","jdk":jdk},"os":"Linux"})
    }

    #[test]
    fn ping_accepts_a_modern_jdk() {
        let mut c = client(vec![ok(1, ping_json("21.0.4", true))]);
        let info = c.ping().unwrap();
        assert_eq!(info.java.version, "21.0.4");
        let request = &c.into_runner().requests[0];
        assert_eq!(request["op"], "ping");
        assert!(request.get("conn").is_none() && request.get("secrets").is_none());
    }

    #[test]
    fn ping_rejects_old_java_and_runtimes_without_a_compiler() {
        assert_eq!(client(vec![ok(1, ping_json("11.0.2", true))]).ping().unwrap_err().code, ErrorCode::JvmTooOld);
        assert_eq!(client(vec![ok(1, ping_json("1.8.0_292", true))]).ping().unwrap_err().code, ErrorCode::JvmTooOld);
        assert_eq!(client(vec![ok(1, ping_json("21", false))]).ping().unwrap_err().code, ErrorCode::JdkRequired);
        assert_eq!(java_major("17.0.2"), Some(17));
        assert_eq!(java_major("21-ea"), Some(21));
        assert_eq!(java_major("1.8.0_292"), Some(8));
        assert_eq!(java_major("x"), None);
    }

    #[test]
    fn password_travels_only_in_secrets_of_the_request_line() {
        let mut c = client(vec![ok(1, json!({"productName":"PostgreSQL","readOnly":true}))]);
        c.test_connection().unwrap();
        let debug = format!("{c:?}");
        assert!(!debug.contains(PASSWORD));
        let request = c.into_runner().requests.remove(0);
        assert_eq!(request["secrets"]["password"], PASSWORD);
        let mut without = request.clone();
        without.as_object_mut().unwrap().remove("secrets");
        assert!(!without.to_string().contains(PASSWORD));
        assert_eq!(request["conn"]["driver"]["class"], "org.postgresql.Driver");
        assert_eq!(request["deadlineMs"], DEFAULT_DEADLINE_MS);
    }

    #[test]
    fn runner_budget_exceeds_the_helper_deadline() {
        let mut c = client(vec![ok(1, json!({"productName":"X"}))]);
        c.deadline_ms = 20_000;
        c.test_connection().unwrap();
        assert_eq!(c.into_runner().timeouts, vec![23_000]);
    }

    #[test]
    fn every_helper_error_code_round_trips() {
        let codes = [
            ("ProtocolVersion", ErrorCode::ProtocolVersion),
            ("BadRequest", ErrorCode::BadRequest),
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
            ("SqlError", ErrorCode::SqlError),
            ("Internal", ErrorCode::Internal),
            ("SomethingNewer", ErrorCode::Unrecognized),
        ];
        for (name, expected) in codes {
            let err = client(vec![fail(1, name, "boom")]).test_connection().unwrap_err();
            assert_eq!(err.code, expected, "{name}");
            assert_eq!(err.message, "boom");
        }
    }

    #[test]
    fn helper_protocol_version_mismatch_is_reported() {
        let reply = out(&format!("{}\n", json!({"v":2,"id":"n1","ok":true,"result":{}})));
        assert_eq!(client(vec![reply]).test_connection().unwrap_err().code, ErrorCode::ProtocolVersion);
        assert_eq!(client(vec![fail(1, "ProtocolVersion", "helper speaks v1")]).test_connection().unwrap_err().code, ErrorCode::ProtocolVersion);
    }

    #[test]
    fn launch_failures_map_to_plugin_side_codes() {
        let denied = Err(LaunchError::NotStarted("process:java denied".into()));
        assert_eq!(client(vec![denied]).ping().unwrap_err().code, ErrorCode::JvmMissing);
        assert_eq!(client(vec![Err(LaunchError::Cancelled)]).ping().unwrap_err().code, ErrorCode::Cancelled);
        assert_eq!(client(vec![Err(LaunchError::Timeout)]).ping().unwrap_err().code, ErrorCode::HostBudget);

        let missing = |code, stderr: &str| Ok(RunOutput { code, stdout: String::new(), stderr: stderr.to_owned() });
        assert_eq!(client(vec![missing(127, "sh: java: not found")]).ping().unwrap_err().code, ErrorCode::JvmMissing);
        assert_eq!(client(vec![missing(1, "The operation couldn't be completed. Unable to locate a Java Runtime.")]).ping().unwrap_err().code, ErrorCode::JvmMissing);
        assert_eq!(client(vec![missing(1, "error: Module jdk.compiler not in boot Layer")]).ping().unwrap_err().code, ErrorCode::JdkRequired);
        assert_eq!(client(vec![missing(1, "Exception in thread main")]).ping().unwrap_err().code, ErrorCode::HelperProtocol);
    }

    #[test]
    fn malformed_output_is_a_protocol_error_never_success() {
        let valid = json!({"v":1,"id":"n1","ok":true,"result":{"productName":"X"}}).to_string();
        let cases = vec![
            out("   \n"),
            out("not json\n"),
            out(&valid),
            out(&format!("{valid}\n{valid}\n")),
            out(&format!("{}\n", json!({"v":1,"id":"other","ok":true,"result":{"productName":"X"}}))),
            out(&format!("{}\n", json!({"v":1,"id":"n1","ok":true}))),
            out(&format!("{}\n", json!({"v":1,"id":"n1","ok":false}))),
            out(&format!("{}\n", json!({"v":1,"id":"n1","ok":true,"result":{"unexpected":true}}))),
        ];
        for (i, reply) in cases.into_iter().enumerate() {
            let err = client(vec![reply]).test_connection().unwrap_err();
            assert_eq!(err.code, ErrorCode::HelperProtocol, "case {i}");
        }
    }

    #[test]
    fn nonzero_exit_with_a_valid_response_is_still_accepted() {
        let mut reply = ok(1, json!({"productName":"X"}));
        reply.as_mut().unwrap().code = 1;
        assert!(client(vec![reply]).test_connection().is_ok());
    }

    #[test]
    fn password_is_scrubbed_from_every_surfaced_message() {
        let message = format!("FATAL: password authentication failed for {PASSWORD}; url jdbc:postgresql://u:{PASSWORD}@h/db?password=abc&x=1");
        let err = client(vec![fail(1, "AuthFailed", &message)]).test_connection().unwrap_err();
        assert!(!err.message.contains(PASSWORD) && !err.message.contains("abc"), "{}", err.message);
        assert!(err.message.contains("password=***&x=1"));

        let leaky = Ok(RunOutput { code: 1, stdout: String::new(), stderr: format!("boom {PASSWORD} PWD=hunter2 end") });
        let err = client(vec![leaky]).test_connection().unwrap_err();
        assert_eq!(err.code, ErrorCode::HelperProtocol);
        assert!(!err.message.contains(PASSWORD) && !err.message.contains("hunter2"), "{}", err.message);

        let denied = Err(LaunchError::NotStarted(format!("spawn failed near {PASSWORD}")));
        assert!(!client(vec![denied]).ping().unwrap_err().message.contains(PASSWORD));
    }

    #[test]
    fn scrub_keeps_short_passwords_literal_but_masks_assignments() {
        assert_eq!(scrub_secrets("Exception p", Some("p")), "Exception p");
        assert_eq!(scrub_secrets("a PASSWORD=p&b=1", Some("p")), "a PASSWORD=***&b=1");
        assert_eq!(scrub_secrets("jdbc:mysql://root:hunter2@host:3306/db", None), "jdbc:mysql://root:***@host:3306/db");
        assert_eq!(scrub_secrets("see https://example.org/a//b done", None), "see https://example.org/a//b done");
        assert_eq!(scrub_secrets("ünï password=ü x", None), "ünï password=*** x");
    }

    #[test]
    fn stderr_tail_is_bounded() {
        let stderr = format!("{}{}", "x".repeat(10_000), "END");
        let reply = Ok(RunOutput { code: 1, stdout: String::new(), stderr });
        let err = client(vec![reply]).ping().unwrap_err();
        assert!(err.message.len() < 2300 && err.message.ends_with("END"));
    }

    fn tables(names: &[&str]) -> Value {
        json!({"tables": names.iter().map(|n| json!({"catalog":null,"schema":"public","name":n,"type":"TABLE","remarks":null})).collect::<Vec<_>>()})
    }

    #[test]
    fn list_all_tables_follows_truncated_pages() {
        let mut c = client(vec![page(1, tables(&["a", "b"]), Some(2)), page(2, tables(&["c", "d"]), Some(4)), page(3, tables(&["e"]), None)]);
        let all = c.list_all_tables(Some("public"), None).unwrap();
        assert_eq!(all.iter().map(|t| t.name.as_str()).collect::<String>(), "abcde");
        let requests = c.into_runner().requests;
        assert_eq!(requests.iter().map(|r| r["args"]["offset"].as_u64().unwrap()).collect::<Vec<_>>(), vec![0, 2, 4]);
        assert_eq!(requests[0]["args"]["schema"], "public");
    }

    #[test]
    fn a_cursor_that_does_not_advance_stops_the_loop() {
        let mut c = client(vec![page(1, tables(&["a"]), Some(0))]);
        assert_eq!(c.list_all_tables(None, None).unwrap_err().code, ErrorCode::HelperProtocol);
        let mut c = client(vec![page(1, tables(&["a"]), None).map(|mut o| {
            o.stdout = o.stdout.replace("\"truncated\":false", "\"truncated\":true");
            o
        })]);
        assert_eq!(c.list_all_tables(None, None).unwrap_err().code, ErrorCode::HelperProtocol);
    }

    fn described(names: &[String]) -> Value {
        json!({"tables": names.iter().map(|n| json!({"name":n,"schema":"public","found":true,"type":"TABLE","columns":[],"primaryKey":{"name":null,"columns":[]},"foreignKeys":[],"indexes":[]})).collect::<Vec<_>>()})
    }

    #[test]
    fn describe_all_chunks_by_25_and_resumes_after_truncation() {
        let names: Vec<String> = (0..60).map(|i| format!("t{i:02}")).collect();
        let first_chunk = &names[..25];
        let mut c = client(vec![
            page(1, described(&first_chunk[..10]), Some(10)),
            page(2, described(&first_chunk[10..]), None),
            ok(3, described(&names[25..50])),
            ok(4, described(&names[50..])),
        ]);
        let all = c.describe_all(None, &names).unwrap();
        assert_eq!(all.len(), 60);
        let requests = c.into_runner().requests;
        let sizes: Vec<usize> = requests.iter().map(|r| r["args"]["tables"].as_array().unwrap().len()).collect();
        assert_eq!(sizes, vec![25, 25, 25, 10]);
        assert_eq!(requests[1]["args"]["offset"], 10);
    }

    #[test]
    fn describe_page_refuses_more_than_25_names_without_launching() {
        let names: Vec<String> = (0..26).map(|i| i.to_string()).collect();
        let mut c = client(vec![]);
        assert_eq!(c.describe_tables_page(None, &names, 0).unwrap_err().code, ErrorCode::BadRequest);
        assert_eq!(c.describe_tables_page(None, &[], 0).unwrap_err().code, ErrorCode::BadRequest);
    }

    #[test]
    fn result_too_large_surfaces_as_an_error_not_a_partial_result() {
        let err = client(vec![fail(1, "ResultTooLarge", "a single table description exceeds maxBytes")]).describe_tables_page(None, &["t".into()], 0).unwrap_err();
        assert_eq!(err.code, ErrorCode::ResultTooLarge);
    }

    #[test]
    fn table_meta_tolerates_missing_optional_fields() {
        let meta: TableMeta = serde_json::from_value(json!({"name":"t","columns":null,"primaryKey":null,"foreignKeys":null,"indexes":null,"facetErrors":{"columns":"permission denied"}})).unwrap();
        assert!(meta.found && meta.columns.is_none());
        assert_eq!(meta.facet_errors["columns"], "permission denied");
    }

    fn recorded(name: &str) -> Result<RunOutput, LaunchError> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/jdbc").join(name);
        out(&std::fs::read_to_string(path).unwrap())
    }

    #[test]
    fn responses_recorded_from_the_real_helper_against_h2_parse() {
        let mut c = client(vec![recorded("h2_real.list_tables.response.json")]);
        let tables = c.list_all_tables(Some("SHOP"), None).unwrap();
        assert_eq!(tables.iter().map(|t| (t.name.as_str(), t.kind.as_str())).collect::<Vec<_>>(), vec![("CUSTOMER", "TABLE"), ("LINE", "TABLE"), ("ORDERS", "TABLE"), ("V_CUST", "VIEW")]);

        let names: Vec<String> = ["CUSTOMER", "ORDERS", "LINE", "NO_SUCH", "V_CUST"].iter().map(|n| n.to_string()).collect();
        let mut c = client(vec![recorded("h2_real.describe_tables.response.json")]);
        let described = c.describe_all(Some("SHOP"), &names).unwrap();
        assert_eq!(described.len(), 5);
        assert_eq!(described[1].foreign_keys.as_ref().unwrap()[0].delete_rule, Some(0));
        assert_eq!(described[2].primary_key.as_ref().unwrap().columns, vec!["ORDER_ID", "N"]);
        assert!(!described[3].found);
        assert_eq!(described[4].kind.as_deref(), Some("VIEW"));
    }
}
