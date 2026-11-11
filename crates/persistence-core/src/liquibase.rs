//! Liquibase XML changelog text for a `ChangePlan`. Nothing here executes anything or touches a database.
//!
//! Policy is the one of `sql::emit`: `Safe` and `Rewrite` changes are emitted; `NeedsBackfill`, `DataLoss` and
//! `Ambiguous` ones become `<!-- SKIPPED (...) -->` comments (with the would-be changeSet in a second comment)
//! unless `EmitOptions::allow` names their danger; a rename is only emitted for an explicit decision (as
//! `renameColumn`); a fact that cannot be rendered skips that change with a visible comment. One changeSet per
//! emitted change (the Liquibase docs recommend one change type per changeSet). Only XML is produced; YAML and
//! JSON syntax was not verified for this increment.
//!
//! Verified against the Liquibase 4.29 `dbchangelog` XSD (element and attribute names, required attributes) and
//! docs.liquibase.com pages: XML format (root, `dbchangelog-latest.xsd`), data type handling (generic type
//! names), create-table (`varchar(255)` length in the type string, nested `constraints`), modify-data-type,
//! add/drop-not-null-constraint (`columnDataType` required on MySQL, MariaDB, MSSQL, Informix), rename-column
//! (`columnDataType` required on MySQL and MariaDB), add-foreign-key-constraint (comma-separated columns),
//! create-index, drop-index (`tableName` required on MySQL), add-unique-constraint, changeset page
//! (author:id plus file path uniqueness). Not verified: decimal(p,s) type strings (not shown in the docs read,
//! so decimals are skipped), time-zone timestamps, binary type choice, the `dropUniqueConstraint` page
//! (attributes taken from the XSD only), composite `addPrimaryKey` runtime behavior (XSD only).
use crate::schema::{Change, ChangePlan, Column, Danger, Fact, ForeignKey, Index, LogicalType, PlannedChange, Table, UniqueConstraint};
use crate::sql::{BackfillValue, EmitOptions, Skip, backfill_for, check_backfills, decided, same};

#[derive(Clone, Debug, Default)]
pub struct LiquibaseOptions {
    pub emit: EmitOptions,
    /// Literal changeSet author (not empty, at most 128 characters, no `${`).
    pub author: String,
    /// changeSet ids are `<prefix>-<n>`; letters, digits, `.`, `_`, `-`, at most 64 characters.
    pub id_prefix: String,
    pub defaults: ChangeSetDefaults,
}

/// What Liquibase does when a changeSet precondition fails or errors: the four values of the docs' "onFail/onError
/// precondition values" table (also the XSD's `onChangeSetPreconditionErrorOrFail`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreconditionAction {
    Halt,
    Continue,
    MarkRan,
    Warn,
}

impl PreconditionAction {
    pub fn name(self) -> &'static str {
        match self {
            Self::Halt => "HALT",
            Self::Continue => "CONTINUE",
            Self::MarkRan => "MARK_RAN",
            Self::Warn => "WARN",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        [Self::Halt, Self::Continue, Self::MarkRan, Self::Warn].into_iter().find(|action| action.name() == text)
    }
}

/// Optional changeSet knobs; `None` writes nothing. All are attributes or children the 4.29 XSD defines on `changeSet`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangeSetAttributes {
    pub fail_on_error: Option<bool>,
    pub run_on_change: Option<bool>,
    /// Comma-separated literal list; each item ASCII letters, digits, `.`, `_`, `-`.
    pub context: Option<String>,
    /// Same rule as `context`.
    pub labels: Option<String>,
    /// Either one set renders `<preConditions>` for `createTable` and `addColumn` changeSets only.
    pub on_fail: Option<PreconditionAction>,
    pub on_error: Option<PreconditionAction>,
    /// `<rollback/>` in changeSets of `ROLLBACK_UNSUPPORTED` types.
    pub empty_rollback: Option<bool>,
}

impl ChangeSetAttributes {
    /// Fields set in `over` replace the ones here.
    pub fn overlay(&mut self, over: &Self) {
        self.fail_on_error = over.fail_on_error.or(self.fail_on_error);
        self.run_on_change = over.run_on_change.or(self.run_on_change);
        self.context = over.context.clone().or(self.context.take());
        self.labels = over.labels.clone().or(self.labels.take());
        self.on_fail = over.on_fail.or(self.on_fail);
        self.on_error = over.on_error.or(self.on_error);
        self.empty_rollback = over.empty_rollback.or(self.empty_rollback);
    }
}

/// `all` applies to every changeSet; a `by_type` entry (element name, see `CHANGE_TYPES`) overrides field by field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangeSetDefaults {
    pub all: ChangeSetAttributes,
    pub by_type: Vec<(String, ChangeSetAttributes)>,
}

pub const CHANGE_TYPES: [&str; 16] = [
    "createTable", "dropTable", "addColumn", "dropColumn", "modifyDataType", "dropNotNullConstraint", "addNotNullConstraint",
    "dropForeignKeyConstraint", "dropUniqueConstraint", "dropIndex", "addUniqueConstraint", "createIndex", "addForeignKeyConstraint",
    "renameColumn", "update", "addPrimaryKey",
];

/// Change types whose docs page lists "Auto Rollback: No" for every database (docs.liquibase.com change-types pages).
pub const ROLLBACK_UNSUPPORTED: [&str; 6] =
    ["dropTable", "dropColumn", "modifyDataType", "dropForeignKeyConstraint", "dropUniqueConstraint", "dropIndex"];

