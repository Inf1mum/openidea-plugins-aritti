//! Persistence tree grouping: persistence units and the artifacts related to each entity.
//! Purely lexical. A fact that cannot be established from scanned source is labelled unknown, never guessed.
use crate::model::{Class, Relation};
use crate::structure_artifacts::Found;
use crate::xml;
use openidea_plugin::views::TreeItem;
use openidea_plugin::{Location, Position, Project};
use std::collections::{HashMap, HashSet};

pub const MAX_PERSISTENCE_FILES: u32 = 20;
pub const MAX_GROUP_CHILDREN: usize = 20;
pub const MAX_RELATED_NODES: usize = 2000;
pub const GROUP_PREFIX: &str = "group:";
pub const UNIT_PREFIX: &str = "unit:";
pub const ARTIFACT_PREFIX: &str = "artifact:";

pub type Locations = HashMap<String, (String, Position)>;

pub struct Unit {
    pub name: String,
    pub path: String,
    pub pos: Position,
    pub classes: HashSet<String>,
}

impl Unit {
    pub fn key(&self) -> String {
        format!("{UNIT_PREFIX}{}#{}", self.path, self.name)
    }
}

pub fn parse_units(path: &str, text: &str) -> Vec<Unit> {
    let mut units: Vec<Unit> = Vec::new();
    for element in xml::parse(text) {
        match (element.name.as_str(), element.parent.as_deref()) {
            ("persistence-unit", Some("persistence")) => {
                let name = element.attributes.iter().find(|a| a.name == "name").map(|a| a.value.clone());
                units.push(Unit {
                    name: name.unwrap_or_else(|| "(unnamed)".into()),
                    path: path.into(),
                    pos: crate::position(text, element.start),
                    classes: HashSet::new(),
                });
            }
            ("class", Some("persistence-unit")) => {
                if let (Some(unit), Some((_, class))) = (units.last_mut(), element.text) {
                    unit.classes.insert(class);
                }
            }
            _ => {}
        }
    }
    units
}

pub fn units(project: &Project) -> Vec<Unit> {
    let mut paths = project.find_files("**/persistence.xml", MAX_PERSISTENCE_FILES).unwrap_or_default();
    paths.sort();
    let mut units: Vec<Unit> = paths
        .iter()
        .filter_map(|path| Some(parse_units(path, &project.read_file(path).ok()?)))
        .flatten()
        .collect();
    units.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));
    units
}

pub fn qualified(class: &Class) -> String {
    match &class.package {
        Some(package) => format!("{package}.{}", class.name),
        None => class.name.clone(),
    }
}

pub enum Membership {
    Unit(usize),
    Ambiguous(Vec<usize>),
    Unassigned,
}

/// Only an explicit `<class>` entry assigns an entity; unlisted entities may still belong to a unit through scanning or mapping files.
pub fn membership(units: &[Unit], class: &Class) -> Membership {
    let name = qualified(class);
    let listed: Vec<usize> = (0..units.len()).filter(|&i| units[i].classes.contains(&name)).collect();
    match listed.as_slice() {
        [] => Membership::Unassigned,
        [one] => Membership::Unit(*one),
        _ => Membership::Ambiguous(listed),
    }
}

fn role(relation: Relation) -> Option<&'static str> {
    Some(match relation {
        Relation::ManyToOne => "Many-to-one",
        Relation::OneToOne => "One-to-one",
        Relation::OneToMany => "One-to-many",
        Relation::ManyToMany => "Many-to-many",
        _ => return None,
    })
}

enum Target<'a> {
    Entity(&'a Class),
    Ambiguous,
    Unknown,
}

pub struct Related<'a> {
    entities: &'a [&'a Class],
    duplicates: &'a HashSet<String>,
    repositories: &'a HashMap<String, Vec<(String, Location)>>,
    repositories_complete: bool,
    linked: &'a HashMap<String, Vec<Found>>,
    incoming: HashMap<&'a str, Vec<(&'a Class, &'a crate::model::Attribute, &'static str)>>,
}

