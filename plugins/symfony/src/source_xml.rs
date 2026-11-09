//! Bounded structural literal XML attributes for source diagnostics. No entity resolution.
use std::ops::Range;
fn name(bytes: &[u8], mut at: usize) -> Option<(usize, usize)> {
    let start = at;
    if !bytes
        .get(at)
        .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
    {
        return None;
    }
    at += 1;
    while bytes
        .get(at)
        .is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':'))
    {
        at += 1;
    }
    Some((start, at))
}
fn whitespace(bytes: &[u8], at: &mut usize) {
    while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) {
        *at += 1;
    }
}
/// None means unsupported input or a structural mismatch; this is not a full XML validator.
fn attribute_values(
    text: &str,
    root_name: &str,
    parents: &[&str],
    element: &str,
    attribute: &str,
    descendants: bool,
) -> Option<Vec<(String, Range<usize>)>> {
    if text.len() > 128 << 10 {
        return None;
    }
    let bytes = text.as_bytes();
    let mut at = usize::from(text.starts_with('\u{feff}')) * 3;
    let mut stack: Vec<&str> = Vec::new();
    let mut root = false;
    let mut closed = false;
    let mut facts = Vec::new();
    let mut tokens = 0;
    while at < bytes.len() {
        if bytes[at] != b'<' {
            if stack.is_empty() && !bytes[at].is_ascii_whitespace() {
                return None;
            }
            at += 1;
            continue;
        }
        if text[at..].starts_with("<!--") {
            at += 4;
            at += text[at..].find("-->")? + 3;
            continue;
        }
        if text[at..].starts_with("<![CDATA[") {
            if stack.is_empty() {
                return None;
            }
            at += 9;
            at += text[at..].find("]]>")? + 3;
            continue;
        }
        if text[at..].starts_with("<?") {
            at += 2;
            at += text[at..].find("?>")? + 2;
            continue;
        }
        if text[at..].starts_with("<!") {
            return None;
        }
        tokens += 1;
        if tokens > 4096 {
            return None;
        }
        at += 1;
        if bytes.get(at) == Some(&b'/') {
            at += 1;
            let (start, end) = name(bytes, at)?;
            let tag = &text[start..end];
            at = end;
            whitespace(bytes, &mut at);
            if bytes.get(at) != Some(&b'>') || stack.pop() != Some(tag) {
                return None;
            }
            at += 1;
            if stack.is_empty() {
                closed = true;
            }
            continue;
        }
        let (start, end) = name(bytes, at)?;
        let tag = &text[start..end];
        at = end;
        if stack.is_empty() {
            if root || closed || tag != root_name {
                return None;
            }
            root = true;
        }
        let mut attrs = std::collections::BTreeSet::new();
        let mut key = None;
        let mut empty = false;
        loop {
            let before = at;
            whitespace(bytes, &mut at);
            if bytes.get(at) == Some(&b'>') {
                at += 1;
                break;
            }
            if bytes.get(at..at + 2) == Some(b"/>") {
                at += 2;
                empty = true;
                break;
            }
            if at == before {
                return None;
            }
            let (a, b) = name(bytes, at)?;
            let attr = &text[a..b];
            at = b;
            if !attrs.insert(attr) || attrs.len() > 64 {
                return None;
            }
            whitespace(bytes, &mut at);
            if bytes.get(at) != Some(&b'=') {
                return None;
            }
            at += 1;
            whitespace(bytes, &mut at);
            let quote = *bytes.get(at)?;
            if !matches!(quote, b'\'' | b'"') {
                return None;
            }
            at += 1;
            let value = at;
            while let Some(&byte) = bytes.get(at) {
                if byte == quote {
                    break;
                }
                if byte == b'<' {
                    return None;
                }
                at += 1;
            }
            if bytes.get(at) != Some(&quote) {
                return None;
            }
            if attr == attribute {
                key = Some((&text[value..at], value..at));
            }
            at += 1;
        }
        if tag == element
            && (stack.as_slice() == parents || descendants && stack.starts_with(parents))
        {
            if let Some((value, range)) = key {
                facts.push((value.into(), range));
            }
        }
        if !empty {
            stack.push(tag);
            if stack.len() > 64 {
                return None;
            }
        } else if stack.is_empty() {
            closed = true;
        }
    }
    (root && closed && stack.is_empty()).then_some(facts)
}
pub(crate) fn requirements(text: &str) -> Option<Vec<(String, Range<usize>)>> {
    Some(
        attribute_values(
            text,
            "routes",
            &["routes", "route"],
            "requirement",
            "key",
            false,
        )?
        .into_iter()
        .filter(|(value, _)| matches!(value.as_str(), "_method" | "_scheme"))
        .collect(),
    )
}
pub(crate) fn route_attributes(text: &str, element: &str, attribute: &str) -> Option<Vec<(String, Range<usize>)>> {
    attribute_values(text,"routes",&["routes"],element,attribute,false)
}
pub(crate) fn attribute_name_anchor(text: &str, value: &Range<usize>, attribute: &str) -> Option<Range<usize>> {
    let before=text.get(..value.start.checked_sub(1)?)?;
    let end=before.trim_end().strip_suffix('=')?.trim_end().len();
    let start=end.checked_sub(attribute.len())?;
    (text.get(start..end)==Some(attribute)).then_some(start..end)
}
pub(crate) fn route_ids(text: &str) -> Option<Vec<(String, Range<usize>)>> {
    Some(
        attribute_values(text, "routes", &["routes"], "route", "id", false)?
            .into_iter()
            .filter(|(value, _)| !value.is_empty())
            .collect(),
    )
}
pub(crate) fn service_ids(text: &str) -> Option<Vec<(String, Range<usize>)>> {
    Some(
        attribute_values(
            text,
            "container",
            &["container", "services"],
            "service",
            "id",
            false,
        )?
        .into_iter()
        .filter(|(value, _)| !value.is_empty() && !value.starts_with('.'))
        .collect(),
    )
}

