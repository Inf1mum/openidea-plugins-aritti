//! Class rewrites: `scopeX` to `#[Scope]`, a method argument to a promoted
//! constructor property, and a facade call to an injected dependency.

use std::ops::Range;

use crate::actions::Rewrite;
use crate::php::{self, is_identifier};

/// Facade, the class the Laravel documentation lists as its underlying class, and the property name.
const FACADE_CLASSES: &[(&str, &str)] = &[
    ("Artisan", "Illuminate\\Contracts\\Console\\Kernel"),
    ("Auth", "Illuminate\\Auth\\AuthManager"),
    ("Cache", "Illuminate\\Cache\\CacheManager"),
    ("Config", "Illuminate\\Config\\Repository"),
    ("Cookie", "Illuminate\\Cookie\\CookieJar"),
    ("Crypt", "Illuminate\\Encryption\\Encrypter"),
    ("DB", "Illuminate\\Database\\DatabaseManager"),
    ("Event", "Illuminate\\Events\\Dispatcher"),
    ("File", "Illuminate\\Filesystem\\Filesystem"),
    ("Gate", "Illuminate\\Contracts\\Auth\\Access\\Gate"),
    ("Hash", "Illuminate\\Hashing\\HashManager"),
    ("Http", "Illuminate\\Http\\Client\\Factory"),
    ("Lang", "Illuminate\\Translation\\Translator"),
    ("Log", "Illuminate\\Log\\LogManager"),
    ("Mail", "Illuminate\\Mail\\Mailer"),
    ("Queue", "Illuminate\\Queue\\QueueManager"),
    ("Redirect", "Illuminate\\Routing\\Redirector"),
    ("Session", "Illuminate\\Session\\SessionManager"),
    ("Storage", "Illuminate\\Filesystem\\FilesystemManager"),
    ("URL", "Illuminate\\Routing\\UrlGenerator"),
    ("Validator", "Illuminate\\Validation\\Factory"),
    ("View", "Illuminate\\View\\Factory"),
];

struct Class {
    open: usize,
    close: usize,
    parent: Option<String>,
}

struct Method {
    line_start: usize,
    name: String,
    is_static: bool,
    params: Range<usize>,
    body: Range<usize>,
}

/// Preserve byte offsets while removing strings and comments. Refactorings must
/// never read class declarations or variable bindings out of quoted examples.
fn code(text: &str) -> Option<String> {
    if text.len() > 256 << 10 || text.contains("<<<") || text.contains('`') {
        return None;
    }
    let source = text.as_bytes();
    let mut masked = source.to_vec();
    let mut at = 0;
    let mut stack = Vec::new();
    while at < source.len() {
        let start = at;
        if source[at..].starts_with(b"/*") {
            at += source[at + 2..].windows(2).position(|pair| pair == b"*/")? + 4;
        } else if source[at..].starts_with(b"//")
            || source[at] == b'#' && source.get(at + 1) != Some(&b'[')
        {
            at += source[at..]
                .iter()
                .position(|byte| *byte == b'\n')
                .unwrap_or(source.len() - at);
        } else if matches!(source[at], b'\'' | b'"') {
            let quote = source[at];
            at += 1;
            while at < source.len() && source[at] != quote {
                at += if source[at] == b'\\' { 2 } else { 1 };
            }
            if at >= source.len() {
                return None;
            }
            at += 1;
        } else {
            match source[at] {
                b'(' => stack.push(b')'),
                b'[' => stack.push(b']'),
                b'{' => stack.push(b'}'),
                b')' | b']' | b'}' if stack.pop() != Some(source[at]) => return None,
                _ => {}
            }
            if stack.len() > 128 {
                return None;
            }
            at += 1;
            continue;
        }
        for byte in &mut masked[start..at] {
            if !matches!(*byte, b'\r' | b'\n') {
                *byte = b' ';
            }
        }
    }
    stack
        .is_empty()
        .then(|| String::from_utf8(masked).ok())
        .flatten()
}

fn direct_scope(masked: &str, open: usize, at: usize) -> bool {
    let body = &masked[open..at];
    body.bytes().filter(|b| *b == b'{').count() == body.bytes().filter(|b| *b == b'}').count() + 1
}

fn class_of(masked: &str) -> Option<Class> {
    if masked
        .match_indices("class ")
        .filter(|(at, _)| {
            *at == 0
                || !is_identifier(masked.as_bytes()[at - 1]) && masked.as_bytes()[at - 1] != b':'
        })
        .count()
        != 1
    {
        return None;
    }
    let at = masked.match_indices("class ").find_map(|(at, _)| {
        let line_start = masked[..at].rfind('\n').map_or(0, |i| i + 1);
        masked[line_start..at]
            .split_whitespace()
            .all(|word| matches!(word, "final" | "abstract" | "readonly"))
            .then_some(at)
    })?;
    let open = masked[at..].find('{').map(|i| at + i)?;
    let close = php::closing(masked, open)?;
    let header: Vec<&str> = masked[at..open].split_whitespace().collect();
    let parent = header
        .iter()
        .position(|word| *word == "extends")
        .and_then(|i| header.get(i + 1))
        .map(|p| (*p).to_owned());

    Some(Class {
        open,
        close,
        parent,
    })
}

fn methods(masked: &str, class: &Class) -> Vec<Method> {
    let mut found = Vec::new();
    let mut from = class.open;

    while let Some(at) = masked[from..class.close]
        .find("function ")
        .map(|i| from + i)
    {
        from = at + "function ".len();
        if !direct_scope(masked, class.open, at) {
            continue;
        }
        let line_start = masked[..at].rfind('\n').map_or(0, |i| i + 1);
        let modifiers = &masked[line_start..at];
        if !modifiers.split_whitespace().all(|word| {
            matches!(
                word,
                "public" | "protected" | "private" | "final" | "abstract" | "static"
            )
        }) {
            continue;
        }

        let name: String = masked[from..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        let open = from + name.len();
        if name.is_empty() || !masked[open..].starts_with('(') {
            continue;
        }

        let Some(close) = php::closing(masked, open) else {
            continue;
        };
        let Some(brace) = masked[close..class.close]
            .find(['{', ';'])
            .map(|i| close + i)
            .filter(|i| masked.as_bytes()[*i] == b'{')
        else {
            continue;
        };
        let Some(end) = php::closing(masked, brace) else {
            continue;
        };

        found.push(Method {
            line_start,
            name,
            is_static: modifiers.split_whitespace().any(|w| w == "static"),
            params: open + 1..close,
            body: brace..end + 1,
        });
        from = end;
    }

    found
}

fn indentation(text: &str, line_start: usize) -> &str {
    let line = &text[line_start..];
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |i| at + i + 1)
}

