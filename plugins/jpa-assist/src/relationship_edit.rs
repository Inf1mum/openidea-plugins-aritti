//! Edits to the literal arguments of one existing relationship annotation. Unknown arguments,
//! comments and the declaration itself survive exactly; nothing is inferred beyond this source.
use crate::{
    designer::locate,
    generate::callback_import,
    model::{self, Relation},
    wizard::{JoinTable, RelationOptions, column_name, order_by_items},
};
use openidea_plugin::{Error, Range, TextEdit, providers::Document, text::position};
use persistence_core::SourceEdit;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "property", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum RelationshipChange {
    Fetch { value: String },
    OrphanRemoval { value: bool },
    Optional { value: bool },
    Cascade { value: Vec<String> },
    JoinColumn { name: String, nullable: bool },
    #[serde(rename_all = "camelCase")]
    JoinTable { name: String, join_column: String, inverse_column: String },
    OrderBy { value: String },
    OrderColumn { value: String },
}

impl RelationshipChange {
    fn key(&self) -> &'static str {
        match self {
            Self::Fetch { .. } => "fetch",
            Self::OrphanRemoval { .. } => "orphanRemoval",
            Self::Optional { .. } => "optional",
            Self::Cascade { .. } => "cascade",
            Self::JoinColumn { .. } => "joinColumn",
            Self::JoinTable { .. } => "joinTable",
            Self::OrderBy { .. } => "orderBy",
            Self::OrderColumn { .. } => "orderColumn",
        }
    }
}

struct Target {
    class: model::Class,
    masked: String,
    annotation: model::Annotation<'static>,
    relation: Relation,
    start: usize,
    end: usize,
}

fn relation_name(relation: Relation) -> Option<&'static str> {
    Some(match relation {
        Relation::ManyToOne => "ManyToOne",
        Relation::OneToOne => "OneToOne",
        Relation::OneToMany => "OneToMany",
        Relation::ManyToMany => "ManyToMany",
        _ => return None,
    })
}

fn plain(value: &str) -> bool {
    value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'{' | b'}' | b'[' | b']' | b',' | b' '))
}

fn last_segment(value: &str) -> &str {
    value.rsplit('.').next().unwrap_or(value).trim()
}

const CASCADES: [&str; 6] = ["ALL", "PERSIST", "MERGE", "REMOVE", "REFRESH", "DETACH"];

fn parse_cascade(raw: &str) -> Option<Vec<String>> {
    let inner = raw
        .strip_prefix('{')
        .and_then(|r| r.strip_suffix('}'))
        .or_else(|| raw.strip_prefix('[').and_then(|r| r.strip_suffix(']')))
        .unwrap_or(raw);
    let mut out = Vec::new();
    for part in inner.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let kind = last_segment(part);
        if !CASCADES.contains(&kind) {
            return None;
        }
        out.push(kind.to_owned());
    }
    Some(out)
}

fn target(doc: &Document, name: &str) -> Result<Option<Target>, Error> {
    let (class, masked, index, start, end) = locate(doc, name)?;
    let relation = class.attributes[index].relation;
    let Some(wanted) = relation_name(relation) else {
        return Ok(None);
    };
    let mut found = model::annotations(&masked)
        .into_iter()
        .filter(|a| start <= a.start && a.start < end && matches!(a.name, "ManyToOne" | "OneToOne" | "OneToMany" | "ManyToMany"))
        .map(|a| model::Annotation { name: wanted, start: a.start, name_end: a.name_end, args: a.args })
        .collect::<Vec<_>>();
    let annotation = match found.len() {
        1 => found.remove(0),
        _ => return Err("This attribute needs exactly one relationship annotation to be edited.".into()),
    };
    if !masked[annotation.start + 1..annotation.name_end].ends_with(wanted) {
        return Err("Multiple relationship annotations cannot be edited.".into());
    }
    Ok(Some(Target { class, masked, annotation, relation, start, end }))
}

