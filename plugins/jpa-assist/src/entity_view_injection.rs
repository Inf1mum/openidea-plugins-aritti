//! JPQL language injection into `@Mapping("...")` string literals of a Blaze-Persistence `@EntityView(Entity.class)`
//! type. The host injects the literal's content as a bare expression (the API has no prefix or suffix text), so only
//! the plugin's lexical JPQL highlighting applies; attribute resolution against the entity is not provided.
//! Requires explicit canonical imports of both annotations, exactly one top-level type in the file, a plain single-line
//! literal, and `com.blazebit:blaze-persistence-entity-view-api[-jakarta]` written literally in the build files.
use super::*;
use crate::model::{annotations, plain_literal, value_range};
use openidea_plugin::{injections::Injection, text::position};

const ENTITY_VIEW: &str = "com.blazebit.persistence.view.EntityView";
const MAPPING: &str = "com.blazebit.persistence.view.Mapping";

fn imported(masked: &str, fqcn: &str) -> bool {
    masked.lines().filter_map(|line| line.trim().strip_prefix("import ")).any(|import| import.trim_end_matches(';').trim() == fqcn)
}

pub(crate) fn injections(doc: &openidea_plugin::injections::Document) -> Vec<Injection> {
    if doc.language != "java" || doc.text.len() > MAX_TEXT || !doc.text.contains("EntityView") || !doc.text.contains("Mapping") {
        return Vec::new();
    }
    let masked = mask(&doc.text);
    if !imported(&masked, ENTITY_VIEW) || !imported(&masked, MAPPING) {
        return Vec::new();
    }
    let types: usize = ["class", "interface", "enum", "record"].iter().map(|keyword| words(&masked, keyword).filter(|at| *at == 0 || masked.as_bytes()[at - 1] != b'.').count()).sum();
    let all = annotations(&masked);
    if types != 1 || all.len() > 512 {
        return Vec::new();
    }
    let views: Vec<_> = all.iter().filter(|annotation| annotation.name == "EntityView").collect();
    let [view] = views.as_slice() else {
        return Vec::new();
    };
    let Some((from, to)) = value_range(&doc.text, &masked, view, "value") else {
        return Vec::new();
    };
    if !doc.text[from..to].ends_with(".class") {
        return Vec::new();
    }
    let mut out = Vec::new();
    for mapping in all.iter().filter(|annotation| annotation.name == "Mapping" && annotation.start > view.end()) {
        let Some(range) = value_range(&doc.text, &masked, mapping, "value") else {
            continue;
        };
        let Some((at, body)) = plain_literal(&doc.text, range, false) else {
            continue;
        };
        if body.is_empty() || body.contains(['\\', '\r', '\n']) || body.contains("#{") || body.contains("${") {
            continue;
        }
        out.push(Injection { range: Range::new(position(&doc.text, at), position(&doc.text, at + body.len())), language: "jpql".into() });
        if out.len() >= 128 {
            break;
        }
    }
    if out.is_empty() || !blaze_provable(doc) {
        return Vec::new();
    }
    out
}

fn blaze_provable(doc: &openidea_plugin::injections::Document) -> bool {
    let Some(project) = host::project_of(&doc.path) else {
        return false;
    };
    let Ok(root) = project.root() else {
        return false;
    };
    let source = Document { path: doc.path.clone(), language: doc.language.clone(), text: String::new(), version: doc.version };
    modelmapper_action::build_proof(&project, &root, &source, "com.blazebit", &modelmapper_action::BLAZE_ARTIFACTS, "Blaze-Persistence entity views").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, text::offset};

    const POM: &str = "<project><dependencies><dependency><groupId>com.blazebit</groupId><artifactId>blaze-persistence-entity-view-api-jakarta</artifactId></dependency></dependencies></project>";
    const VIEW: &str = "package shop.view;\n\nimport com.blazebit.persistence.view.EntityView;\nimport com.blazebit.persistence.view.Mapping;\nimport shop.domain.Customer;\n\n@EntityView(Customer.class)\npublic interface CustomerView {\n    @Mapping(\"title\")\n    String getName();\n\n    @Mapping(value = \"address.city\")\n    String getCity();\n}\n";
    const PATH: &str = "/project/src/main/java/shop/view/CustomerView.java";

    fn run(text: &str, pom: bool) -> Vec<String> {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("src/main/java/shop/view/CustomerView.java", text);
        if pom {
            host.add_file("pom.xml", POM);
        }
        let doc = openidea_plugin::injections::Document { path: PATH.into(), language: "java".into(), version: 1, text: text.into() };
        injections(&doc)
            .iter()
            .map(|injection| {
                assert_eq!(injection.language, "jpql");
                text[offset(text, injection.range.start).unwrap()..offset(text, injection.range.end).unwrap()].to_owned()
            })
            .collect()
    }

    #[test]
    fn mapping_literals_of_an_entity_view_are_injected_when_blaze_is_in_the_build() {
        assert_eq!(run(VIEW, true), ["title", "address.city"]);
    }

    #[test]
    fn unprovable_dependency_lookalike_imports_and_dynamic_literals_stay_silent() {
        assert!(run(VIEW, false).is_empty());
        assert!(run(&VIEW.replace("import com.blazebit.persistence.view.Mapping;", "import foreign.Mapping;"), true).is_empty());
        assert!(run(&VIEW.replace("\"title\"", "\"ti\" + \"tle\""), true) == ["address.city"]);
        assert!(run(&VIEW.replace("public interface CustomerView {", "public interface CustomerView {\n    interface Nested {}"), true).is_empty());
        assert!(run(&VIEW.replace("@EntityView(Customer.class)", "@EntityView(name)"), true).is_empty());
    }
}
