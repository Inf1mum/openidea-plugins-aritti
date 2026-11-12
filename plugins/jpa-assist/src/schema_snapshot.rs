//! Model-side schema snapshot built from entity sources only (no host calls).
//!
//! Closed world: the provided documents are taken as the whole model. Everything the
//! sources do not prove becomes `Unknown` with a reason: unresolved superclasses,
//! annotations this reader skips, types without a documented mapping. Java types map
//! to logical types per the Hibernate 6.4 user guide basic-type table; names derive
//! with Spring Boot's snake-case convention and tables record that as `Derived`.
#![allow(dead_code)]

use crate::model::{self, Attribute, Class, Kind, Relation, snake_case};
use crate::naming_strategy::{NamingStrategy, apply_physical};
use openidea_plugin::providers::Document;
use persistence_core::schema::{
    Column, Fact, ForeignKey, Index, LogicalType, MAX_COLUMNS, MAX_TABLES, Provenance, SchemaSnapshot, SqlType, Table,
    UniqueConstraint,
};
use std::collections::BTreeMap;

const MAX_TEXT: usize = 128 * 1024;
const MAX_CHAIN: usize = 32;

const OPAQUE: [&str; 15] = [
    "SecondaryTable",
    "SecondaryTables",
    "IdClass",
    "MapsId",
    "PrimaryKeyJoinColumn",
    "PrimaryKeyJoinColumns",
    "JoinColumns",
    "AttributeOverride",
    "AttributeOverrides",
    "AssociationOverride",
    "AssociationOverrides",
    "Convert",
    "Formula",
    "JdbcTypeCode",
    "DiscriminatorFormula",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Strategy {
    Single,
    Joined,
    PerClass,
}

struct Info {
    class: Class,
    qualified: String,
    inheritance: Option<String>,
    discriminator: Option<String>,
    is_abstract: bool,
    ref_override: bool,
    unique_unknown: bool,
    index_unknown: bool,
    unknown: Option<String>,
    envers: Envers,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum AuditTableName {
    #[default]
    Default,
    Literal(String),
    Opaque,
}

#[derive(Default)]
struct Envers {
    class_audited: bool,
    audited_args: bool,
    audit_table: AuditTableName,
    revision_entity: bool,
    revision_number: Option<String>,
    revision_timestamp: Option<String>,
    audited_fields: Vec<String>,
    not_audited_fields: Vec<String>,
    kotlin_field_level: bool,
}

impl Envers {
    fn audited(&self) -> bool {
        self.class_audited || !self.audited_fields.is_empty()
    }
}

fn is_envers(text: &str, class: &Class, a: &model::Annotation, name: &str) -> bool {
    let fqcn = format!("org.hibernate.envers.{name}");
    a.name == name && (text[a.start + 1..].starts_with(&fqcn) || class.imports.iter().any(|i| *i == fqcn || i == "org.hibernate.envers.*"))
}

fn read_envers(text: &str, masked: &str, anns: &[model::Annotation], class: &Class, class_at: usize, boundary: usize) -> Envers {
    let mut envers = Envers::default();

    for a in anns.iter().filter(|a| a.start >= boundary && a.start < class_at) {
        if is_envers(text, class, a, "Audited") {
            envers.class_audited = true;
            envers.audited_args |= a.args.is_some();
        }
        if is_envers(text, class, a, "RevisionEntity") {
            envers.revision_entity = true;
        }
        if is_envers(text, class, a, "AuditTable") {
            let opaque = ["schema", "catalog"].iter().any(|k| model::arg(text, masked, a, k).is_some());
            let literal = model::value_range(text, masked, a, "value").and_then(|r| model::plain_literal(text, r, class.kotlin)).map(|(_, v)| v.to_owned());
            envers.audit_table = match (opaque, literal) {
                (false, Some(v)) => AuditTableName::Literal(v),
                _ => AuditTableName::Opaque,
            };
        }
    }

    let body = masked[class_at..].find('{').map(|p| p + class_at);
    let Some(open) = body else { return envers };
    let b = masked.as_bytes();
    let close = model::closing(b, open).unwrap_or(masked.len());

    if class.kotlin {
        envers.kotlin_field_level = anns.iter().any(|a| a.start > class_at && (is_envers(text, class, a, "Audited") || is_envers(text, class, a, "NotAudited")));
        return envers;
    }

    for (start, end) in model::members(masked, open + 1, close, false, false) {
        let decl = model::without_annotations(masked, start, end);
        let Some((_, name, _)) = model::declaration(&decl, false) else { continue };
        for a in anns.iter().filter(|a| a.start >= start && a.start < end) {
            if is_envers(text, class, a, "Audited") {
                envers.audited_args |= a.args.is_some();
                envers.audited_fields.push(name.clone());
            }
            if is_envers(text, class, a, "NotAudited") {
                envers.not_audited_fields.push(name.clone());
            }
            if is_envers(text, class, a, "RevisionNumber") {
                envers.revision_number = Some(name.clone());
            }
            if is_envers(text, class, a, "RevisionTimestamp") {
                envers.revision_timestamp = Some(name.clone());
            }
        }
    }

    envers
}

fn last_segment(s: &str) -> &str {
    s.rsplit('.').next().unwrap_or(s).trim()
}

impl Info {
    fn parse(doc: &Document) -> Option<Self> {
        let class = model::parse_class(&doc.text, &doc.path).filter(|c| !c.name.is_empty())?;
        let (text, masked) = (doc.text.as_str(), model::mask(&doc.text));
        let anns = model::annotations(&masked);
        let marker = anns.iter().find(|a| matches!(a.name, "Entity" | "MappedSuperclass" | "Embeddable"))?;
        let (class_at, _) = model::declaration_keyword(&masked, marker.end(), &["class"])?;
        let boundary = masked[..class_at].rfind([';', '}']).map_or(0, |p| p + 1);
        let has = |a: &model::Annotation, key: &str| model::arg(text, &masked, a, key).is_some();

        let mut info = Self {
            qualified: class.package.as_ref().map_or(class.name.clone(), |p| format!("{p}.{}", class.name)),
            inheritance: None,
            discriminator: None,
            is_abstract: model::has_word(&masked[boundary..class_at], "abstract"),
            ref_override: false,
            unique_unknown: false,
            index_unknown: false,
            unknown: None,
            envers: Envers::default(),
            class,
        };
        info.envers = read_envers(text, &masked, &anns, &info.class, class_at, boundary);

        let mut discriminator_opaque = false;
        for a in &anns {
            match a.name {
                n if OPAQUE.contains(&n) => info.unknown = Some(format!("@{n} is not read")),
                "Column" if has(a, "columnDefinition") => info.unknown = Some("@Column(columnDefinition) is not read".into()),
                "JoinColumn" if has(a, "referencedColumnName") => info.ref_override = true,
                "Table" => {
                    if has(a, "schema") || has(a, "catalog") {
                        info.unknown = Some("schema or catalog qualified tables are not modeled".into());
                    }
                    info.unique_unknown = has(a, "uniqueConstraints");
                    info.index_unknown = has(a, "indexes");
                }
                "Inheritance" if a.start < class_at => {
                    let strategy = model::arg(text, &masked, a, "strategy");
                    info.inheritance = Some(strategy.map_or("?", last_segment).to_owned());
                }
                "DiscriminatorColumn" if a.start < class_at => {
                    discriminator_opaque = ["discriminatorType", "columnDefinition", "length", "options"].iter().any(|k| has(a, k));
                    let name = model::arg(text, &masked, a, "name").and_then(|v| v.trim().strip_prefix('"')?.strip_suffix('"'));
                    info.discriminator = name.map(str::to_owned);
                }
                _ => {}
            }
        }
        if discriminator_opaque {
            info.discriminator = None;
        }

        Some(info)
    }
}

struct Hierarchy {
    chain: Vec<usize>,
    root: usize,
    strategy: Strategy,
}

#[derive(Default)]
struct Built {
    columns: Vec<Column>,
    primary_key: Vec<String>,
    foreign_keys: Vec<ForeignKey>,
    foreign_keys_unknown: Option<String>,
    unique: Vec<UniqueConstraint>,
    incomplete: bool,
    late_unknown: Vec<(usize, String)>,
}

struct Model {
    infos: Vec<Info>,
    index: BTreeMap<String, usize>,
    naming: NamingStrategy,
}

pub fn snapshot_from_entities(docs: &[Document]) -> SchemaSnapshot {
    build(docs, &Scope::All, &NamingStrategy::unknown("naming strategy was not read"), &EnversConfig::default()).snapshot
}

/// Like [`snapshot_from_entities`], deriving unannotated table and column names with a detected naming strategy.
pub fn snapshot_from_entities_named(docs: &[Document], naming: &NamingStrategy) -> SchemaSnapshot {
    build(docs, &Scope::All, naming, &EnversConfig::default()).snapshot
}

/// The model of one persistence scope with the project's naming strategy and Envers configuration.
pub fn snapshot_configured(docs: &[Document], scope: &Scope, naming: &NamingStrategy, envers: &EnversConfig) -> ScopedSnapshot {
    build(docs, scope, naming, envers)
}

const SUFFIX_NAME: &str = "org.hibernate.envers.audit_table_suffix";
const PREFIX_NAME: &str = "org.hibernate.envers.audit_table_prefix";
const SPRING_SUFFIX_KEY: &str = "spring.jpa.properties.org.hibernate.envers.audit_table_suffix";
const SPRING_PREFIX_KEY: &str = "spring.jpa.properties.org.hibernate.envers.audit_table_prefix";

/// Literal Envers settings read from `application*.properties|yml|yaml` (Spring `spring.jpa.properties.` keys) and
/// `persistence.xml` `<property>` elements. A conflict, placeholder or profile-only value makes audit table names unknown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnversConfig {
    pub suffix: Option<String>,
    pub prefix: Option<String>,
    pub names_unknown: Option<String>,
    config_text: String,
}

impl EnversConfig {
    pub fn from_sources(applications: &[(String, String)], persistence_xml: &[(String, String)]) -> Self {
        let mut suffix: Vec<(String, String, bool)> = Vec::new();
        let mut prefix: Vec<(String, String, bool)> = Vec::new();
        for (path, text) in applications {
            let file = path.rsplit('/').next().unwrap_or(path);
            let profile = file.starts_with("application-");
            for (key, value, later) in crate::naming_strategy::read_keys(text, !file.ends_with(".properties"), &[SPRING_SUFFIX_KEY, SPRING_PREFIX_KEY]) {
                let target = if key == SPRING_SUFFIX_KEY { &mut suffix } else { &mut prefix };
                target.push((path.clone(), value, profile || later));
            }
        }
        for (path, text) in persistence_xml {
            for (name, value) in persistence_properties(text) {
                match name.as_str() {
                    SUFFIX_NAME => suffix.push((path.clone(), value, false)),
                    PREFIX_NAME => prefix.push((path.clone(), value, false)),
                    _ => {}
                }
            }
        }

        let mut names_unknown = None;
        let mut settle = |values: &[(String, String, bool)], what: &str| match crate::naming_strategy::settle(values) {
            Ok(value) => value,
            Err(why) => {
                names_unknown.get_or_insert(format!("audit table {what}: {why}"));
                None
            }
        };
        let suffix = settle(&suffix, "suffix");
        let prefix = settle(&prefix, "prefix");
        let config_text = applications.iter().chain(persistence_xml).map(|(_, text)| text.to_lowercase()).collect::<Vec<_>>().join("\n");

        Self { suffix, prefix, names_unknown, config_text }
    }

    pub fn detect(project: &openidea_plugin::Project) -> Self {
        let applications = crate::naming_strategy::gather(project, &crate::naming_strategy::APPLICATION_PATTERNS, &crate::naming_strategy::is_main_resource);
        let persistence_xml = crate::naming_strategy::gather(project, &["**/persistence.xml"], &|_| true);
        Self::from_sources(&applications, &persistence_xml)
    }
}

/// `(name, value)` of every `<property name=".." value=".."/>` element.
fn persistence_properties(text: &str) -> Vec<(String, String)> {
    text.split("<property")
        .skip(1)
        .filter_map(|tag| {
            let tag = tag.split('>').next()?;
            let attr = |name: &str| {
                let at = tag.find(&format!("{name}=\""))? + name.len() + 2;
                let rest = &tag[at..];
                Some(rest[..rest.find('"')?].trim().to_owned())
            };
            Some((attr("name")?, attr("value")?))
        })
        .collect()
}

/// Persistence scope of a snapshot (G106). `Unit` carries a persistence.xml unit's `<class>` entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    All,
    Unit { name: String, classes: Vec<String>, exclude_unlisted: bool },
}

