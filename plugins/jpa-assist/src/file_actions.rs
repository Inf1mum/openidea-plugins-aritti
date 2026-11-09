//! Previewed source-local generation. Cross-file inverse facts are read-only;
//! inverse edits apply only to the invoking target document, whose version the host guards.
use super::*;
use openidea_plugin::file_actions::{FileAction, FileActions, FileOperation};

const MAX_TEXT: usize = 128 << 10;
#[path = "dto_action.rs"]
mod dto_action;
#[path = "ddl_action.rs"]
mod ddl_action;
#[path = "attribute_refactor.rs"]
mod attribute_refactor;
// Reuse the unchanged plain manual scalar source gate for current-request templates.
pub(super) fn scalar_source(doc: &Document) -> Option<Class> { ddl_action::source(doc) }

pub(super) fn find_attribute_usages(ctx: &Invocation) -> Result<(),Error> { attribute_refactor::find_usages(ctx) }

#[derive(Clone, Copy)]
enum Operation {
    Entity,
    Attribute,
    Inverse,
    Dto,
    Ddl,
    Rename,
}
impl Operation {
    fn key(self) -> &'static str {
        match self {
            Self::Entity => "entity",
            Self::Attribute => "attribute",
            Self::Inverse => "inverse",
            Self::Dto => "dto",
            Self::Ddl => "ddl",
            Self::Rename => "rename-local",
        }
    }
}
#[derive(Clone)]
struct Pending {
    root: String,
    doc: Document,
    operation: Operation,
    selection: Range,
}
#[derive(Default)]
pub(crate) struct Generation {
    next: u64,
    pending: HashMap<String, Pending>,
}

