//! Exact dotfile creation and snapshot-bound append operations. FileActions are
//! the mutation boundary; no shell writes participate in creating ignore files.
use super::*;
use openidea_plugin::file_actions::{FileAction, FileActions, FileOperation};
use std::hash::{Hash, Hasher};
const MAX: usize = 256 << 10;
fn fingerprint(text: &str) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hash);
    hash.finish()
}
fn lazy(doc: &Document, root: &str, title: String, kind: &str, arg: &str, expected: u64) -> FileAction {
    FileAction {
        title,
        kind: Some("source".into()),
        data: Some(serde_json::json!([root, doc.path, doc.version, kind, arg, expected]).to_string()),
        ..FileAction::default()
    }
}
fn current(doc: &Document, root: &str) -> Result<Project, Error> {
    let project = host::project_of(&doc.path).ok_or(Error::Stale)?;
    if project.root()?.trim_end_matches('/') != root {
        return Err(Error::Stale);
    }
    Ok(project)
}
fn vacant(project: &Project, target: &str) -> Result<(), Error> {
    if project.exists(target)? {
        return Err("The ignore file already exists; nothing will be overwritten.".into());
    }
    match project.read_file(target) {
        Ok(_) => Err("The destination has an open buffer; choose another location.".into()),
        Err(Error::NotFound(_)) => Ok(()),
        Err(error) => Err(error),
    }
}
fn directory(project: &Project, root: &str, input: &str) -> Result<String, Error> {
    let input = input.trim();
    if input.len() > 512
        || input.chars().any(char::is_control)
        || input.starts_with('/')
        || input.contains('\\')
        || input.contains(':')
        || input.split('/').any(|p| matches!(p, "." | ".."))
    {
        return Err("Use an existing project-relative directory, without traversal or control characters; leave empty for the root.".into());
    }
    let path = if input.is_empty() { root.to_owned() } else { format!("{root}/{input}") };
    if project.stat(&path)?.kind != openidea_plugin::FileKind::Directory {
        return Err("Choose an existing directory inside the project.".into());
    }
    Ok(path)
}
fn source_unchanged(doc: &Document, root: &str) -> Result<Project, Error> {
    let project = current(doc, root)?;
    if project.read_file(&doc.path)? != doc.text {
        return Err(Error::Stale);
    }
    Ok(project)
}

