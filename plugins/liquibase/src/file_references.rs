//! Read-only physical resource navigation for explicitly relative core changes.
use crate::{changelog::Include, generate};
use openidea_plugin::providers::Document;
use spring_text::xml;

pub fn reference(doc: &Document, caret: usize) -> Option<Include> {
    let parent = generate::parent_log(&doc.path, &doc.text).ok()?;
    if parent.opaque { return None; }
    if !doc.path.ends_with(".xml") { return array_reference(doc, caret); }
    let elements = xml::elements(&doc.text);
    for (index, element) in elements.iter().enumerate() {
        let key = match element.local() {
            "sqlFile" => "path",
            "loadData" | "loadUpdateData" => "file",
            _ => continue,
        };
        if xml::namespace(&elements, index).as_deref() != Some(generate::NS) { continue; }
        let owner = element.parent.and_then(|i| elements.get(i))?;
        if owner.local() != "changeSet" || owner.parent != Some(0)
            || element.value("relativeToChangelogFile") != Some("true") { continue; }
        let Some(attribute) = element.attrs.iter().find(|a| a.name == key && a.holds(caret)) else { continue; };
        let path = &attribute.value;
        // No URI/search-path interpretation, expansion, encoded aliases or ambiguous spelling.
        if !literal_path(path) || doc.text.get(attribute.value_at..attribute.value_end) != Some(path) {
            return None;
        }
        return Some(Include { path: path.clone(), relative: true, dir: false, optional: false,
            at: attribute.value_at, end: attribute.value_end });
    }
    None
}

fn literal_path(path: &str) -> bool {
    !path.is_empty() && path.len() <= 4096 && path.trim() == path
        && !path.chars().any(char::is_control) && !path.contains([':', '\\', '$', '%', '#', '?']) && !path.starts_with('/')
}

