//! Blade directive names: Laravel's and Livewire's own, and the position of
//! the name being typed after an `@`.

/// The directives `aritti.blade` knows; a test keeps the two lists equal.
// ponytail: the plugins are separate Cargo projects, so the list is copied, not shared.
pub const CORE: &[&str] = &[
    "if",
    "endif",
    "hasSection",
    "sectionMissing",
    "unless",
    "endunless",
    "isset",
    "endisset",
    "empty",
    "endempty",
    "auth",
    "endauth",
    "guest",
    "endguest",
    "production",
    "endproduction",
    "env",
    "endenv",
    "section",
    "endsection",
    "show",
    "stop",
    "append",
    "overwrite",
    "push",
    "endpush",
    "pushIf",
    "endPushIf",
    "prepend",
    "endprepend",
    "pushOnce",
    "endPushOnce",
    "prependOnce",
    "endPrependOnce",
    "once",
    "endonce",
    "for",
    "endfor",
    "foreach",
    "endforeach",
    "forelse",
    "endforelse",
    "while",
    "endwhile",
    "switch",
    "endswitch",
    "php",
    "endphp",
    "verbatim",
    "endverbatim",
    "error",
    "enderror",
    "component",
    "endcomponent",
    "slot",
    "endslot",
    "can",
    "endcan",
    "cannot",
    "endcannot",
    "canany",
    "endcanany",
    "session",
    "endsession",
    "fragment",
    "endfragment",
    "persist",
    "endpersist",
    "teleport",
    "endteleport",
    "script",
    "endscript",
    "assets",
    "endassets",
    "else",
    "elseif",
    "elsecan",
    "elsecannot",
    "elsecanany",
    "extends",
    "yield",
    "parent",
    "include",
    "includeIf",
    "includeWhen",
    "includeUnless",
    "includeFirst",
    "each",
    "stack",
    "continue",
    "break",
    "case",
    "default",
    "props",
    "aware",
    "class",
    "style",
    "checked",
    "selected",
    "disabled",
    "readonly",
    "required",
    "csrf",
    "method",
    "json",
    "js",
    "lang",
    "choice",
    "use",
    "inject",
    "vite",
    "viteReactRefresh",
    "dd",
    "dump",
    "livewire",
    "livewireStyles",
    "livewireScripts",
    "livewireScriptConfig",
    "bool",
    "unlessenv",
];

/// `(directive, insert text)` in snippet syntax, the `@` excluded, a literal `$` written `\$` in snippet syntax.
pub const SNIPPETS: &[(&str, &str)] = &[
    ("if", "if(${1:condition})\n    $0\n@endif"),
    ("unless", "unless(${1:condition})\n    $0\n@endunless"),
    ("isset", "isset(${1:\\$variable})\n    $0\n@endisset"),
    ("empty", "empty(${1:\\$variable})\n    $0\n@endempty"),
    ("auth", "auth\n    $0\n@endauth"),
    ("guest", "guest\n    $0\n@endguest"),
    ("env", "env('${1:local}')\n    $0\n@endenv"),
    ("production", "production\n    $0\n@endproduction"),
    ("section", "section('${1:name}')\n    $0\n@endsection"),
    ("push", "push('${1:stack}')\n    $0\n@endpush"),
    ("pushOnce", "pushOnce('${1:stack}')\n    $0\n@endPushOnce"),
    ("prepend", "prepend('${1:stack}')\n    $0\n@endprepend"),
    ("for", "for(${1:\\$i = 0}; ${2:\\$i < 10}; ${3:\\$i++})\n    $0\n@endfor"),
    ("foreach", "foreach(${1:\\$items} as ${2:\\$item})\n    $0\n@endforeach"),
    ("forelse", "forelse(${1:\\$items} as ${2:\\$item})\n    $0\n@empty\n    ${3:empty}\n@endforelse"),
    ("while", "while(${1:condition})\n    $0\n@endwhile"),
    ("switch", "switch(${1:\\$value})\n    $0\n@endswitch"),
    ("can", "can('${1:ability}')\n    $0\n@endcan"),
    ("cannot", "cannot('${1:ability}')\n    $0\n@endcannot"),
    ("canany", "canany(['${1:ability}'])\n    $0\n@endcanany"),
    ("error", "error('${1:field}')\n    $0\n@enderror"),
    ("component", "component('${1:name}')\n    $0\n@endcomponent"),
    ("slot", "slot('${1:name}')\n    $0\n@endslot"),
    ("php", "php\n    $0\n@endphp"),
    ("verbatim", "verbatim\n    $0\n@endverbatim"),
    ("extends", "extends('${1:layout}')"),
    ("yield", "yield('${1:section}')"),
    ("include", "include('${1:view}')"),
    ("includeIf", "includeIf('${1:view}')"),
    ("includeWhen", "includeWhen(${1:condition}, '${2:view}')"),
    ("includeUnless", "includeUnless(${1:condition}, '${2:view}')"),
    ("includeFirst", "includeFirst(['${1:first}', '${2:second}'])"),
    ("each", "each('${1:view}', ${2:\\$items}, '${3:item}')"),
    ("stack", "stack('${1:stack}')"),
    ("props", "props(['${1:name}'])"),
    ("class", "class(['${1:class}'])"),
    ("style", "style(['${1:style}'])"),
    ("method", "method('${1:PUT}')"),
    ("json", "json(${1:\\$data})"),
    ("vite", "vite(['${1:resources/css/app.css}', '${2:resources/js/app.js}'])"),
    ("lang", "lang('${1:key}')"),
    ("inject", "inject('${1:name}', '${2:Service}')"),
    ("livewire", "livewire('${1:component}')"),
    ("csrf", "csrf"),
];

