//! Declaration-only OpenAPI export with positive bounded written DTO shapes.
use serde_json::{Value, json};
use ui_kit::routes::Route;

#[cfg(test)]
pub fn export(routes: &[Route]) -> String {
    export_with_sources(routes, |_| None)
}

#[cfg(test)]
pub fn export_with_sources(
    routes: &[Route],
    source: impl FnMut(&Route) -> Option<String>,
) -> String {
    export_with_registry(routes, source, None)
}

pub fn export_with_registry(
    routes: &[Route],
    mut source: impl FnMut(&Route) -> Option<String>,
    registry: Option<&crate::dto_schemas::Sources>,
) -> String {
    let mut schemas = registry.map(|s| s.schemas());
    let mut paths = serde_json::Map::new();
    let mut unsupported = Vec::new();
    for route in routes {
        let method = route.method.to_ascii_lowercase();
        if ![
            "get", "post", "put", "delete", "patch", "head", "options", "trace",
        ]
        .contains(&method.as_str())
            || !crate::safe_request_path(&route.path)
            || route.path.contains(['*', '?'])
        {
            unsupported.push(json!({"method":route.method,"path":route.path,"reason":"ant pattern (`*`, `**`, `?`) is not exported"}));
            continue;
        }
        let Some((template, variables)) = path_template(&route.path) else {
            unsupported.push(json!({"method":route.method,"path":route.path}));
            continue;
        };
        let parameters = variables
            .into_iter()
            .map(|(name, pattern)| {
                let mut schema = json!({"type":"string"});
                if let Some(pattern) = pattern {
                    schema["pattern"] = json!(pattern);
                }
                json!({"name":name,"in":"path","required":true,"schema":schema})
            })
            .collect::<Vec<_>>();
        let handler = route.handler.member.as_ref().map_or_else(
            || route.handler.class.clone(),
            |m| format!("{}@{m}", route.handler.class),
        );
        let path = paths.entry(template).or_insert_with(|| json!({}));
        let operation = path.as_object_mut().unwrap().entry(method).or_insert_with(|| json!({
            "responses":{"default":{"description":"Response schema is not inferred from source mappings."}},
            "parameters":parameters,
            "x-spring-handlers":[]
        }));
        let handlers = operation["x-spring-handlers"].as_array_mut().unwrap();
        if !handlers.contains(&Value::String(handler.clone())) {
            handlers.push(Value::String(handler));
        }
        if let Some(declared) = source(route)
            .and_then(|text| declared_operation_with_schemas(route, &text, schemas.as_mut()))
        {
            operation
                .as_object_mut()
                .unwrap()
                .entry("x-spring-source-operations")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .unwrap()
                .push(declared);
        }
    }
    for path in paths.values_mut() {
        for op in path.as_object_mut().unwrap().values_mut() {
            if op["x-spring-handlers"]
                .as_array()
                .is_some_and(|a| a.len() == 1)
            {
                if let Some(declared) = op["x-spring-source-operations"]
                    .as_array()
                    .filter(|a| a.len() == 1)
                    .and_then(|a| a.first())
                    .cloned()
                {
                    for (key, value) in declared.as_object().unwrap() {
                        if key == "parameters" {
                            op["parameters"]
                                .as_array_mut()
                                .unwrap()
                                .extend(value.as_array().unwrap().iter().cloned());
                        } else {
                            op.as_object_mut()
                                .unwrap()
                                .insert(key.clone(), value.clone());
                        }
                    }
                }
            }
        }
    }
    let mut result = json!({
        "openapi":"3.0.3",
        "info":{"title":"Spring source mappings","version":"1.0.0"},
        "paths":paths,
        "x-openidea-unexported-mappings":unsupported,
        "x-openidea-scope":"Source declarations only; positive written DTO shapes do not establish serializer configuration, validation or runtime conditions."
    });
    if let Some(schemas) = schemas {
        if !schemas.components.is_empty() {
            result["components"] = json!({"schemas":schemas.components})
        }
    }
    serde_json::to_string_pretty(&result).unwrap() + "\n"
}

