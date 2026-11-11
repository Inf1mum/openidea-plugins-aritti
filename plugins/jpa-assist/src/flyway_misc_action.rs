//! Reviewed create-file actions for projects that provably use Flyway: a Java migration skeleton, a SQL callback
//! and a Java callback skeleton. Nothing is executed and no runner is duplicated (see the `flyway` plugin).
//!
//! Facts below come from `flyway-core` 9.22.3 and 10.22.0 inspected with `javap`/reflection (Maven Central
//! jars; documentation.flyway.dev was unreachable from the build host, so documentation-only rules such as
//! the default `classpath:db/migration` location are NOT doc-verified and stay conservative):
//! `BaseJavaMigration` (abstract, `org.flywaydb.core.api.migration`) leaves `migrate(Context) throws Exception`
//! abstract; `Context.getConnection()` returns `java.sql.Connection`; `BaseCallback`
//! (`org.flywaydb.core.api.callback`) leaves only `handle(Event, Context)` abstract and is therefore the
//! minimal callback base; `Event.getId()` values are the SQL callback names. The names are identical in both versions
//! (10.22.0 adds only `BEFORE_CREATE_SCHEMA`, which is not offered). Flyway 10 splits database support into modules;
//! these API types remain in `flyway-core`, so only a literal `flyway-core` in the build files is the proof.
//!
//! Location rule: only the default Flyway location is supported. Java migrations go to
//! `<module>/src/main/java/db/migration`, accepted only when an existing Java migration already lives there or an
//! existing versioned SQL layout `<module>/src/main/resources/db/migration` proves the classpath location.
//! Configured locations or naming refuse. Kotlin migrations are not generated (not compiled/verified).
use super::migration_action::{absent, absolute, custom_flyway_settings, description_of, flyway_folder, next_version, read_optional, version_key, versioned_ext, MAX_MIGRATIONS};
use super::*;
use std::collections::BTreeSet;

pub(super) const TITLE_JAVA_MIGRATION: &str = "Create Flyway Java migration…";
pub(super) const TITLE_SQL_CALLBACK: &str = "Create Flyway SQL callback…";
pub(super) const TITLE_JAVA_CALLBACK: &str = "Create Flyway Java callback…";
const JAVA_DIR_SUFFIX: &str = "/src/main/java/db/migration";
const SQL_DIR_SUFFIX: &str = "/src/main/resources/db/migration";

/// (SQL callback name, whether a SQL script of that name is offered). Statement events are only offered for Java.
const EVENTS: [(&str, bool); 27] = [
    ("beforeClean", true),
    ("afterClean", true),
    ("afterCleanError", true),
    ("beforeMigrate", true),
    ("beforeEachMigrate", true),
    ("beforeEachMigrateStatement", false),
    ("afterEachMigrateStatement", false),
    ("afterEachMigrateStatementError", false),
    ("afterEachMigrate", true),
    ("afterEachMigrateError", true),
    ("beforeRepeatables", true),
    ("afterVersioned", true),
    ("afterMigrateApplied", true),
    ("afterMigrate", true),
    ("afterMigrateError", true),
    ("beforeValidate", true),
    ("afterValidate", true),
    ("afterValidateError", true),
    ("beforeBaseline", true),
    ("afterBaseline", true),
    ("afterBaselineError", true),
    ("beforeRepair", true),
    ("afterRepair", true),
    ("afterRepairError", true),
    ("beforeInfo", true),
    ("afterInfo", true),
    ("afterInfoError", true),
];

fn event_constant(id: &str) -> String {
    let mut constant = String::new();
    for ch in id.chars() {
        if ch.is_ascii_uppercase() {
            constant.push('_');
        }
        constant.push(ch.to_ascii_uppercase());
    }
    constant
}

fn doc_directory(doc: &Document) -> Option<(String, String)> {
    let root = host::project_of(&doc.path).and_then(|project| project.root().ok()).and_then(|root| path(&root))?;
    if doc.text.len() > MAX_TEXT || path(&doc.path).as_deref() != Some(doc.path.as_str()) || !inside(&root, &doc.path) {
        return None;
    }
    Some((root, doc.path.rsplit_once('/')?.0.to_owned()))
}

