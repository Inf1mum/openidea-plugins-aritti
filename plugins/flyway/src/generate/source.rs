//! Parameterized Java/Kotlin migration and SQL/Java callback workspace actions.
use super::*;
use openidea_plugin::PickItem;
pub(super) const JAVA: &str = "java-migration";
pub(super) const KOTLIN: &str = "kotlin-migration";
pub(super) const JAVA_CALLBACK: &str = "java-callback";
pub(super) const SQL_CALLBACK: &str = "sql-callback";
const EVENTS: &[(&str, &str)] = &[
    ("beforeClean", "BEFORE_CLEAN"), ("afterClean", "AFTER_CLEAN"), ("afterCleanError", "AFTER_CLEAN_ERROR"),
    ("beforeMigrate", "BEFORE_MIGRATE"), ("afterMigrate", "AFTER_MIGRATE"), ("afterMigrateError", "AFTER_MIGRATE_ERROR"),
    ("beforeEachMigrate", "BEFORE_EACH_MIGRATE"), ("afterEachMigrate", "AFTER_EACH_MIGRATE"), ("afterEachMigrateError", "AFTER_EACH_MIGRATE_ERROR"),
    ("beforeValidate", "BEFORE_VALIDATE"), ("afterValidate", "AFTER_VALIDATE"), ("afterValidateError", "AFTER_VALIDATE_ERROR"),
    ("beforeBaseline", "BEFORE_BASELINE"), ("afterBaseline", "AFTER_BASELINE"), ("afterBaselineError", "AFTER_BASELINE_ERROR"),
    ("beforeRepair", "BEFORE_REPAIR"), ("afterRepair", "AFTER_REPAIR"), ("afterRepairError", "AFTER_REPAIR_ERROR"),
    ("beforeInfo", "BEFORE_INFO"), ("afterInfo", "AFTER_INFO"), ("afterInfoError", "AFTER_INFO_ERROR"),
    ("beforeUndo", "BEFORE_UNDO"), ("afterUndo", "AFTER_UNDO"), ("afterUndoError", "AFTER_UNDO_ERROR"),
    ("beforeEachUndo", "BEFORE_EACH_UNDO"), ("afterEachUndo", "AFTER_EACH_UNDO"), ("afterEachUndoError", "AFTER_EACH_UNDO_ERROR"),
    ("beforeEachMigrateStatement", "BEFORE_EACH_MIGRATE_STATEMENT"), ("afterEachMigrateStatement", "AFTER_EACH_MIGRATE_STATEMENT"),
    ("afterEachMigrateStatementError", "AFTER_EACH_MIGRATE_STATEMENT_ERROR"), ("beforeRepeatables", "BEFORE_REPEATABLES"),
    ("afterVersioned", "AFTER_VERSIONED"), ("afterMigrateApplied", "AFTER_MIGRATE_APPLIED"),
    ("beforeEachUndoStatement", "BEFORE_EACH_UNDO_STATEMENT"), ("afterEachUndoStatement", "AFTER_EACH_UNDO_STATEMENT"),
    ("afterEachUndoStatementError", "AFTER_EACH_UNDO_STATEMENT_ERROR"), ("beforeCreateSchema", "BEFORE_CREATE_SCHEMA"), ("afterConnect", "AFTER_CONNECT"),
];
fn identifier(value: &str) -> bool {
    const RESERVED: &[&str] = &["abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "const", "continue", "default", "do", "double", "else", "enum", "extends", "final", "finally", "float", "for", "goto", "if", "implements", "import", "instanceof", "int", "interface", "long", "native", "new", "package", "private", "protected", "public", "return", "short", "static", "strictfp", "super", "switch", "synchronized", "this", "throw", "throws", "transient", "try", "void", "volatile", "while", "true", "false", "null", "record", "sealed", "permits", "yield", "var", "fun", "val", "object", "when", "in", "is", "as", "typealias", "_", "it"];
    value.len() <= 200 && value.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') && !RESERVED.contains(&value)
}
fn package(value: &str) -> bool { value.is_empty() || value.len() <= 512 && value.split('.').all(identifier) }
fn source_directory(root: &str, value: &str) -> Result<String, Error> {
    let value = relative(root, value)?;
    if value.split('/').any(|part| part.starts_with('.')) { return Err("Choose a visible project source directory.".into()); }
    Ok(value)
}
fn callback_events(multiple: bool) -> Result<Vec<usize>, Error> {
    let choices: Vec<_> = EVENTS.iter().enumerate().map(|(index, (event, _))| PickItem::new(&index.to_string(), event).description(if event.contains("Undo") { "Requires a Flyway edition with Undo support; generation does not enable the feature." } else { "Callback availability depends on installed Flyway version/edition." })).collect();
    let picked = if multiple { host::pick_many("Callback events (availability depends on installed Flyway edition/version)", &choices)?.ok_or(Error::Cancelled)? }
        else { vec![host::pick("Callback event (availability depends on installed Flyway edition/version)", &choices)?.ok_or(Error::Cancelled)?] };
    let selected: BTreeSet<_> = picked.iter().map(|key| key.parse::<usize>().ok().filter(|index| *index < EVENTS.len()).ok_or(Error::Stale)).collect::<Result<_, _>>()?;
    if selected.is_empty() { return Err(Error::Cancelled); }
    Ok(selected.into_iter().collect())
}
fn target_owned(project: &Project, root: &str, target: &str) -> Result<(), Error> {
    if host::project_of(target).and_then(|owner| owner.root().ok()).as_deref() != Some(root) { return Err(Error::Stale); }
    vacant(project, target)
}
fn version_collision(project: &Project, config: &Config, target: &str) -> Result<(), Error> {
    let file = target.rsplit('/').next().ok_or(Error::Stale)?;
    let mut naming = config.naming.clone(); naming.suffixes = vec![".java".into(), ".kt".into()];
    let Name::Migration { version, kind, .. } = parse(file, &naming) else { return Err(Error::Stale); };
    let module = |path: &str| path.split_once("/src/").map(|(prefix, _)| prefix.to_owned());
    let mut paths = inventory(project, config)?;
    for glob in ["**/*.java", "**/*.kt"] {
        let found = project.find_files(glob, 4097)?;
        if found.len() > 4096 { return Err("Source inventory reached its cap; version collision checking is incomplete.".into()); }
        paths.extend(found);
    }
    for path in paths {
        if module(&path).is_some() && module(target).is_some() && module(&path) != module(target) { continue; }
        let mut all = config.naming.clone(); all.suffixes.extend([".java".into(), ".kt".into()]);
        if matches!(parse(path.rsplit('/').next().unwrap_or_default(), &all), Name::Migration { version: other, kind: other_kind, .. } if other_kind == kind && version_key(&other) == version_key(&version)) {
            return Err("Another SQL/Java/Kotlin migration in this module has the same normalized version.".into());
        }
    }
    Ok(())
}
pub(super) fn resolve(plugin: &mut Flyway, doc: &Document, action: FileAction, project: &Project, expected: &str, kind: &str) -> Result<FileAction, Error> {
    let config = configuration(project)?;
    let java_callback = kind == JAVA_CALLBACK;
    let sql_callback = kind == SQL_CALLBACK;
    let kotlin = kind == KOTLIN;
    let mut events = Vec::new();
    let mut transaction = "true".to_owned();
    let class = if java_callback {
        let class = host::input("Java callback class name", Some("MigrationCallback"))?.ok_or(Error::Cancelled)?;
        if !identifier(&class) { return Err("Use a literal non-keyword Java class identifier.".into()); }
        events = callback_events(true)?;
        transaction = host::pick("Can handle callback inside a transaction?", &[PickItem::new("true", "Yes"), PickItem::new("false", "No")])?.ok_or(Error::Cancelled)?;
        if !matches!(transaction.as_str(), "true" | "false") { return Err(Error::Stale); }
        class
    } else if sql_callback {
        events = callback_events(false)?;
        let description = host::input("Optional callback description", Some(""))?.ok_or(Error::Cancelled)?;
        let description = migration_description(&description);
        format!("{}{}{}", EVENTS[events[0]].0, if description.is_empty() { "" } else { &config.naming.separator }, description)
    } else {
        plugin.begin_generation(project)?;
        let draft = plugin.generation.as_ref().ok_or(Error::Stale)?;
        let version = host::input("Java/Kotlin migration version", Some(&draft.version))?.ok_or(Error::Cancelled)?;
        if version.is_empty() || !version.split(['.', '_']).all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())) { return Err("Use numeric migration version segments.".into()); }
        let description = host::input("Migration description", None)?.ok_or(Error::Cancelled)?;
        let name = format!("{}{}{}{}", config.naming.versioned, version.replace('.', "_"), config.naming.separator, migration_description(&description));
        if migration_description(&description).is_empty() || !identifier(&name) { return Err("Configured prefix/separator and description must form a Java/Kotlin class identifier.".into()); }
        name
    };
    let default_directory = if sql_callback { plugin.next_migration(project)?.1 } else if kotlin { "src/main/kotlin".into() } else { "src/main/java".into() };
    let directory = source_directory(expected, &host::input("Source directory inside project", Some(&default_directory))?.ok_or(Error::Cancelled)?)?;
    let package_name = if sql_callback { String::new() } else { host::input("Package name (empty for default package)", Some("db.migration"))?.ok_or(Error::Cancelled)? };
    if !package(&package_name) { return Err("Use a dotted Java/Kotlin package of non-keyword identifiers.".into()); }
    let extension = if sql_callback { config.naming.suffixes.first().filter(|suffix| suffix.starts_with('.') && suffix.len() <= 24 && file_name(suffix)).ok_or("Choose a bounded literal configured SQL suffix.")?.as_str() } else if kotlin { ".kt" } else { ".java" };
    let package_path = package_name.replace('.', "/");
    let target = format!("{expected}/{directory}/{}{}{}", package_path, if package_path.is_empty() { "" } else { "/" }, format!("{class}{extension}"));
    relative(expected, &target)?;
    if sql_callback && !supported_location(expected, &target, &config.locations) { return Err("Choose a configured migration location for the SQL callback.".into()); }
    target_owned(project, expected, &target)?;
    if !java_callback && !sql_callback { version_collision(project, &config, &target)?; }
    let mut text = if sql_callback { include_str!("../../templates/callback.sql").to_owned() }
        else if java_callback { include_str!("../../templates/Callback.java").to_owned() }
        else if kotlin { include_str!("../../templates/Migration.kt").to_owned() }
        else { include_str!("../../templates/Migration.java").to_owned() };
    text = text.replace("$NAME$", &class).replace("$END$", "");
    if java_callback {
        let expression = events.iter().map(|index| format!("event == Event.{}", EVENTS[*index].1)).collect::<Vec<_>>().join(" || ");
        text = text.replace("event == Event.AFTER_MIGRATE", &expression).replace("return true;", &format!("return {transaction};"));
    }
    if !package_name.is_empty() { text = format!("package {package_name}{}\n\n{text}", if kotlin { "" } else { ";" }); }
    if !ui_kit::prompts::confirm_exact("Create Flyway source?", &format!("Create: {target}\n\n{}\nOne non-overwriting undoable workspace action. Source skeleton only; Flyway runtime/edition, classpath and callback discovery are not configured. No process or database operation runs.", ui_kit::prompts::readable_source(&text)))? { return Err(Error::Cancelled); }
    let project = context(doc, expected)?;
    source_current(&project, doc)?;
    if configuration(&project)? != config { return Err(Error::Stale); }
    target_owned(&project, expected, &target)?;
    if !java_callback && !sql_callback { version_collision(&project, &config, &target)?; }
    Ok(FileAction { title: "Create Flyway source".into(), kind: action.kind, operations: vec![FileOperation::Create { path: target.clone(), text, overwrite: false }], open: Some(target), ..FileAction::default() })
}
