//! Advice on association mappings for the creation previews.
//!
//! The same detectors that run as editor inspections (unidirectional `@OneToMany`, explicit or default EAGER fetching,
//! `@ManyToMany` over a `List`) are run on the source before and after a creation flow; a finding that exists only after is
//! explained in the confirm preview, so the reason is visible before anything is written. Detectors that stay quiet on a source
//! (unknown layouts, disabled settings) add nothing.
use crate::checks::inspect_entity_owned;
use crate::inspections_more;
use crate::model::parse_class;
use openidea_plugin::providers::Document;
use std::collections::HashMap;

const CODES: [&str; 4] = ["jpa-unidirectional-one-to-many", "jpa-eager-fetch", "jpa-eager-to-one", "jpa-many-to-many-list"];

pub(super) fn findings(path: &str, language: &str, text: &str) -> Vec<crate::checks::Finding> {
    let doc = Document { path: path.into(), language: language.into(), version: 0, text: text.into() };
    let mut found = inspections_more::findings(&doc);
    if crate::setting("checkAssociationFetch") {
        if let Some(class) = parse_class(text, path) {
            found.extend(inspect_entity_owned(text, language == "kotlin", &class));
        }
    }
    found.into_iter().filter(|finding| CODES.contains(&finding.code)).collect()
}

fn explanations(path: &str, language: &str, text: &str) -> Vec<(&'static str, String)> {
    findings(path, language, text).into_iter().map(|finding| (finding.code, finding.message)).collect()
}

/// Preview text for the associations `after` introduces over `before`; empty when there is nothing to explain.
pub(super) fn note(path: &str, language: &str, before: &str, after: &str) -> String {
    let mut known: HashMap<&str, usize> = HashMap::new();
    for (code, _) in explanations(path, language, before) {
        *known.entry(code).or_default() += 1;
    }

    let mut lines = Vec::new();
    for (code, message) in explanations(path, language, after) {
        match known.get_mut(code) {
            Some(count) if *count > 0 => *count -= 1,
            _ => lines.push(format!("- {message}")),
        }
    }
    if lines.is_empty() {
        return String::new();
    }
    format!("\nAssociation advice (the editor inspections will report this too):\n{}\n", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use openidea_plugin::mock::MockHost;

    const PATH: &str = "/project/src/Owner.java";
    const BEFORE: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.OneToMany;\nimport jakarta.persistence.ManyToOne;\nimport jakarta.persistence.ManyToMany;\nimport jakarta.persistence.FetchType;\nimport java.util.List;\n@Entity\npublic class Owner {\n    @Id Long id;\n}\n";

    fn note(path: &str, language: &str, before: &str, after: &str) -> String {
        let _host = MockHost::install().with_capabilities(&["project.read"]);
        super::note(path, language, before, after)
    }

    fn with(member: &str) -> String {
        BEFORE.replace("    @Id Long id;\n", &format!("    @Id Long id;\n    {member}\n"))
    }

    #[test]
    fn a_unidirectional_one_to_many_is_explained() {
        let text = note(PATH, "java", BEFORE, &with("@OneToMany List<Item> items;"));
        assert!(text.contains("link table") && text.contains("Association advice"), "{text}");
    }

    #[test]
    fn a_bidirectional_one_to_many_adds_nothing() {
        assert_eq!(note(PATH, "java", BEFORE, &with("@OneToMany(mappedBy = \"owner\") List<Item> items;")), "");
    }

    #[test]
    fn explicit_and_default_eager_fetching_are_explained() {
        let explicit = note(PATH, "java", BEFORE, &with("@ManyToOne(fetch = FetchType.EAGER) Customer customer;"));
        assert!(explicit.contains("explicitly requests eager fetching"), "{explicit}");
        let default = note(PATH, "java", BEFORE, &with("@ManyToOne Customer customer;"));
        assert!(default.contains("defaults to eager for to-one"), "{default}");
        assert_eq!(note(PATH, "java", BEFORE, &with("@ManyToOne(fetch = FetchType.LAZY) Customer customer;")), "");
    }

    #[test]
    fn a_many_to_many_list_is_explained_and_a_set_is_not() {
        assert!(note(PATH, "java", BEFORE, &with("@ManyToMany List<Tag> tags;")).contains("Sets are the best choice"));
        assert_eq!(note(PATH, "java", BEFORE, &with("@ManyToMany java.util.Set<Tag> tags;")), "");
    }

    #[test]
    fn what_the_source_already_had_is_not_repeated() {
        let before = with("@ManyToMany List<Tag> tags;");
        assert_eq!(note(PATH, "java", &before, &before.replace("Long id;", "Long id;\n    String name;")), "");
    }
}