impl<'a> Related<'a> {
    pub fn new(
        entities: &'a [&'a Class],
        duplicates: &'a HashSet<String>,
        repositories: &'a HashMap<String, Vec<(String, Location)>>,
        repositories_complete: bool,
        linked: &'a HashMap<String, Vec<Found>>,
    ) -> Self {
        let mut related = Self { entities, duplicates, repositories, repositories_complete, linked, incoming: HashMap::new() };
        for owner in entities {
            for attribute in &owner.attributes {
                let Some(role) = role(attribute.relation) else { continue };
                if let Target::Entity(target) = related.resolve(&attribute.target) {
                    related.incoming.entry(target.path.as_str()).or_default().push((owner, attribute, role));
                }
            }
        }
        related
    }

    fn resolve(&self, target: &str) -> Target<'a> {
        let simple = target.rsplit('.').next().unwrap_or(target);
        if self.duplicates.contains(simple) {
            return Target::Ambiguous;
        }
        let qualified_name = target.contains('.');
        match self.entities.iter().find(|e| e.name == simple && (!qualified_name || qualified(e) == target)) {
            Some(entity) => Target::Entity(entity),
            None => Target::Unknown,
        }
    }

    /// Group nodes and their children under `parent`, bounded per group and by `budget` in total. Returns whether anything was cut.
    pub fn nodes(
        &self,
        entity: &Class,
        parent: &str,
        expanded: &HashSet<String>,
        budget: &mut usize,
        items: &mut Vec<TreeItem>,
        locations: &mut Locations,
    ) -> bool {
        let at = (entity.path.clone(), entity.pos);
        let mut cut = false;
        let mut group = |kind: &str, label: &str, children: Vec<(String, String, String, &str, (String, Position))>,
                         items: &mut Vec<TreeItem>, locations: &mut Locations, budget: &mut usize| {
            if children.is_empty() {
                return;
            }
            let key = format!("{GROUP_PREFIX}{kind}:{}", entity.path);
            let total = children.len();
            let shown = total.min(MAX_GROUP_CHILDREN).min(*budget);
            cut |= shown < total;
            let detail = if shown < total { format!("{shown} of {total}") } else { total.to_string() };
            items.push(TreeItem::new(&key, label).parent(parent).icon("folder").detail(&detail).expandable(expanded.contains(&key)));
            locations.insert(key.clone(), at.clone());
            for (child_key, name, detail, icon, location) in children.into_iter().take(shown) {
                items.push(TreeItem::new(&child_key, &name).parent(&key).icon(icon).detail(&detail));
                locations.insert(child_key, location);
            }
            *budget -= shown;
        };

        let mut outgoing = Vec::new();
        for attribute in &entity.attributes {
            let Some(role) = role(attribute.relation) else { continue };
            let key = format!("{ARTIFACT_PREFIX}outgoing:{}#{}", entity.path, attribute.name);
            let detail = |state: &str| format!("{role} via {} · {state}", attribute.name);
            outgoing.push(match self.resolve(&attribute.target) {
                Target::Entity(target) => (key, target.name.clone(), detail("entity"), "db-foreign-key", (target.path.clone(), target.pos)),
                Target::Ambiguous => (key, attribute.target.clone(), detail("unknown: duplicate simple name"), "db-foreign-key", (entity.path.clone(), attribute.pos)),
                Target::Unknown => (key, attribute.target.clone(), detail("unknown: not a scanned entity"), "db-foreign-key", (entity.path.clone(), attribute.pos)),
            });
        }
        group("outgoing", "Refers to", outgoing, items, locations, budget);

        let mut incoming = Vec::new();
        if self.duplicates.contains(&entity.name) {
            let key = format!("{ARTIFACT_PREFIX}note:incoming-ambiguous:{}", entity.path);
            incoming.push((key, "Unknown".into(), "duplicated simple name; incoming associations not attributed".into(), "db-foreign-key", at.clone()));
        }
        for (owner, attribute, role) in self.incoming.get(entity.path.as_str()).into_iter().flatten() {
            let key = format!("{ARTIFACT_PREFIX}incoming:{}#{}", owner.path, attribute.name);
            let label = format!("{}.{}", owner.name, attribute.name);
            incoming.push((key, label, format!("{role} · owner scanned"), "db-foreign-key", (owner.path.clone(), attribute.pos)));
        }
        group("incoming", "Referenced by", incoming, items, locations, budget);

        let mut repositories = Vec::new();
        // The repository index keeps only the simple entity name, so a duplicated name cannot be attributed.
        let simple = self.repositories.get(&entity.name).filter(|_| !self.duplicates.contains(&entity.name));
        for (name, location) in simple.into_iter().flatten() {
            let key = format!("{ARTIFACT_PREFIX}repository:{}", location.path);
            let at = (location.path.clone(), location.range.start);
            repositories.push((key, name.clone(), "Spring Data repository".to_owned(), "db-properties", at));
        }
        repositories.sort_by(|a, b| a.1.cmp(&b.1));
        if self.duplicates.contains(&entity.name) {
            let unattributed = self.repositories.get(&entity.name).map_or(0, Vec::len);
            if unattributed > 0 {
                let key = format!("{ARTIFACT_PREFIX}note:repository-ambiguous:{}", entity.path);
                let detail = format!("{unattributed} repositories name the duplicated simple name {}; entity unknown", entity.name);
                repositories.push((key, "Unknown".into(), detail, "db-properties", at.clone()));
            }
        } else if !self.repositories_complete && repositories.is_empty() {
            let key = format!("{ARTIFACT_PREFIX}note:repository-incomplete:{}", entity.path);
            repositories.push((key, "Unknown".into(), "repository scan incomplete; refresh to continue".into(), "db-properties", at.clone()));
        }
        group("repositories", "Repositories", repositories, items, locations, budget);

        let mut dtos = Vec::new();
        if self.duplicates.contains(&entity.name) {
            if self.linked.get(&entity.name).is_some_and(|found| !found.is_empty()) {
                let key = format!("{ARTIFACT_PREFIX}note:dto-ambiguous:{}", entity.path);
                dtos.push((key, "Unknown".into(), "duplicated simple name; DTOs and projections not attributed".into(), "record", at.clone()));
            }
        } else {
            for found in self.linked.get(&entity.name).into_iter().flatten() {
                let key = format!("{ARTIFACT_PREFIX}linked:{}>{}", entity.path, found.path);
                let icon = if found.detail.starts_with("Projection") { "interface" } else { "record" };
                dtos.push((key, found.name.clone(), found.detail.clone(), icon, (found.path.clone(), found.pos)));
            }
        }
        group("dtos", "DTOs and Projections", dtos, items, locations, budget);

        cut
    }
}

