//! Seed the native model wizard from explicitly captured schema metadata. No
//! capture, PHP execution, SQL or project mutation occurs in this adapter.
use crate::generator_plan::{self, Draft};
use openidea_plugin::{Error, Invocation, PickItem, host};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn table<'a>(snapshot: &'a Value, key: &str) -> Result<&'a Value, String> {
    let tables = snapshot["tables"]
        .as_array()
        .filter(|tables| !tables.is_empty() && tables.len() <= 128)
        .ok_or("Capture database schema explicitly before creating a model")?;
    let identity: Option<(String, String)> = serde_json::from_str(key).ok();
    let matches: Vec<_> = tables
        .iter()
        .filter(|table| {
            let name = table["name"].as_str().unwrap_or("");
            let schema = table["schema"].as_str().unwrap_or("");
            identity
                .as_ref()
                .map(|(wanted_schema, wanted_name)| schema == wanted_schema && name == wanted_name)
                .unwrap_or(name == key)
        })
        .collect();
    match matches.as_slice() {
        [table] => Ok(table),
        [] => Err("Captured table is unavailable".into()),
        _ => Err("Ambiguous captured table name; select its schema-qualified identity".into()),
    }
}
fn kind(sql: &str) -> Option<(&'static str, Option<&'static str>)> {
    match sql.trim().to_ascii_lowercase().as_str() {
        "varchar" | "character varying" | "nvarchar" => Some(("string", None)),
        "char" | "character" | "nchar" => Some(("char", None)),
        "text" => Some(("text", None)),
        "tinytext" => Some(("tinyText", None)),
        "mediumtext" => Some(("mediumText", None)),
        "longtext" => Some(("longText", None)),
        "integer" | "int" | "int4" => Some(("integer", Some("integer"))),
        "smallint" | "int2" => Some(("smallInteger", Some("integer"))),
        "tinyint" => Some(("tinyInteger", Some("integer"))),
        "bigint" | "int8" => Some(("bigInteger", Some("integer"))),
        "bool" | "boolean" => Some(("boolean", Some("boolean"))),
        "float" | "real" | "float4" => Some(("float", Some("float"))),
        "double" | "double precision" | "float8" => Some(("double", Some("double"))),
        "date" => Some(("date", Some("date"))),
        "time" | "time without time zone" => Some(("time", None)),
        "timetz" | "time with time zone" => Some(("timeTz", None)),
        "datetime" => Some(("dateTime", Some("datetime"))),
        "timestamp" | "timestamp without time zone" => Some(("timestamp", Some("datetime"))),
        "timestamptz" | "timestamp with time zone" => Some(("timestampTz", Some("datetime"))),
        "json" => Some(("json", Some("array"))),
        "jsonb" => Some(("jsonb", Some("array"))),
        "uuid" => Some(("uuid", None)),
        "ulid" => Some(("ulid", None)),
        "blob" | "bytea" | "binary" | "varbinary" => Some(("binary", None)),
        // Decimal precision/scale, enum values, arrays, unsignedness, generated
        // expressions and vendor-specific domains require richer capture.
        _ => None,
    }
}
#[derive(Default)]
struct SqlDetails {
    base: String,
    unsigned: bool,
    length: Option<u32>,
    precision: Option<u32>,
    scale: Option<u32>,
    values: Option<Vec<String>>,
}
fn enum_values(text: &str) -> Result<Vec<String>, String> {
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut values = Vec::new();
    while at < bytes.len() {
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if bytes.get(at) != Some(&b'\'') {
            return Err("Enum/set values must be exact quoted literals".into());
        }
        at += 1;
        let mut value = String::new();
        let mut closed = false;
        while at < bytes.len() {
            match bytes[at] {
                b'\'' if bytes.get(at + 1) == Some(&b'\'') => {
                    value.push('\'');
                    at += 2;
                }
                b'\'' => {
                    at += 1;
                    closed = true;
                    break;
                }
                b'\\' => {
                    let next = *bytes.get(at + 1).ok_or("Incomplete enum escape")?;
                    if !matches!(next, b'\'' | b'\\') {
                        return Err(
                            "Enum/set escape cannot be translated without driver-specific rules"
                                .into(),
                        );
                    }
                    value.push(next as char);
                    at += 2;
                }
                _ => {
                    let ch = text[at..].chars().next().ok_or("Invalid enum character")?;
                    if ch.is_control() {
                        return Err("Enum/set values contain control characters".into());
                    }
                    value.push(ch);
                    at += ch.len_utf8();
                }
            }
        }
        if !closed || value.is_empty() || value.len() > 256 {
            return Err("Invalid captured enum/set literal".into());
        }
        values.push(value);
        if values.len() > 100 {
            return Err("Too many enum/set values".into());
        }
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if at == bytes.len() {
            break;
        }
        if bytes[at] != b',' {
            return Err("Invalid captured enum/set delimiter".into());
        }
        at += 1;
        if at == bytes.len() {
            return Err("Incomplete captured enum/set list".into());
        }
    }
    if values.is_empty() {
        return Err("Enum/set values are unavailable".into());
    }
    Ok(values)
}
fn sql_details(raw: &str) -> Result<SqlDetails, String> {
    if raw.len() > 2048 || raw.chars().any(char::is_control) {
        return Err("Invalid full SQL type metadata".into());
    }
    let mut descriptor = raw.trim();
    let mut out = SqlDetails::default();
    if descriptor.to_ascii_lowercase().ends_with(" unsigned") {
        out.unsigned = true;
        descriptor = descriptor[..descriptor.len() - 9].trim_end();
    }
    if let Some(open) = descriptor.find('(') {
        if !descriptor.ends_with(')') {
            return Err("Unsupported SQL type modifiers".into());
        }
        out.base = descriptor[..open].trim().to_ascii_lowercase();
        let args = &descriptor[open + 1..descriptor.len() - 1];
        match out.base.as_str() {
            "decimal" | "numeric" => {
                let args = args
                    .split(',')
                    .map(|part| part.trim().parse::<u32>())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| "Invalid decimal precision/scale")?;
                match args.as_slice() {
                    [precision] => {
                        out.precision = Some(*precision);
                        out.scale = Some(0);
                    }
                    [precision, scale] => {
                        out.precision = Some(*precision);
                        out.scale = Some(*scale);
                    }
                    _ => return Err("Decimal precision/scale metadata is incomplete".into()),
                }
            }
            "varchar" | "character varying" | "nvarchar" | "char" | "character" | "nchar" => {
                out.length = Some(
                    args.trim()
                        .parse()
                        .map_err(|_| "Invalid string length metadata")?,
                )
            }
            "enum" | "set" => out.values = Some(enum_values(args)?),
            "int" | "integer" | "tinyint" | "smallint" | "mediumint" | "bigint" => {
                args.trim()
                    .parse::<u32>()
                    .map_err(|_| "Unsupported integer display width")?;
            }
            _ => return Err(format!("Unsupported parameterized SQL type {}", out.base)),
        }
    } else {
        out.base = descriptor.to_ascii_lowercase();
    }
    Ok(out)
}
fn optional_u32(column: &Value, key: &str) -> Result<Option<u32>, String> {
    match column.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| format!("Invalid {key} metadata")),
    }
}
fn column_info(column: &Value) -> Result<Value, String> {
    let raw = column["fullType"]
        .as_str()
        .or_else(|| column["type"].as_str())
        .ok_or("Missing column type")?;
    let info = sql_details(raw)?;
    let mut unsigned = info.unsigned;
    if let Some(flag) = column.get("unsigned").filter(|v| !v.is_null()) {
        let flag = flag.as_bool().ok_or("Invalid unsigned metadata")?;
        if unsigned && !flag {
            return Err("Conflicting unsigned metadata".into());
        }
        unsigned = flag;
    }
    let mut length = info.length;
    let mut precision = info.precision;
    let mut scale = info.scale;
    for (key, target) in [
        ("length", &mut length),
        ("precision", &mut precision),
        ("scale", &mut scale),
    ] {
        if let Some(explicit) = optional_u32(column, key)? {
            if target.is_some_and(|inferred| inferred != explicit) {
                return Err(format!("Conflicting {key} metadata"));
            }
            *target = Some(explicit);
        }
    }
    let mut values = info.values;
    if let Some(explicit) = column.get("values").filter(|v| !v.is_null()) {
        let explicit = explicit
            .as_array()
            .filter(|v| !v.is_empty() && v.len() <= 100)
            .ok_or("Invalid enum/set values")?
            .iter()
            .map(|v| {
                v.as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
                    .map(str::to_owned)
                    .ok_or("Invalid enum/set value")
            })
            .collect::<Result<Vec<_>, _>>()?;
        if values
            .as_ref()
            .is_some_and(|inferred| inferred != &explicit)
        {
            return Err("Conflicting enum/set values".into());
        }
        values = Some(explicit);
    }
    let (mut mapped, mut cast) = match info.base.as_str() {
        "decimal" | "numeric" => {
            let (Some(precision), Some(scale)) = (precision, scale) else {
                return Err("Decimal precision/scale metadata is unavailable".into());
            };
            if precision == 0 || precision > 65 || scale > precision || scale > 30 {
                return Err("Invalid decimal precision/scale".into());
            }
            ("decimal".to_owned(), Some(format!("decimal:{scale}")))
        }
        "enum" | "set" => {
            if values.is_none() {
                return Err("Enum/set values metadata is unavailable".into());
            }
            (info.base.clone(), None)
        }
        "mediumint" => ("mediumInteger".into(), Some("integer".into())),
        other => {
            let (mapped, cast) = kind(other)
                .ok_or_else(|| format!("unsupported or insufficient type metadata {raw}"))?;
            (mapped.to_owned(), cast.map(str::to_owned))
        }
    };
    if unsigned {
        mapped = match mapped.as_str() {
            "tinyInteger" => "unsignedTinyInteger",
            "smallInteger" => "unsignedSmallInteger",
            "mediumInteger" => "unsignedMediumInteger",
            "integer" => "unsignedInteger",
            "bigInteger" => "unsignedBigInteger",
            _ => return Err("Unsigned modifier is not supported for this captured type".into()),
        }
        .into();
        cast = Some("integer".into());
    }
    if length.is_some_and(|n| n == 0 || n > 65535)
        || length.is_some() && !matches!(mapped.as_str(), "string" | "char")
    {
        return Err("String length metadata cannot be represented".into());
    }
    if mapped != "decimal" && (precision.is_some() || scale.is_some()) {
        return Err("Precision/scale cannot be represented for this column type".into());
    }
    let nullable = column["nullable"].as_bool().ok_or("Missing nullability")?;
    Ok(
        json!({"name":column["name"],"kind":mapped,"nullable":nullable,"length":length,"precision":precision,"scale":scale,"values":values.unwrap_or_default(),"cast":cast}),
    )
}

