//! Tools > JPA Buddy > DTO Declaration: how a generated DTO is named, commented and made serializable.
//! The linking settings (name regexp, comment link regexp) live in `projection_links` next to the projection ones.
use super::*;
use crate::model::import_edit;
use super::projection_links::{Conventions, Kind};

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Serializable {
    No,
    Plain,
    WithVersionUid,
}

pub(super) fn serializable() -> Result<Serializable, Error> {
    match host::setting_string("dtoSerializableType").as_deref() {
        None | Some("none") => Ok(Serializable::No),
        Some("serializable") => Ok(Serializable::Plain),
        Some("serializable-with-version-uid") => Ok(Serializable::WithVersionUid),
        Some(other) => Err(format!("Unknown dtoSerializableType `{other}`; use none, serializable or serializable-with-version-uid.").into()),
    }
}

pub(super) fn conventions() -> Result<Conventions, Error> {
    Ok(Conventions::load_for(Kind::Dto)?)
}

fn splice(text: &str, edits: Vec<crate::model::Edit>) -> String {
    let mut edits = edits;
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.start));
    let mut out = text.to_owned();
    for edit in edits {
        out.replace_range(edit.start..edit.end, &edit.text);
    }
    out
}

/// Offset just after the primary parameter list (records, Kotlin data classes) or just at the `{` (classes).
fn header_end(masked: &str, keyword: &str, name: &str) -> Option<usize> {
    let at = words(masked, keyword).find(|at| masked[at + keyword.len()..].trim_start().starts_with(name))?;
    let mut depth = 0usize;
    for (offset, ch) in masked[at..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            '{' if depth == 0 => return Some(at + offset),
            _ => {}
        }
    }
    Some(masked.len())
}

fn make_serializable(text: &str, name: &str, kotlin: bool, mode: Serializable) -> Result<String, String> {
    if mode == Serializable::No {
        return Ok(text.to_owned());
    }
    let masked = mask(text);
    let version_uid = mode == Serializable::WithVersionUid;
    let mut edits = Vec::new();
    if kotlin {
        let at = words(&masked, "class").find(|at| masked[at + 5..].trim_start().starts_with(name)).ok_or("The generated DTO has no class declaration.")?;
        let open = masked[at..].find('(').map(|offset| at + offset).ok_or("The generated DTO has no primary constructor.")?;
        let close = crate::model::closing(masked.as_bytes(), open).ok_or("The generated DTO has an unbalanced constructor.")?;
        let body = if version_uid { " {\n    companion object {\n        private const val serialVersionUID = 1L\n    }\n}" } else { "" };
        edits.push(crate::model::Edit::insert(close + 1, &format!(" : Serializable{body}")));
    } else {
        let keyword = if words(&masked, "record").any(|at| masked[at + 6..].trim_start().starts_with(name)) { "record" } else { "class" };
        let open = header_end(&masked, keyword, name).filter(|open| *open < masked.len()).ok_or("The generated DTO has no body.")?;
        let head = text[..open].trim_end().len();
        edits.push(crate::model::Edit::insert(head, " implements Serializable"));
        if version_uid {
            let body = "\n    private static final long serialVersionUID = 1L;";
            edits.push(crate::model::Edit::insert(open + 1, body));
        }
    }
    let mut out = splice(text, edits);
    if let Some(import) = import_edit(&out, &["java.io.Serializable"], kotlin) {
        out = splice(&out, vec![import]);
    }
    Ok(out)
}

