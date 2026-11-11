//! Literal resource properties/constructor inputs of exact source-declared Spring-WS classes.
//! Original SpringWebServicesResourceTypeProvider class/property filters only.
use crate::{generate, xml_namespace};
use openidea_plugin::{Range, providers::Document};
use spring_text::{range, xml};
const BEANS: &str = "http://www.springframework.org/schema/beans";
const MAX_TEXT: usize = 32 << 10;
const MAX_ELEMENTS: usize = 512;
pub(crate) struct Site {
    pub value: String,
    pub range: Range,
    pub extension: &'static str,
}
pub(crate) fn references(doc: &Document) -> Vec<(String, Range)> {
    sites(doc)
        .into_iter()
        .filter_map(|site| {
            literal_path(&site.value, site.extension).map(|path| (path.to_owned(), site.range))
        })
        .collect()
}
pub(crate) fn sites(doc: &Document) -> Vec<Site> {
    if doc.language != "xml"
        || !doc.path.ends_with(".xml")
        || doc.text.len() > MAX_TEXT
        || doc.text.contains('&')
        || !generate::xml_valid(&doc.text)
    {
        return vec![];
    }
    let elements = xml::elements(&doc.text);
    if elements.len() > MAX_ELEMENTS {
        return vec![];
    }
    let Some(root) = elements.first() else {
        return vec![];
    };
    if root.local() != "beans"
        || xml_namespace(&elements, 0) != Some(BEANS)
        || root.value("profile").is_some()
    {
        return vec![];
    }
    if elements.iter().enumerate().skip(1).any(|(i, e)| {
        xml_namespace(&elements, i) == Some(BEANS)
            && matches!(e.local(), "beans" | "import" | "alias")
    }) {
        return vec![];
    }
    let mut result = vec![];
    for (i, bean) in elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.parent == Some(0))
    {
        if bean.local() != "bean"
            || xml_namespace(&elements, i) != Some(BEANS)
            || bean.attrs.iter().any(|a| {
                !matches!(a.name.as_str(), "id" | "class")
                    && a.name != "xmlns"
                    && !a.name.starts_with("xmlns:")
            })
        {
            continue;
        }
        let (property_names, extension, constructor): (&[&str], &'static str, bool) = match bean
            .value("class")
        {
            Some("org.springframework.xml.xsd.SimpleXsdSchema") => (&["xsd"], ".xsd", true),
            Some("org.springframework.ws.wsdl.wsdl11.SimpleWsdl11Definition") => {
                (&["wsdl"], ".wsdl", true)
            }
            Some("org.springframework.xml.xsd.commons.CommonsXsdSchema") => (&[], ".xsd", true),
            Some(
                "org.springframework.ws.soap.server.endpoint.interceptor.PayloadValidatingInterceptor",
            ) => (&["schema"], ".xsd", false),
            Some(
                "org.springframework.ws.server.endpoint.interceptor.PayloadTransformingInterceptor",
            ) => (&["requestXslt", "responseXslt"], ".xslt", false),
            Some("org.springframework.ws.soap.security.xwss.XwsSecurityInterceptor") => {
                (&["policyConfiguration"], ".xml", false)
            }
            _ => continue,
        };
        if constructor {
            let args: Vec<_> = elements
                .iter()
                .enumerate()
                .filter(|(j, e)| {
                    e.parent == Some(i)
                        && e.local() == "constructor-arg"
                        && xml_namespace(&elements, *j) == Some(BEANS)
                })
                .collect();
            if let [(j, argument)] = args.as_slice() {
                if argument
                    .attrs
                    .iter()
                    .filter(|a| a.name == "index")
                    .all(|a| a.value == "0")
                {
                    if let Some(value) = plain_value(doc, &elements, *j, &["value", "index"]) {
                        result.push(Site {
                            value: value.value.clone(),
                            range: range(&doc.text, value.value_at, value.value_end),
                            extension,
                        });
                    }
                }
            }
        }
        let properties: Vec<_> = elements
            .iter()
            .enumerate()
            .filter(|(j, e)| {
                e.parent == Some(i)
                    && e.local() == "property"
                    && xml_namespace(&elements, *j) == Some(BEANS)
            })
            .collect();
        if properties
            .iter()
            .any(|(_, e)| e.value("name").is_none_or(|v| v.contains(['$', '#', '&'])))
        {
            continue;
        }
        for property in property_names {
            let matching: Vec<_> = properties
                .iter()
                .filter(|(_, e)| e.value("name") == Some(*property))
                .collect();
            let [(j, _)] = matching.as_slice() else {
                continue;
            };
            let Some((value, source_range)) = property_value(doc, &elements, *j) else {
                continue;
            };
            result.push(Site { value, range: source_range, extension });
        }
    }
    result
}

