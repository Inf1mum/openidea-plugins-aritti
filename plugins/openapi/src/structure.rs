//! Bounded OpenAPI JSON structure checks. Generic JSON syntax and complete
//! JSON Schema/dialect validation remain the JSON language server's concern.
use crate::spec;
use openidea_plugin::{Diagnostic, Severity};
use serde_json::Value;
const MAX_BYTES: usize = 512 * 1024;
const MAX_NODES: usize = 8192;
const MAX_DEPTH: usize = 48;
const MAX_RESULTS: usize = 256;
#[derive(Clone, Copy, PartialEq)]
enum Version {
    Swagger,
    V30,
    V31,
}

fn bounded(value: &Value, depth: usize, nodes: &mut usize) -> bool {
    *nodes += 1;
    if depth > MAX_DEPTH || *nodes > MAX_NODES {
        return false;
    }
    match value {
        Value::Object(map) => map.values().all(|v| bounded(v, depth + 1, nodes)),
        Value::Array(array) => array.iter().all(|v| bounded(v, depth + 1, nodes)),
        _ => true,
    }
}
fn version(root: &Value) -> Option<Version> {
    if root.get("openapi").is_some() && root.get("swagger").is_some() {
        return None;
    }
    if root["swagger"] == "2.0" {
        return Some(Version::Swagger);
    }
    let value = root["openapi"].as_str()?;
    let (prefix, patch) = value.rsplit_once('.')?;
    if patch.is_empty() || !patch.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    match prefix {
        "3.0" => Some(Version::V30),
        "3.1" => Some(Version::V31),
        _ => None,
    }
}
struct Check<'a> {
    text: &'a str,
    locations: Locations,
    version: Version,
    schema_dialect: bool,
    found: Vec<Diagnostic>,
}
struct Locations {
    spans:std::collections::BTreeMap<String,Option<(usize,usize)>>,
    blocked:std::collections::BTreeSet<String>,
}
impl Locations {
    fn get(&self,path:&str)->Option<(usize,usize)> {
        for end in path.match_indices(['.','[']).map(|(at,_)|at).chain([path.len()]) {
            if self.blocked.contains(&path[..end]) {return None;}
        }
        self.spans.get(path).copied().flatten()
    }
}
fn locations(root:&Value,text:&str,keys:&[spec::Key])->Option<Locations> {
    const MAX_PATH_BYTES:usize=4<<20;
    if keys.iter().map(|key|key.key.len()).sum::<usize>()>MAX_PATH_BYTES {return None;}
    let mut source:std::collections::BTreeMap<&str,Vec<&spec::Key>>=std::collections::BTreeMap::new();
    for key in keys {source.entry(&key.key).or_default().push(key);}
    fn source_span(text:&str,key:&spec::Key)->Option<(usize,usize)> {
        if key.at<key.end && text.get(key.at..key.end).is_some() {Some((key.at,key.end))}
        else if key.at==key.end && key.at>0 && text.get(key.at-1..key.end+1)==Some("\"\"") {Some((key.at-1,key.end+1))} else {None}
    }
    fn key(source:&std::collections::BTreeMap<&str,Vec<&spec::Key>>,text:&str,path:&str,name:&str)->Option<(usize,usize)> {
        let mut matches=source.get(path)?.iter().copied().filter(|key|source_span(text,key).is_some()&&text.get(key.at..key.end).is_some_and(|raw|crate::json::unescape(raw)==name));
        let first=matches.next()?;if matches.next().is_some() {None} else {source_span(text,first)}
    }
    fn walk(value:&Value,path:&str,span:(usize,usize),source:&std::collections::BTreeMap<&str,Vec<&spec::Key>>,text:&str,found:&mut Locations,bytes:&mut usize)->Option<()> {
        *bytes=bytes.checked_add(path.len())?;if *bytes>MAX_PATH_BYTES {return None;}
        found.spans.entry(path.into()).and_modify(|span|*span=None).or_insert(Some(span));
        match value {
            Value::Object(map)=>for (name,value) in map {
                let next=if path.is_empty() {name.clone()} else {format!("{path}.{name}")};
                if let Some(span)=key(source,text,&next,name) {walk(value,&next,span,source,text,found,bytes)?;}
                else {found.blocked.insert(next);}
            },
            Value::Array(items)=>for (index,value) in items.iter().enumerate() {
                let next=format!("{path}[{index}]");
                let primitive=if !value.is_array()&&!value.is_object() {
                    source.get(next.as_str()).and_then(|keys| {
                        let mut candidates=keys.iter().filter(|key|source_span(text,key).is_some()&&key.value_at==key.at&&key.value_end==key.end&&text.get(key.at..key.end).is_some_and(|raw| {
                            value.as_str().map(|value|crate::json::unescape(raw)==value).unwrap_or_else(||serde_json::from_str::<Value>(raw).ok().as_ref()==Some(value))
                        }));
                        let key=candidates.next()?;if candidates.next().is_some() {None} else {source_span(text,key)}
                    })
                } else {Some(span)};
                if let Some(span)=primitive {walk(value,&next,span,source,text,found,bytes)?;} else {found.blocked.insert(next);}
            },
            _=>{},
        }
        Some(())
    }
    let version=if root.get("swagger").is_some() {"swagger"} else {"openapi"};
    let version=key(&source,text,version,version)?;
    let mut found=Locations {spans:Default::default(),blocked:Default::default()};
    walk(root,"",version,&source,text,&mut found,&mut 0)?;Some(found)
}
impl Check<'_> {
    fn issue(&mut self, path: &str, code: &str, message: &str) {
        if self.found.len() >= MAX_RESULTS {
            return;
        }
        let Some((at,end))=self.locations.get(path) else {
            return;
        };
        let mut diagnostic = Diagnostic::new(
            spring_text::range(self.text, at, end),
            Severity::Error,
            message,
        );
        diagnostic.code = Some(format!("openapi-structure-{code}"));
        self.found.push(diagnostic);
    }
    fn object(&mut self, value: &Value, path: &str) -> bool {
        if value.is_object() {
            true
        } else {
            self.issue(path, "type", "Expected an OpenAPI object.");
            false
        }
    }
    fn field(&mut self, value: &Value, path: &str, key: &str, kind: &str, required: bool) {
        let target = if path.is_empty() {
            key.to_owned()
        } else {
            format!("{path}.{key}")
        };
        let Some(member) = value.get(key) else {
            if required {
                self.issue(
                    path,
                    "required",
                    &format!("Missing required OpenAPI field `{key}`."),
                );
            }
            return;
        };
        let valid = match kind {
            "object" => member.is_object(),
            "array" => member.is_array(),
            "boolean" => member.is_boolean(),
            _ => member.is_string(),
        };
        if !valid {
            self.issue(
                &target,
                "type",
                &format!("OpenAPI field `{key}` must be {kind}."),
            );
        }
    }
    fn reference(&mut self, value: &Value, path: &str) -> bool {
        if value.get("$ref").is_none() {
            return false;
        }
        self.field(value, path, "$ref", "string", false);
        true
    }
    fn schema(&mut self, value: &Value, path: &str, allow_boolean: bool) {
        if !self.schema_dialect
            || (self.version == Version::V31
                && value
                    .get("$schema")
                    .is_some_and(|value| !known_dialect(value)))
        {
            return;
        }
        if value.is_boolean() && (self.version == Version::V31 || allow_boolean) {
            return;
        }
        if !self.object(value, path) {
            return;
        }
        // Schema references can carry dialect/scope semantics; do not traverse
        // their siblings or resolve targets here.
        if self.reference(value, path) {
            return;
        }
        if let Some(ty) = value.get("type") {
            let legal = |v: &Value| {
                v.as_str().is_some_and(|s| {
                    ["array", "boolean", "integer", "number", "object", "string"].contains(&s)
                        || (self.version == Version::V31 && s == "null")
                })
            };
            let valid = legal(ty)
                || (self.version == Version::V31
                    && ty
                        .as_array()
                        .is_some_and(|types| !types.is_empty() && types.iter().all(legal)));
            if !valid {
                self.issue(
                    &format!("{path}.type"),
                    "schema-type",
                    "Schema type is not valid for this OpenAPI version.",
                );
            }
        }
        if self.version != Version::V31 && value["type"] == "array" && value.get("items").is_none()
        {
            self.issue(
                path,
                "required",
                "Array schemas require `items` in Swagger 2 / OpenAPI 3.0.",
            );
        }
        for key in ["title", "description", "format", "pattern"] {
            self.field(value, path, key, "string", false);
        }
        for key in if self.version == Version::Swagger {
            &["readOnly"][..]
        } else {
            &["readOnly", "writeOnly", "deprecated"][..]
        } {
            self.field(value, path, key, "boolean", false);
        }
        if self.version == Version::V30 {
            self.field(value, path, "nullable", "boolean", false);
        }
        for key in [
            "minimum",
            "maximum",
            "multipleOf",
            "exclusiveMinimum",
            "exclusiveMaximum",
        ] {
            if let Some(bound) = value.get(key) {
                let exclusive = key.starts_with("exclusive") && self.version != Version::V31;
                if (exclusive && !bound.is_boolean()) || (!exclusive && !bound.is_number()) {
                    self.issue(
                        &format!("{path}.{key}"),
                        "schema-bound",
                        "Schema bound has the wrong type for this OpenAPI version.",
                    );
                } else if key == "multipleOf" && bound.as_f64().is_some_and(|n| n <= 0.0) {
                    self.issue(
                        &format!("{path}.{key}"),
                        "schema-bound",
                        "Schema multipleOf must be positive.",
                    );
                }
            }
        }
        for key in [
            "minLength",
            "maxLength",
            "minItems",
            "maxItems",
            "minProperties",
            "maxProperties",
        ] {
            if value
                .get(key)
                .is_some_and(|v| !v.as_f64().is_some_and(|n| n >= 0.0 && n.fract() == 0.0))
            {
                self.issue(
                    &format!("{path}.{key}"),
                    "schema-bound",
                    "Schema size bounds must be nonnegative integers.",
                );
            }
        }
        if let Some(required) = value.get("required") {
            let valid = required.as_array().is_some_and(|a| {
                (self.version == Version::V31 || !a.is_empty())
                    && a.iter().all(Value::is_string)
                    && {
                        let mut seen = std::collections::HashSet::new();
                        a.iter().all(|v| seen.insert(v.as_str()))
                    }
            });
            if !valid {
                self.issue(&format!("{path}.required"), "schema-required", "Schema required must contain distinct property-name strings (nonempty before OpenAPI 3.1).");
            }
        }
        if let Some(values) = value.get("enum") {
            if !values.is_array()
                || (self.version != Version::V31 && values.as_array().is_some_and(Vec::is_empty))
            {
                self.issue(
                    &format!("{path}.enum"),
                    "schema-enum",
                    "Schema enum must be an array (nonempty before OpenAPI 3.1).",
                );
            }
        }
        if let Some(properties) = value.get("properties") {
            if self.object(properties, &format!("{path}.properties")) {
                for (name, schema) in properties.as_object().unwrap() {
                    self.schema(schema, &format!("{path}.properties.{name}"), false);
                }
            }
        }
        for key in ["items", "additionalProperties", "not"] {
            if let Some(schema) = value.get(key) {
                self.schema(
                    schema,
                    &format!("{path}.{key}"),
                    key == "additionalProperties",
                );
            }
        }
        for key in ["allOf", "anyOf", "oneOf"] {
            if let Some(schemas) = value.get(key) {
                if let Some(schemas) = schemas.as_array() {
                    for (i, schema) in schemas.iter().enumerate() {
                        self.schema(schema, &format!("{path}.{key}[{i}]"), false);
                    }
                } else {
                    self.issue(
                        &format!("{path}.{key}"),
                        "type",
                        "Schema composition must be an array.",
                    );
                }
            }
        }
    }
    fn content(&mut self, value: &Value, path: &str) {
        if !self.object(value, path) {
            return;
        }
        for (media, item) in value.as_object().unwrap() {
            let target = format!("{path}.{media}");
            if !self.object(item, &target) {
                continue;
            }
            if let Some(schema) = item.get("schema") {
                self.schema(schema, &format!("{target}.schema"), false);
            }
        }
    }
    fn response(&mut self, value: &Value, path: &str) {
        if !self.object(value, path) || self.reference(value, path) {
            return;
        }
        self.field(value, path, "description", "string", true);
        if self.version == Version::Swagger {
            if let Some(schema) = value.get("schema") {
                self.schema(schema, &format!("{path}.schema"), false);
            }
        } else if let Some(content) = value.get("content") {
            self.content(content, &format!("{path}.content"));
        }
        if let Some(headers) = value.get("headers") {
            if self.object(headers, &format!("{path}.headers")) {
                for (name, header) in headers.as_object().unwrap() {
                    self.header(header, &format!("{path}.headers.{name}"));
                }
            }
        }
    }
    fn header(&mut self, value: &Value, path: &str) {
        if !self.object(value, path)
            || (self.version != Version::Swagger && self.reference(value, path))
        {
            return;
        }
        if self.version == Version::Swagger {
            self.field(value, path, "type", "string", true);
            return;
        }
        self.field(value, path, "required", "boolean", false);
        if let Some(schema) = value.get("schema") {
            self.schema(schema, &format!("{path}.schema"), false);
        }
        if let Some(content) = value.get("content") {
            self.content(content, &format!("{path}.content"));
        }
        if value.get("schema").is_some() && value.get("content").is_some() {
            self.issue(
                path,
                "parameter-content",
                "A header cannot contain both schema and content.",
            );
        }
    }
    fn request_body(&mut self, value: &Value, path: &str) {
        if !self.object(value, path) || self.reference(value, path) {
            return;
        }
        self.field(value, path, "content", "object", true);
        self.field(value, path, "required", "boolean", false);
        if let Some(content) = value.get("content") {
            self.content(content, &format!("{path}.content"));
        }
    }
    fn servers(&mut self, value: &Value, path: &str) {
        let Some(servers) = value.as_array() else {
            self.issue(path, "type", "Servers must be an array.");
            return;
        };
        for (i, server) in servers.iter().enumerate() {
            let path = format!("{path}[{i}]");
            if !self.object(server, &path) {
                continue;
            }
            self.field(server, &path, "url", "string", true);
            self.field(server, &path, "description", "string", false);
            if let Some(variables) = server.get("variables") {
                if !self.object(variables, &format!("{path}.variables")) {
                    continue;
                }
                for (name, variable) in variables.as_object().unwrap() {
                    let p = format!("{path}.variables.{name}");
                    if !self.object(variable, &p) {
                        continue;
                    }
                    self.field(variable, &p, "default", "string", true);
                    if let Some(values) = variable.get("enum") {
                        if !values.as_array().is_some_and(|a| {
                            (self.version != Version::V31 || !a.is_empty())
                                && a.iter().all(Value::is_string)
                        }) {
                            self.issue(
                                &format!("{p}.enum"),
                                "server-enum",
                                "Server variable enum must contain nonempty string choices.",
                            );
                        } else if self.version == Version::V31
                            && variable.get("default").is_some_and(|d| {
                                d.is_string() && !values.as_array().unwrap().contains(d)
                            })
                        {
                            self.issue(
                                &p,
                                "server-default",
                                "Server variable default must be one of its enum choices.",
                            );
                        }
                    }
                }
            }
        }
    }
    fn reusable(&mut self, root: &Value) {
        let (object, path) = if self.version == Version::Swagger {
            (root, "")
        } else {
            (&root["components"], "components.")
        };
        let groups = if self.version == Version::Swagger {
            &["responses", "parameters"][..]
        } else {
            &["responses", "parameters", "requestBodies", "headers"][..]
        };
        for group in groups {
            let Some(values) = object.get(group) else {
                continue;
            };
            let p = format!("{path}{group}");
            if !self.object(values, &p) {
                continue;
            }
            for (name, value) in values.as_object().unwrap() {
                if self.version == Version::Swagger && name.starts_with("x-") {
                    continue;
                }
                let p = format!("{p}.{name}");
                match *group {
                    "responses" => self.response(value, &p),
                    "parameters" => self.parameter(value, &p),
                    "requestBodies" => self.request_body(value, &p),
                    "headers" => self.header(value, &p),
                    _ => {}
                }
            }
        }
    }
    fn parameters(&mut self, value: &Value, path: &str) {
        let Some(parameters) = value.as_array() else {
            self.issue(path, "type", "Parameters must be an array.");
            return;
        };
        for (i, parameter) in parameters.iter().enumerate() {
            self.parameter(parameter, &format!("{path}[{i}]"));
        }
    }
    fn parameter(&mut self, parameter: &Value, path: &str) {
        if !self.object(parameter, &path) || self.reference(parameter, &path) {
            return;
        }
        self.field(parameter, &path, "name", "string", true);
        self.field(parameter, &path, "in", "string", true);
        let allowed = if self.version == Version::Swagger {
            &["query", "header", "path", "formData", "body"][..]
        } else {
            &["query", "header", "path", "cookie"][..]
        };
        if parameter["in"]
            .as_str()
            .is_some_and(|location| !allowed.contains(&location))
        {
            self.issue(
                &format!("{path}.in"),
                "parameter-in",
                "Parameter location is not valid for this OpenAPI version.",
            );
        }
        if parameter["in"] == "path" && parameter["required"] != true {
            self.issue(
                &path,
                "path-required",
                "Path parameters must set `required` to true.",
            );
        }
        self.field(parameter, &path, "required", "boolean", false);
        if let Some(schema) = parameter.get("schema") {
            self.schema(schema, &format!("{path}.schema"), false);
        }
        if self.version != Version::Swagger {
            if let Some(content) = parameter.get("content") {
                self.content(content, &format!("{path}.content"));
            }
            if parameter.get("schema").is_some() && parameter.get("content").is_some() {
                self.issue(
                    &path,
                    "parameter-content",
                    "A parameter cannot contain both `schema` and `content`.",
                );
            }
        }
    }

    fn paths(&mut self, value: &Value, path: &str) {
        if !self.object(value, path) {
            return;
        }
        for (name, item) in value.as_object().unwrap() {
            if name.starts_with("x-") {
                continue;
            }
            let path = format!("{path}.{name}");
            if !self.object(item, &path) {
                continue;
            }
            self.field(item, &path, "$ref", "string", false);
            if self.version != Version::Swagger {
                if let Some(servers) = item.get("servers") {
                    self.servers(servers, &format!("{path}.servers"));
                }
            }
            if let Some(parameters) = item.get("parameters") {
                self.parameters(parameters, &format!("{path}.parameters"));
            }
            for method in spec::METHODS {
                if method == "trace" && self.version == Version::Swagger {
                    continue;
                }
                let Some(operation) = item.get(method) else {
                    continue;
                };
                let path = format!("{path}.{method}");
                if !self.object(operation, &path) {
                    continue;
                }
                self.field(operation, &path, "operationId", "string", false);
                if self.version != Version::Swagger {
                    if let Some(servers) = operation.get("servers") {
                        self.servers(servers, &format!("{path}.servers"));
                    }
                }
                self.field(
                    operation,
                    &path,
                    "responses",
                    "object",
                    self.version != Version::V31,
                );
                if let Some(parameters) = operation.get("parameters") {
                    self.parameters(parameters, &format!("{path}.parameters"));
                }
                if let Some(responses) = operation["responses"].as_object() {
                    if responses.is_empty() {
                        self.issue(
                            &format!("{path}.responses"),
                            "responses-empty",
                            "Responses must contain at least one response.",
                        );
                    }
                    for (status, response) in responses {
                        if status.starts_with("x-") {
                            continue;
                        }
                        let target = format!("{path}.responses.{status}");
                        self.response(response, &target);
                    }
                }
                if self.version != Version::Swagger {
                    if let Some(body) = operation.get("requestBody") {
                        let target = format!("{path}.requestBody");
                        self.request_body(body, &target);
                    }
                }
            }
        }
    }
}
fn known_dialect(value: &Value) -> bool {
    value.as_str().is_some_and(|dialect| {
        matches!(
            dialect,
            "https://spec.openapis.org/oas/3.1/dialect/base"
                | "https://json-schema.org/draft/2020-12/schema"
        )
    })
}
pub fn diagnostics(text: &str) -> Vec<Diagnostic> {
    if text.len() > MAX_BYTES || !text.trim_start().starts_with('{') || !spec::maybe_spec(text) {
        return Vec::new();
    }
    let Ok(root) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let Some(version) = version(&root) else {
        return Vec::new();
    };
    if !bounded(&root, 0, &mut 0) {
        return Vec::new();
    }
    let Some(keys)=crate::json::bounded_keys(text,MAX_NODES,4<<20) else {return Vec::new();};
    let Some(locations)=locations(&root,text,&keys) else {return Vec::new();};
    let mut check = Check {
        text,
        locations,
        version,
        schema_dialect: version != Version::V31
            || root.get("jsonSchemaDialect").is_none_or(known_dialect),
        found: Vec::new(),
    };
    check.field(&root, "", "info", "object", true);
    if root["info"].is_object() {
        check.field(&root["info"], "info", "title", "string", true);
        check.field(&root["info"], "info", "version", "string", true);
    }
    if version != Version::V31 {
        check.field(&root, "", "paths", "object", true);
    } else if ["paths", "components", "webhooks"]
        .iter()
        .all(|key| root.get(key).is_none())
    {
        check.issue(
            "",
            "root-content",
            "OpenAPI 3.1 requires at least one of paths, components or webhooks.",
        );
    }
    if let Some(paths) = root.get("paths") {
        check.paths(paths, "paths");
    }
    if version == Version::Swagger {
        if let Some(definitions) = root.get("definitions") {
            if check.object(definitions, "definitions") {
                for (name, schema) in definitions.as_object().unwrap() {
                    check.schema(schema, &format!("definitions.{name}"), false);
                }
            }
        }
    } else {
        check.field(&root, "", "components", "object", false);
        if let Some(schemas) = root["components"].get("schemas") {
            if check.object(schemas, "components.schemas") {
                for (name, schema) in schemas.as_object().unwrap() {
                    check.schema(schema, &format!("components.schemas.{name}"), false);
                }
            }
        }
        if version == Version::V31 {
            if let Some(webhooks) = root.get("webhooks") {
                check.paths(webhooks, "webhooks");
            }
        }
    }
    check.reusable(&root);
    if version != Version::Swagger {
        if let Some(servers) = root.get("servers") {
            check.servers(servers, "servers");
        }
    }
    check.found
}

