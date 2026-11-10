//! Native choices for existing-model factory/resource generation. Choices are
//! data, never PHP expressions; source facts are checked again on action resolve.
use super::*;
use crate::eloquent_members::{helper_facts, related_facts};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub(super) struct Selection {
    pub fields: BTreeSet<String>,
    pub factories: BTreeMap<String, String>,
    // None emits the loaded relation directly; Some wraps its declared resource.
    pub relations: BTreeMap<String, Option<String>>,
    runtime: Option<(crate::artisan_runner::Runner, serde_json::Value)>,
}
fn fields(
    index: &Index,
    model: &ModelIdentity,
    kind: &str,
    runtime: Option<&(crate::artisan_runner::Runner, serde_json::Value)>,
) -> Vec<String> {
    let mut fields = index
        .columns
        .get(&model.table)
        .into_iter()
        .flatten()
        .filter(|(name, entry)| {
            kind != "factory" || faker(name, entry.detail.as_deref().unwrap_or("")).is_some()
        })
        .map(|(name, _)| name.clone())
        .collect::<BTreeSet<_>>();
    if kind == "resource" {
        if let Some(project) = host::project_of(&model.path) {
            fields.extend(
                related_facts::resource_fields(index, &project, &model.full)
                    .into_iter()
                    .map(|fact| fact.name),
            );
        }
        if let Some((_, snapshot)) = runtime {
            fields.extend(runtime_fields(index, model, snapshot));
        }
        fields.retain(|name| !matches!(name.as_str(), "password" | "remember_token"));
    }
    fields.into_iter().collect()
}

