//! Reviewed `@NamedEntityGraph` generation in the invoking entity's document only.
//!
//! Verified against the Jakarta Persistence 3.1 Javadoc: `@NamedEntityGraph` is `@Repeatable(NamedEntityGraphs.class)`,
//! targets types, `name` defaults to the entity name, `attributeNodes`/`subgraphs` default to `{}`;
//! `@NamedAttributeNode(value, subgraph)` names a `@NamedSubgraph(name, attributeNodes)` of the same graph.
//! The Java EE 7 (JPA 2.1) Javadoc shows no `@Repeatable`, the Java EE 8 (JPA 2.2) one does, so on `javax.persistence`
//! (version unknown) a second graph is only added to an existing `@NamedEntityGraphs` container.
//! Subgraphs are offered only when the target entity is found by the same bounded lookup as repository actions.
use super::index_query_action::{self as iq, Source};
use super::*;
use crate::model::{Annotation, Attribute};

pub(super) const TITLE: &str = "Add entity graph with preview…";
const MAX_NODES: usize = 32;
const GRAPH_NAMES: [&str; 4] = ["NamedEntityGraph", "NamedEntityGraphs", "NamedAttributeNode", "NamedSubgraph"];

pub(super) fn eligible(doc: &Document) -> bool {
    source(doc).is_some()
}

fn source(doc: &Document) -> Option<Source> {
    let s = iq::source(doc)?;
    let all = annotations(&s.masked);
    for name in GRAPH_NAMES {
        if all.iter().any(|a| a.name == name) && !annotation_is(&doc.text, &s.class, name, s.ns) {
            return None;
        }
    }
    (!s.class.attributes.is_empty()).then_some(s)
}

fn graph_name_ok(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().enumerate().all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || (i > 0 && (b.is_ascii_digit() || b == b'.')))
}

fn unreadable() -> Error {
    "An existing @NamedEntityGraph is not a literal annotation with a literal name; edit manually.".into()
}

fn literal_name(doc: &Document, s: &Source, args: &[(String, iq::Span)]) -> Result<String, Error> {
    let (_, span) = args.iter().find(|(k, _)| k == "name").ok_or_else(unreadable)?;
    iq::lit(&doc.text, *span, s.class.kotlin).ok_or_else(unreadable)
}

struct Existing<'a> {
    names: Vec<String>,
    container: Option<&'a Annotation<'a>>,
    singles: Vec<&'a Annotation<'a>>,
}

