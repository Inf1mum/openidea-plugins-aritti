//! Reviewed single-document edits: `@Table` index / unique constraint and `@NamedQuery`.
//! Only literal source is merged into; anything else is refused rather than guessed.
use super::*;
use crate::model::{Annotation, Edit, closing, split_top_level, value_range};
use openidea_plugin::syntax::Highlight;

pub(super) const INDEX_TITLE: &str = "Add index or unique constraint with preview…";
pub(super) const QUERY_TITLE: &str = "Add named query with preview…";
const MAX_QUERY: usize = 4096;
const MAX_COLUMNS: usize = 8;
const DERIVED: &str = "derived name, naming strategy not verified";

pub(super) type Span = (usize, usize);

pub(super) struct Source {
    pub(super) class: Class,
    pub(super) ns: &'static str,
    pub(super) masked: String,
    pub(super) class_at: usize,
    pub(super) prefix: String,
}

pub(super) fn eligible(doc: &Document) -> bool {
    source(doc).is_some()
}

pub(super) fn source(doc: &Document) -> Option<Source> {
    let class = current_class(doc)?;
    if class.kind != Kind::Entity {
        return None;
    }
    let masked = mask(&doc.text);
    let class_at = crate::checks::owned_class_start(&doc.text, &masked, &class)?;
    let ns = namespace(&doc.text, &class)?;
    let all = annotations(&masked);
    for name in ["Table", "NamedQuery", "NamedQueries", "NamedNativeQuery", "NamedNativeQueries", "Index", "UniqueConstraint"] {
        if all.iter().any(|a| a.name == name) && !annotation_is(&doc.text, &class, name, ns) {
            return None;
        }
    }
    let entity = top(&masked, class_at).into_iter().find(|a| a.name == "Entity")?;
    let prefix = masked[entity.start + 1..entity.name_end].strip_suffix("Entity")?.to_owned();
    if prefix.contains(':') {
        return None;
    }
    Some(Source { class, ns, masked, class_at, prefix })
}

pub(super) fn top(masked: &str, class_at: usize) -> Vec<Annotation<'_>> {
    let all = annotations(masked);
    let spans: Vec<Span> = all.iter().filter_map(|a| a.args).collect();
    all.into_iter()
        .filter(|a| a.start < class_at && !spans.iter().any(|(s, e)| *s <= a.start && a.start < *e))
        .collect()
}

fn trim(text: &str, (s, e): Span) -> Span {
    let slice = &text[s..e];
    let lead = slice.len() - slice.trim_start().len();
    (s + lead, s + lead + slice.trim().len())
}

pub(super) fn clean(text: &str, (s, e): Span) -> bool {
    !text[s..e].contains("//") && !text[s..e].contains("/*")
}

pub(super) fn named_args(text: &str, masked: &str, (open, close): Span) -> Option<Vec<(String, Span)>> {
    let mut out: Vec<(String, Span)> = Vec::new();
    for (off, part) in split_top_level(&masked[open..close]) {
        if part.trim().is_empty() {
            continue;
        }
        let eq = part.find('=')?;
        let key = part[..eq].trim();
        if !identifier_like(key) || out.iter().any(|(k, _)| k == key) {
            return None;
        }
        let from = open + off + eq + 1;
        out.push((key.to_owned(), trim(text, (from, open + off + part.len()))));
    }
    Some(out)
}

fn identifier_like(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

pub(super) fn lit(text: &str, span: Span, kotlin: bool) -> Option<String> {
    let (_, inner) = crate::model::plain_literal(text, span, kotlin)?;
    (!inner.contains('\\')).then(|| inner.to_owned())
}

pub(super) fn bracket_parts(text: &str, masked: &str, span: Span, kotlin: bool) -> Option<Vec<Span>> {
    let (open, close) = if kotlin { (b'[', b']') } else { (b'{', b'}') };
    let bytes = masked.as_bytes();
    if span.1 < span.0 + 2 || bytes[span.0] != open || bytes[span.1 - 1] != close || !clean(text, span) {
        return None;
    }
    let mut parts = Vec::new();
    let inner = (span.0 + 1, span.1 - 1);
    let pieces = split_top_level(&masked[inner.0..inner.1]);
    for (index, (off, piece)) in pieces.iter().enumerate() {
        if piece.trim().is_empty() {
            if index + 1 == pieces.len() {
                break;
            }
            return None;
        }
        parts.push(trim(text, (inner.0 + off, inner.0 + off + piece.len())));
    }
    Some(parts)
}

fn str_list(text: &str, masked: &str, span: Span, kotlin: bool) -> Option<Vec<String>> {
    if text[span.0..span.1].starts_with('"') {
        return Some(vec![lit(text, span, kotlin)?]);
    }
    bracket_parts(text, masked, span, kotlin)?.into_iter().map(|s| lit(text, s, kotlin)).collect()
}

pub(super) fn parse_call(text: &str, masked: &str, span: Span, names: &[&str], ns: &str, kotlin: bool) -> Option<(String, Vec<(String, Span)>)> {
    let mut rest = span.0;
    if !kotlin {
        if text.as_bytes().get(rest) != Some(&b'@') {
            return None;
        }
        rest += 1;
    }
    let path_end = rest + text[rest..span.1].find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))?;
    let path = &text[rest..path_end];
    let name = path.rsplit('.').next()?;
    if !names.contains(&name) || (path != name && path != format!("{ns}.{name}")) {
        return None;
    }
    let open = path_end + masked[path_end..span.1].find(|c: char| !c.is_whitespace())?;
    if masked.as_bytes()[open] != b'(' || closing(masked.as_bytes(), open)? + 1 != span.1 || !clean(text, span) {
        return None;
    }
    Some((name.to_owned(), named_args(text, masked, (open + 1, span.1 - 1))?))
}

