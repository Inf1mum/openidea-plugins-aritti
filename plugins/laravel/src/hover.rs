//! Positive cached source facts only; never reads files or starts discovery in a hover.
use crate::*;
use openidea_plugin::hover::{Hover, HoverProvider};

pub(super) fn canonical(path: &str) -> bool {
    path.starts_with('/')
        && !path.contains(['\\', '\0'])
        && path
            .split('/')
            .skip(1)
            .all(|part| !matches!(part, "" | "." | ".."))
}

fn escape(value: &str) -> String {
    let mut result = String::new();
    let mut chars = value.chars();
    for c in chars.by_ref().take(512) {
        if c.is_control() {
            result.push(' ');
        } else {
            if "\\`*_{}[]<>#|!".contains(c) {
                result.push('\\');
            }
            result.push(c);
        }
    }
    if chars.next().is_some() {
        result.push('…');
    }
    result
}

impl HoverProvider for Main {
    fn hover(&mut self, doc: &Document, at: Position) -> Option<Hover> {
        self.advance_document_index(&doc.path);
        if doc.text.len() > 128 * 1024
            || !canonical(&doc.path)
            || [
                "/vendor/",
                "/storage/",
                "/bootstrap/cache/",
                "/node_modules/",
            ]
            .iter()
            .any(|part| doc.path.contains(part))
        {
            return None;
        }
        let language = language_of(&doc.path, &doc.language)?;
        let offset = text::offset(&doc.text, at)?;
        let reference = refs::reference_at(language, &doc.text, offset)?;
        if !reference.closed || offset < reference.start || offset >= reference.end {
            return None;
        }
        let name = doc.text.get(reference.start..reference.end)?;
        // Refuse interpolation, escapes and expression fragments instead of documenting a prefix.
        if name.is_empty() || name.len() > 512 || name.contains(['\\', '$', '\n', '\r']) {
            return None;
        }
        let quote = *doc.text.as_bytes().get(reference.start.checked_sub(1)?)?;
        if !doc
            .text
            .get(..reference.start - 1)?
            .trim_end()
            .ends_with(['(', ','])
        {
            return None;
        }
        if !matches!(quote, b'\'' | b'"')
            || doc.text.as_bytes().get(reference.end) != Some(&quote)
            || !doc
                .text
                .get(reference.end + 1..)?
                .trim_start()
                .starts_with([',', ')', ']'])
        {
            return None;
        }

        let roots: Vec<String> = host::projects()
            .ok()?
            .into_iter()
            .filter_map(|project| project.root().ok())
            .filter(|root| canonical(root))
            .collect();
        let owner = |path: &str| {
            roots
                .iter()
                .filter(|root| under(path, root))
                .max_by_key(|root| root.len())
        };
        let root = owner(&doc.path)?;
        let index = self.indexes.get(root)?;
        let (label, names, documentation) = match reference.kind {
            Kind::Route => (
                crate::ui_text::text("final.hover.kind.route", "named route"),
                &index.routes,
                "urls#urls-for-named-routes",
            ),
            Kind::View => (
                crate::ui_text::text("final.hover.kind.view", "view"),
                &index.views,
                "views#creating-and-rendering-views",
            ),
            Kind::Config => (
                crate::ui_text::text("final.hover.kind.config", "configuration key"),
                &index.config,
                "configuration#accessing-configuration-values",
            ),
            Kind::Translation => (
                crate::ui_text::text("final.hover.kind.translation", "translation key"),
                &index.translations,
                "localization#retrieving-translation-strings",
            ),
            _ => return None,
        };
        let entry = names.get(name)?;
        if !canonical(&entry.path) || entry.path == doc.path || owner(&entry.path) != Some(root) {
            return None;
        }
        let relative = entry.path.strip_prefix(root)?.strip_prefix('/')?;
        let mut markdown = crate::ui_text::format(
            "final.hover.header",
            "**Laravel {label}** {name}\n\nCached source: {path}:{line}",
            &[
                ("label", label),
                ("name", &escape(name)),
                ("path", &escape(relative)),
                ("line", &entry.at.line.saturating_add(1).to_string()),
            ],
        );
        let displayed_detail=if reference.kind==Kind::View {crate::package_integrations::presentation_detail(entry.detail.clone())}else{entry.detail.clone()};
        if let Some(detail) = displayed_detail.as_deref().filter(|detail| !detail.is_empty()) {
            markdown.push_str(&format!("\n\n{}", escape(detail)));
        }
        markdown.push_str(crate::ui_text::text("final.hover.sourceNote","\n\nSource index refreshed on relevant saved files or Reindex. Unsaved target changes, duplicate registrations and runtime overrides are not resolved."));
        if index.partial {
            markdown.push_str(crate::ui_text::text(
                "final.hover.partialNote",
                " This inventory is partial; only this recorded positive match is shown.",
            ));
        }
        markdown.push_str(&crate::ui_text::format(
            "final.hover.documentationLink",
            "\n\n[Laravel documentation](https://laravel.com/docs/12.x/{page})",
            &[("page", documentation)],
        ));
        Some(Hover::new(markdown).range(range_of(&doc.text, reference.start, reference.end)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{activated, cursor, doc};
    use openidea_plugin::mock::MockHost;

    #[test]
    fn cached_hover_describes_four_literal_reference_kinds_and_utf16_range() {
        let host = MockHost::install();
        let mut plugin = activated(&host);
        for (marked, label, detail) in [
            (
                "<?php /* 😀 */ route('ho|me');",
                "named route",
                "GET\\|HEAD /",
            ),
            (
                "{{ view('posts.in|dex') }}",
                "view",
                "resources/views/posts/index.blade.php",
            ),
            (
                "{{ config('app.na|me') }}",
                "configuration key",
                "config/app.php",
            ),
            (
                "{{ __('messages.wel|come') }}",
                "translation key",
                "Welcome",
            ),
        ] {
            let (source, at) = cursor(marked);
            let (path, language) = if source.starts_with("<?php") {
                ("/project/test.php", "php")
            } else {
                ("/project/resources/views/test.blade.php", "blade")
            };
            let d = doc(path, language, &source);
            let result = plugin.hover(&d, at).unwrap_or_else(|| panic!("{marked}"));
            assert!(
                result.markdown.contains(label) && result.markdown.contains(detail),
                "{}",
                result.markdown
            );
            let reference =
                refs::reference_at(language, &source, text::offset(&source, at).unwrap()).unwrap();
            assert_eq!(
                result.range,
                Some(range_of(&source, reference.start, reference.end))
            );
            assert!(
                result.markdown.contains("Cached source:")
                    && result.markdown.contains("runtime overrides")
            );
        }
    }

    #[test]
    fn cached_hover_uses_unsaved_reference_text_and_refuses_dynamic_or_unknown_names() {
        let host = MockHost::install();
        let mut plugin = activated(&host);
        for marked in [
            "<?php route('miss|ing');",
            "<?php route('ho|me' . $suffix);",
            "<?php route($prefix . 'ho|me');",
            "<?php route('ho|me",
            "<?php route(\"ho|me$suffix\");",
            "<?php route('ho|me\\n');",
            "<?php $x = 'ho|me';",
            "<?php env('APP_NA|ME');",
        ] {
            let (source, at) = cursor(marked);
            assert!(
                plugin
                    .hover(&doc("/project/test.php", "php", &source), at)
                    .is_none(),
                "{marked}"
            );
        }
        let (source, at) = cursor("<?php route('posts.sh|ow');");
        assert!(
            plugin
                .hover(&doc("/project/test.php", "php", &source), at)
                .unwrap()
                .markdown
                .contains("posts.show")
        );
    }

    #[test]
    fn cached_hover_requires_current_innermost_project_and_target_ownership() {
        let host = MockHost::install();
        let mut plugin = activated(&host);
        let (source, at) = cursor("<?php route('ho|me');");
        for path in [
            "/project/../foreign/test.php",
            "/project/vendor/pkg/test.php",
            "/project//test.php",
            "/project/routes/web.php",
            "/project-other/test.php",
        ] {
            assert!(
                plugin.hover(&doc(path, "php", &source), at).is_none(),
                "{path}"
            );
        }
        host.add_project("/project/nested");
        assert!(
            plugin
                .hover(&doc("/project/nested/test.php", "php", &source), at)
                .is_none()
        );
        plugin
            .indexes
            .get_mut("/project")
            .unwrap()
            .routes
            .get_mut("home")
            .unwrap()
            .path = "/project/nested/routes/web.php".into();
        assert!(
            plugin
                .hover(&doc("/project/test.php", "php", &source), at)
                .is_none()
        );
        host.set_project_root(None);
        assert!(
            plugin
                .hover(&doc("/project/test.php", "php", &source), at)
                .is_none()
        );
    }

    #[test]
    fn cached_hover_is_bounded_escaped_and_explicit_about_partial_inventory() {
        let host = MockHost::install();
        let mut plugin = activated(&host);
        let index = plugin.indexes.get_mut("/project").unwrap();
        index.partial = true;
        index.routes.get_mut("home").unwrap().detail = Some(format!(
            "[unsafe](javascript:bad) <script> `code`\n{}",
            "x".repeat(2000)
        ));
        let (source, at) = cursor("<?php route('ho|me');");
        let result = plugin
            .hover(&doc("/project/test.php", "php", &source), at)
            .unwrap();
        assert!(
            result.markdown.contains("inventory is partial")
                && result.markdown.contains("\\[unsafe\\]")
                && result.markdown.contains("\\<script\\>")
        );
        assert!(result.markdown.len() < 1500);
        assert!(
            plugin
                .hover(
                    &doc(
                        "/project/test.php",
                        "php",
                        &format!("{source}{}", " ".repeat(128 * 1024))
                    ),
                    at
                )
                .is_none()
        );
        assert!(
            plugin
                .hover(
                    &doc("/project/test.php", "php", &source),
                    Position::new(99, 99)
                )
                .is_none()
        );
    }

    #[test]
    fn cached_hover_does_not_read_sources_run_processes_or_publish_manual_diagnostics() {
        let host = MockHost::install();
        let mut plugin = activated(&host);
        // Remove all files from relevance by changing their contents after the cache was built.
        host.add_file("routes/web.php", "<?php /* no routes */");
        let (source, at) = cursor("<?php route('ho|me');");
        assert!(
            plugin
                .hover(&doc("/project/test.php", "php", &source), at)
                .unwrap()
                .markdown
                .contains("Cached source:")
        );
        assert!(
            host.process_runs().is_empty()
                && host.requests().is_empty()
                && host.diagnostics().is_empty()
        );
        let host = host.with_capabilities(&[]);
        assert!(
            plugin
                .hover(&doc("/project/test.php", "php", &source), at)
                .is_none()
        );
        drop(host);
    }

    #[test]
    fn cached_hover_changes_only_after_saved_source_refresh_and_preserves_php_blade_fallback() {
        let host = MockHost::install();
        let mut plugin = activated(&host);
        let (source, at) = cursor("{{ route('ho|me') }}");
        let d = doc("/project/resources/views/test.blade.php", "php", &source);
        assert!(plugin.hover(&d, at).unwrap().markdown.contains("Closure"));
        host.add_file(
            "routes/web.php",
            "<?php Route::post('/new', 'NewController@store')->name('home');",
        );
        assert!(plugin.hover(&d, at).unwrap().markdown.contains("Closure"));
        plugin.handle_events(
            &[Event::FileSaved("/project/routes/web.php".into())],
            &host.invocation(),
        );
        crate::tests::finish_indexing(&mut plugin);
        assert!(plugin.hover(&d, at).unwrap().markdown.contains("POST /new"));
        host.add_file("routes/web.php", "<?php /* route removed */");
        plugin.handle_events(
            &[Event::FileSaved("/project/routes/web.php".into())],
            &host.invocation(),
        );
        crate::tests::finish_indexing(&mut plugin);
        assert!(plugin.hover(&d, at).is_none());
    }
    #[test]
    fn registered_hover_does_not_attribute_explicit_foreign_namespaced_helpers_to_laravel() {
        let host = MockHost::install();
        let mut plugin = activated(&host);
        for marked in [
            "<?php Vendor\\view('posts.in|dex');",
            "<?php \\Vendor\\config('app.na|me');",
            "<?php namespace\\route('ho|me');",
            "<?php Vendor\\__('messages.wel|come');",
        ] {
            let (source, at) = cursor(marked);
            assert!(
                plugin
                    .hover(&doc("/project/test.php", "php", &source), at)
                    .is_none(),
                "{marked}"
            );
        }
    }

    #[test]
    fn registered_hover_refuses_explicit_foreign_facade_classes_despite_matching_short_names() {
        let host = MockHost::install();
        let mut plugin = activated(&host);
        let observed: Vec<_> = [
            "<?php \\Vendor\\View::make('posts.in|dex');",
            "<?php \\Vendor\\Config::get('app.na|me');",
            "<?php \\Vendor\\Route::has('ho|me');",
            "<?php \\Vendor\\Lang::get('messages.wel|come');",
        ]
        .into_iter()
        .map(|marked| {
            let (source, at) = cursor(marked);
            (
                marked,
                plugin
                    .hover(&doc("/project/test.php", "php", &source), at)
                    .is_some(),
            )
        })
        .collect();
        assert!(observed.iter().all(|(_, found)| !*found), "{observed:?}");
    }

    #[test]
    fn registered_hover_does_not_read_blade_directives_from_raw_php_data_or_comments() {
        let host = MockHost::install();
        let mut plugin = activated(&host);
        let observed: Vec<_> = [
            r#"<?php $x = "@include('posts.in|dex')"; ?>"#,
            r#"<?php $x = "?> @include('posts.in|dex')"; ?>"#,
            r#"<?php /* ?> @include('posts.in|dex') */ ?>"#,
        ]
        .into_iter()
        .map(|marked| {
            let (source, at) = cursor(marked);
            (
                marked,
                plugin
                    .hover(
                        &doc("/project/resources/views/test.blade.php", "blade", &source),
                        at,
                    )
                    .is_some(),
            )
        })
        .collect();
        assert!(observed.iter().all(|(_, found)| !*found), "{observed:?}");
    }

    #[test]
    fn registered_hover_after_real_php_close_preserves_original_utf16_range_and_line_comment_rules()
    {
        let host = MockHost::install();
        let mut plugin = activated(&host);
        for marked in [
            r#"<?php $x="?> @include('fake')"; ?> 😀 @include('posts.in|dex')"#,
            r#"<?php /* ?> @include('fake') */ ?> @include('posts.in|dex')"#,
            "<?php // comment ?> @include('posts.in|dex')",
            "<?php # comment ?> @include('posts.in|dex')",
        ] {
            let (source, at) = cursor(marked);
            let found = plugin
                .hover(
                    &doc("/project/resources/views/test.blade.php", "blade", &source),
                    at,
                )
                .unwrap_or_else(|| panic!("{marked}"));
            let start = source.find("posts.index").unwrap();
            assert_eq!(
                found.range,
                Some(range_of(&source, start, start + "posts.index".len()))
            );
            assert!(found.markdown.contains("posts/index.blade.php"));
        }
    }

    #[test]
    fn registered_hover_ignores_php_argument_comment_directives_instead_of_inventing_outer_regions()
    {
        let host = MockHost::install();
        let mut plugin = activated(&host);
        let observed: Vec<_> = [
            "@if(/* ) @include('posts.in|dex') */ $real) @endif",
            "@if(// ) @include('posts.in|dex')\n$real) @endif",
            "@if(# ) @include('posts.in|dex')\n$real) @endif",
        ]
        .into_iter()
        .map(|marked| {
            let (source, at) = cursor(marked);
            (
                marked,
                plugin
                    .hover(
                        &doc("/project/resources/views/test.blade.php", "blade", &source),
                        at,
                    )
                    .is_some(),
            )
        })
        .collect();
        assert!(observed.iter().all(|(_, v)| !*v), "{observed:?}");
    }
}
