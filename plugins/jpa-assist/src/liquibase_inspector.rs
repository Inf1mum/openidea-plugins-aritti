//! Changelog inspector items (G113) as reviewed single-document edits on a Liquibase XML changelog. The original shows
//! the attributes of the changeSet or change under the caret in a tool-window pane; here every item is a code action
//! (or an attribute-value completion) on the invoking changelog and nothing else is read or written.
//!
//! Items and the Liquibase documentation they rest on (docs.liquibase.com: changeset attribute reference, dbms
//! attribute page, contexts and labels pages, preconditions page, change-type pages):
//! - changeSet `dbms`: comma separated database short names from the documented list, each optionally prefixed with
//!   `!`, or `all` / `none` alone. Other names stay unsupported (never guessed).
//! - changeSet `context`: an expression of names with `,` (= or), `and`, `or`, `!` and parentheses.
//! - changeSet `labels`: a comma separated list of names (no operators).
//! - changeSet `runAlways`, `runOnChange`, `failOnError`, `runInTransaction` (true or false) and `onValidationFail`
//!   (HALT or MARK_RAN).
//! - `preConditions` `onFail` / `onError`: HALT, CONTINUE, MARK_RAN, WARN; CONTINUE and MARK_RAN only inside a changeSet.
//! - column type: `type` of a `column` of createTable/addColumn, `newDataType` of modifyDataType, `columnDataType` of
//!   addNotNullConstraint. Offered types are the generic ones the changelog generator already writes; a length is asked
//!   for char and varchar only; any other value is typed by the user (decimal precision syntax is not offered).
//! - `sequenceName` of createSequence, alterSequence and dropSequence.
//! - removing the changeSet, or one change of a changeSet that keeps another change.
//!
//! Values for the pickers and for completion come from the invoking document and the `liquibaseChangeSetDefaults`
//! setting only; no other file is read.
use super::*;
use quick_xml::{events::Event, name::ResolveResult, reader::NsReader};

const NAMESPACE: &str = "http://www.liquibase.org/xml/ns/dbchangelog";
const MAX_EVENTS: usize = 50_000;
const MAX_KNOWN: usize = 40;

pub(crate) const DBMS_NAMES: [&str; 35] = [
    "aerospike", "asany", "bigquery", "cassandra", "cloudspanner", "cockroachdb", "cosmosdb", "databricks", "db2", "db2i", "db2z",
    "derby", "dynamodb", "edb", "firebird", "hana", "h2", "hsqldb", "informix", "ingres", "mariadb", "maxdb", "mongodb", "mssql",
    "mysql", "neo4j", "oracle", "postgresql", "redshift", "snowflake", "sqlite", "sybase", "teradata", "vertica", "yugabytedb",
];
const GENERIC_TYPES: [&str; 12] = ["tinyint", "smallint", "int", "bigint", "boolean", "float", "double", "clob", "date", "time", "datetime", "uuid"];
const RUN_FLAGS: [&str; 4] = ["runAlways", "runOnChange", "failOnError", "runInTransaction"];
const VALIDATION_FAIL: [&str; 2] = ["HALT", "MARK_RAN"];
const PRECONDITION_ACTIONS: [&str; 4] = ["HALT", "CONTINUE", "MARK_RAN", "WARN"];
const SEQUENCE_ELEMENTS: [&str; 3] = ["createSequence", "alterSequence", "dropSequence"];
const NON_CHANGE_CHILDREN: [&str; 5] = ["preConditions", "comment", "rollback", "validCheckSum", "modifySql"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Item {
    Dbms,
    Contexts,
    Labels,
    RunOptions,
    PreconditionOutcome,
    ColumnType,
    SequenceName,
    RemoveChangeSet,
    RemoveChange,
}

const ITEMS: [Item; 9] = [
    Item::Dbms,
    Item::Contexts,
    Item::Labels,
    Item::RunOptions,
    Item::PreconditionOutcome,
    Item::ColumnType,
    Item::SequenceName,
    Item::RemoveChangeSet,
    Item::RemoveChange,
];

impl Item {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Dbms => "Set changeSet dbms…",
            Self::Contexts => "Set changeSet contexts…",
            Self::Labels => "Set changeSet labels…",
            Self::RunOptions => "Set changeSet run options…",
            Self::PreconditionOutcome => "Set preConditions onFail and onError…",
            Self::ColumnType => "Set column type…",
            Self::SequenceName => "Set sequence name…",
            Self::RemoveChangeSet => "Remove this changeSet…",
            Self::RemoveChange => "Remove this change…",
        }
    }
}

#[derive(PartialEq, Eq)]
enum Kind {
    Eof,
    Skip,
    End,
    Start,
    Empty,
}

struct Attr {
    name: String,
    start: usize,
    value_start: usize,
    value_end: usize,
    end: usize,
}

struct El {
    name: String,
    start: usize,
    tag_end: usize,
    end: usize,
    parent: Option<usize>,
    attrs: Vec<Attr>,
}

struct Parsed {
    els: Vec<El>,
}

fn scan_attrs(text: &str, start: usize, end: usize) -> Vec<Attr> {
    let bytes = text.as_bytes();
    let mut at = start + 1;
    let mut found = Vec::new();
    while at < end && !bytes[at].is_ascii_whitespace() && bytes[at] != b'/' && bytes[at] != b'>' {
        at += 1;
    }
    loop {
        while at < end && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if at >= end || bytes[at] == b'/' || bytes[at] == b'>' {
            break;
        }
        let name_start = at;
        while at < end && bytes[at] != b'=' && !bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        let name = text[name_start..at].to_owned();
        while at < end && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if at >= end || bytes[at] != b'=' {
            break;
        }
        at += 1;
        while at < end && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if at >= end || (bytes[at] != b'"' && bytes[at] != b'\'') {
            break;
        }
        let quote = bytes[at];
        let value_start = at + 1;
        let mut close = value_start;
        while close < end && bytes[close] != quote {
            close += 1;
        }
        if close >= end {
            break;
        }
        found.push(Attr { name, start: name_start, value_start, value_end: close, end: close + 1 });
        at = close + 1;
    }
    found
}

