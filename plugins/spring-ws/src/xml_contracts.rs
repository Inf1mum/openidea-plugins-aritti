//! Source-local Spring-WS 2.0 schema contracts, independent of schema discovery.
use crate::*;
const OXM: &str = "http://www.springframework.org/schema/oxm";
const BEANS: &str = "http://www.springframework.org/schema/beans";
fn elements(doc: &Document) -> Option<Vec<xml::Element>> {
    if doc.language != "xml"
        || doc.text.len() > MAX_DOCUMENT_BYTES
        || !generate::xml_valid(&doc.text)
    {
        return None;
    }
    let es = xml::elements(&doc.text);
    if es.len() > 512 {
        return None;
    }
    // Do not apply current contracts to an explicitly selected older/custom XSD.
    for e in &es {
        for a in &e.attrs {
            if a.local() == "schemaLocation" {
                let parts: Vec<_> = a.value.split_whitespace().collect();
                for pair in parts.chunks_exact(2) {
                    if pair[0] == SWS_NAMESPACE
                        && !pair[1].ends_with("/web-services-2.0.xsd")
                        && !pair[1].ends_with("/web-services.xsd")
                    {
                        return None;
                    }
                }
            }
        }
    }
    Some(es)
}
fn top(es: &[xml::Element], i: usize) -> bool {
    es[i].parent.is_none()
        || es[i].parent.is_some_and(|p| {
            p == 0 && es[p].local() == "beans" && xml_namespace(es, p) == Some(BEANS)
        })
}
fn role<'a>(es: &'a [xml::Element], i: usize) -> Option<&'a str> {
    let e = &es[i];
    if xml_namespace(es, i) == Some(OXM) {
        return match e.local() {
            "jaxb2-marshaller" if top(es, i) => Some("oxm-marshaller"),
            "class-to-be-bound"
                if e.parent.is_some_and(|p| {
                    es[p].local() == "jaxb2-marshaller"
                        && xml_namespace(es, p) == Some(OXM)
                        && top(es, p)
                }) =>
            {
                Some("oxm-class")
            }
            _ => None,
        };
    }
    if xml_namespace(es, i) != Some(SWS_NAMESPACE) {
        return None;
    }
    match e.local() {
        "static-wsdl" | "dynamic-wsdl" | "annotation-driven" | "interceptors" if top(es, i) => {
            Some(e.local())
        }
        "xsd"
            if e.parent.is_some_and(|p| {
                es[p].local() == "dynamic-wsdl"
                    && xml_namespace(es, p) == Some(SWS_NAMESPACE)
                    && top(es, p)
            }) =>
        {
            Some("xsd")
        }
        "payloadRoot" | "soapAction"
            if e.parent.is_some_and(|p| {
                es[p].local() == "interceptors"
                    && xml_namespace(es, p) == Some(SWS_NAMESPACE)
                    && top(es, p)
            }) =>
        {
            Some(e.local())
        }
        _ => None,
    }
}
fn dynamic(value: &str) -> bool {
    value.contains(['&', '\\']) || value.contains("${") || value.contains("#{")
}
pub(crate) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let Some(es) = elements(doc) else {
        return vec![];
    };
    let mut out = vec![];
    for (i, e) in es.iter().enumerate() {
        let Some(role) = role(&es, i) else {
            continue;
        };
        let required: &[&str] = match role {
            "static-wsdl" | "xsd" => &["location"],
            "dynamic-wsdl" => &["id", "portTypeName", "locationUri"],
            "payloadRoot" => &["namespaceUri"],
            "soapAction" => &["value"],
            "oxm-class" => &["name"],
            _ => &[],
        };
        for name in required {
            if !e.attrs.iter().any(|a| a.name == *name) {
                out.push(
                    Diagnostic::new(
                        range(&doc.text, e.at, e.at + e.name.len()),
                        Severity::Warning,
                        format!("`{}` requires the `{name}` attribute.", e.local()),
                    )
                    .code("sws-required-attribute"),
                );
            }
        }
        if role == "oxm-marshaller" {
            let classes = es
                .iter()
                .enumerate()
                .filter(|(j, c)| {
                    c.parent == Some(i)
                        && c.local() == "class-to-be-bound"
                        && xml_namespace(&es, *j) == Some(OXM)
                })
                .count();
            let context = e.attrs.iter().find(|a| a.name == "context-path");
            if context.is_none_or(|a| !dynamic(&a.value)) {
                let context = context.is_some_and(|a| !a.value.is_empty());
                if context && classes > 0 || !context && classes == 0 {
                    out.push(Diagnostic::new(range(&doc.text,e.at,e.at+e.name.len()),Severity::Warning,if context{"Declared JAXB marshaller supplies both context-path and classes; choose one binding source."}else{"Declared JAXB marshaller needs context-path or class-to-be-bound entries."}).code("oxm-binding-source"));
                }
            }
        }
        if role == "dynamic-wsdl" {
            if !es.iter().enumerate().any(|(j, c)| {
                c.parent == Some(i)
                    && c.local() == "xsd"
                    && xml_namespace(&es, j) == Some(SWS_NAMESPACE)
            }) {
                out.push(
                    Diagnostic::new(
                        range(&doc.text, e.at, e.at + e.name.len()),
                        Severity::Warning,
                        "Spring-WS dynamic-wsdl requires at least one xsd schema child.",
                    )
                    .code("sws-required-schema"),
                );
            }
            for a in e.attrs.iter().filter(|a| {
                matches!(
                    a.name.as_str(),
                    "createSoap11Binding" | "createSoap12Binding"
                )
            }) {
                if !dynamic(&a.value) && !matches!(a.value.trim(), "true" | "false" | "1" | "0") {
                    out.push(
                        Diagnostic::new(
                            range(&doc.text, a.value_at, a.value_end),
                            Severity::Warning,
                            "Expected an XML Schema boolean: true, false, 1 or 0.",
                        )
                        .code("sws-boolean-value"),
                    );
                }
            }
        }
        if out.len() >= 256 {
            out.truncate(256);
            break;
        }
    }
    out
}
pub(crate) fn completion(doc: &Document, at: Position) -> Option<Vec<CompletionItem>> {
    let es = elements(doc)?;
    for (i, e) in es.iter().enumerate() {
        if role(&es, i) != Some("dynamic-wsdl") {
            continue;
        }
        for a in e.attrs.iter().filter(|a| {
            matches!(
                a.name.as_str(),
                "createSoap11Binding" | "createSoap12Binding"
            )
        }) {
            let span = range(&doc.text, a.value_at, a.value_end);
            if !(span.start <= at && at <= span.end) || dynamic(&a.value) {
                continue;
            }
            let caret = openidea_plugin::text::offset(&doc.text, at)?;
            let prefix = doc.text.get(a.value_at..caret)?;
            return Some(
                ["true", "false", "1", "0"]
                    .into_iter()
                    .filter(|v| v.starts_with(prefix))
                    .map(|v| CompletionItem {
                        replace: Some(span),
                        detail: Some("Spring-WS XML Schema boolean".into()),
                        ..CompletionItem::new(v, CompletionKind::Value)
                    })
                    .collect(),
            );
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{
        mock::MockHost,
        text::{offset, position},
    };
    fn doc(body: &str) -> Document {
        Document {
            path: "/project/context.xml".into(),
            text: format!("<b:beans xmlns:b='{BEANS}' xmlns:s='{SWS_NAMESPACE}'>{body}</b:beans>"),
            language: "xml".into(),
            version: 1,
        }
    }
    #[test]
    fn known_required_attributes_children_and_boolean_values_are_current_text_checks() {
        let _h = MockHost::install().with_capabilities(&[]);
        let d = doc(
            "<s:static-wsdl/><s:dynamic-wsdl createSoap11Binding='yes'/><s:interceptors><s:payloadRoot/><s:soapAction/></s:interceptors>",
        );
        let values = WsTools::default().diagnostics(&d);
        assert_eq!(
            values
                .iter()
                .filter(|d| d.code.as_deref() == Some("sws-required-attribute"))
                .count(),
            6
        );
        assert_eq!(
            values
                .iter()
                .filter(|d| d.code.as_deref() == Some("sws-required-schema"))
                .count(),
            1
        );
        assert_eq!(
            values
                .iter()
                .filter(|d| d.code.as_deref() == Some("sws-boolean-value"))
                .count(),
            1
        );
        let valid = doc(
            "<s:dynamic-wsdl id='orders' portTypeName='Orders' locationUri='/ws' createSoap11Binding='1' createSoap12Binding=' false '><s:xsd location='classpath:orders.xsd'/></s:dynamic-wsdl>",
        );
        assert!(diagnostics(&valid).is_empty());
    }
    #[test]
    fn foreign_contexts_older_schema_and_dynamic_boolean_values_do_not_borrow_current_contracts() {
        for d in [
            doc("<b:dynamic-wsdl/>"),
            doc("<b:bean><s:dynamic-wsdl/></b:bean>"),
            Document {
                path: "/p/a.xml".into(),
                text: format!(
                    "<s:dynamic-wsdl xmlns:s='{SWS_NAMESPACE}' xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance' xsi:schemaLocation='{SWS_NAMESPACE} https://www.springframework.org/schema/web-services/web-services-1.5.xsd'/>"
                ),
                language: "xml".into(),
                version: 1,
            },
        ] {
            assert!(diagnostics(&d).is_empty());
        }
        let d = doc(
            "<s:dynamic-wsdl id='x' portTypeName='P' locationUri='/ws' createSoap11Binding='${soap}'><s:xsd location='classpath:x.xsd'/></s:dynamic-wsdl>",
        );
        assert!(diagnostics(&d).is_empty());
    }
    #[test]
    fn boolean_completion_replaces_exact_value_after_unicode_and_does_not_need_host_access() {
        let _h = MockHost::install().with_capabilities(&[]);
        let d = doc(
            "<!--😀-->\r\n<s:dynamic-wsdl id='x' portTypeName='P' locationUri='/ws' createSoap11Binding='tr'><s:xsd location='classpath:x.xsd'/></s:dynamic-wsdl>",
        );
        let start = d.text.find("='tr'").unwrap() + 2;
        let at = position(&d.text, start + 2);
        let items = WsTools::default().completion(&d, at, None);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "true");
        let r = items[0].replace.unwrap();
        assert_eq!(
            &d.text[offset(&d.text, r.start).unwrap()..offset(&d.text, r.end).unwrap()],
            "tr"
        );
    }
}

#[cfg(test)]
mod oxm_contract_tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn document(body: &str) -> Document {
        Document {
            path: "/project/context.xml".into(),
            language: "xml".into(),
            version: 1,
            text: format!("<b:beans xmlns:b='{BEANS}' xmlns:o='{OXM}'>{body}</b:beans>"),
        }
    }
    #[test]
    fn jaxb_configuration_requires_one_declared_binding_source_and_class_names() {
        let _h = MockHost::install().with_capabilities(&[]);
        for body in [
            "<o:jaxb2-marshaller/>",
            "<o:jaxb2-marshaller context-path='com.acme'><o:class-to-be-bound name='com.acme.Model'/></o:jaxb2-marshaller>",
        ] {
            assert_eq!(
                WsTools::default()
                    .diagnostics(&document(body))
                    .iter()
                    .filter(|d| d.code.as_deref() == Some("oxm-binding-source"))
                    .count(),
                1
            );
        }
        assert_eq!(
            WsTools::default()
                .diagnostics(&document(
                    "<o:jaxb2-marshaller><o:class-to-be-bound/></o:jaxb2-marshaller>"
                ))
                .iter()
                .filter(|d| d.code.as_deref() == Some("sws-required-attribute"))
                .count(),
            1
        );
        for body in [
            "<o:jaxb2-marshaller context-path='com.acme'/>",
            "<o:jaxb2-marshaller><o:class-to-be-bound name='com.acme.Model'/></o:jaxb2-marshaller>",
            "<o:jaxb2-marshaller context-path='${context}'><o:class-to-be-bound name='com.acme.Model'/></o:jaxb2-marshaller>",
            "<b:jaxb2-marshaller/>",
        ] {
            assert!(
                WsTools::default().diagnostics(&document(body)).is_empty(),
                "{body}"
            );
        }
    }
}
