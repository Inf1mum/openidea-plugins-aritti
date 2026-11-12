//! Reviewed Java record DTO for an entity and its associations. For every association the user chooses
//! how its data is represented: a new record in a separate file, a new nested record, an existing DTO of
//! the project, or flat (the inner fields become top-level components named from the inner class name and
//! the field name; to-one associations only). A nested record is implicitly static. New files only
//! (`overwrite: false`); no existing file or repository is edited.
use super::*;
use crate::model::Relation;
use projection_action::Form;

pub(super) const TITLE: &str = "Create DTO with associations…";
const MAX_ASSOCIATIONS: usize = 6;
const COLLECTIONS: [&str; 2] = ["List", "Set"];

#[derive(Clone, PartialEq)]
enum Shape {
    NewClass,
    Nested,
    Existing { simple: String, import: Option<String> },
    Flat,
}

struct Association {
    attribute: String,
    collection: Option<String>,
    target: Class,
    shape: Shape,
    name: String,
    fields: Vec<(String, String)>,
}

fn capitalised(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map(|c| c.to_ascii_uppercase().to_string() + chars.as_str()).unwrap_or_default()
}

fn decapitalised(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map(|c| c.to_ascii_lowercase().to_string() + chars.as_str()).unwrap_or_default()
}

fn base(ty: &str) -> &str {
    ty.split('<').next().unwrap_or(ty).rsplit('.').next().unwrap_or(ty)
}

fn eligible_associations(class: &Class) -> Vec<&crate::model::Attribute> {
    class
        .attributes
        .iter()
        .filter(|a| identifier(&a.name) && identifier(&a.target))
        .filter(|a| match a.relation {
            Relation::ManyToOne | Relation::OneToOne => true,
            Relation::OneToMany | Relation::ManyToMany => COLLECTIONS.iter().any(|c| a.ty.starts_with(&format!("{c}<"))),
            _ => false,
        })
        .collect()
}

pub(super) fn eligible(doc: &Document) -> bool {
    doc.language == "java"
        && dto_action::record_offered(&doc.path)
        && dto_action::source(doc).is_some_and(|class| !class.kotlin && dto_action::fields(&class).is_some() && !eligible_associations(&class).is_empty())
}

fn component(indent: &str, (name, ty): &(String, String)) -> String {
    format!("{indent}{ty} {name}")
}

fn record(indent: &str, public: bool, name: &str, components: &[String], nested: &[String]) -> String {
    let head = if public { "public " } else { "" };
    let mut out = format!("{indent}{head}record {name}(\n{}\n{indent}) {{", components.join(",\n"));
    if nested.is_empty() {
        out.push_str(&format!("\n{indent}}}\n"));
        return out;
    }
    for text in nested {
        out.push_str(&format!("\n\n{text}"));
    }
    out.push_str(&format!("{indent}}}\n"));
    out
}

fn flat_components(associations: &[Association]) -> Result<Vec<(String, String)>, Error> {
    let mut out = Vec::new();
    for association in associations.iter().filter(|a| a.shape == Shape::Flat) {
        for (name, ty) in &association.fields {
            out.push((format!("{}{}", decapitalised(&association.target.name), capitalised(name)), ty.clone()));
        }
    }
    Ok(out)
}

fn view_type(association: &Association) -> String {
    let simple = match &association.shape {
        Shape::Existing { simple, .. } => simple.clone(),
        _ => association.name.clone(),
    };
    match &association.collection {
        Some(collection) => format!("{collection}<{simple}>"),
        None => simple,
    }
}

fn imports(package_name: &str, types: &[(&Class, String)], extra: &[String]) -> String {
    let mut found: Vec<String> = extra.to_vec();
    for (class, ty) in types {
        let (outer, inner) = ty.split_once('<').map_or((ty.as_str(), ""), |(outer, rest)| (outer, rest.trim_end_matches('>')));
        found.extend(class.qualified(base(outer)));
        if !inner.is_empty() && class.qualified(base(inner)).is_some_and(|q| q.starts_with("java.")) {
            found.extend(class.qualified(base(inner)));
        }
    }
    found.sort();
    found.dedup();
    found.retain(|import| import.rsplit_once('.').map(|(package, _)| package) != Some(package_name));
    let text: String = found.iter().map(|import| format!("import {import};\n")).collect();
    if text.is_empty() { text } else { format!("{text}\n") }
}

