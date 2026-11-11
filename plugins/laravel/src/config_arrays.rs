//! Static Config arrays, including list offsets. Never evaluates PHP.
use crate::php::{self, Leaf};
use std::ops::Range;

pub const MAX_DEPTH: usize = 64;
const MAX_ENTRIES: usize = 20_000;
const MAX_SOURCE: usize = 2 * 1024 * 1024;

fn string_end(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    let quote = bytes[start];
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i = (i + 2).min(bytes.len());
        } else if bytes[i] == quote {
            return i + 1;
        } else {
            i += 1;
        }
    }
    bytes.len()
}
fn array(text: &str, mut range: Range<usize>) -> Option<usize> {
    range = php::trimmed(text, range);
    while text.as_bytes().get(range.start) == Some(&b'(')
        && php::closing(text, range.start) == Some(range.end.checked_sub(1)?)
    {
        range = php::trimmed(text, range.start + 1..range.end - 1);
    }
    let open = php::array_open(text, range.clone())?;
    (php::closing(text, open) == Some(range.end.checked_sub(1)?)).then_some(open)
}
fn returned(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' | b'"' | b'`' => {
                i = string_end(text, i);
                continue;
            }
            b'(' | b'[' | b'{' => {
                i = php::closing(text, i)? + 1;
                continue;
            }
            _ => {}
        }
        if bytes[i..].starts_with(b"return")
            && (i == 0 || !php::is_identifier(bytes[i - 1]))
            && !bytes.get(i + 6).is_some_and(|b| php::is_identifier(*b))
        {
            let start = i + 6;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b';' {
                match bytes[end] {
                    b'\'' | b'"' | b'`' => end = string_end(text, end),
                    b'(' | b'[' | b'{' => end = php::closing(text, end)? + 1,
                    _ => end += 1,
                }
            }
            return array(text, start..end);
        }
        i += 1;
    }
    None
}
fn string_key(text: &str, range: Range<usize>) -> Option<(String, usize, usize)> {
    let range = php::trimmed(text, range);
    if let Some((key, _)) = literal_key(text, range.clone(), 0) {
        if php::literal(text, range.clone()).is_none() {
            return Some((key, range.start, range.len()));
        }
    }
    let (raw, offset) = php::literal(text, range)?;
    let single = text.as_bytes()[offset - 1] == b'\'';
    if !single && (raw.contains('$') || raw.contains('\\')) {
        return None;
    }
    let key = if single {
        raw.replace("\\'", "'").replace("\\\\", "\\")
    } else {
        raw.to_owned()
    };
    (!key.is_empty()).then_some((key, offset, raw.len()))
}
fn literal_key(text: &str, range: Range<usize>, depth: usize) -> Option<(String, usize)> {
    if depth > 12 {
        return None;
    }
    let range = php::trimmed(text, range);
    if text.as_bytes().get(range.start) == Some(&b'(')
        && php::closing(text, range.start) == Some(range.end.checked_sub(1)?)
    {
        return literal_key(text, range.start + 1..range.end - 1, depth + 1);
    }
    let mut at = range.start;
    let mut parts = Vec::new();
    let mut begin = at;
    while at < range.end {
        match text.as_bytes()[at] {
            b'\'' | b'"' => at = php::string_end(text.as_bytes(), at),
            b'(' => at = php::closing(text, at)? + 1,
            b'.' => {
                parts.push(begin..at);
                begin = at + 1;
                at += 1;
            }
            _ => at += 1,
        }
    }
    if !parts.is_empty() {
        parts.push(begin..range.end);
        let mut key = String::new();
        for part in parts {
            key.push_str(&literal_key(text, part, depth + 1)?.0);
        }
        return Some((key, range.start));
    }
    let (raw, at) = php::literal(text, range)?;
    let single = text.as_bytes()[at - 1] == b'\'';
    if !single && (raw.contains('$') || raw.contains('\\')) {
        return None;
    }
    let key = if single {
        raw.replace("\\'", "'").replace("\\\\", "\\")
    } else {
        raw.to_owned()
    };
    (!key.is_empty()).then_some((key, at))
}
struct Walker<'a> {
    text: &'a str,
    out: Vec<Leaf>,
    complete: bool,
    global_incomplete: bool,
    incomplete_arrays: std::collections::BTreeSet<String>,
    key_resolver: Option<&'a dyn Fn(Range<usize>) -> Option<String>>,
}
impl Walker<'_> {
    fn mark_incomplete(&mut self, prefix: &str) {
        self.complete = false;
        self.incomplete_arrays.insert(prefix.to_owned());
    }
    fn walk(&mut self, open: usize, prefix: &str, depth: usize) {
        if depth > MAX_DEPTH {
            self.complete = false;
            self.global_incomplete = true;
            return;
        }
        let mut next = Some(0i64);
        for item in php::items(self.text, open) {
            if self.out.len() >= MAX_ENTRIES {
                self.mark_incomplete(prefix);
                self.global_incomplete = true;
                return;
            }
            let expression = php::trimmed(self.text, item.value.clone());
            let value = &self.text[expression.clone()];
            if item.key.is_none() && value.starts_with("...") {
                next = None;
                self.mark_incomplete(prefix);
                continue;
            }
            let resolved = if let Some(range) = item.key {
                let range = php::trimmed(self.text, range);
                if let Some((key, offset, width)) = string_key(self.text, range.clone()) {
                    if let Ok(number) = key.parse::<i64>()
                        && number.to_string() == key
                    {
                        next = next.and_then(|n| {
                            if number < 0 && n == 0 {
                                None
                            } else {
                                Some(n.max(number.checked_add(1)?))
                            }
                        });
                    }
                    Some((key, offset, width))
                } else if let Some(class) = self.text[range.clone()]
                    .strip_suffix("::class")
                    .and_then(|name| crate::eloquent_members::source_class_name(self.text, name))
                {
                    Some((class, range.start, range.len()))
                } else if let Ok(number) = self.text[range.clone()].parse::<i64>() {
                    let raw = &self.text[range.clone()];
                    if number.to_string() != raw && format!("+{number}") != raw {
                        next = None;
                        self.mark_incomplete(prefix);
                        continue;
                    }
                    next = next.and_then(|n| {
                        if number < 0 && n == 0 {
                            None
                        } else {
                            Some(n.max(number.checked_add(1)?))
                        }
                    });
                    Some((number.to_string(), range.start, range.len()))
                } else if let Some(key) =
                    self.key_resolver.and_then(|resolve| resolve(range.clone()))
                {
                    if let Ok(number) = key.parse::<i64>() {
                        if number.to_string() == key {
                            next = next.and_then(|n| {
                                if number < 0 && n == 0 {
                                    None
                                } else {
                                    Some(n.max(number.checked_add(1)?))
                                }
                            });
                        }
                    }
                    Some((key, range.start, range.len()))
                } else {
                    // Unknown keys may change subsequent implicit integer offsets.
                    next = None;
                    self.mark_incomplete(prefix);
                    None
                }
            } else if let Some(number) = next {
                next = number.checked_add(1);
                Some((number.to_string(), expression.start, 0))
            } else {
                self.mark_incomplete(prefix);
                None
            };
            let Some((key, offset, width)) = resolved else {
                continue;
            };
            let path = format!("{prefix}.{key}");
            let nested = array(self.text, expression.clone());
            // A computed array or merge remains a source expression, not invented child keys.
            if nested.is_none() && (value.starts_with('[') || value.starts_with("array(")) {
                self.mark_incomplete(&path);
            }
            let literal = php::literal(self.text, expression.clone()).map(|(s, _)| s.to_owned());
            self.out.push(Leaf {
                path: path.clone(),
                offset,
                width,
                value: literal,
                expression,
                array: nested.is_some(),
            });
            if let Some(open) = nested {
                self.walk(open, &path, depth + 1);
            }
        }
    }
}
/// Includes known entries even when an unsupported spread/key or bound prevents full coverage.
pub fn paths(masked: &str, prefix: &str) -> (Vec<Leaf>, bool) {
    paths_with_keys(masked, prefix, None)
}
/// A pure source-backed resolver may provide otherwise unknown scalar keys.
/// Unresolved expressions retain the existing incomplete-array semantics.
pub(crate) fn paths_with_keys(
    masked: &str,
    prefix: &str,
    key_resolver: Option<&dyn Fn(Range<usize>) -> Option<String>>,
) -> (Vec<Leaf>, bool) {
    let coverage = coverage(masked, prefix, key_resolver);
    (coverage.leaves, coverage.complete)
}
/// Which part of a config file could not be fully inventoried.
pub(crate) struct Coverage {
    pub leaves: Vec<Leaf>,
    pub complete: bool,
    /// The whole file is unknown (size, depth, entry bound, heredoc or no returned array).
    pub global: bool,
    /// Dotted arrays whose own key set is unknown; arrays elsewhere in the file stay authoritative.
    pub incomplete_arrays: std::collections::BTreeSet<String>,
}
pub(crate) fn coverage(
    masked: &str,
    prefix: &str,
    key_resolver: Option<&dyn Fn(Range<usize>) -> Option<String>>,
) -> Coverage {
    let unknown = || Coverage { leaves: vec![], complete: false, global: true, incomplete_arrays: Default::default() };
    if masked.len() > MAX_SOURCE || masked.contains("<<<") {
        return unknown();
    }
    let Some(open) = returned(masked) else {
        return unknown();
    };
    let mut walker = Walker {
        text: masked,
        out: vec![],
        complete: true,
        global_incomplete: false,
        incomplete_arrays: Default::default(),
        key_resolver,
    };
    walker.walk(open, prefix, 0);
    Coverage { leaves: walker.out, complete: walker.complete, global: walker.global_incomplete, incomplete_arrays: walker.incomplete_arrays }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lists_explicit_offsets_and_nested_legacy_arrays_have_real_paths() {
        let text = "<?php return ['items'=>[10=>'a','b','named'=>'c','d', '02'=>'e', '20'=>'f', 'g'], 'rows'=>array(array('name'=>'first'),array('name'=>'second'))];";
        let (paths, complete) = paths(text, "test");
        assert!(complete);
        for wanted in [
            "test.items.10",
            "test.items.11",
            "test.items.12",
            "test.items.02",
            "test.items.20",
            "test.items.21",
            "test.rows.0.name",
            "test.rows.1.name",
        ] {
            assert!(paths.iter().any(|p| p.path == wanted), "{wanted}");
        }
        let implicit = paths.iter().find(|p| p.path == "test.rows.0").unwrap();
        assert_eq!(implicit.width, 0);
        assert!(text[implicit.offset..].starts_with("array("));
    }
    #[test]
    fn unknown_offsets_are_withheld_without_losing_explicit_named_keys() {
        let (rows, complete) = paths(
            "<?php return [...$other,'unknown-offset','known'=>['yes'=>true],$key=>'value','also-unknown'];",
            "test",
        );
        assert!(!complete);
        assert_eq!(
            rows.iter().map(|p| p.path.as_str()).collect::<Vec<_>>(),
            ["test.known", "test.known.yes"]
        );
    }
    #[test]
    fn deep_arrays_go_past_eight_levels_and_report_the_depth_bound() {
        for depth in [20, MAX_DEPTH + 2] {
            let source = format!(
                "<?php return {}1{};",
                "['child'=>".repeat(depth),
                "]".repeat(depth)
            );
            let (rows, complete) = paths(&source, "deep");
            assert_eq!(complete, depth <= MAX_DEPTH + 1);
            assert_eq!(rows.len(), depth.min(MAX_DEPTH + 1));
        }
    }
    #[test]
    fn returned_arrays_ignore_string_trivia_and_do_not_assume_merges() {
        let (rows, complete) = paths(
            "<?php $example='return [wrong]';return (['ok'=>true]);",
            "test",
        );
        assert!(complete);
        assert_eq!(rows[0].path, "test.ok");
        assert!(!paths("<?php return array_merge(['fake'=>true],other());", "test").1);
        let (rows, complete) = paths("<?php return ['combined'=>['fake'=>true]+other()];", "test");
        assert!(!complete);
        assert_eq!(rows.len(), 1);
    }
}