impl Scope {
    /// The named unit of one persistence.xml, read with `structure_related::parse_units`.
    pub fn from_persistence_xml(path: &str, text: &str, unit: &str) -> Option<Scope> {
        let parsed = crate::structure_related::parse_units(path, text).into_iter().find(|u| u.name == unit)?;
        let mut classes: Vec<String> = parsed.classes.into_iter().collect();
        classes.sort();

        let mut current: Option<String> = None;
        let mut exclude_unlisted = false;
        for element in crate::xml::parse(text) {
            match element.name.as_str() {
                "persistence-unit" => current = element.attributes.iter().find(|a| a.name == "name").map(|a| a.value.clone()),
                "exclude-unlisted-classes" if current.as_deref() == Some(unit) => {
                    exclude_unlisted = element.text.map_or(true, |(_, v)| v.trim() != "false");
                }
                _ => {}
            }
        }

        Some(Scope::Unit { name: unit.to_owned(), classes, exclude_unlisted })
    }
}

pub struct ScopedSnapshot {
    pub snapshot: SchemaSnapshot,
    pub notes: Vec<String>,
}

pub fn snapshot_from_entities_scoped(docs: &[Document], scope: &Scope) -> ScopedSnapshot {
    build(docs, scope, &NamingStrategy::unknown("naming strategy was not read"), &EnversConfig::default())
}

struct EnversSettings {
    names_unknown: Option<String>,
    columns_unknown: Option<String>,
    suffix: Option<String>,
    prefix: Option<String>,
}

fn envers_settings(docs: &[Document], config: &EnversConfig) -> EnversSettings {
    let text: String = docs.iter().map(|d| d.text.to_lowercase()).collect::<Vec<_>>().join("\n");
    let in_sources = |needles: &[&str], text: &str| needles.iter().find(|n| text.contains(**n)).map(|n| format!("the property {n} is present in the provided sources"));
    let name_needles_in_code = in_sources(&["audit_table_suffix", "audit_table_prefix"], &text);
    let schema_needles = ["envers.default_schema", "envers.default_catalog"];
    let column_needles = ["revision_field_name", "revision_type_field_name", "audit_strategy", "modified_flag"];

    EnversSettings {
        names_unknown: name_needles_in_code
            .or_else(|| config.names_unknown.clone())
            .or_else(|| in_sources(&schema_needles, &text))
            .or_else(|| in_sources(&schema_needles, &config.config_text)),
        columns_unknown: in_sources(&column_needles, &text).or_else(|| in_sources(&column_needles, &config.config_text)),
        suffix: config.suffix.clone(),
        prefix: config.prefix.clone(),
    }
}

enum Revision {
    Default,
    Custom { table: String, number: Option<(String, Fact<SqlType>)> },
    Unknown(String),
}

const AUDIT_NOTE_SEQUENCES: &str = "sequence objects (for example REVINFO_SEQ) are not modelled";
const AUDIT_NOTE_COLLECTIONS: &str = "audit tables of collections and join tables are not modelled";

fn build(docs: &[Document], scope: &Scope, naming: &NamingStrategy, envers: &EnversConfig) -> ScopedSnapshot {
    let mut notes: Vec<String> = Vec::new();
    let mut complete = true;
    let mut infos = Vec::new();

    for doc in docs {
        let mentions = ["@Entity", "@MappedSuperclass", "@Embeddable"].iter().any(|m| doc.text.contains(m));
        if !mentions || model::language_of(&doc.path).is_none() {
            continue;
        }
        if doc.text.len() > MAX_TEXT {
            complete = false;
            continue;
        }
        match Info::parse(doc) {
            Some(info) => infos.push(info),
            None => complete = false,
        }
    }

    infos.sort_by(|a, b| a.qualified.cmp(&b.qualified).then_with(|| a.class.path.cmp(&b.class.path)));
    for i in 1..infos.len() {
        if infos[i].qualified == infos[i - 1].qualified {
            let reason = format!("class {} is declared more than once in the sources", infos[i].qualified);
            infos[i].unknown = Some(reason.clone());
            infos[i - 1].unknown = Some(reason);
        }
    }

    let index = infos.iter().enumerate().map(|(i, info)| (info.qualified.clone(), i)).collect();
    let model = Model { infos, index, naming: naming.clone() };
    if let Some(label) = naming.physical_label() {
        notes.push(format!("derived table and column names use {label} ({})", naming.source));
    }
    let mut late: BTreeMap<String, String> = BTreeMap::new();
    let mut tables: Vec<Table> = Vec::new();
    let settings = envers_settings(docs, envers);
    let revision = model.revision();
    let mut audit_tables = 0usize;

    if let Scope::Unit { name, classes, exclude_unlisted } = scope {
        for class in classes.iter().filter(|c| !model.index.contains_key(*c)) {
            notes.push(format!("persistence unit {name} lists class {class}, which is not in the provided sources"));
            complete = false;
        }
        if !exclude_unlisted {
            notes.push(format!("persistence unit {name} does not set exclude-unlisted-classes to true; unlisted entities may belong to it"));
            complete = false;
        }
    }

    for idx in 0..model.infos.len() {
        if model.infos[idx].class.kind != Kind::Entity {
            continue;
        }
        if let Scope::Unit { classes, .. } = scope {
            if !classes.contains(&model.infos[idx].qualified) {
                continue;
            }
        }
        let (table, built) = model.build_table(idx);
        if let (Some(base), Some(built)) = (&table, &built) {
            if model.infos[idx].envers.audited() {
                tables.push(model.audit_table(idx, base, built, &settings, &revision));
                audit_tables += 1;
                if built.incomplete {
                    notes.push(format!("{}: {AUDIT_NOTE_COLLECTIONS}", model.infos[idx].qualified));
                }
            }
        } else if let (Some(base), None) = (&table, &built) {
            if model.infos[idx].envers.audited() {
                tables.push(Table::unknown(&format!("{}_aud", base.name), Provenance::Derived, "the audited entity's table is unknown"));
                audit_tables += 1;
            }
        } else if table.is_none() && model.infos[idx].envers.audited() {
            notes.push(format!("{} shares another entity's table or has none; its audit table is not modelled", model.infos[idx].qualified));
            complete = false;
        }
        complete &= !built.as_ref().is_some_and(|b| b.incomplete);
        for (target, reason) in built.map(|b| b.late_unknown).unwrap_or_default() {
            match model.home_table(target) {
                Ok((name, _)) => {
                    late.insert(name.to_lowercase(), reason);
                }
                Err(_) => complete = false,
            }
        }
        tables.extend(table);
    }

    if audit_tables > 0 {
        notes.push(AUDIT_NOTE_SEQUENCES.to_owned());
        match &revision {
            Revision::Default => tables.push(default_revinfo()),
            Revision::Custom { .. } => {}
            Revision::Unknown(reason) => {
                notes.push(format!("revision info table is not modelled: {reason}"));
                complete = false;
            }
        }
    }

    let mut by_name: BTreeMap<String, Table> = BTreeMap::new();
    for table in tables {
        let key = table.name.to_lowercase();
        match by_name.remove(&key) {
            None => {
                by_name.insert(key, table);
            }
            Some(first) => {
                let reason = format!("several entities map to table {}", first.name);
                by_name.insert(key, Table::unknown(&first.name, first.provenance, &reason));
            }
        }
    }
    for (key, reason) in late {
        if let Some(table) = by_name.get_mut(&key) {
            *table = Table::unknown(&table.name, table.provenance, &reason);
        }
    }

    let mut tables: Vec<Table> = by_name.into_values().collect();
    if tables.len() > MAX_TABLES {
        tables.truncate(MAX_TABLES);
        complete = false;
    }

    let mut snapshot = SchemaSnapshot { tables, complete, ..SchemaSnapshot::default() };
    snapshot.normalize();
    notes.sort();
    notes.dedup();
    ScopedSnapshot { snapshot, notes }
}

