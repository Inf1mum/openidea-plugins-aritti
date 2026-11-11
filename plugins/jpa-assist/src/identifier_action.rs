//! Explicit identifier creation for a closed source-local persistence class.
use super::*;
use persistence_core::generation::{
    GenerationStrategy, IdentifierDraft, IdentifierType, Namespace, SequenceGenerator,
    TableGenerator,
};

pub(super) fn source(doc: &Document) -> Option<(Class, Namespace)> {
    let class = crate::attribute_info::snapshot(doc)?;
    if class.kind == Kind::Embeddable || class.superclass.is_some() {
        return None;
    }
    let masked = mask(&doc.text);
    let at = offset(&doc.text, class.pos)?;
    let open = at + masked[at..].find('{')?;
    if masked[at..open].trim() != class.name
        || annotations(&masked)
            .iter()
            .any(|a| matches!(a.name, "Id" | "EmbeddedId" | "IdClass" | "Access"))
    {
        return None;
    }
    if ["java", "kotlin", "jakarta", "javax"].iter().any(|p| {
        class.name == *p
            || class
                .imports
                .iter()
                .any(|i| i.rsplit('.').next() == Some(p))
    }) {
        return None;
    }
    let namespace = match super::namespace(&doc.text, &class)? {
        "jakarta.persistence" => Namespace::Jakarta,
        "javax.persistence" => Namespace::Javax,
        _ => return None,
    };
    Some((class, namespace))
}

fn input(label: &str, default: &str) -> Result<String, Error> {
    host::input(label, Some(default))?
        .map(|s| s.trim().to_owned())
        .ok_or(Error::Cancelled)
}
fn integer(label: &str, default: &str) -> Result<i32, Error> {
    input(label, default)?
        .parse()
        .map_err(|_| "Enter a Java integer between -2147483648 and 2147483647.".into())
}
fn allocation() -> Result<u32, Error> {
    let value = integer("Allocation size", "50")?;
    if value <= 0 {
        return Err("Allocation size must be positive.".into());
    }
    Ok(value as u32)
}

pub(super) fn prompt(namespace: Namespace, owner: &str) -> Result<IdentifierDraft, Error> {
    let name = input("Identifier attribute name", "id")?;
    let column = input(
        "Identifier column name (empty uses the attribute name)",
        &snake_case(&name),
    )?;
    let types = [
        ("long", "Long"),
        ("integer", "Integer"),
        ("short", "Short (assigned only)"),
        ("string", "String"),
        ("uuid", "UUID"),
        ("big-integer", "BigInteger (assigned only)"),
    ];
    let selected = host::pick(
        "Identifier type",
        &types
            .iter()
            .map(|(k, l)| PickItem::new(k, l))
            .collect::<Vec<_>>(),
    )?
    .ok_or(Error::Cancelled)?;
    let id_type = match selected.as_str() {
        "long" => IdentifierType::Long,
        "integer" => IdentifierType::Integer,
        "short" => IdentifierType::Short,
        "string" => IdentifierType::String,
        "uuid" => IdentifierType::Uuid,
        "big-integer" => IdentifierType::BigInteger,
        _ => return Err(Error::Stale),
    };
    let mut strategies = vec![PickItem::new("assigned", "Assigned")];
    if matches!(id_type, IdentifierType::Long | IdentifierType::Integer) {
        strategies.extend([
            PickItem::new("identity", "Identity"),
            PickItem::new("sequence", "Sequence"),
            PickItem::new("table", "Table"),
            PickItem::new("auto", "Auto"),
        ]);
    } else if namespace == Namespace::Jakarta
        && matches!(id_type, IdentifierType::Uuid | IdentifierType::String)
    {
        strategies.extend([PickItem::new("uuid", "UUID"), PickItem::new("auto", "Auto")]);
    }
    let strategy = match host::pick("Identifier generation", &strategies)?
        .ok_or(Error::Cancelled)?
        .as_str()
    {
        "assigned" => GenerationStrategy::Assigned,
        "identity" => GenerationStrategy::Identity,
        "sequence" => GenerationStrategy::Sequence,
        "table" => GenerationStrategy::Table,
        "auto" => GenerationStrategy::Auto,
        "uuid" => GenerationStrategy::Uuid,
        _ => return Err(Error::Stale),
    };
    let generator = format!("{}_ids", snake_case(owner));
    let sequence = if strategy == GenerationStrategy::Sequence {
        Some(SequenceGenerator {
            generator: input("Generator name", &generator)?,
            sequence_name: input("Sequence name", &format!("{}_seq", snake_case(owner)))?,
            initial_value: integer("Initial value", "1")?,
            allocation_size: allocation()?,
        })
    } else {
        None
    };
    let table = if strategy == GenerationStrategy::Table {
        Some(TableGenerator {
            generator: input("Generator name", &generator)?,
            table_name: input("Generator table", "id_sequences")?,
            pk_column_name: input("Generator key column", "sequence_name")?,
            value_column_name: input("Generator value column", "next_value")?,
            pk_column_value: input("Generator key value", &snake_case(owner))?,
            initial_value: integer("Initial value", "0")?,
            allocation_size: allocation()?,
        })
    } else {
        None
    };
    let draft = IdentifierDraft {
        name,
        column,
        id_type,
        strategy,
        sequence,
        table,
    };
    draft.validate(namespace).map_err(Error::from)?;
    Ok(draft)
}

