//! Reviewed creation of a new sibling entity together with the relationship attribute on both sides.
//!
//! "Relation to" puts the chosen cardinality on the invoking entity, pointing at the new one; "relation from" puts it on the
//! new entity, pointing at the invoking one. The other side gets the matching inverse. Only the invoking document is edited;
//! the new entity is a non-overwriting Create in the same folder and package.
use super::*;
use crate::wizard::plural;

pub(super) const TITLE_TO: &str = "Create related entity (relation to)…";
pub(super) const TITLE_FROM: &str = "Create related entity (relation from)…";

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Direction {
    To,
    From,
}

pub(super) fn eligible(doc: &Document) -> bool {
    current_class(doc).is_some_and(|class| class.kind == Kind::Entity)
}

fn inverse(relation: Relation) -> Relation {
    match relation {
        Relation::ManyToOne => Relation::OneToMany,
        Relation::OneToMany => Relation::ManyToOne,
        other => other,
    }
}

fn default_name(entity: &str, relation: Relation) -> String {
    let single = decapitalize(entity);
    if relation.to_many() { plural(&single) } else { single }
}

fn spec(name: String, target: &str, relation: Relation, mapped_by: Option<String>) -> AttributeSpec {
    AttributeSpec {
        name,
        transient: false,
        ty: target.to_owned(),
        relation,
        enumerated: false,
        not_null: false,
        unique: false,
        options: RelationOptions::default(),
        mapped_by,
        imports: Vec::new(),
    }
}

struct Plan {
    holder: AttributeSpec,
    other: AttributeSpec,
}

/// The attribute on the entity the cardinality is read from (`holder`) and its inverse (`other`); `holder_owns` says which one
/// carries the join mapping. ManyToOne is always owned by its holder and OneToMany by the other side.
fn plan(
    holder_entity: &str,
    other_entity: &str,
    relation: Relation,
    holder_name: String,
    other_name: String,
    holder_owns: bool,
) -> Result<Plan, Error> {
    let (holder_by, other_by) = if holder_owns { (None, Some(holder_name.clone())) } else { (Some(other_name.clone()), None) };
    let holder = spec(holder_name, other_entity, relation, holder_by);
    let other = spec(other_name, holder_entity, inverse(relation), other_by);
    for side in [&holder, &other] {
        side.options.validate(side.relation, false, side.mapped_by.is_some()).map_err(Error::from)?;
    }
    Ok(Plan { holder, other })
}

fn ask_cardinality(direction: Direction, this: &str, new: &str) -> Result<Relation, Error> {
    let (holder, other) = if direction == Direction::To { (this, new) } else { (new, this) };
    let items = [
        PickItem::new("ManyToOne", "ManyToOne").description(&format!("many {holder} to one {other}")),
        PickItem::new("OneToMany", "OneToMany").description(&format!("one {holder} to many {other}")),
        PickItem::new("OneToOne", "OneToOne").description(&format!("one {holder} to one {other}")),
        PickItem::new("ManyToMany", "ManyToMany").description(&format!("many {holder} to many {other}")),
    ];
    let Some(key) = host::pick("Cardinality of the new relationship", &items)? else {
        return Err(Error::Cancelled);
    };
    Ok(match key.as_str() {
        "ManyToOne" => Relation::ManyToOne,
        "OneToMany" => Relation::OneToMany,
        "OneToOne" => Relation::OneToOne,
        "ManyToMany" => Relation::ManyToMany,
        _ => return Err(Error::Stale),
    })
}

fn ask_attribute(prompt: &str, proposal: &str) -> Result<String, Error> {
    let Some(answer) = host::input(prompt, Some(proposal))? else {
        return Err(Error::Cancelled);
    };
    let answer = decapitalize(answer.trim());
    if !identifier(&answer) {
        return Err(format!("\"{answer}\" isn't a valid attribute name.").into());
    }
    Ok(answer)
}

