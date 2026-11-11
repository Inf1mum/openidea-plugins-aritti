//! Unsent review comments kept in plugin storage, bound to project + MR iid + head commit.
//! A draft whose head no longer matches is flagged stale and never submitted.
use crate::{DETAILS, Error, GitLab, Invocation, api, gitlab_project, refresh_mr_view, review, safe};
use openidea_plugin::{
    PickItem,
    host::{self, storage},
    views::{Body, Button, View, ViewEvent},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub(crate) const MAX_DRAFTS: usize = 50;
pub(crate) const MAX_TEXT: usize = 8 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Draft {
    pub id: u64,
    pub file: String,
    pub line: u32,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Store {
    project: String,
    iid: u64,
    head: String,
    next: u64,
    pub items: Vec<Draft>,
}

fn key(project: &str, iid: u64) -> String {
    format!("local-drafts:{project}:{iid}")
}

fn checked_text(text: &str) -> Result<String, Error> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Write the comment text first.".into());
    }
    if text.len() > MAX_TEXT {
        return Err("A draft comment is limited to 8 KiB.".into());
    }
    Ok(text.to_owned())
}

impl Store {
    pub fn load(project: &str, iid: u64) -> Result<Self, Error> {
        let Some(raw) = storage::get(&key(project, iid))? else {
            return Ok(Self { project: project.into(), iid, ..Default::default() });
        };
        let store: Self = serde_json::from_str(&raw).map_err(|_| Error::from("Stored drafts are unreadable; clear them."))?;
        if store.project != project || store.iid != iid || store.items.len() > MAX_DRAFTS {
            return Err("Stored drafts do not belong to this merge request; clear them.".into());
        }
        Ok(store)
    }

    pub fn save(&self) -> Result<(), Error> {
        if self.items.is_empty() {
            return storage::delete(&key(&self.project, self.iid));
        }
        let raw = serde_json::to_string(self).map_err(|e| Error::Failed(e.to_string()))?;
        storage::set(&key(&self.project, self.iid), &raw)
    }

    pub fn is_stale(&self, head: &str) -> bool {
        !self.items.is_empty() && self.head != head
    }

    pub fn rebind(&mut self, head: &str) {
        self.head = head.into();
    }

    pub fn add(&mut self, head: &str, file: &str, line: u32, text: &str) -> Result<u64, Error> {
        if head.is_empty() {
            return Err("The merge request head is unknown; refresh it.".into());
        }
        if self.is_stale(head) {
            return Err("Existing drafts are stale for this head; rebind or clear them first.".into());
        }
        if self.items.len() >= MAX_DRAFTS {
            return Err("At most 50 drafts per merge request.".into());
        }
        safe::repo_file_path(file)?;
        if line == 0 {
            return Err("Line numbers start at 1.".into());
        }
        let text = checked_text(text)?;
        self.head = head.into();
        self.next += 1;
        self.items.push(Draft { id: self.next, file: file.into(), line, text });
        Ok(self.next)
    }

    pub fn edit(&mut self, id: u64, text: &str) -> Result<(), Error> {
        let text = checked_text(text)?;
        let draft = self.items.iter_mut().find(|d| d.id == id).ok_or("That draft no longer exists.")?;
        draft.text = text;
        Ok(())
    }

    pub fn delete(&mut self, id: u64) {
        self.items.retain(|d| d.id != id);
    }
}

fn clip(text: &str, max: usize) -> String {
    let line = text.lines().next().unwrap_or("");
    match line.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line.into(),
    }
}

/// Position for a draft: the row must still exist in the current diff.
fn position(diffs: &[api::FileDiff], refs: &api::DiffRefs, draft: &Draft) -> Result<serde_json::Value, Error> {
    let file = diffs.iter().find(|f| f.new_path == draft.file);
    let row = file.and_then(|f| review::parse_diff(&f.diff).into_iter().find(|r| r.kind != '@' && r.new == Some(draft.line)));
    match (file, row) {
        (Some(file), Some(row)) => Ok(api::diff_position(refs, &file.old_path, &file.new_path, row.old, row.new)),
        _ => Err(format!("{}:{} is not a changed line in the current diff.", draft.file, draft.line).into()),
    }
}

impl GitLab {
    fn draft_context(&mut self, ctx: &Invocation) -> Result<(String, u64, String), Error> {
        let repo = gitlab_project(ctx)?;
        let iid = self.checked_selection(&repo, ctx)?;
        self.ensure_details(&repo.path, iid)?;
        let head = self
            .details
            .as_ref()
            .and_then(|d| d.mr.diff_refs.as_ref())
            .map(|r| r.head_sha.clone())
            .filter(|h| !h.is_empty())
            .ok_or("The merge request has no head commit.")?;
        Ok((repo.path, iid, head))
    }

