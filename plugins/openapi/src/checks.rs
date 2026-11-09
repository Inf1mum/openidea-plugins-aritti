//! Problems in an OpenAPI document: unresolved `$ref`s, duplicate operationIds
//! and path parameters that disagree with their path template.

use crate::spec::{Operation, Spec, operations, refs, scan, split_ref};
use openidea_plugin::{Diagnostic, Severity};
use spring_text::range;
use std::collections::{HashMap, HashSet};


pub enum Fetched {
    Text(String),
    Missing,
    /// The file exists but can't be read; nothing is reported about refs into it.
    Unreadable,
}

enum Loaded {
    Model(crate::pointer_status::Proof),
    Missing,
    Unreadable,
}

#[cfg(test)]
mod pointer_integrity_regressions {
    use super::*;
    fn unresolved(text:&str)->Vec<Diagnostic> {
        check(text,&crate::spec::parse(text).unwrap(),&|_|Fetched::Unreadable).into_iter().filter(|d|d.code.as_deref()==Some("unresolved-ref")).collect()
    }
    #[test]
    fn ambiguous_duplicate_dotted_escaped_and_base_owners_do_not_prove_missing_local_refs() {
        for text in [
            r##"{"openapi":"3.1.1","components":{},"components":{"schemas":{}},"x":{"$ref":"#/components/schemas/Missing"}}"##,
            "openapi: 3.1.1\ncomponents: {}\ncomponents:\n  schemas: {}\nx:\n  $ref: '#/components/schemas/Missing'\n",
            r##"{"openapi":"3.1.1","components.schemas.Pet":{},"x":{"$ref":"#/components/schemas/Pet/properties/Missing"}}"##,
            "openapi: 3.1.1\ncomponents:\n  schemas:\n    \"P\\x65t\": {}\nx:\n  $ref: '#/components/schemas/Pet/properties/Missing'\n",
            r##"{"openapi":"3.1.1","$id":"other.json","x":{"$ref":"#/components/schemas/Missing"}}"##,
        ] {assert!(unresolved(text).is_empty(),"{text}");}
    }
    #[test]
    fn ordinary_missing_json_yaml_and_escaped_string_keys_keep_exact_reference_diagnostics() {
        for text in [
            r##"{"openapi":"3.1.1","components":{"schemas":{"\u0050et":{"properties":{}}}},"x":{"$ref":"#/components/schemas/Pet/properties/Missing"}}"##,
            r##"{"openapi":"3.1.1","paths":{"/pets.v1":{"get":{"responses":{"200":{"$ref":"#/Missing"}}}}}}"##,
            "openapi: 3.1.1\ncomponents:\n  schemas:\n    'A/B':\n      properties:\n        '~id': {}\nx:\n  $ref: '#/components/schemas/A~1B/properties/~0id'\ny:\n  $ref: '#/components/schemas/A~1B/properties/Missing'\n",
        ] {
            let found=unresolved(text);assert_eq!(found.len(),1,"{text}");
            assert!(found[0].message.contains("Missing"));
            let range=found[0].range;
            let start=openidea_plugin::text::offset(text,range.start).unwrap();
            let end=openidea_plugin::text::offset(text,range.end).unwrap();
            assert!(text[start..end].contains("Missing"));
        }
    }
    #[test]
    fn unsupported_pointer_or_non_reference_source_forms_do_not_manufacture_absence() {
        for text in [
            r##"{"openapi":"3.1.1","x":{"$ref":"#/Missing%41"}}"##,
            r##"{"openapi":"3.1.1","x":{"$ref":"#/Missing~2"}}"##,
            r##"{"openapi":"3.1.1","Pet":[{}],"x":{"$ref":"#/Pet/0/Missing"}}"##,
            "openapi: 3.1.1\ndescription: |\n  $ref: '#/Missing'\n",
            "openapi: 3.1.1\nx:\n  - $ref: '#/Missing'\n",
        ] {assert!(unresolved(text).is_empty(),"{text}");}
    }
    #[test]
    fn repeated_external_targets_are_captured_once_and_reference_file_limits_stay_unknown() {
        use std::cell::Cell;
        let mut root=serde_json::Map::new();root.insert("openapi".into(),serde_json::json!("3.1.1"));
        for n in 0..1100 {root.insert(format!("x{n}"),serde_json::json!({"$ref":"common.json#/Missing"}));}
        let text=serde_json::Value::Object(root).to_string();
        let reads=Cell::new(0);
        let found=check(&text,&crate::spec::parse(&text).unwrap(),&|_| {reads.set(reads.get()+1);Fetched::Text("{}".into())});
        assert_eq!(reads.get(),1);assert_eq!(found.iter().filter(|d|d.code.as_deref()==Some("unresolved-ref")).count(),1024);
        let mut root=serde_json::Map::new();root.insert("openapi".into(),serde_json::json!("3.1.1"));
        for n in 0..40 {root.insert(format!("x{n}"),serde_json::json!({"$ref":format!("{n}.json#/Missing")}));}
        let text=serde_json::Value::Object(root).to_string();reads.set(0);
        let found=check(&text,&crate::spec::parse(&text).unwrap(),&|_| {reads.set(reads.get()+1);Fetched::Missing});
        assert_eq!(reads.get(),32);assert_eq!(found.len(),32);
        reads.set(0);
        let content=format!("{{}}{}"," ".repeat((512<<10)-2));
        let found=check(&text,&crate::spec::parse(&text).unwrap(),&|_| {reads.set(reads.get()+1);Fetched::Text(content.clone())});
        assert_eq!(reads.get(),8);assert_eq!(found.len(),8);
    }
    #[test]
    fn unreadable_or_ambiguous_external_models_are_unknown_instead_of_missing_pointers() {
        let text=r##"{"openapi":"3.1.1","x":{"$ref":"common.json#/Missing"}}"##;
        for content in [r#"{"Pet":{},"Pet":{}}"#,r#"{"$id":"other.json"}"#,r#"{"Pet":"#] {
            let found=check(text,&crate::spec::parse(text).unwrap(),&|_|Fetched::Text(content.into()));
            assert!(found.is_empty(),"{content}");
        }
        for text in [
            r##"{"openapi":"3.1.1","$id":"other.json","x":{"$ref":"common.json#/Missing"}}"##,
            r##"{"openapi":"3.1.1","x":{"$ref":"file:common.json#/Missing"}}"##,
        ] {
            assert!(check(text,&crate::spec::parse(text).unwrap(),&|_|panic!("unsupported physical source must not read a target")).is_empty());
        }
    }
    #[test]
    fn large_flattened_paths_hit_retained_model_budget_without_missing_claims_for_uncaptured_files() {
        let mut members=serde_json::Map::new();
        for n in 0..3000 {members.insert(format!("n{n}"),serde_json::json!({}));}
        let content=serde_json::json!({"x".repeat(1024):serde_json::Value::Object(members)}).to_string();
        let text=r##"{"openapi":"3.1.1","a":{"$ref":"a.json#/Missing"},"b":{"$ref":"b.json#/Missing"},"c":{"$ref":"c.json#/Missing"}}"##;
        let found=check(text,&crate::spec::parse(text).unwrap(),&|_|Fetched::Text(content.clone()));
        assert_eq!(found.len(),2);
    }
    #[test]
    fn ambiguous_parameter_reference_and_fake_array_owners_do_not_prove_missing_path_parameters() {
        for text in [
            "openapi: 3.1.1\npaths:\n  /a/{id}:\n    get:\n      parameters:\n        - $ref: '#/components/parameters/P'\ncomponents:\n  parameters:\n    P:\n      name: wrong\n      in: path\ncomponents:\n  parameters:\n    P:\n      name: id\n      in: path\n",
            "openapi: 3.1.1\npaths:\n  /a/{id}:\n    get:\n      parameters:\n        - $ref: '#/components/parameters/P'\ncomponents.parameters.P:\n  name: wrong\n  in: path\n",
            "openapi: 3.1.1\npaths:\n  /a/{id}:\n    get:\n      'parameters[0].name': wrong\n      'parameters[0].in': path\n",
        ] {
            let found=check(text,&crate::spec::parse(text).unwrap(),&|_|Fetched::Unreadable);
            assert!(found.iter().all(|d|d.code.as_deref()!=Some("path-parameter")),"{text}");
        }
    }
    #[test]
    fn actual_shared_and_local_reusable_parameter_facts_preserve_real_template_errors() {
        let text="openapi: 3.1.1\npaths:\n  /a/{id}/{x}:\n    parameters:\n      - $ref: '#/components/parameters/I~1D'\n    get:\n      parameters:\n        - name: extra\n          in: path\ncomponents:\n  parameters:\n    'I/D':\n      name: id\n      in: path\n";
        let found=check(text,&crate::spec::parse(text).unwrap(),&|_|Fetched::Unreadable);
        let found:Vec<_>=found.iter().filter(|d|d.code.as_deref()==Some("path-parameter")).collect();
        assert_eq!(found.len(),2);assert!(found.iter().any(|d|d.message.contains("`x` is not declared")));
        let warning=found.iter().find(|d|d.severity==Severity::Warning).unwrap();
        assert_eq!(&text[openidea_plugin::text::offset(text,warning.range.start).unwrap()..openidea_plugin::text::offset(text,warning.range.end).unwrap()],"extra");
        let json=r##"{"openapi":"3.1.1","paths":{"/a/{id}":{"get":{"parameters":[{"$ref":"#/components/parameters/Id"}]}}},"components":{"parameters":{"Id":{"name":"\u0069d","in":"path"}}}}"##;
        assert!(check(json,&crate::spec::parse(json).unwrap(),&|_|Fetched::Unreadable).iter().all(|d|d.code.as_deref()!=Some("path-parameter")));
    }
    #[test]
    fn operation_parameter_override_owns_the_effective_warning_range() {
        let text="openapi: 3.1.1\npaths:\n  /a/{id}:\n    parameters:\n      - name: other\n        in: path\n    get:\n      parameters:\n        - name: other\n          in: path\n";
        let found=check(text,&crate::spec::parse(text).unwrap(),&|_|Fetched::Unreadable);
        let found:Vec<_>=found.iter().filter(|d|d.code.as_deref()==Some("path-parameter")).collect();
        assert_eq!(found.len(),2);
        let warning=found.iter().find(|d|d.severity==Severity::Warning).unwrap();
        assert_eq!(openidea_plugin::text::offset(text,warning.range.start),text.rfind("other"));
    }
    #[test]
    fn incomplete_duplicate_external_rebased_or_excess_parameter_context_stays_unknown() {
        for text in [
            "openapi: 3.1.1\npaths:\n  /a/{id}:\n    $ref: 'common.yaml#/Path'\n    get: {}\n",
            "openapi: 3.1.1\npaths:\n  /a/{id}:\n    get:\n      parameters:\n        - name: wrong\n",
            "openapi: 3.1.1\npaths:\n  /a/{id}:\n    get:\n      parameters:\n        - name: wrong\n          in: path\n        - name: wrong\n          in: path\n",
            "openapi: 3.1.1\npaths:\n  /a/{id}:\n    get:\n      parameters:\n        - $ref: '#/components/parameters/I%64'\n",
            "openapi: 3.1.1\npaths:\n  /a/{id}:\n    get:\n      parameters:\n        - name: \"\\x69d\"\n          in: path\n",
        ] {
            assert!(check(text,&crate::spec::parse(text).unwrap(),&|_|Fetched::Unreadable).iter().all(|d|d.code.as_deref()!=Some("path-parameter")),"{text}");
        }
        let parameters=(0..1025).map(|n|serde_json::json!({"name":format!("q{n}"),"in":"query"})).collect::<Vec<_>>();
        let text=serde_json::json!({"openapi":"3.1.1","paths":{"/a/{id}":{"get":{"parameters":parameters}}}}).to_string();
        assert!(check(&text,&crate::spec::parse(&text).unwrap(),&|_|Fetched::Unreadable).iter().all(|d|d.code.as_deref()!=Some("path-parameter")));
    }
    #[test]
    fn duplicate_or_dotted_operationid_owners_do_not_establish_duplicate_id_facts() {
        for text in [
            "openapi: 3.1.1\npaths:\n  /a:\n    get:\n      operationId: same\n  /a:\n    get:\n      operationId: same\n",
            "openapi: 3.1.1\npaths:\n  /a:\n    get: {}\n  /b:\n    get:\n      operationId: same\npaths./a.get.operationId: same\n",
            r#"{"openapi":"3.1.1","paths":{"/a":{"get":{"operationId":123}},"/b":{"get":{"operationId":123}}}}"#,
        ] {
            let found=check(text,&crate::spec::parse(text).unwrap(),&|_|Fetched::Unreadable);
            assert!(found.iter().all(|d|d.code.as_deref()!=Some("duplicate-operation-id")),"{text}");
        }
    }
    #[test]
    fn genuine_escaped_json_duplicate_ids_keep_exact_utf16_value_ranges() {
        let text=r#"{"openapi":"3.1.1","info":{"title":"😀"},"paths":{"/a":{"get":{"operationId":"dup\uD83D\uDE00"}},"/b":{"post":{"operationId":"dup😀"}}}}"#;
        let found=check(text,&crate::spec::parse(text).unwrap(),&|_|Fetched::Unreadable);
        let found:Vec<_>=found.iter().filter(|d|d.code.as_deref()==Some("duplicate-operation-id")).collect();
        assert_eq!(found.len(),2);
        for diagnostic in found {
            assert!(diagnostic.message.contains("dup😀"));
            let range=diagnostic.range;let at=openidea_plugin::text::offset(text,range.start).unwrap();let end=openidea_plugin::text::offset(text,range.end).unwrap();
            assert!(matches!(&text[at..end],"dup\\uD83D\\uDE00"|"dup😀"));
            assert_eq!(range.start.character as usize,text[..at].encode_utf16().count());
        }
    }
    #[test]
    fn unknown_composed_transformed_control_and_long_ids_are_quiet_and_results_are_bounded() {
        for text in [
            "openapi: 3.1.1\npaths:\n  /a:\n    $ref: 'common.yaml#/Path'\n    get:\n      operationId: same\n  /b:\n    get:\n      operationId: same\n",
            "openapi: 3.1.1\npaths:\n  /a:\n    get:\n      operationId: \"\\x73ame\"\n  /b:\n    get:\n      operationId: same\n",
            r#"{"openapi":"3.1.1","paths":{"/a":{"get":{"operationId":"bad\nid"}},"/b":{"get":{"operationId":"bad\nid"}}}}"#,
        ] {
            assert!(check(text,&crate::spec::parse(text).unwrap(),&|_|Fetched::Unreadable).iter().all(|d|d.code.as_deref()!=Some("duplicate-operation-id")),"{text}");
        }
        let long="a".repeat(4097);
        let text=serde_json::json!({"openapi":"3.1.1","paths":{"/a":{"get":{"operationId":long}},"/b":{"get":{"operationId":long}}}}).to_string();
        assert!(check(&text,&crate::spec::parse(&text).unwrap(),&|_|Fetched::Unreadable).iter().all(|d|d.code.as_deref()!=Some("duplicate-operation-id")));
        let paths=(0..300).map(|n|(format!("/p{n}"),serde_json::json!({"get":{"operationId":"same"}}))).collect::<serde_json::Map<_,_>>();
        let text=serde_json::json!({"openapi":"3.1.1","paths":paths}).to_string();
        assert_eq!(check(&text,&crate::spec::parse(&text).unwrap(),&|_|Fetched::Unreadable).iter().filter(|d|d.code.as_deref()==Some("duplicate-operation-id")).count(),256);
    }
    #[test]
    fn current_source_id_changes_remove_the_prior_duplicate_facts_without_io() {
        let original="openapi: 3.1.1\npaths:\n  /a:\n    get:\n      operationId: same\n  /b:\n    get:\n      operationId: same\n";
        let changed=original.replacen("operationId: same","operationId: changed",1);
        for (text,count) in [(original,2),(changed.as_str(),0)] {
            assert_eq!(check(text,&crate::spec::parse(text).unwrap(),&|_|panic!("ID facts must not read external files")).iter().filter(|d|d.code.as_deref()==Some("duplicate-operation-id")).count(),count);
        }
    }
}