fn camel(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(chars).collect()
    })
}

fn whole_word_positions(haystack: &str, word: &str) -> Vec<usize> {
    let bytes = haystack.as_bytes();
    haystack
        .match_indices(word)
        .map(|(at, _)| at)
        .filter(|at| {
            !bytes
                .get(at + word.len())
                .is_some_and(|b| is_identifier(*b))
        })
        .collect()
}

/// Declaration identity does not depend on the narrower editable signature
/// grammar. An uneditable declaration must still reserve its PHP method name.
fn declares_method(masked: &str, name: &str) -> bool {
    let folded = masked.to_ascii_lowercase();
    folded.match_indices("function").any(|(at, _)| {
        if at > 0 && is_identifier(folded.as_bytes()[at - 1]) {
            return false;
        }
        let tail = &masked[at + "function".len()..];
        if !tail.as_bytes().first().is_some_and(u8::is_ascii_whitespace) {
            return false;
        }
        let tail = tail.trim_start().trim_start_matches('&').trim_start();
        let end = tail
            .bytes()
            .take_while(|b| is_identifier(*b) || *b >= 128)
            .count();
        tail[..end].eq_ignore_ascii_case(name)
    })
}

/// A property used by another method may be dynamically created rather than
/// declared. Promotion must not silently turn that property into a new typed
/// dependency. Unknown `$this->{$name}` accesses cannot prove name separation.
fn property_used(masked: &str, class: &Class, name: &str) -> bool {
    let body = &masked[class.open..class.close];
    whole_word_positions(body, "$this").into_iter().any(|at| {
        let rest = body[at + "$this".len()..].trim_start();
        let Some(member) = rest.strip_prefix("->").or_else(|| rest.strip_prefix("?->")) else {
            return false;
        };
        let member = member.trim_start();
        if member.starts_with(['{', '$']) {
            return true;
        }
        member.strip_prefix(name).is_some_and(|tail| {
            tail.as_bytes()
                .first()
                .is_none_or(|b| !is_identifier(*b) && *b < 128)
        })
    })
}

fn direct_model_source(text: &str, class: &Class) -> bool {
    class
        .parent
        .as_deref()
        .and_then(|parent| crate::eloquent_members::source_class_name(text, parent))
        .is_some_and(|parent| {
            [
                "Illuminate\\Database\\Eloquent\\Model",
                "Illuminate\\Database\\Eloquent\\Relations\\Pivot",
                "Illuminate\\Database\\Eloquent\\Relations\\MorphPivot",
                "Illuminate\\Foundation\\Auth\\User",
            ]
            .iter()
            .any(|known| parent.eq_ignore_ascii_case(known))
        })
}

/// Edits adding `private <ty> $<name>` to the constructor, creating it when the class has none.
fn constructor_edits(
    text: &str,
    masked: &str,
    class: &Class,
    found: &[Method],
    ty: &str,
    name: &str,
) -> Option<Vec<(Range<usize>, String)>> {
    if let Some(parent) = &class.parent {
        let base = "Illuminate\\Routing\\Controller";
        let imported = php::insert_use(text, base).filter(|import| import.text.is_empty());
        if !parent.trim_start_matches('\\').eq_ignore_ascii_case(base)
            && !imported.is_some_and(|import| import.short_name.eq_ignore_ascii_case(parent))
        {
            return None;
        }
    }

    let mut members = masked[class.open..class.close].to_owned();
    for method in found {
        let begin = method.line_start.saturating_sub(class.open);
        let end = method
            .body
            .end
            .saturating_sub(class.open)
            .min(members.len());
        if begin <= end {
            members.replace_range(begin..end, &" ".repeat(end - begin));
        }
    }
    if property_used(masked, class, name)
        || !whole_word_positions(&members, &format!("${name}")).is_empty()
        || !whole_word_positions(&members, "use").is_empty()
    {
        return None;
    }
    let parameter = format!("private {ty} ${name}");
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };

    if declares_method(&masked[class.open..class.close], "__construct")
        && !found
            .iter()
            .any(|m| m.name.eq_ignore_ascii_case("__construct"))
    {
        return None;
    }
    if let Some(constructor) = found
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case("__construct"))
    {
        let existing = &masked[constructor.params.clone()];
        if !whole_word_positions(existing, &format!("${name}")).is_empty() {
            return None;
        }

        let arguments = php::split(masked, constructor.params.clone());
        if arguments.iter().any(|range| {
            masked[range.clone()].contains('=') || masked[range.clone()].contains("...")
        }) {
            return None;
        }
        let Some(last) = arguments.last() else {
            return Some(vec![(constructor.params.clone(), parameter)]);
        };
        let end = php::trimmed(masked, last.clone()).end;
        let multiline = existing.contains('\n');
        let has_trailing_comma = masked[end..constructor.params.end]
            .trim_start()
            .starts_with(',');
        let indent = indentation(
            text,
            text[..last.start]
                .trim_end()
                .rfind('\n')
                .map_or(0, |i| i + 1),
        )
        .to_owned();
        let inserted = match (multiline, has_trailing_comma) {
            (true, true) => {
                return Some(vec![(
                    end + 1..end + 1,
                    format!("{eol}{indent}{parameter},"),
                )]);
            }
            (true, false) => format!(",{eol}{indent}{parameter}"),
            (false, _) => format!(", {parameter}"),
        };
        return Some(vec![(end..end, inserted)]);
    }

    let anchor = found.first()?;
    let mut insert_at = anchor.line_start;
    loop {
        let before = text[..insert_at].trim_end_matches(['\n', '\r']);
        let previous_start = before.rfind('\n').map_or(0, |i| i + 1);
        let previous = before[previous_start..].trim();
        if previous.ends_with("*/")
            || previous.starts_with('*')
            || previous.starts_with("/**")
            || previous.starts_with("#[")
        {
            insert_at = previous_start;
            continue;
        }
        break;
    }

    let indent = indentation(text, anchor.line_start);
    Some(vec![(
        insert_at..insert_at,
        format!(
            "{indent}public function __construct({parameter}){eol}{indent}{{{eol}{indent}}}{eol}{eol}"
        ),
    )])
}

#[cfg(test)]
fn rewrites(text: &str, at: usize) -> Vec<Rewrite> {
    rewrites_with_scope(text, at, true)
}

#[cfg(test)]
pub fn rewrites_with_scope(text: &str, at: usize, supports_scope_attribute: bool) -> Vec<Rewrite> {
    rewrites_with_scope_identity(text, at, supports_scope_attribute, false)
}

