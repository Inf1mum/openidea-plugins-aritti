//! Context-dependent model of the Designer's JPA Palette (G005-C1, G005-C2). A pure function of the source open in
//! the Designer: it reuses the eligibility checks of the code actions the items name, reads no other file and
//! searches nothing, so it fits the provider budget. A web view cannot start editor actions, so a non-form item
//! carries the exact action title to run from the editor.
use super::*;
use serde_json::{Value, json};

const NO_ENTITY: &str = "Open a JPA entity in the editor first.";
const MAX_TEXT: usize = 128 * 1024;
const NOT_IN_DESIGNER: &str = "The Designer edits only sources whose annotations it fully understands (persistence annotations; Lombok and other foreign annotations are not). Use the editor action Add JPA attribute with preview….";
const ON_CLASS: &str = "on the entity class declaration";
const LOMBOK: [&str; 4] = ["Data", "Value", "EqualsAndHashCode", "ToString"];

fn item(id: &str, label: &str, block: Option<String>, rest: Value) -> Value {
    let mut out = json!({"id":id,"label":label,"enabled":block.is_none(),"reason":block});
    if let (Some(out), Some(rest)) = (out.as_object_mut(), rest.as_object()) {
        out.extend(rest.clone());
    }
    out
}

fn attribute(id: &str, label: &str, kind: &str, enumerated: bool, block: Option<String>) -> Value {
    item(id, label, block, json!({"attribute":{"kind":kind,"enumerated":enumerated}}))
}

fn action(id: &str, label: &str, title: &str, place: &str, block: Option<String>) -> Value {
    item(id, label, block, json!({"action":title,"where":place}))
}

fn reason(ok: bool, text: impl Into<String>) -> Option<String> {
    (!ok).then(|| text.into())
}

fn lombok_in(masked: &str, names: &[&str]) -> Option<String> {
    annotations(masked).iter().find(|a| names.contains(&a.name)).map(|a| a.name.to_owned())
}

fn why_not_equality(class: &Class, masked: &str, doc: &Document, what: &str, lombok: &[&str]) -> String {
    if let Some(name) = lombok_in(masked, lombok) {
        return format!("Lombok @{name} already generates {what}.");
    }
    if class.superclass.is_some() {
        return "This entity has a superclass, so inherited equality is unknown.".into();
    }
    if class.kotlin && words(masked, "data").any(|at| masked[at + 4..].trim_start().starts_with("class")) {
        return "Kotlin data classes already generate equality and are not offered.".into();
    }
    format!("{what} is already declared, or this source is not a plain entity with field access and trivial getters ({}).", doc.language)
}

