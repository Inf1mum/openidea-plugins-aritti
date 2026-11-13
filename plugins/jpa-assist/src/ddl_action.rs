//! Reviewed scalar SQL drafts. A draft never executes or claims an effective schema.
use super::*;
pub(super) const TITLE: &str = "Create scalar SQL table draft with preview…";
fn sql_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
pub(super) fn source(doc: &Document) -> Option<Class> {
    if doc.language != "java" {
        return None;
    }
    let class = dto_action::source(doc)?;
    let ns = namespace(&doc.text, &class)?;
    let masked = mask(&doc.text);
    let anns = annotations(&masked);
    // This action only supports plain field access and explicit manual IDs.
    if anns.iter().any(|a| {
        !matches!(a.name, "Entity" | "Id")
            || a.args.is_some()
            || !annotation_is(&doc.text, &class, a.name, ns)
    }) || anns.iter().filter(|a| a.name == "Entity").count() != 1
        || anns.iter().filter(|a| a.name == "Id").count() != 1
        || masked.contains(['(', '=', '<', '>'])
        || masked.matches('{').count() != 1
        || masked.matches('}').count() != 1
        || class.imports.iter().any(|import| {
            import.ends_with(".*") && import != &format!("{ns}.*") && import != "java.lang.*"
        })
        || class.attributes.is_empty()
        || class.attributes.len() > 32
        || class.attributes.iter().filter(|a| a.id).count() != 1
    {
        return None;
    }
    let mut names = HashSet::new();
    for attribute in &class.attributes {
        let column = super::naming_column_name(&attribute.name);
        if attribute.relation != Relation::None
            || attribute.generated.is_some()
            || attribute.ty.rsplit('.').next() == Some(class.name.as_str())
            || !dto_action::supported(&class, &attribute.ty)
            || !sql_name(&column)
            || !names.insert(column)
            || crate::generate::sql_type(attribute, Dialect::PostgreSql).is_none()
            || crate::generate::sql_type(attribute, Dialect::MySql).is_none()
        {
            return None;
        }
    }
    Some(class)
}
pub(super) fn eligible(doc: &Document) -> bool {
    source(doc).is_some()
}
fn absent(project: &Project, root: &str, target: &str) -> Result<(), Error> {
    if host::project_of(target)
        .and_then(|p| p.root().ok())
        .as_deref()
        != Some(root)
    {
        return Err(Error::Stale);
    }
    match project.read_file(target) {
        Ok(_) => {
            return Err(
                "SQL draft target exists or has a current buffer; choose another file stem.".into(),
            );
        }
        Err(Error::NotFound(_)) => {}
        Err(error) => return Err(error),
    }
    let (directory, filename) = target.rsplit_once('/').ok_or(Error::Stale)?;
    let relative = directory
        .strip_prefix(&format!("{}/", root.trim_end_matches('/')))
        .ok_or(Error::Stale)?;
    let files = project.find_files(&format!("{relative}/*"), 129)?;
    if files.len() >= 129 {
        return Err("Target folder exceeds bounded SQL draft collision discovery.".into());
    }
    if files.iter().any(|file| {
        file.rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(filename))
    }) {
        return Err("A case-insensitive SQL draft filename collision exists.".into());
    }
    Ok(())
}
fn sql(class: &Class, table: &str, dialect: Dialect, policy: crate::naming_strategy::IdentifierPolicy, options: &super::ddl_options::DdlOptions) -> Option<String> {
    let quote = if dialect == Dialect::MySql { '`' } else { '"' };
    let quoted = |name: &str| format!("{quote}{}{quote}", options.shorten(&policy.apply(name)));
    let mut cased = HashSet::new();
    let mut fields = Vec::new();
    let mut key = None;
    for attribute in &class.attributes {
        let name = quoted(&super::naming_column_name(&attribute.name));
        if !cased.insert(name.clone()) {
            return None;
        }
        let ty = crate::generate::sql_type(attribute, dialect)?;
        fields.push(format!(
            "    {name} {ty}{}",
            if attribute.id { " NOT NULL" } else { "" }
        ));
        if attribute.id {
            key = Some(name);
        }
    }
    let constraint = options.primary_key_constraint(table).map_or_else(String::new, |name| format!("CONSTRAINT {} ", quoted(&name)));
    fields.push(format!("    {constraint}PRIMARY KEY ({})", key?));
    Some(format!(
        "-- Source-derived scalar draft; review before external use.\n-- Snake-case column names and nullable non-ID columns are draft choices.\n-- No database, naming strategy, converter, schema or deployment validation.\nCREATE TABLE {} (\n{}\n);\n",
        quoted(table),
        fields.join(",\n")
    ))
}
pub(super) fn resolve(
    pending: &Pending,
    doc: &Document,
    project: &Project,
) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let class = source(doc)
        .ok_or("This source is outside the plain Java scalar field-access SQL draft subset.")?;
    let scratch = super::ddl_target::pick_scratch()?;
    let mut choices = [
        PickItem::new("postgresql", "PostgreSQL scalar draft"),
        PickItem::new("mysql", "MySQL scalar draft"),
    ];
    crate::datasource_facts::prefer(project, &mut choices);
    let selected = host::pick("SQL draft dialect (no database connection)", &choices)?
        .ok_or(Error::Cancelled)?;
    let dialect = match selected.as_str() {
        "postgresql" => Dialect::PostgreSql,
        "mysql" => Dialect::MySql,
        _ => return Err(Error::Stale),
    };
    let table = host::input(
        "Draft table name (one unqualified ASCII identifier)",
        Some(&super::naming_table_name(class.table.as_deref().unwrap_or(&class.name))),
    )?
    .ok_or(Error::Cancelled)?;
    if !sql_name(&table) {
        return Err("Use an unqualified ASCII SQL identifier of at most 63 bytes.".into());
    }
    let stem = host::input(
        "SQL draft filename stem (beside this entity)",
        Some(&format!("{}Schema", class.name)),
    )?
    .ok_or(Error::Cancelled)?;
    if !identifier(&stem) {
        return Err(
            "Use a Java-style ASCII filename stem of at most 128 bytes; .sql is appended.".into(),
        );
    }
    let entity_directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let directory = if scratch {
        super::ddl_target::scratch_directory(&pending.root, host::setting_string(super::ddl_target::SCRATCH_SETTING).as_deref())?
    } else {
        let default = entity_directory.strip_prefix(&format!("{}/", pending.root.trim_end_matches('/'))).unwrap_or_default();
        let chosen = host::input("Directory for the SQL draft (relative to the project root)", Some(default))?.ok_or(Error::Cancelled)?;
        super::ddl_target::directory(&pending.root, &chosen)?
    };
    let target = if scratch {
        super::ddl_target::unique_target(project, &directory, &stem)?
    } else {
        format!("{directory}/{stem}.sql")
    };
    let target_kind = if scratch { "Scratch file (a plain project file; not the IDE scratch system)" } else { "File" };
    let check_free = |target: &str| {
        if scratch {
            return match super::ddl_target::exists(project, target)? {
                true => Err("SQL draft target exists or has a current buffer; choose another file stem.".into()),
                false => Ok(()),
            };
        }
        absent(project, &pending.root, target)
    };
    if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
        return Err(Error::Stale);
    }
    let policy = crate::naming_strategy::IdentifierPolicy::from_setting(host::setting_string("jpaAssist.identifierPolicy").as_deref());
    let strategy = crate::naming_strategy::detect(project);
    let options = super::ddl_options::DdlOptions::from_settings()?;
    let mut shortened = HashSet::new();
    for attribute in &class.attributes {
        let column = options.shorten(&policy.apply(&super::naming_column_name(&attribute.name)));
        if !shortened.insert(column.clone()) {
            return Err(format!("jpaAssist.maxIdentifierLength shortens column {} to {column}, which another column already has; raise the limit.", attribute.name).into());
        }
    }
    let generated = sql(&class, &table, dialect, policy, &options).ok_or(Error::Stale)?;
    let limits = format!(
        "Maximum identifier length: {}. Primary key constraint: {}.",
        options.max_identifier.map_or("unlimited".to_owned(), |n| format!("{n} bytes (longer names are cut)")),
        options.primary_key_constraint(&table).map_or("unnamed".to_owned(), |name| format!("{}{}", options.shorten(&policy.apply(&name)), if dialect == Dialect::MySql { " (MySQL always names a primary key PRIMARY)" } else { "" })),
    );
    current(pending, doc)?;
    check_free(&target)?;
    let preview = format!(
        "Source: {}\nSource version: {}\nPackage: {}\nDialect: {selected}\nTable: {table}\nSave target: {target_kind}\nTarget: {target}\n\nAll {} parsed scalar fields, one manual primary key. Non-ID columns are nullable draft choices. Detected physical naming strategy: {}; the names below come from the naming templates, not from it. Identifier policy: {policy:?} (all names are quoted). {limits} No DB/schema diff, generated IDs, converters, relationships or deployment check.\n\nExact new SQL file:\n{generated}\nOne atomic non-overwriting Create. This action never executes SQL.",
        doc.path,
        doc.version,
        package(doc).ok_or(Error::Stale)?,
        class.attributes.len(),
        strategy.physical_label().map_or_else(|| "not verified".to_owned(), |label| format!("{label} ({})", strategy.source))
    );
    if !ui_kit::prompts::confirm_exact("Create reviewed scalar SQL draft?", &preview)? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    check_free(&target)?;
    Ok(FileAction {
        title: format!("Create scalar SQL draft {stem}.sql"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create {
            path: target.clone(),
            text: generated,
            overwrite: false,
        }],
        open: Some(target),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const JAVA: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\nclass Customer {\n@Id Long id;\nString displayName;\nboolean active;\n}\n";
    fn setup() -> (MockHost, JpaAssist, Document) {
        let h = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let path = "/project/src/main/java/shop/Customer.java";
        h.open_editor(path, "java", JAVA, &[]);
        (
            h,
            JpaAssist::default(),
            Document {
                path: path.into(),
                language: "java".into(),
                version: 1,
                text: JAVA.into(),
            },
        )
    }
    fn action(p: &mut JpaAssist, d: &Document) -> FileAction {
        p.file_actions(d, Range::default())
            .into_iter()
            .find(|a| a.title == TITLE)
            .unwrap()
    }
    fn answer(h: &MockHost, dialect: &str, table: &str, stem: &str, confirm: bool) {
        for value in ["file", dialect, table, stem, "src/main/java/shop"] {
            h.answer_next_prompt(Some(&[value]));
        }
        h.answer_next_prompt(confirm.then_some(&[]));
    }
    fn answer_scratch(h: &MockHost, stem: &str) {
        for value in ["scratch", "postgresql", "customers", stem] {
            h.answer_next_prompt(Some(&[value]));
        }
        h.answer_next_prompt(Some(&[]));
    }
    #[test]
    fn file_target_asks_for_the_directory_and_keeps_it_inside_the_project() {
        let (h, mut p, d) = setup();
        let a = action(&mut p, &d);
        for value in ["file", "mysql", "customers", "Moved", "db/drafts"] {
            h.answer_next_prompt(Some(&[value]));
        }
        h.answer_next_prompt(Some(&[]));
        let resolved = p.resolve_file_action(&d, a).unwrap();
        let [FileOperation::Create { path, overwrite: false, .. }] = resolved.operations.as_slice() else { panic!("one create") };
        assert_eq!(path, "/project/db/drafts/Moved.sql");
        let (h, mut p, d) = setup();
        let a = action(&mut p, &d);
        for value in ["file", "mysql", "customers", "Moved", "../outside"] {
            h.answer_next_prompt(Some(&[value]));
        }
        assert!(p.resolve_file_action(&d, a).is_err());
    }
    #[test]
    fn scratch_target_uses_the_setting_directory_and_a_unique_name_without_a_directory_prompt() {
        let (h, mut p, d) = setup();
        let a = action(&mut p, &d);
        answer_scratch(&h, "Draft");
        let resolved = p.resolve_file_action(&d, a).unwrap();
        let [FileOperation::Create { path, overwrite: false, .. }] = resolved.operations.as_slice() else { panic!("one create") };
        assert_eq!(path, "/project/.openidea/scratches/Draft.sql");
        let (h, mut p, d) = setup();
        h.add_file(".openidea/scratches/Draft.sql", "old");
        let a = action(&mut p, &d);
        answer_scratch(&h, "Draft");
        let resolved = p.resolve_file_action(&d, a).unwrap();
        let [FileOperation::Create { path, .. }] = resolved.operations.as_slice() else { panic!("one create") };
        assert_eq!(path, "/project/.openidea/scratches/Draft2.sql");
    }
    #[test]
    fn scalar_sql_draft_is_exact_quoted_nonoverwriting_create_without_writes_or_processes() {
        for (dialect, quote, boolean) in [("postgresql", '"', "BOOLEAN"), ("mysql", '`', "BIT(1)")]
        {
            let (h, mut p, d) = setup();
            let pending = action(&mut p, &d);
            assert!(pending.operations.is_empty());
            answer(&h, dialect, "order", "CustomerDraft", true);
            let resolved = p.resolve_file_action(&d, pending).unwrap();
            let [
                FileOperation::Create {
                    path,
                    text,
                    overwrite: false,
                },
            ] = resolved.operations.as_slice()
            else {
                panic!("one create")
            };
            assert_eq!(path, "/project/src/main/java/shop/CustomerDraft.sql");
            assert_eq!(resolved.open.as_ref(), Some(path));
            assert!(text.contains(&format!("CREATE TABLE {quote}order{quote}")));
            assert!(text.contains(&format!("{quote}display_name{quote} VARCHAR(255)")));
            assert!(text.contains(&format!("{quote}active{quote} {boolean}")));
            assert!(text.contains(&format!("PRIMARY KEY ({quote}id{quote})")));
            assert!(resolved.edits.is_empty());
            assert_eq!(h.editor_text(), JAVA);
            assert!(h.invocation().project().unwrap().read_file(path).is_err());
            assert!(h.requests().is_empty() && h.process_runs().is_empty());
        }
    }
    #[test]
    fn unsupported_mapping_types_and_ambiguous_columns_do_not_offer_draft() {
        let (_h, mut p, d) = setup();
        for text in [
            JAVA.replace("@Id Long id;", "@Id @GeneratedValue Long id;"),
            JAVA.replace(
                "String displayName;",
                "@Column(name=\"name\") String displayName;",
            ),
            JAVA.replace("String displayName;", "Other displayName;"),
            JAVA.replace(
                "String displayName;",
                "String displayName; String display_name;",
            ),
            JAVA.replace("class Customer {", "class Customer extends Base {"),
            JAVA.replace("String displayName;", "String displayName = call();"),
            JAVA.replace("class Customer {", "class Customer<T> {"),
            format!("{JAVA}}}"),
            JAVA.replace(
                "import jakarta.persistence.Id;",
                "import jakarta.persistence.Id;\nimport foreign.*;",
            ),
            JAVA.replace("@Id Long id;", "@Id @foreign.Id Long id;"),
            JAVA.replace(
                "String displayName;",
                "String displayName; void custom() {}",
            ),
        ] {
            let d = Document { text, ..d.clone() };
            assert!(
                !p.file_actions(&d, Range::default())
                    .iter()
                    .any(|a| a.title == TITLE),
                "{}",
                d.text
            );
        }
    }
    #[test]
    fn identifier_policy_changes_case_of_quoted_names_and_defaults_to_as_written() {
        use crate::naming_strategy::IdentifierPolicy;
        let (_host, _plugin, doc) = setup();
        let class = source(&doc).unwrap();
        let written = sql(&class, "Customers", Dialect::PostgreSql, IdentifierPolicy::AsWritten, &Default::default()).unwrap();
        assert!(written.contains("CREATE TABLE \"Customers\"") && written.contains("\"display_name\""), "{written}");
        let upper = sql(&class, "Customers", Dialect::MySql, IdentifierPolicy::Uppercase, &Default::default()).unwrap();
        assert!(upper.contains("CREATE TABLE `CUSTOMERS`") && upper.contains("`DISPLAY_NAME`"), "{upper}");
    }
    #[test]
    fn max_identifier_length_and_primary_key_name_shape_the_draft() {
        use crate::naming_strategy::IdentifierPolicy;
        use super::super::ddl_options::DdlOptions;
        let (_host, _plugin, doc) = setup();
        let class = source(&doc).unwrap();
        let options = DdlOptions::parse(Some("8"), Some("pk_{table}")).unwrap();
        let text = sql(&class, "customer_accounts", Dialect::PostgreSql, IdentifierPolicy::AsWritten, &options).unwrap();
        assert!(text.contains("CREATE TABLE \"customer\"") && text.contains("\"display_\" VARCHAR(255)"), "{text}");
        assert!(text.contains("CONSTRAINT \"pk_custo\" PRIMARY KEY (\"id\")"), "{text}");
        let named = DdlOptions::parse(None, Some("pk_{table}")).unwrap();
        assert!(sql(&class, "customer", Dialect::MySql, IdentifierPolicy::AsWritten, &named).unwrap().contains("CONSTRAINT `pk_customer` PRIMARY KEY (`id`)"));
    }

    #[test]
    fn cancellation_invalid_inputs_collisions_and_stale_or_foreign_source_return_no_operations() {
        let (h, mut p, d) = setup();
        let a = action(&mut p, &d);
        answer(&h, "postgresql", "customers", "Draft", false);
        assert!(matches!(
            p.resolve_file_action(&d, a),
            Err(Error::Cancelled)
        ));
        for (table, stem) in [
            ("public.customers", "Draft"),
            ("a;drop", "Draft"),
            ("good", "../Draft"),
        ] {
            let (h, mut p, d) = setup();
            let a = action(&mut p, &d);
            answer(&h, "postgresql", table, stem, true);
            assert!(p.resolve_file_action(&d, a).is_err());
        }
        for path in ["Draft.sql", "draft.sql"] {
            let (h, mut p, d) = setup();
            h.add_file(&format!("src/main/java/shop/{path}"), "old");
            let a = action(&mut p, &d);
            answer(&h, "postgresql", "customers", "Draft", true);
            assert!(p.resolve_file_action(&d, a).is_err());
        }
        let (h, mut p, d) = setup();
        let a = action(&mut p, &d);
        h.open_editor(
            "/project/src/main/java/shop/Draft.sql",
            "sql",
            "unsaved",
            &[],
        );
        answer(&h, "mysql", "customers", "Draft", true);
        assert!(p.resolve_file_action(&d, a).is_err());
        let (_h, mut p, d) = setup();
        let a = action(&mut p, &d);
        let changed = Document {
            version: 2,
            ..d.clone()
        };
        assert!(matches!(
            p.resolve_file_action(&changed, a),
            Err(Error::Stale)
        ));
        let (h, mut p, d) = setup();
        let a = action(&mut p, &d);
        h.add_project("/project/src/main/java/shop");
        assert!(matches!(p.resolve_file_action(&d, a), Err(Error::Stale)));
    }
}
