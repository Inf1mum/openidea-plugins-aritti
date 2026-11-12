//! Connection profiles and user-facing messages for the managed JDBC helper (P3). Pure and host-free.
//!
//! A profile never holds a password: `deny_unknown_fields` rejects a stored `password`, a URL carrying userinfo
//! or a password parameter does not validate, and the password is asked for (or read from the project's literal
//! Spring configuration) at connect time only. See `plugins/jpa-assist/parity/JDBC_HELPER.md` sections 4 and 9.
use crate::ddl_parse::Dialect;
use crate::introspect::{ConnectionInfo, ErrorCode, HelperError, TableMeta};
use crate::introspect_map::Mapped;
use crate::schema::{Fact, MAX_TABLES};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

pub const MAX_PROFILES: usize = 50;
const MAX_URL: usize = 2048;
const MAX_PATH: usize = 1024;

/// Database families. Only the first four are enabled (2026-11-12 decision 3); the others stay disabled until
/// their URL rules are verified, so a profile or a Spring data source for them is refused with a clear reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Dbms {
    #[serde(rename = "h2")]
    H2,
    #[serde(rename = "postgresql")]
    PostgreSql,
    #[serde(rename = "mysql")]
    MySql,
    #[serde(rename = "mariadb")]
    MariaDb,
    #[serde(rename = "sqlserver")]
    SqlServer,
    #[serde(rename = "oracle")]
    Oracle,
    #[serde(rename = "db2")]
    Db2,
}

impl Dbms {
    pub const ENABLED: [Dbms; 4] = [Dbms::PostgreSql, Dbms::MySql, Dbms::MariaDb, Dbms::H2];

    pub fn is_enabled(self) -> bool {
        Self::ENABLED.contains(&self)
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::H2 => "h2",
            Self::PostgreSql => "postgresql",
            Self::MySql => "mysql",
            Self::MariaDb => "mariadb",
            Self::SqlServer => "sqlserver",
            Self::Oracle => "oracle",
            Self::Db2 => "db2",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::H2 => "H2",
            Self::PostgreSql => "PostgreSQL",
            Self::MySql => "MySQL",
            Self::MariaDb => "MariaDB",
            Self::SqlServer => "SQL Server",
            Self::Oracle => "Oracle",
            Self::Db2 => "Db2",
        }
    }

    pub fn url_prefix(self) -> &'static str {
        match self {
            Self::H2 => "jdbc:h2:",
            Self::PostgreSql => "jdbc:postgresql:",
            Self::MySql => "jdbc:mysql:",
            Self::MariaDb => "jdbc:mariadb:",
            Self::SqlServer => "jdbc:sqlserver:",
            Self::Oracle => "jdbc:oracle:thin:",
            Self::Db2 => "jdbc:db2:",
        }
    }

    /// The driver class the helper allowlists for this family.
    pub fn driver_class(self) -> &'static str {
        match self {
            Self::H2 => "org.h2.Driver",
            Self::PostgreSql => "org.postgresql.Driver",
            Self::MySql => "com.mysql.cj.jdbc.Driver",
            Self::MariaDb => "org.mariadb.jdbc.Driver",
            Self::SqlServer => "com.microsoft.sqlserver.jdbc.SQLServerDriver",
            Self::Oracle => "oracle.jdbc.OracleDriver",
            Self::Db2 => "com.ibm.db2.jcc.DB2Driver",
        }
    }

    pub fn default_port(self) -> Option<u16> {
        match self {
            Self::PostgreSql => Some(5432),
            Self::MySql | Self::MariaDb => Some(3306),
            Self::SqlServer => Some(1433),
            Self::Oracle => Some(1521),
            Self::Db2 => Some(50000),
            Self::H2 => None,
        }
    }

    /// The row set `introspect_map` maps types with. H2 has none yet (its type names are not mapped).
    pub fn mapping_dialect(self) -> Option<Dialect> {
        match self {
            Self::PostgreSql => Some(Dialect::PostgreSql),
            Self::MySql | Self::MariaDb => Some(Dialect::MySql),
            _ => None,
        }
    }

    pub fn from_url(url: &str) -> Option<Dbms> {
        let lower = url.trim().to_ascii_lowercase();
        [Self::H2, Self::PostgreSql, Self::MySql, Self::MariaDb, Self::SqlServer, Self::Oracle, Self::Db2]
            .into_iter()
            .find(|d| lower.starts_with(d.url_prefix()))
    }

    pub fn from_key(key: &str) -> Option<Dbms> {
        [Self::H2, Self::PostgreSql, Self::MySql, Self::MariaDb, Self::SqlServer, Self::Oracle, Self::Db2].into_iter().find(|d| d.key() == key)
    }

    pub fn disabled_reason(self) -> String {
        format!("{} is not enabled yet: its connection URL rules are still being verified.", self.label())
    }
}

