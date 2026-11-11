//! Reviewed entity generation from an open `.sql` file (reverse engineering, DDL-text path; no
//! database access). The DDL is read into a `SchemaSnapshot` by `persistence_core::ddl_parse`;
//! every table is either generated completely or refused with its reason. Only new files are
//! created (`overwrite: false`), all in one `FileAction`.
//!
//! Options (all prompts, nothing persisted): column selection per table (the key is always
//! mapped), `@Table(indexes/uniqueConstraints)`, `@Table(schema)`, sibling enums for enumerated
//! columns, table/column prefix stripping, singularized class names, Javadoc from database comments.
//! Enum constants are never renamed: `@Enumerated(EnumType.STRING)` persists `Enum.name()`, so a
//! label that is not already a valid, unique constant name refuses the column. Reserved words are
//! written as delimited identifiers (`@Column(name = "\"order\"")`, Jakarta Persistence 3.2 section
//! 2.15 "Naming of Database Objects").
use super::*;
use crate::model::{camel_case, decapitalize};
use persistence_core::ddl_parse::{self, DdlError, Dialect as SqlDialect, ParsedDdl};
use persistence_core::schema::{Column, Fact, ForeignKey, Index, LogicalType, Table, UniqueConstraint};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const TITLE: &str = "Generate entities from SQL file…";
const MAX_FILES: usize = 20;
const MAX_SQL: usize = 512 << 10;
const MAX_SOURCES: u32 = 5000;
const MAX_SKIPPED_SHOWN: usize = 30;
const MAX_PACKAGES: usize = 40;
const MAX_ENUM_VALUES: usize = 100;
const MAX_COMMENT_CHARS: usize = 240;
const MAX_PREFIX: usize = 32;
const OTHER_PACKAGE: &str = "\u{0}other";
const ALL_COLUMNS: &str = "\u{0}all";
const TAKEN_NAMES: &[&str] = &[
    "Entity", "Table", "Id", "Column", "GeneratedValue", "GenerationType", "ManyToOne", "FetchType", "JoinColumn", "BigDecimal",
    "LocalDate", "LocalTime", "LocalDateTime", "OffsetDateTime", "UUID", "String", "Object", "Long", "Integer", "Short", "Byte",
    "Boolean", "Float", "Double", "Int", "ByteArray", "Any", "Unit", "Number", "Index", "UniqueConstraint", "Enumerated", "EnumType",
    "Immutable", "Inheritance", "InheritanceType", "PrimaryKeyJoinColumn", "OneToOne", "OneToMany", "ManyToMany", "JoinTable", "MapsId", "Set",
    "LinkedHashSet", "MutableSet",
];
const VALIDATION_NAMES: [&str; 2] = ["NotNull", "Size"];
const MAX_AFFIXES: usize = 16;
const UNUSABLE_CONSTANTS: [&str; 19] = [
    "name", "ordinal", "values", "valueOf", "entries", "declaringClass", "getDeclaringClass", "compareTo", "hashCode", "toString",
    "equals", "clone", "finalize", "getClass", "notify", "notifyAll", "wait", "describeConstable", "Companion",
];
const SINGULAR_RULES: [(&str, &str); 6] = [("sses", "ss"), ("ies", "y"), ("ches", "ch"), ("shes", "sh"), ("xes", "x"), ("ses", "s")];

/// Actions for a `.sql` document; `None` for every other document so the Java/Kotlin path runs.
pub(super) fn actions(generation: &mut Generation, doc: &Document, selection: Range) -> Option<Vec<FileAction>> {
    if !doc.path.to_ascii_lowercase().ends_with(".sql") {
        return None;
    }
    let eligible = doc.text.len() <= MAX_SQL && doc.text.to_ascii_lowercase().contains("create table");
    let root = host::project_of(&doc.path).and_then(|project| project.root().ok()).and_then(|root| path(&root));
    let (true, Some(root)) = (eligible, root) else { return Some(Vec::new()) };
    if path(&doc.path).as_deref() != Some(doc.path.as_str()) || !inside(&root, &doc.path) {
        return Some(Vec::new());
    }
    if generation.pending.len() > 8 {
        generation.pending.clear();
    }
    generation.next = generation.next.checked_add(1)?;
    let data = format!("jpa:{}:{}:{}", Operation::DdlEntities.key(), generation.next, doc.version);
    generation.pending.insert(data.clone(), Pending { root, doc: doc.clone(), operation: Operation::DdlEntities, selection });
    Some(vec![FileAction { title: TITLE.into(), kind: Some("refactor.rewrite".into()), data: Some(data), ..FileAction::default() }])
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum IdGeneration {
    #[default]
    Identity,
    Sequence,
    Auto,
    Uuid,
    Unassigned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Link {
    ManyToOne,
    OneToOne,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Deferred {
    Skip,
    Definition(String),
}

#[derive(Clone, Default)]
struct Options {
    id_generation: IdGeneration,
    /// `sqlImportFetch = default`: generated `@ManyToOne` keeps the JPA default instead of an explicit LAZY.
    default_fetch: bool,
    /// `sqlImportOptional = never`: no `nullable = false` on generated `@JoinColumn`.
    never_required: bool,
    offer_deferred: bool,
    /// Indexes of tables that are really views (appended after the real tables).
    views: BTreeSet<usize>,
    view_block: Option<String>,
    /// Child table index to parent table index (shared primary key, JOINED inheritance).
    parents: HashMap<usize, usize>,
    /// Per table index and lower-cased column name: the decision for a column of an unmapped SQL type.
    deferred: HashMap<(usize, String), Deferred>,
    schema: bool,
    indexes: bool,
    comments: bool,
    comment_annotation: bool,
    enums: bool,
    singular: bool,
    table_prefixes: Vec<String>,
    column_prefixes: Vec<String>,
    table_suffixes: Vec<String>,
    column_suffixes: Vec<String>,
    /// Package of the Bean Validation constraints (`sqlImportValidation` on and Bean Validation proven).
    validation: Option<&'static str>,
    kotlin: bool,
    /// Child table index to parent table index: a key that is also a foreign key mapped with `@MapsId`.
    maps_id: HashMap<usize, usize>,
    /// Class names the user edited, by table index.
    classes: HashMap<usize, String>,
    /// Lower-cased selected column names by table index; a table that is absent maps every column.
    columns: HashMap<usize, BTreeSet<String>>,
}

#[derive(Debug)]
struct Attr {
    name: String,
    column: String,
    quoted: bool,
    java: String,
    kotlin: String,
    import: Option<&'static str>,
    key: bool,
    identity: bool,
    not_null: bool,
    length: Option<u32>,
    precision: Option<u32>,
    scale: Option<u32>,
    unique: bool,
    target: Option<String>,
    target_table: Option<usize>,
    link: Link,
    size: Option<u32>,
    enum_type: Option<String>,
    comment: Option<String>,
    comment_annotation: Option<String>,
    definition: Option<String>,
}

#[derive(Debug)]
enum Member {
    OneToMany { name: String, target: String, mapped_by: String },
    OneToOne { name: String, target: String, mapped_by: String },
    ManyToManyOwner { name: String, target: String, table: String, schema: Option<String>, join: String, inverse: String },
    ManyToManyMapped { name: String, target: String, mapped_by: String },
}

impl Member {
    fn name(&self) -> &str {
        match self {
            Member::OneToMany { name, .. } | Member::OneToOne { name, .. } | Member::ManyToManyOwner { name, .. } | Member::ManyToManyMapped { name, .. } => name,
        }
    }
}

#[derive(Debug)]
struct MapsIdOut {
    name: String,
    target: String,
    column: String,
}

#[derive(Debug)]
struct TodoField {
    name: String,
    column: String,
    sql_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EnumPlan {
    name: String,
    values: Vec<String>,
}

#[derive(Debug)]
struct IndexOut {
    name: Option<String>,
    columns: Vec<String>,
    unique: bool,
}

#[derive(Debug)]
struct Plan {
    class: String,
    table: String,
    table_quoted: bool,
    schema: Option<String>,
    comment: Option<String>,
    comment_annotation: Option<String>,
    attrs: Vec<Attr>,
    members: Vec<Member>,
    maps_id: Option<MapsIdOut>,
    todo_fields: Vec<TodoField>,
    indexes: Vec<IndexOut>,
    uniques: Vec<IndexOut>,
    enums: Vec<EnumPlan>,
    notes: Vec<String>,
    view: bool,
    id_generation: IdGeneration,
    default_fetch: bool,
    never_required: bool,
    validation: Option<&'static str>,
    deferred: Vec<String>,
    /// Class name of the JOINED parent; the key attribute is then inherited, not mapped here.
    parent: Option<String>,
    join_column: Option<String>,
    open: bool,
    joined_root: bool,
}

fn java_type(logical: LogicalType) -> (&'static str, &'static str, Option<&'static str>) {
    use LogicalType::*;
    match logical {
        Tinyint => ("Byte", "Byte", None),
        Smallint => ("Short", "Short", None),
        Integer => ("Integer", "Int", None),
        Bigint => ("Long", "Long", None),
        Boolean => ("Boolean", "Boolean", None),
        Real => ("Float", "Float", None),
        Double => ("Double", "Double", None),
        Decimal => ("BigDecimal", "BigDecimal", Some("java.math.BigDecimal")),
        Char | Varchar | Text => ("String", "String", None),
        Date => ("LocalDate", "LocalDate", Some("java.time.LocalDate")),
        Time => ("LocalTime", "LocalTime", Some("java.time.LocalTime")),
        Timestamp => ("LocalDateTime", "LocalDateTime", Some("java.time.LocalDateTime")),
        TimestampTz => ("OffsetDateTime", "OffsetDateTime", Some("java.time.OffsetDateTime")),
        Uuid => ("UUID", "UUID", Some("java.util.UUID")),
        Binary => ("byte[]", "ByteArray", None),
    }
}

fn simple(name: &str, case_sensitive: bool) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.len() <= 63
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !(case_sensitive && name.chars().any(|c| c.is_ascii_uppercase()))
}

/// The database name as it is written inside a Java/Kotlin string: a reserved word becomes a
/// delimited identifier with escaped inner quotes.
fn literal(name: &str, quoted: bool) -> String {
    if quoted { format!("\\\"{name}\\\"") } else { name.to_owned() }
}

fn strip_prefix(name: &str, prefix: &str) -> String {
    let matches = !prefix.is_empty() && name.len() > prefix.len() && name.is_char_boundary(prefix.len()) && name[..prefix.len()].eq_ignore_ascii_case(prefix);
    if matches { name[prefix.len()..].trim_start_matches('_').to_owned() } else { name.to_owned() }
}

/// Documented rules, applied to the last `_` segment only: sses->ss, ies->y, ches->ch, shes->sh,
/// xes->x, ses->s, otherwise one trailing `s` is dropped unless the word ends in ss, us or is.
/// Irregular plurals are not guessed (the class name is editable in a prompt).
fn singularize(name: &str) -> String {
    let (head, last) = name.rsplit_once('_').map_or(("", name), |(head, last)| (head, last));
    let lower = last.to_ascii_lowercase();
    let mut singular = last.to_owned();
    if let Some((suffix, with)) = SINGULAR_RULES.iter().find(|(suffix, _)| lower.len() > suffix.len() && lower.ends_with(suffix)) {
        singular = format!("{}{with}", &last[..last.len() - suffix.len()]);
    } else if lower.len() > 1 && lower.ends_with('s') && !["ss", "us", "is"].iter().any(|ending| lower.ends_with(ending)) {
        singular = last[..last.len() - 1].to_owned();
    }
    if head.is_empty() { singular } else { format!("{head}_{singular}") }
}

fn strip_suffix(name: &str, suffix: &str) -> String {
    let matches = !suffix.is_empty() && name.len() > suffix.len() && name.is_char_boundary(name.len() - suffix.len()) && name[name.len() - suffix.len()..].eq_ignore_ascii_case(suffix);
    if matches { name[..name.len() - suffix.len()].trim_end_matches('_').to_owned() } else { name.to_owned() }
}

/// The first configured prefix that matches is removed, then the first configured suffix that matches.
fn strip_affixes(name: &str, prefixes: &[String], suffixes: &[String]) -> String {
    let stripped = prefixes.iter().map(|prefix| strip_prefix(name, prefix)).find(|stripped| stripped != name).unwrap_or_else(|| name.to_owned());
    suffixes.iter().map(|suffix| strip_suffix(&stripped, suffix)).find(|result| *result != stripped).unwrap_or(stripped)
}

/// Plural collection name for an entity class name: ies for a trailing consonant+y, es after s/x/z/ch/sh,
/// a name that already ends in s is kept, otherwise s is appended.
fn pluralize(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let vowel_before_y = lower.len() > 1 && lower[..lower.len() - 1].ends_with(['a', 'e', 'i', 'o', 'u']);
    if lower.ends_with('y') && !vowel_before_y {
        return format!("{}ies", &name[..name.len() - 1]);
    }
    if lower.ends_with("ss") || lower.ends_with('x') || lower.ends_with('z') || lower.ends_with("ch") || lower.ends_with("sh") {
        return format!("{name}es");
    }
    if lower.ends_with('s') {
        return name.to_owned();
    }
    format!("{name}s")
}

/// A comment as one string literal body (Java or Kotlin), or `None` when empty or over the bound.
fn string_text(text: &str, kotlin: bool) -> Option<String> {
    let cleaned: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() || collapsed.chars().count() > MAX_COMMENT_CHARS {
        return None;
    }
    let escaped = collapsed.replace('\\', "\\\\").replace('"', "\\\"");
    Some(if kotlin { escaped.replace('$', "\\$") } else { escaped })
}

fn default_class(table: &Table, options: &Options) -> String {
    let name = table.name.rsplit('.').next().unwrap_or(&table.name);
    let name = strip_affixes(name, &options.table_prefixes, &options.table_suffixes);
    let name = if options.singular { singularize(&name) } else { name };
    camel_case(&name, true)
}

/// A comment as one Javadoc/KDoc line: controls dropped, whitespace collapsed, and every sequence
/// that could end or open a comment, start a Java unicode escape or an HTML tag neutralised.
fn doc_text(text: &str) -> Option<String> {
    let cleaned: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() || collapsed.chars().count() > MAX_COMMENT_CHARS {
        return None;
    }
    Some(
        collapsed
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('\\', "&#92;")
            .replace("/*", "/&#42;")
            .replace("*/", "*&#47;")
            .replace('@', "&#64;"),
    )
}

fn enum_values_ok(values: &[String]) -> Result<(), String> {
    if values.is_empty() || values.len() > MAX_ENUM_VALUES {
        return Err(format!("an enumeration with {} values is not generated (1 to {MAX_ENUM_VALUES})", values.len()));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        if !identifier(value) || UNUSABLE_CONSTANTS.contains(&value.as_str()) {
            return Err(format!("enumeration value '{value}' is not a usable constant name; STRING mapping stores the constant name, so values are never renamed"));
        }
        if !seen.insert(value.as_str()) {
            return Err(format!("enumeration value '{value}' is listed twice"));
        }
    }
    Ok(())
}

fn taken(name: &str, options: &Options) -> bool {
    TAKEN_NAMES.contains(&name) || options.validation.is_some() && VALIDATION_NAMES.contains(&name)
}

struct Gate<'a> {
    columns: &'a [Column],
    key: &'a str,
    foreign_keys: &'a [ForeignKey],
    uniques: &'a [UniqueConstraint],
    schema: Option<String>,
    name: String,
    class: String,
}

fn gate<'a>(parsed: &'a ParsedDdl, ti: usize, dialect: SqlDialect, options: &Options) -> Result<Gate<'a>, String> {
    let table: &Table = &parsed.snapshot.tables[ti];
    let refuse = |reason: String| Err(format!("{}: {reason}", table.name));
    let case_sensitive = dialect == SqlDialect::PostgreSql;
    let columns = match &table.columns {
        Fact::Known(columns) if !columns.is_empty() => columns,
        Fact::Known(_) => return refuse("no columns".into()),
        Fact::Unknown(reason) => return refuse(format!("columns are not fully known ({reason})")),
    };
    let key = match &table.primary_key {
        Fact::Known(key) if key.len() == 1 => &key[0],
        Fact::Known(key) if key.is_empty() => return refuse("no primary key; an entity needs an @Id".into()),
        Fact::Known(key) => return refuse(format!("composite primary key ({}) is not generated", key.join(", "))),
        Fact::Unknown(reason) => return refuse(format!("primary key is not known ({reason})")),
    };
    let foreign_keys = match &table.foreign_keys {
        Fact::Known(fks) => fks,
        Fact::Unknown(reason) => return refuse(format!("foreign keys are not known ({reason})")),
    };
    let uniques = match &table.unique_constraints {
        Fact::Known(uniques) => uniques,
        Fact::Unknown(reason) => return refuse(format!("unique constraints are not known ({reason})")),
    };
    let (schema, name) = match table.name.split_once('.') {
        Some((schema, name)) => (Some(schema.to_owned()), name.to_owned()),
        None => (None, table.name.clone()),
    };
    if !simple(&name, case_sensitive) || schema.as_deref().is_some_and(|s| !simple(s, case_sensitive)) {
        return refuse("the table name needs identifier quoting, which is not generated".into());
    }
    if schema.as_deref().is_some_and(|s| options.schema && ddl_parse::is_reserved(dialect, s)) {
        return refuse("the schema name is a reserved word, which is not generated as a delimited schema".into());
    }
    let class = options.classes.get(&ti).cloned().unwrap_or_else(|| default_class(table, options));
    if !identifier(&class) || taken(&class, options) {
        return refuse(format!("class name `{class}` is not usable (keyword or generated import name)"));
    }
    if !columns.iter().any(|c| c.name.eq_ignore_ascii_case(key)) {
        return refuse(format!("primary key column {key} is not a column"));
    }
    Ok(Gate { columns, key, foreign_keys, uniques, schema, name, class })
}

struct Built {
    attr: Attr,
    enum_plan: Option<EnumPlan>,
    notes: Vec<String>,
}

/// One column as an attribute, or the reason it cannot be generated. `linked` maps the indexes of
/// other generated tables to their class names.
fn column_attr(parsed: &ParsedDdl, ti: usize, dialect: SqlDialect, options: &Options, gate: &Gate, column: &Column, linked: &HashMap<usize, String>) -> Result<Built, String> {
    let table = &parsed.snapshot.tables[ti];
    let case_sensitive = dialect == SqlDialect::PostgreSql;
    if !simple(&column.name, case_sensitive) {
        return Err("the column name needs identifier quoting, which is not generated".into());
    }
    let quoted = ddl_parse::is_reserved(dialect, &column.name);
    let is_key = column.name.eq_ignore_ascii_case(gate.key);
    let enumeration = parsed.column_enums.iter().find(|e| e.table == table.name && e.column == column.name);
    let mut enum_plan = None;
    let mut definition = None;
    let (java, kotlin, import, logical) = match (&column.sql_type, enumeration) {
        (Fact::Unknown(reason), Some(found)) => {
            if !options.enums {
                return Err(format!("{reason} (an enumerated type with {} values; choose the enum option to generate it)", found.values.len()));
            }
            if is_key {
                return Err("an enumerated key is not generated".into());
            }
            enum_values_ok(&found.values)?;
            let name = match &found.type_name {
                Some(type_name) => camel_case(type_name, true),
                None => format!("{}{}", gate.class, camel_case(&column.name, true)),
            };
            if !identifier(&name) || taken(&name, options) {
                return Err(format!("enum name `{name}` is not usable (keyword or generated import name)"));
            }
            enum_plan = Some(EnumPlan { name: name.clone(), values: found.values.clone() });
            (name.clone(), name, None, None)
        }
        (Fact::Unknown(reason), None) => match options.deferred.get(&(ti, column.name.to_ascii_lowercase())) {
            Some(Deferred::Definition(text)) if !is_key => {
                definition = Some(text.clone());
                ("String".to_owned(), "String".to_owned(), None, Some(LogicalType::Text))
            }
            _ => return Err(reason.clone()),
        },
        (Fact::Known(ty), _) => {
            let (java, kotlin, import) = java_type(ty.logical);
            (java.to_owned(), kotlin.to_owned(), import, Some(ty.logical))
        }
    };
    let not_null = match &column.nullable {
        Fact::Known(value) => !*value,
        Fact::Unknown(reason) => return Err(format!("nullability is not known ({reason})")),
    };
    let generated = match &column.generated {
        Fact::Known(value) => *value,
        Fact::Unknown(reason) => return Err(format!("generated state is not known ({reason})")),
    };
    if generated && !is_key {
        return Err("a generated column that is not the key is not generated".into());
    }
    if generated && !matches!(logical, Some(LogicalType::Smallint | LogicalType::Integer | LogicalType::Bigint)) {
        return Err(format!("a generated key of SQL type {:?} is not mapped to an identity", logical.unwrap_or(LogicalType::Text)));
    }
    let unique = gate.uniques.iter().any(|u| u.columns.len() == 1 && u.columns[0].eq_ignore_ascii_case(&column.name));
    let text = matches!(logical, Some(LogicalType::Char | LogicalType::Varchar | LogicalType::Binary));
    let comment = (options.comments || options.comment_annotation)
        .then(|| parsed.comments.iter().find(|c| c.table == table.name && c.column.as_deref() == Some(column.name.as_str())))
        .flatten();
    let mut notes = Vec::new();
    let (comment, comment_annotation) = match comment {
        Some(found) => {
            let shown = doc_text(&found.text).filter(|_| options.comments);
            let annotated = string_text(&found.text, options.kotlin).filter(|_| options.comment_annotation);
            if doc_text(&found.text).is_none() {
                notes.push(format!("{}: database comment is empty or longer than {MAX_COMMENT_CHARS} characters and is not copied", column.name));
            }
            (shown, annotated)
        }
        None => (None, None),
    };
    let stem_source = strip_affixes(&column.name, &options.column_prefixes, &options.column_suffixes);
    let mut attr = Attr {
        name: camel_case(&stem_source, false),
        column: column.name.clone(),
        quoted,
        java: java.clone(),
        kotlin,
        import,
        key: is_key,
        identity: generated,
        not_null: not_null && !is_key,
        length: column.length.filter(|n| text && *n != 255 && *n > 0),
        precision: column.precision.filter(|_| logical == Some(LogicalType::Decimal)),
        scale: column.scale.filter(|s| logical == Some(LogicalType::Decimal) && *s != 0),
        unique: unique && !is_key,
        target: None,
        target_table: None,
        link: Link::ManyToOne,
        size: column.length.filter(|n| *n > 0 && matches!(logical, Some(LogicalType::Char | LogicalType::Varchar))),
        enum_type: enum_plan.as_ref().map(|e| e.name.clone()),
        comment,
        comment_annotation,
        definition,
    };
    let single = gate.foreign_keys.iter().find(|fk| fk.columns.len() == 1 && fk.columns[0].eq_ignore_ascii_case(&column.name));
    if let Some(fk) = single.filter(|_| !is_key && enum_plan.is_none()) {
        let target = ddl_parse::find_table(&parsed.snapshot, &fk.ref_table).filter(|t| linked.contains_key(t));
        let target_key = target.and_then(|t| parsed.snapshot.tables[t].primary_key.known());
        let ref_matches = match (&fk.ref_columns, target_key) {
            (Fact::Known(refs), Some(key)) => refs.len() == 1 && key.len() == 1 && refs[0].eq_ignore_ascii_case(&key[0]),
            _ => false,
        };
        match (target, ref_matches) {
            (Some(t), true) => {
                attr.target = Some(linked[&t].clone());
                attr.target_table = Some(t);
                attr.link = if attr.unique { Link::OneToOne } else { Link::ManyToOne };
                let stem = stem_source.trim_end_matches("_id").trim_end_matches("_ID");
                let relation = camel_case(stem, false);
                if !relation.is_empty() && relation != attr.name {
                    attr.name = relation;
                }
            }
            _ => notes.push(format!(
                "{}: foreign key to {} stays a plain {} column (target not selected, not generated, or not its primary key)",
                column.name, fk.ref_table, java
            )),
        }
    }
    if !identifier(&attr.name) {
        let suffixed = format!("{}Field", attr.name);
        if !identifier(&suffixed) {
            return Err(format!("attribute name `{}` is not a valid identifier", attr.name));
        }
        notes.push(format!("{}: `{}` is a reserved word, so the attribute is named `{suffixed}`", column.name, attr.name));
        attr.name = suffixed;
    }
    Ok(Built { attr, enum_plan, notes })
}

