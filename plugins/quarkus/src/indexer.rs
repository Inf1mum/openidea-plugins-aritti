//! Former config/Java associations used normalized, profile-stripped and
//! transformed names. Host indexes also drive Rename, so they are withheld.
//! Read-only exact-source navigation lives in navigation.rs instead.
use crate::QuarkusTools;
use openidea_plugin::indexing::{Document, DocumentIndexer, Edges};
impl DocumentIndexer for QuarkusTools {
    fn index(&mut self, _doc: &Document) -> Edges {
        Edges::default()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transformed_profile_named_and_unowned_keys_never_register_rename_anchors() {
        for (path, text) in [
            ("/project/A.java", "@ConfigMapping(prefix=\"server\") interface A {String hostName();}"),
            ("/project/application.properties", "%dev.server.host-name=x\nquarkus.datasource.\"named\".jdbc.url=x\n"),
            ("/other/application.yaml", "server:\n  host-name: x\n"),
        ] {
            let edges = QuarkusTools::default().index(&Document { path: path.into(), text: text.into(), symbols: vec![] });
            assert!(edges.definitions.is_empty() && edges.references.is_empty() && edges.markers.is_empty());
        }
    }
}
