//! Inspection with a quick fix that assigns a Hibernate type to an attribute. The JPA Buddy page lists "Hibernate Types:
//! Assign Hibernate Custom Types to attributes via a code inspection with a quick fix (Alt+Enter)". Here the candidates are
//! plain attributes of a type JPA has no basic mapping for, `java.util.Map` and Jackson `JsonNode`, and the fix is the
//! JSON mapping the Hibernate 6 user guide shows (`@JdbcTypeCode(SqlTypes.JSON)`), produced by the same code as the
//! reviewed "Add JSON column mapping" action. The Jakarta namespace is the only evidence of Hibernate 6 available to an
//! editor provider, which never reads build files; the message names the requirement.
use super::*;
use crate::{checks::Finding, model::Edit};
use openidea_plugin::Severity;

const TITLE: &str = "Assign Hibernate JSON type (@JdbcTypeCode)";

fn candidate(src: &Source, ty: &str) -> Option<&'static str> {
    let imported = |full: &str| src.class.imports.iter().any(|i| i == full);
    let simple = ty.split('<').next()?.trim();
    match simple {
        "Map" if imported("java.util.Map") || src.class.imports.iter().any(|i| i == "java.util.*") => Some("java.util.Map"),
        "java.util.Map" => Some("java.util.Map"),
        "JsonNode" if imported("com.fasterxml.jackson.databind.JsonNode") => Some("JsonNode"),
        "com.fasterxml.jackson.databind.JsonNode" => Some("JsonNode"),
        _ => None,
    }
}

pub(crate) fn findings(doc: &Document) -> Vec<Finding> {
    if !crate::setting("checkHibernateTypes") || doc.language != "java" {
        return vec![];
    }
    let Ok(src) = source(doc) else {
        return vec![];
    };
    if src.ns != "jakarta.persistence" {
        return vec![];
    }
    let mut found = Vec::new();
    for field in &src.fields {
        let Some(kind) = candidate(&src, &field.ty) else {
            continue;
        };
        let Ok(edits) = render_json(doc, &field.name) else {
            continue;
        };
        let Some(edits) = edits
            .iter()
            .map(|edit| Some(Edit { start: offset(&doc.text, edit.range.start)?, end: offset(&doc.text, edit.range.end)?, text: edit.text.clone() }))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let (from, to) = field.span;
        let member = &src.masked[from..to];
        let head = member.find('=').unwrap_or(member.len());
        let Some(at) = words(&member[..head], &field.name).last() else {
            continue;
        };
        let start = from + at;
        let mut finding = Finding::new(
            start,
            start + field.name.len(),
            Severity::Warning,
            "jpa-hibernate-type",
            format!("`{}` is a {kind} attribute without a Hibernate type: JPA defines no basic mapping for it. Assign a type such as JSON (@JdbcTypeCode(SqlTypes.JSON), Hibernate 6.0 or later).", field.name),
        );
        finding.fix = Some((TITLE, edits));
        found.push(finding);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ENTITY: &str = "package shop;\n\nimport java.util.Map;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n\n@Entity\npublic class Invoice {\n    @Id\n    private Long id;\n\n    private Map<String, Object> attributes;\n\n    private String title;\n}\n";

    fn doc(text: &str) -> Document {
        Document { path: "/project/src/main/java/shop/Invoice.java".into(), language: "java".into(), text: text.into(), version: 1 }
    }

    #[test]
    fn map_attribute_is_flagged_on_its_name_with_the_json_fix() {
        let _host = MockHost::install();
        let found = findings(&doc(ENTITY));
        let [finding] = found.as_slice() else { panic!("one finding: {}", found.len()) };
        assert_eq!(&ENTITY[finding.start..finding.end], "attributes");
        assert_eq!(finding.code, "jpa-hibernate-type");
        let (title, edits) = finding.fix.as_ref().unwrap();
        assert_eq!(*title, TITLE);
        let mut text = ENTITY.to_owned();
        let mut ordered: Vec<_> = edits.iter().collect();
        ordered.sort_by_key(|edit| std::cmp::Reverse(edit.start));
        for edit in ordered {
            text.replace_range(edit.start..edit.end, &edit.text);
        }
        assert!(text.contains("@JdbcTypeCode(SqlTypes.JSON)\n    private Map<String, Object> attributes;"), "{text}");
        assert!(text.contains("import org.hibernate.annotations.JdbcTypeCode;") && text.contains("import org.hibernate.type.SqlTypes;"), "{text}");
    }

    #[test]
    fn json_node_is_flagged_and_assigned_attributes_and_other_contexts_stay_quiet() {
        let _host = MockHost::install();
        let node = ENTITY.replace("import java.util.Map;", "import com.fasterxml.jackson.databind.JsonNode;").replace("Map<String, Object>", "JsonNode");
        assert_eq!(findings(&doc(&node)).len(), 1);
        let assigned = ENTITY.replace("    private Map", "    @JdbcTypeCode(SqlTypes.JSON)\n    private Map");
        assert!(findings(&doc(&assigned)).is_empty(), "already assigned");
        assert!(findings(&doc(&ENTITY.replace("jakarta.persistence", "javax.persistence"))).is_empty(), "no Hibernate 6 evidence");
        assert!(findings(&doc(&ENTITY.replace("import java.util.Map;\n", "import other.Map;\n"))).is_empty(), "foreign Map");
        assert!(findings(&doc(&ENTITY.replace("    private Map", "    @Lob\n    private Map"))).is_empty(), "annotated attribute");
        let off = MockHost::install();
        off.set_setting("jpaAssist.checkHibernateTypes", "false");
        assert!(findings(&doc(ENTITY)).is_empty(), "toggle off");
    }

    #[test]
    fn live_diagnostic_and_quick_fix_are_offered_for_the_attribute() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let document = doc(ENTITY);
        let diagnostics = plugin.diagnostics(&document);
        let d = diagnostics.iter().find(|d| d.code.as_deref() == Some("jpa-hibernate-type")).expect("diagnostic");
        let actions = plugin.code_actions(&document, d.range);
        assert!(actions.iter().any(|a| a.title == TITLE && a.kind.as_deref() == Some("quickfix")), "{:?}", actions.iter().map(|a| &a.title).collect::<Vec<_>>());
    }
}
