//! "Entity from POJO": relationship detection. A field whose type is, or is a List/Set of, an entity of
//! the project is a relationship candidate. The cardinality is proposed from the other side (a
//! back-reference on the target entity) and the user confirms or changes it per field. Only the
//! invoking file is edited, so a related plain class is never converted here: it is reported and the
//! field stays unmapped.
use super::*;
use crate::model::Relation;

const COLLECTIONS: [&str; 2] = ["List", "Set"];

struct Candidate {
    name: String,
    target: String,
    collection: bool,
    line_start: usize,
    indent: String,
    options: Vec<(String, String)>,
}

fn element(ty: &str) -> Option<(&str, bool)> {
    for collection in COLLECTIONS {
        if let Some(inner) = ty.strip_prefix(collection).and_then(|rest| rest.strip_prefix('<')).and_then(|rest| rest.strip_suffix('>')) {
            return identifier(inner).then_some((inner, true));
        }
    }
    identifier(ty).then_some((ty, false))
}

fn imports_of(text: &str) -> Vec<String> {
    mask(text)
        .lines()
        .filter_map(|line| line.trim().strip_prefix("import "))
        .map(|rest| rest.trim().trim_end_matches(';').trim().to_owned())
        .collect()
}

fn annotation(namespace: &str, kind: &str, mapped_by: Option<&str>) -> String {
    match mapped_by {
        Some(field) => format!("@{namespace}.{kind}(mappedBy = \"{field}\")"),
        None => format!("@{namespace}.{kind}"),
    }
}

/// The relationship annotations to offer for a field, the detected one first.
fn options(namespace: &str, pojo: &str, collection: bool, target: &Class) -> Vec<(String, String)> {
    let back = target.attributes.iter().find(|a| a.target == pojo);
    let back_collection = back.is_some_and(|a| matches!(a.relation, Relation::OneToMany | Relation::ManyToMany) || a.ty.contains('<'));
    let mut all: Vec<(&str, Option<&str>)> = if collection {
        vec![("OneToMany", None), ("ManyToMany", None)]
    } else {
        vec![("ManyToOne", None), ("OneToOne", None)]
    };
    let detected = match (collection, back) {
        (false, Some(_)) if back_collection => ("ManyToOne", None),
        (false, Some(back)) if back.relation == Relation::OneToOne && !back.mapped_by => ("OneToOne", Some(back.name.as_str())),
        (false, Some(_)) => ("OneToOne", None),
        (true, Some(back)) if back_collection => ("ManyToMany", Some(back.name.as_str()).filter(|_| !back.mapped_by)),
        (true, Some(back)) => ("OneToMany", Some(back.name.as_str())),
        (false, None) => ("ManyToOne", None),
        (true, None) => ("OneToMany", None),
    };
    all.retain(|(kind, _)| *kind != detected.0);
    all.insert(0, detected);
    let mut out: Vec<(String, String)> = all.into_iter().map(|(kind, mapped_by)| (kind.to_owned(), annotation(namespace, kind, mapped_by))).collect();
    out.push(("none".into(), "Leave unmapped".into()));
    out
}

fn candidates(project: &Project, root: &str, doc: &Document, namespace: &str) -> Result<(Vec<Candidate>, Vec<String>), Error> {
    let own_package = package(doc).ok_or(Error::Stale)?;
    let imports = imports_of(&doc.text);
    let pojo = doc.path.rsplit('/').next().and_then(|n| n.strip_suffix(".java")).unwrap_or_default().to_owned();
    let mut found = Vec::new();
    let mut notes = Vec::new();
    for field in entity_from_class::fields(doc)? {
        let Some((target, collection)) = element(&field.ty) else {
            continue;
        };
        match query_method_action::find_entity(project, root, target, Some(&own_package), &imports) {
            Ok((class, _)) => {
                let options = options(namespace, &pojo, collection, &class);
                found.push(Candidate { name: field.name, target: target.to_owned(), collection, line_start: field.line_start, indent: field.indent, options });
            }
            Err(_) if !project.find_files(&format!("**/{target}.java"), 2)?.is_empty() => {
                notes.push(format!("`{}` ({target}) is a project class that is not an entity; convert {target} first. This action edits only the current file, so the field stays unmapped.", field.name));
            }
            Err(_) => {}
        }
    }
    Ok((found, notes))
}

