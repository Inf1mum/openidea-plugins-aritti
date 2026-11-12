//! Create a missing Spring Data repository, projection or DTO from a reference to a type that does not exist yet.
//! The name decides what is offered: it must end with the repository suffix, a projection suffix or the DTO postfix,
//! and its stem must name an entity of the project. Nothing is offered inside an entity class.
use super::*;
use super::dto_conventions;
use super::projection_links::{Conventions, Kind as Family};
use crate::model::{import_edit, is_ident};
use openidea_plugin::text::offset;

#[path = "unresolved_context.rs"]
mod context;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Created {
    Repository,
    Projection,
    Dto,
    Entity,
    MappedSuperclass,
    ContextDto,
}

pub(super) const REPOSITORY_TITLE: &str = "Create missing Spring Data repository…";
pub(super) const PROJECTION_TITLE: &str = "Create missing projection…";
pub(super) const DTO_TITLE: &str = "Create missing DTO…";
const DEFAULT_PROJECTION_SUFFIXES: &str = "Info,Prj,Projection,VO,Vo,View,Request,Browse";
const MAX_CANDIDATES: u32 = 64;

fn repository_suffix() -> String {
    host::setting_string("repositoryClassSuffix").filter(|suffix| !suffix.is_empty()).unwrap_or_else(|| "Repository".into())
}

fn projection_suffixes() -> Vec<String> {
    let configured = host::setting_string("unresolvedProjectionSuffixes").unwrap_or_else(|| DEFAULT_PROJECTION_SUFFIXES.into());
    let mut suffixes: Vec<String> = configured.split(',').map(|suffix| suffix.trim().to_owned()).filter(|suffix| !suffix.is_empty()).collect();
    suffixes.extend(host::setting_string("projectionClassPostfix").filter(|postfix| !postfix.is_empty()));
    suffixes
}

fn dto_postfix() -> String {
    host::setting_string("dtoClassPostfix").unwrap_or_else(|| "Dto".into())
}

/// The identifier under `selection.start`.
fn word_at(doc: &Document, selection: Range) -> Option<String> {
    let at = offset(&doc.text, selection.start)?;
    let bytes = doc.text.as_bytes();
    let start = at - bytes[..at].iter().rev().take_while(|&&byte| is_ident(byte)).count();
    let end = at + bytes[at..].iter().take_while(|&&byte| is_ident(byte)).count();
    let word = doc.text.get(start..end)?;
    (word.chars().next().is_some_and(|ch| ch.is_ascii_uppercase()) && identifier(word)).then(|| word.to_owned())
}

fn declared_or_imported(doc: &Document, name: &str) -> bool {
    let masked = mask(&doc.text);
    let imported = masked.lines().filter_map(|line| line.trim().strip_prefix("import ")).any(|import| import.trim_end_matches(';').trim().rsplit('.').next() == Some(name));
    imported || ["class", "interface", "record", "enum", "object", "typealias"].iter().any(|keyword| words(&masked, keyword).any(|at| masked[at + keyword.len()..].trim_start().starts_with(name)))
}

fn stem_of<'a>(name: &'a str, suffix: &str) -> Option<&'a str> {
    name.strip_suffix(suffix).filter(|stem| !stem.is_empty())
}

pub(super) fn offers(doc: &Document, selection: Range) -> Vec<(Created, &'static str)> {
    let mut offered = named_offers(doc, selection);
    offered.extend(context::offers(doc, selection));
    offered
}

fn named_offers(doc: &Document, selection: Range) -> Vec<(Created, &'static str)> {
    let Some(name) = word_at(doc, selection) else {
        return Vec::new();
    };
    if declared_or_imported(doc, &name) || parse_class(&doc.text, &doc.path).is_some_and(|class| class.kind == Kind::Entity) {
        return Vec::new();
    }
    let mut offered = Vec::new();
    if stem_of(&name, &repository_suffix()).is_some() {
        offered.push((Created::Repository, REPOSITORY_TITLE));
    }
    if doc.language == "java" && projection_suffixes().iter().any(|suffix| stem_of(&name, suffix).is_some()) {
        offered.push((Created::Projection, PROJECTION_TITLE));
    }
    if stem_of(&name, &dto_postfix()).is_some() {
        offered.push((Created::Dto, DTO_TITLE));
    }
    offered
}

pub(super) fn absolute(root: &str, file: String) -> String {
    if file.starts_with('/') { file } else { format!("{}/{file}", root.trim_end_matches('/')) }
}

