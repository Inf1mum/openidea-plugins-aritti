//! Reviewed "Add attributes from database columns…" on an existing entity (reverse engineering, update side).
//! One document is edited: the invoking entity. The columns come from one table of a `SchemaSnapshot`, read either from a
//! SQL DDL file of the project (`ddl_parse`) or, read-only and only through the existing connection confirmation, from a
//! saved database connection. Columns the entity does not map yet are offered; the mapped ones are only listed in the
//! preview. A column whose mapping cannot be proven (relationship join columns without a literal name, hierarchy or
//! secondary-table layouts) is neither offered nor treated as mapped, and the entity is refused when the columns of a
//! superclass or embedded type cannot be resolved. Attribute text is rendered by the SQL import (`render`), so the type
//! mapping and the options are the same.
use super::*;
use crate::jdbc_connect;
use crate::model::{Relation, import_edit};
use persistence_core::connection::{Dbms, Profile};
use persistence_core::introspect::{Client, ErrorCode, HelperError, Runner};
use persistence_core::introspect_map::{MapOptions, to_snapshot};

pub(in crate::file_actions) const TITLE: &str = "Add attributes from database columns…";
const MAX_CANDIDATES: usize = 20;
const MAX_OFFERED: usize = 200;
const MAX_ADDED: usize = 50;
const MAX_DEPTH: usize = 4;
const LIVE_DEADLINE_MS: u64 = 20_000;
const REFUSING_ANNOTATIONS: [&str; 11] = [
    "Inheritance", "DiscriminatorColumn", "DiscriminatorValue", "SecondaryTable", "SecondaryTables", "JoinColumns", "AttributeOverride",
    "AttributeOverrides", "AssociationOverride", "AssociationOverrides", "IdClass",
];

struct Subject {
    class: Class,
    ns: &'static str,
    body_end: usize,
    table: String,
}

fn subject(doc: &Document) -> Result<Subject, String> {
    let masked = mask(&doc.text);
    if has_word(&masked, "typealias") || masked.lines().any(|line| line.trim_start().starts_with("import ") && line.contains(" as ")) {
        return Err("Imports with aliases are not supported.".into());
    }
    let class = parse_class(&doc.text, &doc.path).filter(|c| c.kind == Kind::Entity).ok_or("Select an entity.")?;
    if !identifier(&class.name)
        || words(&masked, "class").filter(|at| *at == 0 || masked.as_bytes()[at - 1] != b'.').count() != 1
        || package(doc).as_ref() != class.package.as_ref()
    {
        return Err("Only a single source-owned entity class is supported.".into());
    }
    let ns = namespace(&doc.text, &class).ok_or("The entity's persistence namespace is not unambiguous.")?;
    let body_end = class.body_end.filter(|end| *end < doc.text.len()).ok_or("The entity has no class body to add attributes to.")?;
    let line_start = doc.text[..body_end].rfind('\n').map_or(0, |i| i + 1);
    if line_start != body_end {
        return Err("The entity's closing brace must start its own line.".into());
    }
    let table = class.table.clone().unwrap_or_else(|| crate::file_actions::naming_table_name(&class.name));
    Ok(Subject { class, ns, body_end, table: plain(&table) })
}

pub(in crate::file_actions) fn eligible(doc: &Document) -> bool {
    subject(doc).is_ok()
}

fn plain(name: &str) -> String {
    name.chars().filter(|c| !matches!(c, '"' | '`' | '\\')).collect()
}

fn normal(name: &str) -> String {
    plain(name).to_ascii_lowercase()
}

type Found = (Class, String);

/// What the project says about a class name: `Ok(None)` is a class that carries no mapping annotation.
fn lookup(project: &Project, name: &str) -> Result<Option<Found>, String> {
    if !identifier(name) {
        return Err(format!("`{name}` is not a plain class name."));
    }
    let mut files = Vec::new();
    for extension in ["java", "kt"] {
        files.extend(project.find_files(&format!("**/{name}.{extension}"), 10).map_err(|e| e.to_string())?);
    }
    let [file] = files.as_slice() else {
        return Err(if files.is_empty() {
            format!("`{name}` is not a source of this project, so the columns it maps are unknown.")
        } else {
            format!("`{name}` is declared in several files, so the columns it maps are ambiguous.")
        });
    };
    let text = project.read_file(file).map_err(|e| e.to_string())?;
    if text.len() > MAX_TEXT {
        return Err(format!("`{name}` is too large to read."));
    }
    Ok(parse_class(&text, file).map(|class| (class, text)))
}

#[derive(Debug, Default)]
struct Mapped {
    /// Lower-cased column to the attribute that maps it.
    columns: BTreeMap<String, String>,
    /// Lower-cased column-name prefix to the relationship whose join column it may be.
    unknown: Vec<(String, String)>,
}

fn refuse_layouts(text: &str, what: &str) -> Result<(), String> {
    let masked = mask(text);
    match annotations(&masked).into_iter().find(|a| REFUSING_ANNOTATIONS.contains(&a.name)) {
        Some(found) => Err(format!("{what} uses @{}, whose columns are not resolved.", found.name)),
        None => Ok(()),
    }
}

fn collect(class: &Class, lookup: &dyn Fn(&str) -> Result<Option<Found>, String>, depth: usize, out: &mut Mapped) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!("{} nests mapped types too deeply.", class.name));
    }
    for attribute in &class.attributes {
        let column = |explicit: &Option<String>| normal(&explicit.clone().unwrap_or_else(|| crate::file_actions::naming_column_name(&attribute.name)));
        match attribute.relation {
            Relation::None => {
                out.columns.insert(column(&attribute.column), attribute.name.clone());
            }
            Relation::ManyToOne | Relation::OneToOne if !attribute.mapped_by => match &attribute.column {
                Some(_) => {
                    out.columns.insert(column(&attribute.column), attribute.name.clone());
                }
                None => out.unknown.push((format!("{}_", normal(&crate::file_actions::naming_column_name(&attribute.name))), attribute.name.clone())),
            },
            Relation::Embedded => {
                let ty = attribute.ty.split('<').next().unwrap_or(&attribute.ty).trim();
                let (embedded, embedded_text) = lookup(ty)?.filter(|(c, _)| c.kind == Kind::Embeddable).ok_or(format!("The embedded type `{ty}` is not an embeddable of this project."))?;
                refuse_layouts(&embedded_text, ty)?;
                collect(&embedded, lookup, depth + 1, out)?;
            }
            _ => {}
        }
    }
    let Some(parent) = &class.superclass else { return Ok(()) };
    if class.kotlin && ["Serializable", "Comparable", "Cloneable"].contains(&parent.as_str()) {
        return Ok(());
    }
    match lookup(parent)? {
        None => Ok(()),
        Some((found, found_text)) if found.kind == Kind::MappedSuperclass => {
            refuse_layouts(&found_text, parent)?;
            collect(&found, lookup, depth + 1, out)
        }
        Some(_) => Err(format!("The superclass `{parent}` is an entity or embeddable; the columns of the hierarchy are not resolved.")),
    }
}

fn mapped_columns(subject: &Subject, text: &str, lookup: &dyn Fn(&str) -> Result<Option<Found>, String>) -> Result<Mapped, String> {
    refuse_layouts(text, &subject.class.name)?;
    let mut mapped = Mapped::default();
    collect(&subject.class, lookup, 0, &mut mapped)?;
    Ok(mapped)
}

