//! Read-only inventory behind the Designer's "From database / SQL" panel (G087, G084, G098). The SQL file is parsed by the
//! same `ddl_parse` the file actions use and compared with the model scan of the project; nothing is written and no
//! database is contacted. Starting "Generate entities from SQL file…" or "Add attributes from database columns…" stays
//! with the editor's code actions: a web view has no way to run one, so the panel only names them.
use super::*;
use super::migration_action::absolute;
use persistence_core::ddl_parse::{self, Dialect as SqlDialect, ParsedDdl};
use persistence_core::schema::{Column, Fact, SchemaSnapshot};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const MAX_SQL_FILES: u32 = 100;
const MAX_SQL: usize = 512 << 10;
const MAX_RELATIONS: usize = 200;
const MAX_COLUMNS: usize = 100;
const MAX_REFERENCES: usize = 50;
const MAX_TYPE_ROWS: usize = 40;
const DEFAULT_SCHEMA: &str = "(default)";

pub(crate) fn sources(project: &Project, root: &str) -> Result<Value, Error> {
    let mut files: Vec<String> = project
        .find_files("**/*.sql", MAX_SQL_FILES)?
        .into_iter()
        .map(|file| absolute(root, &file))
        .filter(|file| path(file).as_deref() == Some(file.as_str()) && inside(root, file))
        .collect();
    files.sort();
    let prefix = format!("{}/", root.trim_end_matches('/'));
    let sql_files: Vec<Value> = files.iter().map(|file| json!({"path":file,"label":file.strip_prefix(&prefix).unwrap_or(file)})).collect();
    let connections: Vec<Value> = crate::jdbc_connect::saved_profiles()
        .unwrap_or_default()
        .iter()
        .take(20)
        .map(|profile| json!({"name":profile.name,"dbms":profile.dbms.label(),"target":profile.target()}))
        .collect();
    Ok(json!({
        "kind":"reverse-sources","sqlFiles":sql_files,"connections":connections,
        "generateAction":super::ddl_entities_action::TITLE,"attributesAction":super::ddl_entities_action::ATTRS_TITLE,
    }))
}

fn bare(name: &str) -> String {
    name.rsplit('.').next().unwrap_or(name).to_ascii_lowercase()
}

fn schema_of(name: &str) -> String {
    name.split_once('.').map_or(DEFAULT_SCHEMA.to_owned(), |(schema, _)| schema.to_owned())
}

fn column_names(columns: &Fact<Vec<Column>>) -> Option<BTreeSet<String>> {
    columns.known().map(|columns| columns.iter().map(|c| c.name.to_ascii_lowercase()).collect())
}

fn type_text(column: &Column) -> String {
    match column.sql_type.known() {
        Some(sql) if !sql.raw.is_empty() => sql.raw.clone(),
        Some(sql) => format!("{:?}", sql.logical).to_ascii_lowercase(),
        None => "unknown".to_owned(),
    }
}

fn java_of(column: &Column) -> String {
    column.sql_type.known().map_or("(no Java mapping)", |sql| super::ddl_entities_action::java_type(sql.logical).0).to_owned()
}

struct Relation<'a> {
    name: &'a str,
    view: bool,
    columns: Vec<&'a Column>,
}

fn relations(parsed: &ParsedDdl) -> Vec<Relation<'_>> {
    let tables = parsed.snapshot.tables.iter().map(|table| Relation { name: &table.name, view: false, columns: table.columns.known().map(|c| c.iter().collect()).unwrap_or_default() });
    let views = parsed.views.iter().map(|view| Relation { name: &view.name, view: true, columns: view.columns.iter().collect() });
    tables.chain(views).collect()
}

fn mapped_columns(model: Option<&SchemaSnapshot>, name: &str) -> Option<BTreeSet<String>> {
    let table = model?.tables.iter().find(|table| bare(&table.name) == bare(name))?;
    Some(column_names(&table.columns).unwrap_or_default())
}

fn dialect_of(text: &str, requested: Option<&str>) -> Result<(SqlDialect, &'static str, bool), Error> {
    match requested {
        Some("postgresql") => Ok((SqlDialect::PostgreSql, "postgresql", false)),
        Some("mysql") => Ok((SqlDialect::MySql, "mysql", false)),
        Some(_) => Err("Choose PostgreSQL or MySQL.".into()),
        None => Ok(match ddl_parse::detect_dialect(text) {
            Some(SqlDialect::MySql) => (SqlDialect::MySql, "mysql", true),
            Some(SqlDialect::PostgreSql) => (SqlDialect::PostgreSql, "postgresql", true),
            None => (SqlDialect::PostgreSql, "postgresql", false),
        }),
    }
}

