//! Reviewed extraction of plain scalar attributes from a Java field-access entity into a new
//! `@MappedSuperclass` sibling file. The invoking document is edited and one new file is created
//! (`overwrite: false`); no other existing file is touched.
//!
//! Conservative scope: only `private <type> <name>;` scalar attributes whose annotations are
//! `@Column`/`@Basic`/`@Temporal`/`@Lob` written with the entity's own persistence namespace.
//! Identifiers, versions, relationships, embedded and unknown-annotated members are never offered.
use super::*;

pub(super) const TITLE: &str = "Extract mapped superclass with preview…";
const MAX_MOVED: usize = 32;
const MOVABLE_ANNOTATIONS: [&str; 4] = ["Column", "Basic", "Temporal", "Lob"];
const MAPPING_ANNOTATIONS: [&str; 14] = [
    "Id", "EmbeddedId", "Column", "Basic", "Temporal", "Lob", "Enumerated", "ManyToOne", "OneToMany", "OneToOne",
    "ManyToMany", "Embedded", "JoinColumn", "Version",
];

struct Import {
    span: (usize, usize),
    path: String,
    simple: String,
    wildcard: bool,
    is_static: bool,
}

struct Field {
    name: String,
    ty: String,
    span: (usize, usize),
}

struct Source {
    class: Class,
    masked: String,
    name_end: usize,
    body: (usize, usize),
    members: Vec<(usize, usize)>,
    imports: Vec<Import>,
    movable: Vec<Field>,
    skipped: Vec<String>,
    ns: &'static str,
}

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn idents(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
        .filter(|token| !token.is_empty() && !token.starts_with(|c: char| c.is_ascii_digit()))
}

fn imports(masked: &str, before: usize) -> Option<Vec<Import>> {
    let mut found = Vec::new();
    let mut offset = 0;
    for line in masked[..before].split_inclusive('\n') {
        if let Some(rest) = line.trim().strip_prefix("import ") {
            let rest = rest.trim().strip_suffix(';')?.trim();
            if rest.contains(';') {
                return None;
            }
            let is_static = rest.starts_with("static ");
            let path = rest.trim_start_matches("static ").trim().to_owned();
            let simple = path.rsplit('.').next()?.to_owned();
            found.push(Import { span: (offset, offset + line.len()), wildcard: simple == "*", path, simple, is_static });
        }
        offset += line.len();
    }
    Some(found)
}

fn owned(written: &str, name: &str, ns: &str, imports: &[Import]) -> bool {
    let canonical = format!("{ns}.{name}");
    if written.contains('.') {
        return written == canonical;
    }
    let explicit: Vec<_> = imports.iter().filter(|i| !i.is_static && i.simple == name).collect();
    if !explicit.is_empty() {
        return explicit.len() == 1 && explicit[0].path == canonical;
    }
    imports.iter().any(|i| !i.is_static && i.path == format!("{ns}.*"))
        && imports.iter().filter(|i| i.wildcard).all(|i| i.path == format!("{ns}.*") || i.path == "java.util.*")
}

fn classify(src: &Source, member: (usize, usize)) -> Option<Result<Field, String>> {
    let (start, finish) = member;
    let clean = crate::model::without_annotations(&src.masked, start, finish);
    let declaration = clean.trim().strip_suffix(';')?;
    let (head, init) = match declaration.split_once('=') {
        Some((head, init)) => (head, Some(init)),
        None => (declaration, None),
    };
    let parts: Vec<_> = head.split_whitespace().collect();
    let ["private", ty, name] = parts.as_slice() else {
        return None;
    };
    if head.contains(['(', '<', ',']) || !identifier(name) {
        return None;
    }
    let attribute = src.class.attributes.iter().find(|a| a.name == *name)?;
    let reject = |reason: &str| Some(Err(format!("{name}: {reason}")));
    if attribute.relation != Relation::None || attribute.id || attribute.generated.is_some() || attribute.ty != *ty {
        return reject("not a plain scalar attribute");
    }
    if !super::dto_action::supported(&src.class, ty) {
        return reject("type is not a recognised scalar");
    }
    let annotations = annotations(&src.masked[start..finish]);
    let mut seen = HashSet::new();
    for annotation in &annotations {
        let written = &src.masked[start + annotation.start + 1..start + annotation.name_end];
        if !MOVABLE_ANNOTATIONS.contains(&annotation.name)
            || !seen.insert(annotation.name)
            || !owned(written, annotation.name, src.ns, &src.imports)
        {
            return reject(&format!("annotation @{} is unknown, repeated or not from {}", annotation.name, src.ns));
        }
    }
    if init.is_some_and(|init| idents(init).any(|token| !["true", "false", "null"].contains(&token))) {
        return reject("initializer references other members");
    }
    Some(Ok(Field { name: (*name).to_owned(), ty: (*ty).to_owned(), span: member }))
}

