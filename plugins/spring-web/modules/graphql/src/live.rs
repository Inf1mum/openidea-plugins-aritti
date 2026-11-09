use super::*;
impl GraphQlTools {
    pub fn live_diagnostics(&mut self, doc: &Document, allow_absence: bool) -> Vec<Diagnostic> {
        let _ = self.java.for_cached_document(&doc.path, &doc.text);
        let _ = self.wiring.for_cached_document(&doc.path, &doc.text);
        let mut out = Vec::new();
        let Some(schemas) = self.schemas.for_cached_document(&doc.path, &doc.text) else {
            return out;
        };
        if is_code(doc) {
            for h in handlers_in(&doc.text, doc.language == "kotlin") {
                let ty = type_of(&h, schemas);
                let found = schema_fields(schemas, &ty, &h.field);
                if found.is_empty() {
                    let known = schemas.iter().any(|(_, s)| s.types.iter().any(|t| t.name == ty));
                    if allow_absence
                        && schemas.fresh()
                        && known
                        && host::setting_bool("springGraphql.checkUnmatchedMapping").unwrap_or(true)
                    {
                        out.push(
                            Diagnostic::new(
                                h.range,
                                Severity::Warning,
                                format!("The GraphQL schema declares no field `{}` on type `{ty}`.", h.field),
                            )
                            .code("unknown-schema-field"),
                        );
                    }
                } else if allow_absence && schemas.fresh() && found.len() == 1 {
                    for (name, at) in &h.args {
                        if !found[0].1.args.contains(name) {
                            out.push(
                                Diagnostic::new(*at, Severity::Warning, format!("`{ty}.{}` has no argument `{name}`.", h.field))
                                    .code("unknown-graphql-argument"),
                            );
                        }
                    }
                }
            }
            if host::setting_bool("springGraphql.checkBatchMappingReturn").unwrap_or(true) {
                out.extend(
                    checks::batch_mapping_return(&spring_text::parse_source(&doc.path, &doc.text), &doc.text)
                        .iter()
                        .map(|f| f.diagnostic(&doc.text)),
                );
            }
        } else {
            let Some(java) = self.java.for_cached_document(&doc.path, &doc.text) else {
                return out;
            };
            let Some(wiring) = self.wiring.for_cached_document(&doc.path, &doc.text) else {
                return out;
            };
            if !allow_absence || !java.fresh() || !schemas.fresh() || !wiring.fresh() || wiring.iter().any(|(_, wired)| *wired) {
                return out;
            }
            let roots: Vec<_> = ROOTS.iter().map(|r| root_type(schemas, r.1)).collect();
            for ty in parse_schema(&doc.text).types {
                if ty.kind != "type" || !roots.contains(&ty.name) {
                    continue;
                }
                for field in ty.fields {
                    if !java.iter().flat_map(|(_, hs)| hs).any(|h| h.field == field.name && type_of(h, schemas) == ty.name) {
                        out.push(
                            Diagnostic::new(
                                field.range,
                                Severity::Hint,
                                format!("No cached source controller method handles `{}.{}`.", ty.name, field.name),
                            )
                            .code("unmapped-root-field"),
                        );
                    }
                }
            }
        }
        out
    }
    pub fn live_hover(&mut self, doc: &Document, at: Position) -> Option<openidea_plugin::hover::Hover> {
        if doc.language=="java" {return super::mapping_docs::hover(doc,at);}
        if doc.language != "graphql" {return None;}
        let schema = parse_schema(&doc.text);
        for ty in schema.types {
            for field in ty.fields {
                if contains(field.range, at) && !field.ty.is_empty() && field.name.len() <= 128 && field.ty.len() <= 128 {
                    return Some(openidea_plugin::hover::Hover::new(format!("GraphQL schema declaration `{}.{}`\n\nNamed field type: `{}`.\n\nArguments: {}.\n\nSource declaration only; list/non-null wrappers and runtime resolver behavior are not inferred.",ty.name,field.name,field.ty,if field.args.is_empty(){"none".into()}else{field.args.join(", ")})).range(field.range));
                }
            }
        }
        None
    }
}