fn eligible_java_callback(doc: &Document) -> bool {
    doc.language == "java"
        && language_of(&doc.path) == Some(false)
        && doc.path.contains("/src/main/java/")
        && package(doc).is_some()
}

pub(super) fn operations(doc: &Document) -> Vec<(Operation, &'static str)> {
    let source = matches!(doc.language.as_str(), "java" | "kotlin") && language_of(&doc.path) == Some(doc.language == "kotlin");
    let sql = doc.language == "sql" && doc.path.ends_with(".sql");
    let Some((root, directory)) = doc_directory(doc).filter(|_| source || sql) else {
        return Vec::new();
    };
    let flyway = host::project_of(&doc.path)
        .and_then(|project| mapper_action::build_text(&project, &root, &directory).ok())
        .is_some_and(|build| build.contains("flyway-core"));
    if !flyway {
        return Vec::new();
    }
    let mut operations = vec![
        (Operation::FlywayJavaMigration, TITLE_JAVA_MIGRATION),
        (Operation::FlywaySqlCallback, TITLE_SQL_CALLBACK),
    ];
    if eligible_java_callback(doc) {
        operations.push((Operation::FlywayJavaCallback, TITLE_JAVA_CALLBACK));
    }
    operations
}

pub(super) fn sql_actions(generation: &mut Generation, doc: &Document) -> Option<Vec<FileAction>> {
    if !doc.path.ends_with(".sql") {
        return None;
    }
    let operations = operations(doc);
    let (root, _) = doc_directory(doc)?;
    if operations.is_empty() {
        return Some(Vec::new());
    }
    if generation.pending.len() > 8 {
        generation.pending.clear();
    }
    Some(
        operations
            .into_iter()
            .filter_map(|(operation, title)| {
                generation.next = generation.next.checked_add(1)?;
                let data = format!("jpa:{}:{}:{}", operation.key(), generation.next, doc.version);
                generation.pending.insert(data.clone(), Pending { root: root.clone(), doc: doc.clone(), operation, selection: Range::default() });
                Some(FileAction { title: title.into(), kind: Some("refactor.rewrite".into()), data: Some(data), ..FileAction::default() })
            })
            .collect(),
    )
}

