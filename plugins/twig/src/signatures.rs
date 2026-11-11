//! Current physical macro signatures. No inferred PHP callable/type signatures.
use crate::*;
use openidea_plugin::language_features::{Parameter, ParameterLabel, Signature, SignatureHelp};
fn active(prefix: &str, parameters: &[String]) -> Option<(u32, bool)> {
    let mut stack = vec![];
    let (mut quote, mut escaped, mut start, mut count) = (None, false, 0usize, 0u32);
    for (i, c) in prefix.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '(' | '[' | '{' => stack.push(c),
            ')' | ']' | '}' => {
                if stack.pop()
                    != Some(match c {
                        ')' => '(',
                        ']' => '[',
                        _ => '{',
                    })
                {
                    return None;
                }
            }
            ',' if stack.is_empty() => {
                start = i + 1;
                count += 1;
            }
            _ => {}
        }
        if stack.len() > 64 {
            return None;
        }
    }
    let current = prefix[start..].trim();
    if let Some((name, _)) = current
        .split_once([':', '='])
        .filter(|(name, _)| macro_arguments::identifier(name.trim()))
    {
        return parameters
            .iter()
            .position(|p| p == name.trim())
            .map(|i| (i as u32, true));
    }
    (count < (parameters.len() as u32)).then_some((count, false))
}
pub fn help(doc: &Document, at: openidea_plugin::Position) -> Option<SignatureHelp> {
    let _scope = delimiters::Syntax::scope()?;
    if doc.language != "twig" || !doc.path.ends_with(".twig") || doc.text.len() > 128 << 10 {
        return None;
    }
    let offset = offset(&doc.text, at)?;
    let call = macro_arguments::call_at(&doc.text, offset, true)?;
    let (declaration, path) = resolved_macro(doc, call.call, call.receiver.as_deref(), &call.name)?;
    let parts = if declaration.arguments.trim().is_empty() {
        vec![]
    } else {
        macro_arguments::parts(&declaration.arguments)?
    };
    let mut names = vec![];
    for part in &parts {
        let name = part.split_once('=').map_or(*part, |p| p.0).trim();
        if !macro_arguments::identifier(name) || names.contains(&name.to_owned()) {
            return None;
        }
        names.push(name.to_owned());
    }
    let (parameter, named) = if names.is_empty() {
        (0, false)
    } else {
        active(&doc.text[call.open + 1..offset], &names)?
    };
    if named && !macro_arguments::supported(project_of(&doc.path).as_ref()) {
        return None;
    }
    Some(SignatureHelp {
        signatures: vec![Signature {
            label: format!("{}({})", declaration.name, declaration.arguments),
            documentation: Some(format!(
                "Declared Twig macro in {path}; current-source parameter signature."
            )),
            parameters: parts
                .into_iter()
                .map(|p| Parameter {
                    label: ParameterLabel::Text(p.trim().into()),
                    documentation: None,
                })
                .collect(),
        }],
        active_signature: 0,
        active_parameter: parameter,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_old_twig_keeps_positional_signatures_but_withholds_named_argument_contract() {
        let host = openidea_plugin::mock::MockHost::install();
        host.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"twig/twig","version":"v3.14.2"}]}"#,
        );
        for (call, supported) in [("_self.badge(", true), ("_self.badge(value: 'part", false)] {
            let text =
                format!("{{% macro badge(label, value = '') %}}{{% endmacro %}} {{{{ {call}");
            let doc = Document {
                path: "/project/a.twig".into(),
                language: "twig".into(),
                text: text.clone(),
                version: 3,
            };
            assert_eq!(help(&doc, position(&text, text.len())).is_some(), supported);
        }
    }
    #[test]
    fn current_macro_signature_tracks_positional_named_and_partial_string_arguments() {
        let _host = openidea_plugin::mock::MockHost::install();
        for (call, want) in [
            ("_self.badge(", 0),
            ("_self.badge('x', ", 1),
            ("_self.badge(value: 'part", 1),
        ] {
            let text =
                format!("😀 {{% macro badge(label, value = '') %}}{{% endmacro %}} {{{{ {call}");
            let doc = Document {
                path: "/project/a.twig".into(),
                language: "twig".into(),
                text: text.clone(),
                version: 3,
            };
            let found = help(&doc, position(&text, text.len())).unwrap();
            assert_eq!(found.active_parameter, want);
            assert_eq!(found.signatures[0].parameters.len(), 2);
        }
    }
}
/// Exact parameter declaration within a known macro signature.
fn parameter_range(text: &str, declaration: &navigate::Macro, name: &str) -> Option<Range<usize>> {
    let tag = lex(text).tags.into_iter().find(|t| {
        t.name == "macro"
            && t.range.start <= declaration.name_range.start
            && declaration.name_range.end <= t.range.end
    })?;
    let open =
        declaration.name_range.end + text[declaration.name_range.end..tag.range.end].find('(')? + 1;
    let mut at = open;
    let mut found = None;
    for part in macro_arguments::parts(&declaration.arguments)? {
        let left = part.split_once('=').map_or(part, |p| p.0);
        let param = left.trim();
        if !macro_arguments::identifier(param) {
            return None;
        }
        if param == name {
            if found.is_some() {
                return None;
            }
            let start = at + left.len() - left.trim_start().len();
            found = Some(start..start + param.len());
        }
        at += part.len() + 1;
    }
    found
}
pub fn definition(doc: &Document, at: openidea_plugin::Position) -> Option<Location> {
    let offset = offset(&doc.text, at)?;
    let parsed = lex(&doc.text);
    let token = parsed
        .tokens
        .iter()
        .find(|t| t.range.start <= offset && offset < t.range.end)?;
    let name = &doc.text[token.range.clone()];
    if !macro_arguments::identifier(name) {
        return None;
    }
    if token.kind == Highlight::Property {
        let call = macro_arguments::slot(&doc.text, token.range.end)?;
        let (declaration, path) =
            resolved_macro(doc, call.call, call.receiver.as_deref(), &call.name)?;
        let text = if path == doc.path {
            doc.text.clone()
        } else {
            let project = project_of(&path)?;
            if !same_owner(&project.root().ok()?, &path)
                || project.stat(&path).ok()?.size > 128 << 10
            {
                return None;
            }
            project.read_file(&path).ok()?
        };
        let found: Vec<_> = navigate::macros(&text)
            .into_iter()
            .filter(|m| m.name == declaration.name)
            .collect();
        if found.len() != 1 {
            return None;
        }
        let range = parameter_range(&text, &found[0], name)?;
        return Some(Location {
            path,
            range: EditorRange::new(position(&text, range.start), position(&text, range.end)),
        });
    }
    if token.kind != Highlight::Variable {
        return None;
    }
    let mut macros = vec![];
    for tag in &parsed.tags {
        if tag.range.start > token.range.start {
            break;
        }
        if tag.name == "macro" {
            macros.push(tag);
        } else if tag.name == "endmacro" {
            macros.pop();
        }
    }
    if macros.len() != 1 {
        return None;
    }
    let owner = macros[0];
    let end = parsed
        .tags
        .iter()
        .find(|t| t.range.start >= owner.range.end && t.name == "endmacro")?
        .range
        .start;
    if parsed
        .tags
        .iter()
        .filter(|t| {
            owner.range.end <= t.range.start
                && t.range.end <= end
                && matches!(t.name.as_str(), "set" | "for")
        })
        .any(|tag| {
            parsed.tokens.iter().any(|t| {
                tag.range.start <= t.range.start
                    && t.range.end <= tag.range.end
                    && &doc.text[t.range.clone()] == name
            })
        })
    {
        return None;
    }
    let declaration = navigate::macros(&doc.text)
        .into_iter()
        .find(|m| owner.range.start <= m.name_range.start && m.name_range.end <= owner.range.end)?;
    let range = parameter_range(&doc.text, &declaration, name)?;
    Some(Location {
        path: doc.path.clone(),
        range: EditorRange::new(
            position(&doc.text, range.start),
            position(&doc.text, range.end),
        ),
    })
}
#[cfg(test)]
mod navigation_tests {
    use super::*;
    #[test]
    fn parameter_declarations_resolve_current_body_reads_and_imported_named_arguments() {
        let host = openidea_plugin::mock::MockHost::install();
        let source = "😀 {% macro badge(label, value = '') %}{{ label }}{% endmacro %}";
        let doc = Document {
            path: "/project/templates/a.twig".into(),
            language: "twig".into(),
            text: source.into(),
            version: 1,
        };
        let target =
            Twig::default().definition(&doc, position(source, source.rfind("label").unwrap() + 2));
        assert_eq!(target.len(), 1);
        assert_eq!(
            target[0].range.start,
            position(source, source.find("label").unwrap())
        );
        host.add_file("templates/forms.twig", source);
        let source = "{% import 'forms.twig' as forms %}{{ forms.badge(value: 'x') }}";
        let doc = Document {
            path: "/project/templates/a.twig".into(),
            language: "twig".into(),
            text: source.into(),
            version: 1,
        };
        let target =
            Twig::default().definition(&doc, position(source, source.find("value").unwrap() + 2));
        assert_eq!(target.len(), 1);
        assert_eq!(target[0].path, "/project/templates/forms.twig");
        let helper = host
            .invocation()
            .project()
            .unwrap()
            .read_file("templates/forms.twig")
            .unwrap();
        assert_eq!(
            &helper[offset(&helper, target[0].range.start).unwrap()
                ..offset(&helper, target[0].range.end).unwrap()],
            "value"
        );
    }
}
