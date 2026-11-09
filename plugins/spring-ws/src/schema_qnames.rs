//! Literal source QNames, distinct from schema/resource inventory completeness.
use crate::{Schema, generate, xml_namespace};
use spring_text::{range, xml};
const XSD: &str = "http://www.w3.org/2001/XMLSchema";
const WSDL: &str = "http://schemas.xmlsoap.org/wsdl/";
pub(crate) fn local(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}
fn namespace(value: &str) -> bool {
    value.len() <= 1024
        && !value.chars().any(|c| c.is_control() || c.is_whitespace())
        && !value.contains(['&', '\\', '"'])
        && !value.contains("${")
        && !value.contains("#{")
}
/// The boolean permits only source-local absence checks. Positive facts survive
/// known external include/import dependencies, but their absence is unproved.
pub(crate) fn read(text: &str) -> (Vec<Schema>, bool) {
    if !generate::xml_valid(text) {
        return (vec![], false);
    }
    let elements = xml::elements(text);
    let mut depths = Vec::with_capacity(elements.len());
    for e in &elements {
        let depth = e.parent.map_or(0, |parent| depths[parent] + 1);
        if depth > 64
            || e.name.matches(':').count() > 1
            || e.attrs.iter().any(|a| a.name.matches(':').count() > 1)
        {
            return (vec![], false);
        }
        depths.push(depth);
    }
    let Some(root) = elements.first() else {
        return (vec![], false);
    };
    let standalone = root.local() == "schema" && xml_namespace(&elements, 0) == Some(XSD);
    let wsdl = root.local() == "definitions" && xml_namespace(&elements, 0) == Some(WSDL);
    if !standalone && !wsdl {
        return (vec![], false);
    }
    let mut schemas = vec![];
    let mut complete = true;
    for (index, schema) in elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.local() == "schema")
    {
        if xml_namespace(&elements, index) != Some(XSD) {
            complete = false;
            continue;
        }
        if index != 0
            && !schema.parent.is_some_and(|parent| {
                let types = &elements[parent];
                types.local() == "types"
                    && types.parent == Some(0)
                    && xml_namespace(&elements, parent) == Some(WSDL)
                    && wsdl
            })
        {
            complete = false;
            continue;
        }
        // Qualified attributes have distinct XML identity. Do not let Attr::value's
        // local-name convenience turn a foreign targetNamespace/name into ours.
        if schema
            .attrs
            .iter()
            .any(|a| a.local() == "targetNamespace" && a.name != "targetNamespace")
        {
            complete = false;
            continue;
        }
        let ns = schema
            .attrs
            .iter()
            .find(|a| a.name == "targetNamespace")
            .map_or("", |a| a.value.as_str());
        if !namespace(ns) {
            complete = false;
            continue;
        }
        let mut declarations = vec![];
        let mut seen = std::collections::BTreeSet::new();
        let mut duplicate = std::collections::BTreeSet::new();
        for (child_index, child) in elements
            .iter()
            .enumerate()
            .filter(|(_, e)| e.parent == Some(index))
        {
            if xml_namespace(&elements, child_index) != Some(XSD) {
                complete = false;
                continue;
            }
            if matches!(
                child.local(),
                "include" | "import" | "redefine" | "override"
            ) {
                complete = false;
                continue;
            }
            if child.local() != "element" {
                continue;
            }
            if child
                .attrs
                .iter()
                .any(|a| a.local() == "name" && a.name != "name")
                || child.attrs.iter().any(|a| a.local() == "ref")
            {
                complete = false;
                continue;
            }
            let Some(name) = child.attrs.iter().find(|a| a.name == "name") else {
                complete = false;
                continue;
            };
            if !local(&name.value) {
                complete = false;
                continue;
            }
            if !seen.insert(name.value.clone()) {
                duplicate.insert(name.value.clone());
                complete = false;
            }
            declarations.push((
                name.value.clone(),
                range(text, name.value_at, name.value_end),
            ));
        }
        declarations.retain(|(name, _)| !duplicate.contains(name));
        schemas.push(Schema {
            namespace: ns.into(),
            elements: declarations,
        });
    }
    // WSDL imports may introduce other schema contexts that this source reader
    // cannot close, even when it retains its own literal inline declarations.
    if elements
        .iter()
        .enumerate()
        .any(|(i, e)| e.local() == "import" && xml_namespace(&elements, i) == Some(WSDL))
    {
        complete = false;
    }
    (schemas, complete)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WsTools, schemas};
    use openidea_plugin::{
        Plugin,
        diagnostics::DiagnosticsProvider,
        mock::MockHost,
        providers::{Document, Providers},
        text::position,
    };
    const PATH: &str = "/project/src/main/resources/schema.xsd";
    fn schema(body: &str) -> String {
        format!("<s:schema xmlns:s=\"{XSD}\" targetNamespace=\"urn:test\">{body}</s:schema>")
    }
    fn endpoint(local: &str) -> Document {
        Document {
            path: "/project/Endpoint.java".into(),
            language: "java".into(),
            version: 1,
            text: format!(
                "@org.springframework.ws.server.endpoint.annotation.Endpoint class E {{ @org.springframework.ws.server.endpoint.annotation.PayloadRoot(namespace=\"urn:test\",localPart=\"{local}\") void handle() {{}} }}"
            ),
        }
    }
    fn xml(text: &str) -> Document {
        Document {
            path: PATH.into(),
            language: "xml".into(),
            version: 2,
            text: text.into(),
        }
    }
    #[test]
    fn literal_qnames_require_exact_schema_and_global_element_namespaces_and_attributes() {
        for source in [
            "<schema targetNamespace='urn:test'><element name='owned'/></schema>".to_owned(),
            schema("<x:element xmlns:x='urn:foreign' name='owned'/>"),
            schema("<s:element xmlns:x='urn:foreign' x:name='owned'/>"),
            schema("<s:element name='bad:name'/>"),
            schema("<s:element name='${dynamic}'/>"),
            schema("<s:element name='en&#116;ity'/>"),
            format!(
                "<s:schema xmlns:s='{XSD}' xmlns:x='urn:foreign' x:targetNamespace='urn:test'><s:element name='owned'/></s:schema>"
            ),
        ] {
            assert!(
                schemas(&source).iter().all(|s| s.elements.is_empty()),
                "{source}"
            );
            assert!(!read(&source).1, "{source}");
        }
        let source = schema(
            "<!-- 😀 --><s:element name='owned'><s:complexType><s:sequence><s:element name='local'/></s:sequence></s:complexType></s:element>",
        );
        let (facts, complete) = read(&source);
        assert!(complete && facts[0].elements.len() == 1 && facts[0].elements[0].0 == "owned");
        let start = source.find("'owned'").unwrap() + 1;
        assert_eq!(facts[0].elements[0].1, range(&source, start, start + 5));
    }
    #[test]
    fn inline_wsdl_and_empty_target_namespace_preserve_known_source_identity() {
        let source = format!(
            "<w:definitions xmlns:w='{WSDL}'><w:types><schema xmlns='{XSD}' targetNamespace='urn:inline'><element name='request'/></schema></w:types></w:definitions>"
        );
        assert!(read(&source).1);
        assert_eq!(schemas(&source)[0].namespace, "urn:inline");
        let source = format!("<schema xmlns='{XSD}'><element name='request'/></schema>");
        assert_eq!(schemas(&source)[0].namespace, "");
        assert!(read(&source).1);
        let unsupported = format!("<other>{source}</other>");
        assert!(schemas(&unsupported).is_empty());
        assert!(!read(&unsupported).1);
    }
    #[test]
    fn dependencies_duplicate_declarations_unknown_names_and_malformed_contexts_cannot_prove_absence()
     {
        for body in [
            "<s:include schemaLocation='other.xsd'/>",
            "<s:import namespace='urn:external'/>",
            "<s:redefine schemaLocation='other.xsd'/>",
            "<s:override schemaLocation='other.xsd'/>",
            "<s:element ref='s:unresolved'/>",
            "<s:element xmlns:f='urn:foreign' f:name='unknown'/>",
        ] {
            let source = schema(&format!("<s:element name='owned'/>{body}"));
            let (facts, complete) = read(&source);
            assert!(!complete && facts[0].elements[0].0 == "owned", "{source}");
        }
        let source =
            schema("<s:element name='dup'/><s:element name='dup'/><s:element name='unique'/>");
        let (facts, complete) = read(&source);
        assert!(!complete);
        assert_eq!(facts[0].elements.len(), 1);
        assert_eq!(facts[0].elements[0].0, "unique");
        for source in [
            schema("<s:element name='owned'>"),
            schema("<s:element name='owned'/>")
                .replace("<s:schema ", "<s:schema xmlns:xml='urn:foreign' "),
            schema("<s:element name='owned'/>").replace("urn:test", "urn:&#116;est"),
            format!(
                "<!DOCTYPE schema []>{}",
                schema("<s:element name='owned'/>")
            ),
        ] {
            assert!(schemas(&source).is_empty());
            assert!(!read(&source).1);
        }
    }
    #[test]
    fn current_malformed_or_import_dependent_schema_retires_absence_warnings_then_recovers() {
        let h = MockHost::install();
        let good = schema("<s:element name='owned'/>");
        h.add_file("src/main/resources/schema.xsd", &good);
        let mut p = WsTools::default();
        p.activate(&h.context("aritti.spring-ws")).unwrap();
        let missing = endpoint("missing");
        assert_eq!(p.diagnostics(&missing).len(), 1);
        for source in [
            good.replace("</s:schema>", ""),
            schema("<s:element name='owned'/><s:include schemaLocation='external.xsd'/>"),
            good.replace(XSD, "urn:foreign"),
        ] {
            p.diagnostics(&xml(&source));
            assert!(p.diagnostics(&missing).is_empty(), "{source}");
        }
        p.diagnostics(&xml(&good));
        assert_eq!(p.diagnostics(&missing).len(), 1);
        assert!(h.requests().is_empty() && h.process_runs().is_empty());
    }
    #[test]
    fn known_positive_navigation_survives_external_dependencies_without_claiming_missing_names() {
        let h = MockHost::install();
        h.add_file(
            "src/main/resources/schema.xsd",
            &schema("<s:element name='owned'/><s:include schemaLocation='external.xsd'/>"),
        );
        let mut p = WsTools::default();
        p.activate(&h.context("aritti.spring-ws")).unwrap();
        let d = endpoint("owned");
        let at = position(&d.text, d.text.find("owned").unwrap());
        assert_eq!(p.definition(&d, at).len(), 1);
        assert!(p.diagnostics(&endpoint("missing")).is_empty());
        let bad = schema("<s:element name='owned'/>").replace(XSD, "urn:foreign");
        p.diagnostics(&xml(&bad));
        assert!(p.definition(&d, at).is_empty());
    }
    #[test]
    fn literal_resource_inventory_stays_distinct_from_unproved_schema_qnames() {
        let h = MockHost::install();
        h.add_file("src/main/resources/schema.xsd", "<schema/>");
        let mut p = WsTools::default();
        p.activate(&h.context("aritti.spring-ws")).unwrap();
        let mut d = endpoint("missing");
        d.text.push_str(
            " class Config { Object x(){return new org.springframework.core.io.ClassPathResource(\"missing.xsd\");} }",
        );
        let diagnostics = p.diagnostics(&d);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code.as_deref(), Some("missing-resource"));
    }
}
