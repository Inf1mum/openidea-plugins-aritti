use super::*;
use openidea_plugin::text::offset;

const PROJECT: &str = "/work/.ddev/config.yaml";
fn document(path: &str, text: &str) -> Document {
    Document {
        path: path.into(),
        language: "yaml".into(),
        text: text.into(),
        version: 1,
    }
}
fn complete(path: &str, marked: &str) -> Vec<CompletionItem> {
    let cursor = marked.find('§').unwrap();
    let text = marked.replacen('§', "", 1);
    Main::default().completion(&document(path, &text), position(&text, cursor), None)
}
fn labels(marked: &str) -> Vec<String> {
    complete(PROJECT, marked)
        .into_iter()
        .map(|i| i.label)
        .collect()
}
fn diagnostics(text: &str) -> Vec<Diagnostic> {
    Main::default().diagnostics(&document(PROJECT, text))
}
fn apply(marked: &str, item: &CompletionItem) -> String {
    let mut text = marked.replace('§', "");
    let range = item.replace.unwrap();
    let start = offset(&text, range.start).unwrap();
    let end = offset(&text, range.end).unwrap();
    text.replace_range(start..end, &item.insert_text);
    text
}
#[test]
fn all_schema_root_keys_are_offered_without_existing_siblings() {
    let items = complete(PROJECT, "§");
    let props = schema::project().root["properties"].as_object().unwrap();
    assert_eq!(items.len(), props.len());
    for name in props.keys() {
        assert!(items.iter().any(|i| &i.label == name), "{name}");
    }
    assert!(!labels("name: demo\n§").contains(&"name".into()));
}
#[test]
fn nested_properties_and_referenced_hook_tasks_complete() {
    assert_eq!(labels("database:\n  ty§"), ["type"]);
    assert!(labels("hooks:\n  post-s§").contains(&"post-start".into()));
    assert!(labels("hooks:\n  post-start:\n    - ex§").contains(&"exec".into()));
    assert!(labels("web_extra_daemons:\n  - na§").contains(&"name".into()));
    assert!(labels("web_extra_exposed_ports:\n  - ht§").contains(&"http_port".into()));
}
#[test]
fn conditional_database_versions_follow_selected_engine() {
    let postgres = labels("database:\n  type: postgres\n  version: §");
    assert!(postgres.contains(&"17".into()), "{postgres:?}");
    assert!(!postgres.contains(&"10.11".into()));
    let maria = labels("database:\n  type: mariadb\n  version: §");
    assert!(maria.contains(&"10.11".into()));
    assert!(!maria.contains(&"17".into()));
    let mysql = labels("database:\n  type: mysql\n  version: §");
    assert!(mysql.contains(&"8.0".into()));
}
#[test]
fn inserts_numeric_strings_and_preserves_quotes() {
    for (marked, label, expected) in [
        ("php_version: 8.§", "8.3", "php_version: \"8.3\""),
        ("php_version: '8.§'", "8.3", "php_version: '8.3'"),
        ("php_version: \"8.§\"", "8.3", "php_version: \"8.3\""),
        ("nodejs_version: 2§", "24", "nodejs_version: \"24\""),
        ("xdebug_enabled: f§", "false", "xdebug_enabled: false"),
    ] {
        let items = complete(PROJECT, marked);
        let item = items
            .iter()
            .find(|i| i.label == label)
            .unwrap_or_else(|| panic!("{marked}: {items:?}"));
        assert_eq!(apply(marked, item), expected);
        assert!(diagnostics(expected).is_empty(), "{expected}");
    }
}
#[test]
fn empty_insert_ranges_preserve_following_newline_and_indentation() {
    let marked = "name: demo\n§\ntype: php\n";
    let items = complete(PROJECT, marked);
    let item = items.iter().find(|i| i.label == "docroot").unwrap();
    assert_eq!(apply(marked, item), "name: demo\ndocroot: \ntype: php\n");
    let marked = "database:\n  §\nname: demo\n";
    let items = complete(PROJECT, marked);
    assert_eq!(
        apply(marked, items.iter().find(|i| i.label == "type").unwrap()),
        "database:\n  type: \nname: demo\n"
    );
}
#[test]
fn flow_maps_and_lists_use_their_schema_path() {
    assert_eq!(labels("database: {ty§: mysql}"), ["type"]);
    assert!(labels("database: {type: mysql, version: 8§}").contains(&"8.0".into()));
    assert!(labels("hooks: {post-start: [{ex§: echo hi}]}").contains(&"exec".into()));
}
#[test]
fn comments_block_scalars_and_unrelated_files_do_not_complete() {
    for marked in [
        "# php_§",
        "name: demo # php_§",
        "hooks:\n  post-start:\n    - exec: |\n        php_§\n",
        "name: php_§",
    ] {
        assert!(labels(marked).is_empty(), "{marked}");
    }
    assert!(complete("/work/config.yaml", "php_§").is_empty());
    assert!(complete("/work/.ddev/docker-compose.yaml", "php_§").is_empty());
    assert!(!complete("C:\\work\\.ddev\\config.local.yaml", "php_§").is_empty());
}
#[test]
fn global_configuration_uses_its_own_schema() {
    let items = complete("/home/user/.ddev/global_config.yaml", "§");
    let props = schema::global().root["properties"].as_object().unwrap();
    assert_eq!(items.len(), props.len());
    assert!(items.iter().any(|i| i.label == "instrumentation_opt_in"));
    assert!(!items.iter().any(|i| i.label == "docroot"));
    assert!(
        Main::default()
            .diagnostics(&document(
                "/home/u/.ddev/global_config.yaml",
                "instrumentation_opt_in: false\n"
            ))
            .is_empty()
    );
}
#[test]
fn hover_documents_keys_values_and_nested_references() {
    for marked in [
        "php_ver§sion: '8.3'",
        "database:\n  type: post§gres",
        "hooks:\n  post-start:\n    - ex§ec: echo hi",
    ] {
        let cursor = marked.find('§').unwrap();
        let text = marked.replace('§', "");
        let hover = Main::default()
            .hover(&document(PROJECT, &text), position(&text, cursor))
            .unwrap();
        assert!(hover.markdown.contains("DDEV v1.25.5"));
        assert!(hover.markdown.contains("configuration reference"));
        let r = hover.range.unwrap();
        assert!(
            offset(&text, r.start).unwrap() <= cursor && offset(&text, r.end).unwrap() > cursor
        );
    }
    assert!(
        Main::default()
            .hover(
                &document("/work/other.yaml", "php_version: '8.3'"),
                Position::default()
            )
            .is_none()
    );
}
#[test]
fn validates_full_schema_constraints_and_conditional_versions() {
    for text in [
        "name: demo\ntype: php\nphp_version: '8.3'\ndatabase: {type: postgres, version: '17'}\n",
        "hooks:\n  post-start:\n    - exec: |\n        echo hi\n        echo bye\n",
        "nodejs_version: '22.14.0'\n",
    ] {
        assert!(
            diagnostics(text).is_empty(),
            "{text}: {:?}",
            diagnostics(text)
        );
    }
    for text in [
        "php_version: 8.3",
        "php_version: '99.0'",
        "xdebug_enabled: yes",
        "database: {type: postgres, version: '10.11'}",
        "hooks:\n  post-start:\n    - service: web\n",
        "web_extra_exposed_ports:\n  - name: test\n    http_port: wrong\n",
    ] {
        assert!(
            diagnostics(text)
                .iter()
                .any(|d| d.code.as_deref() == Some("ddev-schema")),
            "{text}"
        );
    }
}
#[test]
fn unknown_and_duplicate_keys_have_exact_source_ranges() {
    let text = "name: café🦀\nphp_versoin: '8.3'\nname: demo\n";
    let ds = diagnostics(text);
    let unknown = ds
        .iter()
        .find(|d| d.code.as_deref() == Some("ddev-unknown-property"))
        .unwrap();
    assert_eq!(
        unknown.range,
        Range::new(Position::new(1, 0), Position::new(1, 11))
    );
    let duplicate = ds
        .iter()
        .find(|d| d.code.as_deref() == Some("ddev-duplicate-key"))
        .unwrap();
    assert_eq!(
        duplicate.range,
        Range::new(Position::new(2, 0), Position::new(2, 4))
    );
    let marked = "name: café🦀\nphp_v§: '8.3'";
    let item = complete(PROJECT, marked).remove(0);
    assert_eq!(apply(marked, &item), "name: café🦀\nphp_version: '8.3'");
}
#[test]
fn yaml_aliases_and_merges_respect_explicit_precedence() {
    let text = "database: &db {type: postgres, version: '17'}\nother: {<<: *db, type: mysql}\nlist: [*db]\n";
    let tree = yaml::parse(text).unwrap();
    assert_eq!(tree.root.value["other"]["type"], "mysql");
    assert_eq!(tree.root.value["other"]["version"], "17");
    assert_eq!(tree.root.value["list"][0]["type"], "postgres");
    assert!(diagnostics("database: &db {type: postgres, version: '17'}\n").is_empty());
    assert!(yaml::parse("x: &x [*x]").is_err());
    assert!(yaml::parse("x: {<<: 3}").is_err());
}
#[test]
fn malformed_multiple_and_excessive_yaml_return_bounded_diagnostics() {
    for text in ["database: [", "a: 1\n---\nb: 2", "x: *missing"] {
        let ds = diagnostics(text);
        assert_eq!(ds.len(), 1, "{text}: {ds:?}");
        assert_eq!(ds[0].code.as_deref(), Some("ddev-yaml"));
    }
    let text = "x".repeat(yaml::MAX_BYTES + 1);
    assert_eq!(
        diagnostics(&text)[0].code.as_deref(),
        Some("ddev-config-size")
    );
    let mut text = String::from("a: &a [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]\n");
    let mut prev = 'a';
    for next in ['b', 'c', 'd', 'e', 'f'] {
        text.push_str(&format!("{next}: &{next} [*{prev}, *{prev}, *{prev}, *{prev}, *{prev}, *{prev}, *{prev}, *{prev}, *{prev}, *{prev}]\n"));
        prev = next;
    }
    assert!(yaml::parse(&text).is_err());
    assert!(yaml::parse(&format!("x: {}0{}", "[".repeat(70), "]".repeat(70))).is_err());
}

