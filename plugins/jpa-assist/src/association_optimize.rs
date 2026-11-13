//! Optimization choice for the association creation flows, after the advice of `association_advice`.
//!
//! When a flow introduces an association the editor inspections would flag, the user may pick optimizations that are applied to
//! the generated source before the exact preview: lazy fetching (`jpa-eager-fetch`, `jpa-eager-to-one`), `@JoinColumn` on a
//! unidirectional `@OneToMany` (no link table; the bidirectional alternative needs the other entity, which these flows do not
//! edit, except in "Create related entity" where both sides are already generated), and a `Set` for a `@ManyToMany` collection.
use super::*;
use crate::{
    checks::Finding,
    model::{Edit, import_edit, snake_case},
};
use std::collections::HashMap;

const PICK_TITLE: &str = "Optimization for the association advice";
const KEEP: &str = "keep";
const MAX_ROUNDS: usize = 4;

struct Optimization {
    label: &'static str,
    detail: String,
    edits: Vec<Edit>,
}

fn new_findings(path: &str, language: &str, before: &str, after: &str) -> Vec<Finding> {
    let mut known: HashMap<&str, usize> = HashMap::new();
    for finding in super::association_advice::findings(path, language, before) {
        *known.entry(finding.code).or_default() += 1;
    }
    super::association_advice::findings(path, language, after)
        .into_iter()
        .filter(|finding| match known.get_mut(finding.code) {
            Some(count) if *count > 0 => {
                *count -= 1;
                false
            }
            _ => true,
        })
        .collect()
}

fn namespace_of(text: &str) -> Option<&'static str> {
    match (text.contains("jakarta.persistence"), text.contains("javax.persistence")) {
        (true, false) => Some("jakarta.persistence"),
        (false, true) => Some("javax.persistence"),
        _ => None,
    }
}

fn join_column(path: &str, language: &str, after: &str, finding: &Finding) -> Option<Optimization> {
    let class = parse_class(after, path)?;
    let namespace = namespace_of(after)?;
    let kotlin = language == "kotlin";
    let line = after[..finding.start].rfind('\n').map_or(0, |at| at + 1);
    let indent: String = after[line..].chars().take_while(|ch| *ch == ' ' || *ch == '\t').collect();
    let newline = if after.contains("\r\n") { "\r\n" } else { "\n" };
    let column = format!("{}_id", snake_case(&class.name));
    let mut edits = vec![Edit::insert(line, &format!("{indent}@JoinColumn(name = \"{column}\"){newline}"))];
    edits.extend(import_edit(after, &[format!("{namespace}.JoinColumn").as_str()], kotlin));
    Some(Optimization {
        label: "Add @JoinColumn",
        detail: format!("the foreign key `{column}` lives in the many side's table instead of a link table"),
        edits,
    })
}

fn set_collection(language: &str, after: &str, finding: &Finding) -> Option<Optimization> {
    let kotlin = language == "kotlin";
    let tail = &after[finding.end..];
    let statement_end = if kotlin { tail.match_indices('\n').nth(2).map(|(at, _)| at) } else { tail.find(';') }.unwrap_or(tail.len());
    let statement = &tail[..statement_end];
    let list_at = statement.find("List<")?;
    let mutable = kotlin && statement[..list_at].ends_with("Mutable");
    let from = finding.end + list_at - if mutable { "Mutable".len() } else { 0 };
    let replaced = if mutable { "MutableList<".len() } else { "List<".len() };
    let mut edits = vec![Edit { start: from, end: from + replaced, text: if mutable { "MutableSet<".into() } else { "Set<".into() } }];
    let mut imports = Vec::new();
    if !kotlin {
        imports.push("java.util.Set");
    }
    let initializers: [(&str, &str, &str); 3] = [("new ArrayList<", "new HashSet<", "java.util.HashSet"), ("mutableListOf", "mutableSetOf", ""), ("listOf", "setOf", "")];
    for (old, new, import) in initializers {
        let Some(at) = statement.find(old) else {
            continue;
        };
        if kotlin == old.contains("new ") {
            continue;
        }
        edits.push(Edit { start: finding.end + at, end: finding.end + at + old.len(), text: new.into() });
        if !import.is_empty() {
            imports.push(import);
        }
        break;
    }
    edits.extend(import_edit(after, &imports, kotlin));
    let list_import = "import java.util.List;\n";
    if !kotlin && words(&mask(after), "List").count() == 2 {
        if let Some(at) = after.find(list_import) {
            edits.push(Edit { start: at, end: at + list_import.len(), text: String::new() });
        }
    }
    Some(Optimization { label: "Use a Set", detail: "a Set is not a bag; removing one element no longer rewrites every link row".into(), edits })
}

