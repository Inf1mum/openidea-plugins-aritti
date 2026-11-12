//! Assistance at a call site in a Spring bean: repository calls typed on an entity class name, `mapTo...` mapper calls
//! typed on a variable, and the quick fix that adds a missing derived query method to the repository.
//! Completions inject the bean the call needs (reusing `injectionStyle` and its detection); nothing is written elsewhere.
use super::*;
use super::injection_action::{self, Repo, Style};
use super::unresolved_create::{absolute, entity_document};
use crate::model::{closing, decapitalize, import_edit, is_ident, split_top_level};
use crate::queries;
use openidea_plugin::{
    providers::{CompletionItem, CompletionKind},
    text::{offset, position},
};
use regex_lite::Regex;

#[path = "call_more.rs"]
mod more;

pub(super) const CREATE_METHOD_TITLE: &str = "Create repository method from this call…";
const SPRING_STEREOTYPES: [&str; 6] = ["Service", "Component", "Controller", "RestController", "Repository", "Configuration"];
const VERBS: [&str; 4] = ["find", "exists", "count", "delete"];
const MAX_ITEMS: usize = 128;
const MAX_FILES: u32 = 64;

fn spring_component(doc: &Document) -> bool {
    annotations(&mask(&doc.text)).iter().any(|annotation| SPRING_STEREOTYPES.contains(&annotation.name))
}

struct Receiver {
    start: usize,
    name: String,
    word_start: usize,
    word_end: usize,
}

fn receiver(text: &str, cursor: usize) -> Option<Receiver> {
    let bytes = text.as_bytes();
    let word_start = cursor - bytes[..cursor].iter().rev().take_while(|&&byte| is_ident(byte)).count();
    let word_end = cursor + bytes[cursor..].iter().take_while(|&&byte| is_ident(byte)).count();
    if word_start == 0 || bytes[word_start - 1] != b'.' {
        return None;
    }
    let end = word_start - 1;
    let start = end - bytes[..end].iter().rev().take_while(|&&byte| is_ident(byte)).count();
    if start == end || (start > 0 && (bytes[start - 1] == b'.' || is_ident(bytes[start - 1]))) {
        return None;
    }
    Some(Receiver { start, name: text[start..end].to_owned(), word_start, word_end })
}

fn simple(ty: &str) -> &str {
    ty.trim().trim_end_matches('?').rsplit('.').next().unwrap_or(ty)
}

/// The declared type of the variable `name` in `text`, with the element type for a collection.
pub(super) fn declared_type(text: &str, name: &str, kotlin: bool) -> Option<(String, bool)> {
    let collection = r"(?:List|Set|Collection|Iterable)\s*<\s*[\w.]+\s*>";
    let pattern = if kotlin {
        format!(r"\b{name}\s*:\s*({collection}|[A-Z][\w.]*)")
    } else {
        format!(r"\b({collection}|[A-Z][\w.]*)\s+{name}\s*[;=,)]")
    };
    let masked = mask(text);
    let found = Regex::new(&pattern).ok()?.captures(&masked)?.get(1)?.as_str().to_owned();
    match found.split_once('<') {
        Some((_, rest)) => Some((simple(rest.trim_end_matches('>')).to_owned(), true)),
        None => Some((simple(&found).to_owned(), false)),
    }
}

fn parameter_names(declaration: &str, kotlin: bool) -> Vec<String> {
    let Some(open) = declaration.find('(') else {
        return Vec::new();
    };
    let Some(close) = closing(declaration.as_bytes(), open) else {
        return Vec::new();
    };
    split_top_level(&declaration[open + 1..close])
        .into_iter()
        .filter_map(|(_, part)| {
            let part = part.trim();
            let name = if kotlin { part.split(':').next() } else { part.rsplit(char::is_whitespace).next() };
            name.map(|name| name.trim().to_owned()).filter(|name| !name.is_empty())
        })
        .collect()
}

fn snippet(variable: &str, method: &str, parameters: &[String]) -> String {
    let placeholders: Vec<String> = parameters.iter().enumerate().map(|(index, name)| format!("${{{}:{name}}}", index + 1)).collect();
    format!("{variable}.{method}({})", placeholders.join(", "))
}