/// Byte range of the directive name being typed, the `@` excluded, when the
/// cursor sits right after `@name`.
pub fn name_at(text: &str, offset: usize) -> Option<(usize, usize)> {
    let before = text.get(..offset)?;
    let name_length = before.bytes().rev().take_while(u8::is_ascii_alphabetic).count();
    let start = offset - name_length;
    let at = start.checked_sub(1).filter(|i| text.as_bytes()[*i] == b'@')?;

    let previous = text[..at].chars().next_back();
    if previous.is_some_and(|c| c == '@' || c == '_' || c == '.' || c.is_alphanumeric()) {
        return None;
    }

    let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let line = &text[line_start..at];
    let open = ["{{", "{!!"].iter().filter_map(|o| line.rfind(o)).max();
    let close = ["}}", "!!}"].iter().filter_map(|c| line.rfind(c)).max();
    if open.is_some_and(|o| close.is_none_or(|c| c < o)) {
        return None;
    }

    let end = offset + text[offset..].bytes().take_while(u8::is_ascii_alphabetic).count();
    Some((start, end))
}

// Same established block/inline conventions as the Blade lexer; separate Cargo projects.
const BLOCKS: &[(&str, &[&str])] = &[
    ("if", &["endif"]),
    ("hasSection", &["endif"]),
    ("sectionMissing", &["endif"]),
    ("unless", &["endunless"]),
    ("isset", &["endisset"]),
    ("empty", &["endempty"]),
    ("auth", &["endauth"]),
    ("guest", &["endguest"]),
    ("production", &["endproduction"]),
    ("env", &["endenv"]),
    ("section", &["endsection", "show", "stop", "append", "overwrite"]),
    ("push", &["endpush"]),
    ("pushIf", &["endPushIf"]),
    ("prepend", &["endprepend"]),
    ("pushOnce", &["endPushOnce"]),
    ("prependOnce", &["endPrependOnce"]),
    ("once", &["endonce"]),
    ("for", &["endfor"]),
    ("foreach", &["endforeach"]),
    ("forelse", &["endforelse"]),
    ("while", &["endwhile"]),
    ("switch", &["endswitch"]),
    ("php", &["endphp"]),
    ("verbatim", &["endverbatim"]),
    ("error", &["enderror"]),
    ("component", &["endcomponent"]),
    ("slot", &["endslot"]),
    ("can", &["endcan"]),
    ("cannot", &["endcannot"]),
    ("canany", &["endcanany"]),
    ("session", &["endsession"]),
    ("fragment", &["endfragment"]),
    ("persist", &["endpersist"]),
    ("teleport", &["endteleport"]),
    ("script", &["endscript"]),
    ("assets", &["endassets"]),
];

