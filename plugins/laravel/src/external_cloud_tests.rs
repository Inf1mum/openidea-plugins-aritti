use super::*;
use openidea_plugin::{Event, HttpResponse, Plugin, mock::MockHost};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
fn environment() -> Value {
    json!({"data":{"id":"env_1","type":"environments","attributes":{"name":"Production","slug":"production","status":"deploying","vanity_domain":"example.laravel.cloud","php_major_version":"8.4","environment_variables":[{"key":"API_KEY","value":"private-cloud-variable"},{"key":"EMPTY","value":""}]},"relationships":{"application":{"data":{"id":"app_1","type":"applications"}}}}})
}
fn deployments() -> Value {
    json!({"data":[{"id":"deploy_1","type":"deployments","attributes":{"status":"build.running","branch_name":"main","started_at":"2026-11-11T10:00:00Z","finished_at":null,"failure_reason":"failed with private-cloud-variable"},"relationships":{"environment":{"data":{"id":"env_1","type":"environments"}}}}]})
}
fn organization() -> Value {
    json!({"data":{"id":"org_1","type":"organizations","attributes":{"slug":"our-org"}}})
}
fn log_data() -> Value {
    json!({"data":{"build":{"available":true,"steps":[{"step":"build","status":"success","description":"Build application","time":"1s","output":"token=private-cloud-variable\n{\"message\":\"done\"}"}]},"deploy":{"available":true,"steps":[{"step":"deploy","status":"running","description":"Deploy application","time":"2s","output":"Starting"}]}},"meta":{"deployment_status":"deployment.running"}})
}
fn clusters() -> Value {
    json!({"data":[{"id":"cluster_1","type":"databases","attributes":{"name":"Primary","status":"available","type":"laravel_mysql","connection":{"driver":"mysql","hostname":"db.example.com","port":3306,"username":"app","password":"private-db-password"}},"relationships":{"databases":{"data":[{"id":"schema_1","type":"databaseSchemas"}]}}}],"included":[{"id":"schema_1","type":"databaseSchemas","attributes":{"name":"production"},"relationships":{"database":{"data":{"id":"cluster_1","type":"databases"}}}}],"links":{"next":null}})
}
fn state() -> State {
    let mut state = State::default();
    state.projects.insert("/project".into(),Session{applications:vec![json!({"id":"app_1","name":"Application","slug":"our-app"})],environments:vec![json!({"id":"env_1","name":"Production","slug":"production","status":"deploying","application":"app_1"})],applications_page:Page{revision:3,..Default::default()},environments_page:Page{revision:4,..Default::default()},environment_application:Some("app_1".into()),..Default::default()});
    state.cloud_application_selected("/project");
    state
}
fn host() -> (MockHost, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let h = MockHost::install().with_capabilities(&[
        "project.read",
        "editor.read",
        "storage",
        "secrets",
        "net:cloud.laravel.com",
        "database.sources",
        "clipboard",
        "ide.commands",
    ]);
    h.set_secret(CLOUD_SECRET, "private-cloud-token");
    let calls = Arc::new(AtomicUsize::new(0));
    let logs = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let l = logs.clone();
    h.on_fetch(move|request|{c.fetch_add(1,Ordering::SeqCst);assert_eq!(request.method,"GET");assert!(request.headers.iter().any(|(key,value)|key=="Authorization"&&value=="Bearer private-cloud-token"));
        let value=match request.url.as_str(){
            "https://cloud.laravel.com/api/applications"=>json!({"data":[{"id":"app_1","type":"applications","attributes":{"name":"Application","slug":"our-app"}}],"links":{"next":null}}),
            "https://cloud.laravel.com/api/environments/env_1"=>environment(),
            "https://cloud.laravel.com/api/environments/env_1/deployments"=>deployments(),
            "https://cloud.laravel.com/api/meta/organization"=>organization(),
            "https://cloud.laravel.com/api/deployments/deploy_1/logs"=>{l.fetch_add(1,Ordering::SeqCst);log_data()},
            "https://cloud.laravel.com/api/applications/app_1/environments"=>json!({"data":[{"id":"env_1","type":"environments","attributes":{"name":"Production","slug":"production","status":"deploying"}}],"links":{"next":null}}),
            "https://cloud.laravel.com/api/databases/clusters?include=databases"=>clusters(),
            "https://cloud.laravel.com/api/databases/clusters/cluster_1?include=databases"=>{let mut value=clusters();value["data"]=value["data"][0].clone();value},
            other=>panic!("Unexpected Cloud path {other}"),
        };Ok(HttpResponse{status:200,headers:vec![],body:value.to_string().into_bytes()})
    });
    (h, calls, logs)
}
fn select(state: &mut State, h: &MockHost) {
    state
        .cloud_select_environment(
            &h.invocation(),
            &resource_key("cloud-env", "/project", 4, "env_1"),
        )
        .unwrap();
}
fn body(state: &State, h: &MockHost) -> Value {
    let Body::Other { data, .. } = state.cloud_body(&h.invocation()).unwrap() else {
        panic!()
    };
    serde_json::from_str(&data).unwrap()
}
#[test]
fn cloud_details_are_masked_read_only_and_never_fetch_during_render() {
    let (h, calls, _) = host();
    let mut state = state();
    select(&mut state, &h);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    let initial = body(&state, &h);
    assert!(!initial.to_string().contains("private-cloud-variable"));
    assert!(!initial.to_string().contains("private-cloud-token"));
    assert!(initial.to_string().contains("8.4"));
    assert!(initial.to_string().contains("main"));
    for row in initial["items"].as_array().unwrap() {
        assert_eq!(row["activationEnabled"], false);
        assert!(row["activationEnabled"].is_boolean());
    }
    assert_eq!(initial["backgroundPoll"]["intervalMs"], 5_000);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    let key = initial["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["cells"][1] == "API_KEY")
        .unwrap()["key"]
        .as_str()
        .unwrap()
        .to_owned();
    state
        .cloud_action(&h.invocation(), "cloud-variable-reveal", Some(&key))
        .unwrap();
    assert!(
        body(&state, &h)
            .to_string()
            .contains("private-cloud-variable")
    );
    state
        .cloud_action(&h.invocation(), "cloud-hide-variables", None)
        .unwrap();
    assert!(
        !body(&state, &h)
            .to_string()
            .contains("private-cloud-variable")
    );
    state
        .cloud_action(&h.invocation(), "cloud-variable-copy", Some(&key))
        .unwrap();
    assert_eq!(h.clipboard().as_deref(), Some("private-cloud-variable"));
    state
        .cloud_action(&h.invocation(), "cloud-selected-refresh", None)
        .unwrap();
    assert!(
        state
            .cloud_action(&h.invocation(), "cloud-variable-reveal", Some(&key))
            .is_err()
    );
    state
        .cloud_action(&h.invocation(), "cloud-open-domain", None)
        .unwrap();
    state
        .cloud_action(&h.invocation(), "cloud-manage", None)
        .unwrap();
    assert!(h.process_runs().is_empty());
    assert!(h.notifications().is_empty());
}
#[test]
fn cloud_pending_refresh_and_logs_are_throttled_and_stale_scopes_rejected() {
    let (h, calls, logs) = host();
    let mut state = state();
    select(&mut state, &h);
    let root = "/project";
    let first = 1_000_000;
    state.cloud.views.get_mut(root).unwrap().last_poll = first;
    state.cloud_logs_at(&h.invocation(), first).unwrap();
    assert_eq!(logs.load(Ordering::SeqCst), 1);
    assert!(
        !body(&state, &h)
            .to_string()
            .contains("private-cloud-variable")
    );
    let scope = state.cloud.views[root].scope(root);
    let tick = json!({"op":"cloud-tick","scope":scope}).to_string();
    let before = calls.load(Ordering::SeqCst);
    state
        .cloud_tick_at(&h.invocation(), &tick, first + 4_999)
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), before);
    state
        .cloud_tick_at(&h.invocation(), &tick, first + 5_000)
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), before + 4);
    assert_eq!(logs.load(Ordering::SeqCst), 1);
    let tick = json!({"op":"cloud-tick","scope":state.cloud.views[root].scope(root)}).to_string();
    state
        .cloud_tick_at(&h.invocation(), &tick, first + 30_000)
        .unwrap();
    assert_eq!(logs.load(Ordering::SeqCst), 2);
    state.cloud_dirty(root);
    let due = state.cloud.views[root].dirty_after.unwrap();
    assert!(state.cloud_tick_at(&h.invocation(), &tick, due).is_err());
    let dirty_tick =
        json!({"op":"cloud-tick","scope":state.cloud.views[root].scope(root)}).to_string();
    state
        .cloud_tick_at(&h.invocation(), &dirty_tick, due)
        .unwrap();
    assert!(
        state
            .cloud_tick_at(&h.invocation(), &tick, due + 5_000)
            .is_err()
    );
    host::secret_delete(CLOUD_SECRET).unwrap();
    let before = calls.load(Ordering::SeqCst);
    assert!(state.cloud_body(&h.invocation()).is_none());
    assert!(
        state
            .cloud_action(&h.invocation(), "cloud-logs", None)
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), before);
}
#[test]
fn cloud_selected_application_is_restored_only_after_a_fresh_authenticated_inventory() {
    let (h, calls, _) = host();
    let mut original = state();
    select(&mut original, &h);
    let mut restored = State::default();
    let before = calls.load(Ordering::SeqCst);
    restored.refresh_cloud(&h.invocation()).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), before + 2);
    assert_eq!(
        restored.projects["/project"]
            .environment_application
            .as_deref(),
        Some("app_1")
    );
    assert!(restored.cloud.views["/project"].snapshot.is_none());
    assert!(!format!("{:?}", h.storage()).contains("private-cloud-variable"));
    restored.disconnect_cloud().unwrap();
    assert!(restored.cloud.views.is_empty());
    assert!(
        !h.storage()
            .keys()
            .any(|key| key.starts_with("external:cloud-application:"))
    );
    assert!(!host::secret_is_set(CLOUD_SECRET));
}

