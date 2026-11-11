//! Explicit selection of a conservative current-source composite key.
//! Only the invoking document is edited. A dependent key is reread, never mutated.
use super::*;
use std::collections::HashSet;

pub(super) const TITLE: &str = "Add embedded identifier with preview…";

#[derive(Clone, Debug, PartialEq)]
struct Candidate {
    path: String,
    qualified: String,
    text: String,
    fields: Vec<String>,
}

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn scalar(ty: &str, class: &Class) -> bool {
    let canonical = match ty {
        "long" | "int" | "short" => return true,
        "Long" | "Integer" | "Short" | "String" => format!("java.lang.{ty}"),
        "UUID" => "java.util.UUID".into(),
        "BigInteger" => "java.math.BigInteger".into(),
        "java.lang.Long"
        | "java.lang.Integer"
        | "java.lang.Short"
        | "java.lang.String"
        | "java.util.UUID"
        | "java.math.BigInteger" => return true,
        _ => return false,
    };
    let explicit: Vec<_> = class
        .imports
        .iter()
        .filter(|i| i.rsplit('.').next() == Some(ty))
        .collect();
    if explicit.iter().any(|i| **i != canonical) {
        return false;
    }
    // A bare java.lang spelling can be shadowed by a same-package/library type.
    // Without compiler resolution require a canonical explicit import instead.
    explicit.len() == 1
}

fn method(signature: &str, result: &str, name: &str, parameter: Option<&str>) -> bool {
    let Some((prefix, params)) = signature.trim().split_once('(') else {
        return false;
    };
    let Some(params) = params.strip_suffix(')') else {
        return false;
    };
    if prefix.split_whitespace().collect::<Vec<_>>() != ["public", result, name] {
        return false;
    }
    match parameter {
        None => params.trim().is_empty(),
        Some(ty) => {
            let parts: Vec<_> = params.split_whitespace().collect();
            parts.len() == 2 && parts[0] == ty && identifier(parts[1])
        }
    }
}