fn package_of_source(text: &str) -> Option<String> {
    mask(text).lines().find_map(|line| line.trim().strip_prefix("package ")).map(|package| package.trim().trim_end_matches(';').trim().to_owned())
}

fn bean_edits(doc: &Document, project: &Project, root: &str, bean: &Repo) -> Option<Vec<TextEdit>> {
    let source = injection_action::analyze(doc).ok()?;
    let style = injection_detect::style_for(&doc.text).ok()?;
    let lombok = style == Style::Lombok && injection_action::lombok_provable(doc, project, root).ok()?;
    match injection_action::plan(doc, &source, bean, style, lombok) {
        Ok(edits) => Some(edits),
        Err(message) if message.contains("already injected") => Some(Vec::new()),
        Err(_) => None,
    }
}

struct MapMethod {
    name: String,
    parameter: String,
    returned: String,
}

struct MapperInfo {
    bean: Repo,
    methods: Vec<MapMethod>,
}

fn parse_mapper(path: &str, text: &str) -> Option<MapperInfo> {
    let kotlin = path.ends_with(".kt");
    let spring = Regex::new(r#"@Mapper\s*\([^)]*componentModel\s*=\s*(?:"spring"|MappingConstants\.ComponentModel\.SPRING)"#).ok()?;
    if !spring.is_match(text) {
        return None;
    }
    let masked = mask(text);
    let keyword = words(&masked, "interface").next()?;
    let open = masked[keyword..].find('{').map(|at| at + keyword)?;
    let close = closing(masked.as_bytes(), open)?;
    let name = masked[keyword + "interface".len()..open].split_whitespace().next()?.to_owned();
    let body = Regex::new(r"@[\w.]+(?:\([^)]*\))?").ok()?.replace_all(&masked[open + 1..close], "").into_owned();
    let pattern = if kotlin {
        r"fun\s+(\w+)\s*\(\s*\w+\s*:\s*([\w.<>?, ]+?)\s*\)\s*:\s*([\w.<>?, ]+?)\s*(?:\n|$)"
    } else {
        r"([\w.<>?, \[\]]+?)\s+(\w+)\s*\(\s*([\w.<>?, \[\]]+?)\s+\w+\s*\)\s*;"
    };
    let methods = Regex::new(pattern)
        .ok()?
        .captures_iter(&body)
        .map(|caps| {
            if kotlin {
                MapMethod { name: caps[1].to_owned(), parameter: caps[2].trim().to_owned(), returned: caps[3].trim().to_owned() }
            } else {
                MapMethod { name: caps[2].to_owned(), parameter: caps[3].trim().to_owned(), returned: caps[1].trim().to_owned() }
            }
        })
        .filter(|method| method.returned != "void")
        .collect();
    Some(MapperInfo { bean: Repo { name, entity: String::new(), package: package_of_source(text)? }, methods })
}

fn spring_mappers(project: &Project, root: &str, own: &str) -> Vec<MapperInfo> {
    let mut found = Vec::new();
    for extension in ["java", "kt"] {
        let files = project.find_files(&format!("**/*Mapper.{extension}"), MAX_FILES).unwrap_or_default();
        for file in files.into_iter().map(|file| absolute(root, file)).filter(|file| file != own && inside(root, file)) {
            let Ok(text) = project.read_file(&file) else { continue };
            if text.len() > MAX_TEXT {
                continue;
            }
            found.extend(parse_mapper(&file, &text));
        }
    }
    found.sort_by(|a, b| a.bean.name.cmp(&b.bean.name));
    found
}

fn call_item(doc: &Document, receiver: &Receiver, label: String, detail: String, insert: String, edits: &[TextEdit], extra_imports: &[&str], kotlin: bool) -> CompletionItem {
    let mut additional: Vec<TextEdit> = edits.to_vec();
    additional.extend(import_edit(&doc.text, extra_imports, kotlin).map(|edit| edit.to_text_edit(&doc.text)));
    CompletionItem {
        detail: Some(detail),
        insert_text: insert,
        snippet: true,
        replace: Some(Range::new(position(&doc.text, receiver.start), position(&doc.text, receiver.word_end))),
        filter_text: Some(format!("{}.{}", receiver.name, doc.text[receiver.word_start..receiver.word_end].to_owned())),
        additional_edits: additional,
        ..CompletionItem::new(&label, CompletionKind::Method)
    }
}

fn entity_props(entity: &Class) -> Vec<queries::Prop> {
    entity
        .attributes
        .iter()
        .map(|attribute| queries::Prop {
            name: attribute.name.clone(),
            ty: attribute.ty.clone(),
            fqcn: entity.qualified(simple(&attribute.ty)).filter(|fqcn| fqcn.contains('.') && !fqcn.starts_with("java.lang.")),
            target: None,
        })
        .collect()
}

/// `(parameter list, return type, imports)` of the derived query `name` of `entity`, `None` when it does not parse.
fn derived(name: &str, entity: &Class, kotlin: bool) -> Option<(String, Option<String>, Vec<String>)> {
    let (verb, predicate_start) = queries::split_subject(name)?;
    let predicate = &name[predicate_start..];
    let props = entity_props(entity);
    let resolve = |owner: &str| (owner == entity.name).then(|| props.clone());
    let parsed = queries::parse(predicate, &entity.name, &resolve);
    if parsed.unverified || !parsed.complete(predicate) {
        return None;
    }
    Some(queries::signature(name, verb, &parsed, &entity.name, kotlin))
}

impl JpaAssist {
    pub(crate) fn call_completion(&mut self, doc: &Document, at: Position) -> Vec<CompletionItem> {
        if !matches!(doc.language.as_str(), "java" | "kotlin") || doc.text.len() > MAX_TEXT || !spring_component(doc) {
            return vec![];
        }
        let Some(cursor) = offset(&doc.text, at) else {
            return vec![];
        };
        let Some(receiver) = receiver(&doc.text, cursor) else {
            return vec![];
        };
        let Some(project) = host::project_of(&doc.path) else {
            return vec![];
        };
        let Ok(root) = project.root() else {
            return vec![];
        };
        let typed = doc.text[receiver.word_start..cursor].to_lowercase();
        if receiver.name.starts_with(|ch: char| ch.is_ascii_uppercase()) {
            return self.repository_calls(doc, &project, &root, &receiver, &typed);
        }
        let mut items = mapper_calls(doc, &project, &root, &receiver, &typed);
        items.extend(more::instance_calls(doc, &project, &root, &receiver, &typed));
        items
    }

    fn repository_calls(&mut self, doc: &Document, project: &Project, root: &str, receiver: &Receiver, typed: &str) -> Vec<CompletionItem> {
        let kotlin = doc.language == "kotlin";
        let Some(entity) = self.repository_completion.entity_by_name(root, &receiver.name).cloned() else {
            return vec![];
        };
        let Some((_, location)) = self.repositories_of(project, &entity.name).into_iter().next() else {
            return vec![];
        };
        let Ok(text) = project.read_file(&location.path) else {
            return vec![];
        };
        let Some(repository) = queries::parse_repository(&text, location.path.ends_with(".kt")) else {
            return vec![];
        };
        let Some(package) = package_of_source(&text) else {
            return vec![];
        };
        let bean = Repo { name: repository.name.clone(), entity: entity.name.clone(), package };
        let Some(edits) = bean_edits(doc, project, root, &bean) else {
            return vec![];
        };
        let variable = decapitalize(&bean.name);
        let mut items = Vec::new();
        let mut declared: Vec<&str> = Vec::new();
        for method in repository.methods.iter().filter(|method| !method.declared_query) {
            declared.push(&method.name);
            let names = parameter_names(&text[method.start..], location.path.ends_with(".kt"));
            items.push(call_item(doc, receiver, format!("{}()", method.name), format!("{} (existing)", bean.name), snippet(&variable, &method.name, &names), &edits, &[], kotlin));
        }
        for attribute in projection_action::offered(&entity, projection_action::Form::Record) {
            for verb in VERBS {
                let name = format!("{verb}By{}", crate::model::capitalize(&attribute.0));
                if declared.contains(&name.as_str()) {
                    continue;
                }
                let Some((parameters, _, _)) = derived(&name, &entity, kotlin) else {
                    continue;
                };
                let names = parameter_names(&format!("({parameters})"), kotlin);
                items.push(call_item(doc, receiver, format!("{name}()"), format!("{} (new: add the method with the quick fix)", bean.name), snippet(&variable, &name, &names), &edits, &[], kotlin));
            }
        }
        items.retain(|item| item.label.to_lowercase().starts_with(typed));
        items.truncate(MAX_ITEMS);
        items
    }
}

fn mapper_calls(doc: &Document, project: &Project, root: &str, receiver: &Receiver, typed: &str) -> Vec<CompletionItem> {
    if !(typed.len() >= 3 && ("mapto".starts_with(typed) || typed.starts_with("mapto"))) {
        return vec![];
    }
    let kotlin = doc.language == "kotlin";
    let Some((ty, collection)) = declared_type(&doc.text, &receiver.name, kotlin) else {
        return vec![];
    };
    let mut items = Vec::new();
    for mapper in spring_mappers(project, root, &doc.path) {
        let variable = decapitalize(&mapper.bean.name);
        let mut offered: Vec<(String, String, Vec<&str>, &MapMethod)> = Vec::new();
        for method in &mapper.methods {
            let parameter = method.parameter.trim_end_matches('?');
            let as_collection = Regex::new(r"^(?:List|Set|Collection|Iterable)\s*<\s*([\w.]+)\s*>$").ok().and_then(|re| re.captures(parameter).map(|caps| simple(&caps[1]).to_owned()));
            let call = match (collection, as_collection) {
                (true, Some(element)) if element == ty => format!("{variable}.{}({})", method.name, receiver.name),
                (false, None) if simple(parameter) == ty => format!("{variable}.{}({})", method.name, receiver.name),
                (true, None) if simple(parameter) == ty && kotlin => format!("{}.map({variable}::{})", receiver.name, method.name),
                (true, None) if simple(parameter) == ty => format!("{}.stream().map({variable}::{}).collect(Collectors.toList())", receiver.name, method.name),
                _ => continue,
            };
            let imports = if call.contains("Collectors") { vec!["java.util.stream.Collectors"] } else { Vec::new() };
            let returned = Regex::new(r"[A-Z]\w*").ok().and_then(|re| re.find_iter(&method.returned).filter(|m| !["List", "Set", "Collection", "Iterable"].contains(&m.as_str())).last().map(|m| m.as_str().to_owned())).unwrap_or_else(|| method.returned.clone());
            offered.push((format!("mapTo{returned}"), call, imports, method));
        }
        if offered.is_empty() {
            continue;
        }
        let Some(edits) = bean_edits(doc, project, root, &mapper.bean) else {
            continue;
        };
        for (label, call, imports, method) in offered {
            items.push(call_item(doc, receiver, format!("{label} : {call}"), format!("{}.{} (injects {})", mapper.bean.name, method.name, mapper.bean.name), call.clone(), &edits, &imports, kotlin));
        }
    }
    items.retain(|item| item.label.to_lowercase().starts_with(typed));
    items.truncate(MAX_ITEMS);
    items
}

/// The method name under `selection` when it is called on a variable whose declared type ends with `Repository`.
fn call_under(doc: &Document, selection: Range) -> Option<(String, String)> {
    let cursor = offset(&doc.text, selection.start)?;
    let receiver = receiver(&doc.text, cursor)?;
    let method = doc.text[receiver.word_start..receiver.word_end].to_owned();
    if !doc.text[receiver.word_end..].trim_start().starts_with('(') || queries::split_subject(&method).is_none() {
        return None;
    }
    let (ty, collection) = declared_type(&doc.text, &receiver.name, doc.language == "kotlin")?;
    (!collection && ty.ends_with("Repository") && ty != "Repository").then_some((receiver.name, ty))
}

pub(super) fn eligible(doc: &Document, selection: Range) -> bool {
    spring_component(doc) && call_under(doc, selection).is_some()
}

fn repository_source(project: &Project, root: &str, name: &str) -> Result<(String, String), Error> {
    for extension in ["java", "kt"] {
        for file in project.find_files(&format!("**/{name}.{extension}"), 4)?.into_iter().map(|file| absolute(root, file)) {
            if inside(root, &file) && path(&file).as_deref() == Some(file.as_str()) {
                let text = project.read_file(&file)?;
                return Ok((file, text));
            }
        }
    }
    Err(format!("No source file for `{name}` was found in this project.").into())
}

pub(super) fn resolve_create_method(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let (_, repository_name) = call_under(doc, pending.selection).ok_or(Error::Stale)?;
    let cursor = offset(&doc.text, pending.selection.start).ok_or(Error::Stale)?;
    let call = receiver(&doc.text, cursor).ok_or(Error::Stale)?;
    let method = doc.text[call.word_start..call.word_end].to_owned();
    let (repository_path, repository_text) = repository_source(project, &pending.root, &repository_name)?;
    let kotlin = repository_path.ends_with(".kt");
    let repository = queries::parse_repository(&repository_text, kotlin).ok_or("The repository interface is not a Spring Data repository this assistant can read.")?;
    if repository.methods.iter().any(|existing| existing.name == method) {
        return Err(format!("{repository_name} already declares `{method}`; nothing is added.").into());
    }
    let entity_doc = entity_document(project, &pending.root, &repository.entity, kotlin)?;
    let entity = current_class(&entity_doc).ok_or(Error::Stale)?;
    let (parameters, returned, imports) = derived(&method, &entity, kotlin)
        .ok_or_else(|| format!("`{method}` is not a derived query of {}: the name after By must be attributes of the entity joined by And or Or.", entity.name))?;
    let declaration = query_method_action::declaration_text(kotlin, &method, &parameters, returned);
    let masked = mask(&repository_text);
    let open = masked[repository.name_start..].find('{').map(|at| at + repository.name_start).ok_or("The repository has no body to add a method to.")?;
    let close = closing(masked.as_bytes(), open).ok_or(Error::Stale)?;
    let line_start = repository_text[..close].rfind('\n').map_or(0, |at| at + 1);
    let own_line = repository_text[line_start..close].trim().is_empty();
    let crlf = repository_text.contains("\r\n");
    let member = format!("    {declaration}\n");
    let (at, body) = if own_line { (line_start, member) } else { (close, format!("\n{member}")) };
    let mut edits = vec![crate::model::Edit::insert(at, &if crlf { body.replace('\n', "\r\n") } else { body })];
    let refs: Vec<&str> = imports.iter().map(String::as_str).collect();
    edits.extend(import_edit(&repository_text, &refs, kotlin));
    let converted: Vec<TextEdit> = edits.iter().map(|edit| edit.to_text_edit(&repository_text)).collect();
    let after = patched(&repository_text, &converted)?;
    if !ui_kit::prompts::confirm_exact(
        "Add the method to the repository?",
        &format!("Call: {} (version {})\nRepository: {repository_path}\nEntity: {} ({})\n\n{after}\nOnly the repository is edited; the call site is not changed.", doc.path, doc.version, entity.name, entity_doc.path),
    )? {
        return Err(Error::Cancelled);
    }
    let project = current(pending, doc)?;
    if project.read_file(&repository_path)? != repository_text {
        return Err(Error::Stale);
    }
    Ok(FileAction {
        title: format!("Add {method} to {repository_name}"),
        kind: Some("quickfix".into()),
        edits: vec![(repository_path, converted)],
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{diagnostics::DiagnosticsProvider, mock::MockHost, providers::Providers};

    const ENTITY: &str = "package shop.domain;\n\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport java.time.LocalDate;\n\n@Entity\npublic class Customer {\n    @Id\n    private Long id;\n\n    private String title;\n\n    private LocalDate joined;\n}\n";
    const REPOSITORY: &str = "package shop.domain;\n\nimport java.util.List;\nimport org.springframework.data.jpa.repository.JpaRepository;\n\npublic interface CustomerRepository extends JpaRepository<Customer, Long> {\n    List<Customer> findByTitle(String title);\n}\n";
    const MAPPER: &str = "package shop.web;\n\nimport java.util.List;\nimport org.mapstruct.Mapper;\nimport shop.domain.Customer;\n\n@Mapper(componentModel = \"spring\")\npublic interface CustomerMapper {\n    CustomerDto toDto(Customer customer);\n\n    Customer toEntity(CustomerDto dto);\n}\n";
    const SERVICE: &str = "package shop.service;\n\nimport org.springframework.stereotype.Service;\nimport shop.domain.Customer;\nimport java.util.List;\n\n@Service\npublic class CustomerService {\n\n    void run(Customer customer, List<Customer> customers) {\n        Customer.findBy\n        customer.mapTo\n        customers.mapTo\n    }\n}\n";
    const PATH: &str = "/project/src/main/java/shop/service/CustomerService.java";

    fn setup(service: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/domain/Customer.java", ENTITY);
        host.add_file("src/main/java/shop/domain/CustomerRepository.java", REPOSITORY);
        host.add_file("src/main/java/shop/web/CustomerMapper.java", MAPPER);
        host.add_file("src/main/java/shop/service/CustomerService.java", service);
        host.open_editor(PATH, "java", service, &[]);
        let mut plugin = JpaAssist::default();
        plugin.diagnostics(&Document { path: "/project/src/main/java/shop/domain/Customer.java".into(), language: "java".into(), version: 1, text: ENTITY.into() });
        let doc = Document { path: PATH.into(), language: "java".into(), version: 1, text: service.into() };
        (host, plugin, doc)
    }

    fn complete(plugin: &mut JpaAssist, doc: &Document, marker: &str) -> Vec<CompletionItem> {
        let at = doc.text.find(marker).unwrap() + marker.len();
        plugin.completion(doc, position(&doc.text, at), None)
    }

    #[test]
    fn a_call_on_the_entity_class_name_offers_existing_and_new_repository_methods_and_injects_the_repository() {
        let (_host, mut plugin, doc) = setup(SERVICE);
        let items = complete(&mut plugin, &doc, "Customer.findBy");
        let existing = items.iter().find(|item| item.label == "findByTitle()").expect("existing method");
        assert_eq!(existing.insert_text, "customerRepository.findByTitle(${1:title})");
        assert!(existing.detail.as_deref().unwrap().contains("existing"));
        let created = items.iter().find(|item| item.label == "findByJoined()").expect("derived method");
        assert_eq!(created.insert_text, "customerRepository.findByJoined(${1:joined})");
        assert!(items.iter().all(|item| item.label.starts_with("findBy")));
        let injected = patched(SERVICE, &existing.additional_edits).unwrap();
        assert!(injected.contains("private final CustomerRepository customerRepository;") && injected.contains("import shop.domain.CustomerRepository;"), "{injected}");
        assert_eq!(existing.replace.unwrap().start, position(SERVICE, SERVICE.find("Customer.findBy").unwrap()));
    }

    #[test]
    fn the_injection_follows_the_detected_or_configured_style() {
        let lombok = SERVICE.replace("@Service\n", "@Service\n@RequiredArgsConstructor\n").replace("import java.util.List;", "import java.util.List;\nimport lombok.RequiredArgsConstructor;");
        let (_host, mut plugin, doc) = setup(&lombok);
        let items = complete(&mut plugin, &doc, "Customer.findBy");
        let injected = patched(&lombok, &items[0].additional_edits).unwrap();
        assert!(injected.contains("private final CustomerRepository customerRepository;") && !injected.contains("CustomerService(CustomerRepository"), "{injected}");
        let already = SERVICE.replace("void run", "private final CustomerRepository customerRepository;\n\n    void run");
        let (_host, mut plugin, doc) = setup(&already);
        assert!(complete(&mut plugin, &doc, "Customer.findBy")[0].additional_edits.is_empty());
    }

    #[test]
    fn map_to_offers_the_mapper_call_for_one_instance_and_for_a_collection() {
        let (_host, mut plugin, doc) = setup(SERVICE);
        let single = complete(&mut plugin, &doc, "customer.mapTo");
        let item = single.iter().find(|item| item.label.starts_with("mapToCustomerDto")).expect("single");
        assert_eq!(item.insert_text, "customerMapper.toDto(customer)");
        assert!(patched(SERVICE, &item.additional_edits).unwrap().contains("private final CustomerMapper customerMapper;"));
        assert!(single.iter().all(|item| !item.label.contains("toEntity")));
        let many = complete(&mut plugin, &doc, "customers.mapTo");
        let item = many.iter().find(|item| item.label.starts_with("mapToCustomerDto")).expect("collection");
        assert_eq!(item.insert_text, "customers.stream().map(customerMapper::toDto).collect(Collectors.toList())");
        assert!(patched(SERVICE, &item.additional_edits).unwrap().contains("import java.util.stream.Collectors;"));
    }

    #[test]
    fn outside_a_spring_bean_or_without_a_known_entity_nothing_is_offered() {
        let (_host, mut plugin, doc) = setup(&SERVICE.replace("@Service\n", ""));
        assert!(complete(&mut plugin, &doc, "Customer.findBy").is_empty());
        let (_host, mut plugin, doc) = setup(&SERVICE.replace("Customer.findBy", "Unknown.findBy"));
        assert!(complete(&mut plugin, &doc, "Unknown.findBy").is_empty());
    }

    #[test]
    fn mapper_parsing_reads_java_and_kotlin_and_requires_the_spring_component_model() {
        let java = parse_mapper("/p/CustomerMapper.java", MAPPER).unwrap();
        assert_eq!(java.methods.iter().map(|m| (m.name.as_str(), m.parameter.as_str(), m.returned.as_str())).collect::<Vec<_>>(), [("toDto", "Customer", "CustomerDto"), ("toEntity", "CustomerDto", "Customer")]);
        let kotlin = parse_mapper("/p/CustomerMapper.kt", "package shop\n\n@Mapper(componentModel = \"spring\")\ninterface CustomerMapper {\n    fun toDto(customer: Customer): CustomerDto\n}\n").unwrap();
        assert_eq!(kotlin.methods[0].returned, "CustomerDto");
        assert!(parse_mapper("/p/CustomerMapper.java", &MAPPER.replace("componentModel = \"spring\"", "")).is_none());
    }

    const CALLER: &str = "package shop.service;\n\nimport org.springframework.stereotype.Service;\nimport shop.domain.CustomerRepository;\n\n@Service\npublic class CustomerService {\n    private final CustomerRepository customerRepository;\n\n    void run() {\n        customerRepository.findByJoinedAndTitle(null, null);\n    }\n}\n";

    #[test]
    fn the_quick_fix_adds_a_missing_derived_method_with_types_and_imports() {
        let (host, mut plugin, doc) = setup(CALLER);
        let start = CALLER.find("findByJoinedAndTitle").unwrap();
        let selection = Range::new(position(CALLER, start), position(CALLER, start + 3));
        let pending = plugin.file_actions(&doc, selection).into_iter().find(|action| action.title == CREATE_METHOD_TITLE).expect("offered");
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        let [(path, edits)] = action.edits.as_slice() else { panic!("one document") };
        assert!(path.ends_with("CustomerRepository.java"));
        let after = patched(REPOSITORY, edits).unwrap();
        assert!(after.contains("    List<Customer> findByJoinedAndTitle(LocalDate joined, String title);\n}"), "{after}");
        assert!(after.contains("import java.time.LocalDate;"), "{after}");
    }

    #[test]
    fn the_quick_fix_is_not_offered_for_declared_calls_and_refuses_unparsable_names() {
        let (host, mut plugin, doc) = setup(CALLER.replace("findByJoinedAndTitle", "findByNothing").as_str());
        let text = CALLER.replace("findByJoinedAndTitle", "findByNothing");
        let start = text.find("findByNothing").unwrap();
        let selection = Range::new(position(&text, start), position(&text, start + 3));
        let doc = Document { text: text.clone(), ..doc };
        let pending = plugin.file_actions(&doc, selection).into_iter().find(|action| action.title == CREATE_METHOD_TITLE).expect("offered");
        host.answer_next_prompt(Some(&[]));
        assert!(format!("{:?}", plugin.resolve_file_action(&doc, pending).unwrap_err()).contains("not a derived query"));
        let declared = text.replace("findByNothing", "findByTitle");
        let doc = Document { text: declared.clone(), ..doc };
        let start = declared.find("findByTitle").unwrap();
        let pending = plugin.file_actions(&doc, Range::new(position(&declared, start), position(&declared, start + 3))).into_iter().find(|action| action.title == CREATE_METHOD_TITLE);
        host.open_editor(PATH, "java", &declared, &[]);
        if let Some(pending) = pending {
            assert!(format!("{:?}", plugin.resolve_file_action(&doc, pending).unwrap_err()).contains("already declares"));
        }
    }
}
