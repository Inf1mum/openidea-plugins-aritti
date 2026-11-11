//! Ordered ide.json completion transformer grammar from Laravel Idea 13.4.1's
//! CompletionTransformerFactory and string delimiter helper. No host calls.
use serde_json::Value;
#[derive(Clone, Debug, Default)]
pub(super) struct Pipeline(Vec<Step>);
#[derive(Clone, Debug)]
enum Step {
    Prefix(Vec<String>),
    Suffix(Vec<String>),
    Contains(Vec<String>),
    Excludes(Vec<String>),
    RemovePrefix(String),
    RemoveSuffix(String),
}
fn alternatives(value: &str) -> Vec<String> {
    if value.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut current = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'|') {
            chars.next();
            current.push('|');
        } else if c == '|' {
            out.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    out.push(current);
    out
}
impl Pipeline {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn parse(value: Option<&Value>) -> Result<Self, &'static str> {
        let Some(value) = value else {
            return Ok(Self::default());
        };
        let values = value
            .as_array()
            .ok_or("completion transformers must be an array of strings")?;
        if values.len() > 64 {
            return Err("completion transformers exceed the 64-step/16-KiB budget");
        }
        let mut bytes = 0usize;
        let mut steps = Vec::new();
        for value in values {
            let expression = value
                .as_str()
                .ok_or("completion transformers must be an array of strings")?;
            bytes = bytes.saturating_add(expression.len());
            if bytes > 16384 {
                return Err("completion transformers exceed the 64-step/16-KiB budget");
            }
            for key in [
                "filter:hasPrefix",
                "filter:hasPostfix",
                "filter:hasSuffix",
                "filter:contains",
                "filter:doesntContain",
                "transform:removePrefix",
                "transform:removePostfix",
                "transform:removeSuffix",
            ] {
                // The pinned factory matches the name prefix first, then removes "name:".
                // Without that colon the whole expression is the argument; preserve this quirk.
                if !expression.starts_with(key) {
                    continue;
                }
                let argument = expression
                    .strip_prefix(&format!("{key}:"))
                    .unwrap_or(expression);
                steps.push(match key {
                    "filter:hasPrefix" => Step::Prefix(alternatives(argument)),
                    "filter:hasPostfix" | "filter:hasSuffix" => {
                        Step::Suffix(alternatives(argument))
                    }
                    "filter:contains" => Step::Contains(alternatives(argument)),
                    "filter:doesntContain" => Step::Excludes(alternatives(argument)),
                    "transform:removePrefix" => Step::RemovePrefix(argument.into()),
                    _ => Step::RemoveSuffix(argument.into()),
                });
                break;
            }
        }
        Ok(Self(steps))
    }
    pub fn forward(&self, name: &str) -> Option<String> {
        let mut name = name.to_owned();
        for step in &self.0 {
            match step {
                Step::Prefix(values) => {
                    if !values.iter().any(|value| name.starts_with(value)) {
                        return None;
                    }
                }
                Step::Suffix(values) => {
                    if !values.iter().any(|value| name.ends_with(value)) {
                        return None;
                    }
                }
                Step::Contains(values) => {
                    if !values.iter().any(|value| name.contains(value)) {
                        return None;
                    }
                }
                Step::Excludes(values) => {
                    if values.iter().any(|value| name.contains(value)) {
                        return None;
                    }
                }
                Step::RemovePrefix(value) => name = name.strip_prefix(value)?.to_owned(),
                Step::RemoveSuffix(value) => name = name.strip_suffix(value)?.to_owned(),
            }
        }
        Some(name)
    }
    pub fn back(&self, name: &str) -> String {
        let mut name = name.to_owned();
        for step in self.0.iter().rev() {
            match step {
                Step::RemovePrefix(value) => name = format!("{value}{name}"),
                Step::RemoveSuffix(value) => name.push_str(value),
                _ => {}
            }
        }
        name
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn ordered_filters_and_affixes_preserve_the_inverse() {
        let p = Pipeline::parse(Some(&json!([
            "filter:hasPrefix:app.",
            "transform:removePrefix:app.",
            "filter:contains:na|en",
            "transform:removeSuffix:me"
        ])))
        .unwrap();
        assert_eq!(p.forward("app.name").as_deref(), Some("na"));
        assert_eq!(p.forward("app.env"), None);
        assert_eq!(p.forward("other.name"), None);
        assert_eq!(p.back("na"), "app.name");
    }
    #[test]
    fn delimiter_escaping_and_empty_alternatives_match_the_pinned_helper() {
        assert_eq!(alternatives(r"a\|b|c\d|"), vec!["a|b", "c\\d", ""]);
        assert_eq!(alternatives(""), Vec::<String>::new());
        assert_eq!(alternatives("|"), vec!["", ""]);
        for expression in [
            "filter:hasPrefix:",
            "filter:hasPostfix:",
            "filter:hasSuffix:",
            "filter:contains:",
        ] {
            assert!(
                Pipeline::parse(Some(&json!([expression])))
                    .unwrap()
                    .forward("anything")
                    .is_none()
            );
        }
        assert_eq!(
            Pipeline::parse(Some(&json!(["filter:doesntContain:"])))
                .unwrap()
                .forward("anything")
                .as_deref(),
            Some("anything")
        );
    }
    #[test]
    fn unknown_only_identity_and_factory_prefix_quirk_are_intentional() {
        let p = Pipeline::parse(Some(&json!(["unknown:thing", "transform:addPrefix:x"]))).unwrap();
        assert_eq!(p.forward("name").as_deref(), Some("name"));
        assert_eq!(p.back("name"), "name");
        let p = Pipeline::parse(Some(&json!(["transform:removePrefixWITHOUTCOLON"]))).unwrap();
        assert_eq!(
            p.forward("transform:removePrefixWITHOUTCOLONname")
                .as_deref(),
            Some("name")
        );
        assert_eq!(p.back("name"), "transform:removePrefixWITHOUTCOLONname");
    }
    #[test]
    fn inverse_filters_are_identity_and_suffix_aliases_are_case_sensitive() {
        let p = Pipeline::parse(Some(&json!([
            "filter:hasSuffix:.blade.php",
            "transform:removePostfix:.blade.php",
            "filter:doesntContain:admin|private"
        ])))
        .unwrap();
        assert_eq!(
            p.forward("pages.card.blade.php").as_deref(),
            Some("pages.card")
        );
        assert!(p.forward("Admin.blade.php").is_some());
        assert!(p.forward("admin.blade.php").is_none());
        assert_eq!(p.back("admin"), "admin.blade.php");
    }
    #[test]
    fn transform_affixes_are_literals_instead_of_delimited_filter_arguments() {
        let p = Pipeline::parse(Some(&json!(["transform:removePrefix:a|b"]))).unwrap();
        assert_eq!(p.forward("a|bname").as_deref(), Some("name"));
        let p = Pipeline::parse(Some(&json!([r"transform:removePrefix:a\|b"]))).unwrap();
        assert!(p.forward("a|bname").is_none());
        assert_eq!(p.forward(r"a\|bname").as_deref(), Some("name"));
    }
    #[test]
    fn malformed_shapes_and_budget_overflow_are_withheld() {
        for value in [
            json!(null),
            json!(true),
            json!([1]),
            json!(vec![""; 65]),
            json!(["x".repeat(16385)]),
        ] {
            assert!(Pipeline::parse(Some(&value)).is_err());
        }
    }
    #[test]
    fn registered_providers_transform_config_views_translations_and_source_locations() {
        use crate::tests::{cursor, doc};
        use openidea_plugin::{mock::MockHost, providers::Providers, text};
        let h = MockHost::install();
        let rules = serde_json::json!({"completions":[
          {"complete":"configKey","condition":[{"functionNames":["chooseConfig"],"parameters":[1]}],"transformers":["filter:hasPrefix:app.","transform:removePrefix:app.","filter:doesntContain:private"]},
          {"complete":"configKey","options":{"prefix":"app"},"condition":[{"functionNames":["chooseScopedConfig"],"parameters":[1]}],"transformers":["filter:hasPrefix:never"]},
          {"complete":"configKey","options":{"prefix":"app","recursive":true},"condition":[{"functionNames":["chooseRecursiveConfig"],"parameters":[1]}],"transformers":["filter:hasPrefix:never"]},
          {"complete":"viewName","condition":[{"functionNames":["chooseView"],"parameters":[1]}],"transformers":["transform:removePrefix:widgets."]},
          {"complete":"translationKey","condition":[{"functionNames":["chooseTranslation"],"parameters":[1]}],"transformers":["filter:hasSuffix:.title","transform:removePrefix:panel."]},
          {"complete":"staticStrings","options":{"stringsWithLocation":{"menu.😀a|b.end":"/refs.php:2","other":"/refs.php:1"}},"condition":[{"functionNames":["chooseStatic"],"parameters":[1]}],"transformers":["filter:contains:😀a\\|b","transform:removePrefix:menu.","transform:removeSuffix:.end"]}
        ]});
        h.add_file("ide.json", &rules.to_string());
        h.add_file(
            "config/app.php",
            "<?php return ['name'=>'OpenIdea','env'=>'test','private'=>'secret','mail'=>['from'=>'a@b']];",
        );
        h.add_file(
            "resources/views/widgets/account.blade.php",
            "<div>Account</div>",
        );
        h.add_file(
            "resources/lang/en/panel.php",
            "<?php return ['title'=>'Panel'];",
        );
        h.add_file("refs.php", "<?php\n//😀 target");
        h.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        h.add_file("artisan", "<?php");
        let mut plugin = crate::Main::default();
        openidea_plugin::Plugin::activate(&mut plugin, &h.context("aritti.laravel")).unwrap();
        crate::tests::finish_indexing(&mut plugin);
        for (marked, label, path) in [
            (
                "<?php chooseConfig('na|');",
                "name",
                "/project/config/app.php",
            ),
            (
                "<?php chooseScopedConfig('na|');",
                "name",
                "/project/config/app.php",
            ),
            (
                "<?php chooseRecursiveConfig('mail.fr|');",
                "mail.from",
                "/project/config/app.php",
            ),
            (
                "<?php chooseView('acc|');",
                "account",
                "/project/resources/views/widgets/account.blade.php",
            ),
            (
                "<?php chooseTranslation('tit|');",
                "title",
                "/project/resources/lang/en/panel.php",
            ),
        ] {
            let (source, at) = cursor(marked);
            let items = plugin.completion(&doc("/project/test.php", "php", &source), at, None);
            let item = items
                .iter()
                .find(|i| i.label == label)
                .unwrap_or_else(|| panic!("{marked}: {items:?}"));
            assert_eq!(item.insert_text, label);
            let source = source.replacen(
                &source[source.find('\'').unwrap() + 1..source.rfind('\'').unwrap()],
                label,
                1,
            );
            let at = text::position(&source, source.rfind('\'').unwrap());
            let locations = plugin.definition(&doc("/project/test.php", "php", &source), at);
            assert_eq!(locations[0].path, path);
        }
        let (source, at) = cursor("<?php chooseScopedConfig('na|');");
        assert!(
            !plugin
                .completion(&doc("/project/test.php", "php", &source), at, None)
                .iter()
                .any(|item| item.label == "mail.from")
        );
        let (source, at) = cursor("<?php chooseConfig('priv|ate');");
        assert!(
            !plugin
                .completion(&doc("/project/test.php", "php", &source), at, None)
                .iter()
                .any(|i| i.label == "private")
        );
        let locations = plugin.definition(&doc("/project/test.php", "php", &source), at);
        assert!(
            !locations.is_empty(),
            "config keys {:?}; direct metadata {:?}",
            plugin.indexes["/project"].config.keys().collect::<Vec<_>>(),
            super::super::definition(
                &plugin.indexes["/project"],
                &source,
                text::offset(&source, at).unwrap()
            )
        );
        assert_eq!(
            locations[0].path, "/project/config/app.php",
            "filter inverse is identity, as in pinned transformBack"
        );
        let (source, at) = cursor("<?php /*😀*/ chooseStatic('😀|');");
        let items = plugin.completion(&doc("/project/test.php", "php", &source), at, None);
        let item = items.iter().find(|i| i.label == "😀a|b").unwrap();
        let replacement = item.replace.unwrap();
        assert_eq!(replacement.end.character - replacement.start.character, 2);
        assert_eq!(item.insert_text, "😀a|b");
        let source = r#"<?php /*😀*/ chooseStatic("😀a\x7cb");"#;
        let at = text::position(source, source.rfind('"').unwrap());
        let locations = plugin.definition(&doc("/project/test.php", "php", source), at);
        assert_eq!(locations[0].path, "/project/refs.php");
        assert_eq!(locations[0].range.start.line, 1);

        assert!(h.process_runs().is_empty());
    }

    #[test]
    fn provider_directory_override_passport_and_ignored_instance_pipelines_match_pinned_consumers()
    {
        use crate::tests::{activated, cursor, doc};
        use openidea_plugin::{mock::MockHost, providers::Providers};
        let h = MockHost::install();
        h.add_file("ide.json",&json!({"packages":{"passport":{"scopes":{"api.read":"Read"}}},"completions":[
            {"complete":"directoryFiles","options":{"directory":"/assets","recursive":false,"suffixToClear":".txt"},"condition":[{"functionNames":["customFiles"],"parameters":[1]}],"transformers":["filter:hasSuffix:.txt","transform:removePrefix:pre-"]},
            {"complete":"directoryFiles","options":{"directory":"/assets","recursive":false,"suffixToClear":".txt"},"condition":[{"functionNames":["legacyFiles"],"parameters":[1]}],"transformers":["unknown:identity"]},
            {"complete":"passportScopeId","condition":[{"functionNames":["chooseScope"],"parameters":[1]}],"transformers":["transform:removePrefix:api."]},
            {"complete":"routeName","condition":[{"functionNames":["chooseRoute"],"parameters":[1]}],"transformers":["filter:hasPrefix:never"]}
        ]}).to_string());
        h.add_file("assets/pre-a.txt", "text");
        h.add_file("assets/other.json", "{}");
        let mut p = activated(&h);
        for (marked, label, target) in [
            (
                "<?php customFiles('a.t|xt');",
                "a.txt",
                "/project/assets/pre-a.txt",
            ),
            (
                "<?php legacyFiles('pre-|a');",
                "pre-a",
                "/project/assets/pre-a.txt",
            ),
            ("<?php chooseScope('re|ad');", "read", "/project/ide.json"),
            (
                "<?php chooseRoute('ho|me');",
                "home",
                "/project/routes/web.php",
            ),
        ] {
            let (source, at) = cursor(marked);
            let items = p.completion(&doc("/project/test.php", "php", &source), at, None);
            assert!(
                items.iter().any(|i| i.label == label),
                "{marked}: {items:?}"
            );
            let locations = p.definition(&doc("/project/test.php", "php", &source), at);
            assert!(
                locations.iter().any(|l| l.path == target),
                "{marked}: {locations:?}"
            );
        }
    }
    #[test]
    fn provider_unknown_pipeline_identity_and_invalid_shapes_do_not_broaden() {
        use crate::tests::{activated, cursor, doc};
        use openidea_plugin::{mock::MockHost, providers::Providers};
        let h = MockHost::install();
        h.add_file("ide.json",r#"{"completions":[{"complete":"staticStrings","options":{"strings":["visible"]},"condition":[{"functionNames":["identity"],"parameters":[1]}],"transformers":["unknown:thing"]},{"complete":"staticStrings","options":{"strings":["hidden"]},"condition":[{"functionNames":["invalid"],"parameters":[1]}],"transformers":[false]}]}"#);
        let mut p = activated(&h);
        let (source, at) = cursor("<?php identity('vis|');");
        assert!(
            p.completion(&doc("/project/test.php", "php", &source), at, None)
                .iter()
                .any(|i| i.label == "visible")
        );
        let (source, at) = cursor("<?php invalid('hid|');");
        assert!(
            p.completion(&doc("/project/test.php", "php", &source), at, None)
                .is_empty()
        );
        assert!(
            p.indexes["/project"]
                .metadata
                .issues
                .iter()
                .any(|i| i.contains("transformers"))
        );
    }
}
