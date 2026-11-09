//! Bounded XML namespace ownership, without external DTD/schema resolution.
use quick_xml::{events::Event, reader::NsReader};
// XML1.0 Fifth Edition NameStartChar/NameChar, excluding colon for NCName.
// https://www.w3.org/TR/xml/#NT-NameStartChar
fn name_start(c: char) -> bool {
    matches!(c,'A'..='Z'|'a'..='z'|'_')
        || matches!(c as u32,
        0xc0..=0xd6|0xd8..=0xf6|0xf8..=0x2ff|0x370..=0x37d|0x37f..=0x1fff|
        0x200c..=0x200d|0x2070..=0x218f|0x2c00..=0x2fef|0x3001..=0xd7ff|
        0xf900..=0xfdcf|0xfdf0..=0xfffd|0x10000..=0xeffff)
}
fn ncname(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(name_start)
        && chars.all(|c| {
            name_start(c)
                || matches!(c, '-' | '.' | '0'..='9' | '\u{b7}')
                || matches!(c as u32,0x300..=0x36f|0x203f..=0x2040)
        })
}
pub(crate) fn binding_valid<R>(
    reader: &quick_xml::reader::NsReader<R>,
    element: &quick_xml::events::BytesStart<'_>,
) -> bool {
    use quick_xml::name::ResolveResult;
    const XML: &str = "http://www.w3.org/XML/1998/namespace";
    const XMLNS: &str = "http://www.w3.org/2000/xmlns/";
    let qualified = |name: &str| {
        let mut parts = name.split(':');
        ncname(parts.next().unwrap_or(""))
            && match parts.next() {
                None => true,
                Some(local) => ncname(local) && parts.next().is_none(),
            }
    };
    if !qualified(element.name().as_ref())
        || element.name().as_ref().starts_with("xmlns:")
        || matches!(
            reader.resolver().resolve_element(element.name()).0,
            ResolveResult::Unknown(_)
        )
    {
        return false;
    }
    let mut expanded = std::collections::BTreeSet::new();
    for attribute in element.attributes() {
        let Ok(attribute) = attribute else {
            return false;
        };
        let key = attribute.key.as_ref();
        if !qualified(key) {
            return false;
        }
        if key == "xmlns" || key.starts_with("xmlns:") {
            let Ok(value) = attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0) else {
                return false;
            };
            if key == "xmlns:xmlns"
                || key.starts_with("xmlns:") && value.is_empty()
                || key == "xmlns:xml" && value != XML
                || key != "xmlns:xml" && matches!(value.as_ref(), XML | XMLNS)
            {
                return false;
            }
        } else {
            let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
            let namespace = match namespace {
                ResolveResult::Unbound => "",
                ResolveResult::Bound(ns) => ns.into_inner(),
                ResolveResult::Unknown(_) => return false,
            };
            // Namespace aliases cannot create duplicate expanded attributes.
            // https://www.w3.org/TR/xml-names/#uniqAttrs
            if !expanded.insert((namespace, local.into_inner())) {
                return false;
            }
        }
    }
    true
}

fn inspect(text: &str) -> Option<bool> {
    if text.len() > 512 * 1024 {
        return None;
    }
    let mut reader = NsReader::from_str(text);
    let (mut depth, mut roots, mut count) = (0usize, 0usize, 0usize);
    let mut beans = false;
    let mut doctype = false;
    loop {
        count += 1;
        if count > 65536 {
            return None;
        }
        let event = reader.read_event();
        let empty = matches!(&event, Ok(Event::Empty(_)));
        match event {
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                if !binding_valid(&reader, &element) {
                    return None;
                }
                if depth == 0 {
                    roots += 1;
                    let (namespace, local) = reader.resolver().resolve_element(element.name());
                    beans = local.as_ref() == "beans"
                        && match namespace {
                            quick_xml::name::ResolveResult::Unbound => true,
                            quick_xml::name::ResolveResult::Bound(ns) => {
                                ns.as_ref() == crate::xml_checks::BEANS_NS
                            }
                            _ => false,
                        };
                }
                if !empty {
                    depth += 1;
                    if depth > 128 {
                        return None;
                    }
                }
            }
            Ok(Event::End(_)) => {
                depth = depth.checked_sub(1)?;
            }
            Ok(Event::Text(value))
                if depth == 0 && value.as_ref().bytes().any(|b| !b.is_ascii_whitespace()) =>
            {
                return None;
            }
            Ok(Event::CData(_) | Event::GeneralRef(_)) if depth == 0 => return None,
            Ok(Event::DocType(_)) => {
                if doctype || roots != 0 || depth != 0 {
                    return None;
                }
                doctype = true;
            }
            Ok(Event::Eof) => return (roots == 1 && depth == 0).then_some(beans),
            Err(_) => return None,
            _ => {}
        }
    }
}
pub(crate) fn namespace_document(text: &str) -> bool {
    inspect(text).is_some()
}
pub(crate) fn beans_document(text: &str) -> bool {
    inspect(text) == Some(true)
}

/// Spring schema fields use exact written attribute names. A foreign-prefixed
/// local-name lookalike never supplies an unqualified schema attribute.
pub(crate) trait CoreAttributes {
    fn core_attr(&self, name: &str) -> Option<&spring_text::xml::Attr>;
    fn core_value(&self, name: &str) -> Option<&str> {
        self.core_attr(name).map(|a| a.value.as_str())
    }
}
impl CoreAttributes for spring_text::xml::Element {
    fn core_attr(&self, name: &str) -> Option<&spring_text::xml::Attr> {
        self.attrs.iter().find(|a| a.name == name)
    }
}
/// Shared rule helpers use a local-name lookup. Give only ordinary schema
/// attributes to those helpers; the original elements retain p/c attributes.
pub(crate) fn plain_attributes(
    elements: &[spring_text::xml::Element],
) -> Vec<spring_text::xml::Element> {
    elements
        .iter()
        .cloned()
        .map(|mut e| {
            e.attrs
                .retain(|a| !a.name.contains(':') || a.name.starts_with("xmlns:"));
            e
        })
        .collect()
}

/// Default (legacy) and explicit Beans namespace element ownership.
pub(crate) fn beans_element(elements:&[spring_text::xml::Element],i:usize)->bool {
    spring_text::xml::namespace(elements,i).is_none_or(|ns|ns.is_empty()||ns==crate::xml_checks::BEANS_NS)
}

/// Namespace bound to a written attribute prefix at its owning element.
pub(crate) fn attribute_namespace<'a>(elements: &'a [spring_text::xml::Element], i: usize, attribute: &spring_text::xml::Attr) -> Option<&'a str> {
    let (prefix, _) = attribute.name.split_once(':')?;
    let declaration = format!("xmlns:{prefix}");
    let mut at = Some(i);
    while let Some(index) = at {
        if let Some(binding) = elements[index].attrs.iter().find(|a| a.name == declaration) { return Some(&binding.value); }
        at = elements[index].parent;
    }
    None
}
