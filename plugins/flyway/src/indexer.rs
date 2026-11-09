//! Migration/table/class associations are not interchangeable Rename symbols.
//! Keep the export empty to clear older entries; read-only navigation is separate.
use crate::Flyway;
use openidea_plugin::indexing::{Document, DocumentIndexer, Edges};
impl DocumentIndexer for Flyway {
    fn index(&mut self, _doc: &Document) -> Edges {
        Edges::default()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placeholders_transformed_entities_and_comment_tables_never_enter_rename() {
        for (path, text) in [
            ("/project/flyway.conf", "flyway.placeholders.seed=42"),
            ("/project/UserAccount.java", "@Entity class UserAccount {}"),
            ("/project/V1__init.sql", "CREATE TABLE user_account (id int); -- CREATE TABLE phantom (x int)"),
        ] {
            let e = Flyway::default().index(&Document { path: path.into(), text: text.into(), symbols: vec![] });
            assert!(e.definitions.is_empty() && e.references.is_empty() && e.markers.is_empty() && e.aliases.is_empty());
        }
    }
}
