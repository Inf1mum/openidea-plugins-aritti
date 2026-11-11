//! Data sources declared in literal project configuration (never connects, never keeps a password).
//!
//! Verified against (fetched 2026-11): Spring Boot "SQL Databases" (`spring.datasource.url|username|password`,
//! `driver-class-name` deduced from the URL, `spring.datasource.hikari.*` prefix; named data sources are NOT
//! documented there, so only the default `spring.datasource.*` is read), Spring Boot "Externalized Configuration"
//! (`${name}` placeholders resolve against the Environment, including environment variables), Quarkus datasource guide
//! (`quarkus.datasource.jdbc.url`, `quarkus.datasource.username`, named `quarkus.datasource."name".jdbc.url`, profile
//! prefix `%prod.`), PostgreSQL JDBC (`jdbc:postgresql://host:port/db`, `user`/`password` URL parameters), MySQL
//! Connector/J (`jdbc:mysql://host:port/db`, `user:password@host`), MariaDB Connector/J (`jdbc:mariadb://`), H2
//! (`jdbc:h2:mem:`/`jdbc:h2:file:`), Microsoft JDBC (`jdbc:sqlserver://`, `;property=value`, `password={...}` escapes),
//! Oracle JDBC (`jdbc:oracle:thin:[user/password]@...`), IBM Db2 (`jdbc:db2://server:port/database`).
//! Micronaut was not verified and is not read.
//!
//! Credential stripping: userinfo (`user:pass@` after `//`, `user/pass@` after an Oracle driver type) is removed, and
//! any URL parameter whose name contains `password` or is `pwd`, plus `user`/`username`/`uid`, is dropped (`;` and
//! `&` delimited, brace-escaped values included). If a password-like parameter still remains the URL becomes unknown.
use persistence_core::schema::Fact;

const MAX_FILES: u32 = 40;
const MAX_TEXT: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dialect {
    PostgreSql,
    MySql,
    MariaDb,
    H2,
    SqlServer,
    Oracle,
    Db2,
    Other(String),
}

