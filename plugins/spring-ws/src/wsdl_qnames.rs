//! WSDL 1.1 QName references (message part element/type, portType message, binding type,
//! port binding), resolved against literal local XML only. Unprefixed or unproved values stay quiet.
use crate::{generate, schema_qnames, xml_namespace};
use openidea_plugin::{
    Diagnostic, Location, Position, Range, Severity, host,
    providers::Document,
};
use spring_text::{contains, range, xml};
use std::collections::BTreeSet;

const XSD: &str = "http://www.w3.org/2001/XMLSchema";
const WSDL: &str = "http://schemas.xmlsoap.org/wsdl/";
const MAX_DEPTH: usize = 4;
const MAX_FILES: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Element,
    Type,
    Message,
    PortType,
    Binding,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Element => "xsd:element",
            Kind::Type => "xsd:complexType/simpleType",
            Kind::Message => "wsdl:message",
            Kind::PortType => "wsdl:portType",
            Kind::Binding => "wsdl:binding",
        }
    }
}

struct Decl {
    ns: String,
    kind: Kind,
    name: String,
    location: Location,
}

struct Reference {
    kind: Kind,
    ns: String,
    local: String,
    range: Range,
}

#[derive(Default)]
struct Universe {
    decls: Vec<Decl>,
    namespaces: BTreeSet<String>,
    complete: bool,
    files: usize,
}

pub(crate) struct Finding {
    range: Range,
    targets: Vec<Location>,
    problem: Option<String>,
}

fn bound<'a>(elements: &'a [xml::Element], mut index: usize, prefix: &str) -> Option<&'a str> {
    if matches!(prefix, "xml" | "xmlns") {
        return None;
    }
    let declaration = format!("xmlns:{prefix}");
    loop {
        let mut found = elements[index].attrs.iter().filter(|a| a.name == declaration);
        if let Some(a) = found.next() {
            return (found.next().is_none() && !a.value.is_empty()).then_some(a.value.as_str());
        }
        index = elements[index].parent?;
    }
}

fn parse(text: &str) -> Option<Vec<xml::Element>> {
    if !generate::xml_valid(text) {
        return None;
    }
    let elements = xml::elements(text);
    let mut depths: Vec<usize> = Vec::with_capacity(elements.len());
    for e in &elements {
        let depth = e.parent.map_or(0, |p| depths[p] + 1);
        if depth > 64 || e.name.matches(':').count() > 1 || e.attrs.iter().any(|a| a.name.matches(':').count() > 1) {
            return None;
        }
        depths.push(depth);
    }
    Some(elements)
}

/// Relative local file only; refuses URLs, absolute/escaping paths, escapes and placeholders.
fn resolve(from: &str, root: &str, location: &str) -> Option<String> {
    if location.is_empty()
        || location.contains([':', '\\', '%', '?', '#', '$', '&'])
        || location.starts_with('/')
    {
        return None;
    }
    let mut parts: Vec<&str> = from.rsplit_once('/')?.0.split('/').collect();
    for part in location.split('/') {
        match part {
            "." => {}
            ".." => {
                parts.pop()?;
            }
            "" => return None,
            part => parts.push(part),
        }
    }
    let path = parts.join("/");
    path.starts_with(&format!("{root}/")).then_some(path)
}

struct Loader {
    root: Option<String>,
}

impl Loader {
    fn read(&self, from: &str, location: &str) -> Option<(String, String)> {
        let root = self.root.as_deref()?;
        let path = resolve(from, root, location)?;
        let project = host::project_of(&path)?;
        if project.root().ok()?.as_str() != root {
            return None;
        }
        let text = project.read_file(&path).ok()?;
        Some((path, text))
    }
}