fn existing<'a>(doc: &Document, s: &Source, tops: &'a [Annotation<'a>]) -> Result<Existing<'a>, Error> {
    let kotlin = s.class.kotlin;
    let mut found = Existing { names: Vec::new(), container: None, singles: Vec::new() };
    for a in tops {
        match a.name {
            "NamedEntityGraph" => {
                let (open, close) = a.args.ok_or_else(unreadable)?;
                if !iq::clean(&doc.text, (open, close)) {
                    return Err(unreadable());
                }
                let args = iq::named_args(&doc.text, &s.masked, (open, close)).ok_or_else(unreadable)?;
                found.names.push(literal_name(doc, s, &args)?);
                found.singles.push(a);
            }
            "NamedEntityGraphs" => {
                if found.container.replace(a).is_some() {
                    return Err("More than one @NamedEntityGraphs was found; edit manually.".into());
                }
                let span = iq::container_value(&doc.text, &s.masked, a).ok_or_else(unreadable)?;
                for part in iq::bracket_parts(&doc.text, &s.masked, span, kotlin).ok_or_else(unreadable)? {
                    let (_, args) = iq::parse_call(&doc.text, &s.masked, part, &["NamedEntityGraph"], s.ns, kotlin).ok_or_else(unreadable)?;
                    found.names.push(literal_name(doc, s, &args)?);
                }
            }
            "NamedAttributeNode" | "NamedSubgraph" => return Err(unreadable()),
            _ => {}
        }
    }
    Ok(found)
}

#[derive(Clone, Copy, PartialEq)]
enum Pickable {
    Scalar,
    Association,
    Unnameable,
}

fn classify(a: &Attribute) -> Pickable {
    let ident = |t: &str| !t.is_empty() && t != "Object" && t.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$');
    match a.relation {
        Relation::None | Relation::Embedded | Relation::ElementCollection => Pickable::Scalar,
        Relation::ManyToOne | Relation::OneToOne if ident(&a.target) => Pickable::Association,
        Relation::OneToMany | Relation::ManyToMany if a.ty.contains('<') && ident(&a.target) => Pickable::Association,
        _ => Pickable::Unnameable,
    }
}

struct Node {
    attribute: String,
    subgraph: Option<(String, Vec<String>)>,
}

fn pick_names(label: &str, offered: &[&Attribute], note: impl Fn(&Attribute) -> Option<String>) -> Result<Vec<String>, Error> {
    let items: Vec<_> = offered
        .iter()
        .map(|a| {
            let item = PickItem::new(&a.name, &format!("{} : {}", a.name, a.ty));
            match note(a) {
                Some(text) => item.description(&text),
                None => item,
            }
        })
        .collect();
    let chosen = host::pick_many(label, &items)?.ok_or(Error::Cancelled)?;
    let mut out: Vec<String> = Vec::new();
    for name in chosen {
        if !offered.iter().any(|a| a.name == name) {
            return Err("Choose only offered attributes of this entity; inherited attributes are not offered.".into());
        }
        if out.contains(&name) {
            return Err("Choose each attribute once.".into());
        }
        out.push(name);
    }
    if out.len() > MAX_NODES {
        return Err(format!("Choose at most {MAX_NODES} attributes.").into());
    }
    Ok(out)
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    super::current(pending, doc)?;
    let s = source(doc).ok_or(Error::Stale)?;
    let kotlin = s.class.kotlin;
    let tops = iq::top(&s.masked, s.class_at);
    let found = existing(doc, &s, &tops)?;
    if found.container.is_some() && !found.singles.is_empty() {
        return Err("Both @NamedEntityGraphs and a directly repeated @NamedEntityGraph are present; edit manually.".into());
    }
    if found.container.is_none() && !found.singles.is_empty() && s.ns == "javax.persistence" {
        return Err("Adding a second @NamedEntityGraph needs a repeatable annotation (javax.persistence 2.2+) or a @NamedEntityGraphs container; the javax.persistence version is not known and this action does not rewrite the existing annotation. Add it manually.".into());
    }
    let name = iq::input("Entity graph name", &format!("{}.detail", s.class.name))?;
    if !graph_name_ok(&name) {
        return Err("Use an ASCII graph name of at most 128 characters: letters, digits, underscore and dots, not starting with a digit or dot.".into());
    }
    if found.names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
        return Err("An entity graph with this name already exists in this file.".into());
    }
    let own: Vec<&Attribute> = s.class.attributes.iter().collect();
    let root = pick_names("Attributes of the graph root", &own, |a| match classify(a) {
        Pickable::Association => Some("association: a subgraph may be offered".into()),
        Pickable::Unnameable => Some("target type cannot be named; not allowed".into()),
        Pickable::Scalar => None,
    })?;
    if root.is_empty() {
        return Err("Choose at least one attribute.".into());
    }
    let package = s.class.package.clone();
    let mut nodes = Vec::new();
    let mut notes = String::new();
    for attribute in &root {
        let a = own.iter().find(|a| &a.name == attribute).ok_or(Error::Stale)?;
        match classify(a) {
            Pickable::Unnameable => {
                return Err(format!("The target type of {attribute} cannot be named from the source ({}); it is not added.", a.ty).into());
            }
            Pickable::Scalar => {
                if a.relation != Relation::None {
                    notes.push_str(&format!("No subgraph for {attribute}: embedded and element-collection targets are not looked up.\n"));
                }
                nodes.push(Node { attribute: attribute.clone(), subgraph: None });
            }
            Pickable::Association => {
                let target = match super::query_method_action::find_entity(project, &pending.root, &a.target, package.as_deref(), &s.class.imports) {
                    Ok((class, _)) => class,
                    Err(Error::Stale) => return Err(Error::Stale),
                    Err(error) => {
                        notes.push_str(&format!("No subgraph for {attribute}: {error}\n"));
                        nodes.push(Node { attribute: attribute.clone(), subgraph: None });
                        continue;
                    }
                };
                let offered: Vec<&Attribute> = target.attributes.iter().collect();
                let chosen = if offered.is_empty() {
                    Vec::new()
                } else {
                    pick_names(&format!("Subgraph attributes of {} (none = no subgraph)", target.name), &offered, |_| None)?
                };
                if chosen.is_empty() {
                    nodes.push(Node { attribute: attribute.clone(), subgraph: None });
                    continue;
                }
                if target.superclass.is_some() {
                    notes.push_str(&format!("{} has a superclass; its inherited attributes were not offered.\n", target.name));
                }
                nodes.push(Node { attribute: attribute.clone(), subgraph: Some((format!("{attribute}Graph"), chosen)) });
            }
        }
    }
    super::current(pending, doc)?;
    let element = render(&s, &name, &nodes, found.container.is_some());
    let edit = match found.container {
        Some(container) => {
            let span = iq::container_value(&doc.text, &s.masked, container).ok_or_else(unreadable)?;
            iq::insert_in_array(&doc.text, &s.masked, span, &element, kotlin)?
        }
        None => {
            let anchor = found
                .singles
                .last()
                .copied()
                .or_else(|| tops.iter().find(|a| a.name == "Entity"))
                .ok_or(Error::Stale)?;
            iq::insert_annotation_line(&doc.text, &s.masked, anchor, &element)
        }
    };
    let mut used = vec!["NamedEntityGraph", "NamedAttributeNode"];
    if nodes.iter().any(|n| n.subgraph.is_some()) {
        used.push("NamedSubgraph");
    }
    let mut edits = vec![edit.to_text_edit(&doc.text)];
    edits.extend(iq::imports(doc, &s, &used)?);
    notes.push_str("Attribute names come from this file's own declarations and the looked-up target entity files; mappings in XML and inherited attributes are not read.\nName uniqueness is checked in this file only.");
    iq::finish(pending, doc, format!("Add entity graph {name}"), "Add entity graph?", edits, &format!("{notes}\n"))
}