pub fn check(text: &str, spec: &Spec, read: &dyn Fn(&str) -> Fetched) -> Vec<Diagnostic> {
    let ops = operations(text, &spec.keys);
    let model=crate::pointer_status::Proof::parse(text);
    let mut found = unresolved_refs(text, model.as_ref(), read);
    found.extend(duplicate_operation_ids(text, model.as_ref(), &ops));
    found.extend(path_parameters(text, model.as_ref(), &ops));
    found
}

/// A file that looks like a spec without its root key: a `paths` object of `/…` items holding HTTP methods.
pub fn spec_candidate(text: &str) -> Vec<Diagnostic> {
    if !text.contains("paths") {
        return Vec::new();
    }

    let keys = scan(text);
    if keys.iter().any(|k| k.key == "openapi" || k.key == "swagger") {
        return Vec::new();
    }

    let Some(paths) = keys.iter().find(|k| k.key == "paths") else {
        return Vec::new();
    };

    if operations(text, &keys).is_empty() {
        return Vec::new();
    }

    let message = "This file has a `paths` object but no `openapi` or `swagger` key, so it is not treated as an API description".to_owned();

    vec![problem(text, (paths.at, paths.end), Severity::Info, "missing-openapi-root", message)]
}

fn problem(text: &str, (at, end): (usize, usize), severity: Severity, code: &str, message: String) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(range(text, at, end), severity, message);
    diagnostic.code = Some(code.into());
    diagnostic
}

