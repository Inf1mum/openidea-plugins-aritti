//! Validation parameters and fields proved by literal rules in the current
//! document. No application code runs and no external request shape is guessed.
use crate::{
    Index, php,
    refs::{self, Kind},
};
use openidea_plugin::{
    Position, Range,
    providers::{CompletionItem, CompletionKind, Document},
    text,
};
use std::{collections::BTreeSet, ops::Range as Bytes};
const MAX_SOURCE: usize = 256 * 1024;
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'*' | b'-') || b >= 128
        })
}
pub(crate) fn form_request(source: &str, code: &str) -> bool {
    let mut classes = code.match_indices("class ").filter(|(at, _)| {
        *at == 0 || !php::is_identifier(code.as_bytes()[at - 1]) && code.as_bytes()[at - 1] != b':'
    });
    let Some((at, _)) = classes.next() else {
        return false;
    };
    if classes.next().is_some() {
        return false;
    }
    let Some(open) = code[at..].find('{').map(|n| at + n) else {
        return false;
    };
    let words = code[at..open].split_whitespace().collect::<Vec<_>>();
    words
        .iter()
        .position(|w| *w == "extends")
        .and_then(|n| words.get(n + 1))
        .and_then(|name| crate::eloquent_members::source_class_name(source, name))
        .is_some_and(|name| name.eq_ignore_ascii_case("Illuminate\\Foundation\\Http\\FormRequest"))
}
fn methods(code: &str, name: &str) -> Vec<Bytes<usize>> {
    let mut result = Vec::new();
    for (at, _) in code.match_indices("function") {
        if at > 0 && php::is_identifier(code.as_bytes()[at - 1]) {
            continue;
        }
        let tail = &code[at + 8..];
        if !tail.as_bytes().first().is_some_and(u8::is_ascii_whitespace) {
            continue;
        }
        let tail = tail.trim_start();
        let len = tail.bytes().take_while(|b| php::is_identifier(*b)).count();
        if !tail[..len].eq_ignore_ascii_case(name) {
            continue;
        }
        let rest = tail[len..].trim_start();
        if !rest.starts_with('(') {
            continue;
        }
        let params = code.len() - rest.len();
        let Some(end) = php::closing(code, params) else {
            continue;
        };
        let Some(open) = code[end + 1..]
            .find(['{', ';'])
            .map(|n| end + 1 + n)
            .filter(|n| code.as_bytes()[*n] == b'{')
        else {
            continue;
        };
        if let Some(close) = php::closing(code, open) {
            result.push(open + 1..close);
        }
    }
    result
}
fn quoted(source: &str, at: usize) -> Option<(Bytes<usize>, char)> {
    let masked = php::mask_comments(source);
    let bytes = masked.as_bytes();
    let mut from = 0;
    while from < bytes.len() {
        if matches!(bytes[from], b'\'' | b'"') {
            let end = php::string_end(bytes, from);
            if from < at && at <= end.saturating_sub(1) {
                let close = if bytes.get(end.saturating_sub(1)) == Some(&bytes[from]) {
                    end - 1
                } else {
                    end
                };
                let value = &source[from + 1..close];
                if value.contains(['\\', '$', '\n', '\r']) {
                    return None;
                }
                return Some((from + 1..close, bytes[from] as char));
            }
            from = end.max(from + 1);
        } else {
            from += 1;
        }
    }
    None
}
fn validation_binding(source: &str, at: usize) -> bool {
    let Some((_, site)) = refs::literal_site_at("php", source, at) else {
        return false;
    };
    if let Some((class, _)) = site.callee.rsplit_once("::") {
        return if class == "Validator" {
            php::insert_use(source, "Illuminate\\Support\\Facades\\Validator").is_some()
        } else {
            crate::view_rename::facade(source, class, "Validator")
        };
    }
    if site.callee == "validator" {
        let Some(open) = source[site.callee_start + site.callee.len()..]
            .find('(')
            .map(|n| site.callee_start + site.callee.len() + n)
        else {
            return false;
        };
        let identity = crate::eloquent_members::metadata_inference::call_identity(source, open);
        if identity.is_some_and(|identity| {
            identity.class.is_some()
                || identity
                    .function_fqn
                    .as_deref()
                    .is_some_and(|name| name != "validator")
        }) {
            return false;
        }
    }
    if site.callee.is_empty() {
        return crate::package_facts::code(source).is_some_and(|code| form_request(source, &code));
    }
    true
}
fn rule_fields(source: &str) -> BTreeSet<String> {
    let mut fields = BTreeSet::new();
    let Some(code) = crate::package_facts::code(source) else {
        return fields;
    };
    if form_request(source, &code) {
        for body in methods(&code, "rules") {
            let trimmed = code[body.clone()].trim_start();
            let Some(tail) = trimmed.strip_prefix("return") else {
                continue;
            };
            if !tail.as_bytes().first().is_some_and(u8::is_ascii_whitespace) {
                continue;
            }
            let begin = body.end - tail.trim_start().len();
            let Some(open) = php::array_open(source, begin..body.end) else {
                continue;
            };
            let Some(close) = php::closing(source, open) else {
                continue;
            };
            if code[close + 1..body.end].trim() != ";" {
                continue;
            }
            for item in php::items(source, open) {
                if let Some((field, _)) = item
                    .key
                    .and_then(|range| php::literal(source, range))
                    .filter(|(field, _)| identifier(field))
                {
                    fields.insert(field.to_owned());
                }
            }
        }
    }
    for (open, _) in code.match_indices('[').take(256) {
        if let Some(close) = php::closing(source, open) {
            for item in php::items(source, open) {
                let Some(key) = item.key else {
                    continue;
                };
                let Some((field, _)) = php::literal(source, key) else {
                    continue;
                };
                if !identifier(field) {
                    continue;
                }
                let (rule, start) = if let Some(value) = php::literal(source, item.value.clone()) {
                    value
                } else {
                    let Some(array) = php::array_open(source, item.value) else {
                        continue;
                    };
                    let Some(first) = php::items(source, array).into_iter().next() else {
                        continue;
                    };
                    let Some(value) = php::literal(source, first.value) else {
                        continue;
                    };
                    value
                };
                if start < close
                    && validation_binding(source, start + rule.len().min(1))
                    && refs::reference_at("php", source, start + rule.len().min(1))
                        .is_some_and(|reference| reference.kind == Kind::Rule)
                {
                    fields.insert(field.to_owned());
                }
            }
        }
    }
    fields
}
fn items(
    names: impl IntoIterator<Item = String>,
    source: &str,
    range: Bytes<usize>,
    detail: &str,
    cursor: usize,
) -> Vec<CompletionItem> {
    let replace = Range::new(
        text::position(source, range.start),
        text::position(source, range.end),
    );
    let prefix = source[range.start..cursor.min(range.end)].to_lowercase();
    names
        .into_iter()
        .filter(|name| name.to_lowercase().starts_with(&prefix))
        .filter(|name| {
            !name.contains(['\'', '"', '\\', '$', '|', ',']) && !name.chars().any(char::is_control)
        })
        .take(512)
        .map(|name| {
            let mut item = CompletionItem::new(&name, CompletionKind::Field);
            item.replace = Some(replace);
            item.detail = Some(detail.into());
            item
        })
        .collect()
}
fn parameter(
    index: &Index,
    doc: &Document,
    at: usize,
    range: Bytes<usize>,
    fields: &BTreeSet<String>,
) -> Vec<CompletionItem> {
    if !validation_binding(&doc.text, at) {
        return vec![];
    }
    if !refs::literal_at("php", &doc.text, at).is_some_and(|(kind, _)| kind == Kind::Rule)
        && refs::rule_list_at("php", &doc.text, at).is_none()
    {
        return vec![];
    }
    let source = &doc.text;
    let relative = at.saturating_sub(range.start).min(range.len());
    let value = &source[range.clone()];
    if value.contains("regex:") {
        return vec![];
    }
    let segment_start = value[..relative].rfind('|').map_or(0, |n| n + 1);
    let Some(colon) = value[segment_start..relative]
        .find(':')
        .map(|n| segment_start + n)
    else {
        return vec![];
    };
    let rule = &value[segment_start..colon];
    let params = &value[colon + 1..relative];
    let argument = params.bytes().filter(|b| *b == b',').count();
    let begin = range.start + colon + 1 + params.rfind(',').map_or(0, |n| n + 1);
    let end = at
        + source[at..range.end]
            .find([',', '|'])
            .unwrap_or(range.end - at);
    let replace = begin..end;
    let names: Vec<String> = match (rule, argument) {
        ("exists" | "unique", 0) => index.columns.keys().cloned().collect(),
        ("exists" | "unique", argument)
            if argument == 1
                || (rule == "unique" && argument == 3)
                || rule == "exists" && argument >= 2 && argument % 2 == 0
                || rule == "unique" && argument >= 4 && argument % 2 == 0 =>
        {
            params
                .split(',')
                .next()
                .and_then(|table| index.columns.get(table))
                .map(|columns| columns.keys().cloned().collect())
                .unwrap_or_default()
        }
        (
            "same" | "different" | "confirmed" | "required_if" | "required_unless"
            | "prohibited_if" | "prohibited_unless" | "exclude_if" | "exclude_unless"
            | "accepted_if" | "declined_if" | "after" | "after_or_equal" | "before"
            | "before_or_equal" | "date_equals" | "gt" | "gte" | "lt" | "lte" | "in_array",
            0,
        ) => fields.iter().cloned().collect(),
        (
            "required_with"
            | "required_with_all"
            | "required_without"
            | "required_without_all"
            | "prohibits"
            | "present_with"
            | "present_with_all",
            _,
        ) => fields.iter().cloned().collect(),
        ("date_format", 0) => ["Y-m-d", "Y-m-d H:i:s", "H:i", "H:i:s", "d/m/Y", "m/d/Y"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        _ => vec![],
    };
    items(
        names,
        source,
        replace,
        &crate::ui_text::t("validation.rule-parameter", "Validation rule parameter"),
        at,
    )
}
pub(crate) fn completion(index: &Index, doc: &Document, at: Position) -> Vec<CompletionItem> {
    let project = crate::project_for(&doc.path);
    completion_checked(index, doc, at, &|path| {
        project
            .as_ref()
            .is_some_and(|project| crate::owns_current_path(project, path))
    })
}
/// All external request facts come from owned cached snapshots; receiver types
/// come from the shared PHP binding resolver and the current editor buffer.
pub(crate) fn completion_checked(
    index: &Index,
    doc: &Document,
    at: Position,
    owns: &dyn Fn(&str) -> bool,
) -> Vec<CompletionItem> {
    if doc.language != "php" || index.partial || doc.text.len() > MAX_SOURCE {
        return vec![];
    }
    let Some(at) = text::offset(&doc.text, at) else {
        return vec![];
    };
    let Some(code) = crate::package_facts::code(&doc.text) else {
        return vec![];
    };
    let current = crate::validation_requests::current_class(doc);
    let self_fields = current.as_deref().and_then(|class| {
        crate::validation_requests::fields(&index.request_fields, doc, class, owns)
    });
    let fields = self_fields
        .clone()
        .unwrap_or_else(|| rule_fields(&doc.text));
    if let Some((range, _)) = quoted(&doc.text, at) {
        let parameters = parameter(index, doc, at, range.clone(), &fields);
        if !parameters.is_empty() {
            return parameters;
        }
        // A controller-bound request can expose known field keys without any
        // filename heuristic or assumption about a variable named $request.
        if let Some((_, site)) = refs::literal_site_at("php", &doc.text, at) {
            let method = site.callee.rsplit("->").next().unwrap_or_default();
            if site.callee.contains("->")
                && [
                    "input",
                    "get",
                    "has",
                    "hasAny",
                    "hasAll",
                    "filled",
                    "missing",
                    "only",
                    "except",
                    "validated",
                ]
                .iter()
                .any(|name| method.eq_ignore_ascii_case(name))
                && site.arg == 0
                && site.depth <= 1
                && site.key.is_none()
            {
                let open = doc.text[site.callee_start + site.callee.len()..]
                    .find('(')
                    .map(|n| site.callee_start + site.callee.len() + n);
                let receiver = open.and_then(|open| {
                    crate::eloquent_members::semantic::receiver_class(index, doc, open, owns)
                });
                let request_fields = receiver.as_deref().and_then(|class| {
                    crate::validation_requests::fields(&index.request_fields, doc, class, owns)
                });
                if let Some(request_fields) = request_fields {
                    return items(
                        request_fields,
                        &doc.text,
                        range,
                        &crate::ui_text::t(
                            "validation.request-field",
                            "FormRequest validation field",
                        ),
                        at,
                    );
                }
            }
        }
        if self_fields.is_some() {
            for method in ["messages", "attributes"] {
                if methods(&code, method)
                    .iter()
                    .any(|body| body.contains(&range.start))
                    && doc.text[range.end + 1..].trim_start().starts_with("=>")
                {
                    let names = fields
                        .iter()
                        .flat_map(|field| {
                            if method == "messages" {
                                index
                                    .rules
                                    .keys()
                                    .map(|rule| format!("{field}.{rule}"))
                                    .collect::<Vec<_>>()
                            } else {
                                vec![field.clone()]
                            }
                        })
                        .collect::<Vec<_>>();
                    return items(
                        names,
                        &doc.text,
                        range,
                        &crate::ui_text::t(
                            "validation.request-field",
                            "FormRequest validation field",
                        ),
                        at,
                    );
                }
            }
            let before = code[..range.start - 1].trim_end();
            if [
                "$this->input(",
                "$this->get(",
                "$this->has(",
                "$this->filled(",
                "$this->missing(",
            ]
            .iter()
            .any(|call| before.ends_with(call))
            {
                return items(
                    fields.into_iter(),
                    &doc.text,
                    range,
                    &crate::ui_text::t("validation.request-field", "FormRequest validation field"),
                    at,
                );
            }
        }
    }
    let property_class = crate::eloquent_members::semantic::receiver_class_checked(
        index,
        doc,
        text::position(&doc.text, at),
        owns,
    );
    let property_fields = property_class.as_deref().and_then(|class| {
        crate::validation_requests::fields(&index.request_fields, doc, class, owns)
    });
    if property_fields.is_some() || self_fields.is_some() {
        let start = code[..at]
            .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c as u32 >= 128))
            .map_or(0, |n| n + 1);
        if property_fields.is_some() || code[..start].trim_end().ends_with("$this->") {
            let end = at
                + code[at..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c as u32 >= 128)
                    .map(char::len_utf8)
                    .sum::<usize>();
            let names = property_fields
                .or(self_fields)
                .unwrap_or_default()
                .into_iter()
                .filter(|field| {
                    !field.contains(['.', '*', '-'])
                        && field.as_bytes().first().is_some_and(|byte| {
                            byte.is_ascii_alphabetic() || *byte == b'_' || *byte >= 128
                        })
                });
            return items(
                names,
                &doc.text,
                start..end,
                &crate::ui_text::t("validation.request-property", "FormRequest input property"),
                at,
            );
        }
    }
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{AddName, Names};
    fn request(body: &str) -> String {
        format!(
            "<?php\nnamespace App\\Http\\Requests;\nuse Illuminate\\Foundation\\Http\\FormRequest as BaseRequest;\nclass UpdateRequest extends BaseRequest\n{{\npublic function rules(): array {{return ['email'=>'required|email','password'=>'required','password_confirmation'=>'same:password','tags.*'=>'string'];}}\n{body}\n}}\n"
        )
    }
    fn index() -> Index {
        let mut index = Index::default();
        for rule in ["required", "email", "same"] {
            index.rules.add(rule, None, "", Position::new(0, 0), 0);
        }
        let mut columns = Names::new();
        for column in ["id", "email"] {
            columns.add(column, None, "", Position::new(0, 0), 0);
        }
        index.columns.insert("users".into(), columns);
        index
    }
    fn complete(marked: &str) -> Vec<CompletionItem> {
        let _host = openidea_plugin::mock::MockHost::install();
        let at = marked.find('^').unwrap();
        let source = marked.replacen('^', "", 1);
        let doc = Document {
            path: "/project/app/Http/Requests/UpdateRequest.php".into(),
            language: "php".into(),
            version: 1,
            text: source,
        };
        completion(&index(), &doc, text::position(&doc.text, at))
    }
    fn replace(marked: &str, item: &CompletionItem) -> String {
        let source = marked.replacen('^', "", 1);
        let range = item.replace.unwrap();
        let mut rendered = source.clone();
        rendered.replace_range(
            text::offset(&source, range.start).unwrap()..text::offset(&source, range.end).unwrap(),
            &item.label,
        );
        rendered
    }
    #[test]
    fn database_rule_parameters_replace_only_current_parameter_and_keep_rule_prefixes() {
        let source = "<?php $request->validate(['email'=>'required|unique:users,em^ail,NULL,id']);";
        let items = complete(source);
        assert!(items.iter().any(|item| item.label == "email"));
        assert!(items.iter().all(|item| item.label != "users"));
        let rendered = replace(
            source,
            items.iter().find(|item| item.label == "email").unwrap(),
        );
        assert!(rendered.contains("required|unique:users,email,NULL,id"));
        let items =
            complete("<?php $request->validate(['email'=>['required','exists:us^ers,email']]);");
        assert_eq!(
            items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            ["users"]
        );
    }
    #[test]
    fn cross_field_rules_complete_fields_without_erasing_existing_arguments() {
        let source = "<?php $request->validate(['email'=>'email','password'=>'required','other'=>'required_with:email,pass^word']);";
        let items = complete(source);
        assert!(items.iter().any(|item| item.label == "password"));
        assert!(
            replace(
                source,
                items.iter().find(|i| i.label == "password").unwrap()
            )
            .contains("required_with:email,password")
        );
        assert!(
            complete("<?php $request->validate(['email'=>'required_if:email,va^lue']);").is_empty()
        );
    }
    #[test]
    fn form_request_messages_attributes_and_properties_use_only_current_rule_fields() {
        let marked =
            request("public function messages(): array {return ['em^ail.required'=>'Message'];}");
        let items = complete(&marked);
        assert!(items.iter().any(|item| item.label == "email.required"));
        let marked =
            request("public function attributes(): array {return ['pass^word'=>'Password'];}");
        assert!(
            complete(&marked)
                .iter()
                .any(|item| item.label == "password")
        );
        let marked = request("public function consume() {return $this->em^ail;}").replacen(
            "required|email",
            "required,email",
            1,
        );
        let items = complete(&marked);
        assert!(items.iter().any(|item| item.label == "email"));
        assert!(items.iter().all(|item| item.label != "tags.*"));
        let marked = request("public function consume() {return $this->input('em^ail');}");
        assert!(complete(&marked).iter().any(|item| item.label == "email"));
    }
    #[test]
    fn literal_form_request_keys_work_with_object_rules_without_inventing_value_types() {
        let marked = "<?php namespace App\\Http\\Requests; class StoreRequest extends \\Illuminate\\Foundation\\Http\\FormRequest { public function rules(): array { return ['nickname' => [\\Illuminate\\Validation\\Rule::in(['a','b'])]]; } public function value() {return $this->input('nick^name');} }";
        assert!(complete(marked).iter().any(|item| item.label == "nickname"));
    }

    #[test]
    fn unrelated_classes_strings_comments_and_ambiguous_shapes_do_not_invent_request_fields() {
        for parent in [
            "App\\FormRequest",
            "Vendor\\Request",
            "Illuminate\\Http\\Request",
        ] {
            let marked = request("public function consume() {return $this->em^ail;}")
                .replace("Illuminate\\Foundation\\Http\\FormRequest", parent);
            assert!(complete(&marked).is_empty(), "{parent}");
        }
        assert!(complete("<?php $data=['email'=>'exists:us^ers,email'];").is_empty());
        assert!(
            complete(
                "<?php use Vendor\\Validator; Validator::make([],['email'=>'exists:us^ers,email']);"
            )
            .is_empty()
        );
        assert!(complete("<?php use function Vendor\\validator; validator([],['email'=>'exists:us^ers,email']);").is_empty());
        assert!(
            complete("<?php // $request->validate(['email'=>'exists:us^ers,email']);").is_empty()
        );
        assert!(complete("<?php $request->validate(['email'=>'regex:/a:us^ers/']);").is_empty());
        assert!(
            complete("<?php $request->validate(['email'=>'regex:/a|unique:us^ers/']);").is_empty()
        );
    }
    #[test]
    fn database_identifiers_needing_php_or_rule_escaping_are_withheld() {
        let _host = openidea_plugin::mock::MockHost::install();
        let marked = "<?php $request->validate(['email'=>'exists:^,email']);";
        let at = marked.find('^').unwrap();
        let source = marked.replacen('^', "", 1);
        let doc = Document {
            path: "/project/app/Request.php".into(),
            language: "php".into(),
            version: 1,
            text: source,
        };
        let mut index = index();
        for table in [
            "bad'quote",
            "bad\\escape",
            "bad$dollar",
            "bad,comma",
            "bad|pipe",
        ] {
            index.columns.insert(table.into(), Names::new());
        }
        let items = completion(&index, &doc, text::position(&doc.text, at));
        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            ["users"]
        );
    }

    #[test]
    fn utf16_ranges_preserve_unicode_and_unsupported_database_parameters_stay_quiet() {
        let source = "<?php $request->validate(['😀'=>'required','email'=>'exists:users,em^ail']);";
        let items = complete(source);
        assert!(
            replace(source, items.iter().find(|i| i.label == "email").unwrap())
                .contains("'😀'=>'required','email'=>'exists:users,email'")
        );
        assert!(
            complete("<?php $request->validate(['email'=>'exists:unknown,em^ail']);").is_empty()
        );
        assert!(
            complete("<?php $request->validate(['email'=>'unique:users,email,1,i^d']);")
                .iter()
                .any(|item| item.label == "id")
        );
        assert!(
            complete("<?php $request->validate(['email'=>'unique:users,email,i^d']);").is_empty()
        );
    }
}

