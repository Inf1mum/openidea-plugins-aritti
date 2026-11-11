//! Source-owned root section additions. Existing keys are never replaced.
use crate::*;

pub(crate) fn actions(doc: &Document, selection: Range) -> Vec<CodeAction> {
    let Some(proof) = pointer_status::Proof::parse(&doc.text) else {
        return Vec::new();
    };
    let Some(at) = offset(&doc.text, selection.start) else {
        return Vec::new();
    };
    if !matches!(
        proof.copy_pointer_at(at).as_deref(),
        Some("#/openapi" | "#/swagger")
    ) {
        return Vec::new();
    }
    let mut actions = Vec::new();
    for (name, json_value, yaml_value) in [
        (
            "info",
            r#"{"title":"API","version":"1.0.0"}"#,
            "info:\n  title: API\n  version: 1.0.0\n",
        ),
        ("paths", "{}", "paths: {}\n"),
        ("components", "{}", "components: {}\n"),
    ] {
        if name == "components" && spec::parse(&doc.text).is_some_and(|spec| spec.version == "2.0")
        {
            continue;
        }
        if proof.status(&format!("/{name}")) != pointer_status::Status::Missing {
            continue;
        }
        let (at, text) = if spec::is_json(&doc.text) {
            let Ok(serde_json::Value::Object(root)) = serde_json::from_str(&doc.text) else {
                continue;
            };
            let Some(at) = doc.text.rfind('}') else {
                continue;
            };
            (
                at,
                format!(
                    "{}\n  \"{name}\": {json_value}\n",
                    if root.is_empty() { "" } else { "," }
                ),
            )
        } else {
            (
                doc.text.len(),
                format!(
                    "{}{yaml_value}",
                    if doc.text.ends_with('\n') || doc.text.is_empty() {
                        ""
                    } else {
                        "\n"
                    }
                ),
            )
        };
        let range = spring_text::range(&doc.text, at, at);
        actions.push(CodeAction {
            title: format!("Add OpenAPI {name} section"),
            kind: Some("quickfix".into()),
            preferred: false,
            edits: vec![(doc.path.clone(), vec![TextEdit { range, text }])],
            data: None,
        });
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document(text: &str, language: &str) -> Document {
        Document {
            path: format!("/project/openapi.{language}"),
            language: language.into(),
            text: text.into(),
            version: 1,
        }
    }
    #[test]
    fn adds_only_missing_root_sections_in_json_and_yaml_without_replacing_existing_text() {
        for (text, language) in [
            (r#"{"openapi":"3.0.3","paths":{}}"#, "json"),
            ("openapi: 3.0.3\npaths: {}\n", "yaml"),
        ] {
            let doc = document(text, language);
            let caret = position(text, text.find("openapi").unwrap());
            let actions = actions(&doc, Range::new(caret, caret));
            assert_eq!(actions.len(), 2);
            assert!(!actions.iter().any(|a| a.title.contains("paths")));
            for action in actions {
                let edit = &action.edits[0].1[0];
                let at = offset(text, edit.range.start).unwrap();
                let updated = format!("{}{}{}", &text[..at], edit.text, &text[at..]);
                let proof = pointer_status::Proof::parse(&updated).unwrap();
                assert_eq!(proof.status("/paths"), pointer_status::Status::Present);
                assert!(
                    proof.status("/components") == pointer_status::Status::Present
                        || proof.status("/info") == pointer_status::Status::Present
                );
            }
        }
    }
    #[test]
    fn duplicate_or_non_version_caret_and_swagger_components_are_not_offered() {
        let text = "swagger: '2.0'\npaths: {}\n";
        let doc = document(text, "yaml");
        let caret = position(text, 0);
        let found = actions(&doc, Range::new(caret, caret));
        assert!(found.iter().all(|a| !a.title.contains("components")));
        let text = r#"{"openapi":"3.0.3","info":{},"info":{}}"#;
        let doc = document(text, "json");
        let caret = position(text, text.find("openapi").unwrap());
        assert!(actions(&doc, Range::new(caret, caret)).is_empty());
    }
}
