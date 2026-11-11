//! Typed helper-generation facts using the same resolver as editor completion.
//! Ownership and ambiguity checks are retained; no UI detail strings are parsed.
use super::*;
#[derive(Clone, Debug)]
pub(crate) struct RelationFact {
    pub name: String,
    pub kind: String,
    pub target: String,
    pub many: bool,
    pub entry: Entry,
}
#[derive(Clone, Debug)]
pub(crate) struct CastFact {
    pub name: String,
    pub label: String,
    pub resolved: Option<String>,
    pub entry: Entry,
}
#[derive(Clone, Debug)]
pub(crate) struct ScopeFact {
    pub name: String,
    pub entry: Entry,
}
#[derive(Clone, Debug)]
pub(crate) struct Facts {
    pub table: Option<String>,
    pub relations: Vec<RelationFact>,
    pub casts: Vec<CastFact>,
    pub casts_complete: bool,
    pub scopes: Vec<ScopeFact>,
}

pub(crate) fn facts(index: &Index, full: &str, owns: &dyn Fn(&str) -> bool) -> Option<Facts> {
    let doc = Document {
        path: String::new(),
        language: "php".into(),
        version: 0,
        text: format!("<?php \\{full}::query()->"),
    };
    let context = context(&doc, text::position(&doc.text, doc.text.len()))?;
    let resolver = Resolver {
        context: &context,
        index,
        overlay: None,
        object_overlay:None,
        path: "",
        owns,
        owner_cache: RefCell::new(BTreeMap::new()),
    };
    resolver.get(full)?;
    let relations = resolver
        .relations(full)
        .into_iter()
        .filter_map(|(name, relation)| {
            if !resolver.owned(&relation.entry.path) {
                return None;
            }
            let target = resolver.get(&relation.target)?;
            Some(RelationFact {
                name: name.into(),
                kind: relation.kind.clone(),
                target: target.full.clone(),
                many: relation.many,
                entry: relation.entry.clone(),
            })
        })
        .collect();
    let casts = resolver.casts(full);
    let cast_facts = casts
        .as_ref()
        .into_iter()
        .flatten()
        .filter(|(_, cast)| resolver.owned(&cast.entry.path))
        .map(|(name, cast)| {
            let resolved = cast.target.as_ref().and_then(|target| {
                if let Some(info) = resolver.enumeration(target) {
                    return Some(info.full.clone());
                }
                let caster = resolver.object(target)?;
                let target = caster.cast_target.as_ref()?;
                if let Some(info) = resolver.get(target) {
                    return Some(info.full.clone());
                }
                if let Some(info) = resolver.enumeration(target) {
                    return Some(info.full.clone());
                }
                resolver.object(target).map(|object| object.full.clone())
            });
            CastFact {
                name: name.clone(),
                label: cast.label.clone(),
                resolved,
                entry: cast.entry.clone(),
            }
        })
        .collect();
    let scopes = resolver
        .scopes(full)
        .into_iter()
        .filter(|(_, entry)| resolver.owned(&entry.path))
        .map(|(name, entry)| ScopeFact {
            name: name.into(),
            entry: entry.clone(),
        })
        .collect();
    Some(Facts {
        table: resolver.table(full).map(str::to_owned),
        relations,
        casts: cast_facts,
        casts_complete: casts.is_some(),
        scopes,
    })
}