fn collect(
    u: &mut Universe,
    loader: &Loader,
    path: &str,
    text: &str,
    elements: &[xml::Element],
    index: usize,
    depth: usize,
    visited: &mut BTreeSet<(String, usize)>,
) {
    if depth > MAX_DEPTH || !visited.insert((path.to_owned(), index)) {
        u.complete &= depth <= MAX_DEPTH;
        return;
    }
    let schema = &elements[index];
    if schema.attrs.iter().any(|a| a.local() == "targetNamespace" && a.name != "targetNamespace") {
        u.complete = false;
        return;
    }
    let ns = schema.value("targetNamespace").unwrap_or("").to_owned();
    if !schema_qnames::namespace(&ns) {
        u.complete = false;
        return;
    }
    u.namespaces.insert(ns.clone());
    for (child_index, child) in elements.iter().enumerate().filter(|(_, e)| e.parent == Some(index)) {
        if xml_namespace(elements, child_index) != Some(XSD) {
            u.complete = false;
            continue;
        }
        match child.local() {
            "element" | "complexType" | "simpleType" => {
                let kind = if child.local() == "element" { Kind::Element } else { Kind::Type };
                let named = child.attrs.iter().find(|a| a.name == "name");
                let Some(name) = named.filter(|a| schema_qnames::local(&a.value)) else {
                    u.complete = false;
                    continue;
                };
                u.decls.push(Decl {
                    ns: ns.clone(),
                    kind,
                    name: name.value.clone(),
                    location: Location { path: path.into(), range: range(text, name.value_at, name.value_end) },
                });
            }
            "include" | "import" => {
                let include = child.local() == "include";
                let expected = if include { ns.clone() } else { child.value("namespace").unwrap_or("").to_owned() };
                let Some(location) = child.attrs.iter().find(|a| a.name == "schemaLocation") else {
                    u.complete = false;
                    continue;
                };
                u.files += 1;
                let loaded = (u.files <= MAX_FILES).then(|| loader.read(path, &location.value)).flatten();
                let Some((file, file_text)) = loaded else {
                    u.complete = false;
                    continue;
                };
                let Some(file_elements) = parse(&file_text) else {
                    u.complete = false;
                    continue;
                };
                let is_schema = file_elements.first().is_some_and(|r| r.local() == "schema")
                    && xml_namespace(&file_elements, 0) == Some(XSD);
                // Chameleon include (no targetNamespace) and mismatches are not modelled.
                if !is_schema || file_elements[0].value("targetNamespace").unwrap_or("") != expected {
                    u.complete = false;
                    continue;
                }
                collect(u, loader, &file, &file_text, &file_elements, 0, depth + 1, visited);
            }
            "annotation" => {}
            "redefine" | "override" => u.complete = false,
            _ => {}
        }
    }
}

fn universe(doc: &Document, elements: &[xml::Element], loader: &Loader) -> Universe {
    let mut u = Universe { complete: true, ..Universe::default() };
    let mut visited = BTreeSet::new();
    for (i, e) in elements.iter().enumerate().filter(|(_, e)| e.local() == "schema") {
        let in_types = e.parent.is_some_and(|p| {
            elements[p].local() == "types" && elements[p].parent == Some(0) && xml_namespace(elements, p) == Some(WSDL)
        });
        if !in_types {
            continue;
        }
        if xml_namespace(elements, i) != Some(XSD) {
            u.complete = false;
            continue;
        }
        collect(&mut u, loader, &doc.path, &doc.text, elements, i, 0, &mut visited);
    }
    u
}

fn qname(elements: &[xml::Element], index: usize, value: &str) -> Option<(String, String)> {
    let (prefix, local) = value.split_once(':')?;
    if !schema_qnames::local(local) || !schema_qnames::local(prefix) || prefix.contains('.') {
        return None;
    }
    Some((bound(elements, index, prefix)?.to_owned(), local.to_owned()))
}

fn wsdl_decls(doc: &Document, elements: &[xml::Element]) -> Vec<Decl> {
    let mut out = vec![];
    for (i, e) in elements.iter().enumerate().filter(|(_, e)| e.parent == Some(0)) {
        let kind = match e.local() {
            "message" => Kind::Message,
            "portType" => Kind::PortType,
            "binding" => Kind::Binding,
            _ => continue,
        };
        if xml_namespace(elements, i) != Some(WSDL) {
            continue;
        }
        if let Some(name) = e.attrs.iter().find(|a| a.name == "name").filter(|a| schema_qnames::local(&a.value)) {
            out.push(Decl {
                ns: String::new(),
                kind,
                name: name.value.clone(),
                location: Location { path: doc.path.clone(), range: range(&doc.text, name.value_at, name.value_end) },
            });
        }
    }
    out
}

fn references(doc: &Document, elements: &[xml::Element]) -> Vec<Reference> {
    let mut out = vec![];
    for (i, e) in elements.iter().enumerate() {
        if xml_namespace(elements, i) != Some(WSDL) {
            continue;
        }
        let parent = |n: usize| elements[n].parent;
        let local = |n: usize| elements[n].local();
        let under = |names: &[&str]| {
            let mut at = i;
            names.iter().all(|name| {
                let Some(p) = parent(at) else { return false };
                at = p;
                local(p) == *name && xml_namespace(elements, p) == Some(WSDL)
            }) && parent(at) == Some(0)
        };
        let sites: &[(&str, Kind)] = match e.local() {
            "part" if under(&["message"]) => &[("element", Kind::Element), ("type", Kind::Type)],
            "input" | "output" | "fault" if under(&["operation", "portType"]) => &[("message", Kind::Message)],
            "binding" if parent(i) == Some(0) => &[("type", Kind::PortType)],
            "port" if under(&["service"]) => &[("binding", Kind::Binding)],
            _ => continue,
        };
        for (attribute, kind) in sites {
            let mut matching = e.attrs.iter().filter(|a| a.name == *attribute);
            let (Some(a), None) = (matching.next(), matching.next()) else { continue };
            if let Some((ns, local)) = qname(elements, i, &a.value) {
                out.push(Reference { kind: *kind, ns, local, range: range(&doc.text, a.value_at, a.value_end) });
            }
        }
    }
    out
}

