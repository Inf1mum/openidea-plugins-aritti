//! "Selected Entities" scope of Init DDL and the model diff (G106, G107): with `schemaLimitToEntities` on, the schema
//! baseline, Flyway and Liquibase actions ask which of the entities in the chosen persistence unit take part.
//! An empty selection keeps the whole scope. The selection is recorded in the baseline; a later run must repeat it.
use super::*;
use crate::schema_snapshot::{self, Scope};

const MAX_ENTITIES: usize = 400;

fn limiting() -> bool {
    host::setting_bool("schemaLimitToEntities").unwrap_or(false)
}

fn simple(qualified: &str) -> &str {
    qualified.rsplit('.').next().unwrap_or(qualified)
}

pub(crate) fn narrow(project: &Project, root: &str, scope: Scope) -> Result<Scope, Error> {
    if !limiting() {
        return Ok(scope);
    }
    let docs = crate::file_actions::migration_action::model_docs(project, root)?;
    let names = schema_snapshot::entity_names(&docs, &scope);
    if names.is_empty() {
        return Err("No entities were found in the chosen scope to select from.".into());
    }
    if names.len() > MAX_ENTITIES {
        return Err(format!("More than {MAX_ENTITIES} entities; the selection list refuses.").into());
    }
    let items: Vec<PickItem> = names.iter().enumerate().map(|(i, n)| PickItem::new(&i.to_string(), simple(n)).description(n)).collect();
    let picked = host::pick_many("Entities to include in the schema scope (leave empty for all)", &items)?.ok_or(Error::Cancelled)?;
    if picked.is_empty() {
        return Ok(scope);
    }
    let mut classes = Vec::new();
    for key in picked {
        let at: usize = key.parse().map_err(|_| Error::Stale)?;
        classes.push(names.get(at).ok_or(Error::Stale)?.clone());
    }
    classes.sort();
    classes.dedup();
    Ok(Scope::Entities { base: Box::new(scope), classes })
}

/// A baseline taken for a selection can only be diffed by a run with the same selection, and vice versa.
pub(crate) fn check_baseline(baseline_text: &str, scope: &Scope) -> Result<(), Error> {
    let recorded = schema_snapshot::recorded_entities(baseline_text);
    let current = match scope {
        Scope::Entities { classes, .. } => Some(classes.clone()),
        _ => None,
    };
    if recorded == current {
        return Ok(());
    }
    let describe = |list: &Option<Vec<String>>| list.as_ref().map_or("all entities of its scope".to_owned(), |l| format!("{} selected entities", l.len()));
    Err(format!(
        "The schema baseline covers {}, this run covers {}; select the same entities (schemaLimitToEntities) or take a new baseline.",
        describe(&recorded),
        describe(&current)
    )
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_must_repeat_the_selection() {
        let all = Scope::All;
        let some = Scope::Entities { base: Box::new(Scope::All), classes: vec!["a.B".into()] };
        let json = format!(r#"{{"scope":{{"kind":"all","entities":["a.B"]}}}}"#);
        assert!(check_baseline(&json, &some).is_ok());
        assert!(check_baseline(&json, &all).is_err());
        assert!(check_baseline("{}", &some).is_err());
        assert!(check_baseline("{}", &all).is_ok());
    }
}
