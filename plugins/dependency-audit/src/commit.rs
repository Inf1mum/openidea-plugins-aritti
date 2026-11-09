//! Review historical scan evidence for changed dependency files. The draft only
//! supplies diffs, not complete included blobs: never certify their current safety.
use crate::{DependencyAudit, kind, now, osv::QUERY_TTL_SECS, stored_report};
use openidea_plugin::commit::{CommitChecks, CommitProblem};
use openidea_plugin::{CommitDraft, Error, Invocation, Location, Position, Range};
use std::collections::BTreeSet;

impl CommitChecks for DependencyAudit {
    fn check(&mut self, check: &str, draft: &CommitDraft, ctx: &Invocation) -> Result<Vec<CommitProblem>, Error> {
        if check != "cached-dependencies" {
            return Err(Error::NotFound(check.into()));
        }
        let project = ctx.project().ok_or("Open a project to review cached dependency evidence.")?;
        let root = project.root()?;
        if root.is_empty() {
            return Err(Error::Failed("Project identity is unavailable.".into()));
        }
        let prefix = format!("{}/", root.trim_end_matches('/'));
        let changed: BTreeSet<_> = draft
            .changes
            .iter()
            .filter(|c| c.status != "deleted" && kind(&c.path).is_some())
            .filter(|c| c.path.starts_with(&prefix) && !c.path.split('/').any(|p| p == ".." || p == "."))
            .map(|c| c.path.as_str())
            .collect();
        if changed.is_empty() {
            return Ok(Vec::new());
        }
        let Some(report) = stored_report(&project)? else {
            return Ok(vec![CommitProblem::new(
                "No cached dependency scan is available. Run Dependency Audit: Scan Project, then review the included dependency changes. This check does not contact OSV.",
            )]);
        };
        let at = report.scanned_at.map(|at| format!("Unix {at}")).unwrap_or_else(|| "date not recorded".into());
        let stale = report.scanned_at.is_none_or(|at| at > now() || now().saturating_sub(at) >= QUERY_TTL_SECS);
        let mut problems = Vec::new();
        if stale
            || report.discovery_limited
            || report.files_skipped > 0
            || changed.iter().any(|path| !report.checked_files.iter().any(|p| p == path))
        {
            problems.push(CommitProblem::new(format!("Cached dependency scan ({at}) is stale, incomplete or does not cover every changed dependency file. Rescan and review the included changes; this check does not certify the selected commit contents.")));
        }
        for row in report.rows.iter().take(5000) {
            let source = row
                .sources
                .iter()
                .find(|s| changed.contains(s.path.as_str()))
                .map(|s| (s.path.as_str(), s.line))
                .or_else(|| changed.contains(row.path.as_str()).then_some((row.path.as_str(), row.line)));
            let Some((path, line)) = source else { continue };
            let position = Position::new(line, 0);
            let message = format!(
                "Previous scan ({at}): {} {} — {}. Historical evidence may differ from the selected commit; review changes or rescan.",
                row.name,
                row.version,
                row.advisories.join(", ")
            );
            problems.push(CommitProblem::new(message).at(Location { path: path.into(), range: Range::new(position, position) }));
            if problems.len() >= 100 {
                break;
            }
        }
        Ok(problems)
    }
}