/// An existing scalar attribute or one direct, type-less Beans value text child.
/// The source text is neither decoded nor trimmed; comments/CDATA/nesting/ref
/// alternatives cannot masquerade as a resource literal.
pub(crate) fn property_value(doc: &Document, elements: &[xml::Element], index: usize) -> Option<(String, Range)> {
    if let Some(value) = plain_value(doc, elements, index, &["name", "value"]) {
        return Some((value.value.clone(), range(&doc.text, value.value_at, value.value_end)));
    }
    let property = &elements[index];
    let namespace_attribute = |a: &xml::Attr| a.name == "xmlns" || a.name.starts_with("xmlns:");
    if property.attrs.iter().any(|a| a.name != "name" && !namespace_attribute(a)) { return None; }
    let children: Vec<_> = elements.iter().enumerate().filter(|(_, e)| e.parent == Some(index)).collect();
    let [(child_index, value)] = children.as_slice() else { return None; };
    if value.local() != "value" || xml_namespace(elements, *child_index) != Some(BEANS)
        || value.attrs.iter().any(|a| !namespace_attribute(a))
        || elements.iter().any(|e| e.parent == Some(*child_index)) { return None; }
    let property_tail = doc.text.get(property.tag_end..property.end)?;
    let property_close = property.tag_end + property_tail.rfind("</")?;
    if value.start < property.tag_end || value.end > property_close
        || !doc.text.get(property.tag_end..value.start)?.trim().is_empty()
        || !doc.text.get(value.end..property_close)?.trim().is_empty() { return None; }
    let tail = doc.text.get(value.tag_end..value.end)?;
    let end = value.tag_end + tail.rfind("</")?;
    let text = doc.text.get(value.tag_end..end)?;
    if text.is_empty() || text.trim() != text || text.chars().any(char::is_control) || text.contains(['<', '>']) { return None; }
    Some((text.to_owned(), range(&doc.text, value.tag_end, end)))
}

fn plain_value<'a>(doc: &Document, elements: &'a [xml::Element], index: usize, attributes: &[&str]) -> Option<&'a xml::Attr> {
    let element = &elements[index];
    if element.attrs.iter().any(|a| !attributes.contains(&a.name.as_str()) && a.name != "xmlns" && !a.name.starts_with("xmlns:"))
        || elements.iter().any(|child| child.parent == Some(index)) { return None; }
    if !doc.text[element.start..element.tag_end].trim_end().ends_with("/>") {
        let inner = doc.text.get(element.tag_end..element.end)?.rsplit_once("</")?.0;
        if !inner.trim().is_empty() { return None; }
    }
    element.attrs.iter().find(|a| a.name == "value")
}

