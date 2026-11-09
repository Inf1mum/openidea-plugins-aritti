//! Scoped XML namespace admission, independent of framework/runtime ownership.
pub(crate) fn valid<R>(reader: &quick_xml::reader::NsReader<R>, element: &quick_xml::events::BytesStart<'_>) -> bool {
    use quick_xml::name::ResolveResult;
    const XML: &str = "http://www.w3.org/XML/1998/namespace";
    const XMLNS: &str = "http://www.w3.org/2000/xmlns/";
    if element.name().as_ref().starts_with("xmlns:")
        || matches!(reader.resolver().resolve_element(element.name()).0, ResolveResult::Unknown(_))
    {
        return false;
    }
    for attribute in element.attributes() {
        let Ok(attribute) = attribute else {
            return false;
        };
        let key = attribute.key.as_ref();
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
        } else if matches!(reader.resolver().resolve_attribute(attribute.key).0, ResolveResult::Unknown(_)) {
            return false;
        }
    }
    true
}

/// Guard legacy navigation fallback as well as the newer local model. Bounded
/// namespace validation must fail closed; it is not an effective framework model.
pub(crate) fn document_valid(text: &str) -> bool {
    if text.len() > 512 * 1024 {
        return false;
    }
    let mut reader = quick_xml::reader::NsReader::from_str(text);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(10);
    let mut depth = 0usize;
    let mut roots = 0usize;
    let mut declaration = false;
    for _ in 0..8192 {
        if std::time::Instant::now() >= deadline { return false; }
        use quick_xml::events::Event;
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                if !valid(&reader,&e) { return false; }
                if depth == 0 { roots += 1; }
                depth += 1;
                if roots > 1 || depth > 128 { return false; }
            }
            Ok(Event::Empty(e)) => {
                if !valid(&reader,&e) { return false; }
                if depth == 0 { roots += 1; }
                if roots > 1 { return false; }
            }
            Ok(Event::End(_)) => {
                let Some(next) = depth.checked_sub(1) else { return false; };
                depth = next;
            }
            Ok(Event::Text(t)) if depth == 0 && !t.as_ref().trim_ascii().is_empty() => return false,
            Ok(Event::CData(_)) if depth == 0 => return false,
            Ok(Event::Decl(_)) => {
                if depth != 0 || roots != 0 || declaration { return false; }
                declaration = true;
            }
            Ok(Event::DocType(_) | Event::GeneralRef(_)) => return false,
            Ok(Event::Eof) => return depth == 0 && roots == 1,
            Err(_) => return false,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reserved_aliases_defaults_and_escapes_are_invalid_at_any_depth() {
        for declaration in [
            "xmlns:xml='urn:foreign'",
            "xmlns:xmlns='urn:foreign'",
            "xmlns:q='http://www.w3.org/XML/1998/namespace'",
            "xmlns:q='http://www.w3.org/2000/xmlns/'",
            "xmlns='http://www.w3.org/XML/1998/namespace'",
            "xmlns='http://www.w3.org/2000/xmlns/'",
            "xmlns:q='http://www.w3.org/XML/1998/namespac&#101;'",
            "xmlns='http://www.w3.org/2000/xmln&#115;/'",
            "xmlns:q=''",
        ] {
            for text in [format!("<root {declaration}/>"), format!("<root><child {declaration}/></root>")] {
                assert!(!document_valid(&text), "{declaration}");
            }
        }
        assert!(!document_valid("<root><unbound:child/></root>"));
        assert!(!document_valid("<root unbound:attr='x'/>"));
    }
    #[test]
    fn legal_xml_prefix_and_normal_namespace_shadowing_remain_valid() {
        for text in [
            "<root xml:lang='en'/>",
            "<root xmlns:xml='http://www.w3.org/XML/1998/namespace' xml:lang='en'/>",
            "<root xmlns:q='urn:outer'><q:child xmlns:q='urn:inner'/><q:sibling/></root>",
            "<root xmlns='urn:outer'><child xmlns=''/></root>",
        ] {
            assert!(document_valid(text));
        }
    }
}