fn prove_flyway(project: &Project, root: &str, directory: &str) -> Result<(), Error> {
    let build = mapper_action::build_text(project, root, directory)?;
    crate::project_facts::gate(crate::project_facts::Feature::Flyway, &build).into_result()?;
    if custom_flyway_settings(project)? {
        return Err("Flyway locations or naming are configured; only the default location and V__ naming are supported, and nothing is guessed.".into());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct JavaPlan {
    directory: String,
    package: String,
    version: String,
}

fn java_plan(project: &Project, root: &str, doc_directory: &str) -> Result<JavaPlan, Error> {
    prove_flyway(project, root, doc_directory)?;
    let mut versions: Vec<String> = Vec::new();
    let mut java_directories = BTreeSet::new();
    let java = project.find_files("**/V*__*.java", MAX_MIGRATIONS)?;
    let sql = project.find_files("**/src/main/resources/db/migration/**/*", MAX_MIGRATIONS)?;
    if java.len() >= MAX_MIGRATIONS as usize || sql.len() >= MAX_MIGRATIONS as usize {
        return Err("Migration discovery reached its limit.".into());
    }
    for file in java.iter().map(|f| absolute(root, f)) {
        let (directory, name) = file.rsplit_once('/').ok_or(Error::Stale)?;
        let Some(version) = versioned_ext(name, ".java") else {
            continue;
        };
        if file.contains("/src/test/") || !project.read_file(&file)?.contains("JavaMigration") {
            continue;
        }
        if !directory.ends_with(JAVA_DIR_SUFFIX) {
            return Err(format!("The existing Java migration {file} is not under the default src/main/java/db/migration location; the location is not proven, so nothing was generated.").into());
        }
        versions.push(version.to_owned());
        java_directories.insert(directory.to_owned());
    }
    for file in sql.iter().map(|f| absolute(root, f)) {
        let name = file.rsplit('/').next().unwrap_or_default();
        if let Some(version) = versioned_ext(name, ".sql") {
            versions.push(version.to_owned());
        }
    }
    if versions.is_empty() {
        return Err("No existing V<version>__<description> migration was found; the version convention cannot be determined.".into());
    }
    let directory = match java_directories.len() {
        0 => {
            let folder = flyway_folder(project, root)?;
            let module = folder.directory.strip_suffix(SQL_DIR_SUFFIX).ok_or("The SQL migration folder is not under src/main/resources/db/migration.")?;
            format!("{module}{JAVA_DIR_SUFFIX}")
        }
        1 => java_directories.into_iter().next().ok_or(Error::Stale)?,
        _ => return Err("Several Java migration folders exist; the target is ambiguous.".into()),
    };
    if path(&directory).as_deref() != Some(directory.as_str()) || !inside(root, &directory) {
        return Err(Error::Stale);
    }
    let version = next_version(&versions.iter().map(String::as_str).collect::<Vec<_>>()).replace('.', "_");
    if versions.iter().any(|existing| version_key(existing) == version_key(&version)) {
        return Err(format!("Version {version} already exists among the SQL or Java migrations; nothing is overwritten.").into());
    }
    let package = directory.split_once("/src/main/java/").ok_or(Error::Stale)?.1.replace('/', ".");
    if !package.split('.').all(identifier) {
        return Err("The migration package is not a valid Java package.".into());
    }
    Ok(JavaPlan { directory, package, version })
}

fn render_java_migration(package: &str, class: &str) -> String {
    format!(
        "package {package};\n\nimport java.sql.Connection;\n\nimport org.flywaydb.core.api.migration.BaseJavaMigration;\nimport org.flywaydb.core.api.migration.Context;\n\npublic class {class} extends BaseJavaMigration {{\n    @Override\n    public void migrate(Context context) throws Exception {{\n        Connection connection = context.getConnection();\n    }}\n}}\n"
    )
}

fn render_java_callback(package: &str, class: &str, events: &[&str]) -> String {
    let conditions = events.iter().map(|id| format!("event == Event.{}", event_constant(id))).collect::<Vec<_>>().join("\n                || ");
    format!(
        "package {package};\n\nimport org.flywaydb.core.api.callback.BaseCallback;\nimport org.flywaydb.core.api.callback.Context;\nimport org.flywaydb.core.api.callback.Event;\n\npublic class {class} extends BaseCallback {{\n    @Override\n    public boolean supports(Event event, Context context) {{\n        return {conditions};\n    }}\n\n    @Override\n    public void handle(Event event, Context context) {{\n    }}\n}}\n"
    )
}

fn render_sql_callback(event: &str) -> String {
    format!("-- Flyway SQL callback for the {event} event.\n")
}

fn confirm(title: &str, body: &str) -> Result<(), Error> {
    if ui_kit::prompts::confirm_exact(title, body)? {
        return Ok(());
    }
    Err(Error::Cancelled)
}

fn create(title: String, target: String, text: String) -> FileAction {
    FileAction {
        title,
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target.clone(), text, overwrite: false }],
        open: Some(target),
        ..FileAction::default()
    }
}

fn directory_of(pending: &Pending) -> Result<String, Error> {
    Ok(pending.doc.path.rsplit_once('/').ok_or(Error::Stale)?.0.to_owned())
}

