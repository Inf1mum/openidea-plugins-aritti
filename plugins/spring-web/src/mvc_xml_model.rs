//! Bounded closed-source MVC schema profiles, not a complete XML/XSD validator.
use openidea_plugin::{
    Location, Position,
    providers::{CompletionItem, CompletionKind, Document},
    text::offset,
};
use spring_text::{
    contains,
    finding::Finding,
    range,
    xml::{self, Element},
};
use std::collections::{BTreeMap, BTreeSet};
const MVC: &str = "http://www.springframework.org/schema/mvc";
const BEANS: &str = "http://www.springframework.org/schema/beans";
const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";
const XML: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS: &str = "http://www.w3.org/2000/xmlns/";
struct Rule {
    path: &'static str,
    profiles: u16,
    required: &'static [&'static str],
    booleans: &'static [&'static str],
    integers: &'static [&'static str],
    longs: &'static [&'static str],
    refs: &'static [&'static str],
}
#[path = "mvc_xml_tables.rs"]
mod tables;
fn name(s: &str) -> bool {
    let mut parts = s.split(':');
    let first = parts.next().unwrap_or("");
    let valid = |p: &str| {
        p.as_bytes()
            .first()
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
            && p.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
    };
    valid(first) && parts.next().is_none_or(valid) && parts.next().is_none()
}
fn uri<'a>(es: &'a [Element], mut i: usize, prefix: &str) -> Option<&'a str> {
    if prefix == "xml" {
        return Some(XML);
    }
    let key = if prefix.is_empty() {
        "xmlns".into()
    } else {
        format!("xmlns:{prefix}")
    };
    loop {
        if let Some(a) = es[i].attrs.iter().find(|a| a.name == key) {
            return Some(&a.value);
        }
        i = es[i].parent?;
    }
}
fn closed(text: &str, es: &[Element]) -> bool {
    if text.len() > 256 << 10
        || text
            .chars()
            .any(|c| (c as u32) < 32 && !matches!(c, '\t' | '\n' | '\r'))
        || text.contains('&')
        || es.is_empty()
        || es.len() > 2048
    {
        return false;
    }
    let mut stack = vec![];
    let mut at = 0;
    let mut element = 0;
    let mut roots = 0;
    while let Some(next) = text[at..].find('<').map(|v| at + v) {
        if text[at..next].contains("]]>") || stack.is_empty() && !text[at..next].trim().is_empty() {
            return false;
        }
        let tail = &text[next..];
        if tail.starts_with("<!--") {
            let Some(end) = tail.find("-->") else {
                return false;
            };
            if tail[4..end].contains("--") {
                return false;
            }
            at = next + end + 3;
            continue;
        }
        if tail.starts_with("<?") {
            let Some(end) = tail.find("?>") else {
                return false;
            };
            let target = tail[2..end].split_whitespace().next().unwrap_or("");
            if target.eq_ignore_ascii_case("xml") && (target != "xml" || next != 0) {
                return false;
            }
            at = next + end + 2;
            continue;
        }
        if tail.starts_with("<![CDATA[") {
            if stack.is_empty() {
                return false;
            }
            let Some(end) = tail.find("]]>") else {
                return false;
            };
            at = next + end + 3;
            continue;
        }
        if tail.starts_with("<!") {
            return false;
        }
        if tail.starts_with("</") {
            let Some(end) = tail.find('>') else {
                return false;
            };
            let written = tail[2..end].trim_end();
            if !name(written) || stack.pop() != Some(written) {
                return false;
            }
            at = next + end + 1;
            continue;
        }
        let Some(e) = es.get(element).filter(|e| e.start == next) else {
            return false;
        };
        let i = element;
        element += 1;
        if !name(&e.name)
            || e.tag_end > text.len()
            || text.as_bytes().get(e.tag_end.saturating_sub(1)) != Some(&b'>')
        {
            return false;
        }
        let prefix = e.name.split_once(':').map_or("", |p| p.0);
        if !prefix.is_empty() && uri(es, i, prefix).is_none() {
            return false;
        }
        if prefix == "xmlns" {
            return false;
        }
        let mut seen = BTreeSet::new();
        let mut cursor = e.at + e.name.len();
        for a in &e.attrs {
            if !name(&a.name)
                || !text[cursor..a.at].trim().is_empty()
                || a.value_at == 0
                || a.value_end >= e.tag_end
            {
                return false;
            }
            let quote = text.as_bytes()[a.value_at - 1];
            if !matches!(quote, b'\'' | b'"')
                || text.as_bytes()[a.value_end] != quote
                || text[a.at + a.name.len()..a.value_at - 1].trim() != "="
                || a.value.contains('<')
            {
                return false;
            }
            let (prefix, local) = a.name.split_once(':').unwrap_or(("", &a.name));
            let namespace = if a.name == "xmlns" || prefix == "xmlns" {
                if local == "xmlns" && prefix == "xmlns"
                    || local == "xml" && prefix == "xmlns" && a.value != XML
                    || a.value == XMLNS
                    || a.value == XML && (prefix != "xmlns" || local != "xml")
                    || prefix == "xmlns" && a.value.is_empty()
                {
                    return false;
                }
                XMLNS
            } else if prefix.is_empty() {
                ""
            } else {
                let Some(value) = uri(es, i, prefix) else {
                    return false;
                };
                value
            };
            if !seen.insert((namespace, local)) {
                return false;
            }
            cursor = a.value_end + 1;
        }
        let rest = text[cursor..e.tag_end - 1].trim();
        if !matches!(rest, "" | "/") {
            return false;
        }
        if stack.is_empty() {
            roots += 1;
            if roots > 1 {
                return false;
            }
        }
        if rest != "/" {
            stack.push(e.name.as_str());
            if stack.len() > 64 {
                return false;
            }
        }
        at = e.tag_end;
    }
    stack.is_empty()
        && roots == 1
        && element == es.len()
        && text[at..].trim().is_empty()
        && !text[at..].contains("]]>")
}
fn profile(es: &[Element]) -> Option<u16> {
    let mut selected = None;
    for (i, e) in es.iter().enumerate() {
        for a in &e.attrs {
            let Some((prefix, local)) = a.name.split_once(':') else {
                continue;
            };
            if local != "schemaLocation" || uri(es, i, prefix) != Some(XSI) {
                continue;
            }
            let pairs = a.value.split_whitespace().collect::<Vec<_>>();
            if pairs.len() % 2 != 0 {
                return None;
            }
            for pair in pairs.chunks_exact(2).filter(|p| p[0] == MVC) {
                let file = pair[1].rsplit('/').next()?;
                let bit = match file {
                    "spring-mvc-3.0.xsd" => 0,
                    "spring-mvc-3.1.xsd" => 1,
                    "spring-mvc-3.2.xsd" => 2,
                    "spring-mvc-4.0.xsd" => 3,
                    "spring-mvc-4.1.xsd" => 4,
                    "spring-mvc-4.2.xsd" => 5,
                    "spring-mvc-4.3.xsd" => 6,
                    "spring-mvc-5.3.xsd" => 7,
                    "spring-mvc-6.2.xsd" => 8,
                    "spring-mvc.xsd" | "spring-mvc-7.0.xsd" | "spring-mvc-7.1.xsd" => 9,
                    _ => return None,
                };
                let value = 1 << bit;
                if selected.is_some_and(|old| old != value) {
                    return None;
                }
                selected = Some(value);
            }
        }
    }
    Some(selected.unwrap_or(1 << 9))
}
fn path(es: &[Element], mut i: usize) -> Option<String> {
    let mut names = vec![];
    loop {
        if xml::namespace(es, i) != Some(MVC) {
            return None;
        }
        names.push(es[i].local());
        if names.len() > 16 {
            return None;
        }
        match es[i].parent {
            None => break,
            Some(parent) if xml::namespace(es, parent) == Some(MVC) => i = parent,
            Some(parent)
                if parent == 0
                    && es[parent].local() == "beans"
                    && matches!(xml::namespace(es, parent), Some(BEANS) | None) =>
            {
                break;
            }
            _ => return None,
        }
    }
    names.reverse();
    Some(names.join("/"))
}
fn model(text: &str) -> Option<(Vec<Element>, u16)> {
    let es = xml::elements(text);
    if !closed(text, &es) {
        return None;
    }
    let profile = profile(&es)?;
    Some((es, profile))
}
fn dynamic(s: &str) -> bool {
    s.contains("${") || s.contains("#{") || s.contains('\\')
}
pub(super) fn check(text: &str) -> Vec<Finding> {
    let Some((es, profile)) = model(text) else {
        return vec![];
    };
    let mut out = vec![];
    for (i, e) in es.iter().enumerate() {
        let Some(path) = path(&es, i) else {
            continue;
        };
        for rule in tables::RULES
            .iter()
            .filter(|r| r.path == path && r.profiles & profile != 0)
        {
            for required in rule.required {
                if !e.attrs.iter().any(|a| a.name == *required) {
                    out.push(Finding::new(
                        e.at,
                        e.at + e.name.len(),
                        "missing-attribute",
                        format!("<{}> needs the attribute \"{}\".", e.name, required),
                    ));
                }
            }
            for a in &e.attrs {
                if dynamic(&a.value) {
                    continue;
                }
                let value = a.value.trim();
                if rule.booleans.contains(&a.name.as_str())
                    && !matches!(value, "true" | "false" | "1" | "0")
                {
                    out.push(Finding::new(
                        a.value_at,
                        a.value_end,
                        "invalid-attribute-value",
                        format!(
                            "\"{}\" is not an XML Schema boolean; use true, false, 1 or 0.",
                            a.value
                        ),
                    ));
                }
                if rule.integers.contains(&a.name.as_str()) && value.parse::<i32>().is_err()
                    || rule.longs.contains(&a.name.as_str()) && value.parse::<i64>().is_err()
                {
                    out.push(Finding::new(
                        a.value_at,
                        a.value_end,
                        "invalid-attribute-value",
                        format!(
                            "\"{}\" is not an XML Schema integer in the declared range.",
                            a.value
                        ),
                    ));
                }
            }
        }
        if out.len() > 256 {
            out.truncate(256);
            break;
        }
    }
    out
}
fn refs(
    es: &[Element],
    profile: u16,
    text: &str,
) -> Option<(
    BTreeMap<String, openidea_plugin::Range>,
    Vec<(String, openidea_plugin::Range)>,
)> {
    if es[0].local() != "beans"
        || xml::namespace(es, 0) != Some(BEANS)
        || es[0].attrs.iter().any(|a| a.name == "profile")
    {
        return None;
    }
    if es.iter().enumerate().skip(1).any(|(i, e)| {
        xml::namespace(es, i) == Some(BEANS) && matches!(e.local(), "import" | "alias" | "beans")
    }) {
        return None;
    }
    let plain = |s: &str| {
        !s.is_empty()
            && s.len() <= 128
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    };
    let mut beans = BTreeMap::new();
    let mut duplicates = BTreeSet::new();
    let mut refs = vec![];
    for (i, e) in es.iter().enumerate() {
        if e.parent == Some(0) && xml::namespace(es, i) == Some(BEANS) && e.local() == "bean" {
            if e.attrs.iter().any(|a| a.name == "name") {
                return None;
            }
            if let Some(id) = e.attrs.iter().find(|a| a.name == "id") {
                if !plain(&id.value) {
                    return None;
                }
                if beans
                    .insert(id.value.clone(), range(text, id.value_at, id.value_end))
                    .is_some()
                {
                    duplicates.insert(id.value.clone());
                }
            }
        }
        if let Some(path) = path(es, i) {
            for rule in tables::RULES
                .iter()
                .filter(|r| r.path == path && r.profiles & profile != 0)
            {
                for a in e
                    .attrs
                    .iter()
                    .filter(|a| rule.refs.contains(&a.name.as_str()))
                {
                    if a.value.is_empty() || plain(&a.value) {
                        refs.push((a.value.clone(), range(text, a.value_at, a.value_end)));
                    }
                }
            }
        }
        if e.local() == "ref"
            && xml::namespace(es, i) == Some(BEANS)
            && e.parent.and_then(|p| path(es, p)).is_some_and(|path| {
                tables::REF_PARENTS
                    .iter()
                    .any(|(p, mask)| *p == path && mask & profile != 0)
            })
        {
            if e.attrs.len() == 1 {
                if let Some(a) = e.attrs.iter().find(|a| a.name == "bean") {
                    if a.value.is_empty() || plain(&a.value) {
                        refs.push((a.value.clone(), range(text, a.value_at, a.value_end)));
                    }
                }
            }
        }
    }
    for duplicate in duplicates {
        beans.remove(&duplicate);
    }
    Some((beans, refs))
}
pub(super) fn definition(doc: &Document, at: Position) -> Vec<Location> {
    let Some((es, profile)) = model(&doc.text) else {
        return vec![];
    };
    let Some((beans, refs)) = refs(&es, profile, &doc.text) else {
        return vec![];
    };
    let Some((name, _)) = refs.iter().find(|(_, r)| contains(*r, at)) else {
        return vec![];
    };
    beans
        .get(name)
        .map(|range| {
            vec![Location {
                path: doc.path.clone(),
                range: *range,
            }]
        })
        .unwrap_or_default()
}
pub(super) fn completion(doc: &Document, at: Position) -> Vec<CompletionItem> {
    let Some((es, profile)) = model(&doc.text) else {
        return vec![];
    };
    for (i, e) in es.iter().enumerate() {
        if let Some(path) = path(&es, i) {
            for rule in tables::RULES
                .iter()
                .filter(|r| r.path == path && r.profiles & profile != 0)
            {
                for a in e
                    .attrs
                    .iter()
                    .filter(|a| rule.booleans.contains(&a.name.as_str()))
                {
                    let span = range(&doc.text, a.value_at, a.value_end);
                    if span.start <= at && at <= span.end && !dynamic(&a.value) {
                        let Some(end) = offset(&doc.text, at) else {
                            return vec![];
                        };
                        let prefix = &doc.text[a.value_at..end];
                        return ["true", "false", "1", "0"]
                            .into_iter()
                            .filter(|v| v.starts_with(prefix))
                            .map(|v| CompletionItem {
                                replace: Some(span),
                                detail: Some("MVC XML Schema boolean".into()),
                                ..CompletionItem::new(v, CompletionKind::Value)
                            })
                            .collect();
                    }
                }
            }
        }
    }
    let Some((beans, refs)) = refs(&es, profile, &doc.text) else {
        return vec![];
    };
    let Some((_, span)) = refs.iter().find(|(_, r)| r.start <= at && at <= r.end) else {
        return vec![];
    };
    let Some(start) = offset(&doc.text, span.start) else {
        return vec![];
    };
    let Some(end) = offset(&doc.text, at) else {
        return vec![];
    };
    let prefix = &doc.text[start..end];
    beans
        .into_keys()
        .filter(|name| name.starts_with(prefix))
        .take(64)
        .map(|name| CompletionItem {
            replace: Some(*span),
            detail: Some(
                "Declared bean ID in this XML document; effective type/context not resolved".into(),
            ),
            ..CompletionItem::new(&name, CompletionKind::Reference)
        })
        .collect()
}