#[cfg(test)]
mod class_key_tests {
    use super::*;
    #[test]
    fn literal_class_keys_resolve_imports_without_loading_classes() {
        let source = r"<?php
use Vendor\Package\Handler as Alias;
return [Alias::class => ['enabled'=>true], \DateTimeZone::class => 'cast'];";
        let (rows, complete) = paths(source, "classes");
        assert!(complete);
        assert!(
            rows.iter()
                .any(|p| p.path == r"classes.Vendor\Package\Handler.enabled")
        );
        assert!(rows.iter().any(|p| p.path == "classes.DateTimeZone"));
    }
    #[test]
    fn nondecimal_integer_keys_and_heredocs_are_not_guessed() {
        let (rows, complete) = paths("<?php return [010=>'octal','unknown-index'];", "test");
        assert!(!complete);
        assert!(rows.is_empty());
        assert!(!paths("<?php return ['text'=><<<TXT\nhi\nTXT];", "test").1);
    }
    #[test]
    fn coverage_names_only_the_arrays_whose_keys_are_unknown() {
        let source = "<?php return ['name'=>'x','previous_keys'=>[...array_filter(explode(',',env('K')))],'links'=>[public_path('s')=>storage_path('a')],'ok'=>['a'=>1]];";
        let found = coverage(source, "app", None);
        assert!(!found.complete && !found.global);
        assert_eq!(found.incomplete_arrays.into_iter().collect::<Vec<_>>(), ["app.links", "app.previous_keys"]);
        let spread = coverage("<?php return [...$a,'k'=>1];", "app", None);
        assert_eq!(spread.incomplete_arrays.into_iter().collect::<Vec<_>>(), ["app"]);
        assert!(coverage("<?php return foo();", "app", None).global);
    }
}
