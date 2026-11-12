//! Creation fixes that read their context instead of the reference's name (children of `unresolved_create`, Java only):
//! - an unresolved attribute type in an `@Entity` class: create the entity (and annotate the attribute with a relationship);
//! - an unresolved type in the `extends` clause of an entity or mapped superclass: create a `@MappedSuperclass`;
//! - an unresolved DTO as the (element) return type of a repository method, as the `Dto.class` target of
//!   `modelMapper.map(source, Dto.class)` (ModelMapper provably in the build files) or as the return or parameter type
//!   of a MapStruct mapper method (MapStruct provably in the build files): create a DTO from the entity the context names.
//! A reference that may be resolved by a wildcard import outside well-known persistence/JDK packages stays silent.
use super::*;
use crate::file_actions::{modelmapper_action, call_assist};
use crate::model::{annotations, closing, split_top_level};
use regex_lite::Regex;

pub(super) const ENTITY_TITLE: &str = "Create missing entity…";
pub(super) const SUPERCLASS_TITLE: &str = "Create missing mapped superclass…";
pub(super) const CONTEXT_DTO_TITLE: &str = "Create missing DTO from this usage…";

const JAVA_LANG: [&str; 28] = [
    "Object", "String", "Integer", "Long", "Short", "Byte", "Boolean", "Double", "Float", "Character", "Number", "Void", "Iterable", "Comparable",
    "CharSequence", "Class", "Enum", "Record", "Exception", "RuntimeException", "Throwable", "Runnable", "Override", "Deprecated", "Math", "System",
    "Thread", "StringBuilder",
];
const KNOWN_WILDCARDS: [&str; 8] =
    ["jakarta.persistence", "javax.persistence", "java.util", "java.time", "java.io", "java.math", "lombok", "jakarta.validation.constraints"];
const RELATIONSHIPS: [&str; 5] = ["ManyToOne", "OneToOne", "OneToMany", "ManyToMany", "Embedded"];

enum Site {
    Entity { collection: bool },
    Superclass,
    Repository(String),
    ModelMapper(String),
    Mapper(Vec<String>),
}

fn span(doc: &Document, selection: Range) -> Option<(usize, usize)> {
    let at = offset(&doc.text, selection.start)?;
    let bytes = doc.text.as_bytes();
    let start = at - bytes[..at].iter().rev().take_while(|&&byte| is_ident(byte)).count();
    let end = at + bytes[at..].iter().take_while(|&&byte| is_ident(byte)).count();
    let word = doc.text.get(start..end)?;
    (word.chars().next().is_some_and(|ch| ch.is_ascii_uppercase()) && identifier(word)).then_some((start, end))
}

fn without_annotations_blanked(masked: &str) -> String {
    let mut out = masked.to_owned();
    let Ok(re) = Regex::new(r"@[\w.]+(?:\s*\([^)]*\))?") else {
        return out;
    };
    for found in re.find_iter(masked) {
        out.replace_range(found.start()..found.end(), &" ".repeat(found.len()));
    }
    out
}

fn wildcard_may_resolve(doc: &Document) -> bool {
    mask(&doc.text).lines().filter_map(|line| line.trim().strip_prefix("import ")).map(|import| import.trim_end_matches(';').trim()).any(|import| {
        import.strip_suffix(".*").is_some_and(|package| !KNOWN_WILDCARDS.contains(&package))
    })
}

fn unresolved(doc: &Document, project: &Project, root: &str, name: &str) -> bool {
    !declared_or_imported(doc, name) && !JAVA_LANG.contains(&name) && !wildcard_may_resolve(doc) && type_exists(project, root, name) == Ok(false)
}

fn repository_return(masked: &str, repository: &queries::Repository, start: usize, end: usize) -> Option<()> {
    let method = repository.methods.iter().filter(|method| method.start >= end).min_by_key(|method| method.start)?;
    let between = &masked[end..method.start];
    let declaration = masked[..start].rfind([';', '{', '}']).map_or(0, |at| at + 1);
    let head = without_annotations_blanked(&masked[declaration..start]);
    let shape = Regex::new(r"^(?:(?:public|default|abstract|static|final)\s+)*(?:[\w.]+\s*<\s*)?$").ok()?;
    let head = head.trim_start();
    if !shape.is_match(head) {
        return None;
    }
    let wrapped = head.trim_end().ends_with('<');
    let closed = if wrapped { between.trim() == ">" } else { between.trim().is_empty() };
    closed.then_some(())
}

