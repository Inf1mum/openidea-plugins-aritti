//! Explicit public value-object members and typed cast getters; no arbitrary PHP evaluation.
use super::{Entry, Function, Kind, Names, Source, entry};
use std::collections::{BTreeMap, BTreeSet};
#[path = "eloquent_objects/call_site.rs"]
mod call_site;
#[path = "eloquent_objects/inventory.rs"]
mod inventory;
pub(super) use call_site::{active_parameter, call_open};
#[path = "eloquent_objects/signatures.rs"]
mod signatures;
pub(super) use inventory::composed;
pub(super) use signatures::constructor;
pub(super) use signatures::parameters;
#[derive(Clone, Debug)]
pub(super) struct Method {
    pub target: String,
    pub static_method: bool,
    pub entry: Entry,
}
#[derive(Clone, Debug)]
pub(crate) struct Object {
    pub(super) container: super::container::Declared,
    pub full: String,
    pub path: String,
    pub(super) declaration: Entry,
    pub ambiguous: bool,
    pub(super) metadata_sources: Vec<String>,
    pub(super) is_interface: bool,
    pub(super) parents: Vec<String>,
    pub(super) cast_target: Option<String>,
    pub(super) properties: BTreeMap<String, Entry>,
    pub(super) methods: BTreeMap<String, Method>,
    pub(super) declared_methods: BTreeSet<String>,
    pub(super) declared_properties: BTreeSet<String>,
    signatures: BTreeMap<String, signatures::Signature>,
}
pub(crate) type Objects = BTreeMap<String, Object>;
pub(super) fn current(text: &str, path: &str) -> Option<Object> {
    let mut objects = Objects::new();
    cache(&mut objects, text, path);
    objects.into_values().next()
}
fn modifiers<'a>(source: &'a Source, function: &Function) -> Vec<&'a str> {
    let begin = (0..function.start)
        .rev()
        .find(|&i| ["{", "}", ";"].contains(&source.tokens[i].value))
        .map_or(0, |i| i + 1);
    source.tokens[begin..function.start]
        .iter()
        .map(|t| t.value)
        .collect()
}
pub(super) fn method(
    source: &Source,
    names: &Names,
    function: &Function,
    path: &str,
) -> Option<Method> {
    let mods = modifiers(source, function);
    if mods.iter().any(|m| {
        matches!(*m, "private" | "protected")
            || *m == "abstract"
                && !names
                    .class
                    .as_ref()
                    .is_some_and(|(_, at, _, _)| source.is(*at, "interface"))
    }) {
        return None;
    }
    let at = function.params.end + 1;
    let target = if source.is(at, ":") {
        let mut cursor = at + 1 + usize::from(source.is(at + 1, "?"));
        let mut targets = vec![];
        let mut separator = None;
        while cursor < function.body {
            let token = source.tokens.get(cursor)?;
            if token.kind != Kind::Name {
                return None;
            }
            let target = match token.value {
                "null" => {
                    cursor += 1;
                    if cursor < function.body {
                        if !source.is(cursor, "|") {
                            return None;
                        }
                        if separator.is_some_and(|separator| separator != '|') {
                            return None;
                        }
                        separator = Some('|');
                        cursor += 1;
                    }
                    continue;
                }
                "static" => "static".into(),
                "self" => {
                    if source.is(names.class.as_ref()?.1, "trait") {
                        "static".into()
                    } else {
                        names.class.as_ref()?.0.clone()
                    }
                }
                "parent" => names.class.as_ref()?.3.clone(),
                _ => names.resolve(token.value),
            };
            if targets.len() >= 8 {
                return None;
            }
            targets.push(target);
            cursor += 1;
            if cursor < function.body {
                let next = if source.is(cursor, "|") {
                    '|'
                } else if source.is(cursor, "&") {
                    '&'
                } else {
                    return None;
                };
                if separator.is_some_and(|separator| separator != next) {
                    return None;
                }
                separator = Some(next);
                cursor += 1;
                if cursor >= function.body {
                    return None;
                }
            }
        }
        if separator == Some('&')
            && source.tokens[at + 1..function.body]
                .iter()
                .any(|token| matches!(token.value, "?" | "null" | "self" | "parent" | "static"))
        {
            return None;
        }
        let count = targets.len();
        targets.sort();
        targets.dedup();
        if targets.len() != count {
            return None;
        }
        targets.join(&separator.unwrap_or('|').to_string())
    } else {
        String::new()
    };
    Some(Method {
        target,
        static_method: mods.contains(&"static"),
        entry: entry(source, path, function.name?),
    })
}
pub(crate) fn cache(objects: &mut Objects, text: &str, path: &str) {
    let Some(source) = Source::new(text) else {
        return;
    };
    let Some(names) = Names::parse(&source) else {
        return;
    };
    let Some((full, start, body, _)) = &names.class else {
        return;
    };
    if !source.is(*start, "class") && !source.is(*start, "interface") && !source.is(*start, "trait")
    {
        return;
    }
    let mut object = Object {
        container: super::container::declared(&source, &names, path),
        full: full.clone(),
        path: path.into(),
        declaration: entry(&source, path, start + 1),
        ambiguous: false,
        metadata_sources: Vec::new(),
        is_interface: source.is(*start, "interface"),
        parents: {
            let mut parents = Vec::new();
            let mut collecting = false;
            for token in &source.tokens[start + 2..*body] {
                if matches!(token.value, "extends" | "implements") {
                    collecting = true;
                    continue;
                }
                if collecting && token.kind == super::Kind::Name {
                    parents.push(names.resolve(token.value));
                }
            }
            parents
        },
        cast_target: None,
        properties: BTreeMap::new(),
        methods: BTreeMap::new(),
        declared_methods: BTreeSet::new(),
        declared_properties: BTreeSet::new(),
        signatures: BTreeMap::new(),
    };
    let caster = source.tokens[*start..*body].iter().any(|token| {
        names.resolve(token.value) == "Illuminate\\Contracts\\Database\\Eloquent\\CastsAttributes"
    });
    let mut functions = source.functions();
    for (at, token) in source
        .tokens
        .iter()
        .enumerate()
        .filter(|(at, token)| token.value == "function" && source.paths[*at] == vec![*body])
    {
        let _ = token;
        let name = at + 1 + usize::from(source.is(at + 1, "&"));
        let open = name + 1;
        if !source.is(open, "(") {
            continue;
        }
        let Some(close) = source.pairs.get(&open).copied() else {
            continue;
        };
        let Some(end) =
            (close + 1..source.tokens.len()).find(|at| source.is(*at, ";") || source.is(*at, "{"))
        else {
            continue;
        };
        if source.is(end, ";") {
            functions.push(Function {
                start: at,
                name: Some(name),
                params: open + 1..close,
                body: end,
                end,
            });
        }
    }
    for function in functions {
        if source.paths[function.start] != vec![*body] {
            continue;
        }
        let Some(at) = function.name else { continue };
        let name = source.tokens[at].value;
        object.declared_methods.insert(name.to_ascii_lowercase());
        let mods = modifiers(&source, &function);
        object.signatures.insert(
            name.to_ascii_lowercase(),
            signatures::Signature {
                parameters: parameters(&source, &names, &function),
                public: !mods
                    .iter()
                    .any(|m| matches!(*m, "private" | "protected" | "abstract")),
            },
        );
        if name == "__construct" {
            let mut begin = function.params.start;
            for at in function.params.clone() {
                if source.is(at, ",") {
                    begin = at + 1;
                }
                if source.tokens[at].kind == Kind::Var
                    && (begin..at).any(|i| source.is(i, "public"))
                    && !(begin..at).any(|i| source.is(i, "private") || source.is(i, "protected"))
                {
                    object.properties.insert(
                        source.tokens[at].value.trim_start_matches('$').into(),
                        entry(&source, path, at),
                    );
                }
            }
        }
        if let Some(info) = method(&source, &names, &function, path) {
            if caster && name == "get" && !info.static_method && !info.target.is_empty() {
                object.cast_target = Some(info.target.clone());
            }
            if !name.starts_with("__") {
                object.methods.insert(name.into(), info);
            }
        }
    }
    for (i, token) in source.tokens.iter().enumerate() {
        if token.kind != Kind::Var || source.paths[i] != vec![*body] {
            continue;
        }
        if source.pairs.iter().any(|(open, close)| {
            *open < i && i < *close && matches!(source.tokens[*open].value, "(" | "[")
        }) {
            continue;
        }
        let begin = (0..i)
            .rev()
            .find(|&j| [";", "{", "}"].contains(&source.tokens[j].value))
            .map_or(0, |j| j + 1);
        object
            .declared_properties
            .insert(token.value.trim_start_matches('$').into());
        if (begin..i).any(|j| source.is(j, "public"))
            && !(begin..i).any(|j| {
                source.is(j, "static") || source.is(j, "private") || source.is(j, "protected")
            })
        {
            object.properties.insert(
                token.value.trim_start_matches('$').into(),
                entry(&source, path, i),
            );
        }
    }
    objects
        .entry(full.to_ascii_lowercase())
        .and_modify(|old| {
            if old.path != path {
                old.ambiguous = true;
            }
        })
        .or_insert(object);
}

#[cfg(test)]
#[path = "eloquent_objects/tests.rs"]
mod tests;