#[test]
fn incomplete_flow_collections_and_quoted_json_keys_complete() {
    assert!(labels("database: {ty§").contains(&"type".into()));
    assert!(labels("hooks: {post-start: [{ex§").contains(&"exec".into()));
    assert!(labels("database: {\"type\":\"post§\"}").contains(&"postgres".into()));
}

#[test]
fn standard_explicit_scalar_tags_respect_types() {
    let tree = yaml::parse(
        "a: !!str 8.3\nb: !!int '80'\nc: !!bool 'true'\nd: !!null null\ne: !!float '2.5'\n",
    )
    .unwrap();
    assert_eq!(
        tree.root.value,
        serde_json::json!({"a":"8.3", "b":80, "c":true, "d":null, "e":2.5})
    );
    assert!(yaml::parse("a: !!int wrong").is_err());
    assert!(diagnostics("php_version: !!str 8.3\nxdebug_enabled: !!bool 'true'\n").is_empty());
}

#[test]
fn completion_immediately_after_colon_keeps_yaml_valid() {
    for (marked, label, expected) in [
        ("php_version:§", "8.3", "php_version: \"8.3\""),
        ("php_version:§ '8.2'", "8.3", "php_version: \"8.3\""),
        ("xdebug_enabled:§", "true", "xdebug_enabled: true"),
        (
            "database: {type:§}",
            "postgres",
            "database: {type: postgres}",
        ),
    ] {
        let items = complete(PROJECT, marked);
        let item = items
            .iter()
            .find(|i| i.label == label)
            .unwrap_or_else(|| panic!("{marked}: {items:?}"));
        assert_eq!(apply(marked, item), expected);
        assert!(diagnostics(expected).is_empty());
    }
}

