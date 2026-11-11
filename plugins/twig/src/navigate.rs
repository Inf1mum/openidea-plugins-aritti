//! Macros and templates: what `{% import %}` and `{% from %}` bring in, the macros a template defines, the
//! template a string names, and the filters, functions and tests a project registers in PHP.

use std::ops::Range;

#[derive(Debug, PartialEq, Eq)]
pub struct Macro {
    pub name: String,
    pub arguments: String,
    pub name_range: Range<usize>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Import {
    /// `{% import 'x' as forms %}`.
    Alias { template: String, alias: String },
    /// `{% from 'x' import a, b as c %}`: (name in the template, name used here).
    Names { template: String, names: Vec<(String, String)> },
}

fn tag_blocks(text: &str) -> impl Iterator<Item = (Range<usize>, &str)> {
    let parsed = if text.len() <= 128 << 10 { crate::lex(text) } else { Default::default() };
    let tags = if parsed.tags.len() <= 4096 { parsed.tags } else { Vec::new() };
    tags.into_iter().filter_map(move |tag| {
        let range = tag.range;
        let syntax=crate::delimiters::Syntax::current();
        let raw = syntax.tag_body(text.get(range.clone())?)?;
        let body = raw.trim_start_matches(['-', '~']).trim_end_matches(['-', '~']).trim();
        Some((range, body))
    })
}

/// Recognized Twig code rather than HTML, comments, raw bodies or string data.
/// Literal arguments use allow_string; identifiers must be separate lexer tokens.
pub fn code_at(text: &str, offset: usize, allow_string: bool) -> bool {
    if text.len() > 128 << 10 || !text.is_char_boundary(offset) {
        return false;
    }
    let parsed = crate::lex(text);
    let mut inside = false;
    let mut boundary=0usize;
    for token in parsed.tokens {
        if token.range.start > offset {
            break;
        }
        if let Some((_,open))=parsed.boundaries.get(boundary).filter(|(range,_)|*range==token.range) {
            boundary+=1;
            if !open && token.range.start<=offset && offset<token.range.end {return false;}
            inside=*open;
        }
        if token.range.start <= offset && offset < token.range.end {
            return inside
                && (allow_string || token.kind != openidea_plugin::syntax::Highlight::String)
                && token.kind != openidea_plugin::syntax::Highlight::Comment;
        }
        if token.range.start > offset {
            break;
        }
    }
    inside
}

/// Select an explicit macro binding within its own lexical container. Unknown
/// control-flow bindings and duplicate same-scope declarations are ambiguous.
pub fn binding(text: &str, offset: usize, receiver: Option<&str>, word: &str) -> Option<(String, String)> {
    if !code_at(text, offset, false) {
        return None;
    }
    let parsed = crate::lex(text);
    if parsed.tags.len() > 4096 || !parsed.unterminated.is_empty() {
        return None;
    }
    let mut stack: Vec<(&str, usize)> = Vec::new();
    let mut current = Vec::new();
    let mut candidates = Vec::new();
    let mut captured = false;
    let known = [
        "apply",
        "autoescape",
        "block",
        "cache",
        "embed",
        "for",
        "if",
        "macro",
        "sandbox",
        "set",
        "verbatim",
        "raw",
        "filter",
        "spaceless",
        "with",
    ];
    for tag in &parsed.tags {
        if !captured && tag.range.start >= offset {
            current = stack.clone();
            captured = true;
        }
        if let Some(end) = tag.name.strip_prefix("end").filter(|n| known.contains(n)) {
            if stack.pop().is_none_or(|(name, _)| name != end) {
                return None;
            }
        } else if matches!(tag.name.as_str(), "import" | "from") {
            let body = &text[tag.range.clone()];
            let imports = imports(body);
            // An unparseable potentially shadowing declaration makes the current
            // document unresolved, rather than falling back to an outer import.
            if imports.is_empty() {
                return None;
            }
            for import in imports {
                let value = match import {
                    Import::Alias { template, alias } if receiver == Some(alias.as_str()) => Some((template, word.to_owned())),
                    Import::Names { template, names } if receiver.is_none() => {
                        names.into_iter().find(|(_, alias)| alias == word).map(|(name, _)| (template, name))
                    }
                    _ => None,
                };
                if let Some(value) = value.filter(|_| tag.range.end <= offset) {
                    // Imports register bindings as encountered; a later declaration
                    // cannot establish the earlier expression's macro owner.
                    candidates.push((stack.clone(), value));
                }
            }
        } else if tag.paired && known.contains(&tag.name.as_str()) {
            stack.push((&tag.name, tag.range.start));
        } else if !known.contains(&tag.name.as_str())
            && !["else", "elseif", "extends", "include", "use", "do", "flush", "deprecated", "types"].contains(&tag.name.as_str())
        {
            return None;
        }
    }
    if !captured {
        current = stack.clone();
    }
    if !stack.is_empty() {
        return None;
    }
    let own = |s: &Vec<(&str, usize)>| s.iter().rev().find(|(n, _)| matches!(*n, "block" | "macro" | "embed")).map(|(_, id)| *id);
    let owner = own(&current);
    let mut local = Vec::new();
    let mut outer = Vec::new();
    for (scope, value) in candidates {
        let scope_owner = own(&scope);
        if scope_owner == owner {
            if scope.iter().any(|(n, _)| !matches!(*n, "block" | "macro" | "embed")) {
                return None;
            }
            local.push(value);
        } else if scope_owner.is_none() && !current.iter().any(|(n, _)| matches!(*n, "macro" | "embed")) {
            if !scope.is_empty() {
                return None;
            }
            outer.push(value);
        }
    }
    let chosen = if local.is_empty() { outer } else { local };
    (chosen.len() == 1).then(|| chosen.into_iter().next().unwrap())
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 128
}

fn quoted(rest: &str) -> Option<(&str, &str)> {
    let rest = rest.trim_start();
    let quote = rest.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
    let inner = &rest[1..];
    let end = inner.find(quote)?;
    if inner[..end].contains('\\') || (quote == '"' && inner[..end].contains("#{")) {
        return None;
    }
    Some((&inner[..end], &inner[end + 1..]))
}

pub fn macros(text: &str) -> Vec<Macro> {
    tag_blocks(text)
        .filter_map(|(range, body)| {
            let rest = body.strip_prefix("macro")?;
            if !rest.starts_with(char::is_whitespace) {
                return None;
            }

            let rest = rest.trim_start();
            let length = rest.bytes().take_while(|b| is_word(*b)).count();
            if length == 0 {
                return None;
            }

            let after = rest[length..].trim_start();
            let arguments = after.strip_prefix('(').and_then(|a| a.rfind(')').map(|end| a[..end].trim().to_owned())).unwrap_or_default();
            let after_keyword = range.start + text[range.clone()].find("macro")? + "macro".len();
            let start = after_keyword + text[after_keyword..range.end].find(&rest[..length])?;

            Some(Macro { name: rest[..length].to_owned(), arguments, name_range: start..start + length })
        })
        .collect()
}

pub fn imports(text: &str) -> Vec<Import> {
    tag_blocks(text)
        .filter_map(|(_, body)| {
            if let Some(rest) = body.strip_prefix("import").filter(|r| r.starts_with(char::is_whitespace)) {
                let (template, rest) = template_and_rest(rest)?;
                let alias = rest.trim().strip_prefix("as")?.strip_prefix(char::is_whitespace)?.trim();
                return identifier(alias).then(|| Import::Alias { template, alias: alias.into() });
            }

            let rest = body.strip_prefix("from").filter(|r| r.starts_with(char::is_whitespace))?;
            let (template, rest) = template_and_rest(rest)?;
            let list = rest.trim().strip_prefix("import")?.strip_prefix(char::is_whitespace)?;
            let names: Option<Vec<_>> = list
                .split(',')
                .map(|item| {
                    let words: Vec<_> = item.split_whitespace().collect();
                    match words.as_slice() {
                        [name] if identifier(name) => Some((name.to_string(), name.to_string())),
                        [name, "as", alias] if identifier(name) && identifier(alias) => Some((name.to_string(), alias.to_string())),
                        _ => None,
                    }
                })
                .collect();
            let names = names?;
            let mut seen = std::collections::HashSet::new();
            if !names.iter().all(|(_, alias)| seen.insert(alias)) {
                return None;
            }
            Some(Import::Names { template, names })
        })
        .collect()
}

fn identifier(value: &str) -> bool {
    value.as_bytes().first().is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_' || *b >= 128) && value.bytes().all(is_word)
}
fn template_and_rest(rest: &str) -> Option<(String, &str)> {
    if let Some((template, after)) = quoted(rest) {
        return Some((template.to_owned(), after));
    }

    let rest = rest.trim_start();
    let after = rest.strip_prefix("_self")?;
    Some(("_self".to_owned(), after))
}

/// The template a string literal at `offset` names (`extends`, `include`, `embed`, `import`, `from`, `use`, `include()`, `source()`),
/// and the literal's range without the quotes.
pub fn template_at(text: &str, offset: usize) -> Option<(String, Range<usize>)> {
    if !code_at(text, offset, true) {
        return None;
    }
    let parsed = crate::lex(text);
    let token = parsed
        .tokens
        .iter()
        .find(|t| t.kind == openidea_plugin::syntax::Highlight::String && t.range.start < offset && offset < t.range.end)?;
    let raw = &text[token.range.clone()];
    let quote = raw.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
    let inner = raw.strip_prefix(quote)?.strip_suffix(quote)?;
    if inner.is_empty() || inner.contains('\\') || (quote == '"' && inner.contains("#{")) {
        return None;
    }
    let before = text[..token.range.start].trim_end();
    let introducer = ["extends", "include", "embed", "import", "from", "use", "include(", "source(", "include (", "source ("]
        .into_iter().find(|word| before.ends_with(word) && before[..before.len()-word.len()].bytes().next_back().is_none_or(|b| !is_word(b) && b != b'.'))?;
    let after = text[token.range.end..].trim_start();
    let keyword = |word: &str| after.strip_prefix(word).is_some_and(|rest| rest.starts_with(char::is_whitespace));
    let closed = after.trim_start_matches(['-','~']).starts_with(&crate::delimiters::Syntax::current().values[1]);
    let complete = if introducer.ends_with('(') {
        after.starts_with(')') || after.starts_with(',')
    } else {
        match introducer {
            "extends" => closed,
            "include" | "embed" => closed || keyword("with") || keyword("ignore") || keyword("only"),
            "import" => keyword("as"),
            "from" => keyword("import"),
            "use" => closed || keyword("with"),
            _ => false,
        }
    };
    complete.then(|| (inner.into(), token.range.start + 1..token.range.end - 1))
}

/// The word around `offset` and the identifier before a `.` in front of it.
pub fn dotted_at(text: &str, offset: usize) -> Option<(Option<String>, String, Range<usize>)> {
    let bytes = text.as_bytes();
    let offset = offset.min(bytes.len());
    let start = (0..offset).rev().take_while(|&i| is_word(bytes[i])).last().unwrap_or(offset);
    let end = (offset..bytes.len()).take_while(|&i| is_word(bytes[i])).last().map_or(offset, |i| i + 1);
    if start == end && !text[..start].ends_with('.') {
        return None;
    }

    let receiver = text[..start].strip_suffix('.').map(|before| {
        let length = before.bytes().rev().take_while(|b| is_word(*b)).count();
        before[before.len() - length..].to_owned()
    });

    Some((receiver.filter(|r| !r.is_empty()), text[start..end].to_owned(), start..end))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtensionKind {
    Filter,
    Function,
    Test,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extension {
    pub kind: ExtensionKind,
    pub name: String,
    /// Byte range of the name literal.
    pub range: Range<usize>,
}

const REGISTRATIONS: &[(&str, ExtensionKind)] = &[
    ("TwigFilter(", ExtensionKind::Filter),
    ("SimpleFilter(", ExtensionKind::Filter),
    ("TwigFunction(", ExtensionKind::Function),
    ("SimpleFunction(", ExtensionKind::Function),
    ("TwigTest(", ExtensionKind::Test),
    ("SimpleTest(", ExtensionKind::Test),
];

/// Bounded lexical PHP admission only. Unsupported heredoc/nowdoc source is
/// withheld wholesale rather than scanning its literal body as registrations.
fn php_code_mask(php: &str) -> Vec<bool> {
    let bytes=php.as_bytes(); let mut mask=vec![false;bytes.len()];
    let (mut i,mut code)=(0,false);
    while i<bytes.len() {
        if !code {
            if bytes[i..].starts_with(b"<?php") { code=true;i+=5; }
            else if bytes[i..].starts_with(b"<?=") {code=true;i+=3;}
            else {i+=1;}
            continue;
        }
        if bytes[i..].starts_with(b"<<<") {return vec![false;bytes.len()];}
        if bytes[i..].starts_with(b"?>") {code=false;i+=2;continue;}
        if bytes[i..].starts_with(b"/*") {
            i+=2; while i<bytes.len()&&!bytes[i..].starts_with(b"*/") {i+=1;} i=(i+2).min(bytes.len());continue;
        }
        if bytes[i..].starts_with(b"//") || bytes[i]==b'#' && !bytes[i..].starts_with(b"#[") {
            while i<bytes.len()&&bytes[i]!=b'\n'&&!bytes[i..].starts_with(b"?>") {i+=1;} continue;
        }
        if matches!(bytes[i],b'\''|b'"'|b'`') {
            let quote=bytes[i];i+=1;
            while i<bytes.len() {if bytes[i]==b'\\' {i=(i+2).min(bytes.len());} else if bytes[i]==quote {i+=1;break;} else {i+=1;}}
            continue;
        }
        mask[i]=true;i+=1;
    }
    mask
}

/// `new TwigFilter('name', …)`, `new TwigFunction('name', …)`, `new TwigTest('name', …)` and the `#[AsTwig…('name')]` attributes of a PHP file.
pub fn extensions(php: &str) -> Vec<Extension> {
    if php.len() > 128 << 10 || !php.contains("Twig") {
        return Vec::new();
    }

    let code=php_code_mask(php);
    let mut found = Vec::new();
    for (marker, kind) in REGISTRATIONS {
        for (at, _) in php.match_indices(marker) {
            if !code.get(at).copied().unwrap_or(false) {continue;}
            let mut start=at;
            while start>0 && (is_word(php.as_bytes()[start-1]) || php.as_bytes()[start-1]==b'\\') {start-=1;}
            let class=&php[start..at+marker.len()-1];
            let simple=class.rsplit('\\').next().unwrap_or(class);
            let prefix=php[..start].trim_end();
            let base=&marker[..marker.len()-1];
            let qualified=class.trim_start_matches('\\');
            let constructor_name=(simple==base && (!qualified.contains('\\') || qualified==format!("Twig\\{base}"))) || qualified==format!("Twig_{base}");
            let constructor=constructor_name && prefix.strip_suffix("new").is_some_and(|p|p.bytes().next_back().is_none_or(|b|!is_word(b)));
            let attribute=simple==format!("As{base}") && (!qualified.contains('\\') || qualified==format!("Twig\\Attribute\\As{base}")) && prefix.ends_with("#[");
            if !constructor&&!attribute {continue;}
            let rest = &php[at + marker.len()..];
            let Some((name, after)) = quoted(rest) else { continue };
            if !after.trim_start().starts_with([',',')']) {continue;}
            let literal = rest.trim_start();
            let start = php.len() - literal.len() + 1;
            let valid = !name.is_empty() && name.bytes().all(|b| is_word(b) || b == b'*');
            if valid {
                found.push(Extension { kind: *kind, name: name.to_owned(), range: start..start + name.len() });
            }
        }
    }

    found.sort_by_key(|e| e.range.start);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_macros_with_their_arguments() {
        let text = "{% macro input(name, value = '') %}x{% endmacro %}\n{%- macro bare -%}y{%- endmacro -%}";
        let found = macros(text);
        assert_eq!(
            found.iter().map(|m| (m.name.as_str(), m.arguments.as_str())).collect::<Vec<_>>(),
            [("input", "name, value = ''"), ("bare", "")]
        );
        assert_eq!(&text[found[0].name_range.clone()], "input");
    }

    #[test]
    fn reads_import_and_from() {
        let text = "{% import 'forms.twig' as forms %}{% import _self as me %}{% from \"m/x.twig\" import a, b as c %}";
        assert_eq!(
            imports(text),
            [
                Import::Alias { template: "forms.twig".into(), alias: "forms".into() },
                Import::Alias { template: "_self".into(), alias: "me".into() },
                Import::Names { template: "m/x.twig".into(), names: vec![("a".into(), "a".into()), ("b".into(), "c".into())] },
            ]
        );
    }

    #[test]
    fn finds_the_template_name_under_the_caret() {
        let text = "{% extends 'base.html.twig' %}{{ include('part/a.twig') }}{{ 'plain' }}";
        let at = text.find("base").unwrap() + 2;
        assert_eq!(template_at(text, at).unwrap().0, "base.html.twig");
        let at = text.find("part").unwrap() + 1;
        assert_eq!(template_at(text, at).unwrap().0, "part/a.twig");
        assert!(template_at(text, text.find("plain").unwrap() + 1).is_none());
    }

    #[test]
    fn finds_the_receiver_of_a_dotted_name() {
        let text = "{{ forms.input(1) }}";
        let (receiver, word, range) = dotted_at(text, text.find("input").unwrap() + 2).unwrap();
        assert_eq!((receiver.as_deref(), word.as_str(), &text[range]), (Some("forms"), "input", "input"));
        assert_eq!(dotted_at("{{ x }}", 3).unwrap().0, None);
        assert_eq!(dotted_at("{{ forms. }}", 9).unwrap().0.as_deref(), Some("forms"));
        assert!(dotted_at("{{  }}", 3).is_none());
    }

    #[test]
    fn extension_anchors_require_php_code_and_complete_literal_registration() {
        let php = r#"outside TwigFilter('html') <?php
// new TwigFilter('comment')
/* new TwigFilter('block') */
$example = "new TwigFilter('quoted')";
TwigFilter('function'); new OtherTwigFilter('suffix'); new \Foreign\TwigFilter('foreign');
new TwigFilter('prefix' . $suffix);
new TwigFilter('actual', fn() => 1);
?> new TwigFilter('tail')"#;
        let found:Vec<_>=extensions(php).into_iter().map(|e|e.name).collect();
        assert_eq!(found,["actual"]);
        assert!(extensions("<?php $x = <<<'TXT'\nnew TwigFilter('example')\nTXT;\n").is_empty());
    }

    #[test]
    fn reads_registered_filters_functions_and_tests() {
        let php = "<?php\nuse Twig\\TwigFilter;\nclass E {\n  function f() { return [new TwigFilter('money', [$this, 'm']), new \\Twig\\TwigFunction(\"asset_url\", fn() => 1), new TwigTest('big', f())]; }\n  #[AsTwigFilter('shout')] function s() {}\n}\n";
        let found: Vec<_> = extensions(php).into_iter().map(|e| (e.kind, e.name)).collect();
        assert_eq!(
            found,
            [
                (ExtensionKind::Filter, "money".to_owned()),
                (ExtensionKind::Function, "asset_url".to_owned()),
                (ExtensionKind::Test, "big".to_owned()),
                (ExtensionKind::Filter, "shout".to_owned()),
            ]
        );
        assert!(extensions("<?php new Other('x');").is_empty());
    }
    #[test]
    fn import_grammar_and_context_reject_partial_dynamic_and_duplicate_aliases() {
        for source in [
            "{% import 'x' asf %}",
            "{% from 'x' importfoo %}",
            "{% from 'x' import a as same,b as same %}",
            "{% import 'x' as f extra %}",
            "{% import \"#{name}\" as f %}",
            "{# {% import 'x' as f %} #}",
            "{% verbatim %}{% import 'x' as f %}{% endverbatim %}",
        ] {
            assert!(imports(source).is_empty(), "{source}");
        }
        assert!(binding("{% import 'x' as f %}{% custom %}{{ f.x() }}", 39, Some("f"), "x").is_none());
        let source = "{{ 'include(\"fake.twig\")' }}";
        assert!(template_at(source, source.find("fake").unwrap() + 2).is_none());
    }
}
