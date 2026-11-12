//! Persistence tree groups that are not tied to one entity: attribute converters, Hibernate custom types,
//! DTOs and projections linked to an entity, Liquibase changelogs and saved connection profiles.
//! Purely lexical; what cannot be established from scanned source stays unknown. Nothing here reads a password.
use crate::model::{has_word, mask, split_top_level};
use crate::file_actions::mapstruct_links::{declaration_offset, locate, mapped_types};
use crate::file_actions::projection_links::{Conventions, Kind};
use crate::structure_related::{ARTIFACT_PREFIX, GROUP_PREFIX, Locations, MAX_GROUP_CHILDREN};
use crate::xml;
use openidea_plugin::views::TreeItem;
use openidea_plugin::{Position, Project};
use regex_lite::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

pub const MAX_CHANGELOG_FILES: u32 = 50;
pub const LIQUIBASE_KEY: &str = "group:liquibase:";
pub const CONNECTIONS_KEY: &str = "group:connections:";
pub const NO_CONNECTIONS_KEY: &str = "connection:none";
pub const CONNECTION_PREFIX: &str = "connection:";
const MAX_TEXT: usize = 512 * 1024;
const USER_TYPE_BASES: [&str; 8] = [
    "UserType",
    "CompositeUserType",
    "UserVersionType",
    "EnhancedUserType",
    "ParameterizedType",
    "AbstractSingleColumnStandardBasicType",
    "AbstractStandardBasicType",
    "BasicType",
];

#[derive(Clone)]
pub struct Found {
    pub name: String,
    pub detail: String,
    pub path: String,
    pub pos: Position,
}

pub struct Links {
    projection: Option<Conventions>,
    dto: Option<Conventions>,
}

impl Links {
    pub fn load() -> Self {
        Self { projection: Conventions::load_for(Kind::Projection).ok(), dto: Conventions::load_for(Kind::Dto).ok() }
    }
}

#[derive(Default)]
pub struct Artifacts {
    pub converters: Vec<Found>,
    pub user_types: Vec<Found>,
    /// DTOs and projections by the simple name of the entity they are linked to.
    pub linked: HashMap<String, Vec<Found>>,
    changelogs: Option<Vec<Found>>,
    mapper_pairs: Vec<(String, String)>,
}

const MAX_MAPPER_PAIRS: usize = 2000;

fn declaration() -> &'static Regex {
    static DECLARATION: OnceLock<Regex> = OnceLock::new();
    DECLARATION.get_or_init(|| Regex::new(r"\b(class|interface|record|object)\s+(\w+)").unwrap())
}

fn type_arguments(header: &str, base: &str) -> Option<Vec<String>> {
    let rest = header[header.find(base)? + base.len()..].trim_start().strip_prefix('<')?;
    let inner = &rest[..rest.rfind('>')?];
    Some(split_top_level(inner).into_iter().map(|(_, part)| part.trim().to_owned()).collect())
}

fn auto_apply(masked: &str, class_start: usize) -> bool {
    let window = masked.get(class_start.saturating_sub(200)..class_start).unwrap_or_default();
    let Some(at) = window.rfind("@Converter") else {
        return false;
    };
    let compact: String = window[at..].chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains("autoApply=true")
}