pub(crate) fn identifier(value: &str) -> bool {
    value.len() <= 128 && value.is_ascii() && is_identifier(value)
        && !"_ abstract assert boolean break byte case catch char class const continue default do double else enum extends false final finally float for fun if implements import in instanceof int interface is long native new null object package private protected public record return sealed short static strictfp super switch synchronized this throw throws transient true try typealias val var void volatile when while yield".split_ascii_whitespace().any(|word| word == value)
}
fn path(value: &str) -> Option<String> {
    let value = value.replace('\\', "/");
    let absolute = value.starts_with('/')
        || (value.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
            && value.as_bytes().get(1) == Some(&b':')
            && value.as_bytes().get(2) == Some(&b'/'));
    (absolute
        && value.len() <= 4096
        && !value.chars().any(char::is_control)
        && !value.split('/').any(|part| matches!(part, "." | ".."))
        && !value.contains(['*', '?', '[', ']', '{', '}']))
    .then_some(value)
}
fn inside(root: &str, value: &str) -> bool {
    value.starts_with(&format!("{}/", root.trim_end_matches('/')))
}
fn package(doc: &Document) -> Option<String> {
    let masked = mask(&doc.text);
    let mut declarations = words(&masked, "package");
    let declared = if let Some(at) = declarations.next() {
        if declarations.next().is_some() {
            return None;
        }
        let rest = masked[at + 7..].trim_start();
        let end = rest.find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')).unwrap_or(rest.len());
        Some(rest[..end].to_owned())
    } else {
        None
    };
    let package = declared.or_else(|| package_of(&doc.path))?;
    (package.len() <= 512
        && package.split('.').all(identifier)
        && doc.path.rsplit_once('/')?.0.ends_with(&format!("/{}", package.replace('.', "/"))))
    .then_some(package)
}
fn annotation_is(text: &str, class: &Class, name: &str, namespace: &str) -> bool {
    let masked = mask(text);
    let annotations: Vec<_> = annotations(&masked).into_iter().filter(|a| a.name == name).collect();
    !annotations.is_empty()
        && annotations.iter().all(|a| {
            let written = &masked[a.start + 1..a.name_end];
            if written.contains('.') {
                return written == format!("{namespace}.{name}");
            }
            if written.contains(':') && !written.starts_with("field:") && !written.starts_with("get:") {
                return false;
            }
            let explicit: Vec<_> = class.imports.iter().filter(|import| import.rsplit('.').next() == Some(name)).collect();
            if !explicit.is_empty() {
                return explicit.len() == 1 && *explicit[0] == format!("{namespace}.{name}");
            }
            class.imports.iter().any(|import| import == &format!("{namespace}.*"))
                && class
                    .imports
                    .iter()
                    .filter(|import| import.ends_with(".*"))
                    .all(|import| import == &format!("{namespace}.*") || import == "java.util.*")
        })
}
fn namespace(text: &str, class: &Class) -> Option<&'static str> {
    let candidates: Vec<_> = ["jakarta.persistence", "javax.persistence"]
        .into_iter()
        .filter(|namespace| annotation_is(text, class, "Entity", namespace))
        .collect();
    let [namespace] = candidates.as_slice() else {
        return None;
    };
    let other = if *namespace == "jakarta.persistence" { "javax.persistence" } else { "jakarta.persistence" };
    (!class.imports.iter().any(|import| import.starts_with(other))).then_some(*namespace)
}
fn current_class(doc: &Document) -> Option<Class> {
    if has_word(&mask(&doc.text), "typealias") {
        return None;
    }
    if mask(&doc.text).lines().any(|line| line.trim_start().starts_with("import ") && line.contains(" as ")) {
        return None;
    }
    let mut class = parse_class(&doc.text, &doc.path).filter(|class| class.kind == Kind::Entity && class.superclass.is_none())?;
    let masked = mask(&doc.text);
    if !identifier(&class.name)
        || words(&masked, "class").filter(|at| *at == 0 || masked.as_bytes()[at - 1] != b'.').count() != 1
        || class.body_end? >= doc.text.len()
        || package(doc).as_ref() != class.package.as_ref()
    {
        return None;
    }
    let ns = namespace(&doc.text, &class)?;
    if !class.imports.iter().any(|import| import.starts_with(ns)) {
        class.imports.push(format!("{ns}.Entity"));
    }
    Some(class)
}
fn checked_after(doc: &Document, edits: &[TextEdit]) -> Result<String, Error> {
    let after = patched(&doc.text, edits)?;
    let class = parse_class(&after, &doc.path).ok_or(Error::Stale)?;
    if doc.language == "kotlin"
        && after.contains("MutableList<")
        && class.imports.iter().any(|import| import.rsplit('.').next() == Some("MutableList") && import != "kotlin.collections.MutableList")
    {
        return Err("A generated Kotlin collection type conflicts with an imported type.".into());
    }
    let mut names = HashMap::new();
    for import in &class.imports {
        let name = import.rsplit('.').next().ok_or(Error::Stale)?;
        if name == "*" {
            continue;
        }
        if names.insert(name, import).is_some_and(|previous| previous != import) {
            return Err("Generated imports would collide with an existing type name.".into());
        }
    }
    if after.len() > MAX_TEXT {
        return Err("Generated source exceeds the supported source bound.".into());
    }
    Ok(after)
}

