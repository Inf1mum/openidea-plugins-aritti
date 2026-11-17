//! Per-project file indexes read a little at a time, so that every editor
//! provider call stays within its 50 ms.

use openidea_plugin::{Error, Event, Project, host};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

const MAX_FILES: u32 = 10_000;
/// How long a full pass over the project is trusted; open documents are always re-read.
const TTL: Duration = Duration::from_secs(120);
/// Reading time per editor provider call.
pub const PROVIDER_BUDGET: Duration = Duration::from_millis(30);
/// Reading time for a command or a view.
pub const COMMAND_BUDGET: Duration = Duration::from_secs(3);
/// Reading time a diagnostics call may spend advancing a cold or stale index.
pub const DIAGNOSTICS_WARM: Duration = Duration::from_millis(10);

/// The project holding `path` (the innermost when projects nest).
pub fn project_of(path: &str) -> Option<Project> {
    host::project_of(path)
}

/// What a project's files matching some globs say, by absolute path. The
/// first pass fills `files` as it goes; later passes replace it when done.
pub struct Index<T> {
    files: BTreeMap<String, T>,
    next: BTreeMap<String, T>,
    unread: Vec<String>,
    scanned: Option<Instant>,
    /// A pass has started and not finished.
    reading: bool,
    passes: u32,
    /// Unreadable files as of the last finished pass, kept current by events.
    failed: BTreeSet<String>,
    discovery_incomplete: bool,
    /// What the pass in progress found so far; published when it finishes.
    pass_failed: BTreeSet<String>,
    pass_discovery_incomplete: bool,
    /// Changes were missed: the last pass no longer counts.
    stale: bool,
    globs: &'static [&'static str],
    parse: fn(&str, &str) -> Option<T>,
}

impl<T: Clone> Index<T> {
    /// `parse(path, text)` keeps what matters of a file (`None`: nothing).
    pub fn new(globs: &'static [&'static str], parse: fn(&str, &str) -> Option<T>) -> Self {
        Self {
            files: BTreeMap::new(),
            next: BTreeMap::new(),
            unread: Vec::new(),
            scanned: None,
            reading: false,
            passes: 0,
            failed: BTreeSet::new(),
            discovery_incomplete: false,
            pass_failed: BTreeSet::new(),
            pass_discovery_incomplete: false,
            stale: false,
            globs,
            parse,
        }
    }

    /// Reads files for at most `budget`.
    pub fn scan(&mut self, project: &Project, budget: Duration) {
        if self.unread.is_empty() && self.scanned.is_none_or(|at| at.elapsed() > TTL) {
            self.pass_failed.clear();
            self.pass_discovery_incomplete = false;
            let mut paths = Vec::new();
            for glob in self.globs {
                match project.find_files(glob, MAX_FILES) {
                    Ok(found) => {
                        self.pass_discovery_incomplete |= found.len() >= MAX_FILES as usize;
                        paths.extend(found);
                    }
                    Err(_) => self.pass_discovery_incomplete = true,
                }
            }
            paths.sort();
            paths.dedup();
            paths.reverse(); // popped from the end: read in path order
            self.unread = paths;
            self.scanned = Some(Instant::now());
            self.reading = true;
        }
        let start = Instant::now();
        let first = self.passes == 0;
        while start.elapsed() < budget
            && let Some(path) = self.unread.pop()
        {
            let Ok(text) = project.read_file(&path) else {
                self.pass_failed.insert(path);
                continue;
            };
            let target = if first { &mut self.files } else { &mut self.next };
            match (self.parse)(&path, &text) {
                Some(value) => {
                    target.insert(path, value);
                }
                None => {
                    target.remove(&path);
                }
            }
        }
        if self.reading && self.unread.is_empty() {
            if !first {
                self.files = std::mem::take(&mut self.next);
            }
            self.failed = std::mem::take(&mut self.pass_failed);
            self.discovery_incomplete = self.pass_discovery_incomplete;
            self.stale = false;
            self.reading = false;
            self.passes += 1;
        }
    }

    /// Replaces what a file says with its current text (an open document).
    pub fn update(&mut self, path: &str, text: &str) {
        self.remove(path);
        let Some(value) = (self.parse)(path, text) else {
            return;
        };
        if self.reading && self.passes > 0 {
            self.next.insert(path.to_owned(), value.clone());
        }
        self.files.insert(path.to_owned(), value);
    }

    /// Whether the last pass finished (some reads may have failed) and no
    /// change was missed since. A refresh in progress keeps the last pass.
    pub fn walked(&self) -> bool {
        self.passes > 0 && !self.stale
    }

    /// Files the last pass, or a later change, could not read.
    pub fn unreadable(&self) -> usize {
        self.failed.len()
    }

    /// Whether the last pass was whole and uncapped and every file was read.
    /// Absence-based diagnostics must not trust partial discovery or failed reads.
    pub fn complete(&self) -> bool {
        self.walked() && self.unreadable() == 0 && !self.discovery_incomplete
    }

    /// Changes were missed: `complete()` and `fresh()` stay false until the
    /// next pass, which starts over, finishes.
    pub fn mark_stale(&mut self) {
        self.stale = true;
        self.scanned = None;
        self.unread.clear();
        self.next.clear();
    }

    /// A complete source pass still inside its trust interval. Cache-only live
    /// checks use this for absence claims without triggering a refresh themselves.
    pub fn fresh(&self) -> bool {
        self.complete() && self.scanned.is_some_and(|at| at.elapsed() <= TTL)
    }

    /// Whether more queued work can be advanced by another render/provider call.
    pub fn scanning(&self) -> bool {
        self.reading
    }

    /// Forgets the files so the next scan starts over.
    pub fn reset(&mut self) {
        self.files.clear();
        self.next.clear();
        self.unread.clear();
        self.scanned = None;
        self.reading = false;
        self.passes = 0;
        self.failed.clear();
        self.discovery_incomplete = false;
        self.pass_failed.clear();
        self.pass_discovery_incomplete = false;
        self.stale = false;
    }

    pub fn remove(&mut self, path: &str) {
        self.failed.remove(path);
        self.pass_failed.remove(path);
        self.files.remove(path);
        self.next.remove(path);
        self.unread.retain(|p| p != path);
    }

    /// Forgets what a file said because it could not be read.
    fn fail(&mut self, path: &str) {
        self.remove(path);
        self.failed.insert(path.to_owned());
        if self.reading {
            self.pass_failed.insert(path.to_owned());
        }
    }

    pub fn files(&self) -> &BTreeMap<String, T> {
        &self.files
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &T)> {
        self.files.iter().map(|(p, v)| (p.as_str(), v))
    }
}

