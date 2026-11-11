//! Reviewed creation of (1) a Spring Data closed interface projection with nested interface
//! projections for to-one associations and (2) a flattened record/data class DTO plus a JPQL
//! constructor-expression snippet shown in the preview. One new sibling file each
//! (`overwrite: false`); no repository or existing file is edited.
//!
//! Verified against the Spring Data JPA projections reference: nested interface projections are
//! documented for a single association getter; collection-valued nested projections are not
//! documented, so to-many associations are never offered. Class-based DTOs "cannot apply nested
//! projections", hence the flattened variant. Projections limit the selection to top-level
//! properties; nested properties resolving to joins select the entire nested property.
//! Constructor expressions need an FQCN, an all-args constructor and no aliases.
use super::*;
use crate::model::Attribute;
use projection_action::Form;

pub(super) const TITLE: &str = "Create projection with nested association…";
pub(super) const FLAT_TITLE: &str = "Create flattened DTO…";

struct Nested {
    attribute: String,
    nested_name: String,
    target: Class,
    fields: Vec<(String, String)>,
}

struct Plan {
    class: Class,
    package_name: String,
    name: String,
    root_fields: Vec<(String, String)>,
    nested: Vec<Nested>,
}

fn to_one(class: &Class) -> Vec<&Attribute> {
    class
        .attributes
        .iter()
        .filter(|a| matches!(a.relation, Relation::ManyToOne | Relation::OneToOne) && identifier(&a.name) && identifier(&a.target))
        .collect()
}

pub(super) fn eligible(doc: &Document) -> bool {
    dto_action::source(doc).is_some_and(|class| !to_one(&class).is_empty() && dto_action::fields(&class).is_some())
}

fn capitalised(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map(|c| c.to_ascii_uppercase().to_string() + chars.as_str()).unwrap_or_default()
}

fn base(ty: &str) -> &str {
    ty.trim_end_matches('?').split('<').next().unwrap_or(ty).rsplit('.').next().unwrap_or(ty)
}

fn bean_safe(class: &Class, name: &str) -> bool {
    let mut chars = name.chars();
    let first = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    let second_upper = chars.next().is_some_and(|c| c.is_ascii_uppercase());
    let kotlin_is = class.kotlin && name.strip_prefix("is").and_then(|r| r.chars().next()).is_some_and(|c| c.is_ascii_uppercase());
    first && !second_upper && !kotlin_is
}

fn imports(plan: &Plan) -> Result<Vec<String>, Error> {
    let mut found: Vec<String> = plan.root_fields.iter().filter_map(|(_, ty)| plan.class.qualified(base(ty))).collect();
    for nested in &plan.nested {
        found.extend(nested.fields.iter().filter_map(|(_, ty)| nested.target.qualified(base(ty))));
    }
    found.sort();
    found.dedup();
    found.retain(|i| i.rsplit_once('.').map(|(pk, _)| pk) != Some(plan.package_name.as_str()));
    for pair in found.windows(2) {
        if pair[0].rsplit('.').next() == pair[1].rsplit('.').next() {
            return Err("Two picked fields use different types with the same simple name.".into());
        }
    }
    Ok(found)
}

fn header(plan: &Plan, imports: &[String]) -> String {
    let semicolon = if plan.class.kotlin { "" } else { ";" };
    let mut out = format!("package {}{semicolon}\n\n", plan.package_name);
    for import in imports {
        out.push_str(&format!("import {import}{semicolon}\n"));
    }
    if !imports.is_empty() {
        out.push('\n');
    }
    out
}

fn members(kotlin: bool, fields: &[(String, String)], indent: &str) -> String {
    fields
        .iter()
        .map(|(n, t)| {
            if kotlin {
                format!("{indent}val {n}: {}?\n", t.trim_end_matches('?'))
            } else {
                format!("{indent}{t} {}();\n", projection_action::getter(n, t))
            }
        })
        .collect()
}

fn render_projection(plan: &Plan) -> Result<String, Error> {
    let kotlin = plan.class.kotlin;
    let mut out = header(plan, &imports(plan)?);
    let mut body = members(kotlin, &plan.root_fields, "    ");
    for nested in &plan.nested {
        if kotlin {
            body.push_str(&format!("    val {}: {}?\n", nested.attribute, nested.nested_name));
        } else {
            body.push_str(&format!("    {} {}();\n", nested.nested_name, projection_action::getter(&nested.attribute, "")));
        }
    }
    for nested in &plan.nested {
        body.push_str(&format!("\n    interface {} {{\n{}    }}\n", nested.nested_name, members(kotlin, &nested.fields, "        ")));
    }
    let public = if kotlin { "" } else { "public " };
    out.push_str(&format!("{public}interface {} {{\n{body}}}\n", plan.name));
    Ok(out)
}

