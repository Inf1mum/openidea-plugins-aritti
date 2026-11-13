//! Designer inspector: existing `@Table` indexes / unique constraints and existing `@NamedQuery` entries of the open
//! entity, listed and edited in place (G006-C1). Only the literal values of known arguments are replaced; every other
//! argument, comment and annotation survives byte for byte. An entry whose shape is not a plain literal call stays
//! read-only with a note. The edit goes through the Designer's prepare/apply flow, so Apply stays version-checked.
use super::index_query_action::{Span, bracket_parts, container_value, lit, named_args, parse_call, query_sanity, quote, source, top};
use super::*;
use crate::model::Edit;
use serde::Deserialize;
use serde_json::{Value, json};

const MAX_QUERY: usize = 4096;
const MAX_COLUMNS: usize = 8;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AnnotationEdit {
    id: String,
    name: Option<String>,
    columns: Option<Vec<String>>,
    unique: Option<bool>,
    query: Option<String>,
}

#[derive(Default, Clone)]
struct Spans {
    name: Option<Span>,
    columns: Option<Span>,
    unique: Option<Span>,
    query: Option<Span>,
    open: usize,
    last_end: usize,
}

struct Entry {
    id: String,
    kind: &'static str,
    name: Option<String>,
    columns: Vec<String>,
    unique: bool,
    query: Option<String>,
    note: Option<&'static str>,
    spans: Option<Spans>,
}

struct Scan {
    entries: Vec<Entry>,
    kotlin: bool,
    note: Option<&'static str>,
}

const UNREADABLE: &str = "Not a plain literal annotation (computed values, escapes, comments or a text block); edit it in the source.";

fn entry(id: String, kind: &'static str) -> Entry {
    Entry { id, kind, name: None, columns: Vec::new(), unique: false, query: None, note: None, spans: None }
}

fn call_entry(doc: &Document, s: &super::index_query_action::Source, mut e: Entry, args: &[(String, Span)], open: usize) -> Entry {
    let kotlin = s.class.kotlin;
    let find = |key: &str| args.iter().find(|(k, _)| k == key).map(|(_, sp)| *sp);
    let mut spans = Spans { open, last_end: args.iter().map(|(_, sp)| sp.1).max().unwrap_or(open), ..Spans::default() };
    let mut readable = true;
    if let Some(sp) = find("name") {
        match lit(&doc.text, sp, kotlin) {
            Some(v) => {
                e.name = Some(v);
                spans.name = Some(sp);
            }
            None => readable = false,
        }
    }
    match e.kind {
        "index" => match find("columnList").and_then(|sp| lit(&doc.text, sp, kotlin).map(|v| (sp, v))) {
            Some((sp, v)) => {
                e.columns = v.split(',').map(|c| c.split_whitespace().next().unwrap_or("").to_owned()).collect();
                spans.columns = Some(sp);
            }
            None => readable = false,
        },
        "unique" => {
                        let list = find("columnNames").and_then(|sp| {
                let parts = if doc.text[sp.0..sp.1].starts_with('"') { Some(vec![sp]) } else { string_array(&doc.text, &s.masked, sp, kotlin) };
                let names: Option<Vec<String>> = parts?.into_iter().map(|p| lit(&doc.text, p, kotlin)).collect();
                Some((sp, names?))
            });
            match list {
                Some((sp, names)) => {
                    e.columns = names;
                    spans.columns = Some(sp);
                }
                None => readable = false,
            }
        }
        _ => match (find("query").and_then(|sp| lit(&doc.text, sp, kotlin).map(|v| (sp, v))), e.name.is_some()) {
            (Some((sp, v)), true) => {
                e.query = Some(v);
                spans.query = Some(sp);
            }
            _ => readable = false,
        },
    }
    if e.kind == "index" {
        match find("unique") {
            Some(sp) => match &doc.text[sp.0..sp.1] {
                "true" => {
                    e.unique = true;
                    spans.unique = Some(sp);
                }
                "false" => spans.unique = Some(sp),
                _ => readable = false,
            },
            None => {}
        }
    }
    if readable && !args.is_empty() {
        e.spans = Some(spans);
    } else {
        e.note = Some(UNREADABLE);
    }
    e
}