fn analyze(doc: &Document, at: Option<Position>) -> Vec<Finding> {
    let Some(elements) = parse(&doc.text) else { return vec![] };
    let Some(root) = elements.first() else { return vec![] };
    if root.local() != "definitions" || xml_namespace(&elements, 0) != Some(WSDL) {
        return vec![];
    }
    let target = root.value("targetNamespace").filter(|ns| schema_qnames::namespace(ns) && !ns.is_empty());
    let wsdl_complete = !elements.iter().enumerate().any(|(i, e)| e.local() == "import" && xml_namespace(&elements, i) == Some(WSDL));
    let own = wsdl_decls(doc, &elements);
    let loader = Loader {
        root: host::project_of(&doc.path).and_then(|p| p.root().ok()).filter(|r| r != "/"),
    };
    let mut schemas: Option<Universe> = None;
    let mut findings = vec![];
    for r in references(doc, &elements) {
        if at.is_some_and(|at| !contains(r.range, at)) {
            continue;
        }
        let (targets, known): (Vec<Location>, bool) = match r.kind {
            Kind::Element | Kind::Type => {
                if r.ns == XSD {
                    continue;
                }
                let u = schemas.get_or_insert_with(|| universe(doc, &elements, &loader));
                let targets = u
                    .decls
                    .iter()
                    .filter(|d| d.kind == r.kind && d.ns == r.ns && d.name == r.local)
                    .map(|d| d.location.clone())
                    .collect();
                (targets, u.complete && u.namespaces.contains(&r.ns))
            }
            _ => {
                if Some(r.ns.as_str()) != target {
                    continue;
                }
                let targets = own
                    .iter()
                    .filter(|d| d.kind == r.kind && d.name == r.local)
                    .map(|d| d.location.clone())
                    .collect();
                (targets, wsdl_complete)
            }
        };
        let problem = (targets.is_empty() && known).then(|| {
            format!("No {} `{}` is defined in namespace {}.", r.kind.label(), r.local, r.ns)
        });
        findings.push(Finding { range: r.range, targets, problem });
    }
    findings
}

pub(crate) fn definition(doc: &Document, at: Position) -> Vec<Location> {
    analyze(doc, Some(at)).into_iter().flat_map(|f| f.targets).collect()
}

