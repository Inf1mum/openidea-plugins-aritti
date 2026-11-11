//! Native choices for existing-model factory/resource generation. Choices are
//! data, never PHP expressions; source facts are checked again on action resolve.
use super::*;
use crate::eloquent_members::{helper_facts, related_facts};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub(super) struct Selection {
    pub fields: BTreeSet<String>,
    pub factories: BTreeMap<String, String>,
    pub inferred_factories: BTreeSet<String>,
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
    choose_ui_inner(project, index, model, kind, runtime).map_err(ui_error)
}
fn choose_ui_inner(
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
        crate::ui_text::text("final.related.fieldsPrompt", "Generated fields"),
        &[
            openidea_plugin::PickItem::new(
                "all",
                crate::ui_text::text("final.related.allFields", "Include all supported fields"),
            ),
            openidea_plugin::PickItem::new(
                "select",
                crate::ui_text::text("final.related.selectFields", "Select fields…"),
            ),
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
            crate::ui_text::text(
                "final.related.includedFields",
                "Fields to include (empty creates an empty body)",
            ),
            &available
                .iter()
                .map(|name| {
                    let item = openidea_plugin::PickItem::new(name, name);
                    if let Some(detail) = details.get(name) {
                        item.description(detail)
                    } else {
                        item.description(crate::ui_text::text(
                            "final.related.schemaColumn",
                            "Schema column",
                        ))
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
        inferred_factories: BTreeSet::new(),
        relations: BTreeMap::new(),
        runtime,
    };
    let related = related_facts::inventory(index, project);
    if kind == "factory" {
        let defaults = crate::generator_workspace::factory_defaults::for_existing(
            project,
            index,
            &model.full,
            &selection.fields,
        );
        selection.factories = defaults
            .iter()
            .map(|(field, suggestion)| (field.clone(), suggestion.class.clone()))
            .collect();
        selection.inferred_factories = selection.factories.keys().cloned().collect();
        let mut candidates = related
            .iter()
            .filter(|fact| fact.kind == "Factory")
            .map(|fact| (fact.label.clone(), fact.model.clone()))
            .collect::<BTreeMap<_, _>>();
        candidates.extend(
            crate::generator_workspace::factory_defaults::bindings(project, index)
                .into_iter()
                .map(|(class, binding)| (class, binding.model)),
        );
        let keys = selection
            .fields
            .iter()
            .filter(|field| field.ends_with("_id") || defaults.contains_key(*field))
            .cloned()
            .collect::<Vec<_>>();
        if !keys.is_empty() && !candidates.is_empty() {
            let picks = keys
                .iter()
                .map(|key| openidea_plugin::PickItem::new(key, key))
                .collect::<Vec<_>>();
            let selected = checked_many(
                crate::ui_text::text(
                    "final.related.factoryKeys",
                    "Foreign keys to map to an existing factory (others keep review placeholders)",
                ),
                &picks,
                &keys.iter().cloned().collect(),
            )?;
            if selected.len() > 32 {
                return Err(Error::Invalid(
                    "Map at most 32 foreign keys per generation".into(),
                ));
            }
            selection
                .factories
                .retain(|field, _| selected.contains(field));
            selection
                .inferred_factories
                .retain(|field| selected.contains(field));
            for field in selected {
                let mut items = candidates
                    .iter()
                    .map(|(class, model)| {
                        openidea_plugin::PickItem::new(class, class).description(
                            &crate::ui_text::format(
                                "final.related.declaredModel",
                                "Declared model: {model}",
                                &[("model", &model)],
                            ),
                        )
                    })
                    .collect::<Vec<_>>();
                if let Some(suggestion) = defaults.get(&field) {
                    items.sort_by_key(|item| item.key != suggestion.class);
                }
                let class = host::pick(
                    &crate::ui_text::format(
                        "final.related.factoryPrompt",
                        "Factory for {field} (explicit association)",
                        &[("field", &field)],
                    ),
                    &items,
                )?
                .ok_or(Error::Cancelled)?;
                if !candidates.contains_key(&class) {
                    return Err(Error::Stale);
                }
                if defaults
                    .get(&field)
                    .is_none_or(|suggestion| suggestion.class != class)
                {
                    selection.inferred_factories.remove(&field);
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
                        if fact.many {
                            crate::ui_text::text("final.related.many", "many")
                        } else {
                            crate::ui_text::text("final.related.one", "one")
                        }
                    ))
                })
                .collect::<Vec<_>>();
            let selected = checked_many(
                crate::ui_text::text(
                    "final.related.loadedRelations",
                    "Relations to include only when loaded",
                ),
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
                        crate::ui_text::text(
                            "final.related.rawRelation",
                            "Loaded relation without a resource wrapper",
                        ),
                    )];
                    items.extend(
                        candidates
                            .iter()
                            .map(|class| openidea_plugin::PickItem::new(class, class)),
                    );
                    let choice = host::pick(
                        &crate::ui_text::format(
                            "final.related.resourcePrompt",
                            "Resource mapping for {name}",
                            &[("name", &name)],
                        ),
                        &items,
                    )?
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
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RequestedSelection {
    fields: Option<Vec<String>>,
    #[serde(default)]
    factories: Option<BTreeMap<String, String>>,
    #[serde(default)]
    inferred_factories: Vec<String>,
    #[serde(default)]
    relations: BTreeMap<String, Option<String>>,
}
pub(super) fn from_request(
    project: &Project,
    index: &Index,
    model: &ModelIdentity,
    kind: &str,
    value: &serde_json::Value,
) -> Result<Selection, Error> {
    let requested: RequestedSelection = serde_json::from_value(value.clone())
        .map_err(|_| Error::Invalid("Invalid related-model output selections".into()))?;
    let names = requested
        .fields
        .unwrap_or_else(|| fields(index, model, kind, None));
    let fields = names.iter().cloned().collect::<BTreeSet<_>>();
    if names.len() != fields.len() {
        return Err(Error::Invalid("Duplicate selected model fields".into()));
    }
    let defaults = if kind == "factory" {
        crate::generator_workspace::factory_defaults::for_existing(
            project,
            index,
            &model.full,
            &fields,
        )
    } else {
        BTreeMap::new()
    };
    let (factories, inferred_factories) = if let Some(factories) = requested.factories {
        let inferred = requested
            .inferred_factories
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        if inferred.len() != requested.inferred_factories.len() {
            return Err(Error::Stale);
        }
        (factories, inferred)
    } else {
        (
            defaults
                .iter()
                .map(|(field, suggestion)| (field.clone(), suggestion.class.clone()))
                .collect(),
            defaults.keys().cloned().collect(),
        )
    };
    let selected = Selection {
        fields,
        factories,
        inferred_factories,
        relations: requested.relations,
        runtime: None,
    };
    selected.validate(project, index, model, kind)?;
    Ok(selected)
}
pub(super) fn catalog(
    project: &Project,
    index: &Index,
    model: &ModelIdentity,
) -> serde_json::Value {
    let related = related_facts::inventory(index, project);
    let names = fields(index, model, "factory", None);
    let bindings = crate::generator_workspace::factory_defaults::bindings(project, index);
    let mut factory_choices = related
        .iter()
        .filter(|fact| fact.kind == "Factory")
        .map(|fact| (fact.label.clone(), fact.model.clone()))
        .collect::<BTreeMap<_, _>>();
    factory_choices.extend(
        bindings
            .into_iter()
            .map(|(class, binding)| (class, binding.model)),
    );
    let defaults = crate::generator_workspace::factory_defaults::for_existing(
        project,
        index,
        &model.full,
        &names.iter().cloned().collect(),
    );
    serde_json::json!({"model":model.full,"table":model.table,"factoryFields":names,"factoryDefaults":defaults,"resourceFields":fields(index,model,"resource",None),"relations":relations(index,project,model).into_iter().map(|fact|serde_json::json!({"name":fact.name,"target":fact.target,"many":fact.many})).collect::<Vec<_>>(),"factories":factory_choices.into_iter().map(|(class,model)|serde_json::json!({"class":class,"model":model})).collect::<Vec<_>>(),"resources":related.iter().filter(|fact|fact.kind=="Resource").map(|fact|serde_json::json!({"class":fact.label,"model":fact.model})).collect::<Vec<_>>()})
}

impl Selection {
    pub(super) fn runtime_note(&self) -> String {
        self.runtime
            .as_ref()
            .map(|(runner, snapshot)| {
                crate::ui_text::format("final.related.runtimeNote","Captured database fields: {runner} · connection {connection} · captured at {captured}\n",&[("runner",runner.name()),("connection",&snapshot["connection"].to_string()),("captured",&snapshot["capturedAt"].to_string())])
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
        let defaults = if kind == "factory" {
            crate::generator_workspace::factory_defaults::for_existing(
                project,
                index,
                &model.full,
                &self.fields,
            )
        } else {
            BTreeMap::new()
        };
        if self.inferred_factories.len() > 32
            || self
                .inferred_factories
                .iter()
                .any(|field| !self.factories.contains_key(field))
        {
            return Err(Error::Stale);
        }
        let bindings = if !self.factories.is_empty() {
            crate::generator_workspace::factory_defaults::bindings(project, index)
        } else {
            BTreeMap::new()
        };
        for (field, class) in &self.factories {
            if (!field.ends_with("_id") && !defaults.contains_key(field))
                || !self.fields.contains(field)
            {
                return Err(Error::Stale);
            }
            if self.inferred_factories.contains(field) {
                let suggestion = defaults
                    .get(field)
                    .filter(|suggestion| suggestion.class == *class)
                    .ok_or(Error::Stale)?;
                evidence.push_str(&suggestion.evidence);
            }
            if let Some(binding) = bindings.get(class) {
                evidence.push_str(&binding.evidence);
            } else {
                evidence.push_str(&prove(class, "Factory", None)?);
            }
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

pub(super) fn ui_error(error: Error) -> Error {
    let translate = |message: String| match message.as_str() {
        "Select from at most 128 model fields" => {
            crate::ui_text::t("final.related.error0", &message)
        }
        "Map at most 32 foreign keys per generation" => {
            crate::ui_text::t("final.related.error1", &message)
        }
        "Select from at most 64 model relations" => {
            crate::ui_text::t("final.related.error2", &message)
        }
        "Invalid related-model output selections" => {
            crate::ui_text::t("final.related.error3", &message)
        }
        "Duplicate selected model fields" => crate::ui_text::t("final.related.error4", &message),
        "A resource field and relation cannot share an output key" => {
            crate::ui_text::t("final.related.error5", &message)
        }
        "Related generation evidence exceeds 4 MiB" => {
            crate::ui_text::t("final.related.error6", &message)
        }
        "Module generation roots are ambiguous" => {
            crate::ui_text::t("final.related.error7", &message)
        }
        _ => message,
    };
    match error {
        Error::Invalid(message) => Error::Invalid(translate(message)),
        Error::Failed(message) => Error::Failed(translate(message)),
        other => other,
    }
}
