//! Cached class/model field inventories for declarative completion rules.
use super::*;
use crate::eloquent_members::{metadata_inference as inference, semantic};
pub(super) fn supported(category: &str) -> bool {
    matches!(category, "classFields" | "eloquentFields" | "eloquentCasts")
}
fn owns(metadata: &Metadata, path: &str) -> bool {
    path.is_empty()
        || metadata.project_root.is_empty()
        || host::project_of(path)
            .and_then(|project| project.root().ok())
            .as_deref()
            == Some(metadata.project_root.as_str())
}
fn fact(metadata: &Metadata, current: Option<&ClassFact>, full: &str) -> Option<ClassFact> {
    if let Some(current) = current.filter(|fact| fact.full.eq_ignore_ascii_case(full)) {
        return Some(current.clone());
    }
    let key = full.trim_matches('\\').to_ascii_lowercase();
    if metadata.ambiguous_classes.contains(&key) {
        return None;
    }
    metadata
        .class_facts
        .get(&key)
        .filter(|fact| owns(metadata, &fact.path))
        .cloned()
}
fn property_target(
    metadata: &Metadata,
    current: Option<&ClassFact>,
    full: &str,
    name: &str,
) -> Option<String> {
    let mut pending = vec![full.to_owned()];
    let mut seen = BTreeSet::new();
    while let Some(full) = pending.pop() {
        if seen.len() > 32 || !seen.insert(full.to_ascii_lowercase()) {
            continue;
        }
        let Some(class) = fact(metadata, current, &full) else {
            continue;
        };
        if let Some(property) = class.properties.get(name) {
            return property.class_target.clone();
        }
        pending.extend(class.parents.into_iter().rev());
        pending.extend(class.traits.into_iter().rev());
    }
    None
}
fn target(
    metadata: &Metadata,
    current: Option<&ClassFact>,
    receiver: Option<&str>,
    options: &Value,
    category: &str,
) -> Option<String> {
    let containing = || current.map(|fact| fact.full.clone());
    if category == "classFields" {
        match options["fetchClass"].as_str().unwrap_or("current") {
            "current" => receiver.map(str::to_owned),
            value if value.starts_with("containingClassField:") => property_target(
                metadata,
                current,
                &containing()?,
                value.strip_prefix("containingClassField:")?,
            ),
            value => class_name(&Value::String(value.into())),
        }
    } else {
        match options["eloquentFields"]["fetch"]
            .as_str()
            .unwrap_or("current")
        {
            "current" => receiver.map(str::to_owned),
            "currentClass" => containing(),
            "classField" => property_target(
                metadata,
                current,
                &containing().or_else(|| receiver.map(str::to_owned))?,
                options["eloquentFields"]["classField"]
                    .as_str()
                    .unwrap_or("model"),
            ),
            _ => None,
        }
    }
}
pub(super) fn inventory(
    index: &Index,
    category: &str,
    options: &Value,
    source: &str,
    at: usize,
    site: &Site,
) -> Names {
    let metadata = &index.metadata;
    let receiver = site.identity.class.as_deref();
    let mut out = Names::new();
    let owner = |path: &str| owns(metadata, path);
    if category == "eloquentCasts" {
        for cast in [
            "array",
            "bool",
            "boolean",
            "collection",
            "date",
            "datetime",
            "decimal",
            "double",
            "encrypted",
            "encrypted:array",
            "encrypted:collection",
            "encrypted:object",
            "float",
            "immutable_date",
            "immutable_datetime",
            "int",
            "integer",
            "json",
            "object",
            "real",
            "string",
            "timestamp",
        ] {
            out.add(
                cast,
                Some("Eloquent cast".into()),
                "",
                Position::new(0, 0),
                0,
            );
        }
        for fact in metadata
            .class_facts
            .values()
            .filter(|fact| owner(&fact.path))
        {
            if fact.cast_enum
                || [
                    "Illuminate\\Contracts\\Database\\Eloquent\\CastsAttributes",
                    "Illuminate\\Contracts\\Database\\Eloquent\\CastsInboundAttributes",
                    "Illuminate\\Contracts\\Database\\Eloquent\\Castable",
                ]
                .iter()
                .any(|parent| inference::is_subtype_fact(metadata, None, &fact.full, parent))
            {
                let mut entry = fact.entry.clone();
                entry.detail = Some("source cast class or enum".into());
                out.insert(fact.full.clone(), entry);
            }
        }
        return out;
    }
    let receiver = if category == "eloquentFields"
        && options["eloquentFields"]["fetch"]
            .as_str()
            .unwrap_or("current")
            == "current"
    {
        site.model.as_deref().or(receiver)
    } else {
        receiver
    };
    let Some(full) = target(
        metadata,
        site.containing.as_ref(),
        receiver,
        options,
        category,
    ) else {
        return out;
    };
    if category == "classFields" {
        let all = options["fieldsFilter"]["fetch"].as_str().map_or_else(
            || options["deep"].as_bool().unwrap_or(false),
            |fetch| fetch == "all",
        );
        let modifiers = options["fieldsFilter"]
            .get("modifier")
            .map(strings)
            .unwrap_or_else(|| vec!["public".into()]);
        let mut pending = vec![full];
        let mut seen = BTreeSet::new();
        let mut shadowed = BTreeSet::new();
        while let Some(full) = pending.pop() {
            if seen.len() > 32 || !seen.insert(full.to_ascii_lowercase()) {
                continue;
            }
            let Some(class) = fact(metadata, site.containing.as_ref(), &full) else {
                continue;
            };
            for (name, property) in class.properties {
                if shadowed.insert(name.clone()) && modifiers.contains(&property.visibility) {
                    out.insert(name, property.entry);
                }
            }
            if all {
                pending.extend(class.parents.into_iter().rev());
            }
            pending.extend(class.traits.into_iter().rev());
        }
        return out;
    }
    let complete = options["eloquentFields"]["complete"]
        .as_str()
        .unwrap_or("fields");
    let Some(mut model) = semantic::model_facts(index, &full, &owner) else {
        return out;
    };
    if complete == "databaseFields" {
        return model
            .table
            .as_ref()
            .and_then(|table| index.columns.get(table))
            .map(|names| {
                names
                    .iter()
                    .filter(|(_, entry)| owner(&entry.path))
                    .map(|(name, entry)| (name.clone(), entry.clone()))
                    .collect()
            })
            .unwrap_or_default();
    }
    if complete == "deepRelations" {
        let options = &options["eloquentFields"];
        let relation_separator = options["relationsSeparator"].as_str().unwrap_or(".");
        let field_separator = options["relationFieldsSeparator"].as_str().unwrap_or(":");
        let Some(contents) = inference::decode_literal(
            &source[site.literal.start..at.min(site.literal.end)],
            site.quote,
        ) else {
            return out;
        };
        let (path, fields) = contents
            .split_once(field_separator)
            .map_or((contents.as_str(), false), |(path, _)| (path, true));
        let segments: Vec<_> = path.split(relation_separator).collect();
        let through = if fields {
            segments.len()
        } else {
            segments.len().saturating_sub(1)
        };
        for name in segments.into_iter().take(through) {
            let Some(relation) = model
                .relations
                .iter()
                .find(|relation| relation.name == name)
            else {
                return out;
            };
            let Some(next) = semantic::model_facts(index, &relation.target, &owner) else {
                return out;
            };
            model = next;
        }
        if fields {
            for field in model.fields {
                out.insert(field.name, field.entry);
            }
        } else {
            for relation in model.relations {
                out.insert(relation.name, relation.entry);
            }
        }
        return out;
    }
    if matches!(complete, "fields" | "fieldsAndRelations") {
        for field in model.fields {
            let mut entry = field.entry;
            if entry.detail.is_none() {
                entry.detail = field.value_type;
            }
            out.insert(field.name, entry);
        }
    }
    if matches!(complete, "relations" | "fieldsAndRelations") {
        for relation in model.relations {
            out.insert(relation.name, relation.entry);
        }
    }
    out
}
pub(super) fn range(options: &Value, source: &str, at: usize, site: &Site) -> Option<Range<usize>> {
    let delimiter = options["delimiter"]
        .as_str()
        .filter(|delimiter| !delimiter.is_empty());
    let fields = &options["eloquentFields"];
    if delimiter.is_none() && fields["complete"].as_str() != Some("deepRelations") {
        return Some(site.literal.clone());
    }
    let decoded = inference::decoded_offsets(source.get(site.literal.clone())?, site.quote)?;
    let value = &decoded.value;
    let relative = decoded.decoded_at(at.checked_sub(site.literal.start)?)?;
    let (from, to) = if let Some(delimiter) = delimiter {
        (
            value[..relative]
                .rfind(delimiter)
                .map_or(0, |i| i + delimiter.len()),
            relative
                + value[relative..]
                    .find(delimiter)
                    .unwrap_or(value.len() - relative),
        )
    } else {
        let relations = fields["relationsSeparator"].as_str().unwrap_or(".");
        let field = fields["relationFieldsSeparator"].as_str().unwrap_or(":");
        let names = fields["fieldsSeparator"].as_str().unwrap_or(",");
        let start = value[..relative].rfind(field).map(|i| i + field.len());
        let (from, separators) = if let Some(start) = start {
            (
                value[start..relative]
                    .rfind(names)
                    .map_or(start, |i| start + i + names.len()),
                vec![names],
            )
        } else {
            (
                value[..relative]
                    .rfind(relations)
                    .map_or(0, |i| i + relations.len()),
                vec![relations, field],
            )
        };
        let to = separators
            .into_iter()
            .filter_map(|separator| value[relative..].find(separator))
            .min()
            .map_or(value.len(), |i| relative + i);
        (from, to)
    };
    Some(site.literal.start + decoded.raw_at(from)?..site.literal.start + decoded.raw_at(to)?)
}