/// Well-formed default-namespace changelog only: a DOCTYPE, a foreign root, a prefixed element name, duplicate
/// attributes or an unclosed element refuse the whole document.
fn parse(text: &str) -> Result<Parsed, String> {
    let mut reader = NsReader::from_str(text);
    let mut els: Vec<El> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    let mut rooted = false;
    for _ in 0..MAX_EVENTS {
        let before = reader.buffer_position() as usize;
        let (kind, qualified, local, bound) = {
            let (namespace, event) = reader.read_resolved_event().map_err(|_| "the changelog is not well-formed XML".to_owned())?;
            match event {
                Event::Eof => (Kind::Eof, String::new(), String::new(), false),
                Event::DocType(_) => return Err("a DOCTYPE is not supported".into()),
                Event::End(_) => (Kind::End, String::new(), String::new(), false),
                Event::Start(ref element) | Event::Empty(ref element) => {
                    for attribute in element.attributes() {
                        attribute.map_err(|_| "the changelog has invalid or duplicate attributes".to_owned())?;
                    }
                    let bound = matches!(&namespace, ResolveResult::Bound(ns) if ns.as_ref() == NAMESPACE);
                    let kind = if matches!(event, Event::Empty(_)) { Kind::Empty } else { Kind::Start };
                    (kind, element.name().as_ref().to_owned(), element.local_name().as_ref().to_owned(), bound)
                }
                _ => (Kind::Skip, String::new(), String::new(), false),
            }
        };
        let after = reader.buffer_position() as usize;
        match kind {
            Kind::Eof => return if rooted && open.is_empty() { Ok(Parsed { els }) } else { Err("the changelog has no closed databaseChangeLog root".into()) },
            Kind::Skip => continue,
            Kind::End => {
                let closed = open.pop().ok_or_else(|| "unbalanced XML".to_owned())?;
                els[closed].end = after;
                continue;
            }
            Kind::Start | Kind::Empty => {}
        }
        let name = if bound && qualified == local { local } else { format!("{{other}}{qualified}") };
        if open.is_empty() && (rooted || name != "databaseChangeLog") {
            return Err("the root is not a default-namespace databaseChangeLog".into());
        }
        rooted = true;
        els.push(El { name, start: before, tag_end: after, end: after, parent: open.last().copied(), attrs: scan_attrs(text, before, after) });
        if kind == Kind::Start {
            open.push(els.len() - 1);
        }
    }
    Err("the changelog has too many XML events".into())
}

impl Parsed {
    fn chain(&self, at: usize) -> Vec<usize> {
        let mut found = None;
        for (index, element) in self.els.iter().enumerate() {
            if element.start <= at && at < element.end {
                found = Some(index);
            }
        }
        let mut chain = Vec::new();
        let mut next = found;
        while let Some(index) = next {
            chain.push(index);
            next = self.els[index].parent;
        }
        chain
    }

    fn parent_is(&self, index: usize, name: &str) -> bool {
        self.els[index].parent.is_some_and(|parent| self.els[parent].name == name)
    }

    fn changeset_at(&self, at: usize) -> Option<usize> {
        self.chain(at).into_iter().find(|&index| self.els[index].name == "changeSet" && self.parent_is(index, "databaseChangeLog"))
    }

    fn attr<'a>(&'a self, text: &'a str, index: usize, name: &str) -> Option<&'a str> {
        self.els[index].attrs.iter().find(|attr| attr.name == name).map(|attr| &text[attr.value_start..attr.value_end])
    }

    fn change_children(&self, changeset: usize) -> usize {
        self.els.iter().filter(|el| el.parent == Some(changeset) && !NON_CHANGE_CHILDREN.contains(&el.name.as_str())).count()
    }

    fn target(&self, item: Item, at: usize) -> Option<usize> {
        let chain = self.chain(at);
        match item {
            Item::Dbms | Item::Contexts | Item::Labels | Item::RunOptions | Item::RemoveChangeSet => self.changeset_at(at),
            Item::PreconditionOutcome => chain.into_iter().find(|&index| {
                self.els[index].name == "preConditions" && (self.parent_is(index, "databaseChangeLog") || (self.parent_is(index, "changeSet") && self.els[index].parent.is_some_and(|set| self.parent_is(set, "databaseChangeLog"))))
            }),
            Item::ColumnType => {
                self.changeset_at(at)?;
                chain.into_iter().find(|&index| match self.els[index].name.as_str() {
                    "column" => self.parent_is(index, "createTable") || self.parent_is(index, "addColumn"),
                    "modifyDataType" | "addNotNullConstraint" => true,
                    _ => false,
                })
            }
            Item::SequenceName => {
                self.changeset_at(at)?;
                chain.into_iter().find(|&index| SEQUENCE_ELEMENTS.contains(&self.els[index].name.as_str()))
            }
            Item::RemoveChange => {
                let changeset = self.changeset_at(at)?;
                let change = chain.into_iter().find(|&index| self.els[index].parent == Some(changeset) && !NON_CHANGE_CHILDREN.contains(&self.els[index].name.as_str()))?;
                (self.change_children(changeset) >= 2).then_some(change)
            }
        }
    }

    /// The attribute that holds the column type of this element.
    fn type_attribute(&self, index: usize) -> &'static str {
        match self.els[index].name.as_str() {
            "modifyDataType" => "newDataType",
            "addNotNullConstraint" => "columnDataType",
            _ => "type",
        }
    }
}

fn target_at(parsed: &Parsed, item: Item, text: &str, selection: Range) -> Option<usize> {
    let from = offset(text, selection.start)?;
    let to = offset(text, selection.end)?;
    let first = parsed.target(item, from)?;
    let second = parsed.target(item, to.saturating_sub(1).max(from))?;
    (first == second).then_some(first)
}

fn name_end(text: &str, element: &El) -> usize {
    let tail = &text[element.start + 1..element.tag_end];
    element.start + 1 + tail.find(|c: char| c.is_whitespace() || c == '/' || c == '>').unwrap_or(tail.len())
}

/// `(start, end, replacement)` that sets, replaces or removes one attribute of the start tag.
fn attribute_edit(text: &str, parsed: &Parsed, index: usize, name: &str, value: Option<&str>) -> Option<(usize, usize, String)> {
    let element = &parsed.els[index];
    let existing = element.attrs.iter().find(|attr| attr.name == name);
    match (existing, value) {
        (Some(attr), Some(value)) => Some((attr.value_start, attr.value_end, value.to_owned())),
        (Some(attr), None) => {
            let mut start = attr.start;
            while start > element.start + 1 && text.as_bytes()[start - 1].is_ascii_whitespace() {
                start -= 1;
            }
            Some((start, attr.end, String::new()))
        }
        (None, Some(value)) => {
            let at = element.attrs.last().map_or_else(|| name_end(text, element), |last| last.end);
            Some((at, at, format!(" {name}=\"{value}\"")))
        }
        (None, None) => None,
    }
}

