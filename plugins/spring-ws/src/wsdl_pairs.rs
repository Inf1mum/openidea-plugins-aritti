//! Spring-WS `dynamic-wsdl` operation derivation (reference: "Publishing the WSDL"):
//! messages ending in the request/response suffix (defaults `Request`/`Response`) pair into one operation,
//! and `void` handlers answer nothing, so `@ResponsePayload` on them contradicts the signature.
use crate::*;

const RESPONSE_PAYLOAD: &str = "org.springframework.ws.server.endpoint.annotation.ResponsePayload";

fn literal(value: &str) -> Option<&str> {
    (!value.is_empty() && !value.contains(['&', '\\', '$', '#', '{', '}', '*', '?']) && value.trim() == value)
        .then_some(value)
}

pub(crate) fn unpaired_requests(doc: &Document, schemas: &Index<SchemaFile>) -> Vec<Diagnostic> {
    if doc.text.len() > MAX_DOCUMENT_BYTES || !generate::xml_valid(&doc.text) {
        return vec![];
    }
    let es = xml::elements(&doc.text);
    let mut out = vec![];
    for (i, e) in es.iter().enumerate() {
        if e.local() != "dynamic-wsdl" || xml_namespace(&es, i) != Some(SWS_NAMESPACE) || e.parent.is_some_and(|p| p != 0) {
            continue;
        }
        let suffix = |name: &str, default: &'static str| {
            let mut found = e.attrs.iter().filter(|a| a.name == name);
            match (found.next(), found.next()) {
                (None, _) => Some(default),
                (Some(a), None) => literal(&a.value),
                _ => None,
            }
        };
        let (Some(request), Some(response)) = (suffix("requestSuffix", "Request"), suffix("responseSuffix", "Response")) else {
            continue;
        };
        let xsds: Vec<_> = es
            .iter()
            .enumerate()
            .filter(|(j, c)| c.parent == Some(i) && c.local() == "xsd" && xml_namespace(&es, *j) == Some(SWS_NAMESPACE))
            .map(|(_, c)| c)
            .collect();
        let mut names: Vec<&str> = vec![];
        let mut resolved = !xsds.is_empty();
        for x in &xsds {
            let mut locations = x.attrs.iter().filter(|a| a.name == "location");
            let found = (|| {
                let location = locations.next()?;
                if locations.next().is_some() {
                    return None;
                }
                let path = literal(location.value.strip_prefix("classpath:")?)?.trim_start_matches('/');
                if !path.ends_with(".xsd") || path.split('/').any(|p| p.is_empty() || p == "." || p == "..") {
                    return None;
                }
                let mut matches = schemas.iter().filter(|(file, _)| {
                    RESOURCE_DIRS.iter().any(|dir| file.ends_with(&format!("{dir}/{path}")))
                });
                let (_, file) = matches.next()?;
                (matches.next().is_none() && file.qnames_complete).then_some(file)
            })();
            match found {
                Some(file) => names.extend(file.schemas.iter().flat_map(|s| s.elements.iter().map(|(n, _)| n.as_str()))),
                None => resolved = false,
            }
        }
        if !resolved {
            continue;
        }
        for x in &xsds {
            let Some(location) = x.attrs.iter().find(|a| a.name == "location") else { continue };
            let Some(path) = location.value.strip_prefix("classpath:").map(|p| p.trim_start_matches('/')) else { continue };
            let Some((_, file)) = schemas.iter().find(|(f, _)| RESOURCE_DIRS.iter().any(|d| f.ends_with(&format!("{d}/{path}")))) else { continue };
            for (name, _) in file.schemas.iter().flat_map(|s| &s.elements) {
                let Some(operation) = name.strip_suffix(request).filter(|o| !o.is_empty()) else { continue };
                if !names.contains(&format!("{operation}{response}").as_str()) {
                    out.push(
                        Diagnostic::new(
                            range(&doc.text, location.value_at, location.value_end),
                            Severity::Info,
                            format!("Spring-WS pairs `{name}` with `{operation}{response}` into the `{operation}` operation; no `{operation}{response}` element is declared in the referenced schemas."),
                        )
                        .code("wsdl-unpaired-request"),
                    );
                }
            }
        }
    }
    out
}

