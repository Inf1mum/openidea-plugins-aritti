//! Bounded trait adaptation composition, performed at indexing or current-document overlay time.
use super::{Model, Models, Names, Source, entry};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug)]
pub(super) enum Adaptation {
    Prefer {
        winner: String,
        method: String,
        losers: Vec<String>,
    },
    Alias {
        owner: Option<String>,
        method: String,
        alias: Option<String>,
        visibility: Option<String>,
        declaration: super::Entry,
    },
}
pub(super) fn parse(
    source: &Source,
    names: &Names,
    open: usize,
    path: &str,
) -> Option<Vec<Adaptation>> {
    let close = *source.pairs.get(&open)?;
    let mut result = vec![];
    let mut at = open + 1;
    while at < close {
        if result.len() >= 32 {
            return None;
        }
        let first = source.tokens.get(at)?;
        if first.kind != super::Kind::Name {
            return None;
        }
        let (owner, method) = if source.is(at + 1, "::") {
            at += 3;
            (
                Some(names.resolve(first.value)),
                source.tokens.get(at - 1)?.value.to_owned(),
            )
        } else {
            at += 1;
            (None, first.value.to_owned())
        };
        if source.is(at, "insteadof") {
            let winner = owner?;
            at += 1;
            let mut losers = vec![];
            loop {
                let token = source.tokens.get(at)?;
                if token.kind != super::Kind::Name {
                    return None;
                }
                losers.push(names.resolve(token.value));
                at += 1;
                if !source.is(at, ",") {
                    break;
                }
                at += 1;
            }
            result.push(Adaptation::Prefer {
                winner,
                method,
                losers,
            });
        } else if source.is(at, "as") {
            at += 1;
            let visibility = if ["public", "protected", "private"]
                .iter()
                .any(|visibility| source.is(at, visibility))
            {
                let visibility = source.tokens[at].value.to_owned();
                at += 1;
                Some(visibility)
            } else {
                None
            };
            let alias = if source.is(at, ";") {
                None
            } else {
                let token = source.tokens.get(at)?;
                if token.kind != super::Kind::Name {
                    return None;
                }
                at += 1;
                Some(token.value.to_owned())
            };
            if alias.is_none() && visibility.is_none() {
                return None;
            }
            let declaration = entry(source, path, if alias.is_some() { at - 1 } else { at - 2 });
            result.push(Adaptation::Alias {
                owner,
                method,
                alias,
                visibility,
                declaration,
            });
        } else {
            return None;
        }
        if !source.is(at, ";") {
            return None;
        }
        at += 1;
    }
    Some(result)
}
fn scope(method: &str) -> Option<String> {
    let suffix = method.strip_prefix("scope")?;
    if !suffix.starts_with(|c: char| c.is_ascii_uppercase()) {
        return None;
    }
    Some(format!(
        "{}{}",
        suffix[..1].to_ascii_lowercase(),
        &suffix[1..]
    ))
}
/// Composition failure preserves the original class's own members and withholds traits.
pub(super) fn compose(
    model: &mut Model,
    models: &Models,
    active: &mut BTreeSet<String>,
    depth: usize,
) -> Option<()> {
    compose_inner(model, models, active, depth, &mut 64)
}
fn compose_inner(
    model: &mut Model,
    models: &Models,
    active: &mut BTreeSet<String>,
    depth: usize,
    remaining: &mut usize,
) -> Option<()> {
    if *remaining == 0 || model.declared.len() > 256 {
        return None;
    }
    *remaining -= 1;
    if model.adapted {
        return Some(());
    }
    if depth >= 8 || model.trait_adaptations || !active.insert(model.full.to_ascii_lowercase()) {
        return None;
    }
    let mut traits = vec![];
    for name in &model.traits {
        let info = models.get(&name.to_ascii_lowercase())?;
        if !info.is_trait || info.ambiguous {
            return None;
        }
        let mut info = info.clone();
        compose_inner(&mut info, models, active, depth + 1, remaining)?;
        if traits
            .iter()
            .any(|old: &Model| old.full.eq_ignore_ascii_case(&info.full))
        {
            continue;
        }
        if traits.len() >= 32 {
            return None;
        }
        traits.push(info);
    }
    active.remove(&model.full.to_ascii_lowercase());
    let mut excluded = BTreeSet::new();
    for rule in &model.adaptations {
        if let Adaptation::Prefer {
            winner,
            method,
            losers,
        } = rule
        {
            if !traits.iter().any(|info| {
                info.full.eq_ignore_ascii_case(winner) && info.declared.contains(method)
            }) {
                return None;
            }
            for loser in losers {
                if loser.eq_ignore_ascii_case(winner)
                    || !traits.iter().any(|info| {
                        info.full.eq_ignore_ascii_case(loser) && info.declared.contains(method)
                    })
                {
                    return None;
                }
                excluded.insert((loser.to_ascii_lowercase(), method.clone()));
            }
        }
    }
    for rule in &model.adaptations {
        if let Adaptation::Prefer { winner, method, .. } = rule {
            if excluded.contains(&(winner.to_ascii_lowercase(), method.clone())) {
                return None;
            }
        }
    }
    let mut selected: BTreeMap<String, &Model> = BTreeMap::new();
    for info in &traits {
        for method in &info.declared {
            if model.declared.contains(method)
                || excluded.contains(&(info.full.to_ascii_lowercase(), method.clone()))
            {
                continue;
            }
            if selected.insert(method.clone(), info).is_some() {
                return None;
            }
        }
    }
    let mut result = model.clone();
    // Properties/casts from competing traits are deliberately not guessed.
    let properties: Vec<_> = traits
        .iter()
        .filter(|info| info.property_casts.is_some())
        .collect();
    if result.property_casts.is_none() && properties.len() == 1 {
        result.property_casts = properties[0].property_casts.clone();
    }
    if result.property_casts.is_none() && properties.len() > 1 {
        result.property_casts = Some(None);
    }
    for (name, info) in selected {
        result.declared.insert(name.clone());
        if let Some(relation) = info.relations.get(&name) {
            result.relations.insert(name.clone(), relation.clone());
        }
        if let Some(method) = info.returns.get(&name) {
            result.returns.insert(name.clone(), method.clone());
        }
        if let Some(scope) = scope(&name)
            .and_then(|scope| info.scopes.get(&scope).map(|entry| (scope, entry.clone())))
        {
            result.scopes.insert(scope.0, scope.1);
        }
        if let Some(entry) = info.scopes.get(&name) {
            result.scopes.insert(name.clone(), entry.clone());
        }
        if name == "casts" {
            result.method_casts = info.method_casts.clone();
        }
    }
    for rule in &model.adaptations {
        let Adaptation::Alias {
            owner,
            method,
            alias,
            visibility,
            declaration,
        } = rule
        else {
            continue;
        };
        let candidates: Vec<_> = traits
            .iter()
            .filter(|info| {
                owner.as_ref().map_or_else(
                    || !excluded.contains(&(info.full.to_ascii_lowercase(), method.clone())),
                    |owner| info.full.eq_ignore_ascii_case(owner),
                ) && info.declared.contains(method)
            })
            .collect();
        if candidates.len() != 1 {
            return None;
        }
        let info = candidates[0];
        let name = alias.as_ref().unwrap_or(method);
        if model.declared.contains(name) {
            continue;
        }
        if alias.is_some() && result.declared.contains(name) {
            return None;
        }
        result.declared.insert(name.clone());
        let public = visibility
            .as_deref()
            .is_none_or(|visibility| visibility == "public");
        if public {
            if let Some(relation) = info.relations.get(method) {
                let mut relation = relation.clone();
                relation.entry = declaration.clone();
                result.relations.insert(name.clone(), relation);
            }
            if let Some(returned) = info.returns.get(method) {
                let mut returned = returned.clone();
                returned.entry = declaration.clone();
                result.returns.insert(name.clone(), returned);
            }
        } else {
            result.relations.remove(name);
            result.returns.remove(name);
        }
        if let Some(original) = scope(method).and_then(|name| info.scopes.get(&name)) {
            if let Some(scope) = scope(name) {
                if visibility.as_deref() != Some("private") {
                    result.scopes.insert(scope, declaration.clone());
                } else {
                    result.scopes.remove(&scope);
                }
            }
            let _ = original;
        }
    }
    if result.declared.len() > 256 {
        return None;
    }
    for info in &traits {
        result.trait_sources.push(info.path.clone());
        result
            .trait_sources
            .extend(info.trait_sources.iter().cloned());
        result.soft_deletes |= info.soft_deletes;
    }
    result.trait_sources.sort();
    result.trait_sources.dedup();
    if result.trait_sources.len() > 32 {
        return None;
    }
    result.adapted = true;
    *model = result;
    Some(())
}
pub(crate) fn finish(models: &mut Models) -> bool {
    let started = std::time::Instant::now();
    let keys: Vec<_> = models.keys().cloned().collect();
    for key in keys {
        if started.elapsed() > std::time::Duration::from_millis(100) {
            return false;
        }
        let Some(mut model) = models.get(&key).cloned() else {
            continue;
        };
        if compose(&mut model, models, &mut BTreeSet::new(), 0).is_some() {
            models.insert(key, model);
        }
    }
    true
}
