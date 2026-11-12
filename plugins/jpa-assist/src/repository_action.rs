//! Reviewed creation of a Spring Data repository interface for a plain entity. One new sibling
//! file (`overwrite: false`); no existing file is edited.
//!
//! The identifier type is taken only from a single `@Id` field of this source. Parent interfaces
//! follow the Spring Data generation implied by the entity's persistence namespace: Spring Data JPA
//! 3.0 moved to Jakarta Persistence, and `ListCrudRepository`/`ListPagingAndSortingRepository`
//! exist since Spring Data Commons 3.0 (`@since 3.0`), so they are offered for jakarta entities
//! only. In 3.x `PagingAndSortingRepository` no longer extends `CrudRepository`, so only the
//! 2.x-era javax entities are offered `PagingAndSortingRepository`.
use super::*;

pub(super) const TITLE: &str = "Create Spring Data repository with options…";
const JPA_PACKAGE: &str = "org.springframework.data.jpa.repository";
const COMMONS_PACKAGE: &str = "org.springframework.data.repository";
const SPECIFICATION_EXECUTOR: &str = "JpaSpecificationExecutor";

pub(super) struct Facts {
    pub entity: String,
    pub id_type: String,
    id_import: Option<&'static str>,
    pub jakarta: bool,
    pub kotlin: bool,
}

/// (simple name, package) of the parents offered for the entity's Spring Data generation.
pub(super) fn parents(jakarta: bool) -> &'static [(&'static str, &'static str)] {
    if jakarta {
        &[
            ("JpaRepository", JPA_PACKAGE),
            ("ListCrudRepository", COMMONS_PACKAGE),
            ("CrudRepository", COMMONS_PACKAGE),
            ("ListPagingAndSortingRepository", COMMONS_PACKAGE),
        ]
    } else {
        &[
            ("JpaRepository", JPA_PACKAGE),
            ("CrudRepository", COMMONS_PACKAGE),
            ("PagingAndSortingRepository", COMMONS_PACKAGE),
        ]
    }
}

fn identifier_type(class: &Class, ty: &str) -> Option<(String, Option<&'static str>)> {
    if !super::dto_action::supported(class, ty) {
        return None;
    }
    let mapped = match (class.kotlin, ty) {
        (false, "long") => "Long",
        (false, "int") => "Integer",
        (false, "short") => "Short",
        (false, "byte") => "Byte",
        (false, "char") => "Character",
        (false, "Long" | "Integer" | "Short" | "Byte" | "Character" | "String") => ty,
        (true, "Long" | "Int" | "Short" | "Byte" | "Char" | "String") => ty,
        (_, "UUID") => return Some(("UUID".into(), Some("java.util.UUID"))),
        _ => return None,
    };
    Some((mapped.into(), None))
}

pub(super) fn source(doc: &Document) -> Result<Facts, String> {
    let class = current_class(doc).filter(|c| c.kind == Kind::Entity).ok_or(
        "Select a plain entity with no superclass; inherited identifiers are not resolved.",
    )?;
    let masked = mask(&doc.text);
    let ns = namespace(&doc.text, &class).ok_or("The entity's persistence namespace is not unambiguous.")?;
    if annotations(&masked).iter().any(|a| matches!(a.name, "EmbeddedId" | "IdClass")) {
        return Err("Composite identifiers (@EmbeddedId/@IdClass) are not resolved; no repository ID type is guessed.".into());
    }
    let ids: Vec<_> = class.attributes.iter().filter(|a| a.id).collect();
    let [id] = ids.as_slice() else {
        return Err(if ids.is_empty() {
            "No @Id field found in this source (inherited or XML-mapped identifiers are not resolved)."
        } else {
            "Several @Id fields (composite identifier); no repository ID type is guessed."
        }
        .into());
    };
    if !annotation_is(&doc.text, &class, "Id", ns) {
        return Err("The @Id annotation is not provably from the entity's persistence namespace.".into());
    }
    let (id_type, id_import) = identifier_type(&class, &id.ty).ok_or_else(|| {
        format!("Identifier type `{}` is not a supported simple type (wrapper, String or UUID); no ID type is guessed.", id.ty)
    })?;
    Ok(Facts { entity: class.name, id_type, id_import, jakarta: ns == "jakarta.persistence", kotlin: class.kotlin })
}

pub(super) fn eligible(doc: &Document) -> bool {
    current_class(doc).is_some_and(|class| class.kind == Kind::Entity)
}

pub(super) fn render(doc: &Document, facts: &Facts, parent: &str, specification: bool, name: &str) -> Result<(String, String), Error> {
    let Some(&(_, parent_package)) = parents(facts.jakarta).iter().find(|(simple, _)| *simple == parent) else {
        return Err("Choose one of the offered parent interfaces.".into());
    };
    render_with(doc, facts, parent, parent_package, specification, name)
}