fn removal(text: &str, element: &El) -> (usize, usize) {
    let line_start = text[..element.start].rfind('\n').map_or(0, |at| at + 1);
    let line_end = text[element.end..].find('\n').map_or(text.len(), |at| element.end + at);
    let blank_before = text[line_start..element.start].chars().all(|c| c == ' ' || c == '\t');
    let blank_after = text[element.end..line_end].chars().all(|c| c == ' ' || c == '\t' || c == '\r');
    if blank_before && blank_after {
        (line_start, (line_end + 1).min(text.len()))
    } else {
        (element.start, element.end)
    }
}

fn splice(text: &str, edit: &(usize, usize, String)) -> String {
    let mut after = text.to_owned();
    after.replace_range(edit.0..edit.1, &edit.2);
    after
}

fn name_chars(value: &str, extra: &str) -> bool {
    value.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
}

fn dbms_value(raw: &str) -> Result<String, String> {
    let items: Vec<&str> = raw.split(',').map(str::trim).collect();
    if items.iter().any(|item| item.is_empty()) {
        return Err("Enter a comma separated list of database names.".into());
    }
    if items.len() == 1 && matches!(items[0], "all" | "none") {
        return Ok(items[0].to_owned());
    }
    for item in &items {
        let name = item.strip_prefix('!').unwrap_or(item);
        if !DBMS_NAMES.contains(&name) {
            return Err(format!("`{name}` is not in the documented Liquibase database list (all and none stand alone, a leading ! excludes)."));
        }
    }
    Ok(items.join(","))
}

fn context_value(raw: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() || value.len() > 256 || !name_chars(value, "._-, !()") {
        return Err("Contexts use letters, digits, . _ -, and the operators , and or ! ( ) (at most 256 characters).".into());
    }
    let mut depth = 0i32;
    for c in value.chars() {
        depth += i32::from(c == '(') - i32::from(c == ')');
        if depth < 0 {
            return Err("The parentheses of the context expression do not match.".into());
        }
    }
    if depth != 0 {
        return Err("The parentheses of the context expression do not match.".into());
    }
    Ok(value.to_owned())
}

fn labels_value(raw: &str) -> Result<String, String> {
    let items: Vec<&str> = raw.split(',').map(str::trim).collect();
    if items.iter().any(|item| item.is_empty() || !name_chars(item, "._-")) || raw.len() > 256 {
        return Err("Labels are a comma separated list of names made of letters, digits, . _ - (at most 256 characters).".into());
    }
    Ok(items.join(","))
}

fn type_value(raw: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() || value.len() > 64 || !name_chars(value, "_ (),.${}") {
        return Err("A column type uses letters, digits and _ ( ) , . $ { } (at most 64 characters).".into());
    }
    Ok(value.to_owned())
}

fn sequence_value(raw: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() || value.len() > 128 || !name_chars(value, "_.$-") {
        return Err("A sequence name uses letters, digits and _ . $ - (at most 128 characters).".into());
    }
    Ok(value.to_owned())
}

fn split_values(value: &str) -> impl Iterator<Item = String> + '_ {
    value.split(',').map(|part| part.trim().to_owned()).filter(|part| !part.is_empty())
}

/// Distinct values of `attribute` on elements called one of `names`, in document order.
fn used_values(text: &str, parsed: &Parsed, names: &[&str], attribute: &str, listed: bool) -> Vec<String> {
    let mut known: Vec<String> = Vec::new();
    for (index, element) in parsed.els.iter().enumerate() {
        if !names.contains(&element.name.as_str()) {
            continue;
        }
        let Some(value) = parsed.attr(text, index, attribute) else { continue };
        let values: Vec<String> = if listed { split_values(value).collect() } else { vec![value.to_owned()] };
        for value in values {
            if !known.contains(&value) && known.len() < MAX_KNOWN {
                known.push(value);
            }
        }
    }
    known
}

fn defaults_values(attribute: &str) -> Vec<String> {
    let Ok(defaults) = super::template_settings::changeset_defaults() else { return Vec::new() };
    let mut sets = vec![&defaults.all];
    sets.extend(defaults.by_type.iter().map(|(_, set)| set));
    let mut values = Vec::new();
    for set in sets {
        let text = if attribute == "context" { &set.context } else { &set.labels };
        for value in text.iter().flat_map(|text| split_values(text)) {
            if !values.contains(&value) {
                values.push(value);
            }
        }
    }
    values
}

fn known_names(text: &str, parsed: &Parsed, item: Item) -> Vec<String> {
    let mut known = match item {
        Item::Contexts => used_values(text, parsed, &["changeSet"], "context", true),
        Item::Labels => used_values(text, parsed, &["changeSet"], "labels", true),
        Item::SequenceName => used_values(text, parsed, &SEQUENCE_ELEMENTS, "sequenceName", false),
        _ => Vec::new(),
    };
    let more = match item {
        Item::Contexts => defaults_values("context"),
        Item::Labels => defaults_values("labels"),
        _ => Vec::new(),
    };
    for value in more {
        if !known.contains(&value) && known.len() < MAX_KNOWN {
            known.push(value);
        }
    }
    known
}

/// Code actions of the changelog inspector at `selection`; `None` for a document that is not a Liquibase XML changelog.
pub(super) fn actions(generation: &mut Generation, doc: &Document, selection: Range) -> Option<Vec<FileAction>> {
    if doc.language != "xml" || !doc.path.ends_with(".xml") || doc.text.len() > MAX_TEXT || !doc.text.contains("<databaseChangeLog") {
        return None;
    }
    let parsed = parse(&doc.text).ok()?;
    let items: Vec<Item> = ITEMS.into_iter().filter(|item| target_at(&parsed, *item, &doc.text, selection).is_some()).collect();
    if items.is_empty() {
        return Some(Vec::new());
    }
    let root = host::project_of(&doc.path).and_then(|project| project.root().ok()).and_then(|root| path(&root));
    let Some(root) = root else { return Some(Vec::new()) };
    if path(&doc.path).as_deref() != Some(doc.path.as_str()) || !inside(&root, &doc.path) {
        return Some(Vec::new());
    }
    if generation.pending.len() > 8 {
        generation.pending.clear();
    }
    let mut actions = Vec::new();
    for item in items {
        generation.next = generation.next.checked_add(1)?;
        let operation = Operation::LiquibaseInspector(item);
        let data = format!("jpa:{}:{}:{}", operation.key(), generation.next, doc.version);
        generation.pending.insert(data.clone(), Pending { root: root.clone(), doc: doc.clone(), operation, selection });
        actions.push(FileAction { title: item.title().into(), kind: Some("refactor.rewrite".into()), data: Some(data), ..FileAction::default() });
    }
    Some(actions)
}