fn source(doc: &Document) -> Result<Source, String> {
    let class = current_class(doc).filter(|c| c.kind == Kind::Entity).ok_or("Select a plain entity with no superclass.")?;
    let masked = mask(&doc.text);
    let at = crate::checks::owned_class_start(&doc.text, &masked, &class).ok_or("Only a single source-owned entity class is supported.")?;
    if doc.language == "kotlin" || class.kotlin {
        return Err("Extracting a mapped superclass is supported for Java entities only; Kotlin is refused.".into());
    }
    let name_end = offset(&doc.text, class.pos).ok_or("Stale source.")? + class.name.len();
    let open = name_end + masked[name_end..].find('{').ok_or("Stale source.")?;
    let end = class.body_end.ok_or("Stale source.")?;
    if at > name_end || !masked[name_end..open].trim().is_empty() {
        return Err("The entity must have no generics, extends or implements clause.".into());
    }
    let ns = namespace(&doc.text, &class).ok_or("The entity's persistence namespace is not unambiguous.")?;
    if annotations(&masked).iter().any(|a| a.name == "Access") {
        return Err("@Access is present; only plain field access is supported.".into());
    }
    let imports = imports(&masked, at).ok_or("Unsupported import layout.")?;
    let members = crate::model::members(&masked, open + 1, end, false, false);
    for &(start, finish) in &members {
        if !masked[..finish].ends_with(';')
            && annotations(&masked[start..finish]).iter().any(|a| MAPPING_ANNOTATIONS.contains(&a.name))
        {
            return Err("Mapping annotations on methods suggest property access; only field access is supported.".into());
        }
    }
    let mut src = Source { class, masked, name_end, body: (open + 1, end), members, imports, movable: Vec::new(), skipped: Vec::new(), ns };
    for member in src.members.clone() {
        match classify(&src, member) {
            Some(Ok(field)) => src.movable.push(field),
            Some(Err(reason)) => src.skipped.push(reason),
            None => {}
        }
    }
    Ok(src)
}

pub(super) fn eligible(doc: &Document) -> bool {
    match source(doc) {
        Ok(src) => !src.movable.is_empty(),
        Err(_) => doc.language == "kotlin" && current_class(doc).is_some_and(|class| class.kind == Kind::Entity),
    }
}

fn accessor(member: &str, field: &Field) -> bool {
    let (name, ty, cap) = (&field.name, &field.ty, capitalize(&field.name));
    let mut forms = vec![
        format!("public{ty}get{cap}(){{return{name};}}"),
        format!("public{ty}get{cap}(){{returnthis.{name};}}"),
        format!("publicvoidset{cap}({ty}{name}){{this.{name}={name};}}"),
    ];
    if ty == "boolean" || ty == "Boolean" {
        forms.push(format!("public{ty}is{cap}(){{return{name};}}"));
        forms.push(format!("public{ty}is{cap}(){{returnthis.{name};}}"));
    }
    forms.contains(&member.to_owned())
}

fn blank(text: &str, base: usize, spans: &[(usize, usize)]) -> String {
    let mut bytes = text.as_bytes().to_vec();
    for &(start, end) in spans {
        bytes[start - base..end - base].fill(b' ');
    }
    String::from_utf8(bytes).unwrap_or_default()
}

fn raw(text: &str, span: (usize, usize)) -> String {
    let slice = &text[span.0..span.1];
    let first = slice.find(|c: char| !c.is_whitespace()).unwrap_or(0);
    let line = slice[..first].rfind('\n').map_or(0, |i| i + 1);
    slice[line..].replace("\r\n", "\n")
}

struct Method {
    key: String,
    name: String,
    span: (usize, usize),
    private: bool,
}