/// What an index covers when it is not the whole project: "scanning…" while
/// reading, "partial index (N unreadable)" or "partial index" after a pass
/// that skipped files or hit the discovery cap; `None` when whole.
pub fn coverage_note<T: Clone>(index: &Index<T>) -> Option<String> {
    if index.scanning() {
        return Some(SCANNING_NOTE.to_owned());
    }
    if index.complete() {
        return None;
    }
    Some(match index.unreadable() {
        0 => "partial index".to_owned(),
        unreadable => format!("partial index ({unreadable} unreadable)"),
    })
}

pub const SCANNING_NOTE: &str = "scanning…";

/// The message of a view with nothing to list: `none_found` is e.g. "No STOMP destinations found".
pub fn empty_message(none_found: &str, note: Option<&str>) -> String {
    match note {
        None => format!("{none_found}."),
        Some(SCANNING_NOTE) => "Scanning…".to_owned(),
        Some(note) => format!("{none_found} · {note}"),
    }
}

/// What [`Indexes::on_events`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventOutcome {
    /// Changed paths applied to a cached index.
    pub updated: usize,
    /// The deadline cut the batch short; the affected indexes were marked stale.
    pub stale: bool,
}

/// One [`Index`] per project root.
pub struct Indexes<T> {
    by_root: BTreeMap<String, Index<T>>,
    unidentified: Index<T>,
    globs: &'static [&'static str],
    parse: fn(&str, &str) -> Option<T>,
}