pub(super) fn resolve(pending: &Pending, doc: &Document) -> Result<FileAction, Error> {
    super::current(pending, doc)?;
    let (class, namespace) = source(doc).ok_or(Error::Stale)?;
    let draft = prompt(namespace, &class.name)?;
    let masked = mask(&doc.text);
    let at = offset(&doc.text, class.pos).ok_or(Error::Stale)?;
    let end = class.body_end.ok_or(Error::Stale)?;
    let suffix = capitalize(&draft.name);
    if class
        .attributes
        .iter()
        .any(|attribute| capitalize(&attribute.name) == suffix)
        || [
            &draft.name,
            &format!("get{suffix}"),
            &format!("set{suffix}"),
        ]
        .iter()
        .any(|name| has_word(&masked[at..end], name))
    {
        return Err("This source identifier already occurs in the class. Choose another name or edit it manually.".into());
    }
    let mut text = format!(
        "\n{}\n",
        crate::wizard::identifier_source(&draft, namespace, class.kotlin, true, false)
            .map_err(Error::from)?
    );
    if doc.text.contains("\r\n") {
        text = text.replace('\n', "\r\n");
    }
    let edit = TextEdit {
        range: Range::new(position(&doc.text, end), position(&doc.text, end)),
        text,
    };
    let after = super::patched(&doc.text, std::slice::from_ref(&edit))?;
    if !ui_kit::prompts::confirm_exact(
        "Add source identifier?",
        &format!(
            "Target: {}\nSource version: {}\n\n{after}\nCurrent source only; external mappings and project-wide generator name uniqueness are unresolved.",
            doc.path, doc.version
        ),
    )? {
        return Err(Error::Cancelled);
    }
    super::current(pending, doc)?;
    Ok(FileAction {
        title: format!("Add identifier {}", draft.name),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), vec![edit])],
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const JAVA: &str = "package shop;\n@jakarta.persistence.Entity\nclass Invoice {\n    private java.lang.String title;\n}\n";
    fn setup(text: &str, language: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let path = format!(
            "/project/src/main/{language}/shop/Invoice.{}",
            if language == "java" { "java" } else { "kt" }
        );
        host.open_editor(&path, language, text, &[]);
        (
            host,
            JpaAssist::default(),
            Document {
                path,
                language: language.into(),
                text: text.into(),
                version: 1,
            },
        )
    }
    fn action(plugin: &mut JpaAssist, doc: &Document) -> FileAction {
        plugin
            .file_actions(doc, Range::default())
            .into_iter()
            .find(|a| a.title == "Add JPA identifier with preview…")
            .unwrap()
    }
    fn answers(host: &MockHost, rows: &[&[&str]]) {
        for r in rows {
            host.answer_next_prompt(Some(r));
        }
    }
    #[test]
    fn sequence_identifier_is_configurable_and_remains_a_reviewed_single_document_edit() {
        let (host, mut plugin, doc) = setup(JAVA, "java");
        let pending = action(&mut plugin, &doc);
        answers(
            &host,
            &[
                &["key"],
                &["invoice_id"],
                &["integer"],
                &["sequence"],
                &["invoice_ids"],
                &["SEQ_INVOICE"],
                &["5"],
                &["10"],
                &[],
            ],
        );
        let result = plugin.resolve_file_action(&doc, pending).unwrap();
        let after = patched(&doc.text, &result.edits[0].1).unwrap();
        for expected in [
            "private java.lang.Integer key;",
            "generator = \"invoice_ids\"",
            "sequenceName = \"SEQ_INVOICE\"",
            "initialValue = 5, allocationSize = 10",
            "name = \"invoice_id\", nullable = false",
        ] {
            assert!(after.contains(expected), "{after}");
        }
        assert_eq!(host.editor_text(), JAVA);
        assert!(result.operations.is_empty());
    }
    #[test]
    fn property_access_inherited_embedded_existing_ids_and_prefix_shadows_are_not_guessed() {
        let (_host, _, _) = setup(JAVA, "java");
        for unknown in [JAVA.replace("class Invoice {","class Invoice extends Base {"),JAVA.replace("class Invoice {","class Invoice<T> {"),JAVA.replace("Entity","Embeddable"),JAVA.replace("private java.lang.String title;","@jakarta.persistence.Id long id;"),JAVA.replace("private java.lang.String title;","@jakarta.persistence.EmbeddedId Key id;"),JAVA.replace("private java.lang.String title;","@jakarta.persistence.Access(jakarta.persistence.AccessType.PROPERTY) java.lang.String title;")] {
            assert!(source(&Document{text:unknown.clone(),..Document{path:"/project/src/main/java/shop/Invoice.java".into(),language:"java".into(),text:String::new(),version:1}}).is_none(),"{unknown}");
        }
    }
    #[test]
    fn cancellation_and_source_drift_never_apply_changes() {
        let (host, mut plugin, doc) = setup(JAVA, "java");
        let pending = action(&mut plugin, &doc);
        host.answer_next_prompt(None);
        assert!(matches!(
            plugin.resolve_file_action(&doc, pending),
            Err(Error::Cancelled)
        ));
        let pending = action(&mut plugin, &doc);
        host.type_in_editor("// changed");
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert_eq!(host.editor_text(), format!("{JAVA}// changed"));
    }
    #[test]
    fn generated_accessors_do_not_collide_with_existing_methods_or_object_get_class() {
        for (extra, name) in [
            ("java.lang.Long getKey() { return null; }", "key"),
            ("void setKey(java.lang.Long value) {}", "key"),
            ("", "Class"),
        ] {
            let text = JAVA.replace("private java.lang.String title;", extra);
            let (host, mut plugin, doc) = setup(&text, "java");
            let pending = action(&mut plugin, &doc);
            answers(&host, &[&[name], &["id"], &["long"], &["assigned"]]);
            assert!(
                plugin.resolve_file_action(&doc, pending).is_err(),
                "{name}: {text}"
            );
            assert_eq!(host.editor_text(), text);
        }
    }
    #[test]
    fn kotlin_assigned_uuid_and_table_generators_render_distinct_contracts() {
        let kotlin = "package shop\n@javax.persistence.Entity\nclass Invoice {}\n";
        let (host, mut plugin, doc) = setup(kotlin, "kotlin");
        let pending = action(&mut plugin, &doc);
        answers(&host, &[&["key"], &[""], &["uuid"], &["assigned"], &[]]);
        let result = plugin.resolve_file_action(&doc, pending).unwrap();
        let after = patched(&doc.text, &result.edits[0].1).unwrap();
        assert!(
            after.contains("var key: java.util.UUID? = null")
                && after.contains("@javax.persistence.Id")
        );
        assert!(!after.contains("GeneratedValue"));
        let (host, mut plugin, doc) = setup(JAVA, "java");
        let pending = action(&mut plugin, &doc);
        answers(
            &host,
            &[
                &["key"],
                &["id"],
                &["long"],
                &["table"],
                &["ids"],
                &["seq_table"],
                &["seq_name"],
                &["next_val"],
                &["invoices"],
                &["0"],
                &["25"],
                &[],
            ],
        );
        let result = plugin.resolve_file_action(&doc, pending).unwrap();
        let after = patched(&doc.text, &result.edits[0].1).unwrap();
        assert!(
            after.contains("@jakarta.persistence.TableGenerator")
                && after.contains("pkColumnValue = \"invoices\"")
                && after.contains("allocationSize = 25")
        );
    }
}
