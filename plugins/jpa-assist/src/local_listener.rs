//! Exact same-source nested listener attachment only; no external role graph.
use crate::{
    attribute_info,
    checks::Finding,
    live,
    model::{self, Kind},
    setting,
};
use openidea_plugin::{Severity, host, providers::Document};

struct Listener {
    masked: String,
    owner: String,
    name: String,
    open: usize,
    close: usize,
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().enumerate().all(|(i, b)| {
            b.is_ascii_alphabetic() || b == b'_' || b == b'$' || i > 0 && b.is_ascii_digit()
        })
}
fn context(doc: &Document) -> Option<Listener> {
    if doc.language != "java"
        || !doc.path.ends_with(".java")
        || doc.text.len() > 128 * 1024
        || !doc.text.contains("EntityListeners")
        || !live::persistence_annotations_only(&doc.text)
    {
        return None;
    }
    let root = host::project_of(&doc.path)?.root().ok()?;
    let owner = model::parse_class(&doc.text, &doc.path)?;
    if owner.kind != Kind::Entity
        || owner.superclass.is_some()
        || !live::top_level_identity(&doc.text, &owner)
        || doc.path.split('/').any(|p| matches!(p, "." | ".."))
        || doc.path
            != format!(
                "{}/src/main/java/{}/{}.java",
                root.trim_end_matches('/'),
                owner.package.as_ref()?.replace('.', "/"),
                owner.name
            )
    {
        return None;
    }
    let masked = model::mask(&doc.text);
    if !attribute_info::balanced(&masked) {
        return None;
    }
    let mut declarations = Vec::new();
    let mut from = 0;
    while let Some((at, word)) = model::declaration_keyword(
        &masked,
        from,
        &[
            "class",
            "interface",
            "enum",
            "record",
            "object",
            "typealias",
        ],
    ) {
        declarations.push((at, word));
        if declarations.len() > 2 {
            return None;
        }
        from = at + word.len();
    }
    let [(owner_at, "class"), (nested_at, "class")] = declarations.as_slice() else {
        return None;
    };
    let owner_open = masked[*owner_at..].find('{')? + owner_at;
    if masked[owner_at + 5..owner_open].trim() != owner.name {
        return None;
    }
    let owner_close = owner.body_end?;
    let members = model::members(&masked, owner_open + 1, owner_close, false, false);
    if members.len() > 128 {
        return None;
    }
    let (start, end) = members
        .iter()
        .copied()
        .find(|(start, end)| *nested_at >= *start && *nested_at < *end)?;
    let decl = model::without_annotations(&masked, start, end);
    let stop = decl.find(['(', '=', '{', ';']).unwrap_or(decl.len()) + start;
    if *nested_at >= stop {
        return None;
    }
    let modifiers: Vec<_> = masked[start..*nested_at].split_whitespace().collect();
    if !modifiers.contains(&"public")
        || !modifiers.contains(&"static")
        || modifiers
            .iter()
            .any(|m| !["public", "static", "final"].contains(m))
    {
        return None;
    }
    let open = masked[*nested_at..end].find('{')? + nested_at;
    let name = masked[nested_at + 5..open].trim();
    if !identifier(name) || name == owner.name {
        return None;
    }
    let close = model::closing(masked.as_bytes(), open)?;
    if close >= owner_close {
        return None;
    }
    if owner.imports.iter().any(|i| {
        i.ends_with(".*")
            || i.rsplit('.')
                .next()
                .is_some_and(|n| n == owner.name || n == name)
    }) {
        return None;
    }
    let all = model::annotations(&masked);
    if all.len() > 256 {
        return None;
    }
    let entities: Vec<_> = all.iter().filter(|a| a.name == "Entity").collect();
    let listeners: Vec<_> = all.iter().filter(|a| a.name == "EntityListeners").collect();
    let ([entity], [listeners]) = (entities.as_slice(), listeners.as_slice()) else {
        return None;
    };
    let statement = masked[..*owner_at].rfind(';').map_or(0, |i| i + 1);
    for annotation in [*entity, *listeners] {
        if annotation.start < statement
            || annotation.start >= *owner_at
            || all.iter().any(|other| {
                other
                    .args
                    .is_some_and(|(s, e)| annotation.start >= s && annotation.start < e)
            })
        {
            return None;
        }
    }
    if all
        .iter()
        .any(|a| a.start >= start && a.end() <= *nested_at)
    {
        return None;
    }
    let (s, e) = listeners.args?;
    let args = masked[s..e].trim();
    let value = if let Some(rest) = args.strip_prefix("value") {
        rest.trim_start().strip_prefix('=')?.trim()
    } else {
        args
    };
    let value = if let Some(rest) = value.strip_prefix('{') {
        rest.strip_suffix('}')?.trim()
    } else {
        value
    };
    if value != format!("{}.{}.class", owner.name, name) {
        return None;
    }
    let name = name.to_owned();
    Some(Listener {
        owner: owner.name,
        masked,
        name,
        open,
        close,
    })
}
pub(super) fn findings(doc: &Document) -> Vec<Finding> {
    if !setting("checkListenerUsage") {
        return vec![];
    }
    let Some(listener) = context(doc) else {
        return vec![];
    };
    let members = model::members(
        &listener.masked,
        listener.open + 1,
        listener.close,
        false,
        false,
    );
    if members.len() > 128 {
        return vec![];
    }
    let all = model::annotations(&listener.masked);
    let mut findings = Vec::new();
    for (start, end) in members {
        let decl = model::without_annotations(&listener.masked, start, end);
        let Some(open) = decl.find('(') else { continue };
        let Some(close) = model::closing(decl.as_bytes(), open) else {
            continue;
        };
        let head: Vec<_> = decl[..open].split_whitespace().collect();
        let bad = match head.as_slice() {
            [name] => *name == listener.name,
            [visibility, name] => {
                ["private", "protected"].contains(visibility) && *name == listener.name
            }
            _ => false,
        };
        if !bad
            || !decl[open + 1..close].trim().is_empty()
            || !decl[close + 1..].trim_start().starts_with('{')
            || all
                .iter()
                .any(|a| a.start >= start && a.end() <= start + open)
        {
            continue;
        }
        let at = start + decl[..open].rfind(&listener.name).unwrap();
        findings.push(Finding::new(at,at+listener.name.len(),Severity::Info,"jpa-listener-constructor",
            "Current source attaches this nested listener and explicitly declares a non-public no-arg constructor. Default JPA construction expects a public one; external/container construction is unresolved."));
    }
    findings
}

