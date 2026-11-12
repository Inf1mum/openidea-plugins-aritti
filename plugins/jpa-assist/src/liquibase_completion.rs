//! Table and column name completion inside Liquibase XML changelogs (G114, G115-style model completion): the values
//! come from the JPA data model (the entity sources' tables and columns), not from a database.
//!
//! Table attributes: `tableName`, `baseTableName`, `referencedTableName`. Column attributes follow the table named in
//! the same tag (`columnName`, `oldColumnName`, `newColumnName`, `baseColumnNames`/`referencedColumnNames` with their
//! base/referenced table) or, for `<column name>`, the nearest enclosing `tableName`.
use super::*;
use crate::schema_snapshot::Scope;
use persistence_core::schema::Fact;
use std::cell::RefCell;

type Tables = Vec<(String, Vec<String>)>;

thread_local! {
    static CACHE: RefCell<HashMap<String, Tables>> = RefCell::new(HashMap::new());
}

pub(crate) fn invalidate(root: &str) {
    CACHE.with(|cache| {
        cache.borrow_mut().remove(root);
    });
}

fn tables_of(project: &Project, root: &str) -> Option<Tables> {
    if let Some(found) = CACHE.with(|cache| cache.borrow().get(root).cloned()) {
        return Some(found);
    }
    let docs = crate::file_actions::migration_action::model_docs(project, root).ok()?;
    let scoped = crate::schema_snapshot::snapshot_configured(&docs, &Scope::All, &crate::naming_strategy::detect(project), &crate::schema_snapshot::EnversConfig::detect(project));
    let tables: Tables = scoped
        .snapshot
        .tables
        .iter()
        .map(|table| {
            let columns = match &table.columns {
                Fact::Known(columns) => columns.iter().map(|c| c.name.clone()).collect(),
                Fact::Unknown(_) => Vec::new(),
            };
            (table.name.clone(), columns)
        })
        .collect();
    CACHE.with(|cache| cache.borrow_mut().insert(root.to_owned(), tables.clone()));
    Some(tables)
}

enum Wanted {
    Table,
    Columns(String),
}

/// Value of `attr` in a start tag, if it has a closed value.
fn attribute(tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=\"");
    let mut from = 0;
    while let Some(found) = tag[from..].find(&needle) {
        let at = from + found;
        let boundary = tag[..at].chars().next_back().is_none_or(|c| c.is_whitespace());
        if boundary {
            let rest = &tag[at + needle.len()..];
            return rest.find('"').map(|end| rest[..end].to_owned());
        }
        from = at + needle.len();
    }
    None
}

fn nearest_table(before_tag: &str) -> Option<String> {
    let limit = before_tag.rfind("<changeSet").unwrap_or(0);
    let scope = &before_tag[limit..];
    let mut best = None;
    let mut from = 0;
    while let Some(found) = scope[from..].find('<') {
        let start = from + found;
        let end = scope[start..].find('>').map_or(scope.len(), |e| start + e);
        let tag = &scope[start..end];
        if !tag.starts_with("</") {
            if let Some(table) = attribute(tag, "tableName") {
                best = Some(table);
            }
        }
        from = start + 1;
    }
    best
}

fn wanted(text: &str, caret: usize) -> Option<(usize, Wanted)> {
    let tag_start = text[..caret].rfind('<')?;
    if text[tag_start..caret].contains('>') || text[tag_start..].starts_with("<!--") || text[tag_start..].starts_with("<?") {
        return None;
    }
    let tag = &text[tag_start..caret];
    let quote = tag.rfind('"')?;
    if tag[..quote].matches('"').count() % 2 != 0 {
        return None;
    }
    let value_start = tag_start + quote + 1;
    let attr_part = tag[..quote].trim_end().strip_suffix('=')?.trim_end();
    let attr = attr_part.rsplit(|c: char| c.is_whitespace()).next()?;
    let full_tag = &text[tag_start..text[tag_start..].find('>').map_or(text.len(), |e| tag_start + e)];
    let wanted = match attr {
        "tableName" | "baseTableName" | "referencedTableName" => Wanted::Table,
        "columnName" | "oldColumnName" | "newColumnName" => Wanted::Columns(attribute(full_tag, "tableName")?),
        "baseColumnNames" => Wanted::Columns(attribute(full_tag, "baseTableName")?),
        "referencedColumnNames" => Wanted::Columns(attribute(full_tag, "referencedTableName")?),
        "name" if full_tag.starts_with("<column") && !full_tag.starts_with("<columns") => Wanted::Columns(attribute(full_tag, "tableName").or_else(|| nearest_table(&text[..tag_start]))?),
        _ => return None,
    };
    Some((value_start, wanted))
}

/// `None`: the document is not a Liquibase changelog or the caret is not in a table or column value.
pub(crate) fn complete(doc: &Document, at: Position) -> Option<Vec<CompletionItem>> {
    if !doc.text.contains("<databaseChangeLog") {
        return None;
    }
    let caret = offset(&doc.text, at)?;
    let (value_start, wanted) = wanted(&doc.text, caret)?;
    let project = project_of(&doc.path)?;
    let root = project.root().ok()?;
    let tables = tables_of(&project, &root)?;
    let typed = doc.text[value_start..caret].to_lowercase();
    let replace = Range::new(position(&doc.text, value_start), at);
    let mut items: Vec<CompletionItem> = match wanted {
        Wanted::Table => tables.iter().map(|(name, _)| (name.clone(), CompletionKind::Class)).collect::<Vec<_>>(),
        Wanted::Columns(table) => tables
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(&table))
            .map(|(_, columns)| columns.iter().map(|c| (c.clone(), CompletionKind::Property)).collect())
            .unwrap_or_default(),
    }
    .into_iter()
    .filter(|(name, _)| name.to_lowercase().starts_with(&typed))
    .map(|(name, kind)| {
        let mut item = CompletionItem::new(&name, kind);
        item.detail = Some("JPA data model".into());
        item.replace = Some(replace);
        item
    })
    .collect();
    items.sort_by(|a, b| a.label.cmp(&b.label));
    Some(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "<databaseChangeLog>\n<changeSet id=\"1\" author=\"a\">\n<createTable tableName=\"cli\">\n<column name=\"\"/>\n</createTable>\n<addForeignKeyConstraint baseTableName=\"invoice\" baseColumnNames=\"x\" referencedTableName=\"client\" referencedColumnNames=\"i\"/>\n</changeSet>\n</databaseChangeLog>";

    fn at(marker: &str, shift: usize) -> usize {
        LOG.find(marker).unwrap() + shift
    }

    #[test]
    fn table_and_column_contexts_are_recognised() {
        let caret = at("tableName=\"cli", 14);
        assert!(matches!(wanted(LOG, caret), Some((_, Wanted::Table))));
        let caret = at("<column name=\"", 14);
        assert!(matches!(wanted(LOG, caret), Some((_, Wanted::Columns(t))) if t == "cli"));
        let caret = at("baseColumnNames=\"x", 18);
        assert!(matches!(wanted(LOG, caret), Some((_, Wanted::Columns(t))) if t == "invoice"));
        let caret = at("referencedColumnNames=\"i", 24);
        assert!(matches!(wanted(LOG, caret), Some((_, Wanted::Columns(t))) if t == "client"));
    }

    #[test]
    fn other_attributes_and_closed_values_are_quiet() {
        assert!(wanted(LOG, at("id=\"1", 5)).is_none());
        assert!(wanted(LOG, at("author=\"a\"", 11)).is_none());
        assert!(wanted(LOG, at("<createTable", 3)).is_none());
    }
}