/// Which schemas and tables a profile introspects. Empty means "ask".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SchemaFilter {
    pub schemas: Vec<String>,
    /// A `LIKE` pattern on table names (`%` matches any run of characters).
    pub table_pattern: Option<String>,
    /// Emit `schema.table` names. Off: bare names, and same-named tables in two schemas are an error.
    pub qualify_schema: bool,
}

impl SchemaFilter {
    pub fn is_empty(&self) -> bool {
        self.schemas.is_empty() && self.table_pattern.as_deref().is_none_or(str::is_empty)
    }
}

/// Where a profile was discovered, so a literal password in that file can be read again at connect time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpringSource {
    pub root: String,
    pub file: String,
    pub prefix: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    pub dbms: Dbms,
    /// The JDBC URL without credentials; it is the template the user confirms before every new connection.
    pub url_template: String,
    pub user: Option<String>,
    /// Absolute path of the driver jar; empty falls back to the per-family driver registry.
    #[serde(default)]
    pub driver_jar: Option<String>,
    #[serde(default)]
    pub schema_filter: SchemaFilter,
    #[serde(default)]
    pub spring_source: Option<SpringSource>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileError(pub String);

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn clean(text: &str, max: usize, what: &str) -> Result<(), ProfileError> {
    if text.chars().any(char::is_control) {
        return Err(ProfileError(format!("{what} must not contain control characters")));
    }
    if text.len() > max {
        return Err(ProfileError(format!("{what} is longer than {max} characters")));
    }
    Ok(())
}

