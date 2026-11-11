//! Routes of `routes/*.php`: `Route::get(…)->name(…)`, the legacy
//! `['as' => …]` option, `Route::resource` with `only`/`except`/`names`, and
//! the name and URI prefixes of the groups around them.

use std::ops::Range;

use crate::php::{self, is_identifier};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteDef {
    pub methods: String,
    pub uri: String,
    pub name: Option<String>,
    /// `UserController@index`, `Closure`, `view: welcome`.
    pub action: String,
    /// Short controller class and method, when the action names them.
    pub controller: Option<(String, String)>,
    /// The name when there is one, else the verb.
    pub offset: usize,
    pub width: usize,
}

const VERBS: &[&str] = &[
    "get",
    "post",
    "put",
    "patch",
    "delete",
    "options",
    "any",
    "match",
    "view",
    "redirect",
    "permanentRedirect",
    "resource",
    "apiResource",
    "resources",
    "apiResources",
    "singleton",
    "apiSingleton",
    "singletons",
    "apiSingletons",
];
const RESOURCE_ACTIONS: &[(&str, &str, &str)] = &[
    ("index", "GET|HEAD", ""),
    ("create", "GET|HEAD", "/create"),
    ("store", "POST", ""),
    ("show", "GET|HEAD", "/{}"),
    ("edit", "GET|HEAD", "/{}/edit"),
    ("update", "PUT|PATCH", "/{}"),
    ("destroy", "DELETE", "/{}"),
];

#[derive(Clone)]
struct Segment {
    name: String,
    start: usize,
    open: usize,
    args: Vec<Range<usize>>,
}

#[derive(Clone)]
struct Chain {
    start: usize,
    segments: Vec<Segment>,
}

/// `Route::a(…)->b(…)->c(…)` starting at each `Route::` of the text.
fn chains(text: &str) -> Vec<Chain> {
    crate::eloquent_members::metadata_inference::route_calls(text)
        .into_iter()
        .filter_map(|(at, method)| parse_chain_from(text, at, method))
        .collect()
}

fn parse_chain_from(text: &str, at: usize, method_start: usize) -> Option<Chain> {
    let mut i = method_start;
    let mut segments = Vec::new();

    loop {
        let name_start = i;
        while i < text.len() && is_identifier(text.as_bytes()[i]) {
            i += 1;
        }
        if i == name_start {
            break;
        }

        let after = text[i..].trim_start();
        let open = text.len() - after.len();
        if !after.starts_with('(') {
            break;
        }

        let raw = &text[name_start..i];
        let lower = raw.to_ascii_lowercase();
        let name = match lower.as_str() {
            "apiresource" => "apiResource",
            "apiresources" => "apiResources",
            "apisingleton" => "apiSingleton",
            "apisingletons" => "apiSingletons",
            "permanentredirect" => "permanentRedirect",
            _ => lower.as_str(),
        }
        .to_owned();
        let Some(close) = php::closing(text, open) else {
            segments.push(Segment {
                name,
                start: name_start,
                open,
                args: Vec::new(),
            });
            break;
        };
        segments.push(Segment {
            name,
            start: name_start,
            open,
            args: php::split(text, open + 1..close),
        });

        let rest = text[close + 1..].trim_start();
        let Some(rest) = rest.strip_prefix("?->").or_else(|| rest.strip_prefix("->")) else {
            break;
        };
        i = text.len() - rest.trim_start().len();
    }

    (!segments.is_empty()).then_some(Chain {
        start: at,
        segments,
    })
}

/// Proven external route-file scope, applied without rewriting source text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MountContext {
    pub prefix: String,
    pub name: String,
    pub namespace: String,
}
fn merge_namespace(old: &str, new: &str) -> String {
    if !old.is_empty() && !new.starts_with('\\') {
        format!("{}\\{}", old.trim_matches('\\'), new.trim_matches('\\'))
    } else {
        new.trim_matches('\\').to_owned()
    }
}
fn provider_namespace_range(text: &str, range: Range<usize>) -> Option<Range<usize>> {
    let raw = php::mask_comments(text.get(range.clone())?);
    if raw
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>()
        != "$this->namespace"
    {
        return None;
    }
    let fact = crate::eloquent_members::metadata_inference::class_fact(text, "")?;
    if fact.class_kind != "class"
        || fact.parents.len() != 1
        || !fact.parents[0].eq_ignore_ascii_case(
            "Illuminate\\Foundation\\Support\\Providers\\RouteServiceProvider",
        )
        || !fact.traits.is_empty()
        || fact.references.iter().any(|reference| {
            ["ReflectionProperty", "ReflectionObject", "Closure"]
                .iter()
                .any(|name| reference.eq_ignore_ascii_case(name))
        })
    {
        return None;
    }
    let code = crate::package_facts::code(text)?;
    let name = openidea_plugin::text::offset(text, fact.entry.at)?;
    let open = code[name..].find('{')? + name;
    let close = php::closing(&code, open)?;
    if !(open < range.start && range.end < close)
        || code.contains("->$")
        || code.contains("->{")
        || code.contains("$$")
    {
        return None;
    }
    let declarations = code
        .match_indices("$namespace")
        .filter(|(at, _)| {
            code.as_bytes()
                .get(at + 10)
                .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_' && *byte < 128)
        })
        .map(|(at, _)| at)
        .collect::<Vec<_>>();
    if declarations.len() != 1 {
        return None;
    }
    let declared = declarations[0];
    if declared < open
        || declared > close
        || code[open + 1..declared]
            .bytes()
            .fold(0i32, |depth, byte| match byte {
                b'{' => depth + 1,
                b'}' => depth - 1,
                _ => depth,
            })
            != 0
    {
        return None;
    }
    let member = code[..declared]
        .rfind(['{', '}', ';'])
        .map_or(0, |at| at + 1);
    let modifiers = code[member..declared]
        .split_whitespace()
        .collect::<Vec<_>>();
    if !matches!(modifiers.as_slice(), ["protected"] | ["public"]) {
        return None;
    }
    let mut begin = declared + 10;
    while code
        .as_bytes()
        .get(begin)
        .is_some_and(u8::is_ascii_whitespace)
    {
        begin += 1;
    }
    if code.as_bytes().get(begin) != Some(&b'=') {
        return None;
    }
    begin += 1;
    let end = code[begin..].find(';')? + begin;
    if php::literal(text, begin..end).is_none() {
        return None;
    }
    let compact = code
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    if compact.to_ascii_lowercase().contains("eval(")
        || compact.to_ascii_lowercase().contains("reflectionproperty")
        || compact.to_ascii_lowercase().contains("reflectionobject")
        || compact
            .match_indices("$this")
            .any(|(at, _)| !compact[at + 5..].starts_with("->"))
    {
        return None;
    }
    let namespace_prefixes = [
        "Route::namespace(",
        "\\Illuminate\\Support\\Facades\\Route::namespace(",
    ];
    if !crate::eloquent_members::source_class_name(text, "Route")
        .is_some_and(|full| full.eq_ignore_ascii_case("Illuminate\\Support\\Facades\\Route"))
    {
        return None;
    }
    for (at, _) in compact.match_indices("$this->") {
        let rest = &compact[at + 7..];
        if rest.starts_with("namespace") {
            if !namespace_prefixes
                .iter()
                .any(|prefix| compact[..at].ends_with(prefix))
                || !rest[9..].starts_with(')')
            {
                return None;
            }
        } else if !rest.starts_with("routes(") && !rest.starts_with("setRootControllerNamespace(") {
            return None;
        }
    }
    Some(begin..end)
}
fn namespace_literal(text: &str, range: Range<usize>) -> Option<String> {
    let literal = if php::literal(text, range.clone()).is_some() {
        range
    } else {
        provider_namespace_range(text, range)?
    };
    let (raw, start) = php::literal(text, literal)?;
    let quote = *text.as_bytes().get(start.checked_sub(1)?)?;
    let value = crate::eloquent_members::metadata_inference::decode_literal(raw, quote)?;
    let trimmed = value.trim_matches('\\');
    (!trimmed.is_empty()
        && trimmed.split('\\').all(|part| {
            !part.is_empty()
                && !part.as_bytes()[0].is_ascii_digit()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 128)
        }))
    .then_some(value)
}
fn namespace_action(class: &str, namespace: &str) -> String {
    if !namespace.is_empty() && !class.starts_with('\\') && !class.starts_with(namespace) {
        format!("{namespace}\\{class}")
    } else {
        class.to_owned()
    }
}
fn class_value(text: &str, range: Range<usize>, full: bool) -> Option<String> {
    if let Some(class) = php::class_constant(text, range.clone()) {
        return if full {
            crate::eloquent_members::source_class_name(text, class)
        } else {
            Some(short(class).into())
        };
    }
    let (raw, start) = php::literal(text, range)?;
    let value = crate::eloquent_members::metadata_inference::decode_literal(
        raw,
        *text.as_bytes().get(start - 1)?,
    )?;
    Some(if full { value } else { short(&value).into() })
}

struct Region {
    body: Range<usize>,
    names: String,
    prefix: String,
    controller: Option<String>,
    namespace: Option<String>,
    valid: bool,
}

fn short(class: &str) -> &str {
    class.rsplit('\\').next().unwrap_or(class)
}

fn join_uri(parts: &[&str]) -> String {
    let joined: Vec<&str> = parts
        .iter()
        .map(|p| p.trim_matches('/'))
        .filter(|p| !p.is_empty())
        .collect();
    format!("/{}", joined.join("/"))
}

fn first_literal<'a>(text: &'a str, segment: &Segment) -> Option<(&'a str, usize)> {
    php::literal(text, segment.args.first()?.clone())
}

#[derive(Default)]
struct GroupScope {
    context: MountContext,
    namespace: Option<String>,
    controller: Option<String>,
}
fn decoded_group_literal(text: &str, range: Range<usize>) -> Option<String> {
    let (raw, start) = php::literal(text, range)?;
    let value = crate::eloquent_members::metadata_inference::decode_literal(
        raw,
        text.as_bytes()[start - 1],
    )?;
    (!value.chars().any(char::is_control)).then_some(value)
}
fn group_scope(text: &str, chain: &Chain, group: &Segment) -> Option<GroupScope> {
    let mut scope = GroupScope::default();
    for segment in chain
        .segments
        .iter()
        .take_while(|segment| segment.start < group.start)
    {
        match segment.name.as_str() {
            "name" => {
                scope.context.name = decoded_group_literal(text, segment.args.first()?.clone())?
            }
            "prefix" => {
                scope.context.prefix = decoded_group_literal(text, segment.args.first()?.clone())?
            }
            "namespace" => {
                scope.namespace = Some(namespace_literal(text, segment.args.first()?.clone())?)
            }
            "controller" => {
                scope.controller = Some(class_value(text, segment.args.first()?.clone(), true)?)
            }
            "domain" => return None,
            _ => {}
        }
    }
    if group.args.len() == 2 {
        // RouteRegistrar::group takes a callback only; array options belong to
        // the direct facade group call, not a preceding registrar chain.
        if chain.segments.first()?.start != group.start {
            return None;
        }
        let open = literal_array(text, group.args[0].clone())?;
        let mut seen = std::collections::BTreeSet::new();
        for item in php::items(text, open) {
            let key = decoded_group_literal(text, item.key?)?;
            if !seen.insert(key.clone()) {
                return None;
            }
            match key.as_str() {
                "as" => scope.context.name = decoded_group_literal(text, item.value)?,
                "prefix" => scope.context.prefix = decoded_group_literal(text, item.value)?,
                "namespace" => scope.namespace = Some(namespace_literal(text, item.value)?),
                "controller" => scope.controller = Some(class_value(text, item.value, true)?),
                "domain" => return None,
                _ => {}
            }
        }
    } else if group.args.len() != 1 {
        return None;
    }
    Some(scope)
}
fn closure_body(text: &str, range: Range<usize>) -> Option<Range<usize>> {
    let range = php::trimmed(text, range);
    let code = crate::package_facts::code(text)?;
    let value = code[range.clone()].trim_start();
    let value = value
        .strip_prefix("static")
        .map(str::trim_start)
        .unwrap_or(value);
    if value.starts_with("function") {
        let begin = code[range.clone()].find('{')? + range.start;
        let end = php::closing(text, begin)?;
        (end < range.end).then_some(begin + 1..end)
    } else if value.starts_with("fn") {
        let begin = code[range.clone()].find("=>")? + range.start + 2;
        Some(begin..range.end)
    } else {
        None
    }
}
/// Closure regions retain real source offsets and mark unknown attributes so
/// children never fall back to an invented unscoped registration.
fn region_of(text: &str, chain: &Chain) -> Option<Region> {
    let group = chain
        .segments
        .iter()
        .find(|segment| segment.name == "group")?;
    let body = closure_body(text, group.args.last()?.clone())?;
    let scope = group_scope(text, chain, group);
    let valid = scope.is_some();
    let scope = scope.unwrap_or_default();
    Some(Region {
        body,
        names: scope.context.name,
        prefix: scope.context.prefix,
        namespace: scope.namespace,
        controller: scope.controller,
        valid,
    })
}

