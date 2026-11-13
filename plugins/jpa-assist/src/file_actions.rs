//! Previewed source-local generation. Cross-file inverse facts are read-only;
//! inverse edits apply only to the invoking target document, whose version the host guards.
use super::*;
use openidea_plugin::file_actions::{FileAction, FileActions, FileOperation};

const MAX_TEXT: usize = 128 << 10;
#[path = "dto_action.rs"]
mod dto_action;
#[path = "ddl_action.rs"]
mod ddl_action;
#[path = "datasource_action.rs"]
mod datasource_action;
#[path = "attribute_refactor.rs"]
mod attribute_refactor;
#[path = "identifier_action.rs"]
mod identifier_action;
#[path = "entity_from_class.rs"]
mod entity_from_class;
#[path = "embedded_id_action.rs"]
mod embedded_id_action;
#[path = "extract_superclass.rs"]
mod extract_superclass;
#[path = "dto_convert_action.rs"]
mod dto_convert_action;
#[path = "flyway_misc_action.rs"]
mod flyway_misc_action;
#[path = "equality_action.rs"]
mod equality_action;
#[path = "ddl_entities_action.rs"]
mod ddl_entities_action;
#[path = "reverse_panel.rs"]
pub(crate) mod reverse_panel;
#[path = "ddl_todo_action.rs"]
mod ddl_todo_action;
#[path = "migration_action.rs"]
pub(crate) mod migration_action;
#[path = "liquibase_action.rs"]
mod liquibase_action;
#[path = "danger_levels.rs"]
pub(crate) mod danger_levels;
#[path = "rename_merge.rs"]
mod rename_merge;
#[path = "entity_scope.rs"]
mod entity_scope;
#[path = "change_routing.rs"]
pub(crate) mod change_routing;
#[path = "liquibase_inspector.rs"]
pub(crate) mod liquibase_inspector;
#[path = "schema_repair_action.rs"]
mod schema_repair_action;
#[path = "template_settings.rs"]
mod template_settings;
#[path = "type_map.rs"]
mod type_map;
#[path = "ddl_options.rs"]
mod ddl_options;
#[path = "envers_action.rs"]
mod envers_action;
#[path = "entity_graph_action.rs"]
mod entity_graph_action;
#[path = "mapper_action.rs"]
mod mapper_action;
#[path = "mapper_more_action.rs"]
mod mapper_more_action;
#[path = "mapper_naming.rs"]
mod mapper_naming;
#[path = "dto_links.rs"]
mod dto_links;
#[path = "mapstruct_model.rs"]
pub(crate) mod mapstruct_model;
#[path = "mapstruct_links.rs"]
pub(crate) mod mapstruct_links;
#[path = "dto_validation.rs"]
mod dto_validation;
#[path = "entity_view_window.rs"]
mod entity_view_window;
#[path = "entity_view_options.rs"]
mod entity_view_options;
#[path = "dto_assoc_action.rs"]
mod dto_assoc_action;
#[path = "pojo_relations.rs"]
mod pojo_relations;
#[path = "related_entity_action.rs"]
mod related_entity_action;
#[path = "tree_create_action.rs"]
mod tree_create_action;
pub(crate) fn run_tree_command(plugin: &mut JpaAssist, id: &str, ctx: &Invocation) -> Result<Option<String>, Error> { tree_create_action::run(plugin, id, ctx) }
#[path = "structure_actions.rs"]
mod structure_actions;
pub(crate) fn run_structure_command(plugin: &mut JpaAssist, id: &str, ctx: &Invocation, arg: Option<&str>) -> Result<Option<String>, Error> { structure_actions::run(plugin, id, ctx, arg) }
pub(crate) fn structure_button(plugin: &mut JpaAssist, id: &str, ctx: &Invocation) -> Result<(), Error> { structure_actions::button(plugin, id, ctx) }
pub(crate) fn structure_select(key: String) { structure_actions::select(key) }
pub(crate) fn structure_filter(items: Vec<TreeItem>) -> Vec<TreeItem> { structure_actions::filter(items) }
#[path = "converter_action.rs"]
mod converter_action;
#[path = "hibernate_action.rs"]
mod hibernate_action;
#[path = "projection_action.rs"]
mod projection_action;
#[path = "projection_links.rs"]
pub(crate) mod projection_links;
#[path = "dto_conventions.rs"]
mod dto_conventions;
#[path = "dto_complete.rs"]
mod dto_complete;
#[path = "injection_detect.rs"]
mod injection_detect;
#[path = "unresolved_create.rs"]
mod unresolved_create;
#[path = "call_assist.rs"]
mod call_assist;
#[path = "entity_view_injection.rs"]
pub(crate) mod entity_view_injection;
#[path = "project_parents.rs"]
mod project_parents;
#[path = "query_options.rs"]
mod query_options;
#[path = "nested_dto_action.rs"]
mod nested_dto_action;
#[path = "repository_action.rs"]
mod repository_action;
#[path = "auditing_action.rs"]
mod auditing_action;
#[path = "validation_action.rs"]
mod validation_action;
#[path = "query_method_action.rs"]
mod query_method_action;
#[path = "attribute_repo_method.rs"]
pub(crate) mod attribute_repo_method;
#[path = "association_advice.rs"]
mod association_advice;
#[path = "query_extract_action.rs"]
mod query_extract_action;
#[path = "query_edit_action.rs"]
mod query_edit_action;
#[path = "query_convert_action.rs"]
mod query_convert_action;
#[path = "index_query_action.rs"]
mod index_query_action;
#[path = "naming_action.rs"]
mod naming_action;
#[path = "injection_action.rs"]
mod injection_action;
#[path = "revision_repo_action.rs"]
mod revision_repo_action;
#[path = "modelmapper_action.rs"]
mod modelmapper_action;
pub(crate) use naming_action::{column_name as naming_column_name, table_name as naming_table_name};
pub(crate) use mapper_action::build_text;
pub(crate) use hibernate_action::{hibernate6, hibernate_core_majors, spring_boot_majors, type_findings as hibernate_type_findings};
// Reuse the unchanged plain manual scalar source gate for current-request templates.
pub(super) fn scalar_source(doc: &Document) -> Option<Class> { ddl_action::source(doc) }

pub(super) fn find_attribute_usages(ctx: &Invocation) -> Result<(),Error> { attribute_refactor::find_usages(ctx) }