/// An absolute path to a `.jar` (POSIX, drive-letter or UNC).
pub fn valid_jar_path(path: &str) -> Result<(), ProfileError> {
    clean(path, MAX_PATH, "driver jar path")?;
    let bytes = path.as_bytes();
    let absolute = path.starts_with('/') || path.starts_with("\\\\") || (bytes.len() > 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/'));
    if !absolute {
        return Err(ProfileError("the driver jar path must be absolute".into()));
    }
    if !path.ends_with(".jar") {
        return Err(ProfileError("the driver jar path must end in .jar".into()));
    }
    Ok(())
}

/// The helper opens H2 read-only from a file or a TCP server: an in-memory database would be empty in a one-shot
/// process, and `;` settings (INIT scripts and the like) are refused by the helper's URL policy.
fn h2_shape(url: &str) -> Result<(), ProfileError> {
    let rest = &url["jdbc:h2:".len().min(url.len())..];
    if url.contains([';', '?']) {
        return Err(ProfileError("H2 URLs may not carry ;settings: the helper opens H2 read-only and refuses INIT and similar options".into()));
    }
    let lower = rest.to_ascii_lowercase();
    if !(lower.starts_with("file:") || lower.starts_with("tcp://") || rest.starts_with('/') || rest.starts_with('~')) {
        return Err(ProfileError("use jdbc:h2:file:<path> or jdbc:h2:tcp://host/db; an in-memory H2 database is empty in a separate process".into()));
    }
    Ok(())
}

/// Userinfo (`user:pw@host`, `user@host`) or a password-like parameter.
pub fn url_has_credentials(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if ["password=", "pwd=", "passwd="].iter().any(|key| lower.contains(key)) {
        return true;
    }
    match url.find("//") {
        Some(at) => {
            let rest = &url[at + 2..];
            let end = rest.find(['/', '?', ';']).unwrap_or(rest.len());
            rest[..end].contains('@')
        }
        None => false,
    }
}

impl Profile {
    pub fn validate(&self) -> Result<(), ProfileError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(ProfileError("the connection needs a name".into()));
        }
        clean(name, 80, "the name")?;
        if !self.dbms.is_enabled() {
            return Err(ProfileError(self.dbms.disabled_reason()));
        }
        clean(&self.url_template, MAX_URL, "the URL")?;
        if self.url_template.contains("${") {
            return Err(ProfileError("the URL has an unresolved ${placeholder}; enter the literal URL".into()));
        }
        if !self.url_template.to_ascii_lowercase().starts_with(self.dbms.url_prefix()) {
            return Err(ProfileError(format!("the URL must start with {} for {}", self.dbms.url_prefix(), self.dbms.label())));
        }
        if url_has_credentials(&self.url_template) {
            return Err(ProfileError("remove the user name and password from the URL; the user is a separate field and the password is asked when connecting".into()));
        }
        if self.dbms == Dbms::H2 {
            h2_shape(&self.url_template)?;
        }
        if let Some(user) = &self.user {
            clean(user, 128, "the user")?;
        }
        if let Some(jar) = &self.driver_jar {
            valid_jar_path(jar)?;
        }
        let filter = &self.schema_filter;
        if filter.schemas.len() > 50 {
            return Err(ProfileError("at most 50 schemas can be selected".into()));
        }
        for schema in &filter.schemas {
            clean(schema, 128, "a schema name")?;
        }
        if let Some(pattern) = &filter.table_pattern {
            clean(pattern, 200, "the table filter")?;
        }
        Ok(())
    }

    /// What the user confirms: the connection target, never a secret.
    pub fn target(&self) -> String {
        describe_target(&self.url_template)
    }

    /// Identity of a confirmed connection: changing the URL, user or driver asks again.
    pub fn confirmation_key(&self, jar: &str) -> String {
        format!("{}\n{}\n{}", self.url_template, self.user.as_deref().unwrap_or(""), jar)
    }
}

/// `host:port, database name` for network URLs, the path for an H2 file URL.
pub fn describe_target(url: &str) -> String {
    let lower = url.to_ascii_lowercase();
    if let Some(path) = lower.strip_prefix("jdbc:h2:") {
        let at = url.len() - path.len();
        let rest = url[at..].split([';', '?']).next().unwrap_or("");
        return match rest.strip_prefix("tcp://").or_else(|| rest.strip_prefix("ssl://")) {
            Some(remote) => match remote.split_once('/') {
                Some((host, db)) => format!("{host}, database {db}"),
                None => remote.to_owned(),
            },
            None => format!("file {}", rest.strip_prefix("file:").unwrap_or(rest)),
        };
    }
    match url.find("//") {
        Some(at) => {
            let rest = &url[at + 2..];
            let end = rest.find(['/', '?', ';']).unwrap_or(rest.len());
            let host = &rest[..end];
            let database = rest[end..].trim_start_matches('/').split(['?', ';']).next().unwrap_or("");
            if database.is_empty() { host.to_owned() } else { format!("{host}, database {database}") }
        }
        None => url.split(['?', ';']).next().unwrap_or(url).to_owned(),
    }
}

// ---- storage documents ----

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreDoc {
    version: u32,
    profiles: Vec<Profile>,
}

const STORE_VERSION: u32 = 1;