/// `doc` is the source open in the editor. `None`, or a source that declares no persistence class, disables every item
/// that needs a class. Attribute forms additionally need the Designer's own stricter reading of the source.
pub(crate) fn palette(doc: Option<&Document>) -> Value {
    let loaded = doc
        .filter(|d| d.text.len() <= MAX_TEXT && matches!(d.language.as_str(), "java" | "kotlin"))
        .and_then(|d| parse_class(&d.text, &d.path).map(|c| (d, c)));
    let designer = loaded.as_ref().is_some_and(|(d, _)| crate::attribute_info::snapshot(d).is_some());
    let none = || Some(NO_ENTITY.to_owned());
    let attributes = {
        let class = loaded.as_ref().map(|(_, c)| c);
        let any = |ok: bool, text: &str| match class {
            None => none(),
            Some(_) if !designer => Some(NOT_IN_DESIGNER.to_owned()),
            Some(_) => reason(ok, text),
        };
        let owner = class.is_some_and(|c| c.kind != Kind::Embeddable);
        let ident = loaded.as_ref().is_some_and(|(d, _)| identifier_action::source(d).is_some());
        let ident_block = match class {
            None => none(),
            Some(c) if c.kind == Kind::Embeddable => Some("Embeddable classes have no identifier.".into()),
            Some(c) if c.has_id => Some("This class already declares an identifier.".into()),
            Some(c) if c.superclass.is_some() => Some("The identifier may be inherited from the superclass, so none is added.".into()),
            Some(_) => reason(ident, "This source is not a plain source-owned class with an unambiguous persistence namespace."),
        };
        let from_db = match &loaded {
            None => none(),
            Some((_, c)) if c.kind != Kind::Entity => Some("Only an entity maps a table.".into()),
            Some((d, _)) => reason(
                ddl_entities_action::attrs_eligible(d),
                "Needs one source-owned entity whose closing brace starts its own line and no inheritance or secondary-table mapping. The table comes from a SQL file or saved connection when the action runs.",
            ),
        };
        vec![
            attribute("association", "Association", "ManyToOne", false, any(owner, "Associations are added to entities and mapped superclasses, not to embeddables.")),
            attribute("basic", "Basic Type", "none", false, any(true, "")),
            attribute("elementCollection", "Element Collection", "ElementCollection", false, any(true, "")),
            attribute("embedded", "Embedded", "Embedded", false, any(true, "")),
            action("embeddedId", "Embedded Id", embedded_id_action::TITLE, ON_CLASS, ident_block.clone()),
            attribute("enum", "Enum", "none", true, any(true, "")),
            action("id", "Id", "Add JPA identifier with preview…", ON_CLASS, ident_block),
            attribute("transient", "Transient", "transient", false, any(true, "")),
            item("version", "Version", Some("No action adds a version field to an existing class. Tick Version field on the Create tab when creating a class.".into()), json!({})),
            action("fromDb", "From DB", ddl_entities_action::ATTRS_TITLE, ON_CLASS, from_db),
        ]
    };
    let callbacks: Vec<Value> = crate::checks::CALLBACKS
        .iter()
        .map(|event| {
            let block = loaded.as_ref().map_or_else(none, |(d, _)| {
                let masked = mask(&d.text);
                if annotations(&masked).iter().any(|a| a.name == *event) {
                    return Some(format!("@{event} is already declared."));
                }
                if declares_method(&masked, &decapitalize(event)) {
                    return Some(format!("A method named {} already exists.", decapitalize(event)));
                }
                reason(crate::owned_callback_source(d).is_some(), "Callbacks are added to a plain source-owned entity without a superclass.")
            });
            action(&format!("callback{event}"), &format!("@{event}"), &format!("Add @{event} callback"), ON_CLASS, block)
        })
        .collect();
    let entity_only = |d: &Document| index_query_action::eligible(d);
    let table_block = |what: &str| match &loaded {
        None => none(),
        Some((_, c)) if c.kind != Kind::Entity => Some(format!("{what} are added to entities only.")),
        Some((d, _)) => reason(entity_only(d), "The entity's @Table, @NamedQuery or @Index annotations are not in a form that can be extended safely."),
    };
    let utility = |what: &str, lombok: &[&str], ok: bool| match &loaded {
        None => none(),
        Some((d, c)) => reason(ok, why_not_equality(c, &mask(&d.text), d, what, lombok)),
    };
    let equals_ok = loaded.as_ref().is_some_and(|(d, _)| equality_action::eligible_equals(d));
    let string_ok = loaded.as_ref().is_some_and(|(d, _)| equality_action::eligible_to_string(d));
    json!({
        "context": {
            "loaded": loaded.is_some(),
            "designer": designer,
            "kind": loaded.as_ref().map(|(_, c)| format!("{:?}", c.kind)),
            "kotlin": loaded.as_ref().map(|(_, c)| c.kotlin),
            "lombok": loaded.as_ref().and_then(|(d, _)| lombok_in(&mask(&d.text), &LOMBOK)),
        },
        "categories": [
            {"id":"attributes","label":"Attributes","items":attributes},
            {"id":"callbacks","label":"Lifecycle Callbacks","items":callbacks},
            {"id":"indexes","label":"Indexes","items":[action("index", "Index or Unique Constraint", index_query_action::INDEX_TITLE, ON_CLASS, table_block("Indexes and unique constraints"))]},
            {"id":"queries","label":"Named Query","items":[action("namedQuery", "Named Query", index_query_action::QUERY_TITLE, ON_CLASS, table_block("Named queries"))]},
            {"id":"utilities","label":"Utilities","items":[
                action("equals", "equals() and hashCode()", equality_action::TITLE_EQUALS, ON_CLASS, utility("equals() and hashCode()", &["Data", "Value", "EqualsAndHashCode"], equals_ok)),
                action("toString", "toString()", equality_action::TITLE_TO_STRING, ON_CLASS, utility("toString()", &["Data", "Value", "ToString"], string_ok)),
            ]},
            {"id":"reverse","label":"Reverse engineering","items":[
                action("generateEntities", "Generate entities from SQL file", ddl_entities_action::TITLE, "from the editor's code actions in any source file of the project", None),
            ]},
        ],
    })
}