fn current(pending: &Pending, doc: &Document) -> Result<Project, Error> {
    if doc.path != pending.doc.path
        || doc.language != pending.doc.language
        || doc.version != pending.doc.version
        || doc.text != pending.doc.text
    {
        return Err(Error::Stale);
    }
    let project = host::project_of(&doc.path).ok_or(Error::Stale)?;
    if path(&project.root()?).as_deref() != Some(pending.root.as_str()) || project.read_file(&doc.path)? != doc.text {
        return Err(Error::Stale);
    }
    Ok(project)
}
fn no_collision(project: &Project, root: &str, directory: &str, name: &str, empty_source: Option<&str>) -> Result<(), Error> {
    for extension in ["java", "kt"] {
        let target = format!("{directory}/{name}.{extension}");
        if empty_source == Some(target.as_str()) {
            continue;
        }
        match project.read_file(&target) {
            Ok(_) => return Err("Target already exists; no file will be overwritten.".into()),
            Err(Error::NotFound(_)) => {}
            Err(error) => return Err(error),
        }
    }
    let relative = directory.strip_prefix(&format!("{}/", root.trim_end_matches('/'))).ok_or(Error::Stale)?;
    let deadline = Instant::now() + Duration::from_secs(1);
    for extension in ["java", "kt"] {
        let files = project.find_files(&format!("{relative}/*.{extension}"), 65)?;
        if files.len() >= 65 {
            return Err("Target folder exceeds bounded type collision discovery.".into());
        }
        for file in files {
            if Instant::now() >= deadline {
                return Err("Type collision discovery timed out.".into());
            }
            let text = project.read_file(&file)?;
            if text.len() > MAX_TEXT {
                return Err("A target-folder source is too large to verify collisions.".into());
            }
            let masked = mask(&text);
            for keyword in ["class", "interface", "record", "enum"] {
                if words(&masked, keyword).any(|at| {
                    masked[at + keyword.len()..]
                        .trim_start()
                        .strip_prefix(name)
                        .is_some_and(|rest| rest.chars().next().is_none_or(|ch| !ch.is_alphanumeric() && ch != '_'))
                }) {
                    return Err("A type with this name already exists in the target folder.".into());
                }
            }
        }
    }
    Ok(())
}
pub(super) fn patched(text: &str, edits: &[TextEdit]) -> Result<String, Error> {
    let mut spans: Vec<_> = edits
        .iter()
        .map(|edit| Some((offset(text, edit.range.start)?, offset(text, edit.range.end)?, edit.text.as_str())))
        .collect::<Option<Vec<_>>>()
        .ok_or(Error::Stale)?;
    spans.sort_by_key(|(start, end, _)| (*start, *end));
    if spans.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(Error::Stale);
    }
    let mut after = text.to_owned();
    for (start, end, replacement) in spans.into_iter().rev() {
        after.replace_range(start..end, replacement);
    }
    Ok(after)
}

