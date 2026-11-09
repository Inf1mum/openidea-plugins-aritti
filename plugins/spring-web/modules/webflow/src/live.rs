use super::*;
impl WebFlow {
    pub fn live_diagnostics(&mut self, doc: &Document, allow_absence: bool) -> Vec<Diagnostic> {
        let _ = self.indexes.for_cached_document(&doc.path, &doc.text);
        let Some(flow) = parse_flow(&doc.text) else {
            return Vec::new();
        };
        let index = self.indexes.for_cached_document(&doc.path, &doc.text).filter(|i| allow_absence && i.fresh());
        problems(&doc.path, &flow, &World::new(index.map(|i| &*i)))
    }
}