#[cfg(test)]
mod cached_request_tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn cached(index: &mut Index, path: &str, source: &str) {
        crate::validation_requests::cache(&mut index.request_fields, source, path);
        crate::eloquent_members::cache(&mut index.eloquent_models, source, path);
        crate::eloquent_members::cache_object(&mut index.eloquent_objects, source, path);
    }
    fn index() -> Index {
        let mut index = Index::default();
        cached(
            &mut index,
            "/project/app/Requests/BaseRequest.php",
            "<?php namespace App\\Requests; class BaseRequest extends \\Illuminate\\Foundation\\Http\\FormRequest {public function rules(): array {return ['email'=>'email','profile.name'=>'string','名'=>'string'];}}",
        );
        cached(
            &mut index,
            "/project/app/Requests/StoreRequest.php",
            "<?php namespace App\\Requests; class StoreRequest extends BaseRequest {}",
        );
        index
    }
    fn controller(body: &str) -> String {
        format!(
            "<?php\nnamespace App\\Http\\Controllers;\nuse App\\Requests\\StoreRequest as Payload;\nclass StoreController\n{{\n public function store(Payload $request) {{ {body} }}\n}}\n"
        )
    }
    fn complete(
        index: &Index,
        marked: &str,
        owns: &dyn Fn(&str) -> bool,
    ) -> (Document, Vec<CompletionItem>) {
        let at = marked.find('^').unwrap();
        let source = marked.replacen('^', "", 1);
        let doc = Document {
            path: "/project/app/Http/Controllers/StoreController.php".into(),
            language: "php".into(),
            version: 1,
            text: source,
        };
        let items = completion_checked(index, &doc, text::position(&doc.text, at), owns);
        (doc, items)
    }
    #[test]
    fn controller_imported_typed_request_fields_and_input_keys_follow_cached_custom_base() {
        let _host = MockHost::install();
        let index = index();
        for body in [
            "return $request->em^ail;",
            "return $request->input('em^ail');",
            "return $request->validated('em^ail');",
            "return $request->only(['em^ail']);",
        ] {
            let source = controller(body);
            let (_, items) = complete(&index, &source, &|_| true);
            assert!(items.iter().any(|item| item.label == "email"), "{body}");
        }
        let (_, items) = complete(&index, &controller("return $request->profile^;"), &|_| true);
        assert!(items.iter().all(|item| item.label != "profile.name"));
    }
    #[test]
    fn registered_completion_uses_indexed_request_facts_and_rechecks_nested_project_ownership() {
        use openidea_plugin::providers::Providers;
        let host = MockHost::install();
        host.add_file("app/Requests/BaseRequest.php","<?php namespace App\\Requests; class BaseRequest extends \\Illuminate\\Foundation\\Http\\FormRequest {public function rules(): array {return ['email'=>'email'];}}");
        host.add_file(
            "app/Requests/StoreRequest.php",
            "<?php namespace App\\Requests; class StoreRequest extends BaseRequest {}",
        );
        let mut plugin = crate::tests::activated(&host);
        // A real application declares its production source namespace; the
        // generic binding resolver deliberately requires that source proof.
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        plugin.index_all();crate::tests::finish_indexing(&mut plugin);
        let marked = controller("/* 😀 */ return $request->input('em^ail');");
        let at = marked.find('^').unwrap();
        let doc = Document {
            path: "/project/app/Http/Controllers/StoreController.php".into(),
            language: "php".into(),
            version: 4,
            text: marked.replacen('^', "", 1),
        };
        let position = text::position(&doc.text, at);
        let items = plugin.completion(&doc, position, None);
        let item = items.iter().find(|item| item.label == "email").unwrap();
        let range = item.replace.unwrap();
        assert_eq!(
            &doc.text[text::offset(&doc.text, range.start).unwrap()
                ..text::offset(&doc.text, range.end).unwrap()],
            "email"
        );
        host.add_project("/project/app/Requests");
        assert!(
            plugin
                .completion(&doc, position, None)
                .iter()
                .all(|item| item.label != "email")
        );
        assert!(host.process_runs().is_empty());
    }

    #[test]
    fn controller_mutated_untyped_shadowed_and_foreign_bindings_never_guess_request_fields() {
        let _host = MockHost::install();
        let index = index();
        for body in [
            "$request=$unknown; return $request->em^ail;",
            "unset($request); return $request->input('em^ail');",
            "return function($request) {return $request->em^ail;};",
        ] {
            let source = controller(body);
            let (_, items) = complete(&index, &source, &|_| true);
            assert!(items.is_empty(), "{body}");
        }
        let source = controller("return $request->em^ail;").replace("Payload $request", "$request");
        assert!(complete(&index, &source, &|_| true).1.is_empty());
        let source = controller("return $request->em^ail;");
        assert!(
            complete(&index, &source, &|path| !path.ends_with("BaseRequest.php"))
                .1
                .is_empty()
        );
    }
    #[test]
    fn current_request_overlay_inheritance_and_empty_dynamic_overrides_are_authoritative() {
        let _host = MockHost::install();
        let index = index();
        for (body, expected) in [
            ("", true),
            ("public function rules(): array {return [];}", false),
            (
                "public function rules(): array {$rules=['email'=>'email'];unset($rules['email']);return $rules;}",
                false,
            ),
        ] {
            let marked = format!(
                "<?php namespace App\\Requests; class StoreRequest extends BaseRequest {{{body} public function value() {{return $this->em^ail;}} }}"
            );
            let at = marked.find('^').unwrap();
            let doc = Document {
                path: "/project/app/Requests/StoreRequest.php".into(),
                language: "php".into(),
                version: 3,
                text: marked.replacen('^', "", 1),
            };
            let items = completion_checked(&index, &doc, text::position(&doc.text, at), &|_| true);
            assert_eq!(
                items.iter().any(|item| item.label == "email"),
                expected,
                "{body}"
            );
        }
    }
    #[test]
    fn cached_controller_field_ranges_roundtrip_unicode_without_reading_sources_or_executing_php() {
        let host = MockHost::install();
        let index = index();
        let source = controller("/* 😀 */ return $request->input('名^');");
        let (doc, items) = complete(&index, &source, &|_| true);
        let item = items.iter().find(|item| item.label == "名").unwrap();
        let range = item.replace.unwrap();
        assert_eq!(
            &doc.text[text::offset(&doc.text, range.start).unwrap()
                ..text::offset(&doc.text, range.end).unwrap()],
            "名"
        );
        assert!(host.process_runs().is_empty());
        assert_eq!(host.fetches(), 0);
    }
}
