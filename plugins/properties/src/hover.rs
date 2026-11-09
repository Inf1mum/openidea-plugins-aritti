//! Hover never builds an index or reads another document. Candidate source
//! locations are informational: global decoded-key matches are not Rename edges.
use crate::{Properties, props, usages};
use openidea_plugin::hover::{Document, Hover, HoverProvider};
use openidea_plugin::{Position, host, text};

const MAX_DOCUMENT: usize = 256 << 10;
const MAX_RESULTS: usize = 8;
const MAX_MARKDOWN: usize = 8192;
fn escaped(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars().take(512) {
        if ch.is_control() {
            out.extend(ch.escape_default());
        } else {
            if "\\`*_{}[]<>()#!|~".contains(ch) {
                out.push('\\');
            }
            out.push(ch);
        }
    }
    if value.chars().count() > 512 {
        out.push('…');
    }
    out
}
impl HoverProvider for Properties {
    fn hover(&mut self, doc: &Document, at: Position) -> Option<Hover> {
        if doc.text.len() > MAX_DOCUMENT {
            return None;
        }
        let offset = text::offset(&doc.text, at)?;
        if doc.language == "properties" {
            let prop = props::parse(&doc.text).into_iter().take(4096).find(|prop| prop.key.start <= offset && offset < prop.key.end)?;
            let raw = &doc.text[prop.key.clone()];
            let decoded = prop.exact_key(&doc.text);
            let key = decoded.as_deref().unwrap_or(raw);
            let value = props::unescape(&doc.text[prop.value.clone()]);
            let locale = props::locale_name(props::suffix(&doc.path));
            let mut markdown = format!(
                "**Property:** {}\n\n**Value:** {}\n\n**Source:** {} · locale {}",
                escaped(key),
                escaped(&value),
                escaped(&doc.path),
                escaped(locale)
            );
            if decoded.is_none() {
                markdown.push_str("\n\nRaw key spelling shown: its UTF-16/Unicode identity is unsupported by the source-reference adapter.");
            } else if raw != key {
                markdown.push_str("\n\nEscaped source spelling; no automatic Rename anchor is published.");
            }
            if markdown.len() > MAX_MARKDOWN {
                let mut end = MAX_MARKDOWN;
                while !markdown.is_char_boundary(end) {
                    end -= 1;
                }
                markdown.truncate(end);
            }
            return Some(Hover::new(markdown).range(props::range_of(&doc.text, &prop.key)));
        }
        let start = doc.text[..offset].rfind('\n').map_or(0, |at| at + 1);
        let end = doc.text[offset..].find('\n').map_or(doc.text.len(), |at| offset + at);
        let line = &doc.text[start..end];
        if line.len() > 4096 {
            return None;
        }
        let source_context =
            if matches!(doc.language.as_str(), "java" | "kotlin") { Some(crate::source::context(doc, offset)?) } else { None };
        let usage = if let Some(context) = &source_context {
            usages::KeyUse { span: context.span.clone(), bundle: Some(context.bundle.clone()) }
        } else {
            if !matches!(doc.language.as_str(), "html" | "jsp") {
                return None;
            }
            let mut usage = usages::markup_uses(line, &doc.path)
                .into_iter()
                .take(128)
                .find(|usage| usage.span.start <= offset - start && offset - start < usage.span.end)?;
            usage.span.start += start;
            usage.span.end += start;
            usage
        };
        let key = &doc.text[usage.span.clone()];
        let root = host::project_of(&doc.path)?.root().ok()?;
        let index = self.index.as_ref().filter(|index| index.root == root)?;
        let ownership = usages::SourceOwnership::current(&root)?;
        if source_context.as_ref().is_some_and(|context| !context.scope_known(index, &ownership)) {
            return None;
        }
        let declarations: Vec<_> = index
            .keys
            .get(key)?
            .iter()
            .filter(|d| source_context.as_ref().is_none_or(|context| context.contains(&d.location.path)))
            .take(128)
            .filter(|declaration| ownership.contains(&declaration.location.path))
            .take(MAX_RESULTS + 1)
            .collect();
        if declarations.is_empty() {
            return None;
        }
        let mut markdown =
            format!("**Property key:** {}\n\nCached candidate declarations (runtime/classpath ownership is not resolved):", escaped(key));
        if let Some(bundle) = usage.bundle.as_deref() {
            markdown.push_str(&format!("\n\nExplicit bundle name (conventional source candidates): {}", escaped(bundle)));
        }
        for declaration in declarations.iter().take(MAX_RESULTS) {
            // Defensive ownership filtering also prevents malformed stale cache
            // records from leaking across sibling project roots.
            if !ownership.contains(&declaration.location.path) {
                continue;
            }
            let line = format!(
                "\n\n• {}:{} · locale {}\n\nValue: {}",
                escaped(&declaration.location.path),
                declaration.location.range.start.line + 1,
                escaped(props::locale_name(props::suffix(&declaration.location.path))),
                escaped(&declaration.value)
            );
            if markdown.len() + line.len() > MAX_MARKDOWN - 256 {
                break;
            }
            markdown.push_str(&line);
        }
        if declarations.len() > MAX_RESULTS {
            markdown.push_str("\n\nMore cached candidates omitted.");
        }
        if declarations.len() > 1 {
            markdown.push_str("\n\nAmbiguous source: this is not a resolved resource-bundle reference.");
        }
        Some(Hover::new(markdown).range(props::range_of(&doc.text, &usage.span)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{Event, FilesChanged, Plugin, mock::MockHost};
    use openidea_plugin::providers::Providers;
    fn doc(path: &str, language: &str, text: &str) -> Document {
        Document { path: path.into(), language: language.into(), version: 9, text: text.into() }
    }
    #[test]
    fn current_buffer_values_and_raw_utf16_spans_work_without_project_io() {
        let _host = MockHost::install().with_capabilities(&[]);
        let source = "# 😀 comment\r\nnaïve.😀=Unstored <img> [link](evil)\r\nescaped\\u002ekey=decoded\\nvalue\r\n";
        let mut plugin = Properties::default();
        let document = doc("/outside/messages.properties", "properties", source);
        let hover = plugin.hover(&document, Position::new(1, 2)).unwrap();
        assert!(hover.markdown.contains("Unstored \\<img\\>"));
        let range = hover.range.unwrap();
        assert_eq!(range.start, Position::new(1, 0));
        assert_eq!(range.end, Position::new(1, 8));
        let span = text::offset(source, range.start).unwrap()..text::offset(source, range.end).unwrap();
        assert_eq!(&source[span], "naïve.😀");
        let escaped = plugin.hover(&document, Position::new(2, 2)).unwrap();
        assert!(escaped.markdown.contains("escaped.key"));
        assert!(escaped.markdown.contains("no automatic Rename anchor"));
        assert_eq!(escaped.range.unwrap().end, Position::new(2, 16));
        assert!(plugin.index.is_none());
    }
    #[test]
    fn code_hover_is_cache_only_bundle_scoped_and_project_owned() {
        let host = MockHost::install();
        host.add_file("/project/src/main/resources/messages.properties", "hello=One\n");
        host.add_file("/project/other/src/main/resources/messages.properties", "hello=Two\n");
        let source = "// 😀 prefix\r\njava.util.ResourceBundle.getBundle(\"messages\").getString(\"hello\");";
        let document = doc("/project/src/main/java/App.java", "java", source);
        let at = text::position(source, source.rfind("hello").unwrap() + 1);
        let mut plugin = Properties::default();
        assert!(plugin.hover(&document, at).is_none());
        assert!(plugin.index.is_none());
        plugin.key_index(&document.path).unwrap();
        let hover = plugin.hover(&document, at).unwrap();
        assert!(hover.markdown.contains("src/main/resources/messages.properties"));
        assert!(hover.markdown.contains("Value: One"));
        assert!(!hover.markdown.contains("Value: Two"));
        let range = hover.range.unwrap();
        assert_eq!(&source[text::offset(source, range.start).unwrap()..text::offset(source, range.end).unwrap()], "hello");
        host.set_project_root(Some("/other"));
        let other = doc("/other/App.java", "java", source);
        assert!(plugin.hover(&other, at).is_none());
        plugin.key_index(&other.path).unwrap();
        assert_eq!(plugin.index.as_ref().unwrap().root, "/other");
        assert!(plugin.hover(&other, at).is_none());
    }
    #[test]
    fn saved_or_deleted_properties_invalidate_candidates_and_work_is_bounded() {
        let host = MockHost::install();
        host.add_file("messages.properties", "hello=Old\n");
        let mut plugin = Properties::default();
        plugin.key_index("/project/App.java").unwrap();
        plugin.handle_events(
            &[Event::FilesChanged(FilesChanged { removed: vec!["/project/messages.properties".into()], ..Default::default() })],
            &host.invocation(),
        );
        assert!(plugin.index.is_none());
        assert!(
            plugin.hover(&doc("/project/messages.properties", "properties", &"x".repeat(MAX_DOCUMENT + 1)), Position::new(0, 0)).is_none()
        );
        let source = format!("{} getString(\"hello\")", "x".repeat(4096));
        assert!(plugin.hover(&doc("/project/App.java", "java", &source), text::position(&source, source.len() - 3)).is_none());
    }

    #[test]
    fn reassigned_bundle_sources_cannot_supply_cached_hover_definition_or_completion() {
        let host = MockHost::install();
        host.add_file("src/main/resources/messages.properties", "hello=Former owner\n");
        let source = "class A { String f(){return java.util.ResourceBundle.getBundle(\"messages\").getString(\"hello\");}}";
        let document = doc("/project/src/main/java/A.java", "java", source);
        let at = text::position(source, source.rfind("hello").unwrap()+1);
        let mut plugin = Properties::default();
        plugin.key_index(&document.path).unwrap();
        assert!(plugin.hover(&document, at).is_some());
        assert_eq!(plugin.definition(&document, at).len(), 1);
        assert_eq!(plugin.completion(&document, at, None).len(), 1);
        host.add_project("/project/src/main/resources");
        assert!(plugin.hover(&document, at).is_none());
        assert!(plugin.definition(&document, at).is_empty());
        assert!(plugin.completion(&document, at, None).is_empty());
    }

    #[test]
    fn markup_candidates_drop_reassigned_sources_but_keep_owned_positive_facts() {
        let host = MockHost::install();
        host.add_file("messages.properties", "hello=Owned\n");
        host.add_file("nested/messages.properties", "hello=Foreign\n");
        let document = doc("/project/page.html", "html", "<p>#{hello}</p>");
        let at = text::position(&document.text, document.text.find("hello").unwrap()+1);
        let mut plugin = Properties::default();
        plugin.key_index(&document.path).unwrap();
        host.add_project("/project/nested");
        let hover = plugin.hover(&document, at).unwrap();
        assert!(hover.markdown.contains("Value: Owned"));
        assert!(!hover.markdown.contains("Foreign"));
        let found = plugin.definition(&document, at);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, "/project/messages.properties");
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn initial_index_excludes_current_nested_project_files() {
        let host = MockHost::install();
        host.add_file("messages.properties", "owned=Here\n");
        host.add_file("nested/messages.properties", "foreign=Elsewhere\n");
        host.add_project("/project/nested");
        let mut plugin = Properties::default();
        let index = plugin.key_index("/project/page.html").unwrap();
        assert!(index.keys.contains_key("owned"));
        assert!(!index.keys.contains_key("foreign"));
    }

    #[test]
    fn candidate_owner_inventory_is_bounded_and_unknown_metadata_stays_quiet() {
        let host = MockHost::install();
        host.add_file("messages.properties", "hello=Owned\n");
        let document = doc("/project/page.html", "html", "#{hello}");
        let at = Position::new(0, 3);
        let mut plugin = Properties::default();
        plugin.key_index(&document.path).unwrap();
        let owner = usages::SourceOwnership::current("/project").unwrap();
        for path in ["/project-other/messages.properties", "/project/../other/messages.properties", "/project//messages.properties", "/project/a\n.properties"] {
            assert!(!owner.contains(path), "{path}");
        }
        assert!(plugin.hover(&document, at).is_some());
        for i in 0..64 { host.add_project(&format!("/other{i}")); }
        assert!(plugin.hover(&document, at).is_none());
        assert!(plugin.definition(&document, at).is_empty());
        let _host = host.with_capabilities(&[]);
        assert!(plugin.hover(&document, at).is_none());
        assert!(plugin.definition(&document, at).is_empty());
    }
    #[test]
    fn decoded_reference_facts_require_lossless_scalar_keys() {
        let host = MockHost::install();
        host.add_file("src/main/resources/messages.properties", "bad\\uD800=Wrong\nbad�=Right\n\\uD83D\\uDE00=Paired\n");
        let source = "class A {String f(){return java.util.ResourceBundle.getBundle(\"messages\").getString(\"bad�\");}}";
        let document = doc("/project/src/main/java/A.java", "java", source);
        let at = text::position(source, source.rfind("bad�").unwrap()+1);
        let mut plugin = Properties::default();
        plugin.key_index(&document.path).unwrap();
        let found = plugin.definition(&document, at);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].range.start.line, 1);
        let hover = plugin.hover(&document, at).unwrap();
        assert!(hover.markdown.contains("Right") && !hover.markdown.contains("Wrong"));
        let paired = Document {text:source.replace("bad�", "😀"), ..document};
        assert_eq!(plugin.definition(&paired, text::position(&paired.text, paired.text.rfind("😀").unwrap())).len(), 1);
        assert!(plugin.inspect("/project/src/main/resources/messages.properties").is_err());
    }
}
