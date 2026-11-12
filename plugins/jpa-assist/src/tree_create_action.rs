//! Creation commands for the Project view context menu (`projectView.context`).
//!
//! The host passes the selected files and folders through `Invocation::selected_paths`. Each command works on exactly one
//! selection: New JPA entity takes a source folder (or a source file's folder); the others take an entity or Java/Kotlin
//! source file and reuse the reviewed editor flows through their own eligibility check, so a file a flow does not accept
//! gets a message instead of a guess. The result of a flow is written through the optional `project.write` capability.
use super::*;
use openidea_plugin::FileKind;

struct Command {
    id: &'static str,
    title: &'static str,
}

const COMMANDS: [Command; 6] = [
    Command { id: "tree-new-entity", title: "Generate JPA entity…" },
    Command { id: "tree-new-converter", title: converter_action::TITLE },
    Command { id: "tree-new-user-type", title: hibernate_action::TITLE_TYPE },
    Command { id: "tree-new-repository", title: repository_action::TITLE },
    Command { id: "tree-new-dto", title: "Create scalar DTO with preview…" },
    Command { id: "tree-new-projection", title: projection_action::TITLE },
];

fn selection(ctx: &Invocation) -> Result<String, Error> {
    let mut selected = ctx.selected_paths();
    if selected.is_empty() {
        selected.extend(ctx.editor().map(|editor| editor.path()));
    }
    let [only] = selected.as_slice() else {
        return Err("Select one folder or source file in the Project view.".into());
    };
    path(only).ok_or_else(|| "The selection is not an absolute project path.".into())
}

pub(super) fn run(plugin: &mut JpaAssist, id: &str, ctx: &Invocation) -> Result<Option<String>, Error> {
    run_on(plugin, id, selection(ctx)?)
}

pub(super) fn run_on(plugin: &mut JpaAssist, id: &str, selected: String) -> Result<Option<String>, Error> {
    let command = COMMANDS.iter().find(|command| command.id == id).ok_or_else(|| Error::NotFound(format!("command {id}")))?;
    let project = host::project_of(&selected).ok_or("The selection does not belong to an open project.")?;
    let root = path(&project.root()?).ok_or(Error::Stale)?;
    if !inside(&root, &selected) {
        return Err("The selection is not inside the project.".into());
    }
    let is_source = language_of(&selected).is_some();
    let is_folder = !is_source && project.stat(&selected)?.kind == FileKind::Directory;
    if !is_source && !is_folder {
        return Err("Select a folder or a Java or Kotlin source file.".into());
    }

    let resolved = if id == "tree-new-entity" {
        new_entity(&project, &root, &selected, is_folder)?
    } else {
        if is_folder {
            return Err("Select an entity or source file; this command works on a file.".into());
        }
        let text = project.read_file(&selected)?;
        let doc = Document {
            path: selected.clone(),
            language: if language_of(&selected) == Some(true) { "kotlin" } else { "java" }.into(),
            text,
            version: 0,
        };
        let action = plugin
            .file_actions(&doc, Range::default())
            .into_iter()
            .find(|action| action.title == command.title)
            .ok_or("This command is not available for the selected file.")?;
        plugin.resolve_file_action(&doc, action)?
    };
    apply(&project, &root, resolved)?;
    Ok(None)
}

fn new_entity(project: &Project, root: &str, selected: &str, is_folder: bool) -> Result<FileAction, Error> {
    let directory = if is_folder { selected } else { selected.rsplit_once('/').ok_or(Error::Stale)?.0 };
    if !inside(root, &format!("{directory}/x")) {
        return Err("The folder is not inside the project.".into());
    }
    let relative: Vec<&str> = directory[root.trim_end_matches('/').len()..].trim_matches('/').split('/').collect();
    let roots: Vec<_> = relative
        .windows(3)
        .enumerate()
        .filter(|(_, s)| s[0] == "src" && matches!(s[1], "main" | "test") && matches!(s[2], "java" | "kotlin"))
        .map(|(at, s)| (at + 3, s[2] == "kotlin"))
        .collect();
    let [(start, kotlin)] = roots.as_slice() else {
        return Err("Use a folder under exactly one src/main or src/test source root.".into());
    };
    let package = relative[*start..].join(".");
    if package.is_empty() || !package.split('.').all(identifier) {
        return Err("Select a folder inside a named package of a source root.".into());
    }

    let items = [PickItem::new("jakarta", "Jakarta Persistence"), PickItem::new("javax", "Javax Persistence")];
    let Some(namespace) = host::pick("Persistence namespace for the new entity", &items)? else {
        return Err(Error::Cancelled);
    };
    let extension = if *kotlin { "kt" } else { "java" };
    let probe = format!("{directory}/Entity.{extension}");
    let Some((name, source)) = crate::prompt_entity(Some(&probe), &namespace)? else {
        return Err(Error::Cancelled);
    };
    let target = format!("{directory}/{name}.{extension}");
    if path(&target).is_none() || source.len() > MAX_TEXT || parse_class(&source, &target).is_none() {
        return Err("Generated target exceeds source/path bounds.".into());
    }
    no_collision(project, root, directory, &name, None)?;
    if !ui_kit::prompts::confirm_exact(
        "Create JPA entity?",
        &format!("Namespace: {namespace}.persistence\nTarget: {target}\n\n{source}\nNo existing file is overwritten."),
    )? {
        return Err(Error::Cancelled);
    }
    no_collision(project, root, directory, &name, None)?;
    Ok(FileAction {
        title: format!("Create JPA entity {name}"),
        operations: vec![FileOperation::Create { path: target, text: source, overwrite: false }],
        ..FileAction::default()
    })
}

