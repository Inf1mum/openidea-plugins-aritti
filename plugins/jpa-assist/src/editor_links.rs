//! Editor-attached navigation between entities, their DTOs and projections and their repositories.
//!
//! - Gutter marker on a DTO or projection type to its entity, and on an entity to the DTOs and projections linked to it.
//!   The links are the Projection and DTO Declaration rules (class name pattern, Javadoc link) plus MapStruct mapper methods;
//!   an entity name that is not unique in a fully scanned project stays unlinked.
//! - Status bar item standing in for the original's editor toolbar (the host has no editor toolbar contribution): it names the
//!   entity, DTO, projection or repository in the focused editor, and its command lists the related artifacts and the
//!   entity commands that apply to the file.
//!
//! Source is read lexically in small slices outside and inside the 50 ms provider budget; nothing here writes a file.
use crate::file_actions::mapstruct_links::{declaration_offset, locate, mapped_types};
use crate::model::{Kind, language_of, mask, parse_class};
use crate::structure_artifacts::{Artifacts, Found, Links};
use openidea_plugin::providers::{Document, LineMarker};
use openidea_plugin::{Error, Event, Invocation, Location, PickItem, Position, Project, Range, StatusItem, host};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub(crate) const STATUS_ITEM: &str = "jpa-editor";
pub(crate) const COMMAND: &str = "editor-related";
const MAX_TEXT: usize = 512 * 1024;
const MAX_FILES: usize = 5_000;
const PROVIDER_BUDGET: Duration = Duration::from_millis(30);
const EVENT_BUDGET: Duration = Duration::from_millis(200);
const MAX_NAMED: usize = 3;
const MAX_MAPPED_READS: usize = 8;
const MAX_LISTED: usize = 30;

#[derive(Clone)]
struct EntityFact {
    name: String,
    fqn: String,
    path: String,
    pos: Position,
}

#[derive(Default)]
struct Facts {
    entity: Option<EntityFact>,
    linked: Vec<(String, Found)>,
    pairs: Vec<(String, String)>,
}

#[derive(Default)]
struct Index {
    listed: bool,
    truncated: bool,
    unread: Vec<String>,
    paths: Vec<String>,
    files: HashMap<String, Facts>,
}

enum Entities<'a> {
    None,
    One(&'a EntityFact),
    Many,
}