#[test]
fn diagnostic_count_is_bounded_even_with_many_duplicate_keys() {
    for entry in ["unknown: 1\n", "xdebug_enabled: nope\n"] {
        assert!(diagnostics(&entry.repeat(130)).len() <= 100);
    }
}

#[test]
fn partial_yaml_and_unicode_cursors_keep_all_provider_ranges_valid() {
    for sample in [
        "php_version: '8.3' # café🦀\n",
        "database: {\"type\":\"postgres\", version: '17'}\n",
        "hooks:\n  post-start:\n    - exec: |\n        echo 'café🦀'\n",
        "web_environment: &env ['A=🦀']\nadditional_hostnames: *env\n",
        "name: \"unclosed🦀\n",
    ] {
        let boundaries: Vec<_> = sample
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(sample.len()))
            .collect();
        for &end in &boundaries {
            let text = &sample[..end];
            let doc = document(PROJECT, text);
            for d in Main::default().diagnostics(&doc) {
                let start = offset(text, d.range.start).expect("diagnostic start");
                let end = offset(text, d.range.end).expect("diagnostic end");
                assert!(start <= end && end <= text.len());
            }
            for &cursor in boundaries.iter().take_while(|&&cursor| cursor <= end) {
                let at = position(text, cursor);
                for item in Main::default().completion(&doc, at, None) {
                    let r = item.replace.unwrap();
                    let start = offset(text, r.start).expect("completion start");
                    let end = offset(text, r.end).expect("completion end");
                    assert!(start <= cursor && cursor <= end && end <= text.len());
                }
                if let Some(h) = Main::default().hover(&doc, at) {
                    let r = h.range.unwrap();
                    assert!(offset(text, r.start).unwrap() <= cursor);
                    assert!(offset(text, r.end).unwrap() > cursor);
                }
            }
        }
    }
}