struct Existing {
    name: String,
    columns: Vec<String>,
}

fn refusal(key: &str) -> Error {
    format!("The existing {key} is not a literal array of literal entries; edit it manually.").into()
}

/// `Ok((array span, entries))`; the array span is `None` when the argument is absent.
fn existing(text: &str, masked: &str, args: &[(String, Span)], key: &str, s: &Source) -> Result<(Option<Span>, Vec<Existing>), Error> {
    let Some((_, span)) = args.iter().find(|(k, _)| k == key) else {
        return Ok((None, Vec::new()));
    };
    let kotlin = s.class.kotlin;
    let (call, list) = match key {
        "indexes" => ("Index", "columnList"),
        _ => ("UniqueConstraint", "columnNames"),
    };
    let mut out = Vec::new();
    for part in bracket_parts(text, masked, *span, kotlin).ok_or_else(|| refusal(key))? {
        let (_, call_args) = parse_call(text, masked, part, &[call], s.ns, kotlin).ok_or_else(|| refusal(key))?;
        let name = match call_args.iter().find(|(k, _)| k == "name") {
            Some((_, sp)) => lit(text, *sp, kotlin).ok_or_else(|| refusal(key))?,
            None => String::new(),
        };
        let (_, value) = call_args.iter().find(|(k, _)| k == list).ok_or_else(|| refusal(key))?;
        let columns = if key == "indexes" {
            lit(text, *value, kotlin)
                .ok_or_else(|| refusal(key))?
                .split(',')
                .map(|c| c.split_whitespace().next().unwrap_or("").to_owned())
                .collect()
        } else {
            str_list(text, masked, *value, kotlin).ok_or_else(|| refusal(key))?
        };
        out.push(Existing { name, columns });
    }
    Ok((Some(*span), out))
}

/// The array of `@NamedQueries(...)`: the lone argument or `value = ...`.
pub(super) fn container_value(text: &str, masked: &str, a: &Annotation) -> Option<Span> {
    let (open, close) = a.args?;
    let (start, end) = trim(text, (open, close));
    let rest = masked[start..end].strip_prefix("value").map(str::trim_start);
    match rest.and_then(|r| r.strip_prefix('=')) {
        Some(after) => Some(trim(text, (end - after.len(), end))),
        None => Some((start, end)),
    }
}

pub(super) fn insert_in_array(text: &str, masked: &str, span: Span, element: &str, kotlin: bool) -> Result<Edit, Error> {
    let parts = bracket_parts(text, masked, span, kotlin).ok_or("The existing array is not a literal.")?;
    let Some(last) = parts.last() else {
        return Ok(Edit::insert(span.0 + 1, element));
    };
    Ok(match text[last.1..span.1 - 1].find(',') {
        Some(comma) => Edit::insert(last.1 + comma + 1, &format!(" {element}")),
        None => Edit::insert(last.1, &format!(", {element}")),
    })
}

fn insert_argument(text: &str, annotation: &Annotation, argument: &str) -> Result<Edit, Error> {
    let Some((open, close)) = annotation.args else {
        return Ok(Edit::insert(annotation.name_end, &format!("({argument})")));
    };
    if !clean(text, (open, close)) {
        return Err("The annotation contains comments; edit it manually.".into());
    }
    let inner = &text[open..close];
    if inner.trim().is_empty() {
        return Ok(Edit::insert(open, argument));
    }
    let end = open + inner.trim_end().len();
    Ok(if text[..end].ends_with(',') {
        Edit::insert(end, &format!(" {argument}"))
    } else {
        Edit::insert(end, &format!(", {argument}"))
    })
}

pub(super) fn insert_annotation_line(text: &str, masked: &str, anchor: &Annotation, annotation: &str) -> Edit {
    let end = anchor.end();
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let line_start = text[..anchor.start].rfind('\n').map_or(0, |i| i + 1);
    let indent: String = text[line_start..anchor.start].chars().take_while(|c| matches!(c, ' ' | '\t')).collect();
    match masked[end..].find('\n') {
        Some(i) if masked[end..end + i].trim().is_empty() => Edit::insert(end + i + 1, &format!("{indent}{annotation}{newline}")),
        _ => Edit::insert(end, &format!(" {annotation}")),
    }
}

pub(super) fn call(s: &Source, name: &str, arguments: &str, nested: bool) -> String {
    let at = if s.class.kotlin && nested { "" } else { "@" };
    format!("{at}{}{name}({arguments})", s.prefix)
}

