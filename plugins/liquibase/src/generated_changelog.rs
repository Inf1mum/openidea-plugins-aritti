//! Complete generated core-XML fragments only. Never synthesizes database DDL.
use crate::generate;
use openidea_plugin::Error;
use std::collections::BTreeSet;
pub(crate) const MAX_BYTES: usize = 128 << 10;
struct Change { start: usize, end: usize, label: String, author: String }
pub(crate) struct Changelog { pub source: String, changes: Vec<Change> }
impl Changelog {
    pub fn parse(source: &str) -> Result<Self, Error> {
        let invalid = || Error::Invalid("Generated output must be bounded UTF-8 core Liquibase XML with1–512 unique literal changesets and no external-file/opaque root dependencies.".into());
        if source.len() > MAX_BYTES { return Err(invalid()); }
        let log = generate::xml_log(source)?;
        if log.opaque || !log.includes.is_empty() { return Err(invalid()); }
        let mut reader = quick_xml::Reader::from_str(source);
        let mut depth = 0usize;
        loop {
            match reader.read_event().map_err(|_| invalid())? {
                quick_xml::events::Event::Decl(decl) => {
                    if let Some(encoding) = decl.encoding() {
                        if !encoding.map_err(|_| invalid())?.as_ref().eq_ignore_ascii_case("utf-8") { return Err(invalid()); }
                    }
                },
                quick_xml::events::Event::Start(_) => depth += 1,
                quick_xml::events::Event::End(_) => depth = depth.saturating_sub(1),
                quick_xml::events::Event::Text(text) if depth == 1 && !text.as_ref().trim().is_empty() => return Err(invalid()),
                quick_xml::events::Event::CData(_) | quick_xml::events::Event::GeneralRef(_) if depth == 1 => return Err(invalid()),
                quick_xml::events::Event::PI(_) => return Err(invalid()),
                quick_xml::events::Event::Eof => break,
                _ => {},
            }
        }
        let elements = spring_text::xml::elements(source);
        if elements.len() > 8192 || elements.is_empty() { return Err(invalid()); }
        for (i, element) in elements.iter().enumerate() {
            if spring_text::xml::namespace(&elements, i).as_deref() != Some(generate::NS)
                || matches!(element.local(), "include" | "includeAll" | "property" | "sqlFile" | "loadData" | "loadUpdateData" | "customChange" | "executeCommand")
                || element.attrs.iter().any(|a| matches!(a.local(), "file" | "path" | "valueBlobFile" | "valueClobFile")) {
                return Err(invalid());
            }
            if element.parent == Some(0) && element.local() != "changeSet" { return Err(invalid()); }
        }
        let sets: Vec<_> = elements.iter().enumerate().filter(|(_, e)| e.parent == Some(0)).collect();
        if sets.is_empty() || sets.len() > 512 || sets.len() != log.identities.len() { return Err(invalid()); }
        let mut seen = BTreeSet::new();
        let mut changes = Vec::new();
        for ((index, element), (id, author)) in sets.into_iter().zip(log.identities) {
            if !generate::identity(&id) || !generate::identity(&author) || !seen.insert((id.clone(), author.clone()))
                || element.start >= element.end || element.end > source.len() || source.get(element.start..element.end).is_none() { return Err(invalid()); }
            let children: Vec<_> = elements.iter().filter(|e| e.parent == Some(index)).collect();
            if !children.iter().any(|e| crate::catalog::change_type(e.local()).is_some())
                || children.iter().any(|e| crate::catalog::change_type(e.local()).is_none() && !matches!(e.local(), "comment" | "rollback" | "preConditions" | "validCheckSum")) { return Err(invalid()); }
            let kinds = children.iter().filter(|e| crate::catalog::change_type(e.local()).is_some()).take(4).map(|e| e.local()).collect::<Vec<_>>().join(", ");
            changes.push(Change { start: element.start, end: element.end, label: format!("{id} ({author}) · {kinds}"), author });
        }
        Ok(Self { source: source.into(), changes })
    }
    pub fn labels(&self) -> impl Iterator<Item = &str> { self.changes.iter().map(|change| change.label.as_str()) }
    pub fn len(&self) -> usize { self.changes.len() }
    pub fn author_matches(&self, author: &str) -> bool { self.changes.iter().all(|change| change.author == author) }
    pub fn subset(&self, chosen: &BTreeSet<usize>) -> Result<String, Error> {
        if chosen.is_empty() || chosen.iter().any(|index| *index >= self.changes.len()) { return Err(Error::Stale); }
        let mut source = String::with_capacity(self.source.len());
        let mut cursor = 0;
        for (index, change) in self.changes.iter().enumerate() {
            source.push_str(&self.source[cursor..change.start]);
            if chosen.contains(&index) { source.push_str(&self.source[change.start..change.end]); }
            cursor = change.end;
        }
        source.push_str(&self.source[cursor..]);
        if Self::parse(&source)?.len() != chosen.len() { return Err(Error::Stale); }
        Ok(source)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    pub fn sample() -> String { format!("<?xml version='1.0' encoding='UTF-8'?><databaseChangeLog xmlns='{}'><!-- 😀 preserved --><changeSet id='1' author='ann'><createTable tableName='CUSTOMER'/></changeSet>\r\n<changeSet id='2' author='ann'><sql><![CDATA[ALTER TABLE CUSTOMER ADD NAME VARCHAR(30);]]></sql></changeSet></databaseChangeLog>", generate::NS) }
    #[test]
    fn subset_copies_only_complete_ordered_fragments_and_keeps_root_comments_sql_and_utf8() {
        let source = sample();
        let log = Changelog::parse(&source).unwrap();
        assert_eq!(log.len(), 2);
        let selected = log.subset(&BTreeSet::from([1])).unwrap();
        assert!(!selected.contains("createTable"));
        assert!(selected.contains("😀 preserved") && selected.contains("<![CDATA[ALTER TABLE"));
        assert!(selected.contains("\r\n"));
        assert_eq!(log.subset(&BTreeSet::from([0, 1])).unwrap(), source);
        assert!(log.subset(&BTreeSet::new()).is_err());
        assert!(log.subset(&BTreeSet::from([2])).is_err());
    }
    #[test]
    fn generated_models_reject_external_files_opaque_or_malformed_and_ambiguous_source() {
        for body in ["<include file='other.xml'/>", "<property name='p' value='v'/>", "<changeSet id='1' author='ann'><sqlFile path='other.sql'/></changeSet>", "<changeSet id='1' author='ann'><sql>SELECT1;</sql></changeSet><changeSet id='1' author='ann'><sql>SELECT2;</sql></changeSet>", "<changeSet id='1' author='ann'/>", "<changeSet id='1' author='ann'><unknownChange/></changeSet>", "<changeSet id='${id}' author='ann'/>", "<preConditions/>"] {
            let source = format!("<databaseChangeLog xmlns='{}'>{body}</databaseChangeLog>", generate::NS);
            assert!(Changelog::parse(&source).is_err(), "{source}");
        }
        assert!(Changelog::parse(&sample().replace("UTF-8", "UTF-16")).is_err());
        assert!(Changelog::parse(&sample().replace("xmlns=", "logicalFilePath='old.xml' xmlns=")).is_err());
        assert!(Changelog::parse(&sample().replace("</databaseChangeLog>", "")).is_err());
        assert!(Changelog::parse(&" ".repeat(MAX_BYTES + 1)).is_err());
    }
}