fn key(doc: &Document, ns: &str) -> Option<Candidate> {
    if doc.language != "java" || doc.text.len() > MAX_TEXT {
        return None;
    }
    let class = current_class(doc)?;
    let masked = mask(&doc.text);
    crate::checks::owned_class_start(&doc.text, &masked, &class)?;
    if class.kind != Kind::Embeddable
        || ["java", "jakarta", "javax", "kotlin"].contains(&class.name.as_str())
        || class.has_id
        || annotations(&masked)
            .iter()
            .filter(|a| a.name == "Embeddable")
            .count()
            != 1
        || class.name != file_stem(&doc.path)
        || namespace(&doc.text, &class) != Some(ns)
        || !crate::attribute_info::balanced(&masked)
        || class
            .imports
            .iter()
            .any(|i| i.ends_with(".*") && i != &format!("{ns}.*"))
        || class.imports.iter().any(|i| {
            ["java", "jakarta", "javax", "kotlin"].contains(&i.rsplit('.').next().unwrap_or(""))
        })
    {
        return None;
    }
    let at = offset(&doc.text, class.pos)?;
    let open = at + masked[at..].find('{')?;
    let end = class.body_end?;
    let header = compact(&masked[at..open]);
    let serializable = header == format!("{}implementsjava.io.Serializable", class.name)
        || (header == format!("{}implementsSerializable", class.name)
            && class
                .imports
                .iter()
                .filter(|i| i.rsplit('.').next() == Some("Serializable"))
                .collect::<Vec<_>>()
                == [&"java.io.Serializable".to_owned()]);
    if !serializable || !masked[end + 1..].trim().is_empty() {
        return None;
    }
    let class_at = words(&masked[..at], "class").last()?;
    let prefix = crate::model::without_annotations(&masked, 0, class_at);
    // The declaration immediately before `class` must be public, not a nested/private type.
    if prefix.rsplit(';').next()?.trim() != "public" {
        return None;
    }
    for annotation in annotations(&masked) {
        let written = &masked[annotation.start + 1..annotation.name_end];
        let canonical = format!("{ns}.{}", annotation.name);
        let owned = written == canonical
            || (written == annotation.name
                && class
                    .imports
                    .iter()
                    .filter(|i| i.rsplit('.').next() == Some(annotation.name))
                    .collect::<Vec<_>>()
                    == [&canonical]);
        let allowed = match annotation.name {
            "Embeddable" => annotation.start < class_at && owned,
            "Column" => annotation.start > open && annotation.start < end && owned,
            "Override" => {
                written == "java.lang.Override"
                    || (written == "Override"
                        && class
                            .imports
                            .iter()
                            .filter(|i| i.rsplit('.').next() == Some("Override"))
                            .collect::<Vec<_>>()
                            == [&"java.lang.Override".to_owned()])
            }
            _ => false,
        };
        if !allowed {
            return None;
        }
    }
    let mut fields = Vec::new();
    let mut names = HashSet::new();
    let (mut constructors, mut no_arg, mut equals, mut hash) = (0, false, false, false);
    for (start, finish) in crate::model::members(&masked, open + 1, end, false, false) {
        let clean = crate::model::without_annotations(&masked, start, finish);
        let member = clean.trim();
        if member.is_empty() {
            continue;
        }
        if let Some(field) = member.strip_suffix(';') {
            let parts: Vec<_> = field.split_whitespace().collect();
            if let [
                "private",
                "static",
                "final",
                "long",
                "serialVersionUID",
                "=",
                value,
            ] = parts.as_slice()
            {
                if value.trim_end_matches(['L', 'l']).parse::<i64>().is_err()
                    || !annotations(&masked[start..finish]).is_empty()
                {
                    return None;
                }
                continue;
            }
            let [visibility, ty, name] = parts.as_slice() else {
                return None;
            };
            if !["private", "protected", "public"].contains(visibility)
                || !scalar(ty, &class)
                || !identifier(name)
                || !names.insert((*name).to_owned())
            {
                return None;
            }
            if annotations(&masked[start..finish])
                .iter()
                .any(|a| a.name != "Column")
                || annotations(&masked[start..finish])
                    .iter()
                    .filter(|a| a.name == "Column")
                    .count()
                    > 1
            {
                return None;
            }
            fields.push((*name).to_owned());
        } else {
            let signature = member.split_once('{')?.0.trim();
            if annotations(&masked[start..finish])
                .iter()
                .any(|a| a.name != "Override")
            {
                return None;
            }
            if signature
                .split_once('(')
                .is_some_and(|(p, _)| p.split_whitespace().last() == Some(class.name.as_str()))
            {
                constructors += 1;
                no_arg |= [
                    format!("public{}()", class.name),
                    format!("protected{}()", class.name),
                ]
                .contains(&compact(signature));
            }
            equals |= method(signature, "boolean", "equals", Some("java.lang.Object"))
                || (method(signature, "boolean", "equals", Some("Object"))
                    && class
                        .imports
                        .iter()
                        .filter(|i| i.rsplit('.').next() == Some("Object"))
                        .collect::<Vec<_>>()
                        == [&"java.lang.Object".to_owned()]);
            hash |= method(signature, "int", "hashCode", None);
        }
    }
    if !(2..=16).contains(&fields.len()) || (constructors > 0 && !no_arg) || !equals || !hash {
        return None;
    }
    let package = class.package?;
    if ["java", "javax", "jakarta", "kotlin"].contains(&package.split('.').next()?) {
        return None;
    }
    Some(Candidate {
        path: doc.path.clone(),
        qualified: format!("{package}.{}", class.name),
        text: doc.text.clone(),
        fields,
    })
}

fn directories(doc: &Document, root: &str) -> Result<Vec<String>, Error> {
    let package = package(doc).ok_or(Error::Stale)?;
    let relative = doc
        .path
        .strip_prefix(&format!("{}/", root.trim_end_matches('/')))
        .ok_or(Error::Stale)?;
    let segments: Vec<_> = relative.split('/').collect();
    let roots: Vec<_> = segments
        .windows(3)
        .enumerate()
        .filter(|(_, s)| {
            s[0] == "src" && matches!(s[1], "main" | "test") && matches!(s[2], "java" | "kotlin")
        })
        .map(|(i, _)| i)
        .collect();
    let [start] = roots.as_slice() else {
        return Err("Embedded identifier selection requires an unambiguous src/main or src/test source set.".into());
    };
    if segments[*start + 3..segments.len() - 1].join(".") != package {
        return Err(Error::Stale);
    }
    let prefix = segments[..*start + 2].join("/");
    Ok(["java", "kotlin"]
        .iter()
        .map(|language| format!("{prefix}/{language}/{}", package.replace('.', "/")))
        .collect())
}