#[test]
fn current_project_schema_covers_new_properties_hooks_and_versions() {
    for property in ["dbimage", "nodejs_root", "skip_global_hooks"] {
        assert!(labels("§").iter().any(|label| label == property));
    }
    assert!(labels("hooks:\n  pre-des§").contains(&"pre-describe".into()));
    assert!(labels("hooks:\n  post-del§").contains(&"post-delete".into()));
    assert!(labels("php_version: 8.§").contains(&"8.6".into()));
    assert!(labels("database:\n  type: mysql\n  version: §").contains(&"9.7".into()));
    for project_type in ["maho", "modx"] {
        assert!(labels("type: §").contains(&project_type.into()));
    }
    let yaml = "name: demo\ntype: modx\nphp_version: '8.6'\nnodejs_root: frontend\ndbimage: custom/image:v1\nskip_global_hooks: true\ndatabase: {type: mysql, version: '9.7'}\nhooks:\n  pre-describe:\n    - exec-host: echo ready\n  post-delete:\n    - exec: echo done\n";
    assert!(diagnostics(yaml).is_empty(), "{:?}", diagnostics(yaml));
}

#[test]
fn current_global_schema_covers_hooks_and_agent_options_with_visible_empty_values() {
    let path = "/home/user/.ddev/global_config.yaml";
    let items = complete(path, "§");
    for property in ["hooks", "override_config", "ssh_agent_upstream"] {
        assert!(items.iter().any(|item| item.label == property));
    }
    assert!(
        complete(path, "hooks:\n  post-start:\n    - ex§")
            .iter()
            .any(|item| item.label == "exec")
    );
    let values = complete(path, "ssh_agent_upstream: §");
    assert!(values.iter().any(|item| item.label == "host"));
    let empty = values.iter().find(|item| item.label == "\"\"").unwrap();
    assert_eq!(empty.insert_text, "\"\"");
    assert!(values.iter().all(|item| !item.label.is_empty()));
    let yaml = "override_config: true\nssh_agent_upstream: host\nhooks:\n  post-start:\n    - exec: echo ready\n";
    assert!(
        Main::default()
            .diagnostics(&document(path, yaml))
            .is_empty()
    );
}

#[test]
fn multipass_windows_case_insensitive_config_paths_preserve_unix_scoping() {
    for path in [
        r"C:\WORK\.DDEV\CONFIG.YAML",
        r"\\server\share\.DDEV\CONFIG.LOCAL.YAML",
    ] {
        assert!(
            complete(path, "php_ver§")
                .iter()
                .any(|item| item.label == "php_version"),
            "{path}"
        );
        assert!(
            Main::default()
                .diagnostics(&document(path, "unknown_setting: true\n"))
                .iter()
                .any(|item| item.code.as_deref() == Some("ddev-unknown-property"))
        );
    }
    assert!(
        complete(r"C:\Users\User\.DDEV\GLOBAL_CONFIG.YAML", "ssh_agent_up§")
            .iter()
            .any(|item| item.label == "ssh_agent_upstream")
    );
    assert!(complete("/work/.DDEV/CONFIG.YAML", "php_ver§").is_empty());
    assert!(complete("/work/config.yaml", "php_ver§").is_empty());
}