    pub(crate) fn render_local_drafts(&mut self, ctx: &Invocation) -> Result<View, Error> {
        let (project, iid, head) = self.draft_context(ctx)?;
        let store = Store::load(&project, iid)?;
        let stale = store.is_stale(&head);
        let items: Vec<_> = store
            .items
            .iter()
            .map(|d| {
                let state = if stale { "STALE" } else { "Ready" };
                json!({"key": format!("draft:{}", d.id), "cells": [state, d.file, d.line.to_string(), clip(&d.text, 160)],
                    "label": clip(&d.text, 60), "details": [{"label": "Comment (plain text)", "value": d.text},
                    {"label": "Bound head", "value": store.head}, {"label": "Current head", "value": head}]})
            })
            .collect();
        let status = match (store.items.len(), stale) {
            (0, _) => format!("!{iid} · no local drafts · stored only in this IDE until submitted"),
            (n, true) => format!("!{iid} · {n} drafts are STALE: the head changed since they were written; rebind or clear"),
            (n, false) => format!("!{iid} · {n} of {MAX_DRAFTS} local drafts · head {}", &head[..head.len().min(12)]),
        };
        let selected = self.draft_selected.filter(|id| store.items.iter().any(|d| d.id == *id));
        let mut view = View::new(Body::Other {
            kind: "resources".into(),
            data: json!({"columns": ["State", "File", "Line", "Comment"], "facets": [0, 1], "items": items}).to_string(),
        })
        .button(Button::new("draft-add", "Add Draft").icon("add").enabled(!stale))
        .button(Button::new("draft-edit", "Edit").icon("edit").enabled(selected.is_some() && !stale))
        .button(Button::new("draft-delete", "Delete").icon("trash").enabled(selected.is_some()))
        .button(Button::new("draft-submit", "Submit All to Review").icon("send").enabled(!store.items.is_empty() && !stale))
        .button(Button::new("draft-rebind", "Rebind to Current Head").icon("refresh").enabled(stale))
        .button(Button::new("draft-clear", "Clear All").icon("trash").enabled(!store.items.is_empty()));
        view = view.status(&status);
        Ok(view)
    }

