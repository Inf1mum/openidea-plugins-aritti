//! `DefaultWsdl11Definition` bean properties written as literals (Java `@Bean` method or XML bean).
//! Spring-WS enforces `portTypeName` (AbstractPortTypesProvider.addPortTypes Assert) and one of
//! `schema`/`schemaCollection` (InliningXsdSchemaTypesProvider.afterPropertiesSet Assert); `locationUri`
//! is documented in the class example but not enforced. Only fully visible definitions are judged.
use crate::*;
use spring_text::java::Tok;
use wsdl_pairs::{literal, pairing, resolve};

const DEFINITION: &str = "org.springframework.ws.wsdl.wsdl11.DefaultWsdl11Definition";
const SIMPLE_XSD: &str = "org.springframework.xml.xsd.SimpleXsdSchema";
const BEANS: &str = "http://www.springframework.org/schema/beans";

struct Def {
    anchor: Range,
    port_type: bool,
    location: bool,
    schema: bool,
    suffixes: Option<(String, String)>,
    xsd: Option<(String, Range)>,
}

fn definitions(doc: &Document) -> Vec<Def> {
    if doc.text.len() > MAX_DOCUMENT_BYTES {
        return vec![];
    }
    match doc.language.as_str() {
        "xml" => xml_definitions(doc),
        "java" => java_definitions(doc),
        _ => vec![],
    }
}

pub(crate) fn required(doc: &Document) -> Vec<Diagnostic> {
    let mut out = vec![];
    for d in definitions(doc) {
        if !d.port_type {
            out.push(
                Diagnostic::new(d.anchor, Severity::Warning, "DefaultWsdl11Definition requires `portTypeName` (Spring-WS asserts \"'portTypeName' is required\").")
                    .code("wsdl-missing-port-type-name"),
            );
        }
        if !d.schema {
            out.push(
                Diagnostic::new(d.anchor, Severity::Warning, "DefaultWsdl11Definition requires `schema` or `schemaCollection` (Spring-WS asserts \"setting 'schema' or 'schemaCollection' is required\").")
                    .code("wsdl-missing-schema"),
            );
        }
        if !d.location {
            out.push(
                Diagnostic::new(d.anchor, Severity::Info, "`locationUri` is not set; the generated SOAP address has no location (Spring-WS does not enforce it).")
                    .code("wsdl-missing-location-uri"),
            );
        }
    }
    out
}

pub(crate) fn pairs(doc: &Document, schemas: &Index<SchemaFile>) -> Vec<Diagnostic> {
    let mut out = vec![];
    for d in definitions(doc) {
        let (Some((request, response)), Some((path, at))) = (d.suffixes, d.xsd) else { continue };
        let Some(file) = resolve(schemas, &path) else { continue };
        out.extend(pairing(&[(file, at)], &request, &response));
    }
    out
}

fn suffixes(request: Option<Option<String>>, response: Option<Option<String>>) -> Option<(String, String)> {
    let part = |v: Option<Option<String>>, default: &str| match v {
        None => Some(default.to_owned()),
        Some(v) => v.filter(|v| literal(v).is_some()),
    };
    Some((part(request, "Request")?, part(response, "Response")?))
}

fn xsd_path(value: &str) -> Option<String> {
    xml_resources::literal_path(value, ".xsd").map(str::to_owned)
}

// ---- XML ----

