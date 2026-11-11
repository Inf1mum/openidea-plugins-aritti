//! Explicitly captured Artisan facts projected into the source index. No read-time discovery.
use super::*;
pub(super) fn cache(project: &Project, metadata: &mut Metadata) {
    metadata.artisan_snapshot = None;
    metadata.issues.retain(|issue| {
        !issue.starts_with(
            "Artisan metadata completions require an explicit Artisan metadata refresh",
        )
    });
    if !metadata.rules.iter().any(|rule| {
        matches!(
            rule.category.as_str(),
            "artisanCommandName" | "artisanCommandParameter"
        )
    }) {
        return;
    }
    // Snapshot identity also reads the Artisan entrypoint and host storage. Their
    // evidence is not represented by the source-only index proof; fail closed on reuse.
    metadata.source_evidence_incomplete = true;
    let Some(root) = project.root().ok() else {
        return;
    };
    if ["artisan", "composer.json", "composer.lock"]
        .iter()
        .all(|path| {
            host::project_of(&format!("{root}/{path}"))
                .and_then(|owner| owner.root().ok())
                .as_deref()
                == Some(root.as_str())
        })
    {
        metadata.artisan_snapshot = crate::artisan_metadata::snapshot(project);
    }
    if metadata.artisan_snapshot.is_none() {
        metadata.issues.push("Artisan metadata completions require an explicit Artisan metadata refresh followed by reindex; no project PHP runs during completion".into());
    }
}
pub(super) fn inventory(index: &Index, category: &str, source: &str, site: &Site) -> Names {
    let mut out = Names::new();
    let Some(snapshot) = &index.metadata.artisan_snapshot else {
        return out;
    };
    let commands = snapshot["commands"].as_array().into_iter().flatten();
    let captured = snapshot["capturedAt"].as_u64().unwrap_or(0);
    if category == "artisanCommandName" {
        for command in commands {
            if let Some(name) = command["name"].as_str() {
                out.add(
                    name,
                    Some(format!(
                        "Captured Artisan metadata · {} · Unix {captured}; may be stale",
                        command["description"].as_str().unwrap_or("")
                    )),
                    "",
                    Position::new(0, 0),
                    0,
                );
            }
        }
        return out;
    }
    if site.return_context || site.argument < 2 {
        return out;
    }
    let masked = crate::php::mask_comments(source);
    let args = crate::php::arguments(&masked, site.open);
    let Some((previous, offset)) = args
        .get(site.argument - 2)
        .cloned()
        .and_then(|range| crate::php::literal(&masked, range))
    else {
        return out;
    };
    let Some(previous) = crate::eloquent_members::metadata_inference::decode_literal(
        previous,
        *source.as_bytes().get(offset - 1).unwrap_or(&b'\''),
    ) else {
        return out;
    };
    let mut matching = commands.filter(|command| command["name"].as_str() == Some(&previous));
    let Some(command) = matching.next() else {
        return out;
    };
    if matching.next().is_some() {
        return out;
    }
    for kind in ["arguments", "options"] {
        for value in command["definition"][kind].as_array().into_iter().flatten() {
            let Some(name) = value["name"].as_str() else {
                continue;
            };
            let name = if kind == "options" {
                format!("--{}", name.trim_start_matches('-'))
            } else {
                name.to_owned()
            };
            out.add(
                &name,
                Some(format!(
                    "Captured Artisan {kind} · {} · Unix {captured}; may be stale",
                    value["description"].as_str().unwrap_or("")
                )),
                "",
                Position::new(0, 0),
                0,
            );
        }
    }
    out
}
