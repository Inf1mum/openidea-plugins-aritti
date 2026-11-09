//! Hover uses only an existing project cache and the current env buffer. Never
//! call `index()` here: its cold path probes env files synchronously.
use super::*;
use openidea_plugin::hover::{Hover, HoverProvider};
const MAX_TEXT: usize = 256 << 10;
const MAX_CACHE: usize = 8192;
const MAX_MATCHES: usize = 32;
const MAX_OUTPUT: usize = 16 << 10;
const FOOTER_RESERVE: usize = 512;

fn inline(value: &str, limit: usize) -> String {
    let mut cleaned = String::new();
    let mut count = 0;
    for c in value.chars() {
        if count == limit {
            cleaned.push('…');
            break;
        }
        if c.is_control() {
            cleaned.extend(c.escape_default());
        } else {
            cleaned.push(c);
        }
        count += 1;
    }
    let mut longest = 0;
    let mut run = 0;
    for c in cleaned.chars() {
        if c == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    let fence = "`".repeat(longest + 1);
    format!("{fence} {cleaned} {fence}")
}

fn holds(range: Range, at: Position) -> bool {
    (range.start.line, range.start.character) <= (at.line, at.character)
        && (at.line, at.character) <= (range.end.line, range.end.character)
}

impl HoverProvider for DotEnv {
    fn hover(&mut self, doc: &Document, at: Position) -> Option<Hover> {
        if doc.text.len() > MAX_TEXT {
            return None;
        }
        let offset = text::offset(&doc.text, at)?;
        if line_of(&doc.text, offset).1.len() > 2000 {
            return None;
        }
        let local = if doc.language == "dotenv" {
            env_declarations(&doc.path, &doc.text)
        } else {
            Vec::new()
        };
        let (key, span) =
            if let Some(declaration) = local.iter().find(|d| holds(d.location.range, at)) {
                (declaration.key.clone(), declaration.location.range)
            } else {
                let key = key_at(doc, at)?;
                (doc.text[key.clone()].to_owned(), range(&doc.text, key))
            };
        if key.len() > 256 {
            return None;
        }
        let root = host::project_of(&doc.path).and_then(|project| project.root().ok());
        let index = root.as_ref().and_then(|root| self.projects.get(root));
        let mut candidates = Vec::new();
        let mut omitted = false;
        for declaration in local.iter().chain(
            index
                .into_iter()
                .flat_map(|index| index.declarations.iter().take(MAX_CACHE))
                .filter(|d| doc.language != "dotenv" || d.location.path != doc.path),
        ) {
            if declaration.key != key {
                continue;
            }
            if candidates.len() == MAX_MATCHES {
                omitted = true;
                break;
            }
            // Only source and optional value reach Markdown, never source text.
            candidates.push(declaration);
        }
        if candidates.is_empty() {
            return None;
        }
        candidates.sort_by(|a, b| {
            (
                &a.location.path,
                a.location.range.start.line,
                a.location.range.start.character,
            )
                .cmp(&(
                    &b.location.path,
                    b.location.range.start.line,
                    b.location.range.start.character,
                ))
        });
        let mut markdown = format!("**Environment variable** {}\n\n", inline(&key, 256));
        if candidates.len() > 1 || omitted {
            markdown.push_str(
                "Multiple declaration candidates; runtime precedence is not resolved.\n\n",
            );
        }
        let show_values = host::setting_bool("showValues").unwrap_or(true);
        for declaration in candidates {
            let path = root
                .as_deref()
                .and_then(|root| {
                    declaration
                        .location
                        .path
                        .strip_prefix(&format!("{}/", root.trim_end_matches('/')))
                })
                .unwrap_or(&declaration.location.path);
            let kind = if is_env_file(&declaration.location.path) {
                "env file"
            } else if index::file_name(&declaration.location.path)
                .to_ascii_lowercase()
                .contains("dockerfile")
            {
                "Dockerfile"
            } else {
                "Compose file"
            };
            let mut entry = format!(
                "- {}:{}:{} ({kind})",
                inline(path, 1024),
                declaration.location.range.start.line + 1,
                declaration.location.range.start.character + 1
            );
            if show_values {
                entry.push_str(&format!(" — {}", inline(&declaration.value, 256)));
            }
            entry.push('\n');
            if markdown
                .len()
                .saturating_add(entry.len())
                .saturating_add(FOOTER_RESERVE)
                > MAX_OUTPUT
            {
                omitted = true;
                break;
            }
            markdown.push_str(&entry);
        }
        if !show_values {
            markdown.push_str("\nValues hidden by showValues.\n");
        }
        if index.is_none_or(|index| !index.complete) {
            markdown.push_str("\nSource cache is incomplete; only known declarations are shown.\n");
        }
        if omitted || index.is_some_and(|index| index.declarations.len() > MAX_CACHE) {
            markdown.push_str(
                "\nHover results are bounded; additional cached declarations may be omitted.\n",
            );
        }
        markdown.push_str("\nCached source facts; runtime environment values are not evaluated.\n");
        Some(Hover::new(markdown).range(span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn code_doc() -> Document {
        Document {
            path: "/project/app.js".into(),
            language: "javascript".into(),
            version: 1,
            text: "process.env.TOKEN".into(),
        }
    }
    fn cached(host: &MockHost) -> DotEnv {
        host.add_file(".env", "TOKEN=private-live-value\n");
        host.add_file(".env.example", "TOKEN=private-example-value\n");
        let mut plugin = DotEnv::default();
        plugin.index_projects();
        plugin
    }
    #[test]
    fn disabled_values_never_disclose_cached_or_current_buffer_secrets() {
        let host = MockHost::install();
        let mut plugin = cached(&host);
        host.set_setting("showValues", "false");
        let hover = plugin.hover(&code_doc(), Position::new(0, 13)).unwrap();
        assert!(hover.markdown.contains("Multiple declaration candidates"));
        assert!(hover.markdown.contains(".env"));
        assert!(
            !hover.markdown.contains("private-live-value")
                && !hover.markdown.contains("private-example-value")
        );
        let doc = Document {
            path: "/project/.env".into(),
            language: "dotenv".into(),
            version: 2,
            text: "TOKEN=unsaved-secret\n".into(),
        };
        let hover = plugin.hover(&doc, Position::new(0, 2)).unwrap();
        assert!(!hover.markdown.contains("unsaved-secret"));
        assert!(hover.markdown.contains("Values hidden"));
    }
    #[test]
    fn enabled_values_and_source_ranges_are_bounded_and_unambiguous_about_precedence() {
        let host = MockHost::install();
        let mut plugin = cached(&host);
        host.set_setting("showValues", "true");
        let hover = plugin.hover(&code_doc(), Position::new(0, 13)).unwrap();
        assert!(
            hover.markdown.contains("private-live-value")
                && hover.markdown.contains("private-example-value")
        );
        assert!(
            hover
                .markdown
                .contains("runtime precedence is not resolved")
        );
        assert_eq!(
            hover.range,
            Some(Range::new(Position::new(0, 12), Position::new(0, 17)))
        );
    }
    #[test]
    fn hover_never_warms_a_cold_index_or_reads_newly_added_declarations() {
        let host = MockHost::install();
        let mut plugin = DotEnv::default();
        host.add_file(".env", "TOKEN=must-not-read\n");
        assert!(plugin.hover(&code_doc(), Position::new(0, 13)).is_none());
        assert!(plugin.projects.is_empty());
        plugin.index_projects();
        host.add_file(".env", "TOKEN=new-unindexed-value\n");
        let hover = plugin.hover(&code_doc(), Position::new(0, 13)).unwrap();
        assert!(hover.markdown.contains("must-not-read"));
        assert!(!hover.markdown.contains("new-unindexed-value"));
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn project_switch_and_permission_denial_cannot_reuse_another_projects_values() {
        let host = MockHost::install();
        let mut plugin = cached(&host);
        host.set_project_root(Some("/other"));
        let mut doc = code_doc();
        doc.path = "/other/app.js".into();
        assert!(plugin.hover(&doc, Position::new(0, 13)).is_none());
        let _host = host.with_capabilities(&[]);
        assert!(plugin.hover(&code_doc(), Position::new(0, 13)).is_none());
    }
    #[test]
    fn current_env_buffer_overlays_saved_declaration_and_unicode_ranges() {
        let host = MockHost::install();
        let mut plugin = cached(&host);
        let doc = Document {
            path: "/project/.env".into(),
            language: "dotenv".into(),
            version: 2,
            text: "PREFIX=😀${TOKEN}\nTOKEN=unsaved\n".into(),
        };
        let hover = plugin.hover(&doc, Position::new(0, 12)).unwrap();
        assert!(hover.markdown.contains("unsaved"));
        assert!(!hover.markdown.contains("private-live-value"));
        assert_eq!(
            hover.range,
            Some(Range::new(Position::new(0, 11), Position::new(0, 16)))
        );
        assert!(inline("`[bad](https://invalid)\n", 128).starts_with("`` "));
    }
    #[test]
    fn response_and_document_work_are_bounded() {
        let host = MockHost::install();
        let mut plugin = cached(&host);
        let index = plugin.projects.get_mut("/project").unwrap();
        let declaration = index.declarations[0].clone();
        index.declarations = (0..40)
            .map(|n| {
                let mut d = declaration.clone();
                d.location.path = format!("/project/.env.{n}");
                d.value = "s".repeat(4096);
                d
            })
            .collect();
        let hover = plugin.hover(&code_doc(), Position::new(0, 13)).unwrap();
        assert!(
            hover
                .markdown
                .contains("additional cached declarations may be omitted")
        );
        assert!(!hover.markdown.contains(&"s".repeat(257)));
        assert!(hover.markdown.len() < MAX_OUTPUT);
        let index = plugin.projects.get_mut("/project").unwrap();
        for (n, declaration) in index.declarations.iter_mut().enumerate() {
            declaration.location.path = format!("/project/{}-{n}/.env", "😀".repeat(1500));
            declaration.value = "\u{1b}".repeat(300);
        }
        let bounded = plugin.hover(&code_doc(), Position::new(0, 13)).unwrap();
        assert!(
            bounded.markdown.len() <= MAX_OUTPUT,
            "actual UTF8 bytes, including code fences and footers"
        );
        assert!(
            bounded
                .markdown
                .contains("additional cached declarations may be omitted")
        );
        assert!(
            bounded
                .markdown
                .ends_with("Cached source facts; runtime environment values are not evaluated.\n")
        );

        let mut long_key = code_doc();
        long_key.language = "dotenv".into();
        long_key.text = format!("{}=value", "A".repeat(257));
        assert!(plugin.hover(&long_key, Position::new(0, 1)).is_none());
        let mut doc = code_doc();
        doc.text = "x".repeat(MAX_TEXT + 1);
        assert!(plugin.hover(&doc, Position::new(0, 0)).is_none());
        assert!(host.process_runs().is_empty());
    }
}
