//! Caret on a `mapTo...` call in a Spring bean whose receiver has a declared type: create a MapStruct mapper for the call
//! ("from new Mapper bean"). The mapper is a new file beside the source type (`overwrite: false`); the calling document gets the
//! mapper call in place of the `mapTo...` text and the mapper injection in the configured style. The eligibility check reads only
//! the invoking document; the source and target types are located when the action is resolved.
use super::*;
use super::call_assist::{self, Receiver};
use super::injection_action::Repo;
use crate::model::{Edit, closing, decapitalize, is_ident};
use openidea_plugin::text::offset;

pub(super) const TITLE: &str = "Create mapper for this call…";
const RESERVED: [&str; 5] = ["Mapper", "Mapping", "Mappings", "MappingTarget", "ReportingPolicy"];
const OPTIONS: [(&str, &str); 2] = [
    ("collections", "List variant of the mapping method"),
    ("ignoreUnmapped", "unmappedTargetPolicy = ReportingPolicy.IGNORE"),
];

struct Call {
    receiver: Receiver,
    source: String,
    collection: bool,
    typed: String,
}

fn call_under(doc: &Document, selection: Range) -> Option<Call> {
    if doc.language != "java" || doc.text.len() > MAX_TEXT || !call_assist::spring_component(doc) {
        return None;
    }
    let receiver = call_assist::receiver(&doc.text, offset(&doc.text, selection.start)?)?;
    let typed = doc.text[receiver.word_start..receiver.word_end].to_owned();
    if !typed.starts_with("mapTo") || !typed[5..].bytes().all(is_ident) {
        return None;
    }
    let (source, collection) = call_assist::declared_type(&doc.text, &receiver.name, false)?;
    Some(Call { receiver, source, collection, typed })
}

pub(super) fn eligible(doc: &Document, selection: Range) -> bool {
    call_under(doc, selection).is_some()
}

struct Located {
    directory: String,
    package: String,
}

fn locate(project: &Project, root: &str, name: &str) -> Result<Located, Error> {
    let mut found = Vec::new();
    for file in project.find_files(&format!("**/{name}.java"), 4)? {
        let file = unresolved_create::absolute(root, file);
        if !inside(root, &file) || path(&file).as_deref() != Some(file.as_str()) {
            continue;
        }
        let text = project.read_file(&file)?;
        let document = Document { path: file.clone(), language: "java".into(), version: 0, text };
        if document.text.len() <= MAX_TEXT && parse_class_name(&document) {
            found.push((file, package(&document)));
        }
    }
    match found.as_slice() {
        [(file, Some(package))] => Ok(Located { directory: file.rsplit_once('/').ok_or(Error::Stale)?.0.to_owned(), package: package.clone() }),
        [] => Err(format!("No class named `{name}` was found in this project; create it first.").into()),
        _ => Err(format!("`{name}` is not one unambiguous class with a package in this project; no guess is made.").into()),
    }
}

fn parse_class_name(doc: &Document) -> bool {
    let masked = mask(&doc.text);
    words(&masked, "class").next().is_some() || words(&masked, "record").next().is_some() || words(&masked, "interface").next().is_some()
}

fn render(package: &str, mapper: &str, source: (&str, &str), target: (&str, &str), options: &[String]) -> String {
    let collections = options.iter().any(|option| option == "collections");
    let ignore = options.iter().any(|option| option == "ignoreUnmapped");
    let mut imports = vec!["org.mapstruct.Mapper".to_owned()];
    for (name, other) in [source, target] {
        if other != package {
            imports.push(format!("{other}.{name}"));
        }
    }
    if collections {
        imports.push("java.util.List".into());
    }
    if ignore {
        imports.push("org.mapstruct.ReportingPolicy".into());
    }
    imports.sort();
    imports.dedup();
    let annotation = if ignore { "@Mapper(componentModel = \"spring\", unmappedTargetPolicy = ReportingPolicy.IGNORE)" } else { "@Mapper(componentModel = \"spring\")" };
    let (source, target) = (source.0, target.0);
    let mut methods = vec![format!("    {target} to{target}({source} source);")];
    if collections {
        methods.push(format!("    List<{target}> to{target}s(List<{source}> sources);"));
    }
    let imports: Vec<String> = imports.iter().map(|import| format!("import {import};")).collect();
    format!("package {package};\n\n{}\n\n{annotation}\npublic interface {mapper} {{\n{}\n}}\n", imports.join("\n"), methods.join("\n\n"))
}

