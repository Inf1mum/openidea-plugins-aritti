//! Concrete command review. Source hints never establish the effective database.
use crate::*;
use sha2::{Digest, Sha256};
const LIMIT: usize = 128 << 10;
struct Review {
    project: Project,
    root: String,
    defaults: Option<String>,
    args: Vec<String>,
}
fn defaults(project: &Project) -> Result<Option<String>, Error> {
    match project.read_file("liquibase.properties") {
        Ok(text) if text.len() <= LIMIT => Ok(Some(text)),
        Ok(_) => Err("Root liquibase.properties exceeds the 128 KiB review limit; command not started.".into()),
        Err(Error::NotFound(_)) => Ok(None),
        Err(error) => Err(error),
    }
}
fn digest(text: &str) -> String { format!("{:x}", Sha256::digest(text.as_bytes())) }
fn safe_jdbc(value: &str) -> Option<String> {
    let base = value.split(['?', ';']).next()?;
    let (prefix, address) = ["jdbc:postgresql://", "jdbc:mysql://", "jdbc:mariadb://"].iter().find_map(|prefix| base.strip_prefix(prefix).map(|rest| (*prefix, rest)))?;
    let (authority, database) = address.split_once('/')?;
    if authority.is_empty() || database.is_empty() || base.len() > 1024
        || !authority.bytes().all(|b| b.is_ascii_alphanumeric() || b".-:[]".contains(&b))
        || !database.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b)) {
        return None;
    }
    Some(format!("{prefix}{authority}/{database} (parameters omitted)"))
}
fn hint(text: &str, names: &[&str], connection: bool) -> String {
    let keys = spring_text::config::keys("liquibase.properties", text);
    let hits: Vec<_> = keys.iter().filter(|key| names.contains(&key.key.as_str())).take(2).collect();
    let [key] = hits.as_slice() else { return "UNRESOLVED (missing or ambiguous source declaration)".into(); };
    if connection { return safe_jdbc(&key.value).unwrap_or_else(|| "UNRESOLVED (connection source is dynamic, credential-bearing or unsupported; value withheld)".into()); }
    let value = key.value.trim();
    if value.is_empty() || value.len() > 1024 || value.starts_with('/') || value.split('/').any(|p| matches!(p, "." | ".."))
        || !value.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b)) {
        "UNRESOLVED (changelog source is dynamic or unsupported; value withheld)".into()
    } else { value.into() }
}
impl Review {
    fn capture(ctx: &Invocation, args: Vec<String>) -> Result<Self, Error> {
        let project = ctx.project().ok_or(Error::Stale)?;
        let root = project.root()?;
        if !root.starts_with('/') || root.len() > 4096 || root.chars().any(char::is_control) || root.contains('\\')
            || root.split('/').any(|p| matches!(p, "." | "..")) {
            return Err(Error::Stale);
        }
        if !host::projects()?.iter().any(|p| p.root().ok().as_deref() == Some(root.as_str())) { return Err(Error::Stale); }
        let defaults = defaults(&project)?;
        Ok(Self { project, root, defaults, args })
    }
    fn body(&self, effect: &str) -> String {
        let (target, config) = self.defaults.as_ref().map_or_else(
            || ("UNRESOLVED (no root liquibase.properties; the CLI's own configuration decides)".to_owned(), String::new()),
            |text| (
                hint(text, &["url", "liquibase.command.url"], true),
                format!(
                    "Changelog (from liquibase.properties): {}\nReviewed liquibase.properties SHA256: {}\n\n",
                    hint(text, &["changeLogFile", "liquibase.command.changelogFile", "liquibase.command.changeLogFile"], false),
                    digest(text)
                ),
            ),
        );
        format!(
            "{effect}\n\nTarget database (from liquibase.properties): {target}\nCommand: liquibase {}\nProject (terminal working directory): {}\n\n{config}Unsaved editor text, the environment, JVM properties, defaults and extensions can override these hints. Contexts and labels: UNRESOLVED. The browser's source selection does not configure this command. Database credentials are never read or shown.\n\nRun it?",
            serde_json::to_string(&self.args).expect("args"),
            serde_json::to_string(&self.root).expect("root")
        )
    }
    fn unchanged(&self, ctx: &Invocation) -> Result<(), Error> {
        if ctx.project().and_then(|p| p.root().ok()).as_deref() != Some(self.root.as_str())
            || !host::projects()?.iter().any(|p| p.root().ok().as_deref() == Some(self.root.as_str()))
            || defaults(&self.project)? != self.defaults {
            return Err(Error::Stale);
        }
        Ok(())
    }
}
/// What the user confirmed: the redacted source hints, never the effective database.
pub(crate) struct Reviewed {
    pub(crate) root: String,
    pub(crate) properties: Option<String>,
    /// Redacted JDBC target, `None` when unresolved.
    pub(crate) target: Option<String>,
    /// The changelog path from liquibase.properties, `None` when unresolved.
    pub(crate) changelog: Option<String>,
}
pub(crate) fn properties(project: &Project) -> Result<Option<String>, Error> { defaults(project) }
/// Shows the review and asks; `None` when declined. Nothing runs before this returns `Some`.
pub(crate) fn review(ctx: &Invocation, args: Vec<String>, title: &str, effect: &str) -> Result<Option<Reviewed>, Error> {
    let review = Review::capture(ctx, args)?;
    if !confirmed(title, &review.body(effect))? { return Ok(None); }
    review.unchanged(ctx)?;
    let resolved = |names: &[&str], connection: bool| review.defaults.as_deref().map(|text| hint(text, names, connection)).filter(|value| !value.starts_with("UNRESOLVED"));
    Ok(Some(Reviewed {
        target: resolved(&["url", "liquibase.command.url"], true).map(|value| value.trim_end_matches(" (parameters omitted)").to_owned()),
        changelog: resolved(&["changeLogFile", "liquibase.command.changelogFile", "liquibase.command.changeLogFile"], false),
        root: review.root,
        properties: review.defaults,
    }))
}
pub(crate) fn run(ctx: &Invocation, args: Vec<String>, title: &str, effect: &str) -> Result<(), Error> {
    if review(ctx, args.clone(), title, effect)?.is_some() {
        run_liquibase(args)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    #[test]
    fn review_displays_exact_project_argv_and_redacted_source_hints_not_effective_target_or_credentials() {
        let host = MockHost::install();
        host.add_file("liquibase.properties", "url=jdbc:postgresql://db.example:5432/app?user=private&password=TOKEN_439\nchangeLogFile=db/master.yaml\nusername=private\npassword=TOKEN_439\n");
        let review = Review::capture(&host.invocation(), vec!["rollback-count".into(), "3".into()]).unwrap();
        let body = review.body("Roll back3");
        for expected in ["/project", "[\"rollback-count\",\"3\"]", "db/master.yaml", "jdbc:postgresql://db.example:5432/app", "parameters omitted", "UNRESOLVED", "SHA256", "Unsaved editor text"] { assert!(body.contains(expected), "{body}"); }
        assert!(body.starts_with("Roll back3\n\nTarget database (from liquibase.properties): jdbc:postgresql://db.example:5432/app"), "{body}");
        assert!(body.find("Command: liquibase").unwrap() < body.find("Unsaved editor text").unwrap());
        for secret in ["private", "password=", "TOKEN_439", "?user="] { assert!(!body.contains(secret), "{secret}: {body}"); }
    }
    #[test]
    fn source_drift_foreign_roots_missing_scope_and_oversize_refuse_after_review() {
        let host = MockHost::install();
        let review = Review::capture(&host.invocation(), vec!["update".into()]).unwrap();
        host.add_file("liquibase.properties", "url=new\n");
        assert!(matches!(review.unchanged(&host.invocation()), Err(Error::Stale)));
        let review = Review::capture(&host.invocation(), vec!["update".into()]).unwrap();
        host.set_project_root(Some("/other"));
        assert!(matches!(review.unchanged(&host.invocation()), Err(Error::Stale)));
        host.set_project_root(None);
        assert!(Review::capture(&host.invocation(), vec!["update".into()]).is_err());
        host.set_project_root(Some("/project"));
        host.add_file("liquibase.properties", &" ".repeat(LIMIT + 1));
        assert!(Review::capture(&host.invocation(), vec!["update".into()]).is_err());
        assert!(host.terminals().is_empty());
    }
    #[test]
    fn ambiguous_or_credential_bearing_dynamic_sources_are_withheld() {
        for url in ["jdbc:postgresql://user:password@host/db", "${DATABASE_URL}", "jdbc:h2:file:private;PASSWORD=secret", "jdbc:mysql://host/db%3Fpassword=secret"] { assert!(safe_jdbc(url).is_none()); }
        assert!(hint("url=jdbc:mysql://host/a\nliquibase.command.url=jdbc:mysql://host/b", &["url", "liquibase.command.url"], true).contains("UNRESOLVED"));
        assert!(hint("changeLogFile=../private", &["changeLogFile"], false).contains("UNRESOLVED"));
    }
}
