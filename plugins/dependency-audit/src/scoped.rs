//! Plugin-owned exact-coordinate suppressions; independent of inspection profiles/settings.
use crate::*;
use openidea_plugin::PickItem;

const MAX_ENTRIES: usize = 200;
const MAX_BYTES: usize = 256 << 10;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ignore {
    ecosystem: String,
    name: String,
    version: String,
    id: String,
    aliases: Vec<String>,
    reason: String,
    at: u64,
}
fn key(root: &str) -> Result<String, Error> {
    if !root.starts_with('/') || root.len() > 4096 || root.chars().any(char::is_control) || root.split('/').any(|p| matches!(p, "." | ".."))
    {
        return Err(Error::Invalid("Project identity is unavailable.".into()));
    }
    Ok(format!("scoped-ignores:{root}"))
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn valid(r: &Ignore) -> bool {
    matches!(r.ecosystem.as_str(), "Maven" | "npm" | "Packagist" | "crates.io" | "PyPI")
        && text(&r.name, 512)
        && text(&r.version, 128)
        && osv::is_safe_id(&r.id)
        && r.aliases.len() <= 100
        && r.aliases.iter().all(|a| osv::is_safe_id(a))
        && text(&r.reason, 512)
}
fn read(root: &str) -> Result<(Option<String>, Vec<Ignore>), Error> {
    let raw = host::storage::get(&key(root)?)?;
    let Some(value) = &raw else {
        return Ok((None, Vec::new()));
    };
    if value.len() > MAX_BYTES {
        return Err(Error::Invalid("Scoped ignores exceed the storage bound; no suppression applied.".into()));
    }
    let rows: Vec<Ignore> =
        serde_json::from_str(value).map_err(|_| Error::Invalid("Scoped ignores are unreadable; no suppression applied.".into()))?;
    if rows.len() > MAX_ENTRIES || rows.iter().any(|r| !valid(r)) {
        return Err(Error::Invalid("Scoped ignores are invalid; no suppression applied.".into()));
    }
    Ok((raw, rows))
}
pub(crate) fn load(root: &str) -> Vec<Ignore> {
    read(root).map(|(_, r)| r).unwrap_or_default()
}
fn name(eco: &str, value: &str) -> String {
    if eco == "PyPI" { manifest::normalise_pypi(value) } else { value.to_owned() }
}
pub(crate) fn matches(rows: &[Ignore], eco: &str, package: &str, version: &str, id: &str, aliases: &[String]) -> bool {
    let package = name(eco, package);
    rows.iter().any(|r| {
        r.ecosystem == eco
            && r.name == package
            && r.version == version
            && std::iter::once(r.id.as_str())
                .chain(r.aliases.iter().map(String::as_str))
                .any(|known| known.eq_ignore_ascii_case(id) || aliases.iter().any(|a| known.eq_ignore_ascii_case(a)))
    })
}
pub(crate) fn filter(rows: &[Ignore], finding: &mut Finding) -> bool {
    finding.advisories.retain(|a| !matches(rows, finding.dep.ecosystem.osv(), &finding.dep.name, &finding.dep.version, &a.id, &a.aliases));
    !finding.advisories.is_empty()
}
pub(crate) fn aliases(row: &Row, id: &str) -> Vec<String> {
    row.details
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.aliases.clone())
        .or_else(|| cached_advisory(id).filter(|a| a.id == id).map(|a| a.aliases))
        .unwrap_or_default()
}
pub(crate) fn visible_count(root: &str, report: &Report) -> usize {
    let scopes = load(root);
    if scopes.is_empty() {
        return report.rows.len();
    }
    report
        .rows
        .iter()
        .filter(|r| r.advisories.iter().any(|id| !matches(&scopes, resources::ecosystem(r), &r.name, &r.version, id, &aliases(r, id))))
        .count()
}
fn selected(root: &str, report: &Report, key: &str) -> Result<Ignore, Error> {
    let key = key.strip_prefix("advisory:").unwrap_or(key);
    let mut selected = None;
    for row in &report.rows {
        for id in &row.advisories {
            if format!("{}|{id}", actions::base(row)) != key {
                continue;
            }
            if !row.path.starts_with(&format!("{}/", root.trim_end_matches('/')))
                || row.path.split('/').any(|p| matches!(p, "." | ".."))
                || selected.is_some()
                || !osv::is_safe_id(id)
            {
                return Err(Error::Stale);
            }
            let eco = resources::ecosystem(row);
            let mut aliases =
                aliases(row, id).into_iter().filter(|a| osv::is_safe_id(a)).map(|a| a.to_ascii_uppercase()).collect::<Vec<_>>();
            aliases.sort();
            aliases.dedup();
            let record = Ignore {
                ecosystem: eco.into(),
                name: name(eco, &row.name),
                version: row.version.clone(),
                id: id.to_ascii_uppercase(),
                aliases,
                reason: "pending".into(),
                at: now(),
            };
            if !valid(&record) {
                return Err(Error::Invalid("The selected package/advisory facts cannot be safely scoped.".into()));
            }
            selected = Some(record);
        }
    }
    selected.ok_or(Error::Stale)
}
fn summary(root: &str, r: &Ignore) -> String {
    format!(
        "Project: {root}\nPackage: {} / {}\nVersion: {} (exact)\nAdvisory: {}\nKnown aliases: {}\nReason: {}\nRecorded: Unix {}\n\nScope is based on the retained scan snapshot, not a new dependency resolution. This is plugin-owned storage, separate from global ignoredAdvisories and native inspection profiles. It does not mark the package safe. No expiry is configured.",
        r.ecosystem,
        r.name,
        r.version,
        r.id,
        r.aliases.join(", "),
        r.reason,
        r.at
    )
}
fn save(root: &str, before: &Option<String>, rows: &[Ignore]) -> Result<(), Error> {
    if host::storage::get(&key(root)?)? != *before {
        return Err(Error::Stale);
    }
    let json = serde_json::to_string(rows).map_err(|e| Error::Failed(e.to_string()))?;
    if rows.len() > MAX_ENTRIES || json.len() > MAX_BYTES || rows.iter().any(|r| !valid(r)) {
        return Err(Error::Invalid("Scoped-ignore storage is full. Review and restore an entry first.".into()));
    }
    host::storage::set(&key(root)?, &json)
}
fn owner(ctx: &Invocation, root: &str) -> Result<(), Error> {
    if ctx.project().and_then(|p| p.root().ok()).as_deref() != Some(root) {
        return Err(Error::Stale);
    }
    Ok(())
}
/// Cached editor finding path, separate from stored-report selection authority.
pub(crate) fn ignore_editor(root:&str,f:&Finding,id:&str,revalidate:impl Fn()->Result<(),Error>)->Result<(),Error> {
    let (before,mut ignores)=read(root)?;
    let row=Row{path:f.dep.path.clone(),line:0,name:f.dep.name.clone(),version:f.dep.version.clone(),fix:f.fix(),advisories:f.advisories.iter().map(|a|a.id.clone()).collect(),ecosystem:Some(f.dep.ecosystem.osv().into()),sources:Vec::new(),details:f.advisories.clone()};
    let key=format!("{}|{id}",actions::base(&row));
    let mut record=selected(root,&Report{rows:vec![row],..Report::default()},&key)?;
    if ignores.len()>=MAX_ENTRIES{return Err(Error::Invalid("Scoped-ignore storage is full; restore an entry first.".into()));}
    let Some(reason)=host::input("Reason for ignoring this exact cached editor finding (required, 512 bytes max)",None)? else{return Err(Error::Cancelled)};
    record.reason=reason.trim().into();if !valid(&record){return Err(Error::Invalid("Enter a nonempty reason of at most512bytes without controls.".into()));}
    let review=summary(root,&record).replace("retained scan snapshot","current source and cached OSV finding");
    if !ui_kit::prompts::confirm_exact("Ignore cached editor finding?",&review)?{return Err(Error::Cancelled);}
    revalidate()?;ignores.push(record);save(root,&before,&ignores)
}
impl DependencyAudit {
    pub(crate) fn scoped_command(&mut self, id: &str, ctx: &Invocation, selection: Option<&str>) -> Result<Option<String>, Error> {
        let project = ctx.project().ok_or("Open a project first.")?;
        let root = project.root()?;
        let (before, mut ignores) = read(&root)?;
        if id == "review-ignores" {
            if ignores.is_empty() {
                return Ok(Some("No project-scoped ignores are stored. Global ignoredAdvisories is a separate setting.".into()));
            }
            let items: Vec<_> = ignores
                .iter()
                .enumerate()
                .map(|(i, r)| PickItem {
                    key: i.to_string(),
                    label: format!("{} {} — {} / {}", r.name, r.version, r.id, r.ecosystem),
                    description: Some(format!("{} · Unix {}", r.reason, r.at)),
                })
                .collect();
            let Some(index) = host::pick("Review scoped ignores — select one to restore", &items)? else { return Ok(None) };
            let index = index.parse::<usize>().ok().filter(|i| *i < ignores.len()).ok_or(Error::Stale)?;
            if !ui_kit::prompts::confirm_exact("Restore this scoped finding?", &summary(&root, &ignores[index]))? {
                return Ok(None);
            }
            owner(ctx, &root)?;
            ignores.remove(index);
            save(&root, &before, &ignores)?;
            self.invalidate_report();
            return Ok(Some("Scoped ignore restored. Retained report/cache evidence is visible again without a rescan; global ignoredAdvisories can still hide it.".into()));
        }
        let report_before = host::storage::get(&rows_key(&project)?)?;
        let report = stored_report(&project)?.ok_or(Error::Stale)?;
        let mut record = selected(&root, &report, selection.ok_or("Select an advisory row first.")?)?;
        if ignores.len() >= MAX_ENTRIES {
            return Err(Error::Invalid("Scoped-ignore storage is full. Review and restore an entry first.".into()));
        }
        let Some(reason) = host::input("Reason for ignoring this exact project/package/version/advisory (required, 512 bytes max)", None)?
        else {
            return Ok(None);
        };
        record.reason = reason.trim().into();
        if !valid(&record) {
            return Err(Error::Invalid("Enter a nonempty reason of at most 512 bytes without control characters.".into()));
        }
        if !ui_kit::prompts::confirm_exact("Ignore this exact finding?", &summary(&root, &record))? {
            return Ok(None);
        }
        owner(ctx, &root)?;
        let latest=actions::current_report(&project,&report_before)?;
        selected(&root,&latest,selection.ok_or(Error::Stale)?)?;
        ignores.push(record);
        save(&root, &before, &ignores)?;
        self.invalidate_report();
        Ok(Some("Exact finding ignored in this project. Evidence is retained; use Review scoped ignores to restore it. Global settings were not changed.".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn record() -> Ignore {
        Ignore {
            ecosystem: "PyPI".into(),
            name: "example-pkg".into(),
            version: "1.0.0".into(),
            id: "GHSA-EXAMPLE".into(),
            aliases: vec!["CVE-2026-1234".into()],
            reason: "Tracked risk acceptance".into(),
            at: 1,
        }
    }
    #[test]
    fn scope_is_exact_in_project_ecosystem_name_version_and_known_alias_identity() {
        let host = MockHost::install();
        let records = vec![record()];
        save("/project", &None, &records).unwrap();
        assert!(matches(&load("/project"), "PyPI", "Example_Pkg", "1.0.0", "cve-2026-1234", &[]));
        assert!(matches(&records, "PyPI", "example-pkg", "1.0.0", "NEW-ID", &["ghsa-example".into()]));
        for (eco, package, version, id) in [
            ("npm", "example-pkg", "1.0.0", "GHSA-EXAMPLE"),
            ("PyPI", "other", "1.0.0", "GHSA-EXAMPLE"),
            ("PyPI", "example-pkg", "1.0.1", "GHSA-EXAMPLE"),
            ("PyPI", "example-pkg", "1.0.0", "UNKNOWN"),
        ] {
            assert!(!matches(&records, eco, package, version, id, &[]));
        }
        assert!(!matches(&load("/another"), "PyPI", "example-pkg", "1.0.0", "GHSA-EXAMPLE", &[]));
        let body = summary("/project", &records[0]);
        for text in ["/project", "PyPI", "example-pkg", "1.0.0", "GHSA-EXAMPLE", "CVE-2026-1234", "Tracked risk acceptance", "No expiry"] {
            assert!(body.contains(text), "{body}");
        }
        assert!(host.requests().is_empty());
    }
    #[test]
    fn corrupt_oversized_denied_and_raced_storage_never_suppresses_or_overwrites() {
        let host = MockHost::install();
        for value in ["not json".into(), "x".repeat(MAX_BYTES + 1), serde_json::to_string(&vec![record(); MAX_ENTRIES + 1]).unwrap()] {
            host::storage::set("scoped-ignores:/project", &value).unwrap();
            assert!(load("/project").is_empty());
            assert!(read("/project").is_err());
            assert_eq!(host.storage()["scoped-ignores:/project"], value);
        }
        host::storage::delete("scoped-ignores:/project").unwrap();
        let before = read("/project").unwrap().0;
        host::storage::set("scoped-ignores:/project", "[]").unwrap();
        assert!(matches!(save("/project", &before, &[record()]), Err(Error::Stale)));
        assert_eq!(host.storage()["scoped-ignores:/project"], "[]");
        for root in ["", "anonymous", "/project/../other"] {
            assert!(key(root).is_err());
        }
        let host = host.with_capabilities(&[]);
        assert!(load("/project").is_empty());
        assert!(read("/project").is_err());
        assert_eq!(host.storage()["scoped-ignores:/project"], "[]");
    }
    #[test]
    fn unsafe_or_ambiguous_snapshot_ids_cannot_create_scopes() {
        let _host = MockHost::install();
        let row: Row = serde_json::from_str(
            r#"{"path":"/project/package.json","line":0,"name":"x","version":"1.0","fix":null,"advisories":["../bad"],"ecosystem":"npm"}"#,
        )
        .unwrap();
        let key = format!("{}|../bad", actions::base(&row));
        assert!(selected("/project", &Report { rows: vec![row], ..Report::default() }, &key).is_err());
        let row:Row=serde_json::from_str(r#"{"path":"/project/package.json","line":0,"name":"x","version":"1.0","fix":null,"advisories":["GHSA-X","GHSA-X"],"ecosystem":"npm"}"#).unwrap();
        let key = format!("{}|GHSA-X", actions::base(&row));
        let report = Report { rows: vec![row], ..Report::default() };
        assert!(selected("/project", &report, &key).is_err());
        assert!(selected("/another", &report, &key).is_err());
    }
}
