//! Schema baseline and Flyway migration drafts from entity-source model changes.
//!
//! Bounds: at most `MAX_FILES` `.java` and `.kt` files each (a reached cap refuses), 128 KiB per file
//! (larger files make the snapshot incomplete), `MAX_TOTAL` bytes and `SCAN_SECONDS` of reading; files under
//! `/src/test/` are not part of the model; the host skips hidden and gitignored paths in `find_files`.
//!
//! The Flyway folder and version rule is a deliberately small subset of the Flyway plugin's conventions
//! (default `classpath:db/migration`, prefix `V`, separator `__`, suffix `.sql`); any configured locations or
//! naming refuse instead of guessing. No SQL is executed and no runner is duplicated.
//!
//! `FileOperation::Create` can carry `overwrite: true`, but an overwrite is not version-guarded (S007), so the
//! refreshed baseline is written to `db/jpa-schema-baseline.next.json` (never overwriting); the user moves it
//! over the baseline. While that file exists, generation refuses so one change set is never emitted twice.
use super::*;
use persistence_core::schema::{self, Danger, Fact, SchemaSnapshot};
use crate::schema_snapshot::{EnversConfig, Scope, ScopedSnapshot};
use persistence_core::sql::{self, BackfillValue, Dialect, EmitOptions};
use super::template_settings::{self, FlywayNaming};
use persistence_core::naming::{self, DateTime, Inputs};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const BASELINE_TITLE: &str = "Create schema baseline…";
pub(super) const MIGRATION_TITLE: &str = "Generate Flyway migration from model changes…";
pub(crate) const BASELINE: &str = "db/jpa-schema-baseline.json";
pub(crate) const BASELINE_NEXT: &str = "db/jpa-schema-baseline.next.json";
const MAX_FILES: u32 = 2_000;
const MAX_TOTAL: usize = 16 << 20;
const SCAN_SECONDS: u64 = 8;
pub(super) const MAX_MIGRATIONS: u32 = 10_000;
const MIGRATION_DIR_SUFFIX: &str = "/src/main/resources/db/migration";

pub(super) fn eligible(doc: &Document) -> bool {
    crate::owned_entity_source(doc).is_some_and(|class| class.kind == Kind::Entity)
}

pub(super) fn absolute(root: &str, file: &str) -> String {
    if file.starts_with('/') { file.to_owned() } else { format!("{}/{file}", root.trim_end_matches('/')) }
}

pub(crate) fn scan(project: &Project, root: &str) -> Result<SchemaSnapshot, Error> {
    Ok(scan_scoped(project, root, &Scope::All)?.snapshot)
}

pub(crate) fn scan_scoped(project: &Project, root: &str, scope: &Scope) -> Result<ScopedSnapshot, Error> {
    let deadline = Instant::now() + Duration::from_secs(SCAN_SECONDS);
    let mut docs = Vec::new();
    let mut total = 0usize;
    for (pattern, language) in [("**/*.java", "java"), ("**/*.kt", "kotlin")] {
        let files = project.find_files(pattern, MAX_FILES)?;
        if files.len() >= MAX_FILES as usize {
            return Err(format!("The project has {MAX_FILES} or more {language} files; the bounded model scan refuses.").into());
        }
        for file in files {
            let file = absolute(root, &file);
            if path(&file).as_deref() != Some(file.as_str()) || !inside(root, &file) {
                return Err(Error::Stale);
            }
            if file.contains("/src/test/") {
                continue;
            }
            if Instant::now() >= deadline {
                return Err("The bounded model scan timed out.".into());
            }
            let text = project.read_file(&file)?;
            total += text.len();
            if total > MAX_TOTAL {
                return Err("The bounded model scan exceeded its byte limit.".into());
            }
            docs.push(Document { path: file, language: language.into(), text, version: 0 });
        }
    }
    let mut scoped = crate::schema_snapshot::snapshot_configured(&docs, scope, &crate::naming_strategy::detect(project), &EnversConfig::detect(project));
    if super::ddl_options::index_foreign_keys_enabled() {
        super::ddl_options::index_foreign_keys(&mut scoped.snapshot);
    }
    Ok(scoped)
}

/// With several persistence units a run is per unit (JetBrains database versioning help: "A persistence unit must be
/// selected"); with one or none the whole project is the scope and nothing is asked.
pub(crate) fn choose_scope(project: &Project, root: &str) -> Result<Scope, Error> {
    let units: Vec<_> = crate::structure_related::units(project).into_iter().filter(|u| !crate::naming_strategy::skipped(&u.path)).collect();
    if units.len() < 2 {
        return Ok(Scope::All);
    }
    let items: Vec<PickItem> = units.iter().enumerate().map(|(i, u)| PickItem::new(&i.to_string(), &u.name).description(&u.path)).collect();
    let key = host::pick("Persistence unit to take the model from (one run per unit)", &items)?.ok_or(Error::Cancelled)?;
    let unit = key.parse::<usize>().ok().and_then(|i| units.get(i)).ok_or(Error::Stale)?;
    let file = absolute(root, &unit.path);
    let text = project.read_file(&file)?;
    Scope::from_persistence_xml(&file, &text, &unit.name).ok_or(Error::Stale)
}

pub(crate) fn scope_unit(scope: &Scope) -> Option<&str> {
    match scope {
        Scope::All => None,
        Scope::Unit { name, .. } => Some(name),
    }
}

/// Everything the Flyway and Liquibase actions share before their first option prompt.
pub(crate) struct Prepared {
    pub baseline_path: String,
    pub next_path: String,
    pub scope: Scope,
    /// `None`: no baseline exists and the whole model is written as an initial migration.
    pub baseline_text: Option<String>,
    pub baseline: SchemaSnapshot,
    pub model: ScopedSnapshot,
    pub plan: schema::ChangePlan,
    pub next_baseline: String,
}

impl Prepared {
    pub(crate) fn init(&self) -> bool {
        self.baseline_text.is_none()
    }

    pub(crate) fn verify_unchanged(&self, project: &Project, root: &str) -> Result<(), Error> {
        if read_optional(project, &self.baseline_path)? != self.baseline_text {
            return Err(Error::Stale);
        }
        let fresh = scan_scoped(project, root, &self.scope)?;
        if crate::schema_snapshot::scoped_json(&fresh, &self.scope).map_err(Error::from)? != self.next_baseline.trim_end() {
            return Err(Error::Stale);
        }
        Ok(())
    }
}

pub(crate) fn prepare(project: &Project, root: &str, output: &str, nothing: &str) -> Result<Prepared, Error> {
    let baseline_path = target_in_root(project, root, BASELINE)?;
    let next_path = target_in_root(project, root, BASELINE_NEXT)?;
    if project.exists(&next_path)? || read_optional(project, &next_path)?.is_some() {
        return Err(format!("{next_path} exists: move it over {baseline_path} (or delete it) before generating another {output}.").into());
    }
    let scope = choose_scope(project, root)?;
    let baseline_text = read_optional(project, &baseline_path)?;
    let baseline = match &baseline_text {
        Some(text) => {
            if crate::schema_snapshot::recorded_unit(text).as_deref() != scope_unit(&scope) {
                return Err(format!(
                    "The schema baseline was taken for {}, this run is for {}; run it for the same persistence unit.",
                    crate::schema_snapshot::recorded_unit(text).map_or("the whole project".to_owned(), |unit| format!("persistence unit {unit}")),
                    scope_unit(&scope).map_or("the whole project".to_owned(), |unit| format!("persistence unit {unit}")),
                )
                .into());
            }
            crate::schema_snapshot::snapshot_from_json(text).map_err(|e| Error::from(format!("The schema baseline is unreadable: {e}")))?
        }
        None => SchemaSnapshot::default(),
    };
    let model = scan_scoped(project, root, &scope)?;
    if baseline_text.is_none() && model.snapshot.tables.is_empty() {
        return Err("No schema baseline exists and no entity tables were found in the project sources.".into());
    }
    let plan = schema::diff(&baseline, &model.snapshot).map_err(|e| Error::from(format!("The snapshots cannot be compared: {e:?}")))?;
    if plan.changes.is_empty() && plan.notes.is_empty() {
        return Err(nothing.into());
    }
    let mut next_baseline = crate::schema_snapshot::scoped_json(&model, &scope).map_err(Error::from)?;
    next_baseline.push('\n');
    Ok(Prepared { baseline_path, next_path, scope, baseline_text, baseline, model, plan, next_baseline })
}