fn ask_owner(holder: &str, other: &str) -> Result<bool, Error> {
    let items = [
        PickItem::new("holder", &format!("{holder} owns the relationship")),
        PickItem::new("other", &format!("{other} owns the relationship")),
    ];
    let Some(key) = host::pick("Owning side", &items)? else {
        return Err(Error::Cancelled);
    };
    Ok(key == "holder")
}

fn edits_for(class: &Class, text: &str, spec: &AttributeSpec) -> Result<Vec<TextEdit>, Error> {
    if class.attributes.iter().any(|attribute| attribute.name == spec.name) {
        return Err(format!("{} already has an attribute named {}.", class.name, spec.name).into());
    }
    let edits = attribute_edits(class, text, &mask(text), spec).ok_or(Error::Stale)?;
    Ok(edits.iter().map(|edit| edit.to_text_edit(text)).collect())
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project, direction: Direction) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let class = current_class(doc).filter(|class| class.kind == Kind::Entity).ok_or(Error::Stale)?;
    let ns = namespace(&doc.text, &class).ok_or(Error::Stale)?;
    let declared = package(doc).ok_or(Error::Stale)?;
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;

    let Some(name) = ask_identifier("Related entity class name")? else {
        return Err(Error::Cancelled);
    };
    let name = capitalize(&name);
    if !identifier(&name) || name == class.name || name == "Long" || class.imports.iter().any(|import| import.rsplit('.').next() == Some(name.as_str())) {
        return Err("Choose a new class name that does not clash with this entity, a generated identifier type or an import.".into());
    }
    no_collision(project, &pending.root, directory, &name, None)?;

    let (holder_entity, other_entity) = if direction == Direction::To { (class.name.as_str(), name.as_str()) } else { (name.as_str(), class.name.as_str()) };
    let relation = ask_cardinality(direction, &class.name, &name)?;
    let holder_name = ask_attribute(&format!("Attribute on {holder_entity}"), &default_name(other_entity, relation))?;
    let other_name = ask_attribute(&format!("Inverse attribute on {other_entity}"), &default_name(holder_entity, inverse(relation)))?;
    let holder_owns = match relation {
        Relation::OneToOne | Relation::ManyToMany => ask_owner(holder_entity, other_entity)?,
        _ => relation == Relation::ManyToOne,
    };
    let Plan { holder, other } = plan(holder_entity, other_entity, relation, holder_name, other_name, holder_owns)?;
    let (existing_spec, created_spec) = if direction == Direction::To { (holder, other) } else { (other, holder) };

    let kotlin = doc.language == "kotlin";
    let table = naming_table_name(&name);
    let bare = entity_source(&EntitySpec {
        role: persistence_core::PersistenceRole::Entity,
        name: name.clone(),
        table,
        package: Some(declared),
        kotlin,
        id: "IDENTITY".into(),
        auditing: false,
        version: false,
        equals: false,
    });
    let target = format!("{directory}/{name}.{}", if kotlin { "kt" } else { "java" });
    let created = parse_class(&bare, &target).ok_or(Error::Stale)?;
    let created_edits = edits_for(&created, &bare, &created_spec)?;
    let source = patched(&bare, &created_edits)?;
    let source = if ns == "javax.persistence" { source.replace("jakarta.persistence.", "javax.persistence.") } else { source };
    let existing_edits = edits_for(&class, &doc.text, &existing_spec)?;
    let after = checked_after(doc, &existing_edits)?;
    let (existing_edits, after) = super::association_optimize::apply(doc, existing_edits, after)?;
    let source = super::association_optimize::optimize(&target, &doc.language, &bare, &source)?;
    if path(&target).is_none() || source.len() > MAX_TEXT || parse_class(&source, &target).is_none() {
        return Err("Generated target exceeds source/path bounds.".into());
    }

    let heading = if direction == Direction::To { "relation to" } else { "relation from" };
    if !ui_kit::prompts::confirm_exact(
        "Create related entity?",
        &format!(
            "Namespace: {ns}\nNew entity ({heading}): {target}\n\n{source}\nEdited entity: {} (version {})\n\n{after}{}{}\nOne host-managed undoable edit; no existing file is overwritten.",
            doc.path, doc.version,
            super::association_advice::note(&doc.path, &doc.language, &doc.text, &after),
            super::association_advice::note(&target, &doc.language, &bare, &source)
        ),
    )? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    no_collision(project, &pending.root, directory, &name, None)?;
    Ok(FileAction {
        title: format!("Create related entity {name}"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target, text: source, overwrite: false }],
        edits: vec![(doc.path.clone(), existing_edits)],
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const PATH: &str = "/project/src/main/java/shop/Order.java";
    const ORDER: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Order {\n    @Id Long id;\n}\n";

    fn setup(path: &str, text: &str, language: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(path, language, text, &[]);
        let doc = Document { path: path.into(), language: language.into(), text: text.into(), version: 1 };
        (host, JpaAssist::default(), doc)
    }

    fn offered(plugin: &mut JpaAssist, doc: &Document, title: &str) -> FileAction {
        plugin.file_actions(doc, Range::default()).into_iter().find(|action| action.title == title).unwrap()
    }

    fn answers(host: &MockHost, rows: &[&[&str]]) {
        for row in rows {
            host.answer_next_prompt(Some(row));
        }
    }

    fn created(result: &FileAction) -> (&str, &str) {
        let [FileOperation::Create { path, text, overwrite: false }] = result.operations.as_slice() else { panic!("one non-overwriting create") };
        (path, text)
    }

    #[test]
    fn relation_to_puts_the_owning_many_to_one_on_the_invoking_entity() {
        let (host, mut plugin, doc) = setup(PATH, ORDER, "java");
        let action = offered(&mut plugin, &doc, TITLE_TO);
        assert!(action.operations.is_empty() && action.edits.is_empty());
        answers(&host, &[&["Customer"], &["ManyToOne"], &["customer"], &["orders"], &[]]);
        let result = plugin.resolve_file_action(&doc, action).unwrap();

        let (path, text) = created(&result);
        assert_eq!(path, "/project/src/main/java/shop/Customer.java");
        assert!(text.contains("@OneToMany(mappedBy = \"customer\")") && text.contains("List<Order> orders"), "{text}");
        assert!(text.contains("@Table(name = \"customer\")") && text.contains("package shop;"), "{text}");
        let [(edited, edits)] = result.edits.as_slice() else { panic!("one edited document") };
        assert_eq!(edited, PATH);
        let after = patched(ORDER, edits).unwrap();
        assert!(after.contains("@ManyToOne") && after.contains("Customer customer") && !after.contains("mappedBy"), "{after}");
        assert_eq!(host.editor_text(), ORDER);
        assert!(parse_class(text, path).is_some_and(|class| class.attributes.iter().any(|a| a.name == "orders")));
    }

    #[test]
    fn relation_from_puts_the_attribute_on_the_new_entity() {
        let (host, mut plugin, doc) = setup(PATH, ORDER, "java");
        let action = offered(&mut plugin, &doc, TITLE_FROM);
        answers(&host, &[&["Shipment"], &["ManyToOne"], &["order"], &["shipments"], &["keep"], &[]]);
        let result = plugin.resolve_file_action(&doc, action).unwrap();

        let (_, text) = created(&result);
        assert!(text.contains("@ManyToOne") && text.contains("Order order"), "{text}");
        let after = patched(ORDER, &result.edits[0].1).unwrap();
        assert!(after.contains("@OneToMany(mappedBy = \"order\")") && after.contains("List<Shipment> shipments"), "{after}");
    }

    #[test]
    fn one_to_many_is_owned_by_the_new_side_and_one_to_one_asks_for_the_owner() {
        let (host, mut plugin, doc) = setup(PATH, ORDER, "java");
        let action = offered(&mut plugin, &doc, TITLE_TO);
        answers(&host, &[&["Line"], &["OneToMany"], &["lines"], &["order"], &["keep"], &[]]);
        let result = plugin.resolve_file_action(&doc, action).unwrap();
        assert!(created(&result).1.contains("@ManyToOne") && created(&result).1.contains("Order order"));
        assert!(patched(ORDER, &result.edits[0].1).unwrap().contains("@OneToMany(mappedBy = \"order\")"));

        for (owner, on_existing) in [("holder", "@OneToOne\n"), ("other", "mappedBy = \"order\"")] {
            let action = offered(&mut plugin, &doc, TITLE_TO);
            let owner_answer: &[&str] = &[owner];
            let confirmations: &[&[&str]] = if owner == "holder" { &[&[]] } else { &[&["keep"], &[]] };
            answers(&host, &[&["Invoice"], &["OneToOne"], &["invoice"], &["order"], owner_answer]);
            answers(&host, confirmations);
            let result = plugin.resolve_file_action(&doc, action).unwrap();
            let after = patched(ORDER, &result.edits[0].1).unwrap();
            assert!(after.contains(on_existing), "{owner}: {after}");
            assert_eq!(after.contains("mappedBy"), owner == "other");
            assert_eq!(created(&result).1.contains("mappedBy"), owner == "holder");
        }
    }

    #[test]
    fn many_to_many_uses_collections_on_both_sides() {
        let (host, mut plugin, doc) = setup(PATH, ORDER, "java");
        let action = offered(&mut plugin, &doc, TITLE_TO);
        answers(&host, &[&["Tag"], &["ManyToMany"], &["tags"], &["orders"], &["holder"], &["keep"], &[]]);
        let result = plugin.resolve_file_action(&doc, action).unwrap();
        assert!(patched(ORDER, &result.edits[0].1).unwrap().contains("List<Tag> tags"));
        assert!(created(&result).1.contains("@ManyToMany(mappedBy = \"tags\")"));
    }

    #[test]
    fn javax_and_kotlin_follow_the_invoking_entity() {
        let javax = ORDER.replace("jakarta", "javax");
        let (host, mut plugin, doc) = setup(PATH, &javax, "java");
        let action = offered(&mut plugin, &doc, TITLE_TO);
        answers(&host, &[&["Customer"], &["ManyToOne"], &["customer"], &["orders"], &[]]);
        let result = plugin.resolve_file_action(&doc, action).unwrap();
        let text = created(&result).1;
        assert!(text.contains("import javax.persistence.OneToMany;") && !text.contains("jakarta"), "{text}");

        let kotlin = "package shop\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\n@Entity\nclass Order {\n    @Id var id: Long? = null\n}\n";
        let (host, mut plugin, doc) = setup("/project/src/main/kotlin/shop/Order.kt", kotlin, "kotlin");
        let action = offered(&mut plugin, &doc, TITLE_FROM);
        answers(&host, &[&["Shipment"], &["ManyToOne"], &["order"], &["shipments"], &[]]);
        let result = plugin.resolve_file_action(&doc, action).unwrap();
        let (path, text) = created(&result);
        assert_eq!(path, "/project/src/main/kotlin/shop/Shipment.kt");
        assert!(text.contains("class Shipment") && text.contains("var order: Order? = null") && !text.contains(';'), "{text}");
        assert!(patched(kotlin, &result.edits[0].1).unwrap().contains("MutableList<Shipment>"));
    }

    #[test]
    fn refusals_and_cancellation_change_nothing() {
        let (host, mut plugin, doc) = setup(PATH, ORDER, "java");
        host.add_file("src/main/java/shop/Customer.java", "class Customer {}");
        for rows in [vec![vec!["Customer"]], vec![vec!["Order"]], vec![vec!["Long"]], vec![vec!["Tag"], vec!["ManyToOne"], vec!["id"]]] {
            let action = offered(&mut plugin, &doc, TITLE_TO);
            for row in &rows {
                host.answer_next_prompt(Some(row));
            }
            assert!(plugin.resolve_file_action(&doc, action).is_err(), "{rows:?}");
        }

        for cancel_at in 0..5 {
            let action = offered(&mut plugin, &doc, TITLE_TO);
            let script: [&[&str]; 5] = [&["Tag"], &["ManyToOne"], &["tag"], &["orders"], &[]];
            for (index, row) in script.iter().enumerate() {
                if index == cancel_at {
                    host.answer_next_prompt(None);
                    break;
                }
                host.answer_next_prompt(Some(row));
            }
            assert!(matches!(plugin.resolve_file_action(&doc, action), Err(Error::Cancelled)), "cancel at {cancel_at}");
        }
        assert_eq!(host.editor_text(), ORDER);
    }

    #[test]
    fn a_buffer_edit_after_the_prompts_makes_the_action_stale() {
        let (host, mut plugin, doc) = setup(PATH, ORDER, "java");
        let action = offered(&mut plugin, &doc, TITLE_TO);
        host.type_in_editor("// changed");
        answers(&host, &[&["Customer"], &["ManyToOne"], &["customer"], &["orders"], &[]]);
        assert!(plugin.resolve_file_action(&doc, action).is_err());
    }

    #[test]
    fn only_entities_are_offered_the_action() {
        let embeddable = "package shop;\nimport jakarta.persistence.*;\n@Embeddable\nclass Address {}\n";
        let (_host, mut plugin, doc) = setup("/project/src/main/java/shop/Address.java", embeddable, "java");
        assert!(plugin.file_actions(&doc, Range::default()).iter().all(|a| !a.title.starts_with("Create related entity")));
    }

    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_API / JPA_JAVAX_API jar paths (source scripts/jpa-compile-env.sh)"]
    fn generated_pairs_compile_against_both_persistence_apis() {
        for (ns, variable) in [("jakarta.persistence", "JPA_JAKARTA_API"), ("javax.persistence", "JPA_JAVAX_API")] {
            let jar = std::env::var(variable).expect("explicit API jar path");
            let source = ORDER.replace("jakarta.persistence", ns);
            let mut count = 0;
            for direction in [Direction::To, Direction::From] {
                for (relation, owner) in [("ManyToOne", ""), ("OneToMany", ""), ("OneToOne", "holder"), ("OneToOne", "other"), ("ManyToMany", "holder"), ("ManyToMany", "other")] {
                    count += 1;
                    let (host, mut plugin, doc) = setup(PATH, &source, "java");
                    let title = if direction == Direction::To { TITLE_TO } else { TITLE_FROM };
                    let action = offered(&mut plugin, &doc, title);
                    let mut rows: Vec<Vec<&str>> = vec![vec!["Peer"], vec![relation], vec!["peer"], vec!["order"]];
                    if !owner.is_empty() {
                        rows.push(vec![owner]);
                    }
                    rows.push(vec![]);
                    for row in &rows {
                        host.answer_next_prompt(Some(row));
                    }
                    let result = plugin.resolve_file_action(&doc, action).unwrap();
                    let dir = std::env::temp_dir().join(format!("jpa-related-{}-{ns}-{count}", std::process::id()));
                    std::fs::create_dir_all(&dir).unwrap();
                    let files = [("Order.java", patched(&source, &result.edits[0].1).unwrap()), ("Peer.java", created(&result).1.to_owned())];
                    let paths: Vec<_> = files
                        .iter()
                        .map(|(file, body)| {
                            let path = dir.join(file);
                            std::fs::write(&path, body).unwrap();
                            path
                        })
                        .collect();
                    let output = std::process::Command::new("javac")
                        .args(["--release", "17", "-proc:none", "-classpath", &jar, "-d"])
                        .arg(&dir)
                        .args(&paths)
                        .output()
                        .expect("javac installed");
                    assert!(output.status.success(), "{ns} {relation} {owner}: {}\n{files:?}", String::from_utf8_lossy(&output.stderr));
                    std::fs::remove_dir_all(dir).unwrap();
                }
            }
            assert_eq!(count, 12);
        }
    }
}