fn known_type(logical: LogicalType, raw: &str) -> Fact<SqlType> {
    Fact::Known(SqlType { logical, raw: raw.to_owned() })
}

/// Guide 6.6 "Understanding the Envers Schema": REVINFO(REV integer, REVTSTMP bigint, primary key (REV)).
/// Unknown: REV generation (the guide DDL shows identity; a REVINFO_SEQ sequence in Hibernate 6 is not in the guide).
fn default_revinfo() -> Table {
    let mut rev = Column::new("rev", LogicalType::Integer);
    rev.sql_type = known_type(LogicalType::Integer, "integer");
    rev.nullable = Fact::Known(false);
    rev.generated = Fact::unknown("REV generation (identity or REVINFO_SEQ) depends on the Hibernate version");
    rev.default = Fact::unknown("column defaults are not read");

    let mut timestamp = Column::new("revtstmp", LogicalType::Bigint);
    timestamp.sql_type = known_type(LogicalType::Bigint, "bigint");
    timestamp.nullable = Fact::Known(true);
    timestamp.default = Fact::unknown("column defaults are not read");

    let mut table = Table::new("revinfo");
    table.provenance = Provenance::Derived;
    table.columns = Fact::Known(vec![rev, timestamp]);
    table.primary_key = Fact::Known(vec!["rev".into()]);
    table
}

/// Baseline file: the snapshot fields plus optional `generatedBy`, `scope` and `notes`.
/// `snapshot_from_json` ignores the extras, so older readers' files and these load alike.
pub const BASELINE_GENERATED_BY: &str = "jpa-assist schema-snapshot 1";
const BASELINE_EXTRAS: [&str; 3] = ["generatedBy", "scope", "notes"];

pub struct BaselineOptions<'a> {
    pub scope: &'a Scope,
    pub notes: &'a [String],
}

#[derive(serde::Serialize)]
struct BaselineFile {
    #[serde(flatten)]
    snapshot: SchemaSnapshot,
    #[serde(rename = "generatedBy")]
    generated_by: &'static str,
    scope: serde_json::Value,
    notes: Vec<String>,
}

pub fn baseline_json(snapshot: &SchemaSnapshot, options: &BaselineOptions) -> Result<String, String> {
    snapshot.validate().map_err(|e| format!("{e:?}"))?;
    let mut sorted = snapshot.clone();
    sorted.normalize();

    let scope = match options.scope {
        Scope::All => serde_json::json!({ "kind": "all" }),
        Scope::Unit { name, classes, exclude_unlisted } => {
            let mut classes = classes.clone();
            classes.sort();
            serde_json::json!({ "kind": "unit", "name": name, "classes": classes, "excludeUnlistedClasses": exclude_unlisted })
        }
    };
    let mut notes = options.notes.to_vec();
    notes.sort();
    notes.dedup();

    serde_json::to_string_pretty(&BaselineFile { snapshot: sorted, generated_by: BASELINE_GENERATED_BY, scope, notes }).map_err(|e| e.to_string())
}

/// The persistence unit a baseline file was taken for; `None` for the whole project (also for files without a header).
pub fn recorded_unit(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let scope = value.get("scope")?;
    (scope.get("kind")?.as_str()? == "unit").then(|| scope.get("name")?.as_str().map(str::to_owned)).flatten()
}

/// Baseline file text for a scope: a plain snapshot for the whole project, the scope-recording form for a unit.
pub fn scoped_json(scoped: &ScopedSnapshot, scope: &Scope) -> Result<String, String> {
    match scope {
        Scope::All => snapshot_json(&scoped.snapshot),
        Scope::Unit { .. } => baseline_json(&scoped.snapshot, &BaselineOptions { scope, notes: &scoped.notes }),
    }
}

pub fn snapshot_json(snapshot: &SchemaSnapshot) -> Result<String, String> {
    snapshot.validate().map_err(|e| format!("{e:?}"))?;
    let mut sorted = snapshot.clone();
    sorted.normalize();
    serde_json::to_string_pretty(&sorted).map_err(|e| e.to_string())
}

pub fn snapshot_from_json(json: &str) -> Result<SchemaSnapshot, String> {
    let mut value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if let Some(object) = value.as_object_mut() {
        BASELINE_EXTRAS.iter().for_each(|k| {
            object.remove(*k);
        });
    }
    let snapshot: SchemaSnapshot = serde_json::from_value(value).map_err(|e| e.to_string())?;
    snapshot.validate().map_err(|e| format!("{e:?}"))?;
    Ok(snapshot)
}

fn map_type(a: &Attribute) -> Fact<SqlType> {
    use LogicalType::*;
    let known = |logical| Fact::Known(SqlType { logical, raw: a.ty.clone() });
    let base = a.ty.trim_end_matches('?').rsplit('.').next().unwrap_or(&a.ty);

    if a.lob {
        return Fact::unknown("@Lob mapping is not verified");
    }
    match a.enum_string {
        Some(true) => return known(Varchar),
        Some(false) => return known(Integer),
        None => {}
    }

    match base {
        "Long" | "long" => known(Bigint),
        "Integer" | "int" | "Int" => known(Integer),
        "Short" | "short" => known(Smallint),
        "Byte" | "byte" => known(Tinyint),
        "BigInteger" | "BigDecimal" | "Duration" => known(Decimal),
        "Character" | "char" | "Char" => known(Char),
        "String" => known(Varchar),
        "Boolean" | "boolean" => known(Boolean),
        "byte[]" | "ByteArray" => known(Binary),
        "LocalDate" => known(Date),
        "LocalDateTime" => known(Timestamp),
        "LocalTime" => known(Time),
        "UUID" => known(Uuid),
        "Double" | "double" | "Float" | "float" => Fact::unknown("floating point columns vary by database"),
        "Instant" | "OffsetDateTime" | "ZonedDateTime" => Fact::unknown("time zone column type varies by database"),
        _ => Fact::Unknown(format!("no documented column mapping for type {base}")),
    }
}

impl Model {
    /// The physical name for a logical one: the detected strategy when known, today's snake case otherwise.
    fn physical(&self, logical: &str) -> String {
        apply_physical(&self.naming, logical).unwrap_or_else(|| snake_case(logical))
    }

    /// An explicit `@Table`/`@Column` name is a logical name too: the physical strategy transforms it (unchanged when the strategy or the quoting is unknown).
    fn explicit(&self, logical: &str) -> String {
        apply_physical(&self.naming, logical).unwrap_or_else(|| logical.to_owned())
    }

    fn column_name(&self, a: &Attribute) -> String {
        a.column.as_deref().map_or_else(|| self.physical(&a.name), |name| self.explicit(name))
    }

    fn resolve(&self, class: &Class, ty: &str) -> Option<usize> {
        let base = ty.trim_end_matches('?').rsplit('.').next().unwrap_or(ty);
        let q = class.qualified(base).unwrap_or_else(|| base.to_owned());
        self.index.get(&q).copied()
    }

    fn own_table(&self, idx: usize) -> (String, Provenance) {
        let c = &self.infos[idx].class;
        match &c.table {
            Some(name) => (self.explicit(name), Provenance::Literal),
            None => (self.physical(&c.name), Provenance::Derived),
        }
    }

    fn attr_column(&self, a: &Attribute) -> String {
        match a.relation.to_one() {
            true => a.column.as_deref().map_or_else(|| self.physical(&format!("{}_id", a.name)), |name| self.explicit(name)),
            false => self.column_name(a),
        }
    }

    fn revision(&self) -> Revision {
        let found: Vec<usize> = (0..self.infos.len()).filter(|&i| self.infos[i].envers.revision_entity).collect();
        let [idx] = found.as_slice() else {
            return match found.is_empty() {
                true => Revision::Default,
                false => Revision::Unknown("several @RevisionEntity classes are an error per the guide".into()),
            };
        };
        let info = &self.infos[*idx];
        let number = info.envers.revision_number.as_ref().and_then(|n| self.own_attrs(*idx).into_iter().find(|(_, a)| a.name == *n)).map(|(_, a)| (self.attr_column(a), map_type(a)));

        Revision::Custom { table: self.own_table(*idx).0, number }
    }

