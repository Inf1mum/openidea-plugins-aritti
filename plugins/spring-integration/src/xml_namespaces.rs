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
    if text.len() > 512 * 1024 || text.contains('&') {
        return false;
    }
    let mut reader = quick_xml::reader::NsReader::from_str(text);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(10);
    let (mut roots,mut depth,mut declarations)=(0usize,0usize,0usize);
    for _ in 0..8192 {
        if std::time::Instant::now() >= deadline {
            return false;
        }
        match reader.read_event() {
            Ok(quick_xml::events::Event::Start(e)) => {
                if !valid(&reader,&e) || depth==64 {return false;}
                if depth==0 {roots+=1;}
                depth+=1;
            },
            Ok(quick_xml::events::Event::Empty(e)) => {
                if !valid(&reader,&e) {return false;}
                if depth==0 {roots+=1;}
            },
            Ok(quick_xml::events::Event::End(_)) => {
                let Some(next)=depth.checked_sub(1) else {return false;};depth=next;
            },
            Ok(quick_xml::events::Event::Text(value)) if depth==0 && !value.as_ref().bytes().all(|b|b.is_ascii_whitespace()) => return false,
            Ok(quick_xml::events::Event::CData(_)) if depth==0 => return false,
            Ok(quick_xml::events::Event::DocType(_) | quick_xml::events::Event::GeneralRef(_)) => return false,
            Ok(quick_xml::events::Event::Decl(_)) => {
                if depth!=0 || roots!=0 || declarations!=0 || !text.strip_prefix('\u{feff}').unwrap_or(text).starts_with("<?xml") {return false;}
                declarations+=1;
            },
            Ok(quick_xml::events::Event::Comment(value)) if value.as_ref().contains("--") || value.as_ref().ends_with('-') => return false,
            Ok(quick_xml::events::Event::Eof) => return roots==1 && depth==0,
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
    #[test]
    fn complete_xml_envelope_is_required_before_legacy_source_facts() {
        for text in ["", "<root>","<root/><other/>","outside<root/>","<root/>outside","<![CDATA[outside]]><root/>","<!DOCTYPE root><root/>","<root value='a&amp;b'/>"," <?xml version='1.0'?><root/>","<?xml version='1.0'?><?xml version='1.0'?><root/>","<root><!-- bad -- comment --></root>"] {
            assert!(!document_valid(text),"{text}");
        }
        for text in ["<root/>","<!-- note --><root/><!-- end -->","<?xml version='1.0'?><root/>","\u{feff}<?xml version='1.0'?><root/>"] {assert!(document_valid(text),"{text}");}
        let nested=format!("{}{}","<r>".repeat(65),"</r>".repeat(65));assert!(!document_valid(&nested));
    }
}