impl Artifacts {
    /// Records what `text` declares. Called for every project source the entity scan reads.
    pub fn observe(&mut self, links: &Links, path: &str, text: &str) {
        if text.len() > MAX_TEXT {
            return;
        }
        if text.contains("@Mapper") && self.mapper_pairs.len() < MAX_MAPPER_PAIRS {
            self.mapper_pairs.extend(mapped_types(text));
        }
        let stem = path.rsplit('/').next().and_then(|file| file.rsplit_once('.')).map_or("", |(stem, _)| stem);
        let mentions_converter = text.contains("AttributeConverter");
        let mentions_type = USER_TYPE_BASES.iter().any(|base| text.contains(base));
        let link = if text.contains("@Entity") {
            None
        } else {
            [("Projection", &links.projection), ("DTO", &links.dto)].into_iter().find_map(|(label, conventions)| {
                let conventions = conventions.as_ref()?;
                let by_name = conventions.entity_of(stem, "");
                let entity = by_name.clone().or_else(|| conventions.entity_of(stem, text))?;
                let how = if by_name.is_some() { "class name" } else { "Javadoc" };
                Some((entity, format!("{label} · linked by {how}")))
            })
        };
        if !mentions_converter && !mentions_type && link.is_none() {
            return;
        }

        let masked = mask(text);
        for caps in declaration().captures_iter(&masked) {
            let name = caps.get(2).unwrap();
            let header_end = masked[name.end()..].find('{').map_or(masked.len(), |at| name.end() + at);
            let header = &masked[name.end()..header_end];
            let found = |detail: String| Found {
                name: name.as_str().to_owned(),
                detail,
                path: path.to_owned(),
                pos: crate::position(text, name.start()),
            };

            if let Some((entity, detail)) = &link {
                if name.as_str() == stem {
                    self.linked.entry(entity.clone()).or_default().push(found(detail.clone()));
                }
            }

            if mentions_converter && has_word(header, "AttributeConverter") {
                let detail = match type_arguments(header, "AttributeConverter").as_deref() {
                    Some([attribute, column]) => format!("{attribute} ↔ {column}"),
                    _ => "AttributeConverter".to_owned(),
                };
                let detail = if auto_apply(&masked, name.start()) { format!("{detail} · auto-apply") } else { detail };
                self.converters.push(found(detail));
                continue;
            }

            if mentions_type {
                if let Some(base) = USER_TYPE_BASES.iter().find(|base| has_word(header, base)) {
                    self.user_types.push(found((*base).to_owned()));
                }
            }
        }
    }

    /// Links the DTO side of every scanned MapStruct mapper method to the entity side (a named entity of `entities`).
    pub fn link_mapped(&mut self, project: &Project, files: &[String], entities: &HashSet<String>) {
        let simple = |qualified: &str| qualified.rsplit('.').next().unwrap_or(qualified).to_owned();
        let mut seen = HashSet::new();
        for (source, target) in std::mem::take(&mut self.mapper_pairs) {
            let (entity, dto) = match (entities.contains(&simple(&source)), entities.contains(&simple(&target))) {
                (true, false) => (simple(&source), target),
                (false, true) => (simple(&target), source),
                _ => continue,
            };
            if !seen.insert((entity.clone(), dto.clone())) {
                continue;
            }
            let Some(path) = locate(files, &dto) else { continue };
            if self.linked.get(&entity).is_some_and(|known| known.iter().any(|found| found.path == path)) {
                continue;
            }
            let Ok(text) = project.read_file(&path) else { continue };
            if text.len() > MAX_TEXT || ["@Entity", "@MappedSuperclass", "@Embeddable"].iter().any(|marker| text.contains(marker)) {
                continue;
            }
            let name = simple(&dto);
            let Some(offset) = declaration_offset(&text, &name) else { continue };
            let found = Found { name, detail: "DTO · linked by mapper method".to_owned(), path, pos: crate::position(&text, offset) };
            self.linked.entry(entity).or_default().push(found);
        }
    }

    pub fn sort(&mut self) {
        for list in [&mut self.converters, &mut self.user_types].into_iter().chain(self.linked.values_mut()) {
            list.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));
            list.dedup_by(|a, b| a.path == b.path && a.name == b.name);
        }
    }

    pub fn forget_changelogs(&mut self) {
        self.changelogs = None;
    }

    /// Changelog files found by name; master first. Read once per refresh, bounded by `MAX_CHANGELOG_FILES` per pattern.
    pub fn changelogs(&mut self, project: &Project) -> &[Found] {
        self.changelogs.get_or_insert_with(|| find_changelogs(project))
    }

    pub fn changelogs_cached(&self) -> &[Found] {
        self.changelogs.as_deref().unwrap_or_default()
    }
}

fn absolute(root: &str, path: String) -> String {
    if path.starts_with('/') { path } else { format!("{}/{path}", root.trim_end_matches('/')) }
}

fn changelog_text(project: &Project, path: &str) -> Option<String> {
    let text = project.read_file(path).ok()?;
    (text.len() <= MAX_TEXT && (text.contains("databaseChangeLog") || text.contains("liquibase formatted sql"))).then_some(text)
}