pub(super) fn quote(value: &str, kotlin: bool) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '$' if kotlin => out.push_str("\\$"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

pub(super) fn imports(doc: &Document, s: &Source, names: &[&str]) -> Result<Option<TextEdit>, Error> {
    if !s.prefix.is_empty() {
        return Ok(None);
    }
    let wanted: Vec<String> = names.iter().map(|n| format!("{}.{n}", s.ns)).collect();
    for (name, full) in names.iter().zip(&wanted) {
        if s.class.imports.iter().any(|i| i.rsplit('.').next() == Some(name) && i != full) {
            return Err(format!("An import named {name} already refers to another type.").into());
        }
    }
    let refs: Vec<&str> = wanted.iter().map(String::as_str).collect();
    Ok(crate::model::import_edit(&doc.text, &refs, s.class.kotlin).map(|mut edit| {
        if doc.text.contains("\r\n") {
            edit.text = edit.text.replace('\n', "\r\n");
        }
        edit.to_text_edit(&doc.text)
    }))
}

pub(super) fn finish(pending: &Pending, doc: &Document, title: String, header: &str, edits: Vec<TextEdit>, notes: &str) -> Result<FileAction, Error> {
    let after = super::patched(&doc.text, &edits)?;
    if !ui_kit::prompts::confirm_exact(
        header,
        &format!("Target: {}\nSource version: {}\n\n{after}\n{notes}\nCurrent source only; other mappings and persistence units are not read.", doc.path, doc.version),
    )? {
        return Err(Error::Cancelled);
    }
    super::current(pending, doc)?;
    Ok(FileAction { title, kind: Some("refactor.rewrite".into()), edits: vec![(doc.path.clone(), edits)], ..FileAction::default() })
}

pub(super) fn input(label: &str, default: &str) -> Result<String, Error> {
    host::input(label, Some(default))?.map(|s| s.trim().to_owned()).ok_or(Error::Cancelled)
}

struct Candidate {
    attribute: String,
    column: String,
    derived: bool,
}

fn candidates(doc: &Document, s: &Source) -> Result<Vec<Candidate>, Error> {
    let kotlin = s.class.kotlin;
    for a in annotations(&s.masked) {
        if !matches!(a.name, "Column" | "JoinColumn" | "JoinColumns" | "AttributeOverride" | "AttributeOverrides") {
            continue;
        }
        if matches!(a.name, "JoinColumns" | "AttributeOverride" | "AttributeOverrides") {
            return Err("Composite join columns or attribute overrides are present; column names cannot be proven.".into());
        }
        if let Some(range) = value_range(&doc.text, &s.masked, &a, "name") {
            if lit(&doc.text, range, kotlin).is_none() {
                return Err("A @Column/@JoinColumn name is not a plain literal; column names cannot be proven.".into());
            }
        }
    }
    Ok(s.class
        .attributes
        .iter()
        .filter(|a| !a.id && !a.lob && !a.mapped_by && a.join_table.is_none() && matches!(a.relation, Relation::None | Relation::ManyToOne))
        .filter_map(|a| {
            let derived = a.column.is_none();
            let column = match (&a.column, a.relation) {
                (Some(c), _) => c.clone(),
                (None, Relation::ManyToOne) => format!("{}_id", snake_case(&a.name)),
                (None, _) => snake_case(&a.name),
            };
            crate::wizard::column_name(&column).then(|| Candidate { attribute: a.name.clone(), column, derived })
        })
        .collect())
}

fn sql_name(value: &str) -> bool {
    !value.is_empty() && value.len() <= 63 && value.is_ascii() && value.bytes().enumerate().all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || (i > 0 && b.is_ascii_digit()))
}

