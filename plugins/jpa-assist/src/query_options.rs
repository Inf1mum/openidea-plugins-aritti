//! The extra controls of the query window: the "+" that creates a new projection interface or DTO for
//! the result, the named-or-positional parameter choice and the independent `Pageable` switch.
//!
//! Named and positional bindings are `@Query` JPQL produced by the derived-to-`@Query` conversion
//! (`query_extract_action`), so its refusals apply: entity results only, no mixed And/Or, no
//! Containing/StartingWith/EndingWith. A positional query is that conversion with `:name` replaced by
//! `?N` in declaration order and the `@Param` annotations dropped.
use super::query_method_action::{Attr, Facts, Returns, Subject};
use super::*;
use projection_action::Form;
use regex_lite::Regex;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Binding {
    Derived,
    Named,
    Positional,
}

pub(super) struct Created {
    pub(super) name: String,
    pub(super) path: String,
    pub(super) text: String,
}

pub(super) struct Choices {
    pub(super) created: Option<Created>,
    pub(super) binding: Binding,
    pub(super) pageable: bool,
}

fn create_type(pending: &Pending, doc: &Document, project: &Project, class: &Class) -> Result<Created, Error> {
    let mut forms = vec![PickItem::new("projection", "New closed projection interface")];
    let dto_form = if class.kotlin { Some(Form::DataClass) } else { dto_action::record_offered(&doc.path).then_some(Form::Record) };
    if dto_form.is_some() {
        forms.push(PickItem::new("dto", if class.kotlin { "New DTO data class" } else { "New DTO record" }));
    }
    let kind = host::pick("New result type", &forms)?.ok_or(Error::Cancelled)?;
    let (form, default) = match (kind.as_str(), dto_form) {
        ("projection", _) => (Form::Interface, projection_links::Conventions::load()?.default_name(&class.name)),
        ("dto", Some(form)) => (form, format!("{}Dto", class.name)),
        _ => return Err("Choose one of the offered result types.".into()),
    };
    let name = host::input("New result type name (same package as the repository)", Some(&default))?.ok_or(Error::Cancelled)?.trim().to_owned();
    let package_name = package(doc).ok_or(Error::Stale)?;
    if !identifier(&name) || name == class.name || class.imports.iter().any(|import| import.rsplit('.').next() == Some(name.as_str())) {
        return Err("Use a non-conflicting Java/Kotlin identifier for the new result type.".into());
    }
    let available = projection_action::offered(class, form);
    if available.is_empty() {
        return Err("No plain scalar attribute has a name usable as a property of the new type.".into());
    }
    let items: Vec<_> = available.iter().map(|(n, t)| PickItem::new(n, n).description(t)).collect();
    let chosen = host::pick_many("Properties of the new type (plain scalar attributes only)", &items)?.ok_or(Error::Cancelled)?;
    let mut seen = HashSet::new();
    if chosen.is_empty() || chosen.iter().any(|n| !available.iter().any(|(f, _)| f == n) || !seen.insert(n.as_str())) {
        return Err("Choose one or more offered attributes exactly once.".into());
    }
    let fields: Vec<_> = available.iter().filter(|(n, _)| chosen.contains(n)).cloned().collect();
    let conventions = projection_links::Conventions::load()?;
    let mut text = conventions.with_comment(&projection_action::render(class, &package_name, form, &name, &fields), class.package.as_deref(), &class.name);
    if doc.text.contains("\r\n") {
        text = text.replace('\n', "\r\n");
    }
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let path = format!("{directory}/{name}.{}", if class.kotlin { "kt" } else { "java" });
    if super::path(&path).as_deref() != Some(path.as_str()) || !inside(&pending.root, &path) {
        return Err(Error::Stale);
    }
    no_collision(project, &pending.root, directory, &name, None)?;
    Ok(Created { name, path, text })
}