#[derive(Clone, Copy)]
enum Operation {
    Entity,
    PersistenceClass,
    Attribute,
    Inverse,
    Dto,
    DtoClass,
    Ddl,
    Rename,
    Identifier,
    ConfiguredEntity,
    EmbeddedIdentifier,
    EntityFromClass,
    ExtractSuperclass,
    DtoToClass,
    DtoToRecord,
    Datasource,
    FlywayJavaMigration,
    FlywaySqlCallback,
    FlywayJavaCallback,
    EqualityMethods,
    LazySafeToString,
    DdlEntities,
    DdlEntityAttrs,
    TodoUncomment,
    TodoRemove,
    SchemaBaseline,
    FlywayMigration,
    LiquibaseChangelog,
    LiquibaseInspector(liquibase_inspector::Item),
    EnversToggle,
    EnversRevision,
    EntityGraph,
    MapstructMapper,
    AddMapping,
    BaseMapper,
    ExtendMapper,
    InjectMapper,
    ModelMapperConfig,
    ModelMapperConversion,
    BlazeEntityView,
    BlazeEntityViewOptions,
    DtoAssociations,
    AttributeConverter,
    HibernateUserType,
    HibernateJson,
    HibernateListener,
    SpringRepository,
    SpringProjection,
    SyncProjections,
    RenameWithProjections,
    SyncDtos,
    UnresolvedRepository,
    UnresolvedProjection,
    UnresolvedDto,
    UnresolvedContext(unresolved_create::Created),
    CreateRepositoryMethod,
    RenameWithDtos,
    NestedProjection,
    FlatDto,
    ValidationConstraint,
    LombokAnnotations,
    QueryMethod,
    QueryMethodOptions,
    QueryExtract,
    QueryEdit,
    QueryAsync,
    QueryDynamicProjection,
    IndexConstraint,
    NamedQuery,
    NameConstants,
    InjectRepository,
    AuditingFields,
    AuditingConfig,
    SpringRepositories,
    RevisionRepository,
    EnableEnversRepositories,
    EntityViewRepository,
    SchemaRepair,
    RelatedEntityTo,
    RelatedEntityFrom,
    AttributeRepositoryMethod,
}
impl Operation {
    fn key(self) -> &'static str {
        match self {
            Self::Entity => "entity",
            Self::PersistenceClass => "persistence-class",
            Self::Attribute => "attribute",
            Self::Inverse => "inverse",
            Self::Dto => "dto",
            Self::DtoClass => "dto-class",
            Self::Ddl => "ddl",
            Self::Rename => "rename-local",
            Self::Identifier => "identifier",
            Self::ConfiguredEntity => "configured-entity",
            Self::EmbeddedIdentifier => "embedded-identifier",
            Self::EntityFromClass => "entity-from-class",
            Self::ExtractSuperclass => "extract-superclass",
            Self::DtoToClass => "dto-to-class",
            Self::DtoToRecord => "dto-to-record",
            Self::Datasource => "datasource",
            Self::FlywayJavaMigration => "flyway-java-migration",
            Self::FlywaySqlCallback => "flyway-sql-callback",
            Self::FlywayJavaCallback => "flyway-java-callback",
            Self::EqualityMethods => "equality-methods",
            Self::LazySafeToString => "lazy-safe-to-string",
            Self::DdlEntities => "ddl-entities",
            Self::DdlEntityAttrs => "ddl-entity-attrs",
            Self::TodoUncomment => "todo-uncomment",
            Self::TodoRemove => "todo-remove",
            Self::SchemaBaseline => "schema-baseline",
            Self::FlywayMigration => "flyway-migration",
            Self::LiquibaseChangelog => "liquibase-changelog",
            Self::LiquibaseInspector(_) => "liquibase-inspector",
            Self::SchemaRepair => "schema-repair",
            Self::EnversToggle => "envers-toggle",
            Self::EnversRevision => "envers-revision",
            Self::EntityGraph => "entity-graph",
            Self::MapstructMapper => "mapstruct-mapper",
            Self::AddMapping => "add-mapping-method",
            Self::BaseMapper => "generic-base-mapper",
            Self::ExtendMapper => "extend-base-mapper",
            Self::InjectMapper => "inject-mapper",
            Self::ModelMapperConfig => "modelmapper-config",
            Self::ModelMapperConversion => "modelmapper-conversion",
            Self::BlazeEntityView => "blaze-entity-view",
            Self::BlazeEntityViewOptions => "blaze-entity-view-options",
            Self::DtoAssociations => "dto-associations",
            Self::AttributeConverter => "attribute-converter",
            Self::HibernateUserType => "hibernate-user-type",
            Self::HibernateJson => "hibernate-json",
            Self::HibernateListener => "hibernate-listener",
            Self::SpringRepository => "spring-repository",
            Self::QueryMethod => "query-method",
            Self::QueryExtract => "query-extract",
            Self::QueryEdit => "query-edit",
            Self::QueryMethodOptions => "query-options",
            Self::SyncProjections => "sync-projections",
            Self::RenameWithProjections => "rename-projections",
            Self::SyncDtos => "sync-dtos",
            Self::UnresolvedRepository => "unresolved-repository",
            Self::UnresolvedProjection => "unresolved-projection",
            Self::UnresolvedDto => "unresolved-dto",
            Self::UnresolvedContext(_) => "unresolved-context",
            Self::CreateRepositoryMethod => "create-repository-method",
            Self::RenameWithDtos => "rename-dtos",
            Self::QueryAsync => "query-async",
            Self::QueryDynamicProjection => "query-dynamic-projection",
            Self::SpringProjection => "spring-projection",
            Self::NestedProjection => "nested-projection",
            Self::FlatDto => "flat-dto",
            Self::ValidationConstraint => "validation-constraint",
            Self::LombokAnnotations => "lombok-annotations",
            Self::IndexConstraint => "index-constraint",
            Self::NamedQuery => "named-query",
            Self::NameConstants => "name-constants",
            Self::InjectRepository => "inject-repository",
            Self::AuditingFields => "auditing-fields",
            Self::AuditingConfig => "auditing-config",
            Self::SpringRepositories => "spring-repositories",
            Self::RevisionRepository => "revision-repository",
            Self::EnableEnversRepositories => "enable-envers-repositories",
            Self::EntityViewRepository => "entity-view-repository",
            Self::RelatedEntityTo => "related-entity-to",
            Self::RelatedEntityFrom => "related-entity-from",
            Self::AttributeRepositoryMethod => "attribute-repository-method",
        }
    }
}
#[derive(Clone)]
struct Pending {
    root: String,
    doc: Document,
    operation: Operation,
    selection: Range,
}
#[derive(Default)]
pub(crate) struct Generation {
    next: u64,
    pending: HashMap<String, Pending>,
}

pub(crate) fn identifier(value: &str) -> bool {
    value.len() <= 128 && value.is_ascii() && is_identifier(value)
        && !"_ abstract assert boolean break byte case catch char class const continue default do double else enum extends false final finally float for fun if implements import in instanceof int interface is long native new null object package private protected public record return sealed short static strictfp super switch synchronized this throw throws transient true try typealias val var void volatile when while yield".split_ascii_whitespace().any(|word| word == value)
}
pub(super) fn path(value: &str) -> Option<String> {
    let value = value.replace('\\', "/");
    let absolute = value.starts_with('/')
        || (value.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
            && value.as_bytes().get(1) == Some(&b':')
            && value.as_bytes().get(2) == Some(&b'/'));
    (absolute
        && value.len() <= 4096
        && !value.chars().any(char::is_control)
        && !value.split('/').any(|part| matches!(part, "." | ".."))
        && !value.contains(['*', '?', '[', ']', '{', '}']))
    .then_some(value)
}
pub(super) fn inside(root: &str, value: &str) -> bool {
    value.starts_with(&format!("{}/", root.trim_end_matches('/')))
}
fn package(doc: &Document) -> Option<String> {
    let masked = mask(&doc.text);
    let mut declarations = words(&masked, "package");
    let declared = if let Some(at) = declarations.next() {
        if declarations.next().is_some() {
            return None;
        }
        let rest = masked[at + 7..].trim_start();
        let end = rest.find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')).unwrap_or(rest.len());
        Some(rest[..end].to_owned())
    } else {
        None
    };
    let package = declared.or_else(|| package_of(&doc.path))?;
    (package.len() <= 512
        && package.split('.').all(identifier)
        && doc.path.rsplit_once('/')?.0.ends_with(&format!("/{}", package.replace('.', "/"))))
    .then_some(package)
}
fn annotation_is(text: &str, class: &Class, name: &str, namespace: &str) -> bool {
    let masked = mask(text);
    let annotations: Vec<_> = annotations(&masked).into_iter().filter(|a| a.name == name).collect();
    !annotations.is_empty()
        && annotations.iter().all(|a| {
            let written = &masked[a.start + 1..a.name_end];
            if written.contains('.') {
                return written == format!("{namespace}.{name}");
            }
            if written.contains(':') && !written.starts_with("field:") && !written.starts_with("get:") {
                return false;
            }
            let explicit: Vec<_> = class.imports.iter().filter(|import| import.rsplit('.').next() == Some(name)).collect();
            if !explicit.is_empty() {
                return explicit.len() == 1 && *explicit[0] == format!("{namespace}.{name}");
            }
            class.imports.iter().any(|import| import == &format!("{namespace}.*"))
                && class
                    .imports
                    .iter()
                    .filter(|import| import.ends_with(".*"))
                    .all(|import| import == &format!("{namespace}.*") || import == "java.util.*")
        })
}
fn namespace(text: &str, class: &Class) -> Option<&'static str> {
    let role = match class.kind { Kind::Entity => "Entity", Kind::Embeddable => "Embeddable", Kind::MappedSuperclass => "MappedSuperclass" };
    let candidates: Vec<_> = ["jakarta.persistence", "javax.persistence"]
        .into_iter()
        .filter(|namespace| annotation_is(text, class, role, namespace))
        .collect();
    let [namespace] = candidates.as_slice() else {
        return None;
    };
    let other = if *namespace == "jakarta.persistence" { "javax.persistence" } else { "jakarta.persistence" };
    (!class.imports.iter().any(|import| import.starts_with(other))).then_some(*namespace)
}
fn current_class(doc: &Document) -> Option<Class> {
    if has_word(&mask(&doc.text), "typealias") {
        return None;
    }
    if mask(&doc.text).lines().any(|line| line.trim_start().starts_with("import ") && line.contains(" as ")) {
        return None;
    }
    let mut class = parse_class(&doc.text, &doc.path).filter(|class| class.superclass.is_none())?;
    let masked = mask(&doc.text);
    if !identifier(&class.name)
        || words(&masked, "class").filter(|at| *at == 0 || masked.as_bytes()[at - 1] != b'.').count() != 1
        || class.body_end? >= doc.text.len()
        || package(doc).as_ref() != class.package.as_ref()
    {
        return None;
    }
    let ns = namespace(&doc.text, &class)?;
    if !class.imports.iter().any(|import| import.starts_with(ns)) {
        let role = match class.kind { Kind::Entity => "Entity", Kind::Embeddable => "Embeddable", Kind::MappedSuperclass => "MappedSuperclass" };
        class.imports.push(format!("{ns}.{role}"));
    }
    Some(class)
}
pub(crate) fn checked_after(doc: &Document, edits: &[TextEdit]) -> Result<String, Error> {
    let after = patched(&doc.text, edits)?;
    let class = parse_class(&after, &doc.path).ok_or(Error::Stale)?;
    if doc.language == "kotlin"
        && after.contains("MutableList<")
        && class.imports.iter().any(|import| import.rsplit('.').next() == Some("MutableList") && import != "kotlin.collections.MutableList")
    {
        return Err("A generated Kotlin collection type conflicts with an imported type.".into());
    }
    let mut names = HashMap::new();
    for import in &class.imports {
        let name = import.rsplit('.').next().ok_or(Error::Stale)?;
        if name == "*" {
            continue;
        }
        if names.insert(name, import).is_some_and(|previous| previous != import) {
            return Err("Generated imports would collide with an existing type name.".into());
        }
    }
    if after.len() > MAX_TEXT {
        return Err("Generated source exceeds the supported source bound.".into());
    }
    Ok(after)
}

