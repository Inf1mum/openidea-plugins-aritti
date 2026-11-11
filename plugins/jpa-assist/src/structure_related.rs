//! Persistence tree grouping: persistence units and the artifacts related to each entity.
//! Purely lexical. A fact that cannot be established from scanned source is labelled unknown, never guessed.
use crate::model::{Class, Relation};
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
    incoming: HashMap<&'a str, Vec<(&'a Class, &'a crate::model::Attribute, &'static str)>>,
}

impl<'a> Related<'a> {
    pub fn new(
        entities: &'a [&'a Class],
        duplicates: &'a HashSet<String>,
        repositories: &'a HashMap<String, Vec<(String, Location)>>,
        repositories_complete: bool,
    ) -> Self {
        let mut related = Self { entities, duplicates, repositories, repositories_complete, incoming: HashMap::new() };
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
        group("outgoing", "Outgoing associations", outgoing, items, locations, budget);

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
        group("incoming", "Incoming associations", incoming, items, locations, budget);

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
}