/// A corrupt or newer document is an error, never an empty list that the next save would overwrite.
pub fn parse_profiles(text: Option<&str>) -> Result<Vec<Profile>, String> {
    let Some(text) = text else {
        return Ok(Vec::new());
    };
    let doc: StoreDoc = serde_json::from_str(text).map_err(|e| format!("the stored connection list is unreadable ({e})"))?;
    if doc.version != STORE_VERSION {
        return Err(format!("the stored connection list has version {}, expected {STORE_VERSION}", doc.version));
    }
    for profile in &doc.profiles {
        profile.validate().map_err(|e| format!("stored connection `{}`: {e}", profile.name))?;
    }
    Ok(doc.profiles)
}

pub fn profiles_json(profiles: &[Profile]) -> String {
    serde_json::to_string(&StoreDoc { version: STORE_VERSION, profiles: profiles.to_vec() }).unwrap_or_default()
}

/// Adds or replaces by case-insensitive name.
pub fn upsert(profiles: &mut Vec<Profile>, profile: Profile) -> Result<(), ProfileError> {
    profile.validate()?;
    let name = profile.name.trim().to_lowercase();
    match profiles.iter().position(|p| p.name.trim().to_lowercase() == name) {
        Some(at) => profiles[at] = profile,
        None if profiles.len() >= MAX_PROFILES => return Err(ProfileError(format!("at most {MAX_PROFILES} connections can be saved"))),
        None => profiles.push(profile),
    }
    Ok(())
}

/// Driver jars the user supplied, one per database family (the cache kept in the plugin directory).
pub fn parse_drivers(text: Option<&str>) -> Result<BTreeMap<Dbms, String>, String> {
    let Some(text) = text else {
        return Ok(BTreeMap::new());
    };
    let map: BTreeMap<Dbms, String> = serde_json::from_str(text).map_err(|e| format!("the driver list is unreadable ({e})"))?;
    for jar in map.values() {
        valid_jar_path(jar).map_err(|e| e.to_string())?;
    }
    Ok(map)
}

pub fn drivers_json(drivers: &BTreeMap<Dbms, String>) -> String {
    serde_json::to_string(drivers).unwrap_or_default()
}

// ---- table-count rule ----

/// More tables than a snapshot can hold: the listing is never truncated, a filter is required first.
pub fn needs_filter(table_count: usize) -> bool {
    table_count > MAX_TABLES
}

pub fn filter_required_message(table_count: usize) -> String {
    format!("{table_count} tables match, more than the {MAX_TABLES} a schema snapshot holds. Nothing is cut off: choose a schema or enter a table name filter (use % as a wildcard) to continue.")
}

// ---- messages ----

