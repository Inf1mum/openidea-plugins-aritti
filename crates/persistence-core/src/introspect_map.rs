//! Maps raw helper metadata (`introspect::TableMeta`, from `DatabaseMetaData`) to a `SchemaSnapshot`.
//!
//! PostgreSQL and MySQL (MariaDB reuses the MySQL rows). Anything the metadata cannot prove is
//! `Fact::Unknown` with a reason, never a guess and never an absence. Renames are not inferred.
//! Rules follow `parity/JDBC_HELPER.md` section 5; the type tables are pinned by the fixtures in
//! `fixtures/jdbc/`, which are synthetic until recorded from real servers.
use crate::ddl_parse::Dialect;
use crate::introspect::{RawColumn, RawForeignKey, RawIndex, TableMeta};
use crate::schema::{
    Column, Fact, ForeignKey, Index, LogicalType, MAX_COLUMNS, MAX_TABLES, Provenance, SchemaSnapshot, SqlType, Table, UniqueConstraint,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapOptions {
    /// `true` only when the table listing was complete (no schema/pattern filter, no truncation).
    /// A filtered or partial listing yields `complete: false`: absence of other tables is unproven.
    pub listing_complete: bool,
    /// Emit `schema.table` names. Off by default: the entity-side snapshot does not model schema-qualified
    /// tables, so bare names are what a diff can compare.
    pub qualify_schema: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mapped {
    pub snapshot: SchemaSnapshot,
    /// Views and materialized views that were given: not tables, left to the view workflow.
    pub views: Vec<String>,
    /// Tables dropped because the snapshot is bounded to `MAX_TABLES`; `snapshot.complete` is then `false`.
    pub omitted_tables: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapError {
    /// Two tables share a name once schemas are dropped; filter to one schema or set `qualify_schema`.
    DuplicateTable { name: String },
}

pub fn to_snapshot(dialect: Dialect, tables: &[TableMeta], options: &MapOptions) -> Result<Mapped, MapError> {
    let mut mapped = Vec::new();
    let mut views = Vec::new();
    for meta in tables {
        match meta.kind.as_deref() {
            Some("TABLE") | None => mapped.push(map_table(dialect, meta, options)),
            Some(_) => views.push(table_name(meta, options)),
        }
    }
    mapped.sort_by_key(|t: &Table| t.name.to_lowercase());

    let mut seen = BTreeSet::new();
    for table in &mapped {
        if !seen.insert(table.name.to_lowercase()) {
            return Err(MapError::DuplicateTable { name: table.name.clone() });
        }
    }

    let omitted_tables = mapped.len().saturating_sub(MAX_TABLES);
    mapped.truncate(MAX_TABLES);
    let mut snapshot = SchemaSnapshot { complete: options.listing_complete && omitted_tables == 0, tables: mapped, ..SchemaSnapshot::default() };
    snapshot.normalize();
    views.sort();
    Ok(Mapped { snapshot, views, omitted_tables })
}

fn table_name(meta: &TableMeta, options: &MapOptions) -> String {
    match meta.schema.as_deref().filter(|s| !s.is_empty()) {
        Some(schema) if options.qualify_schema => format!("{schema}.{}", meta.name),
        _ => meta.name.clone(),
    }
}

fn map_table(dialect: Dialect, meta: &TableMeta, options: &MapOptions) -> Table {
    let name = table_name(meta, options);
    if !meta.found {
        return Table::unknown(&name, Provenance::Literal, "table not found in database metadata");
    }

    let columns = match (&meta.columns, meta.facet_errors.get("columns")) {
        (Some(raw), _) if raw.len() > MAX_COLUMNS => Fact::Unknown(format!("table has {} columns, more than the {MAX_COLUMNS} limit", raw.len())),
        (Some(raw), _) => {
            let mut ordered: Vec<&RawColumn> = raw.iter().collect();
            ordered.sort_by_key(|c| c.position);
            Fact::Known(ordered.into_iter().map(|c| map_column(dialect, c)).collect())
        }
        (None, reason) => unavailable("columns", reason),
    };
    let primary_key = match (&meta.primary_key, meta.facet_errors.get("primaryKey")) {
        (Some(pk), _) => Fact::Known(pk.columns.clone()),
        (None, reason) => unavailable("primary key", reason),
    };
    let foreign_keys = match (&meta.foreign_keys, meta.facet_errors.get("foreignKeys")) {
        (Some(raw), _) => Fact::Known(raw.iter().map(|fk| map_foreign_key(meta, fk, options)).collect()),
        (None, reason) => unavailable("foreign keys", reason),
    };

    let (unique_constraints, indexes) = match (&meta.indexes, meta.facet_errors.get("indexes")) {
        (Some(raw), _) => map_indexes(dialect, raw, &columns, &primary_key, &foreign_keys),
        (None, reason) => {
            let why = unavailable_reason("indexes", reason);
            (Fact::Unknown(why.clone()), Fact::Unknown(why))
        }
    };

    Table { name, columns, primary_key, foreign_keys, unique_constraints, indexes, provenance: Provenance::Literal }
}

fn unavailable_reason(facet: &str, reason: Option<&String>) -> String {
    match reason {
        Some(reason) => format!("{facet} metadata could not be read: {reason}"),
        None => format!("{facet} metadata was not returned"),
    }
}

fn unavailable<T>(facet: &str, reason: Option<&String>) -> Fact<T> {
    Fact::Unknown(unavailable_reason(facet, reason))
}

fn map_foreign_key(meta: &TableMeta, fk: &RawForeignKey, options: &MapOptions) -> ForeignKey {
    let own = meta.schema.as_deref().filter(|s| !s.is_empty());
    let other = fk.ref_schema.as_deref().filter(|s| !s.is_empty());
    let cross_schema = match (own, other) {
        (Some(a), Some(b)) => !a.eq_ignore_ascii_case(b),
        _ => false,
    };
    let ref_table = match other {
        Some(schema) if options.qualify_schema || cross_schema => format!("{schema}.{}", fk.ref_table),
        _ => fk.ref_table.clone(),
    };
    ForeignKey { name: fk.name.clone(), columns: fk.columns.clone(), ref_table, ref_columns: Fact::Known(fk.ref_columns.clone()) }
}

fn lower(names: &[String]) -> Vec<String> {
    names.iter().map(|n| n.to_lowercase()).collect()
}

/// A unique index that is not the primary key becomes a unique constraint and is not repeated as an index
/// (metadata cannot tell a unique constraint from a unique index; the DDL parser makes the same choice).
/// Expression and partial indexes make their facet Unknown. Descending key parts stay plain column names.
fn map_indexes(
    dialect: Dialect,
    raw: &[RawIndex],
    columns: &Fact<Vec<Column>>,
    primary_key: &Fact<Vec<String>>,
    foreign_keys: &Fact<Vec<ForeignKey>>,
) -> (Fact<Vec<UniqueConstraint>>, Fact<Vec<Index>>) {
    let known_columns: Option<BTreeSet<String>> = columns.known().map(|cols| cols.iter().map(|c| c.name.to_lowercase()).collect());
    let pk_columns = primary_key.known().map(|pk| lower(pk));

    let mut uniques: Fact<Vec<UniqueConstraint>> = Fact::Known(Vec::new());
    let mut plain: Fact<Vec<Index>> = Fact::Known(Vec::new());

    if pk_columns.is_none() {
        uniques = Fact::unknown("primary key unknown, so the primary key index cannot be told from a unique index");
    }
    if dialect == Dialect::MySql && foreign_keys.known().is_none() {
        plain = Fact::unknown("foreign keys unknown, so foreign key backing indexes cannot be told from declared indexes");
    }

    for index in raw {
        let names: Vec<Option<&str>> = index.columns.iter().map(|c| c.name.as_deref()).collect();
        let lossy = index.filter.as_deref().is_some_and(|f| !f.trim().is_empty())
            || names.iter().any(|n| match n {
                None => true,
                Some(n) => known_columns.as_ref().is_some_and(|known| !known.contains(&n.to_lowercase())),
            });
        if lossy {
            if index.unique {
                uniques = Fact::unknown("expression or partial unique index");
            } else {
                plain = Fact::unknown("expression or partial index");
            }
            continue;
        }

        let columns: Vec<String> = names.into_iter().flatten().map(str::to_owned).collect();
        if index.unique {
            if pk_columns.as_ref().is_some_and(|pk| *pk == lower(&columns)) {
                continue;
            }
            if let Fact::Known(list) = &mut uniques {
                list.push(UniqueConstraint { name: Some(index.name.clone()), columns });
            }
            continue;
        }
        if is_foreign_key_backing(dialect, index, &columns, foreign_keys) {
            continue;
        }
        if let Fact::Known(list) = &mut plain {
            list.push(Index { name: Some(index.name.clone()), columns, unique: false });
        }
    }
    (uniques, plain)
}

/// MySQL (and H2) create a non-unique index named like the constraint to back a foreign key.
fn is_foreign_key_backing(dialect: Dialect, index: &RawIndex, columns: &[String], foreign_keys: &Fact<Vec<ForeignKey>>) -> bool {
    if dialect != Dialect::MySql {
        return false;
    }
    let Some(fks) = foreign_keys.known() else {
        return false;
    };
    let index_name = index.name.to_lowercase();
    fks.iter().any(|fk| lower(&fk.columns) == lower(columns) && fk.name.as_deref().is_some_and(|n| index_name.starts_with(&n.to_lowercase())))
}

fn map_column(dialect: Dialect, raw: &RawColumn) -> Column {
    let mapped = match dialect {
        Dialect::PostgreSql => postgres_type(raw),
        Dialect::MySql => mysql_type(raw),
    };
    let sql_type = match mapped.logical {
        Some(logical) => Fact::Known(SqlType { logical, raw: mapped.raw }),
        None => Fact::Unknown(format!("unmapped SQL type '{}' (java.sql.Types {})", raw.type_name, raw.data_type)),
    };
    let nullable = match raw.nullable {
        Some(0) => Fact::Known(false),
        Some(1) => Fact::Known(true),
        _ => Fact::unknown("the driver reports nullability as unknown"),
    };
    Column {
        name: raw.name.clone(),
        sql_type,
        nullable,
        default: Fact::Known(raw.default.clone()),
        length: mapped.length,
        precision: mapped.precision,
        scale: mapped.scale,
        generated: generated(raw),
    }
}

fn generated(raw: &RawColumn) -> Fact<bool> {
    let yes = |v: &Option<String>| v.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("YES"));
    let no = |v: &Option<String>| v.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("NO"));
    if yes(&raw.auto_increment) || yes(&raw.generated) {
        Fact::Known(true)
    } else if no(&raw.auto_increment) || no(&raw.generated) {
        Fact::Known(false)
    } else {
        Fact::unknown("the driver did not report IS_AUTOINCREMENT or IS_GENERATEDCOLUMN")
    }
}

struct TypeMap {
    logical: Option<LogicalType>,
    raw: String,
    length: Option<u32>,
    precision: Option<u32>,
    scale: Option<u32>,
}

fn unsigned_size(value: Option<i64>) -> Option<u32> {
    value.filter(|v| *v > 0).and_then(|v| u32::try_from(v).ok())
}

fn scale_of(raw: &RawColumn) -> Option<u32> {
    raw.scale.filter(|v| *v >= 0).and_then(|v| u32::try_from(v).ok())
}

/// Fills length or precision and scale for the types that carry them and spells `raw` with them.
fn finish(logical: Option<LogicalType>, type_name: &str, raw: &RawColumn, length_cap: u32, precision_cap: u32) -> TypeMap {
    let size = unsigned_size(raw.size);
    let mut map = TypeMap { logical, raw: type_name.to_owned(), length: None, precision: None, scale: None };
    match logical {
        Some(LogicalType::Char | LogicalType::Varchar) => {
            map.length = size.filter(|s| *s <= length_cap);
            if let Some(length) = map.length {
                map.raw = format!("{type_name}({length})");
            }
        }
        Some(LogicalType::Decimal) => {
            map.precision = size.filter(|s| *s <= precision_cap);
            if let Some(precision) = map.precision {
                map.scale = scale_of(raw);
                map.raw = format!("{type_name}({precision},{})", map.scale.unwrap_or(0));
            }
        }
        _ => {}
    }
    map
}

/// PostgreSQL `TYPE_NAME` as pgjdbc reports it (`int4`, `varchar`, `timestamptz`, `serial` for serial columns).
/// An unbounded varchar reports 2147483647 and an unconstrained numeric reports 0 or a huge size: both mean unspecified.
fn postgres_type(raw: &RawColumn) -> TypeMap {
    use LogicalType::*;
    let name = raw.type_name.trim();
    let logical = match name.to_ascii_lowercase().as_str() {
        "int2" | "smallint" | "smallserial" | "serial2" => Some(Smallint),
        "int4" | "int" | "integer" | "serial" | "serial4" => Some(Integer),
        "int8" | "bigint" | "bigserial" | "serial8" => Some(Bigint),
        "bool" | "boolean" => Some(Boolean),
        "float4" | "real" => Some(Real),
        "float8" | "double precision" => Some(Double),
        "numeric" | "decimal" => Some(Decimal),
        "bpchar" | "char" | "character" => Some(Char),
        "varchar" | "character varying" => Some(Varchar),
        "text" => Some(Text),
        "date" => Some(Date),
        "time" | "time without time zone" => Some(Time),
        "timestamp" | "timestamp without time zone" => Some(Timestamp),
        "timestamptz" | "timestamp with time zone" => Some(TimestampTz),
        "uuid" => Some(Uuid),
        "bytea" => Some(Binary),
        _ => None,
    };
    finish(logical, name, raw, 10_485_760, 1000)
}

/// MySQL and MariaDB `TYPE_NAME` as Connector/J reports it, including the ` UNSIGNED` suffix.
/// Unsigned integers widen to the next signed type so every value fits; `BIGINT UNSIGNED` becomes `DECIMAL(20,0)`.
fn mysql_type(raw: &RawColumn) -> TypeMap {
    use LogicalType::*;
    let name = raw.type_name.trim();
    let upper = name.to_ascii_uppercase();
    let mut unsigned = false;
    let mut words = Vec::new();
    for word in upper.split_whitespace() {
        match word {
            "UNSIGNED" => unsigned = true,
            "SIGNED" | "ZEROFILL" => {}
            other => words.push(other),
        }
    }
    let base = words.join(" ");
    let size = unsigned_size(raw.size);

    let logical = match base.as_str() {
        "TINYINT" if raw.data_type == -7 => Some(Boolean),
        "TINYINT" => Some(if unsigned { Smallint } else { Tinyint }),
        "SMALLINT" => Some(if unsigned { Integer } else { Smallint }),
        "MEDIUMINT" => Some(Integer),
        "INT" | "INTEGER" => Some(if unsigned { Bigint } else { Integer }),
        "BIGINT" if unsigned => Some(Decimal),
        "BIGINT" => Some(Bigint),
        "BIT" if size == Some(1) => Some(Boolean),
        "BOOL" | "BOOLEAN" => Some(Boolean),
        "FLOAT" | "REAL" => Some(Real),
        "DOUBLE" | "DOUBLE PRECISION" => Some(Double),
        "DECIMAL" | "NUMERIC" | "DEC" => Some(Decimal),
        "CHAR" => Some(Char),
        "VARCHAR" => Some(Varchar),
        "TINYTEXT" | "TEXT" | "MEDIUMTEXT" | "LONGTEXT" => Some(Text),
        "DATE" => Some(Date),
        "TIME" => Some(Time),
        "DATETIME" | "TIMESTAMP" => Some(Timestamp),
        "BINARY" | "VARBINARY" | "TINYBLOB" | "BLOB" | "MEDIUMBLOB" | "LONGBLOB" => Some(Binary),
        _ => None,
    };

    let mut map = finish(logical, name, raw, 65_535, 65);
    if base == "BIGINT" && unsigned && logical.is_some() {
        map.precision = Some(20);
        map.scale = Some(0);
        map.raw = name.to_owned();
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::introspect::{RawColumn, RawPrimaryKey};
    use crate::schema::{LogicalType as L, SchemaError};
    use serde_json::Value;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/jdbc").join(name)
    }

    fn load(name: &str) -> Vec<TableMeta> {
        let text = std::fs::read_to_string(fixture_path(name)).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert!(value["_provenance"].as_str().unwrap().starts_with("SYNTHETIC"), "fixtures from this increment are synthetic");
        serde_json::from_value(value["result"]["tables"].clone()).unwrap()
    }

    fn check_golden(name: &str, snapshot: &SchemaSnapshot) {
        let actual = format!("{}\n", serde_json::to_string_pretty(snapshot).unwrap());
        let path = fixture_path(name);
        if std::env::var("UPDATE_GOLDEN").is_ok() {
            std::fs::write(&path, &actual).unwrap();
        }
        let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing golden {name}; run with UPDATE_GOLDEN=1"));
        assert_eq!(actual, expected, "golden {name} differs");
    }

    fn map(dialect: Dialect, fixture: &str, options: &MapOptions) -> Mapped {
        to_snapshot(dialect, &load(fixture), options).unwrap()
    }

    fn table<'a>(mapped: &'a Mapped, name: &str) -> &'a Table {
        mapped.snapshot.tables.iter().find(|t| t.name == name).unwrap_or_else(|| panic!("no table {name}"))
    }

    fn column<'a>(table: &'a Table, name: &str) -> &'a Column {
        table.columns.known().unwrap().iter().find(|c| c.name == name).unwrap_or_else(|| panic!("no column {name}"))
    }

    fn raw(type_name: &str, data_type: i32, size: Option<i64>, scale: Option<i64>) -> RawColumn {
        RawColumn {
            name: "c".into(),
            type_name: type_name.into(),
            data_type,
            size,
            scale,
            nullable: Some(1),
            default: None,
            auto_increment: Some("NO".into()),
            generated: Some("NO".into()),
            remarks: None,
            position: 1,
        }
    }

    type Expect = (Option<L>, Option<u32>, Option<u32>, Option<u32>, &'static str);

    fn mapped_type(dialect: Dialect, c: &RawColumn) -> Expect {
        let column = map_column(dialect, c);
        match &column.sql_type {
            Fact::Known(t) => (Some(t.logical), column.length, column.precision, column.scale, Box::leak(t.raw.clone().into_boxed_str())),
            Fact::Unknown(_) => (None, column.length, column.precision, column.scale, ""),
        }
    }

    #[test]
    fn postgres_type_rows() {
        let rows: Vec<(&str, i32, Option<i64>, Option<i64>, Expect)> = vec![
            ("int2", 5, Some(5), Some(0), (Some(L::Smallint), None, None, None, "int2")),
            ("smallserial", 5, Some(5), Some(0), (Some(L::Smallint), None, None, None, "smallserial")),
            ("int4", 4, Some(10), Some(0), (Some(L::Integer), None, None, None, "int4")),
            ("serial", 4, Some(10), Some(0), (Some(L::Integer), None, None, None, "serial")),
            ("int8", -5, Some(19), Some(0), (Some(L::Bigint), None, None, None, "int8")),
            ("bigserial", -5, Some(19), Some(0), (Some(L::Bigint), None, None, None, "bigserial")),
            ("bool", -7, Some(1), Some(0), (Some(L::Boolean), None, None, None, "bool")),
            ("float4", 7, Some(8), Some(8), (Some(L::Real), None, None, None, "float4")),
            ("float8", 8, Some(17), Some(17), (Some(L::Double), None, None, None, "float8")),
            ("numeric", 2, Some(12), Some(2), (Some(L::Decimal), None, Some(12), Some(2), "numeric(12,2)")),
            ("numeric", 2, Some(0), Some(0), (Some(L::Decimal), None, None, None, "numeric")),
            ("numeric", 2, Some(131089), Some(0), (Some(L::Decimal), None, None, None, "numeric")),
            ("bpchar", 1, Some(3), Some(0), (Some(L::Char), Some(3), None, None, "bpchar(3)")),
            ("varchar", 12, Some(255), Some(0), (Some(L::Varchar), Some(255), None, None, "varchar(255)")),
            ("varchar", 12, Some(2147483647), Some(0), (Some(L::Varchar), None, None, None, "varchar")),
            ("text", 12, Some(2147483647), Some(0), (Some(L::Text), None, None, None, "text")),
            ("date", 91, Some(13), Some(0), (Some(L::Date), None, None, None, "date")),
            ("time", 92, Some(15), Some(6), (Some(L::Time), None, None, None, "time")),
            ("timestamp", 93, Some(29), Some(6), (Some(L::Timestamp), None, None, None, "timestamp")),
            ("timestamptz", 93, Some(35), Some(6), (Some(L::TimestampTz), None, None, None, "timestamptz")),
            ("uuid", 1111, Some(2147483647), Some(0), (Some(L::Uuid), None, None, None, "uuid")),
            ("bytea", -2, Some(2147483647), Some(0), (Some(L::Binary), None, None, None, "bytea")),
            ("INT4", 4, Some(10), Some(0), (Some(L::Integer), None, None, None, "INT4")),
        ];
        for (name, dt, size, scale, expected) in rows {
            assert_eq!(mapped_type(Dialect::PostgreSql, &raw(name, dt, size, scale)), expected, "postgres {name} {size:?}");
        }
    }

    #[test]
    fn postgres_unknown_types_keep_the_raw_name_in_the_reason() {
        for (name, dt) in [("timetz", 92), ("json", 1111), ("jsonb", 1111), ("xml", 2009), ("inet", 1111), ("cidr", 1111), ("_int4", 2003), ("interval", 1111), ("money", 8), ("hstore", 1111), ("geometry", 1111), ("mood", 12), ("int4range", 1111)] {
            let column = map_column(Dialect::PostgreSql, &raw(name, dt, Some(10), Some(0)));
            assert_eq!(column.sql_type, Fact::Unknown(format!("unmapped SQL type '{name}' (java.sql.Types {dt})")));
            assert_eq!((column.length, column.precision, column.scale), (None, None, None));
        }
    }

    #[test]
    fn mysql_type_rows() {
        let rows: Vec<(&str, i32, Option<i64>, Option<i64>, Expect)> = vec![
            ("TINYINT", -6, Some(3), Some(0), (Some(L::Tinyint), None, None, None, "TINYINT")),
            ("TINYINT", -7, Some(1), Some(0), (Some(L::Boolean), None, None, None, "TINYINT")),
            ("TINYINT UNSIGNED", -6, Some(3), Some(0), (Some(L::Smallint), None, None, None, "TINYINT UNSIGNED")),
            ("SMALLINT", 5, Some(5), Some(0), (Some(L::Smallint), None, None, None, "SMALLINT")),
            ("SMALLINT UNSIGNED", 5, Some(5), Some(0), (Some(L::Integer), None, None, None, "SMALLINT UNSIGNED")),
            ("MEDIUMINT", 4, Some(7), Some(0), (Some(L::Integer), None, None, None, "MEDIUMINT")),
            ("MEDIUMINT UNSIGNED", 4, Some(8), Some(0), (Some(L::Integer), None, None, None, "MEDIUMINT UNSIGNED")),
            ("INT", 4, Some(10), Some(0), (Some(L::Integer), None, None, None, "INT")),
            ("INT UNSIGNED", -5, Some(10), Some(0), (Some(L::Bigint), None, None, None, "INT UNSIGNED")),
            ("BIGINT", -5, Some(19), Some(0), (Some(L::Bigint), None, None, None, "BIGINT")),
            ("BIGINT UNSIGNED", -5, Some(20), Some(0), (Some(L::Decimal), None, Some(20), Some(0), "BIGINT UNSIGNED")),
            ("BIT", -7, Some(1), Some(0), (Some(L::Boolean), None, None, None, "BIT")),
            ("FLOAT", 7, Some(12), Some(0), (Some(L::Real), None, None, None, "FLOAT")),
            ("DOUBLE", 8, Some(22), Some(0), (Some(L::Double), None, None, None, "DOUBLE")),
            ("DECIMAL", 3, Some(10), Some(2), (Some(L::Decimal), None, Some(10), Some(2), "DECIMAL(10,2)")),
            ("DECIMAL UNSIGNED", 3, Some(12), Some(4), (Some(L::Decimal), None, Some(12), Some(4), "DECIMAL UNSIGNED(12,4)")),
            ("CHAR", 1, Some(2), Some(0), (Some(L::Char), Some(2), None, None, "CHAR(2)")),
            ("VARCHAR", 12, Some(255), Some(0), (Some(L::Varchar), Some(255), None, None, "VARCHAR(255)")),
            ("TINYTEXT", -1, Some(255), Some(0), (Some(L::Text), None, None, None, "TINYTEXT")),
            ("TEXT", -1, Some(65535), Some(0), (Some(L::Text), None, None, None, "TEXT")),
            ("MEDIUMTEXT", -1, Some(16777215), Some(0), (Some(L::Text), None, None, None, "MEDIUMTEXT")),
            ("LONGTEXT", -1, Some(2147483647), Some(0), (Some(L::Text), None, None, None, "LONGTEXT")),
            ("DATE", 91, Some(10), Some(0), (Some(L::Date), None, None, None, "DATE")),
            ("TIME", 92, Some(8), Some(0), (Some(L::Time), None, None, None, "TIME")),
            ("DATETIME", 93, Some(19), Some(0), (Some(L::Timestamp), None, None, None, "DATETIME")),
            ("TIMESTAMP", 93, Some(19), Some(0), (Some(L::Timestamp), None, None, None, "TIMESTAMP")),
            ("BINARY", -2, Some(16), Some(0), (Some(L::Binary), None, None, None, "BINARY")),
            ("VARBINARY", -3, Some(64), Some(0), (Some(L::Binary), None, None, None, "VARBINARY")),
            ("TINYBLOB", -3, Some(255), Some(0), (Some(L::Binary), None, None, None, "TINYBLOB")),
            ("BLOB", -4, Some(65535), Some(0), (Some(L::Binary), None, None, None, "BLOB")),
            ("MEDIUMBLOB", -4, Some(16777215), Some(0), (Some(L::Binary), None, None, None, "MEDIUMBLOB")),
            ("LONGBLOB", -4, Some(2147483647), Some(0), (Some(L::Binary), None, None, None, "LONGBLOB")),
            ("varchar", 12, Some(10), Some(0), (Some(L::Varchar), Some(10), None, None, "varchar(10)")),
        ];
        for (name, dt, size, scale, expected) in rows {
            assert_eq!(mapped_type(Dialect::MySql, &raw(name, dt, size, scale)), expected, "mysql {name}");
        }
    }

    #[test]
    fn mysql_unknown_types_keep_the_raw_name_in_the_reason() {
        for (name, dt, size) in [("ENUM", 1, 6), ("SET", 1, 12), ("JSON", -1, 1073741824), ("YEAR", 91, 4), ("GEOMETRY", -2, 65535), ("POINT", -2, 65535), ("BIT", -7, 8)] {
            let column = map_column(Dialect::MySql, &raw(name, dt, Some(size), Some(0)));
            assert_eq!(column.sql_type, Fact::Unknown(format!("unmapped SQL type '{name}' (java.sql.Types {dt})")), "{name}");
        }
    }

    #[test]
    fn nullability_default_and_generated_facts() {
        let mut c = raw("int4", 4, Some(10), Some(0));
        c.nullable = Some(0);
        assert_eq!(map_column(Dialect::PostgreSql, &c).nullable, Fact::Known(false));
        c.nullable = Some(1);
        assert_eq!(map_column(Dialect::PostgreSql, &c).nullable, Fact::Known(true));
        c.nullable = Some(2);
        assert!(matches!(map_column(Dialect::PostgreSql, &c).nullable, Fact::Unknown(_)));
        c.nullable = None;
        assert!(matches!(map_column(Dialect::PostgreSql, &c).nullable, Fact::Unknown(_)));

        c.default = Some("nextval('s'::regclass)".into());
        assert_eq!(map_column(Dialect::PostgreSql, &c).default, Fact::Known(Some("nextval('s'::regclass)".into())));
        c.default = None;
        assert_eq!(map_column(Dialect::PostgreSql, &c).default, Fact::Known(None));

        let generated = |auto: Option<&str>, expr: Option<&str>| {
            let mut c = raw("int4", 4, Some(10), Some(0));
            c.auto_increment = auto.map(str::to_owned);
            c.generated = expr.map(str::to_owned);
            map_column(Dialect::MySql, &c).generated
        };
        assert_eq!(generated(Some("YES"), Some("NO")), Fact::Known(true));
        assert_eq!(generated(Some("NO"), Some("YES")), Fact::Known(true));
        assert_eq!(generated(Some("NO"), Some("NO")), Fact::Known(false));
        assert_eq!(generated(Some("NO"), Some("")), Fact::Known(false));
        assert!(matches!(generated(Some(""), Some("")), Fact::Unknown(_)));
        assert!(matches!(generated(None, None), Fact::Unknown(_)));
    }

    #[test]
    fn postgres_golden_snapshot() {
        let mapped = map(Dialect::PostgreSql, "pg_shop.describe.json", &MapOptions { listing_complete: true, qualify_schema: false });
        check_golden("pg_shop.snapshot.json", &mapped.snapshot);
        assert_eq!(mapped.views, vec!["mv_sales", "v_customer"]);
        assert!(mapped.snapshot.complete);
        mapped.snapshot.validate().unwrap();
    }

    #[test]
    fn mysql_golden_snapshot() {
        let mapped = map(Dialect::MySql, "mysql_shop.describe.json", &MapOptions { listing_complete: true, qualify_schema: false });
        check_golden("mysql_shop.snapshot.json", &mapped.snapshot);
        assert_eq!(mapped.views, vec!["v_account"]);
        mapped.snapshot.validate().unwrap();
    }

    #[test]
    fn postgres_constraints_and_unknown_facets() {
        let mapped = map(Dialect::PostgreSql, "pg_shop.describe.json", &MapOptions::default());
        assert!(!mapped.snapshot.complete, "a listing that was not proven complete must not claim completeness");

        let customer = table(&mapped, "customer");
        assert_eq!(customer.primary_key, Fact::Known(vec!["id".to_owned()]));
        let uniques = customer.unique_constraints.known().unwrap();
        assert_eq!(uniques, &vec![UniqueConstraint { name: Some("uq_customer_email".into()), columns: vec!["email".into()] }]);
        let indexes = customer.indexes.known().unwrap();
        assert_eq!(indexes, &vec![Index { name: Some("idx_customer_created".into()), columns: vec!["created_at".into(), "country".into()], unique: false }]);

        let orders = table(&mapped, "orders");
        assert_eq!(orders.unique_constraints, Fact::unknown("expression or partial unique index"));
        assert_eq!(orders.indexes.known().unwrap().len(), 1, "postgres keeps a declared index on the fk column");
        assert_eq!(orders.foreign_keys.known().unwrap()[0].ref_columns, Fact::Known(vec!["id".to_owned()]));

        let product = table(&mapped, "product");
        assert_eq!(product.indexes, Fact::unknown("expression or partial index"));
        assert_eq!(product.unique_constraints.known().unwrap().len(), 1);

        let restricted = table(&mapped, "restricted");
        assert!(matches!(&restricted.foreign_keys, Fact::Unknown(r) if r.contains("permission denied for table pg_constraint")));
        assert!(matches!(&restricted.indexes, Fact::Unknown(r) if r.contains("pg_index")));
        assert!(matches!(&restricted.unique_constraints, Fact::Unknown(r) if r.contains("pg_index")));
        assert!(restricted.columns.known().is_some());

        let ghost = table(&mapped, "ghost");
        assert_eq!(ghost.columns, Fact::unknown("table not found in database metadata"));
    }

    #[test]
    fn postgres_column_details() {
        let mapped = map(Dialect::PostgreSql, "pg_shop.describe.json", &MapOptions::default());
        let customer = table(&mapped, "customer");
        let id = column(customer, "id");
        assert_eq!(id.generated, Fact::Known(true));
        assert_eq!(id.sql_type, Fact::Known(SqlType { logical: L::Integer, raw: "serial".into() }));
        assert_eq!(column(customer, "full_name").length, None);
        assert_eq!(column(customer, "email").length, Some(255));
        assert_eq!(column(customer, "loose_amount").precision, None);
        assert_eq!((column(customer, "balance").precision, column(customer, "balance").scale), (Some(12), Some(2)));
        assert!(matches!(column(customer, "payload").sql_type, Fact::Unknown(_)));
        assert!(matches!(column(customer, "legacy_flag").nullable, Fact::Unknown(_)));
        assert_eq!(column(customer, "total_cents").generated, Fact::Known(true));
        assert!(matches!(column(customer, "no_report").generated, Fact::Unknown(_)));
        assert_eq!(column(customer, "active").default, Fact::Known(Some("true".into())));
        let order: Vec<&str> = customer.columns.known().unwrap().iter().map(|c| c.name.as_str()).collect();
        assert_eq!(order[..3], ["id", "email", "full_name"], "declaration order is kept");
    }

    #[test]
    fn mysql_filters_primary_and_foreign_key_backing_indexes() {
        let mapped = map(Dialect::MySql, "mysql_shop.describe.json", &MapOptions::default());
        let invoice = table(&mapped, "invoice");
        assert_eq!(invoice.unique_constraints, Fact::Known(vec![]), "PRIMARY is not a unique constraint");
        assert_eq!(invoice.indexes, Fact::Known(vec![Index { name: Some("idx_invoice_issued".into()), columns: vec!["issued".into()], unique: false }]));
        assert_eq!(invoice.foreign_keys.known().unwrap()[0].ref_table, "account");

        let account = table(&mapped, "account");
        assert_eq!(account.unique_constraints.known().unwrap().len(), 1);
        assert_eq!(account.indexes.known().unwrap().len(), 1);

        let log = table(&mapped, "legacy_log");
        assert_eq!(log.primary_key, Fact::Known(vec![]), "no primary key is a known fact");
        assert!(matches!(column(log, "n").nullable, Fact::Unknown(_)));

        let note = table(&mapped, "note");
        assert_eq!(note.indexes, Fact::unknown("expression or partial index"));
    }

    #[test]
    fn mysql_column_details() {
        let mapped = map(Dialect::MySql, "mysql_shop.describe.json", &MapOptions::default());
        let account = table(&mapped, "account");
        let id = column(account, "id");
        assert_eq!(id.sql_type, Fact::Known(SqlType { logical: L::Decimal, raw: "BIGINT UNSIGNED".into() }));
        assert_eq!((id.precision, id.scale), (Some(20), Some(0)));
        assert_eq!(id.generated, Fact::Known(true));
        assert_eq!(column(account, "flag").sql_type, Fact::Known(SqlType { logical: L::Boolean, raw: "BIT".into() }));
        assert!(matches!(column(account, "bits").sql_type, Fact::Unknown(_)));
        assert_eq!(column(account, "tiny_bool").sql_type, Fact::Known(SqlType { logical: L::Boolean, raw: "TINYINT".into() }));
        assert_eq!(column(account, "name").default, Fact::Known(Some("anon".into())));
        for unknown in ["yr", "status", "perms", "doc", "geom", "bits"] {
            assert!(matches!(column(account, unknown).sql_type, Fact::Unknown(_)), "{unknown}");
        }
        assert_eq!(column(account, "stored_total").generated, Fact::Known(true));
    }

    #[test]
    fn schema_qualification_rules() {
        let qualified = map(Dialect::PostgreSql, "pg_shop.describe.json", &MapOptions { listing_complete: true, qualify_schema: true });
        assert!(qualified.snapshot.tables.iter().all(|t| t.name.starts_with("public.")));
        let line = table(&qualified, "public.order_line");
        let refs: Vec<&str> = line.foreign_keys.known().unwrap().iter().map(|f| f.ref_table.as_str()).collect();
        assert!(refs.contains(&"public.orders") && refs.contains(&"audit.event"));

        let bare = map(Dialect::PostgreSql, "pg_shop.describe.json", &MapOptions::default());
        let line = table(&bare, "order_line");
        let mut refs: Vec<&str> = line.foreign_keys.known().unwrap().iter().map(|f| f.ref_table.as_str()).collect();
        refs.sort();
        assert_eq!(refs, vec!["audit.event", "orders"], "same-schema references are bare, cross-schema ones keep the schema");
    }

    #[test]
    fn same_name_in_two_schemas_is_an_error_without_qualification() {
        let mut tables = load("pg_shop.describe.json");
        let mut other = tables.iter().find(|t| t.name == "customer").unwrap().clone();
        other.schema = Some("archive".into());
        tables.push(other);
        assert_eq!(to_snapshot(Dialect::PostgreSql, &tables, &MapOptions::default()), Err(MapError::DuplicateTable { name: "customer".into() }));
        assert!(to_snapshot(Dialect::PostgreSql, &tables, &MapOptions { qualify_schema: true, ..MapOptions::default() }).is_ok());
    }

    fn bare_table(name: &str, columns: usize) -> TableMeta {
        let raw_columns = (0..columns).map(|i| RawColumn { name: format!("c{i}"), position: i as i32, ..raw("int4", 4, Some(10), Some(0)) }).collect();
        TableMeta {
            name: name.into(),
            schema: Some("public".into()),
            found: true,
            kind: Some("TABLE".into()),
            remarks: None,
            columns: Some(raw_columns),
            primary_key: Some(RawPrimaryKey { name: None, columns: vec![] }),
            foreign_keys: Some(vec![]),
            indexes: Some(vec![]),
            facet_errors: BTreeMap::new(),
        }
    }

    #[test]
    fn five_hundred_and_one_tables_fail_validate_unbounded_and_map_to_an_incomplete_snapshot() {
        let tables: Vec<TableMeta> = (0..501).map(|i| bare_table(&format!("t{i:03}"), 1)).collect();

        let unbounded = SchemaSnapshot { tables: tables.iter().map(|t| map_table(Dialect::PostgreSql, t, &MapOptions::default())).collect(), ..SchemaSnapshot::default() };
        assert_eq!(unbounded.validate(), Err(SchemaError::TooManyTables { count: 501 }));

        let mapped = to_snapshot(Dialect::PostgreSql, &tables, &MapOptions { listing_complete: true, qualify_schema: false }).unwrap();
        assert_eq!(mapped.snapshot.tables.len(), MAX_TABLES);
        assert_eq!(mapped.omitted_tables, 1);
        assert!(!mapped.snapshot.complete, "a bounded snapshot never claims completeness");
        assert_eq!(mapped.snapshot.validate(), Ok(()));
        assert_eq!(mapped.snapshot.tables[0].name, "t000", "the cut is deterministic by name");
    }

    #[test]
    fn exactly_five_hundred_tables_keep_the_callers_completeness() {
        let tables: Vec<TableMeta> = (0..500).map(|i| bare_table(&format!("t{i:03}"), 1)).collect();
        let mapped = to_snapshot(Dialect::PostgreSql, &tables, &MapOptions { listing_complete: true, qualify_schema: false }).unwrap();
        assert!(mapped.snapshot.complete && mapped.omitted_tables == 0);
        mapped.snapshot.validate().unwrap();
    }

    #[test]
    fn a_table_over_the_column_limit_gets_unknown_columns_not_a_failure() {
        let tables = vec![bare_table("wide", 501), bare_table("ok", 500)];
        let mapped = to_snapshot(Dialect::MySql, &tables, &MapOptions::default()).unwrap();
        assert!(matches!(&table(&mapped, "wide").columns, Fact::Unknown(r) if r.contains("501")));
        assert!(table(&mapped, "ok").columns.known().is_some());
        mapped.snapshot.validate().unwrap();
    }

    #[test]
    fn unknown_primary_key_makes_unique_constraints_unknown() {
        let mut t = bare_table("t", 2);
        t.primary_key = None;
        t.facet_errors.insert("primaryKey".into(), "permission denied".into());
        let mapped = to_snapshot(Dialect::PostgreSql, &[t], &MapOptions::default()).unwrap();
        let t = table(&mapped, "t");
        assert!(matches!(&t.primary_key, Fact::Unknown(r) if r.contains("permission denied")));
        assert!(matches!(&t.unique_constraints, Fact::Unknown(r) if r.contains("primary key unknown")));
    }

    #[test]
    fn unknown_foreign_keys_make_mysql_indexes_unknown_but_not_postgres() {
        let mut t = bare_table("t", 2);
        t.foreign_keys = None;
        t.facet_errors.insert("foreignKeys".into(), "denied".into());
        let my = to_snapshot(Dialect::MySql, &[t.clone()], &MapOptions::default()).unwrap();
        assert!(matches!(&table(&my, "t").indexes, Fact::Unknown(r) if r.contains("foreign keys unknown")));
        let pg = to_snapshot(Dialect::PostgreSql, &[t], &MapOptions::default()).unwrap();
        assert_eq!(table(&pg, "t").indexes, Fact::Known(vec![]));
    }

    #[test]
    fn mapping_is_deterministic_and_order_independent() {
        let mut tables = load("pg_shop.describe.json");
        let a = to_snapshot(Dialect::PostgreSql, &tables, &MapOptions::default()).unwrap();
        tables.reverse();
        let b = to_snapshot(Dialect::PostgreSql, &tables, &MapOptions::default()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn mapped_snapshot_diffs_to_nothing_against_itself() {
        let mapped = map(Dialect::MySql, "mysql_shop.describe.json", &MapOptions { listing_complete: true, qualify_schema: false });
        let plan = crate::schema::diff(&mapped.snapshot, &mapped.snapshot).unwrap();
        assert!(plan.changes.is_empty());
    }
}