/// `bracket_parts` tests emptiness on the masked text, where a string literal is blank; this one tests the real text.
fn string_array(text: &str, masked: &str, span: Span, kotlin: bool) -> Option<Vec<Span>> {
    let (open, close) = if kotlin { (b'[', b']') } else { (b'{', b'}') };
    if span.1 < span.0 + 2 || masked.as_bytes()[span.0] != open || masked.as_bytes()[span.1 - 1] != close || !super::index_query_action::clean(text, span) {
        return None;
    }
    let inner = (span.0 + 1, span.1 - 1);
    let pieces = crate::model::split_top_level(&masked[inner.0..inner.1]);
    let mut parts = Vec::new();
    for (index, (off, piece)) in pieces.iter().enumerate() {
        let (from, to) = (inner.0 + off, inner.0 + off + piece.len());
        let lead = text[from..to].len() - text[from..to].trim_start().len();
        if text[from..to].trim().is_empty() {
            if index + 1 == pieces.len() && index > 0 {
                break;
            }
            return None;
        }
        parts.push((from + lead, from + lead + text[from..to].trim().len()));
    }
    Some(parts)
}

fn scan(doc: &Document) -> Option<Scan> {
    let s = source(doc)?;
    let kotlin = s.class.kotlin;
    let tops = top(&s.masked, s.class_at);
    let mut entries = Vec::new();
    let mut note = None;
    let (mut indexes, mut uniques, mut queries) = (0, 0, 0);
    let tables: Vec<_> = tops.iter().filter(|a| a.name == "Table").collect();
    if tables.len() > 1 {
        note = Some("More than one @Table was found; indexes are not listed.");
    }
    if let [table] = tables.as_slice()
        && let Some(open_close) = table.args
    {
        match named_args(&doc.text, &s.masked, open_close) {
            Some(args) => {
                for (key, call, kind) in [("indexes", "Index", "index"), ("uniqueConstraints", "UniqueConstraint", "unique")] {
                    let Some((_, span)) = args.iter().find(|(k, _)| k == key) else { continue };
                    let Some(parts) = bracket_parts(&doc.text, &s.masked, *span, kotlin) else {
                        note = Some("An @Table array is not a plain literal; its entries are not listed.");
                        continue;
                    };
                    for part in parts {
                        let counter = if kind == "index" { &mut indexes } else { &mut uniques };
                        let e = entry(format!("{kind}:{counter}"), kind);
                        *counter += 1;
                        let at = s.masked[part.0..part.1].find('(').map(|i| part.0 + i + 1);
                        entries.push(match (parse_call(&doc.text, &s.masked, part, &[call], s.ns, kotlin), at) {
                            (Some((_, args)), Some(at)) => call_entry(doc, &s, e, &args, at),
                            _ => Entry { note: Some(UNREADABLE), ..e },
                        });
                    }
                }
            }
            None => note = Some("The @Table arguments are not plain literals; its entries are not listed."),
        }
    }
    for a in tops.iter().filter(|a| matches!(a.name, "NamedQuery" | "NamedQueries")) {
        let mut push = |parsed: Option<(Vec<(String, Span)>, usize)>| {
            let e = entry(format!("query:{queries}"), "query");
            queries += 1;
            entries.push(match parsed {
                Some((args, at)) => call_entry(doc, &s, e, &args, at),
                None => Entry { note: Some(UNREADABLE), ..e },
            });
        };
        if a.name == "NamedQuery" {
            push(a.args.filter(|sp| super::index_query_action::clean(&doc.text, *sp)).and_then(|sp| named_args(&doc.text, &s.masked, sp).map(|args| (args, sp.0))));
            continue;
        }
        let Some(parts) = container_value(&doc.text, &s.masked, a).and_then(|sp| bracket_parts(&doc.text, &s.masked, sp, kotlin)) else {
            note = Some("An @NamedQueries container is not a plain literal array; its queries are not listed.");
            continue;
        };
        for part in parts {
            let at = s.masked[part.0..part.1].find('(').map(|i| part.0 + i + 1);
            push(parse_call(&doc.text, &s.masked, part, &["NamedQuery"], s.ns, kotlin).zip(at).map(|((_, args), at)| (args, at)));
        }
    }
    Some(Scan { entries, kotlin, note })
}