/// `linked` maps the indexes of other generated tables to their class names.
fn plan(parsed: &ParsedDdl, ti: usize, dialect: SqlDialect, linked: &HashMap<usize, String>, options: &Options) -> Result<Plan, String> {
    let table: &Table = &parsed.snapshot.tables[ti];
    if let (true, Some(reason)) = (options.views.contains(&ti), &options.view_block) {
        return Err(format!("{}: {reason}", table.name));
    }
    let gate = gate(parsed, ti, dialect, options)?;
    let selected = options.columns.get(&ti);
    let mut deferred_columns = Vec::new();
    let mut attrs: Vec<Attr> = Vec::new();
    let mut enums: Vec<EnumPlan> = Vec::new();
    let mut notes = Vec::new();
    let mut omitted = Vec::new();
    let mut todo_fields = Vec::new();
    for column in gate.columns {
        let is_key = column.name.eq_ignore_ascii_case(gate.key);
        if !is_key && selected.is_some_and(|set| !set.contains(&column.name.to_ascii_lowercase())) {
            omitted.push(column.name.clone());
            continue;
        }
        if !is_key && matches!(column.sql_type, Fact::Unknown(_)) && options.deferred.get(&(ti, column.name.to_ascii_lowercase())) == Some(&Deferred::Skip) {
            let sql_type = unmapped_text(parsed, &table.name, &column.name).unwrap_or("unmapped type");
            deferred_columns.push(format!("{} ({sql_type})", column.name));
            let name = camel_case(&strip_affixes(&column.name, &options.column_prefixes, &options.column_suffixes), false);
            let sql_type = if safe_definition(sql_type) { format!("the unmapped SQL type {sql_type}") } else { "an unmapped SQL type".to_owned() };
            if identifier(&name) && simple(&column.name, dialect == SqlDialect::PostgreSql) {
                todo_fields.push(TodoField { name, column: literal(&column.name, ddl_parse::is_reserved(dialect, &column.name)), sql_type });
            }
            continue;
        }
        let built = column_attr(parsed, ti, dialect, options, &gate, column, linked).map_err(|reason| format!("{}: column {}: {reason}", table.name, column.name))?;
        if attrs.iter().any(|a| a.name == built.attr.name) {
            return Err(format!("{}: column {}: attribute name `{}` is produced by two columns", table.name, column.name, built.attr.name));
        }
        notes.extend(built.notes);
        if let Some(found) = built.enum_plan {
            if !enums.contains(&found) {
                enums.push(found);
            }
        }
        attrs.push(built.attr);
    }
    if !omitted.is_empty() {
        notes.push(format!("columns left out by selection: {}", omitted.join(", ")));
    }
    for fk in gate.foreign_keys.iter().filter(|fk| fk.columns.len() > 1) {
        notes.push(format!("composite foreign key ({}) to {} stays plain columns", fk.columns.join(", "), fk.ref_table));
    }
    let composite_uniques: Vec<&UniqueConstraint> = gate.uniques.iter().filter(|u| u.columns.len() > 1).collect();
    let known_indexes: &[Index] = table.indexes.known().map_or(&[], Vec::as_slice);
    let (mut indexes, mut uniques) = (Vec::new(), Vec::new());
    if options.indexes {
        let mapped = |columns: &[String]| -> Result<Vec<String>, String> {
            columns
                .iter()
                .map(|wanted| match attrs.iter().find(|a| a.column.eq_ignore_ascii_case(wanted)) {
                    Some(attr) if !attr.quoted => Ok(attr.column.clone()),
                    Some(_) => Err(format!("column {wanted} is a delimited identifier")),
                    None => Err(format!("column {wanted} is not mapped")),
                })
                .collect()
        };
        let name_ok = |name: &Option<String>| name.as_ref().map_or(true, |n| simple(n, false));
        if let Fact::Unknown(reason) = &table.indexes {
            notes.push(format!("indexes are not mapped ({reason})"));
        }
        for index in known_indexes {
            let ordered = parsed.ordered_indexes.iter().any(|o| o.table == table.name && o.name == index.name && o.columns == index.columns);
            match mapped(&index.columns) {
                _ if ordered => notes.push(format!("index {} is not mapped (DESC or operator-class ordering is not expressed)", index.name.as_deref().unwrap_or("(unnamed)"))),
                _ if !name_ok(&index.name) => notes.push("an index with an unusable name is not mapped".into()),
                Ok(columns) => indexes.push(IndexOut { name: index.name.clone(), columns, unique: index.unique }),
                Err(reason) => notes.push(format!("index {} is not mapped ({reason})", index.name.as_deref().unwrap_or("(unnamed)"))),
            }
        }
        for unique in &composite_uniques {
            match mapped(&unique.columns) {
                _ if !name_ok(&unique.name) => notes.push("a unique constraint with an unusable name is not mapped".into()),
                Ok(columns) => uniques.push(IndexOut { name: unique.name.clone(), columns, unique: true }),
                Err(reason) => notes.push(format!("unique constraint ({}) is not mapped ({reason})", unique.columns.join(", "))),
            }
        }
    } else if composite_uniques.len() + known_indexes.len() > 0 {
        notes.push(format!("{} composite unique constraint(s) and {} index(es) are not mapped", composite_uniques.len(), known_indexes.len()));
    }
    let schema = match (&gate.schema, options.schema && dialect == SqlDialect::PostgreSql) {
        (Some(schema), true) => Some(schema.clone()),
        (Some(schema), false) if dialect == SqlDialect::PostgreSql => {
            notes.push(format!("schema `{schema}` is not written to @Table (schema option not chosen)"));
            None
        }
        (Some(schema), false) => {
            notes.push(format!("MySQL database qualifier `{schema}` is dropped (schema versus catalog mapping is provider specific)"));
            None
        }
        _ => None,
    };
    let table_comment = (options.comments || options.comment_annotation).then(|| parsed.comments.iter().find(|c| c.table == table.name && c.column.is_none())).flatten();
    let (comment, comment_annotation) = match table_comment {
        Some(found) => {
            if doc_text(&found.text).is_none() {
                notes.push(format!("table comment is empty or longer than {MAX_COMMENT_CHARS} characters and is not copied"));
            }
            (doc_text(&found.text).filter(|_| options.comments), string_text(&found.text, options.kotlin).filter(|_| options.comment_annotation))
        }
        None => (None, None),
    };
    let table_quoted = ddl_parse::is_reserved(dialect, &gate.name);
    if attrs.iter().any(|a| a.identity) {
        if options.id_generation == IdGeneration::Sequence {
            if dialect == SqlDialect::MySql {
                return Err(format!("{}: sequence id generation is not offered for MySQL (its generated keys are AUTO_INCREMENT columns)", table.name));
            }
            notes.push("sqlImportIdGeneration = sequence writes GenerationType.SEQUENCE without a named generator: the provider's default sequence name and allocation size apply, not the column's own sequence".into());
        }
    }
    let (mut parent, mut join_column) = (None, None);
    if let Some(parent_class) = options.parents.get(&ti).and_then(|p| linked.get(p)) {
        if let Some(key) = attrs.iter().position(|a| a.key) {
            let key = attrs.remove(key);
            join_column = Some(literal(&key.column, key.quoted));
            parent = Some(parent_class.clone());
        }
    }
    let maps_id = match options.maps_id.get(&ti).filter(|_| parent.is_none()) {
        Some(&parent_table) => maps_id_member(parsed, &gate, &attrs, linked, parent_table, &mut notes),
        None => None,
    };
    Ok(Plan {
        class: gate.class,
        table: gate.name,
        table_quoted,
        schema,
        comment,
        comment_annotation,
        members: Vec::new(),
        maps_id,
        todo_fields,
        attrs,
        indexes,
        uniques,
        enums,
        notes,
        view: options.views.contains(&ti),
        id_generation: options.id_generation,
        default_fetch: options.default_fetch,
        never_required: options.never_required,
        validation: options.validation,
        deferred: deferred_columns,
        parent,
        join_column,
        open: false,
        joined_root: false,
    })
}

/// The association of a shared primary key: `@OneToOne @MapsId` next to the plain `@Id`. Needs the parent to be
/// generated and its key to have the same SQL type, otherwise the key stays a plain `@Id`.
fn maps_id_member(parsed: &ParsedDdl, gate: &Gate, attrs: &[Attr], linked: &HashMap<usize, String>, parent: usize, notes: &mut Vec<String>) -> Option<MapsIdOut> {
    let target = linked.get(&parent)?;
    let key_attr = attrs.iter().find(|a| a.key)?;
    let parent_key = parsed.snapshot.tables[parent].primary_key.known().and_then(|key| key.first())?;
    let key_type = |columns: &[Column], name: &str| columns.iter().find(|c| c.name.eq_ignore_ascii_case(name)).and_then(|c| c.sql_type.known().map(|t| t.logical));
    let parent_columns = parsed.snapshot.tables[parent].columns.known()?;
    if key_type(gate.columns, gate.key) != key_type(parent_columns, parent_key) {
        notes.push(format!("the primary key {} stays a plain @Id: its SQL type differs from the key of {}, so @MapsId is not generated", key_attr.column, parsed.snapshot.tables[parent].name));
        return None;
    }
    let name = decapitalize(target);
    if !identifier(&name) || attrs.iter().any(|a| a.name == name) {
        notes.push(format!("the primary key {} stays a plain @Id: the association attribute `{name}` is not usable here, so @MapsId is not generated", key_attr.column));
        return None;
    }
    Some(MapsIdOut { name, target: target.clone(), column: literal(&key_attr.column, key_attr.quoted) })
}

fn unmapped_text<'a>(parsed: &'a ParsedDdl, table: &str, column: &str) -> Option<&'a str> {
    parsed.unmapped_columns.iter().find(|u| u.table == table && u.column == column).map(|u| u.sql_type.as_str())
}

/// Letters, digits, space, parentheses, comma and underscore only: the text is written into a Java/Kotlin string unescaped.
fn safe_definition(text: &str) -> bool {
    !text.is_empty() && text.len() <= 64 && text.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '(' | ')' | ',' | '_'))
}

fn render(plan: &Plan, package: &str, ns: &str, kotlin: bool) -> String {
    let semi = if kotlin { "" } else { ";" };
    let mut imports: BTreeSet<String> = BTreeSet::new();
    for name in ["Entity", "Table"] {
        imports.insert(format!("{ns}.{name}"));
    }
    let fetch_line = |imports: &mut BTreeSet<String>, kind: &str| {
        if plan.default_fetch {
            format!("@{kind}")
        } else {
            imports.insert(format!("{ns}.FetchType"));
            format!("@{kind}(fetch = FetchType.LAZY)")
        }
    };
    let property = |field: &str, kt: &str| {
        let private_set = if plan.view { "\n        private set" } else { "" };
        format!("var {field}: {kt}? = null{private_set}")
    };
    let mut body = String::new();
    let mut accessors = Vec::new();
    for attr in &plan.attrs {
        let mut lines = Vec::new();
        if let Some(comment) = &attr.comment {
            lines.push(format!("/** {comment} */"));
        }
        if let Some(text) = &attr.comment_annotation {
            lines.push(format!("@org.hibernate.annotations.Comment(\"{text}\")"));
        }
        if attr.key {
            imports.insert(format!("{ns}.Id"));
            lines.push("@Id".to_owned());
            let strategy = match plan.id_generation {
                _ if attr.identity && plan.id_generation == IdGeneration::Unassigned => None,
                IdGeneration::Sequence if attr.identity => Some("SEQUENCE"),
                IdGeneration::Auto if attr.identity => Some("AUTO"),
                IdGeneration::Uuid if !attr.identity && attr.java == "UUID" => Some("UUID"),
                _ if attr.identity => Some("IDENTITY"),
                _ => None,
            };
            if let Some(strategy) = strategy {
                imports.insert(format!("{ns}.GeneratedValue"));
                imports.insert(format!("{ns}.GenerationType"));
                lines.push(format!("@GeneratedValue(strategy = GenerationType.{strategy})"));
            }
        }
        let mut args = Vec::new();
        let column = literal(&attr.column, attr.quoted);
        if let Some(target) = &attr.target {
            let kind = if attr.link == Link::OneToOne { "OneToOne" } else { "ManyToOne" };
            for name in [kind, "JoinColumn"] {
                imports.insert(format!("{ns}.{name}"));
            }
            lines.push(fetch_line(&mut imports, kind));
            args.push(format!("name = \"{column}\""));
            if attr.not_null && !plan.never_required {
                args.push("nullable = false".into());
            }
            lines.push(format!("@JoinColumn({})", args.join(", ")));
            let (field, ty) = (&attr.name, target);
            lines.push(if kotlin { property(field, ty) } else { format!("private {ty} {field};") });
            accessors.push((field.clone(), ty.clone(), ty.clone()));
        } else {
            if attr.enum_type.is_some() {
                imports.insert(format!("{ns}.Enumerated"));
                imports.insert(format!("{ns}.EnumType"));
                lines.push("@Enumerated(EnumType.STRING)".into());
            }
            if let Some(package) = plan.validation {
                if attr.not_null {
                    imports.insert(format!("{package}.NotNull"));
                    lines.push("@NotNull".into());
                }
                if let Some(size) = attr.size {
                    imports.insert(format!("{package}.Size"));
                    lines.push(format!("@Size(max = {size})"));
                }
            }
            if attr.quoted || attr.column != crate::file_actions::naming_column_name(&attr.name) {
                args.push(format!("name = \"{column}\""));
            }
            if attr.not_null {
                args.push("nullable = false".into());
            }
            if attr.unique {
                args.push("unique = true".into());
            }
            if let Some(length) = attr.length {
                args.push(format!("length = {length}"));
            }
            if let Some(precision) = attr.precision {
                args.push(format!("precision = {precision}"));
            }
            if let Some(scale) = attr.scale {
                args.push(format!("scale = {scale}"));
            }
            if let Some(definition) = &attr.definition {
                args.push(format!("columnDefinition = \"{definition}\""));
            }
            if !args.is_empty() {
                imports.insert(format!("{ns}.Column"));
                lines.push(format!("@Column({})", args.join(", ")));
            }
            if let Some(import) = attr.import {
                imports.insert(import.to_owned());
            }
            let (field, java, kt) = (&attr.name, &attr.java, &attr.kotlin);
            lines.push(if kotlin { property(field, kt) } else { format!("private {java} {field};") });
            accessors.push((field.clone(), java.clone(), kt.clone()));
        }
        body.push_str(&lines.iter().map(|l| format!("    {l}\n")).collect::<String>());
        body.push('\n');
    }
    if let Some(maps_id) = &plan.maps_id {
        for name in ["OneToOne", "MapsId", "JoinColumn"] {
            imports.insert(format!("{ns}.{name}"));
        }
        let optional = if plan.default_fetch { "@OneToOne(optional = false)".to_owned() } else {
            imports.insert(format!("{ns}.FetchType"));
            "@OneToOne(fetch = FetchType.LAZY, optional = false)".to_owned()
        };
        let (field, ty) = (&maps_id.name, &maps_id.target);
        let declaration = if kotlin { property(field, ty) } else { format!("private {ty} {field};") };
        body.push_str(&format!("    {optional}\n    @MapsId\n    @JoinColumn(name = \"{}\", nullable = false)\n    {declaration}\n\n", maps_id.column));
        accessors.push((field.clone(), ty.clone(), ty.clone()));
    }
    for member in &plan.members {
        let mut lines = Vec::new();
        let (name, target, collection) = match member {
            Member::OneToMany { name, target, mapped_by } => {
                imports.insert(format!("{ns}.OneToMany"));
                lines.push(format!("@OneToMany(mappedBy = \"{mapped_by}\")"));
                (name, target, true)
            }
            Member::ManyToManyMapped { name, target, mapped_by } => {
                imports.insert(format!("{ns}.ManyToMany"));
                lines.push(format!("@ManyToMany(mappedBy = \"{mapped_by}\")"));
                (name, target, true)
            }
            Member::ManyToManyOwner { name, target, table, schema, join, inverse } => {
                for kind in ["ManyToMany", "JoinTable", "JoinColumn"] {
                    imports.insert(format!("{ns}.{kind}"));
                }
                let at = if kotlin { "" } else { "@" };
                let list = |item: String| if kotlin { format!("[{item}]") } else { format!("{{{item}}}") };
                let schema = schema.as_ref().map_or(String::new(), |schema| format!(", schema = \"{schema}\""));
                lines.push("@ManyToMany".to_owned());
                lines.push(format!(
                    "@JoinTable(name = \"{table}\"{schema}, joinColumns = {}, inverseJoinColumns = {})",
                    list(format!("{at}JoinColumn(name = \"{join}\")")),
                    list(format!("{at}JoinColumn(name = \"{inverse}\")"))
                ));
                (name, target, true)
            }
            Member::OneToOne { name, target, mapped_by } => {
                imports.insert(format!("{ns}.OneToOne"));
                lines.push(format!("@OneToOne(mappedBy = \"{mapped_by}\")"));
                (name, target, false)
            }
        };
        if collection {
            if kotlin {
                lines.push(format!("var {name}: MutableSet<{target}> = mutableSetOf()"));
            } else {
                imports.insert("java.util.LinkedHashSet".to_owned());
                imports.insert("java.util.Set".to_owned());
                lines.push(format!("private Set<{target}> {name} = new LinkedHashSet<>();"));
            }
            accessors.push((name.clone(), format!("Set<{target}>"), format!("MutableSet<{target}>")));
        } else {
            lines.push(if kotlin { property(name, target) } else { format!("private {target} {name};") });
            accessors.push((name.clone(), target.clone(), target.clone()));
        }
        body.push_str(&lines.iter().map(|l| format!("    {l}\n")).collect::<String>());
        body.push('\n');
    }
    for todo in &plan.todo_fields {
        let declaration = if kotlin { format!("var {}: Any? = null", todo.name) } else { format!("private Object {};", todo.name) };
        body.push_str(&format!(
            "    // TODO: column {} has {}; choose a Java type and uncomment\n    // @Column(name = \"{}\")\n    // {declaration}\n\n",
            todo.name, todo.sql_type, todo.column
        ));
    }
    let mut table_args = vec![format!("name = \"{}\"", literal(&plan.table, plan.table_quoted))];
    if let Some(schema) = &plan.schema {
        table_args.push(format!("schema = \"{schema}\""));
    }
    let strings = |items: &[String]| items.iter().map(|i| format!("\"{i}\"")).collect::<Vec<_>>().join(", ");
    let array = |items: Vec<String>| if kotlin { format!("[{}]", items.join(", ")) } else { format!("{{{}}}", items.join(", ")) };
    let at = if kotlin { "" } else { "@" };
    if !plan.indexes.is_empty() {
        imports.insert(format!("{ns}.Index"));
        let items = plan
            .indexes
            .iter()
            .map(|i| {
                let name = i.name.as_ref().map_or(String::new(), |n| format!("name = \"{n}\", "));
                let unique = if i.unique { ", unique = true" } else { "" };
                format!("{at}Index({name}columnList = \"{}\"{unique})", i.columns.join(", "))
            })
            .collect();
        table_args.push(format!("indexes = {}", array(items)));
    }
    if !plan.uniques.is_empty() {
        imports.insert(format!("{ns}.UniqueConstraint"));
        let items = plan
            .uniques
            .iter()
            .map(|u| {
                let name = u.name.as_ref().map_or(String::new(), |n| format!("name = \"{n}\", "));
                format!("{at}UniqueConstraint({name}columnNames = {})", array(vec![strings(&u.columns)]))
            })
            .collect();
        table_args.push(format!("uniqueConstraints = {}", array(items)));
    }
    let table_annotation = if table_args.len() > 2 || plan.schema.is_none() && table_args.len() > 1 {
        format!("@Table(\n    {}\n)", table_args.join(",\n    "))
    } else {
        format!("@Table({})", table_args.join(", "))
    };
    if plan.view {
        imports.insert("org.hibernate.annotations.Immutable".to_owned());
    }
    if plan.joined_root {
        imports.insert(format!("{ns}.Inheritance"));
        imports.insert(format!("{ns}.InheritanceType"));
    }
    if plan.join_column.is_some() {
        imports.insert(format!("{ns}.PrimaryKeyJoinColumn"));
    }
    let mut out = format!("package {package}{semi}\n\n");
    out.push_str(&imports.iter().map(|i| format!("import {i}{semi}\n")).collect::<String>());
    out.push('\n');
    if let Some(comment) = &plan.comment {
        out.push_str(&format!("/** {comment} */\n"));
    }
    if let Some(text) = &plan.comment_annotation {
        out.push_str(&format!("@org.hibernate.annotations.Comment(\"{text}\")\n"));
    }
    out.push_str("@Entity\n");
    if plan.view {
        out.push_str("@Immutable\n");
    }
    out.push_str(&format!("{table_annotation}\n"));
    if plan.joined_root {
        out.push_str("@Inheritance(strategy = InheritanceType.JOINED)\n");
    }
    if let Some(join) = &plan.join_column {
        out.push_str(&format!("@PrimaryKeyJoinColumn(name = \"{join}\")\n"));
    }
    if kotlin {
        let open = if plan.open { "open " } else { "" };
        let parent = plan.parent.as_ref().map_or(String::new(), |p| format!(" : {p}()"));
        let constructor = if plan.view { " protected constructor()" } else { "" };
        out.push_str(&format!("{open}class {}{constructor}{parent} {{\n", plan.class));
        out.push_str(body.trim_end());
        out.push_str("\n}\n");
    } else {
        let parent = plan.parent.as_ref().map_or(String::new(), |p| format!(" extends {p}"));
        out.push_str(&format!("public class {}{parent} {{\n", plan.class));
        out.push_str(&body);
        if plan.view {
            out.push_str(&format!("    protected {}() {{\n    }}\n\n", plan.class));
        }
        let methods: Vec<String> = accessors
            .iter()
            .map(|(field, ty, _)| {
                let cap = capitalize(field);
                let getter = format!("    public {ty} get{cap}() {{\n        return {field};\n    }}\n");
                if plan.view {
                    return getter;
                }
                format!("{getter}\n    public void set{cap}({ty} {field}) {{\n        this.{field} = {field};\n    }}\n")
            })
            .collect();
        out.push_str(&methods.join("\n"));
        out.push_str("}\n");
    }
    out
}