    pub(crate) fn handle_local_drafts(&mut self, event: ViewEvent, ctx: &Invocation) -> Result<(), Error> {
        let (project, iid, head) = self.draft_context(ctx)?;
        let mut store = Store::load(&project, iid)?;
        match event {
            ViewEvent::Select(k) | ViewEvent::Activate(k) => {
                self.draft_selected = k.strip_prefix("draft:").and_then(|n| n.parse().ok());
            }
            ViewEvent::Button(id) => match id.as_str() {
                "draft-add" => {
                    self.ensure_diffs(&project, iid)?;
                    let files: Vec<PickItem> = self
                        .diffs
                        .iter()
                        .flat_map(|(_, d)| d.iter())
                        .map(|f| PickItem::new(&f.new_path, &f.new_path))
                        .collect();
                    let Some(file) = host::pick("Draft file", &files)? else { return Ok(()) };
                    let Some(line) = host::input("New-side line number", None)? else { return Ok(()) };
                    let line: u32 = line.trim().parse().map_err(|_| Error::from("Enter a line number."))?;
                    let Some(text) = host::input("Draft comment", None)? else { return Ok(()) };
                    self.draft_selected = Some(store.add(&head, &file, line, &text)?);
                    store.save()?;
                }
                "draft-edit" => {
                    let id = self.draft_selected.ok_or("Select a draft first.")?;
                    if store.is_stale(&head) {
                        return Err("Stale drafts cannot be edited; rebind or clear them.".into());
                    }
                    let current = store.items.iter().find(|d| d.id == id).ok_or("That draft no longer exists.")?.text.clone();
                    let Some(text) = host::input("Draft comment", Some(&current))? else { return Ok(()) };
                    store.edit(id, &text)?;
                    store.save()?;
                }
                "draft-delete" => {
                    store.delete(self.draft_selected.take().ok_or("Select a draft first.")?);
                    store.save()?;
                }
                "draft-rebind" => {
                    if host::confirm("Rebind drafts", "Positions may have moved. Bind these drafts to the current head?")? {
                        store.rebind(&head);
                        store.save()?;
                    }
                }
                "draft-clear" => {
                    if host::confirm("Clear drafts", "Delete all local drafts for this merge request?")? {
                        store.items.clear();
                        store.save()?;
                        self.draft_selected = None;
                    }
                }
                "draft-submit" => {
                    if store.is_stale(&head) {
                        return Err("Drafts are stale for the current head; rebind or clear them.".into());
                    }
                    self.ensure_diffs(&project, iid)?;
                    let refs = self.details.as_ref().and_then(|d| d.mr.diff_refs.clone()).ok_or("The merge request has no diff refs.")?;
                    let diffs = self.diffs.as_ref().map(|(_, d)| d.clone()).unwrap_or_default();
                    let positions = store.items.iter().map(|d| position(&diffs, &refs, d)).collect::<Result<Vec<_>, _>>()?;
                    let api = api::Project(&project);
                    let _task = host::begin_task("Sending drafts to your review", false);
                    let mut failure = None;
                    for (draft, position) in store.items.clone().into_iter().zip(positions) {
                        match api.create_draft(iid, &draft.text, Some(position), None) {
                            Ok(()) => store.delete(draft.id),
                            Err(e) => {
                                failure = Some(e);
                                break;
                            }
                        }
                    }
                    store.save()?;
                    self.reload_merge_request(iid);
                    if let Some(e) = failure {
                        return Err(e);
                    }
                    return Ok(());
                }
                _ => return Ok(()),
            },
            _ => return Ok(()),
        }
        refresh_mr_view(DETAILS);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    fn store() -> (MockHost, Store) {
        let host = MockHost::install().with_capabilities(&["storage"]);
        (host, Store::load("g/p", 7).unwrap())
    }

    #[test]
    fn drafts_are_bounded_and_persist_per_merge_request() {
        let host = MockHost::install().with_capabilities(&["storage"]);
        let mut s = Store::load("g/p", 7).unwrap();
        s.add("h1", "src/a.rs", 4, "  fix this ").unwrap();
        s.save().unwrap();
        assert_eq!(Store::load("g/p", 7).unwrap().items[0].text, "fix this");
        assert!(Store::load("g/p", 8).unwrap().items.is_empty());
        assert!(s.add("h1", "src/a.rs", 4, &"x".repeat(MAX_TEXT + 1)).is_err());
        assert!(s.add("h1", "src/a.rs", 0, "x").is_err());
        assert!(s.add("h1", "../a", 1, "x").is_err());
        assert!(s.add("h1", "a", 1, "   ").is_err());
        for _ in 1..MAX_DRAFTS {
            s.add("h1", "src/a.rs", 1, "x").unwrap();
        }
        assert!(s.add("h1", "src/a.rs", 1, "x").unwrap_err().to_string().contains("50"));
        drop(host);
    }

    #[test]
    fn head_change_flags_drafts_stale_until_rebound() {
        let (_host, mut s) = store();
        s.add("h1", "a", 1, "x").unwrap();
        assert!(!s.is_stale("h1"));
        assert!(s.is_stale("h2"));
        assert!(s.add("h2", "a", 2, "y").is_err());
        s.rebind("h2");
        assert!(!s.is_stale("h2"));
        s.add("h2", "a", 2, "y").unwrap();
    }

    #[test]
    fn edit_delete_and_empty_store_removes_storage_key() {
        let (_host, mut s) = store();
        let id = s.add("h1", "a", 1, "x").unwrap();
        s.edit(id, "y").unwrap();
        assert_eq!(s.items[0].text, "y");
        assert!(s.edit(99, "z").is_err());
        s.save().unwrap();
        s.delete(id);
        s.save().unwrap();
        assert!(storage::get(&key("g/p", 7)).unwrap().is_none());
    }

    #[test]
    fn foreign_or_corrupt_storage_is_refused() {
        let _h = MockHost::install().with_capabilities(&["storage"]);
        storage::set(&key("g/p", 7), "{").unwrap();
        assert!(Store::load("g/p", 7).is_err());
        storage::set(&key("g/p", 7), r#"{"project":"x/y","iid":7,"head":"h","next":1,"items":[]}"#).unwrap();
        assert!(Store::load("g/p", 7).is_err());
    }

    #[test]
    fn position_requires_a_changed_line_in_the_current_diff() {
        let diffs = vec![api::FileDiff { old_path: "a".into(), new_path: "a".into(), diff: "@@ -3,2 +3,2 @@\n a\n-b\n+c\n x\n".into(), ..Default::default() }];
        let refs = api::DiffRefs { base_sha: "b".into(), head_sha: "h".into(), start_sha: "s".into() };
        let draft = |line| Draft { id: 1, file: "a".into(), line, text: "t".into() };
        assert_eq!(position(&diffs, &refs, &draft(4)).unwrap()["new_line"], 4);
        assert!(position(&diffs, &refs, &draft(40)).is_err());
    }
}
