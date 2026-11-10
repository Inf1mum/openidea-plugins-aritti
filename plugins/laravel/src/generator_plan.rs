//! Deterministic native model generation. No filesystem, PHP or clock access.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Draft {
    pub name: String,
    pub table: String,
    #[serde(default)]
    pub connection: Option<String>,
    #[serde(default = "namespace")]
    pub namespace: String,
    #[serde(default = "source_root")]
    pub source_root: String,
    #[serde(default = "migration_root")]
    pub migration_root: String,
    /// Supplied once by the invoking session; preview and apply use the same value.
    pub migration_stamp: String,
    #[serde(default = "yes")]
    pub timestamps: bool,
    #[serde(default)]
    pub timezone: bool,
    #[serde(default)]
    pub primary_key: PrimaryKey,
    #[serde(default)]
    pub soft_deletes: bool,
    #[serde(default)]
    pub fields: Vec<Field>,
    /// Omitted means the historical default; an explicit empty list emits no entries.
    #[serde(default)]
    pub factory_fields: Option<Vec<String>>,
    #[serde(default)]
    pub resource_fields: Option<Vec<String>>,
    #[serde(default)]
    pub resource_relations: Option<Vec<String>>,
    #[serde(default)]
    pub relations: Vec<Relation>,
    #[serde(default)]
    pub indexes: Vec<Index>,
    #[serde(default)]
    pub related: BTreeSet<String>,
    #[serde(default)]
    pub settings: GenerationSettings,
    #[serde(default)]
    pub layout: RelatedLayout,
    #[serde(default)]
    pub destinations: BTreeMap<String, Destination>,
    #[serde(default = "yes")]
    pub use_project_settings: bool,
}
pub(crate) const KINDS: &[&str] = &[
    "model",
    "migration",
    "factory",
    "seeder",
    "controller",
    "policy",
    "request",
    "resource",
];
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Destination {
    pub directory: String,
    #[serde(default)]
    pub namespace: String,
}
impl Draft {
    pub(crate) fn directory(&self, kind: &str) -> String {
        if let Some(destination) = self.destinations.get(kind) {
            return destination.directory.clone();
        }
        match kind {
            "migration" => self.migration_root.clone(),
            "factory" => self.layout.factory_root.clone(),
            "seeder" => self.layout.seeder_root.clone(),
            _ => format!(
                "{}/{}",
                self.source_root,
                match kind {
                    "model" => "Models",
                    "controller" => "Http/Controllers",
                    "policy" => "Policies",
                    "request" => "Http/Requests",
                    "resource" => "Http/Resources",
                    _ => "",
                }
            ),
        }
    }
    pub(crate) fn namespace_for(&self, kind: &str) -> String {
        if let Some(destination) = self.destinations.get(kind) {
            return destination.namespace.clone();
        }
        match kind {
            "factory" => self.layout.factory_namespace.clone(),
            "seeder" => self.layout.seeder_namespace.clone(),
            _ => format!(
                "{}\\{}",
                self.namespace,
                match kind {
                    "model" => "Models",
                    "controller" => "Http\\Controllers",
                    "policy" => "Policies",
                    "request" => "Http\\Requests",
                    "resource" => "Http\\Resources",
                    _ => "",
                }
            ),
        }
    }
    pub(crate) fn class_name(&self, kind: &str) -> String {
        let postfix = match kind {
            "controller" => &self.settings.controller_postfix,
            "policy" => &self.settings.policy_postfix,
            "request" => &self.settings.request_postfix,
            "resource" => &self.settings.resource_postfix,
            "factory" => &self.settings.factory_postfix,
            "seeder" => &self.settings.seeder_postfix,
            _ => "",
        };
        format!(
            "{}{}{postfix}",
            if kind == "request" { "Store" } else { "" },
            self.name
        )
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub(crate) struct RelatedLayout {
    pub factory_root: String,
    pub factory_namespace: String,
    pub seeder_root: String,
    pub seeder_namespace: String,
}
impl Default for RelatedLayout {
    fn default() -> Self {
        Self {
            factory_root: "database/factories".into(),
            factory_namespace: "Database\\Factories".into(),
            seeder_root: "database/seeders".into(),
            seeder_namespace: "Database\\Seeders".into(),
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub(crate) struct GenerationSettings {
    pub model_base: String,
    pub controller_base: String,
    pub policy_base: Option<String>,
    pub request_base: String,
    pub resource_base: String,
    pub factory_base: String,
    pub seeder_base: String,
    pub controller_postfix: String,
    pub policy_postfix: String,
    pub request_postfix: String,
    pub resource_postfix: String,
    pub factory_postfix: String,
    pub seeder_postfix: String,
    pub type_hints: bool,
    pub constructor_promotion: bool,
    pub controller_dependencies: Vec<Dependency>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Dependency {
    pub name: String,
    pub class: String,
}
impl Default for GenerationSettings {
    fn default() -> Self {
        Self {
            model_base: "Illuminate\\Database\\Eloquent\\Model".into(),
            controller_base: "Illuminate\\Routing\\Controller".into(),
            policy_base: None,
            request_base: "Illuminate\\Foundation\\Http\\FormRequest".into(),
            resource_base: "Illuminate\\Http\\Resources\\Json\\JsonResource".into(),
            factory_base: "Illuminate\\Database\\Eloquent\\Factories\\Factory".into(),
            seeder_base: "Illuminate\\Database\\Seeder".into(),
            controller_postfix: "Controller".into(),
            policy_postfix: "Policy".into(),
            request_postfix: "Request".into(),
            resource_postfix: "Resource".into(),
            factory_postfix: "Factory".into(),
            seeder_postfix: "Seeder".into(),
            type_hints: true,
            constructor_promotion: true,
            controller_dependencies: Vec::new(),
        }
    }
}
impl GenerationSettings {
    pub(crate) fn validate(&self) -> Result<(), String> {
        for base in [
            &self.model_base,
            &self.controller_base,
            &self.request_base,
            &self.resource_base,
            &self.factory_base,
            &self.seeder_base,
        ]
        .into_iter()
        .chain(self.policy_base.as_ref())
        {
            if !qualified(base) {
                return Err(format!("Invalid generator base class {base}"));
            }
        }
        for postfix in [
            &self.controller_postfix,
            &self.policy_postfix,
            &self.request_postfix,
            &self.resource_postfix,
            &self.factory_postfix,
            &self.seeder_postfix,
        ] {
            if !postfix.is_empty() && !identifier(postfix) {
                return Err("Generator postfixes must be PHP identifier suffixes".into());
            }
        }
        if self.controller_dependencies.len() > 16 {
            return Err("Choose at most 16 constructor dependencies".into());
        }
        let mut names = BTreeSet::new();
        for dependency in &self.controller_dependencies {
            if !identifier(&dependency.name)
                || matches!(dependency.name.as_str(), "this" | "GLOBALS")
                || !qualified(&dependency.class)
                || !names.insert(&dependency.name)
            {
                return Err(
                    "Constructor dependencies need distinct names and fully qualified types".into(),
                );
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PrimaryKey {
    #[serde(default = "key_name")]
    pub name: String,
    #[serde(default = "key_kind")]
    pub kind: String,
    #[serde(default)]
    pub length: Option<u32>,
}
fn key_name() -> String {
    "id".into()
}
fn key_kind() -> String {
    "bigIncrements".into()
}
impl Default for PrimaryKey {
    fn default() -> Self {
        Self {
            name: key_name(),
            kind: key_kind(),
            length: None,
        }
    }
}
fn namespace() -> String {
    "App".into()
}
fn source_root() -> String {
    "app".into()
}
fn migration_root() -> String {
    "database/migrations".into()
}
fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Field {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub nullable: bool,
    #[serde(default)]
    pub length: Option<u32>,
    #[serde(default)]
    pub precision: Option<u32>,
    #[serde(default)]
    pub scale: Option<u32>,
    #[serde(
        default,
        deserialize_with = "explicit_default",
        skip_serializing_if = "Option::is_none"
    )]
    pub default: Option<Value>,
    #[serde(default)]
    pub cast: Option<String>,
    #[serde(default)]
    pub values: Vec<String>,
}
fn explicit_default<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Relation {
    pub name: String,
    pub kind: String,
    pub related: String,
    #[serde(default)]
    pub through: Option<String>,
    /// Explicit trailing string arguments: foreign/local keys, pivot table, morph name.
    #[serde(default)]
    pub arguments: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Index {
    pub name: String,
    pub columns: Vec<String>,
    #[serde(default)]
    pub unique: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Output {
    pub path: String,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Plan {
    pub files: Vec<Output>,
}

pub(crate) fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 96
        && s.bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || i > 0 && b.is_ascii_digit())
}
fn class_identifier(s: &str) -> bool {
    identifier(s)
        && ![
            "class",
            "trait",
            "interface",
            "enum",
            "function",
            "const",
            "extends",
            "implements",
            "abstract",
            "final",
            "static",
            "self",
            "parent",
            "namespace",
            "use",
            "return",
            "new",
            "match",
            "default",
            "true",
            "false",
            "null",
            "int",
            "float",
            "string",
            "bool",
            "array",
            "object",
            "mixed",
            "never",
            "void",
            "resource",
            "callable",
            "iterable",
            "readonly",
        ]
        .contains(&s.to_ascii_lowercase().as_str())
}
pub(crate) fn qualified(s: &str) -> bool {
    !s.is_empty() && s.trim_start_matches('\\').split('\\').all(class_identifier)
}
pub(crate) fn relative(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && s.split('/').all(|p| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        })
}
fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}
fn php_value(value: &Value) -> Result<String, String> {
    match value {
        Value::Null => Ok("null".into()),
        Value::Bool(v) => Ok(v.to_string()),
        Value::Number(v) => Ok(v.to_string()),
        Value::String(v) if v.len() <= 1024 && !v.contains('\0') => Ok(literal(v)),
        _ => Err("Defaults must be a JSON string, number, boolean or null".into()),
    }
}
fn header(ns: &str, body: &str) -> String {
    format!("<?php\n\ndeclare(strict_types=1);\n\nnamespace {ns};\n\n{body}\n")
}
pub(crate) fn field_line(f: &Field) -> Result<String, String> {
    if !identifier(&f.name) {
        return Err("Invalid field name".into());
    }
    let types = [
        "string",
        "text",
        "char",
        "tinyText",
        "mediumText",
        "longText",
        "tinyInteger",
        "smallInteger",
        "mediumInteger",
        "integer",
        "bigInteger",
        "unsignedTinyInteger",
        "unsignedSmallInteger",
        "unsignedMediumInteger",
        "unsignedInteger",
        "unsignedBigInteger",
        "boolean",
        "decimal",
        "float",
        "double",
        "date",
        "time",
        "timeTz",
        "year",
        "dateTime",
        "dateTimeTz",
        "timestamp",
        "timestampTz",
        "json",
        "jsonb",
        "uuid",
        "ulid",
        "binary",
        "enum",
        "set",
        "ipAddress",
        "macAddress",
    ];
    if !types.contains(&f.kind.as_str()) {
        return Err(format!("Unsupported field type {}", f.kind));
    }
    let mut args = literal(&f.name);
    if let Some(length) = f.length {
        if !matches!(f.kind.as_str(), "string" | "char") || length == 0 || length > 65535 {
            return Err("Length applies to string/char fields and must be 1–65535".into());
        }
        args.push_str(&format!(", {length}"));
    }
    if f.kind == "decimal" {
        let precision = f.precision.unwrap_or(10);
        let scale = f.scale.unwrap_or(2);
        if precision == 0 || precision > 65 || scale > precision || scale > 30 {
            return Err("Invalid decimal precision/scale".into());
        }
        args.push_str(&format!(", {precision}, {scale}"));
    } else if matches!(
        f.kind.as_str(),
        "time" | "timeTz" | "dateTime" | "dateTimeTz" | "timestamp" | "timestampTz"
    ) {
        if f.scale.is_some() || f.precision.is_some_and(|p| p > 6) {
            return Err("Temporal precision must be 0–6 with no scale".into());
        }
        if let Some(p) = f.precision {
            args.push_str(&format!(", {p}"));
        }
    } else if f.precision.is_some() || f.scale.is_some() {
        return Err("Precision and scale apply to decimal fields".into());
    }
    if matches!(f.kind.as_str(), "enum" | "set") {
        if f.values.is_empty()
            || f.values.len() > 100
            || f.values.iter().any(|v| {
                v.is_empty()
                    || v.len() > 256
                    || v.contains('\0')
                    || f.kind == "set" && v.contains(',')
            })
            || f.values.iter().collect::<BTreeSet<_>>().len() != f.values.len()
        {
            return Err("Enum/set fields need 1–100 distinct nonempty values".into());
        }
        args.push_str(&format!(
            ", [{}]",
            f.values
                .iter()
                .map(|v| literal(v))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    } else if !f.values.is_empty() {
        return Err("Values apply only to enum/set fields".into());
    }
    let mut line = format!("            $table->{}({args})", f.kind);
    if f.nullable {
        line.push_str("->nullable()");
    }
    if let Some(value) = &f.default {
        if value.is_null() && !f.nullable {
            return Err(format!("{} requires nullable for a null default", f.name));
        }
        if !value.is_null() && matches!(f.kind.as_str(), "enum" | "set") {
            let value = value
                .as_str()
                .ok_or_else(|| "Enum/set defaults must be strings".to_owned())?;
            let valid = if f.kind == "enum" {
                f.values.iter().any(|v| v == value)
            } else {
                value
                    .split(',')
                    .all(|part| f.values.iter().any(|v| v == part))
            };
            if !valid {
                return Err("Enum/set default is not among the declared values".into());
            }
        }
        line.push_str(&format!("->default({})", php_value(value)?));
    }
    line.push_str(";\n");
    Ok(line)
}
pub(crate) fn cast(f: &Field) -> Result<Option<String>, String> {
    let Some(cast) = &f.cast else { return Ok(None) };
    let core = [
        "integer",
        "int",
        "real",
        "float",
        "double",
        "decimal:2",
        "string",
        "boolean",
        "bool",
        "object",
        "array",
        "collection",
        "date",
        "datetime",
        "immutable_date",
        "immutable_datetime",
        "timestamp",
        "encrypted",
        "encrypted:array",
        "encrypted:collection",
        "encrypted:object",
        "json:unicode",
        "hashed",
    ];
    let parameterized = cast
        .split_once(':')
        .is_some_and(|(name, value)| match name {
            "decimal" => value.parse::<u32>().is_ok_and(|v| v <= 30),
            "date" | "datetime" | "immutable_date" | "immutable_datetime" => {
                !value.is_empty() && value.len() <= 96 && !value.chars().any(char::is_control)
            }
            _ => false,
        });
    if core.contains(&cast.as_str()) || parameterized {
        Ok(Some(literal(cast)))
    } else if qualified(cast) {
        Ok(Some(format!("\\{}::class", cast.trim_start_matches('\\'))))
    } else {
        Err(format!("Invalid cast {cast}"))
    }
}

fn integer_kind(kind: &str) -> bool {
    kind == "integer" || kind.ends_with("Integer")
}
fn factory_value(f: &Field) -> String {
    if let Some(default) = &f.default {
        return php_value(default).expect("validated literal default");
    }
    if f.nullable {
        return "null".into();
    }
    match f.kind.as_str() {
        kind if integer_kind(kind) => "fake()->numberBetween(1, 100)".into(),
        "boolean" => "fake()->boolean()".into(),
        "decimal" => format!("fake()->randomFloat({}, 0, 100)", f.scale.unwrap_or(2)),
        "float" | "double" => "fake()->randomFloat(2, 0, 1000)".into(),
        "date" => "fake()->date()".into(),
        "time" | "timeTz" => "fake()->time()".into(),
        "year" => "fake()->numberBetween(2000, 2030)".into(),
        "dateTime" | "dateTimeTz" | "timestamp" | "timestampTz" => "now()".into(),
        "json" | "jsonb" => {
            if matches!(
                f.cast.as_deref(),
                Some("array" | "collection" | "encrypted:array" | "encrypted:collection")
            ) {
                "[]".into()
            } else if matches!(f.cast.as_deref(), Some("object" | "encrypted:object")) {
                "(object) []".into()
            } else {
                "json_encode([])".into()
            }
        }
        "uuid" => "fake()->uuid()".into(),
        "ulid" => "(string) \\Illuminate\\Support\\Str::ulid()".into(),
        "enum" => format!(
            "fake()->randomElement([{}])",
            f.values
                .iter()
                .map(|v| literal(v))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        "set" => literal(&f.values[0]),
        "ipAddress" => "fake()->ipv4()".into(),
        "macAddress" => "fake()->macAddress()".into(),
        "binary" => "random_bytes(16)".into(),
        _ => format!(
            "fake()->lexify({})",
            literal(&"?".repeat(f.length.unwrap_or(16).min(16) as usize))
        ),
    }
}
fn output_selection(
    selection: &Option<Vec<String>>,
    allowed: &BTreeSet<String>,
    label: &str,
) -> Result<(), String> {
    if let Some(selection) = selection {
        if selection.len() > allowed.len()
            || selection.iter().any(|name| !allowed.contains(name))
            || selection.iter().collect::<BTreeSet<_>>().len() != selection.len()
        {
            return Err(format!(
                "{label} must select unique names from this draft's fields/relations"
            ));
        }
    }
    Ok(())
}
fn selected(selection: &Option<Vec<String>>, name: &str) -> bool {
    selection
        .as_ref()
        .is_none_or(|items| items.iter().any(|item| item == name))
}
pub(crate) fn plan(d: &Draft) -> Result<Plan, String> {
    d.settings.validate()?;
    if d.connection.as_ref().is_some_and(|name| {
        name.is_empty() || name.chars().count() > 128 || name.chars().any(char::is_control)
    }) {
        return Err("Database connection names need 1–128 characters without controls".into());
    }
    for (kind, destination) in &d.destinations {
        if !KINDS.contains(&kind.as_str())
            || !relative(&destination.directory)
            || kind != "migration"
                && (!qualified(&destination.namespace) || destination.namespace.starts_with('\\'))
            || kind == "migration" && !destination.namespace.is_empty()
        {
            return Err("Invalid per-generator destination".into());
        }
    }
    if !relative(&d.layout.factory_root)
        || !relative(&d.layout.seeder_root)
        || !qualified(&d.layout.factory_namespace)
        || !qualified(&d.layout.seeder_namespace)
        || d.layout.factory_namespace.starts_with('\\')
        || d.layout.seeder_namespace.starts_with('\\')
    {
        return Err("Invalid related-file roots or namespaces".into());
    }
    let mut generated_classes = BTreeSet::new();
    for kind in std::iter::once("model")
        .chain(d.related.iter().map(String::as_str))
        .filter(|kind| *kind != "migration")
    {
        let class =
            format!("{}\\{}", d.namespace_for(kind), d.class_name(kind)).to_ascii_lowercase();
        if !generated_classes.insert(class) {
            return Err("Per-generator namespaces/postfixes produce duplicate PHP classes".into());
        }
    }
    let settings = &d.settings;
    let model_base = format!("\\{}", settings.model_base.trim_start_matches('\\'));
    let controller_base = format!("\\{}", settings.controller_base.trim_start_matches('\\'));
    let request_base = format!("\\{}", settings.request_base.trim_start_matches('\\'));
    let resource_base = format!("\\{}", settings.resource_base.trim_start_matches('\\'));
    let factory_base = format!("\\{}", settings.factory_base.trim_start_matches('\\'));
    let seeder_base = format!("\\{}", settings.seeder_base.trim_start_matches('\\'));
    let policy_extends = settings
        .policy_base
        .as_ref()
        .map(|base| format!(" extends \\{}", base.trim_start_matches('\\')))
        .unwrap_or_default();
    let controller_name = format!("{}{}", d.name, settings.controller_postfix);
    let factory_name = format!("{}{}", d.name, settings.factory_postfix);
    let resource_name = format!("{}{}", d.name, settings.resource_postfix);
    let request_name = format!("Store{}{}", d.name, settings.request_postfix);
    let policy_name = format!("{}{}", d.name, settings.policy_postfix);
    let seeder_name = format!("{}{}", d.name, settings.seeder_postfix);
    for name in [
        &controller_name,
        &factory_name,
        &resource_name,
        &request_name,
        &policy_name,
        &seeder_name,
    ] {
        if !class_identifier(name) {
            return Err("Configured generated class name is invalid or too long".into());
        }
    }
    let mut constructor = String::new();
    if !settings.controller_dependencies.is_empty() {
        if !settings.constructor_promotion {
            for dep in &settings.controller_dependencies {
                constructor.push_str(&format!(
                    "    protected \\{} ${};\n",
                    dep.class.trim_start_matches('\\'),
                    dep.name
                ));
            }
            constructor.push('\n');
        }
        constructor.push_str("    public function __construct(\n");
        for dep in &settings.controller_dependencies {
            constructor.push_str(&format!(
                "        {}\\{} ${},\n",
                if settings.constructor_promotion {
                    "protected "
                } else {
                    ""
                },
                dep.class.trim_start_matches('\\'),
                dep.name
            ));
        }
        constructor.push_str("    ) {\n");
        if !settings.constructor_promotion {
            for dep in &settings.controller_dependencies {
                constructor.push_str(&format!("        $this->{} = ${};\n", dep.name, dep.name));
            }
        }
        constructor.push_str("    }\n");
    }

    if !class_identifier(&d.name)
        || d.table.split('.').count() > 2
        || !d.table.split('.').all(identifier)
        || !qualified(&d.namespace)
        || d.namespace.starts_with('\\')
        || !d.namespace.split('\\').all(class_identifier)
    {
        return Err("Model name, table and namespace must be valid PHP identifiers".into());
    }
    if !relative(&d.source_root) || !relative(&d.migration_root) {
        return Err("Generation roots must be relative project paths".into());
    }
    if d.migration_stamp.len() != 17
        || !d.migration_stamp.bytes().enumerate().all(|(i, b)| {
            if [4, 7, 10].contains(&i) {
                b == b'_'
            } else {
                b.is_ascii_digit()
            }
        })
    {
        return Err("Migration timestamp must use YYYY_MM_DD_HHMMSS".into());
    }
    if d.fields.len() > 128 || d.relations.len() > 64 || d.indexes.len() > 64 {
        return Err("Too many fields, relations or indexes".into());
    }
    let allowed = [
        "migration",
        "factory",
        "seeder",
        "controller",
        "policy",
        "request",
        "resource",
    ];
    if d.related.iter().any(|s| !allowed.contains(&s.as_str())) {
        return Err("Unknown related file selection".into());
    }
    let key = &d.primary_key;
    if !identifier(&key.name)
        || ![
            "bigIncrements",
            "increments",
            "uuid",
            "ulid",
            "string",
            "integer",
            "bigInteger",
            "unsignedBigInteger",
            "unsignedInteger",
            "unsignedTinyInteger",
            "unsignedSmallInteger",
            "unsignedMediumInteger",
            "tinyInteger",
            "smallInteger",
            "mediumInteger",
            "tinyIncrements",
            "smallIncrements",
            "mediumIncrements",
        ]
        .contains(&key.kind.as_str())
    {
        return Err("Invalid primary key name or strategy".into());
    }
    if key
        .length
        .is_some_and(|length| key.kind != "string" || length == 0 || length > 65535)
    {
        return Err("Primary key length applies only to string keys (1–65535)".into());
    }
    let mut names = BTreeSet::from([key.name.clone()]);
    if d.timestamps {
        names.extend(["created_at".into(), "updated_at".into()]);
    }
    if d.soft_deletes {
        names.insert("deleted_at".into());
    }
    let incrementing = matches!(
        key.kind.as_str(),
        "bigIncrements" | "increments" | "tinyIncrements" | "smallIncrements" | "mediumIncrements"
    );
    let string_key = matches!(key.kind.as_str(), "uuid" | "ulid" | "string");
    let mut columns = if key.kind == "bigIncrements" && key.name == "id" {
        "            $table->id();\n".into()
    } else {
        let length = if key.kind == "string" {
            format!(", {}", key.length.unwrap_or(191))
        } else {
            String::new()
        };
        format!(
            "            $table->{}({}{length}){};\n",
            key.kind,
            literal(&key.name),
            if incrementing { "" } else { "->primary()" }
        )
    };
    let mut casts = String::new();
    for f in &d.fields {
        if !identifier(&f.name) || !names.insert(f.name.clone()) {
            return Err(format!("Invalid or duplicate field {}", f.name));
        }
        columns.push_str(&field_line(f)?);
        if let Some(cast) = cast(f)? {
            casts.push_str(&format!("        {} => {cast},\n", literal(&f.name)));
        }
    }
    let fillable = d
        .fields
        .iter()
        .map(|f| literal(&f.name))
        .collect::<Vec<_>>()
        .join(", ");
    let mut methods = String::new();
    let mut relation_names = BTreeSet::new();
    for r in &d.relations {
        if !identifier(&r.name)
            || r.name.starts_with("__")
            || [
                "casts",
                "boot",
                "booted",
                "getTable",
                "getKey",
                "getFillable",
                "getGuarded",
                "getAttribute",
                "setAttribute",
                "save",
                "delete",
                "restore",
                "fill",
                "query",
                "newQuery",
                "newCollection",
                "toArray",
                "toJson",
                "factory",
            ]
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&r.name))
            || names.contains(&r.name)
            || !relation_names.insert(r.name.to_ascii_lowercase())
            || (r.kind != "morphTo" && !qualified(&r.related))
            || r.arguments.len() > 5
            || r.arguments.iter().any(|a| !identifier(a))
        {
            return Err(format!("Invalid or duplicate relation {}", r.name));
        }
        let return_type = match r.kind.as_str() {
            "belongsTo" => "BelongsTo",
            "hasOne" => "HasOne",
            "hasMany" => "HasMany",
            "belongsToMany" => "BelongsToMany",
            "hasOneThrough" => "HasOneThrough",
            "hasManyThrough" => "HasManyThrough",
            "morphTo" => "MorphTo",
            "morphOne" => "MorphOne",
            "morphMany" => "MorphMany",
            "morphToMany" => "MorphToMany",
            "morphedByMany" => "MorphToMany",
            _ => return Err("Unsupported relation kind".into()),
        };
        if matches!(
            r.kind.as_str(),
            "morphOne" | "morphMany" | "morphToMany" | "morphedByMany"
        ) && r.arguments.is_empty()
        {
            return Err(format!("{} requires an explicit morph name", r.name));
        }
        let through = if matches!(r.kind.as_str(), "hasOneThrough" | "hasManyThrough") {
            Some(r.through.as_ref().filter(|s| qualified(s)).ok_or_else(|| {
                format!("{} requires a fully qualified intermediate class", r.name)
            })?)
        } else {
            if r.through.is_some() {
                return Err("Intermediate classes apply only to through relations".into());
            }
            None
        };
        let args = r
            .arguments
            .iter()
            .map(|s| literal(s))
            .collect::<Vec<_>>()
            .join(", ");
        let call_args = if r.kind == "morphTo" {
            args
        } else {
            let mut parts = vec![format!("\\{}::class", r.related.trim_start_matches('\\'))];
            if let Some(through) = through {
                parts.push(format!("\\{}::class", through.trim_start_matches('\\')));
            }
            if !args.is_empty() {
                parts.push(args);
            }
            parts.join(", ")
        };
        methods.push_str(&format!("\n    public function {}(): \\Illuminate\\Database\\Eloquent\\Relations\\{return_type}\n    {{\n        return $this->{}({call_args});\n    }}\n",r.name,r.kind));
    }
    if d.related.contains("factory") {
        methods.push_str(&format!("\n    protected static function newFactory(): \\Illuminate\\Database\\Eloquent\\Factories\\Factory\n    {{\n        return \\{}\\{factory_name}::new();\n    }}\n",d.namespace_for("factory").trim_start_matches('\\')));
    }
    let mut index_names = BTreeSet::new();
    for index in &d.indexes {
        if !identifier(&index.name)
            || !index_names.insert(&index.name)
            || index.columns.is_empty()
            || index.columns.len() > 16
            || index.columns.iter().any(|c| !names.contains(c))
            || index.columns.iter().collect::<BTreeSet<_>>().len() != index.columns.len()
        {
            return Err("Invalid index name or columns".into());
        }
        columns.push_str(&format!(
            "            $table->{}([{}], {});\n",
            if index.unique { "unique" } else { "index" },
            index
                .columns
                .iter()
                .map(|s| literal(s))
                .collect::<Vec<_>>()
                .join(", "),
            literal(&index.name)
        ));
    }
    if d.timestamps {
        columns.push_str(if d.timezone {
            "            $table->timestampsTz();\n"
        } else {
            "            $table->timestamps();\n"
        });
    }
    if d.soft_deletes {
        columns.push_str(if d.timezone {
            "            $table->softDeletesTz();\n"
        } else {
            "            $table->softDeletes();\n"
        });
    }
    let mut traits = if d.soft_deletes {
        "    use \\Illuminate\\Database\\Eloquent\\Factories\\HasFactory;\n    use \\Illuminate\\Database\\Eloquent\\SoftDeletes;\n"
    } else {
        "    use \\Illuminate\\Database\\Eloquent\\Factories\\HasFactory;\n"
    }.to_owned();
    match key.kind.as_str() {
        "uuid" => {
            traits.push_str("    use \\Illuminate\\Database\\Eloquent\\Concerns\\HasUuids;\n")
        }
        "ulid" => {
            traits.push_str("    use \\Illuminate\\Database\\Eloquent\\Concerns\\HasUlids;\n")
        }
        _ => {}
    }
    let connection_attribute = d
        .connection
        .as_ref()
        .map(|connection| format!("    protected $connection = {};\n\n", literal(connection)))
        .unwrap_or_default();
    let schema_create = d
        .connection
        .as_ref()
        .map(|connection| format!("Schema::connection({})->create", literal(connection)))
        .unwrap_or_else(|| "Schema::create".into());
    let schema_drop = d
        .connection
        .as_ref()
        .map(|connection| format!("Schema::connection({})->dropIfExists", literal(connection)))
        .unwrap_or_else(|| "Schema::dropIfExists".into());
    let key_attributes = format!(
        "    protected $primaryKey = {};\n\n    public $incrementing = {incrementing};\n\n    protected $keyType = {};\n",
        literal(&key.name),
        literal(if string_key { "string" } else { "int" })
    );
    let model = header(
        &d.namespace_for("model"),
        &format!(
            "class {} extends {model_base}\n{{\n{traits}\n{connection_attribute}    protected $table = {};\n\n{key_attributes}\n    public $timestamps = {};\n\n    protected $fillable = [{fillable}];\n\n    protected $casts = [\n{casts}    ];\n{methods}}}",
            d.name,
            literal(&d.table),
            d.timestamps
        ),
    );
    let mut files = vec![Output {
        path: format!("{}/{}.php", d.directory("model"), d.name),
        text: if settings.type_hints {
            model
        } else {
            let mut value = model;
            for relation_type in [
                "BelongsTo",
                "HasOne",
                "HasMany",
                "BelongsToMany",
                "HasOneThrough",
                "HasManyThrough",
                "MorphTo",
                "MorphOne",
                "MorphMany",
                "MorphToMany",
            ] {
                value = value.replace(
                    &format!(": \\Illuminate\\Database\\Eloquent\\Relations\\{relation_type}\n"),
                    "\n",
                );
            }
            value.replace(
                ": \\Illuminate\\Database\\Eloquent\\Factories\\Factory\n",
                "\n",
            )
        },
    }];
    let mut field_choices = d
        .fields
        .iter()
        .map(|field| field.name.clone())
        .collect::<BTreeSet<_>>();
    let mut factory_choices = field_choices.clone();
    let manual_factory_key = !incrementing && !matches!(key.kind.as_str(), "uuid" | "ulid");
    if manual_factory_key {
        factory_choices.insert(key.name.clone());
    }
    field_choices.insert(key.name.clone());
    output_selection(&d.factory_fields, &factory_choices, "Factory fields")?;
    output_selection(&d.resource_fields, &field_choices, "Resource fields")?;
    output_selection(
        &d.resource_relations,
        &d.relations
            .iter()
            .map(|relation| relation.name.clone())
            .collect(),
        "Resource relations",
    )?;
    let primary_resource = if selected(&d.resource_fields, &key.name) {
        format!("            {} => $this->getKey(),\n", literal(&key.name))
    } else {
        String::new()
    };
    let factory_key = if manual_factory_key && selected(&d.factory_fields, &key.name) {
        format!(
            "            {} => {},\n",
            literal(&key.name),
            if string_key {
                format!(
                    "fake()->unique()->lexify({})",
                    literal(&"?".repeat(key.length.unwrap_or(16).min(16) as usize))
                )
            } else {
                "fake()->unique()->numberBetween(1, 1000000)".into()
            }
        )
    } else {
        String::new()
    };
    for kind in &d.related {
        let (path, text) = match kind.as_str() {
            "migration" => (
                format!(
                    "{}/{}_create_{}_table.php",
                    d.directory("migration"),
                    d.migration_stamp,
                    d.table
                ),
                format!(
                    "<?php\n\nuse Illuminate\\Database\\Migrations\\Migration;\nuse Illuminate\\Database\\Schema\\Blueprint;\nuse Illuminate\\Support\\Facades\\Schema;\n\nreturn new class extends Migration\n{{\n    public function up(): void\n    {{\n        {schema_create}({}, function (Blueprint $table) {{\n{columns}        }});\n    }}\n\n    public function down(): void\n    {{\n        {schema_drop}({});\n    }}\n}};\n",
                    literal(&d.table),
                    literal(&d.table)
                ),
            ),
            "factory" => (
                format!("{}/{factory_name}.php", d.directory("factory")),
                header(
                    &d.namespace_for("factory"),
                    &format!(
                        "/** @extends \\Illuminate\\Database\\Eloquent\\Factories\\Factory<\\{}\\{}> */\nclass {factory_name} extends {factory_base}\n{{\n    protected $model = \\{}\\{}::class;\n\n    public function definition(): array\n    {{\n        return [\n{factory_key}{}        ];\n    }}\n}}",
                        d.namespace_for("model"),
                        d.name,
                        d.namespace_for("model"),
                        d.name,
                        d.fields
                            .iter()
                            .filter(|f| selected(&d.factory_fields, &f.name))
                            .map(|f| format!(
                                "            {} => {},\n",
                                literal(&f.name),
                                factory_value(f)
                            ))
                            .collect::<String>()
                    ),
                ),
            ),
            "resource" => (
                format!("{}/{resource_name}.php", d.directory("resource")),
                header(
                    &d.namespace_for("resource"),
                    &format!(
                        "class {resource_name} extends {resource_base}\n{{\n    public function toArray(\\Illuminate\\Http\\Request $request): array\n    {{\n        return [\n{primary_resource}{}        ];\n    }}\n}}",
                        d.fields
                            .iter()
                            .filter(|f| selected(&d.resource_fields, &f.name))
                            .map(|f| format!(
                                "            {} => $this->{},\n",
                                literal(&f.name),
                                f.name
                            ))
                            .chain(
                                d.relations
                                    .iter()
                                    .filter(|r| selected(&d.resource_relations, &r.name))
                                    .map(|r| format!(
                                        "            {} => $this->whenLoaded({}),\n",
                                        literal(&r.name),
                                        literal(&r.name)
                                    ))
                            )
                            .collect::<String>()
                    ),
                ),
            ),
            "request" => (
                format!("{}/{request_name}.php", d.directory("request")),
                header(
                    &d.namespace_for("request"),
                    &format!(
                        "class {request_name} extends {request_base}\n{{\n    public function authorize(): bool\n    {{\n        // Configure the authorization policy before enabling this request.\n        return false;\n    }}\n\n    public function rules(): array\n    {{\n        return [\n{}        ];\n    }}\n}}",
                        d.fields
                            .iter()
                            .map(|f| format!(
                                "            {} => [{}, {}],\n",
                                literal(&f.name),
                                literal(if f.nullable { "nullable" } else { "required" }),
                                literal(match f.kind.as_str() {
                                    "boolean" => "boolean",
                                    kind if integer_kind(kind) => "integer",
                                    "decimal" | "float" | "double" => "numeric",
                                    "json" | "jsonb" => "array",
                                    "date" | "dateTime" | "dateTimeTz" | "timestamp"
                                    | "timestampTz" => "date",
                                    _ => "string",
                                })
                            ))
                            .collect::<String>()
                    ),
                ),
            ),
            "controller" => (
                format!("{}/{controller_name}.php", d.directory("controller")),
                header(
                    &d.namespace_for("controller"),
                    &format!(
                        "class {controller_name} extends {controller_base}\n{{\n{constructor}}}"
                    ),
                ),
            ),
            "policy" => (
                format!("{}/{policy_name}.php", d.directory("policy")),
                header(
                    &d.namespace_for("policy"),
                    &format!("class {policy_name}{policy_extends}\n{{\n}}"),
                ),
            ),
            "seeder" => (
                format!("{}/{seeder_name}.php", d.directory("seeder")),
                header(
                    &d.namespace_for("seeder"),
                    &format!(
                        "class {seeder_name} extends {seeder_base}\n{{\n    public function run(): void\n    {{\n        \\{}\\{}::factory()->count(10)->create();\n    }}\n}}",
                        d.namespace_for("model"),
                        d.name
                    ),
                ),
            ),
            _ => unreachable!(),
        };
        let text = if settings.type_hints {
            text
        } else {
            let mut text = text
                .replace(": array\n", "\n")
                .replace(": bool\n", "\n")
                .replace(": void\n", "\n");
            for relation_type in [
                "BelongsTo",
                "HasOne",
                "HasMany",
                "BelongsToMany",
                "HasOneThrough",
                "HasManyThrough",
                "MorphTo",
                "MorphOne",
                "MorphMany",
                "MorphToMany",
            ] {
                text = text.replace(
                    &format!(": \\Illuminate\\Database\\Eloquent\\Relations\\{relation_type}\n"),
                    "\n",
                );
            }
            text = text.replace(
                ": \\Illuminate\\Database\\Eloquent\\Factories\\Factory\n",
                "\n",
            );
            text
        };
        files.push(Output { path, text });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    if files.iter().map(|f| &f.path).collect::<BTreeSet<_>>().len() != files.len() {
        return Err("Duplicate generated path".into());
    }
    Ok(Plan { files })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draft() -> Draft {
        serde_json::from_value(serde_json::json!({"name":"Post","table":"posts","migrationStamp":"2026_11_10_120000","fields":[{"name":"title","kind":"string","length":80}],"related":["migration","factory","resource","request"]})).unwrap()
    }
    #[test]
    fn output_selections_filter_only_factory_and_resource_entries_and_allow_explicit_empty() {
        let mut d = draft();
        d.fields.push(
            serde_json::from_value(serde_json::json!({"name":"secret","kind":"string"})).unwrap(),
        );
        d.relations=serde_json::from_value(serde_json::json!([{"name":"author","kind":"belongsTo","related":"App\\Models\\User","arguments":[]},{"name":"comments","kind":"hasMany","related":"App\\Models\\Comment","arguments":[]}])).unwrap();
        let before = plan(&d).unwrap();
        d.factory_fields = Some(vec!["title".into()]);
        d.resource_fields = Some(vec![]);
        d.resource_relations = Some(vec!["author".into()]);
        let after = plan(&d).unwrap();
        for file in after.files.iter().filter(|file| {
            !file.path.ends_with("Factory.php") && !file.path.ends_with("Resource.php")
        }) {
            assert_eq!(
                file.text,
                before
                    .files
                    .iter()
                    .find(|other| other.path == file.path)
                    .unwrap()
                    .text
            );
        }
        let factory = &after
            .files
            .iter()
            .find(|file| file.path.ends_with("Factory.php"))
            .unwrap()
            .text;
        assert!(factory.contains("'title' =>"));
        assert!(!factory.contains("'secret' =>"));
        let resource = &after
            .files
            .iter()
            .find(|file| file.path.ends_with("Resource.php"))
            .unwrap()
            .text;
        assert!(!resource.contains("getKey()"));
        assert!(!resource.contains("'title' =>"));
        assert!(!resource.contains("'secret' =>"));
        assert!(resource.contains("'author' => $this->whenLoaded('author')"));
        assert!(!resource.contains("whenLoaded('comments')"));
        d.factory_fields = Some(vec![]);
        d.resource_relations = Some(vec![]);
        let empty = plan(&d).unwrap();
        assert!(
            !empty
                .files
                .iter()
                .find(|file| file.path.ends_with("Factory.php"))
                .unwrap()
                .text
                .contains("fake()->")
        );
        assert!(
            !empty
                .files
                .iter()
                .find(|file| file.path.ends_with("Resource.php"))
                .unwrap()
                .text
                .contains("whenLoaded(")
        );
    }
    #[test]
    fn output_selection_defaults_and_primary_key_choices_match_generation_strategy() {
        let mut d = draft();
        let original = plan(&d).unwrap();
        d.factory_fields = Some(vec!["title".into()]);
        d.resource_fields = Some(vec!["id".into(), "title".into()]);
        d.resource_relations = Some(vec![]);
        assert_eq!(
            serde_json::to_value(plan(&d).unwrap()).unwrap(),
            serde_json::to_value(original).unwrap()
        );
        d.factory_fields = Some(vec!["id".into()]);
        assert!(plan(&d).is_err());
        d.primary_key = PrimaryKey {
            name: "record_key".into(),
            kind: "string".into(),
            length: Some(24),
        };
        d.factory_fields = Some(vec!["record_key".into()]);
        d.resource_fields = Some(vec!["record_key".into()]);
        let output = plan(&d).unwrap();
        let factory = &output
            .files
            .iter()
            .find(|file| file.path.ends_with("Factory.php"))
            .unwrap()
            .text;
        assert!(factory.contains("'record_key' => fake()->unique()->lexify("));
        assert!(!factory.contains("'title' =>"));
        assert!(
            output
                .files
                .iter()
                .find(|file| file.path.ends_with("Resource.php"))
                .unwrap()
                .text
                .contains("'record_key' => $this->getKey()")
        );
        d.factory_fields = Some(vec![]);
        d.resource_fields = Some(vec![]);
        let output = plan(&d).unwrap();
        assert!(
            !output
                .files
                .iter()
                .find(|file| file.path.ends_with("Factory.php"))
                .unwrap()
                .text
                .contains("'record_key' =>")
        );
        let serialized = serde_json::to_value(&d).unwrap();
        assert_eq!(serialized["factoryFields"], serde_json::json!([]));
        assert_eq!(
            serde_json::from_value::<Draft>(serialized)
                .unwrap()
                .resource_fields,
            Some(vec![])
        );
    }
    #[test]
    fn output_selections_reject_unknown_stale_duplicate_and_unproved_mapping_inputs() {
        for (key, values) in [
            ("factoryFields", serde_json::json!(["missing"])),
            ("factoryFields", serde_json::json!(["title", "title"])),
            ("resourceFields", serde_json::json!(["title", "title"])),
            ("resourceFields", serde_json::json!(["created_at"])),
            ("resourceRelations", serde_json::json!(["missing"])),
            ("resourceRelations", serde_json::json!(["$this->query()"])),
            ("factoryFields", serde_json::json!(["title => system('x')"])),
        ] {
            let mut value = serde_json::to_value(draft()).unwrap();
            value[key] = values;
            assert!(
                plan(&serde_json::from_value(value).unwrap()).is_err(),
                "{key}"
            );
        }
        for unknown in ["factoryMappings", "resourceWrappers"] {
            let mut value = serde_json::to_value(draft()).unwrap();
            value[unknown] = serde_json::json!({"author":"Unproved\\Resource"});
            assert!(serde_json::from_value::<Draft>(value).is_err());
        }
    }
    #[test]
    fn named_connection_roundtrips_and_generates_matching_escaped_model_and_migration() {
        let mut d = draft();
        assert!(
            !plan(&d)
                .unwrap()
                .files
                .iter()
                .any(|file| file.text.contains("protected $connection"))
        );
        d.connection = Some("tenant'\\east".into());
        let encoded = serde_json::to_value(&d).unwrap();
        assert_eq!(
            serde_json::from_value::<Draft>(encoded).unwrap().connection,
            d.connection
        );
        let generated = plan(&d).unwrap();
        assert!(generated.files.iter().any(|file| {
            file.text
                .contains("protected $connection = 'tenant\\'\\\\east';")
        }));
        assert!(generated.files.iter().any(|file| {
            file.text
                .contains("Schema::connection('tenant\\'\\\\east')->create")
                && file
                    .text
                    .contains("Schema::connection('tenant\\'\\\\east')->dropIfExists")
        }));
        for invalid in ["".into(), "\nmysql".into(), "x".repeat(129)] {
            d.connection = Some(invalid);
            assert!(plan(&d).is_err());
        }
    }
    #[test]
    fn per_kind_paths_cannot_generate_duplicate_php_classes() {
        let mut d = draft();
        d.related.insert("policy".into());
        d.settings.policy_postfix = String::new();
        d.destinations.insert(
            "policy".into(),
            Destination {
                directory: "app/Elsewhere".into(),
                namespace: "APP\\Models".into(),
            },
        );
        assert!(plan(&d).unwrap_err().contains("duplicate PHP classes"));
    }
    #[test]
    fn module_related_layout_and_factory_binding_use_the_same_namespaces() {
        let mut d = draft();
        d.namespace = "Modules\\Blog".into();
        d.source_root = "modules/Blog/app".into();
        d.layout = RelatedLayout {
            factory_root: "modules/Blog/database/factories".into(),
            factory_namespace: "Modules\\Blog\\Database\\Factories".into(),
            seeder_root: "modules/Blog/database/seeders".into(),
            seeder_namespace: "Modules\\Blog\\Database\\Seeders".into(),
        };
        d.related.insert("seeder".into());
        let p = plan(&d).unwrap();
        assert!(p.files.iter().any(|f| {
            f.path == "modules/Blog/database/factories/PostFactory.php"
                && f.text
                    .contains("namespace Modules\\Blog\\Database\\Factories;")
        }));
        assert!(
            p.files
                .iter()
                .any(|f| f.path == "modules/Blog/database/seeders/PostSeeder.php")
        );
        let model = &p
            .files
            .iter()
            .find(|f| f.path.ends_with("Models/Post.php"))
            .unwrap()
            .text;
        assert!(model.contains("return \\Modules\\Blog\\Database\\Factories\\PostFactory::new()"));
        assert!(
            !p.files
                .iter()
                .any(|f| f.path.starts_with("database/factories/"))
        );
    }
    #[test]
    fn configured_bases_postfixes_return_hints_and_promotion_affect_real_output() {
        let mut d = draft();
        d.related.extend(
            ["controller", "policy", "seeder"]
                .into_iter()
                .map(str::to_owned),
        );
        d.settings.controller_base = "App\\Http\\Controllers\\BaseController".into();
        d.settings.controller_postfix = "Endpoint".into();
        d.settings.policy_base = Some("App\\Policies\\BasePolicy".into());
        d.settings.factory_postfix = "Maker".into();
        d.settings.resource_postfix = "Representation".into();
        d.settings.type_hints = false;
        d.settings.controller_dependencies = vec![Dependency {
            name: "cache".into(),
            class: "Illuminate\\Contracts\\Cache\\Repository".into(),
        }];
        let p = plan(&d).unwrap();
        let controller = p
            .files
            .iter()
            .find(|f| f.path.ends_with("PostEndpoint.php"))
            .unwrap();
        assert!(
            controller
                .text
                .contains("extends \\App\\Http\\Controllers\\BaseController")
        );
        assert!(
            controller
                .text
                .contains("protected \\Illuminate\\Contracts\\Cache\\Repository $cache")
        );
        assert!(p.files.iter().any(|f| f.path.ends_with("PostPolicy.php")
            && f.text.contains("extends \\App\\Policies\\BasePolicy")));
        assert!(p.files.iter().any(|f|f.path.ends_with("PostRepresentation.php")&&!f.text.contains("): array")));
        assert!(p.files.iter().any(|f| f.path.ends_with("PostMaker.php")));
        d.settings.constructor_promotion = false;
        let p = plan(&d).unwrap();
        let controller = &p
            .files
            .iter()
            .find(|f| f.path.ends_with("PostEndpoint.php"))
            .unwrap()
            .text;
        assert!(controller.contains("$this->cache = $cache;"));
        assert!(
            controller.contains("protected \\Illuminate\\Contracts\\Cache\\Repository $cache;")
        );
        d.settings.controller_base = "Bad; run()".into();
        assert!(plan(&d).is_err());
    }
    #[test]
    fn qualified_database_table_is_preserved_and_invalid_parts_are_refused() {
        let mut draft = draft();
        draft.table = "tenant.posts".into();
        let plan = plan(&draft).unwrap();
        assert!(
            plan.files
                .iter()
                .find(|file| file.path.ends_with("/Post.php"))
                .unwrap()
                .text
                .contains("protected $table = 'tenant.posts'")
        );
        assert!(
            plan.files
                .iter()
                .find(|file| file.path.contains("/migrations/"))
                .unwrap()
                .text
                .contains("Schema::create('tenant.posts'")
        );
        for name in [
            ".posts",
            "tenant.",
            "a.b.c",
            "tenant/../posts",
            "tenant.posts';",
        ] {
            draft.table = name.into();
            assert!(super::plan(&draft).is_err(), "{name}");
        }
    }
    #[test]
    fn coordinated_plan_is_stable_and_populates_related_files() {
        let d = draft();
        let p = plan(&d).unwrap();
        assert_eq!(p, plan(&d).unwrap());
        assert_eq!(p.files.len(), 5);
        assert!(
            p.files
                .iter()
                .any(|f| f.text.contains("$table->string('title', 80)"))
        );
        assert!(
            p.files
                .iter()
                .any(|f| f.text.contains("'title' => $this->title"))
        );
        assert!(p.files.iter().any(|f| f.text.contains("return false;")));
    }
    #[test]
    fn refuses_traversal_duplicate_fields_and_expression_defaults() {
        let mut d = draft();
        d.source_root = "../outside".into();
        assert!(plan(&d).is_err());
        d.source_root = "app".into();
        d.fields.push(d.fields[0].clone());
        assert!(plan(&d).is_err());
        d.fields.pop();
        d.fields[0].default = Some(serde_json::json!({"raw":"exec()"}));
        assert!(plan(&d).is_err());
    }
    #[test]
    fn validates_indexes_and_decimal_precision() {
        let mut d = draft();
        d.indexes.push(Index {
            name: "posts_title_unique".into(),
            columns: vec!["title".into()],
            unique: true,
        });
        assert!(
            plan(&d)
                .unwrap()
                .files
                .iter()
                .any(|f| f.text.contains("unique(['title'], 'posts_title_unique')"))
        );
        d.indexes[0].columns = vec!["missing".into()];
        assert!(plan(&d).is_err());
        d.indexes.clear();
        d.fields[0].kind = "decimal".into();
        d.fields[0].length = None;
        d.fields[0].precision = Some(2);
        d.fields[0].scale = Some(3);
        assert!(plan(&d).is_err());
    }
    #[test]
    fn relation_is_typed_and_does_not_create_related_model() {
        let mut d = draft();
        d.relations.push(Relation {
            name: "author".into(),
            kind: "belongsTo".into(),
            related: "App\\Models\\User".into(),
            through: None,
            arguments: vec!["user_id".into()],
        });
        let p = plan(&d).unwrap();
        assert!(p.files.iter().any(|f| {
            f.text
                .contains("return $this->belongsTo(\\App\\Models\\User::class, 'user_id')")
        }));
        assert!(!p.files.iter().any(|f| f.path.ends_with("User.php")));
    }
    #[test]
    fn through_and_inverse_morph_relations_have_correct_call_shapes() {
        let mut d = draft();
        d.relations=serde_json::from_value(serde_json::json!([{"name":"owner","kind":"morphTo","related":"","arguments":[]},{"name":"comments","kind":"hasManyThrough","related":"App\\Models\\Comment","through":"App\\Models\\User","arguments":[]}])).unwrap();
        let p = plan(&d).unwrap();
        let model = &p
            .files
            .iter()
            .find(|f| f.path.contains("/Models/"))
            .unwrap()
            .text;
        assert!(model.contains("$this->morphTo()"));
        assert!(model.contains(
            "$this->hasManyThrough(\\App\\Models\\Comment::class, \\App\\Models\\User::class)"
        ));
        d.relations[1].through = None;
        assert!(plan(&d).is_err());
    }
    #[test]
    fn quotes_defaults_without_interpreting_php() {
        let mut d = draft();
        d.fields[0].default = Some(Value::String("'); system('x".into()));
        let p = plan(&d).unwrap();
        assert!(
            p.files
                .iter()
                .any(|f| f.text.contains("default('\\'); system(\\'x')"))
        );
    }
    #[test]
    fn explicit_null_defaults_are_preserved_and_reserved_class_names_refused() {
        let field: Field = serde_json::from_value(
            serde_json::json!({"name":"title","kind":"string","nullable":true,"default":null}),
        )
        .unwrap();
        assert_eq!(field.default, Some(Value::Null));
        assert!(field_line(&field).unwrap().contains("->default(null)"));
        let mut d = draft();
        d.name = "Class".into();
        assert!(plan(&d).is_err());
    }
    #[test]
    fn primary_key_strategies_and_timezone_generate_matching_model_and_migration() {
        for (kind, column, trait_name) in [
            ("uuid", "uuid('post_key')->primary()", "HasUuids"),
            ("ulid", "ulid('post_key')->primary()", "HasUlids"),
            ("string", "string('post_key', 191)->primary()", ""),
            ("integer", "integer('post_key')->primary()", ""),
        ] {
            let mut d = draft();
            d.primary_key = PrimaryKey {
                name: "post_key".into(),
                kind: kind.into(),
                length: None,
            };
            d.timezone = true;
            d.soft_deletes = true;
            let p = plan(&d).unwrap();
            let model = &p
                .files
                .iter()
                .find(|f| f.path.contains("/Models/"))
                .unwrap()
                .text;
            let migration = &p
                .files
                .iter()
                .find(|f| f.path.contains("/migrations/"))
                .unwrap()
                .text;
            assert!(model.contains("protected $primaryKey = 'post_key'"));
            assert!(model.contains("public $incrementing = false"));
            assert!(model.contains(&format!(
                "protected $keyType = '{}'",
                if kind == "integer" { "int" } else { "string" }
            )));
            assert!(migration.contains(column));
            assert!(migration.contains("timestampsTz()"));
            assert!(migration.contains("softDeletesTz()"));
            if !trait_name.is_empty() {
                assert!(model.contains(trait_name));
            }
            let resource = &p
                .files
                .iter()
                .find(|f| f.path.contains("/Resources/"))
                .unwrap()
                .text;
            assert!(resource.contains("'post_key' => $this->getKey()"));
        }
    }
    #[test]
    fn enums_temporal_precision_and_parameterized_casts_are_validated() {
        let mut d = draft();
        d.fields=serde_json::from_value(serde_json::json!([{"name":"status","kind":"enum","values":["draft","published"]},{"name":"published_at","kind":"timestampTz","precision":6,"cast":"immutable_datetime:Y-m-d H:i:s"},{"name":"amount","kind":"decimal","precision":12,"scale":4,"cast":"decimal:4"}])).unwrap();
        let p = plan(&d).unwrap();
        assert!(
            p.files
                .iter()
                .any(|f| f.text.contains("enum('status', ['draft', 'published'])"))
        );
        assert!(
            p.files
                .iter()
                .any(|f| f.text.contains("timestampTz('published_at', 6)"))
        );
        d.fields[0].values.push("draft".into());
        assert!(plan(&d).is_err());
    }
    #[test]
    #[ignore = "requires local PHP executable; lints generated files without executing application code"]
    fn generated_php_lints_all_related_files_primary_keys_and_relationships() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "laravel-generated-php-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        for kind in [
            "bigIncrements",
            "increments",
            "uuid",
            "ulid",
            "string",
            "integer",
            "bigInteger",
            "unsignedBigInteger",
        ] {
            let mut d = draft();
            d.connection = Some("tenant\'\\east".into());
            if kind == "uuid" {
                for kind in KINDS {
                    d.destinations.insert(
                        (*kind).into(),
                        Destination {
                            directory: format!("custom/{kind}"),
                            namespace: if *kind == "migration" {
                                String::new()
                            } else {
                                format!("Custom\\Generated{}", kind)
                            },
                        },
                    );
                }
            }
            d.primary_key = PrimaryKey {
                name: "post_key".into(),
                kind: kind.into(),
                length: None,
            };
            d.timezone = true;
            d.soft_deletes = true;
            d.related = [
                "migration",
                "factory",
                "seeder",
                "controller",
                "policy",
                "request",
                "resource",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect();
            d.settings.controller_dependencies = vec![Dependency {
                name: "cache".into(),
                class: "Illuminate\\Contracts\\Cache\\Repository".into(),
            }];
            d.settings.constructor_promotion = kind != "string";
            d.settings.type_hints = kind != "integer";
            if kind == "uuid" {
                d.settings.factory_postfix = "Maker".into();
                d.settings.controller_postfix = "Endpoint".into();
                d.layout.factory_root = "modules/Blog/database/factories".into();
                d.layout.factory_namespace = "Modules\\Blog\\Database\\Factories".into();
            }
            d.fields=serde_json::from_value(serde_json::json!([{"name":"title","kind":"string","length":80,"default":"a'quoted\\value"},{"name":"status","kind":"enum","values":["draft","published"]},{"name":"amount","kind":"decimal","precision":12,"scale":4,"cast":"decimal:4"},{"name":"data","kind":"json","cast":"array"},{"name":"published_at","kind":"timestampTz","precision":6,"cast":"immutable_datetime:Y-m-d H:i:s"}])).unwrap();
            for (at, relation) in [
                "belongsTo",
                "hasOne",
                "hasMany",
                "belongsToMany",
                "hasOneThrough",
                "hasManyThrough",
                "morphTo",
                "morphOne",
                "morphMany",
                "morphToMany",
                "morphedByMany",
            ]
            .into_iter()
            .enumerate()
            {
                d.relations.push(Relation {
                    name: format!("relation{at}"),
                    kind: relation.into(),
                    related: "App\\Models\\User".into(),
                    through: if relation.ends_with("Through") {
                        Some("App\\Models\\Team".into())
                    } else {
                        None
                    },
                    arguments: if relation.starts_with("morph") && relation != "morphTo" {
                        vec!["commentable".into()]
                    } else {
                        vec![]
                    },
                });
            }
            match kind {
                "uuid" => {
                    d.factory_fields = Some(vec![]);
                    d.resource_fields = Some(vec![]);
                    d.resource_relations = Some(vec![]);
                }
                "ulid" => {
                    d.factory_fields = Some(vec!["title".into()]);
                    d.resource_fields = Some(vec!["post_key".into(), "status".into()]);
                    d.resource_relations = Some(vec!["relation0".into(), "relation3".into()]);
                }
                "integer" => {
                    d.factory_fields = Some(vec!["post_key".into()]);
                    d.resource_fields = Some(vec![]);
                    d.resource_relations = Some(vec!["relation1".into()]);
                }
                _ => {}
            }
            for file in plan(&d).unwrap().files {
                let path = root.join(kind).join(&file.path);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, &file.text).unwrap();
                let result = std::process::Command::new("php")
                    .arg("-l")
                    .arg(&path)
                    .output()
                    .expect("Install PHP to run the explicit generated-code lint acceptance");
                assert!(
                    result.status.success(),
                    "{}: {}{}",
                    path.display(),
                    String::from_utf8_lossy(&result.stdout),
                    String::from_utf8_lossy(&result.stderr)
                );
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