/// Jakarta Persistence `@Column.length` defaults to 255, so a `varchar` without a length is `varchar(255)`; Liquibase needs it written.
pub(super) fn apply_default_lengths(plan: &mut schema::ChangePlan) {
    let widen = |column: &mut schema::Column| {
        if column.length.is_none() && column.sql_type.known().is_some_and(|t| t.logical == schema::LogicalType::Varchar) {
            column.length = Some(255);
        }
    };
    for planned in &mut plan.changes {
        match &mut planned.change {
            schema::Change::AddColumn { column, .. } => widen(column),
            schema::Change::CreateTable { table } => {
                if let Fact::Known(columns) = &mut table.columns {
                    columns.iter_mut().for_each(widen);
                }
            }
            _ => {}
        }
    }
}

const TYPE_ATTEMPTS: usize = 3;
use crate::schema_snapshot::CUSTOM_TYPE_REASON;

/// Columns of a custom Java type get no automatic SQL mapping (JetBrains database versioning help); the user names the
/// target type once per column name and Java type, so an audit table's mirror column reuses the answer. Empty input
/// leaves the column unmapped, so its statement stays withheld.
pub(super) fn ask_custom_types(plan: &mut schema::ChangePlan) -> Result<(), Error> {
    let mut answers: BTreeMap<(String, String), Option<schema::Column>> = BTreeMap::new();
    let mut ask = |table: &str, column: &mut schema::Column| -> Result<(), Error> {
        let Fact::Unknown(reason) = &column.sql_type else {
            return Ok(());
        };
        let Some(java_type) = reason.strip_prefix(CUSTOM_TYPE_REASON).map(str::to_owned) else {
            return Ok(());
        };
        let key = (column.name.clone(), java_type.clone());
        if let Some(answer) = answers.get(&key) {
            if let Some(typed) = answer {
                (column.sql_type, column.length, column.precision, column.scale) = (typed.sql_type.clone(), typed.length, typed.precision, typed.scale);
            }
            return Ok(());
        }
        if let Some(mapped) = super::type_map::mapped(&java_type)? {
            sql::apply_type_text(column, &mapped).map_err(|why| Error::from(format!("sqlTypeMappings maps {java_type} to {mapped:?}, which was refused: {why}.")))?;
            answers.insert(key, Some(column.clone()));
            return Ok(());
        }
        let question = format!("SQL type for column {table}.{} (Java type {java_type}); for example varchar(255), bigint, numeric(19,2); leave empty to withhold it", column.name);
        let mut prompt = question.clone();
        for attempt in 1..=TYPE_ATTEMPTS {
            let input = host::input(&prompt, None)?.ok_or(Error::Cancelled)?;
            if input.trim().is_empty() {
                answers.insert(key, None);
                return Ok(());
            }
            match sql::apply_type_text(column, &input) {
                Ok(()) => {
                    answers.insert(key, Some(column.clone()));
                    return Ok(());
                }
                Err(why) if attempt == TYPE_ATTEMPTS => return Err(format!("The SQL type of {table}.{} was refused: {why}.", column.name).into()),
                Err(why) => prompt = format!("Refused: {why}. {question}"),
            }
        }
        Ok(())
    };
    for planned in &mut plan.changes {
        match &mut planned.change {
            schema::Change::AddColumn { table, column } => ask(table, column)?,
            schema::Change::CreateTable { table } => {
                let name = table.name.clone();
                if let Fact::Known(columns) = &mut table.columns {
                    for column in columns {
                        ask(&name, column)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn target_in_root(project: &Project, root: &str, relative: &str) -> Result<String, Error> {
    let target = format!("{}/{relative}", root.trim_end_matches('/'));
    if path(&target).as_deref() != Some(target.as_str()) || !inside(root, &target) {
        return Err(Error::Stale);
    }
    if host::project_of(&target).and_then(|p| p.root().ok()).as_deref() != Some(root) || project.root()? != root {
        return Err(Error::Stale);
    }
    Ok(target)
}

pub(crate) fn read_optional(project: &Project, file: &str) -> Result<Option<String>, Error> {
    match project.read_file(file) {
        Ok(text) => Ok(Some(text)),
        Err(Error::NotFound(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) fn absent(project: &Project, file: &str) -> Result<(), Error> {
    if project.exists(file)? || read_optional(project, file)?.is_some() {
        return Err(format!("{file} already exists or has an open buffer; nothing is overwritten.").into());
    }
    Ok(())
}

pub(super) fn acknowledge(reasons: &[String]) -> Result<(), Error> {
    let body = format!("The result is incomplete:\n- {}\n\nContinue anyway? Unknown or unproven parts are skipped, not guessed.", reasons.join("\n- "));
    if !ui_kit::prompts::confirm_exact("Incomplete schema information", &body)? {
        return Err(Error::Cancelled);
    }
    Ok(())
}

pub(super) fn resolve_baseline(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let scope = choose_scope(project, &pending.root)?;
    let scoped = scan_scoped(project, &pending.root, &scope)?;
    let snapshot = &scoped.snapshot;
    if snapshot.tables.is_empty() {
        return Err("No entity tables were found in the project sources.".into());
    }
    let json = crate::schema_snapshot::scoped_json(&scoped, &scope).map_err(Error::from)?;
    let target = target_in_root(project, &pending.root, BASELINE)?;
    absent(project, &target)?;
    let mut text = json;
    text.push('\n');
    if !snapshot.complete {
        acknowledge(&["some entity sources were not fully understood; absent tables are unproven".into()])?;
    }
    let body = format!(
        "Target: {target}\nScope: {}\nTables: {}\nComplete: {}\n\nA JSON snapshot of the entity sources (no database, no test sources, bounded scan of {MAX_FILES} files per language). The exact file is the deterministic serialization of this scan; its first lines:\n{}\n\nOne non-overwriting Create.",
        scope_unit(&scope).map_or("the whole project".to_owned(), |unit| format!("persistence unit {unit}")),
        snapshot.tables.len(),
        snapshot.complete,
        text.lines().take(12).collect::<Vec<_>>().join("\n"),
    );
    if !ui_kit::prompts::confirm_exact("Create schema baseline?", &body)? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    absent(project, &target)?;
    let fresh = scan_scoped(project, &pending.root, &scope)?;
    if crate::schema_snapshot::scoped_json(&fresh, &scope).map_err(Error::from)? != text.trim_end() {
        return Err(Error::Stale);
    }
    Ok(FileAction {
        title: "Create schema baseline".into(),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target.clone(), text, overwrite: false }],
        open: Some(target),
        ..FileAction::default()
    })
}

fn is_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 64
        && !version.starts_with(['.', '_'])
        && !version.ends_with(['.', '_'])
        && !version.contains("..")
        && !version.contains("__")
        && !version.contains("._")
        && !version.contains("_.")
        && version.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '_')
}

/// Version of `V<version>__<description>.sql`, or `None`.
#[cfg(test)]
fn versioned(name: &str) -> Option<&str> {
    versioned_ext(name, ".sql")
}

pub(super) fn versioned_ext<'a>(name: &'a str, suffix: &str) -> Option<&'a str> {
    versioned_named(name, "V", "__", suffix)
}

fn versioned_named<'a>(name: &'a str, prefix: &str, separator: &str, suffix: &str) -> Option<&'a str> {
    let (version, description) = name.strip_prefix(prefix)?.strip_suffix(suffix)?.split_once(separator)?;
    (is_version(version) && !description.is_empty()).then_some(version)
}

pub(super) fn version_key(version: &str) -> Vec<(usize, String)> {
    let mut parts: Vec<String> = version
        .split(['.', '_'])
        .map(|part| match part.trim_start_matches('0') {
            "" => "0".to_owned(),
            trimmed => trimmed.to_owned(),
        })
        .collect();
    while parts.last().is_some_and(|part| part == "0") {
        parts.pop();
    }
    parts.into_iter().map(|part| (part.len(), part)).collect()
}

/// Same rule as the Flyway plugin's `next_version`: bump the last numeric part of the highest version.
pub(super) fn next_version(versions: &[&str]) -> String {
    let Some(highest) = versions.iter().max_by_key(|version| version_key(version)) else {
        return "1".into();
    };
    let (head, last) = highest.split_at(highest.rfind(['.', '_']).map_or(0, |i| i + 1));
    let mut digits = last.as_bytes().to_vec();
    let mut carry = true;
    for digit in digits.iter_mut().rev() {
        if *digit == b'9' {
            *digit = b'0';
        } else {
            *digit += 1;
            carry = false;
            break;
        }
    }
    if carry {
        digits.insert(0, b'1');
    }
    format!("{head}{}", String::from_utf8(digits).expect("ascii digits"))
}

pub(super) fn custom_flyway_settings(project: &Project) -> Result<bool, Error> {
    let mut texts = Vec::new();
    for file in ["flyway.toml", "flyway.conf", "conf/flyway.toml", "conf/flyway.conf"] {
        if let Some(text) = read_optional(project, file)? {
            texts.push((true, text));
        }
    }
    let spring = project.find_files("**/src/main/resources/application*", 100)?;
    if spring.len() >= 100 {
        return Err("Flyway configuration discovery reached its limit.".into());
    }
    for file in spring {
        texts.push((false, project.read_file(&file)?));
    }
    Ok(texts.iter().any(|(is_flyway_file, text)| {
        let lower = text.to_ascii_lowercase();
        (*is_flyway_file || lower.contains("flyway"))
            && ["locations", "sqlmigrationprefix", "sqlmigrationseparator", "sqlmigrationsuffixes"].iter().any(|key| lower.contains(key))
    }))
}

pub(super) struct Folder {
    pub(super) directory: String,
    pub(super) version: String,
    pub(super) names: Vec<String>,
    /// Versions of the versioned SQL and Java migrations of the module; Flyway rejects two migrations of one version.
    pub(super) versions: Vec<String>,
}

pub(super) const JAVA_DIR_SUFFIX: &str = "/src/main/java/db/migration";

/// `(directory, version)` of every existing Java migration (`V<version>__<description>.java` extending a
/// `JavaMigration` type, outside `src/test`), refusing one that is not in the default location.
pub(super) fn java_migrations(project: &Project, root: &str) -> Result<Vec<(String, String)>, Error> {
    let java = project.find_files("**/V*__*.java", MAX_MIGRATIONS)?;
    if java.len() >= MAX_MIGRATIONS as usize {
        return Err("Migration discovery reached its limit.".into());
    }
    let mut found = Vec::new();
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
        found.push((directory.to_owned(), version.to_owned()));
    }
    Ok(found)
}