fn optimizations(path: &str, language: &str, before: &str, after: &str) -> Vec<Optimization> {
    new_findings(path, language, before, after)
        .into_iter()
        .filter_map(|finding| match finding.code {
            "jpa-eager-fetch" | "jpa-eager-to-one" => finding.fix.as_ref().map(|(_, edits)| Optimization {
                label: "Fetch lazily",
                detail: "fetch = FetchType.LAZY, load explicitly where needed".into(),
                edits: edits.iter().map(|edit| Edit { start: edit.start, end: edit.end, text: edit.text.clone() }).collect(),
            }),
            "jpa-unidirectional-one-to-many" => join_column(path, language, after, &finding),
            "jpa-many-to-many-list" => set_collection(language, after, &finding),
            _ => None,
        })
        .collect()
}

fn apply_edits(text: &str, mut edits: Vec<Edit>) -> Option<String> {
    edits.sort_by_key(|edit| std::cmp::Reverse((edit.start, edit.end)));
    let mut text = text.to_owned();
    let mut limit = usize::MAX;
    for edit in edits {
        if edit.end > limit || edit.end > text.len() || !text.is_char_boundary(edit.start) || !text.is_char_boundary(edit.end) {
            return None;
        }
        text.replace_range(edit.start..edit.end, &edit.text);
        limit = edit.start;
    }
    Some(text)
}

/// `after` with the optimizations the user picks, one at a time, for the advice it introduces over `before`; `after` itself
/// when there is no advice, nothing applicable, or the user keeps the mapping as generated.
pub(super) fn optimize(path: &str, language: &str, before: &str, after: &str) -> Result<String, Error> {
    let mut current = after.to_owned();
    for _ in 0..MAX_ROUNDS {
        let mut offered = optimizations(path, language, before, &current);
        if offered.is_empty() {
            break;
        }
        let mut items = vec![PickItem::new(KEEP, "Keep the mapping as generated")];
        items.extend(offered.iter().enumerate().map(|(index, found)| PickItem::new(&index.to_string(), found.label).description(&found.detail)));
        let chosen = host::pick(PICK_TITLE, &items)?.ok_or(Error::Cancelled)?;
        if chosen == KEEP {
            break;
        }
        let index = chosen.parse::<usize>().ok().filter(|index| *index < offered.len()).ok_or("Choose an offered optimization.")?;
        let optimized = apply_edits(&current, offered.swap_remove(index).edits).ok_or(Error::Stale)?;
        if parse_class(&optimized, path).is_none() {
            return Err(Error::Stale);
        }
        current = optimized;
    }
    Ok(current)
}

/// The single edit that turns `before` into `after` (common prefix and suffix are kept).
pub(super) fn rebase(before: &str, after: &str) -> Vec<TextEdit> {
    let mut prefix = before.bytes().zip(after.bytes()).take_while(|(a, b)| a == b).count();
    while !before.is_char_boundary(prefix) || !after.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let limit = before.len().min(after.len()) - prefix;
    let mut suffix = before.bytes().rev().zip(after.bytes().rev()).take(limit).take_while(|(a, b)| a == b).count();
    while !before.is_char_boundary(before.len() - suffix) || !after.is_char_boundary(after.len() - suffix) {
        suffix -= 1;
    }
    let range = Edit { start: prefix, end: before.len() - suffix, text: after[prefix..after.len() - suffix].to_owned() };
    vec![range.to_text_edit(before)]
}