impl Dialect {
    /// The `host::pick` key of the two dialects the SQL generators support.
    pub fn sql_key(&self) -> Option<&'static str> {
        match self {
            Self::PostgreSql => Some("postgresql"),
            Self::MySql => Some("mysql"),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataSource {
    pub name: String,
    pub profile: Option<String>,
    pub url: Fact<String>,
    pub dialect: Fact<Dialect>,
    pub driver: Option<String>,
    pub username_present: bool,
    pub source_file: String,
    pub key_prefix: String,
}

/// Dialect from the JDBC URL prefix only.
pub fn dialect_of(url: &str) -> Fact<Dialect> {
    let lower = url.trim().to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("jdbc:") else {
        return Fact::Unknown(format!("`{}` is not a JDBC URL", url.trim()));
    };
    let sub = rest.split([':', ';', '/', '?']).next().unwrap_or("");
    Fact::Known(match sub {
        "postgresql" => Dialect::PostgreSql,
        "mysql" => Dialect::MySql,
        "mariadb" => Dialect::MariaDb,
        "h2" => Dialect::H2,
        "sqlserver" => Dialect::SqlServer,
        "oracle" => Dialect::Oracle,
        "db2" => Dialect::Db2,
        "" => return Fact::unknown("empty JDBC subprotocol"),
        other => Dialect::Other(other.to_owned()),
    })
}

fn is_secret_param(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    name.contains("password") || ["pwd", "passwd", "user", "username", "uid"].contains(&name.as_str())
}

/// Removes userinfo and credential parameters; `None` when a password-like parameter could not be removed.
pub fn strip_credentials(url: &str) -> Option<String> {
    let url = url.trim();
    let mut head = url.to_owned();
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("jdbc:oracle:") {
        let colons: Vec<usize> = url.match_indices(':').map(|(i, _)| i).collect();
        if let (Some(&third), Some(at)) = (colons.get(2), url.find('@')) {
            if at > third {
                head = format!("{}{}", &url[..=third], &url[at..]);
            }
        }
    } else if let Some(slashes) = url.find("//") {
        let start = slashes + 2;
        let end = url[start..].find(['/', '?', ';']).map_or(url.len(), |i| start + i);
        if let Some(at) = url[start..end].rfind('@') {
            head = format!("{}{}", &url[..start], &url[start + at + 1..]);
        }
    }

    let mut parts: Vec<(char, String)> = Vec::new();
    let mut sep = ' ';
    let mut current = String::new();
    let mut in_brace = false;
    let mut chars = head.chars().peekable();
    while let Some(c) = chars.next() {
        if in_brace {
            current.push(c);
            if c == '}' {
                match chars.peek() {
                    Some('}') => current.extend(chars.next()),
                    _ => in_brace = false,
                }
            }
        } else if c == '{' && current.ends_with('=') {
            in_brace = true;
            current.push(c);
        } else if matches!(c, '?' | '&' | ';') {
            parts.push((sep, std::mem::take(&mut current)));
            sep = c;
        } else {
            current.push(c);
        }
    }
    parts.push((sep, current));

    let query = parts.iter().any(|(s, _)| *s == '?');
    let mut out = String::new();
    let mut query_open = false;
    for (sep, segment) in parts {
        if sep != ' ' && is_secret_param(segment.split('=').next().unwrap_or("")) {
            continue;
        }
        match sep {
            ' ' => {}
            '&' | '?' if query && !query_open => {
                out.push('?');
                query_open = true;
                out.push_str(&segment);
                continue;
            }
            other => out.push(other),
        }
        out.push_str(&segment);
    }
    let lowered = out.to_ascii_lowercase();
    if lowered.contains("password") || lowered.contains("pwd=") {
        return None;
    }
    Some(out)
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    let value = match value.find(" #") {
        Some(at) => value[..at].trim(),
        None => value,
    };
    value.trim_matches(['"', '\'']).trim().to_owned()
}

/// `(flattened key, value, from_later_document)` for every setting of a `.properties` or YAML file.
fn flatten(text: &str, yaml: bool) -> Vec<(String, String, bool)> {
    let mut found = Vec::new();
    let mut later = false;
    let mut stack: Vec<(usize, String)> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if yaml && trimmed == "---" {
            later = true;
            stack.clear();
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        let (key, value) = if yaml {
            let Some((key, value)) = trimmed.split_once(':') else {
                continue;
            };
            (key.trim().trim_matches(['"', '\'']).to_owned(), value.to_owned())
        } else {
            let Some(at) = trimmed.find(['=', ':']) else {
                continue;
            };
            (trimmed[..at].trim().to_owned(), trimmed[at + 1..].to_owned())
        };
        let full = if yaml {
            let indent = line.len() - line.trim_start().len();
            while stack.last().is_some_and(|(i, _)| *i >= indent) {
                stack.pop();
            }
            let full = stack.iter().map(|(_, k)| k.as_str()).chain([key.as_str()]).collect::<Vec<_>>().join(".");
            if unquote(&value).is_empty() {
                stack.push((indent, key));
            }
            full
        } else {
            key
        };
        found.push((full.replace('"', ""), unquote(&value), later));
    }
    found
}

fn build(source_file: &str, profile: Option<String>, name: &str, key_prefix: &str, url: &str, driver: Option<String>, username: bool) -> DataSource {
    let raw_dialect = dialect_of(url);
    let (url_fact, dialect) = if url.contains("${") {
        (Fact::Unknown(format!("placeholder {}", url_placeholders(url))), raw_dialect)
    } else {
        match strip_credentials(url) {
            Some(clean) => (Fact::Known(clean), raw_dialect),
            None => (Fact::unknown("URL carries a credential that could not be stripped"), Fact::unknown("URL not kept")),
        }
    };
    DataSource {
        name: name.to_owned(),
        profile,
        url: url_fact,
        dialect,
        driver,
        username_present: username,
        source_file: source_file.to_owned(),
        key_prefix: key_prefix.to_owned(),
    }
}

/// Only the `${...}` tokens of a value, so literal text around them (possibly a secret) is never echoed.
fn url_placeholders(value: &str) -> String {
    let mut out = Vec::new();
    let mut rest = value;
    while let Some(at) = rest.find("${") {
        let Some(end) = rest[at..].find('}') else { break };
        out.push(&rest[at..=at + end]);
        rest = &rest[at + end + 1..];
    }
    out.join(" ")
}

#[derive(Default)]
struct Group {
    url: Option<String>,
    driver: Option<String>,
    username: bool,
}

fn from_application(path: &str, text: &str, out: &mut Vec<DataSource>) {
    let file = path.rsplit('/').next().unwrap_or(path);
    let file_profile = file.strip_prefix("application-").and_then(|rest| rest.rsplit_once('.')).map(|(p, _)| p.to_owned());
    let yaml = !file.ends_with(".properties");
    let mut groups: Vec<((Option<String>, String, String, bool), Group)> = Vec::new();
    for (key, value, later) in flatten(text, yaml) {
        let (quarkus_profile, key) = match key.strip_prefix('%').and_then(|k| k.split_once('.')) {
            Some((profile, rest)) => (Some(profile.to_owned()), rest.to_owned()),
            None => (None, key),
        };
        let profile = quarkus_profile.or_else(|| file_profile.clone()).or_else(|| later.then(|| "yaml-document".to_owned()));
        let (name, prefix, field) = if let Some(field) = key.strip_prefix("spring.datasource.") {
            ("default".to_owned(), "spring.datasource".to_owned(), field.to_owned())
        } else if let Some(rest) = key.strip_prefix("quarkus.datasource.") {
            match rest.split_once('.') {
                Some((name, field)) if name != "jdbc" => (name.to_owned(), format!("quarkus.datasource.\"{name}\""), field.to_owned()),
                _ => ("default".to_owned(), "quarkus.datasource".to_owned(), rest.to_owned()),
            }
        } else {
            continue;
        };
        let slot = (profile, name, prefix, later);
        let index = match groups.iter().position(|(k, _)| *k == slot) {
            Some(i) => i,
            None => {
                groups.push((slot, Group::default()));
                groups.len() - 1
            }
        };
        let group = &mut groups[index].1;
        match field.as_str() {
            "url" | "jdbc.url" => group.url = Some(value),
            "driver-class-name" | "jdbc.driver" => group.driver = Some(value).filter(|v| !v.is_empty()),
            "username" => group.username = true,
            _ => {}
        }
    }
    for ((profile, name, prefix, _), group) in groups {
        if let Some(url) = group.url {
            out.push(build(path, profile, &name, &prefix, &url, group.driver, group.username));
        }
    }
}

fn from_persistence_xml(path: &str, text: &str, out: &mut Vec<DataSource>) {
    let mut url = None;
    let mut driver = None;
    let mut username = false;
    for tag in text.split("<property").skip(1) {
        let Some(tag) = tag.split('>').next() else { continue };
        let attr = |name: &str| {
            let at = tag.find(&format!("{name}=\""))? + name.len() + 2;
            let rest = &tag[at..];
            Some(rest[..rest.find('"')?].trim().to_owned())
        };
        let (Some(name), value) = (attr("name"), attr("value")) else { continue };
        match name.as_str() {
            "jakarta.persistence.jdbc.url" | "javax.persistence.jdbc.url" => url = value,
            "jakarta.persistence.jdbc.driver" | "javax.persistence.jdbc.driver" => driver = value,
            "jakarta.persistence.jdbc.user" | "javax.persistence.jdbc.user" => username = true,
            _ => {}
        }
    }
    if let Some(url) = url {
        out.push(build(path, None, "persistence-unit", "jakarta.persistence.jdbc", &url, driver, username));
    }
}

pub struct Sources<'a> {
    pub applications: &'a [(String, String)],
    pub persistence_xml: &'a [(String, String)],
}

pub fn discover_from(sources: &Sources) -> Vec<DataSource> {
    let mut out = Vec::new();
    for (path, text) in sources.applications {
        from_application(path, text, &mut out);
    }
    for (path, text) in sources.persistence_xml {
        from_persistence_xml(path, text, &mut out);
    }
    out
}

fn skipped(path: &str) -> bool {
    ["/src/test/", "/target/", "/build/", "/node_modules/"].iter().any(|part| path.contains(part))
}

/// Bounded literal sources with the existing `project.read` capability only.
pub fn discover(project: &openidea_plugin::Project) -> Vec<DataSource> {
    let gather = |patterns: &[&str], keep: &dyn Fn(&str) -> bool| -> Vec<(String, String)> {
        let mut out = Vec::new();
        for pattern in patterns {
            let Ok(files) = project.find_files(pattern, MAX_FILES) else {
                continue;
            };
            for file in files.into_iter().filter(|f| !skipped(f) && keep(f)).take(MAX_FILES as usize) {
                if let Ok(text) = project.read_file(&file).map(|t| t.to_string()) {
                    if text.len() <= MAX_TEXT {
                        out.push((file, text));
                    }
                }
            }
        }
        out
    };
    let applications = gather(&["**/application*.properties", "**/application*.yml", "**/application*.yaml"], &|f| {
        f.contains("/src/main/resources/") || f.starts_with("src/main/resources/")
    });
    let persistence_xml = gather(&["**/persistence.xml"], &|_| true);
    discover_from(&Sources { applications: &applications, persistence_xml: &persistence_xml })
}

/// The `host::pick` key when every discovered data source is the same named one with the same known PostgreSQL or
/// MySQL dialect (profile files of one data source agree); `None` otherwise, so prompts stay unchanged.
pub fn single_sql_dialect(sources: &[DataSource]) -> Option<&'static str> {
    let first = sources.first()?;
    let key = first.dialect.known()?.sql_key()?;
    sources
        .iter()
        .all(|s| s.name == first.name && s.dialect.known().and_then(Dialect::sql_key) == Some(key))
        .then_some(key)
}