    /// Guide 6.6 "Understanding the Envers Schema": `<table>_AUD(id.., REV integer not null, REVTYPE tinyint, audited columns.., primary key (id.., REV))`
    /// plus a foreign key REV -> REVINFO. Lower-case names follow the snake-case naming this reader assumes.
    /// Audited value columns are nullable (guide DDL: `customer_name varchar(255)`), REVTYPE is `not null` (current guide; the 5.2 guide
    /// printed it nullable). Unknown: inherited attributes, generation.
    fn audit_table(&self, idx: usize, base: &Table, built: &Built, settings: &EnversSettings, revision: &Revision) -> Table {
        let info = &self.infos[idx];
        let envers = &info.envers;
        let default_name = match (&settings.prefix, &settings.suffix) {
            (None, None) => format!("{}_aud", base.name),
            (prefix, suffix) => self.physical(&format!("{}{}{}", prefix.as_deref().unwrap_or(""), base.name, suffix.as_deref().unwrap_or("_AUD"))),
        };
        let (name, provenance, name_reason) = match (&envers.audit_table, &settings.names_unknown) {
            (AuditTableName::Literal(v), _) => (v.clone(), Provenance::Literal, None),
            (AuditTableName::Opaque, _) => (default_name, Provenance::Derived, Some("@AuditTable is not a plain literal name or sets a schema or catalog".to_owned())),
            (AuditTableName::Default, Some(reason)) => (default_name, Provenance::Derived, Some(format!("the audit table name depends on a custom setting: {reason}"))),
            (AuditTableName::Default, None) => (default_name, Provenance::Derived, None),
        };
        let unknown = |reason: &str| Table::unknown(&name, provenance, reason);

        if let Some(reason) = name_reason {
            return unknown(&reason);
        }
        if let Some(reason) = &settings.columns_unknown {
            return unknown(&format!("audit columns depend on a custom setting: {reason}"));
        }
        if envers.audited_args || envers.kotlin_field_level {
            return unknown("@Audited arguments or Kotlin field-level Envers annotations are not read");
        }
        let Fact::Known(base_columns) = &base.columns else {
            return unknown("the audited entity's own columns are unknown");
        };
        let descendants = (0..self.infos.len()).any(|i| i != idx && self.infos[i].class.kind == Kind::Entity && self.hierarchy(i).is_ok_and(|d| d.chain.contains(&idx)));
        if descendants || self.hierarchy(idx).map_or(true, |h| h.chain.len() != 1) || self.own_attrs(idx).iter().any(|(o, _)| *o != idx) {
            return unknown("audit tables of inheritance hierarchies and mapped superclasses are not modelled");
        }

        let attrs = &info.class.attributes;
        let included: Vec<&Attribute> = attrs
            .iter()
            .filter(|a| a.id || match envers.class_audited {
                true => !envers.not_audited_fields.contains(&a.name),
                false => envers.audited_fields.contains(&a.name),
            })
            .collect();
        let included_names: Vec<String> = included.iter().map(|a| self.attr_column(a)).collect();
        let to_one = included.iter().any(|a| a.relation.to_one());

        let mut id_columns = Vec::new();
        let mut value_columns = Vec::new();
        for column in base_columns.iter().filter(|c| included_names.contains(&c.name)) {
            let mut audit = column.clone();
            audit.generated = Fact::Known(false);
            if !included.iter().any(|a| a.id && self.attr_column(a) == column.name) {
                audit.nullable = Fact::Known(true);
                value_columns.push(audit);
                continue;
            }
            audit.nullable = Fact::Known(false);
            id_columns.push(audit);
        }

        let (rev_type, rev_ref, rev_table) = match revision {
            Revision::Default => (known_type(LogicalType::Integer, "integer"), Fact::Known(vec!["rev".to_owned()]), "revinfo".to_owned()),
            Revision::Custom { table, number } => (
                number.as_ref().map_or(Fact::unknown("@RevisionNumber is not declared in the revision entity's own source"), |(_, t)| t.clone()),
                number.as_ref().map_or(Fact::unknown("@RevisionNumber is not declared in the revision entity's own source"), |(c, _)| Fact::Known(vec![c.clone()])),
                table.clone(),
            ),
            Revision::Unknown(reason) => (Fact::Unknown(reason.clone()), Fact::Unknown(reason.clone()), String::new()),
        };

        let mut rev = Column::new("rev", LogicalType::Integer);
        rev.sql_type = rev_type;
        rev.nullable = Fact::Known(false);
        rev.generated = Fact::Known(false);
        rev.default = Fact::unknown("column defaults are not read");

        let mut revtype = Column::new("revtype", LogicalType::Tinyint);
        revtype.sql_type = known_type(LogicalType::Tinyint, "tinyint");
        revtype.nullable = Fact::Known(false);
        revtype.generated = Fact::Known(false);
        revtype.default = Fact::unknown("column defaults are not read");

        let mut table = Table::new(&name);
        table.provenance = provenance;
        let mut primary_key: Vec<String> = id_columns.iter().map(|c| c.name.clone()).collect();
        primary_key.push("rev".into());
        table.columns = Fact::Known(id_columns.into_iter().chain([rev, revtype]).chain(value_columns).collect());
        table.primary_key = match (envers.class_audited, &base.primary_key) {
            (true, Fact::Known(ids)) if !ids.is_empty() => Fact::Known(primary_key),
            (true, _) => Fact::unknown("the audited entity's id columns are unknown"),
            (false, _) => Fact::unknown("the guide shows the audit primary key only for class-level @Audited"),
        };
        table.foreign_keys = match (to_one, revision) {
            (_, Revision::Unknown(reason)) => Fact::Unknown(reason.clone()),
            (true, _) => Fact::unknown("foreign keys of audited to-one columns are not proven"),
            _ => Fact::Known(vec![ForeignKey { name: None, columns: vec!["rev".into()], ref_table: rev_table, ref_columns: rev_ref }]),
        };
        table.unique_constraints = match built.unique.is_empty() {
            true => Fact::Known(Vec::new()),
            false => Fact::unknown("unique constraints of the audited entity are not proven to carry over"),
        };
        table.indexes = match base.indexes {
            Fact::Known(_) => Fact::Known(Vec::<Index>::new()),
            Fact::Unknown(_) => Fact::unknown("@Table(indexes) is not read"),
        };

        table
    }

    fn hierarchy(&self, start: usize) -> Result<Hierarchy, String> {
        let mut chain = vec![start];
        loop {
            let class = &self.infos[*chain.last().unwrap_or(&start)].class;
            let Some(sup) = class.superclass.as_deref() else { break };
            if matches!(sup, "Object" | "Any") {
                break;
            }
            let next = self.resolve(class, sup).ok_or_else(|| format!("superclass {sup} is not in the provided sources"))?;
            if chain.contains(&next) || chain.len() >= MAX_CHAIN {
                return Err("inheritance cycle or excessive depth".into());
            }
            if self.infos[next].class.kind == Kind::Embeddable {
                return Err(format!("superclass {sup} is an embeddable"));
            }
            chain.push(next);
        }

        let root = chain.iter().rev().copied().find(|&i| self.infos[i].class.kind == Kind::Entity).unwrap_or(start);
        let strategy = match self.infos[root].inheritance.as_deref() {
            None | Some("SINGLE_TABLE") => Strategy::Single,
            Some("JOINED") => Strategy::Joined,
            Some("TABLE_PER_CLASS") => Strategy::PerClass,
            Some(other) => return Err(format!("inheritance strategy {other} is not a literal strategy")),
        };

        Ok(Hierarchy { chain, root, strategy })
    }

    fn home_table(&self, idx: usize) -> Result<(String, Provenance), String> {
        if self.infos[idx].class.kind != Kind::Entity {
            return Err(format!("{} is not an entity", self.infos[idx].class.name));
        }
        let h = self.hierarchy(idx)?;
        Ok(self.own_table(if h.strategy == Strategy::Single { h.root } else { idx }))
    }

    /// Attributes of `idx` and the mapped superclasses directly above it, root first.
    fn own_attrs(&self, idx: usize) -> Vec<(usize, &Attribute)> {
        let mut owners = vec![idx];
        let mut at = idx;
        while let Some(sup) = self.infos[at].class.superclass.as_deref().and_then(|s| self.resolve(&self.infos[at].class, s)) {
            if self.infos[sup].class.kind != Kind::MappedSuperclass || owners.contains(&sup) {
                break;
            }
            owners.push(sup);
            at = sup;
        }
        owners.iter().rev().flat_map(|&o| self.infos[o].class.attributes.iter().map(move |a| (o, a))).collect()
    }

    fn id_column(&self, idx: usize) -> Result<(String, Fact<SqlType>), String> {
        let h = self.hierarchy(idx)?;
        let ids: Vec<&Attribute> = h.chain.iter().flat_map(|&i| &self.infos[i].class.attributes).filter(|a| a.id).collect();
        match ids.as_slice() {
            [a] if a.relation == Relation::None => Ok((self.column_name(a), map_type(a))),
            [] => Err("no @Id in the provided sources".into()),
            _ => Err("composite or embedded id".into()),
        }
    }

    fn ids(&self, h: &Hierarchy) -> Vec<String> {
        h.chain.iter().rev().flat_map(|&i| &self.infos[i].class.attributes).filter(|a| a.id).map(|a| self.column_name(a)).collect()
    }

