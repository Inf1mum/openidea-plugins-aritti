//! Managed-class generation into a verified empty current editor.
use crate::{file_actions, wizard};
use openidea_plugin::{Error, Project, providers::Document};
use persistence_core::{Language, SourceIdentity, generation::ManagedClassDraft};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CreationContext {
    pub name: String,
    pub package: String,
    pub source: SourceIdentity,
    pub default_table_name: String,
}

pub(super) fn context(doc: &Document, root: &str) -> Result<CreationContext, Error> {
    if !doc.text.trim().is_empty()
        || file_actions::path(&doc.path).as_deref() != Some(doc.path.as_str())
        || file_actions::path(root).as_deref() != Some(root)
        || !file_actions::inside(root, &doc.path)
    {
        return Err("Open an empty Java or Kotlin file to create a persistence class.".into());
    }
    let (language, folder, extension) = match doc.language.as_str() {
        "java" => (Language::Java, "java", ".java"),
        "kotlin" => (Language::Kotlin, "kotlin", ".kt"),
        _ => return Err("Choose a Java or Kotlin source file.".into()),
    };
    let relative = doc
        .path
        .strip_prefix(&format!("{}/", root.trim_end_matches('/')))
        .ok_or(Error::Stale)?;
    let segments: Vec<_> = relative.split('/').collect();
    let roots: Vec<_> = segments
        .windows(3)
        .enumerate()
        .filter(|(_, s)| s[0] == "src" && matches!(s[1], "main" | "test") && s[2] == folder)
        .map(|(i, _)| i + 3)
        .collect();
    let [start] = roots.as_slice() else {
        return Err(
            "Use a named package under an unambiguous src/main or src/test source root.".into(),
        );
    };
    if *start >= segments.len() - 1 {
        return Err("Persistence class creation requires a named package.".into());
    }
    let name = segments
        .last()
        .and_then(|s| s.strip_suffix(extension))
        .ok_or(Error::Stale)?;
    let package = segments[*start..segments.len() - 1].join(".");
    if !persistence_core::generation::identifier(name)
        || package.len() > 512
        || !package
            .split('.')
            .all(persistence_core::generation::identifier)
    {
        return Err("The source path must declare a valid class name and package.".into());
    }
    Ok(CreationContext {
        name: name.into(),
        package,
        source: SourceIdentity {
            root: root.into(),
            path: doc.path.clone(),
            language,
            version: doc.version,
        },
        default_table_name: file_actions::naming_table_name(name),
    })
}

pub(super) fn prepare(
    doc: &Document,
    root: &str,
    project: &Project,
    draft: &ManagedClassDraft,
) -> Result<String, Error> {
    draft.validate().map_err(Error::from)?;
    let target = context(doc, root)?;
    if draft.name != target.name {
        return Err("The created class name must match the invoking empty source filename.".into());
    }
    file_actions::no_collision(
        project,
        root,
        doc.path.rsplit_once('/').ok_or(Error::Stale)?.0,
        &draft.name,
        Some(&doc.path),
    )?;
    let source = wizard::configured_class_source(draft, &target.package, doc.language == "kotlin")
        .map_err(Error::from)?;
    if source.len() > 128 * 1024 {
        return Err("Generated source exceeds the supported preview size.".into());
    }
    Ok(if doc.text.contains("\r\n") {
        source.replace('\n', "\r\n")
    } else {
        source
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    use persistence_core::{
        PersistenceRole,
        generation::{GenerationStrategy, IdentifierDraft, IdentifierType, Namespace},
    };
    fn draft() -> ManagedClassDraft {
        ManagedClassDraft {
            name: "Customer".into(),
            role: PersistenceRole::Entity,
            namespace: Namespace::Jakarta,
            table_name: "customer".into(),
            identifier: Some(IdentifierDraft {
                name: "key".into(),
                column: "customer_id".into(),
                id_type: IdentifierType::Integer,
                strategy: GenerationStrategy::Identity,
                sequence: None,
                table: None,
            }),
            versioned: true,
            inheritance: None,
        }
    }
    fn doc(path: &str, language: &str) -> Document {
        Document {
            path: path.into(),
            language: language.into(),
            text: String::new(),
            version: 1,
        }
    }
    #[test]
    fn source_roots_are_language_owned_and_package_markers_are_not_guessed() {
        let _host = openidea_plugin::mock::MockHost::install();
        for (path, language, expected) in [
            (
                "/project/src/main/java/shop/java/Customer.java",
                "java",
                "shop.java",
            ),
            (
                "/project/module/src/test/kotlin/shop/Customer.kt",
                "kotlin",
                "shop",
            ),
        ] {
            assert_eq!(
                context(&doc(path, language), "/project").unwrap().package,
                expected
            );
        }
        for (path, language) in [
            ("/other/src/main/java/shop/Customer.java", "java"),
            ("/project/src/main/kotlin/shop/Customer.java", "java"),
            ("/project/src/main/java/Customer.java", "java"),
            ("/project/src/main/java/shop/../Customer.java", "java"),
            (
                "/project/src/main/java/shop/src/test/java/Customer.java",
                "java",
            ),
        ] {
            assert!(context(&doc(path, language), "/project").is_err(), "{path}");
        }
    }
    #[test]
    fn creation_refuses_nonempty_source_name_drift_and_other_language_collisions() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let mut document = doc("/project/src/main/java/shop/Customer.java", "java");
        host.open_editor(&document.path, "java", "", &[]);
        let project = host.invocation().project().unwrap();
        let source = prepare(&document, "/project", &project, &draft()).unwrap();
        assert!(
            source.contains("java.lang.Integer key")
                && source.contains("getKey()")
                && source.contains("GenerationType.IDENTITY")
        );
        let mut wrong = draft();
        wrong.name = "Other".into();
        assert!(prepare(&document, "/project", &project, &wrong).is_err());
        document.text = "// do not overwrite".into();
        assert!(prepare(&document, "/project", &project, &draft()).is_err());
        document.text.clear();
        host.add_file(
            "src/main/java/shop/Customer.kt",
            "package shop\nclass Customer",
        );
        assert!(prepare(&document, "/project", &project, &draft()).is_err());
        assert_eq!(host.editor_text(), "");
    }
    #[test]
    fn kotlin_roles_and_crlf_render_without_import_or_identifier_guessing() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let mut document = doc("/project/src/main/kotlin/shop/Customer.kt", "kotlin");
        document.text = "\r\n".into();
        host.open_editor(&document.path, "kotlin", &document.text, &[]);
        let project = host.invocation().project().unwrap();
        let source = prepare(&document, "/project", &project, &draft()).unwrap();
        assert!(
            source.contains("var key: kotlin.Int? = null")
                && !source.replace("\r\n", "").contains('\n')
        );
        let mut role = draft();
        role.role = PersistenceRole::Embeddable;
        role.identifier = None;
        role.table_name.clear();
        role.versioned = false;
        let source = prepare(&document, "/project", &project, &role).unwrap();
        assert!(
            source.contains("@jakarta.persistence.Embeddable")
                && !source.contains("@jakarta.persistence.Id")
                && !source.contains("@jakarta.persistence.Table")
        );
    }
}
