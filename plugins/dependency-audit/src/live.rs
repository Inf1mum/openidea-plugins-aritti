//! Bounded, read-only editor evidence. No fetches, discovery or storage writes.
use crate::{
    diagnostic,
    manifest::{self, Dep},
    osv,
};
use openidea_plugin::{Diagnostic, providers::Document};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const MAX_TEXT: usize = 512 << 10;
const MAX_CACHED_TEXT: usize = 4 << 20;

#[derive(Default)]
pub(crate) struct Live {
    // A lock-dependent manifest may reuse facts only while its full text matches.
    // Edits to a range must not inherit the previous lock resolution.
    files: HashMap<(String, String), (String, Vec<Dep>)>,
    bytes: usize,
}

impl Live {
    pub fn remember(&mut self, root: &str, path: &str, text: &str, deps: &[Dep]) {
        if text.len() > MAX_TEXT || deps.len() > 2000 {
            return;
        }
        if self.files.len() >= 64 || self.bytes.saturating_add(text.len()) > MAX_CACHED_TEXT {
            self.clear();
        }
        let key = (root.to_owned(), path.to_owned());
        if let Some((old, _)) = self.files.remove(&key) {
            self.bytes -= old.len();
        }
        self.bytes += text.len();
        self.files.insert(key, (text.to_owned(), deps.to_vec()));
    }

    pub fn clear(&mut self) {
        self.files.clear();
        self.bytes = 0;
    }

    pub fn diagnostics(&self, root: &str, doc: &Document) -> Vec<Diagnostic> {
        if doc.text.len() > MAX_TEXT || manifest::kind(&doc.path).is_none() {
            return Vec::new();
        }
        let deps = self
            .files
            .get(&(root.to_owned(), doc.path.clone()))
            .filter(|(text, _)| text == &doc.text)
            .map(|(_, deps)| deps.clone())
            .unwrap_or_else(|| manifest::parse(&doc.path, &doc.text, None));
        let scopes = crate::scoped::load(root);
        osv::cached_findings(&deps, Instant::now() + Duration::from_millis(12))
            .into_iter()
            .filter_map(|(mut f,at)|crate::scoped::filter(&scopes,&mut f).then_some((f,at)))
            .take(500)
            .map(|(finding, at)| {
                let mut item = diagnostic(&finding, &doc.text);
                let stale = if crate::now().saturating_sub(at) >= osv::QUERY_TTL_SECS { "; stale, rescan recommended" } else { "" };
                // Keep the host's 4 KiB message bound, including the provenance suffix.
                let end = item.message.char_indices().map(|(i, _)| i).take_while(|i| *i <= 3400).last().unwrap_or(0);
                if item.message.len() > 3400 {
                    item.message.truncate(end);
                }
                item.message.push_str(&format!(" [Cached OSV query: Unix {at}{stale}]"));
                item
            })
            .collect()
    }
}
