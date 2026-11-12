//! Entity views in the standard repository creation flow: when the Blaze-Persistence Spring Data
//! integration is declared in the build and the project has entity views of the selected entity,
//! "Create Spring Data repository with options" first asks whether the repository is for the entity or
//! for one of its views. Choosing a view continues with the entity view repository flow.
use super::*;
use super::revision_repo_action as view_repo;

struct EntityViewSource {
    doc: Document,
}

fn view_of(entity: &Class, text: &str, file: &str) -> Option<Document> {
    let masked = mask(text);
    let at = masked.find("@EntityView")?;
    let value = masked[at..].split_once('(')?.1.split_once(')')?.0.trim();
    if value.strip_suffix(".class")? != entity.name {
        return None;
    }
    let document = Document { path: file.to_owned(), text: text.to_owned(), language: "java".into(), version: 0 };
    view_repo::view_ok(&document).then_some(())?;
    let package_name = package(&document);
    let qualified = entity.package.as_deref().map(|package| format!("{package}.{}", entity.name));
    let same_package = package_name == entity.package;
    let imported = qualified.is_some_and(|qualified| mask(text).lines().any(|line| line.trim().strip_prefix("import ").is_some_and(|rest| rest.trim().trim_end_matches(';').trim() == qualified)));
    (same_package || imported).then_some(document)
}

fn views_of(project: &Project, root: &str, entity: &Class, own: &str) -> Result<Vec<EntityViewSource>, Error> {
    let mut found = Vec::new();
    view_repo::scan_sources(project, root, |file, text| {
        if file != own && text.contains("@EntityView") {
            if let Some(doc) = view_of(entity, text, file) {
                found.push(EntityViewSource { doc });
            }
        }
        Ok(())
    })?;
    found.sort_by(|a, b| a.doc.path.cmp(&b.doc.path));
    Ok(found)
}

/// `Some(action)` when the user chose a view; `None` continues with the entity repository flow.
pub(super) fn offer(pending: &Pending, doc: &Document, project: &Project) -> Result<Option<FileAction>, Error> {
    if doc.language != "java" || view_repo::blaze_proof(project, &pending.root, doc).is_err() {
        return Ok(None);
    }
    let Some(class) = current_class(doc) else {
        return Ok(None);
    };
    let views = views_of(project, &pending.root, &class, &doc.path)?;
    if views.is_empty() {
        return Ok(None);
    }
    let mut items = vec![PickItem::new("entity", &format!("The entity {}", class.name))];
    items.extend(views.iter().map(|view| PickItem::new(&view.doc.path, &format!("Entity view {}", file_stem(&view.doc.path))).description(&view.doc.path)));
    let key = host::pick("Repository for", &items)?.ok_or(Error::Cancelled)?;
    if key == "entity" {
        return Ok(None);
    }
    let view = views.into_iter().find(|view| view.doc.path == key).ok_or("Choose one of the offered options.")?;
    let view_pending = Pending { root: pending.root.clone(), doc: view.doc.clone(), operation: pending.operation, selection: Range::default() };
    let action = view_repo::resolve_view(&view_pending, &view.doc, project)?;
    current(pending, doc)?;
    Ok(Some(action))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ENTITY_PATH: &str = "/project/src/main/java/shop/Order.java";
    const ENTITY: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Order {\n    @Id Long id;\n    String title;\n}\n";
    const VIEW: &str = "package shop.web;\n\nimport com.blazebit.persistence.view.EntityView;\nimport com.blazebit.persistence.view.IdMapping;\nimport shop.Order;\n\n@EntityView(Order.class)\npublic interface OrderView {\n\n    @IdMapping\n    Long getId();\n}\n";
    const POM: &str = "<project><dependencies><dependency><groupId>com.blazebit</groupId><artifactId>blaze-persistence-integration-spring-data-3.4</artifactId></dependency></dependencies></project>";

    fn run(files: &[(&str, &str)], answers: &[&[&str]]) -> Result<FileAction, Error> {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(ENTITY_PATH, "java", ENTITY, &[]);
        host.add_file(ENTITY_PATH, ENTITY);
        for (path, text) in files {
            host.add_file(path, text);
        }
        let doc = Document { path: ENTITY_PATH.into(), text: ENTITY.into(), language: "java".into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == repository_action::TITLE).unwrap();
        for answer in answers {
            host.answer_next_prompt(Some(answer));
        }
        plugin.resolve_file_action(&doc, pending)
    }

    #[test]
    fn views_are_offered_next_to_the_entity_only_with_blaze_and_a_view_of_this_entity() {
        let view = "/project/src/main/java/shop/web/OrderView.java";
        let created = run(&[("pom.xml", POM), (view, VIEW)], &[&[view], &["OrderViewRepository"], &[]]).unwrap();
        let [FileOperation::Create { path, text, overwrite: false }] = created.operations.as_slice() else { panic!("one create") };
        assert_eq!(path, "/project/src/main/java/shop/web/OrderViewRepository.java");
        assert!(text.contains("extends EntityViewRepository<OrderView, Long>"), "{text}");
        let entity = run(&[("pom.xml", POM), (view, VIEW)], &[&["entity"], &["JpaRepository"], &["no"], &["OrderRepository"], &[]]).unwrap();
        assert!(matches!(entity.operations.as_slice(), [FileOperation::Create { path, .. }] if path.ends_with("OrderRepository.java")));
        let without_blaze = run(&[(view, VIEW)], &[&["JpaRepository"], &["no"], &["OrderRepository"], &[]]).unwrap();
        assert!(matches!(without_blaze.operations.as_slice(), [FileOperation::Create { path, .. }] if path.ends_with("OrderRepository.java")));
        let foreign = VIEW.replace("Order.class", "Customer.class");
        let ignored = run(&[("pom.xml", POM), (view, &foreign)], &[&["JpaRepository"], &["no"], &["OrderRepository"], &[]]).unwrap();
        assert!(matches!(ignored.operations.as_slice(), [FileOperation::Create { path, .. }] if path.ends_with("OrderRepository.java")));
    }

    #[test]
    fn a_forged_or_cancelled_choice_changes_nothing() {
        let view = "/project/src/main/java/shop/web/OrderView.java";
        assert!(run(&[("pom.xml", POM), (view, VIEW)], &[&["/project/other.java"]]).is_err());
        assert!(matches!(run(&[("pom.xml", POM), (view, VIEW)], &[]), Err(Error::Cancelled)));
    }
}