thread_local! {
    static INDEXES: RefCell<HashMap<String, Index>> = RefCell::new(HashMap::new());
    static FOCUSED: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn package_of(text: &str) -> Option<String> {
    let masked = mask(text);
    let line = masked.lines().map(str::trim).find(|line| line.starts_with("package "))?;
    let name = line["package ".len()..].trim().trim_end_matches(';').trim();
    (!name.is_empty()).then(|| name.to_owned())
}

fn stem_of(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

fn location(path: &str, at: Position) -> Location {
    Location { path: path.to_owned(), range: Range::new(at, at) }
}

impl Index {
    fn complete(&self) -> bool {
        self.listed && !self.truncated && self.unread.is_empty()
    }

    fn entity_named(&self, name: &str) -> Entities<'_> {
        let mut found = self.files.values().filter_map(|facts| facts.entity.as_ref()).filter(|entity| entity.name == name);
        match (found.next(), found.next()) {
            (None, _) => Entities::None,
            (Some(one), None) => Entities::One(one),
            _ => Entities::Many,
        }
    }

    fn entity_qualified(&self, fqn: &str) -> Option<&EntityFact> {
        self.files.values().filter_map(|facts| facts.entity.as_ref()).find(|entity| entity.fqn == fqn)
    }

    fn observe(&mut self, links: &Links, path: &str, text: &str) {
        let facts = facts_of(links, path, text);
        if facts.entity.is_none() && facts.linked.is_empty() && facts.pairs.is_empty() {
            self.files.remove(path);
            return;
        }
        self.files.insert(path.to_owned(), facts);
    }

    fn forget(&mut self, path: &str) {
        self.files.remove(path);
        self.unread.retain(|p| p != path);
    }

    fn advance(&mut self, project: &Project, budget: Duration) {
        if !self.listed {
            self.listed = true;
            for pattern in ["**/*.java", "**/*.kt"] {
                let found = project.find_files(pattern, MAX_FILES as u32).unwrap_or_default();
                self.truncated |= found.len() >= MAX_FILES;
                self.paths.extend(found.iter().filter(|path| path.ends_with(".java")).cloned());
                self.unread.extend(found);
            }
        }

        let links = Links::load();
        let start = Instant::now();
        while start.elapsed() < budget {
            let Some(path) = self.unread.pop() else {
                break;
            };
            match project.read_file(&path) {
                Ok(text) => self.observe(&links, &path, &text),
                Err(_) => {
                    self.files.remove(&path);
                }
            }
        }
    }

    /// DTOs and projections of `entity`: by name or Javadoc rule from any scanned file, then MapStruct mapper methods.
    fn linked_to(&self, project: &Project, entity: &EntityFact) -> Vec<Found> {
        let mut found: Vec<Found> = self
            .files
            .values()
            .flat_map(|facts| facts.linked.iter())
            .filter(|(name, _)| *name == entity.name)
            .map(|(_, found)| found.clone())
            .collect();

        let mut reads = 0;
        for pair in self.files.values().flat_map(|facts| facts.pairs.iter()) {
            let other = if pair.0 == entity.fqn {
                &pair.1
            } else if pair.1 == entity.fqn {
                &pair.0
            } else {
                continue;
            };
            if self.entity_qualified(other).is_some() {
                continue;
            }
            let Some(path) = locate(&self.paths, other) else {
                continue;
            };
            if found.iter().any(|known| known.path == path) || reads >= MAX_MAPPED_READS {
                continue;
            }
            reads += 1;
            let Ok(text) = project.read_file(&path) else {
                continue;
            };
            let name = other.rsplit('.').next().unwrap_or(other).to_owned();
            let Some(at) = declaration_offset(&text, &name) else {
                continue;
            };
            found.push(Found { name, detail: "DTO · linked by mapper method".into(), path, pos: crate::position(&text, at) });
        }

        found.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));
        found.dedup_by(|a, b| a.path == b.path && a.name == b.name);
        found
    }

    /// The unique entity a DTO or projection declared in `path` belongs to, with how it was linked.
    fn entity_of_linked(&self, path: &str, text: &str) -> Vec<(EntityFact, Found)> {
        let mut result: Vec<(EntityFact, Found)> = Vec::new();
        for (name, found) in self.files.get(path).into_iter().flat_map(|facts| facts.linked.iter()) {
            if let Entities::One(entity) = self.entity_named(name) {
                result.push((entity.clone(), found.clone()));
            }
        }

        let stem = stem_of(path);
        let fqn = package_of(text).map_or_else(|| stem.to_owned(), |package| format!("{package}.{stem}"));
        for pair in self.files.values().flat_map(|facts| facts.pairs.iter()) {
            let other = if pair.0 == fqn {
                &pair.1
            } else if pair.1 == fqn {
                &pair.0
            } else {
                continue;
            };
            let Some(entity) = self.entity_qualified(other) else {
                continue;
            };
            if result.iter().any(|(known, _)| known.path == entity.path) {
                continue;
            }
            let Some(at) = declaration_offset(text, stem) else {
                continue;
            };
            let found = Found { name: stem.to_owned(), detail: "DTO · linked by mapper method".into(), path: path.to_owned(), pos: crate::position(text, at) };
            result.push((entity.clone(), found));
        }
        result
    }
}

fn facts_of(links: &Links, path: &str, text: &str) -> Facts {
    let mut facts = Facts::default();
    if text.len() > MAX_TEXT || language_of(path).is_none() {
        return facts;
    }

    if text.contains("@Entity") {
        facts.entity = parse_class(text, path).filter(|class| class.kind == Kind::Entity).map(|class| EntityFact {
            fqn: class.package.as_ref().map_or_else(|| class.name.clone(), |package| format!("{package}.{}", class.name)),
            name: class.name,
            path: path.to_owned(),
            pos: class.pos,
        });
    }

    let mut artifacts = Artifacts::default();
    artifacts.observe(links, path, text);
    facts.linked = artifacts.linked.into_iter().flat_map(|(entity, list)| list.into_iter().map(move |found| (entity.clone(), found))).collect();
    if text.contains("@Mapper") {
        facts.pairs = mapped_types(text);
    }
    facts
}

