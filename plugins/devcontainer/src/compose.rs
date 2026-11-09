//! Strict JSON Compose source facts only: no YAML inference, schema/runtime
//! validation, interpolation, overrides/includes resolution or mount detection.
use openidea_plugin::{Error, FileKind, FileStat, PickItem, Project, host};
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::Value;

// serde_json's default Value accepts duplicate keys. This visitor refuses them
// at every object level before extracting service names. Default depth limit
// remains enabled; input byte bounds apply before deserialization.
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Strict;
        impl<'de> Visitor<'de> for Strict {
            type Value = Unique;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON without duplicate object keys")
            }
            fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Unique, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Unique(Value::Number(n)))
                    .ok_or_else(|| E::custom("invalid JSON number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut values = Vec::new();
                while let Some(Unique(v)) = a.next_element()? {
                    values.push(v);
                }
                Ok(Unique(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = a.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom("duplicate JSON object key"));
                    }
                    let Unique(v) = a.next_value()?;
                    values.insert(key, v);
                }
                Ok(Unique(Value::Object(values)))
            }
        }
        d.deserialize_any(Strict)
    }
}
const MAX_BYTES: usize = 8 * 1024;
pub(crate) struct Snapshot {
    root: String,
    path: String,
    source: String,
    metadata: FileStat,
    services: Vec<String>,
}
impl Snapshot {
    pub(crate) fn capture(project: &Project, input: &str) -> Result<Self, Error> {
        if input.len() > 512 || input.chars().any(char::is_control) {
            return Err(
                "Use a project Compose JSON path up to512bytes without control characters.".into(),
            );
        }
        let root = project.root()?;
        let path = crate::configs::relative(&root, input)?;
        crate::configs::ensure_owned(project, &path)?;
        let metadata = project.stat(&path)?;
        if metadata.kind != FileKind::File || metadata.size > MAX_BYTES as u64 {
            return Err(
                "Choose an existing Compose JSON file up to8KiB for complete review.".into(),
            );
        }
        let source = project.read_file(&path)?;
        if source.len() > MAX_BYTES {
            return Err("Compose JSON snapshot exceeds8KiB.".into());
        }
        let Unique(value)=serde_json::from_str(&source).map_err(|_| Error::Invalid("Use strict JSON without duplicate keys. YAML, comments, interpolation and multi-file merging are not resolved by this form.".into()))?;
        let map = value
            .get("services")
            .and_then(Value::as_object)
            .ok_or("Compose JSON needs an explicit services object.")?;
        if map.is_empty() || map.len() > 64 {
            return Err("Choose a Compose JSON file with1–64 explicitly declared services.".into());
        }
        let mut services = Vec::new();
        for (key, value) in map {
            if key.is_empty()
                || key.len() > 128
                || !key
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
                || !value.is_object()
            {
                return Err("Each service needs a literal name up to128ASCII letters/digits/._- and an object definition.".into());
            }
            services.push(key.clone());
        }
        Ok(Self {
            root,
            path,
            source,
            metadata,
            services,
        })
    }
    pub(crate) fn validate(&self, project: &Project) -> Result<(), Error> {
        crate::configs::ensure_owned(project, &self.path)?;
        if project.root()? != self.root
            || project.stat(&self.path)? != self.metadata
            || project.read_file(&self.path)? != self.source
        {
            return Err(Error::Stale);
        }
        Ok(())
    }
    pub(crate) fn configuration(&self, target: &str, name: &str) -> Result<String, Error> {
        let items:Vec<_>=self.services.iter().enumerate().map(|(n,service)| PickItem::new(&format!("service-{n}"),service).description("Declared in the reviewed JSON; mount, dependencies and runtime are unverified.")).collect();
        let key = host::pick("Compose service for this Dev Container", &items)?
            .ok_or(Error::Cancelled)?;
        let n = key
            .strip_prefix("service-")
            .and_then(|v| v.parse::<usize>().ok())
            .ok_or(Error::Stale)?;
        let service = self.services.get(n).ok_or(Error::Stale)?;
        let workspace = host::input(
            "Explicit container workspace path (not mount-verified)",
            None,
        )?
        .ok_or(Error::Cancelled)?;
        if workspace.len() > 512
            || !workspace.starts_with('/')
            || workspace.starts_with("//")
            || workspace
                .chars()
                .any(|c| c.is_control() || matches!(c, '$' | '\\'))
            || workspace.split('/').any(|v| matches!(v, "." | ".."))
        {
            return Err("Enter a literal absolute POSIX container path up to512bytes; variables and traversal are outside this form.".into());
        }
        let mut value: Value = serde_json::from_str(include_str!("../templates/compose.json"))
            .map_err(|_| "Bundled Compose template is invalid.")?;
        // Reuse the existing display-name admission, without keeping its image.
        let name_value: Value = serde_json::from_str(&crate::create::source("base", name)?)
            .map_err(|_| "Cannot render the display name.")?;
        let folder: Vec<_> = target.split('/').collect();
        let folder = &folder[..folder.len().saturating_sub(1)];
        let file: Vec<_> = self.path.split('/').collect();
        let common = folder
            .iter()
            .zip(file.iter())
            .take_while(|(a, b)| a == b)
            .count();
        let mut relative = vec![".."; folder.len() - common];
        relative.extend_from_slice(&file[common..]);
        value["name"] = name_value["name"].clone();
        value["dockerComposeFile"] = relative.join("/").into();
        value["service"] = service.clone().into();
        value["workspaceFolder"] = workspace.into();
        Ok(format!(
            "{}\n",
            serde_json::to_string_pretty(&value).map_err(|_| "Cannot render the configuration.")?
        ))
    }
    pub(crate) fn preview(&self) -> String {
        format!(
            "Existing Compose JSON snapshot: {}\n--- complete current text ---\n{}\n--- end Compose JSON ---\nOnly literal service names were extracted; this is not Compose schema validation. Workspace path is supplied by you, not verified against any mount/container. YAML, interpolation, includes, extends, overrides, profiles, dependencies, secrets, environment files, images, builds and readiness are not resolved or validated. Save source/editor changes before Start. No Compose source is written or service started.",
            serde_json::to_string(&format!(
                "{}/{}",
                self.root.trim_end_matches('/'),
                self.path
            ))
            .unwrap_or_default(),
            self.source
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn setup(text: &str) -> (MockHost, Project) {
        let host = MockHost::install();
        host.add_file("compose.json", text);
        let project = host.invocation().project().unwrap();
        (host, project)
    }
    #[test]
    fn ambiguous_duplicate_keys_and_non_json_never_supply_service_facts() {
        for source in [
            r#"{"services":{"api":{},"api":{}}}"#,
            r#"{"services":{"api":{}},"services":{"db":{}}}"#,
            r#"{"services":{"api":{"image":"a","image":"b"}}}"#,
            r#"{"services":{"api":{},"\u0061pi":{}}}"#,
            "services:\n  api:\n    image: busybox\n",
            r#"{"services":{"api":{}}} trailing"#,
        ] {
            let (_host, project) = setup(source);
            assert!(
                Snapshot::capture(&project, "compose.json").is_err(),
                "{source}"
            );
        }
        let (_host, project) = setup(
            r#"{"services":{"api":{"ports":[80,"8080:80"],"init":true,"mem_swappiness":1.5,"labels":{"optional":null}}}}"#,
        );
        assert_eq!(
            Snapshot::capture(&project, "compose.json")
                .unwrap()
                .services,
            ["api"]
        );
    }
    #[test]
    fn source_admission_requires_bounded_service_objects_and_literal_schema_names() {
        for source in [
            r#"{"services":{}}"#,
            r#"{"services":[]}"#,
            r#"{"services":{"${SERVICE}":{}}}"#,
            r#"{"services":{"api":null}}"#,
            r#"{"services":{"bad name":{}}}"#,
            "[]",
        ] {
            let (_host, p) = setup(source);
            assert!(Snapshot::capture(&p, "compose.json").is_err());
        }
        let source=serde_json::json!({"services":(0..65).map(|n|(format!("s{n}"),serde_json::json!({}))).collect::<serde_json::Map<_,_>>()}).to_string();
        let (_host, p) = setup(&source);
        assert!(Snapshot::capture(&p, "compose.json").is_err());
        let (host, p) = setup(&"x".repeat(MAX_BYTES + 1));
        assert!(Snapshot::capture(&p, "compose.json").is_err());
        assert!(
            Snapshot::capture(&p, "../compose.json").is_err()
                && Snapshot::capture(&p, "missing.json").is_err()
        );
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn source_text_open_editor_and_current_owner_changes_invalidate_review() {
        let text = r#"{"services":{"api":{}}}"#;
        let (host, p) = setup(text);
        let snap = Snapshot::capture(&p, "compose.json").unwrap();
        snap.validate(&p).unwrap();
        host.add_file("compose.json", r#"{"services":{"db":{}}}"#);
        assert!(snap.validate(&p).is_err());
        host.add_file("compose.json", text);
        host.open_editor(
            "/project/compose.json",
            "json",
            r#"{"services":{"editor":{}}}"#,
            &[],
        );
        assert!(snap.validate(&p).is_err());
        host.add_project("/project/nested");
        host.add_file("nested/compose.json", text);
        assert!(Snapshot::capture(&p, "nested/compose.json").is_err());
        host.set_project_root(Some("/different"));
        assert!(
            snap.validate(&host.invocation().project().unwrap())
                .is_err()
        );
    }
    #[test]
    fn service_selection_and_workspace_are_explicit_not_inferred_from_service_metadata() {
        for workspace in [
            "relative",
            "/workspace/../other",
            "/workspace/${NAME}",
            "//workspace",
            "/work\nspace",
            "/work\\space",
        ] {
            let (host, p) = setup(r#"{"services":{"api":{"volumes":[".:/do-not-infer"]}}}"#);
            let snap = Snapshot::capture(&p, "compose.json").unwrap();
            host.answer_next_prompt(Some(&["service-0"]));
            host.answer_next_prompt(Some(&[workspace]));
            assert!(
                snap.configuration(".devcontainer/devcontainer.json", "Project")
                    .is_err()
            );
            assert!(host.process_runs().is_empty());
        }
        let (host, p) = setup(r#"{"services":{"api":{}}}"#);
        let snap = Snapshot::capture(&p, "compose.json").unwrap();
        host.answer_next_prompt(Some(&["service-999"]));
        assert!(
            snap.configuration(".devcontainer/devcontainer.json", "Project")
                .is_err()
        );
        assert!(
            snap.preview().contains("not verified against any mount")
                && host.terminals().is_empty()
        );
    }
}
