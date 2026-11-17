//! Strict JSON or unambiguous YAML Compose source facts: no schema/runtime
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
// YAML source follows the same strict ownership model: aliases, tags, merge
// keys, duplicate decoded names, multiple documents and complex keys do not
// establish a literal service inventory.
#[derive(Default)]
struct YamlGuard {frames:Vec<Option<(bool,std::collections::BTreeSet<String>)>>,nodes:usize,docs:usize,bad:bool}
impl yaml_rust2::parser::EventReceiver for YamlGuard {
    fn on_event(&mut self,event:yaml_rust2::parser::Event){
        use yaml_rust2::parser::Event;
        self.nodes+=1;if self.nodes>8192||self.frames.len()>32{self.bad=true;return;}
        match event {
            Event::DocumentStart=>{self.docs+=1;if self.docs>1{self.bad=true;}},
            Event::Alias(_)=>self.bad=true,
            Event::Scalar(value,_,anchor,tag)=>{
                if anchor!=0||tag.is_some(){self.bad=true;}
                if let Some(Some((key,seen)))=self.frames.last_mut(){if *key&&!seen.insert(value){self.bad=true;}*key=!*key;}
            },
            Event::MappingStart(anchor,ref tag)|Event::SequenceStart(anchor,ref tag)=>{
                if anchor!=0||tag.is_some(){self.bad=true;}
                if let Some(Some((key,_)))=self.frames.last_mut(){if *key{self.bad=true;}else{*key=true;}}
                self.frames.push(if matches!(event,Event::MappingStart(..)){Some((true,Default::default()))}else{None});
            },
            Event::MappingEnd|Event::SequenceEnd=>{self.frames.pop();},_=>{},
        }
    }
}
fn yaml_json(value:&yaml_rust2::Yaml)->Option<Value>{
    use yaml_rust2::Yaml;
    Some(match value {
        Yaml::Null=>Value::Null,Yaml::Boolean(b)=>Value::Bool(*b),Yaml::Integer(n)=>Value::Number((*n).into()),Yaml::Real(n)=>serde_json::from_str(n).ok()?,Yaml::String(s)=>Value::String(s.clone()),
        Yaml::Array(v)=>Value::Array(v.iter().map(yaml_json).collect::<Option<_>>()?),
        Yaml::Hash(v)=>{let mut out=serde_json::Map::new();for(k,v)in v{out.insert(k.as_str()?.into(),yaml_json(v)?);}Value::Object(out)},_=>return None,
    })
}
pub(crate) fn parse_source(path:&str,source:&str)->Result<Value,Error>{
    if path.ends_with(".yaml")||path.ends_with(".yml") {
        let mut guard=YamlGuard::default();yaml_rust2::parser::Parser::new_from_str(source).load(&mut guard,true).map_err(|_|Error::Invalid("Unreadable Compose YAML.".into()))?;
        if guard.bad{return Err("Use one unambiguous Compose YAML document without aliases, tags or duplicate keys.".into());}
        let docs=yaml_rust2::YamlLoader::load_from_str(source).map_err(|_|Error::Invalid("Unreadable Compose YAML.".into()))?;
        if docs.len()!=1{return Err("Choose one Compose YAML document.".into());}
        yaml_json(&docs[0]).ok_or_else(||Error::Invalid("Compose YAML needs literal string mapping keys and finite JSON-compatible values.".into()))
    }else{
        let Unique(value)=serde_json::from_str(source).map_err(|_| Error::Invalid("Use strict JSON without duplicate keys, or choose a .yaml/.yml Compose file. Interpolation and multi-file merging are not resolved by this form.".into()))?;Ok(value)
    }
}

