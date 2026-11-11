//! Literal PHPDoc constructor unions offered only for the owning component's
//! ordinary string attributes. Bound PHP expressions retain PHP completion.
use openidea_plugin::{
    Position, Range,
    providers::{CompletionItem, CompletionKind, Document},
    text,
};
fn context(source: &str, at: usize) -> Option<(String, String, std::ops::Range<usize>, bool)> {
    if at > source.len() || !source.is_char_boundary(at) {
        return None;
    }
    let visible = crate::refs::without_comments(source);
    let start = visible[..at].rfind('<')?;
    let raw = visible[start + 1..].strip_prefix("x-")?;
    let n = raw
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
        .count();
    let name = &raw[..n];
    if !crate::refs::valid_tag_name(name, false) {
        return None;
    }
    let bytes = source.as_bytes();
    let mut i = start + 3 + n;
    while i < at {
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        let a = i;
        while bytes
            .get(i)
            .is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'-' | b'_' | b'.'))
        {
            i += 1;
        }
        if i == a {
            return None;
        }
        let attr = &source[a..i];
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
        let quote = *bytes.get(i)?;
        if !matches!(quote, b'\'' | b'"') {
            return None;
        }
        i += 1;
        let value = i;
        while i < source.len() && bytes[i] != quote {
            i += 1;
        }
        if at >= value && at <= i {
            if attr.starts_with("::")
                || source[value..i].contains(['$', '{', '\n', '\r'])
                || !attr.starts_with(':') && source[value..i].contains('\\')
            {
                return None;
            }
            return Some((
                name.into(),
                attr.trim_start_matches(':').into(),
                value..i,
                attr.starts_with(':'),
            ));
        }
        if i >= source.len() {
            return None;
        }
        i += 1;
    }
    None
}
fn values(source: &str, attribute: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Some(code) = crate::package_facts::code(source) else {
        return out;
    };
    let Some(method) = code.find("function __construct") else {
        return out;
    };
    let before = &source[..method];
    let Some(close) = before.rfind("*/") else {
        return out;
    };
    if !before[close + 2..]
        .trim()
        .chars()
        .all(|c| c.is_ascii_alphabetic() || c.is_whitespace())
    {
        return out;
    }
    let Some(open) = before[..close].rfind("/**") else {
        return out;
    };
    let comment = &source[open + 3..close];
    for line in comment.lines() {
        let Some(param) = line.find("@param ") else {
            continue;
        };
        let tail = line[param + 7..].trim();
        let Some(dollar) = tail.find('$') else {
            continue;
        };
        let name = tail[dollar + 1..]
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .next()
            .unwrap_or("");
        if crate::components::attribute_name(name) != attribute {
            continue;
        }
        let union = tail[..dollar].trim();
        for part in union.split('|') {
            let part = part.trim();
            let Some(value) = part
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
                .or_else(|| part.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
            else {
                return vec![];
            };
            if value.is_empty() || value.contains(['\'', '"', '\\', '$', '\n', '\r']) {
                return vec![];
            }
            out.push(value.into());
        }
    }
    out.sort();
    out.dedup();
    out
}
pub(crate) fn completion(
    index: &crate::Index,
    doc: &Document,
    at: Position,
) -> Vec<CompletionItem> {
    let run = || {
        if doc.language != "blade" || doc.text.len() > 256 * 1024 {
            return None;
        }
        let at = text::offset(&doc.text, at)?;
        let (name, attribute, range, bound) = context(&doc.text, at)?;
        let entry = index.components.get(&name)?;
        if entry.path.ends_with(".blade.php") {
            return None;
        }
        let project = crate::project_for(&doc.path)?;
        if !crate::owns_current_path(&project, &entry.path)
            || project.stat(&entry.path).ok()?.size > 128 * 1024
        {
            return None;
        }
        let source = project.read_file(&entry.path).ok()?;
        let props = crate::components::constructor_props(&source);
        if !props
            .iter()
            .any(|p| crate::components::attribute_name(&p.name) == attribute)
        {
            return None;
        }
        let typed = &doc.text[range.start..at];
        let enum_items = enum_items(
            index,
            &project,
            &source,
            &attribute,
            bound,
            typed,
            &doc.text,
            range.clone(),
        );
        if !enum_items.is_empty() {
            return Some(enum_items);
        }
        if bound {
            return None;
        }
        Some(
            values(&source, &attribute)
                .into_iter()
                .filter(|v| v.starts_with(typed))
                .map(|v| {
                    let mut item = CompletionItem::new(&v, CompletionKind::Value);
                    item.insert_text = v;
                    item.replace = Some(Range::new(
                        text::position(&doc.text, range.start),
                        text::position(&doc.text, range.end),
                    ));
                    item
                })
                .collect(),
        )
    };
    run().unwrap_or_default()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn values_only_from_constructor_doc_and_plain_attribute() {
        let source = "<?php class Card { /** @param 'small'|'large' $size */ public function __construct(public string $size) {} }";
        assert_eq!(values(source, "size"), ["large", "small"]);
        assert!(values(source, "other").is_empty());
        assert!(values(&source.replace("'small'|'large'", "string|int"), "size").is_empty());
        let s = "<x-ui::card size=\"sm\"/>";
        assert_eq!(context(s, s.find("sm").unwrap() + 1).unwrap().0, "ui::card");
        assert!(context("<x-card :size=\"sm\"/>", 16).unwrap().3);
    }
}
#[cfg(test)]
mod provider_tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, providers::Providers};
    #[test]
    fn phpdoc_value_union_reaches_provider_and_tracks_current_owned_class() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("artisan", "<?php");
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        let class = "<?php\nnamespace App\\View\\Components;\nclass Card {\n /** @param 'small'|'large' $size */\n public function __construct(public string $size) {}\n}";
        host.add_file("app/View/Components/Card.php", class);
        let index = crate::Index::build(&host.invocation().project().unwrap());
        let mut main = crate::Main::default();
        main.indexes.insert("/project".into(), index);
        let source = "😀<x-card size=\"sm\"/>";
        let doc = Document {
            path: "/project/resources/views/a.blade.php".into(),
            language: "blade".into(),
            version: 1,
            text: source.into(),
        };
        let at = text::position(source, source.find("sm").unwrap() + 2);
        let found = main.completion(&doc, at, None);
        assert!(found.iter().any(|i| i.label == "small"));
        host.add_file(
            "app/View/Components/Card.php",
            &class.replace("'small'|'large'", "'medium'"),
        );
        assert!(
            !main
                .completion(&doc, at, None)
                .iter()
                .any(|i| i.label == "small")
        );
        host.add_project("/project/app/View/Components");
        assert!(main.completion(&doc, at, None).is_empty());
    }
}