pub fn rewrites_with_scope_identity(
    text: &str,
    at: usize,
    supports_scope_attribute: bool,
    indexed_model: bool,
) -> Vec<Rewrite> {
    let Some(masked) = code(text) else {
        return Vec::new();
    };
    let Some(class) = class_of(&masked) else {
        return Vec::new();
    };
    let found = methods(&masked, &class);

    let mut rewrites =
        if supports_scope_attribute && (indexed_model || direct_model_source(text, &class)) {
            scope_attribute(text, &masked, &found, at)
        } else {
            Vec::new()
        };
    rewrites.extend(move_argument(text, &masked, &class, &found, at));
    rewrites.extend(move_matching_arguments(text, &masked, &class, &found, at));
    rewrites.extend(inject_facade(text, &masked, &class, &found, at));
    rewrites.extend(inject_facade_in_method(text, &masked, &class, &found, at));
    rewrites
}

fn scope_edit(
    text: &str,
    masked: &str,
    found: &[Method],
    method: &Method,
    attribute: &str,
) -> Option<(String, (Range<usize>, String))> {
    let rest = method
        .name
        .strip_prefix("scope")
        .filter(|r| r.chars().next().is_some_and(|c| c.is_ascii_uppercase()))?;
    let new_name = camel(rest);
    // PHP method identities are case insensitive. Withhold direct legacy
    // calls rather than leaving a now-renamed method reference broken.
    let folded = masked.to_ascii_lowercase();
    if declares_method(masked, &new_name)
        || found.iter().any(|m| m.name.eq_ignore_ascii_case(&new_name))
        || whole_word_positions(&folded, &method.name.to_ascii_lowercase()).len() > 1
    {
        return None;
    }

    let line = &text[method.line_start..line_end(text, method.line_start)];
    let name_at = method.line_start + line.find("function ")? + "function ".len();
    let indent = indentation(text, method.line_start);
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let visibility_end = method.line_start + line.find("function ")?;
    let modifiers = text[method.line_start..visibility_end]
        .trim()
        .replace("public", "protected");
    let modifiers = if modifiers.is_empty() {
        "protected".to_owned()
    } else {
        modifiers
    };
    let edit = (
        method.line_start..name_at + method.name.len(),
        format!("{indent}#[{attribute}]{eol}{indent}{modifiers} function {new_name}"),
    );

    Some((new_name, edit))
}

fn scope_attribute(text: &str, masked: &str, found: &[Method], at: usize) -> Vec<Rewrite> {
    let import = php::insert_use(text, "Illuminate\\Database\\Eloquent\\Attributes\\Scope");
    let Some(import) = import else {
        return Vec::new();
    };
    let with_import = |mut edits: Vec<(Range<usize>, String)>| {
        if !import.text.is_empty() {
            edits.insert(0, (import.offset..import.offset, import.text.clone()));
        }
        edits
    };

    let mut rewrites = Vec::new();
    let all: Vec<(String, (Range<usize>, String))> = found
        .iter()
        .filter_map(|m| scope_edit(text, masked, found, m, &import.short_name))
        .collect();

    if let Some(method) = found
        .iter()
        .find(|m| m.line_start <= at && at <= m.params.start)
        && let Some((new_name, edit)) = scope_edit(text, masked, found, method, &import.short_name)
    {
        rewrites.push(Rewrite {
            title: format!("Use #[Scope] on {new_name}()"),
            kind: "refactor.rewrite",
            edits: with_import(vec![edit]),
        });
    }

    if all.len() > 1 {
        let edits = with_import(all.into_iter().map(|(_, edit)| edit).collect());
        rewrites.push(Rewrite {
            title: "Use #[Scope] on all scope methods".into(),
            kind: "refactor.rewrite",
            edits,
        });
    }

    rewrites
}

fn move_argument(
    text: &str,
    masked: &str,
    class: &Class,
    found: &[Method],
    at: usize,
) -> Option<Rewrite> {
    let method = found.iter().find(|m| {
        m.params.start <= at && at <= m.params.end && !m.name.starts_with("__") && !m.is_static
    })?;
    let arguments = php::split(masked, method.params.clone());
    let (index, argument) = arguments
        .iter()
        .enumerate()
        .find(|(_, range)| range.start <= at && at <= range.end)?;
    let source = masked[argument.clone()].trim();

    let (ty, variable) = source.rsplit_once(char::is_whitespace)?;
    let name = variable.strip_prefix('$')?;
    let ty = ty.trim();
    let class_like = ty
        .trim_start_matches('\\')
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase());
    if !class_like
        || ty.contains(['|', '&', '?', '='])
        || ty.split_whitespace().count() > 1
        || name.is_empty()
        || !name.bytes().all(is_identifier)
    {
        return None;
    }

    let body = &masked[method.body.clone()];
    let variable = format!("${name}");
    let uses = whole_word_positions(body, &variable);
    if !safe_local_uses(text, masked, method, &variable) {
        return None;
    }
    if body.contains("compact(") || body.contains("extract(") || body.contains("$$") {
        return None;
    }
    for &position in &uses {
        let after = body[position + variable.len()..].trim_start();
        let before = body[..position].trim_end();
        let assigned =
            after.starts_with('=') && !after.starts_with("==") && !after.starts_with("=>");
        if assigned
            || before.ends_with(" as")
            || before.ends_with("static")
            || before.ends_with("global")
        {
            return None;
        }
    }
    if body.match_indices("use (").any(|(open, _)| {
        body[open..]
            .find(')')
            .is_some_and(|close| body[open..open + close].contains(&variable))
    }) {
        return None;
    }

    let mut edits = constructor_edits(text, masked, class, found, ty, name)?;
    let removal = if arguments.len() == 1 {
        method.params.clone()
    } else if index == 0 {
        let next = arguments[1].start;
        argument.start..next + (masked[next..].len() - masked[next..].trim_start().len())
    } else {
        arguments[index - 1].end..argument.end
    };
    edits.push((removal, String::new()));
    for position in uses {
        let start = method.body.start + position;
        edits.push((start..start + variable.len(), format!("$this->{name}")));
    }

    Some(Rewrite {
        title: format!("Move {variable} to a constructor property"),
        kind: "refactor.rewrite",
        edits,
    })
}

