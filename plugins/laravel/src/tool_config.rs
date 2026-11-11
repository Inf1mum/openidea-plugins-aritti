//! Completion in the configuration files of Pint and PHPStan.

const PINT_KEYS: &[&str] = &["preset", "rules", "exclude", "notName", "notPath", "path"];
const PINT_PRESETS: &[&str] = &["laravel", "per", "psr12", "symfony", "empty"];
const PINT_RULES: &[&str] = &[
    "array_syntax", "binary_operator_spaces", "blank_line_before_statement", "braces_position", "class_attributes_separation",
    "concat_space", "declare_strict_types", "final_class", "fully_qualified_strict_types", "global_namespace_import",
    "line_ending", "method_argument_space", "new_with_parentheses", "no_unused_imports", "not_operator_with_successor_space",
    "ordered_imports", "phpdoc_align", "phpdoc_order", "phpdoc_separation", "phpdoc_summary", "single_quote",
    "strict_comparison", "strict_param", "trailing_comma_in_multiline", "trim_array_spaces", "visibility_required", "yoda_style",
];

const NEON_KEYS: &[&str] = &[
    "level", "paths", "excludePaths", "ignoreErrors", "bootstrapFiles", "tmpDir", "treatPhpDocTypesAsCertain",
    "reportUnmatchedIgnoredErrors", "checkMissingIterableValueType", "checkGenericClassInNonGenericObjectType",
    "universalObjectCratesClasses", "checkOctaneCompatibility", "checkModelProperties", "noEnvCallsOutsideOfConfig",
];
const NEON_BOOLEAN_KEYS: &[&str] = &[
    "treatPhpDocTypesAsCertain", "reportUnmatchedIgnoredErrors", "checkMissingIterableValueType",
    "checkGenericClassInNonGenericObjectType", "checkOctaneCompatibility", "checkModelProperties", "noEnvCallsOutsideOfConfig",
];
const NEON_LEVELS: &[&str] = &["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "max"];
const NEON_INCLUDES: &[&str] = &["vendor/larastan/larastan/extension.neon", "phpstan-baseline.neon"];
const NEON_BOOLEANS: &[&str] = &["true", "false"];

pub struct Completion {
    pub start: usize,
    pub end: usize,
    pub names: &'static [&'static str],
}

pub fn completion(path: &str, text: &str, offset: usize) -> Option<Completion> {
    let name = path.rsplit('/').next().unwrap_or(path);

    if name == "pint.json" {
        return pint(text, offset);
    }

    if name.starts_with("phpstan") && name.ends_with(".neon") {
        return neon(text, offset);
    }

    None
}

fn pint(text: &str, offset: usize) -> Option<Completion> {
    let before = &text[..offset];
    let quote = before.rfind('"')?;
    let typed = &before[quote + 1..];
    if typed.contains(['"', '\n']) || before[..quote].matches('"').count() % 2 == 1 {
        return None;
    }

    let previous = before[..quote].trim_end();
    let names = if previous.ends_with(':') && previous[..previous.len() - 1].trim_end().ends_with("\"preset\"") {
        PINT_PRESETS
    } else if previous.ends_with(':') || previous.ends_with('[') {
        return None;
    } else if inside_rules(&before[..quote]) {
        PINT_RULES
    } else {
        PINT_KEYS
    };

    Some(Completion { start: quote + 1, end: offset, names })
}

fn inside_rules(before: &str) -> bool {
    let Some(rules) = before.rfind("\"rules\"") else {
        return false;
    };
    let after = &before[rules..];
    after.matches('{').count() > after.matches('}').count()
}

fn neon(text: &str, offset: usize) -> Option<Completion> {
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let line = &text[line_start..offset];
    let indent = line.len() - line.trim_start().len();
    let content = line.trim_start();
    let section = section_of(text, line_start);
    let value = content.split_once(':').filter(|(_, rest)| rest.starts_with(' ') || rest.is_empty());
    let word_start = match value {
        Some((_, rest)) => offset - rest.trim_start().len(),
        None => offset - content.rsplit(' ').next().map_or(0, str::len),
    };

    let names: &'static [&'static str] = match value {
        Some(("level", _)) => NEON_LEVELS,
        Some((key, _)) if NEON_BOOLEAN_KEYS.contains(&key) => NEON_BOOLEANS,
        Some(_) => return None,
        None if content.contains(':') => return None,
        None if section == "includes" => NEON_INCLUDES,
        None if section == "parameters" && indent > 0 => NEON_KEYS,
        None => return None,
    };

    Some(Completion { start: word_start, end: offset, names })
}

/// The top-level key whose block holds the line that starts at `line_start`.
fn section_of(text: &str, line_start: usize) -> &str {
    text[..line_start]
        .lines()
        .rev()
        .find(|line| !line.starts_with([' ', '\t', '-', '#']) && line.trim_end().ends_with(':'))
        .map_or("", |line| line.trim_end().trim_end_matches(':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(path: &str, text: &str) -> Option<&'static [&'static str]> {
        completion(path, text, text.len()).map(|c| c.names)
    }

    #[test]
    fn pint_completes_keys_presets_and_rules() {
        assert_eq!(names("pint.json", "{\n  \"pr"), Some(PINT_KEYS));
        assert_eq!(names("pint.json", "{\n  \"preset\": \"la"), Some(PINT_PRESETS));
        assert_eq!(names("pint.json", "{\"rules\": {\n  \"sing"), Some(PINT_RULES));
        assert_eq!(names("pint.json", "{\"rules\": {\"a\": true},\n \"ex"), Some(PINT_KEYS));
        assert_eq!(names("composer.json", "{\n  \"pr"), None);
    }

    #[test]
    fn neon_completes_levels_keys_and_includes() {
        assert_eq!(names("phpstan.neon", "parameters:\n    level: "), Some(NEON_LEVELS));
        assert_eq!(names("phpstan.neon", "parameters:\n    pa"), Some(NEON_KEYS));
        assert_eq!(names("phpstan.neon", "includes:\n    - vendor/la"), Some(NEON_INCLUDES));
        assert_eq!(names("phpstan.neon", "pa"), None);
    }
}