#[derive(Clone, Debug)]
pub(crate) struct Signature {
    pub parameters: Vec<String>,
    pub return_type: String,
}
/// Exact declared or locally proven scope signatures. The injected query
/// parameter is omitted; unsupported defaults/types/returns withhold the method.
pub(crate) fn scope_signature(source: &str, entry: &Entry, model_full: &str) -> Option<Signature> {
    let source = Source::new(source)?;
    let names = Names::parse(&source)?;
    let is_trait=names.class.as_ref().is_some_and(|(_,at,_,_)|source.is(*at,"trait"));
    let offset = text::offset(source.text, entry.at)?;
    let function = source.functions().into_iter().find(|f| {
        f.name
            .is_some_and(|name| source.tokens[name].start == offset)
    })?;
    let open = function.params.start.checked_sub(1)?;
    let parameters = php::arguments(source.text, source.tokens[open].start);
    let first = parameters.first()?;
    let first_tokens: Vec<_> = source
        .tokens
        .iter()
        .filter(|t| t.start >= first.start && t.end <= first.end)
        .collect();
    if first_tokens
        .iter()
        .any(|t| t.value == "=" || t.value == "..." || t.value == "&")
    {
        return None;
    }
    let variables = first_tokens
        .iter()
        .filter(|t| t.kind == Kind::Var)
        .copied()
        .collect::<Vec<_>>();
    let [query] = variables.as_slice() else {
        return None;
    };
    let query = query.value;
    let first_variable = first_tokens
        .iter()
        .position(|token| token.kind == Kind::Var)?;
    if first_variable > 0 {
        let declared = doc_type(&first_tokens[..first_variable], &names, model_full,is_trait)?;
        if ![
            "\\Illuminate\\Database\\Eloquent\\Builder",
            "\\Illuminate\\Contracts\\Database\\Eloquent\\Builder",
            "mixed",
            "object",
        ]
        .contains(&declared.trim_start_matches('?'))
        {
            return None;
        }
    }
    let mut result = Vec::new();
    for parameter in parameters.iter().skip(1) {
        let tokens: Vec<_> = source
            .tokens
            .iter()
            .filter(|t| t.start >= parameter.start && t.end <= parameter.end)
            .collect();
        let equals = tokens.iter().position(|t| t.value == "=");
        let head = &tokens[..equals.unwrap_or(tokens.len())];
        let variable = head.iter().position(|t| t.kind == Kind::Var)?;
        if variable + 1 != head.len() {
            return None;
        }
        let name = head[variable].value;
        let mut end = variable;
        let mut reference = "";
        let mut variadic = "";
        if end > 0 && head[end - 1].value == "..." {
            variadic = "...";
            end -= 1;
        }
        if end > 0 && head[end - 1].value == "&" {
            reference = "&";
            end -= 1;
        }
        let kind = doc_type(&head[..end], &names, model_full,is_trait)?;
        let default = if let Some(equals) = equals {
            if !variadic.is_empty() {
                return None;
            }
            let start = tokens.get(equals + 1)?.start;
            let raw = source.text[start..parameter.end].trim();
            let lower = raw.to_ascii_lowercase();
            let safe = matches!(lower.as_str(), "true" | "false" | "null" | "[]")
                || raw
                    .bytes()
                    .all(|b| b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.'))
                    && raw.parse::<f64>().is_ok()
                || php::literal(source.text, start..parameter.end).is_some()
                    && !raw.contains(['$', '\n', '\r', '`'])
                    && !raw.contains("*/");
            if !safe {
                return None;
            }
            format!(" = {raw}")
        } else {
            String::new()
        };
        result.push(format!("{kind} {reference}{variadic}{name}{default}"));
    }
    let after = function.params.end + 1;
    let return_type = if source.is(after, ":") {
        let tokens: Vec<_> = source.tokens[after + 1..function.body].iter().collect();
        let kind = doc_type(&tokens, &names, model_full,is_trait)?;
        if kind == "void" || kind == "null" {
            builder(model_full)
        } else if let Some(nonnull) = kind.strip_prefix('?') {
            format!("{nonnull}|{}", builder(model_full))
        } else if kind.split('|').any(|part| part == "null") {
            let mut parts = kind
                .split('|')
                .filter(|part| *part != "null")
                .map(str::to_owned)
                .collect::<Vec<_>>();
            parts.push(builder(model_full));
            parts.join("|")
        } else {
            kind
        }
    } else {
        // No return, bare return or return of the injected builder retains the
        // framework's builder result. Other untyped expressions are withheld.
        for at in function.body + 1..function.end {
            if source.is(at, "return") {
                if !source.is(at + 1, ";") && !(source.is(at + 1, query) && source.is(at + 2, ";"))
                {
                    return None;
                }
            }
        }
        builder(model_full)
    };
    Some(Signature {
        parameters: result,
        return_type,
    })
}
fn builder(full: &str) -> String {
    format!("\\Illuminate\\Database\\Eloquent\\Builder<\\{full}>")
}
pub(super) fn declared_type(tokens:&[&Token<'_>],names:&Names,full:&str,is_trait:bool)->Option<String>{doc_type(tokens,names,full,is_trait)}
fn doc_type(tokens: &[&Token<'_>], names: &Names, full: &str,is_trait:bool) -> Option<String> {
    if tokens.is_empty() {
        return Some("mixed".into());
    }
    let mut out = String::new();
    let mut expect_type = true;
    for (index, token) in tokens.iter().enumerate() {
        if index == 0 && token.value == "?" {
            out.push('?');
            continue;
        }
        if expect_type {
            if token.kind != Kind::Name {
                return None;
            }
            let lower = token.value.to_ascii_lowercase();
            let name = match lower.as_str() {
                "int" | "float" | "string" | "bool" | "array" | "object" | "mixed" | "null"
                | "false" | "true" | "void" | "never" | "iterable" | "callable" => lower,
                "self"=>format!("\\{}",if is_trait{full}else{&names.class.as_ref()?.0}),
                "static" => format!("\\{full}"),
                "parent" => {if is_trait||names.class.as_ref()?.3.is_empty(){return None;}format!("\\{}", names.class.as_ref()?.3)},
                _ => format!("\\{}", names.resolve(token.value)),
            };
            if name.contains("*/") || name.chars().any(char::is_control) {
                return None;
            }
            out.push_str(&name);
            expect_type = false;
        } else {
            if !matches!(token.value, "|" | "&") {
                return None;
            }
            out.push_str(token.value);
            expect_type = true;
        }
    }
    (!expect_type).then_some(out)
}

/// Nullability from the exact owned migration definition and its own fluent
/// chain. SQL dumps and dynamic modifier arguments remain unknown.
pub(crate) fn column_nullable(source: &str, entry: &Entry) -> Option<bool> {
    let source = Source::new(source)?;
    let offset = text::offset(source.text, entry.at)?;
    let kind = entry.detail.as_deref()?;
    for (at, token) in source.tokens.iter().enumerate() {
        if token.value != kind || !source.is(at + 1, "(") {
            continue;
        }
        let close = *source.pairs.get(&(at + 1))?;
        let named = source
            .tokens
            .get(at + 2)
            .is_some_and(|t| t.kind == Kind::String && t.start + 1 == offset);
        if !named && token.start != offset {
            continue;
        }
        let mut nullable = matches!(
            kind,
            "timestamps"
                | "timestampsTz"
                | "nullableTimestamps"
                | "softDeletes"
                | "softDeletesTz"
                | "nullableMorphs"
                | "nullableUuidMorphs"
                | "nullableUlidMorphs"
        );
        let mut next = close + 1;
        while source.is(next, "->") {
            let name = source.tokens.get(next + 1)?.value;
            if !source.is(next + 2, "(") {
                return None;
            }
            let end = *source.pairs.get(&(next + 2))?;
            if name == "nullable" {
                let args = &source.tokens[next + 3..end];
                nullable = match args {
                    [] => true,
                    [value] if value.value.eq_ignore_ascii_case("true") => true,
                    [value] if value.value.eq_ignore_ascii_case("false") => false,
                    _ => return None,
                };
            }
            next = end + 1;
        }
        return source.is(next, ";").then_some(nullable);
    }
    None
}