fn unresolved_refs(text: &str, local:Option<&crate::pointer_status::Proof>, read: &dyn Fn(&str) -> Fetched) -> Vec<Diagnostic> {
    use crate::pointer_status::{Proof,Status};
    const MAX_REFS:usize=1024;
    const MAX_FILES:usize=32;
    const MAX_CAPTURE:usize=4<<20;
    const MAX_MODELS:usize=16<<20;
    let found_refs=refs(text);
    let Some(local)=local else {return Vec::new();};
    let mut files:HashMap<String,Loaded>=HashMap::new();
    let (mut reads,mut bytes,mut retained)=(0usize,0usize,0usize);
    for r in found_refs.iter().take(MAX_REFS) {
        if !local.reference(r) {continue;}
        let Some((file,_))=split_ref(&r.target) else {continue;};
        if file.is_empty() || files.contains_key(file) {continue;}
        let loaded=if reads>=MAX_FILES || bytes>=MAX_CAPTURE || file.len()>4096 || file.starts_with('/') || file.chars().any(char::is_control) || file.contains([':', '\\', '%', '?', '#']) {Loaded::Unreadable} else {
            reads+=1;
            match read(file) {
                Fetched::Missing=>Loaded::Missing,
                Fetched::Unreadable=>Loaded::Unreadable,
                Fetched::Text(content)=>{
                    bytes=bytes.saturating_add(content.len());
                    if bytes>MAX_CAPTURE {Loaded::Unreadable} else {
                        match Proof::parse(&content) {
                            Some(model) if retained.saturating_add(model.retained_bytes())<=MAX_MODELS=>{retained+=model.retained_bytes();Loaded::Model(model)},
                            _=>Loaded::Unreadable,
                        }
                    }
                },
            }
        };
        files.insert(file.into(),loaded);
    }
    let mut found=Vec::new();
    for r in found_refs.into_iter().take(MAX_REFS) {
        if !local.reference(&r) {continue;}
        let Some((file,fragment))=split_ref(&r.target) else {continue;};
        let message=if file.is_empty() {
            (local.status(fragment)==Status::Missing).then(||format!("Cannot find `{}`",r.target))
        } else {
            match &files[file] {
                Loaded::Missing=>Some(format!("Cannot find file `{file}`")),
                Loaded::Unreadable=>None,
                Loaded::Model(model)=>(model.status(fragment)==Status::Missing).then(||format!("Cannot find `{}`",r.target)),
            }
        };
        if let Some(message)=message {found.push(problem(text,(r.at,r.end),Severity::Error,"unresolved-ref",message));}
    }
    found
}