    fn piece(&self, owner: usize, a: &Attribute, force_nullable: bool, out: &mut Built) -> Result<(), String> {
        let info = &self.infos[owner];
        let class = &info.class;

        match a.relation {
            Relation::Embedded => return Err(format!("attribute {} is embedded; its columns are not read", a.name)),
            Relation::ElementCollection => {
                out.incomplete = true;
                return Ok(());
            }
            Relation::ManyToMany | Relation::OneToMany => {
                if a.mapped_by {
                    return Ok(());
                }
                match (a.relation, a.join_column, self.resolve(class, &a.target)) {
                    (Relation::OneToMany, true, Some(target)) => out
                        .late_unknown
                        .push((target, format!("unidirectional @OneToMany {} adds a column to this table", a.name))),
                    _ => out.incomplete = true,
                }
                return Ok(());
            }
            Relation::OneToOne if a.mapped_by => return Ok(()),
            _ => {}
        }

        let nullable = match (a.not_null || a.id, class.kotlin) {
            (true, _) => Fact::Known(false),
            (false, true) => Fact::unknown("Kotlin nullability is not read"),
            (false, false) => Fact::Known(!a.id),
        };
        let nullable = if force_nullable && !a.id { Fact::Known(true) } else { nullable };
        let generated = match a.generated.as_deref() {
            None | Some("SEQUENCE" | "TABLE" | "UUID") => Fact::Known(false),
            Some("IDENTITY") => Fact::Known(true),
            Some(_) => Fact::unknown("generation strategy depends on the provider"),
        };

        let mut column = Column {
            name: self.column_name(a),
            sql_type: Fact::unknown("unset"),
            nullable,
            default: Fact::unknown("column defaults are not read"),
            length: None,
            precision: None,
            scale: None,
            generated,
        };

        if a.relation.to_one() {
            column.name = a.column.as_deref().map_or_else(|| self.physical(&format!("{}_id", a.name)), |name| self.explicit(name));
            let target = self.resolve(class, &a.target).filter(|&t| self.infos[t].class.kind == Kind::Entity);
            let outcome = target
                .ok_or_else(|| format!("target {} is not an entity in the provided sources", a.target))
                .and_then(|t| Ok((self.id_column(t)?, self.home_table(t)?)));
            match outcome {
                Ok(((id_name, id_type), (table, _))) => {
                    column.sql_type = id_type;
                    let ref_columns = if info.ref_override { Fact::unknown("@JoinColumn(referencedColumnName) is not read") } else { Fact::Known(vec![id_name]) };
                    out.foreign_keys.push(ForeignKey { name: None, columns: vec![column.name.clone()], ref_table: table, ref_columns });
                }
                Err(reason) => {
                    column.sql_type = Fact::Unknown(reason.clone());
                    out.foreign_keys_unknown = Some(reason);
                }
            }
        } else {
            if let Some(embeddable) = self.resolve(class, &a.ty).filter(|&t| self.infos[t].class.kind == Kind::Embeddable) {
                return Err(format!("attribute {} is the embeddable {}", a.name, self.infos[embeddable].class.name));
            }
            column.sql_type = map_type(a);
            let logical = column.sql_type.known().map(|t| t.logical);
            if matches!(logical, Some(LogicalType::Varchar | LogicalType::Char | LogicalType::Binary)) {
                column.length = a.length;
            }
            if logical == Some(LogicalType::Decimal) {
                column.precision = a.precision;
                column.scale = a.scale;
            }
        }

        if a.id {
            out.primary_key.push(column.name.clone());
        }
        if a.unique {
            out.unique.push(UniqueConstraint { name: None, columns: vec![column.name.clone()] });
        }
        out.columns.push(column);

        Ok(())
    }

    fn collect(&self, parts: &[(usize, bool)], out: &mut Built) -> Result<(), String> {
        for &(idx, force_nullable) in parts {
            for (owner, a) in self.own_attrs(idx) {
                self.piece(owner, a, force_nullable, out)?;
            }
        }
        Ok(())
    }

