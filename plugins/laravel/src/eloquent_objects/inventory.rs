//! Materialized public donor inventories, retaining declaration blockers and provenance.
use super::super::Models;
use super::{Object, Objects};
use std::collections::{BTreeMap, BTreeSet};
pub(in crate::eloquent_members) fn composed(
    full: &str,
    objects: &Objects,
    models: &Models,
) -> Option<Object> {
    fn walk(
        full: &str,
        objects: &Objects,
        models: &Models,
        active: &mut BTreeSet<String>,
        depth: usize,
        budget: &mut usize,
    ) -> Option<Object> {
        if depth >= 16 || *budget == 0 || !active.insert(full.to_ascii_lowercase()) {
            return None;
        }
        *budget -= 1;
        let key = full.to_ascii_lowercase();
        let mut output = objects.get(&key).filter(|o| !o.ambiguous)?.clone();
        let model = models.get(&key).filter(|m| !m.ambiguous);
        let mut blocked = output.declared_methods.clone();
        if let Some(model) = model {
            if model.trait_adaptations || (!model.traits.is_empty() && !model.adapted) {
                return None;
            }
            blocked.extend(model.declared.iter().map(|n| n.to_ascii_lowercase()));
            for (name, method) in &model.returns {
                if !name.starts_with("__") {
                    output.methods.insert(name.clone(), method.clone());
                }
            }
            output.metadata_sources.extend(model.trait_sources.clone());
        }
        let parent = model.map(|m| m.parent.as_str()).filter(|p| !p.is_empty());
        // PHP's real class parent precedes interfaces; interface methods contribute
        // only when no concrete method or declaration already establishes the name.
        let mut parents = output.parents.clone();
        if let Some(parent) = parent {
            parents.retain(|p| !p.eq_ignore_ascii_case(parent));
            parents.insert(0, parent.into());
        }
        let mut concrete = BTreeSet::new();
        for parent in parents {
            if !objects.contains_key(&parent.to_ascii_lowercase()) {
                continue;
            }
            let inherited = walk(
                &parent,
                objects,
                models,
                &mut active.clone(),
                depth + 1,
                budget,
            )?;
            output.metadata_sources.push(inherited.path.clone());
            output
                .metadata_sources
                .extend(inherited.metadata_sources.clone());
            for (name, method) in inherited.methods {
                let lower = name.to_ascii_lowercase();
                if blocked.contains(&lower) || concrete.contains(&lower) {
                    continue;
                }
                output.methods.insert(name.clone(), method);
                concrete.insert(lower);
            }
            for (name, property) in inherited.properties {
                if !output.declared_properties.contains(&name) {
                    output.properties.entry(name).or_insert(property);
                }
            }
        }
        // Composed trait methods above already include adaptations; public properties
        // have no alias/visibility adaptations. Conflicting trait declarations withhold
        // that property instead of selecting an arbitrary donor.
        if let Some(model) = model {
            let mut properties: BTreeMap<String, Vec<super::super::Entry>> = BTreeMap::new();
            for trait_ in &model.traits {
                let inherited = walk(
                    trait_,
                    objects,
                    models,
                    &mut active.clone(),
                    depth + 1,
                    budget,
                )?;
                output.metadata_sources.push(inherited.path.clone());
                output.metadata_sources.extend(inherited.metadata_sources);
                for (name, property) in inherited.properties {
                    properties.entry(name).or_default().push(property);
                }
            }
            for (name, entries) in properties {
                if output.declared_properties.contains(&name) {
                    continue;
                }
                if entries.len() == 1 {
                    output.properties.insert(name, entries[0].clone());
                } else {
                    output.properties.remove(&name);
                }
            }
        }
        output.metadata_sources.sort();
        output.metadata_sources.dedup();
        if output.metadata_sources.len() > 128 {
            return None;
        }
        Some(output)
    }
    walk(full, objects, models, &mut BTreeSet::new(), 0, &mut 256)
}
