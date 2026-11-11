//! Stable-key UI text. Project identifiers, source, values and logs are never translated.
use openidea_plugin::host;
fn normalize(value: &str) -> &'static str {
    match value.replace('_', "-").to_ascii_lowercase().as_str() {
        value if value == "ja" || value.starts_with("ja-") => "ja",
        value if value == "ko" || value.starts_with("ko-") => "ko",
        "zh-cn" | "zh-sg" | "zh-hans" => "zh-CN",
        _ => "en",
    }
}
/// Preserve the configured mode for the browser; native leaves use its correlated locale hint.
pub(crate) fn locale() -> String {
    let language = host::setting_string("uiLanguage").unwrap_or_else(|| "auto".into());
    if language == "auto" || language.is_empty() {
        "auto".into()
    } else {
        normalize(&language).into()
    }
}
fn resolved_locale() -> String {
    let configured = locale();
    if configured == "auto" {
        ui_kit::web::resolved_language("Laravel").unwrap_or_else(|| "en".into())
    } else {
        configured
    }
}
/// Type-preserving static leaf lookup, suitable for authored &'static str UI sites.
pub(crate) fn text(key: &str, fallback: &'static str) -> &'static str {
    let language = resolved_locale();
    generated::lookup(normalize(&language), key)
        .or_else(|| generated::lookup("en", key))
        .unwrap_or(fallback)
}
#[path = "ui_text_catalog.rs"]
#[rustfmt::skip]
mod generated;
pub(crate) fn t(key: &str, fallback: &str) -> String {
    translate(&resolved_locale(), key, fallback, &[])
}
pub(crate) fn format(key: &str, fallback: &str, args: &[(&str, &str)]) -> String {
    translate(&resolved_locale(), key, fallback, args)
}
/// Match only a caller-owned error template at its UI delivery boundary.
/// Ambiguous data separators are refused rather than assigning project data to
/// the wrong placeholder. Parsers keep returning their original English facts.
pub(crate) fn owned_error(message: &str, templates: &[(&str, &str)]) -> Option<String> {
    for (key, template) in templates {
        if let Some(args) = owned_arguments(template, message) {
            return Some(format(key, template, &args));
        }
    }
    None
}
fn owned_arguments<'a>(template: &'a str, message: &'a str) -> Option<Vec<(&'a str, &'a str)>> {
    if message.len() > 256 * 1024 || template.len() > 8192 {
        return None;
    }
    let mut pieces = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let close = rest[open + 1..].find('}')? + open + 1;
        let name = &rest[open + 1..close];
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return None;
        }
        pieces.push((&rest[..open], name));
        rest = &rest[close + 1..];
        if pieces.len() > 16 {
            return None;
        }
    }
    if pieces.is_empty() {
        return None;
    }
    fn walk<'a>(
        pieces: &[(&'a str, &'a str)],
        tail: &'a str,
        message: &'a str,
        args: &mut Vec<(&'a str, &'a str)>,
        found: &mut Vec<Vec<(&'a str, &'a str)>>,
        budget: &mut usize,
    ) {
        if *budget == 0 || found.len() > 1 {
            return;
        }
        *budget -= 1;
        let Some((literal, name)) = pieces.first() else {
            if message == tail {
                found.push(args.clone());
            }
            return;
        };
        let Some(message) = message.strip_prefix(literal) else {
            return;
        };
        let separator = pieces.get(1).map_or(tail, |piece| piece.0);
        if separator.is_empty() {
            if pieces.len() != 1 {
                return;
            }
            if args
                .iter()
                .any(|(known, value)| known == name && *value != message)
            {
                return;
            }
            args.push((name, message));
            walk(&[], tail, "", args, found, budget);
            args.pop();
            return;
        }
        for (at, _) in message.match_indices(separator) {
            if *budget == 0 || found.len() > 1 {
                return;
            }
            let value = &message[..at];
            if args
                .iter()
                .any(|(known, previous)| known == name && *previous != value)
            {
                continue;
            }
            args.push((name, value));
            walk(&pieces[1..], tail, &message[at..], args, found, budget);
            args.pop();
        }
    }
    let mut found = Vec::new();
    let mut budget = 4096;
    walk(
        &pieces,
        rest,
        message,
        &mut Vec::new(),
        &mut found,
        &mut budget,
    );
    (budget > 0 && found.len() == 1).then(|| found.pop().unwrap())
}
fn translate(language: &str, key: &str, fallback: &str, args: &[(&str, &str)]) -> String {
    let template = generated::lookup(normalize(language), key)
        .or_else(|| generated::lookup("en", key))
        .unwrap_or(fallback);
    // Expand in a single pass; values containing {other} are application data.
    let mut result = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        result.push_str(&rest[..open]);
        let Some(close) = rest[open + 1..].find('}') else {
            result.push_str(&rest[open..]);
            return result;
        };
        let end = open + 1 + close;
        let name = &rest[open + 1..end];
        if let Some((_, value)) = args.iter().find(|(key, _)| *key == name) {
            result.push_str(value);
        } else {
            result.push_str(&rest[open..=end]);
        }
        rest = &rest[end + 1..];
    }
    result.push_str(rest);
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owned_template_matching_is_pure_exact_and_rejects_ambiguous_project_data() {
        assert_eq!(
            owned_arguments(
                "Column {column}: {detail}",
                "Column 顧客: raw {column} data"
            ),
            Some(vec![("column", "顧客"), ("detail", "raw {column} data")])
        );
        assert!(owned_arguments("{first}: {last}", "one: two: three").is_none());
        assert_eq!(
            owned_arguments("{value}/{value}", "x/x"),
            Some(vec![("value", "x"), ("value", "x")])
        );
        assert!(owned_arguments("{value}/{value}", "x/y").is_none());
        assert!(owned_arguments("Column {value}.", "Other data.").is_none());
        assert!(owned_arguments("{one}{two}", "undelimited").is_none());
    }
    #[test]
    fn catalogs_cover_each_explicit_language_and_fallback_preserves_application_values() {
        for language in ["ja", "ko", "zh-CN"] {
            assert_ne!(
                translate(language, "shared.search", "Search", &[]),
                "Search"
            );
        }
        assert_eq!(
            translate("ja", "unknown", "SourceValue", &[]),
            "SourceValue"
        );
        assert_eq!(
            translate("en", "command.count", "", &[("count", "{status}")]),
            "{status} commands"
        );
        assert_eq!(normalize("ja-JP"), "ja");
        assert_eq!(normalize("zh-TW"), "en");
    }
    #[test]
    fn native_static_leaf_text_respects_explicit_setting_without_translating_project_data() {
        let host = openidea_plugin::mock::MockHost::install();
        host.set_setting("uiLanguage", "\"ja\"");
        assert_eq!(locale(), "ja");
        assert_eq!(t("shared.search", "Search"), "検索");
        assert_eq!(
            format("command.count", "", &[("count", "AuthController")]),
            "AuthController 件のコマンド"
        );
    }
    #[test]
    fn auto_uses_only_the_browser_locale_hint_and_explicit_settings_take_precedence() {
        let host = openidea_plugin::mock::MockHost::install();
        openidea_plugin::host::storage::set("ui-kit.language:Laravel", "ja").unwrap();
        assert_eq!(locale(), "auto");
        assert_eq!(text("shared.search", "Search"), "検索");
        host.set_setting("uiLanguage", "\"ko\"");
        assert_eq!(text("shared.search", "Search"), "검색");
    }
}