fn inject_facade(
    text: &str,
    masked: &str,
    class: &Class,
    found: &[Method],
    at: usize,
) -> Option<Rewrite> {
    let bytes = masked.as_bytes();
    let start = (0..at.min(bytes.len()))
        .rev()
        .take_while(|&i| is_identifier(bytes[i]))
        .last()
        .unwrap_or(at);
    let end = (at..bytes.len())
        .take_while(|&i| is_identifier(bytes[i]))
        .last()
        .map_or(at, |i| i + 1);
    let facade = &masked[start..end];
    let (_, fqcn) = FACADE_CLASSES.iter().find(|(name, _)| *name == facade)?;
    if !masked[end..].starts_with("::")
        || start > 0 && matches!(bytes[start - 1], b'\\' | b'$' | b':' | b'>')
    {
        return None;
    }

    let import_line = format!("use Illuminate\\Support\\Facades\\{facade};");
    if !masked.contains(&import_line) {
        return None;
    }

    let mut calls = Vec::new();
    for position in whole_word_positions(&masked[class.open..class.close], facade) {
        let position = class.open + position;
        let before_ok = position == 0
            || !matches!(bytes[position - 1], b'\\' | b'$' | b':' | b'>')
                && !is_identifier(bytes[position - 1]);
        if !before_ok || !masked[position + facade.len()..].starts_with("::") {
            continue;
        }

        let after = &masked[position + facade.len() + 2..];
        let name_length = after.bytes().take_while(|b| is_identifier(*b)).count();
        if name_length == 0
            || facade_only(&after[..name_length])
            || !after[name_length..].trim_start().starts_with('(')
        {
            return None;
        }

        let method = found.iter().find(|m| m.body.contains(&position))?;
        let body = &masked[method.body.clone()];
        if method.is_static
            || ["function", "fn", "class"]
                .iter()
                .any(|word| !whole_word_positions(body, word).is_empty())
        {
            return None;
        }
        calls.push(position);
    }
    if calls.is_empty() || masked.matches(&format!("{facade}::")).count() != calls.len() {
        return None;
    }

    let property = camel(facade);
    let import = php::insert_use(text, fqcn)?;
    let ty = import.short_name.clone();
    let mut edits = constructor_edits(text, masked, class, found, &ty, &property)?;
    if !import.text.is_empty() {
        edits.push((import.offset..import.offset, import.text));
    }

    for position in calls {
        edits.push((
            position..position + facade.len() + 2,
            format!("$this->{property}->"),
        ));
    }

    let import_at = masked.find(&import_line)?;
    let line_start = masked[..import_at].rfind('\n').map_or(0, |i| i + 1);
    edits.push((line_start..line_end(text, import_at), String::new()));

    Some(Rewrite {
        title: format!("Inject {ty} instead of the {facade} facade"),
        kind: "refactor.rewrite",
        edits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, rewrite: &Rewrite) -> String {
        let mut edits = rewrite.edits.clone();
        edits.sort_by_key(|(range, _)| std::cmp::Reverse((range.start, range.end)));
        let mut out = text.to_owned();
        for (range, replacement) in edits {
            out.replace_range(range, &replacement);
        }
        out
    }

    fn at(text: &str, marker: &str) -> usize {
        text.find(marker).unwrap() + 1
    }

    #[test]
    fn scope_method_becomes_an_attribute() {
        let text = "<?php\nnamespace App\\Models;\n\nuse Illuminate\\Database\\Eloquent\\Model;\n\nclass Post extends Model\n{\n    public function scopeActive($query)\n    {\n        return $query->where('a', 1);\n    }\n}\n";
        let found = rewrites(text, at(text, "scopeActive"));
        let rewrite = found
            .iter()
            .find(|r| r.title.starts_with("Use #[Scope]"))
            .unwrap();
        let out = apply(text, rewrite);
        assert!(
            out.contains("use Illuminate\\Database\\Eloquent\\Attributes\\Scope;"),
            "{out}"
        );
        assert!(
            out.contains("    #[Scope]\n    protected function active($query)"),
            "{out}"
        );
    }

    #[test]
    fn all_scope_methods_convert_together_with_one_import() {
        let text = "<?php\nnamespace App\\Models;\n\nuse Illuminate\\Database\\Eloquent\\Model;\n\nclass Post extends Model\n{\n    public function scopeActive($q) {}\n\n    public function scopeRecent($q) {}\n}\n";
        let found = rewrites(text, at(text, "scopeActive"));
        let all = found
            .iter()
            .find(|r| r.title == "Use #[Scope] on all scope methods")
            .unwrap();
        let out = apply(text, all);
        assert_eq!(out.matches("#[Scope]").count(), 2, "{out}");
        assert_eq!(out.matches("Attributes\\Scope;").count(), 1, "{out}");
        assert!(
            out.contains("function recent($q)") && out.contains("function active($q)"),
            "{out}"
        );
    }

    #[test]
    fn scope_is_left_alone_when_called_by_name_or_not_a_model() {
        let text = "<?php\nclass Post extends Model\n{\n    public function scopeActive($q) {}\n    public function a() { return $this->scopeActive(1); }\n}\n";
        assert!(rewrites(text, at(text, "scopeActive")).is_empty());
        let plain = "<?php\nclass Post\n{\n    public function scopeActive($q) {}\n}\n";
        assert!(rewrites(plain, at(plain, "scopeActive")).is_empty());
    }

    #[test]
    fn argument_moves_into_a_new_constructor() {
        let text = "<?php\nuse Illuminate\\Routing\\Controller;\nclass ReportController extends Controller\n{\n    public function index(Request $request, ReportService $service)\n    {\n        return $service->all();\n    }\n}\n";
        let found = rewrites(text, at(text, "ReportService"));
        let out = apply(
            text,
            found
                .iter()
                .find(|r| r.title.starts_with("Move $service"))
                .unwrap(),
        );
        assert_eq!(
            out,
            "<?php\nuse Illuminate\\Routing\\Controller;\nclass ReportController extends Controller\n{\n    public function __construct(private ReportService $service)\n    {\n    }\n\n    public function index(Request $request)\n    {\n        return $this->service->all();\n    }\n}\n"
        );
    }

    #[test]
    fn argument_joins_an_existing_constructor() {
        let text = "<?php\nclass A\n{\n    public function __construct(private Foo $foo)\n    {\n    }\n\n    public function run(Bar $bar)\n    {\n        $bar->go();\n    }\n}\n";
        let out = apply(
            text,
            rewrites(text, at(text, "Bar $bar"))
                .iter()
                .find(|r| r.title.starts_with("Move"))
                .unwrap(),
        );
        assert!(
            out.contains("__construct(private Foo $foo, private Bar $bar)"),
            "{out}"
        );
        assert!(out.contains("public function run()\n"), "{out}");
        assert!(out.contains("$this->bar->go();"), "{out}");
    }

    #[test]
    fn argument_stays_put_when_reassigned_or_class_has_a_foreign_parent() {
        let reassigned = "<?php\nclass A\n{\n    public function run(Bar $bar)\n    {\n        $bar = new Bar();\n    }\n}\n";
        assert!(rewrites(reassigned, at(reassigned, "Bar $bar")).is_empty());
        let parent = "<?php\nclass A extends Base\n{\n    public function run(Bar $bar)\n    {\n        $bar->go();\n    }\n}\n";
        assert!(rewrites(parent, at(parent, "Bar $bar")).is_empty());
        let scalar = "<?php\nclass A\n{\n    public function run(int $n, ?Bar $bar = null) {}\n}\n";
        assert!(
            rewrites(scalar, at(scalar, "int $n")).is_empty()
                && rewrites(scalar, at(scalar, "Bar $bar")).is_empty()
        );
    }

    #[test]
    fn facade_becomes_an_injected_property() {
        let text = "<?php\nnamespace App;\n\nuse Illuminate\\Support\\Facades\\Cache;\n\nclass A\n{\n    public function run()\n    {\n        return Cache::get('x');\n    }\n}\n";
        let found = rewrites(text, at(text, "Cache::get"));
        let out = apply(
            text,
            found
                .iter()
                .find(|r| r.title.starts_with("Inject"))
                .unwrap(),
        );
        assert!(
            out.contains("use Illuminate\\Cache\\CacheManager;"),
            "{out}"
        );
        assert!(!out.contains("Facades\\Cache"), "{out}");
        assert!(
            out.contains("public function __construct(private CacheManager $cache)"),
            "{out}"
        );
        assert!(out.contains("return $this->cache->get('x');"), "{out}");
    }

    #[test]
    fn facade_in_a_static_method_or_used_as_a_constant_is_left_alone() {
        let static_call = "<?php\nuse Illuminate\\Support\\Facades\\Cache;\nclass A\n{\n    public static function run()\n    {\n        return Cache::get('x');\n    }\n}\n";
        assert!(
            rewrites(static_call, at(static_call, "Cache::get"))
                .iter()
                .all(|r| !r.title.starts_with("Inject"))
        );
        let constant = "<?php\nuse Illuminate\\Support\\Facades\\Cache;\nclass A\n{\n    public function run()\n    {\n        return [Cache::get('x'), Cache::class];\n    }\n}\n";
        assert!(
            rewrites(constant, at(constant, "Cache::get"))
                .iter()
                .all(|r| !r.title.starts_with("Inject"))
        );
    }
}

fn safe_local_uses(text: &str, masked: &str, method: &Method, variable: &str) -> bool {
    let body = &masked[method.body.clone()];
    if [
        "function",
        "fn",
        "class",
        "global",
        "static",
        "eval",
        "extract",
        "compact",
        "get_defined_vars",
    ]
    .iter()
    .any(|word| !whole_word_positions(body, word).is_empty())
        || body.contains("$$")
    {
        return false;
    }
    // A variable mentioned in an interpolated string would need a different
    // syntactic replacement; keep that case out of this code-token transform.
    let comments = php::mask_comments(&text[method.body.clone()]);
    if whole_word_positions(&comments, variable).len() != whole_word_positions(body, variable).len()
    {
        return false;
    }
    for position in whole_word_positions(body, variable) {
        let before = body[..position].trim_end();
        let after = body[position + variable.len()..].trim_start();
        // Direct method/property access and a direct return preserve the local
        // object's identity. A bare call argument might be passed by reference;
        // mutation, closure captures and dynamic variable lookup are withheld.
        if before.ends_with('&')
            || before.ends_with("->")
            || before.ends_with("::")
            || before.ends_with('$')
        {
            return false;
        }
        if !after.starts_with("->") && !(before.ends_with("return") && after.starts_with(';')) {
            return false;
        }
    }
    true
}

fn move_matching_arguments(
    text: &str,
    masked: &str,
    class: &Class,
    found: &[Method],
    at: usize,
) -> Option<Rewrite> {
    let selected = found.iter().find(|method| method.params.contains(&at))?;
    let selected_range = php::split(masked, selected.params.clone())
        .into_iter()
        .find(|range| range.contains(&at))?;
    let signature = masked[selected_range].trim();
    let (_, variable) = signature.rsplit_once(char::is_whitespace)?;
    let mut edits = Vec::new();
    let mut count = 0;
    for method in found {
        for argument in php::split(masked, method.params.clone()) {
            if masked[argument.clone()].trim() != signature {
                continue;
            }
            let action = move_argument(
                text,
                masked,
                class,
                found,
                argument.start + masked[argument.clone()].len()
                    - masked[argument.clone()].trim_start().len(),
            )?;
            count += 1;
            for edit in action.edits {
                if !edits.contains(&edit) {
                    edits.push(edit);
                }
            }
        }
    }
    if count < 2 {
        return None;
    }
    Some(Rewrite {
        title: format!("Move {variable} to a constructor property in all matching methods"),
        kind: "refactor.rewrite",
        edits,
    })
}

fn facade_only(name: &str) -> bool {
    [
        "swap",
        "spy",
        "partialMock",
        "shouldReceive",
        "expects",
        "shouldHaveReceived",
        "shouldNotHaveReceived",
        "isFake",
        "resolved",
        "clearResolvedInstance",
        "clearResolvedInstances",
        "getFacadeRoot",
        "getFacadeApplication",
        "setFacadeApplication",
    ]
    .iter()
    .any(|known| name.eq_ignore_ascii_case(known))
}

fn dependency_class<'a>(facade: &str, calls: &[String], fallback: &'a str) -> &'a str {
    let all = |allowed: &[&str]| {
        calls
            .iter()
            .all(|name| allowed.iter().any(|known| name.eq_ignore_ascii_case(known)))
    };
    match facade {
        "Cache"
            if all(&[
                "get",
                "put",
                "has",
                "add",
                "increment",
                "decrement",
                "forever",
                "remember",
                "rememberForever",
                "sear",
                "forget",
                "pull",
                "getStore",
            ]) =>
        {
            "Illuminate\\Contracts\\Cache\\Repository"
        }
        "Log"
            if all(&[
                "emergency",
                "alert",
                "critical",
                "error",
                "warning",
                "notice",
                "info",
                "debug",
                "log",
            ]) =>
        {
            "Psr\\Log\\LoggerInterface"
        }
        "Config" if all(&["get", "has", "all", "set"]) => {
            "Illuminate\\Contracts\\Config\\Repository"
        }
        _ => fallback,
    }
}