pub(super) fn ask(pending: &Pending, doc: &Document, project: &Project, class: &Class, subject: Subject, returns: Returns) -> Result<Choices, Error> {
    let mut created = None;
    if subject == Subject::Find {
        let results = [PickItem::new("entity", "The entity"), PickItem::new("new", "A new projection interface or DTO (+)")];
        if host::pick("Result element", &results)?.ok_or(Error::Cancelled)? == "new" {
            created = Some(create_type(pending, doc, project, class)?);
        }
    }
    let bindings = [
        PickItem::new("derived", "Derived method (parameters follow the method name)"),
        PickItem::new("named", "@Query with named parameters (:name and @Param)"),
        PickItem::new("positional", "@Query with positional parameters (?1, ?2)"),
    ];
    let binding = match host::pick("Parameter binding", &bindings)?.ok_or(Error::Cancelled)?.as_str() {
        "derived" => Binding::Derived,
        "named" => Binding::Named,
        "positional" => Binding::Positional,
        _ => return Err("Choose one of the offered bindings.".into()),
    };
    if binding != Binding::Derived && created.is_some() {
        return Err("Named and positional @Query parameters are generated for entity results only; choose a derived method for a new projection or DTO.".into());
    }
    let mut pageable = false;
    if subject == Subject::Find && matches!(returns, Returns::List | Returns::Stream) {
        let choices = [PickItem::new("no", "No Pageable parameter"), PickItem::new("yes", "Add a Pageable parameter")];
        pageable = host::pick("Pageable parameter", &choices)?.ok_or(Error::Cancelled)? == "yes";
    }
    Ok(Choices { created, binding, pageable })
}

fn single_edit(old: &str, new: &str) -> Vec<TextEdit> {
    let prefix = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
    let mut prefix = prefix;
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let limit = old.len().min(new.len()) - prefix;
    let mut suffix = old.bytes().rev().zip(new.bytes().rev()).take(limit).take_while(|(a, b)| a == b).count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    vec![crate::model::Edit { start: prefix, end: old.len() - suffix, text: new[prefix..new.len() - suffix].to_owned() }.to_text_edit(old)]
}

