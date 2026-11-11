//! Generators that fill existing files: a lang key for a selected string, and
//! validation rules or `$fillable` entries for the columns of a model.

use std::ops::Range;

use openidea_plugin::providers::{CodeAction, Document};
use openidea_plugin::{Project, TextEdit, text};

use crate::actions::Rewrite;
use crate::index::Index;
use crate::php;
use crate::refs;

const LEFT_OUT_COLUMNS: &[&str] = &[
    "id",
    "created_at",
    "updated_at",
    "deleted_at",
    "remember_token",
    "email_verified_at",
];

fn eol_of(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

fn slug(value: &str) -> String {
    let mut key = String::new();
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            key.push(character.to_ascii_lowercase());
        } else if !key.ends_with('_') && !key.is_empty() {
            key.push('_');
        }
    }

    key.trim_end_matches('_')
        .chars()
        .take(40)
        .collect::<String>()
        .trim_end_matches('_')
        .to_owned()
}

/// The edit adding `'key' => 'value',` before the closing `];` of a lang file.
pub fn entry_edit(file_text: &str, key: &str, value: &str) -> Option<TextEdit> {
    let closing = file_text.rfind("];").or_else(|| file_text.rfind(");"))?;
    let line_start = file_text[..closing].rfind('\n').map_or(0, |i| i + 1);
    let at = text::position(file_text, line_start);
    let quote = |s: &str| format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"));

    Some(TextEdit {
        range: openidea_plugin::Range::new(at, at),
        text: format!(
            "    {} => {},{}",
            quote(key),
            quote(value),
            eol_of(file_text)
        ),
    })
}

fn selected_range(doc: &Document, selection: openidea_plugin::Range) -> Option<Range<usize>> {
    let start = text::offset(&doc.text, selection.start)?;
    let end = text::offset(&doc.text, selection.end)?;
    (start < end && !doc.text[start..end].contains('\n')).then_some(start..end)
}

fn inside_tag(text: &str, at: usize) -> bool {
    text[..at].rfind('<') > text[..at].rfind('>')
}

fn plain_words(selected: &str) -> bool {
    let trimmed = selected.trim();
    trimmed.len() >= 2
        && trimmed.len() <= 100
        && trimmed == selected
        && trimmed.chars().any(char::is_alphabetic)
        && !trimmed.contains(['{', '}', '<', '>', '@', '$', '"', '`', '\\', '%'])
}

/// Replaces the selected text with `__('file.key')` and adds the entry to the main locale's lang file.
pub fn extract_string(
    index: &Index,
    project: &Project,
    doc: &Document,
    language: &str,
    selection: openidea_plugin::Range,
) -> Option<CodeAction> {
    let range = selected_range(doc, selection)?;
    let selected = &doc.text[range.clone()];
    if !plain_words(selected) {
        return None;
    }

    let file_key = if index.translation_files.contains_key("messages") {
        "messages".to_owned()
    } else {
        index
            .translation_files
            .keys()
            .find(|key| !key.contains(['/', ':']))?
            .clone()
    };
    let lang_file = index.translation_files.get(&file_key)?.first()?.clone();

    let replace = if language == "blade" {
        let regions = refs::blade_regions(&doc.text);
        if selected.contains('&')
            || inside_tag(&doc.text, range.start)
            || regions
                .iter()
                .any(|region| region.start < range.end && range.start < region.end)
        {
            return None;
        }
        range.clone()
    } else {
        let before = doc.text[..range.start].chars().next_back()?;
        let after = doc.text[range.end..].chars().next()?;
        if before != '\'' || after != '\'' {
            return None;
        }
        range.start - 1..range.end + 1
    };

    let base = slug(selected);
    if base.is_empty() {
        return None;
    }

    let mut key = base.clone();
    let mut counter = 2;
    while index
        .translations
        .contains_key(&format!("{file_key}.{key}"))
    {
        key = format!("{base}_{counter}");
        counter += 1;
    }

    let call = format!("__('{file_key}.{key}')");
    let new_text = if language == "blade" {
        format!("{{{{ {call} }}}}")
    } else {
        call
    };
    let lang_text = project.read_file(&lang_file).ok()?;
    let entry = entry_edit(&lang_text, &key, selected)?;

    Some(CodeAction {
        title: format!("Extract to translation key \"{file_key}.{key}\""),
        kind: Some("refactor.extract".into()),
        preferred: false,
        edits: vec![
            (
                doc.path.clone(),
                vec![TextEdit {
                    range: crate::range_of(&doc.text, replace.start, replace.end),
                    text: new_text,
                }],
            ),
            (lang_file, vec![entry]),
        ],
        data: None,
    })
}