pub(super) fn flyway_folder(project: &Project, root: &str) -> Result<Folder, Error> {
    flyway_folder_with(project, root, &FlywayNaming::default(), None)
}

/// `module_fallback`: the module an initial migration goes into when the project has no migration at all.
fn flyway_folder_with(project: &Project, root: &str, naming: &FlywayNaming, module_fallback: Option<&str>) -> Result<Folder, Error> {
    let versioned = |name: &str| versioned_named(name, &naming.prefix, &naming.separator, ".sql").map(str::to_owned);
    if custom_flyway_settings(project)? {
        return Err("Flyway locations or naming are configured; only the default classpath:db/migration with V__.sql names is supported, and nothing is guessed.".into());
    }
    let files = project.find_files("**/src/main/resources/db/migration/**/*", MAX_MIGRATIONS)?;
    if files.len() >= MAX_MIGRATIONS as usize {
        return Err("Migration discovery reached its limit.".into());
    }
    let mut folders: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in files.iter().map(|f| absolute(root, f)).filter(|f| f.ends_with(".sql")) {
        let (directory, name) = file.rsplit_once('/').ok_or(Error::Stale)?;
        if !directory.ends_with(MIGRATION_DIR_SUFFIX) {
            return Err("Vendor or nested migration folders are not supported; the migration folder is ambiguous.".into());
        }
        folders.entry(directory.to_owned()).or_default().push(name.to_owned());
    }
    let java = java_migrations(project, root)?;
    let mut with_versions = folders.into_iter().filter(|(_, names)| names.iter().any(|n| versioned(n).is_some()));
    let (directory, names) = match with_versions.next() {
        Some(found) => found,
        None => {
            let modules: BTreeSet<&str> = java.iter().filter_map(|(dir, _)| dir.strip_suffix(JAVA_DIR_SUFFIX)).collect();
            let mut modules = modules.into_iter();
            let (Some(module), None) = (modules.next(), modules.next()) else {
                if let (true, Some(module)) = (java.is_empty(), module_fallback) {
                    return Ok(Folder { directory: format!("{module}{MIGRATION_DIR_SUFFIX}"), version: "1".into(), names: Vec::new(), versions: Vec::new() });
                }
                return Err("No existing <prefix><version><separator><description>.sql or Java migration (V<version>__<description> by default) was found; the folder and version cannot be determined.".into());
            };
            (format!("{module}{MIGRATION_DIR_SUFFIX}"), Vec::new())
        }
    };
    if with_versions.next().is_some() {
        return Err("Several modules contain versioned migrations; the migration folder is ambiguous.".into());
    }
    if path(&directory).as_deref() != Some(directory.as_str()) || !inside(root, &directory) {
        return Err(Error::Stale);
    }
    let module_java = format!("{}{JAVA_DIR_SUFFIX}", directory.strip_suffix(MIGRATION_DIR_SUFFIX).unwrap_or_default());
    let mut versions: Vec<String> = names.iter().filter_map(|n| versioned(n)).collect();
    versions.extend(java.into_iter().filter(|(dir, _)| *dir == module_java).map(|(_, version)| version));
    let version = next_version(&versions.iter().map(String::as_str).collect::<Vec<_>>());
    Ok(Folder { directory, version, names, versions })
}

pub(crate) fn description_of(text: &str) -> String {
    let words: Vec<&str> = text.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    words.join("_").chars().take(100).collect()
}

/// The version of the new file: the convention's next version, or the configured `flywayVersionPattern` with
/// `{seq}` standing for that convention version. A patterned version must be a Flyway version and sort after
/// every existing one; `#increment` needs every existing version to be a plain number.
fn chosen_version(folder: &Folder, naming: &FlywayNaming, description: &str, now: Option<DateTime>, project: &Project) -> Result<String, Error> {
    let Some((pattern, needs)) = &naming.version_pattern else {
        return Ok(folder.version.clone());
    };
    let existing: Vec<&str> = folder.versions.iter().map(String::as_str).collect();
    let mut highest = None;
    if needs.increment {
        for version in &existing {
            let plain = version.len() <= 18 && version.bytes().all(|b| b.is_ascii_digit());
            let number: u64 = plain.then(|| version.parse().ok()).flatten().ok_or("#increment needs every existing version to be a plain number; nothing is guessed.")?;
            highest = highest.max(Some(number));
        }
    }
    let sem_ver = if needs.sem_ver { Some(template_settings::project_version(project)?) } else { None };
    let inputs = Inputs { seq: Some(&folder.version), highest, description: Some(description), now, sem_ver: sem_ver.as_deref() };
    let version = naming::expand(pattern, &inputs).map_err(|e| Error::from(format!("flywayVersionPattern: {e}.")))?;
    if !is_version(&version) {
        return Err(format!("flywayVersionPattern produced {version:?}, which is not a Flyway version (digits separated by '.' or '_').").into());
    }
    if existing.iter().any(|top| version_key(&version) <= version_key(top)) {
        return Err(format!("The version {version} would not sort after every existing migration; nothing is written.").into());
    }
    Ok(version)
}