#[derive(Debug, Default)]
struct Split {
    offered: Vec<usize>,
    mapped: Vec<(String, String)>,
    notes: Vec<String>,
}

fn split(table: &Table, columns: &[Column], mapped: &Mapped) -> Split {
    let key = table.primary_key.known().cloned().unwrap_or_default();
    let mut out = Split::default();
    for (at, column) in columns.iter().enumerate() {
        let name = normal(&column.name);
        if let Some(attribute) = mapped.columns.get(&name) {
            out.mapped.push((column.name.clone(), attribute.clone()));
        } else if key.iter().any(|k| k.eq_ignore_ascii_case(&column.name)) {
            out.notes.push(format!("{}: primary key column not mapped by this entity; not offered (declare @Id by hand)", column.name));
        } else if let Some((_, relation)) = mapped.unknown.iter().find(|(prefix, _)| name.starts_with(prefix.as_str())) {
            out.notes.push(format!("{}: may be the join column of `{relation}`, which names none; mapping unknown, not offered", column.name));
        } else {
            out.offered.push(at);
        }
    }
    out
}

fn from_snapshot(snapshot: persistence_core::schema::SchemaSnapshot) -> ParsedDdl {
    ParsedDdl {
        snapshot,
        skipped: Vec::new(),
        skipped_overflow: 0,
        enum_types: Vec::new(),
        column_enums: Vec::new(),
        comments: Vec::new(),
        ordered_indexes: Vec::new(),
        views: Vec::new(),
        unmapped_columns: Vec::new(),
    }
}

#[derive(Debug)]
enum Live {
    Denied,
    Helper(HelperError),
    Missing(String),
    Ambiguous,
    Unmapped(String),
}

/// One table through the helper, mapped to a `SchemaSnapshot`; the table is looked up under the name as written and
/// lower-cased in every filtered schema (the current one when no filter is saved).
fn read_table<R: Runner>(client: &mut Client<R>, dbms: Dbms, schemas: &[String], table: &str, cancelled: &dyn Fn() -> bool) -> Result<ParsedDdl, Live> {
    let dialect = dbms.mapping_dialect().ok_or_else(|| Live::Unmapped(format!("{} types are not mapped yet", dbms.label())))?;
    let scopes: Vec<Option<String>> = if schemas.is_empty() { vec![None] } else { schemas.iter().cloned().map(Some).collect() };
    let mut names = vec![table.to_owned()];
    if table.to_ascii_lowercase() != table {
        names.push(table.to_ascii_lowercase());
    }
    let mut found = Vec::new();
    for scope in &scopes {
        for name in &names {
            if cancelled() {
                return Err(Live::Helper(HelperError { code: ErrorCode::Cancelled, message: "cancelled".into(), sql_state: None, vendor_code: 0, retryable: false }));
            }
            let metas = match client.describe_all(scope.as_deref(), std::slice::from_ref(name)) {
                Ok(metas) => metas,
                Err(error) if error.code == ErrorCode::ProcessDenied => return Err(Live::Denied),
                Err(error) => return Err(Live::Helper(error)),
            };
            if let Some(meta) = metas.into_iter().find(|m| m.found) {
                found.push(meta);
                break;
            }
        }
    }
    let meta = match found.len() {
        0 => return Err(Live::Missing(table.to_owned())),
        1 => found.remove(0),
        _ => return Err(Live::Ambiguous),
    };
    let mapped = to_snapshot(dialect, &[meta], &MapOptions { listing_complete: false, qualify_schema: false }).map_err(|_| Live::Ambiguous)?;
    if mapped.snapshot.tables.is_empty() {
        return Err(Live::Missing(format!("{table} (it is a view)")));
    }
    Ok(from_snapshot(mapped.snapshot))
}

fn live_message(error: &Live) -> String {
    match error {
        Live::Denied => "The database helper needs the optional process:java permission.".into(),
        Live::Helper(error) => persistence_core::connection::error_message(error),
        Live::Missing(table) => format!("Table `{table}` was not found in the database."),
        Live::Ambiguous => "The table exists in more than one schema; save a schema filter for the connection (JPA: Manage Database Connections).".into(),
        Live::Unmapped(why) => why.clone(),
    }
}

struct Candidate {
    path: String,
    text: String,
}

fn sql_candidates(project: &Project, root: &str, table: &str) -> Result<Vec<Candidate>, Error> {
    let wanted = table.to_ascii_lowercase();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut found = Vec::new();
    for file in project.find_files("**/*.sql", 2001)? {
        if found.len() >= MAX_CANDIDATES || Instant::now() >= deadline {
            break;
        }
        let file = absolute(root, &file);
        if path(&file).as_deref() != Some(file.as_str()) || !inside(root, &file) {
            continue;
        }
        let Ok(text) = project.read_file(&file) else { continue };
        let lower = text.to_ascii_lowercase();
        if text.len() <= MAX_SQL && lower.contains("create table") && lower.contains(&wanted) {
            found.push(Candidate { path: file, text });
        }
    }
    Ok(found)
}

enum Source {
    Sql(usize),
    Live(usize),
}

fn choose_source(root: &str, table: &str, candidates: &[Candidate], profiles: &[Profile]) -> Result<Source, Error> {
    let mut items = Vec::new();
    for (i, candidate) in candidates.iter().enumerate() {
        let shown = candidate.path.strip_prefix(&format!("{}/", root.trim_end_matches('/'))).unwrap_or(&candidate.path);
        items.push(PickItem::new(&format!("sql:{i}"), shown).description("CREATE TABLE statements of this SQL file; ALTER TABLE ADD COLUMN is not read"));
    }
    for (i, profile) in profiles.iter().enumerate() {
        let label = format!("Database: {}", profile.name);
        items.push(PickItem::new(&format!("db:{i}"), &label).description(&format!("{} {}; read-only metadata through a local java process, confirmed first", profile.dbms.label(), profile.target())));
    }
    if items.is_empty() {
        return Err(format!("No SQL file of the project creates table `{table}` and no saved database connection can be read (JPA: Manage Database Connections).").into());
    }
    let chosen = host::pick(&format!("Where are the columns of table {table} read from?"), &items)?.ok_or(Error::Cancelled)?;
    let index = |prefix: &str| chosen.strip_prefix(prefix).and_then(|n| n.parse::<usize>().ok());
    match (index("sql:").filter(|i| *i < candidates.len()), index("db:").filter(|i| *i < profiles.len())) {
        (Some(i), _) => Ok(Source::Sql(i)),
        (_, Some(i)) => Ok(Source::Live(i)),
        _ => Err(Error::Stale),
    }
}

fn describe_column(parsed: &ParsedDdl, ti: usize, dialect: SqlDialect, options: &Options, column: &Column) -> String {
    let Ok(found) = gate(parsed, ti, dialect, options) else { return "refused: the table has facts that are not known".into() };
    match column_attr(parsed, ti, dialect, options, &found, column, &HashMap::new()) {
        Ok(built) => format!("{} {}", if options.kotlin { &built.attr.kotlin } else { &built.attr.java }, built.attr.name),
        Err(reason) => format!("refused: {reason}"),
    }
}

