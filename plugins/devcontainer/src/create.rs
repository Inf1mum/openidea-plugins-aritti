//! Native intention for creating a configuration from our bundled
//! file templates. Resolution confirms the concrete target/content, then returns
//! a non-overwriting workspace operation; it never writes or starts a CLI.
use openidea_plugin::file_actions::{FileAction, FileActions, FileOperation};
use openidea_plugin::providers::Document;
use openidea_plugin::{Error, FileKind, PickItem, Project, Range, host};

use crate::Main;

const TEMPLATES: &[(&str, &str, &str)] = &[
    ("base", "Base (Ubuntu)", include_str!("../templates/devcontainer.json")),
    ("node", "Node", include_str!("../templates/node.json")),
    ("python", "Python", include_str!("../templates/python.json")),
    ("java", "Java", include_str!("../templates/java.json")),
    ("php", "PHP", include_str!("../templates/php.json")),
    ("rust", "Rust", include_str!("../templates/rust.json")),
];

fn destination(root: &str, input: &str) -> Result<String, Error> {
    if input.len() > 512 || input.chars().any(char::is_control) {
        return Err("Use a configuration path up to 512 characters without control characters.".into());
    }
    let path = crate::configs::relative(root, input)?;
    let parts: Vec<_> = path.split('/').collect();
    let allowed = matches!(parts.as_slice(), [".devcontainer.json"] | [".devcontainer", "devcontainer.json"])
        || matches!(parts.as_slice(), [".devcontainer", name, "devcontainer.json"]
            if !name.is_empty() && name.len() <= 64 && name.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_')));
    if !allowed {
        return Err(
            "Use .devcontainer.json, .devcontainer/devcontainer.json or .devcontainer/<name>/devcontainer.json (letters, digits, - and _)."
                .into(),
        );
    }
    Ok(path)
}

fn vacant(project: &Project, path: &str) -> Result<(), Error> {
    crate::configs::ensure_owned(project, path)?;
    if project.exists(path)? {
        return Err("The configuration already exists; choose a new destination. Nothing was overwritten.".into());
    }
    // read_file prefers open editor text, also covering a new unsaved buffer.
    match project.read_file(path) {
        Ok(_) => return Err("The destination has an open buffer; save it or choose another destination.".into()),
        Err(Error::NotFound(_)) => {}
        Err(error) => return Err(error),
    }
    let parts: Vec<_> = path.split('/').collect();
    for n in 1..parts.len() {
        let parent = parts[..n].join("/");
        if project.exists(&parent)? && project.stat(&parent)?.kind != FileKind::Directory {
            return Err(format!("The parent path {parent} is not a directory.").into());
        }
    }
    Ok(())
}

pub(crate) fn source(key: &str, name: &str) -> Result<String, Error> {
    if name.trim().is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
        return Err("Enter a nonempty display name up to 128 characters without control characters.".into());
    }
    let (_, _, text) = TEMPLATES.iter().find(|(id, ..)| *id == key).ok_or("Choose one of the bundled image templates.")?;
    let mut value: serde_json::Value = serde_json::from_str(text).map_err(|_| "The bundled template is invalid.")?;
    value["name"] = name.trim().into();
    Ok(format!("{}\n", serde_json::to_string_pretty(&value).map_err(|_| "Cannot render the configuration.")?))
}

