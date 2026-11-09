//! Explicit reviewed source operation. No automatic host string Rename graph.
use super::*;
use openidea_plugin::{TextEdit, file_actions::FileAction, host};
use std::collections::{BTreeMap, BTreeSet};
use std::hash::BuildHasher;
#[derive(Clone, PartialEq, Eq)]
struct CommandName {
    class: String,
    name: String,
    definition: Range,
    uses: Vec<Range>,
    names: BTreeSet<String>,
}
#[derive(Clone)]
struct Pending {
    root: String,
    doc: Document,
    command: CommandName,
}
#[derive(Default)]
pub(super) struct State {
    pending: BTreeMap<String, Pending>,
}
fn name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn strings<'a>(doc: &Document, a: &'a Annotation, field: &str) -> Option<Vec<&'a Str>> {
    let arg = a.arg(field)?;
    let tokens = java::tokens(&arg.text);
    if !hover::literal_values(arg)
        || arg.strings.is_empty()
        || tokens
            .iter()
            .any(|t| t.s.starts_with("\"\"\"") || t.s.starts_with('\''))
    {
        return None;
    }
    arg.strings
        .iter()
        .all(|s| {
            s.at < s.end
                && doc.text.get(s.at..s.end) == Some(s.value.as_str())
                && (name(&s.value) || s.value == "*")
        })
        .then(|| arg.strings.iter().collect())
}
// The shared lexer intentionally tolerates editing fragments. This operation
// requires all comments/quoted units closed before treating the class as complete.
fn closed_java(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at..].starts_with(b"//") {
            at += bytes[at..]
                .iter()
                .position(|b| *b == b'\n')
                .unwrap_or(bytes.len() - at);
            continue;
        }
        if bytes[at..].starts_with(b"/*") {
            let Some(end) = bytes[at + 2..].windows(2).position(|s| s == b"*/") else {
                return false;
            };
            at += end + 4;
            continue;
        }
        if bytes[at..].starts_with(b"\"\"\"") {
            return false;
        }
        if matches!(bytes[at], b'\'' | b'\"') {
            let quote = bytes[at];
            at += 1;
            let mut closed = false;
            while at < bytes.len() {
                if bytes[at] == quote {
                    at += 1;
                    closed = true;
                    break;
                }
                if matches!(bytes[at], b'\n' | b'\r') {
                    return false;
                }
                at += if bytes[at] == b'\\' { 2 } else { 1 };
            }
            if !closed {
                return false;
            }
        } else {
            at += 1;
        }
    }
    true
}
fn model(doc: &Document) -> Vec<CommandName> {
    if doc.language != "java"
        || !doc.path.ends_with(".java")
        || doc.text.len() > 64 << 10
        || !closed_java(&doc.text)
    {
        return vec![];
    }
    let Some(f) = document_model(doc) else {
        return vec![];
    };
    let tokens = java::tokens(&doc.text);
    if tokens.len() > 8192
        || tokens
            .iter()
            .any(|t| t.s == "<" || t.s == "new" || t.s == "\\")
        || doc.text.contains("\\u")
        || f.types.len() != 1
        || f.types.iter().any(|t| t.outer.is_some())
        || f.imports.iter().any(|i| {
            i.path.ends_with(".*") || i.is_static || i.path.rsplit('.').next() == Some("org")
        })
    {
        return vec![];
    }
    let mut out = Vec::new();
    for (ti, t) in f.types.iter().enumerate() {
        if t.kind != "class"
            || t.supers().next().is_some()
            || t.annotations.len() != 1
            || t.annotations[0].name != "ShellComponent"
            || !t.annotations[0].args.is_empty()
            || !rules::supported(&f, &doc.text, &t.annotations[0])
            || f.types.iter().filter(|o| o.name == t.name).count() != 1
        {
            continue;
        }
        let methods: Vec<_> = t.methods().collect();
        if methods.len() > 64
            || methods
                .iter()
                .any(|m| methods.iter().filter(|o| o.name == m.name).count() != 1)
        {
            continue;
        }
        let annotations: BTreeSet<_> = t
            .annotations
            .iter()
            .chain(t.members.iter().flat_map(|m| {
                m.annotations
                    .iter()
                    .chain(m.params.iter().flat_map(|p| &p.annotations))
            }))
            .map(|a| a.at)
            .collect();
        if tokens.iter().any(|tok| {
            t.start <= tok.at && tok.at < t.end && tok.s == "@" && !annotations.contains(&tok.at)
        }) {
            continue;
        }
        let mut defs = BTreeMap::new();
        let mut refs = Vec::new();
        let mut valid = true;
        for m in &methods {
            if m.annotations.iter().any(|a| {
                m.annotations.iter().filter(|b| b.name == a.name).count() != 1
                    || a.args
                        .iter()
                        .any(|v| a.args.iter().filter(|o| o.name == v.name).count() != 1)
            }) {
                valid = false;
                break;
            }
            if m.annotations.iter().any(|a| {
                !matches!(a.name.as_str(), "ShellMethod" | "ShellMethodAvailability")
                    || !rules::supported(&f, &doc.text, a)
            }) {
                valid = false;
                break;
            }
            if let Some(a) = m.annotation("ShellMethod") {
                if m.annotations
                    .iter()
                    .filter(|a| a.name == "ShellMethod")
                    .count()
                    != 1
                {
                    valid = false;
                    break;
                }
                let Some(keys) =
                    strings(doc, a, "key").filter(|v| v.len() == 1 && name(&v[0].value))
                else {
                    valid = false;
                    break;
                };
                let s = keys[0];
                if defs
                    .insert(s.value.clone(), range(&doc.text, s.at, s.end))
                    .is_some()
                {
                    valid = false;
                    break;
                }
                // This annotation role names an indicator method, never the command key.
                if let Some(a) = m.annotation("ShellMethodAvailability") {
                    let Some(values) =
                        strings(doc, a, "value").filter(|v| v.len() == 1 && v[0].value != "*")
                    else {
                        valid = false;
                        break;
                    };
                    if !methods.iter().any(|other| {
                        other.name == values[0].value
                            && other.params.is_empty()
                            && other.annotation("ShellMethod").is_none()
                    }) {
                        valid = false;
                        break;
                    }
                }
            } else if let Some(a) = m.annotation("ShellMethodAvailability") {
                if m.annotations.len() != 1 {
                    valid = false;
                    break;
                }
                let Some(values) = strings(doc, a, "value") else {
                    valid = false;
                    break;
                };
                if values.iter().any(|s| s.value == "*") {
                    if values.len() != 1 {
                        valid = false;
                        break;
                    }
                } else {
                    for s in values {
                        refs.push((s.value.clone(), range(&doc.text, s.at, s.end)));
                    }
                }
            }
        }
        if !valid || defs.is_empty() || refs.iter().any(|(n, _)| !defs.contains_key(n)) {
            continue;
        }
        let names: BTreeSet<String> = defs.keys().cloned().collect();
        for (n, definition) in defs {
            out.push(CommandName {
                class: f.qualified(ti),
                uses: refs
                    .iter()
                    .filter(|(key, _)| *key == n)
                    .map(|(_, r)| *r)
                    .collect(),
                name: n,
                definition,
                names: names.clone(),
            });
            if out.len() == 64 {
                return out;
            }
        }
    }
    out
}
fn selected(span: Range, selection: Range) -> bool {
    if selection.start == selection.end {
        span.start <= selection.start && selection.start < span.end
    } else {
        span.start < selection.end && selection.start < span.end
    }
}
fn rename_preview(
    doc: &Document,
    command: &CommandName,
    new: &str,
    edits: &[TextEdit],
    after: &str,
) -> String {
    let spans = edits
        .iter()
        .map(|e| {
            format!(
                "{}:{}..{}:{}",
                e.range.start.line + 1,
                e.range.start.character + 1,
                e.range.end.line + 1,
                e.range.end.character + 1
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Local physical-source edit only. Runtime/global registry, other files and host indexed usages/Rename are unresolved.\n\nOld literal: \"{}\"\nNew literal: \"{}\"\nExact spans: {} (UTF-16, 1-based, end-exclusive)\n{}\n\nFile: {}\nVersion: {}\nClass: {}\n\nWildcards and command-side indicator method names remain unchanged.\n\nComplete resulting source:\n{}",
        command.name,
        new,
        edits.len(),
        spans,
        doc.path,
        doc.version,
        command.class,
        after
    )
}
impl ShellTools {
    pub(super) fn command_rename_actions(
        &mut self,
        doc: &Document,
        selection: Range,
    ) -> Vec<FileAction> {
        let Some(root) = actions::root(doc) else {
            return vec![];
        };
        let commands = model(doc).into_iter().filter(|c| {
            selected(c.definition, selection) || c.uses.iter().any(|r| selected(*r, selection))
        });
        let mut out = Vec::new();
        for command in commands.take(4) {
            if self.command_renames.pending.len() >= 8 {
                self.command_renames.pending.clear();
            }
            let random = std::collections::hash_map::RandomState::new();
            let token = format!(
                "shell-command:{:016x}{:016x}",
                random.hash_one(0u8),
                random.hash_one(1u8)
            );
            self.command_renames.pending.insert(
                token.clone(),
                Pending {
                    root: root.clone(),
                    doc: doc.clone(),
                    command,
                },
            );
            out.push(FileAction {
                title: "Preview rename local explicit Shell command key…".into(),
                kind: Some("refactor.rename".into()),
                data: Some(token),
                ..Default::default()
            });
        }
        out
    }
    pub(super) fn resolve_command_rename(
        &mut self,
        doc: &Document,
        action: FileAction,
    ) -> Result<FileAction, Error> {
        let token = action.data.as_deref().ok_or(Error::Stale)?;
        let pending = self
            .command_renames
            .pending
            .get(token)
            .cloned()
            .ok_or(Error::Stale)?;
        let fresh = |d: &Document| {
            d.path == pending.doc.path
                && d.language == pending.doc.language
                && d.version == pending.doc.version
                && d.text == pending.doc.text
                && actions::root(d).as_deref() == Some(&pending.root)
                && model(d).contains(&pending.command)
        };
        if !fresh(doc) {
            return Err(Error::Stale);
        }
        let project = host::project_of(&doc.path).ok_or(Error::Stale)?;
        if project.read_file(&doc.path)? != doc.text {
            return Err(Error::Stale);
        }
        let new=host::input("New explicit local Shell command key (lowercase ASCII letters/digits/hyphens, starting with a letter; max64)",Some(&pending.command.name))?.ok_or(Error::Cancelled)?;
        if !name(&new) || new == pending.command.name || pending.command.names.contains(&new) {
            return Err(Error::Invalid("Use a different non-colliding lowercase literal command key, at most64 characters.".into()));
        }
        let mut edits: Vec<_> = std::iter::once(pending.command.definition)
            .chain(pending.command.uses.iter().copied())
            .map(|range| TextEdit {
                range,
                text: new.clone(),
            })
            .collect();
        edits.sort_by_key(|e| (e.range.start.line, e.range.start.character));
        let mut after = doc.text.clone();
        for e in edits.iter().rev() {
            after.replace_range(
                offset(&doc.text, e.range.start).ok_or(Error::Stale)?
                    ..offset(&doc.text, e.range.end).ok_or(Error::Stale)?,
                &e.text,
            );
        }
        let preview = rename_preview(doc, &pending.command, &new, &edits, &after);
        if !ui_kit::prompts::confirm_exact("Rename explicit local Shell command key?", &preview)? {
            return Err(Error::Cancelled);
        }
        if !fresh(doc) || project.read_file(&doc.path)? != doc.text {
            return Err(Error::Stale);
        }
        self.command_renames.pending.remove(token);
        Ok(FileAction {
            title: "Rename explicit local Shell command key".into(),
            kind: Some("refactor.rename".into()),
            edits: vec![(doc.path.clone(), edits)],
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{file_actions::FileActions, mock::MockHost};
    const TEXT: &str = "import org.springframework.shell.standard.ShellMethod;\r\nimport org.springframework.shell.standard.ShellMethodAvailability;\r\n@org.springframework.shell.standard.ShellComponent class C {\r\n /*😀*/ @ShellMethod(key=\"run\",value=\"Run\") @ShellMethodAvailability(\"run\") void execute(){}\r\n Availability run(){return null;}\r\n @ShellMethodAvailability({\"run\"}) Availability gate(){return null;}\r\n @ShellMethodAvailability(\"*\") Availability wildcard(){return null;}\r\n @ShellMethod(key=\"other\",value=\"Other\") void other(){}\r\n}";
    fn doc(text: &str) -> Document {
        Document {
            path: "/project/C.java".into(),
            language: "java".into(),
            version: 7,
            text: text.into(),
        }
    }
    fn selection(d: &Document) -> Range {
        let at = position(&d.text, d.text.find("\"run\"").unwrap() + 1);
        Range::new(at, at)
    }
    fn pending(tools: &mut ShellTools, d: &Document) -> FileAction {
        tools
            .file_actions(d, selection(d))
            .into_iter()
            .find(|a| {
                a.data
                    .as_deref()
                    .is_some_and(|s| s.starts_with("shell-command:"))
            })
            .unwrap()
    }
    #[test]
    fn compact_review_starts_with_scope_and_lists_exact_spans_and_one_complete_result() {
        let _h = MockHost::install();
        let d = doc(TEXT);
        let command = model(&d).into_iter().find(|c| c.name == "run").unwrap();
        let from = TEXT.find("key=\"run\"").unwrap() + 5;
        let reference = TEXT.find("{\"run\"}").unwrap() + 2;
        let edits = vec![
            TextEdit {
                range: range(TEXT, from, from + 3),
                text: "launch".into(),
            },
            TextEdit {
                range: range(TEXT, reference, reference + 3),
                text: "launch".into(),
            },
        ];
        let after = TEXT.replacen("key=\"run\"", "key=\"launch\"", 1).replacen(
            "{\"run\"}",
            "{\"launch\"}",
            1,
        );
        let preview = rename_preview(&d, &command, "launch", &edits, &after);
        assert!(preview.starts_with("Local physical-source edit only. Runtime/global registry, other files and host indexed usages/Rename are unresolved."));
        assert!(preview.contains("Old literal: \"run\"\nNew literal: \"launch\"\nExact spans: 2 (UTF-16, 1-based, end-exclusive)"));
        for e in &edits {
            let exact = format!(
                "{}:{}..{}:{}",
                e.range.start.line + 1,
                e.range.start.character + 1,
                e.range.end.line + 1,
                e.range.end.character + 1
            );
            assert!(preview.contains(&exact));
        }
        assert_eq!(preview.matches("Complete resulting source:").count(), 1);
        assert!(preview.ends_with(&after));
        assert_eq!(preview.matches(&after).count(), 1);
        assert!(!preview.contains("Complete source before:"));
        assert!(!preview.contains(TEXT));
        assert!(
            preview
                .find("Wildcards and command-side indicator")
                .unwrap()
                < preview.find("Complete resulting source:").unwrap()
        );
    }
    #[test]
    fn registered_action_previews_atomic_exact_literals_and_preserves_other_roles_and_wildcard() {
        let h = MockHost::install();
        h.add_file("C.java", TEXT);
        let mut tools = ShellTools::default();
        let d = doc(TEXT);
        let action = pending(&mut tools, &d);
        let replay = action.clone();
        assert!(action.edits.is_empty() && action.operations.is_empty());
        h.answer_next_prompt(Some(&["launch"]));
        h.answer_next_prompt(Some(&["yes"]));
        let resolved = tools.resolve_file_action(&d, action).unwrap();
        assert!(resolved.operations.is_empty());
        assert_eq!(resolved.edits.len(), 1);
        assert_eq!(resolved.edits[0].1.len(), 2);
        let mut after = d.text.clone();
        for e in resolved.edits[0].1.iter().rev() {
            assert_eq!(
                &d.text[offset(&d.text, e.range.start).unwrap()
                    ..offset(&d.text, e.range.end).unwrap()],
                "run"
            );
            after.replace_range(
                offset(&d.text, e.range.start).unwrap()..offset(&d.text, e.range.end).unwrap(),
                &e.text,
            );
        }
        assert!(after.contains("key=\"launch\""));
        assert!(after.contains("Availability({\"launch\"})"));
        assert!(after.contains("@ShellMethodAvailability(\"run\") void execute"));
        assert!(after.contains("Availability run()"));
        assert!(after.contains("Availability(\"*\")"));
        assert!(after.contains("/*😀*/"));
        assert!(after.contains("\r\n"));
        assert_eq!(
            host::project_of(&d.path)
                .unwrap()
                .read_file(&d.path)
                .unwrap(),
            TEXT
        );
        assert!(tools.resolve_file_action(&d, replay).is_err());
        assert!(h.process_runs().is_empty());
        assert_eq!(h.fetches(), 0);
    }
    #[test]
    fn incomplete_alias_duplicate_foreign_inherited_generic_and_selector_owners_refuse() {
        let _h = MockHost::install();
        for text in [
            TEXT.replace("key=\"run\"", "key={\"run\",\"alias\"}"),
            TEXT.replace("key=\"run\"", "key=PREFIX"),
            TEXT.replace("key=\"run\",", ""),
            TEXT.replace("key=\"other\"", "key=\"run\""),
            TEXT.replace("Availability({\"run\"})", "Availability(KEY)"),
            TEXT.replace("Availability({\"run\"})", "Availability({\"run\",\"*\"})"),
            TEXT.replace("Availability({\"run\"})", "Availability(\"unknown\")"),
            TEXT.replace("class C", "class C extends Base"),
            TEXT.replace("class C", "class C<T>"),
            TEXT.replace("class C", "class C {} class C"),
            TEXT.replace(
                "org.springframework.shell.standard.ShellMethod;",
                "foreign.ShellMethod;",
            ),
            TEXT.replace(
                "org.springframework.shell.standard.ShellMethod;",
                "org.springframework.shell.standard.*;",
            ),
            TEXT.replace("key=\"run\"", "key=\"run\\n\""),
            TEXT.replace("Availability run(){return null;}", ""),
            TEXT.replace(
                "@ShellMethodAvailability({\"run\"})",
                "@Foreign @ShellMethodAvailability({\"run\"})",
            ),
            TEXT.replace("void execute", "<T> void execute"),
            format!("{TEXT}\nclass ShellMethod {{}}"),
            TEXT.replace("class C", "class Outer { class C") + "}",
            TEXT.replace("key=\"run\"", "key=\"run\",key=\"hidden\""),
        ] {
            assert!(model(&doc(&text)).is_empty(), "{text}");
        }
        assert!(model(&doc(&format!("import foreign.org;\n{TEXT}"))).is_empty());
        assert!(model(&doc(&TEXT[..TEXT.len() - 1])).is_empty());
        assert!(model(&doc(&format!("{TEXT} /* unfinished"))).is_empty());
    }
    #[test]
    fn validated_new_names_collision_and_cancel_never_return_edits() {
        for input in [
            "",
            "other",
            "run",
            "UPPER",
            "-start",
            "two words",
            "quote\"",
            "back\\slash",
            "new\nline",
            "é",
            "new$key",
        ] {
            let h = MockHost::install();
            h.add_file("C.java", TEXT);
            let mut tools = ShellTools::default();
            let d = doc(TEXT);
            let a = pending(&mut tools, &d);
            h.answer_next_prompt(Some(&[input]));
            h.answer_next_prompt(Some(&["yes"]));
            assert!(tools.resolve_file_action(&d, a).is_err(), "{input:?}");
            assert_eq!(
                host::project_of(&d.path)
                    .unwrap()
                    .read_file(&d.path)
                    .unwrap(),
                TEXT
            );
        }
        for cancel_input in [true, false] {
            let h = MockHost::install();
            h.add_file("C.java", TEXT);
            let mut tools = ShellTools::default();
            let d = doc(TEXT);
            let a = pending(&mut tools, &d);
            if cancel_input {
                h.answer_next_prompt(None);
            } else {
                h.answer_next_prompt(Some(&["launch"]));
                h.answer_next_prompt(None);
            }
            assert!(matches!(
                tools.resolve_file_action(&d, a),
                Err(Error::Cancelled)
            ));
        }
    }
    #[test]
    fn source_project_path_version_class_update_and_delete_are_revalidated() {
        for mode in 0..6 {
            let h = MockHost::install();
            h.add_file("C.java", TEXT);
            let mut tools = ShellTools::default();
            let mut d = doc(TEXT);
            let a = pending(&mut tools, &d);
            match mode {
                0 => d.version += 1,
                1 => d.path = "/project/Renamed.java".into(),
                2 => d.text = d.text.replace("class C", "class Changed"),
                3 => h.set_project_root(Some("/other")),
                4 => h.add_file("C.java", "changed"),
                _ => d.text.clear(),
            }
            h.answer_next_prompt(Some(&["launch"]));
            h.answer_next_prompt(Some(&["yes"]));
            assert!(tools.resolve_file_action(&d, a).is_err());
        }
        let h = MockHost::install();
        let before = doc(TEXT);
        let changed = doc(&TEXT
            .replace("key=\"run\"", "key=\"launch\"")
            .replace("Availability({\"run\"})", "Availability({\"launch\"})"));
        assert!(model(&before).iter().any(|c| c.name == "run"));
        assert!(!model(&changed).iter().any(|c| c.name == "run"));
        assert!(model(&changed).iter().any(|c| c.name == "launch"));
        assert!(model(&doc("")).is_empty());
        h.set_project_root(None);
        assert!(
            ShellTools::default()
                .file_actions(&before, selection(&before))
                .is_empty()
        );
    }
    #[test]
    fn duplicate_class_owners_nested_projects_denied_identity_and_reclaimed_tokens_refuse() {
        let h = MockHost::install();
        h.add_file("C.java", TEXT);
        let mut tools = ShellTools::default();
        let d = doc(TEXT);
        assert!(model(&doc(&format!("{TEXT} class Other {{}}"))).is_empty());
        let old = pending(&mut tools, &d);
        let mut reclaimed = ShellTools::default();
        let fresh = pending(&mut reclaimed, &d);
        assert_ne!(old.data, fresh.data);
        assert!(reclaimed.resolve_file_action(&d, old).is_err());
        h.add_file("nested/C.java", TEXT);
        let mut nested = d.clone();
        nested.path = "/project/nested/C.java".into();
        let old = pending(&mut tools, &nested);
        h.add_project("/project/nested");
        assert!(tools.resolve_file_action(&nested, old).is_err());
        let _denied = MockHost::install().with_capabilities(&[]);
        assert!(
            ShellTools::default()
                .file_actions(&d, selection(&d))
                .is_empty()
        );
    }
    #[test]
    fn unsaved_visible_source_supported_but_oversized_complete_preview_refuses() {
        let h = MockHost::install();
        h.add_file("C.java", "different disk");
        let d = doc(TEXT);
        h.open_editor(&d.path, "java", &d.text, &[]);
        let mut tools = ShellTools::default();
        let a = pending(&mut tools, &d);
        h.answer_next_prompt(Some(&["launch"]));
        h.answer_next_prompt(Some(&["yes"]));
        assert_eq!(
            tools.resolve_file_action(&d, a).unwrap().edits[0].1.len(),
            2
        );
        let text = TEXT.replace("/*😀*/", &format!("/*{}*/", "x".repeat(18000)));
        let d = doc(&text);
        h.open_editor(&d.path, "java", &d.text, &[]);
        let a = pending(&mut tools, &d);
        h.answer_next_prompt(Some(&["launch"]));
        h.answer_next_prompt(Some(&["yes"]));
        assert!(tools.resolve_file_action(&d, a).is_err());
        assert!(!include_str!("../plugin.toml").contains("contributes.documentIndexers"));
        let registration = include_str!("lib.rs")
            .lines()
            .find(|s| s.starts_with("register_plugin!("))
            .unwrap();
        assert!(!registration.contains("indexing"));
    }
}
