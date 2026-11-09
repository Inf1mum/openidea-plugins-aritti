//! Source-local repository creation. Only the host applies the atomic workspace edit.
use super::*;
use openidea_plugin::file_actions::{FileAction, FileActions, FileOperation};
use std::time::{Duration, Instant};

const MAX_TEXT: usize = 128 * 1024;
const ENTITIES: &[&str] = &[
    "jakarta.persistence.Entity",
    "javax.persistence.Entity",
    "org.springframework.data.mongodb.core.mapping.Document",
    "org.springframework.data.relational.core.mapping.Table",
];
#[derive(Clone)]
struct Entity {
    name: String,
    at: usize,
    package: String,
    id: String,
}
#[derive(Clone)]
struct Pending {
    root: String,
    doc: Document,
    entity: Entity,
}
#[derive(Default)]
pub(crate) struct Generation {
    next: u64,
    pending: BTreeMap<String, Pending>,
}

fn identifier(value: &str) -> bool {
    let mut chars = value.chars();
    value.len() <= 128 && chars.next().is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && !"_ abstract as assert boolean break byte case catch char class companion const continue data default do double else enum extends false final finally float for fun if implements import in instanceof int interface is long native new null object open out override package permits private protected public record return sealed short static strictfp super switch synchronized this throw throws trait transient true try typealias typeof val var void volatile when while yield"
            .split_ascii_whitespace().any(|keyword| keyword == value)
}
fn path(value: &str) -> Option<String> {
    let value = value.replace('\\', "/");
    let absolute = value.starts_with('/')
        || (value.as_bytes().get(1) == Some(&b':') && value.as_bytes().get(2) == Some(&b'/') && value.as_bytes()[0].is_ascii_alphabetic());
    (absolute
        && value.len() <= 4096
        && !value.chars().any(char::is_control)
        && !value.split('/').any(|part| matches!(part, "." | ".."))
        && !value.contains(['*', '?', '[', ']', '{', '}']))
    .then_some(value)
}
fn inside(root: &str, file: &str) -> bool {
    file.starts_with(&format!("{}/", root.trim_end_matches('/')))
}
fn balanced(text: &str) -> bool {
    let mut stack = Vec::new();
    for token in java::tokens(text) {
        if token.s.starts_with('"') && (!token.s.ends_with('"') || token.s.len() < 2) {
            return false;
        }
        match token.s {
            "(" => stack.push(")"),
            "[" => stack.push("]"),
            "{" => stack.push("}"),
            ")" | "]" | "}" => {
                if stack.pop() != Some(token.s) {
                    return false;
                }
            }
            _ => {}
        }
    }
    stack.is_empty()
}
fn annotation_namespace(file: &JavaFile, text: &str, annotation: &java::Annotation) -> Option<String> {
    let tokens = java::tokens(&text[annotation.at..annotation.name_at + annotation.name.len()]);
    let mut names = &tokens[1..];
    if names.get(1).is_some_and(|token| token.s == ":") {
        if annotation.name != "Id" || !matches!(names[0].s, "field" | "get") {
            return None;
        }
        names = &names[2..];
    }
    let written: String = names.iter().map(|token| token.s).collect();
    if written.contains('.') {
        return Some(written);
    }
    if file.types.iter().any(|ty| ty.name == written) {
        return None;
    }
    let imports: Vec<_> =
        file.imports.iter().filter(|import| !import.is_static && import.path.rsplit('.').next() == Some(written.as_str())).collect();
    (imports.len() == 1).then(|| imports[0].path.clone())
}
fn scalar_id(file: &JavaFile, ty: &TypeRef, kotlin: bool) -> Option<String> {
    if !ty.args.is_empty() || ty.text.contains(['[', ']', '<', '>']) {
        return None;
    }
    let written = ty.text.trim();
    let written = if kotlin { written.strip_suffix('?').unwrap_or(written) } else { written };
    let java_primitive = match written {
        "byte" => Some("Byte"),
        "short" => Some("Short"),
        "int" => Some("Integer"),
        "long" => Some("Long"),
        "float" => Some("Float"),
        "double" => Some("Double"),
        "boolean" => Some("Boolean"),
        "char" => Some("Character"),
        _ => None,
    };
    if !kotlin && let Some(name) = java_primitive {
        return Some(format!("java.lang.{name}"));
    }
    let simple = written.rsplit('.').next()?;
    let builtin = ["Byte", "Short", "Integer", "Int", "Long", "Float", "Double", "Boolean", "Character", "Char", "String"];
    let namespace = if written.contains('.') {
        written.to_owned()
    } else {
        if file.types.iter().any(|ty| ty.name == simple) || file.imports.iter().any(|import| import.path.ends_with(".*")) {
            return None;
        }
        let imports: Vec<_> = file.imports.iter().filter(|import| import.path.rsplit('.').next() == Some(simple)).collect();
        if imports.len() > 1 {
            return None;
        }
        if let Some(import) = imports.first() {
            import.path.clone()
        } else if builtin.contains(&simple) && !(kotlin && matches!(simple, "Integer" | "Character")) {
            format!("{}.{simple}", if kotlin { "kotlin" } else { "java.lang" })
        } else {
            return None;
        }
    };
    if namespace == "java.util.UUID" || namespace == "java.math.BigInteger" {
        return Some(namespace);
    }
    let java_name = namespace.strip_prefix("java.lang.");
    let kotlin_name = namespace.strip_prefix("kotlin.");
    let canonical = java_name.or(kotlin_name)?;
    if !builtin.contains(&canonical)
        || (java_name.is_some() && matches!(canonical, "Int" | "Char"))
        || (kotlin_name.is_some() && matches!(canonical, "Integer" | "Character"))
    {
        return None;
    }
    if kotlin {
        Some(format!(
            "kotlin.{}",
            match canonical {
                "Integer" => "Int",
                "Character" => "Char",
                other => other,
            }
        ))
    } else if kotlin_name.is_none() && !matches!(canonical, "Int" | "Char") {
        Some(namespace)
    } else {
        None
    }
}
fn entity(doc: &Document, selection: Range) -> Option<Entity> {
    if doc.text.len() > MAX_TEXT || !matches!(doc.language.as_str(), "java" | "kotlin") || !balanced(&doc.text) {
        return None;
    }
    let kotlin = is_kotlin(&doc.path);
    if kotlin != (doc.language == "kotlin") || (!kotlin && !doc.path.ends_with(".java")) {
        return None;
    }
    let file = parse_source(&doc.path, &doc.text);
    if file.package.is_empty()
        || file.package.len() > 512
        || !file.package.split('.').all(identifier)
        || file.types.iter().any(|ty| ty.outer.is_none() && matches!(ty.name.as_str(), "org" | "java" | "kotlin"))
    {
        return None;
    }
    // Aliased imports and wildcard annotation identities need semantic resolution.
    if kotlin && java::tokens(&doc.text).windows(2).any(|tokens| tokens[0].s == "as" && tokens[1].is_ident()) {
        return None;
    }
    let at = offset(&doc.text, selection.start)?;
    let candidates: Vec<_> = file
        .types
        .iter()
        .filter(|ty| ty.outer.is_none() && matches!(ty.kind.as_str(), "class" | "record") && ty.start <= at && at <= ty.end)
        .collect();
    let [ty] = candidates.as_slice() else {
        return None;
    };
    if !identifier(&ty.name) || !ty.extends.is_empty() || file.types.iter().filter(|other| other.name == ty.name).count() != 1 {
        return None;
    }
    let header = &doc.text[ty.at + ty.name.len()..ty.end];
    if java::tokens(header).iter().take_while(|token| !matches!(token.s, "{" | "(")).any(|token| token.s == "<") {
        return None;
    }
    let markers: Vec<_> = ty
        .annotations
        .iter()
        .filter_map(|annotation| annotation_namespace(&file, &doc.text, annotation))
        .filter(|name| ENTITIES.contains(&name.as_str()))
        .collect();
    let [marker] = markers.as_slice() else {
        return None;
    };
    if ty.annotations.iter().any(|annotation| matches!(annotation.name.as_str(), "IdClass")) {
        return None;
    }
    let expected = if marker == "jakarta.persistence.Entity" {
        "jakarta.persistence.Id"
    } else if marker == "javax.persistence.Entity" {
        "javax.persistence.Id"
    } else {
        "org.springframework.data.annotation.Id"
    };
    let mut ids = Vec::new();
    for member in &ty.members {
        if member.annotations.iter().any(|annotation| annotation.name == "EmbeddedId") {
            return None;
        }
        if member.annotations.iter().filter_map(|a| annotation_namespace(&file, &doc.text, a)).any(|name| name == expected) {
            if member.modifiers.iter().any(|modifier| matches!(modifier.as_str(), "static" | "transient"))
                || member.has(&["Transient"])
                || (member.kind == java::MemberKind::Method
                    && (!member.params.is_empty()
                        || !member
                            .name
                            .strip_prefix("get")
                            .or_else(|| member.name.strip_prefix("is"))
                            .is_some_and(|name| name.starts_with(char::is_uppercase))))
            {
                return None;
            }
            ids.push(scalar_id(&file, member.ty.as_ref()?, kotlin)?);
        }
    }
    for component in &ty.components {
        if component.annotations.iter().any(|annotation| annotation.name == "EmbeddedId") {
            return None;
        }
        if component.annotations.iter().filter_map(|a| annotation_namespace(&file, &doc.text, a)).any(|name| name == expected) {
            ids.push(scalar_id(&file, &component.ty, kotlin)?);
        }
    }
    let [id] = ids.as_slice() else {
        return None;
    };
    Some(Entity { name: ty.name.clone(), at: ty.at, package: file.package, id: id.clone() })
}
fn folder(root: &str, doc: &Document, entity: &Entity) -> Result<String, Error> {
    let source = path(&doc.path).ok_or("Source path is not an unambiguous absolute path.")?;
    if !inside(root, &source) {
        return Err(Error::Stale);
    }
    let parent = source.rsplit_once('/').ok_or(Error::Stale)?.0;
    if !parent.ends_with(&format!("/{}", entity.package.replace('.', "/"))) {
        return Err("The source folder does not match its declared package; choose a conventional package folder first.".into());
    }
    Ok(parent.into())
}
fn collision(project: &openidea_plugin::Project, root: &str, directory: &str, name: &str) -> Result<(), Error> {
    for extension in ["java", "kt"] {
        let target = format!("{directory}/{name}.{extension}");
        match project.read_file(&target) {
            Ok(_) => return Err("A Java/Kotlin file with this repository name already exists; it will not be overwritten.".into()),
            Err(Error::NotFound(_)) => {}
            Err(error) => return Err(error),
        }
    }
    let relative = directory.strip_prefix(&format!("{}/", root.trim_end_matches('/'))).ok_or(Error::Stale)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    for extension in ["java", "kt"] {
        let files = project.find_files(&format!("{relative}/*.{extension}"), 65)?;
        if files.len() >= 65 {
            return Err("Package folder exceeds the bounded collision check; choose repository creation manually.".into());
        }
        for file in files {
            if Instant::now() >= deadline {
                return Err("Package collision check timed out; no files were changed.".into());
            }
            let text = project.read_file(&file)?;
            if text.len() > MAX_TEXT {
                return Err("A package source exceeds the bounded collision check; no files were changed.".into());
            }
            if parse_source(&file, &text)
                .types
                .iter()
                .any(|ty| ty.outer.is_none() && (ty.name == name || matches!(ty.name.as_str(), "org" | "java" | "kotlin")))
            {
                return Err("A top-level type with this repository name already exists in the target folder.".into());
            }
        }
    }
    Ok(())
}
impl FileActions for DataTools {
    fn file_actions(&mut self, doc: &Document, selection: Range) -> Vec<FileAction> {
        let Some(entity) = entity(doc, selection) else {
            return Vec::new();
        };
        let Some(root) = spring_text::index::project_of(&doc.path).and_then(|project| project.root().ok()).and_then(|root| path(&root))
        else {
            return Vec::new();
        };
        if folder(&root, doc, &entity).is_err() {
            return Vec::new();
        }
        let Some(next) = self.generation.next.checked_add(1) else {
            return Vec::new();
        };
        self.generation.next = next;
        let token = format!("repository:{next}:{}:{}:{}", doc.version, entity.at, entity.name);
        if self.generation.pending.len() >= 4 {
            self.generation.pending.clear();
        }
        self.generation.pending.insert(token.clone(), Pending { root, doc: doc.clone(), entity: entity.clone() });
        vec![FileAction {
            title: format!("Generate CrudRepository for {}…", entity.name),
            kind: Some("refactor.rewrite".into()),
            data: Some(token),
            ..FileAction::default()
        }]
    }
    fn resolve_file_action(&mut self, doc: &Document, action: FileAction) -> Result<FileAction, Error> {
        let pending = self.generation.pending.get(action.data.as_deref().ok_or(Error::Stale)?).cloned().ok_or(Error::Stale)?;
        if doc.path != pending.doc.path
            || doc.version != pending.doc.version
            || doc.text != pending.doc.text
            || doc.language != pending.doc.language
        {
            return Err(Error::Stale);
        }
        let project = spring_text::index::project_of(&doc.path).ok_or(Error::Stale)?;
        if path(&project.root()?).as_deref() != Some(pending.root.as_str()) {
            return Err(Error::Stale);
        }
        if project.read_file(&doc.path)? != doc.text {
            return Err(Error::Stale);
        }
        let directory = folder(&pending.root, doc, &pending.entity)?;
        let Some(name) =
            host::input("Repository interface name (same package and folder)", Some(&format!("{}Repository", pending.entity.name)))?
        else {
            return Err(Error::Cancelled);
        };
        let name = name.trim();
        if !identifier(name)
            || matches!(name, "org" | "java" | "kotlin")
            || name == pending.entity.name
            || parse_source(&doc.path, &doc.text).types.iter().any(|ty| ty.name == name)
        {
            return Err("Choose a new Java/Kotlin identifier without path separators or keywords.".into());
        }
        collision(&project, &pending.root, &directory, name)?;
        let kotlin = is_kotlin(&doc.path);
        let target = format!("{directory}/{name}.{}", if kotlin { "kt" } else { "java" });
        if path(&target).is_none() {
            return Err("The target path exceeds the supported path bound.".into());
        }
        let text = if kotlin {
            format!(
                "package {}\n\ninterface {name} : org.springframework.data.repository.CrudRepository<{}, {}>\n",
                pending.entity.package, pending.entity.name, pending.entity.id
            )
        } else {
            format!(
                "package {};\n\npublic interface {name} extends org.springframework.data.repository.CrudRepository<{}, {}> {{\n}}\n",
                pending.entity.package, pending.entity.name, pending.entity.id
            )
        };
        if !host::confirm(
            "Create Spring Data repository?",
            &format!(
                "Create only: {target}\nPackage: {}\nEntity: {}.{}\nIdentifier: {}\nSource version: {}\n\n{text}\nNo existing file is overwritten. The host applies this as one undoable workspace edit.",
                pending.entity.package, pending.entity.package, pending.entity.name, pending.entity.id, doc.version
            ),
        )? {
            return Err(Error::Cancelled);
        }
        if spring_text::index::project_of(&doc.path).and_then(|project| project.root().ok()).and_then(|root| path(&root)).as_deref()
            != Some(pending.root.as_str())
            || project.read_file(&doc.path)? != doc.text
        {
            return Err(Error::Stale);
        }
        collision(&project, &pending.root, &directory, name)?;
        Ok(FileAction {
            title: format!("Create {name}"),
            kind: Some("refactor.rewrite".into()),
            operations: vec![FileOperation::Create { path: target.clone(), text, overwrite: false }],
            open: Some(target),
            ..FileAction::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, text::position};
    const SOURCE: &str = "package com.acme;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity public class User { @Id private long id; }\n";
    const SOURCE_PATH: &str = "/project/src/main/java/com/acme/User.java";
    fn doc(text: &str) -> Document {
        Document { path: SOURCE_PATH.into(), language: "java".into(), text: text.into(), version: 7 }
    }
    fn selection(doc: &Document) -> Range {
        let at = position(&doc.text, doc.text.find("User").unwrap());
        Range::new(at, at)
    }
    fn setup(text: &str) -> (MockHost, DataTools, Document) {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("src/main/java/com/acme/User.java", text);
        (host, DataTools::default(), doc(text))
    }
    fn action(plugin: &mut DataTools, doc: &Document) -> FileAction {
        let actions = plugin.file_actions(doc, selection(doc));
        assert_eq!(actions.len(), 1, "{}", doc.text);
        assert!(actions[0].operations.is_empty() && actions[0].edits.is_empty());
        actions.into_iter().next().unwrap()
    }
    fn approve(host: &MockHost, name: &str) {
        host.answer_next_prompt(Some(&[name]));
        host.answer_next_prompt(Some(&[]));
    }
    fn created(action: &FileAction) -> (&str, &str) {
        assert_eq!(action.operations.len(), 1);
        assert!(action.edits.is_empty());
        assert!(action.data.is_none());
        let FileOperation::Create { path, text, overwrite } = &action.operations[0] else { panic!("create only") };
        assert!(!overwrite);
        assert_eq!(action.open.as_deref(), Some(path.as_str()));
        (path, text)
    }

    #[test]
    fn java_generation_is_one_non_overwriting_host_operation_without_external_writes() {
        let (host, mut plugin, doc) = setup(SOURCE);
        let pending = action(&mut plugin, &doc);
        approve(&host, "UserRepository");
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let (target, text) = created(&resolved);
        assert_eq!(target, "/project/src/main/java/com/acme/UserRepository.java");
        assert_eq!(
            text,
            "package com.acme;\n\npublic interface UserRepository extends org.springframework.data.repository.CrudRepository<User, java.lang.Long> {\n}\n"
        );
        let project = spring_text::index::project_of(SOURCE_PATH).unwrap();
        assert_eq!(project.read_file(SOURCE_PATH).unwrap(), SOURCE);
        assert!(matches!(project.read_file(target), Err(Error::NotFound(_))), "resolution must not write the file");
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }

    #[test]
    fn kotlin_nullable_scalar_id_uses_constructor_annotation_facts() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        let text = "package com.acme\nimport org.springframework.data.mongodb.core.mapping.Document\nimport org.springframework.data.annotation.Id\n@Document data class User(@field:Id val id: Long?)\n";
        let doc =
            Document { path: "/project/src/main/kotlin/com/acme/User.kt".into(), language: "kotlin".into(), text: text.into(), version: 2 };
        host.add_file("src/main/kotlin/com/acme/User.kt", text);
        let mut plugin = DataTools::default();
        let pending = action(&mut plugin, &doc);
        approve(&host, "UserRepository");
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        assert_eq!(
            created(&resolved).1,
            "package com.acme\n\ninterface UserRepository : org.springframework.data.repository.CrudRepository<User, kotlin.Long>\n"
        );
    }

    #[test]
    fn fully_qualified_annotations_record_components_and_getters_preserve_id_namespace() {
        for text in [
            "package com.acme; @jakarta.persistence.Entity public class User { @jakarta.persistence.Id private java.util.UUID id; }",
            "package com.acme; @javax.persistence.Entity public record User(@javax.persistence.Id java.util.UUID id) {}",
            "package com.acme; @jakarta.persistence.Entity public class User { @jakarta.persistence.Id public java.util.UUID getId() { return null; } }",
        ] {
            let (_host, mut plugin, doc) = setup(text);
            let pending = action(&mut plugin, &doc);
            let snapshot = &plugin.generation.pending[pending.data.as_deref().unwrap()];
            assert_eq!(snapshot.entity.id, "java.util.UUID");
        }
    }

    #[test]
    fn namespace_ambiguity_composite_inherited_and_unknown_ids_offer_no_generation() {
        for text in [
            "package com.acme; @Entity class User { @Id long id; }",
            "package com.acme; import other.Entity; import other.Id; @Entity class User { @Id long id; }",
            "package com.acme; import jakarta.persistence.*; @Entity class User { @Id long id; }",
            "package com.acme; @jakarta.persistence.Entity class User { @javax.persistence.Id long id; }",
            "package com.acme; @jakarta.persistence.Entity class User { @jakarta.persistence.Id Other id; }",
            "package com.acme; @jakarta.persistence.Entity class User { @jakarta.persistence.Id java.util.List<Long> id; }",
            "package com.acme; @jakarta.persistence.Entity class User { @jakarta.persistence.Id long id; @jakarta.persistence.Id long second; }",
            "package com.acme; @jakarta.persistence.Entity @jakarta.persistence.IdClass(Key.class) class User { @jakarta.persistence.Id long id; }",
            "package com.acme; @jakarta.persistence.Entity class User { @jakarta.persistence.EmbeddedId Key id; }",
            "package com.acme; @jakarta.persistence.Entity class User extends Base { @jakarta.persistence.Id long id; }",
            "package com.acme; @jakarta.persistence.Entity class User<T> { @jakarta.persistence.Id long id; }",
            "package com.acme; class Outer { @jakarta.persistence.Entity class User { @jakarta.persistence.Id long id; } }",
            "package com.acme; @jakarta.persistence.Entity class User { @jakarta.persistence.Id static long id; }",
            "package com.acme; @jakarta.persistence.Entity class User { @jakarta.persistence.Id transient long id; }",
            "package com.acme; @jakarta.persistence.Entity class User { @jakarta.persistence.Id long calculate() { return 1; } }",
            "package com.acme; @jakarta.persistence.Entity class User { @jakarta.persistence.Id java.lang.Int id; }",
            "package com.acme; @jakarta.persistence.Entity class User { @jakarta.persistence.Id long id;",
        ] {
            let (_host, mut plugin, doc) = setup(text);
            assert!(plugin.file_actions(&doc, selection(&doc)).is_empty(), "{text}");
        }
    }

    #[test]
    fn package_path_language_root_and_text_bounds_are_conservative() {
        let (host, mut plugin, doc) = setup(SOURCE);
        for changed in [
            Document { text: SOURCE.replace("com.acme", "other.package"), ..doc.clone() },
            Document { path: "/project/src/User.java".into(), ..doc.clone() },
            Document { path: "/project/src/main/java/com/acme/../User.java".into(), ..doc.clone() },
            Document { language: "kotlin".into(), ..doc.clone() },
            Document { text: format!("{SOURCE}{}", " ".repeat(MAX_TEXT)), ..doc.clone() },
        ] {
            assert!(plugin.file_actions(&changed, selection(&changed)).is_empty());
        }
        host.set_project_root(None);
        assert!(plugin.file_actions(&doc, selection(&doc)).is_empty());
    }

    #[test]
    fn colliding_java_kotlin_and_sibling_declared_type_are_never_overwritten() {
        for (path, text) in [
            ("src/main/java/com/acme/UserRepository.java", "existing dirty or saved file"),
            ("src/main/java/com/acme/UserRepository.kt", "interface UserRepository"),
            ("src/main/java/com/acme/Other.java", "package com.acme; interface UserRepository {}"),
        ] {
            let (host, mut plugin, doc) = setup(SOURCE);
            let pending = action(&mut plugin, &doc);
            host.add_file(path, text);
            approve(&host, "UserRepository");
            assert!(plugin.resolve_file_action(&doc, pending).is_err());
            assert_eq!(spring_text::index::project_of(SOURCE_PATH).unwrap().read_file(path).unwrap(), text);
        }
    }

    #[test]
    fn invalid_repository_names_cannot_escape_package_or_inject_source() {
        for name in [
            "../UserRepository",
            "other.Repo",
            "/tmp/Repo",
            "class",
            "_",
            "User",
            "A; interface Evil {}",
            "A\nB",
            "Répo",
            "org",
            "java",
            "kotlin",
        ] {
            let (host, mut plugin, doc) = setup(SOURCE);
            let pending = action(&mut plugin, &doc);
            approve(&host, name);
            assert!(plugin.resolve_file_action(&doc, pending).is_err(), "{name}");
            assert!(host.process_runs().is_empty());
        }
    }

    #[test]
    fn dismissed_name_or_concrete_confirmation_returns_cancelled_without_writes() {
        let (host, mut plugin, doc) = setup(SOURCE);
        let pending = action(&mut plugin, &doc);
        assert!(matches!(plugin.resolve_file_action(&doc, pending.clone()), Err(Error::Cancelled)));
        host.answer_next_prompt(Some(&["UserRepository"]));
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)));
        assert!(spring_text::index::project_of(SOURCE_PATH).unwrap().read_file("src/main/java/com/acme/UserRepository.java").is_err());
    }

    #[test]
    fn changed_version_text_path_or_project_and_expired_actions_are_stale() {
        let (host, mut plugin, doc) = setup(SOURCE);
        let pending = action(&mut plugin, &doc);
        for changed in [
            Document { version: 8, ..doc.clone() },
            Document { text: format!("{SOURCE}//changed"), ..doc.clone() },
            Document { path: "/project/Other.java".into(), ..doc.clone() },
        ] {
            assert!(matches!(plugin.resolve_file_action(&changed, pending.clone()), Err(Error::Stale)));
        }
        let mut fresh = DataTools::default();
        assert!(matches!(fresh.resolve_file_action(&doc, pending.clone()), Err(Error::Stale)));
        host.set_project_root(Some("/other"));
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Stale)));
    }

    #[test]
    fn current_dirty_source_is_preserved_and_stale_editor_text_rejects_resolution() {
        let (host, mut plugin, doc) = setup(SOURCE);
        let dirty = format!("{SOURCE}//unsaved 🦀\n");
        let doc = Document { text: dirty.clone(), ..doc };
        host.open_editor(SOURCE_PATH, "java", &dirty, &[]);
        let pending = action(&mut plugin, &doc);
        approve(&host, "UserRepository");
        plugin.resolve_file_action(&doc, pending.clone()).unwrap();
        assert_eq!(host.editor_text(), dirty);
        host.type_in_editor("//newer edit");
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Stale)));
        assert!(host.editor_text().ends_with("//newer edit"));
    }

    #[test]
    fn bounded_package_collision_check_refuses_incomplete_discovery() {
        let (host, mut plugin, doc) = setup(SOURCE);
        let pending = action(&mut plugin, &doc);
        for index in 0..65 {
            host.add_file(&format!("src/main/java/com/acme/C{index}.java"), &format!("package com.acme; class C{index} {{}}"));
        }
        approve(&host, "UserRepository");
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
    }

    #[test]
    fn utf16_selection_and_windows_paths_preserve_project_package_identity() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.set_project_root(Some("C:/project"));
        let text = SOURCE.replace("@Entity public", "/* 🦀 */ @Entity public");
        let doc = Document { path: "C:/project/src/main/java/com/acme/User.java".into(), text: text.clone(), ..doc(&text) };
        host.add_file("src/main/java/com/acme/User.java", &text);
        let mut plugin = DataTools::default();
        let pending = action(&mut plugin, &doc);
        approve(&host, "UserRepository");
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        assert_eq!(created(&resolved).0, "C:/project/src/main/java/com/acme/UserRepository.java");
    }

    #[test]
    fn kotlin_unknown_implicit_types_aliases_and_parameter_id_sites_are_not_guessed() {
        for declaration in [
            "@field:Id val id: Integer",
            "@param:Id val id: Long",
            "@field:Id val id: java.lang.Int",
            "@field:Id val id: kotlin.Integer",
            "@field:Id val id: Long??",
        ] {
            let text = format!(
                "package com.acme\nimport org.springframework.data.mongodb.core.mapping.Document\nimport org.springframework.data.annotation.Id\n@Document data class User({declaration})\n"
            );
            let doc = Document { path: "/project/src/main/kotlin/com/acme/User.kt".into(), language: "kotlin".into(), text, version: 1 };
            assert!(entity(&doc, selection(&doc)).is_none(), "{declaration}");
        }
        let text = "package com.acme\nimport org.springframework.data.mongodb.core.mapping.Document\nimport org.springframework.data.annotation.Id as Identifier\n@Document data class User(@Identifier val id: Long)\n";
        let doc =
            Document { path: "/project/src/main/kotlin/com/acme/User.kt".into(), language: "kotlin".into(), text: text.into(), version: 1 };
        assert!(entity(&doc, selection(&doc)).is_none());
    }
}
