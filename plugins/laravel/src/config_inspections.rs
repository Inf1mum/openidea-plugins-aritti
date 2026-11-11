//! Unknown config paths only below an authoritative complete indexed map.
use crate::{Index, refs};
use openidea_plugin::providers::Document;
pub(crate) fn missing(index: &Index, name: &str) -> bool {
    if index.partial || index.config.contains_key(name) {
        return false;
    }
    let mut prefix = name;
    while let Some((parent, _)) = prefix.rsplit_once('.') {
        if let Some(entry) = index.config.get(parent) {
            let wholly_incomplete = index.config_incomplete.contains(&entry.path) && !index.config_gap_is_local(&entry.path, parent);
            return !wholly_incomplete && matches!(entry.detail.as_deref(), Some("array" | "config file"));
        }
        prefix = parent;
    }
    false
}
pub(crate) struct Binding {
    doc: Document,
}
pub(crate) fn prepare(
    index: &Index,
    language: &str,
    source: &str,
    count: usize,
) -> Option<Binding> {
    if !crate::view_rename::binding_budget(source.len(), count) {
        return None;
    }
    let mut visible = source.to_owned();
    if language == "blade" {
        let mut code = vec![b' '; source.len()];
        for region in refs::blade_regions(source) {
            code[region.clone()].copy_from_slice(&source.as_bytes()[region]);
        }
        // Opaque PHP may import/declare a helper that shadows an echo call;
        // keep it for binding proof, while verbatim/template text stays blank.
        for region in crate::blade_php::ranges(source)? {
            let value = &source[region.clone()];
            if value.starts_with("@php") || value.starts_with("<?php") || value.starts_with("<?=") {
                code[region.clone()].copy_from_slice(&source.as_bytes()[region]);
            }
        }

        visible = String::from_utf8(code).ok()?;
    }
    let path = format!(
        "{}/__laravel_reference_inspection__{}",
        index.metadata.project_root,
        if language == "blade" {
            ".blade.php"
        } else {
            ".php"
        }
    );
    Some(Binding {
        doc: Document {
            path,
            language: language.into(),
            text: visible,
            version: 0,
        },
    })
}
impl Binding {
    pub(crate) fn safe(&self, index: &Index, reference: &refs::Reference) -> bool {
        crate::view_rename::binding_safe(index, &self.doc, reference)
    }
}
#[cfg(test)]
fn binding_safe(index: &Index, language: &str, source: &str, reference: &refs::Reference) -> bool {
    prepare(
        index,
        language,
        source,
        refs::references(language, source).len(),
    )
    .is_some_and(|context| context.safe(index, reference))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Entry;
    use openidea_plugin::Position;
    fn index() -> Index {
        let mut index = Index::default();
        index.config.insert(
            "app".into(),
            Entry {
                path: "/project/config/app.php".into(),
                at: Position::new(0, 0),
                width: 0,
                detail: Some("config file".into()),
            },
        );
        index.config.insert(
            "app.dynamic".into(),
            Entry {
                path: "/project/config/app.php".into(),
                at: Position::new(0, 0),
                width: 0,
                detail: Some("runtime()".into()),
            },
        );
        index
    }
    #[test]
    fn missing_nested_descendants_report_first_absent_branch_but_dynamic_values_and_partial_files_do_not()
     {
        let mut index = index();
        assert!(missing(&index, "app.absent.deep"));
        assert!(!missing(&index, "app.dynamic.deep"));
        assert!(!missing(&index, "foreign.absent"));
        index
            .config_incomplete
            .insert("/project/config/app.php".into());
        assert!(!missing(&index, "app.absent.deep"));
        index.config_incomplete.clear();
        index.partial = true;
        assert!(!missing(&index, "app.absent.deep"));
    }
    #[test]
    fn actual_framework_bindings_include_blade_html_attributes_but_foreign_functions_and_facades_do_not()
     {
        let index = index();
        for (language, source, expected) in [
            ("php", "<?php config('app.absent');", true),
            (
                "php",
                "<?php use function Vendor\\config; config('app.absent');",
                false,
            ),
            (
                "php",
                "<?php function config($key){} config('app.absent');",
                false,
            ),
            (
                "php",
                "<?php use Vendor\\Config; Config::get('app.absent');",
                false,
            ),
            (
                "blade",
                "<p title=\"{{ config('app.absent') }}\">x</p>",
                true,
            ),
        ] {
            let reference = refs::references(language, source)
                .into_iter()
                .find(|r| r.kind == refs::Kind::Config)
                .unwrap();
            assert_eq!(
                binding_safe(&index, language, source, &reference),
                expected,
                "{source}"
            );
        }
    }
}