pub(super) fn resolve_java_migration(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let root = pending.root.as_str();
    let doc_dir = directory_of(pending)?;
    let plan = java_plan(project, root, &doc_dir)?;
    let description = host::input("Migration description (ASCII letters and digits)", None)?.ok_or(Error::Cancelled)?;
    let description = description_of(&description);
    if description.is_empty() {
        return Err("Enter a description containing ASCII letters or digits.".into());
    }
    let class = format!("V{}__{description}", plan.version);
    let target = format!("{}/{class}.java", plan.directory);
    if !identifier(&class) || path(&target).is_none() {
        return Err("The migration class name is not valid.".into());
    }
    let source = render_java_migration(&plan.package, &class);
    absent(project, &target)?;
    absent(project, &format!("{}/{class}.kt", plan.directory))?;
    confirm(
        "Create Flyway Java migration?",
        &format!("Target: {target}\nSource version: {}\n\n{source}\nThe migrate method only obtains the connection from the context and executes nothing; add your own logic. One non-overwriting Create; nothing is run.", doc.version),
    )?;
    current(pending, doc)?;
    if java_plan(project, root, &doc_dir)? != plan {
        return Err(Error::Stale);
    }
    absent(project, &target)?;
    absent(project, &format!("{}/{class}.kt", plan.directory))?;
    Ok(create(format!("Create Flyway Java migration {class}"), target, source))
}

pub(super) fn resolve_sql_callback(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let root = pending.root.as_str();
    let doc_dir = directory_of(pending)?;
    prove_flyway(project, root, &doc_dir)?;
    let folder = flyway_folder(project, root)?;
    let items: Vec<PickItem> = EVENTS.iter().filter(|(_, sql)| *sql).map(|(id, _)| PickItem::new(id, id)).collect();
    let event = host::pick("Flyway callback event", &items)?.ok_or(Error::Cancelled)?;
    if !EVENTS.iter().any(|(id, sql)| *sql && *id == event) {
        return Err(Error::Stale);
    }
    let description = host::input("Callback description (optional, ASCII letters and digits)", None)?.ok_or(Error::Cancelled)?;
    let name = match description.trim() {
        "" => format!("{event}.sql"),
        text => match description_of(text).as_str() {
            "" => return Err("The description has no ASCII letters or digits.".into()),
            description => format!("{event}__{description}.sql"),
        },
    };
    let target = format!("{}/{name}", folder.directory);
    if folder.names.iter().any(|existing| existing.eq_ignore_ascii_case(&name)) || path(&target).is_none() {
        return Err(format!("{name} already exists in {}; nothing is overwritten.", folder.directory).into());
    }
    absent(project, &target)?;
    let text = render_sql_callback(&event);
    confirm(
        "Create Flyway SQL callback?",
        &format!("Target: {target}\n\n{text}\nA comment-only SQL script (a no-op); add statements yourself. One non-overwriting Create; nothing is run."),
    )?;
    current(pending, doc)?;
    if flyway_folder(project, root)?.directory != folder.directory || read_optional(project, &target)?.is_some() {
        return Err(Error::Stale);
    }
    absent(project, &target)?;
    Ok(create(format!("Create Flyway SQL callback {name}"), target, text))
}