/// Method-scoped injection preserves unrelated static facade uses elsewhere in
/// the class. Imported aliases and explicit facade FQNs share the same binding.
fn inject_facade_in_method(
    text: &str,
    masked: &str,
    class: &Class,
    found: &[Method],
    at: usize,
) -> Vec<Rewrite> {
    let Some(method) = found.iter().find(|method| {
        method.body.contains(&at) && !method.is_static && !method.name.starts_with("__")
    }) else {
        return Vec::new();
    };
    let body = &masked[method.body.clone()];
    if [
        "function",
        "fn",
        "class",
        "eval",
        "extract",
        "compact",
        "get_defined_vars",
    ]
    .iter()
    .any(|word| !whole_word_positions(body, word).is_empty())
        || body.contains("$$")
    {
        return Vec::new();
    }
    let bytes = masked.as_bytes();
    let start = (0..at.min(bytes.len()))
        .rev()
        .take_while(|&i| is_identifier(bytes[i]) || bytes[i] == b'\\')
        .last()
        .unwrap_or(at);
    let end = (at..bytes.len())
        .take_while(|&i| is_identifier(bytes[i]) || bytes[i] == b'\\')
        .last()
        .map_or(at, |i| i + 1);
    let Some(callee) = masked.get(start..end) else {
        return Vec::new();
    };
    let binding = FACADE_CLASSES.iter().find_map(|(facade, underlying)| {
        let qualified = format!("Illuminate\\Support\\Facades\\{facade}");
        let imported = php::insert_use(text, &qualified).filter(|import| import.text.is_empty());
        if callee == format!("\\{qualified}")
            || imported
                .as_ref()
                .is_some_and(|import| import.short_name == callee)
        {
            Some((*facade, *underlying))
        } else {
            None
        }
    });
    let Some((facade, underlying)) = binding else {
        return Vec::new();
    };
    let mut calls = Vec::new();
    let mut names = Vec::new();
    for (relative, _) in body.match_indices(callee) {
        let position = method.body.start + relative;
        if position > 0
            && (is_identifier(bytes[position - 1])
                || matches!(bytes[position - 1], b'\\' | b'$' | b':' | b'>'))
        {
            continue;
        }
        let rest = &masked[position + callee.len()..method.body.end];
        let trim = rest.len() - rest.trim_start().len();
        let Some(after) = rest.trim_start().strip_prefix("::") else {
            continue;
        };
        let member = after.trim_start();
        let length = member
            .bytes()
            .take_while(|byte| is_identifier(*byte))
            .count();
        if length == 0
            || facade_only(&member[..length])
            || !member[length..].trim_start().starts_with('(')
        {
            return Vec::new();
        }
        names.push(member[..length].to_owned());
        calls.push(position..position + callee.len() + trim + 2);
    }
    if !calls.iter().any(|range| range.contains(&at)) {
        return Vec::new();
    }
    let fqcn = dependency_class(facade, &names, underlying);
    let Some(import) = php::insert_use(text, fqcn) else {
        return Vec::new();
    };
    let ty = import.short_name.clone();
    let mut name = camel(facade);
    let bound = |name: &str| {
        !whole_word_positions(
            &masked[method.params.start..method.body.end],
            &format!("${name}"),
        )
        .is_empty()
    };
    if bound(&name) {
        name.push_str("Dependency");
    }
    if bound(&name) {
        return Vec::new();
    }
    let common = |receiver: &str| {
        let mut edits: Vec<_> = calls
            .iter()
            .map(|range| (range.clone(), format!("{receiver}->")))
            .collect();
        if !import.text.is_empty() {
            edits.push((import.offset..import.offset, import.text.clone()));
        }
        edits
    };
    let mut result = Vec::new();
    let mut parameters = php::split(masked, method.params.clone());
    // Appending a required dependency after defaults/variadics is not a safe
    // callable signature change. Ordinary required parameters are supported.
    if parameters.iter().all(|range| {
        !masked[range.clone()].contains(['=', '&']) && !masked[range.clone()].contains("...")
    }) {
        let mut edits = common(&format!("${name}"));
        let insertion = if let Some(last) = parameters.pop() {
            let end = php::trimmed(masked, last).end;
            let tail = &masked[end..method.params.end];
            if tail.trim_start().starts_with(',') {
                (end + 1..end + 1, format!(" {ty} ${name},"))
            } else {
                (end..end, format!(", {ty} ${name}"))
            }
        } else {
            (method.params.clone(), format!("{ty} ${name}"))
        };
        edits.push(insertion);
        result.push(Rewrite {
            title: format!(
                "Inject {ty} into {}() instead of the {facade} facade",
                method.name
            ),
            kind: "refactor.rewrite",
            edits,
        });
    }
    if let Some(mut constructor) = constructor_edits(text, masked, class, found, &ty, &name) {
        let mut edits = common(&format!("$this->{name}"));
        edits.append(&mut constructor);
        result.push(Rewrite {
            title: format!("Inject {ty} into the constructor for {}()", method.name),
            kind: "refactor.rewrite",
            edits,
        });
    }
    result
}

