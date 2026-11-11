//! Reviewed creation of a JPA `AttributeConverter` sibling file for one plain boxed scalar attribute
//! of a Java field-access entity. Verified against the Jakarta Persistence 3.2 Javadoc
//! (AttributeConverter, Converter, Convert): `AttributeConverter<X, Y>` with `Y convertToDatabaseColumn(X)`
//! and `X convertToEntityAttribute(Y)`; `@Converter(autoApply)` defaults to false; `@Convert` must not be
//! used on id, version, relationship, `@Enumerated` or `@Temporal` attributes. The Javadoc does not
//! specify null handling, so the skeleton's null-in/null-out choice is ours and is stated in the preview.
use super::*;

pub(super) const TITLE: &str = "Create attribute converter with preview…";
const TYPES: [&str; 4] = ["String", "Integer", "Long", "Boolean"];
const PLAIN_ANNOTATIONS: [&str; 2] = ["Column", "Basic"];
const MAPPING_ANNOTATIONS: [&str; 14] = [
    "Id", "EmbeddedId", "Column", "Basic", "Temporal", "Lob", "Enumerated", "ManyToOne", "OneToMany", "OneToOne",
    "ManyToMany", "Embedded", "JoinColumn", "Version",
];
const RESERVED_NAMES: [&str; 10] = [
    "String", "Integer", "Long", "Boolean", "Override", "UnsupportedOperationException", "AttributeConverter", "Converter", "Convert", "Object",
];

struct Candidate {
    name: String,
    ty: String,
    span: (usize, usize),
}

struct Source {
    class: Class,
    masked: String,
    ns: &'static str,
    candidates: Vec<Candidate>,
    skipped: Vec<String>,
}

fn classify(doc: &Document, class: &Class, masked: &str, ns: &str, member: (usize, usize)) -> Option<Result<Candidate, String>> {
    let (start, finish) = member;
    let clean = crate::model::without_annotations(masked, start, finish);
    let declaration = clean.trim().strip_suffix(';')?;
    let head = declaration.split_once('=').map_or(declaration, |(head, _)| head);
    let parts: Vec<_> = head.split_whitespace().collect();
    let ["private", ty, name] = parts.as_slice() else {
        return None;
    };
    if head.contains(['(', '<', ',']) || !identifier(name) {
        return None;
    }
    let attribute = class.attributes.iter().find(|a| a.name == *name)?;
    let reject = |reason: &str| Some(Err(format!("{name}: {reason}")));
    if attribute.relation != Relation::None
        || attribute.id
        || attribute.generated.is_some()
        || attribute.lob
        || attribute.enum_string.is_some()
        || attribute.ty != *ty
    {
        return reject("not a plain basic attribute");
    }
    if !TYPES.contains(ty) {
        return reject("type is not one of String, Integer, Long, Boolean");
    }
    for annotation in annotations(&masked[start..finish]) {
        if !PLAIN_ANNOTATIONS.contains(&annotation.name) || !annotation_is(&doc.text, class, annotation.name, ns) {
            return reject(&format!("annotation @{} is not a plain @Column/@Basic from {ns}", annotation.name));
        }
    }
    Some(Ok(Candidate { name: (*name).to_owned(), ty: (*ty).to_owned(), span: member }))
}

fn source(doc: &Document) -> Result<Source, String> {
    let class = current_class(doc).filter(|c| c.kind == Kind::Entity).ok_or("Select a plain entity with no superclass.")?;
    let masked = mask(&doc.text);
    let at = crate::checks::owned_class_start(&doc.text, &masked, &class).ok_or("Only a single source-owned entity class is supported.")?;
    if doc.language == "kotlin" || class.kotlin {
        return Err("Creating a converter is supported for Java entities only; Kotlin is refused.".into());
    }
    let ns = namespace(&doc.text, &class).ok_or("The entity's persistence namespace is not unambiguous.")?;
    if annotations(&masked).iter().any(|a| a.name == "Access") {
        return Err("@Access is present; only plain field access is supported.".into());
    }
    let name_end = offset(&doc.text, class.pos).ok_or("Stale source.")? + class.name.len();
    let open = name_end + masked[name_end..].find('{').ok_or("Stale source.")?;
    let end = class.body_end.ok_or("Stale source.")?;
    if at > name_end {
        return Err("Stale source.".into());
    }
    let members = crate::model::members(&masked, open + 1, end, false, false);
    for &(start, finish) in &members {
        if !masked[..finish].ends_with(';') && annotations(&masked[start..finish]).iter().any(|a| MAPPING_ANNOTATIONS.contains(&a.name)) {
            return Err("Mapping annotations on methods suggest property access; only field access is supported.".into());
        }
    }
    let mut candidates = Vec::new();
    let mut skipped = Vec::new();
    for member in members {
        match classify(doc, &class, &masked, ns, member) {
            Some(Ok(candidate)) => candidates.push(candidate),
            Some(Err(reason)) => skipped.push(reason),
            None => {}
        }
    }
    Ok(Source { class, masked, ns, candidates, skipped })
}