fn ask_type(prompt: &str, default: &str) -> Result<String, Error> {
    let answer = host::input(prompt, Some(default))?.ok_or(Error::Cancelled)?;
    let answer = answer.trim().to_owned();
    if !type_name_ok(&answer) {
        return Err(format!("`{answer}` is not a usable type name.").into());
    }
    Ok(answer)
}

fn type_name_ok(name: &str) -> bool {
    identifier(name) && name.chars().next().is_some_and(|ch| ch.is_ascii_uppercase()) && !RESERVED.contains(&name)
}

fn call_end(doc: &Document, call: &Call) -> Result<usize, Error> {
    let rest = &doc.text[call.receiver.word_end..];
    let open = call.receiver.word_end + rest.len() - rest.trim_start().len();
    if !doc.text[open..].starts_with('(') {
        return Ok(call.receiver.word_end);
    }
    closing(mask(&doc.text).as_bytes(), open).map(|close| close + 1).ok_or(Error::Stale)
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let call = call_under(doc, pending.selection).ok_or(Error::Stale)?;
    let root = &pending.root;
    let source = locate(project, root, &call.source)?;

    let stem = call.typed.strip_prefix("mapTo").filter(|stem| type_name_ok(stem)).map(str::to_owned);
    let target_name = ask_type("Type the call maps to", &stem.unwrap_or_else(|| format!("{}Dto", call.source)))?;
    if target_name == call.source {
        return Err("The mapper must map to a different type than the receiver's.".into());
    }
    let target = locate(project, root, &target_name)?;
    mapper_action::build_proof(project, root, &source.directory, false)?;

    let default_mapper = format!("{}Mapper", if call.source.ends_with("Dto") { &target_name } else { &call.source });
    let mapper = ask_type("Mapper type name (next to the receiver's type)", &default_mapper)?;
    if mapper == call.source || mapper == target_name {
        return Err("Use a mapper name that differs from the mapped types.".into());
    }
    no_collision(project, root, &source.directory, &mapper, None)?;

    let items: Vec<_> = OPTIONS.iter().map(|(key, label)| PickItem::new(key, label)).collect();
    let options = host::pick_many("Mapper options (the mapping method is always generated)", &items)?.ok_or(Error::Cancelled)?;
    let mut seen = HashSet::new();
    if options.iter().any(|option| !OPTIONS.iter().any(|(key, _)| key == option) || !seen.insert(option.as_str())) {
        return Err("Choose only offered options, each once.".into());
    }

    let text = render(&source.package, &mapper, (&call.source, &source.package), (&target_name, &target.package), &options);
    let text = if doc.text.contains("\r\n") { text.replace('\n', "\r\n") } else { text };
    let variable = decapitalize(&mapper);
    let method = format!("to{target_name}");
    let receiver = &call.receiver.name;
    let (replacement, imports) = if call.collection {
        (format!("{receiver}.stream().map({variable}::{method}).collect(Collectors.toList())"), vec!["java.util.stream.Collectors"])
    } else {
        (format!("{variable}.{method}({receiver})"), Vec::new())
    };
    let mut edits = vec![Edit { start: call.receiver.start, end: call_end(doc, &call)?, text: replacement }];
    edits.extend(crate::model::import_edit(&doc.text, &imports, false));
    let mut edits: Vec<TextEdit> = edits.iter().map(|edit| edit.to_text_edit(&doc.text)).collect();
    let bean = Repo { name: mapper.clone(), entity: String::new(), package: source.package.clone() };
    let injection = call_assist::bean_edits(doc, project, root, &bean).ok_or("The mapper cannot be injected into this class in the detected style; nothing is created.")?;
    edits.extend(injection);
    let after = patched(&doc.text, &edits)?;
    let target_file = format!("{}/{mapper}.java", source.directory);
    if path(&target_file).as_deref() != Some(target_file.as_str()) || !inside(root, &target_file) || text.len() > MAX_TEXT {
        return Err(Error::Stale);
    }

    if !ui_kit::prompts::confirm_exact(
        "Create mapper for this call?",
        &format!(
            "Call: {} (version {})\nNew mapper: {target_file}\n\n{text}\nThe calling class after the edit:\n{after}\nThe mapper is a new file (no existing file is overwritten); only the calling document is edited. MapStruct was found in the build files.",
            doc.path, doc.version
        ),
    )? {
        return Err(Error::Cancelled);
    }
    let project = current(pending, doc)?;
    no_collision(&project, root, &source.directory, &mapper, None)?;
    Ok(FileAction {
        title: format!("Create mapper {mapper}"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target_file.clone(), text, overwrite: false }],
        edits: vec![(doc.path.clone(), edits)],
        open: Some(target_file),
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, text::position};

    const POM: &str = "<project><dependencies><dependency><groupId>org.mapstruct</groupId><artifactId>mapstruct</artifactId></dependency></dependencies><build><annotationProcessorPaths><path><groupId>org.mapstruct</groupId><artifactId>mapstruct-processor</artifactId></path></annotationProcessorPaths></build></project>";
    const CUSTOMER: &str = "package shop.domain;\n\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n\n@Entity\npublic class Customer {\n    @Id\n    private Long id;\n\n    private String title;\n}\n";
    const DTO: &str = "package shop.web;\n\npublic record CustomerDto(Long id, String title) {}\n";
    const SERVICE: &str = "package shop.service;\n\nimport java.util.List;\nimport org.springframework.stereotype.Service;\nimport shop.domain.Customer;\n\n@Service\npublic class CustomerService {\n\n    Object one(Customer customer) {\n        return customer.mapToCustomerDto();\n    }\n\n    Object many(List<Customer> customers) {\n        return customers.mapTo;\n    }\n}\n";
    const PATH: &str = "/project/src/main/java/shop/service/CustomerService.java";

    fn setup(pom: Option<&str>) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/domain/Customer.java", CUSTOMER);
        host.add_file("src/main/java/shop/web/CustomerDto.java", DTO);
        host.add_file("src/main/java/shop/service/CustomerService.java", SERVICE);
        if let Some(pom) = pom {
            host.add_file("pom.xml", pom);
        }
        host.open_editor(PATH, "java", SERVICE, &[]);
        (host, JpaAssist::default(), Document { path: PATH.into(), language: "java".into(), version: 1, text: SERVICE.into() })
    }

    fn offered(plugin: &mut JpaAssist, doc: &Document, anchor: &str) -> Option<FileAction> {
        let at = doc.text.find(anchor).unwrap();
        plugin.file_actions(doc, Range::new(position(&doc.text, at), position(&doc.text, at))).into_iter().find(|action| action.title == TITLE)
    }

    #[test]
    fn offered_on_a_map_to_call_of_a_typed_receiver_in_a_spring_bean_only() {
        let (_host, mut plugin, doc) = setup(Some(POM));
        assert!(offered(&mut plugin, &doc, "mapToCustomerDto").is_some());
        assert!(offered(&mut plugin, &doc, "mapTo;").is_some());
        assert!(offered(&mut plugin, &doc, "customer.map").is_none());
        let plain = SERVICE.replace("@Service\n", "");
        let (_host, mut plugin, doc) = setup(Some(POM));
        let doc = Document { text: plain, ..doc };
        assert!(offered(&mut plugin, &doc, "mapToCustomerDto").is_none());
    }

    #[test]
    fn creates_the_mapper_beside_the_source_and_edits_the_call_with_the_injection() {
        let (host, mut plugin, doc) = setup(Some(POM));
        let pending = offered(&mut plugin, &doc, "mapToCustomerDto").unwrap();
        assert!(pending.operations.is_empty() && pending.edits.is_empty());
        for answer in [&["CustomerDto"] as &[&str], &["CustomerMapper"], &["collections", "ignoreUnmapped"], &[]] {
            host.answer_next_prompt(Some(answer));
        }
        let result = plugin.resolve_file_action(&doc, pending).unwrap();
        let [FileOperation::Create { path, text, overwrite: false }] = result.operations.as_slice() else { panic!("one non-overwriting create") };
        assert_eq!(path, "/project/src/main/java/shop/domain/CustomerMapper.java");
        for expected in [
            "package shop.domain;",
            "import shop.web.CustomerDto;",
            "@Mapper(componentModel = \"spring\", unmappedTargetPolicy = ReportingPolicy.IGNORE)",
            "CustomerDto toCustomerDto(Customer source);",
            "List<CustomerDto> toCustomerDtos(List<Customer> sources);",
        ] {
            assert!(text.contains(expected), "{expected}\n{text}");
        }
        let [(edited, edits)] = result.edits.as_slice() else { panic!("only the caller is edited") };
        assert_eq!(edited, PATH);
        let after = patched(SERVICE, edits).unwrap();
        assert!(after.contains("return customerMapper.toCustomerDto(customer);"), "{after}");
        assert!(after.contains("private final CustomerMapper customerMapper;") && after.contains("import shop.domain.CustomerMapper;"), "{after}");
        assert_eq!(host.editor_text(), SERVICE);
    }

    #[test]
    fn a_collection_receiver_uses_the_stream_form_without_options() {
        let (host, mut plugin, doc) = setup(Some(POM));
        let pending = offered(&mut plugin, &doc, "mapTo;").unwrap();
        for answer in [&["CustomerDto"] as &[&str], &["CustomerMapper"], &[], &[]] {
            host.answer_next_prompt(Some(answer));
        }
        let result = plugin.resolve_file_action(&doc, pending).unwrap();
        let after = patched(SERVICE, &result.edits[0].1).unwrap();
        assert!(after.contains("return customers.stream().map(customerMapper::toCustomerDto).collect(Collectors.toList());"), "{after}");
        assert!(after.contains("import java.util.stream.Collectors;"), "{after}");
    }

    #[test]
    fn refuses_without_the_build_proof_a_missing_target_or_an_existing_mapper() {
        let (host, mut plugin, doc) = setup(None);
        let pending = offered(&mut plugin, &doc, "mapToCustomerDto").unwrap();
        host.answer_next_prompt(Some(&["CustomerDto"]));
        assert!(format!("{:?}", plugin.resolve_file_action(&doc, pending).unwrap_err()).contains("MapStruct"));

        let (host, mut plugin, doc) = setup(Some(POM));
        let pending = offered(&mut plugin, &doc, "mapToCustomerDto").unwrap();
        host.answer_next_prompt(Some(&["Missing"]));
        assert!(format!("{:?}", plugin.resolve_file_action(&doc, pending).unwrap_err()).contains("No class named"));

        let (host, mut plugin, doc) = setup(Some(POM));
        host.add_file("src/main/java/shop/domain/CustomerMapper.java", "package shop.domain;\npublic interface CustomerMapper {}\n");
        let pending = offered(&mut plugin, &doc, "mapToCustomerDto").unwrap();
        host.answer_next_prompt(Some(&["CustomerDto"]));
        host.answer_next_prompt(Some(&["CustomerMapper"]));
        assert!(format!("{:?}", plugin.resolve_file_action(&doc, pending).unwrap_err()).contains("already exists"));
    }

    #[test]
    #[ignore = "requires javac, JPA_JAKARTA_API, MAPSTRUCT_JAR and MAPSTRUCT_PROCESSOR_JAR (source scripts/jpa-compile-env.sh); compiles and processes the mapper and the edited caller"]
    fn the_created_mapper_and_the_edited_caller_compile_and_process_with_mapstruct() {
        let api = std::env::var("JPA_JAKARTA_API").expect("explicit API jar path");
        let jar = std::env::var("MAPSTRUCT_JAR").expect("mapstruct jar");
        let processor = std::env::var("MAPSTRUCT_PROCESSOR_JAR").expect("mapstruct-processor jar");
        let (host, mut plugin, doc) = setup(Some(POM));
        let customer = CUSTOMER.replace("    private String title;\n}", "    private String title;\n\n    public Long getId() { return id; }\n    public void setId(Long id) { this.id = id; }\n    public String getTitle() { return title; }\n    public void setTitle(String title) { this.title = title; }\n}");
        let pending = offered(&mut plugin, &doc, "mapToCustomerDto").unwrap();
        for answer in [&["CustomerDto"] as &[&str], &["CustomerMapper"], &["collections"], &[]] {
            host.answer_next_prompt(Some(answer));
        }
        let result = plugin.resolve_file_action(&doc, pending).unwrap();
        let [FileOperation::Create { text: mapper, .. }] = result.operations.as_slice() else { panic!("one file") };
        let caller = patched(SERVICE, &result.edits[0].1).unwrap().replace("return customers.mapTo;", "return customers;");
        let files = [
            ("shop/domain/Customer.java", customer),
            ("shop/web/CustomerDto.java", DTO.to_owned()),
            ("shop/domain/CustomerMapper.java", mapper.clone()),
            ("shop/service/CustomerService.java", caller),
            ("org/springframework/stereotype/Service.java", "package org.springframework.stereotype; public @interface Service {}".into()),
            ("org/springframework/stereotype/Component.java", "package org.springframework.stereotype; public @interface Component {}".into()),
            ("org/springframework/beans/factory/annotation/Autowired.java", "package org.springframework.beans.factory.annotation; public @interface Autowired {}".into()),
        ];
        let dir = std::env::temp_dir().join(format!("jpa-call-mapper-{}", std::process::id()));
        let paths: Vec<_> = files
            .iter()
            .map(|(name, body)| {
                let path = dir.join(name);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, body).unwrap();
                path
            })
            .collect();
        let classpath = format!("{api}:{jar}");
        let result = std::process::Command::new("javac").args(["--release", "17", "-classpath", &classpath, "-processorpath", &processor, "-d"]).arg(&dir).args(&paths).output().expect("javac installed");
        assert!(result.status.success(), "{}\n{files:?}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