fn method_of(src: &Source, span: (usize, usize)) -> Option<Method> {
    let clean = crate::model::without_annotations(&src.masked, span.0, span.1);
    let head = &clean[..clean.find('{')?];
    let paren = head.find('(')?;
    let name = idents(&head[..paren]).last()?.to_owned();
    if name == src.class.name || head.split_whitespace().any(|w| matches!(w, "class" | "interface" | "enum" | "record")) {
        return None;
    }
    let key = head.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(Method { private: key.split_whitespace().any(|w| w == "private"), key, name, span })
}

fn methods_of(src: &Source) -> Vec<Method> {
    src.members.iter().filter(|&&(start, finish)| !src.masked[..finish].ends_with(';') && start < finish).filter_map(|&span| method_of(src, span)).collect()
}

/// Methods that are not a class constructor or a conventional accessor of one of its attributes.
fn candidate_methods(src: &Source) -> Vec<Method> {
    let fields: Vec<Field> = src.class.attributes.iter().map(|a| Field { name: a.name.clone(), ty: a.ty.clone(), span: (0, 0) }).collect();
    methods_of(src)
        .into_iter()
        .filter(|m| {
            let shape = compact(&src.masked[m.span.0..m.span.1]);
            !fields.iter().any(|f| accessor(&shape, f))
        })
        .collect()
}

/// Target path, new file text and edits for the invoking document.
#[cfg(test)]
pub(super) fn render(doc: &Document, chosen: &[String], name: &str) -> Result<(String, String, Vec<TextEdit>), Error> {
    render_with(doc, chosen, &[], name)
}