fn components(plan: &Plan) -> Result<Vec<(String, String, String)>, Error> {
    let mut all: Vec<(String, String, String)> = plan.root_fields.iter().map(|(n, t)| (n.clone(), t.clone(), format!("e.{n}"))).collect();
    for nested in &plan.nested {
        for (n, t) in &nested.fields {
            all.push((format!("{}{}", nested.attribute, capitalised(n)), t.clone(), format!("e.{}.{n}", nested.attribute)));
        }
    }
    let mut seen = HashSet::new();
    for (name, _, _) in &all {
        if !identifier(name) || !seen.insert(name.as_str()) {
            return Err(format!("Flattened component `{name}` collides with another component or is not a valid identifier; pick fewer fields.").into());
        }
    }
    Ok(all)
}

fn render_flat(plan: &Plan) -> Result<(String, String), Error> {
    let all = components(plan)?;
    let mut out = header(plan, &imports(plan)?);
    if plan.class.kotlin {
        let params: Vec<_> = all.iter().map(|(n, t, _)| format!("    val {n}: {}?", t.trim_end_matches('?'))).collect();
        out.push_str(&format!("data class {}(\n{}\n)\n", plan.name, params.join(",\n")));
    } else {
        let params: Vec<_> = all.iter().map(|(n, t, _)| format!("{t} {n}")).collect();
        out.push_str(&format!("public record {}(\n        {}\n) {{\n}}\n", plan.name, params.join(",\n        ")));
    }
    let paths: Vec<_> = all.iter().map(|(_, _, p)| p.as_str()).collect();
    let snippet = format!("select new {}.{}({}) from {} e", plan.package_name, plan.name, paths.join(", "), plan.class.name);
    Ok((out, snippet))
}

fn gather(pending: &Pending, doc: &Document, project: &Project, flat: bool) -> Result<Plan, Error> {
    current(pending, doc)?;
    let class = dto_action::source(doc).ok_or(Error::Stale)?;
    if doc.text.contains("@Entity(") && doc.text.split("@Entity(").nth(1).and_then(|r| r.split(')').next()).is_some_and(|a| a.contains("name")) {
        return Err("The entity declares a custom @Entity name; the JPQL entity name is not guessed.".into());
    }
    let associations = to_one(&class);
    if associations.is_empty() {
        return Err("No to-one association with a plain target type is available (to-many nested projections are not documented by Spring Data).".into());
    }
    let default = format!("{}{}", class.name, if flat { "FlatDto" } else { "NestedProjection" });
    let name = host::input("Type name (same package)", Some(&default))?.ok_or(Error::Cancelled)?.trim().to_owned();
    if !identifier(&name)
        || name == class.name
        || class.imports.iter().any(|import| import.rsplit('.').next() == Some(name.as_str()))
        || class.attributes.iter().any(|a| base(&a.ty) == name)
    {
        return Err("Use a non-conflicting Java/Kotlin identifier.".into());
    }
    let form = if flat { Form::Record } else { Form::Interface };
    let available = projection_action::offered(&class, form);
    let items: Vec<_> = available.iter().map(|(n, t)| PickItem::new(n, n).description(t)).collect();
    let chosen = host::pick_many("Scalar properties (may be empty)", &items)?.ok_or(Error::Cancelled)?;
    let mut seen = HashSet::new();
    if chosen.iter().any(|n| !available.iter().any(|(f, _)| f == n) || !seen.insert(n.as_str())) {
        return Err("Choose offered scalar attributes at most once.".into());
    }
    let root_fields: Vec<_> = available.iter().filter(|(n, _)| chosen.contains(n)).cloned().collect();
    let eligible: Vec<_> = associations.iter().filter(|a| flat || bean_safe(&class, &a.name)).collect();
    let items: Vec<_> = eligible.iter().map(|a| PickItem::new(&a.name, &a.name).description(&a.ty)).collect();
    let picked = host::pick_many("To-one associations to include", &items)?.ok_or(Error::Cancelled)?;
    let mut seen = HashSet::new();
    if picked.is_empty() || picked.iter().any(|n| !eligible.iter().any(|a| a.name == *n) || !seen.insert(n.as_str())) {
        return Err("Choose one or more offered to-one associations exactly once.".into());
    }
    let package_name = package(doc).ok_or(Error::Stale)?;
    let mut nested = Vec::new();
    for association in eligible.iter().filter(|a| picked.contains(&a.name)) {
        let (target, _) = query_method_action::find_entity(project, &pending.root, &association.target, Some(&package_name), &class.imports)
            .map_err(|e| match e {
                Error::Stale => Error::Stale,
                other => Error::from(format!("Association `{}` is refused: {other}", association.name)),
            })?;
        if target.kotlin != class.kotlin {
            return Err(format!("Association `{}` targets a {} entity; mixed-language projections are not generated.", association.name, if target.kotlin { "Kotlin" } else { "Java" }).into());
        }
        let offered = projection_action::offered(&target, form);
        if offered.is_empty() {
            return Err(format!("Entity {} has no plain scalar attribute usable here.", target.name).into());
        }
        let items: Vec<_> = offered.iter().map(|(n, t)| PickItem::new(n, n).description(t)).collect();
        let chosen = host::pick_many(&format!("Fields of {} to include for `{}`", target.name, association.name), &items)?.ok_or(Error::Cancelled)?;
        let mut seen = HashSet::new();
        if chosen.is_empty() || chosen.iter().any(|n| !offered.iter().any(|(f, _)| f == n) || !seen.insert(n.as_str())) {
            return Err("Choose one or more offered target fields exactly once.".into());
        }
        nested.push(Nested {
            attribute: association.name.clone(),
            nested_name: format!("{}Summary", capitalised(&association.name)),
            fields: offered.into_iter().filter(|(n, _)| chosen.contains(n)).collect(),
            target,
        });
    }
    let plan = Plan { class, package_name, name, root_fields, nested };
    if !flat {
        let clash = plan.nested.iter().any(|n| {
            n.nested_name == plan.name
                || plan.class.imports.iter().any(|i| i.rsplit('.').next() == Some(n.nested_name.as_str()))
                || plan.root_fields.iter().chain(&n.fields).any(|(_, t)| base(t) == n.nested_name)
        });
        if clash {
            return Err("A generated nested interface name conflicts with another type.".into());
        }
    }
    Ok(plan)
}