/// The inspector section model. `None` source (not an entity, foreign annotations) lists nothing and says why.
pub(crate) fn list(doc: &Document) -> Value {
    let Some(scan) = scan(doc) else {
        return json!({"indexes":[],"queries":[],"note":"Indexes and named queries are listed for entities whose @Table, @NamedQuery and @Index annotations are persistence annotations."});
    };
    let row = |e: &Entry| json!({"id":e.id,"kind":e.kind,"name":e.name,"columns":e.columns,"unique":e.unique,"query":e.query,"editable":e.spans.is_some(),"note":e.note});
    json!({
        "indexes": scan.entries.iter().filter(|e| e.kind != "query").map(row).collect::<Vec<_>>(),
        "queries": scan.entries.iter().filter(|e| e.kind == "query").map(row).collect::<Vec<_>>(),
        "note": scan.note,
    })
}

fn sql_name(value: &str) -> bool {
    !value.is_empty() && value.len() <= 63 && value.is_ascii() && value.bytes().enumerate().all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || (i > 0 && b.is_ascii_digit()))
}

fn query_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().enumerate().all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || (i > 0 && (b.is_ascii_digit() || b == b'.')))
}

fn replace(span: Span, text: String) -> Edit {
    Edit { start: span.0, end: span.1, text }
}

/// Text edits (plus the resulting source) that change the selected entry. Refuses unknown ids, read-only entries,
/// fields that do not apply to the kind, invalid or duplicate names and an edit that changes nothing.
pub(crate) fn prepare(doc: &Document, request: &AnnotationEdit) -> Result<(Vec<TextEdit>, String), Error> {
    let scan = scan(doc).ok_or("Inspect an entity whose @Table and @NamedQuery annotations are plain persistence annotations.")?;
    let kotlin = scan.kotlin;
    let target = scan.entries.iter().find(|e| e.id == request.id).ok_or(Error::Stale)?;
    let spans = target.spans.clone().ok_or_else(|| Error::from(target.note.unwrap_or(UNREADABLE)))?;
    let applies = match target.kind {
        "index" => request.query.is_none(),
        "unique" => request.query.is_none() && request.unique.is_none(),
        _ => request.columns.is_none() && request.unique.is_none(),
    };
    if !applies {
        return Err("That property does not exist on this entry.".into());
    }
    let mut edits = Vec::new();
    let others = |kind_query: bool| scan.entries.iter().filter(move |e| e.id != target.id && (e.kind == "query") == kind_query).filter_map(|e| e.name.as_deref());
    if let Some(name) = request.name.as_deref().filter(|n| Some(*n) != target.name.as_deref()) {
        if target.kind == "query" {
            if !query_name(name) {
                return Err("Use an ASCII query name of at most 128 characters: letters, digits, underscore and dots, not starting with a digit or dot.".into());
            }
        } else if !sql_name(name) {
            return Err("Use an ASCII name of at most 63 characters: letters, digits and underscore, not starting with a digit.".into());
        }
        if others(target.kind == "query").any(|n| n == name) {
            return Err("Another entry in this file already has this name.".into());
        }
        edits.push(match spans.name {
            Some(span) => replace(span, quote(name, kotlin)),
            None => Edit::insert(spans.open, &format!("name = {}, ", quote(name, kotlin))),
        });
    }
    if let Some(columns) = request.columns.as_ref().filter(|c| **c != target.columns) {
        if columns.is_empty() || columns.len() > MAX_COLUMNS || !columns.iter().all(|c| sql_name(c)) {
            return Err(format!("Give 1 to {MAX_COLUMNS} column names: ASCII letters, digits and underscore, not starting with a digit.").into());
        }
        let span = spans.columns.ok_or(Error::Stale)?;
        let literal = if target.kind == "index" {
            quote(&columns.join(", "), kotlin)
        } else {
            let items: Vec<String> = columns.iter().map(|c| quote(c, kotlin)).collect();
            match (items.as_slice(), doc.text[span.0..span.1].starts_with('"')) {
                ([one], true) => one.clone(),
                _ if kotlin => format!("[{}]", items.join(", ")),
                _ => format!("{{{}}}", items.join(", ")),
            }
        };
        edits.push(replace(span, literal));
    }
    if let Some(unique) = request.unique.filter(|u| *u != target.unique) {
        edits.push(match (spans.unique, unique) {
            (Some(span), _) => replace(span, unique.to_string()),
            (None, true) => Edit::insert(spans.last_end, ", unique = true"),
            (None, false) => return Err("Nothing to change.".into()),
        });
    }
    if let Some(query) = request.query.as_deref().filter(|q| Some(*q) != target.query.as_deref()) {
        if query.trim().is_empty() || query.len() > MAX_QUERY || query.chars().any(char::is_control) {
            return Err(format!("The query must be non-blank, single-line text of at most {MAX_QUERY} bytes.").into());
        }
        if query.contains(['"', '\\']) || (kotlin && query.contains('$')) {
            return Err("Write JPQL string literals with single quotes; double quotes, backslashes and Kotlin dollar signs would need escapes, which the inspector does not edit.".into());
        }
        query_sanity(query)?;
        edits.push(replace(spans.query.ok_or(Error::Stale)?, quote(query, kotlin)));
    }
    if edits.is_empty() {
        return Err("Nothing changed.".into());
    }
    edits.sort_by_key(|e| e.start);
    let text_edits: Vec<TextEdit> = edits.iter().map(|e| e.to_text_edit(&doc.text)).collect();
    let after = crate::file_actions::checked_after(doc, &text_edits)?;
    let reread = Document { text: after.clone(), ..doc.clone() };
    let stable = scan_again(&reread, &scan.entries, &request.id);
    if !stable {
        return Err("The edit would change the shape of the annotations; nothing was prepared.".into());
    }
    Ok((text_edits, after))
}