fn current(pending: &Pending, doc: &Document) -> Result<Project, Error> {
    if doc.path != pending.doc.path
        || doc.language != pending.doc.language
        || doc.version != pending.doc.version
        || doc.text != pending.doc.text
    {
        return Err(Error::Stale);
    }
    let project = host::project_of(&doc.path).ok_or(Error::Stale)?;
    if path(&project.root()?).as_deref() != Some(pending.root.as_str()) || project.read_file(&doc.path)? != doc.text {
        return Err(Error::Stale);
    }
    Ok(project)
}
pub(super) fn no_collision(project: &Project, root: &str, directory: &str, name: &str, empty_source: Option<&str>) -> Result<(), Error> {
    for extension in ["java", "kt"] {
        let target = format!("{directory}/{name}.{extension}");
        if empty_source == Some(target.as_str()) {
            continue;
        }
        match project.read_file(&target) {
            Ok(_) => return Err("Target already exists; no file will be overwritten.".into()),
            Err(Error::NotFound(_)) => {}
            Err(error) => return Err(error),
        }
    }
    let relative = directory.strip_prefix(&format!("{}/", root.trim_end_matches('/'))).ok_or(Error::Stale)?;
    let deadline = Instant::now() + Duration::from_secs(1);
    for extension in ["java", "kt"] {
        let files = project.find_files(&format!("{relative}/*.{extension}"), 65)?;
        if files.len() >= 65 {
            return Err("Target folder exceeds bounded type collision discovery.".into());
        }
        for file in files {
            if Instant::now() >= deadline {
                return Err("Type collision discovery timed out.".into());
            }
            let text = project.read_file(&file)?;
            if text.len() > MAX_TEXT {
                return Err("A target-folder source is too large to verify collisions.".into());
            }
            let masked = mask(&text);
            for keyword in ["class", "interface", "record", "enum"] {
                if words(&masked, keyword).any(|at| {
                    masked[at + keyword.len()..]
                        .trim_start()
                        .strip_prefix(name)
                        .is_some_and(|rest| rest.chars().next().is_none_or(|ch| !ch.is_alphanumeric() && ch != '_'))
                }) {
                    return Err("A type with this name already exists in the target folder.".into());
                }
            }
        }
    }
    Ok(())
}
pub(super) fn patched(text: &str, edits: &[TextEdit]) -> Result<String, Error> {
    let mut spans: Vec<_> = edits
        .iter()
        .map(|edit| Some((offset(text, edit.range.start)?, offset(text, edit.range.end)?, edit.text.as_str())))
        .collect::<Option<Vec<_>>>()
        .ok_or(Error::Stale)?;
    spans.sort_by_key(|(start, end, _)| (*start, *end));
    if spans.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(Error::Stale);
    }
    let mut after = text.to_owned();
    for (start, end, replacement) in spans.into_iter().rev() {
        after.replace_range(start..end, replacement);
    }
    Ok(after)
}

fn resolve_lombok(pending: &Pending, doc: &Document) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let class = crate::owned_entity_source(doc).ok_or(Error::Stale)?;
    let toggles = crate::lombok_checks::toggle_items(&doc.text, &class).ok_or(Error::Stale)?;
    let items: Vec<_> = toggles.iter().map(|(name, present)| PickItem::new(name, &format!("{} @{name}", if *present { "Remove" } else { "Add" }))).collect();
    let picked = host::pick_many("Lombok annotations to add or remove", &items)?.ok_or(Error::Cancelled)?;
    if picked.is_empty() {
        return Err(Error::Cancelled);
    }
    let edits: Vec<TextEdit> = crate::lombok_checks::plan(&doc.text, &class, &picked)?.iter().map(|e| e.to_text_edit(&doc.text)).collect();
    let after = checked_after(doc, &edits)?;
    if !ui_kit::prompts::confirm_exact("Edit Lombok annotations?", &format!("Target: {}\nSource version: {}\n\n{after}\nlombok.config is not consulted.", doc.path, doc.version))? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    Ok(FileAction { title: crate::lombok_checks::TITLE.trim_end_matches('…').into(), kind: Some("refactor.rewrite".into()), edits: vec![(doc.path.clone(), edits)], ..FileAction::default() })
}