fn pick_options(parsed: &ParsedDdl, table: &str, offered: &[&Column]) -> Result<Options, Error> {
    let mut items = Vec::new();
    if parsed.comments.iter().any(|c| c.table == table) {
        items.push(PickItem::new("comments", "Copy database comments as Javadoc").description("Up to 240 characters each"));
    }
    if offered.iter().any(|c| matches!(c.sql_type, Fact::Unknown(_))) {
        items.push(PickItem::new("deferred", "Decide per column for unmapped SQL types").description("Skip it (listed as deferred) or String with @Column(columnDefinition); never a guessed Java type"));
    }
    items.push(PickItem::new("columnPrefix", "Strip column-name prefixes").description("Comma-separated, for example col_"));
    items.push(PickItem::new("columnSuffix", "Strip column-name suffixes").description("Comma-separated, for example _col"));
    items.push(PickItem::new("editNames", "Edit attribute names").description("One prompt per added column"));
    items.push(PickItem::new("typeOverrides", "Override SQL-to-Java type mappings").description("Java attribute type or JPA attribute converter"));
    let picked: BTreeSet<String> = host::pick_many("Attribute options (none chosen: defaults)", &items)?.ok_or(Error::Cancelled)?.into_iter().collect();
    let mut options = Options {
        comments: picked.contains("comments"),
        offer_deferred: picked.contains("deferred"),
        edit_names: picked.contains("editNames"),
        type_overrides: picked.contains("typeOverrides"),
        ..Options::default()
    };
    if picked.contains("columnPrefix") {
        options.column_prefixes = prompt_affixes("Column prefixes to strip (comma-separated, case-insensitive)")?;
    }
    if picked.contains("columnSuffix") {
        options.column_suffixes = prompt_affixes("Column suffixes to strip (comma-separated, case-insensitive)")?;
    }
    Ok(options)
}

/// The members of a rendered one-entity file: its imports and the class body, with the accessor methods only when asked.
fn members(rendered: &str, class: &str, accessors: bool, ns: &str) -> Result<(Vec<String>, String), Error> {
    let header = format!("class {class} {{\n");
    let start = rendered.find(&header).map(|at| at + header.len()).ok_or(Error::Stale)?;
    let end = rendered.rfind("}\n").filter(|end| *end >= start).ok_or(Error::Stale)?;
    let inner = &rendered[start..end];
    let (fields, methods) = match inner.find("\n    public ") {
        Some(at) => (&inner[..=at], &inner[at + 1..]),
        None => (inner, ""),
    };
    if fields.trim().is_empty() {
        return Err(Error::Stale);
    }
    let mut body = format!("{}\n", fields.trim_end());
    if accessors && !methods.trim().is_empty() {
        body = format!("{}\n\n{}\n", fields.trim_end(), methods.trim_end());
    }
    let skipped = [format!("{ns}.Entity"), format!("{ns}.Table")];
    let imports = rendered[..start]
        .lines()
        .filter_map(|line| line.strip_prefix("import "))
        .map(|line| line.trim_end_matches(';').to_owned())
        .filter(|import| !skipped.contains(import))
        .collect();
    Ok((imports, body))
}

fn taken_names(subject: &Subject, body: &str, mapped_names: &BTreeSet<String>) -> impl Fn(&str) -> bool {
    let attributes: BTreeSet<String> = subject.class.attributes.iter().map(|a| a.name.clone()).chain(mapped_names.iter().cloned()).collect();
    let body = body.to_owned();
    move |name: &str| attributes.contains(name) || ["get", "set"].iter().any(|prefix| has_word(&body, &format!("{prefix}{}", capitalize(name))))
}

fn live_for(project: Option<Project>, profile: &Profile, table: &str) -> Result<ParsedDdl, Result<Live, Error>> {
    let Some(mut client) = jdbc_connect::prepare_with(project, profile).map_err(Err)? else { return Err(Err(Error::Cancelled)) };
    client.deadline_ms = LIVE_DEADLINE_MS;
    let task = host::begin_task("Reading table columns", true);
    let read = read_table(&mut client, profile.dbms, &profile.schema_filter.schemas, table, &|| task.is_cancelled());
    drop(task);
    read.map_err(Ok)
}