pub(crate) fn inventory(project: &Project, root: &str, file: &str, dialect: Option<&str>, schema: Option<&str>, selected: Option<&str>) -> Result<Value, Error> {
    if path(file).as_deref() != Some(file) || !inside(root, file) || !file.to_ascii_lowercase().ends_with(".sql") {
        return Err("Choose a .sql file of this project.".into());
    }
    let text = project.read_file(file)?;
    if text.len() > MAX_SQL {
        return Err("The SQL file is over the 512 KiB bound.".into());
    }
    let (sql_dialect, dialect_name, detected) = dialect_of(&text, dialect)?;
    let parsed = ddl_parse::parse_ddl(&text, sql_dialect).map_err(|error| Error::from(format!("The SQL file cannot be read: {error:?}")))?;
    let model = super::migration_action::scan(project, root).ok();
    let all = relations(&parsed);
    let schemas: BTreeSet<String> = all.iter().map(|relation| schema_of(relation.name)).collect();
    let chosen = schema.filter(|wanted| schemas.contains(*wanted));
    let in_schema: Vec<&Relation> = all.iter().filter(|relation| chosen.is_none_or(|wanted| schema_of(relation.name) == wanted)).collect();
    let listed: Vec<Value> = in_schema
        .iter()
        .take(MAX_RELATIONS)
        .map(|relation| {
            let mapped = mapped_columns(model.as_ref(), relation.name);
            let unmapped = mapped.as_ref().map_or(0, |known| relation.columns.iter().filter(|c| !known.contains(&c.name.to_ascii_lowercase())).count());
            let group = match (&mapped, relation.view) {
                (Some(_), _) => "mapped",
                (None, true) => "view",
                (None, false) => "table",
            };
            json!({"name":relation.name,"group":group,"view":relation.view,"columns":relation.columns.len(),"unmapped":unmapped})
        })
        .collect();
    let detail = selected.and_then(|name| in_schema.iter().find(|relation| relation.name == name)).map(|relation| detail_of(&parsed, model.as_ref(), relation));
    let mut types: BTreeMap<(String, String), usize> = BTreeMap::new();
    for relation in &in_schema {
        for column in &relation.columns {
            *types.entry((type_text(column), java_of(column))).or_default() += 1;
        }
    }
    let type_mappings: Vec<Value> = types.into_iter().take(MAX_TYPE_ROWS).map(|((sql, java), count)| json!({"sql":sql,"java":java,"count":count})).collect();
    Ok(json!({
        "kind":"reverse","path":file,"dialect":dialect_name,"dialectDetected":detected,
        "schemas":schemas.into_iter().map(|s| json!({"value":s,"label":s})).collect::<Vec<_>>(),
        "schema":chosen,"relations":listed,"truncated":in_schema.len() > MAX_RELATIONS,
        "selected":detail,"typeMappings":type_mappings,"skipped":parsed.skipped.len() + parsed.skipped_overflow,
        "modelScanned":model.is_some(),"modelComplete":model.as_ref().is_none_or(|m| m.complete),
        "generateAction":super::ddl_entities_action::TITLE,"attributesAction":super::ddl_entities_action::ATTRS_TITLE,
    }))
}

fn detail_of(parsed: &ParsedDdl, model: Option<&SchemaSnapshot>, relation: &Relation) -> Value {
    let mapped = mapped_columns(model, relation.name);
    let key: BTreeSet<String> = parsed
        .snapshot
        .tables
        .iter()
        .find(|table| table.name == relation.name)
        .and_then(|table| table.primary_key.known().map(|k| k.iter().map(|c| c.to_ascii_lowercase()).collect()))
        .unwrap_or_default();
    let columns: Vec<Value> = relation
        .columns
        .iter()
        .take(MAX_COLUMNS)
        .map(|column| {
            let lower = column.name.to_ascii_lowercase();
            json!({
                "name":column.name,"sql":type_text(column),"java":java_of(column),"key":key.contains(&lower),
                "nullable":column.nullable.known().copied(),"mapped":mapped.as_ref().is_some_and(|known| known.contains(&lower)),
            })
        })
        .collect();
    let names: BTreeSet<String> = relation.columns.iter().map(|c| c.name.to_ascii_lowercase()).collect();
    let references: Vec<Value> = parsed
        .snapshot
        .tables
        .iter()
        .find(|table| table.name == relation.name)
        .and_then(|table| table.foreign_keys.known())
        .map(|keys| {
            keys.iter()
                .take(MAX_REFERENCES)
                .map(|fk| {
                    json!({
                        "name":fk.name,"columns":fk.columns,"refTable":fk.ref_table,"refColumns":fk.ref_columns.known(),
                        "columnPresent":fk.columns.iter().all(|c| names.contains(&c.to_ascii_lowercase())),
                        "refMapped":mapped_columns(model, &fk.ref_table).is_some(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({"name":relation.name,"view":relation.view,"mappedEntity":mapped.is_some(),"columns":columns,"references":references,"columnsTruncated":relation.columns.len() > MAX_COLUMNS})
}

/// The effective settings that shape a CREATE TABLE statement. Read-only: a web view cannot write settings.
pub(crate) fn create_table_options() -> Value {
    let text = |key: &str, empty: &str| host::setting_string(key).map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()).unwrap_or_else(|| empty.to_owned());
    json!([
        {"setting":"jpaAssist.identifierPolicy","label":"Identifier case policy","value":text("jpaAssist.identifierPolicy","default")},
        {"setting":"jpaAssist.maxIdentifierLength","label":"Longest identifier","value":text("jpaAssist.maxIdentifierLength","unlimited")},
        {"setting":"jpaAssist.primaryKeyConstraintName","label":"Primary key constraint name","value":text("jpaAssist.primaryKeyConstraintName","unnamed")},
        {"setting":"jpaAssist.indexForeignKeys","label":"Index foreign keys","value":if host::setting_bool("jpaAssist.indexForeignKeys").unwrap_or(false) { "yes" } else { "no" }},
    ])
}

/// Per change type: the level override and the location in force, with how many of the plan's changes use it.
pub(crate) fn statement_types(plan: &persistence_core::schema::ChangePlan, routes: &[(String, super::change_routing::Location)]) -> Result<Value, Error> {
    let levels = super::danger_levels::overrides()?;
    let rows: Vec<Value> = super::danger_levels::CHANGE_TYPES
        .iter()
        .map(|kind| {
            let level = levels.iter().rev().find(|(name, _)| name == kind).map(|(_, level)| level.label());
            let location = routes.iter().rev().find(|(name, _)| name == kind).map_or("primary", |(_, location)| location.label());
            let count = plan.changes.iter().filter(|planned| super::danger_levels::change_type(&planned.change) == *kind).count();
            json!({"type":kind,"levelOverride":level,"location":location,"count":count})
        })
        .collect();
    Ok(json!(rows))
}