// Only a unique table on a currently proven named connection can add fields.
fn runtime_fields(
    index: &Index,
    model: &ModelIdentity,
    snapshot: &serde_json::Value,
) -> Vec<String> {
    let Some(project) = host::project_of(&model.path) else {
        return vec![];
    };
    if helper_facts::facts(index, &model.full, &|path| {
        crate::owns_current_path(&project, path)
    })
    .and_then(|facts| facts.table)
    .as_deref()
        != Some(model.table.as_str())
    {
        return vec![];
    }
    let Some(connection) = related_facts::connection(index, &project, &model.full) else {
        return vec![];
    };
    if snapshot["connection"].as_str() != Some(connection.as_str())
        || snapshot["capturedAt"].as_u64().is_none()
    {
        return vec![];
    }
    let tables = snapshot["tables"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|table| {
            let name = table["name"].as_str().unwrap_or("");
            if model.table.contains('.') {
                model.table == format!("{}.{}", table["schema"].as_str().unwrap_or(""), name)
            } else {
                model.table == name
            }
        })
        .collect::<Vec<_>>();
    let [table] = tables.as_slice() else {
        return vec![];
    };
    table["columns"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|column| column["name"].as_str())
        .filter(|name| !name.is_empty() && name.len() <= 512 && !name.chars().any(char::is_control))
        .map(str::to_owned)
        .collect()
}
fn relations(
    index: &Index,
    project: &Project,
    model: &ModelIdentity,
) -> Vec<helper_facts::RelationFact> {
    helper_facts::facts(index, &model.full, &|path| {
        crate::owns_current_path(project, path)
    })
    .map(|facts| facts.relations)
    .unwrap_or_default()
}
fn checked_many(
    title: &str,
    choices: &[openidea_plugin::PickItem],
    allowed: &BTreeSet<String>,
) -> Result<BTreeSet<String>, Error> {
    let selected = host::pick_many(title, choices)?.ok_or(Error::Cancelled)?;
    let count = selected.len();
    let selected: BTreeSet<_> = selected.into_iter().collect();
    if count != selected.len() || !selected.is_subset(allowed) {
        return Err(Error::Stale);
    }
    Ok(selected)
}
pub(super) fn choose(
    project: &Project,
    index: &Index,
    model: &ModelIdentity,
    kind: &str,
    runtime: Option<(crate::artisan_runner::Runner, serde_json::Value)>,
) -> Result<Selection, Error> {
    let available = fields(index, model, kind, runtime.as_ref());
    if available.len() > 128 {
        return Err(Error::Invalid(
            "Select from at most 128 model fields".into(),
        ));
    }
    let mode = host::pick(
        "Generated fields",
        &[
            openidea_plugin::PickItem::new("all", "Include all supported fields"),
            openidea_plugin::PickItem::new("select", "Select fields…"),
        ],
    )?
    .ok_or(Error::Cancelled)?;
    let details = if kind == "resource" {
        related_facts::resource_fields(index, project, &model.full)
            .into_iter()
            .map(|fact| (fact.name, fact.kind))
            .collect::<BTreeMap<_, _>>()
    } else {
        BTreeMap::new()
    };
    let allowed = available.iter().cloned().collect();
    let fields = match mode.as_str() {
        "all" => allowed,
        "select" => checked_many(
            "Fields to include (empty creates an empty body)",
            &available
                .iter()
                .map(|name| {
                    let item = openidea_plugin::PickItem::new(name, name);
                    if let Some(detail) = details.get(name) {
                        item.description(detail)
                    } else {
                        item.description("Schema column")
                    }
                })
                .collect::<Vec<_>>(),
            &allowed,
        )?,
        _ => return Err(Error::Stale),
    };
    let runtime = runtime.filter(|(_, snapshot)| {
        kind == "resource"
            && runtime_fields(index, model, snapshot)
                .iter()
                .any(|name| fields.contains(name))
    });
    let mut selection = Selection {
        fields,
        factories: BTreeMap::new(),
        relations: BTreeMap::new(),
        runtime,
    };
    let related = related_facts::inventory(index, project);
    if kind == "factory" {
        let candidates = related
            .iter()
            .filter(|fact| fact.kind == "Factory")
            .map(|fact| (fact.label.clone(), fact.model.clone()))
            .collect::<BTreeMap<_, _>>();
        let keys = selection
            .fields
            .iter()
            .filter(|field| field.ends_with("_id"))
            .cloned()
            .collect::<Vec<_>>();
        if !keys.is_empty() && !candidates.is_empty() {
            let picks = keys
                .iter()
                .map(|key| openidea_plugin::PickItem::new(key, key))
                .collect::<Vec<_>>();
            let selected = checked_many(
                "Foreign keys to map to an existing factory (others keep review placeholders)",
                &picks,
                &keys.iter().cloned().collect(),
            )?;
            if selected.len() > 32 {
                return Err(Error::Invalid(
                    "Map at most 32 foreign keys per generation".into(),
                ));
            }
            for field in selected {
                let items = candidates
                    .iter()
                    .map(|(class, model)| {
                        openidea_plugin::PickItem::new(class, class)
                            .description(&format!("Declared model: {model}"))
                    })
                    .collect::<Vec<_>>();
                let class = host::pick(
                    &format!("Factory for {field} (explicit association)"),
                    &items,
                )?
                .ok_or(Error::Cancelled)?;
                if !candidates.contains_key(&class) {
                    return Err(Error::Stale);
                }
                selection.factories.insert(field, class);
            }
        }
    } else {
        let facts = relations(index, project, model);
        if facts.len() > 64 {
            return Err(Error::Invalid(
                "Select from at most 64 model relations".into(),
            ));
        }
        if !facts.is_empty() {
            let items = facts
                .iter()
                .map(|fact| {
                    openidea_plugin::PickItem::new(&fact.name, &fact.name).description(&format!(
                        "{} · {}",
                        fact.target,
                        if fact.many { "many" } else { "one" }
                    ))
                })
                .collect::<Vec<_>>();
            let selected = checked_many(
                "Relations to include only when loaded",
                &items,
                &facts.iter().map(|fact| fact.name.clone()).collect(),
            )?;
            for name in selected {
                let relation = facts
                    .iter()
                    .find(|fact| fact.name == name)
                    .ok_or(Error::Stale)?;
                let candidates = related
                    .iter()
                    .filter(|fact| fact.kind == "Resource" && fact.model == relation.target)
                    .map(|fact| fact.label.clone())
                    .collect::<BTreeSet<_>>();
                let class = if candidates.is_empty() {
                    None
                } else {
                    let mut items = vec![openidea_plugin::PickItem::new(
                        "raw",
                        "Loaded relation without a resource wrapper",
                    )];
                    items.extend(
                        candidates
                            .iter()
                            .map(|class| openidea_plugin::PickItem::new(class, class)),
                    );
                    let choice = host::pick(&format!("Resource mapping for {name}"), &items)?
                        .ok_or(Error::Cancelled)?;
                    if choice == "raw" {
                        None
                    } else if candidates.contains(&choice) {
                        Some(choice)
                    } else {
                        return Err(Error::Stale);
                    }
                };
                selection.relations.insert(name, class);
            }
        }
    }
    selection.validate(project, index, model, kind)?;
    Ok(selection)
}
impl Selection {
    pub(super) fn runtime_note(&self) -> String {
        self.runtime
            .as_ref()
            .map(|(runner, snapshot)| {
                format!(
                    "Captured database fields: {} · connection {} · captured at {}\n",
                    runner.name(),
                    snapshot["connection"],
                    snapshot["capturedAt"]
                )
            })
            .unwrap_or_default()
    }
    pub(super) fn check_runtime(&self, main: &crate::Main, project: &Project) -> Result<(), Error> {
        if let Some((runner, snapshot)) = &self.runtime {
            if main.artisan_workspace.runner_for_project(project)? != *runner
                || main
                    .runtime_inspection
                    .schema_for_generation(project, *runner)?
                    != *snapshot
            {
                return Err(Error::Stale);
            }
        }
        Ok(())
    }
    pub(super) fn validate(
        &self,
        project: &Project,
        index: &Index,
        model: &ModelIdentity,
        kind: &str,
    ) -> Result<String, Error> {
        let available = fields(index, model, kind, self.runtime.as_ref())
            .into_iter()
            .collect();
        if self.fields.len() > 128
            || !self.fields.is_subset(&available)
            || self.factories.len() > 32
            || self.relations.len() > 64
        {
            return Err(Error::Stale);
        }
        if kind == "resource" && !self.factories.is_empty()
            || kind == "factory" && !self.relations.is_empty()
        {
            return Err(Error::Stale);
        }
        let related = related_facts::inventory(index, project);
        let mut evidence = self
            .runtime
            .as_ref()
            .map(|(runner, snapshot)| {
                format!(
                    "Captured schema: {} · connection {} · captured at {}",
                    runner.name(),
                    snapshot["connection"],
                    snapshot["capturedAt"]
                )
            })
            .unwrap_or_default();
        let prove = |class: &str, kind: &str, target: Option<&str>| -> Result<String, Error> {
            if !crate::generator_plan::qualified(class) || class.starts_with('\\') {
                return Err(Error::Stale);
            }
            let matches = related
                .iter()
                .filter(|fact| {
                    fact.label == class
                        && fact.kind == kind
                        && target.is_none_or(|target| fact.model == target)
                })
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err(Error::Stale);
            }
            let fact = matches[0];
            if !crate::owns_current_path(project, &fact.entry.path) {
                return Err(Error::Stale);
            }
            Ok(format!(
                "\n{}:{}:{}",
                fact.model,
                fact.entry.path,
                project.read_file(&fact.entry.path)?
            ))
        };
        for (field, class) in &self.factories {
            if !field.ends_with("_id") || !self.fields.contains(field) {
                return Err(Error::Stale);
            }
            evidence.push_str(&prove(class, "Factory", None)?);
        }
        if kind == "resource" {
            let paths = related_facts::resource_fields(index, project, &model.full)
                .into_iter()
                .filter(|fact| self.fields.contains(&fact.name))
                .map(|fact| fact.entry.path)
                .collect::<BTreeSet<_>>();
            for path in paths {
                if !crate::owns_current_path(project, &path) {
                    return Err(Error::Stale);
                }
                evidence.push_str(&format!("\n{path}:{}", project.read_file(&path)?));
            }
        }
        let facts = relations(index, project, model);
        for (name, class) in &self.relations {
            if self.fields.contains(name) || !crate::generator_plan::identifier(name) {
                return Err(Error::Invalid(
                    "A resource field and relation cannot share an output key".into(),
                ));
            }
            let relation = facts
                .iter()
                .find(|fact| fact.name == *name)
                .ok_or(Error::Stale)?;
            if let Some(class) = class {
                evidence.push_str(&prove(class, "Resource", Some(&relation.target))?);
            }
            evidence.push_str(&format!("\n{name}:{}:{}", relation.target, relation.many));
        }
        if evidence.len() > 4 * 1024 * 1024 {
            return Err(Error::Invalid(
                "Related generation evidence exceeds 4 MiB".into(),
            ));
        }
        Ok(evidence)
    }
    pub(super) fn rows(&self, index: &Index, model: &ModelIdentity, kind: &str) -> String {
        let mut rows = String::new();
        for field in &self.fields {
            let value = if kind == "factory" {
                if let Some(class) = self.factories.get(field) {
                    format!("\\{class}::new()")
                } else {
                    index
                        .columns
                        .get(&model.table)
                        .and_then(|columns| columns.get(field))
                        .and_then(|entry| faker(field, entry.detail.as_deref().unwrap_or("")))
                        .unwrap_or_default()
                }
            } else {
                property(field)
            };
            rows.push_str(&format!("            {} => {value},\n", literal(field)));
        }
        // Cardinality is regenerated from current verified facts for each preview.
        let project = host::project_of(&model.path);
        let facts = project
            .as_ref()
            .map(|project| relations(index, project, model))
            .unwrap_or_default();
        for (name, class) in &self.relations {
            let loaded = format!("$this->whenLoaded({})", literal(name));
            let value = match class {
                None => loaded,
                Some(class) if facts.iter().any(|fact| fact.name == *name && fact.many) => {
                    format!("\\{class}::collection({loaded})")
                }
                Some(class) => format!("new \\{class}({loaded})"),
            };
            rows.push_str(&format!("            {} => {value},\n", literal(name)));
        }
        rows
    }
}