fn declares_method(masked: &str, name: &str) -> bool {
    words(masked, name).any(|i| masked[i + name.len()..].trim_start().starts_with('('))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ENTITY: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport java.lang.Long;\n@Entity\npublic class Person {\n    @Id\n    private Long id;\n}\n";

    fn model(text: &str, path: &str, language: &str) -> Value {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file(path.trim_start_matches("/project/"), text);
        let doc = Document { path: path.into(), text: text.into(), language: language.into(), version: 1 };
        palette(Some(&doc))
    }

    fn find<'a>(model: &'a Value, id: &str) -> &'a Value {
        model["categories"].as_array().unwrap().iter().flat_map(|c| c["items"].as_array().unwrap()).find(|i| i["id"] == id).unwrap()
    }

    #[test]
    fn without_a_source_everything_but_the_project_level_entry_waits_for_an_entity() {
        let model = palette(None);
        let labels: Vec<_> = model["categories"].as_array().unwrap().iter().map(|c| c["label"].as_str().unwrap()).collect();
        assert_eq!(labels, ["Attributes", "Lifecycle Callbacks", "Indexes", "Named Query", "Utilities", "Reverse engineering"]);
        for id in ["association", "basic", "id", "fromDb", "callbackPrePersist", "index", "namedQuery", "equals", "toString"] {
            assert_eq!(find(&model, id)["enabled"], false, "{id}");
            assert_eq!(find(&model, id)["reason"], "Open a JPA entity in the editor first.");
        }
        assert_eq!(find(&model, "generateEntities")["enabled"], true);
        assert_eq!(model["context"]["loaded"], false);
    }

    #[test]
    fn a_plain_entity_enables_attribute_wizards_and_names_the_exact_action_titles() {
        let model = model(ENTITY, "/project/src/main/java/shop/Person.java", "java");
        assert_eq!(find(&model, "association")["enabled"], true);
        assert_eq!(find(&model, "basic")["attribute"]["kind"], "none");
        assert_eq!(find(&model, "enum")["attribute"]["enumerated"], true);
        assert_eq!(find(&model, "index")["action"], "Add index or unique constraint with preview…");
        assert_eq!(find(&model, "namedQuery")["action"], "Add named query with preview…");
        assert_eq!(find(&model, "callbackPostLoad")["action"], "Add @PostLoad callback");
        assert_eq!(find(&model, "equals")["action"], "Generate proxy-safe equals and hashCode with preview…");
        assert_eq!(find(&model, "toString")["action"], "Generate lazy-safe toString with preview…");
        assert_eq!(find(&model, "fromDb")["action"], "Add attributes from database columns…");
        assert_eq!(find(&model, "version")["enabled"], false);
        assert_eq!(model["context"]["kind"], "Entity");
    }

    #[test]
    fn an_existing_identifier_a_declared_callback_and_lombok_change_what_is_offered() {
        let text = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.PrePersist;\nimport lombok.Data;\nimport java.lang.Long;\n@Data\n@Entity\npublic class Person {\n    @Id\n    private Long id;\n    @PrePersist\n    void prePersist() {}\n}\n";
        let model = model(text, "/project/src/main/java/shop/Person.java", "java");
        assert_eq!(find(&model, "id")["reason"], "This class already declares an identifier.");
        assert_eq!(find(&model, "callbackPrePersist")["reason"], "@PrePersist is already declared.");
        assert_eq!(find(&model, "callbackPreUpdate")["enabled"], false, "callbacks need persistence annotations only");
        assert_eq!(find(&model, "basic")["reason"], NOT_IN_DESIGNER);
        assert_eq!(find(&model, "equals")["enabled"], false);
        assert!(find(&model, "equals")["reason"].as_str().unwrap().contains("Lombok @Data"));
        assert_eq!(model["context"]["lombok"], "Data");
    }

    #[test]
    fn an_embeddable_has_no_association_identifier_index_or_query_items() {
        let text = "package shop;\nimport jakarta.persistence.Embeddable;\nimport java.lang.String;\n@Embeddable\npublic class Address {\n    private String city;\n}\n";
        let model = model(text, "/project/src/main/java/shop/Address.java", "java");
        for id in ["association", "id", "embeddedId", "fromDb", "index", "namedQuery"] {
            assert_eq!(find(&model, id)["enabled"], false, "{id}");
        }
        assert_eq!(find(&model, "basic")["enabled"], true);
    }

    #[test]
    fn a_kotlin_entity_keeps_the_wizards() {
        let text = "package shop\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\n@Entity\nclass Person {\n    @Id\n    var id: Long? = null\n}\n";
        let model = model(text, "/project/src/main/kotlin/shop/Person.kt", "kotlin");
        assert_eq!(model["context"]["kotlin"], true);
        assert_eq!(find(&model, "basic")["enabled"], true);
        assert_eq!(find(&model, "index")["enabled"], true);
    }
}