pub(crate) fn literal_path<'a>(value: &'a str, extension: &str) -> Option<&'a str> {
    if value.len() > 512 {
        return None;
    }
    let path = value.strip_prefix("classpath:")?;
    let path = path.strip_prefix('/').unwrap_or(path);
    if !path.ends_with(extension)
        || path.split('/').any(|p| p.is_empty() || p == "." || p == "..")
        || !path.bytes().all(|b| b.is_ascii_alphanumeric() || b"/_-.".contains(&b))
    {
        return None;
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WsTools;
    use openidea_plugin::diagnostics::DiagnosticsProvider;
    use openidea_plugin::{Event, FilesChanged, Plugin, Position, mock::MockHost, providers::Providers, text::position};
    const XSD: &str = "org.springframework.xml.xsd.SimpleXsdSchema";
    const WSDL: &str = "org.springframework.ws.wsdl.wsdl11.SimpleWsdl11Definition";
    const URI: &str = "http://www.springframework.org/schema/beans";
    fn doc(class: &str, property: &str, value: &str) -> Document {
        Document {
            path: "/project/context.xml".into(),
            language: "xml".into(),
            version: 1,
            text: format!(
                "<b:beans xmlns:b='{URI}'>\n<!-- 😀 --> <b:bean id='schema' class='{class}'><b:property name='{property}' value='{value}'/></b:bean>\n</b:beans>"
            ),
        }
    }
    fn at(d: &Document) -> Position {
        position(&d.text, d.text.find("classpath:").unwrap() + 12)
    }
    fn host() -> MockHost {
        let h = MockHost::install();
        h.add_file("src/main/resources/countries.xsd", "<schema/>");
        h.add_file("src/main/resources/orders.wsdl", "<definitions/>");
        h
    }
    fn warm(h: &MockHost) -> WsTools {
        let mut p = WsTools::default();
        p.activate(&h.context("aritti.spring-ws")).unwrap();
        p
    }
    #[test]
    fn both_original_class_property_pairs_resolve_through_public_provider_from_warm_inventory() {
        let h = host();
        let mut p = warm(&h);
        for (class, prop, value, target) in [
            (XSD, "xsd", "classpath:/countries.xsd", "/project/src/main/resources/countries.xsd"),
            (WSDL, "wsdl", "classpath:orders.wsdl", "/project/src/main/resources/orders.wsdl"),
        ] {
            let d = doc(class, prop, value);
            let r = p.definition(&d, at(&d));
            assert_eq!(r.len(), 1);
            assert_eq!(r[0].path, target);
            assert_eq!(r[0].range, Range::default(), "file navigation only; no invented symbol/rename token");
            let facts = references(&d);
            assert_eq!(facts.len(), 1);
            let from = d.text.find(value).unwrap();
            assert_eq!(facts[0].1, range(&d.text, from, from + value.len()));
        }
        assert!(h.process_runs().is_empty());
        assert!(h.requests().is_empty());
    }
    #[test]
    fn cold_and_missing_watch_grant_do_not_discover_resources_in_provider() {
        let h = host();
        let d = doc(XSD, "xsd", "classpath:/countries.xsd");
        assert!(WsTools::default().definition(&d, at(&d)).is_empty());
        let h = h.with_capabilities(&["project.read"]);
        let mut p = warm(&h);
        assert!(p.definition(&d, at(&d)).is_empty());
        assert!(h.process_runs().is_empty());
    }
    #[test]
    fn wrong_class_property_dynamic_inheritance_and_contexts_are_quiet() {
        let h = host();
        let mut p = warm(&h);
        for d in [
            doc("SimpleXsdSchema", "xsd", "classpath:/countries.xsd"),
            doc("example.SimpleXsdSchema", "xsd", "classpath:/countries.xsd"),
            doc(XSD, "wsdl", "classpath:/countries.xsd"),
            doc(WSDL, "xsd", "classpath:/countries.xsd"),
        ] {
            assert!(p.definition(&d, at(&d)).is_empty());
        }
        let good = doc(XSD, "xsd", "classpath:/countries.xsd");
        for text in [
            good.text.replace("class=", "parent='base' class="),
            good.text.replace("class=", "factory-method='make' class="),
            good.text.replace("</b:beans>", "<b:import resource='other.xml'/></b:beans>"),
            good.text.replace("</b:beans>", "<b:alias name='schema' alias='other'/></b:beans>"),
            good.text.replace("<b:beans ", "<b:beans profile='dev' "),
            good.text.replace("<b:bean ", "<b:beans><b:bean ").replace("</b:bean>", "</b:bean></b:beans>"),
            good.text.replace("<b:property ", "<b:property ref='other' "),
        ] {
            let d = Document { text, ..good.clone() };
            assert!(p.definition(&d, at(&d)).is_empty());
        }
    }
    #[test]
    fn default_prefixed_and_shadowed_namespaces_obey_exact_source_scope() {
        let h = host();
        let mut p = warm(&h);
        let d = doc(XSD, "xsd", "classpath:/countries.xsd");
        let default = d.text.replace("b:", "").replace("xmlns:b", "xmlns");
        let good = Document { text: default, ..d.clone() };
        assert_eq!(p.definition(&good, at(&good)).len(), 1);
        for text in [
            d.text.replace("<b:property ", "<b:property xmlns:b='urn:foreign' "),
            d.text.replace("xmlns:b=", "xmlns:xml=").replace("<b:", "<xml:").replace("</b:", "</xml:"),
            d.text.replace("value=", "b:value="),
            d.text.replace("name='xsd'", "b:name='xsd'"),
        ] {
            let bad = Document { text, ..d.clone() };
            assert!(p.definition(&bad, at(&bad)).is_empty());
        }
    }
    #[test]
    fn source_replacement_and_observed_resource_removal_withdraw_targets() {
        let h = host();
        let mut p = warm(&h);
        let d = doc(XSD, "xsd", "classpath:/countries.xsd");
        assert_eq!(p.definition(&d, at(&d)).len(), 1);
        let changed = Document { version: 2, text: d.text.replace("countries.xsd", "missing.xsd"), ..d.clone() };
        assert!(p.definition(&changed, at(&changed)).is_empty());
        p.handle_events(
            &[Event::FilesChanged(FilesChanged {
                removed: vec!["/project/src/main/resources/countries.xsd".into()],
                ..Default::default()
            })],
            &h.invocation(),
        );
        assert!(p.definition(&d, at(&d)).is_empty());
    }
    #[test]
    fn ambiguous_inventory_or_foreign_project_resource_never_selects_an_owner() {
        let h = host();
        h.add_file("src/test/resources/countries.xsd", "<schema/>");
        let mut p = warm(&h);
        let d = doc(XSD, "xsd", "classpath:/countries.xsd");
        assert!(p.definition(&d, at(&d)).is_empty());
        let h = host();
        h.add_project("/project/src/main/resources");
        let mut p = warm(&h);
        assert!(p.definition(&d, at(&d)).is_empty());
        h.set_project_root(None);
        assert!(p.definition(&d, at(&d)).is_empty());
        let h = MockHost::install();
        h.add_file("xsrc/main/resources/countries.xsd", "<schema/>");
        let mut p = warm(&h);
        assert!(p.definition(&d, at(&d)).is_empty(), "directory component must be src, not a suffix of xsrc");
    }
    #[test]
    fn paths_alternatives_and_caps_are_conservative() {
        let h = host();
        let mut p = warm(&h);
        for value in [
            "classpath:../countries.xsd",
            "classpath://countries.xsd",
            "classpath:*.xsd",
            "classpath:country%2exsd",
            "file:/countries.xsd",
            "https://example.invalid/countries.xsd",
            "classpath:${schema}.xsd",
            "classpath:/countries.wsdl",
        ] {
            let d = doc(XSD, "xsd", value);
            assert!(references(&d).is_empty());
        }
        let d = doc(XSD, "xsd", "classpath:/countries.xsd");
        for text in [
            d.text.replace("/></b:bean>", "><value>other</value></b:property></b:bean>"),
            d.text.replace("/></b:bean>", "/><b:property name='xsd' value='classpath:/countries.xsd'/></b:bean>"),
            format!("{}{}", d.text, " ".repeat(MAX_TEXT)),
        ] {
            let d = Document { text, ..d.clone() };
            assert!(p.definition(&d, at(&d)).is_empty());
        }
    }
    #[test]
    fn resource_path_authority_is_independent_of_schema_content_completeness() {
        let h = host();
        h.add_file("src/main/resources/large.xsd", &" ".repeat(crate::MAX_SCHEMA_BYTES + 1));
        let mut p = warm(&h);
        let d = doc(XSD, "xsd", "classpath:/countries.xsd");
        assert_eq!(p.definition(&d, at(&d)).len(), 1);
        let index=p.schemas.for_cached_document(&d.path,&d.text).unwrap();
        assert!(!crate::complete_schemas(index), "QName/source-content completeness stays false");
    }
    #[test]
    fn exact_payload_validating_interceptor_schema_property_uses_existing_xsd_inventory() {
        let h = host(); let mut p = warm(&h);
        let class = "org.springframework.ws.soap.server.endpoint.interceptor.PayloadValidatingInterceptor";
        let d = doc(class, "schema", "classpath:/countries.xsd");
        assert_eq!(p.definition(&d, at(&d))[0].path, "/project/src/main/resources/countries.xsd");
        for wrong in [doc(class,"schemas","classpath:/countries.xsd"),doc("example.PayloadValidatingInterceptor","schema","classpath:/countries.xsd"),doc(class,"schema","classpath:/orders.wsdl")] {
            assert!(p.definition(&wrong,at(&wrong)).is_empty());
        }
        assert!(h.requests().is_empty() && h.process_runs().is_empty());
    }
    fn constructor(class: &str, value: &str, extra: &str) -> Document {
        Document { path: "/project/context.xml".into(), language: "xml".into(), version: 8,
            text: format!("<b:beans xmlns:b='{URI}'><!-- 😀 --><b:bean id='schema' class='{class}'><b:constructor-arg {extra} value='{value}'/></b:bean></b:beans>") }
    }
    #[test]
    fn original_three_constructor_resource_class_filters_resolve_literal_single_input() {
        let h = host(); let mut p = warm(&h);
        for (class, value, target) in [(XSD,"classpath:/countries.xsd","/project/src/main/resources/countries.xsd"),
            (WSDL,"classpath:/orders.wsdl","/project/src/main/resources/orders.wsdl"),
            ("org.springframework.xml.xsd.commons.CommonsXsdSchema","classpath:countries.xsd","/project/src/main/resources/countries.xsd")] {
            for index in ["", "index='0'"] {
                let d=constructor(class,value,index);let result=p.definition(&d,at(&d));assert_eq!(result.len(),1,"{}",d.text);assert_eq!(result[0].path,target);
                let facts=references(&d);let start=d.text.find(value).unwrap();assert_eq!(facts[0].1,range(&d.text,start,start+value.len()));
            }
        }
    }
    #[test]
    fn constructor_ambiguity_ref_type_indirection_and_foreign_ownership_are_quiet() {
        let h = host(); let mut p=warm(&h); let d=constructor(XSD,"classpath:/countries.xsd","");
        for text in [d.text.replace("<b:constructor-arg ","<b:constructor-arg index='1' "),
            d.text.replace("<b:constructor-arg ","<b:constructor-arg name='resource' "),
            d.text.replace("<b:constructor-arg ","<b:constructor-arg ref='other' "),
            d.text.replace("<b:constructor-arg ","<b:constructor-arg type='org.springframework.core.io.Resource' "),
            d.text.replace("</b:bean>","<b:constructor-arg value='classpath:/second.xsd'/></b:bean>"),
            d.text.replace("<b:constructor-arg ","<b:constructor-arg xmlns:b='urn:foreign' "),
            d.text.replace("value=","b:value="),d.text.replace("class=","parent='base' class="),
            d.text.replace("class=","factory-method='build' class="),
            d.text.replace("/></b:bean>","><b:value>other</b:value></b:constructor-arg></b:bean>")] {
            let bad=Document{text,..d.clone()};assert!(p.definition(&bad,at(&bad)).is_empty(),"{}",bad.text);
        }
    }
    #[test]
    fn new_resource_inputs_remain_current_source_only_and_no_host_io_when_parsed() {
        let _denied=MockHost::install().with_capabilities(&[]);
        let d=constructor(XSD,"classpath:/countries.xsd","");assert_eq!(references(&d).len(),1);
        let default=Document{text:d.text.replace("b:","").replace("xmlns:b","xmlns"),..d.clone()};assert_eq!(references(&default).len(),1);
        let changed=Document{text:d.text.replace("countries.xsd","../outside.xsd"),version:9,..d.clone()};assert!(references(&changed).is_empty());
        let wrong=Document{text:d.text.replace(XSD,"example.SimpleXsdSchema"),..d};assert!(references(&wrong).is_empty());
    }

#[test]
fn remaining_original_xslt_and_security_xml_filters_navigate_owned_cached_resources() {
    let h = host();
    h.add_file(
        "src/main/resources/transforms/request.xslt",
        "<not-executed/>",
    );
    h.add_file(
        "src/main/resources/transforms/response.xslt",
        "<not-executed/>",
    );
    h.add_file("src/main/resources/security/policy.xml", "<not-executed/>");
    let mut p = warm(&h);
    for (class, property, value, target) in [
        (
            "org.springframework.ws.server.endpoint.interceptor.PayloadTransformingInterceptor",
            "requestXslt",
            "classpath:/transforms/request.xslt",
            "/project/src/main/resources/transforms/request.xslt",
        ),
        (
            "org.springframework.ws.server.endpoint.interceptor.PayloadTransformingInterceptor",
            "responseXslt",
            "classpath:transforms/response.xslt",
            "/project/src/main/resources/transforms/response.xslt",
        ),
        (
            "org.springframework.ws.soap.security.xwss.XwsSecurityInterceptor",
            "policyConfiguration",
            "classpath:/security/policy.xml",
            "/project/src/main/resources/security/policy.xml",
        ),
    ] {
        let d = doc(class, property, value);
        let result = p.definition(&d, at(&d));
        assert_eq!(result.len(), 1, "{class}/{property}");
        assert_eq!(result[0].path, target);
    }
}

#[test]
fn xslt_security_properties_reject_wrong_owners_extensions_duplicates_and_constructor_guesses() {
    let h = host();
    h.add_file("src/main/resources/policy.xml", "opaque");
    h.add_file("src/main/resources/request.xslt", "opaque");
    let mut p = warm(&h);
    let class = "org.springframework.ws.server.endpoint.interceptor.PayloadTransformingInterceptor";
    let good = doc(class, "requestXslt", "classpath:/request.xslt");
    assert_eq!(p.definition(&good, at(&good)).len(), 1);
    for bad in [
        doc(
            "example.PayloadTransformingInterceptor",
            "requestXslt",
            "classpath:/request.xslt",
        ),
        doc(class, "requestXslt", "classpath:/policy.xml"),
        doc(class, "requestXslt", "classpath:/request.xsl"),
        doc(class, "schema", "classpath:/request.xslt"),
        constructor(class, "classpath:/request.xslt", ""),
    ] {
        assert!(p.definition(&bad, at(&bad)).is_empty(), "{}", bad.text);
    }
    for text in [
        good.text.replace(
            "</b:bean>",
            "<b:property name='requestXslt' value='classpath:/request.xslt'/></b:bean>",
        ),
        good.text
            .replace("<b:property ", "<b:property xmlns:b='urn:foreign' "),
        good.text.replace("value=", "b:value="),
        good.text.replace("class=", "parent='base' class="),
    ] {
        let bad = Document {
            text,
            ..good.clone()
        };
        assert!(p.definition(&bad, at(&bad)).is_empty());
    }
    let both = Document {
        text: good.text.replace(
            "</b:bean>",
            "<b:property name='responseXslt' value='classpath:/request.xslt'/></b:bean>",
        ),
        ..good
    };
    assert_eq!(references(&both).len(), 2);
}
#[test]
fn resource_events_are_filtered_and_policy_xml_never_changes_qname_authority() {
    let h = MockHost::install();
    h.add_file("src/main/resources/schema.xsd","<schema xmlns='http://www.w3.org/2001/XMLSchema' targetNamespace='urn:owned'><element name='known'/></schema>");
    let mut p = warm(&h);
    let endpoint=Document{path:"/project/E.java".into(),language:"java".into(),version:1,text:"@org.springframework.ws.server.endpoint.annotation.Endpoint class E { @org.springframework.ws.server.endpoint.annotation.PayloadRoot(namespace=\"urn:owned\",localPart=\"missing\") void handle(){} }".into()};
    assert_eq!(p.diagnostics(&endpoint).len(), 1);
    let target = "/project/src/main/resources/security/policy.xml";
    h.add_file(
        "src/main/resources/security/policy.xml",
        "malformed policy content is not read or executed",
    );
    p.handle_events(
        &[Event::FilesChanged(FilesChanged {
            created: vec![target.into()],
            ..Default::default()
        })],
        &h.invocation(),
    );
    let d = doc(
        "org.springframework.ws.soap.security.xwss.XwsSecurityInterceptor",
        "policyConfiguration",
        "classpath:/security/policy.xml",
    );
    assert_eq!(p.definition(&d, at(&d)).len(), 1);
    assert_eq!(p.diagnostics(&endpoint).len(), 1);
    p.handle_events(
        &[Event::FilesChanged(FilesChanged {
            removed: vec![target.into()],
            ..Default::default()
        })],
        &h.invocation(),
    );
    assert!(p.definition(&d, at(&d)).is_empty());
    let before = p
        .resource_paths
        .paths("/project", "policy.xml")
        .unwrap()
        .clone();
    p.handle_events(
        &[Event::FileSaved("/project/unrelated.md".into())],
        &h.invocation(),
    );
    assert_eq!(
        &before,
        p.resource_paths.paths("/project", "policy.xml").unwrap()
    );
    let overflow: Vec<_> = (0..257).map(|i| format!("/project/p{i}.xml")).collect();
    p.handle_events(
        &[Event::FilesChanged(FilesChanged {
            modified: overflow,
            ..Default::default()
        })],
        &h.invocation(),
    );
    assert!(p.resource_paths.paths("/project", "policy.xml").is_none());
    assert_eq!(p.diagnostics(&endpoint).len(), 1);
    p.handle_events(
        &[Event::FileOpened(
            "/project/src/main/resources/schema.xsd".into(),
        )],
        &h.invocation(),
    );
    assert_eq!(p.definition(&d, at(&d)).len(), 1);
    assert!(h.requests().is_empty() && h.process_runs().is_empty());
}
#[test]
fn new_xml_family_ambiguity_and_foreign_nested_root_withhold_navigation() {
    let h = MockHost::install();
    h.add_file("src/main/resources/policy.xml", "opaque");
    h.add_file("src/test/resources/policy.xml", "opaque");
    let mut p = warm(&h);
    let d = doc(
        "org.springframework.ws.soap.security.xwss.XwsSecurityInterceptor",
        "policyConfiguration",
        "classpath:/policy.xml",
    );
    assert!(p.definition(&d, at(&d)).is_empty());
    let h = MockHost::install();
    h.add_file("src/main/resources/policy.xml", "opaque");
    h.add_project("/project/src/main/resources");
    let mut p = warm(&h);
    assert!(p.definition(&d, at(&d)).is_empty());
}

#[test]
fn capped_policy_family_cannot_suppress_known_schema_qname_diagnostics() {
    let h = MockHost::install();
    h.add_file("src/main/resources/schema.xsd","<schema xmlns='http://www.w3.org/2001/XMLSchema' targetNamespace='urn:owned'><element name='known'/></schema>");
    for i in 0..4097 {
        h.add_file(
            &format!("src/main/resources/p{i}.xml"),
            "not parsed as a schema",
        );
    }
    let mut p = warm(&h);
    assert!(p.resource_paths.paths("/project", "p0.xml").is_none());
    let endpoint=Document{path:"/project/E.java".into(),language:"java".into(),version:1,text:"@org.springframework.ws.server.endpoint.annotation.Endpoint class E { @org.springframework.ws.server.endpoint.annotation.PayloadRoot(namespace=\"urn:owned\",localPart=\"missing\") void handle(){} }".into()};
    let diagnostics = p.diagnostics(&endpoint);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].code.as_deref(),
        Some("unknown-payload-element")
    );
}

    #[test]
    fn created_directory_with_resource_extension_cannot_be_admitted_as_a_file_target() {
        let h=MockHost::install();let mut p=warm(&h);
        let target="/project/src/main/resources/policy.xml";h.add_dir("src/main/resources/policy.xml");
        p.handle_events(&[Event::FilesChanged(FilesChanged{created:vec![target.into()],..Default::default()})],&h.invocation());
        let d=doc("org.springframework.ws.soap.security.xwss.XwsSecurityInterceptor","policyConfiguration","classpath:/policy.xml");
        assert!(p.definition(&d,at(&d)).is_empty());
    }