#[cfg(test)]
mod tests {
    use super::*;
    fn has(text: &str, code: &str) -> bool {
        diagnostics(text)
            .iter()
            .any(|d| d.code.as_deref() == Some(&format!("openapi-structure-{code}")))
    }
    #[test]
    fn duplicate_version_or_component_owners_do_not_establish_structure_context() {
        for text in [
            r#"{"openapi":"3.0.4","openapi":"3.1.1","info":{"title":"A","version":"1"},"components":{"schemas":{"Pet":5}}}"#,
            r#"{"openapi":"3.1.1","info":{"title":"A","version":"1"},"components":{},"components":{"schemas":{"Pet":5}}}"#,
        ] {assert!(diagnostics(text).is_empty(),"{text}");}
    }
    #[test]
    fn proved_dotted_schema_and_base_metadata_keep_actual_utf16_field_errors_without_io() {
        use openidea_plugin::{diagnostics::DiagnosticsProvider,mock::MockHost,providers::Document};
        let host=MockHost::install().with_capabilities(&[]);
        let text=r#"{"openapi":"3.1.1","info":{"title":"😀","version":"1"},"paths":{},"components":{"schemas":{"A.B":{"$id":"relative.json","type":42}}}}"#;
        let doc=Document {path:"/unknown/openapi.json".into(),language:"json".into(),text:text.into(),version:1};
        let found=crate::OpenApi::default().diagnostics(&doc);
        let issue=found.iter().find(|d|d.code.as_deref()==Some("openapi-structure-schema-type")).unwrap();
        let at=openidea_plugin::text::offset(text,issue.range.start).unwrap();let end=openidea_plugin::text::offset(text,issue.range.end).unwrap();
        assert_eq!(&text[at..end],"type");assert_eq!(issue.range.start.character as usize,text[..at].encode_utf16().count());
        assert!(host.requests().is_empty());assert!(host.process_runs().is_empty());assert!(host.opened_locations().is_empty());
    }
    #[test]
    fn empty_property_keys_and_primitive_array_items_have_real_nonempty_source_spans() {
        let text=r#"{"openapi":"3.1.1","info":{"title":"A","version":"1"},"components":{"schemas":{"A":{"properties":{"":42}}}},"servers":[""]}"#;
        let found=diagnostics(text);assert_eq!(found.len(),2);
        for issue in found {
            let at=openidea_plugin::text::offset(text,issue.range.start).unwrap();let end=openidea_plugin::text::offset(text,issue.range.end).unwrap();
            assert_eq!(&text[at..end],"\"\"");assert!(at<end);
        }
    }
    #[test]
    fn ambiguous_branches_do_not_hide_independent_owned_errors_and_path_memory_is_bounded() {
        let text=r#"{"openapi":"3.1.1","info":{},"components":{},"components":{"schemas":{"Pet":5}}}"#;
        let found=diagnostics(text);assert_eq!(found.len(),2);
        assert!(found.iter().all(|d|d.code.as_deref()==Some("openapi-structure-required")));
        let members=(0..3000).map(|n|(format!("n{n}"),Value::Null)).collect::<serde_json::Map<_,_>>();
        let mut root=serde_json::json!({"openapi":"3.1.1","info":{},"components":{}});
        root.as_object_mut().unwrap().insert("x".repeat(2048),Value::Object(members));
        let text=root.to_string();assert!(text.len()<MAX_BYTES);assert!(diagnostics(&text).is_empty());
    }
    #[test]
    fn roots_are_version_specific() {
        assert!(has(
            r#"{"swagger":"2.0","info":{"title":"A","version":"1"}}"#,
            "required"
        ));
        assert!(has(
            r#"{"openapi":"3.0.4","info":{"title":"A","version":"1"}}"#,
            "required"
        ));
        assert!(
            diagnostics(
                r#"{"openapi":"3.1.1","info":{"title":"A","version":"1"},"components":{}}"#
            )
            .is_empty()
        );
        assert!(has(
            r#"{"openapi":"3.1.1","info":{"title":"A","version":"1"}}"#,
            "root-content"
        ));
    }
    #[test]
    fn schemas_respect_version_differences() {
        for version in ["3.0.4", "3.1.1"] {
            let text = format!(
                r#"{{"openapi":"{version}","info":{{"title":"A","version":"1"}},"paths":{{}},"components":{{"schemas":{{"Bool":false,"Union":{{"type":["string","null"]}},"Array":{{"type":"array"}}}}}}}}"#
            );
            if version == "3.0.4" {
                assert!(has(&text, "type"));
                assert!(has(&text, "schema-type"));
                assert!(has(&text, "required"));
            } else {
                assert!(diagnostics(&text).is_empty());
            }
        }
        assert!(diagnostics(r#"{"swagger":"2.0","info":{"title":"A","version":"1"},"paths":{},"definitions":{"Object":{"type":"object","additionalProperties":false}}}"#).is_empty());
    }
    #[test]
    fn operations_responses_parameters_and_request_body_are_located() {
        let text = r#"{"openapi":"3.0.4","info":{"title":"A","version":"1"},"paths":{"/pets":{"get":{"parameters":[{"name":"id","in":"path","required":false}],"responses":{"200":{}},"requestBody":{}}}}}"#;
        assert!(has(text, "path-required"));
        assert!(has(text, "required"));
        let found = diagnostics(text);
        assert!(found.iter().all(|d| d.range.start != d.range.end));
        assert!(
            found
                .iter()
                .all(|d| openidea_plugin::text::offset(text, d.range.end).is_some())
        );
        assert!(has(
            r#"{"swagger":"2.0","info":{},"paths":{"/":{"get":{"parameters":[{"name":"x","in":"cookie"}]}}}}"#,
            "parameter-in"
        ));
        assert!(!has(
            r#"{"openapi":"3.1.1","info":{"title":"A","version":"1"},"paths":{"/":{"get":{}}}}"#,
            "required"
        ));
    }
    #[test]
    fn malformed_generic_unknown_versions_and_roots_are_quiet() {
        for text in [
            r#"{"openapi":"3.0.4", "info": "#,
            r#"{"openapi":"3.2.0","info":false}"#,
            r#"{"openapi":"4.0.0","paths":false}"#,
            r#"{"swagger":"3.0","paths":false}"#,
            r#"{"openapi":3,"paths":false}"#,
            r#"{"openapi":"3.0.0","swagger":"2.0"}"#,
            r#"{"description":"openapi","info":false}"#,
            r#"{"openapi":"3.0.bad","info":false}"#,
        ] {
            assert!(diagnostics(text).is_empty(), "{text}");
        }
    }
    #[test]
    fn provider_uses_current_document_without_projects_or_reference_reads() {
        use openidea_plugin::{
            diagnostics::DiagnosticsProvider, mock::MockHost, providers::Document,
        };
        let host = MockHost::install().with_capabilities(&[]);
        let text = r#"{"openapi":"3.0.4","info":{},"paths":{"/":{"get":{"responses":{"200":{"$ref":"https://example.test/response.json"}}}}}}"#;
        let mut plugin = crate::OpenApi::default();
        let mut doc = Document {
            path: "/unknown/api.json".into(),
            language: "json".into(),
            text: text.into(),
            version: 1,
        };
        assert!(!plugin.diagnostics(&doc).is_empty());
        doc.text =
            r#"{"openapi":"3.0.4","info":{"title":"Fixed","version":"1"},"paths":{}}"#.into();
        assert!(plugin.diagnostics(&doc).is_empty());
        host.open_editor("/unknown/api.json", "json", &doc.text, &[]);
        host.refresh_diagnostics(&mut plugin);
        assert!(
            host.provider_diagnostics()
                .get("/unknown/api.json")
                .is_none_or(Vec::is_empty)
        );
        assert!(host.requests().is_empty());
        assert!(host.diagnostics().is_empty());
        doc.language = "yaml".into();
        assert!(plugin.diagnostics(&doc).is_empty());
    }
    #[test]
    fn utf16_locations_round_trip() {
        let text = r#"{"openapi":"3.0.4","info":{"title":"😀"},"paths":{},"components":{"schemas":{"a.b":{"type":false}}}}"#;
        let found = diagnostics(text);
        let d = found
            .iter()
            .find(|d| d.code.as_deref() == Some("openapi-structure-schema-type"))
            .unwrap();
        let from = openidea_plugin::text::offset(text, d.range.start).unwrap();
        let to = openidea_plugin::text::offset(text, d.range.end).unwrap();
        assert_eq!(&text[from..to], "type");
        assert_eq!(
            d.range.start.character as usize,
            text[..from].encode_utf16().count()
        );
    }
    #[test]
    fn size_depth_nodes_and_result_counts_are_bounded() {
        let mut text = r#"{"openapi":"3.0.4","info":{}}"#.to_owned();
        text.push_str(&" ".repeat(MAX_BYTES - text.len()));
        assert!(
            !diagnostics(&text).is_empty(),
            "exact byte boundary is checked"
        );
        text.push(' ');
        assert!(diagnostics(&text).is_empty());
        let nested = format!(
            r#"{{"openapi":"3.0.4","x":{}0{}}}"#,
            "[".repeat(60),
            "]".repeat(60)
        );
        assert!(diagnostics(&nested).is_empty());
        let duplicates = format!(
            r#"{{"openapi":"3.0.4",{}}}"#,
            vec![r#""x":0"#; MAX_NODES + 1].join(",")
        );
        assert!(diagnostics(&duplicates).is_empty());
        let nodes = format!(
            r#"{{"openapi":"3.0.4","x":[{}]}}"#,
            vec!["0"; MAX_NODES].join(",")
        );
        assert!(diagnostics(&nodes).is_empty());
        let paths = (0..1000)
            .map(|i| format!(r#""/{i}":{{"get":{{"responses":{{"200":{{}}}}}}}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let doc = format!(r#"{{"openapi":"3.0.4","info":{{}},"paths":{{{paths}}}}}"#);
        assert_eq!(diagnostics(&doc).len(), MAX_RESULTS);
    }
    #[test]
    fn custom_dialects_and_reference_siblings_are_not_assumed() {
        let text = r#"{"openapi":"3.1.1","jsonSchemaDialect":"https://example.test/dialect","info":{"title":"A","version":"1"},"components":{"schemas":{"Custom":{"type":false}}}}"#;
        assert!(diagnostics(text).is_empty());
        let text = r#"{"openapi":"3.0.4","info":{"title":"A","version":"1"},"paths":{},"components":{"schemas":{"Ref":{"$ref":"external.json","type":false}}}}"#;
        assert!(diagnostics(text).is_empty());
    }
}

#[cfg(test)]
mod location_regressions {
    use super::*;
    #[test]
    fn ambiguous_flattened_locations_stay_quiet_instead_of_parent_fallback() {
        let text = r#"{"openapi":"3.0.4","info":{"title":"A","version":"1"},"paths":{},"components.schemas.A.type":"unrelated","components":{"schemas":{"A":{"type":false}}}}"#;
        let found = diagnostics(text);
        assert!(found.iter().all(|d|d.code.as_deref()!=Some("openapi-structure-schema-type")));
    }
    #[test]
    fn every_utf8_document_prefix_stays_quiet_or_located() {
        let text = r#"{"openapi":"3.0.4","info":{"title":"😀","version":"1"},"paths":{"/":{"get":{"responses":{"200":{}}}}}}"#;
        for end in (0..=text.len()).filter(|end| text.is_char_boundary(*end)) {
            for d in diagnostics(&text[..end]) {
                assert!(openidea_plugin::text::offset(&text[..end], d.range.end).is_some());
            }
        }
    }
}

#[cfg(test)]
mod extended_tests {
    use super::*;
    fn spec(version: &str, body: Value) -> String {
        let mut root =
            serde_json::json!({"openapi":version,"info":{"title":"API","version":"1"},"paths":{}});
        root.as_object_mut()
            .unwrap()
            .extend(body.as_object().unwrap().clone());
        root.to_string()
    }
    fn codes(text: &str) -> Vec<String> {
        diagnostics(text)
            .into_iter()
            .filter_map(|d| d.code)
            .collect()
    }
    #[test]
    fn reusable_objects_share_inline_validation_without_resolving_references() {
        let text = spec(
            "3.0.4",
            serde_json::json!({"components":{
                "responses":{"Bad":{"description":false},"Ref":{"$ref":"https://example.test/r","description":false}},
                "parameters":{"Bad":{"name":"id","in":"path","required":false}},
                "requestBodies":{"Bad":{"required":"yes"}},
                "headers":{"Bad":{"schema":{"type":"array"},"content":{}}}
            }}),
        );
        let found = diagnostics(&text);
        assert!(
            found
                .iter()
                .any(|d| d.code.as_deref() == Some("openapi-structure-path-required"))
        );
        assert!(
            found
                .iter()
                .any(|d| d.code.as_deref() == Some("openapi-structure-parameter-content"))
        );
        assert_eq!(
            found
                .iter()
                .filter(|d| d.message.contains("`description`"))
                .count(),
            1,
            "reference siblings stay quiet"
        );
        for d in found {
            assert!(d.range.start != d.range.end);
            assert!(openidea_plugin::text::offset(&text, d.range.end).is_some());
        }
        assert!(!codes(&text).is_empty());
    }
    #[test]
    fn swagger_reusable_responses_and_parameters_are_version_owned() {
        let text = r#"{"swagger":"2.0","info":{"title":"A","version":"1"},"paths":{},"responses":{"R":{"schema":{"type":"array"}}},"parameters":{"P":{"name":"q","in":"cookie"}}}"#;
        let c = codes(text);
        assert!(c.contains(&"openapi-structure-required".into()));
        assert!(c.contains(&"openapi-structure-parameter-in".into()));
        let valid = r#"{"swagger":"2.0","info":{"title":"A","version":"1"},"paths":{},"responses":{"x-vendor":false,"R":{"description":"ok","headers":{"H":{"type":"string"}}}},"parameters":{"x-vendor":false,"P":{"name":"q","in":"query","type":"string"}}}"#;
        assert!(diagnostics(valid).is_empty());
    }
    #[test]
    fn server_requirements_preserve_should_vs_must_version_difference() {
        let body = serde_json::json!({"servers":[{"url":"/{x}","variables":{"x":{"default":"outside","enum":[]}}}]});
        assert!(
            diagnostics(&spec("3.0.4", body.clone())).is_empty(),
            "3.0 SHOULD advice is not a schema error"
        );
        assert!(codes(&spec("3.1.1", body)).contains(&"openapi-structure-server-enum".into()));
        let body = serde_json::json!({"servers":[{"url":"/{x}","variables":{"x":{"default":"outside","enum":["inside"]}}}]});
        assert!(codes(&spec("3.1.1", body)).contains(&"openapi-structure-server-default".into()));
        let body = serde_json::json!({"paths":{"/":{"servers":[{}],"get":{"servers":[{"url":false}],"responses":{"200":{"description":"ok"}}}}}});
        let c = codes(&spec("3.0.4", body));
        assert!(c.contains(&"openapi-structure-required".into()));
        assert!(c.contains(&"openapi-structure-type".into()));
    }
    #[test]
    fn schema_keyword_types_are_dialect_and_version_specific() {
        let body = serde_json::json!({"components":{"schemas":{"S":{"required":[],"enum":[],"exclusiveMinimum":true}}}});
        let c = codes(&spec("3.0.4", body.clone()));
        assert!(c.contains(&"openapi-structure-schema-required".into()));
        assert!(c.contains(&"openapi-structure-schema-enum".into()));
        assert!(!c.contains(&"openapi-structure-schema-bound".into()));
        let c = codes(&spec("3.1.1", body));
        assert!(!c.contains(&"openapi-structure-schema-required".into()));
        assert!(!c.contains(&"openapi-structure-schema-enum".into()));
        assert!(c.contains(&"openapi-structure-schema-bound".into()));
        let body = serde_json::json!({"components":{"schemas":{"S":{"required":["x","x"],"minLength":-1,"maxItems":1.5,"multipleOf":0,"pattern":false}}}});
        let c = codes(&spec("3.1.1", body.clone()));
        assert!(c.contains(&"openapi-structure-schema-required".into()));
        assert_eq!(
            c.iter()
                .filter(|c| *c == "openapi-structure-schema-bound")
                .count(),
            3
        );
        let mut custom = body;
        custom["jsonSchemaDialect"] = Value::String("https://example.test/dialect".into());
        assert!(diagnostics(&spec("3.1.1", custom)).is_empty());
    }
    #[test]
    fn reusable_parameter_source_ranges_use_real_keys_after_unicode() {
        let text = r#"{"openapi":"3.0.4","info":{"title":"😀","version":"1"},"paths":{},"components":{"parameters":{"P":{"name":"q","in":false}}}}"#;
        let d = diagnostics(text)
            .into_iter()
            .find(|d| d.message.contains("`in`"))
            .unwrap();
        let a = openidea_plugin::text::offset(text, d.range.start).unwrap();
        let b = openidea_plugin::text::offset(text, d.range.end).unwrap();
        assert_eq!(&text[a..b], "in");
        assert_eq!(
            d.range.start.character as usize,
            text[..a].encode_utf16().count()
        );
    }
}