fn candidates(
    project: &Project,
    pending: &Pending,
    doc: &Document,
    ns: &str,
) -> Result<Vec<Candidate>, Error> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut found = Vec::new();
    let mut declarations: HashMap<String, usize> = HashMap::new();
    let mut seen = HashSet::new();
    for directory in directories(doc, &pending.root)? {
        for extension in ["java", "kt"] {
            let files = project.find_files(&format!("{directory}/*.{extension}"), 65)?;
            if files.len() >= 65 {
                return Err("This source package exceeds bounded embedded key discovery.".into());
            }
            for file in files {
                if !seen.insert(file.clone()) {
                    continue;
                }
                if Instant::now() >= deadline {
                    return Err("Embedded key discovery timed out.".into());
                }
                if path(&file).as_deref() != Some(file.as_str())
                    || !inside(&pending.root, &file)
                    || file.rsplit_once('/').map(|(d, _)| d)
                        != Some(
                            format!("{}/{directory}", pending.root.trim_end_matches('/')).as_str(),
                        )
                {
                    return Err(Error::Stale);
                }
                let owner = host::project_of(&file).ok_or(Error::Stale)?;
                if owner.root()? != pending.root {
                    continue;
                }
                let text = project.read_file(&file)?;
                if text.len() > MAX_TEXT {
                    return Err(
                        "A source package file exceeds embedded key discovery bounds.".into(),
                    );
                }
                let candidate_doc = Document {
                    path: file,
                    language: if extension == "java" {
                        "java"
                    } else {
                        "kotlin"
                    }
                    .into(),
                    text,
                    version: 0,
                };
                if package(&candidate_doc) != package(doc) {
                    continue;
                }
                let masked = mask(&candidate_doc.text);
                for kind in [
                    "class",
                    "interface",
                    "record",
                    "enum",
                    "object",
                    "typealias",
                ] {
                    for at in words(&masked, kind) {
                        let rest = masked[at + kind.len()..].trim_start();
                        let name: String = rest
                            .chars()
                            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                            .collect();
                        if identifier(&name) {
                            *declarations.entry(name).or_default() += 1;
                        }
                    }
                }
                if let Some(candidate) = key(&candidate_doc, ns) {
                    found.push(candidate);
                }
            }
        }
    }
    found.retain(|c| declarations.get(c.qualified.rsplit('.').next().unwrap_or("")) == Some(&1));
    found.sort_by(|a, b| a.qualified.cmp(&b.qualified));
    Ok(found)
}

fn revalidate(
    pending: &Pending,
    doc: &Document,
    ns: &str,
    candidate: &Candidate,
) -> Result<(), Error> {
    let project = current(pending, doc)?;
    if !candidates(&project, pending, doc, ns)?.contains(candidate) {
        return Err(Error::Stale);
    }
    Ok(())
}

fn insertion(
    doc: &Document,
    class: &Class,
    ns: &str,
    candidate: &Candidate,
    name: &str,
) -> Result<TextEdit, Error> {
    if !persistence_core::generation::identifier(name)
        || ["Class", "java", "javax", "jakarta", "kotlin"].contains(&name)
    {
        return Err("Choose a valid embedded identifier attribute name.".into());
    }
    let root = candidate.qualified.split('.').next().ok_or(Error::Stale)?;
    if class.name == root
        || class
            .imports
            .iter()
            .any(|i| i.rsplit('.').next() == Some(root))
    {
        return Err("The embedded key's package prefix conflicts with a source type name.".into());
    }
    let masked = mask(&doc.text);
    let at = offset(&doc.text, class.pos).ok_or(Error::Stale)?;
    let end = class.body_end.ok_or(Error::Stale)?;
    let suffix = capitalize(name);
    if class
        .attributes
        .iter()
        .any(|attribute| capitalize(&attribute.name) == suffix)
        || [name, &format!("get{suffix}"), &format!("set{suffix}")]
            .iter()
            .any(|name| has_word(&masked[at..end], name))
    {
        return Err("The embedded identifier or its accessors already occur in this class.".into());
    }
    let mut text = if class.kotlin {
        format!(
            "\n    @{ns}.EmbeddedId\n    var {name}: {}? = null\n",
            candidate.qualified
        )
    } else {
        format!(
            "\n    @{ns}.EmbeddedId\n    private {} {name};\n\n    public {} get{suffix}() {{\n        return {name};\n    }}\n\n    public void set{suffix}({} {name}) {{\n        this.{name} = {name};\n    }}\n",
            candidate.qualified, candidate.qualified, candidate.qualified
        )
    };
    if doc.text.contains("\r\n") {
        text = text.replace('\n', "\r\n");
    }
    Ok(TextEdit {
        range: Range::new(position(&doc.text, end), position(&doc.text, end)),
        text,
    })
}