pub(in crate::file_actions) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let subject = subject(doc).map_err(Error::from)?;
    let class = &subject.class;
    let masked = mask(&doc.text);
    let mapped = mapped_columns(&subject, &doc.text, &|name| lookup(project, name))?;
    let table_name = subject.table.clone();
    if !table_name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$')) || table_name.is_empty() {
        return Err(format!("Table name `{table_name}` needs identifier quoting, which is not supported.").into());
    }

    let candidates = sql_candidates(project, &pending.root, &table_name)?;
    let mut profiles: Vec<Profile> = jdbc_connect::saved_profiles().unwrap_or_default().into_iter().filter(|p| p.dbms.mapping_dialect().is_some()).collect();
    let (parsed, dialect, origin) = loop {
        match choose_source(&pending.root, &table_name, &candidates, &profiles)? {
            Source::Sql(i) => {
                let candidate = &candidates[i];
                let (dialect, detected) = match ddl_parse::detect_dialect(&candidate.text) {
                    Some(dialect) => (dialect, true),
                    None => {
                        let chosen = crate::choose_dialect_for(None)?.ok_or(Error::Cancelled)?;
                        (if chosen == crate::Dialect::MySql { SqlDialect::MySql } else { SqlDialect::PostgreSql }, false)
                    }
                };
                let parsed = ddl_parse::parse_ddl(&candidate.text, dialect).map_err(|e| Error::from(describe(&e)))?;
                let skipped = parsed.skipped.len() + parsed.skipped_overflow;
                let name = if dialect == SqlDialect::MySql { "MySQL" } else { "PostgreSQL" };
                let note = if skipped > 0 { format!(", {skipped} statement(s) skipped" ) } else { String::new() };
                break (parsed, dialect, format!("SQL file {} ({name}{}{note})", candidate.path, if detected { ", detected" } else { ", chosen" }));
            }
            Source::Live(i) => {
                let profile = profiles[i].clone();
                match live_for(host::project_of(&doc.path), &profile, &table_name) {
                    Ok(parsed) => {
                        let dialect = if profile.dbms.mapping_dialect() == Some(SqlDialect::MySql) { SqlDialect::MySql } else { SqlDialect::PostgreSql };
                        break (parsed, dialect, format!("database connection `{}` ({}, read-only metadata)", profile.name, profile.dbms.label()));
                    }
                    Err(Err(error)) => return Err(error),
                    Err(Ok(Live::Denied)) => {
                        host::notify(NotificationLevel::Info, "JPA Assist", "The optional process:java permission is not granted, so the live database source is skipped.");
                        profiles.clear();
                        if candidates.is_empty() {
                            return Err("The live source needs the optional process:java permission, and no SQL file creates the table.".into());
                        }
                    }
                    Err(Ok(other)) => return Err(live_message(&other).into()),
                }
            }
        }
    };

    let Some(ti) = ddl_parse::find_table(&parsed.snapshot, &table_name) else {
        return Err(format!("Table `{table_name}` was not found in the chosen source.").into());
    };
    let table = &parsed.snapshot.tables[ti];
    let columns = match &table.columns {
        Fact::Known(columns) => columns,
        Fact::Unknown(reason) => return Err(format!("The columns of `{table_name}` are not known ({reason}).").into()),
    };
    let divided = split(table, columns, &mapped);
    if divided.offered.is_empty() {
        return Err(format!("Every column of `{table_name}` is already mapped by {} or cannot be offered; nothing to add.{}", class.name, notes_text(&divided.notes)).into());
    }
    if divided.offered.len() > MAX_OFFERED {
        return Err(format!("{} columns are not mapped yet, more than the {MAX_OFFERED} this action offers.", divided.offered.len()).into());
    }

    let existing = Options { existing: true, basic_references: true, classes: HashMap::from([(ti, class.name.clone())]), kotlin: class.kotlin, ..Options::default() };
    let items: Vec<PickItem> = divided
        .offered
        .iter()
        .map(|&at| PickItem::new(&columns[at].name, &columns[at].name).description(&describe_column(&parsed, ti, dialect, &existing, &columns[at])))
        .collect();
    let title = format!("Columns of {table_name} not mapped by {} (at most {MAX_ADDED})", class.name);
    let chosen = host::pick_many(&title, &items)?.ok_or(Error::Cancelled)?;
    if chosen.is_empty() || chosen.len() > MAX_ADDED {
        return Err(format!("Choose between one and {MAX_ADDED} columns.").into());
    }
    let mut picked = BTreeSet::new();
    for name in &chosen {
        if !divided.offered.iter().any(|&at| columns[at].name == *name) {
            return Err(Error::Stale);
        }
        picked.insert(name.to_ascii_lowercase());
    }

    let offered_columns: Vec<&Column> = divided.offered.iter().map(|&at| &columns[at]).filter(|c| picked.contains(&c.name.to_ascii_lowercase())).collect();
    let mut options = pick_options(&parsed, &table.name, &offered_columns)?;
    options.existing = true;
    options.basic_references = true;
    options.kotlin = class.kotlin;
    options.classes = HashMap::from([(ti, class.name.clone())]);
    options.columns = HashMap::from([(ti, picked)]);
    let selected = [ti];
    extra::prompt_names(&parsed, dialect, &selected, &mut options)?;
    extra::prompt_overrides(&parsed, dialect, &selected, &mut options)?;
    prompt_deferred(&parsed, dialect, &selected, &mut options)?;

    let mut built = plan(&parsed, ti, dialect, &HashMap::new(), &options).map_err(Error::from)?;
    let mapped_names: BTreeSet<String> = mapped.columns.values().cloned().collect();
    let taken = taken_names(&subject, &masked, &mapped_names);
    let mut seen = BTreeSet::new();
    let clashes: Vec<(String, String)> = built.attrs.iter().filter(|a| taken(&a.name)).map(|a| (a.column.clone(), a.name.clone())).collect();
    for (column, name) in clashes {
        let wanted = host::input(&format!("Attribute name for {table_name}.{column}: `{name}` is already used in {}", class.name), None)?.ok_or(Error::Cancelled)?.trim().to_owned();
        if !identifier(&wanted) || taken(&wanted) {
            return Err(format!("`{wanted}` is not a valid, unused attribute name.").into());
        }
        options.names.insert((ti, column.to_ascii_lowercase()), wanted);
    }
    if !options.names.is_empty() {
        built = plan(&parsed, ti, dialect, &HashMap::new(), &options).map_err(Error::from)?;
    }
    for attr in &built.attrs {
        if taken(&attr.name) || !seen.insert(attr.name.clone()) {
            return Err(format!("Attribute name `{}` is used twice or already exists; use the option to edit attribute names.", attr.name).into());
        }
    }
    if built.attrs.is_empty() && built.todo_fields.is_empty() {
        return Err("Nothing to add: every chosen column was deferred.".into());
    }
    let added: Vec<(String, String)> = built.attrs.iter().map(|a| (a.column.clone(), format!("{} {}", if class.kotlin { &a.kotlin } else { &a.java }, a.name))).collect();
    let mut notes: Vec<String> = built.notes.iter().filter(|n| built.attrs.iter().any(|a| n.starts_with(&format!("{}:", a.column)))).cloned().collect();
    notes.extend(built.deferred.iter().map(|d| format!("deferred: {d} is not mapped")));
    notes.push("foreign key columns are added as plain attributes; no relationship is created".into());

    let accessors = class.attributes.iter().any(|a| has_word(&masked, &format!("get{}", capitalize(&a.name)))) && !class.kotlin;
    let rendered = render(&built, class.package.as_deref().unwrap_or(""), subject.ns, class.kotlin);
    let (imports, body) = members(&rendered, &class.name, accessors, subject.ns)?;
    if let Some(clash) = imports.iter().find(|i| class.imports.iter().any(|c| c.rsplit('.').next() == i.rsplit('.').next() && c != *i && !c.ends_with('*'))) {
        return Err(format!("An import with the simple name of `{clash}` already exists; edit it manually.").into());
    }
    let refs: Vec<&str> = imports.iter().map(String::as_str).collect();
    let mut edits: Vec<Edit> = Vec::new();
    edits.extend(import_edit(&doc.text, &refs, class.kotlin));
    edits.push(Edit::insert(subject.body_end, &format!("\n{body}")));
    if doc.text.contains("\r\n") {
        edits.iter_mut().for_each(|e| e.text = e.text.replace('\n', "\r\n"));
    }
    let edits: Vec<TextEdit> = edits.iter().map(|e| e.to_text_edit(&doc.text)).collect();
    checked_after(doc, &edits)?;

    let mut preview = format!(
        "Target: {}\nSource version: {}\nTable: {table_name}\nColumns read from: {origin}\n\nAlready mapped by {} ({}):\n",
        doc.path,
        doc.version,
        class.name,
        divided.mapped.len()
    );
    preview.extend(divided.mapped.iter().map(|(column, attribute)| format!("- {column} -> {attribute}\n")));
    if !divided.notes.is_empty() {
        preview.push_str(&format!("\nNot offered, mapping unknown or key:\n{}", divided.notes.iter().map(|n| format!("- {n}\n")).collect::<String>()));
    }
    preview.push_str(&format!("\nAdding ({}):\n", added.len()));
    preview.extend(added.iter().map(|(column, attribute)| format!("- {column} -> {attribute}\n")));
    preview.extend(notes.iter().map(|n| format!("note: {n}\n")));
    for edit in &edits {
        preview.push_str(&format!("\n{}:\n{}", if edit.text.contains("import ") { "Imports" } else { "Inserted before the closing brace" }, edit.text));
    }
    preview.push_str("\nOnly this source is edited. Existing members are untouched; defaults and CHECK constraints are not mapped.");
    if preview.len() > ui_kit::prompts::MAX_CONFIRM_BYTES {
        return Err("The complete preview exceeds the host's 16 KiB confirmation limit; pick fewer columns.".into());
    }
    if !ui_kit::prompts::confirm_exact("Add attributes from database columns?", &preview)? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    Ok(FileAction { title: "Add attributes from database columns".into(), kind: Some("refactor.rewrite".into()), edits: vec![(doc.path.clone(), edits)], ..FileAction::default() })
}

