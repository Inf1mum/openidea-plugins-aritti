//! Positive source identifier compatibility only. No classpath or absent-type inference.
use crate::{attribute_info, model};
use openidea_plugin::{host, providers::Document, text::offset};
fn canonical(ty: &str) -> Option<&str> {
    Some(match ty {
        "long" | "java.lang.Long" => "java.lang.Long",
        "int" | "java.lang.Integer" => "java.lang.Integer",
        "short" | "java.lang.Short" => "java.lang.Short",
        "byte" | "java.lang.Byte" => "java.lang.Byte",
        "java.lang.String" => "java.lang.String",
        "java.util.UUID" => "java.util.UUID",
        "java.math.BigInteger" => "java.math.BigInteger",
        _ => return None,
    })
}
pub(super) fn identifier(doc: &Document) -> Option<String> {
    if doc.language != "java" {
        return None;
    }
    let class = attribute_info::snapshot(doc)?;
    if class.kind != model::Kind::Entity
        || class.superclass.is_some()
        || class
            .imports
            .iter()
            .any(|name| matches!(name.rsplit('.').next(), Some("java" | "jakarta" | "javax")))
    {
        return None;
    }
    let masked = model::mask(&doc.text);
    let (at, _) = model::declaration_keyword(&masked, 0, &["class"])?;
    let open = at + masked[at..].find('{')?;
    if masked[at + 5..open].trim() != class.name {
        return None;
    }
    let annotations = model::annotations(&masked);
    if annotations
        .iter()
        .any(|a| matches!(a.name, "IdClass" | "EmbeddedId" | "Access"))
    {
        return None;
    }
    let ids: Vec<_> = class.attributes.iter().filter(|a| a.id).collect();
    let [id] = ids.as_slice() else {
        return None;
    };
    let pos = offset(&doc.text, id.pos)?;
    let (from, to) = model::members(&masked, open + 1, class.body_end?, false, false)
        .into_iter()
        .find(|(from, to)| *from <= pos && pos < *to)?;
    let declaration = model::without_annotations(&masked, from, to);
    if declaration.contains(['(', '{', ',', '='])
        || !declaration.trim_end().ends_with(';')
        || ["static", "transient"]
            .iter()
            .any(|w| model::has_word(&declaration, w))
    {
        return None;
    }
    Some(canonical(&id.ty)?.into())
}
/// A source absence note, not an assertion about the effective persistence model.
pub(super) fn missing_identifier(doc: &Document) -> Option<crate::checks::Finding> {
    if doc.language != "java" {
        return None;
    }
    let class = attribute_info::snapshot(doc)?;
    if class.kind != model::Kind::Entity || class.superclass.is_some() {
        return None;
    }
    let masked = model::mask(&doc.text);
    let annotations = model::annotations(&masked);
    if annotations
        .iter()
        .any(|a| matches!(a.name, "Id" | "EmbeddedId" | "IdClass" | "Access"))
    {
        return None;
    }
    let (at, _) = model::declaration_keyword(&masked, 0, &["class"])?;
    let open = at + masked[at..].find('{')?;
    if masked[at + 5..open].trim() != class.name {
        return None;
    }
    let from = offset(&doc.text, class.pos)?;
    let fqcn = class.package.as_deref().map(|package| format!("{package}.{}", class.name));
    let hbm_free = host::project_of(&doc.path).and_then(|p| p.find_files("**/*.hbm.xml", 1).ok()).is_some_and(|found| found.is_empty());
    let proven = hbm_free && fqcn.is_some_and(|f| !crate::orm_xml::load(&doc.path).may_declare(&f));
    if !proven {
        return Some(crate::checks::Finding::new(
            from,
            from + class.name.len(),
            openidea_plugin::Severity::Info,
            "jpa-source-identifier-absent",
            "This complete source-local entity declares no Id or EmbeddedId. Add an identifier if it is annotation-mapped; effective XML/provider mappings are unresolved.",
        ));
    }
    let mut finding = crate::checks::Finding::new(
        from,
        from + class.name.len(),
        openidea_plugin::Severity::Error,
        "jpa-source-identifier-absent",
        "This entity declares no Id or EmbeddedId, and no mapping XML in the project maps it, so it has no primary key (Jakarta Persistence 3.2, section 2.4).",
    );
    finding.fix = create_id_fix(&doc.text, &class, open);
    Some(finding)
}

fn create_id_fix(text: &str, class: &model::Class, open: usize) -> Option<(&'static str, Vec<model::Edit>)> {
    let entity = class.imports.iter().find(|i| matches!(i.as_str(), "jakarta.persistence.Entity" | "javax.persistence.Entity"))?;
    let namespace = entity.trim_end_matches(".Entity");
    if class.attributes.iter().any(|a| a.name == "id") || class.imports.iter().any(|i| i.ends_with(".Id")) {
        return None;
    }
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut edits = vec![model::Edit::insert(open + 1, &format!("{eol}    @Id{eol}    private Long id;{eol}"))];
    edits.extend(model::import_edit(text, &[&format!("{namespace}.Id")], false));
    Some(("Create id", edits))
}