fn model_mapper_target(text: &str, masked: &str, start: usize, end: usize) -> Option<String> {
    let comma = masked[..start].trim_end().strip_suffix(',')?.len();
    let bytes = masked.as_bytes();
    let mut depth = 0;
    let mut open = None;
    for index in (0..comma).rev() {
        match bytes[index] {
            b')' => depth += 1,
            b'(' if depth == 0 => {
                open = Some(index);
                break;
            }
            b'(' => depth -= 1,
            b';' | b'{' | b'}' => return None,
            _ => {}
        }
    }
    let open = open?;
    let argument = masked[open + 1..comma].trim();
    if split_top_level(argument).len() != 1 || !identifier(argument) {
        return None;
    }
    if !masked[end..].trim_start().strip_prefix(".class")?.trim_start().starts_with(')') {
        return None;
    }
    let caller = Regex::new(r"(\w+)\s*\.\s*map\s*$").ok()?;
    let receiver = caller.captures(&masked[..open])?.get(1)?.as_str().to_owned();
    let (mapper, false) = call_assist::declared_type(text, &receiver, false)? else {
        return None;
    };
    if mapper != "ModelMapper" {
        return None;
    }
    let (ty, false) = call_assist::declared_type(text, argument, false)? else {
        return None;
    };
    Some(ty)
}

fn simple_type(ty: &str) -> Option<String> {
    let ty = ty.trim();
    (!ty.contains(['<', '[', '?']) && ty != "void").then(|| ty.rsplit('.').next().unwrap_or(ty).to_owned()).filter(|name| identifier(name))
}

fn mapper_site(text: &str, masked: &str, start: usize, end: usize) -> Option<Vec<String>> {
    if !annotations(masked).iter().any(|annotation| annotation.name == "Mapper") || !text.contains("org.mapstruct") {
        return None;
    }
    let statement = masked[..start].rfind([';', '{', '}']).map_or(0, |at| at + 1);
    let statement_end = masked[end..].find([';', '{']).map(|at| end + at)?;
    let raw = &masked[statement..statement_end];
    let marker = Regex::new(r"@MappingTarget\b").ok()?;
    let targets: Vec<usize> = marker.find_iter(raw).map(|found| found.start()).collect();
    let cleaned = without_annotations_blanked(raw);
    let paren = cleaned.find('(')?;
    let close = closing(cleaned.as_bytes(), paren)?;
    let relative = start - statement;
    let modifiers = ["public", "abstract", "default", "static"];
    let head: Vec<&str> = cleaned[..paren].split_whitespace().filter(|token| !modifiers.contains(token)).collect();
    let (_, returned) = head.split_last()?;
    let returned = returned.join("");
    let parameters: Vec<(usize, &str)> = split_top_level(&cleaned[paren + 1..close]);
    let parameter_type = |part: &str| -> Option<String> {
        let tokens: Vec<&str> = part.split_whitespace().collect();
        let (_, rest) = tokens.split_last()?;
        Some(rest.join(""))
    };
    let mut candidates = Vec::new();
    if relative < paren {
        if returned != text[start..end] {
            return None;
        }
        candidates.extend(parameters.first().and_then(|(_, part)| parameter_type(part)).and_then(|ty| simple_type(&ty)));
    } else {
        let (offset, part) = parameters.iter().find(|(offset, part)| {
            let from = paren + 1 + offset;
            relative >= from && relative < from + part.len()
        })?;
        if parameter_type(part)? != text[start..end] {
            return None;
        }
        if parameters.first().is_some_and(|(first, _)| first == offset) {
            candidates.extend(simple_type(&returned));
        }
        let marked = parameters.iter().find(|(offset, part)| targets.iter().any(|target| *target >= paren + 1 + offset && *target < paren + 1 + offset + part.len()));
        candidates.extend(marked.and_then(|(_, part)| parameter_type(part)).and_then(|ty| simple_type(&ty)));
    }
    (!candidates.is_empty()).then_some(candidates)
}

