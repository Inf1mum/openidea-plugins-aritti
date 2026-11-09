//! Regression evidence for P17.2. These are parser/navigation contracts, not
//! rename anchors. Boot does not yet have a configuration-context owner model.

use super::*;

#[test]
fn yaml_full_path_is_not_the_replaceable_leaf_token() {
    let text = "app:\n  pool:\n    max-size: 8\n";
    let keys = config::yaml(text);
    let key = keys.iter().find(|key| key.key == "app.pool.max-size").unwrap();
    assert_eq!(&text[key.at..key.end], "max-size");
    assert_ne!(&text[key.at..key.end], key.key);
    let mut replaced = text.to_owned();
    replaced.replace_range(key.at..key.end, "app.pool.min-size");
    assert!(
        !config::yaml(&replaced).iter().any(|key| key.key == "app.pool.min-size"),
        "replacing a leaf with a full key changes its owner path"
    );
}

#[test]
fn properties_escapes_and_relaxed_keys_do_not_have_one_source_spelling() {
    let text = "app.pool\\-size=8\n";
    let keys = config::properties(text);
    let key = &keys[0];
    assert_eq!(key.key, "app.pool-size");
    assert_ne!(&text[key.at..key.end], key.key);
    assert_eq!(canonical("app.pool-size"), canonical("app.poolSize"));
    assert_ne!("app.pool-size", "app.poolSize");
}

#[test]
fn java_binding_name_is_derived_not_an_exact_identifier_anchor() {
    let text = "import org.springframework.boot.context.properties.ConfigurationProperties; @ConfigurationProperties(prefix=\"app\") class Pool { private int maxPoolSize; public int getMaxPoolSize() { return maxPoolSize; } }";
    let meta = meta::parse_java(text).unwrap();
    assert!(meta.props.iter().any(|property| property.name == "app.max-pool-size"));
    assert!(!text.contains("app.max-pool-size"));
    assert_eq!(meta::dashed("maxPoolSize"), "max-pool-size");
}

#[test]
fn profile_and_project_paths_cannot_be_discarded_from_owner_identity() {
    let text = "app.name=demo\n";
    let paths = [
        "/one/src/main/resources/application.properties",
        "/one/src/main/resources/application-prod.properties",
        "/two/src/main/resources/application.properties",
    ];
    for path in paths {
        assert!(is_config_file(path));
        assert_eq!(config::properties(text)[0].key, "app.name");
    }
    // Equal keys alone establish neither one effective profile context nor one
    // project owner. No global `config#app.name` anchor is published for them.
    assert_ne!(paths[0], paths[1]);
    assert_ne!(paths[0], paths[2]);
}

#[test]
fn exact_property_token_range_uses_utf16_but_still_needs_semantic_owner() {
    let text = "😀.label=value\r\n";
    let key = &config::properties(text)[0];
    let span = range(text, key.at, key.end);
    assert_eq!(span.start, Position::new(0, 0));
    assert_eq!(span.end, Position::new(0, 8));
    assert_eq!(&text[offset(text, span.start).unwrap()..offset(text, span.end).unwrap()], "😀.label");
    assert!(span.start != span.end);
}

#[test]
fn boot_does_not_register_automatic_index_rename_without_an_owner_model() {
    let manifest = include_str!("../plugin.toml");
    assert!(!manifest.contains("contributes.documentIndexers"));
    let source = include_str!("lib.rs");
    let registration = source.lines().find(|line| line.starts_with("register_plugin!(")).unwrap();
    assert!(!registration.contains("indexing"));
}
