//! Rewrites of the string or echo under the caret, as byte-range edits.

use std::ops::Range;

use crate::php::{self, is_identifier};
use crate::refs::{self, Kind};

#[derive(Clone)]
pub struct Rewrite {
    pub title: String,
    pub kind: &'static str,
    pub edits: Vec<(Range<usize>, String)>,
}

const FACADES: &[&str] = &[
    "App",
    "Artisan",
    "Auth",
    "Blade",
    "Broadcast",
    "Bus",
    "Cache",
    "Config",
    "Context",
    "Cookie",
    "Crypt",
    "Date",
    "DB",
    "Event",
    "File",
    "Gate",
    "Hash",
    "Http",
    "Lang",
    "Log",
    "Mail",
    "Notification",
    "Password",
    "Process",
    "Queue",
    "RateLimiter",
    "Redirect",
    "Request",
    "Response",
    "Route",
    "Schema",
    "Session",
    "Storage",
    "URL",
    "Validator",
    "View",
    "Vite",
];

const SUPPORT_HELPERS: &[&str] = &["Str", "Arr", "Number", "Js", "Uri"];

/// `Rule::array` is opt-in: unlike the other builders it is absent from older
/// Laravel installations. The caller must establish its availability from the
/// installed framework or a captured version, never from a loose Composer range.
pub struct FrameworkFeatures {
    pub rule_array: bool,
    pub scope_attribute: bool,
    pub scope_model: bool,
    pub blade_class: bool,
}

pub fn rewrites_with_framework_features(
    path: &str,
    language: &str,
    text: &str,
    at: usize,
    controllers_namespace: &str,
    features: FrameworkFeatures,
) -> Vec<Rewrite> {
    let mut found = Vec::new();

    if path.ends_with(".blade.php") {
        found.extend(echo_toggle(text, at));
        if features.blade_class {
            found.extend(class_directive(text, at));
            found.extend(wrap_class_directive(text, at));
        }
    }

    if let Some((kind, literal)) = refs::literal_at(language, text, at) {
        match kind {
            Kind::Rule => {
                found.extend(rules_array(text, literal.clone()));
                found.extend(rules_all_arrays(text, literal));
            }
            Kind::Action => {
                found.extend(controller_array(text, literal, controllers_namespace));
                found.extend(controller_calls_all(
                    language,
                    text,
                    at,
                    controllers_namespace,
                ));
            }
            Kind::Controller => {
                found.extend(controller_class(text, literal, controllers_namespace));
                found.extend(controller_calls_all(
                    language,
                    text,
                    at,
                    controllers_namespace,
                ));
            }
            _ => {}
        }
    }

    if language == "php" {
        found.extend(rule_object(language, text, at, features.rule_array));
        found.extend(root_facade(text, at));
        found.extend(crate::class_actions::rewrites_with_scope_identity(
            text,
            at,
            features.scope_attribute,
            features.scope_model,
        ));
    }

    if let Some(open) = refs::rule_list_at(language, text, at) {
        found.extend(rules_string(text, open));
    }

    found
}

fn rules_string(text: &str, open: usize) -> Option<Rewrite> {
    let close = php::closing(text, open)?;
    if text.as_bytes()[open] != b'['
        || text[open..close].contains("//")
        || text[open..close].contains("/*")
    {
        return None;
    }

    let mut rules = Vec::new();
    for item in php::items(text, open) {
        let (rule, _) = php::literal(text, item.value).filter(|_| item.key.is_none())?;
        let is_regex = rule.starts_with("regex:") || rule.starts_with("not_regex:");
        if rule.is_empty()
            || rule.contains(['|', '\'', '"', '\\', '$'])
            || is_regex && rule.contains(',')
        {
            return None;
        }
        rules.push(rule);
    }

    let quote = text[open + 1..close].trim_start().chars().next()?;
    Some(Rewrite {
        title: "Convert validation rules to a string".into(),
        kind: "refactor.rewrite",
        edits: vec![(
            open..close + 1,
            format!("{quote}{}{quote}", rules.join("|")),
        )],
    })
}

fn array_edit(text: &str, literal: Range<usize>) -> Option<(Range<usize>, String)> {
    let contents = &text[literal.clone()];
    let quote = text[..literal.start].chars().next_back()?;
    if contents.contains("regex:")
        || !contents.contains('|')
        || contents.contains(['\'', '"', '\\', '$'])
    {
        return None;
    }

    let items: Vec<_> = contents.split('|').collect();
    if items.iter().any(|item| item.trim().is_empty()) {
        return None;
    }

    let array = items
        .iter()
        .map(|item| format!("{quote}{item}{quote}"))
        .collect::<Vec<_>>()
        .join(", ");
    Some((literal.start - 1..literal.end + 1, format!("[{array}]")))
}

fn rules_array(text: &str, literal: Range<usize>) -> Option<Rewrite> {
    Some(Rewrite {
        title: "Convert validation rules to an array".into(),
        kind: "refactor.rewrite",
        edits: vec![array_edit(text, literal)?],
    })
}

fn enclosing_bracket(text: &str, before: usize) -> Option<usize> {
    let masked = php::mask_comments(&text[..before]);
    let bytes = masked.as_bytes();
    let mut open = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'\'' | b'"' => {
                i = php::string_end(bytes, i).max(i + 1);
                continue;
            }
            b'(' | b'[' | b'{' => open.push(i),
            b')' | b']' | b'}' => {
                open.pop();
            }
            _ => {}
        }
        i += 1;
    }

    open.pop()
}

fn rules_all_arrays(text: &str, literal: Range<usize>) -> Option<Rewrite> {
    let open = enclosing_bracket(text, literal.start)?;
    if text.as_bytes()[open] != b'[' {
        return None;
    }

    let edits: Vec<_> = php::items(text, open)
        .into_iter()
        .filter_map(|item| {
            let (contents, start) = php::literal(text, item.value)?;
            array_edit(text, start..start + contents.len())
        })
        .collect();

    if edits.len() < 2 {
        return None;
    }

    Some(Rewrite {
        title: "Convert all validation rules in this array to arrays".into(),
        kind: "refactor.rewrite",
        edits,
    })
}

/// Decode only literal PHP class/method text. Unknown backslash escapes remain
/// literal in PHP; recognized double-quoted escapes and interpolation are not
/// class names and must never be guessed from their source spelling.
pub(crate) fn controller_literal(contents: &str, quote: char) -> Option<String> {
    let mut decoded = String::new();
    let mut chars = contents.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '$' && quote == '"' {
            return None;
        }
        if ch == '\\' {
            let next = *chars.peek()?;
            if next == '\\' {
                chars.next();
                decoded.push('\\');
            } else if next == quote {
                return None;
            } else if quote == '"'
                && matches!(
                    next,
                    'n' | 'r' | 't' | 'v' | 'e' | 'f' | 'x' | 'u' | '$' | '0'..='7'
                )
            {
                return None;
            } else {
                decoded.push('\\');
            }
        } else {
            decoded.push(ch);
        }
    }
    Some(decoded)
}