pub(super) fn summary(plan: &schema::ChangePlan) -> String {
    let count = |danger| plan.changes.iter().filter(|c| c.danger == danger).count();
    format!(
        "{} changes (Safe {}, Rewrite {}, NeedsBackfill {}, DataLoss {}, Ambiguous {}), {} notes",
        plan.changes.len(),
        count(Danger::Safe),
        count(Danger::Rewrite),
        count(Danger::NeedsBackfill),
        count(Danger::DataLoss),
        count(Danger::Ambiguous),
        plan.notes.len()
    )
}

const BACKFILL_ATTEMPTS: usize = 3;

/// Asks for a typed backfill value per `NeedsBackfill` change when that danger is included; empty input means none.
pub(super) fn ask_backfills(plan: &schema::ChangePlan, options: &mut EmitOptions) -> Result<(), Error> {
    if !options.allow.contains(&Danger::NeedsBackfill) {
        return Ok(());
    }
    for target in sql::backfill_targets(plan, &options.renames) {
        let Some(column) = &target.column_type else {
            continue;
        };
        let Some(logical) = column.sql_type.known().map(|t| t.logical) else {
            continue;
        };
        if !BackfillValue::supports(logical) {
            continue;
        }
        let question = format!("Backfill value for {}.{} ({logical:?}); leave empty for none", target.table, target.column);
        let mut prompt = question.clone();
        for attempt in 1..=BACKFILL_ATTEMPTS {
            let input = host::input(&prompt, None)?.ok_or(Error::Cancelled)?;
            if input.trim().is_empty() {
                break;
            }
            match BackfillValue::parse(&input, logical).and_then(|value| value.check(column).map(|()| value)) {
                Ok(value) => {
                    options.backfills.push(sql::Backfill { table: target.table.clone(), column: target.column.clone(), value });
                    break;
                }
                Err(why) if attempt == BACKFILL_ATTEMPTS => {
                    return Err(format!("The backfill value for {}.{} was refused: {why}.", target.table, target.column).into());
                }
                Err(why) => prompt = format!("Refused: {why}. {question}"),
            }
        }
    }
    Ok(())
}

/// The module of an entity source (`<module>/src/main/java|kotlin/...`), where an initial migration goes if the project has none.
fn module_of(file: &str) -> Option<&str> {
    ["/src/main/java/", "/src/main/kotlin/"].iter().find_map(|marker| file.find(marker).map(|at| &file[..at]))
}