impl FileActions for IgnorePlugin {
    fn file_actions(&mut self, doc: &Document, _selection: Range) -> Vec<FileAction> {
        let Some((project, root, relative)) = project_of(&doc.path) else { return Vec::new() };
        if doc.text.len() > MAX {
            return Vec::new();
        }
        let mut out = Vec::new();
        if !preferences::creatable_names().is_empty() {out.push(lazy(doc,&root,"Create exact-name ignore file…".into(),"create","",0));}
        if dialect(&doc.path).is_none() {
            let target = format!("{root}/.gitignore");
            if let Ok(content) = project.read_file(&target) {
                if !preferences::can_edit(&target) {return out;}
                if content.len() <= MAX {
                    if let Ok(entry) = literal_entry(&relative) {
                        for (line, note) in [(entry.clone(), ""), (format!("!{entry}"), " (unignore)")] {
                            if !setting("unignoreActions", true) && !note.is_empty() {
                                continue;
                            }
                            if !content.lines().any(|old| old == line) {
                                out.push(lazy(
                                    doc,
                                    &root,
                                    format!("Add {line} to .gitignore{note}"),
                                    "entry",
                                    &line,
                                    fingerprint(&content),
                                ));
                            }
                        }
                    }
                }
            }
        } else if preferences::can_edit(&doc.path) && dialect(&doc.path).is_some_and(|d| d.syntax == check::Syntax::Glob) {
            out.push(lazy(doc, &root, "Append ignore template…".into(), "template", "", fingerprint(&doc.text)));
        }
        out
    }
    fn resolve_file_action(&mut self, doc: &Document, action: FileAction) -> Result<FileAction, Error> {
        let data: serde_json::Value = serde_json::from_str(action.data.as_deref().ok_or("Missing ignore action context.")?)
            .map_err(|_| "Invalid ignore action context.")?;
        let p = data.as_array().filter(|p| p.len() == 6).ok_or("Invalid ignore action context.")?;
        let root = p[0].as_str().ok_or("Invalid project.")?;
        if p[1].as_str() != Some(&doc.path) || p[2].as_u64() != Some(doc.version) {
            return Err(Error::Stale);
        }
        let project = current(doc, root)?;
        let kind = p[3].as_str().ok_or("Invalid operation.")?;
        if kind == "create" {
            let names=preferences::creatable_names();
            if names.is_empty() {return Err("No enabled Ignore Create actions are available.".into());}
            let items: Vec<_> = names.iter().map(|n| PickItem::new(n, n)).collect();
            let name = host::pick("Ignore file type (exact filename)", &items)?.ok_or(Error::Cancelled)?;
            if !names.contains(&name.as_str()) || !preferences::creation(&name) {
                return Err(Error::Stale);
            }
            let folder = host::input("Existing directory inside project (empty for root)", Some(""))?.ok_or(Error::Cancelled)?;
            let folder = directory(&project, root, &folder)?;
            let target = format!("{folder}/{name}");
            vacant(&project, &target)?;
            let content = if name == ".gitignore" {
                include_str!("../file-templates/gitignore").to_owned()
            } else if name == ".hgignore" {
                "syntax: glob\n".into()
            } else {
                format!("# {name}\n")
            };
            if !ui_kit::prompts::confirm_exact("Create ignore file?", &format!("{}\n\n{}", serde_json::to_string(&target).unwrap_or_default(), content))? {
                return Err(Error::Cancelled);
            }
            let project = source_unchanged(doc, root)?;
            if !preferences::creation(&name) {return Err(Error::Stale);}
            directory(&project, root, folder.strip_prefix(root).unwrap_or("").trim_start_matches('/'))?;
            vacant(&project, &target)?;
            return Ok(FileAction {
                title: action.title,
                operations: vec![FileOperation::Create { path: target.clone(), text: content, overwrite: false }],
                open: Some(target),
                ..FileAction::default()
            });
        }
        let (target, before, edit) = match kind {
            "entry" => {
                let relative = doc.path.strip_prefix(&format!("{root}/")).ok_or(Error::Stale)?;
                let entry = literal_entry(relative)?;
                let line = p[4].as_str().ok_or("Invalid entry.")?;
                if line != entry && !(setting("unignoreActions", true) && line == format!("!{entry}")) {
                    return Err(Error::Stale);
                }
                let target = format!("{root}/.gitignore");
                if !preferences::can_edit(&target) {return Err(Error::Stale);}
                let before = project.read_file(&target)?;
                if fingerprint(&before) != p[5].as_u64().ok_or("Invalid snapshot.")? || before.lines().any(|old| old == line) {
                    return Err(Error::Stale);
                }
                let edit = append(&before, &format!("{line}\n"), false);
                (target, before, edit)
            }
            "template" => {
                if !preferences::can_edit(&doc.path) || !dialect(&doc.path).is_some_and(|d| d.syntax == check::Syntax::Glob)
                    || fingerprint(&doc.text) != p[5].as_u64().ok_or("Invalid snapshot.")?
                {
                    return Err(Error::Stale);
                }
                let mut items: Vec<_> = TEMPLATES.iter().map(|(name, _)| PickItem::new(&format!("t:{name}"), name)).collect();
                items.extend(user_templates().iter().map(|n| PickItem::new(&format!("u:{n}"), n)));
                let keys = host::pick_many("Ignore templates to append", &items)?.ok_or(Error::Cancelled)?;
                if keys.is_empty() {
                    return Err(Error::Cancelled);
                }
                if keys.len() > 16
                    || keys.iter().collect::<HashSet<_>>().len() != keys.len()
                    || keys.iter().any(|k| !items.iter().any(|i| i.key == *k))
                {
                    return Err("Choose up to 16 distinct current templates.".into());
                }
                let mut block = String::new();
                for key in keys {
                    let (label, content) = template_content(&key)?;
                    let header = format!("### {label} template");
                    if doc.text.lines().any(|l| l == header) {
                        return Err("A selected template header is already present; review the file before appending again.".into());
                    }
                    block.push_str(&format!("{header}\n{content}"));
                    if !block.ends_with('\n') {
                        block.push('\n')
                    }
                    if block.len() > MAX {
                        return Err("Selected templates exceed 256 KiB.".into());
                    }
                }
                let edit = append(&doc.text, &block, true);
                (doc.path.clone(), doc.text.clone(), edit)
            }
            _ => return Err("Unknown ignore action.".into()),
        };
        if before.len() > MAX || edit.text.len() > MAX {
            return Err("Ignore append exceeds 256 KiB.".into());
        }
        if !ui_kit::prompts::confirm_exact(
            "Append to ignore file?",
            &format!(
                "{}\nInsertion at {}:{}\n{}",
                serde_json::to_string(&target).unwrap_or_default(),
                edit.range.start.line + 1,
                edit.range.start.character + 1,
                serde_json::to_string(&edit.text).unwrap_or_default()
            ),
        )? {
            return Err(Error::Cancelled);
        }
        let project = source_unchanged(doc, root)?;
        if !preferences::can_edit(&target) {return Err(Error::Stale);}
        if project.read_file(&target)? != before {
            return Err(Error::Stale);
        }
        Ok(FileAction { title: action.title, edits: vec![(target, vec![edit])], ..FileAction::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn setup(path: &str, content: &str) -> (MockHost, IgnorePlugin, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read", "editor.write", "storage"]);
        host.add_file(path, content);
        host.open_editor(path, "ignore", content, &[]);
        (host, IgnorePlugin::default(), Document { path: path.into(), language: "ignore".into(), version: 1, text: content.into() })
    }
    fn action(plugin: &mut IgnorePlugin, doc: &Document, title: &str) -> FileAction {
        plugin.file_actions(doc, Range::default()).into_iter().find(|a| a.title.starts_with(title)).unwrap()
    }
    #[test]
    fn literal_paths_cannot_become_globs_or_extra_lines() {
        assert_eq!(literal_entry("a*b?/x[1] ").unwrap(), "/a\\*b\\?/x\\[1\\]\\ ");
        for path in ["../secret", "a/../b", "a\nb", "a\rb", "", "a//b"] {
            assert!(literal_entry(path).is_err(), "{path:?}");
        }
    }
    #[test]
    fn creates_selected_exact_dotfile_without_overwriting_or_processes() {
        let (host, mut plugin, doc) = setup("/project/src/a.rs", "");
        let a = action(&mut plugin, &doc, "Create exact");
        for answer in [Some(&[".dockerignore"][..]), Some(&[""][..]), Some(&[][..])] {
            host.answer_next_prompt(answer);
        }
        let resolved = plugin.resolve_file_action(&doc, a).unwrap();
        let FileOperation::Create { path, text, overwrite } = &resolved.operations[0] else { panic!() };
        assert_eq!(path, "/project/.dockerignore");
        assert_eq!(text, "# .dockerignore\n");
        assert!(!overwrite);
        assert!(!host.invocation().project().unwrap().exists(path).unwrap());
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn creation_cancellation_collision_and_directory_escape_are_safe() {
        for stage in 0..3 {
            let (host, mut plugin, doc) = setup("/project/a.txt", "");
            let a = action(&mut plugin, &doc, "Create exact");
            if stage > 0 {
                host.answer_next_prompt(Some(&[".gitignore"]));
            }
            if stage > 1 {
                host.answer_next_prompt(Some(&[""]));
            }
            host.answer_next_prompt(None);
            assert!(matches!(plugin.resolve_file_action(&doc, a), Err(Error::Cancelled)));
            assert!(host.process_runs().is_empty());
        }
        let (host, mut plugin, doc) = setup("/project/a.txt", "");
        let project = host.invocation().project().unwrap();
        for bad in ["../outside", "/outside", "a\\b", "a\nb", "C:/x", "missing"] {
            assert!(directory(&project, "/project", bad).is_err());
        }
        host.open_editor("/project/.gitignore", "ignore", "unsaved", &[]);
        assert!(vacant(&project, "/project/.gitignore").is_err());
        let a = action(&mut plugin, &doc, "Create exact");
        host.answer_next_prompt(Some(&[".gitignore"]));
        host.answer_next_prompt(Some(&[""]));
        assert!(plugin.resolve_file_action(&doc, a).is_err());
    }
    #[test]
    fn entries_refuse_stale_target_and_preserve_unsaved_crlf() {
        let (host, mut plugin, doc) = setup("/project/a.txt", "");
        host.add_file(".gitignore", "# old\r\n");
        let a = action(&mut plugin, &doc, "Add /a.txt");
        host.add_file(".gitignore", "# new\r\n");
        assert!(matches!(plugin.resolve_file_action(&doc, a), Err(Error::Stale)));
        host.open_editor("/project/.gitignore", "ignore", "# unsaved\r\n", &[]);
        let a = action(&mut plugin, &doc, "Add /a.txt");
        host.answer_next_prompt(Some(&[]));
        let resolved = plugin.resolve_file_action(&doc, a).unwrap();
        assert_eq!(resolved.edits[0].1[0].text, "/a.txt\r\n");
        assert_eq!(host.editor_text(), "# unsaved\r\n");
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn template_append_preserves_line_endings_unsaved_text_and_version() {
        let (host, mut plugin, mut doc) = setup("/project/.gitignore", "# unsaved\r\n");
        let a = action(&mut plugin, &doc, "Append ignore");
        host.answer_next_prompt(Some(&["t:Rust"]));
        host.answer_next_prompt(Some(&[]));
        let resolved = plugin.resolve_file_action(&doc, a).unwrap();
        let text = &resolved.edits[0].1[0].text;
        assert!(text.contains("### Rust template\r\n"));
        assert!(!text.replace("\r\n", "").contains('\n'));
        assert_eq!(host.editor_text(), doc.text);
        let a = action(&mut plugin, &doc, "Append ignore");
        doc.version += 1;
        assert!(matches!(plugin.resolve_file_action(&doc, a), Err(Error::Stale)));
    }
    #[test]
    fn cancelled_append_and_project_switch_have_no_edits() {
        let (host, mut plugin, doc) = setup("/project/.gitignore", "");
        let a = action(&mut plugin, &doc, "Append ignore");
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, a), Err(Error::Cancelled)));
        let a = action(&mut plugin, &doc, "Append ignore");
        host.answer_next_prompt(Some(&["t:Rust"]));
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, a), Err(Error::Cancelled)));
        let a = action(&mut plugin, &doc, "Create exact");
        host.set_project_root(Some("/different"));
        assert!(matches!(plugin.resolve_file_action(&doc, a), Err(Error::Stale)));
        assert!(host.process_runs().is_empty());
    }
}
