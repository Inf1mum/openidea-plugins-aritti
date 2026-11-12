//! Persistence tree actions: the toolbar Add menu, the New submenu of the item context menu, persistence unit creation
//! and the persisted group filter. Creation reuses the Project view flows (`tree_create_action`); nothing here writes a file
//! except through that reviewed, non-overwriting apply step.
use super::*;
use crate::structure_related::{parse_units, qualified};
use std::cell::RefCell;

const KINDS: [(&str, &str, &str); 7] = [
    ("structure-new-entity", "Entity", "tree-new-entity"),
    ("structure-new-converter", "JPA Converter", "tree-new-converter"),
    ("structure-new-user-type", "Hibernate Custom Type", "tree-new-user-type"),
    ("structure-new-repository", "Spring Data Repository", "tree-new-repository"),
    ("structure-new-dto", "DTO", "tree-new-dto"),
    ("structure-new-projection", "Projection", "tree-new-projection"),
    ("structure-new-persistence-unit", "Persistence Unit", ""),
];

const HIDEABLE: [(&str, &str); 9] = [
    ("outgoing", "Refers to"),
    ("incoming", "Referenced by"),
    ("repositories", "Repositories"),
    ("dtos", "DTOs and Projections"),
    ("usertypes", "Hibernate Custom Types"),
    ("converters", "JPA Converters"),
    ("liquibase", "Liquibase"),
    ("connections", "DB connections"),
    ("unassigned", "Unassigned entities"),
];

const HIDDEN_KEY: &str = "jpaAssist.structure.hiddenGroups";
const MAX_UNIT_NAME: usize = 64;