fn render_enum(plan: &EnumPlan, package: &str, kotlin: bool) -> String {
    let constants = plan.values.iter().map(|v| format!("    {v}")).collect::<Vec<_>>().join(",\n");
    if kotlin {
        format!("package {package}\n\nenum class {} {{\n{constants}\n}}\n", plan.name)
    } else {
        format!("package {package};\n\npublic enum {} {{\n{constants}\n}}\n", plan.name)
    }
}

struct Built3 {
    plans: Vec<Plan>,
    refused: Vec<String>,
    enums: Vec<EnumPlan>,
}

struct Junction {
    table: usize,
    ends: [(String, usize); 2],
}

/// A table of exactly two columns that together are its primary key, each a single-column foreign key to the
/// single-column primary key of another table. Both columns are returned with the table they reference.
fn junction_shape(parsed: &ParsedDdl, ti: usize, dialect: SqlDialect, options: &Options) -> Option<Junction> {
    let table = &parsed.snapshot.tables[ti];
    let case_sensitive = dialect == SqlDialect::PostgreSql;
    let (schema, name) = table.name.split_once('.').map_or((None, table.name.as_str()), |(schema, name)| (Some(schema), name));
    if options.views.contains(&ti) || !simple(name, case_sensitive) || schema.is_some_and(|s| !simple(s, case_sensitive)) {
        return None;
    }
    let columns = table.columns.known()?;
    let key = table.primary_key.known()?;
    let foreign_keys = table.foreign_keys.known()?;
    let [first, second] = columns.as_slice() else { return None };
    let is_key = |column: &Column| key.iter().any(|k| k.eq_ignore_ascii_case(&column.name));
    if key.len() != 2 || !is_key(first) || !is_key(second) || foreign_keys.len() != 2 {
        return None;
    }
    let end = |column: &Column| -> Option<(String, usize)> {
        if !simple(&column.name, case_sensitive) {
            return None;
        }
        let fk = foreign_keys.iter().find(|fk| fk.columns.len() == 1 && fk.columns[0].eq_ignore_ascii_case(&column.name))?;
        let target = ddl_parse::find_table(&parsed.snapshot, &fk.ref_table)?;
        let target_key = parsed.snapshot.tables[target].primary_key.known()?;
        let matches = match &fk.ref_columns {
            Fact::Known(refs) => refs.len() == 1 && target_key.len() == 1 && refs[0].eq_ignore_ascii_case(&target_key[0]),
            Fact::Unknown(_) => false,
        };
        (matches && target != ti).then(|| (column.name.clone(), target))
    };
    let ends = [end(first)?, end(second)?];
    (ends[0].1 != ends[1].1).then_some(Junction { table: ti, ends })
}

/// Free attribute name for a new member: `base`, else `baseByOwner`; `None` when both are taken or not identifiers.
fn free_name(taken: &BTreeSet<String>, base: &str, owner: &str) -> Option<String> {
    [base.to_owned(), format!("{base}By{}", capitalize(owner))].into_iter().find(|name| identifier(name) && !taken.contains(name))
}

fn names_of(plan: &Plan) -> BTreeSet<String> {
    plan.attrs.iter().map(|a| a.name.clone()).chain(plan.members.iter().map(|m| m.name().to_owned())).chain(plan.maps_id.iter().map(|m| m.name.clone())).collect()
}

/// Inverse sides of the generated foreign keys (`@OneToMany(mappedBy)` or `@OneToOne(mappedBy)`) on the referenced
/// entity, and `@ManyToMany` for junction tables between two generated entities. Only tables that are both generated.
fn add_members(parsed: &ParsedDdl, dialect: SqlDialect, planned: &mut [(usize, Plan)], junctions: &[Junction], options: &Options) {
    let links: Vec<(usize, String, String, Link, usize)> = planned
        .iter()
        .flat_map(|(ti, plan)| plan.attrs.iter().filter_map(move |a| Some((a.target_table?, plan.class.clone(), a.name.clone(), a.link, *ti))))
        .collect();
    for (target, child_class, owner_attr, link, _) in links {
        let Some((_, plan)) = planned.iter_mut().find(|(ti, _)| *ti == target) else { continue };
        let taken = names_of(plan);
        let (base, make): (String, fn(String, String, String) -> Member) = match link {
            Link::ManyToOne => (pluralize(&decapitalize(&child_class)), |name, target, mapped_by| Member::OneToMany { name, target, mapped_by }),
            Link::OneToOne => (decapitalize(&child_class), |name, target, mapped_by| Member::OneToOne { name, target, mapped_by }),
        };
        match free_name(&taken, &base, &owner_attr) {
            Some(name) => plan.members.push(make(name, child_class, owner_attr)),
            None => plan.notes.push(format!("no inverse side for {child_class}.{owner_attr}: the attribute name `{base}` is taken")),
        }
    }
    for junction in junctions {
        let name = &parsed.snapshot.tables[junction.table].name;
        let class_of = |ti: usize| planned.iter().find(|(t, _)| *t == ti).map(|(_, p)| p.class.clone());
        let ([(join, owner), (inverse, other)], Some(owner_class), Some(other_class)) = (&junction.ends, class_of(junction.ends[0].1), class_of(junction.ends[1].1)) else { continue };
        let quoted = |part: &str| literal(part, ddl_parse::is_reserved(dialect, part));
        let (schema, table) = name.split_once('.').map_or((None, name.as_str()), |(schema, table)| (Some(schema), table));
        let schema = schema.filter(|_| options.schema && dialect == SqlDialect::PostgreSql).map(str::to_owned);
        let owner_name = pluralize(&decapitalize(&other_class));
        let mapped_name = pluralize(&decapitalize(&owner_class));
        let free = |ti: usize, base: &str, planned: &[(usize, Plan)]| planned.iter().find(|(t, _)| *t == ti).and_then(|(_, p)| free_name(&names_of(p), base, ""));
        let (Some(owner_member), Some(mapped_member)) = (free(*owner, &owner_name, planned), free(*other, &mapped_name, planned)) else {
            for (ti, plan) in planned.iter_mut().filter(|(ti, _)| ti == owner || ti == other) {
                let _ = ti;
                plan.notes.push(format!("junction table {name} is not mapped: the attribute names `{owner_name}` and `{mapped_name}` are not both free"));
            }
            continue;
        };
        if let Some((_, plan)) = planned.iter_mut().find(|(ti, _)| ti == owner) {
            plan.notes.push(format!("junction table {name} is mapped as @ManyToMany with {other_class}; no entity is generated for it"));
            plan.members.push(Member::ManyToManyOwner { name: owner_member.clone(), target: other_class.clone(), table: quoted(table), schema, join: quoted(join), inverse: quoted(inverse) });
        }
        if let Some((_, plan)) = planned.iter_mut().find(|(ti, _)| ti == other) {
            plan.members.push(Member::ManyToManyMapped { name: mapped_member, target: owner_class, mapped_by: owner_member });
        }
    }
}

/// Plans every selected table once alone (refusals, class and enum names), then again with
/// relations to the tables that were planned successfully.
fn build_with(parsed: &ParsedDdl, dialect: SqlDialect, selected: &[usize], options: &Options) -> Result<Built3, Error> {
    let alone = HashMap::new();
    let mut linked: HashMap<usize, String> = HashMap::new();
    let mut enums: Vec<EnumPlan> = Vec::new();
    let mut refused: Vec<String> = Vec::new();
    let mut junction_candidates = Vec::new();
    for &ti in selected {
        let name = &parsed.snapshot.tables[ti].name;
        if let Some(junction) = junction_shape(parsed, ti, dialect, options) {
            junction_candidates.push(junction);
            continue;
        }
        let first = match plan(parsed, ti, dialect, &alone, options) {
            Ok(first) => first,
            Err(reason) => {
                refused.push(reason);
                continue;
            }
        };
        let clash = if linked.values().any(|c| *c == first.class) {
            Some(format!("class name {} is produced by two tables", first.class))
        } else if enums.iter().any(|e| e.name == first.class) {
            Some(format!("class name {} equals a generated enum name", first.class))
        } else {
            first.enums.iter().find_map(|e| match enums.iter().find(|x| x.name == e.name) {
                Some(other) if other != e => Some(format!("enum name {} is produced with different values", e.name)),
                None if e.name == first.class || linked.values().any(|c| *c == e.name) => Some(format!("enum name {} equals a generated class name", e.name)),
                _ => None,
            })
        };
        if let Some(clash) = clash {
            refused.push(format!("{name}: {clash}"));
            continue;
        }
        for e in &first.enums {
            if !enums.contains(e) {
                enums.push(e.clone());
            }
        }
        linked.insert(ti, first.class);
    }
    let mut junctions = Vec::new();
    for junction in junction_candidates {
        if junction.ends.iter().all(|(_, target)| linked.contains_key(target)) {
            junctions.push(junction);
        } else if let Err(reason) = plan(parsed, junction.table, dialect, &alone, options) {
            refused.push(reason);
        }
    }
    let mut local = options.clone();
    let mut downgraded: BTreeMap<usize, String> = BTreeMap::new();
    let mut planned: Vec<(usize, Plan)>;
    loop {
        planned = Vec::new();
        for &ti in selected.iter().filter(|ti| linked.contains_key(ti)) {
            planned.push((ti, plan(parsed, ti, dialect, &linked, &local).map_err(Error::from)?));
        }
        let clash = planned.iter().find_map(|(ti, child)| {
            let parent = child.parent.as_ref()?;
            let (mut inherited, mut current) = (Vec::new(), *ti);
            for _ in 0..20 {
                let Some(above) = local.parents.get(&current).and_then(|p| planned.iter().find(|(t, _)| t == p)) else { break };
                inherited.extend(above.1.attrs.iter().map(|a| a.name.as_str()));
                current = above.0;
            }
            child.attrs.iter().find(|a| inherited.contains(&a.name.as_str())).map(|a| (*ti, format!("JOINED inheritance from {parent} not applied: attribute `{}` is also an attribute of a parent class", a.name)))
        });
        let Some((ti, why)) = clash else { break };
        local.parents.remove(&ti);
        downgraded.insert(ti, why);
    }
    add_members(parsed, dialect, &mut planned, &junctions, &local);
    let parent_classes: BTreeSet<String> = planned.iter().filter_map(|(_, p)| p.parent.clone()).collect();
    let mut plans = Vec::new();
    for (ti, mut p) in planned {
        p.open = parent_classes.contains(&p.class);
        p.joined_root = p.open && p.parent.is_none();
        p.notes.extend(downgraded.get(&ti).cloned());
        plans.push(p);
    }
    Ok(Built3 { plans, refused, enums })
}

fn absolute(root: &str, file: &str) -> String {
    if file.starts_with('/') { file.to_owned() } else { format!("{}/{file}", root.trim_end_matches('/')) }
}

struct Sources {
    root_dir: String,
    packages: BTreeSet<String>,
    files: Vec<String>,
}

/// The single `src/main/java|kotlin` root that already holds sources; refuses zero or several.
fn sources(project: &Project, root: &str, kotlin: bool) -> Result<Sources, Error> {
    let (folder, extension) = if kotlin { ("kotlin", "kt") } else { ("java", "java") };
    let marker = format!("/src/main/{folder}/");
    let found = project.find_files(&format!("**/*.{extension}"), MAX_SOURCES)?;
    if found.len() >= MAX_SOURCES as usize {
        return Err("The project has too many sources to discover the source root unambiguously.".into());
    }
    let mut roots: BTreeMap<String, (BTreeSet<String>, Vec<String>)> = BTreeMap::new();
    for file in found {
        let file = absolute(root, &file);
        if path(&file).as_deref() != Some(file.as_str()) || !inside(root, &file) {
            continue;
        }
        let Some(at) = file.find(&marker) else { continue };
        let root_dir = file[..at + marker.len() - 1].to_owned();
        let entry = roots.entry(root_dir.clone()).or_default();
        entry.1.push(file.clone());
        if let Some((directory, _)) = file[root_dir.len() + 1..].rsplit_once('/') {
            entry.0.insert(directory.replace('/', "."));
        }
    }
    let mut roots = roots.into_iter();
    match (roots.next(), roots.next()) {
        (Some((root_dir, (packages, files))), None) => Ok(Sources { root_dir, packages, files }),
        (None, _) => Err(format!("No sources under src/main/{folder} were found, so the source root is not known.").into()),
        (Some(first), Some(second)) => Err(format!(
            "The source root is ambiguous ({} and {}{}); nothing was generated.",
            first.0,
            second.0,
            if roots.next().is_some() { " and more" } else { "" }
        )
        .into()),
    }
}

fn prompt_package(found: &Sources) -> Result<String, Error> {
    let mut items: Vec<PickItem> = found.packages.iter().take(MAX_PACKAGES).map(|p| PickItem::new(p, p)).collect();
    items.push(PickItem::new(OTHER_PACKAGE, "Another package…"));
    let chosen = host::pick("Target package for the entities", &items)?.ok_or(Error::Cancelled)?;
    let package = if chosen == OTHER_PACKAGE {
        host::input("Package name", None)?.ok_or(Error::Cancelled)?.trim().to_owned()
    } else {
        chosen
    };
    if package.is_empty() || package.len() > 512 || !package.split('.').all(identifier) {
        return Err("Choose a valid package name.".into());
    }
    Ok(package)
}

fn detect_namespace(project: &Project, files: &[String]) -> Option<&'static str> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let (mut jakarta, mut javax) = (false, false);
    for file in files.iter().take(300) {
        if Instant::now() >= deadline {
            return None;
        }
        let Ok(text) = project.read_file(file) else { continue };
        if text.len() > MAX_TEXT {
            continue;
        }
        jakarta |= text.contains("import jakarta.persistence");
        javax |= text.contains("import javax.persistence");
    }
    match (jakarta, javax) {
        (true, false) => Some("jakarta.persistence"),
        (false, true) => Some("javax.persistence"),
        _ => None,
    }
}

fn describe(error: &DdlError) -> String {
    match error {
        DdlError::TooLarge { len } => format!("The SQL file is {len} bytes, over the supported bound."),
        DdlError::TooManyStatements => "The SQL file has too many statements.".into(),
        DdlError::Schema(error) => format!("The SQL file defines an unsupported schema shape: {error:?}"),
    }
}

