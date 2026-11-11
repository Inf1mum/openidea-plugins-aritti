//! Adapt an existing model to the native generator configuration without
//! enabling or generating the model itself.
use super::*;
use crate::generator_plan::Draft;

pub(super) fn effective(
    project: &Project,
    model: &ModelIdentity,
    kind: &str,
) -> Result<(Draft, String, bool), Error> {
    effective_in_module(project, model, kind, None)
}
pub(super) fn effective_in_module(
    project: &Project,
    model: &ModelIdentity,
    kind: &str,
    requested: Option<Option<&str>>,
) -> Result<(Draft, String, bool), Error> {
    let root = project.root()?;
    let bundle = crate::generator_presets::load(project)?;
    let catalog = bundle.catalog();
    let relative = model
        .path
        .strip_prefix(&format!("{root}/"))
        .ok_or(Error::Stale)?;
    let modules = catalog["modules"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|module| {
            module["sourceRoot"]
                .as_str()
                .is_some_and(|prefix| relative.starts_with(&format!("{prefix}/")))
        })
        .collect::<Vec<_>>();
    let longest = modules
        .iter()
        .filter_map(|module| module["sourceRoot"].as_str())
        .map(str::len)
        .max();
    let modules = modules
        .into_iter()
        .filter(|module| module["sourceRoot"].as_str().map(str::len) == longest)
        .collect::<Vec<_>>();
    if modules.len() > 1 && requested.is_none() {
        return Err(Error::Invalid(
            "Module generation roots are ambiguous".into(),
        ));
    }
    let namespace = resource_namespace(model)
        .strip_suffix("\\Http\\Resources")
        .ok_or(Error::Stale)?
        .to_owned();
    let source_root = relative
        .split_once("/Models/")
        .map(|(root, _)| root)
        .unwrap_or("app");
    let mut draft: Draft = serde_json::from_value(serde_json::json!({
        "name":model.full.rsplit('\\').next().ok_or(Error::Stale)?,
        "table":model.table,"namespace":namespace,"sourceRoot":source_root,
        "migrationStamp":"2000_01_01_000000"
    }))
    .map_err(|_| Error::Stale)?;
    bundle.apply_settings(&mut draft);
    let module_id = match requested {
        Some(value) => value.map(str::to_owned),
        None => modules
            .first()
            .map(|module| module["id"].as_str().map(str::to_owned).ok_or(Error::Stale))
            .transpose()?,
    };
    if requested == Some(None) {
        bundle.apply_application(&mut draft);
    }
    if let Some(id) = module_id.as_deref() {
        bundle
            .apply_module(&mut draft, id)
            .map_err(Error::Invalid)?;
        if requested.is_none() && !model.full.starts_with(&format!("{}\\", draft.namespace)) {
            return Err(Error::Stale);
        }
    }
    bundle
        .apply_related_generator_options(&mut draft, kind, module_id.as_deref())
        .map_err(Error::Invalid)?;
    draft.settings.validate().map_err(Error::Invalid)?;
    Ok((
        draft,
        bundle.fingerprint(),
        module_id.is_some() || requested.is_some(),
    ))
}

pub(super) fn render(
    project: &Project,
    model: &ModelIdentity,
    kind: &str,
    draft: &Draft,
    requested: Option<Option<&str>>,
    generated_class: &str,
    generated: &str,
) -> Result<String, Error> {
    let bundle = crate::generator_presets::load(project)?;
    let catalog = bundle.catalog();
    let root = project.root()?;
    let relative = model
        .path
        .strip_prefix(&format!("{root}/"))
        .ok_or(Error::Stale)?;
    let module = match requested {
        Some(value) => value.map(str::to_owned),
        None => catalog["modules"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|module| {
                module["sourceRoot"]
                    .as_str()
                    .is_some_and(|path| relative.starts_with(&format!("{path}/")))
            })
            .max_by_key(|module| module["sourceRoot"].as_str().map(str::len).unwrap_or(0))
            .and_then(|module| module["id"].as_str())
            .map(str::to_owned),
    };
    bundle
        .render_related_output(
            draft,
            kind,
            module.as_deref(),
            generated_class,
            &model.full,
            generated,
        )
        .map_err(Error::Invalid)
}