enum Choice {
    Set(String),
    Remove,
}

fn ask_text(title: &str, current: Option<&str>, known: &[String], removable: bool, validate: fn(&str) -> Result<String, String>) -> Result<Choice, Error> {
    let mut modes = Vec::new();
    if !known.is_empty() {
        modes.push(PickItem::new("known", "Choose from the values listed here"));
    }
    modes.push(PickItem::new("type", "Type a value"));
    if removable && current.is_some() {
        modes.push(PickItem::new("remove", "Remove the attribute"));
    }
    let mode = host::pick(title, &modes)?.ok_or(Error::Cancelled)?;
    match mode.as_str() {
        "remove" => Ok(Choice::Remove),
        "known" => {
            let items: Vec<PickItem> = known.iter().map(|value| PickItem::new(value, value)).collect();
            let picked = host::pick_many(title, &items)?.ok_or(Error::Cancelled)?;
            if picked.is_empty() {
                return Err(Error::Cancelled);
            }
            Ok(Choice::Set(validate(&picked.join(",")).map_err(Error::from)?))
        }
        _ => {
            let typed = host::input(title, current)?.ok_or(Error::Cancelled)?;
            Ok(Choice::Set(validate(&typed).map_err(Error::from)?))
        }
    }
}

fn ask_fixed(title: &str, values: &[&str], removable: bool) -> Result<Choice, Error> {
    let mut items: Vec<PickItem> = values.iter().map(|value| PickItem::new(value, value)).collect();
    if removable {
        items.push(PickItem::new("remove", "Remove the attribute"));
    }
    let picked = host::pick(title, &items)?.ok_or(Error::Cancelled)?;
    Ok(if picked == "remove" { Choice::Remove } else { Choice::Set(picked) })
}

fn ask_column_type(current: Option<&str>) -> Result<Choice, Error> {
    let mut items: Vec<PickItem> = GENERIC_TYPES.iter().map(|value| PickItem::new(value, value)).collect();
    items.push(PickItem::new("varchar-n", "varchar(n)"));
    items.push(PickItem::new("char-n", "char(n)"));
    items.push(PickItem::new("other", "Type another value"));
    let title = format!("Column type (now {})", current.unwrap_or("not set"));
    let picked = host::pick(&title, &items)?.ok_or(Error::Cancelled)?;
    let sized = |kind: &str| -> Result<Choice, Error> {
        let length = host::input(&format!("Length of {kind}"), None)?.ok_or(Error::Cancelled)?;
        let length = length.trim();
        if length.is_empty() || length.len() > 9 || !length.bytes().all(|b| b.is_ascii_digit()) || length.parse::<u64>().unwrap_or(0) == 0 {
            return Err("The length must be a positive whole number.".into());
        }
        Ok(Choice::Set(format!("{kind}({length})")))
    };
    match picked.as_str() {
        "varchar-n" => sized("varchar"),
        "char-n" => sized("char"),
        "other" => Ok(Choice::Set(type_value(&host::input("Column type", current)?.ok_or(Error::Cancelled)?).map_err(Error::from)?)),
        _ => Ok(Choice::Set(picked)),
    }
}

fn shown(value: Option<&str>) -> &str {
    value.unwrap_or("not set")
}

pub(super) fn resolve(pending: &Pending, doc: &Document, item: Item) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let text = doc.text.as_str();
    let parsed = parse(text).map_err(|why| Error::from(format!("Cannot edit the changelog: {why}.")))?;
    let index = target_at(&parsed, item, text, pending.selection).ok_or(Error::Stale)?;
    let element = &parsed.els[index];
    let (edit, subject): ((usize, usize, String), String) = match item {
        Item::RemoveChangeSet | Item::RemoveChange => {
            let (start, end) = removal(text, element);
            ((start, end, String::new()), format!("Remove <{}>", element.name))
        }
        Item::Dbms | Item::Contexts | Item::Labels | Item::SequenceName => {
            let (attribute, validate, removable): (&str, fn(&str) -> Result<String, String>, bool) = match item {
                Item::Dbms => ("dbms", dbms_value, true),
                Item::Contexts => ("context", context_value, true),
                Item::Labels => ("labels", labels_value, true),
                _ => ("sequenceName", sequence_value, false),
            };
            let current_value = parsed.attr(text, index, attribute);
            let known = if item == Item::Dbms { DBMS_NAMES.iter().map(|name| (*name).to_owned()).collect() } else { known_names(text, &parsed, item) };
            let title = format!("{} (now {})", item.title().trim_end_matches('…'), shown(current_value));
            let choice = ask_text(&title, current_value, &known, removable, validate)?;
            change_attribute(text, &parsed, index, attribute, choice)?
        }
        Item::RunOptions => {
            let options: Vec<PickItem> = RUN_FLAGS.iter().chain(["onValidationFail"].iter()).map(|name| PickItem::new(name, &format!("{name} (now {})", shown(parsed.attr(text, index, name))))).collect();
            let attribute = host::pick("changeSet attribute to set", &options)?.ok_or(Error::Cancelled)?;
            let values: &[&str] = if attribute == "onValidationFail" { &VALIDATION_FAIL } else { &["true", "false"] };
            if !RUN_FLAGS.contains(&attribute.as_str()) && attribute != "onValidationFail" {
                return Err(Error::Stale);
            }
            let choice = ask_fixed(&format!("Value of {attribute}"), values, parsed.attr(text, index, &attribute).is_some())?;
            change_attribute(text, &parsed, index, &attribute, choice)?
        }
        Item::PreconditionOutcome => {
            let options: Vec<PickItem> = ["onFail", "onError"].iter().map(|name| PickItem::new(name, &format!("{name} (now {})", shown(parsed.attr(text, index, name))))).collect();
            let attribute = host::pick("preConditions attribute to set", &options)?.ok_or(Error::Cancelled)?;
            if attribute != "onFail" && attribute != "onError" {
                return Err(Error::Stale);
            }
            let choice = ask_fixed(&format!("Value of {attribute}"), &PRECONDITION_ACTIONS, parsed.attr(text, index, &attribute).is_some())?;
            if let Choice::Set(value) = &choice {
                if matches!(value.as_str(), "CONTINUE" | "MARK_RAN") && parsed.parent_is(index, "databaseChangeLog") {
                    return Err("CONTINUE and MARK_RAN apply only to preConditions inside a changeSet.".into());
                }
            }
            change_attribute(text, &parsed, index, &attribute, choice)?
        }
        Item::ColumnType => {
            let attribute = parsed.type_attribute(index);
            let choice = ask_column_type(parsed.attr(text, index, attribute))?;
            change_attribute(text, &parsed, index, attribute, choice)?
        }
    };
    let after = splice(text, &edit);
    if after == text {
        return Err("The changelog already has this value; nothing to change.".into());
    }
    let reparsed = parse(&after).map_err(|why| Error::from(format!("The edit would leave an invalid changelog: {why}.")))?;
    let expected = if matches!(item, Item::RemoveChangeSet | Item::RemoveChange) { parsed.els.len() - removed_elements(&parsed, index) } else { parsed.els.len() };
    if reparsed.els.len() != expected {
        return Err("The edit would change the structure of the changelog.".into());
    }
    if after.len() > MAX_TEXT {
        return Err("The edited changelog exceeds the supported size.".into());
    }
    let edits = vec![TextEdit { range: Range::new(position(text, edit.0), position(text, edit.1)), text: edit.2.clone() }];
    let note = "Only this document changes; contexts, labels and names are not checked against other files.";
    if !ui_kit::prompts::confirm_exact(&format!("{}?", subject), &format!("Target: {}\nSource version: {}\n{note}\n\n{after}", doc.path, doc.version))? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    Ok(FileAction { title: item.title().trim_end_matches('…').into(), kind: Some("refactor.rewrite".into()), edits: vec![(doc.path.clone(), edits)], ..FileAction::default() })
}

