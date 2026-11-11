//! Source-backed scalar configuration keys. No enum object or PHP is evaluated.
use crate::{
    Index,
    index::{AddName, Lines},
    php,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};
pub(crate) type Sources = BTreeMap<String, Option<String>>;
const MAX_BYTES: usize = 2 * 1024 * 1024;
fn source<'a>(sources: &'a Sources, root: &str, path: &str) -> Option<&'a str> {
    let relative = path
        .strip_prefix(root)
        .and_then(|path| path.strip_prefix('/'))
        .unwrap_or(path);
    let absolute = format!("{}/{relative}", root.trim_end_matches('/'));
    let a = sources.get(&absolute);
    let b = sources.get(relative);
    if a.is_some() && b.is_some() && a != b {
        return None;
    }
    a.or(b)?.as_deref()
}
fn unparen(text: &str, mut range: Range<usize>) -> Option<Range<usize>> {
    range = php::trimmed(text, range);
    while text.as_bytes().get(range.start) == Some(&b'(')
        && php::closing(text, range.start) == Some(range.end.checked_sub(1)?)
    {
        range = php::trimmed(text, range.start + 1..range.end - 1);
    }
    Some(range)
}
fn parts(text: &str, range: Range<usize>) -> Option<Vec<Range<usize>>> {
    let mut at = range.start;
    let mut begin = at;
    let mut out = Vec::new();
    while at < range.end {
        match text.as_bytes()[at] {
            b'\'' | b'"' => at = php::string_end(text.as_bytes(), at),
            b'(' => at = php::closing(text, at)? + 1,
            b'.' if !(at > range.start
                && text.as_bytes()[at - 1].is_ascii_digit()
                && text.as_bytes().get(at + 1).is_some_and(u8::is_ascii_digit)) =>
            {
                out.push(begin..at);
                begin = at + 1;
                at += 1;
            }
            _ => at += 1,
        }
    }
    if out.is_empty() {
        None
    } else {
        out.push(begin..range.end);
        Some(out)
    }
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && !value.as_bytes()[0].is_ascii_digit()
        && value
            .bytes()
            .all(|byte| php::is_identifier(byte) || byte >= 128)
}
fn class_name(value: &str) -> bool {
    let value = value.trim_start_matches('\\');
    !value.is_empty() && value.split('\\').all(identifier)
}
type EnumCases = (
    String,
    Vec<(String, Option<Value>, crate::index::Entry)>,
    Option<String>,
);
struct Resolver<'a> {
    index: &'a Index,
    root: &'a str,
    sources: &'a Sources,
    owns: &'a dyn Fn(&str) -> bool,
    dependencies: BTreeSet<String>,
    enum_memo: BTreeMap<String, Option<std::sync::Arc<EnumCases>>>,
    blocked: &'a BTreeSet<String>,
}
impl Resolver<'_> {
    fn enum_info(&mut self, full: &str) -> Option<std::sync::Arc<EnumCases>> {
        let key = full.to_ascii_lowercase();
        if let Some(info) = self.enum_memo.get(&key) {
            return info.clone();
        }
        let info = (|| {
            let cached = crate::eloquent_members::enum_values(self.index, full, self.owns)?;
            self.dependencies.insert(cached.0.clone());
            let text = source(self.sources, self.root, &cached.0)?;
            if text.len() > 128 * 1024 {
                return None;
            }
            let mut checked = Index::default();
            crate::eloquent_members::cache_enum(&mut checked.eloquent_enums, text, &cached.0);
            if crate::eloquent_members::enum_values(&checked, full, self.owns).as_ref()
                != Some(&cached)
            {
                return None;
            }
            Some(std::sync::Arc::new(cached))
        })();
        self.enum_memo.insert(key, info.clone());
        info
    }
    fn scalar(&mut self, text: &str, range: Range<usize>, depth: usize) -> Option<Value> {
        if depth > 12 {
            return None;
        }
        let range = unparen(text, range)?;
        let expression = text.get(range.clone())?;
        if expression.contains(['\r', '\n']) {
            return None;
        }
        if let Some(parts) = parts(text, range.clone()) {
            let mut value = String::new();
            for part in parts {
                match self.scalar(text, part, depth + 1)? {
                    Value::String(part) => value.push_str(&part),
                    Value::Number(part) => value.push_str(&part.to_string()),
                    _ => return None,
                }
            }
            return Some(Value::String(value));
        }
        if let Some((raw, at)) = php::literal(text, range.clone()) {
            return crate::eloquent_members::metadata_inference::decode_literal(
                raw,
                text.as_bytes()[at - 1],
            )
            .map(Value::String);
        }
        if let Some(number) = crate::eloquent_members::integer_literal(expression) {
            return Some(Value::from(number));
        }
        let compact = expression
            .chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect::<String>();
        if !compact.contains("->") {
            let (class, member) = compact.split_once("::")?;
            if !class_name(class) || !identifier(member) {
                return None;
            }
            let full = crate::eloquent_members::source_class_name(text, class)?;
            if self.blocked.contains(&full.to_ascii_lowercase()) {
                return None;
            }
            if member.eq_ignore_ascii_case("class") {
                return Some(Value::String(full));
            }
            let identities = crate::eloquent_members::class_inventory(&self.index.eloquent_models);
            let matches = identities
                .iter()
                .filter(|identity| identity.full.eq_ignore_ascii_case(&full))
                .collect::<Vec<_>>();
            let path =
                if matches.len() == 1 && !matches[0].ambiguous && (self.owns)(&matches[0].path) {
                    matches[0].path.clone()
                } else {
                    crate::eloquent_members::enum_values(self.index, &full, self.owns)?.0
                };
            self.dependencies.insert(path.clone());
            let declaration = source(self.sources, self.root, &path)?;
            let (actual, constants) = crate::eloquent_members::literal_constants(declaration)?;
            if !actual.eq_ignore_ascii_case(&full) {
                return None;
            }
            let value = constants.get(member)?.clone();
            self.dependencies.insert(path);
            return Some(value);
        }
        let (head, property) = compact.rsplit_once("->")?;
        let head = head.strip_suffix('?').unwrap_or(head);
        if !matches!(property, "name" | "value") {
            return None;
        }
        let (class, case) = head.split_once("::")?;
        if !class_name(class) || !identifier(case) {
            return None;
        }
        let full = crate::eloquent_members::source_class_name(text, class)?;
        if self.blocked.contains(&full.to_ascii_lowercase()) {
            return None;
        }
        let cached = self.enum_info(&full)?;
        let enum_source = source(self.sources, self.root, &cached.0)?;
        let (_, backing, entry) = cached.1.iter().find(|(name, _, _)| name == case)?;
        let at = openidea_plugin::text::offset(enum_source, entry.at)?;
        let after = at + case.len();
        let masked = php::mask_comments(enum_source);
        let tail = masked.get(after..)?.trim_start();
        let value = if let Some(tail) = tail.strip_prefix('=') {
            let begin = masked.len() - tail.len();
            let mut end = begin;
            while end < masked.len() {
                match masked.as_bytes()[end] {
                    b'\'' | b'"' => end = php::string_end(masked.as_bytes(), end),
                    b'(' => end = php::closing(&masked, end)? + 1,
                    b';' => break,
                    _ => end += 1,
                }
            }
            let value = self.scalar(&masked, begin..end, depth + 1)?;
            match (cached.2.as_deref(), &value) {
                (Some("string"), Value::String(_)) | (Some("int"), Value::Number(_)) => {}
                _ => return None,
            }
            Some(value)
        } else if cached.2.is_none() && tail.starts_with(';') {
            None
        } else {
            return None;
        };
        // The cached backing may be absent for a compound literal. Its exact
        // current source, not a stale or incorrectly decimalized value, wins.
        let _ = backing;
        self.dependencies.insert(cached.0.clone());
        if property == "name" {
            Some(Value::String(case.into()))
        } else {
            value
        }
    }
}
pub(crate) fn paths(
    index: &Index,
    root: &str,
    sources: &Sources,
    owns: &dyn Fn(&str) -> bool,
    text: &str,
    prefix: &str,
) -> (Vec<php::Leaf>, bool, BTreeSet<String>) {
    resolved_paths(index, root, sources, owns, text, prefix, &BTreeSet::new())
}
fn resolved_paths(
    index: &Index,
    root: &str,
    sources: &Sources,
    owns: &dyn Fn(&str) -> bool,
    text: &str,
    prefix: &str,
    blocked: &BTreeSet<String>,
) -> (Vec<php::Leaf>, bool, BTreeSet<String>) {
    let resolver = std::cell::RefCell::new(Resolver {
        index,
        root,
        sources,
        owns,
        dependencies: BTreeSet::new(),
        enum_memo: BTreeMap::new(),
        blocked,
    });
    let calls = std::cell::Cell::new(0usize);
    let key = |range: Range<usize>| {
        calls.set(calls.get() + 1);
        if !crate::view_rename::binding_budget(text.len(), calls.get()) {
            return None;
        }
        match resolver.borrow_mut().scalar(text, range, 0)? {
            Value::String(value) => Some(value),
            Value::Number(value) => value.as_i64().map(|value| value.to_string()),
            _ => None,
        }
    };
    let (leaves, complete) = crate::config_arrays::paths_with_keys(text, prefix, Some(&key));
    let dependencies = resolver.into_inner().dependencies;
    (leaves, complete, dependencies)
}
fn references(text: &str, range: Range<usize>, out: &mut BTreeSet<String>, depth: usize) {
    if depth > 12 {
        return;
    }
    let Some(range) = unparen(text, range) else {
        return;
    };
    if let Some(parts) = parts(text, range.clone()) {
        for part in parts {
            references(text, part, out, depth + 1);
        }
        return;
    }
    let compact = text[range]
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect::<String>();
    let Some((class, tail)) = compact.split_once("::") else {
        return;
    };
    if !class_name(class) {
        return;
    }
    let member = if let Some((case, property)) = tail.rsplit_once("->") {
        if !matches!(property, "name" | "value") {
            return;
        }
        case.trim_end_matches('?')
    } else {
        tail
    };
    if !identifier(member) || member.eq_ignore_ascii_case("class") {
        return;
    }
    if let Some(full) = crate::eloquent_members::source_class_name(text, class) {
        out.insert(full);
    }
}
fn config_relative<'a>(root: &str, path: &'a str) -> Option<&'a str> {
    let relative = path
        .strip_prefix(root)
        .and_then(|p| p.strip_prefix('/'))
        .unwrap_or(path);
    relative.strip_prefix("config/")?.strip_suffix(".php")?;
    Some(relative)
}
fn issue_budget(index: &mut Index) {
    let issue = "Enum-backed config key inventory exceeded its 2-MiB source budget; unresolved files remain incomplete";
    if !index.metadata.issues.iter().any(|value| value == issue) {
        index.metadata.issues.push(issue.into());
    }
}
/// Load only referenced classes through installed Composer descriptors. Existing
/// captured sources are reused; added reads carry the shared exact evidence.
pub(crate) fn apply_project(
    project: &openidea_plugin::Project,
    index: &mut Index,
    root: &str,
    sources: &Sources,
    owns: &dyn Fn(&str) -> bool,
) {
    let mut pending = BTreeSet::new();
    let mut bytes = 0usize;
    for (path, text) in sources {
        let Some(text) = text else {
            continue;
        };
        if config_relative(root, path).is_none() || !text.contains("::") {
            continue;
        }
        if bytes.saturating_add(text.len()) > MAX_BYTES {
            issue_budget(index);
            continue;
        }
        bytes += text.len();
        let masked = php::mask_comments(text);
        let collect = |range| {
            references(&masked, range, &mut pending, 0);
            None
        };
        let collect = std::cell::RefCell::new(collect);
        let _ = crate::config_arrays::paths_with_keys(
            &masked,
            "config",
            Some(&|range| (collect.borrow_mut())(range)),
        );
    }
    let mut combined = sources.clone();
    let mut blocked = BTreeSet::new();
    let mut seen = BTreeSet::new();
    while let Some(full) = pending.pop_first() {
        let lower = full.to_ascii_lowercase();
        if !seen.insert(lower.clone()) {
            continue;
        }
        if seen.len() > 64 {
            blocked.insert(lower);
            issue_budget(index);
            continue;
        }
        let roots = index
            .packages
            .roots
            .iter()
            .filter(|candidate| full.starts_with(&candidate.namespace))
            .cloned()
            .collect::<Vec<_>>();
        let candidates = roots
            .iter()
            .map(|candidate| {
                (
                    candidate.package.clone(),
                    format!(
                        "{}/{}.php",
                        candidate.directory,
                        full[candidate.namespace.len()..].replace('\\', "/")
                    ),
                )
            })
            .collect::<BTreeSet<_>>();
        if candidates.len() > 8 {
            blocked.insert(lower);
            issue_budget(index);
            continue;
        }
        let mut packages = BTreeSet::new();
        let mut declarations = BTreeMap::new();
        let mut invalid = false;
        for (package, relative) in candidates {
            let absolute = format!("{}/{relative}", root.trim_end_matches('/'));
            index.dependency_files.insert(relative.clone());
            if !owns(&absolute) {
                invalid = true;
                continue;
            }
            if let Some(text) = source(sources, root, &absolute) {
                if text.len() > 128 * 1024 || bytes.saturating_add(text.len()) > MAX_BYTES {
                    invalid = true;
                    issue_budget(index);
                    continue;
                }
                bytes += text.len();
                if crate::eloquent_members::declaration(text).as_deref() == Some(full.as_str()) {
                    declarations.insert(absolute, text.to_owned());
                }
            } else {
                match project.stat(&absolute) {
                    Ok(stat)
                        if stat.kind == openidea_plugin::FileKind::File
                            && stat.size <= 128 * 1024
                            && bytes.saturating_add(stat.size as usize) <= MAX_BYTES =>
                    {
                        packages.insert(package);
                    }
                    Err(openidea_plugin::Error::NotFound(_)) => {
                        packages.insert(package);
                    }
                    _ => {
                        invalid = true;
                        issue_budget(index);
                    }
                }
            }
        }
        if invalid {
            blocked.insert(lower);
            continue;
        }
        for package in packages {
            if let Some((relative, text)) =
                crate::package_integrations::snapshot_class(project, index, &package, &full)
            {
                if bytes.saturating_add(text.len()) > MAX_BYTES {
                    invalid = true;
                    issue_budget(index);
                    break;
                }
                bytes += text.len();
                let absolute = format!(
                    "{}/{}",
                    root.trim_end_matches('/'),
                    relative.trim_start_matches('/')
                );
                if let Some(old) = source(sources, root, &absolute) {
                    if old != text {
                        invalid = true;
                        break;
                    }
                }
                declarations.insert(absolute, text);
            }
        }
        if invalid || declarations.len() > 1 {
            blocked.insert(lower);
            continue;
        }
        for (path, text) in declarations {
            crate::eloquent_members::cache_enum(&mut index.eloquent_enums, &text, &path);
            crate::eloquent_members::cache(&mut index.eloquent_models, &text, &path);
            combined.insert(path.clone(), Some(text.clone()));
            if let Some((_, cases, _)) = crate::eloquent_members::enum_values(index, &full, owns) {
                let masked = php::mask_comments(&text);
                for (case, _, entry) in cases {
                    let Some(start) =
                        openidea_plugin::text::offset(&text, entry.at).map(|at| at + case.len())
                    else {
                        continue;
                    };
                    let tail = masked[start..].trim_start();
                    let Some(tail) = tail.strip_prefix('=') else {
                        continue;
                    };
                    let begin = masked.len() - tail.len();
                    let mut end = begin;
                    while end < masked.len() {
                        match masked.as_bytes()[end] {
                            b'\'' | b'"' => end = php::string_end(masked.as_bytes(), end),
                            b'(' => {
                                let Some(close) = php::closing(&masked, end) else {
                                    break;
                                };
                                end = close + 1;
                            }
                            b';' => break,
                            _ => end += 1,
                        }
                    }
                    references(&masked, begin..end, &mut pending, 0);
                }
            }
        }
    }
    apply_inner(index, root, &combined, owns, &blocked);
}