fn create(pending: &Pending, doc: &Document, project: &Project, plan: &Plan, mut text: String, flat: bool, extra: &str) -> Result<FileAction, Error> {
    let name = &plan.name;
    if text.lines().filter_map(|l| l.trim().strip_prefix("import ")).any(|i| i.trim_end_matches(';').rsplit('.').next() == Some(name.as_str())) {
        return Err("Type name conflicts with an imported type.".into());
    }
    let crlf = doc.text.contains("\r\n");
    let shown = text.clone();
    if crlf {
        text = text.replace('\n', "\r\n");
    }
    if text.len() > MAX_TEXT {
        return Err("Output exceeds the supported bound.".into());
    }
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let target = format!("{directory}/{name}.{}", if plan.class.kotlin { "kt" } else { "java" });
    if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
        return Err(Error::Stale);
    }
    if flat {
        let wanted: Vec<_> = components(plan)?.into_iter().map(|(n, t, _)| (n, t)).collect();
        dto_convert_action::offer_reuse(project, &pending.root, directory, plan.class.kotlin, &doc.path, &wanted)?;
    }
    no_collision(project, &pending.root, directory, name, None)?;
    let note = if flat {
        "Flattened DTO: components are entity fields and to-one association fields with prefixed names. Class-based DTOs cannot carry nested projections (Spring Data). Java record needs Java 16+; Kotlin properties are nullable."
    } else {
        "Closed interface projection with nested interface projections for to-one associations. Spring Data: projections limit the selection to top-level properties; a nested property resolving to a join selects the entire nested property. To-many nested projections are not offered (not documented)."
    };
    let preview = format!(
        "Source: {}\nSource version: {}\nTarget: {target}\n\n{note}\n{extra}\nExact new file:\n{shown}\nOne atomic host-managed Create; no existing file or repository is edited.",
        doc.path, doc.version
    );
    if !ui_kit::prompts::confirm_exact(if flat { "Create flattened DTO?" } else { "Create nested projection?" }, &preview)? {
        return Err(Error::Cancelled);
    }
    let project = current(pending, doc)?;
    no_collision(&project, &pending.root, directory, name, None)?;
    Ok(FileAction {
        title: format!("Create {name}"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target.clone(), text, overwrite: false }],
        open: Some(target),
        ..FileAction::default()
    })
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    let plan = gather(pending, doc, project, false)?;
    let text = render_projection(&plan)?;
    create(pending, doc, project, &plan, text, false, "")
}

