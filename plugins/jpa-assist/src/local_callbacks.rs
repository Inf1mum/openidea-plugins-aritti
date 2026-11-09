//! Modifier checks for a proved current Java Entity; no listener-role inference.
use crate::{
    attribute_info,
    checks::Finding,
    model::{self, Kind},
};
use openidea_plugin::{Severity, host, providers::Document};

pub(super) fn findings(doc: &Document) -> Vec<Finding> {
    if doc.language != "java" {
        return vec![];
    }
    let Some(class) = attribute_info::snapshot(doc) else {
        return vec![];
    };
    let Some(root) = host::project_of(&doc.path).and_then(|p| p.root().ok()) else {
        return vec![];
    };
    let Some(package) = &class.package else {
        return vec![];
    };
    if class.kind != Kind::Entity
        || class.superclass.is_some()
        || class.imports.iter().any(|i| i.ends_with(".*"))
        || doc.path
            != format!(
                "{root}/src/main/java/{}/{}.java",
                package.replace('.', "/"),
                class.name
            )
    {
        return vec![];
    }
    let masked = model::mask(&doc.text);
    let Some((class_at, _)) = model::declaration_keyword(&masked, 0, &["class"]) else {
        return vec![];
    };
    let Some(open) = masked[class_at..].find('{').map(|i| class_at + i) else {
        return vec![];
    };
    // No generic/supertype/implementation or nested ownership claims.
    if masked[class_at + 5..open].trim() != class.name {
        return vec![];
    }
    let all = model::annotations(&masked);
    let namespace = |a: &model::Annotation<'_>| {
        let written = &masked[a.start + 1..a.name_end];
        let full = if written.contains('.') {
            written
        } else {
            class
                .imports
                .iter()
                .find(|i| i.rsplit('.').next() == Some(a.name))?
                .as_str()
        };
        ["jakarta.persistence", "javax.persistence"]
            .into_iter()
            .find(|ns| full == format!("{ns}.{}", a.name))
    };
    let entities: Vec<_> = all.iter().filter(|a| a.name == "Entity").collect();
    let [entity] = entities.as_slice() else {
        return vec![];
    };
    let Some(ns) = namespace(entity) else {
        return vec![];
    };
    if entity.start >= class_at
        || all.iter().any(|a| {
            namespace(a) != Some(ns)
                || ![
                    "Entity",
                    "Id",
                    "PrePersist",
                    "PostPersist",
                    "PreUpdate",
                    "PostUpdate",
                    "PreRemove",
                    "PostRemove",
                    "PostLoad",
                ]
                .contains(&a.name)
        })
    {
        return vec![];
    }
    let Some(end) = class.body_end else {
        return vec![];
    };
    let mut found = vec![];
    let mut events = std::collections::HashSet::new();
    for (start, end) in model::members(&masked, open + 1, end, false, false) {
        let callbacks: Vec<_> = all
            .iter()
            .filter(|a| {
                a.start >= start && a.start < end && crate::checks::CALLBACKS.contains(&a.name)
            })
            .collect();
        if callbacks.is_empty() || callbacks.iter().any(|a| a.args.is_some()) {
            continue;
        }
        let decl = model::without_annotations(&masked, start, end);
        let Some(paren) = decl.find('(') else {
            continue;
        };
        if callbacks.iter().any(|a| a.end() > start + paren) {
            continue;
        }
        let Some(close) = model::closing(decl.as_bytes(), paren) else {
            continue;
        };
        let head: Vec<_> = decl[..paren].split_whitespace().collect();
        if head.len() < 2 {
            continue;
        }
        let name = head[head.len() - 1];
        let ty = head[head.len() - 2];
        let identifier = |s: &str| {
            !s.is_empty()
                && s.bytes().enumerate().all(|(i, b)| {
                    b.is_ascii_alphabetic() || b == b'_' || b == b'$' || i > 0 && b.is_ascii_digit()
                })
        };
        if !identifier(name)
            || ![
                "void", "int", "long", "short", "byte", "char", "boolean", "float", "double",
            ]
            .contains(&ty)
            || head[..head.len() - 2].iter().any(|m| {
                ![
                    "public",
                    "private",
                    "protected",
                    "static",
                    "final",
                    "synchronized",
                    "strictfp",
                ]
                .contains(m)
            })
            || !decl[close + 1..].trim_start().starts_with('{')
        {
            continue;
        }
        let params = decl[paren + 1..close].trim();
        if !params.is_empty()
            && !params.split(',').all(|p| {
                let words: Vec<_> = p.split_whitespace().collect();
                words.len() == 2 && identifier(words[1]) && words[0].split('.').all(identifier)
            })
        {
            continue;
        }
        let problem = if head[..head.len() - 2]
            .iter()
            .any(|m| ["static", "final"].contains(m))
        {
            Some((
                "jpa-callback-modifiers",
                "A source-declared entity lifecycle callback must not be static or final.",
            ))
        } else if ty != "void" || !params.is_empty() {
            Some((
                "entity-listener-signature",
                "A source-declared entity lifecycle callback must return void and take no parameters.",
            ))
        } else {
            None
        };
        let at = start + decl[..paren].rfind(name).unwrap();
        if let Some((code, message)) = problem {
            found.push(Finding::new(
                at,
                at + name.len(),
                Severity::Error,
                code,
                message,
            ));
        } else {
            // Multiple different events on one method are legal. Only a later
            // distinct supported method for an already seen event is reported.
            let mut method_events = std::collections::HashSet::new();
            let mut duplicates = vec![];
            for callback in callbacks {
                if method_events.insert(callback.name) && !events.insert(callback.name) {
                    duplicates.push(callback.name);
                }
            }
            if !duplicates.is_empty() {
                found.push(Finding::new(
                    at, at + name.len(), Severity::Error,
                    "jpa-duplicate-lifecycle-event",
                    format!("This source-declared entity already has a local callback method for {}. Use one method per lifecycle event.", duplicates.join(", ")),
                ));
            }
        }
        if found.len() >= 128 {
            break;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::JpaAssist;
    use openidea_plugin::{diagnostics::DiagnosticsProvider, mock::MockHost, text::offset};
    const SOURCE: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.PrePersist;\nimport jakarta.persistence.PostLoad;\n@Entity class Person { @Id long id; /* 🦉 */ @PrePersist static void before() {} @PostLoad final void after() {} }";
    fn doc(text: &str) -> Document {
        Document {
            path: "/project/src/main/java/shop/Person.java".into(),
            language: "java".into(),
            version: 1,
            text: text.into(),
        }
    }
    fn diagnostics(plugin: &mut JpaAssist, d: &Document) -> Vec<openidea_plugin::Diagnostic> {
        plugin
            .diagnostics(d)
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("jpa-callback-modifiers"))
            .collect()
    }
    #[test]
    fn current_entity_modifiers_have_exact_ranges_and_current_text_clears_them() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let mut d = doc(SOURCE);
        let found = diagnostics(&mut plugin, &d);
        assert_eq!(found.len(), 2);
        let names: Vec<_> = found
            .iter()
            .map(|f| {
                &d.text
                    [offset(&d.text, f.range.start).unwrap()..offset(&d.text, f.range.end).unwrap()]
            })
            .collect();
        assert_eq!(names, ["before", "after"]);
        d.text = SOURCE
            .replace("static void", "private void")
            .replace("final void", "protected void");
        d.version += 1;
        assert!(diagnostics(&mut plugin, &d).is_empty());
    }
    #[test]
    fn unknown_roles_namespaces_and_ownership_are_quiet() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        for text in [
            SOURCE.replace("@Entity class", "class"),
            SOURCE.replace("class Person {", "class Person extends Base {"),
            SOURCE.replace("class Person {", "class Person implements Marker {"),
            SOURCE.replace("class Person {", "class Person<T> {"),
            SOURCE.replace("import jakarta.persistence.Entity;", ""),
            SOURCE.replace(
                "import jakarta.persistence.PrePersist;",
                "import foreign.PrePersist;",
            ),
            SOURCE.replace(
                "import jakarta.persistence.PostLoad;",
                "import javax.persistence.PostLoad;",
            ),
            SOURCE.replace(
                "import jakarta.persistence.Entity;",
                "import jakarta.persistence.*;",
            ),
            SOURCE.replace("@Id long id;", "@Id long id; class Nested {}"),
            SOURCE.replace("@Entity class", "@MappedSuperclass class"),
            SOURCE.replace(
                "@PrePersist static void before() {} @PostLoad final void after() {}",
                "static void outer() { @PrePersist int local; }",
            ),
        ] {
            let d = doc(&text);
            assert!(diagnostics(&mut plugin, &d).is_empty(), "{text}");
            assert!(
                plugin
                    .diagnostics(&d)
                    .iter()
                    .all(|d| d.code.as_deref() != Some("entity-listener-signature"))
            );
        }
        let mut d = doc(SOURCE);
        d.language = "kotlin".into();
        assert!(diagnostics(&mut plugin, &d).is_empty());
        d = doc(SOURCE);
        d.path = "/project/other/Person.java".into();
        assert!(diagnostics(&mut plugin, &d).is_empty());
    }
    #[test]
    fn role_dependent_signatures_require_owned_entity_and_gating_is_preserved() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let text = SOURCE
            .replace("static void before()", "void before(int value)")
            .replace("final void after()", "int after()");
        let d = doc(&text);
        assert_eq!(
            plugin
                .diagnostics(&d)
                .iter()
                .filter(|d| d.code.as_deref() == Some("entity-listener-signature"))
                .count(),
            2
        );
        let listener = text.replace("@Entity class", "class");
        assert!(
            plugin
                .diagnostics(&doc(&listener))
                .iter()
                .all(|d| d.code.as_deref() != Some("entity-listener-signature"))
        );
        let qualified = SOURCE
            .replace("@Entity", "@jakarta.persistence.Entity")
            .replace("@PrePersist", "@jakarta.persistence.PrePersist")
            .replace("@PostLoad", "@jakarta.persistence.PostLoad");
        assert_eq!(diagnostics(&mut plugin, &doc(&qualified)).len(), 2);
        host.set_setting("jpaAssist.checkListenerSignatures", "false");
        assert!(diagnostics(&mut plugin, &doc(SOURCE)).is_empty());
        host.set_setting("jpaAssist.checkListenerSignatures", "true");
        host.add_project("/project/src/main/java/shop");
        assert!(diagnostics(&mut plugin, &doc(SOURCE)).is_empty());
        let _host = MockHost::install().with_capabilities(&[]);
        assert!(diagnostics(&mut JpaAssist::default(), &doc(SOURCE)).is_empty());
    }
    #[test]
    fn duplicate_events_report_second_local_method_and_current_text_clears() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let text = SOURCE.replace("@PrePersist static void before() {} @PostLoad final void after() {}", "@PrePersist void first() {} /* 🦉 */ @PrePersist void second() {} @PostLoad void loaded() {}");
        let mut d = doc(&text);
        let found: Vec<_> = plugin
            .diagnostics(&d)
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("jpa-duplicate-lifecycle-event"))
            .collect();
        assert_eq!(found.len(), 1);
        let range = found[0].range;
        assert_eq!(
            &d.text[offset(&d.text, range.start).unwrap()..offset(&d.text, range.end).unwrap()],
            "second"
        );
        d.text = d.text.replace("@PrePersist void second", "void second");
        d.version += 1;
        assert!(
            plugin
                .diagnostics(&d)
                .iter()
                .all(|d| d.code.as_deref() != Some("jpa-duplicate-lifecycle-event"))
        );
    }

    #[test]
    fn multiple_distinct_events_on_one_method_are_legal_and_unknown_roles_stay_quiet() {
        let _host = MockHost::install();
        let mut plugin = JpaAssist::default();
        let text = SOURCE.replace(
            "@PrePersist static void before() {} @PostLoad final void after() {}",
            "@PrePersist @PostLoad void both() {} void other() {}",
        );
        assert!(
            plugin
                .diagnostics(&doc(&text))
                .iter()
                .all(|d| d.code.as_deref() != Some("jpa-duplicate-lifecycle-event"))
        );
        let duplicate = text.replace("void other()", "@PrePersist void other()");
        assert_eq!(
            plugin
                .diagnostics(&doc(&duplicate))
                .iter()
                .filter(|d| d.code.as_deref() == Some("jpa-duplicate-lifecycle-event"))
                .count(),
            1
        );
        for unknown in [
            duplicate.replace("@Entity class", "class"),
            duplicate.replace("class Person {", "class Person extends Base {"),
            duplicate.replace(
                "import jakarta.persistence.PrePersist;",
                "import foreign.PrePersist;",
            ),
            duplicate.replace(
                "import jakarta.persistence.PostLoad;",
                "import javax.persistence.PostLoad;",
            ),
            duplicate.replace(
                "@PrePersist void other()",
                "void other() { @PrePersist int local; } void extra()",
            ),
            duplicate.replace("void both()", "LibraryType both()"),
            duplicate.replace("void other()", "LibraryType other()"),
        ] {
            assert!(
                plugin
                    .diagnostics(&doc(&unknown))
                    .iter()
                    .all(|d| d.code.as_deref() != Some("jpa-duplicate-lifecycle-event")),
                "{unknown}"
            );
        }
    }
}