impl<T: Clone> Indexes<T> {
    pub fn new(globs: &'static [&'static str], parse: fn(&str, &str) -> Option<T>) -> Self {
        Self { by_root: BTreeMap::new(), unidentified: Index::new(globs, parse), globs, parse }
    }

    /// The index of `project`, read for at most `budget` more.
    pub fn get(&mut self, project: &Project, budget: Duration) -> &mut Index<T> {
        let Ok(root) = project.root() else {
            self.unidentified.reset();
            return &mut self.unidentified;
        };
        if root.is_empty() {
            self.unidentified.reset();
            return &mut self.unidentified;
        }
        let (globs, parse) = (self.globs, self.parse);
        let index = self.by_root.entry(root).or_insert_with(|| Index::new(globs, parse));
        index.scan(project, budget);
        index
    }

    /// The index of the project holding `path`, with the open document's text applied.
    pub fn for_document(&mut self, path: &str, text: &str, budget: Duration) -> Option<&mut Index<T>> {
        let project = project_of(path)?;
        let index = self.get(&project, budget);
        if index.globs.iter().any(|g| glob_matches(g, path)) {
            index.update(path, text);
        }
        Some(index)
    }

    /// Current-document facts plus already cached project facts, without file
    /// discovery or scanning. A cold cache stays incomplete until a background,
    /// navigation or explicit command path warms it.
    pub fn for_cached_document(&mut self, path: &str, text: &str) -> Option<&mut Index<T>> {
        let project = project_of(path)?;
        let root = project.root().ok().filter(|root| !root.is_empty())?;
        let (globs, parse) = (self.globs, self.parse);
        let index = self.by_root.entry(root).or_insert_with(|| Index::new(globs, parse));
        if index.globs.iter().any(|g| glob_matches(g, path)) {
            index.update(path, text);
        }
        Some(index)
    }

    /// Like [`Self::for_cached_document`], but a cold, stale, expired or
    /// refreshing index reads on for at most `budget` first.
    pub fn for_cached_document_warm(&mut self, path: &str, text: &str, budget: Duration) -> Option<&mut Index<T>> {
        let project = project_of(path)?;
        let root = project.root().ok().filter(|root| !root.is_empty())?;
        let (globs, parse) = (self.globs, self.parse);
        let index = self.by_root.entry(root).or_insert_with(|| Index::new(globs, parse));
        if !index.fresh() || index.scanning() {
            index.scan(&project, budget);
        }
        if index.globs.iter().any(|g| glob_matches(g, path)) {
            index.update(path, text);
        }
        Some(index)
    }

    /// Apply a watched change to a cache already owned by this project. This
    /// does not start a scan; a missing cache will discover the file normally.
    pub fn update_existing(&mut self, project: &Project, path: &str, text: Option<&str>) -> bool {
        let Some(index) = self.owned(project, path) else { return false };
        match text {
            Some(text) => index.update(path, text),
            None => index.remove(path),
        }
        true
    }

    /// Applies file events path by path to the caches they touch, reading each
    /// changed file once. A missing file is removed; an unreadable one counts
    /// against completeness on its own. Paths left when `deadline` passes mark
    /// their index stale instead.
    pub fn on_events(&mut self, events: &[Event], deadline: Instant) -> EventOutcome {
        let mut outcome = EventOutcome::default();
        for (path, removed) in changed_paths(events) {
            let Some(project) = project_of(path) else { continue };
            let Some(index) = self.owned(&project, path) else { continue };
            if Instant::now() >= deadline {
                index.mark_stale();
                outcome.stale = true;
                continue;
            }
            outcome.updated += 1;
            if removed {
                index.remove(path);
                continue;
            }
            match project.read_file(path) {
                Ok(text) => index.update(path, &text),
                // A file that exists but is not text may be reported as missing.
                Err(Error::NotFound(_)) if project.read_bytes(path).is_ok() => index.fail(path),
                Err(Error::NotFound(_)) => index.remove(path),
                Err(_) => index.fail(path),
            }
        }
        outcome
    }

    /// The cache of `project` that watches `path`, when `path` is inside its root.
    fn owned(&mut self, project: &Project, path: &str) -> Option<&mut Index<T>> {
        let root = project.root().ok()?;
        let prefix = format!("{}/", root.trim_end_matches('/'));
        let relative = path.strip_prefix(&prefix)?;
        if relative.split('/').any(|part| matches!(part, "." | "..")) {
            return None;
        }
        self.by_root.get_mut(&root).filter(|index| index.globs.iter().any(|g| glob_matches(g, path)))
    }

    /// Forgets every project's files.
    pub fn reset(&mut self) {
        self.by_root.clear();
        self.unidentified.reset();
    }
}

/// Changed paths with whether they were removed: removals first, so a path
/// removed and created again in one batch ends up read.
fn changed_paths(events: &[Event]) -> Vec<(&str, bool)> {
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    for event in events {
        match event {
            Event::FileSaved(path) => changed.push((path.as_str(), false)),
            Event::FilesChanged(files) => {
                removed.extend(files.removed.iter().map(|path| (path.as_str(), true)));
                changed.extend(files.created.iter().chain(&files.modified).map(|path| (path.as_str(), false)));
            }
            _ => {}
        }
    }
    removed.extend(changed);
    removed
}