pub(super) fn validate(category: &str, options: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if category == "classFields" {
        if options.get("fetchClass").is_some_and(|v| {
            v.as_str().is_none_or(|value| {
                value != "current"
                    && !value
                        .strip_prefix("containingClassField:")
                        .is_some_and(identifier)
                    && class_name(v).is_none()
            })
        }) {
            out.push("invalid classFields.fetchClass".into());
        }
        if options.get("deep").is_some_and(|v| !v.is_boolean()) {
            out.push("classFields.deep must be boolean".into());
        }
        if options.get("delimiter").is_some_and(|v| !v.is_string()) {
            out.push("classFields.delimiter must be a string".into());
        }
        if let Some(filter) = options.get("fieldsFilter") {
            if filter.as_object().is_none_or(|filter| {
                filter
                    .keys()
                    .any(|key| !matches!(key.as_str(), "fetch" | "modifier"))
            }) {
                out.push("invalid classFields.fieldsFilter".into());
            }
            if filter
                .get("fetch")
                .is_some_and(|v| !matches!(v.as_str(), Some("all" | "own")))
            {
                out.push("invalid classFields.fieldsFilter.fetch".into());
            }
            if filter.get("modifier").is_some_and(|v| {
                v.as_array().is_none_or(|values| {
                    values
                        .iter()
                        .any(|v| !matches!(v.as_str(), Some("public" | "protected" | "private")))
                })
            }) {
                out.push("invalid classFields.fieldsFilter.modifier".into());
            }
        }
    }
    if category == "eloquentFields" {
        if let Some(value) = options.get("eloquentFields") {
            if value.as_object().is_none_or(|v| {
                v.keys().any(|key| {
                    !matches!(
                        key.as_str(),
                        "fetch"
                            | "classField"
                            | "complete"
                            | "relationsSeparator"
                            | "relationFieldsSeparator"
                            | "fieldsSeparator"
                    )
                })
            }) {
                out.push("invalid options.eloquentFields object".into());
            }
            if value.get("fetch").is_some_and(|v| {
                !matches!(v.as_str(), Some("current" | "classField" | "currentClass"))
            }) {
                out.push("invalid eloquentFields.fetch".into());
            }
            if value
                .get("classField")
                .is_some_and(|v| v.as_str().is_none_or(|s| !identifier(s)))
            {
                out.push("invalid eloquentFields.classField".into());
            }
            if value.get("complete").is_some_and(|v| {
                !matches!(
                    v.as_str(),
                    Some(
                        "fields"
                            | "relations"
                            | "fieldsAndRelations"
                            | "deepRelations"
                            | "databaseFields"
                    )
                )
            }) {
                out.push("invalid eloquentFields.complete".into());
            }
            for key in [
                "relationsSeparator",
                "relationFieldsSeparator",
                "fieldsSeparator",
            ] {
                if value.get(key).is_some_and(|v| {
                    v.as_str().is_none_or(|s| {
                        s.is_empty() || s.len() > 32 || s.chars().any(char::is_control)
                    })
                }) {
                    out.push(format!("invalid eloquentFields.{key}"));
                }
            }
        }
    }
    out
}
