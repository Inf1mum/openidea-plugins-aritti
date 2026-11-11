//! Reviewed SQL text for a `ChangePlan`. Nothing here executes SQL or connects to a database.
//!
//! Policy: `Safe` and `Rewrite` changes are emitted; `NeedsBackfill`, `DataLoss` and `Ambiguous` ones are
//! listed as commented-out `-- SKIPPED` lines unless `EmitOptions::allow` names their danger. A rename is only
//! emitted when `EmitOptions::renames` carries a decision for it. A fact the emitter cannot render
//! (unknown, or not verified for the dialect) skips that statement with a visible comment.
//!
//! Syntax verified against the PostgreSQL 18 and MySQL 8.4 manuals (ALTER TABLE, CREATE TABLE, CREATE/DROP
//! INDEX, identifier rules, numeric/date/character/binary/boolean/uuid types). Not verified: MySQL VARCHAR
//! needing a length, CHAR/VARCHAR/identifier length limits for MySQL (64 assumed from general knowledge).
use crate::schema::{
    Change, ChangePlan, Column, Danger, Fact, ForeignKey, Index, LogicalType, PlannedChange, Table, UniqueConstraint,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    PostgreSql,
    MySql,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenameDecision {
    pub table: String,
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, Default)]
pub struct EmitOptions {
    /// Dangers (other than Safe and Rewrite) the caller explicitly accepts.
    pub allow: Vec<Danger>,
    pub renames: Vec<RenameDecision>,
    /// Values for `NeedsBackfill` changes that `allow` accepts; see `check_backfills`.
    pub backfills: Vec<Backfill>,
}

/// A typed literal (never NULL) that existing rows receive before a column becomes NOT NULL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackfillValue {
    Integer(i64),
    /// Digits with an optional sign and fraction, e.g. `-12.50`; checked by `BackfillValue::check`.
    Decimal(String),
    Boolean(bool),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backfill {
    pub table: String,
    pub column: String,
    pub value: BackfillValue,
}

fn digit_run(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

impl BackfillValue {
    pub fn supports(logical: LogicalType) -> bool {
        use LogicalType::*;
        matches!(logical, Tinyint | Smallint | Integer | Bigint | Real | Double | Decimal | Boolean | Char | Varchar | Text)
    }

    /// Parses reviewer input for a column of this logical type. Date, time, timestamp, uuid and binary columns
    /// are refused: their literal syntax differs per dialect and was not verified.
    pub fn parse(input: &str, logical: LogicalType) -> Result<Self, String> {
        use LogicalType::*;
        let value = match logical {
            Tinyint | Smallint | Integer | Bigint => {
                Self::Integer(input.trim().parse().map_err(|_| "enter a whole number".to_owned())?)
            }
            Real | Double | Decimal => Self::Decimal(input.trim().to_owned()),
            Boolean => match input.trim().to_ascii_lowercase().as_str() {
                "true" => Self::Boolean(true),
                "false" => Self::Boolean(false),
                _ => return Err("enter true or false".into()),
            },
            Char | Varchar | Text => Self::Text(input.to_owned()),
            _ => return Err("a backfill value is not supported for this column type".into()),
        };
        Ok(value)
    }

    /// Refuses a value that does not fit the column's logical type, length, precision or scale.
    pub fn check(&self, column: &Column) -> Result<(), String> {
        use LogicalType::*;
        let Some(logical) = column.sql_type.known().map(|t| t.logical) else {
            return Err("the column type is unknown".into());
        };
        match (self, logical) {
            (Self::Integer(n), Tinyint) if i8::try_from(*n).is_err() => Err("outside the tinyint range".into()),
            (Self::Integer(n), Smallint) if i16::try_from(*n).is_err() => Err("outside the smallint range".into()),
            (Self::Integer(n), Integer) if i32::try_from(*n).is_err() => Err("outside the integer range".into()),
            (Self::Integer(_), Tinyint | Smallint | Integer | Bigint | Real | Double | Decimal) => Ok(()),
            (Self::Decimal(text), Real | Double | Decimal) => {
                let unsigned = text.strip_prefix('-').unwrap_or(text);
                let (whole, fraction) = unsigned.split_once('.').map_or((unsigned, ""), |(w, f)| (w, f));
                if !digit_run(whole) || (unsigned.contains('.') && !digit_run(fraction)) || whole.len() + fraction.len() > 65 {
                    return Err("not a plain decimal number (digits, optional minus, optional fraction)".into());
                }
                if logical != Decimal {
                    return Ok(());
                }
                let significant = whole.trim_start_matches('0').len();
                let scale = column.scale.unwrap_or(0) as usize;
                match column.precision {
                    Some(p) if fraction.len() > scale || significant > (p as usize).saturating_sub(scale) => {
                        Err(format!("does not fit decimal({p},{scale})"))
                    }
                    _ => Ok(()),
                }
            }
            (Self::Boolean(_), Boolean) => Ok(()),
            (Self::Text(text), Char | Varchar | Text) => {
                if text.chars().any(|c| c.is_control() || matches!(c, '\\' | '\u{FFFE}' | '\u{FFFF}')) {
                    return Err("control characters and backslashes are not accepted (their meaning depends on server settings)".into());
                }
                match column.length {
                    Some(n) if logical != Text && text.chars().count() > n as usize => Err(format!("longer than {n} characters")),
                    _ => Ok(()),
                }
            }
            _ => Err(format!("a {} value does not fit a {logical:?} column", self.kind())),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Integer(_) => "whole-number",
            Self::Decimal(_) => "decimal",
            Self::Boolean(_) => "boolean",
            Self::Text(_) => "text",
        }
    }

    fn literal(&self) -> String {
        match self {
            Self::Integer(n) => n.to_string(),
            Self::Decimal(text) => text.clone(),
            Self::Boolean(true) => "TRUE".into(),
            Self::Boolean(false) => "FALSE".into(),
            Self::Text(text) => format!("'{}'", text.replace('\'', "''")),
        }
    }
}

/// A change a backfill can target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackfillTarget {
    pub table: String,
    pub column: String,
    pub column_type: Option<Column>,
}

fn target_of(change: &Change) -> Option<BackfillTarget> {
    match change {
        Change::AddColumn { table, column } => {
            Some(BackfillTarget { table: table.clone(), column: column.name.clone(), column_type: Some(column.clone()) })
        }
        Change::AlterNullability { table, column, nullable: false, column_type } => {
            Some(BackfillTarget { table: table.clone(), column: column.clone(), column_type: column_type.clone() })
        }
        _ => None,
    }
}

fn replaced_by_rename(change: &Change, renames: &[RenameDecision]) -> bool {
    match change {
        Change::DropColumn { table, column } => renames.iter().any(|r| same(&r.table, table) && same(&r.from, column)),
        Change::AddColumn { table, column } => renames.iter().any(|r| same(&r.table, table) && same(&r.to, &column.name)),
        _ => false,
    }
}