    /// The entity's own table, or `None` when it has none (merged into its root's, or abstract per-class).
    fn build_table(&self, idx: usize) -> (Option<Table>, Option<Built>) {
        let info = &self.infos[idx];
        let (name, provenance) = self.own_table(idx);
        let unknown = |reason: &str| (Some(Table::unknown(&name, provenance, reason)), None);

        if let Some(reason) = &info.unknown {
            return unknown(reason);
        }
        let h = match self.hierarchy(idx) {
            Ok(h) => h,
            Err(reason) => return unknown(&reason),
        };
        if let Some(reason) = h.chain.iter().find_map(|&i| self.infos[i].unknown.as_ref()) {
            return unknown(&format!("inherited mapping: {reason}"));
        }

        let mut built = Built::default();
        let mut parts = vec![(idx, false)];
        let mut ids = self.ids(&h);
        let result = match h.strategy {
            Strategy::Single if h.root != idx => return (None, None),
            Strategy::Single => {
                let descendants: Vec<usize> = (0..self.infos.len())
                    .filter(|&i| i != idx && self.infos[i].class.kind == Kind::Entity)
                    .filter(|&i| self.hierarchy(i).is_ok_and(|d| d.root == idx))
                    .collect();
                parts.extend(descendants.iter().map(|&d| (d, true)));
                match (descendants.is_empty(), &info.discriminator) {
                    (true, _) => self.collect(&parts, &mut built),
                    (false, None) => Err("discriminator column is not a literal @DiscriminatorColumn(name)".to_owned()),
                    (false, Some(n)) => self.collect(&parts, &mut built).map(|()| {
                        let mut d = Column::new(n, LogicalType::Varchar);
                        d.nullable = Fact::Known(false);
                        d.default = Fact::unknown("column defaults are not read");
                        built.columns.push(d);
                    }),
                }
            }
            Strategy::Joined if h.root != idx => {
                let parent = h.chain.iter().copied().skip(1).find(|&i| self.infos[i].class.kind == Kind::Entity);
                let root_id = self.id_column(h.root);
                self.collect(&parts, &mut built).and_then(|()| {
                    let (id_name, id_type) = root_id?;
                    let parent_table = self.own_table(parent.unwrap_or(h.root)).0;
                    let mut pk = Column::new(&id_name, LogicalType::Bigint);
                    pk.sql_type = id_type;
                    pk.nullable = Fact::Known(false);
                    pk.default = Fact::unknown("column defaults are not read");
                    built.columns.insert(0, pk);
                    built.foreign_keys.push(ForeignKey {
                        name: None,
                        columns: vec![id_name.clone()],
                        ref_table: parent_table,
                        ref_columns: Fact::Known(vec![id_name.clone()]),
                    });
                    ids = vec![id_name];
                    Ok(())
                })
            }
            Strategy::Joined => self.collect(&parts, &mut built),
            Strategy::PerClass => {
                if info.is_abstract {
                    return (None, None);
                }
                let entities: Vec<(usize, bool)> = h.chain.iter().rev().filter(|&&i| self.infos[i].class.kind == Kind::Entity).map(|&i| (i, false)).collect();
                self.collect(&entities, &mut built)
            }
        };

        if let Err(reason) = result {
            return (Some(Table::unknown(&name, provenance, &reason)), Some(built));
        }

        let mut seen = std::collections::BTreeSet::new();
        if let Some(c) = built.columns.iter().find(|c| !seen.insert(c.name.to_lowercase())) {
            return (Some(Table::unknown(&name, provenance, &format!("column {} is mapped twice", c.name))), Some(built));
        }
        if built.columns.len() > MAX_COLUMNS {
            return (Some(Table::unknown(&name, provenance, "too many columns")), Some(built));
        }

        let mut table = Table::new(&name);
        table.provenance = provenance;
        table.columns = Fact::Known(built.columns.clone());
        table.primary_key = if ids.is_empty() { Fact::unknown("no @Id in the provided sources") } else { Fact::Known(ids) };
        table.foreign_keys = match &built.foreign_keys_unknown {
            Some(reason) => Fact::Unknown(reason.clone()),
            None => Fact::Known(built.foreign_keys.clone()),
        };
        table.unique_constraints = if info.unique_unknown { Fact::unknown("@Table(uniqueConstraints) is not read") } else { Fact::Known(built.unique.clone()) };
        table.indexes = if info.index_unknown { Fact::unknown("@Table(indexes) is not read") } else { Fact::Known(Vec::<Index>::new()) };

        (Some(table), Some(built))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::schema::{Change, Danger, diff};

    fn doc(path: &str, text: &str) -> Document {
        Document { path: path.into(), language: "java".into(), version: 1, text: text.into() }
    }

    const CUSTOMER: &str = r#"package shop;
import jakarta.persistence.*;
@Entity
@Table(name = "customers")
public class Customer {
    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;
    @Column(nullable = false, length = 100)
    private String name;
    @Column(unique = true)
    private String email;
}
"#;

    fn order(extra: &str, customer_optional: &str) -> String {
        format!(
            r#"package shop;
import jakarta.persistence.*;
import java.math.BigDecimal;
@Entity
public class Order {{
    @Id
    private Long id;
    @Version
    private Long version;
    @Column(precision = 10, scale = 2)
    private BigDecimal total;
    @Enumerated(EnumType.STRING)
    private Status status;
    @ManyToOne{customer_optional}
    private Customer customer;
    {extra}
}}
"#
        )
    }

    fn snap(docs: &[Document]) -> SchemaSnapshot {
        snapshot_from_entities(docs)
    }

    fn table<'a>(s: &'a SchemaSnapshot, name: &str) -> &'a Table {
        s.tables.iter().find(|t| t.name == name).unwrap()
    }

    fn cols(t: &Table) -> Vec<&Column> {
        t.columns.known().unwrap().iter().collect()
    }

    #[test]
    fn known_naming_strategy_names_derived_tables_and_columns() {
        use crate::naming_strategy::{Implicit, NamingStrategy, Physical};
        let source = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class OrderLine {\n    @Id\n    private Long id;\n    private String customerID;\n    private String shipTo;\n}\n";
        let docs = [doc("/p/OrderLine.java", source)];
        let named = |physical| NamingStrategy { physical: Fact::Known(physical), implicit: Fact::Known(Implicit::Spring), source: "test".into() };

        let camel = snapshot_from_entities_named(&docs, &named(Physical::CamelCaseToUnderscores));
        let t = table(&camel, "order_line");
        assert_eq!(t.provenance, Provenance::Derived);
        assert_eq!(cols(t).iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["id", "customerid", "ship_to"]);

        let standard = snapshot_from_entities_named(&docs, &named(Physical::Standard));
        assert_eq!(cols(table(&standard, "OrderLine")).iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["id", "customerID", "shipTo"]);

        let unknown = snap(&docs);
        assert_eq!(cols(table(&unknown, "order_line")).iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["id", "customer_id", "ship_to"]);
    }

    #[test]
    fn explicit_names_are_logical_and_go_through_a_known_physical_strategy() {
        use crate::naming_strategy::{Implicit, NamingStrategy, Physical};
        let source = "package shop;\nimport jakarta.persistence.*;\n@Entity\n@Table(name = \"GadgetItem\")\npublic class Gadget {\n    @Id\n    private Long id;\n    @Column(name = \"shipTo\")\n    private String address;\n}\n";
        let docs = [doc("/p/Gadget.java", source)];
        let named = |physical| NamingStrategy { physical: Fact::Known(physical), implicit: Fact::Known(Implicit::Spring), source: "test".into() };
        let spring = snapshot_from_entities_named(&docs, &named(Physical::SpringPhysical));
        let t = table(&spring, "gadget_item");
        assert_eq!(t.provenance, Provenance::Literal);
        assert_eq!(cols(t).iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["id", "ship_to"]);
        let standard = snapshot_from_entities_named(&docs, &named(Physical::Standard));
        assert_eq!(cols(table(&standard, "GadgetItem")).iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["id", "shipTo"]);
        let unknown = NamingStrategy::unknown("test");
        assert!(snapshot_from_entities_named(&docs, &unknown).tables.iter().any(|t| t.name == "GadgetItem"), "unknown strategy keeps the literal name");
    }

    #[test]
    fn builds_tables_columns_keys_and_provenance() {
        let s = snap(&[doc("/p/Order.java", &order("", "")), doc("/p/Customer.java", CUSTOMER)]);
        assert!(s.complete);
        assert_eq!(s.tables.iter().map(|t| (t.name.as_str(), t.provenance)).collect::<Vec<_>>(), [("customers", Provenance::Literal), ("order", Provenance::Derived)]);

        let customers = table(&s, "customers");
        let name = cols(customers)[1];
        assert_eq!((name.name.as_str(), name.nullable.clone(), name.length), ("name", Fact::Known(false), Some(100)));
        assert_eq!(cols(customers)[0].generated, Fact::Known(true));
        assert_eq!(customers.primary_key, Fact::Known(vec!["id".into()]));
        assert_eq!(customers.unique_constraints.known().unwrap().len(), 1);

        let o = table(&s, "order");
        let total = cols(o).into_iter().find(|c| c.name == "total").unwrap();
        assert_eq!((total.precision, total.scale), (Some(10), Some(2)));
        let status = cols(o).into_iter().find(|c| c.name == "status").unwrap();
        assert_eq!(status.sql_type.known().unwrap().logical, LogicalType::Varchar);
        assert!(cols(o).iter().any(|c| c.name == "version"));
        let fk = &o.foreign_keys.known().unwrap()[0];
        assert_eq!((fk.columns[0].as_str(), fk.ref_table.as_str(), fk.ref_columns.clone()), ("customer_id", "customers", Fact::Known(vec!["id".into()])));
        let fk_col = cols(o).into_iter().find(|c| c.name == "customer_id").unwrap();
        assert_eq!(fk_col.sql_type.known().unwrap().logical, LogicalType::Bigint);
        assert_eq!(fk_col.nullable, Fact::Known(true));
    }

    #[test]
    fn optional_false_and_literal_join_column() {
        let text = order("", "(optional = false)").replace("private Customer customer;", "@JoinColumn(name = \"buyer_id\")\n    private Customer customer;");
        let s = snap(&[doc("/p/Order.java", &text), doc("/p/Customer.java", CUSTOMER)]);
        let c = cols(table(&s, "order")).into_iter().find(|c| c.name == "buyer_id").unwrap().clone();
        assert_eq!(c.nullable, Fact::Known(false));
    }

    #[test]
    fn unmappable_types_are_unknown_not_guessed() {
        let s = snap(&[doc("/p/Order.java", &order("private java.time.Instant created;\n    private Double ratio;\n    private Money money;", "")), doc("/p/Customer.java", CUSTOMER)]);
        for n in ["created", "ratio", "money"] {
            let c = cols(table(&s, "order")).into_iter().find(|c| c.name == n).unwrap().clone();
            assert!(matches!(c.sql_type, Fact::Unknown(_)), "{n}");
        }
    }

    #[test]
    fn model_diff_add_column_nullable_fk_and_backfill() {
        let v1 = snap(&[doc("/p/Customer.java", CUSTOMER)]);
        let v2_customer = CUSTOMER
            .replace("@Column(unique = true)\n    private String email;", "@Column(unique = true)\n    private String email;\n    @Column(nullable = false)\n    private String phone;")
            .replace("@Column(nullable = false, length = 100)", "@Column(length = 100)");
        let v2 = snap(&[doc("/p/Customer.java", &v2_customer), doc("/p/Order.java", &order("", ""))]);
        let plan = diff(&v1, &v2).unwrap();
        let find = |pred: &dyn Fn(&Change) -> bool| plan.changes.iter().find(|c| pred(&c.change)).unwrap();

        assert_eq!(find(&|c| matches!(c, Change::AddColumn { column, .. } if column.name == "phone")).danger, Danger::NeedsBackfill);
        assert_eq!(find(&|c| matches!(c, Change::AlterNullability { nullable: true, .. })).danger, Danger::Safe);
        assert!(matches!(find(&|c| matches!(c, Change::CreateTable { .. })).change, Change::CreateTable { .. }));

        let v3 = snap(&[doc("/p/Customer.java", &v2_customer), doc("/p/Order.java", &order("", "")), doc("/p/Line.java", "package shop;\n@Entity class Line { @Id Long id; }")]);
        let v4_order = order("", "").replace("@ManyToOne\n    private Customer customer;", "private String note;");
        let v4 = snap(&[doc("/p/Customer.java", &v2_customer), doc("/p/Order.java", &v4_order)]);
        let with_fk = diff(&v4, &v2).unwrap();
        assert!(with_fk.changes.iter().any(|c| matches!(&c.change, Change::AddForeignKey { .. })));
        let _ = v3;
    }

    #[test]
    fn model_diff_rename_candidate_is_not_applied() {
        let before = snap(&[doc("/p/Customer.java", CUSTOMER)]);
        let renamed = CUSTOMER.replace("private String email;", "private String mail;");
        let after = snap(&[doc("/p/Customer.java", &renamed)]);
        let plan = diff(&before, &after).unwrap();
        assert!(plan.changes.iter().any(|c| matches!(&c.change, Change::RenameCandidate { from, to, .. } if from == "email" && to == "mail")));
        assert!(plan.changes.iter().any(|c| matches!(&c.change, Change::DropColumn { column, .. } if column == "email")));
    }

    #[test]
    fn unresolved_superclass_makes_table_facts_unknown() {
        let text = "package shop;\n@Entity\npublic class Invoice extends BaseEntity { @Id Long id; }";
        let s = snap(&[doc("/p/Invoice.java", text)]);
        let t = table(&s, "invoice");
        assert!(matches!(&t.columns, Fact::Unknown(r) if r.contains("BaseEntity")));
        let baseline = snap(&[]);
        let plan = diff(&baseline, &s).unwrap();
        assert!(plan.changes.is_empty() && !plan.notes.is_empty());
    }

    #[test]
    fn provable_inheritance_strategies() {
        let base = "package shop;\n@MappedSuperclass\npublic abstract class Audited { private String createdBy; }";
        let root = |strategy: &str| format!("package shop;\n@Entity\n{strategy}\npublic class Animal extends Audited {{ @Id Long id; String name; }}");
        let dog = "package shop;\n@Entity\npublic class Dog extends Animal { String breed; }";

        let single = snap(&[doc("/p/Audited.java", base), doc("/p/Animal.java", &root("@DiscriminatorColumn(name = \"kind\")")), doc("/p/Dog.java", dog)]);
        assert_eq!(single.tables.len(), 1);
        let names: Vec<_> = cols(table(&single, "animal")).iter().map(|c| c.name.clone()).collect();
        assert_eq!(names, ["created_by", "id", "name", "breed", "kind"]);
        assert_eq!(cols(table(&single, "animal"))[3].nullable, Fact::Known(true));

        let no_discriminator = snap(&[doc("/p/Audited.java", base), doc("/p/Animal.java", &root("")), doc("/p/Dog.java", dog)]);
        assert!(matches!(table(&no_discriminator, "animal").columns, Fact::Unknown(_)));

        let joined = snap(&[doc("/p/Audited.java", base), doc("/p/Animal.java", &root("@Inheritance(strategy = InheritanceType.JOINED)")), doc("/p/Dog.java", dog)]);
        assert_eq!(cols(table(&joined, "animal")).len(), 3);
        let d = table(&joined, "dog");
        assert_eq!(cols(d).iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["id", "breed"]);
        assert_eq!(d.primary_key, Fact::Known(vec!["id".into()]));
        assert_eq!(d.foreign_keys.known().unwrap()[0].ref_table, "animal");

        let per_class = snap(&[doc("/p/Audited.java", base), doc("/p/Animal.java", &root("@Inheritance(strategy = InheritanceType.TABLE_PER_CLASS)")), doc("/p/Dog.java", dog)]);
        assert_eq!(cols(table(&per_class, "dog")).len(), 4);

        let abstract_root = root("@Inheritance(strategy = InheritanceType.TABLE_PER_CLASS)").replace("public class", "public abstract class");
        let abs = snap(&[doc("/p/Audited.java", base), doc("/p/Animal.java", &abstract_root), doc("/p/Dog.java", dog)]);
        assert_eq!(abs.tables.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["dog"]);

        let odd = snap(&[doc("/p/Animal.java", &root("@Inheritance(strategy = STRATEGY)")), doc("/p/Audited.java", base)]);
        assert!(matches!(table(&odd, "animal").columns, Fact::Unknown(_)));
    }

    #[test]
    fn opaque_mappings_and_collections() {
        let embedded = "package shop;\n@Entity\npublic class A { @Id Long id; @Embedded Address address; }";
        let secondary = "package shop;\n@Entity\n@SecondaryTable(name = \"x\")\npublic class B { @Id Long id; }";
        let many = "package shop;\n@Entity\npublic class C { @Id Long id; @ManyToMany List<A> as; }";
        let s = snap(&[doc("/p/A.java", embedded), doc("/p/B.java", secondary), doc("/p/C.java", many)]);
        assert!(matches!(table(&s, "a").columns, Fact::Unknown(_)));
        assert!(matches!(table(&s, "b").columns, Fact::Unknown(_)));
        assert!(table(&s, "c").columns.known().is_some());
        assert!(!s.complete);
    }

    #[test]
    fn kotlin_nullability_is_unknown_unless_annotated() {
        let kt = Document {
            path: "/p/K.kt".into(),
            language: "kotlin".into(),
            version: 1,
            text: "package shop\n@Entity\nclass K(\n @Id var id: Long = 0,\n var note: String?,\n @Column(nullable = false) var label: String)".into(),
        };
        let s = snap(&[kt]);
        let t = cols(table(&s, "k"));
        assert_eq!(t[1].nullable, Fact::unknown("Kotlin nullability is not read"));
        assert_eq!(t[2].nullable, Fact::Known(false));
    }

    #[test]
    fn malformed_sources_do_not_panic_and_mark_incomplete() {
        let junk = ["", "@Entity", "@Entity class", "@Entity public class {", "@Entity interface X {}", "}}}@Entity@@(((", "@Entity\nclass A { @Column(name = \"x\" int a; @Id }"];
        for (i, text) in junk.iter().enumerate() {
            let s = snap(&[doc(&format!("/p/J{i}.java"), text)]);
            assert!(s.validate().is_ok(), "{text}");
        }
        assert!(!snap(&[doc("/p/J.java", "@Entity")]).complete);
        assert!(!snap(&[doc("/p/Big.java", &format!("@Entity class A {{ /* {} */ }}", "x".repeat(MAX_TEXT)))]).complete);
        assert!(snap(&[doc("/p/readme.txt", "@Entity")]).tables.is_empty());
        assert!(snap(&[doc("/p/Plain.java", "class Plain {}")]).complete);
    }

    #[test]
    fn duplicate_classes_and_shared_tables_become_unknown() {
        let s = snap(&[doc("/p/a/Customer.java", CUSTOMER), doc("/p/b/Customer.java", CUSTOMER)]);
        assert!(matches!(table(&s, "customers").columns, Fact::Unknown(_)));

        let other = CUSTOMER.replace("class Customer", "class Client");
        let s = snap(&[doc("/p/Customer.java", CUSTOMER), doc("/p/Client.java", &other)]);
        assert!(matches!(&table(&s, "customers").columns, Fact::Unknown(r) if r.contains("several entities")));
    }

    #[test]
    fn json_export_is_stable_across_input_order_and_round_trips() {
        let a = doc("/p/Order.java", &order("", ""));
        let b = doc("/p/Customer.java", CUSTOMER);
        let one = snapshot_json(&snap(&[a.clone_doc(), b.clone_doc()])).unwrap();
        let two = snapshot_json(&snap(&[b, a])).unwrap();
        assert_eq!(one, two);
        assert_eq!(snapshot_from_json(&one).unwrap(), snap(&[doc("/p/Order.java", &order("", "")), doc("/p/Customer.java", CUSTOMER)]));
        assert!(snapshot_from_json("{").is_err());
        assert!(snapshot_from_json("{\"version\":2,\"tables\":[]}").is_err());
    }

    fn audited(class_extra: &str, field_extra: &str) -> String {
        format!(
            "package shop;\nimport jakarta.persistence.*;\nimport org.hibernate.envers.*;\n@Entity\n@Audited\n{class_extra}\npublic class Invoice {{\n    @Id\n    private Long id;\n    private String title;\n    {field_extra}\n}}\n"
        )
    }

    const REVISION: &str = "package shop;\nimport jakarta.persistence.*;\nimport org.hibernate.envers.*;\n@Entity\n@Table(name = \"TRACKING_REV_INFO\")\n@RevisionEntity\npublic class TrackingRevision {\n    @Id\n    @RevisionNumber\n    private int customId;\n    @RevisionTimestamp\n    private long customTimestamp;\n}\n";

    fn names(t: &Table) -> Vec<String> {
        cols(t).iter().map(|c| c.name.clone()).collect()
    }

    #[test]
    fn audited_entity_gets_audit_table_and_default_revinfo() {
        let s = snap(&[doc("/p/Invoice.java", &audited("", ""))]);
        let aud = table(&s, "invoice_aud");
        assert_eq!(names(aud), ["id", "rev", "revtype", "title"]);
        assert_eq!(aud.primary_key, Fact::Known(vec!["id".into(), "rev".into()]));
        let rev = &cols(aud)[1];
        assert_eq!((rev.nullable.clone(), rev.sql_type.known().unwrap().logical), (Fact::Known(false), LogicalType::Integer));
        let revtype = &cols(aud)[2];
        assert_eq!(revtype.sql_type.known().unwrap().raw, "tinyint");
        assert_eq!(revtype.nullable, Fact::Known(false));
        assert_eq!(cols(aud)[3].nullable, Fact::Known(true));
        let fk = &aud.foreign_keys.known().unwrap()[0];
        assert_eq!((fk.columns.clone(), fk.ref_table.as_str(), fk.ref_columns.clone()), (vec!["rev".to_owned()], "revinfo", Fact::Known(vec!["rev".to_owned()])));

        let info = table(&s, "revinfo");
        assert_eq!(names(info), ["rev", "revtstmp"]);
        assert_eq!(info.primary_key, Fact::Known(vec!["rev".into()]));
        assert!(matches!(cols(info)[0].generated, Fact::Unknown(_)));
        assert!(s.complete);
        assert!(snapshot_from_entities_scoped(&[doc("/p/Invoice.java", &audited("", ""))], &Scope::All).notes.iter().any(|n| n.contains("REVINFO_SEQ")));
    }

    fn configured(applications: &[(&str, &str)], xml: &[(&str, &str)]) -> SchemaSnapshot {
        let own = |items: &[(&str, &str)]| items.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
        let config = EnversConfig::from_sources(&own(applications), &own(xml));
        snapshot_configured(&[doc("/p/Invoice.java", &audited("", ""))], &Scope::All, &NamingStrategy::unknown("test"), &config).snapshot
    }

    #[test]
    fn audit_table_suffix_and_prefix_come_from_literal_configuration() {
        const APP: &str = "/p/src/main/resources/application.properties";
        let suffix = "spring.jpa.properties.org.hibernate.envers.audit_table_suffix=_history\n";
        let s = configured(&[(APP, suffix)], &[]);
        assert_eq!(table(&s, "invoice_history").columns.known().map(|c| c.len()), Some(4));
        assert!(!s.tables.iter().any(|t| t.name == "invoice_aud"));

        let yaml = "spring:\n  jpa:\n    properties:\n      org.hibernate.envers.audit_table_suffix: _log\n      org.hibernate.envers.audit_table_prefix: h_\n";
        let s = configured(&[("/p/src/main/resources/application.yml", yaml)], &[]);
        assert!(s.tables.iter().any(|t| t.name == "h_invoice_log" && t.columns.known().is_some()), "{:?}", s.tables.iter().map(|t| &t.name).collect::<Vec<_>>());

        let xml = "<persistence><persistence-unit name=\"x\"><properties><property name=\"org.hibernate.envers.audit_table_suffix\" value=\"_rev\"/></properties></persistence-unit></persistence>";
        assert!(configured(&[], &[("/p/persistence.xml", xml)]).tables.iter().any(|t| t.name == "invoice_rev"));
        assert!(configured(&[(APP, "spring.jpa.show-sql=true\n")], &[]).tables.iter().any(|t| t.name == "invoice_aud"));
    }

    #[test]
    fn conflicting_or_unreadable_audit_table_names_stay_unknown() {
        const APP: &str = "/p/src/main/resources/application.properties";
        let key = "spring.jpa.properties.org.hibernate.envers.audit_table_suffix";
        let xml = "<property name=\"org.hibernate.envers.audit_table_suffix\" value=\"_b\"/>";
        for conflict in [configured(&[(APP, &format!("{key}=_a\n"))], &[("/p/persistence.xml", xml)]), configured(&[(APP, &format!("{key}=${{SUFFIX}}\n"))], &[]), configured(&[("/p/src/main/resources/application-dev.properties", &format!("{key}=_a\n"))], &[])] {
            let aud = conflict.tables.iter().find(|t| t.name.starts_with("invoice_")).unwrap();
            assert!(matches!(aud.columns, Fact::Unknown(_)), "{aud:?}");
        }
        let schema = configured(&[(APP, "spring.jpa.properties.org.hibernate.envers.default_schema=audit\n")], &[]);
        assert!(matches!(table(&schema, "invoice_aud").columns, Fact::Unknown(_)));
    }

    #[test]
    fn audited_entities_migrate_their_audit_tables_on_postgresql() {
        use persistence_core::sql::{Dialect, EmitOptions, emit};
        let before = snap(&[doc("/p/Invoice.java", &audited("", ""))]);
        let after = snap(&[doc("/p/Invoice.java", &audited("", "private String note;"))]);
        let sql = emit(&persistence_core::schema::diff(&before, &after).unwrap(), Dialect::PostgreSql, &EmitOptions::default()).unwrap();
        let text = sql.render();
        assert!(text.contains("ALTER TABLE \"invoice\" ADD COLUMN \"note\"") && text.contains("ALTER TABLE \"invoice_aud\" ADD COLUMN \"note\" varchar;"), "{text}");

        let plain = snap(&[doc("/p/Invoice.java", &audited("", "").replace("@Audited\n", ""))]);
        let created = emit(&persistence_core::schema::diff(&plain, &before).unwrap(), Dialect::PostgreSql, &EmitOptions::default()).unwrap().render();
        assert!(created.contains("CREATE TABLE \"invoice_aud\"") && created.contains("\"revtype\" smallint NOT NULL"), "{created}");
        assert!(created.contains("CREATE TABLE \"revinfo\""), "{created}");
    }

    #[test]
    fn plain_entities_and_foreign_audited_annotations_add_no_audit_tables() {
        let plain = audited("", "").replace("@Audited\n", "");
        let foreign = audited("", "").replace("import org.hibernate.envers.*;", "import other.envers.*;");
        for text in [plain, foreign] {
            let s = snap(&[doc("/p/Invoice.java", &text)]);
            assert_eq!(s.tables.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["invoice"]);
        }
    }

    #[test]
    fn not_audited_field_is_left_out_and_field_level_audited_selects_columns() {
        let s = snap(&[doc("/p/Invoice.java", &audited("", "@NotAudited\n    private String secret;"))]);
        assert_eq!(names(table(&s, "invoice_aud")), ["id", "rev", "revtype", "title"]);
        assert!(names(table(&s, "invoice")).contains(&"secret".to_owned()));

        let field_only = audited("", "@Audited\n    private String tracked;").replace("@Entity\n@Audited\n", "@Entity\n").replace("    private String title;\n", "    private String title;\n");
        let s = snap(&[doc("/p/Invoice.java", &field_only)]);
        let aud = table(&s, "invoice_aud");
        assert_eq!(names(aud), ["id", "rev", "revtype", "tracked"]);
        assert!(matches!(aud.primary_key, Fact::Unknown(_)));
    }

    #[test]
    fn audit_table_literal_is_known_and_everything_else_is_unknown() {
        let literal = snap(&[doc("/p/Invoice.java", &audited("@AuditTable(\"invoice_history\")", ""))]);
        let t = table(&literal, "invoice_history");
        assert_eq!(t.provenance, Provenance::Literal);
        assert!(t.columns.known().is_some());

        for extra in ["@AuditTable(value = \"h\", schema = \"s\")", "@AuditTable(Names.AUDIT)"] {
            let s = snap(&[doc("/p/Invoice.java", &audited(extra, ""))]);
            assert!(matches!(table(&s, "invoice_aud").columns, Fact::Unknown(_)), "{extra}");
        }

        let properties = doc("/p/Config.java", "class Config { String s = \"org.hibernate.envers.audit_table_suffix\"; }");
        let s = snap(&[doc("/p/Invoice.java", &audited("", "")), properties]);
        assert!(matches!(&table(&s, "invoice_aud").columns, Fact::Unknown(r) if r.contains("audit_table_suffix")));

        let strategy = doc("/p/Config2.java", "class C { String s = \"org.hibernate.envers.audit_strategy\"; }");
        let s = snap(&[doc("/p/Invoice.java", &audited("", "")), strategy]);
        assert!(matches!(table(&s, "invoice_aud").columns, Fact::Unknown(_)));
        assert!(matches!(table(&audited_literal_with_args(), "invoice_aud").columns, Fact::Unknown(_)));
    }

    fn audited_literal_with_args() -> SchemaSnapshot {
        snap(&[doc("/p/Invoice.java", &audited("", "").replace("@Audited\n", "@Audited(withModifiedFlag = true)\n"))])
    }

    #[test]
    fn custom_revision_entity_replaces_revinfo() {
        let s = snap(&[doc("/p/Invoice.java", &audited("", "")), doc("/p/TrackingRevision.java", REVISION)]);
        assert!(s.tables.iter().all(|t| t.name != "revinfo"));
        let aud = table(&s, "invoice_aud");
        let fk = &aud.foreign_keys.known().unwrap()[0];
        assert_eq!((fk.ref_table.as_str(), fk.ref_columns.clone()), ("TRACKING_REV_INFO", Fact::Known(vec!["custom_id".to_owned()])));
        assert_eq!(cols(aud)[1].sql_type.known().unwrap().logical, LogicalType::Integer);
        assert!(table(&s, "TRACKING_REV_INFO").columns.known().is_some());

        let inherited = REVISION.replace("@Id\n    @RevisionNumber\n    private int customId;", "").replace("@RevisionTimestamp\n    private long customTimestamp;", "");
        let s = snap(&[doc("/p/Invoice.java", &audited("", "")), doc("/p/TrackingRevision.java", &inherited)]);
        let fk = &table(&s, "invoice_aud").foreign_keys.known().unwrap()[0];
        assert!(matches!(fk.ref_columns, Fact::Unknown(_)));
        assert!(matches!(cols(table(&s, "invoice_aud"))[1].sql_type, Fact::Unknown(_)));

        let second = REVISION.replace("TrackingRevision", "OtherRevision").replace("TRACKING_REV_INFO", "OTHER");
        let s = snap(&[doc("/p/Invoice.java", &audited("", "")), doc("/p/A.java", REVISION), doc("/p/B.java", &second)]);
        assert!(!s.complete);
        assert!(matches!(table(&s, "invoice_aud").foreign_keys, Fact::Unknown(_)));
    }

    #[test]
    fn audited_inheritance_and_collections_stay_unknown_or_incomplete() {
        let base = "package shop;\nimport jakarta.persistence.*;\n@MappedSuperclass\npublic abstract class Base { private String createdBy; }";
        let child = audited("", "").replace("class Invoice", "class Invoice extends Base");
        let s = snap(&[doc("/p/Base.java", base), doc("/p/Invoice.java", &child)]);
        assert!(matches!(table(&s, "invoice_aud").columns, Fact::Unknown(_)));

        let many = audited("", "@ManyToMany\n    private java.util.List<Invoice> related;");
        let scoped = snapshot_from_entities_scoped(&[doc("/p/Invoice.java", &many)], &Scope::All);
        assert!(!scoped.snapshot.complete);
        assert!(scoped.notes.iter().any(|n| n.contains("collections")));
    }

    fn two_entities() -> Vec<Document> {
        vec![doc("/p/Customer.java", CUSTOMER), doc("/p/Invoice.java", &audited("", ""))]
    }

    #[test]
    fn unit_scope_keeps_only_listed_entities() {
        let scope = Scope::Unit { name: "main".into(), classes: vec!["shop.Customer".into()], exclude_unlisted: true };
        let scoped = snapshot_from_entities_scoped(&two_entities(), &scope);
        assert_eq!(scoped.snapshot.tables.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["customers"]);
        assert!(scoped.snapshot.complete);
        assert!(scoped.notes.is_empty());
        assert_eq!(snapshot_from_entities_scoped(&two_entities(), &Scope::All).snapshot.tables.len(), 4);
    }

    #[test]
    fn unit_scope_is_incomplete_for_missing_classes_or_open_unit() {
        let missing = Scope::Unit { name: "main".into(), classes: vec!["shop.Customer".into(), "shop.Ghost".into()], exclude_unlisted: true };
        let scoped = snapshot_from_entities_scoped(&two_entities(), &missing);
        assert!(!scoped.snapshot.complete);
        assert!(scoped.notes.iter().any(|n| n.contains("shop.Ghost")));

        let open = Scope::Unit { name: "main".into(), classes: vec!["shop.Customer".into()], exclude_unlisted: false };
        let scoped = snapshot_from_entities_scoped(&two_entities(), &open);
        assert!(!scoped.snapshot.complete);
        assert!(scoped.notes.iter().any(|n| n.contains("exclude-unlisted-classes")));
    }

    #[test]
    fn scope_reads_persistence_xml_units() {
        let xml = "<persistence><persistence-unit name=\"main\"><class>shop.Customer</class><exclude-unlisted-classes/></persistence-unit><persistence-unit name=\"other\"><class>shop.Invoice</class><exclude-unlisted-classes>false</exclude-unlisted-classes></persistence-unit></persistence>";
        assert_eq!(
            Scope::from_persistence_xml("/p/persistence.xml", xml, "main"),
            Some(Scope::Unit { name: "main".into(), classes: vec!["shop.Customer".into()], exclude_unlisted: true })
        );
        assert!(matches!(Scope::from_persistence_xml("/p/persistence.xml", xml, "other"), Some(Scope::Unit { exclude_unlisted: false, .. })));
        assert_eq!(Scope::from_persistence_xml("/p/persistence.xml", xml, "none"), None);
    }

    #[test]
    fn baseline_json_records_header_and_older_files_still_load() {
        let scoped = snapshot_from_entities_scoped(&two_entities(), &Scope::All);
        let unit = Scope::Unit { name: "main".into(), classes: vec!["shop.Invoice".into(), "shop.Customer".into()], exclude_unlisted: true };
        let options = BaselineOptions { scope: &unit, notes: &scoped.notes };
        let json = baseline_json(&scoped.snapshot, &options).unwrap();
        assert_eq!(json, baseline_json(&scoped.snapshot, &options).unwrap());

        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["generatedBy"], BASELINE_GENERATED_BY);
        assert_eq!(value["scope"]["classes"], serde_json::json!(["shop.Customer", "shop.Invoice"]));
        assert_eq!(snapshot_from_json(&json).unwrap(), scoped.snapshot);

        let plain = snapshot_json(&scoped.snapshot).unwrap();
        assert!(!plain.contains("generatedBy"));
        assert_eq!(snapshot_from_json(&plain).unwrap(), scoped.snapshot);
        assert!(snapshot_from_json("{\"version\":1,\"tables\":[],\"surprise\":1}").is_err());
    }

    trait CloneDoc {
        fn clone_doc(&self) -> Document;
    }

    impl CloneDoc for Document {
        fn clone_doc(&self) -> Document {
            Document { path: self.path.clone(), language: self.language.clone(), version: self.version, text: self.text.clone() }
        }
    }
}