/// Edits for the relationships the user chose, and a note for the preview.
pub(super) fn choose(project: &Project, root: &str, doc: &Document, namespace: persistence_core::generation::Namespace) -> Result<(Vec<TextEdit>, String), Error> {
    if doc.language != "java" {
        return Ok((Vec::new(), String::new()));
    }
    let (found, notes) = candidates(project, root, doc, namespace.package())?;
    let mut edits = Vec::new();
    let mut summary = Vec::new();
    for candidate in &found {
        let items: Vec<_> = candidate.options.iter().map(|(key, text)| PickItem::new(key, if key == "none" { text } else { key }).description(if key == "none" { "" } else { text })).collect();
        let title = format!("Relationship of `{}` ({}{})", candidate.name, candidate.target, if candidate.collection { " collection" } else { "" });
        let key = host::pick(&title, &items)?.ok_or(Error::Cancelled)?;
        let (_, text) = candidate.options.iter().find(|(known, _)| *known == key).ok_or("Choose one of the offered relationship kinds.")?;
        if key == "none" {
            continue;
        }
        let at = position(&doc.text, candidate.line_start);
        let line = format!("{}{text}\n", candidate.indent);
        edits.push(TextEdit { range: Range::new(at, at), text: if doc.text.contains("\r\n") { line.replace('\n', "\r\n") } else { line } });
        summary.push(format!("{} -> {key}", candidate.name));
    }
    let mut note = String::new();
    if !summary.is_empty() {
        note.push_str(&format!("\nRelationships: {}.", summary.join(", ")));
    }
    for line in notes {
        note.push_str(&format!("\n{line}"));
    }
    Ok((edits, note))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const POJO: &str = "package shop;\n\nimport java.util.List;\n\npublic class Invoice {\n\n    private String title;\n\n    private Customer customer;\n\n    private List<Line> lines;\n\n    private Address address;\n}\n";
    const CUSTOMER: &str = "package shop;\nimport jakarta.persistence.*;\nimport java.util.List;\n@Entity\npublic class Customer {\n @Id Long id;\n @OneToMany(mappedBy = \"customer\") List<Invoice> invoices;\n}\n";
    const LINE: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Line {\n @Id Long id;\n @ManyToOne Invoice invoice;\n}\n";
    const ADDRESS: &str = "package shop;\npublic class Address {\n    private String street;\n}\n";
    const PATH: &str = "/project/src/main/java/shop/Invoice.java";

    fn convert(answers: &[&[&str]]) -> Result<String, Error> {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(PATH, "java", POJO, &[]);
        host.add_file(PATH, POJO);
        host.add_file("/project/src/main/java/shop/Customer.java", CUSTOMER);
        host.add_file("/project/src/main/java/shop/Line.java", LINE);
        host.add_file("/project/src/main/java/shop/Address.java", ADDRESS);
        let doc = Document { path: PATH.into(), text: POJO.into(), language: "java".into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == entity_from_class::TITLE).expect("offered");
        for row in answers {
            host.answer_next_prompt(Some(row));
        }
        let action = plugin.resolve_file_action(&doc, pending)?;
        assert!(action.operations.is_empty());
        Ok(patched(POJO, &action.edits[0].1).unwrap())
    }

    const HEAD: [&[&str]; 6] = [&["jakarta"], &["invoices"], &["id"], &[""], &["long"], &["identity"]];

    fn rows(relations: &[&'static [&'static str]]) -> Vec<&'static [&'static str]> {
        let mut rows: Vec<&[&str]> = HEAD.to_vec();
        rows.extend_from_slice(relations);
        rows.push(&[]);
        rows
    }

    #[test]
    fn cardinality_is_proposed_from_the_back_reference_and_confirmed_per_field() {
        let text = convert(&rows(&[&["ManyToOne"], &["OneToMany"]])).unwrap();
        assert!(text.contains("    @jakarta.persistence.ManyToOne\n    private Customer customer;"), "{text}");
        assert!(text.contains("    @jakarta.persistence.OneToMany(mappedBy = \"invoice\")\n    private List<Line> lines;"), "{text}");
        assert!(!text.contains("@jakarta.persistence.ManyToOne\n    private Address") && text.contains("    private Address address;"), "{text}");
    }

    #[test]
    fn the_user_can_override_or_skip_a_detected_relationship_and_forged_kinds_are_refused() {
        let text = convert(&rows(&[&["OneToOne"], &["none"]])).unwrap();
        assert!(text.contains("@jakarta.persistence.OneToOne\n    private Customer customer;") && !text.contains("OneToMany"), "{text}");
        assert!(convert(&rows(&[&["ManyToMany"], &["none"]])).is_err(), "ManyToMany is not an option of a single-valued field");
        assert!(matches!(convert(&HEAD), Err(Error::Cancelled)));
    }
}
