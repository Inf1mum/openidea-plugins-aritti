//! Current-source facts only; deliberately no index/Rename contribution.
use crate::{
    JpaAssist,
    checks::name_references,
    live,
    model::{self, Attribute, Class},
};
use openidea_plugin::{
    Location, Position, Range, host,
    hover::{Hover, HoverProvider},
    providers::Document,
    text::{offset, position},
};
const MAX_TEXT: usize = 128 * 1024;

pub(super) fn snapshot(doc: &Document) -> Option<Class> {
    if !matches!(doc.language.as_str(), "java" | "kotlin")
        || doc.text.len() > MAX_TEXT
        || (doc.language == "java" && !doc.path.ends_with(".java"))
        || (doc.language == "kotlin" && !doc.path.ends_with(".kt"))
    {
        return None;
    }
    let root = host::project_of(&doc.path)?.root().ok()?;
    if !doc.path.strip_prefix(&root).is_some_and(|rest| root == "/" || rest.starts_with('/'))
        || doc.path.split('/').any(|part| matches!(part, "." | ".."))
    {
        return None;
    }
    let masked = model::mask(&doc.text);
    if model::words(&masked, "class").count() != 1
        || ["interface", "enum", "object", "typealias"].iter().any(|word| model::words(&masked, word).next().is_some())
        || !live::trusted_annotations(&doc.text)
    {
        return None;
    }
    if !balanced(&masked) {
        return None;
    }
    let annotations = model::annotations(&masked);
    if annotations.len() > 256 || annotations.iter().any(|a| a.name == "Access") {
        return None;
    }
    let class = model::parse_class(&doc.text, &doc.path)?;
    crate::checks::owned_class_start(&doc.text,&masked,&class)?;
    if class.body_end.is_none()
        || class.name.len() > 128
        || class.attributes.len() > 128
        || class.package.as_ref().is_none_or(|p| p.len() > 512 || !qualified(p))
    {
        return None;
    }
    // An annotation's short spelling is not evidence of framework ownership.
    for a in &annotations {
        let written = &masked[a.start + 1..a.name_end];
        let known = |name: &str| name == format!("jakarta.persistence.{}", a.name) || name == format!("javax.persistence.{}", a.name);
        if written.contains('.') {
            if !known(written) {
                return None;
            }
        } else {
            let imports: Vec<_> = class.imports.iter().filter(|i| i.rsplit('.').next() == Some(a.name)).collect();
            if imports.len() != 1 || !known(imports[0]) {
                return None;
            }
        }
    }
    let mut names = std::collections::HashSet::new();
    if class.attributes.iter().any(|a| {
        !identifier(&a.name)
            || a.name.len() > 128
            || a.ty.len() > 512
            || a.column.as_ref().is_some_and(|s| s.len() > 512)
            || !names.insert(&a.name)
            || span(doc, a).is_none()
    }) {
        return None;
    }
    Some(class)
}
pub(super) fn balanced(text: &str) -> bool {
    let mut stack = Vec::new();
    for byte in text.bytes() {
        match byte {
            b'{' | b'(' | b'[' => {
                if stack.len() >= 128 {
                    return false;
                }
                stack.push(byte);
            }
            b'}' | b')' | b']' => {
                let expected = match byte {
                    b'}' => b'{',
                    b')' => b'(',
                    _ => b'[',
                };
                if stack.pop() != Some(expected) {
                    return false;
                }
            }
            _ => {}
        }
    }
    stack.is_empty()
}
fn escaped(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().take(512) {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\n' | '\r' => out.push(' '),
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '#' | '|' | '!' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}
fn qualified(s: &str) -> bool {
    !s.is_empty() && s.split('.').all(identifier)
}
fn identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}
fn span(doc: &Document, attribute: &Attribute) -> Option<Range> {
    let at = offset(&doc.text, attribute.pos)?;
    (doc.text.get(at..at + attribute.name.len()) == Some(attribute.name.as_str()))
        .then(|| Range::new(attribute.pos, position(&doc.text, at + attribute.name.len())))
}
fn selected<'a>(doc: &Document, at: Position, class: &'a Class) -> Option<(&'a Attribute, Range)> {
    let byte = offset(&doc.text, at)?;
    for attribute in &class.attributes {
        let range = span(doc, attribute)?;
        if range.start <= at && at < range.end {
            return Some((attribute, range));
        }
    }
    // Only top-level NamedEntityGraph nodes have an established current-class
    // owner. Subgraphs, paths, overrides and external owners remain unknown.
    let reference = name_references(&doc.text, class.kotlin, class).into_iter().find(|r| {
        r.annotation == "@NamedAttributeNode" && r.owner == class.name && r.start <= byte && byte < r.end && identifier(&r.value)
    })?;
    let attribute = class.attributes.iter().find(|a| a.name == reference.value)?;
    Some((attribute, Range::new(position(&doc.text, reference.start), position(&doc.text, reference.end))))
}
fn text(class: &Class, attribute: &Attribute) -> String {
    let owner = format!("{}.{}", class.package.as_deref().unwrap_or_default(), class.name);
    let mut lines = vec![
        format!("JPA source attribute: {}.{}", escaped(&owner), escaped(&attribute.name)),
        format!("Declared type: {}", escaped(&attribute.ty)),
    ];
    if attribute.id {
        lines.push("Declared identifier annotation".into());
    }
    if let Some(column) = &attribute.column {
        lines.push(format!("Declared column: {}", escaped(column)));
    }
    if let Some(table) = &class.table {
        lines.push(format!("Declared table: {}", escaped(table)));
    }
    lines.push("Current source declaration; effective access, inheritance, XML overrides and database mappings are not resolved.".into());
    lines.join("\n")
}
impl HoverProvider for JpaAssist {
    fn hover(&mut self, doc: &Document, at: Position) -> Option<Hover> {
        let class = snapshot(doc)?;
        let (attribute, range) = selected(doc, at, &class)?;
        Some(Hover::new(text(&class, attribute)).range(range))
    }
}
pub fn definition(doc: &Document, at: Position) -> Vec<Location> {
    let Some(class) = snapshot(doc) else {
        return vec![];
    };
    let Some((attribute, selected)) = selected(doc, at, &class) else {
        return vec![];
    };
    let Some(range) = span(doc, attribute) else {
        return vec![];
    };
    if selected == range {
        return vec![];
    }
    vec![Location { path: doc.path.clone(), range }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, providers::Providers};
    const JAVA: &str = "package app;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.Column;\nimport jakarta.persistence.NamedEntityGraph;\nimport jakarta.persistence.NamedAttributeNode;\n@Entity\n@NamedEntityGraph(name = \"summary\", attributeNodes = @NamedAttributeNode(\"name\"))\nclass Person {\n@Id Long id;\n/* 😀 */ @Column(name = \"display_name\") String name;\n}";
    fn doc(text: &str) -> Document {
        Document { path: "/project/Person.java".into(), language: "java".into(), version: 1, text: text.into() }
    }
    fn at(doc: &Document, token: &str) -> Position {
        position(&doc.text, doc.text.find(token).unwrap())
    }
    #[test]
    fn declaration_and_local_graph_reference_have_exact_utf16_ranges() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let d = doc(JAVA);
        let declaration = at(&d, "name;");
        let hover = plugin.hover(&d, declaration).unwrap();
        assert_eq!(hover.range.unwrap().start, declaration);
        assert_eq!(hover.range.unwrap().end.character, declaration.character + 4);
        assert!(hover.markdown.contains("app.Person.name") && hover.markdown.contains("display\\_name"));
        let byte = d.text.find("@NamedAttributeNode(\"name\"").unwrap() + "@NamedAttributeNode(\"".len();
        let reference = position(&d.text, byte);
        let hover = plugin.hover(&d, reference).unwrap();
        assert_eq!(hover.range.unwrap(), Range::new(reference, position(&d.text, byte + 4)));
        let locations = plugin.definition(&d, reference);
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].range.start, declaration);
        assert_eq!(locations[0].path, d.path);
        assert!(plugin.definition(&d, declaration).is_empty());
        assert!(plugin.hover(&d, position(&d.text, byte + 4)).is_none());
    }
    #[test]
    fn current_unsaved_text_wins_without_any_file_or_cache_read() {
        let host = MockHost::install();
        host.add_file("Person.java", JAVA);
        let mut plugin = JpaAssist::default();
        let old = doc(JAVA);
        assert!(plugin.hover(&old, at(&old, "name;")).is_some());
        let mut changed = doc(&JAVA.replace("String name;", "String renamed;"));
        changed.version = 2;
        let graph = position(&changed.text, changed.text.find("@NamedAttributeNode(\"name\"").unwrap() + "@NamedAttributeNode(\"".len());
        assert!(plugin.hover(&changed, graph).is_none());
        assert!(plugin.definition(&changed, graph).is_empty());
        assert!(plugin.hover(&changed, at(&changed, "renamed;")).unwrap().markdown.contains("renamed"));
        assert!(plugin.classes.is_empty() && plugin.sources.is_empty());
    }
    #[test]
    fn unknown_foreign_wildcard_duplicate_and_malformed_owners_are_quiet() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        for text in [
            JAVA.replace("import jakarta.persistence.Entity;", ""),
            JAVA.replace("import jakarta.persistence.Entity;", "import custom.Entity;"),
            JAVA.replace("import jakarta.persistence.Entity;", "import jakarta.persistence.*;"),
            format!("{JAVA} class Other {{}}"),
            JAVA.replace("String name;", "String name; String name;"),
            JAVA.replace("String name;", "String name; void malformed( {}"),
            format!("{JAVA}{}", " ".repeat(MAX_TEXT)),
            JAVA.replace("package app;", ""),
            JAVA.replace("@Entity", "@Access(AccessType.PROPERTY) @Entity"),
        ] {
            let d = doc(&text);
            let at = at(&d, "name;");
            assert!(plugin.hover(&d, at).is_none(), "{text}");
            assert!(plugin.definition(&d, at).is_empty());
        }
    }
    #[test]
    fn dotted_dynamic_unknown_and_subgraph_references_are_not_rebound() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        for expression in ["\"name.child\"", "\"missing\"", "NAME", "\"na\" + \"me\""] {
            let d = doc(&JAVA.replace("@NamedAttributeNode(\"name\")", &format!("@NamedAttributeNode({expression})")));
            let at = position(&d.text, d.text.find("@NamedAttributeNode(").unwrap() + "@NamedAttributeNode(".len() + 1);
            assert!(plugin.hover(&d, at).is_none());
            assert!(plugin.definition(&d, at).is_empty());
        }
        let d = doc(&JAVA
            .replace(
                "import jakarta.persistence.NamedEntityGraph;",
                "import jakarta.persistence.NamedEntityGraph;\nimport jakarta.persistence.NamedSubgraph;",
            )
            .replace(
                "attributeNodes = @NamedAttributeNode(\"name\")",
                "subgraphs = @NamedSubgraph(name = \"sub\", attributeNodes = @NamedAttributeNode(\"name\"))",
            ));
        let at = position(&d.text, d.text.find("@NamedAttributeNode(\"name\"").unwrap() + "@NamedAttributeNode(\"".len());
        assert!(plugin.hover(&d, at).is_none());
        assert!(plugin.definition(&d, at).is_empty());
    }
    #[test]
    fn anonymous_denied_outside_and_project_switch_requests_do_not_reuse_facts() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let d = doc(JAVA);
        let at = at(&d, "name;");
        assert!(plugin.hover(&d, at).is_some());
        host.set_project_root(None);
        assert!(plugin.hover(&d, at).is_none());
        host.set_project_root(Some("/other"));
        assert!(plugin.hover(&d, at).is_none());
        let mut other = d.clone();
        other.path = "/other/Person.java".into();
        assert!(plugin.hover(&other, at).is_some());
        let mut traversal = other.clone();
        traversal.path = "/other/../project/Person.java".into();
        assert!(plugin.hover(&traversal, at).is_none());
        let _host = host.with_capabilities(&[]);
        assert!(plugin.hover(&other, at).is_none());
    }
    #[test]
    fn kotlin_javax_and_markup_are_source_facts_without_html() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let text = "package app\nimport javax.persistence.Entity\nimport javax.persistence.Id\nimport javax.persistence.Column\n@Entity class Person {\n@Id var id: Long = 1\n@Column(name = \"<img>![x](https://example.com)\") var name: String = \"\"\n}";
        let mut d = doc(text);
        d.path = "/project/Person.kt".into();
        d.language = "kotlin".into();
        let hover = plugin.hover(&d, at(&d, "name: String")).unwrap();
        assert!(hover.markdown.contains("&lt;img&gt;\\!\\[x\\]"));
        assert!(!hover.markdown.contains("<img>"));
    }
    #[test]
    fn local_kotlin_class_does_not_borrow_top_level_qualified_hover_identity() {
        let _host=MockHost::install();let mut plugin=JpaAssist::default();
        let text="package app\nfun create() {\n @jakarta.persistence.Entity class Person { @jakarta.persistence.Id var id: kotlin.Long? = null; var title: kotlin.String = \"name\" }\n}\n";
        let d=Document{path:"/project/Person.kt".into(),language:"kotlin".into(),version:1,text:text.into()};let at=position(text,text.find("title").unwrap());
        assert!(plugin.hover(&d,at).is_none());
        let top="package app\n@jakarta.persistence.Entity class Person { @jakarta.persistence.Id var id: Long? = null; var title: String = \"name\" }";
        let top=Document{text:top.into(),..d.clone()};
        openidea_plugin::diagnostics::DiagnosticsProvider::diagnostics(&mut plugin,&top);
        let repository="package app\nimport org.springframework.data.jpa.repository.JpaRepository\ninterface PersonRepository : JpaRepository<Person,Long> { fun findBy }";
        let repository=Document{path:"/project/PersonRepository.kt".into(),language:"kotlin".into(),version:1,text:repository.into()};
        let at=position(&repository.text,repository.text.find("findBy").unwrap()+6);
        assert!(plugin.completion(&repository,at,None).iter().any(|i|i.label=="findByTitle"));
        openidea_plugin::diagnostics::DiagnosticsProvider::diagnostics(&mut plugin,&d);
        assert!(plugin.completion(&repository,at,None).is_empty(),"function-local class cannot remain a cached top-level repository domain");
    }

    #[test]
    fn snapshot_owner_preserves_direct_top_level_roles_ranges_and_unknown_corrections() {
        let host=MockHost::install();let mut plugin=JpaAssist::default();
        for role in ["Entity","MappedSuperclass","Embeddable"] {
            let text=JAVA.replace("import jakarta.persistence.Entity;",&format!("import jakarta.persistence.{role};")).replace("@Entity",&format!("@{role}"));
            let d=doc(&text);let pos=at(&d,"name;");let hover=plugin.hover(&d,pos).unwrap();assert_eq!(hover.range.unwrap().start,pos);assert!(hover.markdown.contains("app.Person.name"));
        }
        let text="package app\n@jakarta.persistence.Entity class Person { @jakarta.persistence.Id var id: kotlin.Long? = null; /*😀*/ var title: kotlin.String = \"name\" }";
        let mut d=Document{path:"/project/Person.kt".into(),language:"kotlin".into(),version:1,text:text.into()};let pos=position(text,text.find("title").unwrap());let hover=plugin.hover(&d,pos).unwrap();assert_eq!(hover.range.unwrap().start,pos);assert!(hover.markdown.contains("app.Person.title"));
        let structural=text.replace("var title: kotlin.String", "@jakarta.persistence.ManyToOne @jakarta.persistence.OneToOne var title: kotlin.String");
        let findings=openidea_plugin::diagnostics::DiagnosticsProvider::diagnostics(&mut plugin,&Document{text:structural,..d.clone()});
        assert!(findings.iter().any(|f|f.code.as_deref()==Some("jpa-attribute-multiple-relations")));
        for unknown in [
            text.replace("@jakarta.persistence.Entity","@jakarta.persistence.NamedEntityGraph(@jakarta.persistence.Entity)"),
            text.replace("@jakarta.persistence.Entity","@jakarta.persistence.Entity @jakarta.persistence.Embeddable"),
            text.replace("@jakarta.persistence.Entity","@foreign.Entity"),
            text.replace("@jakarta.persistence.Entity","@Entity"),
            text.trim_end_matches('}').into(),
        ] {d.text=unknown;d.version+=1;let pos=position(&d.text,d.text.find("title").unwrap());assert!(plugin.hover(&d,pos).is_none() && snapshot(&d).is_none(),"{}",d.text);}
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }

}