/// The action shown for the route and the controller method it names.
fn action_of(
    text: &str,
    range: Option<Range<usize>>,
    controller: Option<&str>,
    namespace: &str,
) -> (String, Option<(String, String)>) {
    let Some(range) = range else {
        return ("Closure".into(), None);
    };
    let trimmed = php::trimmed(text, range.clone());
    let source = &text[trimmed];
    let named = |class: String, method: String| {
        let action = format!("{class}@{method}");
        (
            action,
            Some((class.trim_start_matches('\\').to_owned(), method)),
        )
    };
    if let Some((raw, start)) = php::literal(text, range.clone()) {
        let Some(action) = crate::eloquent_members::metadata_inference::decode_literal(
            raw,
            text.as_bytes()[start - 1],
        ) else {
            return ("Dynamic handler".into(), None);
        };
        return if let Some((class, method)) = action.split_once('@') {
            named(namespace_action(class, namespace), method.into())
        } else if let Some(class) = controller {
            named(namespace_action(class, namespace), action)
        } else if !namespace.is_empty() {
            named(namespace_action(&action, namespace), "__invoke".into())
        } else {
            (action, None)
        };
    }
    if php::class_constant(text, range.clone()).is_some() {
        return match class_value(text, range.clone(), !namespace.is_empty()) {
            Some(class) => named(namespace_action(&class, namespace), "__invoke".into()),
            None => ("Dynamic handler".into(), None),
        };
    }
    if let Some(open) = php::array_open(text, range) {
        let items = php::items(text, open);
        if let Some(uses) = items.iter().find(|item| {
            item.key
                .clone()
                .and_then(|key| php::literal(text, key))
                .is_some_and(|(key, _)| key == "uses")
        }) {
            return action_of(text, Some(uses.value.clone()), controller, namespace);
        }
        let class = items.first().and_then(|item| {
            php::class_constant(text, item.value.clone())
                .and_then(|_| class_value(text, item.value.clone(), !namespace.is_empty()))
        });
        let method = items
            .get(1)
            .and_then(|item| php::literal(text, item.value.clone()));
        if let (Some(class), Some((method, _))) = (class, method) {
            return named(class, method.into());
        }
    }
    if source.starts_with("function") || source.starts_with("fn") || source.starts_with("static") {
        ("Closure".into(), None)
    } else {
        ("Dynamic handler".into(), None)
    }
}

fn strings_of(text: &str, range: &Range<usize>) -> Vec<String> {
    php::array_open(text, range.clone())
        .map(|open| php::strings(text, open, false))
        .unwrap_or_default()
}

/// The routes of one file, from its text with comments blanked.
pub fn parse(text: &str) -> Vec<RouteDef> {
    let chains = chains(text);
    let regions: Vec<Region> = chains
        .iter()
        .filter_map(|chain| region_of(text, chain))
        .collect();
    let mut defs = Vec::new();

    for chain in &chains {
        let enclosing: Vec<&Region> = regions
            .iter()
            .filter(|r| r.body.contains(&chain.start))
            .collect();
        defs.extend(route_defs(text, chain, &enclosing));
    }

    defs.sort_by_key(|d| d.offset);
    defs
}

/// Parse the original source under an external mounted scope. Every byte range
/// still points into `text`; no synthetic PHP or offset adjustment is involved.
pub(crate) fn parse_with_context(
    text: &str,
    custom: &std::collections::BTreeMap<String, crate::metadata::RouteMethod>,
    context: &MountContext,
) -> Vec<RouteDef> {
    let all = chains(text);
    let regions = all
        .iter()
        .filter_map(|chain| region_of(text, chain))
        .collect::<Vec<_>>();
    let mut defs = Vec::new();
    for chain in &all {
        let enclosing = regions
            .iter()
            .filter(|region| region.body.contains(&chain.start))
            .collect::<Vec<_>>();
        defs.extend(configured_defs_seeded(
            text, chain, &enclosing, custom, context,
        ));
    }
    defs.sort_by_key(|def| def.offset);
    defs
}

fn scoped_namespace(
    text: &str,
    context: &MountContext,
    enclosing: &[&Region],
    chain: &Chain,
    verb: &Segment,
) -> Option<String> {
    let mut namespace = context.namespace.clone();
    for region in enclosing {
        if let Some(value) = &region.namespace {
            namespace = merge_namespace(&namespace, value);
        }
    }
    let own = chain
        .segments
        .iter()
        .filter(|segment| segment.name == "namespace")
        .collect::<Vec<_>>();
    if own.iter().any(|segment| segment.start > verb.start) {
        return None;
    }
    if let Some(segment) = own.last() {
        namespace = merge_namespace(
            &namespace,
            &namespace_literal(text, segment.args.first()?.clone())?,
        );
    }
    Some(namespace)
}
fn runtime_handler(
    text: &str,
    range: Range<usize>,
    controller: Option<&str>,
    namespace: &str,
) -> Option<(String, String)> {
    let pair =
        |class: String, method: String| Some((class.trim_start_matches('\\').to_owned(), method));
    if let Some((raw, start)) = php::literal(text, range.clone()) {
        let action = crate::eloquent_members::metadata_inference::decode_literal(
            raw,
            text.as_bytes()[start - 1],
        )?;
        let (class, method) = action
            .split_once('@')
            .map(|(class, method)| (class.to_owned(), method.to_owned()))
            .unwrap_or_else(|| {
                controller
                    .map(|class| (class.to_owned(), action.clone()))
                    .unwrap_or((action, "__invoke".into()))
            });
        if class.trim_start_matches('\\').split('\\').any(|part| {
            part.is_empty()
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 128)
        }) || method.is_empty()
            || !method
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 128)
        {
            return None;
        }
        return pair(namespace_action(&class, namespace), method);
    }
    if php::class_constant(text, range.clone()).is_some() {
        return pair(
            namespace_action(&class_value(text, range, true)?, namespace),
            "__invoke".into(),
        );
    }
    let open = literal_array(text, range)?;
    let items = php::items(text, open);
    if let Some(uses) = items.iter().find(|item| {
        item.key
            .clone()
            .and_then(|key| php::literal(text, key))
            .is_some_and(|(key, _)| key == "uses")
    }) {
        return runtime_handler(text, uses.value.clone(), controller, namespace);
    }
    let class = class_value(text, items.first()?.value.clone(), true)?;
    let method = decoded_group_literal(text, items.get(1)?.value.clone())?;
    pair(class, method)
}
/// Exact canonical handlers for a source inventory. Runtime string classes are
/// never interpreted as PHP import aliases or guessed under an application root.
pub(crate) fn canonical_controllers(
    text: &str,
    custom: &std::collections::BTreeMap<String, crate::metadata::RouteMethod>,
    context: &MountContext,
) -> Vec<(RouteDef, (String, String))> {
    let all = chains(text);
    let regions = all
        .iter()
        .filter_map(|chain| region_of(text, chain))
        .collect::<Vec<_>>();
    let mut out = Vec::new();
    for chain in &all {
        let Some(verb) = chain.segments.iter().find(|segment| {
            VERBS.contains(&segment.name.as_str())
                || custom.contains_key(&segment.name.to_ascii_lowercase())
        }) else {
            continue;
        };
        if matches!(
            verb.name.as_str(),
            "view" | "redirect" | "permanentRedirect"
        ) {
            continue;
        }
        let enclosing = regions
            .iter()
            .filter(|region| region.body.contains(&chain.start))
            .collect::<Vec<_>>();
        let Some(namespace) = scoped_namespace(text, context, &enclosing, chain, verb) else {
            continue;
        };
        let group = chain
            .segments
            .iter()
            .find(|segment| segment.name == "controller")
            .and_then(|segment| segment.args.first())
            .and_then(|range| class_value(text, range.clone(), true))
            .or_else(|| {
                enclosing
                    .iter()
                    .rev()
                    .find_map(|region| region.controller.clone())
            });
        if chain
            .segments
            .iter()
            .any(|segment| segment.name == "controller")
            && group.is_none()
        {
            continue;
        }
        let bulk = matches!(
            verb.name.as_str(),
            "resources" | "apiResources" | "singletons" | "apiSingletons"
        );
        let resource = bulk
            || matches!(
                verb.name.as_str(),
                "resource" | "apiResource" | "singleton" | "apiSingleton"
            );
        for def in configured_defs_seeded(text, chain, &enclosing, custom, context) {
            let range = if bulk {
                verb.args
                    .first()
                    .and_then(|range| literal_array(text, range.clone()))
                    .and_then(|open| {
                        php::items(text, open)
                            .into_iter()
                            .find(|item| {
                                item.key
                                    .clone()
                                    .and_then(|key| php::literal(text, key))
                                    .is_some_and(|(_, at)| at == def.offset)
                            })
                            .map(|item| item.value)
                    })
            } else {
                let index = custom
                    .get(&verb.name.to_ascii_lowercase())
                    .and_then(|method| method.controller_index.map(|index| index - 1))
                    .unwrap_or(if verb.name == "match" { 2 } else { 1 });
                verb.args.get(index).cloned()
            };
            let handler = if resource {
                range
                    .and_then(|range| class_value(text, range, true))
                    .or_else(|| group.clone())
                    .and_then(|class| {
                        Some((
                            namespace_action(&class, &namespace)
                                .trim_start_matches('\\')
                                .to_owned(),
                            def.controller.as_ref()?.1.clone(),
                        ))
                    })
            } else {
                range.and_then(|range| runtime_handler(text, range, group.as_deref(), &namespace))
            };
            if let Some(handler) = handler {
                out.push((def, handler));
            }
        }
    }
    out
}
pub(crate) fn canonical_controller(
    text: &str,
    custom: &std::collections::BTreeMap<String, crate::metadata::RouteMethod>,
    context: &MountContext,
    def: &RouteDef,
) -> Option<(String, String)> {
    let mut matches = canonical_controllers(text, custom, context)
        .into_iter()
        .filter(|(candidate, _)| candidate == def)
        .map(|(_, handler)| handler)
        .collect::<std::collections::BTreeSet<_>>();
    (matches.len() == 1).then(|| matches.pop_first()).flatten()
}

