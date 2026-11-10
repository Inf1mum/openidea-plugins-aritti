//! Opt-in acceptance against the released host Registry and HTTP MCP server.
//! This uses only a newly-created disposable project and profile, never the user's IDE.
use ide_core::IdeEvent;
use ide_host::{
    Host, HostConfig,
    mcp::Registry,
    plugins::{Consent, HostWorkspace, PluginEvent, PluginHost},
};
use ide_mcp::{McpServer, discovery_path};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

struct Harness {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    host: Arc<Host>,
    plugins: Arc<PluginHost>,
    server: McpServer,
    http: reqwest::Client,
    events: Arc<Mutex<Vec<IdeEvent>>>,
}
impl Harness {
    async fn new(package: &Path) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap().join("project");
        std::fs::create_dir_all(root.join("resources/views")).unwrap();
        std::fs::write(
            root.join("resources/views/welcome.blade.php"),
            "<h1>Fixture</h1>",
        )
        .unwrap();
        std::fs::write(
            root.join("composer.json"),
            r#"{"require":{"laravel/framework":"^13.0"}}"#,
        )
        .unwrap();
        // Only this isolated script executes in process-grant tests; it is not an installed Laravel app.
        std::fs::write(
            root.join("artisan"),
            r#"<?php
if (($argv[1] ?? '') !== 'about') { fwrite(STDERR, 'Unsupported fixture command'); exit(2); }
file_put_contents(__DIR__.'/executed.marker', 'fixture executed');
echo json_encode(['environment'=>['php_version'=>PHP_VERSION,'application_name'=>'MCP fixture']]);
"#,
        )
        .unwrap();
        let events = Arc::new(Mutex::new(vec![]));
        let target = events.clone();
        let host = Arc::new(Host::new(HostConfig {
            config_dir: tmp.path().join("config"),
            data_dir: tmp.path().join("data"),
            local_data_dir: tmp.path().join("local"),
            languages: Arc::new(ide_language::LanguageRegistry::load(None).unwrap()),
            syntax: Arc::default(),
            sink_for: Box::new(move |_| {
                let target = target.clone();
                Arc::new(move |event| target.lock().unwrap().push(event))
            }),
            owner: |window| window,
            runtime: tokio::runtime::Handle::current(),
            project_environment: false,
        }));
        host.open_project("acceptance", &root).unwrap();
        host.set_project_trust(&root, true, false).await.unwrap();
        let plugins = Arc::new(
            PluginHost::new(
                tmp.path().join("plugins"),
                Arc::new(|_: Option<&str>, _: PluginEvent| {}),
                ide_plugin::Limits::default(),
            )
            .unwrap(),
        );
        plugins.set_workspace(Some(Arc::new(HostWorkspace(Arc::downgrade(&host)))));
        host.set_plugin_host(&plugins);
        // Storage granted; process execution deliberately absent initially.
        plugins
            .install_from_dir(
                package,
                &Consent {
                    optional: vec!["storage".into()],
                    signer_change: false,
                },
            )
            .unwrap();
        let owner = root.clone();
        let activated = plugins.clone();
        tokio::task::spawn_blocking(move || {
            activated.run_command(
                "aritti.laravel",
                "reindex",
                None,
                ide_plugin::Invocation {
                    client: Some("acceptance".into()),
                    workspace_root: Some(owner),
                    ..Default::default()
                },
            )
        })
        .await
        .unwrap()
        .unwrap();
        let server = McpServer::start(
            Arc::new(Registry::new(host.clone())),
            Some(discovery_path(&tmp.path().join("discovery"))),
            &tokio::runtime::Handle::current(),
        )
        .unwrap();
        Self {
            _tmp: tmp,
            root,
            host,
            plugins,
            server,
            http: reqwest::Client::new(),
            events,
        }
    }
    async fn rpc(&self, method: &str, params: Value) -> Value {
        let response = self
            .http
            .post(self.server.url(&self.root))
            .bearer_auth(self.server.token())
            .header("Accept", "application/json, text/event-stream")
            .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 200);
        response.json().await.unwrap()
    }
    async fn list(&self) -> Vec<Value> {
        let value = self.rpc("tools/list", json!({})).await;
        value["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|tool| {
                tool["name"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("aritti_laravel__")
            })
            .cloned()
            .collect()
    }
    async fn call(&self, name: &str, args: Value) -> Value {
        self.rpc(
            "tools/call",
            json!({"name":format!("aritti_laravel__{name}"),"arguments":args}),
        )
        .await
    }
    async fn confirmation(&self, allow: bool) {
        let id = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Some(id) = self.events.lock().unwrap().iter().rev().find_map(|event| {
                    if let IdeEvent::McpRequest { id, .. } = event {
                        Some(*id)
                    } else {
                        None
                    }
                }) {
                    break id;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("MCP confirmation request not received");
        assert!(
            self.host
                .mcp_requests
                .answer("other-window", id, true, None)
                .is_err()
        );
        self.host
            .mcp_requests
            .answer("acceptance", id, allow, None)
            .unwrap();
    }
    async fn confirmed_call(&self, allow: bool) -> Value {
        self.events.lock().unwrap().clear();
        let (reply, ()) = tokio::join!(
            self.call("runtime_refresh", json!({"section":"about"})),
            self.confirmation(allow)
        );
        reply
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.host.close_project("acceptance");
    }
}
fn text(reply: &Value) -> &str {
    reply["result"]["content"][0]["text"].as_str().unwrap_or("")
}
#[tokio::main]
async fn main() {
    assert_eq!(
        std::env::var("LARAVEL_RUN_MCP_ACCEPTANCE").ok().as_deref(),
        Some("1"),
        "Acceptance is opt-in: explicitly set LARAVEL_RUN_MCP_ACCEPTANCE=1"
    );
    let package = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let h = Harness::new(&package.canonicalize().unwrap()).await;
    let init=h.rpc("initialize",json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"Laravel acceptance","version":"1"}})).await;
    assert_eq!(init["result"]["serverInfo"]["name"], "OpenIDEA");
    let tools = h.list().await;
    assert_eq!(tools.len(), 7);
    for tool in &tools {
        assert_eq!(tool["inputSchema"]["type"], "object");
    }
    let inventory = h.call("inventory", json!({"resource":"views"})).await;
    assert!(text(&inventory).contains("welcome"));
    assert!(!h.root.join("executed.marker").exists());
    assert_eq!(
        h.call("inventory", json!({"resource":"views","limit":0}))
            .await["result"]["isError"],
        true
    );
    h.host
        .set_project_trust(&h.root, false, false)
        .await
        .unwrap();
    assert_eq!(h.list().await.len(), 4);
    assert!(
        h.call("runtime_refresh", json!({"section":"about"}))
            .await
            .get("error")
            .is_some()
    );
    h.host
        .set_project_trust(&h.root, true, false)
        .await
        .unwrap();
    // Missing UI, explicit denial and missing process grants must all leave the marker absent.
    assert_eq!(
        h.call("runtime_refresh", json!({"section":"about"})).await["result"]["isError"],
        true
    );
    h.host.mcp_requests.set_ready("acceptance", true);
    assert_eq!(h.confirmed_call(false).await["result"]["isError"], true);
    let denied = h.confirmed_call(true).await;
    assert_eq!(denied["result"]["isError"], true);
    assert!(text(&denied).contains("process:php"));
    assert!(!h.root.join("executed.marker").exists());
    h.plugins
        .set_capability("aritti.laravel", "process:php", true)
        .unwrap();
    let approved = h.confirmed_call(true).await;
    assert_ne!(approved["result"]["isError"], true, "{approved}");
    assert!(h.root.join("executed.marker").is_file());
    h.plugins.set_enabled("aritti.laravel", false).unwrap();
    assert!(h.list().await.is_empty());
    println!(
        "PASS real host MCP HTTP: discovery, seven namespaced schemas, schema rejection, Restricted Mode filtering, missing-window/denied/wrong-window confirmation, process grants and live disable withdrawal"
    );
    // No bearer token or discovery contents are printed or retained outside the temp profile.
}