fn persistence_site(doc: &Document, masked: &str, start: usize, end: usize) -> Option<Site> {
    let class = parse_class(&doc.text, &doc.path)?;
    let at = words(masked, "class").next()?;
    let body = masked[at..].find('{').map(|found| at + found)?;
    let before = &masked[..start];
    let after = masked[end..].trim_start();
    let extends = Regex::new(r"\bclass\s+\w+\s+extends\s*$").ok()?;
    if extends.is_match(before) && (after.starts_with('{') || after.starts_with("implements")) {
        return matches!(class.kind, Kind::Entity | Kind::MappedSuperclass).then_some(Site::Superclass);
    }
    if class.kind != Kind::Entity || start < body {
        return None;
    }
    let declaration = before.rfind([';', '{', '}']).map_or(0, |found| found + 1);
    let head = without_annotations_blanked(&masked[declaration..start]);
    let shape = Regex::new(r"^(?:(?:private|protected|public|final|transient)\s+)*(?:[\w.]+\s*<\s*)?$").ok()?;
    let head = head.trim_start();
    let tail = Regex::new(r"^(?:>\s*)?\w+\s*(?:;|=)").ok()?;
    (shape.is_match(head) && tail.is_match(after)).then(|| Site::Entity { collection: head.trim_end().ends_with('<') })
}

fn site(doc: &Document, masked: &str, start: usize, end: usize) -> Option<Site> {
    if let Some(repository) = queries::parse_repository(&doc.text, false) {
        return repository_return(masked, &repository, start, end).map(|_| Site::Repository(repository.entity.clone()));
    }
    if masked[end..].trim_start().starts_with(".class") {
        return model_mapper_target(&doc.text, masked, start, end).map(Site::ModelMapper);
    }
    if let Some(candidates) = mapper_site(&doc.text, masked, start, end) {
        return Some(Site::Mapper(candidates));
    }
    persistence_site(doc, masked, start, end)
}

fn entity_of_site(site: &Site, doc: &Document, project: &Project, root: &str) -> Result<String, Error> {
    match site {
        Site::Repository(entity) => entity_document(project, root, entity, false).map(|_| entity.clone()),
        Site::ModelMapper(ty) => {
            modelmapper_action::build_proof(project, root, doc, "org.modelmapper", &["modelmapper"], "ModelMapper")?;
            entity_document(project, root, ty, false).map(|_| ty.clone())
        }
        Site::Mapper(candidates) => {
            modelmapper_action::build_proof(project, root, doc, "org.mapstruct", &["mapstruct"], "MapStruct")?;
            let mut last = Error::from("No entity could be derived from this mapper method.");
            for candidate in candidates {
                match entity_document(project, root, candidate, false) {
                    Ok(_) => return Ok(candidate.clone()),
                    Err(error) => last = error,
                }
            }
            Err(last)
        }
        Site::Entity { .. } | Site::Superclass => Err(Error::Stale),
    }
}

pub(super) fn offers(doc: &Document, selection: Range) -> Vec<(Created, &'static str)> {
    if doc.language != "java" || doc.text.len() > MAX_TEXT {
        return Vec::new();
    }
    let Some((start, end)) = span(doc, selection) else {
        return Vec::new();
    };
    let masked = mask(&doc.text);
    if masked.as_bytes()[..start].last() == Some(&b'.') {
        return Vec::new();
    }
    let Some(site) = site(doc, &masked, start, end) else {
        return Vec::new();
    };
    let Some(project) = host::project_of(&doc.path) else {
        return Vec::new();
    };
    let Ok(root) = project.root() else {
        return Vec::new();
    };
    if !unresolved(doc, &project, &root, &doc.text[start..end]) {
        return Vec::new();
    }
    match site {
        Site::Entity { .. } => vec![(Created::Entity, ENTITY_TITLE)],
        Site::Superclass => vec![(Created::MappedSuperclass, SUPERCLASS_TITLE)],
        other => entity_of_site(&other, doc, &project, &root).map(|_| vec![(Created::ContextDto, CONTEXT_DTO_TITLE)]).unwrap_or_default(),
    }
}