fn render(class: &Class, package_name: &str, name: &str, scalars: &[(String, String)], associations: &[Association]) -> Result<Vec<(String, String)>, Error> {
    let mut all: Vec<(String, String)> = scalars.to_vec();
    all.extend(flat_components(associations)?);
    for association in associations.iter().filter(|a| a.shape != Shape::Flat) {
        all.push((association.attribute.clone(), view_type(association)));
    }
    let mut seen = HashSet::new();
    if let Some((clash, _)) = all.iter().find(|(n, _)| !identifier(n) || !seen.insert(n.as_str())) {
        return Err(format!("Component `{clash}` collides with another component or is not a valid identifier; pick fewer fields.").into());
    }
    let mut types: Vec<(&Class, String)> = scalars.iter().map(|(_, ty)| (class, ty.clone())).collect();
    let mut extra: Vec<String> = Vec::new();
    for association in associations {
        match &association.shape {
            Shape::Flat => types.extend(association.fields.iter().map(|(_, ty)| (&association.target, ty.clone()))),
            Shape::Nested => types.extend(association.fields.iter().map(|(_, ty)| (&association.target, ty.clone()))),
            Shape::Existing { import, .. } => extra.extend(import.iter().cloned()),
            Shape::NewClass => {}
        }
        if let Some(collection) = &association.collection {
            extra.extend(class.qualified(collection));
        }
    }
    let nested: Vec<String> = associations
        .iter()
        .filter(|a| a.shape == Shape::Nested)
        .map(|a| record("    ", true, &a.name, &a.fields.iter().map(|f| component("            ", f)).collect::<Vec<_>>(), &[]))
        .collect();
    let components: Vec<String> = all.iter().map(|c| component("        ", c)).collect();
    let mut files = vec![(name.to_owned(), format!("package {package_name};\n\n{}{}", imports(package_name, &types, &extra), record("", true, name, &components, &nested)))];
    for association in associations.iter().filter(|a| a.shape == Shape::NewClass) {
        let own: Vec<(&Class, String)> = association.fields.iter().map(|(_, ty)| (&association.target, ty.clone())).collect();
        let components: Vec<String> = association.fields.iter().map(|f| component("        ", f)).collect();
        files.push((association.name.clone(), format!("package {package_name};\n\n{}{}", imports(package_name, &own, &[]), record("", true, &association.name, &components, &[]))));
    }
    Ok(files)
}

