//! Dialect-neutral schema snapshots and a deterministic snapshot diff.
//!
//! Every fact is `Known(v)` or `Unknown(reason)`; unknown never reads as absent, and
//! the diff reports unknown facets as notes instead of drops. Renames are never
//! inferred: a dropped and an added column of one logical type yield a
//! `RenameCandidate` next to the literal drop and add, and nothing applies it.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_TABLES: usize = 500;
pub const MAX_COLUMNS: usize = 500;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "value", rename_all = "kebab-case")]
pub enum Fact<T> {
    Known(T),
    /// The reason this fact could not be established from the provided sources.
    Unknown(String),
}

impl<T> Fact<T> {
    pub fn known(&self) -> Option<&T> {
        match self {
            Self::Known(v) => Some(v),
            Self::Unknown(_) => None,
        }
    }

    pub fn unknown(reason: &str) -> Self {
        Self::Unknown(reason.to_owned())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LogicalType {
    Tinyint,
    Smallint,
    Integer,
    Bigint,
    Boolean,
    Real,
    Double,
    Decimal,
    Char,
    Varchar,
    Text,
    Date,
    Time,
    Timestamp,
    TimestampTz,
    Uuid,
    Binary,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SqlType {
    pub logical: LogicalType,
    /// The spelling it came from (a Java type, or SQL text from a database baseline).
    pub raw: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provenance {
    Literal,
    Derived,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Column {
    pub name: String,
    pub sql_type: Fact<SqlType>,
    pub nullable: Fact<bool>,
    pub default: Fact<Option<String>>,
    /// `None` means unspecified (provider default), not absent.
    pub length: Option<u32>,
    pub precision: Option<u32>,
    pub scale: Option<u32>,
    pub generated: Fact<bool>,
}

impl Column {
    pub fn new(name: &str, logical: LogicalType) -> Self {
        Self {
            name: name.to_owned(),
            sql_type: Fact::Known(SqlType { logical, raw: String::new() }),
            nullable: Fact::Known(true),
            default: Fact::Known(None),
            length: None,
            precision: None,
            scale: None,
            generated: Fact::Known(false),
        }
    }

    fn logical(&self) -> Option<LogicalType> {
        self.sql_type.known().map(|t| t.logical)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForeignKey {
    pub name: Option<String>,
    pub columns: Vec<String>,
    pub ref_table: String,
    pub ref_columns: Fact<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UniqueConstraint {
    pub name: Option<String>,
    pub columns: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Index {
    pub name: Option<String>,
    pub columns: Vec<String>,
    pub unique: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    pub name: String,
    pub columns: Fact<Vec<Column>>,
    pub primary_key: Fact<Vec<String>>,
    pub foreign_keys: Fact<Vec<ForeignKey>>,
    pub unique_constraints: Fact<Vec<UniqueConstraint>>,
    pub indexes: Fact<Vec<Index>>,
    pub provenance: Provenance,
}

impl Table {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            columns: Fact::Known(Vec::new()),
            primary_key: Fact::Known(Vec::new()),
            foreign_keys: Fact::Known(Vec::new()),
            unique_constraints: Fact::Known(Vec::new()),
            indexes: Fact::Known(Vec::new()),
            provenance: Provenance::Literal,
        }
    }

    /// Every facet unknown for one reason (for example an unprovable inheritance root).
    pub fn unknown(name: &str, provenance: Provenance, reason: &str) -> Self {
        Self {
            name: name.to_owned(),
            columns: Fact::unknown(reason),
            primary_key: Fact::unknown(reason),
            foreign_keys: Fact::unknown(reason),
            unique_constraints: Fact::unknown(reason),
            indexes: Fact::unknown(reason),
            provenance,
        }
    }
}

fn default_complete() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaSnapshot {
    pub version: u32,
    /// `false` when tables may be missing from `tables` (partial sources): absence is then unproven.
    #[serde(default = "default_complete")]
    pub complete: bool,
    pub tables: Vec<Table>,
}

impl Default for SchemaSnapshot {
    fn default() -> Self {
        Self { version: SCHEMA_VERSION, complete: true, tables: Vec::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "error", rename_all = "kebab-case")]
pub enum SchemaError {
    TooManyTables { count: usize },
    TooManyColumns { table: String, count: usize },
    EmptyName { table: Option<String> },
    DuplicateTable { table: String },
    DuplicateColumn { table: String, column: String },
    UnsupportedVersion { version: u32 },
}

impl SchemaSnapshot {
    /// Sorts tables and constraint lists so equal snapshots serialize identically.
    /// Column order is declaration order and is kept.
    pub fn normalize(&mut self) {
        self.tables.sort_by_key(|t| t.name.to_lowercase());
        for table in &mut self.tables {
            if let Fact::Known(v) = &mut table.foreign_keys {
                v.sort_by(|a, b| (&a.columns, &a.ref_table).cmp(&(&b.columns, &b.ref_table)));
            }
            if let Fact::Known(v) = &mut table.unique_constraints {
                v.sort_by(|a, b| a.columns.cmp(&b.columns));
            }
            if let Fact::Known(v) = &mut table.indexes {
                v.sort_by(|a, b| (&a.columns, a.unique).cmp(&(&b.columns, b.unique)));
            }
        }
    }

    /// Bounds, version, empty and duplicate names (identifiers compare case-insensitively).
    pub fn validate(&self) -> Result<(), SchemaError> {
        self.by_name().map(|_| ())
    }

    fn by_name(&self) -> Result<BTreeMap<String, &Table>, SchemaError> {
        if self.version != SCHEMA_VERSION {
            return Err(SchemaError::UnsupportedVersion { version: self.version });
        }
        if self.tables.len() > MAX_TABLES {
            return Err(SchemaError::TooManyTables { count: self.tables.len() });
        }

        let mut tables = BTreeMap::new();
        for table in &self.tables {
            if table.name.trim().is_empty() {
                return Err(SchemaError::EmptyName { table: None });
            }
            if tables.insert(table.name.to_lowercase(), table).is_some() {
                return Err(SchemaError::DuplicateTable { table: table.name.clone() });
            }
            if let Fact::Known(columns) = &table.columns {
                column_map(table, columns)?;
            }
        }

        Ok(tables)
    }
}

fn column_map<'a>(table: &Table, columns: &'a [Column]) -> Result<BTreeMap<String, &'a Column>, SchemaError> {
    if columns.len() > MAX_COLUMNS {
        return Err(SchemaError::TooManyColumns { table: table.name.clone(), count: columns.len() });
    }

    let mut map = BTreeMap::new();
    for column in columns {
        if column.name.trim().is_empty() {
            return Err(SchemaError::EmptyName { table: Some(table.name.clone()) });
        }
        if map.insert(column.name.to_lowercase(), column).is_some() {
            return Err(SchemaError::DuplicateColumn { table: table.name.clone(), column: column.name.clone() });
        }
    }

    Ok(map)
}

/// Risk of applying one change to a populated database.
///
/// | Danger | Rule |
/// |---|---|
/// | `Safe` | create table; add nullable column, or NOT NULL column with a default or generated value; widen to nullable; drop FK, unique or index |
/// | `NeedsBackfill` | add NOT NULL column without default; nullable to NOT NULL (existing NULL rows must get a value first) |
/// | `Rewrite` | any type change not provably narrowing; add FK, unique or index. May lock, scan or rewrite the table; per-dialect behavior is not asserted |
/// | `DataLoss` | drop table; drop column; provably narrowing type change (smaller integer, shorter varchar, lower decimal precision or scale, text to varchar, timestamp to date, time zone dropped) |
/// | `Ambiguous` | rename candidates; add column whose nullability is unknown |
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Danger {
    Safe,
    NeedsBackfill,
    Rewrite,
    DataLoss,
    Ambiguous,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "kebab-case")]
pub enum Change {
    CreateTable { table: Table },
    DropTable { table: String },
    AddColumn { table: String, column: Column },
    DropColumn { table: String, column: String },
    AlterColumnType { table: String, column: String, from: Column, to: Column },
    AlterNullability {
        table: String,
        column: String,
        nullable: bool,
        /// The column as the model declares it; lets a backfill be type-checked and MySQL restate the definition.
        #[serde(default)]
        column_type: Option<Column>,
    },
    AddForeignKey { table: String, foreign_key: ForeignKey },
    DropForeignKey { table: String, foreign_key: ForeignKey },
    AddUnique { table: String, unique: UniqueConstraint },
    DropUnique { table: String, unique: UniqueConstraint },
    AddIndex { table: String, index: Index },
    DropIndex { table: String, index: Index },
    /// Needs an explicit decision. The literal `DropColumn` and `AddColumn` stay in the plan.
    RenameCandidate { table: String, from: String, to: String },
}

impl Change {
    fn rank(&self) -> u8 {
        match self {
            Self::CreateTable { .. } => 0,
            Self::AddColumn { .. } => 1,
            Self::AlterColumnType { .. } => 2,
            Self::AlterNullability { .. } => 3,
            Self::DropForeignKey { .. } => 4,
            Self::DropUnique { .. } => 5,
            Self::DropIndex { .. } => 6,
            Self::AddUnique { .. } => 7,
            Self::AddIndex { .. } => 8,
            Self::AddForeignKey { .. } => 9,
            Self::RenameCandidate { .. } => 10,
            Self::DropColumn { .. } => 11,
            Self::DropTable { .. } => 12,
        }
    }

    fn table(&self) -> &str {
        match self {
            Self::CreateTable { table } => &table.name,
            Self::DropTable { table }
            | Self::AddColumn { table, .. }
            | Self::DropColumn { table, .. }
            | Self::AlterColumnType { table, .. }
            | Self::AlterNullability { table, .. }
            | Self::AddForeignKey { table, .. }
            | Self::DropForeignKey { table, .. }
            | Self::AddUnique { table, .. }
            | Self::DropUnique { table, .. }
            | Self::AddIndex { table, .. }
            | Self::DropIndex { table, .. }
            | Self::RenameCandidate { table, .. } => table,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedChange {
    pub change: Change,
    pub danger: Danger,
    pub reason: String,
}

/// A fact the diff could not decide, or a difference it does not plan.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub table: String,
    pub subject: String,
    pub reason: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangePlan {
    pub changes: Vec<PlannedChange>,
    pub notes: Vec<Note>,
}

const RENAME_PAIR_LIMIT: usize = 16;

/// `from` is the baseline (database or older snapshot), `to` the desired model.
pub fn diff(from: &SchemaSnapshot, to: &SchemaSnapshot) -> Result<ChangePlan, SchemaError> {
    let (old, new) = (from.by_name()?, to.by_name()?);
    let mut plan = ChangePlan::default();
    let mut names: Vec<&String> = old.keys().chain(new.keys()).collect();
    names.sort();
    names.dedup();

    for name in names {
        match (old.get(name), new.get(name)) {
            (None, Some(table)) => create_or_note(&mut plan, table, from.complete),
            (Some(table), None) => drop_or_note(&mut plan, table, to.complete),
            (Some(a), Some(b)) => diff_table(&mut plan, a, b)?,
            (None, None) => {}
        }
    }

    plan.changes.sort_by(|a, b| {
        (a.change.rank(), a.change.table().to_lowercase(), format!("{:?}", a.change))
            .cmp(&(b.change.rank(), b.change.table().to_lowercase(), format!("{:?}", b.change)))
    });
    plan.notes.sort();
    plan.notes.dedup();

    Ok(plan)
}

fn push(plan: &mut ChangePlan, change: Change, danger: Danger, reason: &str) {
    plan.changes.push(PlannedChange { change, danger, reason: reason.to_owned() });
}

fn note(plan: &mut ChangePlan, table: &str, subject: &str, reason: &str) {
    plan.notes.push(Note { table: table.to_owned(), subject: subject.to_owned(), reason: reason.to_owned() });
}

fn create_or_note(plan: &mut ChangePlan, table: &Table, baseline_complete: bool) {
    if !baseline_complete {
        note(plan, &table.name, "table", "baseline is incomplete: absence of this table is not proven");
        return;
    }
    if let Fact::Unknown(reason) = &table.columns {
        note(plan, &table.name, "table", &format!("cannot create a table with unknown columns: {reason}"));
        return;
    }

    push(plan, Change::CreateTable { table: table.clone() }, Danger::Safe, "new table");
}

fn drop_or_note(plan: &mut ChangePlan, table: &Table, model_complete: bool) {
    if !model_complete {
        note(plan, &table.name, "table", "model is incomplete: absence of this table is not proven");
        return;
    }

    push(plan, Change::DropTable { table: table.name.clone() }, Danger::DataLoss, "dropping a table deletes its rows");
}

fn facet<'a, T>(plan: &mut ChangePlan, table: &str, subject: &str, a: &'a Fact<T>, b: &'a Fact<T>) -> Option<(&'a T, &'a T)> {
    match (a, b) {
        (Fact::Known(x), Fact::Known(y)) => Some((x, y)),
        _ => {
            let reason = |f: &Fact<T>| match f {
                Fact::Unknown(r) => Some(r.clone()),
                Fact::Known(_) => None,
            };
            let why = [reason(a).map(|r| format!("baseline: {r}")), reason(b).map(|r| format!("model: {r}"))];
            note(plan, table, subject, &why.into_iter().flatten().collect::<Vec<_>>().join("; "));
            None
        }
    }
}

fn diff_table(plan: &mut ChangePlan, a: &Table, b: &Table) -> Result<(), SchemaError> {
    let t = b.name.as_str();

    if let Some((x, y)) = facet(plan, t, "columns", &a.columns, &b.columns) {
        diff_columns(plan, t, column_map(a, x)?, column_map(b, y)?);
    }

    if let Some((x, y)) = facet(plan, t, "primary key", &a.primary_key, &b.primary_key) {
        let lower = |v: &Vec<String>| v.iter().map(|c| c.to_lowercase()).collect::<Vec<_>>();
        if lower(x) != lower(y) {
            note(plan, t, "primary key", "primary key changes are not planned");
        }
    }

    if let Some((x, y)) = facet(plan, t, "foreign keys", &a.foreign_keys, &b.foreign_keys) {
        let key = |f: &ForeignKey| (lower_all(&f.columns), f.ref_table.to_lowercase(), f.ref_columns.known().map(|c| lower_all(c)));
        let same = |p: &ForeignKey, q: &ForeignKey| {
            key(p).0 == key(q).0
                && key(p).1 == key(q).1
                && (key(p).2.is_none() || key(q).2.is_none() || key(p).2 == key(q).2)
        };
        for f in x.iter().filter(|f| !y.iter().any(|g| same(f, g))) {
            push(plan, Change::DropForeignKey { table: t.to_owned(), foreign_key: f.clone() }, Danger::Safe, "drops a constraint, not data");
        }
        for f in y.iter().filter(|f| !x.iter().any(|g| same(f, g))) {
            push(plan, Change::AddForeignKey { table: t.to_owned(), foreign_key: f.clone() }, Danger::Rewrite, "validates existing rows and may lock; orphan rows fail it");
        }
    }

    if let Some((x, y)) = facet(plan, t, "unique constraints", &a.unique_constraints, &b.unique_constraints) {
        let same = |p: &UniqueConstraint, q: &UniqueConstraint| sorted_lower(&p.columns) == sorted_lower(&q.columns);
        for u in x.iter().filter(|u| !y.iter().any(|v| same(u, v))) {
            push(plan, Change::DropUnique { table: t.to_owned(), unique: u.clone() }, Danger::Safe, "drops a constraint, not data");
        }
        for u in y.iter().filter(|u| !x.iter().any(|v| same(u, v))) {
            push(plan, Change::AddUnique { table: t.to_owned(), unique: u.clone() }, Danger::Rewrite, "builds an index and may lock; duplicate rows fail it");
        }
    }

    if let Some((x, y)) = facet(plan, t, "indexes", &a.indexes, &b.indexes) {
        let same = |p: &Index, q: &Index| lower_all(&p.columns) == lower_all(&q.columns) && p.unique == q.unique;
        for i in x.iter().filter(|i| !y.iter().any(|j| same(i, j))) {
            push(plan, Change::DropIndex { table: t.to_owned(), index: i.clone() }, Danger::Safe, "drops an index, not data");
        }
        for i in y.iter().filter(|i| !x.iter().any(|j| same(i, j))) {
            push(plan, Change::AddIndex { table: t.to_owned(), index: i.clone() }, Danger::Rewrite, "builds an index and may lock");
        }
    }

    Ok(())
}

fn lower_all(v: &[String]) -> Vec<String> {
    v.iter().map(|c| c.to_lowercase()).collect()
}

fn sorted_lower(v: &[String]) -> Vec<String> {
    let mut v = lower_all(v);
    v.sort();
    v
}

fn diff_columns(plan: &mut ChangePlan, t: &str, old: BTreeMap<String, &Column>, new: BTreeMap<String, &Column>) {
    let dropped: Vec<&Column> = old.iter().filter(|(k, _)| !new.contains_key(*k)).map(|(_, c)| *c).collect();
    let added: Vec<&Column> = new.iter().filter(|(k, _)| !old.contains_key(*k)).map(|(_, c)| *c).collect();

    for c in &dropped {
        push(plan, Change::DropColumn { table: t.to_owned(), column: c.name.clone() }, Danger::DataLoss, "dropping a column deletes its values");
    }
    for c in &added {
        let (danger, reason) = add_column_danger(c);
        push(plan, Change::AddColumn { table: t.to_owned(), column: (*c).clone() }, danger, reason);
    }

    rename_candidates(plan, t, &dropped, &added);

    for (key, before) in &old {
        if let Some(after) = new.get(key) {
            diff_column(plan, t, before, after);
        }
    }
}

fn add_column_danger(c: &Column) -> (Danger, &'static str) {
    match &c.nullable {
        Fact::Unknown(_) => (Danger::Ambiguous, "nullability of the new column is unknown"),
        Fact::Known(true) => (Danger::Safe, "nullable column"),
        Fact::Known(false) => {
            let has_default = matches!(c.default, Fact::Known(Some(_)));
            if has_default || c.generated == Fact::Known(true) {
                (Danger::Safe, "NOT NULL column with a default or generated value")
            } else {
                (Danger::NeedsBackfill, "NOT NULL column without default needs a value for existing rows")
            }
        }
    }
}

fn rename_candidates(plan: &mut ChangePlan, t: &str, dropped: &[&Column], added: &[&Column]) {
    let mut groups: BTreeMap<LogicalType, (Vec<&Column>, Vec<&Column>)> = BTreeMap::new();
    for c in dropped {
        if let Some(l) = c.logical() {
            groups.entry(l).or_default().0.push(c);
        }
    }
    for c in added {
        if let Some(l) = c.logical() {
            groups.entry(l).or_default().1.push(c);
        }
    }

    for (d, a) in groups.values().filter(|(d, a)| !d.is_empty() && !a.is_empty()) {
        if d.len() * a.len() > RENAME_PAIR_LIMIT {
            note(plan, t, "rename candidates", "too many same-type dropped and added columns to pair; review them manually");
            continue;
        }
        for from in d {
            for to in a {
                push(
                    plan,
                    Change::RenameCandidate { table: t.to_owned(), from: from.name.clone(), to: to.name.clone() },
                    Danger::Ambiguous,
                    "same logical type dropped and added; a rename needs an explicit decision and is never inferred",
                );
            }
        }
    }
}

fn diff_column(plan: &mut ChangePlan, t: &str, before: &Column, after: &Column) {
    match (&before.sql_type, &after.sql_type) {
        (Fact::Known(_), Fact::Known(_)) => {
            if type_differs(before, after) {
                let (danger, reason) = if narrowing(before, after) {
                    (Danger::DataLoss, "narrowing type change can truncate or reject existing values")
                } else {
                    (Danger::Rewrite, "type change may rewrite the table or fail on existing values")
                };
                push(plan, Change::AlterColumnType { table: t.to_owned(), column: after.name.clone(), from: before.clone(), to: after.clone() }, danger, reason);
            }
        }
        _ => note(plan, t, &format!("column {}", after.name), "type unknown on one side; not compared"),
    }

    match (&before.nullable, &after.nullable) {
        (Fact::Known(true), Fact::Known(false)) => push(
            plan,
            Change::AlterNullability { table: t.to_owned(), column: after.name.clone(), nullable: false, column_type: Some(after.clone()) },
            Danger::NeedsBackfill,
            "existing NULL rows must be given a value before NOT NULL; a default does not rewrite them",
        ),
        (Fact::Known(false), Fact::Known(true)) => push(
            plan,
            Change::AlterNullability { table: t.to_owned(), column: after.name.clone(), nullable: true, column_type: Some(after.clone()) },
            Danger::Safe,
            "dropping NOT NULL keeps all rows valid",
        ),
        (Fact::Known(_), Fact::Known(_)) => {}
        _ => note(plan, t, &format!("column {}", after.name), "nullability unknown on one side; not compared"),
    }
}

fn differs(a: Option<u32>, b: Option<u32>) -> bool {
    matches!((a, b), (Some(x), Some(y)) if x != y)
}

fn type_differs(a: &Column, b: &Column) -> bool {
    a.logical() != b.logical() || differs(a.length, b.length) || differs(a.precision, b.precision) || differs(a.scale, b.scale)
}

fn rank(l: LogicalType) -> Option<(u8, u8)> {
    use LogicalType::*;
    Some(match l {
        Tinyint => (0, 0),
        Smallint => (0, 1),
        Integer => (0, 2),
        Bigint => (0, 3),
        Real => (1, 1),
        Double => (1, 2),
        Date => (2, 1),
        Timestamp => (2, 2),
        TimestampTz => (2, 3),
        Char => (3, 1),
        Varchar => (3, 2),
        Text => (3, 3),
        _ => return None,
    })
}

fn shrinks(a: Option<u32>, b: Option<u32>) -> bool {
    matches!((a, b), (Some(x), Some(y)) if y < x)
}

/// True only when the change provably loses range; everything else stays `Rewrite`.
fn narrowing(a: &Column, b: &Column) -> bool {
    let (Some(x), Some(y)) = (a.logical(), b.logical()) else {
        return false;
    };

    if x == y {
        return match x {
            LogicalType::Varchar | LogicalType::Char => shrinks(a.length, b.length),
            LogicalType::Decimal => shrinks(a.precision, b.precision) || shrinks(a.scale, b.scale),
            _ => false,
        };
    }

    match (rank(x), rank(y)) {
        (Some((fx, rx)), Some((fy, ry))) if fx == fy => ry < rx,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use LogicalType::*;

    fn col(name: &str, l: LogicalType) -> Column {
        Column::new(name, l)
    }

    fn table(name: &str, columns: Vec<Column>) -> Table {
        let mut t = Table::new(name);
        t.columns = Fact::Known(columns);
        t
    }

    fn snap(tables: Vec<Table>) -> SchemaSnapshot {
        SchemaSnapshot { tables, ..Default::default() }
    }

    fn kinds(plan: &ChangePlan) -> Vec<(u8, Danger)> {
        plan.changes.iter().map(|c| (c.change.rank(), c.danger)).collect()
    }

    fn one(from: Column, to: Column) -> ChangePlan {
        diff(&snap(vec![table("t", vec![from])]), &snap(vec![table("t", vec![to])])).unwrap()
    }

    #[test]
    fn identical_snapshots_have_an_empty_plan() {
        let s = snap(vec![table("a", vec![col("x", Integer)])]);
        assert_eq!(diff(&s, &s).unwrap(), ChangePlan::default());
    }

    #[test]
    fn create_and_drop_table() {
        let a = snap(vec![table("old", vec![col("x", Integer)])]);
        let b = snap(vec![table("new", vec![col("x", Integer)])]);
        let plan = diff(&a, &b).unwrap();
        assert_eq!(kinds(&plan), [(0, Danger::Safe), (12, Danger::DataLoss)]);
    }

    #[test]
    fn incomplete_snapshots_never_prove_absence() {
        let full = snap(vec![table("a", vec![col("x", Integer)])]);
        let mut partial = snap(vec![]);
        partial.complete = false;
        assert!(diff(&full, &partial).unwrap().changes.is_empty());
        assert_eq!(diff(&full, &partial).unwrap().notes.len(), 1);
        assert!(diff(&partial, &full).unwrap().changes.is_empty());
    }

    #[test]
    fn unknown_facets_become_notes_not_drops() {
        let a = snap(vec![table("a", vec![col("x", Integer)])]);
        let b = snap(vec![Table::unknown("a", Provenance::Derived, "inheritance root outside sources")]);
        let plan = diff(&a, &b).unwrap();
        assert!(plan.changes.is_empty());
        assert_eq!(plan.notes.len(), 5);
        assert!(plan.notes[0].reason.contains("inheritance root"));
    }

    #[test]
    fn unknown_new_table_is_not_created() {
        let b = snap(vec![Table::unknown("a", Provenance::Derived, "why")]);
        let plan = diff(&snap(vec![]), &b).unwrap();
        assert!(plan.changes.is_empty() && plan.notes.len() == 1);
    }

    #[test]
    fn add_column_dangers() {
        let mut nn = col("c", Integer);
        nn.nullable = Fact::Known(false);
        let mut with_default = nn.clone();
        with_default.default = Fact::Known(Some("0".into()));
        let mut generated = nn.clone();
        generated.generated = Fact::Known(true);
        let mut unknown = col("c", Integer);
        unknown.nullable = Fact::unknown("x");

        for (c, danger) in [
            (col("c", Integer), Danger::Safe),
            (nn, Danger::NeedsBackfill),
            (with_default, Danger::Safe),
            (generated, Danger::Safe),
            (unknown, Danger::Ambiguous),
        ] {
            let plan = diff(&snap(vec![table("t", vec![])]), &snap(vec![table("t", vec![c])])).unwrap();
            assert_eq!(plan.changes[0].danger, danger);
        }
    }

    #[test]
    fn drop_column_is_data_loss() {
        let plan = diff(&snap(vec![table("t", vec![col("a", Text)])]), &snap(vec![table("t", vec![])])).unwrap();
        assert_eq!(kinds(&plan), [(11, Danger::DataLoss)]);
    }

    #[test]
    fn nullability_rules() {
        let mut nn = col("c", Integer);
        nn.nullable = Fact::Known(false);
        assert_eq!(kinds(&one(col("c", Integer), nn.clone())), [(3, Danger::NeedsBackfill)]);
        assert_eq!(kinds(&one(nn.clone(), col("c", Integer))), [(3, Danger::Safe)]);
        assert!(one(nn.clone(), nn).changes.is_empty());
    }

    #[test]
    fn type_change_rules() {
        let danger = |a: Column, b: Column| one(a, b).changes[0].danger;
        assert_eq!(danger(col("c", Integer), col("c", Bigint)), Danger::Rewrite);
        assert_eq!(danger(col("c", Bigint), col("c", Integer)), Danger::DataLoss);
        assert_eq!(danger(col("c", Double), col("c", Real)), Danger::DataLoss);
        assert_eq!(danger(col("c", Text), col("c", Varchar)), Danger::DataLoss);
        assert_eq!(danger(col("c", Timestamp), col("c", Date)), Danger::DataLoss);
        assert_eq!(danger(col("c", Varchar), col("c", Integer)), Danger::Rewrite);

        let mut short = col("c", Varchar);
        short.length = Some(20);
        let mut long = col("c", Varchar);
        long.length = Some(255);
        assert_eq!(danger(long.clone(), short.clone()), Danger::DataLoss);
        assert_eq!(danger(short, long), Danger::Rewrite);

        let mut p = col("c", Decimal);
        p.precision = Some(10);
        p.scale = Some(2);
        let mut q = p.clone();
        q.scale = Some(4);
        assert_eq!(danger(q.clone(), p.clone()), Danger::DataLoss);
        assert_eq!(danger(p, q), Danger::Rewrite);
    }

    #[test]
    fn unspecified_sizes_do_not_differ() {
        let mut sized = col("c", Varchar);
        sized.length = Some(255);
        assert!(one(col("c", Varchar), sized).changes.is_empty());
    }

    #[test]
    fn unknown_type_is_noted_not_compared() {
        let mut u = col("c", Integer);
        u.sql_type = Fact::unknown("custom type");
        let plan = one(col("c", Integer), u);
        assert!(plan.changes.is_empty());
        assert_eq!(plan.notes.len(), 1);
    }

    #[test]
    fn rename_is_a_candidate_next_to_literal_drop_and_add() {
        let a = snap(vec![table("t", vec![col("name", Varchar)])]);
        let b = snap(vec![table("t", vec![col("full_name", Varchar)])]);
        let plan = diff(&a, &b).unwrap();
        assert_eq!(kinds(&plan), [(1, Danger::Safe), (10, Danger::Ambiguous), (11, Danger::DataLoss)]);
        assert!(matches!(&plan.changes[1].change, Change::RenameCandidate { from, to, .. } if from == "name" && to == "full_name"));
    }

    #[test]
    fn different_types_are_not_rename_candidates() {
        let a = snap(vec![table("t", vec![col("a", Varchar)])]);
        let b = snap(vec![table("t", vec![col("b", Integer)])]);
        assert!(!kinds(&diff(&a, &b).unwrap()).iter().any(|(r, _)| *r == 10));
    }

    #[test]
    fn many_same_type_columns_are_not_paired() {
        let cols = |p: &str| (0..5).map(|i| col(&format!("{p}{i}"), Integer)).collect::<Vec<_>>();
        let plan = diff(&snap(vec![table("t", cols("a"))]), &snap(vec![table("t", cols("b"))])).unwrap();
        assert!(!kinds(&plan).iter().any(|(r, _)| *r == 10));
        assert_eq!(plan.notes.len(), 1);
    }

    #[test]
    fn constraints_diff_by_identity() {
        let fk = ForeignKey { name: None, columns: vec!["u_id".into()], ref_table: "u".into(), ref_columns: Fact::Known(vec!["id".into()]) };
        let uq = UniqueConstraint { name: None, columns: vec!["a".into(), "b".into()] };
        let ix = Index { name: None, columns: vec!["a".into()], unique: false };
        let empty = table("t", vec![]);
        let mut full = empty.clone();
        full.foreign_keys = Fact::Known(vec![fk.clone()]);
        full.unique_constraints = Fact::Known(vec![uq]);
        full.indexes = Fact::Known(vec![ix]);

        let up = diff(&snap(vec![empty.clone()]), &snap(vec![full.clone()])).unwrap();
        assert_eq!(kinds(&up), [(7, Danger::Rewrite), (8, Danger::Rewrite), (9, Danger::Rewrite)]);
        let down = diff(&snap(vec![full.clone()]), &snap(vec![empty])).unwrap();
        assert_eq!(kinds(&down), [(4, Danger::Safe), (5, Danger::Safe), (6, Danger::Safe)]);

        let mut reordered = full.clone();
        reordered.unique_constraints = Fact::Known(vec![UniqueConstraint { name: Some("n".into()), columns: vec!["B".into(), "a".into()] }]);
        assert!(diff(&snap(vec![full.clone()]), &snap(vec![reordered])).unwrap().changes.is_empty());

        let mut other_ref = full.clone();
        other_ref.foreign_keys = Fact::Known(vec![ForeignKey { ref_columns: Fact::Known(vec!["uid".into()]), ..fk.clone() }]);
        assert_eq!(kinds(&diff(&snap(vec![full.clone()]), &snap(vec![other_ref])).unwrap()), [(4, Danger::Safe), (9, Danger::Rewrite)]);

        let mut unknown_ref = full.clone();
        unknown_ref.foreign_keys = Fact::Known(vec![ForeignKey { ref_columns: Fact::unknown("x"), ..fk }]);
        assert!(diff(&snap(vec![full]), &snap(vec![unknown_ref])).unwrap().changes.is_empty());
    }

    #[test]
    fn primary_key_change_is_noted() {
        let mut a = table("t", vec![col("id", Bigint)]);
        let b = a.clone();
        a.primary_key = Fact::Known(vec!["id".into()]);
        let plan = diff(&snap(vec![a]), &snap(vec![b])).unwrap();
        assert!(plan.changes.is_empty() && plan.notes[0].subject == "primary key");
    }

    #[test]
    fn bounds_and_malformed_input_are_errors() {
        let many_tables = snap((0..=MAX_TABLES).map(|i| Table::new(&format!("t{i}"))).collect());
        assert_eq!(many_tables.validate(), Err(SchemaError::TooManyTables { count: 501 }));
        let wide = snap(vec![table("t", (0..=MAX_COLUMNS).map(|i| col(&format!("c{i}"), Integer)).collect())]);
        assert!(matches!(diff(&wide, &wide), Err(SchemaError::TooManyColumns { .. })));
        assert!(matches!(snap(vec![Table::new(" ")]).validate(), Err(SchemaError::EmptyName { table: None })));
        assert!(matches!(snap(vec![Table::new("A"), Table::new("a")]).validate(), Err(SchemaError::DuplicateTable { .. })));
        assert!(matches!(snap(vec![table("t", vec![col("X", Integer), col("x", Integer)])]).validate(), Err(SchemaError::DuplicateColumn { .. })));
        assert!(matches!(snap(vec![table("t", vec![col("", Integer)])]).validate(), Err(SchemaError::EmptyName { table: Some(_) })));
        let mut bad = snap(vec![]);
        bad.version = 9;
        assert_eq!(diff(&bad, &bad), Err(SchemaError::UnsupportedVersion { version: 9 }));
        assert!(serde_json::from_str::<SchemaSnapshot>("{\"version\":1,\"tables\":[{\"nope\":1}]}").is_err());
        assert!(serde_json::from_str::<SchemaSnapshot>("not json").is_err());
    }

    fn lcg(seed: &mut u64) -> u64 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *seed >> 33
    }

    fn random_snapshot(seed: &mut u64) -> SchemaSnapshot {
        let types = [Smallint, Integer, Bigint, Boolean, Varchar, Text, Decimal, Timestamp];
        let tables = (0..lcg(seed) % 4)
            .map(|i| {
                let columns = (0..lcg(seed) % 5)
                    .map(|j| {
                        let mut c = col(&format!("c{j}"), types[(lcg(seed) % 8) as usize]);
                        c.nullable = Fact::Known(lcg(seed) % 2 == 0);
                        c
                    })
                    .collect();
                table(&format!("t{i}"), columns)
            })
            .collect();
        snap(tables)
    }

    #[test]
    fn property_self_diff_empty_and_order_independent() {
        let mut seed = 7;
        for _ in 0..200 {
            let a = random_snapshot(&mut seed);
            let b = random_snapshot(&mut seed);
            assert!(diff(&a, &a).unwrap().changes.is_empty());

            let mut reversed = b.clone();
            reversed.tables.reverse();
            let (p1, p2) = (diff(&a, &b).unwrap(), diff(&a, &reversed).unwrap());
            assert_eq!(serde_json::to_string(&p1).unwrap(), serde_json::to_string(&p2).unwrap());

            let inverse = diff(&b, &a).unwrap();
            let count = |p: &ChangePlan, r: u8| p.changes.iter().filter(|c| c.change.rank() == r).count();
            assert_eq!(count(&p1, 1) + count(&p1, 0), count(&inverse, 11) + count(&inverse, 12));
        }
    }

    #[test]
    fn snapshot_json_is_stable_and_round_trips() {
        let mut s = snap(vec![table("b", vec![col("x", Integer)]), table("a", vec![])]);
        s.normalize();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.find("\"a\"").unwrap() < json.find("\"b\"").unwrap());
        assert_eq!(serde_json::from_str::<SchemaSnapshot>(&json).unwrap(), s);
        assert!(json.contains("\"state\":\"known\""));
        assert_eq!(serde_json::to_string(&Fact::<bool>::unknown("r")).unwrap(), "{\"state\":\"unknown\",\"value\":\"r\"}");
    }
}
