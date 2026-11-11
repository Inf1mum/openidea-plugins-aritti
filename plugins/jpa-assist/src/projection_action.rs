//! Reviewed creation of a Spring Data projection type for a plain entity: closed interface
//! projection, Java record, or Kotlin data class. One new sibling file (`overwrite: false`).
//! The identifier-only interface projection of the `spring-data` plugin is the precursor; field
//! eligibility is `dto_action`'s. Nested projections, `@Value` (open) projections and
//! `@PersistenceCreator` are out of scope.
//!
//! Spring Data docs: a closed projection interface exposes accessors that "exactly match
//! properties in the aggregate root"; DTO projections need a single constructor whose parameter
//! names are the properties, and Java records fit. Interface accessors are resolved through
//! `BeanUtils.findPropertyForMethod` (java.beans), so `is` getters are only recognised for
//! primitive `boolean`.
use super::*;

pub(super) const TITLE: &str = "Create projection with preview…";

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Form {
    Interface,
    Record,
    DataClass,
}

fn forms(kotlin: bool) -> [(Form, &'static str, &'static str); 2] {
    if kotlin {
        [(Form::Interface, "interface", "Closed interface projection (val properties)"), (Form::DataClass, "data-class", "Kotlin data class (constructor projection)")]
    } else {
        [(Form::Interface, "interface", "Closed interface projection (getters)"), (Form::Record, "record", "Java record (constructor projection, Java 16+)")]
    }
}

pub(super) fn eligible(doc: &Document) -> bool {
    dto_action::eligible(doc)
}

pub(super) fn offered(class: &Class, form: Form) -> Vec<(String, String)> {
    let mut fields = dto_action::fields(class).unwrap_or_default();
    if form == Form::Interface {
        fields.retain(|(name, _)| {
            let mut chars = name.chars();
            let first = chars.next().is_some_and(|c| c.is_ascii_lowercase());
            let second_upper = chars.next().is_some_and(|c| c.is_ascii_uppercase());
            let kotlin_is = class.kotlin && name.strip_prefix("is").and_then(|r| r.chars().next()).is_some_and(|c| c.is_ascii_uppercase());
            first && !second_upper && !kotlin_is
        });
    }
    fields
}

pub(super) fn getter(name: &str, ty: &str) -> String {
    let mut chars = name.chars();
    let head = chars.next().map(|c| c.to_ascii_uppercase()).unwrap_or_default();
    let prefix = if ty == "boolean" { "is" } else { "get" };
    format!("{prefix}{head}{}", chars.as_str())
}