/// A `void` Java handler returns no payload, so `@ResponsePayload` contradicts it.
pub(crate) fn void_response_payload(doc: &Document) -> Vec<Diagnostic> {
    if doc.language != "java" {
        return vec![];
    }
    let file = parse_source(&doc.path, &doc.text);
    let owners = annotation_owners::Owners::new(&file, &doc.text, false);
    let mut out = vec![];
    for ty in file.types.iter().filter(|t| owners.endpoint(&doc.text, t)) {
        for m in ty.methods() {
            if m.ty.as_ref().is_none_or(|t| t.text != "void" || t.array) {
                continue;
            }
            if !m.annotations.iter().any(|a| owners.payload(&doc.text, a)) {
                continue;
            }
            for a in m.annotations.iter().filter(|a| a.name == "ResponsePayload" && owners.known(&doc.text, a, RESPONSE_PAYLOAD)) {
                out.push(
                    Diagnostic::new(
                        range(&doc.text, a.at, a.end),
                        Severity::Info,
                        format!("`{}` returns void, so Spring-WS ignores @ResponsePayload (DefaultMethodEndpointAdapter skips return handling for void); remove it or return a payload.", m.name),
                    )
                    .code("void-response-payload"),
                );
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const BEANS: &str = "http://www.springframework.org/schema/beans";

    fn xml(attrs: &str, xsd: &str) -> Document {
        Document {
            path: "/project/src/main/resources/ws.xml".into(),
            text: format!("<b:beans xmlns:b='{BEANS}' xmlns:s='{SWS_NAMESPACE}'><s:dynamic-wsdl id='h' portTypeName='P' locationUri='/p/' {attrs}>{xsd}</s:dynamic-wsdl></b:beans>"),
            language: "xml".into(),
            version: 1,
        }
    }

    fn host(xsd: &str) -> MockHost {
        let host = MockHost::install();
        host.add_file("src/main/resources/hr.xsd", xsd);
        host
    }

    const XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:hr">
<xs:element name="HolidayRequest"/><xs:element name="HolidayResponse"/><xs:element name="FireRequest"/><xs:element name="Request"/></xs:schema>"#;

    #[test]
    fn request_without_response_is_info_and_pairs_are_quiet() {
        let h = host(XSD);
        let mut ws = WsTools::default();
        ws.activate(&h.context("aritti.spring-ws")).unwrap();
        let d = xml("", "<s:xsd location='classpath:hr.xsd'/>");
        let found: Vec<_> = ws.diagnostics(&d).into_iter().filter(|p| p.code.as_deref() == Some("wsdl-unpaired-request")).collect();
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("FireRequest") && found[0].message.contains("FireResponse"));
        assert_eq!(found[0].severity, Severity::Info);
    }

    #[test]
    fn custom_suffixes_and_unresolvable_locations_stay_quiet() {
        let h = host(XSD);
        let mut ws = WsTools::default();
        ws.activate(&h.context("aritti.spring-ws")).unwrap();
        for d in [
            xml("requestSuffix='Query'", "<s:xsd location='classpath:hr.xsd'/>"),
            xml("responseSuffix='${r}'", "<s:xsd location='classpath:hr.xsd'/>"),
            xml("", "<s:xsd location='/WEB-INF/hr.xsd'/>"),
            xml("", "<s:xsd location='classpath:hr.xsd'/><s:xsd location='classpath:gone.xsd'/>"),
        ] {
            assert!(ws.diagnostics(&d).iter().all(|p| p.code.as_deref() != Some("wsdl-unpaired-request")), "{}", d.text);
        }
    }

    #[test]
    fn second_xsd_may_supply_the_response() {
        let h = host(XSD);
        h.add_file("src/main/resources/more.xsd", r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:hr"><xs:element name="FireResponse"/></xs:schema>"#);
        let mut ws = WsTools::default();
        ws.activate(&h.context("aritti.spring-ws")).unwrap();
        let d = xml("", "<s:xsd location='classpath:hr.xsd'/><s:xsd location='classpath:more.xsd'/>");
        assert!(ws.diagnostics(&d).iter().all(|p| p.code.as_deref() != Some("wsdl-unpaired-request")));
    }

    fn java(body: &str) -> Document {
        Document {
            path: "/project/src/main/java/E.java".into(),
            text: format!("import org.springframework.ws.server.endpoint.annotation.Endpoint;\nimport org.springframework.ws.server.endpoint.annotation.PayloadRoot;\nimport org.springframework.ws.server.endpoint.annotation.ResponsePayload;\n@Endpoint\nclass E {{\n{body}\n}}"),
            language: "java".into(),
            version: 1,
        }
    }

    #[test]
    fn void_with_response_payload_is_flagged_only_when_owned() {
        let _h = MockHost::install();
        let bad = void_response_payload(&java("@PayloadRoot(namespace=\"urn:a\", localPart=\"XRequest\") @ResponsePayload public void x() {}"));
        assert_eq!(bad.len(), 1);
        assert_eq!(bad[0].severity, Severity::Info);
        assert_eq!(bad[0].code.as_deref(), Some("void-response-payload"));
        for ok in [
            "@PayloadRoot(namespace=\"urn:a\", localPart=\"XRequest\") @ResponsePayload public Object x() { return null; }",
            "@PayloadRoot(namespace=\"urn:a\", localPart=\"XRequest\") public void x() {}",
            "@ResponsePayload public void x() {}",
        ] {
            assert!(void_response_payload(&java(ok)).is_empty(), "{ok}");
        }
        let foreign = Document { text: bad_foreign(), ..java("") };
        assert!(void_response_payload(&foreign).is_empty());
    }

    fn bad_foreign() -> String {
        "import org.springframework.ws.server.endpoint.annotation.Endpoint;\nimport org.springframework.ws.server.endpoint.annotation.PayloadRoot;\nimport my.ResponsePayload;\n@Endpoint class E { @PayloadRoot(namespace=\"urn:a\", localPart=\"XRequest\") @ResponsePayload public void x() {} }".into()
    }
}
