//! The options each "create missing ..." fix asks for before its exact preview, as the creation wizards of the standard
//! actions do. The type name is fixed by the reference (the wizard of the original shows it read-only); these are the choices
//! that the standard repository, projection, DTO and entity actions offer besides the name.
use super::*;
use super::dto_conventions::{self, Serializable};
use super::projection_action::Form;

fn pick_key(title: &str, items: &[PickItem]) -> Result<String, Error> {
    let key = host::pick(title, items)?.ok_or(Error::Cancelled)?;
    if !items.iter().any(|item| item.key == key) {
        return Err("Choose one of the offered options.".into());
    }
    Ok(key)
}

fn pick_keys(title: &str, items: &[PickItem]) -> Result<Vec<String>, Error> {
    let chosen = host::pick_many(title, items)?.ok_or(Error::Cancelled)?;
    let mut seen = HashSet::new();
    if chosen.iter().any(|key| !items.iter().any(|item| item.key == *key) || !seen.insert(key.as_str())) {
        return Err("Choose only offered options, each once.".into());
    }
    Ok(chosen)
}

/// Parent interface and whether `JpaSpecificationExecutor` is added.
pub(super) fn repository(jakarta: bool) -> Result<(String, bool), Error> {
    let items: Vec<_> = repository_action::parents(jakarta).iter().map(|(simple, _)| PickItem::new(simple, simple)).collect();
    let parent = pick_key("Repository parent interface", &items)?;
    let choices = [PickItem::new("no", "No"), PickItem::new("yes", "Yes, also extend JpaSpecificationExecutor")];
    let specification = pick_key("Add JpaSpecificationExecutor?", &choices)? == "yes";
    Ok((parent, specification))
}

pub(super) fn projection_form(record_offered: bool) -> Result<Form, Error> {
    let mut items = vec![PickItem::new("interface", "Closed interface projection (getters)")];
    if record_offered {
        items.push(PickItem::new("record", "Java record (constructor projection, Java 16+)"));
    }
    Ok(match pick_key("Projection form", &items)?.as_str() {
        "record" => Form::Record,
        _ => Form::Interface,
    })
}

/// How the DTO is made serializable; the first choice follows the DTO Declaration setting.
pub(super) fn dto_serialization() -> Result<Serializable, Error> {
    let configured = dto_conventions::serializable()?;
    let items = [
        PickItem::new("setting", "As configured in the DTO Declaration settings"),
        PickItem::new("none", "Not serializable"),
        PickItem::new("serializable", "Implements Serializable"),
        PickItem::new("serializable-with-version-uid", "Implements Serializable with serialVersionUID"),
    ];
    Ok(match pick_key("DTO serialization", &items)?.as_str() {
        "none" => Serializable::No,
        "serializable" => Serializable::Plain,
        "serializable-with-version-uid" => Serializable::WithVersionUid,
        _ => configured,
    })
}

pub(super) struct Mappings {
    pub version: bool,
    pub auditing: bool,
    pub equals: bool,
}

/// The "Additional mappings" of the entity wizard; `equals` only exists for entities (a mapped superclass has no `@Id`).
pub(super) fn additional_mappings(entity: bool) -> Result<Mappings, Error> {
    let mut items = vec![
        PickItem::new("version", "Version field (@Version)"),
        PickItem::new("auditing", "Auditing fields (createdAt, updatedAt; needs Spring Data JPA auditing)"),
    ];
    if entity {
        items.push(PickItem::new("equals", "equals() and hashCode() on the identifier"));
    }
    let chosen = pick_keys("Additional mappings", &items)?;
    Ok(Mappings { version: chosen.iter().any(|key| key == "version"), auditing: chosen.iter().any(|key| key == "auditing"), equals: chosen.iter().any(|key| key == "equals") })
}