pub(super) fn repository(doc: &Document) -> Option<(String, String, usize, usize)> {
    if doc.language != "java" || doc.text.len() > 128 * 1024 {
        return None;
    }
    let masked = model::mask(&doc.text);
    if masked.contains('@')
        || model::words(&masked, "interface").count() != 1
        || model::words(&masked, "class").next().is_some()
    {
        return None;
    }
    let repository = crate::queries::parse_repository(&doc.text, false)?;
    let (at, _) = model::declaration_keyword(&masked, 0, &["interface"])?;
    let prefix = masked[..at].trim();
    let prefix = prefix.strip_prefix("package ")?;
    let (package, modifiers) = prefix.split_once(';')?;
    let package = package.trim();
    if !matches!(modifiers.trim(), "" | "public")
        || doc
            .path
            .rsplit_once('/')?
            .0
            .rsplit_once("/src/main/java/")?
            .1
            != package.replace('.', "/")
    {
        return None;
    }
    let open = at + masked[at..].find('{')?;
    let close = model::closing(masked.as_bytes(), open)?;
    if !masked[close + 1..].trim().is_empty() {
        return None;
    }
    let header = masked[at + 9..open].trim();
    let parent = format!("{} extends ", repository.name);
    let args = header.strip_prefix(&parent)?;
    if ![
        "org.springframework.data.repository.CrudRepository",
        "org.springframework.data.jpa.repository.JpaRepository",
    ]
    .iter()
    .any(|name| args.starts_with(&format!("{name}<")))
    {
        return None;
    }
    let generic = args.split_once('<')?.1.strip_suffix('>')?;
    let (entity, id) = generic.split_once(',')?;
    let entity = entity.trim();
    if id.trim() != repository.id_type
        || !entity.contains('.')
        || !entity.split('.').all(|p| {
            !p.is_empty()
                && p.bytes().enumerate().all(|(i, b)| {
                    b.is_ascii_alphabetic() || b == b'_' || i > 0 && b.is_ascii_digit()
                })
        })
    {
        return None;
    }
    let ty = canonical(&repository.id_type)?;
    // Generic arguments cannot be primitive, escaped, projected or transformed spellings.
    if doc.text.get(repository.id_start..repository.id_end) != Some(ty) {
        return None;
    }
    Some((
        entity.into(),
        ty.into(),
        repository.id_start,
        repository.id_end,
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::JpaAssist;
    use openidea_plugin::{diagnostics::DiagnosticsProvider, mock::MockHost, text::offset};
    const ENTITY: &str = "package shop;\n@jakarta.persistence.Entity class Person { @jakarta.persistence.Id long id; }";
    const REPO: &str = "package shop;\ninterface People extends org.springframework.data.jpa.repository.JpaRepository<shop.Person, java.lang.String> {}";
    fn doc(path: &str, text: &str) -> Document {
        Document {
            path: path.into(),
            text: text.into(),
            language: "java".into(),
            version: 1,
        }
    }
    fn check(plugin: &mut JpaAssist, text: &str) -> Vec<openidea_plugin::Diagnostic> {
        plugin
            .diagnostics(&doc("/project/src/main/java/shop/People.java", text))
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("jpa-source-repository-id"))
            .collect()
    }
    #[test]
    fn missing_identifier_note_is_current_source_absence_not_runtime_error() {
        let _host = MockHost::install().with_capabilities(&["project.read"]);
        let mut plugin = JpaAssist::default();
        let source = ENTITY.replace("@jakarta.persistence.Id long id;", "long id;");
        let path = "/project/src/main/java/shop/Person.java";
        let found = plugin.diagnostics(&doc(path, &source));
        let d = found
            .iter()
            .find(|d| d.code.as_deref() == Some("jpa-source-identifier-absent"))
            .unwrap();
        assert_eq!(d.severity, openidea_plugin::Severity::Error);
        assert!(d.message.contains("no mapping XML"));
        assert_eq!(
            &source[offset(&source, d.range.start).unwrap()..offset(&source, d.range.end).unwrap()],
            "Person"
        );
        for unknown in [
            ENTITY.into(),
            source.replace("class Person {", "class Person extends Base {"),
            source.replace("jakarta.persistence.Entity", "vendor.Entity"),
            source.replace("class Person {", "class Person<T> {"),
            source.trim_end_matches('}').into(),
        ] {
            assert!(
                plugin
                    .diagnostics(&doc(path, &unknown))
                    .iter()
                    .all(|d| d.code.as_deref() != Some("jpa-source-identifier-absent")),
                "{unknown}"
            );
        }
    }

    #[test]
    fn xml_that_may_map_the_class_keeps_the_note_at_info_without_a_fix() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        let mut plugin = JpaAssist::default();
        let source = ENTITY.replace("@jakarta.persistence.Id long id;", "long id;");
        let path = "/project/src/main/java/shop/Person.java";
        let level = |plugin: &mut JpaAssist| {
            let found = plugin.diagnostics(&doc(path, &source));
            found.into_iter().find(|d| d.code.as_deref() == Some("jpa-source-identifier-absent")).map(|d| d.severity)
        };
        assert_eq!(level(&mut plugin), Some(openidea_plugin::Severity::Error));
        host.add_file("src/main/resources/META-INF/orm.xml", "<entity-mappings xmlns=\"x\"><entity class=\"shop.Other\"><attributes><id name=\"k\"/></attributes></entity></entity-mappings>");
        assert_eq!(level(&mut plugin), Some(openidea_plugin::Severity::Error));
        host.add_file("src/main/resources/META-INF/orm.xml", "<entity-mappings xmlns=\"x\"><entity class=\"shop.Person\"><attributes><id name=\"k\"/></attributes></entity></entity-mappings>");
        assert_eq!(level(&mut plugin), Some(openidea_plugin::Severity::Info));
        host.add_file("src/main/resources/META-INF/orm.xml", "<entity-mappings xmlns=\"x\"/>");
        host.add_file("src/main/resources/legacy.hbm.xml", "<hibernate-mapping/>");
        assert_eq!(level(&mut plugin), Some(openidea_plugin::Severity::Info));
    }

    #[test]
    fn create_id_fix_is_one_edit_with_the_import_and_skips_name_clashes() {
        let _host = MockHost::install().with_capabilities(&["project.read"]);
        let class = |text: &str| model::parse_class(text, "src/main/java/shop/Person.java").unwrap();
        let text = "package shop;\nimport jakarta.persistence.Entity;\n@Entity\nclass Person {\n  String name;\n}\n";
        let open = text.find('{').unwrap();
        let (title, edits) = create_id_fix(text, &class(text), open).unwrap();
        assert_eq!(title, "Create id");
        let mut after = text.to_owned();
        let mut ordered: Vec<_> = edits.iter().collect();
        ordered.sort_by_key(|e| std::cmp::Reverse(e.start));
        for e in ordered {
            after.replace_range(e.start..e.end, &e.text);
        }
        assert_eq!(after, "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\nclass Person {\n    @Id\n    private Long id;\n\n  String name;\n}\n");
        let clash = text.replace("String name;", "String id;");
        assert!(create_id_fix(&clash, &class(&clash), clash.find('{').unwrap()).is_none());
    }

    #[test]
    fn positive_saved_identifier_model_reports_exact_mismatch_and_withdraws_stale_facts() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("src/main/java/shop/Person.java", ENTITY);
        let mut plugin = JpaAssist::default();
        assert!(check(&mut plugin, REPO).is_empty(), "cold cache is unknown");
        assert_eq!(
            identifier(&doc("/project/src/main/java/shop/Person.java", ENTITY)),
            Some("java.lang.Long".into())
        );
        assert!(repository(&doc("/project/src/main/java/shop/People.java", REPO)).is_some());
        plugin.diagnostics(&doc("/project/src/main/java/shop/Person.java", ENTITY));
        let source = format!("/* 🦉 */ {REPO}");
        let ds = check(&mut plugin, &source);
        assert_eq!(ds.len(), 1);
        assert_eq!(
            &source[offset(&source, ds[0].range.start).unwrap()
                ..offset(&source, ds[0].range.end).unwrap()],
            "java.lang.String"
        );
        assert!(
            check(
                &mut plugin,
                &REPO.replace("java.lang.String", "java.lang.Long")
            )
            .is_empty()
        );
        host.add_file(
            "src/main/java/shop/Person.java",
            &ENTITY.replace("long id", "java.lang.String id"),
        );
        assert!(
            check(&mut plugin, REPO).is_empty(),
            "changed disk source invalidates positive cached facts"
        );
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }
    #[test]
    fn inherited_ambiguous_foreign_and_transformed_models_remain_unknown() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        let mut plugin = JpaAssist::default();
        for entity in [
            ENTITY.replace("class Person {", "class Person extends Base {"),
            ENTITY.replace("jakarta.persistence.Entity", "app.Entity"),
            ENTITY.replace("long id", "Money id"),
            format!("import app.java;\n{}", ENTITY),
            ENTITY.replace(
                "@jakarta.persistence.Id long id;",
                "@jakarta.persistence.Id long getId(){return 0;}",
            ),
        ] {
            host.add_file("src/main/java/shop/Person.java", &entity);
            plugin.diagnostics(&doc("/project/src/main/java/shop/Person.java", &entity));
            assert!(check(&mut plugin, REPO).is_empty());
        }
        host.add_file("src/main/java/shop/Person.java", ENTITY);
        plugin.diagnostics(&doc("/project/src/main/java/shop/Person.java", ENTITY));
        for repo in [
            REPO.replace(
                "org.springframework.data.jpa.repository.JpaRepository",
                "vendor.JpaRepository",
            ),
            REPO.replace("shop.Person", "Person"),
            REPO.replace("java.lang.String", "String"),
            REPO.trim_end_matches('}').into(),
        ] {
            assert!(check(&mut plugin, &repo).is_empty(), "{repo}");
        }
        host.set_setting("jpaAssist.checkAttributeTypes", "false");
        assert!(check(&mut plugin, REPO).is_empty());
    }
}