/// Handler identity for the controller argument containing `at`, evaluated in
/// the same mounted/lexical scope as the route inventory. An empty member names
/// a resource controller with several expanded methods, never an arbitrary one.
pub(crate) fn controller_at(
    text: &str,
    custom: &std::collections::BTreeMap<String, crate::metadata::RouteMethod>,
    context: &MountContext,
    at: usize,
) -> Option<(String, String)> {
    let all = chains(text);
    let regions = all
        .iter()
        .filter_map(|chain| region_of(text, chain))
        .collect::<Vec<_>>();
    let canonical = canonical_controllers(text, custom, context);
    let mut selected = None;
    for chain in &all {
        let Some(verb) = chain.segments.iter().find(|segment| {
            VERBS.contains(&segment.name.as_str())
                || custom.contains_key(&segment.name.to_ascii_lowercase())
        }) else {
            continue;
        };
        let action = if matches!(
            verb.name.as_str(),
            "resources" | "apiResources" | "singletons" | "apiSingletons"
        ) {
            let open = literal_array(text, verb.args.first()?.clone())?;
            php::items(text, open)
                .into_iter()
                .find(|item| item.value.start <= at && at <= item.value.end)
                .and_then(|item| Some((item.value, Some(php::literal(text, item.key?)?.1))))
        } else {
            let index = custom
                .get(&verb.name.to_ascii_lowercase())
                .and_then(|method| method.controller_index.map(|index| index - 1))
                .unwrap_or(if verb.name == "match" { 2 } else { 1 });
            verb.args
                .get(index)
                .filter(|range| range.start <= at && at <= range.end)
                .cloned()
                .map(|range| (range, None))
        };
        let Some((_range, resource_offset)) = action else {
            continue;
        };
        let enclosing = regions
            .iter()
            .filter(|region| region.body.contains(&chain.start))
            .collect::<Vec<_>>();
        let defs = configured_defs_seeded(text, chain, &enclosing, custom, context);
        let mut identities = std::collections::BTreeSet::new();
        for def in defs
            .into_iter()
            .filter(|def| resource_offset.is_none_or(|offset| def.offset == offset))
        {
            for (candidate, handler) in &canonical {
                if candidate == &def {
                    identities.insert(handler.clone());
                }
            }
        }
        let class = identities.first()?.0.clone();
        if identities.iter().any(|(other, _)| other != &class) {
            return None;
        }
        let member = if identities.len() == 1 {
            identities.pop_first()?.1
        } else {
            String::new()
        };
        if selected
            .as_ref()
            .is_some_and(|previous| previous != &(class.clone(), member.clone()))
        {
            return None;
        }
        selected = Some((class, member));
    }
    selected
}

/// Declaration offsets for proven resource-family registrations. All expanded
/// methods of a registration retain their original literal-key range, including
/// a one-method `only` selection and entries in a bulk map.
pub(crate) fn resource_offsets(text: &str) -> std::collections::BTreeSet<usize> {
    if text.len() > 512 << 10 {
        return std::collections::BTreeSet::new();
    }
    let all = chains(text);
    let regions = all
        .iter()
        .filter_map(|chain| region_of(text, chain))
        .collect::<Vec<_>>();
    all.iter()
        .filter(|chain| {
            chain.segments.iter().any(|segment| {
                matches!(
                    segment.name.as_str(),
                    "resource"
                        | "apiResource"
                        | "resources"
                        | "apiResources"
                        | "singleton"
                        | "apiSingleton"
                        | "singletons"
                        | "apiSingletons"
                )
            })
        })
        .flat_map(|chain| {
            let enclosing = regions
                .iter()
                .filter(|region| region.body.contains(&chain.start))
                .collect::<Vec<_>>();
            route_defs(text, chain, &enclosing)
                .into_iter()
                .map(|def| def.offset)
        })
        .collect()
}

/// Parameters in the route chain enclosing a completion caret. Dynamic URLs
/// and unrelated outer groups produce no invented names.
pub(crate) fn parameters_at(text: &str, at: usize) -> Vec<String> {
    if text.len() > 512 << 10 || at > text.len() {
        return Vec::new();
    }
    let all = chains(text);
    let Some(chain) = all
        .iter()
        .filter(|chain| {
            chain.segments.iter().any(|segment| {
                segment
                    .args
                    .iter()
                    .any(|arg| arg.start <= at && at <= arg.end)
            })
        })
        .max_by_key(|chain| chain.start)
    else {
        return Vec::new();
    };
    let regions: Vec<_> = all
        .iter()
        .filter_map(|chain| region_of(text, chain))
        .collect();
    let enclosing = regions
        .iter()
        .filter(|region| region.body.contains(&chain.start))
        .collect::<Vec<_>>();
    let mut result = Vec::new();
    for def in route_defs(text, chain, &enclosing) {
        for after in def.uri.split('{').skip(1) {
            let Some((parameter, _)) = after.split_once('}') else {
                continue;
            };
            let name = parameter
                .trim_end_matches('?')
                .split(':')
                .next()
                .unwrap_or("");
            if !name.is_empty()
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && !result.iter().any(|previous| previous == name)
            {
                result.push(name.to_owned());
            }
        }
    }
    result
}

/// Original plural resource keys, before parameter-map overrides. This remains
/// useful while a map key is incomplete and expansion is intentionally withheld.
pub(crate) fn resource_names_at(text: &str, at: usize) -> Vec<String> {
    if text.len() > 512 << 10 || at > text.len() {
        return Vec::new();
    }
    let all = chains(text);
    let Some(chain) = all
        .iter()
        .filter(|chain| {
            chain.segments.iter().any(|segment| {
                segment
                    .args
                    .iter()
                    .any(|arg| arg.start <= at && at <= arg.end)
            })
        })
        .max_by_key(|chain| chain.start)
    else {
        return Vec::new();
    };
    let Some(resource) = chain
        .segments
        .iter()
        .find(|segment| {
            matches!(
                segment.name.as_str(),
                "resource" | "apiResource" | "singleton" | "apiSingleton"
            )
        })
        .and_then(|segment| first_literal(text, segment).map(|(resource, _)| resource))
    else {
        return Vec::new();
    };
    let resource = resource.rsplit('/').next().unwrap_or(resource);
    let mut names = Vec::new();
    for part in resource.split('.') {
        if part.is_empty()
            || !part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Vec::new();
        }
        if !names.iter().any(|name| name == part) {
            names.push(part.to_owned());
        }
    }
    names
}

/// Add documented custom methods, retaining the existing core route parser.
/// Custom calls use bounded, source-proven facade/import identities.
pub(crate) fn parse_with_custom(
    text: &str,
    custom: &std::collections::BTreeMap<String, crate::metadata::RouteMethod>,
) -> Vec<RouteDef> {
    let mut defs = parse(text);
    if custom.is_empty() {
        return defs;
    }
    let chains: Vec<_> = crate::eloquent_members::metadata_inference::route_calls(text)
        .into_iter()
        .filter_map(|(at, method)| parse_chain_from(text, at, method))
        .collect();
    let regions: Vec<_> = chains
        .iter()
        .filter_map(|chain| region_of(text, chain))
        .collect();
    for chain in &chains {
        if !chain
            .segments
            .iter()
            .any(|segment| custom.contains_key(&segment.name.to_ascii_lowercase()))
        {
            continue;
        }
        let enclosing: Vec<_> = regions
            .iter()
            .filter(|region| region.body.contains(&chain.start))
            .collect();
        let original = route_defs(text, chain, &enclosing);
        defs.retain(|def| !original.iter().any(|old| old.offset == def.offset));
        defs.extend(configured_defs(text, chain, &enclosing, custom));
    }
    defs.sort_by_key(|def| def.offset);
    defs
}

fn route_defs(text: &str, chain: &Chain, enclosing: &[&Region]) -> Vec<RouteDef> {
    configured_defs(text, chain, enclosing, &std::collections::BTreeMap::new())
}
fn configured_defs(
    text: &str,
    chain: &Chain,
    enclosing: &[&Region],
    custom: &std::collections::BTreeMap<String, crate::metadata::RouteMethod>,
) -> Vec<RouteDef> {
    configured_defs_seeded(text, chain, enclosing, custom, &MountContext::default())
}
fn configured_defs_seeded(
    text: &str,
    chain: &Chain,
    enclosing: &[&Region],
    custom: &std::collections::BTreeMap<String, crate::metadata::RouteMethod>,
    context: &MountContext,
) -> Vec<RouteDef> {
    if enclosing.iter().any(|region| !region.valid)
        || chain
            .segments
            .iter()
            .filter(|segment| segment.name == "prefix")
            .any(|segment| first_literal(text, segment).is_none())
    {
        return Vec::new();
    }
    let Some(verb) = chain.segments.iter().find(|s| {
        VERBS.contains(&s.name.as_str()) || custom.contains_key(&s.name.to_ascii_lowercase())
    }) else {
        return Vec::new();
    };
    let method = custom
        .get(&verb.name.to_ascii_lowercase())
        .filter(|method| method.name.eq_ignore_ascii_case(&verb.name));
    if let Some(method) = method {
        let through = method
            .url_index
            .into_iter()
            .chain(method.controller_index)
            .max()
            .unwrap_or(0);
        if verb.args.iter().take(through).any(|range| {
            let argument = text[range.clone()].trim_start();
            argument.starts_with("...")
                || argument.split_once(':').is_some_and(|(name, _)| {
                    let mut chars = name.trim().chars();
                    chars
                        .next()
                        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
                })
        }) {
            return Vec::new();
        }
    }
    let name_prefix = format!(
        "{}{}",
        context.name,
        enclosing
            .iter()
            .map(|region| region.names.as_str())
            .collect::<String>()
    );
    let Some(namespace) = scoped_namespace(text, context, enclosing, chain, verb) else {
        return Vec::new();
    };
    let controller = chain
        .segments
        .iter()
        .find(|s| s.name == "controller")
        .and_then(|s| {
            s.args
                .first()
                .and_then(|a| class_value(text, a.clone(), !namespace.is_empty()))
        })
        .or_else(|| enclosing.iter().rev().find_map(|r| r.controller.clone()));

    let mut uri_parts = vec![context.prefix.as_str()];
    uri_parts.extend(enclosing.iter().map(|region| region.prefix.as_str()));
    uri_parts.extend(
        chain
            .segments
            .iter()
            .filter(|s| s.name == "prefix")
            .filter_map(|s| first_literal(text, s).map(|(p, _)| p)),
    );

    let own_names: Vec<(&str, usize)> = chain
        .segments
        .iter()
        .filter(|s| s.name == "name")
        .filter_map(|s| first_literal(text, s))
        .collect();
    let arg = |i: usize| verb.args.get(i).cloned();
    let uri_literal = |i: usize| arg(i).and_then(|r| php::literal(text, r)).map(|(u, _)| u);
    if let Some(method) = method {
        if method
            .url_index
            .and_then(|i| uri_literal(i - 1))
            .is_none_or(|uri| uri.contains(['$', '\\']))
        {
            return Vec::new();
        }
    } else if !matches!(
        verb.name.as_str(),
        "resource"
            | "apiResource"
            | "resources"
            | "apiResources"
            | "singleton"
            | "apiSingleton"
            | "singletons"
            | "apiSingletons"
    ) && uri_literal(if verb.name == "match" { 1 } else { 0 }).is_none()
    {
        return Vec::new();
    }

    let (methods, uri, options, action_range) = if let Some(method) = method {
        let action = method.controller_index.and_then(|i| arg(i - 1));
        (
            method.http_method.clone(),
            uri_literal(method.url_index.unwrap() - 1).unwrap(),
            action.clone(),
            action,
        )
    } else {
        match verb.name.as_str() {
            "match" => {
                let methods = arg(0)
                    .map(|r| strings_of(text, &r))
                    .unwrap_or_default()
                    .join("|")
                    .to_uppercase();
                (methods, uri_literal(1).unwrap(), arg(2), arg(2))
            }
            "any" => ("ANY".to_owned(), uri_literal(0).unwrap(), arg(1), arg(1)),
            "view" => ("GET|HEAD".to_owned(), uri_literal(0).unwrap(), None, None),
            "redirect" | "permanentRedirect" => (
                "GET|HEAD|POST|PUT|PATCH|DELETE|OPTIONS".to_owned(),
                uri_literal(0).unwrap(),
                None,
                None,
            ),
            "resource" | "apiResource" | "singleton" | "apiSingleton" => {
                return resource_defs(
                    text,
                    chain,
                    verb,
                    &name_prefix,
                    &uri_parts,
                    controller,
                    &namespace,
                );
            }
            "resources" | "apiResources" | "singletons" | "apiSingletons" => {
                return bulk_resources(
                    text,
                    chain,
                    verb,
                    &name_prefix,
                    &uri_parts,
                    controller,
                    &namespace,
                );
            }
            name => (
                name.to_uppercase().replace("GET", "GET|HEAD"),
                uri_literal(0).unwrap(),
                arg(1),
                arg(1),
            ),
        }
    };

    let options_array = options.and_then(|r| php::array_open(text, r));
    let legacy_name = options_array.and_then(|open| php::string_value(text, open, "as"));
    let mut name: Option<String> = None;
    let mut offset = verb.start;
    let mut width = verb.name.len();

    if let Some((value, at)) = legacy_name {
        name = Some(value.to_owned());
        (offset, width) = (at, value.len());
    }

    if let Some((_, at)) = own_names.first() {
        let joined: String = own_names.iter().map(|(n, _)| *n).collect();
        name = Some(format!("{}{joined}", name.unwrap_or_default()));
        (offset, width) = (*at, own_names[0].0.len());
    }

    let (action, controller) = if method.is_some() {
        action_of(text, action_range, controller.as_deref(), &namespace)
    } else {
        match verb.name.as_str() {
            "view" => (
                format!(
                    "view: {}",
                    arg(1)
                        .and_then(|r| php::literal(text, r))
                        .map_or("?", |(v, _)| v)
                ),
                None,
            ),
            "redirect" | "permanentRedirect" => (
                format!(
                    "Redirect → {}",
                    arg(1)
                        .and_then(|r| php::literal(text, r))
                        .map_or("dynamic destination", |(v, _)| v)
                ),
                None,
            ),
            _ => action_of(text, action_range, controller.as_deref(), &namespace),
        }
    };

    uri_parts.push(uri);
    vec![RouteDef {
        methods,
        uri: join_uri(&uri_parts),
        name: name.map(|n| format!("{name_prefix}{n}")),
        action,
        controller,
        offset,
        width,
    }]
}