#[cfg(test)]
mod tests {
    use crate::{JpaAssist, ENTITY_KEY_PREFIX};
    use openidea_plugin::mock::MockHost;
    use openidea_plugin::views::{Body, TreeItem, ViewEvent, Views};

    fn entity(package: &str, name: &str, body: &str) -> String {
        format!("package {package};\n@Entity\npublic class {name} {{\n    @Id\n    private Long id;\n{body}}}\n")
    }

    fn host() -> MockHost {
        let host = MockHost::install().with_capabilities(&["events", "project.read", "editor.read", "storage"]);
        host.add_file("src/shop/Order.java", &entity("shop", "Order", "    @ManyToOne\n    private Customer customer;\n    @ManyToOne\n    private Missing missing;\n"));
        host.add_file("src/shop/Customer.java", &entity("shop", "Customer", ""));
        host.add_file(
            "src/shop/data/OrderRepository.java",
            "package shop.data;\npublic interface OrderRepository extends JpaRepository<Order, Long> {}\n",
        );
        host
    }

    fn tree(plugin: &mut JpaAssist, host: &MockHost) -> (Vec<TreeItem>, Option<String>) {
        let view = plugin.render("structure", None, &host.invocation()).unwrap();
        let Body::Tree(items) = view.body else { panic!("a tree") };
        (items, view.status)
    }