fn persistence_namespace(text: &str) -> Result<&'static str, Error> {
    match (text.contains("jakarta.persistence"), text.contains("javax.persistence")) {
        (true, false) => Ok("jakarta"),
        (false, true) => Ok("javax"),
        _ => Err("The persistence namespace (jakarta or javax) of this file is not unambiguous; nothing is created.".into()),
    }
}

fn relationship_edits(doc: &Document, start: usize, collection: bool, namespace: &str) -> Result<Vec<TextEdit>, Error> {
    let masked = mask(&doc.text);
    let declaration = masked[..start].rfind([';', '{', '}']).map_or(0, |found| found + 1);
    if RELATIONSHIPS.iter().any(|name| words(&masked[declaration..start], name).next().is_some()) {
        return Ok(Vec::new());
    }
    let options: [(&str, &str); 3] = if collection {
        [("OneToMany", "One to many"), ("ManyToMany", "Many to many"), ("none", "No relationship annotation")]
    } else {
        [("ManyToOne", "Many to one"), ("OneToOne", "One to one"), ("none", "No relationship annotation")]
    };
    let items: Vec<PickItem> = options.iter().map(|(key, label)| PickItem::new(key, label)).collect();
    let chosen = host::pick("Relationship of this attribute to the new entity", &items)?.ok_or(Error::Cancelled)?;
    if chosen == "none" {
        return Ok(Vec::new());
    }
    if !options.iter().any(|(key, _)| *key == chosen) {
        return Err(Error::Stale);
    }
    let line = doc.text[..start].rfind('\n').map_or(0, |found| found + 1);
    let indent: String = doc.text[line..].chars().take_while(|ch| *ch == ' ' || *ch == '\t').collect();
    let newline = if doc.text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut edits = vec![crate::model::Edit::insert(line, &format!("{indent}@{chosen}{newline}"))];
    let wildcard = doc.text.contains(&format!("import {namespace}.persistence.*;"));
    if !wildcard {
        edits.extend(import_edit(&doc.text, &[format!("{namespace}.persistence.{chosen}").as_str()], false));
    }
    Ok(edits.iter().map(|edit| edit.to_text_edit(&doc.text)).collect())
}

fn id_strategy(namespace: &str) -> Result<String, Error> {
    let all = [("IDENTITY", "IDENTITY"), ("SEQUENCE", "SEQUENCE"), ("AUTO", "AUTO"), ("UUID", "UUID"), ("none", "Assigned")];
    let items: Vec<PickItem> = all.iter().filter(|(key, _)| namespace != "javax" || *key != "UUID").map(|(key, label)| PickItem::new(key, label)).collect();
    let chosen = host::pick("ID strategy of the new entity", &items)?.ok_or(Error::Cancelled)?;
    if !all.iter().any(|(key, _)| *key == chosen) || (namespace == "javax" && chosen == "UUID") {
        return Err(Error::Stale);
    }
    Ok(chosen)
}