#[cfg(test)]
mod injection_tests {
    use super::*;
    fn apply(text: &str, action: &Rewrite) -> String {
        let mut edits = action.edits.clone();
        edits.sort_by_key(|(range, _)| std::cmp::Reverse((range.start, range.end)));
        for pair in edits.windows(2) {
            assert!(pair[1].0.end <= pair[0].0.start, "overlapping edits");
        }
        let mut result = text.to_owned();
        for (range, replacement) in edits {
            result.replace_range(range, &replacement);
        }
        result
    }
    fn source(import: &str, body: &str, parameters: &str) -> String {
        format!(
            "<?php\nnamespace App;\n{import}\nclass A\n{{\n    public function run({parameters})\n    {{\n        {body}\n    }}\n    public function other()\n    {{\n        return Cache::get('other');\n    }}\n}}\n"
        )
    }
    fn action<'a>(actions: &'a [Rewrite], marker: &str) -> &'a Rewrite {
        actions
            .iter()
            .find(|action| action.title.contains(marker))
            .unwrap_or_else(|| {
                panic!(
                    "missing {marker}: {:?}",
                    actions.iter().map(|a| &a.title).collect::<Vec<_>>()
                )
            })
    }
    #[test]
    fn facade_method_injection_preserves_other_methods_and_selects_a_contract() {
        let text = source(
            "use Illuminate\\Support\\Facades\\Cache;",
            "return Cache::get('x');",
            "Request $request",
        );
        let actions = rewrites(&text, text.find("Cache::get").unwrap() + 1);
        let output = apply(&text, action(&actions, "into run()"));
        assert!(
            output.contains("use Illuminate\\Contracts\\Cache\\Repository;"),
            "{output}"
        );
        assert!(
            output.contains("run(Request $request, Repository $cache)"),
            "{output}"
        );
        assert!(output.contains("return $cache->get('x');"), "{output}");
        assert!(output.contains("return Cache::get('other');"), "{output}");
        let constructor = apply(&text, action(&actions, "into the constructor for run()"));
        assert!(
            constructor.contains("__construct(private Repository $cache)"),
            "{constructor}"
        );
        assert!(
            constructor.contains("return $this->cache->get('x');"),
            "{constructor}"
        );
    }
    #[test]
    fn imported_alias_and_explicit_fqn_share_resolved_binding() {
        for (import, callee) in [
            ("use Illuminate\\Support\\Facades\\Cache as Store;", "Store"),
            ("", "\\Illuminate\\Support\\Facades\\Cache"),
        ] {
            let text = source(import, &format!("return {callee}::get('x');"), "");
            let actions = rewrites(&text, text.find(&format!("{callee}::get")).unwrap() + 1);
            let output = apply(&text, action(&actions, "into run()"));
            assert!(output.contains("run(Repository $cache)"), "{output}");
            assert!(output.contains("return $cache->get('x');"), "{output}");
        }
    }
    #[test]
    fn local_and_parameter_names_do_not_collide_with_injected_parameter() {
        let text = source(
            "use Illuminate\\Support\\Facades\\Cache;",
            "return Cache::get('x');",
            "Request $cache",
        );
        let actions = rewrites(&text, text.find("Cache::get").unwrap() + 1);
        let output = apply(&text, action(&actions, "into run()"));
        assert!(
            output.contains("run(Request $cache, Repository $cacheDependency)"),
            "{output}"
        );
        assert!(output.contains("$cacheDependency->get('x')"), "{output}");
    }
    #[test]
    fn constructor_injection_does_not_duplicate_declared_properties() {
        let text = source(
            "use Illuminate\\Support\\Facades\\Cache;",
            "return Cache::get('x');",
            "",
        )
        .replace("class A\n{", "class A\n{\n    private Repository $cache;");
        let actions = rewrites(&text, text.find("Cache::get").unwrap() + 1);
        assert!(
            actions
                .iter()
                .all(|action| !action.title.contains("into the constructor"))
        );
        assert!(
            actions
                .iter()
                .any(|action| action.title.contains("into run()"))
        );
    }
    #[test]
    fn facade_specific_magic_and_nested_scopes_are_not_rewritten() {
        for body in [
            "return Cache::shouldReceive('get');",
            "return [Cache::get('x'), Cache::class];",
            "return static function () { return Cache::get('x'); };",
            "return Cache::$method('x');",
        ] {
            let text = source("use Illuminate\\Support\\Facades\\Cache;", body, "");
            let actions = rewrites(&text, text.find("Cache::").unwrap() + 1);
            assert!(
                actions
                    .iter()
                    .all(|action| !action.title.starts_with("Inject")),
                "{body}"
            );
        }
    }
    #[test]
    fn matching_dependencies_move_with_one_constructor_edit() {
        let text = "<?php\nclass A\n{\n    public function run(Service $service)\n    {\n        return $service->run();\n    }\n    public function other(Service $service)\n    {\n        return $service->other();\n    }\n}\n";
        let actions = rewrites(text, text.find("Service $service").unwrap() + 1);
        let output = apply(text, action(&actions, "in all matching methods"));
        assert_eq!(
            output
                .matches("__construct(private Service $service)")
                .count(),
            1,
            "{output}"
        );
        assert!(
            output.contains("function run()") && output.contains("function other()"),
            "{output}"
        );
        assert!(
            output.contains("$this->service->run()") && output.contains("$this->service->other()"),
            "{output}"
        );
    }
    #[test]
    fn interpolated_strings_bare_reference_arguments_and_mutation_stay_local() {
        for body in [
            "return \"{$service->name}\";",
            "consume($service);",
            "$service++;",
            "$service += new Service();",
            "return fn () => $service->run();",
            "$alias =& $service;",
        ] {
            let text = source("", body, "Service $service");
            let actions = rewrites(&text, text.find("Service $service").unwrap() + 1);
            assert!(
                actions
                    .iter()
                    .all(|action| !action.title.starts_with("Move")),
                "{body}"
            );
        }
    }
    #[test]
    fn quoted_class_or_function_examples_cannot_manufacture_declarations() {
        let text = source(
            "",
            "return 'class Fake { public function forged(Foo $foo) {} }';",
            "Service $service",
        );
        let actions = rewrites(&text, text.find("forged").unwrap() + 1);
        assert!(actions.is_empty());
        let actions = rewrites(&text, text.find("Service $service").unwrap() + 1);
        assert!(
            actions
                .iter()
                .any(|action| action.title.starts_with("Move"))
        );
    }
    #[test]
    fn unknown_controller_parent_does_not_get_an_overriding_constructor() {
        let text = source(
            "use Illuminate\\Support\\Facades\\Cache;",
            "return Cache::get('x');",
            "",
        )
        .replace("class A\n{", "class A extends Controller\n{");
        let actions = rewrites(&text, text.find("Cache::get").unwrap() + 1);
        assert!(
            actions
                .iter()
                .all(|action| !action.title.contains("constructor")
                    && !action.title.starts_with("Inject CacheManager"))
        );
        assert!(
            actions
                .iter()
                .any(|action| action.title.contains("into run()"))
        );
    }

    #[test]
    fn optional_constructor_parameters_are_not_followed_by_required_dependencies() {
        let text = source(
            "use Illuminate\\Support\\Facades\\Cache;",
            "return Cache::get('x');",
            "",
        )
        .replace(
            "class A\n{",
            "class A\n{\n    public function __construct(string $name = 'default') {}\n",
        );
        let actions = rewrites(&text, text.find("Cache::get").unwrap() + 1);
        assert!(
            actions
                .iter()
                .all(|action| !action.title.contains("constructor"))
        );
    }
}

