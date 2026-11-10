//! Shape and unsupported-key diagnostics for the supported public metadata domains.
use serde_json::Value;
#[derive(Clone, Copy)]
enum Shape {
    Object,
    Array,
    String,
    Bool,
    Integer,
}
fn object(
    source: &str,
    path: &str,
    value: &Value,
    fields: &[(&str, Shape)],
    issues: &mut Vec<String>,
) {
    let Some(map) = value.as_object() else {
        return;
    };
    for (key, value) in map {
        let name = if path.is_empty() {
            key.clone()
        } else {
            format!("{path}.{key}")
        };
        match fields.iter().find(|(field, _)| *field == key) {
            None => issues.push(format!("{source}: unsupported metadata field {name}")),
            Some((_, shape)) => {
                let valid = match shape {
                    Shape::Object => value.is_object(),
                    Shape::Array => value.is_array(),
                    Shape::String => value.is_string(),
                    Shape::Bool => value.is_boolean(),
                    Shape::Integer => value.as_u64().is_some(),
                };
                if !valid {
                    issues.push(format!("{source}: malformed metadata field {name}"));
                }
            }
        }
    }
}
fn items<'a>(value: &'a Value) -> impl Iterator<Item = &'a Value> {
    value.as_array().into_iter().flatten()
}
fn string_items(source: &str, path: &str, value: &Value, issues: &mut Vec<String>) {
    for (index, item) in items(value).enumerate() {
        if !item.is_string() {
            issues.push(format!(
                "{source}: malformed metadata field {path}[{index}] (expected string)"
            ));
        }
    }
}
fn object_items(
    source: &str,
    path: &str,
    value: &Value,
    fields: &[(&str, Shape)],
    issues: &mut Vec<String>,
) {
    for (index, item) in items(value).enumerate() {
        let path = format!("{path}[{index}]");
        if !item.is_object() {
            issues.push(format!(
                "{source}: malformed metadata field {path} (expected object)"
            ));
        } else {
            object(source, &path, item, fields, issues);
        }
    }
}
fn required(source: &str, path: &str, value: &Value, fields: &[&str], issues: &mut Vec<String>) {
    if !value.is_object() {
        return;
    }
    for field in fields {
        if value
            .get(*field)
            .is_none_or(|v| v.is_null() || v.as_str().is_some_and(str::is_empty))
        {
            issues.push(format!(
                "{source}: missing required metadata field {path}.{field}"
            ));
        }
    }
}
pub(super) fn issues(source: &str, value: &Value) -> Vec<String> {
    use Shape::*;
    let mut out = Vec::new();
    object(
        source,
        "",
        value,
        &[
            ("$schema", String),
            ("completions", Array),
            ("view", Object),
            ("routing", Object),
            ("auth", Object),
            ("eloquent", Object),
            ("blade", Object),
            ("packages", Object),
            ("database", Object),
            ("helperCode", Object),
            ("inertia", Object),
        ],
        &mut out,
    );
    object(
        source,
        "view",
        &value["view"],
        &[("paths", Array)],
        &mut out,
    );
    object_items(
        source,
        "view.paths",
        &value["view"]["paths"],
        &[("path", String), ("namespace", String)],
        &mut out,
    );
    object(
        source,
        "routing",
        &value["routing"],
        &[
            ("rootFiles", Array),
            ("routeFacadeMethods", Array),
            ("additionalMiddleware", Array),
        ],
        &mut out,
    );
    object(
        source,
        "auth",
        &value["auth"],
        &[("additionalGates", Array)],
        &mut out,
    );
    string_items(
        source,
        "auth.additionalGates",
        &value["auth"]["additionalGates"],
        &mut out,
    );
    object(
        source,
        "eloquent",
        &value["eloquent"],
        &[("modelTables", Array)],
        &mut out,
    );
    object_items(
        source,
        "eloquent.modelTables",
        &value["eloquent"]["modelTables"],
        &[("model", String), ("table", String)],
        &mut out,
    );
    for (index, item) in items(&value["eloquent"]["modelTables"]).enumerate() {
        required(
            source,
            &format!("eloquent.modelTables[{index}]"),
            item,
            &["model", "table"],
            &mut out,
        );
        if let Some(model) = item["model"].as_str() {
            if super::class_name(&item["model"]).is_none() {
                out.push(format!(
                    "{source}: invalid eloquent.modelTables model {model}"
                ));
            }
        }
    }
    string_items(
        source,
        "routing.additionalMiddleware",
        &value["routing"]["additionalMiddleware"],
        &mut out,
    );
    object_items(
        source,
        "routing.routeFacadeMethods",
        &value["routing"]["routeFacadeMethods"],
        &[
            ("name", String),
            ("urlIndex", Integer),
            ("controllerIndex", Integer),
            ("httpMethod", String),
        ],
        &mut out,
    );
    for (index, item) in items(&value["routing"]["routeFacadeMethods"]).enumerate() {
        let path = format!("routing.routeFacadeMethods[{index}]");
        required(source, &path, item, &["name"], &mut out);
        if item["name"]
            .as_str()
            .is_some_and(|name| !super::identifier(name))
        {
            out.push(format!("{source}: invalid {path}.name"));
        }
        for key in ["urlIndex", "controllerIndex"] {
            if item
                .get(key)
                .is_some_and(|v| v.as_u64().is_none_or(|n| n == 0 || n > 256))
            {
                out.push(format!(
                    "{source}: invalid {path}.{key} (expected 1-based index up to 256)"
                ));
            }
        }
    }
    string_items(
        source,
        "routing.rootFiles",
        &value["routing"]["rootFiles"],
        &mut out,
    );
    object(
        source,
        "blade",
        &value["blade"],
        &[
            ("directives", Array),
            ("ifDirectives", Array),
            ("scriptDirectives", Array),
            ("components", Object),
        ],
        &mut out,
    );
    for kind in ["directives", "ifDirectives", "scriptDirectives"] {
        object_items(
            source,
            &format!("blade.{kind}"),
            &value["blade"][kind],
            &[("name", String), ("prefix", String), ("suffix", String)],
            &mut out,
        );
    }
    object(
        source,
        "packages",
        &value["packages"],
        &[
            ("enabledPackages", Object),
            ("disabledPackages", Array),
            ("livewire", Object),
            ("inertia", Object),
            ("passport", Object),
        ],
        &mut out,
    );
    if let Some(map) = value["packages"]["enabledPackages"].as_object() {
        for (name, version) in map {
            if !version.is_string() {
                out.push(format!(
                    "{source}: malformed packages.enabledPackages.{name} (expected version string)"
                ));
            }
        }
    }
    string_items(
        source,
        "packages.disabledPackages",
        &value["packages"]["disabledPackages"],
        &mut out,
    );
    object(
        source,
        "packages.livewire",
        &value["packages"]["livewire"],
        &[("namespaces", Array)],
        &mut out,
    );
    object_items(
        source,
        "packages.livewire.namespaces",
        &value["packages"]["livewire"]["namespaces"],
        &[
            ("classNamespace", String),
            ("viewPath", String),
            ("tagPrefix", String),
        ],
        &mut out,
    );
    for path in ["packages.inertia", "inertia"] {
        let node = if path == "inertia" {
            &value["inertia"]
        } else {
            &value["packages"]["inertia"]
        };
        object(
            source,
            path,
            node,
            &[("pagePaths", Array), ("ignoreSettingsPaths", Bool)],
            &mut out,
        );
        object_items(
            source,
            &format!("{path}.pagePaths"),
            &node["pagePaths"],
            &[("path", String), ("prefix", String), ("suffix", String)],
            &mut out,
        );
    }
    object(
        source,
        "packages.passport",
        &value["packages"]["passport"],
        &[("scopes", Object), ("scopeFiles", Array)],
        &mut out,
    );
    string_items(
        source,
        "packages.passport.scopeFiles",
        &value["packages"]["passport"]["scopeFiles"],
        &mut out,
    );
    if let Some(map) = value["packages"]["passport"]["scopes"].as_object() {
        for (name, description) in map {
            if !description.is_string() {
                out.push(format!("{source}: malformed packages.passport.scopes.{name} (expected description string)"));
            }
        }
    }
    object(
        source,
        "database",
        &value["database"],
        &[
            ("columnMigrationMethods", Array),
            ("schemaDirectories", Array),
            ("schemaFilePaths", Array),
        ],
        &mut out,
    );
    for key in ["schemaDirectories", "schemaFilePaths"] {
        string_items(
            source,
            &format!("database.{key}"),
            &value["database"][key],
            &mut out,
        );
    }
    object_items(
        source,
        "database.columnMigrationMethods",
        &value["database"]["columnMigrationMethods"],
        &[
            ("name", String),
            ("type", String),
            ("nameParameterIndex", Integer),
            ("columnNameTemplate", String),
            ("defaultName", String),
            ("columns", Array),
        ],
        &mut out,
    );
    for (index, method) in items(&value["database"]["columnMigrationMethods"]).enumerate() {
        object_items(
            source,
            &format!("database.columnMigrationMethods[{index}].columns"),
            &method["columns"],
            &[("name", String), ("type", String)],
            &mut out,
        );
    }
    object(
        source,
        "helperCode",
        &value["helperCode"],
        &[
            ("eloquent", Object),
            ("classMixins", Array),
            ("methodsWithConstructorParameters", Array),
        ],
        &mut out,
    );
    object(
        source,
        "helperCode.eloquent",
        &value["helperCode"]["eloquent"],
        &[("ignoreClasses", Array), ("ignoreNamespaces", Array)],
        &mut out,
    );
    for key in ["ignoreClasses", "ignoreNamespaces"] {
        string_items(
            source,
            &format!("helperCode.eloquent.{key}"),
            &value["helperCode"]["eloquent"][key],
            &mut out,
        );
    }
    object_items(
        source,
        "helperCode.classMixins",
        &value["helperCode"]["classMixins"],
        &[("classFqn", String), ("mixinFqn", String)],
        &mut out,
    );
    object_items(
        source,
        "helperCode.methodsWithConstructorParameters",
        &value["helperCode"]["methodsWithConstructorParameters"],
        &[("baseFqn", String), ("methods", Array)],
        &mut out,
    );
    for (index, base) in items(&value["helperCode"]["methodsWithConstructorParameters"]).enumerate()
    {
        let path = format!("helperCode.methodsWithConstructorParameters[{index}].methods");
        object_items(
            source,
            &path,
            &base["methods"],
            &[
                ("name", String),
                ("returnType", String),
                ("additionalParameters", Array),
            ],
            &mut out,
        );
        for (index, method) in items(&base["methods"]).enumerate() {
            object_items(
                source,
                &format!("{path}[{index}].additionalParameters"),
                &method["additionalParameters"],
                &[("name", String), ("type", String)],
                &mut out,
            );
        }
    }
    object_items(
        source,
        "completions",
        &value["completions"],
        &[
            ("complete", String),
            ("condition", Array),
            ("options", Object),
        ],
        &mut out,
    );
    for (index, completion) in items(&value["completions"]).enumerate() {
        let path = format!("completions[{index}]");
        object_items(
            source,
            &format!("{path}.condition"),
            &completion["condition"],
            &[
                ("functionNames", Array),
                ("functionFqn", Array),
                ("parameterNames", Array),
                ("methodNames", Array),
                ("classFqn", Array),
                ("newClassFqn", Array),
                ("parameters", Array),
                ("place", String),
            ],
            &mut out,
        );
        for (i, condition) in items(&completion["condition"]).enumerate() {
            for key in [
                "functionNames",
                "functionFqn",
                "parameterNames",
                "methodNames",
                "classFqn",
                "newClassFqn",
            ] {
                string_items(
                    source,
                    &format!("{path}.condition[{i}].{key}"),
                    &condition[key],
                    &mut out,
                );
            }
        }
        object(
            source,
            &format!("{path}.options"),
            &completion["options"],
            &[("strings", Array), ("dataFile", String), ("prefix", String)],
            &mut out,
        );
        string_items(
            source,
            &format!("{path}.options.strings"),
            &completion["options"]["strings"],
            &mut out,
        );
    }
    for (i, item) in items(&value["view"]["paths"]).enumerate() {
        required(
            source,
            &format!("view.paths[{i}]"),
            item,
            &["path"],
            &mut out,
        );
    }
    for kind in ["directives", "ifDirectives", "scriptDirectives"] {
        for (i, item) in items(&value["blade"][kind]).enumerate() {
            required(
                source,
                &format!("blade.{kind}[{i}]"),
                item,
                if kind == "ifDirectives" {
                    &["name", "prefix", "suffix"]
                } else {
                    &["name"]
                },
                &mut out,
            );
        }
    }
    for (i, item) in items(&value["helperCode"]["methodsWithConstructorParameters"]).enumerate() {
        required(
            source,
            &format!("helperCode.methodsWithConstructorParameters[{i}]"),
            item,
            &["baseFqn", "methods"],
            &mut out,
        );
    }
    for (i, item) in items(&value["database"]["columnMigrationMethods"]).enumerate() {
        let path = format!("database.columnMigrationMethods[{i}]");
        required(source, &path, item, &["name"], &mut out);
        let columns = if item.get("columns").is_some() {
            items(&item["columns"]).collect::<Vec<_>>()
        } else {
            vec![item]
        };
        for column in columns {
            required(source, &path, column, &["type"], &mut out);
            if column["type"].as_str().is_some_and(|kind| {
                !matches!(
                    kind,
                    "string" | "integer" | "float" | "boolean" | "array" | "timestamp" | "other"
                )
            }) {
                out.push(format!("{source}: unsupported column type in {path}"));
            }
        }
        if item.get("columns").is_none() {
            required(source, &path, item, &["defaultName"], &mut out);
            if item.get("nameParameterIndex").is_none() && item.get("columnNameTemplate").is_none()
            {
                out.push(format!(
                    "{source}: {path} requires nameParameterIndex or columnNameTemplate"
                ));
            }
        }
        if item
            .get("nameParameterIndex")
            .is_some_and(|value| value.as_u64().is_none_or(|index| index == 0))
        {
            out.push(format!(
                "{source}: invalid one-based nameParameterIndex in {path}"
            ));
        }
    }
    for (i, completion) in items(&value["completions"]).enumerate() {
        required(
            source,
            &format!("completions[{i}]"),
            completion,
            &["complete", "condition"],
            &mut out,
        );
        for (j, condition) in items(&completion["condition"]).enumerate() {
            for (k, parameter) in items(&condition["parameters"]).enumerate() {
                if parameter.as_u64().is_none_or(|n| n == 0 || n >= 256) {
                    out.push(format!("{source}: invalid one-based completions[{i}].condition[{j}].parameters[{k}]"));
                }
            }
        }
    }

    out
}