/// Optimizes the edited document of a flow: the edits and the text they produce, replaced by one equivalent edit when
/// an optimization was picked.
pub(super) fn apply(doc: &Document, edits: Vec<TextEdit>, after: String) -> Result<(Vec<TextEdit>, String), Error> {
    let optimized = optimize(&doc.path, &doc.language, &doc.text, &after)?;
    if optimized == after {
        return Ok((edits, after));
    }
    Ok((rebase(&doc.text, &optimized), optimized))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const PATH: &str = "/project/src/Owner.java";
    const BASE: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.OneToMany;\nimport jakarta.persistence.ManyToOne;\nimport jakarta.persistence.ManyToMany;\nimport java.util.List;\n@Entity\npublic class OrderLine {\n    @Id Long id;\n}\n";

    fn with(member: &str) -> String {
        BASE.replace("    @Id Long id;\n", &format!("    @Id Long id;\n    {member}\n"))
    }

    fn run(member: &str, picked: &[&str]) -> String {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        for key in picked {
            host.answer_next_prompt(Some(&[*key]));
        }
        optimize(PATH, "java", BASE, &with(member)).unwrap()
    }

    #[test]
    fn a_default_eager_to_one_becomes_lazy() {
        let text = run("@ManyToOne Customer customer;", &["0"]);
        assert!(text.contains("@ManyToOne(fetch = jakarta.persistence.FetchType.LAZY)"), "{text}");
    }

    #[test]
    fn a_unidirectional_one_to_many_gets_a_join_column_named_after_the_owner() {
        let text = run("@OneToMany List<Item> items;", &["0"]);
        assert!(text.contains("    @JoinColumn(name = \"order_line_id\")\n    @OneToMany List<Item> items;"), "{text}");
        assert!(text.contains("import jakarta.persistence.JoinColumn;"), "{text}");
    }

    #[test]
    fn a_many_to_many_list_becomes_a_set_with_its_initializer_and_imports() {
        let text = run("@ManyToMany List<Tag> tags = new ArrayList<>();", &["0"]);
        assert!(text.contains("@ManyToMany Set<Tag> tags = new HashSet<>();"), "{text}");
        assert!(text.contains("import java.util.Set;") && text.contains("import java.util.HashSet;"), "{text}");
    }

    #[test]
    fn picking_nothing_or_having_no_advice_changes_nothing() {
        let member = "@ManyToOne Customer customer;";
        assert_eq!(run(member, &["keep"]), with(member));
        let host = MockHost::install().with_capabilities(&["project.read"]);
        let quiet = with("@OneToMany(mappedBy = \"owner\") List<Item> items;");
        assert_eq!(optimize(PATH, "java", BASE, &quiet).unwrap(), quiet);
        drop(host);
    }

    #[test]
    fn several_findings_are_optimized_one_round_at_a_time_until_the_user_keeps_the_rest() {
        let member = "@ManyToOne Customer customer;\n    @ManyToMany List<Tag> tags;";
        let both = run(member, &["0", "0"]);
        assert!(both.contains("FetchType.LAZY") && both.contains("Set<Tag> tags"), "{both}");
        let one = run(member, &["0", "keep"]);
        assert!(one.contains("Set<Tag> tags") && !one.contains("FetchType.LAZY"), "{one}");
    }

    #[test]
    fn the_rebased_edit_reproduces_the_optimized_text() {
        let before = with("@ManyToOne Customer customer;");
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.answer_next_prompt(Some(&["0"]));
        let after = optimize(PATH, "java", BASE, &before).unwrap();
        assert_eq!(patched(&before, &rebase(&before, &after)).unwrap(), after);
        assert_eq!(patched(BASE, &rebase(BASE, BASE)).unwrap(), BASE);
    }

    const OWNER: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.ManyToMany;\nimport java.util.List;\n@Entity\npublic class Order {\n    @Id Long id;\n    @ManyToMany\n    List<Customer> customers;\n}\n";
    const CUSTOMER_PATH: &str = "/project/src/shop/Customer.java";
    const CUSTOMER: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\npublic class Customer {\n    @Id Long id;\n}\n";

    fn open(path: &str, text: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(path, "java", text, &[]);
        (host, JpaAssist::default(), Document { path: path.into(), language: "java".into(), text: text.into(), version: 1 })
    }

    fn start(plugin: &mut JpaAssist, doc: &Document, title: &str) -> FileAction {
        plugin.file_actions(doc, Range::default()).into_iter().find(|action| action.title.starts_with(title)).unwrap()
    }

    fn feed(host: &MockHost, rows: &[&[&str]]) {
        for row in rows {
            host.answer_next_prompt(Some(row));
        }
    }

    #[test]
    fn the_add_attribute_flow_applies_the_picked_optimization_before_the_preview() {
        let (host, mut plugin, doc) = open(CUSTOMER_PATH, CUSTOMER);
        let pending = start(&mut plugin, &doc, "Add JPA attribute");
        feed(&host, &[&["tags"], &["ManyToMany"], &["Tag"], &[], &["0"], &[]]);
        let result = plugin.resolve_file_action(&doc, pending).unwrap();
        let after = patched(&doc.text, &result.edits[0].1).unwrap();
        assert!(after.contains("Set<Tag> tags") && after.contains("import java.util.Set;"), "{after}");
        assert!(!after.contains("List<Tag>") && !after.contains("import java.util.List;"), "{after}");
    }

    #[test]
    fn the_add_attribute_flow_offers_lazy_fetching_and_a_join_column() {
        let (host, mut plugin, doc) = open(CUSTOMER_PATH, CUSTOMER);
        let pending = start(&mut plugin, &doc, "Add JPA attribute");
        feed(&host, &[&["boss"], &["ManyToOne"], &["Boss"], &[], &["0"], &[]]);
        let after = patched(&doc.text, &plugin.resolve_file_action(&doc, pending).unwrap().edits[0].1).unwrap();
        assert!(after.contains("fetch = jakarta.persistence.FetchType.LAZY"), "{after}");

        let (host, mut plugin, doc) = open(CUSTOMER_PATH, CUSTOMER);
        let pending = start(&mut plugin, &doc, "Add JPA attribute");
        feed(&host, &[&["items"], &["OneToMany"], &["Item"], &[], &["0"], &[]]);
        let after = patched(&doc.text, &plugin.resolve_file_action(&doc, pending).unwrap().edits[0].1).unwrap();
        assert!(after.contains("@JoinColumn(name = \"customer_id\")") && after.contains("import jakarta.persistence.JoinColumn;"), "{after}");
    }

    #[test]
    fn the_add_inverse_flow_applies_the_picked_optimization() {
        let (host, mut plugin, doc) = open(CUSTOMER_PATH, CUSTOMER);
        host.add_file("src/shop/Order.java", OWNER);
        let pending = start(&mut plugin, &doc, "Add inverse");
        feed(&host, &[&["0"], &["0"], &[]]);
        let after = patched(&doc.text, &plugin.resolve_file_action(&doc, pending).unwrap().edits[0].1).unwrap();
        assert!(after.contains("@ManyToMany(mappedBy = \"customers\")") && after.contains("Set<Order>"), "{after}");
    }

    #[test]
    fn the_related_entity_flow_optimizes_both_the_edited_and_the_created_source() {
        let owner = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\npublic class Order {\n    @Id Long id;\n}\n";
        let (host, mut plugin, doc) = open("/project/src/shop/Order.java", owner);
        let pending = start(&mut plugin, &doc, "Create related entity (relation to)");
        feed(&host, &[&["Customer"], &["ManyToOne"], &["customer"], &["orders"], &["0"], &[]]);
        let result = plugin.resolve_file_action(&doc, pending).unwrap();
        let after = patched(&doc.text, &result.edits[0].1).unwrap();
        assert!(after.contains("fetch = jakarta.persistence.FetchType.LAZY"), "{after}");
        let [FileOperation::Create { text, .. }] = result.operations.as_slice() else { panic!("one file") };
        assert!(text.contains("@OneToMany(mappedBy = \"customer\")"), "{text}");
    }

    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_API (source scripts/jpa-compile-env.sh); compiles every optimization"]
    fn optimized_entities_compile_against_the_persistence_api() {
        let api = std::env::var("JPA_JAKARTA_API").expect("explicit API jar path");
        let dir = std::env::temp_dir().join(format!("jpa-association-optimize-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut paths = Vec::new();
        for (index, member) in ["@ManyToOne Customer customer;", "@OneToMany List<Item> items;", "@ManyToMany List<Tag> tags = new ArrayList<>();"].into_iter().enumerate() {
            let text = run(member, &["0"]).replace("class OrderLine", &format!("class OrderLine{index}")).replace("package shop;", "package shop; import java.util.ArrayList;");
            let file = dir.join(format!("OrderLine{index}.java"));
            std::fs::write(&file, text).unwrap();
            paths.push(file);
        }
        for stub in ["Customer", "Item", "Tag"] {
            let file = dir.join(format!("{stub}.java"));
            std::fs::write(&file, format!("package shop; @jakarta.persistence.Entity public class {stub} {{ @jakarta.persistence.Id Long id; }}")).unwrap();
            paths.push(file);
        }
        let result = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &api, "-d"]).arg(&dir).args(&paths).output().expect("javac installed");
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