fn enum_targets(source: &str, attribute: &str) -> Vec<String> {
    let mut out = std::collections::BTreeSet::new();
    let Some(code) = crate::package_facts::code(source) else {
        return vec![];
    };
    let Some(method) = code.find("function __construct") else {
        return vec![];
    };
    let Some(open) = code[method..].find('(').map(|n| n + method) else {
        return vec![];
    };
    let mut types = Vec::new();
    for arg in crate::php::arguments(source, open) {
        let raw = &code[arg];
        let Some(dollar) = raw.find('$') else {
            continue;
        };
        let name = raw[dollar + 1..]
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .next()
            .unwrap_or("");
        if crate::components::attribute_name(name) == attribute {
            if let Some(ty) = raw[..dollar].split_whitespace().last() {
                types.push(ty.to_owned());
            }
        }
    }
    let before = &source[..method];
    if let Some(close) = before.rfind("*/") {
        if before[close + 2..]
            .trim()
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c.is_whitespace())
        {
            if let Some(open) = before[..close].rfind("/**") {
                for line in before[open + 3..close].lines() {
                    let Some(at) = line.find("@param ") else {
                        continue;
                    };
                    let tail = line[at + 7..].trim();
                    let Some(dollar) = tail.find('$') else {
                        continue;
                    };
                    let name = tail[dollar + 1..]
                        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                        .next()
                        .unwrap_or("");
                    if crate::components::attribute_name(name) == attribute {
                        types.push(tail[..dollar].trim().into());
                    }
                }
            }
        }
    }
    for ty in types {
        for class in ty.split('|').take(16) {
            let class = class.trim().trim_start_matches('?').trim_end_matches("::*");
            if let Some(full) =
                crate::eloquent_members::metadata_inference::class_identity(source, class)
            {
                out.insert(full);
            }
        }
    }
    out.into_iter().take(16).collect()
}
fn enum_items(
    index: &crate::Index,
    project: &openidea_plugin::Project,
    source: &str,
    attribute: &str,
    bound: bool,
    typed: &str,
    document: &str,
    range: std::ops::Range<usize>,
) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    for full in enum_targets(source, attribute) {
        let Some((path, cases, backing)) =
            crate::eloquent_members::enum_values(index, &full, &|p| {
                crate::owns_current_path(project, p)
            })
        else {
            continue;
        };
        if project.stat(&path).ok().is_none_or(|s| s.size > 128 * 1024) {
            continue;
        }
        let Ok(current) = project.read_file(&path) else {
            continue;
        };
        if current.len().saturating_mul(cases.len()) > 4 * 1024 * 1024 {
            continue;
        }
        let Some(code) = crate::package_facts::code(&current) else {
            continue;
        };
        let Some(fact) = crate::eloquent_members::metadata_inference::class_fact(&current, &path)
        else {
            continue;
        };
        if fact.class_kind != "enum" || !fact.full.eq_ignore_ascii_case(&full) {
            continue;
        }
        let Some(declaration) = text::offset(&current, fact.entry.at) else {
            continue;
        };
        let Some(body) = code[declaration..].find('{').map(|n| n + declaration) else {
            continue;
        };
        let actual_backing = code[declaration..body]
            .split_once(':')
            .and_then(|(_, tail)| tail.trim().split_whitespace().next());
        if actual_backing != backing.as_deref() {
            continue;
        }
        for (name, value, entry) in cases {
            let Some(at) = text::offset(&current, entry.at) else {
                continue;
            };
            if !code[at..].starts_with(&name) {
                continue;
            }
            let (label, insertion) = if bound {
                (name.clone(), format!("\\{full}::{name}"))
            } else {
                let Some(value) = value else {
                    continue;
                };
                let insertion = match (backing.as_deref(), value) {
                    (Some("string"), serde_json::Value::String(s)) => s,
                    (Some("int"), serde_json::Value::Number(n)) => n.to_string(),
                    _ => continue,
                };
                if insertion.contains(['\'', '"', '\\', '$', '\n', '\r']) {
                    continue;
                }
                // A cached backing literal must still agree with the current owned enum.
                let rest = current[at + name.len()..].trim_start();
                let Some(raw) = rest
                    .strip_prefix('=')
                    .and_then(|s| s.split(';').next())
                    .map(str::trim)
                else {
                    continue;
                };
                let equal = if backing.as_deref() == Some("string") {
                    let start = current.len() - rest.len() + 1;
                    let literal = crate::php::trimmed(
                        &current,
                        start..start + rest[1..].find(';').unwrap_or(0),
                    );
                    crate::php::literal(&current, literal).is_some_and(|(v, _)| v == insertion)
                } else {
                    raw.parse::<i64>()
                        .ok()
                        .is_some_and(|v| v.to_string() == insertion)
                };
                if !equal {
                    continue;
                }
                (insertion.clone(), insertion)
            };
            if !(label.starts_with(typed) || insertion.starts_with(typed)) {
                continue;
            }
            let mut item = CompletionItem::new(&label, CompletionKind::Value);
            item.insert_text = insertion;
            item.detail = Some(format!("{full}::{name}"));
            item.replace = Some(Range::new(
                text::position(document, range.start),
                text::position(document, range.end),
            ));
            out.push(item);
        }
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}
#[cfg(test)]
mod enum_provider_tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, providers::Providers};
    #[test]
    fn owned_cached_backed_enums_offer_values_and_bound_cases_without_reading_foreign_sources() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("artisan", "<?php");
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        let enumeration =
            "<?php namespace App\\Enums; enum Size:string {case Small='small';case Large='large';}";
        host.add_file("app/Enums/Size.php", enumeration);
        host.add_file("app/View/Components/Card.php","<?php\nnamespace App\\View\\Components;\nuse App\\Enums\\Size;\nclass Card {\n/** @param Size $size */\npublic function __construct(public string $size) {}\n}");
        let index = crate::Index::build(&host.invocation().project().unwrap());
        let mut main = crate::Main::default();
        main.indexes.insert("/project".into(), index);
        let source = "<x-card size=\"sm\"/>";
        let doc = Document {
            path: "/project/resources/views/a.blade.php".into(),
            language: "blade".into(),
            version: 1,
            text: source.into(),
        };
        let at = text::position(source, source.find("sm").unwrap() + 2);
        assert!(
            main.completion(&doc, at, None)
                .iter()
                .any(|i| i.label == "small")
        );
        let source = "<x-card :size=\"Sm\"/>";
        let bound = Document {
            text: source.into(),
            ..doc.clone()
        };
        let found = main.completion(
            &bound,
            text::position(source, source.find("Sm").unwrap() + 2),
            None,
        );
        assert!(
            found
                .iter()
                .any(|i| i.label == "Small" && i.insert_text == "\\App\\Enums\\Size::Small")
        );
        host.add_file(
            "app/Enums/Size.php",
            &enumeration.replace("'small'", "'compact'"),
        );
        assert!(
            !main
                .completion(&doc, at, None)
                .iter()
                .any(|i| i.label == "small")
        );
        host.add_project("/project/app/Enums");
        assert!(
            !main
                .completion(
                    &bound,
                    text::position(source, source.find("Sm").unwrap() + 2),
                    None
                )
                .iter()
                .any(|i| i.label == "Small")
        );
    }
}
pub(crate) fn definition(
    index: &crate::Index,
    doc: &Document,
    at: Position,
) -> Vec<openidea_plugin::Location> {
    let run = || {
        let at = text::offset(&doc.text, at)?;
        let (name, attribute, range, bound) = context(&doc.text, at)?;
        let project = crate::project_for(&doc.path)?;
        let entry = index.components.get(&name)?;
        if !crate::owns_current_path(&project, &entry.path)
            || project.stat(&entry.path).ok()?.size > 128 * 1024
        {
            return None;
        }
        let source = project.read_file(&entry.path).ok()?;
        let selected = &doc.text[range.clone()];
        let items = enum_items(
            index, &project, &source, &attribute, bound, "", &doc.text, range,
        );
        let mut found = Vec::new();
        for item in items {
            if item.insert_text != selected {
                continue;
            }
            let detail = item.detail?;
            let (full, case) = detail.rsplit_once("::")?;
            let (_, cases, _) = crate::eloquent_members::enum_values(index, full, &|p| {
                crate::owns_current_path(&project, p)
            })?;
            for (name, _, entry) in cases {
                if name == case {
                    if let Some(location) = crate::scoped_location(&project, &entry) {
                        found.push(location);
                    }
                }
            }
        }
        (found.len() == 1).then_some(found)
    };
    run().unwrap_or_default()
}
