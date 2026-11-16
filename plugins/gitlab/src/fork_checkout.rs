//! Reviewed, version-qualified official CLI checkout. Terminal owns execution;
//! launching does not assert checkout success or pin a moving remote head.
use crate::*;

fn identity(mr: &MergeRequest) -> Result<(u64, u64, String, String), Error> {
    if mr.iid == 0 || mr.source_project_id == 0 || mr.target_project_id == 0 {
        return Err("The merge request source identity is incomplete.".into());
    }
    safe::ref_name(&mr.source_branch)?;
    let sha = mr.sha.as_deref().ok_or("Refresh the merge request head before checkout.")?;
    if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("A complete40-character merge request head is required.".into());
    }
    Ok((mr.source_project_id, mr.target_project_id, mr.source_branch.clone(), sha.to_owned()))
}
fn clean(ctx: &Invocation, root: &str) -> Result<(), Error> {
    let project = ctx.project().ok_or(Error::Stale)?;
    let status = project.vcs_status()?.ok_or(Error::Stale)?;
    if status.root != root || status.operation.is_some() || status.changed_files != 0 {
        return Err("Finish any Git operation and save/commit/stash work before CLI checkout.".into());
    }
    if host::open_editors()?.iter().any(|e| e.dirty && e.path.starts_with(&format!("{root}/"))) {
        return Err("Save project editor buffers before CLI checkout.".into());
    }
    Ok(())
}
impl GitLab {
    pub(crate) fn checkout_source_cli(&mut self, ctx: &Invocation) -> Result<Option<String>, Error> {
        if !host::secret_is_set(api::token()) { return Err(NO_TOKEN.into()); }
        let repo = gitlab_project(ctx)?;
        let iid = self.checked_selection(&repo, ctx)?;
        let before = self.checked_merge_request(&repo, ctx, iid)?;
        let proof = identity(&before)?;
        clean(ctx, &repo.status.root)?;
        let branch = format!("review/mr-{iid}-{}", &proof.3[..12]);
        let repository = format!("{}/{}", crate::api::server(), repo.path);
        if !host::confirm("Check out merge request source with glab", &format!(
            "Working repository: {}\nTarget: {repository}\nMerge request: !{iid}\nSource project ID: {}{}\nSource branch: {}\nReviewed head: {}\nLocal branch: {branch}\n\nRun official glab1.130.0 mr checkout with its independently configured credentials and Git transports. It fetches the MR source (including forks), may add/update remote tracking and local branch configuration, and switches the current working tree. No --force is used. Existing branch conflicts are delegated to glab/Git. This is a source-branch checkout, not a head-pinned snapshot: a server change after review can be fetched. Inspect terminal output and the resulting commit before review. Stop the native terminal to cancel; cancellation may leave fetched refs/configuration. CLI host overrides/hooks/configuration remain effective.",
            repo.status.root, proof.0, if proof.0 != proof.1 { " (fork)" } else { "" }, proof.2, proof.3))? { return Ok(None); }
        let project = ctx.project().ok_or(Error::Stale)?;
        let probe = project.run_process("glab", &["version".into()], None)?;
        let version = probe.stdout.lines().next().and_then(|s|s.strip_prefix("glab ")).and_then(|s|s.split_whitespace().next());
        if probe.status != Some(0) || probe.stdout.len() > 4096 || version != Some("1.130.0") {
            return Err("This checkout profile is qualified for official glab1.130.0. Approve/install that version and authenticate it separately.".into());
        }
        if identity(&self.checked_merge_request(&repo, ctx, iid)?)? != proof { return Err(Error::Stale); }
        clean(ctx, &repo.status.root)?;
        let current = gitlab_project(ctx)?;
        if selection_scope(&current) != selection_scope(&repo) || current.status.commit != repo.status.commit || current.status.branch != repo.status.branch || current.status.changed_files != 0 || current.status.operation.is_some() || self.checked_selection(&current, ctx)? != iid { return Err(Error::Stale); }
        clean(ctx, &repo.status.root)?;
        let args=["mr".into(),"checkout".into(),iid.to_string(),"--repo".into(),repository,"--branch".into(),branch];
        host::open_terminal(&format!("GitLab !{iid} source checkout"), "glab", &args)?;
        Ok(Some("Source checkout launched in the native terminal; verify its exit status and resulting commit.".into()))
    }
}