pub(super) fn render_with(doc: &Document, facts: &Facts, parent: &str, parent_package: &str, specification: bool, name: &str) -> Result<(String, String), Error> {
    let package_name = package(doc).ok_or(Error::Stale)?;
    let mut imports = Vec::new();
    if parent_package != package_name {
        imports.push(format!("{parent_package}.{parent}"));
    }
    if specification {
        imports.push(format!("{JPA_PACKAGE}.{SPECIFICATION_EXECUTOR}"));
    }
    imports.extend(facts.id_import.map(str::to_owned));
    imports.sort();
    imports.dedup();
    let simple_names: Vec<_> = imports.iter().filter_map(|i| i.rsplit('.').next()).collect();
    if !identifier(name) || name == parent || name == facts.entity || simple_names.contains(&name) || simple_names.contains(&facts.entity.as_str()) {
        return Err("Choose a valid repository name that does not clash with the entity or imported types.".into());
    }
    let mut supers = vec![format!("{parent}<{}, {}>", facts.entity, facts.id_type)];
    if specification {
        supers.push(format!("{SPECIFICATION_EXECUTOR}<{}>", facts.entity));
    }
    let semicolon = if facts.kotlin { "" } else { ";" };
    let imports: String = imports.iter().map(|i| format!("import {i}{semicolon}\n")).collect();
    let imports = if imports.is_empty() { imports } else { format!("{imports}\n") };
    let mut text = if facts.kotlin {
        format!("package {package_name}\n\n{imports}interface {name} : {}\n", supers.join(", "))
    } else {
        format!("package {package_name};\n\n{imports}public interface {name} extends {} {{\n}}\n", supers.join(", "))
    };
    if doc.text.contains("\r\n") {
        text = text.replace('\n', "\r\n");
    }
    if text.len() > MAX_TEXT {
        return Err("Generated source exceeds the supported bound.".into());
    }
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    Ok((format!("{directory}/{name}.{}", if facts.kotlin { "kt" } else { "java" }), text))
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    if revision_repo_action::eligible_view(doc) {
        return revision_repo_action::resolve_view(pending, doc, project);
    }
    let facts = source(doc).map_err(Error::from)?;
    let project_parents = project_parents::discover(project, &pending.root, &doc.path).unwrap_or_default();
    let mut items: Vec<_> = parents(facts.jakarta).iter().map(|(simple, _)| PickItem::new(simple, simple)).collect();
    items.extend(project_parents.iter().map(|found| PickItem::new(&format!("project:{}", found.name), &found.name).description(&format!("project repository in {}", found.package))));
    let parent = host::pick("Repository parent interface", &items)?.ok_or(Error::Cancelled)?;
    let choices = [PickItem::new("no", "No"), PickItem::new("yes", "Yes, also extend JpaSpecificationExecutor")];
    let specification = host::pick("Add JpaSpecificationExecutor?", &choices)?.ok_or(Error::Cancelled)? == "yes";
    let default = format!("{}Repository", facts.entity);
    let name = host::input("Repository name (same package)", Some(&default))?.ok_or(Error::Cancelled)?;
    let name = name.trim().to_owned();
    current(pending, doc)?;
    let (target, text) = match parent.strip_prefix("project:") {
        Some(chosen) => {
            let found = project_parents.iter().find(|found| found.name == chosen).ok_or("Choose one of the offered parent interfaces.")?;
            render_with(doc, &facts, &found.name, &found.package, specification, &name)?
        }
        None => render(doc, &facts, &parent, specification, &name)?,
    };
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
        return Err(Error::Stale);
    }
    no_collision(project, &pending.root, directory, &name, None)?;
    if !ui_kit::prompts::confirm_exact(
        "Create Spring Data repository?",
        &format!(
            "Source: {}\nSource version: {}\nNew file: {target}\n\n{text}\nThe ID type comes from the @Id field of this source only. The Spring Data version is not read from the build; parents follow the entity's persistence namespace. No existing file is overwritten.",
            doc.path, doc.version
        ),
    )? {
        return Err(Error::Cancelled);
    }
    let project = current(pending, doc)?;
    no_collision(&project, &pending.root, directory, &name, None)?;
    Ok(FileAction {
        title: format!("Create Spring Data repository {name}"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target, text, overwrite: false }],
        ..FileAction::default()
    })
}

pub(super) const BATCH_TITLE: &str = "Create repositories for entities…";
const MAX_BATCH: usize = 10;
const ENTITY_SCAN_LIMIT: u32 = 1001;
const PREVIEW_LIMIT: usize = 16 << 10;

struct Candidate {
    doc: Document,
    facts: Facts,
}