fn xml_definitions(doc: &Document) -> Vec<Def> {
    if doc.text.contains('&') || !generate::xml_valid(&doc.text) {
        return vec![];
    }
    let es = xml::elements(&doc.text);
    let Some(root) = es.first() else { return vec![] };
    if es.len() > 512 || root.local() != "beans" || xml_namespace(&es, 0) != Some(BEANS) || root.value("profile").is_some() {
        return vec![];
    }
    let is_bean = |i: usize| es[i].local() == "bean" && xml_namespace(&es, i) == Some(BEANS);
    if (1..es.len()).any(|i| xml_namespace(&es, i) == Some(BEANS) && matches!(es[i].local(), "beans" | "import" | "alias")) {
        return vec![];
    }
    let plain = |i: usize| es[i].attrs.iter().all(|a| matches!(a.name.as_str(), "id" | "class") || a.name == "xmlns" || a.name.starts_with("xmlns:"));
    // `Some(properties)` only for a plain bean made of unique, literal-named properties.
    let properties = |i: usize| -> Option<Vec<(String, usize)>> {
        if !is_bean(i) || !plain(i) {
            return None;
        }
        let mut found: Vec<(String, usize)> = vec![];
        for (j, c) in es.iter().enumerate().filter(|(_, c)| c.parent == Some(i)) {
            let name = c.value("name").filter(|n| !n.contains(['$', '#', '&']))?;
            if c.local() != "property" || xml_namespace(&es, j) != Some(BEANS) || found.iter().any(|(n, _)| n == name) {
                return None;
            }
            found.push((name.to_owned(), j));
        }
        Some(found)
    };
    let simple_xsd = |i: usize| -> Option<(String, Range)> {
        if es[i].value("class") != Some(SIMPLE_XSD) {
            return None;
        }
        let props = properties(i)?;
        let (_, j) = props.iter().find(|(n, _)| n == "xsd")?;
        let (value, at) = xml_resources::property_value(doc, &es, *j)?;
        Some((xsd_path(&value)?, at))
    };
    let mut out = vec![];
    for i in es.iter().enumerate().filter(|(_, e)| e.parent == Some(0)).map(|(i, _)| i) {
        if es[i].value("class") != Some(DEFINITION) {
            continue;
        }
        let Some(props) = properties(i) else { continue };
        let get = |n: &str| props.iter().find(|(p, _)| p == n).map(|(_, j)| *j);
        let literal_of = |n: &str| get(n).map(|j| xml_resources::property_value(doc, &es, j).map(|(v, _)| v));
        let xsd = get("schema").and_then(|j| {
            let p = &es[j];
            let children: Vec<usize> = (0..es.len()).filter(|c| es[*c].parent == Some(j)).collect();
            let bean = match (p.value("ref"), children.as_slice()) {
                (Some(r), []) if p.attrs.iter().all(|a| matches!(a.name.as_str(), "name" | "ref")) => {
                    let mut with_id = (1..es.len()).filter(|c| es[*c].parent == Some(0) && es[*c].value("id") == Some(r));
                    let one = with_id.next()?;
                    with_id.next().is_none().then_some(one)?
                }
                (None, [c]) if is_bean(*c) => *c,
                _ => return None,
            };
            simple_xsd(bean)
        });
        out.push(Def {
            anchor: range(&doc.text, es[i].at, es[i].at + es[i].name.len()),
            port_type: get("portTypeName").is_some(),
            location: get("locationUri").is_some(),
            schema: get("schema").or_else(|| get("schemaCollection")).is_some(),
            suffixes: suffixes(literal_of("requestSuffix"), literal_of("responseSuffix")),
            xsd,
        });
    }
    out
}

// ---- Java ----

/// `new A.B(args)` spanning the whole slice: the written class name and the argument tokens.
fn constructor<'a, 'b>(t: &'b [Tok<'a>]) -> Option<(String, &'b [Tok<'a>])> {
    let (first, rest) = t.split_first()?;
    let open = rest.iter().position(|x| x.s == "(")?;
    let name_tokens = &rest[..open];
    if first.s != "new" || name_tokens.is_empty() || name_tokens.iter().any(|x| !x.is_ident() && x.s != ".") || java::closing(rest, open) != rest.len() - 1 {
        return None;
    }
    Some((name_tokens.iter().map(|x| x.s).collect(), &rest[open + 1..rest.len() - 1]))
}

