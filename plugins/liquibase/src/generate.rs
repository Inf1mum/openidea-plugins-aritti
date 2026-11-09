//! Bounded source generation. XML mutations use parsed namespaces, not local tag names.
use crate::changelog::{Changelog, Format, Include};
use openidea_plugin::{Error, TextEdit};
use quick_xml::{NsReader, events::Event, name::ResolveResult};

pub const NS: &str = "http://www.liquibase.org/xml/ns/dbchangelog";
pub const MAX_TEXT: usize = 256 << 10;
fn xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r') || matches!(c as u32,0x20..=0xD7FF|0xE000..=0xFFFD|0x10000..=0x10FFFF)
}
pub fn identity(text: &str) -> bool {
    !text.trim().is_empty()
        && text.len() <= 128
        && !text.chars().any(char::is_control)
        && text.chars().all(xml_char)
        && !text.contains("${")
}
pub fn id(text: &str) -> bool {
    identity(text) && text.chars().all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-'))
}
fn xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}
fn quoted(text: &str) -> String {
    serde_json::to_string(text).expect("string")
}
pub fn sql_author(author: &str) -> Result<String, Error> {
    if !identity(author) || author.contains(['"', ':']) {
        return Err("SQL authors must not contain quotes, colons, control characters or dynamic properties.".into());
    }
    Ok(if author.chars().any(char::is_whitespace) { format!("\"{author}\"") } else { author.into() })
}
pub fn next_id(log: &Changelog) -> String {
    next_numeric(log.change_sets.iter().filter_map(|s| s.id.as_deref()))
}
pub fn next_numeric<'a>(ids: impl Iterator<Item = &'a str>) -> String {
    let highest = ids
        .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .max_by(|a, b| {
            let a = a.trim_start_matches('0');
            let b = b.trim_start_matches('0');
            a.len().cmp(&b.len()).then(a.cmp(b))
        })
        .unwrap_or("0");
    let mut digits = highest.as_bytes().to_vec();
    let mut carry = true;
    for d in digits.iter_mut().rev() {
        if *d == b'9' {
            *d = b'0';
        } else {
            *d += 1;
            carry = false;
            break;
        }
    }
    if carry {
        digits.insert(0, b'1');
    }
    String::from_utf8(digits).expect("digits")
}
#[derive(Clone)]
pub struct XmlLog {
    qualified: String,
    from: usize,
    to: usize,
    self_closed: bool,
    pub identities: Vec<(String, String)>,
    pub includes: Vec<Include>,
    pub opaque: bool,
    property_owners: Vec<(usize, String)>,
    indexed_sets: Vec<(usize, String, String)>,
    rollback_refs: Vec<(usize, String, String)>,
    unknown_identities: bool,
}
enum ParentKind { Xml(XmlLog), Array(crate::array_actions::ArrayLog) }
pub struct ParentLog {
    kind: ParentKind,
    pub identities: Vec<(String, String)>,
    pub includes: Vec<Include>,
    pub opaque: bool,
    pub property_owners: Vec<(usize, String)>,
    pub indexed_sets: Vec<(usize, String, String)>,
    pub rollback_refs: Vec<(usize, String, String)>,
    pub complete_ids: bool,
}
pub fn parent_log(path: &str, text: &str) -> Result<ParentLog, Error> {
    let kind = if path.ends_with(".xml") { ParentKind::Xml(xml_log(text)?) }
    else if path.ends_with(".json") { ParentKind::Array(crate::array_actions::ArrayLog::parse(text, Format::Json)?) }
    else if path.ends_with(".yaml") || path.ends_with(".yml") { ParentKind::Array(crate::array_actions::ArrayLog::parse(text, Format::Yaml)?) }
    else { return Err("Use a namespaced XML, YAML or JSON changelog parent.".into()); };
    let (identities, includes, opaque, property_owners, indexed_sets, rollback_refs, complete_ids) = match &kind {
        ParentKind::Xml(log) => (log.identities.clone(), log.includes.clone(), log.opaque, log.property_owners.clone(), log.indexed_sets.clone(), log.rollback_refs.clone(), !log.unknown_identities),
        ParentKind::Array(log) => (log.identities.clone(), log.includes.clone(), log.opaque, log.property_owners.clone(), log.indexed_sets.clone(), log.rollback_refs.clone(), log.identities_complete()),
    };
    Ok(ParentLog { kind, identities, includes, opaque, property_owners, indexed_sets, rollback_refs, complete_ids })
}
impl ParentLog {
    pub fn changeset(&self, text: &str, id: &str, author: &str) -> Result<TextEdit, Error> {
        match &self.kind { ParentKind::Xml(log) => log.changeset(text, id, author), ParentKind::Array(log) => log.changeset(text, id, author) }
    }
    pub fn include(&self, text: &str, file: &str) -> Result<TextEdit, Error> {
        match &self.kind { ParentKind::Xml(log) => Ok(log.include(text, file)), ParentKind::Array(log) => log.include(text, file) }
    }
}
fn attributes(e: &quick_xml::events::BytesStart<'_>) -> Result<Vec<(String, String)>, Error> {
    e.attributes()
        .map(|a| {
            let a = a.map_err(|_| Error::from("XML attributes are invalid or duplicated."))?;
            let value = a
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(|_| Error::from("XML attributes contain unresolved entities."))?
                .into_owned();
            if !value.chars().all(xml_char) {
                return Err("XML attributes contain invalid character references.".into());
            }
            Ok((a.key.0.to_owned(), value))
        })
        .collect()
}
// XML1.0 Fifth Edition NameStartChar/NameChar, excluding ':' for NCName.
fn ncname_start(c: char) -> bool {
    matches!(c, 'A'..='Z' | '_' | 'a'..='z' | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}'
        | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}' | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}' | '\u{10000}'..='\u{EFFFF}')
}
fn ncname(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(ncname_start) && chars.all(|c| ncname_start(c)
        || matches!(c, '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}'))
}
fn namespace_bindings<R>(reader: &NsReader<R>, element: &quick_xml::events::BytesStart<'_>, attrs: &[(String, String)]) -> Result<(), Error> {
    const XML: &str = "http://www.w3.org/XML/1998/namespace";
    const XMLNS: &str = "http://www.w3.org/2000/xmlns/";
    let qualified = |name: &str| {
        let parts: Vec<_> = name.split(':').collect();
        parts.len() <= 2 && parts.iter().all(|part| ncname(part))
    };
    let name = element.name();
    if !qualified(name.as_ref()) || name.as_ref().starts_with("xmlns:") {
        return Err("XML element qualified name is invalid.".into());
    }
    let mut expanded = std::collections::BTreeSet::new();
    for (key, value) in attrs {
        if !qualified(key) { return Err("XML attribute qualified name is invalid.".into()); }
        if key == "xmlns" || key.starts_with("xmlns:") {
            if key == "xmlns:xmlns"
                || key.starts_with("xmlns:") && value.is_empty()
                || key == "xmlns:xml" && value != XML
                || key != "xmlns:xml" && matches!(value.as_str(), XML | XMLNS)
            { return Err("XML contains an invalid reserved namespace binding.".into()); }
        } else {
            let (namespace, local) = reader.resolver().resolve_attribute(quick_xml::name::QName(key));
            let namespace = match namespace {
                ResolveResult::Unknown(_) => return Err("XML contains an undeclared attribute namespace.".into()),
                ResolveResult::Unbound => None,
                ResolveResult::Bound(namespace) => Some(namespace.0.to_owned()),
            };
            if !expanded.insert((namespace, local.as_ref().to_owned())) {
                return Err("XML attributes have duplicate expanded namespace names.".into());
            }
        }
    }
    Ok(())
}
pub fn xml_log(text: &str) -> Result<XmlLog, Error> {
    xml_log_mode(text, false)
}
/// Read-only include navigation also supports legacy unnamespaced core XML.
/// Its syntax/ownership admission must still precede scanner-backed locations.
pub(crate) fn navigation_xml_log(text: &str) -> Result<XmlLog, Error> {
    xml_log_mode(text, true)
}
fn xml_log_mode(text: &str, legacy: bool) -> Result<XmlLog, Error> {
    if !text.chars().all(xml_char) {
        return Err("XML contains unsupported control/non-XML characters.".into());
    }
    if text.len() > MAX_TEXT {
        return Err("Changelog exceeds the 256 KiB mutation limit.".into());
    }
    let mut reader = NsReader::from_str(text);
    reader.config_mut().check_comments = true;
    let mut depth = 0usize;
    let mut roots = 0usize;
    let mut closed = false;
    let mut qualified = String::new();
    let mut insertion = None;
    let mut identities = Vec::new();
    let mut unknown_identities = false;
    let mut property_owners = Vec::new();
    let mut indexed_sets = Vec::new();
    let mut rollback_refs = Vec::new();
    let mut includes = Vec::new();
    let mut opaque = false;
    let mut events = 0;
    let mut declaration = false;
    loop {
        let before = reader.buffer_position() as usize;
        let (namespace, event) = reader.read_resolved_event().map_err(|_| Error::from("The XML must be well formed before mutation."))?;
        events += 1;
        if events > 50_000 || depth > 64 {
            return Err("XML structure exceeds the mutation limit.".into());
        }
        if matches!(&namespace, ResolveResult::Unknown(_)) {
            return Err("XML contains an undeclared namespace prefix.".into());
        }
        let liquibase = matches!(&namespace,ResolveResult::Bound(n) if n.0==NS)
            || legacy && matches!(&namespace, ResolveResult::Unbound);
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(&event, Event::Empty(_));
                let attrs = attributes(e)?;
                namespace_bindings(&reader, e, &attrs)?;
                if depth == 0 {
                    roots += 1;
                    if roots != 1 || closed || !liquibase || e.local_name().as_ref() != "databaseChangeLog" {
                        return Err("Use one databaseChangeLog root in the Liquibase namespace.".into());
                    }
                    qualified = e.name().0.to_owned();
                    if empty {
                        let end = reader.buffer_position() as usize;
                        insertion = Some((end - 2, end, true));
                        closed = true;
                    }
                }
                if depth == 1 && matches!(e.local_name().as_ref(), "changeSet" | "include" | "includeAll" | "property") && !liquibase {
                    return Err("Core changelog elements must use the root Liquibase namespace.".into());
                }
                let get = |name: &str| attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
                opaque |= attrs.iter().any(|(k, _)| matches!(k.as_str(), "xml:base" | "logicalFilePath"));
                if depth == 1 && liquibase {
                    match e.local_name().as_ref() {
                        "changeSet" => {
                            unknown_identities |= get("id").is_none_or(|v| !identity(&v)) || get("author").is_none_or(|v| !identity(&v));
                            if let (Some(id), Some(author)) = (get("id"), get("author")) {
                                indexed_sets.push((before, id.clone(), author.clone())); identities.push((id, author));
                            }
                        }
                        "property" if get("file").is_none() => { if let Some(name) = get("name") { property_owners.push((before, name)); } },
                        "include" | "includeAll" => {
                            let dir = e.local_name().as_ref() == "includeAll";
                            let include = Include {
                                path: get(if dir { "path" } else { "file" }).unwrap_or_default(),
                                relative: get("relativeToChangelogFile").is_some_and(|v| v.eq_ignore_ascii_case("true")),
                                dir,
                                optional: false,
                                at: before,
                                end: before,
                            };
                            includes.push(include);
                        }
                        _ => {}
                    }
                }
                if depth == 2 && liquibase && e.local_name().as_ref() == "rollback" && get("changeSetPath").is_none() {
                    if let (Some(id), Some(author)) = (get("changeSetId"), get("changeSetAuthor")) { rollback_refs.push((before, id, author)); }
                }
                if !empty {
                    depth += 1;
                }
            }
            Event::End(_) => {
                if depth == 0 {
                    return Err("Unmatched XML closing tag.".into());
                }
                depth -= 1;
                if depth == 0 {
                    insertion = Some((before, before, false));
                    closed = true;
                }
            }
            Event::Decl(ref decl) => {
                if declaration || before != 0 || roots != 0 || depth != 0 {
                    return Err("XML declaration must occur once at the start of the document.".into());
                }
                let tag = quick_xml::events::BytesStart::from_content(decl.as_ref(), 3);
                let mut order = 0;
                for attribute in tag.attributes() {
                    let attribute = attribute.map_err(|_| Error::from("XML declaration attributes are invalid or duplicated."))?;
                    let value = attribute.value.as_ref();
                    match attribute.key.as_ref() {
                        "version" if order == 0 && value == "1.0" => order = 1,
                        "encoding" if order == 1 && value.len() <= 64
                            && value.bytes().next().is_some_and(|b| b.is_ascii_alphabetic())
                            && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) => order = 2,
                        "standalone" if matches!(order, 1 | 2) && matches!(value, "yes" | "no") => order = 3,
                        _ => return Err("Use an ordered XML1.0 version, optional encoding and standalone declaration.".into()),
                    }
                }
                if order == 0 { return Err("XML declaration requires version1.0.".into()); }
                declaration = true;
            }
            Event::PI(ref pi) if pi.target().eq_ignore_ascii_case("xml") => return Err("XML is a reserved processing-instruction target.".into()),
            Event::DocType(_) => return Err("DTD-based changelogs are not supported for safe source generation.".into()),
            Event::Text(ref t) if depth == 0 && !t.as_ref().trim_matches('\u{feff}').trim().is_empty() => {
                return Err("Text outside the XML root is not supported.".into());
            }
            Event::CData(_) if depth == 0 => return Err("CDATA outside the XML root is invalid.".into()),
            Event::GeneralRef(ref r) => {
                if quick_xml::escape::resolve_predefined_entity(r.as_ref()).is_none()
                    && !r.resolve_char_ref().ok().flatten().is_some_and(xml_char)
                {
                    return Err("XML contains an unresolved entity.".into());
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if roots != 1 || depth != 0 || !closed {
        return Err("Close the Liquibase XML root before mutation.".into());
    }
    let (from, to, self_closed) = insertion.ok_or("Cannot find the XML insertion point.")?;
    Ok(XmlLog { qualified, from, to, self_closed, identities, includes, opaque, property_owners, indexed_sets, rollback_refs, unknown_identities })
}
impl XmlLog {
    fn tag(&self, local: &str) -> String {
        self.qualified.rsplit_once(':').map_or(local.to_owned(), |(prefix, _)| format!("{prefix}:{local}"))
    }
    fn edit(&self, text: &str, block: &str) -> TextEdit {
        let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
        let mut from = self.from;
        let mut inserted = block.to_owned();
        if self.self_closed {
            inserted = format!(">\n{inserted}</{}>", self.qualified);
        } else {
            let line = text[..from].rfind('\n').map_or(0, |n| n + 1);
            if text[line..from].trim().is_empty() {
                inserted.push_str(&text[line..from]);
                from = line;
            } else if from > 0 && !text[..from].ends_with('\n') {
                inserted.insert(0, '\n');
            }
        }
        if eol == "\r\n" {
            inserted = inserted.replace('\n', eol);
        }
        TextEdit { range: spring_text::range(text, from, self.to), text: inserted }
    }
    pub fn changeset(&self, text: &str, id: &str, author: &str) -> Result<TextEdit, Error> {
        if self.unknown_identities {
            return Err("Existing changeset identities are incomplete or dynamic; uniqueness cannot be established.".into());
        }
        if !crate::generate::id(id) || !identity(author) {
            return Err("Use a bounded literal changeset id and author.".into());
        }
        if self.identities.iter().any(|(i, a)| i == id && a == author) {
            return Err("This id/author pair already exists in the current changelog.".into());
        }
        let tag = self.tag("changeSet");
        Ok(self.edit(text, &format!("    <{tag} id=\"{}\" author=\"{}\">\n        \n    </{tag}>\n", xml(id), xml(author))))
    }
    pub fn include(&self, text: &str, file: &str) -> TextEdit {
        self.edit(text, &format!("    <{} file=\"{}\" relativeToChangelogFile=\"true\"/>\n", self.tag("include"), xml(file)))
    }
}
pub fn sql_changeset(text: &str, id: &str, author: &str) -> Result<TextEdit, Error> {
    if !crate::generate::id(id) {
        return Err("Use a literal changeset id with letters, digits, dot, underscore or hyphen.".into());
    }
    let author = sql_author(author)?;
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let separator = if text.ends_with('\n') { "" } else { eol };
    Ok(TextEdit {
        range: spring_text::range(text, text.len(), text.len()),
        text: format!("{separator}{eol}--changeset {author}:{id}{eol}"),
    })
}
pub fn changelog_source(format: Format, author: &str) -> Result<String, Error> {
    if !identity(author) {
        return Err("Use a bounded literal author without control characters or dynamic properties.".into());
    }
    Ok(match format {
        Format::Xml => include_str!("../templates/changelog.xml").replace("$END$", "").replace("$USER$", &xml(author)),
        Format::Yaml => include_str!("../templates/changelog.yaml").replace("$END$", "").replace("$USER$", &quoted(author)),
        Format::Sql => include_str!("../templates/changelog.sql").replace("$END$", "").replace("$USER$", &sql_author(author)?),
        Format::Json => format!(
            "{}\n",
            serde_json::to_string_pretty(&serde_json::json!({"databaseChangeLog":[{"changeSet":{"id":"1","author":author,"changes":[]}}]}))
                .expect("source")
        ),
    })
}
pub fn extension(format: Format) -> &'static str {
    match format {
        Format::Xml => "xml",
        Format::Yaml => "yaml",
        Format::Json => "json",
        Format::Sql => "sql",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn apply(text: &str, edit: &TextEdit) -> String {
        let from = openidea_plugin::text::offset(text, edit.range.start).unwrap();
        let to = openidea_plugin::text::offset(text, edit.range.end).unwrap();
        format!("{}{}{}", &text[..from], edit.text, &text[to..])
    }
    #[test]
    fn namespace_prefixes_self_closing_roots_and_literals_preserve_valid_xml() {
        for text in [
            format!("<databaseChangeLog xmlns=\"{NS}\"/>"),
            format!("<?xml version=\"1.0\"?>\n<lb:databaseChangeLog xmlns:lb=\"{NS}\">\n</lb:databaseChangeLog>\n"),
        ] {
            let log = xml_log(&text).unwrap();
            let edit = log.changeset(&text, "1", "Jane & \"Doe\" <dev>").unwrap();
            let result = apply(&text, &edit);
            let parsed = xml_log(&result).unwrap();
            assert_eq!(parsed.identities, [("1".into(), "Jane & \"Doe\" <dev>".into())]);
            assert!(result.contains("&amp;") && result.contains("&quot;") && result.contains("&lt;"));
            if text.contains("lb:") {
                assert!(result.contains("<lb:changeSet") && result.contains("</lb:changeSet>"));
            }
            assert!(parsed.changeset(&result, "1", "Jane & \"Doe\" <dev>").is_err());
        }
    }
    #[test]
    fn malformed_foreign_unbound_and_ambiguous_xml_are_not_mutation_sources() {
        for text in [
            "<databaseChangeLog/>",
            "<databaseChangeLog xmlns=\"urn:unrelated\"/>",
            "<x:databaseChangeLog/>",
            "<databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\"><changeSet></databaseChangeLog>",
            "<databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\"><changeSet xmlns=\"urn:other\"/></databaseChangeLog>",
            "<!DOCTYPE databaseChangeLog [<!ENTITY x 'value'>]><databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\"/>",
            "<databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\"/><other/>",
            "<databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\" a=\"1\" a=\"2\"/>",
            "<databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\">&undefined;</databaseChangeLog>",
        ] {
            assert!(xml_log(text).is_err(), "{text}");
        }
        assert!(xml_log(&"x".repeat(MAX_TEXT + 1)).is_err());
        for text in [
            format!("<databaseChangeLog xmlns=\"{NS}\">&#0;</databaseChangeLog>"),
            format!("<databaseChangeLog xmlns=\"{NS}\" bad=\"&#xFFFF;\"/>"),
            format!("<databaseChangeLog xmlns=\"{NS}\">\0</databaseChangeLog>"),
        ] {
            assert!(xml_log(&text).is_err());
        }
    }
    #[test]
    fn crlf_and_utf16_ranges_and_new_content_formats_are_bounded_and_escaped() {
        let text = format!("<databaseChangeLog xmlns=\"{NS}\"><!-- 😀é --></databaseChangeLog>\r\n");
        let info = xml_log(&text).unwrap();
        let edit = info.changeset(&text, "2", "ann").unwrap();
        let at = text.find("</databaseChangeLog>").unwrap();
        assert_eq!(edit.range.start.character, text[..at].encode_utf16().count() as u32);
        let result = apply(&text, &edit);
        assert!(xml_log(&result).is_ok());
        assert!(!edit.text.replace("\r\n", "").contains('\n'));
        let author = "Jane & \"Doe\"";
        assert_eq!(xml_log(&changelog_source(Format::Xml, author).unwrap()).unwrap().identities[0].1, author);
        let json: serde_json::Value = serde_json::from_str(&changelog_source(Format::Json, author).unwrap()).unwrap();
        assert_eq!(json["databaseChangeLog"][0]["changeSet"]["author"], author);
        assert!(changelog_source(Format::Yaml, author).unwrap().contains("author: \"Jane & \\\"Doe\\\"\""));
        assert!(changelog_source(Format::Sql, author).is_err());
        assert!(changelog_source(Format::Sql, "Jane Doe").unwrap().contains("--changeset \"Jane Doe\":1"));
        assert_eq!(next_numeric(["18446744073709551615"].into_iter()), "18446744073709551616");
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    #[test]
    fn template_sentinels_in_author_remain_literal_and_unknown_ids_block_append() {
        let author = "user$END$literal";
        for format in [Format::Xml, Format::Yaml, Format::Sql, Format::Json] {
            assert!(changelog_source(format, author).unwrap().contains(author));
        }
        for set in ["<changeSet id=\"${id}\" author=\"ann\"/>", "<changeSet id=\"1\"/>"] {
            let text = format!("<databaseChangeLog xmlns=\"{NS}\">{set}</databaseChangeLog>");
            let log = xml_log(&text).unwrap();
            assert!(log.changeset(&text, "2", "ann").is_err());
        }
    }
}