pub(super) fn bind_parameters(
    doc: &Document,
    facts: &Facts,
    attrs: &[Attr],
    entity_text: &str,
    derived: &str,
    method: &str,
    binding: Binding,
) -> Result<(Vec<TextEdit>, String), Error> {
    let staged = Document { text: derived.to_owned(), ..doc.clone() };
    let plan = query_extract_action::plan(&staged, facts, attrs, entity_text, method)?;
    index_query_action::query_sanity(&plan.query)?;
    let edits = query_extract_action::edits(&staged, facts, &plan, method)?;
    let mut text = patched(derived, &edits)?;
    if binding == Binding::Positional {
        let names: Vec<&str> = plan.params.iter().map(|param| param.name.as_str()).collect();
        let call = text.find(&format!(" {method}(")).ok_or(Error::Stale)?;
        let start = text[..call].rfind("@Query(").ok_or(Error::Stale)?;
        let end = call + text[call..].find(['\n', ';']).unwrap_or(text.len() - call);
        let mut region = text[start..end].to_owned();
        for name in &names {
            region = region.replace(&format!("@Param(\"{name}\") "), "");
        }
        let replaced = Regex::new(r":(\w+)").unwrap().replace_all(&region, |caps: &regex_lite::Captures<'_>| {
            match names.iter().position(|name| *name == &caps[1]) {
                Some(at) => format!("?{}", at + 1),
                None => caps[0].to_owned(),
            }
        });
        text = format!("{}{replaced}{}", &text[..start], &text[end..]);
        if !text.contains("@Param(") {
            let kept: Vec<_> = text.split_inclusive('\n').filter(|line| !line.trim().starts_with("import org.springframework.data.repository.query.Param")).collect();
            text = kept.concat();
        }
    }
    Ok((single_edit(&doc.text, &text), text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ENTITY: &str = "package shop;\nimport jakarta.persistence.*;\nimport java.time.LocalDate;\n@Entity\npublic class Order {\n    @Id Long id;\n    String number;\n    int age;\n    LocalDate placedOn;\n}\n";
    const REPO: &str = "package shop;\n\nimport org.springframework.data.jpa.repository.JpaRepository;\n\npublic interface OrderRepository extends JpaRepository<Order, Long> {\n}\n";
    const PATH: &str = "/project/src/main/java/shop/OrderRepository.java";

    fn run(answers: &[&[&str]]) -> Result<FileAction, Error> {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/Order.java", ENTITY);
        host.open_editor(PATH, "java", REPO, &[]);
        let doc = Document { path: PATH.into(), language: "java".into(), text: REPO.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == query_method_action::OPTIONS_TITLE).expect("offered");
        for answer in answers {
            host.answer_next_prompt(Some(answer));
        }
        host.answer_next_prompt(Some(&[]));
        plugin.resolve_file_action(&doc, pending)
    }

    fn after(action: &FileAction) -> String {
        let [(_, edits)] = action.edits.as_slice() else { panic!("one document") };
        patched(REPO, edits).unwrap()
    }

    #[test]
    fn new_projection_is_created_and_returned() {
        let action = run(&[&["find"], &["number"], &["is"], &["none"], &[""], &["list"], &["new"], &["projection"], &["OrderNumbers"], &["id", "number"], &["derived"], &["no"]]);
        let action = action.unwrap_or_else(|e| panic!("{e:?}"));
        assert!(after(&action).contains("List<OrderNumbers> findByNumber(String number);"));
        let [FileOperation::Create { path, text, overwrite: false }] = action.operations.as_slice() else { panic!("one create") };
        assert!(path.ends_with("shop/OrderNumbers.java"));
        assert!(text.contains("public interface OrderNumbers {\n    Long getId();\n    String getNumber();\n}"));
    }

    fn flow(binding: &str, pageable: Option<&str>, returns: &str, conditions: &[&str], operators: &[&str]) -> Result<FileAction, Error> {
        let mut answers: Vec<Vec<&str>> = vec![vec!["find"], conditions.to_vec()];
        answers.extend(operators.iter().map(|operator| vec![*operator]));
        if conditions.len() > 1 {
            answers.push(vec!["and"]);
        }
        answers.extend([vec!["none"], vec![""], vec![returns], vec!["entity"], vec![binding]]);
        if let Some(pageable) = pageable {
            answers.push(vec![pageable]);
        }
        let refs: Vec<&[&str]> = answers.iter().map(Vec::as_slice).collect();
        run(&refs)
    }

    #[test]
    fn named_and_positional_bindings_generate_query_parameters() {
        let named = after(&flow("named", Some("no"), "list", &["number", "age"], &["like", "greaterThan"]).unwrap_or_else(|e| panic!("{e:?}")));
        assert!(named.contains("@Query(\"select e from Order e where e.number like :number and e.age > :age\")"), "{named}");
        assert!(named.contains("@Param(\"number\") String number"), "{named}");
        let positional = after(&flow("positional", Some("no"), "list", &["number", "age"], &["like", "greaterThan"]).unwrap_or_else(|e| panic!("{e:?}")));
        assert!(positional.contains("@Query(\"select e from Order e where e.number like ?1 and e.age > ?2\")"), "{positional}");
        assert!(!positional.contains("@Param") && !positional.contains("query.Param"), "{positional}");
        assert!(positional.contains("String number, int age"), "{positional}");
    }

    #[test]
    fn pageable_is_independent_of_the_page_wrapper() {
        let with = after(&flow("derived", Some("yes"), "list", &["number"], &["is"]).unwrap());
        assert!(with.contains("List<Order> findByNumber(String number, Pageable pageable);"), "{with}");
        assert!(with.contains("import org.springframework.data.domain.Pageable;"));
        let without = after(&flow("derived", Some("no"), "list", &["number"], &["is"]).unwrap());
        assert!(without.contains("List<Order> findByNumber(String number);"));
        let page = after(&flow("derived", None, "page", &["number"], &["is"]).unwrap());
        assert!(page.contains("Page<Order> findByNumber(String number, Pageable pageable);"));
    }

    #[test]
    fn named_binding_with_a_new_projection_and_pageable_with_optional_are_refused() {
        let refused = run(&[&["find"], &["number"], &["is"], &["none"], &[""], &["list"], &["new"], &["projection"], &["V"], &["id"], &["named"]]);
        assert!(matches!(refused, Err(Error::Failed(ref m)) if m.contains("entity results only")), "{refused:?}");
        let optional = run(&[&["find"], &["number"], &["is"], &["none"], &[""], &["optional"], &["entity"], &["derived"]]);
        assert!(optional.is_ok());
    }

    #[test]
    fn new_dto_record_is_created_with_the_chosen_properties() {
        let action = run(&[&["find"], &["age"], &["is"], &["none"], &[""], &["single"], &["new"], &["dto"], &["OrderDto"], &["number", "age"], &["derived"]]).unwrap_or_else(|e| panic!("{e:?}"));
        let [FileOperation::Create { text, .. }] = action.operations.as_slice() else { panic!("one create") };
        assert!(text.contains("public record OrderDto("), "{text}");
        assert!(after(&action).contains("OrderDto findByAge(int age);"));
    }

    #[test]
    fn a_minimal_edit_reproduces_the_new_text() {
        let old = "a\nbcd\ne\n";
        let new = "a\nbXYd\ne\n";
        let edits = single_edit(old, new);
        assert_eq!(patched(old, &edits).unwrap(), new);
        let unicode = single_edit("é1", "é22");
        assert_eq!(patched("é1", &unicode).unwrap(), "é22");
    }

    #[test]
    #[ignore = "requires javac and SPRING_DATA_CLASSPATH"]
    fn generated_option_outputs_compile_against_spring_data() {
        let classpath = std::env::var("SPRING_DATA_CLASSPATH").expect("explicit classpath");
        let base = std::env::temp_dir().join(format!("jpa-options-compile-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("Order.java"), "package shop;\npublic class Order {\n    Long id;\n    String number;\n    int age;\n}\n").unwrap();
        let mut files = vec![base.join("Order.java")];
        let cases: Vec<(&str, FileAction)> = vec![
            ("RNamed", flow("named", Some("no"), "list", &["number", "age"], &["like", "greaterThan"]).unwrap()),
            ("RPositional", flow("positional", Some("yes"), "list", &["number", "age"], &["like", "greaterThan"]).unwrap()),
            ("RPage", flow("derived", None, "page", &["number"], &["is"]).unwrap()),
            ("RProjection", run(&[&["find"], &["number"], &["is"], &["none"], &[""], &["list"], &["new"], &["projection"], &["OrderNumbers"], &["id", "number"], &["derived"], &["no"]]).unwrap()),
            ("RDto", run(&[&["find"], &["age"], &["is"], &["none"], &[""], &["single"], &["new"], &["dto"], &["OrderDto"], &["number", "age"], &["derived"]]).unwrap()),
        ];
        for (name, action) in cases {
            let file = base.join(format!("{name}.java"));
            std::fs::write(&file, after(&action).replace("OrderRepository", name)).unwrap();
            files.push(file);
            for operation in &action.operations {
                let FileOperation::Create { path, text, .. } = operation else { continue };
                let target = base.join(path.rsplit('/').next().unwrap());
                std::fs::write(&target, text).unwrap();
                files.push(target);
            }
        }
        let result = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &classpath, "-d"]).arg(&base).args(&files).output().expect("javac installed");
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(base).unwrap();
    }
}
