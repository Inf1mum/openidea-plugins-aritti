//! Include/changeset/table associations are not a closed automatic Rename set.
//! Tables can be referenced by SQL, other changelogs, schemas and runtime mappings;
//! path-scoped lexical equality does not prove ownership. Keep the export empty to
//! clear previous unsafe graph entries; navigation remains in dedicated providers.
use crate::Liquibase;
use openidea_plugin::indexing::{Document, DocumentIndexer, Edges};
impl DocumentIndexer for Liquibase {
    fn index(&mut self, _doc: &Document) -> Edges {
        Edges::default()
    }
}
#[cfg(test)]
mod lexical_receipts {
    // Historical lexical range receipts below are not published as symbols.
    //! The host index also powers Rename: publish only exact, source-owned tokens.
    //! File names have no replaceable declaration in a changelog; changeset identity
    //! needs logicalFilePath + author ownership not represented by this model. Those
    //! groups deliberately stay out of the automatic graph (include providers remain).
    use crate::changelog::{self, Format};
    use openidea_plugin::indexing::{Anchor, Document, Edges};
    use openidea_plugin::{Position, Range};
    use spring_text::{config, xml};
    use std::collections::HashSet;

    const MAX_ANCHORS: usize = 4096; // Host limit is definitions + references together.
    const MAX_TEXT: usize = 512 << 10;
    const TABLE_ATTRS: [&str; 3] = ["tableName", "baseTableName", "referencedTableName"];