fn type_exists(project: &Project, root: &str, name: &str) -> Result<bool, Error> {
    for extension in ["java", "kt"] {
        let files = project.find_files(&format!("**/{name}.{extension}"), 2)?;
        if files.into_iter().map(|file| absolute(root, file)).any(|file| inside(root, &file)) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The source document of the entity named `entity`: exactly one `@Entity` file of that name must exist.
pub(super) fn entity_document(project: &Project, root: &str, entity: &str, kotlin: bool) -> Result<Document, Error> {
    let extension = if kotlin { "kt" } else { "java" };
    let mut found = Vec::new();
    for file in project.find_files(&format!("**/{entity}.{extension}"), MAX_CANDIDATES)? {
        let file = absolute(root, file);
        if path(&file).as_deref() != Some(file.as_str()) || !inside(root, &file) {
            continue;
        }
        let text = project.read_file(&file)?;
        if text.len() > MAX_TEXT {
            continue;
        }
        let document = Document { path: file, text, language: if kotlin { "kotlin" } else { "java" }.into(), version: 0 };
        if current_class(&document).is_some_and(|class| class.kind == Kind::Entity) {
            found.push(document);
        }
    }
    match found.len() {
        0 => Err(format!("No @Entity class named `{entity}` was found in this project, so nothing can be derived for it.").into()),
        1 => Ok(found.remove(0)),
        _ => Err(format!("Several @Entity classes are named `{entity}`; no guess is made.").into()),
    }
}

fn pick_fields(title: &str, available: &[(String, String)]) -> Result<Vec<(String, String)>, Error> {
    let items: Vec<_> = available.iter().map(|(name, ty)| PickItem::new(name, name).description(ty)).collect();
    let chosen = host::pick_many(title, &items)?.ok_or(Error::Cancelled)?;
    let mut seen = HashSet::new();
    if chosen.is_empty() || chosen.iter().any(|name| !available.iter().any(|(field, _)| field == name) || !seen.insert(name.as_str())) {
        return Err("Choose one or more offered attributes exactly once.".into());
    }
    Ok(available.iter().filter(|(name, _)| chosen.contains(name)).cloned().collect())
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project, created: Created) -> Result<FileAction, Error> {
    if matches!(created, Created::Entity | Created::MappedSuperclass | Created::ContextDto) {
        return context::resolve(pending, doc, project, created);
    }
    resolve_with(pending, doc, project, created, None)
}

/// `context_entity`: the entity a DTO is created for when its name does not say (usage-driven DTO creation).
fn resolve_with(pending: &Pending, doc: &Document, project: &Project, created: Created, context_entity: Option<String>) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let name = word_at(doc, pending.selection).ok_or(Error::Stale)?;
    let kotlin = doc.language == "kotlin";
    let entity_name = match context_entity {
        Some(entity) => entity,
        None => {
            let (suffix, family) = match created {
                Created::Repository => (repository_suffix(), None),
                Created::Dto => (dto_postfix(), Some(Family::Dto)),
                Created::Projection => (projection_suffixes().into_iter().find(|suffix| stem_of(&name, suffix).is_some()).ok_or(Error::Stale)?, Some(Family::Projection)),
                _ => return Err(Error::Stale),
            };
            let stem = stem_of(&name, &suffix).ok_or(Error::Stale)?.to_owned();
            match family {
                Some(Family::Dto) => Conventions::load_for(Family::Dto)?.entity_of(&name, "").unwrap_or(stem),
                _ => stem,
            }
        }
    };
    if type_exists(project, &pending.root, &name)? {
        return Err(format!("A type named `{name}` already exists in this project; nothing is created.").into());
    }
    let entity_doc = entity_document(project, &pending.root, &entity_name, kotlin)?;
    let class = current_class(&entity_doc).ok_or(Error::Stale)?;
    let directory = entity_doc.path.rsplit_once('/').ok_or(Error::Stale)?.0.to_owned();
    let declared = package(&entity_doc).ok_or(Error::Stale)?;
    let (target, text) = match created {
        Created::Repository => {
            let facts = repository_action::source(&entity_doc).map_err(Error::from)?;
            repository_action::render(&entity_doc, &facts, "JpaRepository", false, &name)?
        }
        Created::Projection => {
            let form = projection_action::Form::Interface;
            let available = projection_action::offered(&class, form);
            if available.is_empty() {
                return Err("No plain scalar attribute has a name usable as a projection property.".into());
            }
            let fields = pick_fields("Projection properties (plain scalar attributes only)", &available)?;
            let conventions = Conventions::load_for(Family::Projection)?;
            let text = conventions.with_comment(&projection_action::render(&class, &declared, form, &name, &fields), class.package.as_deref(), &class.name);
            (format!("{directory}/{name}.java"), text)
        }
        Created::Dto => {
            let source = dto_action::source(&entity_doc).ok_or("The entity is not a plain class this generator can derive a DTO from.")?;
            if !source.kotlin {
                if let Some(level) = dto_action::java_level_below_record(&entity_doc.path) {
                    return Err(dto_action::record_refusal(level).into());
                }
            }
            let available = dto_action::fields(&source).ok_or("No bounded unambiguous scalar fields are available for a DTO.")?;
            let fields = pick_fields("DTO fields (scalar attributes only)", &available)?;
            let chosen: Vec<String> = fields.iter().map(|(field, _)| field.clone()).collect();
            let mut narrowed = source.clone();
            narrowed.attributes.retain(|attribute| attribute.relation == Relation::None && chosen.contains(&attribute.name));
            let none = |_: &str| None;
            let generated = dto(&narrowed, &none, Some(&chosen));
            let (original, replacement) = if source.kotlin {
                (format!("data class {}Dto(", source.name), format!("data class {name}("))
            } else {
                (format!("public record {}Dto(", source.name), format!("public record {name}("))
            };
            if !generated.contains(&original) {
                return Err(Error::Stale);
            }
            let conventions = dto_conventions::conventions()?;
            let generated = dto_conventions::finish(generated.replacen(&original, &replacement, 1), &source, &name, &conventions, dto_conventions::serializable()?)?;
            (format!("{directory}/{name}.{}", if source.kotlin { "kt" } else { "java" }), generated)
        }
        _ => return Err(Error::Stale),
    };
    if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) || text.len() > MAX_TEXT {
        return Err(Error::Stale);
    }
    no_collision(project, &pending.root, &directory, &name, None)?;
    let import = (package(doc).as_deref() != Some(declared.as_str()))
        .then(|| import_edit(&doc.text, &[format!("{declared}.{name}").as_str()], kotlin))
        .flatten()
        .map(|edit| edit.to_text_edit(&doc.text));
    let import_note = import.as_ref().map(|edit| format!("\nImport added to {}:\n{}", doc.path, edit.text)).unwrap_or_default();
    if !ui_kit::prompts::confirm_exact(
        "Create the missing type?",
        &format!("Reference: {} (version {})\nEntity: {} ({})\nNew file: {target}\n\n{text}{import_note}\nThe file is created next to the entity; no existing file is overwritten.", doc.path, doc.version, class.name, entity_doc.path),
    )? {
        return Err(Error::Cancelled);
    }
    let project = current(pending, doc)?;
    no_collision(&project, &pending.root, &directory, &name, None)?;
    Ok(FileAction {
        title: format!("Create {name}"),
        kind: Some("quickfix".into()),
        operations: vec![FileOperation::Create { path: target.clone(), text, overwrite: false }],
        edits: import.map(|edit| vec![(doc.path.clone(), vec![edit])]).unwrap_or_default(),
        open: Some(target),
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ENTITY: &str = "package shop.domain;\n\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n\n@Entity\npublic class Customer {\n    @Id\n    private Long id;\n\n    private String title;\n}\n";
    const SERVICE: &str = "package shop.service;\n\nimport org.springframework.stereotype.Service;\n\n@Service\npublic class CustomerService {\n    private CustomerRepository customers;\n    private CustomerDto last;\n    private CustomerView view;\n}\n";
    const SERVICE_PATH: &str = "/project/src/main/java/shop/service/CustomerService.java";

    fn prepare() -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/domain/Customer.java", ENTITY);
        host.add_file("src/main/java/shop/service/CustomerService.java", SERVICE);
        host.open_editor(SERVICE_PATH, "java", SERVICE, &[]);
        let doc = Document { path: SERVICE_PATH.into(), language: "java".into(), text: SERVICE.into(), version: 1 };
        (host, JpaAssist::default(), doc)
    }

    fn at(word: &str) -> Range {
        let start = SERVICE.find(word).unwrap();
        Range::new(openidea_plugin::text::position(SERVICE, start), openidea_plugin::text::position(SERVICE, start + 2))
    }

    fn titles(plugin: &mut JpaAssist, doc: &Document, word: &str) -> Vec<String> {
        plugin.file_actions(doc, at(word)).into_iter().map(|action| action.title).filter(|title| title.starts_with("Create missing")).collect()
    }

    #[test]
    fn the_suffix_decides_which_creation_is_offered() {
        let (_host, mut plugin, doc) = prepare();
        assert_eq!(titles(&mut plugin, &doc, "CustomerRepository"), [REPOSITORY_TITLE]);
        assert_eq!(titles(&mut plugin, &doc, "CustomerDto"), [DTO_TITLE]);
        assert_eq!(titles(&mut plugin, &doc, "CustomerView"), [PROJECTION_TITLE]);
        assert!(titles(&mut plugin, &doc, "CustomerService").is_empty());
    }

    #[test]
    fn suffix_settings_replace_the_defaults() {
        let (host, mut plugin, doc) = prepare();
        host.set_setting("repositoryClassSuffix", "\"Dao\"");
        host.set_setting("dtoClassPostfix", "\"Data\"");
        assert!(titles(&mut plugin, &doc, "CustomerRepository").is_empty());
        assert!(titles(&mut plugin, &doc, "CustomerDto").is_empty());
    }

    #[test]
    fn imported_declared_and_entity_references_are_not_offered() {
        let (_host, mut plugin, doc) = prepare();
        let imported = Document { text: SERVICE.replace("import org.springframework.stereotype.Service;", "import org.springframework.stereotype.Service;\nimport shop.domain.CustomerRepository;"), ..doc.clone() };
        assert!(plugin.file_actions(&imported, at("CustomerRepository")).iter().all(|action| !action.title.starts_with("Create missing")));
        let entity = Document { path: "/project/src/main/java/shop/domain/Customer.java".into(), text: ENTITY.replace("private String title;", "private CustomerDto title;"), ..doc };
        let start = entity.text.find("CustomerDto").unwrap();
        let range = Range::new(openidea_plugin::text::position(&entity.text, start), openidea_plugin::text::position(&entity.text, start + 2));
        assert!(plugin.file_actions(&entity, range).iter().all(|action| ![REPOSITORY_TITLE, PROJECTION_TITLE, DTO_TITLE].contains(&action.title.as_str())));
    }

    #[test]
    fn creating_the_repository_derives_the_id_type_and_imports_it_into_the_caller() {
        let (host, mut plugin, doc) = prepare();
        let pending = plugin.file_actions(&doc, at("CustomerRepository")).into_iter().find(|action| action.title == REPOSITORY_TITLE).unwrap();
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        let [FileOperation::Create { path, text, .. }] = action.operations.as_slice() else { panic!("one file") };
        assert!(path.ends_with("shop/domain/CustomerRepository.java"), "{path}");
        assert!(text.contains("public interface CustomerRepository extends JpaRepository<Customer, Long>"), "{text}");
        let [(_, edits)] = action.edits.as_slice() else { panic!("one import edit") };
        assert!(patched(SERVICE, edits).unwrap().contains("import shop.domain.CustomerRepository;"));
    }

    #[test]
    fn creating_a_dto_and_a_projection_use_the_chosen_fields_and_settings() {
        let (host, mut plugin, doc) = prepare();
        host.set_setting("dtoClassComment", "\"Transfer object of {@link {entity}}\"");
        let pending = plugin.file_actions(&doc, at("CustomerDto")).into_iter().find(|action| action.title == DTO_TITLE).unwrap();
        host.answer_next_prompt(Some(&["id", "title"]));
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        let [FileOperation::Create { text, .. }] = action.operations.as_slice() else { panic!("one file") };
        assert!(text.contains("/** Transfer object of {@link shop.domain.Customer} */") && text.contains("public record CustomerDto("), "{text}");
        let (host, mut plugin, doc) = prepare();
        let pending = plugin.file_actions(&doc, at("CustomerView")).into_iter().find(|action| action.title == PROJECTION_TITLE).unwrap();
        host.answer_next_prompt(Some(&["title"]));
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        let [FileOperation::Create { text, .. }] = action.operations.as_slice() else { panic!("one file") };
        assert!(text.contains("public interface CustomerView {\n    String getTitle();\n}"), "{text}");
    }

    #[test]
    fn an_existing_type_or_a_missing_entity_refuses() {
        let (host, mut plugin, doc) = prepare();
        host.add_file("src/main/java/shop/domain/CustomerRepository.java", "package shop.domain;\npublic interface CustomerRepository {}\n");
        let pending = plugin.file_actions(&doc, at("CustomerRepository")).into_iter().find(|action| action.title == REPOSITORY_TITLE).unwrap();
        assert!(format!("{:?}", plugin.resolve_file_action(&doc, pending).unwrap_err()).contains("already exists"));
        let (_host, mut plugin, doc) = prepare();
        let text = SERVICE.replace("CustomerRepository customers", "OrderRepository customers");
        let doc = Document { text, ..doc };
        let start = doc.text.find("OrderRepository").unwrap();
        let range = Range::new(openidea_plugin::text::position(&doc.text, start), openidea_plugin::text::position(&doc.text, start + 2));
        let pending = plugin.file_actions(&doc, range).into_iter().find(|action| action.title == REPOSITORY_TITLE).unwrap();
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
    }
}