/// The `NeedsBackfill` changes a backfill value can be supplied for (not those a rename decision replaces).
pub fn backfill_targets(plan: &ChangePlan, renames: &[RenameDecision]) -> Vec<BackfillTarget> {
    plan.changes
        .iter()
        .filter(|c| c.danger == Danger::NeedsBackfill && !replaced_by_rename(&c.change, renames))
        .filter_map(|c| target_of(&c.change))
        .collect()
}

/// Every backfill must name a distinct `NeedsBackfill` change that `allow` accepts and fit its column type.
pub fn check_backfills(plan: &ChangePlan, options: &EmitOptions) -> Result<(), SqlError> {
    let targets = backfill_targets(plan, &options.renames);
    for (at, backfill) in options.backfills.iter().enumerate() {
        let bad = |reason: String| Err(SqlError::InvalidBackfill { table: comment(&backfill.table), column: comment(&backfill.column), reason });
        if options.backfills[..at].iter().any(|b| same(&b.table, &backfill.table) && same(&b.column, &backfill.column)) {
            return bad("listed twice".into());
        }
        let Some(target) = targets.iter().find(|t| same(&t.table, &backfill.table) && same(&t.column, &backfill.column)) else {
            return bad("no NeedsBackfill change targets this column".into());
        };
        if !options.allow.contains(&Danger::NeedsBackfill) {
            return bad("NeedsBackfill changes are not included".into());
        }
        let Some(column) = &target.column_type else {
            return bad("the plan does not carry the column type".into());
        };
        if let Err(reason) = backfill.value.check(column) {
            return bad(reason);
        }
    }
    Ok(())
}

