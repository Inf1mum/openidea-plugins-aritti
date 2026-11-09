//! Current unsaved text owns diagnostics. Events only prepare bounded facts;
//! no file read, project scan or diagnostic publication occurs on this path.
use crate::{
    Properties,
    checks::{Settings, lint},
    props,
};
use openidea_plugin::diagnostics::{DiagnosticsProvider, Document};
use openidea_plugin::{Diagnostic, Position, Range, Severity, host};

pub(super) const MAX_TEXT: usize = 128 << 10;
const MAX_PROPS: usize = 4096;
const MAX_PROBLEMS: usize = 512;
struct Coordinates {
    lines: Vec<usize>,
    columns: Vec<u32>,
}
impl Coordinates {
    fn new(text: &str) -> Self {
        let mut lines = vec![0];
        let mut columns = vec![0; text.len() + 1];
        let mut column = 0;
        for (at, ch) in text.char_indices() {
            columns[at] = column;
            if ch == '\n' {
                lines.push(at + 1);
                column = 0;
            } else {
                column += ch.len_utf16() as u32;
            }
        }
        columns[text.len()] = column;
        Self { lines, columns }
    }
    fn at(&self, at: usize) -> Position {
        Position::new((self.lines.partition_point(|start| *start <= at) - 1) as u32, self.columns[at])
    }
    fn range(&self, start: usize, end: usize) -> Range {
        Range::new(self.at(start), self.at(end))
    }
}
impl DiagnosticsProvider for Properties {
    fn diagnostics(&mut self, doc: &Document) -> Vec<Diagnostic> {
        if doc.language != "properties" || !doc.path.ends_with(".properties") || doc.text.len() > MAX_TEXT {
            return Vec::new();
        }
        let parsed = props::parse(&doc.text);
        if parsed.len() > MAX_PROPS {
            return Vec::new();
        }
        let settings = Settings::load();
        let positions = Coordinates::new(&doc.text);
        let mut out: Vec<_> = lint(&doc.text, &parsed, &settings)
            .into_iter()
            .filter(|problem| {
                problem.span.start < problem.span.end
                    && problem.span.end <= doc.text.len()
                    && doc.text.is_char_boundary(problem.span.start)
                    && doc.text.is_char_boundary(problem.span.end)
            })
            .take(MAX_PROBLEMS)
            .map(|problem| {
                Diagnostic::new(positions.range(problem.span.start, problem.span.end), problem.severity, problem.message).code(problem.code)
            })
            .collect();
        // These are positive facts from a successful bounded read of known
        // sibling locales, not a global proof that a source key is undefined.
        let root = host::project_of(&doc.path).and_then(|project| project.root().ok());
        let safe_current = out.iter().all(|problem| problem.code.as_deref() != Some("InvalidUnicodeEscape"));
        if safe_current && root.as_ref().is_some_and(|root| self.missing_complete.get(&doc.path) == Some(root)) {
            if let Some(missing) = self.missing.get(&doc.path) {
                let facts: std::collections::HashMap<_, _> = missing.iter().take(MAX_PROPS).map(|fact| (fact.key.as_str(), fact)).collect();
                for prop in &parsed {
                    if out.len() == MAX_PROBLEMS {
                        break;
                    }
                    let Some(key) = prop.exact_key(&doc.text) else { continue; };
                    let Some(fact) = facts.get(key.as_str()) else {
                        continue;
                    };
                    let locales: Vec<_> = fact
                        .files
                        .iter()
                        .filter(|(locale, _)| !settings.ignored_suffixes.contains(locale))
                        .take(16)
                        .map(|(locale, _)| props::locale_name(locale).chars().take(64).collect::<String>())
                        .collect();
                    if !locales.is_empty() && prop.key.start < prop.key.end {
                        out.push(
                            Diagnostic::new(
                                positions.range(prop.key.start, prop.key.end),
                                Severity::Error,
                                format!(
                                    "Property '{}' has no translation in {}{}.",
                                    key.chars().take(128).collect::<String>(),
                                    locales.join(", "),
                                    if fact.files.len() > 16 { " (more locales omitted)" } else { "" }
                                ),
                            )
                            .code("IncompleteProperty"),
                        );
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{Event, FilesChanged, Plugin, mock::MockHost, text};
    fn doc(text: &str) -> Document {
        Document { path: "/project/messages.properties".into(), language: "properties".into(), version: 7, text: text.into() }
    }
    fn codes(plugin: &mut Properties, text: &str) -> Vec<String> {
        plugin.diagnostics(&doc(text)).into_iter().filter_map(|problem| problem.code).collect()
    }
    #[test]
    fn unsaved_source_checks_follow_fixes_without_disk_reads_or_publication() {
        let host = MockHost::install().with_capabilities(&[]);
        let mut plugin = Properties::default();
        let source = "# 😀 prefix\r\na=1\r\na=2\r\nb:😀  \r\nc=\\u12g\r\n";
        let problems = plugin.diagnostics(&doc(source));
        for code in ["DuplicatePropertyKey", "WrongPropertyKeyValueDelimiter", "TrailingSpacesInProperty", "InvalidUnicodeEscape"] {
            assert!(problems.iter().any(|problem| problem.code.as_deref() == Some(code)), "{code}");
        }
        for problem in &problems {
            let start = text::offset(source, problem.range.start).unwrap();
            let end = text::offset(source, problem.range.end).unwrap();
            assert!(start < end);
            assert!(end <= source.len());
            assert_eq!(problem.range.start, text::position(source, start));
            assert_eq!(problem.range.end, text::position(source, end));
        }
        let trailing = problems.iter().find(|problem| problem.code.as_deref() == Some("TrailingSpacesInProperty")).unwrap();
        assert_eq!(trailing.range, Range::new(Position::new(3, 4), Position::new(3, 6)));
        assert!(plugin.diagnostics(&doc("a=1\r\nb=😀\r\nc=valid\r\n")).is_empty());
        assert!(host.diagnostics().is_empty());
        assert!(plugin.index.is_none());
        assert!(plugin.missing.is_empty());
    }
    #[test]
    fn optional_source_rules_and_caps_are_current_text_owned() {
        let host = MockHost::install();
        let mut plugin = Properties::default();
        assert!(codes(&mut plugin, "b=Loading...\na=1\n").is_empty());
        host.set_setting("inspections.useEllipsis", "true");
        host.set_setting("inspections.alphaUnsorted", "true");
        let found = codes(&mut plugin, "b=Loading...\na=1\n");
        assert!(found.contains(&"UseEllipsisInPropertyInspection".into()));
        assert!(found.contains(&"AlphaUnsortedPropertiesFile".into()));
        assert!(plugin.diagnostics(&doc(&"#".repeat(MAX_TEXT + 1))).is_empty());
        assert!(plugin.diagnostics(&doc(&"x=1\n".repeat(MAX_PROPS + 1))).is_empty());
        let problems = plugin.diagnostics(&doc(&"a=1\n".repeat(1000)));
        assert_eq!(problems.len(), MAX_PROBLEMS);
    }
    #[test]
    fn only_complete_root_owned_warm_locale_facts_produce_missing_translation_diagnostics() {
        let host = MockHost::install();
        host.add_file("/project/messages.properties", "hello=Hello\n");
        host.add_file("/project/messages_fr.properties", "other=Autre\n");
        let mut plugin = Properties::default();
        assert!(codes(&mut plugin, "hello=Unsaved\n").is_empty(), "cold discovery is not absence");
        plugin.handle_events(&[Event::FileOpened("/project/messages.properties".into())], &host.invocation());
        assert!(host.diagnostics().is_empty(), "events only warm facts");
        assert!(codes(&mut plugin, "hello=Unsaved\n").contains(&"IncompleteProperty".into()));
        assert!(codes(&mut plugin, "renamed=Unsaved\n").is_empty(), "new keys with no warm absence fact stay quiet");
        plugin.missing_complete.clear();
        assert!(codes(&mut plugin, "hello=Unsaved\n").is_empty());
        plugin.handle_events(&[Event::FileOpened("/project/messages.properties".into())], &host.invocation());
        host.set_project_root(Some("/other"));
        assert!(codes(&mut plugin, "hello=Unsaved\n").is_empty());
        host.set_project_root(Some("/project"));
        plugin.handle_events(
            &[Event::FilesChanged(FilesChanged { modified: vec!["/project/messages_fr.properties".into()], ..Default::default() })],
            &host.invocation(),
        );
        assert!(codes(&mut plugin, "hello=Unsaved\n").is_empty(), "changed locale invalidates absence facts");
    }
    #[test]
    fn unknown_oversized_or_uninterpretable_locale_discovery_is_quiet() {
        let host = MockHost::install();
        host.add_file("messages.properties", "hello=Hello\n");
        host.add_file("messages_fr.properties", &"#".repeat(MAX_TEXT + 1));
        let mut plugin = Properties::default();
        let event = [Event::FileOpened("/project/messages.properties".into())];
        plugin.handle_events(&event, &host.invocation());
        assert!(codes(&mut plugin, "hello=Hello\n").is_empty());
        assert!(!plugin.missing_complete.contains_key("/project/messages.properties"));
        host.add_file("messages_fr.properties", "other=\\uXX\n");
        plugin.handle_events(&event, &host.invocation());
        assert!(codes(&mut plugin, "hello=Hello\n").is_empty());
        assert!(host.diagnostics().is_empty());
        let source = Document { language: "java".into(), text: "messageSource.getMessage(\"missing\",null,locale)".into(), ..doc("") };
        assert!(plugin.diagnostics(&source).is_empty(), "classpath/type owner unknown");
    }
    #[test]
    fn raw_inventory_cap_cannot_be_hidden_by_filtering_other_bundle_names() {
        let host = MockHost::install();
        host.add_file("messages.properties", "hello=Hello\n");
        host.add_file("messages_fr.properties", "other=Autre\n");
        for n in 0..201 {
            host.add_file(&format!("messagesx{n}.properties"), "other=1\n");
        }
        let mut plugin = Properties::default();
        plugin.handle_events(&[Event::FileOpened("/project/messages.properties".into())], &host.invocation());
        assert!(!plugin.missing_complete.contains_key("/project/messages.properties"));
        assert!(codes(&mut plugin, "hello=Hello\n").is_empty());
        assert!(host.diagnostics().is_empty());
    }
}