#[derive(Default)]
struct ResourceOptions {
    only: Option<Vec<String>>,
    except: Option<Vec<String>>,
    parameters: std::collections::BTreeMap<String, String>,
    names: std::collections::BTreeMap<String, String>,
    name_base: Option<String>,
    name_prefix: String,
    shallow: bool,
    creatable: bool,
    destroyable: bool,
}
fn strict_literal(text: &str, range: Range<usize>) -> Option<String> {
    let (value, _) = php::literal(text, range)?;
    (!value.contains(['\\', '$']) && !value.chars().any(char::is_control)).then(|| value.to_owned())
}
fn literal_array(text: &str, range: Range<usize>) -> Option<usize> {
    let range = php::trimmed(text, range);
    let open = php::array_open(text, range.clone())?;
    (php::closing(text, open)? + 1 == range.end).then_some(open)
}
fn resource_options(text: &str, chain: &Chain, verb: &Segment) -> Option<ResourceOptions> {
    use std::collections::BTreeMap;
    fn list(text: &str, args: &[Range<usize>]) -> Option<Vec<String>> {
        if args.len() == 1
            && let Some(open) = literal_array(text, args[0].clone())
        {
            return php::items(text, open)
                .into_iter()
                .map(|item| {
                    if item.key.is_none() {
                        strict_literal(text, item.value)
                    } else {
                        None
                    }
                })
                .collect();
        }
        if args.is_empty() {
            return None;
        }
        args.iter()
            .map(|range| strict_literal(text, range.clone()))
            .collect()
    }
    fn map(text: &str, range: Range<usize>) -> Option<BTreeMap<String, String>> {
        let open = literal_array(text, range)?;
        let mut result = BTreeMap::new();
        for item in php::items(text, open) {
            let key = strict_literal(text, item.key?)?;
            let value = strict_literal(text, item.value)?;
            if result.insert(key, value).is_some() {
                return None;
            }
        }
        Some(result)
    }
    fn apply(
        text: &str,
        out: &mut ResourceOptions,
        key: &str,
        args: &[Range<usize>],
    ) -> Option<()> {
        match key {
            "only" => out.only = Some(list(text, args)?),
            "except" => out.except = Some(list(text, args)?),
            "parameters" if args.len() == 1 => out.parameters = map(text, args[0].clone())?,
            "parameter" if args.len() == 2 => {
                out.parameters.insert(
                    strict_literal(text, args[0].clone())?,
                    strict_literal(text, args[1].clone())?,
                );
            }
            "names" if args.len() == 1 => {
                if let Some(name) = strict_literal(text, args[0].clone()) {
                    out.name_base = Some(name);
                    out.names.clear();
                } else {
                    out.names = map(text, args[0].clone())?;
                    out.name_base = None;
                }
            }
            "name" if args.len() == 2 => {
                out.names.insert(
                    strict_literal(text, args[0].clone())?,
                    strict_literal(text, args[1].clone())?,
                );
            }
            "as" if args.len() == 1 => out.name_prefix = strict_literal(text, args[0].clone())?,
            "creatable" | "destroyable" => {
                let present = match args {
                    [] => true,
                    [range] => match text[range.clone()].trim() {
                        "null" => false,
                        "true" | "false" => true,
                        _ => return None,
                    },
                    _ => return None,
                };
                if key == "creatable" {
                    out.creatable = present;
                } else {
                    out.destroyable = present;
                }
            }
            "shallow" => {
                out.shallow = match args {
                    [] => true,
                    [range] => match text[range.clone()].trim() {
                        "true" => true,
                        "false" => false,
                        _ => return None,
                    },
                    _ => return None,
                }
            }
            "parameters" | "parameter" | "names" | "name" | "as" => {
                return None;
            }
            _ => {}
        }
        Some(())
    }
    let mut out = ResourceOptions::default();
    if let Some(range) = verb.args.get(2) {
        let open = literal_array(text, range.clone())?;
        let mut seen = std::collections::BTreeSet::new();
        for item in php::items(text, open) {
            let key = strict_literal(text, item.key?)?;
            if !seen.insert(key.clone()) {
                return None;
            }
            apply(text, &mut out, &key, &[item.value])?;
        }
    }
    for segment in chain
        .segments
        .iter()
        .filter(|segment| segment.start > verb.start)
    {
        apply(text, &mut out, &segment.name, &segment.args)?;
    }
    if out.parameters.values().any(|name| {
        name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    }) {
        return None;
    }
    Some(out)
}
/// Laravel uses its configured pluralizer; this covers the conventional English
/// defaults while explicit `parameters` always takes precedence.
fn resource_wildcard(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let irregular = match lower.as_str() {
        "statuses" => Some("status"),
        "buses" => Some("bus"),
        "aliases" => Some("alias"),
        "people" => Some("person"),
        "men" => Some("man"),
        "women" => Some("woman"),
        "children" => Some("child"),
        "mice" => Some("mouse"),
        "geese" => Some("goose"),
        "teeth" => Some("tooth"),
        "feet" => Some("foot"),
        "news" | "series" | "species" | "sheep" | "fish" | "deer" | "status" | "alias" | "bus" => {
            Some(lower.as_str())
        }
        _ => None,
    };
    let value = if let Some(value) = irregular {
        value.to_owned()
    } else if lower.ends_with("ies") && !lower.ends_with("movies") {
        format!("{}y", &name[..name.len() - 3])
    } else if ["ches", "shes", "sses", "xes", "zzes"]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
    {
        name[..name.len() - 2].to_owned()
    } else {
        name.strip_suffix('s').unwrap_or(name).to_owned()
    };
    value.replace('-', "_")
}

fn bulk_resources(
    text: &str,
    chain: &Chain,
    verb: &Segment,
    name_prefix: &str,
    uri_parts: &[&str],
    group_controller: Option<String>,
    namespace: &str,
) -> Vec<RouteDef> {
    // Bulk Router calls return void, so a following fluent chain is not a
    // registration that can be safely projected from source.
    if chain
        .segments
        .iter()
        .any(|segment| segment.start > verb.start)
        || !(1..=2).contains(&verb.args.len())
    {
        return Vec::new();
    }
    let Some(open) = literal_array(text, verb.args[0].clone()) else {
        return Vec::new();
    };
    let method = match verb.name.as_str() {
        "resources" => "resource",
        "apiResources" => "apiResource",
        "singletons" => "singleton",
        "apiSingletons" => "apiSingleton",
        _ => return Vec::new(),
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut all = Vec::new();
    for item in php::items(text, open) {
        let Some(key) = item.key else {
            return Vec::new();
        };
        let Some(name) = strict_literal(text, key.clone()) else {
            return Vec::new();
        };
        if !seen.insert(name) {
            return Vec::new();
        }
        let class = php::class_constant(text, item.value.clone())
            .or_else(|| php::literal(text, item.value.clone()).map(|(class, _)| class));
        if class.is_none_or(|class| {
            !class.trim_start_matches('\\').split('\\').all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                    && !part.as_bytes()[0].is_ascii_digit()
            })
        }) {
            return Vec::new();
        }
        let mut args = vec![key, item.value];
        if let Some(options) = verb.args.get(1) {
            args.push(options.clone());
        }
        let expanded = Segment {
            name: method.into(),
            start: verb.start,
            open: verb.open,
            args,
        };
        let mut copied = chain.clone();
        if let Some(segment) = copied
            .segments
            .iter_mut()
            .find(|segment| segment.start == verb.start)
        {
            *segment = expanded;
        }
        let Some(expanded) = copied
            .segments
            .iter()
            .find(|segment| segment.start == verb.start)
        else {
            return Vec::new();
        };
        let defs = resource_defs(
            text,
            &copied,
            expanded,
            name_prefix,
            uri_parts,
            group_controller.clone(),
            namespace,
        );
        if defs.is_empty() {
            return Vec::new();
        }
        all.extend(defs);
    }
    all
}