pub(crate) fn backfill_for<'a>(options: &'a EmitOptions, table: &str, column: &str) -> Option<&'a BackfillValue> {
    options.backfills.iter().find(|b| same(&b.table, table) && same(&b.column, column)).map(|b| &b.value)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SqlError {
    InvalidIdentifier { name: String, reason: &'static str },
    InvalidBackfill { table: String, column: String, reason: String },
    UnknownRenameDecision { table: String, from: String, to: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Skip {
    /// Withheld by the danger policy.
    Policy(Danger),
    /// A fact is unknown or not verified for this dialect.
    Unsupported(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statement {
    pub danger: Danger,
    pub reason: String,
    /// Present for emitted statements and for policy-skipped ones (rendered commented out).
    pub sql: Option<String>,
    pub notes: Vec<String>,
    pub skip: Option<Skip>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmittedSql {
    pub dialect: Dialect,
    pub statements: Vec<Statement>,
    pub plan_notes: Vec<String>,
}

impl EmittedSql {
    pub fn emitted(&self) -> usize {
        self.statements.iter().filter(|s| s.skip.is_none()).count()
    }

    pub fn unsupported(&self) -> usize {
        self.statements.iter().filter(|s| matches!(s.skip, Some(Skip::Unsupported(_)))).count()
    }

    pub fn policy_skipped(&self) -> usize {
        self.statements.iter().filter(|s| matches!(s.skip, Some(Skip::Policy(_)))).count()
    }

    /// Text of the migration file; `\n` line endings.
    pub fn render(&self) -> String {
        let name = if self.dialect == Dialect::PostgreSql { "PostgreSQL" } else { "MySQL" };
        let mut out = format!("-- Generated from a schema diff ({name}). Review before applying; nothing was executed.\n");
        for note in &self.plan_notes {
            out.push_str(&format!("-- NOTE: {}\n", comment(note)));
        }
        for statement in &self.statements {
            out.push('\n');
            let reason = comment(&statement.reason);
            match &statement.skip {
                None => out.push_str(&format!("-- [{:?}] {reason}\n", statement.danger)),
                Some(Skip::Policy(danger)) => out.push_str(&format!("-- SKIPPED ({danger:?}): {reason}\n")),
                Some(Skip::Unsupported(why)) => {
                    out.push_str(&format!("-- SKIPPED (unsupported): {}; plan reason: {reason}\n", comment(why)))
                }
            }
            for note in &statement.notes {
                out.push_str(&format!("-- REVIEW: {}\n", comment(note)));
            }
            if let Some(sql) = &statement.sql {
                let commented = statement.skip.is_some();
                for line in sql.lines() {
                    out.push_str(if commented { "-- " } else { "" });
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        out
    }
}

fn comment(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

fn quote(dialect: Dialect, name: &str) -> Result<String, SqlError> {
    let bad = |reason| Err(SqlError::InvalidIdentifier { name: comment(name), reason });
    if name.is_empty() {
        return bad("empty");
    }
    if name.contains(['"', '`']) {
        return bad("contains a quote character");
    }
    if name.chars().any(char::is_control) {
        return bad("contains a control character");
    }
    match dialect {
        Dialect::PostgreSql if name.len() > 63 => bad("longer than 63 bytes"),
        Dialect::MySql if name.chars().count() > 64 => bad("longer than 64 characters"),
        Dialect::MySql if name.ends_with(' ') => bad("ends with a space"),
        Dialect::MySql if name.chars().any(|c| c as u32 > 0xFFFF) => bad("has a character outside the BMP"),
        Dialect::PostgreSql => Ok(format!("\"{name}\"")),
        Dialect::MySql => Ok(format!("`{name}`")),
    }
}

fn quote_list(dialect: Dialect, names: &[String]) -> Result<String, SqlError> {
    let quoted: Result<Vec<_>, _> = names.iter().map(|n| quote(dialect, n)).collect();
    Ok(quoted?.join(", "))
}

type Unsupported = String;

fn type_name(dialect: Dialect, column: &Column) -> Result<String, Unsupported> {
    use LogicalType::*;
    let Fact::Known(sql_type) = &column.sql_type else {
        return Err(format!("type of column {} is unknown", column.name));
    };
    let pg = dialect == Dialect::PostgreSql;
    let no = |what: &str| Err(format!("column {}: {what}", column.name));
    let numeric = |kind: &str, bare_ok: bool| -> Result<String, Unsupported> {
        match (column.precision, column.scale) {
            (Some(p), _) if p == 0 || p > if pg { 1000 } else { 65 } => no("decimal precision out of range"),
            (Some(p), Some(s)) if s > p => no("scale exceeds precision"),
            (Some(p), Some(s)) => Ok(format!("{kind}({p},{s})")),
            (Some(p), None) => Ok(format!("{kind}({p})")),
            (None, None) if bare_ok => Ok(kind.to_owned()),
            (None, None) => no("decimal without precision has a dialect-specific default; not guessed"),
            (None, Some(_)) => no("scale without precision"),
        }
    };
    match (sql_type.logical, pg) {
        (Tinyint, true) => no("PostgreSQL has no 1-byte integer type"),
        (Tinyint, false) => Ok("tinyint".into()),
        (Smallint, true) => Ok("smallint".into()),
        (Smallint, false) => Ok("smallint".into()),
        (Integer, true) => Ok("integer".into()),
        (Integer, false) => Ok("int".into()),
        (Bigint, _) => Ok("bigint".into()),
        (Boolean, _) => Ok("boolean".into()),
        (Real, true) => Ok("real".into()),
        (Real, false) => Ok("float".into()),
        (Double, true) => Ok("double precision".into()),
        (Double, false) => Ok("double".into()),
        (Decimal, true) => numeric("numeric", true),
        (Decimal, false) => numeric("decimal", false),
        (Char, _) => match column.length {
            Some(n) if n > 0 && (!pg || n <= 10_485_760) => Ok(format!("{}({n})", if pg { "character" } else { "char" })),
            _ => no("fixed-length char needs a valid length"),
        },
        (Varchar, true) => match column.length {
            Some(n) if n > 0 && n <= 10_485_760 => Ok(format!("varchar({n})")),
            Some(_) => no("varchar length out of range"),
            None => Ok("varchar".into()),
        },
        (Varchar, false) => match column.length {
            Some(n) if n > 0 => Ok(format!("varchar({n})")),
            _ => no("MySQL varchar needs a length (not verified in the manual excerpt read)"),
        },
        (Text, _) => Ok("text".into()),
        (Date, _) => Ok("date".into()),
        (Time, _) => Ok("time".into()),
        (Timestamp, true) => Ok("timestamp".into()),
        (Timestamp, false) => Ok("datetime".into()),
        (TimestampTz, true) => Ok("timestamp with time zone".into()),
        (TimestampTz, false) => no("MySQL has no time-zone-carrying timestamp type"),
        (Uuid, true) => Ok("uuid".into()),
        (Uuid, false) => no("MySQL has no native uuid type; storage choice not guessed"),
        (Binary, true) if column.length.is_none() => Ok("bytea".into()),
        (Binary, true) => no("bytea takes no length"),
        (Binary, false) => no("MySQL binary type choice (BINARY/VARBINARY/BLOB) not guessed"),
    }
}

struct Column_ {
    text: String,
    notes: Vec<String>,
}

/// `name type [NOT NULL]`; defaults and generation are never emitted, only reported.
fn column_definition(dialect: Dialect, column: &Column) -> Result<Result<Column_, Unsupported>, SqlError> {
    let name = quote(dialect, &column.name)?;
    let ty = match type_name(dialect, column) {
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
    let text = format!("{name} {ty}{}", if nullable { "" } else { " NOT NULL" });
    Ok(Ok(Column_ { text, notes }))
}

struct Item {
    sql: Result<String, Unsupported>,
    notes: Vec<String>,
    danger: Danger,
    reason: String,
    post: bool,
}

fn foreign_key_clause(dialect: Dialect, fk: &ForeignKey) -> Result<Result<String, Unsupported>, SqlError> {
    let Fact::Known(ref_columns) = &fk.ref_columns else {
        return Ok(Err("referenced columns are unknown".into()));
    };
    let name = match &fk.name {
        Some(name) => format!("CONSTRAINT {} ", quote(dialect, name)?),
        None => String::new(),
    };
    Ok(Ok(format!(
        "{name}FOREIGN KEY ({}) REFERENCES {} ({})",
        quote_list(dialect, &fk.columns)?,
        quote(dialect, &fk.ref_table)?,
        quote_list(dialect, ref_columns)?
    )))
}

fn index_statement(dialect: Dialect, table: &str, index: &Index) -> Result<String, SqlError> {
    let t = quote(dialect, table)?;
    let cols = quote_list(dialect, &index.columns)?;
    let name = match &index.name {
        Some(name) => Some(quote(dialect, name)?),
        None => None,
    };
    Ok(match dialect {
        Dialect::PostgreSql => format!(
            "CREATE {}INDEX {}ON {t} ({cols});",
            if index.unique { "UNIQUE " } else { "" },
            name.map(|n| format!("{n} ")).unwrap_or_default()
        ),
        Dialect::MySql => format!(
            "ALTER TABLE {t} ADD {}INDEX {}({cols});",
            if index.unique { "UNIQUE " } else { "" },
            name.map(|n| format!("{n} ")).unwrap_or_default()
        ),
    })
}

fn create_table(dialect: Dialect, table: &Table, danger: Danger, reason: &str) -> Result<Vec<Item>, SqlError> {
    let item = |sql, notes, post| Item { sql, notes, danger, reason: reason.to_owned(), post };
    let Fact::Known(columns) = &table.columns else {
        return Ok(vec![item(Err("columns are unknown".into()), vec![], false)]);
    };
    let mut lines = Vec::new();
    let mut notes = Vec::new();
    for column in columns {
        match column_definition(dialect, column)? {
            Ok(def) => {
                lines.push(def.text);
                notes.extend(def.notes);
            }
            Err(why) => return Ok(vec![item(Err(why), vec![], false)]),
        }
    }
    if lines.is_empty() {
        return Ok(vec![item(Err("table has no columns".into()), vec![], false)]);
    }
    match &table.primary_key {
        Fact::Known(key) if key.is_empty() => {}
        Fact::Known(key) => lines.push(format!("PRIMARY KEY ({})", quote_list(dialect, key)?)),
        Fact::Unknown(_) => return Ok(vec![item(Err("primary key is unknown".into()), vec![], false)]),
    }
    match &table.unique_constraints {
        Fact::Known(uniques) => {
            for unique in uniques {
                let name = match &unique.name {
                    Some(name) => format!("CONSTRAINT {} ", quote(dialect, name)?),
                    None => String::new(),
                };
                lines.push(format!("{name}UNIQUE ({})", quote_list(dialect, &unique.columns)?));
            }
        }
        Fact::Unknown(_) => notes.push("unique constraints are unknown and not emitted".into()),
    }
    let mut out = Vec::new();
    match &table.indexes {
        Fact::Known(indexes) => {
            for index in indexes {
                out.push(item(Ok(index_statement(dialect, &table.name, index)?), vec![], true));
            }
        }
        Fact::Unknown(_) => notes.push("indexes are unknown and not emitted".into()),
    }
    match &table.foreign_keys {
        Fact::Known(fks) => {
            for fk in fks {
                let sql = match foreign_key_clause(dialect, fk)? {
                    Ok(clause) => Ok(format!("ALTER TABLE {} ADD {clause};", quote(dialect, &table.name)?)),
                    Err(why) => Err(why),
                };
                out.push(item(sql, vec![], true));
            }
        }
        Fact::Unknown(_) => notes.push("foreign keys are unknown and not emitted".into()),
    }
    let body = lines.iter().map(|l| format!("    {l}")).collect::<Vec<_>>().join(",\n");
    let main = format!("CREATE TABLE {} (\n{body}\n);", quote(dialect, &table.name)?);
    out.insert(0, item(Ok(main), notes, false));
    Ok(out)
}

/// `UPDATE ... WHERE col IS NULL`, then NOT NULL (PostgreSQL SET NOT NULL; MySQL MODIFY restating the definition).
fn backfill_steps(
    dialect: Dialect,
    table: &str,
    column: &str,
    value: &BackfillValue,
    definition: Option<&Column>,
    created_here: bool,
    step: &dyn Fn(Result<String, Unsupported>, Vec<String>) -> Item,
) -> Result<Vec<Item>, SqlError> {
    let (t, c) = (quote(dialect, table)?, quote(dialect, column)?);
    let update = format!("UPDATE {t} SET {c} = {} WHERE {c} IS NULL;", value.literal());
    let mut update_notes = vec!["the backfill value was chosen by the reviewer; check it is right for existing rows".to_owned()];
    let not_null = if dialect == Dialect::PostgreSql {
        Ok(format!("ALTER TABLE {t} ALTER COLUMN {c} SET NOT NULL;"))
    } else {
        update_notes.push("MySQL ALTER TABLE ends the transaction implicitly, so these steps are not atomic".into());
        match definition {
            None => Err("MODIFY needs the column type and the plan does not carry it".to_owned()),
            Some(def) if !created_here && (def.default != Fact::Known(None) || def.generated != Fact::Known(false)) => {
                Err("MODIFY restates the whole column definition and would drop a default or auto-increment attribute that is present or not known".to_owned())
            }
            Some(def) => {
                let mut required = def.clone();
                required.nullable = Fact::Known(false);
                match column_definition(dialect, &required)? {
                    Ok(rendered) => Ok(format!("ALTER TABLE {t} MODIFY COLUMN {};", rendered.text)),
                    Err(why) => Err(why),
                }
            }
        }
    };
    if let Err(why) = not_null {
        return Ok(vec![step(Err(why), vec![])]);
    }
    Ok(vec![step(Ok(update), update_notes), step(not_null, vec![])])
}

fn render_change(dialect: Dialect, planned: &PlannedChange, options: &EmitOptions) -> Result<Vec<Item>, SqlError> {
    let (danger, reason) = (planned.danger, planned.reason.clone());
    let one = |sql: Result<String, Unsupported>, notes: Vec<String>| {
        Ok(vec![Item { sql, notes, danger, reason: reason.clone(), post: false }])
    };
    let q = |name: &str| quote(dialect, name);
    let pg = dialect == Dialect::PostgreSql;
    match &planned.change {
        Change::CreateTable { table } => create_table(dialect, table, danger, &reason),
        Change::DropTable { table } => one(Ok(format!("DROP TABLE {};", q(table)?)), vec![]),
        Change::AddColumn { table, column } => {
            let def = match column_definition(dialect, column)? {
                Ok(def) => def,
                Err(why) => return one(Err(why), vec![]),
            };
            let not_null = column.nullable.known() == Some(&false);
            if not_null && column.default != Fact::Known(None) && backfill_for(options, table, &column.name).is_none() {
                return one(Err("NOT NULL column with a default: the default is not emitted, so the statement would fail on populated tables".into()), vec![]);
            }
            if let Some(value) = backfill_for(options, table, &column.name).filter(|_| not_null) {
                let mut nullable = column.clone();
                nullable.nullable = Fact::Known(true);
                let first = column_definition(dialect, &nullable)?.expect("rendered above");
                let step = |sql, notes| Item { sql, notes, danger, reason: reason.clone(), post: false };
                let steps = backfill_steps(dialect, table, &column.name, value, Some(column), true, &step)?;
                if steps.len() == 1 {
                    return Ok(steps);
                }
                let mut out = vec![step(Ok(format!("ALTER TABLE {} ADD COLUMN {};", q(table)?, first.text)), first.notes)];
                out.extend(steps);
                return Ok(out);
            }
            let mut notes = def.notes;
            if not_null {
                notes.push("NOT NULL column without default needs a backfill".into());
            }
            one(Ok(format!("ALTER TABLE {} ADD COLUMN {};", q(table)?, def.text)), notes)
        }
        Change::DropColumn { table, column } => one(Ok(format!("ALTER TABLE {} DROP COLUMN {};", q(table)?, q(column)?)), vec![]),
        Change::AlterColumnType { table, column, to, .. } => {
            let def = match column_definition(dialect, to)? {
                Ok(def) => def,
                Err(why) => return one(Err(why), vec![]),
            };
            let t = q(table)?;
            if pg {
                let ty = type_name(dialect, to).expect("rendered above");
                one(
                    Ok(format!("ALTER TABLE {t} ALTER COLUMN {} TYPE {ty};", q(column)?)),
                    vec!["PostgreSQL may need a USING expression when no implicit cast exists".into()],
                )
            } else {
                if to.default != Fact::Known(None) || to.generated != Fact::Known(false) {
                    return one(Err("MODIFY restates the whole column definition and would drop the default or auto-increment attribute".into()), vec![]);
                }
                one(
                    Ok(format!("ALTER TABLE {t} MODIFY COLUMN {};", def.text)),
                    vec!["MODIFY restates the column definition; other attributes (comment, charset) are not carried forward".into()],
                )
            }
        }
        Change::AlterNullability { table, column, nullable, column_type } => {
            if let Some(value) = backfill_for(options, table, column).filter(|_| !nullable) {
                let step = |sql, notes| Item { sql, notes, danger, reason: reason.clone(), post: false };
                let steps = backfill_steps(dialect, table, column, value, column_type.as_ref(), false, &step)?;
                return Ok(steps);
            }
            if !pg {
                return one(Err("MySQL MODIFY must restate the whole column definition, which this change does not carry".into()), vec![]);
            }
            let verb = if *nullable { "DROP" } else { "SET" };
            let mut notes = Vec::new();
            if !nullable {
                notes.push("NOT NULL column without default needs a backfill".into());
            }
            one(Ok(format!("ALTER TABLE {} ALTER COLUMN {} {verb} NOT NULL;", q(table)?, q(column)?)), notes)
        }
        Change::AddForeignKey { table, foreign_key } => {
            let clause = foreign_key_clause(dialect, foreign_key)?;
            let t = q(table)?;
            one(clause.map(|c| format!("ALTER TABLE {t} ADD {c};")), vec![])
        }
        Change::DropForeignKey { table, foreign_key } => {
            let Some(name) = &foreign_key.name else {
                return one(Err("the foreign key has no name to drop by".into()), vec![]);
            };
            let verb = if pg { "DROP CONSTRAINT" } else { "DROP FOREIGN KEY" };
            one(Ok(format!("ALTER TABLE {} {verb} {};", q(table)?, q(name)?)), vec![])
        }
        Change::AddUnique { table, unique } => {
            let name = match &unique.name {
                Some(name) => format!("CONSTRAINT {} ", q(name)?),
                None => String::new(),
            };
            one(Ok(format!("ALTER TABLE {} ADD {name}UNIQUE ({});", q(table)?, quote_list(dialect, &unique.columns)?)), vec![])
        }
        Change::DropUnique { table, unique: UniqueConstraint { name, .. } } => {
            let Some(name) = name else {
                return one(Err("the unique constraint has no name to drop by".into()), vec![]);
            };
            let verb = if pg { "DROP CONSTRAINT" } else { "DROP INDEX" };
            one(Ok(format!("ALTER TABLE {} {verb} {};", q(table)?, q(name)?)), vec![])
        }
        Change::AddIndex { table, index } => one(Ok(index_statement(dialect, table, index)?), vec![]),
        Change::DropIndex { table, index } => {
            let Some(name) = &index.name else {
                return one(Err("the index has no name to drop by".into()), vec![]);
            };
            let sql = if pg {
                format!("DROP INDEX {};", q(name)?)
            } else {
                format!("ALTER TABLE {} DROP INDEX {};", q(table)?, q(name)?)
            };
            one(Ok(sql), vec![])
        }
        Change::RenameCandidate { table, from, to } => {
            one(Ok(format!("ALTER TABLE {} RENAME COLUMN {} TO {};", q(table)?, q(from)?, q(to)?)), vec![])
        }
    }
}

pub(crate) fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

pub(crate) fn decided<'a>(options: &'a EmitOptions, table: &str, from: &str, to: &str) -> Option<&'a RenameDecision> {
    options.renames.iter().find(|r| same(&r.table, table) && same(&r.from, from) && same(&r.to, to))
}

pub fn emit(plan: &ChangePlan, dialect: Dialect, options: &EmitOptions) -> Result<EmittedSql, SqlError> {
    check_backfills(plan, options)?;
    for decision in &options.renames {
        let found = plan.changes.iter().any(|c| {
            matches!(&c.change, Change::RenameCandidate { table, from, to }
                if same(table, &decision.table) && same(from, &decision.from) && same(to, &decision.to))
        });
        if !found {
            return Err(SqlError::UnknownRenameDecision {
                table: decision.table.clone(),
                from: decision.from.clone(),
                to: decision.to.clone(),
            });
        }
    }

    let mut statements = Vec::new();
    let mut post = Vec::new();
    for planned in &plan.changes {
        // A decided rename replaces the literal drop and add of the same pair.
        let replaced = match &planned.change {
            Change::DropColumn { table, column } => {
                options.renames.iter().any(|r| same(&r.table, table) && same(&r.from, column))
            }
            Change::AddColumn { table, column } => {
                options.renames.iter().any(|r| same(&r.table, table) && same(&r.to, &column.name))
            }
            _ => false,
        };
        if replaced {
            continue;
        }

        let explicit_rename = matches!(&planned.change, Change::RenameCandidate { table, from, to } if decided(options, table, from, to).is_some());
        let allowed = explicit_rename
            || matches!(planned.danger, Danger::Safe | Danger::Rewrite)
            || options.allow.contains(&planned.danger);
        for item in render_change(dialect, planned, options)? {
            let (sql, skip) = match item.sql {
                Err(why) => (None, Some(Skip::Unsupported(why))),
                Ok(sql) if allowed || item.post => (Some(sql), None),
                Ok(sql) => (Some(sql), Some(Skip::Policy(item.danger))),
            };
            let skip_policy_reason = matches!(skip, Some(Skip::Policy(_))) && item.danger == Danger::Ambiguous;
            let reason = if skip_policy_reason { format!("{}; needs an explicit decision", item.reason) } else { item.reason };
            let notes = if skip.is_some() { Vec::new() } else { item.notes };
            let statement = Statement { danger: item.danger, reason, sql, notes, skip };
            if item.post {
                post.push(statement);
            } else {
                statements.push(statement);
            }
        }
    }
    statements.extend(post);

    let plan_notes = plan.notes.iter().map(|n| format!("{} / {}: {}", n.table, n.subject, n.reason)).collect();
    Ok(EmittedSql { dialect, statements, plan_notes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Note, SqlType};

    fn col(name: &str, logical: LogicalType) -> Column {
        let mut c = Column::new(name, logical);
        c.sql_type = Fact::Known(SqlType { logical, raw: String::new() });
        c
    }

    fn planned(change: Change, danger: Danger, reason: &str) -> PlannedChange {
        PlannedChange { change, danger, reason: reason.into() }
    }

    fn orders() -> Table {
        let mut id = col("id", LogicalType::Bigint);
        id.nullable = Fact::Known(false);
        id.generated = Fact::Known(true);
        let mut name = col("display_name", LogicalType::Varchar);
        name.length = Some(80);
        let mut price = col("price", LogicalType::Decimal);
        price.precision = Some(10);
        price.scale = Some(2);
        let mut table = Table::new("orders");
        table.columns = Fact::Known(vec![id, name, price, col("created", LogicalType::TimestampTz)]);
        table.primary_key = Fact::Known(vec!["id".into()]);
        table.unique_constraints = Fact::Known(vec![UniqueConstraint { name: Some("uq_name".into()), columns: vec!["display_name".into()] }]);
        table.indexes = Fact::Known(vec![Index { name: Some("ix_price".into()), columns: vec!["price".into()], unique: false }]);
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
        let ix = Index { name: Some("ix_a".into()), columns: vec!["a".into()], unique: true };
        let mut to = col("n", LogicalType::Bigint);
        to.nullable = Fact::Known(false);
        let changes = vec![
            planned(Change::CreateTable { table: orders() }, Danger::Safe, "new table"),
            planned(Change::AddColumn { table: "t".into(), column: col("note", LogicalType::Text) }, Danger::Safe, "nullable column"),
            planned(Change::AddColumn { table: "t".into(), column: not_null }, Danger::NeedsBackfill, "NOT NULL column without default"),
            planned(Change::AlterColumnType { table: "t".into(), column: "n".into(), from: col("n", LogicalType::Integer), to }, Danger::Rewrite, "type change"),
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

    const GOLDEN_PG: &str = r#"-- Generated from a schema diff (PostgreSQL). Review before applying; nothing was executed.
-- NOTE: x / s: unknown fact

-- [Safe] new table
-- REVIEW: generation (identity/auto-increment/sequence) of id is not emitted
CREATE TABLE "orders" (
    "id" bigint NOT NULL,
    "display_name" varchar(80),
    "price" numeric(10,2),
    "created" timestamp with time zone,
    PRIMARY KEY ("id"),
    CONSTRAINT "uq_name" UNIQUE ("display_name")
);

-- [Safe] nullable column
ALTER TABLE "t" ADD COLUMN "note" text;

-- SKIPPED (NeedsBackfill): NOT NULL column without default
-- ALTER TABLE "t" ADD COLUMN "qty" integer NOT NULL;

-- [Rewrite] type change
-- REVIEW: PostgreSQL may need a USING expression when no implicit cast exists
ALTER TABLE "t" ALTER COLUMN "n" TYPE bigint;

-- SKIPPED (NeedsBackfill): to NOT NULL
-- ALTER TABLE "t" ALTER COLUMN "m" SET NOT NULL;

-- [Safe] drop fk
ALTER TABLE "t" DROP CONSTRAINT "fk_a";

-- [Safe] drop unique
ALTER TABLE "t" DROP CONSTRAINT "uq_a";

-- [Safe] drop index
DROP INDEX "ix_a";

-- [Rewrite] add unique
ALTER TABLE "t" ADD CONSTRAINT "uq_a" UNIQUE ("a");

-- [Rewrite] add index
CREATE UNIQUE INDEX "ix_a" ON "t" ("a");

-- [Rewrite] add fk
ALTER TABLE "t" ADD CONSTRAINT "fk_a" FOREIGN KEY ("a") REFERENCES "b" ("id");

-- SKIPPED (DataLoss): drop column
-- ALTER TABLE "t" DROP COLUMN "old";

-- SKIPPED (DataLoss): drop table
-- DROP TABLE "gone";

-- [Safe] new table
CREATE INDEX "ix_price" ON "orders" ("price");

-- [Safe] new table
ALTER TABLE "orders" ADD CONSTRAINT "fk_cust" FOREIGN KEY ("id") REFERENCES "customers" ("id");
"#;

    const GOLDEN_MYSQL: &str = r#"-- Generated from a schema diff (MySQL). Review before applying; nothing was executed.
-- NOTE: x / s: unknown fact

-- SKIPPED (unsupported): column created: MySQL has no time-zone-carrying timestamp type; plan reason: new table

-- [Safe] nullable column
ALTER TABLE `t` ADD COLUMN `note` text;

-- SKIPPED (NeedsBackfill): NOT NULL column without default
-- ALTER TABLE `t` ADD COLUMN `qty` int NOT NULL;

-- [Rewrite] type change
-- REVIEW: MODIFY restates the column definition; other attributes (comment, charset) are not carried forward
ALTER TABLE `t` MODIFY COLUMN `n` bigint NOT NULL;

-- SKIPPED (unsupported): MySQL MODIFY must restate the whole column definition, which this change does not carry; plan reason: to NOT NULL

-- [Safe] drop fk
ALTER TABLE `t` DROP FOREIGN KEY `fk_a`;

-- [Safe] drop unique
ALTER TABLE `t` DROP INDEX `uq_a`;

-- [Safe] drop index
ALTER TABLE `t` DROP INDEX `ix_a`;

-- [Rewrite] add unique
ALTER TABLE `t` ADD CONSTRAINT `uq_a` UNIQUE (`a`);

-- [Rewrite] add index
ALTER TABLE `t` ADD UNIQUE INDEX `ix_a` (`a`);

-- [Rewrite] add fk
ALTER TABLE `t` ADD CONSTRAINT `fk_a` FOREIGN KEY (`a`) REFERENCES `b` (`id`);

-- SKIPPED (DataLoss): drop column
-- ALTER TABLE `t` DROP COLUMN `old`;

-- SKIPPED (DataLoss): drop table
-- DROP TABLE `gone`;
"#;

    #[test]
    fn golden_postgresql() {
        let out = emit(&full_plan(), Dialect::PostgreSql, &EmitOptions::default()).unwrap();
        assert_eq!(out.render(), GOLDEN_PG);
        assert_eq!((out.emitted(), out.policy_skipped(), out.unsupported()), (11, 4, 0));
    }

    #[test]
    fn golden_mysql() {
        let out = emit(&full_plan(), Dialect::MySql, &EmitOptions::default()).unwrap();
        assert_eq!(out.render(), GOLDEN_MYSQL);
        assert_eq!(out.unsupported(), 2);
    }

    #[test]
    fn allowing_a_danger_emits_it_with_review_comments() {
        let options = EmitOptions { allow: vec![Danger::NeedsBackfill, Danger::DataLoss], ..EmitOptions::default() };
        let text = emit(&full_plan(), Dialect::PostgreSql, &options).unwrap().render();
        assert!(text.contains("-- REVIEW: NOT NULL column without default needs a backfill\nALTER TABLE \"t\" ADD COLUMN \"qty\" integer NOT NULL;"));
        assert!(text.contains("\nDROP TABLE \"gone\";"));
        assert!(!text.contains("SKIPPED"));
    }

    #[test]
    fn identifiers_with_quotes_or_control_characters_are_rejected() {
        for bad in ["a\"b", "a`b", "a\nb", "", "a\0b"] {
            for dialect in [Dialect::PostgreSql, Dialect::MySql] {
                let plan = ChangePlan {
                    changes: vec![planned(Change::DropTable { table: bad.into() }, Danger::DataLoss, "x")],
                    notes: vec![],
                };
                assert!(matches!(emit(&plan, dialect, &EmitOptions::default()), Err(SqlError::InvalidIdentifier { .. })), "{bad:?}");
            }
        }
        assert!(quote(Dialect::PostgreSql, &"a".repeat(64)).is_err());
        assert!(quote(Dialect::PostgreSql, &"a".repeat(63)).is_ok());
        assert!(quote(Dialect::MySql, "trail ").is_err());
        assert_eq!(quote(Dialect::PostgreSql, "order").unwrap(), "\"order\"");
    }

    #[test]
    fn comments_cannot_smuggle_statements() {
        let plan = ChangePlan {
            changes: vec![planned(Change::DropIndex { table: "t".into(), index: Index { name: None, columns: vec![], unique: false } }, Danger::Safe, "x\nDROP TABLE t;")],
            notes: vec![],
        };
        let text = emit(&plan, Dialect::PostgreSql, &EmitOptions::default()).unwrap().render();
        assert!(!text.lines().any(|l| l.starts_with("DROP")));
        assert!(text.contains("SKIPPED (unsupported)"));
    }

    #[test]
    fn rename_needs_an_explicit_decision_and_replaces_drop_and_add() {
        let changes = vec![
            planned(Change::AddColumn { table: "t".into(), column: col("b", LogicalType::Text) }, Danger::Safe, "add"),
            planned(Change::RenameCandidate { table: "t".into(), from: "a".into(), to: "b".into() }, Danger::Ambiguous, "maybe rename"),
            planned(Change::DropColumn { table: "t".into(), column: "a".into() }, Danger::DataLoss, "drop"),
        ];
        let plan = ChangePlan { changes, notes: vec![] };
        let none = emit(&plan, Dialect::PostgreSql, &EmitOptions::default()).unwrap().render();
        assert!(none.contains("-- ALTER TABLE \"t\" RENAME COLUMN \"a\" TO \"b\";"));
        assert!(none.contains("needs an explicit decision"));
        assert!(none.contains("\nALTER TABLE \"t\" ADD COLUMN \"b\" text;"));

        let options = EmitOptions { renames: vec![RenameDecision { table: "T".into(), from: "a".into(), to: "B".into() }], ..EmitOptions::default() };
        for (dialect, q) in [(Dialect::PostgreSql, '"'), (Dialect::MySql, '`')] {
            let text = emit(&plan, dialect, &options).unwrap().render();
            assert!(text.contains(&format!("\nALTER TABLE {q}t{q} RENAME COLUMN {q}a{q} TO {q}b{q};")));
            assert!(!text.contains("ADD COLUMN") && !text.contains("DROP COLUMN"), "{text}");
        }

        let stray = EmitOptions { renames: vec![RenameDecision { table: "t".into(), from: "x".into(), to: "y".into() }], ..EmitOptions::default() };
        assert!(matches!(emit(&plan, Dialect::MySql, &stray), Err(SqlError::UnknownRenameDecision { .. })));
    }

    #[test]
    fn unknown_facts_skip_the_statement() {
        let mut unknown_type = col("c", LogicalType::Text);
        unknown_type.sql_type = Fact::unknown("converter");
        let mut unknown_null = col("d", LogicalType::Text);
        unknown_null.nullable = Fact::unknown("n");
        let mut defaulted = col("e", LogicalType::Integer);
        defaulted.nullable = Fact::Known(false);
        defaulted.default = Fact::Known(Some("0".into()));
        let plan = ChangePlan {
            changes: [unknown_type, unknown_null, defaulted]
                .into_iter()
                .map(|column| planned(Change::AddColumn { table: "t".into(), column }, Danger::Safe, "add"))
                .collect(),
            notes: vec![],
        };
        let out = emit(&plan, Dialect::PostgreSql, &EmitOptions::default()).unwrap();
        assert_eq!((out.emitted(), out.unsupported()), (0, 3));
        assert!(out.render().lines().all(|l| l.starts_with("--") || l.is_empty()));

        let mut table = orders();
        table.primary_key = Fact::unknown("composite");
        let plan = ChangePlan { changes: vec![planned(Change::CreateTable { table }, Danger::Safe, "new")], notes: vec![] };
        assert_eq!(emit(&plan, Dialect::PostgreSql, &EmitOptions::default()).unwrap().unsupported(), 1);
    }

    #[test]
    fn type_names_per_dialect() {
        use LogicalType::*;
        let name = |d, l, len: Option<u32>, p: Option<u32>, s: Option<u32>| {
            let mut c = col("c", l);
            c.length = len;
            c.precision = p;
            c.scale = s;
            type_name(d, &c).ok()
        };
        let (pg, my) = (Dialect::PostgreSql, Dialect::MySql);
        assert_eq!(name(pg, Tinyint, None, None, None), None);
        assert_eq!(name(my, Tinyint, None, None, None).as_deref(), Some("tinyint"));
        assert_eq!(name(pg, Double, None, None, None).as_deref(), Some("double precision"));
        assert_eq!(name(my, Decimal, None, None, None), None);
        assert_eq!(name(pg, Decimal, None, None, None).as_deref(), Some("numeric"));
        assert_eq!(name(my, Decimal, None, Some(12), None).as_deref(), Some("decimal(12)"));
        assert_eq!(name(my, Varchar, None, None, None), None);
        assert_eq!(name(pg, Varchar, None, None, None).as_deref(), Some("varchar"));
        assert_eq!(name(pg, Char, Some(3), None, None).as_deref(), Some("character(3)"));
        assert_eq!(name(my, Timestamp, None, None, None).as_deref(), Some("datetime"));
        assert_eq!(name(pg, Uuid, None, None, None).as_deref(), Some("uuid"));
        assert_eq!(name(my, Uuid, None, None, None), None);
        assert_eq!(name(pg, Binary, None, None, None).as_deref(), Some("bytea"));
    }

    fn backfill_plan() -> ChangePlan {
        let mut qty = col("qty", LogicalType::Integer);
        qty.nullable = Fact::Known(false);
        let mut name = col("label", LogicalType::Varchar);
        name.length = Some(8);
        name.nullable = Fact::Known(false);
        let mut tightened = col("mail", LogicalType::Varchar);
        tightened.length = Some(40);
        tightened.nullable = Fact::Known(false);
        ChangePlan {
            changes: vec![
                planned(Change::AddColumn { table: "t".into(), column: qty }, Danger::NeedsBackfill, "NOT NULL column without default"),
                planned(Change::AddColumn { table: "t".into(), column: name }, Danger::NeedsBackfill, "NOT NULL column without default"),
                planned(
                    Change::AlterNullability { table: "t".into(), column: "mail".into(), nullable: false, column_type: Some(tightened) },
                    Danger::NeedsBackfill,
                    "to NOT NULL",
                ),
            ],
            notes: vec![],
        }
    }

    fn backfills() -> Vec<Backfill> {
        let text = |column: &str, value: &str| Backfill { table: "t".into(), column: column.into(), value: BackfillValue::Text(value.into()) };
        vec![
            Backfill { table: "t".into(), column: "qty".into(), value: BackfillValue::Integer(0) },
            text("label", "it's"),
            text("mail", "n/a"),
        ]
    }

    fn backfill_options() -> EmitOptions {
        EmitOptions { allow: vec![Danger::NeedsBackfill], backfills: backfills(), ..EmitOptions::default() }
    }

    #[test]
    fn golden_backfill_postgresql() {
        let text = emit(&backfill_plan(), Dialect::PostgreSql, &backfill_options()).unwrap().render();
        assert_eq!(
            text,
            r#"-- Generated from a schema diff (PostgreSQL). Review before applying; nothing was executed.

-- [NeedsBackfill] NOT NULL column without default
ALTER TABLE "t" ADD COLUMN "qty" integer;

-- [NeedsBackfill] NOT NULL column without default
-- REVIEW: the backfill value was chosen by the reviewer; check it is right for existing rows
UPDATE "t" SET "qty" = 0 WHERE "qty" IS NULL;

-- [NeedsBackfill] NOT NULL column without default
ALTER TABLE "t" ALTER COLUMN "qty" SET NOT NULL;

-- [NeedsBackfill] NOT NULL column without default
ALTER TABLE "t" ADD COLUMN "label" varchar(8);

-- [NeedsBackfill] NOT NULL column without default
-- REVIEW: the backfill value was chosen by the reviewer; check it is right for existing rows
UPDATE "t" SET "label" = 'it''s' WHERE "label" IS NULL;

-- [NeedsBackfill] NOT NULL column without default
ALTER TABLE "t" ALTER COLUMN "label" SET NOT NULL;

-- [NeedsBackfill] to NOT NULL
-- REVIEW: the backfill value was chosen by the reviewer; check it is right for existing rows
UPDATE "t" SET "mail" = 'n/a' WHERE "mail" IS NULL;

-- [NeedsBackfill] to NOT NULL
ALTER TABLE "t" ALTER COLUMN "mail" SET NOT NULL;
"#
        );
    }

    #[test]
    fn golden_backfill_mysql() {
        let text = emit(&backfill_plan(), Dialect::MySql, &backfill_options()).unwrap().render();
        assert_eq!(
            text,
            r#"-- Generated from a schema diff (MySQL). Review before applying; nothing was executed.

-- [NeedsBackfill] NOT NULL column without default
ALTER TABLE `t` ADD COLUMN `qty` int;

-- [NeedsBackfill] NOT NULL column without default
-- REVIEW: the backfill value was chosen by the reviewer; check it is right for existing rows
-- REVIEW: MySQL ALTER TABLE ends the transaction implicitly, so these steps are not atomic
UPDATE `t` SET `qty` = 0 WHERE `qty` IS NULL;

-- [NeedsBackfill] NOT NULL column without default
ALTER TABLE `t` MODIFY COLUMN `qty` int NOT NULL;

-- [NeedsBackfill] NOT NULL column without default
ALTER TABLE `t` ADD COLUMN `label` varchar(8);

-- [NeedsBackfill] NOT NULL column without default
-- REVIEW: the backfill value was chosen by the reviewer; check it is right for existing rows
-- REVIEW: MySQL ALTER TABLE ends the transaction implicitly, so these steps are not atomic
UPDATE `t` SET `label` = 'it''s' WHERE `label` IS NULL;

-- [NeedsBackfill] NOT NULL column without default
ALTER TABLE `t` MODIFY COLUMN `label` varchar(8) NOT NULL;

-- [NeedsBackfill] to NOT NULL
-- REVIEW: the backfill value was chosen by the reviewer; check it is right for existing rows
-- REVIEW: MySQL ALTER TABLE ends the transaction implicitly, so these steps are not atomic
UPDATE `t` SET `mail` = 'n/a' WHERE `mail` IS NULL;

-- [NeedsBackfill] to NOT NULL
ALTER TABLE `t` MODIFY COLUMN `mail` varchar(40) NOT NULL;
"#
        );
    }

    #[test]
    fn without_a_backfill_behavior_is_unchanged() {
        let options = EmitOptions { allow: vec![Danger::NeedsBackfill], ..EmitOptions::default() };
        let text = emit(&backfill_plan(), Dialect::PostgreSql, &options).unwrap().render();
        assert!(text.contains("ALTER TABLE \"t\" ADD COLUMN \"qty\" integer NOT NULL;"), "{text}");
        assert!(!text.contains("UPDATE"), "{text}");
        let text = emit(&backfill_plan(), Dialect::MySql, &options).unwrap().render();
        assert!(text.contains("SKIPPED (unsupported): MySQL MODIFY must restate"), "{text}");
    }

    #[test]
    fn mysql_modify_that_would_drop_attributes_is_skipped_whole() {
        let mut plan = backfill_plan();
        if let Change::AlterNullability { column_type: Some(c), .. } = &mut plan.changes[2].change {
            c.default = Fact::Known(Some("'x'".into()));
        }
        let text = emit(&plan, Dialect::MySql, &backfill_options()).unwrap().render();
        assert!(text.contains("SKIPPED (unsupported): MODIFY restates the whole column definition"), "{text}");
        assert!(!text.contains("UPDATE `t` SET `mail`"), "{text}");
    }

    #[test]
    fn backfills_are_validated() {
        let plan = backfill_plan();
        let try_with = |backfill: Backfill, allow: Vec<Danger>| {
            emit(&plan, Dialect::PostgreSql, &EmitOptions { allow, backfills: vec![backfill], ..EmitOptions::default() })
        };
        let all = vec![Danger::NeedsBackfill];
        let text = |column: &str, value: &str| Backfill { table: "t".into(), column: column.into(), value: BackfillValue::Text(value.into()) };
        let invalid = |result: Result<EmittedSql, SqlError>| matches!(result, Err(SqlError::InvalidBackfill { .. }));

        assert!(invalid(try_with(Backfill { table: "t".into(), column: "qty".into(), value: BackfillValue::Text("0".into()) }, all.clone())));
        assert!(invalid(try_with(Backfill { table: "t".into(), column: "qty".into(), value: BackfillValue::Integer(1) }, vec![])));
        assert!(invalid(try_with(Backfill { table: "t".into(), column: "nope".into(), value: BackfillValue::Integer(1) }, all.clone())));
        assert!(invalid(try_with(Backfill { table: "other".into(), column: "qty".into(), value: BackfillValue::Integer(1) }, all.clone())));
        assert!(invalid(try_with(text("label", "too long for 8"), all.clone())));
        for bad in ["a\nb", "a\tb", "a\\b", "a\u{0}b", "a\u{7f}b"] {
            assert!(invalid(try_with(text("label", bad), all.clone())), "{bad:?}");
        }
        assert!(try_with(text("label", "o'neil"), all.clone()).unwrap().render().contains("'o''neil'"));
        let twice = EmitOptions {
            allow: all.clone(),
            backfills: vec![text("label", "a"), text("LABEL", "b")],
            ..EmitOptions::default()
        };
        assert!(invalid(emit(&plan, Dialect::PostgreSql, &twice)));
    }

    #[test]
    fn backfill_for_a_safe_change_or_a_renamed_column_is_an_error() {
        let safe = ChangePlan {
            changes: vec![planned(Change::AddColumn { table: "t".into(), column: col("note", LogicalType::Text) }, Danger::Safe, "nullable")],
            notes: vec![],
        };
        let options = EmitOptions {
            allow: vec![Danger::NeedsBackfill],
            backfills: vec![Backfill { table: "t".into(), column: "note".into(), value: BackfillValue::Text("x".into()) }],
            ..EmitOptions::default()
        };
        assert!(matches!(emit(&safe, Dialect::PostgreSql, &options), Err(SqlError::InvalidBackfill { .. })));

        let mut renamed = backfill_plan();
        renamed.changes.push(planned(Change::DropColumn { table: "t".into(), column: "old".into() }, Danger::DataLoss, "drop"));
        renamed.changes.push(planned(Change::RenameCandidate { table: "t".into(), from: "old".into(), to: "qty".into() }, Danger::Ambiguous, "rename?"));
        let mut options = backfill_options();
        options.renames = vec![RenameDecision { table: "t".into(), from: "old".into(), to: "qty".into() }];
        assert!(matches!(emit(&renamed, Dialect::PostgreSql, &options), Err(SqlError::InvalidBackfill { column, .. }) if column == "qty"));
        assert_eq!(backfill_targets(&renamed, &options.renames).len(), 2);
    }

    #[test]
    fn typed_values_parse_and_fit_the_column() {
        let mut price = col("price", LogicalType::Decimal);
        price.precision = Some(5);
        price.scale = Some(2);
        let decimal = |text: &str| BackfillValue::Decimal(text.into());
        assert!(decimal("123.45").check(&price).is_ok());
        assert!(decimal("-0.5").check(&price).is_ok());
        for bad in ["1234.5", "1.234", "1e3", "", ".5", "5.", "+1", "1,5", "0x10"] {
            assert!(decimal(bad).check(&price).is_err(), "{bad}");
        }
        assert_eq!(BackfillValue::parse(" 12 ", LogicalType::Smallint), Ok(BackfillValue::Integer(12)));
        assert!(BackfillValue::Integer(40_000).check(&col("n", LogicalType::Smallint)).is_err());
        assert!(BackfillValue::parse("1.5", LogicalType::Integer).is_err());
        assert_eq!(BackfillValue::parse("TRUE", LogicalType::Boolean), Ok(BackfillValue::Boolean(true)));
        assert!(BackfillValue::parse("yes", LogicalType::Boolean).is_err());
        assert!(BackfillValue::parse("2024-01-01", LogicalType::Date).is_err());
        assert!(BackfillValue::Boolean(true).check(&col("b", LogicalType::Integer)).is_err());
        let rendered = BackfillValue::Boolean(false).literal();
        assert_eq!(rendered, "FALSE");
    }

    #[test]
    fn unread_defaults_do_not_block_a_backfilled_add_but_stay_noted() {
        let mut plan = backfill_plan();
        if let Change::AddColumn { column, .. } = &mut plan.changes[0].change {
            column.default = Fact::unknown("column defaults are not read");
        }
        let text = emit(&plan, Dialect::MySql, &backfill_options()).unwrap().render();
        assert!(text.contains("-- REVIEW: default of qty is unknown and not emitted\nALTER TABLE `t` ADD COLUMN `qty` int;"), "{text}");
        assert!(text.contains("MODIFY COLUMN `qty` int NOT NULL;"), "{text}");
        let without = EmitOptions { allow: vec![Danger::NeedsBackfill], ..EmitOptions::default() };
        let text = emit(&plan, Dialect::PostgreSql, &without).unwrap().render();
        assert!(text.contains("SKIPPED (unsupported): NOT NULL column with a default"), "{text}");
    }
}
