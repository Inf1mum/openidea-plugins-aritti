//! Explicit current-document field/graph refactor; no compiler or indexed Rename.
use super::*;
pub(super) const TITLE: &str = "Rename source-local JPA attribute with preview…";
type Span = (usize, usize);
struct Field {
    name: String,
    ty: String,
    declaration: Span,
    column: Option<String>,
    id: bool,
}
struct Inventory {
    class: Class,
    fields: Vec<Field>,
    references: Vec<(String, Span)>,
}
fn trim(text: &str, (start, end): Span) -> Span {
    let value = &text[start..end];
    let first = start + value.len() - value.trim_start().len();
    (first, first + value.trim().len())
}
fn args(
    text: &str,
    masked: &str,
    a: &model::Annotation<'_>,
    default: &str,
) -> Option<HashMap<String, Span>> {
    let mut result = HashMap::new();
    let Some((start, end)) = a.args else {
        return Some(result);
    };
    if text[start..end].trim().is_empty() {
        return Some(result);
    }
    for (offset, part) in model::split_top_level(&masked[start..end]) {
        let at = start + offset;
        let mut depth = 0i32;
        let mut equal = None;
        for (i, byte) in part.bytes().enumerate() {
            match byte {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                b'=' if depth == 0 => {
                    equal = Some(i);
                    break;
                }
                _ => {}
            }
            if depth < 0 {
                return None;
            }
        }
        let (key, from) = if let Some(i) = equal {
            (part[..i].trim(), at + i + 1)
        } else {
            (default, at)
        };
        if key.is_empty()
            || result
                .insert(key.to_owned(), trim(text, (from, at + part.len())))
                .is_some()
        {
            return None;
        }
    }
    Some(result)
}
fn literal(text: &str, span: Span) -> Option<String> {
    let (_, value) = model::plain_literal(text, span, false)?;
    (value.len() <= 128
        && value.is_ascii()
        && !value.chars().any(char::is_control)
        && !value.contains('\\'))
    .then(|| value.to_owned())
}
fn annotation_list<'a>(
    masked: &str,
    span: Span,
    all: &'a [model::Annotation<'a>],
    name: &str,
) -> Option<Vec<&'a model::Annotation<'a>>> {
    let (mut start, mut end) = trim(masked, span);
    if masked.as_bytes().get(start) == Some(&b'{') {
        if model::closing(masked.as_bytes(), start)? + 1 != end {
            return None;
        }
        start += 1;
        end -= 1;
    }
    if masked[start..end].trim().is_empty() {
        return Some(vec![]);
    }
    model::split_top_level(&masked[start..end])
        .into_iter()
        .map(|(offset, part)| {
            let (s, e) = trim(masked, (start + offset, start + offset + part.len()));
            all.iter()
                .find(|a| a.start == s && a.end() == e && a.name == name)
        })
        .collect()
}
fn graph(
    text: &str,
    masked: &str,
    a: &model::Annotation<'_>,
    all: &[model::Annotation<'_>],
    names: &HashSet<String>,
    consumed: &mut HashSet<usize>,
    references: &mut Vec<(String, Span)>,
) -> Option<()> {
    if !consumed.insert(a.start) {
        return None;
    }
    let mut values = args(text, masked, a, "")?;
    if let Some(span) = values.remove("name") {
        literal(text, span)?;
    }
    if let Some(span) = values.remove("attributeNodes") {
        for node in annotation_list(masked, span, all, "NamedAttributeNode")? {
            if !consumed.insert(node.start) {
                return None;
            }
            let mut node_args = args(text, masked, node, "value")?;
            let span = node_args.remove("value")?;
            if !node_args.is_empty() {
                return None;
            }
            let name = literal(text, span)?;
            if !names.contains(&name) {
                return None;
            }
            let (start, value) = model::plain_literal(text, span, false)?;
            references.push((name, (start, start + value.len())));
        }
    }
    values.is_empty().then_some(())
}
fn inventory(doc: &Document) -> Option<Inventory> {
    let class = crate::local_graph::source(doc)?;
    let masked = mask(&doc.text);
    let all = annotations(&masked);
    if all.iter().any(|a| {
        ![
            "Entity",
            "Id",
            "Column",
            "NamedEntityGraph",
            "NamedEntityGraphs",
            "NamedAttributeNode",
        ]
        .contains(&a.name)
    }) {
        return None;
    }
    let (class_at, _) = model::declaration_keyword(&masked, 0, &["class"])?;
    let open = masked[class_at..].find('{')? + class_at;
    let end = class.body_end?;
    if !identifier(&class.name) || !masked[end + 1..].trim().is_empty() {
        return None;
    }
    let mut prelude = masked[..class_at].as_bytes().to_vec();
    for a in all.iter().filter(|a| a.start < class_at) {
        if a.end() > class_at {
            return None;
        }
        prelude[a.start..a.end()].fill(b' ');
    }
    let prelude = String::from_utf8(prelude).ok()?;
    let mut parts: Vec<_> = prelude.split(';').collect();
    let final_part = parts.pop()?.trim();
    if !final_part.is_empty() && final_part != "public" {
        return None;
    }
    let (package, imports) = parts.split_first()?;
    if package.split_whitespace().collect::<Vec<_>>() != ["package", class.package.as_deref()?] {
        return None;
    }
    for part in imports {
        let words: Vec<_> = part.split_whitespace().collect();
        if words.len() != 2 || words[0] != "import" || !words[1].split('.').all(identifier) {
            return None;
        }
    }
    let mut consumed = HashSet::new();
    let mut fields = Vec::new();
    for (start, end) in model::members(&masked, open + 1, end, false, false) {
        let decl = model::without_annotations(&masked, start, end);
        if decl.trim().is_empty() {
            continue;
        }
        let words: Vec<_> = decl.trim().strip_suffix(';')?.split_whitespace().collect();
        let ["private", ty, name] = words.as_slice() else {
            return None;
        };
        if !identifier(name) {
            return None;
        }
        let attribute = class
            .attributes
            .iter()
            .find(|a| a.name == *name && a.ty == *ty)?;
        let at = offset(&doc.text, attribute.pos)?;
        if at < start || at + name.len() > end || doc.text.get(at..at + name.len()) != Some(*name) {
            return None;
        }
        let mut column = None;
        let mut id = false;
        for a in all.iter().filter(|a| a.start >= start && a.start < end) {
            if a.end() > at || !consumed.insert(a.start) {
                return None;
            }
            let mut values = args(&doc.text, &masked, a, "")?;
            match a.name {
                "Id" if !id && values.is_empty() => id = true,
                "Column" if column.is_none() => {
                    let value = literal(&doc.text, values.remove("name")?)?;
                    if value.is_empty() || !values.is_empty() {
                        return None;
                    }
                    column = Some(value);
                }
                _ => return None,
            }
        }
        if id != attribute.id {
            return None;
        }
        fields.push(Field {
            name: (*name).into(),
            ty: (*ty).into(),
            declaration: (at, at + name.len()),
            column,
            id,
        });
    }
    if fields.len() != class.attributes.len() {
        return None;
    }
    let names: HashSet<_> = fields.iter().map(|f| f.name.clone()).collect();
    let mut references = Vec::new();
    for a in all.iter().filter(|a| a.start < class_at) {
        if consumed.contains(&a.start) {
            continue;
        }
        match a.name {
            "Entity" => {
                consumed.insert(a.start);
                let mut values = args(&doc.text, &masked, a, "")?;
                if let Some(span) = values.remove("name") {
                    literal(&doc.text, span)?;
                }
                if !values.is_empty() {
                    return None;
                }
            }
            "NamedEntityGraph" => graph(
                &doc.text,
                &masked,
                a,
                &all,
                &names,
                &mut consumed,
                &mut references,
            )?,
            "NamedEntityGraphs" => {
                consumed.insert(a.start);
                let mut values = args(&doc.text, &masked, a, "value")?;
                let list =
                    annotation_list(&masked, values.remove("value")?, &all, "NamedEntityGraph")?;
                if !values.is_empty() {
                    return None;
                }
                for child in list {
                    graph(
                        &doc.text,
                        &masked,
                        child,
                        &all,
                        &names,
                        &mut consumed,
                        &mut references,
                    )?;
                }
            }
            _ => return None,
        }
    }
    if all.iter().any(|a| !consumed.contains(&a.start)) {
        return None;
    }
    Some(Inventory {
        class,
        fields,
        references,
    })
}
fn selected(inventory: &Inventory, text: &str, selection: Range) -> Option<String> {
    let start = offset(text, selection.start)?;
    let end = offset(text, selection.end)?;
    if start > end {
        return None;
    }
    inventory
        .fields
        .iter()
        .filter(|f| !f.id && f.column.is_some())
        .find(|f| {
            std::iter::once(f.declaration)
                .chain(
                    inventory
                        .references
                        .iter()
                        .filter(|(n, _)| n == &f.name)
                        .map(|(_, s)| *s),
                )
                .any(|(a, b)| start >= a && end <= b && start < b)
        })
        .map(|f| f.name.clone())
}
pub(super) fn stamp(root: &str, doc: &Document, selection: Range) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut state = std::collections::hash_map::DefaultHasher::new();
    (
        root,
        &doc.path,
        &doc.language,
        doc.version,
        &doc.text,
        selection.start.line,
        selection.start.character,
        selection.end.line,
        selection.end.character,
    )
        .hash(&mut state);
    state.finish()
}
pub(super) fn eligible(doc: &Document, selection: Range) -> bool {
    inventory(doc)
        .and_then(|i| selected(&i, &doc.text, selection))
        .is_some()
}
pub(super) fn resolve(pending: &Pending, doc: &Document) -> Result<FileAction, Error> {
    let project = current(pending, doc)?;
    let class = parse_class(&doc.text, &doc.path).ok_or(Error::Stale)?;
    let renamed = std::cell::RefCell::new(None::<(String, String)>);
    let mapper_plan = std::cell::RefCell::new(None::<mapstruct_links::Plan>);
    let mut action = resolve_with(
        pending,
        doc,
        |old| {
            let new = host::input(
                "New source-local attribute name (this file only)",
                Some(old),
            )?;
            if let Some(new) = &new {
                *renamed.borrow_mut() = Some((old.to_owned(), new.clone()));
            }
            Ok(new)
        },
        |preview| {
            let Some((old, new)) = renamed.borrow().clone() else { return Ok(false) };
            let plan = projection_links::rename_plan_for(&project, &pending.root, &class, &[], &old, &new);
            let shown = format!("{preview}{}", plan.preview());
            *mapper_plan.borrow_mut() = Some(plan);
            ui_kit::prompts::confirm_exact("Rename source-local JPA attribute?", &shown)
        },
    )?;
    if let Some(plan) = mapper_plan.into_inner() {
        plan.verify(&project)?;
        plan.extend(&mut action)?;
    }
    Ok(action)
}
pub(super) fn resolve_with(
    pending: &Pending,
    doc: &Document,
    input: impl FnOnce(&str) -> Result<Option<String>, Error>,
    confirm: impl FnOnce(&str) -> Result<bool, Error>,
) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let before = inventory(doc).ok_or(Error::Stale)?;
    let old = selected(&before, &doc.text, pending.selection).ok_or(Error::Stale)?;
    let field = before
        .fields
        .iter()
        .find(|f| f.name == old)
        .ok_or(Error::Stale)?;
    let new = input(&old)?.ok_or(Error::Cancelled)?;
    current(pending, doc)?;
    if !identifier(&new) || new == old || before.fields.iter().any(|f| f.name == new) {
        return Err(
            "Use a new non-keyword ASCII identifier without a declared field-name collision."
                .into(),
        );
    }
    let spans: Vec<_> = std::iter::once(field.declaration)
        .chain(
            before
                .references
                .iter()
                .filter(|(n, _)| n == &old)
                .map(|(_, s)| *s),
        )
        .collect();
    if spans
        .iter()
        .any(|&(a, b)| doc.text.get(a..b) != Some(old.as_str()))
    {
        return Err(Error::Stale);
    }
    let edits: Vec<_> = spans
        .iter()
        .map(|&(start, end)| {
            Edit {
                start,
                end,
                text: new.clone(),
            }
            .to_text_edit(&doc.text)
        })
        .collect();
    let after = patched(&doc.text, &edits)?;
    let after_doc = Document {
        text: after.clone(),
        ..doc.clone()
    };
    let checked = inventory(&after_doc).ok_or(Error::Stale)?;
    if checked.class.name != before.class.name
        || checked.class.package != before.class.package
        || checked.fields.len() != before.fields.len()
        || checked.references.len() != before.references.len()
        || before.fields.iter().any(|f| {
            !checked.fields.iter().any(|next| {
                next.name
                    == if f.name == old {
                        new.as_str()
                    } else {
                        f.name.as_str()
                    }
                    && next.ty == f.ty
                    && next.id == f.id
                    && next.column == f.column
            })
        })
        || before
            .references
            .iter()
            .map(|(n, _)| if n == &old { new.as_str() } else { n.as_str() })
            .collect::<Vec<_>>()
            != checked
                .references
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>()
    {
        return Err(Error::Stale);
    }
    let locations = spans
        .iter()
        .enumerate()
        .map(|(index, &(at, _))| {
            let p = position(&doc.text, at);
            format!(
                "{}: line {}, column {}",
                if index == 0 {
                    "Declaration"
                } else {
                    "Local graph use"
                },
                p.line + 1,
                p.character + 1
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let preview = format!(
        "Source-local attribute: {}.{}.{} -> {new}\nTarget: {}\nSource version: {}\nDeclared physical Column remains: {}\nSupported local references: {}\n{locations}\n\nScope: this document's private field and exact local graph values only. External XML/queries/reflection/compiler bindings are unresolved and are not updated. No getter/setter APIs or schema state are inferred.\n\n{after}",
        before.class.package.as_deref().unwrap_or(""),
        before.class.name,
        old,
        doc.path,
        doc.version,
        field.column.as_deref().unwrap_or(""),
        spans.len() - 1
    );
    if !confirm(&preview)? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    Ok(FileAction {
        title: format!("Rename source-local attribute {old} to {new}"),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), edits)],
        ..FileAction::default()
    })
}