fn scan_again(doc: &Document, before: &[Entry], id: &str) -> bool {
    scan(doc).is_some_and(|after| after.entries.len() == before.len() && after.entries.iter().find(|e| e.id == id).is_some_and(|e| e.spans.is_some()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const JAVA: &str = "package shop;\n\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.Index;\nimport jakarta.persistence.NamedQuery;\nimport jakarta.persistence.Table;\nimport jakarta.persistence.UniqueConstraint;\n\n@Entity\n@Table(name = \"invoices\", indexes = {@Index(name = \"ix_no\", columnList = \"invoice_no\"), @Index(columnList = \"a, b DESC\", unique = true)}, uniqueConstraints = {@UniqueConstraint(columnNames = {\"x\", \"y\"})})\n@NamedQuery(name = \"Invoice.all\", query = \"select i from Invoice i\", lockMode = jakarta.persistence.LockModeType.NONE)\n@NamedQuery(name = \"Invoice.dyn\", query = \"select i from \" + \"Invoice i\")\npublic class Invoice {\n    @Id\n    private Long id;\n}\n";
    const KOTLIN: &str = "package shop\n\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Index\nimport jakarta.persistence.NamedQuery\nimport jakarta.persistence.NamedQueries\nimport jakarta.persistence.Table\n\n@Entity\n@Table(name = \"invoices\", indexes = [Index(columnList = \"no\")])\n@NamedQueries([NamedQuery(name = \"Invoice.all\", query = \"select i from Invoice i\")])\nclass Invoice {\n    @jakarta.persistence.Id\n    var id: Long? = null\n}\n";

    fn doc(text: &str, kotlin: bool) -> Document {
        let (path, language) = if kotlin { ("/project/src/main/kotlin/shop/Invoice.kt", "kotlin") } else { ("/project/src/main/java/shop/Invoice.java", "java") };
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file(path.trim_start_matches("/project/"), text);
        std::mem::forget(host);
        Document { path: path.into(), text: text.into(), language: language.into(), version: 1 }
    }

    fn change(doc: &Document, id: &str, fields: Value) -> Result<String, Error> {
        let mut body = json!({"id": id});
        body.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
        let edit: AnnotationEdit = serde_json::from_value(body).unwrap();
        prepare(doc, &edit).map(|(_, after)| after)
    }

    #[test]
    fn lists_indexes_unique_constraints_and_literal_queries_and_keeps_computed_ones_read_only() {
        let listed = list(&doc(JAVA, false));
        let indexes = listed["indexes"].as_array().unwrap();
        assert_eq!(indexes.len(), 3);
        assert_eq!((indexes[0]["id"].as_str(), indexes[0]["name"].as_str(), indexes[0]["columns"].clone()), (Some("index:0"), Some("ix_no"), json!(["invoice_no"])));
        assert_eq!((indexes[1]["unique"].clone(), indexes[1]["columns"].clone()), (json!(true), json!(["a", "b"])));
        assert_eq!((indexes[2]["kind"].as_str(), indexes[2]["columns"].clone()), (Some("unique"), json!(["x", "y"])));
        let queries = listed["queries"].as_array().unwrap();
        assert_eq!(queries[0]["query"], "select i from Invoice i");
        assert_eq!((queries[1]["editable"].clone(), queries[1]["note"].is_string()), (json!(false), true));
    }

    #[test]
    fn index_name_columns_and_uniqueness_change_in_place_and_nothing_else() {
        let source = doc(JAVA, false);
        let after = change(&source, "index:0", json!({"name": "ix_number", "columns": ["invoice_no", "id"], "unique": true})).unwrap();
        assert!(after.contains("@Index(name = \"ix_number\", columnList = \"invoice_no, id\", unique = true)"), "{after}");
        assert!(after.contains("@Index(columnList = \"a, b DESC\", unique = true)") && after.contains("lockMode = jakarta.persistence.LockModeType.NONE"));
        let unnamed = change(&source, "index:1", json!({"name": "ix_ab", "unique": false})).unwrap();
        assert!(unnamed.contains("@Index(name = \"ix_ab\", columnList = \"a, b DESC\", unique = false)"), "{unnamed}");
        let unchanged_columns = change(&source, "index:1", json!({"columns": ["a", "b"], "name": "ix_ab"})).unwrap();
        assert!(unchanged_columns.contains("columnList = \"a, b DESC\""), "an unchanged column list keeps its sort order");
        let constraint = change(&source, "unique:0", json!({"name": "uq_xyz", "columns": ["x", "y", "z"]})).unwrap();
        assert!(constraint.contains("@UniqueConstraint(name = \"uq_xyz\", columnNames = {\"x\", \"y\", \"z\"})"), "{constraint}");
    }

    #[test]
    fn query_name_and_text_change_and_the_rest_of_the_annotation_survives() {
        let source = doc(JAVA, false);
        let after = change(&source, "query:0", json!({"name": "Invoice.byNo", "query": "select i from Invoice i where i.no = 'x'"})).unwrap();
        assert!(after.contains("@NamedQuery(name = \"Invoice.byNo\", query = \"select i from Invoice i where i.no = 'x'\", lockMode = jakarta.persistence.LockModeType.NONE)"), "{after}");
    }

    #[test]
    fn kotlin_entries_in_a_container_edit_with_kotlin_array_syntax() {
        let source = doc(KOTLIN, true);
        let after = change(&source, "query:0", json!({"query": "select i from Invoice i where i.id > 0"})).unwrap();
        assert!(after.contains("@NamedQueries([NamedQuery(name = \"Invoice.all\", query = \"select i from Invoice i where i.id > 0\")])"), "{after}");
        let index = change(&source, "index:0", json!({"columns": ["no", "id"]})).unwrap();
        assert!(index.contains("Index(columnList = \"no, id\")"), "{index}");
    }

    #[test]
    fn invalid_duplicate_unknown_read_only_and_empty_edits_are_refused() {
        let source = doc(JAVA, false);
        for (id, fields) in [
            ("index:0", json!({"name": "1bad"})),
            ("index:0", json!({"columns": []})),
            ("index:0", json!({"columns": ["a b"]})),
            ("index:0", json!({"query": "select 1"})),
            ("unique:0", json!({"unique": true})),
            ("query:0", json!({"columns": ["a"]})),
            ("query:0", json!({"name": "bad name"})),
            ("query:0", json!({"query": "drop table x"})),
            ("query:0", json!({"query": "select a\nfrom b"})),
            ("query:0", json!({"query": "select a from B where c = \"x\""})),
            ("query:0", json!({"name": "Invoice.dyn"})),
            ("query:1", json!({"query": "select 1"})),
            ("query:9", json!({"name": "A"})),
            ("index:0", json!({"name": "ix_no"})),
        ] {
            assert!(change(&source, id, fields.clone()).is_err(), "{id} {fields}");
        }
    }
}