fn existing_candidates(project: &Project, root: &str, target: &Class) -> Result<Vec<(String, String, Option<String>)>, Error> {
    let directory = target.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let scalars = projection_action::offered(target, Form::Record);
    let mut found = dto_links::linked(project, root, &target.path, false)?;
    for candidate in dto_convert_action::reusable(project, root, directory, false, &target.path, &scalars)? {
        if !found.iter().any(|known| known.path == candidate.path) {
            found.push(candidate);
        }
    }
    let mut out = Vec::new();
    for candidate in found {
        let text = project.read_file(&candidate.path)?;
        let document = Document { path: candidate.path.clone(), text, language: "java".into(), version: 0 };
        let import = package(&document).map(|package| format!("{package}.{}", candidate.name));
        out.push((candidate.path, candidate.name, import));
    }
    Ok(out)
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    if let Some(level) = dto_action::java_level_below_record(&doc.path) {
        return Err(dto_action::record_refusal(level).into());
    }
    let class = dto_action::source(doc).filter(|class| !class.kotlin).ok_or(Error::Stale)?;
    let eligible = eligible_associations(&class);
    if eligible.is_empty() {
        return Err("No to-one association or List/Set association with a plain target type is available.".into());
    }
    let default = format!("{}Dto", class.name);
    let name = host::input("DTO record name (same package; ending in Dto)", Some(&default))?.ok_or(Error::Cancelled)?.trim().to_owned();
    if !identifier(&name) || !name.ends_with("Dto") || name == class.name || class.imports.iter().any(|i| i.rsplit('.').next() == Some(name.as_str())) {
        return Err("Use a non-conflicting Java identifier ending in Dto.".into());
    }
    let available = projection_action::offered(&class, Form::Record);
    let items: Vec<_> = available.iter().map(|(n, t)| PickItem::new(n, n).description(t)).collect();
    let chosen = host::pick_many("Scalar fields of the entity (may be empty)", &items)?.ok_or(Error::Cancelled)?;
    let mut seen = HashSet::new();
    if chosen.iter().any(|n| !available.iter().any(|(f, _)| f == n) || !seen.insert(n.as_str())) {
        return Err("Choose offered scalar fields at most once.".into());
    }
    let scalars: Vec<_> = available.iter().filter(|(n, _)| chosen.contains(n)).cloned().collect();
    let items: Vec<_> = eligible.iter().map(|a| PickItem::new(&a.name, &a.name).description(&a.ty)).collect();
    let picked = host::pick_many("Associations to include", &items)?.ok_or(Error::Cancelled)?;
    let mut seen = HashSet::new();
    if picked.is_empty() || picked.len() > MAX_ASSOCIATIONS || picked.iter().any(|n| !eligible.iter().any(|a| a.name == *n) || !seen.insert(n.as_str())) {
        return Err(format!("Choose one to {MAX_ASSOCIATIONS} offered associations, each once.").into());
    }
    let declared = package(doc).ok_or(Error::Stale)?;
    let mut taken = vec![name.clone(), class.name.clone()];
    let mut associations = Vec::new();
    for association in eligible.iter().filter(|a| picked.contains(&a.name)) {
        current(pending, doc)?;
        let (target, _) = query_method_action::find_entity(project, &pending.root, &association.target, Some(&declared), &class.imports)
            .map_err(|e| match e {
                Error::Stale => Error::Stale,
                other => Error::from(format!("Association `{}` is refused: {other}", association.name)),
            })?;
        if target.kotlin {
            return Err(format!("Association `{}` targets a Kotlin entity; Java DTOs only.", association.name).into());
        }
        let collection = association.ty.split_once('<').map(|(c, _)| c.to_owned());
        let target_scalars = projection_action::offered(&target, Form::Record);
        let existing = existing_candidates(project, &pending.root, &target)?;
        let mut forms = vec![PickItem::new("new-class", "New class: a new record in a separate file"), PickItem::new("nested", "New nested class: a public nested record")];
        if !existing.is_empty() {
            forms.push(PickItem::new("existing", "Existing class: a DTO already in the project"));
        }
        if collection.is_none() {
            forms.push(PickItem::new("flat", "Flat: the inner fields become top-level components"));
        }
        let form = host::pick(&format!("Representation of `{}` ({})", association.name, target.name), &forms)?.ok_or(Error::Cancelled)?;
        let shape = match form.as_str() {
            "new-class" => Shape::NewClass,
            "nested" => Shape::Nested,
            "flat" if collection.is_none() => Shape::Flat,
            "existing" if !existing.is_empty() => {
                let items: Vec<_> = existing.iter().map(|(path, name, _)| PickItem::new(path, name).description(path)).collect();
                let key = host::pick(&format!("Existing DTO for `{}`", association.name), &items)?.ok_or(Error::Cancelled)?;
                let (_, simple, import) = existing.iter().find(|(path, _, _)| *path == key).ok_or("Choose one of the offered DTOs.")?;
                let import = import.clone().filter(|import| import.rsplit_once('.').map(|(package, _)| package) != Some(declared.as_str()));
                Shape::Existing { simple: simple.clone(), import }
            }
            _ => return Err("Choose one of the offered representations.".into()),
        };
        let mut view_name = String::new();
        if matches!(shape, Shape::NewClass | Shape::Nested) {
            let default = format!("{}Dto", target.name);
            view_name = host::input(&format!("Name of the new DTO for `{}`", association.name), Some(&default))?.ok_or(Error::Cancelled)?.trim().to_owned();
            if !identifier(&view_name) || taken.contains(&view_name) || class.imports.iter().any(|i| i.rsplit('.').next() == Some(view_name.as_str())) {
                return Err("Use a Java identifier that does not clash with the other DTOs, imports or the entity.".into());
            }
            taken.push(view_name.clone());
        }
        let mut fields = Vec::new();
        if matches!(shape, Shape::NewClass | Shape::Nested | Shape::Flat) {
            if target_scalars.is_empty() {
                return Err(format!("Entity {} has no plain scalar attribute usable here.", target.name).into());
            }
            let items: Vec<_> = target_scalars.iter().map(|(n, t)| PickItem::new(n, n).description(t)).collect();
            let chosen = host::pick_many(&format!("Fields of {} for `{}`", target.name, association.name), &items)?.ok_or(Error::Cancelled)?;
            let mut seen = HashSet::new();
            if chosen.is_empty() || chosen.iter().any(|n| !target_scalars.iter().any(|(f, _)| f == n) || !seen.insert(n.as_str())) {
                return Err("Choose one or more offered target fields exactly once.".into());
            }
            fields = target_scalars.iter().filter(|(n, _)| chosen.contains(n)).cloned().collect();
        }
        associations.push(Association { attribute: association.name.clone(), collection, target, shape, name: view_name, fields });
    }
    let files = render(&class, &declared, &name, &scalars, &associations)?;
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let mut operations = Vec::new();
    let mut shown = String::new();
    for (file_name, text) in files {
        let text = if doc.text.contains("\r\n") { text.replace('\n', "\r\n") } else { text };
        if text.len() > MAX_TEXT {
            return Err("DTO output exceeds the supported bound.".into());
        }
        let target = format!("{directory}/{file_name}.java");
        if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
            return Err(Error::Stale);
        }
        no_collision(project, &pending.root, directory, &file_name, None)?;
        shown.push_str(&format!("New file: {target}\n{text}\n"));
        operations.push(FileOperation::Create { path: target, text, overwrite: false });
    }
    let note = "Java records need Java 16 or newer; a nested record is implicitly static. Flat components are named from the inner class name and the field name. Existing DTOs are referenced by name only (their fields are not verified), no mapper or repository is generated.";
    if !ui_kit::prompts::confirm_exact(
        "Create DTO with associations?",
        &format!("Source: {}\nSource version: {}\n\n{note}\n\nExact new files:\n{shown}One atomic host-managed Create; no existing file is overwritten.", doc.path, doc.version),
    )? {
        return Err(Error::Cancelled);
    }
    let project = current(pending, doc)?;
    for operation in &operations {
        if let FileOperation::Create { path, .. } = operation {
            let stem = path.rsplit('/').next().and_then(|n| n.strip_suffix(".java")).unwrap_or_default();
            no_collision(&project, &pending.root, directory, stem, None)?;
        }
    }
    let open = match operations.first() {
        Some(FileOperation::Create { path, .. }) => Some(path.clone()),
        _ => None,
    };
    Ok(FileAction { title: format!("Create {name}"), kind: Some("refactor.rewrite".into()), operations, open, ..FileAction::default() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const PATH: &str = "/project/src/main/java/shop/Order.java";
    const ORDER: &str = "package shop;\nimport jakarta.persistence.*;\nimport java.time.LocalDate;\nimport java.util.List;\n@Entity\npublic class Order {\n @Id Long id;\n String note;\n LocalDate placed;\n @ManyToOne Customer customer;\n @OneToOne Address shipping;\n @OneToMany(mappedBy = \"order\") List<Line> lines;\n}\n";
    const CUSTOMER: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Customer {\n @Id Long id;\n String name;\n}\n";
    const ADDRESS: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Address {\n @Id Long id;\n String street;\n String city;\n}\n";
    const LINE: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Line {\n @Id Integer id;\n String sku;\n}\n";
    const SUMMARY: &str = "package shop.dto;\npublic record CustomerSummary(Long id, String name) {\n}\n";

    fn host(extra: &[(&str, &str)]) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(PATH, "java", ORDER, &[]);
        host.add_file(PATH, ORDER);
        host.add_file("/project/src/main/java/shop/Customer.java", CUSTOMER);
        host.add_file("/project/src/main/java/shop/Address.java", ADDRESS);
        host.add_file("/project/src/main/java/shop/Line.java", LINE);
        for (path, text) in extra {
            host.add_file(path, text);
        }
        let doc = Document { path: PATH.into(), text: ORDER.into(), language: "java".into(), version: 1 };
        (host, JpaAssist::default(), doc)
    }

    fn run(extra: &[(&str, &str)], answers: &[&[&str]]) -> Result<Vec<(String, String)>, Error> {
        let (host, mut plugin, doc) = host(extra);
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).ok_or(Error::Stale)?;
        for answer in answers {
            host.answer_next_prompt(Some(answer));
        }
        let action = plugin.resolve_file_action(&doc, pending)?;
        assert!(action.edits.is_empty());
        Ok(action
            .operations
            .into_iter()
            .map(|operation| match operation {
                FileOperation::Create { path, text, overwrite: false } => (path, text),
                _ => panic!("only non-overwriting creates"),
            })
            .collect())
    }

    #[test]
    fn nested_and_separate_file_records_for_to_one_and_to_many_associations() {
        let answers: [&[&str]; 10] = [&["OrderDto"], &["note"], &["customer", "lines"], &["nested"], &["CustomerDto"], &["name"], &["new-class"], &["LineDto"], &["sku"], &[]];
        let files = run(&[], &answers).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(
            files[0].1,
            "package shop;\n\nimport java.util.List;\n\npublic record OrderDto(\n        String note,\n        CustomerDto customer,\n        List<LineDto> lines\n) {\n\n    public record CustomerDto(\n            String name\n    ) {\n    }\n}\n"
        );
        assert_eq!(files[1].0, "/project/src/main/java/shop/LineDto.java");
        assert_eq!(files[1].1, "package shop;\n\npublic record LineDto(\n        String sku\n) {\n}\n");
    }

    #[test]
    fn existing_and_flat_forms_reference_a_project_dto_and_flatten_inner_fields() {
        let summary = ("/project/src/main/java/shop/dto/CustomerSummary.java", SUMMARY);
        let files = run(&[summary], &[&["OrderDto"], &["id"], &["customer", "shipping"], &["existing"], &[summary.0], &["flat"], &["street", "city"], &[]]).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].1,
            "package shop;\n\nimport shop.dto.CustomerSummary;\n\npublic record OrderDto(\n        Long id,\n        String addressStreet,\n        String addressCity,\n        CustomerSummary customer\n) {\n}\n"
        );
    }

    #[test]
    fn existing_is_offered_only_when_a_dto_exists_and_flat_never_for_collections() {
        let answers: [&[&str]; 4] = [&["OrderDto"], &[], &["lines"], &["flat"]];
        assert!(run(&[], &answers).is_err(), "flat is not offered for a collection");
        assert!(run(&[], &[&["OrderDto"], &[], &["customer"], &["existing"]]).is_err(), "existing is not offered without a DTO");
    }

    #[test]
    fn refusals_and_cancellations_create_nothing() {
        assert!(run(&[], &[&["Order"]]).is_err());
        assert!(run(&[], &[&["OrderDto"], &["ghost"]]).is_err());
        assert!(run(&[], &[&["OrderDto"], &[], &[]]).is_err(), "at least one association");
        assert!(run(&[], &[&["OrderDto"], &[], &["customer"], &["nested"], &["OrderDto"]]).is_err(), "inner name clashes with the outer DTO");
        assert!(matches!(run(&[], &[&["OrderDto"], &[], &["customer"], &["nested"], &["CustomerDto"], &["name"]]), Err(Error::Cancelled)));
        assert!(run(&[("/project/src/main/java/shop/OrderDto.java", "package shop;\npublic record OrderDto() {}\n")], &[&["OrderDto"], &[], &["customer"], &["nested"], &["CustomerDto"], &["name"], &[]]).is_err());
    }

    #[test]
    #[ignore = "requires javac on PATH"]
    fn generated_records_compile() {
        let summary = ("/project/src/main/java/shop/dto/CustomerSummary.java", SUMMARY);
        let mut files = run(&[summary], &[&["OrderDto"], &["note", "placed"], &["customer", "shipping", "lines"], &["existing"], &[summary.0], &["flat"], &["street"], &["new-class"], &["LineDto"], &["sku"], &[]]).unwrap();
        files.push((summary.0.into(), SUMMARY.into()));
        let nested: [&[&str]; 10] = [&["OrderDto"], &["note"], &["customer", "lines"], &["nested"], &["CustomerDto"], &["name"], &["new-class"], &["LineDto"], &["sku"], &[]];
        for (index, group) in [files, run(&[], &nested).unwrap()].into_iter().enumerate() {
            let dir = std::env::temp_dir().join(format!("jpa-dto-assoc-{}-{index}", std::process::id()));
            std::fs::create_dir_all(dir.join("shop/dto")).unwrap();
            let paths: Vec<_> = group
                .iter()
                .map(|(path, body)| {
                    let target = dir.join(path.trim_start_matches("/project/src/main/java/"));
                    std::fs::write(&target, body).unwrap();
                    target
                })
                .collect();
            let result = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-d"]).arg(&dir).args(&paths).output().expect("javac installed");
            assert!(result.status.success(), "{}\n{group:?}", String::from_utf8_lossy(&result.stderr));
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
