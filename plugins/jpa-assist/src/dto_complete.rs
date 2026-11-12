//! Typing a field name that a DTO lacks offers to add it (DTO generator, attribute synchronization).
//! A DTO is recognised by the DTO Declaration settings; its entity must be in the observed-source cache.
use super::*;
use super::projection_links::{Conventions, Kind};
use crate::model::{import_edit, is_ident};
use openidea_plugin::text::{offset, position};

const MAX_TEXT: usize = 128 << 10;

fn word_bounds(text: &str, at: usize) -> (usize, usize) {
    let bytes = text.as_bytes();
    let start = at - bytes[..at].iter().rev().take_while(|&&byte| is_ident(byte)).count();
    let end = at + bytes[at..].iter().take_while(|&&byte| is_ident(byte)).count();
    (start, end)
}

/// Whether a member declaration may begin at `start`: after `(`, `,`, `{`, `;` or first on its line.
fn member_start(text: &str, start: usize) -> bool {
    let before = &text[..start];
    let line = before.rsplit('\n').next().unwrap_or_default();
    if line.trim().is_empty() {
        return true;
    }
    matches!(before.trim_end().chars().next_back(), Some('(' | ',' | '{' | ';'))
}

fn association_type(entity: &Class, attribute: &crate::model::Attribute, postfix: &str, kotlin: bool) -> Option<(String, Option<&'static str>)> {
    let target = attribute.target.rsplit('.').next().filter(|name| !name.is_empty())?;
    let dto = format!("{target}{postfix}");
    match attribute.relation {
        Relation::ManyToOne | Relation::OneToOne => Some((dto, None)),
        Relation::OneToMany | Relation::ManyToMany => {
            let _ = entity;
            Some((format!("List<{dto}>"), (!kotlin).then_some("java.util.List")))
        }
        _ => None,
    }
}

