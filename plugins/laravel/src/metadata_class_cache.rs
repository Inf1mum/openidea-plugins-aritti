//! Follow only declared PSR-4 type dependencies during indexing, never editor callbacks.
use super::*;
use crate::{
    eloquent_members::metadata_inference as inference, package_integrations::ComposerRoot,
};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
pub(super) fn complete(project: &Project, metadata: &mut Metadata, roots: &[ComposerRoot]) {
    if !metadata.needs_class_facts() {
        return;
    }
    let Ok(root) = project.root() else {
        return;
    };
    let started = Instant::now();
    let mut pending = VecDeque::new();
    for rule in &metadata.rules {
        for condition in &rule.conditions {
            pending.extend(
                condition
                    .classes
                    .iter()
                    .chain(&condition.parent_classes)
                    .chain(&condition.constructors)
                    .chain(&condition.parent_constructors)
                    .cloned(),
            );
        }
        if let Some(full) = rule.options["fetchClass"]
            .as_str()
            .filter(|value| *value != "current" && !value.starts_with("containingClassField:"))
        {
            pending.push_back(full.into());
        }
    }
    let mut references = Vec::new();
    for fact in metadata.class_facts.values() {
        pending.extend(fact.parents.iter().chain(&fact.traits).cloned());
        pending.extend(
            fact.properties
                .values()
                .filter_map(|property| property.class_target.clone()),
        );
        references.extend(fact.references.iter().cloned());
    }
    pending.extend(references);
    let mut seen = BTreeSet::new();
    let mut queries = 0;
    let mut files = 0;
    let mut bytes: usize = 0;
    while let Some(full) = pending.pop_front() {
        let full = full.trim_matches('\\').to_owned();
        let key = full.to_ascii_lowercase();
        if !seen.insert(key.clone())
            || metadata.class_facts.contains_key(&key)
            || metadata.ambiguous_classes.contains(&key)
        {
            continue;
        }
        if queries >= 256
            || files >= 64
            || bytes >= 4 * 1024 * 1024
            || started.elapsed() > Duration::from_millis(350)
        {
            // Unvisited dependencies contribute no facts; their budget is separate
            // from failed or conflicting reads of sources actually consumed.
            metadata.issues.push("Metadata type dependency scan exceeded its 256-path/64-file/4-MiB/350-ms budget; remaining class facts withheld".into());
            break;
        }
        let mut candidates = BTreeSet::new();
        for mapping in roots {
            let Some(tail) = full.strip_prefix(&mapping.namespace) else {
                continue;
            };
            if tail.is_empty() || !tail.split('\\').all(identifier) {
                continue;
            }
            candidates.insert(format!(
                "{}/{}.php",
                mapping.directory.trim_end_matches('/'),
                tail.replace('\\', "/")
            ));
        }
        let mut found = Vec::new();
        for relative in candidates {
            if queries >= 256 {
                break;
            }
            queries += 1;
            metadata.inputs.insert(relative.clone());
            let path = format!("{root}/{relative}");
            if host::project_of(&path)
                .and_then(|owner| owner.root().ok())
                .as_deref()
                != Some(root.as_str())
            {
                metadata.source_evidence_incomplete = true;
                continue;
            }
            let stat = match project.stat(&path) {
                Ok(stat) => stat,
                Err(openidea_plugin::Error::NotFound(_)) => {
                    evidence::missing(
                        &mut metadata.source_evidence,
                        &relative,
                        &mut metadata.source_evidence_incomplete,
                    );
                    continue;
                }
                Err(_) => {
                    metadata.source_evidence_incomplete = true;
                    continue;
                }
            };
            if stat.kind != openidea_plugin::FileKind::File {
                metadata.source_evidence_incomplete = true;
                continue;
            }
            if stat.size > 128 * 1024 {
                metadata.source_evidence_incomplete = true;
                metadata.issues.push(format!(
                    "Metadata type source {relative} exceeds 128-KiB declaration budget; withheld"
                ));
                continue;
            }
            if files >= 64 || bytes.saturating_add(stat.size as usize) > 4 * 1024 * 1024 {
                break;
            }
            files += 1;
            let Ok(source) = project.read_file(&path) else {
                metadata.source_evidence_incomplete = true;
                continue;
            };
            evidence::record(
                &mut metadata.source_evidence,
                &relative,
                &source,
                &mut metadata.source_evidence_incomplete,
            );
            bytes += source.len();
            if source.len() > 128 * 1024 {
                continue;
            }
            if let Some(fact) = inference::class_fact(&source, &path)
                .filter(|fact| fact.full.eq_ignore_ascii_case(&full))
            {
                found.push((source, fact));
            }
        }
        if found.len() > 1 {
            metadata.ambiguous_classes.insert(key);
            metadata.issues.push(format!(
                "Ambiguous Composer declarations for metadata class {full}; withheld"
            ));
            continue;
        }
        if let Some((source, fact)) = found.pop() {
            pending.extend(fact.parents.iter().chain(&fact.traits).cloned());
            pending.extend(
                fact.properties
                    .values()
                    .filter_map(|property| property.class_target.clone()),
            );
            pending.extend(fact.references.iter().cloned());
            inference::cache_class(metadata, &source, &fact.path);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    #[test]
    fn unconsumed_type_budget_does_not_invalidate_exact_owned_source_evidence() {
        let h = MockHost::install();
        let project = h.invocation().project().unwrap();
        let mut metadata = Metadata::default();
        metadata.extend("ide.json",r#"{"completions":[{"complete":"classFields","options":{"fetchClass":"App\\Seed"},"condition":[{"functionNames":["field"],"parameters":[1]}]}]}"#,|_|None);
        let imports = (0..80)
            .map(|n| format!("use App\\C{n};"))
            .collect::<String>();
        let seed = format!("<?php namespace App;{imports}class Seed {{}}");
        h.add_file("app/Seed.php", &seed);
        evidence::record(
            &mut metadata.source_evidence,
            "app/Seed.php",
            &seed,
            &mut metadata.source_evidence_incomplete,
        );
        inference::cache_class(&mut metadata, &seed, "/project/app/Seed.php");
        for n in 0..80 {
            h.add_file(
                &format!("app/C{n}.php"),
                &format!("<?php namespace App; class C{n} {{ public string $name; }}"),
            );
        }
        complete(
            &project,
            &mut metadata,
            &[ComposerRoot {
                namespace: "App\\".into(),
                directory: "app".into(),
                package: "project".into(),
            }],
        );
        assert!(
            metadata
                .issues
                .iter()
                .any(|issue| issue.contains("dependency scan exceeded"))
        );
        assert!(metadata.class_facts.len() < 81);
        assert!(!metadata.source_evidence.is_empty());
        assert!(!metadata.source_evidence_incomplete);
        let user =
            "<?php namespace App; class User extends \\Illuminate\\Database\\Eloquent\\Model {}";
        h.add_file("app/User.php", user);
        evidence::record(
            &mut metadata.source_evidence,
            "app/User.php",
            user,
            &mut metadata.source_evidence_incomplete,
        );
        let mut index = Index {
            source_root: "/project".into(),
            source_evidence: metadata.source_evidence.clone(),
            source_queries: metadata.source_queries.clone(),
            source_evidence_incomplete: metadata.source_evidence_incomplete,
            ..Index::default()
        };
        crate::eloquent_members::cache(&mut index.eloquent_models, user, "/project/app/User.php");
        assert!(index.eloquent_models.contains_key("app\\user"));
        assert!(
            crate::index::index_sources_current(&project, &index),
            "bounded unknown type facts must not freeze unrelated indexed features"
        );
    }
}