fn render(s: &Source, name: &str, nodes: &[Node], nested: bool) -> String {
    let kotlin = s.class.kotlin;
    let array = |items: Vec<String>| if kotlin { format!("[{}]", items.join(", ")) } else { format!("{{{}}}", items.join(", ")) };
    let node = |attribute: &str, subgraph: Option<&str>| match subgraph {
        Some(sub) => iq::call(s, "NamedAttributeNode", &format!("value = {}, subgraph = {}", iq::quote(attribute, kotlin), iq::quote(sub, kotlin)), true),
        None => iq::call(s, "NamedAttributeNode", &iq::quote(attribute, kotlin), true),
    };
    let roots: Vec<String> = nodes.iter().map(|n| node(&n.attribute, n.subgraph.as_ref().map(|(g, _)| g.as_str()))).collect();
    let subgraphs: Vec<String> = nodes
        .iter()
        .filter_map(|n| n.subgraph.as_ref())
        .map(|(g, attributes)| {
            let inner: Vec<String> = attributes.iter().map(|a| node(a, None)).collect();
            iq::call(s, "NamedSubgraph", &format!("name = {}, attributeNodes = {}", iq::quote(g, kotlin), array(inner)), true)
        })
        .collect();
    let mut arguments = format!("name = {}, attributeNodes = {}", iq::quote(name, kotlin), array(roots));
    if !subgraphs.is_empty() {
        arguments.push_str(&format!(", subgraphs = {}", array(subgraphs)));
    }
    iq::call(s, "NamedEntityGraph", &arguments, nested)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const PERSON: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport java.lang.Long;\nimport java.lang.String;\n@Entity\nclass Person { @Id Long id; String name; }";
    const ORDER: &str = "package shop;\n\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.OneToMany;\nimport jakarta.persistence.ManyToOne;\nimport java.util.List;\n\n@Entity\npublic class Order {\n    @Id\n    private Long id;\n    private String number;\n    @ManyToOne\n    private Customer customer;\n    @OneToMany\n    private List<Line> lines;\n    @ManyToOne\n    private Missing missing;\n}\n";
    const ORDER_KT: &str = "package shop\n\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\nimport jakarta.persistence.ManyToOne\n\n@Entity\nclass Order {\n    @Id var id: Long? = null\n    var number: String? = null\n    @ManyToOne var customer: Customer? = null\n}\n";
    const CUSTOMER: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\npublic class Customer {\n    @Id private Long id;\n    private String email;\n    private String city;\n}\n";
    const LINE: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\npublic class Line {\n    @Id private Long id;\n    private String sku;\n}\n";

    fn javax(text: &str) -> String {
        text.replace("jakarta.persistence", "javax.persistence")
    }

    fn drive(text: &str, kotlin: bool, with_targets: bool, rows: &[&[&str]]) -> Result<String, Error> {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let (dir, ext, language) = if kotlin { ("kotlin", "kt", "kotlin") } else { ("java", "java", "java") };
        let path = format!("/project/src/main/{dir}/shop/Order.{ext}");
        host.open_editor(&path, language, text, &[]);
        if with_targets {
            host.add_file("src/main/java/shop/Customer.java", CUSTOMER);
            host.add_file("src/main/java/shop/Line.java", LINE);
        }
        let doc = Document { path, language: language.into(), text: text.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).expect("offered");
        for row in rows {
            host.answer_next_prompt(Some(row));
        }
        let result = plugin.resolve_file_action(&doc, pending)?;
        assert_eq!(result.edits.len(), 1);
        assert!(result.operations.is_empty());
        assert_eq!(host.editor_text(), text);
        patched(&doc.text, &result.edits[0].1)
    }

    const FULL: &[&[&str]] = &[&["order.graph"], &["number", "customer", "lines"], &["email"], &["sku"], &[]];

    #[test]
    fn java_graph_with_subgraphs_and_imports_is_rendered_after_entity() {
        let after = drive(ORDER, false, true, FULL).unwrap();
        assert!(after.contains("@Entity\n@NamedEntityGraph(name = \"order.graph\", attributeNodes = {@NamedAttributeNode(\"number\"), @NamedAttributeNode(value = \"customer\", subgraph = \"customerGraph\"), @NamedAttributeNode(value = \"lines\", subgraph = \"linesGraph\")}, subgraphs = {@NamedSubgraph(name = \"customerGraph\", attributeNodes = {@NamedAttributeNode(\"email\")}), @NamedSubgraph(name = \"linesGraph\", attributeNodes = {@NamedAttributeNode(\"sku\")})})\npublic class Order"), "{after}");
        for name in ["NamedEntityGraph", "NamedAttributeNode", "NamedSubgraph"] {
            assert_eq!(after.matches(&format!("import jakarta.persistence.{name};")).count(), 1, "{after}");
        }
    }

    #[test]
    fn kotlin_uses_array_literals_without_at_signs_inside_and_keeps_crlf() {
        let text = ORDER_KT.replace('\n', "\r\n");
        let rows: &[&[&str]] = &[&["order.graph"], &["number", "customer"], &["city"], &[]];
        let after = drive(&text, true, true, rows).unwrap();
        assert!(after.contains("@Entity\r\n@NamedEntityGraph(name = \"order.graph\", attributeNodes = [NamedAttributeNode(\"number\"), NamedAttributeNode(value = \"customer\", subgraph = \"customerGraph\")], subgraphs = [NamedSubgraph(name = \"customerGraph\", attributeNodes = [NamedAttributeNode(\"city\")])])\r\nclass Order"), "{after:?}");
        assert!(after.contains("import jakarta.persistence.NamedSubgraph\r\n") && !after.contains("NamedSubgraph;"));
        assert!(!after.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn unresolvable_targets_get_no_subgraph_and_a_reason() {
        let rows: &[&[&str]] = &[&["g1"], &["number", "customer"], &[]];
        let after = drive(ORDER, false, false, rows).unwrap();
        assert!(after.contains("attributeNodes = {@NamedAttributeNode(\"number\"), @NamedAttributeNode(\"customer\")})") && !after.contains("NamedSubgraph"), "{after}");
        assert!(!after.contains("import jakarta.persistence.NamedSubgraph"));
    }

    #[test]
    fn empty_subgraph_choice_means_plain_node() {
        let rows: &[&[&str]] = &[&["g1"], &["customer"], &[], &[]];
        let after = drive(ORDER, false, true, rows).unwrap();
        assert!(after.contains("attributeNodes = {@NamedAttributeNode(\"customer\")})") && !after.contains("subgraph"), "{after}");
    }

    #[test]
    fn jakarta_repeats_after_existing_graph_and_merges_into_a_container() {
        let first = drive(ORDER, false, false, &[&["g1"], &["number"], &[]]).unwrap();
        let second = drive(&first, false, false, &[&["g2"], &["id"], &[]]).unwrap();
        assert_eq!(second.matches("@NamedEntityGraph(").count(), 2, "{second}");
        assert!(second.contains("@NamedAttributeNode(\"number\")})\n@NamedEntityGraph(name = \"g2\""), "{second}");
        let container = ORDER.replace("@Entity\n", "@Entity\n@NamedEntityGraphs({@NamedEntityGraph(name = \"old\", attributeNodes = {@NamedAttributeNode(\"id\")})})\n").replace("import jakarta.persistence.Entity;", "import jakarta.persistence.Entity;\nimport jakarta.persistence.NamedAttributeNode;\nimport jakarta.persistence.NamedEntityGraph;\nimport jakarta.persistence.NamedEntityGraphs;");
        let after = drive(&container, false, false, &[&["new"], &["number"], &[]]).unwrap();
        assert!(after.contains("@NamedAttributeNode(\"id\")}), @NamedEntityGraph(name = \"new\", attributeNodes = {@NamedAttributeNode(\"number\")})})"), "{after}");
        assert_eq!(after.matches("@NamedEntityGraphs").count(), 1);
        assert_eq!(after.matches("import jakarta.persistence.NamedEntityGraph;").count(), 1);
    }

    #[test]
    fn javax_first_graph_is_added_but_a_second_needs_a_container() {
        let first = drive(&javax(ORDER), false, false, &[&["g1"], &["number"], &[]]).unwrap();
        assert!(first.contains("@NamedEntityGraph(name = \"g1\""));
        let err = drive(&first, false, false, &[]).unwrap_err().to_string();
        assert!(err.contains("repeatable") && err.contains("javax"), "{err}");
        let container = javax(ORDER).replace("@Entity\n", "@Entity\n@NamedEntityGraphs({@NamedEntityGraph(name = \"old\", attributeNodes = {@NamedAttributeNode(\"id\")})})\n").replace("import javax.persistence.Entity;", "import javax.persistence.Entity;\nimport javax.persistence.NamedAttributeNode;\nimport javax.persistence.NamedEntityGraph;\nimport javax.persistence.NamedEntityGraphs;");
        assert!(drive(&container, false, false, &[&["new"], &["number"], &[]]).is_ok());
    }

    #[test]
    fn kotlin_container_merge_uses_unprefixed_element() {
        let container = ORDER_KT.replace("@Entity\n", "@Entity\n@NamedEntityGraphs([NamedEntityGraph(name = \"old\", attributeNodes = [NamedAttributeNode(\"id\")])])\n").replace("import jakarta.persistence.Entity\n", "import jakarta.persistence.Entity\nimport jakarta.persistence.NamedAttributeNode\nimport jakarta.persistence.NamedEntityGraph\nimport jakarta.persistence.NamedEntityGraphs\n");
        let after = drive(&container, true, false, &[&["new"], &["number"], &[]]).unwrap();
        assert!(after.contains("NamedAttributeNode(\"id\")]), NamedEntityGraph(name = \"new\", attributeNodes = [NamedAttributeNode(\"number\")])])"), "{after}");
    }

    #[test]
    fn refusals_never_guess() {
        let err = |text: &str, rows: &[&[&str]]| drive(text, false, true, rows).unwrap_err().to_string();
        for bad in ["", "1x", ".a", "a b", "a\"b", "é"] {
            assert!(err(ORDER, &[&[bad]]).contains("graph name"), "{bad}");
        }
        let with = |graph: &str| ORDER.replace("@Entity\n", &format!("@Entity\n{graph}\n")).replace("import jakarta.persistence.Entity;", "import jakarta.persistence.Entity;\nimport jakarta.persistence.NamedAttributeNode;\nimport jakarta.persistence.NamedEntityGraph;");
        assert!(err(&with("@NamedEntityGraph(name = \"Dup\", attributeNodes = {@NamedAttributeNode(\"id\")})"), &[&["dup"]]).contains("already exists"));
        for hostile in ["@NamedEntityGraph(name = NAME)", "@NamedEntityGraph", "@NamedEntityGraph(name = \"a\" + \"b\")", "@NamedEntityGraph(name = \"a\" /* c */)"] {
            assert!(err(&with(hostile), &[&["x"]]).contains("literal"), "{hostile}");
        }
        assert!(err(&ORDER.replace("@Entity\n", "@Entity\n@NamedEntityGraphs(GRAPHS)\n").replace("import jakarta.persistence.Entity;", "import jakarta.persistence.Entity;\nimport jakarta.persistence.NamedEntityGraphs;"), &[&["x"]]).contains("literal"));
        let mixed = ORDER.replace("@Entity\n", "@Entity\n@NamedEntityGraphs({@NamedEntityGraph(name = \"a\", attributeNodes = {})})\n@NamedEntityGraph(name = \"b\", attributeNodes = {})\n").replace("import jakarta.persistence.Entity;", "import jakarta.persistence.Entity;\nimport jakarta.persistence.NamedEntityGraph;\nimport jakarta.persistence.NamedEntityGraphs;");
        assert!(err(&mixed, &[&["x"]]).contains("Both"));
        assert!(err(ORDER, &[&["x"], &[]]).contains("at least one"));
        assert!(err(ORDER, &[&["x"], &["number", "number"]]).contains("once"));
                assert!(err(ORDER, &[&["x"], &[]]).contains("at least one"));
        let raw = ORDER.replace("List<Line>", "List");
        assert!(err(&raw, &[&["x"], &["lines"]]).contains("cannot be named"));
        let object = ORDER.replace("private Customer customer", "private Object customer");
        assert!(err(&object, &[&["x"], &["customer"]]).contains("cannot be named"));
        let clash = ORDER.replace("import java.util.List;", "import java.util.List;\nimport other.NamedSubgraph;");
        assert!(err(&clash, &[&["x"], &["customer"], &["email"]]).contains("already refers"));
    }

    #[test]
    fn not_offered_for_foreign_namespaces_non_entities_and_inherited_entities() {
        let offered = |text: &str| {
            let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
            host.open_editor("/project/src/main/java/shop/Order.java", "java", text, &[]);
            let doc = Document { path: "/project/src/main/java/shop/Order.java".into(), language: "java".into(), text: text.into(), version: 1 };
            JpaAssist::default().file_actions(&doc, Range::default()).iter().any(|a| a.title == TITLE)
        };
        assert!(offered(ORDER));
        assert!(!offered(&ORDER.replace("@Entity", "@Embeddable")));
        assert!(!offered(&ORDER.replace("public class Order", "public class Order extends Base")));
        let foreign = ORDER.replace("@Entity\n", "@Entity\n@NamedEntityGraph(name = \"a\")\n").replace("import jakarta.persistence.Entity;", "import jakarta.persistence.Entity;\nimport vendor.NamedEntityGraph;");
        assert!(!offered(&foreign));
    }

    #[test]
    fn cancel_and_drift_do_not_apply() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let path = "/project/src/main/java/shop/Order.java";
        host.open_editor(path, "java", ORDER, &[]);
        let doc = Document { path: path.into(), language: "java".into(), text: ORDER.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)));
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.type_in_editor("// changed");
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
    }

    #[test]
    fn generated_scalar_graph_passes_local_graph_diagnostics_and_a_typo_would_not() {
        let host = MockHost::install();
        let path = "/project/src/main/java/shop/Person.java";
        host.open_editor(path, "java", PERSON, &[]);
        let doc = Document { path: path.into(), language: "java".into(), text: PERSON.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).expect("offered");
        host.answer_next_prompt(Some(&["person.summary"]));
        host.answer_next_prompt(Some(&["id", "name"]));
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&doc, pending).unwrap();
        let after = patched(&doc.text, &result.edits[0].1).unwrap();
        let generated = Document { text: after.clone(), version: 2, ..doc.clone() };
        assert!(crate::local_graph::source(&generated).is_some(), "local graph must accept the generated entity: {after}");
        assert!(crate::local_graph::findings(&generated).is_empty());
        let typo = Document { text: after.replace("@NamedAttributeNode(\"name\")", "@NamedAttributeNode(\"nme\")"), ..generated };
        assert_eq!(crate::local_graph::findings(&typo).len(), 1);
    }

    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_API / JPA_JAVAX_API (and Kotlin env vars)"]
    fn generated_entity_graph_sources_compile_for_java_kotlin_and_both_namespaces() {
        let base = std::env::temp_dir().join(format!("jpa-entity-graph-compile-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        for kotlin in [false, true] {
            for ns in ["jakarta.persistence", "javax.persistence"] {
                let seed = match (kotlin, ns == "jakarta.persistence") {
                    (false, true) => ORDER.to_owned(),
                    (false, false) => javax(ORDER),
                    (true, true) => ORDER_KT.to_owned(),
                    (true, false) => javax(ORDER_KT),
                };
                let rows: &[&[&str]] = if kotlin { &[&["order.graph"], &["number", "customer"], &["city"], &[]] } else { FULL };
                let first = drive(&seed, kotlin, true, rows).unwrap();
                let source = if ns == "jakarta.persistence" {
                    drive(&first, kotlin, true, &[&["second"], &["number"], &[]]).unwrap()
                } else {
                    first
                };
                let directory = base.join(format!("{}-{ns}", if kotlin { "kotlin" } else { "java" }));
                std::fs::create_dir_all(&directory).unwrap();
                let jar = std::env::var(if ns == "jakarta.persistence" { "JPA_JAKARTA_API" } else { "JPA_JAVAX_API" }).expect("explicit persistence API jar");
                let mut support = Vec::new();
                for (file, text) in [("Customer.java", CUSTOMER), ("Line.java", LINE)] {
                    let path = directory.join(file);
                    std::fs::write(&path, if ns == "jakarta.persistence" { text.to_owned() } else { javax(text) }).unwrap();
                    support.push(path);
                }
                let owner = directory.join(if kotlin { "Order.kt" } else { "Order.java" });
                std::fs::write(&owner, &source).unwrap();
                std::fs::write(directory.join("Missing.java"), "package shop;\npublic class Missing {}\n").unwrap();
                support.push(directory.join("Missing.java"));
                let mut javac = std::process::Command::new("javac");
                javac.args(["--release", "17", "-proc:none", "-classpath", &jar, "-d"]).arg(&directory).args(&support);
                if !kotlin {
                    javac.arg(&owner);
                }
                let result = javac.output().expect("javac installed");
                assert!(result.status.success(), "java {ns}: {}", String::from_utf8_lossy(&result.stderr));
                if kotlin {
                    let compiler = std::env::var("KOTLIN_COMPILER_CLASSPATH").expect("Kotlin compiler classpath");
                    let stdlib = std::env::var("KOTLIN_STDLIB").expect("Kotlin stdlib");
                    let classpath = std::env::join_paths([std::path::Path::new(&jar), std::path::Path::new(&stdlib), &directory]).unwrap();
                    let result = std::process::Command::new("java")
                        .args(["-classpath", &compiler, "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler", "-no-stdlib", "-no-reflect", "-jvm-target", "17", "-classpath"])
                        .arg(classpath)
                        .arg("-d")
                        .arg(&directory)
                        .arg(&owner)
                        .output()
                        .expect("java installed");
                    assert!(result.status.success(), "kotlin {ns}: {}\n{}", String::from_utf8_lossy(&result.stderr), String::from_utf8_lossy(&result.stdout));
                }
            }
        }
        std::fs::remove_dir_all(base).unwrap();
        eprintln!("Compiled 4 generated entities with named entity graphs: Java/Kotlin x Jakarta/javax.");
    }
}