#[cfg(test)]
mod scope_property_safety_tests {
    use super::*;
    fn source(parent: &str, imports: &str) -> String {
        format!(
            "<?php\nnamespace App\\Models;\n{imports}\nclass Post extends {parent}\n{{\n    public function scopeActive($query)\n    {{\n        return $query->where('active', true);\n    }}\n}}\n"
        )
    }
    #[test]
    fn actual_eloquent_aliases_and_fqns_work_but_lookalike_bases_do_not() {
        for (parent, imports) in [
            (
                "Record",
                "use Illuminate\\Database\\Eloquent\\Model as Record;",
            ),
            ("\\Illuminate\\Database\\Eloquent\\Model", ""),
            (
                "PivotRecord",
                "use Illuminate\\Database\\Eloquent\\Relations\\Pivot as PivotRecord;",
            ),
        ] {
            let text = source(parent, imports);
            let at = text.find("scopeActive").unwrap() + 1;
            assert!(
                rewrites_with_scope_identity(&text, at, true, false)
                    .iter()
                    .any(|r| r.title.starts_with("Use #[Scope]")),
                "{text}"
            );
            assert!(
                rewrites_with_scope_identity(&text, at, false, true)
                    .iter()
                    .all(|r| !r.title.starts_with("Use #[Scope]"))
            );
        }
        for (parent, imports) in [
            ("Modelish", ""),
            ("Model", "use App\\Model;"),
            ("Model", ""),
            ("Model", "use Vendor\\Model;"),
        ] {
            let text = source(parent, imports);
            let at = text.find("scopeActive").unwrap() + 1;
            assert!(
                rewrites_with_scope_identity(&text, at, true, false)
                    .iter()
                    .all(|r| !r.title.starts_with("Use #[Scope]")),
                "{text}"
            );
        }
    }
    #[test]
    fn scope_conversion_refuses_case_variant_method_collisions_and_direct_calls() {
        let base = source("Model", "use Illuminate\\Database\\Eloquent\\Model;");
        for method in [
            "    public function Active($query) {}",
            "    public function Active ($query) {}",
            "    #[Example] public function Active($query) {}",
            "    public function other() { return $this->scopeACTIVE(null); }",
            "    public function other() { return $this -> ScopeActive (null); }",
            "    public function other() { return self::SCOPEACTIVE(null); }",
        ] {
            let text = base.replacen(
                "    public function scopeActive",
                &format!("{method}\n    public function scopeActive"),
                1,
            );
            let at = text.rfind("scopeActive").unwrap() + 1;
            assert!(
                rewrites_with_scope_identity(&text, at, true, false)
                    .iter()
                    .all(|r| !r.title.starts_with("Use #[Scope]")),
                "{text}"
            );
        }
        let text=base.replacen("    public function scopeActive","    public function other() { return 'scopeACTIVE'; }\n    public function scopeActive",1);
        assert!(
            rewrites_with_scope_identity(&text, text.find("scopeActive").unwrap() + 1, true, false)
                .iter()
                .any(|r| r.title.starts_with("Use #[Scope]"))
        );
    }