/// Apply after enum discovery and before package/default config merging. Inputs
/// are the builder's captured sources; this performs no reads or whole builds.
pub(crate) fn apply(index: &mut Index, root: &str, sources: &Sources, owns: &dyn Fn(&str) -> bool) {
    apply_inner(index, root, sources, owns, &BTreeSet::new())
}
fn apply_inner(
    index: &mut Index,
    root: &str,
    sources: &Sources,
    owns: &dyn Fn(&str) -> bool,
    blocked: &BTreeSet<String>,
) {
    let mut bytes = 0usize;
    for (path, captured) in sources {
        let Some(text) = captured else {
            continue;
        };
        let relative = path
            .strip_prefix(root)
            .and_then(|p| p.strip_prefix('/'))
            .unwrap_or(path);
        let Some(prefix) = relative
            .strip_prefix("config/")
            .and_then(|p| p.strip_suffix(".php"))
        else {
            continue;
        };
        if !text.contains("::") {
            continue;
        }
        let absolute = format!("{}/{relative}", root.trim_end_matches('/'));
        if !owns(&absolute) {
            continue;
        }
        bytes = bytes.saturating_add(text.len());
        if bytes > MAX_BYTES {
            let issue = "Enum-backed config key inventory exceeded its 2-MiB source budget; unresolved files remain incomplete";
            if !index.metadata.issues.iter().any(|message| message == issue) {
                index.metadata.issues.push(issue.into());
            }
            index.config_incomplete.insert(absolute);
            continue;
        }
        let masked = php::mask_comments(text);
        let prefix = prefix.replace('/', ".");
        let (leaves, complete, dependencies) =
            resolved_paths(index, root, sources, owns, &masked, &prefix, blocked);
        let old = index
            .config
            .iter()
            .filter(|(name, entry)| entry.path == absolute && name.as_str() != prefix)
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        for name in old {
            index.config.remove(&name);
            index.config_expressions.remove(&name);
        }
        if complete {
            index.config_incomplete.remove(&absolute);
        } else {
            index.config_incomplete.insert(absolute.clone());
        }
        for dependency in dependencies {
            if let Some(relative) = dependency
                .strip_prefix(root)
                .and_then(|p| p.strip_prefix('/'))
            {
                index.dependency_files.insert(relative.into());
            }
        }
        let lines = Lines::new(text);
        for leaf in leaves {
            if !leaf.array {
                let raw = masked[leaf.expression.clone()].trim();
                let mut preview = raw.chars().take(512).collect::<String>();
                if raw.chars().count() > 512 {
                    preview.push('…');
                }
                index.config_expressions.insert(leaf.path.clone(), preview);
            }
            let detail = if leaf.array {
                Some("array".into())
            } else {
                leaf.value
            };
            index.config.add(
                &leaf.path,
                detail,
                &absolute,
                lines.position(leaf.offset),
                lines.width(leaf.offset, leaf.width),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const KEY: &str = r#"<?php namespace App;
enum Key:string{case Enabled='enabled';case Other='other';case Zero='0';case Leading='01';case Plus='+1';case Minus='-1';case NegativeZero='-0';case Decimal='1.5';case Exponent='1e2';case Limit='9223372036854775808';case Empty='';case Unicode='é😀';case Dotted='group.key';case Backslash='x\\y';}
"#;
    const INTS: &str = "<?php namespace App; enum Number:int{case Octal=010;case Binary=0b1_010;case Hex=0xFF;case Two=2;}";
    fn fixture() -> (Index, Sources) {
        let mut index = Index::default();
        let mut sources = Sources::new();
        for (path, text) in [
            ("/project/app/Key.php", KEY),
            ("/project/app/Number.php", INTS),
            (
                "/project/app/Flag.php",
                "<?php namespace App;enum Flag{case Other;}",
            ),
            (
                "/project/app/Keys.php",
                "<?php namespace App;class Keys{public const ENABLED='constant';const HEX=0x10;private const SECRET='secret';const DYNAMIC=make();}",
            ),
        ] {
            crate::eloquent_members::cache_enum(&mut index.eloquent_enums, text, path);
            crate::eloquent_members::cache(&mut index.eloquent_models, text, path);
            sources.insert(path.into(), Some(text.into()));
        }
        (index, sources)
    }
    #[test]
    fn enums_constants_literal_concat_and_parentheses_keep_key_expression_destinations() {
        let (index, sources) = fixture();
        let source = r#"<?php /* 😀 */ use App\Key as Alias;use App\Keys;use App\Number;use App\Flag;return [Alias::Enabled->value=>['nested'=>true],Alias::Other->name=>false,(Alias::Unicode->value).'tail'=>true,Keys::ENABLED=>true,Keys::HEX=>true,Number::Octal->value=>true,Flag::Other->name=>true,'foo'.('bar')=>true];"#;
        let (rows, complete, deps) = paths(
            &index,
            "/project",
            &sources,
            &|path| path.starts_with("/project/"),
            source,
            "foo",
        );
        assert!(complete);
        for wanted in [
            "foo.enabled",
            "foo.enabled.nested",
            "foo.Other",
            "foo.é😀tail",
            "foo.constant",
            "foo.16",
            "foo.8",
            "foo.foobar",
        ] {
            assert!(rows.iter().any(|row| row.path == wanted), "{wanted}");
        }
        let key = rows.iter().find(|row| row.path == "foo.enabled").unwrap();
        assert_eq!(
            &source[key.offset..key.offset + key.width],
            "Alias::Enabled->value"
        );
        assert_eq!(
            openidea_plugin::text::position(source, key.offset).character,
            source[..key.offset].encode_utf16().count() as u32
        );
        assert!(deps.contains("/project/app/Key.php"));
        assert!(deps.contains("/project/app/Keys.php"));
    }
    #[test]
    fn known_keys_survive_unknown_cases_objects_dynamic_values_and_private_constants() {
        let (index, sources) = fixture();
        let source = r#"<?php use App\Key;use App\Keys;use App\Flag;return [Key::Enabled->value=>true,Key::Missing->value=>false,Key::Enabled=>false,Key::Enabled->Name=>false,Flag::Other->value=>false,Keys::SECRET=>false,Keys::DYNAMIC=>false,$dynamic=>false,'known'=>['child'=>true]];"#;
        let (rows, complete, deps) = paths(&index, "/project", &sources, &|_| true, source, "foo");
        assert!(!complete);
        assert_eq!(
            rows.iter().map(|row| row.path.as_str()).collect::<Vec<_>>(),
            ["foo.enabled", "foo.known", "foo.known.child"]
        );
        assert!(deps.contains("/project/app/Key.php"));
    }
    #[test]
    fn fresh_owned_enum_source_and_unambiguous_import_identity_are_required() {
        let (mut index, mut sources) = fixture();
        let source =
            "<?php use App\\Key as Alias;return [Alias::Enabled->value=>true,'known'=>true];";
        assert!(
            !paths(
                &index,
                "/project",
                &sources,
                &|path| !path.ends_with("Key.php"),
                source,
                "foo"
            )
            .1
        );
        sources.insert(
            "/project/app/Key.php".into(),
            Some(KEY.replace("'enabled'", "'changed'")),
        );
        let (rows, complete, _) = paths(&index, "/project", &sources, &|_| true, source, "foo");
        assert!(!complete);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "foo.known");
        sources.insert("/project/app/Key.php".into(), Some(KEY.into()));
        crate::eloquent_members::cache_enum(
            &mut index.eloquent_enums,
            KEY,
            "/project/elsewhere/Key.php",
        );
        assert!(!paths(&index, "/project", &sources, &|_| true, source, "foo").1);
        let (index, sources) = fixture();
        assert!(
            !paths(
                &index,
                "/project",
                &sources,
                &|_| true,
                "<?php use Other\\Key;return [Key::Enabled->value=>true];",
                "foo"
            )
            .1
        );
    }
    #[test]
    fn numeric_backings_advance_implicit_offsets_with_native_integer_keys() {
        let (index, sources) = fixture();
        let source = r#"<?php use App\Key;use App\Number;return [Key::Zero->value=>'zero','implicit',Key::Leading->value=>'string',Number::Hex->value=>'hex','next'];"#;
        let (rows, complete, _) = paths(&index, "/project", &sources, &|_| true, source, "foo");
        assert!(complete);
        assert_eq!(
            rows.iter().map(|row| row.path.as_str()).collect::<Vec<_>>(),
            ["foo.0", "foo.1", "foo.01", "foo.255", "foo.256"]
        );
    }
    #[test]
    fn coerced_duplicate_keys_keep_all_source_records_for_ambiguity_checks() {
        let (index, sources) = fixture();
        let text =
            r"<?php use App\Key;return [Key::Zero->value=>['old'=>true],0=>'after','other'=>true];";
        let (rows, complete, _) = paths(&index, "/project", &sources, &|_| true, text, "foo");
        assert!(complete);
        assert_eq!(rows.iter().filter(|row| row.path == "foo.0").count(), 2);
        assert_eq!(
            rows.iter()
                .filter(|row| row.path == "foo.0")
                .map(|row| &text[row.offset..row.offset + row.width])
                .collect::<Vec<_>>(),
            ["Key::Zero->value", "0"]
        );
    }
    #[test]
    fn applying_resolved_rows_clears_only_complete_file_and_records_enum_dependencies() {
        let (mut index, mut sources) = fixture();
        let source = "<?php use App\\Key;return [Key::Enabled->value=>['child'=>true]];";
        sources.insert("/project/config/foo.php".into(), Some(source.into()));
        index
            .config_incomplete
            .insert("/project/config/foo.php".into());
        index.config.add(
            "foo",
            Some("config file".into()),
            "/project/config/foo.php",
            openidea_plugin::Position::new(0, 0),
            0,
        );
        apply(&mut index, "/project", &sources, &|_| true);
        assert!(index.config.contains_key("foo.enabled.child"));
        assert!(!index.config_incomplete.contains("/project/config/foo.php"));
        assert!(index.dependency_files.contains("app/Key.php"));
        let entry = &index.config["foo.enabled"];
        let begin = openidea_plugin::text::offset(source, entry.at).unwrap();
        assert_eq!(
            &source[begin..begin + entry.width as usize],
            "Key::Enabled->value"
        );
    }
    #[test]
    #[ignore = "requires php >=8.1; compares actual enum key types, values and implicit offsets"]
    fn actual_php_enum_backed_keys_match_fourteen_native_string_coercions_and_integer_syntax() {
        let (index, sources) = fixture();
        let cases = [
            "Enabled",
            "Other",
            "Zero",
            "Leading",
            "Plus",
            "Minus",
            "NegativeZero",
            "Decimal",
            "Exponent",
            "Limit",
            "Empty",
            "Unicode",
            "Dotted",
            "Backslash",
        ];
        let entries = cases
            .iter()
            .map(|case| format!("\\App\\Key::{case}->value=>true"))
            .collect::<Vec<_>>()
            .join(",");
        let config = format!(
            r"<?php return [{entries}, 'implicit',\App\Number::Octal->value=>true,\App\Number::Binary->value=>true,\App\Number::Hex->value=>true,\App\Number::Two->value=>true,'after-integers'];"
        );
        let (rows, complete, _) = paths(&index, "/project", &sources, &|_| true, &config, "foo");
        assert!(complete);
        let program = format!(
            "{} {} $keys=include $argv[1];echo json_encode(array_keys($keys),JSON_THROW_ON_ERROR);",
            KEY,
            INTS.trim_start_matches("<?php")
        );
        let folder =
            std::env::temp_dir().join(format!("laravel-enum-config-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("keys.php");
        std::fs::write(&file, program).unwrap();
        let config_file = folder.join("config.php");
        std::fs::write(&config_file, &config).unwrap();
        let output = std::process::Command::new("php")
            .arg(&file)
            .arg(&config_file)
            .output()
            .unwrap();
        let _ = std::fs::remove_dir_all(folder);
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let native: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
        let native = native
            .into_iter()
            .map(|key| match key {
                Value::String(key) => format!("foo.{key}"),
                Value::Number(key) => format!("foo.{key}"),
                _ => panic!("native array key type"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            rows.into_iter().map(|row| row.path).collect::<Vec<_>>(),
            native
        );
    }
}

#[cfg(test)]
mod provider_tests {
    use openidea_plugin::{
        Event, Plugin,
        mock::MockHost,
        providers::{Document, Providers},
    };
    fn setup(config: &str) -> (MockHost, crate::Main) {
        let host = MockHost::install();
        host.add_file("artisan", "<?php");
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        host.add_file(
            "app/Key.php",
            "<?php namespace App;enum Key:string{case Enabled='enabled';case Other='other';}",
        );
        host.add_file("config/foo.php", config);
        let mut main = crate::Main::default();
        main.activate(&host.context("aritti.laravel")).unwrap();
        crate::tests::finish_indexing(&mut main);
        (host, main)
    }
    fn document(text: &str) -> Document {
        Document {
            path: "/project/routes/use.php".into(),
            language: "php".into(),
            version: 1,
            text: text.into(),
        }
    }
    #[test]
    fn real_main_completion_and_definition_select_full_unicode_key_expression() {
        let config = "<?php /* 😀 */ use App\\Key as Κλειδί;return [Κλειδί::Enabled->value=>['nested'=>true],Κλειδί::Other->name=>false];";
        let (host, mut main) = setup(config);
        let source = "<?php config('foo.en');";
        let at = openidea_plugin::text::position(source, source.find("foo.en").unwrap() + 6);
        let items = main.completion(&document(source), at, None);
        assert!(
            items.iter().any(|item| item.label == "foo.enabled"),
            "config {:?}; enum {:?}; evidence {:?}",
            main.completed_index_for("/project")
                .unwrap()
                .config
                .keys()
                .collect::<Vec<_>>(),
            crate::eloquent_members::enum_values(
                main.completed_index_for("/project").unwrap(),
                "App\\Key",
                &|_| true
            ),
            main.completed_index_for("/project")
                .unwrap()
                .source_evidence
                .keys()
                .collect::<Vec<_>>()
        );
        assert!(items.iter().any(|item| item.label == "foo.enabled.nested"));
        let source = "<?php /* 😀 */ config('foo.enabled');";
        let targets = main.definition(
            &document(source),
            openidea_plugin::text::position(source, source.find("enabled").unwrap() + 2),
        );
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].path, "/project/config/foo.php");
        let range = targets[0].range;
        let start = openidea_plugin::text::offset(config, range.start).unwrap();
        let end = openidea_plugin::text::offset(config, range.end).unwrap();
        assert_eq!(&config[start..end], "Κλειδί::Enabled->value");
        let index = main.completed_index_for("/project").unwrap();
        assert!(index.config.contains_key("foo.Other"));
        assert!(!index.config_incomplete.contains("/project/config/foo.php"));
        assert!(index.affects("app/Key.php"));
        assert!(crate::index::index_sources_current(
            &host.invocation().project().unwrap(),
            index
        ));
        host.add_file(
            "app/Key.php",
            "<?php namespace App;enum Key:string{case Enabled='changed';case Other='other';}",
        );
        assert!(!crate::index::index_sources_current(
            &host.invocation().project().unwrap(),
            index
        ));
        main.handle_events(
            &[Event::FileSaved("/project/app/Key.php".into())],
            &host.invocation(),
        );
        crate::tests::finish_indexing(&mut main);
        let index = main.completed_index_for("/project").unwrap();
        assert!(index.config.contains_key("foo.changed"));
        assert!(!index.config.contains_key("foo.enabled"));
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn unresolved_enum_keys_preserve_known_siblings_and_suppress_false_missing_config() {
        let config = "<?php use App\\Key;return [Key::Enabled->value=>true,Key::Missing->value=>false,$dynamic=>1,'known'=>true];";
        let (_host, mut main) = setup(config);
        let index = main.completed_index_for("/project").unwrap();
        assert!(index.config.contains_key("foo.enabled"));
        assert!(index.config.contains_key("foo.known"));
        assert!(index.config_incomplete.contains("/project/config/foo.php"));
        assert!(!crate::config_inspections::missing(index, "foo.missing"));
        let source = "<?php config('foo.Missing');";
        assert!(
            main.definition(
                &document(source),
                openidea_plugin::text::position(source, source.find("Missing").unwrap() + 1)
            )
            .is_empty()
        );
    }
    fn installed_sources(host: &MockHost) {
        host.add_file("vendor/composer/installed.json",r#"{"packages":[{"name":"acme/config","version":"1.0.0","install-path":"../acme/config","autoload":{"psr-4":{"Vendor\\Pkg\\":"src/"}}},{"name":"symfony/http-foundation","version":"7.0.0","install-path":"../symfony/http-foundation","autoload":{"psr-4":{"Symfony\\Component\\HttpFoundation\\":""}}}]}"#);
        host.add_file("vendor/acme/config/src/Key.php","<?php namespace Vendor\\Pkg;enum Key:string{case Enabled='package-enabled';case Other='other';}");
        host.add_file("vendor/symfony/http-foundation/Response.php","<?php namespace Symfony\\Component\\HttpFoundation;class Response{public const HTTP_OK=200;}");
    }
    #[test]
    fn installed_vendor_enum_and_standard_class_constant_keys_are_loaded_by_exact_descriptors() {
        let config = r"<?php use Vendor\Pkg\Key as PackageKey;use Symfony\Component\HttpFoundation\Response;return [PackageKey::Enabled->value=>true,PackageKey::Other->name=>false,Response::HTTP_OK=>['status'=>true]];";
        let (host, mut main) = setup(config);
        installed_sources(&host);
        let project = host.invocation().project().unwrap();
        main.queue_index(&project);
        crate::tests::finish_indexing(&mut main);
        let index = main.completed_index_for("/project").unwrap();
        for wanted in [
            "foo.package-enabled",
            "foo.Other",
            "foo.200",
            "foo.200.status",
        ] {
            assert!(index.config.contains_key(wanted), "{wanted}");
        }
        assert!(!index.config_incomplete.contains("/project/config/foo.php"));
        assert!(
            index
                .source_evidence
                .get("vendor/acme/config/src/Key.php")
                .is_some_and(Option::is_some)
        );
        assert!(
            index
                .source_evidence
                .get("vendor/symfony/http-foundation/Response.php")
                .is_some_and(Option::is_some)
        );
        assert!(crate::index::index_sources_current(&project, index));
        let source = "<?php config('foo.200');";
        let targets = main.definition(
            &document(source),
            openidea_plugin::text::position(source, source.find("200").unwrap() + 1),
        );
        assert_eq!(targets.len(), 1);
        let begin = openidea_plugin::text::offset(config, targets[0].range.start).unwrap();
        let end = openidea_plugin::text::offset(config, targets[0].range.end).unwrap();
        assert_eq!(&config[begin..end], "Response::HTTP_OK");
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn absent_descriptors_foreign_vendor_owners_and_ambiguous_class_paths_do_not_publish_keys() {
        let config = r"<?php use Vendor\Pkg\Key;return [Key::Enabled->value=>true,'known'=>true];";
        let (host, mut main) = setup(config);
        host.add_file(
            "vendor/acme/config/src/Key.php",
            "<?php namespace Vendor\\Pkg;enum Key:string{case Enabled='package-enabled';}",
        );
        let project = host.invocation().project().unwrap();
        main.queue_index(&project);
        crate::tests::finish_indexing(&mut main);
        assert!(
            !main
                .completed_index_for("/project")
                .unwrap()
                .config
                .contains_key("foo.package-enabled")
        );
        installed_sources(&host);
        host.add_project("/project/vendor/acme");
        main.queue_index(&project);
        crate::tests::finish_indexing(&mut main);
        let index = main.completed_index_for("/project").unwrap();
        assert!(!index.config.contains_key("foo.package-enabled"));
        assert!(index.config.contains_key("foo.known"));
        assert!(index.config_incomplete.contains("/project/config/foo.php"));
        let (host, mut main) = setup(config);
        installed_sources(&host);
        host.add_file(
            "vendor/second/config/src/Key.php",
            "<?php namespace Vendor\\Pkg;enum Key:string{case Enabled='other';}",
        );
        host.add_file("vendor/composer/installed.json",r#"{"packages":[{"name":"acme/config","autoload":{"psr-4":{"Vendor\\Pkg\\":"src/"}}},{"name":"second/config","autoload":{"psr-4":{"Vendor\\Pkg\\":"src/"}}}]}"#);
        main.queue_index(&host.invocation().project().unwrap());
        crate::tests::finish_indexing(&mut main);
        let index = main.completed_index_for("/project").unwrap();
        assert!(!index.config.contains_key("foo.package-enabled"));
        assert!(!index.config.contains_key("foo.other"));
        assert!(index.config_incomplete.contains("/project/config/foo.php"));
    }
}