/// PHP namespaces/imports cannot establish an extra runtime namespace applied
/// by a route group. Keep that scope unchanged until its binding is proven.
fn controller_namespace_safe(text: &str) -> bool {
    let source = php::mask_comments(text);
    let compact = source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    ![
        "->namespace(",
        "::namespace(",
        "'namespace'=>",
        "\"namespace\"=>",
    ]
    .iter()
    .any(|pattern| compact.contains(pattern))
}

/// A user class named Route/URL/Redirect is not a framework action-string API.
/// Preserve offsets while hiding only imports whose source binding is foreign.
pub(crate) fn controller_binding_safe(text: &str, literal: &Range<usize>) -> bool {
    let Some(code) = crate::package_facts::code(text) else {
        return false;
    };
    // Helper calls have a separate import namespace. A local/imported action
    // function must not receive a Laravel-specific array transformation.
    let mut stack = Vec::new();
    for (at, byte) in code.bytes().enumerate().take(literal.start) {
        match byte {
            b'(' | b'[' | b'{' => stack.push((byte, at)),
            b')' | b']' | b'}' => {
                stack.pop();
            }
            _ => {}
        }
    }
    if let Some((_, open)) = stack.into_iter().rev().find(|(byte, _)| *byte == b'(') {
        let before = code[..open].trim_end();
        let start = before
            .rfind(|c: char| {
                !(c.is_alphanumeric() || matches!(c, '_' | '\\' | ':' | '>' | '-' | '$'))
            })
            .map_or(0, |at| at + 1);
        let raw = &before[start..];
        let helper = raw.trim_start_matches('\\');
        if ["action", "link_to_action"].contains(&helper) {
            let folded = code.to_ascii_lowercase();
            let local = folded.match_indices("function").any(|(at, _)| {
                if at > 0 && is_identifier(folded.as_bytes()[at - 1]) {
                    return false;
                }
                let tail = &folded[at + 8..];
                if !tail.as_bytes().first().is_some_and(u8::is_ascii_whitespace) {
                    return false;
                }
                let tail = tail.trim_start().trim_start_matches('&').trim_start();
                tail.strip_prefix(helper)
                    .is_some_and(|rest| rest.trim_start().starts_with('('))
            });
            if local {
                return false;
            }
            if !raw.starts_with('\\') {
                let identity =
                    crate::eloquent_members::metadata_inference::call_identity(text, open);
                if identity.is_some_and(|identity| {
                    identity.class.is_some()
                        || identity
                            .function_fqn
                            .as_deref()
                            .is_some_and(|name| !name.eq_ignore_ascii_case(helper))
                }) {
                    return false;
                }
            }
        }
    }
    let mut source = php::mask_comments(text).into_bytes();
    for expected in ["Route", "URL", "Redirect"] {
        // Resolve each conventional binding once per source, not per route.
        let conventional =
            php::insert_use(text, &format!("Illuminate\\Support\\Facades\\{expected}")).is_some();
        let mut qualified = std::collections::BTreeMap::new();
        for (at, _) in code.match_indices(&format!("{expected}::")) {
            let start = code[..at]
                .rfind(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '\\')))
                .map_or(0, |at| at + 1);
            let raw = &code[start..at + expected.len()];
            let framework = if raw == expected {
                // Keep conventional unimported facade aliases; an explicit
                // foreign import or local declaration still blocks the name.
                conventional
            } else {
                *qualified
                    .entry(raw)
                    .or_insert_with(|| crate::view_rename::facade(text, raw, expected))
            };
            if !framework {
                source[at..at + expected.len()].fill(b' ');
            }
        }
    }
    let Ok(source) = String::from_utf8(source) else {
        return false;
    };
    refs::literal_at("php", &source, literal.start).is_some_and(|(kind, range)| {
        matches!(kind, Kind::Action | Kind::Controller | Kind::Method) && range == *literal
    })
}

fn controller_edit(
    text: &str,
    kind: Kind,
    literal: Range<usize>,
    namespace: &str,
) -> Option<(Range<usize>, String)> {
    if text.len() > 256 << 10
        || !controller_namespace_safe(text)
        || !controller_binding_safe(text, &literal)
    {
        return None;
    }
    let quote = text[..literal.start].chars().next_back()?;
    if !matches!(quote, '\'' | '"') || text[literal.end..].chars().next() != Some(quote) {
        return None;
    }
    let contents = controller_literal(&text[literal.clone()], quote)?;
    let (class, method) = match contents.split_once('@') {
        Some((class, method))
            if kind == Kind::Action
                && !method.is_empty()
                && method.bytes().all(|b| is_identifier(b) || b >= 128)
                && !method.as_bytes()[0].is_ascii_digit() =>
        {
            (class, Some(method))
        }
        Some(_) => return None,
        None => (contents.as_str(), None),
    };
    let reference = php::controller_reference(text, class, namespace)?;
    let replacement = match method {
        Some(method) => format!("[{reference}::class, {quote}{method}{quote}]"),
        None => format!("{reference}::class"),
    };
    Some((literal.start - 1..literal.end + 1, replacement))
}

fn controller_array(text: &str, literal: Range<usize>, namespace: &str) -> Option<Rewrite> {
    Some(Rewrite {
        title: if text[literal.clone()].contains('@') {
            "Convert controller string to a class array"
        } else {
            "Convert controller string to ::class"
        }
        .into(),
        kind: "refactor.rewrite",
        edits: vec![controller_edit(text, Kind::Action, literal, namespace)?],
    })
}

fn controller_class(text: &str, literal: Range<usize>, namespace: &str) -> Option<Rewrite> {
    Some(Rewrite {
        title: "Convert controller string to ::class".into(),
        kind: "refactor.rewrite",
        edits: vec![controller_edit(text, Kind::Controller, literal, namespace)?],
    })
}

fn controller_calls_all(language: &str, text: &str, at: usize, namespace: &str) -> Option<Rewrite> {
    if text.len() > 256 << 10 {
        return None;
    }
    let candidates: Vec<_> = refs::references(language, text)
        .into_iter()
        .filter(|reference| {
            reference.closed && matches!(reference.kind, Kind::Action | Kind::Controller)
        })
        .take(129)
        .collect();
    if candidates.len() > 128 {
        return None;
    }
    let converted: Vec<_> = candidates
        .into_iter()
        .filter_map(|reference| {
            controller_edit(
                text,
                reference.kind,
                reference.start..reference.end,
                namespace,
            )
        })
        .collect();

    if converted.len() < 2
        || !converted
            .iter()
            .any(|(range, _)| range.start <= at && at <= range.end)
    {
        return None;
    }

    Some(Rewrite {
        title: "Convert all controller strings in this file".into(),
        kind: "refactor.rewrite",
        edits: converted,
    })
}

