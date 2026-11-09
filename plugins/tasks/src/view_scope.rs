//! Opaque rendered callback identities. Never authorization beyond project/storage checks.
use openidea_plugin::views::{Body, View, ViewEvent};
use openidea_plugin::{Error, Invocation};
use std::hash::{BuildHasher, RandomState};
#[derive(Default)]
pub struct Scope {
    owner: Option<String>,
    raw: Option<String>,
    nonce: RandomState,
    generation: u64,
    action: u64,
}
impl Scope {
    pub fn bind(&mut self, owner: &str, raw: Option<String>) -> bool {
        if self.owner.as_deref() != Some(owner) || self.raw != raw {
            self.owner = Some(owner.into());
            self.raw = raw;
            self.generation = self.generation.wrapping_add(1);
            self.action = self.action.wrapping_add(1);
            return true;
        }
        false
    }
    pub fn owner_is(&self, owner: &str) -> bool {
        self.owner.as_deref() == Some(owner)
    }
    pub fn clear(&mut self) {
        self.owner = None;
        self.raw = None;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn rotate(&mut self) {
        self.action = self.action.wrapping_add(1);
    }
    fn prefix(&self, action: bool) -> String {
        format!("@tasks:{:016x}:{}:{}:", self.nonce.hash_one("Tasks rendered scope"), self.generation, if action { self.action } else { 0 })
    }
    pub fn key(&self, key: &str, action: bool) -> String {
        format!("{}{key}", self.prefix(action))
    }
    pub fn view(&self, mut view: View) -> View {
        for b in &mut view.toolbar {
            b.id = self.key(&b.id, true);
        }
        match &mut view.body {
            Body::Tree(items) => {
                for i in items {
                    i.key = self.key(&i.key, false);
                    if let Some(parent) = &mut i.parent {
                        *parent = self.key(parent, false);
                    }
                }
            }
            Body::Form(fields) => {
                for f in fields {
                    f.key = self.key(&f.key, true);
                }
            }
            _ => {}
        }
        view
    }
    pub fn decode(&self, event: ViewEvent, ctx: &Invocation) -> Result<ViewEvent, Error> {
        let owner = self.owner.as_deref().ok_or(Error::Stale)?;
        if ctx.project().and_then(|p| p.root().ok()).as_deref() != Some(owner) {
            return Err(Error::Stale);
        }
        crate::state::unchanged(owner, &self.raw)?;
        let key = |s: String, action: bool| s.strip_prefix(&self.prefix(action)).map(str::to_owned).ok_or(Error::Stale);
        Ok(match event {
            ViewEvent::Button(s) => ViewEvent::Button(key(s, true)?),
            ViewEvent::Field(s, v) => ViewEvent::Field(key(s, true)?, v),
            ViewEvent::Select(s) => ViewEvent::Select(key(s, false)?),
            ViewEvent::Activate(s) => ViewEvent::Activate(key(s, false)?),
            ViewEvent::Expand(s) => ViewEvent::Expand(key(s, false)?),
            ViewEvent::Collapse(s) => ViewEvent::Collapse(key(s, false)?),
            other => other,
        })
    }
    pub fn after(&mut self, ctx: &Invocation) {
        let Some(owner) = ctx.project().and_then(|p| p.root().ok()) else {
            self.clear();
            return;
        };
        let Some(written) = crate::state::receipt(&owner) else {
            return;
        };
        match crate::state::raw(&owner) {
            Ok(Some(current)) if current == written => {
                self.bind(&owner, Some(written));
            }
            _ => self.clear(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tasks;
    use openidea_plugin::mock::MockHost;
    use openidea_plugin::views::Views;
    #[test]
    fn old_project_callbacks_are_rejected_before_and_after_another_project_renders() {
        let h = MockHost::install().with_capabilities(&["project.read", "storage"]);
        h.add_project("/other");
        let mut p = Tasks::default();
        let view = p.render("tasks", None, &h.invocation()).unwrap();
        let old = view.toolbar.iter().find(|b| b.label == "New Task").unwrap().id.clone();
        h.set_project_root(Some("/other"));
        assert!(p.handle("tasks", ViewEvent::Button(old.clone()), &h.invocation()).is_err());
        p.render("tasks", None, &h.invocation()).unwrap();
        assert!(p.handle("tasks", ViewEvent::Button(old), &h.invocation()).is_err());
        assert!(p.form.is_none());
        assert_eq!(crate::load("/other").unwrap().tasks.len(), 1);
    }
    #[test]
    fn old_form_submit_after_model_change_or_new_form_is_rejected() {
        let h = MockHost::install().with_capabilities(&["project.read", "storage"]);
        let mut p = Tasks::default();
        p.render("tasks", None, &h.invocation()).unwrap();
        let new = p.scope.key("new", true);
        p.handle("tasks", ViewEvent::Button(new), &h.invocation()).unwrap();
        let old = p.scope.key("ok", true);
        p.handle("tasks", ViewEvent::Field(p.scope.key("summary", true), "Draft".into()), &h.invocation()).unwrap();
        crate::update("/project", |t, _| {
            t.add_local("External", "change");
            Ok(())
        })
        .unwrap();
        assert!(p.handle("tasks", ViewEvent::Button(old.clone()), &h.invocation()).is_err());
        p.render("tasks", None, &h.invocation()).unwrap();
        assert!(p.handle("tasks", ViewEvent::Button(old), &h.invocation()).is_err());
        assert_eq!(crate::load("/project").unwrap().tasks.len(), 2);
    }
    #[test]
    fn anonymous_and_denied_contexts_cannot_reuse_a_form_or_status() {
        let h = MockHost::install().with_capabilities(&["project.read", "storage"]);
        let mut p = Tasks::default();
        p.render("tasks", None, &h.invocation()).unwrap();
        let old = p.scope.key("new", true);
        h.set_project_root(None);
        p.render("tasks", None, &h.invocation()).unwrap();
        assert!(p.handle("tasks", ViewEvent::Button(old), &h.invocation()).is_err());
        assert!(p.form.is_none());
        h.set_project_root(Some("/project"));
        let _h = h.with_capabilities(&["project.read"]);
        assert!(p.render("tasks", None, &_h.invocation()).is_err());
    }
    #[test]
    fn current_form_state_cannot_submit_a_now_empty_summary() {
        let h = MockHost::install().with_capabilities(&["project.read", "storage"]);
        let mut p = Tasks::default();
        p.render("tasks", None, &h.invocation()).unwrap();
        p.handle("tasks", ViewEvent::Button(p.scope.key("new", true)), &h.invocation()).unwrap();
        let old_ok = p.scope.key("ok", true);
        p.handle("tasks", ViewEvent::Field(p.scope.key("summary", true), "Title".into()), &h.invocation()).unwrap();
        p.handle("tasks", ViewEvent::Field(p.scope.key("summary", true), String::new()), &h.invocation()).unwrap();
        assert!(p.handle("tasks", ViewEvent::Button(old_ok), &h.invocation()).is_err());
        assert_eq!(crate::load("/project").unwrap().tasks.len(), 1);
    }
    #[test]
    fn file_events_never_capture_prefix_siblings_or_nested_project_contexts() {
        let h = MockHost::install().with_capabilities(&["project.read", "storage"]);
        h.add_project("/other");
        h.add_project("/project/nested");
        let mut p = Tasks::default();
        openidea_plugin::Plugin::handle_events(
            &mut p,
            &[openidea_plugin::Event::FileOpened("/project-sibling/secret.rs".into())],
            &h.invocation(),
        );
        assert!(crate::state::raw("/project").unwrap().is_none());
        let events = ["/project/a.rs", "/other/secret.rs", "/project/nested/a.rs"].map(|p| openidea_plugin::Event::FileOpened(p.into()));
        openidea_plugin::Plugin::handle_events(&mut p, &events, &h.invocation());
        assert_eq!(crate::load("/project").unwrap().active().files, ["/project/a.rs"]);
        p.render("tasks", None, &h.invocation()).unwrap();
        assert_eq!(h.status_items()[crate::STATUS_ITEM].text, "Default task");
    }
    #[test]
    fn recorded_context_reopen_refuses_stale_or_newly_foreign_paths_before_opening() {
        let h = MockHost::install().with_capabilities(&["project.read", "storage"]);
        crate::update("/project", |t, _| {
            t.contexts.push(crate::model::NamedContext { name: "Focus".into(), files: vec!["/project/nested/a.rs".into()], selection: None });
            Ok(())
        })
        .unwrap();
        let before = crate::load("/project").unwrap();
        h.add_project("/project/nested");
        h.answer_next_prompt(Some(&["Focus"]));
        assert!(crate::load_context(&h.invocation().project().unwrap(), &before).is_err());
        assert!(h.opened_locations().is_empty());
        crate::update("/project", |t, _| {
            t.contexts[0].files = vec!["/project/current.rs".into()];
            Ok(())
        })
        .unwrap();
        h.answer_next_prompt(Some(&["Focus"]));
        assert!(matches!(crate::load_context(&h.invocation().project().unwrap(), &before), Err(Error::Stale)));
        assert!(h.opened_locations().is_empty());
    }
    #[test]
    fn nonwriting_callback_cannot_adopt_an_unrelated_concurrent_storage_head() {
        let h = MockHost::install().with_capabilities(&["project.read", "storage"]);
        crate::update("/project", |_, _| Ok(())).unwrap();
        let (_, original) = crate::state::snapshot("/project").unwrap();
        let mut scope = Scope::default();
        scope.bind("/project", original);
        crate::state::clear_receipt();
        let mut concurrent = crate::load("/project").unwrap();
        concurrent.add_local("Concurrent", "external writer");
        openidea_plugin::host::storage::set(&crate::state::key("/project"), &serde_json::to_string(&concurrent).unwrap()).unwrap();
        scope.after(&h.invocation());
        assert!(scope.decode(ViewEvent::Field(scope.key("summary", true), "Old draft".into()), &h.invocation()).is_err());
    }
    #[test]
    fn exact_local_write_receipt_advances_scope_without_rebinding_to_later_external_write() {
        let h = MockHost::install().with_capabilities(&["project.read", "storage"]);
        let (_, original) = crate::state::snapshot("/project").unwrap();
        let mut scope = Scope::default();
        scope.bind("/project", original);
        crate::state::clear_receipt();
        crate::update("/project", |tasks, _| {
            tasks.add_local("Local", "this invocation");
            Ok(())
        })
        .unwrap();
        scope.after(&h.invocation());
        assert!(scope.decode(ViewEvent::Button(scope.key("new", true)), &h.invocation()).is_ok());
        openidea_plugin::host::storage::set(
            &crate::state::key("/project"),
            &serde_json::to_string(&crate::model::TaskList::new(1)).unwrap(),
        )
        .unwrap();
        scope.after(&h.invocation());
        assert!(scope.decode(ViewEvent::Button(scope.key("new", true)), &h.invocation()).is_err());
    }
}