fn xsd_resource(owners: &annotation_owners::Owners, expr: &[Tok], text: &str) -> Option<(String, Range)> {
    let (schema, args) = constructor(expr)?;
    if !owners.written_known(&schema, SIMPLE_XSD) {
        return None;
    }
    let (resource, inner) = constructor(args)?;
    if !owners.resource_constructor(&resource) {
        return None;
    }
    let [token] = inner else { return None };
    let s = token.string()?;
    if s.value.contains(['\\', ':', '$']) || text.get(s.at..s.end) != Some(s.value.as_str()) {
        return None;
    }
    let path = xsd_path(&format!("classpath:{}", s.value))?;
    Some((path, range(text, s.at, s.end)))
}

fn java_definitions(doc: &Document) -> Vec<Def> {
    let text = &doc.text;
    let file = parse_source(&doc.path, text);
    let owners = annotation_owners::Owners::new(&file, text, false);
    let all = java::tokens(text);
    let body_tokens = |m: &java::Member| -> Option<Vec<Tok>> {
        let (s, e) = m.body?;
        let mut toks: Vec<Tok> = all.iter().filter(|t| t.at >= s && t.at < e).copied().collect();
        if toks.first().is_some_and(|t| t.s == "{") {
            toks.remove(0);
        }
        if toks.last().is_some_and(|t| t.s == "}") {
            toks.pop();
        }
        let blocked = ["{", "}", "if", "for", "while", "do", "switch", "try", "throw", "synchronized", "->", "?", "::"];
        (!toks.iter().any(|t| blocked.contains(&t.s))).then_some(toks)
    };
    let mut out = vec![];
    for ty in &file.types {
        for m in ty.methods() {
            let Some(toks) = body_tokens(m) else { continue };
            let statements: Vec<&[Tok]> = toks.split(|t| t.s == ";").filter(|s| !s.is_empty()).collect();
            let mut var: Option<(&str, Range)> = None;
            let mut setters: Vec<(&str, &[Tok])> = vec![];
            let mut locals: Vec<(&str, Option<(String, Range)>)> = vec![];
            let mut returned = false;
            let mut ok = true;
            for st in &statements {
                if returned {
                    ok = false;
                    break;
                }
                let mentions = |name: &str| st.iter().any(|t| t.s == name);
                if let Some(p) = st.iter().position(|t| t.s == "=").filter(|p| *p >= 2) {
                    let (lhs, rhs) = (&st[..p], &st[p + 1..]);
                    let name = lhs[p - 1];
                    if let Some((class, args)) = constructor(rhs).filter(|_| name.is_ident()) {
                        let written: String = lhs[..p - 1].iter().map(|t| t.s).collect();
                        if owners.written_known(&class, DEFINITION) && args.is_empty() && (written == "var" || owners.written_known(&written, DEFINITION)) {
                            if var.is_some() {
                                ok = false;
                                break;
                            }
                            let at = rhs[1];
                            var = Some((name.s, range(text, at.at, at.end())));
                            continue;
                        }
                        if owners.written_known(&class, SIMPLE_XSD) {
                            locals.push((name.s, xsd_resource(&owners, rhs, text)));
                            continue;
                        }
                    }
                }
                let Some((v, _)) = var else { continue };
                if st.len() >= 5 && st[0].s == v && st[1].s == "." && st[3].s == "(" && java::closing(st, 3) == st.len() - 1 {
                    if setters.iter().any(|(n, _)| *n == st[2].s) {
                        ok = false;
                        break;
                    }
                    setters.push((st[2].s, &st[4..st.len() - 1]));
                } else if st.len() == 2 && st[0].s == "return" && st[1].s == v {
                    returned = true;
                } else if mentions(v) {
                    ok = false;
                    break;
                }
            }
            let Some((_, anchor)) = var.filter(|_| ok && returned) else { continue };
            let arg = |n: &str| setters.iter().find(|(s, _)| *s == n).map(|(_, a)| *a);
            let literal_of = |n: &str| {
                arg(n).map(|a| match a {
                    [t] => t.string().map(|s| s.value),
                    _ => None,
                })
            };
            let xsd = arg("setSchema").and_then(|a| match a {
                [t] if t.is_ident() => match locals.iter().find(|(n, _)| *n == t.s) {
                    Some((_, resolved)) => resolved.clone(),
                    None => {
                        let mut sibling = ty.methods().filter(|s| s.name == t.s && s.params.is_empty());
                        let first = sibling.next()?;
                        let only = sibling.next().is_none();
                        let (s, e) = first.body?;
                        let body: Vec<Tok> = all.iter().filter(|x| x.at > s && x.at + x.s.len() < e && x.s != ";").copied().collect();
                        match body.split_first() {
                            Some((r, expr)) if only && r.s == "return" && expr.last().is_some_and(|x| x.s == ")") => xsd_resource(&owners, expr, text),
                            _ => None,
                        }
                    }
                },
                expr => xsd_resource(&owners, expr, text),
            });
            out.push(Def {
                anchor,
                port_type: arg("setPortTypeName").is_some(),
                location: arg("setLocationUri").is_some(),
                schema: arg("setSchema").or_else(|| arg("setSchemaCollection")).is_some(),
                suffixes: suffixes(literal_of("setRequestSuffix"), literal_of("setResponseSuffix")),
                xsd,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::diagnostics::DiagnosticsProvider;
    use openidea_plugin::{Plugin, mock::MockHost, providers::Providers, text::position};

    const XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:hr"><xs:element name="HolidayRequest"/><xs:element name="FireRequest"/><xs:element name="FireResponse"/></xs:schema>"#;
    const IMPORTS: &str = "import org.springframework.context.annotation.Bean;\nimport org.springframework.core.io.ClassPathResource;\nimport org.springframework.ws.wsdl.wsdl11.DefaultWsdl11Definition;\nimport org.springframework.xml.xsd.SimpleXsdSchema;\nimport org.springframework.xml.xsd.XsdSchema;\n";

    fn java(body: &str) -> Document {
        Document {
            path: "/project/src/main/java/C.java".into(),
            text: format!("{IMPORTS}class C {{\n{body}\n}}"),
            language: "java".into(),
            version: 1,
        }
    }

    fn xml(body: &str) -> Document {
        Document {
            path: "/project/src/main/resources/ws.xml".into(),
            text: format!("<beans xmlns='{BEANS}'>{body}</beans>"),
            language: "xml".into(),
            version: 1,
        }
    }

    fn tools() -> (MockHost, WsTools) {
        let h = MockHost::install();
        h.add_file("src/main/resources/hr.xsd", XSD);
        let mut ws = WsTools::default();
        ws.activate(&h.context("aritti.spring-ws")).unwrap();
        (h, ws)
    }

    fn codes(ws: &mut WsTools, d: &Document) -> Vec<String> {
        ws.diagnostics(d).into_iter().filter_map(|p| p.code).filter(|c| c.starts_with("wsdl-")).collect()
    }

    const FULL: &str = "@Bean(name=\"x\") public DefaultWsdl11Definition x(XsdSchema schema) { DefaultWsdl11Definition d = new DefaultWsdl11Definition(); d.setPortTypeName(\"P\"); d.setLocationUri(\"/ws\"); d.setTargetNamespace(\"urn:x\"); d.setSchema(schema); return d; }";

    #[test]
    fn complete_java_definition_is_quiet_and_missing_properties_are_flagged() {
        let (_h, mut ws) = tools();
        assert!(codes(&mut ws, &java(FULL)).is_empty());
        let d = java(&FULL.replace(" d.setPortTypeName(\"P\");", "").replace(" d.setLocationUri(\"/ws\");", "").replace(" d.setSchema(schema);", ""));
        let found = ws.diagnostics(&d);
        let sev = |c: &str| found.iter().find(|p| p.code.as_deref() == Some(c)).map(|p| p.severity);
        assert_eq!(sev("wsdl-missing-port-type-name"), Some(Severity::Warning));
        assert_eq!(sev("wsdl-missing-schema"), Some(Severity::Warning));
        assert_eq!(sev("wsdl-missing-location-uri"), Some(Severity::Info));
    }

    #[test]
    fn java_definitions_not_fully_visible_stay_quiet() {
        let (_h, mut ws) = tools();
        for body in [
            "public DefaultWsdl11Definition x() { DefaultWsdl11Definition d = new DefaultWsdl11Definition(); configure(d); return d; }",
            "public DefaultWsdl11Definition x(boolean b) { DefaultWsdl11Definition d = new DefaultWsdl11Definition(); if (b) { d.setPortTypeName(\"P\"); } return d; }",
            "public DefaultWsdl11Definition x() { DefaultWsdl11Definition d = new DefaultWsdl11Definition(); return other(d); }",
            "public Object x() { DefaultWsdl11Definition d = new DefaultWsdl11Definition(); return null; }",
        ] {
            assert!(codes(&mut ws, &java(body)).is_empty(), "{body}");
        }
        let foreign = Document { text: java(FULL).text.replace("import org.springframework.ws.wsdl.wsdl11.DefaultWsdl11Definition;", "import foreign.DefaultWsdl11Definition;"), ..java("") };
        assert!(codes(&mut ws, &foreign).is_empty());
    }

    #[test]
    fn java_pairing_follows_local_inline_and_sibling_schema_literals() {
        let (_h, mut ws) = tools();
        let literal = "new SimpleXsdSchema(new ClassPathResource(\"hr.xsd\"))";
        for body in [
            format!("public DefaultWsdl11Definition x() {{ XsdSchema s = {literal}; DefaultWsdl11Definition d = new DefaultWsdl11Definition(); d.setPortTypeName(\"P\"); d.setLocationUri(\"/w\"); d.setSchema(s); return d; }}"),
            format!("public DefaultWsdl11Definition x() {{ DefaultWsdl11Definition d = new DefaultWsdl11Definition(); d.setPortTypeName(\"P\"); d.setLocationUri(\"/w\"); d.setSchema({literal}); return d; }}"),
            format!("@Bean public XsdSchema hr() {{ return {literal}; }}\n{}", FULL.replace("XsdSchema schema", "XsdSchema hr").replace("setSchema(schema)", "setSchema(hr)")),
        ] {
            let d = java(&body);
            let found: Vec<_> = ws.diagnostics(&d).into_iter().filter(|p| p.code.as_deref() == Some("wsdl-unpaired-request")).collect();
            assert_eq!(found.len(), 1, "{body}");
            assert!(found[0].message.contains("HolidayRequest") && found[0].message.contains("HolidayResponse"));
            assert_eq!(found[0].severity, Severity::Info);
            let from = d.text.find("hr.xsd").unwrap();
            assert_eq!(found[0].range, range(&d.text, from, from + 6));
        }
    }

    #[test]
    fn java_pairing_stays_quiet_for_unresolved_or_custom_suffix() {
        let (_h, mut ws) = tools();
        let literal = "new SimpleXsdSchema(new ClassPathResource(\"hr.xsd\"))";
        let base = format!("public DefaultWsdl11Definition x() {{ DefaultWsdl11Definition d = new DefaultWsdl11Definition(); d.setPortTypeName(\"P\"); d.setLocationUri(\"/w\"); d.setSchema({literal}); return d; }}");
        assert!(!codes(&mut ws, &java(&base)).is_empty());
        for body in [
            base.replace("return d;", "d.setRequestSuffix(\"Query\"); return d;"),
            base.replace("return d;", "d.setResponseSuffix(suffix()); return d;"),
            base.replace("hr.xsd", "gone.xsd"),
            base.replace("d.setSchema(", "d.setSchema(unknown("),
            base.replace("hr.xsd", "${x}.xsd"),
        ] {
            assert!(codes(&mut ws, &java(&body)).is_empty(), "{body}");
        }
    }

    #[test]
    fn java_resource_literal_navigates_to_the_xsd() {
        let (_h, mut ws) = tools();
        let d = java(&FULL.replace("XsdSchema schema) {", "XsdSchema schema) { Object o = new SimpleXsdSchema(new ClassPathResource(\"hr.xsd\"));"));
        let at = d.text.find("hr.xsd").unwrap() + 2;
        let found = ws.definition(&d, position(&d.text, at));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, "/project/src/main/resources/hr.xsd");
    }

    const WSDL_CLASS: &str = "org.springframework.ws.wsdl.wsdl11.DefaultWsdl11Definition";
    const XSD_CLASS: &str = "org.springframework.xml.xsd.SimpleXsdSchema";

    fn bean(props: &str) -> String {
        format!("<bean id='w' class='{WSDL_CLASS}'>{props}</bean><bean id='hr' class='{XSD_CLASS}'><property name='xsd' value='classpath:hr.xsd'/></bean>")
    }

    #[test]
    fn xml_required_properties_and_ref_pairing() {
        let (_h, mut ws) = tools();
        let full = xml(&bean("<property name='portTypeName' value='P'/><property name='locationUri' value='/w'/><property name='schema' ref='hr'/>"));
        assert_eq!(codes(&mut ws, &full), vec!["wsdl-unpaired-request"]);
        let d = xml(&bean("<property name='locationUri' value='/w'/>"));
        let c = codes(&mut ws, &d);
        assert!(c.contains(&"wsdl-missing-port-type-name".to_owned()) && c.contains(&"wsdl-missing-schema".to_owned()));
        let d = xml(&bean("<property name='portTypeName' value='P'/><property name='schemaCollection' ref='hr'/>"));
        assert_eq!(codes(&mut ws, &d), vec!["wsdl-missing-location-uri"]);
    }

    #[test]
    fn xml_inline_schema_bean_pairs_and_unsure_shapes_stay_quiet() {
        let (_h, mut ws) = tools();
        let inline = format!("<bean class='{WSDL_CLASS}'><property name='portTypeName' value='P'/><property name='locationUri' value='/w'/><property name='schema'><bean class='{XSD_CLASS}'><property name='xsd' value='classpath:hr.xsd'/></bean></property></bean>");
        assert_eq!(codes(&mut ws, &xml(&inline)), vec!["wsdl-unpaired-request"]);
        for body in [
            bean("<property name='portTypeName' value='P'/><property name='schema' ref='gone'/>").replace("<property name='schema' ref='gone'/>", "<property name='schema' ref='gone'/><property name='locationUri' value='/w'/>"),
            bean("<property name='portTypeName' value='P'/><property name='locationUri' value='/w'/><property name='schema' ref='hr'/><property name='requestSuffix' value='${s}'/>"),
            bean("<property name='portTypeName' value='P'/><property name='schema' ref='hr'/>").replace("<bean id='w'", "<bean id='w' parent='base'"),
            bean("<property name='portTypeName' value='P'/><property name='schema' ref='hr'/>").replace("<bean id='w'", "<bean id='w' p:schema-ref='hr'"),
            bean("<property name='${p}' value='P'/>"),
        ] {
            let c = codes(&mut ws, &xml(&body));
            assert!(c.iter().all(|c| c != "wsdl-unpaired-request" && c != "wsdl-missing-port-type-name" && c != "wsdl-missing-schema"), "{body}: {c:?}");
        }
    }

    #[test]
    fn xml_simple_xsd_literal_navigates() {
        let (_h, mut ws) = tools();
        let d = xml(&bean("<property name='portTypeName' value='P'/>"));
        let at = d.text.find("classpath:").unwrap() + 12;
        let found = ws.definition(&d, position(&d.text, at));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, "/project/src/main/resources/hr.xsd");
    }
}