fn with_index<T>(project: &Project, run: impl FnOnce(&mut Index) -> T) -> Option<T> {
    let root = project.root().ok()?;
    Some(INDEXES.with(|indexes| run(indexes.borrow_mut().entry(root).or_default())))
}

fn marker(line: u32, tooltip: String, target: Location) -> LineMarker {
    LineMarker { line, icon: Some("navigate".into()), tooltip, target: Some(target) }
}

/// Markers of `doc`; reads at most one provider budget's worth of unread project files first.
pub(crate) fn markers(doc: &Document) -> Vec<LineMarker> {
    if language_of(&doc.path).is_none() || doc.text.len() > MAX_TEXT {
        return Vec::new();
    }
    let Some(project) = crate::project_of(&doc.path) else {
        return Vec::new();
    };
    let links = Links::load();
    with_index(&project, |index| {
        index.observe(&links, &doc.path, &doc.text);
        let known = index.files.contains_key(&doc.path);
        if known || !index.listed {
            index.advance(&project, PROVIDER_BUDGET);
        }
        index.unread.retain(|path| *path != doc.path);
        markers_with(index, &project, doc)
    })
    .unwrap_or_default()
}

fn markers_with(index: &Index, project: &Project, doc: &Document) -> Vec<LineMarker> {
    if let Some(entity) = index.files.get(&doc.path).and_then(|facts| facts.entity.as_ref()) {
        let Entities::One(_) = index.entity_named(&entity.name) else {
            return Vec::new();
        };
        let linked = index.linked_to(project, entity);
        let Some(first) = linked.first() else {
            return Vec::new();
        };
        let named: Vec<&str> = linked.iter().take(MAX_NAMED).map(|found| found.name.as_str()).collect();
        let more = linked.len().saturating_sub(MAX_NAMED);
        let rest = if more > 0 { format!(" and {more} more") } else { String::new() };
        let partial = if index.complete() { "" } else { " (project scan incomplete)" };
        return vec![marker(entity.pos.line, format!("DTOs and projections: {}{rest}{partial}", named.join(", ")), location(&first.path, first.pos))];
    }

    if !index.complete() {
        return Vec::new();
    }
    index
        .entity_of_linked(&doc.path, &doc.text)
        .into_iter()
        .map(|(entity, found)| marker(found.pos.line, format!("Entity {}: {}", entity.name, found.detail), location(&entity.path, entity.pos)))
        .collect()
}

/// Reads the project index outside the provider budget, drops what changed on disk and refreshes the status item.
pub(crate) fn events(events: &[Event], invocation: &Invocation) {
    let Some(project) = invocation.project() else {
        return;
    };
    let interesting = |path: &str| path.ends_with(".java") || path.ends_with(".kt");
    let mut refocus = false;
    for event in events {
        match event {
            Event::EditorFocused(path) => {
                FOCUSED.with(|focused| *focused.borrow_mut() = path.clone());
                refocus = true;
            }
            Event::FilesChanged(batch) => {
                refocus = true;
                with_index(&project, |index| {
                    for path in batch.created.iter().chain(&batch.modified).chain(&batch.removed).filter(|p| interesting(p)) {
                        index.forget(path);
                        index.unread.push(path.clone());
                    }
                });
            }
            Event::FileSaved(path) if interesting(path) => {
                refocus = true;
                with_index(&project, |index| {
                    index.forget(path);
                    index.unread.push(path.clone());
                });
            }
            Event::FileOpened(_) => refocus = true,
            _ => {}
        }
    }
    if !refocus {
        return;
    }
    refresh_status(&project);
}

#[derive(Clone, PartialEq, Debug)]
enum Role {
    Entity,
    Linked,
    Repository,
}

fn role_in(index: &Index, path: &str, text: &str) -> Option<(Role, String)> {
    let kotlin = language_of(path)?;
    if let Some(facts) = index.files.get(path) {
        if let Some(entity) = &facts.entity {
            return Some((Role::Entity, entity.name.clone()));
        }
        if let Some((name, _)) = facts.linked.first() {
            return Some((Role::Linked, name.clone()));
        }
    }
    if let Some((entity, _)) = index.entity_of_linked(path, text).first() {
        return Some((Role::Linked, entity.name.clone()));
    }
    let repository = crate::queries::parse_repository(text, kotlin)?;
    Some((Role::Repository, repository.entity))
}