/// Files the XML master `text` at `path` includes: `include file=` (relative to the master or found by path suffix) and `includeAll path=`.
fn included_by(project: &Project, root: &str, path: &str, text: &str) -> Vec<String> {
    let directory = path.rsplit_once('/').map_or("", |(directory, _)| directory);
    let mut resolved = Vec::new();
    for element in xml::parse(text) {
        let attribute = |name: &str| element.attributes.iter().find(|a| a.name == name).map(|a| a.value.trim_start_matches("classpath:").trim_start_matches("./").trim_start_matches('/').to_owned());
        match element.name.as_str() {
            "include" => {
                let Some(file) = attribute("file").filter(|file| !file.contains("..")) else { continue };
                if attribute("relativeToChangelogFile").is_some_and(|value| value == "true") {
                    resolved.push(format!("{directory}/{file}"));
                    continue;
                }
                let suffix = format!("/{file}");
                let matches = project.find_files(&format!("**/{file}"), 5).unwrap_or_default();
                resolved.extend(matches.into_iter().map(|found| absolute(root, found)).filter(|found| found.ends_with(&suffix)));
            }
            "includeAll" => {
                let Some(folder) = attribute("path").filter(|folder| !folder.contains("..")) else { continue };
                let folder = folder.trim_end_matches('/');
                for extension in ["xml", "yaml", "yml", "json", "sql"] {
                    let found = project.find_files(&format!("**/{folder}/*.{extension}"), MAX_CHANGELOG_FILES).unwrap_or_default();
                    resolved.extend(found.into_iter().map(|found| absolute(root, found)));
                }
            }
            _ => {}
        }
        if resolved.len() > MAX_CHANGELOG_FILES as usize {
            break;
        }
    }
    resolved
}

fn find_changelogs(project: &Project) -> Vec<Found> {
    let root = project.root().unwrap_or_default();
    let mut paths: Vec<String> = Vec::new();
    for extension in ["xml", "yaml", "yml", "json", "sql"] {
        for path in project.find_files(&format!("**/*changelog*.{extension}"), MAX_CHANGELOG_FILES + 1).unwrap_or_default() {
            paths.push(absolute(&root, path));
        }
    }
    paths.sort();
    paths.dedup();

    let mut read: Vec<(String, String)> = paths.iter().filter_map(|path| Some((path.clone(), changelog_text(project, path)?))).collect();
    let is_master = |path: &str| path.rsplit('/').next().is_some_and(|file| file.contains("master"));
    let mut by_master: HashMap<String, Vec<String>> = HashMap::new();
    for (path, text) in read.clone().iter().filter(|(path, _)| is_master(path) && path.ends_with(".xml")) {
        let master = path.rsplit('/').next().unwrap_or(path).to_owned();
        for included in included_by(project, &root, path, text) {
            if &included == path {
                continue;
            }
            if !read.iter().any(|(known, _)| *known == included) {
                if let Some(text) = changelog_text(project, &included) {
                    read.push((included.clone(), text));
                }
            }
            let masters = by_master.entry(included).or_default();
            if !masters.contains(&master) {
                masters.push(master.clone());
            }
        }
    }

    let mut found: Vec<Found> = read
        .iter()
        .map(|(path, text)| {
            let detail = if is_master(path) {
                let count = xml::parse(text).iter().filter(|element| matches!(element.name.as_str(), "include" | "includeAll")).count();
                if path.ends_with(".xml") { format!("master \u{b7} {count} include entries") } else { "master \u{b7} includes not read (not XML)".to_owned() }
            } else {
                match by_master.get_mut(path) {
                    Some(masters) => {
                        masters.sort();
                        format!("included by {}", masters.join(", "))
                    }
                    None => "changelog".to_owned(),
                }
            };
            Found { name: path.rsplit('/').next().unwrap_or(path).to_owned(), detail, path: path.clone(), pos: Position { line: 0, character: 0 } }
        })
        .collect();
    found.sort_by(|a, b| (!a.detail.starts_with("master"), &a.path).cmp(&(!b.detail.starts_with("master"), &b.path)));
    found.dedup_by(|a, b| a.path == b.path);
    found
}

