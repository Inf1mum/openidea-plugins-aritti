//! Versioned Nova/Filament resource plans. Inputs are typed schema data, never PHP.
//! Contracts: nova.laravel.com/docs/v5/resources/fields and Filament 3/4/5
//! resources/overview, schemas and relation-manager documentation. Installed
//! package acceptance is separate from PHP syntax validation.
use crate::generator_plan::{self, Field, Output, Plan, PrimaryKey, Relation};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Request {
    pub kind: String,
    pub name: String,
    pub model: String,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub relations: Vec<Relation>,
    #[serde(default)]
    pub primary_key: PrimaryKey,
    #[serde(default)]
    pub selected_fields: Option<Vec<String>>,
    #[serde(default)]
    pub selected_relations: Option<Vec<String>>,
    #[serde(default)]
    pub separate: Option<bool>,
    #[serde(default)]
    pub label_mode: Option<LabelMode>,
    #[serde(default)]
    pub label_prefix: Option<String>,
    /// Relation-manager relationship on the owner. Fields describe its related model.
    #[serde(default)]
    pub relationship: Option<String>,
    #[serde(default)]
    pub owner_model: Option<String>,
    #[serde(default)]
    pub relation_kind: Option<String>,
    #[serde(default)]
    pub related_title: Option<String>,
    #[serde(default)]
    pub nova_resources: BTreeMap<String, Vec<String>>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum LabelMode {
    #[default]
    None,
    Title,
    FieldPrefix,
}
#[derive(Clone, Debug)]
pub(crate) struct Settings {
    pub directory: String,
    pub namespace: String,
    pub base_class: Option<String>,
    pub postfix: String,
    pub proven_base: bool,
    pub proven_nova_resources: BTreeSet<String>,
    pub owner_relation_proven: bool,
}
fn quoted(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}
fn header(namespace: &str, body: &str) -> String {
    format!("<?php\n\ndeclare(strict_types=1);\n\nnamespace {namespace};\n\n{body}\n")
}
fn title(value: &str) -> String {
    value
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn label(request: &Request, name: &str) -> String {
    match request.label_mode.as_ref().unwrap_or(&LabelMode::None) {
        LabelMode::None => quoted(&title(name)),
        LabelMode::Title => format!("__({})", quoted(&title(name))),
        LabelMode::FieldPrefix => {
            format!(
                "__({})",
                quoted(&format!(
                    "{}{name}",
                    request.label_prefix.as_deref().unwrap_or("fields.")
                ))
            )
        }
    }
}
fn selected(selection: &Option<Vec<String>>, name: &str) -> bool {
    selection
        .as_ref()
        .is_none_or(|items| items.iter().any(|item| item == name))
}
fn options(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!("{} => {}", quoted(value), quoted(value)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
fn integer(kind: &str) -> bool {
    matches!(
        kind,
        "tinyInteger"
            | "smallInteger"
            | "mediumInteger"
            | "integer"
            | "bigInteger"
            | "unsignedTinyInteger"
            | "unsignedSmallInteger"
            | "unsignedMediumInteger"
            | "unsignedInteger"
            | "unsignedBigInteger"
    )
}
fn numeric(kind: &str) -> bool {
    integer(kind) || matches!(kind, "decimal" | "float" | "double")
}
fn validate(request: &Request, settings: &Settings) -> Result<(), String> {
    if !generator_plan::identifier(&request.name)
        || !generator_plan::qualified(&request.model)
        || !generator_plan::qualified(&settings.namespace)
        || settings.namespace.starts_with('\\')
        || !generator_plan::relative(&settings.directory)
        || !generator_plan::identifier(&format!("Generated{}", settings.postfix))
    {
        return Err("Invalid package resource name, model, namespace, directory or postfix".into());
    }
    if request
        .label_prefix
        .as_ref()
        .is_some_and(|prefix| prefix.len() > 128 || prefix.chars().any(char::is_control))
    {
        return Err("Field label prefixes must be bounded text without controls".into());
    }
    if request.kind != "filament-resource" && request.separate.is_some() {
        return Err(
            "Separate schema/table settings apply only to Filament resource generation".into(),
        );
    }
    if request.fields.len() > 128 || request.relations.len() > 64 {
        return Err("Package fields/relations exceed generation limits".into());
    }
    let mut fields = BTreeSet::from([request.primary_key.name.as_str()]);
    if !generator_plan::identifier(&request.primary_key.name) {
        return Err("Invalid package primary key".into());
    }
    for field in &request.fields {
        generator_plan::field_line(field)?;
        if !fields.insert(&field.name) {
            return Err("Duplicate package field".into());
        }
    }
    if request.kind != "nova-resource"
        && request
            .selected_relations
            .as_ref()
            .is_some_and(|relations| !relations.is_empty())
    {
        return Err("Use the proven relation-manager workflow for Filament relationships".into());
    }
    let mut relations = BTreeSet::new();
    for relation in &request.relations {
        if !generator_plan::identifier(&relation.name)
            || !relations.insert(relation.name.as_str())
            || fields.contains(relation.name.as_str())
            || relation.kind != "morphTo" && !generator_plan::qualified(&relation.related)
        {
            return Err("Invalid package relation".into());
        }
    }
    for (selection, allowed) in [
        (&request.selected_fields, &fields),
        (&request.selected_relations, &relations),
    ] {
        if let Some(selection) = selection {
            if selection.len() > allowed.len()
                || selection
                    .iter()
                    .any(|name| !allowed.contains(name.as_str()))
                || selection.iter().collect::<BTreeSet<_>>().len() != selection.len()
            {
                return Err(
                    "Choose unique package fields/relations from the captured model".into(),
                );
            }
        }
    }
    if request.nova_resources.len() > 64
        || request
            .nova_resources
            .keys()
            .any(|name| !relations.contains(name.as_str()))
    {
        return Err("Nova resource mappings require declared relationships".into());
    }
    Ok(())
}
pub(crate) fn major(version: &str) -> Option<u32> {
    let version = version.strip_prefix('v').unwrap_or(version);
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?;
    if minor.is_empty()
        || !minor.bytes().all(|ch| ch.is_ascii_digit())
        || version.contains(['^', '~', '*', ' ', '|'])
    {
        return None;
    }
    Some(major)
}
pub(crate) fn catalog(versions: &BTreeMap<String, String>) -> Value {
    let nova = versions
        .get("laravel/nova")
        .filter(|version| matches!(major(version), Some(4 | 5)));
    let filament = versions
        .get("filament/filament")
        .filter(|version| matches!(major(version), Some(3 | 4 | 5)));
    json!({"nova":nova,"filament":filament,"generators":[{"id":"nova-resource","label":"Nova resource","enabled":nova.is_some()},{"id":"filament-resource","label":"Filament resource","enabled":filament.is_some()},{"id":"filament-schema","label":"Filament form schema","enabled":filament.is_some_and(|version|major(version).unwrap_or(0)>=4)},{"id":"filament-table","label":"Filament table","enabled":filament.is_some()},{"id":"filament-relation-manager","label":"Filament relation manager","enabled":filament.is_some()}]})
}
fn nova_field(request: &Request, field: &Field) -> Result<String, String> {
    let kind = match field.kind.as_str() {
        "boolean" => "Boolean",
        "date" => "Date",
        "dateTime" | "dateTimeTz" | "timestamp" | "timestampTz" => "DateTime",
        "text" | "tinyText" | "mediumText" | "longText" => "Textarea",
        "json" | "jsonb"
            if matches!(
                field.cast.as_deref(),
                Some("array" | "json" | "encrypted:array")
            ) =>
        {
            "KeyValue"
        }
        "json" | "jsonb" => "Textarea",
        "enum" => "Select",
        "set" => "MultiSelect",
        kind if numeric(kind) => "Number",
        "binary" => {
            return Err("Binary Nova fields require an explicit upload/storage mapping".into());
        }
        _ => "Text",
    };
    let mut result = format!(
        "\\Laravel\\Nova\\Fields\\{kind}::make({}, {})",
        label(request, &field.name),
        quoted(&field.name)
    );
    if matches!(field.kind.as_str(), "enum" | "set") {
        result.push_str(&format!("->options({})", options(&field.values)));
    }
    if matches!(field.kind.as_str(), "json" | "jsonb") && kind == "Textarea" {
        result.push_str("->rules('json')");
    }
    if field.kind == "decimal" {
        result.push_str(&format!(
            "->step({})",
            quoted(&format!(
                "{:.*}",
                field.scale.unwrap_or(2) as usize,
                10f64.powi(-(field.scale.unwrap_or(2) as i32))
            ))
        ));
    }
    if field.nullable {
        result.push_str("->nullable()");
    } else {
        result.push_str("->rules('required')");
    }
    if let Some(length) = field.length {
        if matches!(field.kind.as_str(), "string" | "char") {
            result.push_str(&format!("->rules('max:{length}')"));
        }
    }
    Ok(result)
}
fn form_field(request: &Request, field: &Field) -> Result<String, String> {
    let component = match field.kind.as_str() {
        "boolean" => "Toggle",
        "date" => "DatePicker",
        "time" | "timeTz" => "TimePicker",
        "dateTime" | "dateTimeTz" | "timestamp" | "timestampTz" => "DateTimePicker",
        "text" | "tinyText" | "mediumText" | "longText" => "Textarea",
        "json" | "jsonb"
            if matches!(
                field.cast.as_deref(),
                Some("array" | "json" | "encrypted:array")
            ) =>
        {
            "KeyValue"
        }
        "json" | "jsonb" => "Textarea",
        "enum" => "Select",
        "set" => "CheckboxList",
        "binary" => {
            return Err("Binary Filament fields require an explicit upload/storage mapping".into());
        }
        _ => "TextInput",
    };
    let mut value = format!(
        "\\Filament\\Forms\\Components\\{component}::make({})->label({})",
        quoted(&field.name),
        label(request, &field.name)
    );
    if matches!(field.kind.as_str(), "json" | "jsonb") && component == "Textarea" {
        value.push_str("->json()");
    }
    if numeric(&field.kind) {
        value.push_str("->numeric()");
        if field.kind == "decimal" {
            value.push_str(&format!(
                "->step({})",
                quoted(&format!(
                    "{:.*}",
                    field.scale.unwrap_or(2) as usize,
                    10f64.powi(-(field.scale.unwrap_or(2) as i32))
                ))
            ));
        }
    }
    if matches!(field.kind.as_str(), "enum" | "set") {
        value.push_str(&format!("->options({})", options(&field.values)));
    }
    if let Some(length) = field.length {
        if matches!(field.kind.as_str(), "string" | "char") {
            value.push_str(&format!("->maxLength({length})"));
        }
    }
    if !field.nullable {
        value.push_str("->required()");
    }
    Ok(value)
}
fn table_field(request: &Request, field: &Field) -> Result<String, String> {
    if field.kind == "binary" {
        return Err("Binary table fields require an explicit display mapping".into());
    }
    let component = if field.kind == "boolean" {
        "IconColumn"
    } else {
        "TextColumn"
    };
    let mut value = format!(
        "\\Filament\\Tables\\Columns\\{component}::make({})->label({})",
        quoted(&field.name),
        label(request, &field.name)
    );
    match field.kind.as_str() {
        "boolean" => value.push_str("->boolean()"),
        "date" => value.push_str("->date()"),
        "dateTime" | "dateTimeTz" | "timestamp" | "timestampTz" => value.push_str("->dateTime()"),
        "json" | "jsonb" | "set" => value.push_str("->listWithLineBreaks()"),
        "enum" => value.push_str("->badge()"),
        kind if numeric(kind) => {
            value.push_str(&format!(
                "->numeric(decimalPlaces: {})",
                if kind == "decimal" {
                    field.scale.unwrap_or(2)
                } else {
                    0
                }
            ));
        }
        _ => {
            value.push_str("->searchable()");
        }
    }
    value.push_str("->sortable()");
    Ok(value)
}
fn lines(values: Vec<String>) -> String {
    values
        .into_iter()
        .map(|value| format!("            {value},\n"))
        .collect()
}
fn body_fields(
    request: &Request,
    mapper: fn(&Request, &Field) -> Result<String, String>,
) -> Result<String, String> {
    let mut values = Vec::new();
    for field in request
        .fields
        .iter()
        .filter(|field| selected(&request.selected_fields, &field.name))
    {
        values.push(mapper(request, field)?);
    }
    Ok(lines(values))
}
fn page(namespace: &str, name: &str, resource: &str, base: &str) -> String {
    header(
        namespace,
        &format!(
            "class {name} extends \\Filament\\Resources\\Pages\\{base}\n{{\n    protected static string $resource = \\{resource}::class;\n}}"
        ),
    )
}
pub(crate) fn plan(request: &Request, version: &str, settings: &Settings) -> Result<Plan, String> {
    validate(request, settings)?;
    let major =
        major(version).ok_or("Package generation requires an exact known installed version")?;
    let nova = request.kind == "nova-resource";
    if nova && !matches!(major, 4 | 5) || !nova && !matches!(major, 3 | 4 | 5) {
        return Err("Unsupported Nova/Filament major; refresh installed package metadata".into());
    }
    let class = format!("{}{}", request.name, settings.postfix);
    if !generator_plan::identifier(&class) {
        return Err("Invalid package class name".into());
    }
    let default_base = if nova {
        "Laravel\\Nova\\Resource"
    } else if request.kind == "filament-relation-manager" {
        "Filament\\Resources\\RelationManagers\\RelationManager"
    } else {
        "Filament\\Resources\\Resource"
    };
    let base = settings.base_class.as_deref().unwrap_or(default_base);
    if !generator_plan::qualified(base) || base != default_base && !settings.proven_base {
        return Err("Configured package base must be a proven indexed subclass".into());
    }
    if matches!(request.kind.as_str(), "filament-schema" | "filament-table")
        && settings.base_class.is_some()
    {
        return Err("Separated schema/table helpers do not use resource base classes".into());
    }
    if nova {
        let mut values = Vec::new();
        if selected(&request.selected_fields, &request.primary_key.name) {
            values.push(format!(
                "\\Laravel\\Nova\\Fields\\ID::make({}, {})->sortable()",
                label(request, &request.primary_key.name),
                quoted(&request.primary_key.name)
            ));
        }
        for field in request
            .fields
            .iter()
            .filter(|field| selected(&request.selected_fields, &field.name))
        {
            values.push(nova_field(request, field)?);
        }
        for relation in request
            .relations
            .iter()
            .filter(|relation| selected(&request.selected_relations, &relation.name))
        {
            let kind = match relation.kind.as_str() {
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
                _ => return Err("Unsupported Nova relationship".into()),
            };
            let resources = request
                .nova_resources
                .get(&relation.name)
                .cloned()
                .unwrap_or_default();
            if resources.len() > 32
                || resources
                    .iter()
                    .any(|class| !settings.proven_nova_resources.contains(class))
            {
                return Err(
                    "Nova relation targets require current indexed Nova resource classes".into(),
                );
            }
            if relation.kind == "morphTo" {
                if resources.is_empty() {
                    return Err(
                        "Choose indexed Nova resource types for polymorphic MorphTo fields".into(),
                    );
                }
                values.push(format!(
                    "\\Laravel\\Nova\\Fields\\{kind}::make({}, {})->types([{}])",
                    label(request, &relation.name),
                    quoted(&relation.name),
                    resources
                        .iter()
                        .map(|class| format!("\\{class}::class"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            } else {
                if resources.len() > 1 {
                    return Err("Non-polymorphic Nova relations select one resource class".into());
                }
                let explicit = resources
                    .first()
                    .map(|class| format!(", \\{class}::class"))
                    .unwrap_or_default();
                values.push(format!(
                    "\\Laravel\\Nova\\Fields\\{kind}::make({}, {}{explicit})",
                    label(request, &relation.name),
                    quoted(&relation.name)
                ));
            }
        }
        return Ok(Plan {
            files: vec![Output {
                path: format!("{}/{class}.php", settings.directory),
                text: header(
                    &settings.namespace,
                    &format!(
                        "class {class} extends \\{base}\n{{\n    public static $model = \\{}::class;\n\n    public function fields(\\Laravel\\Nova\\Http\\Requests\\NovaRequest $request): array\n    {{\n        return [\n{}        ];\n    }}\n}}",
                        request.model.trim_start_matches('\\'),
                        lines(values)
                    ),
                ),
            }],
        });
    }
    if ![
        "filament-resource",
        "filament-schema",
        "filament-table",
        "filament-relation-manager",
    ]
    .contains(&request.kind.as_str())
    {
        return Err("Unknown package generation kind".into());
    }
    if request.kind == "filament-schema" && major == 3 {
        return Err("Separated Schema classes require Filament 4 or 5".into());
    }
    let schema = if major == 3 {
        "Filament\\Forms\\Form"
    } else {
        "Filament\\Schemas\\Schema"
    };
    let schema_method = if major == 3 { "schema" } else { "components" };
    let form_fields = body_fields(request, form_field)?;
    let mut table_values = Vec::new();
    if selected(&request.selected_fields, &request.primary_key.name) {
        table_values.push(format!(
            "\\Filament\\Tables\\Columns\\TextColumn::make({})->sortable()",
            quoted(&request.primary_key.name)
        ));
    }
    for field in request
        .fields
        .iter()
        .filter(|field| selected(&request.selected_fields, &field.name))
    {
        table_values.push(table_field(request, field)?);
    }
    let table_fields = lines(table_values);
    let configure_form = format!(
        "    public static function configure(\\{schema} $schema): \\{schema}\n    {{\n        return $schema->{schema_method}([\n{form_fields}        ]);\n    }}"
    );
    let configure_table = format!(
        "    public static function configure(\\Filament\\Tables\\Table $table): \\Filament\\Tables\\Table\n    {{\n        return $table->columns([\n{table_fields}        ]);\n    }}"
    );
    if request.kind == "filament-schema" || request.kind == "filament-table" {
        return Ok(Plan {
            files: vec![Output {
                path: format!("{}/{class}.php", settings.directory),
                text: header(
                    &settings.namespace,
                    &format!(
                        "class {class}\n{{\n{}\n}}",
                        if request.kind == "filament-schema" {
                            configure_form
                        } else {
                            configure_table
                        }
                    ),
                ),
            }],
        });
    }
    let form = format!(
        "    public {}function form(\\{schema} $schema): \\{schema}\n    {{\n        return $schema->{schema_method}([\n{form_fields}        ]);\n    }}",
        if request.kind == "filament-resource" {
            "static "
        } else {
            ""
        }
    );
    let table = format!(
        "    public {}function table(\\Filament\\Tables\\Table $table): \\Filament\\Tables\\Table\n    {{\n        return $table->columns([\n{table_fields}        ]);\n    }}",
        if request.kind == "filament-resource" {
            "static "
        } else {
            ""
        }
    );
    if request.kind == "filament-relation-manager" {
        if !settings.owner_relation_proven
            || request
                .owner_model
                .as_deref()
                .is_none_or(|model| !generator_plan::qualified(model))
        {
            return Err(
                "Relation manager owner and relationship require current indexed source proof"
                    .into(),
            );
        }
        let relationship = request
            .relationship
            .as_deref()
            .filter(|value| generator_plan::identifier(value))
            .ok_or("Select a declared owner relationship")?;
        if !matches!(
            request.relation_kind.as_deref(),
            Some(
                "hasMany"
                    | "belongsToMany"
                    | "hasManyThrough"
                    | "morphMany"
                    | "morphToMany"
                    | "morphedByMany"
            )
        ) {
            return Err("Relation managers require a proven collection relationship".into());
        }
        let record = request
            .related_title
            .as_deref()
            .unwrap_or(&request.primary_key.name);
        if !request.fields.iter().any(|field| field.name == record)
            && record != request.primary_key.name
        {
            return Err("Choose a declared related record title field".into());
        }
        let table = table.replace(
            "return $table->columns(",
            &format!(
                "return $table->recordTitleAttribute({})->columns(",
                quoted(record)
            ),
        );
        return Ok(Plan {
            files: vec![Output {
                path: format!("{}/{class}.php", settings.directory),
                text: header(
                    &settings.namespace,
                    &format!(
                        "class {class} extends \\{base}\n{{\n    protected static string $relationship = {};\n\n{form}\n\n{table}\n}}",
                        quoted(relationship)
                    ),
                ),
            }],
        });
    }
    let resource = format!("{}\\{class}", settings.namespace);
    let mut files = Vec::new();
    let (form, table) = if request.separate.unwrap_or(true) && major >= 4 {
        let form_class = format!("{}Form", request.name);
        let table_class = format!("{}Table", request.name);
        files.push(Output {
            path: format!("{}/Schemas/{form_class}.php", settings.directory),
            text: header(
                &format!("{}\\Schemas", settings.namespace),
                &format!("class {form_class}\n{{\n{configure_form}\n}}"),
            ),
        });
        files.push(Output {
            path: format!("{}/Tables/{table_class}.php", settings.directory),
            text: header(
                &format!("{}\\Tables", settings.namespace),
                &format!("class {table_class}\n{{\n{configure_table}\n}}"),
            ),
        });
        (
            format!(
                "    public static function form(\\{schema} $schema): \\{schema}\n    {{\n        return \\{}\\Schemas\\{form_class}::configure($schema);\n    }}",
                settings.namespace
            ),
            format!(
                "    public static function table(\\Filament\\Tables\\Table $table): \\Filament\\Tables\\Table\n    {{\n        return \\{}\\Tables\\{table_class}::configure($table);\n    }}",
                settings.namespace
            ),
        )
    } else {
        (form, table)
    };
    let page_namespace = if major >= 4 {
        format!("{}\\Pages", settings.namespace)
    } else {
        format!("{}\\{class}\\Pages", settings.namespace)
    };
    let page_root = if major >= 4 {
        format!("{}/Pages", settings.directory)
    } else {
        format!("{}/{class}/Pages", settings.directory)
    };
    let mut routes = Vec::new();
    for (prefix, page_base, route) in [
        ("List", "ListRecords", "/"),
        ("Create", "CreateRecord", "/create"),
        ("Edit", "EditRecord", "/{record}/edit"),
    ] {
        let name = format!("{prefix}{}", request.name);
        files.push(Output {
            path: format!("{page_root}/{name}.php"),
            text: page(&page_namespace, &name, &resource, page_base),
        });
        let route_key = if prefix == "List" {
            "index".to_string()
        } else {
            prefix.to_ascii_lowercase()
        };
        routes.push(format!(
            "            {} => \\{page_namespace}\\{name}::route({}),",
            quoted(&route_key),
            quoted(route)
        ));
    }
    files.push(Output{path:format!("{}/{class}.php",settings.directory),text:header(&settings.namespace,&format!("class {class} extends \\{base}\n{{\n    protected static ?string $model = \\{}::class;\n\n{form}\n\n{table}\n\n    public static function getRelations(): array\n    {{\n        return [];\n    }}\n\n    public static function getPages(): array\n    {{\n        return [\n{}\n        ];\n    }}\n}}",request.model.trim_start_matches('\\'),routes.join("\n")))});
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(Plan { files })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(kind: &str) -> Request {
        serde_json::from_value(json!({"kind":kind,"name":"Post","model":"App\\Models\\Post","fields":[{"name":"title","kind":"string","length":80,"nullable":false},{"name":"active","kind":"boolean","nullable":false},{"name":"amount","kind":"decimal","precision":12,"scale":2,"nullable":true},{"name":"status","kind":"enum","values":["draft","published"],"nullable":false},{"name":"metadata","kind":"json","cast":"array","nullable":true},{"name":"body","kind":"text","nullable":true},{"name":"published_at","kind":"timestampTz","nullable":true}],"selectedRelations":[]})).unwrap()
    }
    fn settings(nova: bool) -> Settings {
        Settings {
            directory: if nova {
                "app/Nova"
            } else {
                "app/Filament/Resources/Posts"
            }
            .into(),
            namespace: if nova {
                "App\\Nova"
            } else {
                "App\\Filament\\Resources\\Posts"
            }
            .into(),
            base_class: None,
            postfix: if nova { "" } else { "Resource" }.into(),
            proven_base: false,
            proven_nova_resources: Default::default(),
            owner_relation_proven: false,
        }
    }
    #[test]
    fn nova_maps_typed_fields_translations_and_declared_relations() {
        let mut request = request("nova-resource");
        request.label_mode = Some(LabelMode::FieldPrefix);
        request.label_prefix = Some("admin.fields.".into());
        request.selected_relations = None;
        request.relations=serde_json::from_value(json!([{"name":"author","kind":"belongsTo","related":"App\\Models\\User","arguments":[]}])).unwrap();
        for version in ["4.35.0", "5.9.0"] {
            let plan = plan(&request, version, &settings(true)).unwrap();
            let text = &plan.files[0].text;
            for class in [
                "ID",
                "Text",
                "Boolean",
                "Number",
                "Select",
                "KeyValue",
                "Textarea",
                "DateTime",
                "BelongsTo",
            ] {
                assert!(
                    text.contains(&format!("\\Laravel\\Nova\\Fields\\{class}::make")),
                    "{class}"
                );
            }
            assert!(text.contains("__('admin.fields.title')"));
            assert!(text.contains("->options(['draft' => 'draft', 'published' => 'published'])"));
            assert!(text.contains("->nullable()"));
        }
    }
    #[test]
    fn filament_versions_generate_actual_schema_table_pages_and_manager_contracts() {
        for version in ["3.3.57", "4.16.2", "5.11.2"] {
            let request = request("filament-resource");
            let plan = plan(&request, version, &settings(false)).unwrap();
            let major = major(version).unwrap();
            assert_eq!(plan.files.len(), if major == 3 { 4 } else { 6 });
            let resource = &plan
                .files
                .iter()
                .find(|file| file.path.ends_with("PostResource.php"))
                .unwrap()
                .text;
            assert!(resource.contains("'index' =>"));
            assert!(resource.contains(if major == 3 {
                "\\Filament\\Forms\\Form"
            } else {
                "\\Filament\\Schemas\\Schema"
            }));
            assert!(plan.files.iter().any(|file| {
                file.text
                    .contains("\\Filament\\Forms\\Components\\Select::make('status')")
            }));
            assert!(plan.files.iter().any(|file| {
                file.text
                    .contains("\\Filament\\Tables\\Columns\\IconColumn::make('active')")
            }));
        }
        let mut manager = request("filament-relation-manager");
        manager.owner_model = Some("App\\Models\\Owner".into());
        manager.relationship = Some("posts".into());
        manager.relation_kind = Some("hasMany".into());
        let mut settings = settings(false);
        settings.owner_relation_proven = true;
        settings.postfix = "RelationManager".into();
        let text = plan(&manager, "4.16.2", &settings).unwrap().files[0]
            .text
            .clone();
        assert!(text.contains("protected static string $relationship = 'posts'"));
        assert!(text.contains("public function form("));
        assert!(text.contains("->recordTitleAttribute('id')"));
    }
    #[test]
    fn filament_relationship_selection_is_never_silently_discarded() {
        let mut request = request("filament-resource");
        request.selected_relations = Some(vec!["comments".into()]);
        request.relations = serde_json::from_value(json!([{"name":"comments","kind":"hasMany","related":"App\\Models\\Comment","arguments":[]}])).unwrap();
        assert!(
            plan(&request, "5.11.2", &settings(false))
                .unwrap_err()
                .contains("relation-manager")
        );
    }
    #[test]
    fn package_unknown_versions_names_literals_and_unproven_bases_are_refused() {
        for version in ["^4.0", "dev-main", "6.0.0", "4.*"] {
            assert!(plan(&request("filament-resource"), version, &settings(false)).is_err());
        }
        let mut request = request("nova-resource");
        request.fields[0].name = "title);execute()".into();
        assert!(plan(&request, "5.0.0", &settings(true)).is_err());
        let mut configuration = settings(false);
        configuration.base_class = Some("App\\UnprovenResource".into());
        assert!(
            plan(
                &self::request("filament-resource"),
                "4.16.2",
                &configuration
            )
            .is_err()
        );
        assert!(
            plan(
                &self::request("filament-schema"),
                "3.3.57",
                &settings(false)
            )
            .is_err()
        );
        let mut request = self::request("filament-relation-manager");
        request.relationship = Some("posts".into());
        request.relation_kind = Some("hasMany".into());
        assert!(plan(&request, "4.16.2", &settings(false)).is_err());
    }
    #[test]
    fn nova_polymorphic_types_require_indexed_resources_and_safe_collection_inverse_mapping() {
        let mut request = request("nova-resource");
        request.selected_relations = None;
        request.relations = serde_json::from_value(
            json!([{"name":"commentable","kind":"morphTo","related":"","arguments":[]}]),
        )
        .unwrap();
        assert!(plan(&request, "5.0.0", &settings(true)).is_err());
        let mut settings = settings(true);
        settings
            .proven_nova_resources
            .insert("App\\Nova\\Post".into());
        request
            .nova_resources
            .insert("commentable".into(), vec!["App\\Nova\\Post".into()]);
        assert!(
            plan(&request, "5.0.0", &settings).unwrap().files[0]
                .text
                .contains("->types([\\App\\Nova\\Post::class])")
        );
        request
            .nova_resources
            .insert("commentable".into(), vec!["Unproved\\Type".into()]);
        assert!(plan(&request, "5.0.0", &settings).is_err());
    }
    #[test]
    #[ignore = "requires local PHP executable; syntax validation does not substitute for licensed Nova runtime acceptance"]
    fn generated_nova_php_lints_supported_versions_and_proven_polymorphic_types() {
        let directory = std::env::temp_dir().join(format!(
            "laravel-nova-syntax-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let mut request = request("nova-resource");
        request.label_mode = Some(LabelMode::FieldPrefix);
        request.label_prefix = Some("admin's.fields.".into());
        request.relations=serde_json::from_value(json!([{"name":"commentable","kind":"morphTo","related":"","arguments":[]},{"name":"comments","kind":"hasMany","related":"App\\Models\\Comment","arguments":[]},{"name":"tags","kind":"morphedByMany","related":"App\\Models\\Tag","arguments":[]}])).unwrap();
        request.selected_relations = None;
        request.nova_resources = BTreeMap::from([
            (
                "commentable".into(),
                vec!["App\\Nova\\Article".into(), "App\\Nova\\Video".into()],
            ),
            ("comments".into(), vec!["App\\Nova\\Comment".into()]),
        ]);
        let mut layout = settings(true);
        layout.proven_nova_resources = request.nova_resources.values().flatten().cloned().collect();
        for version in ["4.35.0", "5.9.0"] {
            for file in plan(&request, version, &layout).unwrap().files {
                let target = directory.join(format!("v{}.php", major(version).unwrap()));
                std::fs::write(&target, file.text).unwrap();
                let output = std::process::Command::new("php")
                    .arg("-l")
                    .arg(&target)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    #[ignore = "requires explicit installed Filament3/4/5 Composer fixtures; loads real package classes without application SQL"]
    fn installed_filament_generated_classes_and_component_apis() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "laravel-package-generated-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        for major in [3, 4, 5] {
            let fixture = std::env::var(format!("FILAMENT_{major}_FIXTURE"))
                .unwrap_or_else(|_| format!("/tmp/laravel-parity-filament-{major}-20261110"));
            let lock: Value = serde_json::from_str(
                &std::fs::read_to_string(format!("{fixture}/composer.lock"))
                    .expect("Install the opt-in fixture first"),
            )
            .unwrap();
            let version = lock["packages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|package| package["name"] == "filament/filament")
                .unwrap()["version"]
                .as_str()
                .unwrap();
            let directory = root.join(format!("v{major}"));
            std::fs::create_dir(&directory).unwrap();
            let request = request("filament-resource");
            let mut outputs = plan(&request, version, &settings(false)).unwrap();
            for (kind, postfix) in [
                ("filament-table", "Table"),
                ("filament-relation-manager", "RelationManager"),
                ("filament-schema", "Schema"),
            ] {
                if major == 3 && kind == "filament-schema" {
                    continue;
                }
                let mut standalone = self::request(kind);
                standalone.owner_model = Some("App\\Models\\Owner".into());
                standalone.relationship = Some("posts".into());
                standalone.relation_kind = Some("hasMany".into());
                let mut layout = settings(false);
                layout.postfix = postfix.into();
                layout.owner_relation_proven = true;
                outputs
                    .files
                    .extend(plan(&standalone, version, &layout).unwrap().files);
            }
            let mut files = Vec::new();
            for output in outputs.files {
                let target = directory.join(&output.path);
                std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                std::fs::write(&target, output.text).unwrap();
                let lint = std::process::Command::new("php")
                    .arg("-l")
                    .arg(&target)
                    .output()
                    .unwrap();
                assert!(
                    lint.status.success(),
                    "{}",
                    String::from_utf8_lossy(&lint.stderr)
                );
                files.push(target);
            }
            let file_json = serde_json::to_string(
                &files
                    .iter()
                    .map(|file| file.to_string_lossy().to_string())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            let script = r#"<?php
namespace { require $argv[1].'/vendor/autoload.php'; $app=new \Illuminate\Foundation\Application($argv[1]); $app->instance('config',new \Illuminate\Config\Repository(['app'=>['locale'=>'en','name'=>'Acceptance']])); $app->instance('translator',new \Illuminate\Translation\Translator(new \Illuminate\Translation\ArrayLoader(),'en')); \Illuminate\Support\Facades\Facade::setFacadeApplication($app); }
namespace App\Models {class Post extends \Illuminate\Database\Eloquent\Model {protected $guarded=[];protected $casts=['metadata'=>'array'];}}
namespace {foreach(json_decode($argv[2],true,512,JSON_THROW_ON_ERROR) as $file)require_once $file; $resource=\App\Filament\Resources\Posts\PostResource::class; $schemaClass=(int)$argv[3]===3?\Filament\Forms\Form::class:\Filament\Schemas\Schema::class; $schema=(new \ReflectionClass($schemaClass))->newInstanceWithoutConstructor();$form=$resource::form($schema);if(count($form->getComponents())!==7)throw new \RuntimeException('Unexpected generated form field count');$table=(new \ReflectionClass(\Filament\Tables\Table::class))->newInstanceWithoutConstructor();$table=$resource::table($table);if(count($table->getColumns())!==8)throw new \RuntimeException('Unexpected generated table column count');if(!isset($resource::getPages()['index']))throw new \RuntimeException('Missing index route');$configured=\App\Filament\Resources\Posts\PostTable::configure((new \ReflectionClass(\Filament\Tables\Table::class))->newInstanceWithoutConstructor());if(count($configured->getColumns())!==8)throw new \RuntimeException('Standalone table failed');if((int)$argv[3]>=4){$configured=\App\Filament\Resources\Posts\PostSchema::configure((new \ReflectionClass($schemaClass))->newInstanceWithoutConstructor());if(count($configured->getComponents())!==7)throw new \RuntimeException('Standalone schema failed');}$manager=(new \ReflectionClass(\App\Filament\Resources\Posts\PostRelationManager::class))->newInstanceWithoutConstructor();$configured=$manager->form((new \ReflectionClass($schemaClass))->newInstanceWithoutConstructor());if(count($configured->getComponents())!==7)throw new \RuntimeException('Relation-manager form failed');$configured=$manager->table((new \ReflectionClass(\Filament\Tables\Table::class))->newInstanceWithoutConstructor());if(count($configured->getColumns())!==8)throw new \RuntimeException('Relation-manager table failed');echo 'PASS installed Filament '.$argv[3].' field/schema/table/page/manager APIs'.PHP_EOL;}
"#;
            let script_path = directory.join("accept.php");
            std::fs::write(&script_path, script).unwrap();
            let output = std::process::Command::new("php")
                .arg(&script_path)
                .arg(&fixture)
                .arg(file_json)
                .arg(major.to_string())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "Filament{major}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