fn numeric_strategy(kind: &str) -> Option<(&str, &str)> {
    match kind {
        "integer" | "unsignedInteger" => Some(("increments", kind)),
        "bigInteger" | "unsignedBigInteger" => Some(("bigIncrements", kind)),
        "tinyInteger" | "unsignedTinyInteger" => Some(("tinyIncrements", kind)),
        "smallInteger" | "unsignedSmallInteger" => Some(("smallIncrements", kind)),
        "mediumInteger" | "unsignedMediumInteger" => Some(("mediumIncrements", kind)),
        _ => None,
    }
}

fn timezone(kind: &str) -> bool {
    matches!(kind, "timestampTz" | "dateTimeTz")
}
fn timestamp(kind: &str) -> bool {
    matches!(
        kind,
        "timestamp" | "timestampTz" | "dateTime" | "dateTimeTz"
    )
}

/// `table_key` is either an unambiguous bare name or JSON [schema,name].
/// Optional `generation.primaryKeyKind` is an explicit UI choice when capture
/// lacks numeric auto-increment metadata; it is not inferred from a column name.
pub(crate) fn draft(snapshot: &Value, table_key: &str, name: &str) -> Result<Draft, String> {
    let selected = table(snapshot, table_key)?;
    let table_name = selected["name"]
        .as_str()
        .filter(|name| generator_plan::identifier(name))
        .ok_or("Table name is not supported by the native generator")?;
    let schema = selected["schema"].as_str().unwrap_or("");
    if !schema.is_empty() && !generator_plan::identifier(schema) {
        return Err("Captured schema name is not supported by the native generator".into());
    }
    let qualified_table = if schema.is_empty() {
        table_name.to_owned()
    } else {
        format!("{schema}.{table_name}")
    };
    let columns = selected["columns"]
        .as_array()
        .filter(|c| !c.is_empty() && c.len() <= 128)
        .ok_or("Expected 1–128 captured table columns")?;
    let indexes = selected["indexes"]
        .as_array()
        .filter(|i| i.len() <= 64)
        .ok_or("Expected captured index metadata")?;
    let primary: Vec<_> = indexes
        .iter()
        .filter(|i| i["primary"].as_bool() == Some(true))
        .collect();
    let [primary] = primary.as_slice() else {
        return Err("Exactly one single-column primary index is required; composite/keyless models need manual setup".into());
    };
    if primary["unique"].as_bool() != Some(true) {
        return Err("Primary-index uniqueness metadata is unavailable or invalid".into());
    }
    let key_columns = primary["columns"]
        .as_array()
        .ok_or("Missing primary-index columns")?;
    let [key] = key_columns.as_slice() else {
        return Err(
            "Composite primary keys cannot be represented by the native model generator".into(),
        );
    };
    let key_name = key
        .as_str()
        .filter(|name| generator_plan::identifier(name))
        .ok_or("Invalid primary key identifier")?;
    let keys: Vec<_> = columns
        .iter()
        .filter(|column| column["name"].as_str() == Some(key_name))
        .collect();
    let [key_column] = keys.as_slice() else {
        return Err("Primary index must reference one captured column".into());
    };
    if key_column["nullable"].as_bool() != Some(false) {
        return Err("A nullable or unknown-nullability primary key cannot be generated".into());
    }
    let key_info = column_info(key_column)
        .map_err(|error| format!("Unsupported primary column {key_name}: {error}"))?;
    let mapped_key = key_info["kind"]
        .as_str()
        .ok_or("Invalid mapped primary type")?;
    let key_kind = if let Some((incrementing, regular)) = numeric_strategy(mapped_key) {
        match (key_column["autoIncrement"].as_bool(),snapshot["generation"]["primaryKeyKind"].as_str()) {
            (Some(true),_)=>incrementing,(Some(false),_)=>regular,
            (None,Some(chosen)) if chosen==incrementing||chosen==regular=>chosen,
            _=>return Err("Numeric primary-key auto-increment is not present in the captured metadata; select its strategy explicitly".into()),
        }
    } else {
        match mapped_key {
            "uuid" | "ulid" => mapped_key,
            "string" if key_info["length"].as_u64().is_some() => "string",
            _ => {
                return Err(format!(
                    "Primary key {key_name} has insufficient type/length metadata for native generation"
                ));
            }
        }
    };
    let mut errors = Vec::new();
    let mut fields = Vec::new();
    let mut seen = BTreeSet::new();
    let mut standard_times = Vec::new();
    for column in columns {
        let Some(column_name) = column["name"]
            .as_str()
            .filter(|n| generator_plan::identifier(n))
        else {
            errors.push(format!("invalid column name {}", column["name"]));
            continue;
        };
        if !seen.insert(column_name) {
            errors.push(format!("duplicate column {column_name}"));
            continue;
        }
        if column_name == key_name {
            continue;
        }
        let field = match column_info(column) {
            Ok(value) => value,
            Err(error) => {
                errors.push(format!("{column_name}: {error}"));
                continue;
            }
        };
        let mapped = field["kind"].as_str().unwrap_or("");
        if matches!(column_name, "created_at" | "updated_at") && timestamp(mapped) {
            standard_times.push((column_name, mapped.to_owned()));
        }
        fields.push(field);
    }
    if !errors.is_empty() {
        return Err(format!(
            "Cannot seed a complete model draft:\n{}",
            errors.join("\n")
        ));
    }
    let timestamps = standard_times.len() == 2;
    let has_timezone = timestamps && standard_times.iter().all(|(_, kind)| timezone(kind));
    if timestamps && standard_times.iter().any(|(_, kind)| timezone(kind)) && !has_timezone {
        return Err(
            "Timestamp columns have different timezone types; configure them manually".into(),
        );
    }
    if timestamps {
        fields.retain(|field| !matches!(field["name"].as_str(), Some("created_at" | "updated_at")));
    }
    let mut mapped_indexes = Vec::new();
    let mut index_names = BTreeSet::new();
    for index in indexes {
        if index["primary"].as_bool() == Some(true) {
            continue;
        }
        let Some(index_name) = index["name"]
            .as_str()
            .filter(|n| generator_plan::identifier(n))
        else {
            return Err("Captured index name is not supported by native generation".into());
        };
        if !index_names.insert(index_name) {
            return Err(format!("Duplicate captured index {index_name}"));
        }
        let index_columns = index["columns"]
            .as_array()
            .filter(|c| !c.is_empty())
            .ok_or("Captured index has no columns")?;
        if index_columns
            .iter()
            .any(|column| !column.as_str().is_some_and(|name| seen.contains(name)))
        {
            return Err(format!("Index {index_name} references an unknown column"));
        }
        let unique = index["unique"]
            .as_bool()
            .ok_or("Captured index uniqueness is unavailable")?;
        mapped_indexes.push(json!({"name":index_name,"columns":index_columns,"unique":unique}));
    }
    let captured = snapshot["capturedAt"]
        .as_u64()
        .ok_or("Schema capture timestamp is unavailable")?;
    let connection = match snapshot.get("connection") {
        None | Some(Value::Null) => None,
        Some(Value::String(name))
            if !name.is_empty()
                && name.chars().count() <= 128
                && !name.chars().any(char::is_control) =>
        {
            Some(name.clone())
        }
        _ => return Err("Captured connection name is invalid or cannot be represented".into()),
    };
    let draft:Draft=serde_json::from_value(json!({"connection":connection,"name":name,"table":qualified_table,"namespace":"App","sourceRoot":"app","migrationRoot":"database/migrations","migrationStamp":stamp(captured),"primaryKey":{"name":key_name,"kind":key_kind,"length":if key_kind=="string"{key_info["length"].clone()}else{Value::Null}},"timestamps":timestamps,"timezone":has_timezone,"softDeletes":false,"fields":fields,"indexes":mapped_indexes,"related":[]})).map_err(|e|e.to_string())?;
    generator_plan::plan(&draft)?;
    Ok(draft)
}