    #[test]
    #[ignore = "requires php executable; lints actual rendered Scope edits"]
    fn actual_scope_edits_lint_with_php() {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        for (parent, imports) in [
            (
                "Record",
                "use Illuminate\\Database\\Eloquent\\Model as Record;",
            ),
            ("\\Illuminate\\Database\\Eloquent\\Model", ""),
        ] {
            let text = source(parent, imports);
            let edits = rewrites_with_scope_identity(
                &text,
                text.find("scopeActive").unwrap() + 1,
                true,
                false,
            );
            let rewrite = edits
                .iter()
                .find(|r| r.title.starts_with("Use #[Scope]"))
                .unwrap();
            let mut ordered = rewrite.edits.clone();
            ordered.sort_by_key(|(r, _)| std::cmp::Reverse((r.start, r.end)));
            let mut rendered = text.clone();
            for (range, replacement) in ordered {
                rendered.replace_range(range, &replacement);
            }
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
                .write_all(rendered.as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{rendered}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[test]
    fn cached_custom_ancestry_is_proven_against_the_current_buffer_before_scope_conversion() {
        use openidea_plugin::{mock::MockHost, providers::Document};
        let host = MockHost::install();
        host.add_file("artisan", "<?php");
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        host.add_file("app/Models/BaseRecord.php","<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class BaseRecord extends Model {}");
        let text = source("BaseRecord", "");
        host.add_file("app/Models/Post.php", &text);
        let project = host.invocation().project().unwrap();
        let index = crate::Index::build(&project);
        let mut doc = Document {
            path: "/project/app/Models/Post.php".into(),
            language: "php".into(),
            version: 1,
            text: text.clone(),
        };
        let proof = crate::eloquent_members::scope_model_source(&index, &doc);
        assert!(proof);
        let at = text.find("scopeActive").unwrap() + 1;
        assert!(
            rewrites_with_scope_identity(&text, at, true, proof)
                .iter()
                .any(|r| r.title.starts_with("Use #[Scope]"))
        );
        doc.text = source("Modelish", "");
        assert!(!crate::eloquent_members::scope_model_source(&index, &doc));
        let at = doc.text.find("scopeActive").unwrap() + 1;
        assert!(
            rewrites_with_scope_identity(&doc.text, at, true, false)
                .iter()
                .all(|r| !r.title.starts_with("Use #[Scope]"))
        );
    }
    #[test]
    fn dynamic_properties_in_other_methods_are_never_retyped_by_constructor_injection() {
        for property in [
            "$this->cache = 'unrelated';",
            "$this -> cache = 'unrelated';",
            "$this?->cache;",
            "$this->{$name};",
            "$this->$name;",
        ] {
            let text = format!(
                "<?php\nnamespace App;\nuse Illuminate\\Support\\Facades\\Cache;\nclass A\n{{\n    public function run() {{return Cache::get('x');}}\n    public function other() {{{property}}}\n}}\n"
            );
            let actions = rewrites_with_scope_identity(
                &text,
                text.find("Cache::get").unwrap() + 1,
                false,
                false,
            );
            assert!(
                actions.iter().all(|r| !r.title.contains("constructor")
                    && !r.title.starts_with("Inject CacheManager")),
                "{property}"
            );
            assert!(
                actions.iter().any(|r| r.title.contains("into run()")),
                "{property}"
            );
        }
    }
    #[test]
    fn uneditable_constructor_declarations_reserve_their_case_insensitive_identity() {
        let base = "<?php\nnamespace App;\nuse Illuminate\\Support\\Facades\\Cache;\nclass A\n{\n    public function run() {return Cache::get('x');}\n}\n";
        for declaration in [
            "public function __construct () {}",
            "#[Example] public function __construct() {}",
            "public function __CONSTRUCT (string $value='existing') {}",
        ] {
            let text = base.replacen(
                "    public function run",
                &format!("    {declaration}\n    public function run"),
                1,
            );
            let edits = rewrites_with_scope_identity(
                &text,
                text.find("Cache::get").unwrap() + 1,
                false,
                false,
            );
            assert!(
                edits.iter().all(|r| !r.title.contains("constructor")
                    && !r.title.starts_with("Inject CacheManager")),
                "{text}"
            );
            assert!(edits.iter().any(|r| r.title.contains("into run()")));
        }
    }

    #[test]
    fn quoted_property_examples_do_not_block_a_fresh_dependency_property() {
        let text = "<?php\nnamespace App;\nuse Illuminate\\Support\\Facades\\Cache;\nclass A\n{\n    public function run() {return Cache::get('x');}\n    public function other() {return '$this->cache';}\n}\n";
        let actions =
            rewrites_with_scope_identity(text, text.find("Cache::get").unwrap() + 1, false, false);
        assert!(
            actions
                .iter()
                .any(|r| r.title.contains("into the constructor for run()"))
        );
    }
}
