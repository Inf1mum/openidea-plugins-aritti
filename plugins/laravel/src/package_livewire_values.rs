//! Names inside wire:model / event / target values retain the current view's
//! proven component receiver, rather than borrowing a different component tag.
use super::*;
#[derive(Clone, Copy)]
enum Usage {
    Property,
    Action,
    Both,
}
fn context(doc: &Document, at: usize) -> Option<(std::ops::Range<usize>, Usage)> {
    if doc.language != "blade" || doc.text.len() > 128 * 1024 {
        return None;
    }
    let visible = crate::refs::without_comments(&doc.text);
    let lower = visible.to_ascii_lowercase();
    let bytes = visible.as_bytes();
    let open = visible[..at].rfind('<')?;
    let mut i = open + 1;
    if bytes.get(i) == Some(&b'/') {
        return None;
    }
    while i < at && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
        i += 1;
    }
    while i <= at {
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        let name_start = i;
        while bytes.get(i).is_some_and(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b':' | b'.' | b'@')
        }) {
            i += 1;
        }
        if i == name_start {
            return None;
        }
        let attr = &visible[name_start..i];
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        if bytes.get(i) != Some(&b'=') {
            continue;
        }
        i += 1;
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        let q = *bytes.get(i)?;
        if !matches!(q, b'\'' | b'"') {
            return None;
        }
        i += 1;
        let value = i;
        while i < bytes.len() && bytes[i] != q {
            i += 1;
        }
        if value <= at && at <= i {
            if !crate::blade::wire_attribute(&visible, &lower, name_start) {
                return None;
            }
            let kind = attr.strip_prefix("wire:")?.split('.').next()?;
            let usage = match kind {
                "model" => Usage::Property,
                "target" => Usage::Both,
                "click" | "dblclick" | "submit" | "keydown" | "keyup" | "blur" | "focus"
                | "mouseenter" | "mouseleave" | "init" | "poll" | "intersect" => Usage::Action,
                _ => return None,
            };
            let mut start = value;
            let mut depth = 0usize;
            let mut quote = None;
            for (pos, b) in visible[value..at].bytes().enumerate() {
                match (quote, b) {
                    (Some(q), b) if q == b => quote = None,
                    (Some(_), _) => {}
                    (None, b'\'' | b'"') => quote = Some(b),
                    (None, b'(') => depth += 1,
                    (None, b')') => depth = depth.saturating_sub(1),
                    (None, b',') if depth == 0 && matches!(usage, Usage::Both) => {
                        start = value + pos + 1
                    }
                    _ => {}
                }
            }
            if quote.is_some() || depth > 0 {
                return None;
            }
            while bytes.get(start).is_some_and(u8::is_ascii_whitespace) {
                start += 1;
            }
            if visible[start..i].starts_with("$wire.") {
                start += 6;
            }
            let end = start
                + visible[start..i]
                    .bytes()
                    .take_while(|b| crate::php::is_identifier(*b))
                    .count();
            if at < start || at > end || visible.get(start..end).is_none() {
                return None;
            }
            return Some((start..end, usage));
        }
        if i >= bytes.len() {
            return None;
        }
        i += 1;
    }
    None
}
fn names(index: &Index, doc: &Document, usage: Usage) -> Names {
    let Some(entry) = super::fixes::target(index, &doc.path) else {
        return Names::new();
    };
    let Some((name, _)) = index
        .livewire
        .iter()
        .find(|(_, candidate)| candidate.path == entry.path)
    else {
        return Names::new();
    };
    let target = format!("name:{name}");
    let mut names = Names::new();
    if matches!(usage, Usage::Property | Usage::Both) {
        names.extend(super::livewire::test_names(
            index, doc, &target, false, false,
        ));
    }
    if matches!(usage, Usage::Action | Usage::Both) {
        names.extend(super::livewire::test_names(
            index, doc, &target, true, false,
        ));
    }
    names
}
pub(super) fn completion(index: &Index, doc: &Document, at: Position) -> Vec<CompletionItem> {
    let Some(at) = text::offset(&doc.text, at) else {
        return vec![];
    };
    let Some((range, usage)) = context(doc, at) else {
        return vec![];
    };
    let typed = &doc.text[range.start..at];
    names(index, doc, usage)
        .into_iter()
        .filter(|(name, _)| name.starts_with(typed))
        .map(|(name, entry)| {
            let mut item = CompletionItem::new(&name, CompletionKind::Property);
            item.detail = super::presentation_detail(entry.detail);
            item.replace = Some(Range::new(
                text::position(&doc.text, range.start),
                text::position(&doc.text, range.end),
            ));
            item
        })
        .collect()
}
pub(super) fn definition(index: &Index, doc: &Document, at: Position) -> Vec<Location> {
    let run = || {
        let at = text::offset(&doc.text, at)?;
        let (range, usage) = context(doc, at)?;
        let names = names(index, doc, usage);
        let entry = names.get(&doc.text[range])?;
        crate::scoped_location(&crate::project_for(&doc.path)?, entry)
    };
    run().into_iter().collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, providers::Providers};
    #[test]
    fn real_provider_wire_values_keep_owned_component_scope_and_argument_suffixes() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("artisan", "<?php");
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        host.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"livewire/livewire","version":"4.4.8"}]}"#,
        );
        let class = "<?php\nnamespace App\\Livewire;\nclass Counter extends \\Livewire\\Component {\npublic int $count=0;\npublic function increment() {}\nprivate function hidden(){}\npublic function wrap(){function nested(){}}\n}";
        host.add_file("app/Livewire/Counter.php", class);
        host.add_file("resources/views/livewire/counter.blade.php", "<div></div>");
        let index = Index::build(&host.invocation().project().unwrap());
        let mut main = crate::Main::default();
        main.indexes.insert("/project".into(), index);
        for (source, needle, label) in [
            ("<button wire:click=\"inc(1)\">+", "inc", "increment"),
            ("<input wire:model.live=\"cou\">", "cou", "count"),
            ("<button wire:target=\"save, inc\">+", "inc", "increment"),
        ] {
            let doc = Document {
                path: "/project/resources/views/livewire/counter.blade.php".into(),
                language: "blade".into(),
                version: 1,
                text: source.into(),
            };
            let found = main.completion(
                &doc,
                text::position(source, source.find(needle).unwrap() + needle.len()),
                None,
            );
            assert!(found.iter().any(|i| i.label == label), "{source}");
            assert!(
                !found
                    .iter()
                    .any(|i| matches!(i.label.as_str(), "hidden" | "nested"))
            );
            let full = source.replacen(needle, label, 1);
            let doc = Document {
                text: full.clone(),
                ..doc
            };
            assert!(
                main.definition(&doc, text::position(&full, full.find(label).unwrap() + 2))
                    .iter()
                    .any(|d| d.path.ends_with("Counter.php"))
            );
        }
        for source in [
            "<?php $x='<button wire:click=\"inc\">'; ?>",
            "<script>const x='<button wire:click=\"inc\">';</script>",
            "<button wire:click=\"increment('inc')\">",
        ] {
            let doc = Document {
                path: "/project/resources/views/livewire/counter.blade.php".into(),
                language: "blade".into(),
                version: 1,
                text: source.into(),
            };
            let at = text::position(source, source.rfind("inc").unwrap() + 3);
            assert!(
                !main
                    .completion(&doc, at, None)
                    .iter()
                    .any(|i| i.label == "increment"),
                "{source}"
            );
        }
        let source = "<livewire:counter wire:click=\"inc\">";
        let doc = Document {
            path: "/project/resources/views/other.blade.php".into(),
            language: "blade".into(),
            version: 1,
            text: source.into(),
        };
        assert!(
            !main
                .completion(
                    &doc,
                    text::position(source, source.rfind("inc").unwrap() + 3),
                    None
                )
                .iter()
                .any(|i| i.label == "increment")
        );
        let doc = Document {
            path: "/project/resources/views/livewire/counter.blade.php".into(),
            ..doc
        };
        host.add_file(
            "app/Livewire/Counter.php",
            &class.replace("\\Livewire\\Component", "\\App\\Component"),
        );
        assert!(
            !main
                .completion(
                    &doc,
                    text::position(source, source.rfind("inc").unwrap() + 3),
                    None
                )
                .iter()
                .any(|i| i.label == "increment")
        );
        host.add_file("app/Livewire/Counter.php", class);
        host.add_project("/project/app/Livewire");
        assert!(
            !main
                .completion(
                    &doc,
                    text::position(source, source.rfind("inc").unwrap() + 3),
                    None
                )
                .iter()
                .any(|i| i.label == "increment")
        );
    }
}