/// Moves the detected dialect to the front of a pick list and marks it; the user can still choose any item.
pub fn prefer(project: &openidea_plugin::Project, items: &mut [openidea_plugin::PickItem]) {
    prefer_in(&discover(project), items);
}

pub fn prefer_in(sources: &[DataSource], items: &mut [openidea_plugin::PickItem]) {
    let Some(key) = single_sql_dialect(sources) else { return };
    let Some(at) = items.iter().position(|i| i.key == key) else { return };
    items[at].description = Some("default: detected from datasource configuration".into());
    items[..=at].rotate_right(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(files: &[(&str, &str)]) -> Vec<DataSource> {
        let own: Vec<(String, String)> = files.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        discover_from(&Sources { applications: &own, persistence_xml: &[] })
    }
    const APP: &str = "/p/src/main/resources/application.properties";

    #[test]
    fn dialect_comes_from_verified_prefixes_only() {
        for (url, expected) in [
            ("jdbc:postgresql://h:5432/db", Dialect::PostgreSql),
            ("jdbc:mysql://h/db", Dialect::MySql),
            ("jdbc:mariadb://h/db", Dialect::MariaDb),
            ("jdbc:h2:mem:test", Dialect::H2),
            ("jdbc:h2:file:/data/sample", Dialect::H2),
            ("jdbc:sqlserver://h;databaseName=x", Dialect::SqlServer),
            ("jdbc:oracle:thin:@//h:1521/svc", Dialect::Oracle),
            ("jdbc:db2://h:50000/db", Dialect::Db2),
            ("jdbc:sqlite:file.db", Dialect::Other("sqlite".into())),
        ] {
            assert_eq!(dialect_of(url), Fact::Known(expected), "{url}");
        }
        assert!(matches!(dialect_of("postgres://h/db"), Fact::Unknown(_)));
    }

    #[test]
    fn credentials_are_stripped_from_every_url_style() {
        for (raw, clean) in [
            ("jdbc:mysql://sandy:secret@myhost:3306/db", "jdbc:mysql://myhost:3306/db"),
            ("jdbc:mysql://sandy:secret@[a:1,b:2]/db?useSSL=true", "jdbc:mysql://[a:1,b:2]/db?useSSL=true"),
            ("jdbc:postgresql://localhost/test?user=fred&password=secret&ssl=true", "jdbc:postgresql://localhost/test?ssl=true"),
            ("jdbc:postgresql://localhost/test?password=secret", "jdbc:postgresql://localhost/test"),
            ("jdbc:postgresql://localhost/test?ssl=true&password=secret", "jdbc:postgresql://localhost/test?ssl=true"),
            ("jdbc:mariadb://localhost:3306/DB?user=root&password=myPassword", "jdbc:mariadb://localhost:3306/DB"),
            ("jdbc:sqlserver://localhost;encrypt=true;user=u;password=p;databaseName=a", "jdbc:sqlserver://localhost;encrypt=true;databaseName=a"),
            ("jdbc:sqlserver://localhost;password={pa;ss}}word};encrypt=true", "jdbc:sqlserver://localhost;encrypt=true"),
            ("jdbc:oracle:thin:HR/secret@localhost:5221/orcl", "jdbc:oracle:thin:@localhost:5221/orcl"),
            ("jdbc:oracle:thin:@//h:1521/svc", "jdbc:oracle:thin:@//h:1521/svc"),
            ("jdbc:h2:mem:test;DB_CLOSE_DELAY=-1;PASSWORD=x", "jdbc:h2:mem:test;DB_CLOSE_DELAY=-1"),
        ] {
            let clean_out = strip_credentials(raw).unwrap_or_else(|| panic!("{raw}"));
            assert_eq!(clean_out, clean, "{raw}");
            assert!(!clean_out.contains("secret") && !clean_out.contains("myPassword"), "{raw}");
        }
    }

    #[test]
    fn spring_properties_and_yaml_are_discovered_without_secrets() {
        let props = "spring.datasource.url=jdbc:postgresql://db:5432/shop?password=hunter2\nspring.datasource.username=app\nspring.datasource.password=hunter2\nspring.datasource.driver-class-name=org.postgresql.Driver\n";
        let [ds] = found(&[(APP, props)]).try_into().unwrap();
        assert_eq!(ds.url, Fact::Known("jdbc:postgresql://db:5432/shop".into()));
        assert_eq!(ds.dialect, Fact::Known(Dialect::PostgreSql));
        assert_eq!(ds.driver.as_deref(), Some("org.postgresql.Driver"));
        assert!(ds.username_present && ds.profile.is_none());
        assert_eq!((ds.name.as_str(), ds.key_prefix.as_str()), ("default", "spring.datasource"));
        assert!(!format!("{ds:?}").contains("hunter2"));

        let yaml = "spring:\n  datasource:\n    url: \"jdbc:mysql://h:3306/x\"\n    username: u\n    password: p\n---\nspring:\n  datasource:\n    url: jdbc:h2:mem:t\n";
        let two = found(&[("/p/src/main/resources/application-dev.yml", yaml)]);
        assert_eq!(two.len(), 2);
        assert_eq!(two[0].profile.as_deref(), Some("dev"));
        assert_eq!(two[0].dialect, Fact::Known(Dialect::MySql));
        assert_eq!(two[1].profile.as_deref(), Some("dev"));
        let multi = found(&[("/p/src/main/resources/application.yml", yaml)]);
        assert_eq!(multi[1].profile.as_deref(), Some("yaml-document"));
    }

    #[test]
    fn placeholders_make_the_url_unknown_but_keep_only_the_placeholder_text() {
        let [ds] = found(&[(APP, "spring.datasource.url=jdbc:postgresql://${DB_HOST}/shop?password=zzz\n")]).try_into().unwrap();
        assert_eq!(ds.url, Fact::Unknown("placeholder ${DB_HOST}".into()));
        assert_eq!(ds.dialect, Fact::Known(Dialect::PostgreSql));
        let [env] = found(&[(APP, "spring.datasource.url=${DB_URL}\n")]).try_into().unwrap();
        assert!(matches!(env.dialect, Fact::Unknown(_)));
        assert!(found(&[(APP, "spring.datasource.username=u\n")]).is_empty());
    }

    #[test]
    fn quarkus_default_named_and_profile_forms() {
        let props = "quarkus.datasource.db-kind=postgresql\nquarkus.datasource.jdbc.url=jdbc:postgresql://localhost:5432/hibernate_orm_test\nquarkus.datasource.username=u\nquarkus.datasource.\"inventory\".jdbc.url=jdbc:mariadb://h/inv\nquarkus.datasource.\"inventory\".username=i\n%prod.quarkus.datasource.jdbc.url=jdbc:postgresql://prod/db\n";
        let all = found(&[(APP, props)]);
        assert_eq!(all.len(), 3, "{all:?}");
        let default = all.iter().find(|d| d.name == "default" && d.profile.is_none()).unwrap();
        assert_eq!(default.key_prefix, "quarkus.datasource");
        assert!(default.username_present);
        let named = all.iter().find(|d| d.name == "inventory").unwrap();
        assert_eq!(named.key_prefix, "quarkus.datasource.\"inventory\"");
        assert_eq!(named.dialect, Fact::Known(Dialect::MariaDb));
        assert!(named.username_present);
        assert_eq!(all.iter().find(|d| d.profile.as_deref() == Some("prod")).unwrap().name, "default");
    }

    #[test]
    fn persistence_xml_properties_are_read_for_both_namespaces() {
        for ns in ["jakarta", "javax"] {
            let xml = format!("<properties><property name=\"{ns}.persistence.jdbc.url\" value=\"jdbc:h2:mem:t;PASSWORD=q\"/><property name=\"{ns}.persistence.jdbc.user\" value=\"sa\"/><property name=\"{ns}.persistence.jdbc.password\" value=\"q\"/></properties>");
            let own = [("/p/src/main/resources/META-INF/persistence.xml".to_owned(), xml)];
            let [ds] = discover_from(&Sources { applications: &[], persistence_xml: &own }).try_into().unwrap();
            assert_eq!(ds.url, Fact::Known("jdbc:h2:mem:t".into()));
            assert!(ds.username_present);
        }
    }

    #[test]
    fn single_sql_dialect_needs_one_agreeing_known_postgres_or_mysql_source() {
        let pg = "spring.datasource.url=jdbc:postgresql://h/db\n";
        assert_eq!(single_sql_dialect(&found(&[(APP, pg)])), Some("postgresql"));
        let dev = "/p/src/main/resources/application-dev.properties";
        assert_eq!(single_sql_dialect(&found(&[(APP, pg), (dev, pg)])), Some("postgresql"));
        assert_eq!(single_sql_dialect(&found(&[(APP, pg), (dev, "spring.datasource.url=jdbc:mysql://h/db\n")])), None);
        assert_eq!(single_sql_dialect(&found(&[(APP, "spring.datasource.url=jdbc:h2:mem:t\n")])), None);
        assert_eq!(single_sql_dialect(&found(&[(APP, "spring.datasource.url=${X}\n")])), None);
        let two = "quarkus.datasource.jdbc.url=jdbc:postgresql://h/a\nquarkus.datasource.\"b\".jdbc.url=jdbc:postgresql://h/b\n";
        assert_eq!(single_sql_dialect(&found(&[(APP, two)])), None);
        assert_eq!(single_sql_dialect(&[]), None);
    }

    #[test]
    fn prefer_moves_the_detected_dialect_first_and_leaves_other_cases_untouched() {
        use openidea_plugin::PickItem;
        let items = || [PickItem::new("postgresql", "PostgreSQL"), PickItem::new("mysql", "MySQL")];
        let mut mysql = items();
        prefer_in(&found(&[(APP, "spring.datasource.url=jdbc:mysql://h/db\n")]), &mut mysql);
        assert_eq!((mysql[0].key.as_str(), mysql[1].key.as_str()), ("mysql", "postgresql"));
        assert!(mysql[0].description.is_some() && mysql[1].description.is_none());
        let mut none = items();
        prefer_in(&[], &mut none);
        assert_eq!((none[0].key.as_str(), none[0].description.clone()), ("postgresql", None));
        let mut h2 = items();
        prefer_in(&found(&[(APP, "spring.datasource.url=jdbc:h2:mem:t\n")]), &mut h2);
        assert_eq!(h2[0].key, "postgresql");
    }
}
