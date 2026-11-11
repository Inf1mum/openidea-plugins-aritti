//! Reviewed, undoable local CRD schema materialization for YAML language servers.
use openidea_plugin::file_actions::{FileAction, FileActions, FileOperation};
use openidea_plugin::providers::Document;
use openidea_plugin::{Error, Position, Range, TextEdit, host};
use serde_json::Value;
use sha2::{Digest, Sha256};
fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn json(value: &yaml_rust2::Yaml) -> Option<Value> {
    use yaml_rust2::Yaml;
    Some(match value {
        Yaml::Null => Value::Null,
        Yaml::Boolean(b) => Value::Bool(*b),
        Yaml::Integer(n) => Value::Number((*n).into()),
        Yaml::Real(n) => serde_json::from_str(n).ok()?,
        Yaml::String(s) => Value::String(s.clone()),
        Yaml::Array(items) => Value::Array(items.iter().map(json).collect::<Option<_>>()?),
        Yaml::Hash(fields) => {
            let mut map = serde_json::Map::new();
            for (key, value) in fields {
                map.insert(key.as_str()?.into(), json(value)?);
            }
            if map.get("nullable") == Some(&Value::Bool(true))
                && let Some(Value::String(ty)) = map.get("type").cloned()
            {
                map.insert("type".into(), serde_json::json!([ty, "null"]));
            }
            if map.get("x-kubernetes-int-or-string") == Some(&Value::Bool(true)) {
                map.remove("type");
                map.insert(
                    "anyOf".into(),
                    serde_json::json!([{"type":"integer"},{"type":"string"}]),
                );
            }
            Value::Object(map)
        }
        _ => return None,
    })
}
fn source(kube: &crate::KubeExplorer, doc: &Document) -> Option<String> {
    if doc.language != "yaml" || doc.text.contains("yaml-language-server: $schema=") {
        return None;
    }
    let mut value = json(&kube.crd_schema.schema(doc)?)?;
    value.as_object_mut()?.insert(
        "$schema".into(),
        "http://json-schema.org/draft-04/schema#".into(),
    );
    Some(format!("{}\n", serde_json::to_string_pretty(&value).ok()?))
}
fn vacant(project: &openidea_plugin::Project, path: &str) -> Result<(), Error> {
    if host::project_of(path).and_then(|p| p.root().ok()) != project.root().ok() {
        return Err(Error::Stale);
    }
    if project.exists(path)? {
        return Err("Schema destination already exists; nothing was overwritten.".into());
    }
    match project.read_file(path) {
        Ok(_) => Err("Schema destination has an open buffer.".into()),
        Err(Error::NotFound(_)) => Ok(()),
        Err(e) => Err(e),
    }
}
impl FileActions for crate::KubeExplorer {
    fn file_actions(&mut self, doc: &Document, _selection: Range) -> Vec<FileAction> {
        let mut actions = self.label_refs.file_actions(doc, _selection);
        let Some(text) = source(self, doc) else {
            return actions;
        };
        let Some(project) = host::project_of(&doc.path) else {
            return actions;
        };
        let Ok(root) = project.root() else {
            return actions;
        };
        actions.push(FileAction {
            title: "Use local CRD schema in YAML language server…".into(),
            kind: Some("source".into()),
            data: Some(
                serde_json::json!([
                    root,
                    doc.path,
                    doc.version,
                    digest(&doc.text),
                    digest(&text)
                ])
                .to_string(),
            ),
            ..Default::default()
        });
        actions
    }
    fn resolve_file_action(
        &mut self,
        doc: &Document,
        action: FileAction,
    ) -> Result<FileAction, Error> {
        let data: Value = serde_json::from_str(action.data.as_deref().ok_or(Error::Stale)?)
            .map_err(|_| Error::Stale)?;
        if data[0].as_str() == Some("kube-source-rename") {
            return self.label_refs.resolve_file_action(doc, action);
        }
        if data[1].as_str() != Some(&doc.path)
            || data[2].as_u64() != Some(doc.version)
            || data[3].as_str() != Some(&digest(&doc.text))
        {
            return Err(Error::Stale);
        }
        let project = host::project_of(&doc.path).ok_or(Error::Stale)?;
        let root = project.root()?;
        if data[0].as_str() != Some(&root) {
            return Err(Error::Stale);
        }
        let schema = source(self, doc).ok_or(Error::Stale)?;
        if data[4].as_str() != Some(&digest(&schema)) {
            return Err(Error::Stale);
        }
        let filename = std::path::Path::new(&doc.path)
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or(Error::Stale)?;
        let filename = format!("{filename}.schema.json");
        let directory = std::path::Path::new(&doc.path)
            .parent()
            .and_then(|s| s.to_str())
            .ok_or(Error::Stale)?;
        let target = format!("{directory}/{filename}");
        vacant(&project, &target)?;
        let escaped = filename
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_') {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect::<String>();
        let line = format!(
            "# yaml-language-server: $schema=./{escaped}{}",
            if doc.text.contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            }
        );
        if !ui_kit::prompts::confirm_exact(
            "Create local CRD schema and annotate this resource?",
            &format!(
                "Schema: {target}\n\n{schema}\nResource: {}\nInsert: {line}\nAn installed YAML language server can then apply this schema. No cluster or network request is performed by this action.",
                doc.path
            ),
        )? {
            return Err(Error::Cancelled);
        }
        vacant(&project, &target)?;
        if project.read_file(&doc.path)? != doc.text
            || source(self, doc).as_deref() != Some(schema.as_str())
        {
            return Err(Error::Stale);
        }
        Ok(FileAction {
            title: "Use local CRD schema in YAML language server".into(),
            kind: Some("source".into()),
            operations: vec![FileOperation::Create {
                path: target,
                text: schema,
                overwrite: false,
            }],
            edits: vec![(
                doc.path.clone(),
                vec![TextEdit {
                    range: Range::new(Position::new(0, 0), Position::new(0, 0)),
                    text: line,
                }],
            )],
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const CRD: &str = "apiVersion: apiextensions.k8s.io/v1\nkind: CustomResourceDefinition\nspec:\n  group: example.test\n  names: {kind: Widget}\n  versions:\n  - name: v1\n    served: true\n    schema:\n      openAPIV3Schema:\n        type: object\n        properties:\n          spec:\n            type: object\n            properties:\n              color: {type: string, nullable: true, enum: [red, blue]}\n";
    fn setup() -> (MockHost, crate::KubeExplorer, Document, FileAction) {
        let h = MockHost::install().with_capabilities(&["project.read"]);
        h.set_project_root(Some("/p"));
        let doc = Document {
            path: "/p/widget one.yaml".into(),
            language: "yaml".into(),
            version: 3,
            text: "apiVersion: example.test/v1\r\nkind: Widget\r\n".into(),
        };
        h.add_file("widget one.yaml", &doc.text);
        h.add_file("crd.yaml", CRD);
        let mut kube = crate::KubeExplorer::default();
        kube.crd_schema.warm("/p/crd.yaml", CRD);
        let action = kube.file_actions(&doc, Range::default()).remove(0);
        (h, kube, doc, action)
    }
    #[test]
    fn reviewed_schema_export_is_atomic_nonoverwriting_and_percent_encoded() {
        let (h, mut kube, doc, action) = setup();
        h.answer_next_prompt(Some(&["yes"]));
        let ready = kube.resolve_file_action(&doc, action).unwrap();
        assert_eq!(ready.operations.len(), 1);
        let FileOperation::Create {
            path,
            text,
            overwrite,
        } = &ready.operations[0]
        else {
            panic!("create expected")
        };
        assert_eq!(path, "/p/widget one.yaml.schema.json");
        assert!(!overwrite);
        let schema: Value = serde_json::from_str(text).unwrap();
        assert_eq!(
            schema["properties"]["spec"]["properties"]["color"]["type"],
            serde_json::json!(["string", "null"])
        );
        assert_eq!(
            ready.edits[0].1[0].text,
            "# yaml-language-server: $schema=./widget%20one.yaml.schema.json\r\n"
        );
        assert!(!host::project_of(&doc.path).unwrap().exists(path).unwrap());
    }
    #[test]
    fn cancellation_collisions_and_stale_schema_never_return_operations() {
        let (h, mut kube, doc, action) = setup();
        h.answer_next_prompt(None);
        assert!(matches!(
            kube.resolve_file_action(&doc, action),
            Err(Error::Cancelled)
        ));
        let (h, mut kube, doc, action) = setup();
        h.add_file("widget one.yaml.schema.json", "keep");
        assert!(kube.resolve_file_action(&doc, action).is_err());
        let (h, mut kube, doc, action) = setup();
        h.add_file("crd.yaml", &CRD.replace("red, blue", "red, green"));
        assert!(matches!(
            kube.resolve_file_action(&doc, action),
            Err(Error::Stale)
        ));
    }
}