pub(super) fn resolve_java_callback(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    if !eligible_java_callback(doc) {
        return Err(Error::Stale);
    }
    let root = pending.root.as_str();
    let directory = directory_of(pending)?;
    prove_flyway(project, root, &directory)?;
    let items: Vec<PickItem> = EVENTS.iter().map(|(id, _)| PickItem::new(id, id)).collect();
    let picked = host::pick_many("Events handled by the callback", &items)?.ok_or(Error::Cancelled)?;
    let events: Vec<&str> = EVENTS.iter().map(|(id, _)| *id).filter(|id| picked.iter().any(|p| p == id)).collect();
    if events.is_empty() {
        return Err("Select at least one event.".into());
    }
    let class = capitalize(&ask_identifier("Callback class name")?.ok_or(Error::Cancelled)?);
    if !identifier(&class) {
        return Err("Choose a valid class name.".into());
    }
    let package = package(doc).ok_or(Error::Stale)?;
    let target = format!("{directory}/{class}.java");
    no_collision(project, root, &directory, &class, None)?;
    let source = render_java_callback(&package, &class, &events);
    confirm(
        "Create Flyway Java callback?",
        &format!("Target: {target}\nSource version: {}\n\n{source}\nThe handle method is empty; add your own logic. Flyway must be given the callback (for example as a Spring bean or through the configuration); this action does not register it. One non-overwriting Create; nothing is run.", doc.version),
    )?;
    current(pending, doc)?;
    no_collision(project, root, &directory, &class, None)?;
    Ok(create(format!("Create Flyway Java callback {class}"), target, source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const JAVA_PATH: &str = "/project/src/main/java/shop/Customer.java";
    const JAVA: &str = "package shop;\n\npublic class Customer {\n}\n";
    const SQL_PATH: &str = "/project/src/main/resources/db/migration/V1__first.sql";
    const MIGRATIONS: &str = "/project/src/main/resources/db/migration";
    const POM: &str = "<dependency><artifactId>flyway-core</artifactId></dependency>";

    fn java_setup(with_flyway: bool) -> (MockHost, JpaAssist, Document) {
        let setup = base_setup(with_flyway);
        setup.0.add_file("src/main/resources/db/migration/V1__first.sql", "select 1;");
        setup.0.add_file("src/main/resources/db/migration/V2__second.sql", "select 1;");
        setup
    }

    fn base_setup(with_flyway: bool) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(JAVA_PATH, "java", JAVA, &[]);
        host.add_file("src/main/java/shop/Customer.java", JAVA);
        if with_flyway {
            host.add_file("pom.xml", POM);
        }
        let doc = Document { path: JAVA_PATH.into(), language: "java".into(), text: JAVA.into(), version: 1 };
        (host, JpaAssist::default(), doc)
    }

    fn pick(plugin: &mut JpaAssist, doc: &Document, title: &str) -> FileAction {
        plugin.file_actions(doc, Range::default()).into_iter().find(|a| a.title == title).unwrap()
    }

    fn run_migration(host: &MockHost, plugin: &mut JpaAssist, doc: &Document, description: &str, confirm: bool) -> Result<FileAction, Error> {
        let action = pick(plugin, doc, TITLE_JAVA_MIGRATION);
        host.answer_next_prompt(Some(&[description]));
        host.answer_next_prompt(confirm.then_some(&[]));
        plugin.resolve_file_action(doc, action)
    }

    fn only_create(action: &FileAction) -> (&str, &str) {
        let [FileOperation::Create { path, text, overwrite: false }] = action.operations.as_slice() else { panic!("one create") };
        assert!(action.edits.is_empty());
        (path, text)
    }

    #[test]
    fn java_migration_skeleton_uses_next_version_and_executes_nothing() {
        let (host, mut plugin, doc) = java_setup(true);
        let action = run_migration(&host, &mut plugin, &doc, "Add users!", true).unwrap();
        let (path, text) = only_create(&action);
        assert_eq!(path, "/project/src/main/java/db/migration/V3__Add_users.java");
        assert_eq!(text, render_java_migration("db.migration", "V3__Add_users"));
        assert!(text.contains("extends BaseJavaMigration") && text.contains("public void migrate(Context context) throws Exception"));
        assert!(text.contains("context.getConnection()") && !text.contains("TODO") && !text.contains("execute"));
        assert!(host.process_runs().is_empty());
    }

    #[test]
    fn existing_java_migrations_count_and_set_the_folder() {
        let (host, mut plugin, doc) = java_setup(true);
        host.add_file("src/main/java/db/migration/V5__legacy.java", "package db.migration;\npublic class V5__legacy extends org.flywaydb.core.api.migration.BaseJavaMigration {}\n");
        let action = run_migration(&host, &mut plugin, &doc, "next", true).unwrap();
        assert_eq!(only_create(&action).0, "/project/src/main/java/db/migration/V6__next.java");
    }

    #[test]
    fn java_migration_refusals() {
        let (_host, mut plugin, doc) = java_setup(false);
        assert!(plugin.file_actions(&doc, Range::default()).iter().all(|a| !a.title.contains("Flyway")), "no flyway-core");

        let (host, mut plugin, doc) = java_setup(true);
        host.add_file("flyway.conf", "flyway.locations=filesystem:sql\n");
        assert!(run_migration(&host, &mut plugin, &doc, "x", true).is_err(), "configured locations");

        let (host, mut plugin, doc) = java_setup(true);
        host.add_file("src/main/java/app/migrations/V9__x.java", "public class V9__x extends BaseJavaMigration {}\n");
        assert!(run_migration(&host, &mut plugin, &doc, "x", true).is_err(), "location not proven");

        let (host, mut plugin, doc) = java_setup(true);
        host.add_file("src/main/resources/db/migration/mysql/V9__x.sql", "");
        assert!(run_migration(&host, &mut plugin, &doc, "x", true).is_err(), "ambiguous sql layout");

        let (host, mut plugin, doc) = java_setup(true);
        assert!(run_migration(&host, &mut plugin, &doc, "!!!", true).is_err(), "empty description");

        let (host, mut plugin, doc) = java_setup(true);
        assert!(matches!(run_migration(&host, &mut plugin, &doc, "x", false), Err(Error::Cancelled)));

        let (host, mut plugin, doc) = java_setup(true);
        host.add_file("src/main/java/db/migration/V3__Add_users.kt", "");
        assert!(run_migration(&host, &mut plugin, &doc, "Add users", true).is_err(), "existing kotlin class");

        let (host, mut plugin, doc) = base_setup(true);
        assert!(run_migration(&host, &mut plugin, &doc, "x", true).is_err(), "no convention");

        let (_host, mut plugin, doc) = java_setup(true);
        let action = pick(&mut plugin, &doc, TITLE_JAVA_MIGRATION);
        let changed = Document { version: 2, ..doc };
        assert!(matches!(plugin.resolve_file_action(&changed, action), Err(Error::Stale)));
    }

    #[test]
    fn version_collision_is_detected_across_notations() {
        assert_eq!(version_key("1_10"), version_key("1.10"));
        assert_ne!(version_key("1_10"), version_key("1_1"));
    }

    fn sql_callback(host: &MockHost, plugin: &mut JpaAssist, doc: &Document, event: &str, description: &str) -> Result<FileAction, Error> {
        let action = pick(plugin, doc, TITLE_SQL_CALLBACK);
        host.answer_next_prompt(Some(&[event]));
        host.answer_next_prompt(Some(&[description]));
        host.answer_next_prompt(Some(&[]));
        plugin.resolve_file_action(doc, action)
    }

    #[test]
    fn sql_callback_is_one_comment_line_in_the_migration_folder() {
        let (host, mut plugin, doc) = java_setup(true);
        let action = sql_callback(&host, &mut plugin, &doc, "beforeMigrate", "").unwrap();
        let (path, text) = only_create(&action);
        assert_eq!(path, format!("{MIGRATIONS}/beforeMigrate.sql"));
        assert_eq!(text, "-- Flyway SQL callback for the beforeMigrate event.\n");
        let action = sql_callback(&host, &mut plugin, &doc, "afterMigrate", "Refresh views").unwrap();
        assert_eq!(only_create(&action).0, format!("{MIGRATIONS}/afterMigrate__Refresh_views.sql"));
    }

    #[test]
    fn sql_callback_is_offered_from_sql_documents_and_refuses_duplicates_and_unknown_events() {
        let (host, mut plugin, _) = java_setup(true);
        let doc = Document { path: SQL_PATH.into(), language: "sql".into(), text: "select 1;".into(), version: 1 };
        host.open_editor(SQL_PATH, "sql", "select 1;", &[]);
        let titles: Vec<_> = plugin.file_actions(&doc, Range::default()).into_iter().map(|a| a.title).collect();
        assert!(titles.iter().any(|t| t == TITLE_JAVA_MIGRATION) && titles.iter().any(|t| t == TITLE_SQL_CALLBACK));
        host.add_file("src/main/resources/db/migration/beforeMigrate.sql", "-- x\n");
        assert!(sql_callback(&host, &mut plugin, &doc, "beforeMigrate", "").is_err());
        let (host, mut plugin, doc) = java_setup(true);
        assert!(sql_callback(&host, &mut plugin, &doc, "beforeEachMigrateStatement", "").is_err());
        let (host, mut plugin, doc) = java_setup(true);
        host.add_file("src/main/resources/db/migration/beforeMigrate.sql", "-- x\n");
        assert!(sql_callback(&host, &mut plugin, &doc, "afterMigrate", "").is_ok());
        let (_host, mut plugin, doc) = java_setup(false);
        assert!(plugin.file_actions(&doc, Range::default()).iter().all(|a| !a.title.contains("Flyway")), "gated on flyway-core");
    }

    #[test]
    fn java_callback_extends_base_callback_with_selected_events() {
        let (host, mut plugin, doc) = java_setup(true);
        let action = pick(&mut plugin, &doc, TITLE_JAVA_CALLBACK);
        host.answer_next_prompt(Some(&["beforeMigrate", "afterEachMigrateError"]));
        host.answer_next_prompt(Some(&["auditCallback"]));
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, action).unwrap();
        let (path, text) = only_create(&action);
        assert_eq!(path, "/project/src/main/java/shop/AuditCallback.java");
        assert_eq!(text, render_java_callback("shop", "AuditCallback", &["beforeMigrate", "afterEachMigrateError"]));
        assert!(text.contains("event == Event.BEFORE_MIGRATE\n                || event == Event.AFTER_EACH_MIGRATE_ERROR;"), "{text}");

        let (host, mut plugin, doc) = java_setup(true);
        host.add_file("src/main/java/shop/AuditCallback.java", "package shop;\nclass AuditCallback {}\n");
        let action = pick(&mut plugin, &doc, TITLE_JAVA_CALLBACK);
        host.answer_next_prompt(Some(&["beforeMigrate"]));
        host.answer_next_prompt(Some(&["AuditCallback"]));
        host.answer_next_prompt(Some(&[]));
        assert!(plugin.resolve_file_action(&doc, action).is_err());

        let (host, mut plugin, doc) = java_setup(true);
        let action = pick(&mut plugin, &doc, TITLE_JAVA_CALLBACK);
        host.answer_next_prompt(Some(&[]));
        assert!(plugin.resolve_file_action(&doc, action).is_err(), "no events");
    }

    #[test]
    fn event_constants_follow_the_enum_naming() {
        assert_eq!(event_constant("afterEachMigrateStatementError"), "AFTER_EACH_MIGRATE_STATEMENT_ERROR");
        assert_eq!(event_constant("beforeClean"), "BEFORE_CLEAN");
    }

    #[test]
    fn nothing_is_offered_for_unrelated_documents_and_kotlin_has_no_java_callback() {
        let (_host, mut plugin, doc) = java_setup(true);
        let titles: Vec<_> = plugin.file_actions(&doc, Range::default()).into_iter().map(|a| a.title).collect();
        assert!(titles.iter().any(|t| t == TITLE_JAVA_CALLBACK));
        let kotlin = Document { path: "/project/src/main/kotlin/shop/C.kt".into(), language: "kotlin".into(), text: "package shop\n".into(), version: 1 };
        assert!(operations(&kotlin).iter().all(|(op, _)| !matches!(op, Operation::FlywayJavaCallback)));
        let text = Document { path: "/project/notes.txt".into(), language: "plaintext".into(), text: String::new(), version: 1 };
        assert!(operations(&text).is_empty());
    }

    #[test]
    #[ignore = "requires javac and FLYWAY_CORE_JAR (flyway-core jar path; none is in ~/.m2)"]
    fn generated_flyway_classes_compile_against_flyway_core() {
        let jar = std::env::var("FLYWAY_CORE_JAR").expect("explicit flyway-core jar path");
        let dir = std::env::temp_dir().join(format!("jpa-flyway-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let all: Vec<&str> = EVENTS.iter().map(|(id, _)| *id).collect();
        let files = [
            ("V3__Add_users.java", render_java_migration("db.migration", "V3__Add_users")),
            ("AllCallback.java", render_java_callback("db.migration", "AllCallback", &all)),
        ];
        let paths: Vec<_> = files
            .iter()
            .map(|(name, body)| {
                let path = dir.join(name);
                std::fs::write(&path, body).unwrap();
                path
            })
            .collect();
        let result = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &jar, "-d"]).arg(&dir).args(&paths).output().expect("javac installed");
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