fn refresh_status(project: &Project) {
    let enabled = crate::setting("editorStatusItem");
    let focused = FOCUSED.with(|focused| focused.borrow().clone());
    let item = focused.filter(|_| enabled).and_then(|path| {
        let text = project.read_file(&path).ok().filter(|text| text.len() <= MAX_TEXT)?;
        with_index(project, |index| {
            index.observe(&Links::load(), &path, &text);
            let (role, entity) = role_in(index, &path, &text)?;
            let linked = match index.entity_named(&entity) {
                Entities::One(fact) => Some(index.linked_to(project, fact).len()),
                _ => None,
            };
            let count = linked.map_or_else(|| "DTOs and projections unknown".to_owned(), |n| if n == 1 { "1 DTO or projection".to_owned() } else { format!("{n} DTOs or projections") });
            let partial = if index.complete() { "" } else { " (project scan incomplete)" };
            let stem = stem_of(&path);
            Some(match role {
                Role::Entity => StatusItem {
                    text: format!("JPA: {entity}"),
                    tooltip: Some(format!("Entity {entity}: {count}{partial}. Click for related artifacts and entity commands.")),
                    command: Some(COMMAND.into()),
                },
                Role::Linked => StatusItem {
                    text: format!("JPA: {stem} → {entity}"),
                    tooltip: Some(format!("{stem} is linked to entity {entity} ({count}{partial}). Click for related artifacts.")),
                    command: Some(COMMAND.into()),
                },
                Role::Repository => StatusItem {
                    text: format!("JPA: {stem} → {entity}"),
                    tooltip: Some(format!("Repository of {entity} ({count}{partial}). Click for related artifacts.")),
                    command: Some(COMMAND.into()),
                },
            })
        })
        .flatten()
    });
    match item {
        Some(item) => {
            let _ = host::set_window_status_item(STATUS_ITEM, &item);
        }
        None => {
            let _ = host::remove_window_status_item(STATUS_ITEM);
        }
    }
}