pub(super) fn resolve(pending: &Pending, doc: &Document) -> Result<FileAction, Error> {
    let project = current(pending, doc)?;
    let (class, namespace) = identifier_action::source(doc).ok_or(Error::Stale)?;
    let ns = namespace.package();
    let available = candidates(&project, pending, doc, ns)?;
    if available.is_empty() {
        return Err("No eligible Java composite key was found in this source package. This source subset needs a public same-namespace embeddable, 2–16 scalar fields, Serializable, a no-argument constructor and declared equals/hashCode.".into());
    }
    let selected = host::pick(
        "Embedded identifier class",
        &available
            .iter()
            .enumerate()
            .map(|(i, c)| {
                PickItem::new(
                    &i.to_string(),
                    &format!("{} — {}", c.qualified, c.fields.join(", ")),
                )
            })
            .collect::<Vec<_>>(),
    )?
    .ok_or(Error::Cancelled)?;
    let candidate = available
        .get(selected.parse::<usize>().map_err(|_| Error::Stale)?)
        .ok_or(Error::Stale)?;
    revalidate(pending, doc, ns, candidate)?;
    let name =
        host::input("Embedded identifier attribute name", Some("id"))?.ok_or(Error::Cancelled)?;
    let edit = insertion(doc, &class, ns, candidate, name.trim())?;
    let after = patched(&doc.text, std::slice::from_ref(&edit))?;
    if after.len() > MAX_TEXT {
        return Err("Generated source exceeds the preview bound.".into());
    }
    if !ui_kit::prompts::confirm_exact(
        "Add embedded identifier?",
        &format!(
            "Target: {}\nKey source: {}\n\n{after}\nReview the key's equals/hashCode for consistency with database equality. Serializable is required by this conservative older-API-compatible subset. Effective mappings, classpath resolution and equality behavior are not evaluated. The key is reread before returning this single-document edit; the host cannot atomically guard a separate key file during final application.",
            doc.path, candidate.path
        ),
    )? {
        return Err(Error::Cancelled);
    }
    revalidate(pending, doc, ns, candidate)?;
    Ok(FileAction {
        title: format!("Add embedded identifier {}", name.trim()),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), vec![edit])],
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const OWNER: &str = "package shop;\n@jakarta.persistence.Entity\npublic class Invoice {\n    private java.lang.String title;\n}\n";
    const KEY: &str = "package shop;\n@jakarta.persistence.Embeddable\npublic class OrderKey implements java.io.Serializable {\n    private static final long serialVersionUID = 1L;\n    private java.lang.Long tenant;\n    private java.lang.String number;\n    public OrderKey() {}\n    @java.lang.Override\n    public boolean equals(java.lang.Object other) {\n        if (this == other) return true;\n        if (!(other instanceof OrderKey)) return false;\n        OrderKey that = (OrderKey) other;\n        return java.util.Objects.equals(tenant, that.tenant) && java.util.Objects.equals(number, that.number);\n    }\n    @java.lang.Override\n    public int hashCode() { return java.util.Objects.hash(tenant, number); }\n}\n";
    fn setup(language: &str, ns: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let path = format!(
            "/project/src/main/{language}/shop/Invoice.{}",
            if language == "java" { "java" } else { "kt" }
        );
        let text = if language == "java" {
            OWNER.replace("jakarta.persistence", ns)
        } else {
            format!(
                "package shop\n@{ns}.Entity\nopen class Invoice {{\n    var title: kotlin.String? = null\n}}\n"
            )
        };
        host.open_editor(&path, language, &text, &[]);
        host.add_file(
            "src/main/java/shop/OrderKey.java",
            &KEY.replace("jakarta.persistence", ns),
        );
        (
            host,
            JpaAssist::default(),
            Document {
                path,
                language: language.into(),
                text,
                version: 1,
            },
        )
    }
    fn action(plugin: &mut JpaAssist, doc: &Document) -> FileAction {
        plugin
            .file_actions(doc, Range::default())
            .into_iter()
            .find(|a| a.title == TITLE)
            .unwrap()
    }
    fn pending(doc: &Document) -> Pending {
        Pending {
            root: "/project".into(),
            doc: doc.clone(),
            operation: Operation::EmbeddedIdentifier,
            selection: Range::default(),
        }
    }
    fn key_doc(text: String) -> Document {
        Document {
            path: "/project/src/main/java/shop/OrderKey.java".into(),
            language: "java".into(),
            text,
            version: 0,
        }
    }
    fn resolve(
        host: &MockHost,
        plugin: &mut JpaAssist,
        doc: &Document,
        name: &str,
    ) -> Result<FileAction, Error> {
        let action = action(plugin, doc);
        for answer in [vec!["0"], vec![name], vec![]] {
            host.answer_next_prompt(Some(&answer));
        }
        plugin.resolve_file_action(doc, action)
    }
    #[test]
    fn java_and_kotlin_owners_select_canonical_java_key_without_editing_it() {
        for language in ["java", "kotlin"] {
            for ns in ["jakarta.persistence", "javax.persistence"] {
                let (host, mut plugin, doc) = setup(language, ns);
                let result = resolve(&host, &mut plugin, &doc, "key").unwrap();
                assert!(result.operations.is_empty());
                assert_eq!(result.edits.len(), 1);
                assert_eq!(result.edits[0].0, doc.path);
                let after = patched(&doc.text, &result.edits[0].1).unwrap();
                assert!(after.contains(&format!("@{ns}.EmbeddedId")), "{after}");
                assert!(
                    after.contains(if language == "java" {
                        "private shop.OrderKey key;"
                    } else {
                        "var key: shop.OrderKey? = null"
                    }),
                    "{after}"
                );
                assert_eq!(host.editor_text(), doc.text);
                assert_eq!(
                    host.invocation()
                        .project()
                        .unwrap()
                        .read_file("src/main/java/shop/OrderKey.java")
                        .unwrap(),
                    KEY.replace("jakarta.persistence", ns)
                );
                assert!(host.process_runs().is_empty());
            }
        }
    }
    #[test]
    fn incomplete_unknown_or_nonportable_key_declarations_are_withheld() {
        let (_host, _, _) = setup("java", "jakarta.persistence");
        assert!(key(&key_doc(KEY.into()), "jakarta.persistence").is_some());
        for text in [
            KEY.replace(" implements java.io.Serializable", ""),
            KEY.replace("public OrderKey()", "private OrderKey()"),
            KEY.replace("public boolean equals", "public boolean different"),
            KEY.replace("public int hashCode", "public int different"),
            KEY.replace("class OrderKey implements", "class OrderKey<T> implements"),
            KEY.replace("class OrderKey implements", "class OrderKey extends Base implements"),
            KEY.replace("java.lang.Long tenant", "Long tenant"),
            KEY.replace("java.lang.Long tenant", "foo.Long tenant"),
            KEY.replace("private java.lang.String number;", ""),
            KEY.replace("private java.lang.String number;", "@jakarta.persistence.ManyToOne private Customer number;"),
            KEY.replace("private java.lang.String number;", "@jakarta.persistence.Transient private java.lang.String number;"),
            KEY.replace("private java.lang.String number;", "private final java.lang.String number = null;"),
            KEY.replace("@jakarta.persistence.Embeddable", "@foreign.Embeddable"),
            KEY.replace("jakarta.persistence", "javax.persistence"),
            KEY.replace("@jakarta.persistence.Embeddable", "@jakarta.persistence.Embeddable\n@jakarta.persistence.Access(jakarta.persistence.AccessType.PROPERTY)"),
            KEY.replace("public boolean equals(java.lang.Object", "public boolean equals(Object"),
        ] { assert!(key(&key_doc(text.clone()), "jakarta.persistence").is_none(), "{text}"); }
        let commented = KEY.replace("@java.lang.Override\n    public int hashCode() { return java.util.Objects.hash(tenant, number); }", "// public int hashCode() { return 0; }");
        assert!(key(&key_doc(commented), "jakarta.persistence").is_none());
        let wildcard = KEY.replace(
            "@jakarta.persistence.Embeddable",
            "import jakarta.persistence.*;\n@Embeddable",
        );
        assert!(key(&key_doc(wildcard), "jakarta.persistence").is_none());
        let explicit = KEY.replace(
            "@jakarta.persistence.Embeddable",
            "import jakarta.persistence.Embeddable;\n@Embeddable",
        );
        assert!(key(&key_doc(explicit), "jakarta.persistence").is_some());
        for root in ["java", "jakarta", "javax", "kotlin"] {
            let mut doc = key_doc(KEY.replace("OrderKey", root));
            doc.path = format!("/project/src/main/java/shop/{root}.java");
            assert!(key(&doc, "jakarta.persistence").is_none());
        }
    }
    #[test]
    fn duplicates_foreign_projects_and_wrong_source_sets_do_not_supply_keys() {
        let (host, mut plugin, doc) = setup("java", "jakarta.persistence");
        host.add_file(
            "src/main/kotlin/shop/Other.kt",
            "package shop\nclass OrderKey {}\n",
        );
        assert!(resolve(&host, &mut plugin, &doc, "id").is_err());
        let (host, mut plugin, doc) = setup("kotlin", "jakarta.persistence");
        host.add_project("/project/src/main/java");
        assert!(resolve(&host, &mut plugin, &doc, "id").is_err());
        let (host, _, mut doc) = setup("java", "jakarta.persistence");
        doc.path = "/project/src/test/java/shop/Invoice.java".into();
        host.open_editor(&doc.path, "java", &doc.text, &[]);
        assert!(
            candidates(
                &host.invocation().project().unwrap(),
                &pending(&doc),
                &doc,
                "jakarta.persistence"
            )
            .unwrap()
            .is_empty()
        );
    }
    #[test]
    fn selected_key_and_owner_are_rechecked_including_unsaved_overlays() {
        let (host, _, doc) = setup("java", "jakarta.persistence");
        let project = host.invocation().project().unwrap();
        let pending = pending(&doc);
        let selected = candidates(&project, &pending, &doc, "jakarta.persistence")
            .unwrap()
            .remove(0);
        revalidate(&pending, &doc, "jakarta.persistence", &selected).unwrap();
        host.add_file(
            "src/main/java/shop/OrderKey.java",
            &KEY.replace("Long tenant", "Integer tenant"),
        );
        assert!(revalidate(&pending, &doc, "jakarta.persistence", &selected).is_err());
        host.add_file("src/main/java/shop/OrderKey.java", KEY);
        host.open_editor(&selected.path, "java", KEY, &[]);
        host.type_in_editor("// unsaved modification");
        assert!(revalidate(&pending, &doc, "jakarta.persistence", &selected).is_err());
        assert_eq!(
            project.read_file(&selected.path).unwrap(),
            format!("{KEY}// unsaved modification")
        );
    }
    #[test]
    fn name_accessor_and_package_prefix_collisions_refuse_without_writes() {
        for (text, name) in [
            (OWNER.to_owned(), "title"),
            (
                OWNER.replace(
                    "title;",
                    "title;\n public shop.OrderKey getKey() { return null; }",
                ),
                "key",
            ),
            (OWNER.to_owned(), "Class"),
            (
                OWNER.replace("public class Invoice", "public class shop"),
                "key",
            ),
            (
                OWNER.replace("package shop;", "package shop;\nimport foreign.shop;"),
                "key",
            ),
        ] {
            let (host, mut plugin, mut doc) = setup("java", "jakarta.persistence");
            doc.text = text;
            host.open_editor(&doc.path, "java", &doc.text, &[]);
            assert!(resolve(&host, &mut plugin, &doc, name).is_err());
            assert_eq!(host.editor_text(), doc.text);
        }
        for title in [TITLE, "Add JPA identifier with preview…"] {
            let (host, mut plugin, doc) = setup("kotlin", "jakarta.persistence");
            let pending = plugin
                .file_actions(&doc, Range::default())
                .into_iter()
                .find(|a| a.title == title)
                .unwrap();
            let rows = if title == TITLE {
                vec![vec!["0"], vec!["Title"], vec![]]
            } else {
                vec![
                    vec!["Title"],
                    vec!["id"],
                    vec!["long"],
                    vec!["assigned"],
                    vec![],
                ]
            };
            for row in rows {
                host.answer_next_prompt(Some(&row));
            }
            assert!(plugin.resolve_file_action(&doc, pending).is_err());
            assert_eq!(host.editor_text(), doc.text);
        }
    }
    #[test]
    fn cancellation_stale_documents_and_crlf_keep_source_unchanged_until_host_apply() {
        let (host, mut plugin, doc) = setup("java", "jakarta.persistence");
        let pending_action = action(&mut plugin, &doc);
        host.answer_next_prompt(None);
        assert!(matches!(
            plugin.resolve_file_action(&doc, pending_action),
            Err(Error::Cancelled)
        ));
        let pending_action = action(&mut plugin, &doc);
        host.type_in_editor("// changed");
        assert!(plugin.resolve_file_action(&doc, pending_action).is_err());
        let (host, mut plugin, mut doc) = setup("java", "jakarta.persistence");
        doc.text = doc.text.replace('\n', "\r\n");
        host.open_editor(&doc.path, "java", &doc.text, &[]);
        let result = resolve(&host, &mut plugin, &doc, "id").unwrap();
        let after = patched(&doc.text, &result.edits[0].1).unwrap();
        assert!(!after.replace("\r\n", "").contains('\n'));
        assert_eq!(host.editor_text(), doc.text);
    }

    #[test]
    fn discovery_bounds_and_invalid_picker_values_fail_closed() {
        let (host, mut plugin, doc) = setup("java", "jakarta.persistence");
        let pending = action(&mut plugin, &doc);
        host.answer_next_prompt(Some(&["999"]));
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        for i in 0..64 {
            host.add_file(
                &format!("src/main/java/shop/Other{i}.java"),
                &format!("package shop;\nclass Other{i} {{}}"),
            );
        }
        let pending = action(&mut plugin, &doc);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        assert_eq!(host.editor_text(), doc.text);
    }

    #[test]
    #[ignore = "requires API jar paths and KOTLIN_COMPILER_CLASSPATH / KOTLIN_STDLIB"]
    fn generated_embedded_identifier_pairs_compile_for_java_kotlin_and_both_namespaces() {
        let base =
            std::env::temp_dir().join(format!("jpa-embedded-id-compile-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        for language in ["java", "kotlin"] {
            for ns in ["jakarta.persistence", "javax.persistence"] {
                let (host, mut plugin, doc) = setup(language, ns);
                let result = resolve(&host, &mut plugin, &doc, "key").unwrap();
                let source = patched(&doc.text, &result.edits[0].1).unwrap();
                let directory = base.join(format!("{language}-{ns}"));
                std::fs::create_dir_all(&directory).unwrap();
                let jar = std::env::var(if ns == "jakarta.persistence" {
                    "JPA_JAKARTA_API"
                } else {
                    "JPA_JAVAX_API"
                })
                .expect("explicit persistence API jar");
                let key_source = directory.join("OrderKey.java");
                std::fs::write(&key_source, KEY.replace("jakarta.persistence", ns)).unwrap();
                let owner = directory.join(if language == "java" {
                    "Invoice.java"
                } else {
                    "Invoice.kt"
                });
                std::fs::write(&owner, source).unwrap();
                let mut javac = std::process::Command::new("javac");
                javac
                    .args(["--release", "17", "-proc:none", "-classpath", &jar, "-d"])
                    .arg(&directory)
                    .arg(&key_source);
                if language == "java" {
                    javac.arg(&owner);
                }
                let result = javac.output().expect("javac installed");
                assert!(
                    result.status.success(),
                    "{language} {ns}: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
                if language == "kotlin" {
                    let compiler = std::env::var("KOTLIN_COMPILER_CLASSPATH")
                        .expect("explicit Kotlin compiler classpath");
                    let stdlib =
                        std::env::var("KOTLIN_STDLIB").expect("explicit Kotlin standard library");
                    let classpath = std::env::join_paths([
                        std::path::Path::new(&jar),
                        std::path::Path::new(&stdlib),
                        &directory,
                    ])
                    .unwrap();
                    let result = std::process::Command::new("java")
                        .args([
                            "-classpath",
                            &compiler,
                            "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler",
                            "-no-stdlib",
                            "-no-reflect",
                            "-jvm-target",
                            "17",
                            "-classpath",
                        ])
                        .arg(classpath)
                        .arg("-d")
                        .arg(&directory)
                        .arg(&owner)
                        .output()
                        .expect("java installed");
                    assert!(
                        result.status.success(),
                        "{language} {ns}: {}",
                        String::from_utf8_lossy(&result.stderr)
                    );
                }
            }
        }
        std::fs::remove_dir_all(base).unwrap();
        eprintln!(
            "Compiled 4 generated entity/Java composite-key pairs: Java and Kotlin owners, Jakarta 3.2 and javax 2.2 APIs."
        );
    }
}
