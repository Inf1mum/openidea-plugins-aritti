//! Reviewed, atomic conventional class-component rename. PHP names are edited
//! only in proven class contexts; explicit import aliases retain local identity.
use crate::{Index, php, refs};
use openidea_plugin::{
    Error, Project, Range, TextEdit,
    file_actions::{FileAction, FileOperation},
    host,
    providers::Document,
    text,
};
use std::{collections::BTreeSet, ops::Range as Bytes};
const PREFIX: &str = "class-component-rename:";
const LIMIT: usize = 512 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Candidate {
    path: String,
    full: String,
    short: String,
    tag: String,
    declaration: usize,
    hash: String,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct Request {
    root: String,
    path: String,
    version: u64,
    hash: String,
    selection: Range,
    candidate: Candidate,
}
#[derive(Clone, Debug)]
struct Token {
    value: String,
    range: Bytes<usize>,
}
fn word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 128
}
fn name(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty() && !bytes[0].is_ascii_digit() && bytes.iter().all(|b| word(*b))
}
fn tokens(source: &str) -> Option<Vec<Token>> {
    let code = crate::package_facts::code(source)?;
    let bytes = code.as_bytes();
    let mut at = 0;
    let mut out = Vec::new();
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        let begin = at;
        if word(bytes[at]) || bytes[at] == b'\\' {
            at += 1;
            while at < bytes.len() && (word(bytes[at]) || bytes[at] == b'\\') {
                at += 1;
            }
        } else if bytes[at..].starts_with(b"::") {
            at += 2;
        } else {
            at += 1;
        }
        out.push(Token {
            value: code[begin..at].into(),
            range: begin..at,
        });
    }
    Some(out)
}
fn tag(relative: &str) -> String {
    relative
        .split('/')
        .map(|part| crate::index::separated(part, '-'))
        .collect::<Vec<_>>()
        .join(".")
}
fn app_mapping(project: &Project) -> bool {
    let Ok(source) = project.read_file("composer.json") else {
        return false;
    };
    let Ok(value) = crate::strict_json::parse(&source, 128 * 1024) else {
        return false;
    };
    value
        .pointer("/autoload/psr-4/App\\")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|path| path.trim_end_matches('/') == "app")
}
fn class_candidate(index: &Index, project: &Project, path: &str) -> Option<Candidate> {
    let root = project.root().ok()?;
    let relative = path
        .strip_prefix(&format!("{root}/app/View/Components/"))?
        .strip_suffix(".php")?;
    if index.partial
        || index.open_components
        || !app_mapping(project)
        || !crate::owns_current_path(project, path)
        || relative
            .split('/')
            .any(|part| !part.is_ascii() || !name(part))
    {
        return None;
    }
    let tag = tag(relative);
    let entry = index.components.get(&tag)?;
    if entry.path != path || entry.detail.is_some() {
        return None;
    }
    if index
        .components
        .iter()
        .any(|(other, entry)| entry.path == path && other != &tag)
    {
        return None;
    }
    let source = project.read_file(path).ok()?;
    let full = format!("App\\View\\Components\\{}", relative.replace('/', "\\"));
    if !crate::eloquent_members::declaration(&source)?.eq_ignore_ascii_case(&full) {
        return None;
    }
    let ts = tokens(&source)?;
    let mut declarations = ts
        .windows(2)
        .filter(|pair| pair[0].value.eq_ignore_ascii_case("class") && name(&pair[1].value));
    let declaration = declarations.next()?;
    if declarations.next().is_some() {
        return None;
    }
    let short = relative.rsplit('/').next()?.to_owned();
    if declaration[1].value != short {
        return None;
    }
    let class_index = ts
        .iter()
        .position(|token| token.range == declaration[0].range)?;
    let header = ts[class_index..]
        .iter()
        .take_while(|token| token.value != "{")
        .collect::<Vec<_>>();
    let parent = header
        .windows(2)
        .find(|pair| pair[0].value.eq_ignore_ascii_case("extends"))?[1];
    if !component_ancestry(
        index,
        project,
        crate::eloquent_members::source_class_name(&source, &parent.value)?,
    ) {
        return None;
    }
    let offset = text::offset(&source, entry.at)?;
    if offset != declaration[1].range.start {
        return None;
    }
    Some(Candidate {
        path: path.into(),
        full,
        short,
        tag,
        declaration: offset,
        hash: crate::artisan_metadata::hash(&source),
    })
}
fn tag_references(source: &str) -> Vec<refs::Reference> {
    let mut found = refs::tag_references(source);
    let mut closing = source.as_bytes().to_vec();
    for (at, _) in source.match_indices("</x-") {
        closing[at] = b'/';
        closing[at + 1] = b'<';
    }
    if let Ok(closing) = String::from_utf8(closing) {
        found.extend(refs::tag_references(&closing));
    }
    found.sort_by_key(|reference| (reference.start, reference.end));
    found.dedup_by_key(|reference| (reference.start, reference.end));
    found
}
fn candidate(
    index: &Index,
    project: &Project,
    doc: &Document,
    selection: Range,
) -> Option<Candidate> {
    let start = text::offset(&doc.text, selection.start)?;
    let end = text::offset(&doc.text, selection.end)?;
    if let Some(candidate) = class_candidate(index, project, &doc.path) {
        if candidate.declaration <= start && end <= candidate.declaration + candidate.short.len() {
            return Some(candidate);
        }
    }
    if crate::language_of(&doc.path, &doc.language) != Some("blade") {
        return None;
    }
    let reference = tag_references(&doc.text)
        .into_iter()
        .find(|reference| reference.start <= start && end <= reference.end)?;
    let alias = doc.text.get(reference.start..reference.end)?;
    class_candidate(index, project, &index.components.get(alias)?.path)
}
pub(crate) fn actions(index: &Index, doc: &Document, selection: Range) -> Vec<FileAction> {
    let run = || -> Option<FileAction> {
        let project = crate::project_for(&doc.path)?;
        let root = project.root().ok()?;
        let candidate = candidate(index, &project, doc, selection)?;
        let request = Request {
            root,
            path: doc.path.clone(),
            version: doc.version,
            hash: crate::artisan_metadata::hash(&doc.text),
            selection,
            candidate: candidate.clone(),
        };
        Some(FileAction {
            title: crate::ui_text::format(
                "classRename.action",
                "Rename component class {class} and proven references…",
                &[("class", &candidate.short)],
            ),
            kind: Some("refactor.rename".into()),
            data: Some(format!("{PREFIX}{}", serde_json::to_string(&request).ok()?)),
            ..Default::default()
        })
    };
    run().into_iter().collect()
}
#[derive(Clone, Debug)]
struct Import {
    full: String,
    alias: String,
    explicit: bool,
    range: Bytes<usize>,
}
/// Namespace class imports only: function/const imports and trait uses occupy
/// different contexts. Grouped imports preserve their original prefix and alias.
fn imports(ts: &[Token]) -> Option<Vec<Import>> {
    let mut out = Vec::new();
    let mut at = 0;
    let mut depth = 0usize;
    let mut namespaces = 0;
    while at < ts.len() {
        let token = &ts[at];
        if token.value == "{" {
            depth += 1;
            at += 1;
            continue;
        }
        if token.value == "}" {
            depth = depth.saturating_sub(1);
            at += 1;
            continue;
        }
        if depth == 0 && token.value.eq_ignore_ascii_case("namespace") {
            namespaces += 1;
            if namespaces > 1 {
                return None;
            }
            let end = ts[at..]
                .iter()
                .position(|token| matches!(token.value.as_str(), ";" | "{"))?
                + at;
            if ts[end].value == "{" {
                return None;
            }
            at = end + 1;
            continue;
        }
        if depth != 0 || !token.value.eq_ignore_ascii_case("use") {
            at += 1;
            continue;
        }
        let end = ts[at..].iter().position(|token| token.value == ";")? + at;
        let mut from = at + 1;
        if ts.get(from).is_some_and(|token| {
            ["function", "const"]
                .iter()
                .any(|keyword| token.value.eq_ignore_ascii_case(keyword))
        }) {
            at = end + 1;
            continue;
        }
        // `use (...)` is a closure capture, never an import.
        if ts.get(from).is_some_and(|token| token.value == "(") {
            at += 1;
            continue;
        }
        let mut prefix = String::new();
        let mut stop = end;
        if ts.get(from + 1).is_some_and(|token| token.value == "{") {
            prefix = ts[from].value.trim_start_matches('\\').into();
            if !prefix.ends_with('\\')
                || ts
                    .get(end.wrapping_sub(1))
                    .is_none_or(|token| token.value != "}")
            {
                return None;
            }
            from += 2;
            stop = end - 1;
        }
        while from < stop {
            if ts[from].value == "," {
                from += 1;
                continue;
            }
            if ["function", "const"]
                .iter()
                .any(|keyword| ts[from].value.eq_ignore_ascii_case(keyword))
            {
                while from < stop && ts[from].value != "," {
                    from += 1;
                }
                continue;
            }
            let target = &ts[from];
            let full = format!("{prefix}{}", target.value.trim_start_matches('\\'));
            if !full.split('\\').all(name) {
                return None;
            }
            from += 1;
            let explicit = ts
                .get(from)
                .is_some_and(|token| token.value.eq_ignore_ascii_case("as"));
            let alias = if explicit {
                from += 1;
                let alias = ts.get(from)?.value.clone();
                from += 1;
                if !name(&alias) {
                    return None;
                }
                alias
            } else {
                full.rsplit('\\').next()?.to_owned()
            };
            out.push(Import {
                full,
                alias,
                explicit,
                range: target.range.clone(),
            });
            if from < stop && ts[from].value != "," {
                return None;
            }
        }
        at = end + 1;
    }
    let mut seen = BTreeSet::new();
    if out
        .iter()
        .any(|item| !seen.insert(item.alias.to_ascii_lowercase()))
    {
        return None;
    }
    Some(out)
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Change {
    path: String,
    source: String,
    edits: Vec<(Bytes<usize>, String)>,
}
fn replacement(raw: &str, new: &str) -> String {
    raw.rsplit_once('\\')
        .map_or_else(|| new.to_owned(), |(prefix, _)| format!("{prefix}\\{new}"))
}
/// Class-name token slots in ordinary PHP types. Parentheses here are DNF
/// type groups or signature delimiters, not guesses based on adjacent colons.
fn type_slots(ts: &[Token]) -> BTreeSet<usize> {
    fn grammar(ts: &[Token], start: usize, end: usize) -> Option<Vec<usize>> {
        fn atom(ts: &[Token], at: &mut usize, end: usize, names: &mut Vec<usize>) -> Option<()> {
            if *at >= end {
                return None;
            }
            if ts[*at].value == "?" {
                *at += 1;
            }
            if *at >= end {
                return None;
            }
            if ts[*at].value == "(" {
                *at += 1;
                expr(ts, at, end, names)?;
                if *at >= end || ts[*at].value != ")" {
                    return None;
                }
                *at += 1;
            } else if ts[*at].value.trim_start_matches('\\').split('\\').all(name) {
                names.push(*at);
                *at += 1;
            } else {
                return None;
            }
            Some(())
        }
        fn expr(ts: &[Token], at: &mut usize, end: usize, names: &mut Vec<usize>) -> Option<()> {
            atom(ts, at, end, names)?;
            while *at < end && matches!(ts[*at].value.as_str(), "|" | "&") {
                *at += 1;
                atom(ts, at, end, names)?;
            }
            Some(())
        }
        if start == end {
            return None;
        }
        let mut at = start;
        let mut names = Vec::new();
        expr(ts, &mut at, end, &mut names)?;
        (at == end).then_some(names)
    }
    fn matching(ts: &[Token], open: usize) -> Option<usize> {
        let close = match ts[open].value.as_str() {
            "(" => ")",
            "[" => "]",
            "{" => "}",
            _ => return None,
        };
        let mut depth = 0;
        for i in open..ts.len() {
            if ts[i].value == ts[open].value {
                depth += 1;
            } else if ts[i].value == close {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
        None
    }
    fn prefix(ts: &[Token], mut start: usize, end: usize) -> usize {
        while start < end {
            if [
                "public",
                "protected",
                "private",
                "readonly",
                "static",
                "var",
            ]
            .iter()
            .any(|keyword| ts[start].value.eq_ignore_ascii_case(keyword))
            {
                start += 1;
            } else if ts[start].value == "#"
                && ts.get(start + 1).is_some_and(|token| token.value == "[")
            {
                if let Some(close) = matching(ts, start + 1) {
                    start = close + 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        start
    }
    let mut out = BTreeSet::new();
    for (function, token) in ts.iter().enumerate() {
        if !["function", "fn"]
            .iter()
            .any(|keyword| token.value.eq_ignore_ascii_case(keyword))
        {
            continue;
        }
        let mut open = function + 1;
        if ts.get(open).is_some_and(|token| token.value == "&") {
            open += 1;
        }
        if ts.get(open).is_some_and(|token| name(&token.value)) {
            open += 1;
        }
        if ts.get(open).is_none_or(|token| token.value != "(") {
            continue;
        }
        let Some(close) = matching(ts, open) else {
            continue;
        };
        let mut start = open + 1;
        let mut at = start;
        while at <= close {
            if at == close || ts[at].value == "," {
                if let Some(variable) = (start..at).find(|&i| ts[i].value == "$") {
                    let begin = prefix(ts, start, variable);
                    let mut end = variable;
                    while end > begin && ts[end - 1].value == "." {
                        end -= 1;
                    }
                    if end > begin && ts[end - 1].value == "&" {
                        end -= 1;
                    }
                    if let Some(names) = grammar(ts, begin, end) {
                        out.extend(names);
                    }
                }
                start = at + 1;
                at += 1;
                continue;
            }
            if matches!(ts[at].value.as_str(), "(" | "[" | "{") {
                if let Some(end) = matching(ts, at) {
                    at = end + 1;
                    continue;
                }
            }
            at += 1;
        }
        let mut returns = close + 1;
        if ts
            .get(returns)
            .is_some_and(|token| token.value.eq_ignore_ascii_case("use"))
            && ts.get(returns + 1).is_some_and(|token| token.value == "(")
        {
            if let Some(end) = matching(ts, returns + 1) {
                returns = end + 1;
            }
        }
        if ts.get(returns).is_some_and(|token| token.value == ":") {
            let begin = returns + 1;
            let end = (begin..ts.len())
                .find(|&i| matches!(ts[i].value.as_str(), "{" | ";" | "="))
                .unwrap_or(ts.len());
            if let Some(names) = grammar(ts, begin, end) {
                out.extend(names);
            }
        }
    }
    // Typed properties begin at a member boundary with an explicit modifier.
    for variable in 0..ts.len() {
        if ts[variable].value != "$" {
            continue;
        }
        let start = (0..variable)
            .rev()
            .find(|&i| matches!(ts[i].value.as_str(), "{" | "}" | ";"))
            .map_or(0, |i| i + 1);
        let begin = prefix(ts, start, variable);
        if begin > start
            && let Some(names) = grammar(ts, begin, variable)
        {
            out.extend(names);
        }
    }
    out
}

fn parent_class(source: &str) -> Option<String> {
    let ts = tokens(source)?;
    let class = ts
        .windows(2)
        .position(|pair| pair[0].value.eq_ignore_ascii_case("class") && name(&pair[1].value))?;
    let header = ts[class..]
        .iter()
        .take_while(|token| token.value != "{")
        .collect::<Vec<_>>();
    let parent = header
        .windows(2)
        .find(|pair| pair[0].value.eq_ignore_ascii_case("extends"))?[1];
    crate::eloquent_members::source_class_name(source, &parent.value)
}
fn component_ancestry(index: &Index, project: &Project, mut parent: String) -> bool {
    let inventory = crate::eloquent_members::class_inventory(&index.eloquent_models);
    let mut seen = BTreeSet::new();
    for _ in 0..8 {
        if parent.eq_ignore_ascii_case("Illuminate\\View\\Component") {
            return true;
        }
        if !seen.insert(parent.to_ascii_lowercase()) {
            return false;
        }
        let mut matches = inventory
            .iter()
            .filter(|class| class.full.eq_ignore_ascii_case(&parent));
        let Some(class) = matches.next() else {
            return false;
        };
        if matches.next().is_some()
            || class.ambiguous
            || class.is_trait
            || !crate::owns_current_path(project, &class.path)
        {
            return false;
        }
        let Ok(source) = project.read_file(&class.path) else {
            return false;
        };
        if source.len() > LIMIT
            || !crate::eloquent_members::declaration(&source)
                .is_some_and(|full| full.eq_ignore_ascii_case(&class.full))
        {
            return false;
        }
        let Some(actual) = parent_class(&source) else {
            return false;
        };
        if !actual.eq_ignore_ascii_case(&class.parent) {
            return false;
        }
        parent = actual;
    }
    parent.eq_ignore_ascii_case("Illuminate\\View\\Component")
}

fn php_edits(
    source: &str,
    path: &str,
    old: &Candidate,
    new: &str,
) -> Result<Vec<(Bytes<usize>, String)>, Error> {
    if !source
        .to_ascii_lowercase()
        .contains(&old.short.to_ascii_lowercase())
    {
        return Ok(Vec::new());
    }
    // Exact class spellings in PHPDoc/runtime strings are unresolved references,
    // not permission for broad textual replacement. Refuse the entire plan.
    let comments = php::mask_comments(source);
    let mut at = 0;
    while at < comments.len() {
        if matches!(comments.as_bytes()[at], b'\'' | b'"') {
            let end = php::string_end(comments.as_bytes(), at);
            if end > comments.len() || end <= at + 1 {
                return Err(Error::Stale);
            }
            let raw = &source[at + 1..end - 1];
            if let Some(value) = crate::eloquent_members::metadata_inference::decode_literal(
                raw,
                comments.as_bytes()[at],
            ) {
                let class = value
                    .split('@')
                    .next()
                    .unwrap_or(&value)
                    .trim_start_matches('\\');
                if class.eq_ignore_ascii_case(&old.full)
                    || value
                        .to_ascii_lowercase()
                        .contains(&old.full.to_ascii_lowercase())
                    || crate::eloquent_members::source_class_name(source, class)
                        .is_some_and(|full| full.eq_ignore_ascii_case(&old.full))
                {
                    return Err(Error::Invalid(crate::ui_text::format(
                        "classRename.runtimeString",
                        "Unresolved runtime class string in {path}; convert it to a proven class constant before renaming.",
                        &[("path", path)],
                    )));
                }
            } else if raw
                .to_ascii_lowercase()
                .contains(&old.full.to_ascii_lowercase())
                || raw
                    .to_ascii_lowercase()
                    .contains(&old.full.replace('\\', "\\\\").to_ascii_lowercase())
            {
                return Err(Error::Invalid(crate::ui_text::format(
                    "classRename.escapedString",
                    "Unresolved escaped/interpolated class spelling in {path}.",
                    &[("path", path)],
                )));
            }
            at = end;
        } else {
            at += source[at..].chars().next().ok_or(Error::Stale)?.len_utf8();
        }
    }
    // Only PHPDoc names resolving to this class obstruct the transaction; foreign
    // same-short-name documentation is independent of this component identity.
    let mut at = 0;
    while let Some(offset) = source[at..].find("/**") {
        let begin = at + offset;
        let Some(end) = source[begin + 3..]
            .find("*/")
            .map(|offset| begin + 3 + offset)
        else {
            return Err(Error::Stale);
        };
        let doc = &source[begin + 3..end];
        for raw in doc.split(|ch: char| !(ch.is_alphanumeric() || matches!(ch, '_' | '\\'))) {
            if !raw.is_empty()
                && crate::eloquent_members::source_class_name(source, raw)
                    .is_some_and(|full| full.eq_ignore_ascii_case(&old.full))
            {
                return Err(Error::Invalid(crate::ui_text::format(
                    "classRename.phpdoc",
                    "Unresolved PHPDoc class reference in {path}; no partial class rename is offered.",
                    &[("path", path)],
                )));
            }
        }
        at = end + 2;
    }
    let ts = tokens(source).ok_or_else(|| {
        Error::Invalid(crate::ui_text::format(
            "classRename.syntax",
            "Unsupported PHP syntax in {path}; no partial class rename is offered.",
            &[("path", path)],
        ))
    })?;
    let type_slots = type_slots(&ts);
    let imports = imports(&ts).ok_or_else(|| {
        Error::Invalid(crate::ui_text::format(
            "classRename.imports",
            "Ambiguous PHP imports/namespaces in {path}.",
            &[("path", path)],
        ))
    })?;
    let relevant = imports
        .iter()
        .filter(|item| item.full.eq_ignore_ascii_case(&old.full))
        .collect::<Vec<_>>();
    let mut edits = relevant
        .iter()
        .map(|item| {
            (
                item.range.clone(),
                replacement(&source[item.range.clone()], new),
            )
        })
        .collect::<Vec<_>>();
    let preserve = relevant
        .iter()
        .filter(|item| item.explicit)
        .map(|item| item.alias.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let changes_short = path == old.path || relevant.iter().any(|item| !item.explicit);
    if changes_short
        && (php::declares_class(source, new)
            || imports.iter().any(|item| {
                item.alias.eq_ignore_ascii_case(new) && !item.full.eq_ignore_ascii_case(&old.full)
            }))
    {
        return Err(Error::Invalid(crate::ui_text::format(
            "classRename.aliasCollision",
            "PHP class/import alias {new} collides in {path}.",
            &[("path", path), ("new", new)],
        )));
    }
    let import_ranges = imports
        .iter()
        .map(|item| item.range.clone())
        .collect::<Vec<_>>();
    for (i, token) in ts.iter().enumerate() {
        if import_ranges.iter().any(|range| range == &token.range) {
            continue;
        }
        if path == old.path && token.range.start == old.declaration {
            edits.push((token.range.clone(), new.into()));
            continue;
        }
        if !token.value.trim_start_matches('\\').split('\\').all(name) {
            continue;
        }
        let before = i.checked_sub(1).map(|i| ts[i].value.as_str()).unwrap_or("");
        let after = ts
            .get(i + 1)
            .map(|token| token.value.as_str())
            .unwrap_or("");
        let Some(full) = crate::eloquent_members::source_class_name(source, &token.value) else {
            continue;
        };
        if !full.eq_ignore_ascii_case(&old.full) {
            continue;
        }
        if !token.value.contains('\\') && preserve.contains(&token.value.to_ascii_lowercase()) {
            continue;
        }
        // Function/method declarations and calls named like a class live in a
        // different symbol namespace and do not inherit class import identity.
        if before.eq_ignore_ascii_case("function")
            || before.eq_ignore_ascii_case("const")
            || before == "$"
            || before == "::"
            || before == ">"
            || after == "(" && !before.eq_ignore_ascii_case("new")
        {
            continue;
        }
        let class_context = type_slots.contains(&i)
            || after == "::"
            || ["new", "instanceof", "extends", "implements"]
                .iter()
                .any(|keyword| before.eq_ignore_ascii_case(keyword))
            || after == "$";
        if !class_context {
            return Err(Error::Invalid(crate::ui_text::format(
                "classRename.context",
                "Unresolved class-name context {name} in {path}; no partial rename is offered.",
                &[("name", &token.value), ("path", path)],
            )));
        }
        edits.push((token.range.clone(), replacement(&token.value, new)));
    }
    edits.sort_by_key(|(range, _)| (range.start, range.end));
    edits.dedup_by(|a, b| a.0 == b.0);
    if edits.windows(2).any(|pair| pair[0].0.end > pair[1].0.start) {
        return Err(Error::Stale);
    }
    Ok(edits)
}
fn new_target(old: &Candidate, new: &str) -> Result<(String, String), Error> {
    if !new.is_ascii()
        || !name(new)
        || new.len() > 128
        || [
            "class",
            "trait",
            "interface",
            "enum",
            "self",
            "static",
            "parent",
            "match",
            "function",
            "namespace",
            "abstract",
            "final",
            "readonly",
            "new",
            "true",
            "false",
            "null",
        ]
        .iter()
        .any(|keyword| new.eq_ignore_ascii_case(keyword))
    {
        return Err(Error::Invalid(crate::ui_text::t(
            "classRename.identifier",
            "Use a PHP class identifier; namespaces, paths and reserved words are not accepted.",
        )));
    }
    if new.eq_ignore_ascii_case(&old.short) {
        return Err(Error::Invalid(crate::ui_text::t(
            "classRename.distinct",
            "Choose a distinct class name; case-only renames are not supported.",
        )));
    }
    let directory = old.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let target = format!("{directory}/{new}.php");
    let prefix = old
        .tag
        .rsplit_once('.')
        .map(|(prefix, _)| format!("{prefix}."))
        .unwrap_or_default();
    Ok((
        target,
        format!("{prefix}{}", crate::index::separated(new, '-')),
    ))
}
fn plan(
    index: &Index,
    project: &Project,
    doc: &Document,
    old: &Candidate,
    new: &str,
) -> Result<(String, String, Vec<Change>), Error> {
    let (target, new_tag) = new_target(old, new)?;
    let root = project.root()?;
    if !crate::owns_current_path(project, &old.path)
        || !crate::owns_current_path(project, &target)
        || project.exists(&target)?
        || index.components.contains_key(&new_tag)
    {
        return Err(Error::Invalid(crate::ui_text::t(
            "classRename.target",
            "The target component class, tag or file is unavailable.",
        )));
    }
    let found = project.find_files("**/*.php", 513)?;
    if found.len() >= 513 {
        return Err(Error::Invalid(crate::ui_text::t(
            "classRename.sourceCount",
            "Component rename exceeds its 512-source bound.",
        )));
    }
    let mut paths = found
        .into_iter()
        .filter(|path| {
            let relative = path
                .strip_prefix(&root)
                .and_then(|path| path.strip_prefix('/'))
                .unwrap_or(path);
            ![
                "vendor/",
                "storage/",
                "node_modules/",
                "bootstrap/cache/",
                ".git/",
            ]
            .iter()
            .any(|prefix| relative.starts_with(prefix))
        })
        .collect::<BTreeSet<_>>();
    paths.insert(old.path.clone());
    paths.insert(doc.path.clone());
    if paths.iter().any(|path| path.eq_ignore_ascii_case(&target)) {
        return Err(Error::Invalid(crate::ui_text::t(
            "classRename.fileCollision",
            "A case-insensitive target file collision exists.",
        )));
    }
    let mut changes = Vec::new();
    let mut total = 0usize;
    for path in paths {
        if !crate::owns_current_path(project, &path) {
            continue;
        }
        let source = if path == doc.path {
            doc.text.clone()
        } else {
            project.read_file(&path)?
        };
        total = total.checked_add(source.len()).ok_or(Error::Stale)?;
        if source.len() > LIMIT || total > 4 * 1024 * 1024 {
            return Err(Error::Invalid(crate::ui_text::t(
                "classRename.sourceBound",
                "Component rename exceeds its source review bound.",
            )));
        }
        if !path.ends_with(".blade.php") && path != old.path && php::declares_class(&source, new) {
            let full = format!(
                "{}\\{new}",
                old.full.rsplit_once('\\').ok_or(Error::Stale)?.0
            );
            if crate::eloquent_members::source_class_name(&source, new)
                .is_some_and(|declared| declared.eq_ignore_ascii_case(&full))
            {
                return Err(Error::Invalid(crate::ui_text::format(
                    "classRename.declaredCollision",
                    "The target class identity is already declared in {path}.",
                    &[("path", &path)],
                )));
            }
        }
        let edits = if path.ends_with(".blade.php") {
            tag_references(&source)
                .into_iter()
                .filter(|reference| source.get(reference.start..reference.end) == Some(&old.tag))
                .map(|reference| (reference.start..reference.end, new_tag.clone()))
                .collect()
        } else {
            php_edits(&source, &path, old, new)?
        };
        if !edits.is_empty() {
            changes.push(Change {
                path,
                source,
                edits,
            });
        }
    }
    if !changes.iter().any(|change| change.path == old.path) {
        return Err(Error::Stale);
    }
    Ok((target, new_tag, changes))
}
pub(crate) fn resolve(
    index: &Index,
    doc: &Document,
    action: FileAction,
) -> Result<FileAction, Error> {
    let data = action
        .data
        .as_deref()
        .and_then(|data| data.strip_prefix(PREFIX))
        .filter(|data| data.len() < 8192)
        .ok_or(Error::Stale)?;
    let request: Request = serde_json::from_str(data).map_err(|_| Error::Stale)?;
    if request.path != doc.path
        || request.version != doc.version
        || request.hash != crate::artisan_metadata::hash(&doc.text)
    {
        return Err(Error::Stale);
    }
    let project = crate::project_for(&doc.path).ok_or(Error::Stale)?;
    if project.root()? != request.root || project.read_file(&doc.path)? != doc.text {
        return Err(Error::Stale);
    }
    if !crate::index::index_sources_current(&project,index){return Err(Error::Stale);}
    let current = index;
    if candidate(current, &project, doc, request.selection).as_ref() != Some(&request.candidate) {
        return Err(Error::Stale);
    }
    let new = host::input(
        &crate::ui_text::t(
            "classRename.prompt",
            "New component class name (namespace and directory retained)",
        ),
        Some(&request.candidate.short),
    )?
    .ok_or(Error::Cancelled)?;
    if !crate::index::index_sources_current(&project,index){return Err(Error::Stale);}
    let (target, new_tag, before) = plan(current, &project, doc, &request.candidate, &new)?;
    let mut preview = crate::ui_text::format(
        "classRename.review",
        "Project: {root}\nRename class {class} → {new}\nMove {from} → {to}\nBlade tag {tag} → {newTag}\n\nProven declarations, import targets, class constants/new/nullable, union, intersection and return/property/parameter types and Blade tags are updated atomically. Explicit import aliases retain their local spelling. Unresolved class strings/PHPDoc prevent this plan; unrelated text and computed component names are unchanged.\n\n",
        &[
            ("root", &request.root),
            ("class", &request.candidate.full),
            ("new", &new),
            ("from", &request.candidate.path),
            ("to", &target),
            ("tag", &request.candidate.tag),
            ("newTag", &new_tag),
        ],
    );
    let mut edits = Vec::new();
    for change in &before {
        let mut after = change.source.clone();
        for (range, replacement) in change.edits.iter().rev() {
            after.replace_range(range.clone(), replacement);
        }
        preview.push_str(&change.path);
        preview.push('\n');
        preview.push_str(&crate::ui_text::t(
            "classRename.resultingContents",
            "Complete resulting contents:",
        ));
        preview.push('\n');
        preview.push_str(&ui_kit::prompts::readable_source(&after));
        preview.push_str("\n\n");
        edits.push((
            if change.path == request.candidate.path {
                target.clone()
            } else {
                change.path.clone()
            },
            change
                .edits
                .iter()
                .map(|(range, replacement)| TextEdit {
                    range: Range::new(
                        text::position(&change.source, range.start),
                        text::position(&change.source, range.end),
                    ),
                    text: replacement.clone(),
                })
                .collect(),
        ));
    }
    if !ui_kit::prompts::confirm_exact(
        &crate::ui_text::t(
            "classRename.confirm",
            "Rename component class and proven references?",
        ),
        &preview,
    )? {
        return Err(Error::Cancelled);
    }
    let latest = index;
    if !crate::index::index_sources_current(&project,index)
        || project.read_file(&doc.path)? != doc.text
        || candidate(latest, &project, doc, request.selection).as_ref() != Some(&request.candidate)
        || plan(latest, &project, doc, &request.candidate, &new)?
            != (target.clone(), new_tag, before)
    {
        return Err(Error::Stale);
    }
    Ok(FileAction {
        title: crate::ui_text::format(
            "classRename.resultTitle",
            "Rename component class {class} → {new}",
            &[("class", &request.candidate.short), ("new", &new)],
        ),
        operations: vec![FileOperation::Rename {
            from: request.candidate.path,
            to: target.clone(),
        }],
        edits,
        open: Some(target),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn setup() -> (MockHost, Project, Index, Document) {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("artisan", "<?php");
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        host.add_dir("app/View/Components");
        host.add_file("app/View/Components/Card.php","<?php\nnamespace App\\View\\Components;\nuse Illuminate\\View\\Component;\nclass Card extends Component\n{\n    public function render(){return view('components.card');}\n}\n");
        host.add_file(
            "resources/views/page.blade.php",
            "😀 <x-card>Hi</x-card> <x-card /> {{-- <x-card/> --}} @php $x='<x-card />'; @endphp",
        );
        let project = host.invocation().project().unwrap();
        let index = Index::build(&project);
        let doc = Document {
            path: "/project/app/View/Components/Card.php".into(),
            language: "php".into(),
            version: 1,
            text: project.read_file("app/View/Components/Card.php").unwrap(),
        };
        (host, project, index, doc)
    }
    fn caret(doc: &Document, needle: &str) -> Range {
        let position = text::position(&doc.text, doc.text.find(needle).unwrap() + 1);
        Range::new(position, position)
    }
    fn rendered(change: &Change) -> String {
        let mut result = change.source.clone();
        for (range, replacement) in change.edits.iter().rev() {
            result.replace_range(range.clone(), replacement);
        }
        result
    }
    fn run(host: &MockHost, index: &Index, doc: &Document, new: &str) -> Result<FileAction, Error> {
        let action = actions(index, doc, caret(doc, "Card extends")).remove(0);
        host.answer_next_prompt(Some(&[new]));
        host.answer_next_prompt(Some(&["yes"]));
        resolve(index, doc, action)
    }
    #[test]
    fn atomic_class_import_reference_and_blade_tag_rename_preserves_alias_identity() {
        let (host, project, _, doc) = setup();
        host.add_file("app/UseCard.php",r#"<?php
namespace App;
use App\View\Components\Card;
use App\View\Components\{Card as DisplayCard};
class UseCard {
    public function make(Card $card) {return [new Card(), Card::class, DisplayCard::class, new \App\View\Components\Card()];}
}
"#);
        host.add_file("app/Foreign.php",r#"<?php namespace Vendor; use Other\Card; class Other {function make(){return new Card;}}"#);
        let index = Index::build(&project);
        let old = candidate(&index, &project, &doc, caret(&doc, "Card extends")).unwrap();
        let (target, new_tag, changes) = plan(&index, &project, &doc, &old, "Notice").unwrap();
        assert_eq!(target, "/project/app/View/Components/Notice.php");
        assert_eq!(new_tag, "notice");
        let usage = rendered(
            changes
                .iter()
                .find(|change| change.path.ends_with("/UseCard.php"))
                .unwrap(),
        );
        assert!(usage.contains("use App\\View\\Components\\Notice;"));
        assert!(usage.contains("{Notice as DisplayCard}"));
        assert!(usage.contains("make(Notice $card)"));
        assert!(usage.contains("new Notice()"));
        assert!(usage.contains("Notice::class, DisplayCard::class"));
        assert!(usage.contains("new \\App\\View\\Components\\Notice()"));
        assert!(
            changes
                .iter()
                .all(|change| !change.path.ends_with("/Foreign.php"))
        );
        let page = changes
            .iter()
            .find(|change| change.path.ends_with("/page.blade.php"))
            .unwrap();
        assert_eq!(page.edits.len(), 3);
        let page = rendered(page);
        assert!(page.contains("<x-notice>Hi</x-notice> <x-notice />"));
        assert!(page.contains("{{-- <x-card/> --}}"));
        assert!(page.contains("$x='<x-card />'"));
        let action = run(&host, &index, &doc, "Notice").unwrap();
        assert_eq!(action.operations.len(), 1);
        assert!(
            matches!(&action.operations[0],FileOperation::Rename{from,to}if from==&old.path&&to==&target)
        );
        assert!(
            action
                .edits
                .iter()
                .any(|(path, edits)| path == &target
                    && edits.iter().any(|edit| edit.text == "Notice"))
        );
        assert!(project.exists(&old.path).unwrap());
        assert!(!project.exists(&target).unwrap());
    }
    #[test]
    fn nullable_union_intersection_dnf_property_parameter_and_return_slots_are_renamed() {
        let (host, project, index, doc) = setup();
        host.add_file("app/TypedUsage.php",r#"<?php
namespace App;
use App\View\Components\Card;
use App\View\Components\Card as Display;
class TypedUsage {
    public ?Card $nullable;
    protected Card|Other $either;
    private (Card&Marker)|null $both;
    public function make(?Card $one, Card|Other $two, Card&Marker $three, Card &...$many): Card|Other {return $one;}
    public function closure(Card $card): callable {return function() use($card): Card {return $card;};}
    public function alias(Display $card): ?Display {return $card;}
}
"#);
        host.add_file("app/ForeignTypes.php",r#"<?php namespace Other; use Vendor\Card; class ForeignTypes {public Card|Other $value; public function make(?Card $card): Card {return $card;}}"#);
        let old = candidate(&index, &project, &doc, caret(&doc, "Card extends")).unwrap();
        let (_, _, changes) =
            plan(&Index::build(&project), &project, &doc, &old, "Notice").unwrap();
        let updated = rendered(
            changes
                .iter()
                .find(|change| change.path.ends_with("/TypedUsage.php"))
                .unwrap(),
        );
        for expected in [
            "public ?Notice $nullable",
            "protected Notice|Other $either",
            "private (Notice&Marker)|null $both",
            "make(?Notice $one, Notice|Other $two, Notice&Marker $three, Notice &...$many): Notice|Other",
            "use($card): Notice",
            "alias(Display $card): ?Display",
        ] {
            assert!(updated.contains(expected), "{expected}\n{updated}");
        }
        assert!(
            changes
                .iter()
                .all(|change| !change.path.ends_with("/ForeignTypes.php"))
        );
    }
    #[test]
    fn custom_component_ancestry_is_cached_owned_fresh_and_cycle_bounded() {
        let (host, project, _, doc) = setup();
        host.add_file("app/View/Components/BaseCard.php","<?php namespace App\\View\\Components; use Illuminate\\View\\Component; abstract class BaseCard extends Component {}");
        host.add_file(
            "app/View/Components/Card.php",
            &doc.text.replace("extends Component", "extends BaseCard"),
        );
        let index = Index::build(&project);
        let current = Document {
            text: project.read_file(&doc.path).unwrap(),
            ..doc.clone()
        };
        assert_eq!(
            actions(&index, &current, caret(&current, "Card extends")).len(),
            1
        );
        host.add_file(
            "app/View/Components/BaseCard.php",
            "<?php namespace App\\View\\Components; abstract class BaseCard extends \\stdClass {}",
        );
        assert!(actions(&index, &current, caret(&current, "Card extends")).is_empty());
        host.add_file(
            "app/View/Components/BaseCard.php",
            "<?php namespace App\\View\\Components; abstract class BaseCard extends OtherBase {}",
        );
        host.add_file(
            "app/View/Components/OtherBase.php",
            "<?php namespace App\\View\\Components; abstract class OtherBase extends BaseCard {}",
        );
        let cyclic = Index::build(&project);
        assert!(actions(&cyclic, &current, caret(&current, "Card extends")).is_empty());
        host.add_file("app/View/Components/BaseCard.php","<?php namespace App\\View\\Components; use Illuminate\\View\\Component; abstract class BaseCard extends Component {}");
        let owned = Index::build(&project);
        host.add_project("/project/app/View/Components/BaseCard.php");
        assert!(actions(&owned, &current, caret(&current, "Card extends")).is_empty());
    }

    #[test]
    fn unresolved_runtime_strings_phpdoc_and_case_collisions_refuse_whole_plan() {
        let (host, project, index, doc) = setup();
        let old = candidate(&index, &project, &doc, caret(&doc, "Card extends")).unwrap();
        for tail in [
            r#"$class='App\View\Components\Card'; new $class();"#,
            r#"$class="App\\View\\Components\\Card"; new $class();"#,
            "/** @return Card */ function make() { return null; }",
        ] {
            host.add_file(
                "app/Usage.php",
                &format!("<?php namespace App; use App\\View\\Components\\Card; {tail}"),
            );
            let latest = Index::build(&project);
            assert!(
                plan(&latest, &project, &doc, &old, "Notice").is_err(),
                "{tail}"
            );
        }
        host.add_file(
            "app/Usage.php",
            "<?php use App\\View\\Components\\Card; use Vendor\\Thing as notice; new Card;",
        );
        let latest = Index::build(&project);
        assert!(plan(&latest, &project, &doc, &old, "Notice").is_err());
        for value in [
            "card",
            "CARD",
            "../Notice",
            "Other\\Notice",
            "new",
            "Notice'",
            "bad-name",
        ] {
            assert!(new_target(&old, value).is_err(), "{value}");
        }
    }
    #[test]
    fn source_identity_ownership_collision_and_stale_data_are_rechecked() {
        let (host, project, index, doc) = setup();
        let action = actions(&index, &doc, caret(&doc, "Card extends")).remove(0);
        host.add_file(
            "app/View/Components/NOTICE.php",
            "<?php namespace App\\View\\Components; class NOTICE {}",
        );
        host.answer_next_prompt(Some(&["Notice"]));
        assert!(resolve(&index, &doc, action).is_err());
        let action = actions(&index, &doc, caret(&doc, "Card extends")).remove(0);
        let mut changed = doc.clone();
        changed.version += 1;
        assert!(matches!(
            resolve(&index, &changed, action),
            Err(Error::Stale)
        ));
        host.add_project("/project/app/View/Components");
        assert!(actions(&index, &doc, caret(&doc, "Card extends")).is_empty());
        assert!(project.exists("app/View/Components/Card.php").unwrap());
    }
    #[test]
    fn genuine_base_and_psr4_mapping_are_required_and_blade_selection_is_utf16_safe() {
        let (host, project, index, doc) = setup();
        let page = Document {
            path: "/project/resources/views/page.blade.php".into(),
            language: "blade".into(),
            version: 1,
            text: project.read_file("resources/views/page.blade.php").unwrap(),
        };
        assert_eq!(actions(&index, &page, caret(&page, "card>")).len(), 1);
        host.add_file(
            "app/View/Components/Card.php",
            &doc.text
                .replace("use Illuminate\\View\\Component;", "use Vendor\\Component;"),
        );
        let latest = Index::build(&project);
        assert!(actions(&latest, &page, caret(&page, "card>")).is_empty());
        host.add_file("app/View/Components/Card.php", &doc.text);
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"App\\":"src/"}}}"#,
        );
        let latest = Index::build(&project);
        assert!(actions(&latest, &page, caret(&page, "card>")).is_empty());
    }
    #[test]
    fn completed_snapshot_rejects_new_component_membership_without_emitting_operations(){
        let(host,project,index,doc)=setup();let action=actions(&index,&doc,caret(&doc,"Card extends")).remove(0);
        assert!(crate::index::index_sources_current(&project,&index));host.add_file("app/View/Components/Added.php","<?php namespace App\\View\\Components; class Added {}");
        host.answer_next_prompt(Some(&["Notice"]));host.answer_next_prompt(Some(&["yes"]));
        assert!(matches!(resolve(&index,&doc,action),Err(Error::Stale)));assert!(project.exists("app/View/Components/Card.php").unwrap());
        assert!(!project.exists("app/View/Components/Notice.php").unwrap());
    }
    #[test]
    fn cancellation_returns_no_file_operations() {
        let (host, _, index, doc) = setup();
        let action = actions(&index, &doc, caret(&doc, "Card extends")).remove(0);
        host.answer_next_prompt(None);
        assert!(matches!(
            resolve(&index, &doc, action),
            Err(Error::Cancelled)
        ));
    }
    #[test]
    #[ignore = "requires php executable; lints every actual transformed PHP source"]
    fn actual_component_class_and_import_rewrite_outputs_lint_with_php() {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let (host, project, index, doc) = setup();
        host.add_file("app/Usage.php","<?php namespace App; use App\\View\\Components\\{Card,Card as Item}; class Usage { public ?Card $value; function make(Card|Other $card): Card|Other {return new Card();} function other(Card&Marker $card): ?Card {return null;} }");
        let old = candidate(&index, &project, &doc, caret(&doc, "Card extends")).unwrap();
        let (_, _, changes) = plan(&index, &project, &doc, &old, "Notice").unwrap();
        for change in changes
            .into_iter()
            .filter(|change| !change.path.ends_with(".blade.php"))
        {
            let source = rendered(&change);
            let mut child = Command::new("php")
                .arg("-l")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(source.as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{source}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