/// Plain entities whose `@Id` type is proven by [`source`], plus `file: reason` for every other `@Entity` source.
fn scan_entities(project: &Project, root: &str) -> Result<(Vec<Candidate>, Vec<String>), Error> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut found = Vec::new();
    let mut refused = Vec::new();
    for extension in ["java", "kt"] {
        let files = project.find_files(&format!("**/*.{extension}"), ENTITY_SCAN_LIMIT)?;
        if files.len() >= ENTITY_SCAN_LIMIT as usize {
            return Err("The project has too many sources to scan for entities; use the single-entity repository action.".into());
        }
        for file in files {
            if Instant::now() >= deadline {
                return Err("Entity discovery timed out.".into());
            }
            let file = if file.starts_with('/') { file } else { format!("{}/{file}", root.trim_end_matches('/')) };
            if path(&file).as_deref() != Some(file.as_str()) || !inside(root, &file) {
                continue;
            }
            let text = project.read_file(&file)?;
            if text.len() > MAX_TEXT || !annotations(&mask(&text)).iter().any(|a| a.name == "Entity") {
                continue;
            }
            let doc = Document { path: file.clone(), language: if extension == "kt" { "kotlin" } else { "java" }.into(), text, version: 0 };
            match source(&doc) {
                Ok(facts) => found.push(Candidate { doc, facts }),
                Err(reason) => refused.push(format!("{}: {reason}", file_stem(&file))),
            }
        }
    }
    Ok((found, refused))
}

