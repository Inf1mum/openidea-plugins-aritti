use super::*;
use openidea_plugin::mock::MockHost;
fn definition() -> PluginDataSource {
    let name = "Cloud · Primary / production [schema_1]".to_owned();
    PluginDataSource {
        id: format!("laravel-{}", crate::artisan_metadata::hash(&name)),
        name,
        driver: "mysql".into(),
        host: Some("db.example.com".into()),
        port: Some(3306),
        database: Some("production".into()),
        user: Some("app".into()),
        url: None,
        options: vec![],
    }
}
#[test]
fn captured_cloud_registration_rejects_same_id_inserted_during_revalidation() {
    let h = MockHost::install();
    let state = State::default();
    let ctx = h.invocation();
    let project = ctx.project().unwrap();
    let definition = definition();
    h.answer_next_prompt(Some(&["yes"]));
    let result =
        state.register_external_source(&ctx, &definition, Some("private-password"), || {
            let mut other = definition.clone();
            other.host = Some("different.example.com".into());
            project.upsert_data_source(&other)
        });
    assert!(matches!(result, Err(Error::Stale)));
    assert_eq!(project.data_sources().unwrap(), vec![definition.id.clone()]);
    assert!(h.process_runs().is_empty());
    // The existing source is retained and a second attempt refuses to replace it.
    assert!(
        state
            .register_external_source(&ctx, &definition, None, || panic!(
                "must not revalidate an existing source"
            ))
            .is_err()
    );
    assert_eq!(project.data_sources().unwrap(), vec![definition.id]);
}
#[test]
fn captured_cloud_registration_enforces_the_manager_inventory_bound_before_review() {
    let h = MockHost::install();
    let state = State::default();
    let ctx = h.invocation();
    let project = ctx.project().unwrap();
    for n in 0..256 {
        project
            .upsert_data_source(&PluginDataSource {
                id: format!("laravel-{n:016x}"),
                name: format!("Existing {n}"),
                ..Default::default()
            })
            .unwrap();
    }
    assert!(
        state
            .register_external_source(&ctx, &definition(), None, || panic!(
                "limit must precede revalidation"
            ))
            .is_err()
    );
    assert_eq!(project.data_sources().unwrap().len(), 256);
    assert!(h.process_runs().is_empty());
}
#[test]
fn captured_cloud_registration_stops_when_project_changes_after_native_review() {
    let h = MockHost::install();
    h.add_project("/other");
    let ctx = h.invocation();
    let project = ctx.project().unwrap();
    let state = State::default();
    h.answer_next_prompt(Some(&["yes"]));
    assert!(matches!(
        state.register_external_source(&ctx, &definition(), None, || {
            h.set_project_root(Some("/other"));
            Ok(())
        }),
        Err(Error::Stale)
    ));
    assert!(project.data_sources().unwrap().is_empty());
}