impl FileActions for JpaAssist {
    fn file_actions(&mut self, doc: &Document, selection: Range) -> Vec<FileAction> {
        if let Some(actions) = liquibase_inspector::actions(&mut self.generation, doc, selection) {
            return actions;
        }
        if let Some(actions) = schema_repair_action::actions(&mut self.generation, doc, selection) {
            return actions;
        }
        if let Some(actions) = datasource_action::actions(&mut self.generation, doc, selection) {
            return actions;
        }
        if let Some(mut actions) = ddl_entities_action::actions(&mut self.generation, doc, selection) {
            actions.extend(flyway_misc_action::sql_actions(&mut self.generation, doc).unwrap_or_default());
            return actions;
        }
        if doc.text.len() > MAX_TEXT
            || language_of(&doc.path).is_none()
            || !matches!(doc.language.as_str(), "java" | "kotlin")
            || (doc.language == "kotlin") != language_of(&doc.path).unwrap()
        {
            return Vec::new();
        }
        let Some(root) = host::project_of(&doc.path).and_then(|project| project.root().ok()).and_then(|root| path(&root)) else {
            return Vec::new();
        };
        if path(&doc.path).as_deref() != Some(doc.path.as_str()) || !inside(&root, &doc.path) || package(doc).is_none() {
            return Vec::new();
        }
        let mut operations = vec![(Operation::Entity, "Generate JPA entity…")];
        operations.push((Operation::PersistenceClass, "Generate JPA embeddable or mapped superclass…"));
        operations.push((Operation::ConfiguredEntity,"Generate JPA entity with identifier options…"));
        if identifier_action::source(doc).is_some() {
            operations.push((Operation::Identifier,"Add JPA identifier with preview…"));
            operations.push((Operation::EmbeddedIdentifier,embedded_id_action::TITLE));
        }
        if ddl_entities_action::attrs_eligible(doc) {
            operations.push((Operation::DdlEntityAttrs, ddl_entities_action::ATTRS_TITLE));
        }
        if entity_from_class::eligible(doc) {
            operations.push((Operation::EntityFromClass, entity_from_class::TITLE));
        }
        if let Some(class) = current_class(doc) {
            operations.push((Operation::Attribute, "Add JPA attribute with preview…"));
            if class.kind == Kind::Entity { operations.push((Operation::Inverse, "Add inverse attribute to this entity…")); }
            if related_entity_action::eligible(doc) {
                operations.push((Operation::RelatedEntityTo, related_entity_action::TITLE_TO));
                operations.push((Operation::RelatedEntityFrom, related_entity_action::TITLE_FROM));
            }
        }
        if dto_convert_action::eligible_record(doc) {
            operations.push((Operation::DtoToClass, dto_convert_action::TO_CLASS_TITLE));
        }
        if dto_convert_action::eligible_class(doc) && dto_action::record_offered(&doc.path) {
            operations.push((Operation::DtoToRecord, dto_convert_action::TO_RECORD_TITLE));
        }
        if extract_superclass::eligible(doc) {
            operations.push((Operation::ExtractSuperclass, extract_superclass::TITLE));
        }
        operations.extend(flyway_misc_action::operations(doc));
        if equality_action::eligible_equals(doc) {
            operations.push((Operation::EqualityMethods, equality_action::TITLE_EQUALS));
        }
        if equality_action::eligible_to_string(doc) {
            operations.push((Operation::LazySafeToString, equality_action::TITLE_TO_STRING));
        }
        if migration_action::eligible(doc) {
            operations.push((Operation::SchemaBaseline, migration_action::BASELINE_TITLE));
            operations.push((Operation::FlywayMigration, migration_action::MIGRATION_TITLE));
            operations.push((Operation::LiquibaseChangelog, liquibase_action::TITLE));
        }
        if envers_action::eligible_toggle(doc) {
            operations.push((Operation::EnversToggle, envers_action::TOGGLE_TITLE));
        }
        if envers_action::eligible_revision(doc) {
            operations.push((Operation::EnversRevision, envers_action::REVISION_TITLE));
        }
        if entity_graph_action::eligible(doc) {
            operations.push((Operation::EntityGraph, entity_graph_action::TITLE));
        }
        if repository_action::eligible(doc) {
            operations.push((Operation::SpringRepository, repository_action::TITLE));
            operations.push((Operation::SpringRepositories, repository_action::BATCH_TITLE));
        } else if revision_repo_action::eligible_view(doc) {
            operations.push((Operation::SpringRepository, repository_action::TITLE));
        }
        if revision_repo_action::eligible(doc) {
            operations.push((Operation::RevisionRepository, revision_repo_action::TITLE));
            operations.push((Operation::EnableEnversRepositories, revision_repo_action::ENABLE_TITLE));
        }
        if revision_repo_action::eligible_view(doc) {
            operations.push((Operation::EntityViewRepository, revision_repo_action::VIEW_TITLE));
        }
        if auditing_action::eligible_fields(doc) {
            operations.push((Operation::AuditingFields, auditing_action::FIELDS_TITLE));
        }
        if auditing_action::eligible_config(doc) {
            operations.push((Operation::AuditingConfig, auditing_action::CONFIG_TITLE));
        }
        if query_method_action::eligible(doc) {
            operations.push((Operation::QueryMethod, query_method_action::TITLE));
            operations.push((Operation::QueryMethodOptions, query_method_action::OPTIONS_TITLE));
        }
        if query_extract_action::eligible(doc) {
            operations.push((Operation::QueryExtract, query_extract_action::TITLE));
        }
        if query_edit_action::eligible(doc) {
            operations.push((Operation::QueryEdit, query_edit_action::TITLE));
        }
        if query_convert_action::eligible(doc) {
            operations.push((Operation::QueryAsync, query_convert_action::TITLE_ASYNC));
            operations.push((Operation::QueryDynamicProjection, query_convert_action::TITLE_DYNAMIC));
        }
        if validation_action::eligible(doc) {
            operations.push((Operation::ValidationConstraint, validation_action::TITLE));
        }
        if crate::owned_entity_source(doc).is_some_and(|class| crate::lombok_checks::eligible(&doc.text, &class)) {
            operations.push((Operation::LombokAnnotations, crate::lombok_checks::TITLE));
        }
        if dto_action::eligible(doc) {
            operations.push((Operation::Dto, "Create scalar DTO with preview…"));
        }
        if dto_action::class_eligible(doc) {
            operations.push((Operation::DtoClass, dto_action::CLASS_TITLE));
        }
        if projection_action::eligible(doc) {
            operations.push((Operation::SpringProjection, projection_action::TITLE));
            operations.push((Operation::SyncProjections, projection_links::SYNC_TITLE));
            operations.push((Operation::SyncDtos, projection_links::SYNC_DTO_TITLE));
        }
        if nested_dto_action::eligible(doc) {
            operations.push((Operation::NestedProjection, nested_dto_action::TITLE));
            if doc.language != "java" || dto_action::record_offered(&doc.path) {
                operations.push((Operation::FlatDto, nested_dto_action::FLAT_TITLE));
            }
        }
        if dto_assoc_action::eligible(doc) {
            operations.push((Operation::DtoAssociations, dto_assoc_action::TITLE));
        }
        if mapper_action::eligible(doc) {
            operations.push((Operation::MapstructMapper, mapper_action::TITLE));
        }
        if mapper_more_action::eligible(doc) {
            operations.push((Operation::AddMapping, mapper_more_action::ADD_TITLE));
            operations.push((Operation::BaseMapper, mapper_more_action::BASE_TITLE));
        }
        if mapper_more_action::eligible_extend(doc) {
            operations.push((Operation::ExtendMapper, mapper_more_action::EXTEND_TITLE));
        }
        if injection_action::eligible(doc) {
            operations.push((Operation::InjectMapper, mapper_more_action::INJECT_TITLE));
        }
        if modelmapper_action::eligible_service(doc) {
            operations.push((Operation::ModelMapperConfig, modelmapper_action::CONFIG_TITLE));
            operations.push((Operation::ModelMapperConversion, modelmapper_action::CONVERT_TITLE));
        }
        if modelmapper_action::eligible_view(doc) {
            operations.push((Operation::BlazeEntityView, modelmapper_action::VIEW_TITLE));
            operations.push((Operation::BlazeEntityViewOptions, entity_view_options::TITLE));
        }
        if converter_action::eligible(doc) {
            operations.push((Operation::AttributeConverter, converter_action::TITLE));
        }
        if hibernate_action::eligible_type(doc) {
            operations.push((Operation::HibernateUserType, hibernate_action::TITLE_TYPE));
        }
        if hibernate_action::eligible_json(doc) {
            operations.push((Operation::HibernateJson, hibernate_action::TITLE_JSON));
        }
        if hibernate_action::eligible_listener(doc) {
            operations.push((Operation::HibernateListener, hibernate_action::TITLE_LISTENER));
        }
        if ddl_action::eligible(doc) {
            operations.push((Operation::Ddl, ddl_action::TITLE));
        }
        if index_query_action::eligible(doc) {
            operations.push((Operation::IndexConstraint, index_query_action::INDEX_TITLE));
            operations.push((Operation::NamedQuery, index_query_action::QUERY_TITLE));
        }
        if naming_action::eligible(doc) {
            operations.push((Operation::NameConstants, naming_action::TITLE));
        }
        if injection_action::eligible(doc) {
            operations.push((Operation::InjectRepository, injection_action::TITLE));
        }
        if ddl_todo_action::eligible(doc, selection) {
            operations.push((Operation::TodoUncomment, ddl_todo_action::UNCOMMENT_TITLE));
            operations.push((Operation::TodoRemove, ddl_todo_action::REMOVE_TITLE));
        }
        if attribute_refactor::eligible(doc,selection) {operations.push((Operation::Rename,attribute_refactor::TITLE));}
        if attribute_refactor::eligible(doc,selection) && projection_action::eligible(doc) {operations.push((Operation::RenameWithProjections,projection_links::RENAME_TITLE));operations.push((Operation::RenameWithDtos,projection_links::RENAME_DTO_TITLE));}
        if attribute_repo_method::attribute_under(doc, selection).is_some_and(|(class, _)| host::project_of(&doc.path).is_some_and(|project| self.repositories_known(&project, &class.name) || attribute_repo_method::repository_nearby(&project, &doc.path, &class.name))) {
            operations.push((Operation::AttributeRepositoryMethod, attribute_repo_method::TITLE));
        }
        if call_assist::eligible(doc, selection) {
            operations.push((Operation::CreateRepositoryMethod, call_assist::CREATE_METHOD_TITLE));
        }
        for (created, title) in unresolved_create::offers(doc, selection) {
            operations.push((match created {
                unresolved_create::Created::Repository => Operation::UnresolvedRepository,
                unresolved_create::Created::Projection => Operation::UnresolvedProjection,
                unresolved_create::Created::Dto => Operation::UnresolvedDto,
                other => Operation::UnresolvedContext(other),
            }, title));
        }
        if self.generation.pending.len() > 8 {
            self.generation.pending.clear();
        }
        operations
            .into_iter()
            .filter_map(|(operation, title)| {
                self.generation.next = self.generation.next.checked_add(1)?;
                let scope=if matches!(operation,Operation::Rename) {format!(":{:016x}",attribute_refactor::stamp(&root,doc,selection))} else {String::new()};
                let data = format!("jpa:{}:{}:{}{}", operation.key(), self.generation.next, doc.version,scope);
                self.generation.pending.insert(data.clone(), Pending { root: root.clone(), doc: doc.clone(), operation, selection });
                Some(FileAction { title: title.into(), kind: Some("refactor.rewrite".into()), data: Some(data), ..FileAction::default() })
            })
            .collect()
    }
    fn resolve_file_action(&mut self, doc: &Document, action: FileAction) -> Result<FileAction, Error> {
        let pending = self.generation.pending.get(action.data.as_deref().ok_or(Error::Stale)?).cloned().ok_or(Error::Stale)?;
        if matches!(pending.operation,Operation::Rename) {self.generation.pending.remove(action.data.as_deref().ok_or(Error::Stale)?);}
        let project = current(&pending, doc)?;
        let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
        match pending.operation {
            Operation::Entity | Operation::PersistenceClass | Operation::ConfiguredEntity => {
                let items = [PickItem::new("jakarta", "Jakarta Persistence"), PickItem::new("javax", "Javax Persistence")];
                let Some(namespace) = host::pick("Persistence namespace for the new entity", &items)? else {
                    return Err(Error::Cancelled);
                };
                let generated = if matches!(pending.operation, Operation::PersistenceClass) {
                    let roles = [PickItem::new("embeddable", "Embeddable"), PickItem::new("mapped-superclass", "Mapped superclass")];
                    let Some(role) = host::pick("Persistence class role", &roles)? else { return Err(Error::Cancelled); };
                    let Some(name) = ask_identifier("Persistence class name")? else { return Err(Error::Cancelled); };
                    let name = capitalize(&name);
                    if !identifier(&name) { return Err("Choose a valid persistence class name.".into()); }
                    let role = match role.as_str() {
                        "embeddable" => persistence_core::PersistenceRole::Embeddable,
                        "mapped-superclass" => persistence_core::PersistenceRole::MappedSuperclass,
                        _ => return Err(Error::Stale),
                    };
                    let source = entity_source(&EntitySpec { role, name: name.clone(), table: String::new(), package: package(doc),
                        kotlin: doc.language == "kotlin", id: "none".into(), auditing: false, version: false, equals: false });
                    Some((name, if namespace == "javax" { source.replace("jakarta.persistence.", "javax.persistence.") } else { source }))
                } else if matches!(pending.operation,Operation::ConfiguredEntity) {
                    let Some(name)=ask_identifier("Entity class name")? else {return Err(Error::Cancelled);};
                    let name=capitalize(&name);
                    let Some(table)=host::input("Table name",Some(&naming_table_name(&name)))? else {return Err(Error::Cancelled);};
                    let ns=match namespace.as_str() {"jakarta"=>persistence_core::generation::Namespace::Jakarta,"javax"=>persistence_core::generation::Namespace::Javax,_=>return Err(Error::Stale)};
                    let identifier=identifier_action::prompt(ns,&name)?;
                    let versioned=host::pick_many("Additional mappings",&[PickItem::new("version","Version field")])?.ok_or(Error::Cancelled)?.iter().any(|s|s=="version");
                    let draft=persistence_core::generation::ManagedClassDraft{name:name.clone(),role:persistence_core::PersistenceRole::Entity,namespace:ns,table_name:table.trim().into(),identifier:Some(identifier),versioned,inheritance:None};
                    let source=crate::wizard::configured_class_source(&draft,&package(doc).ok_or(Error::Stale)?,doc.language=="kotlin").map_err(Error::from)?;
                    Some((name,source))
                } else { prompt_entity(Some(&doc.path), &namespace)? };
                let Some((name, source)) = generated else {
                    return Err(Error::Cancelled);
                };
                let declared = package(doc).ok_or(Error::Stale)?;
                let body = if source.starts_with("package ") { source.split_once('\n').ok_or(Error::Stale)?.1 } else { &source };
                let source =
                    format!("package {declared}{}\n\n{}", if doc.language == "kotlin" { "" } else { ";" }, body.trim_start_matches('\n'));
                let target = format!("{directory}/{name}.{}", if doc.language == "kotlin" { "kt" } else { "java" });
                if path(&target).is_none() || source.len() > MAX_TEXT {
                    return Err("Generated target exceeds source/path bounds.".into());
                }
                let generated = parse_class(&source, &target).ok_or(Error::Stale)?;
                if name == "Long" || generated.imports.iter().any(|import| import.rsplit('.').next() == Some(name.as_str())) {
                    return Err("Entity name conflicts with a generated identifier type or imported annotation/type.".into());
                }
                let same = target == doc.path && doc.text.trim().is_empty();
                no_collision(&project, &pending.root, directory, &name, same.then_some(doc.path.as_str()))?;
                if !ui_kit::prompts::confirm_exact(
                    if matches!(pending.operation, Operation::PersistenceClass) { "Create persistence class?" } else { "Create JPA entity?" },
                    &format!(
                        "Namespace: {namespace}.persistence\nTarget: {target}\nSource version: {}\n\n{source}\nOne host-managed undoable edit; no existing file is overwritten.",
                        doc.version
                    ),
                )? {
                    return Err(Error::Cancelled);
                }
                current(&pending, doc)?;
                no_collision(&project, &pending.root, directory, &name, same.then_some(doc.path.as_str()))?;
                Ok(FileAction {
                    title: if matches!(pending.operation, Operation::PersistenceClass) { format!("Create persistence class {name}") } else { format!("Create JPA entity {name}") },
                    kind: Some("refactor.rewrite".into()),
                    operations: if same {
                        Vec::new()
                    } else {
                        vec![FileOperation::Create { path: target.clone(), text: source.clone(), overwrite: false }]
                    },
                    edits: if same {
                        vec![(
                            doc.path.clone(),
                            vec![TextEdit { range: Range::new(Position::new(0, 0), position(&doc.text, doc.text.len())), text: source }],
                        )]
                    } else {
                        Vec::new()
                    },
                    open: Some(target),
                    ..FileAction::default()
                })
            }
            Operation::Attribute => {
                let Some(edits) = prompt_attribute(doc)? else {
                    return Err(Error::Cancelled);
                };
                let after = checked_after(doc, &edits)?;
                if !ui_kit::prompts::confirm_exact(
                    "Add JPA attribute?",
                    &format!("Target: {}\nSource version: {}\n\n{after}{}", doc.path, doc.version, association_advice::note(&doc.path, &doc.language, &doc.text, &after)),
                )? {
                    return Err(Error::Cancelled);
                }
                current(&pending, doc)?;
                Ok(FileAction {
                    title: action.title,
                    kind: Some("refactor.rewrite".into()),
                    edits: vec![(doc.path.clone(), edits)],
                    ..FileAction::default()
                })
            }
            Operation::Inverse => self.resolve_inverse(&pending, doc, &project),
            Operation::AttributeRepositoryMethod => attribute_repo_method::resolve(&pending, doc, &project),
            Operation::RelatedEntityTo => related_entity_action::resolve(&pending, doc, &project, related_entity_action::Direction::To),
            Operation::RelatedEntityFrom => related_entity_action::resolve(&pending, doc, &project, related_entity_action::Direction::From),
            Operation::ValidationConstraint => validation_action::resolve(&pending, doc),
            Operation::LombokAnnotations => resolve_lombok(&pending, doc),
            Operation::Dto => dto_action::resolve(&pending, doc, &project),
            Operation::DtoClass => dto_action::resolve_class(&pending, doc, &project),
            Operation::Ddl => ddl_action::resolve(&pending, doc, &project),
            Operation::Rename => attribute_refactor::resolve(&pending,doc),
            Operation::Identifier => identifier_action::resolve(&pending,doc),
            Operation::EmbeddedIdentifier => embedded_id_action::resolve(&pending,doc),
            Operation::EntityFromClass => entity_from_class::resolve(&pending, doc),
            Operation::ExtractSuperclass => extract_superclass::resolve(&pending, doc, &project),
            Operation::DtoToClass => dto_convert_action::resolve_to_class(&pending, doc, &project),
            Operation::DtoToRecord => dto_convert_action::resolve_to_record(&pending, doc, &project),
            Operation::Datasource => datasource_action::resolve(&pending, doc, &project),
            Operation::FlywayJavaMigration => flyway_misc_action::resolve_java_migration(&pending, doc, &project),
            Operation::FlywaySqlCallback => flyway_misc_action::resolve_sql_callback(&pending, doc, &project),
            Operation::FlywayJavaCallback => flyway_misc_action::resolve_java_callback(&pending, doc, &project),
            Operation::EqualityMethods => equality_action::resolve_equals(&pending, doc, &project),
            Operation::LazySafeToString => equality_action::resolve_to_string(&pending, doc),
            Operation::DdlEntities => ddl_entities_action::resolve(&pending, doc, &project),
            Operation::DdlEntityAttrs => ddl_entities_action::attrs_resolve(&pending, doc, &project),
            Operation::TodoUncomment => ddl_todo_action::resolve(&pending, doc, true),
            Operation::TodoRemove => ddl_todo_action::resolve(&pending, doc, false),
            Operation::SchemaRepair => schema_repair_action::resolve(&pending, doc, &project),
            Operation::SchemaBaseline => migration_action::resolve_baseline(&pending, doc, &project),
            Operation::FlywayMigration => migration_action::resolve_migration(&pending, doc, &project),
            Operation::LiquibaseChangelog => liquibase_action::resolve(&pending, doc, &project),
            Operation::LiquibaseInspector(item) => liquibase_inspector::resolve(&pending, doc, item),
            Operation::EnversToggle => envers_action::resolve_toggle(&pending, doc, &project),
            Operation::EnversRevision => envers_action::resolve_revision(&pending, doc, &project),
            Operation::EntityGraph => entity_graph_action::resolve(&pending, doc, &project),
            Operation::MapstructMapper => mapper_action::resolve(&pending, doc, &project),
            Operation::AddMapping => mapper_more_action::resolve_add(&pending, doc, &project),
            Operation::BaseMapper => mapper_more_action::resolve_base(&pending, doc, &project),
            Operation::ExtendMapper => mapper_more_action::resolve_extend(&pending, doc, &project),
            Operation::InjectMapper => mapper_more_action::resolve_inject(&pending, doc, &project),
            Operation::ModelMapperConfig => modelmapper_action::resolve_config(&pending, doc, &project),
            Operation::ModelMapperConversion => modelmapper_action::resolve_conversion(&pending, doc, &project),
            Operation::BlazeEntityView => modelmapper_action::resolve_view(&pending, doc, &project),
            Operation::BlazeEntityViewOptions => entity_view_options::resolve(&pending, doc, &project),
            Operation::DtoAssociations => dto_assoc_action::resolve(&pending, doc, &project),
            Operation::AttributeConverter => converter_action::resolve(&pending, doc, &project),
            Operation::HibernateUserType => hibernate_action::resolve_type_action(&pending, doc, &project),
            Operation::HibernateJson => hibernate_action::resolve_json_action(&pending, doc, &project),
            Operation::HibernateListener => hibernate_action::resolve_listener_action(&pending, doc, &project),
            Operation::SpringRepository => repository_action::resolve(&pending, doc, &project),
            Operation::QueryMethod => query_method_action::resolve(&pending, doc, &project),
            Operation::QueryMethodOptions => query_method_action::resolve_with_options(&pending, doc, &project),
            Operation::QueryExtract => query_extract_action::resolve(&pending, doc, &project),
            Operation::QueryEdit => query_edit_action::resolve(&pending, doc, &project),
            Operation::QueryAsync => query_convert_action::resolve_async(&pending, doc),
            Operation::QueryDynamicProjection => query_convert_action::resolve_dynamic(&pending, doc),
            Operation::SpringProjection => projection_action::resolve(&pending, doc, &project),
            Operation::SyncProjections => projection_links::resolve_sync(&pending, doc, &project),
            Operation::RenameWithProjections => projection_links::resolve_rename(&pending, doc, &project),
            Operation::SyncDtos => projection_links::resolve_sync_of(&pending, doc, &project, projection_links::Kind::Dto),
            Operation::RenameWithDtos => projection_links::resolve_rename_of(&pending, doc, &project, projection_links::Kind::Dto),
            Operation::UnresolvedRepository => unresolved_create::resolve(&pending, doc, &project, unresolved_create::Created::Repository),
            Operation::UnresolvedProjection => unresolved_create::resolve(&pending, doc, &project, unresolved_create::Created::Projection),
            Operation::UnresolvedDto => unresolved_create::resolve(&pending, doc, &project, unresolved_create::Created::Dto),
            Operation::UnresolvedContext(created) => unresolved_create::resolve(&pending, doc, &project, created),
            Operation::CreateRepositoryMethod => call_assist::resolve_create_method(&pending, doc, &project),
            Operation::NestedProjection => nested_dto_action::resolve(&pending, doc, &project),
            Operation::FlatDto => nested_dto_action::resolve_flat(&pending, doc, &project),
            Operation::IndexConstraint => index_query_action::resolve_index(&pending, doc),
            Operation::NamedQuery => index_query_action::resolve_query(&pending, doc),
            Operation::NameConstants => naming_action::resolve(&pending, doc, &project),
            Operation::InjectRepository => injection_action::resolve(&pending, doc, &project),
            Operation::AuditingFields => auditing_action::resolve_fields(&pending, doc, &project),
            Operation::AuditingConfig => auditing_action::resolve_config(&pending, doc, &project),
            Operation::SpringRepositories => self.resolve_repositories(&pending, doc, &project),
            Operation::RevisionRepository => revision_repo_action::resolve_revision(&pending, doc, &project),
            Operation::EnableEnversRepositories => revision_repo_action::resolve_enable(&pending, doc, &project),
            Operation::EntityViewRepository => revision_repo_action::resolve_view(&pending, doc, &project),
        }
    }
}
impl JpaAssist {
    fn resolve_inverse(&mut self, pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
        let target = current_class(doc).ok_or(Error::Stale)?;
        let ns = namespace(&doc.text, &target).ok_or(Error::Stale)?;
        let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
        let relative = directory.strip_prefix(&format!("{}/", pending.root.trim_end_matches('/'))).ok_or(Error::Stale)?;
        let mut candidates = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(2);
        for extension in ["java", "kt"] {
            let files = project.find_files(&format!("{relative}/*.{extension}"), 65)?;
            if files.len() >= 65 {
                return Err("Inverse owner discovery is incomplete in this folder.".into());
            }
            for file in files.into_iter().filter(|file| file != &doc.path) {
                if Instant::now() >= deadline {
                    return Err("Inverse owner discovery timed out.".into());
                }
                if path(&file).as_deref() != Some(file.as_str()) || !inside(&pending.root, &file) {
                    return Err(Error::Stale);
                }
                let text = project.read_file(&file)?;
                if text.len() > MAX_TEXT {
                    return Err("An inverse owner candidate exceeds the source bound.".into());
                }
                let owner_doc = Document {
                    path: file.clone(),
                    language: if extension == "kt" { "kotlin" } else { "java" }.into(),
                    text: text.clone(),
                    version: 0,
                };
                let Some(owner) = current_class(&owner_doc).filter(|owner| {
                    owner.name == file_stem(&file) && owner.package == target.package && namespace(&text, owner) == Some(ns)
                }) else {
                    continue;
                };
                let target_fq = format!("{}.{}", target.package.as_deref().ok_or(Error::Stale)?, target.name);
                let owner_fq = format!("{}.{}", owner.package.as_deref().ok_or(Error::Stale)?, owner.name);
                if owner.imports.iter().any(|import| {
                    (import.rsplit('.').next() == Some(target.name.as_str()) && import != &target_fq)
                        || (import.ends_with(".*") && import != &format!("{ns}.*") && import != "java.util.*")
                }) || target.imports.iter().any(|import| import.rsplit('.').next() == Some(owner.name.as_str()) && import != &owner_fq)
                {
                    continue;
                }
                for attribute in owner
                    .attributes
                    .iter()
                    .filter(|attribute| attribute.target == target.name && !attribute.ty.contains('.') && !attribute.mapped_by)
                {
                    let relation = format!("{:?}", attribute.relation);
                    if !annotation_is(&text, &owner, &relation, ns) {
                        continue;
                    }
                    if let Some(spec) = inverse_spec(&owner, attribute, &target).filter(|spec| identifier(&spec.name)) {
                        candidates.push((
                            file.clone(),
                            text.clone(),
                            format!("{}.{} → {}.{}", owner_fq, attribute.name, target_fq, spec.name),
                            spec,
                        ));
                    }
                }
            }
        }
        if candidates.is_empty() {
            return Err("No verified same-package, same-folder owning association can be mirrored into this entity.".into());
        }
        let items: Vec<_> =
            candidates.iter().enumerate().map(|(index, (_, _, label, _))| PickItem::new(&index.to_string(), label)).collect();
        let Some(index) = host::pick("Owning association to mirror into this entity", &items)? else {
            return Err(Error::Cancelled);
        };
        let (owner_path, owner_text, label, spec) =
            candidates.get(index.parse::<usize>().map_err(|_| Error::Stale)?).ok_or(Error::Stale)?;
        let edits = attribute_edits(&target, &doc.text, &mask(&doc.text), spec).ok_or(Error::Stale)?;
        let edits: Vec<_> = edits.iter().map(|edit| edit.to_text_edit(&doc.text)).collect();
        let after = checked_after(doc, &edits)?;
        if !ui_kit::prompts::confirm_exact(
            "Add inverse attribute to this entity?",
            &format!("{label}\nRead-only owner: {owner_path}\nEdit only: {} (version {})\n\n{after}{}", doc.path, doc.version, association_advice::note(&doc.path, &doc.language, &doc.text, &after)),
        )? {
            return Err(Error::Cancelled);
        }
        current(pending, doc)?;
        if project.read_file(owner_path)? != *owner_text
            || host::project_of(owner_path).and_then(|p| p.root().ok()).as_deref() != Some(pending.root.as_str())
        {
            return Err(Error::Stale);
        }
        Ok(FileAction {
            title: format!("Add inverse attribute {}", spec.name),
            kind: Some("refactor.rewrite".into()),
            edits: vec![(doc.path.clone(), edits)],
            ..FileAction::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const CUSTOMER: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Customer {\n    @Id Long id;\n}\n";
    const ORDER: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Order {\n    @Id Long id;\n    @ManyToOne\n    Customer customer;\n}\n";
    fn setup(path: &str, text: &str, language: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(path, language, text, &[]);
        let doc = Document { path: path.into(), language: language.into(), text: text.into(), version: 1 };
        (host, JpaAssist::default(), doc)
    }
    fn action(plugin: &mut JpaAssist, doc: &Document, title: &str) -> FileAction {
        let action = plugin.file_actions(doc, Range::default()).into_iter().find(|action| action.title.starts_with(title)).unwrap();
        assert!(action.operations.is_empty() && action.edits.is_empty());
        action
    }
    fn answers(host: &MockHost, rows: &[&[&str]]) {
        for row in rows {
            host.answer_next_prompt(Some(row));
        }
    }
    #[test]
    fn transient_source_names_are_not_reused_for_generated_attributes() {
        let source = CUSTOMER.replace("    @Id Long id;", "    @Id Long id;\n    @Transient Long cached;");
        let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Customer.java", &source, "java");
        let pending = action(&mut plugin, &doc, "Add JPA attribute");
        answers(&host, &[&["cached"], &["transient"], &["Long"]]);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert_eq!(host.editor_text(), source);
    }
    #[test]
    fn value_collections_embedded_values_and_transient_fields_have_distinct_mappings() {
        for (name, rows, expected, forbidden) in [
            ("address", vec![vec!["address"],vec!["Embedded"],vec!["Address"],vec![]], "@Embedded", "cascade"),
            ("labels", vec![vec!["labels"],vec!["ElementCollection"],vec!["String"],vec![],vec![]], "List<String> labels = new ArrayList<>()", "cascade"),
            ("cached", vec![vec!["cached"],vec!["transient"],vec!["Long"],vec![]], "@Transient", "@Column"),
        ] {
            let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Customer.java", CUSTOMER, "java");
            let pending = action(&mut plugin, &doc, "Add JPA attribute");
            for row in rows { host.answer_next_prompt(Some(&row)); }
            let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
            let after = patched(&doc.text, &resolved.edits[0].1).unwrap();
            assert!(after.contains(expected), "{name}: {after}");
            assert!(!after.contains(forbidden), "{name}: {after}");
            assert_eq!(host.editor_text(), CUSTOMER);
        }
    }
    #[test]
    fn embeddables_and_mapped_superclasses_allow_attributes_but_not_inverse_generation() {
        for role in ["Embeddable", "MappedSuperclass"] {
            let source = format!("package shop;\nimport jakarta.persistence.*;\n@{role}\nclass Address {{}}\n");
            let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Address.java", &source, "java");
            assert!(plugin.file_actions(&doc, Range::default()).iter().all(|a| !a.title.starts_with("Add inverse")));
            let pending = action(&mut plugin, &doc, "Add JPA attribute");
            answers(&host, &[&["city"], &["none"], &["String"], &["notNull"], &[]]);
            let result = plugin.resolve_file_action(&doc, pending).unwrap();
            let after = patched(&doc.text, &result.edits[0].1).unwrap();
            assert!(after.contains("@Column(nullable = false)") && after.contains("String city"), "{after}");
            assert!(!after.contains("@Entity") && !after.contains("@Id"), "{after}");
        }
    }
    #[test]
    fn kotlin_value_collections_use_kotlin_scalar_element_names() {
        let source = "package shop\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\n@Entity\nclass Customer { @Id var id: Long? = null }\n";
        let (host, mut plugin, doc) = setup("/project/src/main/kotlin/shop/Customer.kt", source, "kotlin");
        let pending = action(&mut plugin, &doc, "Add JPA attribute");
        answers(&host, &[&["values"], &["ElementCollection"], &["Integer"], &[], &[]]);
        let result = plugin.resolve_file_action(&doc, pending).unwrap();
        let after = patched(&doc.text, &result.edits[0].1).unwrap();
        assert!(after.contains("MutableList<Int>") && after.contains("@ElementCollection"), "{after}");
        assert!(!after.contains("MutableList<Integer>"));
    }
    #[test]
    fn persistence_roles_create_java_and_kotlin_without_table_or_identifier() {
        for (language, extension, role, expected) in [("java", "java", "embeddable", "@Embeddable"), ("kotlin", "kt", "mapped-superclass", "@MappedSuperclass")] {
            let path = format!("/project/src/main/{language}/shop/Address.{extension}");
            let (host, mut plugin, doc) = setup(&path, "", language);
            let pending = action(&mut plugin, &doc, "Generate JPA embeddable");
            answers(&host, &[&["jakarta"], &[role], &["Address"], &[]]);
            let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
            assert!(resolved.operations.is_empty());
            let source = &resolved.edits[0].1[0].text;
            assert!(source.contains(expected), "{source}");
            assert!(!source.contains("@Entity") && !source.contains("@Table") && !source.contains("@Id") && !source.contains("var id") && !source.contains("Long id"), "{source}");
            assert_eq!(host.editor_text(), "");
        }
    }
    #[test]
    fn persistence_class_creation_cancellation_and_existing_targets_refuse() {
        let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Customer.java", CUSTOMER, "java");
        let pending = action(&mut plugin, &doc, "Generate JPA embeddable");
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)));
        host.add_file("src/main/java/shop/Address.java", "class Address {}");
        let pending = action(&mut plugin, &doc, "Generate JPA embeddable");
        answers(&host, &[&["javax"], &["embeddable"], &["Address"]]);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert_eq!(host.editor_text(), CUSTOMER);
    }
    #[test]
    fn configured_entity_sibling_has_custom_identifier_and_non_overwriting_creation() {
        let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Customer.java", CUSTOMER, "java");
        let pending = action(&mut plugin, &doc, "Generate JPA entity with identifier options");
        answers(&host, &[&["javax"], &["Invoice"], &["invoices"], &["key"], &["invoice_id"], &["integer"], &["sequence"], &["invoiceIds"], &["invoice_seq"], &["5"], &["10"], &["version"], &[]]);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let [FileOperation::Create { path, text, overwrite }] = resolved.operations.as_slice() else { panic!("single sibling create") };
        assert!(!overwrite);
        assert_eq!(path, "/project/src/main/java/shop/Invoice.java");
        assert!(text.contains("@javax.persistence.SequenceGenerator") && text.contains("sequenceName = \"invoice_seq\"") && text.contains("initialValue = 5, allocationSize = 10"), "{text}");
        assert!(text.contains("java.lang.Integer key") && text.contains("@javax.persistence.Version"), "{text}");
        assert!(resolved.edits.is_empty());
        assert_eq!(host.editor_text(), CUSTOMER);
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn empty_java_entity_is_previewed_host_edit_and_command_never_writes() {
        let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Tag.java", "", "java");
        let action = action(&mut plugin, &doc, "Generate JPA");
        answers(&host, &[&["jakarta"], &["Tag"], &["tags"], &["IDENTITY"], &["version"], &[]]);
        let resolved = plugin.resolve_file_action(&doc, action).unwrap();
        assert!(resolved.operations.is_empty());
        assert_eq!(resolved.edits.len(), 1);
        assert_eq!(resolved.edits[0].0, doc.path);
        let text = &resolved.edits[0].1[0].text;
        assert!(text.contains("package shop;") && text.contains("private Long version;"));
        assert_eq!(host.editor_text(), "");
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn sibling_entity_creation_is_non_overwriting_and_preserves_selected_namespace_and_package() {
        let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Customer.java", CUSTOMER, "java");
        let action = action(&mut plugin, &doc, "Generate JPA");
        answers(&host, &[&["javax"], &["Tag"], &["tags"], &["IDENTITY"], &[], &[]]);
        let resolved = plugin.resolve_file_action(&doc, action).unwrap();
        let [FileOperation::Create { path, text, overwrite }] = resolved.operations.as_slice() else { panic!("single create") };
        assert!(!overwrite);
        assert_eq!(path, "/project/src/main/java/shop/Tag.java");
        assert!(text.contains("package shop;") && text.contains("import javax.persistence.Entity;"));
        assert!(!text.contains("jakarta.persistence"));
        assert!(resolved.edits.is_empty());
        assert_eq!(host.editor_text(), CUSTOMER);
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }
    #[test]
    fn kotlin_generation_and_attribute_edit_reuse_wizard_sources_without_applying() {
        let (host, mut plugin, doc) = setup("/project/src/main/kotlin/shop/Tag.kt", "\n", "kotlin");
        let pending = action(&mut plugin, &doc, "Generate JPA");
        answers(&host, &[&["jakarta"], &["tag"], &["tags"], &["UUID"], &[], &[]]);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        assert!(resolved.edits[0].1[0].text.contains("var id: UUID? = null"));
        assert_eq!(host.editor_text(), "\n");
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", CUSTOMER, "java");
        let pending = action(&mut plugin, &doc, "Add JPA attribute");
        answers(&host, &[&["total"], &["none"], &["BigDecimal"], &["notNull"], &[]]);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let text = patched(&doc.text, &resolved.edits[0].1).unwrap();
        assert!(text.contains("import java.math.BigDecimal;") && text.contains("private BigDecimal total;"));
        assert_eq!(resolved.edits[0].0, doc.path);
        assert_eq!(host.editor_text(), CUSTOMER);
    }
    #[test]
    fn inverse_edits_only_invoking_target_and_keeps_owner_and_dirty_target_untouched() {
        let text = format!("{CUSTOMER}//unsaved 🦀\n");
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", &text, "java");
        host.add_file("src/shop/Order.java", ORDER);
        let pending = action(&mut plugin, &doc, "Add inverse");
        answers(&host, &[&["0"], &[]]);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        assert!(resolved.operations.is_empty());
        assert_eq!(resolved.edits.len(), 1);
        assert_eq!(resolved.edits[0].0, doc.path);
        let after = patched(&doc.text, &resolved.edits[0].1).unwrap();
        assert!(after.contains("@OneToMany(mappedBy = \"customer\")") && after.contains("List<Order> orders"));
        assert!(after.ends_with("//unsaved 🦀\n"));
        assert_eq!(host.editor_text(), text);
        assert_eq!(host.invocation().project().unwrap().read_file("src/shop/Order.java").unwrap(), ORDER);
    }
    #[test]
    fn inverse_one_to_one_many_to_many_and_kotlin_targets_share_verified_mapping_facts() {
        for (relation, expected) in
            [("OneToOne", "@OneToOne(mappedBy = \"customer\")"), ("ManyToMany", "@ManyToMany(mappedBy = \"customer\")")]
        {
            let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", CUSTOMER, "java");
            let owner = ORDER
                .replace("ManyToOne", relation)
                .replace("Customer customer", if relation == "ManyToMany" { "List<Customer> customer" } else { "Customer customer" });
            host.add_file("src/shop/Order.java", &owner);
            let pending = action(&mut plugin, &doc, "Add inverse");
            answers(&host, &[&["0"], &[]]);
            let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
            assert!(patched(&doc.text, &resolved.edits[0].1).unwrap().contains(expected));
        }
        let text = "package shop\nimport jakarta.persistence.*\n@Entity\nclass Customer {\n @Id var id: Long? = null\n}\n";
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.kt", text, "kotlin");
        host.add_file("src/shop/Order.java", ORDER);
        let pending = action(&mut plugin, &doc, "Add inverse");
        answers(&host, &[&["0"], &[]]);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        assert!(patched(&doc.text, &resolved.edits[0].1).unwrap().contains("MutableList<Order>"));
    }
    #[test]
    fn inverse_refuses_foreign_namespaces_imported_other_owners_and_existing_properties() {
        for owner in [
            ORDER.replace("jakarta.persistence", "javax.persistence"),
            ORDER.replace("import jakarta.persistence.*;", "import other.Customer;\nimport jakarta.persistence.*;"),
            ORDER.replace("@ManyToOne", "@other.ManyToOne"),
            ORDER.replace("package shop", "package other"),
        ] {
            let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", CUSTOMER, "java");
            host.add_file("src/shop/Order.java", &owner);
            let pending = action(&mut plugin, &doc, "Add inverse");
            assert!(plugin.resolve_file_action(&doc, pending).is_err());
        }
        let text = CUSTOMER.replace("@Id Long id;", "@Id Long id;\n String orders;");
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", &text, "java");
        host.add_file("src/shop/Order.java", ORDER);
        let pending = action(&mut plugin, &doc, "Add inverse");
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
    }
    #[test]
    fn entity_collisions_cancellations_and_invalid_table_names_create_nothing() {
        let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Customer.java", CUSTOMER, "java");
        let pending = action(&mut plugin, &doc, "Generate JPA");
        assert!(matches!(plugin.resolve_file_action(&doc, pending.clone()), Err(Error::Cancelled)));
        host.add_file("src/main/java/shop/Tag.kt", "class Tag");
        answers(&host, &[&["jakarta"], &["Tag"], &["tags"], &["none"], &[]]);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert_eq!(host.editor_text(), CUSTOMER);
        let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Customer.java", CUSTOMER, "java");
        let pending = action(&mut plugin, &doc, "Generate JPA");
        answers(&host, &[&["jakarta"], &["Tag"], &["x\" evil"], &["none"], &[]]);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn action_versions_text_roots_paths_and_expired_instances_are_checked() {
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", CUSTOMER, "java");
        let pending = action(&mut plugin, &doc, "Add inverse");
        for changed in [
            Document { version: 2, ..doc.clone() },
            Document { text: format!("{CUSTOMER}//new"), ..doc.clone() },
            Document { path: "/project-other/Customer.java".into(), ..doc.clone() },
        ] {
            assert!(matches!(plugin.resolve_file_action(&changed, pending.clone()), Err(Error::Stale)));
        }
        assert!(matches!(JpaAssist::default().resolve_file_action(&doc, pending.clone()), Err(Error::Stale)));
        host.type_in_editor("//newer");
        assert!(matches!(plugin.resolve_file_action(&doc, pending.clone()), Err(Error::Stale)));
        host.set_project_root(Some("/other"));
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Stale)));
    }

