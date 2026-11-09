//! Bounded local route-name facts. Unsupported sources retain the saved checker.
use crate::{
    checks::{self, Finding},
    index, php_checks, php_text, source_xml,
};
use std::{collections::BTreeSet, ops::Range};
const ROUTE: [&str; 2] = [
    r"Symfony\Component\Routing\Attribute\Route",
    r"Symfony\Component\Routing\Annotation\Route",
];

fn repeated(entries: Vec<(String, Range<usize>)>) -> Vec<Finding> {
    let mut seen = BTreeSet::new();
    entries
        .into_iter()
        .filter_map(|(name, range)| {
            (!seen.insert(name.clone())).then(|| {
                Finding::warn(
                    range,
                    "duplicate-route",
                    format!("Route \"{name}\" is already defined in this file."),
                )
            })
        })
        .collect()
}
fn indentation(text: &str, at: usize) -> usize {
    let start = text[..at].rfind('\n').map_or(0, |at| at + 1);
    text[start..]
        .bytes()
        .take_while(|b| matches!(b, b' ' | b'\t'))
        .count()
}
fn yaml(text: &str) -> Option<Vec<(String, Range<usize>)>> {
    let mut content = false;
    let mut marker = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "---" || line.starts_with("--- #") {
            if content || marker {
                return None;
            }
            marker = true;
            continue;
        }
        if line == "..." || line.starts_with("... #") || line.starts_with('%') {
            return None;
        }
        content = true;
    }
    let keys = index::yaml_keys(text);
    let Some(root) = keys.iter().map(|key| indentation(text, key.offset)).min() else {
        return Some(Vec::new());
    };
    let mut entries = Vec::new();
    for key in keys
        .into_iter()
        .filter(|key| indentation(text, key.offset) == root)
    {
        if key.key.contains('\\') || key.key.chars().any(char::is_control) {
            return None;
        }
        let line = text[..key.offset].rfind('\n').map_or(0, |at| at + 1);
        let raw = text[line..].trim_start();
        let quoted = raw.starts_with(['\'', '"']);
        if !quoted && raw.starts_with(['&', '*', '!', '?', '@', '%']) {
            return None;
        }
        let range = key.offset..key.offset + key.key.len();
        entries.push((key.key, range));
    }
    Some(entries)
}
fn php(text: &str) -> Option<Vec<(String, Range<usize>)>> {
    // Legacy doc annotations and multi-class/ambiguous declarations keep their saved fallback.
    if text.contains("@Route") {
        return None;
    }
    let masked = php_text::mask(text);
    let classes = masked
        .match_indices("class ")
        .filter(|(at, _)| {
            !masked[..*at]
                .bytes()
                .last()
                .is_some_and(|b| crate::refs::is_identifier(b) || b == b':')
        })
        .count();
    if classes != 1 {
        return None;
    }
    let namespaces: Vec<_> = masked.match_indices("namespace ").collect();
    if namespaces.len() > 1 {
        return None;
    }
    if let Some((at, _)) = namespaces.first() {
        let rest = &masked[*at..];
        let semi = rest.find(';')?;
        if rest.find('{').is_some_and(|brace| brace < semi) {
            return None;
        }
    }
    let model = php_text::class_model(text)?;
    let attributes = php_text::attributes(text);
    if attributes.iter().any(|attribute| {
        attribute.name.rsplit('\\').next() == Some("Route")
            && !ROUTE.contains(&attribute.name.as_str())
    }) {
        return None;
    }
    let mut entries = Vec::new();
    for attribute in attributes {
        if !ROUTE.contains(&attribute.name.as_str()) || !model.body.contains(&attribute.range.start)
        {
            continue;
        }
        let php_text::Target::Method {
            visibility: php_text::Visibility::Public,
            name_range,
            ..
        } = &attribute.target
        else {
            continue;
        };
        if !model
            .methods
            .iter()
            .any(|method| method.name_range == *name_range)
        {
            continue;
        }
        let Some(open) = attribute
            .arguments
            .start
            .checked_sub(1)
            .filter(|at| masked.as_bytes().get(*at) == Some(&b'('))
        else {
            continue;
        };
        let mut names = Vec::new();
        for argument in php_checks::arguments(&masked, open) {
            let Some((name, value)) = php_checks::named(&masked, argument) else {
                continue;
            };
            if &masked[name] == "name" {
                names.push(value);
            }
        }
        if names.len() > 1 {
            return None;
        }
        let Some(value) = names.pop() else {
            continue;
        };
        let Some(range) = checks::literal(text, &text[value]) else {
            continue;
        };
        let name = &text[range.clone()];
        if name.contains('\\') {
            return None;
        }
        if checks::is_dynamic(name) {
            continue;
        }
        entries.push((name.into(), range));
    }
    Some(entries)
}
pub(crate) fn check(path: &str, language: &str, text: &str) -> Option<Vec<Finding>> {
    if text.len() > 128 << 10 || path.contains("/var/cache/") {
        return None;
    }
    let entries = match language {
        "xml" => source_xml::route_ids(text)?,
        "yaml" if checks::is_route_file(path) => yaml(text)?,
        "php" => php(text)?,
        _ => return None,
    };
    Some(repeated(entries))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn yaml_root_mapping_does_not_flatten_environment_overrides_or_documents() {
        let text = "same:\n  path: /a\nwhen@test:\n  same:\n    path: /test\nsame:\n  path: /b\n";
        let out = check("/p/config/routes.yaml", "yaml", text).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(&text[out[0].start..out[0].end], "same");
        assert!(
            check(
                "/p/config/routes.yaml",
                "yaml",
                "same:\n  path: /a\n---\nsame:\n  path: /b\n"
            )
            .is_none()
        );
        let imports = "one:\n  resource: first.yaml\none:\n  resource: second.yaml\n";
        assert_eq!(
            check("/p/config/routes.yaml", "yaml", imports)
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn php_requires_resolved_own_public_method_attributes_and_exact_name_ranges() {
        let text = r#"<?php
use Symfony\Component\Routing\Attribute\Route as R;
#[R(name: 'prefix_')]
class C {
    // #[R(name: 'same')]
    #[R('/a', name: 'same')]
    public function a() {}
    #[R('/b', name: 'same')]
    public function b() {}
    #[R('/private', name: 'same')]
    private function hidden() {}
}
"#;
        let out = check("/p/src/C.php", "php", text).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(&text[out[0].start..out[0].end], "same");
        assert_eq!(
            out[0].start,
            text.find("#[R('/b', name: 'same')]").unwrap() + 17
        );
        assert!(
            check(
                "/p/src/C.php",
                "php",
                &format!("{text}\nclass Other {{}}\n")
            )
            .is_none()
        );
        assert!(
            check(
                "/p/src/C.php",
                "php",
                "<?php\nclass C {\n #[Route(name: 'same')]\n public function a() {}\n}\n"
            )
            .is_none()
        );
        assert!(
            check(
                "/p/src/C.php",
                "php",
                &format!("{text}\n/** @Route(name=\"same\") */")
            )
            .is_none()
        );
    }
}