pub(super) fn eligible(doc: &Document) -> bool {
    match source(doc) {
        Ok(src) => !src.candidates.is_empty(),
        Err(_) => doc.language == "kotlin" && current_class(doc).is_some_and(|class| class.kind == Kind::Entity),
    }
}

fn import_edit(doc: &Document, masked: &str, ns: &str, eol: &str) -> Result<Option<(usize, usize, String)>, Error> {
    let stale = || Error::from("Unsupported import layout.");
    let mut last_import = None;
    let mut package_end = None;
    let mut at = 0;
    for line in masked.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") {
            last_import = Some(at + line.len());
        } else if trimmed.starts_with("package ") {
            package_end = Some(at + line.len());
        }
        at += line.len();
    }
    let line = format!("import {ns}.Convert;{eol}");
    if let Some(end) = last_import {
        if !masked[..end].ends_with('\n') {
            return Err(stale());
        }
        return Ok(Some((end, end, line)));
    }
    let end = package_end.ok_or_else(stale)?;
    if !masked[..end].ends_with('\n') || doc.text.len() < end {
        return Err(stale());
    }
    Ok(Some((end, end, format!("{eol}{line}"))))
}

fn converter_text(package: &str, ns: &str, name: &str, attribute: &str, database: &str, auto_apply: bool) -> String {
    let annotation = if auto_apply { "@Converter(autoApply = true)" } else { "@Converter" };
    format!(
        "package {package};\n\nimport {ns}.AttributeConverter;\nimport {ns}.Converter;\n\n{annotation}\npublic class {name} implements AttributeConverter<{attribute}, {database}> {{\n    @Override\n    public {database} convertToDatabaseColumn({attribute} attribute) {{\n        if (attribute == null) {{\n            return null;\n        }}\n\n        throw new UnsupportedOperationException(\"Conversion from {attribute} to {database} is not implemented\");\n    }}\n\n    @Override\n    public {attribute} convertToEntityAttribute({database} dbData) {{\n        if (dbData == null) {{\n            return null;\n        }}\n\n        throw new UnsupportedOperationException(\"Conversion from {database} to {attribute} is not implemented\");\n    }}\n}}\n"
    )
}

pub(super) fn default_name(attribute: &str, database: &str) -> String {
    format!("{attribute}To{database}Converter")
}