/// A group node and up to `MAX_GROUP_CHILDREN` children (`limit` overrides it). Returns whether anything was cut.
#[allow(clippy::too_many_arguments)]
pub fn push_group(
    items: &mut Vec<TreeItem>,
    locations: &mut Locations,
    groups: &mut HashSet<String>,
    expanded: &HashSet<String>,
    parent: Option<&str>,
    key: &str,
    label: &str,
    found: &[Found],
    child_icon: &str,
    limit: usize,
    open_ended: bool,
) -> bool {
    if found.is_empty() {
        return false;
    }
    let total = found.len();
    let shown = total.min(limit);
    let detail = match (shown < total, open_ended) {
        (false, _) => total.to_string(),
        (true, false) => format!("{shown} of {total}"),
        (true, true) => format!("{shown} of {total}+"),
    };
    let mut group = TreeItem::new(key, label).icon("folder").detail(&detail).expandable(expanded.contains(key));
    if let Some(parent) = parent {
        group = group.parent(parent);
    }
    items.push(group);
    groups.insert(key.to_owned());
    for artifact in &found[..shown] {
        let child = format!("{ARTIFACT_PREFIX}{key}>{}#{}", artifact.path, artifact.name);
        items.push(TreeItem::new(&child, &artifact.name).parent(key).icon(child_icon).detail(&artifact.detail));
        locations.insert(child, (artifact.path.clone(), artifact.pos));
    }
    shown < total
}

impl Artifacts {
    /// Converters and Hibernate custom types of the project, under a persistence unit (`parent`) or at the top level.
    pub fn unit_groups(&self, parent: Option<&str>, items: &mut Vec<TreeItem>, locations: &mut Locations, groups: &mut HashSet<String>, expanded: &HashSet<String>) -> bool {
        let scope = parent.unwrap_or_default();
        let mut cut = false;
        for (kind, label, found, icon) in [
            ("usertypes", "Hibernate Custom Types", &self.user_types, "type"),
            ("converters", "JPA Converters", &self.converters, "class"),
        ] {
            let key = format!("{GROUP_PREFIX}{kind}:{scope}");
            cut |= push_group(items, locations, groups, expanded, parent, &key, label, found, icon, MAX_GROUP_CHILDREN, false);
        }
        cut
    }

    pub fn liquibase_group(&self, items: &mut Vec<TreeItem>, locations: &mut Locations, groups: &mut HashSet<String>, expanded: &HashSet<String>) -> bool {
        push_group(items, locations, groups, expanded, None, LIQUIBASE_KEY, "Liquibase", self.changelogs_cached(), "file-xml", MAX_CHANGELOG_FILES as usize, true)
    }
}

/// The saved connection profiles, never a password: name, DBMS and the credential-free target.
pub fn connection_group(profiles: Result<Vec<persistence_core::connection::Profile>, openidea_plugin::Error>, items: &mut Vec<TreeItem>, groups: &mut HashSet<String>, expanded: &HashSet<String>) {
    use persistence_core::connection::Dbms;
    groups.insert(CONNECTIONS_KEY.to_owned());
    let (detail, children) = match profiles {
        Err(_) => ("unknown".to_owned(), vec![TreeItem::new(NO_CONNECTIONS_KEY, "Unknown").parent(CONNECTIONS_KEY).icon("warning").detail("saved connections cannot be read; open to manage")]),
        Ok(profiles) if profiles.is_empty() => {
            ("0".to_owned(), vec![TreeItem::new(NO_CONNECTIONS_KEY, "No saved connections").parent(CONNECTIONS_KEY).icon("info").detail("open to add one")])
        }
        Ok(profiles) => (
            profiles.len().to_string(),
            profiles
                .iter()
                .take(MAX_GROUP_CHILDREN)
                .map(|profile| {
                    let icon = match profile.dbms {
                        Dbms::PostgreSql => "db-postgres",
                        Dbms::MySql => "db-mysql",
                        Dbms::MariaDb => "db-mariadb",
                        Dbms::SqlServer => "db-sqlserver",
                        _ => "database",
                    };
                    let user = profile.user.as_deref().filter(|user| !user.is_empty()).map_or(String::new(), |user| format!(" · user {user}"));
                    let detail = format!("{}: {}{user}", profile.dbms.label(), profile.target());
                    TreeItem::new(&format!("{CONNECTION_PREFIX}{}", profile.name), &profile.name).parent(CONNECTIONS_KEY).icon(icon).detail(&detail)
                })
                .collect(),
        ),
    };
    items.push(TreeItem::new(CONNECTIONS_KEY, "DB connections").icon("database").detail(&detail).expandable(expanded.contains(CONNECTIONS_KEY)));
    items.extend(children);
}