fn rule_for(name: &str, kind: &str) -> String {
    let rules: &[&str] = match kind {
        "string" | "char" if name.contains("email") => &["required", "email", "max:255"],
        "string" | "char" => &["required", "string", "max:255"],
        "text" | "tinyText" | "mediumText" | "longText" => &["required", "string"],
        "boolean" => &["required", "boolean"],
        "date" | "dateTime" | "dateTimeTz" | "timestamp" | "timestampTz" => &["required", "date"],
        "float" | "double" | "decimal" | "unsignedDecimal" => &["required", "numeric"],
        "json" | "jsonb" => &["required", "array"],
        "uuid" | "ulid" => &["required", "uuid"],
        kind if kind.contains("nteger") || kind == "foreignId" => &["required", "integer"],
        _ => &["required"],
    };

    format!(
        "[{}]",
        rules
            .iter()
            .map(|rule| format!("'{rule}'"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn indentation_of_line(text: &str, at: usize) -> &str {
    let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let line = &text[line_start..];
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// The edit adding `lines` to the multi-line or empty array opened at `open`.
fn append_items(text: &str, open: usize, lines: &[String]) -> Option<(Range<usize>, String)> {
    let close = php::closing(text, open)?;
    let eol = eol_of(text);
    let items = php::items(text, open);
    let base = indentation_of_line(text, open);

    if items.is_empty() {
        let body = lines
            .iter()
            .map(|line| format!("{base}    {line},"))
            .collect::<Vec<_>>()
            .join(eol);
        return Some((open + 1..close, format!("{eol}{body}{eol}{base}")));
    }

    if !text[open..close].contains('\n') {
        return None;
    }

    let first = items.first()?;
    let first_start = php::trimmed(text, first.key.clone().unwrap_or(first.value.clone())).start;
    let item_indent = indentation_of_line(text, first_start);
    let last = php::trimmed(text, items.last()?.value.clone()).end;
    let rest = &text[last..close];
    let comma = rest.trim_start().starts_with(',');
    let at = if comma {
        last + rest.find(',')? + 1
    } else {
        last
    };
    let body: String = lines
        .iter()
        .map(|line| format!("{eol}{item_indent}{line},"))
        .collect();

    Some((at..at, if comma { body } else { format!(",{body}") }))
}

fn model_of_request(stem: &str) -> Option<String> {
    let name = stem.strip_suffix("Request")?;
    let name = ["Store", "Update", "Create", "Edit", "Destroy"]
        .iter()
        .find_map(|prefix| name.strip_prefix(prefix))
        .unwrap_or(name);
    (!name.is_empty()).then(|| name.to_owned())
}

pub fn column_rewrites(index: &Index, path: &str, text: &str, at: usize) -> Vec<Rewrite> {
    let masked = php::mask_comments(text);
    let mut found = Vec::new();
    found.extend(request_rules(index, path, text, &masked, at));
    found.extend(fillable_columns(index, path, text, &masked, at));
    found
}

fn request_rules(
    index: &Index,
    path: &str,
    text: &str,
    masked: &str,
    at: usize,
) -> Option<Rewrite> {
    if !masked.contains("extends FormRequest") {
        return None;
    }

    let stem = path.rsplit('/').next()?.strip_suffix(".php")?;
    let model = model_of_request(stem)?;
    let table = index.models.get(&model)?;
    let columns = index.columns.get(table)?;

    let function = masked.find("function rules(")?;
    let body_open = masked[function..].find('{').map(|i| function + i)?;
    let body_close = php::closing(masked, body_open)?;
    if at < function || at > body_close {
        return None;
    }

    let returned = masked[body_open..body_close].find("return")? + body_open;
    let open = masked[returned..body_close]
        .find('[')
        .map(|i| returned + i)?;
    let present: Vec<String> = php::items(masked, open)
        .into_iter()
        .filter_map(|item| php::literal(masked, item.key?).map(|(key, _)| key.to_owned()))
        .collect();

    let lines: Vec<String> = columns
        .iter()
        .filter(|(name, _)| !LEFT_OUT_COLUMNS.contains(&name.as_str()) && !present.contains(name))
        .map(|(name, entry)| {
            format!(
                "'{name}' => {}",
                rule_for(name, entry.detail.as_deref().unwrap_or_default())
            )
        })
        .collect();
    if lines.is_empty() {
        return None;
    }

    let edit = append_items(text, open, &lines)?;
    Some(Rewrite {
        title: format!("Add validation rules for the columns of {model}"),
        kind: "refactor.rewrite",
        edits: vec![edit],
    })
}

fn fillable_columns(
    index: &Index,
    path: &str,
    text: &str,
    masked: &str,
    at: usize,
) -> Option<Rewrite> {
    let table = index.model_files.get(path)?;
    let columns = index.columns.get(table)?;

    let declaration = masked.find("$fillable")?;
    let open = masked[declaration..].find('[').map(|i| declaration + i)?;
    let semicolon = masked[declaration..].find(';').map(|i| declaration + i)?;
    if open > semicolon || at < declaration || at > semicolon {
        return None;
    }

    let present: Vec<String> = php::strings(masked, open, false);
    let lines: Vec<String> = columns
        .keys()
        .filter(|name| !LEFT_OUT_COLUMNS.contains(&name.as_str()) && !present.contains(name))
        .map(|name| format!("'{name}'"))
        .collect();
    if lines.is_empty() {
        return None;
    }

    let edit = append_items(text, open, &lines)?;
    Some(Rewrite {
        title: "Add the missing columns to $fillable".into(),
        kind: "refactor.rewrite",
        edits: vec![edit],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_snake_case_and_short() {
        assert_eq!(slug("Welcome back, friend!"), "welcome_back_friend");
        assert_eq!(slug("  Hi  "), "hi");
        assert_eq!(slug("!!!"), "");
    }

    #[test]
    fn lang_entries_go_before_the_closing_bracket() {
        let file = "<?php\n\nreturn [\n    'a' => 'b',\n];\n";
        let edit = entry_edit(file, "k", "it's").unwrap();
        assert_eq!(
            (edit.range.start.line, edit.text.as_str()),
            (4, "    'k' => 'it\\'s',\n")
        );
    }

    #[test]
    fn items_are_appended_to_empty_and_filled_arrays() {
        let empty = "return [\n];";
        let open = empty.find('[').unwrap();
        let (range, inserted) = append_items(empty, open, &["'a' => 1".into()]).unwrap();
        let mut out = empty.to_owned();
        out.replace_range(range, &inserted);
        assert_eq!(out, "return [\n    'a' => 1,\n];");

        let filled = "    return [\n        'x' => 1,\n        'y' => 2\n    ];";
        let open = filled.find('[').unwrap();
        let (range, inserted) = append_items(filled, open, &["'z' => 3".into()]).unwrap();
        let mut out = filled.to_owned();
        out.replace_range(range, &inserted);
        assert_eq!(
            out,
            "    return [\n        'x' => 1,\n        'y' => 2,\n        'z' => 3,\n    ];"
        );

        assert!(append_items("[1, 2]", 0, &["3".into()]).is_none());
    }

    #[test]
    fn rules_follow_the_column_type() {
        assert_eq!(
            rule_for("title", "string"),
            "['required', 'string', 'max:255']"
        );
        assert_eq!(
            rule_for("contact_email", "string"),
            "['required', 'email', 'max:255']"
        );
        assert_eq!(rule_for("owner_id", "foreignId"), "['required', 'integer']");
        assert_eq!(rule_for("x", "weird"), "['required']");
    }

    #[test]
    fn request_names_map_to_models() {
        assert_eq!(
            model_of_request("StorePostRequest").as_deref(),
            Some("Post")
        );
        assert_eq!(model_of_request("PostRequest").as_deref(), Some("Post"));
        assert_eq!(model_of_request("Request"), None);
    }
}

fn standalone_php_literal(source: &str, start: usize, end: usize) -> bool {
    if !source.trim_start().starts_with("<?php")
        || source.contains("?>")
        || source.contains("<<<")
        || start == 0
    {
        return false;
    }
    let masked = php::mask_comments(source);
    let bytes = masked.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if matches!(bytes[at], b'\'' | b'"') {
            let stop = php::string_end(bytes, at);
            if at == start - 1 {
                return bytes[at] == b'\'' && stop == end + 1;
            }
            at = stop;
        } else {
            at += 1;
        }
    }
    false
}
fn locale_owned(project: &Project, root: &str, path: &str) -> Result<(), openidea_plugin::Error> {
    use openidea_plugin::{Error, FileKind, host};
    if host::project_of(path)
        .and_then(|p| p.root().ok())
        .as_deref()
        != Some(root)
    {
        return Err(Error::Stale);
    }
    let mut parent = root.to_owned();
    for part in path
        .strip_prefix(&format!("{root}/"))
        .ok_or(Error::Stale)?
        .split('/')
    {
        parent.push('/');
        parent.push_str(part);
        match project.stat(&parent) {
            Ok(s) if s.kind == FileKind::Symlink => return Err(Error::Stale),
            Err(Error::NotFound(_)) => {}
            Err(e) => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

/// Explicit project-relative setting wins; otherwise select a proven conventional language root.
pub(crate) fn language_directory(
    project: &Project,
    root: &str,
) -> Result<String, openidea_plugin::Error> {
    use openidea_plugin::{Error, FileKind, PickItem, host};
    if let Some(configured) =
        host::setting_string("languageDirectory").filter(|value| !value.trim().is_empty())
    {
        if configured.len() > 512
            || configured.starts_with('/')
            || configured.contains('\\')
            || configured.chars().any(char::is_control)
            || configured
                .split('/')
                .any(|part| part.is_empty() || matches!(part, "." | ".."))
        {
            return Err(crate::ui_text::text(
                "native.lang_actions.70600750cc",
                "Language directory must be a project-relative directory without traversal.",
            )
            .into());
        }
        let path = format!("{root}/{configured}");
        locale_owned(project, root, &path)?;
        match project.stat(&path) {
            Ok(stat) if stat.kind != FileKind::Directory => {
                return Err(Error::Invalid(
                    crate::ui_text::text(
                        "native.lang_actions.d0335d035a",
                        "Language directory is not a plain directory.",
                    )
                    .into(),
                ));
            }
            Err(Error::NotFound(_)) => {}
            Err(error) => return Err(error),
            _ => {}
        }
        return Ok(configured);
    }
    let modern = project
        .stat(&format!("{root}/lang"))
        .is_ok_and(|s| s.kind == FileKind::Directory);
    let legacy = project
        .stat(&format!("{root}/resources/lang"))
        .is_ok_and(|s| s.kind == FileKind::Directory);
    match (modern, legacy) {
        (false, true) => Ok("resources/lang".into()),
        (true, true) => host::pick(
            &crate::ui_text::t("prompt.translation-directory", "Translation directory"),
            &[
                PickItem::new("lang", "lang"),
                PickItem::new("resources/lang", "resources/lang"),
            ],
        )?
        .filter(|value| matches!(value.as_str(), "lang" | "resources/lang"))
        .ok_or(Error::Cancelled),
        _ => Ok("lang".into()),
    }
}

fn parse_php_key(key: &str) -> Option<(String, Vec<String>)> {
    let (namespace, key) = if let Some((namespace, key)) = key.split_once("::") {
        if namespace.is_empty()
            || namespace.len() > 128
            || !namespace
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return None;
        }
        (Some(namespace), key)
    } else {
        (None, key)
    };
    let mut segments = key.split('.');
    let file = segments.next()?;
    let rest: Vec<String> = segments.map(str::to_owned).collect();
    if rest.is_empty()
        || rest.len() > 16
        || std::iter::once(file)
            .chain(rest.iter().map(String::as_str))
            .any(|s| {
                s.is_empty()
                    || s.len() > 128
                    || !s
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            })
    {
        return None;
    }
    Some((
        namespace.map_or_else(
            || file.to_owned(),
            |namespace| format!("{namespace}::{file}"),
        ),
        rest,
    ))
}
fn php_quote(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}
fn literal_array_tree(source: &str, open: usize, depth: usize, budget: &mut usize) -> bool {
    if depth > 16 || *budget == 0 {
        return false;
    }
    *budget -= 1;
    let mut names = std::collections::BTreeSet::new();
    for item in php::items(source, open) {
        let Some((key, _)) = item.key.and_then(|k| php::literal(source, k)) else {
            return false;
        };
        if key.contains(['\\', '$']) || !names.insert(key.to_owned()) {
            return false;
        }
        if let Some(child) = php::array_open(source, item.value.clone()) {
            if php::closing(source, child).map(|end| end + 1)
                != Some(php::trimmed(source, item.value.clone()).end)
                || !literal_array_tree(source, child, depth + 1, budget)
            {
                return false;
            }
        } else if php::literal(source, item.value).is_none() {
            return false;
        }
    }
    true
}
/// Insert only into a closed, standalone literal PHP return array, preserving its existing source.
#[cfg(test)]
fn insert_php_translation(
    source: &str,
    segments: &[String],
    value: &str,
) -> Result<String, openidea_plugin::Error> {
    plan_php_translation(source, segments, value, false)
}
fn plan_php_translation(
    source: &str,
    segments: &[String],
    value: &str,
    allow_reuse: bool,
) -> Result<String, openidea_plugin::Error> {
    use openidea_plugin::Error;
    if source.len() > 128 * 1024
        || source.contains("<<<")
        || source.contains("?>")
        || segments.is_empty()
    {
        return Err(Error::Invalid(
            "Translation PHP must be one literal return array.".into(),
        ));
    }
    let masked = php::mask_comments(source);
    let head = masked
        .trim_start()
        .strip_prefix("<?php")
        .ok_or_else(|| Error::Invalid("Translation PHP must start with <?php.".into()))?
        .trim_start();
    let returned = head
        .strip_prefix("return")
        .filter(|tail| tail.starts_with(char::is_whitespace))
        .ok_or_else(|| Error::Invalid("Translation PHP must contain only a return array.".into()))?
        .trim_start();
    let open = masked.len() - returned.len();
    let open = php::array_open(&masked, open..masked.len())
        .ok_or_else(|| Error::Invalid("Translation PHP must return a literal array.".into()))?;
    let close = php::closing(&masked, open).ok_or(Error::Stale)?;
    if masked[close + 1..].trim() != ";" || !literal_array_tree(&masked, open, 0, &mut 4096) {
        return Err(Error::Invalid(
            "Translation PHP must contain unique literal string keys and string/array values only."
                .into(),
        ));
    }
    let mut current = open;
    let mut remaining = segments;
    while let Some(segment) = remaining.first() {
        let existing = php::items(&masked, current).into_iter().find(|item| {
            item.key
                .clone()
                .and_then(|r| php::literal(&masked, r))
                .is_some_and(|(name, _)| name == segment)
        });
        let Some(existing) = existing else {
            break;
        };
        if remaining.len() == 1 {
            if allow_reuse && php::literal(&masked, existing.value.clone()).is_some() {
                return Ok(source.to_owned());
            }
            return Err("Translation key already exists; no translation is overwritten.".into());
        }
        current = php::array_open(&masked, existing.value).ok_or_else(|| {
            Error::Invalid("A parent translation key is a scalar, not an array.".into())
        })?;
        remaining = &remaining[1..];
    }
    let eol = eol_of(source);
    let base = indentation_of_line(source, current);
    let indent = format!("{base}    ");
    let mut body = php_quote(value);
    for (at, segment) in remaining.iter().enumerate().rev() {
        body = format!("{} => {body}", php_quote(segment));
        if at != 0 {
            body = format!("[{body}]");
        }
    }
    let close = php::closing(&masked, current).ok_or(Error::Stale)?;
    let before_line = source[..close].rfind('\n').map_or(0, |at| at + 1);
    let insert = if source[before_line..close].trim().is_empty() {
        before_line
    } else {
        close
    };
    let items = php::items(&masked, current);
    let mut out = source.to_owned();
    let prefix = if insert > 0 && source.as_bytes()[insert - 1] == b'\n' {
        ""
    } else {
        eol
    };
    out.insert_str(insert, &format!("{prefix}{indent}{body},{eol}"));
    if let Some(last) = items.last() {
        let last_end = php::trimmed(&masked, last.value.clone()).end;
        if !masked[last_end..close].trim_start().starts_with(',') {
            out.insert(last_end, ',');
        }
    }
    Ok(out)
}

/// Explicit locale JSON extraction uses one reviewed workspace transaction.
pub(crate) fn locale_actions(
    doc: &Document,
    language: &str,
    selection: openidea_plugin::Range,
) -> Vec<openidea_plugin::file_actions::FileAction> {
    let Some(range) = selected_range(doc, selection) else {
        return vec![];
    };
    if doc.text.len() > 128 * 1024 || !plain_words(&doc.text[range.clone()]) {
        return vec![];
    }
    if language == "blade" {
        if doc.text[range.clone()].contains('&')
            || inside_tag(&doc.text, range.start)
            || refs::blade_regions(&doc.text)
                .iter()
                .any(|r| r.start < range.end && range.start < r.end)
        {
            return vec![];
        }
    } else if language != "php" || !standalone_php_literal(&doc.text, range.start, range.end) {
        return vec![];
    }
    let json_action = openidea_plugin::file_actions::FileAction {
        title: crate::ui_text::t(
            "action.locale-json",
            "Extract to reviewed locale JSON translation…",
        ),
        kind: Some("refactor.extract".into()),
        data: Some(format!(
            "locale-json:{}",
            serde_json::json!([
                doc.path,
                doc.version,
                doc.text,
                range.start,
                range.end,
                language
            ])
        )),
        ..Default::default()
    };
    let mut php_action = json_action.clone();
    php_action.title = crate::ui_text::t(
        "action.locale-php",
        "Extract to reviewed locale PHP translation…",
    );
    php_action.data = php_action
        .data
        .map(|d| d.replacen("locale-json:", "locale-php:", 1));
    vec![json_action, php_action]
}
pub(crate) fn resolve_locale(
    doc: &Document,
    action: openidea_plugin::file_actions::FileAction,
) -> Result<openidea_plugin::file_actions::FileAction, openidea_plugin::Error> {
    use openidea_plugin::{
        Error, Position, Range as IdeRange,
        file_actions::{FileAction, FileOperation},
        host,
    };
    let php_target = action
        .data
        .as_deref()
        .is_some_and(|s| s.starts_with("locale-php:"));
    let data = action
        .data
        .as_deref()
        .and_then(|s| {
            s.strip_prefix(if php_target {
                "locale-php:"
            } else {
                "locale-json:"
            })
        })
        .filter(|s| s.len() <= 256 * 1024)
        .ok_or(Error::Stale)?;
    let data: serde_json::Value = serde_json::from_str(data).map_err(|_| Error::Stale)?;
    if data[0] != doc.path || data[1] != doc.version || data[2] != doc.text {
        return Err(Error::Stale);
    }
    let start = data[3].as_u64().ok_or(Error::Stale)? as usize;
    let end = data[4].as_u64().ok_or(Error::Stale)? as usize;
    let language = data[5].as_str().ok_or(Error::Stale)?;
    let selection = IdeRange::new(
        text::position(&doc.text, start),
        text::position(&doc.text, end),
    );
    if !locale_actions(doc, language, selection)
        .iter()
        .any(|a| a.data == action.data)
    {
        return Err(Error::Stale);
    }
    let (project, root) = crate::view_creation::owner(doc, None)?;
    if project.read_file(&doc.path)? != doc.text {
        return Err(Error::Stale);
    }
    let Some(locale) = host::input(
        &if php_target {
            crate::ui_text::t("prompt.locale-php", "Target locale (PHP translations)")
        } else {
            crate::ui_text::t("prompt.locale-json", "Target locale (JSON translations)")
        },
        Some("en"),
    )?
    else {
        return Err(Error::Cancelled);
    };
    let locales: Vec<String> = locale
        .split(',')
        .map(str::trim)
        .map(str::to_owned)
        .collect();
    let unique: std::collections::BTreeSet<_> = locales.iter().collect();
    if locales.len() > 8
        || unique.len() != locales.len()
        || locales.iter().any(|locale| {
            locale.is_empty()
                || locale.len() > 64
                || !locale
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        })
    {
        return Err(crate::ui_text::text(
            "native.lang_actions.b2a1cccdfe",
            "Provide one to eight unique locale names separated by commas.",
        )
        .into());
    }
    let Some(key) = host::input(
        &if php_target {
            crate::ui_text::t(
                "prompt.translation-key-php",
                "Translation key (file.nested.key or package::file.key)",
            )
        } else {
            crate::ui_text::t("prompt.translation-key", "Translation key")
        },
        Some(if php_target {
            "messages.greeting"
        } else {
            &doc.text[start..end]
        }),
    )?
    else {
        return Err(Error::Cancelled);
    };
    if key.is_empty() || key.len() > 512 || key.contains(['\0', '\n', '\r']) {
        return Err(crate::ui_text::text(
            "native.lang_actions.e223d50f7e",
            "Invalid translation key.",
        )
        .into());
    }
    let language_setting = host::setting_string("languageDirectory");
    let directory = language_directory(&project, &root)?;
    let php_key = if php_target {
        Some(parse_php_key(&key).ok_or_else(|| Error::Invalid(crate::ui_text::text("native.lang_actions.38c01446cd", "PHP translation keys must have file.nested.key segments using letters, numbers, underscores or hyphens.").into()))?)
    } else {
        None
    };
    locale_owned(&project, &root, &doc.path)?;
    let mut targets = Vec::new();
    for locale in &locales {
        let path = match &php_key {
            Some((file, _)) => match file.split_once("::") {
                Some((namespace, file)) => {
                    format!("{root}/{directory}/vendor/{namespace}/{locale}/{file}.php")
                }
                None => format!("{root}/{directory}/{locale}/{file}.php"),
            },
            None => format!("{root}/{directory}/{locale}.json"),
        };
        locale_owned(&project, &root, &path)?;
        let original = match project.read_file(&path) {
            Ok(source) if source.len() <= 128 * 1024 => Some(source),
            Err(Error::NotFound(_)) => {
                crate::view_creation::missing(&project, &path)?;
                None
            }
            Ok(_) => {
                return Err(crate::ui_text::text(
                    "native.lang_actions.7ad15bae3a",
                    "Locale file exceeds the edit limit.",
                )
                .into());
            }
            Err(e) => return Err(e),
        };
        let translated = if let Some((_, segments)) = &php_key {
            plan_php_translation(
                original.as_deref().unwrap_or("<?php\n\nreturn [\n];\n"),
                segments,
                &doc.text[start..end],
                true,
            )
            .map_err(locale_plan_error)?
        } else {
            let mut values = match &original {
                Some(source) => crate::strict_json::parse(source, 128 * 1024)?
                    .as_object()
                    .filter(|o| o.values().all(serde_json::Value::is_string))
                    .cloned()
                    .ok_or_else(|| {
                        Error::Invalid(
                            crate::ui_text::text(
                                "native.lang_actions.407a4bb649",
                                "Locale JSON must contain string values.",
                            )
                            .into(),
                        )
                    })?,
                None => serde_json::Map::new(),
            };
            if values.contains_key(&key) {
                original.clone().ok_or(Error::Stale)?
            } else {
                values.insert(
                    key.clone(),
                    serde_json::Value::String(doc.text[start..end].into()),
                );
                format!(
                    "{}\n",
                    serde_json::to_string_pretty(&values)
                        .map_err(|e| Error::Invalid(e.to_string()))?
                )
            }
        };
        if original.as_deref() == Some(translated.as_str()) {
            let prompt = crate::ui_text::format(
                "locale.reusePrompt",
                "Reuse existing translation {key} in {locale}?",
                &[("key", &key), ("locale", locale)],
            );
            let choice = host::pick(
                &prompt,
                &[
                    openidea_plugin::PickItem::new(
                        "reuse",
                        crate::ui_text::text(
                            "locale.reuseValue",
                            "Reuse existing value without changing the locale file",
                        ),
                    ),
                    openidea_plugin::PickItem::new(
                        "cancel",
                        crate::ui_text::text("locale.cancelExtraction", "Cancel extraction"),
                    ),
                ],
            )?;
            if choice.as_deref() != Some("reuse") {
                return Err(Error::Cancelled);
            }
        }
        targets.push((locale.clone(), path, original, translated));
    }
    let replace = if language == "php" {
        start - 1..end + 1
    } else {
        start..end
    };
    let literal = format!("'{}'", key.replace('\\', "\\\\").replace('\'', "\\'"));
    let call = format!("__({literal})");
    let call = if language == "blade" {
        format!("{{{{ {call} }}}}")
    } else {
        call
    };
    let mut replacement = doc.text.clone();
    replacement.replace_range(replace, &call);
    let files_preview = targets
        .iter()
        .map(|(locale, path, _, contents)| {
            crate::ui_text::format(
                "locale.file-review",
                "Locale: {locale}\nTarget: {path}\nComplete locale contents:\n{contents}",
                &[("locale", locale), ("path", path), ("contents", contents)],
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let review = crate::ui_text::format(
        "locale.review",
        "Source: {source}\n\n{files}\nComplete replacement source:\n{replacement}\n\nApply all locale files and the source replacement in one undoable transaction?",
        &[
            ("source", &doc.path),
            ("files", &files_preview),
            ("replacement", &replacement),
        ],
    );
    if !ui_kit::prompts::confirm_exact(
        &crate::ui_text::t("prompt.translation-confirm", "Extract locale translation?"),
        &review,
    )? {
        return Err(Error::Cancelled);
    }
    crate::view_creation::owner(doc, Some(&root))?;
    if host::setting_string("languageDirectory") != language_setting {
        return Err(Error::Stale);
    }
    locale_owned(&project, &root, &doc.path)?;
    if project.read_file(&doc.path)? != doc.text {
        return Err(Error::Stale);
    }
    let mut fixed = FileAction {
        title: action.title,
        edits: vec![(
            doc.path.clone(),
            vec![TextEdit {
                range: IdeRange::new(
                    Position::new(0, 0),
                    text::position(&doc.text, doc.text.len()),
                ),
                text: replacement,
            }],
        )],
        open: targets.first().map(|(_, path, _, _)| path.clone()),
        ..Default::default()
    };
    for (_, path, original, translated) in targets {
        locale_owned(&project, &root, &path)?;
        if let Some(original) = original {
            if project.read_file(&path)? != original {
                return Err(Error::Stale);
            }
            if translated == original {
                continue;
            }
            fixed.edits.push((
                path,
                vec![TextEdit {
                    range: IdeRange::new(
                        Position::new(0, 0),
                        text::position(&original, original.len()),
                    ),
                    text: translated,
                }],
            ));
        } else {
            crate::view_creation::missing(&project, &path)?;
            fixed.operations.push(FileOperation::Create {
                path,
                text: translated,
                overwrite: false,
            });
        }
    }
    Ok(fixed)
}

#[cfg(test)]
mod locale_tests {
    use super::*;
    use openidea_plugin::{Position, Range, file_actions::FileOperation, mock::MockHost};
    fn fixture() -> (MockHost, Document, Range) {
        let h = MockHost::install();
        let source = "<?php echo 'Welcome home';\n";
        h.add_file("app/example.php", source);
        let start = source.find("Welcome").unwrap();
        let doc = Document {
            path: "/project/app/example.php".into(),
            language: "php".into(),
            version: 1,
            text: source.into(),
        };
        let range = Range::new(
            text::position(source, start),
            text::position(source, start + 12),
        );
        (h, doc, range)
    }
    #[test]
    fn locale_extraction_plans_source_edit_and_json_create_atomically() {
        let (h, doc, range) = fixture();
        let action = locale_actions(&doc, "php", range).remove(0);
        h.answer_next_prompt(Some(&["fr"]));
        h.answer_next_prompt(Some(&["welcome"]));
        h.answer_next_prompt(Some(&["yes"]));
        let fixed = resolve_locale(&doc, action).unwrap();
        assert_eq!(fixed.edits.len(), 1);
        assert!(fixed.edits[0].1[0].text.contains("__('welcome')"));
        let [
            FileOperation::Create {
                path,
                text,
                overwrite,
            },
        ] = fixed.operations.as_slice()
        else {
            panic!("one create");
        };
        assert_eq!(path, "/project/lang/fr.json");
        assert!(!overwrite);
        assert!(text.contains("Welcome home"));
    }
    #[test]
    fn locale_extraction_preserves_existing_keys_and_refuses_duplicates() {
        for duplicate in [false, true] {
            let (h, doc, range) = fixture();
            h.add_file("lang/fr.json", r#"{"existing":"Bonjour"}"#);
            let action = locale_actions(&doc, "php", range).remove(0);
            h.answer_next_prompt(Some(&["fr"]));
            h.answer_next_prompt(Some(&[if duplicate { "existing" } else { "welcome" }]));
            h.answer_next_prompt(Some(&["yes"]));
            let result = resolve_locale(&doc, action);
            if duplicate {
                assert!(result.is_err());
            } else {
                let fixed = result.unwrap();
                assert!(fixed.operations.is_empty());
                assert_eq!(fixed.edits.len(), 2);
                assert!(fixed.edits[1].1[0].text.contains("Bonjour"));
            }
        }
    }
    #[test]
    fn existing_php_and_json_keys_can_be_explicitly_reused_without_locale_edits() {
        for php in [false, true] {
            let (host, doc, range) = fixture();
            let (path, contents, key) = if php {
                (
                    "lang/fr/messages.php",
                    "<?php\nreturn ['greeting' => ['welcome' => 'Bonjour']];\n",
                    "messages.greeting.welcome",
                )
            } else {
                (
                    "lang/fr.json",
                    "{\n  \"welcome\": \"Bonjour\"\n}\n",
                    "welcome",
                )
            };
            host.add_file(path, contents);
            let action = locale_actions(&doc, "php", range).remove(usize::from(php));
            host.answer_next_prompt(Some(&["fr"]));
            host.answer_next_prompt(Some(&[key]));
            host.answer_next_prompt(Some(&["reuse"]));
            host.answer_next_prompt(Some(&["yes"]));
            let fixed = resolve_locale(&doc, action).unwrap();
            assert!(fixed.operations.is_empty());
            assert_eq!(fixed.edits.len(), 1);
            assert!(fixed.edits[0].1[0].text.contains(&format!("__('{key}')")));
            assert_eq!(
                openidea_plugin::host::projects().unwrap()[0]
                    .read_file(path)
                    .unwrap(),
                contents
            );
        }
    }
    #[test]
    fn reused_locale_and_new_locale_stay_one_reviewed_transaction() {
        let (host, doc, range) = fixture();
        host.add_file("lang/fr.json", "{\"welcome\":\"Bonjour\"}");
        let action = locale_actions(&doc, "php", range).remove(0);
        host.answer_next_prompt(Some(&["fr,ja"]));
        host.answer_next_prompt(Some(&["welcome"]));
        host.answer_next_prompt(Some(&["reuse"]));
        host.answer_next_prompt(Some(&["yes"]));
        let fixed = resolve_locale(&doc, action).unwrap();
        assert_eq!(fixed.edits.len(), 1);
        assert!(
            matches!(&fixed.operations[..], [FileOperation::Create{path,overwrite:false,..}] if path=="/project/lang/ja.json")
        );
    }
    #[test]
    fn blade_entities_are_not_double_escaped_by_locale_extraction() {
        let _host = MockHost::install();
        let source = "<p>Welcome &amp; hello</p>";
        let doc = Document {
            path: "/project/resources/views/example.blade.php".into(),
            language: "blade".into(),
            version: 1,
            text: source.into(),
        };
        let start = source.find("Welcome").unwrap();
        let end = source.find("</p>").unwrap();
        assert!(
            locale_actions(
                &doc,
                "blade",
                Range::new(text::position(source, start), text::position(source, end))
            )
            .is_empty()
        );
    }
    #[test]
    fn php_comment_and_string_lookalikes_are_not_extractable() {
        for source in ["<?php // 'Welcome home'", "<?php $x=\" 'Welcome home' \";"] {
            let start = source.find("Welcome").unwrap();
            let doc = Document {
                path: "/project/a.php".into(),
                language: "php".into(),
                version: 1,
                text: source.into(),
            };
            assert!(
                locale_actions(
                    &doc,
                    "php",
                    Range::new(
                        text::position(source, start),
                        text::position(source, start + 12)
                    )
                )
                .is_empty()
            );
        }
        let _ = Position::new(0, 0);
    }
    #[test]
    fn php_locale_extracts_new_nested_key_with_one_atomic_create() {
        let (h, doc, range) = fixture();
        let action = locale_actions(&doc, "php", range).remove(1);
        h.answer_next_prompt(Some(&["fr"]));
        h.answer_next_prompt(Some(&["messages.account.welcome"]));
        h.answer_next_prompt(Some(&["yes"]));
        let fixed = resolve_locale(&doc, action).unwrap();
        assert!(
            fixed.edits[0].1[0]
                .text
                .contains("__('messages.account.welcome')")
        );
        let [
            FileOperation::Create {
                path,
                text,
                overwrite,
            },
        ] = fixed.operations.as_slice()
        else {
            panic!("one PHP create");
        };
        assert_eq!(path, "/project/lang/fr/messages.php");
        assert!(!overwrite);
        assert!(text.contains("'account' => ['welcome' => 'Welcome home']"));
    }
    #[test]
    fn php_locale_preserves_nested_existing_source_and_adds_missing_comma() {
        let _host = MockHost::install();
        let source = "<?php\nreturn [\n    // Keep this documentation.\n    'account' => [\n        'old' => 'Bonjour'\n    ],\n];\n";
        let updated =
            insert_php_translation(source, &["account".into(), "welcome".into()], "Salut").unwrap();
        assert!(updated.contains("// Keep this documentation."));
        assert!(updated.contains("'old' => 'Bonjour',"));
        assert!(updated.contains("'welcome' => 'Salut',"));
        assert!(
            insert_php_translation(source, &["account".into(), "old".into()], "changed").is_err()
        );
        let repeated =
            insert_php_translation("<?php return [];", &["a".into(), "a".into()], "value").unwrap();
        assert!(repeated.contains("'a' => ['a' => 'value']"));
    }
    #[test]
    fn php_locale_refuses_dynamic_duplicate_or_scalar_parent_arrays() {
        let _host = MockHost::install();
        for source in [
            "<?php return $values;",
            "<?php return ['a' => helper()];",
            "<?php return ['a' => [], 'a' => []];",
            "<?php return ['a' => [] + $other];",
            "<?php return ['a' => 'scalar'];",
        ] {
            assert!(
                insert_php_translation(source, &["a".into(), "b".into()], "Hello").is_err(),
                "{source}"
            );
        }
        for key in ["../x.a", "messages..a", "messages", "messages.a/b"] {
            assert!(parse_php_key(key).is_none());
        }
    }
    #[test]
    fn php_locale_existing_target_uses_edit_and_duplicate_never_overwrites() {
        for duplicate in [false, true] {
            let (h, doc, range) = fixture();
            h.add_file(
                "lang/fr/messages.php",
                "<?php\nreturn [\n    'existing' => 'Bonjour',\n];\n",
            );
            let action = locale_actions(&doc, "php", range).remove(1);
            h.answer_next_prompt(Some(&["fr"]));
            h.answer_next_prompt(Some(&[if duplicate {
                "messages.existing"
            } else {
                "messages.welcome"
            }]));
            h.answer_next_prompt(Some(&["yes"]));
            let result = resolve_locale(&doc, action);
            if duplicate {
                assert!(result.is_err());
            } else {
                let fixed = result.unwrap();
                assert!(fixed.operations.is_empty());
                assert_eq!(fixed.edits.len(), 2);
                assert!(fixed.edits[1].1[0].text.contains("'existing' => 'Bonjour'"));
                assert!(
                    fixed.edits[1].1[0]
                        .text
                        .contains("'welcome' => 'Welcome home'")
                );
            }
        }
    }
    #[test]
    fn php_locale_explicit_vendor_namespace_creates_owned_application_translation() {
        let (h, doc, range) = fixture();
        let action = locale_actions(&doc, "php", range).remove(1);
        h.answer_next_prompt(Some(&["fr"]));
        h.answer_next_prompt(Some(&["billing::messages.welcome"]));
        h.answer_next_prompt(Some(&["yes"]));
        let fixed = resolve_locale(&doc, action).unwrap();
        let [FileOperation::Create { path, .. }] = fixed.operations.as_slice() else {
            panic!("vendor locale create");
        };
        assert_eq!(path, "/project/lang/vendor/billing/fr/messages.php");
        assert!(
            fixed.edits[0].1[0]
                .text
                .contains("__('billing::messages.welcome')")
        );
    }
    #[test]
    fn multiple_locale_targets_are_reviewed_and_created_as_one_transaction() {
        let (host, doc, range) = fixture();
        let action = locale_actions(&doc, "php", range).remove(0);
        host.answer_next_prompt(Some(&["fr, ja, ko"]));
        host.answer_next_prompt(Some(&["greeting"]));
        host.answer_next_prompt(Some(&["yes"]));
        let fixed = resolve_locale(&doc, action).unwrap();
        assert_eq!(fixed.operations.len(), 3);
        assert_eq!(fixed.edits.len(), 1);
        let paths: Vec<_> = fixed
            .operations
            .iter()
            .map(|op| match op {
                FileOperation::Create { path, .. } => path.as_str(),
                _ => panic!("create"),
            })
            .collect();
        assert_eq!(
            paths,
            vec![
                "/project/lang/fr.json",
                "/project/lang/ja.json",
                "/project/lang/ko.json"
            ]
        );
    }
    #[test]
    fn one_conflicting_locale_refuses_the_entire_multifile_plan() {
        let (host, doc, range) = fixture();
        host.add_file("lang/ko.json", r#"{"greeting":"Existing"}"#);
        let action = locale_actions(&doc, "php", range).remove(0);
        host.answer_next_prompt(Some(&["fr,ko"]));
        host.answer_next_prompt(Some(&["greeting"]));
        assert!(resolve_locale(&doc, action).is_err());
        assert!(
            !host
                .invocation()
                .project()
                .unwrap()
                .exists("lang/fr.json")
                .unwrap()
        );
    }
    #[test]
    fn configured_language_root_is_used_without_leaving_the_owned_project() {
        let (host, doc, range) = fixture();
        host.set_setting("languageDirectory", "\"translations/app\"");
        let action = locale_actions(&doc, "php", range).remove(0);
        host.answer_next_prompt(Some(&["fr"]));
        host.answer_next_prompt(Some(&["greeting"]));
        host.answer_next_prompt(Some(&["yes"]));
        let fixed = resolve_locale(&doc, action).unwrap();
        let [FileOperation::Create { path, .. }] = fixed.operations.as_slice() else {
            panic!("create");
        };
        assert_eq!(path, "/project/translations/app/fr.json");
        host.set_setting("languageDirectory", "\"../outside\"");
        assert!(language_directory(&host.invocation().project().unwrap(), "/project").is_err());
    }
}

fn locale_plan_error(error: openidea_plugin::Error) -> openidea_plugin::Error {
    let translate = |message: String| match message.as_str() {
        "Translation PHP must be one literal return array." => {
            crate::ui_text::t("native.lang_actions.4c32d2e4ed", &message)
        }
        "Translation PHP must start with <?php." => {
            crate::ui_text::t("native.lang_actions.8da97a3963", &message)
        }
        "Translation PHP must contain only a return array." => {
            crate::ui_text::t("native.lang_actions.4037b4c585", &message)
        }
        "Translation PHP must return a literal array." => {
            crate::ui_text::t("native.lang_actions.f2daa86d24", &message)
        }
        "Translation PHP must contain unique literal string keys and string/array values only." => {
            crate::ui_text::t("native.lang_actions.e89e8e53ec", &message)
        }
        "Translation key already exists; no translation is overwritten." => {
            crate::ui_text::t("native.lang_actions.856aec0563", &message)
        }
        "A parent translation key is a scalar, not an array." => {
            crate::ui_text::t("native.lang_actions.2f926815de", &message)
        }
        _ => message,
    };
    match error {
        openidea_plugin::Error::Invalid(message) => {
            openidea_plugin::Error::Invalid(translate(message))
        }
        openidea_plugin::Error::Failed(message) => {
            openidea_plugin::Error::Failed(translate(message))
        }
        other => other,
    }
}

// Only called when publishing actions; source extraction and column inference remain pure.
pub(crate) fn localized_title(title: &str) -> String {
    if let Some(key) = title
        .strip_prefix("Extract to translation key \"")
        .and_then(|s| s.strip_suffix('"'))
    {
        return crate::ui_text::format(
            "final.lang.extractTitle",
            "Extract to translation key \"{key}\"",
            &[("key", key)],
        );
    }
    if let Some(model) = title.strip_prefix("Add validation rules for the columns of ") {
        return crate::ui_text::format(
            "final.lang.columnsTitle",
            "Add validation rules for the columns of {model}",
            &[("model", model)],
        );
    }
    title.to_owned()
}

#[cfg(test)]
mod publication_locale_tests {
    #[test]
    fn legacy_action_titles_localize_without_replacing_project_key_or_class() {
        let host = openidea_plugin::mock::MockHost::install();
        for locale in ["ja", "ko", "zh-CN"] {
            host.set_setting("uiLanguage", &serde_json::to_string(locale).unwrap());
            for (title, value) in [
                (
                    "Extract to translation key \"messages.raw_{model}.日本\"",
                    "messages.raw_{model}.日本",
                ),
                (
                    "Add validation rules for the columns of App\\Models\\日本_{key}",
                    "App\\Models\\日本_{key}",
                ),
            ] {
                let localized = super::localized_title(title);
                assert_ne!(localized, title);
                assert!(localized.contains(value));
            }
            assert_eq!(
                super::localized_title("project supplied opaque title"),
                "project supplied opaque title"
            );
        }
    }
}
