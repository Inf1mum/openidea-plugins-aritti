//! Current-document syntax relationships only. Named Laravel symbols deliberately
//! stay with aritti.laravel; these directive tokens are never Rename anchors.
use crate::{BLOCKS, Blade, directive_opens, lex};
use openidea_plugin::providers::{Document, Providers};
use openidea_plugin::{Location, Position, Range, text};

impl Providers for Blade {
    fn definition(&mut self, doc: &Document, at: Position) -> Vec<Location> {
        if doc.language != "blade" || !doc.path.ends_with(".blade.php") || doc.text.len() > 128 << 10 {
            return Vec::new();
        }
        let Some(offset) = text::offset(&doc.text, at) else { return Vec::new() };
        let parsed = lex(&doc.text);
        if parsed.directives.len() > 4096 {
            return Vec::new();
        }
        let mut stack = Vec::new();
        for directive in &parsed.directives {
            if directive.arguments.as_ref().is_some_and(|arguments|directive.range.end<=arguments.end) {
                return Vec::new();
            }
            let Ok(role)=directive_opens(&doc.text,directive) else {return Vec::new();};
            if let Some(closers) = role {
                stack.push((directive, closers));
            } else if BLOCKS.iter().any(|(_, closers)| closers.contains(&directive.name.as_str())) {
                let Some((opener, closers)) = stack.pop() else { continue };
                if !closers.contains(&directive.name.as_str()) {
                    // Crossed/malformed blocks do not establish a syntax owner.
                    stack.clear();
                    continue;
                }
                if directive.range.start <= offset && offset < directive.range.start + 1 + directive.name.len() {
                    return vec![Location {
                        path: doc.path.clone(),
                        range: Range::new(
                            text::position(&doc.text, opener.range.start),
                            text::position(&doc.text, opener.range.start + 1 + opener.name.len()),
                        ),
                    }];
                }
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn doc(text: &str) -> Document {
        Document { path: "/project/view.blade.php".into(), language: "blade".into(), version: 19, text: text.into() }
    }
    fn target(text: &str, needle: &str) -> Vec<Location> {
        Blade.definition(&doc(text), text::position(text, text.rfind(needle).unwrap() + 2))
    }
    #[test]
    fn php_argument_comment_cannot_steal_closing_directive_owner() {
        let _h=MockHost::install().with_capabilities(&[]);
        let source="@if(/* ) @if($fake) */ $real) @endif";
        let found=target(source,"@endif");assert_eq!(found.len(),1);
        assert_eq!(found[0].range.start,Position::new(0,0));
    }
    #[test]
    fn php_string_close_text_does_not_publish_false_block_navigation() {
        let h=MockHost::install().with_capabilities(&[]);
        let source="<?php $x='?> @if($fake) @endif'; ?> 😀 @if($real) @endif";
        assert!(Blade.definition(&doc(source),text::position(source,source.find("@endif").unwrap()+2)).is_empty());
        let target=Blade.definition(&doc(source),text::position(source,source.rfind("@endif").unwrap()+2));
        assert_eq!(target.len(),1);assert_eq!(text::offset(source,target[0].range.start).unwrap(),source.find("@if($real)").unwrap());
        assert!(h.requests().is_empty() && h.process_runs().is_empty());
    }
    #[test]
    fn nearest_matching_current_text_opener_has_exact_utf16_range_without_io() {
        let host = MockHost::install().with_capabilities(&[]);
        let source = "😀 @if($outer)\n@if($inner) inner @endif\n@endif";
        let inner = &target(source, "@endif\n")[0];
        assert_eq!(inner.range.start, Position::new(1, 0));
        assert_eq!(inner.range.end, Position::new(1, 3));
        let outer = &target(source, "@endif")[0];
        assert_eq!(outer.range.start, Position::new(0, 3));
        assert_eq!(outer.range.end, Position::new(0, 6));
        let fixed = "@if($outer) @foreach($items as $item) @endforeach @endif";
        assert_eq!(target(fixed, "@endforeach")[0].range.start.character, 12);
        assert_eq!(host.fetches(), 0);
    }
    #[test]
    fn inline_forms_forelse_and_section_closers_have_distinct_owners() {
        let _host = MockHost::install().with_capabilities(&[]);
        assert_eq!(target("@section('outer') @section('inline','x') @show", "@show")[0].range.start.character, 0);
        assert_eq!(target("@forelse($xs as $x) @empty none @endforelse", "@endforelse")[0].range.start.character, 0);
        assert_eq!(target("@empty($x) empty @endempty", "@endempty")[0].range.start.character, 0);
        assert!(target("@section('inline','x') @endsection", "@endsection").is_empty());
    }
    #[test]
    fn masked_dynamic_and_malformed_contexts_do_not_gain_links() {
        let _host = MockHost::install().with_capabilities(&[]);
        for source in [
            "{{-- @if($x) @endif --}}",
            "@@if($x) @@endif",
            "@verbatim @if($x) @endif @endverbatim",
            "{{ '@if($x) @endif' }}",
            "@if($x) @foreach($xs as $x) @endif",
            "@if( @endif",
            "@endif",
        ] {
            assert!(target(source, "@endif").is_empty(), "{source}");
        }
        let mut wrong = doc("@if($x) @endif");
        wrong.language = "php".into();
        assert!(Blade.definition(&wrong, Position::new(0, 10)).is_empty());
        wrong = doc(&format!("{}@if($x) @endif", " ".repeat(128 << 10)));
        assert!(Blade.definition(&wrong, text::position(&wrong.text, wrong.text.len() - 3)).is_empty());
    }
    #[test]
    fn current_text_changes_and_directive_budget_never_reuse_old_owners() {
        let _host = MockHost::install().with_capabilities(&[]);
        let source = "😀 @section('é') @if($x) @endif @endsection";
        for end in source.char_indices().map(|(at, _)| at).chain([source.len()]) {
            let prefix = &source[..end];
            for (at, _) in prefix.char_indices() {
                for found in Blade.definition(&doc(prefix), text::position(prefix, at)) {
                    let start = text::offset(prefix, found.range.start).unwrap();
                    let end = text::offset(prefix, found.range.end).unwrap();
                    assert!(start < end && prefix[start..end].starts_with('@'));
                }
            }
        }
        assert_eq!(target("@if($x) @endif", "@endif").len(), 1);
        assert!(target("@if($x) @endfor @endif", "@endif").is_empty());
        let over_budget = format!("{}@if($x) @endif", "@csrf ".repeat(4096));
        assert!(target(&over_budget, "@endif").is_empty());
    }
}
