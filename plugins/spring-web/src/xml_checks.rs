//! Bounded MVC XML contracts from official current and versioned schema profiles.
//! These source checks do not implement a complete XML DOM/XSD/type resolver.
#[path = "mvc_xml_model.rs"]
mod model;
use openidea_plugin::{
    Location, Position,
    providers::{CompletionItem, Document},
};
use spring_text::finding::Finding;
pub fn check(text: &str) -> Vec<Finding> {
    model::check(text)
}
pub(crate) fn definition(doc: &Document, at: Position) -> Vec<Location> {
    model::definition(doc, at)
}
pub(crate) fn completion(doc: &Document, at: Position) -> Vec<CompletionItem> {
    model::completion(doc, at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{
        mock::MockHost,
        text::{offset, position},
    };
    const MVC: &str = "http://www.springframework.org/schema/mvc";
    const BEANS: &str = "http://www.springframework.org/schema/beans";
    fn doc(body: &str) -> Document {
        Document {
            path: "/project/context.xml".into(),
            text: format!("<b:beans xmlns:b='{BEANS}' xmlns:m='{MVC}'>{body}</b:beans>"),
            language: "xml".into(),
            version: 1,
        }
    }
    fn versioned(body: &str, version: &str) -> Document {
        let mut d = doc(body);
        d.text=d.text.replacen("<b:beans",&format!("<b:beans xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance' xsi:schemaLocation='{MVC} https://www.springframework.org/schema/mvc/spring-mvc-{version}.xsd'"),1);
        d
    }
    #[test]
    fn current_schema_checks_required_fields_in_their_actual_parent_roles() {
        let d = doc(
            "<m:view-controller/><m:resources mapping='/static/**'><m:resource-chain><m:resolvers><m:version-resolver><m:fixed-version-strategy/></m:version-resolver></m:resolvers></m:resource-chain></m:resources><m:cors><m:mapping/></m:cors><m:script-template-configurer><m:script/></m:script-template-configurer>",
        );
        let values = check(&d.text);
        assert_eq!(
            values
                .iter()
                .filter(|f| f.code == "missing-attribute")
                .count(),
            9
        );
        assert!(values.iter().any(|f| f.message.contains("engine-name")));
        assert!(values.iter().any(|f| f.message.contains("patterns")));
        assert!(values.iter().any(|f| f.message.contains("render-function")));
        let misplaced = doc(
            "<m:mapping/><m:resource-chain/><m:resources mapping='/x' location='/x'><m:mapping/></m:resources>",
        );
        assert!(check(&misplaced.text).is_empty());
    }
    #[test]
    fn booleans_accept_schema_lexical_forms_and_integers_keep_their_declared_width() {
        let d = doc(
            "<m:annotation-driven enable-matrix-variables='1' ignore-default-model-on-redirect=' false '><m:message-converters register-defaults='0'/><m:async-support default-timeout='2147483648'/></m:annotation-driven><m:status-controller path='/x' status-code='+200'/>",
        );
        assert!(check(&d.text).is_empty());
        let bad = doc(
            "<m:annotation-driven ignore-default-model-on-redirect='yes'><m:async-support default-timeout='9223372036854775808'/></m:annotation-driven><m:status-controller path='/x' status-code='2147483648'/>",
        );
        assert_eq!(check(&bad.text).len(), 3);
        let dynamic = doc(
            "<m:annotation-driven ignore-default-model-on-redirect='${redirect}'><m:async-support default-timeout='#{timeout}'/></m:annotation-driven>",
        );
        assert!(check(&dynamic.text).is_empty());
    }
    #[test]
    fn versioned_profiles_do_not_apply_renamed_or_later_attribute_contracts() {
        let legacy = versioned(
            "<m:annotation-driven ignoreDefaultModelOnRedirect='wrong' ignore-default-model-on-redirect='wrong'/>",
            "3.2",
        );
        let found = check(&legacy.text);
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("boolean"));
        let older = versioned(
            "<m:resources mapping='/x' location='/x'><m:resource-chain/></m:resources>",
            "4.0",
        );
        assert!(check(&older.text).is_empty());
        let modern = versioned(
            "<m:resources mapping='/x' location='/x'><m:resource-chain/></m:resources>",
            "4.1",
        );
        assert_eq!(check(&modern.text).len(), 1);
        let unsupported = versioned("<m:view-controller/>", "99.0");
        assert!(check(&unsupported.text).is_empty());
        let current = doc(
            "<m:annotation-driven><m:path-matching suffix-pattern='invalid'/></m:annotation-driven>",
        );
        assert!(check(&current.text).is_empty());
        let six = versioned(
            "<m:annotation-driven><m:path-matching suffix-pattern='invalid'/></m:annotation-driven>",
            "6.2",
        );
        assert_eq!(check(&six.text).len(), 1);
    }
    #[test]
    fn closed_xml_namespace_and_context_guards_suppress_guessed_schema_roles() {
        for text in [
            format!("<r xmlns:m='{MVC}'><m:view-controller/></r>"),
            format!("<beans xmlns:m='{MVC}'><m:view-controller></beans>"),
            format!("<beans xmlns:m='{MVC}'><m:view-controller path='/a' path='/b'/></beans>"),
            format!("<beans xmlns:m='{MVC}' xmlns:q='urn:foreign'><q:view-controller/></beans>"),
            format!("<beans xmlns:xml='{MVC}'><xml:view-controller/></beans>"),
            format!("<!DOCTYPE beans []><beans xmlns:m='{MVC}'><m:view-controller/></beans>"),
            format!("<beans xmlns:m='{MVC}'><m:view-controller path='&name;'/></beans>"),
            format!("<beans xmlns:m='{MVC}'><m:view-controller path='/x' broken/></beans>"),
        ] {
            assert!(check(&text).is_empty(), "{text}");
        }
        let snippet = format!("<beans xmlns:m='{MVC}'><m:view-controller/></beans>");
        assert_eq!(check(&snippet).len(), 1);
        let other =
            format!("<beans xmlns='urn:foreign' xmlns:m='{MVC}'><m:view-controller/></beans>");
        assert!(check(&other).is_empty());
    }
    #[test]
    fn completion_uses_current_unsaved_utf16_value_spans_without_host_access() {
        let _h = MockHost::install().with_capabilities(&[]);
        let d = doc("<!--😀-->\r\n<m:annotation-driven ignore-default-model-on-redirect='fa'/>");
        let start = d.text.find("='fa'").unwrap() + 2;
        let values = completion(&d, position(&d.text, start + 2));
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].label, "false");
        let r = values[0].replace.unwrap();
        assert_eq!(
            &d.text[offset(&d.text, r.start).unwrap()..offset(&d.text, r.end).unwrap()],
            "fa"
        );
        let old = versioned(
            "<m:annotation-driven ignore-default-model-on-redirect='fa'/>",
            "3.2",
        );
        let start = old.text.find("='fa'").unwrap() + 2;
        assert!(completion(&old, position(&old.text, start)).is_empty());
    }
    #[test]
    fn reference_roles_navigate_and_complete_only_unambiguous_declared_local_ids() {
        let _h = MockHost::install().with_capabilities(&[]);
        let d = doc(
            "<b:bean id='validator'/><b:bean id='converter'/><m:annotation-driven validator='validator'><m:message-converters><b:ref bean='converter'/></m:message-converters></m:annotation-driven><m:interceptors><b:ref bean='converter'/></m:interceptors>",
        );
        for needle in ["validator='validator'", "bean='converter'"] {
            let start = d.text.find(needle).unwrap() + needle.find('=').unwrap() + 2;
            let at = position(&d.text, start);
            let targets = definition(&d, at);
            assert_eq!(targets.len(), 1);
            assert_eq!(targets[0].path, d.path);
            let values = completion(&d, at);
            assert_eq!(values.len(), 2);
        }
        let duplicate = Document {
            text: d.text.replace(
                "<b:bean id='converter'/>",
                "<b:bean id='converter'/><b:bean id='converter'/>",
            ),
            ..d.clone()
        };
        let at = position(
            &duplicate.text,
            duplicate.text.find("bean='converter'").unwrap() + 6,
        );
        assert!(definition(&duplicate, at).is_empty());
        let imported = Document {
            text: d.text.replace(
                "<b:bean id='converter'/>",
                "<b:import resource='other.xml'/><b:bean id='converter'/>",
            ),
            ..d.clone()
        };
        let at = position(
            &imported.text,
            imported.text.find("validator='validator'").unwrap() + 11,
        );
        assert!(definition(&imported, at).is_empty());
        let snippet = Document {
            text: d
                .text
                .replace(&format!("xmlns:b='{BEANS}'"), "xmlns:b='urn:foreign'"),
            ..d
        };
        let at = position(
            &snippet.text,
            snippet.text.find("validator='validator'").unwrap() + 11,
        );
        assert!(definition(&snippet, at).is_empty());
    }
    #[test]
    fn reference_absence_is_never_reported_as_missing_runtime_bean() {
        let d = doc("<m:annotation-driven validator='external'/>");
        assert!(check(&d.text).is_empty());
    }
}