/// Checks every step first, then writes the new files and the edited text. Nothing is overwritten and no other operation kind is run.
pub(super) fn apply(project: &Project, root: &str, action: FileAction) -> Result<(), Error> {
    let mut creates = Vec::new();
    for operation in &action.operations {
        let FileOperation::Create { path: target, text, overwrite: false } = operation else {
            return Err("The flow asked for an operation this command does not run.".into());
        };
        if path(target).is_none() || !inside(root, target) || project.exists(target)? {
            return Err("Target already exists or is outside the project; nothing was written.".into());
        }
        creates.push((target, text));
    }
    let mut rewrites = Vec::new();
    for (edited, edits) in &action.edits {
        if !inside(root, edited) {
            return Err(Error::Stale);
        }
        let after = patched(&project.read_file(edited)?, edits)?;
        rewrites.push((edited, after));
    }

    for (target, text) in creates {
        project.write_file(target, text)?;
    }
    for (edited, after) in rewrites {
        project.write_file(edited, &after).map_err(|error| match error {
            Error::Denied(_) => error,
            other => Error::from(format!("New files were written, but {edited} could not be updated ({other:?}). Close its unsaved editor changes and apply the edit by hand.")),
        })?;
    }
    host::notify(NotificationLevel::Info, "JPA Assist", &action.title);
    if let Some(open) = action.open {
        let _ = host::open_editor(&open, None);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const CUSTOMER: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Customer {\n    @Id Long id;\n    String name;\n}\n";

    const INVOICE: &str = "package shop;\n\nimport jakarta.persistence.Column;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n\n@Entity\npublic class Invoice {\n    @Id\n    private Long id;\n\n    @Column(name = \"title_text\")\n    private String title;\n}\n";

    fn host() -> MockHost {
        let host = MockHost::install().with_capabilities(&["project.read", "project.write", "editor.read"]);
        host.add_file("src/main/java/shop/Customer.java", CUSTOMER);
        host.add_file("src/main/java/shop/Invoice.java", INVOICE);
        host.add_dir("src/main/java/shop");
        host.add_dir("src/main/kotlin/shop");
        host.add_dir("src/main/java");
        host
    }

    fn text_of(host: &MockHost, relative: &str) -> Option<String> {
        host.invocation().project().unwrap().read_file(&format!("/project/{relative}")).ok()
    }

    fn run_on(host: &MockHost, id: &str, selected: &[&str]) -> Result<Option<String>, Error> {
        host.select_paths(selected);
        JpaAssist::default().run_command(id, &host.invocation(), None)
    }

    #[test]
    fn new_entity_in_a_package_folder_writes_one_reviewed_file() {
        let host = host();
        for row in [&["jakarta"][..], &["Tag"], &["tag"], &["IDENTITY"], &["version"], &[]] {
            host.answer_next_prompt(Some(row));
        }
        run_on(&host, "tree-new-entity", &["/project/src/main/java/shop"]).unwrap();
        let written = text_of(&host, "src/main/java/shop/Tag.java").unwrap();
        assert!(written.contains("package shop;") && written.contains("@Table(name = \"tag\")") && written.contains("Long version"), "{written}");
    }

    #[test]
    fn a_source_file_selection_uses_its_folder_and_kotlin_roots() {
        let host = host();
        host.add_file("src/main/kotlin/shop/Cart.kt", "package shop\n");
        for row in [&["javax"][..], &["Tag"], &["tag"], &["none"], &[], &[]] {
            host.answer_next_prompt(Some(row));
        }
        run_on(&host, "tree-new-entity", &["/project/src/main/kotlin/shop/Cart.kt"]).unwrap();
        let written = text_of(&host, "src/main/kotlin/shop/Tag.kt").unwrap();
        assert!(written.contains("class Tag") && written.contains("javax.persistence.Entity") && !written.contains(';'), "{written}");
    }

    #[test]
    fn entity_creation_refuses_unnamed_packages_existing_types_and_non_folders() {
        let host = host();
        for selected in ["/project/src/main/java", "/project", "/project/README.md", "/project/src/main/java/shop/../shop"] {
            host.add_file("README.md", "x");
            assert!(run_on(&host, "tree-new-entity", &[selected]).is_err(), "{selected}");
        }
        for row in [&["jakarta"][..], &["Customer"]] {
            host.answer_next_prompt(Some(row));
        }
        assert!(run_on(&host, "tree-new-entity", &["/project/src/main/java/shop"]).is_err());
        assert!(text_of(&host, "src/main/java/shop/Customer.java").unwrap() == CUSTOMER);
        assert!(run_on(&host, "tree-new-entity", &["/project/src/main/java/shop", "/project/src/main/java/shop/Customer.java"]).is_err());
    }

    #[test]
    fn dismissing_a_prompt_writes_nothing() {
        let host = host();
        host.answer_next_prompt(None);
        assert!(matches!(run_on(&host, "tree-new-entity", &["/project/src/main/java/shop"]), Err(Error::Cancelled)));
        for row in [&["jakarta"][..], &["Tag"], &["tag"], &["IDENTITY"], &[]] {
            host.answer_next_prompt(Some(row));
        }
        host.answer_next_prompt(None);
        assert!(run_on(&host, "tree-new-entity", &["/project/src/main/java/shop"]).is_err());
        assert!(text_of(&host, "src/main/java/shop/Tag.java").is_none());
    }

    #[test]
    fn repository_command_reuses_the_reviewed_flow_and_creates_the_file() {
        let host = host();
        for row in [&["JpaRepository"][..], &["no"], &["CustomerRepository"], &[]] {
            host.answer_next_prompt(Some(row));
        }
        run_on(&host, "tree-new-repository", &["/project/src/main/java/shop/Customer.java"]).unwrap();
        let written = text_of(&host, "src/main/java/shop/CustomerRepository.java").unwrap();
        assert!(written.contains("interface CustomerRepository extends JpaRepository<Customer, Long>"), "{written}");
        assert_eq!(text_of(&host, "src/main/java/shop/Customer.java").unwrap(), CUSTOMER);
    }

    #[test]
    fn converter_command_creates_the_file_and_edits_the_entity() {
        let host = host();
        for row in [&["title"][..], &["Integer"], &["TitleConverter"], &["explicit"], &[]] {
            host.answer_next_prompt(Some(row));
        }
        run_on(&host, "tree-new-converter", &["/project/src/main/java/shop/Invoice.java"]).unwrap();
        assert!(text_of(&host, "src/main/java/shop/TitleConverter.java").unwrap().contains("AttributeConverter<String, Integer>"));
        assert!(text_of(&host, "src/main/java/shop/Invoice.java").unwrap().contains("@Convert(converter = TitleConverter.class)"));
    }

    #[test]
    fn an_existing_target_stops_before_anything_is_written() {
        let host = host();
        host.add_file("src/main/java/shop/TitleConverter.java", "class TitleConverter {}");
        for row in [&["title"][..], &["Integer"], &["TitleConverter"], &["explicit"], &[]] {
            host.answer_next_prompt(Some(row));
        }
        assert!(run_on(&host, "tree-new-converter", &["/project/src/main/java/shop/Invoice.java"]).is_err());
        assert_eq!(text_of(&host, "src/main/java/shop/Invoice.java").unwrap(), INVOICE);
    }

    #[test]
    fn flows_that_do_not_accept_the_file_say_so_and_folders_are_refused() {
        let host = host();
        host.add_file("src/main/java/shop/Plain.java", "package shop;\nclass Plain {}\n");
        for id in ["tree-new-converter", "tree-new-user-type", "tree-new-dto", "tree-new-projection"] {
            let error = run_on(&host, id, &["/project/src/main/java/shop/Plain.java"]).unwrap_err();
            assert!(format!("{error:?}").contains("not available"), "{id}: {error:?}");
            assert!(run_on(&host, id, &["/project/src/main/java/shop"]).is_err());
        }
        assert!(run_on(&host, "tree-new-unknown", &["/project/src/main/java/shop"]).is_err());
    }

    #[test]
    fn without_the_write_capability_nothing_is_written() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/Customer.java", CUSTOMER);
        for row in [&["jakarta"][..], &["Tag"], &["tag"], &["IDENTITY"], &[], &[]] {
            host.answer_next_prompt(Some(row));
        }
        assert!(run_on(&host, "tree-new-entity", &["/project/src/main/java/shop"]).is_err());
        assert!(text_of(&host, "src/main/java/shop/Tag.java").is_none());
    }
}