    fn find<'a>(items: &'a [TreeItem], key: &str) -> &'a TreeItem {
        items.iter().find(|i| i.key == key).unwrap_or_else(|| panic!("{key} in {:?}", items.iter().map(|i| &i.key).collect::<Vec<_>>()))
    }

    #[test]
    fn related_artifacts_group_under_each_entity_and_open_their_files() {
        let host = host();
        let mut plugin = JpaAssist::default();
        let (items, _) = tree(&mut plugin, &host);
        let order = format!("{ENTITY_KEY_PREFIX}/project/src/shop/Order.java");
        let customer = format!("{ENTITY_KEY_PREFIX}/project/src/shop/Customer.java");

        let repositories = find(&items, "group:repositories:/project/src/shop/Order.java");
        assert_eq!(repositories.parent.as_deref(), Some(order.as_str()));
        let repository = find(&items, "artifact:repository:/project/src/shop/data/OrderRepository.java");
        assert_eq!(repository.parent.as_deref(), Some(repositories.key.as_str()));
        assert!(!items.iter().any(|i| i.key.starts_with("group:repositories:/project/src/shop/Customer")));

        let outgoing = find(&items, "artifact:outgoing:/project/src/shop/Order.java#customer");
        assert_eq!(outgoing.label, "Customer");
        let missing = find(&items, "artifact:outgoing:/project/src/shop/Order.java#missing");
        assert!(missing.detail.as_deref().unwrap().contains("unknown: not a scanned entity"));

        let incoming = find(&items, "artifact:incoming:/project/src/shop/Order.java#customer");
        assert_eq!(incoming.label, "Order.customer");
        assert_eq!(incoming.parent.as_deref(), Some("group:incoming:/project/src/shop/Customer.java"));
        assert!(items.iter().any(|i| i.key == customer));

        plugin.handle("structure", ViewEvent::Activate(repository.key.clone()), &host.invocation()).unwrap();
        plugin.handle("structure", ViewEvent::Activate(outgoing.key.clone()), &host.invocation()).unwrap();
        let opened = host.opened_locations();
        assert_eq!(opened[0].0, "/project/src/shop/data/OrderRepository.java");
        assert_eq!(opened[1].0, "/project/src/shop/Customer.java");
    }

    #[test]
    fn units_group_listed_entities_and_the_rest_is_explicitly_unassigned() {
        let host = host();
        host.add_file(
            "src/META-INF/persistence.xml",
            "<persistence><persistence-unit name=\"main\"><class>shop.Order</class></persistence-unit></persistence>",
        );
        let mut plugin = JpaAssist::default();
        let (items, status) = tree(&mut plugin, &host);
        let unit = items.iter().find(|i| i.key.starts_with("unit:") && i.label == "main").unwrap();
        let order = find(&items, "entity:/project/src/shop/Order.java");
        assert_eq!(order.parent.as_deref(), Some(unit.key.as_str()));
        let unassigned = find(&items, "unit:unassigned");
        assert!(unassigned.detail.as_deref().unwrap().contains("unknown"));
        assert_eq!(find(&items, "entity:/project/src/shop/Customer.java").parent.as_deref(), Some("unit:unassigned"));
        assert_eq!(status.as_deref(), Some("2 entities"));
    }

    #[test]
    fn entity_listed_in_two_units_is_ambiguous_not_guessed() {
        let host = host();
        host.add_file(
            "src/META-INF/persistence.xml",
            "<persistence><persistence-unit name=\"a\"><class>shop.Order</class></persistence-unit><persistence-unit name=\"b\"><class>shop.Order</class></persistence-unit></persistence>",
        );
        let mut plugin = JpaAssist::default();
        let (items, _) = tree(&mut plugin, &host);
        let order = find(&items, "entity:/project/src/shop/Order.java");
        assert_eq!(order.parent.as_deref(), Some("unit:ambiguous"));
        assert!(order.detail.as_deref().unwrap().contains("units: a, b"));
        assert_eq!(items.iter().filter(|i| i.key == order.key).count(), 1);
    }

    #[test]
    fn duplicate_simple_names_are_labelled_unknown_not_attributed() {
        let host = host();
        host.add_file("src/other/Customer.java", &entity("other", "Customer", ""));
        let mut plugin = JpaAssist::default();
        let (items, _) = tree(&mut plugin, &host);
        let outgoing = find(&items, "artifact:outgoing:/project/src/shop/Order.java#customer");
        assert!(outgoing.detail.as_deref().unwrap().contains("unknown: duplicate simple name"));
        assert!(!items.iter().any(|i| i.key.starts_with("artifact:incoming:")));
        let note = items.iter().find(|i| i.key.starts_with("artifact:note:incoming-ambiguous:")).unwrap();
        assert_eq!(note.label, "Unknown");
    }

    #[test]
    fn repositories_naming_a_duplicated_simple_name_are_unknown() {
        let host = host();
        host.add_file("src/other/Order.java", &entity("other", "Order", ""));
        let mut plugin = JpaAssist::default();
        let (items, _) = tree(&mut plugin, &host);
        assert!(!items.iter().any(|i| i.key.starts_with("artifact:repository:")));
        let note = items.iter().find(|i| i.key.starts_with("artifact:note:repository-ambiguous:")).unwrap();
        assert!(note.detail.as_deref().unwrap().contains("entity unknown"));
    }

    #[test]
    fn related_nodes_are_bounded_per_group() {
        let host = host();
        let fields: String = (0..30).map(|n| format!("    @ManyToOne\n    private Customer c{n};\n")).collect();
        host.add_file("src/shop/Order.java", &entity("shop", "Order", &fields));
        let mut plugin = JpaAssist::default();
        let (items, status) = tree(&mut plugin, &host);
        let group = find(&items, "group:outgoing:/project/src/shop/Order.java");
        assert_eq!(group.detail.as_deref(), Some("20 of 30"));
        assert_eq!(items.iter().filter(|i| i.parent.as_deref() == Some(group.key.as_str())).count(), 20);
        assert!(status.unwrap().contains("truncated"));
    }

    #[test]
    fn unread_repository_files_are_unknown_until_refresh_finishes_them() {
        let host = host();
        for n in 0..501 {
            host.add_file(&format!("src/bulk/Bulk{n}Repository.java"), "package bulk;\npublic interface Bulk {}\n");
        }
        let mut plugin = JpaAssist::default();
        let (items, status) = tree(&mut plugin, &host);
        assert!(status.unwrap().contains("repository scan incomplete"));
        let notes = items.iter().filter(|i| i.key.starts_with("artifact:note:repository-incomplete:")).count();
        let repositories = items.iter().filter(|i| i.key.starts_with("artifact:repository:")).count();
        assert!(notes + repositories >= 1);
        plugin.handle("structure", ViewEvent::Button("expand-all".into()), &host.invocation()).unwrap();
        let (_, status) = tree(&mut plugin, &host);
        assert!(!status.unwrap().contains("incomplete"));
    }

    const MONEY_CONVERTER: &str = "package shop.conv;\nimport jakarta.persistence.*;\n@Converter(autoApply = true)\npublic class MoneyConverter implements AttributeConverter<Money, Long> {}\n";
    const JSON_TYPE: &str = "package shop.conv;\npublic class JsonType implements UserType<Object> {}\n";
    const UNIT_XML: &str = "<persistence><persistence-unit name=\"main\"><class>shop.Order</class></persistence-unit></persistence>";

    #[test]
    fn converters_and_custom_types_group_under_each_unit_and_open_their_files() {
        let host = host();
        host.add_file("src/META-INF/persistence.xml", UNIT_XML);
        host.add_file("src/shop/conv/MoneyConverter.java", MONEY_CONVERTER);
        host.add_file("src/shop/conv/JsonType.java", JSON_TYPE);
        let mut plugin = JpaAssist::default();
        let (items, _) = tree(&mut plugin, &host);
        let unit = items.iter().find(|i| i.key.starts_with("unit:") && i.label == "main").unwrap();

        let converters = find(&items, &format!("group:converters:{}", unit.key));
        assert_eq!(converters.label, "JPA Converters");
        assert_eq!(converters.parent.as_deref(), Some(unit.key.as_str()));
        let converter = items.iter().find(|i| i.parent.as_deref() == Some(converters.key.as_str())).unwrap();
        assert_eq!(converter.label, "MoneyConverter");
        assert_eq!(converter.detail.as_deref(), Some("Money \u{2194} Long \u{b7} auto-apply"));
        let types = find(&items, &format!("group:usertypes:{}", unit.key));
        assert_eq!(types.label, "Hibernate Custom Types");
        let user_type = items.iter().find(|i| i.parent.as_deref() == Some(types.key.as_str())).unwrap();
        assert_eq!((user_type.label.as_str(), user_type.detail.as_deref()), ("JsonType", Some("UserType")));

        plugin.handle("structure", ViewEvent::Activate(converter.key.clone()), &host.invocation()).unwrap();
        plugin.handle("structure", ViewEvent::Activate(user_type.key.clone()), &host.invocation()).unwrap();
        plugin.handle("structure", ViewEvent::Activate(converters.key.clone()), &host.invocation()).unwrap();
        let opened = host.opened_locations();
        assert_eq!(opened.len(), 2);
        assert_eq!(opened[0].0, "/project/src/shop/conv/MoneyConverter.java");
        assert_eq!(opened[1].0, "/project/src/shop/conv/JsonType.java");
    }

    #[test]
    fn without_units_converters_sit_at_the_top_and_absent_ones_are_omitted() {
        let host = host();
        let mut plugin = JpaAssist::default();
        let (items, _) = tree(&mut plugin, &host);
        assert!(!items.iter().any(|i| i.key.starts_with("group:converters:") || i.key.starts_with("group:usertypes:")));
        assert!(!items.iter().any(|i| i.key.starts_with("group:dtos:") || i.key == "group:liquibase:"));

        host.add_file("src/shop/conv/MoneyConverter.java", MONEY_CONVERTER);
        let mut plugin = JpaAssist::default();
        let (items, _) = tree(&mut plugin, &host);
        let converters = find(&items, "group:converters:");
        assert_eq!(converters.parent, None);
        assert!(!items.iter().any(|i| i.key.starts_with("group:usertypes:")));
    }

    #[test]
    fn converter_group_is_bounded() {
        let host = host();
        for n in 0..25 {
            host.add_file(&format!("src/shop/conv/C{n}.java"), &format!("package shop.conv;\npublic class C{n} implements AttributeConverter<A, B> {{}}\n"));
        }
        let mut plugin = JpaAssist::default();
        let (items, status) = tree(&mut plugin, &host);
        let group = find(&items, "group:converters:");
        assert_eq!(group.detail.as_deref(), Some("20 of 25"));
        assert_eq!(items.iter().filter(|i| i.parent.as_deref() == Some(group.key.as_str())).count(), 20);
        assert!(status.unwrap().contains("truncated"));
    }

    #[test]
    fn linked_dtos_and_projections_group_beside_repositories() {
        let host = host();
        host.add_file("src/shop/dto/OrderDto.java", "package shop.dto;\npublic record OrderDto(Long id) {}\n");
        host.add_file("src/shop/dto/OrderProjection.java", "package shop.dto;\npublic interface OrderProjection {\n Long getId();\n}\n");
        host.add_file("src/shop/dto/GhostDto.java", "package shop.dto;\npublic record GhostDto(Long id) {}\n");
        host.add_file("src/shop/dto/CustomerDto.java", "package shop.dto;\npublic record SomethingElse(Long id) {}\n");
        let mut plugin = JpaAssist::default();
        let (items, _) = tree(&mut plugin, &host);
        let order = format!("{ENTITY_KEY_PREFIX}/project/src/shop/Order.java");

        let group = find(&items, "group:dtos:/project/src/shop/Order.java");
        assert_eq!((group.label.as_str(), group.parent.as_deref()), ("DTOs and Projections", Some(order.as_str())));
        let children: Vec<(&str, &str)> = items.iter().filter(|i| i.parent.as_deref() == Some(group.key.as_str())).map(|i| (i.label.as_str(), i.detail.as_deref().unwrap())).collect();
        assert_eq!(children, [("OrderDto", "DTO \u{b7} linked by class name"), ("OrderProjection", "Projection \u{b7} linked by class name")]);
        assert!(!items.iter().any(|i| i.key.starts_with("group:dtos:/project/src/shop/Customer")));
        assert!(!items.iter().any(|i| i.label == "GhostDto" || i.label == "SomethingElse"));

        let position = |key: &str| items.iter().position(|i| i.key == key).unwrap();
        assert!(position("group:repositories:/project/src/shop/Order.java") < position(&group.key));

        let dto = items.iter().find(|i| i.label == "OrderDto").unwrap();
        plugin.handle("structure", ViewEvent::Activate(dto.key.clone()), &host.invocation()).unwrap();
        assert_eq!(host.opened_locations()[0].0, "/project/src/shop/dto/OrderDto.java");
    }

    #[test]
    fn dtos_of_a_duplicated_entity_name_are_unknown() {
        let host = host();
        host.add_file("src/other/Order.java", &entity("other", "Order", ""));
        host.add_file("src/shop/dto/OrderDto.java", "package shop.dto;\npublic record OrderDto(Long id) {}\n");
        let mut plugin = JpaAssist::default();
        let (items, _) = tree(&mut plugin, &host);
        assert!(!items.iter().any(|i| i.label == "OrderDto"));
        assert!(items.iter().any(|i| i.key.starts_with("artifact:note:dto-ambiguous:")));
    }

    fn changelog(body: &str) -> String {
        format!("<databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\">{body}</databaseChangeLog>")
    }

    #[test]
    fn liquibase_group_lists_master_and_included_changelogs() {
        let host = host();
        host.add_file("src/main/resources/db/changelog/db.changelog-master.xml", &changelog("<include file=\"db/changelog/001-init.xml\"/>"));
        host.add_file("src/main/resources/db/changelog/001-init.xml", &changelog(""));
        host.add_file("src/main/resources/db/changelog/loose-changelog.yaml", "databaseChangeLog:\n  - changeSet: {}\n");
        host.add_file("docs/changelog-notes.xml", "<notes/>");
        let mut plugin = JpaAssist::default();
        let (items, _) = tree(&mut plugin, &host);
        let group = find(&items, "group:liquibase:");
        assert_eq!((group.label.as_str(), group.parent.as_deref(), group.detail.as_deref()), ("Liquibase", None, Some("3")));
        let children: Vec<(&str, &str)> = items.iter().filter(|i| i.parent.as_deref() == Some(group.key.as_str())).map(|i| (i.label.as_str(), i.detail.as_deref().unwrap())).collect();
        assert_eq!(
            children,
            [
                ("db.changelog-master.xml", "master \u{b7} 1 include entries"),
                ("001-init.xml", "included by db.changelog-master.xml"),
                ("loose-changelog.yaml", "changelog"),
            ]
        );
        let master = items.iter().find(|i| i.label == "db.changelog-master.xml").unwrap();
        plugin.handle("structure", ViewEvent::Activate(master.key.clone()), &host.invocation()).unwrap();
        assert_eq!(host.opened_locations()[0].0, "/project/src/main/resources/db/changelog/db.changelog-master.xml");
    }

    #[test]
    fn liquibase_group_is_bounded() {
        let host = host();
        for n in 0..60 {
            host.add_file(&format!("src/main/resources/db/changelog/{n:03}-changelog.xml"), &changelog(""));
        }
        let mut plugin = JpaAssist::default();
        let (items, _) = tree(&mut plugin, &host);
        let group = find(&items, "group:liquibase:");
        assert_eq!(group.detail.as_deref(), Some("50 of 51+"));
        assert_eq!(items.iter().filter(|i| i.parent.as_deref() == Some(group.key.as_str())).count(), 50);
    }

    const SENTINEL: &str = "S3NT1NEL-pw";

    fn profile(name: &str, url: &str) -> persistence_core::connection::Profile {
        persistence_core::connection::Profile {
            name: name.into(),
            dbms: persistence_core::connection::Dbms::PostgreSql,
            url_template: url.into(),
            user: Some("app".into()),
            driver_jar: None,
            schema_filter: Default::default(),
            spring_source: None,
        }
    }

    #[test]
    fn saved_connections_show_name_and_credential_free_target_never_a_password() {
        let host = host();
        let profiles = [profile("dev", "jdbc:postgresql://localhost:5432/scheduler?sslmode=require")];
        openidea_plugin::host::storage::set("jdbc.profiles", &persistence_core::connection::profiles_json(&profiles)).unwrap();
        let mut plugin = JpaAssist::default();
        let (items, _) = tree(&mut plugin, &host);
        let group = find(&items, "group:connections:");
        assert_eq!((group.label.as_str(), group.parent.as_deref(), group.detail.as_deref()), ("DB connections", None, Some("1")));
        let dev = find(&items, "connection:dev");
        assert_eq!(dev.parent.as_deref(), Some(group.key.as_str()));
        assert_eq!(dev.detail.as_deref(), Some("PostgreSQL: localhost:5432, database scheduler \u{b7} user app"));
        assert_eq!(dev.icon.as_deref(), Some("db-postgres"));
        for item in &items {
            assert!(!format!("{item:?}").contains(SENTINEL), "{}", item.key);
        }

        plugin.handle("structure", ViewEvent::Activate(dev.key.clone()), &host.invocation()).unwrap();
        let notes = host.notifications();
        assert_eq!(notes.len(), 1);
        assert!(notes[0].body.contains("localhost:5432, database scheduler") && !notes[0].body.contains(SENTINEL));
        assert!(host.opened_locations().is_empty());
    }

    #[test]
    fn a_stored_profile_with_a_password_in_its_url_is_unknown_and_never_shown() {
        let host = host();
        let profiles = [profile("dev", &format!("jdbc:postgresql://localhost:5432/scheduler?user=app&password={SENTINEL}"))];
        openidea_plugin::host::storage::set("jdbc.profiles", &persistence_core::connection::profiles_json(&profiles)).unwrap();
        let mut plugin = JpaAssist::default();
        let (items, _) = tree(&mut plugin, &host);
        assert_eq!(find(&items, "group:connections:").detail.as_deref(), Some("unknown"));
        assert_eq!(find(&items, "connection:none").label, "Unknown");
        for item in &items {
            assert!(!format!("{item:?}").contains(SENTINEL), "{}", item.key);
        }
    }

    #[test]
    fn empty_connections_group_says_so_and_activating_it_runs_the_manage_command() {
        let host = host();
        let mut plugin = JpaAssist::default();
        let (items, _) = tree(&mut plugin, &host);
        let hint = find(&items, "connection:none");
        assert_eq!(hint.label, "No saved connections");
        assert_eq!(hint.parent.as_deref(), Some("group:connections:"));

        let profiles = [profile("dev", "jdbc:postgresql://localhost:5432/scheduler")];
        openidea_plugin::host::storage::set("jdbc.profiles", &persistence_core::connection::profiles_json(&profiles)).unwrap();
        host.answer_next_prompt(Some(&["filter"]));
        plugin.handle("structure", ViewEvent::Activate("group:connections:".into()), &host.invocation()).unwrap();
        assert!(host.notifications().iter().any(|n| n.body.contains("Filter reset")));
    }

    #[test]
    fn expand_all_opens_the_new_groups() {
        let host = host();
        host.add_file("src/shop/conv/MoneyConverter.java", MONEY_CONVERTER);
        let mut plugin = JpaAssist::default();
        tree(&mut plugin, &host);
        plugin.handle("structure", ViewEvent::Button("expand-all".into()), &host.invocation()).unwrap();
        let (items, _) = tree(&mut plugin, &host);
        assert!(find(&items, "group:converters:").expandable);
        assert!(find(&items, "group:connections:").expandable);
    }
}