impl FileActions for JpaAssist {
    fn file_actions(&mut self, doc: &Document, selection: Range) -> Vec<FileAction> {
        if doc.text.len() > MAX_TEXT
            || language_of(&doc.path).is_none()
            || !matches!(doc.language.as_str(), "java" | "kotlin")
            || (doc.language == "kotlin") != language_of(&doc.path).unwrap()
        {
            return Vec::new();
        }
        let Some(root) = host::project_of(&doc.path).and_then(|project| project.root().ok()).and_then(|root| path(&root)) else {
            return Vec::new();
        };
        if path(&doc.path).as_deref() != Some(doc.path.as_str()) || !inside(&root, &doc.path) || package(doc).is_none() {
            return Vec::new();
        }
        let mut operations = vec![(Operation::Entity, "Generate JPA entity…")];
        if current_class(doc).is_some() {
            operations.extend([
                (Operation::Attribute, "Add JPA attribute with preview…"),
                (Operation::Inverse, "Add inverse attribute to this entity…"),
            ]);
        }
        if dto_action::eligible(doc) {
            operations.push((Operation::Dto, "Create scalar DTO with preview…"));
        }
        if ddl_action::eligible(doc) {
            operations.push((Operation::Ddl, ddl_action::TITLE));
        }
        if attribute_refactor::eligible(doc,selection) {operations.push((Operation::Rename,attribute_refactor::TITLE));}
        if self.generation.pending.len() > 8 {
            self.generation.pending.clear();
        }
        operations
            .into_iter()
            .filter_map(|(operation, title)| {
                self.generation.next = self.generation.next.checked_add(1)?;
                let scope=if matches!(operation,Operation::Rename) {format!(":{:016x}",attribute_refactor::stamp(&root,doc,selection))} else {String::new()};
                let data = format!("jpa:{}:{}:{}{}", operation.key(), self.generation.next, doc.version,scope);
                self.generation.pending.insert(data.clone(), Pending { root: root.clone(), doc: doc.clone(), operation, selection });
                Some(FileAction { title: title.into(), kind: Some("refactor.rewrite".into()), data: Some(data), ..FileAction::default() })
            })
            .collect()
    }
    fn resolve_file_action(&mut self, doc: &Document, action: FileAction) -> Result<FileAction, Error> {
        let pending = self.generation.pending.get(action.data.as_deref().ok_or(Error::Stale)?).cloned().ok_or(Error::Stale)?;
        if matches!(pending.operation,Operation::Rename) {self.generation.pending.remove(action.data.as_deref().ok_or(Error::Stale)?);}
        let project = current(&pending, doc)?;
        let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
        match pending.operation {
            Operation::Entity => {
                let items = [PickItem::new("jakarta", "Jakarta Persistence"), PickItem::new("javax", "Javax Persistence")];
                let Some(namespace) = host::pick("Persistence namespace for the new entity", &items)? else {
                    return Err(Error::Cancelled);
                };
                let Some((name, source)) = prompt_entity(Some(&doc.path), &namespace)? else {
                    return Err(Error::Cancelled);
                };
                let declared = package(doc).ok_or(Error::Stale)?;
                let body = if source.starts_with("package ") { source.split_once('\n').ok_or(Error::Stale)?.1 } else { &source };
                let source =
                    format!("package {declared}{}\n\n{}", if doc.language == "kotlin" { "" } else { ";" }, body.trim_start_matches('\n'));
                let target = format!("{directory}/{name}.{}", if doc.language == "kotlin" { "kt" } else { "java" });
                if path(&target).is_none() || source.len() > MAX_TEXT {
                    return Err("Generated target exceeds source/path bounds.".into());
                }
                let generated = parse_class(&source, &target).ok_or(Error::Stale)?;
                if name == "Long" || generated.imports.iter().any(|import| import.rsplit('.').next() == Some(name.as_str())) {
                    return Err("Entity name conflicts with a generated identifier type or imported annotation/type.".into());
                }
                let same = target == doc.path && doc.text.trim().is_empty();
                no_collision(&project, &pending.root, directory, &name, same.then_some(doc.path.as_str()))?;
                if !ui_kit::prompts::confirm_exact(
                    "Create JPA entity?",
                    &format!(
                        "Namespace: {namespace}.persistence\nTarget: {target}\nSource version: {}\n\n{source}\nOne host-managed undoable edit; no existing file is overwritten.",
                        doc.version
                    ),
                )? {
                    return Err(Error::Cancelled);
                }
                current(&pending, doc)?;
                no_collision(&project, &pending.root, directory, &name, same.then_some(doc.path.as_str()))?;
                Ok(FileAction {
                    title: format!("Create JPA entity {name}"),
                    kind: Some("refactor.rewrite".into()),
                    operations: if same {
                        Vec::new()
                    } else {
                        vec![FileOperation::Create { path: target.clone(), text: source.clone(), overwrite: false }]
                    },
                    edits: if same {
                        vec![(
                            doc.path.clone(),
                            vec![TextEdit { range: Range::new(Position::new(0, 0), position(&doc.text, doc.text.len())), text: source }],
                        )]
                    } else {
                        Vec::new()
                    },
                    open: Some(target),
                    ..FileAction::default()
                })
            }
            Operation::Attribute => {
                let Some(edits) = prompt_attribute(doc)? else {
                    return Err(Error::Cancelled);
                };
                let after = checked_after(doc, &edits)?;
                if !ui_kit::prompts::confirm_exact(
                    "Add JPA attribute?",
                    &format!("Target: {}\nSource version: {}\n\n{after}", doc.path, doc.version),
                )? {
                    return Err(Error::Cancelled);
                }
                current(&pending, doc)?;
                Ok(FileAction {
                    title: action.title,
                    kind: Some("refactor.rewrite".into()),
                    edits: vec![(doc.path.clone(), edits)],
                    ..FileAction::default()
                })
            }
            Operation::Inverse => self.resolve_inverse(&pending, doc, &project),
            Operation::Dto => dto_action::resolve(&pending, doc, &project),
            Operation::Ddl => ddl_action::resolve(&pending, doc, &project),
            Operation::Rename => attribute_refactor::resolve(&pending,doc),
        }
    }
}
impl JpaAssist {
    fn resolve_inverse(&mut self, pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
        let target = current_class(doc).ok_or(Error::Stale)?;
        let ns = namespace(&doc.text, &target).ok_or(Error::Stale)?;
        let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
        let relative = directory.strip_prefix(&format!("{}/", pending.root.trim_end_matches('/'))).ok_or(Error::Stale)?;
        let mut candidates = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(2);
        for extension in ["java", "kt"] {
            let files = project.find_files(&format!("{relative}/*.{extension}"), 65)?;
            if files.len() >= 65 {
                return Err("Inverse owner discovery is incomplete in this folder.".into());
            }
            for file in files.into_iter().filter(|file| file != &doc.path) {
                if Instant::now() >= deadline {
                    return Err("Inverse owner discovery timed out.".into());
                }
                if path(&file).as_deref() != Some(file.as_str()) || !inside(&pending.root, &file) {
                    return Err(Error::Stale);
                }
                let text = project.read_file(&file)?;
                if text.len() > MAX_TEXT {
                    return Err("An inverse owner candidate exceeds the source bound.".into());
                }
                let owner_doc = Document {
                    path: file.clone(),
                    language: if extension == "kt" { "kotlin" } else { "java" }.into(),
                    text: text.clone(),
                    version: 0,
                };
                let Some(owner) = current_class(&owner_doc).filter(|owner| {
                    owner.name == file_stem(&file) && owner.package == target.package && namespace(&text, owner) == Some(ns)
                }) else {
                    continue;
                };
                let target_fq = format!("{}.{}", target.package.as_deref().ok_or(Error::Stale)?, target.name);
                let owner_fq = format!("{}.{}", owner.package.as_deref().ok_or(Error::Stale)?, owner.name);
                if owner.imports.iter().any(|import| {
                    (import.rsplit('.').next() == Some(target.name.as_str()) && import != &target_fq)
                        || (import.ends_with(".*") && import != &format!("{ns}.*") && import != "java.util.*")
                }) || target.imports.iter().any(|import| import.rsplit('.').next() == Some(owner.name.as_str()) && import != &owner_fq)
                {
                    continue;
                }
                for attribute in owner
                    .attributes
                    .iter()
                    .filter(|attribute| attribute.target == target.name && !attribute.ty.contains('.') && !attribute.mapped_by)
                {
                    let relation = format!("{:?}", attribute.relation);
                    if !annotation_is(&text, &owner, &relation, ns) {
                        continue;
                    }
                    if let Some(spec) = inverse_spec(&owner, attribute, &target).filter(|spec| identifier(&spec.name)) {
                        candidates.push((
                            file.clone(),
                            text.clone(),
                            format!("{}.{} → {}.{}", owner_fq, attribute.name, target_fq, spec.name),
                            spec,
                        ));
                    }
                }
            }
        }
        if candidates.is_empty() {
            return Err("No verified same-package, same-folder owning association can be mirrored into this entity.".into());
        }
        let items: Vec<_> =
            candidates.iter().enumerate().map(|(index, (_, _, label, _))| PickItem::new(&index.to_string(), label)).collect();
        let Some(index) = host::pick("Owning association to mirror into this entity", &items)? else {
            return Err(Error::Cancelled);
        };
        let (owner_path, owner_text, label, spec) =
            candidates.get(index.parse::<usize>().map_err(|_| Error::Stale)?).ok_or(Error::Stale)?;
        let edits = attribute_edits(&target, &doc.text, &mask(&doc.text), spec).ok_or(Error::Stale)?;
        let edits: Vec<_> = edits.iter().map(|edit| edit.to_text_edit(&doc.text)).collect();
        let after = checked_after(doc, &edits)?;
        if !ui_kit::prompts::confirm_exact(
            "Add inverse attribute to this entity?",
            &format!("{label}\nRead-only owner: {owner_path}\nEdit only: {} (version {})\n\n{after}", doc.path, doc.version),
        )? {
            return Err(Error::Cancelled);
        }
        current(pending, doc)?;
        if project.read_file(owner_path)? != *owner_text
            || host::project_of(owner_path).and_then(|p| p.root().ok()).as_deref() != Some(pending.root.as_str())
        {
            return Err(Error::Stale);
        }
        Ok(FileAction {
            title: format!("Add inverse attribute {}", spec.name),
            kind: Some("refactor.rewrite".into()),
            edits: vec![(doc.path.clone(), edits)],
            ..FileAction::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const CUSTOMER: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Customer {\n    @Id Long id;\n}\n";
    const ORDER: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Order {\n    @Id Long id;\n    @ManyToOne\n    Customer customer;\n}\n";
    fn setup(path: &str, text: &str, language: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(path, language, text, &[]);
        let doc = Document { path: path.into(), language: language.into(), text: text.into(), version: 1 };
        (host, JpaAssist::default(), doc)
    }
    fn action(plugin: &mut JpaAssist, doc: &Document, title: &str) -> FileAction {
        let action = plugin.file_actions(doc, Range::default()).into_iter().find(|action| action.title.starts_with(title)).unwrap();
        assert!(action.operations.is_empty() && action.edits.is_empty());
        action
    }
    fn answers(host: &MockHost, rows: &[&[&str]]) {
        for row in rows {
            host.answer_next_prompt(Some(row));
        }
    }
    #[test]
    fn empty_java_entity_is_previewed_host_edit_and_command_never_writes() {
        let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Tag.java", "", "java");
        let action = action(&mut plugin, &doc, "Generate JPA");
        answers(&host, &[&["jakarta"], &["Tag"], &["tags"], &["IDENTITY"], &["version"], &[]]);
        let resolved = plugin.resolve_file_action(&doc, action).unwrap();
        assert!(resolved.operations.is_empty());
        assert_eq!(resolved.edits.len(), 1);
        assert_eq!(resolved.edits[0].0, doc.path);
        let text = &resolved.edits[0].1[0].text;
        assert!(text.contains("package shop;") && text.contains("private Long version;"));
        assert_eq!(host.editor_text(), "");
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn sibling_entity_creation_is_non_overwriting_and_preserves_selected_namespace_and_package() {
        let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Customer.java", CUSTOMER, "java");
        let action = action(&mut plugin, &doc, "Generate JPA");
        answers(&host, &[&["javax"], &["Tag"], &["tags"], &["IDENTITY"], &[], &[]]);
        let resolved = plugin.resolve_file_action(&doc, action).unwrap();
        let [FileOperation::Create { path, text, overwrite }] = resolved.operations.as_slice() else { panic!("single create") };
        assert!(!overwrite);
        assert_eq!(path, "/project/src/main/java/shop/Tag.java");
        assert!(text.contains("package shop;") && text.contains("import javax.persistence.Entity;"));
        assert!(!text.contains("jakarta.persistence"));
        assert!(resolved.edits.is_empty());
        assert_eq!(host.editor_text(), CUSTOMER);
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }
    #[test]
    fn kotlin_generation_and_attribute_edit_reuse_wizard_sources_without_applying() {
        let (host, mut plugin, doc) = setup("/project/src/main/kotlin/shop/Tag.kt", "\n", "kotlin");
        let pending = action(&mut plugin, &doc, "Generate JPA");
        answers(&host, &[&["jakarta"], &["tag"], &["tags"], &["UUID"], &[], &[]]);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        assert!(resolved.edits[0].1[0].text.contains("var id: UUID? = null"));
        assert_eq!(host.editor_text(), "\n");
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", CUSTOMER, "java");
        let pending = action(&mut plugin, &doc, "Add JPA attribute");
        answers(&host, &[&["total"], &["none"], &["BigDecimal"], &["notNull"], &[]]);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let text = patched(&doc.text, &resolved.edits[0].1).unwrap();
        assert!(text.contains("import java.math.BigDecimal;") && text.contains("private BigDecimal total;"));
        assert_eq!(resolved.edits[0].0, doc.path);
        assert_eq!(host.editor_text(), CUSTOMER);
    }
    #[test]
    fn inverse_edits_only_invoking_target_and_keeps_owner_and_dirty_target_untouched() {
        let text = format!("{CUSTOMER}//unsaved 🦀\n");
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", &text, "java");
        host.add_file("src/shop/Order.java", ORDER);
        let pending = action(&mut plugin, &doc, "Add inverse");
        answers(&host, &[&["0"], &[]]);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        assert!(resolved.operations.is_empty());
        assert_eq!(resolved.edits.len(), 1);
        assert_eq!(resolved.edits[0].0, doc.path);
        let after = patched(&doc.text, &resolved.edits[0].1).unwrap();
        assert!(after.contains("@OneToMany(mappedBy = \"customer\")") && after.contains("List<Order> orders"));
        assert!(after.ends_with("//unsaved 🦀\n"));
        assert_eq!(host.editor_text(), text);
        assert_eq!(host.invocation().project().unwrap().read_file("src/shop/Order.java").unwrap(), ORDER);
    }
    #[test]
    fn inverse_one_to_one_many_to_many_and_kotlin_targets_share_verified_mapping_facts() {
        for (relation, expected) in
            [("OneToOne", "@OneToOne(mappedBy = \"customer\")"), ("ManyToMany", "@ManyToMany(mappedBy = \"customer\")")]
        {
            let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", CUSTOMER, "java");
            let owner = ORDER
                .replace("ManyToOne", relation)
                .replace("Customer customer", if relation == "ManyToMany" { "List<Customer> customer" } else { "Customer customer" });
            host.add_file("src/shop/Order.java", &owner);
            let pending = action(&mut plugin, &doc, "Add inverse");
            answers(&host, &[&["0"], &[]]);
            let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
            assert!(patched(&doc.text, &resolved.edits[0].1).unwrap().contains(expected));
        }
        let text = "package shop\nimport jakarta.persistence.*\n@Entity\nclass Customer {\n @Id var id: Long? = null\n}\n";
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.kt", text, "kotlin");
        host.add_file("src/shop/Order.java", ORDER);
        let pending = action(&mut plugin, &doc, "Add inverse");
        answers(&host, &[&["0"], &[]]);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        assert!(patched(&doc.text, &resolved.edits[0].1).unwrap().contains("MutableList<Order>"));
    }
    #[test]
    fn inverse_refuses_foreign_namespaces_imported_other_owners_and_existing_properties() {
        for owner in [
            ORDER.replace("jakarta.persistence", "javax.persistence"),
            ORDER.replace("import jakarta.persistence.*;", "import other.Customer;\nimport jakarta.persistence.*;"),
            ORDER.replace("@ManyToOne", "@other.ManyToOne"),
            ORDER.replace("package shop", "package other"),
        ] {
            let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", CUSTOMER, "java");
            host.add_file("src/shop/Order.java", &owner);
            let pending = action(&mut plugin, &doc, "Add inverse");
            assert!(plugin.resolve_file_action(&doc, pending).is_err());
        }
        let text = CUSTOMER.replace("@Id Long id;", "@Id Long id;\n String orders;");
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", &text, "java");
        host.add_file("src/shop/Order.java", ORDER);
        let pending = action(&mut plugin, &doc, "Add inverse");
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
    }
    #[test]
    fn entity_collisions_cancellations_and_invalid_table_names_create_nothing() {
        let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Customer.java", CUSTOMER, "java");
        let pending = action(&mut plugin, &doc, "Generate JPA");
        assert!(matches!(plugin.resolve_file_action(&doc, pending.clone()), Err(Error::Cancelled)));
        host.add_file("src/main/java/shop/Tag.kt", "class Tag");
        answers(&host, &[&["jakarta"], &["Tag"], &["tags"], &["none"], &[]]);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert_eq!(host.editor_text(), CUSTOMER);
        let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Customer.java", CUSTOMER, "java");
        let pending = action(&mut plugin, &doc, "Generate JPA");
        answers(&host, &[&["jakarta"], &["Tag"], &["x\" evil"], &["none"], &[]]);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn action_versions_text_roots_paths_and_expired_instances_are_checked() {
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", CUSTOMER, "java");
        let pending = action(&mut plugin, &doc, "Add inverse");
        for changed in [
            Document { version: 2, ..doc.clone() },
            Document { text: format!("{CUSTOMER}//new"), ..doc.clone() },
            Document { path: "/project-other/Customer.java".into(), ..doc.clone() },
        ] {
            assert!(matches!(plugin.resolve_file_action(&changed, pending.clone()), Err(Error::Stale)));
        }
        assert!(matches!(JpaAssist::default().resolve_file_action(&doc, pending.clone()), Err(Error::Stale)));
        host.type_in_editor("//newer");
        assert!(matches!(plugin.resolve_file_action(&doc, pending.clone()), Err(Error::Stale)));
        host.set_project_root(Some("/other"));
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Stale)));
    }

    #[test]
    fn filling_empty_source_still_checks_other_language_and_generated_type_name_collisions() {
        let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Tag.java", "", "java");
        host.add_file("src/main/java/shop/Tag.kt", "package shop; class Tag");
        let pending = action(&mut plugin, &doc, "Generate JPA");
        answers(&host, &[&["jakarta"], &["Tag"], &["tags"], &["none"], &[]]);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert_eq!(host.editor_text(), "");
        for name in ["Entity", "Long"] {
            let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Tag.java", "", "java");
            let pending = action(&mut plugin, &doc, "Generate JPA");
            answers(&host, &[&["jakarta"], &[name], &["tags"], &["none"], &[]]);
            assert!(plugin.resolve_file_action(&doc, pending).is_err());
        }
    }
    #[test]
    fn inverse_cancellation_and_discovery_limits_leave_both_sides_untouched() {
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", CUSTOMER, "java");
        host.add_file("src/shop/Order.java", ORDER);
        let pending = action(&mut plugin, &doc, "Add inverse");
        assert!(matches!(plugin.resolve_file_action(&doc, pending.clone()), Err(Error::Cancelled)));
        answers(&host, &[&["0"]]);
        assert!(matches!(plugin.resolve_file_action(&doc, pending.clone()), Err(Error::Cancelled)));
        for index in 0..65 {
            host.add_file(&format!("src/shop/C{index}.java"), "class Unrelated {}");
        }
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert_eq!(host.editor_text(), CUSTOMER);
        assert_eq!(host.invocation().project().unwrap().read_file("src/shop/Order.java").unwrap(), ORDER);
    }
    #[test]
    fn ambiguous_annotation_identity_and_import_collision_cannot_generate_mapping_edits() {
        let text = CUSTOMER.replace("import jakarta.persistence.*;", "import other.Entity;\nimport jakarta.persistence.*;");
        let (_host, mut plugin, doc) = setup("/project/src/shop/Customer.java", &text, "java");
        let actions = plugin.file_actions(&doc, Range::default());
        assert!(actions.iter().all(|action| action.title.starts_with("Generate JPA")));
        let text = CUSTOMER.replace("import jakarta.persistence.*;", "import jakarta.persistence.*;\nimport other.Column;");
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", &text, "java");
        let pending = action(&mut plugin, &doc, "Add JPA attribute");
        answers(&host, &[&["total"], &["none"], &["String"], &["notNull"]]);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert_eq!(host.editor_text(), text);
    }
}