/// Mounted namespaces are runtime routing scopes, distinct from PHP imports.
/// Reuse the parser's exact selected action identity before creating PHP code.
pub(crate) fn contextual_controllers(index:&crate::Index,doc:&openidea_plugin::providers::Document,at:usize,namespace:&str)->Option<Vec<Rewrite>>{
    let context=index.route_mounts.get(&doc.path).cloned().unwrap_or_default();
    if context.namespace.is_empty()&&controller_namespace_safe(&doc.text)&&!index.route_mount_unknown.contains(&doc.path){return None;}
    if index.route_mount_unknown.contains(&doc.path)||doc.text.len()>128*1024{return Some(vec![]);}
    let mut candidates=refs::references("php",&doc.text).into_iter().filter(|reference|reference.closed&&matches!(reference.kind,Kind::Action|Kind::Controller)).collect::<Vec<_>>();
    if let Some(reference)=crate::metadata::route_reference(&index.metadata,&doc.text,at){if reference.closed&&!candidates.iter().any(|candidate|candidate.start==reference.start&&candidate.end==reference.end){candidates.push(reference);}}
    if !crate::view_rename::binding_budget(doc.text.len(),candidates.len()){return Some(vec![]);}
    let mut edits=Vec::new();let mut selected=None;
    for reference in candidates{
        let literal=reference.start..reference.end;
        let edit=(||{if let Some((full,member))=crate::routes::controller_at(&doc.text,&index.metadata.route_methods,&context,reference.start){
            let quote=doc.text.as_bytes().get(reference.start.checked_sub(1)?).copied()? as char;
            let raw=controller_literal(doc.text.get(literal.clone())?,quote)?;
            let method=if reference.kind==Kind::Action{raw.split_once('@').map(|(_,method)|method)}else{None};
            if method.is_none()&&(!controller_namespace_safe(&doc.text)||!context.namespace.is_empty()&&!full.starts_with(&context.namespace)){return None;}
            let class=php::controller_reference(&doc.text,&format!("\\{full}"),namespace)?;
            let replacement=if method.is_some(){format!("[{class}::class, {quote}{member}{quote}]")}else{format!("{class}::class")};
            Some((reference.start-1..reference.end+1,replacement))
        }else{
            let helper=refs::literal_site_at("php",&doc.text,reference.start).is_some_and(|(_,site)|matches!(site.callee.trim_start_matches('\\'),"action"|"link_to_action"));
            if helper{controller_edit(&doc.text,reference.kind,literal,namespace)}else{None}
        }})();
        if let Some(edit)=edit{
            if reference.start<=at&&at<=reference.end{selected=Some((edit.clone(),reference.kind,doc.text[reference.start..reference.end].contains('@')));}
            edits.push(edit);
        }
    }
    let Some((edit,kind,method))=selected else{return Some(vec![]);};
    let title=if kind==Kind::Action&&method{"Convert controller string to a class array"}else{"Convert controller string to ::class"};
    let mut found=vec![Rewrite{title:title.into(),kind:"refactor.rewrite",edits:vec![edit]}];
    if edits.len()>1{found.push(Rewrite{title:"Convert all controller strings in this file".into(),kind:"refactor.rewrite",edits});}
    Some(found)
}
pub(crate) fn controller_rewrite(rewrite:&Rewrite)->bool{
    matches!(rewrite.title.as_str(),"Convert controller string to a class array"|"Convert controller string to ::class"|"Convert all controller strings in this file")
}

/// Localize only at the published action boundary; rewrite parsers remain pure.
pub(crate) fn localized_title(title:&str)->String{
    match title{
        "Add the missing columns to $fillable"=>return crate::ui_text::t("editor.final.missingFillable","Add the missing columns to $fillable"),
        "Convert validation rules to a string" => return crate::ui_text::t("editor.final.rulesString","Convert validation rules to a string"),
        "Convert validation rules to an array" => return crate::ui_text::t("editor.final.rulesArray","Convert validation rules to an array"),
        "Convert all validation rules in this array to arrays" => return crate::ui_text::t("editor.final.allRulesArrays","Convert all validation rules in this array to arrays"),
        "Convert controller string to a class array" => return crate::ui_text::t("editor.final.controllerArray","Convert controller string to a class array"),
        "Convert controller string to ::class" => return crate::ui_text::t("editor.final.controllerClass","Convert controller string to ::class"),
        "Convert all controller strings in this file" => return crate::ui_text::t("editor.final.allControllers","Convert all controller strings in this file"),
        "Use #[Scope] on all scope methods" => return crate::ui_text::t("editor.final.scopeAll","Use #[Scope] on all scope methods"),
        "Wrap class attribute in @class" => return crate::ui_text::t("editor.final.wrapDynamicClass","Wrap class attribute in @class"),
        "Convert class attribute to @class" => return crate::ui_text::t("editor.final.convertClass","Convert class attribute to @class"),
        "Wrap class attribute with @class" => return crate::ui_text::t("editor.final.wrapStaticClass","Wrap class attribute with @class"),
        "Output unescaped: {!! !!}" => return crate::ui_text::t("editor.final.unescaped","Output unescaped: {!! !!}"),
        "Output escaped: {{ }}" => return crate::ui_text::t("editor.final.escaped","Output escaped: {{ }}"),
        _=>{}
    }
    if let Some(call)=title.strip_prefix("Convert to "){return crate::ui_text::format("editor.final.convertBuilder","Convert to {call}",&[("call",call)]);}
    if let Some(class)=title.strip_prefix("Import "){return crate::ui_text::format("editor.final.importClass","Import {class}",&[("class",class)]);}
    if let Some(name)=title.strip_prefix("Use #[Scope] on ").and_then(|tail|tail.strip_suffix("()")){return crate::ui_text::format("editor.final.scope","Use #[Scope] on {name}()",&[("name",name)]);}
    if let Some(variable)=title.strip_prefix("Move ").and_then(|tail|tail.strip_suffix(" to a constructor property in all matching methods")){return crate::ui_text::format("editor.final.moveAll","Move {variable} to a constructor property in all matching methods",&[("variable",variable)]);}
    if let Some(variable)=title.strip_prefix("Move ").and_then(|tail|tail.strip_suffix(" to a constructor property")){return crate::ui_text::format("editor.final.move","Move {variable} to a constructor property",&[("variable",variable)]);}
    if let Some((class,method))=title.strip_prefix("Inject ").and_then(|tail|tail.split_once(" into the constructor for ")).and_then(|(class,method)|Some((class,method.strip_suffix("()")?))){return crate::ui_text::format("editor.final.injectConstructor","Inject {class} into the constructor for {method}()",&[("class",class),("method",method)]);}
    if let Some((class,method,facade))=title.strip_prefix("Inject ").and_then(|tail|tail.split_once(" into ")).and_then(|(class,tail)|{let(method,facade)=tail.split_once("() instead of the ")?;Some((class,method,facade.strip_suffix(" facade")?))}){return crate::ui_text::format("editor.final.injectMethod","Inject {class} into {method}() instead of the {facade} facade",&[("class",class),("method",method),("facade",facade)]);}
    if let Some((class,facade))=title.strip_prefix("Inject ").and_then(|tail|tail.split_once(" instead of the ")).and_then(|(class,facade)|Some((class,facade.strip_suffix(" facade")?))){return crate::ui_text::format("editor.final.injectFacade","Inject {class} instead of the {facade} facade",&[("class",class),("facade",facade)]);}
    title.to_owned()
}