/// One actionable sentence per error code (`JDBC_HELPER.md` section 2.5). The helper message is already scrubbed.
pub fn error_message(error: &HelperError) -> String {
    let detail = error.message.trim();
    let state = match &error.sql_state {
        Some(state) if !state.is_empty() => format!(" (SQLState {state})"),
        _ => String::new(),
    };
    match error.code {
        ErrorCode::JvmMissing => "Java was not found. Install a JDK 17 or newer and put `java` on the PATH of your login shell, then retry.".into(),
        ErrorCode::JdkRequired => "The Java runtime has no compiler (a JRE was found). Install a full JDK 17 or newer.".into(),
        ErrorCode::JvmTooOld => format!("Java is too old: {detail}. JDK 17 or newer is required."),
        ErrorCode::HelperProtocol => format!("The database helper did not answer as expected: {detail}. Check that `java` is a JDK 17 or newer."),
        ErrorCode::Cancelled => "Cancelled. Nothing was changed.".into(),
        ErrorCode::HostBudget => "The database did not answer within the time the IDE allows one command (about a minute). Check the host and network, or narrow the schema and table filter.".into(),
        ErrorCode::ProcessDenied => "JPA Assist may not run `java`. Allow the optional `process:java` permission in Settings > Plugins > JPA Assist; live database access stays off without it.".into(),
        ErrorCode::ProtocolVersion => "The database helper and the plugin disagree on the protocol version. Reload the plugin and retry.".into(),
        ErrorCode::BadRequest => format!("The connection settings were rejected: {detail}"),
        ErrorCode::DriverNotFound => format!("The JDBC driver jar was not found: {detail}. Set the driver jar with JPA: Manage Database Connections."),
        ErrorCode::DriverLoadFailed => format!("The JDBC driver could not be loaded: {detail}. A jar the driver depends on may be missing; use the driver's self-contained jar."),
        ErrorCode::UrlUnsupported => "The driver does not accept this URL. Check the URL and that the jar belongs to this database.".into(),
        ErrorCode::UrlRejected => format!("The URL was refused for safety: {detail}. Remove that property from the URL."),
        ErrorCode::ConnectTimeout => "The database did not answer in time. Check the host and port, and that the server is running and reachable from this machine.".into(),
        ErrorCode::NetworkUnreachable => "The database server cannot be reached. Check the host and port, a VPN or firewall, and that the server is running.".into(),
        ErrorCode::TlsFailed => "The TLS handshake failed: the server certificate is not trusted or SSL is required. Fix the certificate or the URL's SSL options; the connection is never downgraded silently.".into(),
        ErrorCode::AuthFailed => format!("Authentication failed{state}. Check the user and password; JPA Assist does not retry on its own so the account is not locked."),
        ErrorCode::PermissionDenied => format!("The account may not read this metadata{state}: {detail}. Use an account with read access to the schema."),
        ErrorCode::SchemaNotFound => format!("The schema or catalog was not found: {detail}."),
        ErrorCode::ResultTooLarge => format!("The answer is too large for one request: {detail}. Narrow the table filter."),
        ErrorCode::DeadlineExceeded => "The database did not finish within the helper's time limit. Try a smaller selection or check the connection.".into(),
        ErrorCode::SqlError => format!("The database returned an error{state}: {detail}"),
        ErrorCode::Internal => format!("Unexpected helper error: {detail}"),
        ErrorCode::Unrecognized => format!("The helper reported an error this plugin version does not know{state}: {detail}"),
    }
}

pub fn connection_summary(info: &ConnectionInfo) -> String {
    let location = match (&info.catalog, &info.schema) {
        (Some(c), Some(s)) => format!("{c}.{s}"),
        (Some(x), None) | (None, Some(x)) => x.clone(),
        (None, None) => "none".to_owned(),
    };
    format!(
        "{} {} through {} {}; default catalog/schema: {location}; session read-only flag: {}.",
        info.product_name,
        info.product_version,
        info.driver_name,
        info.driver_version,
        if info.read_only { "set" } else { "not honoured by the driver (metadata calls only are used)" }
    )
}

/// Counts for the read-only snapshot summary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SnapshotCounts {
    pub tables: usize,
    pub views: usize,
    pub missing: usize,
    pub columns: usize,
    pub foreign_keys: usize,
    pub unknown_facts: usize,
    pub unknown_reasons: Vec<String>,
}

pub fn count_snapshot(described: &[TableMeta], mapped: Option<&Mapped>) -> SnapshotCounts {
    let mut counts = SnapshotCounts::default();
    for meta in described {
        if !meta.found {
            counts.missing += 1;
            continue;
        }
        if !matches!(meta.kind.as_deref(), Some("TABLE") | None) {
            counts.views += 1;
            continue;
        }
        counts.tables += 1;
        counts.columns += meta.columns.as_ref().map_or(0, Vec::len);
        counts.foreign_keys += meta.foreign_keys.as_ref().map_or(0, Vec::len);
    }
    let Some(mapped) = mapped else {
        return counts;
    };
    fn note(reason: &str, counts: &mut SnapshotCounts) {
        counts.unknown_facts += 1;
        if counts.unknown_reasons.len() < 3 && !counts.unknown_reasons.iter().any(|r| r == reason) {
            counts.unknown_reasons.push(reason.to_owned());
        }
    }
    fn reason<T>(fact: &Fact<T>) -> Option<&str> {
        match fact {
            Fact::Unknown(reason) => Some(reason),
            Fact::Known(_) => None,
        }
    }
    for table in &mapped.snapshot.tables {
        for found in [reason(&table.primary_key), reason(&table.foreign_keys), reason(&table.unique_constraints), reason(&table.indexes), reason(&table.columns)].into_iter().flatten() {
            note(found, &mut counts);
        }
        for column in table.columns.known().into_iter().flatten() {
            if let Some(found) = reason(&column.sql_type) {
                note(found, &mut counts);
            }
        }
    }
    counts
}