pub(super) fn resolve_flat(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    let plan = gather(pending, doc, project, true)?;
    let (text, snippet) = render_flat(&plan)?;
    let extra = format!(
        "JPQL constructor expression to paste into a @Query (not applied; the implicit inner joins drop rows whose association is null; no aliases are allowed in constructor expressions):\n{snippet}\n"
    );
    create(pending, doc, project, &plan, text, true, &extra)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const JAVA: &str = "package shop;\nimport jakarta.persistence.*;\nimport java.time.LocalDate;\n@Entity\npublic class Order {\n @Id Long id;\n String note;\n LocalDate placed;\n @ManyToOne Customer customer;\n @OneToMany(mappedBy = \"order\") java.util.List<Line> lines;\n @ManyToOne Ghost ghost;\n}\n";
    const CUSTOMER: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Customer {\n @Id Long id;\n String name;\n boolean active;\n}\n";
    const KOTLIN: &str = "package shop\nimport jakarta.persistence.*\n@Entity\nclass Order(@Id var id: Long? = null, var note: String = \"\", @ManyToOne var customer: Customer? = null) {}\n";
    const KCUSTOMER: &str = "package shop\nimport jakarta.persistence.*\n@Entity\nclass Customer(@Id var id: Long? = null, var name: String = \"\") {}\n";
    const JPATH: &str = "/project/src/main/java/shop/Order.java";
    const KPATH: &str = "/project/src/main/kotlin/shop/Order.kt";

    struct Case<'a> {
        title: &'a str,
        path: &'a str,
        source: &'a str,
        language: &'a str,
        files: &'a [(&'a str, &'a str)],
        answers: &'a [&'a [&'a str]],
        confirm: bool,
    }

    fn run(c: Case) -> Result<FileAction, Error> {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(c.path, c.language, c.source, &[]);
        for (p, t) in c.files {
            host.add_file(p, t);
        }
        let doc = Document { path: c.path.into(), text: c.source.into(), language: c.language.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == c.title).unwrap();
        for a in c.answers {
            host.answer_next_prompt(Some(a));
        }
        host.answer_next_prompt(c.confirm.then_some(&[]));
        plugin.resolve_file_action(&doc, pending)
    }

    fn java_case<'a>(title: &'a str, answers: &'a [&'a [&'a str]]) -> Case<'a> {
        Case { title, path: JPATH, source: JAVA, language: "java", files: &[("src/main/java/shop/Customer.java", CUSTOMER)], answers, confirm: true }
    }

    fn text_of(result: Result<FileAction, Error>) -> (String, String) {
        let action = result.unwrap();
        let [FileOperation::Create { path, text, overwrite: false }] = action.operations.as_slice() else { panic!("one non-overwriting create") };
        assert!(action.edits.is_empty());
        (path.clone(), text.clone())
    }

    #[test]
    fn java_nested_interface_projection_is_rendered() {
        let (path, text) = text_of(run(java_case(TITLE, &[&["OrderView"], &["id", "placed"], &["customer"], &["name", "active"]])));
        assert_eq!(path, "/project/src/main/java/shop/OrderView.java");
        assert_eq!(
            text,
            "package shop;\n\nimport java.time.LocalDate;\n\npublic interface OrderView {\n    Long getId();\n    LocalDate getPlaced();\n    CustomerSummary getCustomer();\n\n    interface CustomerSummary {\n        String getName();\n        boolean isActive();\n    }\n}\n"
        );
    }

    #[test]
    fn kotlin_nested_interface_projection_is_rendered() {
        let files = [("src/main/kotlin/shop/Customer.kt", KCUSTOMER)];
        let c = Case { title: TITLE, path: KPATH, source: KOTLIN, language: "kotlin", files: &files, answers: &[&["OrderView"], &["note"], &["customer"], &["name"]], confirm: true };
        let (path, text) = text_of(run(c));
        assert!(path.ends_with("/OrderView.kt"));
        assert_eq!(text, "package shop\n\ninterface OrderView {\n    val note: String?\n    val customer: CustomerSummary?\n\n    interface CustomerSummary {\n        val name: String?\n    }\n}\n");
    }

    #[test]
    fn java_flattened_record_and_constructor_expression() {
        let host_answers: &[&[&str]] = &[&["OrderFlat"], &["id"], &["customer"], &["name", "active"]];
        let (_, text) = text_of(run(java_case(FLAT_TITLE, host_answers)));
        assert_eq!(text, "package shop;\n\npublic record OrderFlat(\n        Long id,\n        String customerName,\n        boolean customerActive\n) {\n}\n");
        let (_, snippet) = render_flat(&flat_plan()).unwrap();
        assert_eq!(snippet, "select new shop.OrderFlat(e.id, e.customer.name) from Order e");
    }

    fn flat_plan() -> Plan {
        let class = parse_class(JAVA, JPATH).unwrap();
        let target = parse_class(CUSTOMER, "/project/src/main/java/shop/Customer.java").unwrap();
        Plan {
            class,
            package_name: "shop".into(),
            name: "OrderFlat".into(),
            root_fields: vec![("id".into(), "Long".into())],
            nested: vec![Nested { attribute: "customer".into(), nested_name: "CustomerSummary".into(), target, fields: vec![("name".into(), "String".into())] }],
        }
    }

    #[test]
    fn kotlin_flattened_data_class_is_nullable() {
        let files = [("src/main/kotlin/shop/Customer.kt", KCUSTOMER)];
        let c = Case { title: FLAT_TITLE, path: KPATH, source: KOTLIN, language: "kotlin", files: &files, answers: &[&["OrderFlat"], &["id"], &["customer"], &["name"]], confirm: true };
        assert_eq!(text_of(run(c)).1, "package shop\n\ndata class OrderFlat(\n    val id: Long?,\n    val customerName: String?\n)\n");
    }

    #[test]
    fn flattened_name_collisions_are_refused() {
        let mut plan = flat_plan();
        plan.root_fields.push(("customerName".into(), "String".into()));
        assert!(render_flat(&plan).is_err());
    }

    #[test]
    fn to_many_is_not_offered_and_unresolved_or_ambiguous_targets_are_refused() {
        let class = parse_class(JAVA, JPATH).unwrap();
        assert!(to_one(&class).iter().all(|a| a.name != "lines"));
        let mut c = java_case(TITLE, &[&["OrderView"], &["id"], &["lines"]]);
        assert!(run(Case { ..c }).is_err());
        c = java_case(TITLE, &[&["OrderView"], &["id"], &["ghost"]]);
        let error = run(c).err().unwrap().to_string();
        assert!(error.contains("ghost") && error.contains("Ghost"), "{error}");
        let mut c = java_case(TITLE, &[&["OrderView"], &["id"], &["customer"]]);
        c.files = &[("src/main/java/shop/Customer.java", CUSTOMER), ("src/main/kotlin/shop/Customer.kt", KCUSTOMER)];
        assert!(run(c).is_err());
    }

    #[test]
    fn forged_selections_empty_choices_and_bad_names_are_refused() {
        for answers in [
            &[&["OrderView"][..], &["id", "id"], &["customer"], &["name"]][..],
            &[&["OrderView"], &["lines"], &["customer"], &["name"]],
            &[&["OrderView"], &["id"], &[], &["name"]],
            &[&["OrderView"], &["id"], &["customer"], &[]],
            &[&["OrderView"], &["id"], &["customer"], &["id", "id"]],
            &[&["Order"], &["id"], &["customer"], &["name"]],
            &[&["LocalDate"], &["id"], &["customer"], &["name"]],
        ] {
            assert!(run(java_case(TITLE, answers)).is_err(), "{answers:?}");
        }
    }

    #[test]
    fn declined_confirmation_existing_files_stale_source_and_crlf() {
        let mut c = java_case(FLAT_TITLE, &[&["OrderFlat"], &["id"], &["customer"], &["name"]]);
        c.confirm = false;
        assert!(matches!(run(c), Err(Error::Cancelled)));
        for (ext, body) in [("java", "package shop; public class OrderView {}"), ("kt", "package shop\nclass OrderView")] {
            let mut c = java_case(TITLE, &[&["OrderView"], &["id"], &["customer"], &["name"]]);
            let existing = format!("src/main/java/shop/OrderView.{ext}");
            let files = [("src/main/java/shop/Customer.java", CUSTOMER), (existing.as_str(), body)];
            c.files = &files;
            assert!(run(c).is_err(), "{ext}");
        }
        let crlf = JAVA.replace('\n', "\r\n");
        let mut c = java_case(FLAT_TITLE, &[&["OrderFlat"], &["id"], &["customer"], &["name"]]);
        c.source = &crlf;
        let text = text_of(run(c)).1;
        assert!(text.contains("\r\n") && !text.replace("\r\n", "").contains('\n'));
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(JPATH, "java", JAVA, &[]);
        let doc = Document { path: JPATH.into(), text: JAVA.into(), language: "java".into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.type_in_editor("// changed");
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Stale)));
    }

    #[test]
    fn custom_entity_name_and_entities_without_to_one_have_no_action_or_refuse() {
        let source = JAVA.replace("@Entity\n", "@Entity(name = \"Ord\")\n");
        let mut c = java_case(FLAT_TITLE, &[&["OrderFlat"], &["id"], &["customer"], &["name"]]);
        c.source = &source;
        assert!(run(c).is_err());
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor("/project/src/main/java/shop/Customer.java", "java", CUSTOMER, &[]);
        let doc = Document { path: "/project/src/main/java/shop/Customer.java".into(), text: CUSTOMER.into(), language: "java".into(), version: 1 };
        let titles: Vec<_> = JpaAssist::default().file_actions(&doc, Range::default()).into_iter().map(|a| a.title).collect();
        assert!(!titles.iter().any(|t| t == TITLE || t == FLAT_TITLE));
    }

    #[test]
    #[ignore = "requires javac, kotlinc classpath and JPA_JAKARTA_API"]
    fn generated_nested_types_compile() {
        let dir = std::env::temp_dir().join(format!("jpa-nested-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let jar = std::env::var("JPA_JAKARTA_API").expect("explicit API jar path");
        let java_entity = JAVA.replace(" @OneToMany(mappedBy = \"order\") java.util.List<Line> lines;\n @ManyToOne Ghost ghost;\n", "");
        let mut java_files = vec![("Order.java".to_owned(), java_entity), ("Customer.java".to_owned(), CUSTOMER.to_owned())];
        let (_, view) = text_of(run(java_case(TITLE, &[&["OrderView"], &["id", "placed"], &["customer"], &["name", "active"]])));
        let (_, flat) = text_of(run(java_case(FLAT_TITLE, &[&["OrderFlat"], &["id", "placed"], &["customer"], &["name", "active"]])));
        java_files.push(("OrderView.java".into(), view));
        java_files.push(("OrderFlat.java".into(), flat));
        let files = [("src/main/kotlin/shop/Customer.kt", KCUSTOMER)];
        let kc = |title, answers: &'static [&'static [&'static str]]| Case { title, path: KPATH, source: KOTLIN, language: "kotlin", files: &files, answers, confirm: true };
        let kview = text_of(run(kc(TITLE, &[&["OrderView"], &["note"], &["customer"], &["name"]]))).1;
        let kflat = text_of(run(kc(FLAT_TITLE, &[&["OrderFlat"], &["id"], &["customer"], &["name"]]))).1;
        let kotlin_files = vec![("Order.kt".to_owned(), KOTLIN.to_owned()), ("Customer.kt".to_owned(), KCUSTOMER.to_owned()), ("OrderView.kt".into(), kview), ("OrderFlat.kt".into(), kflat)];
        let write = |sub: &str, files: &[(String, String)]| {
            let d = dir.join(sub);
            std::fs::create_dir_all(&d).unwrap();
            let paths: Vec<_> = files
                .iter()
                .map(|(n, b)| {
                    let p = d.join(n);
                    std::fs::write(&p, b).unwrap();
                    p
                })
                .collect();
            (d, paths)
        };
        let (d, paths) = write("java", &java_files);
        let out = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &jar, "-d"]).arg(&d).args(&paths).output().expect("javac installed");
        assert!(out.status.success(), "{}\n{java_files:?}", String::from_utf8_lossy(&out.stderr));
        let compiler = std::env::var("KOTLIN_COMPILER_CLASSPATH").expect("explicit Kotlin compiler classpath");
        let stdlib = std::env::var("KOTLIN_STDLIB").expect("explicit Kotlin standard library jar");
        let (d, paths) = write("kotlin", &kotlin_files);
        let out = std::process::Command::new("java")
            .args(["-classpath", &compiler, "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler", "-no-stdlib", "-no-reflect", "-jvm-target", "17", "-classpath", &format!("{jar}:{stdlib}"), "-d"])
            .arg(&d)
            .args(&paths)
            .output()
            .expect("java installed");
        assert!(out.status.success(), "{}{}\n{kotlin_files:?}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
