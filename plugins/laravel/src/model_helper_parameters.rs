//! Project-scoped helper preferences in plugin storage, never application files.
use openidea_plugin::{Error, Invocation, PickItem, Project, host};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Parameters {
    pub version: u32,
    pub storage: String,
    pub relation_counts: bool,
    pub date_class: String,
}
impl Default for Parameters {
    fn default() -> Self {
        Self { version: 1, storage: "helpers".into(), relation_counts: false, date_class: "Illuminate\\Support\\Carbon".into() }
    }
}
fn key(project: &Project) -> Result<String, Error> { Ok(format!("laravel.helper-parameters:{}", project.root()?)) }
fn validate(value: &Parameters) -> Result<(), Error> {
    if value.version != 1 || !matches!(value.storage.as_str(), "helpers" | "preserve" | "overwrite") || !crate::generator_plan::qualified(&value.date_class) || value.date_class.starts_with('\\') || value.date_class.len() > 256 {
        return Err(Error::Invalid("Invalid persisted helper parameters".into()));
    }
    Ok(())
}
pub(crate) fn load(project: &Project) -> Result<Parameters, Error> {
    let raw = match host::storage::get(&key(project)?) { Ok(value) => value, Err(Error::Denied(_)) => return Ok(Parameters::default()), Err(error) => return Err(error) };
    let Some(raw) = raw else { return Ok(Parameters::default()) };
    if raw.len() > 4096 { return Err(Error::Invalid("Persisted helper parameters exceed the supported size".into())); }
    let value: Parameters = serde_json::from_str(&raw).map_err(|_| Error::Invalid("Persisted helper parameters are malformed".into()))?;
    validate(&value)?;
    Ok(value)
}
fn owner(ctx: &Invocation) -> Result<Project, Error> {
    let project = ctx.project().ok_or("Open a Laravel project first")?;
    let root = project.root()?;
    if !host::projects()?.iter().any(|p| p.root().ok().as_deref() == Some(&root)) { return Err(Error::Stale); }
    Ok(project)
}
pub(crate) fn configure(ctx: &Invocation) -> Result<(), Error> {
    let project = owner(ctx)?;
    let root = project.root()?;
    let storage_key = key(&project)?;
    let before = host::storage::get(&storage_key)?;
    let current = load(&project)?;
    let storage = host::pick(&format!("Helper storage (saved: {})", current.storage), &[
        PickItem::new("helpers", "Only in plugin-owned helper files"),
        PickItem::new("preserve", "In model PHPDoc; preserve handwritten documentation"),
        PickItem::new("overwrite", "In model PHPDoc; replace attached PHPDoc after review"),
    ])?.ok_or(Error::Cancelled)?;
    let counts = host::pick(&format!("Relationship count annotations (saved: {})", current.relation_counts), &[
        PickItem::new("off", "Do not synthesize relation count attributes"),
        PickItem::new("on", "Include optional withCount attributes for proven many relations"),
    ])?.ok_or(Error::Cancelled)?;
    if !matches!(counts.as_str(), "on" | "off") { return Err(Error::Stale); }
    let date_class = host::input("Declared default date annotation class (type hint only; no PHP runs)", Some(&current.date_class))?.ok_or(Error::Cancelled)?.trim().trim_start_matches('\\').to_owned();
    let value = Parameters { version: 1, storage, relation_counts: counts == "on", date_class };
    validate(&value)?;
    let raw = serde_json::to_string_pretty(&value).map_err(|_| Error::Stale)?;
    let preview = format!("Project: {root}\nSave these project-scoped plugin preferences:\n{raw}\n\nStorage modes still require review before files are changed. The date class is an authored annotation assumption; it does not change runtime dates. Save parameters?");
    if !ui_kit::prompts::confirm_exact("Save helper parameters?", &preview)? { return Err(Error::Cancelled); }
    if owner(ctx)?.root()? != root || host::storage::get(&storage_key)? != before { return Err(Error::Stale); }
    host::storage::set(&storage_key, &raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    #[test]
    fn reviewed_parameters_persist_only_after_confirmation_and_cancel_preserves_them(){
        let mock=MockHost::install();mock.add_file("artisan","<?php");
        let project=host::projects().unwrap().remove(0);
        assert_eq!(load(&project).unwrap(),Parameters::default());
        for answer in ["preserve","on","App\\Support\\ProjectDate","yes"]{mock.answer_next_prompt(Some(&[answer]));}
        configure(&mock.invocation()).unwrap();
        let saved=load(&project).unwrap();assert_eq!(saved.storage,"preserve");assert!(saved.relation_counts);assert_eq!(saved.date_class,"App\\Support\\ProjectDate");
        let raw=host::storage::get(&key(&project).unwrap()).unwrap();
        for answer in ["overwrite","off","Other\\Date"]{mock.answer_next_prompt(Some(&[answer]));}mock.answer_next_prompt(None);
        assert_eq!(configure(&mock.invocation()),Err(Error::Cancelled));
        assert_eq!(host::storage::get(&key(&project).unwrap()).unwrap(),raw);
        assert!(mock.process_runs().is_empty());
    }
    #[test]
    fn persisted_parameters_validate_schema_and_refuse_unknown_settings(){
        let mock=MockHost::install();let project=host::projects().unwrap().remove(0);
        for raw in [r#"{"version":2,"storage":"helpers","relationCounts":false,"dateClass":"Carbon"}"#,r#"{"version":1,"storage":"helpers","relationCounts":false,"dateClass":"Carbon","unexpected":true}"#,"invalid"] {
            host::storage::set(&key(&project).unwrap(),raw).unwrap();assert!(load(&project).is_err());
        }
        assert!(mock.process_runs().is_empty());
    }
}
