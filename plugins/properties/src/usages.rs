//! Conservative text-use hints, legacy markup candidates and positive cached
//! property declarations. Explicit Java/Kotlin bundle ownership lives in source.rs.

use crate::props::{file_name, parse, range_of, unescape};
use openidea_plugin::{Error, Location, Project, host};
use std::collections::{BTreeMap, HashSet};
use std::ops::Range as Span;

fn key_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '_' | '-')
}

/// Removes from `unused` the keys `text` mentions: every dot-separated run of
/// a word (`a.b` in `"x.a.b.c"`), and plain substrings for keys with other
/// characters. A mention is not a reference, so this errs towards "used".
pub fn mark_used(text: &str, unused: &mut HashSet<String>) {
    for word in text.split(|c: char| !key_char(c)) {
        let segments: Vec<&str> = word.split('.').collect();
        for from in 0..segments.len() {
            for to in from + 1..=segments.len() {
                unused.remove(&segments[from..to].join("."));
            }
        }
    }

    unused.retain(|key| key.chars().all(key_char) || !text.contains(key.as_str()));
}

/// `LoggerConfigPropertyUsageProvider` and `SimpleLoggerImplicitPropertiesUsageProvider`:
/// files read by a framework, not by code.
pub fn implicitly_used(path: &str) -> bool {
    let name = file_name(path);
    ["log4j", "commons-logging", "logging"].iter().any(|prefix| name.starts_with(prefix)) || name == "simplelogger.properties"
}

/// A candidate key use and optional literal bundle hint; neither proves type ownership.
pub struct KeyUse {
    pub span: Span<usize>,
    /// The literal base name of the bundle the call reads, when it names one.
    pub bundle: Option<String>,
}

/// Keys in a template: `#{a.b}` (Thymeleaf messages; Java EL in `.xhtml`/JSP
/// is only navigated), `<spring:message code="a.b"/>` and `<fmt:message key="a.b"/>`.
pub fn markup_uses(text: &str, path: &str) -> Vec<KeyUse> {
    let mut uses = Vec::new();
    let _ = path;

    for (at, _) in text.match_indices("#{") {
        let start = at + 2;
        let len = text[start..].find(|c: char| !key_char(c)).unwrap_or(text.len() - start);
        if len > 0 && matches!(text[start + len..].chars().next(), Some('}' | '(')) {
            uses.push(KeyUse { span: start..start + len, bundle: None });
        }
    }

    let basenames: Vec<&str> = text
        .match_indices("basename=")
        .filter_map(|(at, _)| text[at + 9..].strip_prefix(['"', '\'']).and_then(|v| v.split(['"', '\'']).next()))
        .collect();

    for (tag, attribute, _quiet_attribute) in
        [("<spring:message", "code", "text"), ("<s:message", "code", "text"), ("<fmt:message", "key", "bundle")]
    {
        for (at, _) in text.match_indices(tag) {
            let end = text[at..].find('>').map_or(text.len(), |i| at + i);
            let element = &text[at..end];
            let Some(found) = element.find(&format!(" {attribute}=")) else { continue };
            let value_at = at + found + attribute.len() + 2;
            let Some(quote) = text[value_at..].chars().next().filter(|c| matches!(c, '"' | '\'')) else { continue };
            let start = value_at + 1;
            let Some(len) = text[start..end].find(quote) else { continue };
            if len == 0 || !text[start..start + len].chars().all(key_char) {
                continue;
            }

            let bundle = (tag == "<fmt:message").then(|| basenames.first().map(|b| (*b).to_owned())).flatten();
            uses.push(KeyUse { span: start..start + len, bundle });
        }
    }

    uses
}

pub struct Declaration {
    pub location: Location,
    pub value: String,
}

/// Every key of the project's `.properties` files.
#[derive(Default)]
pub struct KeyIndex {
    pub root: String,
    pub keys: BTreeMap<String, Vec<Declaration>>,
    pub files: Vec<String>,
    /// Positive cached names by source file, so editor completion need not scan
    /// every unrelated key in the project.
    pub file_keys: BTreeMap<String, Vec<String>>,
    /// False when the scan was truncated; `skipped` lists bundle files left out of it.
    pub complete: bool,
    pub skipped: Vec<String>,
}