pub fn snapshot_summary(dbms: Dbms, schemas: &[String], counts: &SnapshotCounts, complete: bool) -> String {
    let mut text = format!(
        "{} tables ({} views, {} not found), {} columns, {} foreign keys in {}.",
        counts.tables,
        counts.views,
        counts.missing,
        counts.columns,
        counts.foreign_keys,
        if schemas.is_empty() { "the default schema".to_owned() } else { schemas.join(", ") }
    );
    if dbms.mapping_dialect().is_none() {
        text.push_str(&format!(" Type mapping for {} is not available yet, so column types are not classified.", dbms.label()));
    } else if counts.unknown_facts == 0 {
        text.push_str(" No unknown facts.");
    } else {
        text.push_str(&format!(" {} facts could not be proven (for example: {}).", counts.unknown_facts, counts.unknown_reasons.join("; ")));
    }
    if !complete {
        text.push_str(" The selection is filtered, so tables outside it are not proven absent.");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::introspect::{RawColumn, TableMeta};

    fn profile(url: &str) -> Profile {
        Profile {
            name: "dev".into(),
            dbms: Dbms::PostgreSql,
            url_template: url.into(),
            user: Some("app".into()),
            driver_jar: Some("/m2/postgresql.jar".into()),
            schema_filter: SchemaFilter::default(),
            spring_source: None,
        }
    }

    #[test]
    fn only_four_families_are_enabled() {
        for dbms in Dbms::ENABLED {
            assert!(dbms.is_enabled());
        }
        for dbms in [Dbms::SqlServer, Dbms::Oracle, Dbms::Db2] {
            assert!(!dbms.is_enabled());
            let mut p = profile("jdbc:sqlserver://h");
            p.dbms = dbms;
            p.url_template = format!("{}//h", dbms.url_prefix());
            assert!(p.validate().unwrap_err().0.contains("not enabled"), "{dbms:?}");
        }
        assert_eq!(Dbms::from_url("JDBC:MariaDB://h/db"), Some(Dbms::MariaDb));
        assert_eq!(Dbms::from_key("postgresql"), Some(Dbms::PostgreSql));
        assert_eq!(Dbms::from_url("jdbc:hsqldb:mem:x"), None);
    }

    #[test]
    fn profile_validation_refuses_credentials_placeholders_and_bad_jars() {
        assert!(profile("jdbc:postgresql://db:5432/app").validate().is_ok());
        for bad in ["jdbc:postgresql://u:p@db/app", "jdbc:postgresql://u@db/app", "jdbc:postgresql://db/app?password=x", "jdbc:postgresql://db/app?PWD=x", "jdbc:postgresql://${HOST}/app", "jdbc:mysql://db/app", "jdbc:postgresql://db/a\nb"] {
            assert!(profile(bad).validate().is_err(), "{bad}");
        }
        let mut p = profile("jdbc:postgresql://db/app");
        for jar in ["relative/pg.jar", "/abs/pg.zip", "/abs/\0pg.jar", ""] {
            p.driver_jar = Some(jar.into());
            assert!(p.validate().is_err(), "{jar}");
        }
        for jar in ["/abs/pg.jar", "C:\\drivers\\pg.jar", "C:/drivers/pg.jar", "\\\\srv\\share\\pg.jar"] {
            assert!(valid_jar_path(jar).is_ok(), "{jar}");
        }
        let mut h2 = profile("jdbc:h2:file:/data/shop");
        h2.dbms = Dbms::H2;
        assert!(h2.validate().is_ok());
        for bad in ["jdbc:h2:mem:test", "jdbc:h2:file:/data/shop;INIT=RUNSCRIPT FROM 'x'", "jdbc:h2:file:/d?x=1", "jdbc:h2:nope"] {
            h2.url_template = bad.into();
            assert!(h2.validate().is_err(), "{bad}");
        }
        p.driver_jar = None;
        p.name = "  ".into();
        assert!(p.validate().is_err());
        p.name = "x".repeat(81);
        assert!(p.validate().is_err());
    }

    #[test]
    fn a_stored_password_field_is_rejected() {
        let text = r#"{"version":1,"profiles":[{"name":"a","dbms":"postgresql","url_template":"jdbc:postgresql://h/d","user":null,"password":"S3NT1NEL-pw"}]}"#;
        assert!(parse_profiles(Some(text)).is_err());
        let mut profiles = Vec::new();
        upsert(&mut profiles, profile("jdbc:postgresql://db/app")).unwrap();
        let json = profiles_json(&profiles);
        assert_eq!(parse_profiles(Some(&json)).unwrap(), profiles);
        assert!(!json.to_ascii_lowercase().contains("password"));
    }

    #[test]
    fn store_errors_are_not_an_empty_list() {
        assert_eq!(parse_profiles(None).unwrap(), vec![]);
        assert!(parse_profiles(Some("{")).is_err());
        assert!(parse_profiles(Some(r#"{"version":2,"profiles":[]}"#)).is_err());
        let bad = r#"{"version":1,"profiles":[{"name":"a","dbms":"postgresql","url_template":"jdbc:postgresql://u:p@h/d","user":null}]}"#;
        assert!(parse_profiles(Some(bad)).unwrap_err().contains("remove the user name"));
    }

    #[test]
    fn upsert_replaces_by_case_insensitive_name_and_caps_the_count() {
        let mut profiles = Vec::new();
        upsert(&mut profiles, profile("jdbc:postgresql://a/db")).unwrap();
        let mut again = profile("jdbc:postgresql://b/db");
        again.name = "DEV".into();
        upsert(&mut profiles, again).unwrap();
        assert_eq!(profiles.len(), 1);
        assert!(profiles[0].url_template.contains("//b/"));
        for i in 0..MAX_PROFILES {
            let mut p = profile("jdbc:postgresql://a/db");
            p.name = format!("p{i}");
            let _ = upsert(&mut profiles, p);
        }
        assert_eq!(profiles.len(), MAX_PROFILES);
        let mut extra = profile("jdbc:postgresql://a/db");
        extra.name = "one-too-many".into();
        assert!(upsert(&mut profiles, extra).is_err());
    }

    #[test]
    fn drivers_cache_round_trips_and_rejects_bad_paths() {
        let mut map = BTreeMap::new();
        map.insert(Dbms::MariaDb, "/m2/mariadb.jar".to_owned());
        assert_eq!(parse_drivers(Some(&drivers_json(&map))).unwrap(), map);
        assert!(parse_drivers(Some(r#"{"postgresql":"pg.jar"}"#)).is_err());
        assert!(parse_drivers(Some("[")).is_err());
        assert!(parse_drivers(None).unwrap().is_empty());
    }

    #[test]
    fn targets_are_described_without_credentials() {
        assert_eq!(describe_target("jdbc:postgresql://db.example:5432/app?sslmode=require"), "db.example:5432, database app");
        assert_eq!(describe_target("jdbc:mysql://db/app"), "db, database app");
        assert_eq!(describe_target("jdbc:h2:file:/data/shop"), "file /data/shop");
        assert_eq!(describe_target("jdbc:h2:tcp://db:9092/shop"), "db:9092, database shop");
        assert_eq!(describe_target("jdbc:h2:/data/shop;MODE=PostgreSQL"), "file /data/shop");
    }

    #[test]
    fn confirmation_key_changes_with_url_user_or_jar() {
        let base = profile("jdbc:postgresql://db/app");
        let key = base.confirmation_key("/a.jar");
        let mut other = base.clone();
        other.url_template = "jdbc:postgresql://evil/app".into();
        assert_ne!(other.confirmation_key("/a.jar"), key);
        assert_ne!(base.confirmation_key("/b.jar"), key);
        other = base.clone();
        other.user = Some("root".into());
        assert_ne!(other.confirmation_key("/a.jar"), key);
    }

    #[test]
    fn filter_is_required_above_the_snapshot_bound_and_never_truncates() {
        assert!(!needs_filter(500));
        assert!(needs_filter(501));
        assert!(filter_required_message(501).contains("501 tables"));
        assert!(SchemaFilter::default().is_empty());
        assert!(!SchemaFilter { table_pattern: Some("ord%".into()), ..SchemaFilter::default() }.is_empty());
    }

    #[test]
    fn every_error_code_has_an_actionable_message() {
        let codes = [
            ErrorCode::JvmMissing,
            ErrorCode::JdkRequired,
            ErrorCode::JvmTooOld,
            ErrorCode::HelperProtocol,
            ErrorCode::Cancelled,
            ErrorCode::HostBudget,
            ErrorCode::ProcessDenied,
            ErrorCode::ProtocolVersion,
            ErrorCode::BadRequest,
            ErrorCode::DriverNotFound,
            ErrorCode::DriverLoadFailed,
            ErrorCode::UrlUnsupported,
            ErrorCode::UrlRejected,
            ErrorCode::ConnectTimeout,
            ErrorCode::NetworkUnreachable,
            ErrorCode::TlsFailed,
            ErrorCode::AuthFailed,
            ErrorCode::PermissionDenied,
            ErrorCode::SchemaNotFound,
            ErrorCode::ResultTooLarge,
            ErrorCode::DeadlineExceeded,
            ErrorCode::SqlError,
            ErrorCode::Internal,
            ErrorCode::Unrecognized,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for code in codes {
            let error = HelperError { code, message: "detail".into(), sql_state: Some("28P01".into()), vendor_code: 0, retryable: false };
            let text = error_message(&error);
            assert!(text.len() > 20 && !text.contains("{"), "{code:?}: {text}");
            assert!(seen.insert(text), "{code:?} repeats another message");
        }
    }

    fn meta(name: &str, kind: &str, columns: usize) -> TableMeta {
        let column = |i: usize| RawColumn {
            name: format!("c{i}"),
            type_name: "int4".into(),
            data_type: 4,
            size: None,
            scale: None,
            nullable: Some(1),
            default: None,
            auto_increment: None,
            generated: None,
            remarks: None,
            position: i as i32,
        };
        serde_json::from_value(serde_json::json!({"name": name, "schema": "public", "type": kind, "columns": (0..columns).map(column).map(|c| serde_json::json!({"name": c.name, "typeName": c.type_name, "position": c.position})).collect::<Vec<_>>(), "primaryKey": {"columns": []}, "foreignKeys": [], "indexes": []})).unwrap()
    }

    #[test]
    fn snapshot_counts_and_summary_state_what_is_unproven() {
        use crate::introspect_map::{MapOptions, to_snapshot};
        let described = vec![meta("a", "TABLE", 2), meta("b", "TABLE", 1), meta("v", "VIEW", 1)];
        let mapped = to_snapshot(Dialect::PostgreSql, &described, &MapOptions::default()).unwrap();
        let counts = count_snapshot(&described, Some(&mapped));
        assert_eq!((counts.tables, counts.views, counts.columns), (2, 1, 3), "view columns are not counted");
        let text = snapshot_summary(Dbms::PostgreSql, &["public".into()], &counts, false);
        assert!(text.contains("2 tables (1 views, 0 not found), 3 columns") && text.contains("not proven absent"), "{text}");

        let h2 = snapshot_summary(Dbms::H2, &[], &count_snapshot(&described, None), true);
        assert!(h2.contains("Type mapping for H2 is not available"), "{h2}");
    }
}