fn array_reference(doc: &Document, caret: usize) -> Option<Include> {
    use yaml_rust2::{Yaml, YamlLoader};
    // parent_log has already rejected duplicate keys, tags, aliases, extra roots
    // and excess structure. Normalize JSON for YAML's narrower escape support.
    let normalized;
    let json = doc.path.ends_with(".json");
    let source = if json {
        normalized = serde_json::to_string(&crate::strict_json::parse(&doc.text,generate::MAX_TEXT).ok()?).ok()?;
        &normalized
    } else if doc.path.ends_with(".yaml") || doc.path.ends_with(".yml") { &doc.text } else { return None; };
    let docs = YamlLoader::load_from_str(source).ok()?;
    let items = docs.first()?["databaseChangeLog"].as_vec()?;
    let keys = if json { crate::json::keys(&doc.text) } else { spring_text::config::yaml(&doc.text) };
    // Select one caret scalar before walking semantic owners: scanning every key
    // for every resource is quadratic for a large changes array.
    let mut at_caret = keys.iter().filter(|k| k.value_at <= caret && caret < k.value_end);
    let span = at_caret.next()?;
    if at_caret.next().is_some() || span.value_at >= span.value_end { return None; }
    let raw_key = doc.text.get(span.at..span.end)?.trim_matches(['\'', '"']);
    if !matches!(raw_key, "path" | "file") { return None; }
    // A flattened scanner path alone is not structural authority. Refuse source
    // collisions even when another scanner entry does not hold this caret.
    if keys.iter().filter(|k| k.key==span.key).take(2).count()!=1 { return None; }
    for (set_index,item) in items.iter().enumerate() {
        let Some(changes) = item["changeSet"]["changes"].as_vec() else { continue; };
        for (change_index,change) in changes.iter().enumerate() {
            let Some(map) = change.as_hash().filter(|map| map.len()==1) else { continue; };
            let (kind,body) = map.iter().next()?;
            let Some(kind) = kind.as_str() else { continue; };
            let leaf = match kind { "sqlFile" => "path", "loadData" | "loadUpdateData" => "file", _ => continue };
            if body["relativeToChangelogFile"] != Yaml::Boolean(true) { continue; }
            let Some(path) = body[leaf].as_str().filter(|p| literal_path(p)) else { continue; };
            let spelling = format!("databaseChangeLog[{set_index}].changeSet.changes[{change_index}].{kind}.{leaf}");
            if spelling != span.key || raw_key != leaf
                || doc.text.get(span.value_at..span.value_end) != Some(path) { continue; }
            return Some(Include { path:path.into(), relative:true, dir:false, optional:false, at:span.value_at, end:span.value_end });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Liquibase;
    use openidea_plugin::{mock::MockHost, providers::Providers, text::position};
    fn document(body: &str) -> Document {
        Document { path: "/project/db/main.xml".into(), language: "xml".into(), version: 7,
            text: format!("<databaseChangeLog xmlns='{}'><!-- 😀 --><changeSet id='1' author='a'>{body}</changeSet></databaseChangeLog>", generate::NS) }
    }
    #[test]
    fn registered_include_fallback_refuses_opaque_or_malformed_xml_ownership() {
        let host=MockHost::install().with_capabilities(&["project.read"]);host.add_file("db/a.xml", "<databaseChangeLog/>");
        for text in [format!("<databaseChangeLog xmlns='{}' xml:base='elsewhere/'><include file='a.xml' relativeToChangelogFile='true'/></databaseChangeLog>",generate::NS),
            format!("<databaseChangeLog xmlns='{}'><include file='a.xml' file='b.xml' relativeToChangelogFile='true'/></databaseChangeLog>",generate::NS),
            format!("<databaseChangeLog xmlns='{}'><?xml version='1.0'?><include file='a.xml' relativeToChangelogFile='true'/></databaseChangeLog>",generate::NS)] {
            let doc=Document{path:"/project/db/main.xml".into(),language:"xml".into(),version:1,text};
            assert!(Liquibase::default().definition(&doc,position(&doc.text,doc.text.find("a.xml").unwrap()+1)).is_empty(),"{}",doc.text);
        }
    }
    #[test]
    fn registered_definition_navigates_only_explicit_relative_files_without_process_grants() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("db/main.xml", "old disk content");
        host.add_file("db/scripts/a.sql", "select 1;");
        host.add_file("db/data/a.csv", "id\n1\n");
        for (kind, key, path) in [("sqlFile", "path", "scripts/a.sql"), ("loadData", "file", "data/a.csv"), ("loadUpdateData", "file", "data/a.csv")] {
            let doc = document(&format!("<sqlFile path='other.sql' relativeToChangelogFile='true'/><{kind} {key}='{path}' relativeToChangelogFile='true'/>"));
            let found = Liquibase::default().definition(&doc, position(&doc.text, doc.text.find(path).unwrap()+2));
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].path, format!("/project/db/{path}"));
            assert!(host.process_runs().is_empty());
        }
    }
    #[test]
    fn unresolved_foreign_malformed_or_dynamic_resources_do_not_navigate() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("db/main.xml", ""); host.add_file("db/a.sql", "");
        for body in [
            "<sqlFile path='a.sql'/>", "<sqlFile path='a.sql' relativeToChangelogFile='false'/>",
            "<sqlFile path='${file}' relativeToChangelogFile='true'/>",
            "<sqlFile path='file:a.sql' relativeToChangelogFile='true'/>",
            "<sqlFile path='a&#46;sql' relativeToChangelogFile='true'/>",
            "<sqlFile path='../missing.sql' relativeToChangelogFile='true'/>",
            "<sqlFile xmlns='urn:foreign' path='a.sql' relativeToChangelogFile='true'/>",
            "<rollback><sqlFile path='a.sql' relativeToChangelogFile='true'/></rollback>",
        ] {
            let doc = document(body);
            let at = doc.text.find("path='").unwrap()+6;
            assert!(Liquibase::default().definition(&doc, position(&doc.text, at)).is_empty(), "{body}");
        }
        let mut doc=document("<sqlFile path='a.sql' relativeToChangelogFile='true'/>");
        doc.text.pop();
        assert!(reference(&doc,doc.text.find("a.sql").unwrap()).is_none());
    }
    #[test]
    fn resource_navigation_respects_project_denial_nested_owners_and_root_containment() {
        for caps in [vec![], vec!["project.read"]] {
            let host = MockHost::install().with_capabilities(&caps);
            host.add_file("db/main.xml", "");
            host.add_file("db/nested/a.sql", "");
            host.add_project("/project/db/nested");
            for path in ["nested/a.sql", "../../outside.sql"] {
                let doc = document(&format!("<sqlFile path='{path}' relativeToChangelogFile='true'/>"));
                assert!(Liquibase::default().definition(&doc,position(&doc.text,doc.text.find(path).unwrap())).is_empty());
            }
        }
    }
    #[test]
    fn array_resources_use_semantic_changes_and_exact_unescaped_scalar_ranges() {
        let host=MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("db/a.sql", "select 1;"); host.add_file("db/a.csv", "id\n1\n");
        for (extension,text,path) in [
            ("yaml", "databaseChangeLog:\n  - changeSet:\n      id: one\n      author: a\n      changes:\n        - sqlFile:\n            path: a.sql\n            relativeToChangelogFile: true\n", "a.sql"),
            ("yml", "databaseChangeLog:\n  - changeSet:\n      id: one\n      author: a\n      changes:\n        - loadData:\n            file: 'a.csv'\n            relativeToChangelogFile: true\n", "a.csv"),
            ("json", r#"{"databaseChangeLog":[{"changeSet":{"id":"one","author":"a","changes":[{"loadUpdateData":{"file":"a.csv","relativeToChangelogFile":true}}]}}]}"#, "a.csv"),
        ] {
            let doc=Document {path:format!("/project/db/main.{extension}"), language:extension.into(),text:text.into(),version:1};
            let found=Liquibase::default().definition(&doc,position(text,text.find(path).unwrap()+1));
            assert_eq!(found.len(),1,"{extension}"); assert_eq!(found[0].path,format!("/project/db/{path}"));
        }
    }
    #[test]
    fn array_resources_refuse_escaped_duplicate_aliased_and_non_changes_paths() {
        let _host=MockHost::install().with_capabilities(&["project.read"]);
        for (extension,text,needle) in [
            ("json", r#"{"databaseChangeLog":[{"changeSet":{"id":"one","author":"a","changes":[{"sqlFile":{"path":"a\u002esql","relativeToChangelogFile":true}}]}}]}"#, "a\\u"),
            ("json", r#"{"databaseChangeLog":[{"changeSet":{"changes":[{"sqlFile":{"path":"a.sql","path":"b.sql","relativeToChangelogFile":true}}]}}]}"#, "a.sql"),
            ("yaml", "databaseChangeLog:\n  - changeSet:\n      changes:\n        - sqlFile:\n            path: a.sql\n            relativeToChangelogFile: 'true'\n", "a.sql"),
            ("yaml", "databaseChangeLog:\n  - changeSet:\n      sqlFile:\n        path: a.sql\n        relativeToChangelogFile: true\n", "a.sql"),
            ("yaml", "databaseChangeLog:\n  - changeSet:\n      changes:\n        - sqlFile: &resource\n            path: a.sql\n            relativeToChangelogFile: true\n        - sqlFile: *resource\n", "a.sql"),
        ] {
            let doc=Document {path:format!("/project/db/main.{extension}"),language:extension.into(),text:text.into(),version:1};
            assert!(reference(&doc,text.find(needle).unwrap()).is_none(),"{text}");
        }
    }
    #[test]
    fn bounded_large_array_navigates_last_resource_without_earlier_targets() {
        let host=MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("db/last.sql", "select 1;");
        let resources=(0..1500).map(|n| serde_json::json!({"sqlFile":{"path":format!("{n}.sql"),"relativeToChangelogFile":true}}))
            .chain(std::iter::once(serde_json::json!({"sqlFile":{"path":"last.sql","relativeToChangelogFile":true}}))).collect::<Vec<_>>();
        let text=serde_json::json!({"databaseChangeLog":[{"changeSet":{"id":"1","author":"a","changes":resources}}]}).to_string();
        assert!(text.len()<generate::MAX_TEXT);
        let doc=Document {path:"/project/db/main.json".into(),language:"json".into(),text,version:1};
        let found=Liquibase::default().definition(&doc,position(&doc.text,doc.text.find("last.sql").unwrap()));
        assert_eq!(found.len(),1);assert_eq!(found[0].path,"/project/db/last.sql");
    }
}