/// What the status item command lists: related files first, then the entity commands that apply to this file.
pub(crate) fn run(ctx: &Invocation, repositories: &mut dyn FnMut(&Project, &str) -> Vec<(String, Location)>) -> Result<Option<String>, Error> {
    let editor = ctx.editor().ok_or("Open an entity, DTO, projection or repository first.")?;
    let project = ctx.project().ok_or("Open a project first.")?;
    let path = editor.path();
    let text = editor.document()?.text;
    let links = Links::load();
    let items = with_index(&project, |index| {
        index.advance(&project, EVENT_BUDGET);
        index.observe(&links, &path, &text);
        let (role, entity_name) = role_in(index, &path, &text)?;
        let mut items: Vec<PickItem> = Vec::new();
        let entity = match index.entity_named(&entity_name) {
            Entities::One(fact) => Some(fact.clone()),
            _ => None,
        };
        let go = |location: &Location| format!("go:{}#{}:{}", location.path, location.range.start.line, location.range.start.character);
        if let (Some(entity), false) = (&entity, role == Role::Entity) {
            items.push(PickItem::new(&go(&location(&entity.path, entity.pos)), &format!("Entity {}", entity.name)).description("Go to entity"));
        }
        if let Some(entity) = &entity {
            for found in index.linked_to(&project, entity).iter().filter(|found| found.path != path).take(MAX_LISTED) {
                items.push(PickItem::new(&go(&location(&found.path, found.pos)), &found.name).description(&format!("Dto & Projections · {}", found.detail)));
            }
        }
        for (name, at) in repositories(&project, &entity_name).into_iter().filter(|(_, at)| at.path != path).take(MAX_LISTED) {
            items.push(PickItem::new(&go(&at), &name).description("Repository"));
        }
        if role == Role::Entity {
            for (id, label) in [
                ("copy-ddl", "Copy DDL of this entity"),
                ("copy-dto", "Copy DTO of this entity"),
                ("source-diagram", "Show entity relationship diagram"),
                ("find-attribute-usages", "Find attribute usages in this document"),
            ] {
                items.push(PickItem::new(&format!("run:{id}"), label).description("Entity command"));
            }
        }
        Some(items)
    })
    .flatten()
    .ok_or("The open file is not an entity, DTO, projection or repository this assistant recognises.")?;

    if items.is_empty() {
        return Err("No related artifact was found in the scanned sources.".into());
    }
    let Some(key) = host::pick("JPA related artifacts", &items)? else {
        return Err(Error::Cancelled);
    };
    if let Some(id) = key.strip_prefix("run:") {
        host::execute_command(&format!("plugin.aritti.jpa-assist.{id}"), None)?;
        return Ok(None);
    }
    let target = key.strip_prefix("go:").and_then(|rest| rest.rsplit_once('#')).ok_or(Error::Stale)?;
    let (line, column) = target.1.split_once(':').ok_or(Error::Stale)?;
    host::open_location(target.0, line.parse().map_err(|_| Error::Stale)?, column.parse().map_err(|_| Error::Stale)?)?;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ENTITY: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Customer {\n    @Id Long id;\n    String name;\n}\n";
    const DTO: &str = "package shop;\npublic record CustomerDto(Long id, String name) {\n}\n";
    const PROJECTION: &str = "package shop;\npublic interface CustomerProjection {\n    String getName();\n}\n";
    const SUMMARY: &str = "package shop.web;\npublic class Summary {\n}\n";
    const MAPPER: &str = "package shop.web;\nimport org.mapstruct.Mapper;\nimport shop.Customer;\n@Mapper\npublic interface CustomerMapper {\n    Summary toSummary(Customer customer);\n}\n";
    const DUPLICATE: &str = "package other;\nimport jakarta.persistence.*;\n@Entity\npublic class Customer {\n    @Id Long id;\n}\n";

    fn host() -> MockHost {
        INDEXES.with(|indexes| indexes.borrow_mut().clear());
        FOCUSED.with(|focused| *focused.borrow_mut() = None);
        let host = MockHost::install().with_capabilities(&["events", "project.read", "editor.read"]);
        host.add_file("src/shop/Customer.java", ENTITY);
        host.add_file("src/shop/CustomerDto.java", DTO);
        host.add_file("src/shop/CustomerProjection.java", PROJECTION);
        host.add_file("src/shop/web/Summary.java", SUMMARY);
        host.add_file("src/shop/web/CustomerMapper.java", MAPPER);
        host
    }

    fn doc(path: &str, text: &str) -> Document {
        Document { path: path.into(), language: "java".into(), version: 1, text: text.into() }
    }

    #[test]
    fn a_dto_and_a_projection_point_at_their_entity() {
        let _host = host();
        let dto = markers(&doc("/project/src/shop/CustomerDto.java", DTO));
        assert_eq!(dto.len(), 1, "{dto:?}");
        assert_eq!(dto[0].tooltip, "Entity Customer: DTO · linked by class name");
        assert_eq!(dto[0].target.as_ref().unwrap().path, "/project/src/shop/Customer.java");
        assert_eq!(dto[0].icon.as_deref(), Some("navigate"));
        let projection = markers(&doc("/project/src/shop/CustomerProjection.java", PROJECTION));
        assert_eq!(projection[0].tooltip, "Entity Customer: Projection · linked by class name");
    }

    #[test]
    fn an_entity_lists_its_dtos_projections_and_mapper_targets() {
        let _host = host();
        let markers = markers(&doc("/project/src/shop/Customer.java", ENTITY));
        assert_eq!(markers.len(), 1, "{markers:?}");
        assert_eq!(markers[0].tooltip, "DTOs and projections: CustomerDto, CustomerProjection, Summary");
        assert_eq!(markers[0].line, 3);
        assert_eq!(markers[0].target.as_ref().unwrap().path, "/project/src/shop/CustomerDto.java");
    }

    #[test]
    fn a_mapper_method_links_a_dto_without_a_naming_convention() {
        let host = host();
        events(&[Event::FileOpened("/project/src/shop/Customer.java".into())], &host.invocation());
        let summary = markers(&doc("/project/src/shop/web/Summary.java", SUMMARY));
        assert_eq!(summary.len(), 1, "{summary:?}");
        assert_eq!(summary[0].tooltip, "Entity Customer: DTO · linked by mapper method");
    }

    #[test]
    fn a_duplicated_entity_name_stays_unlinked() {
        let host = host();
        host.add_file("src/other/Customer.java", DUPLICATE);
        assert!(markers(&doc("/project/src/shop/CustomerDto.java", DTO)).is_empty());
        assert!(markers(&doc("/project/src/shop/Customer.java", ENTITY)).is_empty());
    }

    #[test]
    fn an_unknown_entity_and_unrelated_files_get_no_marker() {
        let host = host();
        host.add_file("src/shop/OrderDto.java", "package shop;\npublic record OrderDto(Long id) {\n}\n");
        host.add_file("src/shop/Service.java", "package shop;\npublic class Service {\n}\n");
        assert!(markers(&doc("/project/src/shop/OrderDto.java", "package shop;\npublic record OrderDto(Long id) {\n}\n")).is_empty());
        assert!(markers(&doc("/project/src/shop/Service.java", "package shop;\npublic class Service {\n}\n")).is_empty());
    }

    #[test]
    fn the_status_item_follows_the_focused_file_and_can_be_switched_off() {
        let host = host();
        let ctx = host.invocation();
        events(&[Event::EditorFocused(Some("/project/src/shop/Customer.java".into()))], &ctx);
        assert_eq!(host.status_items()[STATUS_ITEM].text, "JPA: Customer");
        assert!(host.status_items()[STATUS_ITEM].tooltip.as_deref().unwrap().contains("project scan incomplete"), "events never scan the project: {:?}", host.status_items());

        let project = ctx.project().unwrap();
        with_index(&project, |index| index.advance(&project, Duration::from_secs(5)));
        events(&[Event::EditorFocused(Some("/project/src/shop/Customer.java".into()))], &ctx);
        let items = host.status_items();
        assert!(items[STATUS_ITEM].tooltip.as_deref().unwrap().contains("3 DTOs or projections"), "{items:?}");
        assert_eq!(items[STATUS_ITEM].command.as_deref(), Some(COMMAND));

        events(&[Event::EditorFocused(Some("/project/src/shop/CustomerDto.java".into()))], &ctx);
        assert_eq!(host.status_items()[STATUS_ITEM].text, "JPA: CustomerDto → Customer");

        events(&[Event::EditorFocused(Some("/project/src/shop/web/Summary.java".into()))], &ctx);
        assert_eq!(host.status_items()[STATUS_ITEM].text, "JPA: Summary → Customer");

        events(&[Event::EditorFocused(Some("/project/src/shop/Plain.java".into()))], &ctx);
        assert!(!host.status_items().contains_key(STATUS_ITEM));

        events(&[Event::EditorFocused(Some("/project/src/shop/Customer.java".into()))], &ctx);
        assert!(host.status_items().contains_key(STATUS_ITEM));
        host.set_setting("jpaAssist.editorStatusItem", "false");
        events(&[Event::EditorFocused(Some("/project/src/shop/Customer.java".into()))], &ctx);
        assert!(!host.status_items().contains_key(STATUS_ITEM));
    }

    #[test]
    fn the_command_lists_related_artifacts_and_navigates() {
        let host = host();
        host.open_editor("/project/src/shop/Customer.java", "java", ENTITY, &[]);
        let ctx = host.invocation();
        host.answer_next_prompt(Some(&["go:/project/src/shop/CustomerDto.java#1:14"]));
        let mut none = |_: &Project, _: &str| Vec::new();
        run(&ctx, &mut none).unwrap();
        assert_eq!(host.opened_locations(), vec![("/project/src/shop/CustomerDto.java".to_owned(), 1, 14)]);
    }

    #[test]
    fn the_command_refuses_an_unrelated_file() {
        let host = host();
        host.open_editor("/project/src/shop/web/Plain.java", "java", "package shop;\npublic class Plain {\n}\n", &[]);
        let mut none = |_: &Project, _: &str| Vec::new();
        assert!(run(&host.invocation(), &mut none).is_err());
    }
}
