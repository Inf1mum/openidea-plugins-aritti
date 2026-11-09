//! Plugin-owned source cache. Metadata-only reads are separate from lifecycle warming.
use crate::{FileModel, GLOBS, model};
use openidea_plugin::{Error, Project, host};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};
const MAX_FILES: usize = 4096;
const INVENTORY_KEY: &str = "beans.source-inventory.v1";
const MAX_INVENTORY_BYTES: usize = 256 << 10;
// Only paths are persisted. Restored paths are hints, never trusted model facts.
fn inventory_paths(value: &str, root: &str) -> Option<Vec<String>> {
    if value.len() > MAX_INVENTORY_BYTES { return None; }
    let value: serde_json::Value = serde_json::from_str(value).ok()?;
    if value.get("version")?.as_u64()? != 1 { return None; }
    let roots = value.get("roots")?.as_object()?;
    if roots.len() > MAX_ROOTS { return None; }
    let paths = roots.get(root)?.as_array()?;
    if paths.len() > MAX_FILES { return None; }
    let mut result = BTreeSet::new();
    for path in paths {
        let path = path.as_str()?;
        if !owned(path, root) || !source(path) || !result.insert(path.to_owned()) { return None; }
    }
    Some(result.into_iter().collect())
}
const MAX_TEXT: usize = 512 << 10;
const TTL: Duration = Duration::from_secs(120);
const MAX_ROOTS: usize = 8;
const MAX_CACHE_BYTES: usize = 8 << 20;
pub(crate) fn source(path: &str) -> bool {
    [".java", ".kt", ".xml", ".properties", ".yml", ".yaml", ".sql"].iter().any(|suffix| path.ends_with(suffix))
}
pub(crate) fn build(path: &str) -> bool {
    matches!(
        path.rsplit('/').next().unwrap_or_default(),
        "pom.xml" | "build.gradle" | "build.gradle.kts" | "settings.gradle" | "settings.gradle.kts" | "gradle.properties"
    )
}
pub(crate) fn owned(path: &str, root: &str) -> bool {
    path.len() <= 4096
        && !path.chars().any(char::is_control)
        && !path.contains('\\')
        && !path.split('/').any(|part| part == "." || part == "..")
        && path.strip_prefix(root).is_some_and(|tail| tail.starts_with('/'))
}
pub(crate) struct Index<T> {
    files: BTreeMap<String, T>,
    weights: BTreeMap<String, usize>,
    bytes: usize,
    unread: Vec<String>,
    failed: BTreeSet<String>,
    scanned: Option<Instant>,
    reading: bool,
    discovery_incomplete: bool,
    globs: &'static [&'static str],
    parse: fn(&str, &str) -> Option<T>,
}
impl<T> Index<T> {
    pub fn new(globs: &'static [&'static str], parse: fn(&str, &str) -> Option<T>) -> Self {
        Self {
            files: BTreeMap::new(),
            weights: BTreeMap::new(),
            bytes: 0,
            unread: Vec::new(),
            failed: BTreeSet::new(),
            scanned: None,
            reading: false,
            discovery_incomplete: false,
            globs,
            parse,
        }
    }
    pub fn scan(&mut self, project: &Project, budget: Duration) {
        if budget.is_zero() {
            return;
        }
        let started = Instant::now();
        if !self.reading && self.scanned.is_none_or(|at| at.elapsed() > TTL) {
            let mut paths = BTreeSet::new();
            self.discovery_incomplete = false;
            self.failed.clear();
            let Ok(root) = project.root() else {
                self.discovery_incomplete = true;
                return;
            };
            for glob in self.globs {
                if started.elapsed() >= budget {
                    self.discovery_incomplete = true;
                    break;
                }
                match project.find_files(glob, (MAX_FILES + 1) as u32) {
                    Ok(found) => {
                        if found.len() > MAX_FILES {
                            self.discovery_incomplete = true;
                        }
                        for path in found.into_iter().take(MAX_FILES + 1) {
                            if !owned(&path, &root) {
                                self.discovery_incomplete = true;
                                continue;
                            }
                            if host::project_of(&path).and_then(|project| project.root().ok()).as_deref() != Some(root.as_str()) {
                                continue;
                            }
                            paths.insert(path);
                            if paths.len() > MAX_FILES {
                                self.discovery_incomplete = true;
                                break;
                            }
                        }
                    }
                    Err(_) => self.discovery_incomplete = true,
                }
                if paths.len() > MAX_FILES {
                    break;
                }
            }
            if paths.len() > MAX_FILES {
                paths.pop_last();
            }
            if !self.discovery_incomplete {
                self.files.retain(|path, _| paths.contains(path));
                self.weights.retain(|path, _| paths.contains(path));
                self.bytes = self.weights.values().sum();
            }
            self.unread = paths.into_iter().rev().collect();
            self.reading = true;
            self.scanned = Some(Instant::now());
        }
        while started.elapsed() < budget {
            let Some(path) = self.unread.pop() else {
                self.reading = false;
                break;
            };
            match project.read_file(&path) {
                Ok(text) if text.len() <= MAX_TEXT => self.update(&path, &text),
                _ => self.fail(&path),
            }
        }
        if self.unread.is_empty() {
            self.reading = false;
        }
    }
    pub fn update(&mut self, path: &str, text: &str) {
        self.unread.retain(|queued| queued != path);
        if text.len() > MAX_TEXT || path.ends_with(".xml")&&!crate::xml_namespace::namespace_document(text) {
            self.fail(path);
            return;
        }
        self.remove(path);
        if self.files.len() >= MAX_FILES || self.bytes.saturating_add(text.len()) > MAX_CACHE_BYTES {
            self.failed.insert(path.into());
            return;
        }
        if let Some(model) = (self.parse)(path, text) {
            self.files.insert(path.into(), model);
            self.weights.insert(path.into(), text.len());
            self.bytes += text.len();
        }
    }
    pub fn fail(&mut self, path: &str) {
        self.remove(path);
        self.failed.insert(path.into());
    }
    pub fn remove(&mut self, path: &str) {
        if let Some(weight) = self.weights.remove(path) {
            self.bytes = self.bytes.saturating_sub(weight);
        }
        self.failed.remove(path);
        self.files.remove(path);
        self.unread.retain(|queued| queued != path);
    }
    pub fn remove_tree(&mut self, path: &str) {
        let prefix = format!("{path}/");
        let keys: Vec<_> = self.files.keys().filter(|file| file.as_str() == path || file.starts_with(&prefix)).cloned().collect();
        for key in keys {
            self.remove(&key);
        }
        self.failed.retain(|file| file != path && !file.starts_with(&prefix));
        self.unread.retain(|file| file != path && !file.starts_with(&prefix));
    }
    pub fn complete(&self) -> bool {
        self.scanned.is_some_and(|at| at.elapsed() <= TTL) && !self.reading && !self.discovery_incomplete && self.failed.is_empty()
    }
    pub fn state(&self) -> &'static str {
        if self.reading {
            "Bounded source scan pending; Refresh to continue."
        } else if self.scanned.is_none() {
            "Source cache cold/current-document only; Refresh to index project."
        } else if self.scanned.is_some_and(|at| at.elapsed() > TTL) {
            "Source cache stale; Refresh to retry."
        } else if self.discovery_incomplete {
            "Source inventory incomplete/capped; Refresh to retry."
        } else if !self.failed.is_empty() {
            "Source files unavailable/oversized/unsupported XML or source cache budget reached; unaffected cached facts retained; Refresh to retry."
        } else {
            "Source cache ready."
        }
    }
    pub fn scanning(&self) -> bool {
        self.reading
    }
    pub fn reset(&mut self) {
        self.files.clear();
        self.weights.clear();
        self.bytes = 0;
        self.unread.clear();
        self.failed.clear();
        self.scanned = None;
        self.reading = false;
        self.discovery_incomplete = false;
    }
    pub fn files(&self) -> &BTreeMap<String, T> {
        &self.files
    }
}
pub(crate) struct Indexes {
    roots: BTreeMap<String, Index<FileModel>>,
    unidentified: Index<FileModel>,
    restored: BTreeSet<String>,
    inventory: Option<String>,
    inventory_loaded: bool,
}
impl Indexes {
    pub fn new(_globs: &'static [&'static str], _parse: fn(&str, &str) -> Option<FileModel>) -> Self {
        Self { roots: BTreeMap::new(), unidentified: Index::new(GLOBS, model::parse), restored: BTreeSet::new(), inventory: None, inventory_loaded: false }
    }
    pub fn prune(&mut self) {
        let roots: BTreeSet<_> = host::projects().unwrap_or_default().iter().filter_map(|project| project.root().ok()).collect();
        self.roots.retain(|root, _| roots.contains(root));
        self.restored.retain(|root| roots.contains(root));
    }
    fn ensure(&mut self, root: &str) -> &mut Index<FileModel> {
        if !self.roots.contains_key(root) && self.roots.len() >= MAX_ROOTS {
            if let Some(old) = self.roots.keys().next().cloned() {
                self.roots.remove(&old);
            }
        }
        self.roots.entry(root.into()).or_insert_with(|| Index::new(GLOBS, model::parse))
    }
    pub fn get(&mut self, project: &Project, budget: Duration) -> &mut Index<FileModel> {
        self.prune();
        let Ok(root) = project.root().and_then(|root| if root.is_empty() { Err(Error::Stale) } else { Ok(root) }) else {
            self.unidentified.reset();
            return &mut self.unidentified;
        };
        if !host::projects().unwrap_or_default().iter().any(|project| project.root().ok().as_deref() == Some(root.as_str())) {
            self.unidentified.reset();
            return &mut self.unidentified;
        }
        // Providers call with ZERO: no storage access, restoration or warming there.
        if !budget.is_zero() && !self.inventory_loaded {
            self.inventory = host::storage::get(INVENTORY_KEY).ok().flatten()
                .filter(|value| value.len() <= MAX_INVENTORY_BYTES);
            self.inventory_loaded = true;
        }
        if !budget.is_zero() && self.restored.insert(root.clone()) {
            if let Some(paths) = self.inventory.as_deref().and_then(|value| inventory_paths(value, &root)) {
                let index = self.ensure(&root);
                if index.scanned.is_none() {
                    index.unread = paths.into_iter().filter(|path| {
                        host::project_of(path).and_then(|project| project.root().ok()).as_deref() == Some(root.as_str())
                    }).rev().collect();
                    index.reading = true;
                    index.discovery_incomplete = true;
                }
            }
        }
        self.ensure(&root).scan(project, budget);
        if !budget.is_zero() { self.checkpoint(); }
        self.ensure(&root)
    }
    pub fn cached(&mut self, project: &Project) -> Option<&Index<FileModel>> {
        self.prune();
        let root = project.root().ok()?;
        self.roots.get(&root)
    }
    pub fn for_document(&mut self, path: &str, text: &str, budget: Duration) -> Option<&mut Index<FileModel>> {
        let project = host::project_of(path)?;
        let root = project.root().ok()?;
        if root.is_empty() || !owned(path, &root) {
            return None;
        }
        let index = self.get(&project, budget);
        if source(path) {
            index.update(path, text);
        }
        Some(index)
    }
    pub fn for_cached_document(&mut self, path: &str, text: &str) -> Option<&mut Index<FileModel>> {
        self.for_document(path, text, Duration::ZERO)
    }
    pub fn reset_project(&mut self, project: &Project) {
        if let Ok(root) = project.root() {
            self.restored.insert(root.clone());
            self.ensure(&root).reset();
            self.checkpoint();
        }
    }
    pub fn changed(&mut self, project: &Project, path: &str, removed: bool) -> bool {
        let Ok(root) = project.root() else {
            return false;
        };
        if !owned(path, &root) {
            return false;
        }
        let directory = self.roots.get(&root).is_some_and(|index| index.files().keys().any(|file| file.starts_with(&format!("{path}/"))));
        if !source(path) && !directory && !build(path) {
            return false;
        }
        if build(path) {
            self.reset_project(project);
            return true;
        }
        self.restored.insert(root.clone());
        let index = self.ensure(&root);
        if removed {
            index.remove_tree(path);
        } else if source(path) {
            match project.read_file(path) {
                Ok(text) => index.update(path, &text),
                Err(_) => index.fail(path),
            }
        }
        true
    }
    /// Lifecycle/command only; optional storage denial leaves the in-memory cache usable.
    pub fn checkpoint(&self) {
        let mut roots = BTreeMap::new();
        // A first root's checkpoint must not erase other open roots before their warm turn.
        if let Some(value) = self.inventory.as_deref() {
            for project in host::projects().unwrap_or_default().into_iter().take(MAX_ROOTS) {
                if let Ok(root) = project.root() {
                    if !self.restored.contains(&root) {
                        if let Some(paths) = inventory_paths(value, &root) {
                            roots.insert(root, paths.into_iter().collect::<BTreeSet<_>>());
                        }
                    }
                }
            }
        }
        let mut size = 32usize;
        let mut overflow = false;
        for (root, index) in &self.roots {
            if root.len() > 4096 { overflow = true; break; }
            size += serde_json::to_string(root).map_or(MAX_INVENTORY_BYTES + 1, |s| s.len()) + 4;
            let mut paths = BTreeSet::new();
            for path in index.files.keys().chain(index.unread.iter()).chain(index.failed.iter()) {
                if !source(path) || !owned(path, root) || paths.contains(path) { continue; }
                size += serde_json::to_string(path).map_or(MAX_INVENTORY_BYTES + 1, |s| s.len()) + 1;
                if size > MAX_INVENTORY_BYTES || paths.len() >= MAX_FILES { overflow = true; break; }
                paths.insert(path.clone());
            }
            if overflow || size > MAX_INVENTORY_BYTES { overflow = true; break; }
            if !roots.contains_key(root) { roots.insert(root.clone(), paths); }
        }
        // Never retain an older inventory when the new one exceeds the bounded format.
        if overflow || roots.len() > MAX_ROOTS { roots.clear(); }
        let value = serde_json::json!({"version": 1, "roots": roots}).to_string();
        let value = if value.len() <= MAX_INVENTORY_BYTES { value } else { r#"{"version":1,"roots":{}}"#.into() };
        let _ = host::storage::set(INVENTORY_KEY, &value);
    }
    pub fn clear(&mut self) {
        self.roots.clear();
        self.unidentified.reset();
        self.restored.clear();
        self.inventory = None;
        self.inventory_loaded = true;
        self.checkpoint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use openidea_plugin::mock::MockHost;
    use openidea_plugin::views::{Body, ViewEvent, Views};
    fn doc(path: &str, text: &str) -> Document {
        Document { path: path.into(), language: "java".into(), version: 1, text: text.into() }
    }
    fn batch(path: &str, removed: bool) -> Event {
        Event::FilesChanged(openidea_plugin::FilesChanged {
            modified: if removed { vec![] } else { vec![path.into()] },
            removed: if removed { vec![path.into()] } else { vec![] },
            ..Default::default()
        })
    }
    #[test]
    fn providers_and_render_stay_cold_until_explicit_lifecycle_warm() {
        let host = MockHost::install();
        host.add_file("src/Hidden.java", "@org.springframework.stereotype.Component class Hidden {}");
        let mut nav = Navigator::default();
        let source = doc("/project/src/Consumer.java", "@org.springframework.stereotype.Component class Consumer { @org.springframework.beans.factory.annotation.Autowired Hidden hidden; }");
        assert!(
            nav.line_markers(&source)
                .iter()
                .all(|marker| marker.target.as_ref().is_none_or(|target| !target.path.ends_with("Hidden.java")))
        );
        let shown = nav.render(BEANS_VIEW, None, &host.invocation()).unwrap();
        assert!(shown.status.as_deref().unwrap().contains("cold"));
        assert!(!nav.indexes.cached(&host.invocation().project().unwrap()).unwrap().files().contains_key("/project/src/Hidden.java"));
        nav.handle_events(&[Event::FileOpened("/project/src/Hidden.java".into())], &host.invocation());
        assert!(nav.indexes.cached(&host.invocation().project().unwrap()).unwrap().files().contains_key("/project/src/Hidden.java"));
        host.add_file("src/Later.java", "@org.springframework.stereotype.Component class Later {}");
        nav.render(BEANS_VIEW, None, &host.invocation()).unwrap();
        nav.line_markers(&source);
        assert!(!nav.indexes.cached(&host.invocation().project().unwrap()).unwrap().files().contains_key("/project/src/Later.java"));
        nav.run_command("reindex", &host.invocation(), None).unwrap();
        assert!(nav.indexes.cached(&host.invocation().project().unwrap()).unwrap().files().contains_key("/project/src/Later.java"));
    }
    #[test]
    fn unrelated_events_do_not_cold_scan_and_updates_removals_are_owned() {
        let host = MockHost::install();
        host.add_file("src/A.java", "@org.springframework.stereotype.Component class A {}");
        let mut nav = Navigator::default();
        nav.handle_events(&[batch("/project/README.md", false)], &host.invocation());
        assert!(nav.indexes.cached(&host.invocation().project().unwrap()).is_none());
        nav.run_command("reindex", &host.invocation(), None).unwrap();
        host.add_file("src/A.java", "@org.springframework.stereotype.Component(\"renamed\") class A {}");
        nav.handle_events(&[batch("/project/src/A.java", false)], &host.invocation());
        let project = host.invocation().project().unwrap();
        assert_eq!(nav.indexes.cached(&project).unwrap().files()["/project/src/A.java"].beans[0].name, "renamed");
        nav.handle_events(&[batch("/project/src", true)], &host.invocation());
        assert!(nav.indexes.cached(&project).unwrap().files().is_empty());
        assert!(!nav.indexes.changed(&project, "/project/../outside.java", false));
        assert!(!nav.indexes.changed(&project, "/project-other/A.java", false));
    }
    #[test]
    fn unreadable_or_oversized_change_drops_stale_file_retains_other_positives() {
        let host = MockHost::install();
        host.add_file("src/A.java", "@org.springframework.stereotype.Component class A {}");
        host.add_file("src/B.java", "@org.springframework.stereotype.Component class B {}");
        let project = host.invocation().project().unwrap();
        let mut indexes = Indexes::new(GLOBS, model::parse);
        let index = indexes.get(&project, COMMAND_BUDGET);
        assert!(index.complete());
        index.update("/project/src/Gone.java", "@org.springframework.stereotype.Component class Gone {}");
        assert!(indexes.changed(&project, "/project/src/Gone.java", false));
        let index = indexes.cached(&project).unwrap();
        assert!(!index.complete());
        assert!(!index.files().contains_key("/project/src/Gone.java"));
        assert!(index.files().contains_key("/project/src/B.java"));
        host.add_file("src/A.java", &"x".repeat(MAX_TEXT + 1));
        indexes.changed(&project, "/project/src/A.java", false);
        let index = indexes.cached(&project).unwrap();
        assert!(!index.files().contains_key("/project/src/A.java"));
        assert!(index.files().contains_key("/project/src/B.java"));
        assert!(index.state().contains("unavailable"));
    }
    #[test]
    fn caps_stale_and_denied_discovery_never_prove_absence() {
        let host = MockHost::install();
        for n in 0..MAX_FILES + 1 {
            host.add_file(&format!("src/C{n}.java"), "");
        }
        let project = host.invocation().project().unwrap();
        let mut index = Index::new(GLOBS, model::parse);
        index.scan(&project, COMMAND_BUDGET);
        assert!(!index.complete());
        assert!(index.state().contains("capped"));
        index.scanned = Some(Instant::now() - TTL - Duration::from_secs(1));
        assert!(index.state().contains("stale"));
        let host = MockHost::install().with_capabilities(&[]);
        let project = host.invocation().project().unwrap();
        let mut index = Index::new(GLOBS, model::parse);
        index.update("/project/src/Positive.java", "@org.springframework.stereotype.Component class Positive {}");
        index.scan(&project, COMMAND_BUDGET);
        assert!(!index.complete());
        assert!(index.files().contains_key("/project/src/Positive.java"));
    }
    #[test]
    fn nested_project_and_closed_root_lifecycle_do_not_mix_sources_or_reports() {
        let host = MockHost::install();
        host.add_project("/project/inner");
        host.add_file("/project/src/Outer.java", "@org.springframework.stereotype.Component class Outer {}");
        host.add_file("/project/inner/src/Inner.java", "@org.springframework.stereotype.Component class Inner {}");
        let project = host.invocation().project().unwrap();
        let mut nav = Navigator::default();
        nav.indexes.get(&project, COMMAND_BUDGET);
        assert!(!nav.indexes.cached(&project).unwrap().files().contains_key("/project/inner/src/Inner.java"));
        nav.report = Report::Dump("outer-only-secret-report".into());
        nav.report_root = Some("/project".into());
        nav.sections.set(REPORT_SECTION);
        host.set_project_root(Some("/other"));
        let shown = nav.render(BEANS_VIEW, None, &host.invocation()).unwrap();
        assert!(!format!("{:?}", shown.body).contains("outer-only-secret-report"));
        nav.handle_events(&[], &host.invocation());
        assert!(!nav.indexes.roots.contains_key("/project"));
        assert!(nav.report_root.is_none());
        let stale = nav.indexes.get(&project, COMMAND_BUDGET);
        assert!(stale.files().is_empty() && !stale.complete());
        assert!(!nav.indexes.roots.contains_key("/project"));
    }
    #[test]
    fn fabricated_or_foreign_ui_locations_are_not_action_members() {
        let host = MockHost::install();
        host.add_file("src/A.java", "@org.springframework.stereotype.Component class A {}");
        let mut nav = Navigator::default();
        nav.run_command("reindex", &host.invocation(), None).unwrap();
        for key in ["b:0:0:/other/A.java", "b:0:0:/project/unknown.java", "b:no:0:/project/src/A.java"] {
            assert!(nav.handle(BEANS_VIEW, ViewEvent::Activate(key.into()), &host.invocation()).is_err());
        }
        assert!(host.opened_locations().is_empty());
    }
    #[test]
    fn overflow_batches_drop_cache_and_never_report_empty_complete() {
        let host = MockHost::install();
        host.add_file("src/A.java", "@org.springframework.stereotype.Component class A {}");
        let mut nav = Navigator::default();
        nav.run_command("reindex", &host.invocation(), None).unwrap();
        nav.handle_events(&vec![Event::FileSaved("/project/src/A.java".into()); 257], &host.invocation());
        assert!(nav.indexes.cached(&host.invocation().project().unwrap()).is_none());
        let shown = nav.render(BEANS_VIEW, None, &host.invocation()).unwrap();
        assert!(matches!(shown.body,Body::Message(message) if message.contains("cold")));
    }
    #[test]
    fn partial_report_never_renders_a_clean_bill_and_build_changes_invalidate() {
        let host = MockHost::install();
        host.add_file("src/A.java", "@org.springframework.stereotype.Component class A {}");
        let project = host.invocation().project().unwrap();
        let mut nav = Navigator::default();
        nav.run_command("reindex", &host.invocation(), None).unwrap();
        nav.report = Report::Problems(vec![]);
        nav.report_root = Some("/project".into());
        nav.report_incomplete = true;
        nav.sections.set(REPORT_SECTION);
        let shown = nav.render(BEANS_VIEW, None, &host.invocation()).unwrap();
        assert!(matches!(shown.body,Body::Message(message) if message.contains("Partial source check")));
        assert!(nav.indexes.changed(&project, "/project/build.gradle.kts", false));
        assert!(!nav.indexes.cached(&project).unwrap().complete());
    }
    #[test]
    fn retained_source_budget_is_bounded_and_recovery_is_accounted() {
        let _host = MockHost::install();
        let mut index = Index::new(GLOBS, model::parse);
        let source = format!("@org.springframework.stereotype.Component class C {{}} /*{}*/", "x".repeat(MAX_TEXT - 61));
        for n in 0..18 {
            index.update(&format!("/project/src/C{n}.java"), &source);
        }
        assert!(index.bytes <= MAX_CACHE_BYTES);
        assert!(!index.failed.is_empty());
        assert!(!index.complete());
        let before = index.bytes;
        index.remove("/project/src/C0.java");
        assert!(index.bytes < before);
        index.remove_tree("/project/src");
        assert_eq!(index.bytes, 0);
        assert!(index.files().is_empty());
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use crate::COMMAND_BUDGET;
    use openidea_plugin::mock::MockHost;
    fn stored(root: &str, paths: &[&str]) -> String {
        serde_json::json!({"version": 1, "roots": {root: paths}}).to_string()
    }
    #[test]
    fn restart_restores_only_reread_positives_and_requires_fresh_discovery() {
        let host = MockHost::install();
        host.add_file("src/A.java", "@org.springframework.stereotype.Component class A {}");
        let project = host.invocation().project().unwrap();
        let mut first = Indexes::new(GLOBS, model::parse);
        assert!(first.get(&project, COMMAND_BUDGET).complete());
        let saved = host.storage()[INVENTORY_KEY].clone();
        assert!(saved.contains("/project/src/A.java"));
        assert!(!saved.contains("@org.springframework.stereotype.Component"));
        host.add_file("src/A.java", "@org.springframework.stereotype.Component(\"fresh\") class A {}");
        host.add_file("src/New.java", "@org.springframework.stereotype.Component class New {}");
        let mut restarted = Indexes::new(GLOBS, model::parse);
        let index = restarted.get(&project, COMMAND_BUDGET);
        assert_eq!(index.files()["/project/src/A.java"].beans[0].name, "fresh");
        assert!(!index.files().contains_key("/project/src/New.java"));
        assert!(!index.complete());
        assert!(index.state().contains("cold"));
        let index = restarted.get(&project, COMMAND_BUDGET);
        assert!(index.complete());
        assert!(index.files().contains_key("/project/src/New.java"));
    }
    #[test]
    fn zero_budget_requests_never_restore_or_write_storage() {
        let host = MockHost::install();
        host.add_file("src/A.java", "@org.springframework.stereotype.Component class A {}");
        let saved = stored("/project", &["/project/src/A.java"]);
        host::storage::set(INVENTORY_KEY, &saved).unwrap();
        let project = host.invocation().project().unwrap();
        let mut indexes = Indexes::new(GLOBS, model::parse);
        assert!(indexes.get(&project, Duration::ZERO).files().is_empty());
        assert!(indexes.cached(&project).unwrap().files().is_empty());
        assert_eq!(host.storage()[INVENTORY_KEY], saved);
        assert!(indexes.get(&project, COMMAND_BUDGET).files().contains_key("/project/src/A.java"));
    }
    #[test]
    fn corrupt_version_traversal_duplicates_and_caps_are_rejected() {
        for value in ["{".into(), stored("/project", &["/project/../A.java"]),
            stored("/project", &["/other/A.java"]), stored("/project", &["/project/A.java", "/project/A.java"]),
            stored("/project", &["/project/readme.md"]), "x".repeat(MAX_INVENTORY_BYTES + 1),
            r#"{"version":2,"roots":{"/project":[]}}"#.into()] {
            assert!(inventory_paths(&value, "/project").is_none());
        }
        assert!(inventory_paths(&stored("/other", &["/other/A.java"]), "/project").is_none());
    }
    #[test]
    fn nested_root_and_missing_paths_cannot_restore_stale_facts() {
        let host = MockHost::install();
        host.add_project("/project/inner");
        host.add_file("/project/inner/A.java", "@org.springframework.stereotype.Component class Inner {}");
        host::storage::set(INVENTORY_KEY, &stored("/project", &["/project/inner/A.java", "/project/Gone.java"])).unwrap();
        let project = host.invocation().project().unwrap();
        let mut indexes = Indexes::new(GLOBS, model::parse);
        let index = indexes.get(&project, COMMAND_BUDGET);
        assert!(index.files().is_empty());
        assert!(!index.complete());
    }
    #[test]
    fn filtered_removal_build_reset_and_closed_root_checkpoint_invalidate_inventory() {
        let host = MockHost::install();
        host.add_file("src/A.java", "@org.springframework.stereotype.Component class A {}");
        let project = host.invocation().project().unwrap();
        let mut indexes = Indexes::new(GLOBS, model::parse);
        indexes.get(&project, COMMAND_BUDGET);
        assert!(!indexes.changed(&project, "/project/readme.md", false));
        indexes.changed(&project, "/project/src", true);
        indexes.checkpoint();
        assert_eq!(inventory_paths(&host.storage()[INVENTORY_KEY], "/project"), Some(vec![]));
        indexes.get(&project, COMMAND_BUDGET).update("/project/src/A.java", "@org.springframework.stereotype.Component class A {}");
        indexes.changed(&project, "/project/pom.xml", false);
        assert_eq!(inventory_paths(&host.storage()[INVENTORY_KEY], "/project"), Some(vec![]));
        host.set_project_root(Some("/other"));
        indexes.prune();
        indexes.checkpoint();
        assert!(inventory_paths(&host.storage()[INVENTORY_KEY], "/project").is_none());
    }
    #[test]
    fn oversized_checkpoint_discards_old_inventory_and_clear_persists_invalidation() {
        let host = MockHost::install();
        let project = host.invocation().project().unwrap();
        host::storage::set(INVENTORY_KEY, &stored("/project", &["/project/Old.java"])).unwrap();
        let mut indexes = Indexes::new(GLOBS, model::parse);
        let index = indexes.ensure("/project");
        for n in 0..80 {
            index.update(&format!("/project/{}{}.java", "a".repeat(3500), n), "@org.springframework.stereotype.Component class A {}");
        }
        indexes.checkpoint();
        let value = &host.storage()[INVENTORY_KEY];
        assert!(value.len() <= MAX_INVENTORY_BYTES);
        assert!(inventory_paths(value, "/project").is_none());
        indexes.clear();
        assert!(inventory_paths(&host.storage()[INVENTORY_KEY], &project.root().unwrap()).is_none());
    }
    #[test]
    fn first_warm_checkpoint_preserves_another_open_roots_pending_inventory() {
        let host = MockHost::install();
        host.add_project("/second");
        host.add_file("/project/A.java", "@org.springframework.stereotype.Component class A {}");
        host.add_file("/second/B.java", "@org.springframework.stereotype.Component class B {}");
        host::storage::set(INVENTORY_KEY, &serde_json::json!({"version":1,"roots":{
            "/project":["/project/A.java"], "/second":["/second/B.java"]}}).to_string()).unwrap();
        let mut indexes = Indexes::new(GLOBS, model::parse);
        let first = host.invocation().project().unwrap();
        assert!(!indexes.get(&first, COMMAND_BUDGET).complete());
        assert_eq!(inventory_paths(&host.storage()[INVENTORY_KEY], "/second"), Some(vec!["/second/B.java".into()]));
        let second = host::projects().unwrap().into_iter().find(|project| project.root().unwrap() == "/second").unwrap();
        let index = indexes.get(&second, COMMAND_BUDGET);
        assert!(index.files().contains_key("/second/B.java"));
        assert!(!index.complete());
    }
    #[test]
    fn storage_denial_quietly_preserves_in_memory_warming() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/A.java", "@org.springframework.stereotype.Component class A {}");
        let project = host.invocation().project().unwrap();
        let mut indexes = Indexes::new(GLOBS, model::parse);
        let index = indexes.get(&project, COMMAND_BUDGET);
        assert!(index.complete());
        assert!(index.files().contains_key("/project/src/A.java"));
        assert!(host.storage().is_empty());
    }
}