fn rule_object(
    language: &str,
    text: &str,
    at: usize,
    supports_rule_array: bool,
) -> Option<Rewrite> {
    let open = refs::rule_list_at(language, text, at)?;
    let (contents, start) = php::items(text, open)
        .into_iter()
        .filter(|item| item.key.is_none())
        .filter_map(|item| php::literal(text, item.value))
        .find(|(contents, start)| *start <= at && at <= start + contents.len())?;

    if contents.contains(['|', '\'', '"', '\\', '$']) || contents.chars().any(char::is_whitespace) {
        return None;
    }
    let (name, arguments) = match contents.split_once(':') {
        Some((name, arguments)) if !arguments.is_empty() => {
            (name, arguments.split(',').collect::<Vec<_>>())
        }
        Some(_) => return None,
        None => (contents, Vec::new()),
    };
    if arguments.iter().any(|argument| argument.is_empty()) {
        return None;
    }

    if declares_class(&php::mask_comments(text), "Rule") {
        return None;
    }

    let import = php::insert_use(text, "Illuminate\\Validation\\Rule")?;
    let rule = &import.short_name;
    let quoted = |arguments: &[&str]| {
        arguments
            .iter()
            .map(|argument| format!("'{argument}'"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let call = match name {
        "exists" | "unique" if !arguments.is_empty() && arguments.len() <= 2 => {
            format!("{rule}::{name}({})", quoted(&arguments))
        }
        "in" if !arguments.is_empty() => format!("{rule}::in([{}])", quoted(&arguments)),
        "not_in" if !arguments.is_empty() => format!("{rule}::notIn([{}])", quoted(&arguments)),
        "array" if supports_rule_array => {
            if arguments.is_empty() {
                format!("{rule}::array()")
            } else {
                format!("{rule}::array([{}])", quoted(&arguments))
            }
        }
        "file" if arguments.is_empty() => format!("{rule}::file()"),
        "image" if arguments.is_empty() => format!("{rule}::imageFile()"),
        "image" if arguments.as_slice() == ["allow_svg"] => format!("{rule}::imageFile(true)"),
        "dimensions" if !arguments.is_empty() => dimensions_builder(rule, &arguments)?,
        _ => return None,
    };

    let mut edits = vec![(start - 1..start + contents.len() + 1, call.clone())];
    if !import.text.is_empty() {
        edits.push((import.offset..import.offset, import.text));
    }

    Some(Rewrite {
        title: format!("Convert to {call}"),
        kind: "refactor.rewrite",
        edits,
    })
}

/// Keep values as quoted literals. Dimensions casts them itself, including ratio
/// fractions, so this avoids changing `ratio=3/2` into PHP division or an integer.
fn dimensions_builder(rule: &str, arguments: &[&str]) -> Option<String> {
    let mut call = format!("{rule}::dimensions()");
    let mut constraints = Vec::new();
    let mut use_constructor = false;
    let mut seen = std::collections::BTreeSet::new();
    for argument in arguments {
        let (name, value) = argument.split_once('=')?;
        if !seen.insert(name) {
            return None;
        }
        let method = match name {
            "width" => "width",
            "height" => "height",
            "min_width" => "minWidth",
            "max_width" => "maxWidth",
            "min_height" => "minHeight",
            "max_height" => "maxHeight",
            "ratio" => "ratio",
            "min_ratio" => "minRatio",
            "max_ratio" => "maxRatio",
            _ => return None,
        };
        // Only numeric literal constraints have an unambiguous fluent equivalent.
        // The string spelling is retained; the framework performs its own coercion.
        let numeric = |part: &str| {
            !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_digit() || b == b'.')
                && part.bytes().filter(|b| *b == b'.').count() <= 1
                && part.bytes().any(|b| b.is_ascii_digit())
        };
        if name.ends_with("ratio") {
            match value.split_once('/') {
                Some((a, b))
                    if numeric(a)
                        && numeric(b)
                        && b.parse::<f64>().ok().is_some_and(|v| v > 0.0) => {}
                None if numeric(value) => {}
                _ => return None,
            }
        } else if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        // minRatio/maxRatio methods are absent in older framework releases;
        // the constructor preserves the exact string constraint on all versions.
        use_constructor |= matches!(name, "min_ratio" | "max_ratio");
        constraints.push(format!("'{name}' => '{value}'"));
        call.push_str(&format!("->{method}('{value}')"));
    }
    if use_constructor {
        Some(format!("{rule}::dimensions([{}])", constraints.join(", ")))
    } else {
        Some(call)
    }
}

fn declares_class(source: &str, name: &str) -> bool {
    php::declares_class(source, name)
}

fn root_facade(text: &str, at: usize) -> Option<Rewrite> {
    let masked = php::mask_comments(text);
    let bytes = masked.as_bytes();
    let at = at.min(bytes.len());
    let start = (0..at)
        .rev()
        .take_while(|&i| is_identifier(bytes[i]))
        .last()
        .unwrap_or(at);
    let end = (at..bytes.len())
        .take_while(|&i| is_identifier(bytes[i]))
        .last()
        .map_or(at, |i| i + 1);
    let name = &masked[start..end];

    if name.is_empty()
        || !masked[end..].starts_with("::")
        || start > 0 && matches!(bytes[start - 1], b'\\' | b'$' | b':')
    {
        return None;
    }

    let fqcn = if FACADES.contains(&name) {
        format!("Illuminate\\Support\\Facades\\{name}")
    } else if SUPPORT_HELPERS.contains(&name) {
        format!("Illuminate\\Support\\{name}")
    } else {
        return None;
    };

    let declares_namespace = masked
        .lines()
        .any(|line| line.trim_start().starts_with("namespace "));
    if !declares_namespace || declares_class(&masked, name) {
        return None;
    }

    let import = php::insert_use(text, &fqcn)?;
    if import.text.is_empty() {
        return None;
    }

    Some(Rewrite {
        title: format!("Import {fqcn}"),
        kind: "refactor.rewrite",
        edits: vec![(import.offset..import.offset, import.text)],
    })
}

fn class_directive(text: &str, at: usize) -> Option<Rewrite> {
    const OPENING: &str = "class=\"";
    let mut from = 0;

    while let Some(found) = text[from..].find(OPENING) {
        let attribute = from + found;
        let value_start = attribute + OPENING.len();
        from = value_start;

        if attribute > at {
            return None;
        }

        let value_end = value_start + text[value_start..].find('"')?;
        if at > value_end + 1 || !text[..attribute].ends_with(char::is_whitespace) {
            continue;
        }

        let tag_start = text[..attribute].rfind('<')?;
        if text[tag_start..attribute].contains('>') {
            return None;
        }

        let tag_end = value_end + text[value_end..].find('>').unwrap_or(0);
        if text[tag_start..tag_end].contains("@class") {
            return None;
        }

        let (items, dynamic) = class_items(&text[value_start..value_end])?;
        return Some(Rewrite {
            title: if dynamic {
                "Wrap class attribute in @class"
            } else {
                "Convert class attribute to @class"
            }
            .into(),
            kind: "refactor.rewrite",
            edits: vec![(
                attribute..value_end + 1,
                format!("@class([{}])", items.join(", ")),
            )],
        });
    }

    None
}

/// Keep the complete class list as one entry, as distinct from splitting it
/// into independently editable entries. Dynamic attributes use the existing
/// expression-aware conversion and do not get this static wrapper.
fn wrap_class_directive(text: &str, at: usize) -> Option<Rewrite> {
    let converted = class_directive(text, at)?;
    let range = converted.edits.first()?.0.clone();
    let value = text.get(range.start + "class=\"".len()..range.end - 1)?;
    let (items, dynamic) = class_items(value)?;
    if dynamic || items.len() < 2 {
        return None;
    }
    Some(Rewrite {
        title: "Wrap class attribute with @class".into(),
        kind: "refactor.rewrite",
        edits: vec![(range, format!("@class(['{value}'])"))],
    })
}

fn class_items(value: &str) -> Option<(Vec<String>, bool)> {
    if value.trim().is_empty()
        || value.contains(['\'', '\\', '@'])
        || value.contains("{!!")
        || value.contains("{{--")
    {
        return None;
    }

    let mut items = Vec::new();
    let mut dynamic = false;
    let mut rest = value.trim_start();

    while !rest.is_empty() {
        if let Some(echo) = rest.strip_prefix("{{") {
            let end = echo.find("}}")?;
            let expression = echo[..end].trim();
            rest = &echo[end + 2..];
            if expression.is_empty() || !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
                return None;
            }
            items.push(expression.to_owned());
            dynamic = true;
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let token = &rest[..end];
            if token.contains("{{") || token.contains("}}") {
                return None;
            }
            items.push(format!("'{token}'"));
            rest = &rest[end..];
        }
        rest = rest.trim_start();
    }

    Some((items, dynamic))
}

fn echo_toggle(text: &str, at: usize) -> Option<Rewrite> {
    let region = refs::blade_regions(text)
        .into_iter()
        .find(|r| r.start <= at && at <= r.end)?;
    let echo = &text[region.clone()];
    if text[..region.start].ends_with('@') {
        return None;
    }

    let (open, close, new_open, new_close, title) =
        if echo.starts_with("{{") && echo.ends_with("}}") {
            ("{{", "}}", "{!!", "!!}", "Output unescaped: {!! !!}")
        } else if echo.starts_with("{!!") && echo.ends_with("!!}") {
            ("{!!", "!!}", "{{", "}}", "Output escaped: {{ }}")
        } else {
            return None;
        };

    if echo.len() < open.len() + close.len() || echo.starts_with("{{--") {
        return None;
    }

    Some(Rewrite {
        title: title.into(),
        kind: "refactor.rewrite",
        edits: vec![
            (region.start..region.start + open.len(), new_open.into()),
            (region.end - close.len()..region.end, new_close.into()),
        ],
    })
}

#[cfg(test)]
mod builder_tests {
    use super::*;

    fn rewrite(rule: &str, array: bool) -> Option<String> {
        let text = format!("<?php\nnamespace App;\n$request->validate(['f' => ['{rule}']]);");
        let rewrite = rule_object("php", &text, text.find(rule).unwrap(), array)?;
        let mut edits = rewrite.edits;
        edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        let mut result = text;
        for (range, replacement) in edits {
            result.replace_range(range, &replacement);
        }
        Some(result)
    }

    #[test]
    fn completes_documented_rule_builders_without_losing_arguments() {
        for (rule, expected) in [
            ("array", "Rule::array()"),
            ("array:name,email", "Rule::array(['name', 'email'])"),
            ("file", "Rule::file()"),
            ("image", "Rule::imageFile()"),
            ("image:allow_svg", "Rule::imageFile(true)"),
            (
                "dimensions:width=100,height=200",
                "Rule::dimensions()->width('100')->height('200')",
            ),
            (
                "dimensions:min_width=12,max_height=34,ratio=3/2",
                "Rule::dimensions()->minWidth('12')->maxHeight('34')->ratio('3/2')",
            ),
            (
                "dimensions:min_ratio=0.5,max_ratio=2",
                "Rule::dimensions(['min_ratio' => '0.5', 'max_ratio' => '2'])",
            ),
        ] {
            let result = rewrite(rule, true).unwrap();
            assert!(result.contains(expected), "{result}");
            assert_eq!(
                result.matches("use Illuminate\\Validation\\Rule;").count(),
                1
            );
        }
    }

    #[test]
    fn array_factory_requires_framework_evidence() {
        assert!(rewrite("array", false).is_none());
        assert!(rewrite("array:a,b", false).is_none());
        assert!(rewrite("file", false).is_some());
    }

    #[test]
    fn malformed_or_unsupported_constraints_never_produce_a_partial_builder() {
        for rule in [
            "file:foo",
            "image:svg",
            "array:",
            "dimensions:",
            "dimensions:width=1,width=2",
            "dimensions:foo=1",
            "dimensions:ratio=1/0",
            "dimensions:width=1.5",
            "dimensions:width=-1",
            "dimensions:width=100,height=",
            "dimensions:ratio=1/2/3",
            "dimensions:width=1|file",
            "dimensions:width=1,height=2,",
            "dimensions:ratio=.",
        ] {
            assert!(rewrite(rule, true).is_none(), "{rule}");
        }
    }

    #[test]
    fn class_wrap_and_split_are_distinct_and_dynamic_content_is_not_quoted() {
        let text = "<div class=\"btn btn-primary\">";
        let action = wrap_class_directive(text, text.find("btn").unwrap()).unwrap();
        assert_eq!(action.edits[0].1, "@class(['btn btn-primary'])");
        assert_eq!(
            class_directive(text, text.find("btn").unwrap())
                .unwrap()
                .edits[0]
                .1,
            "@class(['btn', 'btn-primary'])"
        );
        for text in [
            "<div class=\"btn\">",
            "<div class=\"btn {{ $size }}\">",
            "<div class=\"btn {!! $raw !!}\">",
        ] {
            assert!(wrap_class_directive(text, text.find("btn").unwrap()).is_none());
        }
    }

    #[test]
    fn builder_reuses_an_aliased_rule_import_and_refuses_collisions() {
        let text = "<?php\nnamespace App;\nuse Illuminate\\Validation\\Rule as ValidationRule;\n$request->validate(['f' => ['file']]);";
        let action = rule_object("php", text, text.find("'file'").unwrap() + 1, false).unwrap();
        assert_eq!(action.edits.len(), 1);
        assert_eq!(action.edits[0].1, "ValidationRule::file()");
        let collision = text.replace(
            "Illuminate\\Validation\\Rule as ValidationRule",
            "App\\Rule",
        );
        assert!(
            rule_object(
                "php",
                &collision,
                collision.find("'file'").unwrap() + 1,
                false
            )
            .is_none()
        );
    }
}

#[cfg(test)]
mod capability_safety_tests {
    use super::*;
    #[test]
    fn blade_class_rewrites_require_explicit_framework_capability_but_echo_toggle_does_not() {
        let text = "<div class=\"btn btn-primary\">{{ $label }}</div>";
        let at = text.find("btn").unwrap() + 1;
        let features = |blade_class| FrameworkFeatures {
            rule_array: false,
            scope_attribute: false,
            scope_model: false,
            blade_class,
        };
        assert!(
            rewrites_with_framework_features(
                "/p/view.blade.php",
                "blade",
                text,
                at,
                "App\\Http\\Controllers",
                features(false)
            )
            .iter()
            .all(|a| !a.title.contains("class attribute"))
        );
        let enabled = rewrites_with_framework_features(
            "/p/view.blade.php",
            "blade",
            text,
            at,
            "App\\Http\\Controllers",
            features(true),
        );
        assert!(
            enabled
                .iter()
                .any(|a| a.title == "Convert class attribute to @class")
        );
        assert!(
            enabled
                .iter()
                .any(|a| a.title == "Wrap class attribute with @class")
        );
        let at = text.find("$label").unwrap() + 1;
        assert!(
            rewrites_with_framework_features(
                "/p/view.blade.php",
                "blade",
                text,
                at,
                "App\\Http\\Controllers",
                features(false)
            )
            .iter()
            .any(|a| a.title.starts_with("Output unescaped"))
        );
    }
    #[test]
    fn rule_and_facade_import_actions_refuse_case_variant_local_declarations() {
        for declaration in [
            "class rule {}",
            "CLASS RULE {}",
            "interface RuLe {}",
            "trait rule {}",
            "enum RULE {}",
        ] {
            let text = format!(
                "<?php\nnamespace App;\n{declaration}\n$request->validate(['f'=>['in:a,b']]);"
            );
            assert!(
                rule_object("php", &text, text.find("in:a,b").unwrap(), false).is_none(),
                "{text}"
            );
        }
        let text = "<?php\nnamespace App;\nclass auth {}\nAuth::user();";
        assert!(root_facade(text, text.find("Auth::user").unwrap() + 1).is_none());
        let text = "<?php\nnamespace App;\n$example='class Rule {}';\n$request->validate(['f'=>['in:a,b']]);";
        assert!(rule_object("php", text, text.find("in:a,b").unwrap(), false).is_some());
    }
}

#[cfg(test)]
mod controller_conversion_tests {
    use super::*;
    fn rewrites(text: &str, marker: &str) -> Vec<Rewrite> {
        rewrites_with_framework_features(
            "/project/routes/web.php",
            "php",
            text,
            text.find(marker).unwrap() + 1,
            "App\\Http\\Controllers",
            FrameworkFeatures {
                rule_array: false,
                scope_attribute: false,
                scope_model: false,
                blade_class: false,
            },
        )
    }
    fn apply(text: &str, rewrite: &Rewrite) -> String {
        let mut edits = rewrite.edits.clone();
        edits.sort_by_key(|(range, _)| std::cmp::Reverse((range.start, range.end)));
        let mut result = text.to_owned();
        for (range, replacement) in edits {
            result.replace_range(range, &replacement);
        }
        result
    }
    fn single(text: &str, marker: &str) -> String {
        let actions = rewrites(text, marker);
        let action = actions
            .iter()
            .find(|r| r.title.starts_with("Convert controller string"))
            .expect(text);
        apply(text, action)
    }
    #[test]
    fn explicit_namespaced_and_fully_qualified_controllers_keep_their_absolute_identity() {
        let text = r#"<?php
namespace Routes;
use App\Other\UserController;
Route::get('/users', 'App\Http\Controllers\UserController@index');
Route::get('/external', '\Vendor\Api\ExternalController@show');
"#;
        assert!(
            single(text, "App\\Http")
                .contains(r#"[\App\Http\Controllers\UserController::class, 'index']"#)
        );
        assert!(
            single(text, "Vendor").contains(r#"[\Vendor\Api\ExternalController::class, 'show']"#)
        );
        assert!(single(text, "Vendor").contains(r#"use App\Other\UserController;"#));
    }
    #[test]
    fn aliases_grouped_imports_namespace_prefixes_and_external_short_aliases_are_exact() {
        let text = r#"<?php
namespace Routes;
use App\Http\Controllers\{UserController as Users, InvokableController as Invoke};
use Vendor\Api as External;
use Vendor\Other\Controller as Outside;
Route::get('/users', 'App\Http\Controllers\UserController@index');
Route::get('/external', 'Vendor\Api\ReportController@show');
Route::get('/short', 'Outside@show');
"#;
        assert!(
            single(text, "App\\Http\\Controllers\\UserController@index")
                .contains("[Users::class, 'index']")
        );
        assert!(
            single(text, "Vendor\\Api\\ReportController@show")
                .contains(r#"[External\ReportController::class, 'show']"#)
        );
        let outside = single(text, "Outside@show");
        assert!(outside.contains("[Outside::class, 'show']"));
        assert!(!outside.contains("App\\Http\\Controllers\\Outside"));
    }
    #[test]
    fn invokable_and_resource_forms_use_class_constants_and_preserve_double_quotes_for_methods() {
        let text = r#"<?php
use App\Http\Controllers\InvokableController as Run;
Route::get('/invoke', 'App\Http\Controllers\InvokableController');
Route::get('/short', 'Run');
Route::post('/show', "Vendor\\Reports\\Controller@show");
Route::resource('reports', 'Vendor\Reports\Controller');
"#;
        assert!(
            single(text, "App\\Http\\Controllers\\InvokableController'")
                .contains("Route::get('/invoke', Run::class)")
        );
        assert!(single(text, "Run'").contains("Route::get('/short', Run::class)"));
        assert!(
            single(text, "Vendor\\\\Reports")
                .contains(r#"[\Vendor\Reports\Controller::class, "show"]"#)
        );
        assert!(
            single(text, "Vendor\\Reports\\Controller'")
                .contains(r#"Route::resource('reports', \Vendor\Reports\Controller::class)"#)
        );
    }
    #[test]
    fn all_conversion_has_disjoint_ranges_keeps_unknown_strings_and_original_imports() {
        let text = r#"<?php
use App\Http\Controllers\UserController as Users;
Route::get('/users', 'App\Http\Controllers\UserController@index');
Route::get('/invoke', '\Vendor\Invoke');
Route::resource('reports', 'Vendor\Reports\Controller');
Route::get('/unknown', 'Missing@index');
"#;
        let actions = rewrites(text, "App\\Http\\Controllers\\UserController@index");
        let all = actions
            .iter()
            .find(|r| r.title == "Convert all controller strings in this file")
            .unwrap();
        assert_eq!(all.edits.len(), 3);
        for (i, (range, _)) in all.edits.iter().enumerate() {
            for (other, _) in &all.edits[i + 1..] {
                assert!(range.end <= other.start || other.end <= range.start);
            }
        }
        let rendered = apply(text, all);
        assert!(rendered.contains("[Users::class, 'index']"));
        assert!(rendered.contains("Route::get('/invoke', \\Vendor\\Invoke::class)"));
        assert!(rendered.contains("Route::get('/unknown', 'Missing@index')"));
        assert!(rendered.contains("use App\\Http\\Controllers\\UserController as Users;"));
    }
    #[test]
    fn ambiguous_namespaces_literals_and_import_collisions_are_withheld_or_use_exact_fqn() {
        for text in [
            r#"<?php Route::get('/x', 'Missing@index');"#,
            r#"<?php namespace App\Http\Controllers; Route::get('/x', 'self@index');"#,
            r#"<?php use Vendor\Route; Route::get('/x', 'App\Controller@index');"#,
            r#"<?php use Vendor\URL; URL::action('App\Controller@index');"#,
            r#"<?php use function Vendor\action; action('App\Controller@index');"#,
            r#"<?php function action($value) {} action('App\Controller@index');"#,
            r#"<?php Route::namespace('Other')->get('/x', 'App\Controller@index');"#,
            r#"<?php Route::group(['namespace' => 'Other'], function(){Route::get('/x', 'App\Controller@index');});"#,
            r#"<?php Route::get('/x', "App\reports\Controller@index");"#,
            r#"<?php Route::get('/x', "App\\$controller@index");"#,
            r#"<?php Route::get('/x', 'App\Controller@bad-method');"#,
            r#"<?php namespace A { Route::get('/x', 'App\Controller@index'); }"#,
            r#"<?php namespace A; namespace B; Route::get('/x', 'App\Controller@index');"#,
        ] {
            assert!(
                rewrites(text, &text[text.rfind('@').unwrap()..])
                    .iter()
                    .all(|r| !r.title.starts_with("Convert controller")),
                "{text}"
            );
        }
        let text = r#"<?php
use App\Http\Controllers\UserController as Users;
class users {}
Route::get('/users', 'App\Http\Controllers\UserController@index');
"#;
        assert!(
            single(text, "App\\Http\\Controllers\\UserController@index")
                .contains(r#"[\App\Http\Controllers\UserController::class, 'index']"#)
        );
    }
    #[test]
    fn oversized_bulk_candidate_sets_keep_single_conversion_available() {
        let mut text = String::from("<?php\n");
        for i in 0..129 {
            text.push_str(&format!("Route::get('/{i}', 'Vendor\\Controller@show');\n"));
        }
        let actions = rewrites(&text, "Vendor");
        assert!(
            actions
                .iter()
                .any(|r| r.title == "Convert controller string to a class array")
        );
        assert!(
            actions
                .iter()
                .all(|r| r.title != "Convert all controller strings in this file")
        );
    }

    #[test]
    fn registered_controller_action_ranges_roundtrip_utf16_with_unicode_before_literal() {
        use openidea_plugin::{Range, mock::MockHost, providers::Providers, text};
        let host = MockHost::install();
        let mut plugin = crate::tests::activated(&host);
        let text = "<?php\n// 😀 accents é\nRoute::get('/😀', 'Vendor\\Api\\Controller@show');\n";
        let position = text::position(text, text.find("Vendor").unwrap() + 2);
        let doc = crate::tests::doc("/project/routes/web.php", "php", text);
        let action = plugin
            .code_actions(&doc, Range::new(position, position))
            .into_iter()
            .find(|a| a.title == "Convert controller string to a class array")
            .unwrap();
        assert_eq!(action.edits.len(), 1);
        assert_eq!(action.edits[0].0, doc.path);
        let edit = &action.edits[0].1[0];
        let start = text::offset(text, edit.range.start).unwrap();
        let end = text::offset(text, edit.range.end).unwrap();
        assert_eq!(&text[start..end], "'Vendor\\Api\\Controller@show'");
        let mut rendered = text.to_owned();
        rendered.replace_range(start..end, &edit.text);
        assert_eq!(
            rendered,
            "<?php\n// 😀 accents é\nRoute::get('/😀', [\\Vendor\\Api\\Controller::class, 'show']);\n"
        );
    }
    #[test]
    #[ignore = "requires php executable; lints actual controller rewrite output"]
    fn actual_controller_edits_lint_with_php() {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let text = r#"<?php
namespace Routes;
use App\Http\Controllers\{UserController as Users, InvokeController as Run};
Route::get('/users', 'App\Http\Controllers\UserController@index');
Route::get('/invoke', 'App\Http\Controllers\InvokeController');
Route::resource('reports', 'Vendor\Reports\Controller');
Route::get('/quoted', "Vendor\\Reports\\Controller@show");
"#;
        let actions = rewrites(text, "App\\Http\\Controllers\\UserController@index");
        for action in actions {
            let rendered = apply(text, &action);
            let mut child = Command::new("php")
                .arg("-l")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(rendered.as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{rendered}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[cfg(test)]
mod mounted_controller_rewrite_tests {
    use super::*;
    use openidea_plugin::providers::Document;
    fn setup(source:&str)->(crate::Index,Document){
        let mut index=crate::Index::default();
        index.route_mounts.insert("/project/custom/admin.php".into(),crate::routes::MountContext{namespace:"App\\Http\\Controllers\\Admin".into(),..Default::default()});
        (index,Document{path:"/project/custom/admin.php".into(),language:"php".into(),version:1,text:source.into()})
    }
    fn apply(source:&str,action:&Rewrite)->String{let mut result=source.to_owned();for(range,replacement)in action.edits.iter().rev(){result.replace_range(range.clone(),replacement);}result}
    #[test]
    fn mounted_namespace_conversion_preserves_aliases_quotes_and_bulk_action_identity(){
        let source=r#"<?php /* 😀 */ use Illuminate\Support\Facades\Route; use App\Http\Controllers\Admin\PostController as Posts; Route::get('/posts','PostController@show'); Route::get('/outside',"\\Vendor\\Outside@show");"#;
        let(index,doc)=setup(source);let at=source.find("PostController@show").unwrap();
        let actions=contextual_controllers(&index,&doc,at,"App\\Http\\Controllers").unwrap();
        assert_eq!(actions.len(),2);let single=apply(source,&actions[0]);
        assert!(single.contains("[Posts::class, 'show']"));assert!(!single.contains("[\\App\\Http\\Controllers\\PostController"));
        let all=apply(source,&actions[1]);assert!(all.contains("[\\Vendor\\Outside::class, \"show\"]"));
        assert!(all.contains("/* 😀 */"));
    }
    #[test]
    fn invokable_and_resource_class_literals_never_drop_runtime_namespace_escape(){
        for call in [r#"Route::get('/x','\Vendor\Outside');"#,r#"Route::resource('posts','\Vendor\Outside');"#]{
            let source=format!("<?php {call}");let(index,doc)=setup(&source);
            assert!(contextual_controllers(&index,&doc,source.find("Vendor").unwrap(),"App\\Http\\Controllers").unwrap().is_empty());
        }
        let source="<?php Route::get('/x','PostController');";let(index,doc)=setup(source);
        let actions=contextual_controllers(&index,&doc,source.find("PostController").unwrap(),"App\\Http\\Controllers").unwrap();
        assert!(apply(source,&actions[0]).contains("\\App\\Http\\Controllers\\Admin\\PostController::class"));
        let mut index=index;index.route_mount_unknown.insert(doc.path.clone());
        assert!(contextual_controllers(&index,&doc,source.find("PostController").unwrap(),"App\\Http\\Controllers").unwrap().is_empty());
    }
    #[test]
    #[ignore="requires LARAVEL_ROUTE_FIXTURE vendor autoload and php; compares actual rewritten route controller identity"]
    fn actual_mounted_controller_rewrite_preserves_laravel_runtime_actions(){
        use std::io::Write;
        let fixture=std::env::var("LARAVEL_ROUTE_FIXTURE").expect("set LARAVEL_ROUTE_FIXTURE");
        let source=r#"<?php use Illuminate\Support\Facades\Route; use App\Http\Controllers\Admin\PostController as Posts; Route::get('/posts','PostController@show'); Route::get('/invoke','PostController'); Route::get('/outside','\Vendor\Outside@show');"#;
        let(index,doc)=setup(source);let actions=contextual_controllers(&index,&doc,source.find("PostController@show").unwrap(),"App\\Http\\Controllers").unwrap();
        let after=apply(source,actions.last().unwrap());
        let script=r#"<?php
require getenv('LARAVEL_ROUTE_FIXTURE').'/vendor/autoload.php';
eval('namespace App\\Http\\Controllers\\Admin; class PostController { public function show(){} public function __invoke(){} }');
eval('namespace Vendor; class Outside { public function show(){} }');
$input=json_decode(stream_get_contents(STDIN),true,512,JSON_THROW_ON_ERROR);$out=[];
foreach($input as $source){$container=new Illuminate\Container\Container; $router=new Illuminate\Routing\Router(new Illuminate\Events\Dispatcher($container),$container);$container->instance('router',$router);Illuminate\Support\Facades\Facade::setFacadeApplication($container);Illuminate\Support\Facades\Facade::clearResolvedInstances();$router->group(['namespace'=>'App\\Http\\Controllers\\Admin'],function()use($source){eval(substr($source,5));});$rows=[];foreach($router->getRoutes() as $route){$rows[]=[$route->uri(),get_class($route->getController()),$route->getActionMethod()];}$out[]=$rows;}
echo json_encode($out,JSON_THROW_ON_ERROR);
"#;
        let directory=std::env::temp_dir().join(format!("laravel-mounted-controller-{}",std::process::id()));std::fs::create_dir_all(&directory).unwrap();let file=directory.join("compare.php");std::fs::write(&file,script).unwrap();
        let mut child=std::process::Command::new("php").arg(&file).env("LARAVEL_ROUTE_FIXTURE",fixture).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(serde_json::to_string(&[source,&after]).unwrap().as_bytes()).unwrap();let output=child.wait_with_output().unwrap();let _=std::fs::remove_dir_all(directory);
        assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));let rows:serde_json::Value=serde_json::from_slice(&output.stdout).unwrap();assert_eq!(rows[0],rows[1],"{after}");
    }
}

#[cfg(test)]
mod action_title_delivery_tests {
    #[test]
    fn localized_dynamic_titles_preserve_php_builders_fqns_variables_and_method_identifiers(){
        let host=openidea_plugin::mock::MockHost::install();host.set_setting("uiLanguage",r#""ja""#);
        for (authored,protected) in [
            ("Convert to Rule::date()->format('Y-m-d')","Rule::date()->format('Y-m-d')"),
            ("Import Illuminate\\Support\\Facades\\Route","Illuminate\\Support\\Facades\\Route"),
            ("Move $request to a constructor property","$request"),
            ("Use #[Scope] on active()","active()"),
            ("Inject CacheManager into save() instead of the Cache facade","save()"),
        ]{
            let translated=super::localized_title(authored);assert_ne!(translated,authored);assert!(translated.contains(protected),"{translated}");
        }
    }
}
