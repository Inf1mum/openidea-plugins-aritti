//! Static directive help shares the lexer and its block relationships; no
//! project files, PHP runtime, framework index or network request is consulted.
use crate::{BLOCKS, Blade, directive_opens, lex};
use openidea_plugin::hover::{Document, Hover, HoverProvider};
use openidea_plugin::{Position, Range, text};
const MAX_TEXT: usize = 128 << 10;
const MAX_DIRECTIVES: usize = 4096;

fn description(name: &str) -> &'static str {
    match name {
        "if" | "elseif" | "else" | "unless" | "isset" | "empty" | "auth" | "guest" | "can" | "cannot" | "canany" | "elsecan"
        | "elsecannot" | "elsecanany" | "env" | "production" => "Conditionally renders this branch.",
        "for" | "foreach" | "forelse" | "while" => "Repeats a template block.",
        "section" => "Defines a named layout section; a null content value starts a captured section.",
        "include" | "includeIf" | "includeWhen" | "includeUnless" | "includeFirst" | "each" => "Renders a view.",
        "extends" | "yield" | "parent" => "Declares or consumes layout inheritance.",
        "push" | "pushIf" | "prepend" | "pushOnce" | "prependOnce" | "stack" => "Adds to or renders a named stack.",
        "csrf" => "Outputs the form's CSRF field.",
        "method" => "Outputs the form method override.",
        "checked" | "selected" | "disabled" | "readonly" | "required" | "class" | "style" => "Conditionally emits HTML attributes.",
        "php" => "Runs inline PHP or a PHP block.",
        "verbatim" => "Leaves its contents uninterpreted as Blade.",
        "component" | "slot" | "props" | "aware" => "Defines component content or data.",
        "json" | "js" => "Serializes data for a script context.",
        "lang" | "choice" => "Looks up translated text.",
        "livewire" | "livewireStyles" | "livewireScripts" | "livewireScriptConfig" | "persist" | "teleport" | "script" | "assets" => {
            "Livewire directive; availability depends on the installed Livewire version."
        }
        _ => "Recognized by Blade's static directive metadata.",
    }
}
fn help(name: &str, arguments: bool, role: Result<Option<&'static [&'static str]>, ()>) -> String {
    let mut markdown = format!("**`@{name}`**\n\n{}", description(name));
    if role.is_err() {
        markdown.push_str("\n\nCapture role is unknown from these arguments; no block ownership is inferred.");
    } else if let Ok(Some(closers)) = role {
        markdown.push_str(&format!(
            "\n\nBlock form. Close with {}.",
            closers.iter().map(|closer| format!("`@{closer}`")).collect::<Vec<_>>().join(" or ")
        ));
    } else if matches!(name, "section" | "push" | "prepend" | "php") {
        markdown.push_str("\n\nInline form; no closing directive is required.");
    } else if name == "empty" && !arguments {
        markdown.push_str("\n\nEmpty branch of `@forelse`; the loop still closes with `@endforelse`.");
    } else {
        let openers: Vec<_> =
            BLOCKS.iter().filter(|(_, closers)| closers.contains(&name)).map(|(opener, _)| format!("`@{opener}`")).collect();
        if !openers.is_empty() {
            markdown.push_str(&format!("\n\nCloses {}.", openers.join(" or ")));
        }
    }
    markdown.push_str(
        "\n\nStatic syntax help; this does not resolve PHP types, arguments, custom directives or the installed framework version.",
    );
    markdown
}
impl HoverProvider for Blade {
    fn hover(&mut self, doc: &Document, at: Position) -> Option<Hover> {
        if doc.language != "blade" || !doc.path.ends_with(".blade.php") || doc.text.len() > MAX_TEXT {
            return None;
        }
        let offset = text::offset(&doc.text, at)?;
        let parsed = lex(&doc.text);
        let directive = parsed
            .directives
            .into_iter()
            .take(MAX_DIRECTIVES)
            .find(|directive| directive.range.start <= offset && offset < directive.range.start + 1 + directive.name.len())?;
        if directive.arguments.as_ref().is_some_and(|arguments|directive.range.end<=arguments.end) {return None;}
        let role=directive_opens(&doc.text,&directive);
        let end = directive.range.start + 1 + directive.name.len();
        Some(
            Hover::new(help(&directive.name, directive.arguments.is_some(), role))
                .range(Range::new(text::position(&doc.text, directive.range.start), text::position(&doc.text, end))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn doc(text: &str) -> Document {
        Document { path: "/outside/view.blade.php".into(), language: "blade".into(), version: 3, text: text.into() }
    }
    fn hover_at(source: &str, needle: &str) -> Option<Hover> {
        let at = source.find(needle)? + 1;
        Blade.hover(&doc(source), text::position(source, at))
    }
    #[test]
    fn php_argument_comment_content_never_creates_directive_hover() {
        let _h=MockHost::install().with_capabilities(&[]);
        let source="@if(/* ) @if($fake) */ $real) @endif";
        assert!(hover_at(source,"@if($fake)").is_none());
    }
    #[test]
    fn section_argument_comment_comma_does_not_invent_inline_form() {
        let _h=MockHost::install().with_capabilities(&[]);
        let source="@section('title' /* , */)\n@endsection";
        let hover=hover_at(source,"@section").unwrap();
        assert!(hover.markdown.contains("Block form") && !hover.markdown.contains("Inline form"));
    }
    #[test]
    fn php_boundary_preserves_real_line_comment_tags_and_withholds_unknown_string_states() {
        let _h=MockHost::install().with_capabilities(&[]);
        for source in [
            "<?php // comment ?> @if($real) @endif",
            "<?php # comment ?> @if($real) @endif",
            "<?php $x=`?> @if($fake)`; ?> @if($real) @endif",
            "<?php #[Example(\"?> @if($fake)\")] function f() {} ?> @if($real) @endif",
            "<?php $x='escaped \\' ?> @if($fake)'; ?> @if($real) @endif",
        ] {
            assert!(hover_at(source,"@if($real)").is_some(),"{source}");
            if source.contains("@if($fake)") {assert!(hover_at(source,"@if($fake)").is_none(),"{source}");}
        }
        for source in [
            "<?php $x='?> @if($fake)",
            "<?php /* ?> @if($fake)",
            "<?php $x=<<<TXT\n?> @if($fake)\nTXT;\n?>",
            r#"<?php $x="{$map["?> @if($fake)"]}"; ?>"#,
        ] {assert!(hover_at(source,"@if($fake)").is_none(),"{source}");}
    }
    #[test]
    fn php_quoted_and_block_comment_close_text_never_creates_directive_hover() {
        let h=MockHost::install().with_capabilities(&[]);
        for source in [
            "<?php $x='?> @if($fake)'; ?> @if($real) @endif",
            "<?php $x=\"?> @if($fake)\"; ?> @if($real) @endif",
            "<?php /* ?> @if($fake) */ ?> @if($real) @endif",
        ] {
            assert!(hover_at(source,"@if($fake)").is_none(),"{source}");
            assert!(hover_at(source,"@if($real)").is_some());
        }
        assert!(h.requests().is_empty() && h.process_runs().is_empty());
    }
    #[test]
    fn static_block_and_inline_help_needs_no_grants_and_has_exact_utf16_range() {
        let _host = MockHost::install().with_capabilities(&[]);
        let source = "😀 prefix\r\n  😀 @section('title')\r\n@endsection";
        let hover = hover_at(source, "@section").unwrap();
        assert!(hover.markdown.contains("Close with `@endsection`"));
        assert_eq!(hover.range, Some(Range::new(Position::new(1, 5), Position::new(1, 13))));
        assert_eq!(
            &source[text::offset(source, hover.range.unwrap().start).unwrap()..text::offset(source, hover.range.unwrap().end).unwrap()],
            "@section"
        );
        assert!(hover_at("@section('title', 'Home')", "@section").unwrap().markdown.contains("Inline form"));
        assert!(hover_at("@php($n = 1)", "@php").unwrap().markdown.contains("Inline form"));
        assert!(hover_at("@php\n$n = 1;\n@endphp", "@php").unwrap().markdown.contains("Close with `@endphp`"));
        assert!(hover_at("@forelse($items as $item)\n@empty\n@endforelse", "@empty").unwrap().markdown.contains("Empty branch"));
        assert!(hover_at("@if($ok) @endif", "@endif").unwrap().markdown.contains("Closes `@if`"));
    }
    #[test]
    fn suppressed_lexer_contexts_and_unknown_directives_have_no_hover() {
        let _host = MockHost::install().with_capabilities(&[]);
        for source in [
            "{{-- @if($x) --}}",
            "@@if($x)",
            "hello@if.example",
            "@verbatim @if($x) @endverbatim",
            "{{ '@if($x)' }}",
            "<?php echo '@if($x)'; ?>",
            "@php echo '@if($x)'; @endphp",
            "@customThing($x)",
        ] {
            let needle = if source.contains("@if") { "@if" } else { "@customThing" };
            assert!(hover_at(source, needle).is_none(), "{source}");
        }
        let source = "@if($condition)";
        let at = text::position(source, source.find("condition").unwrap());
        assert!(Blade.hover(&doc(source), at).is_none());
        let mut foreign = doc(source);
        foreign.language = "php".into();
        assert!(Blade.hover(&foreign, Position::new(0, 1)).is_none());
        foreign = doc(source);
        foreign.path = "/outside/file.php".into();
        assert!(Blade.hover(&foreign, Position::new(0, 1)).is_none());
    }
    #[test]
    fn all_static_names_have_help_and_work_is_bounded() {
        let _host = MockHost::install();
        for name in crate::STANDALONE
            .iter()
            .copied()
            .chain(BLOCKS.iter().flat_map(|(opener, closers)| std::iter::once(*opener).chain(closers.iter().copied())))
        {
            let source = format!("@{name}");
            assert!(hover_at(&source, &source).is_some(), "{name}");
            assert!(help(name, false, Ok(crate::opens(name,false,0))).len() < 2048);
        }
        assert!(Blade.hover(&doc(&format!("@if{}", " ".repeat(MAX_TEXT))), Position::new(0, 1)).is_none());
        let source = format!("{}@csrf", "@csrf\n".repeat(MAX_DIRECTIVES));
        assert!(Blade.hover(&doc(&source), text::position(&source, source.len() - 3)).is_none());
    }
}
