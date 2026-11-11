//! "Add datasource configuration…" for a Spring Boot `application.properties` / `application-<profile>.properties`.
//!
//! Appends `spring.datasource.url` and `spring.datasource.username` plus `spring.datasource.password=${DB_PASSWORD}`.
//! The password itself is never asked for or written: Spring Boot resolves `${name}` placeholders against the
//! Environment, which includes environment variables (Spring Boot "Externalized Configuration", Property
//! Placeholders). `driver-class-name` is omitted because Spring Boot deduces the driver from the URL (Spring Boot
//! "SQL Databases"). URL templates: `jdbc:postgresql://host:port/db` (default port 5432), `jdbc:mysql://host:port/db`
//! (3306), `jdbc:mariadb://host:port/db` (3306), `jdbc:db2://host:port/db` (50000), `jdbc:sqlserver://host:port;databaseName=db`
//! (1433, Microsoft JDBC `serverName:portNumber;property=value`), `jdbc:oracle:thin:@//host:port/service` (1521, Oracle
//! thin service-name form), `jdbc:h2:mem:<name>` and `jdbc:hsqldb:mem:<name>`. Optional `spring.jpa.hibernate.ddl-auto`
//! (`none`, `validate`, `update`, `create`, `create-drop`; the empty choice writes nothing), `spring.jpa.show-sql=true`
//! and `spring.jpa.properties.hibernate.format_sql=true` (only written when enabled). Refused when the file already sets
//! `spring.datasource.url`, `.username` or `.password` or a JPA key about to be written (a second line would silently
//! override the first).
use super::*;

pub(super) const TITLE: &str = "Add datasource configuration…";
const MAX_FIELD: usize = 63;
const PASSWORD_LINE: &str = "spring.datasource.password=${DB_PASSWORD}";

fn properties_file(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    let stem = file.strip_suffix(".properties");
    stem.is_some_and(|stem| stem == "application" || stem.strip_prefix("application-").is_some_and(|p| !p.is_empty()))
}

pub(super) fn eligible(doc: &Document) -> bool {
    doc.language == "properties" && doc.text.len() <= MAX_TEXT && properties_file(&doc.path) && doc.path.contains("/src/main/resources/")
}

/// Actions for a Spring Boot properties document; `None` for every other document.
pub(super) fn actions(generation: &mut Generation, doc: &Document, selection: Range) -> Option<Vec<FileAction>> {
    if !properties_file(&doc.path) {
        return None;
    }
    let root = host::project_of(&doc.path).and_then(|project| project.root().ok()).and_then(|root| path(&root));
    let (true, Some(root)) = (eligible(doc), root) else { return Some(Vec::new()) };
    if path(&doc.path).as_deref() != Some(doc.path.as_str()) || !inside(&root, &doc.path) {
        return Some(Vec::new());
    }
    if generation.pending.len() > 8 {
        generation.pending.clear();
    }
    generation.next = generation.next.checked_add(1)?;
    let data = format!("jpa:{}:{}:{}", Operation::Datasource.key(), generation.next, doc.version);
    generation.pending.insert(data.clone(), Pending { root, doc: doc.clone(), operation: Operation::Datasource, selection });
    Some(vec![FileAction { title: TITLE.into(), kind: Some("refactor.rewrite".into()), data: Some(data), ..FileAction::default() }])
}

fn existing_keys(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with('#') && !line.starts_with('!'))
        .filter_map(|line| line.split(['=', ':', ' ', '\t']).next())
        .collect()
}

fn host_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && !value.starts_with(['-', '.'])
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

fn port(value: &str) -> Option<u16> {
    value.parse::<u16>().ok().filter(|port| *port > 0 && value.bytes().all(|b| b.is_ascii_digit()))
}