fn duplicate_operation_ids(text: &str, model:Option<&crate::pointer_status::Proof>, ops: &[Operation]) -> Vec<Diagnostic> {
    let Some(model)=model else {return Vec::new();};
    let facts:Vec<_>=ops.iter().filter_map(|operation|model.operation_id(operation)).collect();
    let mut counts:HashMap<&str,usize>=HashMap::new();
    for (id,_) in &facts {*counts.entry(id).or_default()+=1;}
    facts.into_iter().filter(|(id,_)|counts.get(id).is_some_and(|count|*count>1)).take(256)
        .map(|(id,span)|problem(text,span,Severity::Error,"duplicate-operation-id",format!("operationId `{id}` is used by more than one operation"))).collect()
}

fn template_names(path: &str) -> Vec<&str> {
    path.split('{').skip(1).filter_map(|rest| rest.split_once('}').map(|(name, _)| name)).collect()
}

fn path_parameters(text: &str, model:Option<&crate::pointer_status::Proof>, ops: &[Operation]) -> Vec<Diagnostic> {
    let Some(model)=model else {return Vec::new();};
    let mut found = Vec::new();

    for op in ops {
        let Some(parameters)=model.operation_parameters(op) else {continue;};
        let declared:Vec<_>=parameters.iter().filter(|parameter|parameter.location=="path").collect();
        let template = template_names(&op.path);
        let declared_names: HashSet<&str> = declared.iter().map(|p| p.name.as_str()).collect();

        for name in template.iter().filter(|name| !declared_names.contains(**name)) {
            let message = format!("Path parameter `{name}` is not declared for {} {}", op.method, op.path);
            found.push(problem(text, (op.at, op.end), Severity::Error, "path-parameter", message));
        }

        for parameter in &declared {
            let Some(at) = parameter.declared_at else {
                continue;
            };
            if template.contains(&parameter.name.as_str()) {
                continue;
            }

            let message = format!("Path parameter `{}` is not in the path {}", parameter.name, op.path);
            found.push(problem(text, at, Severity::Warning, "path-parameter", message));
        }
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{PETSTORE, parse};
    use openidea_plugin::Severity;

    const COMMON: &str = "Pet:\n  type: object\n";

    fn found(text: &str) -> Vec<(u32, Severity, String, String)> {
        let read = |f: &str| match f {
            "common.yaml" => Fetched::Text(COMMON.to_owned()),
            "locked.yaml" => Fetched::Unreadable,
            _ => Fetched::Missing,
        };
        check(text, &parse(text).unwrap(), &read)
            .into_iter()
            .map(|d| (d.range.start.line, d.severity, d.code.unwrap(), d.message))
            .collect()
    }

    #[test]
    fn the_petstore_is_clean() {
        assert!(found(PETSTORE).is_empty());
    }

    #[test]
    fn reports_unresolved_local_and_file_refs() {
        let text = "openapi: 3.0.0\npaths: {}\ncomponents:\n  schemas:\n    A:\n      $ref: '#/components/schemas/B'\n    C:\n      $ref: 'common.yaml#/Missing'\n    D:\n      $ref: 'common.yaml#/Pet'\n    E:\n      $ref: 'nope.yaml'\n    F:\n      $ref: 'https://example.com/x.yaml#/Y'\n";
        let e = |line, msg: &str| (line, Severity::Error, "unresolved-ref".to_owned(), msg.to_owned());
        assert_eq!(found(text), [
            e(5, "Cannot find `#/components/schemas/B`"),
            e(7, "Cannot find `common.yaml#/Missing`"),
            e(11, "Cannot find file `nope.yaml`"),
        ]);
    }

    #[test]
    fn a_ref_into_an_unreadable_file_is_not_reported() {
        let text = "openapi: 3.0.0\npaths: {}\nx:\n  $ref: 'locked.yaml#/Pet'\ny:\n  $ref: 'locked.yaml'\n";
        assert!(found(text).is_empty());
    }

    #[test]
    fn reports_every_duplicate_operation_id() {
        let text = "openapi: 3.0.0\npaths:\n  /a:\n    get:\n      operationId: same\n    post:\n      operationId: same\n";
        let lines: Vec<_> =
            found(text).into_iter().filter(|f| f.2 == "duplicate-operation-id").map(|f| f.0).collect();
        assert_eq!(lines, [4, 6]);
    }

    #[test]
    fn checks_path_parameters_against_the_template() {
        let text = "openapi: 3.0.0\npaths:\n  /a/{id}/{x}:\n    get:\n      parameters:\n        - name: id\n          in: path\n        - name: y\n          in: path\n      responses: {}\n";
        assert_eq!(found(text), [
            (3, Severity::Error, "path-parameter".into(), "Path parameter `x` is not declared for GET /a/{id}/{x}".into()),
            (7, Severity::Warning, "path-parameter".into(), "Path parameter `y` is not in the path /a/{id}/{x}".into()),
        ]);
    }

    #[test]
    fn skips_the_parameter_check_when_parameters_cannot_be_read() {
        let flow = "openapi: 3.0.0\npaths:\n  /a/{id}:\n    get:\n      parameters: [{ $ref: '#/components/parameters/Id' }]\n      responses: {}\ncomponents:\n  parameters:\n    Id: { name: id, in: path }\n";
        assert!(found(flow).iter().all(|f| f.2 != "path-parameter"));
        let external = "openapi: 3.0.0\npaths:\n  /a/{id}:\n    get:\n      parameters:\n        - $ref: 'common.yaml#/Pet'\n      responses: {}\n";
        assert!(found(external).iter().all(|f| f.2 != "path-parameter"));
    }

    #[test]
    fn checks_a_large_spec_quickly() {
        let mut text = String::from("openapi: 3.0.0\npaths:\n");
        for i in 0..3500 {
            text.push_str(&format!(
                "  /p{i}/{{id}}:\n    get:\n      operationId: op{i}\n      parameters:\n        - name: id\n          in: path\n      responses:\n        '200':\n          $ref: '#/components/schemas/S{}'\n",
                i % 200
            ));
        }
        text.push_str("components:\n  schemas:\n");
        for i in 0..200 {
            text.push_str(&format!("    S{i}:\n      type: object\n"));
        }
        assert!(text.lines().count() > 30_000);

        let started = std::time::Instant::now();
        let problems = found(&text);
        let elapsed = started.elapsed();
        assert!(problems.is_empty());
        assert!(elapsed.as_millis() < 1000, "took {elapsed:?}");
        eprintln!("large spec check: {elapsed:?}");
    }

    fn template(name: &str) -> String {
        std::fs::read_to_string(format!("{}/templates/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    #[test]
    fn templates_pass_the_openapi_checks() {
        for name in ["openapi.yaml", "openapi.json"] {
            let text = template(name);
            let spec = parse(&text).unwrap_or_else(|| panic!("{name} is not detected as an OpenAPI document"));
            assert_eq!(spec.version, "3.1.0", "{name}");
            assert!(found(&text).is_empty(), "{name}");
            assert_eq!(operations(&text, &spec.keys).len(), 1, "{name}");
        }
    }

    #[test]
    fn swagger_templates_are_registered_and_exist() {
        let manifest = std::fs::read_to_string(format!("{}/plugin.toml", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let ids: Vec<&str> = manifest.lines().filter_map(|l| l.strip_prefix("id = \"")?.strip_suffix('"')).collect();
        for id in ["openapi-yaml", "openapi-json", "asyncapi-yaml", "swagger-yaml", "swagger-json"] {
            assert!(ids.contains(&id), "{id}");
        }
        assert_eq!(manifest.matches("[[contributes.fileTemplates]]").count(), 5);
        for name in ["swagger.yaml", "swagger.json"] {
            assert!(manifest.contains(&format!("path = \"templates/{name}\"")), "{name}");
            let text = template(name);
            assert!(text.contains("$NAME$"), "{name}");
            assert!(!text.replace("$NAME$", "").replace("$END$", "").contains('$'), "{name}");
        }
    }

    #[test]
    fn swagger_templates_are_detected_as_specs() {
        for name in ["swagger.yaml", "swagger.json"] {
            let text = template(name).replace("$NAME$", "Demo");
            let spec = parse(&text).unwrap_or_else(|| panic!("{name} is not detected as a spec"));
            assert_eq!(spec.version, "2.0", "{name}");
            assert!(found(&text).is_empty(), "{name}");
            assert_eq!(operations(&text, &spec.keys).len(), 1, "{name}");
        }
    }

    #[test]
    fn asyncapi_template_is_not_checked_as_openapi() {
        assert!(parse(&template("asyncapi.yaml")).is_none());
    }

    #[test]
    fn every_template_is_registered_and_exists() {
        let manifest = std::fs::read_to_string(format!("{}/plugin.toml", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let mut registered: Vec<&str> = manifest.lines().filter_map(|l| l.strip_prefix("path = \"templates/")?.strip_suffix('"')).collect();
        let mut files: Vec<String> = std::fs::read_dir(format!("{}/templates", env!("CARGO_MANIFEST_DIR")))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        registered.sort();
        files.sort();
        assert_eq!(registered, ["asyncapi.yaml", "openapi.json", "openapi.yaml", "swagger.json", "swagger.yaml"]);
        assert_eq!(files, registered);
        for name in registered {
            let text = template(name);
            for (at, _) in text.match_indices('$') {
                let word: String = text[at + 1..].chars().take_while(|c| c.is_ascii_uppercase() || *c == '_').collect();
                let closed = !word.is_empty() && text[at + 1 + word.len()..].starts_with('$');
                assert!(!closed || word == "NAME" || word == "END", "{name} has the placeholder ${word}$");
            }
        }
    }
}
