//! Applies a resolved `FileAction` from a command: new files go through the
//! project, edits through the open editor. Not one undo step.
use crate::denied;
use openidea_plugin::file_actions::{FileAction, FileOperation};
use openidea_plugin::{Error, Project, TextEdit, TextEditor, host, text};

const ONLY_FROM_CODE_ACTIONS: &str = "This action can only be applied from ⌥⏎.";

fn unsupported() -> Error {
    Error::Invalid(ONLY_FROM_CODE_ACTIONS.into())
}

pub fn apply(
    project: &Project,
    editor: Option<&TextEditor>,
    action: &FileAction,
    plugin: &str,
) -> Result<(), Error> {
    let mut created: Vec<(&str, String)> = Vec::new();
    for operation in &action.operations {
        let FileOperation::Create { path, text, overwrite: false } = operation else {
            return Err(unsupported());
        };
        if project.exists(path)? {
            return Err(Error::Invalid(format!("{path} already exists; nothing was written.")));
        }
        created.push((path, text.clone()));
    }

    let mut editor_edits: &[TextEdit] = &[];
    for (path, edits) in &action.edits {
        if let Some((_, text)) = created.iter_mut().find(|(created_path, _)| created_path == path) {
            apply_to_text(text, edits)?;
            continue;
        }
        match editor {
            Some(editor) if editor.path() == *path => editor_edits = edits,
            _ => return Err(unsupported()),
        }
    }

    for (path, text) in &created {
        project
            .write_file(path, text)
            .map_err(|e| denied::map(e, "Creating files", "project.write", plugin))?;
    }

    if let Some(editor) = editor.filter(|_| !editor_edits.is_empty()) {
        let version = editor.document()?.version;
        editor
            .apply_edits(version, editor_edits)
            .map_err(|e| denied::map(e, "Editing the file", "editor.write", plugin))?;
    }

    if let Some(path) = &action.open {
        host::open_location(path, 0, 0)?;
    }

    Ok(())
}

fn apply_to_text(text: &mut String, edits: &[TextEdit]) -> Result<(), Error> {
    let mut spans = Vec::with_capacity(edits.len());
    for edit in edits {
        let start = text::offset(text, edit.range.start);
        let end = text::offset(text, edit.range.end);
        match (start, end) {
            (Some(a), Some(b)) if a <= b => spans.push((a, b, edit.text.as_str())),
            _ => return Err(Error::Invalid("An edit lies outside the new file.".into())),
        }
    }
    spans.sort_by_key(|(a, b, _)| (*a, *b));
    for (a, b, replacement) in spans.into_iter().rev() {
        text.replace_range(a..b, replacement);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{Position, Range};
    use openidea_plugin::mock::MockHost;

    fn create(path: &str, text: &str) -> FileOperation {
        FileOperation::Create { path: path.into(), text: text.into(), overwrite: false }
    }

    fn writable() -> MockHost {
        MockHost::install().with_capabilities(&["project.read", "project.write", "editor.read", "editor.write"])
    }

    fn run(host: &MockHost, action: &FileAction) -> Result<(), Error> {
        let invocation = host.invocation();
        apply(&invocation.project().unwrap(), invocation.editor().as_ref(), action, "P")
    }

    #[test]
    fn create_writes_new_file_and_opens_it() {
        let host = writable();
        let action = FileAction {
            operations: vec![create("a/b.txt", "x")],
            open: Some("a/b.txt".into()),
            ..FileAction::default()
        };

        run(&host, &action).unwrap();

        let invocation = host.invocation();
        assert_eq!(invocation.project().unwrap().read_file("a/b.txt").unwrap(), "x");
        assert_eq!(host.opened_locations(), vec![("a/b.txt".to_string(), 0, 0)]);
    }

    #[test]
    fn create_refuses_existing_file() {
        let host = MockHost::install();
        host.add_file("a/b.txt", "old");
        let action = FileAction { operations: vec![create("a/b.txt", "x")], ..FileAction::default() };

        let result = run(&host, &action);

        assert!(matches!(result, Err(Error::Invalid(m)) if m.contains("already exists")));
        assert_eq!(host.invocation().project().unwrap().read_file("a/b.txt").unwrap(), "old");
    }

    #[test]
    fn edits_apply_to_open_editor() {
        let host = writable();
        host.open_editor("log.xml", "xml", "<a/>", &[]);
        let insert = TextEdit { range: Range::new(Position::new(0, 0), Position::new(0, 0)), text: "<!-- x -->".into() };
        let action = FileAction { edits: vec![("log.xml".into(), vec![insert])], ..FileAction::default() };

        run(&host, &action).unwrap();

        assert_eq!(host.editor_text(), "<!-- x --><a/>");
    }

    #[test]
    fn edits_for_created_file_are_applied_before_writing() {
        let host = writable();
        let insert = TextEdit { range: Range::new(Position::new(0, 1), Position::new(0, 1)), text: "Y".into() };
        let action = FileAction {
            operations: vec![create("n.txt", "ab")],
            edits: vec![("n.txt".into(), vec![insert])],
            ..FileAction::default()
        };

        run(&host, &action).unwrap();

        assert_eq!(host.invocation().project().unwrap().read_file("n.txt").unwrap(), "aYb");
    }

    #[test]
    fn denied_write_names_project_write() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        let action = FileAction { operations: vec![create("a.txt", "x")], ..FileAction::default() };

        let result = run(&host, &action);

        assert!(matches!(result, Err(Error::Invalid(m)) if m.contains("project.write") && m.contains("Settings › Plugins › P")));
    }

    #[test]
    fn rename_is_refused_without_side_effects() {
        let host = MockHost::install();
        let action = FileAction {
            operations: vec![create("a.txt", "x"), FileOperation::Rename { from: "a".into(), to: "b".into() }],
            ..FileAction::default()
        };

        let result = run(&host, &action);

        assert!(matches!(result, Err(Error::Invalid(m)) if m.contains("⌥⏎")));
        assert!(host.invocation().project().unwrap().read_file("a.txt").is_err());
    }
}