/// `methods` are keys from `candidate_methods`; attributes the selected methods use are moved with them.
pub(super) fn render_with(doc: &Document, chosen: &[String], methods: &[String], name: &str) -> Result<(String, String, Vec<TextEdit>), Error> {
    let src = source(doc).map_err(Error::from)?;
    if (chosen.is_empty() && methods.is_empty()) || chosen.len() > MAX_MOVED || methods.len() > MAX_MOVED {
        return Err("Choose between one and 32 offered attributes or methods.".into());
    }
    let mut picked = Vec::new();
    let mut seen = HashSet::new();
    for chosen_name in chosen {
        let field = src.movable.iter().find(|f| f.name == *chosen_name).filter(|_| seen.insert(chosen_name.as_str()));
        picked.push(field.ok_or("Choose offered attributes exactly once.")?);
    }
    let offered = candidate_methods(&src);
    let mut moved_methods: Vec<&Method> = Vec::new();
    for key in methods {
        let method = offered.iter().find(|m| m.key == *key).filter(|m| !moved_methods.iter().any(|p| p.span == m.span));
        moved_methods.push(method.ok_or("Choose offered methods exactly once.")?);
    }
    for method in &moved_methods {
        let text = &src.masked[method.span.0..method.span.1];
        if has_word(text, &src.class.name) {
            return Err(format!("Method `{}` refers to the entity class itself and cannot move.", method.name).into());
        }
        for attribute in &src.class.attributes {
            if !has_word(text, &attribute.name) || picked.iter().any(|f| f.name == attribute.name) {
                continue;
            }
            let field = src.movable.iter().find(|f| f.name == attribute.name);
            picked.push(field.ok_or_else(|| format!("Method `{}` uses attribute `{}`, which cannot be moved.", method.name, attribute.name))?);
        }
    }
    if !identifier(name) || name == src.class.name || has_word(&src.masked, name) {
        return Err("Choose a valid superclass name that does not occur in this source file.".into());
    }

    let mut spans: Vec<_> = picked.iter().map(|f| f.span).collect();
    let mut accessors = Vec::new();
    for &(start, finish) in &src.members {
        let text = &src.masked[start..finish];
        if !text.ends_with(';') && annotations(text).is_empty() {
            let shape = compact(text);
            if picked.iter().any(|f| accessor(&shape, f)) {
                accessors.push((start, finish));
            }
        }
    }
    spans.extend(&accessors);
    spans.extend(moved_methods.iter().map(|m| m.span));
    for method in &moved_methods {
        let text = &src.masked[method.span.0..method.span.1];
        if let Some(stays) = methods_of(&src).iter().find(|m| !spans.contains(&m.span) && has_word(text, &m.name)) {
            return Err(format!("Method `{}` calls `{}`, which stays in the entity; move it too or edit manually first.", method.name, stays.name).into());
        }
    }

    let remaining = blank(&src.masked[src.body.0..src.body.1], src.body.0, &spans);
    if let Some(field) = picked.iter().find(|f| has_word(&remaining, &f.name)) {
        return Err(format!(
            "Attribute `{}` is still referenced by another member (not just its conventional accessors); edit it manually first.",
            field.name
        )
        .into());
    }

    for method in moved_methods.iter().filter(|m| m.private) {
        if has_word(&remaining, &method.name) {
            return Err(format!("Private method `{}` is still called by another member; edit it manually first.", method.name).into());
        }
    }

    let moved_masked: String = spans.iter().map(|&(s, e)| &src.masked[s..e]).collect::<Vec<_>>().join(" ");
    let used: HashSet<_> = idents(&moved_masked).collect();
    let mut copied = Vec::new();
    let mut removed_imports = Vec::new();
    let outside_imports: Vec<_> = spans.iter().copied().chain(src.imports.iter().map(|i| i.span)).collect();
    let rest = blank(&doc.text, 0, &outside_imports);
    for import in &src.imports {
        if import.wildcard {
            copied.push(import.path.clone());
        } else if used.contains(import.simple.as_str()) {
            if import.is_static {
                return Err(format!("Static import `{}` is used by moved members; edit it manually first.", import.simple).into());
            }
            copied.push(import.path.clone());
            if !has_word(&rest, &import.simple) {
                removed_imports.push(import.span);
            }
        }
    }
    copied.sort();
    copied.dedup();

    let mut body: Vec<String> = picked.iter().map(|f| raw(&doc.text, f.span)).collect();
    body.extend(accessors.iter().map(|&span| raw(&doc.text, span)));
    body.extend(moved_methods.iter().map(|m| raw(&doc.text, m.span)));
    let imports_block: String = copied.iter().map(|i| format!("import {i};\n")).collect();
    let mut source_text = format!(
        "package {};\n\n{}{}@{}.MappedSuperclass\npublic abstract class {name} {{\n{}\n}}\n",
        package(doc).ok_or(Error::Stale)?,
        imports_block,
        if imports_block.is_empty() { "" } else { "\n" },
        src.ns,
        body.join("\n\n")
    );

    let mut edits: Vec<(usize, usize, String)> = vec![(src.name_end, src.name_end, format!(" extends {name}"))];
    edits.extend(spans.iter().chain(&removed_imports).map(|&(s, e)| (s, e, String::new())));
    if doc.text.contains("\r\n") {
        source_text = source_text.replace('\n', "\r\n");
    }
    let edits: Vec<_> = edits
        .into_iter()
        .map(|(s, e, text)| TextEdit { range: Range::new(position(&doc.text, s), position(&doc.text, e)), text })
        .collect();

    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let target = format!("{directory}/{name}.java");
    let after = checked_after(doc, &edits)?;
    let generated = parse_class(&source_text, &target).ok_or(Error::Stale)?;
    if generated.kind != Kind::MappedSuperclass || source_text.len() > MAX_TEXT || after.len() > MAX_TEXT {
        return Err("Generated source exceeds the supported bound.".into());
    }
    Ok((target, source_text, edits))
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let src = source(doc).map_err(Error::from)?;
    if src.movable.is_empty() {
        return Err(format!("No attribute can be moved. Skipped: {}", src.skipped.join("; ")).into());
    }
    let default = format!("Base{}", src.class.name);
    let name = host::input("Mapped superclass name (same package)", Some(&default))?.ok_or(Error::Cancelled)?;
    let name = name.trim().to_owned();
    let items: Vec<_> = src.movable.iter().map(|f| PickItem::new(&f.name, &f.name).description(&f.ty)).collect();
    let chosen = host::pick_many("Attributes to move (identifiers, versions, relationships and unknown annotations are not offered)", &items)?
        .ok_or(Error::Cancelled)?;
    let candidates = candidate_methods(&src);
    let mut methods = Vec::new();
    if !candidates.is_empty() {
        let items: Vec<_> = candidates.iter().map(|m| PickItem::new(&m.key, &m.key)).collect();
        methods = host::pick_many("Methods to move (attributes a selected method uses are moved with it)", &items)?.ok_or(Error::Cancelled)?;
    }
    let (target, text, edits) = render_with(doc, &chosen, &methods, &name)?;
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
        return Err(Error::Stale);
    }
    no_collision(project, &pending.root, directory, &name, None)?;
    let after = patched(&doc.text, &edits)?;
    if !ui_kit::prompts::confirm_exact(
        "Extract mapped superclass?",
        &format!(
            "Source: {}\nSource version: {}\nNew file: {target}\n\n{text}\nUpdated entity:\n{after}\nCurrent source only: other files that read the moved private fields, JPQL/XML mappings and subclasses are not checked. One undoable host edit; no existing file is overwritten.",
            doc.path, doc.version
        ),
    )? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    no_collision(project, &pending.root, directory, &name, None)?;
    Ok(FileAction {
        title: format!("Extract mapped superclass {name}"),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), edits)],
        operations: vec![FileOperation::Create { path: target, text, overwrite: false }],
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ENTITY: &str = "package shop;\n\nimport jakarta.persistence.Column;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport java.time.Instant;\n\n@Entity\npublic class Invoice {\n    @Id\n    private Long id;\n\n    @Column(name = \"title_text\")\n    private String title;\n\n    private Instant createdAt = null;\n\n    private String note;\n\n    public String getTitle() {\n        return title;\n    }\n\n    public void setTitle(String title) {\n        this.title = title;\n    }\n\n    public Instant getCreatedAt() {\n        return createdAt;\n    }\n}\n";
    const PATH: &str = "/project/src/main/java/shop/Invoice.java";

    fn doc(text: &str) -> Document {
        Document { path: PATH.into(), language: "java".into(), text: text.into(), version: 1 }
    }
    fn setup(text: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(PATH, "java", text, &[]);
        (host, JpaAssist::default(), doc(text))
    }
    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }
    fn refuse(text: &str, chosen: &[&str], name: &str) -> String {
        match render(&doc(text), &names(chosen), name) {
            Ok((_, source, _)) => panic!("expected refusal, got\n{source}"),
            Err(error) => format!("{error:?}"),
        }
    }
    fn applied(text: &str, chosen: &[&str]) -> (String, String) {
        let (_, source, edits) = render(&doc(text), &names(chosen), "BaseInvoice").unwrap();
        (source, patched(text, &edits).unwrap())
    }

    #[test]
    fn moves_attributes_annotations_accessors_and_imports_in_both_namespaces() {
        for ns in ["jakarta.persistence", "javax.persistence"] {
            let text = ENTITY.replace("jakarta.persistence", ns);
            let (superclass, entity) = applied(&text, &["title", "createdAt"]);
            for expected in [
                &format!("@{ns}.MappedSuperclass\npublic abstract class BaseInvoice {{"),
                "import java.time.Instant;",
                &format!("import {ns}.Column;"),
                "@Column(name = \"title_text\")\n    private String title;",
                "private Instant createdAt = null;",
                "public String getTitle() {",
                "public void setTitle(String title) {",
                "public Instant getCreatedAt() {",
            ] {
                assert!(superclass.contains(expected), "{expected}\n{superclass}");
            }
            assert!(entity.contains("public class Invoice extends BaseInvoice {"), "{entity}");
            for gone in ["title", "createdAt", "Instant", &format!("import {ns}.Column;")] {
                assert!(!entity.contains(gone), "{gone}\n{entity}");
            }
            for kept in [&format!("import {ns}.Entity;"), &format!("import {ns}.Id;"), "private Long id;", "private String note;"] {
                assert!(entity.contains(kept), "{kept}\n{entity}");
            }
        }
    }

    #[test]
    fn shared_imports_stay_in_the_entity_and_unmoved_accessors_stay() {
        let text = ENTITY.replace("private String note;", "@Column(name = \"n\")\n    private String note;");
        let (superclass, entity) = applied(&text, &["title"]);
        assert!(entity.contains("import jakarta.persistence.Column;") && entity.contains("import java.time.Instant;"));
        assert!(entity.contains("getCreatedAt") && !entity.contains("getTitle"));
        assert!(superclass.contains("import jakarta.persistence.Column;") && !superclass.contains("Instant"));
    }

    #[test]
    fn crlf_sources_keep_crlf_in_edits_and_new_file() {
        let text = ENTITY.replace('\n', "\r\n");
        let (superclass, entity) = applied(&text, &["title"]);
        assert!(superclass.contains("\r\n") && !superclass.replace("\r\n", "").contains('\n'));
        assert!(entity.contains("extends BaseInvoice {"));
    }

    #[test]
    fn identifiers_relationships_unknown_annotations_and_non_private_members_are_not_offered() {
        let text = ENTITY.replace(
            "    private String note;",
            "    @jakarta.persistence.Version\n    private Long version;\n    @jakarta.validation.constraints.NotNull\n    private String code;\n    @jakarta.persistence.ManyToOne\n    private Customer customer;\n    private final String fixed = \"x\";\n    protected String shown;\n    private String a, b;\n    @Column @Column\n    private String twice;\n    private String note;",
        );
        let src = source(&doc(&text)).unwrap();
        let offered: Vec<_> = src.movable.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(offered, ["title", "createdAt", "note"]);
        assert!(src.skipped.iter().any(|s| s.starts_with("version")));
        assert!(src.skipped.iter().any(|s| s.starts_with("code")));
        assert!(refuse(&text, &["id"], "BaseInvoice").contains("offered"));
        assert!(refuse(&text, &["code"], "BaseInvoice").contains("offered"));
    }

    #[test]
    fn initializers_may_only_be_literals() {
        let text = ENTITY.replace("private String note;", "private String note = DEFAULTS.note;");
        let src = source(&doc(&text)).unwrap();
        assert!(src.skipped.iter().any(|s| s.contains("initializer references other members")));
        assert!(refuse(&text, &["note"], "BaseInvoice").contains("offered"));
        let text = ENTITY.replace("private String note;", "private int count = 0x1F + 2;");
        assert!(render(&doc(&text), &names(&["count"]), "BaseInvoice").is_ok());
    }

    #[test]
    fn attributes_used_by_other_members_or_odd_accessors_are_refused() {
        for extra in [
            "public boolean same(Invoice other) { return title.equals(other.title); }",
            "public String toString() { return \"I\" + this.title; }",
            "@Override\n    public String getTitle() { return title; }",
            "public String getTitleUpper() { return getTitle().toUpperCase(); }",
        ] {
            let base = if extra.contains("@Override") { ENTITY.replace("    public String getTitle() {\n        return title;\n    }\n\n", "") } else { ENTITY.to_owned() };
            let text = base.replacen("    public void setTitle", &format!("    {extra}\n\n    public void setTitle"), 1);
            if extra.contains("getTitleUpper") {
                assert!(render(&doc(&text), &names(&["title"]), "BaseInvoice").is_ok(), "{extra}");
                continue;
            }
            let reason = refuse(&text, &["title"], "BaseInvoice");
            assert!(reason.contains("still referenced"), "{extra}: {reason}");
        }
    }

    #[test]
    fn existing_extends_generics_implements_access_and_property_access_are_refused() {
        for text in [
            ENTITY.replace("class Invoice {", "class Invoice extends Base {"),
            ENTITY.replace("class Invoice {", "class Invoice<T> {"),
            ENTITY.replace("class Invoice {", "class Invoice implements java.io.Serializable {"),
            ENTITY.replace("@Entity\n", "@Entity\n@jakarta.persistence.Access(jakarta.persistence.AccessType.FIELD)\n"),
            ENTITY.replace("    public String getTitle()", "    @Column\n    public String getTitle()"),
        ] {
            assert!(source(&doc(&text)).is_err() && !eligible(&doc(&text)), "{text}");
        }
    }

    #[test]
    fn kotlin_is_refused_with_a_clear_message() {
        let text = "package shop\n\n@jakarta.persistence.Entity\nopen class Invoice {\n    var title: kotlin.String? = null\n}\n";
        let document = Document { path: "/project/src/main/kotlin/shop/Invoice.kt".into(), language: "kotlin".into(), text: text.into(), version: 1 };
        assert!(eligible(&document));
        let Err(message) = source(&document) else { panic!("kotlin must be refused") };
        assert!(message.contains("Java entities only"), "{message}");
    }

    #[test]
    fn invalid_or_colliding_names_and_selections_are_refused() {
        for name in ["", "class", "Invoice", "Instant", "Column", "Entity", "9Base", "A B"] {
            assert!(refuse(ENTITY, &["title"], name).contains("valid superclass name"), "{name}");
        }
        assert!(refuse(ENTITY, &[], "BaseInvoice").contains("between one and 32"));
        assert!(refuse(ENTITY, &["title", "title"], "BaseInvoice").contains("exactly once"));
        assert!(refuse(ENTITY, &["missing"], "BaseInvoice").contains("exactly once"));
    }

    fn with_methods(methods: &str) -> String {
        let at = ENTITY.rfind('}').unwrap();
        format!("{}\n{methods}\n}}\n", &ENTITY[..at])
    }
    fn render_methods(text: &str, attributes: &[&str], methods: &[&str]) -> Result<(String, String), String> {
        let (attributes, methods) = (names(attributes), names(methods));
        render_with(&doc(text), &attributes, &methods, "BaseInvoice")
            .map(|(_, source, edits)| (source, patched(text, &edits).unwrap()))
            .map_err(|e| format!("{e:?}"))
    }

    #[test]
    fn selected_methods_move_and_the_attributes_they_use_move_with_them() {
        let text = with_methods("    public String label() {\n        return title + note;\n    }\n\n    public Invoice() {\n    }");
        let src = source(&doc(&text)).unwrap();
        let offered: Vec<_> = candidate_methods(&src).into_iter().map(|m| m.key).collect();
        assert_eq!(offered, ["public String label()"]);
        let (superclass, entity) = render_methods(&text, &[], &["public String label()"]).unwrap();
        for expected in ["public String label() {", "@Column(name = \"title_text\")\n    private String title;", "private String note;", "public String getTitle() {", "public void setTitle(String title) {"] {
            assert!(superclass.contains(expected), "{expected}\n{superclass}");
        }
        assert!(!superclass.contains("createdAt") && !superclass.contains("Invoice()"));
        for gone in ["label", "title", "note", "setTitle"] {
            assert!(!entity.contains(gone), "{gone}\n{entity}");
        }
        assert!(entity.contains("extends BaseInvoice") && entity.contains("public Invoice() {") && entity.contains("getCreatedAt"));
    }

    #[test]
    fn methods_that_cannot_move_cleanly_are_refused() {
        let cases = [
            ("public String tag() { return \"x\" + id; }", "uses attribute `id`"),
            ("public String both() { return getCreatedAt() + \"\"; }", "calls `getCreatedAt`"),
            ("public Invoice copy() { return new Invoice(); }", "refers to the entity class"),
        ];
        for (method, reason) in cases {
            let message = render_methods(&with_methods(&format!("    {method}")), &[], &[method.split('{').next().unwrap().trim()]).unwrap_err();
            assert!(message.contains(reason), "{method}: {message}");
        }
        let text = with_methods("    private String clean() { return title; }\n\n    public String show() { return clean(); }");
        assert!(render_methods(&text, &[], &["private String clean()"]).unwrap_err().contains("Private method `clean`"));
        assert!(render_methods(&text, &[], &["public String missing()"]).unwrap_err().contains("exactly once"));
        assert!(render_methods(&text, &[], &["public String show()", "public String show()"]).unwrap_err().contains("exactly once"));
        assert!(render_methods(&text, &[], &[]).unwrap_err().contains("between one and 32"));
    }

    #[test]
    fn resolve_asks_for_methods_only_when_some_can_move() {
        let text = with_methods("    public String label() {\n        return title;\n    }");
        let (host, mut plugin, document) = setup(&text);
        let action = pick(&mut plugin, &document);
        host.answer_next_prompt(Some(&["BaseInvoice"]));
        host.answer_next_prompt(Some(&[]));
        host.answer_next_prompt(Some(&["public String label()"]));
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&document, action).unwrap();
        let [FileOperation::Create { text: created, overwrite: false, .. }] = result.operations.as_slice() else { panic!("one create") };
        assert!(created.contains("public String label()") && created.contains("private String title;"));
    }

    #[test]
    fn oversized_sources_are_not_offered() {
        let text = format!("{ENTITY}// {}\n", "x".repeat(MAX_TEXT));
        let (_host, mut plugin, document) = setup(&text);
        assert!(!plugin.file_actions(&document, Range::default()).iter().any(|a| a.title == TITLE));
    }

    fn pick(plugin: &mut JpaAssist, document: &Document) -> FileAction {
        plugin.file_actions(document, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap()
    }

    #[test]
    fn resolve_returns_one_reviewed_edit_plus_a_no_overwrite_create() {
        let (host, mut plugin, document) = setup(ENTITY);
        let action = pick(&mut plugin, &document);
        host.answer_next_prompt(Some(&["BaseInvoice"]));
        host.answer_next_prompt(Some(&["title", "createdAt"]));
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&document, action).unwrap();
        assert_eq!(result.edits.len(), 1);
        assert_eq!(result.edits[0].0, PATH);
        let [FileOperation::Create { path, text, overwrite }] = result.operations.as_slice() else { panic!("one create") };
        assert_eq!(path, "/project/src/main/java/shop/BaseInvoice.java");
        assert!(!overwrite && text.contains("abstract class BaseInvoice"));
        assert_eq!(host.editor_text(), ENTITY);
    }

    #[test]
    fn cancellation_collision_and_source_drift_never_apply_changes() {
        let (host, mut plugin, document) = setup(ENTITY);
        let action = pick(&mut plugin, &document);
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&document, action), Err(Error::Cancelled)));

        let action = pick(&mut plugin, &document);
        host.answer_next_prompt(Some(&["BaseInvoice"]));
        host.answer_next_prompt(Some(&["title"]));
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&document, action), Err(Error::Cancelled)));

        host.add_file("src/main/java/shop/BaseInvoice.kt", "open class BaseInvoice");
        let action = pick(&mut plugin, &document);
        host.answer_next_prompt(Some(&["BaseInvoice"]));
        host.answer_next_prompt(Some(&["title"]));
        assert!(plugin.resolve_file_action(&document, action).is_err());

        let action = pick(&mut plugin, &document);
        host.type_in_editor("// changed");
        assert!(plugin.resolve_file_action(&document, action).is_err());
    }

    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_API / JPA_JAVAX_API jar paths"]
    fn generated_extracted_superclass_compiles_against_both_persistence_apis() {
        for (ns, variable) in [("jakarta.persistence", "JPA_JAKARTA_API"), ("javax.persistence", "JPA_JAVAX_API")] {
            let jar = std::env::var(variable).expect("explicit API jar path");
            let text = ENTITY.replace("jakarta.persistence", ns);
            let (superclass, entity) = applied(&text, &["title", "createdAt"]);
            let dir = std::env::temp_dir().join(format!("jpa-extract-superclass-{}-{ns}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let files = [("BaseInvoice.java", superclass), ("Invoice.java", entity)];
            let paths: Vec<_> = files
                .iter()
                .map(|(name, body)| {
                    let path = dir.join(name);
                    std::fs::write(&path, body).unwrap();
                    path
                })
                .collect();
            let result = std::process::Command::new("javac")
                .args(["--release", "17", "-proc:none", "-classpath", &jar, "-d"])
                .arg(&dir)
                .args(&paths)
                .output()
                .expect("javac installed");
            assert!(result.status.success(), "{ns}: {}\n{files:?}", String::from_utf8_lossy(&result.stderr));
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_API / JPA_JAVAX_API jar paths"]
    fn generated_superclass_with_moved_methods_compiles_against_both_persistence_apis() {
        for (ns, variable) in [("jakarta.persistence", "JPA_JAKARTA_API"), ("javax.persistence", "JPA_JAVAX_API")] {
            let jar = std::env::var(variable).expect("explicit API jar path");
            let text = with_methods("    public String label() {\n        return title + note;\n    }").replace("jakarta.persistence", ns);
            let (superclass, entity) = render_methods(&text, &[], &["public String label()"]).unwrap();
            let dir = std::env::temp_dir().join(format!("jpa-extract-methods-{}-{ns}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let paths: Vec<_> = [("BaseInvoice.java", superclass), ("Invoice.java", entity)]
                .iter()
                .map(|(name, body)| {
                    let path = dir.join(name);
                    std::fs::write(&path, body).unwrap();
                    path
                })
                .collect();
            let result = std::process::Command::new("javac")
                .args(["--release", "17", "-proc:none", "-classpath", &jar, "-d"])
                .arg(&dir)
                .args(&paths)
                .output()
                .expect("javac installed");
            assert!(result.status.success(), "{ns}: {}", String::from_utf8_lossy(&result.stderr));
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
