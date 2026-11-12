//! Two more call-site completions in a Spring bean (children of `call_assist`):
//! - `repository.find…` on a repository INSTANCE offers its existing methods and the derived methods it lacks; the
//!   missing ones are created afterwards with "Create repository method from this call…".
//! - `entity.mapTo…` offers a ModelMapper `map(source, Target.class)` call and injects the `ModelMapper` bean when
//!   `org.modelmapper:modelmapper` is written literally in the build files. The target is the DTO named after the
//!   entity by the DTO conventions, or the entity a DTO is named after. Java only.
use super::*;
use crate::file_actions::projection_links::{Conventions, Kind as Family};

pub(super) fn instance_calls(doc: &Document, project: &Project, root: &str, receiver: &Receiver, typed: &str) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    if typed.len() >= 2 {
        items.extend(repository_instance(doc, project, root, receiver, typed));
    }
    if typed.len() >= 3 && ("mapto".starts_with(typed) || typed.starts_with("mapto")) {
        items.extend(model_mapper_calls(doc, project, root, receiver, typed));
    }
    items
}

fn repository_instance(doc: &Document, project: &Project, root: &str, receiver: &Receiver, typed: &str) -> Vec<CompletionItem> {
    let kotlin = doc.language == "kotlin";
    let Some((ty, false)) = declared_type(&doc.text, &receiver.name, kotlin) else {
        return vec![];
    };
    if !ty.ends_with("Repository") || ty == "Repository" {
        return vec![];
    }
    let Ok((path, text)) = repository_source(project, root, &ty) else {
        return vec![];
    };
    let repository_kotlin = path.ends_with(".kt");
    let Some(repository) = queries::parse_repository(&text, repository_kotlin) else {
        return vec![];
    };
    let Ok(entity_doc) = entity_document(project, root, &repository.entity, repository_kotlin) else {
        return vec![];
    };
    let Some(entity) = current_class(&entity_doc) else {
        return vec![];
    };
    let mut items = Vec::new();
    let mut declared: Vec<&str> = Vec::new();
    for method in repository.methods.iter().filter(|method| !method.declared_query) {
        declared.push(&method.name);
        let names = parameter_names(&text[method.start..], repository_kotlin);
        items.push(call_item(doc, receiver, format!("{}()", method.name), format!("{ty} (existing)"), snippet(&receiver.name, &method.name, &names), &[], &[], kotlin));
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
            items.push(call_item(doc, receiver, format!("{name}()"), format!("{ty} (new: add the method with the quick fix)"), snippet(&receiver.name, &name, &names), &[], &[], kotlin));
        }
    }
    items.retain(|item| item.label.to_lowercase().starts_with(typed));
    items.truncate(MAX_ITEMS);
    items
}

struct Mapped {
    name: String,
    package: Option<String>,
}

fn type_package(project: &Project, root: &str, name: &str) -> Option<Option<String>> {
    let (_, text) = repository_source(project, root, name).ok()?;
    Some(package_of_source(&text))
}

fn mapped_type(project: &Project, root: &str, ty: &str) -> Option<Mapped> {
    let conventions = Conventions::load_for(Family::Dto).ok()?;
    if entity_document(project, root, ty, false).is_ok() {
        let name = conventions.default_name(ty);
        let package = type_package(project, root, &name)?;
        return Some(Mapped { name, package });
    }
    let (_, text) = repository_source(project, root, ty).ok()?;
    let entity = conventions.entity_of(ty, &text)?;
    let entity_doc = entity_document(project, root, &entity, false).ok()?;
    Some(Mapped { name: entity, package: package_of_source(&entity_doc.text) })
}