/// Findings for literal Compose references of a devcontainer.json at `doc_path`. Quiet unless a missing file or a missing service is provable.
pub(crate) fn reference_findings(doc_path: &str, text: &str) -> Vec<crate::checks::Finding> {
    let (files, service) = crate::checks::compose_references(text);
    let Some(project) = host::project_of(doc_path) else { return vec![] };
    let Ok(root) = project.root() else { return vec![] };
    let root = root.replace('\\', "/");
    let prefix = format!("{}/", root.trim_end_matches('/'));
    let doc_path = doc_path.replace('\\', "/");
    let Some(folder) = doc_path.rsplit_once('/').map(|(folder, _)| folder) else { return vec![] };
    let mut findings = vec![];
    let mut services: Option<Vec<String>> = Some(vec![]);
    let warn = |range: &std::ops::Range<usize>, message: String| crate::checks::Finding {
        start: range.start,
        end: range.end,
        code: crate::checks::COMPOSE_REFERENCE,
        message,
        severity: openidea_plugin::Severity::Warning,
    };
    let mut depends = vec![];
    for (file, range) in &files {
        let Some(absolute) = resolve(folder, file, &prefix) else {
            services = None;
            continue;
        };
        match project.read_file(&absolute) {
            Err(Error::NotFound(_) | Error::Invalid(_)) => {
                services = None;
                findings.push(warn(range, format!("Compose file `{file}` was not found relative to devcontainer.json.")));
            }
            Err(_) => services = None,
            Ok(source) if source.len() > 256 * 1024 => services = None,
            Ok(source) => match parse_source(&absolute, &source) {
                Ok(value) if value.get("include").is_none() => {
                    let facts = crate::compose_semantics::inspect(&value, files.len() == 1);
                    findings.extend(facts.messages.iter().map(|m| warn(range, format!("Compose file `{file}`: {m}"))));
                    let own_folder = absolute.rsplit_once('/').map_or("", |(f, _)| f);
                    for (what, path) in &facts.paths {
                        let Some(target) = resolve(own_folder, path, &prefix) else { continue };
                        if matches!(project.stat(&target), Err(Error::NotFound(_) | Error::Invalid(_))) {
                            findings.push(warn(range, format!("Compose file `{file}`: {what} `{path}` was not found relative to the Compose file.")));
                        }
                    }
                    depends.extend(facts.depends.iter().map(|(from, to)| (file.clone(), range.clone(), from.clone(), to.clone())));
                    match (value.get("services").and_then(Value::as_object), services.as_mut()) {
                        (Some(map), Some(all)) => all.extend(map.keys().cloned()),
                        _ => services = None,
                    }
                }
                _ => services = None,
            },
        }
    }
    if let Some(all) = &services {
        for (file, range, from, to) in &depends {
            if !all.contains(to) {
                findings.push(warn(range, format!("Compose file `{file}`: service `{from}` depends_on `{to}`, which is not declared in the referenced Compose file(s).")));
            }
        }
    }
    if let (Some((name, range)), Some(all)) = (service, services) {
        if !files.is_empty() && !all.contains(&name) {
            findings.push(warn(&range, format!("Service `{name}` is not declared in the referenced Compose file(s).")));
        }
    }
    findings
}

