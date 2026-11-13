//! Caret on an entity attribute: add a Spring Data query method for that attribute to the entity's repository.
//!
//! Offered only in a plain entity, on a plain scalar attribute the query method flow supports, and only when the repository
//! index already names a repository for the entity. Resolving runs the reviewed query method flow on the repository with the
//! attribute as its one condition; the repository is the only file edited, the entity is read-only.
use super::*;
use super::unresolved_create::absolute;
use crate::queries;

pub(super) const TITLE: &str = "Create Spring Data repository method…";
const MAX_REPOSITORY_FILES: u32 = 500;

/// The query-method attribute whose declaration, annotations included, holds the caret.
pub(super) fn attribute_under(doc: &Document, selection: Range) -> Option<(Class, String)> {
    if !doc.text.contains("@Entity") {
        return None;
    }
    let caret = offset(&doc.text, selection.start)?;
    let class = parse_class(&doc.text, &doc.path).filter(|class| class.kind == Kind::Entity)?;
    let (attribute, name_at) = class.attributes.iter().filter_map(|attribute| Some((attribute, offset(&doc.text, attribute.pos)?))).filter(|(_, at)| *at >= caret).min_by_key(|(_, at)| *at)?;
    let masked = mask(&doc.text);
    let first = crate::converter_markers::member_start(&masked, name_at);
    let start = first + masked[first..name_at].len() - masked[first..name_at].trim_start().len();
    let end = name_at + doc.text[name_at..].find('\n').unwrap_or(doc.text.len() - name_at);
    if caret < start || caret > end {
        return None;
    }
    let name = attribute.name.clone();
    query_method_action::attributes(&class, &doc.text).iter().any(|supported| supported.name == name).then_some((class, name))
}