    #[test]
    fn filling_empty_source_still_checks_other_language_and_generated_type_name_collisions() {
        let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Tag.java", "", "java");
        host.add_file("src/main/java/shop/Tag.kt", "package shop; class Tag");
        let pending = action(&mut plugin, &doc, "Generate JPA");
        answers(&host, &[&["jakarta"], &["Tag"], &["tags"], &["none"], &[]]);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert_eq!(host.editor_text(), "");
        for name in ["Entity", "Long"] {
            let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Tag.java", "", "java");
            let pending = action(&mut plugin, &doc, "Generate JPA");
            answers(&host, &[&["jakarta"], &[name], &["tags"], &["none"], &[]]);
            assert!(plugin.resolve_file_action(&doc, pending).is_err());
        }
    }
    #[test]
    fn inverse_cancellation_and_discovery_limits_leave_both_sides_untouched() {
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", CUSTOMER, "java");
        host.add_file("src/shop/Order.java", ORDER);
        let pending = action(&mut plugin, &doc, "Add inverse");
        assert!(matches!(plugin.resolve_file_action(&doc, pending.clone()), Err(Error::Cancelled)));
        answers(&host, &[&["0"]]);
        assert!(matches!(plugin.resolve_file_action(&doc, pending.clone()), Err(Error::Cancelled)));
        for index in 0..65 {
            host.add_file(&format!("src/shop/C{index}.java"), "class Unrelated {}");
        }
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert_eq!(host.editor_text(), CUSTOMER);
        assert_eq!(host.invocation().project().unwrap().read_file("src/shop/Order.java").unwrap(), ORDER);
    }
    #[test]
    fn ambiguous_annotation_identity_and_import_collision_cannot_generate_mapping_edits() {
        let text = CUSTOMER.replace("import jakarta.persistence.*;", "import other.Entity;\nimport jakarta.persistence.*;");
        let (_host, mut plugin, doc) = setup("/project/src/shop/Customer.java", &text, "java");
        let actions = plugin.file_actions(&doc, Range::default());
        assert!(actions.iter().all(|action| action.title.starts_with("Generate JPA")));
        let text = CUSTOMER.replace("import jakarta.persistence.*;", "import jakarta.persistence.*;\nimport other.Column;");
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", &text, "java");
        let pending = action(&mut plugin, &doc, "Add JPA attribute");
        answers(&host, &[&["total"], &["none"], &["String"], &["notNull"]]);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert_eq!(host.editor_text(), text);
    }
}