/// Attribute-name anchors of conventional container service declarations.
pub(crate) fn service_attribute_names(text: &str, attribute: &str) -> Option<Vec<Range<usize>>> {
    attribute_values(text,"container",&["container","services"],"service",attribute,true)?.into_iter().map(|(_,range)| {
        let before=text.get(..range.start.checked_sub(1)?)?;
        let name_end=before.trim_end().strip_suffix('=')?.trim_end().len();
        let start=name_end.checked_sub(attribute.len())?;
        (text.get(start..name_end)==Some(attribute)).then_some(start..name_end)
    }).collect()
}

/// Explicit class wins over FQCN service ID, including nested anonymous services.
pub(crate) fn service_classes(text: &str) -> Option<Vec<(String, Range<usize>)>> {
    let mut classes = attribute_values(
        text,
        "container",
        &["container", "services"],
        "service",
        "class",
        true,
    )?;
    // The scanner rejects raw '<' in attributes, so this locates the owning start tag.
    let explicit_tags: std::collections::BTreeSet<_> = classes
        .iter()
        .map(|(_, range)| text[..range.start].rfind('<'))
        .collect();
    classes.extend(
        attribute_values(
            text,
            "container",
            &["container", "services"],
            "service",
            "id",
            true,
        )?
        .into_iter()
        .filter(|(value, range)| {
            value.contains('\\') && !explicit_tags.contains(&text[..range.start].rfind('<'))
        }),
    );
    classes.sort_by_key(|(_, range)| range.start);
    Some(classes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn service_class_values_prefer_explicit_class_and_include_nested_fqcn_ids() {
        let text = r#"<container><!-- <service class="App\Comment"/> --><services><service id="App\Implicit"/><service id="App\Skipped" class=""/><service id="App\Ignored" class="App\Known"/><service id="outer"><argument><service class="App\Nested"/></argument></service></services><service class="App\Outside"/></container>"#;
        let values = service_classes(text).unwrap();
        assert_eq!(
            values
                .iter()
                .map(|(value, _)| value.as_str())
                .collect::<Vec<_>>(),
            [r"App\Implicit", "", r"App\Known", r"App\Nested"]
        );
        for (value, range) in values {
            assert_eq!(&text[range], value);
        }
    }

    #[test]
    fn literal_service_ids_require_container_services_context_and_preserve_ranges() {
        let text = "<container><services><service id='same'/><service id = \"same\"/><service id='.private'/><service id=''/><service id='outer'><argument><service id='nested'/></argument></service></services><other><service id='outside'/></other></container>";
        let values = service_ids(text).unwrap();
        assert_eq!(
            values.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            ["same", "same", "outer"]
        );
        for (id, range) in values {
            assert_eq!(&text[range], id);
        }
        assert!(service_ids("<c:container><services/></c:container>").is_none());
    }

    #[test]
    fn all_scanner_bounds_refuse_without_partial_facts() {
        assert!(requirements(&" ".repeat((128 << 10) + 1)).is_none());
        assert!(
            requirements(&format!(
                "<routes>{}{}</routes>",
                "<node>".repeat(64),
                "</node>".repeat(64)
            ))
            .is_none()
        );
        assert!(requirements(&format!("<routes>{}</routes>", "<node/>".repeat(4096))).is_none());
        let attrs = (0..65).map(|i| format!(" a{i}='x'")).collect::<String>();
        assert!(requirements(&format!("<routes><route{attrs}/></routes>")).is_none());
        assert!(requirements("\u{feff}<routes/>").unwrap().is_empty());
    }
    #[test]
    fn structural_route_keys_ignore_fragments_comments_cdata_and_other_contexts() {
        let text = "<?xml version='1.0'?><routes><!-- <route><requirement key='_method'/></route> --><route id='a' note='a > b'><requirement key = '_method'>GET</requirement><requirement key=\"_scheme\">https</requirement><![CDATA[<requirement key='_method'/>]]><default key='_method'/><nested><requirement key='_scheme'/></nested></route><requirement key='_method'/></routes>";
        let out = requirements(text).unwrap();
        assert_eq!(out.len(), 2);
        for (key, range) in out {
            assert_eq!(&text[range], key);
        }
        for bad in [
            "<routes><route><requirement key='_method'/></routes>",
            "<routes/><routes/>",
            "<!DOCTYPE routes SYSTEM 'file:///outside'><routes/>",
            "<routes><route><requirement key='_method' key='_scheme'/></route></routes>",
            "<routes><!-- missing",
            "<route><requirement key='_method'/></route>",
            "<r:routes><r:route/></r:routes>",
        ] {
            assert!(requirements(bad).is_none(), "{bad}");
        }
    }
}
