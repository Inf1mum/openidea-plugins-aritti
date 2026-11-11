//! Source syntax is retained rather than reconstructed from whitespace-free tokens.
use super::super::{Function, Kind, Names, Source};
use super::Objects;
use std::collections::BTreeSet;
#[derive(Clone, Debug)]
pub(super) struct Signature {
    pub parameters: Option<Vec<String>>,
    pub public: bool,
}
fn resolved(names: &Names, name: &str, is_trait: bool) -> Option<String> {
    let value = match name.to_ascii_lowercase().as_str() {
        "int" | "string" | "bool" | "float" | "array" | "object" | "mixed" | "callable"
        | "iterable" | "null" | "false" | "true" | "never" | "void" => {
            return Some(name.into());
        }
        "self" if is_trait => "__OpenIDEA_Trait_Self__".into(),
        "parent" if is_trait => "__OpenIDEA_Trait_Parent__".into(),
        "self" => names.class.as_ref()?.0.clone(),
        "parent" => {
            let p = &names.class.as_ref()?.3;
            if p.is_empty() {
                return None;
            }
            p.clone()
        }
        "static" => return None,
        _ => names.resolve(name),
    };
    Some(format!("\\{value}"))
}
pub(in crate::eloquent_members) fn parameters(
    source: &Source,
    names: &Names,
    function: &Function,
) -> Option<Vec<String>> {
    let trait_ = names
        .class
        .as_ref()
        .is_some_and(|(_, at, _, _)| source.is(*at, "trait"));
    let mut result = Vec::new();
    let mut at = function.params.start;
    while at < function.params.end {
        if result.len() >= 64 {
            return None;
        }
        let start = at;
        while at < function.params.end && !source.is(at, ",") {
            if let Some(end) = source.pairs.get(&at).filter(|end| **end > at) {
                at = end + 1
            } else {
                at += 1;
            }
        }
        let end = at;
        at += 1;
        if start == end {
            return None;
        }
        let mut cursor = start;
        let mut edits = Vec::new();
        while cursor < end && source.is(cursor, "#") && source.is(cursor + 1, "[") {
            let close = *source.pairs.get(&(cursor + 1))?;
            edits.push((
                source.tokens[cursor].start..source.tokens[close].end,
                String::new(),
            ));
            cursor = close + 1;
        }
        let variable = (cursor..end).find(|i| source.tokens[*i].kind == Kind::Var)?;
        if (variable + 1..end).any(|i| source.tokens[i].kind == Kind::Var) {
            return None;
        }
        for i in cursor..variable {
            let token = &source.tokens[i];
            if matches!(token.value, "public" | "protected" | "private" | "readonly") {
                edits.push((token.start..token.end, String::new()));
            } else if token.kind == Kind::Name {
                edits.push((
                    token.start..token.end,
                    resolved(names, token.value, trait_)?,
                ));
            } else if !matches!(token.value, "?" | "|" | "&" | "..." | "(" | ")") {
                return None;
            }
        }
        for i in variable + 1..end {
            let token = &source.tokens[i];
            if token.kind == Kind::Name
                && (source.is(i + 1, "::") || i > 0 && source.is(i - 1, "new"))
            {
                edits.push((
                    token.start..token.end,
                    resolved(names, token.value, trait_)?,
                ));
            }
        }
        let begin = source.tokens[start].start;
        let finish = source.tokens[end - 1].end;
        let mut label = source.text[begin..finish].to_owned();
        edits.sort_by_key(|(range, _)| range.start);
        for (range, value) in edits.into_iter().rev() {
            label.replace_range(range.start - begin..range.end - begin, &value);
        }
        result.push(label.trim().to_owned());
    }
    Some(result)
}
/// Class declarations precede selected traits; traits precede the real class parent.
/// Visibility-changing aliases and insteadof use the same parsed adaptation facts
/// as ordinary model methods, retaining source parameter syntax from the donor.
pub(in crate::eloquent_members) fn constructor(
    full: &str,
    objects: &Objects,
    models: &super::super::Models,
) -> Option<Vec<String>> {
    fn callable(
        full: &str,
        name: &str,
        objects: &Objects,
        models: &super::super::Models,
        seen: &mut BTreeSet<String>,
        depth: usize,
    ) -> Option<(Signature, String, String)> {
        if depth >= 16
            || !seen.insert(format!(
                "{}::{}",
                full.to_ascii_lowercase(),
                name.to_ascii_lowercase()
            ))
        {
            return None;
        }
        let key = full.to_ascii_lowercase();
        let object = objects.get(&key).filter(|o| !o.ambiguous)?;
        let model = models.get(&key).filter(|m| !m.ambiguous)?;
        if let Some(signature) = object.signatures.get(&name.to_ascii_lowercase()) {
            return Some((signature.clone(), model.full.clone(), model.parent.clone()));
        }
        if model.trait_adaptations || (!model.traits.is_empty() && !model.adapted) {
            return None;
        }
        let mut excluded = BTreeSet::new();
        for rule in &model.adaptations {
            if let super::super::traits::Adaptation::Prefer {
                winner: _,
                method,
                losers,
            } = rule
            {
                if method.eq_ignore_ascii_case(name) {
                    excluded.extend(losers.iter().map(|s| s.to_ascii_lowercase()));
                }
            }
        }
        let aliases = model
            .adaptations
            .iter()
            .filter_map(|rule| {
                if let super::super::traits::Adaptation::Alias {
                    owner,
                    method,
                    alias,
                    visibility,
                    ..
                } = rule
                {
                    (alias
                        .as_deref()
                        .is_some_and(|alias| alias.eq_ignore_ascii_case(name)))
                    .then_some((owner, method, visibility))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let mut candidates = Vec::new();
        if aliases.len() > 1 {
            return None;
        }
        if let Some((owner, method, visibility)) = aliases.first() {
            for trait_ in &model.traits {
                if owner
                    .as_ref()
                    .is_none_or(|owner| owner.eq_ignore_ascii_case(trait_))
                    && (owner.is_some() || !excluded.contains(&trait_.to_ascii_lowercase()))
                {
                    if let Some((mut signature, _, _)) = callable(
                        trait_,
                        method,
                        objects,
                        models,
                        &mut seen.clone(),
                        depth + 1,
                    ) {
                        if let Some(visibility) = visibility {
                            signature.public = visibility == "public";
                        }
                        candidates.push((signature, model.full.clone(), model.parent.clone()));
                    }
                }
            }
        } else {
            for trait_ in &model.traits {
                if excluded.contains(&trait_.to_ascii_lowercase()) {
                    continue;
                }
                if let Some((mut signature, _, _)) =
                    callable(trait_, name, objects, models, &mut seen.clone(), depth + 1)
                {
                    for rule in &model.adaptations {
                        if let super::super::traits::Adaptation::Alias {
                            owner,
                            method,
                            alias: None,
                            visibility: Some(visibility),
                            ..
                        } = rule
                        {
                            if method.eq_ignore_ascii_case(name)
                                && owner
                                    .as_ref()
                                    .is_none_or(|owner| owner.eq_ignore_ascii_case(trait_))
                            {
                                signature.public = visibility == "public";
                            }
                        }
                    }
                    candidates.push((signature, model.full.clone(), model.parent.clone()));
                }
            }
        }
        if !aliases.is_empty() && candidates.is_empty() {
            return None;
        }
        match candidates.len() {
            1 => candidates.pop(),
            n if n > 1 => None,
            _ if !model.parent.is_empty() => {
                callable(&model.parent, name, objects, models, seen, depth + 1)
            }
            _ if model.is_trait => None,
            _ => Some((
                Signature {
                    parameters: Some(vec![]),
                    public: true,
                },
                model.full.clone(),
                model.parent.clone(),
            )),
        }
    }
    let (signature, owner, parent) = callable(
        full,
        "__construct",
        objects,
        models,
        &mut BTreeSet::new(),
        0,
    )?;
    if !signature.public {
        return None;
    }
    signature
        .parameters?
        .into_iter()
        .map(|parameter| {
            if parent.is_empty() && parameter.contains("__OpenIDEA_Trait_Parent__") {
                None
            } else {
                Some(
                    parameter
                        .replace("__OpenIDEA_Trait_Self__", &owner)
                        .replace("__OpenIDEA_Trait_Parent__", &parent),
                )
            }
        })
        .collect()
}
