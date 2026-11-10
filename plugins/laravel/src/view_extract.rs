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
    component: bool,
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
fn overlaps(a: std::ops::Range<usize>, from: usize, to: usize) -> bool {
    a.start < to && from < a.end
}
fn blank(bytes: &mut [u8], from: usize, to: usize) {
    for byte in &mut bytes[from..to] {
        if !matches!(*byte, b'\r' | b'\n') {
            *byte = b' ';
        }
    }
}
fn echo_end(text: &str, start: usize, raw: bool) -> Option<usize> {
    let bytes = text.as_bytes();
    let marker = if raw {
        b"!!}".as_slice()
    } else {
        b"}}".as_slice()
    };
    let mut at = start + if raw { 3 } else { 2 };
    let mut stack = Vec::new();
    while at < bytes.len() {
        if stack.is_empty() && bytes[at..].starts_with(marker) {
            return Some(at + marker.len());
        }
        if matches!(bytes[at], b'\'' | b'"') {
            let quote = bytes[at];
            let begin = at;
            at = crate::php::string_end(bytes, at);
            if at <= begin + 1 || at > bytes.len() || bytes.get(at - 1) != Some(&quote) {
                return None;
            }
        } else if bytes[at..].starts_with(b"/*") {
            at += text[at + 2..].find("*/")? + 4;
        } else if bytes[at..].starts_with(b"//") || bytes[at] == b'#' {
            at += text[at..].find('\n')?;
        } else {
            match bytes[at] {
                b'(' => stack.push(b')'),
                b'[' => stack.push(b']'),
                b'{' => stack.push(b'}'),
                b')' | b']' | b'}' if stack.pop() != Some(bytes[at]) => return None,
                b'`' => return None,
                _ => {}
            }
            if stack.len() > 64 {
                return None;
            }
            at += 1;
        }
    }
    None
}

