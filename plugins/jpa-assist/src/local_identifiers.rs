//! Conflicting source identifier declarations on a closed Java entity.
//! No inherited, XML, effective access or external mapping inference.
use crate::{
    attribute_info,
    checks::Finding,
    model::{self, Kind},
};
use openidea_plugin::{Severity, providers::Document, text::offset};
pub(super) fn findings(doc: &Document) -> Vec<Finding> {
    if doc.language != "java" {
        return vec![];
    }
    let Some(class) = attribute_info::snapshot(doc) else {
        return vec![];
    };
    if class.kind != Kind::Entity || class.superclass.is_some() {
        return vec![];
    }
    let masked = model::mask(&doc.text);
    let annotations = model::annotations(&masked);
    if annotations
        .iter()
        .any(|a| matches!(a.name, "Access" | "Transient"))
    {
        return vec![];
    }
    let Some((at, _)) = model::declaration_keyword(&masked, 0, &["class"]) else {
        return vec![];
    };
    let Some(open) = masked[at..].find('{').map(|p| at + p) else {
        return vec![];
    };
    if masked[at + 5..open].trim() != class.name {
        return vec![];
    }
    let Some(end) = class.body_end else {
        return vec![];
    };
    let mut identifiers = Vec::new();
    for (from, to) in model::members(&masked, open + 1, end, false, false) {
        let local: Vec<_> = annotations
            .iter()
            .filter(|a| from <= a.start && a.start < to)
            .collect();
        if !local.iter().any(|a| matches!(a.name, "Id" | "EmbeddedId")) {
            continue;
        }
        let declaration = model::without_annotations(&masked, from, to);
        if declaration.contains(['(', '{', ',', '='])
            || !declaration.trim_end().ends_with(';')
            || model::has_word(&declaration, "static")
            || model::has_word(&declaration, "transient")
        {
            return vec![];
        }
        let attrs: Vec<_> = class
            .attributes
            .iter()
            .filter(|a| offset(&doc.text, a.pos).is_some_and(|p| from <= p && p < to))
            .collect();
        let [attribute] = attrs.as_slice() else {
            return vec![];
        };
        let selected: Vec<_> = local
            .iter()
            .filter(|a| matches!(a.name, "Id" | "EmbeddedId"))
            .collect();
        if selected.len() != 1 || selected.iter().any(|a| a.args.is_some()) {
            return vec![];
        }
        let Some(pos) = offset(&doc.text, attribute.pos) else {
            return vec![];
        };
        if doc.text.get(pos..pos + attribute.name.len()) != Some(attribute.name.as_str()) {
            return vec![];
        }
        identifiers.push((selected[0].name, pos, attribute.name.len()));
    }
    if !identifiers.iter().any(|(kind, _, _)| *kind == "EmbeddedId") {
        return vec![];
    }
    if identifiers
        .iter()
        .filter(|(kind, _, _)| *kind == "EmbeddedId")
        .count()
        < 2
    {
        return vec![];
    }
    identifiers.into_iter().filter(|(kind,_,_)| *kind == "EmbeddedId").map(|(_,at,len)| Finding::new(at,at+len,Severity::Warning,"jpa-duplicate-embedded-id","This source-local entity declares multiple EmbeddedId fields. An annotation-based mapping may declare only one EmbeddedId and no other Id or EmbeddedId; XML and inherited mappings are not resolved.")).collect()
}
#[cfg(test)]
mod tests {
    use crate::*;
    use openidea_plugin::{diagnostics::DiagnosticsProvider, mock::MockHost, text::offset};
    const SOURCE: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.EmbeddedId;\n@Entity class Person { @EmbeddedId Key key; /* 🦉 */ @Id long id; }";
    fn doc(text: &str) -> openidea_plugin::providers::Document {
        openidea_plugin::providers::Document {
            path: "/project/src/main/java/shop/Person.java".into(),
            language: "java".into(),
            text: text.into(),
            version: 1,
        }
    }
    fn found(plugin: &mut JpaAssist, source: &str) -> Vec<openidea_plugin::Diagnostic> {
        plugin
            .diagnostics(&doc(source))
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("jpa-duplicate-embedded-id"))
            .collect()
    }
    #[test]
    fn registered_identifier_conflict_has_exact_ranges_and_clears() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let source = SOURCE.replace("@Id long id;", "@EmbeddedId Key id;");
        let ds = found(&mut plugin, &source);
        assert_eq!(ds.len(), 2);
        let names: Vec<_> = ds
            .iter()
            .map(|d| {
                &source
                    [offset(&source, d.range.start).unwrap()..offset(&source, d.range.end).unwrap()]
            })
            .collect();
        assert_eq!(names, vec!["key", "id"]);
        assert_eq!(
            found(
                &mut plugin,
                &SOURCE.replace("@Id long id;", "@EmbeddedId Key other;")
            )
            .len(),
            2
        );
        for source in [
            SOURCE.replace("@Id long id;", "long id;"),
            SOURCE.replace("@EmbeddedId Key key;", "@Id long key;"),
            SOURCE.replace("class Person {", "class Person extends Base {"),
            SOURCE.replace("jakarta.persistence.EmbeddedId", "vendor.EmbeddedId"),
            SOURCE.replace("@Id long id;", "@Id long getId(){ return 0; }"),
            SOURCE.replace("@Id long id;", "@Id static long id;"),
            SOURCE.trim_end_matches('}').into(),
        ] {
            assert!(found(&mut plugin, &source).is_empty(), "{source}");
        }
        host.set_setting("jpaAssist.checkAttributeTypes", "false");
        assert!(found(&mut plugin, &source).is_empty());
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }
}