/// A repository of `entity` named `<Entity>Repository` beside the entity or in a `repository`/`repositories` package next to it: a few direct reads,
/// no project search, so code-action requests stay fast. Other layouts become eligible once the repository index is warm.
thread_local! {
    static NEARBY: std::cell::RefCell<std::collections::HashMap<String, bool>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Called when source files change: nearby-repository answers are remembered only until then.
pub(crate) fn forget_nearby() {
    NEARBY.with(|nearby| nearby.borrow_mut().clear());
}

pub(super) fn repository_nearby(project: &Project, entity_path: &str, entity: &str) -> bool {
    let key = format!("{}\u{0}{entity_path}\u{0}{entity}", project.root().unwrap_or_default());
    if let Some(known) = NEARBY.with(|nearby| nearby.borrow().get(&key).copied()) {
        return known;
    }
    let found = repository_nearby_uncached(project, entity_path, entity);
    NEARBY.with(|nearby| nearby.borrow_mut().insert(key, found));
    found
}

fn repository_nearby_uncached(project: &Project, entity_path: &str, entity: &str) -> bool {
    let Some((dir, _)) = entity_path.rsplit_once('/') else {
        return false;
    };
    let parent = dir.rsplit_once('/').map_or(dir, |(parent, _)| parent);
    let mut directories = vec![dir.to_owned()];
    for name in ["repository", "repositories"] {
        directories.push(format!("{dir}/{name}"));
        directories.push(format!("{parent}/{name}"));
    }
    directories.iter().any(|directory| {
        ["java", "kt"].iter().any(|extension| {
            project.read_file(&format!("{directory}/{entity}Repository.{extension}")).is_ok_and(|text| text.len() <= MAX_TEXT && queries::parse_repository(&text, *extension == "kt").is_some_and(|repository| repository.entity == entity))
        })
    })
}

fn repository_document(project: &Project, root: &str, entity: &str, file: String) -> Option<Document> {
    let file = absolute(root, file);
    if !inside(root, &file) || path(&file).as_deref() != Some(file.as_str()) {
        return None;
    }
    let text = project.read_file(&file).ok().filter(|text| text.len() <= MAX_TEXT)?;
    let kotlin = language_of(&file)?;
    queries::parse_repository(&text, kotlin).filter(|repo| repo.entity == entity)?;
    let document = Document { path: file, language: if kotlin { "kotlin" } else { "java" }.into(), version: 0, text };
    query_method_action::eligible(&document).then_some(document)
}

fn repositories(project: &Project, root: &str, entity: &str) -> Result<Vec<Document>, Error> {
    let mut found = Vec::new();
    for extension in ["java", "kt"] {
        for file in project.find_files(&format!("**/*Repository.{extension}"), MAX_REPOSITORY_FILES)? {
            found.extend(repository_document(project, root, entity, file));
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let (class, attribute) = attribute_under(doc, pending.selection).ok_or(Error::Stale)?;
    let mut candidates = repositories(project, &pending.root, &class.name)?;
    if candidates.is_empty() {
        return Err(format!("No Spring Data repository for {} was found in this project.", class.name).into());
    }

    let repository = if candidates.len() == 1 {
        candidates.remove(0)
    } else {
        let items: Vec<_> = candidates.iter().map(|candidate| PickItem::new(&candidate.path, file_stem(&candidate.path))).collect();
        let Some(chosen) = host::pick(&format!("Repository of {}", class.name), &items)? else {
            return Err(Error::Cancelled);
        };
        candidates.into_iter().find(|candidate| candidate.path == chosen).ok_or(Error::Stale)?
    };

    let repository_pending = Pending { root: pending.root.clone(), doc: repository.clone(), operation: Operation::QueryMethod, selection: Range::default() };
    query_method_action::resolve_for_attribute(&repository_pending, &repository, project, &attribute)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ENTITY_PATH: &str = "/project/src/main/java/shop/Order.java";
    const ENTITY: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Order {\n    @Id Long id;\n    @Column(name = \"num\")\n    String number;\n    @ManyToOne Customer customer;\n}\n";
    const REPOSITORY: &str = "package shop;\n\nimport org.springframework.data.jpa.repository.JpaRepository;\n\npublic interface OrderRepository extends JpaRepository<Order, Long> {\n}\n";

    fn setup(repository: bool) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/Order.java", ENTITY);
        if repository {
            host.add_file("src/main/java/shop/OrderRepository.java", REPOSITORY);
        }
        host.open_editor(ENTITY_PATH, "java", ENTITY, &[]);
        let doc = Document { path: ENTITY_PATH.into(), language: "java".into(), version: 1, text: ENTITY.into() };
        let mut plugin = JpaAssist::default();
        let project = host::project_of(ENTITY_PATH).unwrap();
        plugin.repositories_of(&project, "Order");
        (host, plugin, doc)
    }

    fn caret(text: &str, anchor: &str) -> Range {
        let at = text.find(anchor).unwrap();
        Range::new(position(text, at), position(text, at))
    }

    fn offered(plugin: &mut JpaAssist, doc: &Document, anchor: &str) -> Option<FileAction> {
        plugin.file_actions(doc, caret(&doc.text, anchor)).into_iter().find(|action| action.title == TITLE)
    }

    #[test]
    fn offered_on_the_attribute_declaration_and_its_annotation_but_not_on_associations() {
        let (_host, mut plugin, doc) = setup(true);
        assert!(offered(&mut plugin, &doc, "String number").is_some());
        assert!(offered(&mut plugin, &doc, "@Column").is_some(), "annotation line of the attribute");
        assert!(offered(&mut plugin, &doc, "@ManyToOne").is_none(), "relationship paths are out of scope");
        assert!(offered(&mut plugin, &doc, "public class").is_none());
    }

    #[test]
    fn not_offered_without_a_repository_for_the_entity() {
        let (_host, mut plugin, doc) = setup(false);
        assert!(offered(&mut plugin, &doc, "String number").is_none());
    }

    #[test]
    fn the_query_flow_runs_on_the_repository_with_the_attribute_preselected() {
        let (host, mut plugin, doc) = setup(true);
        let action = offered(&mut plugin, &doc, "String number").unwrap();
        for answer in [&["find"][..], &["containing"], &["none"], &[""], &["list"], &[]] {
            host.answer_next_prompt(Some(answer));
        }
        let result = plugin.resolve_file_action(&doc, action).unwrap();
        let [(path, edits)] = result.edits.as_slice() else { panic!("only the repository is edited") };
        assert_eq!(path, "/project/src/main/java/shop/OrderRepository.java");
        let after = patched(REPOSITORY, edits).unwrap();
        assert!(after.contains("List<Order> findByNumberContaining(String number);"), "{after}");
        assert_eq!(host.editor_text(), ENTITY);
    }

    #[test]
    fn a_stale_entity_is_refused() {
        let (_host, mut plugin, doc) = setup(true);
        let action = offered(&mut plugin, &doc, "String number").unwrap();
        let changed = Document { text: format!("{}\n", doc.text), version: 2, ..doc };
        assert!(plugin.resolve_file_action(&changed, action).is_err());
    }
}
