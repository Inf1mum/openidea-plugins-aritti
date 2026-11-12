//! Duplicate optimistic-lock versions on a closed source-local field-access
//! entity. No inherited, property-access, XML or external mapping inference.
//! Rule: https://jakarta.ee/specifications/persistence/3.2/apidocs/jakarta.persistence/jakarta/persistence/Version.html
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
        .any(|a| matches!(a.name, "Access" | "IdClass" | "EmbeddedId" | "Transient"))
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
    let mut versions = Vec::new();
    let mut field_id = false;
    for (from, to) in model::members(&masked, open + 1, end, false, false) {
        let local: Vec<_> = annotations
            .iter()
            .filter(|a| from <= a.start && a.start < to)
            .collect();
        if !local.iter().any(|a| matches!(a.name, "Id" | "Version")) {
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
        let [_] = attrs.as_slice() else {
            return vec![];
        };
        if local.iter().any(|a| a.name == "Id") {
            field_id = true;
        }
        let selected: Vec<_> = local.iter().filter(|a| a.name == "Version").collect();
        if selected.len() > 1 || selected.iter().any(|a| a.args.is_some()) {
            return vec![];
        }
        if let Some(version) = selected.first() {
            versions.push((version.start, version.end()));
        }
    }
    if !field_id {
        return vec![];
    }
    if versions.len() < 2 {
        return vec![];
    }
    versions.into_iter().map(|(from,to)| Finding::new(from,to,Severity::Error,"jpa-duplicate-version","This source-local field-access entity already declares a version attribute. Use one optimistic-lock version per entity hierarchy; inherited and external mappings are not resolved.")).collect()
}
#[cfg(test)]
mod tests {
    use crate::*;
    use openidea_plugin::{diagnostics::DiagnosticsProvider, mock::MockHost, text::offset};
    const SOURCE: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.Version;\n@Entity class Person { @Id long id; @Version long revision; /* 🦉 */ @Version long other; }";
    fn doc(text: &str) -> openidea_plugin::providers::Document {
        openidea_plugin::providers::Document {
            path: "/project/src/main/java/shop/Person.java".into(),
            language: "java".into(),
            text: text.into(),
            version: 1,
        }
    }
    #[test]
    fn registered_live_duplicate_version_has_exact_range_and_clears() {
        let _h = MockHost::install();
        let mut plugin = JpaAssist::default();
        let d = doc(SOURCE);
        let found: Vec<_> = plugin
            .diagnostics(&d)
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("jpa-duplicate-version"))
            .collect();
        assert_eq!(found.len(), 2);
        let first_start = offset(SOURCE, found[0].range.start).unwrap();
        assert_eq!(&SOURCE[first_start..first_start + 22], "@Version long revision");
        assert_eq!(
            &SOURCE[offset(SOURCE, found[1].range.start).unwrap()
                ..offset(SOURCE, found[1].range.end).unwrap()],
            "@Version"
        );
        for source in [
            SOURCE.replace("@Version long other;", "long other;"),
            SOURCE.replace("class Person {", "class Person extends Base {"),
            SOURCE.replace("jakarta.persistence.Version", "app.Version"),
            SOURCE.replace("@Id long id;", "long id;"),
            SOURCE.replace(
                "@Version long other;",
                "@Version long getOther(){return 0;}",
            ),
            SOURCE.replace("@Version long other;", "@Version long other = 0;"),
        ] {
            assert!(
                plugin
                    .diagnostics(&doc(&source))
                    .iter()
                    .all(|d| d.code.as_deref() != Some("jpa-duplicate-version")),
                "{source}"
            );
        }
    }
}
