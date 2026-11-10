//! Explicit, bounded timeline capture. Rendering never performs HTTP requests.
use crate::{
    DETAILS, Error, GitLab, Invocation, NOTE_KEY, Section, THREAD_KEY, api, gitlab_project,
    refresh_mr_view, selection_scope,
};
use openidea_plugin::{
    host::{self, storage},
    views::{Body, Button, View, ViewEvent},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const MAX_ROWS: usize = 500;
const MAX_BYTES: usize = 384 * 1024;

#[derive(Default)]
pub(crate) struct State {
    capture: Option<Capture>,
    selected: Option<String>,
    failed: bool,
}
struct Capture {
    scope: String,
    iid: u64,
    head: String,
    rows: Vec<Row>,
    partial: bool,
}
struct Row {
    key: String,
    cells: Vec<String>,
    body: String,
    reply: Option<(String, u64)>,
    note: Option<u64>,
}
impl Row {
    fn value(&self, head: &str) -> Value {
        json!({"key":self.key,"cells":self.cells,"label":self.cells[4],"details":[{"label":"Body (bounded plain text)","value":self.body},{"label":"Head observed at capture","value":head},{"label":"Provenance","value":"Bounded GitLab API snapshot; activity may be incomplete or historical."},{"label":"Open target","value":if self.note.is_some(){"GitLab comment"}else{"GitLab merge request activity"}}]})
    }
}
fn bounded(value: &str, bytes: usize) -> String {
    let mut out = String::new();
    for c in value.chars() {
        let c = if c.is_control() && !matches!(c, '\n' | '\t') {
            '\u{fffd}'
        } else {
            c
        };
        if out.len() + c.len_utf8() > bytes {
            out.push_str("…");
            break;
        }
        out.push(c);
    }
    out
}
fn valid_head(head: &str) -> bool {
    matches!(head.len(), 40 | 64) && head.bytes().all(|b| b.is_ascii_hexdigit())
}
fn valid_discussion(id: &str) -> bool {
    crate::safe::discussion_id(id).is_ok()
}
impl Capture {
    fn build(
        scope: String,
        iid: u64,
        head: String,
        threads: &[api::Discussion],
        events: &[api::TimelineEvent],
    ) -> Self {
        let mut rows = Vec::new();
        let mut partial = false;
        let mut bytes = 0;
        if threads.len() > 5000
            || threads
                .iter()
                .map(|d| d.notes.len())
                .try_fold(0usize, |a, b| a.checked_add(b))
                .is_none_or(|n| n > 5000)
        {
            return Self {
                scope,
                iid,
                head,
                rows,
                partial: true,
            };
        }
        // Duplicate discussion/note identities never become actionable rows.
        let mut discussions = BTreeSet::new();
        let mut duplicate_discussions = BTreeSet::new();
        let mut notes = BTreeSet::new();
        let mut duplicate_notes = BTreeSet::new();
        for d in threads {
            if !discussions.insert(&d.id) {
                duplicate_discussions.insert(&d.id);
            }
            for n in &d.notes {
                if !notes.insert(n.id) {
                    duplicate_notes.insert(n.id);
                }
            }
        }
        'threads: for d in threads {
            if !valid_discussion(&d.id) || duplicate_discussions.contains(&d.id) {
                partial = true;
                continue;
            }
            for n in &d.notes {
                if rows.len() >= MAX_ROWS {
                    partial = true;
                    break 'threads;
                }
                if n.id == 0 || duplicate_notes.contains(&n.id) {
                    partial = true;
                    continue;
                }
                let body = bounded(&n.body, 4096);
                let cells = vec![
                    if n.system { "System" } else { "Comment" }.into(),
                    bounded(&n.author.username, 128),
                    bounded(&n.created_at, 64),
                    if n.resolvable {
                        if n.resolved { "Resolved" } else { "Unresolved" }
                    } else {
                        ""
                    }
                    .into(),
                    bounded(n.body.lines().next().unwrap_or(""), 160),
                ];
                rows.push(Row {
                    key: String::new(),
                    cells,
                    body,
                    reply: (!n.system).then(|| (d.id.clone(), n.id)),
                    note: Some(n.id),
                });
            }
        }
        for e in events.iter().take(MAX_ROWS + 1) {
            if rows.len() >= MAX_ROWS {
                partial = true;
                break;
            }
            rows.push(Row {
                key: String::new(),
                cells: vec![
                    "Activity".into(),
                    bounded(&e.user, 128),
                    bounded(&e.created_at, 64),
                    String::new(),
                    bounded(&e.text, 160),
                ],
                body: bounded(&e.text, 4096),
                reply: None,
                note: None,
            });
        }
        rows.sort_by(|a, b| a.cells[2].cmp(&b.cells[2]));
        // A nonce-sized content identity includes the full captured scope and bounded row facts.
        // Hash collisions are refused below; arbitrary caller keys are never parsed as authority.
        let mut keys = BTreeSet::new();
        let mut collisions = BTreeSet::new();
        rows.retain_mut(|row| {
            let identity = serde_json::to_string(&(
                &scope, iid, &head, &row.cells, &row.body, &row.reply, row.note,
            ))
            .unwrap_or_default();
            let hash = identity.bytes().fold(0xcbf29ce484222325_u64, |h, b| {
                (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
            });
            row.key = format!("timeline:{hash:016x}");
            bytes += row.value(&head).to_string().len() + 1;
            if !keys.insert(row.key.clone()) {
                collisions.insert(row.key.clone());
                partial = true;
                return false;
            }
            if keys.len() > MAX_ROWS || bytes > MAX_BYTES - 1024 {
                partial = true;
                return false;
            }
            true
        });
        rows.retain(|r| !collisions.contains(&r.key));
        Self {
            scope,
            iid,
            head,
            rows,
            partial,
        }
    }
}
impl GitLab {
    pub(crate) fn render_timeline(&self, ctx: &Invocation) -> Result<View, Error> {
        let repo = gitlab_project(ctx)?;
        let iid = self.checked_selection(&repo, ctx)?;
        let Some(capture) = self
            .timeline
            .capture
            .as_ref()
            .filter(|c| c.iid == iid && c.scope == selection_scope(&repo))
        else {
            return Ok(View::message(
                "No timeline snapshot. Refresh Timeline explicitly to load comments and activity.",
            )
            .button(Button::new("refresh", "Refresh Timeline").icon("refresh")));
        };
        let items: Vec<Value> = capture
            .rows
            .iter()
            .map(|r| r.value(&capture.head))
            .collect();
        let status = format!(
            "{}{}Timeline snapshot · !{} · {} rows · head {}",
            if self.timeline.failed {
                "STALE — refresh failed; last-good "
            } else {
                ""
            },
            if capture.partial {
                "Partial "
            } else {
                "Bounded "
            },
            iid,
            items.len(),
            capture.head
        );
        let mut view = View::message(&status);
        view.body = Body::Other{kind:"resources".into(),data:json!({"columns":["Kind","Author","When","Status","Summary"],"facets":[0,1,3],"items":items}).to_string()};
        view.status = Some(status);
        view.toolbar
            .push(Button::new("refresh", "Refresh Timeline").icon("refresh"));
        view.toolbar.push(
            Button::new("timeline-reply", "Reply in Overview").icon("review-comment").enabled(
                self.timeline
                    .selected
                    .as_ref()
                    .and_then(|key| capture.rows.iter().find(|r| &r.key == key))
                    .is_some_and(|r| r.reply.is_some()),
            ),
        );
        Ok(view)
    }
    fn capture_timeline(&mut self, ctx: &Invocation) -> Result<(), Error> {
        let repo = gitlab_project(ctx)?;
        let iid = self.checked_selection(&repo, ctx)?;
        let scope = selection_scope(&repo);
        if repo.path.len() > 1000
            || repo.path.split('/').any(|p| {
                p.is_empty()
                    || matches!(p, "." | "..")
                    || !p
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
            })
        {
            return Err("Unsupported GitLab project path for Timeline.".into());
        }
        let task = host::begin_task("Loading bounded GitLab timeline", true);
        let mr = self.checked_merge_request(&repo, ctx, iid)?;
        let head = mr
            .sha
            .as_deref()
            .filter(|s| valid_head(s))
            .ok_or("A complete head commit is required for the timeline snapshot.")?;
        let api = api::Project(&repo.path);
        api::require_read_budget()?;
        let threads = api.discussions(iid)?;
        if task.is_cancelled() {
            return Err(Error::Cancelled);
        }
        api::require_read_budget()?;
        let events = if crate::preference(crate::EVENTS_KEY, true) {
            api.events(iid)?
        } else {
            Vec::new()
        };
        if task.is_cancelled() {
            return Err(Error::Cancelled);
        }
        api::require_read_budget()?;
        let after = self.checked_merge_request(&repo, ctx, iid)?;
        if after.sha.as_deref() != Some(head) || selection_scope(&gitlab_project(ctx)?) != scope {
            return Err("Timeline owner or merge request head changed; retry explicitly.".into());
        }
        let capture = Capture::build(scope, iid, head.into(), &threads, &events);
        self.threads.insert(iid, threads);
        self.events.insert(iid, events);
        self.timeline = State {
            capture: Some(capture),
            selected: None,
            failed: false,
        };
        Ok(())
    }
    pub(crate) fn handle_timeline(
        &mut self,
        event: ViewEvent,
        ctx: &Invocation,
    ) -> Result<(), Error> {
        if matches!(&event,ViewEvent::Button(id) if matches!(id.as_str(),"timeline-refresh"|"refresh"))
        {
            let result = self.capture_timeline(ctx);
            if result.is_err() {
                self.timeline.failed = true;
            }
            refresh_mr_view(DETAILS);
            return result;
        }
        let repo = gitlab_project(ctx)?;
        let iid = self.checked_selection(&repo, ctx)?;
        let capture = self
            .timeline
            .capture
            .as_ref()
            .filter(|c| c.iid == iid && c.scope == selection_scope(&repo))
            .ok_or("Refresh Timeline in this project first.")?;
        let key = match &event {
            ViewEvent::Select(key) | ViewEvent::Activate(key) => key.as_str(),
            ViewEvent::Button(id) if id == "timeline-reply" => self
                .timeline
                .selected
                .as_deref()
                .ok_or("Select a timeline comment first.")?,
            _ => return Ok(()),
        };
        let row = capture
            .rows
            .iter()
            .find(|r| r.key == key)
            .ok_or("The timeline item is stale or unknown; refresh it.")?;
        if matches!(event, ViewEvent::Select(_)) {
            self.timeline.selected = Some(key.into());
            refresh_mr_view(DETAILS);
            return Ok(());
        }
        let current = self.checked_merge_request(&repo, ctx, iid)?;
        if current.sha.as_deref() != Some(capture.head.as_str()) {
            return Err("The merge request head changed; refresh Timeline first.".into());
        }
        if matches!(event, ViewEvent::Activate(_)) {
            // Construct from the validated repository and numeric MR/note identities, never API text URLs.
            let url = format!(
                "https://gitlab.com/{}/-/merge_requests/{iid}{}",
                repo.path,
                row.note.map(|id| format!("#note_{id}")).unwrap_or_default()
            );
            return host::open_url(&url);
        }
        let (thread, note) = row
            .reply
            .as_ref()
            .ok_or("Select a non-system comment to reply.")?;
        let cached = self
            .threads
            .get(&iid)
            .into_iter()
            .flatten()
            .filter(|d| &d.id == thread)
            .collect::<Vec<_>>();
        if cached.len() != 1
            || cached[0]
                .notes
                .iter()
                .filter(|n| n.id == *note && !n.system && bounded(&n.body, 4096) == row.body)
                .count()
                != 1
        {
            return Err("The cached discussion changed; refresh Timeline first.".into());
        }
        storage::set(THREAD_KEY, thread)?;
        storage::set(NOTE_KEY, &note.to_string())?;
        self.set_section(Section::Overview)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_controls_duplicates_and_empty_discussions_are_conservative() {
        let note = api::Note {
            id: 1,
            body: format!("{}\0", "😀".repeat(5000)),
            ..Default::default()
        };
        let d = api::Discussion {
            id: "a1".into(),
            notes: vec![note.clone()],
        };
        let capture = Capture::build(
            "scope".into(),
            7,
            "1".repeat(40),
            &[d.clone(), api::Discussion::default()],
            &[],
        );
        assert_eq!(capture.rows.len(), 1);
        assert!(capture.partial);
        assert!(capture.rows[0].body.len() <= 4099);
        assert!(!capture.rows[0].body.contains('\0'));
        let duplicate = Capture::build("scope".into(), 7, "1".repeat(40), &[d.clone(), d], &[]);
        assert!(duplicate.rows.is_empty());
        assert!(duplicate.partial);
        let events = vec![
            api::TimelineEvent {
                text: "event".into(),
                ..Default::default()
            };
            600
        ];
        let sample = Capture::build("scope".into(), 7, "1".repeat(40), &[], &events);
        assert!(sample.rows.len() <= MAX_ROWS);
        assert!(sample.partial);
        let large = api::Discussion {
            id: "a2".into(),
            notes: (1..1000)
                .map(|id| api::Note {
                    id,
                    body: "x".repeat(4096),
                    ..Default::default()
                })
                .collect(),
        };
        let capture = Capture::build("scope".into(), 7, "1".repeat(40), &[large], &[]);
        assert!(capture.partial);
        assert!(capture.rows.len() < 100);
        let items: Vec<Value> = capture
            .rows
            .iter()
            .map(|r| r.value(&capture.head))
            .collect();
        assert!(json!({"columns":["Kind","Author","When","Status","Summary"],"facets":[0,1,3],"items":items}).to_string().len()<=MAX_BYTES);
    }
}
