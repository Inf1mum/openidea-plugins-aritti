use super::*;
impl Messaging {
    pub fn live_diagnostics(&mut self, doc: &Document, allow_absence: bool) -> Vec<openidea_plugin::Diagnostic> {
        let _ = self.listener_beans.for_cached_document(&doc.path, &doc.text);
        if doc.language != "xml" {
            return Vec::new();
        }
        let Some(index) = self.listener_beans.for_cached_document(&doc.path, &doc.text) else {
            return Vec::new();
        };
        let mut out = listener_xml::problems(&doc.text, index);
        if !allow_absence || !index.fresh() {
            out.retain(|d| !matches!(d.code.as_deref(), Some("unresolved-reference" | "unresolved-listener-method")));
        }
        let elements = xml::elements(&doc.text);
        out.retain(|d| {
            openidea_plugin::text::offset(&doc.text, d.range.start).and_then(|at| xml::element_at(&elements, at)).is_some_and(|i| {
                matches!(
                    xml::namespace(&elements, i),
                    Some("http://www.springframework.org/schema/jms" | "http://www.springframework.org/schema/rabbit")
                )
            })
        });
        out
    }
    pub fn live_hover(&mut self, doc: &Document, at: Position) -> Option<openidea_plugin::hover::Hover> {
        if doc.language=="java" {return super::destination_docs::hover(doc,at);}
        // Namespace-qualified XML declarations are source facts; template receiver names alone are not broker proof.
        if doc.language != "xml" {
            return None;
        }
        let offset = offset(&doc.text, at)?;
        let elements = xml::elements(&doc.text);
        let element = xml::element_at(&elements, offset)?;
        if !matches!(
            xml::namespace(&elements, element),
            Some("http://www.springframework.org/schema/jms" | "http://www.springframework.org/schema/rabbit")
        ) {
            return None;
        }
        let endpoints = parse(&doc.path, &doc.text)?;
        let hits: Vec<_> = endpoints
            .iter()
            .filter(|e| e.holds(offset) && !e.name.contains(['$', '#', '{', '}']) && !e.name.starts_with(CONSTANT))
            .collect();
        if hits.len() != 1 {
            return None;
        }
        let e = hits[0];
        if e.name.len() > 256 || e.name.contains(['`', '\n', '\r']) {
            return None;
        }
        Some(openidea_plugin::hover::Hover::new(format!("{} {} declaration\n\nDestination: `{}`.\n\nSource XML declaration only; broker availability, delivery and runtime routing are not inferred.",e.broker.label(),e.space,e.name)).range(e.range))
    }
}