fn model_mapper_calls(doc: &Document, project: &Project, root: &str, receiver: &Receiver, typed: &str) -> Vec<CompletionItem> {
    if doc.language != "java" {
        return vec![];
    }
    let Some((ty, collection)) = declared_type(&doc.text, &receiver.name, false) else {
        return vec![];
    };
    let Some(target) = mapped_type(project, root, &ty) else {
        return vec![];
    };
    if modelmapper_action::build_proof(project, root, doc, "org.modelmapper", &["modelmapper"], "ModelMapper").is_err() {
        return vec![];
    }
    let bean = Repo { name: "ModelMapper".into(), entity: String::new(), package: "org.modelmapper".into() };
    let Some(edits) = bean_edits(doc, project, root, &bean) else {
        return vec![];
    };
    let mapper = decapitalize(&bean.name);
    let call = if collection {
        let element = if receiver.name == "element" { "item" } else { "element" };
        format!("{}.stream().map({element} -> {mapper}.map({element}, {}.class)).collect(Collectors.toList())", receiver.name, target.name)
    } else {
        format!("{mapper}.map({}, {}.class)", receiver.name, target.name)
    };
    let own_package = package_of_source(&doc.text);
    let mut imports: Vec<String> = Vec::new();
    if let Some(package) = target.package.as_ref().filter(|package| Some(*package) != own_package.as_ref()) {
        imports.push(format!("{package}.{}", target.name));
    }
    if collection {
        imports.push("java.util.stream.Collectors".into());
    }
    let imports: Vec<&str> = imports.iter().map(String::as_str).collect();
    let label = format!("mapTo{}", target.name);
    let mut items = vec![call_item(doc, receiver, label, "from ModelMapper (injects ModelMapper)".into(), call, &edits, &imports, false)];
    items.retain(|item| item.label.to_lowercase().starts_with(typed));
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{diagnostics::DiagnosticsProvider, mock::MockHost, providers::Providers};

    const ENTITY: &str = "package shop.domain;\n\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport java.time.LocalDate;\n\n@Entity\npublic class Customer {\n    @Id\n    private Long id;\n\n    private String title;\n\n    private LocalDate joined;\n}\n";
    const REPOSITORY: &str = "package shop.domain;\n\nimport java.util.List;\nimport org.springframework.data.jpa.repository.JpaRepository;\n\npublic interface CustomerRepository extends JpaRepository<Customer, Long> {\n    List<Customer> findByTitle(String title);\n}\n";
    const DTO: &str = "package shop.web;\n\npublic record CustomerDto(Long id, String title) {}\n";
    const POM: &str = "<project><dependencies><dependency><groupId>org.modelmapper</groupId><artifactId>modelmapper</artifactId><version>3.2.4</version></dependency></dependencies></project>";
    const SERVICE: &str = "package shop.service;\n\nimport java.util.List;\nimport org.springframework.stereotype.Service;\nimport shop.domain.Customer;\nimport shop.domain.CustomerRepository;\n\n@Service\npublic class CustomerService {\n    private final CustomerRepository customerRepository;\n\n    void run(Customer customer, List<Customer> customers) {\n        customerRepository.find\n        customer.mapTo\n        customers.mapTo\n    }\n}\n";
    const PATH: &str = "/project/src/main/java/shop/service/CustomerService.java";

    fn setup(service: &str, with_modelmapper: bool) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/domain/Customer.java", ENTITY);
        host.add_file("src/main/java/shop/domain/CustomerRepository.java", REPOSITORY);
        host.add_file("src/main/java/shop/web/CustomerDto.java", DTO);
        host.add_file("src/main/java/shop/service/CustomerService.java", service);
        if with_modelmapper {
            host.add_file("pom.xml", POM);
        }
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
    fn a_repository_instance_offers_existing_and_missing_derived_methods_without_touching_the_bean() {
        let (_host, mut plugin, doc) = setup(SERVICE, false);
        let items = complete(&mut plugin, &doc, "customerRepository.find");
        let existing = items.iter().find(|item| item.label == "findByTitle()").expect("existing");
        assert_eq!(existing.insert_text, "customerRepository.findByTitle(${1:title})");
        assert!(existing.detail.as_deref().unwrap().contains("existing") && existing.additional_edits.is_empty());
        let created = items.iter().find(|item| item.label == "findByJoined()").expect("missing");
        assert_eq!(created.insert_text, "customerRepository.findByJoined(${1:joined})");
        assert!(created.detail.as_deref().unwrap().contains("quick fix"));
        assert!(items.iter().all(|item| item.label.starts_with("find")));
        assert_eq!(existing.replace.unwrap().start, position(&doc.text, doc.text.find("customerRepository.find").unwrap()));
    }

    #[test]
    fn an_unknown_repository_type_or_a_non_repository_variable_stays_silent() {
        let unknown = SERVICE.replace("CustomerRepository customerRepository", "OrderRepository customerRepository");
        let (_host, mut plugin, doc) = setup(&unknown, false);
        assert!(complete(&mut plugin, &doc, "customerRepository.find").is_empty());
        let plain = SERVICE.replace("private final CustomerRepository customerRepository;", "private final Customer customerRepository;");
        let (_host, mut plugin, doc) = setup(&plain, false);
        assert!(complete(&mut plugin, &doc, "customerRepository.find").is_empty());
    }

    #[test]
    fn map_to_offers_a_model_mapper_call_for_an_entity_and_for_a_collection_and_injects_the_bean() {
        let (_host, mut plugin, doc) = setup(SERVICE, true);
        let single = complete(&mut plugin, &doc, "customer.mapTo");
        let item = single.iter().find(|item| item.label == "mapToCustomerDto").expect("single");
        assert_eq!(item.insert_text, "modelMapper.map(customer, CustomerDto.class)");
        let after = patched(SERVICE, &item.additional_edits).unwrap();
        assert!(after.contains("private final ModelMapper modelMapper;") && after.contains("import org.modelmapper.ModelMapper;") && after.contains("import shop.web.CustomerDto;"), "{after}");
        let many = complete(&mut plugin, &doc, "customers.mapTo");
        let item = many.iter().find(|item| item.label == "mapToCustomerDto").expect("collection");
        assert_eq!(item.insert_text, "customers.stream().map(element -> modelMapper.map(element, CustomerDto.class)).collect(Collectors.toList())");
        assert!(patched(SERVICE, &item.additional_edits).unwrap().contains("import java.util.stream.Collectors;"));
    }

    #[test]
    fn map_to_without_a_provable_model_mapper_or_a_linked_type_stays_silent() {
        let (_host, mut plugin, doc) = setup(SERVICE, false);
        assert!(complete(&mut plugin, &doc, "customer.mapTo").is_empty());
        let (host, mut plugin, doc) = setup(SERVICE, true);
        host.add_file("src/main/java/shop/web/Other.java", "package shop.web;\npublic class Other {}\n");
        let other = SERVICE.replace("Customer customer,", "Other customer,");
        let doc = Document { text: other, ..doc };
        assert!(complete(&mut plugin, &doc, "customer.mapTo").is_empty());
    }

    #[test]
    fn a_dto_receiver_maps_back_to_its_entity() {
        let service = SERVICE.replace("Customer customer,", "CustomerDto customer,").replace("import shop.domain.Customer;", "import shop.web.CustomerDto;");
        let (_host, mut plugin, doc) = setup(&service, true);
        let items = complete(&mut plugin, &doc, "customer.mapTo");
        let item = items.iter().find(|item| item.label == "mapToCustomer").expect("entity target");
        assert_eq!(item.insert_text, "modelMapper.map(customer, Customer.class)");
        assert!(patched(&service, &item.additional_edits).unwrap().contains("import shop.domain.Customer;"));
    }

    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_API; compiles against hand-written ModelMapper/Spring stubs"]
    fn generated_model_mapper_calls_compile_against_stubs() {
        let api = std::env::var("JPA_JAKARTA_API").expect("explicit API jar path");
        let service = SERVICE
            .replace("        customerRepository.find\n        customer.mapTo\n        customers.mapTo\n", "        Object single = customer.mapTo;\n        Object many = customers.mapTo;\n")
            .replace("    private final CustomerRepository customerRepository;\n\n", "")
            .replace("import shop.domain.CustomerRepository;\n", "import java.util.stream.Collectors;\n");
        let (_host, mut plugin, doc) = setup(&service, true);
        let mut edits = Vec::new();
        for marker in ["customer.mapTo", "customers.mapTo"] {
            let items = complete(&mut plugin, &doc, marker);
            let item = items.iter().find(|item| item.label == "mapToCustomerDto").expect("offered");
            edits.push(TextEdit { range: item.replace.unwrap(), text: item.insert_text.clone() });
            if marker == "customer.mapTo" {
                edits.extend(item.additional_edits.clone());
            }
        }
        let compiled = patched(&service, &edits).unwrap();
        let files: Vec<(&str, String)> = vec![
            ("shop/domain/Customer.java", ENTITY.to_owned()),
            ("shop/web/CustomerDto.java", DTO.to_owned()),
            ("shop/service/CustomerService.java", compiled),
            ("org/modelmapper/ModelMapper.java", "package org.modelmapper; public class ModelMapper { public <D> D map(Object s, Class<D> d) { return null; } }".into()),
            ("org/springframework/stereotype/Service.java", "package org.springframework.stereotype; public @interface Service {}".into()),
            ("shop/domain/CustomerRepository.java", "package shop.domain; public interface CustomerRepository {}".into()),
        ];
        let dir = std::env::temp_dir().join(format!("jpa-call-more-{}", std::process::id()));
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
}