impl JpaAssist {
    /// Entity simple names that already have a repository; the whole repository index must have been read.
    fn repository_entities(&mut self, project: &Project) -> Result<HashSet<String>, Error> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let (by_entity, complete) = self.repository_facts(project);
            if complete {
                return Ok(by_entity.into_iter().filter(|(_, found)| !found.is_empty()).map(|(entity, _)| entity).collect());
            }
            if Instant::now() >= deadline {
                return Err("Repository discovery is incomplete, so missing repositories cannot be proven.".into());
            }
        }
    }

    pub(super) fn resolve_repositories(&mut self, pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
        current(pending, doc)?;
        let (candidates, mut refused) = scan_entities(project, &pending.root)?;
        let existing = self.repository_entities(project)?;
        let mut offered = Vec::new();
        for candidate in candidates {
            let name = &candidate.facts.entity;
            if existing.contains(name) {
                continue;
            }
            if offered.iter().any(|o: &Candidate| &o.facts.entity == name) {
                refused.push(format!("{name}: several entities share this simple name; repositories are matched by simple name."));
                continue;
            }
            offered.push(candidate);
        }
        let summary = |refused: &[String]| {
            let mut lines: Vec<_> = refused.iter().take(10).map(|r| format!("  {r}")).collect();
            if refused.len() > 10 {
                lines.push(format!("  and {} more", refused.len() - 10));
            }
            if lines.is_empty() { "None.".to_owned() } else { format!("\n{}", lines.join("\n")) }
        };
        if offered.is_empty() {
            return Err(format!("No entity without a repository can be handled. Refused: {}", summary(&refused)).into());
        }
        let items: Vec<_> = offered
            .iter()
            .enumerate()
            .map(|(index, c)| PickItem::new(&index.to_string(), &c.facts.entity).description(&format!("{} · ID {}", package(&c.doc).unwrap_or_default(), c.facts.id_type)))
            .collect();
        let title = format!("Entities without a repository (up to {MAX_BATCH}; {} refused, listed in the review)", refused.len());
        let picked = host::pick_many(&title, &items)?.ok_or(Error::Cancelled)?;
        let mut chosen = Vec::new();
        for key in &picked {
            let index = key.parse::<usize>().map_err(|_| Error::Stale)?;
            let candidate = offered.get(index).ok_or(Error::Stale)?;
            if chosen.iter().any(|c: &&Candidate| c.doc.path == candidate.doc.path) {
                return Err("Choose each entity once.".into());
            }
            chosen.push(candidate);
        }
        if chosen.is_empty() || chosen.len() > MAX_BATCH {
            return Err(format!("Choose between one and {MAX_BATCH} entities; pick fewer.").into());
        }
        let jakarta = chosen[0].facts.jakarta;
        if chosen.iter().any(|c| c.facts.jakarta != jakarta) {
            return Err("The chosen entities mix jakarta and javax persistence; pick entities of one generation.".into());
        }
        let items: Vec<_> = parents(jakarta).iter().map(|(simple, _)| PickItem::new(simple, simple)).collect();
        let parent = host::pick("Repository parent interface (applied to all)", &items)?.ok_or(Error::Cancelled)?;
        let choices = [PickItem::new("no", "No"), PickItem::new("yes", "Yes, also extend JpaSpecificationExecutor")];
        let specification = host::pick("Add JpaSpecificationExecutor to all?", &choices)?.ok_or(Error::Cancelled)? == "yes";

        let mut files = Vec::new();
        let mut preview = String::new();
        for candidate in &chosen {
            let name = format!("{}Repository", candidate.facts.entity);
            let (target, text) = render(&candidate.doc, &candidate.facts, &parent, specification, &name)?;
            let directory = candidate.doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
            if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) || files.iter().any(|(t, _, _): &(String, String, String)| *t == target) {
                return Err(Error::Stale);
            }
            no_collision(project, &pending.root, directory, &name, None)?;
            preview.push_str(&format!("New file: {target}\n{text}\n"));
            files.push((target, text, name));
        }
        let review = format!(
            "{} repositories, parent {parent}{}.\n\n{preview}Refused entities: {}\n\nEach ID type comes from the @Id field of that entity's own source only. Existing repositories are matched by entity simple name from files named *Repository.java/kt; the Spring Data version is not read from the build. No existing file is overwritten.",
            files.len(),
            if specification { " + JpaSpecificationExecutor" } else { "" },
            summary(&refused)
        );
        if review.len() > PREVIEW_LIMIT {
            return Err("The review would exceed 16 KiB; pick fewer entities.".into());
        }
        if !ui_kit::prompts::confirm_exact("Create Spring Data repositories?", &review)? {
            return Err(Error::Cancelled);
        }
        let project = current(pending, doc)?;
        for (candidate, (target, _, name)) in chosen.iter().zip(&files) {
            if project.read_file(&candidate.doc.path)? != candidate.doc.text {
                return Err(Error::Stale);
            }
            let directory = target.rsplit_once('/').ok_or(Error::Stale)?.0;
            no_collision(&project, &pending.root, directory, name, None)?;
        }
        Ok(FileAction {
            title: format!("Create {} Spring Data repositories", files.len()),
            kind: Some("refactor.rewrite".into()),
            operations: files.into_iter().map(|(path, text, _)| FileOperation::Create { path, text, overwrite: false }).collect(),
            ..FileAction::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    fn java(ns: &str, id: &str) -> String {
        format!("package shop;\nimport {ns}.*;\n@Entity\npublic class Order {{\n    @Id {id} id;\n}}\n")
    }

    fn run(path: &str, text: &str, language: &str, answers: &[&str]) -> (MockHost, Result<FileAction, Error>) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(path, language, text, &[]);
        let doc = Document { path: path.into(), language: language.into(), text: text.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        assert!(pending.operations.is_empty() && pending.edits.is_empty());
        for answer in answers {
            host.answer_next_prompt(Some(&[*answer]));
        }
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&doc, pending);
        (host, result)
    }

    fn created(result: Result<FileAction, Error>) -> (String, String) {
        let action = result.unwrap();
        let [FileOperation::Create { path, text, overwrite }] = action.operations.as_slice() else { panic!("one create") };
        assert!(!overwrite && action.edits.is_empty());
        (path.clone(), text.clone())
    }

    const JAVA_PATH: &str = "/project/src/main/java/shop/Order.java";
    const KOTLIN_PATH: &str = "/project/src/main/kotlin/shop/Order.kt";

    #[test]
    fn jakarta_java_jpa_repository_with_specification_executor() {
        let (_, result) = run(JAVA_PATH, &java("jakarta.persistence", "Long"), "java", &["JpaRepository", "yes", "OrderRepository"]);
        let (path, text) = created(result);
        assert_eq!(path, "/project/src/main/java/shop/OrderRepository.java");
        assert_eq!(
            text,
            "package shop;\n\nimport org.springframework.data.jpa.repository.JpaRepository;\nimport org.springframework.data.jpa.repository.JpaSpecificationExecutor;\n\npublic interface OrderRepository extends JpaRepository<Order, Long>, JpaSpecificationExecutor<Order> {\n}\n"
        );
    }

    #[test]
    fn javax_java_wrapper_ids_map_primitives_and_offer_paging_and_sorting_only_for_javax() {
        let (_, result) = run(JAVA_PATH, &java("javax.persistence", "int"), "java", &["PagingAndSortingRepository", "no", "Orders"]);
        let (_, text) = created(result);
        assert!(text.contains("import org.springframework.data.repository.PagingAndSortingRepository;"), "{text}");
        assert!(text.contains("interface Orders extends PagingAndSortingRepository<Order, Integer> {"), "{text}");
        let (_, result) = run(JAVA_PATH, &java("javax.persistence", "Long"), "java", &["ListCrudRepository", "no", "OrderRepository"]);
        assert!(result.is_err(), "List* parents are not offered for javax (Spring Data 2.x)");
        let (_, result) = run(JAVA_PATH, &java("jakarta.persistence", "Long"), "java", &["PagingAndSortingRepository", "no", "OrderRepository"]);
        assert!(result.is_err(), "plain PagingAndSortingRepository is not offered for jakarta");
    }

    #[test]
    fn a_project_repository_can_be_the_parent_and_needs_no_import_in_its_own_package() {
        let base = "package shop;\n\nimport org.springframework.data.jpa.repository.JpaRepository;\n\npublic interface BaseRepository<T, ID> extends JpaRepository<T, ID> {\n}\n";
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/BaseRepository.java", base);
        host.add_file("src/main/java/other/ChainRepository.java", "package other;\n\nimport shop.BaseRepository;\n\npublic interface ChainRepository<T, K> extends BaseRepository<T, K> {\n}\n");
        let text = java("jakarta.persistence", "Long");
        host.open_editor(JAVA_PATH, "java", &text, &[]);
        let doc = Document { path: JAVA_PATH.into(), language: "java".into(), text, version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.answer_next_prompt(Some(&["project:BaseRepository"]));
        host.answer_next_prompt(Some(&["no"]));
        host.answer_next_prompt(Some(&["OrderRepository"]));
        host.answer_next_prompt(Some(&[]));
        let (_, text) = created(plugin.resolve_file_action(&doc, pending));
        assert_eq!(text, "package shop;\n\npublic interface OrderRepository extends BaseRepository<Order, Long> {\n}\n");
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.answer_next_prompt(Some(&["project:ChainRepository"]));
        host.answer_next_prompt(Some(&["no"]));
        host.answer_next_prompt(Some(&["OrderRepository"]));
        host.answer_next_prompt(Some(&[]));
        let (_, text) = created(plugin.resolve_file_action(&doc, pending));
        assert!(text.contains("import other.ChainRepository;") && text.contains("extends ChainRepository<Order, Long>"), "{text}");
    }

    #[test]
    fn the_standard_action_is_offered_on_an_entity_view_and_needs_blaze_persistence() {
        let view = "package shop;\n\nimport com.blazebit.persistence.view.EntityView;\nimport com.blazebit.persistence.view.IdMapping;\n\n@EntityView(Order.class)\npublic interface OrderView {\n\n    @IdMapping\n    Long getId();\n}\n";
        let view_path = "/project/src/main/java/shop/OrderView.java";
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("pom.xml", "<project><dependencies><dependency><groupId>com.blazebit</groupId><artifactId>blaze-persistence-integration-spring-data-3.4</artifactId></dependency></dependencies></project>");
        host.open_editor(view_path, "java", view, &[]);
        let doc = Document { path: view_path.into(), language: "java".into(), text: view.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let titles: Vec<_> = plugin.file_actions(&doc, Range::default()).into_iter().map(|a| a.title).collect();
        assert!(titles.iter().any(|t| t == TITLE) && !titles.iter().any(|t| t == BATCH_TITLE), "{titles:?}");
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.answer_next_prompt(Some(&["OrderViewRepository"]));
        host.answer_next_prompt(Some(&[]));
        let (_, text) = created(plugin.resolve_file_action(&doc, pending));
        assert!(text.contains("extends EntityViewRepository<OrderView, Long>"), "{text}");
    }

    #[test]
    fn jakarta_list_parents_and_uuid_import() {
        let text = "package shop;\nimport jakarta.persistence.*;\nimport java.util.UUID;\n@Entity\npublic class Order {\n    @Id UUID id;\n}\n";
        let (_, result) = run(JAVA_PATH, text, "java", &["ListPagingAndSortingRepository", "no", "OrderRepository"]);
        let (_, out) = created(result);
        assert!(out.contains("import java.util.UUID;\nimport org.springframework.data.repository.ListPagingAndSortingRepository;"), "{out}");
        assert!(out.contains("extends ListPagingAndSortingRepository<Order, UUID>"), "{out}");
    }

    #[test]
    fn kotlin_jakarta_and_javax_render() {
        let source = "package shop\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\n@Entity\nclass Order { @Id var id: Int? = null }\n";
        let (_, result) = run(KOTLIN_PATH, source, "kotlin", &["ListCrudRepository", "yes", "OrderRepository"]);
        let (path, text) = created(result);
        assert_eq!(path, "/project/src/main/kotlin/shop/OrderRepository.kt");
        assert_eq!(
            text,
            "package shop\n\nimport org.springframework.data.jpa.repository.JpaSpecificationExecutor\nimport org.springframework.data.repository.ListCrudRepository\n\ninterface OrderRepository : ListCrudRepository<Order, Int>, JpaSpecificationExecutor<Order>\n"
        );
        let javax = source.replace("jakarta", "javax");
        let (_, result) = run(KOTLIN_PATH, &javax, "kotlin", &["CrudRepository", "no", "OrderRepository"]);
        assert!(created(result).1.contains("interface OrderRepository : CrudRepository<Order, Int>\n"));
    }

    #[test]
    fn unproven_identifiers_are_refused_with_a_message_and_create_nothing() {
        let cases = [
            "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Order {\n    @EmbeddedId OrderKey id;\n}\n",
            "package shop;\nimport jakarta.persistence.*;\n@Entity\n@IdClass(OrderKey.class)\npublic class Order {\n    @Id Long a;\n    @Id Long b;\n}\n",
            "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Order {\n    @Id @Column Long a;\n    @Id Long b;\n}\n",
            "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Order {\n    String name;\n}\n",
            "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Order {\n    @Id BigDecimal id;\n}\n",
            "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Order {\n    @Id OrderKey id;\n}\n",
            "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Order extends Base {\n    @Id Long id;\n}\n",
            "package shop;\nimport org.springframework.data.annotation.Id;\nimport jakarta.persistence.Entity;\n@Entity\npublic class Order {\n    @Id Long id;\n}\n",
        ];
        for (index, text) in cases.iter().enumerate() {
            let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
            host.open_editor(JAVA_PATH, "java", text, &[]);
            let doc = Document { path: JAVA_PATH.into(), language: "java".into(), text: (*text).into(), version: 1 };
            let facts = source(&doc);
            assert!(facts.is_err(), "case {index} must be refused");
            assert!(!facts.err().unwrap().is_empty());
        }
    }

    #[test]
    fn existing_targets_in_either_language_cancellation_and_stale_source_create_nothing() {
        let source = java("jakarta.persistence", "Long");
        for existing in ["src/main/java/shop/OrderRepository.java", "src/main/java/shop/OrderRepository.kt"] {
            let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
            host.open_editor(JAVA_PATH, "java", &source, &[]);
            host.add_file(existing, "interface OrderRepository {}");
            let doc = Document { path: JAVA_PATH.into(), language: "java".into(), text: source.clone(), version: 1 };
            let mut plugin = JpaAssist::default();
            let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
            for answer in ["JpaRepository", "no", "OrderRepository"] {
                host.answer_next_prompt(Some(&[answer]));
            }
            host.answer_next_prompt(Some(&[]));
            let message = format!("{:?}", plugin.resolve_file_action(&doc, pending).err().unwrap());
            assert!(message.contains("already exists"), "{message}");
        }
        let (_, result) = run(JAVA_PATH, &source, "java", &["JpaRepository", "no", "OrderRepository"]);
        assert!(result.is_ok());
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(JAVA_PATH, "java", &source, &[]);
        let doc = Document { path: JAVA_PATH.into(), language: "java".into(), text: source.clone(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)));
    }

    #[test]
    fn invalid_or_clashing_names_are_refused() {
        let source = java("jakarta.persistence", "Long");
        for name in ["Order", "JpaRepository", "1Bad", "bad name", ""] {
            let (_, result) = run(JAVA_PATH, &source, "java", &["JpaRepository", "no", name]);
            assert!(result.is_err(), "{name}");
        }
    }

    #[test]
    #[ignore = "requires javac and SPRING_DATA_CLASSPATH (spring-data-commons and spring-data-jpa jars, Spring Data 3.x)"]
    fn generated_java_repositories_compile_against_spring_data() {
        let classpath = std::env::var("SPRING_DATA_CLASSPATH").expect("explicit classpath");
        let base = std::env::temp_dir().join(format!("jpa-repository-compile-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("Order.java"), "package shop;\npublic class Order {}\n").unwrap();
        let mut files = vec![base.join("Order.java")];
        let source = java("jakarta.persistence", "Long");
        let doc = Document { path: JAVA_PATH.into(), language: "java".into(), text: source, version: 1 };
        let facts = Facts { entity: "Order".into(), id_type: "Long".into(), id_import: None, jakarta: true, kotlin: false };
        for (index, (parent, _)) in parents(true).iter().enumerate() {
            let name = format!("R{index}");
            let (_, text) = render(&doc, &facts, parent, index % 2 == 0, &name).unwrap();
            let file = base.join(format!("{name}.java"));
            std::fs::write(&file, text).unwrap();
            files.push(file);
        }
        let result = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &classpath, "-d"]).arg(&base).args(&files).output().expect("javac installed");
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn crlf_entities_produce_crlf_only_repositories_in_java_and_kotlin() {
        let java_source = java("jakarta.persistence", "Long").replace('\n', "\r\n");
        let kotlin_source = "package shop\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\n@Entity\nclass Order { @Id var id: Int? = null }\n".replace('\n', "\r\n");
        for (path, source, language) in [(JAVA_PATH, java_source, "java"), (KOTLIN_PATH, kotlin_source, "kotlin")] {
            let (_, result) = run(path, &source, language, &["ListCrudRepository", "yes", "OrderRepository"]);
            let text = created(result).1;
            assert!(text.contains("\r\n") && !text.replace("\r\n", "").contains('\n'), "{language}: {text:?}");
        }
    }

    #[test]
    #[ignore = "requires Kotlin compiler (KOTLIN_COMPILER_CLASSPATH, KOTLIN_STDLIB) and SPRING_DATA_CLASSPATH (Spring Data 3.x jars)"]
    fn generated_kotlin_repositories_compile_against_spring_data() {
        let spring = std::env::var("SPRING_DATA_CLASSPATH").expect("explicit classpath");
        let compiler = std::env::var("KOTLIN_COMPILER_CLASSPATH").expect("explicit Kotlin compiler classpath");
        let stdlib = std::env::var("KOTLIN_STDLIB").expect("explicit Kotlin standard library jar");
        let base = std::env::temp_dir().join(format!("jpa-kotlin-repository-compile-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("Order.kt"), "package shop\nclass Order\n").unwrap();
        let mut files = vec![base.join("Order.kt")];
        let source = "package shop\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\n@Entity\nclass Order { @Id var id: Long? = null }\n";
        let doc = Document { path: KOTLIN_PATH.into(), language: "kotlin".into(), text: source.into(), version: 1 };
        let facts = Facts { entity: "Order".into(), id_type: "Long".into(), id_import: None, jakarta: true, kotlin: true };
        for (index, (parent, _)) in parents(true).iter().enumerate() {
            let name = format!("R{index}");
            let (_, text) = render(&doc, &facts, parent, index % 2 == 0, &name).unwrap();
            let file = base.join(format!("{name}.kt"));
            std::fs::write(&file, text).unwrap();
            files.push(file);
        }
        let result = std::process::Command::new("java")
            .args(["-classpath", &compiler, "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler", "-no-stdlib", "-no-reflect", "-jvm-target", "17", "-classpath", &format!("{spring}:{stdlib}"), "-d"])
            .arg(&base)
            .args(&files)
            .output()
            .expect("java installed");
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(base).unwrap();
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ORDER_PATH: &str = "/project/src/main/java/shop/Order.java";

    fn entity(name: &str, id: &str, ns: &str) -> String {
        format!("package shop;\nimport {ns}.*;\nimport java.util.UUID;\n@Entity\npublic class {name} {{\n    @Id {id} id;\n}}\n")
    }

    fn files(ns: &str) -> Vec<(String, String)> {
        vec![
            ("src/main/java/shop/Order.java".into(), entity("Order", "Long", ns)),
            ("src/main/java/shop/Customer.java".into(), entity("Customer", "UUID", ns)),
            ("src/main/java/shop/Broken.java".into(), "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Broken {\n    String name;\n}\n".into()),
            ("src/main/java/shop/Item.java".into(), entity("Item", "Long", ns)),
            ("src/main/java/shop/ItemRepository.java".into(), "package shop;\nimport org.springframework.data.jpa.repository.JpaRepository;\npublic interface ItemRepository extends JpaRepository<Item, Long> {\n}\n".into()),
        ]
    }

    fn batch(files: &[(String, String)], picks: &[&str], answers: &[&str]) -> (MockHost, Result<FileAction, Error>) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        for (name, body) in files {
            host.add_file(name, body);
        }
        let order = files.iter().find(|(n, _)| n.ends_with("Order.java")).unwrap().1.clone();
        host.open_editor(ORDER_PATH, "java", &order, &[]);
        let doc = Document { path: ORDER_PATH.into(), language: "java".into(), text: order, version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == BATCH_TITLE).expect("menu entry");
        assert!(pending.operations.is_empty() && pending.edits.is_empty());
        host.answer_next_prompt(Some(picks));
        for answer in answers {
            host.answer_next_prompt(Some(&[*answer]));
        }
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&doc, pending);
        (host, result)
    }

    #[test]
    fn creates_one_no_overwrite_file_per_chosen_entity_with_one_parent_choice() {
        let (_, result) = batch(&files("jakarta.persistence"), &["0", "1"], &["JpaRepository", "yes"]);
        let action = result.unwrap();
        assert!(action.edits.is_empty() && action.operations.len() == 2);
        let mut seen = Vec::new();
        for operation in &action.operations {
            let FileOperation::Create { path, text, overwrite } = operation else { panic!("create") };
            assert!(!overwrite);
            seen.push((path.rsplit('/').next().unwrap().to_owned(), text.clone()));
        }
        seen.sort();
        assert_eq!(seen[0].0, "CustomerRepository.java");
        assert!(seen[0].1.contains("import java.util.UUID;") && seen[0].1.contains("extends JpaRepository<Customer, UUID>, JpaSpecificationExecutor<Customer>"), "{}", seen[0].1);
        assert_eq!(seen[1].0, "OrderRepository.java");
        assert!(seen[1].1.contains("extends JpaRepository<Order, Long>, JpaSpecificationExecutor<Order>"), "{}", seen[1].1);
    }

    #[test]
    fn javax_entities_get_javax_parents_and_entities_with_repositories_are_not_offered() {
        let (_, result) = batch(&files("javax.persistence"), &["0", "1"], &["PagingAndSortingRepository", "no"]);
        let action = result.unwrap();
        assert_eq!(action.operations.len(), 2, "Item already has a repository, Broken is refused");
        let (_, result) = batch(&files("javax.persistence"), &["0", "1"], &["ListCrudRepository", "no"]);
        assert!(result.is_err(), "List* parents are not offered for javax");
    }

    #[test]
    fn selection_bounds_and_cancellation_create_nothing() {
        let (_, result) = batch(&files("jakarta.persistence"), &["0", "0"], &["JpaRepository", "no"]);
        assert!(format!("{:?}", result.err().unwrap()).contains("once"));
        let (_, result) = batch(&files("jakarta.persistence"), &["9"], &["JpaRepository", "no"]);
        assert!(result.is_err());
        let (_, result) = batch(&files("jakarta.persistence"), &[], &["JpaRepository", "no"]);
        assert!(format!("{:?}", result.err().unwrap()).contains("pick fewer"));
        let (host, _) = batch(&files("jakarta.persistence"), &["0"], &[]);
        drop(host);
    }

    #[test]
    fn more_than_ten_entities_say_pick_fewer() {
        let mut many = files("jakarta.persistence");
        for n in 0..11 {
            many.push((format!("src/main/java/shop/Extra{n}.java"), entity(&format!("Extra{n}"), "Long", "jakarta.persistence")));
        }
        let all: Vec<String> = (0..13).map(|i| i.to_string()).collect();
        let picks: Vec<&str> = all.iter().map(String::as_str).collect();
        let (_, result) = batch(&many, &picks, &["JpaRepository", "no"]);
        assert!(format!("{:?}", result.err().unwrap()).contains("pick fewer"));
    }

    #[test]
    fn only_refused_entities_are_listed_with_reasons() {
        let only_broken: Vec<_> = files("jakarta.persistence").into_iter().filter(|(n, _)| n.ends_with("Order.java") || n.ends_with("Broken.java") || n.ends_with("OrderRepository.java")).collect();
        let mut only = only_broken;
        only.push(("src/main/java/shop/OrderRepository.java".into(), "package shop;\nimport org.springframework.data.jpa.repository.JpaRepository;\npublic interface OrderRepository extends JpaRepository<Order, Long> {\n}\n".into()));
        let (_, result) = batch(&only, &[], &[]);
        let message = format!("{:?}", result.err().unwrap());
        assert!(message.contains("No entity without a repository") && message.contains("Broken: No @Id field"), "{message}");
    }

    #[test]
    fn existing_same_named_repository_files_in_either_language_are_refused() {
        let mut with_clash = files("jakarta.persistence");
        with_clash.push(("src/main/java/shop/OrderRepository.kt".into(), "interface OrderRepository".into()));
        let (_, result) = batch(&with_clash, &["0", "1"], &["JpaRepository", "no"]);
        assert!(format!("{:?}", result.err().unwrap()).contains("already exists"));
    }

    #[test]
    fn an_oversized_review_asks_for_fewer_entities() {
        let package = (0..16).map(|i| format!("{}{i:02}", "p".repeat(28))).collect::<Vec<_>>().join(".");
        let directory = package.replace('.', "/");
        let mut many = Vec::new();
        for n in 0..10 {
            let name = "E".repeat(105);
            let text = format!("package {package};\nimport jakarta.persistence.*;\n@Entity\npublic class {name}{n} {{\n    @Id Long id;\n}}\n");
            many.push((format!("src/main/java/{directory}/{}{n}.java", "E".repeat(105)), text));
        }
        let order = many[0].0.clone();
        for n in 0..6 {
            let broken = format!("package {package};\nimport jakarta.persistence.*;\n@Entity\npublic class {}B{n} {{\n}}\n", "E".repeat(105));
            many.push((format!("src/main/java/{directory}/{}B{n}.java", "E".repeat(105)), broken));
        }
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        for (name, body) in &many {
            host.add_file(name, body);
        }
        let path = format!("/project/{order}");
        host.open_editor(&path, "java", &many[0].1, &[]);
        let doc = Document { path, language: "java".into(), text: many[0].1.clone(), version: 1 };
        let mut plugin = JpaAssist::default();
                let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == BATCH_TITLE).unwrap();
        let all: Vec<String> = (0..10).map(|i| i.to_string()).collect();
        let picks: Vec<&str> = all.iter().map(String::as_str).collect();
        host.answer_next_prompt(Some(&picks));
        host.answer_next_prompt(Some(&["JpaRepository"]));
        host.answer_next_prompt(Some(&["no"]));
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&doc, pending);
        let message = format!("{:?}", result.err().unwrap());
        assert!(message.contains("16 KiB") && message.contains("pick fewer"), "{message}");
    }

    #[test]
    #[ignore = "requires javac and SPRING_DATA_CLASSPATH (source scripts/jpa-compile-env.sh)"]
    fn generated_batch_repositories_compile_against_spring_data() {
        let classpath = std::env::var("SPRING_DATA_CLASSPATH").expect("explicit classpath");
        let base = std::env::temp_dir().join(format!("jpa-batch-repository-compile-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let mut paths = Vec::new();
        for name in ["Order", "Customer"] {
            let file = base.join(format!("{name}.java"));
            std::fs::write(&file, format!("package shop;\npublic class {name} {{}}\n")).unwrap();
            paths.push(file);
        }
        let (_, result) = batch(&files("jakarta.persistence"), &["0", "1"], &["ListPagingAndSortingRepository", "yes"]);
        for operation in result.unwrap().operations {
            let FileOperation::Create { path, text, .. } = operation else { panic!("create") };
            let file = base.join(path.rsplit('/').next().unwrap());
            std::fs::write(&file, text).unwrap();
            paths.push(file);
        }
        let result = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &classpath, "-d"]).arg(&base).args(&paths).output().expect("javac installed");
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(base).unwrap();
    }
}