fn render(class: &Class, package_name: &str, form: Form, name: &str, fields: &[(String, String)]) -> String {
    let semicolon = if class.kotlin { "" } else { ";" };
    let mut imports: Vec<String> =
        fields.iter().filter_map(|(_, ty)| class.qualified(ty.trim_end_matches('?').split('<').next().unwrap_or(ty).rsplit('.').next().unwrap_or(ty))).collect();
    imports.sort();
    imports.dedup();
    imports.retain(|i| i.rsplit_once('.').map(|(pk, _)| pk) != Some(package_name));
    let mut out = format!("package {package_name}{semicolon}\n\n");
    for import in &imports {
        out.push_str(&format!("import {import}{semicolon}\n"));
    }
    if !imports.is_empty() {
        out.push('\n');
    }
    match (form, class.kotlin) {
        (Form::Interface, false) => {
            let lines: String = fields.iter().map(|(n, t)| format!("    {t} {}();\n", getter(n, t))).collect();
            out.push_str(&format!("public interface {name} {{\n{lines}}}\n"));
        }
        (Form::Interface, true) => {
            let lines: String = fields.iter().map(|(n, t)| format!("    val {n}: {}?\n", t.trim_end_matches('?'))).collect();
            out.push_str(&format!("interface {name} {{\n{lines}}}\n"));
        }
        (_, false) => {
            let params: Vec<_> = fields.iter().map(|(n, t)| format!("{t} {n}")).collect();
            out.push_str(&format!("public record {name}(\n        {}\n) {{\n}}\n", params.join(",\n        ")));
        }
        (_, true) => {
            let params: Vec<_> = fields.iter().map(|(n, t)| format!("    val {n}: {}?", t.trim_end_matches('?'))).collect();
            out.push_str(&format!("data class {name}(\n{}\n)\n", params.join(",\n")));
        }
    }
    out
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let class = dto_action::source(doc).ok_or(Error::Stale)?;
    let choices: Vec<_> = forms(class.kotlin).iter().map(|(_, key, label)| PickItem::new(key, label)).collect();
    let key = host::pick("Projection form", &choices)?.ok_or(Error::Cancelled)?;
    let form = forms(class.kotlin).iter().find(|(_, k, _)| *k == key).map(|(f, _, _)| *f).ok_or("Choose one of the offered forms.")?;
    let available = offered(&class, form);
    if available.is_empty() {
        return Err("No plain scalar attribute has a name usable as a projection property.".into());
    }
    let default = format!("{}Projection", class.name);
    let name = host::input("Projection type name (same package)", Some(&default))?.ok_or(Error::Cancelled)?.trim().to_owned();
    if !identifier(&name)
        || name == class.name
        || class.imports.iter().any(|import| import.rsplit('.').next() == Some(name.as_str()))
        || class.attributes.iter().any(|attribute| attribute.ty.rsplit('.').next() == Some(name.as_str()))
    {
        return Err("Use a non-conflicting Java/Kotlin identifier for the projection.".into());
    }
    current(pending, doc)?;
    let items: Vec<_> = available.iter().map(|(n, t)| PickItem::new(n, n).description(t)).collect();
    let chosen = host::pick_many("Projection properties (plain scalar attributes only)", &items)?.ok_or(Error::Cancelled)?;
    let mut seen = HashSet::new();
    if chosen.is_empty() || chosen.iter().any(|n| !available.iter().any(|(f, _)| f == n) || !seen.insert(n.as_str())) {
        return Err("Choose one or more offered attributes exactly once.".into());
    }
    let fields: Vec<_> = available.iter().filter(|(n, _)| chosen.contains(n)).cloned().collect();
    let reuse_directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    dto_convert_action::offer_reuse(project, &pending.root, reuse_directory, class.kotlin, &doc.path, &fields)?;
    let declared = package(doc).ok_or(Error::Stale)?;
    let mut text = render(&class, &declared, form, &name, &fields);
    if text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("import "))
        .any(|import| import.trim_end_matches(';').rsplit('.').next() == Some(name.as_str()))
    {
        return Err("Projection name conflicts with an imported type.".into());
    }
    if doc.text.contains("\r\n") {
        text = text.replace('\n', "\r\n");
    }
    if text.len() > MAX_TEXT {
        return Err("Projection output exceeds the supported bound.".into());
    }
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let target = format!("{directory}/{name}.{}", if class.kotlin { "kt" } else { "java" });
    if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
        return Err(Error::Stale);
    }
    no_collision(project, &pending.root, directory, &name, None)?;
    let note = match form {
        Form::Interface => "Closed interface projection: accessor names must match entity properties; `is` getters only for primitive boolean.",
        Form::Record => "Java record: requires Java 16 or newer; compiler/source level was not verified. Constructor parameter names must match entity properties.",
        Form::DataClass => "Kotlin data class without defaults (single constructor); properties are nullable. Constructor parameter names must match entity properties.",
    };
    let preview = format!(
        "Source: {}\nSource version: {}\nTarget: {target}\n\n{note}\nNo nested or @Value (open) projections, relationships or inherited properties. Spring Data is not required to compile this file.\n\nExact new file:\n{text}\nOne atomic host-managed Create; no existing file is overwritten.",
        doc.path, doc.version
    );
    if !ui_kit::prompts::confirm_exact("Create Spring Data projection?", &preview)? {
        return Err(Error::Cancelled);
    }
    let project = current(pending, doc)?;
    no_collision(&project, &pending.root, directory, &name, None)?;
    Ok(FileAction {
        title: format!("Create projection {name}"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target.clone(), text, overwrite: false }],
        open: Some(target),
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const JAVA: &str = "package shop;\nimport jakarta.persistence.*;\nimport java.time.LocalDate;\n@Entity\npublic class Customer {\n @Id Long id;\n String title;\n boolean active;\n Boolean archived;\n LocalDate joined;\n int aB;\n @ManyToOne Other parent;\n @Embedded Address address;\n Money amount;\n}\n";
    const KOTLIN: &str = "package shop\nimport jakarta.persistence.*\n@Entity\nclass Customer(@Id var id: Long? = null, var title: String = \"\", var active: Boolean = false, var isFlag: Boolean = false) {}\n";
    const JPATH: &str = "/project/src/main/java/shop/Customer.java";
    const KPATH: &str = "/project/src/main/kotlin/shop/Customer.kt";

    fn run(path: &str, source: &str, language: &str, form: &str, name: &str, fields: &[&str], confirm: bool) -> (MockHost, Result<FileAction, Error>) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(path, language, source, &[]);
        let doc = Document { path: path.into(), text: source.into(), language: language.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        assert!(pending.operations.is_empty() && pending.edits.is_empty());
        host.answer_next_prompt(Some(&[form]));
        host.answer_next_prompt(Some(&[name]));
        host.answer_next_prompt(Some(fields));
        host.answer_next_prompt(confirm.then_some(&[]));
        let result = plugin.resolve_file_action(&doc, pending);
        (host, result)
    }

    fn text_of(result: Result<FileAction, Error>) -> (String, String) {
        let action = result.unwrap();
        let [FileOperation::Create { path, text, overwrite: false }] = action.operations.as_slice() else { panic!("one non-overwriting create") };
        assert!(action.edits.is_empty());
        (path.clone(), text.clone())
    }

    #[test]
    fn java_interface_uses_bean_getters_and_is_only_for_primitive_boolean() {
        let (host, result) = run(JPATH, JAVA, "java", "interface", "CustomerView", &["id", "title", "active", "archived", "joined"], true);
        let (path, text) = text_of(result);
        assert_eq!(path, "/project/src/main/java/shop/CustomerView.java");
        assert_eq!(
            text,
            "package shop;\n\nimport java.time.LocalDate;\n\npublic interface CustomerView {\n    Long getId();\n    String getTitle();\n    boolean isActive();\n    Boolean getArchived();\n    LocalDate getJoined();\n}\n"
        );
        assert_eq!(host.editor_text(), JAVA);
    }

    #[test]
    fn java_record_keeps_property_names_and_order_of_the_entity() {
        let (_, result) = run(JPATH, JAVA, "java", "record", "CustomerProjection", &["joined", "id"], true);
        let (_, text) = text_of(result);
        assert_eq!(text, "package shop;\n\nimport java.time.LocalDate;\n\npublic record CustomerProjection(\n        Long id,\n        LocalDate joined\n) {\n}\n");
    }

    #[test]
    fn kotlin_interface_and_data_class_are_nullable_without_defaults() {
        let (_, result) = run(KPATH, KOTLIN, "kotlin", "interface", "CustomerView", &["id", "active"], true);
        assert_eq!(text_of(result).1, "package shop\n\ninterface CustomerView {\n    val id: Long?\n    val active: Boolean?\n}\n");
        let (_, result) = run(KPATH, KOTLIN, "kotlin", "data-class", "CustomerProjection", &["id", "title", "isFlag"], true);
        let (path, text) = text_of(result);
        assert!(path.ends_with("/CustomerProjection.kt"));
        assert_eq!(text, "package shop\n\ndata class CustomerProjection(\n    val id: Long?,\n    val title: String?,\n    val isFlag: Boolean?\n)\n");
    }

    #[test]
    fn interface_form_omits_names_with_ambiguous_bean_properties() {
        let (_, result) = run(JPATH, JAVA, "java", "interface", "CustomerView", &["aB"], true);
        assert!(result.is_err());
        let (_, result) = run(JPATH, JAVA, "java", "record", "CustomerProjection", &["aB"], true);
        assert!(text_of(result).1.contains("int aB"));
        let (_, result) = run(KPATH, KOTLIN, "kotlin", "interface", "CustomerView", &["isFlag"], true);
        assert!(result.is_err());
    }

    #[test]
    fn relationships_embedded_unknown_and_forged_selections_are_refused() {
        for fields in [&["parentId"][..], &["address"][..], &["amount"][..], &["id", "id"][..], &[][..]] {
            let (host, result) = run(JPATH, JAVA, "java", "record", "CustomerProjection", fields, true);
            assert!(result.is_err());
            assert_eq!(host.editor_text(), JAVA);
        }
        let (_, result) = run(JPATH, JAVA, "java", "data-class", "CustomerProjection", &["id"], true);
        assert!(result.is_err());
        let (_, result) = run(JPATH, JAVA, "java", "bogus", "CustomerProjection", &["id"], true);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_or_conflicting_names_are_refused() {
        for name in ["../Escape", "Customer", "class", "Bad;", "LocalDate"] {
            let (_, result) = run(JPATH, JAVA, "java", "record", name, &["id"], true);
            assert!(result.is_err(), "{name}");
        }
    }

    #[test]
    fn declined_confirmation_cancels_and_every_prompt_stage_cancels() {
        let (host, result) = run(JPATH, JAVA, "java", "record", "CustomerProjection", &["id"], false);
        assert!(matches!(result, Err(Error::Cancelled)));
        assert_eq!(host.editor_text(), JAVA);
        for stage in 0..3 {
            let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
            host.open_editor(JPATH, "java", JAVA, &[]);
            let doc = Document { path: JPATH.into(), text: JAVA.into(), language: "java".into(), version: 1 };
            let mut plugin = JpaAssist::default();
            let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
            let answers: [&[&str]; 3] = [&["record"], &["CustomerProjection"], &["id"]];
            for answer in &answers[..stage] {
                host.answer_next_prompt(Some(answer));
            }
            assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)));
        }
    }

    #[test]
    fn existing_files_across_languages_and_stale_source_are_refused() {
        for (extension, body) in [("java", "package shop; public class CustomerProjection {}"), ("kt", "package shop\nclass CustomerProjection")] {
            let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
            host.open_editor(JPATH, "java", JAVA, &[]);
            host.add_file(&format!("src/main/java/shop/CustomerProjection.{extension}"), body);
            let doc = Document { path: JPATH.into(), text: JAVA.into(), language: "java".into(), version: 1 };
            let mut plugin = JpaAssist::default();
            let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
            for answer in [&["record"][..], &["CustomerProjection"], &["id"]] {
                host.answer_next_prompt(Some(answer));
            }
            host.answer_next_prompt(Some(&[]));
            assert!(plugin.resolve_file_action(&doc, pending).is_err(), "{extension}");
        }
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(JPATH, "java", JAVA, &[]);
        let doc = Document { path: JPATH.into(), text: JAVA.into(), language: "java".into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.type_in_editor("// changed");
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Stale)));
    }

    #[test]
    fn crlf_sources_produce_crlf_output_and_ineligible_entities_have_no_action() {
        let crlf = JAVA.replace('\n', "\r\n");
        let (_, result) = run(JPATH, &crlf, "java", "record", "CustomerProjection", &["id"], true);
        let text = text_of(result).1;
        assert!(text.contains("\r\n") && !text.replace("\r\n", "").contains('\n'));
        let source = JAVA.replace("class Customer", "class Customer extends Base");
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(JPATH, "java", &source, &[]);
        let doc = Document { path: JPATH.into(), text: source, language: "java".into(), version: 1 };
        assert!(!JpaAssist::default().file_actions(&doc, Range::default()).iter().any(|a| a.title == TITLE));
    }

    #[test]
    #[ignore = "requires javac, kotlinc classpath and JPA_JAKARTA_API"]
    fn generated_projection_types_compile() {
        let dir = std::env::temp_dir().join(format!("jpa-projection-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let jar = std::env::var("JPA_JAKARTA_API").expect("explicit API jar path");
        let java_entity = JAVA.replace("@ManyToOne Other parent;\n @Embedded Address address;\n Money amount;\n", "");
        let mut java_files = vec![("Customer.java".to_owned(), java_entity)];
        let java_fields = ["id", "title", "active", "archived", "joined"];
        for (form, name) in [("interface", "CustomerView"), ("record", "CustomerRecord")] {
            let (_, result) = run(JPATH, JAVA, "java", form, name, &java_fields, true);
            java_files.push((format!("{name}.java"), text_of(result).1));
        }
        let mut kotlin_files = vec![("Customer.kt".to_owned(), KOTLIN.to_owned())];
        for (form, name) in [("interface", "CustomerView"), ("data-class", "CustomerData")] {
            let (_, result) = run(KPATH, KOTLIN, "kotlin", form, name, &["id", "title", "active"], true);
            kotlin_files.push((format!("{name}.kt"), text_of(result).1));
        }
        let write = |sub: &str, files: &[(String, String)]| {
            let d = dir.join(sub);
            std::fs::create_dir_all(&d).unwrap();
            let paths: Vec<_> = files
                .iter()
                .map(|(n, b)| {
                    let p = d.join(n);
                    std::fs::write(&p, b).unwrap();
                    p
                })
                .collect();
            (d, paths)
        };
        let (d, paths) = write("java", &java_files);
        let out = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &jar, "-d"]).arg(&d).args(&paths).output().expect("javac installed");
        assert!(out.status.success(), "{}\n{java_files:?}", String::from_utf8_lossy(&out.stderr));
        let signatures = std::process::Command::new("javap").args(["-classpath"]).arg(&d).arg("shop.CustomerView").output().unwrap();
        assert!(String::from_utf8_lossy(&signatures.stdout).contains("boolean isActive();"));
        let compiler = std::env::var("KOTLIN_COMPILER_CLASSPATH").expect("explicit Kotlin compiler classpath");
        let stdlib = std::env::var("KOTLIN_STDLIB").expect("explicit Kotlin standard library jar");
        let (d, paths) = write("kotlin", &kotlin_files);
        let out = std::process::Command::new("java")
            .args(["-classpath", &compiler, "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler", "-no-stdlib", "-no-reflect", "-jvm-target", "17", "-classpath", &format!("{jar}:{stdlib}"), "-d"])
            .arg(&d)
            .args(&paths)
            .output()
            .expect("java installed");
        assert!(out.status.success(), "{}{}\n{kotlin_files:?}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
