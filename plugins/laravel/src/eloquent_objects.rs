//! Explicit public value-object members and typed cast getters; no arbitrary PHP evaluation.
use super::{Entry, Function, Kind, Names, Source, entry};
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
pub(super) struct Method {
    pub target: String,
    pub static_method: bool,
    pub entry: Entry,
}
#[derive(Clone, Debug)]
pub(crate) struct Object {
    pub full: String,
    pub path: String,
    pub ambiguous: bool,
    pub(super) metadata_sources: Vec<String>,
    pub(super) is_interface: bool,
    pub(super) cast_target: Option<String>,
    pub(super) properties: BTreeMap<String, Entry>,
    pub(super) methods: BTreeMap<String, Method>,
}
pub(crate) type Objects = BTreeMap<String, Object>;
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
    if mods
        .iter()
        .any(|m| matches!(*m, "private" | "protected" | "abstract"))
    {
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
    if !source.is(*start, "class") && !source.is(*start, "interface") {
        return;
    }
    let mut object = Object {
        full: full.clone(),
        path: path.into(),
        ambiguous: false,
        metadata_sources: Vec::new(),
        is_interface: source.is(*start, "interface"),
        cast_target: None,
        properties: BTreeMap::new(),
        methods: BTreeMap::new(),
    };
    let caster = source.tokens[*start..*body].iter().any(|token| {
        names.resolve(token.value) == "Illuminate\\Contracts\\Database\\Eloquent\\CastsAttributes"
    });
    for function in source.functions() {
        if source.paths[function.start] != vec![*body] {
            continue;
        }
        let Some(at) = function.name else { continue };
        let name = source.tokens[at].value;
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
            if !name.starts_with("__") && !info.static_method {
                object.methods.insert(name.into(), info);
            }
        }
    }
    for (i, token) in source.tokens.iter().enumerate() {
        if token.kind != Kind::Var || source.paths[i] != vec![*body] {
            continue;
        }
        let begin = (0..i)
            .rev()
            .find(|&j| [";", "{", "}"].contains(&source.tokens[j].value))
            .map_or(0, |j| j + 1);
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