pub(super) fn resolve_index(pending: &Pending, doc: &Document) -> Result<FileAction, Error> {
    super::current(pending, doc)?;
    let s = source(doc).ok_or(Error::Stale)?;
    let kotlin = s.class.kotlin;
    let tops = top(&s.masked, s.class_at);
    let tables: Vec<&Annotation> = tops.iter().filter(|a| a.name == "Table").collect();
    let [table] = tables.as_slice() else {
        return Err(if tables.is_empty() {
            "This entity has no @Table. Add @Table(name = ...) first; the table name is not guessed.".into()
        } else {
            "More than one @Table was found; edit manually.".into()
        });
    };
    let args = match table.args {
        Some((open, close)) => {
            if !clean(&doc.text, (open, close)) {
                return Err("@Table contains comments; edit it manually.".into());
            }
            named_args(&doc.text, &s.masked, (open, close)).ok_or("@Table arguments are not plain named arguments; edit manually.")?
        }
        None => Vec::new(),
    };
    let table_name = match args.iter().find(|(k, _)| k == "name") {
        Some((_, span)) => Some(lit(&doc.text, *span, kotlin).ok_or("The @Table name is not a plain literal.")?),
        None => None,
    };
    let (index_array, indexes) = existing(&doc.text, &s.masked, &args, "indexes", &s)?;
    let (constraint_array, constraints) = existing(&doc.text, &s.masked, &args, "uniqueConstraints", &s)?;
    let offered = candidates(doc, &s)?;
    if offered.is_empty() {
        return Err("No scalar or @ManyToOne attributes with provable column names are available.".into());
    }
    let items: Vec<_> = offered
        .iter()
        .map(|c| {
            let item = PickItem::new(&c.attribute, &format!("{} → {}", c.attribute, c.column));
            if c.derived { item.description(DERIVED) } else { item }
        })
        .collect();
    let chosen = host::pick_many("Columns, in index order", &items)?.ok_or(Error::Cancelled)?;
    let mut picked: Vec<&Candidate> = Vec::new();
    for name in &chosen {
        let found = offered.iter().find(|c| &c.attribute == name).ok_or("Choose only offered attributes.")?;
        if picked.iter().any(|c| c.attribute == found.attribute) {
            return Err("Choose each attribute once.".into());
        }
        picked.push(found);
    }
    if picked.is_empty() || picked.len() > MAX_COLUMNS {
        return Err("Choose between one and eight attributes.".into());
    }
    let kind = host::pick(
        "Kind",
        &[
            PickItem::new("index", "Index (@Index)"),
            PickItem::new("unique-index", "Unique index (@Index unique = true)"),
            PickItem::new("constraint", "Unique constraint (@UniqueConstraint)"),
        ],
    )?
    .ok_or(Error::Cancelled)?;
    if !matches!(kind.as_str(), "index" | "unique-index" | "constraint") {
        return Err(Error::Stale);
    }
    let mut directions = Vec::new();
    if kind != "constraint" {
        for c in &picked {
            let d = host::pick(
                &format!("Order of {}", c.column),
                &[PickItem::new("ASC", "ASC"), PickItem::new("DESC", "DESC")],
            )?
            .ok_or(Error::Cancelled)?;
            if !matches!(d.as_str(), "ASC" | "DESC") {
                return Err(Error::Stale);
            }
            directions.push(d);
        }
    }
    let table_for_name = table_name.clone().unwrap_or_else(|| snake_case(&s.class.name));
    let default = format!(
        "{}_{}_{}",
        if kind == "constraint" || kind == "unique-index" { "uq" } else { "idx" },
        table_for_name,
        picked.iter().map(|c| c.column.as_str()).collect::<Vec<_>>().join("_")
    );
    let name = input("Index or constraint name", &default)?;
    if !sql_name(&name) {
        return Err("Use an ASCII name of at most 63 characters: letters, digits and underscore, not starting with a digit.".into());
    }
    super::current(pending, doc)?;
    let columns: Vec<&str> = picked.iter().map(|c| c.column.as_str()).collect();
    if indexes.iter().chain(&constraints).any(|e| e.name.eq_ignore_ascii_case(&name)) {
        return Err("An index or unique constraint with this name already exists.".into());
    }
    if indexes.iter().chain(&constraints).any(|e| {
        e.columns.len() == columns.len() && e.columns.iter().zip(&columns).all(|(a, b)| a.eq_ignore_ascii_case(b))
    }) {
        return Err("An index or unique constraint on this column list already exists.".into());
    }
    let (key, element, array, used) = if kind == "constraint" {
        let list: Vec<String> = columns.iter().map(|c| quote(c, kotlin)).collect();
        let list = if kotlin { format!("[{}]", list.join(", ")) } else { format!("{{{}}}", list.join(", ")) };
        (
            "uniqueConstraints",
            call(&s, "UniqueConstraint", &format!("name = {}, columnNames = {list}", quote(&name, kotlin)), true),
            constraint_array,
            "UniqueConstraint",
        )
    } else {
        let list: Vec<String> = picked
            .iter()
            .zip(&directions)
            .map(|(c, d)| if d == "DESC" { format!("{} DESC", c.column) } else { c.column.clone() })
            .collect();
        let unique = if kind == "unique-index" { ", unique = true" } else { "" };
        (
            "indexes",
            call(&s, "Index", &format!("name = {}, columnList = {}{unique}", quote(&name, kotlin), quote(&list.join(", "), kotlin)), true),
            index_array,
            "Index",
        )
    };
    let mut edits = Vec::new();
    match array {
        Some(span) => edits.push(insert_in_array(&doc.text, &s.masked, span, &element, kotlin)?),
        None => {
            let array = if kotlin { format!("[{element}]") } else { format!("{{{element}}}") };
            edits.push(insert_argument(&doc.text, table, &format!("{key} = {array}"))?);
        }
    }
    let mut edits: Vec<TextEdit> = edits.iter().map(|e| e.to_text_edit(&doc.text)).collect();
    edits.extend(imports(doc, &s, &[used])?);
    let mut notes = format!("Table: {}\n", table_name.as_deref().unwrap_or("(no literal name; entity-name default not verified)"));
    for c in picked.iter().filter(|c| c.derived) {
        notes.push_str(&format!("Column {} for {}: {DERIVED}.\n", c.column, c.attribute));
    }
    notes.push_str("Existing names are compared in this file only; the database and schema generation are not read.");
    finish(pending, doc, format!("Add {key} entry {name}"), "Add index or unique constraint?", edits, &notes)
}

pub(super) fn query_sanity(query: &str) -> Result<(), Error> {
    let bad = |m: &str| Error::from(format!("The JPQL looks wrong: {m}."));
    let tokens = crate::jpql_language::tokens(query);
    let first = tokens.first().ok_or_else(|| bad("it could not be read"))?;
    let word = query[first.range.clone()].to_ascii_lowercase();
    if first.kind != Highlight::Keyword || !matches!(word.as_str(), "select" | "update" | "delete") {
        return Err(bad("it must start with SELECT, UPDATE or DELETE"));
    }
    let mut depth = 0i32;
    for t in &tokens {
        let text = &query[t.range.clone()];
        match t.kind {
            Highlight::Comment => return Err(bad("comments are not allowed")),
            Highlight::String if text.len() < 2 || !text.ends_with('\'') => return Err(bad("unterminated string literal")),
            Highlight::Punctuation if text == "(" => depth += 1,
            Highlight::Punctuation if text == ")" => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return Err(bad("unbalanced parentheses"));
        }
    }
    if depth != 0 {
        return Err(bad("unbalanced parentheses"));
    }
    Ok(())
}