/// A comma-separated list of prefixes or suffixes; each of letters, digits and underscores.
fn prompt_affixes(title: &str) -> Result<Vec<String>, Error> {
    let value = host::input(title, None)?.ok_or(Error::Cancelled)?;
    let affixes: Vec<String> = value.split(',').map(|part| part.trim().to_owned()).collect();
    let valid = affixes.len() <= MAX_AFFIXES
        && affixes.iter().all(|a| !a.is_empty() && a.len() <= MAX_PREFIX && a.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    if !valid {
        return Err(format!("Choose a comma-separated list of at most {MAX_AFFIXES} entries, each of letters, digits and underscores (at most {MAX_PREFIX} characters).").into());
    }
    Ok(affixes)
}

fn prompt_options(parsed: &ParsedDdl, dialect: SqlDialect, selected: &[usize], comment_annotation_ok: bool) -> Result<Options, Error> {
    let tables = &parsed.snapshot.tables;
    let chosen_tables: Vec<&Table> = selected.iter().map(|&ti| &tables[ti]).collect();
    let names: BTreeSet<&str> = chosen_tables.iter().map(|t| t.name.as_str()).collect();
    let mut items = Vec::new();
    if dialect == SqlDialect::PostgreSql && chosen_tables.iter().any(|t| t.name.contains('.')) {
        items.push(PickItem::new("schema", "Write the schema into @Table(schema)").description("Otherwise the schema qualifier is dropped"));
    }
    let has_indexes = chosen_tables.iter().any(|t| {
        t.indexes.known().is_some_and(|i| !i.is_empty()) || t.unique_constraints.known().is_some_and(|u| u.iter().any(|u| u.columns.len() > 1))
    });
    if has_indexes {
        items.push(PickItem::new("indexes", "Map indexes and composite unique constraints").description("@Table(indexes, uniqueConstraints)"));
    }
    if parsed.comments.iter().any(|c| names.contains(c.table.as_str())) {
        items.push(PickItem::new("comments", "Copy database comments as Javadoc").description("Up to 240 characters each"));
        if comment_annotation_ok {
            items.push(PickItem::new("commentAnnotation", "Copy database comments as @Comment (Hibernate)").description("org.hibernate.annotations.Comment reaches generated DDL; up to 240 characters each"));
        }
    }
    if parsed.column_enums.iter().any(|e| names.contains(e.table.as_str())) {
        items.push(PickItem::new("enums", "Generate enums for enumerated columns").description("New enum files; @Enumerated(EnumType.STRING)"));
    }
    let unmapped_present = parsed.unmapped_columns.iter().any(|u| {
        names.contains(u.table.as_str()) && !parsed.column_enums.iter().any(|e| e.table == u.table && e.column == u.column)
    });
    if unmapped_present {
        items.push(PickItem::new("deferred", "Decide per column for unmapped SQL types").description("Skip it (listed as deferred) or String with @Column(columnDefinition); never a guessed Java type"));
    }
    items.push(PickItem::new("singular", "Singularize class names").description("ies->y, ses->s, xes->x, ches->ch, shes->sh, sses->ss, trailing s; editable next"));
    items.push(PickItem::new("tablePrefix", "Strip table-name prefixes").description("Comma-separated, for example tbl_,old_"));
    items.push(PickItem::new("columnPrefix", "Strip column-name prefixes").description("Comma-separated, for example col_"));
    items.push(PickItem::new("tableSuffix", "Strip table-name suffixes").description("Comma-separated, for example _tbl,_old"));
    items.push(PickItem::new("columnSuffix", "Strip column-name suffixes").description("Comma-separated, for example _col"));
    let picked: BTreeSet<String> = host::pick_many("Generation options (none chosen: defaults)", &items)?.ok_or(Error::Cancelled)?.into_iter().collect();
    let mut options = Options {
        schema: picked.contains("schema"),
        indexes: picked.contains("indexes"),
        comments: picked.contains("comments"),
        comment_annotation: picked.contains("commentAnnotation") && comment_annotation_ok,
        enums: picked.contains("enums"),
        singular: picked.contains("singular"),
        offer_deferred: picked.contains("deferred"),
        ..Options::default()
    };
    if picked.contains("tablePrefix") {
        options.table_prefixes = prompt_affixes("Table prefixes to strip (comma-separated, case-insensitive)")?;
    }
    if picked.contains("columnPrefix") {
        options.column_prefixes = prompt_affixes("Column prefixes to strip (comma-separated, case-insensitive)")?;
    }
    if picked.contains("tableSuffix") {
        options.table_suffixes = prompt_affixes("Table suffixes to strip (comma-separated, case-insensitive)")?;
    }
    if picked.contains("columnSuffix") {
        options.column_suffixes = prompt_affixes("Column suffixes to strip (comma-separated, case-insensitive)")?;
    }
    Ok(options)
}

fn prompt_classes(parsed: &ParsedDdl, dialect: SqlDialect, selected: &[usize], options: &mut Options) -> Result<(), Error> {
    if !options.singular && options.table_prefixes.is_empty() && options.table_suffixes.is_empty() {
        return Ok(());
    }
    for &ti in selected {
        let Ok(found) = gate(parsed, ti, dialect, options) else { continue };
        let name = host::input(&format!("Class name for table {}", parsed.snapshot.tables[ti].name), Some(&found.class))?.ok_or(Error::Cancelled)?;
        let name = name.trim().to_owned();
        if !identifier(&name) {
            return Err(format!("`{name}` is not a valid class name.").into());
        }
        options.classes.insert(ti, name);
    }
    Ok(())
}

fn prompt_columns(parsed: &ParsedDdl, dialect: SqlDialect, selected: &[usize], options: &mut Options) -> Result<(), Error> {
    for &ti in selected {
        let Ok(found) = gate(parsed, ti, dialect, options) else { continue };
        if found.columns.len() < 2 {
            continue;
        }
        let alone = HashMap::new();
        let mut items = vec![PickItem::new(ALL_COLUMNS, "All columns").description("Chosen when nothing else is")];
        for column in found.columns {
            let description = if column.name.eq_ignore_ascii_case(found.key) {
                "primary key, always mapped".to_owned()
            } else {
                match column_attr(parsed, ti, dialect, options, &found, column, &alone) {
                    Ok(built) => built.attr.java,
                    Err(reason) => format!("refused: {reason}"),
                }
            };
            items.push(PickItem::new(&column.name, &column.name).description(&description));
        }
        let table = &parsed.snapshot.tables[ti].name;
        let chosen = host::pick_many(&format!("Columns to map for {table} (the primary key is always mapped)"), &items)?.ok_or(Error::Cancelled)?;
        if chosen.is_empty() || chosen.iter().any(|c| c == ALL_COLUMNS) {
            continue;
        }
        let mut set = BTreeSet::new();
        for name in &chosen {
            if !found.columns.iter().any(|c| c.name == *name) {
                return Err(Error::Stale);
            }
            set.insert(name.to_ascii_lowercase());
        }
        options.columns.insert(ti, set);
    }
    Ok(())
}

fn prompt_deferred(parsed: &ParsedDdl, dialect: SqlDialect, selected: &[usize], options: &mut Options) -> Result<(), Error> {
    if !options.offer_deferred {
        return Ok(());
    }
    for &ti in selected {
        let Ok(found) = gate(parsed, ti, dialect, options) else { continue };
        let table = &parsed.snapshot.tables[ti].name;
        for column in found.columns {
            let wanted = options.columns.get(&ti).is_none_or(|set| set.contains(&column.name.to_ascii_lowercase()));
            let enumerated = parsed.column_enums.iter().any(|e| e.table == *table && e.column == column.name);
            let is_key = column.name.eq_ignore_ascii_case(found.key);
            if !wanted || enumerated || is_key || !matches!(column.sql_type, Fact::Unknown(_)) {
                continue;
            }
            let Some(raw) = unmapped_text(parsed, table, &column.name).filter(|raw| safe_definition(raw)) else { continue };
            let items = [
                PickItem::new("skip", "Skip the column").description("Not mapped; listed as deferred in the preview"),
                PickItem::new("string", &format!("String with @Column(columnDefinition = \"{raw}\")")).description("No Java type is guessed; binding a String to this SQL type is not verified"),
            ];
            let chosen = host::pick(&format!("Column {} of {table} has the unmapped SQL type {raw}", column.name), &items)?.ok_or(Error::Cancelled)?;
            let decision = match chosen.as_str() {
                "skip" => Deferred::Skip,
                "string" => Deferred::Definition(raw.to_owned()),
                _ => return Err(Error::Stale),
            };
            options.deferred.insert((ti, column.name.to_ascii_lowercase()), decision);
        }
    }
    Ok(())
}

fn leads_to(options: &Options, mut from: usize, target: usize) -> bool {
    for _ in 0..20 {
        if from == target {
            return true;
        }
        let Some(&next) = options.parents.get(&from) else { return false };
        from = next;
    }
    true
}

/// Selected tables whose single-column primary key is also a foreign key to the single-column primary key of another selected table.
fn shared_key_parents(parsed: &ParsedDdl, dialect: SqlDialect, selected: &[usize], ti: usize, options: &Options) -> Vec<usize> {
    let Ok(child) = gate(parsed, ti, dialect, options) else { return Vec::new() };
    let key_generated = child.columns.iter().any(|c| c.name.eq_ignore_ascii_case(child.key) && c.generated != Fact::Known(false));
    if options.views.contains(&ti) || key_generated {
        return Vec::new();
    }
    child
        .foreign_keys
        .iter()
        .filter(|fk| fk.columns.len() == 1 && fk.columns[0].eq_ignore_ascii_case(child.key))
        .filter_map(|fk| {
            let parent = ddl_parse::find_table(&parsed.snapshot, &fk.ref_table).filter(|p| *p != ti && selected.contains(p) && !options.views.contains(p))?;
            let found = gate(parsed, parent, dialect, options).ok()?;
            match &fk.ref_columns {
                Fact::Known(refs) if refs.len() == 1 && refs[0].eq_ignore_ascii_case(found.key) => Some(parent),
                _ => None,
            }
        })
        .collect()
}

fn prompt_parents(parsed: &ParsedDdl, dialect: SqlDialect, selected: &[usize], options: &mut Options) -> Result<(), Error> {
    let tables = &parsed.snapshot.tables;
    for &ti in selected {
        let mut candidates = shared_key_parents(parsed, dialect, selected, ti, options);
        candidates.retain(|&parent| !leads_to(options, parent, ti));
        if candidates.is_empty() {
            continue;
        }
        let mut items = vec![PickItem::new("\u{0}none", "Normal entity").description("The key stays this entity's own @Id; with one candidate parent it also gets @OneToOne @MapsId @JoinColumn(nullable = false), optional = false")];
        for &parent in &candidates {
            items.push(
                PickItem::new(&tables[parent].name, &format!("Extends {} (JOINED inheritance)", tables[parent].name))
                    .description("Shared primary key: @Inheritance(JOINED) on the root, @PrimaryKeyJoinColumn here, no @Id; no discriminator column"),
            );
        }
        let title = format!("Table {}: its primary key is also a foreign key to another selected table", tables[ti].name);
        let chosen = host::pick(&title, &items)?.ok_or(Error::Cancelled)?;
        if chosen == "\u{0}none" {
            if let [only] = candidates.as_slice() {
                options.maps_id.insert(ti, *only);
            }
            continue;
        }
        let parent = candidates.iter().find(|&&p| tables[p].name == chosen).ok_or(Error::Stale)?;
        options.parents.insert(ti, *parent);
    }
    Ok(())
}

/// A view has no key: the user picks exactly one column as `@Id`.
fn prompt_view_ids(parsed: &mut ParsedDdl, dialect: SqlDialect, selected: &[usize], options: &Options) -> Result<(), Error> {
    for &ti in selected.iter().filter(|ti| options.views.contains(ti)) {
        let Ok(found) = gate(parsed, ti, dialect, options) else { continue };
        let alone = HashMap::new();
        let items: Vec<PickItem> = found
            .columns
            .iter()
            .map(|column| {
                let description = match column_attr(parsed, ti, dialect, options, &found, column, &alone) {
                    Ok(built) => built.attr.java,
                    Err(reason) => format!("refused: {reason}"),
                };
                PickItem::new(&column.name, &column.name).description(&description)
            })
            .collect();
        let names: Vec<String> = found.columns.iter().map(|c| c.name.clone()).collect();
        let view = parsed.snapshot.tables[ti].name.clone();
        let chosen = host::pick_many(&format!("Id column for view {view} (exactly one; a view has no primary key)"), &items)?.ok_or(Error::Cancelled)?;
        let [id] = chosen.as_slice() else {
            return Err(format!("View {view}: choose exactly one id column; a composite id is not generated for views.").into());
        };
        if !names.contains(id) {
            return Err(Error::Stale);
        }
        parsed.snapshot.tables[ti].primary_key = Fact::Known(vec![id.clone()]);
    }
    Ok(())
}

/// Views become synthetic tables after the real ones, keyed by their first column until the user picks the id.
fn append_views(parsed: &mut ParsedDdl) -> BTreeSet<usize> {
    let mut indexes = BTreeSet::new();
    for view in parsed.views.clone() {
        let Some(first) = view.columns.first() else { continue };
        indexes.insert(parsed.snapshot.tables.len());
        parsed.snapshot.tables.push(Table {
            name: view.name.clone(),
            columns: Fact::Known(view.columns.clone()),
            primary_key: Fact::Known(vec![first.name.clone()]),
            foreign_keys: Fact::Known(Vec::new()),
            unique_constraints: Fact::Known(Vec::new()),
            indexes: Fact::Known(Vec::new()),
            provenance: persistence_core::schema::Provenance::Derived,
        });
    }
    indexes
}

fn import_defaults(options: &mut Options) -> Result<(), Error> {
    options.id_generation = match host::setting_string("sqlImportIdGeneration").as_deref() {
        None | Some("identity") => IdGeneration::Identity,
        Some("sequence") => IdGeneration::Sequence,
        Some("auto") => IdGeneration::Auto,
        Some("uuid") => IdGeneration::Uuid,
        Some("none") => IdGeneration::Unassigned,
        Some(other) => return Err(format!("Unknown sqlImportIdGeneration `{other}`; use identity, sequence, auto, uuid or none.").into()),
    };
    options.default_fetch = match host::setting_string("sqlImportFetch").as_deref() {
        None | Some("lazy") => false,
        Some("default") => true,
        Some(other) => return Err(format!("Unknown sqlImportFetch `{other}`; use lazy or default.").into()),
    };
    options.never_required = match host::setting_string("sqlImportOptional").as_deref() {
        None | Some("nullability") => false,
        Some("never") => true,
        Some(other) => return Err(format!("Unknown sqlImportOptional `{other}`; use nullability or never.").into()),
    };
    Ok(())
}

/// Build text that could hide a Kotlin plugin or dependency from a literal scan.
const HIDING_BUILD_TEXT: [&str; 14] = [
    "alias(", "buildSrc", "includeBuild", "apply(from", "apply from", "noArg", "no-arg", "plugin.noarg", "libs.", "versionCatalogs", "platform(",
    "<scope>import</scope>", "pluginManagement", "gradle.properties",
];

/// Kotlin is absent from the module when `project_facts` could not tell (Unknown, usually because of a Maven
/// `<parent>` or Gradle `allprojects`), no build file of the module or an ancestor mentions Kotlin (a plugin
/// declaration written `apply false` does not apply it), nothing in them hides one, and no `.kt` source exists
/// under the module. A parent POM outside the project is not read.
fn kotlin_proven_absent(project: &Project, root: &str, facts: &crate::project_facts::ProjectFacts) -> bool {
    if facts.kotlin != crate::project_facts::Fact::Unknown {
        return false;
    }
    let Ok(build) = super::mapper_action::build_text(project, root, &facts.module) else { return false };
    let applied: String = build.lines().filter(|line| !line.contains("apply false")).collect::<Vec<_>>().join("\n");
    let lower = applied.to_ascii_lowercase();
    if applied.trim().is_empty() || lower.contains("kotlin") || HIDING_BUILD_TEXT.iter().any(|needle| applied.contains(needle)) {
        return false;
    }
    let Ok(found) = project.find_files("**/*.kt", MAX_SOURCES) else { return false };
    let module = facts.module.trim_end_matches('/');
    found.len() < MAX_SOURCES as usize && !found.iter().any(|file| inside(module, &absolute(root, file)))
}

fn version_pair(text: &str) -> Option<(u32, u32)> {
    let text = text.trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '\'' | '"' | '(' | '=' | ':'));
    let digits = |value: &str| -> Option<(u32, usize)> {
        let end = value.find(|c: char| !c.is_ascii_digit()).unwrap_or(value.len());
        value[..end].parse().ok().map(|number| (number, end))
    };
    let (major, end) = digits(text)?;
    let (minor, _) = digits(text[end..].strip_prefix('.')?)?;
    Some((major, minor))
}