pub(super) fn resolve_migration(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let root = pending.root.as_str();
    let mut prepared = prepare(project, root, "migration", "The model matches the baseline; there is nothing to migrate.")?;
    let init = prepared.init();
    let (baseline_path, next_path) = (prepared.baseline_path.clone(), prepared.next_path.clone());

    let mut dialects = [PickItem::new("postgresql", "PostgreSQL"), PickItem::new("mysql", "MySQL")];
    crate::datasource_facts::prefer(project, &mut dialects);
    let dialect = match host::pick("SQL dialect (no database connection)", &dialects)?.ok_or(Error::Cancelled)?.as_str() {
        "postgresql" => Dialect::PostgreSql,
        "mysql" => Dialect::MySql,
        _ => return Err(Error::Stale),
    };
    let withheld = [
        PickItem::new("needs-backfill", "NeedsBackfill: NOT NULL without default or value"),
        PickItem::new("data-loss", "DataLoss: drops and narrowing changes"),
    ];
    let picked = host::pick_many("Also emit statements withheld by default (each is marked in the preview)", &withheld)?.ok_or(Error::Cancelled)?;
    let mut options = EmitOptions::default();
    for key in &picked {
        options.allow.push(match key.as_str() {
            "needs-backfill" => Danger::NeedsBackfill,
            "data-loss" => Danger::DataLoss,
            _ => return Err(Error::Stale),
        });
    }
    ask_custom_types(&mut prepared.plan)?;
    let (plan, model, baseline) = (&prepared.plan, &prepared.model.snapshot, &prepared.baseline);
    ask_backfills(plan, &mut options)?;
    let description = host::input("Migration description (ASCII letters and digits)", Some(if init { "init" } else { "jpa_model_changes" }))?.ok_or(Error::Cancelled)?;
    let description = naming::style_description(&description, template_settings::description_style()?);
    if description.is_empty() {
        return Err("Enter a description containing ASCII letters or digits.".into());
    }
    let flyway_naming = template_settings::flyway_naming()?;
    let now = match &flyway_naming.version_pattern {
        Some((_, needs)) => template_settings::ask_date(needs)?,
        None => None,
    };

    let emitted = sql::emit(plan, dialect, &options).map_err(|e| Error::from(format!("SQL cannot be generated: {e:?}")))?;
    if emitted.emitted() == 0 {
        return Err("Every change is withheld or unsupported; there is no SQL to write. Adjust the options or the model.".into());
    }
    let mut incomplete = Vec::new();
    if !baseline.complete {
        incomplete.push("the baseline snapshot is incomplete".to_owned());
    }
    if !model.complete {
        incomplete.push("the current model scan is incomplete".to_owned());
    }
    if !plan.notes.is_empty() {
        incomplete.push(format!("{} facts could not be compared (see NOTE lines)", plan.notes.len()));
    }
    if emitted.unsupported() > 0 {
        incomplete.push(format!("{} statements were skipped as unsupported", emitted.unsupported()));
    }
    if !incomplete.is_empty() {
        acknowledge(&incomplete)?;
    }

    let fallback = init.then(|| module_of(&doc.path)).flatten().filter(|module| inside(root, module));
    let folder = flyway_folder_with(project, root, &flyway_naming, fallback)?;
    let version = chosen_version(&folder, &flyway_naming, &description, now, project)?;
    let name = format!("{}{version}{}{description}.sql", flyway_naming.prefix, flyway_naming.separator);
    if folder.names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
        return Err(Error::Stale);
    }
    let target = format!("{}/{name}", folder.directory);
    if path(&target).as_deref() != Some(target.as_str()) || !inside(root, &target) {
        return Err(Error::Stale);
    }
    absent(project, &target)?;
    let sql_text = emitted.render();
    let next_baseline = prepared.next_baseline.clone();

    let kind = if init {
        "No baseline exists: this is an INIT migration, a DDL script of the whole model (every table as CREATE TABLE).\n"
    } else {
        ""
    };
    let preview = format!(
        "{kind}Target: {target}\nDialect: {dialect:?}\nPlan: {}\nStatements: {} emitted, {} withheld by policy, {} unsupported\n\nExact SQL file:\n{sql_text}\nAlso creates {next_path} ({} tables, deterministic serialization of this scan). The plugin never overwrites files: afterwards move it over {baseline_path} to {} the baseline; until then this action refuses. SQL is never executed.",
        summary(plan),
        emitted.emitted(),
        emitted.policy_skipped(),
        emitted.unsupported(),
        model.tables.len(),
        if init { "make it" } else { "refresh" },
    );
    if !ui_kit::prompts::confirm_exact("Create Flyway migration?", &preview)? {
        return Err(Error::Cancelled);
    }

    current(pending, doc)?;
    prepared.verify_unchanged(project, root)?;
    let again = flyway_folder_with(project, root, &flyway_naming, fallback)?;
    if again.directory != folder.directory || again.version != folder.version {
        return Err(Error::Stale);
    }
    absent(project, &target)?;
    absent(project, &next_path)?;
    Ok(FileAction {
        title: format!("Create Flyway migration {name}"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![
            FileOperation::Create { path: target.clone(), text: sql_text, overwrite: false },
            FileOperation::Create { path: next_path, text: next_baseline, overwrite: false },
        ],
        open: Some(target),
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ENTITY: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\npublic class Customer {\n    @Id Long id;\n    String name;\n}\n";
    const ENTITY_PLUS: &str = "package shop;\nimport jakarta.persistence.Column;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\npublic class Customer {\n    @Id Long id;\n    String name;\n    @Column(length = 100) String email;\n}\n";
    const PATH: &str = "/project/src/main/java/shop/Customer.java";
    const MIGRATIONS: &str = "/project/src/main/resources/db/migration";

    fn baseline_json(source: &str) -> String {
        let doc = Document { path: PATH.into(), language: "java".into(), text: source.into(), version: 0 };
        let snapshot = crate::schema_snapshot::snapshot_from_entities(&[doc]);
        assert_eq!(snapshot.tables.len(), 1);
        format!("{}\n", crate::schema_snapshot::snapshot_json(&snapshot).unwrap())
    }

    fn setup(source: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(PATH, "java", source, &[]);
        host.add_file("src/main/java/shop/Customer.java", source);
        let doc = Document { path: PATH.into(), language: "java".into(), text: source.into(), version: 1 };
        (host, JpaAssist::default(), doc)
    }

    fn migration_ready(source: &str) -> (MockHost, JpaAssist, Document) {
        let (host, plugin, doc) = setup(source);
        host.add_file("db/jpa-schema-baseline.json", &baseline_json(ENTITY));
        host.add_file("src/main/resources/db/migration/V2__init.sql", "select 1;");
        host.add_file("src/main/resources/db/migration/V1__first.sql", "select 1;");
        (host, plugin, doc)
    }

    fn action(plugin: &mut JpaAssist, doc: &Document, title: &str) -> FileAction {
        let action = plugin.file_actions(doc, Range::default()).into_iter().find(|a| a.title == title).unwrap();
        assert!(action.operations.is_empty() && action.edits.is_empty());
        action
    }

    fn generate(host: &MockHost, plugin: &mut JpaAssist, doc: &Document, dialect: &str, allow: &[&str], confirms: &[bool]) -> Result<FileAction, Error> {
        let pending = action(plugin, doc, MIGRATION_TITLE);
        host.answer_next_prompt(Some(&[dialect]));
        host.answer_next_prompt(Some(allow));
        host.answer_next_prompt(Some(&["Add customer email!"]));
        for confirm in confirms {
            host.answer_next_prompt(confirm.then_some(&[]));
        }
        plugin.resolve_file_action(doc, pending)
    }

    fn generate_named(host: &MockHost, plugin: &mut JpaAssist, doc: &Document, extra: &[&str]) -> Result<FileAction, Error> {
        let pending = action(plugin, doc, MIGRATION_TITLE);
        host.answer_next_prompt(Some(&["postgresql"]));
        host.answer_next_prompt(Some(&[]));
        host.answer_next_prompt(Some(&["Add customer email!"]));
        for answer in extra {
            host.answer_next_prompt(Some(&[answer]));
        }
        host.answer_next_prompt(Some(&[]));
        plugin.resolve_file_action(doc, pending)
    }

    fn created(resolved: Result<FileAction, Error>) -> String {
        let resolved = resolved.unwrap();
        let FileOperation::Create { path, .. } = &resolved.operations[0] else { panic!() };
        path.rsplit('/').next().unwrap().to_owned()
    }

    #[test]
    fn default_flyway_naming_settings_equal_the_unset_behavior() {
        let (host, mut plugin, doc) = migration_ready(ENTITY_PLUS);
        host.set_setting("flywayMigrationPrefix", "\"V\"");
        host.set_setting("flywayMigrationSeparator", "\"__\"");
        host.set_setting("flywayVersionPattern", "\"\"");
        host.set_setting("migrationDescriptionCase", "\"keep\"");
        assert_eq!(created(generate_named(&host, &mut plugin, &doc, &[])), "V3__Add_customer_email.sql");
    }

    #[test]
    fn flyway_version_patterns_description_case_prefix_and_separator() {
        let (host, mut plugin, doc) = migration_ready(ENTITY_PLUS);
        host.set_setting("flywayVersionPattern", "\"#date(yyyyMMddHHmmss)\"");
        host.set_setting("migrationDescriptionCase", "\"kebab\"");
        assert_eq!(created(generate_named(&host, &mut plugin, &doc, &["20250309040506"])), "V20250309040506__add-customer-email.sql");

        let (host, mut plugin, doc) = migration_ready(ENTITY_PLUS);
        host.set_setting("flywayVersionPattern", "\"#increment(100, 10, 000)\"");
        assert_eq!(created(generate_named(&host, &mut plugin, &doc, &[])), "V012__Add_customer_email.sql");

        let (host, mut plugin, doc) = migration_ready(ENTITY_PLUS);
        host.set_setting("flywayVersionPattern", "\"{seq}.1\"");
        host.set_setting("migrationDescriptionCase", "\"lower_snake\"");
        assert_eq!(created(generate_named(&host, &mut plugin, &doc, &[])), "V3.1__add_customer_email.sql");

        let (host, mut plugin, doc) = migration_ready(ENTITY_PLUS);
        host.add_file("build.gradle.kts", "version = \"4.7.1-SNAPSHOT\"\n");
        host.set_setting("flywayVersionPattern", "\"${semVer.getMajor()}_${semVer.getMinor()}_${semVer.getPatch()}\"");
        assert_eq!(created(generate_named(&host, &mut plugin, &doc, &[])), "V4_7_1__Add_customer_email.sql");

        let (host, mut plugin, doc) = setup(ENTITY_PLUS);
        host.add_file("db/jpa-schema-baseline.json", &baseline_json(ENTITY));
        host.add_file("src/main/resources/db/migration/U5___first.sql", "select 1;");
        host.set_setting("flywayMigrationPrefix", "\"U\"");
        host.set_setting("flywayMigrationSeparator", "\"___\"");
        assert_eq!(created(generate_named(&host, &mut plugin, &doc, &[])), "U6___Add_customer_email.sql");
    }

    #[test]
    fn flyway_naming_refusals() {
        let refused = |settings: &[(&str, &str)], extra: &[&str], files: &[(&str, &str)]| {
            let (host, mut plugin, doc) = migration_ready(ENTITY_PLUS);
            for (key, value) in settings {
                host.set_setting(key, value);
            }
            for (file, text) in files {
                host.add_file(file, text);
            }
            generate_named(&host, &mut plugin, &doc, extra).is_err()
        };
        assert!(refused(&[("flywayVersionPattern", "\"{nope}\"")], &[], &[]), "unknown token");
        assert!(refused(&[("flywayVersionPattern", "\"#date(yyyyMMdd)\"")], &["2025-03-09"], &[]), "bad date answer");
        assert!(refused(&[("flywayVersionPattern", "\"#date(yyyyMMdd)\"")], &["20250309"], &[("src/main/resources/db/migration/V20260101__later.sql", "x")]), "sorts before existing");
        assert!(refused(&[("flywayVersionPattern", "\"x{seq}\"")], &[], &[]), "not a flyway version");
        assert!(refused(&[("flywayVersionPattern", "\"#increment(1,1,0)\"")], &[], &[("src/main/resources/db/migration/V1_5__dotted.sql", "x")]), "increment over non-plain versions");
        assert!(refused(&[("flywayVersionPattern", "\"${semVer.getMajor()}\"")], &[], &[]), "no project version");
        assert!(refused(&[("flywayMigrationPrefix", "\"R\"")], &[], &[]));
        assert!(refused(&[("flywayMigrationSeparator", "\"_\"")], &[], &[]));
        assert!(refused(&[("migrationDescriptionCase", "\"loud\"")], &[], &[]));
    }

    #[test]
    fn baseline_is_one_nonoverwriting_create_of_the_snapshot() {
        let (host, mut plugin, doc) = setup(ENTITY);
        let pending = action(&mut plugin, &doc, BASELINE_TITLE);
        host.answer_next_prompt(Some(&[]));
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let [FileOperation::Create { path, text, overwrite: false }] = resolved.operations.as_slice() else { panic!("one create") };
        assert_eq!(path, "/project/db/jpa-schema-baseline.json");
        assert_eq!(text, &baseline_json(ENTITY));
        assert_eq!(resolved.open.as_ref(), Some(path));
        assert!(resolved.edits.is_empty() && host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn baseline_refuses_overwrite_cancel_and_no_entities() {
        let (host, mut plugin, doc) = setup(ENTITY);
        host.add_file("db/jpa-schema-baseline.json", "{}");
        let pending = action(&mut plugin, &doc, BASELINE_TITLE);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());

        let (host, mut plugin, doc) = setup(ENTITY);
        let pending = action(&mut plugin, &doc, BASELINE_TITLE);
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)));

        let (_host, mut plugin, doc) = setup(ENTITY);
        let pending = action(&mut plugin, &doc, BASELINE_TITLE);
        let changed = Document { version: 2, ..doc.clone() };
        assert!(matches!(plugin.resolve_file_action(&changed, pending), Err(Error::Stale)));
    }

    #[test]
    fn actions_are_offered_only_for_entities() {
        let (_host, mut plugin, doc) = setup(ENTITY);
        let titles: Vec<_> = plugin.file_actions(&doc, Range::default()).into_iter().map(|a| a.title).collect();
        assert!(titles.iter().any(|t| t == BASELINE_TITLE) && titles.iter().any(|t| t == MIGRATION_TITLE));
        let plain = Document { text: "package shop;\nclass Plain {}\n".into(), ..doc };
        assert!(!plugin.file_actions(&plain, Range::default()).iter().any(|a| a.title == BASELINE_TITLE));
    }

    #[test]
    fn migration_per_dialect_uses_next_flyway_version_and_stages_the_next_baseline() {
        for (dialect, quote) in [("postgresql", '"'), ("mysql", '`')] {
            let (host, mut plugin, doc) = migration_ready(ENTITY_PLUS);
            let resolved = generate(&host, &mut plugin, &doc, dialect, &[], &[true]).unwrap();
            let [FileOperation::Create { path, text, overwrite: false }, FileOperation::Create { path: next, text: next_text, overwrite: false }] =
                resolved.operations.as_slice()
            else {
                panic!("two creates")
            };
            assert_eq!(path, &format!("{MIGRATIONS}/V3__Add_customer_email.sql"));
            assert_eq!(next, "/project/db/jpa-schema-baseline.next.json");
            assert_eq!(next_text, &baseline_json(ENTITY_PLUS));
            assert!(text.contains(&format!("ALTER TABLE {quote}customer{quote} ADD COLUMN {quote}email{quote} varchar(100);")), "{text}");
            assert!(text.contains("-- [Safe]"));
            assert!(host.process_runs().is_empty() && host.requests().is_empty());
        }
    }

    const ENTITY_REQUIRED: &str = "package shop;\nimport jakarta.persistence.Column;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\npublic class Customer {\n    @Id Long id;\n    String name;\n    @Column(nullable = false, length = 5) String email;\n}\n";

    #[test]
    fn needs_backfill_prompts_for_a_typed_value_and_emits_the_safe_sequence() {
        let (host, mut plugin, doc) = migration_ready(ENTITY_REQUIRED);
        let pending = action(&mut plugin, &doc, MIGRATION_TITLE);
        host.answer_next_prompt(Some(&["postgresql"]));
        host.answer_next_prompt(Some(&["needs-backfill"]));
        host.answer_next_prompt(Some(&["much too long"]));
        host.answer_next_prompt(Some(&["it's"]));
        host.answer_next_prompt(Some(&["Add customer email!"]));
        host.answer_next_prompt(Some(&[]));
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let FileOperation::Create { text, .. } = &resolved.operations[0] else { panic!() };
        assert!(text.contains("ADD COLUMN \"email\" varchar(5);"), "{text}");
        assert!(text.contains("UPDATE \"customer\" SET \"email\" = 'it''s' WHERE \"email\" IS NULL;"), "{text}");
        assert!(text.contains("ALTER COLUMN \"email\" SET NOT NULL;"), "{text}");
    }

    #[test]
    fn empty_backfill_input_keeps_the_review_skip_and_no_prompt_without_needs_backfill() {
        let (host, mut plugin, doc) = migration_ready(ENTITY_REQUIRED);
        let pending = action(&mut plugin, &doc, MIGRATION_TITLE);
        host.answer_next_prompt(Some(&["postgresql"]));
        host.answer_next_prompt(Some(&["needs-backfill"]));
        host.answer_next_prompt(Some(&[""]));
        host.answer_next_prompt(Some(&["Add customer email!"]));
        host.answer_next_prompt(Some(&[]));
        assert!(plugin.resolve_file_action(&doc, pending).is_err(), "an empty value keeps the add withheld or unsupported: no SQL");

        let (host, mut plugin, doc) = migration_ready(ENTITY_REQUIRED);
        let resolved = generate(&host, &mut plugin, &doc, "postgresql", &[], &[true]);
        assert!(resolved.is_err(), "only a withheld NeedsBackfill add: no SQL");
    }

    #[test]
    fn three_refused_backfill_values_end_the_action() {
        let (host, mut plugin, doc) = migration_ready(ENTITY_REQUIRED);
        let pending = action(&mut plugin, &doc, MIGRATION_TITLE);
        host.answer_next_prompt(Some(&["postgresql"]));
        host.answer_next_prompt(Some(&["needs-backfill"]));
        for _ in 0..3 {
            host.answer_next_prompt(Some(&["line\nbreak"]));
        }
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
    }

    #[test]
    fn withheld_statements_need_an_explicit_choice() {
        let wider = baseline_json(ENTITY_PLUS);
        let (host, mut plugin, doc) = migration_ready(ENTITY);
        host.add_file("db/jpa-schema-baseline.json", &wider);
        assert!(generate(&host, &mut plugin, &doc, "postgresql", &[], &[true]).is_err(), "only a withheld drop: no SQL");

        let (host, mut plugin, doc) = migration_ready(ENTITY);
        host.add_file("db/jpa-schema-baseline.json", &wider);
        let resolved = generate(&host, &mut plugin, &doc, "postgresql", &["data-loss"], &[true]).unwrap();
        let FileOperation::Create { text, .. } = &resolved.operations[0] else { panic!() };
        assert!(text.contains("\nALTER TABLE \"customer\" DROP COLUMN \"email\";"), "{text}");
    }

    const TWO_UNITS: &str = "<persistence xmlns=\"https://jakarta.ee/xml/ns/persistence\">\n<persistence-unit name=\"a\"><class>shop.Customer</class><exclude-unlisted-classes>true</exclude-unlisted-classes></persistence-unit>\n<persistence-unit name=\"b\"><class>shop.Other</class><exclude-unlisted-classes>true</exclude-unlisted-classes></persistence-unit>\n</persistence>\n";
    const UNIT_A: &str = "0";

    #[test]
    fn several_persistence_units_ask_which_one_and_record_it_in_the_baseline() {
        let (host, mut plugin, doc) = setup(ENTITY);
        host.add_file("src/main/resources/META-INF/persistence.xml", TWO_UNITS);
        let pending = action(&mut plugin, &doc, BASELINE_TITLE);
        host.answer_next_prompt(Some(&[UNIT_A]));
        host.answer_next_prompt(Some(&[]));
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let FileOperation::Create { text, .. } = &resolved.operations[0] else { panic!() };
        assert!(text.contains("\"kind\": \"unit\"") && text.contains("\"name\": \"a\""), "{text}");
        assert!(text.contains("\"name\": \"customer\""), "{text}");

        let (host, mut plugin, doc) = setup(ENTITY);
        host.add_file("src/main/resources/META-INF/persistence.xml", &TWO_UNITS.replace("<persistence-unit name=\"b\"><class>shop.Other</class><exclude-unlisted-classes>true</exclude-unlisted-classes></persistence-unit>", ""));
        let pending = action(&mut plugin, &doc, BASELINE_TITLE);
        host.answer_next_prompt(Some(&[]));
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let FileOperation::Create { text, .. } = &resolved.operations[0] else { panic!() };
        assert_eq!(text, &baseline_json(ENTITY), "a single unit asks nothing and keeps the plain baseline");
    }

    #[test]
    fn a_baseline_of_another_scope_is_refused_by_the_migration() {
        let (host, mut plugin, doc) = migration_ready(ENTITY_PLUS);
        host.add_file("src/main/resources/META-INF/persistence.xml", TWO_UNITS);
        let pending = action(&mut plugin, &doc, MIGRATION_TITLE);
        host.answer_next_prompt(Some(&[UNIT_A]));
        let err = plugin.resolve_file_action(&doc, pending).unwrap_err();
        assert!(format!("{err:?}").contains("same persistence unit"), "{err:?}");
    }

    #[test]
    fn a_custom_java_type_column_asks_for_its_sql_type() {
        let entity_money = ENTITY_PLUS.replace("String name;", "String name;\n    Money price;");
        let (host, mut plugin, doc) = migration_ready(&entity_money);
        let pending = action(&mut plugin, &doc, MIGRATION_TITLE);
        host.answer_next_prompt(Some(&["postgresql"]));
        host.answer_next_prompt(Some(&[]));
        host.answer_next_prompt(Some(&["numeric(19,2)"]));
        host.answer_next_prompt(Some(&["Add price"]));
        host.answer_next_prompt(Some(&[]));
        host.answer_next_prompt(Some(&[]));
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let FileOperation::Create { text, .. } = &resolved.operations[0] else { panic!() };
        assert!(text.contains("ALTER TABLE \"customer\" ADD COLUMN \"price\" numeric(19,2);"), "{text}");

        let (host, mut plugin, doc) = migration_ready(&entity_money);
        let pending = action(&mut plugin, &doc, MIGRATION_TITLE);
        host.answer_next_prompt(Some(&["postgresql"]));
        host.answer_next_prompt(Some(&[]));
        for refused in ["bigint(x)", "bigint(x)", "bigint(x)"] {
            host.answer_next_prompt(Some(&[refused]));
        }
        let err = plugin.resolve_file_action(&doc, pending).unwrap_err();
        assert!(format!("{err:?}").contains("customer.price"), "{err:?}");
    }

const HIBERNATE_IMPORTS: &str = "import jakarta.persistence.Convert;\nimport java.time.OffsetDateTime;\nimport org.hibernate.annotations.JavaType;\nimport org.hibernate.annotations.JdbcType;\nimport org.hibernate.annotations.JdbcTypeCode;\nimport org.hibernate.annotations.TimeZoneColumn;\nimport org.hibernate.annotations.TimeZoneStorage;\nimport org.hibernate.annotations.TimeZoneStorageType;\nimport org.hibernate.annotations.Type;\nimport org.hibernate.type.SqlTypes;\n";

fn order_with(host: &MockHost, field: &str) {
        let source = format!("package shop;\n{HIBERNATE_IMPORTS}import jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\npublic class Order {{\n    @Id Long id;\n    {field}\n}}\n");
        host.add_file("src/main/java/shop/Order.java", &source);
    }

    fn order_migration(field: &str, mapping: Option<&str>, typed: Option<&str>) -> String {
        let (host, mut plugin, doc) = migration_ready(ENTITY);
        order_with(&host, field);
        if let Some(mapping) = mapping {
            host.set_setting("sqlTypeMappings", &format!("[{mapping}]"));
        }
        let pending = action(&mut plugin, &doc, MIGRATION_TITLE);
        host.answer_next_prompt(Some(&["postgresql"]));
        host.answer_next_prompt(Some(&[]));
        if let Some(typed) = typed {
            host.answer_next_prompt(Some(&[typed]));
        }
        host.answer_next_prompt(Some(&["Add order"]));
        for _ in 0..3 {
            host.answer_next_prompt(Some(&[]));
        }
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap_or_else(|e| panic!("{field}: {e:?}"));
        let FileOperation::Create { text, .. } = &resolved.operations[0] else { panic!() };
        text.clone()
    }

    #[test]
    fn an_unknown_sql_type_name_passes_through_and_clob_is_generic() {
        let entity_money = ENTITY_PLUS.replace("String name;", "String name;\n    Money price;");
        for (typed, expected) in [("geometry(Point,4326)", "\"price\" geometry(Point,4326);"), ("clob", "\"price\" text;")] {
            let (host, mut plugin, doc) = migration_ready(&entity_money);
            let pending = action(&mut plugin, &doc, MIGRATION_TITLE);
            host.answer_next_prompt(Some(&["postgresql"]));
            host.answer_next_prompt(Some(&[]));
            host.answer_next_prompt(Some(&[typed]));
            host.answer_next_prompt(Some(&["Add price"]));
            host.answer_next_prompt(Some(&[]));
            host.answer_next_prompt(Some(&[]));
            let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
            let FileOperation::Create { text, .. } = &resolved.operations[0] else { panic!() };
            assert!(text.contains(expected), "{text}");
        }
    }

    #[test]
    fn sql_type_mappings_setting_answers_the_prompt_for_types_converters_and_hibernate_types() {
        for (field, mapping) in [
            ("Money price;", r#"{"type":"shop.Money","sqlType":"numeric(19,2)"}"#),
            ("@Convert(converter = MoneyConverter.class) String price;", r#"{"type":"MoneyConverter","sqlType":"numeric(19,2)"}"#),
            ("@Type(MoneyType.class) String price;", r#"{"type":"MoneyType","sqlType":"numeric(19,2)"}"#),
            ("@JdbcTypeCode(SqlTypes.JSON) String price;", r#"{"type":"@JdbcTypeCode(SqlTypes.JSON)","sqlType":"numeric(19,2)"}"#),
        ] {
            let text = order_migration(field, Some(mapping), None);
            assert!(text.contains("\"price\" numeric(19,2)"), "{field}: {text}");
        }
    }

    #[test]
    fn hibernate_type_annotations_ask_instead_of_guessing_the_java_type() {
        for field in ["@JdbcTypeCode(SqlTypes.JSON) String price;", "@JavaType(MoneyJt.class) String price;", "@JdbcType(MoneyJdbc.class) String price;", "@TimeZoneStorage(TimeZoneStorageType.NORMALIZE) OffsetDateTime price;", "@TimeZoneColumn(name = \"p_tz\") OffsetDateTime price;"] {
            let text = order_migration(field, None, Some("varchar(40)"));
            assert!(text.contains("\"price\" varchar(40)"), "{field}: {text}");
        }
    }

    #[test]
    fn index_foreign_keys_setting_adds_an_index_statement_for_the_foreign_key_column() {
        for (enabled, expected) in [(true, 1), (false, 0)] {
            let (host, mut plugin, doc) = migration_ready(ENTITY);
            host.add_file("src/main/java/shop/Order.java", "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.ManyToOne;\n@Entity\npublic class Order {\n    @Id Long id;\n    @ManyToOne Customer customer;\n}\n");
            host.set_setting("jpaAssist.indexForeignKeys", if enabled { "true" } else { "false" });
            let pending = action(&mut plugin, &doc, MIGRATION_TITLE);
            host.answer_next_prompt(Some(&["postgresql"]));
            host.answer_next_prompt(Some(&[]));
            host.answer_next_prompt(Some(&["Add order"]));
            for _ in 0..3 {
                host.answer_next_prompt(Some(&[]));
            }
            let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
            let FileOperation::Create { text, .. } = &resolved.operations[0] else { panic!() };
            assert!(text.contains("FOREIGN KEY"), "{text}");
            assert_eq!(text.matches("CREATE INDEX ON \"order\" (\"customer_id\");").count(), expected, "{text}");
        }
    }

    #[test]
    fn migration_refusals() {
        let refused = |setup_extra: &dyn Fn(&MockHost), source: &str| {
            let (host, mut plugin, doc) = migration_ready(source);
            setup_extra(&host);
            generate(&host, &mut plugin, &doc, "postgresql", &[], &[true, true]).is_err()
        };
        assert!(refused(&|h| h.add_file("db/jpa-schema-baseline.next.json", "{}"), ENTITY_PLUS));
        assert!(refused(&|h| h.add_file("flyway.conf", "flyway.locations=filesystem:sql\n"), ENTITY_PLUS));
        assert!(refused(&|h| h.add_file("src/main/resources/application.yml", "spring:\n  flyway:\n    locations: x\n"), ENTITY_PLUS));
        assert!(refused(&|h| h.add_file("other/src/main/resources/db/migration/V1__x.sql", ""), ENTITY_PLUS));
        assert!(refused(&|h| h.add_file("src/main/resources/db/migration/mysql/V9__x.sql", ""), ENTITY_PLUS));
        assert!(refused(&|_| {}, ENTITY), "no model changes");

        let (host, mut plugin, doc) = setup(ENTITY_PLUS);
        host.add_file("src/main/resources/db/migration/V1__first.sql", "");
        let resolved = generate(&host, &mut plugin, &doc, "postgresql", &[], &[true]).expect("no baseline: init migration");
        let FileOperation::Create { path, text, .. } = &resolved.operations[0] else { panic!() };
        assert!(path.ends_with("/V2__Add_customer_email.sql"), "{path}");
        assert!(text.contains("CREATE TABLE \"customer\"") && text.contains("\"email\""), "{text}");
        let FileOperation::Create { path, .. } = &resolved.operations[1] else { panic!() };
        assert!(path.ends_with("/db/jpa-schema-baseline.next.json"));

        let (host, mut plugin, doc) = setup(ENTITY_PLUS);
        host.add_file("db/jpa-schema-baseline.json", &baseline_json(ENTITY));
        assert!(generate(&host, &mut plugin, &doc, "postgresql", &[], &[true]).is_err(), "no migration folder to learn from");

        let (host, mut plugin, doc) = migration_ready(ENTITY_PLUS);
        host.add_file("db/jpa-schema-baseline.json", "not json");
        assert!(generate(&host, &mut plugin, &doc, "postgresql", &[], &[true]).is_err());

        let (host, mut plugin, doc) = migration_ready(ENTITY_PLUS);
        assert!(matches!(generate(&host, &mut plugin, &doc, "postgresql", &[], &[false]), Err(Error::Cancelled)));
    }

    #[test]
    fn incomplete_baseline_needs_a_second_confirmation() {
        let incomplete = baseline_json(ENTITY).replace("\"complete\": true", "\"complete\": false");
        assert!(incomplete.contains("\"complete\": false"));
        let (host, mut plugin, doc) = migration_ready(ENTITY_PLUS);
        host.add_file("db/jpa-schema-baseline.json", &incomplete);
        assert!(matches!(generate(&host, &mut plugin, &doc, "postgresql", &[], &[false]), Err(Error::Cancelled)));
        let (host, mut plugin, doc) = migration_ready(ENTITY_PLUS);
        host.add_file("db/jpa-schema-baseline.json", &incomplete);
        assert!(generate(&host, &mut plugin, &doc, "postgresql", &[], &[true, true]).is_ok());
    }

    #[test]
    fn oversized_sql_preview_is_refused() {
        let (host, mut plugin, doc) = migration_ready(ENTITY_PLUS);
        let mut fields = String::new();
        for n in 0..400 {
            fields.push_str(&format!("    String longColumnNameForPreviewLimit{n};\n"));
        }
        let big = ENTITY.replace("String name;\n", &format!("String name;\n{fields}"));
        host.add_file("src/main/java/shop/Customer.java", &big);
        host.open_editor(PATH, "java", &big, &[]);
        let doc = Document { text: big, ..doc };
        let err = generate(&host, &mut plugin, &doc, "postgresql", &[], &[true]).unwrap_err();
        assert!(format!("{err:?}").contains("16 KiB"), "{err:?}");
    }

    const JAVA_MIGRATION: &str = "package db.migration;\npublic class V3__Backfill extends org.flywaydb.core.api.migration.BaseJavaMigration {}\n";

    #[test]
    fn next_version_counts_java_migrations_so_it_never_collides() {
        let (host, mut plugin, doc) = migration_ready(ENTITY_PLUS);
        host.add_file("src/main/java/db/migration/V3__Backfill.java", JAVA_MIGRATION);
        assert_eq!(created(generate_named(&host, &mut plugin, &doc, &[])), "V4__Add_customer_email.sql");
    }

    #[test]
    fn next_version_with_only_java_migrations_targets_the_resources_folder() {
        let (host, mut plugin, doc) = setup(ENTITY_PLUS);
        host.add_file("db/jpa-schema-baseline.json", &baseline_json(ENTITY));
        host.add_file("src/main/java/db/migration/V3__Backfill.java", JAVA_MIGRATION);
        let resolved = generate_named(&host, &mut plugin, &doc, &[]).unwrap();
        let FileOperation::Create { path, .. } = &resolved.operations[0] else { panic!() };
        assert_eq!(path, &format!("{MIGRATIONS}/V4__Add_customer_email.sql"));
    }

    #[test]
    fn java_migrations_without_a_migration_class_or_in_tests_are_ignored() {
        let (host, mut plugin, doc) = migration_ready(ENTITY_PLUS);
        host.add_file("src/main/java/db/migration/V9__NotAMigration.java", "package db.migration;\npublic class V9__NotAMigration {}\n");
        host.add_file("src/test/java/db/migration/V8__Test.java", JAVA_MIGRATION);
        assert_eq!(created(generate_named(&host, &mut plugin, &doc, &[])), "V3__Add_customer_email.sql");
    }

    #[test]
    fn version_patterns_see_java_migration_versions() {
        let (host, mut plugin, doc) = migration_ready(ENTITY_PLUS);
        host.add_file("src/main/java/db/migration/V7__Backfill.java", JAVA_MIGRATION);
        host.set_setting("flywayVersionPattern", "\"5\"");
        assert!(generate_named(&host, &mut plugin, &doc, &[]).is_err(), "5 sorts before the Java V7");
        let (host, mut plugin, doc) = migration_ready(ENTITY_PLUS);
        host.add_file("src/main/java/db/migration/V1_5__Backfill.java", JAVA_MIGRATION);
        host.set_setting("flywayVersionPattern", "\"#increment(1,1,0)\"");
        assert!(generate_named(&host, &mut plugin, &doc, &[]).is_err(), "#increment over a non-plain Java version");
    }

    #[test]
    fn no_migrations_at_all_refuses() {
        let (host, mut plugin, doc) = setup(ENTITY_PLUS);
        host.add_file("db/jpa-schema-baseline.json", &baseline_json(ENTITY));
        assert!(generate_named(&host, &mut plugin, &doc, &[]).is_err());
    }

    #[test]
    fn flyway_version_rules() {
        assert_eq!(versioned("V2__init.sql"), Some("2"));
        assert_eq!(versioned("V1_1__a.sql"), Some("1_1"));
        for bad in ["V__a.sql", "V1.sql", "v1__a.sql", "V1a__x.sql", "V1__.sql", "R__a.sql", "V1__a.txt", "V1..2__a.sql"] {
            assert_eq!(versioned(bad), None, "{bad}");
        }
        assert_eq!(next_version(&[]), "1");
        assert_eq!(next_version(&["1", "10", "9"]), "11");
        assert_eq!(next_version(&["1.2", "1.10"]), "1.11");
        assert_eq!(next_version(&["0099"]), "0100");
        assert_eq!(next_version(&["2_1", "2"]), "2_2");
        assert_eq!(description_of("  Add customer email!! "), "Add_customer_email");
        assert_eq!(description_of("!!"), "");
    }
}