fn removed_elements(parsed: &Parsed, index: usize) -> usize {
    let target = &parsed.els[index];
    parsed.els.iter().filter(|el| el.start >= target.start && el.end <= target.end).count()
}

fn change_attribute(text: &str, parsed: &Parsed, index: usize, attribute: &str, choice: Choice) -> Result<((usize, usize, String), String), Error> {
    let value = match &choice {
        Choice::Set(value) => Some(value.as_str()),
        Choice::Remove => None,
    };
    let edit = attribute_edit(text, parsed, index, attribute, value).ok_or_else(|| Error::from("The attribute is not set; nothing to remove."))?;
    let subject = match value {
        Some(value) => format!("Set {attribute} to {value}"),
        None => format!("Remove {attribute}"),
    };
    Ok((edit, subject))
}

/// Attribute value under the caret: element name, attribute name, offset where the value starts.
fn value_context(text: &str, caret: usize) -> Option<(String, String, usize)> {
    let tag_start = text[..caret].rfind('<')?;
    let tag = &text[tag_start..caret];
    if tag.contains('>') || tag.starts_with("<!") || tag.starts_with("<?") || tag.starts_with("</") {
        return None;
    }
    let quote = tag.rfind('"')?;
    if tag[..quote].matches('"').count() % 2 != 0 {
        return None;
    }
    let attr = tag[..quote].trim_end().strip_suffix('=')?.trim_end().rsplit(|c: char| c.is_whitespace()).next()?;
    let element = tag[1..].split(|c: char| c.is_whitespace() || c == '/').next()?;
    Some((element.to_owned(), attr.to_owned(), tag_start + quote + 1))
}

fn inside_open(text: &str, before: usize, names: &[&str]) -> bool {
    let last = |marker: String| text[..before].rfind(&marker);
    let boundary = |at: usize, len: usize| text[at + len..].chars().next().is_some_and(|c| c.is_whitespace() || c == '>' || c == '/');
    names.iter().any(|name| {
        let opened = last(format!("<{name}")).filter(|at| boundary(*at, name.len() + 1));
        let closed = last(format!("</{name}"));
        opened.is_some_and(|open| closed.is_none_or(|close| close < open))
    })
}