enum Slot<T> {
    Absent,
    Present(model::Annotation<'static>, T),
    Unresolved,
}

/// Sibling annotations of the attribute itself; annotations nested in another annotation's arguments do not count.
fn siblings(target: &Target, wanted: &'static str) -> Vec<model::Annotation<'static>> {
    let all = model::annotations(&target.masked);
    all.iter()
        .filter(|a| target.start <= a.start && a.start < target.end && a.name == wanted)
        .filter(|a| !all.iter().any(|o| o.args.is_some_and(|(s, e)| s <= a.start && a.start < e)))
        .map(|a| model::Annotation { name: wanted, start: a.start, name_end: a.name_end, args: a.args })
        .collect()
}

fn slot<T>(
    target: &Target,
    wanted: &'static str,
    parse: impl Fn(&model::Annotation) -> Option<T>,
) -> Slot<T> {
    let mut found = siblings(target, wanted);
    match found.len() {
        0 => Slot::Absent,
        1 => {
            let annotation = found.remove(0);
            match parse(&annotation) {
                Some(value) => Slot::Present(annotation, value),
                None => Slot::Unresolved,
            }
        }
        _ => Slot::Unresolved,
    }
}

fn only_named_arguments(target: &Target, a: &model::Annotation) -> bool {
    let Some((open, close)) = a.args else {
        return true;
    };
    model::split_top_level(&target.masked[open..close])
        .iter()
        .all(|(_, part)| part.trim().is_empty() || part.contains('='))
}

/// `Some(None)` when the argument is absent, `None` when it is not a plain literal without escapes.
fn string_arg(doc: &Document, target: &Target, a: &model::Annotation, key: &str) -> Option<Option<String>> {
    let Some(range) = model::value_range(&doc.text, &target.masked, a, key) else {
        return Some(None);
    };
    let (_, inner) = model::plain_literal(&doc.text, range, target.class.kotlin)?;
    (!inner.contains('\\')).then(|| Some(inner.to_owned()))
}

fn column_arg(doc: &Document, target: &Target, a: &model::Annotation, key: &str) -> Option<String> {
    match string_arg(doc, target, a, key)? {
        Some(name) => column_name(&name).then_some(name),
        None => Some(String::new()),
    }
}

fn parse_join_column(doc: &Document, target: &Target, a: &model::Annotation) -> Option<(String, bool)> {
    if !only_named_arguments(target, a) {
        return None;
    }
    let name = column_arg(doc, target, a, "name")?;
    let nullable = match model::arg(&doc.text, &target.masked, a, "nullable") {
        None | Some("true") => true,
        Some("false") => false,
        Some(_) => return None,
    };
    Some((name, nullable))
}

fn nested_column(raw: &str) -> Option<String> {
    let mut rest = raw.trim();
    if let Some(inner) = rest
        .strip_prefix('{')
        .and_then(|r| r.strip_suffix('}'))
        .or_else(|| rest.strip_prefix('[').and_then(|r| r.strip_suffix(']')))
    {
        rest = inner.trim();
    }
    let rest = rest.strip_prefix('@').unwrap_or(rest);
    let rest = rest.strip_prefix("JoinColumn")?.trim_start().strip_prefix('(')?.strip_suffix(')')?.trim();
    let rest = rest.strip_prefix("name")?.trim_start().strip_prefix('=')?.trim();
    let name = rest.strip_prefix('"')?.strip_suffix('"')?;
    column_name(name).then(|| name.to_owned())
}

fn parse_join_table(doc: &Document, target: &Target, a: &model::Annotation) -> Option<(String, String, String)> {
    if !only_named_arguments(target, a) {
        return None;
    }
    let column = |key: &str| match model::arg(&doc.text, &target.masked, a, key) {
        None => Some(String::new()),
        Some(raw) => nested_column(raw),
    };
    Some((column_arg(doc, target, a, "name")?, column("joinColumns")?, column("inverseJoinColumns")?))
}

fn parse_order_by(doc: &Document, target: &Target, a: &model::Annotation) -> Option<String> {
    let value = match a.args {
        Some((open, close)) if !doc.text[open..close].trim().is_empty() => string_arg(doc, target, a, "value")??,
        _ => String::new(),
    };
    order_by_items(&value).map(|_| value)
}

fn parse_order_column(doc: &Document, target: &Target, a: &model::Annotation) -> Option<String> {
    only_named_arguments(target, a).then(|| column_arg(doc, target, a, "name")).flatten()
}

fn sibling_properties(doc: &Document, target: &Target, mapped_by: bool) -> Vec<Value> {
    let unresolved = |key: &str, label: &str| property(key, label, json!(null), "unresolved", false);
    let mut out = Vec::new();

    if target.relation.to_one() && !mapped_by {
        let composite = !siblings(target, "JoinColumns").is_empty();
        out.push(match slot(target, "JoinColumn", |a| parse_join_column(doc, target, a)) {
            Slot::Absent if !composite => {
                property("joinColumn", "Join column", json!({"name": "", "nullable": true}), "annotation-default", true)
            }
            Slot::Present(_, (name, nullable)) if !composite => {
                property("joinColumn", "Join column", json!({"name": name, "nullable": nullable}), "explicit", true)
            }
            _ => unresolved("joinColumn", "Join column"),
        });
    }
    if target.relation == Relation::ManyToMany && !mapped_by {
        out.push(match slot(target, "JoinTable", |a| parse_join_table(doc, target, a)) {
            Slot::Absent => property(
                "joinTable",
                "Join table",
                json!({"name": "", "joinColumn": "", "inverseColumn": ""}),
                "annotation-default",
                true,
            ),
            Slot::Present(_, (name, join, inverse)) => property(
                "joinTable",
                "Join table",
                json!({"name": name, "joinColumn": join, "inverseColumn": inverse}),
                "explicit",
                true,
            ),
            Slot::Unresolved => unresolved("joinTable", "Join table"),
        });
    }
    if target.relation.to_many() {
        let both = !siblings(target, "OrderBy").is_empty() && !siblings(target, "OrderColumn").is_empty();
        let ordered = |key: &str, label: &str, state: Slot<String>| match state {
            Slot::Absent if !both => property(key, label, json!(""), "annotation-default", true),
            Slot::Present(_, value) if !both => property(key, label, json!(value), "explicit", true),
            _ => unresolved(key, label),
        };
        out.push(ordered("orderBy", "Order by", slot(target, "OrderBy", |a| parse_order_by(doc, target, a))));
        if !mapped_by {
            out.push(ordered("orderColumn", "Order column", slot(target, "OrderColumn", |a| parse_order_column(doc, target, a))));
        }
    }
    out
}

fn property(key: &str, label: &str, value: Value, origin: &str, editable: bool) -> Value {
    json!({"key": key, "label": label, "value": value, "origin": origin, "editable": editable})
}

fn flag(raw: Option<&str>, key: &str, label: &str, default: bool) -> Value {
    match raw {
        Some("true") => property(key, label, json!(true), "explicit", true),
        Some("false") => property(key, label, json!(false), "explicit", true),
        Some(_) => property(key, label, json!(null), "unresolved", false),
        None => property(key, label, json!(default), "annotation-default", true),
    }
}

/// Editable relationship properties of the selected attribute; `Ok(None)` for non-relationships.
pub(crate) fn properties(doc: &Document, name: &str) -> Result<Option<Vec<Value>>, Error> {
    let Some(target) = target(doc, name)? else {
        return Ok(None);
    };
    let raw = |key: &str| model::arg(&doc.text, &target.masked, &target.annotation, key);
    let to_one = target.relation.to_one();
    let mut out = Vec::new();

    out.push(match raw("fetch").map(last_segment) {
        Some(f @ ("LAZY" | "EAGER")) => property("fetch", "Fetch", json!(f), "explicit", true),
        Some(_) => property("fetch", "Fetch", json!(null), "unresolved", false),
        None => property("fetch", "Fetch", json!(if to_one { "EAGER" } else { "LAZY" }), "annotation-default", true),
    });
    if matches!(target.relation, Relation::OneToOne | Relation::OneToMany) {
        out.push(flag(raw("orphanRemoval"), "orphanRemoval", "Orphan removal", false));
    }
    if to_one {
        out.push(flag(raw("optional"), "optional", "Optional", true));
    }
    out.push(match raw("cascade") {
        Some(value) => match parse_cascade(value) {
            Some(kinds) => property("cascade", "Cascade", json!(kinds), "explicit", true),
            None => property("cascade", "Cascade", json!(null), "unresolved", false),
        },
        None => property("cascade", "Cascade", json!([]), "annotation-default", true),
    });
    let mapped_by = model::arg(&doc.text, &target.masked, &target.annotation, "mappedBy").is_some();
    out.extend(sibling_properties(doc, &target, mapped_by));
    Ok(Some(out))
}

fn args_edits(
    doc: &Document,
    masked: &str,
    a: &model::Annotation,
    pairs: &[(&str, String)],
    guard_plain: bool,
) -> Result<Vec<SourceEdit>, Error> {
    let mut edits = Vec::new();
    let mut additions = Vec::new();
    for (key, literal) in pairs {
        match model::value_range(&doc.text, masked, a, key) {
            Some((from, to)) => {
                if guard_plain && !plain(&doc.text[from..to]) {
                    return Err("An existing argument uses an expression that cannot be edited safely.".into());
                }
                edits.push(SourceEdit { start: from, end: to, replacement: literal.clone() });
            }
            None => additions.push(format!("{key} = {literal}")),
        }
    }
    if additions.is_empty() {
        return Ok(edits);
    }
    match a.args {
        Some((open, close)) => {
            let mut keys = HashSet::new();
            for (_, part) in model::split_top_level(&masked[open..close]) {
                if part.trim().is_empty() {
                    continue;
                }
                let (key, _) = part.split_once('=').ok_or("Unsupported relationship argument syntax.")?;
                if !keys.insert(key.trim()) {
                    return Err("Duplicate relationship arguments cannot be edited.".into());
                }
            }
            let separator = if doc.text[open..close].trim().is_empty() { "" } else { ", " };
            edits.push(SourceEdit { start: close, end: close, replacement: format!("{separator}{}", additions.join(", ")) });
        }
        None => edits.push(SourceEdit {
            start: a.name_end,
            end: a.name_end,
            replacement: format!("({})", additions.join(", ")),
        }),
    }
    Ok(edits)
}

fn quoted(value: &str) -> String {
    format!("\"{value}\"")
}

fn nested_join_column(name: &str, kotlin: bool) -> String {
    match kotlin {
        true => format!("[JoinColumn(name = \"{name}\")]"),
        false => format!("@JoinColumn(name = \"{name}\")"),
    }
}

struct SiblingEdits {
    edits: Vec<SourceEdit>,
    new_annotations: Vec<String>,
}

fn sibling_change(
    doc: &Document,
    target: &Target,
    change: &RelationshipChange,
    options: &mut RelationOptions,
    imports: &mut Vec<String>,
    out: &mut SiblingEdits,
) -> Result<(), Error> {
    let class = &target.class;
    let kotlin = class.kotlin;
    let unsafe_existing = || Error::from("The existing annotation cannot be edited safely.");
    let mut pairs: Vec<(&str, String)> = Vec::new();
    let existing: model::Annotation;

    match change {
        RelationshipChange::JoinColumn { name, nullable } => {
            options.join_column = Some(name.clone());
            options.join_column_not_null = !nullable;
            match slot(target, "JoinColumn", |a| parse_join_column(doc, target, a)) {
                Slot::Absent => {
                    imports.push(callback_import(class, "JoinColumn"));
                    let nullability = if *nullable { "" } else { ", nullable = false" };
                    out.new_annotations.push(format!("@JoinColumn(name = \"{name}\"{nullability})"));
                    return Ok(());
                }
                Slot::Present(a, (old_name, old_nullable)) => {
                    if *name != old_name {
                        pairs.push(("name", quoted(name)));
                    }
                    if *nullable != old_nullable {
                        pairs.push(("nullable", nullable.to_string()));
                    }
                    existing = a;
                }
                Slot::Unresolved => return Err(unsafe_existing()),
            }
        }
        RelationshipChange::JoinTable { name, join_column, inverse_column } => {
            options.join_table = Some(JoinTable {
                name: name.clone(),
                join_column: join_column.clone(),
                inverse_column: inverse_column.clone(),
            });
            imports.push(callback_import(class, "JoinColumn"));
            match slot(target, "JoinTable", |a| parse_join_table(doc, target, a)) {
                Slot::Absent => {
                    imports.push(callback_import(class, "JoinTable"));
                    out.new_annotations.push(format!(
                        "@JoinTable(name = \"{name}\", joinColumns = {}, inverseJoinColumns = {})",
                        nested_join_column(join_column, kotlin),
                        nested_join_column(inverse_column, kotlin),
                    ));
                    return Ok(());
                }
                Slot::Present(a, (old_name, old_join, old_inverse)) => {
                    if *name != old_name {
                        pairs.push(("name", quoted(name)));
                    }
                    if *join_column != old_join {
                        pairs.push(("joinColumns", nested_join_column(join_column, kotlin)));
                    }
                    if *inverse_column != old_inverse {
                        pairs.push(("inverseJoinColumns", nested_join_column(inverse_column, kotlin)));
                    }
                    existing = a;
                }
                Slot::Unresolved => return Err(unsafe_existing()),
            }
        }
        RelationshipChange::OrderBy { value } | RelationshipChange::OrderColumn { value } => {
            let by = matches!(change, RelationshipChange::OrderBy { .. });
            let (this, other, key) = if by { ("OrderBy", "OrderColumn", "value") } else { ("OrderColumn", "OrderBy", "name") };
            if by {
                options.order_by = Some(value.clone());
            } else {
                options.order_column = Some(value.clone());
            }
            if !siblings(target, other).is_empty() {
                return Err("Choose either @OrderBy or @OrderColumn, not both.".into());
            }
            let normalized = match by {
                true => order_by_items(value)
                    .map(|items| items.join(", "))
                    .ok_or("Order by takes up to four property names, each optionally followed by ASC or DESC.")?,
                false => value.clone(),
            };
            let parsed = slot(target, this, |a| match by {
                true => parse_order_by(doc, target, a),
                false => parse_order_column(doc, target, a),
            });
            match parsed {
                Slot::Absent => {
                    imports.push(callback_import(class, this));
                    out.new_annotations.push(match normalized.is_empty() {
                        true => format!("@{this}"),
                        false if by => format!("@{this}(\"{normalized}\")"),
                        false => format!("@{this}(name = \"{normalized}\")"),
                    });
                    return Ok(());
                }
                Slot::Present(a, old) => {
                    if normalized == old {
                        return Ok(());
                    }
                    if normalized.is_empty() {
                        return Err("Removing an ordering annotation is not supported here.".into());
                    }
                    pairs.push((key, quoted(&normalized)));
                    existing = a;
                }
                Slot::Unresolved => return Err(unsafe_existing()),
            }
        }
        _ => return Ok(()),
    }
    out.edits.extend(args_edits(doc, &target.masked, &existing, &pairs, false)?);
    Ok(())
}

fn sibling_insertion(doc: &Document, target: &Target, texts: &[String]) -> SourceEdit {
    let a = &target.annotation;
    let end = a.end();
    let line_start = doc.text[..a.start].rfind('\n').map_or(0, |i| i + 1);
    let indent: String = doc.text[line_start..a.start].chars().take_while(|c| matches!(c, ' ' | '\t')).collect();
    let newline = if doc.text.contains("\r\n") { "\r\n" } else { "\n" };
    match target.masked[end..].find('\n') {
        Some(i) if target.masked[end..end + i].trim().is_empty() => SourceEdit {
            start: end + i + 1,
            end: end + i + 1,
            replacement: texts.iter().map(|t| format!("{indent}{t}{newline}")).collect(),
        },
        _ => SourceEdit { start: end, end, replacement: texts.iter().map(|t| format!(" {t}")).collect() },
    }
}

/// Replaces or appends only the changed arguments of the one relationship annotation.
pub(crate) fn prepare(
    doc: &Document,
    name: &str,
    changes: &[RelationshipChange],
) -> Result<(Vec<TextEdit>, String), Error> {
    if changes.is_empty() || changes.len() > 8 {
        return Err("Change one or more supported relationship properties.".into());
    }
    let target = target(doc, name)?.ok_or("Select a relationship attribute.")?;
    let supported = properties(doc, name)?.unwrap_or_default();
    let a = &target.annotation;
    let kotlin = target.class.kotlin;

    let mut seen = HashSet::new();
    let mut options = RelationOptions::default();
    let mut pairs: Vec<(&str, String)> = Vec::new();
    let mut sibling_edits = SiblingEdits { edits: Vec::new(), new_annotations: Vec::new() };
    let mut imports: Vec<String> = Vec::new();
    for change in changes {
        let key = change.key();
        let editable = supported.iter().any(|p| p["key"] == key && p["editable"] == true);
        if !seen.insert(key) || !editable {
            return Err("A property is duplicated or cannot be edited in this source context.".into());
        }
        let literal = match change {
            RelationshipChange::Fetch { value } => {
                options.fetch = Some(value.clone());
                imports.push(callback_import(&target.class, "FetchType"));
                format!("FetchType.{value}")
            }
            RelationshipChange::OrphanRemoval { value } => {
                options.orphan_removal = *value;
                value.to_string()
            }
            RelationshipChange::Optional { value } => value.to_string(),
            RelationshipChange::Cascade { value } => {
                options.cascade = value.clone();
                if !value.is_empty() {
                    imports.push(callback_import(&target.class, "CascadeType"));
                }
                let kinds: Vec<String> = value.iter().map(|k| format!("CascadeType.{k}")).collect();
                match (kinds.as_slice(), kotlin) {
                    ([], true) => "[]".into(),
                    ([], false) => "{}".into(),
                    (_, true) => format!("[{}]", kinds.join(", ")),
                    ([one], false) => one.clone(),
                    (_, false) => format!("{{{}}}", kinds.join(", ")),
                }
            }
            _ => {
                sibling_change(doc, &target, change, &mut options, &mut imports, &mut sibling_edits)?;
                continue;
            }
        };
        pairs.push((key, literal));
    }
    let mapped_by = model::arg(&doc.text, &target.masked, a, "mappedBy").is_some();
    options.validate(target.relation, false, mapped_by).map_err(Error::from)?;

    let mut edits = args_edits(doc, &target.masked, a, &pairs, true)?;
    edits.extend(sibling_edits.edits);
    if !sibling_edits.new_annotations.is_empty() {
        let insertion = sibling_insertion(doc, &target, &sibling_edits.new_annotations);
        match edits.iter_mut().find(|e| e.start == insertion.start && e.end == insertion.end) {
            Some(existing) => existing.replacement.push_str(&insertion.replacement),
            None => edits.push(insertion),
        }
    }
    let refs: Vec<&str> = imports.iter().map(String::as_str).collect();
    if let Some(import) = model::import_edit(&doc.text, &refs, kotlin) {
        let text = if doc.text.contains("\r\n") { import.text.replace('\n', "\r\n") } else { import.text };
        edits.push(SourceEdit { start: import.start, end: import.end, replacement: text });
    }
    let after = persistence_core::apply_edits(&doc.text, &edits).map_err(Error::from)?;
    let updated = Document { text: after.clone(), ..doc.clone() };
    crate::file_actions::checked_after(&updated, &[])?;
    Ok((
        edits
            .iter()
            .map(|e| TextEdit {
                range: Range::new(position(&doc.text, e.start), position(&doc.text, e.end)),
                text: e.replacement.clone(),
            })
            .collect(),
        after,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const PATH: &str = "/project/src/main/java/shop/Order.java";
    const SOURCE: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.ManyToOne;\nimport jakarta.persistence.OneToMany;\nimport jakarta.persistence.CascadeType;\nimport java.util.List;\n@Entity\nclass Order {\n    @Id long id;\n    // 😀 keep\n    @ManyToOne(optional = false)\n    private Customer customer;\n    @OneToMany(mappedBy = \"order\", cascade = CascadeType.ALL)\n    private List<Line> lines;\n}\n";

    fn setup(source: &str) -> (MockHost, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/Order.java", source);
        let doc = Document { path: PATH.into(), language: "java".into(), text: source.into(), version: 1 };
        (host, doc)
    }

    fn value<'a>(props: &'a [Value], key: &str) -> &'a Value {
        &props.iter().find(|p| p["key"] == key).unwrap()["value"]
    }

    #[test]
    fn properties_report_explicit_values_defaults_and_applicable_keys() {
        let (_host, doc) = setup(SOURCE);
        let customer = properties(&doc, "customer").unwrap().unwrap();
        assert_eq!(value(&customer, "optional"), &json!(false));
        assert_eq!(value(&customer, "fetch"), &json!("EAGER"));
        assert!(customer.iter().all(|p| p["key"] != "orphanRemoval"));
        let lines = properties(&doc, "lines").unwrap().unwrap();
        assert_eq!(value(&lines, "cascade"), &json!(["ALL"]));
        assert_eq!(value(&lines, "fetch"), &json!("LAZY"));
        assert!(lines.iter().all(|p| p["key"] != "optional"));
        assert!(properties(&doc, "id").unwrap().is_none());
    }

    #[test]
    fn changes_replace_or_append_only_the_selected_arguments_and_add_imports() {
        let (_host, doc) = setup(SOURCE);
        let (_, after) = prepare(&doc, "customer", &[
            RelationshipChange::Fetch { value: "LAZY".into() },
            RelationshipChange::Optional { value: true },
        ])
        .unwrap();
        assert!(after.contains("@ManyToOne(optional = true, fetch = FetchType.LAZY)"), "{after}");
        assert!(after.contains("import jakarta.persistence.FetchType;") && after.contains("// 😀 keep"));
        let (_, after) = prepare(&doc, "lines", &[
            RelationshipChange::Cascade { value: vec!["PERSIST".into(), "MERGE".into()] },
            RelationshipChange::OrphanRemoval { value: true },
        ])
        .unwrap();
        assert!(after.contains("@OneToMany(mappedBy = \"order\", cascade = {CascadeType.PERSIST, CascadeType.MERGE}, orphanRemoval = true)"), "{after}");
        let (_, after) = prepare(&doc, "lines", &[RelationshipChange::Cascade { value: vec![] }]).unwrap();
        assert!(after.contains("cascade = {}"), "{after}");
    }

    #[test]
    fn invalid_inapplicable_duplicate_and_unsafe_changes_are_refused() {
        let (_host, doc) = setup(SOURCE);
        for (name, changes) in [
            ("customer", vec![RelationshipChange::OrphanRemoval { value: true }]),
            ("lines", vec![RelationshipChange::Optional { value: false }]),
            ("customer", vec![RelationshipChange::Fetch { value: "NOW".into() }]),
            ("lines", vec![RelationshipChange::Cascade { value: vec!["ALL".into(), "MERGE".into()] }]),
            ("lines", vec![RelationshipChange::Cascade { value: vec!["MERGE".into(), "MERGE".into()] }]),
            ("lines", vec![RelationshipChange::OrphanRemoval { value: true }, RelationshipChange::OrphanRemoval { value: false }]),
            ("id", vec![RelationshipChange::Fetch { value: "LAZY".into() }]),
            ("lines", vec![]),
        ] {
            assert!(prepare(&doc, name, &changes).is_err(), "{name} {changes:?}");
        }
        let computed = SOURCE.replace("cascade = CascadeType.ALL", "cascade = pick(CascadeType.ALL)");
        let (_host, doc) = setup(&computed);
        assert!(properties(&doc, "lines").unwrap().unwrap().iter().any(|p| p["key"] == "cascade" && p["editable"] == false));
        assert!(prepare(&doc, "lines", &[RelationshipChange::Cascade { value: vec!["MERGE".into()] }]).is_err());
    }

    #[test]
    fn kotlin_cascade_uses_array_literals_and_crlf_imports_survive() {
        let source = "package shop\r\nimport jakarta.persistence.Entity\r\nimport jakarta.persistence.Id\r\nimport jakarta.persistence.OneToMany\r\n@Entity\r\nopen class Order {\r\n    @Id var id: Long? = null\r\n    @OneToMany\r\n    var lines: MutableList<Line> = mutableListOf()\r\n}\r\n";
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/kotlin/shop/Order.kt", source);
        let doc = Document { path: "/project/src/main/kotlin/shop/Order.kt".into(), language: "kotlin".into(), text: source.into(), version: 1 };
        let (_, after) = prepare(&doc, "lines", &[RelationshipChange::Cascade { value: vec!["PERSIST".into()] }]).unwrap();
        assert!(after.contains("@OneToMany(cascade = [CascadeType.PERSIST])"), "{after}");
        assert!(after.contains("import jakarta.persistence.CascadeType\r\n") && !after.replace("\r\n", "").contains('\n'), "{after:?}");
    }

    const MAPPING: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.JoinColumn;\nimport jakarta.persistence.JoinTable;\nimport jakarta.persistence.ManyToMany;\nimport jakarta.persistence.ManyToOne;\nimport jakarta.persistence.OneToMany;\nimport jakarta.persistence.OrderBy;\nimport java.util.List;\n@Entity\nclass Order {\n    @Id long id;\n    @ManyToOne\n    private Customer plain;\n    @ManyToOne\n    // 😀 keep\n    @JoinColumn(name = \"customer_id\", referencedColumnName = \"id\") // trailing\n    private Customer joined;\n    @ManyToMany\n    @JoinTable(name = \"order_tag\", schema = \"s\", joinColumns = @JoinColumn(name = \"order_id\"), inverseJoinColumns = {@JoinColumn(name = \"tag_id\")})\n    @OrderBy(\"name DESC\")\n    private List<Tag> tags;\n    @OneToMany(mappedBy = \"order\") @OrderBy\n    private List<Line> lines;\n    @ManyToMany(mappedBy = \"orders\")\n    private List<Tag> backTags;\n}\n";

    fn join_column(name: &str, nullable: bool) -> RelationshipChange {
        RelationshipChange::JoinColumn { name: name.into(), nullable }
    }

    fn join_table(name: &str, join: &str, inverse: &str) -> RelationshipChange {
        RelationshipChange::JoinTable { name: name.into(), join_column: join.into(), inverse_column: inverse.into() }
    }

    #[test]
    fn sibling_properties_report_values_and_hide_inapplicable_keys() {
        let (_host, doc) = setup(MAPPING);
        let keys = |name: &str| -> Vec<String> {
            properties(&doc, name).unwrap().unwrap().iter().map(|p| p["key"].as_str().unwrap().to_owned()).filter(|k| k.starts_with("join") || k.starts_with("order") && k != "orphanRemoval").collect()
        };
        let plain = properties(&doc, "plain").unwrap().unwrap();
        assert_eq!(value(&plain, "joinColumn"), &json!({"name": "", "nullable": true}));
        let joined = properties(&doc, "joined").unwrap().unwrap();
        assert_eq!(value(&joined, "joinColumn"), &json!({"name": "customer_id", "nullable": true}));
        let tags = properties(&doc, "tags").unwrap().unwrap();
        assert_eq!(value(&tags, "joinTable"), &json!({"name": "order_tag", "joinColumn": "order_id", "inverseColumn": "tag_id"}));
        assert_eq!(value(&tags, "orderBy"), &json!("name DESC"));
        assert_eq!(value(&tags, "orderColumn"), &json!(""));
        assert_eq!(keys("plain"), ["joinColumn"]);
        assert_eq!(keys("tags"), ["joinTable", "orderBy", "orderColumn"]);
        assert_eq!(keys("lines"), ["orderBy"]);
        assert_eq!(keys("backTags"), ["orderBy"]);
        assert_eq!(value(&properties(&doc, "lines").unwrap().unwrap(), "orderBy"), &json!(""));
    }

    #[test]
    fn join_column_edits_existing_arguments_and_adds_a_new_annotation_with_its_import() {
        let (_host, doc) = setup(MAPPING);
        let (_, after) = prepare(&doc, "joined", &[join_column("buyer_id", false)]).unwrap();
        assert!(after.contains("@JoinColumn(name = \"buyer_id\", referencedColumnName = \"id\", nullable = false) // trailing"), "{after}");
        assert!(after.contains("// 😀 keep"));
        let (_, after) = prepare(&doc, "plain", &[join_column("customer_id", false)]).unwrap();
        assert!(after.contains("    @ManyToOne\n    @JoinColumn(name = \"customer_id\", nullable = false)\n    private Customer plain;"), "{after}");
        let (_, after) = prepare(&doc, "plain", &[join_column("customer_id", true), RelationshipChange::Fetch { value: "LAZY".into() }]).unwrap();
        assert!(after.contains("@ManyToOne(fetch = FetchType.LAZY)\n    @JoinColumn(name = \"customer_id\")\n"), "{after}");
        let bare = MAPPING.replace("import jakarta.persistence.JoinColumn;\n", "").replace("    @JoinColumn(name = \"customer_id\", referencedColumnName = \"id\") // trailing\n", "").replace("    @JoinTable(name = \"order_tag\", schema = \"s\", joinColumns = @JoinColumn(name = \"order_id\"), inverseJoinColumns = {@JoinColumn(name = \"tag_id\")})\n", "").replace("import jakarta.persistence.JoinTable;\n", "");
        let (_host, doc) = setup(&bare);
        let (_, after) = prepare(&doc, "plain", &[join_column("c", true)]).unwrap();
        assert!(after.contains("import jakarta.persistence.JoinColumn;"), "{after}");
        let inline = MAPPING.replace("@ManyToOne\n    private Customer plain;", "@ManyToOne private Customer plain;");
        let (_host, doc) = setup(&inline);
        let (_, after) = prepare(&doc, "plain", &[join_column("c", true), RelationshipChange::Optional { value: false }]).unwrap();
        assert!(after.contains("@ManyToOne(optional = false) @JoinColumn(name = \"c\") private Customer plain;"), "{after}");
    }

    #[test]
    fn join_table_edits_only_the_changed_arguments_and_renders_new_tables() {
        let (_host, doc) = setup(MAPPING);
        let (_, after) = prepare(&doc, "tags", &[join_table("order_tags", "order_id", "label_id")]).unwrap();
        assert!(after.contains("@JoinTable(name = \"order_tags\", schema = \"s\", joinColumns = @JoinColumn(name = \"order_id\"), inverseJoinColumns = @JoinColumn(name = \"label_id\"))"), "{after}");
        let none = MAPPING.replace("    @JoinTable(name = \"order_tag\", schema = \"s\", joinColumns = @JoinColumn(name = \"order_id\"), inverseJoinColumns = {@JoinColumn(name = \"tag_id\")})\n", "");
        let (_host, doc) = setup(&none);
        let (_, after) = prepare(&doc, "tags", &[join_table("order_tag", "order_id", "tag_id")]).unwrap();
        assert!(after.contains("@ManyToMany\n    @JoinTable(name = \"order_tag\", joinColumns = @JoinColumn(name = \"order_id\"), inverseJoinColumns = @JoinColumn(name = \"tag_id\"))\n    @OrderBy"), "{after}");
    }

    #[test]
    fn ordering_edits_replace_or_add_annotations_and_never_combine() {
        let (_host, doc) = setup(MAPPING);
        let (_, after) = prepare(&doc, "tags", &[RelationshipChange::OrderBy { value: "name  ASC ,id".into() }]).unwrap();
        assert!(after.contains("@OrderBy(\"name ASC, id\")\n    private List<Tag> tags;"), "{after}");
        let (_, after) = prepare(&doc, "lines", &[RelationshipChange::OrderBy { value: "position".into() }]).unwrap();
        assert!(after.contains("@OrderBy(value = \"position\")\n    private List<Line> lines;"), "{after}");
        let plain = MAPPING.replace("    @OrderBy(\"name DESC\")\n", "");
        let (_host, doc) = setup(&plain);
        let (_, after) = prepare(&doc, "tags", &[RelationshipChange::OrderColumn { value: "tag_index".into() }]).unwrap();
        assert!(after.contains("@ManyToMany\n    @OrderColumn(name = \"tag_index\")\n    @JoinTable("), "{after}");
        assert!(after.contains("import jakarta.persistence.OrderColumn;"), "{after}");
        let (_, after) = prepare(&doc, "tags", &[RelationshipChange::OrderColumn { value: String::new() }]).unwrap();
        assert!(after.contains("@ManyToMany\n    @OrderColumn\n    @JoinTable("), "{after}");
        let column = MAPPING.replace("    @OrderBy(\"name DESC\")\n", "    @OrderColumn(name = \"a\", nullable = false)\n").replace("import jakarta.persistence.OrderBy;", "import jakarta.persistence.OrderBy;\nimport jakarta.persistence.OrderColumn;");
        let (_host, doc) = setup(&column);
        let (_, after) = prepare(&doc, "tags", &[RelationshipChange::OrderColumn { value: "b".into() }]).unwrap();
        assert!(after.contains("@OrderColumn(name = \"b\", nullable = false)"), "{after}");
    }

    #[test]
    fn join_and_ordering_changes_are_refused_when_unsafe_or_inapplicable() {
        let (_host, doc) = setup(MAPPING);
        for (name, changes) in [
            ("backTags", vec![join_table("t", "a", "b")]),
            ("backTags", vec![RelationshipChange::OrderColumn { value: "i".into() }]),
            ("lines", vec![RelationshipChange::OrderColumn { value: "i".into() }]),
            ("lines", vec![join_column("c", true)]),
            ("plain", vec![join_table("t", "a", "b")]),
            ("plain", vec![RelationshipChange::OrderBy { value: "id".into() }]),
            ("tags", vec![join_column("c", true)]),
            ("tags", vec![RelationshipChange::OrderColumn { value: "i".into() }]),
            ("tags", vec![RelationshipChange::OrderBy { value: String::new() }]),
            ("tags", vec![RelationshipChange::OrderBy { value: "a b c".into() }]),
            ("tags", vec![join_table("t", "same", "same")]),
            ("plain", vec![join_column("bad name", true)]),
            ("plain", vec![join_column("", false)]),
            ("plain", vec![join_column("a", true), join_column("b", true)]),
        ] {
            assert!(prepare(&doc, name, &changes).is_err(), "{name} {changes:?}");
        }
        let (_host, doc) = setup(&MAPPING.replace("    @OrderBy(\"name DESC\")\n", ""));
        assert!(prepare(&doc, "tags", &[RelationshipChange::OrderBy { value: "id".into() }, RelationshipChange::OrderColumn { value: "i".into() }]).is_err());
        for source in [
            MAPPING.replace("name = \"customer_id\", referencedColumnName", "name = COL, referencedColumnName"),
            MAPPING.replace("nullable", "nullable").replace("name = \"customer_id\"", "name = \"customer_id\", nullable = flag()"),
            MAPPING.replace("    private Customer plain;", "    @JoinColumn(name = \"a\")\n    @JoinColumn(name = \"b\")\n    private Customer plain;"),
            MAPPING.replace("    private Customer plain;", "    @JoinColumns({@JoinColumn(name = \"a\"), @JoinColumn(name = \"b\")})\n    private Customer plain;").replace("import java.util.List;", "import jakarta.persistence.JoinColumns;\nimport java.util.List;"),
        ] {
            let (_host, doc) = setup(&source);
            let target = if source.contains("COL") || source.contains("flag()") { "joined" } else { "plain" };
            let props = properties(&doc, target).unwrap().unwrap();
            assert!(props.iter().any(|p| p["key"] == "joinColumn" && p["editable"] == false), "{target}");
            assert!(prepare(&doc, target, &[join_column("x", true)]).is_err());
        }
        let both = MAPPING.replace("    @OrderBy(\"name DESC\")\n", "    @OrderBy(\"name\")\n    @OrderColumn\n").replace("import java.util.List;", "import jakarta.persistence.OrderColumn;\nimport java.util.List;");
        let (_host, doc) = setup(&both);
        let tags = properties(&doc, "tags").unwrap().unwrap();
        assert!(tags.iter().filter(|p| p["key"] == "orderBy" || p["key"] == "orderColumn").all(|p| p["editable"] == false));
        let computed = MAPPING.replace("joinColumns = @JoinColumn(name = \"order_id\")", "joinColumns = cols()");
        let (_host, doc) = setup(&computed);
        assert!(prepare(&doc, "tags", &[join_table("t", "a", "b")]).is_err());
    }

    #[test]
    fn kotlin_and_crlf_sibling_annotations_render_native_syntax() {
        let source = "package shop\r\nimport jakarta.persistence.Entity\r\nimport jakarta.persistence.Id\r\nimport jakarta.persistence.ManyToMany\r\nimport jakarta.persistence.ManyToOne\r\n@Entity\r\nopen class Order {\r\n    @Id var id: Long? = null\r\n    @ManyToOne\r\n    var customer: Customer? = null\r\n    @ManyToMany\r\n    var tags: MutableList<Tag> = mutableListOf()\r\n}\r\n";
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/kotlin/shop/Order.kt", source);
        let doc = Document { path: "/project/src/main/kotlin/shop/Order.kt".into(), language: "kotlin".into(), text: source.into(), version: 1 };
        let (_, after) = prepare(&doc, "customer", &[join_column("customer_id", false)]).unwrap();
        assert!(after.contains("    @ManyToOne\r\n    @JoinColumn(name = \"customer_id\", nullable = false)\r\n    var customer"), "{after:?}");
        assert!(after.contains("import jakarta.persistence.JoinColumn\r\n") && !after.replace("\r\n", "").contains('\n'), "{after:?}");
        let (_, after) = prepare(&doc, "tags", &[join_table("order_tag", "order_id", "tag_id"), RelationshipChange::OrderBy { value: "name".into() }]).unwrap();
        assert!(after.contains("@JoinTable(name = \"order_tag\", joinColumns = [JoinColumn(name = \"order_id\")], inverseJoinColumns = [JoinColumn(name = \"tag_id\")])\r\n    @OrderBy(\"name\")\r\n    var tags"), "{after:?}");
        assert!(!after.replace("\r\n", "").contains('\n'), "{after:?}");
        let existing = after.clone();
        let doc = Document { text: existing, ..doc };
        let (_, after) = prepare(&doc, "tags", &[join_table("order_tag", "order_id", "label_id")]).unwrap();
        assert!(after.contains("inverseJoinColumns = [JoinColumn(name = \"label_id\")])"), "{after:?}");
    }
}
