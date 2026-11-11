//! Same-document Spring-WS marshaller bean references, not effective context/type resolution.
use crate::{SWS_NAMESPACE, generate, xml_namespace};
use openidea_plugin::{
    Location, Position, Range,
    providers::{CompletionItem, CompletionKind, Document},
    text::offset,
};
use spring_text::{contains, range, xml};
use std::collections::BTreeMap;
const BEANS: &str = "http://www.springframework.org/schema/beans";
const MAX_ITEMS: usize = 256;
const OXM:&str="http://www.springframework.org/schema/oxm";
const CXF:&str="http://cxf.apache.org/jaxws";
fn literal(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}
struct Model {
    beans: BTreeMap<String, Range>,
    references: Vec<(String, Range)>,
}
fn model(text: &str) -> Option<Model> {
    if !generate::xml_valid(text) {
        return None;
    }
    let elements = xml::elements(text);
    let mut depths: Vec<usize> = Vec::with_capacity(elements.len());
    for element in &elements {
        let depth = element.parent.map_or(0, |parent| depths[parent] + 1);
        if depth > 64 {
            return None;
        }
        depths.push(depth);
    }
    if elements.iter().any(|e| e.name.matches(':').count() > 1 || e.attrs.iter().any(|a| a.name.matches(':').count() > 1)) {
        return None;
    }
    let root = elements.first()?;
    if root.local() != "beans" || xml_namespace(&elements, 0) != Some(BEANS) || root.attrs.iter().any(|a| a.name == "profile") {
        return None;
    }
    // Context imports, aliases and nested/profile contexts can change the binding.
    // Do not present a guessed effective bean owner when these forms are present.
    if elements
        .iter()
        .enumerate()
        .skip(1)
        .any(|(i, e)| xml_namespace(&elements, i) == Some(BEANS) && matches!(e.local(), "import" | "alias" | "beans"))
    {
        return None;
    }
    let mut beans = BTreeMap::new();
    let mut duplicates = std::collections::BTreeSet::new();
    let mut references = Vec::new();
    for (i, e) in elements.iter().enumerate().skip(1) {
        let namespace = xml_namespace(&elements, i);
        if e.parent == Some(0) && (e.local() == "bean" && namespace == Some(BEANS) || e.local()=="jaxb2-marshaller" && namespace==Some(OXM)) {
            if e.attrs.iter().any(|a| a.name == "name") {
                return None;
            }
            let Some(id) = e.attrs.iter().find(|a| a.name == "id") else {
                continue;
            };
            if !literal(&id.value) {
                return None;
            }
            if beans.insert(id.value.clone(), range(text, id.value_at, id.value_end)).is_some() {
                duplicates.insert(id.value.clone());
            }
            if beans.len() > MAX_ITEMS {
                return None;
            }
        }
        // The Spring-WS interceptor vocabulary accepts direct Beans ref
        // children, including scoped payloadRoot/soapAction interceptors.
        // Resolve only the written local bean id; no interceptor type inference.
        let interceptor_ref = namespace == Some(BEANS) && e.local() == "ref"
            && e.parent.is_some_and(|parent| {
                let owner = &elements[parent];
                xml_namespace(&elements, parent) == Some(SWS_NAMESPACE)
                    && (owner.local() == "interceptors" && owner.parent == Some(0)
                        || matches!(owner.local(), "payloadRoot" | "soapAction")
                            && owner.parent.is_some_and(|grandparent| {
                                let group = &elements[grandparent];
                                group.local() == "interceptors" && group.parent == Some(0)
                                    && xml_namespace(&elements, grandparent) == Some(SWS_NAMESPACE)
                            }))
            });
        if interceptor_ref && e.attrs.len() == 1 {
            if let Some(a) = e.attrs.iter().find(|a| a.name == "bean") {
                if a.value.is_empty() || literal(&a.value) {
                    references.push((a.value.clone(), range(text, a.value_at, a.value_end)));
                    if references.len() > MAX_ITEMS { return None; }
                }
            }
        }
        if e.parent==Some(0) && namespace==Some(CXF) && matches!(e.local(),"endpoint"|"server"|"client") {
            for a in &e.attrs {
                let bean=if a.name=="bus" {Some((a.value.as_str(),a.value_at))}
                    else if (e.local()=="endpoint"&&a.name=="implementor" || e.local()=="server"&&a.name=="serviceBean") && a.value.starts_with('#') {Some((&a.value[1..],a.value_at+1))}
                    else {None};
                if let Some((name,start))=bean.filter(|(name,_)|name.is_empty()||literal(name)) {
                    references.push((name.into(),range(text,start,a.value_end)));
                    if references.len()>MAX_ITEMS{return None;}
                }
            }
        }
        if e.parent == Some(0) && matches!(e.local(), "marshalling-endpoints" | "annotation-driven") && namespace == Some(SWS_NAMESPACE) {
            for a in e.attrs.iter().filter(|a| matches!(a.name.as_str(), "marshaller" | "unmarshaller")) {
                if a.value.len() <= 128 && (a.value.is_empty() || literal(&a.value)) {
                    references.push((a.value.clone(), range(text, a.value_at, a.value_end)));
                    if references.len() > MAX_ITEMS {
                        return None;
                    }
                }
            }
        }
    }
    for id in duplicates {
        beans.remove(&id);
    }
    Some(Model { beans, references })
}
pub(super) fn definition(doc: &Document, at: Position) -> Vec<Location> {
    let Some(model) = model(&doc.text) else {
        return vec![];
    };
    let Some((name, _)) = model.references.iter().find(|(_, r)| contains(*r, at)) else {
        return vec![];
    };
    model.beans.get(name).map(|r| vec![Location { path: doc.path.clone(), range: *r }]).unwrap_or_default()
}
pub(super) fn completion(doc: &Document, at: Position) -> Vec<CompletionItem> {
    let Some(model) = model(&doc.text) else {
        return vec![];
    };
    let Some((_, r)) = model.references.iter().find(|(_, r)| contains(*r, at)) else {
        return vec![];
    };
    let Some(start) = offset(&doc.text, r.start) else {
        return vec![];
    };
    let Some(end) = offset(&doc.text, at) else {
        return vec![];
    };
    let Some(prefix) = doc.text.get(start..end) else {
        return vec![];
    };
    model
        .beans
        .into_keys()
        .filter(|name| name.starts_with(prefix))
        .map(|name| CompletionItem {
            detail: Some("Literal bean id in this XML document; runtime type/context not resolved".into()),
            replace: Some(*r),
            ..CompletionItem::new(&name, CompletionKind::Variable)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WsTools;
    use openidea_plugin::{mock::MockHost, providers::Providers, text::position};
    const START: &str =
        r#"<b:beans xmlns:b="http://www.springframework.org/schema/beans" xmlns:s="http://www.springframework.org/schema/web-services">"#;
    fn document(body: &str) -> Document {
        Document { path: "/project/context.xml".into(), language: "xml".into(), version: 19, text: format!("{START}{body}</b:beans>") }
    }
    fn last(d: &Document) -> Position {
        position(&d.text, d.text.rfind("codec").unwrap())
    }
    #[test]
    fn interceptor_reference_children_resolve_only_in_the_sws_vocabulary() {
        let body = r#"<b:bean id="audit" class="example.Audit"/><s:interceptors><b:ref bean="audit"/><s:payloadRoot namespaceUri="urn:test" localPart="Request"><b:ref bean="audit"/></s:payloadRoot><s:soapAction value="urn:action"><b:ref bean="audit"/></s:soapAction></s:interceptors>"#;
        let d = document(body);
        let observed = model(&d.text).unwrap();
        assert_eq!(observed.references.len(), 3);
        for (_, span) in observed.references {
            let targets = definition(&d, span.start);
            assert_eq!(targets.len(), 1);
            assert_eq!(targets[0].range, observed.beans["audit"]);
            assert_eq!(completion(&d, span.start).iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["audit"]);
        }
        for foreign in [body.replace("s:interceptors", "b:interceptors"), body.replace("<b:ref", "<s:ref")] {
            assert!(model(&document(&foreign).text).unwrap().references.is_empty());
        }
        let unrelated = document(r#"<b:bean id="audit"/><b:ref bean="audit"/>"#);
        assert!(model(&unrelated.text).unwrap().references.is_empty());
    }
    #[test]
    fn exact_local_marshaller_and_unmarshaller_targets_have_utf16_source_ranges() {
        let d = document(
            "<!-- 🦀 --><b:bean id=\"codec\" class=\"example.Codec\"/>\n<s:marshalling-endpoints marshaller=\"codec\" unmarshaller=\"codec\"/>",
        );
        for attr in ["marshaller=", "unmarshaller="] {
            let at = position(&d.text, d.text.find(attr).unwrap() + attr.len() + 2);
            let r = definition(&d, at);
            assert_eq!(r.len(), 1);
            assert_eq!(r[0].path, d.path);
            let start = d.text.find("id=\"codec").unwrap() + 4;
            assert_eq!(r[0].range, range(&d.text, start, start + 5));
            assert_eq!(r[0].range, Range::new(Position::new(0, 147), Position::new(0, 152)));
        }
    }
    #[test]
    fn completion_is_prefix_filtered_replaces_whole_literal_and_needs_no_host_io() {
        let _h = MockHost::install().with_capabilities(&[]);
        let d = document(r#"<b:bean id="codec"/><b:bean id="other"/><s:marshalling-endpoints marshaller="co"/>"#);
        let at = position(&d.text, d.text.rfind("co\"").unwrap() + 2);
        let mut p = WsTools::default();
        let items = p.completion(&d, at, None);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "codec");
        assert_eq!(items[0].replace.unwrap().end, at);
        assert!(items[0].detail.as_ref().unwrap().contains("runtime type/context not resolved"));
        let changed = Document { text: d.text.replace("marshaller=\"co\"", "marshaller=\"codec\""), ..d };
        assert_eq!(p.definition(&changed, at).len(), 1);
    }
    #[test]
    fn empty_literal_lists_only_unique_direct_same_document_ids() {
        let d = document(
            r#"<b:bean id="a"/><b:bean id="dup"/><b:bean id="dup"/><b:bean><b:bean id="nested"/></b:bean><s:marshalling-endpoints marshaller=""/>"#,
        );
        let at = position(&d.text, d.text.find("marshaller=\"\"").unwrap() + 12);
        let items = completion(&d, at);
        assert_eq!(items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), vec!["a"]);
    }
    #[test]
    fn imports_aliases_profiles_nested_contexts_and_named_aliases_withhold_ownership() {
        for extra in [
            r#"<b:import resource="other.xml"/>"#,
            r#"<b:alias name="x" alias="codec"/>"#,
            "<b:beans/>",
            r#"<b:bean id="alias" name="codec"/>"#,
        ] {
            let d = document(&format!(r#"<b:bean id="codec"/>{extra}<s:marshalling-endpoints marshaller="codec"/>"#));
            assert!(model(&d.text).is_none(), "{extra}");
        }
        let d = document(r#"<b:bean id="codec"/><s:marshalling-endpoints marshaller="codec"/>"#);
        assert!(model(&d.text.replace("<b:beans ", "<b:beans profile=\"dev\" ")).is_none());
    }
    #[test]
    fn foreign_unbound_shadowed_and_qualified_attributes_never_resolve() {
        for reference in [
            r#"<x:marshalling-endpoints marshaller="codec"/>"#,
            r#"<s:marshalling-endpoints xmlns:s="urn:foreign" marshaller="codec"/>"#,
            r#"<s:marshalling-endpoints s:marshaller="codec"/>"#,
            r#"<s:other marshaller="codec"/>"#,
        ] {
            let d = document(&format!(r#"<b:bean id="codec"/>{reference}"#));
            assert!(definition(&d, last(&d)).is_empty());
            assert!(completion(&d, last(&d)).is_empty());
        }
        let d = document(r#"<x:bean id="codec"/><s:marshalling-endpoints marshaller="codec"/>"#);
        assert!(definition(&d, last(&d)).is_empty());
    }
    #[test]
    fn malformed_dynamic_entities_and_oversized_sources_are_quiet() {
        for text in [
            format!(r#"{START}<b:bean id="codec"/><s:marshalling-endpoints marshaller="codec"/>"#),
            format!(r#"{START}<b:bean id="codec"/><s:marshalling-endpoints marshaller="codec" marshaller="codec"/></b:beans>"#),
            format!("<!DOCTYPE beans [<!ENTITY x 'codec'>]>{START}</b:beans>"),
            " ".repeat(128 * 1024 + 1),
        ] {
            assert!(model(&text).is_none());
        }
        for value in ["${codec}", "#{bean}", "codec&amp;x", "a b", "&codec"] {
            let d = document(&format!(r#"<b:bean id="codec"/><s:marshalling-endpoints marshaller="{value}"/>"#));
            assert!(model(&d.text).map_or(true, |m| m.references.is_empty()));
        }
    }
    #[test]
    fn default_namespaces_and_current_document_identity_do_not_cross_files() {
        let mut d = document(r#"<b:bean id="codec"/><s:marshalling-endpoints marshaller="codec"/>"#);
        d.text = d.text.replace("b:beans", "beans").replace("b:bean", "bean").replace("xmlns:b=", "xmlns=");
        let at = last(&d);
        assert_eq!(definition(&d, at)[0].path, "/project/context.xml");
        let other = Document { path: "/other/context.xml".into(), ..d.clone() };
        assert_eq!(definition(&other, at)[0].path, other.path);
        d.text = d.text.replace("id=\"codec\"", "id=\"changed\"");
        assert!(definition(&d, at).is_empty());
    }
    #[test]
    fn bounded_declared_sets_and_ambiguous_ids_are_not_guessed() {
        let deep = format!("{START}{}{}</b:beans>", "<foreign>".repeat(65), "</foreign>".repeat(65));
        assert!(model(&deep).is_none());
        let d = document(&format!(
            r#"{}<s:marshalling-endpoints marshaller="v0"/>"#,
            (0..257).map(|i| format!(r#"<b:bean id="v{i}"/>"#)).collect::<String>()
        ));
        assert!(model(&d.text).is_none());
        let d = document(r#"<b:bean id="codec"/><b:bean id="codec"/><s:marshalling-endpoints marshaller="codec"/>"#);
        assert!(definition(&d, last(&d)).is_empty());
    }
    #[test]
    fn reserved_namespace_rebinding_aliases_and_default_uris_are_quiet() {
        let text = "<b:beans xmlns:b='http://www.springframework.org/schema/beans' xmlns:xml='http://www.springframework.org/schema/web-services'><b:bean id='codec'/><xml:marshalling-endpoints marshaller='codec'/></b:beans>";
        assert!(model(text).is_none());
        for declaration in [
            "xmlns:xml='urn:foreign'",
            "xmlns:xmlns='urn:foreign'",
            "xmlns:q='http://www.w3.org/XML/1998/namespace'",
            "xmlns:q='http://www.w3.org/2000/xmlns/'",
            "xmlns:q='http://www.w3.org/XML/1998/namespac&#101;'",
            "xmlns:q=''",
        ] {
            let mut d = document("<b:bean id='codec'/><s:marshalling-endpoints marshaller='codec'/>");
            d.text = d.text.replace("<b:beans ", &format!("<b:beans {declaration} "));
            assert!(definition(&d, last(&d)).is_empty(), "{declaration}");
            assert!(completion(&d, last(&d)).is_empty());
        }
        for uri in ["http://www.w3.org/XML/1998/namespace", "http://www.w3.org/2000/xmlns/", "http://www.w3.org/2000/xmln&#115;/"] {
            let d = document(&format!("<b:bean id='codec'/><s:marshalling-endpoints xmlns='{uri}' marshaller='codec'/>"));
            assert!(definition(&d, last(&d)).is_empty());
        }
    }
    #[test]
    fn legal_explicit_and_implicit_xml_prefix_remain_usable() {
        for declaration in ["", "xmlns:xml='http://www.w3.org/XML/1998/namespace'"] {
            let d = document(&format!("<b:bean id='codec' {declaration} xml:lang='en'/><s:marshalling-endpoints marshaller='codec'/>"));
            assert_eq!(definition(&d, last(&d)).len(), 1);
            assert_eq!(completion(&d, last(&d)).len(), 1);
        }
    }
}

#[cfg(test)]
mod unknown_bean_identity {
    use super::*;
    #[test]
    fn encoded_or_dynamic_declared_ids_cannot_hide_duplicate_identity() {
        for id in ["co&#100;ec", "${codec}", "a b"] {
            let text = format!(
                r#"<beans xmlns="http://www.springframework.org/schema/beans" xmlns:s="http://www.springframework.org/schema/web-services"><bean id="codec"/><bean id="{id}"/><s:marshalling-endpoints marshaller="codec"/></beans>"#
            );
            assert!(model(&text).is_none());
        }
    }
}

/// Explicit Spring-WS payload-root interceptor selectors reference the same
/// global element identity as source PayloadRoot and absolute XPath roots.
pub(crate) fn payload_qnames(doc:&Document)->Vec<(String,String,Range)> {
    if doc.language!="xml" || doc.text.len()>32768 || !generate::xml_valid(&doc.text) {return vec![];}
    let es=xml::elements(&doc.text);
    if es.len()>512 || es.first().is_none_or(|e|e.local()!="beans") || xml_namespace(&es,0)!=Some(BEANS) {return vec![];}
    let mut out=vec![];
    for (i,e) in es.iter().enumerate() {
        if e.local()!="payloadRoot" || xml_namespace(&es,i)!=Some(SWS_NAMESPACE) {continue;}
        let Some(parent)=e.parent else {continue;};
        if es[parent].local()!="interceptors" || es[parent].parent!=Some(0) || xml_namespace(&es,parent)!=Some(SWS_NAMESPACE) {continue;}
        let Some(uri)=e.attrs.iter().find(|a|a.name=="namespaceUri") else {continue;};
        let Some(local)=e.attrs.iter().find(|a|a.name=="localPart") else {continue;};
        if !crate::schema_qnames::local(&local.value) || uri.value.len()>1024
            || uri.value.chars().any(|c|c.is_whitespace()||c.is_control())
            || uri.value.contains(['&','$','\\']) || uri.value.contains("#{") {continue;}
        out.push((uri.value.clone(),local.value.clone(),range(&doc.text,local.value_at,local.value_end)));
    }
    out
}

#[cfg(test)]
mod annotation_driven_tests {
    use super::*;
    use crate::WsTools;
    use openidea_plugin::{mock::MockHost,providers::Providers,text::position};
    #[test]
    fn current_annotation_driven_marshaller_attributes_resolve_local_ids() {
        let _h=MockHost::install().with_capabilities(&[]);
        let text=format!("<b:beans xmlns:b='{BEANS}' xmlns:s='{SWS_NAMESPACE}'><b:bean id='codec'/><s:annotation-driven marshaller='codec' unmarshaller='codec'/></b:beans>");
        let doc=Document{path:"/project/context.xml".into(),text:text.clone(),language:"xml".into(),version:1};
        let at=position(&text,text.find("marshaller='codec'").unwrap()+12);
        let locations=WsTools::default().definition(&doc,at);assert_eq!(locations.len(),1);
        let values=WsTools::default().completion(&doc,at,None);assert_eq!(values.len(),1);assert_eq!(values[0].label,"codec");
        let foreign=Document{text:text.replace("s:annotation-driven","b:annotation-driven"),..doc};assert!(WsTools::default().definition(&foreign,at).is_empty());
    }
}

#[cfg(test)]
mod oxm_cxf_reference_tests {
    use super::*;
    use crate::WsTools;
    use openidea_plugin::{mock::MockHost,providers::Providers,text::{position,offset}};
    #[test]
    fn oxm_marshaller_ids_and_cxf_explicit_bean_references_keep_their_written_roles() {
        let _h=MockHost::install().with_capabilities(&[]);
        let text=format!("<b:beans xmlns:b='{BEANS}' xmlns:s='{SWS_NAMESPACE}' xmlns:o='{OXM}' xmlns:c='{CXF}'><o:jaxb2-marshaller id='codec' context-path='com.acme'/><b:bean id='bus'/><b:bean id='service'/><s:annotation-driven marshaller='codec'/><c:endpoint bus='bus' implementor='#service'/><c:server serviceBean='#service'/></b:beans>");
        let doc=Document{path:"/project/context.xml".into(),text:text.clone(),language:"xml".into(),version:1};
        for needle in ["marshaller='codec'","bus='bus'","implementor='#service'","serviceBean='#service'"] {
            let start=text.find(needle).unwrap()+needle.find('=').unwrap()+2+usize::from(needle.contains('#'));let at=position(&text,start);
            let definitions=WsTools::default().definition(&doc,at);assert_eq!(definitions.len(),1,"{needle}");
            let values=WsTools::default().completion(&doc,at,None);assert_eq!(values.len(),3);assert!(values.iter().all(|v|!v.label.starts_with('#')));
            if needle.contains('#') {let span=values[0].replace.unwrap();assert_eq!(&text[offset(&text,span.start).unwrap()..offset(&text,span.end).unwrap()],"service");assert_eq!(text.as_bytes()[offset(&text,span.start).unwrap()-1],b'#');}
        }
        for changed in [text.replace("c:endpoint","b:endpoint"),text.replace("xmlns:c=", "xmlns:foreign=").replace("c:","foreign:")] {
            let foreign=Document{text:changed.clone(),..doc.clone()};let at=position(&changed,changed.find("#service").unwrap()+1);
            // Renaming the prefix preserves ownership only when its URI remains CXF.
            let locations=WsTools::default().definition(&foreign,at);
            if changed.contains("<b:endpoint"){assert!(locations.is_empty());}else{assert_eq!(locations.len(),1);}
        }
    }
}