pub(crate) fn declared_operation(route: &Route, text: &str) -> Option<Value> {
    declared_operation_with_schemas(route, text, None)
}
fn declared_operation_with_schemas(
    route: &Route,
    text: &str,
    mut schemas: Option<&mut crate::dto_schemas::Schemas<'_>>,
) -> Option<Value> {
    let location = route.location.as_ref()?;
    let file = spring_text::parse_source(&location.path, text);
    for class in &file.types {
        for method in class.methods() {
            if spring_text::range(text, method.at, method.at + method.name.len()) != location.range
                || route.handler.member.as_deref() != Some(method.name.as_str())
                || route.handler.class != class.name
            {
                continue;
            }
            let mapping = method
                .annotations
                .iter()
                .find(|a| crate::MAPPINGS.contains(&a.name.as_str()))?;
            let class_mapping = class.annotation("RequestMapping");
            let media = |key: &str| {
                mapping
                    .arg(key)
                    .or_else(|| class_mapping.and_then(|a| a.arg(key)))
                    .map(|a| {
                        a.strings
                            .iter()
                            .map(|s| s.value.clone())
                            .filter(|s| {
                                !s.contains(['$', '#']) && !s.starts_with('!') && s.contains('/')
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            };
            let consumes = media("consumes");
            let produces = media("produces");
            let mut parameters = Vec::new();
            let mut body = None;
            let mut body_shape = None;
            for param in &method.params {
                if let Some(annotation) = param
                    .annotation("RequestParam")
                    .or_else(|| param.annotation("RequestHeader"))
                {
                    if !file.from_spring(&annotation.name) {
                        continue;
                    }
                    if annotation
                        .arg("required")
                        .is_some_and(|a| !matches!(a.text.trim(), "true" | "false"))
                    {
                        continue;
                    }
                    let explicit = annotation.arg("name").or_else(|| annotation.arg("value"));
                    if explicit.is_none()
                        && matches!(param.ty.name.as_str(), "Map" | "MultiValueMap")
                    {
                        continue;
                    }
                    if annotation
                        .arg("defaultValue")
                        .is_some_and(|a| a.strings.len() != 1)
                    {
                        continue;
                    }
                    let name = if let Some(arg) = explicit {
                        let [value] = arg.strings.as_slice() else {
                            continue;
                        };
                        value.value.clone()
                    } else {
                        param.name.clone()
                    };
                    if name.is_empty() || name.contains(['$', '#']) {
                        continue;
                    }
                    let required = annotation
                        .arg("required")
                        .is_none_or(|a| a.text.trim() != "false")
                        && annotation.arg("defaultValue").is_none()
                        && param.ty.name != "Optional"
                        && !param.ty.text.trim().ends_with('?');
                    parameters.push(json!({"name":name,"in":if annotation.name=="RequestHeader"{"header"}else{"query"},"required":required,"schema":scalar_schema(&param.ty)}));
                }
                if let Some(annotation) = param.annotation("RequestBody") {
                    if !file.from_spring("RequestBody")
                        || annotation
                            .arg("required")
                            .is_some_and(|a| !matches!(a.text.trim(), "true" | "false"))
                    {
                        continue;
                    }
                    let required = annotation
                        .arg("required")
                        .is_none_or(|a| a.text.trim() != "false")
                        && param.ty.name != "Optional"
                        && !param.ty.text.trim().ends_with('?');
                    body_shape = schemas.as_mut().and_then(|s| s.schema(&param.ty, &file));
                    let mut content = serde_json::Map::new();
                    for media in &consumes {
                        content.insert(
                            media.clone(),
                            json!({"schema":body_shape.clone().unwrap_or(json!({})),"x-java-type":param.ty.text}),
                        );
                    }
                    if !content.is_empty() {
                        body = Some(json!({"required":required,"content":content}));
                    }
                }
            }
            let mut data = json!({"parameters":parameters});
            if let Some(body) = body {
                data["requestBody"] = body;
            }
            if let Some(shape) = body_shape {
                data["x-spring-source-request-schema"] = shape;
            }
            let response_shape = method
                .ty
                .as_ref()
                .filter(|_| {
                    class.has(&["RestController", "ResponseBody"])
                        || method.has(&["ResponseBody"])
                        || method.ty.as_ref().is_some_and(|ty| {
                            ty.name == "ResponseEntity" && file.from_spring("ResponseEntity")
                        })
                })
                .and_then(|ty| {
                    let ty = if ty.name == "ResponseEntity"
                        && ty.args.len() == 1
                        && file.from_spring("ResponseEntity")
                    {
                        &ty.args[0]
                    } else {
                        ty
                    };
                    schemas.as_mut().and_then(|s| s.schema(ty, &file))
                });
            if let Some(shape) = &response_shape {
                data["x-spring-source-response-schema"] = shape.clone();
            }
            if !produces.is_empty() {
                let mut content = serde_json::Map::new();
                for media in produces {
                    content.insert(
                        media,
                        response_shape
                            .clone()
                            .map_or(json!({}), |s| json!({"schema":s})),
                    );
                }
                data["responses"] = json!({"default":{"description":"Declared response media types and positive written source shape when available.","content":content}});
            }
            if let Some(code) = declared_status(method, &file, text) {
                let mut response = json!({"description":"Declared HTTP status; body schema only as written in source."});
                if let Some(content) = data["responses"]["default"].get("content") {
                    response["content"] = content.clone();
                }
                data["responses"] = json!({code.to_string():response});
            }
            let mut conditions = serde_json::Map::new();
            for key in ["params", "headers"] {
                let values = class_mapping
                    .into_iter()
                    .chain(std::iter::once(mapping))
                    .filter_map(|a| a.arg(key))
                    .flat_map(|arg| arg.strings.iter().map(|s| &s.value))
                    .collect::<Vec<_>>();
                if !values.is_empty() {
                    conditions.insert(key.into(), json!(values));
                }
            }
            if !conditions.is_empty() {
                data["x-spring-request-conditions"] = Value::Object(conditions);
            }
            return Some(data);
        }
    }
    None
}
/// Strips `{name:regex}` to `{name}`; `None` for malformed templates or `{*name}` captures.
fn path_template(path: &str) -> Option<(String, Vec<(String, Option<String>)>)> {
    let mut out = String::new();
    let mut variables: Vec<(String, Option<String>)> = Vec::new();
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        match c {
            '}' | ':' => return None,
            '{' => {
                let mut inner = String::new();
                let mut depth = 0usize;
                loop {
                    match chars.next()? {
                        '{' => {
                            depth += 1;
                            inner.push('{');
                        }
                        '}' if depth == 0 => break,
                        '}' => {
                            depth -= 1;
                            inner.push('}');
                        }
                        c => inner.push(c),
                    }
                }
                let (name, pattern) = match inner.split_once(':') {
                    Some((name, regex)) if !regex.is_empty() => (name, Some(regex.to_owned())),
                    Some(_) => return None,
                    None => (inner.as_str(), None),
                };
                if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    return None;
                }
                if !variables.iter().any(|(n, _)| n == name) {
                    variables.push((name.to_owned(), pattern));
                }
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
            c => out.push(c),
        }
    }
    Some((out, variables))
}

fn http_status(name: &str) -> Option<u16> {
    Some(match name {
        "OK" => 200,
        "CREATED" => 201,
        "ACCEPTED" => 202,
        "NO_CONTENT" => 204,
        "RESET_CONTENT" => 205,
        "PARTIAL_CONTENT" => 206,
        "MOVED_PERMANENTLY" => 301,
        "FOUND" => 302,
        "SEE_OTHER" => 303,
        "NOT_MODIFIED" => 304,
        "BAD_REQUEST" => 400,
        "UNAUTHORIZED" => 401,
        "FORBIDDEN" => 403,
        "NOT_FOUND" => 404,
        "CONFLICT" => 409,
        "GONE" => 410,
        "UNPROCESSABLE_ENTITY" => 422,
        "INTERNAL_SERVER_ERROR" => 500,
        "NOT_IMPLEMENTED" => 501,
        "SERVICE_UNAVAILABLE" => 503,
        _ => return None,
    })
}

fn declared_status(
    method: &spring_text::java::Member,
    file: &spring_text::java::JavaFile,
    text: &str,
) -> Option<u16> {
    if let Some(a) = method.annotation("ResponseStatus") {
        if !file.from_spring("ResponseStatus") {
            return None;
        }
        let arg = a.arg("code").or_else(|| a.arg("value"))?;
        return http_status(arg.text.trim().strip_prefix("HttpStatus.")?);
    }
    if method.ty.as_ref()?.name != "ResponseEntity" || !file.from_spring("ResponseEntity") {
        return None;
    }
    let (from, to) = method.body?;
    let body = text.get(from..to)?;
    let tokens = spring_text::java::tokens(body);
    let returns = tokens.iter().filter(|t| t.s == "return").count();
    let mut codes = std::collections::BTreeSet::new();
    let mut hits = 0;
    for (i, t) in tokens.iter().enumerate() {
        if t.s == "new" && tokens.get(i + 1).is_some_and(|n| n.s == "ResponseEntity") {
            return None;
        }
        if t.s != "ResponseEntity" || tokens.get(i + 1).is_none_or(|d| d.s != ".") {
            continue;
        }
        let name = tokens.get(i + 2)?.s;
        let code = match name {
            "ok" => 200,
            "created" => 201,
            "accepted" => 202,
            "noContent" => 204,
            "badRequest" => 400,
            "notFound" => 404,
            "unprocessableEntity" => 422,
            "internalServerError" => 500,
            "status" => {
                let args = tokens.get(i + 3..i + 7)?;
                if args[0].s != "(" || args[1].s != "HttpStatus" || args[2].s != "." {
                    return None;
                }
                http_status(args[3].s)?
            }
            _ => return None,
        };
        codes.insert(code);
        hits += 1;
    }
    match (codes.len(), hits == returns) {
        (1, true) => codes.into_iter().next(),
        _ => None,
    }
}

fn scalar_schema(ty: &spring_text::java::TypeRef) -> Value {
    if ty.name == "Optional" && ty.args.len() == 1 {
        return scalar_schema(&ty.args[0]);
    }
    let mut value = match ty.name.as_str() {
        "int" | "Int" | "Integer" | "long" | "Long" | "short" | "Short" | "byte" | "Byte" => {
            json!({"type":"integer"})
        }
        "float" | "Float" | "double" | "Double" => json!({"type":"number"}),
        "boolean" | "Boolean" => json!({"type":"boolean"}),
        "String" | "Char" | "char" | "Character" => json!({"type":"string"}),
        _ => json!({"x-java-type":ty.text}),
    };
    if ty.array {
        value = json!({"type":"array","items":value});
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_kit::routes::Handler;
    fn route(method: &str, path: &str, member: &str) -> Route {
        Route {
            method: method.into(),
            path: path.into(),
            handler: Handler {
                class: "Users".into(),
                member: Some(member.into()),
            },
            name: None,
            location: None,
        }
    }
    #[test]
    fn exports_path_parameters_and_retains_overloaded_handlers_without_guessing_schemas() {
        let data: Value = serde_json::from_str(&export(&[
            route("GET", "/users/{id}", "one"),
            route("GET", "/users/{id}", "two"),
        ]))
        .unwrap();
        let op = &data["paths"]["/users/{id}"]["get"];
        assert_eq!(op["parameters"][0]["name"], "id");
        assert_eq!(op["parameters"][0]["required"], true);
        assert_eq!(op["x-spring-handlers"], json!(["Users@one", "Users@two"]));
        assert!(op.get("requestBody").is_none());
    }
    #[test]
    fn repeated_parameters_are_unique_and_unbalanced_templates_are_not_exported() {
        let data: Value = serde_json::from_str(&export(&[
            route("GET", "/{id}/{id}", "same"),
            route("GET", "/bad}/{id}", "invalid"),
        ]))
        .unwrap();
        assert_eq!(
            data["paths"]["/{id}/{id}"]["get"]["parameters"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(data["paths"].get("/bad}/{id}").is_none());
        assert_eq!(
            data["x-openidea-unexported-mappings"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn source_metadata_exports_declared_bindings_media_and_conditions_without_dto_guessing() {
        let source = "@RestController\n@RequestMapping(headers=\"X-App=web\")\nclass Users { @PostMapping(value=\"/users\",consumes=\"application/json\",produces=\"application/json\",params=\"mode=fast\") void save(@RequestParam(name=\"limit\",required=false) int limit,@RequestHeader(\"X-Trace\") String trace,@RequestBody UserDto body) {} }";
        let file = spring_text::parse_source("/project/Users.java", source);
        let method = file.types[0].methods().next().unwrap();
        let mut r = route("POST", "/users", "save");
        r.location = Some(openidea_plugin::Location {
            path: "/project/Users.java".into(),
            range: spring_text::range(source, method.at, method.at + method.name.len()),
        });
        let data: Value =
            serde_json::from_str(&export_with_sources(&[r], |_| Some(source.into()))).unwrap();
        let op = &data["paths"]["/users"]["post"];
        assert_eq!(op["parameters"][0]["name"], "limit");
        assert_eq!(op["parameters"][0]["required"], false);
        assert_eq!(op["parameters"][0]["schema"]["type"], "integer");
        assert_eq!(op["parameters"][1]["in"], "header");
        assert_eq!(
            op["requestBody"]["content"]["application/json"]["schema"],
            json!({})
        );
        assert_eq!(
            op["requestBody"]["content"]["application/json"]["x-java-type"],
            "UserDto"
        );
        assert_eq!(
            op["x-spring-request-conditions"]["headers"],
            json!(["X-App=web"])
        );
        assert_eq!(
            op["x-spring-request-conditions"]["params"],
            json!(["mode=fast"])
        );
    }
    #[test]
    fn implicit_query_maps_are_not_single_named_parameters_and_optional_is_not_required() {
        let source = "@RestController class Users { @GetMapping(\"/users\") void list(@RequestParam Optional<Integer> limit,@RequestParam Map<String,String> all){} }";
        let file = spring_text::parse_source("/project/Users.java", source);
        let method = file.types[0].methods().next().unwrap();
        let mut r = route("GET", "/users", "list");
        r.location = Some(openidea_plugin::Location {
            path: "/project/Users.java".into(),
            range: spring_text::range(source, method.at, method.at + method.name.len()),
        });
        let data: Value =
            serde_json::from_str(&export_with_sources(&[r], |_| Some(source.into()))).unwrap();
        let parameters = data["paths"]["/users"]["get"]["parameters"]
            .as_array()
            .unwrap();
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0]["required"], false);
        assert_eq!(parameters[0]["schema"]["type"], "integer");
    }
    #[test]
    fn preserves_unsupported_predicates_as_explicit_unexported_inventory() {
        let data: Value = serde_json::from_str(&export(&[
            route("ANY", "/any", "all"),
            route("GET", "/users/{id:[0-9]+}", "numeric"),
            route("GET", "/files/**", "files"),
        ]))
        .unwrap();
        assert_eq!(data["paths"].as_object().unwrap().len(), 1);
        assert_eq!(
            data["paths"]["/users/{id}"]["get"]["parameters"][0]["schema"]["pattern"],
            "[0-9]+"
        );
        assert_eq!(
            data["x-openidea-unexported-mappings"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    fn exported(source: &str, path: &str, name: &str) -> Value {
        let file = spring_text::parse_source("/project/Users.java", source);
        let method = file.types[0].methods().find(|m| m.name == name).unwrap();
        let mut r = route("GET", path, name);
        r.location = Some(openidea_plugin::Location {
            path: "/project/Users.java".into(),
            range: spring_text::range(source, method.at, method.at + method.name.len()),
        });
        serde_json::from_str(&export_with_sources(&[r], |_| Some(source.into()))).unwrap()
    }

    #[test]
    fn regex_path_variables_become_patterns_and_ant_patterns_are_listed_unexported() {
        let source = "@RestController class Users { @GetMapping(\"/u\") void one() {} }";
        let data = exported(source, "/u/{id:[0-9]{2,}}/x/{name}", "one");
        let op = &data["paths"]["/u/{id}/x/{name}"]["get"];
        assert_eq!(op["parameters"][0]["schema"]["pattern"], "[0-9]{2,}");
        assert!(op["parameters"][1]["schema"].get("pattern").is_none());
        let skipped = export(&[route("GET", "/files/**", "a"), route("GET", "/t?st", "b")]);
        let skipped: Value = serde_json::from_str(&skipped).unwrap();
        assert_eq!(skipped["x-openidea-unexported-mappings"].as_array().unwrap().len(), 2);
        assert_eq!(skipped["paths"], json!({}));
    }

    #[test]
    fn literal_response_status_replaces_default_only_when_provable() {
        let source = "import org.springframework.web.bind.annotation.*; import org.springframework.http.*; @RestController class Users {
            @ResponseStatus(HttpStatus.CREATED) @PostMapping(\"/a\") void a() {}
            @GetMapping(\"/b\") ResponseEntity<String> b() { return ResponseEntity.noContent().build(); }
            @GetMapping(\"/c\") ResponseEntity<String> c() { if (x) { return ResponseEntity.ok(\"\"); } return ResponseEntity.notFound().build(); }
        }";
        let a = exported(source, "/a", "a");
        assert!(a["paths"]["/a"]["get"]["responses"].get("201").is_some());
        let b = exported(source, "/b", "b");
        assert!(b["paths"]["/b"]["get"]["responses"].get("204").is_some());
        let c = exported(source, "/c", "c");
        assert!(c["paths"]["/c"]["get"]["responses"].get("default").is_some());
    }
}