/// Native completion ranking is represented with stable sort_text values. No
/// project access or runtime execution is needed to interpret the current view.
pub struct Ranking {
    prefix: String,
    preferred: Vec<String>,
    repair_opening: Vec<String>,
}
impl Ranking {
    pub fn repairs_opening(&self, name: &str) -> bool {
        self.repair_opening.iter().any(|candidate| candidate == name)
    }
    pub fn key(&self, name: &str, is_core: bool) -> String {
        let folded = name.to_lowercase();
        let prefix = self.prefix.to_lowercase();
        let matching = if name.starts_with(&self.prefix) {
            0
        } else if folded.starts_with(&prefix) {
            1
        } else if folded.contains(&prefix) {
            2
        } else {
            3
        };
        let context = self.preferred.iter().position(|candidate| candidate == name).unwrap_or(9999);
        format!("{matching}:{context:04}:{}:{folded}:{name}", if is_core { 1 } else { 0 })
    }
}
fn after(text: &str, from: usize, needle: &str) -> usize {
    text[from..].find(needle).map_or(text.len(), |i| from + i + needle.len())
}
fn php_end(text: &str, mut at: usize, delimiter: &str) -> usize {
    let bytes = text.as_bytes();
    while at < bytes.len() {
        if bytes[at..].starts_with(delimiter.as_bytes()) {
            return at + delimiter.len();
        }
        if matches!(bytes[at], b'\'' | b'"') {
            let quote = bytes[at];
            at += 1;
            while at < bytes.len() {
                if bytes[at] == b'\\' {
                    at = (at + 2).min(bytes.len());
                } else if bytes[at] == quote {
                    at += 1;
                    break;
                } else {
                    at += 1;
                }
            }
        } else if bytes[at..].starts_with(b"//") || bytes[at] == b'#' {
            at = text[at..].find('\n').map_or(bytes.len(), |i| at + i + 1);
        } else if bytes[at..].starts_with(b"/*") {
            at = after(text, at + 2, "*/");
        } else {
            at += 1;
        }
    }
    bytes.len()
}
/// Source-local Blade scan, matching the owner's existing lexer conventions:
/// comments/echoes/PHP/verbatim are excluded, argument lists reuse php::closing,
/// inline section/php and forelse's bare empty do not open another block.
pub fn ranking(text: &str, cursor: usize, start: usize, end: usize, custom: &std::collections::BTreeSet<&str>) -> Option<Ranking> {
    if text.len() > 512 * 1024
        || start > cursor
        || cursor > end
        || end > text.len()
        || cursor - start > 128
        || !text.is_char_boundary(cursor)
        || !text.is_char_boundary(start)
        || !text.is_char_boundary(end)
    {
        return None;
    }
    let target = start.checked_sub(1)?;
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut tokens = Vec::new();
    while at < bytes.len() {
        if at == target {
            at = end;
            continue;
        }
        let skip = if bytes[at..].starts_with(b"{{--") {
            Some(after(text, at + 4, "--}}"))
        } else if bytes[at..].starts_with(b"<!--") {
            Some(after(text, at + 4, "-->"))
        } else if bytes[at..].starts_with(b"{{") {
            Some(php_end(text, at + 2, "}}"))
        } else if bytes[at..].starts_with(b"{!!") {
            Some(php_end(text, at + 3, "!!}"))
        } else if bytes[at..].starts_with(b"<?") {
            Some(php_end(text, at + 2, "?>"))
        } else if bytes[at..].starts_with(b"@@") {
            Some(at + 2)
        } else if bytes[at..].starts_with(b"@{{") {
            Some(php_end(text, at + 3, "}}"))
        } else {
            None
        };
        if let Some(next) = skip {
            if at <= target && target < next {
                return None;
            }
            at = next;
            continue;
        }
        if bytes[at] != b'@' {
            at += 1;
            continue;
        }
        let first = at;
        at += 1;
        let begin = at;
        while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
            at += 1;
        }
        if begin == at {
            continue;
        }
        let name = &text[begin..at];
        if text[..first].chars().next_back().is_some_and(|c| c == '@' || c == '_' || c == '.' || c.is_alphanumeric()) {
            continue;
        }
        let mut open = at;
        while bytes.get(open).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            open += 1;
        }
        let args = bytes.get(open) == Some(&b'(');
        let mut count = 0;
        if args {
            let Some(close) = crate::php::closing(text, open) else {
                if target >= open {
                    return None;
                }
                break;
            };
            if open <= target && target <= close {
                return None;
            }
            count = crate::php::arguments(text, open).len();
            at = close + 1;
        }
        if name == "verbatim" || (name == "php" && !args) {
            let closing = if name == "php" { "@endphp" } else { "@endverbatim" };
            let mut next = at;
            loop {
                let found = text[next..].find(closing).map(|i| next + i);
                let Some(found) = found else {
                    if target >= at {
                        return None;
                    }
                    at = text.len();
                    break;
                };
                let end = found + closing.len();
                if !bytes.get(end).is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_') {
                    if at <= target && target < end {
                        return None;
                    }
                    at = end;
                    break;
                }
                next = end;
            }
            continue;
        }
        let inline = (name == "section" && count >= 2) || (name == "php" && args) || (name == "empty" && !args);
        if !inline {
            tokens.push((name, first));
            if tokens.len() > 8192 {
                return None;
            }
        }
    }
    let mut stack: Vec<(&str, usize, &[&str])> = Vec::new();
    let mut missing = Vec::new();
    for (name, at) in tokens {
        if custom.contains(name) {
            continue;
        }
        if let Some((_, closers)) = BLOCKS.iter().find(|(open, _)| *open == name) {
            stack.push((name, at, *closers));
        } else if BLOCKS.iter().any(|(_, closers)| closers.contains(&name)) {
            if let Some(index) = stack.iter().rposition(|(_, _, closers)| closers.contains(&name)) {
                stack.remove(index);
            } else {
                missing.push((name, at));
            }
        }
    }
    let mut preferred = Vec::new();
    for (_, at, closers) in stack.into_iter().rev() {
        if at < target {
            for closer in closers {
                if !preferred.iter().any(|candidate| candidate == closer) {
                    preferred.push(closer.to_string());
                }
            }
        }
    }
    let mut repair_opening = Vec::new();
    for (closer, at) in missing {
        if at >= cursor {
            for (open, closers) in BLOCKS {
                if closers.contains(&closer) && !preferred.iter().any(|candidate| candidate == open) {
                    preferred.push(open.to_string());
                    repair_opening.push(open.to_string());
                }
            }
        }
    }
    preferred.retain(|name| !custom.contains(name.as_str()));
    repair_opening.retain(|name| !custom.contains(name.as_str()));
    Some(Ranking { prefix: text[start..cursor].to_owned(), preferred, repair_opening })
}