fn resource_defs(
    text: &str,
    chain: &Chain,
    verb: &Segment,
    name_prefix: &str,
    uri_parts: &[&str],
    group_controller: Option<String>,
    namespace: &str,
) -> Vec<RouteDef> {
    let Some((resource, offset)) = verb
        .args
        .first()
        .and_then(|r| php::literal(text, r.clone()))
    else {
        return Vec::new();
    };
    let controller = verb
        .args
        .get(1)
        .and_then(|r| class_value(text, r.clone(), !namespace.is_empty()))
        .or(group_controller)
        .map(|class| namespace_action(&class, namespace));
    let singleton = matches!(verb.name.as_str(), "singleton" | "apiSingleton");
    let api = matches!(verb.name.as_str(), "apiResource" | "apiSingleton");
    if singleton
        && chain
            .segments
            .iter()
            .any(|segment| segment.start > verb.start && segment.name == "shallow")
    {
        return Vec::new();
    }

    let options = match resource_options(text, chain, verb) {
        Some(options) => options,
        None => return Vec::new(),
    };
    // A URI prefix separated by `/` does not participate in the resource name.
    let (path_prefix, base_name) = resource.rsplit_once('/').unwrap_or(("", resource));
    let segments: Vec<_> = base_name.split('.').collect();
    if segments.iter().any(|name| {
        name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    }) {
        return Vec::new();
    }
    let wildcard = |name: &str| {
        options
            .parameters
            .get(name)
            .cloned()
            .unwrap_or_else(|| resource_wildcard(name))
            .replace('-', "_")
    };
    let parameter = format!("{{{}}}", wildcard(segments.last().unwrap()));
    let base_uri = segments
        .iter()
        .enumerate()
        .map(|(i, name)| {
            if i + 1 < segments.len() {
                format!("{name}/{{{}}}", wildcard(name))
            } else {
                (*name).to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("/");

    let singleton_actions = [
        ("show", "GET|HEAD", ""),
        ("edit", "GET|HEAD", "/edit"),
        ("update", "PUT|PATCH", ""),
        ("create", "GET|HEAD", "/create"),
        ("store", "POST", ""),
        ("destroy", "DELETE", ""),
    ];
    let actions = if singleton {
        singleton_actions.as_slice()
    } else {
        RESOURCE_ACTIONS
    };
    actions
        .iter()
        .filter(|(action, _, _)| {
            !singleton
                || match *action {
                    "show" | "edit" | "update" => true,
                    "create" | "store" => options.creatable,
                    "destroy" => options.creatable || options.destroyable,
                    _ => false,
                }
        })
        .filter(|(action, _, _)| {
            !(api && options.only.is_none() && matches!(*action, "create" | "edit"))
        })
        .filter(|(action, _, _)| {
            options
                .only
                .as_ref()
                .is_none_or(|o| o.iter().any(|a| a == action))
        })
        .filter(|(action, _, _)| {
            options
                .except
                .as_ref()
                .is_none_or(|e| !e.iter().any(|a| a == action))
        })
        .map(|(action, methods, suffix)| {
            let member = if singleton {
                matches!(*action, "edit" | "update" | "destroy")
            } else {
                matches!(*action, "show" | "edit" | "update" | "destroy")
            };
            let shallow = options.shallow && member;
            let resource_name = if shallow {
                *segments.last().unwrap()
            } else {
                base_name
            };
            let name = options.names.get(*action).cloned().unwrap_or_else(|| {
                format!(
                    "{}.{}",
                    options.name_base.as_deref().unwrap_or(resource_name),
                    action
                )
            });
            let name = if options.name_prefix.is_empty() {
                name
            } else {
                format!("{}.{name}", options.name_prefix)
            };
            let mut parts = uri_parts.to_vec();
            parts.push(path_prefix);
            let path = format!(
                "{}{}",
                if shallow {
                    *segments.last().unwrap()
                } else {
                    base_uri.as_str()
                },
                suffix.replace("{}", &parameter)
            );
            parts.push(&path);

            RouteDef {
                methods: (*methods).to_owned(),
                uri: join_uri(&parts),
                name: Some(format!("{name_prefix}{name}")),
                action: controller
                    .as_deref()
                    .map_or_else(|| (*action).to_owned(), |c| format!("{c}@{action}")),
                controller: controller.clone().map(|c| (c, (*action).to_owned())),
                offset,
                width: resource.len(),
            }
        })
        .collect()
}

/// `use` lines of a routes file: the namespace each short class name is imported from.
pub fn imports(masked: &str) -> Vec<(String, String)> {
    masked
        .lines()
        .filter_map(|line| line.trim().strip_prefix("use ")?.strip_suffix(';'))
        .filter(|path| !path.contains(['{', ',', ' ']))
        .filter_map(|path| {
            let path = path.trim_start_matches('\\');
            let (namespace, class) = path.rsplit_once('\\')?;
            Some((class.to_owned(), namespace.to_owned()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(defs: &[RouteDef]) -> Vec<(&str, &str, &str)> {
        defs.iter()
            .map(|d| {
                (
                    d.name.as_deref().unwrap_or("-"),
                    d.uri.as_str(),
                    d.action.as_str(),
                )
            })
            .collect()
    }

    #[test]
    fn reads_named_routes_and_actions() {
        let text = "<?php\nRoute::get('/', fn () => 1)->name('home');\nRoute::post('/login', [AuthController::class, 'login'])->name('login');\nRoute::get('/old', ['as' => 'old', 'uses' => 'OldController@show']);\nRoute::view('/about', 'about')->name('about');\nRoute::get('/plain', 'PlainController@index');\n";
        let defs = parse(&crate::php::mask_comments(text));
        assert_eq!(
            names(&defs),
            [
                ("home", "/", "Closure"),
                ("login", "/login", "AuthController@login"),
                ("old", "/old", "OldController@show"),
                ("about", "/about", "view: about"),
                ("-", "/plain", "PlainController@index"),
            ]
        );
        assert_eq!(defs[1].methods, "POST");
        assert_eq!(
            defs[1].controller,
            Some(("AuthController".into(), "login".into()))
        );
        assert_eq!(
            &text[defs[0].offset..defs[0].offset + defs[0].width],
            "home"
        );
    }

    #[test]
    fn stacks_group_prefixes() {
        let text = "<?php\nRoute::prefix('admin')->name('admin.')->middleware('auth')->group(function () {\n    Route::get('/users', [UserController::class, 'index'])->name('users.index');\n    Route::name('reports.')->prefix('reports')->group(function () {\n        Route::get('/', fn () => 1)->name('home');\n    });\n});\nRoute::group(['as' => 'api.', 'prefix' => 'api'], function () {\n    Route::get('/ping', 'P@ping')->name('ping');\n});\nRoute::controller(PostController::class)->group(function () {\n    Route::get('/posts', 'index')->name('posts');\n});\n";
        let defs = parse(text);
        assert_eq!(
            names(&defs),
            [
                ("admin.users.index", "/admin/users", "UserController@index"),
                ("admin.reports.home", "/admin/reports", "Closure"),
                ("api.ping", "/api/ping", "P@ping"),
                ("posts", "/posts", "PostController@index"),
            ]
        );
    }

    #[test]
    fn expands_resources() {
        let text = "<?php\nRoute::resource('photos', PhotoController::class)->only(['index', 'show']);\nRoute::apiResource('admin/posts', 'PostController', ['except' => ['destroy']]);\nRoute::resource('comments', CommentController::class)->names(['index' => 'all.comments'])->only(['index']);\nRoute::name('v1.')->group(function () { Route::resource('tags', TagController::class)->only(['edit']); });\n";
        let defs = parse(text);
        let listed: Vec<_> = defs
            .iter()
            .map(|d| {
                (
                    d.name.as_deref().unwrap(),
                    d.methods.as_str(),
                    d.uri.as_str(),
                )
            })
            .collect();
        assert_eq!(
            listed,
            [
                ("photos.index", "GET|HEAD", "/photos"),
                ("photos.show", "GET|HEAD", "/photos/{photo}"),
                ("posts.index", "GET|HEAD", "/admin/posts"),
                ("posts.store", "POST", "/admin/posts"),
                ("posts.show", "GET|HEAD", "/admin/posts/{post}"),
                ("posts.update", "PUT|PATCH", "/admin/posts/{post}"),
                ("all.comments", "GET|HEAD", "/comments"),
                ("v1.tags.edit", "GET|HEAD", "/tags/{tag}/edit"),
            ]
        );
    }

    #[test]
    fn survives_half_typed_routes() {
        let text = "Route::prefix('a')->name('b.')->group(function () { Route::get('/x', [A::class, 'y'])->name('z'); Route::resource('r', R::class)->only(['index";
        for end in 0..=text.len() {
            parse(&text[..end]);
        }
    }

    #[test]
    fn reads_class_imports() {
        let imports = imports(
            "<?php\nuse App\\Http\\Controllers\\PostController;\nuse Foo\\{A, B};\nuse Bar\\Baz as Q;\n",
        );
        assert_eq!(
            imports,
            [(
                "PostController".to_owned(),
                "App\\Http\\Controllers".to_owned()
            )]
        );
    }
}

fn mount_path(text: &str, range: Range<usize>, dir: &str) -> Option<String> {
    let range = php::trimmed(text, range);
    let code = &text[range];
    let value = if let Some(rest) = code.strip_prefix("__DIR__") {
        let tail = rest.trim_start().strip_prefix('.')?.trim();
        let value = decoded_group_literal(tail, 0..tail.len())?;
        format!("{dir}/{value}")
    } else if code.starts_with("base_path(") {
        let close = php::closing(code, 9)?;
        if close + 1 != code.len() {
            return None;
        }
        let args = php::split(code, 10..close);
        if args.len() != 1 {
            return None;
        }
        decoded_group_literal(code, args[0].clone())?
    } else {
        return None;
    };
    if value.starts_with('/')
        || value.contains(['\\', '$', ':'])
        || value.chars().any(char::is_control)
    {
        return None;
    }
    let mut parts = Vec::new();
    for part in value.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    let value = parts.join("/");
    (!value.is_empty() && value.ends_with(".php")).then_some(value)
}
#[derive(Default)]
struct MountAnalysis {
    known: std::collections::BTreeMap<String, MountContext>,
    unknown: std::collections::BTreeSet<String>,
    dynamic: bool,
}
impl MountAnalysis {
    fn insert(&mut self, path: String, context: Option<MountContext>) {
        if context.is_none() || self.known.contains_key(&path) || self.unknown.contains(&path) {
            self.known.remove(&path);
            self.unknown.insert(path);
        } else if let Some(context) = context {
            self.known.insert(path, context);
        }
    }
    fn finish(
        mut self,
    ) -> (
        std::collections::BTreeMap<String, MountContext>,
        std::collections::BTreeSet<String>,
    ) {
        if self.dynamic {
            self.unknown.extend(self.known.keys().cloned());
            self.known.clear();
        }
        (self.known, self.unknown)
    }
}
fn file_mounts(text: &str, dir: &str, body: Option<Range<usize>>, analysis: &mut MountAnalysis) {
    let all = chains(text);
    let regions = all
        .iter()
        .filter_map(|chain| region_of(text, chain))
        .collect::<Vec<_>>();
    for chain in &all {
        if body
            .as_ref()
            .is_some_and(|range| !range.contains(&chain.start))
        {
            continue;
        }
        let Some(group) = chain
            .segments
            .iter()
            .find(|segment| segment.name == "group")
        else {
            continue;
        };
        let Some(last) = group.args.last() else {
            continue;
        };
        if closure_body(text, last.clone()).is_some() {
            continue;
        }
        let Some(path) = mount_path(text, last.clone(), dir) else {
            analysis.dynamic = true;
            continue;
        };
        let enclosing = regions
            .iter()
            .filter(|region| region.body.contains(&chain.start))
            .collect::<Vec<_>>();
        let mut context = MountContext::default();
        let mut valid = true;
        for region in enclosing {
            valid &= region.valid && region.controller.is_none();
            context.prefix = join_uri(&[&context.prefix, &region.prefix])
                .trim_matches('/')
                .to_owned();
            context.name.push_str(&region.names);
            if let Some(namespace) = &region.namespace {
                context.namespace = merge_namespace(&context.namespace, namespace);
            }
        }
        if let Some(scope) = group_scope(text, chain, group) {
            valid &= scope.controller.is_none();
            context.prefix = join_uri(&[&context.prefix, &scope.context.prefix])
                .trim_matches('/')
                .to_owned();
            context.name.push_str(&scope.context.name);
            if let Some(namespace) = scope.namespace {
                context.namespace = merge_namespace(&context.namespace, &namespace);
            }
        } else {
            valid = false;
        }
        analysis.insert(path, valid.then_some(context));
    }
}
fn mount_analysis(
    bootstrap: &str,
    provider: &str,
) -> (
    Option<std::collections::BTreeMap<String, MountContext>>,
    std::collections::BTreeSet<String>,
) {
    use std::collections::{BTreeMap, BTreeSet};
    if bootstrap.len() > 128 << 10 || provider.len() > 128 << 10 {
        return (Some(BTreeMap::new()), BTreeSet::new());
    }
    let source = php::mask_comments(bootstrap);
    let Some(code) = crate::package_facts::code(&source) else {
        return (Some(BTreeMap::new()), BTreeSet::new());
    };
    let calls = code
        .match_indices("->withRouting(")
        .map(|(at, _)| at + "->withRouting".len())
        .collect::<Vec<_>>();
    let mut analysis = MountAnalysis::default();
    if !calls.is_empty() {
        if calls.len() != 1 {
            file_mounts(&source, "bootstrap", None, &mut analysis);
            analysis.dynamic = true;
            let (known, unknown) = analysis.finish();
            return (Some(known), unknown);
        }
        let open = calls[0];
        let Some(close) = php::closing(&source, open) else {
            return (Some(BTreeMap::new()), BTreeSet::new());
        };
        let mut args = BTreeMap::new();
        for range in php::split(&source, open + 1..close) {
            let Some(colon) = source[range.clone()].find(':') else {
                return (Some(BTreeMap::new()), BTreeSet::new());
            };
            let key = source[range.start..range.start + colon].trim();
            if args
                .insert(key, range.start + colon + 1..range.end)
                .is_some()
            {
                analysis.dynamic = true;
            }
        }
        let using = args
            .get("using")
            .filter(|range| source[(*range).clone()].trim() != "null");
        let then = args
            .get("then")
            .filter(|range| source[(*range).clone()].trim() != "null");
        // A literal then callback causes ApplicationBuilder to compose its
        // standard web/api loader even if a separate using callback was passed.
        let then_body = then.and_then(|range| closure_body(&source, range.clone()));
        let load_defaults = using.is_none() || then_body.is_some();
        for key in ["web", "api"] {
            let Some(range) = args.get(key) else {
                continue;
            };
            let paths = if let Some(array) = literal_array(&source, range.clone()) {
                php::items(&source, array)
                    .into_iter()
                    .map(|item| {
                        if item.key.is_none() {
                            mount_path(&source, item.value, "bootstrap")
                        } else {
                            None
                        }
                    })
                    .collect::<Option<Vec<_>>>()
            } else if source[range.clone()].trim() == "null" {
                Some(Vec::new())
            } else {
                mount_path(&source, range.clone(), "bootstrap").map(|path| vec![path])
            };
            let Some(paths) = paths else {
                analysis.dynamic = true;
                continue;
            };
            let prefix = if key == "api" {
                args.get("apiPrefix")
                    .map(|range| decoded_group_literal(&source, range.clone()))
                    .unwrap_or_else(|| Some("api".into()))
            } else {
                Some(String::new())
            };
            for path in paths {
                if load_defaults {
                    analysis.insert(
                        path,
                        prefix.as_ref().map(|prefix| MountContext {
                            prefix: prefix.trim_matches('/').into(),
                            ..Default::default()
                        }),
                    );
                }
            }
        }
        if let Some(then) = then {
            if let Some(body) = then_body {
                file_mounts(&source, "bootstrap", Some(body), &mut analysis);
            } else {
                let _ = then;
                analysis.dynamic = true;
            }
        } else if let Some(using) = using {
            if let Some(body) = closure_body(&source, using.clone()) {
                file_mounts(&source, "bootstrap", Some(body), &mut analysis);
            } else {
                analysis.dynamic = true;
            }
        }
        file_mounts(
            &php::mask_comments(provider),
            "app/Providers",
            None,
            &mut analysis,
        );
        let (known, unknown) = analysis.finish();
        return (Some(known), unknown);
    }
    if source.trim().is_empty() && provider.trim().is_empty() {
        return (None, BTreeSet::new());
    }
    file_mounts(&source, "bootstrap", None, &mut analysis);
    file_mounts(
        &php::mask_comments(provider),
        "app/Providers",
        None,
        &mut analysis,
    );
    if analysis.known.is_empty()
        && analysis.unknown.is_empty()
        && !analysis.dynamic
        && provider.trim().is_empty()
    {
        return (None, BTreeSet::new());
    }
    let (known, unknown) = analysis.finish();
    (Some(known), unknown)
}
/// Known mount scopes; `Some` is authoritative even when empty.
pub fn application_mount_contexts(
    bootstrap: &str,
    provider: &str,
) -> Option<std::collections::BTreeMap<String, MountContext>> {
    mount_analysis(bootstrap, provider).0
}
/// Literal file targets whose effective scope is unknown or multiply mounted.
/// Consumers must withhold them rather than fall back to unseeded local names.
pub fn application_mount_unknown_paths(
    bootstrap: &str,
    provider: &str,
) -> std::collections::BTreeSet<String> {
    mount_analysis(bootstrap, provider).1
}
/// Compatibility URI-only projection for consumers not yet using scoped names.
pub fn application_mounts(
    bootstrap: &str,
    provider: &str,
) -> Option<std::collections::BTreeMap<String, String>> {
    application_mount_contexts(bootstrap, provider).map(|mounts| {
        mounts
            .into_iter()
            .map(|(path, context)| (path, context.prefix))
            .collect()
    })
}

#[cfg(test)]
mod mount_tests {
    use super::*;
    #[test]
    fn modern_mounts_and_unknowns() {
        let source = "<?php return Application::configure()->withRouting(web: __DIR__.'/../routes/web.php', api: __DIR__.'/../routes/api.php');";
        let m = application_mounts(source, "").unwrap();
        assert_eq!(m.get("routes/api.php").map(String::as_str), Some("api"));
        assert_eq!(m.get("routes/web.php").map(String::as_str), Some(""));
        for prefix in ["'v2'", "''", "$prefix", "'v2' . suffix()"] {
            let s = format!(
                "<?php Application::configure()->withRouting(api: __DIR__.'/../routes/api.php', apiPrefix: {prefix});"
            );
            let m = application_mounts(&s, "").unwrap();
            assert_eq!(
                m.get("routes/api.php").map(String::as_str),
                match prefix {
                    "'v2'" => Some("v2"),
                    "''" => Some(""),
                    _ => None,
                }
            );
        }
        assert!(
            application_mounts(
                "<?php // ->withRouting(api: x)\n $x='->withRouting(api: x)';",
                ""
            )
            .is_none()
        );
        assert!(
            application_mounts(
                "<?php $a->withRouting(using: fn()=>1, api: __DIR__.'/../routes/api.php');",
                ""
            )
            .unwrap()
            .is_empty()
        );
    }
    #[test]
    fn legacy_literal_mounts() {
        let m = application_mounts("", "<?php Route::middleware('api')->prefix('api')->group(base_path('routes/api.php')); Route::middleware('web')->group(base_path('routes/web.php'));").unwrap();
        assert_eq!(m.get("routes/api.php").map(String::as_str), Some("api"));
        assert_eq!(m.get("routes/web.php").map(String::as_str), Some(""));
    }
    #[test]
    fn custom_facade_methods_use_declared_indices_groups_and_http_verbs() {
        use crate::metadata::RouteMethod;
        let custom = std::collections::BTreeMap::from([(
            "apipost".into(),
            RouteMethod {
                name: "apiPost".into(),
                url_index: Some(2),
                controller_index: Some(3),
                http_method: "POST".into(),
            },
        )]);
        let source = "<?php use Illuminate\\Support\\Facades\\Route as R; R::prefix('admin')->name('admin.')->group(function(){ R::apiPost('ignored','/users',[PostController::class,'store'])->name('users'); });";
        let defs = parse_with_custom(source, &custom);
        assert_eq!(defs.len(), 1, "{defs:?}");
        assert_eq!(
            (
                defs[0].methods.as_str(),
                defs[0].uri.as_str(),
                defs[0].name.as_deref(),
                defs[0].action.as_str()
            ),
            (
                "POST",
                "/admin/users",
                Some("admin.users"),
                "PostController@store"
            )
        );
        assert_eq!(
            &source[defs[0].offset..defs[0].offset + defs[0].width],
            "users"
        );
        for source in [
            "<?php use Other\\Route as R; R::apiPost('ignored','/x','P@x')->name('bad');",
            "<?php $x=\"Route::apiPost('ignored','/x','P@x')->name('bad');\";",
            "<?php Route::apiPost('ignored',$url,'P@x')->name('bad');",
            "<?php class Route{} Route::apiPost('ignored','/x','P@x')->name('bad');",
        ] {
            assert!(parse_with_custom(source, &custom).is_empty(), "{source}");
        }
    }
}

#[cfg(test)]
mod registration_semantics_tests {
    use super::*;
    fn inventory(source: &str) -> Vec<(String, String, String)> {
        parse(source)
            .into_iter()
            .map(|route| (route.methods, route.uri, route.name.unwrap_or_default()))
            .collect()
    }
    #[test]
    fn genuine_route_aliases_fqns_case_variants_and_foreign_decoys() {
        let source = r#"<?php
use Illuminate\Support\Facades\Route as Router;
Router::PREFIX('v1')->NAME('api.')->GROUP(function(){Router::GET('/ping',fn()=>1)->NAME('ping');});
\Illuminate\Support\Facades\Route::post('/post',fn()=>1)->name('post');
$example="Route::get('/fake', fn()=>1)->name('fake');";
"#;
        assert_eq!(
            inventory(source),
            [
                ("GET|HEAD".into(), "/v1/ping".into(), "api.ping".into()),
                ("POST".into(), "/post".into(), "post".into())
            ]
        );
        for source in [
            "<?php use Vendor\\Route; Route::get('/fake',fn()=>1);",
            "<?php class Route{} Route::get('/fake',fn()=>1);",
            "<?php $x=\"Route::get('/fake',fn()=>1);\";",
        ] {
            assert!(parse(source).is_empty(), "{source}");
        }
    }
    #[test]
    fn nested_resources_have_distinct_parameters_and_shallow_member_routes() {
        let source = r#"<?php
Route::resource('photos.comments',CommentController::class)->parameters(['photos'=>'image','comments'=>'reply'])->shallow()->only('index','show','edit')->name('show','reply.detail');
"#;
        assert_eq!(
            inventory(source),
            [
                (
                    "GET|HEAD".into(),
                    "/photos/{image}/comments".into(),
                    "photos.comments.index".into()
                ),
                (
                    "GET|HEAD".into(),
                    "/comments/{reply}".into(),
                    "reply.detail".into()
                ),
                (
                    "GET|HEAD".into(),
                    "/comments/{reply}/edit".into(),
                    "comments.edit".into()
                )
            ]
        );
        let default = inventory("<?php Route::resource('photos.comments',C::class)->only('show');");
        assert_eq!(default[0].1, "/photos/{photo}/comments/{comment}");
    }
    #[test]
    fn resource_options_names_variadic_filters_custom_parameters_and_overrides() {
        let source = r#"<?php
Route::resource('admin/people.news-items',C::class,['as'=>'staff','only'=>['show','destroy'],'parameters'=>['people'=>'author']])->parameter('news-items','story-id')->except('destroy')->names('stories');
Route::apiResource('categories',C::class)->only(['index'])->only('show');
"#;
        assert_eq!(
            inventory(source),
            [
                (
                    "GET|HEAD".into(),
                    "/admin/people/{author}/news-items/{story_id}".into(),
                    "staff.stories.show".into()
                ),
                (
                    "GET|HEAD".into(),
                    "/categories/{category}".into(),
                    "categories.show".into()
                )
            ]
        );
        for source in [
            "<?php Route::resource('posts',C::class)->only($actions);",
            "<?php Route::resource('posts',C::class)->parameters($map);",
            "<?php Route::resource('posts',C::class,['only'=>['show',dynamic()]]);",
            "<?php Route::prefix($prefix)->group(function(){Route::get('/x',fn()=>1);});",
        ] {
            assert!(parse(source).is_empty(), "{source}");
        }
    }
    #[test]
    fn parameters_at_includes_group_parameters_and_ignores_bindings_and_optionality() {
        let source = "<?php Route::prefix('teams/{team:slug}')->group(function(){Route::get('/posts/{post}/{tab?}',fn()=>1)->where('');});";
        assert_eq!(
            parameters_at(source, source.find("where('").unwrap() + 7),
            ["team", "post", "tab"]
        );
        let source = "<?php Route::resource('photos.comments',C::class)->parameters(['photos'=>'image'])->where('');";
        assert_eq!(
            parameters_at(source, source.find("where('").unwrap() + 7),
            ["image", "comment"]
        );
        let dynamic = "<?php Route::get($url,fn()=>1)->where('');";
        assert!(parameters_at(dynamic, dynamic.find("where('").unwrap() + 7).is_empty());
    }
    #[test]
    fn bulk_resources_and_singletons_preserve_group_prefix_names_and_source_ranges() {
        let source = r#"<?php
use Illuminate\Support\Facades\Route as R;
R::prefix('v2')->name('v2.')->group(function(){
    R::resources(['photos'=>PhotoController::class,'teams.posts'=>'PostController'],['only'=>['show'],'parameters'=>['teams'=>'group']]);
    R::apiResources(['categories'=>CategoryController::class],['only'=>['index','show']]);
    R::singletons(['settings'=>SettingsController::class,'teams.profile'=>ProfileController::class],['only'=>['show']]);
    R::apiSingletons(['account'=>AccountController::class]);
});
"#;
        assert_eq!(
            inventory(source),
            [
                (
                    "GET|HEAD".into(),
                    "/v2/photos/{photo}".into(),
                    "v2.photos.show".into()
                ),
                (
                    "GET|HEAD".into(),
                    "/v2/teams/{group}/posts/{post}".into(),
                    "v2.teams.posts.show".into()
                ),
                (
                    "GET|HEAD".into(),
                    "/v2/categories".into(),
                    "v2.categories.index".into()
                ),
                (
                    "GET|HEAD".into(),
                    "/v2/categories/{category}".into(),
                    "v2.categories.show".into()
                ),
                (
                    "GET|HEAD".into(),
                    "/v2/settings".into(),
                    "v2.settings.show".into()
                ),
                (
                    "GET|HEAD".into(),
                    "/v2/teams/{team}/profile".into(),
                    "v2.teams.profile.show".into()
                ),
                (
                    "GET|HEAD".into(),
                    "/v2/account".into(),
                    "v2.account.show".into()
                ),
                (
                    "PUT|PATCH".into(),
                    "/v2/account".into(),
                    "v2.account.update".into()
                ),
            ]
        );
        for def in parse(source) {
            assert!(
                source
                    .get(def.offset..def.offset + def.width)
                    .is_some_and(|value| [
                        "photos",
                        "teams.posts",
                        "categories",
                        "settings",
                        "teams.profile",
                        "account"
                    ]
                    .contains(&value))
            );
        }
    }
    #[test]
    fn singleton_creatable_destroyable_api_filters_and_explicit_only_override_match_registrar() {
        let source = r#"<?php
Route::singleton('teams.profile',Profile::class)->creatable()->except('edit');
Route::singleton('settings',Settings::class)->destroyable()->only('show','destroy');
Route::apiSingleton('teams.avatar',Avatar::class)->creatable();
Route::apiSingleton('editor',Editor::class)->only('edit');
Route::apiResource('drafts',Draft::class)->only('create');
"#;
        let listed = inventory(source);
        assert_eq!(
            listed
                .iter()
                .map(|(_, uri, name)| (uri.as_str(), name.as_str()))
                .collect::<Vec<_>>(),
            [
                ("/teams/{team}/profile", "teams.profile.show"),
                ("/teams/{team}/profile", "teams.profile.update"),
                ("/teams/{team}/profile/create", "teams.profile.create"),
                ("/teams/{team}/profile", "teams.profile.store"),
                ("/teams/{team}/profile", "teams.profile.destroy"),
                ("/settings", "settings.show"),
                ("/settings", "settings.destroy"),
                ("/teams/{team}/avatar", "teams.avatar.show"),
                ("/teams/{team}/avatar", "teams.avatar.update"),
                ("/teams/{team}/avatar", "teams.avatar.store"),
                ("/teams/{team}/avatar", "teams.avatar.destroy"),
                ("/editor/edit", "editor.edit"),
                ("/drafts/create", "drafts.create"),
            ]
        );
    }
    #[test]
    fn bulk_dynamic_duplicate_or_nonliteral_options_and_controllers_are_withheld() {
        for source in [
            "<?php Route::resources($resources);",
            "<?php Route::resources(['posts'=>$controller]);",
            "<?php Route::resources(['posts'=>C::class,'posts'=>Other::class]);",
            "<?php Route::resources(['posts'=>C::class],$options);",
            "<?php Route::resources(['posts'=>C::class]+other());",
            "<?php Route::resource('posts',C::class,['only'=>['show']]+other());",
            "<?php Route::singletons(['profile'=>C::class],['creatable'=>$flag]);",
            "<?php Route::apiSingleton('profile',C::class)->shallow();",
        ] {
            assert!(parse(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn resource_offsets_are_registration_ranges_not_expansion_counts_or_ordinary_routes() {
        let source = r#"<?php
use Illuminate\Support\Facades\Route as R;
R::prefix('admin')->group(function(){
 R::resource('posts',Post::class)->only('show');
 R::apiResources(['photos'=>Photo::class,'categories'=>Category::class],['only'=>['index','show']]);
 R::singleton('profile',Profile::class);
 R::apiSingletons(['account'=>Account::class]);
 R::get('/ordinary',fn()=>1)->name('ordinary');
 R::resource($resource,Dynamic::class);
});
"#;
        let offsets = resource_offsets(source);
        assert_eq!(offsets.len(), 5);
        let names = offsets
            .into_iter()
            .map(|offset| source[offset..].split(['\'', '"']).next().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            ["posts", "photos", "categories", "profile", "account"]
        );
        assert!(
            resource_offsets("<?php use Vendor\\Route; Route::resource('fake',C::class);")
                .is_empty()
        );
    }
    #[test]
    fn explicitly_registered_route_paths_may_live_outside_routes_without_escaping_project() {
        let modern = r#"<?php Application::configure()->withRouting(web:__DIR__.'/../app/Http/routes.php',api:[base_path('custom/routes.php')],apiPrefix:'v3');"#;
        let mounts = application_mounts(modern, "").unwrap();
        assert_eq!(
            mounts.get("app/Http/routes.php").map(String::as_str),
            Some("")
        );
        assert_eq!(
            mounts.get("custom/routes.php").map(String::as_str),
            Some("v3")
        );
        let legacy = r#"<?php Route::prefix('legacy')->group(base_path('custom/routes.php')); Route::group(__DIR__.'/../Http/routes.php');"#;
        let mounts = application_mounts("", legacy).unwrap();
        assert_eq!(
            mounts.get("custom/routes.php").map(String::as_str),
            Some("legacy")
        );
        assert_eq!(
            mounts.get("app/Http/routes.php").map(String::as_str),
            Some("")
        );
        for target in [
            "../outside.php",
            "/outside.php",
            "custom/routes.txt",
            "custom/../../outside.php",
            "custom/route\\s.php",
        ] {
            let source =
                format!("<?php Application::configure()->withRouting(web:base_path('{target}'));");
            assert!(
                application_mounts(&source, "").unwrap().is_empty(),
                "{target}"
            );
        }
    }

    #[test]
    fn resource_name_keys_are_available_before_custom_parameter_map_can_expand() {
        for source in [
            "<?php Route::resource('teams.posts',C::class)->parameters(['' => 'item']);",
            "<?php Route::apiResource('admin/teams.posts',C::class)->parameter('', 'item');",
        ] {
            let caret = source.rfind("''").unwrap() + 1;
            assert_eq!(
                resource_names_at(source, caret),
                ["teams", "posts"],
                "{source}"
            );
        }
        let source = "<?php Route::resource($resource,C::class)->parameter('', 'item');";
        assert!(resource_names_at(source, source.find("''").unwrap() + 1).is_empty());
        let source = "<?php Route::get('/teams/{team}',C::class)->where('');";
        assert!(resource_names_at(source, source.find("''").unwrap() + 1).is_empty());
    }

    #[test]
    fn bootstrap_array_mounts_null_callbacks_and_nested_provider_prefixes_are_literal() {
        let bootstrap = r#"<?php Application::configure()->withRouting(using:null,web:[__DIR__.'/../routes/web.php',base_path('routes/admin.php')],api:[base_path('routes/api.php'),base_path('routes/mobile.php')],apiPrefix:'v2',then:null);"#;
        let mounts = application_mounts(bootstrap, "").unwrap();
        assert_eq!(mounts.get("routes/admin.php").map(String::as_str), Some(""));
        assert_eq!(
            mounts.get("routes/mobile.php").map(String::as_str),
            Some("v2")
        );
        let provider = r#"<?php use Illuminate\Support\Facades\Route as Router; Router::prefix('v1')->group(function(){Router::prefix('admin')->group(base_path('routes/admin.php'));});"#;
        assert_eq!(
            application_mounts("", provider)
                .unwrap()
                .get("routes/admin.php")
                .map(String::as_str),
            Some("v1/admin")
        );
        let dynamic = r#"<?php Route::prefix($prefix)->group(function(){Route::group(base_path('routes/admin.php'));});"#;
        assert!(application_mounts("", dynamic).unwrap().is_empty());
    }
    #[test]
    #[ignore = "requires LARAVEL_ROUTE_FIXTURE vendor autoload; compares against actual Router registrations"]
    fn actual_laravel_resource_inventory_matches_literal_registration_semantics() {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let fixture = std::env::var("LARAVEL_ROUTE_FIXTURE")
            .expect("set LARAVEL_ROUTE_FIXTURE to Laravel project");
        let registrations = r#"
Route::resource('photos.comments', 'CommentController')->parameters(['photos'=>'image','comments'=>'reply'])->shallow()->only('index','show','edit')->name('show','reply.detail');
Route::resource('admin/people.news-items', 'C', ['as'=>'staff','only'=>['show','destroy'],'parameters'=>['people'=>'author']])->parameter('news-items','story-id')->except('destroy')->names('stories');
Route::apiResource('categories', 'C')->only(['index'])->only('show');
Route::resources(['albums'=>'AlbumController','users.posts'=>'PostController'],['only'=>['show'],'parameters'=>['users'=>'author']]);
Route::apiResources(['jobs'=>'JobController'],['only'=>['index','destroy']]);
Route::singleton('teams.profile','ProfileController')->creatable()->except('edit');
Route::singleton('settings','SettingsController')->destroyable()->only('show','destroy');
Route::apiSingleton('teams.avatar','AvatarController')->creatable();
Route::apiSingleton('editor','EditorController')->only('edit');
Route::apiResource('drafts','DraftController')->only('create');
Route::singletons(['account'=>'AccountController','teams.icon'=>'IconController'],['only'=>['show']]);
Route::apiSingletons(['billing'=>'BillingController'],['creatable'=>true]);
"#;
        let expected = inventory(&format!("<?php\n{registrations}"));
        let autoload = serde_json::to_string(&format!("{fixture}/vendor/autoload.php")).unwrap();
        let php = format!(
            "<?php require {autoload}; $app=new Illuminate\\Container\\Container; $router=new Illuminate\\Routing\\Router(new Illuminate\\Events\\Dispatcher($app),$app); Illuminate\\Support\\Facades\\Facade::setFacadeApplication($app); $app->instance('router',$router); class_alias(Illuminate\\Support\\Facades\\Route::class,'Route'); {registrations} $result=[]; foreach($router->getRoutes() as $route){{$result[]=[implode('|',$route->methods()),'/'.ltrim($route->uri(),'/'),$route->getName()??''];}} echo json_encode($result);"
        );
        let mut child = Command::new("php")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(php.as_bytes())
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let actual: Vec<(String, String, String)> = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(expected, actual);
    }
}

#[cfg(test)]
mod mounted_context_tests {
    use super::*;
    #[test]
    fn literal_provider_alias_name_prefix_and_namespace_seed_original_source_ranges() {
        let provider = r#"<?php use Illuminate\Support\Facades\Route as Router; Router::prefix('v2')->name('admin.')->namespace('App\Http\Controllers\Admin')->group(base_path('custom/admin.php'));"#;
        let mounts = application_mount_contexts("", provider).unwrap();
        let context = mounts.get("custom/admin.php").unwrap();
        assert_eq!(
            context,
            &MountContext {
                prefix: "v2".into(),
                name: "admin.".into(),
                namespace: "App\\Http\\Controllers\\Admin".into()
            }
        );
        let source = r#"<?php Route::get('/users','UserController@show')->name('users.show');"#;
        let defs = parse_with_context(source, &Default::default(), context);
        assert_eq!(defs.len(), 1);
        assert_eq!(
            (&defs[0].uri, defs[0].name.as_deref(), &defs[0].action),
            (
                &"/v2/users".to_owned(),
                Some("admin.users.show"),
                &"App\\Http\\Controllers\\Admin\\UserController@show".to_owned()
            )
        );
        assert_eq!(
            &source[defs[0].offset..defs[0].offset + defs[0].width],
            "users.show"
        );
        assert_eq!(
            controller_at(
                source,
                &Default::default(),
                context,
                source.find("UserController").unwrap() + 1
            ),
            Some((
                "App\\Http\\Controllers\\Admin\\UserController".into(),
                "show".into()
            ))
        );
    }
    #[test]
    fn nested_group_namespaces_compose_and_absolute_resets_preserve_external_class_arrays() {
        let context = MountContext {
            prefix: "api".into(),
            name: "api.".into(),
            namespace: "App\\Controllers".into(),
        };
        let source = r#"<?php
use External\Outside as Imported;
Route::prefix('teams/{team}')->name('teams.')->namespace('Admin')->group(function(){
    Route::get('/string','UsersController@show')->name('string');
    Route::get('/array',[Imported::class,'show'])->name('array');
    Route::get('/absolute','\External\Outside@show')->name('absolute');
    Route::namespace('\Standalone')->group(function(){Route::get('/reset','Handler@show')->name('reset');});
});
"#;
        let defs = parse_with_context(source, &Default::default(), &context);
        assert_eq!(defs.len(), 4, "{defs:?}");
        assert_eq!(
            defs.iter()
                .map(|def| def.action.as_str())
                .collect::<Vec<_>>(),
            [
                "App\\Controllers\\Admin\\UsersController@show",
                "External\\Outside@show",
                "\\External\\Outside@show",
                "Standalone\\Handler@show"
            ]
        );
        assert_eq!(defs[1].name.as_deref(), Some("api.teams.array"));
        assert_eq!(defs[1].uri, "/api/teams/{team}/array");
        assert_eq!(
            controller_at(
                source,
                &Default::default(),
                &context,
                source.find("Imported::class").unwrap() + 1
            ),
            Some(("External\\Outside".into(), "show".into()))
        );
    }
    #[test]
    fn bootstrap_callbacks_and_provider_mounts_merge_without_unsafe_ambiguous_fallback() {
        let bootstrap = r#"<?php use Illuminate\Support\Facades\Route; Application::configure()->withRouting(web:__DIR__.'/../routes/web.php',then:function(){Route::prefix('extra')->name('extra.')->namespace('App\Extra')->group(base_path('custom/extra.php'));});"#;
        let provider = r#"<?php use Illuminate\Support\Facades\Route as R; R::name('outer.')->prefix('v1')->namespace('App\Controllers')->group(function(){R::name('inner.')->prefix('admin')->namespace('Admin')->group(base_path('custom/admin.php'));});"#;
        let mounts = application_mount_contexts(bootstrap, provider).unwrap();
        assert_eq!(mounts.len(), 3);
        assert_eq!(
            mounts.get("custom/admin.php"),
            Some(&MountContext {
                prefix: "v1/admin".into(),
                name: "outer.inner.".into(),
                namespace: "App\\Controllers\\Admin".into()
            })
        );
        assert_eq!(mounts.get("custom/extra.php").unwrap().name, "extra.");
        assert!(application_mount_unknown_paths(bootstrap, provider).is_empty());
        let repeated = r#"<?php Route::name('first.')->group(base_path('custom/repeated.php')); Route::name('second.')->group(base_path('custom/repeated.php')); Route::prefix($prefix)->group(base_path('custom/dynamic.php')); Route::group(base_path('custom/known.php'));"#;
        let known = application_mount_contexts("", repeated).unwrap();
        assert_eq!(
            known.keys().map(String::as_str).collect::<Vec<_>>(),
            ["custom/known.php"]
        );
        assert_eq!(
            application_mount_unknown_paths("", repeated),
            [
                "custom/dynamic.php".to_owned(),
                "custom/repeated.php".to_owned()
            ]
            .into()
        );
    }
    #[test]
    fn literal_array_group_scopes_and_arrow_callbacks_keep_parameter_braces_out_of_body_detection()
    {
        let source = r#"<?php Route::group(['prefix'=>'teams/{team}','as'=>'team.','namespace'=>'App\Team'],fn()=>Route::get('/members','MemberController@show')->name('members'));"#;
        let defs = parse(source);
        assert_eq!(defs.len(), 1, "{defs:?}");
        assert_eq!(defs[0].uri, "/teams/{team}/members");
        assert_eq!(defs[0].name.as_deref(), Some("team.members"));
        assert_eq!(defs[0].action, "App\\Team\\MemberController@show");
        let bootstrap = r#"<?php Application::configure()->withRouting(using:fn()=>Route::name('only.')->namespace('App\Only')->group(base_path('custom/only.php')));"#;
        let mounts = application_mount_contexts(bootstrap, "").unwrap();
        assert_eq!(mounts.get("custom/only.php").unwrap().name, "only.");
    }
    #[test]
    fn canonical_handlers_resolve_php_aliases_but_never_reinterpret_runtime_strings() {
        let source = r#"<?php
use App\Http\Controllers\PostController as P;
Route::get('/typed',[P::class,'show'])->name('typed');
Route::get('/literal','P@show')->name('literal');
Route::resource('posts',P::class)->only('index','show');
Route::resources(['other'=>P::class],['only'=>['show']]);
"#;
        let context = MountContext::default();
        let defs = parse_with_context(source, &Default::default(), &context);
        assert_eq!(
            controller_at(
                source,
                &Default::default(),
                &context,
                source.find("P::class").unwrap() + 1
            ),
            Some((
                "App\\Http\\Controllers\\PostController".into(),
                "show".into()
            ))
        );
        assert_eq!(
            controller_at(
                source,
                &Default::default(),
                &context,
                source.find("P@show").unwrap() + 1
            ),
            Some(("P".into(), "show".into()))
        );
        let handlers = defs
            .iter()
            .map(|def| canonical_controller(source, &Default::default(), &context, def).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            handlers,
            [
                (
                    "App\\Http\\Controllers\\PostController".into(),
                    "show".into()
                ),
                ("P".into(), "show".into()),
                (
                    "App\\Http\\Controllers\\PostController".into(),
                    "index".into()
                ),
                (
                    "App\\Http\\Controllers\\PostController".into(),
                    "show".into()
                ),
                (
                    "App\\Http\\Controllers\\PostController".into(),
                    "show".into()
                )
            ]
        );
    }
    #[test]
    fn literal_unmodified_framework_provider_namespace_property_is_proven_and_mutation_withheld() {
        let provider = r#"<?php
namespace App\Providers;
use Illuminate\Foundation\Support\Providers\RouteServiceProvider as ServiceProvider;
use Illuminate\Support\Facades\Route;
class RouteServiceProvider extends ServiceProvider {
 protected $namespace='App\Http\Controllers';
 public function boot(){ $this->routes(function(){Route::namespace($this->namespace)->name('legacy.')->group(base_path('app/Http/routes.php'));}); }
}
"#;
        let map = application_mount_contexts("", provider).unwrap();
        assert_eq!(
            map.get("app/Http/routes.php").unwrap().namespace,
            "App\\Http\\Controllers"
        );
        assert_eq!(map.get("app/Http/routes.php").unwrap().name, "legacy.");
        for source in [
            provider.replace(
                "$this->routes(function(){",
                "$this->namespace='Other'; $this->routes(function(){",
            ),
            provider.replace(
                "protected $namespace='App\\Http\\Controllers';",
                "protected $namespace=unknown();",
            ),
            provider.replace(
                "Illuminate\\Foundation\\Support\\Providers\\RouteServiceProvider",
                "Vendor\\RouteServiceProvider",
            ),
        ] {
            assert!(
                application_mount_contexts("", &source).unwrap().is_empty(),
                "{source}"
            );
            assert!(application_mount_unknown_paths("", &source).contains("app/Http/routes.php"));
        }
    }

    #[test]
    #[ignore = "requires LARAVEL_ROUTE_FIXTURE vendor autoload; actual external file namespace/name group differential"]
    fn actual_laravel_external_mount_matches_namespace_strings_alias_arrays_and_invokable_constants()
     {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let fixture = std::env::var("LARAVEL_ROUTE_FIXTURE").expect("set LARAVEL_ROUTE_FIXTURE");
        let source = r#"<?php
use Illuminate\Support\Facades\Route;
use External\Outside as Imported;
Route::get('/string','UsersController@show')->name('string');
Route::get('/array',[Imported::class,'show'])->name('array');
Route::get('/absolute','\External\Outside@show')->name('absolute');
Route::get('/invokable',Imported::class)->name('invokable');
Route::resource('items',Imported::class)->only('show');
Route::namespace('Admin')->name('admin.')->group(function(){Route::get('/nested','UsersController@show')->name('nested');});
"#;
        let context = MountContext {
            prefix: "v2".into(),
            name: "mounted.".into(),
            namespace: "App\\Controllers".into(),
        };
        let expected = parse_with_context(source, &Default::default(), &context)
            .into_iter()
            .map(|def| {
                (
                    def.methods,
                    def.uri,
                    def.name.unwrap_or_default(),
                    def.action,
                )
            })
            .collect::<Vec<_>>();
        let file = std::env::temp_dir().join(format!(
            "openidea-mounted-routes-{}-{}.php",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&file, source).unwrap();
        let autoload = serde_json::to_string(&format!("{fixture}/vendor/autoload.php")).unwrap();
        let path = serde_json::to_string(file.to_str().unwrap()).unwrap();
        let scope=serde_json::to_string(&serde_json::json!({"prefix":context.prefix,"as":context.name,"namespace":context.namespace}).to_string()).unwrap();
        let php = format!(
            "<?php namespace {{require {autoload};}} namespace External {{class Outside {{public function __invoke(){{}} public function show(){{}}}}}} namespace App\\Controllers\\External {{class Outside {{public function __invoke(){{}} public function show(){{}}}}}} namespace {{$app=new Illuminate\\Container\\Container; $router=new Illuminate\\Routing\\Router(new Illuminate\\Events\\Dispatcher($app),$app); Illuminate\\Support\\Facades\\Facade::setFacadeApplication($app); $app->instance('router',$router);$router->group(json_decode({scope},true),{path});$result=[];foreach($router->getRoutes() as $route){{$result[]=[implode('|',$route->methods()),'/'.ltrim($route->uri(),'/'),$route->getName()??'',$route->getAction('uses')];}}echo json_encode($result);}}"
        );
        let mut child = Command::new("php")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(php.as_bytes())
            .unwrap();
        let result = child.wait_with_output().unwrap();
        let _ = std::fs::remove_file(&file);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let actual: Vec<(String, String, String, String)> =
            serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(expected, actual);
    }
}
