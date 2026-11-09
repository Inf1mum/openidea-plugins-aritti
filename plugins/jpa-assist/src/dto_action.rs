//! Reviewed source-derived DTO creation. No database/schema facts or guessed
//! association IDs participate; existing copy-only generators remain unchanged.
use super::*;
const MAX_FIELDS: usize = 32;
const PLATFORM: &[(&str, &str)] = &[
    ("BigDecimal", "java.math.BigDecimal"),
    ("BigInteger", "java.math.BigInteger"),
    ("UUID", "java.util.UUID"),
    ("LocalDate", "java.time.LocalDate"),
    ("LocalDateTime", "java.time.LocalDateTime"),
    ("LocalTime", "java.time.LocalTime"),
    ("Instant", "java.time.Instant"),
    ("OffsetDateTime", "java.time.OffsetDateTime"),
    ("ZonedDateTime", "java.time.ZonedDateTime"),
    ("Duration", "java.time.Duration"),
    ("Date", "java.util.Date"),
];
pub(super) fn supported(class: &Class, ty: &str) -> bool {
    let native = if class.kotlin {
        ["String", "Long", "Int", "Short", "Byte", "Boolean", "Double", "Float", "Char", "ByteArray"].as_slice()
    } else {
        [
            "String",
            "Long",
            "Integer",
            "Short",
            "Byte",
            "Boolean",
            "Double",
            "Float",
            "Character",
            "long",
            "int",
            "short",
            "byte",
            "boolean",
            "double",
            "float",
            "char",
            "byte[]",
        ]
        .as_slice()
    };
    let simple = ty.rsplit('.').next().unwrap_or(ty);
    let explicit: Vec<_> = class.imports.iter().filter(|import| import.rsplit('.').next() == Some(simple)).collect();
    if native.contains(&ty) {
        let namespace = if class.kotlin { "kotlin" } else { "java.lang" };
        return explicit.iter().all(|import| import.as_str() == format!("{namespace}.{simple}"));
    }
    PLATFORM.iter().any(|(name, fqcn)| {
        if ty == *fqcn {
            return true;
        }
        ty == *name && explicit.len() == 1 && explicit[0].as_str() == *fqcn
    })
}
pub(super) fn source(doc: &Document) -> Option<Class> {
    let class = current_class(doc)?;
    let masked = mask(&doc.text);
    let at = words(&masked, "class").next()?;
    let header = masked[at + 5..].trim_start().strip_prefix(&class.name)?.trim_start();
    if header.starts_with('<') || class.attributes.len() > 128 {
        return None;
    }
    Some(class)
}
fn fields(class: &Class) -> Option<Vec<(String, String)>> {
    let mut names = HashSet::new();
    if class.attributes.iter().any(|attribute| !names.insert(attribute.name.as_str())) {
        return None;
    }
    let mut candidate = class.clone();
    candidate.attributes.retain(|attribute| {
        attribute.relation == Relation::None
            && identifier(&attribute.name)
            && supported(class, &attribute.ty)
            && (class.kotlin
                || !["clone", "finalize", "getClass", "hashCode", "notify", "notifyAll", "toString", "wait"]
                    .contains(&attribute.name.as_str()))
    });
    let none = |_: &str| None;
    let fields = dto_fields(&candidate, &none);
    (!fields.is_empty() && fields.len() <= MAX_FIELDS).then_some(fields)
}
pub(super) fn eligible(doc: &Document) -> bool {
    source(doc).as_ref().and_then(fields).is_some()
}
pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let mut class = source(doc).ok_or(Error::Stale)?;
    let available = fields(&class).ok_or("No bounded unambiguous scalar fields are available for a DTO.")?;
    let default = format!("{}Dto", class.name);
    let Some(name) = host::input("DTO type name (same package; ending in Dto)", Some(&default))? else {
        return Err(Error::Cancelled);
    };
    if !identifier(&name)
        || !name.ends_with("Dto")
        || name == class.name
        || class.imports.iter().any(|import| import.rsplit('.').next() == Some(name.as_str()))
        || class.attributes.iter().any(|attribute| attribute.ty.rsplit('.').next() == Some(name.as_str()))
    {
        return Err("Use a non-conflicting Java/Kotlin identifier ending in Dto (at most 128 bytes).".into());
    }
    current(pending, doc)?;
    let items: Vec<_> = available.iter().map(|(name, ty)| PickItem::new(name, name).description(ty)).collect();
    let Some(chosen) = host::pick_many("Source scalar fields (relationships, embedded and unknown types omitted)", &items)? else {
        return Err(Error::Cancelled);
    };
    let mut selected = HashSet::new();
    if chosen.is_empty()
        || chosen.len() > MAX_FIELDS
        || chosen.iter().any(|name| !available.iter().any(|(field, _)| field == name) || !selected.insert(name.as_str()))
    {
        return Err("Choose one or more offered scalar fields exactly once.".into());
    }
    class.attributes.retain(|attribute| attribute.relation == Relation::None && selected.contains(attribute.name.as_str()));
    let none = |_: &str| None;
    let generated = dto(&class, &none, Some(&chosen));
    let original = if class.kotlin { format!("data class {default}(") } else { format!("public record {default}(") };
    let replacement = if class.kotlin { format!("data class {name}(") } else { format!("public record {name}(") };
    if !generated.contains(&original) {
        return Err(Error::Stale);
    }
    let generated = generated.replacen(&original, &replacement, 1);
    let declared = package(doc).ok_or(Error::Stale)?;
    if generated.len() > MAX_TEXT || !generated.starts_with(&format!("package {declared}{}\n", if class.kotlin { "" } else { ";" })) {
        return Err("DTO output exceeds the validated source/package bound.".into());
    }
    if generated
        .lines()
        .filter_map(|line| line.trim().strip_prefix("import "))
        .any(|import| import.trim_end_matches(';').rsplit('.').next() == Some(name.as_str()))
    {
        return Err("DTO name conflicts with an imported output type.".into());
    }
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let target = format!("{directory}/{name}.{}", if class.kotlin { "kt" } else { "java" });
    if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
        return Err(Error::Stale);
    }
    current(pending, doc)?;
    no_collision(project, &pending.root, directory, &name, None)?;
    let note = if class.kotlin {
        "Kotlin data class: selected transfer fields are nullable with null defaults."
    } else {
        "Java record: requires Java 16 or newer; compiler/source level was not verified."
    };
    let preview = format!(
        "Source: {}\nSource version: {}\nPackage: {declared}\nTarget: {target}\n\n{note}\nSource-derived draft only: no DB/schema diff, mapper, inherited fields or association-ID guesses.\n\nExact new file:\n{generated}\nOne atomic host-managed Create; no existing file is overwritten.",
        doc.path, doc.version
    );
    if !ui_kit::prompts::confirm_exact("Create source-derived scalar DTO?", &preview)? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    no_collision(project, &pending.root, directory, &name, None)?;
    Ok(FileAction {
        title: format!("Create scalar DTO {name}"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target.clone(), text: generated, overwrite: false }],
        open: Some(target),
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const JAVA: &str = "package shop;\nimport jakarta.persistence.*;\nimport java.time.LocalDate;\n@Entity\npublic class Customer {\n @Id Long id;\n String title;\n LocalDate joined;\n @ManyToOne Other parent;\n @Embedded Address address;\n Money amount;\n}\n";
    fn setup(path: &str, source: &str, language: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(path, language, source, &[]);
        let doc = Document { path: path.into(), text: source.into(), language: language.into(), version: 1 };
        (host, JpaAssist::default(), doc)
    }
    fn action(plugin: &mut JpaAssist, doc: &Document) -> FileAction {
        let action = plugin
            .file_actions(doc, Range::default())
            .into_iter()
            .find(|action| action.title == "Create scalar DTO with preview…")
            .unwrap();
        assert!(action.operations.is_empty() && action.edits.is_empty());
        action
    }
    fn answer(host: &MockHost, name: &str, fields: &[&str], confirm: bool) {
        host.answer_next_prompt(Some(&[name]));
        host.answer_next_prompt(Some(fields));
        host.answer_next_prompt(confirm.then_some(&[]));
    }
    fn created(action: &FileAction) -> (&str, &str) {
        let [FileOperation::Create { path, text, overwrite: false }] = action.operations.as_slice() else {
            panic!("one non-overwriting create")
        };
        assert!(action.edits.is_empty());
        (path, text)
    }
    #[test]
    fn java_dto_is_exact_reviewed_create_from_selected_current_source_fields_only() {
        let (host, mut plugin, doc) = setup("/project/src/main/java/shop/Customer.java", JAVA, "java");
        host.add_file(&doc.path, &JAVA.replace("String title;", "String oldDiskField;"));
        let pending = action(&mut plugin, &doc);
        answer(&host, "CustomerSummaryDto", &["id", "title", "joined"], true);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let (path, text) = created(&resolved);
        assert_eq!(path, "/project/src/main/java/shop/CustomerSummaryDto.java");
        assert_eq!(resolved.open.as_deref(), Some(path));
        assert!(
            text.contains("package shop;")
                && text.contains("import java.time.LocalDate;")
                && text.contains("public record CustomerSummaryDto(")
        );
        for field in ["Long id", "String title", "LocalDate joined"] {
            assert!(text.contains(field), "{text}");
        }
        for omitted in ["oldDiskField", "parentId", "address", "amount"] {
            assert!(!text.contains(omitted));
        }
        assert_eq!(host.editor_text(), JAVA);
        let project = host.invocation().project().unwrap();
        assert!(matches!(project.read_file(path), Err(Error::NotFound(_))));
        assert!(host.requests().is_empty());
    }
    #[test]
    fn kotlin_reuses_data_class_generator_and_documents_nullable_transfer_shape() {
        let source =
            "package shop\nimport jakarta.persistence.*\n@Entity\nclass Customer(@Id var id: Long? = null, var title: String = \"\") {}\n";
        let (host, mut plugin, doc) = setup("/project/src/main/kotlin/shop/Customer.kt", source, "kotlin");
        let pending = action(&mut plugin, &doc);
        answer(&host, "CustomerDto", &["id", "title"], true);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let (path, text) = created(&resolved);
        assert!(path.ends_with("/CustomerDto.kt"));
        assert!(text.contains("package shop\n") && text.contains("data class CustomerDto(") && text.contains("val title: String? = null"));
        assert_eq!(host.editor_text(), source);
    }
    #[test]
    fn each_cancellation_stage_and_forged_field_selection_leave_files_untouched() {
        for stage in 0..3 {
            let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", JAVA, "java");
            let pending = action(&mut plugin, &doc);
            if stage > 0 {
                host.answer_next_prompt(Some(&["CustomerDto"]));
            }
            if stage > 1 {
                host.answer_next_prompt(Some(&["id"]));
            }
            assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)));
            assert_eq!(host.editor_text(), JAVA);
        }
        for fields in [&["parentId"][..], &["id", "id"][..], &[][..]] {
            let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", JAVA, "java");
            let pending = action(&mut plugin, &doc);
            answer(&host, "CustomerDto", fields, true);
            assert!(plugin.resolve_file_action(&doc, pending).is_err());
            assert_eq!(host.editor_text(), JAVA);
        }
    }
    #[test]
    fn target_names_existing_files_and_declared_type_collisions_are_refused() {
        for name in ["../EscapeDto", "wrong.PackageDto", "Customer", "class", "CustomerDto;", "x\nDto"] {
            let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", JAVA, "java");
            let pending = action(&mut plugin, &doc);
            answer(&host, name, &["id"], true);
            assert!(plugin.resolve_file_action(&doc, pending).is_err());
        }
        for (extension, body) in [("java", "package shop; public class CustomerDto {}"), ("kt", "package shop\nclass CustomerDto")] {
            let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", JAVA, "java");
            host.add_file(&format!("src/shop/CustomerDto.{extension}"), body);
            let pending = action(&mut plugin, &doc);
            answer(&host, "CustomerDto", &["id"], true);
            assert!(plugin.resolve_file_action(&doc, pending).is_err());
        }
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", JAVA, "java");
        host.add_file("src/shop/Other.java", "package shop; interface CustomerDto {}");
        let pending = action(&mut plugin, &doc);
        answer(&host, "CustomerDto", &["id"], true);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
    }
    #[test]
    fn stale_source_project_changes_and_unprovable_entity_shapes_have_no_output() {
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", JAVA, "java");
        let pending = action(&mut plugin, &doc);
        let mut changed = doc.clone();
        changed.version += 1;
        assert!(matches!(plugin.resolve_file_action(&changed, pending.clone()), Err(Error::Stale)));
        host.type_in_editor("// changed");
        assert!(matches!(plugin.resolve_file_action(&doc, pending.clone()), Err(Error::Stale)));
        host.set_project_root(Some("/other"));
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        for source in [
            JAVA.replace("class Customer", "class Customer<T>"),
            JAVA.replace("class Customer", "class Customer extends Base"),
            JAVA.replace("package shop;", "package wrong;"),
            JAVA.replace("import jakarta.persistence.*;", "import unknown.Entity;"),
        ] {
            let (_host, mut plugin, doc) = setup("/project/src/shop/Customer.java", &source, "java");
            assert!(!plugin.file_actions(&doc, Range::default()).iter().any(|action| action.title.starts_with("Create scalar DTO")));
        }
    }
    #[test]
    fn shadowed_imports_relationships_unknown_types_and_output_bounds_are_conservative() {
        let source = JAVA.replace("import java.time.LocalDate;", "import foreign.String;\nimport foreign.LocalDate;");
        let (_host, _, doc) = setup("/project/src/shop/Customer.java", &source, "java");
        let class = source_class(&doc);
        let offered = fields(&class).unwrap();
        assert_eq!(offered, [("id".into(), "Long".into())]);
        let source = JAVA.replace(
            "@Id Long id;",
            &format!("@Id Long id;\n{}", (0..MAX_FIELDS).map(|n| format!("String field{n};\n")).collect::<String>()),
        );
        let (_host, mut plugin, doc) = setup("/project/src/shop/Customer.java", &source, "java");
        assert!(!plugin.file_actions(&doc, Range::default()).iter().any(|action| action.title.starts_with("Create scalar DTO")));
    }
    fn source_class(doc: &Document) -> Class {
        source(doc).unwrap()
    }
    #[test]
    fn dirty_destination_and_bounded_folder_discovery_refuse_creates() {
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", JAVA, "java");
        host.add_file(&doc.path, JAVA);
        let pending = action(&mut plugin, &doc);
        host.open_editor("/project/src/shop/CustomerDto.java", "java", "unsaved destination", &[]);
        answer(&host, "CustomerDto", &["id"], true);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert_eq!(host.editor_text(), "unsaved destination");
        let (host, mut plugin, doc) = setup("/project/src/shop/Customer.java", JAVA, "java");
        for n in 0..65 {
            host.add_file(&format!("src/shop/Type{n}.java"), &format!("package shop; class Type{n} {{}}"));
        }
        let pending = action(&mut plugin, &doc);
        answer(&host, "CustomerDto", &["id"], true);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert_eq!(host.editor_text(), JAVA);
    }
}