fn notes_text(notes: &[String]) -> String {
    if notes.is_empty() { String::new() } else { format!(" Not offered: {}.", notes.join("; ")) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::ProcessOutput;
    use openidea_plugin::mock::MockHost;
    use persistence_core::connection::{SchemaFilter, profiles_json};
    use serde_json::{Value, json};

    const SENTINEL: &str = "S3NT1NEL-pw";
    const ENTITY_PATH: &str = "/project/src/main/java/shop/Customer.java";
    const SQL: &str = "CREATE TABLE regions (id bigserial PRIMARY KEY);\n\
        CREATE TABLE customers (\n  id bigserial PRIMARY KEY,\n  email varchar(120) NOT NULL,\n  full_name varchar(80),\n  created_at timestamptz NOT NULL,\n  balance numeric(12,2),\n  region_id bigint REFERENCES regions (id),\n  legacy_code varchar(10),\n  doc jsonb\n);\n";
    const ENTITY: &str = "package shop;\n\nimport jakarta.persistence.Column;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.JoinColumn;\nimport jakarta.persistence.ManyToOne;\nimport jakarta.persistence.Table;\n\n@Entity\n@Table(name = \"customers\")\npublic class Customer {\n\n    @Id\n    private Long id;\n\n    @Column(name = \"email\")\n    private String mail;\n\n    private String fullName;\n\n    @ManyToOne\n    @JoinColumn(name = \"region_id\")\n    private Region region;\n\n    public Long getId() {\n        return id;\n    }\n}\n";

    fn host_with(entity: &str, files: &[(&str, &str)]) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read", "storage", "process:java"]);
        for (path, text) in files {
            host.add_file(path, text);
        }
        host.open_editor(ENTITY_PATH, "java", entity, &[]);
        let doc = Document { path: ENTITY_PATH.into(), language: "java".into(), text: entity.into(), version: 1 };
        (host, JpaAssist::default(), doc)
    }

    fn offered(plugin: &mut JpaAssist, doc: &Document) -> Option<FileAction> {
        plugin.file_actions(doc, Range::default()).into_iter().find(|a| a.title == TITLE)
    }

    fn answer(host: &MockHost, answers: &[&[&str]]) {
        for one in answers {
            host.answer_next_prompt(Some(one));
        }
    }

    fn run(host: &MockHost, plugin: &mut JpaAssist, doc: &Document, answers: &[&[&str]]) -> Result<FileAction, Error> {
        let action = offered(plugin, doc).expect("action offered");
        answer(host, answers);
        plugin.resolve_file_action(doc, action)
    }

    fn after(doc: &Document, result: &FileAction) -> String {
        assert!(result.operations.is_empty() && result.edits.len() == 1 && result.edits[0].0 == doc.path);
        patched(&doc.text, &result.edits[0].1).unwrap()
    }

    #[test]
    fn adds_only_unmapped_columns_with_imports_and_leaves_the_rest_untouched() {
        let (host, mut plugin, doc) = host_with(ENTITY, &[("db/schema.sql", SQL)]);
        let result = run(&host, &mut plugin, &doc, &[&["sql:0"], &["created_at", "balance", "legacy_code"], &[], &[]]).unwrap();
        let text = after(&doc, &result);
        assert!(text.contains("import java.math.BigDecimal;") && text.contains("import java.time.OffsetDateTime;"), "{text}");
        assert!(text.contains("    @Column(nullable = false)\n    private OffsetDateTime createdAt;\n"), "{text}");
        assert!(text.contains("    @Column(precision = 12, scale = 2)\n    private BigDecimal balance;\n"), "{text}");
        assert!(text.contains("    @Column(length = 10)\n    private String legacyCode;\n"), "{text}");
        assert!(text.contains("    public OffsetDateTime getCreatedAt() {"), "{text}");
        assert!(!text.contains("fullName;\n\n    private") && text.starts_with("package shop;"), "{text}");
        assert!(text.contains("    @Id\n    private Long id;\n\n    @Column(name = \"email\")\n    private String mail;"), "existing members are untouched");
        assert!(!text.contains("docType") && !text.contains("private String doc"), "{text}");
        let class = parse_class(&text, &doc.path).unwrap();
        assert_eq!(class.attributes.len(), 7);
        assert_eq!(host.editor_text(), ENTITY);
    }

    #[test]
    fn already_mapped_and_unknown_columns_are_not_offered() {
        let (host, mut plugin, doc) = host_with(ENTITY, &[("db/schema.sql", SQL)]);
        for column in ["email", "full_name", "region_id", "id"] {
            let result = run(&host, &mut plugin, &doc, &[&["sql:0"], &[column]]);
            assert!(format!("{result:?}").contains("is not one of the items"), "{column}: {result:?}");
        }
    }

    #[test]
    fn refused_column_types_name_the_reason_and_the_action_stops() {
        let (host, mut plugin, doc) = host_with(ENTITY, &[("db/schema.sql", SQL)]);
        let message = format!("{:?}", run(&host, &mut plugin, &doc, &[&["sql:0"], &["doc"], &[], &[]]).unwrap_err());
        assert!(message.contains("column doc") && message.contains("jsonb"), "{message}");
    }

    #[test]
    fn deferred_unmapped_type_becomes_a_string_with_column_definition() {
        let (host, mut plugin, doc) = host_with(ENTITY, &[("db/schema.sql", SQL)]);
        let result = run(&host, &mut plugin, &doc, &[&["sql:0"], &["doc"], &["deferred"], &["string"], &[]]).unwrap();
        let text = after(&doc, &result);
        assert!(text.contains("@Column(columnDefinition = \"jsonb\")\n    private String doc;"), "{text}");
    }

    #[test]
    fn foreign_key_columns_become_plain_attributes_and_attribute_names_can_be_edited() {
        let sql = "CREATE TABLE customers (id bigserial PRIMARY KEY, owner_id bigint REFERENCES customers (id), nick varchar(20));\n";
        let (host, mut plugin, doc) = host_with(ENTITY, &[("db/schema.sql", sql)]);
        let result = run(&host, &mut plugin, &doc, &[&["sql:0"], &["owner_id", "nick"], &["editNames"], &["ownerId"], &["alias"], &[]]).unwrap();
        let text = after(&doc, &result);
        assert!(text.contains("private Long ownerId;") && text.contains("@Column(name = \"nick\", length = 20)\n    private String alias;"), "{text}");
        assert!(!text.contains("ManyToOne\n    @JoinColumn(name = \"owner_id\""), "{text}");
    }

    #[test]
    fn a_name_that_is_already_used_is_asked_again_and_a_used_answer_refuses() {
        let sql = "CREATE TABLE customers (id bigserial PRIMARY KEY, email varchar(120), mail_address varchar(5), region varchar(5));\n";
        let (host, mut plugin, doc) = host_with(ENTITY, &[("db/schema.sql", sql)]);
        let result = run(&host, &mut plugin, &doc, &[&["sql:0"], &["region"], &[], &["regionName"], &[]]).unwrap();
        assert!(after(&doc, &result).contains("private String regionName;"));
        let refused = run(&host, &mut plugin, &doc, &[&["sql:0"], &["region"], &[], &["id"]]);
        assert!(format!("{refused:?}").contains("not a valid, unused attribute name"), "{refused:?}");
    }

    #[test]
    fn the_configured_column_naming_decides_what_is_mapped_and_written() {
        let sql = "CREATE TABLE customers (c_id bigserial PRIMARY KEY, c_full_name varchar(80), c_nick varchar(20));\n";
        let (host, mut plugin, doc) = host_with(ENTITY, &[("db/schema.sql", sql)]);
        host.set_setting("jpaAssist.columnNamePrefix", "\"c_\"");
        let result = run(&host, &mut plugin, &doc, &[&["sql:0"], &["c_nick"], &[], &[]]).unwrap();
        let text = after(&doc, &result);
        assert!(text.contains("    @Column(name = \"c_nick\", length = 20)\n    private String cNick;"), "{text}");
        let refused = run(&host, &mut plugin, &doc, &[&["sql:0"], &["c_full_name"]]);
        assert!(format!("{refused:?}").contains("is not one of the items"), "fullName maps c_full_name under the prefix: {refused:?}");
    }

    #[test]
    fn mapping_is_taken_from_explicit_names_the_template_and_resolved_types() {
        let _host = MockHost::install();
        let class = |text: &str| parse_class(text, "/project/src/main/java/shop/A.java").unwrap();
        let none = |_: &str| -> Result<Option<Found>, String> { Err("unexpected lookup".into()) };
        let subject = |text: &str| Subject { class: class(text), ns: "jakarta.persistence", body_end: 0, table: "a".into() };
        let entity = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class A {\n @Id Long id;\n @Column(name=\"Mail\") String email;\n String fullName;\n @ManyToOne @JoinColumn(name=\"region_id\") R region;\n @ManyToOne R owner;\n @OneToMany(mappedBy=\"a\") java.util.List<B> bs;\n @Transient String cache;\n}\n";
        let mapped = mapped_columns(&subject(entity), entity, &none).unwrap();
        assert_eq!(mapped.columns.keys().map(String::as_str).collect::<Vec<_>>(), ["full_name", "id", "mail", "region_id"]);
        assert_eq!(mapped.unknown, [("owner_".to_owned(), "owner".to_owned())]);
    }

    #[test]
    fn join_columns_without_a_literal_name_stay_unknown_and_are_listed_in_notes() {
        let _host = MockHost::install();
        let table = Table::new("customers");
        let columns: Vec<Column> = ["id", "owner_id", "owner_name", "other"].iter().map(|n| Column::new(n, LogicalType::Text)).collect();
        let mapped = Mapped { columns: BTreeMap::from([("id".into(), "id".into())]), unknown: vec![("owner_".into(), "owner".into())] };
        let divided = split(&table, &columns, &mapped);
        assert_eq!(divided.offered, [3]);
        assert_eq!(divided.mapped, [("id".to_owned(), "id".to_owned())]);
        assert!(divided.notes.iter().filter(|n| n.contains("join column of `owner`")).count() == 2, "{:?}", divided.notes);
    }

    #[test]
    fn primary_key_columns_that_are_not_mapped_are_not_offered() {
        let _host = MockHost::install();
        let mut table = Table::new("t");
        table.primary_key = Fact::Known(vec!["uid".into()]);
        let columns = vec![Column::new("uid", LogicalType::Bigint), Column::new("x", LogicalType::Text)];
        let divided = split(&table, &columns, &Mapped::default());
        assert_eq!(divided.offered, [1]);
        assert!(divided.notes[0].contains("primary key"));
    }

    #[test]
    fn superclass_and_embedded_columns_count_and_unresolved_ones_refuse() {
        let _host = MockHost::install();
        let entity = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class A extends Base {\n @Id Long id;\n @Embedded Address address;\n}\n";
        let base = "package shop;\nimport jakarta.persistence.*;\n@MappedSuperclass\npublic class Base {\n @Column(name=\"made\") java.time.Instant createdAt;\n}\n";
        let address = "package shop;\nimport jakarta.persistence.*;\n@Embeddable\npublic class Address {\n String street;\n}\n";
        let find = |name: &str| -> Result<Option<Found>, String> {
            let (text, file) = match name {
                "Base" => (base, "Base.java"),
                "Address" => (address, "Address.java"),
                other => return Err(format!("`{other}` is not a source of this project")),
            };
            Ok(parse_class(text, file).map(|c| (c, text.to_owned())))
        };
        let subject = Subject { class: parse_class(entity, "/project/A.java").unwrap(), ns: "jakarta.persistence", body_end: 0, table: "a".into() };
        let mapped = mapped_columns(&subject, entity, &find).unwrap();
        assert_eq!(mapped.columns.keys().map(String::as_str).collect::<Vec<_>>(), ["id", "made", "street"]);
        let missing = |name: &str| -> Result<Option<Found>, String> { if name == "Address" { find(name) } else { Err("`Base` is not a source of this project, so the columns it maps are unknown.".into()) } };
        assert!(mapped_columns(&subject, entity, &missing).unwrap_err().contains("unknown"));
        let entity_parent = |name: &str| -> Result<Option<Found>, String> {
            let text = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Base {\n @Id Long id;\n}\n";
            if name == "Base" { Ok(parse_class(text, "Base.java").map(|c| (c, text.to_owned()))) } else { find(name) }
        };
        assert!(mapped_columns(&subject, entity, &entity_parent).unwrap_err().contains("entity or embeddable"));
    }

    #[test]
    fn hierarchy_secondary_table_and_override_layouts_are_refused() {
        let _host = MockHost::install();
        for annotation in ["@Inheritance(strategy = InheritanceType.SINGLE_TABLE)", "@DiscriminatorColumn(name = \"k\")", "@SecondaryTable(name = \"x\")", "@AttributeOverride(name = \"a\", column = @Column(name = \"b\"))"] {
            let entity = format!("package shop;\nimport jakarta.persistence.*;\n@Entity\n{annotation}\npublic class A {{\n @Id Long id;\n}}\n");
            let subject = Subject { class: parse_class(&entity, "/project/A.java").unwrap(), ns: "jakarta.persistence", body_end: 0, table: "a".into() };
            let message = mapped_columns(&subject, &entity, &|_| Ok(None)).unwrap_err();
            assert!(message.contains("columns are not resolved"), "{annotation}: {message}");
        }
    }

    #[test]
    fn not_offered_where_the_class_is_not_a_plain_single_entity() {
        let (_host, mut plugin, mut doc) = host_with(ENTITY, &[]);
        assert!(offered(&mut plugin, &doc).is_some());
        doc.text = ENTITY.replace("}\n", "}\n\nclass Other {\n}\n");
        assert!(offered(&mut plugin, &doc).is_none());
        doc.text = ENTITY.replace("@Entity", "@Embeddable");
        assert!(offered(&mut plugin, &doc).is_none());
        doc.text = ENTITY.replace("    }\n}\n", "    }\n    }\n");
        assert!(eligible(&Document { text: ENTITY.replace("\n}\n", " }\n"), ..doc.clone() }) == false);
    }

    #[test]
    fn missing_table_missing_source_and_fully_mapped_tables_are_refused() {
        let (host, mut plugin, doc) = host_with(ENTITY, &[("db/schema.sql", "CREATE TABLE regions (id bigserial PRIMARY KEY);\n")]);
        let message = format!("{:?}", run(&host, &mut plugin, &doc, &[]).unwrap_err());
        assert!(message.contains("No SQL file of the project creates table `customers`"), "{message}");

        let (host, mut plugin, doc) = host_with(ENTITY, &[("db/schema.sql", "CREATE TABLE regions (id bigserial PRIMARY KEY);\nCREATE TABLE audit_customers (id bigserial PRIMARY KEY);\n")]);
        let message = format!("{:?}", run(&host, &mut plugin, &doc, &[&["sql:0"]]).unwrap_err());
        assert!(message.contains("was not found in the chosen source"), "{message}");

        let all = "CREATE TABLE customers (id bigserial PRIMARY KEY, email varchar(5), full_name varchar(5), region_id bigint);\n";
        let (host, mut plugin, doc) = host_with(ENTITY, &[("db/schema.sql", all)]);
        let message = format!("{:?}", run(&host, &mut plugin, &doc, &[&["sql:0"]]).unwrap_err());
        assert!(message.contains("already mapped"), "{message}");
    }

    #[test]
    fn default_table_name_follows_the_naming_template_and_bounds_hold() {
        let entity = ENTITY.replace("@Table(name = \"customers\")\n", "").replace("import jakarta.persistence.Table;\n", "");
        let many: String = (0..201).map(|i| format!(", c{i} int")).collect();
        let sql = format!("CREATE TABLE customer (id bigserial PRIMARY KEY, email varchar(5){many});\n");
        let (host, mut plugin, doc) = host_with(&entity, &[("db/schema.sql", &sql)]);
        let message = format!("{:?}", run(&host, &mut plugin, &doc, &[&["sql:0"]]).unwrap_err());
        assert!(message.contains("more than the 200"), "{message}");

        let sql = format!("CREATE TABLE customer (id bigserial PRIMARY KEY{});\n", (0..60).map(|i| format!(", c{i} int")).collect::<String>());
        let (host, mut plugin, doc) = host_with(&entity, &[("db/schema.sql", &sql)]);
        let names: Vec<String> = (0..51).map(|i| format!("c{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let message = format!("{:?}", run(&host, &mut plugin, &doc, &[&["sql:0"], &refs]).unwrap_err());
        assert!(message.contains("between one and 50"), "{message}");
    }

    #[test]
    fn cancelling_any_prompt_applies_nothing_and_stale_sources_refuse() {
        let (host, mut plugin, doc) = host_with(ENTITY, &[("db/schema.sql", SQL)]);
        for keep in 0..4 {
            let action = offered(&mut plugin, &doc).unwrap();
            let all: [&[&str]; 4] = [&["sql:0"], &["balance"], &[], &[]];
            answer(&host, &all[..keep]);
            host.answer_next_prompt(None);
            assert!(matches!(plugin.resolve_file_action(&doc, action), Err(Error::Cancelled)), "cancel at {keep}");
        }
        let action = offered(&mut plugin, &doc).unwrap();
        host.type_in_editor("// changed\n");
        assert!(plugin.resolve_file_action(&doc, action).is_err());
    }

    #[test]
    fn crlf_entities_keep_their_line_endings() {
        let crlf = ENTITY.replace('\n', "\r\n");
        let (host, mut plugin, doc) = host_with(&crlf, &[("db/schema.sql", SQL)]);
        let result = run(&host, &mut plugin, &doc, &[&["sql:0"], &["balance"], &[], &[]]).unwrap();
        let text = after(&doc, &result);
        assert!(!text.replace("\r\n", "").contains('\n') && text.contains("private BigDecimal balance;"), "{text:?}");
    }

    #[test]
    fn kotlin_entities_get_properties_without_accessors() {
        let entity = "package shop\n\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\nimport jakarta.persistence.Table\n\n@Entity\n@Table(name = \"customers\")\nclass Customer {\n\n    @Id\n    var id: Long? = null\n}\n";
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read", "storage"]);
        host.add_file("db/schema.sql", SQL);
        let path = "/project/src/main/kotlin/shop/Customer.kt";
        host.open_editor(path, "kotlin", entity, &[]);
        let doc = Document { path: path.into(), language: "kotlin".into(), text: entity.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let result = run(&host, &mut plugin, &doc, &[&["sql:0"], &["balance", "created_at"], &[], &[]]).unwrap();
        let text = after(&doc, &result);
        assert!(text.contains("import java.math.BigDecimal\n") && text.contains("import jakarta.persistence.Column\n"), "{text}");
        assert!(text.contains("    @Column(precision = 12, scale = 2)\n    var balance: BigDecimal? = null\n") && !text.contains("fun "), "{text}");
    }

    fn profile() -> Profile {
        Profile {
            name: "dev".into(),
            dbms: Dbms::PostgreSql,
            url_template: "jdbc:postgresql://db.example:5432/app".into(),
            user: Some("app".into()),
            driver_jar: Some("/m2/postgresql-42.7.4.jar".into()),
            schema_filter: SchemaFilter::default(),
            spring_source: None,
        }
    }

    fn live_columns() -> Value {
        json!([
            {"name":"id","typeName":"int8","dataType":-5,"nullable":0,"autoIncrement":"YES","generated":"NO","position":1},
            {"name":"email","typeName":"varchar","dataType":12,"size":120,"nullable":0,"autoIncrement":"NO","generated":"NO","position":2},
            {"name":"full_name","typeName":"varchar","dataType":12,"size":80,"nullable":1,"autoIncrement":"NO","generated":"NO","position":3},
            {"name":"balance","typeName":"numeric","dataType":2,"size":12,"scale":2,"nullable":1,"autoIncrement":"NO","generated":"NO","position":4},
            {"name":"region_id","typeName":"int8","dataType":-5,"nullable":1,"autoIncrement":"NO","generated":"NO","position":5},
            {"name":"nick","typeName":"varchar","dataType":12,"size":20,"nullable":1,"autoIncrement":"NO","generated":"NO","position":6}
        ])
    }

    fn serve(host: &MockHost, table: &'static str) -> std::rc::Rc<std::cell::RefCell<Vec<(Vec<String>, String)>>> {
        let seen: std::rc::Rc<std::cell::RefCell<Vec<(Vec<String>, String)>>> = Default::default();
        let log = seen.clone();
        host.on_process("java", move |args, stdin| {
            let stdin = stdin.unwrap_or("");
            log.borrow_mut().push((args.to_vec(), stdin.to_owned()));
            let request: Value = serde_json::from_str(stdin).unwrap();
            let id = &request["id"];
            let body = match request["op"].as_str().unwrap() {
                "describeTables" => {
                    let tables: Vec<Value> = request["args"]["tables"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|n| {
                            let name = n.as_str().unwrap();
                            if name == table {
                                json!({"name":name,"schema":"public","found":true,"type":"TABLE","columns":live_columns(),"primaryKey":{"name":"pk","columns":["id"]},"foreignKeys":[],"indexes":[]})
                            } else {
                                json!({"name":name,"schema":"public","found":false})
                            }
                        })
                        .collect();
                    json!({"v":1,"id":id,"ok":true,"truncated":false,"next":null,"result":{"tables":tables}})
                }
                other => json!({"v":1,"id":id,"ok":false,"error":{"code":"Unsupported","message":other,"sqlState":null,"vendorCode":0,"retryable":false}}),
            };
            Ok(ProcessOutput { status: Some(0), stdout: format!("{body}\n"), stderr: String::new() })
        });
        seen
    }

    fn no_secret(host: &MockHost, seen: &std::rc::Rc<std::cell::RefCell<Vec<(Vec<String>, String)>>>, result: &FileAction) {
        assert!(host.notifications().iter().all(|n| !n.title.contains(SENTINEL) && !n.body.contains(SENTINEL)));
        assert!(host.storage().values().all(|v| !v.contains(SENTINEL)));
        assert!(host.logs().iter().all(|(_, m)| !m.contains(SENTINEL)));
        assert!(seen.borrow().iter().all(|(args, _)| args.iter().all(|a| !a.contains(SENTINEL))), "argv leaks the password");
        assert!(!format!("{result:?}").contains(SENTINEL));
        assert!(seen.borrow().iter().any(|(_, stdin)| stdin.contains(SENTINEL)), "the helper receives the password on stdin only");
    }

    #[test]
    fn live_snapshot_is_read_only_confirmed_first_and_never_leaks_the_password() {
        let (host, mut plugin, doc) = host_with(ENTITY, &[]);
        host::storage::set(jdbc_connect::PROFILES_KEY, &profiles_json(&[profile()])).unwrap();
        let seen = serve(&host, "customers");
        let result = run(&host, &mut plugin, &doc, &[&["db:0"], &[], &[SENTINEL], &["balance", "nick"], &[], &[]]).unwrap();
        let text = after(&doc, &result);
        assert!(text.contains("@Column(precision = 12, scale = 2)\n    private BigDecimal balance;") && text.contains("@Column(length = 20)\n    private String nick;"), "{text}");
        assert!(!text.contains("fullName;\n\n    @Column") && !text.contains("private Long regionId;"), "{text}");
        let requests: Vec<Value> = seen.borrow().iter().map(|(_, stdin)| serde_json::from_str(stdin).unwrap()).collect();
        assert!(requests.iter().all(|r| r["op"] == "describeTables"), "metadata only: {requests:?}");
        no_secret(&host, &seen, &result);
        assert_eq!(host.editor_text(), ENTITY);
    }

    #[test]
    fn live_table_missing_ambiguous_or_view_is_refused_with_the_reason() {
        let (host, mut plugin, doc) = host_with(ENTITY, &[]);
        host::storage::set(jdbc_connect::PROFILES_KEY, &profiles_json(&[profile()])).unwrap();
        let _seen = serve(&host, "other");
        let message = format!("{:?}", run(&host, &mut plugin, &doc, &[&["db:0"], &[], &["pw"]]).unwrap_err());
        assert!(message.contains("`customers` was not found in the database"), "{message}");
    }

    #[test]
    fn denied_process_permission_skips_the_live_source_and_falls_back_to_sql() {
        let (host, mut plugin, doc) = host_with(ENTITY, &[("db/schema.sql", SQL)]);
        host::storage::set(jdbc_connect::PROFILES_KEY, &profiles_json(&[profile()])).unwrap();
        host.on_process("java", |_, _| Err(Error::Denied("process:java".into())));
        let result = run(&host, &mut plugin, &doc, &[&["db:0"], &[], &["pw"], &["sql:0"], &["balance"], &[], &[]]).unwrap();
        assert!(after(&doc, &result).contains("private BigDecimal balance;"));
        assert!(host.notifications().iter().any(|n| n.body.contains("live database source is skipped")));
        let (host, mut plugin, doc) = host_with(ENTITY, &[]);
        host::storage::set(jdbc_connect::PROFILES_KEY, &profiles_json(&[profile()])).unwrap();
        host.on_process("java", |_, _| Err(Error::Denied("process:java".into())));
        let message = format!("{:?}", run(&host, &mut plugin, &doc, &[&["db:0"], &[], &["pw"]]).unwrap_err());
        assert!(message.contains("process:java"), "{message}");
    }

    #[test]
    fn views_and_unmapped_dbms_are_not_read() {
        let _host = MockHost::install();
        struct Never;
        impl Runner for Never {
            fn run(&mut self, _: &str, _: u64) -> Result<persistence_core::introspect::RunOutput, persistence_core::introspect::LaunchError> {
                panic!("not launched")
            }
        }
        let mut client = Client::new(Never, persistence_core::introspect::Connection { url: "jdbc:h2:mem:x".into(), user: None, driver: persistence_core::introspect::DriverSpec { jars: vec![], class: String::new() }, props: Default::default() }, None);
        let error = read_table(&mut client, Dbms::H2, &[], "customers", &|| false).unwrap_err();
        assert!(live_message(&error).contains("not mapped yet"));
    }

    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_API / JPA_JAVAX_API jar paths (and Kotlin env vars)"]
    fn edited_entities_compile_for_java_kotlin_and_both_namespaces() {
        let base = std::env::temp_dir().join(format!("jpa-entity-attrs-{}", std::process::id()));
        let sql = "CREATE TABLE customers (id bigserial PRIMARY KEY, email varchar(120) NOT NULL, created_at timestamptz NOT NULL, balance numeric(12,2), legacy_code varchar(10), born date, token uuid, flag boolean, blob bytea, owner_id bigint REFERENCES customers (id), full_name varchar(5));\n";
        for kotlin in [false, true] {
            for ns in ["jakarta.persistence", "javax.persistence"] {
                let (entity, path, language) = if kotlin {
                    (format!("package shop\n\nimport {ns}.Entity\nimport {ns}.Id\nimport {ns}.Table\n\n@Entity\n@Table(name = \"customers\")\nclass Customer {{\n\n    @Id\n    var id: Long? = null\n}}\n"), "/project/src/main/kotlin/shop/Customer.kt", "kotlin")
                } else {
                    (format!("package shop;\n\nimport {ns}.Entity;\nimport {ns}.Id;\nimport {ns}.Table;\n\n@Entity\n@Table(name = \"customers\")\npublic class Customer {{\n\n    @Id\n    private Long id;\n\n    public Long getId() {{\n        return id;\n    }}\n}}\n"), ENTITY_PATH, "java")
                };
                let host = MockHost::install().with_capabilities(&["project.read", "editor.read", "storage"]);
                host.add_file("db/schema.sql", sql);
                host.open_editor(path, language, &entity, &[]);
                let doc = Document { path: path.into(), language: language.into(), text: entity, version: 1 };
                let mut plugin = JpaAssist::default();
                let columns = ["email", "created_at", "balance", "legacy_code", "born", "token", "flag", "blob", "owner_id", "full_name"];
                let result = run(&host, &mut plugin, &doc, &[&["sql:0"], &columns, &[], &[]]).unwrap();
                let text = after(&doc, &result);
                let directory = base.join(format!("{kotlin}-{ns}"));
                std::fs::create_dir_all(&directory).unwrap();
                let file = directory.join(if kotlin { "Customer.kt" } else { "Customer.java" });
                std::fs::write(&file, &text).unwrap();
                let api = std::env::var(if ns == "jakarta.persistence" { "JPA_JAKARTA_API" } else { "JPA_JAVAX_API" }).expect("explicit persistence API jar");
                if kotlin {
                    let compiler = std::env::var("KOTLIN_COMPILER_CLASSPATH").expect("Kotlin compiler classpath");
                    let stdlib = std::env::var("KOTLIN_STDLIB").expect("Kotlin stdlib");
                    let classpath = format!("{api}:{stdlib}");
                    let result = std::process::Command::new("java")
                        .args(["-classpath", &compiler, "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler", "-no-stdlib", "-no-reflect", "-jvm-target", "17", "-classpath", &classpath, "-d"])
                        .arg(&directory)
                        .arg(&file)
                        .output()
                        .expect("java installed");
                    assert!(result.status.success(), "kotlin {ns}: {}\n{}\n{text}", String::from_utf8_lossy(&result.stderr), String::from_utf8_lossy(&result.stdout));
                } else {
                    let result = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &api, "-d"]).arg(&directory).arg(&file).output().expect("javac installed");
                    assert!(result.status.success(), "java {ns}: {}\n{text}", String::from_utf8_lossy(&result.stderr));
                }
            }
        }
        std::fs::remove_dir_all(base).unwrap();
    }
}
