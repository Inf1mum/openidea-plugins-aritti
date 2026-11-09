//! Explicit task-to-commit message replacement; never stages or commits.
use super::*;
use std::path::{Component, Path};

struct Prepared {
    root: String,
    active: String,
    message: String,
    draft: openidea_plugin::CommitDraft,
}

fn prepare(project: &Project) -> Result<Prepared, Error> {
    let root = project.root()?;
    let tasks = load(&root)?;
    let message = tasks.commit_message(&commit_message_format())?;
    let draft = host::pending_commit()?.ok_or("No commit draft is available in this window.")?;
    if message.trim().is_empty() || message.len() > 65536 || message.contains('\0') || draft.message.len() > 65536 {
        return Err("The task or existing commit message is empty, invalid or too large to preview.".into());
    }
    // The API does not expose an independent draft repository identity. With no
    // included files there is no safe way to bind this draft to the task project.
    if draft.changes.is_empty()
        || !draft.changes.iter().all(|change| {
            let path = Path::new(&change.path);
            let repository = Path::new(&change.repository);
            path.is_absolute()
                && repository.is_absolute()
                && path.starts_with(&root)
                && path.starts_with(repository)
                && !path.components().chain(repository.components()).any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        })
    {
        return Err("Include changes from this task project only before applying its commit message.".into());
    }
    Ok(Prepared { root, active: tasks.active, message, draft })
}

fn apply(prepared: Prepared, project: &Project) -> Result<(), Error> {
    if project.root()? != prepared.root {
        return Err(Error::Stale);
    }
    let tasks = load(&prepared.root)?;
    if tasks.active != prepared.active || tasks.commit_message(&commit_message_format())? != prepared.message {
        return Err(Error::Stale);
    }
    // Revision guards both existing text and included-change selection. Never retry
    // against a newer revision: it may contain text the user entered during review.
    host::set_commit_message(prepared.draft.revision, &prepared.message)
}

pub(super) fn run(project: &Project) -> Result<(), Error> {
    let prepared = prepare(project)?;
    if !ui_kit::prompts::confirm_exact(
        "Replace commit message with active task?",
        &format!(
            "Project: {}\nTask: {}\nDraft revision: {}\nAmend: {}\nIncluded files: {}\n\nCurrent message:\n{}\n\nReplacement:\n{}\n\nOnly the commit message is replaced. Review it in Commit before committing.",
            prepared.root,
            prepared.active,
            prepared.draft.revision,
            prepared.draft.amend,
            prepared.draft.changes.len(),
            prepared.draft.message,
            prepared.message
        ),
    )? {
        return Ok(());
    }
    apply(prepared, project)?;
    host::notify(NotificationLevel::Info, "Task commit message applied", "Review the message in Commit.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{CommitChange, CommitDraft, mock::MockHost};
    fn setup() -> MockHost {
        let h = MockHost::install().with_capabilities(&["project.read", "storage", "vcs.read", "vcs.write"]);
        update("/project", |tasks, _| {
            let id = tasks.add_local("Fix parser", "");
            tasks.activate(&id, 1)
        })
        .unwrap();
        h.set_commit_draft(Some(CommitDraft {
            revision: 7,
            message: "User draft".into(),
            changes: vec![CommitChange {
                repository: "/project".into(),
                path: "/project/src/a.rs".into(),
                old_path: None,
                status: "modified".into(),
                diff: None,
                truncated: false,
            }],
            ..CommitDraft::default()
        }));
        h
    }
    #[test]
    fn confirmation_applies_message_only() {
        let h = setup();
        h.answer_next_prompt(Some(&["ok"]));
        let p = h.invocation().project().unwrap();
        run(&p).unwrap();
        let d = h.commit_draft().unwrap();
        assert_eq!(d.message, "Fix parser");
        assert_eq!(d.revision, 8);
        assert_eq!(d.changes.len(), 1);
    }
    #[test]
    fn cancellation_preserves_user_text() {
        let h = setup();
        let p = h.invocation().project().unwrap();
        run(&p).unwrap();
        assert_eq!(h.commit_draft().unwrap().message, "User draft");
    }
    #[test]
    fn changed_commit_revision_is_not_retried() {
        let h = setup();
        let p = h.invocation().project().unwrap();
        let v = prepare(&p).unwrap();
        h.edit_commit_message("New user text");
        assert!(matches!(apply(v, &p), Err(Error::Stale)));
        assert_eq!(h.commit_draft().unwrap().message, "New user text");
    }
    #[test]
    fn changed_task_cannot_apply_old_message() {
        let h = setup();
        let p = h.invocation().project().unwrap();
        let v = prepare(&p).unwrap();
        update("/project", |tasks, _| {
            tasks.active_mut().summary = "Different task text".into();
            Ok(())
        })
        .unwrap();
        assert!(matches!(apply(v, &p), Err(Error::Stale)));
        assert_eq!(h.commit_draft().unwrap().message, "User draft");
    }
    #[test]
    fn foreign_or_empty_draft_is_rejected() {
        let h = setup();
        let p = h.invocation().project().unwrap();
        let mut d = h.commit_draft().unwrap();
        d.changes[0].path = "/project-other/a.rs".into();
        h.set_commit_draft(Some(d.clone()));
        assert!(prepare(&p).is_err());
        d.changes.clear();
        h.set_commit_draft(Some(d));
        assert!(prepare(&p).is_err());
        h.set_commit_draft(None);
        assert!(prepare(&p).is_err());
    }
    #[test]
    fn write_denial_preserves_draft() {
        let h = setup();
        let p = h.invocation().project().unwrap();
        let v = prepare(&p).unwrap();
        let _limited = h.with_capabilities(&["project.read", "storage", "vcs.read"]);
        assert!(matches!(apply(v, &p), Err(Error::Denied(_))));
    }
}
