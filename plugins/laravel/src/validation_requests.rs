//! Cached authored FormRequest rule keys. No source files or PHP are read during
//! resolution: the index supplies current owned snapshots and the editor buffer
//! overlays its own cached unit before ancestry or override checks.
use crate::php;
use openidea_plugin::providers::Document;
use std::collections::{BTreeMap, BTreeSet};
const MAX_SOURCE: usize = 256 * 1024;
const BASE: &str = "Illuminate\\Foundation\\Http\\FormRequest";
#[derive(Clone, Debug, PartialEq, Eq)]
enum Rules {
    Inherit,
    Literal(BTreeSet<String>),
    Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Request {
    full: String,
    parent: String,
    path: String,
    rules: Rules,
    blocked: bool,
    validated_override: bool,
    stamp: String,
}
pub(crate) type Requests = BTreeMap<String, Request>;
fn byte_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 128
}
fn words(code: &str) -> Vec<(usize, usize)> {
    let bytes = code.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if !byte_word(bytes[at]) {
            at += 1;
            continue;
        }
        let start = at;
        while at < bytes.len() && byte_word(bytes[at]) {
            at += 1;
        }
        out.push((start, at));
    }
    out
}
fn valid_field(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'*' | b'-') || b >= 128
        })
        && !value.chars().any(char::is_control)
}
fn literal_rules(
    source: &str,
    code: &str,
    body: std::ops::Range<usize>,
) -> Option<BTreeSet<String>> {
    let trimmed = code[body.clone()].trim_start();
    let Some(tail) = trimmed
        .get(..6)
        .filter(|word| word.eq_ignore_ascii_case("return"))
        .and_then(|_| trimmed.get(6..))
    else {
        return None;
    };
    if !tail.as_bytes().first().is_some_and(u8::is_ascii_whitespace) {
        return None;
    }
    let begin = body.end - tail.trim_start().len();
    let open = php::array_open(source, begin..body.end)?;
    let close = php::closing(source, open)?;
    if code[close + 1..body.end].trim() != ";" {
        return None;
    }
    let items = php::items(source, open);
    if items.len() > 256 {
        return None;
    }
    let mut fields = BTreeSet::new();
    for item in items {
        let (field, _) = php::literal(source, item.key?)?;
        if !valid_field(field) || !fields.insert(field.into()) {
            return None;
        }
    }
    Some(fields)
}
fn parse(source: &str, path: &str) -> Option<Request> {
    if source.len() > MAX_SOURCE
        || !source.trim_start().starts_with("<?php")
        || source.contains("<<<")
        || source.contains('`')
    {
        return None;
    }
    let code = crate::package_facts::code(source)?;
    let tokens = words(&code);
    let declarations = tokens
        .iter()
        .filter(|(start, end)| {
            let prefix = code[..*start].trim_end();
            let next = code[*end..].trim_start().as_bytes().first().copied();
            code[*start..*end].eq_ignore_ascii_case("class")
                && !["::", "->"].iter().any(|ending| prefix.ends_with(ending))
                && !code
                    .as_bytes()
                    .get(start.saturating_sub(1))
                    .is_some_and(|byte| matches!(byte, b'$' | b'\\'))
                && next.is_some_and(|byte| byte_word(byte) || byte == b'{')
        })
        .collect::<Vec<_>>();
    let [(class_at, class_end)] = declarations.as_slice() else {
        return None;
    };
    let after = code[*class_end..].trim_start();
    let name_end = after.bytes().take_while(|byte| byte_word(*byte)).count();
    let name = &after[..name_end];
    if name.is_empty() || name.as_bytes()[0].is_ascii_digit() {
        return None;
    }
    let open = code[*class_at..].find('{').map(|n| *class_at + n)?;
    // Namespace/import identity belongs to the proven class header. Valid
    // methods or members named `class` must not become extra declarations.
    let header_source = format!("{}{{}}", &source[..open]);
    let full = crate::eloquent_members::source_class_name(&header_source, name)?;
    let close = php::closing(&code, open)?;
    let header = code[*class_at..open].split_whitespace().collect::<Vec<_>>();
    let parent = header
        .iter()
        .position(|word| word.eq_ignore_ascii_case("extends"))
        .and_then(|n| header.get(n + 1))
        .and_then(|parent| crate::eloquent_members::source_class_name(&header_source, parent))
        .unwrap_or_default();
    let mut blocked = false;
    let mut rules = Rules::Inherit;
    let mut seen_rules = false;
    let mut validated_override = false;
    let mut cursor = open + 1;
    let mut nesting = 0i32;
    for (start, end) in &tokens {
        if *start <= open || *start >= close {
            continue;
        }
        for byte in code[cursor..*start].bytes() {
            match byte {
                b'{' => nesting += 1,
                b'}' => nesting -= 1,
                _ => {}
            }
        }
        cursor = *end;
        if nesting != 0 {
            continue;
        }
        let word = &code[*start..*end];
        if word.eq_ignore_ascii_case("use")
            && code.as_bytes().get(start.saturating_sub(1)) != Some(&b'$')
        {
            blocked = true;
        }
        if !word.eq_ignore_ascii_case("function") {
            continue;
        }
        let tail = code[*end..].trim_start();
        let tail = tail.strip_prefix('&').unwrap_or(tail).trim_start();
        let len = tail.bytes().take_while(|byte| byte_word(*byte)).count();
        if tail[..len].eq_ignore_ascii_case("validated") {
            validated_override = true;
        }
        if !tail[..len].eq_ignore_ascii_case("rules") {
            continue;
        }
        if seen_rules {
            blocked = true;
            continue;
        }
        seen_rules = true;
        rules = Rules::Unknown;
        let rest = tail[len..].trim_start();
        if !rest.starts_with('(') {
            continue;
        }
        let params = code.len() - rest.len();
        let Some(params_end) = php::closing(&code, params) else {
            continue;
        };
        let Some(body) = code[params_end + 1..close]
            .find(['{', ';'])
            .map(|n| params_end + 1 + n)
            .filter(|at| code.as_bytes()[*at] == b'{')
        else {
            continue;
        };
        let Some(body_end) = php::closing(&code, body) else {
            continue;
        };
        if let Some(fields) = literal_rules(source, &code, body + 1..body_end) {
            rules = Rules::Literal(fields);
        }
    }
    Some(Request {
        full,
        parent,
        path: path.into(),
        rules,
        blocked,
        validated_override,
        stamp: crate::artisan_metadata::hash(source),
    })
}
/// Replace the facts for a path, including removing obsolete facts when a file
/// no longer declares a supported single class.
pub(crate) fn cache(requests: &mut Requests, source: &str, path: &str) {
    requests.remove(path);
    if let Some(request) = parse(source, path) {
        requests.insert(path.into(), request);
    }
}
pub(crate) fn current_class(doc: &Document) -> Option<String> {
    parse(&doc.text, &doc.path).map(|request| request.full)
}
/// Prove the framework validated() provider through a known FormRequest chain.
/// Custom validated() implementations and unresolved traits require separate
/// return-contract proof; literal/dynamic rules do not change that method.
pub(crate) fn validated_request(
    requests: &Requests,
    doc: &Document,
    class: &str,
    owns: &dyn Fn(&str) -> bool,
) -> bool {
    validated_dependencies(requests, doc, class, owns).is_some()
}
pub(crate) fn dependency_path(dependency: &Request) -> &str {
    &dependency.path
}
pub(crate) fn unchanged(dependency: &Request, source: &str) -> bool {
    crate::artisan_metadata::hash(source) == dependency.stamp
        && parse(source, &dependency.path).as_ref() == Some(dependency)
}
pub(crate) fn validated_dependencies(
    requests: &Requests,
    doc: &Document,
    class: &str,
    owns: &dyn Fn(&str) -> bool,
) -> Option<Vec<Request>> {
    if !owns(&doc.path) {
        return None;
    }
    let current = parse(&doc.text, &doc.path);
    let mut full = class.trim_start_matches('\\').to_owned();
    let mut visited = BTreeSet::new();
    let mut dependencies = Vec::new();
    for _ in 0..16 {
        if full.eq_ignore_ascii_case(BASE) {
            return Some(dependencies);
        }
        if !visited.insert(full.to_ascii_lowercase()) {
            return None;
        }
        let mut candidates = requests
            .values()
            .filter(|unit| unit.path != doc.path)
            .chain(current.iter())
            .filter(|unit| unit.full.eq_ignore_ascii_case(&full));
        let Some(unit) = candidates.next() else {
            return None;
        };
        if candidates.next().is_some()
            || !owns(&unit.path)
            || unit.blocked
            || unit.validated_override
            || unit.parent.is_empty()
        {
            return None;
        }
        dependencies.push(unit.clone());
        full = unit.parent.clone();
    }
    None
}
/// A known empty rules array returns Some(empty), preventing a child override
/// from leaking inherited fields. Missing/dynamic ancestry never falls back.
pub(crate) fn fields(
    requests: &Requests,
    doc: &Document,
    class: &str,
    owns: &dyn Fn(&str) -> bool,
) -> Option<BTreeSet<String>> {
    if !owns(&doc.path) {
        return None;
    }
    let current = parse(&doc.text, &doc.path);
    let mut full = class.trim_start_matches('\\').to_owned();
    let mut visited = BTreeSet::new();
    let mut selected = None;
    for _ in 0..16 {
        if full.eq_ignore_ascii_case(BASE) {
            return Some(selected.unwrap_or_default());
        }
        if !visited.insert(full.to_ascii_lowercase()) {
            return None;
        }
        let mut candidates = requests
            .values()
            .filter(|unit| unit.path != doc.path)
            .chain(current.iter())
            .filter(|unit| unit.full.eq_ignore_ascii_case(&full));
        let unit = candidates.next()?;
        if candidates.next().is_some() || !owns(&unit.path) || unit.blocked {
            return None;
        }
        if selected.is_none() {
            match &unit.rules {
                Rules::Literal(fields) => selected = Some(fields.clone()),
                Rules::Unknown => return None,
                Rules::Inherit => {}
            }
        }
        if unit.parent.is_empty() {
            return None;
        }
        full = unit.parent.clone();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn doc(path: &str, text: &str) -> Document {
        Document {
            path: path.into(),
            language: "php".into(),
            version: 1,
            text: text.into(),
        }
    }
    fn request(class: &str, parent: &str, rules: &str) -> String {
        format!(
            "<?php\nnamespace App\\Http\\Requests;\nclass {class} extends {parent}\n{{\n{rules}\n}}\n"
        )
    }
    fn base() -> String {
        request(
            "BaseRequest",
            "\\Illuminate\\Foundation\\Http\\FormRequest",
            "public function rules(): array {return ['email'=>'required|email','profile.名'=>'string'];}",
        )
    }
    #[test]
    fn cache_inherits_real_form_request_alias_chains_and_retains_unicode_keys() {
        let mut requests = Requests::new();
        cache(
            &mut requests,
            &base(),
            "/project/app/Http/Requests/BaseRequest.php",
        );
        let child = "<?php namespace App\\Http\\Requests; use App\\Http\\Requests\\BaseRequest as ParentRequest; class UpdateRequest extends ParentRequest {}";
        let doc = doc("/project/app/Http/Requests/UpdateRequest.php", child);
        let fields = fields(
            &requests,
            &doc,
            "App\\Http\\Requests\\UpdateRequest",
            &|_| true,
        )
        .unwrap();
        assert_eq!(
            fields.into_iter().collect::<Vec<_>>(),
            ["email", "profile.名"]
        );
    }
    #[test]
    fn class_constants_variables_and_member_names_do_not_manufacture_type_declarations() {
        let source = request(
            "BaseRequest",
            "\\Illuminate\\Foundation\\Http\\FormRequest",
            "public function rules(): array {return ['name'=>[Rule::enum(Name::class )]];} public $use = true; public function helper() {$class = Name::class; return $this->class;} public function class() {return 1;}",
        );
        let doc = doc("/project/BaseRequest.php", &source);
        assert_eq!(
            fields(
                &Requests::new(),
                &doc,
                "App\\Http\\Requests\\BaseRequest",
                &|_| true
            )
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>(),
            ["name"]
        );
    }

    #[test]
    fn literal_and_empty_child_overrides_replace_rather_than_merge_parent_fields() {
        let mut requests = Requests::new();
        cache(
            &mut requests,
            &base(),
            "/project/app/Http/Requests/BaseRequest.php",
        );
        for (body, expected) in [
            (
                "public function rules(): array {return ['name'=>'string'];}",
                vec!["name"],
            ),
            ("public function rules(): array {return [];}", vec![]),
        ] {
            let source = request("UpdateRequest", "BaseRequest", body);
            let d = doc("/project/app/Http/Requests/UpdateRequest.php", &source);
            assert_eq!(
                fields(&requests, &d, "App\\Http\\Requests\\UpdateRequest", &|_| {
                    true
                })
                .unwrap()
                .into_iter()
                .collect::<Vec<_>>(),
                expected
            );
        }
    }
    #[test]
    fn dynamic_mutated_spread_trait_and_missing_ancestry_never_leak_parent_fields() {
        let mut requests = Requests::new();
        cache(
            &mut requests,
            &base(),
            "/project/app/Http/Requests/BaseRequest.php",
        );
        for body in [
            "public function rules(): array {$rules=['name'=>'string']; unset($rules['name']); return $rules;}",
            "public function rules(): array {return array_merge(parent::rules(),['name'=>'string']);}",
            "public function rules(): array {return [...parent::rules(),'name'=>'string'];}",
            "use RequestRules;",
            "public function rules(): array {if ($this->has('x')) {return ['name'=>'string'];} return ['email'=>'email'];}",
        ] {
            let source = request("UpdateRequest", "BaseRequest", body);
            let d = doc("/project/app/Http/Requests/UpdateRequest.php", &source);
            assert!(
                fields(&requests, &d, "App\\Http\\Requests\\UpdateRequest", &|_| {
                    true
                })
                .is_none(),
                "{body}"
            );
        }
        for parent in [
            "MissingRequest",
            "\\Vendor\\FormRequest",
            "\\App\\FormRequest",
        ] {
            let source = request(
                "UpdateRequest",
                parent,
                "public function rules(): array {return ['name'=>'string'];}",
            );
            let d = doc("/project/app/Http/Requests/UpdateRequest.php", &source);
            assert!(
                fields(&requests, &d, "App\\Http\\Requests\\UpdateRequest", &|_| {
                    true
                })
                .is_none()
            );
        }
    }
    #[test]
    fn duplicate_case_identity_foreign_ancestor_and_current_buffer_replacements_are_authoritative()
    {
        let mut requests = Requests::new();
        let path = "/project/app/Http/Requests/BaseRequest.php";
        cache(&mut requests, &base(), path);
        let source = request("UpdateRequest", "BaseRequest", "");
        let d = doc("/project/app/Http/Requests/UpdateRequest.php", &source);
        assert!(
            fields(&requests, &d, "app\\http\\requests\\updaterequest", &|p| p
                != path)
            .is_none()
        );
        cache(
            &mut requests,
            &base().replace("BaseRequest", "BASEREQUEST"),
            "/project/app/Other.php",
        );
        assert!(
            fields(&requests, &d, "App\\Http\\Requests\\UpdateRequest", &|_| {
                true
            })
            .is_none()
        );
        requests.remove("/project/app/Other.php");
        let changed = doc(
            path,
            &request(
                "BaseRequest",
                "\\Illuminate\\Foundation\\Http\\FormRequest",
                "public function rules(): array {return ['changed'=>'string'];}",
            ),
        );
        assert_eq!(
            fields(
                &requests,
                &changed,
                "App\\Http\\Requests\\BaseRequest",
                &|_| true
            )
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>(),
            ["changed"]
        );
        let changed = doc(path, "<?php class Unrelated {}");
        assert!(
            fields(
                &requests,
                &changed,
                "App\\Http\\Requests\\UpdateRequest",
                &|_| true
            )
            .is_none()
        );
        cache(&mut requests, "<?php // removed", path);
        assert!(requests.is_empty());
    }
}