/// `GenerationType.UUID` exists from Jakarta Persistence 3.1 (Hibernate 6.2, Spring Boot 3.1). It is written only
/// for the `jakarta.persistence` namespace and a literal Spring Boot 3.1+ or hibernate-core 6.2+ version.
fn uuid_generation_proof(ns: &str, build: &str) -> Result<(), Error> {
    let refuse = |reason: &str| Err(Error::from(format!("sqlImportIdGeneration = uuid writes GenerationType.UUID, which needs Jakarta Persistence 3.1 (Hibernate 6.2, Spring Boot 3.1): {reason} Choose another id generation or define the key yourself; nothing was generated.")));
    if ns != "jakarta.persistence" {
        return refuse("javax.persistence has no GenerationType.UUID.");
    }
    if super::hibernate_action::hibernate6(build).is_err() {
        return refuse("Hibernate 6 is not provable from the build files.");
    }
    let mut hibernate = Vec::new();
    for needle in ["org.hibernate.orm:hibernate-core:", "org.hibernate:hibernate-core:"] {
        hibernate.extend(build.match_indices(needle).filter_map(|(at, _)| version_pair(&build[at + needle.len()..])));
    }
    for (at, tag) in build.match_indices("<artifactId>hibernate-core</artifactId>") {
        let window = &build[at + tag.len()..(at + tag.len() + 200).min(build.len())];
        hibernate.extend(window.split_once("<version>").and_then(|(_, rest)| version_pair(rest)));
    }
    let mut boot = Vec::new();
    for artifact in ["spring-boot-starter-parent", "spring-boot-dependencies"] {
        let tag = format!("<artifactId>{artifact}</artifactId>");
        for (at, _) in build.match_indices(&tag) {
            let window = &build[at..(at + 300).min(build.len())];
            boot.extend(window.split_once("<version>").and_then(|(_, rest)| version_pair(rest)));
        }
    }
    for (at, needle) in build.match_indices("org.springframework.boot") {
        let line = build[at + needle.len()..].lines().next().unwrap_or("");
        boot.extend(line.split_once("version").or_else(|| line.split_once("spring-boot-gradle-plugin:")).and_then(|(_, after)| version_pair(after)));
    }
    let proven = match (hibernate.is_empty(), boot.is_empty()) {
        (false, _) => hibernate.iter().all(|pair| *pair >= (6, 2)),
        (true, false) => boot.iter().all(|pair| *pair >= (3, 1)),
        (true, true) => false,
    };
    if !proven {
        return refuse("a literal Spring Boot 3.1+ or hibernate-core 6.2+ version is not written in the build files.");
    }
    Ok(())
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let (dialect, detected) = match ddl_parse::detect_dialect(&doc.text) {
        Some(dialect) => (dialect, true),
        None => {
            let chosen = crate::choose_dialect_for(None)?.ok_or(Error::Cancelled)?;
            (if chosen == crate::Dialect::MySql { SqlDialect::MySql } else { SqlDialect::PostgreSql }, false)
        }
    };
    let mut parsed = ddl_parse::parse_ddl(&doc.text, dialect).map_err(|e| Error::from(describe(&e)))?;
    if parsed.snapshot.tables.is_empty() {
        return Err("No CREATE TABLE statement could be read from this file.".into());
    }
    let mut defaults = Options::default();
    import_defaults(&mut defaults)?;
    defaults.views = append_views(&mut parsed);
    let tables = &parsed.snapshot.tables;
    let alone = HashMap::new();
    let items: Vec<_> = (0..tables.len())
        .map(|ti| {
            let view = defaults.views.contains(&ti);
            let description = match plan(&parsed, ti, dialect, &alone, &defaults) {
                Ok(p) if view => format!("{} attributes; read-only @Immutable, needs Hibernate and an id column you choose", p.attrs.len()),
                Ok(p) => format!("{} attributes", p.attrs.len()),
                Err(reason) => format!("refused: {reason}"),
            };
            let label = if view { format!("{} (VIEW)", tables[ti].name) } else { tables[ti].name.clone() };
            PickItem::new(&tables[ti].name, &label).description(&description)
        })
        .collect();
    let chosen = host::pick_many("Tables to generate entities for (at most 20)", &items)?.ok_or(Error::Cancelled)?;
    if chosen.is_empty() || chosen.len() > MAX_FILES {
        return Err("Choose between one and 20 tables.".into());
    }
    let mut selected: Vec<usize> = Vec::new();
    for name in &chosen {
        let ti = tables.iter().position(|t| t.name == *name).ok_or(Error::Stale)?;
        if !selected.contains(&ti) {
            selected.push(ti);
        }
    }

    let setting = host::setting_string("entityLanguage");
    let module_dir = doc.path.rsplit_once('/').map_or(pending.root.as_str(), |(dir, _)| dir);
    let mut facts = crate::project_facts::detect_cached(project, module_dir);
    if kotlin_proven_absent(project, &pending.root, &facts) {
        facts.kotlin = crate::project_facts::Fact::No;
    }
    let kotlin = match crate::project_facts::decided_language(setting.as_deref(), &facts) {
        Some(kotlin) => kotlin,
        None => crate::choose_entity_language()?.ok_or(Error::Cancelled)?,
    };
    let found = sources(project, &pending.root, kotlin)?;
    let package = prompt_package(&found)?;
    let ns = match detect_namespace(project, &found.files) {
        Some(ns) => ns,
        None => {
            let items = [PickItem::new("jakarta", "Jakarta Persistence"), PickItem::new("javax", "Javax Persistence")];
            match host::pick("Persistence namespace for the new entities", &items)?.ok_or(Error::Cancelled)?.as_str() {
                "jakarta" => "jakarta.persistence",
                "javax" => "javax.persistence",
                _ => return Err(Error::Stale),
            }
        }
    };

    let mut view_options = Options { views: defaults.views.clone(), ..Options::default() };
    if selected.iter().any(|ti| view_options.views.contains(ti)) {
        let build = super::mapper_action::build_text(project, &pending.root, &found.root_dir)?;
        match super::hibernate_action::hibernate6(&build) {
            Ok(_) => prompt_view_ids(&mut parsed, dialect, &selected, &view_options)?,
            Err(reason) => view_options.view_block = Some(format!("a view is mapped with the Hibernate @Immutable annotation, and {reason}")),
        }
    }
    let comment_annotation_ok = facts.hibernate6 == crate::project_facts::Fact::Yes;
    let mut options = prompt_options(&parsed, dialect, &selected, comment_annotation_ok)?;
    options.kotlin = kotlin;
    let validation_note = match (host::setting_bool("sqlImportValidation").unwrap_or(false), facts.bean_validation) {
        (false, _) => None,
        (true, crate::project_facts::Fact::Yes) => {
            options.validation = Some(if ns == "jakarta.persistence" { "jakarta.validation.constraints" } else { "javax.validation.constraints" });
            None
        }
        (true, _) => Some("sqlImportValidation is on, but Bean Validation (jakarta.validation, javax.validation, hibernate-validator or spring-boot-starter-validation) is not literally declared in the build files above this module: no @NotNull/@Size written."),
    };
    options.views = view_options.views;
    options.view_block = view_options.view_block;
    options.id_generation = defaults.id_generation;
    options.default_fetch = defaults.default_fetch;
    options.never_required = defaults.never_required;
    prompt_classes(&parsed, dialect, &selected, &mut options)?;
    prompt_columns(&parsed, dialect, &selected, &mut options)?;
    prompt_deferred(&parsed, dialect, &selected, &mut options)?;
    prompt_parents(&parsed, dialect, &selected, &mut options)?;
    let Built3 { plans, refused, enums } = build_with(&parsed, dialect, &selected, &options)?;
    if plans.is_empty() {
        return Err(format!("Every selected table was refused: {}", refused.join("; ")).into());
    }
    if options.id_generation == IdGeneration::Uuid && plans.iter().any(|p| p.attrs.iter().any(|a| a.key && a.java == "UUID")) {
        let build = super::mapper_action::build_text(project, &pending.root, &found.root_dir)?;
        uuid_generation_proof(ns, &build)?;
    }
    if plans.len() + enums.len() > MAX_FILES {
        return Err(format!("{} files (entities and enums) exceed the limit of {MAX_FILES}; pick fewer tables or columns.", plans.len() + enums.len()).into());
    }

    let directory = format!("{}/{}", found.root_dir, package.replace('.', "/"));
    let extension = if kotlin { "kt" } else { "java" };
    let mut files = Vec::new();
    for plan in &plans {
        let target = format!("{directory}/{}.{extension}", plan.class);
        let text = render(plan, &package, ns, kotlin);
        let generated = parse_class(&text, &target).filter(|c| c.kind == Kind::Entity).ok_or(Error::Stale)?;
        if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) || text.len() > MAX_TEXT || generated.name != plan.class {
            return Err("Generated target exceeds source/path bounds.".into());
        }
        no_collision(project, &pending.root, &directory, &plan.class, None)?;
        files.push((target, text));
    }
    for plan in &enums {
        let target = format!("{directory}/{}.{extension}", plan.name);
        let text = render_enum(plan, &package, kotlin);
        if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) || text.len() > MAX_TEXT {
            return Err("Generated target exceeds source/path bounds.".into());
        }
        no_collision(project, &pending.root, &directory, &plan.name, None)?;
        files.push((target, text));
    }

    let mut preview = format!(
        "SQL file: {}\nSource version: {}\nDialect: {}\nNamespace: {ns}\nLanguage: {}\nTarget folder: {directory}\n\nGenerated ({}):\n",
        doc.path,
        doc.version,
        format!("{} ({})", if dialect == SqlDialect::MySql { "MySQL" } else { "PostgreSQL" }, if detected { "detected from syntax" } else { "chosen" }),
        if kotlin { "Kotlin" } else { "Java" },
        files.len()
    );
    for (plan, (target, _)) in plans.iter().zip(&files) {
        let inherited = plan.parent.as_ref().map(|p| format!("inherited from {p}"));
        let key = plan.attrs.iter().find(|a| a.key).map(|a| a.name.clone()).or(inherited).unwrap_or_else(|| "-".into());
        let relations = plan.attrs.iter().filter(|a| a.target.is_some()).count();
        let uuid_key = plan.id_generation == IdGeneration::Uuid && plan.attrs.iter().any(|a| a.key && !a.identity && a.java == "UUID");
        let generation = match plan.id_generation {
            _ if uuid_key => " (UUID)",
            _ if !plan.attrs.iter().any(|a| a.identity) => "",
            IdGeneration::Sequence => " (SEQUENCE)",
            IdGeneration::Auto => " (AUTO)",
            IdGeneration::Unassigned => " (no @GeneratedValue)",
            IdGeneration::Identity | IdGeneration::Uuid => " (IDENTITY)",
        };
        let kind = if plan.view { "view (@Immutable)" } else { "table" };
        preview.push_str(&format!("- {target}: {kind} {}, {} attributes, @Id {key}{generation}, {relations} to-one, {} collection/inverse\n", plan.table, plan.attrs.len(), plan.members.len()));
        if let Some(parent) = &plan.parent {
            preview.push_str(&format!("    JOINED: extends {parent}, @PrimaryKeyJoinColumn\n"));
        }
        if plan.joined_root {
            preview.push_str("    JOINED: @Inheritance(strategy = JOINED) root, no discriminator column\n");
        }
        if plan.view {
            preview.push_str("    note: the id column was chosen by you and is not checked for uniqueness; Hibernate schema generation is not verified for views\n");
        }
        preview.extend(plan.deferred.iter().map(|d| format!("    deferred: {d} is not mapped\n")));
        if plan.attrs.iter().any(|a| a.definition.is_some()) {
            preview.push_str("    note: columnDefinition columns are String; binding a String to that SQL type is not verified\n");
        }
        preview.extend(plan.notes.iter().map(|n| format!("    note: {n}\n")));
    }
    for (plan, (target, _)) in enums.iter().zip(&files[plans.len()..]) {
        preview.push_str(&format!("- {target}: enum {} with {} constants, @Enumerated(EnumType.STRING)\n", plan.name, plan.values.len()));
    }
    if !enums.is_empty() && parsed.enum_types.iter().any(|t| enums.iter().any(|e| camel_case(&t.name, true) == e.name)) {
        preview.push_str("    note: PostgreSQL named enum types may need a provider-specific JDBC type mapping on the field; that is not verified or generated here.\n");
    }
    if let Some(note) = validation_note {
        preview.push_str(&format!("\nNote: {note}\n"));
    }
    if !refused.is_empty() {
        preview.push_str("\nRefused tables:\n");
        preview.extend(refused.iter().map(|r| format!("- {r}\n")));
    }
    if !parsed.skipped.is_empty() {
        preview.push_str(&format!("\nSkipped SQL ({}):\n", parsed.skipped.len() + parsed.skipped_overflow));
        preview.extend(parsed.skipped.iter().take(MAX_SKIPPED_SHOWN).map(|s| format!("- line {}: {}\n", s.line, s.reason)));
        if parsed.skipped.len() > MAX_SKIPPED_SHOWN {
            preview.push_str(&format!("- and {} more\n", parsed.skipped.len() + parsed.skipped_overflow - MAX_SKIPPED_SHOWN));
        }
    }
    preview.push_str("\nSources:\n");
    for (target, text) in &files {
        preview.push_str(&format!("\n// {target}\n{text}"));
    }
    preview.push_str("\nOne reviewed host edit creating new files only; no existing file is overwritten. @Column(name) is left out where the configured column naming template already yields the column name. Defaults and CHECK constraints are not mapped; indexes, schema, comments and enums only as chosen above.");
    if preview.len() > ui_kit::prompts::MAX_CONFIRM_BYTES {
        return Err("The complete preview exceeds the host's 16 KiB confirmation limit; pick fewer tables or columns.".into());
    }
    if !ui_kit::prompts::confirm_exact("Generate entities from SQL?", &preview)? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    for plan in &plans {
        no_collision(project, &pending.root, &directory, &plan.class, None)?;
    }
    for plan in &enums {
        no_collision(project, &pending.root, &directory, &plan.name, None)?;
    }
    Ok(FileAction {
        title: format!("Generate {} files from SQL", files.len()),
        kind: Some("refactor.rewrite".into()),
        operations: files.into_iter().map(|(path, text)| FileOperation::Create { path, text, overwrite: false }).collect(),
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const PG: &str = "-- shop\nCREATE TABLE customers (\n  id bigserial PRIMARY KEY,\n  email varchar(120) NOT NULL UNIQUE,\n  balance numeric(12,2),\n  created_at timestamptz NOT NULL,\n  avatar bytea,\n  active boolean,\n  token uuid\n);\nCREATE TABLE order_lines (\n  id serial PRIMARY KEY,\n  customer_id bigint NOT NULL REFERENCES customers (id),\n  parent_id integer REFERENCES order_lines (id),\n  product_name varchar(80) NOT NULL,\n  note text,\n  ref_no char(8),\n  registry_id integer REFERENCES registries (id),\n  placed date,\n  at_time timestamp,\n  hour time\n);\nCREATE INDEX idx_note ON order_lines (note);\nCREATE TABLE registries (id integer PRIMARY KEY);\nCREATE TABLE tag_links (a integer NOT NULL, b integer NOT NULL, PRIMARY KEY (a, b));\nCREATE TABLE docs (id integer PRIMARY KEY, body jsonb);\nCREATE TABLE nokey (x integer);\nCREATE TABLE \"Mixed\" (id integer PRIMARY KEY);\nCREATE TABLE \"column\" (id integer PRIMARY KEY);\nCREATE TABLE plain (id integer PRIMARY KEY, \"Odd Name\" text);\nCREATE VIEW v AS SELECT 1;\n";
    const MY: &str = "CREATE TABLE `users` (\n  `id` int NOT NULL AUTO_INCREMENT,\n  `name` varchar(50) NOT NULL,\n  `big` bigint unsigned,\n  `flag` bool,\n  `price` decimal(10,2) DEFAULT NULL,\n  `body` longtext,\n  PRIMARY KEY (`id`)\n) ENGINE=InnoDB;\nCREATE TABLE `big_ids` (`id` bigint unsigned NOT NULL AUTO_INCREMENT PRIMARY KEY);\nCREATE TABLE `posts` (\n  `id` int AUTO_INCREMENT PRIMARY KEY,\n  `user_id` int NOT NULL,\n  `kind` enum('a','b'),\n  `code` char(4) NOT NULL UNIQUE,\n  CONSTRAINT `fk_user` FOREIGN KEY (`user_id`) REFERENCES `users` (`id`)\n);\nCREATE TABLE `posts2` (`id` int PRIMARY KEY, `user_id` int, FOREIGN KEY (`user_id`) REFERENCES `users` (`id`));\n";

    fn build_for(sql: &str, dialect: SqlDialect, names: &[&str]) -> (ParsedDdl, Vec<Plan>, Vec<String>) {
        let parsed = ddl_parse::parse_ddl(sql, dialect).unwrap();
        let selected: Vec<usize> = names.iter().map(|n| parsed.snapshot.tables.iter().position(|t| t.name == *n).unwrap()).collect();
        let built = build_with(&parsed, dialect, &selected, &Options::default()).unwrap();
        (parsed, built.plans, built.refused)
    }
    fn source(plans: &[Plan], class: &str, kotlin: bool, ns: &str) -> String {
        let plan = plans.iter().find(|p| p.class == class).unwrap_or_else(|| panic!("{class} not planned"));
        let text = render(plan, "shop.db", ns, kotlin);
        let target = format!("/project/src/main/{}/shop/db/{class}.{}", if kotlin { "kotlin" } else { "java" }, if kotlin { "kt" } else { "java" });
        let parsed = parse_class(&text, &target).unwrap_or_else(|| panic!("does not parse:\n{text}"));
        assert_eq!((parsed.kind, parsed.name.as_str()), (Kind::Entity, class), "{text}");
        text
    }

    #[test]
    fn postgres_java_entity_with_identity_relations_and_only_non_default_column_arguments() {
        let _host = MockHost::install();
        let (_, plans, refused) = build_for(PG, SqlDialect::PostgreSql, &["customers", "order_lines"]);
        assert!(refused.is_empty(), "{refused:?}");
        let customers = source(&plans, "Customers", false, "jakarta.persistence");
        for expected in [
            "package shop.db;\n",
            "import jakarta.persistence.Column;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.GeneratedValue;\nimport jakarta.persistence.GenerationType;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.OneToMany;\nimport jakarta.persistence.Table;\nimport java.math.BigDecimal;\nimport java.time.OffsetDateTime;\nimport java.util.LinkedHashSet;\nimport java.util.Set;\nimport java.util.UUID;\n",
            "    @OneToMany(mappedBy = \"customer\")\n    private Set<OrderLines> orderLines = new LinkedHashSet<>();\n",
            "    public Set<OrderLines> getOrderLines() {\n        return orderLines;\n    }\n\n    public void setOrderLines(Set<OrderLines> orderLines) {\n        this.orderLines = orderLines;\n    }\n",
            "@Entity\n@Table(name = \"customers\")\npublic class Customers {\n",
            "    @Id\n    @GeneratedValue(strategy = GenerationType.IDENTITY)\n    private Long id;\n",
            "    @Column(nullable = false, unique = true, length = 120)\n    private String email;\n",
            "    @Column(precision = 12, scale = 2)\n    private BigDecimal balance;\n",
            "    @Column(nullable = false)\n    private OffsetDateTime createdAt;\n",
            "    private byte[] avatar;\n",
            "    private Boolean active;\n",
            "    private UUID token;\n",
            "    public Long getId() {\n        return id;\n    }\n\n    public void setId(Long id) {\n        this.id = id;\n    }\n",
            "public byte[] getAvatar()",
        ] {
            assert!(customers.contains(expected), "{expected}\n{customers}");
        }
        assert!(!customers.contains("@Column(name = \"email\""), "{customers}");
        let lines = source(&plans, "OrderLines", false, "jakarta.persistence");
        for expected in [
            "    @ManyToOne(fetch = FetchType.LAZY)\n    @JoinColumn(name = \"customer_id\", nullable = false)\n    private Customers customer;\n",
            "    @ManyToOne(fetch = FetchType.LAZY)\n    @JoinColumn(name = \"parent_id\")\n    private OrderLines parent;\n",
            "    @Column(nullable = false, length = 80)\n    private String productName;\n",
            "    @Column(length = 8)\n    private String refNo;\n",
            "    private Integer registryId;\n",
            "    private LocalDate placed;\n    private LocalDateTime atTime;\n".replace("    private LocalDateTime atTime;\n", "").as_str(),
            "    private LocalTime hour;\n",
            "public Customers getCustomer()",
        ] {
            assert!(lines.contains(expected), "{expected}\n{lines}");
        }
        assert!(lines.contains("    private LocalDateTime atTime;\n"), "{lines}");
        let notes = &plans.iter().find(|p| p.class == "OrderLines").unwrap().notes;
        assert!(notes.iter().any(|n| n.starts_with("registry_id: foreign key to registries stays a plain Integer")), "{notes:?}");
        assert!(notes.iter().any(|n| n.contains("1 index(es)")), "{notes:?}");
    }

    #[test]
    fn selecting_the_referenced_table_turns_the_column_into_a_relation() {
        let _host = MockHost::install();
        let (_, plans, _) = build_for(PG, SqlDialect::PostgreSql, &["order_lines", "registries"]);
        let lines = source(&plans, "OrderLines", false, "javax.persistence");
        assert!(lines.contains("private Registries registry;") && lines.contains("import javax.persistence.JoinColumn;"), "{lines}");
        assert!(!lines.contains("jakarta"));
        let (_, plans, _) = build_for(PG, SqlDialect::PostgreSql, &["order_lines"]);
        assert!(!source(&plans, "OrderLines", false, "jakarta.persistence").contains("customer;"), "unselected targets stay scalar");
    }

    #[test]
    fn postgres_kotlin_entity() {
        let _host = MockHost::install();
        let (_, plans, _) = build_for(PG, SqlDialect::PostgreSql, &["customers", "order_lines"]);
        let customers = source(&plans, "Customers", true, "jakarta.persistence");
        for expected in [
            "package shop.db\n\nimport jakarta.persistence.Column\n",
            "import java.math.BigDecimal\n",
            "@Entity\n@Table(name = \"customers\")\nclass Customers {\n",
            "    @Id\n    @GeneratedValue(strategy = GenerationType.IDENTITY)\n    var id: Long? = null\n",
            "    @Column(precision = 12, scale = 2)\n    var balance: BigDecimal? = null\n",
            "    var avatar: ByteArray? = null\n",
        ] {
            assert!(customers.contains(expected), "{expected}\n{customers}");
        }
        assert!(customers.ends_with("    var token: UUID? = null\n\n    @OneToMany(mappedBy = \"customer\")\n    var orderLines: MutableSet<OrderLines> = mutableSetOf()\n}\n"), "{customers}");
        let lines = source(&plans, "OrderLines", true, "jakarta.persistence");
        assert!(lines.contains("    @JoinColumn(name = \"customer_id\", nullable = false)\n    var customer: Customers? = null\n"), "{lines}");
    }

    #[test]
    fn postgres_refusals_name_the_reason() {
        let _host = MockHost::install();
        let (_, plans, refused) = build_for(
            PG,
            SqlDialect::PostgreSql,
            &["tag_links", "docs", "nokey", "Mixed", "column", "plain", "registries"],
        );
        let has = |needle: &str| refused.iter().any(|r| r.contains(needle));
        assert!(has("tag_links: composite primary key (a, b)"), "{refused:?}");
        assert!(has("docs: column body: unmapped SQL type 'jsonb'"), "{refused:?}");
        assert!(has("nokey: no primary key"), "{refused:?}");
        assert!(has("Mixed: the table name needs identifier quoting"), "{refused:?}");
        assert!(has("column: class name `Column`"), "{refused:?}");
        assert!(has("plain: column Odd Name: the column name needs identifier quoting"), "{refused:?}");
        assert_eq!(plans.iter().map(|p| p.class.as_str()).collect::<Vec<_>>(), ["Registries"]);
    }

    #[test]
    fn mysql_entities_and_refusals() {
        let _host = MockHost::install();
        let (_, plans, refused) = build_for(MY, SqlDialect::MySql, &["users", "big_ids", "posts", "posts2"]);
        let has = |needle: &str| refused.iter().any(|r| r.contains(needle));
        assert!(has("big_ids: column id: a generated key of SQL type Decimal"), "{refused:?}");
        assert!(has("posts: column kind: unmapped SQL type 'enum('a','b')'"), "{refused:?}");
        let users = source(&plans, "Users", false, "jakarta.persistence");
        for expected in [
            "    @Id\n    @GeneratedValue(strategy = GenerationType.IDENTITY)\n    private Integer id;\n",
            "    @Column(nullable = false, length = 50)\n    private String name;\n",
            "    @Column(precision = 20)\n    private BigDecimal big;\n",
            "    private Boolean flag;\n",
            "    @Column(precision = 10, scale = 2)\n    private BigDecimal price;\n",
            "    private String body;\n",
        ] {
            assert!(users.contains(expected), "{expected}\n{users}");
        }
        let posts2 = source(&plans, "Posts2", false, "jakarta.persistence");
        assert!(posts2.contains("@JoinColumn(name = \"user_id\")\n    private Users user;"), "{posts2}");
        assert!(!posts2.contains("@GeneratedValue"), "a key without AUTO_INCREMENT is assigned");
        let (_, plans, refused) = build_for(MY, SqlDialect::MySql, &["users", "posts"]);
        assert_eq!(refused.len(), 1);
        assert_eq!(plans.len(), 1);
    }

    #[test]
    fn unique_columns_and_not_null_foreign_keys_are_kept_on_the_relation() {
        let _host = MockHost::install();
        let sql = "CREATE TABLE a (id int PRIMARY KEY);\nCREATE TABLE b (id int PRIMARY KEY, a_id int NOT NULL UNIQUE REFERENCES a (id));";
        let (_, plans, _) = build_for(sql, SqlDialect::PostgreSql, &["a", "b"]);
        let b = source(&plans, "B", false, "jakarta.persistence");
        assert!(b.contains("@OneToOne(fetch = FetchType.LAZY)\n    @JoinColumn(name = \"a_id\", nullable = false)\n    private A a;"), "{b}");
        let a = source(&plans, "A", false, "jakarta.persistence");
        assert!(a.contains("    @OneToOne(mappedBy = \"a\")\n    private B b;\n"), "{a}");
    }

    #[test]
    fn duplicate_class_names_and_attribute_collisions_are_refused() {
        let _host = MockHost::install();
        let sql = "CREATE TABLE foo_bar (id int PRIMARY KEY);\nCREATE TABLE foo__bar (id int PRIMARY KEY);\nCREATE TABLE t (id int PRIMARY KEY, a_b int, a__b int, class int);";
        let (_, plans, refused) = build_for(sql, SqlDialect::MySql, &["foo_bar", "foo__bar", "t"]);
        assert_eq!(plans.len(), 1);
        assert!(refused.iter().any(|r| r.contains("produced by two tables")), "{refused:?}");
        assert!(refused.iter().any(|r| r.starts_with("t: column")), "{refused:?}");
    }

    const RELATIONS: &str = "CREATE TABLE authors (id bigserial PRIMARY KEY, name varchar(80) NOT NULL);\nCREATE TABLE books (id bigserial PRIMARY KEY, author_id bigint NOT NULL REFERENCES authors (id));\nCREATE TABLE tags (id bigserial PRIMARY KEY, label varchar(40));\nCREATE TABLE book_tags (book_id bigint NOT NULL REFERENCES books (id), tag_id bigint NOT NULL REFERENCES tags (id), PRIMARY KEY (book_id, tag_id));\nCREATE TABLE people (id bigserial PRIMARY KEY);\nCREATE TABLE passports (id bigserial PRIMARY KEY, person_id bigint NOT NULL UNIQUE REFERENCES people (id));\nCREATE TABLE users (id bigserial PRIMARY KEY, login varchar(40) NOT NULL);\nCREATE TABLE user_profiles (id bigint PRIMARY KEY REFERENCES users (id), bio text);\nCREATE TABLE lonely_links (a bigint NOT NULL REFERENCES authors (id), b bigint NOT NULL REFERENCES authors (id), PRIMARY KEY (a, b));\n";

    #[test]
    fn many_to_one_gets_an_initialised_inverse_collection_on_the_referenced_entity() {
        let _host = MockHost::install();
        let result = built(RELATIONS, SqlDialect::PostgreSql, &["authors", "books"], &Options::default());
        assert!(result.refused.is_empty(), "{:?}", result.refused);
        let authors = checked(&result.plans[0], false, "jakarta.persistence");
        for expected in [
            "import jakarta.persistence.OneToMany;\n",
            "import java.util.LinkedHashSet;\nimport java.util.Set;\n",
            "    @OneToMany(mappedBy = \"author\")\n    private Set<Books> books = new LinkedHashSet<>();\n",
            "    public Set<Books> getBooks() {\n        return books;\n    }\n",
        ] {
            assert!(authors.contains(expected), "{expected}\n{authors}");
        }
        let kotlin = checked(&result.plans[0], true, "jakarta.persistence");
        assert!(kotlin.contains("    @OneToMany(mappedBy = \"author\")\n    var books: MutableSet<Books> = mutableSetOf()\n"), "{kotlin}");
        assert!(!kotlin.contains("java.util"), "{kotlin}");
        let books = checked(&result.plans[1], false, "jakarta.persistence");
        assert!(books.contains("@ManyToOne(fetch = FetchType.LAZY)\n    @JoinColumn(name = \"author_id\", nullable = false)\n    private Authors author;"), "{books}");
        let alone = built(RELATIONS, SqlDialect::PostgreSql, &["books"], &Options::default());
        assert!(!checked(&alone.plans[0], false, "jakarta.persistence").contains("OneToMany"), "no inverse without the referenced table");
    }

    #[test]
    fn a_unique_foreign_key_is_a_one_to_one_with_the_inverse_on_the_other_entity() {
        let _host = MockHost::install();
        let result = built(RELATIONS, SqlDialect::PostgreSql, &["people", "passports"], &Options::default());
        let passports = checked(&result.plans[1], false, "jakarta.persistence");
        assert!(passports.contains("    @OneToOne(fetch = FetchType.LAZY)\n    @JoinColumn(name = \"person_id\", nullable = false)\n    private People person;\n"), "{passports}");
        let people = checked(&result.plans[0], false, "jakarta.persistence");
        assert!(people.contains("    @OneToOne(mappedBy = \"person\")\n    private Passports passports;\n"), "{people}");
        let kotlin = checked(&result.plans[0], true, "jakarta.persistence");
        assert!(kotlin.contains("@OneToOne(mappedBy = \"person\")\n    var passports: Passports? = null"), "{kotlin}");
        let eager = built(RELATIONS, SqlDialect::PostgreSql, &["people", "passports"], &Options { default_fetch: true, ..Options::default() });
        assert!(checked(&eager.plans[1], false, "jakarta.persistence").contains("    @OneToOne\n    @JoinColumn(name = \"person_id\", nullable = false)\n"));
    }

    #[test]
    fn a_junction_table_between_two_selected_tables_is_a_many_to_many_and_not_an_entity() {
        let _host = MockHost::install();
        let result = built(RELATIONS, SqlDialect::PostgreSql, &["books", "tags", "book_tags"], &Options::default());
        assert!(result.refused.is_empty(), "{:?}", result.refused);
        assert_eq!(result.plans.iter().map(|p| p.class.as_str()).collect::<Vec<_>>(), ["Books", "Tags"]);
        let books = checked(&result.plans[0], false, "jakarta.persistence");
        assert!(
            books.contains("    @ManyToMany\n    @JoinTable(name = \"book_tags\", joinColumns = {@JoinColumn(name = \"book_id\")}, inverseJoinColumns = {@JoinColumn(name = \"tag_id\")})\n    private Set<Tags> tags = new LinkedHashSet<>();\n"),
            "{books}"
        );
        let tags = checked(&result.plans[1], false, "jakarta.persistence");
        assert!(tags.contains("    @ManyToMany(mappedBy = \"tags\")\n    private Set<Books> books = new LinkedHashSet<>();\n"), "{tags}");
        let kotlin = checked(&result.plans[0], true, "jakarta.persistence");
        assert!(kotlin.contains("@JoinTable(name = \"book_tags\", joinColumns = [JoinColumn(name = \"book_id\")], inverseJoinColumns = [JoinColumn(name = \"tag_id\")])\n    var tags: MutableSet<Tags> = mutableSetOf()"), "{kotlin}");
        assert!(books.contains("@ManyToOne") == false && result.plans[0].notes.iter().any(|n| n.contains("junction table book_tags is mapped as @ManyToMany")), "{:?}", result.plans[0].notes);
        let missing = built(RELATIONS, SqlDialect::PostgreSql, &["books", "book_tags"], &Options::default());
        assert!(missing.refused.iter().any(|r| r.contains("book_tags: composite primary key (book_id, tag_id)")), "{:?}", missing.refused);
        let same_end = built(RELATIONS, SqlDialect::PostgreSql, &["authors", "lonely_links"], &Options::default());
        assert!(same_end.refused.iter().any(|r| r.contains("lonely_links: composite primary key")), "both ends the same table stays refused: {:?}", same_end.refused);
    }

    #[test]
    fn a_shared_key_chosen_as_a_normal_entity_maps_the_parent_with_maps_id() {
        let _host = MockHost::install();
        let parsed = ddl_parse::parse_ddl(RELATIONS, SqlDialect::PostgreSql).unwrap();
        let position = |name: &str| parsed.snapshot.tables.iter().position(|t| t.name == name).unwrap();
        let (users, profiles) = (position("users"), position("user_profiles"));
        let options = Options { maps_id: HashMap::from([(profiles, users)]), ..Options::default() };
        let result = build_with(&parsed, SqlDialect::PostgreSql, &[users, profiles], &options).unwrap();
        let text = checked(&result.plans[1], false, "jakarta.persistence");
        for expected in [
            "    @Id\n    private Long id;\n",
            "    @OneToOne(fetch = FetchType.LAZY, optional = false)\n    @MapsId\n    @JoinColumn(name = \"id\", nullable = false)\n    private Users users;\n",
            "import jakarta.persistence.MapsId;\n",
            "public Users getUsers()",
        ] {
            assert!(text.contains(expected), "{expected}\n{text}");
        }
        assert!(checked(&result.plans[1], true, "javax.persistence").contains("@MapsId\n    @JoinColumn(name = \"id\", nullable = false)\n    var users: Users? = null\n"));
        let plain = build_with(&parsed, SqlDialect::PostgreSql, &[users, profiles], &Options::default()).unwrap();
        assert!(!checked(&plain.plans[1], false, "jakarta.persistence").contains("MapsId"));
        let alone = build_with(&parsed, SqlDialect::PostgreSql, &[profiles], &options).unwrap();
        assert!(!checked(&alone.plans[0], false, "jakarta.persistence").contains("MapsId"), "the parent is not generated");
    }

    #[test]
    fn a_column_named_like_a_keyword_gets_the_field_suffix_and_keeps_its_column_name() {
        let _host = MockHost::install();
        let result = built("CREATE TABLE t_words (id bigserial PRIMARY KEY, class varchar(20), object_id int, \"order\" int);", SqlDialect::PostgreSql, &["t_words"], &Options::default());
        assert!(result.refused.is_empty(), "{:?}", result.refused);
        let java = checked(&result.plans[0], false, "jakarta.persistence");
        assert!(java.contains("    @Column(name = \"class\", length = 20)\n    private String classField;\n") || java.contains("    @Column(name = \"\\\"class\\\"\", length = 20)\n    private String classField;\n"), "{java}");
        assert!(java.contains("private Integer objectId;") && java.contains("@Column(name = \"\\\"order\\\"\")\n    private Integer order;"), "{java}");
        assert!(result.plans[0].notes.iter().any(|n| n.contains("`class` is a reserved word")), "{:?}", result.plans[0].notes);
    }

    #[test]
    fn prefix_and_suffix_lists_strip_the_first_matching_entry() {
        let list = |items: &[&str]| items.iter().map(|i| (*i).to_owned()).collect::<Vec<_>>();
        let prefixes = list(&["tbl_", "old_"]);
        assert_eq!(strip_affixes("tbl_alpha", &prefixes, &[]), "alpha");
        assert_eq!(strip_affixes("OLD_beta", &prefixes, &[]), "beta");
        assert_eq!(strip_affixes("gamma", &prefixes, &[]), "gamma");
        assert_eq!(strip_affixes("tbl_old_x", &prefixes, &[]), "old_x");
        assert_eq!(strip_affixes("users_tbl", &[], &list(&["_old", "_tbl"])), "users");
        assert_eq!(strip_affixes("tbl_users_old", &prefixes, &list(&["_old"])), "users");
        assert_eq!(strip_affixes("_tbl", &[], &list(&["_tbl"])), "_tbl");
        let options = Options { table_prefixes: prefixes, table_suffixes: list(&["_tbl"]), singular: true, ..Options::default() };
        let parsed = ddl_parse::parse_ddl("CREATE TABLE old_boxes_tbl (id int PRIMARY KEY);", SqlDialect::PostgreSql).unwrap();
        assert_eq!(default_class(&parsed.snapshot.tables[0], &options), "Box");
    }

    #[test]
    fn affix_list_prompt_rejects_empty_entries_and_bad_characters() {
        let (host, _, _) = setup(PG);
        for (answer, valid) in [("tbl_,old_", true), ("tbl_", true), ("tbl_,,old_", false), ("tbl_, ", false), ("tbl-", false), ("", false)] {
            host.answer_next_prompt(Some(&[answer]));
            assert_eq!(prompt_affixes("Prefixes").is_ok(), valid, "{answer:?}");
        }
    }

    #[test]
    fn comments_can_be_copied_as_the_hibernate_comment_annotation_as_well() {
        let _host = MockHost::install();
        let parsed = ddl_parse::parse_ddl(PG_OPTIONS, SqlDialect::PostgreSql).unwrap();
        let ti = parsed.snapshot.tables.iter().position(|t| t.name == "tbl_orders").unwrap();
        let keep = BTreeSet::from(["note".to_owned()]);
        let only_annotation = Options { comment_annotation: true, columns: HashMap::from([(ti, keep.clone())]), ..Options::default() };
        let planned = plan(&parsed, ti, SqlDialect::PostgreSql, &HashMap::new(), &only_annotation).unwrap();
        let java = checked(&planned, false, "jakarta.persistence");
        assert!(java.contains("    @org.hibernate.annotations.Comment(\"Free note\")\n    private String note;\n"), "{java}");
        assert!(java.contains("@org.hibernate.annotations.Comment(\"Orders */ and /* x \\\\ <b> @tag\")\n@Entity\n"), "{java}");
        assert!(!java.contains("/** "), "Javadoc not chosen: {java}");
        let both = Options { comments: true, comment_annotation: true, kotlin: true, columns: HashMap::from([(ti, keep)]), ..Options::default() };
        let kotlin = checked(&plan(&parsed, ti, SqlDialect::PostgreSql, &HashMap::new(), &both).unwrap(), true, "jakarta.persistence");
        assert!(kotlin.contains("    /** Free note */\n    @org.hibernate.annotations.Comment(\"Free note\")\n    var note: String? = null\n"), "{kotlin}");
        assert_eq!(string_text("a \"q\" \\ $x\n y", true).as_deref(), Some("a \\\"q\\\" \\\\ \\$x y"));
        assert_eq!(string_text("a $x", false).as_deref(), Some("a $x"));
        assert_eq!(string_text("  ", false), None);
        assert_eq!(string_text(&"x".repeat(241), false), None);
    }

    #[test]
    fn validation_annotations_follow_the_column_metadata_in_the_chosen_namespace() {
        let _host = MockHost::install();
        for package in ["jakarta.validation.constraints", "javax.validation.constraints"] {
            let options = Options { validation: Some(package), ..Options::default() };
            let result = built(PG, SqlDialect::PostgreSql, &["customers"], &options);
            let java = checked(&result.plans[0], false, "jakarta.persistence");
            assert!(java.contains(&format!("import {package}.NotNull;\nimport {package}.Size;\n")) || java.contains(&format!("import {package}.NotNull;")), "{java}");
            assert!(java.contains("    @NotNull\n    @Size(max = 120)\n    @Column(nullable = false, unique = true, length = 120)\n    private String email;\n"), "{java}");
            assert!(java.contains("    @NotNull\n    @Column(nullable = false)\n    private OffsetDateTime createdAt;\n"), "{java}");
            assert!(java.contains("    @Id\n    @GeneratedValue(strategy = GenerationType.IDENTITY)\n    private Long id;\n") && !java.contains("@NotNull\n    @Id"), "keys carry no constraint: {java}");
            assert!(!java.contains("@Size(max = 255)") && java.contains("    private byte[] avatar;"), "{java}");
        }
        let off = built(PG, SqlDialect::PostgreSql, &["customers"], &Options::default());
        assert!(!checked(&off.plans[0], false, "jakarta.persistence").contains("NotNull"));
        let clash = Options { validation: Some("jakarta.validation.constraints"), classes: HashMap::from([(0, "Size".to_owned())]), ..Options::default() };
        assert!(taken("Size", &clash) && taken("NotNull", &clash) && !taken("Size", &Options::default()));
    }

    #[test]
    fn uuid_generation_applies_to_uuid_keys_only_and_numeric_keys_keep_identity() {
        let _host = MockHost::install();
        let sql = "CREATE TABLE docs (id uuid PRIMARY KEY, title text);\nCREATE TABLE items (id bigserial PRIMARY KEY);";
        let uuid = Options { id_generation: IdGeneration::Uuid, ..Options::default() };
        let result = built(sql, SqlDialect::PostgreSql, &["docs", "items"], &uuid);
        let docs = checked(&result.plans[0], false, "jakarta.persistence");
        assert!(docs.contains("    @Id\n    @GeneratedValue(strategy = GenerationType.UUID)\n    private UUID id;\n"), "{docs}");
        assert!(checked(&result.plans[1], false, "jakarta.persistence").contains("GenerationType.IDENTITY"));
        let identity = built(sql, SqlDialect::PostgreSql, &["docs"], &Options::default());
        assert!(!checked(&identity.plans[0], false, "jakarta.persistence").contains("GeneratedValue"));
    }

    #[test]
    fn uuid_generation_needs_jakarta_and_a_literal_boot_3_1_or_hibernate_6_2() {
        let pom = |version: &str| format!("<parent><artifactId>spring-boot-starter-parent</artifactId><version>{version}</version></parent>\n<dependency><artifactId>spring-boot-starter-data-jpa</artifactId></dependency>");
        assert!(uuid_generation_proof("jakarta.persistence", &pom("3.3.5")).is_ok());
        assert!(uuid_generation_proof("jakarta.persistence", &pom("3.1.0")).is_ok());
        for refused in [pom("3.0.9"), pom("2.7.18"), "no build".to_owned(), String::new()] {
            assert!(uuid_generation_proof("jakarta.persistence", &refused).is_err(), "{refused}");
        }
        assert!(format!("{:?}", uuid_generation_proof("javax.persistence", &pom("3.3.5"))).contains("javax.persistence has no GenerationType.UUID"));
        let gradle = |hibernate: &str| format!("implementation(\"org.hibernate.orm:hibernate-core:{hibernate}\")\nid(\"org.springframework.boot\") version \"3.3.5\"");
        assert!(uuid_generation_proof("jakarta.persistence", &gradle("6.2.7.Final")).is_ok());
        assert!(uuid_generation_proof("jakarta.persistence", &gradle("6.1.7.Final")).is_err());
        assert_eq!(version_pair(" \"3.3.5\""), Some((3, 3)));
        assert_eq!(version_pair("6.2.7.Final"), Some((6, 2)));
        assert_eq!(version_pair("6"), None);
    }

    #[test]
    fn kotlin_is_proven_absent_only_without_literals_and_without_kotlin_sources() {
        let facts_of = |host: &MockHost| crate::project_facts::detect(&host.invocation().project().unwrap(), "/project");
        let absent = |host: &MockHost| kotlin_proven_absent(&host.invocation().project().unwrap(), "/project", &facts_of(host));
        let pom = "<project><parent><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-parent</artifactId><version>3.3.5</version></parent></project>";
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("pom.xml", pom);
        host.add_file("src/main/java/shop/A.java", "package shop;\nclass A {}\n");
        assert_eq!(facts_of(&host).kotlin, crate::project_facts::Fact::Unknown);
        assert!(absent(&host), "a Maven parent alone does not keep Kotlin Unknown");
        host.add_file("src/test/kotlin/shop/T.kt", "package shop\nclass T\n");
        assert!(!absent(&host), "a .kt source under the module");
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("pom.xml", &pom.replace("</project>", "<build><plugins><plugin><artifactId>kotlin-maven-plugin</artifactId></plugin></plugins></build></project>"));
        assert!(!absent(&host), "a Kotlin plugin literal");
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("pom.xml", &pom.replace("</project>", "<kotlin.version>1.9</kotlin.version></project>"));
        assert!(!absent(&host), "any Kotlin mention keeps it Unknown");
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("pom.xml", &pom.replace("</project>", "<dependencies><dependency><artifactId>x</artifactId><scope>import</scope></dependency></dependencies></project>"));
        host.add_file("build.gradle.kts", "dependencies { implementation(libs.foo) }");
        assert!(!absent(&host), "a version catalog could hide the plugin");
        let host = MockHost::install().with_capabilities(&["project.read"]);
        assert!(!absent(&host), "no build text at all");
    }

    #[test]
    fn gradle_kotlin_plugin_declared_apply_false_at_the_root_does_not_make_a_java_module_kotlin() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("build.gradle.kts", "plugins {\n    kotlin(\"jvm\") version \"2.0.21\" apply false\n}\nallprojects { group = \"x\" }\n");
        host.add_file("settings.gradle.kts", "include(\"entities\", \"app\")");
        host.add_file("entities/build.gradle.kts", "plugins { `java-library` }\n");
        host.add_file("entities/src/main/java/x/E.java", "package x;\nclass E {}\n");
        host.add_file("app/src/main/kotlin/x/App.kt", "package x\nclass App\n");
        let project = host.invocation().project().unwrap();
        let facts = crate::project_facts::detect(&project, "/project/entities");
        assert_eq!(facts.kotlin, crate::project_facts::Fact::Unknown);
        assert!(kotlin_proven_absent(&project, "/project", &facts));
        let app = crate::project_facts::detect(&project, "/project/app");
        assert_eq!(app.kotlin, crate::project_facts::Fact::Yes);
    }

    const PG_OPTIONS: &str = "CREATE TYPE mood AS ENUM ('sad', 'happy');\nCREATE TYPE bad_mood AS ENUM ('not ok', 'x');\nCREATE TYPE dup AS ENUM ('a', 'a');\nCREATE TABLE tbl_orders (\n  id bigserial PRIMARY KEY,\n  col_name varchar(40) NOT NULL,\n  \"order\" integer,\n  mood mood NOT NULL,\n  note text,\n  customer_id integer\n);\nCOMMENT ON TABLE tbl_orders IS 'Orders */ and /* x \\ <b> @tag';\nCOMMENT ON COLUMN tbl_orders.note IS 'Free note';\nCOMMENT ON COLUMN tbl_orders.col_name IS '';\nCREATE INDEX idx_name ON tbl_orders (col_name, note);\nCREATE INDEX idx_desc ON tbl_orders (note DESC);\nCREATE INDEX idx_order ON tbl_orders (\"order\");\nCREATE UNIQUE INDEX uq_name_note ON tbl_orders (col_name, note);\nCREATE TABLE public.categories (id int PRIMARY KEY, label text);\nCREATE TABLE bad_enum (id int PRIMARY KEY, m bad_mood);\nCREATE TABLE dup_enum (id int PRIMARY KEY, m dup);\nCREATE TABLE statuses (id int PRIMARY KEY, kind mood);\n";
    const MY_OPTIONS: &str = "CREATE TABLE `tbl_boxes` (\n  `id` int NOT NULL AUTO_INCREMENT COMMENT 'Key',\n  `size` enum('small','large') NOT NULL,\n  `tags` set('a','b'),\n  `rank` int COMMENT 'the rank',\n  `code` varchar(10) NOT NULL,\n  `label` varchar(10),\n  PRIMARY KEY (`id`),\n  KEY `idx_code` (`code`, `label`),\n  UNIQUE KEY `uq_code_label` (`code`, `label`)\n) ENGINE=InnoDB COMMENT='Boxes';\nCREATE TABLE `shop`.`bus` (`id` int PRIMARY KEY);\nCREATE TABLE `categories` (`id` int PRIMARY KEY, `tag` enum('x','b y'));\n";

    fn everything() -> Options {
        Options { schema: true, indexes: true, comments: true, enums: true, ..Options::default() }
    }
    fn built(sql: &str, dialect: SqlDialect, names: &[&str], options: &Options) -> Built3 {
        let parsed = ddl_parse::parse_ddl(sql, dialect).unwrap();
        let selected: Vec<usize> = names.iter().map(|n| parsed.snapshot.tables.iter().position(|t| t.name == *n).unwrap()).collect();
        build_with(&parsed, dialect, &selected, options).unwrap()
    }
    fn checked(plan: &Plan, kotlin: bool, ns: &str) -> String {
        let text = render(plan, "shop.db", ns, kotlin);
        let target = format!("/project/src/main/{}/shop/db/{}.{}", if kotlin { "kotlin" } else { "java" }, plan.class, if kotlin { "kt" } else { "java" });
        let parsed = parse_class(&text, &target).unwrap_or_else(|| panic!("does not parse:\n{text}"));
        assert_eq!(parsed.name, plan.class, "{text}");
        text
    }

    #[test]
    fn singular_rules_are_the_documented_small_set() {
        for (word, expected) in [
            ("categories", "category"),
            ("addresses", "address"),
            ("buses", "bus"),
            ("classes", "class"),
            ("boxes", "box"),
            ("branches", "branch"),
            ("dishes", "dish"),
            ("orders", "order"),
            ("status", "status"),
            ("address", "address"),
            ("analysis", "analysis"),
            ("people", "people"),
            ("s", "s"),
            ("user_orders", "user_order"),
        ] {
            assert_eq!(singularize(word), expected, "{word}");
        }
        assert_eq!(strip_prefix("TBL_orders", "tbl_"), "orders");
        assert_eq!(strip_prefix("tbl_", "tbl_"), "tbl_");
        assert_eq!(strip_prefix("other", "tbl_"), "other");
    }

    #[test]
    fn column_selection_keeps_the_key_and_omits_the_rest() {
        let _host = MockHost::install();
        let parsed = ddl_parse::parse_ddl(PG_OPTIONS, SqlDialect::PostgreSql).unwrap();
        let ti = parsed.snapshot.tables.iter().position(|t| t.name == "tbl_orders").unwrap();
        let options = Options { columns: HashMap::from([(ti, BTreeSet::from(["note".to_owned()]))]), ..Options::default() };
        let planned = plan(&parsed, ti, SqlDialect::PostgreSql, &HashMap::new(), &options).unwrap();
        assert_eq!(planned.attrs.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["id", "note"]);
        assert!(planned.notes.iter().any(|n| n == "columns left out by selection: col_name, order, mood, customer_id"), "{:?}", planned.notes);
        let all = plan(&parsed, ti, SqlDialect::PostgreSql, &HashMap::new(), &Options::default());
        assert!(all.unwrap_err().contains("tbl_orders: column mood: unmapped SQL type 'mood'"), "unselected-all keeps the old refusal");
    }

    #[test]
    fn indexes_and_unique_constraints_per_namespace_and_language() {
        let _host = MockHost::install();
        let options = Options { indexes: true, ..Options::default() };
        let result = built(PG_OPTIONS, SqlDialect::PostgreSql, &["tbl_orders"], &Options { columns: HashMap::new(), ..options.clone() });
        assert!(result.plans.is_empty(), "mood column is still refused without the enum option");
        let parsed = ddl_parse::parse_ddl(PG_OPTIONS, SqlDialect::PostgreSql).unwrap();
        let ti = parsed.snapshot.tables.iter().position(|t| t.name == "tbl_orders").unwrap();
        let keep = BTreeSet::from(["col_name", "note", "order"].map(str::to_owned));
        let options = Options { columns: HashMap::from([(ti, keep)]), ..options };
        let planned = plan(&parsed, ti, SqlDialect::PostgreSql, &HashMap::new(), &options).unwrap();
        for ns in ["jakarta.persistence", "javax.persistence"] {
            let java = checked(&planned, false, ns);
            assert!(java.contains(&format!("import {ns}.Index;\nimport {ns}.Table;\nimport {ns}.UniqueConstraint;\n")), "{java}");
            assert!(
                java.contains("@Table(\n    name = \"tbl_orders\",\n    indexes = {@Index(name = \"idx_name\", columnList = \"col_name, note\")},\n    uniqueConstraints = {@UniqueConstraint(name = \"uq_name_note\", columnNames = {\"col_name\", \"note\"})}\n)\npublic class TblOrders {"),
                "{java}"
            );
            let kotlin = checked(&planned, true, ns);
            assert!(
                kotlin.contains("    indexes = [Index(name = \"idx_name\", columnList = \"col_name, note\")],\n    uniqueConstraints = [UniqueConstraint(name = \"uq_name_note\", columnNames = [\"col_name\", \"note\"])]\n)\nclass TblOrders {"),
                "{kotlin}"
            );
        }
        let notes = planned.notes.join("\n");
        assert!(notes.contains("index idx_desc is not mapped (DESC or operator-class ordering"), "{notes}");
        assert!(notes.contains("index idx_order is not mapped (column order is a delimited identifier)"), "{notes}");
        let off = plan(&parsed, ti, SqlDialect::PostgreSql, &HashMap::new(), &Options { columns: options.columns.clone(), ..Options::default() }).unwrap();
        assert!(off.indexes.is_empty() && off.notes.iter().any(|n| n.contains("3 index(es)") && n.contains("1 composite unique")), "{:?}", off.notes);
    }

    #[test]
    fn mysql_indexes_comments_and_enums() {
        let _host = MockHost::install();
        let result = built(MY_OPTIONS, SqlDialect::MySql, &["tbl_boxes", "categories", "shop.bus"], &everything());
        let refused = result.refused.join("\n");
        assert!(refused.contains("tbl_boxes: column tags: unmapped SQL type 'set('a','b')'"), "SET is never mapped: {refused}");
        assert!(refused.contains("categories: column tag: enumeration value 'b y' is not a usable constant name"), "{refused}");
        let parsed = ddl_parse::parse_ddl(MY_OPTIONS, SqlDialect::MySql).unwrap();
        let ti = parsed.snapshot.tables.iter().position(|t| t.name == "tbl_boxes").unwrap();
        let keep = BTreeSet::from(["size", "rank", "code", "label"].map(str::to_owned));
        let options = Options { columns: HashMap::from([(ti, keep)]), ..everything() };
        let result = build_with(&parsed, SqlDialect::MySql, &[ti], &options).unwrap();
        assert_eq!(result.enums, [EnumPlan { name: "TblBoxesSize".into(), values: vec!["small".into(), "large".into()] }]);
        let java = checked(&result.plans[0], false, "jakarta.persistence");
        for expected in [
            "/** Boxes */\n@Entity\n@Table(\n    name = \"tbl_boxes\",\n    indexes = {@Index(name = \"idx_code\", columnList = \"code, label\")},\n    uniqueConstraints = {@UniqueConstraint(name = \"uq_code_label\", columnNames = {\"code\", \"label\"})}\n)\n",
            "    /** Key */\n    @Id\n    @GeneratedValue(strategy = GenerationType.IDENTITY)\n    private Integer id;\n",
            "    @Enumerated(EnumType.STRING)\n    @Column(nullable = false)\n    private TblBoxesSize size;\n",
            "    /** the rank */\n    @Column(name = \"`rank`\")\n".replace('`', "\\\"").as_str(),
            "public TblBoxesSize getSize()",
        ] {
            assert!(java.contains(expected), "{expected}\n{java}");
        }
        assert_eq!(render_enum(&result.enums[0], "shop.db", false), "package shop.db;\n\npublic enum TblBoxesSize {\n    small,\n    large\n}\n");
        assert_eq!(render_enum(&result.enums[0], "shop.db", true), "package shop.db\n\nenum class TblBoxesSize {\n    small,\n    large\n}\n");
        let kotlin = checked(&result.plans[0], true, "javax.persistence");
        assert!(kotlin.contains("    @Enumerated(EnumType.STRING)\n    @Column(nullable = false)\n    var size: TblBoxesSize? = null\n"), "{kotlin}");
        assert!(!java.contains("ORDINAL"));
        let bus = built(MY_OPTIONS, SqlDialect::MySql, &["shop.bus"], &Options::default()).plans;
        assert!(bus[0].notes.is_empty() && bus[0].schema.is_none() || bus[0].notes.iter().any(|n| n.contains("MySQL database qualifier")), "{:?}", bus[0].notes);
    }

    #[test]
    fn enums_postgres_shared_types_refusals_and_the_declined_option() {
        let _host = MockHost::install();
        let declined = built(PG_OPTIONS, SqlDialect::PostgreSql, &["statuses", "bad_enum", "dup_enum"], &Options::default());
        assert!(declined.plans.is_empty() && declined.refused.iter().all(|r| r.contains("choose the enum option")), "{:?}", declined.refused);
        assert!(declined.refused[0].contains("statuses: column kind: unmapped SQL type 'mood'"), "{:?}", declined.refused);
        let result = built(PG_OPTIONS, SqlDialect::PostgreSql, &["statuses", "bad_enum", "dup_enum"], &everything());
        let refused = result.refused.join("\n");
        assert!(refused.contains("bad_enum: column m: enumeration value 'not ok' is not a usable constant name"), "{refused}");
        assert!(refused.contains("dup_enum: column m: enumeration value 'a' is listed twice"), "{refused}");
        assert_eq!(result.enums, [EnumPlan { name: "Mood".into(), values: vec!["sad".into(), "happy".into()] }]);
        let text = checked(&result.plans[0], false, "jakarta.persistence");
        assert!(text.contains("    @Enumerated(EnumType.STRING)\n    private Mood kind;\n"), "{text}");
        let parsed = ddl_parse::parse_ddl("CREATE TYPE entity AS ENUM ('a'); CREATE TABLE t (id int PRIMARY KEY, c entity); CREATE TABLE u (id int PRIMARY KEY, c entity);", SqlDialect::PostgreSql).unwrap();
        let (plans, refused) = {
            let built = build_with(&parsed, SqlDialect::PostgreSql, &[0, 1], &everything()).unwrap();
            (built.plans, built.refused)
        };
        assert!(plans.is_empty() && refused[0].contains("enum name `Entity` is not usable"), "{refused:?}");
    }

    #[test]
    fn schema_prefixes_singular_names_comments_and_reserved_words() {
        let _host = MockHost::install();
        let result = built(PG_OPTIONS, SqlDialect::PostgreSql, &["public.categories"], &Options::default());
        assert!(result.plans[0].schema.is_none() && result.plans[0].notes.iter().any(|n| n == "schema `public` is not written to @Table (schema option not chosen)"), "{:?}", result.plans[0].notes);
        let with = built(PG_OPTIONS, SqlDialect::PostgreSql, &["public.categories"], &Options { schema: true, singular: true, ..Options::default() });
        let java = checked(&with.plans[0], false, "jakarta.persistence");
        assert!(java.contains("@Table(name = \"categories\", schema = \"public\")\npublic class Category {"), "{java}");

        let parsed = ddl_parse::parse_ddl(PG_OPTIONS, SqlDialect::PostgreSql).unwrap();
        let ti = parsed.snapshot.tables.iter().position(|t| t.name == "tbl_orders").unwrap();
        let keep = BTreeSet::from(["col_name", "order", "note"].map(str::to_owned));
        let options = Options {
            comments: true,
            singular: true,
            table_prefixes: vec!["tbl_".into()],
            column_prefixes: vec!["col_".into()],
            columns: HashMap::from([(ti, keep)]),
            ..Options::default()
        };
        let planned = plan(&parsed, ti, SqlDialect::PostgreSql, &HashMap::new(), &options).unwrap();
        let java = checked(&planned, false, "jakarta.persistence");
        for expected in [
            "/** Orders *&#47; and /&#42; x &#92; &lt;b&gt; &#64;tag */\n@Entity\n@Table(name = \"tbl_orders\")\npublic class Order {",
            "    @Column(name = \"col_name\", nullable = false, length = 40)\n    private String name;\n",
            "    @Column(name = \"\\\"order\\\"\")\n    private Integer order;\n",
            "    /** Free note */\n    private String note;\n",
        ] {
            assert!(java.contains(expected), "{expected}\n{java}");
        }
        assert!(!java.contains("*/ and"), "{java}");
        let kotlin = checked(&planned, true, "jakarta.persistence");
        assert!(kotlin.contains("    @Column(name = \"\\\"order\\\"\")\n    var order: Int? = null\n"), "{kotlin}");

        let renamed = Options { classes: HashMap::from([(ti, "Purchase".to_owned())]), ..options.clone() };
        assert_eq!(plan(&parsed, ti, SqlDialect::PostgreSql, &HashMap::new(), &renamed).unwrap().class, "Purchase");
        let collide = Options { table_prefixes: vec!["tbl_".into()], ..Options::default() };
        assert!(plan(&parsed, ti, SqlDialect::PostgreSql, &HashMap::new(), &collide).unwrap_err().contains("mood"), "other refusals stay");
        let sql = "CREATE TABLE t (id int PRIMARY KEY, col_a int, a int);";
        let parsed = ddl_parse::parse_ddl(sql, SqlDialect::PostgreSql).unwrap();
        let stripped = Options { column_prefixes: vec!["col_".into()], ..Options::default() };
        assert!(plan(&parsed, 0, SqlDialect::PostgreSql, &HashMap::new(), &stripped).unwrap_err().contains("attribute name `a` is produced by two columns"));
        let long = "x".repeat(MAX_COMMENT_CHARS + 1);
        assert!(doc_text(&long).is_none() && doc_text("  \n ").is_none());
        assert_eq!(doc_text("a\nb\u{1}c\\u000a */").as_deref(), Some("a b c&#92;u000a *&#47;"));
        assert_eq!(doc_text("*/*").as_deref(), Some("*&#47;&#42;"));
    }

    #[test]
    fn options_flow_creates_entities_and_enums_in_one_action() {
        let (host, mut plugin, doc) = setup(PG_OPTIONS);
        project(&host, "jakarta.persistence");
        let action = offered(&mut plugin, &doc);
        host.answer_next_prompt(Some(&["tbl_orders", "statuses"]));
        host.answer_next_prompt(Some(&["java"]));
        host.answer_next_prompt(Some(&["shop.db"]));
        host.answer_next_prompt(Some(&["indexes", "comments", "enums", "singular", "tablePrefix", "columnPrefix"]));
        host.answer_next_prompt(Some(&["tbl_"]));
        host.answer_next_prompt(Some(&["col_"]));
        host.answer_next_prompt(Some(&["Purchase"]));
        host.answer_next_prompt(Some(&["Status"]));
        host.answer_next_prompt(Some(&["col_name", "note", "mood", "order"]));
        host.answer_next_prompt(Some(&[]));
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&doc, action).unwrap();
        let created: Vec<_> = result
            .operations
            .iter()
            .map(|op| match op {
                FileOperation::Create { path, text, overwrite } => {
                    assert!(!overwrite);
                    (path.rsplit('/').next().unwrap().to_owned(), text.clone())
                }
                _ => panic!("only creates"),
            })
            .collect();
        assert_eq!(created.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), ["Purchase.java", "Status.java", "Mood.java"]);
        assert!(created[0].1.contains("private Mood mood;") && created[0].1.contains("@Index(name = \"idx_name\", columnList = \"col_name, note\")"), "{}", created[0].1);
        assert!(created[2].1.contains("public enum Mood {\n    sad,\n    happy\n}"));
        assert_eq!(result.title, "Generate 3 files from SQL");
    }

    #[test]
    fn invalid_prompts_and_too_many_files_refuse_without_creating() {
        let (host, mut plugin, doc) = setup(PG_OPTIONS);
        project(&host, "jakarta.persistence");
        let action = offered(&mut plugin, &doc);
        for answer in [&["statuses"][..], &["java"], &["shop.db"], &["tablePrefix"], &["bad prefix"]] {
            host.answer_next_prompt(Some(answer));
        }
        assert!(format!("{:?}", plugin.resolve_file_action(&doc, action).unwrap_err()).contains("Choose a comma-separated list"));
        let action = offered(&mut plugin, &doc);
        for answer in [&["statuses"][..], &["java"], &["shop.db"], &["singular"], &["not a class"]] {
            host.answer_next_prompt(Some(answer));
        }
        assert!(format!("{:?}", plugin.resolve_file_action(&doc, action).unwrap_err()).contains("not a valid class name"));

        let many: String = (0..20).map(|i| format!("CREATE TABLE t{i} (id integer PRIMARY KEY, m mood{i});\nCREATE TYPE mood{i} AS ENUM ('a');\n")).collect();
        let (host, mut plugin, doc) = setup(&many);
        project(&host, "jakarta.persistence");
        let action = offered(&mut plugin, &doc);
        let names: Vec<String> = (0..20).map(|i| format!("t{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        host.answer_next_prompt(Some(&["postgresql"]));
        host.answer_next_prompt(Some(&refs));
        host.answer_next_prompt(Some(&["java"]));
        host.answer_next_prompt(Some(&["shop.db"]));
        host.answer_next_prompt(Some(&["enums"]));
        for _ in 0..20 {
            host.answer_next_prompt(Some(&[]));
        }
        let message = format!("{:?}", plugin.resolve_file_action(&doc, action).unwrap_err());
        assert!(message.contains("exceed the limit of 20") && message.contains("pick fewer tables or columns"), "{message}");
    }

    fn project(host: &MockHost, namespace: &str) {
        host.add_file("src/main/java/shop/Existing.java", &format!("package shop;\nimport {namespace}.Entity;\n@Entity\nclass Existing {{}}\n"));
        host.add_file("src/main/java/shop/db/Other.java", "package shop.db;\nclass Other {}\n");
        host.add_file("src/main/kotlin/shop/K.kt", &format!("package shop\nimport {namespace}.Entity\n@Entity class K\n"));
    }
    const SQL_PATH: &str = "/project/db/schema.sql";
    fn setup(sql: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(SQL_PATH, "sql", sql, &[]);
        let doc = Document { path: SQL_PATH.into(), language: "sql".into(), text: sql.into(), version: 1 };
        (host, JpaAssist::default(), doc)
    }
    fn offered(plugin: &mut JpaAssist, doc: &Document) -> FileAction {
        plugin.file_actions(doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap()
    }

    #[test]
    fn offered_only_for_sql_files_that_create_tables() {
        let (_host, mut plugin, doc) = setup(PG);
        assert_eq!(plugin.file_actions(&doc, Range::default()).len(), 1);
        let none = Document { text: "SELECT 1;".into(), ..doc.clone() };
        assert!(plugin.file_actions(&none, Range::default()).is_empty());
        let big = Document { text: format!("{PG}-- {}\n", "x".repeat(MAX_SQL)), ..doc.clone() };
        assert!(plugin.file_actions(&big, Range::default()).is_empty());
        let outside = Document { path: "/elsewhere/schema.sql".into(), ..doc.clone() };
        assert!(plugin.file_actions(&outside, Range::default()).is_empty());
        let mut java = ddl_entities_action_doc("/project/src/main/java/shop/A.java");
        java.text = "create table x".into();
        assert!(actions(&mut Generation::default(), &java, Range::default()).is_none());
    }
    fn ddl_entities_action_doc(path: &str) -> Document {
        Document { path: path.into(), language: "java".into(), text: String::new(), version: 1 }
    }

    #[test]
    fn resolve_creates_all_files_in_one_action_without_overwrite() {
        let (host, mut plugin, doc) = setup(PG);
        project(&host, "jakarta.persistence");
        let action = offered(&mut plugin, &doc);
        host.answer_next_prompt(Some(&["customers", "order_lines"]));
        host.answer_next_prompt(Some(&["java"]));
        host.answer_next_prompt(Some(&["shop.db"]));
        for _ in 0..3 {
            host.answer_next_prompt(Some(&[]));
        }
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&doc, action).unwrap();
        assert!(result.edits.is_empty());
        let paths: Vec<_> = result
            .operations
            .iter()
            .map(|op| match op {
                FileOperation::Create { path, overwrite, .. } => {
                    assert!(!overwrite);
                    path.as_str()
                }
                _ => panic!("only creates"),
            })
            .collect();
        assert_eq!(paths, ["/project/src/main/java/shop/db/Customers.java", "/project/src/main/java/shop/db/OrderLines.java"]);
        assert_eq!(host.editor_text(), PG);
    }

    fn language_run(host: &MockHost, plugin: &mut JpaAssist, doc: &Document, answers: &[&str]) -> Result<FileAction, Error> {
        crate::project_facts::invalidate_all();
        let action = offered(plugin, doc);
        host.answer_next_prompt(Some(&["customers"]));
        for answer in answers {
            host.answer_next_prompt(Some(&[answer]));
        }
        host.answer_next_prompt(Some(&["shop.db"]));
        for _ in 0..3 {
            host.answer_next_prompt(Some(&[]));
        }
        host.answer_next_prompt(Some(&[]));
        plugin.resolve_file_action(doc, action)
    }
    fn first_path(result: Result<FileAction, Error>) -> String {
        let FileOperation::Create { path, .. } = &result.unwrap().operations[0] else { panic!() };
        path.clone()
    }
    const JAVA_BUILD: &str = "plugins { id 'java' }\n";
    const KOTLIN_BUILD: &str = "plugins { id 'org.jetbrains.kotlin.jvm' version '2.0.0' }\n";

    #[test]
    fn language_is_not_asked_when_kotlin_is_not_detected() {
        let (host, mut plugin, doc) = setup(PG);
        host.add_file("src/main/java/shop/Existing.java", "package shop;\nimport jakarta.persistence.Entity;\n@Entity\nclass Existing {}\n");
        host.add_file("src/main/java/shop/db/Other.java", "package shop.db;\nclass Other {}\n");
        host.add_file("build.gradle", JAVA_BUILD);
        assert_eq!(first_path(language_run(&host, &mut plugin, &doc, &[])), "/project/src/main/java/shop/db/Customers.java");
    }

    #[test]
    fn language_is_not_asked_for_a_java_only_maven_module_with_a_parent_and_is_asked_once_a_kotlin_source_exists() {
        let pom = "<project><parent><artifactId>spring-boot-starter-parent</artifactId><version>3.3.5</version></parent></project>";
        let (host, mut plugin, doc) = setup(PG);
        host.add_file("pom.xml", pom);
        host.add_file("src/main/java/shop/Existing.java", "package shop;\nimport jakarta.persistence.Entity;\n@Entity\nclass Existing {}\n");
        host.add_file("src/main/java/shop/db/Other.java", "package shop.db;\nclass Other {}\n");
        assert_eq!(first_path(language_run(&host, &mut plugin, &doc, &[])), "/project/src/main/java/shop/db/Customers.java");
        let (host, mut plugin, doc) = setup(PG);
        host.add_file("pom.xml", pom);
        host.add_file("src/main/java/shop/Existing.java", "package shop;\nimport jakarta.persistence.Entity;\n@Entity\nclass Existing {}\n");
        host.add_file("src/main/java/shop/db/Other.java", "package shop.db;\nclass Other {}\n");
        host.add_file("src/test/kotlin/shop/T.kt", "package shop\nclass T\n");
        assert_eq!(first_path(language_run(&host, &mut plugin, &doc, &["java"])), "/project/src/main/java/shop/db/Customers.java");
    }

    #[test]
    fn language_is_not_asked_for_kotlin_only_modules_under_auto_and_is_asked_otherwise() {
        let (host, mut plugin, doc) = setup(PG);
        host.add_file("src/main/kotlin/shop/K.kt", "package shop\nimport jakarta.persistence.Entity\n@Entity class K\n");
        host.add_file("src/main/kotlin/shop/db/Other.kt", "package shop.db\nclass Other\n");
        host.add_dir("src/main/kotlin");
        host.add_file("build.gradle", KOTLIN_BUILD);
        host.set_setting("entityLanguage", "\"auto\"");
        assert_eq!(first_path(language_run(&host, &mut plugin, &doc, &[])), "/project/src/main/kotlin/shop/db/Customers.kt");

        let (host, mut plugin, doc) = setup(PG);
        host.add_file("src/main/kotlin/shop/K.kt", "package shop\nimport jakarta.persistence.Entity\n@Entity class K\n");
        host.add_file("src/main/kotlin/shop/db/Other.kt", "package shop.db\nclass Other\n");
        host.add_file("build.gradle", KOTLIN_BUILD);
        assert_eq!(first_path(language_run(&host, &mut plugin, &doc, &["kotlin"])), "/project/src/main/kotlin/shop/db/Customers.kt", "default java setting still asks");

        let (host, mut plugin, doc) = setup(PG);
        project(&host, "jakarta.persistence");
        host.add_file("build.gradle", KOTLIN_BUILD);
        host.set_setting("entityLanguage", "\"auto\"");
        assert_eq!(first_path(language_run(&host, &mut plugin, &doc, &["java"])), "/project/src/main/java/shop/db/Customers.java", "both languages ask");

        let (host, mut plugin, doc) = setup(PG);
        host.add_file("src/main/java/shop/Existing.java", "package shop;\nimport jakarta.persistence.Entity;\n@Entity\nclass Existing {}\n");
        host.add_file("src/main/java/shop/db/Other.java", "package shop.db;\nclass Other {}\n");
        assert_eq!(first_path(language_run(&host, &mut plugin, &doc, &["java"])), "/project/src/main/java/shop/db/Customers.java", "unknown kotlin asks");
    }

    #[test]
    fn cancellation_collision_ambiguous_roots_and_drift_create_nothing() {
        let (host, mut plugin, doc) = setup(PG);
        project(&host, "jakarta.persistence");
        let action = offered(&mut plugin, &doc);
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, action), Err(Error::Cancelled)));

        let action = offered(&mut plugin, &doc);
        for answer in [&["customers"][..], &["java"], &["shop.db"]] {
            host.answer_next_prompt(Some(answer));
        }
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, action), Err(Error::Cancelled)));

        host.add_file("src/main/java/shop/db/Customers.java", "package shop.db;\nclass Customers {}\n");
        let action = offered(&mut plugin, &doc);
        for answer in [&["customers"][..], &["java"], &["shop.db"], &[], &[]] {
            host.answer_next_prompt(Some(answer));
        }
        assert!(format!("{:?}", plugin.resolve_file_action(&doc, action).unwrap_err()).contains("already exists"));

        host.add_file("other/src/main/java/x/Y.java", "package x;\nclass Y {}\n");
        let action = offered(&mut plugin, &doc);
        for answer in [&["registries"][..], &["java"]] {
            host.answer_next_prompt(Some(answer));
        }
        assert!(format!("{:?}", plugin.resolve_file_action(&doc, action).unwrap_err()).contains("ambiguous"));

        let action = offered(&mut plugin, &doc);
        host.type_in_editor("-- changed");
        assert!(plugin.resolve_file_action(&doc, action).is_err());
    }

    #[test]
    fn every_selected_table_refused_creates_nothing_and_names_reasons() {
        let (host, mut plugin, doc) = setup(PG);
        project(&host, "jakarta.persistence");
        let action = offered(&mut plugin, &doc);
        for answer in [&["docs", "nokey"][..], &["java"], &["shop.db"], &[], &[]] {
            host.answer_next_prompt(Some(answer));
        }
        let message = format!("{:?}", plugin.resolve_file_action(&doc, action).unwrap_err());
        assert!(message.contains("jsonb") && message.contains("no primary key"), "{message}");
    }

    #[test]
    fn too_many_tables_and_oversized_previews_are_refused() {
        let many: String = (0..21).map(|i| format!("CREATE TABLE t{i} (id integer PRIMARY KEY);\n")).collect();
        let (host, mut plugin, doc) = setup(&many);
        project(&host, "jakarta.persistence");
        let action = offered(&mut plugin, &doc);
        let names: Vec<String> = (0..21).map(|i| format!("t{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        host.answer_next_prompt(Some(&["postgresql"]));
        host.answer_next_prompt(Some(&refs));
        let message = format!("{:?}", plugin.resolve_file_action(&doc, action).unwrap_err());
        assert!(message.contains("between one and 20"), "{message}");

        let wide: String = (0..4)
            .map(|t| {
                let columns: String = (0..60).map(|c| format!("  column_number_{c} varchar(40) NOT NULL,\n")).collect();
                format!("CREATE TABLE wide{t} (\n  id integer PRIMARY KEY,\n{columns}  last_one text\n);\n")
            })
            .collect();
        let (host, mut plugin, doc) = setup(&wide);
        project(&host, "jakarta.persistence");
        let action = offered(&mut plugin, &doc);
        for answer in [&["postgresql"][..], &["wide0", "wide1", "wide2", "wide3"], &["java"], &["shop.db"], &[], &[], &[], &[], &[]] {
            host.answer_next_prompt(Some(answer));
        }
        assert!(format!("{:?}", plugin.resolve_file_action(&doc, action).unwrap_err()).contains("pick fewer tables"));
    }

    #[test]
    fn unknown_namespace_is_prompted_and_mysql_is_prompted_when_syntax_is_ambiguous() {
        let sql = "CREATE TABLE t (id int PRIMARY KEY, name varchar(10));";
        let (host, mut plugin, doc) = setup(sql);
        host.add_file("src/main/java/shop/Existing.java", "package shop;\nclass Existing {}\n");
        let action = offered(&mut plugin, &doc);
        for answer in [&["mysql"][..], &["t"], &["java"], &["shop"], &["javax"], &[], &[], &[]] {
            host.answer_next_prompt(Some(answer));
        }
        let result = plugin.resolve_file_action(&doc, action).unwrap();
        let [FileOperation::Create { path, text, .. }] = result.operations.as_slice() else { panic!("one create") };
        assert_eq!(path, "/project/src/main/java/shop/T.java");
        assert!(text.contains("import javax.persistence.Entity;"), "{text}");
    }

    const EXTRA: &str = "CREATE TABLE accounts (id serial PRIMARY KEY, email varchar(80) NOT NULL, meta jsonb, shape geometry(Point,4326), odd text[]);\nCREATE VIEW account_emails AS SELECT id, email FROM accounts;\nCREATE MATERIALIZED VIEW account_mat (account_id, mail) AS SELECT id, email FROM accounts;\nCREATE TABLE people (id serial PRIMARY KEY, name varchar(50) NOT NULL);\nCREATE TABLE employees (id integer PRIMARY KEY REFERENCES people (id), salary integer);\nCREATE TABLE managers (id integer PRIMARY KEY REFERENCES employees (id), level integer);\nCREATE TABLE clashes (id integer PRIMARY KEY REFERENCES people (id), name text);\nCREATE TABLE orders2 (id integer PRIMARY KEY, person_id integer NOT NULL REFERENCES people (id));\n";
    const HIBERNATE_POM: &str = "<dependency><groupId>org.hibernate.orm</groupId><artifactId>hibernate-core</artifactId><version>6.5.3.Final</version></dependency>";

    fn extra() -> (ParsedDdl, BTreeSet<usize>) {
        let mut parsed = ddl_parse::parse_ddl(EXTRA, SqlDialect::PostgreSql).unwrap();
        let views = append_views(&mut parsed);
        (parsed, views)
    }
    fn index(parsed: &ParsedDdl, name: &str) -> usize {
        parsed.snapshot.tables.iter().position(|t| t.name == name).unwrap_or_else(|| panic!("{name}"))
    }
    fn build_extra(parsed: &ParsedDdl, names: &[&str], options: &Options) -> Built3 {
        let selected: Vec<usize> = names.iter().map(|n| index(parsed, n)).collect();
        build_with(parsed, SqlDialect::PostgreSql, &selected, options).unwrap()
    }

    #[test]
    fn views_are_immutable_entities_with_a_chosen_id_and_need_hibernate() {
        let _host = MockHost::install();
        let (mut parsed, views) = extra();
        assert_eq!(views.len(), 2);
        let mat = index(&parsed, "account_mat");
        parsed.snapshot.tables[mat].primary_key = Fact::Known(vec!["mail".into()]);
        let options = Options { views, ..Options::default() };
        let Built3 { plans, refused, .. } = build_extra(&parsed, &["account_emails", "account_mat"], &options);
        assert!(refused.is_empty(), "{refused:?}");
        let java = checked(&plans[0], false, "jakarta.persistence");
        for expected in ["import org.hibernate.annotations.Immutable;\n", "@Entity\n@Immutable\n@Table(name = \"account_emails\")\npublic class AccountEmails {", "    @Id\n    private Integer id;\n", "    @Column(length = 80)\n    private String email;\n"] {
            assert!(java.contains(expected), "{expected}\n{java}");
        }
        assert!(!java.contains("GeneratedValue"), "{java}");
        let kotlin = checked(&plans[1], true, "javax.persistence");
        assert!(kotlin.contains("import org.hibernate.annotations.Immutable\n") && kotlin.contains("@Entity\n@Immutable\n@Table(name = \"account_mat\")\nclass AccountMat protected constructor() {"), "{kotlin}");
        assert!(kotlin.contains("    var mail: String? = null\n        private set\n"), "{kotlin}");
        assert!(java.contains("    protected AccountEmails() {\n    }\n") && java.contains("public Integer getId()") && java.contains("public String getEmail()"), "{java}");
        assert!(!java.contains("void set") && !java.contains("public AccountEmails("), "getters only, one protected constructor: {java}");
        let blocked = Options { view_block: Some("Hibernate is not proven".into()), ..options };
        let Built3 { plans, refused, .. } = build_extra(&parsed, &["account_emails", "accounts"], &blocked);
        assert_eq!(plans.len(), 0, "accounts has jsonb");
        assert!(refused.iter().any(|r| r.starts_with("account_emails: Hibernate is not proven")), "{refused:?}");
    }

    #[test]
    fn views_flow_asks_for_the_id_and_refuses_without_hibernate_or_with_a_composite_id() {
        let (host, mut plugin, doc) = setup(EXTRA);
        project(&host, "jakarta.persistence");
        host.add_file("pom.xml", HIBERNATE_POM);
        let action = offered(&mut plugin, &doc);
        for answer in [&["postgresql"][..], &["account_emails"], &["shop.db"], &["email"], &[], &[], &[]] {
            host.answer_next_prompt(Some(answer));
        }
        let result = plugin.resolve_file_action(&doc, action).unwrap();
        let [FileOperation::Create { text, .. }] = result.operations.as_slice() else { panic!("one create") };
        assert!(text.contains("@Immutable") && text.contains("    @Id\n    @Column(length = 80)\n    private String email;"), "{text}");

        let action = offered(&mut plugin, &doc);
        for answer in [&["postgresql"][..], &["account_emails"], &["shop.db"], &["id", "email"]] {
            host.answer_next_prompt(Some(answer));
        }
        assert!(format!("{:?}", plugin.resolve_file_action(&doc, action).unwrap_err()).contains("exactly one id column"));

        let (host, mut plugin, doc) = setup(EXTRA);
        project(&host, "jakarta.persistence");
        let action = offered(&mut plugin, &doc);
        for answer in [&["postgresql"][..], &["account_emails"], &["shop.db"], &[], &[]] {
            host.answer_next_prompt(Some(answer));
        }
        let message = format!("{:?}", plugin.resolve_file_action(&doc, action).unwrap_err());
        assert!(message.contains("Every selected table was refused") && message.contains("Hibernate"), "{message}");
    }

    #[test]
    fn shared_primary_keys_map_as_joined_inheritance_with_a_fallback_on_attribute_clashes() {
        let _host = MockHost::install();
        let (parsed, _) = extra();
        let (people, employees, managers) = (index(&parsed, "people"), index(&parsed, "employees"), index(&parsed, "managers"));
        let selected = [people, employees, managers, index(&parsed, "clashes"), index(&parsed, "orders2")];
        assert_eq!(shared_key_parents(&parsed, SqlDialect::PostgreSql, &selected, employees, &Options::default()), [people]);
        assert_eq!(shared_key_parents(&parsed, SqlDialect::PostgreSql, &selected, managers, &Options::default()), [employees]);
        assert!(shared_key_parents(&parsed, SqlDialect::PostgreSql, &selected, index(&parsed, "orders2"), &Options::default()).is_empty());
        let options = Options { parents: HashMap::from([(employees, people), (managers, employees), (index(&parsed, "clashes"), people)]), ..Options::default() };
        let Built3 { plans, refused, .. } = build_extra(&parsed, &["people", "employees", "managers", "clashes", "orders2"], &options);
        assert!(refused.is_empty(), "{refused:?}");
        let root = checked(&plans[0], false, "jakarta.persistence");
        assert!(root.contains("@Entity\n@Table(name = \"people\")\n@Inheritance(strategy = InheritanceType.JOINED)\npublic class People {") && root.contains("import jakarta.persistence.InheritanceType;"), "{root}");
        let middle = checked(&plans[1], false, "jakarta.persistence");
        assert!(middle.contains("@Table(name = \"employees\")\n@PrimaryKeyJoinColumn(name = \"id\")\npublic class Employees extends People {"), "{middle}");
        assert!(!middle.contains("@Id") && !middle.contains("getId") && !middle.contains("@Inheritance"), "{middle}");
        let leaf = checked(&plans[2], false, "jakarta.persistence");
        assert!(leaf.contains("public class Managers extends Employees {") && leaf.contains("import jakarta.persistence.PrimaryKeyJoinColumn;"), "{leaf}");
        let clash = checked(&plans[3], false, "jakarta.persistence");
        assert!(clash.contains("public class Clashes {") && clash.contains("    @Id\n    private Integer id;"), "{clash}");
        assert!(plans[3].notes.iter().any(|n| n.contains("JOINED inheritance from People not applied") && n.contains("`name`")), "{:?}", plans[3].notes);
        let kotlin = checked(&plans[1], true, "javax.persistence");
        assert!(kotlin.contains("open class Employees : People() {"), "{kotlin}");
        assert!(checked(&plans[0], true, "javax.persistence").contains("open class People {"));
        assert!(checked(&plans[2], true, "javax.persistence").contains("\nclass Managers : Employees() {"));
        assert!(!plans[4].parent.is_some() && !plans[4].open);
    }

    #[test]
    fn joined_flow_prompts_only_for_shared_keys_and_never_creates_a_cycle() {
        let (host, mut plugin, doc) = setup(EXTRA);
        project(&host, "jakarta.persistence");
        let action = offered(&mut plugin, &doc);
        for answer in [&["postgresql"][..], &["people", "employees"], &["java"], &["shop.db"], &[], &[], &[], &["people"], &[]] {
            host.answer_next_prompt(Some(answer));
        }
        let result = plugin.resolve_file_action(&doc, action).unwrap();
        let texts: Vec<_> = result.operations.iter().map(|op| if let FileOperation::Create { text, .. } = op { text.clone() } else { panic!() }).collect();
        assert!(texts[0].contains("@Inheritance(strategy = InheritanceType.JOINED)") && texts[1].contains("extends People"), "{texts:?}");

        let action = offered(&mut plugin, &doc);
        for answer in [&["postgresql"][..], &["people", "employees"], &["java"], &["shop.db"], &[], &[], &[], &["\u{0}none"], &[]] {
            host.answer_next_prompt(Some(answer));
        }
        let result = plugin.resolve_file_action(&doc, action).unwrap();
        let FileOperation::Create { text, .. } = &result.operations[1] else { panic!() };
        assert!(!text.contains("extends") && text.contains("@Id"), "{text}");

        let cyclic = "CREATE TABLE a (id integer PRIMARY KEY REFERENCES b (id));\nCREATE TABLE b (id integer PRIMARY KEY REFERENCES a (id));\n";
        let parsed = ddl_parse::parse_ddl(cyclic, SqlDialect::PostgreSql).unwrap();
        let mut options = Options::default();
        options.parents.insert(0, 1);
        assert!(shared_key_parents(&parsed, SqlDialect::PostgreSql, &[0, 1], 1, &options).iter().all(|p| leads_to(&options, *p, 1)));
    }

    #[test]
    fn unmapped_types_are_skipped_or_mapped_as_string_with_a_safe_column_definition() {
        let _host = MockHost::install();
        let (parsed, _) = extra();
        let accounts = index(&parsed, "accounts");
        let refused = build_extra(&parsed, &["accounts"], &Options::default());
        assert!(refused.plans.is_empty() && refused.refused[0].contains("unmapped SQL type"), "{:?}", refused.refused);
        let options = Options {
            deferred: HashMap::from([((accounts, "meta".to_owned()), Deferred::Definition("jsonb".into())), ((accounts, "shape".to_owned()), Deferred::Definition("geometry(Point,4326)".into())), ((accounts, "odd".to_owned()), Deferred::Skip)]),
            ..Options::default()
        };
        let Built3 { plans, .. } = build_extra(&parsed, &["accounts"], &options);
        let java = checked(&plans[0], false, "jakarta.persistence");
        assert!(java.contains("    @Column(columnDefinition = \"jsonb\")\n    private String meta;\n") && java.contains("@Column(columnDefinition = \"geometry(Point,4326)\")\n    private String shape;"), "{java}");
        assert!(!java.contains("\n    private Object odd") && java.contains("    // TODO: column odd has an unmapped SQL type; choose a Java type and uncomment\n    // @Column(name = \"odd\")\n    // private Object odd;\n") && plans[0].deferred == ["odd (text[])"], "{:?}\n{java}", plans[0].deferred);
        let kotlin = checked(&plans[0], true, "jakarta.persistence");
        assert!(kotlin.contains("@Column(columnDefinition = \"jsonb\")\n    var meta: String? = null"), "{kotlin}");
        assert!(safe_definition("numeric(10, 2)") && !safe_definition("text[]") && !safe_definition("a\"b") && !safe_definition("public.mood") && !safe_definition(""));
        let key = Options { deferred: HashMap::from([((accounts, "id".to_owned()), Deferred::Skip)]), ..Options::default() };
        assert!(build_extra(&parsed, &["accounts"], &key).plans.is_empty());
    }

    #[test]
    fn deferred_flow_asks_per_column_and_unsafe_type_text_stays_refused() {
        let (host, mut plugin, doc) = setup(EXTRA);
        project(&host, "jakarta.persistence");
        let action = offered(&mut plugin, &doc);
        for answer in [&["postgresql"][..], &["accounts"], &["java"], &["shop.db"], &["deferred"], &["email", "meta", "shape"], &["skip"], &["string"], &[]] {
            host.answer_next_prompt(Some(answer));
        }
        let result = plugin.resolve_file_action(&doc, action).unwrap();
        let FileOperation::Create { text, .. } = &result.operations[0] else { panic!() };
        assert!(!text.contains("private String meta;") && text.contains("    // TODO: column meta has the unmapped SQL type jsonb; choose a Java type and uncomment\n    // @Column(name = \"meta\")\n    // private Object meta;\n"), "{text}");
        assert!(text.contains("@Column(columnDefinition = \"geometry(Point,4326)\")\n    private String shape;"), "{text}");

        let action = offered(&mut plugin, &doc);
        for answer in [&["postgresql"][..], &["accounts"], &["java"], &["shop.db"], &["deferred"], &[], &["skip"], &["skip"]] {
            host.answer_next_prompt(Some(answer));
        }
        host.answer_next_prompt(Some(&[]));
        let message = format!("{:?}", plugin.resolve_file_action(&doc, action).unwrap_err());
        assert!(message.contains("text[]"), "{message}");
    }

    #[test]
    fn generation_defaults_keep_todays_output_and_each_setting_changes_only_its_part() {
        let host = MockHost::install();
        let parsed = ddl_parse::parse_ddl(PG, SqlDialect::PostgreSql).unwrap();
        let both = |options: &Options| {
            let selected: Vec<usize> = ["customers", "order_lines"].iter().map(|n| parsed.snapshot.tables.iter().position(|t| t.name == *n).unwrap()).collect();
            let plans = build_with(&parsed, SqlDialect::PostgreSql, &selected, options).unwrap().plans;
            plans.iter().map(|p| checked(p, false, "jakarta.persistence")).collect::<Vec<_>>()
        };
        let mut defaults = Options::default();
        import_defaults(&mut defaults).unwrap();
        assert_eq!((defaults.id_generation, defaults.default_fetch, defaults.never_required), (IdGeneration::Identity, false, false));
        let today = both(&defaults);
        assert!(today[0].contains("@GeneratedValue(strategy = GenerationType.IDENTITY)") && today[1].contains("@ManyToOne(fetch = FetchType.LAZY)\n    @JoinColumn(name = \"customer_id\", nullable = false)"));
        host.set_setting("sqlImportIdGeneration", "\"identity\"");
        host.set_setting("sqlImportFetch", "\"lazy\"");
        host.set_setting("sqlImportOptional", "\"nullability\"");
        let mut explicit = Options::default();
        import_defaults(&mut explicit).unwrap();
        assert_eq!(both(&explicit), today);

        for (value, expected) in [("sequence", "GenerationType.SEQUENCE"), ("auto", "GenerationType.AUTO")] {
            host.set_setting("sqlImportIdGeneration", &format!("\"{value}\""));
            let mut options = Options::default();
            import_defaults(&mut options).unwrap();
            let text = both(&options);
            assert!(text[0].contains(&format!("@GeneratedValue(strategy = {expected})")) && !text[0].contains("IDENTITY"), "{}", text[0]);
            assert_eq!(text[1].replace(expected, "GenerationType.IDENTITY"), today[1]);
        }
        host.set_setting("sqlImportIdGeneration", "\"uuid\"");
        let mut uuid = Options::default();
        import_defaults(&mut uuid).unwrap();
        assert_eq!(uuid.id_generation, IdGeneration::Uuid);
        host.set_setting("sqlImportIdGeneration", "\"random\"");
        let message = format!("{:?}", import_defaults(&mut Options::default()).unwrap_err());
        assert!(message.contains("identity, sequence, auto, uuid or none"), "{message}");
        host.set_setting("sqlImportIdGeneration", "\"none\"");
        let mut options = Options::default();
        import_defaults(&mut options).unwrap();
        let text = both(&options);
        assert!(!text[0].contains("GeneratedValue") && text[0].contains("    @Id\n    private Long id;"), "{}", text[0]);
        host.set_setting("sqlImportFetch", "\"default\"");
        host.set_setting("sqlImportOptional", "\"never\"");
        let mut options = Options::default();
        import_defaults(&mut options).unwrap();
        let text = both(&options);
        assert!(text[1].contains("    @ManyToOne\n    @JoinColumn(name = \"customer_id\")\n") && !text[1].contains("FetchType"), "{}", text[1]);
        host.set_setting("sqlImportFetch", "\"eager\"");
        assert!(format!("{:?}", import_defaults(&mut Options::default()).unwrap_err()).contains("sqlImportFetch"));
        host.set_setting("sqlImportFetch", "\"lazy\"");
        host.set_setting("sqlImportIdGeneration", "\"sequence\"");
        let mut options = Options::default();
        import_defaults(&mut options).unwrap();
        let my = ddl_parse::parse_ddl(MY, SqlDialect::MySql).unwrap();
        let users = my.snapshot.tables.iter().position(|t| t.name == "users").unwrap();
        let built = build_with(&my, SqlDialect::MySql, &[users], &options).unwrap();
        assert!(built.plans.is_empty() && built.refused[0].contains("not offered for MySQL"), "{:?}", built.refused);
    }

    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_API / JPA_JAVAX_API jar paths (and Kotlin env vars)"]
    fn generated_entities_compile_for_java_kotlin_and_both_namespaces() {
        let _host = MockHost::install();
        let base = std::env::temp_dir().join(format!("jpa-ddl-entities-{}", std::process::id()));
        let option_pg = Options { singular: true, table_prefixes: vec!["tbl_".into()], column_prefixes: vec!["col_".into()], comment_annotation: true, ..everything() };
        let boxes_columns = {
            let parsed = ddl_parse::parse_ddl(MY_OPTIONS, SqlDialect::MySql).unwrap();
            let ti = parsed.snapshot.tables.iter().position(|t| t.name == "tbl_boxes").unwrap();
            Options { columns: HashMap::from([(ti, BTreeSet::from(["size", "rank", "code", "label"].map(str::to_owned)))]), comment_annotation: true, ..everything() }
        };
        for (sql, dialect, names, options) in [
            (PG, SqlDialect::PostgreSql, &["customers", "order_lines", "registries"][..], Options::default()),
            (MY, SqlDialect::MySql, &["users", "posts2"][..], Options::default()),
            (PG_OPTIONS, SqlDialect::PostgreSql, &["tbl_orders", "statuses", "public.categories"][..], option_pg),
            (MY_OPTIONS, SqlDialect::MySql, &["tbl_boxes"][..], boxes_columns),
            (EXTRA, SqlDialect::PostgreSql, &["account_emails", "account_mat", "accounts", "people", "employees", "managers", "clashes", "orders2"][..], Options::default()),
            (RELATIONS, SqlDialect::PostgreSql, &["authors", "books", "tags", "book_tags", "people", "passports", "users", "user_profiles"][..], Options::default()),
            (PG, SqlDialect::PostgreSql, &["customers", "order_lines"][..], Options { validation: Some("validation"), ..Options::default() }),
        ] {
            let mut parsed = ddl_parse::parse_ddl(sql, dialect).unwrap();
            let views = append_views(&mut parsed);
            let mut options = options;
            if sql == EXTRA {
                let mat = index(&parsed, "account_mat");
                parsed.snapshot.tables[mat].primary_key = Fact::Known(vec!["mail".into()]);
                let at = |name: &str| index(&parsed, name);
                let accounts = at("accounts");
                options = Options {
                    views,
                    parents: HashMap::from([(at("employees"), at("people")), (at("managers"), at("employees")), (at("clashes"), at("people"))]),
                    deferred: HashMap::from([((accounts, "meta".to_owned()), Deferred::Definition("jsonb".into())), ((accounts, "shape".to_owned()), Deferred::Definition("geometry(Point,4326)".into())), ((accounts, "odd".to_owned()), Deferred::Skip)]),
                    ..Options::default()
                };
            }
            if sql == RELATIONS {
                let at = |name: &str| index(&parsed, name);
                options.maps_id = HashMap::from([(at("user_profiles"), at("users"))]);
            }
            let selected: Vec<usize> = names.iter().map(|n| parsed.snapshot.tables.iter().position(|t| t.name == *n).unwrap()).collect();
            let validated = options.validation.is_some();
            for kotlin in [false, true] {
                for ns in ["jakarta.persistence", "javax.persistence"] {
                    let validation = if ns == "jakarta.persistence" { "jakarta.validation.constraints" } else { "javax.validation.constraints" };
                    let for_ns = Options { validation: options.validation.map(|_| validation), ..options.clone() };
                    let Built3 { plans, refused, enums } = build_with(&parsed, dialect, &selected, &for_ns).unwrap();
                    assert!(refused.is_empty(), "{refused:?}");
                    let directory = base.join(format!("{dialect:?}-{kotlin}-{ns}"));
                    std::fs::create_dir_all(&directory).unwrap();
                    let api = std::env::var(if ns == "jakarta.persistence" { "JPA_JAKARTA_API" } else { "JPA_JAVAX_API" }).expect("explicit persistence API jar");
                    let hibernate = std::env::var("HIBERNATE_CORE_JAR").expect("HIBERNATE_CORE_JAR for the @Immutable views");
                    let mut jar = format!("{api}:{hibernate}");
                    if validated {
                        let validation_api = std::env::var(if ns == "jakarta.persistence" { "JPA_JAKARTA_VALIDATION_API" } else { "JPA_JAVAX_VALIDATION_API" }).expect("validation API jar");
                        jar = format!("{jar}:{validation_api}");
                    }
                    let extension = if kotlin { "kt" } else { "java" };
                    let mut files: Vec<_> = plans
                        .iter()
                        .map(|p| {
                            let file = directory.join(format!("{}.{extension}", p.class));
                            std::fs::write(&file, render(p, "shop.db", ns, kotlin)).unwrap();
                            file
                        })
                        .collect();
                    for e in &enums {
                        let file = directory.join(format!("{}.{extension}", e.name));
                        std::fs::write(&file, render_enum(e, "shop.db", kotlin)).unwrap();
                        files.push(file);
                    }
                    if !kotlin {
                        let result = std::process::Command::new("javac")
                            .args(["--release", "17", "-proc:none", "-classpath", &jar, "-d"])
                            .arg(&directory)
                            .args(&files)
                            .output()
                            .expect("javac installed");
                        assert!(result.status.success(), "java {dialect:?} {ns}: {}", String::from_utf8_lossy(&result.stderr));
                    } else {
                        let compiler = std::env::var("KOTLIN_COMPILER_CLASSPATH").expect("Kotlin compiler classpath");
                        let stdlib = std::env::var("KOTLIN_STDLIB").expect("Kotlin stdlib");
                        let classpath = std::env::join_paths(jar.split(':').chain(std::iter::once(stdlib.as_str())).map(std::path::Path::new)).unwrap();
                        let result = std::process::Command::new("java")
                            .args(["-classpath", &compiler, "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler", "-no-stdlib", "-no-reflect", "-jvm-target", "17", "-classpath"])
                            .arg(classpath)
                            .arg("-d")
                            .arg(&directory)
                            .args(&files)
                            .output()
                            .expect("java installed");
                        assert!(result.status.success(), "kotlin {dialect:?} {ns}: {}\n{}", String::from_utf8_lossy(&result.stderr), String::from_utf8_lossy(&result.stdout));
                    }
                }
            }
        }
        std::fs::remove_dir_all(base).unwrap();
    }
}