/// Target path, new file text and edits for the invoking document (none for autoApply).
pub(super) fn render(doc: &Document, chosen: &str, database: &str, name: &str, auto_apply: bool) -> Result<(String, String, Vec<TextEdit>), Error> {
    let src = source(doc).map_err(Error::from)?;
    let candidate = src.candidates.iter().find(|c| c.name == chosen).ok_or("Choose an offered attribute.")?;
    if !TYPES.contains(&database) || database == candidate.ty {
        return Err("Choose a database column type from String, Integer, Long, Boolean that differs from the attribute type.".into());
    }
    if !identifier(name) || name == src.class.name || RESERVED_NAMES.contains(&name) || (!auto_apply && has_word(&src.masked, name)) {
        return Err("Choose a valid converter class name that does not occur in this source file.".into());
    }
    if has_word(&src.masked, "Convert") && !auto_apply {
        return Err("This source already mentions `Convert`; edit it manually first.".into());
    }
    if !auto_apply
        && src.class.imports.iter().any(|i| i.ends_with(".*") && i != &format!("{}.*", src.ns) && i != "java.util.*" && i != "java.lang.*")
    {
        return Err("Another wildcard import could make `Convert` ambiguous; edit it manually first.".into());
    }
    let eol = if doc.text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut edits = Vec::new();
    if !auto_apply {
        let slice = &doc.text[candidate.span.0..candidate.span.1];
        let first = slice.find(|c: char| !c.is_whitespace()).ok_or(Error::Stale)?;
        let insert_at = candidate.span.0 + first;
        let line_start = doc.text[..insert_at].rfind('\n').map_or(0, |i| i + 1);
        let indent = &doc.text[line_start..insert_at];
        if !indent.chars().all(|c| c == ' ' || c == '\t') {
            return Err("The attribute declaration shares a line with other code; edit it manually first.".into());
        }
        edits.push((insert_at, insert_at, format!("@Convert(converter = {name}.class){eol}{indent}")));
        if !src.class.imports.iter().any(|i| i == &format!("{}.*", src.ns)) {
            edits.extend(import_edit(doc, &src.masked, src.ns, eol)?);
        }
    }
    let edits: Vec<_> = edits
        .into_iter()
        .map(|(s, e, text)| TextEdit { range: Range::new(position(&doc.text, s), position(&doc.text, e)), text })
        .collect();
    let mut text = converter_text(&package(doc).ok_or(Error::Stale)?, src.ns, name, &candidate.ty, database, auto_apply);
    if eol == "\r\n" {
        text = text.replace('\n', "\r\n");
    }
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let target = format!("{directory}/{name}.java");
    let after = checked_after(doc, &edits)?;
    if text.len() > MAX_TEXT || after.len() > MAX_TEXT {
        return Err("Generated source exceeds the supported bound.".into());
    }
    Ok((target, text, edits))
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let src = source(doc).map_err(Error::from)?;
    if src.candidates.is_empty() {
        return Err(format!("No attribute is eligible. Skipped: {}", src.skipped.join("; ")).into());
    }
    let items: Vec<_> = src.candidates.iter().map(|c| PickItem::new(&c.name, &c.name).description(&c.ty)).collect();
    let chosen = host::pick("Attribute to convert (identifiers, versions, enums, temporals, LOBs, relationships and annotated attributes are not offered)", &items)?
        .ok_or(Error::Cancelled)?;
    let candidate = src.candidates.iter().find(|c| c.name == chosen).ok_or("Choose an offered attribute.")?;
    let databases: Vec<_> = TYPES.iter().filter(|t| **t != candidate.ty).map(|t| PickItem::new(t, t)).collect();
    let database = host::pick("Database column type (Y in AttributeConverter<X, Y>)", &databases)?.ok_or(Error::Cancelled)?;
    let default = default_name(&candidate.ty, &database);
    let name = host::input("Converter class name (same package)", Some(&default))?.ok_or(Error::Cancelled)?;
    let name = name.trim().to_owned();
    let modes = [
        PickItem::new("explicit", "Explicit: add @Convert(converter = ...) on this attribute only"),
        PickItem::new("auto", "autoApply: applies to ALL attributes of this type in the persistence unit; no @Convert"),
    ];
    let auto_apply = host::pick("How should the converter be applied?", &modes)?.ok_or(Error::Cancelled)? == "auto";
    let (target, text, edits) = render(doc, &chosen, &database, &name, auto_apply)?;
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
        return Err(Error::Stale);
    }
    no_collision(project, &pending.root, directory, &name, None)?;
    let after = patched(&doc.text, &edits)?;
    let scope = if auto_apply {
        format!(
            "WARNING: @Converter(autoApply = true) applies to EVERY mapped {} attribute of every entity in the persistence unit, except id, version, relationship, @Enumerated and @Temporal attributes and those with another explicit converter or disableConversion = true. Existing columns would be read and written through this converter. The invoking entity is not edited.",
            candidate.ty
        )
    } else {
        format!("Explicit: only `{chosen}` gets @Convert(converter = {name}.class); the converter is not auto-applied to other attributes.")
    };
    if !ui_kit::prompts::confirm_exact(
        "Create attribute converter?",
        &format!(
            "Source: {}\nSource version: {}\nNew file: {target}\n\n{scope}\n\nNew converter:\n{text}\nUpdated entity:\n{after}\nSkeleton: both conversion methods return null for null input (the API Javadoc does not specify null handling) and otherwise throw UnsupportedOperationException, because {} and {database} are not identity-convertible; implement both before use. Current source only: other files, persistence.xml / orm.xml converter registration, existing column data and other converters for this type are not checked. One undoable host edit plus one new file; no existing file is overwritten.",
            doc.path, doc.version, candidate.ty
        ),
    )? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    no_collision(project, &pending.root, directory, &name, None)?;
    Ok(FileAction {
        title: format!("Create attribute converter {name}"),
        kind: Some("refactor.rewrite".into()),
        edits: if edits.is_empty() { Vec::new() } else { vec![(doc.path.clone(), edits)] },
        operations: vec![FileOperation::Create { path: target, text, overwrite: false }],
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ENTITY: &str = "package shop;\n\nimport jakarta.persistence.Column;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n\n@Entity\npublic class Invoice {\n    @Id\n    private Long id;\n\n    @Column(name = \"title_text\")\n    private String title;\n\n    private Boolean paid;\n\n    private int count;\n}\n";
    const PATH: &str = "/project/src/main/java/shop/Invoice.java";

    fn doc(text: &str) -> Document {
        Document { path: PATH.into(), language: "java".into(), text: text.into(), version: 1 }
    }
    fn setup(text: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(PATH, "java", text, &[]);
        (host, JpaAssist::default(), doc(text))
    }
    fn refuse(text: &str, attribute: &str, database: &str, name: &str, auto: bool) -> String {
        match render(&doc(text), attribute, database, name, auto) {
            Ok((_, source, _)) => panic!("expected refusal, got\n{source}"),
            Err(error) => format!("{error:?}"),
        }
    }
    fn applied(text: &str, attribute: &str, database: &str, auto: bool) -> (String, String) {
        let (_, source, edits) = render(&doc(text), attribute, database, "TitleConverter", auto).unwrap();
        (source, patched(text, &edits).unwrap())
    }

    #[test]
    fn explicit_converter_adds_convert_annotation_and_import_in_both_namespaces() {
        for ns in ["jakarta.persistence", "javax.persistence"] {
            let text = ENTITY.replace("jakarta.persistence", ns);
            let (converter, entity) = applied(&text, "title", "Integer", false);
            for expected in [
                "package shop;".to_owned(),
                format!("import {ns}.AttributeConverter;\nimport {ns}.Converter;"),
                "@Converter\npublic class TitleConverter implements AttributeConverter<String, Integer> {".to_owned(),
                "public Integer convertToDatabaseColumn(String attribute) {".to_owned(),
                "public String convertToEntityAttribute(Integer dbData) {".to_owned(),
                "if (attribute == null) {\n            return null;\n        }".to_owned(),
                "throw new UnsupportedOperationException(\"Conversion from String to Integer is not implemented\");".to_owned(),
            ] {
                assert!(converter.contains(&expected), "{expected}\n{converter}");
            }
            assert!(!converter.contains("autoApply") && !converter.contains("TODO"), "{converter}");
            assert!(entity.contains(&format!("import {ns}.Id;\nimport {ns}.Convert;\n")), "{entity}");
            assert!(entity.contains("    @Convert(converter = TitleConverter.class)\n    @Column(name = \"title_text\")\n    private String title;"), "{entity}");
            assert_eq!(entity.matches("@Convert(").count(), 1);
        }
    }

    #[test]
    fn auto_apply_converter_has_class_level_flag_and_leaves_the_entity_untouched() {
        let (converter, entity) = applied(ENTITY, "paid", "Integer", true);
        assert!(converter.contains("@Converter(autoApply = true)\npublic class TitleConverter implements AttributeConverter<Boolean, Integer>"), "{converter}");
        assert_eq!(entity, ENTITY);
    }

    #[test]
    fn wildcard_namespace_import_needs_no_new_import_and_bare_files_get_one_after_package() {
        let wildcard = ENTITY.replace("import jakarta.persistence.Column;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n", "import jakarta.persistence.*;\n");
        let (_, entity) = applied(&wildcard, "title", "Long", false);
        assert_eq!(entity.matches("import ").count(), 1);
        assert!(entity.contains("@Convert(converter = TitleConverter.class)"));
        let bare = "package shop;\n\n@jakarta.persistence.Entity\npublic class Invoice {\n    private String title;\n}\n";
        let (_, entity) = applied(bare, "title", "Long", false);
        assert!(entity.starts_with("package shop;\n\nimport jakarta.persistence.Convert;\n\n@jakarta"), "{entity}");
    }

    #[test]
    fn crlf_sources_keep_crlf_in_edits_and_new_file() {
        let text = ENTITY.replace('\n', "\r\n");
        let (converter, entity) = applied(&text, "title", "Integer", false);
        assert!(!converter.replace("\r\n", "").contains('\n') && !entity.replace("\r\n", "").contains('\n'));
        assert!(entity.contains("@Convert(converter = TitleConverter.class)\r\n    @Column"));
    }

    #[test]
    fn only_plain_boxed_scalars_are_offered() {
        let text = ENTITY.replace(
            "    private int count;",
            "    @jakarta.persistence.Version\n    private Long version;\n    @jakarta.persistence.Enumerated\n    private String kind;\n    @jakarta.persistence.Temporal\n    private String moment;\n    @jakarta.persistence.Lob\n    private String body;\n    @jakarta.persistence.ManyToOne\n    private Customer customer;\n    @jakarta.persistence.Convert(converter = Other.class)\n    private String converted;\n    private java.time.Instant at;\n    private int count;\n    private String a, b;",
        );
        let src = source(&doc(&text)).unwrap();
        let offered: Vec<_> = src.candidates.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(offered, ["title", "paid"]);
        for skipped in ["version", "kind", "moment", "body", "customer", "converted", "count"] {
            assert!(src.skipped.iter().any(|s| s.starts_with(skipped)), "{skipped}: {:?}", src.skipped);
        }
        assert!(refuse(&text, "id", "Integer", "TitleConverter", false).contains("offered"));
        assert!(refuse(&text, "version", "Integer", "TitleConverter", false).contains("offered"));
    }

    #[test]
    fn database_type_must_be_a_different_member_of_the_conservative_set() {
        for database in ["String", "Double", "", "Date"] {
            assert!(refuse(ENTITY, "title", database, "TitleConverter", false).contains("differs from the attribute type"), "{database}");
        }
        assert_eq!(default_name("String", "Integer"), "StringToIntegerConverter");
    }

    #[test]
    fn invalid_or_colliding_converter_names_are_refused() {
        for name in ["", "class", "Invoice", "String", "Converter", "Convert", "9X", "A B", "Column", "Entity"] {
            let reason = refuse(ENTITY, "title", "Integer", name, false);
            assert!(reason.contains("valid converter class name"), "{name}: {reason}");
        }
        assert!(refuse(ENTITY, "title", "Integer", "Invoice", true).contains("valid converter class name"));
    }

    #[test]
    fn existing_convert_mentions_foreign_wildcards_and_shared_lines_are_refused() {
        let mentions = ENTITY.replace("private Boolean paid;", "private Boolean Convert;");
        assert!(refuse(&mentions, "title", "Integer", "TitleConverter", false).contains("Convert"));
        let foreign = ENTITY.replace("import jakarta.persistence.Id;", "import jakarta.persistence.Id;\nimport other.*;");
        assert!(refuse(&foreign, "title", "Integer", "TitleConverter", false).contains("wildcard"));
        let shared = "package shop;\n\nimport jakarta.persistence.Entity;\n\n@Entity public class Invoice { private String title; }\n";
        assert!(source(&doc(shared)).is_ok_and(|s| s.candidates.is_empty()) || render(&doc(shared), "title", "Integer", "TitleConverter", false).is_err());
    }

    #[test]
    fn kotlin_access_and_property_access_are_refused() {
        let kotlin = "package shop\n\n@jakarta.persistence.Entity\nopen class Invoice {\n    var title: kotlin.String? = null\n}\n";
        let document = Document { path: "/project/src/main/kotlin/shop/Invoice.kt".into(), language: "kotlin".into(), text: kotlin.into(), version: 1 };
        assert!(eligible(&document));
        let Err(message) = source(&document) else { panic!("kotlin must be refused") };
        assert!(message.contains("Java entities only"), "{message}");
        for text in [
            ENTITY.replace("@Entity\n", "@Entity\n@jakarta.persistence.Access(jakarta.persistence.AccessType.FIELD)\n"),
            ENTITY.replace("    private Boolean paid;", "    @Column\n    public Boolean getPaid() { return null; }"),
            ENTITY.replace("class Invoice {", "class Invoice extends Base {"),
        ] {
            assert!(source(&doc(&text)).is_err() && !eligible(&doc(&text)), "{text}");
        }
    }

    fn pick(plugin: &mut JpaAssist, document: &Document) -> FileAction {
        plugin.file_actions(document, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap()
    }
    fn answer(host: &MockHost, attribute: &str, database: &str, name: &str, mode: &str) {
        for value in [attribute, database, name, mode] {
            host.answer_next_prompt(Some(&[value]));
        }
    }

    #[test]
    fn resolve_returns_one_versioned_edit_and_a_no_overwrite_create() {
        let (host, mut plugin, document) = setup(ENTITY);
        let action = pick(&mut plugin, &document);
        answer(&host, "title", "Integer", "TitleConverter", "explicit");
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&document, action).unwrap();
        assert_eq!(result.edits.len(), 1);
        assert_eq!(result.edits[0].0, PATH);
        let [FileOperation::Create { path, text, overwrite }] = result.operations.as_slice() else { panic!("one create") };
        assert_eq!(path, "/project/src/main/java/shop/TitleConverter.java");
        assert!(!overwrite && text.contains("AttributeConverter<String, Integer>"));
        assert_eq!(host.editor_text(), ENTITY);
    }

    #[test]
    fn resolve_auto_apply_creates_only_the_file() {
        let (host, mut plugin, document) = setup(ENTITY);
        let action = pick(&mut plugin, &document);
        answer(&host, "paid", "Integer", "PaidConverter", "auto");
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&document, action).unwrap();
        assert!(result.edits.is_empty());
        let [FileOperation::Create { text, overwrite, .. }] = result.operations.as_slice() else { panic!("one create") };
        assert!(!overwrite && text.contains("@Converter(autoApply = true)"));
    }

    #[test]
    fn cancellation_collision_and_source_drift_never_apply_changes() {
        let (host, mut plugin, document) = setup(ENTITY);
        let action = pick(&mut plugin, &document);
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&document, action), Err(Error::Cancelled)));

        let action = pick(&mut plugin, &document);
        answer(&host, "title", "Integer", "TitleConverter", "explicit");
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&document, action), Err(Error::Cancelled)));

        host.add_file("src/main/java/shop/TitleConverter.kt", "class TitleConverter");
        let action = pick(&mut plugin, &document);
        answer(&host, "title", "Integer", "TitleConverter", "explicit");
        assert!(plugin.resolve_file_action(&document, action).is_err());

        let action = pick(&mut plugin, &document);
        host.type_in_editor("// changed");
        assert!(plugin.resolve_file_action(&document, action).is_err());
    }

    #[test]
    fn oversized_sources_are_not_offered() {
        let text = format!("{ENTITY}// {}\n", "x".repeat(MAX_TEXT));
        let (_host, mut plugin, document) = setup(&text);
        assert!(!plugin.file_actions(&document, Range::default()).iter().any(|a| a.title == TITLE));
    }

    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_API / JPA_JAVAX_API jar paths"]
    fn generated_converters_compile_against_both_persistence_apis() {
        for (ns, variable) in [("jakarta.persistence", "JPA_JAKARTA_API"), ("javax.persistence", "JPA_JAVAX_API")] {
            let jar = std::env::var(variable).expect("explicit API jar path");
            let text = ENTITY.replace("jakarta.persistence", ns);
            let mut count = 0;
            for (attribute, ty) in [("title", "String"), ("paid", "Boolean")] {
                for database in TYPES.iter().filter(|t| **t != ty) {
                    for auto in [false, true] {
                        count += 1;
                        let name = format!("{ty}To{database}Converter");
                        let (_, converter, edits) = render(&doc(&text), attribute, database, &name, auto).unwrap();
                        let entity = patched(&text, &edits).unwrap();
                        let dir = std::env::temp_dir().join(format!("jpa-converter-{}-{ns}-{count}", std::process::id()));
                        std::fs::create_dir_all(&dir).unwrap();
                        let files = [(format!("{name}.java"), converter), ("Invoice.java".to_owned(), entity)];
                        let paths: Vec<_> = files
                            .iter()
                            .map(|(file, body)| {
                                let path = dir.join(file);
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
                        assert!(result.status.success(), "{ns} {name} auto={auto}: {}\n{files:?}", String::from_utf8_lossy(&result.stderr));
                        std::fs::remove_dir_all(dir).unwrap();
                    }
                }
            }
            assert_eq!(count, 12);
        }
    }
}
