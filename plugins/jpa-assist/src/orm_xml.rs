//! XML mapping facts from `META-INF/orm.xml` and `persistence.xml` `mapping-file` entries.
//! Resolved only when every mapping file is the unique match of its name; anything else is `Unresolved`.
use crate::xml::{self, Element};
use openidea_plugin::host;
use std::collections::HashMap;

const MAX_TEXT: usize = 256 * 1024;

#[derive(Debug, Default, PartialEq)]
pub struct Mapped {
    pub identifier: bool,
    pub attributes: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub enum Orm {
    Absent,
    Unresolved,
    Resolved(HashMap<String, Mapped>),
}

impl Orm {
    /// `true` when XML may still add an identifier or attribute to `fqcn`.
    pub fn may_declare(&self, fqcn: &str) -> bool {
        match self {
            Orm::Absent => false,
            Orm::Unresolved => true,
            Orm::Resolved(map) => map.contains_key(fqcn),
        }
    }

    pub fn declares_identifier(&self, fqcn: &str) -> bool {
        matches!(self, Orm::Resolved(map) if map.get(fqcn).is_some_and(|m| m.identifier))
    }

    pub fn declares_attribute(&self, fqcn: &str, name: &str) -> bool {
        matches!(self, Orm::Resolved(map) if map.get(fqcn).is_some_and(|m| m.attributes.iter().any(|a| a == name)))
    }
}

pub fn load(path: &str) -> Orm {
    let Some(project) = host::project_of(path) else { return Orm::Unresolved };
    let find = |glob: &str| project.find_files(glob, 4).unwrap_or_default();
    let mut wanted = find("**/META-INF/orm.xml");
    if wanted.len() > 1 {
        return Orm::Unresolved;
    }
    let persistence = find("**/persistence.xml");
    for file in &persistence {
        let Ok(text) = project.read_file(file) else { return Orm::Unresolved };
        for element in xml::parse(&text).iter().filter(|e| e.name == "mapping-file") {
            let name = element.text.as_ref().map_or("", |(_, t)| t.as_str());
            if name.is_empty() || name.starts_with('/') || name.split('/').any(|p| matches!(p, "" | "." | "..")) {
                return Orm::Unresolved;
            }
            let mut found = find(&format!("**/{name}"));
            if found.len() != 1 {
                return Orm::Unresolved;
            }
            let found = found.remove(0);
            if !wanted.contains(&found) {
                wanted.push(found);
            }
        }
    }
    if project.find_files("**/*orm.xml", 8).unwrap_or_default().iter().any(|f| !wanted.contains(f)) {
        return Orm::Unresolved;
    }
    if wanted.is_empty() {
        return Orm::Absent;
    }
    let mut map = HashMap::new();
    for file in &wanted {
        let Ok(text) = project.read_file(file) else { return Orm::Unresolved };
        if text.len() > MAX_TEXT || !read_mappings(&text, &mut map) {
            return Orm::Unresolved;
        }
    }
    Orm::Resolved(map)
}

const KINDS: [&str; 3] = ["entity", "mapped-superclass", "embeddable"];
const ATTRIBUTES: [&str; 8] = ["id", "embedded-id", "basic", "version", "many-to-one", "one-to-many", "one-to-one", "many-to-many"];
const MORE_ATTRIBUTES: [&str; 2] = ["embedded", "element-collection"];

/// `false` when the file is not a plain, non-metadata-complete mapping file.
fn read_mappings(text: &str, map: &mut HashMap<String, Mapped>) -> bool {
    let elements = xml::parse(text);
    if !elements.iter().any(|e| e.name == "entity-mappings") {
        return false;
    }
    let complete = |e: &Element| e.attributes.iter().any(|a| a.name == "metadata-complete" && a.value != "false");
    if elements.iter().any(|e| complete(e) || e.name == "xml-mapping-metadata-complete") {
        return false;
    }
    let package = elements
        .iter()
        .find(|e| e.name == "package" && e.parent.as_deref() == Some("entity-mappings"))
        .and_then(|e| e.text.as_ref())
        .map_or(String::new(), |(_, t)| t.clone());
    let mut current: Option<String> = None;
    for element in &elements {
        let name = element.name.as_str();
        let class = element.attributes.iter().find(|a| a.name == "class").map(|a| a.value.trim());
        if KINDS.contains(&name) && element.parent.as_deref() == Some("entity-mappings") {
            let Some(class) = class else { return false };
            let fqcn = if class.contains('.') || package.is_empty() { class.to_owned() } else { format!("{package}.{class}") };
            map.entry(fqcn.clone()).or_default();
            current = Some(fqcn);
            continue;
        }
        let Some(mapped) = current.as_ref().and_then(|c| map.get_mut(c)) else { continue };
        match (name, element.parent.as_deref()) {
            ("id-class", _) => mapped.identifier = true,
            (n, Some("attributes")) if ATTRIBUTES.contains(&n) || MORE_ATTRIBUTES.contains(&n) => {
                mapped.identifier |= matches!(n, "id" | "embedded-id");
                if let Some(a) = element.attributes.iter().find(|a| a.name == "name") {
                    mapped.attributes.push(a.value.clone());
                }
            }
            _ => {}
        }
    }
    true
}