pub(crate) fn run(ctx: &Invocation, snapshot: &Value) -> Result<Option<Draft>, Error> {
    let project = ctx.project().ok_or_else(|| {
        Error::Invalid(
            crate::ui_text::text(
                "native.model_actions.a74a0baad4",
                "Open a Laravel project first",
            )
            .into(),
        )
    })?;
    let root = project.root()?;
    let tables = snapshot["tables"]
        .as_array()
        .filter(|tables| !tables.is_empty() && tables.len() <= 128)
        .ok_or_else(|| {
            Error::Invalid(
                crate::ui_text::text(
                    "modelUi.model_from_schema.c396854983",
                    "Refresh Schema explicitly in Runtime before creating a model",
                )
                .into(),
            )
        })?;
    let items: Vec<_> = tables
        .iter()
        .map(|table| {
            let schema = table["schema"].as_str().unwrap_or("");
            let name = table["name"].as_str().unwrap_or("");
            let key = json!([schema, name]).to_string();
            let label = if schema.is_empty() {
                name.to_owned()
            } else {
                format!("{schema}.{name}")
            };
            PickItem::new(&key, &label)
        })
        .collect();
    let Some(selected) = host::pick(
        crate::ui_text::text(
            "modelUi.model_from_schema.5efa269501",
            "Captured database table",
        ),
        &items,
    )?
    else {
        return Ok(None);
    };
    if !items.iter().any(|item| item.key == selected) {
        return Err(Error::Stale);
    }
    let table = table(snapshot, &selected).map_err(ui_error)?;
    let default_name = table["name"].as_str().unwrap_or("");
    let Some(name) = host::input(
        crate::ui_text::text(
            "modelUi.model_from_schema.36fcfd261b",
            "New model class name (review table and field settings in Generators)",
        ),
        Some(default_name),
    )?
    else {
        return Ok(None);
    };
    let mut source = snapshot.clone();
    let primary = table["indexes"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|i| i["primary"].as_bool() == Some(true))
        .and_then(|i| i["columns"].as_array())
        .filter(|c| c.len() == 1)
        .and_then(|c| c[0].as_str());
    if let Some(column) = primary.and_then(|name| {
        table["columns"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|c| c["name"].as_str() == Some(name))
    }) {
        let info = column_info(column).map_err(ui_error)?;
        let kind = info["kind"].as_str().unwrap_or("");
        if column["autoIncrement"].is_null() {
            if let Some((incrementing, regular)) = numeric_strategy(kind) {
                let choices = [
                    PickItem::new(
                        incrementing,
                        &crate::ui_text::format(
                            "modelUi.primaryIncrementing",
                            "Auto-incrementing {kind} primary key",
                            &[("kind", kind)],
                        ),
                    ),
                    PickItem::new(
                        regular,
                        &crate::ui_text::format(
                            "modelUi.primaryRegular",
                            "Non-incrementing {kind} primary key",
                            &[("kind", kind)],
                        ),
                    ),
                ];
                let Some(choice) = host::pick(
                    crate::ui_text::text(
                        "modelUi.model_from_schema.515f4cdc57",
                        "Captured metadata omits auto-increment: choose the actual primary-key strategy",
                    ),
                    &choices,
                )?
                else {
                    return Ok(None);
                };
                if !choices.iter().any(|item| item.key == choice) {
                    return Err(Error::Stale);
                }
                source["generation"] = json!({"primaryKeyKind":choice});
            }
        }
    }
    if ctx.project().and_then(|p| p.root().ok()).as_deref() != Some(&root) {
        return Err(Error::Stale);
    }
    draft(&source, &selected, &name).map(Some).map_err(ui_error)
}
fn stamp(seconds: u64) -> String {
    let days = (seconds / 86400) as i64 + 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    format!(
        "{year:04}_{month:02}_{day:02}_{:02}{:02}{:02}",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}

/// Localize only authored literal planner errors at the UI delivery boundary.
fn ui_error(message: String) -> Error {
    ui_error_depth(message, 0)
}
fn ui_error_depth(message: String, depth: u8) -> Error {
    if depth > 8 {
        return Error::Invalid(message);
    }
    if let Some(errors) = message.strip_prefix("Cannot seed a complete model draft:\n") {
        let lines = errors
            .lines()
            .map(|line| {
                let (prefix, detail) = line
                    .split_once(": ")
                    .filter(|(name, _)| crate::generator_plan::identifier(name))
                    .map_or((None, line), |(name, detail)| (Some(name), detail));
                let Error::Invalid(rendered) = ui_error_depth(detail.to_owned(), depth + 1) else {
                    unreachable!()
                };
                prefix.map_or_else(|| rendered.clone(), |name| format!("{name}: {rendered}"))
            })
            .collect::<Vec<_>>()
            .join("\n");
        return Error::Invalid(crate::ui_text::format(
            "final.model_from_schema.modelDraft",
            "Cannot seed a complete model draft:\n{errors}",
            &[("errors", &lines)],
        ));
    }
    let localized = match message.as_str() {
        "Capture database schema explicitly before creating a model" => {
            crate::ui_text::t("modelUi.model_from_schema.e8b8009c8f", &message)
        }
        "Captured table is unavailable" => {
            crate::ui_text::t("modelUi.model_from_schema.1ec0614d83", &message)
        }
        "Ambiguous captured table name; select its schema-qualified identity" => {
            crate::ui_text::t("modelUi.model_from_schema.c78c3553c2", &message)
        }
        "Enum/set values must be exact quoted literals" => {
            crate::ui_text::t("modelUi.model_from_schema.64c85ac0f6", &message)
        }
        "Incomplete enum escape" => {
            crate::ui_text::t("modelUi.model_from_schema.264f9abbd6", &message)
        }
        "Enum/set escape cannot be translated without driver-specific rules" => {
            crate::ui_text::t("modelUi.model_from_schema.c5070be6f4", &message)
        }
        "Invalid enum character" => {
            crate::ui_text::t("modelUi.model_from_schema.7328318b6e", &message)
        }
        "Enum/set values contain control characters" => {
            crate::ui_text::t("modelUi.model_from_schema.bec850aaee", &message)
        }
        "Invalid captured enum/set literal" => {
            crate::ui_text::t("modelUi.model_from_schema.a80c696901", &message)
        }
        "Too many enum/set values" => {
            crate::ui_text::t("modelUi.model_from_schema.80ab9db367", &message)
        }
        "Invalid captured enum/set delimiter" => {
            crate::ui_text::t("modelUi.model_from_schema.27f64c07cf", &message)
        }
        "Incomplete captured enum/set list" => {
            crate::ui_text::t("modelUi.model_from_schema.e82a5cb2b1", &message)
        }
        "Enum/set values are unavailable" => {
            crate::ui_text::t("modelUi.model_from_schema.a77010d383", &message)
        }
        "Invalid full SQL type metadata" => {
            crate::ui_text::t("modelUi.model_from_schema.ad50c14f89", &message)
        }
        "Unsupported SQL type modifiers" => {
            crate::ui_text::t("modelUi.model_from_schema.d3b7cefd34", &message)
        }
        "Decimal precision/scale metadata is incomplete" => {
            crate::ui_text::t("modelUi.model_from_schema.35f30e7ddc", &message)
        }
        "Missing column type" => {
            crate::ui_text::t("modelUi.model_from_schema.6f31bd5210", &message)
        }
        "Invalid unsigned metadata" => {
            crate::ui_text::t("modelUi.model_from_schema.b7118f0ad8", &message)
        }
        "Conflicting unsigned metadata" => {
            crate::ui_text::t("modelUi.model_from_schema.08ff26a970", &message)
        }
        "Invalid enum/set values" => {
            crate::ui_text::t("modelUi.model_from_schema.292527392d", &message)
        }
        "Invalid enum/set value" => {
            crate::ui_text::t("modelUi.model_from_schema.ff15320228", &message)
        }
        "Conflicting enum/set values" => {
            crate::ui_text::t("modelUi.model_from_schema.e1b029ca36", &message)
        }
        "Decimal precision/scale metadata is unavailable" => {
            crate::ui_text::t("modelUi.model_from_schema.8648674f2a", &message)
        }
        "Invalid decimal precision/scale" => {
            crate::ui_text::t("modelUi.model_from_schema.ae2aa3c384", &message)
        }
        "Enum/set values metadata is unavailable" => {
            crate::ui_text::t("modelUi.model_from_schema.21f9133d46", &message)
        }
        "Unsigned modifier is not supported for this captured type" => {
            crate::ui_text::t("modelUi.model_from_schema.361f396f55", &message)
        }
        "String length metadata cannot be represented" => {
            crate::ui_text::t("modelUi.model_from_schema.50a87ab564", &message)
        }
        "Precision/scale cannot be represented for this column type" => {
            crate::ui_text::t("modelUi.model_from_schema.ded702e080", &message)
        }
        "Missing nullability" => {
            crate::ui_text::t("modelUi.model_from_schema.20f9f0a4e1", &message)
        }
        "Table name is not supported by the native generator" => {
            crate::ui_text::t("modelUi.model_from_schema.cfa105ab76", &message)
        }
        "Captured schema name is not supported by the native generator" => {
            crate::ui_text::t("modelUi.model_from_schema.41d404aa67", &message)
        }
        "Expected 1–128 captured table columns" => {
            crate::ui_text::t("modelUi.model_from_schema.66f83628ff", &message)
        }
        "Expected captured index metadata" => {
            crate::ui_text::t("modelUi.model_from_schema.efc2dd3b34", &message)
        }
        "Exactly one single-column primary index is required; composite/keyless models need manual setup" => {
            crate::ui_text::t("modelUi.model_from_schema.64ad36cf59", &message)
        }
        "Primary-index uniqueness metadata is unavailable or invalid" => {
            crate::ui_text::t("modelUi.model_from_schema.98df99cf6d", &message)
        }
        "Missing primary-index columns" => {
            crate::ui_text::t("modelUi.model_from_schema.8b24c4bbe6", &message)
        }
        "Composite primary keys cannot be represented by the native model generator" => {
            crate::ui_text::t("modelUi.model_from_schema.b0acf191c1", &message)
        }
        "Invalid primary key identifier" => {
            crate::ui_text::t("modelUi.model_from_schema.f7440019cf", &message)
        }
        "Primary index must reference one captured column" => {
            crate::ui_text::t("modelUi.model_from_schema.780748a18d", &message)
        }
        "A nullable or unknown-nullability primary key cannot be generated" => {
            crate::ui_text::t("modelUi.model_from_schema.f5f53774ff", &message)
        }
        "Invalid mapped primary type" => {
            crate::ui_text::t("modelUi.model_from_schema.f314426b82", &message)
        }
        "Numeric primary-key auto-increment is not present in the captured metadata; select its strategy explicitly" => {
            crate::ui_text::t("modelUi.model_from_schema.f400234143", &message)
        }
        "Timestamp columns have different timezone types; configure them manually" => {
            crate::ui_text::t("modelUi.model_from_schema.c4656cff0b", &message)
        }
        "Captured index name is not supported by native generation" => {
            crate::ui_text::t("modelUi.model_from_schema.130fadb880", &message)
        }
        "Captured index has no columns" => {
            crate::ui_text::t("modelUi.model_from_schema.70626f9373", &message)
        }
        "Captured index uniqueness is unavailable" => {
            crate::ui_text::t("modelUi.model_from_schema.09ac10e552", &message)
        }
        "Schema capture timestamp is unavailable" => {
            crate::ui_text::t("modelUi.model_from_schema.e4d756bc67", &message)
        }
        "Captured connection name is invalid or cannot be represented" => {
            crate::ui_text::t("modelUi.model_from_schema.a53eff3dee", &message)
        }
        _ => crate::ui_text::owned_error(&message, UI_ERROR_TEMPLATES).unwrap_or(message),
    };
    Error::Invalid(localized)
}

#[cfg(test)]
mod presentation_locale_tests {
    use super::*;
    #[test]
    fn owned_errors_translate_at_delivery_while_project_details_stay_exact() {
        let host = openidea_plugin::mock::MockHost::install();
        for language in ["ja", "ko", "zh-CN"] {
            host.set_setting("uiLanguage", &serde_json::to_string(language).unwrap());
            let Error::Invalid(message) = ui_error("Missing nullability".into()) else {
                panic!("invalid UI error")
            };
            assert_ne!(message, "Missing nullability");
            let raw_type = "Domain{root}の型";
            let Error::Invalid(nested) = ui_error(format!(
                "Cannot seed a complete model draft:\nemail: unsupported or insufficient type metadata {raw_type}"
            )) else {
                panic!("invalid UI error")
            };
            assert!(nested.contains("email: "));
            assert!(nested.contains(raw_type));
            assert!(!nested.contains("Cannot seed a complete model draft"));
            assert!(!nested.contains("unsupported or insufficient type metadata"));
            let raw = "ProjectModel{root}の列 : custom SQL type {kind}";
            assert_eq!(ui_error(raw.into()), Error::Invalid(raw.into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Value {
        json!({"capturedAt":1794300000u64,"tables":[{"name":"posts","schema":"","columns":[{"name":"id","type":"uuid","nullable":false},{"name":"title","type":"varchar","nullable":false},{"name":"published","type":"boolean","nullable":true},{"name":"created_at","type":"timestamp","nullable":true},{"name":"updated_at","type":"timestamp","nullable":true}],"indexes":[{"name":"posts_pk","columns":["id"],"unique":true,"primary":true},{"name":"posts_title_unique","columns":["title"],"unique":true,"primary":false}]}]})
    }
    #[test]
    fn captured_schema_seeds_model_fields_casts_indexes_and_no_migration() {
        let draft = draft(&fixture(), "posts", "Post").unwrap();
        assert_eq!(draft.table, "posts");
        assert_eq!(draft.primary_key.kind, "uuid");
        assert!(draft.timestamps);
        assert!(!draft.soft_deletes);
        assert!(draft.related.is_empty());
        assert_eq!(
            draft
                .fields
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["title", "published"]
        );
        assert_eq!(draft.fields[1].cast.as_deref(), Some("boolean"));
        assert!(draft.fields[1].nullable);
        assert_eq!(draft.indexes.len(), 1);
        assert!(draft.indexes[0].unique);
        let plan = generator_plan::plan(&draft).unwrap();
        assert_eq!(plan.files.len(), 1);
        assert!(plan.files[0].text.contains("public $incrementing = false"));
    }
    #[test]
    fn exact_schema_identity_is_preserved_and_ambiguous_bare_names_are_refused() {
        let mut fixture = fixture();
        fixture["tables"][0]["schema"] = json!("public");
        let mut other = fixture["tables"][0].clone();
        other["schema"] = json!("analytics");
        fixture["tables"].as_array_mut().unwrap().push(other);
        assert!(
            draft(&fixture, "posts", "Post")
                .unwrap_err()
                .contains("Ambiguous")
        );
        let value = draft(&fixture, r#"["analytics","posts"]"#, "Post").unwrap();
        assert_eq!(value.table, "analytics.posts");
    }
    #[test]
    fn unsupported_columns_are_reported_together_without_guessing_decimal_scale_or_arrays() {
        let mut fixture = fixture();
        fixture["tables"][0]["columns"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"name":"price","type":"numeric","nullable":false}),
                json!({"name":"tags","type":"ARRAY","nullable":true}),
            ]);
        let error = draft(&fixture, "posts", "Post").unwrap_err();
        assert!(error.contains("price"));
        assert!(error.contains("tags"));
    }
    #[test]
    fn numeric_primary_requires_captured_or_explicit_strategy_and_composite_keys_are_refused() {
        let mut fixture = fixture();
        fixture["tables"][0]["columns"][0]["type"] = json!("bigint");
        assert!(
            draft(&fixture, "posts", "Post")
                .unwrap_err()
                .contains("auto-increment")
        );
        fixture["generation"] = json!({"primaryKeyKind":"bigInteger"});
        assert_eq!(
            draft(&fixture, "posts", "Post").unwrap().primary_key.kind,
            "bigInteger"
        );
        fixture["tables"][0]["columns"][0]["autoIncrement"] = json!(true);
        assert_eq!(
            draft(&fixture, "posts", "Post").unwrap().primary_key.kind,
            "bigIncrements"
        );
        fixture["tables"][0]["indexes"][0]["columns"] = json!(["id", "title"]);
        assert!(
            draft(&fixture, "posts", "Post")
                .unwrap_err()
                .contains("Composite")
        );
    }
    #[test]
    fn native_run_asks_numeric_strategy_without_capturing_or_executing() {
        let host = openidea_plugin::mock::MockHost::install();
        let mut fixture = fixture();
        fixture["tables"][0]["columns"][0]["type"] = json!("integer");
        host.answer_next_prompt(Some(&[r#"["","posts"]"#]));
        host.answer_next_prompt(Some(&["Post"]));
        host.answer_next_prompt(Some(&["integer"]));
        let value = run(&host.invocation(), &fixture).unwrap().unwrap();
        assert_eq!(value.primary_key.kind, "integer");
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn cancelled_or_missing_capture_does_not_run_anything() {
        let host = openidea_plugin::mock::MockHost::install();
        host.answer_next_prompt(None);
        assert!(run(&host.invocation(), &fixture()).unwrap().is_none());
        assert!(run(&host.invocation(), &json!({"tables":[]})).is_err());
        assert!(host.process_runs().is_empty());
    }
}

#[cfg(test)]
mod richer_tests {
    use super::*;
    fn fixture() -> Value {
        json!({"capturedAt":1794300000u64,"tables":[{"name":"invoices","schema":"","columns":[{"name":"id","type":"int","fullType":"int(11) unsigned","nullable":false,"autoIncrement":true},{"name":"reference","type":"varchar","fullType":"varchar(64)","nullable":false},{"name":"amount","type":"decimal","fullType":"decimal(12,3)","nullable":false},{"name":"count","type":"bigint","fullType":"bigint unsigned","nullable":false},{"name":"status","type":"enum","fullType":"enum('draft','owner''s review','déjà, payé')","nullable":false}],"indexes":[{"name":"invoices_pk","columns":["id"],"unique":true,"primary":true}]}]})
    }
    #[test]
    fn captured_full_types_preserve_decimal_unsigned_string_length_and_enum_values() {
        let draft = draft(&fixture(), "invoices", "Invoice").unwrap();
        assert_eq!(draft.primary_key.kind, "increments");
        let reference = draft.fields.iter().find(|f| f.name == "reference").unwrap();
        assert_eq!(reference.length, Some(64));
        let amount = draft.fields.iter().find(|f| f.name == "amount").unwrap();
        assert_eq!(amount.kind, "decimal");
        assert_eq!(amount.precision, Some(12));
        assert_eq!(amount.scale, Some(3));
        assert_eq!(amount.cast.as_deref(), Some("decimal:3"));
        assert_eq!(
            draft
                .fields
                .iter()
                .find(|f| f.name == "count")
                .unwrap()
                .kind,
            "unsignedBigInteger"
        );
        let status = draft.fields.iter().find(|f| f.name == "status").unwrap();
        assert_eq!(status.values, ["draft", "owner's review", "déjà, payé"]);
        assert!(draft.related.is_empty());
        assert_eq!(generator_plan::plan(&draft).unwrap().files.len(), 1);
    }
    #[test]
    fn explicit_dimensions_values_and_string_primary_are_used_only_when_proven() {
        let mut fixture = fixture();
        fixture["tables"][0]["columns"][0] =
            json!({"name":"id","type":"varchar","length":32,"nullable":false});
        fixture["tables"][0]["columns"][2] =
            json!({"name":"amount","type":"numeric","precision":8,"scale":2,"nullable":false});
        fixture["tables"][0]["columns"][4] =
            json!({"name":"status","type":"enum","values":["new","paid"],"nullable":true});
        let draft = draft(&fixture, "invoices", "Invoice").unwrap();
        assert_eq!(draft.primary_key.kind, "string");
        assert_eq!(draft.primary_key.length, Some(32));
        assert_eq!(
            draft
                .fields
                .iter()
                .find(|f| f.name == "amount")
                .unwrap()
                .precision,
            Some(8)
        );
        fixture["tables"][0]["columns"][0]
            .as_object_mut()
            .unwrap()
            .remove("length");
        assert!(super::draft(&fixture, "invoices", "Invoice").is_err());
    }
    #[test]
    fn conflicting_or_unsupported_dimensions_refuse_instead_of_defaulting() {
        let mut fixture = fixture();
        fixture["tables"][0]["columns"][2]["precision"] = json!(9);
        fixture["tables"][0]["columns"][3]["unsigned"] = json!(false);
        let error = draft(&fixture, "invoices", "Invoice").unwrap_err();
        assert!(error.contains("amount: Conflicting precision"));
        assert!(error.contains("count: Conflicting unsigned"));
        fixture["tables"][0]["columns"][2] =
            json!({"name":"amount","type":"numeric","nullable":false});
        assert!(
            draft(&fixture, "invoices", "Invoice")
                .unwrap_err()
                .contains("precision/scale metadata is unavailable")
        );
    }
}

#[cfg(test)]
mod sized_key_tests {
    use super::*;
    #[test]
    fn sized_unsigned_primary_keys_preserve_width_and_explicit_incrementing_state() {
        for (sql, kind, incrementing) in [
            ("tinyint unsigned", "unsignedTinyInteger", "tinyIncrements"),
            (
                "smallint unsigned",
                "unsignedSmallInteger",
                "smallIncrements",
            ),
            (
                "mediumint unsigned",
                "unsignedMediumInteger",
                "mediumIncrements",
            ),
        ] {
            let mut fixture = json!({"capturedAt":1794300000u64,"tables":[{"name":"posts","schema":"","columns":[{"name":"id","type":sql,"nullable":false,"autoIncrement":false}],"indexes":[{"name":"posts_pk","columns":["id"],"unique":true,"primary":true}]}]});
            let value = draft(&fixture, "posts", "Post").unwrap();
            assert_eq!(value.primary_key.kind, kind);
            fixture["tables"][0]["columns"][0]["autoIncrement"] = json!(true);
            assert_eq!(
                draft(&fixture, "posts", "Post").unwrap().primary_key.kind,
                incrementing
            );
        }
    }
}

#[cfg(test)]
mod connection_tests {
    use super::*;
    #[test]
    fn captured_connection_is_preserved_escaped_and_never_guessed_for_historical_snapshots() {
        let mut snapshot = json!({"capturedAt":1794300000u64,"connection":"analytics'archive\\east","tables":[{"name":"reports","schema":"","columns":[{"name":"id","type":"integer","nullable":false,"autoIncrement":true}],"indexes":[{"name":"reports_pk","columns":["id"],"unique":true,"primary":true}]}]});
        let seeded = draft(&snapshot, "reports", "Report").unwrap();
        assert_eq!(
            seeded.connection.as_deref(),
            Some("analytics'archive\\east")
        );
        let output = generator_plan::plan(&seeded).unwrap();
        assert!(
            output.files[0]
                .text
                .contains("protected $connection = 'analytics\\'archive\\\\east';")
        );
        snapshot["connection"] = Value::Null;
        assert!(
            draft(&snapshot, "reports", "Report")
                .unwrap()
                .connection
                .is_none()
        );
        for invalid in [
            json!(""),
            json!("bad\nname"),
            json!("x".repeat(129)),
            json!({"name":"analytics"}),
        ] {
            snapshot["connection"] = invalid;
            assert!(draft(&snapshot, "reports", "Report").is_err());
        }
    }
}

// Explicit formats emitted by this module; only UI delivery uses locale state.
const UI_ERROR_TEMPLATES: &[(&str, &str)] = &[
    (
        "final.model_from_schema.sqlType",
        "Unsupported parameterized SQL type {value}",
    ),
    (
        "final.model_from_schema.invalidMetadata",
        "Invalid {key} metadata",
    ),
    (
        "final.model_from_schema.conflictingMetadata",
        "Conflicting {key} metadata",
    ),
    (
        "final.model_from_schema.unsupportedType",
        "unsupported or insufficient type metadata {raw}",
    ),
    (
        "final.model_from_schema.primaryType",
        "Primary key {key} has insufficient type/length metadata for native generation",
    ),
    (
        "final.model_from_schema.modelDraft",
        "Cannot seed a complete model draft:\n{errors}",
    ),
    (
        "final.model_from_schema.duplicateIndex",
        "Duplicate captured index {index}",
    ),
    (
        "final.model_from_schema.unknownIndexColumn",
        "Index {index} references an unknown column",
    ),
    (
        "final.model_from_schema.invalidColumn",
        "invalid column name {value}",
    ),
    (
        "final.model_from_schema.duplicateColumn",
        "duplicate column {column}",
    ),
];