impl ChangeSetDefaults {
    fn for_type(&self, kind: &str) -> ChangeSetAttributes {
        let mut merged = self.all.clone();
        for (_, over) in self.by_type.iter().filter(|(name, _)| name == kind) {
            merged.overlay(over);
        }
        merged
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiquibaseError {
    InvalidIdentifier { name: String, reason: &'static str },
    InvalidOption { option: &'static str, reason: &'static str },
    UnknownRenameDecision { table: String, from: String, to: String },
    InvalidBackfill { table: String, column: String, reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeSet {
    pub danger: Danger,
    pub reason: String,
    /// `Some` for emitted changeSets.
    pub id: Option<String>,
    pub notes: Vec<String>,
    /// The change elements (already escaped); present for emitted and policy-skipped changes.
    pub body: Option<String>,
    pub skip: Option<Skip>,
    /// Extra changeSet attributes, each with a leading space; empty by default.
    pub attributes: String,
    /// Rendered `<preConditions>` block (ends with a newline), if any.
    pub preconditions: Option<String>,
    pub empty_rollback: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmittedChangelog {
    pub author: String,
    pub changesets: Vec<ChangeSet>,
    pub plan_notes: Vec<String>,
}

impl EmittedChangelog {
    pub fn emitted(&self) -> usize {
        self.changesets.iter().filter(|s| s.skip.is_none()).count()
    }

    pub fn unsupported(&self) -> usize {
        self.changesets.iter().filter(|s| matches!(s.skip, Some(Skip::Unsupported(_)))).count()
    }

    pub fn policy_skipped(&self) -> usize {
        self.changesets.iter().filter(|s| matches!(s.skip, Some(Skip::Policy(_)))).count()
    }

    /// Text of the changelog file; `\n` line endings, well-formed XML.
    pub fn render(&self) -> String {
        let mut out = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<databaseChangeLog\n    xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\"\n    xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"\n    xsi:schemaLocation=\"http://www.liquibase.org/xml/ns/dbchangelog http://www.liquibase.org/xml/ns/dbchangelog/dbchangelog-latest.xsd\">\n\n",
        );
        out.push_str("    <!-- Generated from a schema diff. Review before applying; nothing was executed. -->\n");
        for note in &self.plan_notes {
            out.push_str(&format!("    <!-- NOTE: {} -->\n", comment(note)));
        }
        for set in &self.changesets {
            out.push('\n');
            let reason = comment(&set.reason);
            match &set.skip {
                None => out.push_str(&format!("    <!-- [{:?}] {reason} -->\n", set.danger)),
                Some(Skip::Policy(danger)) => out.push_str(&format!("    <!-- SKIPPED ({danger:?}): {reason} -->\n")),
                Some(Skip::Unsupported(why)) => {
                    out.push_str(&format!("    <!-- SKIPPED (unsupported): {}; plan reason: {reason} -->\n", comment(why)))
                }
            }
            for note in &set.notes {
                out.push_str(&format!("    <!-- REVIEW: {} -->\n", comment(note)));
            }
            let Some(body) = &set.body else {
                continue;
            };
            match &set.id {
                Some(id) => out.push_str(&format!(
                    "    <changeSet id=\"{}\" author=\"{}\"{}>\n{}{body}{}    </changeSet>\n",
                    esc(id),
                    esc(&self.author),
                    set.attributes,
                    set.preconditions.as_deref().unwrap_or(""),
                    if set.empty_rollback { format!("{PAD}<rollback/>\n") } else { String::new() },
                )),
                None => out.push_str(&format!("    <!-- would be:\n{}    -->\n", comment_block(body))),
            }
        }
        out.push_str("\n</databaseChangeLog>\n");
        out
    }
}

fn xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r') || matches!(c as u32, 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

fn esc(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// Text safe inside an XML comment: no control or invalid characters, no `--`, no trailing `-`.
fn comment(text: &str) -> String {
    let mut out: String = text.chars().map(|c| if c.is_control() || !xml_char(c) { ' ' } else { c }).collect();
    while out.contains("--") {
        out = out.replace("--", "- -");
    }
    if out.ends_with('-') {
        out.push(' ');
    }
    out
}

/// Multi-line variant of `comment` (keeps newlines).
fn comment_block(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        out.push_str(&comment(line));
        out.push('\n');
    }
    out
}

fn attr_value(name: &str) -> Result<String, LiquibaseError> {
    let bad = |reason| Err(LiquibaseError::InvalidIdentifier { name: comment(name), reason });
    if name.is_empty() {
        return bad("empty");
    }
    if name.chars().any(char::is_control) {
        return bad("contains a control character");
    }
    if name.chars().any(|c| !xml_char(c)) {
        return bad("contains a character that is not valid in XML");
    }
    Ok(esc(name))
}

/// `<name a="v" b="v"` without the closing `/>` or `>`.
fn start(element: &str, attrs: &[(&str, &str)]) -> Result<String, LiquibaseError> {
    let mut out = format!("<{element}");
    for (key, value) in attrs {
        out.push_str(&format!(" {key}=\"{}\"", attr_value(value)?));
    }
    Ok(out)
}

fn list(names: &[String]) -> Result<String, LiquibaseError> {
    if names.is_empty() {
        return Err(LiquibaseError::InvalidIdentifier { name: String::new(), reason: "empty column list" });
    }
    for name in names {
        attr_value(name)?;
        if name.contains(',') {
            return Err(LiquibaseError::InvalidIdentifier { name: comment(name), reason: "contains a comma (column lists are comma-separated)" });
        }
    }
    Ok(names.join(","))
}

type Unsupported = String;

fn type_string(column: &Column) -> Result<String, Unsupported> {
    use LogicalType::*;
    let Fact::Known(sql_type) = &column.sql_type else {
        return Err(format!("type of column {} is unknown", column.name));
    };
    let no = |what: &str| Err(format!("column {}: {what}", column.name));
    match sql_type.logical {
        Tinyint => Ok("tinyint".into()),
        Smallint => Ok("smallint".into()),
        Integer => Ok("int".into()),
        Bigint => Ok("bigint".into()),
        Boolean => Ok("boolean".into()),
        Real => Ok("float".into()),
        Double => Ok("double".into()),
        Decimal => no("decimal(p,s) type syntax is not verified in the Liquibase docs read; not emitted"),
        Char => match column.length {
            Some(n) if n > 0 => Ok(format!("char({n})")),
            _ => no("fixed-length char needs a valid length"),
        },
        Varchar => match column.length {
            Some(n) if n > 0 => Ok(format!("varchar({n})")),
            _ => no("varchar needs a length; Liquibase's default for a bare varchar is not guessed"),
        },
        Text => Ok("clob".into()),
        Date => Ok("date".into()),
        Time => Ok("time".into()),
        Timestamp => Ok("datetime".into()),
        TimestampTz => no("a time-zone-carrying timestamp type is not verified in the Liquibase docs read"),
        Uuid => Ok("uuid".into()),
        Binary => no("binary storage type choice (BLOB mapping differs per database) is not guessed"),
    }
}

struct Rendered {
    xml: String,
    notes: Vec<String>,
}

/// `<column name type>` with `constraints`; defaults and generation are never emitted, only reported.
fn column_element(indent: usize, column: &Column, primary_key: bool) -> Result<Result<Rendered, Unsupported>, LiquibaseError> {
    let ty = match type_string(column) {
        Ok(ty) => ty,
        Err(why) => return Ok(Err(why)),
    };
    let Some(&nullable) = column.nullable.known() else {
        return Ok(Err(format!("nullability of column {} is unknown", column.name)));
    };
    let mut notes = Vec::new();
    match &column.default {
        Fact::Known(None) => {}
        Fact::Known(Some(_)) => notes.push(format!("default of {} is not emitted (expression dialect not verified)", column.name)),
        Fact::Unknown(_) => notes.push(format!("default of {} is unknown and not emitted", column.name)),
    }
    match &column.generated {
        Fact::Known(false) => {}
        Fact::Known(true) => notes.push(format!("generation (identity/auto-increment/sequence) of {} is not emitted", column.name)),
        Fact::Unknown(_) => notes.push(format!("whether {} is generated is unknown", column.name)),
    }
    let pad = " ".repeat(indent);
    let head = start("column", &[("name", &column.name), ("type", &ty)])?;
    let mut constraints = Vec::new();
    if primary_key {
        constraints.push("primaryKey=\"true\"");
    }
    if !nullable {
        constraints.push("nullable=\"false\"");
    }
    let xml = if constraints.is_empty() {
        format!("{pad}{head}/>\n")
    } else {
        format!("{pad}{head}>\n{pad}    <constraints {}/>\n{pad}</column>\n", constraints.join(" "))
    };
    Ok(Ok(Rendered { xml, notes }))
}

struct Item {
    xml: Result<String, Unsupported>,
    notes: Vec<String>,
    danger: Danger,
    reason: String,
    post: bool,
}

const PAD: &str = "        ";

fn fk_element(table: &str, fk: &ForeignKey) -> Result<Result<String, Unsupported>, LiquibaseError> {
    let Fact::Known(ref_columns) = &fk.ref_columns else {
        return Ok(Err("referenced columns are unknown".into()));
    };
    let Some(name) = &fk.name else {
        return Ok(Err("addForeignKeyConstraint requires a constraintName and the foreign key has none".into()));
    };
    let head = start(
        "addForeignKeyConstraint",
        &[
            ("baseTableName", table),
            ("baseColumnNames", &list(&fk.columns)?),
            ("constraintName", name),
            ("referencedTableName", &fk.ref_table),
            ("referencedColumnNames", &list(ref_columns)?),
        ],
    )?;
    Ok(Ok(format!("{PAD}{head}/>\n")))
}

fn unique_element(table: &str, unique: &UniqueConstraint) -> Result<String, LiquibaseError> {
    let columns = list(&unique.columns)?;
    let head = match &unique.name {
        Some(name) => start("addUniqueConstraint", &[("tableName", table), ("columnNames", &columns), ("constraintName", name)])?,
        None => start("addUniqueConstraint", &[("tableName", table), ("columnNames", &columns)])?,
    };
    Ok(format!("{PAD}{head}/>\n"))
}

fn index_element(table: &str, index: &Index) -> Result<Result<String, Unsupported>, LiquibaseError> {
    let Some(name) = &index.name else {
        return Ok(Err("createIndex requires an indexName and the index has none".into()));
    };
    let mut head = start("createIndex", &[("tableName", table), ("indexName", name)])?;
    if index.unique {
        head.push_str(" unique=\"true\"");
    }
    let mut out = format!("{PAD}{head}>\n");
    for column in &index.columns {
        out.push_str(&format!("{PAD}    {}/>\n", start("column", &[("name", column)])?));
    }
    out.push_str(&format!("{PAD}</createIndex>\n"));
    Ok(Ok(out))
}

fn create_table(table: &Table, danger: Danger, reason: &str) -> Result<Vec<Item>, LiquibaseError> {
    let item = |xml, notes, post| Item { xml, notes, danger, reason: reason.to_owned(), post };
    let refuse = |why: &str| Ok(vec![item(Err(why.to_owned()), vec![], false)]);
    let Fact::Known(columns) = &table.columns else {
        return refuse("columns are unknown");
    };
    if columns.is_empty() {
        return refuse("table has no columns");
    }
    let key: &[String] = match &table.primary_key {
        Fact::Known(key) => key,
        Fact::Unknown(_) => return refuse("primary key is unknown"),
    };
    if let Some(missing) = key.iter().find(|k| !columns.iter().any(|c| same(&c.name, k))) {
        return refuse(&format!("primary key column {missing} is not a column of the table"));
    }
    let single_key = key.len() == 1;
    let mut body = String::new();
    let mut notes = Vec::new();
    for column in columns {
        let primary = single_key && same(&column.name, &key[0]);
        match column_element(12, column, primary)? {
            Ok(rendered) => {
                body.push_str(&rendered.xml);
                notes.extend(rendered.notes);
            }
            Err(why) => return refuse(&why),
        }
    }
    let mut post = Vec::new();
    if key.len() > 1 {
        let head = start("addPrimaryKey", &[("tableName", &table.name), ("columnNames", &list(key)?)])?;
        post.push(item(Ok(format!("{PAD}{head}/>\n")), vec![], true));
    }
    match &table.unique_constraints {
        Fact::Known(uniques) => {
            for unique in uniques {
                post.push(item(Ok(unique_element(&table.name, unique)?), vec![], true));
            }
        }
        Fact::Unknown(_) => notes.push("unique constraints are unknown and not emitted".into()),
    }
    match &table.indexes {
        Fact::Known(indexes) => {
            for index in indexes {
                post.push(item(index_element(&table.name, index)?, vec![], true));
            }
        }
        Fact::Unknown(_) => notes.push("indexes are unknown and not emitted".into()),
    }
    match &table.foreign_keys {
        Fact::Known(fks) => {
            for fk in fks {
                post.push(item(fk_element(&table.name, fk)?, vec![], true));
            }
        }
        Fact::Unknown(_) => notes.push("foreign keys are unknown and not emitted".into()),
    }
    let head = start("createTable", &[("tableName", &table.name)])?;
    let main = format!("{PAD}{head}>\n{body}{PAD}</createTable>\n");
    let mut out = vec![item(Ok(main), notes, false)];
    out.extend(post);
    Ok(out)
}

fn simple_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// addNotNullConstraint with the data type; used after a backfill, where the type is known.
fn not_null_element(table: &str, column: &str, column_type: &str) -> Result<String, LiquibaseError> {
    let head = start("addNotNullConstraint", &[("tableName", table), ("columnName", column), ("columnDataType", column_type)])?;
    Ok(format!("{PAD}{head}/>\n"))
}

/// An `update` that fills NULLs, as its own changeSet between the nullable add and the NOT NULL step.
fn update_element(table: &str, column: &str, value: &BackfillValue) -> Result<String, LiquibaseError> {
    let (attribute, text) = match value {
        BackfillValue::Integer(n) => ("valueNumeric", n.to_string()),
        BackfillValue::Decimal(text) => ("valueNumeric", text.clone()),
        BackfillValue::Boolean(flag) => ("valueBoolean", flag.to_string()),
        BackfillValue::Text(text) => ("value", text.clone()),
    };
    let head = start("update", &[("tableName", table)])?;
    let set = start("column", &[("name", column), (attribute, &text)])?;
    Ok(format!("{PAD}{head}>\n{PAD}    {set}/>\n{PAD}    <where>{column} IS NULL</where>\n{PAD}</update>\n"))
}

fn backfill_items(
    table: &str,
    column: &str,
    value: &BackfillValue,
    definition: Option<&Column>,
    step: &dyn Fn(Result<String, Unsupported>, Vec<String>) -> Item,
) -> Result<Option<Vec<Item>>, LiquibaseError> {
    let Some(definition) = definition else {
        return Ok(None);
    };
    let Ok(column_type) = type_string(definition) else {
        return Ok(None);
    };
    let update_notes = vec![
        "the backfill value was chosen by the reviewer; check it is right for existing rows".to_owned(),
        "each step is its own changeSet, so an earlier step stays applied if a later one fails".to_owned(),
    ];
    let not_null_notes = vec![format!(
        "columnDataType is taken from the model column; check the SQL Liquibase generates for {column} on MySQL, MariaDB, MSSQL and Informix"
    )];
    Ok(Some(vec![
        step(Ok(update_element(table, column, value)?), update_notes),
        step(Ok(not_null_element(table, column, &column_type)?), not_null_notes),
    ]))
}

fn render_change(planned: &PlannedChange, options: &EmitOptions) -> Result<Vec<Item>, LiquibaseError> {
    let (danger, reason) = (planned.danger, planned.reason.clone());
    let one = |xml: Result<String, Unsupported>, notes: Vec<String>| {
        Ok(vec![Item { xml, notes, danger, reason: reason.clone(), post: false }])
    };
    let leaf = |element: &str, attrs: &[(&str, &str)]| -> Result<Result<String, Unsupported>, LiquibaseError> {
        Ok(Ok(format!("{PAD}{}/>\n", start(element, attrs)?)))
    };
    match &planned.change {
        Change::CreateTable { table } => create_table(table, danger, &reason),
        Change::DropTable { table } => one(leaf("dropTable", &[("tableName", table)])?, vec![]),
        Change::AddColumn { table, column } => {
            let rendered = match column_element(12, column, false)? {
                Ok(rendered) => rendered,
                Err(why) => return one(Err(why), vec![]),
            };
            let not_null = column.nullable.known() == Some(&false);
            if not_null && column.default != Fact::Known(None) && backfill_for(options, table, &column.name).is_none() {
                return one(Err("NOT NULL column with a default: the default is not emitted, so the change would fail on populated tables".into()), vec![]);
            }
            if let Some(value) = backfill_for(options, table, &column.name).filter(|_| not_null) {
                let mut nullable = column.clone();
                nullable.nullable = Fact::Known(true);
                let first = column_element(12, &nullable, false)?.expect("rendered above");
                let step = |xml, notes| Item { xml, notes, danger, reason: reason.clone(), post: false };
                if !simple_name(&column.name) {
                    return one(Err("the update's where clause cannot quote this column name; use a plain name or write the update by hand".into()), vec![]);
                }
                let Some(mut steps) = backfill_items(table, &column.name, value, Some(column), &step)? else {
                    return one(Err("the backfill steps need a column data type that could not be rendered".into()), vec![]);
                };
                let head = start("addColumn", &[("tableName", table)])?;
                steps.insert(0, step(Ok(format!("{PAD}{head}>\n{}{PAD}</addColumn>\n", first.xml)), first.notes));
                return Ok(steps);
            }
            let mut notes = rendered.notes;
            if not_null {
                notes.push("NOT NULL column without default needs a backfill".into());
            }
            let head = start("addColumn", &[("tableName", table)])?;
            one(Ok(format!("{PAD}{head}>\n{}{PAD}</addColumn>\n", rendered.xml)), notes)
        }
        Change::DropColumn { table, column } => one(leaf("dropColumn", &[("tableName", table), ("columnName", column)])?, vec![]),
        Change::AlterColumnType { table, column, to, .. } => {
            let ty = match type_string(to) {
                Ok(ty) => ty,
                Err(why) => return one(Err(why), vec![]),
            };
            one(
                leaf("modifyDataType", &[("tableName", table), ("columnName", column), ("newDataType", &ty)])?,
                vec!["modifyDataType changes only the data type; constraints and defaults are not restated".into()],
            )
        }
        Change::AlterNullability { table, column, nullable, column_type } => {
            if let Some(value) = backfill_for(options, table, column).filter(|_| !nullable) {
                let step = |xml, notes| Item { xml, notes, danger, reason: reason.clone(), post: false };
                if !simple_name(column) {
                    return one(Err("the update's where clause cannot quote this column name; use a plain name or write the update by hand".into()), vec![]);
                }
                return match backfill_items(table, column, value, column_type.as_ref(), &step)? {
                    Some(steps) => Ok(steps),
                    None => one(Err("the backfill steps need a column data type that could not be rendered".into()), vec![]),
                };
            }
            let element = if *nullable { "dropNotNullConstraint" } else { "addNotNullConstraint" };
            let mut notes = vec![format!(
                "{element} needs columnDataType on MySQL, MariaDB, MSSQL and Informix; the plan does not carry the column type, so add it before running there"
            )];
            if !nullable {
                notes.push("NOT NULL column without default needs a backfill".into());
            }
            one(leaf(element, &[("tableName", table), ("columnName", column)])?, notes)
        }
        Change::AddForeignKey { table, foreign_key } => one(fk_element(table, foreign_key)?, vec![]),
        Change::DropForeignKey { table, foreign_key } => {
            let Some(name) = &foreign_key.name else {
                return one(Err("the foreign key has no name to drop by".into()), vec![]);
            };
            one(leaf("dropForeignKeyConstraint", &[("baseTableName", table), ("constraintName", name)])?, vec![])
        }
        Change::AddUnique { table, unique } => one(Ok(unique_element(table, unique)?), vec![]),
        Change::DropUnique { table, unique: UniqueConstraint { name, .. } } => {
            let Some(name) = name else {
                return one(Err("the unique constraint has no name to drop by".into()), vec![]);
            };
            one(leaf("dropUniqueConstraint", &[("tableName", table), ("constraintName", name)])?, vec![])
        }
        Change::AddIndex { table, index } => one(index_element(table, index)?, vec![]),
        Change::DropIndex { table, index } => {
            let Some(name) = &index.name else {
                return one(Err("the index has no name to drop by".into()), vec![]);
            };
            one(leaf("dropIndex", &[("tableName", table), ("indexName", name)])?, vec![])
        }
        Change::RenameCandidate { table, from, to } => one(
            leaf("renameColumn", &[("tableName", table), ("oldColumnName", from), ("newColumnName", to)])?,
            vec!["renameColumn needs columnDataType on MySQL and MariaDB; the plan does not carry the column type, so add it before running there".into()],
        ),
    }
}

/// Text of the first ` name="` attribute in already-escaped XML.
fn attr_of<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let rest = &xml[xml.find(&format!(" {name}=\""))? + name.len() + 3..];
    Some(&rest[..rest.find('"')?])
}

fn kind_of(body: &str) -> &str {
    let name = body.trim_start().strip_prefix('<').unwrap_or("");
    &name[..name.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(name.len())]
}

fn apply_defaults(set: &mut ChangeSet, defaults: &ChangeSetDefaults) {
    let Some(body) = set.body.as_deref() else {
        return;
    };
    let kind = kind_of(body).to_owned();
    let wanted = defaults.for_type(&kind);
    let flag = |value: bool| if value { "true" } else { "false" };
    let mut attributes = String::new();
    if let Some(value) = wanted.run_on_change {
        attributes.push_str(&format!(" runOnChange=\"{}\"", flag(value)));
    }
    if let Some(value) = wanted.fail_on_error {
        attributes.push_str(&format!(" failOnError=\"{}\"", flag(value)));
    }
    if let Some(value) = &wanted.context {
        attributes.push_str(&format!(" context=\"{}\"", list_text(value)));
    }
    if let Some(value) = &wanted.labels {
        attributes.push_str(&format!(" labels=\"{}\"", list_text(value)));
    }
    set.attributes = attributes;
    set.empty_rollback = wanted.empty_rollback == Some(true) && ROLLBACK_UNSUPPORTED.contains(&kind.as_str());
    if wanted.on_fail.is_none() && wanted.on_error.is_none() {
        return;
    }
    let check = match (kind.as_str(), attr_of(body, "tableName")) {
        ("createTable", Some(table)) => format!("<tableExists tableName=\"{table}\"/>"),
        ("addColumn", Some(table)) => match attr_of(body, "name") {
            Some(column) => format!("<columnExists tableName=\"{table}\" columnName=\"{column}\"/>"),
            None => return,
        },
        _ => return,
    };
    let mut head = String::from("<preConditions");
    if let Some(action) = wanted.on_fail {
        head.push_str(&format!(" onFail=\"{}\"", action.name()));
    }
    if let Some(action) = wanted.on_error {
        head.push_str(&format!(" onError=\"{}\"", action.name()));
    }
    set.preconditions = Some(format!("{PAD}{head}>\n{PAD}    <not>\n{PAD}        {check}\n{PAD}    </not>\n{PAD}</preConditions>\n"));
}

fn list_items(option: &'static str, text: &str) -> Result<Vec<String>, LiquibaseError> {
    let bad = |reason| Err(LiquibaseError::InvalidOption { option, reason });
    let items: Vec<String> = text.split(',').map(|item| item.trim().to_owned()).collect();
    if items.len() > 16 {
        return bad("at most 16 comma-separated items");
    }
    for item in &items {
        if item.is_empty() || item.len() > 64 {
            return bad("items must be 1 to 64 characters (no empty item between commas)");
        }
        if !item.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
            return bad("items may contain only ASCII letters, digits, '.', '_' and '-' (no quotes, spaces or expressions)");
        }
    }
    Ok(items)
}

fn list_text(text: &str) -> String {
    text.split(',').map(str::trim).collect::<Vec<_>>().join(",")
}

fn validate_attributes(attributes: &ChangeSetAttributes) -> Result<(), LiquibaseError> {
    if let Some(context) = &attributes.context {
        list_items("context", context)?;
    }
    if let Some(labels) = &attributes.labels {
        list_items("labels", labels)?;
    }
    Ok(())
}

fn validate_options(options: &LiquibaseOptions) -> Result<(), LiquibaseError> {
    let bad = |option, reason| Err(LiquibaseError::InvalidOption { option, reason });
    let author = &options.author;
    if author.trim().is_empty() || author.trim() != author {
        return bad("author", "must be non-empty without surrounding whitespace");
    }
    if author.chars().count() > 128 || author.chars().any(|c| c.is_control() || !xml_char(c)) || author.contains("${") {
        return bad("author", "must be a literal of at most 128 characters without control characters or ${");
    }
    let prefix = &options.id_prefix;
    if prefix.is_empty() || prefix.len() > 64 {
        return bad("id_prefix", "must be 1 to 64 characters");
    }
    if !prefix.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
        return bad("id_prefix", "may contain only ASCII letters, digits, '.', '_' and '-'");
    }
    validate_attributes(&options.defaults.all)?;
    for (kind, attributes) in &options.defaults.by_type {
        if !CHANGE_TYPES.contains(&kind.as_str()) {
            return bad("defaults", "unknown change type name");
        }
        validate_attributes(attributes)?;
    }
    Ok(())
}

pub fn emit_xml(plan: &ChangePlan, options: &LiquibaseOptions) -> Result<EmittedChangelog, LiquibaseError> {
    validate_options(options)?;
    let emit = &options.emit;
    check_backfills(plan, emit).map_err(|e| match e {
        crate::sql::SqlError::InvalidBackfill { table, column, reason } => LiquibaseError::InvalidBackfill { table, column, reason },
        other => unreachable!("check_backfills only reports backfill errors, got {other:?}"),
    })?;
    for decision in &emit.renames {
        let found = plan.changes.iter().any(|c| {
            matches!(&c.change, Change::RenameCandidate { table, from, to }
                if same(table, &decision.table) && same(from, &decision.from) && same(to, &decision.to))
        });
        if !found {
            return Err(LiquibaseError::UnknownRenameDecision {
                table: comment(&decision.table),
                from: comment(&decision.from),
                to: comment(&decision.to),
            });
        }
    }

    let mut changesets = Vec::new();
    let mut post = Vec::new();
    for planned in &plan.changes {
        let replaced = match &planned.change {
            Change::DropColumn { table, column } => emit.renames.iter().any(|r| same(&r.table, table) && same(&r.from, column)),
            Change::AddColumn { table, column } => emit.renames.iter().any(|r| same(&r.table, table) && same(&r.to, &column.name)),
            _ => false,
        };
        if replaced {
            continue;
        }
        let explicit_rename =
            matches!(&planned.change, Change::RenameCandidate { table, from, to } if decided(emit, table, from, to).is_some());
        let allowed = explicit_rename || matches!(planned.danger, Danger::Safe | Danger::Rewrite) || emit.allow.contains(&planned.danger);
        for item in render_change(planned, emit)? {
            let (body, skip) = match item.xml {
                Err(why) => (None, Some(Skip::Unsupported(why))),
                Ok(xml) if allowed || item.post => (Some(xml), None),
                Ok(xml) => (Some(xml), Some(Skip::Policy(item.danger))),
            };
            let ambiguous = matches!(skip, Some(Skip::Policy(_))) && item.danger == Danger::Ambiguous;
            let reason = if ambiguous { format!("{}; needs an explicit decision", item.reason) } else { item.reason };
            let notes = if skip.is_some() { Vec::new() } else { item.notes };
            let set = ChangeSet { danger: item.danger, reason, id: None, notes, body, skip, attributes: String::new(), preconditions: None, empty_rollback: false };
            if item.post {
                post.push(set);
            } else {
                changesets.push(set);
            }
        }
    }
    changesets.extend(post);

    let mut number = 0usize;
    for set in changesets.iter_mut().filter(|s| s.skip.is_none()) {
        number += 1;
        set.id = Some(format!("{}-{number}", options.id_prefix));
        apply_defaults(set, &options.defaults);
    }
    let plan_notes = plan.notes.iter().map(|n| format!("{} / {}: {}", n.table, n.subject, n.reason)).collect();
    Ok(EmittedChangelog { author: options.author.clone(), changesets, plan_notes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Note, SqlType};
    use crate::sql::{Backfill, BackfillValue, RenameDecision};

    fn col(name: &str, logical: LogicalType) -> Column {
        let mut c = Column::new(name, logical);
        c.sql_type = Fact::Known(SqlType { logical, raw: String::new() });
        c
    }

    fn planned(change: Change, danger: Danger, reason: &str) -> PlannedChange {
        PlannedChange { change, danger, reason: reason.into() }
    }

    fn options() -> LiquibaseOptions {
        LiquibaseOptions { emit: EmitOptions::default(), author: "jpa-assist".into(), id_prefix: "model".into(), defaults: ChangeSetDefaults::default() }
    }

    fn orders() -> Table {
        let mut id = col("id", LogicalType::Bigint);
        id.nullable = Fact::Known(false);
        id.generated = Fact::Known(true);
        let mut name = col("display_name", LogicalType::Varchar);
        name.length = Some(80);
        let mut table = Table::new("orders");
        table.columns = Fact::Known(vec![id, name, col("created", LogicalType::Timestamp)]);
        table.primary_key = Fact::Known(vec!["id".into()]);
        table.unique_constraints = Fact::Known(vec![UniqueConstraint { name: Some("uq_name".into()), columns: vec!["display_name".into()] }]);
        table.indexes = Fact::Known(vec![Index { name: Some("ix_created".into()), columns: vec!["created".into()], unique: false }]);
        table.foreign_keys = Fact::Known(vec![ForeignKey {
            name: Some("fk_cust".into()),
            columns: vec!["id".into()],
            ref_table: "customers".into(),
            ref_columns: Fact::Known(vec!["id".into()]),
        }]);
        table
    }

    fn full_plan() -> ChangePlan {
        let mut not_null = col("qty", LogicalType::Integer);
        not_null.nullable = Fact::Known(false);
        let fk = ForeignKey { name: Some("fk_a".into()), columns: vec!["a".into()], ref_table: "b".into(), ref_columns: Fact::Known(vec!["id".into()]) };
        let uq = UniqueConstraint { name: Some("uq_a".into()), columns: vec!["a".into()] };
        let ix = Index { name: Some("ix_a".into()), columns: vec!["a".into(), "b".into()], unique: true };
        let mut to = col("n", LogicalType::Bigint);
        to.nullable = Fact::Known(false);
        let changes = vec![
            planned(Change::CreateTable { table: orders() }, Danger::Safe, "new table"),
            planned(Change::AddColumn { table: "t".into(), column: col("note", LogicalType::Text) }, Danger::Safe, "nullable column"),
            planned(Change::AddColumn { table: "t".into(), column: not_null }, Danger::NeedsBackfill, "NOT NULL column without default"),
            planned(Change::AlterColumnType { table: "t".into(), column: "n".into(), from: col("n", LogicalType::Integer), to }, Danger::Rewrite, "type change"),
            planned(Change::AlterNullability { table: "t".into(), column: "m".into(), nullable: true, column_type: None }, Danger::Safe, "to nullable"),
            planned(Change::AlterNullability { table: "t".into(), column: "m".into(), nullable: false, column_type: None }, Danger::NeedsBackfill, "to NOT NULL"),
            planned(Change::DropForeignKey { table: "t".into(), foreign_key: fk.clone() }, Danger::Safe, "drop fk"),
            planned(Change::DropUnique { table: "t".into(), unique: uq.clone() }, Danger::Safe, "drop unique"),
            planned(Change::DropIndex { table: "t".into(), index: ix.clone() }, Danger::Safe, "drop index"),
            planned(Change::AddUnique { table: "t".into(), unique: uq }, Danger::Rewrite, "add unique"),
            planned(Change::AddIndex { table: "t".into(), index: ix }, Danger::Rewrite, "add index"),
            planned(Change::AddForeignKey { table: "t".into(), foreign_key: fk }, Danger::Rewrite, "add fk"),
            planned(Change::DropColumn { table: "t".into(), column: "old".into() }, Danger::DataLoss, "drop column"),
            planned(Change::DropTable { table: "gone".into() }, Danger::DataLoss, "drop table"),
        ];
        ChangePlan { changes, notes: vec![Note { table: "x".into(), subject: "s".into(), reason: "unknown\nfact".into() }] }
    }

    const GOLDEN: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<databaseChangeLog
    xmlns="http://www.liquibase.org/xml/ns/dbchangelog"
    xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
    xsi:schemaLocation="http://www.liquibase.org/xml/ns/dbchangelog http://www.liquibase.org/xml/ns/dbchangelog/dbchangelog-latest.xsd">

    <!-- Generated from a schema diff. Review before applying; nothing was executed. -->
    <!-- NOTE: x / s: unknown fact -->

    <!-- [Safe] new table -->
    <!-- REVIEW: generation (identity/auto-increment/sequence) of id is not emitted -->
    <changeSet id="model-1" author="jpa-assist">
        <createTable tableName="orders">
            <column name="id" type="bigint">
                <constraints primaryKey="true" nullable="false"/>
            </column>
            <column name="display_name" type="varchar(80)"/>
            <column name="created" type="datetime"/>
        </createTable>
    </changeSet>

    <!-- [Safe] nullable column -->
    <changeSet id="model-2" author="jpa-assist">
        <addColumn tableName="t">
            <column name="note" type="clob"/>
        </addColumn>
    </changeSet>

    <!-- SKIPPED (NeedsBackfill): NOT NULL column without default -->
    <!-- would be:
        <addColumn tableName="t">
            <column name="qty" type="int">
                <constraints nullable="false"/>
            </column>
        </addColumn>
    -->

    <!-- [Rewrite] type change -->
    <!-- REVIEW: modifyDataType changes only the data type; constraints and defaults are not restated -->
    <changeSet id="model-3" author="jpa-assist">
        <modifyDataType tableName="t" columnName="n" newDataType="bigint"/>
    </changeSet>

    <!-- [Safe] to nullable -->
    <!-- REVIEW: dropNotNullConstraint needs columnDataType on MySQL, MariaDB, MSSQL and Informix; the plan does not carry the column type, so add it before running there -->
    <changeSet id="model-4" author="jpa-assist">
        <dropNotNullConstraint tableName="t" columnName="m"/>
    </changeSet>

    <!-- SKIPPED (NeedsBackfill): to NOT NULL -->
    <!-- would be:
        <addNotNullConstraint tableName="t" columnName="m"/>
    -->

    <!-- [Safe] drop fk -->
    <changeSet id="model-5" author="jpa-assist">
        <dropForeignKeyConstraint baseTableName="t" constraintName="fk_a"/>
    </changeSet>

    <!-- [Safe] drop unique -->
    <changeSet id="model-6" author="jpa-assist">
        <dropUniqueConstraint tableName="t" constraintName="uq_a"/>
    </changeSet>

    <!-- [Safe] drop index -->
    <changeSet id="model-7" author="jpa-assist">
        <dropIndex tableName="t" indexName="ix_a"/>
    </changeSet>

    <!-- [Rewrite] add unique -->
    <changeSet id="model-8" author="jpa-assist">
        <addUniqueConstraint tableName="t" columnNames="a" constraintName="uq_a"/>
    </changeSet>

    <!-- [Rewrite] add index -->
    <changeSet id="model-9" author="jpa-assist">
        <createIndex tableName="t" indexName="ix_a" unique="true">
            <column name="a"/>
            <column name="b"/>
        </createIndex>
    </changeSet>

    <!-- [Rewrite] add fk -->
    <changeSet id="model-10" author="jpa-assist">
        <addForeignKeyConstraint baseTableName="t" baseColumnNames="a" constraintName="fk_a" referencedTableName="b" referencedColumnNames="id"/>
    </changeSet>

    <!-- SKIPPED (DataLoss): drop column -->
    <!-- would be:
        <dropColumn tableName="t" columnName="old"/>
    -->

    <!-- SKIPPED (DataLoss): drop table -->
    <!-- would be:
        <dropTable tableName="gone"/>
    -->

    <!-- [Safe] new table -->
    <changeSet id="model-11" author="jpa-assist">
        <addUniqueConstraint tableName="orders" columnNames="display_name" constraintName="uq_name"/>
    </changeSet>

    <!-- [Safe] new table -->
    <changeSet id="model-12" author="jpa-assist">
        <createIndex tableName="orders" indexName="ix_created">
            <column name="created"/>
        </createIndex>
    </changeSet>

    <!-- [Safe] new table -->
    <changeSet id="model-13" author="jpa-assist">
        <addForeignKeyConstraint baseTableName="orders" baseColumnNames="id" constraintName="fk_cust" referencedTableName="customers" referencedColumnNames="id"/>
    </changeSet>

</databaseChangeLog>
"#;

    #[test]
    fn golden_full_plan() {
        let emitted = emit_xml(&full_plan(), &options()).unwrap();
        assert_eq!(emitted.render(), GOLDEN);
        assert_eq!((emitted.emitted(), emitted.policy_skipped(), emitted.unsupported()), (13, 4, 0));
    }

    #[test]
    fn allowed_dangers_are_emitted() {
        let mut o = options();
        o.emit.allow = vec![Danger::DataLoss];
        let text = emit_xml(&full_plan(), &o).unwrap().render();
        assert!(text.contains("<dropColumn tableName=\"t\" columnName=\"old\"/>\n    </changeSet>"));
        assert!(text.contains("<dropTable tableName=\"gone\"/>\n    </changeSet>"));
        assert!(text.contains("SKIPPED (NeedsBackfill)"));
    }

    #[test]
    fn unsupported_facts_are_skipped_with_a_comment() {
        let mut price = col("price", LogicalType::Decimal);
        price.precision = Some(10);
        price.scale = Some(2);
        let mut t = Table::new("p");
        t.columns = Fact::Known(vec![price]);
        let plan = ChangePlan {
            changes: vec![
                planned(Change::CreateTable { table: t }, Danger::Safe, "new"),
                planned(Change::AddColumn { table: "t".into(), column: col("v", LogicalType::Varchar) }, Danger::Safe, "no length"),
                planned(Change::AddColumn { table: "t".into(), column: col("u", LogicalType::TimestampTz) }, Danger::Safe, "tz"),
                planned(
                    Change::DropIndex { table: "t".into(), index: Index { name: None, columns: vec!["a".into()], unique: false } },
                    Danger::Safe,
                    "unnamed",
                ),
            ],
            notes: vec![],
        };
        let emitted = emit_xml(&plan, &options()).unwrap();
        assert_eq!((emitted.emitted(), emitted.unsupported()), (0, 4));
        let text = emitted.render();
        assert!(text.contains("SKIPPED (unsupported): column price: decimal(p,s)"));
        assert!(!text.contains("<changeSet"));
    }

    #[test]
    fn renames_need_an_explicit_decision_and_replace_the_literal_pair() {
        let plan = ChangePlan {
            changes: vec![
                planned(Change::AddColumn { table: "t".into(), column: col("full_name", LogicalType::Text) }, Danger::Safe, "add"),
                planned(Change::RenameCandidate { table: "t".into(), from: "name".into(), to: "full_name".into() }, Danger::Ambiguous, "rename?"),
                planned(Change::DropColumn { table: "t".into(), column: "name".into() }, Danger::DataLoss, "drop"),
            ],
            notes: vec![],
        };
        let text = emit_xml(&plan, &options()).unwrap().render();
        assert!(text.contains("SKIPPED (Ambiguous): rename?; needs an explicit decision"));
        assert!(!text.contains("newColumnName=\"full_name\"/>\n    </changeSet>"));

        let mut o = options();
        o.emit.renames = vec![RenameDecision { table: "T".into(), from: "NAME".into(), to: "full_name".into() }];
        let emitted = emit_xml(&plan, &o).unwrap();
        let text = emitted.render();
        assert_eq!(emitted.emitted(), 1);
        assert!(text.contains("<renameColumn tableName=\"t\" oldColumnName=\"name\" newColumnName=\"full_name\"/>"));
        assert!(!text.contains("addColumn") && !text.contains("dropColumn"));

        o.emit.renames = vec![RenameDecision { table: "t".into(), from: "a".into(), to: "b".into() }];
        assert!(matches!(emit_xml(&plan, &o), Err(LiquibaseError::UnknownRenameDecision { .. })));
    }

    #[test]
    fn composite_key_uses_add_primary_key_after_the_table() {
        let mut t = Table::new("link");
        t.columns = Fact::Known(vec![col("a", LogicalType::Bigint), col("b", LogicalType::Bigint)]);
        t.primary_key = Fact::Known(vec!["a".into(), "b".into()]);
        let plan = ChangePlan { changes: vec![planned(Change::CreateTable { table: t }, Danger::Safe, "new")], notes: vec![] };
        let text = emit_xml(&plan, &options()).unwrap().render();
        assert!(text.contains("<addPrimaryKey tableName=\"link\" columnNames=\"a,b\"/>"));
        assert!(!text.contains("primaryKey=\"true\""));

        let mut bad = Table::new("x");
        bad.columns = Fact::Known(vec![col("a", LogicalType::Bigint)]);
        bad.primary_key = Fact::Known(vec!["zzz".into()]);
        let plan = ChangePlan { changes: vec![planned(Change::CreateTable { table: bad }, Danger::Safe, "new")], notes: vec![] };
        assert_eq!(emit_xml(&plan, &options()).unwrap().unsupported(), 1);
    }

    #[test]
    fn escaping_and_comment_hygiene() {
        let mut plan = ChangePlan {
            changes: vec![planned(Change::DropTable { table: "a\"b<c>&'d".into() }, Danger::Safe, "why -- not-")],
            notes: vec![],
        };
        let text = emit_xml(&plan, &options()).unwrap().render();
        assert!(text.contains("tableName=\"a&quot;b&lt;c&gt;&amp;&apos;d\""), "{text}");
        assert!(text.contains("<!-- [Safe] why - - not-  -->"), "{text}");

        for bad in ["", "a\nb", "a\u{0}b", "a\u{FFFE}b"] {
            plan.changes = vec![planned(Change::DropTable { table: bad.into() }, Danger::Safe, "x")];
            assert!(matches!(emit_xml(&plan, &options()), Err(LiquibaseError::InvalidIdentifier { .. })), "{bad:?}");
        }
        plan.changes = vec![planned(
            Change::AddUnique { table: "t".into(), unique: UniqueConstraint { name: None, columns: vec!["a,b".into()] } },
            Danger::Rewrite,
            "x",
        )];
        assert!(emit_xml(&plan, &options()).is_err());
    }

    #[test]
    fn skipped_body_cannot_break_out_of_its_comment() {
        let plan = ChangePlan { changes: vec![planned(Change::DropTable { table: "x--y".into() }, Danger::DataLoss, "drop")], notes: vec![] };
        let text = emit_xml(&plan, &options()).unwrap().render();
        assert!(text.contains("<dropTable tableName=\"x- -y\"/>"), "{text}");
        let inner = text.split("<!--").skip(1).map(|c| c.split("-->").next().unwrap()).collect::<Vec<_>>();
        assert!(inner.iter().all(|c| !c.contains("--")));
    }

    #[test]
    fn options_are_validated() {
        let plan = ChangePlan::default();
        for (author, prefix) in [("", "p"), (" a", "p"), ("a${x}", "p"), ("a\nb", "p"), ("a", ""), ("a", "has space"), ("a", "x\"y"), ("a", &"p".repeat(65))] {
            let o = LiquibaseOptions { author: author.into(), id_prefix: prefix.into(), ..options() };
            assert!(matches!(emit_xml(&plan, &o), Err(LiquibaseError::InvalidOption { .. })), "{author:?} {prefix:?}");
        }
        let o = LiquibaseOptions { author: "Ann & <Bob>".into(), ..options() };
        let p = ChangePlan { changes: vec![planned(Change::DropTable { table: "t".into() }, Danger::Safe, "x")], notes: vec![] };
        assert!(emit_xml(&p, &o).unwrap().render().contains("author=\"Ann &amp; &lt;Bob&gt;\""));
    }

    #[test]
    fn output_is_structurally_well_formed() {
        let mut o = options();
        o.emit.allow = vec![Danger::DataLoss, Danger::NeedsBackfill];
        let text = emit_xml(&full_plan(), &o).unwrap().render();
        assert_well_formed(&text);
    }

    fn backfill_plan() -> ChangePlan {
        let mut qty = col("qty", LogicalType::Integer);
        qty.nullable = Fact::Known(false);
        let mut mail = col("mail", LogicalType::Varchar);
        mail.length = Some(40);
        mail.nullable = Fact::Known(false);
        let mut active = col("active", LogicalType::Boolean);
        active.nullable = Fact::Known(false);
        ChangePlan {
            changes: vec![
                planned(Change::AddColumn { table: "t".into(), column: qty }, Danger::NeedsBackfill, "NOT NULL column without default"),
                planned(
                    Change::AlterNullability { table: "t".into(), column: "mail".into(), nullable: false, column_type: Some(mail) },
                    Danger::NeedsBackfill,
                    "to NOT NULL",
                ),
                planned(Change::AddColumn { table: "t".into(), column: active }, Danger::NeedsBackfill, "NOT NULL column without default"),
            ],
            notes: vec![],
        }
    }

    fn backfill_options() -> LiquibaseOptions {
        let mut o = options();
        o.emit.allow = vec![Danger::NeedsBackfill];
        o.emit.backfills = vec![
            Backfill { table: "t".into(), column: "qty".into(), value: BackfillValue::Integer(0) },
            Backfill { table: "t".into(), column: "mail".into(), value: BackfillValue::Text("a&b's \"x\" <n/a>".into()) },
            Backfill { table: "t".into(), column: "active".into(), value: BackfillValue::Boolean(true) },
        ];
        o
    }

    #[test]
    fn golden_backfill_changelog() {
        let text = emit_xml(&backfill_plan(), &backfill_options()).unwrap().render();
        assert_well_formed(&text);
        let body = text.split("<!-- Generated from a schema diff. Review before applying; nothing was executed. -->\n").nth(1).unwrap();
        assert_eq!(
            body,
            r#"
    <!-- [NeedsBackfill] NOT NULL column without default -->
    <changeSet id="model-1" author="jpa-assist">
        <addColumn tableName="t">
            <column name="qty" type="int"/>
        </addColumn>
    </changeSet>

    <!-- [NeedsBackfill] NOT NULL column without default -->
    <!-- REVIEW: the backfill value was chosen by the reviewer; check it is right for existing rows -->
    <!-- REVIEW: each step is its own changeSet, so an earlier step stays applied if a later one fails -->
    <changeSet id="model-2" author="jpa-assist">
        <update tableName="t">
            <column name="qty" valueNumeric="0"/>
            <where>qty IS NULL</where>
        </update>
    </changeSet>

    <!-- [NeedsBackfill] NOT NULL column without default -->
    <!-- REVIEW: columnDataType is taken from the model column; check the SQL Liquibase generates for qty on MySQL, MariaDB, MSSQL and Informix -->
    <changeSet id="model-3" author="jpa-assist">
        <addNotNullConstraint tableName="t" columnName="qty" columnDataType="int"/>
    </changeSet>

    <!-- [NeedsBackfill] to NOT NULL -->
    <!-- REVIEW: the backfill value was chosen by the reviewer; check it is right for existing rows -->
    <!-- REVIEW: each step is its own changeSet, so an earlier step stays applied if a later one fails -->
    <changeSet id="model-4" author="jpa-assist">
        <update tableName="t">
            <column name="mail" value="a&amp;b&apos;s &quot;x&quot; &lt;n/a&gt;"/>
            <where>mail IS NULL</where>
        </update>
    </changeSet>

    <!-- [NeedsBackfill] to NOT NULL -->
    <!-- REVIEW: columnDataType is taken from the model column; check the SQL Liquibase generates for mail on MySQL, MariaDB, MSSQL and Informix -->
    <changeSet id="model-5" author="jpa-assist">
        <addNotNullConstraint tableName="t" columnName="mail" columnDataType="varchar(40)"/>
    </changeSet>

    <!-- [NeedsBackfill] NOT NULL column without default -->
    <changeSet id="model-6" author="jpa-assist">
        <addColumn tableName="t">
            <column name="active" type="boolean"/>
        </addColumn>
    </changeSet>

    <!-- [NeedsBackfill] NOT NULL column without default -->
    <!-- REVIEW: the backfill value was chosen by the reviewer; check it is right for existing rows -->
    <!-- REVIEW: each step is its own changeSet, so an earlier step stays applied if a later one fails -->
    <changeSet id="model-7" author="jpa-assist">
        <update tableName="t">
            <column name="active" valueBoolean="true"/>
            <where>active IS NULL</where>
        </update>
    </changeSet>

    <!-- [NeedsBackfill] NOT NULL column without default -->
    <!-- REVIEW: columnDataType is taken from the model column; check the SQL Liquibase generates for active on MySQL, MariaDB, MSSQL and Informix -->
    <changeSet id="model-8" author="jpa-assist">
        <addNotNullConstraint tableName="t" columnName="active" columnDataType="boolean"/>
    </changeSet>

</databaseChangeLog>
"#
        );
    }

    #[test]
    fn backfill_errors_and_fallbacks() {
        let mut o = backfill_options();
        o.emit.backfills.push(Backfill { table: "t".into(), column: "ghost".into(), value: BackfillValue::Integer(1) });
        assert!(matches!(emit_xml(&backfill_plan(), &o), Err(LiquibaseError::InvalidBackfill { .. })));

        let mut o = backfill_options();
        o.emit.backfills[0].value = BackfillValue::Text("x".into());
        assert!(matches!(emit_xml(&backfill_plan(), &o), Err(LiquibaseError::InvalidBackfill { .. })));

        let mut o = backfill_options();
        o.emit.allow.clear();
        assert!(matches!(emit_xml(&backfill_plan(), &o), Err(LiquibaseError::InvalidBackfill { .. })));

        let mut plan = backfill_plan();
        if let Change::AddColumn { column, .. } = &mut plan.changes[0].change {
            column.name = "my col".into();
        }
        let mut o = backfill_options();
        o.emit.backfills[0].column = "my col".into();
        let text = emit_xml(&plan, &o).unwrap().render();
        assert!(text.contains("SKIPPED (unsupported): the update's where clause cannot quote"), "{text}");

        let mut o = options();
        o.emit.allow = vec![Danger::NeedsBackfill];
        let text = emit_xml(&backfill_plan(), &o).unwrap().render();
        assert!(!text.contains("<update"), "{text}");
    }

    fn small_plan() -> ChangePlan {
        ChangePlan {
            changes: vec![
                planned(Change::CreateTable { table: orders() }, Danger::Safe, "new table"),
                planned(Change::AddColumn { table: "t".into(), column: col("note", LogicalType::Text) }, Danger::Safe, "nullable column"),
                planned(Change::DropColumn { table: "t".into(), column: "old".into() }, Danger::DataLoss, "drop column"),
                planned(Change::DropIndex { table: "t".into(), index: Index { name: Some("ix_a".into()), columns: vec!["a".into()], unique: false } }, Danger::Safe, "drop index"),
                planned(Change::AlterNullability { table: "t".into(), column: "m".into(), nullable: true, column_type: None }, Danger::Safe, "to nullable"),
            ],
            notes: vec![],
        }
    }

    fn render_with(defaults: ChangeSetDefaults) -> String {
        let mut o = options();
        o.emit.allow = vec![Danger::DataLoss];
        o.defaults = defaults;
        let text = emit_xml(&small_plan(), &o).unwrap().render();
        assert_well_formed(&text);
        text
    }

    fn all(attributes: ChangeSetAttributes) -> ChangeSetDefaults {
        ChangeSetDefaults { all: attributes, by_type: vec![] }
    }

    #[test]
    fn unset_defaults_reproduce_the_plain_output_byte_for_byte() {
        let plain = render_with(ChangeSetDefaults::default());
        assert_eq!(plain, render_with(all(ChangeSetAttributes::default())));
        assert_eq!(plain, render_with(ChangeSetDefaults { all: ChangeSetAttributes::default(), by_type: vec![("dropTable".into(), ChangeSetAttributes::default())] }));
        assert!(plain.contains("<changeSet id=\"model-1\" author=\"jpa-assist\">\n        <createTable"), "{plain}");
        assert!(!plain.contains("rollback") && !plain.contains("preConditions") && !plain.contains("runOnChange"));
    }

    #[test]
    fn golden_attribute_knobs() {
        let text = render_with(all(ChangeSetAttributes {
            fail_on_error: Some(false),
            run_on_change: Some(true),
            context: Some("dev, test".into()),
            labels: Some("v1.0,shop_cart".into()),
            ..ChangeSetAttributes::default()
        }));
        let head = "<changeSet id=\"model-2\" author=\"jpa-assist\" runOnChange=\"true\" failOnError=\"false\" context=\"dev,test\" labels=\"v1.0,shop_cart\">\n        <addColumn tableName=\"t\">";
        assert!(text.contains(head), "{text}");
        assert_eq!(text.matches("failOnError=\"false\"").count(), 8);
        let only_true = render_with(all(ChangeSetAttributes { fail_on_error: Some(true), ..ChangeSetAttributes::default() }));
        assert!(only_true.contains("author=\"jpa-assist\" failOnError=\"true\">"), "{only_true}");
    }

    #[test]
    fn golden_preconditions_for_create_table_and_add_column_only() {
        let text = render_with(all(ChangeSetAttributes { on_fail: Some(PreconditionAction::MarkRan), on_error: Some(PreconditionAction::Halt), ..ChangeSetAttributes::default() }));
        let table = "    <changeSet id=\"model-1\" author=\"jpa-assist\">\n        <preConditions onFail=\"MARK_RAN\" onError=\"HALT\">\n            <not>\n                <tableExists tableName=\"orders\"/>\n            </not>\n        </preConditions>\n        <createTable tableName=\"orders\">";
        assert!(text.contains(table), "{text}");
        let column = "        <preConditions onFail=\"MARK_RAN\" onError=\"HALT\">\n            <not>\n                <columnExists tableName=\"t\" columnName=\"note\"/>\n            </not>\n        </preConditions>\n        <addColumn tableName=\"t\">";
        assert!(text.contains(column), "{text}");
        assert_eq!(text.matches("<preConditions").count(), 2, "{text}");
        let warn = render_with(all(ChangeSetAttributes { on_fail: Some(PreconditionAction::Warn), ..ChangeSetAttributes::default() }));
        assert!(warn.contains("<preConditions onFail=\"WARN\">"), "{warn}");
        let cont = render_with(all(ChangeSetAttributes { on_error: Some(PreconditionAction::Continue), ..ChangeSetAttributes::default() }));
        assert!(cont.contains("<preConditions onError=\"CONTINUE\">"), "{cont}");
    }

    #[test]
    fn golden_empty_rollback_only_where_there_is_no_automatic_one() {
        let text = render_with(all(ChangeSetAttributes { empty_rollback: Some(true), ..ChangeSetAttributes::default() }));
        assert!(text.contains("<dropColumn tableName=\"t\" columnName=\"old\"/>\n        <rollback/>\n    </changeSet>"), "{text}");
        assert!(text.contains("<dropIndex tableName=\"t\" indexName=\"ix_a\"/>\n        <rollback/>\n    </changeSet>"), "{text}");
        assert_eq!(text.matches("<rollback/>").count(), 2, "{text}");
        let off = render_with(all(ChangeSetAttributes { empty_rollback: Some(false), ..ChangeSetAttributes::default() }));
        assert!(!off.contains("rollback"));
    }

    #[test]
    fn per_type_rows_override_the_global_row_field_by_field() {
        let text = render_with(ChangeSetDefaults {
            all: ChangeSetAttributes { run_on_change: Some(true), context: Some("dev".into()), ..ChangeSetAttributes::default() },
            by_type: vec![
                ("createTable".into(), ChangeSetAttributes { context: Some("prod".into()), fail_on_error: Some(false), ..ChangeSetAttributes::default() }),
                ("dropColumn".into(), ChangeSetAttributes { run_on_change: Some(false), ..ChangeSetAttributes::default() }),
            ],
        });
        assert!(text.contains("<changeSet id=\"model-1\" author=\"jpa-assist\" runOnChange=\"true\" failOnError=\"false\" context=\"prod\">"), "{text}");
        assert!(text.contains("<changeSet id=\"model-2\" author=\"jpa-assist\" runOnChange=\"true\" context=\"dev\">"), "{text}");
        assert!(text.contains("author=\"jpa-assist\" runOnChange=\"false\" context=\"dev\">\n        <dropColumn"), "{text}");
    }

    #[test]
    fn changeset_defaults_are_validated() {
        for bad in ["", "a,,b", "a b", "a\"b", "a\nb", "dev and test", "!dev", "a'b", "a&b", "x,"] {
            for field in 0..2 {
                let mut attributes = ChangeSetAttributes::default();
                if field == 0 { attributes.context = Some(bad.into()) } else { attributes.labels = Some(bad.into()) }
                let mut o = options();
                o.defaults = all(attributes);
                assert!(matches!(emit_xml(&small_plan(), &o), Err(LiquibaseError::InvalidOption { .. })), "{bad:?}");
            }
        }
        let long = vec!["a"; 17].join(",");
        let mut o = options();
        o.defaults = all(ChangeSetAttributes { labels: Some(long), ..ChangeSetAttributes::default() });
        assert!(emit_xml(&small_plan(), &o).is_err());
        o.defaults = ChangeSetDefaults { all: ChangeSetAttributes::default(), by_type: vec![("dropEverything".into(), ChangeSetAttributes::default())] };
        assert!(emit_xml(&small_plan(), &o).is_err());
        o.defaults = ChangeSetDefaults { all: ChangeSetAttributes::default(), by_type: vec![("createTable".into(), ChangeSetAttributes { context: Some("a;b".into()), ..ChangeSetAttributes::default() })] };
        assert!(emit_xml(&small_plan(), &o).is_err());
        assert_eq!(PreconditionAction::parse("MARK_RAN"), Some(PreconditionAction::MarkRan));
        assert_eq!(PreconditionAction::parse("off"), None);
    }

    #[test]
    fn defaults_apply_to_backfill_steps_by_their_own_type() {
        let mut o = backfill_options();
        o.defaults = ChangeSetDefaults {
            all: ChangeSetAttributes::default(),
            by_type: vec![("update".into(), ChangeSetAttributes { fail_on_error: Some(true), ..ChangeSetAttributes::default() })],
        };
        let text = emit_xml(&backfill_plan(), &o).unwrap().render();
        assert_well_formed(&text);
        assert_eq!(text.matches("failOnError=\"true\"").count(), 3, "{text}");
        assert!(text.contains("failOnError=\"true\">\n        <update tableName=\"t\">"), "{text}");
    }

    fn assert_well_formed(text: &str) {
        let mut stack: Vec<String> = Vec::new();
        let mut rest = text;
        let mut roots = 0;
        while let Some(at) = rest.find('<') {
            rest = &rest[at..];
            if let Some(body) = rest.strip_prefix("<!--") {
                let end = body.find("-->").expect("comment closed");
                assert!(!body[..end].contains("--"), "-- in comment");
                assert!(!body[..end].ends_with('-'));
                rest = &body[end + 3..];
            } else if let Some(body) = rest.strip_prefix("<?") {
                rest = &body[body.find("?>").expect("pi closed") + 2..];
            } else {
                let end = rest.find('>').expect("tag closed");
                let inner = &rest[1..end];
                assert!(!inner.contains('<'));
                rest = &rest[end + 1..];
                if let Some(name) = inner.strip_prefix('/') {
                    assert_eq!(stack.pop().as_deref(), Some(name), "mismatched close");
                } else {
                    let name = inner.split_whitespace().next().unwrap().trim_end_matches('/').to_owned();
                    let quotes = inner.matches('"').count();
                    assert_eq!(quotes % 2, 0, "unbalanced attribute quotes in {inner}");
                    if stack.is_empty() {
                        roots += 1;
                    }
                    if !inner.ends_with('/') {
                        stack.push(name);
                    }
                }
            }
        }
        assert!(stack.is_empty());
        assert_eq!(roots, 1);
    }
}
