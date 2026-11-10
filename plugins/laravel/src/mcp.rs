//! Project-scoped tools over the same owned inventories and explicit execution paths.
use crate::{Main, artisan_runner::Runner};
use openidea_plugin::{Error, Invocation};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    resource: String,
    #[serde(default)]
    query: String,
    #[serde(default)]
    offset: usize,
    #[serde(default = "limit")]
    limit: usize,
}
fn limit() -> usize {
    100
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunnerInput {
    #[serde(default)]
    runner: Runner,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotInput {
    #[serde(default)]
    runner: Runner,
    #[serde(default, rename = "includeLogs")]
    include_logs: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Refresh {
    #[serde(default)]
    runner: Runner,
    section: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Execute {
    runner: Runner,
    command: String,
    token: String,
    #[serde(rename = "requestId")]
    id: String,
    values: std::collections::BTreeMap<String, Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    id: String,
}
fn decode<T: serde::de::DeserializeOwned>(input: &str) -> Result<T, Error> {
    if input.len() > 128 * 1024 {
        return Err(Error::Invalid("Tool arguments exceed 128 KiB.".into()));
    }
    let value = crate::strict_json::parse(input, 128 * 1024)?;
    serde_json::from_value(value).map_err(|error| Error::Invalid(error.to_string()))
}
// Log tails can contain credentials or request payloads. Capturing them in the UI
// must not silently broaden ordinary metadata reads by an agent.
fn metadata_snapshot(mut snapshot: Value, include_logs: bool) -> Value {
    if !include_logs {
        if let Some(snapshots) = snapshot["snapshots"].as_array_mut() {
            snapshots.retain(|section| section["section"] != "logs");
        }
    }
    snapshot
}
impl openidea_plugin::McpProvider for Main {
    fn call(&mut self, tool: &str, arguments: &str, ctx: &Invocation) -> Result<String, Error> {
        let project = ctx.project().ok_or("Open a Laravel project first.")?;
        // Job lifecycle calls must remain available after source/index invalidation.
        // Keep their project ownership gate independent from the source inventory.
        let root = project.root()?;
        if !crate::hover::canonical(&root)
            || !openidea_plugin::host::projects()?
                .iter()
                .any(|project| project.root().ok().as_deref() == Some(root.as_str()))
        {
            return Err(Error::Stale);
        }
        let result = match tool {
            "inventory" => {
                let index = self
                    .view_index(ctx)
                    .ok_or("Index this Laravel project before using its tools.")?;
                let request: Inventory = decode(arguments)?;
                if request.query.len() > 200
                    || request.limit == 0
                    || request.limit > 200
                    || request.offset > 10000
                {
                    return Err(Error::Invalid("Invalid inventory pagination/query.".into()));
                }
                let root = project.root()?;
                let mut items = vec![];
                if request.resource == "routes" {
                    for route in self.active_routes(ctx) {
                        items.push(json!({"name":route.name,"path":route.path,"methods":route.method,"handler":route.handler.class}));
                    }
                } else if request.resource == "models" {
                    for (path, table) in &index.model_files {
                        if crate::owns_current_path(&project, path) {
                            items.push(json!({"name":table,"file":path.strip_prefix(&root).unwrap_or(path)}));
                        }
                    }
                } else {
                    let names = match request.resource.as_str() {
                        "views" => &index.views,
                        "components" => &index.components,
                        "livewire" => &index.livewire,
                        "config" => &index.config,
                        "translations" => &index.translations,
                        "middleware" => &index.middleware,
                        "env" => &index.env,
                        _ => return Err(Error::NotFound(request.resource)),
                    };
                    for (name, entry) in names.iter() {
                        if crate::owns_current_path(&project, &entry.path) {
                            items.push(json!({"name":name,"file":entry.path.strip_prefix(&root).unwrap_or(&entry.path).trim_start_matches('/')}));
                        }
                    }
                }
                let query = request.query.to_lowercase();
                items.retain(|item| item.to_string().to_lowercase().contains(&query));
                let total = items.len();
                let page: Vec<_> = items
                    .into_iter()
                    .skip(request.offset)
                    .take(request.limit)
                    .collect();
                json!({"resource":request.resource,"total":total,"offset":request.offset,"items":page,"provenance":"Cached source inventory; values and environment secrets omitted"})
            }
            "artisan_commands" => {
                let input: RunnerInput = decode(arguments)?;
                let (mut snapshot, token) = input.runner.catalog(&project)?;
                // Defaults can contain app-specific values; agents receive the parameter schema only.
                for command in snapshot["commands"].as_array_mut().into_iter().flatten() {
                    for kind in ["arguments", "options"] {
                        for field in command["definition"][kind]
                            .as_array_mut()
                            .into_iter()
                            .flatten()
                        {
                            if let Some(object) = field.as_object_mut() {
                                object.remove("default");
                            }
                        }
                    }
                }
                json!({"runner":input.runner,"token":token,"capturedAt":snapshot["capturedAt"],"commands":snapshot["commands"]})
            }
            "runtime_snapshot" => {
                let input: SnapshotInput = decode(arguments)?;
                metadata_snapshot(
                    self.runtime_inspection.cached(ctx, input.runner)?,
                    input.include_logs,
                )
            }
            "runtime_refresh" => {
                let input: Refresh = decode(arguments)?;
                if ![
                    "about",
                    "events",
                    "schedule",
                    "queues",
                    "queue-status",
                    "workers",
                    "schema",
                ]
                .contains(&input.section.as_str())
                {
                    return Err(Error::Invalid("This runtime tool only refreshes metadata sections; select a log file explicitly in Runtime.".into()));
                }
                self.runtime_inspection
                    .capture(ctx, input.runner, &input.section)?;
                metadata_snapshot(self.runtime_inspection.cached(ctx, input.runner)?, false)
            }
            "artisan_execute" => {
                let input: Execute = decode(arguments)?;
                let (snapshot, token) = input.runner.catalog(&project)?;
                if token != input.token {
                    return Err(Error::Stale);
                }
                let definition = snapshot["commands"]
                    .as_array()
                    .and_then(|commands| {
                        commands
                            .iter()
                            .find(|command| command["name"] == input.command)
                    })
                    .ok_or(Error::Stale)?;
                crate::artisan_workspace::arguments(definition, &input.values, true)?;
                // Same token/argv/duplicate-submission checks and single-run lock as the workspace.
                self.artisan_workspace.handle(
                    ctx,
                    &json!({"op":"runner","runner":input.runner}).to_string(),
                    &self.artisan_terminals,
                )?;
                self.artisan_workspace.handle(ctx,&json!({"op":"run","id":input.id,"command":input.command,"token":input.token,"values":input.values}).to_string(),&self.artisan_terminals)?
            }
            "artisan_job" | "artisan_cancel" => {
                let input: Job = decode(arguments)?;
                let result = self.artisan_workspace.handle(
                    ctx,
                    &json!({"op":if tool=="artisan_job"{"poll"}else{"cancel"},"id":input.id})
                        .to_string(),
                    &self.artisan_terminals,
                )?;
                if result["completed"] == true {
                    self.runtime_routes.remove(&project.root()?);
                    self.index_project(&project);
                    crate::refresh_laravel();
                }
                result
            }
            _ => return Err(Error::NotFound(tool.into())),
        };
        let raw = result.to_string();
        if raw.len() > 4 * 1024 * 1024 {
            return Err(Error::Invalid("Tool result exceeds 4 MiB.".into()));
        }
        Ok(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{McpProvider, host, mock::MockHost};
    fn seeded(host: &MockHost) -> Main {
        let plugin = crate::tests::activated(host);
        host.add_file("artisan", "<?php");
        let project = host.invocation().project().unwrap();
        let definition = json!({"commands":[{"name":"app:probe","description":"Probe","definition":{"arguments":[{"name":"name","is_required":true,"default":"secret"}],"options":[]}}]});
        let commands = crate::artisan_metadata::parse(&definition.to_string()).unwrap();
        let snapshot = json!({"schema":2,"identity":crate::artisan_metadata::identity(&project).unwrap(),"capturedAt":1,"commands":commands});
        host::storage::set(
            &format!(
                "artisan-metadata-v2:{}",
                crate::artisan_metadata::hash("/project")
            ),
            &snapshot.to_string(),
        )
        .unwrap();
        plugin
    }
    #[test]
    fn captured_logs_require_explicit_agent_opt_in_and_refresh_never_returns_them() {
        let captured = json!({"runner":"php","snapshots":[{"section":"about","rows":[]},{"section":"logs","rows":[{"cells":["Logs","laravel.log","private log content"]}]}]});
        let ordinary = metadata_snapshot(captured.clone(), false);
        assert_eq!(ordinary["snapshots"].as_array().unwrap().len(), 1);
        assert!(!ordinary.to_string().contains("private log content"));
        assert!(
            metadata_snapshot(captured, true)
                .to_string()
                .contains("private log content")
        );
        assert!(!decode::<SnapshotInput>("{}").unwrap().include_logs);
        assert!(
            decode::<SnapshotInput>(r#"{"includeLogs":true}"#)
                .unwrap()
                .include_logs
        );
        assert!(decode::<SnapshotInput>(r#"{"includeLogs":"true"}"#).is_err());
    }

    #[test]
    fn agent_inventory_is_paginated_and_omits_config_env_values() {
        let host = MockHost::install();
        let mut plugin = seeded(&host);
        let ctx = host.invocation();
        let output = plugin
            .call(
                "inventory",
                r#"{"resource":"config","query":"app.name","limit":1}"#,
                &ctx,
            )
            .unwrap();
        let value: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["total"], 1);
        assert_eq!(value["items"][0]["name"], "app.name");
        assert!(!output.contains("APP_NAME"));
        assert!(
            plugin
                .call("inventory", r#"{"resource":"env","limit":201}"#, &ctx)
                .is_err()
        );
        assert!(
            plugin
                .call("inventory", r#"{"resource":"env","unknown":true}"#, &ctx)
                .is_err()
        );
        assert!(
            plugin
                .call(
                    "inventory",
                    r#"{"resource":"env","resource":"config"}"#,
                    &ctx
                )
                .is_err()
        );
        let catalog = plugin.call("artisan_commands", "{}", &ctx).unwrap();
        assert!(!catalog.contains("secret"));
        assert!(catalog.contains("token"));
        assert!(
            plugin
                .call("runtime_snapshot", "{}", &ctx)
                .unwrap()
                .contains("snapshots")
        );
        assert!(host.process_runs().is_empty());
        assert!(plugin.artisan_terminals.commands().is_empty());
    }
    #[test]
    fn agent_execution_uses_the_workspace_token_validation_lock_and_job_owner() {
        let host = MockHost::install().with_capabilities(&[
            "project.read",
            "storage",
            "events",
            "process:php",
        ]);
        let mut plugin = seeded(&host);
        let ctx = host.invocation();
        let catalog: Value =
            serde_json::from_str(&plugin.call("artisan_commands", "{}", &ctx).unwrap()).unwrap();
        let mut request = json!({"runner":"php","command":"app:probe","token":"stale","requestId":"agent-execution-0001","values":{"name":"A B; $HOME"}});
        assert!(
            plugin
                .call("artisan_execute", &request.to_string(), &ctx)
                .is_err()
        );
        request["token"] = catalog["token"].clone();
        let reply: Value = serde_json::from_str(
            &plugin
                .call("artisan_execute", &request.to_string(), &ctx)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            reply["job"]["argv"],
            json!([
                "artisan",
                "app:probe",
                "--no-interaction",
                "--no-ansi",
                "--",
                "A B; $HOME"
            ])
        );
        assert!(
            plugin
                .call("artisan_execute", &request.to_string(), &ctx)
                .is_err()
        );
        assert!(
            plugin
                .call("artisan_job", r#"{"id":"foreign-execution-0000"}"#, &ctx)
                .is_err()
        );
        plugin
            .call("artisan_cancel", r#"{"id":"agent-execution-0001"}"#, &ctx)
            .unwrap();
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn invalid_agent_arguments_do_not_switch_the_selected_workspace_runner() {
        let host = MockHost::install();
        let mut plugin = seeded(&host);
        let ctx = host.invocation();
        let catalog: Value =
            serde_json::from_str(&plugin.call("artisan_commands", "{}", &ctx).unwrap()).unwrap();
        plugin
            .artisan_workspace
            .handle(
                &ctx,
                r#"{"op":"runner","runner":"docker"}"#,
                &plugin.artisan_terminals,
            )
            .unwrap();
        let request = json!({"runner":"php","command":"app:probe","token":catalog["token"],"requestId":"agent-invalid-argv-0001","values":{"unknown":"value"}});
        assert!(
            plugin
                .call("artisan_execute", &request.to_string(), &ctx)
                .is_err()
        );
        assert_eq!(
            plugin.artisan_workspace.runner(&ctx).unwrap(),
            Runner::Docker
        );
        assert!(host.process_runs().is_empty());
    }

    #[test]
    fn owned_job_can_be_polled_and_cancelled_after_source_inventory_is_removed() {
        let host = MockHost::install().with_capabilities(&[
            "project.read",
            "storage",
            "events",
            "process:php",
        ]);
        let mut plugin = seeded(&host);
        let ctx = host.invocation();
        let catalog: Value =
            serde_json::from_str(&plugin.call("artisan_commands", "{}", &ctx).unwrap()).unwrap();
        let request = json!({"runner":"php","command":"app:probe","token":catalog["token"],"requestId":"agent-index-retired-0001","values":{"name":"Keep job controllable"}});
        plugin
            .call("artisan_execute", &request.to_string(), &ctx)
            .unwrap();
        plugin.indexes.clear();
        assert!(
            plugin
                .call("inventory", r#"{"resource":"views"}"#, &ctx)
                .is_err()
        );
        plugin
            .call("artisan_job", r#"{"id":"agent-index-retired-0001"}"#, &ctx)
            .unwrap();
        plugin
            .call(
                "artisan_cancel",
                r#"{"id":"agent-index-retired-0001"}"#,
                &ctx,
            )
            .unwrap();
        host.add_project("/project");
        host.set_project_root(Some("/other"));
        assert!(
            plugin
                .call(
                    "artisan_cancel",
                    r#"{"id":"agent-index-retired-0001"}"#,
                    &host.invocation()
                )
                .is_err()
        );
    }

    #[test]
    fn agent_calls_require_owned_index_and_process_grants() {
        let host = MockHost::install();
        let mut plugin = seeded(&host);
        host.add_project("/project/resources/views");
        let output = plugin
            .call("inventory", r#"{"resource":"views"}"#, &host.invocation())
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&output).unwrap()["total"], 0);
        host.set_project_root(None);
        assert!(
            plugin
                .call("inventory", r#"{"resource":"views"}"#, &host.invocation())
                .is_err()
        );
        let host = MockHost::install().with_capabilities(&["project.read", "storage", "events"]);
        let mut plugin = seeded(&host);
        let ctx = host.invocation();
        let catalog: Value =
            serde_json::from_str(&plugin.call("artisan_commands", "{}", &ctx).unwrap()).unwrap();
        let request = json!({"runner":"php","command":"app:probe","token":catalog["token"],"requestId":"agent-execution-0001","values":{"name":"A"}});
        assert!(matches!(
            plugin.call("artisan_execute", &request.to_string(), &ctx),
            Err(Error::Denied(_))
        ));
    }
}