#[cfg(test)]
mod ranking_tests {
    use super::*;
    use crate::{Index, Main, index::Entry};
    use openidea_plugin::{
        Position,
        mock::MockHost,
        providers::{CompletionItem, Document, Providers},
    };
    fn complete(plugin: &mut Main, marked: &str) -> Vec<CompletionItem> {
        let cursor = marked.find('|').unwrap();
        let text = marked.replacen('|', "", 1);
        let position = openidea_plugin::text::position(&text, cursor);
        plugin.completion(
            &Document { path: "/project/resources/views/view.blade.php".into(), language: "blade".into(), text, version: 1 },
            position,
            None,
        )
    }
    fn names(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }
    #[test]
    fn screenshot_se_prefix_ranks_section_before_substring_candidates() {
        let _host = MockHost::install().with_capabilities(&[]);
        let mut plugin = Main::default();
        let items = complete(&mut plugin, "@if($ok) @endif\n@se|");
        assert_eq!(items[0].label, "section");
        assert!(names(&items).contains(&"else"));
        assert!(names(&items).contains(&"endsection"));
        assert!(items.windows(2).all(|pair| pair[0].sort_text <= pair[1].sort_text));
        let items = complete(&mut plugin, "@if($ok)\n@se|");
        assert_eq!(items[0].label, "section", "prefix beats unmatched nonmatching closer");
    }
    #[test]
    fn unmatched_nearest_blocks_and_section_closer_order_are_ranked() {
        let _host = MockHost::install();
        let mut plugin = Main::default();
        let items = complete(&mut plugin, "@section('content')\n@if($ok)\n@|");
        let labels = names(&items);
        assert_eq!(&labels[..4], ["endif", "endsection", "show", "stop"]);
        assert_eq!(complete(&mut plugin, "@if($ok)\n@end|")[0].label, "endif");
        assert_ne!(complete(&mut plugin, "@if($ok) @endif\n@|")[0].label, "endif");
    }
    #[test]
    fn missing_opening_uses_existing_closer_instead_of_duplicating_it() {
        let _host = MockHost::install();
        let mut plugin = Main::default();
        let items = complete(&mut plugin, "@|\n@endsection");
        assert_eq!(items[0].label, "section");
        assert!(items[0].snippet);
        assert!(!items[0].insert_text.contains("@endsection"));
        assert!(items[0].insert_text.contains("${1:name}"));
        let items = complete(&mut plugin, "@|\n@endif");
        assert_eq!(items[0].label, "if");
        assert!(!items[0].insert_text.contains("@endif"));
    }
    #[test]
    fn inline_sections_and_forelse_empty_follow_the_existing_blade_model() {
        let _host = MockHost::install();
        let mut plugin = Main::default();
        assert_ne!(complete(&mut plugin, "@section('title', 'Inline')\n@|")[0].label, "endsection");
        let items = complete(&mut plugin, "@forelse($items as $item)\n@empty\n@|");
        assert_eq!(items[0].label, "endforelse");
        assert_ne!(items[0].label, "endempty");
        assert_eq!(complete(&mut plugin, "@empty($items)\n@|")[0].label, "endempty");
    }
    #[test]
    fn suppressed_contexts_do_not_offer_directives_or_contribute_phantom_blocks() {
        let _host = MockHost::install();
        let mut plugin = Main::default();
        for marked in [
            "{{-- @se| --}}",
            "<!-- @se| -->",
            "{{\n @se|\n}}",
            "{!!\n @se|\n!!}",
            "{{ '}}' . '@se|' }}",
            "<?php $x='@se|'; ?>",
            "@php\n$x='@se|';\n@endphp",
            "@verbatim\n@se|\n@endverbatim",
            "@if('@se|')",
            "@@se|",
            "é@se|.test",
        ] {
            assert!(complete(&mut plugin, marked).is_empty(), "{marked}");
        }
        let items = complete(&mut plugin, "{{-- @if($fake) --}}\n@verbatim @if($fake) @endverbatim\n@|");
        assert_ne!(items[0].label, "endif");
    }
    #[test]
    fn unicode_replacement_and_right_of_caret_text_remain_exact() {
        let host = MockHost::install().with_capabilities(&[]);
        let mut plugin = Main::default();
        let items = complete(&mut plugin, "😀 @se|TAIL");
        assert_eq!(items[0].label, "section");
        let range = items[0].replace.unwrap();
        assert_eq!(range.start, Position::new(0, 4));
        assert_eq!(range.end, Position::new(0, 10));
        assert_eq!(items[0].insert_text, SNIPPETS.iter().find(|(name, _)| *name == "section").unwrap().1);
        assert!(host.process_runs().is_empty());
        assert!(host.requests().is_empty());
    }
    #[test]
    fn custom_collision_keeps_ownership_and_does_not_invent_core_pairing() {
        let _host = MockHost::install();
        let mut plugin = Main::default();
        let mut index = Index::default();
        for name in ["if", "money"] {
            index.directives.insert(
                name.into(),
                Entry {
                    detail: Some("Custom directive".into()),
                    path: "/project/provider.php".into(),
                    at: Position::new(0, 0),
                    width: name.len() as u32,
                },
            );
        }
        plugin.indexes.insert("/project".into(), index);
        let items = complete(&mut plugin, "@if($custom)\n@|");
        assert_eq!(items.iter().filter(|item| item.label == "if").count(), 1);
        let custom = items.iter().find(|item| item.label == "if").unwrap();
        assert!(!custom.snippet);
        assert_eq!(custom.detail.as_deref(), Some("Custom directive"));
        assert_ne!(items[0].label, "endif");
    }
    #[test]
    fn oversized_views_and_invalid_offsets_are_quiet() {
        let _host = MockHost::install();
        let mut plugin = Main::default();
        let marked = format!("{}@se|", " ".repeat(512 * 1024));
        assert!(complete(&mut plugin, &marked).is_empty());
        assert!(ranking("😀 @", 1, 5, 5, &Default::default()).is_none());
    }
}