impl JpaAssist {
    pub(crate) fn dto_field_completion(&mut self, doc: &Document, at: Position) -> Vec<CompletionItem> {
        if !matches!(doc.language.as_str(), "java" | "kotlin") || doc.text.len() > MAX_TEXT {
            return vec![];
        }
        let kotlin = doc.language == "kotlin";
        let Some(cursor) = offset(&doc.text, at) else {
            return vec![];
        };
        let Ok(conventions) = Conventions::load_for(Kind::Dto) else {
            return vec![];
        };
        let stem = doc.path.rsplit('/').next().and_then(|file| file.rsplit_once('.')).map(|(stem, _)| stem).unwrap_or_default();
        let Some(entity_name) = conventions.entity_of(stem, &doc.text) else {
            return vec![];
        };
        let (start, end) = word_bounds(&doc.text, cursor);
        if !member_start(&doc.text, start) {
            return vec![];
        }
        let stripped = format!("{}{}", &doc.text[..start], &doc.text[end..]);
        let Ok(Some((dto, _))) = projection_links::parse(&doc.path, &stripped, kotlin) else {
                return vec![];
        };
        if !dto.in_list(start) {
            return vec![];
        }
        let Some(project) = host::project_of(&doc.path) else {
            return vec![];
        };
        let Ok(root) = project.root() else {
            return vec![];
        };
        let Some(entity) = self.repository_completion.entity_by_name(&root, &entity_name).cloned() else {
            return vec![];
        };
        let typed = doc.text[start..cursor].to_lowercase();
        let directory = doc.path.rsplit_once('/').map_or("", |(directory, _)| directory);
        let form = if dto.is_interface() { projection_action::Form::Interface } else { projection_action::Form::Record };
        let mut candidates: Vec<(String, String, Option<&'static str>)> =
            projection_action::offered(&entity, form).into_iter().map(|(name, ty)| (name, ty, None)).collect();
        for attribute in &entity.attributes {
            let Some((ty, import)) = association_type(&entity, attribute, &conventions.postfix, kotlin) else {
                continue;
            };
            let target = ty.trim_start_matches("List<").trim_end_matches('>');
            let exists = ["java", "kt"].iter().any(|extension| project.read_file(&format!("{directory}/{target}.{extension}")).is_ok());
            if exists {
                candidates.push((attribute.name.clone(), ty, import));
            }
        }
        let range = Range::new(position(&doc.text, start), position(&doc.text, end));
        candidates
            .into_iter()
            .filter(|(name, _, _)| !dto.has_property(name) && name.to_lowercase().starts_with(&typed))
            .take(128)
            .map(|(name, ty, import)| {
                let imports: Vec<String> = import.map(str::to_owned).into_iter().chain(dto.import_for(&entity, &ty)).collect();
                let refs: Vec<&str> = imports.iter().map(String::as_str).collect();
                CompletionItem {
                    label: format!("{name} : {ty}"),
                    detail: Some(format!("Add to {stem}")),
                    insert_text: dto.member(&name, &ty),
                    replace: Some(range),
                    filter_text: Some(name.clone()),
                    additional_edits: import_edit(&doc.text, &refs, kotlin).map(|edit| edit.to_text_edit(&doc.text)).into_iter().collect(),
                    ..CompletionItem::new(&name, CompletionKind::Field)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{diagnostics::DiagnosticsProvider, mock::MockHost, providers::Providers};

    const ENTITY: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.ManyToOne;\nimport jakarta.persistence.OneToMany;\nimport java.time.LocalDate;\nimport java.util.List;\n@Entity\npublic class Customer {\n    @Id Long id;\n    String title;\n    LocalDate joined;\n    @ManyToOne Region region;\n    @OneToMany(mappedBy = \"customer\") List<Order> orders;\n}\n";

    fn complete(host: &MockHost, path: &str, text: &str, marker: &str) -> Vec<CompletionItem> {
        host.add_file("src/main/java/shop/Customer.java", ENTITY);
        host.add_file("src/main/java/shop/RegionDto.java", "package shop;\npublic record RegionDto(Long id) {}\n");
        let mut plugin = JpaAssist::default();
        plugin.diagnostics(&Document { path: "/project/src/main/java/shop/Customer.java".into(), language: "java".into(), version: 1, text: ENTITY.into() });
        let at = text.find(marker).unwrap() + marker.len();
        let doc = Document { path: path.into(), language: "java".into(), version: 1, text: text.into() };
        plugin.completion(&doc, position(text, at), None)
    }

    #[test]
    fn typing_a_missing_field_in_a_record_dto_offers_the_entity_attribute() {
        let host = MockHost::install();
        let text = "package shop;\n\npublic record CustomerDto(Long id, ti) {\n}\n";
        let items = complete(&host, "/project/src/main/java/shop/CustomerDto.java", text, "Long id, ti");
        let item = items.iter().find(|item| item.filter_text.as_deref() == Some("title")).expect("title offered");
        assert_eq!(item.label, "title : String");
        assert_eq!(item.insert_text, "String title");
        assert!(items.iter().all(|item| item.filter_text.as_deref() != Some("id")));
    }

    #[test]
    fn dates_bring_their_import_and_associations_use_an_existing_dto_type() {
        let host = MockHost::install();
        let text = "package shop;\n\npublic record CustomerDto(Long id, ) {\n}\n";
        let items = complete(&host, "/project/src/main/java/shop/CustomerDto.java", text, "Long id, ");
        let joined = items.iter().find(|item| item.filter_text.as_deref() == Some("joined")).expect("joined offered");
        assert!(joined.additional_edits.iter().any(|edit| edit.text.contains("import java.time.LocalDate;")), "{:?}", joined.additional_edits);
        let region = items.iter().find(|item| item.filter_text.as_deref() == Some("region")).expect("association offered");
        assert_eq!(region.insert_text, "RegionDto region");
        assert!(items.iter().all(|item| item.filter_text.as_deref() != Some("orders")), "OrderDto does not exist");
    }

    #[test]
    fn unlinked_types_and_non_member_positions_stay_quiet() {
        let host = MockHost::install();
        let text = "package shop;\n\npublic record OtherThing(Long id, ti) {\n}\n";
        assert!(complete(&host, "/project/src/main/java/shop/OtherThing.java", text, "Long id, ti").is_empty());
        let host = MockHost::install();
        let text = "package shop;\n\npublic record CustomerDto(Long id) {\n    static int x = ti;\n}\n";
        assert!(complete(&host, "/project/src/main/java/shop/CustomerDto.java", text, "x = ti").is_empty());
    }
}