fn with_cached_feature(text: String) -> Result<String, Error> {
    let mut value: serde_json::Value = serde_json::from_str(&text).map_err(|_| "The bundled template is invalid.")?;
    let candidates: Vec<_> = crate::features::creation_candidates().into_iter()
        .filter(|(reference, _)| value.get("features").and_then(|v| v.get(reference)).is_none()).collect();
    if candidates.is_empty() { return Ok(text); }
    let mut items = vec![PickItem::new("none", "No additional feature")];
    items.extend(candidates.iter().enumerate().map(|(index, (reference, name))| {
        PickItem::new(&format!("feature-{index}"), reference)
            .description(&format!("Cached registry name: {name}. Defaults available; any cached Boolean/enum/string options require separate review. Availability/options are not verified."))
    }));
    let selected = host::pick("Optional cached GHCR feature", &items)?.ok_or(Error::Cancelled)?;
    if selected == "none" { return Ok(text); }
    let index = selected.strip_prefix("feature-").and_then(|v| v.parse::<usize>().ok()).ok_or(Error::Stale)?;
    let (reference, _) = candidates.get(index).ok_or(Error::Stale)?;
    if value.get("features").is_none() { value["features"] = serde_json::json!({}); }
    value["features"].as_object_mut().ok_or("The bundled features value is not an object.")?
        .insert(reference.clone(), crate::feature_options::choose(reference)?);
    Ok(format!("{}\n", serde_json::to_string_pretty(&value).map_err(|_| "Cannot render the configuration.")?))
}

fn current_project(doc: &Document, expected: &str) -> Result<Project, Error> {
    let project = host::project_of(&doc.path).ok_or(Error::Stale)?;
    if project.root()? != expected {
        return Err(Error::Stale);
    }
    Ok(project)
}

impl FileActions for Main {
    fn file_actions(&mut self, doc: &Document, _selection: Range) -> Vec<FileAction> {
        if !matches!(doc.language.as_str(), "json" | "jsonc") {
            return Vec::new();
        }
        let Some(project) = host::project_of(&doc.path) else { return Vec::new() };
        let Ok(root) = project.root() else { return Vec::new() };
        vec![FileAction {
            title: "Create Dev Container configuration…".into(),
            kind: Some("refactor.rewrite".into()),
            data: Some(serde_json::json!([root, doc.path, doc.version]).to_string()),
            ..FileAction::default()
        }]
    }