    fn qualifier(name: &str) -> bool {
        matches!(name, "schemaName" | "catalogName") || name.ends_with("SchemaName") || name.ends_with("CatalogName")
    }
    fn literal_name(name: &str) -> bool {
        let mut chars = name.bytes();
        chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == b'_') && chars.all(|c| c.is_ascii_alphanumeric() || c == b'_')
    }
    fn table_key(path: &str, name: &str) -> String {
        // Length prefixes avoid ambiguous path/name concatenation; spelling is exact,
        // never lowercased. A table in another source file is a different owner.
        format!("lb:table:{}:{path}:{name}", path.len())
    }
    // Precompute UTF-16 columns once; SDK position() rescans the whole prefix for
    // each anchor, which makes large same-line changelogs quadratic.
    struct Coordinates {
        lines: Vec<usize>,
        columns: Vec<u32>,
    }
    impl Coordinates {
        fn new(text: &str) -> Self {
            let mut lines = vec![0];
            let mut columns = vec![0; text.len() + 1];
            let mut column = 0;
            for (at, c) in text.char_indices() {
                columns[at] = column;
                let end = at + c.len_utf8();
                if c == '\n' {
                    lines.push(end);
                    column = 0;
                } else {
                    column += c.len_utf16() as u32;
                }
                columns[end] = column;
            }
            Self { lines, columns }
        }
        fn position(&self, at: usize) -> Position {
            Position::new((self.lines.partition_point(|start| *start <= at) - 1) as u32, self.columns[at])
        }
        fn range(&self, from: usize, to: usize) -> Range {
            Range::new(self.position(from), self.position(to))
        }
    }
    #[derive(Default)]
    struct Found {
        definitions: Vec<Anchor>,
        references: Vec<Anchor>,
    }
    impl Found {
        fn push(&mut self, coordinates: &Coordinates, definition: bool, path: &str, text: &str, name: &str, from: usize, to: usize) {
            if self.definitions.len() + self.references.len() >= MAX_ANCHORS
                || !literal_name(name)
                || from >= to
                || text.get(from..to) != Some(name)
            {
                return;
            }
            let key = table_key(path, name);
            if key.len() > 1024 {
                return;
            }
            let anchor = Anchor { key, range: coordinates.range(from, to) };
            if definition { self.definitions.push(anchor) } else { self.references.push(anchor) }
        }
    }
    fn collect(path: &str, text: &str, format: Format) -> Found {
        let mut found = Found::default();
        let coordinates = Coordinates::new(text);
        match format {
            Format::Xml => {
                let elements = xml::elements(text);
                // Without full schema/catalog ownership, a qualified document is
                // unknown rather than a global simple-name Rename graph.
                if elements.iter().any(|e| e.attrs.iter().any(|a| qualifier(a.local()))) {
                    return found;
                }
                for e in elements {
                    for a in e.attrs.iter().filter(|a| TABLE_ATTRS.contains(&a.local())) {
                        found.push(
                            &coordinates,
                            e.local() == "createTable" && a.local() == "tableName",
                            path,
                            text,
                            &a.value,
                            a.value_at,
                            a.value_end,
                        );
                    }
                }
            }
            Format::Yaml | Format::Json => {
                let keys = if format == Format::Yaml { config::yaml(text) } else { crate::json::keys(text) };
                if keys.iter().any(|k| qualifier(k.key.rsplit('.').next().unwrap_or(""))) {
                    return found;
                }
                for k in &keys {
                    let Some((parent, name)) = k.key.rsplit_once('.') else { continue };
                    if !TABLE_ATTRS.contains(&name) {
                        continue;
                    }
                    if format == Format::Json
                        && (k.value_at == 0
                            || text.as_bytes().get(k.value_at - 1) != Some(&b'"')
                            || text.as_bytes().get(k.value_end) != Some(&b'"'))
                    {
                        continue;
                    }
                    found.push(
                        &coordinates,
                        parent.ends_with("createTable") && name == "tableName",
                        path,
                        text,
                        &k.value,
                        k.value_at,
                        k.value_end,
                    );
                }
            }
            Format::Sql => {} // No precise source-owned table/changeSet model here.
        }
        found
    }
    fn lexical_edges(doc: &Document) -> Edges {
        if doc.text.len() > MAX_TEXT {
            return Edges::default();
        }
        let Some(log) = changelog::parse(&doc.path, &doc.text) else { return Edges::default() };
        let mut found = collect(&doc.path, &doc.text, log.format);
        let owned: HashSet<_> = found.definitions.iter().map(|a| a.key.as_str()).collect();
        found.references.retain(|a| owned.contains(a.key.as_str()));
        Edges { definitions: found.definitions, references: found.references, aliases: Vec::new(), markers: Vec::new() }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        fn index(path: &str, text: &str) -> Edges {
            lexical_edges(&Document { path: path.into(), text: text.into(), symbols: Vec::new() })
        }
        // Mirrors pinned v0.1.39 navigation.rs::valid_range and anchor/key budget.
        fn contract(text: &str, edges: &Edges) {
            let lines: Vec<_> = text.split('\n').map(|l| l.encode_utf16().count()).collect();
            assert!(edges.definitions.len() + edges.references.len() <= 4096);
            let mut boundaries = HashSet::new();
            let mut line = 0;
            let mut column = 0;
            boundaries.insert((line, column));
            for c in text.chars() {
                if c == '\n' {
                    line += 1;
                    column = 0;
                } else {
                    column += c.len_utf16() as u32;
                }
                boundaries.insert((line, column));
            }
            for a in edges.definitions.iter().chain(&edges.references).chain(&edges.markers) {
                assert!(!a.key.is_empty() && a.key.len() <= 1024, "{}", a.key);
                let start = (a.range.start.line, a.range.start.character);
                let end = (a.range.end.line, a.range.end.character);
                assert!(start < end, "{} {:?}", a.key, a.range);
                for p in [a.range.start, a.range.end] {
                    assert!(lines.get(p.line as usize).is_some_and(|n| p.character as usize <= *n));
                    assert!(boundaries.contains(&(p.line, p.character)));
                }
            }
        }
        const MASTER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\">\n    <include file=\"changelog-1.xml\" relativeToChangelogFile=\"true\"/>\n</databaseChangeLog>\n";
        const CHILD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<databaseChangeLog\n        xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\"\n        xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"\n        xsi:schemaLocation=\"http://www.liquibase.org/xml/ns/dbchangelog\n                            https://www.liquibase.org/xml/ns/dbchangelog/dbchangelog-latest.xsd\">\n\n    <changeSet id=\"1\" author=\"fixture\">\n        \n    </changeSet>\n</databaseChangeLog>\n";
        #[test]
        fn native_acceptance_changelogs_do_not_emit_synthetic_zero_length_anchors() {
            for (path, text) in [
                ("/fixture/src/main/resources/db/changelog/db.changelog-master.xml", MASTER),
                ("/fixture/src/main/resources/db/changelog/changelog-1.xml", CHILD),
            ] {
                let e = index(path, text);
                contract(text, &e);
                assert!(e.definitions.is_empty() && e.references.is_empty());
            }
        }
        #[test]
        fn every_supported_format_satisfies_nonempty_utf16_anchor_contract() {
            for (path, text) in [
                (
                    "/p/x.xml",
                    "<databaseChangeLog><!-- 😀é --><changeSet id='1' author='ann'><createTable tableName='Orders'/><dropTable tableName='Orders'/><rollback changeSetId='' changeSetAuthor='ann'/></changeSet></databaseChangeLog>",
                ),
                (
                    "/p/x.yaml",
                    "databaseChangeLog:\n  - changeSet:\n      id: 1\n      author: ann\n      changes:\n        - createTable:\n            tableName: Orders\n        - dropTable:\n            tableName: Orders\n",
                ),
                (
                    "/p/x.json",
                    r#"{"databaseChangeLog":[{"changeSet":{"id":"1","author":"ann","changes":[{"createTable":{"tableName":"Orders"}},{"dropTable":{"tableName":"Orders"}}]}}]}"#,
                ),
                ("/p/x.sql", "--liquibase formatted sql\n--changeset ann:1\nCREATE TABLE orders(id INT);\n"),
            ] {
                let e = index(path, text);
                contract(text, &e);
                assert!(e.definitions.iter().chain(&e.references).all(|a| a.key.starts_with("lb:table:")));
                if !path.ends_with("sql") {
                    assert_eq!(e.definitions.len(), 1);
                    assert_eq!(e.references.len(), 1);
                }
            }
        }
        #[test]
        fn rename_tokens_are_exact_names_not_headers_paths_ids_or_whole_tags() {
            let text = "<databaseChangeLog><!-- 😀é -->\r\n<include file='other.xml'/><changeSet id='1' author='ann'><createTable tableName='orders'/><dropTable tableName='orders'/></changeSet></databaseChangeLog>";
            let e = index("/p/a.xml", text);
            contract(text, &e);
            let mut spans: Vec<_> = e
                .definitions
                .iter()
                .chain(&e.references)
                .map(|a| {
                    let from = openidea_plugin::text::offset(text, a.range.start).unwrap();
                    let to = openidea_plugin::text::offset(text, a.range.end).unwrap();
                    assert_eq!(&text[from..to], "orders");
                    (from, to)
                })
                .collect();
            spans.sort_by_key(|(from, _)| std::cmp::Reverse(*from));
            let mut renamed = text.to_owned();
            for (from, to) in spans {
                renamed.replace_range(from..to, "customers");
            }
            assert_eq!(renamed, text.replace("tableName='orders'", "tableName='customers'"));
            let other = index("/p/b.xml", text);
            assert_ne!(e.definitions[0].key, other.definitions[0].key);
        }
        #[test]
        fn unknown_qualified_dynamic_escaped_or_unowned_names_are_suppressed() {
            for body in [
                "<createTable tableName='orders' schemaName='a'/><dropTable tableName='orders' schemaName='b'/>",
                "<createTable tableName='${table}'/>",
                "<createTable tableName='a&amp;b'/>",
                "<dropTable tableName='external'/>",
                "<createTable tableName='Orders'/><dropTable tableName='orders'/>",
            ] {
                let text = format!("<databaseChangeLog>{body}</databaseChangeLog>");
                let e = index("/p/x.xml", &text);
                contract(&text, &e);
                assert!(e.references.is_empty());
                if !body.contains("Orders") {
                    assert!(e.definitions.is_empty());
                }
            }
            for text in [
                r#"{"databaseChangeLog":[{"createTable":{"tableName":123}}]}"#,
                r#"{"databaseChangeLog":[{"createTable":{"tableName":"ord\u0065rs"}}]}"#,
            ] {
                let e = index("/p/x.json", text);
                contract(text, &e);
                assert!(e.definitions.is_empty());
            }
        }
        #[test]
        fn malformed_empty_oversized_key_and_combined_budget_outputs_are_safe() {
            for text in [
                "",
                " ",
                "<databaseChangeLog>",
                "<databaseChangeLog><changeSet id='' author=''/><rollback changeSetId='' changeSetAuthor=''/></databaseChangeLog>",
            ] {
                let e = index("/p/x.xml", text);
                contract(text, &e);
            }
            let text = format!("<databaseChangeLog><createTable tableName='{}'/></databaseChangeLog>", "a".repeat(1025));
            let e = index("/p/x.xml", &text);
            contract(&text, &e);
            assert!(e.definitions.is_empty());
            let body = (0..2500).map(|n| format!("<createTable tableName='t{n}'/><dropTable tableName='t{n}'/>")).collect::<String>();
            let text = format!("<databaseChangeLog>{body}</databaseChangeLog>");
            let e = index("/p/x.xml", &text);
            contract(&text, &e);
            assert_eq!(e.definitions.len() + e.references.len(), 4096);
            assert!(index("/p/x.xml", &"x".repeat(MAX_TEXT + 1)).definitions.is_empty());
        }
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    #[test]
    fn table_source_tokens_do_not_prove_external_reference_completeness() {
        let text = r#"<databaseChangeLog><changeSet id="1" author="a"><createTable tableName="orders"/><dropTable tableName="orders"/></changeSet></databaseChangeLog>"#;
        let edges = Liquibase::default().index(&Document { path: "/project/a.xml".into(), text: text.into(), symbols: vec![] });
        assert!(edges.definitions.is_empty() && edges.references.is_empty() && edges.aliases.is_empty());
    }
}