pub(super) fn find_usages(ctx: &Invocation) -> Result<(), Error> {
    let Some(editor) = ctx.editor() else {
        return Ok(());
    };
    let Some(project) = ctx.project() else {
        return Ok(());
    };
    let read = || {
        let value = editor.document()?;
        Ok(Document {
            path: editor.path(),
            language: editor.language(),
            version: value.version,
            text: value.text,
        })
    };
    let doc = read()?;
    let selections = editor.selections()?;
    let [selection] = selections.as_slice() else {
        return Ok(());
    };
    let root = project.root()?;
    usages_with(
        &root,
        &doc,
        *selection,
        read,
        |items| host::pick("JPA: Attribute Usages in This Document", items),
        |path, line, column| host::open_location(path, line, column),
    )
}
fn usages_with(
    root: &str,
    doc: &Document,
    selection: Range,
    fresh: impl Fn() -> Result<Document, Error>,
    pick: impl FnOnce(&[PickItem]) -> Result<Option<String>, Error>,
    navigate: impl FnOnce(&str, u32, u32) -> Result<(), Error>,
) -> Result<(), Error> {
    let Some(inventory) = inventory(doc) else {
        return Ok(());
    };
    let Some(name) = selected(&inventory, &doc.text, selection) else {
        return Ok(());
    };
    let pending = Pending {
        root: root.into(),
        doc: doc.clone(),
        operation: Operation::Rename,
        selection,
    };
    current(&pending, &fresh()?)?;
    let field = inventory
        .fields
        .iter()
        .find(|f| f.name == name)
        .ok_or(Error::Stale)?;
    let spans: Vec<_> = std::iter::once(field.declaration)
        .chain(
            inventory
                .references
                .iter()
                .filter(|(n, _)| n == &name)
                .map(|(_, s)| *s),
        )
        .collect();
    let items: Vec<_> = spans
        .iter()
        .enumerate()
        .map(|(i, &(at, _))| {
            let p = position(&doc.text, at);
            PickItem::new(
                &i.to_string(),
                &format!(
                    "{}: {}",
                    if i == 0 {
                        "Declaration"
                    } else {
                        "Local graph use"
                    },
                    name
                ),
            )
            .description(&format!(
                "{}:{}:{} — this document only; external mappings unresolved",
                doc.path,
                p.line + 1,
                p.character + 1
            ))
        })
        .collect();
    let Some(choice) = pick(&items)? else {
        return Ok(());
    };
    current(&pending, &fresh()?)?;
    let index = choice.parse::<usize>().map_err(|_| Error::Stale)?;
    let &(at, end) = spans.get(index).ok_or(Error::Stale)?;
    if doc.text.get(at..end) != Some(name.as_str()) {
        return Err(Error::Stale);
    }
    let p = position(&doc.text, at);
    navigate(&doc.path, p.line, p.character)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const SOURCE: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.Column;\nimport jakarta.persistence.NamedEntityGraph;\nimport jakarta.persistence.NamedAttributeNode;\nimport java.lang.Long;\nimport java.lang.String;\n@Entity(name=\"name\")\n@NamedEntityGraph(name=\"name\",attributeNodes={@NamedAttributeNode(\"name\"),@NamedAttributeNode(value=\"name\"),@NamedAttributeNode(\"id\")})\npublic class Person {\n@Id private Long id;\n/* 😀 name unchanged */ @Column(name=\"name\") private String name;\n}\n";
    fn setup(text: &str) -> (MockHost, JpaAssist, Document, Range) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let path = "/project/src/main/java/shop/Person.java";
        host.open_editor(path, "java", text, &[]);
        let doc = Document {
            path: path.into(),
            language: "java".into(),
            version: 1,
            text: text.into(),
        };
        let at = text.find("String name;").unwrap() + "String ".len();
        let selection = Range::new(position(text, at), position(text, at + 4));
        (host, JpaAssist::default(), doc, selection)
    }
    fn editor_doc(host: &MockHost) -> Result<Document, Error> {
        let editor = host.invocation().editor().ok_or(Error::Stale)?;
        let value = editor.document()?;
        Ok(Document {
            path: editor.path(),
            language: editor.language(),
            version: value.version,
            text: value.text,
        })
    }
    fn action(plugin: &mut JpaAssist, doc: &Document, selection: Range) -> FileAction {
        let action = plugin
            .file_actions(doc, selection)
            .into_iter()
            .find(|a| a.title == TITLE)
            .expect("eligible local rename");
        assert!(action.edits.is_empty() && action.operations.is_empty());
        action
    }
    #[test]
    fn exported_review_edits_only_declaration_and_local_graph_users() {
        let (host, mut plugin, doc, selection) = setup(SOURCE);
        let pending = action(&mut plugin, &doc, selection);
        host.answer_next_prompt(Some(&["displayName"]));
        host.answer_next_prompt(Some(&["ok"]));
        let result = plugin.resolve_file_action(&doc, pending.clone()).unwrap();
        assert_eq!(result.edits.len(), 1);
        assert_eq!(result.edits[0].0, doc.path);
        assert_eq!(result.edits[0].1.len(), 3);
        assert!(result.operations.is_empty() && result.data.is_none());
        let after = patched(&doc.text, &result.edits[0].1).unwrap();
        assert!(
            after.contains("String displayName;")
                && after.contains("@NamedAttributeNode(\"displayName\")")
                && after.contains("@NamedAttributeNode(value=\"displayName\")")
        );
        assert!(
            after.contains("@Entity(name=\"name\")")
                && after.contains("@NamedEntityGraph(name=\"name\"")
                && after.contains("@Column(name=\"name\")")
                && after.contains("😀 name unchanged")
        );
        assert!(after.contains("@NamedAttributeNode(\"id\")"));
        assert_eq!(host.editor_text(), SOURCE);
        assert!(matches!(
            plugin.resolve_file_action(&doc, pending),
            Err(Error::Stale)
        ));
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }
    #[test]
    fn literal_selection_crlf_and_exact_utf16_declaration_ranges_are_retained() {
        let text = SOURCE.replace('\n', "\r\n");
        let (host, mut plugin, doc, _) = setup(&text);
        let at =
            text.find("@NamedAttributeNode(\"name\")").unwrap() + "@NamedAttributeNode(\"".len();
        let selection = Range::new(position(&text, at), position(&text, at + 4));
        let pending = action(&mut plugin, &doc, selection);
        host.answer_next_prompt(Some(&["fullName"]));
        host.answer_next_prompt(Some(&["ok"]));
        let result = plugin.resolve_file_action(&doc, pending).unwrap();
        for edit in &result.edits[0].1 {
            let a = offset(&text, edit.range.start).unwrap();
            let b = offset(&text, edit.range.end).unwrap();
            assert_eq!(&text[a..b], "name");
        }
        let after = patched(&text, &result.edits[0].1).unwrap();
        assert!(!after.replace("\r\n", "").contains('\n'));
        assert!(after.contains("@Column(name=\"name\") private String fullName;"));
    }
    #[test]
    fn invalid_collision_noop_and_cancellation_consume_rename_token_without_edits() {
        for name in [
            "name".to_owned(),
            "id".into(),
            "class".into(),
            "a.b".into(),
            "naïve".into(),
            "x".repeat(129),
        ] {
            let (host, mut plugin, doc, selection) = setup(SOURCE);
            let pending = action(&mut plugin, &doc, selection);
            host.answer_next_prompt(Some(&[&name]));
            assert!(plugin.resolve_file_action(&doc, pending.clone()).is_err());
            assert!(matches!(
                plugin.resolve_file_action(&doc, pending),
                Err(Error::Stale)
            ));
            assert_eq!(host.editor_text(), SOURCE);
        }
        for confirmation in [false, true] {
            let (host, mut plugin, doc, selection) = setup(SOURCE);
            let pending = action(&mut plugin, &doc, selection);
            if confirmation {
                host.answer_next_prompt(Some(&["fullName"]));
            }
            host.answer_next_prompt(None);
            assert!(matches!(
                plugin.resolve_file_action(&doc, pending.clone()),
                Err(Error::Cancelled)
            ));
            assert!(matches!(
                plugin.resolve_file_action(&doc, pending),
                Err(Error::Stale)
            ));
            assert_eq!(host.editor_text(), SOURCE);
        }
    }
    #[test]
    fn unconsumed_code_metadata_and_nonprivate_or_id_targets_refuse_actions() {
        let (host, mut plugin, doc, selection) = setup(SOURCE);
        for text in [
            SOURCE.replace("private String name;","public String name;"),
            SOURCE.replace("@Column(name=\"name\") private String name;","private String name;"),
            SOURCE.replace("private String name;","private String name = \"x\";"),
            SOURCE.replace("private String name;","private String name; public String getName() {return this.name;}"),
            SOURCE.replace("class Person {","class Person extends Base {"),
            SOURCE.replace("class Person {","class Person<T> {"),
            SOURCE.replace("private String name;","private String name; class Inner {}"),
            SOURCE.replace("@Column(name=\"name\")","@Column(name=\"name\",nullable=false)"),
            SOURCE.replace("@Entity(name=\"name\")","@Entity(name=\"name\") @jakarta.persistence.NamedQuery(name=\"q\",query=\"select p.name from Person p\")"),
            SOURCE.replace("@NamedAttributeNode(\"name\")","@NamedAttributeNode(\"unknown\")"),
            SOURCE.replace("@NamedAttributeNode(\"name\")","@NamedAttributeNode(\"na\" + \"me\")"),
            format!("{SOURCE} name;"),
            SOURCE.replace("package shop;","package shop; int unrelated;"),
            SOURCE.replace("package shop;","package shop; public;;"),
        ] {
            let d=Document{text:text.clone(),..doc.clone()};
            let at=text.find("String name").unwrap()+"String ".len();
            let selected=Range::new(position(&text,at),position(&text,at+4));
            assert!(plugin.file_actions(&d,selected).iter().all(|a|a.title!=TITLE),"{text}");
        }
        let at = SOURCE.find("Long id;").unwrap() + 5;
        let id = Range::new(position(SOURCE, at), position(SOURCE, at + 2));
        assert!(!eligible(&doc, id));
        assert!(eligible(&doc, selection));
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }
    #[test]
    fn version_text_owner_dirty_buffer_and_instance_changes_refuse_before_prompt() {
        for mode in 0..5 {
            let (host, mut plugin, doc, selection) = setup(SOURCE);
            let pending = action(&mut plugin, &doc, selection);
            let mut changed = doc.clone();
            match mode {
                0 => changed.version += 1,
                1 => changed.text.push(' '),
                2 => host.set_project_root(Some("/foreign")),
                3 => host.type_in_editor(" "),
                _ => plugin = JpaAssist::default(),
            }
            assert!(matches!(
                plugin.resolve_file_action(&changed, pending),
                Err(Error::Stale)
            ));
        }
    }
    #[test]
    fn prompt_time_text_and_root_drift_refuse_after_input_and_confirmation() {
        for stage in 0..3 {
            let (host, mut plugin, doc, selection) = setup(SOURCE);
            let action = action(&mut plugin, &doc, selection);
            let pending = plugin
                .generation
                .pending
                .get(action.data.as_deref().unwrap())
                .unwrap()
                .clone();
            let result = resolve_with(
                &pending,
                &doc,
                |_| {
                    if stage == 0 {
                        host.type_in_editor(" ");
                    }
                    Ok(Some("fullName".into()))
                },
                |preview| {
                    assert!(
                        preview.contains("Supported local references: 2")
                            && preview.contains("Declared physical Column remains: name")
                    );
                    if stage == 1 {
                        host.type_in_editor(" ");
                    }
                    if stage == 2 {
                        host.set_project_root(Some("/foreign"));
                    }
                    Ok(true)
                },
            );
            assert!(matches!(result, Err(Error::Stale)));
        }
    }
    #[test]
    fn closed_graph_container_and_unknown_members_have_explicit_role_inventory() {
        let text=SOURCE.replace("import jakarta.persistence.NamedEntityGraph;","import jakarta.persistence.NamedEntityGraph;\nimport jakarta.persistence.NamedEntityGraphs;")
            .replace("@NamedEntityGraph(name=\"name\",attributeNodes={@NamedAttributeNode(\"name\"),@NamedAttributeNode(value=\"name\"),@NamedAttributeNode(\"id\")})",
                "@NamedEntityGraphs({@NamedEntityGraph(name=\"name\",attributeNodes=@NamedAttributeNode(\"name\")),@NamedEntityGraph(name=\"second\",attributeNodes=@NamedAttributeNode(value=\"name\"))})");
        let (_host, _plugin, doc, selection) = setup(&text);
        assert!(eligible(&doc, selection));
        for unknown in [
            text.replace("value=\"name\"", "value=\"name\",subgraph=\"external\""),
            text.replace(
                "name=\"second\"",
                "name=\"second\",includeAllAttributes=true",
            ),
        ] {
            assert!(
                inventory(&Document {
                    text: unknown,
                    ..doc.clone()
                })
                .is_none()
            );
        }
    }
    #[test]
    fn oversized_concrete_preview_refuses_unseen_suffix_and_consumes_token() {
        let text = SOURCE.replace(
            "/* 😀 name unchanged */",
            &format!("/*{}*/", "x".repeat(17 * 1024)),
        );
        let (host, mut plugin, doc, selection) = setup(&text);
        let action = action(&mut plugin, &doc, selection);
        host.answer_next_prompt(Some(&["fullName"]));
        host.answer_next_prompt(Some(&["ok"]));
        assert!(matches!(
            plugin.resolve_file_action(&doc, action.clone()),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            plugin.resolve_file_action(&doc, action),
            Err(Error::Stale)
        ));
        assert_eq!(host.editor_text(), text);
    }
    #[test]
    fn exported_find_usages_navigates_declaration_and_graph_without_writes() {
        let (host, mut plugin, doc, selection) = setup(SOURCE);
        host.open_editor(
            &doc.path,
            "java",
            SOURCE,
            &[(
                selection.start.line,
                selection.start.character,
                selection.end.line,
                selection.end.character,
            )],
        );
        host.answer_next_prompt(Some(&["0"]));
        assert!(
            plugin
                .run_command("find-attribute-usages", &host.invocation(), None)
                .is_ok()
        );
        assert_eq!(
            host.opened_locations(),
            vec![(
                doc.path.clone(),
                selection.start.line,
                selection.start.character
            )]
        );
        host.answer_next_prompt(Some(&["1"]));
        assert!(
            plugin
                .run_command("find-attribute-usages", &host.invocation(), None)
                .is_ok()
        );
        let at =
            SOURCE.find("@NamedAttributeNode(\"name\")").unwrap() + "@NamedAttributeNode(\"".len();
        let p = position(SOURCE, at);
        assert_eq!(
            host.opened_locations().last(),
            Some(&(doc.path.clone(), p.line, p.character))
        );
        assert_eq!(host.editor_text(), SOURCE);
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }
    #[test]
    fn usages_picker_inventory_distinguishes_name_roles_and_literal_selection() {
        let (host, _plugin, doc, _) = setup(SOURCE);
        let at =
            SOURCE.find("@NamedAttributeNode(\"name\")").unwrap() + "@NamedAttributeNode(\"".len();
        let selection = Range::new(position(SOURCE, at), position(SOURCE, at + 4));
        usages_with(
            "/project",
            &doc,
            selection,
            || editor_doc(&host),
            |items| {
                assert_eq!(items.len(), 3);
                assert!(
                    items[0].label.contains("Declaration")
                        && items[1].label.contains("Local graph use")
                );
                assert!(items.iter().all(|i| {
                    i.description
                        .as_deref()
                        .unwrap()
                        .contains("this document only")
                }));
                Ok(Some("2".into()))
            },
            |path, line, column| {
                let at = SOURCE.find("@NamedAttributeNode(value=\"name\")").unwrap()
                    + "@NamedAttributeNode(value=\"".len();
                let p = position(SOURCE, at);
                assert_eq!(
                    (path, line, column),
                    (doc.path.as_str(), p.line, p.character)
                );
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(host.editor_text(), SOURCE);
    }
    #[test]
    fn usages_cancel_and_prompt_time_text_version_owner_or_pick_drift_never_navigate() {
        for mode in 0..5 {
            let (host, _plugin, doc, selection) = setup(SOURCE);
            let result = usages_with(
                "/project",
                &doc,
                selection,
                || editor_doc(&host),
                |_| {
                    match mode {
                        0 => return Ok(None),
                        1 => host.type_in_editor(" "),
                        2 => host.type_in_editor(""),
                        3 => host.set_project_root(Some("/foreign")),
                        _ => return Ok(Some("/outside".into())),
                    }
                    Ok(Some("1".into()))
                },
                |_, _, _| panic!("stale/cancelled inventory must not navigate"),
            );
            if mode == 0 {
                assert!(result.is_ok());
                assert_eq!(host.editor_text(), SOURCE);
            } else {
                assert!(matches!(result, Err(Error::Stale)));
            }
            assert!(host.opened_locations().is_empty());
        }
    }
    #[test]
    fn usages_unknown_or_unselected_source_is_quiet_without_discovery() {
        let text = SOURCE.replace(
            "private String name;",
            "private String name; public String getName(){return this.name;}",
        );
        let (host, mut plugin, doc, selection) = setup(&text);
        host.open_editor(
            &doc.path,
            "java",
            &text,
            &[(
                selection.start.line,
                selection.start.character,
                selection.end.line,
                selection.end.character,
            )],
        );
        assert!(
            plugin
                .run_command("find-attribute-usages", &host.invocation(), None)
                .is_ok()
        );
        assert!(
            host.opened_locations().is_empty()
                && host.requests().is_empty()
                && host.process_runs().is_empty()
        );
        host.open_editor(&doc.path, "java", SOURCE, &[(0, 0, 0, 0)]);
        assert!(
            plugin
                .run_command("find-attribute-usages", &host.invocation(), None)
                .is_ok()
        );
        assert!(host.opened_locations().is_empty());
    }
}