fn resolve_persistence(pending: &Pending, doc: &Document, project: &Project, created: Created) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let (start, end) = span(doc, pending.selection).ok_or(Error::Stale)?;
    let masked = mask(&doc.text);
    let found = site(doc, &masked, start, end).ok_or(Error::Stale)?;
    let name = doc.text[start..end].to_owned();
    if type_exists(project, &pending.root, &name)? {
        return Err(format!("A type named `{name}` already exists in this project; nothing is created.").into());
    }
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0.to_owned();
    let declared = package(doc).ok_or(Error::Stale)?;
    let namespace = persistence_namespace(&doc.text)?;
    let (role, collection) = match (created, found) {
        (Created::Entity, Site::Entity { collection }) => (persistence_core::PersistenceRole::Entity, Some(collection)),
        (Created::MappedSuperclass, Site::Superclass) => (persistence_core::PersistenceRole::MappedSuperclass, None),
        _ => return Err(Error::Stale),
    };
    let edits = match collection {
        Some(collection) => relationship_edits(doc, start, collection, namespace)?,
        None => Vec::new(),
    };
    let (table, id) = if collection.is_some() {
        let id = id_strategy(namespace)?;
        let table = host::input("Table name", Some(&naming_table_name(&name)))?.ok_or(Error::Cancelled)?;
        let table = table.trim().to_owned();
        if table.is_empty() || table.len() > 128 || !table.split('.').all(identifier) {
            return Err("Use a literal table identifier (optionally schema-qualified), without quotes or control characters.".into());
        }
        (table, id)
    } else {
        (String::new(), "none".to_owned())
    };
    let spec = EntitySpec { role, name: name.clone(), table, package: Some(declared), kotlin: false, id, auditing: false, version: false, equals: false };
    let source = crate::wizard::entity_source_annotated(&spec, &crate::lombok_entity_annotations());
    let source = if namespace == "javax" { source.replace("jakarta.persistence.", "javax.persistence.") } else { source };
    let target = format!("{directory}/{name}.java");
    if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) || source.len() > MAX_TEXT {
        return Err(Error::Stale);
    }
    no_collision(project, &pending.root, &directory, &name, None)?;
    let after = patched(&doc.text, &edits)?;
    let attribute_note = if edits.is_empty() { String::new() } else { format!("\nThe invoking file after the edit ({}):\n{after}", doc.path) };
    if !ui_kit::prompts::confirm_exact(
        if collection.is_some() { "Create the missing entity?" } else { "Create the missing mapped superclass?" },
        &format!("Reference: {} (version {})\nNew file: {target}\n\n{source}{attribute_note}\nThe file is created next to the invoking class; no existing file is overwritten.", doc.path, doc.version),
    )? {
        return Err(Error::Cancelled);
    }
    let project = current(pending, doc)?;
    no_collision(&project, &pending.root, &directory, &name, None)?;
    Ok(FileAction {
        title: format!("Create {name}"),
        kind: Some("quickfix".into()),
        operations: vec![FileOperation::Create { path: target.clone(), text: source, overwrite: false }],
        edits: if edits.is_empty() { Vec::new() } else { vec![(doc.path.clone(), edits)] },
        open: Some(target),
        ..FileAction::default()
    })
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project, created: Created) -> Result<FileAction, Error> {
    if created != Created::ContextDto {
        return resolve_persistence(pending, doc, project, created);
    }
    current(pending, doc)?;
    let (start, end) = span(doc, pending.selection).ok_or(Error::Stale)?;
    let masked = mask(&doc.text);
    let found = site(doc, &masked, start, end).ok_or(Error::Stale)?;
    let name = &doc.text[start..end];
    if !unresolved(doc, project, &pending.root, name) {
        return Err(format!("`{name}` already resolves; nothing is created.").into());
    }
    let entity = entity_of_site(&found, doc, project, &pending.root)?;
    resolve_with(pending, doc, project, Created::Dto, Some(entity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, text::position};

    const ENTITY: &str = "package shop.domain;\n\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n\n@Entity\npublic class Customer {\n    @Id\n    private Long id;\n\n    private String title;\n}\n";
    const ORDER: &str = "package shop.domain;\n\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport java.util.List;\n\n@Entity\npublic class Order extends BaseEntity {\n    @Id\n    private Long id;\n\n    private Invoice invoice;\n\n    private List<Line> lines;\n}\n";
    const REPOSITORY: &str = "package shop.domain;\n\nimport java.util.List;\nimport java.util.Optional;\nimport org.springframework.data.jpa.repository.JpaRepository;\n\npublic interface CustomerRepository extends JpaRepository<Customer, Long> {\n    List<CustomerSummary> findByTitle(String title);\n\n    Optional<CustomerCard> findFirstByTitle(String title);\n\n    Page<String> findByIdNotNull();\n}\n";
    const SERVICE: &str = "package shop.service;\n\nimport org.modelmapper.ModelMapper;\nimport org.springframework.stereotype.Service;\nimport shop.domain.Customer;\n\n@Service\npublic class CustomerService {\n    private final ModelMapper modelMapper;\n\n    CustomerService(ModelMapper modelMapper) {\n        this.modelMapper = modelMapper;\n    }\n\n    Object run(Customer customer) {\n        return modelMapper.map(customer, CustomerView.class);\n    }\n}\n";
    const MAPPER: &str = "package shop.web;\n\nimport org.mapstruct.Mapper;\nimport org.mapstruct.MappingTarget;\nimport shop.domain.Customer;\n\n@Mapper(componentModel = \"spring\")\npublic interface CustomerMapper {\n    CustomerPayload toPayload(Customer customer);\n\n    Customer toEntity(CustomerForm form);\n\n    void update(CustomerPatch patch, @MappingTarget Customer customer);\n}\n";
    const MAPSTRUCT_POM: &str = "<project><dependencies><dependency><groupId>org.mapstruct</groupId><artifactId>mapstruct</artifactId></dependency></dependencies></project>";
    const MODELMAPPER_POM: &str = "<project><dependencies><dependency><groupId>org.modelmapper</groupId><artifactId>modelmapper</artifactId></dependency></dependencies></project>";

    fn prepare(path: &str, text: &str, build: Option<&str>) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/domain/Customer.java", ENTITY);
        host.add_file(path.trim_start_matches("/project/"), text);
        if let Some(build) = build {
            host.add_file("pom.xml", build);
        }
        host.open_editor(path, "java", text, &[]);
        (host, JpaAssist::default(), Document { path: path.into(), language: "java".into(), text: text.into(), version: 1 })
    }

    fn context_titles(plugin: &mut JpaAssist, doc: &Document, word: &str) -> Vec<String> {
        let start = doc.text.find(word).unwrap();
        let range = Range::new(position(&doc.text, start), position(&doc.text, start + 1));
        plugin.file_actions(doc, range).into_iter().map(|action| action.title).filter(|title| [ENTITY_TITLE, SUPERCLASS_TITLE, CONTEXT_DTO_TITLE].contains(&title.as_str())).collect()
    }

    const ORDER_PATH: &str = "/project/src/main/java/shop/domain/Order.java";
    const REPO_PATH: &str = "/project/src/main/java/shop/domain/CustomerRepository.java";
    const SERVICE_PATH: &str = "/project/src/main/java/shop/service/CustomerService.java";
    const MAPPER_PATH: &str = "/project/src/main/java/shop/web/CustomerMapper.java";

    #[test]
    fn an_unresolved_attribute_type_and_extends_clause_offer_entity_and_mapped_superclass() {
        let (_host, mut plugin, doc) = prepare(ORDER_PATH, ORDER, None);
        assert_eq!(context_titles(&mut plugin, &doc, "Invoice invoice"), [ENTITY_TITLE]);
        assert_eq!(context_titles(&mut plugin, &doc, "Line>"), [ENTITY_TITLE]);
        assert_eq!(context_titles(&mut plugin, &doc, "BaseEntity"), [SUPERCLASS_TITLE]);
    }

    #[test]
    fn creating_the_entity_adds_the_relationship_and_the_file() {
        let (host, mut plugin, doc) = prepare(ORDER_PATH, ORDER, None);
        let start = doc.text.find("Invoice invoice").unwrap();
        let range = Range::new(position(&doc.text, start), position(&doc.text, start + 1));
        let pending = plugin.file_actions(&doc, range).into_iter().find(|action| action.title == ENTITY_TITLE).unwrap();
        host.answer_next_prompt(Some(&["ManyToOne"]));
        host.answer_next_prompt(Some(&["IDENTITY"]));
        host.answer_next_prompt(Some(&["invoice"]));
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        let [FileOperation::Create { path, text, overwrite }] = action.operations.as_slice() else { panic!("one file") };
        assert!(path.ends_with("shop/domain/Invoice.java") && !overwrite, "{path}");
        assert!(text.contains("@Entity") && text.contains("@Table(name = \"invoice\")") && text.contains("GenerationType.IDENTITY"), "{text}");
        let [(edited, edits)] = action.edits.as_slice() else { panic!("one edit") };
        assert_eq!(edited, ORDER_PATH);
        let after = patched(ORDER, edits).unwrap();
        assert!(after.contains("    @ManyToOne\n    private Invoice invoice;") && after.contains("import jakarta.persistence.ManyToOne;"), "{after}");
    }

    #[test]
    fn creating_the_mapped_superclass_writes_one_file_and_edits_nothing() {
        let (host, mut plugin, doc) = prepare(ORDER_PATH, ORDER, None);
        let start = doc.text.find("BaseEntity").unwrap();
        let range = Range::new(position(&doc.text, start), position(&doc.text, start + 1));
        let pending = plugin.file_actions(&doc, range).into_iter().find(|action| action.title == SUPERCLASS_TITLE).unwrap();
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        let [FileOperation::Create { text, .. }] = action.operations.as_slice() else { panic!("one file") };
        assert!(text.contains("@MappedSuperclass") && text.contains("class BaseEntity"), "{text}");
        assert!(action.edits.is_empty());
    }

    #[test]
    fn declared_wildcard_and_existing_types_stay_silent() {
        let (host, mut plugin, doc) = prepare(ORDER_PATH, ORDER, None);
        host.add_file("src/main/java/shop/domain/Invoice.java", "package shop.domain;\npublic class Invoice {}\n");
        assert!(context_titles(&mut plugin, &doc, "Invoice invoice").is_empty());
        let wildcard = Document { text: ORDER.replace("import java.util.List;", "import java.util.List;\nimport com.acme.model.*;"), ..doc.clone() };
        assert!(context_titles(&mut plugin, &wildcard, "BaseEntity").is_empty());
    }

    #[test]
    fn a_repository_return_type_offers_a_dto_for_the_repository_entity() {
        let (host, mut plugin, doc) = prepare(REPO_PATH, REPOSITORY, None);
        assert_eq!(context_titles(&mut plugin, &doc, "CustomerSummary"), [CONTEXT_DTO_TITLE]);
        assert_eq!(context_titles(&mut plugin, &doc, "CustomerCard"), [CONTEXT_DTO_TITLE]);
        assert!(context_titles(&mut plugin, &doc, "Page<String>").is_empty());
        let start = doc.text.find("CustomerSummary").unwrap();
        let range = Range::new(position(&doc.text, start), position(&doc.text, start + 1));
        let pending = plugin.file_actions(&doc, range).into_iter().find(|action| action.title == CONTEXT_DTO_TITLE).unwrap();
        host.answer_next_prompt(Some(&["id", "title"]));
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        let [FileOperation::Create { path, text, .. }] = action.operations.as_slice() else { panic!("one file") };
        assert!(path.ends_with("shop/domain/CustomerSummary.java") && text.contains("public record CustomerSummary(") && text.contains("Long id,") && text.contains("String title"), "{path} {text}");
    }

    #[test]
    fn a_model_mapper_target_needs_the_build_proof_and_an_entity_argument() {
        let (_host, mut plugin, doc) = prepare(SERVICE_PATH, SERVICE, Some(MODELMAPPER_POM));
        assert_eq!(context_titles(&mut plugin, &doc, "CustomerView"), [CONTEXT_DTO_TITLE]);
        let (_host, mut plugin, doc) = prepare(SERVICE_PATH, SERVICE, None);
        assert!(context_titles(&mut plugin, &doc, "CustomerView").is_empty());
        let (_host, mut plugin, doc) = prepare(SERVICE_PATH, &SERVICE.replace("Object run(Customer customer)", "Object run(Missing customer)"), Some(MODELMAPPER_POM));
        assert!(context_titles(&mut plugin, &doc, "CustomerView").is_empty());
    }

    #[test]
    fn mapper_method_types_resolve_the_entity_from_the_other_side_or_the_mapping_target() {
        let (host, mut plugin, doc) = prepare(MAPPER_PATH, MAPPER, Some(MAPSTRUCT_POM));
        assert_eq!(context_titles(&mut plugin, &doc, "CustomerPayload"), [CONTEXT_DTO_TITLE]);
        assert_eq!(context_titles(&mut plugin, &doc, "CustomerForm"), [CONTEXT_DTO_TITLE]);
        assert_eq!(context_titles(&mut plugin, &doc, "CustomerPatch"), [CONTEXT_DTO_TITLE]);
        let start = doc.text.find("CustomerForm").unwrap();
        let range = Range::new(position(&doc.text, start), position(&doc.text, start + 1));
        let pending = plugin.file_actions(&doc, range).into_iter().find(|action| action.title == CONTEXT_DTO_TITLE).unwrap();
        host.answer_next_prompt(Some(&["title"]));
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        let [FileOperation::Create { text, .. }] = action.operations.as_slice() else { panic!("one file") };
        assert!(text.contains("public record CustomerForm(") && text.contains("String title") && !text.contains("Long id"), "{text}");
        let (_host, mut plugin, doc) = prepare(MAPPER_PATH, MAPPER, None);
        assert!(context_titles(&mut plugin, &doc, "CustomerPayload").is_empty());
    }

    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_API; compiles the created entity, mapped superclass and DTO with the edited caller"]
    fn generated_entity_mapped_superclass_and_dto_compile() {
        let api = std::env::var("JPA_JAKARTA_API").expect("explicit API jar path");
        let order = ORDER.replace("    private List<Line> lines;\n", "");
        let (host, mut plugin, doc) = prepare(ORDER_PATH, &order, None);
        let mut files: Vec<(String, String)> = vec![("shop/domain/Customer.java".into(), ENTITY.into())];
        let mut caller = order.clone();
        for (word, answers) in [("BaseEntity", vec![vec![]]), ("Invoice invoice", vec![vec!["ManyToOne"], vec!["IDENTITY"], vec!["invoice"], vec![]])] {
            let start = doc.text.find(word).unwrap();
            let range = Range::new(position(&doc.text, start), position(&doc.text, start + 1));
            let title = if word == "BaseEntity" { SUPERCLASS_TITLE } else { ENTITY_TITLE };
            let pending = plugin.file_actions(&doc, range).into_iter().find(|action| action.title == title).unwrap();
            for answer in answers {
                host.answer_next_prompt(Some(&answer));
            }
            let action = plugin.resolve_file_action(&doc, pending).unwrap();
            for operation in &action.operations {
                let FileOperation::Create { path, text, .. } = operation else { panic!("create") };
                files.push((path.rsplit_once("src/main/java/").unwrap().1.to_owned(), text.clone()));
            }
            for (_, edits) in &action.edits {
                caller = patched(&caller, edits).unwrap();
            }
        }
        files.push(("shop/domain/Order.java".into(), caller));
        let dir = std::env::temp_dir().join(format!("jpa-unresolved-context-{}", std::process::id()));
        let paths: Vec<_> = files
            .iter()
            .map(|(file, body)| {
                let path = dir.join(file);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, body).unwrap();
                path
            })
            .collect();
        let result = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &api, "-d"]).arg(&dir).args(&paths).output().expect("javac installed");
        assert!(result.status.success(), "{}\n{files:?}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "requires javac; the DTO created from a repository method return type compiles"]
    fn generated_context_dto_compiles() {
        let (host, mut plugin, doc) = prepare(REPO_PATH, REPOSITORY, None);
        let start = doc.text.find("CustomerSummary").unwrap();
        let range = Range::new(position(&doc.text, start), position(&doc.text, start + 1));
        let pending = plugin.file_actions(&doc, range).into_iter().find(|action| action.title == CONTEXT_DTO_TITLE).unwrap();
        host.answer_next_prompt(Some(&["id", "title"]));
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        let [FileOperation::Create { text, .. }] = action.operations.as_slice() else { panic!("one file") };
        let dir = std::env::temp_dir().join(format!("jpa-context-dto-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("CustomerSummary.java");
        std::fs::write(&file, text).unwrap();
        let result = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-d"]).arg(&dir).arg(&file).output().expect("javac installed");
        assert!(result.status.success(), "{}\n{text}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