pub(crate) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    analyze(doc, None)
        .into_iter()
        .filter_map(|f| {
            f.problem.map(|m| Diagnostic::new(f.range, Severity::Warning, m).code("unresolved-wsdl-reference"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const PATH: &str = "/project/src/main/resources/orders.wsdl";

    fn doc(body: &str) -> Document {
        let text = format!(
            "<w:definitions xmlns:w='{WSDL}' xmlns:x='{XSD}' xmlns:tns='urn:t' xmlns:o='urn:o' targetNamespace='urn:t'>{body}</w:definitions>"
        );
        Document { path: PATH.into(), language: "xml".into(), version: 1, text }
    }

    fn xsd(tns: &str, body: &str) -> String {
        format!("<x:schema xmlns:x='{XSD}' targetNamespace='{tns}'>{body}</x:schema>")
    }

    fn inline(body: &str) -> String {
        format!("<w:types>{}</w:types>", xsd("urn:o", body))
    }

    fn at(d: &Document, needle: &str) -> Position {
        openidea_plugin::text::position(&d.text, d.text.find(needle).unwrap() + 1)
    }

    #[test]
    fn part_element_and_type_resolve_inline_schema_and_warn_when_absent() {
        let _h = MockHost::install();
        let d = doc(&format!(
            "{}<w:message name='M'><w:part name='a' element='o:Req'/><w:part name='b' type='o:T'/><w:part name='c' element='o:Gone'/><w:part name='d' type='x:string'/></w:message>",
            inline("<x:element name='Req'/><x:complexType name='T'/>")
        ));
        assert_eq!(definition(&d, at(&d, "o:Req")).len(), 1);
        assert_eq!(definition(&d, at(&d, "o:T'")).len(), 1);
        let problems = diagnostics(&d);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].message.contains("Gone"));
    }

    #[test]
    fn element_and_type_symbol_spaces_do_not_mix() {
        let _h = MockHost::install();
        let d = doc(&format!(
            "{}<w:message name='M'><w:part name='a' element='o:T'/></w:message>",
            inline("<x:complexType name='T'/>")
        ));
        assert_eq!(diagnostics(&d).len(), 1);
    }

    #[test]
    fn unprefixed_unbound_foreign_and_unknown_namespaces_stay_quiet() {
        let _h = MockHost::install();
        let d = doc(&format!(
            "{}<w:message name='M'><w:part name='a' element='Req'/><w:part name='b' element='u:Req'/><w:part name='c' element='f:Req' xmlns:f='urn:other'/></w:message>",
            inline("<x:element name='Other'/>")
        ));
        assert!(diagnostics(&d).is_empty());
    }

    #[test]
    fn imports_and_includes_resolve_relative_locally_and_missing_files_retire_warnings() {
        let h = MockHost::install();
        h.add_file("src/main/resources/xsd/common.xsd", &xsd("urn:o", "<x:include schemaLocation='more.xsd'/><x:element name='Req'/>"));
        h.add_file("src/main/resources/xsd/more.xsd", &xsd("urn:o", "<x:include schemaLocation='common.xsd'/><x:complexType name='T'/>"));
        let types = |location: &str| {
            format!(
                "<w:types><x:schema targetNamespace='urn:o'><x:import namespace='urn:o' schemaLocation='{location}'/></x:schema></w:types>"
            )
        };
        let message = "<w:message name='M'><w:part name='a' element='o:Req'/><w:part name='b' type='o:T'/><w:part name='c' type='o:Nope'/></w:message>";
        let d = doc(&format!("{}{message}", types("xsd/common.xsd")));
        // The import's namespace equals the including schema's, and cycles terminate.
        assert_eq!(definition(&d, at(&d, "o:Req")).len(), 1);
        assert!(definition(&d, at(&d, "o:Req"))[0].path.ends_with("xsd/common.xsd"));
        assert_eq!(definition(&d, at(&d, "o:T'")).len(), 1);
        assert_eq!(diagnostics(&d).len(), 1);
        for bad in ["xsd/missing.xsd", "../../../../etc/x.xsd", "http://h/x.xsd", "/abs.xsd"] {
            let d = doc(&format!("{}{message}", types(bad)));
            assert!(diagnostics(&d).is_empty(), "{bad}");
        }
    }

    #[test]
    fn chameleon_include_import_mismatch_and_wsdl_imports_are_not_guessed() {
        let h = MockHost::install();
        h.add_file("src/main/resources/cham.xsd", &format!("<x:schema xmlns:x='{XSD}'><x:element name='Req'/></x:schema>"));
        h.add_file("src/main/resources/other.xsd", &xsd("urn:else", "<x:element name='Req'/>"));
        for location in ["cham.xsd", "other.xsd"] {
            let d = doc(&format!(
                "<w:types><x:schema targetNamespace='urn:o'><x:include schemaLocation='{location}'/></x:schema></w:types><w:message name='M'><w:part name='a' element='o:Gone'/></w:message>"
            ));
            assert!(diagnostics(&d).is_empty(), "{location}");
        }
        let d = doc(&format!(
            "<w:import namespace='urn:t' location='b.wsdl'/>{}<w:message name='M'><w:part name='a' element='o:Gone'/></w:message><w:portType name='P'><w:operation name='op'><w:input message='tns:Gone'/></w:operation></w:portType>",
            inline("<x:element name='Req'/>")
        ));
        assert_eq!(diagnostics(&d).len(), 1);
    }

    #[test]
    fn port_type_binding_and_service_references_navigate_and_warn() {
        let _h = MockHost::install();
        let d = doc(
            "<w:message name='M'/><w:portType name='P'><w:operation name='op'><w:input message='tns:M'/><w:output message='tns:Nope'/></w:operation></w:portType>\
             <w:binding name='B' type='tns:P'/><w:binding name='B2' type='tns:Nope'/>\
             <w:service name='S'><w:port name='p' binding='tns:B'/><w:port name='q' binding='tns:Nope'/></w:service>",
        );
        for needle in ["tns:M'", "tns:P'", "tns:B'"] {
            assert_eq!(definition(&d, at(&d, needle)).len(), 1, "{needle}");
        }
        assert_eq!(diagnostics(&d).len(), 3);
        let misplaced = doc("<w:binding name='B' type='tns:P'><w:operation name='o'><w:input message='tns:Nope'/></w:operation></w:binding>");
        assert_eq!(diagnostics(&misplaced).len(), 1);
    }

    #[test]
    fn non_wsdl_malformed_and_foreign_vocabularies_are_ignored() {
        let _h = MockHost::install();
        let d = doc("<w:message name='M'><w:part name='a' element='o:Gone'/></w:message>");
        let _ = &d;
        for text in [
            d.text.replace(WSDL, "urn:other"),
            d.text.replace("</w:definitions>", ""),
            format!("<!DOCTYPE d []>{}", d.text),
        ] {
            let bad = Document { text, ..d.clone() };
            assert!(diagnostics(&bad).is_empty());
        }
    }
}