/// Listener-role signature subset. Parameter assignability is deliberately unresolved.
pub(super) fn callback_findings(doc: &Document) -> Vec<Finding> {
    if !setting("checkListenerSignatures") {
        return vec![];
    }
    let Some(listener) = context(doc) else {
        return vec![];
    };
    let members = model::members(
        &listener.masked,
        listener.open + 1,
        listener.close,
        false,
        false,
    );
    if members.len() > 128 {
        return vec![];
    }
    let all = model::annotations(&listener.masked);
    let mut found = vec![];
    let mut events = std::collections::HashSet::new();
    for (start, end) in members {
        let callbacks: Vec<_> = all
            .iter()
            .filter(|a| {
                a.start >= start && a.start < end && crate::checks::CALLBACKS.contains(&a.name)
            })
            .collect();
        if callbacks.is_empty()
            || callbacks.iter().any(|a| {
                a.args.is_some()
                    || all
                        .iter()
                        .any(|outer| outer.args.is_some_and(|(s, e)| a.start >= s && a.start < e))
            })
        {
            continue;
        }
        let decl = model::without_annotations(&listener.masked, start, end);
        let Some(open) = decl.find('(') else {
            continue;
        };
        let Some(close) = model::closing(decl.as_bytes(), open) else {
            continue;
        };
        if callbacks.iter().any(|a| a.end() > start + open) {
            continue;
        }
        let head: Vec<_> = decl[..open].split_whitespace().collect();
        if head.len() < 2 {
            continue;
        }
        let name = head[head.len() - 1];
        let ty = head[head.len() - 2];
        if !identifier(name)
            || ![
                "void", "int", "long", "short", "byte", "char", "boolean", "float", "double",
            ]
            .contains(&ty)
            || head[..head.len() - 2].iter().any(|m| {
                ![
                    "public",
                    "private",
                    "protected",
                    "static",
                    "final",
                    "synchronized",
                    "strictfp",
                ]
                .contains(m)
            })
            || !decl[close + 1..].trim_start().starts_with('{')
        {
            continue;
        }
        let params = decl[open + 1..close].trim();
        if !params.is_empty()
            && !params.split(',').all(|p| {
                let words: Vec<_> = p.split_whitespace().collect();
                words.len() == 2 && identifier(words[1]) && words[0].split('.').all(identifier)
            })
        {
            continue;
        }
        let problem = if head[..head.len() - 2]
            .iter()
            .any(|m| ["static", "final"].contains(m))
        {
            Some((
                "jpa-callback-modifiers",
                "A source-attached listener lifecycle callback must not be static or final.",
            ))
        } else if ty != "void" || params.is_empty() || params.split(',').count() != 1 {
            Some((
                "entity-listener-signature",
                "A source-attached listener lifecycle callback must return void and take exactly one argument. Argument assignability is unresolved.",
            ))
        } else {
            None
        };
        if let Some((code, message)) = problem {
            let at = start + decl[..open].rfind(name).unwrap();
            found.push(Finding::new(
                at,
                at + name.len(),
                Severity::Error,
                code,
                message,
            ));
        } else if params.split_whitespace().next() == Some(listener.owner.as_str()) {
            // Only the enclosing source entity type establishes assignability here.
            let mut current = std::collections::HashSet::new();
            let duplicates: Vec<_> = callbacks
                .iter()
                .filter_map(|a| {
                    (current.insert(a.name) && !events.insert(a.name)).then_some(a.name)
                })
                .collect();
            if !duplicates.is_empty() {
                let at = start + decl[..open].rfind(name).unwrap();
                found.push(Finding::new(at, at + name.len(), Severity::Error,
                    "jpa-duplicate-lifecycle-event", format!("This source-attached listener already has a supported local callback for {}. Use one method per lifecycle event.", duplicates.join(", "))));
            }
        }
    }
    found
}