/// Values for the attribute whose value is being typed: changeSet `dbms`, `context`, `labels`, the run attributes and
/// `onValidationFail`, `preConditions` `onFail` / `onError`, column types and sequence names. Only the document is
/// read. `None`: the caret is not in one of those values.
pub(crate) fn complete(doc: &Document, at: Position) -> Option<Vec<CompletionItem>> {
    if !doc.text.contains("<databaseChangeLog") || doc.text.len() > MAX_TEXT {
        return None;
    }
    let caret = offset(&doc.text, at)?;
    let (element, attribute, value_start) = value_context(&doc.text, caret)?;
    let tag_start = doc.text[..caret].rfind('<')?;
    let listed = matches!((element.as_str(), attribute.as_str()), ("changeSet", "dbms" | "context" | "labels"));
    let mut values: Vec<(String, &str)> = match (element.as_str(), attribute.as_str()) {
        ("changeSet", "dbms") => DBMS_NAMES.iter().chain(["all", "none"].iter()).map(|name| ((*name).to_owned(), "Liquibase database")).collect(),
        ("changeSet", "context" | "labels") => {
            let item = if attribute == "context" { Item::Contexts } else { Item::Labels };
            let scan = lenient_values(&doc.text, tag_start, &attribute);
            let mut known = scan;
            if let Ok(parsed) = parse(&doc.text) {
                known.extend(known_names(&doc.text, &parsed, item));
            } else {
                known.extend(defaults_values(&attribute));
            }
            known.sort();
            known.dedup();
            known.into_iter().map(|name| (name, "used in this changelog")).collect()
        }
        ("changeSet", flag) if RUN_FLAGS.contains(&flag) => vec![("true".to_owned(), "boolean"), ("false".to_owned(), "boolean")],
        ("changeSet", "onValidationFail") => VALIDATION_FAIL.iter().map(|value| ((*value).to_owned(), "onValidationFail")).collect(),
        ("preConditions", "onFail" | "onError") => PRECONDITION_ACTIONS.iter().map(|value| ((*value).to_owned(), "precondition outcome")).collect(),
        ("modifyDataType", "newDataType") | ("addNotNullConstraint", "columnDataType") => GENERIC_TYPES.iter().map(|value| ((*value).to_owned(), "generic type")).collect(),
        ("column", "type") if inside_open(&doc.text, tag_start, &["createTable", "addColumn"]) => GENERIC_TYPES.iter().map(|value| ((*value).to_owned(), "generic type")).collect(),
        (name, "sequenceName") if SEQUENCE_ELEMENTS.contains(&name) => lenient_values(&doc.text, tag_start, "sequenceName").into_iter().map(|value| (value, "used in this changelog")).collect(),
        _ => return None,
    };
    values.truncate(200);
    let typed_from = if listed {
        let typed = &doc.text[value_start..caret];
        let after_comma = typed.rfind(',').map_or(0, |comma| comma + 1);
        let blanks = typed[after_comma..].len() - typed[after_comma..].trim_start().len();
        let negation = usize::from(attribute == "dbms" && typed[after_comma + blanks..].starts_with('!'));
        value_start + after_comma + blanks + negation
    } else {
        value_start
    };
    let typed = doc.text[typed_from..caret].to_lowercase();
    let replace = Range::new(position(&doc.text, typed_from), at);
    let mut items: Vec<CompletionItem> = values
        .into_iter()
        .filter(|(name, _)| name.to_lowercase().starts_with(&typed))
        .map(|(name, detail)| {
            let mut item = CompletionItem::new(&name, CompletionKind::Value);
            item.detail = Some(detail.into());
            item.replace = Some(replace);
            item
        })
        .collect();
    items.sort_by(|a, b| a.label.cmp(&b.label));
    Some(items)
}