    fn resolve_file_action(&mut self, doc: &Document, action: FileAction) -> Result<FileAction, Error> {
        let data: serde_json::Value =
            serde_json::from_str(action.data.as_deref().ok_or("Missing creation context.")?).map_err(|_| "Invalid creation context.")?;
        let data = data.as_array().filter(|a| a.len() == 3).ok_or("Invalid creation context.")?;
        let root = data[0].as_str().ok_or("Invalid creation project.")?;
        if data[1].as_str() != Some(doc.path.as_str()) || data[2].as_u64() != Some(doc.version) {
            return Err(Error::Stale);
        }
        let project = current_project(doc, root)?;
        let mut items: Vec<_> = TEMPLATES.iter().map(|(id, label, _)| PickItem::new(id, label)).collect();
        items.push(PickItem::new("dockerfile", "Existing project Dockerfile").description("Preview a bounded existing Dockerfile; create only the configuration. Build context is the project root."));
        items.push(PickItem::new("compose-json", "Existing project Compose YAML or JSON").description("Strict JSON or unambiguous YAML; choose a declared service and enter an explicit container workspace path."));
        let key = host::pick("Dev Container configuration source", &items)?.ok_or(Error::Cancelled)?;
        let default_name = if key == "dockerfile" { "Dockerfile project" } else if key == "compose-json" { "Compose project" } else {
            TEMPLATES.iter().find(|(id, ..)| *id == key).map(|(_, name, _)| *name).ok_or("Unknown bundled template.")?
        };
        let target = host::input("Configuration destination inside this project", Some(".devcontainer/devcontainer.json"))?
            .ok_or(Error::Cancelled)?;
        let target = destination(root, &target)?;
        vacant(&project, &target)?;
        let name = host::input("Container display name", Some(default_name))?.ok_or(Error::Cancelled)?;
        let dockerfile = if key == "dockerfile" {
            let path = host::input("Existing Dockerfile inside this project", Some("Dockerfile"))?.ok_or(Error::Cancelled)?;
            Some(crate::dockerfile::Snapshot::capture(&project, &path)?)
        } else { None };
        let compose = if key == "compose-json" {
            let path = host::input("Existing Compose YAML or JSON inside this project", Some("compose.json"))?.ok_or(Error::Cancelled)?;
            Some(crate::compose::Snapshot::capture(&project, &path)?)
        } else { None };
        let text = with_cached_feature(match (&dockerfile, &compose) {
            (Some(snapshot), _) => snapshot.configuration(&target, &name)?,
            (_, Some(snapshot)) => snapshot.configuration(&target, &name)?,
            _ => source(&key, &name)?,
        })?;
        let source_preview = dockerfile.as_ref().map(|snapshot| snapshot.preview()).or_else(|| compose.as_ref().map(|snapshot| snapshot.preview())).map(|text| format!("\n\n{text}")).unwrap_or_default();
        let absolute = format!("{}/{}", root.trim_end_matches('/'), target);
        if !ui_kit::prompts::confirm_exact(
            "Create Dev Container configuration?",
            &format!(
                "Destination: {}\n\n{}\nCreates this file only. Feature options are explicitly selected from cached Boolean/enum/string metadata or omitted for feature defaults; metadata does not verify current availability, image compatibility, options or runtime behavior. No image/feature is pulled and no container is started. Review the generated configuration before using Start.{}",
                serde_json::to_string(&absolute).unwrap_or_default(),
                text,
                source_preview
            ),
        )? {
            return Err(Error::Cancelled);
        }
        // Prompts can stay open while project/files change. Recheck after the
        // final confirmation; overwrite:false is also enforced when applied.
        let project = current_project(doc, root)?;
        vacant(&project, &target)?;
        if let Some(snapshot) = &dockerfile { snapshot.validate(&project)?; }
        if let Some(snapshot) = &compose { snapshot.validate(&project)?; }
        Ok(FileAction {
            title: "Create Dev Container configuration".into(),
            kind: Some("refactor.rewrite".into()),
            operations: vec![FileOperation::Create { path: absolute.clone(), text, overwrite: false }],
            open: Some(absolute),
            ..FileAction::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn setup() -> (MockHost, Main, Document, FileAction) {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        let doc = Document { path: "/project/package.json".into(), language: "json".into(), version: 3, text: "{}".into() };
        let mut plugin = Main::default();
        let action = plugin.file_actions(&doc, Range::default()).pop().unwrap();
        (host, plugin, doc, action)
    }
    fn answers(host: &MockHost, target: &str) {
        for answer in [Some(&["node"][..]), Some(&[target][..]), Some(&["Team \"A\""][..]), Some(&[][..])] {
            host.answer_next_prompt(answer);
        }
    }

    #[test]
    fn bundled_templates_create_exact_confirmed_json_without_writes_or_cli() {
        let (host, mut plugin, doc, action) = setup();
        answers(&host, ".devcontainer/team-a/devcontainer.json");
        let resolved = plugin.resolve_file_action(&doc, action).unwrap();
        let FileOperation::Create { path, text, overwrite } = &resolved.operations[0] else { panic!() };
        assert_eq!(path, "/project/.devcontainer/team-a/devcontainer.json");
        assert!(!overwrite);
        assert_eq!(resolved.open.as_ref(), Some(path));
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(value["name"], "Team \"A\"");
        assert_eq!(value["image"], "mcr.microsoft.com/devcontainers/javascript-node:22");
        assert!(!host.invocation().project().unwrap().exists(path).unwrap(), "resolution does not write");
        assert!(host.terminals().is_empty() && host.process_runs().is_empty());
        for (key, _, bundled) in TEMPLATES {
            let mut expected: serde_json::Value = serde_json::from_str(bundled).unwrap();
            expected["name"] = "Name".into();
            assert_eq!(serde_json::from_str::<serde_json::Value>(&source(key, "Name").unwrap()).unwrap(), expected);
        }
    }

    #[test]
    fn cancellation_at_every_prompt_returns_no_operation() {
        for stage in 0..4 {
            let (host, mut plugin, doc, action) = setup();
            let values = ["base", ".devcontainer/devcontainer.json", "Development", "yes"];
            for (n, value) in values.iter().enumerate().take(stage + 1) {
                let answer = [*value];
                host.answer_next_prompt(if n == stage { None } else { Some(&answer) });
            }
            assert!(matches!(plugin.resolve_file_action(&doc, action), Err(Error::Cancelled)));
            assert!(host.terminals().is_empty() && host.process_runs().is_empty());
        }
    }

    #[test]
    fn destination_rejects_traversal_external_deep_and_control_paths() {
        for invalid in [
            "../devcontainer.json",
            "/other/.devcontainer.json",
            ".devcontainer/x/y/devcontainer.json",
            "arbitrary.json",
            ".devcontainer/x\ny/devcontainer.json",
            ".devcontainer/x.y/devcontainer.json",
            "",
        ] {
            assert!(destination("/project", invalid).is_err(), "{invalid:?}");
        }
        for valid in [".devcontainer.json", ".devcontainer/devcontainer.json", ".devcontainer/team_1/devcontainer.json"] {
            assert_eq!(destination("/project", valid).unwrap(), valid);
        }
        assert!(source("compose", "Development").is_err());
        assert!(source("node", "\n").is_err());
    }

    #[test]
    fn existing_files_open_buffers_and_parent_files_cannot_be_overwritten() {
        let (host, _, _, _) = setup();
        let project = host.invocation().project().unwrap();
        host.add_file(".devcontainer.json", "original");
        assert!(vacant(&project, ".devcontainer.json").is_err());
        host.open_editor("/project/.devcontainer/new/devcontainer.json", "jsonc", "unsaved", &[]);
        assert!(vacant(&project, ".devcontainer/new/devcontainer.json").is_err());
        host.add_file(".devcontainer", "a regular file");
        assert!(vacant(&project, ".devcontainer/devcontainer.json").is_err());
    }

    #[test]
    fn source_path_version_and_project_ownership_are_required() {
        let (host, mut plugin, mut doc, action) = setup();
        doc.version += 1;
        assert!(matches!(plugin.resolve_file_action(&doc, action.clone()), Err(Error::Stale)));
        doc.version -= 1;
        doc.path = "/project/other.json".into();
        assert!(matches!(plugin.resolve_file_action(&doc, action.clone()), Err(Error::Stale)));
        doc.path = "/project/package.json".into();
        host.set_project_root(Some("/different"));
        assert!(matches!(plugin.resolve_file_action(&doc, action), Err(Error::Stale)));
    }

    #[test]
    fn collision_is_checked_during_resolution_without_overwriting() {
        let (host, mut plugin, doc, action) = setup();
        host.add_file(".devcontainer/devcontainer.json", "keep this");
        answers(&host, ".devcontainer/devcontainer.json");
        assert!(plugin.resolve_file_action(&doc, action).is_err());
        assert_eq!(host.invocation().project().unwrap().read_file(".devcontainer/devcontainer.json").unwrap(), "keep this");
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn creation_is_available_only_for_project_json_with_read_access() {
        let (host, mut plugin, mut doc, _) = setup();
        doc.language = "rust".into();
        assert!(plugin.file_actions(&doc, Range::default()).is_empty());
        doc.language = "jsonc".into();
        doc.path = "/elsewhere/config.json".into();
        assert!(plugin.file_actions(&doc, Range::default()).is_empty());
        doc.path = "/project/package.json".into();
        let _host = host.with_capabilities(&[]);
        assert!(plugin.file_actions(&doc, Range::default()).is_empty());
    }

    #[test]
    fn creation_cannot_target_another_open_nested_project() {
        let (host, mut plugin, doc, action) = setup();
        host.add_project("/project/.devcontainer/team");
        answers(&host, ".devcontainer/team/devcontainer.json");
        assert!(plugin.resolve_file_action(&doc, action).is_err());
        assert!(host.terminals().is_empty() && host.process_runs().is_empty());
        assert!(!host.invocation().project().unwrap().exists(".devcontainer/team/devcontainer.json").unwrap());
    }

    #[test]
    fn cached_feature_metadata_can_be_selected_without_fetching_or_executing() {
        let (host, mut plugin, doc, action) = setup();
        let host = host.with_capabilities(&["project.read", "storage"]);
        host::storage::set("feature-index", "ghcr.io/devcontainers/features/node:1\tNode.js\n").unwrap();
        for answer in ["base", ".devcontainer/devcontainer.json", "Feature project", "feature-0", "yes"] {
            host.answer_next_prompt(Some(&[answer]));
        }
        let resolved = plugin.resolve_file_action(&doc, action).unwrap();
        let FileOperation::Create { text, overwrite: false, .. } = &resolved.operations[0] else { panic!("create only") };
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(value["features"]["ghcr.io/devcontainers/features/node:1"], serde_json::json!({}));
        assert!(host.requests().is_empty() && host.process_runs().is_empty() && host.terminals().is_empty());
    }

    #[test]
    fn optional_feature_none_cancel_forged_or_denied_cache_never_runs_a_tool() {
        for choice in [Some("none"), None, Some("feature-999")] {
            let (host, mut plugin, doc, action) = setup();
            let host = host.with_capabilities(&["project.read", "storage"]);
            host::storage::set("feature-index", "ghcr.io/devcontainers/features/node:1\tNode.js\n").unwrap();
            for answer in ["base", ".devcontainer/devcontainer.json", "Feature project"] { host.answer_next_prompt(Some(&[answer])); }
            if let Some(choice) = choice { host.answer_next_prompt(Some(&[choice])); } else { host.answer_next_prompt(None); }
            host.answer_next_prompt(Some(&[]));
            let resolved = plugin.resolve_file_action(&doc, action);
            match choice {
                Some("none") => {
                    let action = resolved.unwrap();
                    let FileOperation::Create { text, .. } = &action.operations[0] else { panic!() };
                    assert!(serde_json::from_str::<serde_json::Value>(text).unwrap()["features"].as_object().unwrap().is_empty());
                }
                None => assert!(matches!(resolved, Err(Error::Cancelled))),
                // The pinned SDK rejects a pick not present in the offered items.
                _ => assert!(matches!(resolved, Err(Error::Invalid(_)))),
            }
            assert!(host.requests().is_empty() && host.process_runs().is_empty() && host.terminals().is_empty());
        }
        let (host, mut plugin, doc, action) = setup();
        let host = host.with_capabilities(&["project.read", "storage"]);
        host::storage::set("feature-index", "ghcr.io/devcontainers/features/node:1\tNode.js\n").unwrap();
        let host = host.with_capabilities(&["project.read"]);
        answers(&host, ".devcontainer/devcontainer.json");
        assert!(plugin.resolve_file_action(&doc, action).is_ok(), "denied optional storage preserves image-only creation");
    }

    #[test]
    fn selected_feature_preserves_existing_bundled_feature_options() {
        let host = MockHost::install().with_capabilities(&["storage"]);
        host::storage::set("feature-index", "ghcr.io/devcontainers/features/node:1\tNode\nghcr.io/devcontainers/features/rust:1\tRust\n").unwrap();
        host.answer_next_prompt(Some(&["feature-0"]));
        let value = with_cached_feature(serde_json::json!({"features":{"ghcr.io/devcontainers/features/node:1":{"version":"20"}}}).to_string()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&value).unwrap();
        assert_eq!(value["features"]["ghcr.io/devcontainers/features/node:1"]["version"], "20");
        assert_eq!(value["features"]["ghcr.io/devcontainers/features/rust:1"], serde_json::json!({}));
    }
    #[test]
    fn public_existing_dockerfile_creation_uses_destination_relative_paths_and_never_executes() {
        let (host, mut plugin, doc, action) = setup();
        host.add_file("containers/Dockerfile.dev", "FROM ubuntu:24.04\nRUN printf 'hello'\n");
        for answer in ["dockerfile", ".devcontainer/team-a/devcontainer.json", "Dockerfile project", "containers/Dockerfile.dev", "yes"] {
            host.answer_next_prompt(Some(&[answer]));
        }
        let resolved = plugin.resolve_file_action(&doc, action).expect("owned Dockerfile-based config is available");
        let FileOperation::Create { path, text, overwrite } = &resolved.operations[0] else { panic!() };
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(value["build"]["dockerfile"], "../../containers/Dockerfile.dev");
        assert_eq!(value["build"]["context"], "../..");
        assert!(value.get("image").is_none() && !overwrite);
        assert_eq!(path, "/project/.devcontainer/team-a/devcontainer.json");
        assert!(host.process_runs().is_empty() && host.terminals().is_empty() && host.requests().is_empty());
        assert!(!host.invocation().project().unwrap().exists(path).unwrap());
    }

    #[test]
    fn dockerfile_creation_cancel_at_each_prompt_returns_no_operation() {
        for stage in 0..5 {
            let (host, mut plugin, doc, action) = setup();
            host.add_file("Dockerfile", "FROM scratch\n");
            for (n, value) in ["dockerfile", ".devcontainer/devcontainer.json", "Build project", "Dockerfile", "yes"].iter().enumerate().take(stage+1) {
                let answer = [*value];
                host.answer_next_prompt(if n == stage { None } else { Some(&answer) });
            }
            assert!(matches!(plugin.resolve_file_action(&doc, action), Err(Error::Cancelled)), "stage {stage}");
            assert!(host.process_runs().is_empty() && host.terminals().is_empty());
            assert!(!host.invocation().project().unwrap().exists(".devcontainer/devcontainer.json").unwrap());
        }
    }
    #[test]
    fn dockerfile_creation_combines_cached_feature_and_refuses_foreign_source_or_collision() {
        let (host, mut plugin, doc, action) = setup();
        let host = host.with_capabilities(&["project.read", "storage"]);
        host.add_file("Dockerfile", "FROM scratch\n");
        host::storage::set("feature-index", "ghcr.io/devcontainers/features/node:1\tNode\n").unwrap();
        for answer in ["dockerfile", ".devcontainer/devcontainer.json", "Build project", "Dockerfile", "feature-0", "yes"] { host.answer_next_prompt(Some(&[answer])); }
        let resolved = plugin.resolve_file_action(&doc, action).unwrap();
        let FileOperation::Create { text, overwrite: false, .. } = &resolved.operations[0] else { panic!() };
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(value["build"]["dockerfile"], "../Dockerfile");
        assert_eq!(value["features"]["ghcr.io/devcontainers/features/node:1"], serde_json::json!({}));
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
        for foreign in [false,true] {
            let (host, mut plugin, doc, action) = setup();
            host.add_file("containers/Dockerfile", "FROM scratch\n");
            if foreign { host.add_project("/project/containers"); } else { host.add_file(".devcontainer/devcontainer.json", "keep"); }
            for answer in ["dockerfile", ".devcontainer/devcontainer.json", "Build project", "containers/Dockerfile", "yes"] { host.answer_next_prompt(Some(&[answer])); }
            assert!(plugin.resolve_file_action(&doc, action).is_err());
            if !foreign { assert_eq!(host.invocation().project().unwrap().read_file(".devcontainer/devcontainer.json").unwrap(), "keep"); }
            assert!(host.process_runs().is_empty() && host.terminals().is_empty());
        }
    }

    #[test]
    fn public_compose_json_creation_selects_existing_service_and_explicit_workspace_only() {
        let (host, mut plugin, doc, action) = setup();
        host.add_file("containers/compose.json", r#"{"services":{"api":{"image":"busybox"},"db":{"image":"postgres"}}}"#);
        for answer in ["compose-json", ".devcontainer/team/devcontainer.json", "Compose project", "containers/compose.json", "service-0", "/workspace/api", "yes"] { host.answer_next_prompt(Some(&[answer])); }
        let resolved = plugin.resolve_file_action(&doc, action).expect("owned Compose JSON creation is available");
        let FileOperation::Create { text, overwrite: false, .. } = &resolved.operations[0] else { panic!() };
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(value["dockerComposeFile"], "../../containers/compose.json");
        assert_eq!(value["service"], "api");assert_eq!(value["workspaceFolder"], "/workspace/api");
        assert!(value.get("image").is_none() && value.get("build").is_none());
        assert!(host.process_runs().is_empty() && host.requests().is_empty() && host.terminals().is_empty());
    }

    #[test]
    fn compose_creation_cancel_at_every_prompt_and_cached_default_feature_stay_side_effect_free() {
        for stage in 0..7 {
            let (host, mut plugin, doc, action) = setup();host.add_file("compose.json", r#"{"services":{"api":{}}}"#);
            for (n,value) in ["compose-json", ".devcontainer/devcontainer.json", "Compose project", "compose.json", "service-0", "/workspace/api", "yes"].iter().enumerate().take(stage+1) {
                let answer=[*value];host.answer_next_prompt(if n==stage {None}else{Some(&answer)});
            }
            assert!(matches!(plugin.resolve_file_action(&doc,action),Err(Error::Cancelled)),"stage {stage}");assert!(host.process_runs().is_empty() && host.terminals().is_empty());
        }
        let (host,mut plugin,doc,action)=setup();let host=host.with_capabilities(&["project.read","storage"]);host.add_file("compose.json",r#"{"services":{"api":{}}}"#);
        host::storage::set("feature-index","ghcr.io/devcontainers/features/node:1\tNode\n").unwrap();
        for answer in ["compose-json", ".devcontainer/devcontainer.json", "Compose project", "compose.json", "service-0", "/workspace/api", "feature-0", "yes"] {host.answer_next_prompt(Some(&[answer]));}
        let resolved=plugin.resolve_file_action(&doc,action).unwrap();let FileOperation::Create{text,overwrite:false,..}=&resolved.operations[0] else{panic!()};let value:serde_json::Value=serde_json::from_str(text).unwrap();
        assert_eq!(value["features"]["ghcr.io/devcontainers/features/node:1"],serde_json::json!({}));assert_eq!(value["service"],"api");assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

    #[test]
    fn public_creation_uses_explicit_cached_boolean_and_enum_feature_values() {
        let (host,mut plugin,doc,action)=setup();let host=host.with_capabilities(&["project.read","storage"]);
        let index="ghcr.io/acme/features/tool:1\tTool\n";
        host::storage::set("feature-index",index).unwrap();
        host::storage::set("feature-options-v1",&serde_json::json!({"index":index,"features":{"ghcr.io/acme/features/tool:1":{"enabled":{"type":"boolean","default":true},"version":{"type":"string","enum":["stable","next"],"default":"stable"}}}}).to_string()).unwrap();
        for answer in ["base",".devcontainer/devcontainer.json","Feature project","feature-0","configure","value-1","value-1","yes"] {host.answer_next_prompt(Some(&[answer]));}
        let resolved=plugin.resolve_file_action(&doc,action).unwrap();let FileOperation::Create{text,..}=&resolved.operations[0] else {panic!()};let value:serde_json::Value=serde_json::from_str(text).unwrap();
        assert_eq!(value["features"]["ghcr.io/acme/features/tool:1"]["enabled"],false);
        assert_eq!(value["features"]["ghcr.io/acme/features/tool:1"]["version"],"next");
        assert!(host.requests().is_empty() && host.process_runs().is_empty() && host.terminals().is_empty());
    }

    #[test]
    fn cancel_during_cached_option_review_returns_no_creation_operation() {
        for option_stage in 0..2 {
            let (host,mut plugin,doc,action)=setup();let host=host.with_capabilities(&["project.read","storage"]);let index="ghcr.io/acme/features/tool:1\tTool\n";
            host::storage::set("feature-index",index).unwrap();host::storage::set("feature-options-v1",&serde_json::json!({"index":index,"features":{"ghcr.io/acme/features/tool:1":{"enabled":{"type":"boolean"}}}}).to_string()).unwrap();
            for answer in ["base",".devcontainer/devcontainer.json","Feature project","feature-0"]{host.answer_next_prompt(Some(&[answer]));}
            if option_stage==1{host.answer_next_prompt(Some(&["configure"]));}host.answer_next_prompt(None);
            assert!(matches!(plugin.resolve_file_action(&doc,action),Err(Error::Cancelled)));assert!(!host.invocation().project().unwrap().exists(".devcontainer/devcontainer.json").unwrap());assert!(host.requests().is_empty() && host.process_runs().is_empty());
        }
    }

}
