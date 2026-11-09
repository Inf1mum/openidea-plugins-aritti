//! Source-preserving YAML/JSON array mutations, after strict bounded parsing.
use crate::{changelog::{Format, Include}, generate};
use openidea_plugin::{Error, TextEdit};
use yaml_rust2::{Yaml, YamlLoader, parser::{Event, Parser}};

pub(crate) struct ArrayLog {
    format: Format,
    at: usize,
    indent: usize,
    flow: bool,
    empty: bool,
    pub identities: Vec<(String, String)>,
    pub includes: Vec<Include>,
    pub opaque: bool,
    pub property_owners: Vec<(usize, String)>,
    pub indexed_sets: Vec<(usize, String, String)>,
    pub rollback_refs: Vec<(usize, String, String)>,
    unknown_identities: bool,
}
fn invalid() -> Error { "Use one bounded YAML/JSON databaseChangeLog array, without aliases, tags or duplicate mapping keys.".into() }
fn byte(text: &str, index: usize) -> usize {
    text.char_indices().nth(index).map_or(text.len(), |(at, _)| at)
}
fn key<'a>(value: &'a Yaml, name: &str) -> &'a Yaml { &value[name] }
fn literal(value: &Yaml) -> Option<String> {
    match value { Yaml::String(s) => Some(s.clone()), Yaml::Integer(n) => Some(n.to_string()), _ => None }
}
fn opaque(value: &Yaml) -> bool {
    match value {
        Yaml::Hash(map) => map.iter().any(|(k, v)| matches!(k.as_str(), Some("logicalFilePath" | "xml:base")) || opaque(v)),
        Yaml::Array(items) => items.iter().any(opaque),
        _ => false,
    }
}
impl ArrayLog {
    pub fn parse(text: &str, format: Format) -> Result<Self, Error> {
        if text.len() > generate::MAX_TEXT || !matches!(format, Format::Yaml | Format::Json) { return Err(invalid()); }
        if format == Format::Json {
            let value = crate::strict_json::parse(text, generate::MAX_TEXT)?;
            if value.as_object().is_none_or(|root| root.len() != 1 || !root.get("databaseChangeLog").is_some_and(|array| array.is_array())) { return Err(invalid()); }
            // YAML does not accept JSON's UTF-16 surrogate-pair escapes. Use the
            // strict standard-JSON value only for semantic metadata; keep the
            // original source and its outer array's closing bracket for edits.
            let normalized = serde_json::to_string(&value).map_err(|_| invalid())?;
            let mut log = Self::parse(&normalized, Format::Yaml)?;
            log.at = text.rfind(']').ok_or_else(invalid)?;
            log.indent = 0;
            log.flow = true;
            log.format = Format::Json;
            return Ok(log);
        }
        // Refuse amplification before loading any aliases or tagged values.
        let mut parser = Parser::new_from_str(text);
        let mut depth = 0usize;
        let mut documents = 0;
        let mut start = None;
        let mut end = None;
        let mut complete = false;
        for _ in 0..50_000 {
            let (event, mark) = parser.next_token().map_err(|_| invalid())?;
            match event {
                Event::DocumentStart => { documents += 1; if documents > 1 { return Err(invalid()); } },
                Event::Alias(_) => return Err(invalid()),
                Event::Scalar(_, _, anchor, tag) if anchor != 0 || tag.is_some() => return Err(invalid()),
                Event::SequenceStart(anchor, ref tag) | Event::MappingStart(anchor, ref tag) => {
                    if anchor != 0 || tag.is_some() || depth >= 64 { return Err(invalid()); }
                    if matches!(event, Event::SequenceStart(..)) && depth == 1 {
                        if start.is_some() { return Err(invalid()); }
                        start = Some((byte(text, mark.index()), mark.col()));
                    }
                    depth += 1;
                },
                Event::SequenceEnd | Event::MappingEnd => {
                    if matches!(event, Event::SequenceEnd) && depth == 2 { end = Some(byte(text, mark.index())); }
                    depth = depth.checked_sub(1).ok_or_else(invalid)?;
                },
                Event::StreamEnd => { complete = true; break; },
                _ => {},
            }
        }
        if !complete || depth != 0 || documents != 1 { return Err(invalid()); }
        let docs = YamlLoader::load_from_str(text).map_err(|_| invalid())?;
        let [root] = docs.as_slice() else { return Err(invalid()); };
        if root.as_hash().is_none_or(|map| map.len() != 1 || !map.contains_key(&Yaml::String("databaseChangeLog".into()))) { return Err(invalid()); }
        let items = key(root, "databaseChangeLog").as_vec().ok_or_else(invalid)?;
        if items.len() > 4096 || items.iter().any(|item| item.as_hash().is_none_or(|map| map.len() != 1)) { return Err(invalid()); }
        let (from, marker_column) = start.ok_or_else(invalid)?;
        let at = end.ok_or_else(invalid)?;
        let flow = text.as_bytes().get(from) == Some(&b'[');
        let indent = if flow {
            if text.as_bytes().get(at) != Some(&b']') { return Err(invalid()); }
            marker_column
        } else {
            // For an indentationless sequence the parser marks the first key,
            // not its dash. Recover only the already-validated item's line.
            let line = text[..from].rfind('\n').map_or(0, |n| n + 1);
            let rest = &text[line..];
            let spaces = rest.bytes().take_while(|b| *b == b' ').count();
            if !rest[spaces..].starts_with("- ") { return Err(invalid()); }
            spaces
        };
        let mut identities = Vec::new();
        let mut includes = Vec::new();
        let mut unknown_identities = false;
        let mut property_owners = Vec::new();
        let mut indexed_sets = Vec::new();
        let mut rollback_refs = Vec::new();
            for (index, item) in items.iter().enumerate() {
            if let Some(property) = item.as_hash().and_then(|map| map.get(&Yaml::String("property".into()))) {
                if matches!(key(property, "file"), Yaml::BadValue) {
                    if let Some(name) = literal(key(property, "name")) { property_owners.push((index, name)); }
                }
            }
            if let Some(set) = item.as_hash().and_then(|map| map.get(&Yaml::String("changeSet".into()))) {
                match (literal(key(set, "id")), literal(key(set, "author"))) {
                    (Some(id), Some(author)) if generate::identity(&id) && generate::identity(&author) => { indexed_sets.push((index, id.clone(), author.clone())); identities.push((id, author)); },
                    _ => unknown_identities = true,
                }
                let rollback = key(set, "rollback");
                if matches!(key(rollback, "changeSetPath"), Yaml::BadValue) {
                    if let (Some(id), Some(author)) = (literal(key(rollback, "changeSetId")), literal(key(rollback, "changeSetAuthor"))) { rollback_refs.push((index, id, author)); }
                }
            }
            for (name, dir) in [("include", false), ("includeAll", true)] {
                if let Some(include) = item.as_hash().and_then(|map| map.get(&Yaml::String(name.into()))) {
                    let relative = key(include, "relativeToChangelogFile");
                    let include = Include { path: literal(key(include, if dir { "path" } else { "file" })).unwrap_or_default(),
                        relative: relative.as_bool() == Some(true) || relative.as_str().is_some_and(|s| s.eq_ignore_ascii_case("true")),
                        dir, optional: false, at: 0, end: 0 };
                    includes.push(include);
                }
            }
        }
        Ok(Self { format, at, indent, flow, empty: items.is_empty(), identities, includes, opaque: opaque(root), property_owners, indexed_sets, rollback_refs, unknown_identities })
    }
    pub fn identities_complete(&self) -> bool { !self.unknown_identities }
    fn insert(&self, text: &str, json: serde_json::Value, block: &str) -> Result<TextEdit, Error> {
        let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
        let inserted = if self.flow {
            format!("{}{}{}{}", if self.empty || text[..self.at].trim_end().ends_with(',') { "" } else { "," }, eol, " ".repeat(self.indent + 2), json)
        } else {
            let separator = if self.at == 0 || text[..self.at].ends_with('\n') { "" } else { eol };
            format!("{separator}{}", block.replace('\n', eol))
        };
        let changed = format!("{}{}{}", &text[..self.at], inserted, &text[self.at..]);
        Self::parse(&changed, self.format)?; // No malformed or oversized mutation leaves the resolver.
        Ok(TextEdit { range: spring_text::range(text, self.at, self.at), text: inserted })
    }
    pub fn changeset(&self, text: &str, id: &str, author: &str) -> Result<TextEdit, Error> {
        if self.unknown_identities || !generate::id(id) || !generate::identity(author) { return Err("Use literal bounded identities; existing changesets must have known id/author pairs.".into()); }
        if self.identities.iter().any(|(i, a)| i == id && a == author) { return Err("This id/author pair already exists in the current changelog.".into()); }
        let quote = |s| serde_json::to_string(s).expect("string");
        let indent = " ".repeat(self.indent);
        let block = format!("{indent}- changeSet:\n{indent}    id: {}\n{indent}    author: {}\n{indent}    changes: []\n", quote(id), quote(author));
        self.insert(text, serde_json::json!({"changeSet":{"id":id,"author":author,"changes":[]}}), &block)
    }
    pub fn include(&self, text: &str, file: &str) -> Result<TextEdit, Error> {
        let indent = " ".repeat(self.indent);
        let block = format!("{indent}- include:\n{indent}    file: {}\n{indent}    relativeToChangelogFile: true\n", serde_json::to_string(file).expect("string"));
        self.insert(text, serde_json::json!({"include":{"file":file,"relativeToChangelogFile":true}}), &block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn apply(text: &str, edit: &TextEdit) -> String {
        let a = openidea_plugin::text::offset(text, edit.range.start).unwrap();
        let b = openidea_plugin::text::offset(text, edit.range.end).unwrap();
        format!("{}{}{}", &text[..a], edit.text, &text[b..])
    }
    #[test]
    fn block_and_flow_yaml_and_json_preserve_original_source_and_unicode_positions() {
        for (format, text) in [
            (Format::Yaml, "# 😀 comment\ndatabaseChangeLog:\n  - changeSet:\n      id: 1\n      author: ann\n      changes: []\n# retained tail\n"),
            (Format::Yaml, "databaseChangeLog: []\n"),
            (Format::Yaml, "databaseChangeLog: [{changeSet: {id: old, author: ann}}, ]\n"),
            (Format::Yaml, "---\ndatabaseChangeLog:\n- changeSet: {id: old, author: ann, changes: []}\n...\n"),
            (Format::Yaml, "databaseChangeLog: [{changeSet: {id: old, author: ann, changes: []}}]\n"),
            (Format::Json, "{\"databaseChangeLog\":[{\"changeSet\":{\"id\":\"😀\",\"author\":\"ann\",\"changes\":[]}}]}"),
            (Format::Json, "{\"databaseChangeLog\":[]}"),
            (Format::Json, r#"{"databaseChangeLog":[{"changeSet":{"id":"old","author":"\ud83d\ude00","changes":[]}}]}"#),
        ] {
            let log = ArrayLog::parse(text, format).unwrap_or_else(|e| panic!("{text}: {e}"));
            let edit = log.changeset(text, "2", "Jane \"Doe\" 😀").unwrap();
            assert_eq!(edit.range.start, edit.range.end, "no original byte is replaced");
            let changed = apply(text, &edit);
            let reparsed = ArrayLog::parse(&changed, format).unwrap_or_else(|e| panic!("{changed}: {e}"));
            assert_eq!(reparsed.identities.len(), log.identities.len() + 1);
            assert!(reparsed.identities.contains(&("2".into(), "Jane \"Doe\" 😀".into())));
            let include = log.include(text, "changes/new.yaml").unwrap();
            let included = apply(text, &include);
            assert!(ArrayLog::parse(&included, format).unwrap().includes.iter().any(|i| i.path == "changes/new.yaml" && i.relative));
        }
    }
    #[test]
    fn duplicate_keys_aliases_tags_multiple_documents_and_wrong_shapes_are_refused() {
        for text in [
            "databaseChangeLog: []\ndatabaseChangeLog: []",
            "databaseChangeLog: [{changeSet: {id: 1, id: 2, author: ann}}]",
            "databaseChangeLog: &a []",
            "databaseChangeLog: [*missing]",
            "databaseChangeLog: !!seq []",
            "databaseChangeLog: []\n---\ndatabaseChangeLog: []",
            "databaseChangeLog: null",
            "databaseChangeLog: []\nother: ignored",
            "databaseChangeLog: [wrong]",
            "databaseChangeLog: [{changeSet: {}, include: {file: x}}]",
        ] { assert!(ArrayLog::parse(text, Format::Yaml).is_err(), "{text}"); }
        for text in ["{\"databaseChangeLog\":[],\"databaseChangeLog\":[]}", "{databaseChangeLog: []}", "{\"databaseChangeLog\":[}"] {
            assert!(ArrayLog::parse(text, Format::Json).is_err(), "{text}");
        }
        assert!(ArrayLog::parse(&format!("databaseChangeLog: {}0{}", "[".repeat(65), "]".repeat(65)), Format::Yaml).is_err());
        assert!(ArrayLog::parse(&" ".repeat(generate::MAX_TEXT + 1), Format::Yaml).is_err());
    }
    #[test]
    fn identities_opaque_ownership_and_crlf_are_preserved_without_guessing_uniqueness() {
        let text = "databaseChangeLog:\r\n  - changeSet:\r\n      id: 1\r\n      author: ann\r\n      changes: []\r\n";
        let log = ArrayLog::parse(text, Format::Yaml).unwrap();
        assert!(log.changeset(text, "1", "ann").is_err());
        let edit = log.changeset(text, "2", "ann").unwrap();
        assert!(!edit.text.replace("\r\n", "").contains('\n'));
        assert_eq!(ArrayLog::parse(&apply(text, &edit), Format::Yaml).unwrap().identities.len(), 2);
        let unknown = "databaseChangeLog: [{changeSet: {id: '${id}', author: ann}}]";
        assert!(ArrayLog::parse(unknown, Format::Yaml).unwrap().changeset(unknown, "2", "ann").is_err());
        assert!(ArrayLog::parse("databaseChangeLog: [{logicalFilePath: other}]", Format::Yaml).unwrap().opaque);
    }
}