#[test]
fn cloud_pending_statuses_match_the_official_api_and_pinned_finished_contract() {
    for status in [
        "pending",
        "build.pending",
        "build.created",
        "build.queued",
        "build.running",
        "build.succeeded",
        "deployment.pending",
        "deployment.created",
        "deployment.queued",
        "deployment.running",
    ] {
        assert!(pending(status), "{status}");
    }
    for status in [
        "cancelled",
        "failed",
        "build.failed",
        "deployment.succeeded",
        "deployment.failed",
        "unknown",
        "running",
        "hibernating",
        "stopped",
    ] {
        assert!(!pending(status), "{status}");
    }
}
#[test]
fn cloud_variables_and_relationships_require_explicit_valid_evidence() {
    assert_eq!(
        environment_url("example.laravel.cloud"),
        Some("https://example.laravel.cloud".into())
    );
    for invalid in [
        "javascript:alert(1)",
        "https://user:password@example.com",
        "http://example.com",
        "example.com/path",
        "example.com?key=secret",
    ] {
        assert!(environment_url(invalid).is_none());
    }
    assert!(
        parse_environment(
            &environment(),
            &deployments(),
            &organization(),
            "env_1",
            "app_1"
        )
        .unwrap()
        .date
        .is_empty()
    );
    let mut completed = deployments();
    completed["data"][0]["attributes"]["finished_at"] = json!("2026-11-11T10:01:00Z");
    assert_eq!(
        parse_environment(
            &environment(),
            &completed,
            &organization(),
            "env_1",
            "app_1"
        )
        .unwrap()
        .date,
        "2026-11-11T10:01:00Z"
    );
    let mut value = environment();
    value["data"]["attributes"]
        .as_object_mut()
        .unwrap()
        .remove("environment_variables");
    assert!(parse_environment(&value, &deployments(), &organization(), "env_1", "app_1").is_err());
    value = environment();
    value["data"]["attributes"]["environment_variables"] = Value::Null;
    assert!(parse_environment(&value, &deployments(), &organization(), "env_1", "app_1").is_err());
    value = environment();
    value["data"]["attributes"]["environment_variables"] = json!([]);
    assert!(
        parse_environment(&value, &deployments(), &organization(), "env_1", "app_1")
            .unwrap()
            .variables
            .is_empty()
    );
    value["data"]["relationships"]["application"]["data"]["id"] = json!("other_app");
    assert!(parse_environment(&value, &deployments(), &organization(), "env_1", "app_1").is_err());
    assert_eq!(
        redact(
            "abab",
            &[("A".into(), "ab".into()), ("B".into(), "redacted".into())]
        ),
        "[redacted][redacted]"
    );
    assert!(
        database_next(&json!({"links":{"next":"https://evil.test/api/databases/clusters?page=2"}}))
            .is_err()
    );
    assert_eq!(database_next(&json!({"links":{"next":"https://cloud.laravel.com/api/databases/clusters?include=databases&page=2"}})).unwrap(),Some(format!("{API}/databases/clusters?include=databases&page=2")));
    let mut db = clusters();
    db["included"][0]["relationships"]["database"]["data"]["id"] = json!("foreign_cluster");
    assert!(database_candidates(&db).is_err());
}
#[test]
fn cloud_log_unavailability_is_explicit_and_missing_availability_is_not_empty_success() {
    let missing = json!({"data":{"build":{},"deploy":{"available":false}}});
    assert!(logs(&missing, &[]).is_err());
    let unavailable =
        json!({"data":{"build":{"available":false},"deploy":{"available":true,"steps":[]}}});
    let rows = logs(&unavailable, &[]).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row["available"] == false));
}
#[test]
fn cloud_database_parser_rejects_duplicate_relations_and_credential_amplification() {
    let mut value = clusters();
    value["data"][0]["relationships"]["databases"]["data"] = json!([{"id":"schema_1","type":"databaseSchemas"},{"id":"schema_1","type":"databaseSchemas"}]);
    assert!(database_candidates(&value).is_err());
    value = clusters();
    value["data"][0]["attributes"]["connection"]["password"] = json!("x".repeat(64 * 1024));
    value["included"]=json!((0..33).map(|i|json!({"id":format!("schema_{i}"),"type":"databaseSchemas","attributes":{"name":format!("db{i}")},"relationships":{"database":{"data":{"id":"cluster_1","type":"databases"}}}})).collect::<Vec<_>>());
    value["data"][0]["relationships"]["databases"]["data"] = json!(
        (0..33)
            .map(|i| json!({"id":format!("schema_{i}"),"type":"databaseSchemas"}))
            .collect::<Vec<_>>()
    );
    assert!(database_candidates(&value).is_err());
}