/// Values of `attribute` found by a plain scan of the document, so completion still works while the tag being typed
/// leaves the XML unparseable. The tag under the caret is skipped.
fn lenient_values(text: &str, skip_from: usize, attribute: &str) -> Vec<String> {
    let needle = format!("{attribute}=\"");
    let mut found: Vec<String> = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find(&needle) {
        let start = from + at;
        from = start + needle.len();
        let boundary = text[..start].chars().next_back().is_some_and(char::is_whitespace);
        let tag_open = text[..start].rfind('<').unwrap_or(0);
        if !boundary || tag_open == skip_from {
            continue;
        }
        if let Some(end) = text[from..].find('"') {
            for value in split_values(&text[from..from + end]) {
                if !found.contains(&value) && found.len() < MAX_KNOWN {
                    found.push(value);
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const LOG: &str = "<databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\">\n    <changeSet id=\"1\" author=\"a\" context=\"dev\" labels=\"v1,v2\">\n        <createTable tableName=\"t\">\n            <column name=\"c\" type=\"int\"/>\n        </createTable>\n        <createSequence sequenceName=\"s1\"/>\n    </changeSet>\n    <changeSet id=\"2\" author=\"a\">\n        <preConditions onFail=\"HALT\"><tableExists tableName=\"t\"/></preConditions>\n        <modifyDataType tableName=\"t\" columnName=\"c\" newDataType=\"bigint\"/>\n    </changeSet>\n</databaseChangeLog>\n";

    fn at(marker: &str, shift: usize) -> usize {
        LOG.find(marker).unwrap() + shift
    }

    fn applied(item: Item, text: &str, marker_at: usize, attribute: &str, value: Option<&str>) -> Option<String> {
        let parsed = parse(text).ok()?;
        let index = parsed.target(item, marker_at)?;
        Some(splice(text, &attribute_edit(text, &parsed, index, attribute, value)?))
    }

    #[test]
    fn targets_follow_the_caret() {
        let parsed = parse(LOG).unwrap();
        assert!(parsed.target(Item::Dbms, at("<changeSet id=\"1\"", 3)).is_some());
        assert!(parsed.target(Item::Dbms, at("<createTable", 3)).is_some());
        assert!(parsed.target(Item::Dbms, 3).is_none());
        assert!(parsed.target(Item::ColumnType, at("<column", 3)).is_some());
        assert!(parsed.target(Item::ColumnType, at("<createTable", 3)).is_none());
        assert!(parsed.target(Item::ColumnType, at("<modifyDataType", 3)).is_some());
        assert!(parsed.target(Item::SequenceName, at("<createSequence", 3)).is_some());
        assert!(parsed.target(Item::SequenceName, at("<column", 3)).is_none());
        assert!(parsed.target(Item::PreconditionOutcome, at("<tableExists", 3)).is_some());
        assert!(parsed.target(Item::PreconditionOutcome, at("<createTable", 3)).is_none());
        assert!(parsed.target(Item::RemoveChange, at("<createSequence", 3)).is_some());
    }

    #[test]
    fn a_change_alone_in_its_changeset_cannot_be_removed() {
        let single = "<databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\">\n<changeSet id=\"1\" author=\"a\">\n<createTable tableName=\"t\"/>\n</changeSet>\n</databaseChangeLog>";
        let parsed = parse(single).unwrap();
        assert!(parsed.target(Item::RemoveChange, single.find("<createTable").unwrap() + 2).is_none());
        assert!(parsed.target(Item::RemoveChangeSet, single.find("<createTable").unwrap() + 2).is_some());
    }

    #[test]
    fn setting_replacing_and_removing_an_attribute() {
        let caret = at("<changeSet id=\"1\"", 3);
        let added = applied(Item::Dbms, LOG, caret, "dbms", Some("postgresql")).unwrap();
        assert!(added.contains("<changeSet id=\"1\" author=\"a\" context=\"dev\" labels=\"v1,v2\" dbms=\"postgresql\">"));
        let replaced = applied(Item::Contexts, LOG, caret, "context", Some("prod")).unwrap();
        assert!(replaced.contains("context=\"prod\" labels"));
        let removed = applied(Item::Labels, LOG, caret, "labels", None).unwrap();
        assert!(removed.contains("context=\"dev\">"));
        assert!(applied(Item::Dbms, LOG, caret, "dbms", None).is_none());
    }

    #[test]
    fn an_edit_is_idempotent() {
        let caret = at("<changeSet id=\"1\"", 3);
        let once = applied(Item::RunOptions, LOG, caret, "runOnChange", Some("true")).unwrap();
        let twice = applied(Item::RunOptions, &once, caret, "runOnChange", Some("true")).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn an_empty_tag_gets_its_attribute_before_the_slash() {
        let text = "<databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\"><changeSet id=\"1\" author=\"a\"><dropSequence/></changeSet></databaseChangeLog>";
        let parsed = parse(text).unwrap();
        let index = parsed.target(Item::SequenceName, text.find("<dropSequence").unwrap() + 2).unwrap();
        let edit = attribute_edit(text, &parsed, index, "sequenceName", Some("s")).unwrap();
        assert!(splice(text, &edit).contains("<dropSequence sequenceName=\"s\"/>"));
    }

    #[test]
    fn removal_takes_the_whole_line_only_when_the_line_holds_nothing_else() {
        let parsed = parse(LOG).unwrap();
        let index = parsed.target(Item::RemoveChange, at("<createSequence", 3)).unwrap();
        let (start, end) = removal(LOG, &parsed.els[index]);
        let after = splice(LOG, &(start, end, String::new()));
        assert!(!after.contains("createSequence"));
        assert!(after.contains("</createTable>\n    </changeSet>"));
        parse(&after).unwrap();
        let inline = "<databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\"><changeSet id=\"1\" author=\"a\"><a/><b/></changeSet></databaseChangeLog>";
        let parsed = parse(inline).unwrap();
        let (start, end) = removal(inline, &parsed.els[parsed.target(Item::RemoveChange, inline.find("<a/>").unwrap() + 1).unwrap()]);
        assert_eq!(&inline[start..end], "<a/>");
    }

    #[test]
    fn malformed_ambiguous_or_foreign_xml_is_refused() {
        assert!(parse("<databaseChangeLog><changeSet>").is_err());
        assert!(parse("<databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\"><changeSet id=\"1\" id=\"2\"/></databaseChangeLog>").is_err());
        assert!(parse("<!DOCTYPE x><databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\"/>").is_err());
        assert!(parse("<project/>").is_err());
        let foreign = "<databaseChangeLog xmlns=\"urn:other\"><changeSet id=\"1\" author=\"a\"/></databaseChangeLog>";
        assert!(parse(foreign).is_err());
        let prefixed = "<databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\"><x:changeSet xmlns:x=\"urn:other\" id=\"1\"/></databaseChangeLog>";
        assert!(parse(prefixed).unwrap().changeset_at(90).is_none());
    }

    #[test]
    fn dbms_accepts_only_the_documented_names() {
        assert_eq!(dbms_value("postgresql, !h2").unwrap(), "postgresql,!h2");
        assert_eq!(dbms_value("all").unwrap(), "all");
        assert!(dbms_value("all,h2").is_err());
        assert!(dbms_value("postgres").is_err());
        assert!(dbms_value("h2,").is_err());
        assert!(dbms_value("!none").is_err());
    }

    #[test]
    fn contexts_labels_types_and_names_are_xml_safe() {
        assert!(context_value("dev, !qa and (a or b)").is_ok());
        assert!(context_value("a\"b").is_err());
        assert!(context_value("(a").is_err());
        assert!(context_value("a<b").is_err());
        assert_eq!(labels_value("v1, v2").unwrap(), "v1,v2");
        assert!(labels_value("a and b").is_err());
        assert!(labels_value("a,,b").is_err());
        assert!(type_value("decimal(10,2)").is_ok());
        assert!(type_value("a\"b").is_err());
        assert!(type_value("a&b").is_err());
        assert!(sequence_value("seq_1.x").is_ok());
        assert!(sequence_value("a b").is_err());
    }

    #[test]
    fn column_type_attribute_follows_the_element() {
        let parsed = parse(LOG).unwrap();
        assert_eq!(parsed.type_attribute(parsed.target(Item::ColumnType, at("<column", 3)).unwrap()), "type");
        assert_eq!(parsed.type_attribute(parsed.target(Item::ColumnType, at("<modifyDataType", 3)).unwrap()), "newDataType");
    }

    #[test]
    fn used_values_come_from_the_document_only() {
        let parsed = parse(LOG).unwrap();
        assert_eq!(used_values(LOG, &parsed, &["changeSet"], "labels", true), vec!["v1", "v2"]);
        assert_eq!(used_values(LOG, &parsed, &SEQUENCE_ELEMENTS, "sequenceName", false), vec!["s1"]);
    }

    fn complete_at(text: &str) -> Option<Vec<String>> {
        let _host = MockHost::install();
        let caret = text.find('|').unwrap();
        let text = text.replacen('|', "", 1);
        let doc = Document { path: "/p/x.xml".into(), language: "xml".into(), text: text.clone(), version: 1 };
        complete(&doc, position(&text, caret)).map(|items| items.into_iter().map(|item| item.label).collect())
    }

    #[test]
    fn completion_offers_documented_values_in_the_right_attribute() {
        let head = "<databaseChangeLog>\n<changeSet id=\"1\" author=\"a\" context=\"dev\" labels=\"x\"/>\n";
        let labels = complete_at(&format!("{head}<changeSet id=\"2\" author=\"a\" dbms=\"po|\"")).unwrap();
        assert_eq!(labels, vec!["postgresql"]);
        let negated = complete_at(&format!("{head}<changeSet dbms=\"h2,!my|\"")).unwrap();
        assert_eq!(negated, vec!["mysql"]);
        let context = complete_at(&format!("{head}<changeSet id=\"2\" context=\"|\"")).unwrap();
        assert_eq!(context, vec!["dev"]);
        let fail = complete_at("<databaseChangeLog><preConditions onFail=\"M|\"").unwrap();
        assert_eq!(fail, vec!["MARK_RAN"]);
        let kinds = complete_at("<databaseChangeLog><createTable t=\"x\"><column type=\"|\"").unwrap();
        assert!(kinds.contains(&"bigint".to_owned()));
        assert!(!kinds.iter().any(|label| label.starts_with("decimal")));
    }

    #[test]
    fn completion_stays_quiet_elsewhere() {
        assert!(complete_at("<databaseChangeLog><changeSet id=\"|\"").is_none());
        assert!(complete_at("<databaseChangeLog><changeSet dbms=\"h2\"| ").is_none());
        assert!(complete_at("<databaseChangeLog><column type=\"|\"").is_none());
        assert!(complete_at("<databaseChangeLog><createIndex dbms=\"|\"").is_none());
        let doc = Document { path: "/p/x.xml".into(), language: "xml".into(), text: "<project dbms=\"\"/>".into(), version: 1 };
        assert!(complete(&doc, Position::new(0, 15)).is_none());
    }

    const PATH: &str = "/project/src/main/resources/db/changelog/a.xml";

    fn flow(marker: &str, shift: usize, title: &str, answers: &[Option<&[&str]>]) -> Result<(String, Vec<String>), Error> {
        let h = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        h.open_editor(PATH, "xml", LOG, &[]);
        let doc = Document { path: PATH.into(), language: "xml".into(), text: LOG.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let at = position(LOG, LOG.find(marker).unwrap() + shift);
        let offered = plugin.file_actions(&doc, Range::new(at, at));
        let titles: Vec<String> = offered.iter().map(|a| a.title.clone()).collect();
        let action = offered.into_iter().find(|a| a.title == title).ok_or_else(|| Error::from(format!("not offered: {titles:?}")))?;
        for answer in answers {
            h.answer_next_prompt(*answer);
        }
        let resolved = plugin.resolve_file_action(&doc, action)?;
        assert_eq!(resolved.edits.len(), 1);
        assert_eq!(resolved.edits[0].0, PATH);
        let edit = &resolved.edits[0].1;
        let mut after = LOG.to_owned();
        for e in edit.iter().rev() {
            after.replace_range(offset(LOG, e.range.start).unwrap()..offset(LOG, e.range.end).unwrap(), &e.text);
        }
        Ok((after, titles))
    }

    #[test]
    fn dbms_is_set_from_the_documented_list_after_review() {
        let (after, titles) = flow("<changeSet id=\"2\"", 3, "Set changeSet dbms…", &[Some(&["known"]), Some(&["postgresql", "h2"]), Some(&[])]).unwrap();
        assert!(after.contains("<changeSet id=\"2\" author=\"a\" dbms=\"postgresql,h2\">"));
        assert!(titles.contains(&"Remove this changeSet…".to_owned()));
        assert!(flow("<changeSet id=\"2\"", 3, "Set changeSet dbms…", &[Some(&["type"]), Some(&["postgres"]), Some(&[])]).is_err());
        assert!(matches!(flow("<changeSet id=\"2\"", 3, "Set changeSet dbms…", &[Some(&["type"]), Some(&["h2"]), None]), Err(Error::Cancelled)));
    }

    #[test]
    fn contexts_offer_the_values_of_this_document() {
        let (after, _) = flow("<changeSet id=\"2\"", 3, "Set changeSet contexts…", &[Some(&["known"]), Some(&["dev"]), Some(&[])]).unwrap();
        assert!(after.contains("<changeSet id=\"2\" author=\"a\" context=\"dev\">"));
    }

    #[test]
    fn setting_the_current_value_changes_nothing() {
        let error = flow("<changeSet id=\"1\"", 3, "Set changeSet contexts…", &[Some(&["type"]), Some(&["dev"])]).unwrap_err();
        assert!(format!("{error:?}").contains("nothing to change") || error.to_string().contains("nothing to change"));
    }

    #[test]
    fn run_options_and_precondition_outcomes() {
        let (after, _) = flow("<changeSet id=\"1\"", 3, "Set changeSet run options…", &[Some(&["runOnChange"]), Some(&["true"]), Some(&[])]).unwrap();
        assert!(after.contains("labels=\"v1,v2\" runOnChange=\"true\">"));
        let (after, _) = flow("<tableExists", 3, "Set preConditions onFail and onError…", &[Some(&["onError"]), Some(&["WARN"]), Some(&[])]).unwrap();
        assert!(after.contains("<preConditions onFail=\"HALT\" onError=\"WARN\">"));
        let (after, _) = flow("<tableExists", 3, "Set preConditions onFail and onError…", &[Some(&["onFail"]), Some(&["remove"]), Some(&[])]).unwrap();
        assert!(after.contains("<preConditions><tableExists"));
    }

    #[test]
    fn column_type_from_the_generic_list_or_with_a_length() {
        let (after, _) = flow("<column", 3, "Set column type…", &[Some(&["bigint"]), Some(&[])]).unwrap();
        assert!(after.contains("<column name=\"c\" type=\"bigint\"/>"));
        let (after, _) = flow("<modifyDataType", 3, "Set column type…", &[Some(&["varchar-n"]), Some(&["40"]), Some(&[])]).unwrap();
        assert!(after.contains("newDataType=\"varchar(40)\""));
        assert!(flow("<column", 3, "Set column type…", &[Some(&["varchar-n"]), Some(&["0"]), Some(&[])]).is_err());
        assert!(flow("<column", 3, "Set column type…", &[Some(&["other"]), Some(&["a\"b"]), Some(&[])]).is_err());
    }

    #[test]
    fn sequence_name_and_removal() {
        let (after, _) = flow("<createSequence", 3, "Set sequence name…", &[Some(&["type"]), Some(&["orders_seq"]), Some(&[])]).unwrap();
        assert!(after.contains("<createSequence sequenceName=\"orders_seq\"/>"));
        let (after, _) = flow("<createSequence", 3, "Remove this change…", &[Some(&[])]).unwrap();
        assert!(!after.contains("createSequence") && after.contains("</createTable>\n    </changeSet>"));
        let (after, _) = flow("<changeSet id=\"2\"", 3, "Remove this changeSet…", &[Some(&[])]).unwrap();
        assert!(!after.contains("modifyDataType") && after.ends_with("    </changeSet>\n</databaseChangeLog>\n"));
    }

    #[test]
    fn nothing_is_offered_outside_a_default_namespace_changelog() {
        let h = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        for text in ["<project><changeSet id=\"1\"/></project>", "<databaseChangeLog><changeSet id=\"1\"/></databaseChangeLog>"] {
            h.open_editor(PATH, "xml", text, &[]);
            let doc = Document { path: PATH.into(), language: "xml".into(), text: text.into(), version: 1 };
            let at = position(text, text.find("changeSet").unwrap() + 2);
            assert!(JpaAssist::default().file_actions(&doc, Range::new(at, at)).iter().all(|a| !a.title.starts_with("Set ")));
        }
    }
}