fn query_name_ok(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().enumerate().all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || (i > 0 && (b.is_ascii_digit() || b == b'.')))
}

pub(super) fn resolve_query(pending: &Pending, doc: &Document) -> Result<FileAction, Error> {
    super::current(pending, doc)?;
    let s = source(doc).ok_or(Error::Stale)?;
    let kotlin = s.class.kotlin;
    let tops = top(&s.masked, s.class_at);
    let unreadable = || Error::from("An existing named query has a non-literal name or shape; edit manually.");
    let mut names: Vec<String> = Vec::new();
    let mut containers: Vec<&Annotation> = Vec::new();
    let mut singles: Vec<&Annotation> = Vec::new();
    for a in &tops {
        match a.name {
            "NamedQuery" | "NamedNativeQuery" => {
                let (open, close) = a.args.ok_or_else(unreadable)?;
                if !clean(&doc.text, (open, close)) {
                    return Err(unreadable());
                }
                let args = named_args(&doc.text, &s.masked, (open, close)).ok_or_else(unreadable)?;
                let (_, span) = args.iter().find(|(k, _)| k == "name").ok_or_else(unreadable)?;
                names.push(lit(&doc.text, *span, kotlin).ok_or_else(unreadable)?);
                if a.name == "NamedQuery" {
                    singles.push(a);
                }
            }
            "NamedQueries" | "NamedNativeQueries" => {
                let span = container_value(&doc.text, &s.masked, a).ok_or_else(unreadable)?;
                let element = if a.name == "NamedQueries" { "NamedQuery" } else { "NamedNativeQuery" };
                for part in bracket_parts(&doc.text, &s.masked, span, kotlin).ok_or_else(unreadable)? {
                    let (_, args) = parse_call(&doc.text, &s.masked, part, &[element], s.ns, kotlin).ok_or_else(unreadable)?;
                    let (_, sp) = args.iter().find(|(k, _)| k == "name").ok_or_else(unreadable)?;
                    names.push(lit(&doc.text, *sp, kotlin).ok_or_else(unreadable)?);
                }
                if a.name == "NamedQueries" {
                    containers.push(a);
                }
            }
            _ => {}
        }
    }
    if containers.len() > 1 {
        return Err("More than one @NamedQueries was found; edit manually.".into());
    }
    if containers.is_empty() && !singles.is_empty() && s.ns == "javax.persistence" {
        return Err("Adding a second @NamedQuery needs a repeatable annotation (javax.persistence 2.2+) or a @NamedQueries container; the javax.persistence version is not known and this action does not rewrite the existing annotation. Add it manually.".into());
    }
    let default_name = format!("{}.findAll", s.class.name);
    let name = input("Named query name", &default_name)?;
    if !query_name_ok(&name) {
        return Err("Use an ASCII query name of at most 128 characters: letters, digits, underscore and dots, not starting with a digit or dot.".into());
    }
    let query = input("JPQL (single line)", &format!("select e from {} e", s.class.name))?;
    if query.is_empty() || query.len() > MAX_QUERY || query.chars().any(char::is_control) {
        return Err(format!("The query must be non-blank, single-line text of at most {MAX_QUERY} bytes.").into());
    }
    query_sanity(&query)?;
    super::current(pending, doc)?;
    if names.iter().any(|n| n == &name) {
        return Err("A named query with this name already exists in this file.".into());
    }
    let arguments = format!("name = {}, query = {}", quote(&name, kotlin), quote(&query, kotlin));
    let element = call(&s, "NamedQuery", &arguments, containers.first().is_some());
    let edit = match containers.first() {
        Some(container) => {
            let span = container_value(&doc.text, &s.masked, container).ok_or_else(unreadable)?;
            insert_in_array(&doc.text, &s.masked, span, &element, kotlin)?
        }
        None => {
            let anchor = singles
                .last()
                .copied()
                .or_else(|| tops.iter().find(|a| a.name == "Entity"))
                .ok_or(Error::Stale)?;
            insert_annotation_line(&doc.text, &s.masked, anchor, &element)
        }
    };
    let mut edits = vec![edit.to_text_edit(&doc.text)];
    edits.extend(imports(doc, &s, &["NamedQuery"])?);
    let notes = "JPQL checked lexically only (leading verb, strings, parentheses, no comments); entity and attribute names are not resolved.\nName uniqueness is checked in this file only (the persistence-unit scope is not read; whether native and JPQL names share a namespace is treated conservatively).";
    finish(pending, doc, format!("Add named query {name}"), "Add named query?", edits, notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const JAVA: &str = "package shop;\n\nimport jakarta.persistence.Column;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.ManyToOne;\nimport jakarta.persistence.Table;\n\n@Entity\n@Table(name = \"invoices\")\npublic class Invoice {\n    @Id\n    private Long id;\n    @Column(name = \"invoice_no\")\n    private String number;\n    private String customerName;\n    @ManyToOne\n    private Customer customer;\n}\n";
    const KOTLIN: &str = "package shop\n\nimport jakarta.persistence.Column\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\nimport jakarta.persistence.Table\n\n@Entity\n@Table(name = \"invoices\")\nclass Invoice {\n    @Id var id: Long? = null\n    @Column(name = \"invoice_no\") var number: String? = null\n    var customerName: String? = null\n}\n";

    fn javax(text: &str) -> String {
        text.replace("jakarta.persistence", "javax.persistence")
    }
    fn setup(text: &str, kotlin: bool) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let (dir, ext, language) = if kotlin { ("kotlin", "kt", "kotlin") } else { ("java", "java", "java") };
        let path = format!("/project/src/main/{dir}/shop/Invoice.{ext}");
        host.open_editor(&path, language, text, &[]);
        (host, JpaAssist::default(), Document { path, language: language.into(), text: text.into(), version: 1 })
    }
    fn run(text: &str, kotlin: bool, title: &str, rows: &[&[&str]]) -> Result<String, Error> {
        let (host, mut plugin, doc) = setup(text, kotlin);
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == title).expect("offered");
        for row in rows {
            host.answer_next_prompt(Some(row));
        }
        let result = plugin.resolve_file_action(&doc, pending)?;
        assert_eq!(result.edits.len(), 1);
        assert!(result.operations.is_empty());
        assert_eq!(host.editor_text(), text);
        patched(&doc.text, &result.edits[0].1)
    }
    fn index(text: &str, kotlin: bool, rows: &[&[&str]]) -> Result<String, Error> {
        run(text, kotlin, INDEX_TITLE, rows)
    }
    fn query(text: &str, kotlin: bool, name: &str, jpql: &str) -> Result<String, Error> {
        run(text, kotlin, QUERY_TITLE, &[&[name], &[jpql], &[]])
    }
    const INDEX_ANSWERS: &[&[&str]] = &[&["number", "customerName"], &["index"], &["ASC"], &["DESC"], &["idx_a"], &[]];

    #[test]
    fn java_index_is_added_to_table_with_literal_and_derived_columns_and_import() {
        let text = JAVA.replace("import jakarta.persistence.Index;\n", "");
        let after = index(&text, false, INDEX_ANSWERS).unwrap();
        assert!(after.contains("@Table(name = \"invoices\", indexes = {@Index(name = \"idx_a\", columnList = \"invoice_no, customer_name DESC\")})"), "{after}");
        assert!(after.contains("import jakarta.persistence.Index;\n"), "{after}");
    }

    #[test]
    fn many_to_one_uses_derived_join_column_and_unique_index_flag() {
        let rows: &[&[&str]] = &[&["customer"], &["unique-index"], &["ASC"], &["uq_c"], &[]];
        let after = index(JAVA, false, rows).unwrap();
        assert!(after.contains("@Index(name = \"uq_c\", columnList = \"customer_id\", unique = true)"), "{after}");
    }

    #[test]
    fn unique_constraint_renders_java_and_kotlin_array_literals() {
        let rows: &[&[&str]] = &[&["number", "customerName"], &["constraint"], &["uq_a"], &[]];
        let java = index(JAVA, false, rows).unwrap();
        assert!(java.contains("uniqueConstraints = {@UniqueConstraint(name = \"uq_a\", columnNames = {\"invoice_no\", \"customer_name\"})}"), "{java}");
        assert!(java.contains("import jakarta.persistence.UniqueConstraint;"));
        let kotlin = index(KOTLIN, true, rows).unwrap();
        assert!(kotlin.contains("uniqueConstraints = [UniqueConstraint(name = \"uq_a\", columnNames = [\"invoice_no\", \"customer_name\"])]"), "{kotlin}");
        assert!(kotlin.contains("import jakarta.persistence.UniqueConstraint\n") && !kotlin.contains("UniqueConstraint;"));
    }

    #[test]
    fn kotlin_javax_merges_into_existing_arrays_and_adds_missing_parens() {
        let existing = javax(KOTLIN).replace(
            "@Table(name = \"invoices\")",
            "@Table(name = \"invoices\", indexes = [Index(name = \"idx_old\", columnList = \"customer_name\")])",
        ).replace("import javax.persistence.Table", "import javax.persistence.Index\nimport javax.persistence.Table");
        let rows: &[&[&str]] = &[&["number"], &["index"], &["DESC"], &["idx_new"], &[]];
        let after = index(&existing, true, rows).unwrap();
        assert!(after.contains("indexes = [Index(name = \"idx_old\", columnList = \"customer_name\"), Index(name = \"idx_new\", columnList = \"invoice_no DESC\")])"), "{after}");
        assert_eq!(after.matches("import javax.persistence.Index").count(), 1);
        let bare = javax(KOTLIN).replace("@Table(name = \"invoices\")", "@Table");
        let after = index(&bare, true, &[&["number"], &["index"], &["ASC"], &["idx_x"], &[]]).unwrap();
        assert!(after.contains("@Table(indexes = [Index(name = \"idx_x\", columnList = \"invoice_no\")])"), "{after}");
    }

    #[test]
    fn existing_java_array_with_trailing_comma_and_crlf_is_merged() {
        let text = JAVA
            .replace("@Table(name = \"invoices\")", "@Table(name = \"invoices\", uniqueConstraints = {@UniqueConstraint(columnNames = \"customer_name\"),})")
            .replace("import jakarta.persistence.Table;", "import jakarta.persistence.Table;\nimport jakarta.persistence.UniqueConstraint;")
            .replace('\n', "\r\n");
        let rows: &[&[&str]] = &[&["number"], &["constraint"], &["uq_n"], &[]];
        let after = index(&text, false, rows).unwrap();
        assert!(after.contains("columnNames = \"customer_name\"),"), "{after}");
        assert!(after.contains(", @UniqueConstraint(name = \"uq_n\", columnNames = {\"invoice_no\"})})"), "{after}");
        assert!(!after.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn index_refusals_never_guess() {
        let no_table = JAVA.replace("@Table(name = \"invoices\")\n", "");
        assert!(index(&no_table, false, &[]).unwrap_err().to_string().contains("no @Table"));
        let with = |args: &str| JAVA.replace("@Table(name = \"invoices\")", &format!("@Table(name = \"invoices\", {args})")).replace("import jakarta.persistence.Table;", "import jakarta.persistence.Index;\nimport jakarta.persistence.Table;");
        let dup_name = with("indexes = {@Index(name = \"IDX_A\", columnList = \"customer_name\")}");
        assert!(index(&dup_name, false, INDEX_ANSWERS).unwrap_err().to_string().contains("name already exists"));
        let dup_cols = with("indexes = {@Index(name = \"other\", columnList = \"invoice_no ASC, customer_name\")}");
        assert!(index(&dup_cols, false, &[&["number", "customerName"], &["index"], &["ASC"], &["DESC"], &["idx_b"], &[]]).unwrap_err().to_string().contains("column list"));
        for hostile in [with("indexes = INDEXES"), with("indexes = {@Index(name = N, columnList = \"a\")}"), with("indexes = {makeIndex()}")] {
            assert!(index(&hostile, false, INDEX_ANSWERS).unwrap_err().to_string().contains("literal array"), "{hostile}");
        }
        let non_literal_column = JAVA.replace("@Column(name = \"invoice_no\")", "@Column(name = NAME)");
        assert!(index(&non_literal_column, false, INDEX_ANSWERS).unwrap_err().to_string().contains("not a plain literal"));
        let commented = JAVA.replace("@Table(name = \"invoices\")", "@Table(name = \"invoices\" /* c */)");
        assert!(index(&commented, false, INDEX_ANSWERS).is_err());
        let bad = ["", "1x", "a b", "a-b", "é"];
        for name in bad {
            assert!(index(JAVA, false, &[&["number"], &["index"], &["ASC"], &[name], &[]]).unwrap_err().to_string().contains("ASCII name"), "{name}");
        }
        assert!(index(JAVA, false, &[&[], &["index"]]).is_err());
    }

    #[test]
    fn index_cancel_and_drift_do_not_apply() {
        let (host, mut plugin, doc) = setup(JAVA, false);
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == INDEX_TITLE).unwrap();
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)));
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == INDEX_TITLE).unwrap();
        host.type_in_editor("// changed");
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
    }

    #[test]
    fn jakarta_adds_repeated_named_query_after_entity_and_escapes_literals() {
        let after = query(JAVA, false, "Invoice.byNo", "select i from Invoice i where i.number = \"x\\y\"").unwrap();
        assert!(after.contains("@Entity\n@NamedQuery(name = \"Invoice.byNo\", query = \"select i from Invoice i where i.number = \\\"x\\\\y\\\"\")\n@Table"), "{after}");
        assert!(after.contains("import jakarta.persistence.NamedQuery;\n"), "{after}");
        let second = query(&after, false, "Invoice.two", "select i from Invoice i").unwrap();
        assert!(second.contains("query = \"select i from Invoice i\")\n@Table") && second.matches("@NamedQuery(").count() == 2, "{second}");
        assert_eq!(second.matches("import jakarta.persistence.NamedQuery;").count(), 1);
    }

    #[test]
    fn kotlin_query_escapes_dollar_and_crlf_is_kept() {
        let text = KOTLIN.replace('\n', "\r\n");
        let after = query(&text, true, "Invoice.cost", "select i from Invoice i where i.number = '$1\\'").unwrap();
        assert!(after.contains("@NamedQuery(name = \"Invoice.cost\", query = \"select i from Invoice i where i.number = '\\$1\\\\'\")\r\n@Table"), "{after:?}");
        assert!(!after.replace("\r\n", "").contains('\n'));
        assert!(after.contains("import jakarta.persistence.NamedQuery\r\n"));
    }

    #[test]
    fn javax_single_is_added_but_second_is_refused_unless_a_container_exists() {
        let first = query(&javax(JAVA), false, "Invoice.one", "select i from Invoice i").unwrap();
        assert!(first.contains("@NamedQuery(name = \"Invoice.one\""));
        let err = query(&first, false, "Invoice.two", "select i from Invoice i").unwrap_err().to_string();
        assert!(err.contains("repeatable") && err.contains("javax"), "{err}");
        let container = javax(JAVA)
            .replace("@Entity\n", "@Entity\n@NamedQueries({@NamedQuery(name = \"Invoice.one\", query = \"select i from Invoice i\")})\n")
            .replace("import javax.persistence.Entity;", "import javax.persistence.Entity;\nimport javax.persistence.NamedQueries;\nimport javax.persistence.NamedQuery;");
        let after = query(&container, false, "Invoice.two", "update Invoice i set i.number = :n").unwrap();
        assert!(after.contains("select i from Invoice i\"), @NamedQuery(name = \"Invoice.two\", query = \"update Invoice i set i.number = :n\")})"), "{after}");
        assert_eq!(after.matches("@NamedQueries").count(), 1);
    }

    #[test]
    fn jakarta_with_existing_container_merges_in_kotlin_and_java() {
        let container = KOTLIN
            .replace("@Entity\n", "@Entity\n@NamedQueries([NamedQuery(name = \"Invoice.one\", query = \"select i from Invoice i\")])\n")
            .replace("import jakarta.persistence.Entity", "import jakarta.persistence.Entity\nimport jakarta.persistence.NamedQueries\nimport jakarta.persistence.NamedQuery");
        let after = query(&container, true, "Invoice.two", "select count(i) from Invoice i").unwrap();
        assert!(after.contains("\"select i from Invoice i\"), NamedQuery(name = \"Invoice.two\", query = \"select count(i) from Invoice i\")])"), "{after}");
    }

    #[test]
    fn query_refusals_cover_names_shapes_and_jpql() {
        let with_native = JAVA.replace("@Entity\n", "@Entity\n@NamedNativeQuery(name = \"Invoice.n\", query = \"select 1\")\n").replace("import jakarta.persistence.Entity;", "import jakarta.persistence.Entity;\nimport jakarta.persistence.NamedNativeQuery;");
        assert!(query(&with_native, false, "Invoice.n", "select i from Invoice i").unwrap_err().to_string().contains("already exists"));
        let non_literal = JAVA.replace("@Entity\n", "@Entity\n@NamedQuery(name = NAME, query = \"select 1\")\n").replace("import jakarta.persistence.Entity;", "import jakarta.persistence.Entity;\nimport jakarta.persistence.NamedQuery;");
        assert!(query(&non_literal, false, "Invoice.x", "select i from Invoice i").unwrap_err().to_string().contains("non-literal"));
        let bad_names = ["", ".a", "1a", "a b", "a\"b"];
        for name in bad_names {
            assert!(query(JAVA, false, name, "select i from Invoice i").unwrap_err().to_string().contains("query name"), "{name}");
        }
        let oversize = format!("select i from Invoice i where i.number = '{}'", "x".repeat(MAX_QUERY));
        for bad in ["", "   ", &oversize] {
            assert!(query(JAVA, false, "Invoice.q", bad).unwrap_err().to_string().contains("non-blank"), "{bad}");
        }
        for bad in ["from Invoice i", "select (i from Invoice i", "select i from Invoice i where i.a = 'x", "select i /* c */ from Invoice i", "select i from Invoice i)"] {
            assert!(query(JAVA, false, "Invoice.q", bad).unwrap_err().to_string().contains("JPQL looks wrong"), "{bad}");
        }
    }

    #[test]
    fn foreign_annotation_namespaces_and_non_entities_are_not_offered() {
        let (_host, mut plugin, doc) = setup(&JAVA.replace("import jakarta.persistence.Table;", "import vendor.Table;"), false);
        assert!(plugin.file_actions(&doc, Range::default()).iter().all(|a| a.title != INDEX_TITLE && a.title != QUERY_TITLE));
        let (_host, mut plugin, doc) = setup(&JAVA.replace("Entity", "Embeddable"), false);
        assert!(plugin.file_actions(&doc, Range::default()).iter().all(|a| a.title != INDEX_TITLE));
    }

    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_API / JPA_JAVAX_API (and Kotlin env vars)"]
    fn generated_index_and_named_query_sources_compile_for_java_kotlin_and_both_namespaces() {
        let base = std::env::temp_dir().join(format!("jpa-index-query-compile-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        for kotlin in [false, true] {
            for ns in ["jakarta.persistence", "javax.persistence"] {
                let seed = if ns == "jakarta.persistence" { if kotlin { KOTLIN.to_owned() } else { JAVA.to_owned() } } else { javax(if kotlin { KOTLIN } else { JAVA }) };
                let first = index(&seed, kotlin, INDEX_ANSWERS_FOR_COMPILE).unwrap();
                let second = run(&first, kotlin, INDEX_TITLE, &[&["number"], &["constraint"], &["uq_n"], &[]]).unwrap();
                let third = query(&second, kotlin, "Invoice.byNo", "select i from Invoice i where i.number = 'a$b'").unwrap();
                let fourth = if ns == "jakarta.persistence" {
                    query(&third, kotlin, "Invoice.two", "select count(i) from Invoice i").unwrap()
                } else {
                    third
                };
                let source = fourth;
                let directory = base.join(format!("{}-{ns}", if kotlin { "kotlin" } else { "java" }));
                std::fs::create_dir_all(&directory).unwrap();
                let jar = std::env::var(if ns == "jakarta.persistence" { "JPA_JAKARTA_API" } else { "JPA_JAVAX_API" }).expect("explicit persistence API jar");
                let customer = directory.join("Customer.java");
                std::fs::write(&customer, "package shop;\npublic class Customer {}\n").unwrap();
                let owner = directory.join(if kotlin { "Invoice.kt" } else { "Invoice.java" });
                std::fs::write(&owner, source).unwrap();
                let mut javac = std::process::Command::new("javac");
                javac.args(["--release", "17", "-proc:none", "-classpath", &jar, "-d"]).arg(&directory).arg(&customer);
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
        eprintln!("Compiled 4 generated entities with index, unique constraint and named queries: Java/Kotlin x Jakarta 3.2/javax 2.2.");
    }
    const INDEX_ANSWERS_FOR_COMPILE: &[&[&str]] = &[&["number", "customerName"], &["index"], &["ASC"], &["DESC"], &["idx_a"], &[]];
}