#[test]
fn cloud_database_permission_is_checked_before_fetching_connection_credentials() {
    let (h, calls, _) = host();
    let mut state = state();
    select(&mut state, &h);
    let h = h.with_capabilities(&["project.read", "secrets", "net:cloud.laravel.com"]);
    let before = calls.load(Ordering::SeqCst);
    assert!(
        state
            .cloud_import_database(
                &h.invocation(),
                &crate::runtime_inspection::State::default()
            )
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), before);
}
#[test]
fn cloud_database_import_is_reviewed_bound_to_schema_and_never_connects_or_runs_php() {
    let (h, _, _) = host();
    let mut state = state();
    select(&mut state, &h);
    let runtime = crate::runtime_inspection::State::default();
    h.answer_next_prompt(Some(&["database:0"]));
    h.answer_next_prompt(None);
    state
        .cloud_import_database(&h.invocation(), &runtime)
        .unwrap();
    assert!(
        h.invocation()
            .project()
            .unwrap()
            .data_sources()
            .unwrap()
            .is_empty()
    );
    h.answer_next_prompt(Some(&["database:0"]));
    h.answer_next_prompt(Some(&["yes"]));
    state
        .cloud_import_database(&h.invocation(), &runtime)
        .unwrap();
    let sources = h.invocation().project().unwrap().data_sources().unwrap();
    assert_eq!(sources.len(), 1);
    assert!(sources[0].starts_with("laravel-"));
    assert!(h.process_runs().is_empty());
    assert!(h.view_messages().is_empty());
    assert!(!format!("{:?}", h.notifications()).contains("private-db-password"));
}
#[test]
fn main_cloud_opaque_bridge_is_project_frame_scoped_and_preserves_masked_native_rows() {
    use openidea_plugin::views::{ViewEvent, Views};
    let (h, calls, _) = host();
    let mut main = crate::Main::default();
    main.external_services = state();
    select(&mut main.external_services, &h);
    let ctx = h.invocation();
    let config = main.web_config(&ctx);
    ui_kit::web::reveal_view("laravel-external", &config).unwrap();
    let frame = "0123456789abcdef0123456789abcdef";
    let render = json!({"channel":"ui-kit","op":"render","sequence":1,"frameId":frame,"project":"/project","targetView":"laravel-external"});
    main.handle(crate::VIEW, ViewEvent::Message(render.to_string()), &ctx)
        .unwrap();
    let messages = h.view_messages();
    let reply = messages
        .iter()
        .filter_map(|(_, message)| serde_json::from_str::<Value>(message).ok())
        .find(|message| message["model"]["activeView"] == "laravel-external")
        .unwrap();
    assert_eq!(reply["frameId"], frame);
    assert!(!reply.to_string().contains("private-cloud-variable"));
    if let Ok(path) = std::env::var("LARAVEL_CLOUD_UI_FIXTURE") {
        std::fs::write(path, serde_json::to_vec_pretty(&reply["model"]).unwrap()).unwrap();
    }
    assert_eq!(
        reply["model"]["body"]["backgroundPoll"]["intervalMs"],
        5_000
    );
    for row in reply["model"]["body"]["items"].as_array().unwrap() {
        assert!(row["activationEnabled"].is_boolean());
        assert_eq!(row["activationEnabled"], false);
    }
    main.external_services
        .cloud
        .views
        .get_mut("/project")
        .unwrap()
        .last_poll = 0;
    let scope = main.external_services.cloud.views["/project"].scope("/project");
    let message = json!({"op":"cloud-tick","scope":scope,"_backgroundPoll":"cloud-fixture-nonce"})
        .to_string();
    let before = calls.load(Ordering::SeqCst);
    let wrong = json!({"channel":"ui-kit","op":"opaque-message","targetView":"laravel-external","message":message,"sequence":2,"frameId":frame,"project":"/foreign"});
    main.handle(crate::VIEW, ViewEvent::Message(wrong.to_string()), &ctx)
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), before);
    let tick = json!({"channel":"ui-kit","op":"opaque-message","targetView":"laravel-external","message":message,"sequence":3,"frameId":frame,"project":"/project"});
    main.handle(crate::VIEW, ViewEvent::Message(tick.to_string()), &ctx)
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), before + 4);
    let messages = h.view_messages();
    assert!(messages.iter().any(|(_, message)| {
        serde_json::from_str::<Value>(message)
            .ok()
            .is_some_and(|envelope| {
                envelope["channel"] == "ui-kit-view-message"
                    && envelope["frameId"] == frame
                    && envelope["targetView"] == "laravel-external"
                    && envelope["message"]
                        .as_str()
                        .is_some_and(|message| message.contains("cloud-fixture-nonce"))
            })
    }));
    let before = calls.load(Ordering::SeqCst);
    main.handle_events(&[Event::VcsChanged("/project".into())], &ctx);
    assert_eq!(calls.load(Ordering::SeqCst), before);
    assert!(
        main.external_services.cloud.views["/project"]
            .dirty_after
            .is_some()
    );
    let change = json!({"channel":"ui-kit","op":"select-view","target":"laravel-about","targetView":"laravel-external","frameId":frame,"sequence":4,"project":"/project"});
    main.handle(crate::VIEW, ViewEvent::Message(change.to_string()), &ctx)
        .unwrap();
    let hidden = json!({"channel":"ui-kit","op":"opaque-message","targetView":"laravel-external","message":message,"frameId":frame,"sequence":5,"project":"/project"});
    main.handle(crate::VIEW, ViewEvent::Message(hidden.to_string()), &ctx)
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), before);
    assert!(h.process_runs().is_empty());
}