/// Prove the selected fragment is template content rather than a comment, tag
/// attribute, directive argument or control-flow body. Known section/stack
/// wrappers preserve the fragment's data environment; unknown directives do not.
fn boundary(text: &str, from: usize, to: usize) -> Option<String> {
    let bytes = text.as_bytes();
    let mut result = bytes.to_vec();
    let opaque = crate::blade_php::ranges(text)?;
    let mut span = 0;
    let mut at = 0;
    let mut stack: Vec<(&str, bool)> = Vec::new();
    let mut checked = false;
    while at < bytes.len() {
        if !checked && at >= from {
            if stack.iter().any(|(_, safe)| !*safe) {
                return None;
            }
            checked = true;
        }
        while opaque.get(span).is_some_and(|range| range.end <= at) {
            span += 1;
        }
        if let Some(range) = opaque.get(span).filter(|range| range.contains(&at)) {
            if overlaps(range.clone(), from, to) {
                return None;
            }
            blank(&mut result, range.start, range.end);
            at = range.end;
            continue;
        }
        if bytes[at..].starts_with(b"<!--") {
            let end = at + 4 + text[at + 4..].find("-->")? + 3;
            if overlaps(at..end, from, to) {
                return None;
            }
            blank(&mut result, at, end);
            at = end;
            continue;
        }
        if bytes[at..].starts_with(b"{{") || bytes[at..].starts_with(b"{!!") {
            let raw = bytes[at..].starts_with(b"{!!");
            let end = echo_end(text, at, raw)?;
            if overlaps(at..end, from, to) {
                if raw || from > at || end > to {
                    return None;
                }
            } else {
                blank(&mut result, at, end);
            }
            at = end;
            continue;
        }
        if bytes[at] != b'@'
            || at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_')
        {
            at += 1;
            continue;
        }
        let begin = at;
        at += 1;
        let start = at;
        while bytes.get(at).is_some_and(|b| b.is_ascii_alphabetic()) {
            at += 1;
        }
        let name = text.get(start..at)?;
        if name.is_empty() {
            return None;
        }
        let mut open = at;
        while bytes.get(open).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            open += 1;
        }
        let arguments = if bytes.get(open) == Some(&b'(') {
            let end = crate::php::closing(text, open)?;
            at = end + 1;
            Some(crate::php::arguments(text, open))
        } else {
            None
        };
        if overlaps(begin..at, from, to) {
            return None;
        }
        match name {
            "extends" => {
                let args = arguments.as_ref()?;
                if args.len() != 1 || crate::php::literal(text, args[0].clone()).is_none() {
                    return None;
                }
            }
            "section" => {
                let args = arguments.as_ref()?;
                if args.is_empty()
                    || args.len() > 2
                    || crate::php::literal(text, args[0].clone()).is_none()
                {
                    return None;
                }
                if args.len() == 1 || text[args[1].clone()].trim() == "null" {
                    stack.push(("section", true));
                } else if crate::php::literal(text, args[1].clone()).is_none() {
                    return None;
                }
            }
            "push" | "prepend" => {
                let args = arguments.as_ref()?;
                if args.len() != 1 || crate::php::literal(text, args[0].clone()).is_none() {
                    return None;
                }
                stack.push((name, true));
            }
            "endsection" | "show" | "stop" | "append" | "overwrite" => {
                if stack.pop()?.0 != "section" {
                    return None;
                }
            }
            "endpush" => {
                if stack.pop()?.0 != "push" {
                    return None;
                }
            }
            "endprepend" => {
                if stack.pop()?.0 != "prepend" {
                    return None;
                }
            }
            "empty"
                if arguments.is_none()
                    && stack.last().is_some_and(|(name, _)| *name == "forelse") => {}
            "if" | "unless" | "foreach" | "forelse" | "for" | "while" | "isset" | "empty"
            | "auth" | "guest" | "env" | "production" | "once" | "hasSection"
            | "sectionMissing" => stack.push((name, false)),
            "else" | "elseif" => {
                if stack.last().is_none_or(|(_, safe)| *safe) {
                    return None;
                }
            }
            "endif" | "endunless" | "endforeach" | "endforelse" | "endfor" | "endwhile"
            | "endisset" | "endempty" | "endauth" | "endguest" | "endenv" | "endproduction"
            | "endonce" => {
                let expected = name.strip_prefix("end")?;
                let actual = stack.pop()?.0;
                if actual != expected
                    && !(name == "endif" && matches!(actual, "hasSection" | "sectionMissing"))
                {
                    return None;
                }
            }
            "include" | "includeIf" | "includeWhen" | "includeUnless" | "includeFirst" | "each"
            | "yield" | "stack" | "vite" | "json" | "method" => {
                arguments.as_ref()?;
            }
            "csrf" | "livewireStyles" | "livewireScripts" => {
                if arguments.is_some() {
                    return None;
                }
            }
            _ => return None,
        }
        if stack.len() > 64 {
            return None;
        }
        blank(&mut result, begin, at);
    }
    if !checked || !stack.is_empty() {
        return None;
    }
    String::from_utf8(result).ok()
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
    let selected = &text[from..to];
    if selected.contains('@')
        || selected.contains("<?")
        || selected.contains("{!!")
        || selected.contains("<!--")
        || selected.contains("{{--")
        || selected.contains('`')
    {
        return None;
    }
    let boundary = boundary(text, from, to)?;
    let mut masked = boundary.as_bytes().to_vec();
    let mut cursor = 0;
    let mut vars = BTreeSet::new();
    while let Some(open) = boundary[cursor..].find("{{").map(|n| cursor + n) {
        let end = echo_end(text, open, false)?;
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
fn shadowed(index: &crate::index::Index, doc: &Document, component: bool) -> bool {
    index.directives.keys().any(|name| {
        name == "include" || component && name == "props" || doc.text.contains(&format!("@{name}"))
    })
}
fn component_name(name: &str) -> bool {
    view_creation::valid_name(name)
        && !matches!(name, "slot" | "dynamic-component")
        && name.split('.').all(|part| {
            part.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}
fn target_plan(
    main: &Main,
    p: &Pending,
    doc: &Document,
    name: &str,
) -> Result<(String, String), Error> {
    let (project, _) = view_creation::owner(doc, Some(&p.root))?;
    let index = main
        .indexes
        .get(&p.root)
        .filter(|index| !index.partial)
        .ok_or(Error::Stale)?;
    if p.component {
        if !component_name(name) || index.open_components || index.components.contains_key(name) {
            return Err(Error::Invalid(
                "Use a new kebab-case anonymous component name with proven registration ownership."
                    .into(),
            ));
        }
        let class = name
            .split('.')
            .map(|part| {
                part.split('-')
                    .map(|word| {
                        let mut chars = word.chars();
                        chars
                            .next()
                            .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                            .unwrap_or_default()
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("/");
        view_creation::missing(
            &project,
            &format!("{}/app/View/Components/{class}.php", p.root),
        )?;
    } else if !view_creation::valid_name(name) || index.views.contains_key(name) {
        return Err(Error::Stale);
    }
    view_creation::validate(
        &project,
        &p.root,
        doc,
        &if p.component {
            format!("components.{name}")
        } else {
            name.into()
        },
    )
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
    if shadowed(index, doc, p.component)
        || p.component && index.open_components
        || project.read_file(&doc.path)? != doc.text
    {
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
    if shadowed(index, doc, false) {
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
    let component_allowed = !index.open_components && !shadowed(index, doc, true);
    let mut result = Vec::new();
    for component in [false, true] {
        if component && !component_allowed {
            continue;
        }
        let Some(next) = main.view_extractions.next.checked_add(1) else {
            break;
        };
        main.view_extractions.next = next;
        let key = format!("extract-view:{next}:{}", doc.version);
        main.view_extractions.pending.insert(
            key.clone(),
            Pending {
                doc: doc.clone(),
                root: root.clone(),
                from,
                to,
                vars: vars.clone(),
                component,
            },
        );
        result.push(FileAction {
            title: if component {
                "Extract reviewed anonymous Blade component…"
            } else {
                "Extract reviewed HTML/simple-variable Blade view…"
            }
            .into(),
            kind: Some("refactor.extract".into()),
            data: Some(key),
            ..Default::default()
        });
    }
    result
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
    let name = host::input(
        if p.component {
            "New anonymous Blade component name (dot/kebab-case)"
        } else {
            "New conventional Blade view name"
        },
        None,
    )?
    .ok_or(Error::Cancelled)?;
    admitted(main, &p, doc)?;
    let (target, config) = target_plan(main, &p, doc, &name)?;
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
    let replacement = if p.component {
        let attributes = p
            .vars
            .iter()
            .map(|name| format!(" :{name}=\"${name}\""))
            .collect::<String>();
        format!("<x-{name}{attributes} />")
    } else {
        format!("@include('{name}'{data})")
    };
    let selected = &doc.text[p.from..p.to];
    let content = if p.component && !p.vars.is_empty() {
        format!(
            "@props([{}]){selected}",
            p.vars
                .iter()
                .map(|name| format!("'{name}'"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        selected.into()
    };
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
    if !ui_kit::prompts::confirm_exact(
        if p.component {
            "Extract anonymous Blade component?"
        } else {
            "Extract local Blade fragment?"
        },
        &preview,
    )? {
        return Err(Error::Cancelled);
    }
    admitted(main, &p, doc)?;
    let (current, current_config) = target_plan(main, &p, doc, &name)?;
    if current != target || current_config != config {
        return Err(Error::Stale);
    }
    main.view_extractions.pending.remove(key);
    Ok(FileAction {
        title: if p.component {
            format!("Extract Blade component {name}")
        } else {
            format!("Extract Blade view {name}")
        },
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
        assert!(
            h.invocation()
                .project()
                .unwrap()
                .read_file("resources/views/partials/card.blade.php")
                .is_err()
        );
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
        let (host, mut main, doc, range) = setup("<section>Hello {{ $name }}</section>");
        let lazy = main.file_actions(&doc, range).remove(0);
        host.add_project("/project/config");
        host.add_file(
            "config/view.php",
            "<?php return ['paths' => [resource_path('views')]];",
        );
        host.answer_next_prompt(Some(&["partials.card"]));
        host.answer_next_prompt(Some(&["yes"]));
        assert!(main.resolve_file_action(&doc, lazy).is_err());
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }

    #[test]
    fn ordinary_layout_sections_and_unrelated_code_do_not_block_selection() {
        for (prefix, suffix) in [
            (
                "@extends('layouts.app')\n@section('title', 'Hello')\n@section('content')\n",
                "\n@endsection",
            ),
            (
                "@extends('layouts.app')\n{{-- @bad('<broken>') --}}\n@section('content')\n{{ $user->name }}\n",
                "\n@endsection",
            ),
            ("<!-- <bad> -->\n@push('scripts')\n", "\n@endpush"),
            ("@if($show) <p>unrelated</p> @endif\n", ""),
            ("@php $greeting = 'Hello'; @endphp\n", ""),
        ] {
            let text = format!("{prefix}<section>Hello {{{{ $name }}}}</section>{suffix}");
            let (host, mut main, doc, selection) = setup(&text);
            let lazy = main
                .file_actions(&doc, selection)
                .into_iter()
                .find(|action| action.title.contains("Blade view"))
                .unwrap_or_else(|| panic!("{text}"));
            host.answer_next_prompt(Some(&["partials.card"]));
            host.answer_next_prompt(Some(&["yes"]));
            let fixed = main.resolve_file_action(&doc, lazy).unwrap();
            assert_eq!(fixed.edits[0].1[0].range, selection);
            assert_eq!(
                fixed.edits[0].1[0].text,
                "@include('partials.card', ['name' => $name])"
            );
            assert_eq!(
                host.invocation()
                    .project()
                    .unwrap()
                    .read_file(&doc.path)
                    .unwrap(),
                text
            );
            assert!(host.process_runs().is_empty());
        }
    }
    #[test]
    fn control_scopes_opaque_regions_directive_arguments_and_malformed_wrappers_are_withheld() {
        for (prefix, suffix) in [
            ("@foreach($items as $name)", "@endforeach"),
            ("@if($show)", "@endif"),
            ("@verbatim", "@endverbatim"),
            ("@php $x = '", "'; @endphp"),
            ("{{-- ", " --}}"),
            ("<!-- ", " -->"),
            ("@include('", "')"),
            ("@section('content')", "@endpush"),
            ("@custom ", ""),
            ("@section($dynamic)", "@endsection"),
        ] {
            let text = format!("{prefix}<section>Hello {{{{ $name }}}}</section>{suffix}");
            let (_host, mut main, doc, selection) = setup(&text);
            assert!(
                main.file_actions(&doc, selection)
                    .iter()
                    .all(|action| !action.title.starts_with("Extract reviewed")),
                "{text}"
            );
        }
    }
    #[test]
    fn anonymous_component_extracts_explicit_props_as_one_create_and_source_edit() {
        let text = "😀\r\n@extends('layouts.app')\r\n@section('content')\r\n<section>Hello {{ $userName }}<b>{{ $title }}</b></section>\r\n@endsection";
        let (host, mut main, doc, selection) = setup(text);
        let lazy = main
            .file_actions(&doc, selection)
            .into_iter()
            .find(|action| action.title.contains("anonymous Blade component"))
            .unwrap();
        let replay = lazy.clone();
        host.answer_next_prompt(Some(&["forms.user-card"]));
        host.answer_next_prompt(Some(&["yes"]));
        let fixed = main.resolve_file_action(&doc, lazy).unwrap();
        assert_eq!(fixed.operations,vec![FileOperation::Create{
            path:"/project/resources/views/components/forms/user-card.blade.php".into(),
            text:"@props(['title', 'userName'])<section>Hello {{ $userName }}<b>{{ $title }}</b></section>".into(),
            overwrite:false,
        }]);
        assert_eq!(fixed.edits[0].1[0].range, selection);
        assert_eq!(
            fixed.edits[0].1[0].text,
            "<x-forms.user-card :title=\"$title\" :userName=\"$userName\" />"
        );
        assert_eq!(
            host.invocation()
                .project()
                .unwrap()
                .read_file(&doc.path)
                .unwrap(),
            text
        );
        assert!(
            host.invocation()
                .project()
                .unwrap()
                .read_file("resources/views/components/forms/user-card.blade.php")
                .is_err()
        );
        assert!(main.resolve_file_action(&doc, replay).is_err());
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn anonymous_component_collision_cancel_custom_props_and_nested_ownership_return_no_transaction()
     {
        for mode in 0..6 {
            let (host, mut main, mut doc, selection) =
                setup("<section>Hello {{ $name }}</section>");
            let lazy = main
                .file_actions(&doc, selection)
                .into_iter()
                .find(|action| action.title.contains("anonymous Blade component"))
                .unwrap();
            match mode {
                0 => host.answer_next_prompt(None),
                1 => {
                    host.answer_next_prompt(Some(&["card"]));
                    host.answer_next_prompt(None);
                }
                2 => {
                    host.add_file("resources/views/components/card.blade.php", "KEEP");
                    host.answer_next_prompt(Some(&["card"]));
                }
                3 => {
                    host.add_file("app/View/Components/Card.php", "KEEP");
                    host.answer_next_prompt(Some(&["card"]));
                }
                4 => {
                    host.add_project("/project/resources/views/components");
                    host.answer_next_prompt(Some(&["card"]));
                }
                _ => doc.version += 1,
            }
            assert!(main.resolve_file_action(&doc, lazy).is_err(), "{mode}");
            assert_eq!(
                host.invocation()
                    .project()
                    .unwrap()
                    .read_file(&doc.path)
                    .unwrap(),
                "<section>Hello {{ $name }}</section>"
            );
            assert!(host.process_runs().is_empty());
        }
        let (_host, mut main, doc, selection) = setup("<section>Hello {{ $name }}</section>");
        main.indexes.get_mut("/project").unwrap().directives.insert(
            "props".into(),
            crate::index::Entry {
                detail: None,
                path: "/project/app/Provider.php".into(),
                at: openidea_plugin::Position::new(0, 0),
                width: 5,
            },
        );
        assert!(
            main.file_actions(&doc, selection)
                .iter()
                .all(|action| !action.title.contains("anonymous Blade component"))
        );
    }
    #[test]
    fn overridden_wrapper_directive_and_incomplete_boundary_do_not_get_extraction() {
        let text = "@section('content')<section>Hello {{ $name }}</section>@endsection";
        let (_host, mut main, doc, selection) = setup(text);
        main.indexes.get_mut("/project").unwrap().directives.insert(
            "section".into(),
            crate::index::Entry {
                detail: None,
                path: "/project/app/Provider.php".into(),
                at: openidea_plugin::Position::new(0, 0),
                width: 7,
            },
        );
        assert!(
            main.file_actions(&doc, selection)
                .iter()
                .all(|action| !action.title.starts_with("Extract reviewed"))
        );
        for text in [
            "@section('content')<section>Hello</section>",
            "<div title=\"<section>Hello</section>\"></div>",
            "{{ '<section>Hello</section>' }}",
        ] {
            let (_host, mut main, doc, selection) = setup(text);
            assert!(
                main.file_actions(&doc, selection)
                    .iter()
                    .all(|action| !action.title.starts_with("Extract reviewed")),
                "{text}"
            );
        }
    }
}