/// Applies the DTO Declaration settings that shape a generated file: class comment and serializable type.
pub(super) fn finish(generated: String, class: &Class, name: &str, conventions: &Conventions, mode: Serializable) -> Result<String, Error> {
    let marked = make_serializable(&generated, name, class.kotlin, mode)?;
    Ok(conventions.with_comment(&marked, class.package.as_deref(), &class.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORD: &str = "package shop;\n\npublic record CustomerDto(\n        Long id,\n        String title\n) {\n}\n";
    const CLASS: &str = "package shop;\n\npublic class CustomerDto {\n\n    private Long id;\n}\n";
    const KOTLIN: &str = "package shop\n\ndata class CustomerDto(\n    val id: Long? = null\n)\n";

    #[test]
    fn record_gets_serializable_with_import_and_uid() {
        let plain = make_serializable(RECORD, "CustomerDto", false, Serializable::Plain).unwrap();
        assert_eq!(plain, "package shop;\n\nimport java.io.Serializable;\n\npublic record CustomerDto(\n        Long id,\n        String title\n) implements Serializable {\n}\n");
        let uid = make_serializable(RECORD, "CustomerDto", false, Serializable::WithVersionUid).unwrap();
        assert!(uid.contains(") implements Serializable {\n    private static final long serialVersionUID = 1L;\n}\n"), "{uid}");
    }

    #[test]
    fn class_and_kotlin_data_class_are_made_serializable() {
        let class = make_serializable(CLASS, "CustomerDto", false, Serializable::WithVersionUid).unwrap();
        assert!(class.contains("public class CustomerDto implements Serializable {\n    private static final long serialVersionUID = 1L;\n\n    private Long id;"), "{class}");
        let kotlin = make_serializable(KOTLIN, "CustomerDto", true, Serializable::WithVersionUid).unwrap();
        assert!(kotlin.contains("import java.io.Serializable") && kotlin.contains(") : Serializable {\n    companion object {\n        private const val serialVersionUID = 1L"), "{kotlin}");
        assert_eq!(make_serializable(CLASS, "CustomerDto", false, Serializable::No).unwrap(), CLASS);
    }

    #[test]
    #[ignore = "requires javac; run after sourcing scripts/jpa-compile-env.sh"]
    fn serializable_dtos_compile() {
        let dir = std::env::temp_dir().join(format!("jpa-dto-serializable-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, source) in [("CustomerDto", RECORD), ("CustomerData", CLASS)] {
            for mode in [Serializable::Plain, Serializable::WithVersionUid] {
                let text = make_serializable(&source.replace("CustomerDto", name), name, false, mode).unwrap();
                let package = dir.join(format!("{name}-{mode:?}"));
                std::fs::create_dir_all(&package).unwrap();
                let file = package.join(format!("{name}.java"));
                std::fs::write(&file, text).unwrap();
                let result = std::process::Command::new("javac").args(["--release", "17", "-d"]).arg(&package).arg(&file).output().expect("javac");
                assert!(result.status.success(), "{name} {mode:?}: {}", String::from_utf8_lossy(&result.stderr));
            }
        }
    }

    use openidea_plugin::mock::MockHost;

    const ENTITY: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Customer {\n    @Id Long id;\n    String title;\n    String phone;\n}\n";
    const ENTITY_PATH: &str = "/project/src/main/java/shop/Customer.java";

    #[test]
    fn dto_sync_links_by_the_dto_name_regexp_and_leaves_projections_alone() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/Customer.java", ENTITY);
        host.add_file("src/main/java/shop/CustomerDto.java", "package shop;\n\npublic record CustomerDto(Long id) {\n}\n");
        host.add_file("src/main/java/shop/CustomerProjection.java", "package shop;\n\npublic interface CustomerProjection {\n    Long getId();\n}\n");
        host.open_editor(ENTITY_PATH, "java", ENTITY, &[]);
        let doc = Document { path: ENTITY_PATH.into(), language: "java".into(), text: ENTITY.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == projection_links::SYNC_DTO_TITLE).expect("offered");
        host.answer_next_prompt(Some(&["toDtos"]));
        host.answer_next_prompt(Some(&["add:0:title", "add:0:phone"]));
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        let [(path, edits)] = action.edits.as_slice() else { panic!("one document") };
        assert!(path.ends_with("CustomerDto.java"));
        let text = patched("package shop;\n\npublic record CustomerDto(Long id) {\n}\n", edits).unwrap();
        assert!(text.contains("(Long id, String title, String phone)"), "{text}");
    }

    #[test]
    fn dto_settings_have_their_own_defaults_and_validation() {
        let host = MockHost::install();
        let conventions = Conventions::load_for(Kind::Dto).unwrap();
        assert_eq!(conventions.default_name("Customer"), "CustomerDto");
        host.set_setting("dtoClassPostfix", "\"Data\"");
        assert_eq!(Conventions::load_for(Kind::Dto).unwrap().default_name("Customer"), "CustomerData");
        assert_eq!(Conventions::load_for(Kind::Projection).unwrap().default_name("Customer"), "CustomerProjection");
        host.set_setting("dtoNameRegexp", "\"(Dto\"");
        assert!(Conventions::load_for(Kind::Dto).is_err());
        host.set_setting("dtoSerializableType", "\"bogus\"");
        assert!(serializable().is_err());
    }
}