/// Whether `path` (absolute or relative) matches a project glob such as
/// `**/*.java` or `**/src/main/resources/**/*.yml`. `**` spans directories,
/// `*` stays within one; the glob is matched against the path's tail.
pub fn glob_matches(glob: &str, path: &str) -> bool {
    fn go(g: &[u8], p: &[u8]) -> bool {
        match g {
            [] => p.is_empty(),
            [b'*', b'*', b'/', rest @ ..] => (0..=p.len()).any(|i| (i == 0 || p[i - 1] == b'/') && go(rest, &p[i..])),
            [b'*', b'*', rest @ ..] => (0..=p.len()).any(|i| go(rest, &p[i..])),
            [b'*', rest @ ..] => (0..=p.len()).take_while(|&i| i == 0 || p[i - 1] != b'/').any(|i| go(rest, &p[i..])),
            [c, rest @ ..] => p.first() == Some(c) && go(rest, &p[1..]),
        }
    }
    let path = path.replace('\\', "/");
    let path = path.trim_start_matches('/');
    let glob = glob.trim_start_matches('/');
    // A relative glob may match from any directory of an absolute path.
    (0..=path.len()).any(|i| (i == 0 || path.as_bytes()[i - 1] == b'/') && go(glob.as_bytes(), &path.as_bytes()[i..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::FilesChanged;
    use openidea_plugin::mock::MockHost;

    fn words(_: &str, text: &str) -> Option<usize> {
        let n = text.split_whitespace().count();
        (n > 0).then_some(n)
    }

    #[test]
    fn reads_the_project_and_applies_open_documents() {
        let host = MockHost::install();
        host.add_file("src/A.java", "a b c");
        host.add_file("src/B.java", "");
        host.add_file("README.md", "x");
        let mut indexes = Indexes::new(&["**/*.java"], words);
        let project = host.invocation().project().unwrap();
        let index = indexes.get(&project, COMMAND_BUDGET);
        assert!(index.complete());
        assert_eq!(index.iter().map(|(p, n)| (p.rsplit('/').next().unwrap(), *n)).collect::<Vec<_>>(), [("A.java", 3)]);

        let index = indexes.for_document("/project/src/B.java", "one two", PROVIDER_BUDGET).unwrap();
        assert_eq!(index.files().keys().collect::<Vec<_>>(), ["/project/src/A.java", "/project/src/B.java"]);
        let index = indexes.for_document("/project/README.md", "x y", PROVIDER_BUDGET).unwrap();
        assert_eq!(index.files().len(), 2, "not one of the globs");
    }

    #[test]
    fn denied_discovery_is_partial_and_does_not_request_more_work() {
        let host = MockHost::install().with_capabilities(&[]);
        let project = host.invocation().project().unwrap();
        let mut index = Index::new(&["**/*.java"], words);
        index.scan(&project, COMMAND_BUDGET);
        assert!(!index.complete());
        assert!(!index.scanning());
        index.update("/project/A.java", "current document");
        assert!(!index.complete(), "one open document cannot prove project-wide absence");
    }

    #[test]
    fn completed_but_expired_facts_cannot_prove_current_absence() {
        let host = MockHost::install();
        host.add_file("src/A.java", "known");
        let mut index = Index::new(&["**/*.java"], words);
        index.scan(&host.invocation().project().unwrap(), COMMAND_BUDGET);
        assert!(index.fresh());
        index.scanned = Some(Instant::now() - TTL - Duration::from_secs(1));
        assert!(index.complete(), "a historical pass still completed");
        assert!(!index.fresh(), "cache-only absence checks need current provenance");
        index.update("/project/src/A.java", "unsaved");
        assert!(!index.fresh(), "an open document cannot refresh the rest of the project");
    }

    #[test]
    fn matches_globs() {
        assert!(glob_matches("**/*.java", "/p/src/A.java"));
        assert!(glob_matches("**/*.java", "A.java"));
        assert!(!glob_matches("**/*.java", "/p/A.javax"));
        assert!(glob_matches("**/application*.yml", "/p/src/main/resources/application-dev.yml"));
        assert!(!glob_matches("src/*.java", "/p/src/a/A.java"));
        assert!(glob_matches("**/resources/**/*.graphqls", "/p/src/main/resources/graphql/schema.graphqls"));
        assert!(glob_matches("**/resources/**/*.graphqls", "/p/src/main/resources/schema.graphqls"));
    }
    #[test]
    fn watched_changes_update_only_the_owned_cache_and_remove_old_paths() {
        let host = MockHost::install();
        host.add_file("src/A.java", "a");
        let project = host.invocation().project().unwrap();
        let mut indexes = Indexes::new(&["**/*.java"], words);
        indexes.get(&project, COMMAND_BUDGET);
        assert!(indexes.update_existing(&project, "/project/src/A.java", None));
        assert!(indexes.update_existing(&project, "/project/src/B.java", Some("b c")));
        assert!(!indexes.get(&project, Duration::ZERO).files().contains_key("/project/src/A.java"));
        assert_eq!(indexes.get(&project, Duration::ZERO).files()["/project/src/B.java"], 2);
        assert!(!indexes.update_existing(&project, "/other/src/B.java", Some("bad")));
        assert!(!indexes.update_existing(&project, "/project/../other/B.java", Some("bad")));
        host.set_project_root(Some("/second"));
        let second = host.invocation().project().unwrap();
        assert!(!indexes.update_existing(&second, "/project/src/B.java", None));
        assert_eq!(indexes.get(&project, Duration::ZERO).files()["/project/src/B.java"], 2);
    }
    #[test]
    fn denied_project_identity_cannot_reuse_an_anonymous_cache() {
        let host = MockHost::install().with_capabilities(&[]);
        let mut indexes = Indexes::new(&["**/*.java"], words);
        let first = host.invocation().project().unwrap();
        indexes.get(&first, Duration::ZERO).update("/project/A.java", "private first");
        host.set_project_root(Some("/second"));
        let second = host.invocation().project().unwrap();
        assert!(indexes.get(&second, Duration::ZERO).files().is_empty());
        assert!(indexes.by_root.is_empty());
    }
    #[test]
    fn cached_document_requests_never_begin_discovery_and_keep_cold_provenance() {
        let host = MockHost::install();
        host.add_file("src/Disk.java", "disk facts");
        let mut indexes = Indexes::new(&["**/*.java"], words);
        let index = indexes.for_cached_document("/project/src/Open.java", "current unsaved facts").unwrap();
        assert!(!index.complete());
        assert!(index.scanned.is_none() && index.unread.is_empty() && !index.reading);
        assert_eq!(index.files().len(), 1);
        assert_eq!(index.files()["/project/src/Open.java"], 3);
        let project = host.invocation().project().unwrap();
        indexes.get(&project, COMMAND_BUDGET);
        let warm = indexes.for_cached_document("/project/src/Open.java", "new").unwrap();
        assert!(warm.complete());
        assert!(warm.files().contains_key("/project/src/Disk.java"));
        assert_eq!(warm.files()["/project/src/Open.java"], 1);
        host.set_project_root(Some("/second"));
        let second = indexes.for_cached_document("/second/src/Next.java", "other").unwrap();
        assert_eq!(second.files().len(), 1);
        assert!(!second.files().contains_key("/project/src/Disk.java"));
    }

    fn changed(created: &[&str], modified: &[&str], removed: &[&str]) -> Vec<Event> {
        let owned = |paths: &[&str]| paths.iter().map(|p| (*p).to_owned()).collect();
        vec![Event::FilesChanged(FilesChanged { created: owned(created), modified: owned(modified), removed: owned(removed) })]
    }

    #[test]
    fn coverage_note_and_empty_message() {
        let host = MockHost::install();
        host.add_file("src/A.java", "a");
        let project = host.invocation().project().unwrap();
        let mut index = Index::new(&["**/*.java"], words);
        index.scan(&project, Duration::ZERO);
        assert_eq!(coverage_note(&index).as_deref(), Some("scanning…"));
        assert_eq!(empty_message("None found", coverage_note(&index).as_deref()), "Scanning…");
        index.scan(&project, COMMAND_BUDGET);
        assert_eq!(coverage_note(&index), None);
        assert_eq!(empty_message("None found", None), "None found.");
        host.add_file_bytes("src/Latin.java", b"caf\xe9");
        let mut indexes = Indexes::new(&["**/*.java"], words);
        indexes.get(&project, COMMAND_BUDGET);
        indexes.on_events(&changed(&["/project/src/Latin.java"], &[], &[]), Instant::now() + COMMAND_BUDGET);
        let index = indexes.get(&project, Duration::ZERO);
        let note = coverage_note(index);
        assert_eq!(note.as_deref(), Some("partial index (1 unreadable)"));
        assert_eq!(empty_message("None found", note.as_deref()), "None found · partial index (1 unreadable)");
    }

    #[test]
    fn complete_survives_ttl_refresh() {
        let host = MockHost::install();
        host.add_file("src/A.java", "a");
        host.add_file("src/B.java", "b");
        let project = host.invocation().project().unwrap();
        let mut index = Index::new(&["**/*.java"], words);
        index.scan(&project, COMMAND_BUDGET);
        assert!(index.complete());
        index.scanned = Some(Instant::now() - TTL - Duration::from_secs(1));
        index.scan(&project, Duration::ZERO);
        assert!(index.scanning(), "the refresh is reading");
        assert!(index.complete(), "the previous pass stays complete while refreshing");
        assert!(index.walked());
        index.scan(&project, COMMAND_BUDGET);
        assert!(!index.scanning() && index.complete());
    }

    #[test]
    fn on_events_notfound_is_removal() {
        let host = MockHost::install();
        host.add_file("src/A.java", "a");
        let project = host.invocation().project().unwrap();
        let mut indexes = Indexes::new(&["**/*.java"], words);
        indexes.get(&project, COMMAND_BUDGET);
        indexes.update_existing(&project, "/project/src/Gone.java", Some("gone"));
        let outcome = indexes.on_events(&changed(&[], &["/project/src/Gone.java"], &[]), Instant::now() + COMMAND_BUDGET);
        assert_eq!((outcome.updated, outcome.stale), (1, false));
        let index = indexes.get(&project, Duration::ZERO);
        assert!(!index.files().contains_key("/project/src/Gone.java"));
        assert!(index.files().contains_key("/project/src/A.java"));
        assert!(index.complete());
    }

    #[test]
    fn on_events_read_error_fails_only_that_path() {
        let host = MockHost::install();
        host.add_file("src/A.java", "a");
        host.add_file("src/B.java", "b c");
        let project = host.invocation().project().unwrap();
        let mut indexes = Indexes::new(&["**/*.java"], words);
        indexes.get(&project, COMMAND_BUDGET);
        host.add_file_bytes("src/Latin.java", b"caf\xe9");
        let events = changed(&["/project/src/Latin.java"], &["/project/src/B.java"], &[]);
        let outcome = indexes.on_events(&events, Instant::now() + COMMAND_BUDGET);
        assert_eq!((outcome.updated, outcome.stale), (2, false));
        let index = indexes.get(&project, Duration::ZERO);
        assert_eq!(index.unreadable(), 1);
        assert!(index.walked());
        assert!(!index.complete());
        assert_eq!(index.files().keys().collect::<Vec<_>>(), ["/project/src/A.java", "/project/src/B.java"]);
        host.add_file("src/Latin.java", "now text");
        indexes.on_events(&changed(&[], &["/project/src/Latin.java"], &[]), Instant::now() + COMMAND_BUDGET);
        let index = indexes.get(&project, Duration::ZERO);
        assert_eq!(index.unreadable(), 0);
        assert!(index.complete());
    }

    #[test]
    fn on_events_deadline_marks_stale() {
        let host = MockHost::install();
        host.add_file("src/A.java", "a");
        let project = host.invocation().project().unwrap();
        let mut indexes = Indexes::new(&["**/*.java"], words);
        assert!(indexes.get(&project, COMMAND_BUDGET).fresh());
        host.add_file("src/A.java", "a b");
        let past = Instant::now() - Duration::from_secs(1);
        let outcome = indexes.on_events(&changed(&[], &["/project/src/A.java"], &[]), past);
        assert_eq!((outcome.updated, outcome.stale), (0, true));
        let index = indexes.by_root.get_mut("/project").unwrap();
        assert!(!index.fresh() && !index.complete());
        index.scan(&project, COMMAND_BUDGET);
        assert!(index.fresh(), "the next finished pass clears the stale mark");
        assert_eq!(index.files()["/project/src/A.java"], 2);
    }

    #[test]
    fn warm_advances_when_not_fresh() {
        let host = MockHost::install();
        host.add_file("src/A.java", "a");
        host.add_file("src/B.java", "b");
        host.add_file("src/C.java", "c");
        let mut indexes = Indexes::new(&["**/*.java"], words);
        let index = indexes.for_cached_document_warm("/project/src/A.java", "open a", DIAGNOSTICS_WARM).unwrap();
        assert_eq!(index.files().len(), 3);
        assert!(index.fresh());
        assert_eq!(index.files()["/project/src/A.java"], 2, "the open document wins");
    }
}