fn name(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_FIELD && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

enum Shape {
    Slash(&'static str),
    Semicolon(&'static str),
    OracleThin,
    InMemory(&'static str),
}

struct Choice {
    key: &'static str,
    label: &'static str,
    default_port: Option<u16>,
    shape: Shape,
}

const CHOICES: [Choice; 8] = [
    Choice { key: "postgresql", label: "PostgreSQL", default_port: Some(5432), shape: Shape::Slash("jdbc:postgresql") },
    Choice { key: "mysql", label: "MySQL", default_port: Some(3306), shape: Shape::Slash("jdbc:mysql") },
    Choice { key: "mariadb", label: "MariaDB", default_port: Some(3306), shape: Shape::Slash("jdbc:mariadb") },
    Choice { key: "h2", label: "H2 (in-memory)", default_port: None, shape: Shape::InMemory("jdbc:h2:mem") },
    Choice { key: "hsqldb", label: "HSQLDB (in-memory)", default_port: None, shape: Shape::InMemory("jdbc:hsqldb:mem") },
    Choice { key: "db2", label: "IBM Db2 LUW", default_port: Some(50000), shape: Shape::Slash("jdbc:db2") },
    Choice { key: "sqlserver", label: "Microsoft SQL Server", default_port: Some(1433), shape: Shape::Semicolon("jdbc:sqlserver") },
    Choice { key: "oracle", label: "Oracle", default_port: Some(1521), shape: Shape::OracleThin },
];

pub(super) fn url(key: &str, host: &str, port: Option<u16>, database: &str) -> Option<String> {
    let choice = CHOICES.iter().find(|c| c.key == key)?;
    if !name(database) {
        return None;
    }
    if let Shape::InMemory(scheme) = choice.shape {
        return Some(format!("{scheme}:{database}"));
    }
    if !host_name(host) {
        return None;
    }
    let port = port?;
    Some(match choice.shape {
        Shape::Slash(scheme) => format!("{scheme}://{host}:{port}/{database}"),
        Shape::Semicolon(scheme) => format!("{scheme}://{host}:{port};databaseName={database}"),
        _ => format!("jdbc:oracle:thin:@//{host}:{port}/{database}"),
    })
}

const DDL_AUTO_CHOICES: [(&str, &str); 6] = [
    ("unset", "(empty) do not set spring.jpa.hibernate.ddl-auto"),
    ("none", "none"),
    ("validate", "validate"),
    ("update", "update"),
    ("create", "create"),
    ("create-drop", "create-drop"),
];

const DDL_AUTO_KEY: &str = "spring.jpa.hibernate.ddl-auto";
const SHOW_SQL_KEY: &str = "spring.jpa.show-sql";
const FORMAT_SQL_KEY: &str = "spring.jpa.properties.hibernate.format_sql";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct JpaOptions {
    pub ddl_auto: Option<String>,
    pub show_sql: bool,
    pub format_sql: bool,
}

impl JpaOptions {
    fn lines(&self, eol: &str) -> String {
        let mut out = String::new();
        if let Some(value) = &self.ddl_auto {
            out.push_str(&format!("{DDL_AUTO_KEY}={value}{eol}"));
        }
        if self.show_sql {
            out.push_str(&format!("{SHOW_SQL_KEY}=true{eol}"));
        }
        if self.format_sql {
            out.push_str(&format!("{FORMAT_SQL_KEY}=true{eol}"));
        }
        out
    }

    fn keys(&self) -> Vec<&'static str> {
        [(self.ddl_auto.is_some(), DDL_AUTO_KEY), (self.show_sql, SHOW_SQL_KEY), (self.format_sql, FORMAT_SQL_KEY)]
            .into_iter()
            .filter_map(|(written, key)| written.then_some(key))
            .collect()
    }
}

/// The lines to append, using the document's own line ending; `None` when a key already exists.
pub(super) fn render(text: &str, url: &str, username: &str, jpa: &JpaOptions) -> Result<String, String> {
    let written = jpa.keys();
    for key in existing_keys(text) {
        if ["spring.datasource.url", "spring.datasource.username", "spring.datasource.password"].contains(&key) || written.contains(&key) {
            return Err(format!("This file already sets {key}; edit it by hand instead."));
        }
    }
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let lead = if text.is_empty() || text.ends_with('\n') { "" } else { eol };
    Ok(format!("{lead}spring.datasource.url={url}{eol}spring.datasource.username={username}{eol}{PASSWORD_LINE}{eol}{}", jpa.lines(eol)))
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    if !eligible(doc) {
        return Err(Error::Stale);
    }
    render(&doc.text, "x", "x", &JpaOptions::default())?;

    let mut items: Vec<PickItem> = CHOICES.iter().map(|c| PickItem::new(c.key, c.label)).collect();
    crate::datasource_facts::prefer_in(&crate::datasource_facts::discover(project), &mut items);
    let key = host::pick("Database for spring.datasource.url (no connection is made)", &items)?.ok_or(Error::Cancelled)?;
    let choice = CHOICES.iter().find(|c| c.key == key).ok_or(Error::Stale)?;

    let (host_value, port_value) = if choice.default_port.is_some() {
        let host_value = host::input("Database host", Some("localhost"))?.ok_or(Error::Cancelled)?.trim().to_owned();
        if !host_name(&host_value) {
            return Err("Use a host name or IPv4 address of letters, digits, dots and dashes.".into());
        }
        let default = choice.default_port.map(|p| p.to_string());
        let raw = host::input("Database port", default.as_deref())?.ok_or(Error::Cancelled)?.trim().to_owned();
        let port_value = port(&raw).ok_or_else(|| Error::from("The port must be a number from 1 to 65535."))?;
        (host_value, Some(port_value))
    } else {
        (String::new(), None)
    };
    let database = host::input("Database name", None)?.ok_or(Error::Cancelled)?.trim().to_owned();
    let username = host::input("Database username (the password is never asked for)", Some(if choice.key == "h2" { "sa" } else { "" }))?
        .ok_or(Error::Cancelled)?
        .trim()
        .to_owned();
    if !name(&database) || !name(&username) {
        return Err(format!("Database and user names use at most {MAX_FIELD} letters, digits, dots, dashes and underscores.").into());
    }
    let url = url(choice.key, &host_value, port_value, &database).ok_or(Error::Stale)?;
    let ddl_items: Vec<PickItem> = DDL_AUTO_CHOICES.iter().map(|(key, label)| PickItem::new(key, label)).collect();
    let ddl_auto = host::pick("Hibernate DDL auto (spring.jpa.hibernate.ddl-auto)", &ddl_items)?.ok_or(Error::Cancelled)?;
    if !DDL_AUTO_CHOICES.iter().any(|(key, _)| *key == ddl_auto) {
        return Err(Error::Stale);
    }
    let show_sql = host::confirm("Show SQL (spring.jpa.show-sql)?", "Confirm to write spring.jpa.show-sql=true; decline to leave it unset.")?;
    let format_sql = host::confirm("Format SQL (spring.jpa.properties.hibernate.format_sql)?", "Confirm to write spring.jpa.properties.hibernate.format_sql=true; decline to leave it unset.")?;
    let jpa = JpaOptions { ddl_auto: (ddl_auto != "unset").then_some(ddl_auto), show_sql, format_sql };
    let block = render(&doc.text, &url, &username, &jpa)?;
    let end = position(&doc.text, doc.text.len());
    let edits = vec![TextEdit { range: Range::new(end, end), text: block.clone() }];

    if !ui_kit::prompts::confirm_exact(
        "Add datasource configuration?",
        &format!(
            "Target: {}\nSource version: {}\n\nAppended exactly:\n{block}\nNo password is written: set the DB_PASSWORD environment variable (or a secret) before starting the application; Spring Boot resolves ${{DB_PASSWORD}} from the Environment. The driver class is deduced by Spring Boot from the URL. Nothing connects to the database.",
            doc.path, doc.version
        ),
    )? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    Ok(FileAction { title: "Add datasource configuration".into(), kind: Some("refactor.rewrite".into()), edits: vec![(doc.path.clone(), edits)], ..FileAction::default() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const PATH: &str = "/project/src/main/resources/application-dev.properties";

    fn setup(text: &str, path: &str) -> (MockHost, JpaAssist, Document) {
        let h = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        h.open_editor(path, "properties", text, &[]);
        let doc = Document { path: path.into(), language: "properties".into(), version: 1, text: text.into() };
        (h, JpaAssist::default(), doc)
    }

    fn offered(p: &mut JpaAssist, d: &Document) -> Option<FileAction> {
        p.file_actions(d, Range::default()).into_iter().find(|a| a.title == TITLE)
    }

    fn answers(h: &MockHost, values: &[&str], confirm: bool) {
        jpa_answers(h, values, "unset", false, false, confirm);
    }

    fn jpa_answers(h: &MockHost, values: &[&str], ddl_auto: &str, show_sql: bool, format_sql: bool, confirm: bool) {
        for value in values {
            h.answer_next_prompt(Some(&[value]));
        }
        h.answer_next_prompt(Some(&[ddl_auto]));
        h.answer_next_prompt(show_sql.then_some(&[]));
        h.answer_next_prompt(format_sql.then_some(&[]));
        h.answer_next_prompt(confirm.then_some(&[]));
    }

    fn appended(resolved: &FileAction) -> String {
        resolved.edits[0].1[0].text.clone()
    }

    #[test]
    fn url_templates_follow_the_verified_formats() {
        assert_eq!(url("postgresql", "db", Some(5432), "shop").as_deref(), Some("jdbc:postgresql://db:5432/shop"));
        assert_eq!(url("mysql", "localhost", Some(3306), "shop").as_deref(), Some("jdbc:mysql://localhost:3306/shop"));
        assert_eq!(url("mariadb", "h", Some(3307), "s").as_deref(), Some("jdbc:mariadb://h:3307/s"));
        assert_eq!(url("h2", "", None, "test").as_deref(), Some("jdbc:h2:mem:test"));
        assert_eq!(url("postgresql", "db", Some(5432), "a/b"), None);
        assert_eq!(url("postgresql", "db?x=1", Some(5432), "a"), None);
        assert_eq!(url("db2", "h", Some(50000), "bludb").as_deref(), Some("jdbc:db2://h:50000/bludb"));
        assert_eq!(url("sqlserver", "h", Some(1433), "shop").as_deref(), Some("jdbc:sqlserver://h:1433;databaseName=shop"));
        assert_eq!(url("oracle", "h", Some(1521), "ORCLPDB1").as_deref(), Some("jdbc:oracle:thin:@//h:1521/ORCLPDB1"));
        assert_eq!(url("hsqldb", "", None, "test").as_deref(), Some("jdbc:hsqldb:mem:test"));
        assert_eq!(url("sqlserver", "h;x=1", Some(1433), "shop"), None);
        assert_eq!(url("derby", "h", Some(1), "a"), None);
    }

    #[test]
    fn render_appends_with_the_document_line_ending_and_a_placeholder_password() {
        let url = "jdbc:postgresql://localhost:5432/shop";
        assert_eq!(
            render("a=b\n", url, "app", &JpaOptions::default()).unwrap(),
            "spring.datasource.url=jdbc:postgresql://localhost:5432/shop\nspring.datasource.username=app\nspring.datasource.password=${DB_PASSWORD}\n"
        );
        assert!(render("a=b", url, "app", &JpaOptions::default()).unwrap().starts_with("\nspring"));
        let crlf = render("a=b\r\nc=d", url, "app", &JpaOptions::default()).unwrap();
        assert!(crlf.starts_with("\r\nspring.datasource.url=") && crlf.ends_with("${DB_PASSWORD}\r\n") && !crlf.replace("\r\n", "").contains('\n'));
        assert!(render("", url, "app", &JpaOptions::default()).unwrap().starts_with("spring.datasource.url="));
    }

    #[test]
    fn existing_datasource_keys_are_refused_but_comments_and_other_keys_are_not() {
        for text in [
            "spring.datasource.url=x\n",
            "  spring.datasource.url = x\n",
            "spring.datasource.url:x\n",
            "spring.datasource.username=u\n",
            "spring.datasource.password=p\n",
        ] {
            assert!(render(text, "u", "n", &JpaOptions::default()).is_err(), "{text}");
        }
        for text in ["# spring.datasource.url=x\n", "spring.datasource.hikari.maximum-pool-size=3\n", "spring.datasource.urls=x\n"] {
            assert!(render(text, "u", "n", &JpaOptions::default()).is_ok(), "{text}");
        }
    }

    #[test]
    fn offered_only_for_main_resources_application_properties() {
        for (path, language, yes) in [
            (PATH, "properties", true),
            ("/project/src/main/resources/application.properties", "properties", true),
            ("/project/src/main/resources/application.yml", "yaml", false),
            ("/project/src/main/resources/messages.properties", "properties", false),
            ("/project/src/test/resources/application.properties", "properties", false),
            ("/project/src/main/resources/application-.properties", "properties", false),
        ] {
            let (_h, mut p, d) = setup("a=b\n", path);
            let d = Document { language: language.into(), ..d };
            assert_eq!(offered(&mut p, &d).is_some(), yes, "{path}");
        }
    }

    #[test]
    fn postgres_flow_is_an_exact_versioned_append_without_a_password() {
        let (h, mut p, d) = setup("a=b\n", PATH);
        let pending = offered(&mut p, &d).unwrap();
        answers(&h, &["postgresql", "db.internal", "5432", "shop", "app"], true);
        let resolved = p.resolve_file_action(&d, pending).unwrap();
        assert_eq!(resolved.edits[0].0, PATH);
        let block = appended(&resolved);
        assert_eq!(
            block,
            "spring.datasource.url=jdbc:postgresql://db.internal:5432/shop\nspring.datasource.username=app\nspring.datasource.password=${DB_PASSWORD}\n"
        );
        assert!(resolved.operations.is_empty());
        assert_eq!(h.editor_text(), "a=b\n");
        assert!(h.requests().is_empty() && h.process_runs().is_empty());
    }

    #[test]
    fn mysql_mariadb_and_h2_flows_and_crlf() {
        for (key, inputs, expected) in [
            ("mysql", vec!["localhost", "3306", "d", "u"], "jdbc:mysql://localhost:3306/d"),
            ("mariadb", vec!["h", "3307", "d", "u"], "jdbc:mariadb://h:3307/d"),
            ("h2", vec!["mem1", "sa"], "jdbc:h2:mem:mem1"),
            ("hsqldb", vec!["mem1", "sa"], "jdbc:hsqldb:mem:mem1"),
            ("db2", vec!["h", "50000", "d", "u"], "jdbc:db2://h:50000/d"),
            ("sqlserver", vec!["h", "1433", "d", "u"], "jdbc:sqlserver://h:1433;databaseName=d"),
            ("oracle", vec!["h", "1521", "svc", "u"], "jdbc:oracle:thin:@//h:1521/svc"),
        ] {
            let (h, mut p, d) = setup("a=b\r\n", PATH);
            let pending = offered(&mut p, &d).unwrap();
            let mut values = vec![key];
            values.extend(inputs);
            answers(&h, &values, true);
            let block = appended(&p.resolve_file_action(&d, pending).unwrap());
            assert!(block.starts_with(&format!("spring.datasource.url={expected}\r\n")), "{block}");
            assert!(!block.replace("\r\n", "").contains('\n'));
        }
    }

    #[test]
    fn jpa_options_are_written_only_when_chosen_and_never_include_a_password() {
        let (h, mut p, d) = setup("a=b\n", PATH);
        let pending = offered(&mut p, &d).unwrap();
        jpa_answers(&h, &["postgresql", "h", "5432", "d", "u"], "validate", true, true, true);
        let block = appended(&p.resolve_file_action(&d, pending).unwrap());
        assert_eq!(
            block,
            "spring.datasource.url=jdbc:postgresql://h:5432/d\nspring.datasource.username=u\nspring.datasource.password=${DB_PASSWORD}\nspring.jpa.hibernate.ddl-auto=validate\nspring.jpa.show-sql=true\nspring.jpa.properties.hibernate.format_sql=true\n"
        );
        for ddl in ["none", "update", "create", "create-drop"] {
            let (h, mut p, d) = setup("a=b\n", PATH);
            let pending = offered(&mut p, &d).unwrap();
            jpa_answers(&h, &["postgresql", "h", "5432", "d", "u"], ddl, false, false, true);
            assert!(appended(&p.resolve_file_action(&d, pending).unwrap()).ends_with(&format!("{{DB_PASSWORD}}\nspring.jpa.hibernate.ddl-auto={ddl}\n")));
        }
        let (h, mut p, d) = setup("spring.jpa.show-sql=false\n", PATH);
        let pending = offered(&mut p, &d).unwrap();
        jpa_answers(&h, &["postgresql", "h", "5432", "d", "u"], "unset", true, false, true);
        assert!(p.resolve_file_action(&d, pending).is_err(), "an existing JPA key that would be written is refused");
        let (h, mut p, d) = setup("spring.jpa.show-sql=false\n", PATH);
        let pending = offered(&mut p, &d).unwrap();
        jpa_answers(&h, &["postgresql", "h", "5432", "d", "u"], "unset", false, false, true);
        assert!(p.resolve_file_action(&d, pending).is_ok(), "an untouched existing JPA key is left alone");
    }

    #[test]
    fn invalid_input_cancel_and_existing_keys_refuse_without_an_edit() {
        for values in [
            vec!["postgresql", "bad host", "5432", "d", "u"],
            vec!["postgresql", "h", "0", "d", "u"],
            vec!["postgresql", "h", "70000", "d", "u"],
            vec!["postgresql", "h", "54x2", "d", "u"],
            vec!["postgresql", "h", "5432", "d;drop", "u"],
            vec!["postgresql", "h", "5432", "d", "u:p"],
            vec!["postgresql", "h", "5432", "", "u"],
        ] {
            let (h, mut p, d) = setup("a=b\n", PATH);
            let pending = offered(&mut p, &d).unwrap();
            answers(&h, &values, true);
            assert!(p.resolve_file_action(&d, pending).is_err(), "{values:?}");
        }
        let (h, mut p, d) = setup("a=b\n", PATH);
        let pending = offered(&mut p, &d).unwrap();
        answers(&h, &["postgresql", "h", "5432", "d", "u"], false);
        assert!(matches!(p.resolve_file_action(&d, pending), Err(Error::Cancelled)));

        let (_h, mut p, d) = setup("spring.datasource.url=jdbc:h2:mem:x\n", PATH);
        let pending = offered(&mut p, &d).unwrap();
        assert!(p.resolve_file_action(&d, pending).is_err(), "refuses before any prompt");
    }

    #[test]
    fn stale_document_is_refused() {
        let (h, mut p, d) = setup("a=b\n", PATH);
        let pending = offered(&mut p, &d).unwrap();
        answers(&h, &["postgresql", "h", "5432", "d", "u"], true);
        let newer = Document { version: 2, text: "a=c\n".into(), ..d };
        assert!(p.resolve_file_action(&newer, pending).is_err());
    }

    #[test]
    fn detected_dialect_is_listed_first_and_marked_but_still_selectable() {
        let (h, mut p, d) = setup("a=b\n", PATH);
        h.add_file("/project/src/main/resources/application.properties", "spring.datasource.url=jdbc:mysql://h/db\n");
        let pending = offered(&mut p, &d).unwrap();
        answers(&h, &["postgresql", "h", "5432", "d", "u"], true);
        let block = appended(&p.resolve_file_action(&d, pending).unwrap());
        assert!(block.contains("jdbc:postgresql://h:5432/d"));
    }
}