/// `rel` resolved against `folder` as a normalised absolute path under `prefix`; None for absolute, dynamic or escaping paths.
pub(crate) fn resolve(folder: &str, rel: &str, prefix: &str) -> Option<String> {
    if rel.starts_with('/') || rel.contains('$') {
        return None;
    }
    let mut parts: Vec<&str> = folder.split('/').collect();
    let rel = rel.replace('\\', "/");
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    let absolute = parts.join("/");
    absolute.starts_with(prefix).then_some(absolute)
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
                "Use a project Compose file path up to 512 bytes without control characters.".into(),
            );
        }
        let root = project.root()?;
        let path = crate::configs::relative(&root, input)?;
        crate::configs::ensure_owned(project, &path)?;
        let metadata = project.stat(&path)?;
        if metadata.kind != FileKind::File || metadata.size > MAX_BYTES as u64 {
            return Err(
                "Choose an existing Compose YAML/JSON file up to 8KiB for complete review.".into(),
            );
        }
        let source = project.read_file(&path)?;
        if source.len() > MAX_BYTES {
            return Err("Compose source snapshot exceeds 8KiB.".into());
        }
        let value=parse_source(&path,&source)?;
        let map = value
            .get("services")
            .and_then(Value::as_object)
            .ok_or("Compose source needs an explicit services object.")?;
        if map.is_empty() || map.len() > 64 {
            return Err("Choose a Compose YAML/JSON file with1–64 explicitly declared services.".into());
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
                return Err("Each service needs a literal name up to 128 ASCII letters/digits/._- and an object definition.".into());
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
        let items:Vec<_>=self.services.iter().enumerate().map(|(n,service)| PickItem::new(&format!("service-{n}"),service).description("Declared in the reviewed source; mount, dependencies and runtime are unverified.")).collect();
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
            return Err("Enter a literal absolute POSIX container path up to 512 bytes; variables and traversal are outside this form.".into());
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
            "Existing Compose source snapshot: {}\n--- complete current text ---\n{}\n--- end Compose source ---\nOnly literal service names were extracted; this is not Compose schema validation. Workspace path is supplied by you, not verified against any mount/container. Aliases, interpolation, includes, extends, overrides, profiles, dependencies, secrets, environment files, images, builds and readiness are not resolved or validated. Save source/editor changes before Start. No Compose source is written or service started.",
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
    #[test]
    fn reference_findings_report_missing_env_file_extends_file_and_depends_on_target() {
        let host = MockHost::install();
        host.add_file(".devcontainer/devcontainer.json", "");
        host.add_file(".devcontainer/ok.env", "A=1");
        host.add_file(".devcontainer/compose.yml", "services:\n  app:\n    image: x\n    depends_on: [db, cache]\n    env_file: [ok.env, missing.env]\n    extends: {file: base.yml, service: s}\n  cache:\n    image: y\n");
        let text = r#"{"dockerComposeFile":"compose.yml","service":"app"}"#;
        let messages: Vec<String> = reference_findings("/project/.devcontainer/devcontainer.json", text).into_iter().map(|f| f.message).collect();
        assert_eq!(messages.len(), 3, "{messages:?}");
        assert!(messages.iter().any(|m| m.contains("missing.env")));
        assert!(messages.iter().any(|m| m.contains("base.yml")));
        assert!(messages.iter().any(|m| m.contains("depends_on `db`")));
    }
    fn setup(text: &str) -> (MockHost, Project) {
        let host = MockHost::install();
        host.add_file("compose.json", text);
        let project = host.invocation().project().unwrap();
        (host, project)
    }
    #[test]
    fn literal_yaml_services_generate_the_same_owned_configuration_as_json() {
        let host=MockHost::install();
        let source="services:\n  api:\n    image: busybox\n    ports: [8080]\n  db:\n    image: postgres\n";
        host.add_file("containers/compose.yaml",source);let project=host.invocation().project().unwrap();
        let snapshot=Snapshot::capture(&project,"containers/compose.yaml").unwrap();
        host.answer_next_prompt(Some(&["service-0"]));host.answer_next_prompt(Some(&["/workspace/api"]));
        let generated=snapshot.configuration(".devcontainer/devcontainer.json","YAML project").unwrap();
        let value:Value=serde_json::from_str(&generated).unwrap();assert_eq!(value["service"],"api");assert_eq!(value["dockerComposeFile"],"../containers/compose.yaml");
        assert!(snapshot.preview().contains(source));assert!(snapshot.validate(&project).is_ok());
        host.add_file("containers/compose.yaml",&source.replace("busybox","ubuntu"));assert!(matches!(snapshot.validate(&project),Err(Error::Stale)));
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn adversarial_yaml_depth_cycles_and_byte_limits_refuse_before_conversion() {
        let deep=format!("services: {{api: {{environment: {{KEY: {}0{}}}}}}}\n","[".repeat(128),"]".repeat(128));
        let oversized=format!("services: {{api: {{image: \"{}\"}}}}\n","x".repeat(MAX_BYTES));
        for source in [deep,oversized,"services: &root {api: {depends_on: *root}}\n".into()] {
            let host=MockHost::install();host.add_file("compose.yaml",&source);let project=host.invocation().project().unwrap();
            assert!(Snapshot::capture(&project,"compose.yaml").is_err());assert!(host.process_runs().is_empty());
        }
    }

    #[test]
    fn ambiguous_yaml_never_establishes_a_service_inventory() {
        for source in ["services:\n  api: {}\n  'api': {}\n","services: &services {api: {}}\n","services:\n  api: !!map {}\n","services: {api: {}}\n---\nservices: {db: {}}\n","services: {123: {}}\n"] {
            let host=MockHost::install();host.add_file("compose.yaml",source);let project=host.invocation().project().unwrap();
            assert!(Snapshot::capture(&project,"compose.yaml").is_err(),"{source}");assert!(host.process_runs().is_empty());
        }
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