#[test]
fn established_nine_scalar_roles_complete_cached_literal_classpath_prefixes() {
    let h = MockHost::install();
    for (ext, body) in [
        ("xsd", "<schema xmlns='http://www.w3.org/2001/XMLSchema'/>"),
        (
            "wsdl",
            "<definitions xmlns='http://schemas.xmlsoap.org/wsdl/'/>",
        ),
        ("xslt", "opaque"),
        ("xml", "opaque"),
    ] {
        h.add_file(&format!("src/main/resources/a/resource.{ext}"), body);
    }
    let mut p = warm(&h);
    for (class, property, ext) in [
        (XSD, Some("xsd"), "xsd"),
        (WSDL, Some("wsdl"), "wsdl"),
        (
            "org.springframework.ws.soap.server.endpoint.interceptor.PayloadValidatingInterceptor",
            Some("schema"),
            "xsd",
        ),
        (
            "org.springframework.ws.server.endpoint.interceptor.PayloadTransformingInterceptor",
            Some("requestXslt"),
            "xslt",
        ),
        (
            "org.springframework.ws.server.endpoint.interceptor.PayloadTransformingInterceptor",
            Some("responseXslt"),
            "xslt",
        ),
        (
            "org.springframework.ws.soap.security.xwss.XwsSecurityInterceptor",
            Some("policyConfiguration"),
            "xml",
        ),
        (XSD, None, "xsd"),
        (WSDL, None, "wsdl"),
        (
            "org.springframework.xml.xsd.commons.CommonsXsdSchema",
            None,
            "xsd",
        ),
    ] {
        let d = match property {
            Some(property) => doc(class, property, "classpath:/a/re"),
            None => constructor(class, "classpath:/a/re", "index='0'"),
        };
        let cursor = position(
            &d.text,
            d.text.find("classpath:/a/re").unwrap() + "classpath:/a/re".len(),
        );
        let items = p.completion(&d, cursor, None);
        assert_eq!(items.len(), 1, "{class}/{property:?}");
        assert_eq!(items[0].insert_text, format!("classpath:/a/resource.{ext}"));
    }
}

    #[test]
    fn direct_value_child_resolves_through_registered_provider_with_exact_text_range() {
        let h=host();let mut p=warm(&h);let value="classpath:/countries.xsd";let mut d=doc(XSD,"xsd",value);
        d.text=d.text.replace(&format!(" value='{value}'/>"),&format!("><b:value>{value}</b:value></b:property>")).replace('\n',"\r\n");
        let found=p.definition(&d,at(&d));assert_eq!(found.len(),1);assert_eq!(found[0].path,"/project/src/main/resources/countries.xsd");
        let span=references(&d);assert_eq!(span.len(),1);let from=d.text.find(value).unwrap();assert_eq!(span[0].1,range(&d.text,from,from+value.len()));
    }

    #[test]
    fn value_child_all_established_property_roles_retain_cache_and_namespace_proof() {
        let h=host();h.add_file("src/main/resources/request.xslt","opaque");h.add_file("src/main/resources/response.xslt","opaque");h.add_file("src/main/resources/policy.xml","opaque");let mut p=warm(&h);
        for (class,prop,value) in [
          (XSD,"xsd","classpath:/countries.xsd"),(WSDL,"wsdl","classpath:orders.wsdl"),
          ("org.springframework.ws.soap.server.endpoint.interceptor.PayloadValidatingInterceptor","schema","classpath:/countries.xsd"),
          ("org.springframework.ws.server.endpoint.interceptor.PayloadTransformingInterceptor","requestXslt","classpath:/request.xslt"),
          ("org.springframework.ws.server.endpoint.interceptor.PayloadTransformingInterceptor","responseXslt","classpath:response.xslt"),
          ("org.springframework.ws.soap.security.xwss.XwsSecurityInterceptor","policyConfiguration","classpath:/policy.xml")
        ] {let mut d=doc(class,prop,value);d.text=d.text.replace(&format!(" value='{value}'/>"),&format!("><b:value>{value}</b:value></b:property>"));
          for text in [d.text.clone(),d.text.replace("b:","").replace("xmlns:b","xmlns"),d.text.replace("<b:beans ","<b:beans xmlns='urn:foreign' ")] {
            let d=Document{text,..d.clone()};assert_eq!(p.definition(&d,at(&d)).len(),1,"{class}.{prop}");let span=references(&d)[0].1;let items=p.completion(&d,span.end,None);assert_eq!(items.len(),1,"{class}.{prop}");assert_eq!(items[0].insert_text,value);assert_eq!(items[0].replace,Some(span));
          }
        }assert!(h.process_runs().is_empty());assert!(h.requests().is_empty());
    }
    #[test]
    fn value_child_unknown_alternatives_and_unowned_text_stay_quiet() {
        let h=host();let mut p=warm(&h);let value="classpath:/countries.xsd";let mut good=doc(XSD,"xsd",value);good.text=good.text.replace(&format!(" value='{value}'/>"),&format!("><b:value>{value}</b:value></b:property>"));
        assert!(WsTools::default().definition(&good,at(&good)).is_empty());
        for text in [
          good.text.replace("<b:property ","<b:property ref='other' "),
          good.text.replace("<b:property ","<b:property value='classpath:/countries.xsd' "),
          good.text.replace("<b:value>","<b:value type='java.lang.String'>"),
          good.text.replace("<b:value>","<b:value xmlns:b='urn:foreign'>"),
          good.text.replace("<b:value>","<b:value xmlns:x='urn:foreign' x:flag='yes'>"),
          good.text.replace("</b:value>",&format!("</b:value><b:value>{value}</b:value>")),
          good.text.replace("</b:property>",&format!("</b:property><b:property name='xsd'><b:value>{value}</b:value></b:property>")),
          good.text.replace("<b:value>","ignored<b:value>"),
          good.text.replace("</b:value>","</b:value>ignored"),
          good.text.replace("<b:value>","<b:value><!--comment-->"),
          good.text.replace(value,&format!("<![CDATA[{value}]]>")),
          good.text.replace(value,"classpath:/countries&#46;xsd"),
          good.text.replace("<b:value>","<b:list><b:value>").replace("</b:value>","</b:value></b:list>"),
          good.text.replace("</b:value>","<b:ref bean='other'/></b:value>"),
          good.text.replace(XSD,"example.SimpleXsdSchema"),
          good.text.replace("<b:property ","<b:property xmlns:b='urn:foreign' "),
          good.text.replace(value," classpath:/countries.xsd "),
        ] {let d=Document{text,..good.clone()};assert!(sites(&d).is_empty(),"{}",d.text);let at=position(&d.text,d.text.find("classpath:").unwrap()+12);assert!(p.definition(&d,at).is_empty());assert!(p.completion(&d,at,None).is_empty());}
        let constructor=Document{text:format!("<b:beans xmlns:b='{URI}'><b:bean class='{XSD}'><b:constructor-arg><b:value>{value}</b:value></b:constructor-arg></b:bean></b:beans>"),..good};assert!(sites(&constructor).is_empty(),"constructor child form not added by property-only contract");
        let cold=WsTools::default().definition(&constructor,at(&constructor));assert!(cold.is_empty());
    }

}