thread_local! {
    static SELECTED: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub(super) fn select(key: String) {
    SELECTED.with(|selected| *selected.borrow_mut() = Some(key));
}

fn selected() -> Option<String> {
    SELECTED.with(|selected| selected.borrow().clone())
}

/// The source file a tree item stands for: its entity, repository, converter or type file.
fn source_path(key: &str) -> Option<String> {
    let rest = ["entity:", "attribute:", "group:", "artifact:"].iter().find_map(|prefix| key.strip_prefix(prefix))?;
    let start = rest.find('/')?;
    let end = rest[start..].find(['#', '>']).map_or(rest.len(), |at| start + at);
    path(&rest[start..end])
}

fn refresh(plugin: &mut JpaAssist, ctx: &Invocation) -> Result<(), Error> {
    plugin.handle("structure", ViewEvent::Button("refresh".into()), ctx)
}

pub(super) fn button(plugin: &mut JpaAssist, id: &str, ctx: &Invocation) -> Result<(), Error> {
    let outcome = match id {
        "add" => run(plugin, "structure-add", ctx, None).map(|_| ()),
        _ => choose_hidden().map(|_| host::refresh_view("structure")),
    };
    match outcome {
        Err(Error::Cancelled) => Ok(()),
        other => other,
    }
}

pub(super) fn run(plugin: &mut JpaAssist, id: &str, ctx: &Invocation, arg: Option<&str>) -> Result<Option<String>, Error> {
    let key = arg.map(str::to_owned).or_else(selected);
    let id = if id == "structure-add" {
        let items: Vec<PickItem> = KINDS.iter().map(|(command, label, _)| PickItem::new(command, label)).collect();
        match host::pick("Add to the persistence structure", &items)? {
            Some(command) => command,
            None => return Ok(None),
        }
    } else {
        id.to_owned()
    };
    let (_, _, tree_command) = KINDS.iter().find(|(command, ..)| *command == id).ok_or_else(|| Error::NotFound(format!("command {id}")))?;
    if tree_command.is_empty() {
        new_persistence_unit(plugin, ctx)?;
        return refresh(plugin, ctx).map(|_| None);
    }

    let from_item = key.as_deref().and_then(source_path);
    let target = match (from_item, ctx.editor().map(|editor| editor.path())) {
        (Some(file), _) | (None, Some(file)) => file,
        (None, None) if id == "structure-new-entity" => {
            let project = ctx.project().ok_or("Open a project first.")?;
            host::pick_directory("Folder for the new entity", Some(&project.root()?))?.ok_or(Error::Cancelled)?
        }
        (None, None) => return Err("Select an entity in Persistence or open its source file first.".into()),
    };
    tree_create_action::run_on(plugin, tree_command, target)?;
    refresh(plugin, ctx).map(|_| None)
}

fn hidden() -> Vec<String> {
    host::storage::get(HIDDEN_KEY).ok().flatten().map(|text| text.split(',').filter(|name| !name.is_empty()).map(str::to_owned).collect()).unwrap_or_default()
}

fn choose_hidden() -> Result<(), Error> {
    let items: Vec<PickItem> = HIDEABLE.iter().map(|(kind, label)| PickItem::new(kind, label)).collect();
    let Some(chosen) = host::pick_many("Hide these groups in Persistence (none shows every group)", &items)? else {
        return Err(Error::Cancelled);
    };
    host::storage::set(HIDDEN_KEY, &chosen.join(","))
}

/// Drops the hidden groups and everything below them.
pub(super) fn filter(items: Vec<TreeItem>) -> Vec<TreeItem> {
    let hidden = hidden();
    if hidden.is_empty() {
        return items;
    }
    let mut dropped: HashSet<String> = HashSet::new();
    let mut kept = Vec::new();
    for item in items {
        let kind = match item.key.strip_prefix("group:") {
            Some(rest) => rest.split(':').next().unwrap_or_default(),
            None => item.key.strip_prefix("unit:").filter(|name| *name == "unassigned").unwrap_or_default(),
        };
        let below_dropped = item.parent.as_ref().is_some_and(|parent| dropped.contains(parent));
        if below_dropped || hidden.iter().any(|name| name == kind) {
            dropped.insert(item.key);
            continue;
        }
        kept.push(item);
    }
    kept
}

fn valid_unit_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= MAX_UNIT_NAME && name.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

fn unit_element(name: &str, classes: &[String]) -> String {
    let listed: String = classes.iter().map(|class| format!("        <class>{class}</class>\n")).collect();
    format!("    <persistence-unit name=\"{name}\">\n{listed}    </persistence-unit>\n")
}

fn new_document(namespace: &str, unit: &str) -> String {
    let (xmlns, version, schema) = if namespace == "javax" {
        ("http://xmlns.jcp.org/xml/ns/persistence", "2.2", "persistence_2_2.xsd")
    } else {
        ("https://jakarta.ee/xml/ns/persistence", "3.0", "persistence_3_0.xsd")
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<persistence xmlns=\"{xmlns}\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"\n             xsi:schemaLocation=\"{xmlns} {xmlns}/{schema}\"\n             version=\"{version}\">\n{unit}</persistence>\n"
    )
}

fn closing_edit(text: &str, unit: &str) -> Result<TextEdit, Error> {
    let closing = text.rfind("</persistence>").ok_or("The persistence.xml has no closing </persistence> tag; it was not changed.")?;
    let line_start = text[..closing].rfind('\n').map_or(0, |at| at + 1);
    let (at, new_text) = if text[line_start..closing].trim().is_empty() { (line_start, unit.to_owned()) } else { (closing, format!("\n{unit}")) };
    let position = crate::position(text, at);
    Ok(TextEdit { range: Range { start: position, end: position }, text: new_text })
}

fn new_persistence_unit(plugin: &mut JpaAssist, ctx: &Invocation) -> Result<(), Error> {
    let project = ctx.project().ok_or("Open a project first.")?;
    let root = path(&project.root()?).ok_or(Error::Stale)?;
    let mut files = project.find_files("**/persistence.xml", 20).unwrap_or_default();
    files.retain(|file| inside(&root, file));
    files.sort();
    let existing = match files.as_slice() {
        [] => None,
        [one] => Some(one.clone()),
        many => {
            let items: Vec<PickItem> = many.iter().map(|file| PickItem::new(file, &file[root.trim_end_matches('/').len() + 1..])).collect();
            Some(host::pick("Add the persistence unit to", &items)?.ok_or(Error::Cancelled)?)
        }
    };
    let text = existing.as_deref().map(|file| project.read_file(file)).transpose()?;
    let target = existing.unwrap_or_else(|| format!("{root}/src/main/resources/META-INF/persistence.xml"));

    let Some(name) = host::input("Persistence unit name", Some("default"))? else {
        return Err(Error::Cancelled);
    };
    let name = name.trim().to_owned();
    if !valid_unit_name(&name) {
        return Err("Use letters, digits, '.', '_' or '-' for the persistence unit name.".into());
    }
    if text.as_deref().is_some_and(|text| parse_units(&target, text).iter().any(|unit| unit.name == name)) {
        return Err(format!("{target} already declares a persistence unit named {name}.").into());
    }

    plugin.scan(&project)?;
    let mut candidates: Vec<String> = plugin.entities().iter().map(|class| qualified(class)).collect();
    candidates.sort();
    candidates.dedup();
    let classes = if candidates.is_empty() {
        Vec::new()
    } else {
        let items: Vec<PickItem> = candidates.iter().map(|class| PickItem::new(class, class)).collect();
        host::pick_many("Entities listed in the persistence unit", &items)?.ok_or(Error::Cancelled)?
    };

    let unit = unit_element(&name, &classes);
    let action = match &text {
        Some(text) => FileAction {
            title: format!("Add persistence unit {name}"),
            edits: vec![(target.clone(), vec![closing_edit(text, &unit)?])],
            open: Some(target.clone()),
            ..FileAction::default()
        },
        None => {
            let items = [PickItem::new("jakarta", "Jakarta Persistence"), PickItem::new("javax", "Javax Persistence")];
            let namespace = host::pick("Persistence namespace for the new persistence.xml", &items)?.ok_or(Error::Cancelled)?;
            FileAction {
                title: format!("Create persistence unit {name}"),
                operations: vec![FileOperation::Create { path: target.clone(), text: new_document(&namespace, &unit), overwrite: false }],
                open: Some(target.clone()),
                ..FileAction::default()
            }
        }
    };
    let shown = match &action.operations[..] {
        [FileOperation::Create { text, .. }] => text.clone(),
        _ => unit,
    };
    if !ui_kit::prompts::confirm_exact("Add persistence unit?", &format!("Target: {target}\n\n{shown}\nNo existing text is replaced."))? {
        return Err(Error::Cancelled);
    }
    tree_create_action::apply(&project, &root, action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ORDER: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Order {\n    @Id Long id;\n    String number;\n}\n";
    const REPOSITORY: &str = "package shop.data;\nimport org.springframework.data.jpa.repository.JpaRepository;\npublic interface OrderRepository extends JpaRepository<Order, Long> {\n}\n";
    const PERSISTENCE: &str = "<persistence version=\"3.0\">\n    <persistence-unit name=\"main\">\n        <class>shop.Order</class>\n    </persistence-unit>\n</persistence>\n";

    fn host() -> MockHost {
        let host = MockHost::install().with_capabilities(&["project.read", "project.write", "editor.read", "storage"]);
        host.add_file("src/main/java/shop/Order.java", ORDER);
        host.add_file("src/main/java/shop/data/OrderRepository.java", REPOSITORY);
        host
    }

    fn text_of(host: &MockHost, relative: &str) -> Option<String> {
        host.invocation().project().unwrap().read_file(&format!("/project/{relative}")).ok()
    }

    fn answers(host: &MockHost, rows: &[&[&str]]) {
        for row in rows {
            host.answer_next_prompt(Some(row));
        }
    }

    fn tree(plugin: &mut JpaAssist, host: &MockHost) -> Vec<TreeItem> {
        let Body::Tree(items) = plugin.render("structure", None, &host.invocation()).unwrap().body else { panic!("tree expected") };
        items
    }

    #[test]
    fn item_keys_map_to_their_source_file() {
        assert_eq!(source_path("entity:/p/A.java").as_deref(), Some("/p/A.java"));
        assert_eq!(source_path("attribute:/p/A.java#id").as_deref(), Some("/p/A.java"));
        assert_eq!(source_path("group:repositories:/p/A.java").as_deref(), Some("/p/A.java"));
        assert_eq!(source_path("artifact:linked:/p/A.java>/p/ADto.java").as_deref(), Some("/p/A.java"));
        assert_eq!(source_path("artifact:repository:/p/R.java").as_deref(), Some("/p/R.java"));
        assert_eq!(source_path("group:liquibase:"), None);
        assert_eq!(source_path("unit:/p/persistence.xml#main"), None);
    }

    #[test]
    fn toolbar_add_offers_every_creatable_kind_and_creates_a_repository() {
        let host = host();
        let mut plugin = JpaAssist::default();
        tree(&mut plugin, &host);
        select("entity:/project/src/main/java/shop/Order.java".into());
        answers(&host, &[&["structure-new-repository"], &["JpaRepository"], &["no"], &["OrderRepository2"], &[]]);
        plugin.handle("structure", ViewEvent::Button("add".into()), &host.invocation()).unwrap();
        let written = text_of(&host, "src/main/java/shop/OrderRepository2.java").unwrap();
        assert!(written.contains("interface OrderRepository2 extends JpaRepository<Order, Long>"), "{written}");
        assert!(host.refreshed_views().iter().any(|view| view == "structure"));
    }

    #[test]
    fn add_menu_choice_can_be_dismissed_without_writing() {
        let host = host();
        let mut plugin = JpaAssist::default();
        host.answer_next_prompt(None);
        plugin.handle("structure", ViewEvent::Button("add".into()), &host.invocation()).unwrap();
        assert!(text_of(&host, "src/main/java/shop/OrderRepository2.java").is_none());
    }

    #[test]
    fn context_menu_new_entity_uses_the_folder_of_the_item() {
        let host = host();
        let mut plugin = JpaAssist::default();
        answers(&host, &[&["jakarta"], &["Tag"], &["tag"], &["IDENTITY"], &["version"], &[]]);
        plugin.run_command("structure-new-entity", &host.invocation(), Some("entity:/project/src/main/java/shop/Order.java")).unwrap();
        assert!(text_of(&host, "src/main/java/shop/Tag.java").unwrap().contains("@Table(name = \"tag\")"));
    }

    #[test]
    fn flows_without_an_item_or_editor_say_what_to_select() {
        let host = host();
        let mut plugin = JpaAssist::default();
        let error = plugin.run_command("structure-new-converter", &host.invocation(), Some("group:liquibase:")).unwrap_err();
        assert!(format!("{error:?}").contains("Select an entity"), "{error:?}");
        assert!(plugin.run_command("structure-new-unknown", &host.invocation(), None).is_err());
    }

    #[test]
    fn new_persistence_unit_edits_the_existing_file_once() {
        let host = host();
        host.add_file("src/main/resources/META-INF/persistence.xml", PERSISTENCE);
        let mut plugin = JpaAssist::default();
        answers(&host, &[&["second"], &["shop.Order"], &[]]);
        plugin.run_command("structure-new-persistence-unit", &host.invocation(), None).unwrap();
        let after = text_of(&host, "src/main/resources/META-INF/persistence.xml").unwrap();
        assert!(after.starts_with(&PERSISTENCE[..PERSISTENCE.len() - "</persistence>\n".len()]), "{after}");
        assert!(after.contains("<persistence-unit name=\"second\">\n        <class>shop.Order</class>"), "{after}");
        assert!(after.ends_with("</persistence>\n"));
        let units = parse_units("x", &after);
        assert_eq!(units.iter().map(|unit| unit.name.as_str()).collect::<Vec<_>>(), ["main", "second"]);
    }

    #[test]
    fn new_persistence_unit_creates_a_missing_file_without_overwriting() {
        let host = host();
        let mut plugin = JpaAssist::default();
        answers(&host, &[&["default"], &["shop.Order"], &["jakarta"], &[]]);
        plugin.run_command("structure-new-persistence-unit", &host.invocation(), None).unwrap();
        let created = text_of(&host, "src/main/resources/META-INF/persistence.xml").unwrap();
        assert!(created.contains("https://jakarta.ee/xml/ns/persistence") && created.contains("<persistence-unit name=\"default\">"), "{created}");
    }

    #[test]
    fn persistence_unit_refusals_leave_files_untouched() {
        let host = host();
        host.add_file("src/main/resources/META-INF/persistence.xml", PERSISTENCE);
        let mut plugin = JpaAssist::default();
        answers(&host, &[&["main"]]);
        assert!(plugin.run_command("structure-new-persistence-unit", &host.invocation(), None).is_err());
        answers(&host, &[&["bad name"]]);
        assert!(plugin.run_command("structure-new-persistence-unit", &host.invocation(), None).is_err());
        answers(&host, &[&["third"], &["shop.Order"]]);
        host.answer_next_prompt(None);
        assert!(plugin.run_command("structure-new-persistence-unit", &host.invocation(), None).is_err());
        assert_eq!(text_of(&host, "src/main/resources/META-INF/persistence.xml").unwrap(), PERSISTENCE);
        host.add_file("src/main/resources/META-INF/persistence.xml", "<persistence>");
        answers(&host, &[&["third"], &["shop.Order"], &[]]);
        assert!(plugin.run_command("structure-new-persistence-unit", &host.invocation(), None).is_err());
    }

    #[test]
    fn filter_hides_chosen_groups_with_their_children_and_remembers_the_choice() {
        let host = host();
        let mut plugin = JpaAssist::default();
        let labels = |items: &[TreeItem]| items.iter().map(|item| item.label.clone()).collect::<Vec<_>>();
        let before = labels(&tree(&mut plugin, &host));
        assert!(before.contains(&"Repositories".to_owned()) && before.contains(&"OrderRepository".to_owned()), "{before:?}");
        answers(&host, &[&["repositories", "connections"]]);
        plugin.handle("structure", ViewEvent::Button("filter".into()), &host.invocation()).unwrap();
        let after = labels(&tree(&mut JpaAssist::default(), &host));
        assert!(!after.contains(&"Repositories".to_owned()) && !after.contains(&"OrderRepository".to_owned()), "{after:?}");
        assert!(!after.contains(&"DB connections".to_owned()) && after.contains(&"Order".to_owned()), "{after:?}");
        answers(&host, &[&[]]);
        plugin.handle("structure", ViewEvent::Button("filter".into()), &host.invocation()).unwrap();
        assert_eq!(labels(&tree(&mut JpaAssist::default(), &host)), before);
    }

    #[test]
    fn hiding_unassigned_entities_drops_their_subtree() {
        let host = host();
        host.add_file("src/main/resources/META-INF/persistence.xml", "<persistence><persistence-unit name=\"main\"><class>shop.Other</class></persistence-unit></persistence>");
        host.add_file("src/main/java/shop/Other.java", "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Other {\n    @Id Long id;\n}\n");
        let mut plugin = JpaAssist::default();
        assert!(tree(&mut plugin, &host).iter().any(|item| item.key == "unit:unassigned"));
        answers(&host, &[&["unassigned"]]);
        plugin.handle("structure", ViewEvent::Button("filter".into()), &host.invocation()).unwrap();
        let after = tree(&mut JpaAssist::default(), &host);
        assert!(after.iter().all(|item| item.key != "unit:unassigned" && item.label != "Order"), "{:?}", after.iter().map(|i| &i.label).collect::<Vec<_>>());
        assert!(after.iter().any(|item| item.label == "Other"));
    }

    #[test]
    fn toolbar_offers_add_and_filter() {
        let host = host();
        let view = JpaAssist::default().render("structure", None, &host.invocation()).unwrap();
        let ids: Vec<&str> = view.toolbar.iter().map(|button| button.id.as_str()).collect();
        assert_eq!(&ids[..2], ["add", "refresh"]);
        assert!(ids.contains(&"filter"));
    }

    #[test]
    fn without_the_write_capability_the_add_flows_write_nothing() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read", "storage"]);
        host.add_file("src/main/java/shop/Order.java", ORDER);
        let mut plugin = JpaAssist::default();
        answers(&host, &[&["default"], &["shop.Order"], &["jakarta"], &[]]);
        assert!(plugin.run_command("structure-new-persistence-unit", &host.invocation(), None).is_err());
        assert!(text_of(&host, "src/main/resources/META-INF/persistence.xml").is_none());
    }
}