/// One bounded metadata snapshot per request, never a source read or scan.
/// Cached source paths cannot retain an outer owner after a nested project opens.
pub(crate) struct SourceOwnership {
    root: String,
    roots: Vec<String>,
}
impl SourceOwnership {
    fn normalized(path: &str) -> bool {
        path.starts_with('/') && path.len() <= 4096 && !path.contains('\\')
            && !path.chars().any(char::is_control)
            && path.split('/').skip(1).all(|part| !matches!(part, "" | "." | ".."))
    }
    pub(crate) fn current(root: &str) -> Option<Self> {
        if !Self::normalized(root) { return None; }
        let projects = host::projects().ok()?;
        if projects.len() > 64 { return None; }
        let roots = projects.into_iter().map(|project| project.root().ok()).collect::<Option<Vec<_>>>()?;
        if roots.iter().any(|root| !Self::normalized(root)) || !roots.iter().any(|candidate| candidate == root) {
            return None;
        }
        Some(Self { root: root.into(), roots })
    }
    pub(crate) fn contains(&self, path: &str) -> bool {
        Self::normalized(path) && self.roots.iter().filter(|root| {
            path.strip_prefix(root.as_str()).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        }).max_by_key(|root| root.len()).is_some_and(|root| root == &self.root)
    }
}

impl KeyIndex {
    pub fn build(project: &Project) -> Result<Self, Error> {
        let mut index = Self { root: project.root()?, complete: true, ..Self::default() };
        let ownership = SourceOwnership::current(&index.root).ok_or(Error::Stale)?;
        let started = std::time::Instant::now();
        let mut bytes = 0usize;
        let paths = project.find_files("**/*.properties", 5000)?;
        index.complete = paths.len() < 5000;
        let skip = |index: &mut Self, path: &String| {
            if index.skipped.len() < 4096 {
                index.skipped.push(path.clone());
            } else {
                index.complete = false;
            }
        };
        for path in paths {
            if started.elapsed() > std::time::Duration::from_millis(250) {
                index.complete = false;
                break;
            }
            if !ownership.contains(&path) { skip(&mut index, &path); continue; }
            let Ok(stat) = project.stat(&path) else { skip(&mut index, &path); continue };
            if stat.kind != openidea_plugin::FileKind::File || stat.size > 128 << 10 {
                skip(&mut index, &path);
                continue;
            }
            if stat.size > ((8 << 20) - bytes) as u64 { index.complete = false; break; }
            let Ok(text) = project.read_file(&path) else { skip(&mut index, &path); continue };
            if text.len() > 128 << 10 {
                skip(&mut index, &path);
                continue;
            }
            bytes = bytes.saturating_add(text.len());
            if bytes > 8 << 20 {
                index.complete = false;
                break;
            }
            let properties = parse(&text);
            if properties.len() > 4096 {
                skip(&mut index, &path);
                continue;
            }
            index.files.push(path.clone());
            for p in properties {
                let Some(key) = p.exact_key(&text) else { skip(&mut index, &path); continue; };
                index.file_keys.entry(path.clone()).or_default().push(key.clone());
                index.keys.entry(key).or_default().push(Declaration {
                    location: Location { path: path.clone(), range: range_of(&text, &p.key) },
                    value: unescape(&text[p.value.clone()]),
                });
            }
        }
        Ok(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn index_admission_rejects_oversized_disk_even_when_an_unsaved_buffer_is_small() {
        let host = openidea_plugin::mock::MockHost::install();
        let path = "/project/src/main/resources/messages.properties";
        host.add_file(path, &"x".repeat((128 << 10) + 1));
        host.open_editor(path, "properties", "hello=Unsaved\n", &[]);
        let index = KeyIndex::build(&crate::project_for(path).unwrap()).unwrap();
        assert!(index.files.is_empty());
        assert!(index.keys.is_empty());
        // The policy intentionally refuses rather than replacing the live text
        // with disk text or publishing a declaration after failed admission.
        assert_eq!(host.editor_text(), "hello=Unsaved\n");
        host.add_file(path, "hello=Disk\n");
        let index = KeyIndex::build(&crate::project_for(path).unwrap()).unwrap();
        assert_eq!(index.keys["hello"][0].value, "Unsaved");
    }

    #[test]
    fn marks_mentioned_keys_as_used() {
        let mut unused: HashSet<String> = ["app.title", "app.unused", "error.code", "with space"].iter().map(|s| (*s).to_owned()).collect();
        mark_used("label = bundle.getString(\"app.title\"); x = \"${prefix.error.code}\" + \"with space\";", &mut unused);
        assert_eq!(unused, HashSet::from(["app.unused".to_owned()]));
    }

    #[test]
    fn unowned_getter_spelling_is_not_a_bundle_reference() {
        let doc = openidea_plugin::providers::Document {
            path: "/p/src/main/java/A.java".into(),
            language: "java".into(),
            version: 1,
            text: "bundle.getString(\"key\")".into(),
        };
        assert!(crate::source::context(&doc, doc.text.find("key").unwrap() + 1).is_none());
    }
}
