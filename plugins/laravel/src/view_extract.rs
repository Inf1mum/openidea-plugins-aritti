//! Previewed current-buffer HTML/simple-echo extraction, never arbitrary PHP scope inference.
use crate::{Main, view_creation};
use openidea_plugin::{
    Error, Range, TextEdit,
    file_actions::{FileAction, FileOperation},
    host,
    providers::Document,
    text::offset,
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
pub(crate) struct State {
    next: u64,
    pending: BTreeMap<String, Pending>,
}
#[derive(Clone)]
struct Pending {
    doc: Document,
    root: String,
    from: usize,
    to: usize,
    vars: Vec<String>,
}
const TAGS: &[&str] = &[
    "html",
    "head",
    "body",
    "title",
    "div",
    "span",
    "p",
    "section",
    "article",
    "header",
    "footer",
    "main",
    "nav",
    "aside",
    "ul",
    "ol",
    "li",
    "a",
    "strong",
    "em",
    "b",
    "i",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "table",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "td",
    "th",
    "form",
    "label",
    "input",
    "button",
    "select",
    "option",
    "textarea",
    "img",
    "br",
    "hr",
    "small",
    "pre",
    "code",
    "figure",
    "figcaption",
];
const VOID: &[&str] = &["input", "img", "br", "hr"];
fn variable(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || i > 0 && b.is_ascii_digit())
}
fn fragment(doc: &Document, from: usize, to: usize) -> Option<Vec<String>> {
    if doc.language != "blade"
        || !doc.path.ends_with(".blade.php")
        || doc.text.len() > 128 << 10
        || from >= to
        || to > doc.text.len()
        || to - from > 4096
        || !doc.text.is_char_boundary(from)
        || !doc.text.is_char_boundary(to)
    {
        return None;
    }
    let text = &doc.text;
    if text.contains('@')
        || text.contains("<?")
        || text.contains("{!!")
        || text.contains("<!--")
        || text.contains("{{--")
        || text.contains('`')
    {
        return None;
    }
    let mut masked = text.as_bytes().to_vec();
    let mut cursor = 0;
    let mut vars = BTreeSet::new();
    while let Some(open) = text[cursor..].find("{{").map(|n| cursor + n) {
        let end = text[open + 2..].find("}}").map(|n| open + 2 + n + 2)?;
        let raw = text[open + 2..end - 2].trim();
        let name = raw.strip_prefix('$')?;
        if !variable(name) {
            return None;
        }
        if open < to && from < end {
            if open < from
                || end > to
                || [
                    "app",
                    "errors",
                    "loop",
                    "slot",
                    "attributes",
                    "component",
                    "__env",
                    "__data",
                    "__path",
                    "GLOBALS",
                    "this",
                ]
                .contains(&name)
            {
                return None;
            }
            vars.insert(name.to_owned());
            if vars.len() > 32 {
                return None;
            }
        }
        for b in &mut masked[open..end] {
            if !matches!(*b, b'\r' | b'\n') {
                *b = b' ';
            }
        }
        cursor = end;
    }
    let masked = String::from_utf8(masked).ok()?;
    let bytes = masked.as_bytes();
    let mut at = 0;
    let mut stack: Vec<(String, usize)> = Vec::new();
    let mut matched = false;
    let mut count = 0;
    while at < bytes.len() {
        if bytes[at] != b'<' {
            at += 1;
            continue;
        }
        let start = at;
        at += 1;
        let close = bytes.get(at) == Some(&b'/');
        if close {
            at += 1;
        }
        let name_at = at;
        while bytes.get(at).is_some_and(u8::is_ascii_alphanumeric) {
            at += 1;
        }
        let name = masked.get(name_at..at)?;
        if !TAGS.contains(&name) {
            return None;
        }
        let mut quote = 0;
        let mut ended = false;
        while at < bytes.len() {
            let b = bytes[at];
            if quote != 0 {
                if b == quote {
                    quote = 0;
                }
            } else if matches!(b, b'\'' | b'"') {
                quote = b;
            } else if b == b'>' {
                at += 1;
                ended = true;
                break;
            } else if b == b'<' {
                return None;
            }
            at += 1;
        }
        if !ended || quote != 0 {
            return None;
        }
        count += 1;
        if count > 2048 {
            return None;
        }
        let self_closing = bytes.get(at.saturating_sub(2)) == Some(&b'/');
        if close {
            if !masked[name_at + name.len()..at - 1].trim().is_empty() {
                return None;
            }
            let (opened, begin) = stack.pop()?;
            if opened != name {
                return None;
            }
            if begin == from && at == to {
                matched = true;
            }
        } else if self_closing || VOID.contains(&name) {
            if start == from && at == to {
                matched = true;
            }
        } else {
            stack.push((name.into(), start));
            if stack.len() > 64 {
                return None;
            }
        }
    }
    (stack.is_empty() && matched).then(|| vars.into_iter().collect())
}
fn admitted(main: &Main, p: &Pending, doc: &Document) -> Result<(), Error> {
    if doc.path != p.doc.path
        || doc.language != p.doc.language
        || doc.version != p.doc.version
        || doc.text != p.doc.text
        || fragment(doc, p.from, p.to).as_ref() != Some(&p.vars)
    {
        return Err(Error::Stale);
    }
    let (project, root) = view_creation::owner(doc, Some(&p.root))?;
    let index = main
        .indexes
        .get(&root)
        .filter(|i| !i.partial)
        .ok_or(Error::Stale)?;
    if index.directives.contains_key("include") || project.read_file(&doc.path)? != doc.text {
        return Err(Error::Stale);
    }
    Ok(())
}
pub(crate) fn actions(main: &mut Main, doc: &Document, selection: Range) -> Vec<FileAction> {
    let Ok((_, root)) = view_creation::owner(doc, None) else {
        return vec![];
    };
    let Some(index) = main.indexes.get(&root).filter(|i| !i.partial) else {
        return vec![];
    };
    if index.directives.contains_key("include") {
        return vec![];
    }
    let (Some(from), Some(to)) = (
        offset(&doc.text, selection.start),
        offset(&doc.text, selection.end),
    ) else {
        return vec![];
    };
    let Some(vars) = fragment(doc, from, to) else {
        return vec![];
    };
    if main.view_extractions.pending.len() >= 8 {
        main.view_extractions.pending.clear();
    }
    let Some(next) = main.view_extractions.next.checked_add(1) else {
        return vec![];
    };
    main.view_extractions.next = next;
    let key = format!("extract-view:{next}:{}", doc.version);
    main.view_extractions.pending.insert(
        key.clone(),
        Pending {
            doc: doc.clone(),
            root,
            from,
            to,
            vars,
        },
    );
    vec![FileAction {
        title: "Extract reviewed HTML/simple-variable Blade view…".into(),
        kind: Some("refactor.extract".into()),
        data: Some(key),
        ..Default::default()
    }]
}
pub(crate) fn resolve(
    main: &mut Main,
    doc: &Document,
    action: FileAction,
) -> Result<FileAction, Error> {
    let key = action.data.as_deref().ok_or(Error::Stale)?;
    let p = main
        .view_extractions
        .pending
        .get(key)
        .cloned()
        .ok_or(Error::Stale)?;
    admitted(main, &p, doc)?;
    let name = host::input("New conventional Blade view name", None)?.ok_or(Error::Cancelled)?;
    admitted(main, &p, doc)?;
    if !view_creation::valid_name(&name) {
        return Err(Error::Invalid(
            "Use a literal dot-separated conventional view name.".into(),
        ));
    }
    if main
        .indexes
        .get(&p.root)
        .is_some_and(|i| i.views.contains_key(&name))
    {
        return Err(Error::Stale);
    }
    let (project, _) = view_creation::owner(doc, Some(&p.root))?;
    let (target, config) = view_creation::validate(&project, &p.root, doc, &name)?;
    let data = if p.vars.is_empty() {
        String::new()
    } else {
        format!(
            ", [{}]",
            p.vars
                .iter()
                .map(|v| format!("'{v}' => ${v}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let replacement = format!("@include('{name}'{data})");
    let content = &doc.text[p.from..p.to];
    let preview = format!(
        "Project: {}\nSource: {}\nDocument version: {}\n\nCreate (no overwrite): {}\nExact new view contents:\n{}\n\nReplace this exact selected fragment with:\n{}\n\nSimple read-only variables passed explicitly: {}. No PHP statements, directives, component/raw contexts, assignments or implicit reserved variable scope is moved. Configured roots/namespaces, view composers/custom rendering, caller variable types and runtime behavior are not resolved.\n\nApply one undoable Create + source Edit atomically?",
        p.root,
        doc.path,
        doc.version,
        target,
        content,
        replacement,
        p.vars.join(", ")
    );
    if !ui_kit::prompts::confirm_exact("Extract local Blade fragment?", &preview)? {
        return Err(Error::Cancelled);
    }
    admitted(main, &p, doc)?;
    if main
        .indexes
        .get(&p.root)
        .is_some_and(|i| i.views.contains_key(&name))
    {
        return Err(Error::Stale);
    }
    let (current, current_config) = view_creation::validate(&project, &p.root, doc, &name)?;
    if current != target || current_config != config {
        return Err(Error::Stale);
    }
    main.view_extractions.pending.remove(key);
    Ok(FileAction {
        title: format!("Extract Blade view {name}"),
        operations: vec![FileOperation::Create {
            path: target.clone(),
            text: content.into(),
            overwrite: false,
        }],
        edits: vec![(
            doc.path.clone(),
            vec![TextEdit {
                range: Range::new(
                    openidea_plugin::text::position(&doc.text, p.from),
                    openidea_plugin::text::position(&doc.text, p.to),
                ),
                text: replacement,
            }],
        )],
        open: Some(target),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{file_actions::FileActions, mock::MockHost, text::position};
    fn setup(text: &str) -> (MockHost, Main, Document, Range) {
        let h = MockHost::install();
        h.add_dir("resources/views");
        h.add_file("resources/views/main.blade.php", text);
        h.open_editor(
            "/project/resources/views/main.blade.php",
            "blade",
            text,
            &[],
        );
        let mut main = Main::default();
        main.indexes
            .insert("/project".into(), crate::index::Index::default());
        let d = Document {
            path: "/project/resources/views/main.blade.php".into(),
            language: "blade".into(),
            version: 3,
            text: text.into(),
        };
        let from = text.find("<section").unwrap_or(0);
        let to = text.rfind("</section>").map_or(text.len(), |n| n + 10);
        let selection = Range::new(position(text, from), position(text, to));
        (h, main, d, selection)
    }
    #[test]
    fn exact_fragment_and_readonly_variables_are_one_atomic_create_plus_edit_without_execution() {
        let text = "😀\r\n<main><section>Hi {{ $name }}<b>{{ $title }}</b></section><footer>keep</footer></main>";
        let (h, mut main, d, selected) = setup(text);
        let a = main.file_actions(&d, selected).remove(0);
        let replay = a.clone();
        assert!(a.edits.is_empty() && a.operations.is_empty());
        h.answer_next_prompt(Some(&["partials.card"]));
        h.answer_next_prompt(Some(&["yes"]));
        let fixed = main.resolve_file_action(&d, a).unwrap();
        assert_eq!(
            fixed.operations,
            vec![FileOperation::Create {
                path: "/project/resources/views/partials/card.blade.php".into(),
                text: "<section>Hi {{ $name }}<b>{{ $title }}</b></section>".into(),
                overwrite: false
            }]
        );
        assert!(h.invocation().project().unwrap().read_file("resources/views/partials/card.blade.php").is_err());
        assert_eq!(fixed.edits.len(), 1);
        let edit = &fixed.edits[0].1[0];
        assert_eq!(edit.range, selected);
        assert_eq!(
            edit.text,
            "@include('partials.card', ['name' => $name, 'title' => $title])"
        );
        assert_eq!(
            h.invocation()
                .project()
                .unwrap()
                .read_file(&d.path)
                .unwrap(),
            text
        );
        assert!(main.resolve_file_action(&d, replay).is_err());
        assert!(h.process_runs().is_empty());
        assert_eq!(h.fetches(), 0);
    }
    #[test]
    fn cancelled_stale_foreign_and_existing_targets_never_return_partial_transactions() {
        for mode in 0..6 {
            let (h, mut main, mut d, selected) = setup("<section>{{ $name }}</section>");
            let a = main.file_actions(&d, selected).remove(0);
            match mode {
                0 => h.answer_next_prompt(None),
                1 => {
                    h.answer_next_prompt(Some(&["partials.card"]));
                    h.answer_next_prompt(None);
                }
                2 => d.version += 1,
                3 => h.set_project_root(Some("/other")),
                4 => {
                    h.add_file("resources/views/partials/card.blade.php", "KEEP");
                    h.answer_next_prompt(Some(&["partials.card"]));
                }
                _ => {
                    h.add_project("/project/resources/views/partials");
                    h.answer_next_prompt(Some(&["partials.card"]));
                }
            }
            assert!(main.resolve_file_action(&d, a).is_err(), "{mode}");
            assert!(h.process_runs().is_empty());
        }
    }
    #[test]
    fn unsaved_source_is_supported_but_directive_php_component_and_variable_scopes_are_withheld() {
        let (h, mut main, d, selected) = setup("<section>{{ $name }}</section>");
        h.add_file("resources/views/main.blade.php", "disk differs");
        let a = main.file_actions(&d, selected).remove(0);
        h.answer_next_prompt(Some(&["card"]));
        h.answer_next_prompt(Some(&["yes"]));
        assert_eq!(main.resolve_file_action(&d, a).unwrap().edits.len(), 1);
        for source in [
            "<section>{{ $loop }}</section>",
            "<section>{{ $user->name }}</section>",
            "<section>{{ $x = 1 }}</section>",
            "@verbatim<section>{{ $name }}</section>@endverbatim",
            "<x-card><section>hi</section></x-card>",
            "<section><b>bad</section></b>",
            "<!-- <section>hidden</section> -->",
            "<section>{!! $name !!}</section>",
        ] {
            let d = Document {
                text: source.into(),
                ..d.clone()
            };
            let selected = Range::new(position(source, 0), position(source, source.len()));
            assert!(main.file_actions(&d, selected).is_empty(), "{source}");
        }
    }
    #[test]
    fn partial_nodes_oversize_and_custom_include_ownership_do_not_get_extraction() {
        let (_h, mut main, d, selected) = setup("<section>{{ $name }}</section>");
        let wrong = Range::new(position(&d.text, 1), selected.end);
        assert!(main.file_actions(&d, wrong).is_empty());
        main.indexes.get_mut("/project").unwrap().directives.insert(
            "include".into(),
            crate::index::Entry {
                detail: None,
                path: "/project/Provider.php".into(),
                at: openidea_plugin::Position::new(0, 0),
                width: 7,
            },
        );
        assert!(main.file_actions(&d, selected).is_empty());
        let big = Document {
            text: format!("<section>{}</section>", "x".repeat(4096)),
            ..d
        };
        assert!(fragment(&big, 0, big.text.len()).is_none());
    }
    #[test]
    fn registered_extract_refuses_view_configuration_newly_owned_by_nested_project() {
        let (host,mut main,doc,range)=setup("<section>Hello {{ $name }}</section>");
        let lazy=main.file_actions(&doc,range).remove(0);
        host.add_project("/project/config");host.add_file("config/view.php","<?php return ['paths' => [resource_path('views')]];");
        host.answer_next_prompt(Some(&["partials.card"]));host.answer_next_prompt(Some(&["yes"]));
        assert!(main.resolve_file_action(&doc,lazy).is_err());
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }

}
