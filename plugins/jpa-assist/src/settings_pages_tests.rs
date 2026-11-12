//! The settings pages of plugin.toml against settings.schema.json (the source of every key).
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;

fn read(path: &str) -> Value {
    let full = format!("{}/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(&full).unwrap_or_else(|_| panic!("{full}"))).unwrap_or_else(|_| panic!("{full} is not JSON"))
}

fn page_files() -> Vec<String> {
    let manifest = fs::read_to_string(format!("{}/plugin.toml", env!("CARGO_MANIFEST_DIR"))).unwrap();
    manifest.lines().filter_map(|line| line.strip_prefix("schema = \"settings/")).map(|rest| format!("settings/{}", rest.trim_end_matches('"'))).collect()
}

#[test]
fn every_schema_key_is_on_exactly_one_page_with_the_same_definition() {
    let schema = read("settings.schema.json");
    let source = schema["properties"].as_object().unwrap();
    let mut seen = BTreeSet::new();
    let files = page_files();
    assert!(files.len() >= 5, "{files:?}");
    for file in &files {
        let page = read(file);
        for (key, definition) in page["properties"].as_object().unwrap() {
            assert!(seen.insert(key.clone()), "{key} is on two pages");
            let mut without_group = definition.clone();
            let group = without_group.as_object_mut().unwrap().remove("group");
            assert!(group.is_some_and(|g| g.as_str().is_some_and(|g| !g.is_empty())), "{key} has no group");
            assert_eq!(Some(&without_group), source.get(key), "{key} differs from settings.schema.json (run scripts/split-jpa-settings.py)");
        }
    }
    let expected: BTreeSet<String> = source.keys().cloned().collect();
    assert_eq!(seen, expected, "keys missing from the pages");
    assert!(source.len() <= 200, "the host accepts 200 settings");
}

#[test]
fn titles_fit_the_validator_and_inspections_live_on_their_own_page() {
    let manifest = fs::read_to_string(format!("{}/plugin.toml", env!("CARGO_MANIFEST_DIR"))).unwrap();
    for file in page_files() {
        let page = read(&file);
        for (key, definition) in page["properties"].as_object().unwrap() {
            assert!(definition["title"].as_str().unwrap().len() <= 256, "{key}");
        }
        if file.ends_with("inspections.json") {
            let keys: Vec<&String> = page["properties"].as_object().unwrap().keys().collect();
            assert!(keys.iter().any(|k| *k == "jpaAssist.inspectionSeverities") && keys.iter().filter(|k| k.starts_with("jpaAssist.check")).count() > 20);
        }
        if file.ends_with("designer.json") {
            for key in ["showPalette", "showInspector", "minimalMode", "showEditorToolbar"] {
                assert!(page["properties"].get(format!("jpaAssist.designer.{key}")).is_some(), "{key}");
            }
        }
    }
    assert!(manifest.contains("command = \"reload-naming-strategy\"") && manifest.contains("id = \"reload-naming-strategy\""));
    assert!(!manifest.contains("schema = \"settings.schema.json\""));
}

#[test]
fn sql_to_java_mapping_rows_cover_the_eight_databases_and_three_targets() {
    let schema = read("settings.schema.json");
    let row = &schema["properties"]["sqlJavaTypeMappings"]["items"]["properties"];
    let names = |value: &Value| value["enum"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_owned()).collect::<Vec<_>>();
    assert_eq!(names(&row["dbms"]), ["", "h2", "hsqldb", "db2", "mariadb", "sqlserver", "mysql", "oracle", "postgresql"]);
    assert_eq!(names(&row["target"]), ["javaType", "converter", "hibernateType"]);
}
