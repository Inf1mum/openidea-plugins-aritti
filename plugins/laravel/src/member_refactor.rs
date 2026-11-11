//! Reviewed member rename for a closed PHP declaration family and proven
//! receiver-bound references. Unknown same-name calls block the transaction.
use crate::{Index, index::Entry, package_facts};
use openidea_plugin::{
    Error, Position, Project, Range, TextEdit, file_actions::FileAction, host, providers::Document,
    text,
};
use std::collections::BTreeSet;
#[derive(serde::Serialize, serde::Deserialize)]
struct Request {
    root: String,
    destination: (String, Position, u32),
    family: Vec<(String, Position, u32)>,
    name: String,
    method: bool,
    path: String,
    version: u64,
    hash: String,
    position: Position,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    entry: Entry,
    name: String,
    range: Range,
    method: bool,
    family: Vec<Entry>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Change {
    path: String,
    source: String,
    ranges: Vec<Range>,
}
fn hash(source: &str) -> String {
    crate::artisan_metadata::hash(source)
}
fn ident(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}
fn target(index: &Index, project: &Project, doc: &Document, position: Position) -> Option<Target> {
    let entry = crate::eloquent_members::definition_checked(index, doc, position, &|p| {
        crate::owns_current_path(project, p)
    })?;
    if !crate::owns_current_path(project, &entry.path) {
        return None;
    }
    let source = if entry.path == doc.path {
        doc.text.clone()
    } else {
        project.read_file(&entry.path).ok()?
    };
    let code = package_facts::code(&source)?;
    let at = text::offset(&source, entry.at)?;
    let end = text::offset(
        &source,
        Position::new(entry.at.line, entry.at.character.checked_add(entry.width)?),
    )?;
    let token = source.get(at..end)?;
    let method = !token.starts_with('$');
    let name = token.trim_start_matches('$');
    if !ident(name) || name.starts_with("__") {
        return None;
    }
    let before = code[..at].trim_end();
    if method && !before.ends_with("function") {
        return None;
    }
    let family = crate::eloquent_members::semantic::member_family(index, doc, &entry, &|path| {
        crate::owns_current_path(project, path)
    })?;
    let class_at = ["class ", "trait ", "interface "]
        .into_iter()
        .filter_map(|keyword| code.find(keyword))
        .min()?;
    let body = class_at + code[class_at..].find('{')?;
    if !method {
        let mut stack = Vec::new();
        for byte in code[body + 1..at].bytes() {
            match byte {
                b'(' | b'[' | b'{' => stack.push(byte),
                b')' | b']' | b'}' => {
                    stack.pop();
                }
                _ => {}
            }
        }
        if !stack.is_empty() {
            return None;
        } // promoted parameters/local variables need a separate binding rewrite
    }
    let start = at + usize::from(!method);
    Some(Target {
        entry,
        name: name.into(),
        range: Range::new(text::position(&source, start), text::position(&source, end)),
        method,
        family,
    })
}
fn occurrences(source: &str, name: &str, casefold: bool) -> Vec<std::ops::Range<usize>> {
    let matching = if casefold {
        source.to_ascii_lowercase()
    } else {
        source.into()
    };
    let name = if casefold {
        name.to_ascii_lowercase()
    } else {
        name.into()
    };
    matching
        .match_indices(&name)
        .filter(|(at, _)| {
            *at == 0
                || !matching.as_bytes()[at - 1].is_ascii_alphanumeric()
                    && matching.as_bytes()[at - 1] != b'_'
        })
        .filter(|(at, _)| {
            matching
                .as_bytes()
                .get(at + name.len())
                .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_' && *b < 128)
        })
        .map(|(at, _)| at..at + name.len())
        .collect()
}
fn route_methods(
    index: &Index,
    project: &Project,
    doc: &Document,
    target: &Target,
) -> Result<(Vec<Range>, Vec<Range>), Error> {
    let mut ranges = Vec::new();
    let mut unrelated = Vec::new();
    for reference in crate::refs::references("php", &doc.text)
        .into_iter()
        .filter(|r| {
            r.closed
                && matches!(
                    r.kind,
                    crate::refs::Kind::Action | crate::refs::Kind::Method
                )
        })
    {
        let (class, method, begin) = if reference.kind == crate::refs::Kind::Method {
            let Some(class) = reference.class else {
                continue;
            };
            (
                class,
                doc.text[reference.start..reference.end].to_owned(),
                reference.start,
            )
        } else {
            let quote = doc
                .text
                .as_bytes()
                .get(reference.start.saturating_sub(1))
                .copied()
                .ok_or(Error::Stale)? as char;
            let Some(decoded) = crate::actions::controller_literal(
                &doc.text[reference.start..reference.end],
                quote,
            ) else {
                continue;
            };
            let Some((class, method)) = decoded.split_once('@') else {
                continue;
            };
            (
                class.to_owned(),
                method.to_owned(),
                reference.end - method.len(),
            )
        };
        if !method.eq_ignore_ascii_case(&target.name) {
            continue;
        }
        if !crate::actions::controller_binding_safe(&doc.text, &(reference.start..reference.end)) {
            return Err(Error::Invalid(crate::ui_text::t(
                "member.bindingUnknown",
                "An action-string caller has an unknown framework binding.",
            )));
        }
        let full = if class.contains('\\') {
            class.trim_start_matches('\\').to_owned()
        } else {
            crate::eloquent_members::source_class_name(&doc.text, &class).ok_or(Error::Stale)?
        };
        let mut probe = doc.clone();
        probe.text.push_str(&format!("\nfunction __laravel_member_rename_probe(\\{full} $laravelMemberReceiver) {{ $laravelMemberReceiver->{method}(); }}"));
        let offset = probe
            .text
            .rfind(&format!("->{method}"))
            .ok_or(Error::Stale)?
            + 2;
        let resolved = crate::eloquent_members::definition_checked(
            index,
            &probe,
            text::position(&probe.text, offset),
            &|p| crate::owns_current_path(project, p),
        );
        let range = Range::new(
            text::position(&doc.text, begin),
            text::position(&doc.text, reference.end),
        );
        match resolved {
            Some(entry)
                if target.family.iter().any(|candidate| {
                    candidate.path == entry.path
                        && candidate.at == entry.at
                        && candidate.width == entry.width
                }) =>
            {
                ranges.push(range)
            }
            Some(_) => unrelated.push(range),
            None => {
                return Err(Error::Invalid(crate::ui_text::t(
                    "member.controllerUnknown",
                    "A controller-method string with the same name could not be assigned to a proven declaration family.",
                )));
            }
        }
    }
    Ok((ranges, unrelated))
}

fn collect(
    index: &Index,
    project: &Project,
    doc: &Document,
    target: &Target,
) -> Result<Vec<Change>, Error> {
    let paths = project.find_files("**/*.php", 257)?;
    if paths.len() > 256 {
        return Err(Error::Invalid(crate::ui_text::t(
            "member.filesBound",
            "Member rename exceeds its 256 PHP source review bound.",
        )));
    }
    let mut paths: BTreeSet<_> = paths
        .into_iter()
        .filter(|p| {
            ![
                "/vendor/",
                "/node_modules/",
                "/storage/",
                "/bootstrap/cache/",
            ]
            .iter()
            .any(|skip| p.contains(skip))
        })
        .collect();
    paths.insert(doc.path.clone());
    paths.extend(target.family.iter().map(|entry| entry.path.clone()));
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
        total += source.len();
        if source.len() > 128 * 1024 || total > 2 * 1024 * 1024 {
            return Err(Error::Invalid(crate::ui_text::t(
                "member.sizeBound",
                "Member rename source proof exceeds its bounded review size.",
            )));
        }
        let document = Document {
            path: path.clone(),
            language: "php".into(),
            text: source.clone(),
            version: 0,
        };
        let mut ranges = Vec::new();
        for entry in &target.family {
            ranges.extend(
                crate::eloquent_members::semantic::member_references(
                    index,
                    &document,
                    entry,
                    &|p| crate::owns_current_path(project, p),
                )
                .ok_or(Error::Stale)?,
            );
            if path == entry.path {
                let begin = text::offset(&source, entry.at).ok_or(Error::Stale)?;
                let end = text::offset(
                    &source,
                    Position::new(
                        entry.at.line,
                        entry
                            .at
                            .character
                            .checked_add(entry.width)
                            .ok_or(Error::Stale)?,
                    ),
                )
                .ok_or(Error::Stale)?;
                let token = source.get(begin..end).ok_or(Error::Stale)?;
                let name = token.trim_start_matches('$');
                if !if target.method {
                    name.eq_ignore_ascii_case(&target.name)
                } else {
                    name == target.name
                } {
                    return Err(Error::Stale);
                }
                ranges.push(Range::new(
                    text::position(&source, begin + usize::from(token.starts_with('$'))),
                    text::position(&source, end),
                ));
            }
        }
        let mut unrelated = Vec::new();
        if target.method {
            let (route, other) = route_methods(index, project, &document, target)?;
            ranges.extend(route);
            unrelated = other;
        }
        for range in &ranges {
            let begin = text::offset(&source, range.start).ok_or(Error::Stale)?;
            let end = text::offset(&source, range.end).ok_or(Error::Stale)?;
            let token = source.get(begin..end).ok_or(Error::Stale)?;
            if !if target.method {
                token.eq_ignore_ascii_case(&target.name)
            } else {
                token == target.name
            } {
                return Err(Error::Invalid(crate::ui_text::t(
                    "member.aliasUnsupported",
                    "An adapted member alias needs a separate name transformation; no edits were produced.",
                )));
            }
        }
        let code = package_facts::code(&source).ok_or(Error::Stale)?;
        for occurrence in occurrences(&source, &target.name, target.method) {
            let position = text::position(&source, occurrence.start);
            if ranges
                .iter()
                .chain(&unrelated)
                .any(|r| r.start <= position && position < r.end)
            {
                continue;
            }
            let before = code[..occurrence.start].trim_end();
            let after = code[occurrence.end..].trim_start();
            let in_string = source.get(occurrence.clone()) != code.get(occurrence.clone());
            if target.method
                && before.ends_with("function")
                && !target.family.iter().any(|entry| entry.path == path)
            {
                let identity = crate::eloquent_members::declaration(&source);
                if crate::eloquent_members::class_inventory(&index.eloquent_models)
                    .iter()
                    .any(|class| {
                        !class.ambiguous
                            && class.path == path
                            && identity.as_deref() == Some(class.full.as_str())
                    })
                {
                    continue;
                }
            }

            if !in_string
                && crate::eloquent_members::definition_checked(index, &document, position, &|p| {
                    crate::owns_current_path(project, p)
                })
                .is_some_and(|entry| {
                    !target.family.iter().any(|candidate| {
                        candidate.path == entry.path
                            && candidate.at == entry.at
                            && candidate.width == entry.width
                    })
                })
            {
                continue;
            }

            if ["->", "::", "?->"]
                .iter()
                .any(|suffix| before.ends_with(suffix))
                || before.ends_with("::$")
                || in_string
                    && source[..occurrence.start]
                        .chars()
                        .next_back()
                        .is_some_and(|c| matches!(c, '\'' | '"' | '@'))
                    && source[occurrence.end..]
                        .chars()
                        .next()
                        .is_some_and(|c| matches!(c, '\'' | '"'))
                || target.method && before.ends_with("function") && after.starts_with('(')
            {
                return Err(Error::Invalid(crate::ui_text::format(
                    "member.candidateUnknown",
                    "A same-name member candidate at {path}:{line} cannot be proven to belong to this declaration. Rename is withheld rather than leaving a possible usage unchanged.",
                    &[("path", &path), ("line", &(position.line + 1).to_string())],
                )));
            }
        }
        ranges.sort_by_key(|r| (r.start.line, r.start.character));
        ranges.dedup();
        if !ranges.is_empty() {
            changes.push(Change {
                path,
                source,
                ranges,
            });
        }
    }
    Ok(changes)
}
pub(crate) fn actions(index: &Index, doc: &Document, selection: Range) -> Vec<FileAction> {
    let run = || -> Option<FileAction> {
        if index.partial || doc.language != "php" || doc.text.len() > 128 * 1024 {
            return None;
        }
        let project = crate::project_for(&doc.path)?;
        let target = target(index, &project, doc, selection.start)?;
        let request = Request {
            root: project.root().ok()?,
            destination: (
                target.entry.path.clone(),
                target.entry.at,
                target.entry.width,
            ),
            family: target
                .family
                .iter()
                .map(|entry| (entry.path.clone(), entry.at, entry.width))
                .collect(),
            name: target.name.clone(),
            method: target.method,
            path: doc.path.clone(),
            version: doc.version,
            hash: hash(&doc.text),
            position: selection.start,
        };
        let data = format!("member-rename:{}", serde_json::to_string(&request).ok()?);
        if data.len() > 65536 {
            return None;
        }
        Some(FileAction {
            title: crate::ui_text::format(
                if target.method {
                    "member.action.method"
                } else {
                    "member.action.property"
                },
                if target.method {
                    "Rename proven PHP method {name}…"
                } else {
                    "Rename proven PHP property {name}…"
                },
                &[("name", &target.name)],
            ),
            kind: Some("refactor.rename".into()),
            data: Some(data),
            ..Default::default()
        })
    };
    run().into_iter().collect()
}
pub(crate) fn resolve(
    index: &Index,
    doc: &Document,
    action: FileAction,
) -> Result<FileAction, Error> {
    let request: Request = serde_json::from_str(
        action
            .data
            .as_deref()
            .and_then(|s| s.strip_prefix("member-rename:"))
            .filter(|s| s.len() < 65536)
            .ok_or(Error::Stale)?,
    )
    .map_err(|_| Error::Stale)?;
    let project = crate::project_for(&doc.path).ok_or(Error::Stale)?;
    if index.partial
        || project.root()? != request.root
        || request.path != doc.path
        || request.version != doc.version
        || request.hash != hash(&doc.text)
        || project.read_file(&doc.path)? != doc.text
    {
        return Err(Error::Stale);
    }
    if !crate::index::index_sources_current(&project,index){return Err(Error::Stale);}
    let current = index;
    let target = target(current, &project, doc, request.position).ok_or(Error::Stale)?;
    if request.destination
        != (
            target.entry.path.clone(),
            target.entry.at,
            target.entry.width,
        )
        || request.name != target.name
        || request.method != target.method
        || request.family
            != target
                .family
                .iter()
                .map(|entry| (entry.path.clone(), entry.at, entry.width))
                .collect::<Vec<_>>()
    {
        return Err(Error::Stale);
    }

    let before = collect(current, &project, doc, &target)?;
    let new = host::input(
        &crate::ui_text::t("member.input", "New PHP member name"),
        Some(&target.name),
    )?
    .ok_or(Error::Cancelled)?;
    if !ident(&new) || new.starts_with("__") {
        return Err(Error::Invalid(crate::ui_text::t(
            "member.invalidName",
            "Use a PHP member identifier; magic member names are not accepted.",
        )));
    }
    if new == target.name || target.method && new.eq_ignore_ascii_case(&target.name) {
        return Err(Error::Cancelled);
    }
    if !crate::index::index_sources_current(&project,index)||project.read_file(&doc.path)?!=doc.text||collect(current,&project,doc,&target)?!=before{return Err(Error::Stale);}
    match crate::eloquent_members::semantic::member_rename_collision(
        current,
        doc,
        &target.entry,
        &new,
        &|p| crate::owns_current_path(&project, p),
    ) {
        Some(false) => {}
        Some(true) => {
            return Err(Error::Invalid(crate::ui_text::t(
                "member.collision",
                "The new name collides with a declaration in the proven member family.",
            )));
        }
        None => return Err(Error::Stale),
    }
    let mut preview = crate::ui_text::format(
        "member.review",
        "Rename {name} → {new} across a proven PHP declaration family and {files} source files. Same-name unresolved calls/strings and unsupported trait adapters block this operation.\n\n",
        &[
            ("name", &target.name),
            ("new", &new),
            ("files", &before.len().to_string()),
        ],
    );
    let mut edits = Vec::new();
    for change in &before {
        let mut rendered = change.source.clone();
        for range in change.ranges.iter().rev() {
            let start = text::offset(&change.source, range.start).ok_or(Error::Stale)?;
            let end = text::offset(&change.source, range.end).ok_or(Error::Stale)?;
            rendered.replace_range(start..end, &new);
        }
        preview.push_str(&format!(
            "{}\n{}\n\n",
            change.path,
            ui_kit::prompts::readable_source(&rendered)
        ));
        edits.push((
            change.path.clone(),
            change
                .ranges
                .iter()
                .map(|r| TextEdit {
                    range: *r,
                    text: new.clone(),
                })
                .collect(),
        ));
    }
    if !ui_kit::prompts::confirm_exact(
        &crate::ui_text::t("member.confirm", "Rename proven PHP member?"),
        &preview,
    )? {
        return Err(Error::Cancelled);
    }
    let latest = index;
    if !crate::index::index_sources_current(&project,index)
        || latest.partial
        || project.read_file(&doc.path)? != doc.text
        || crate::eloquent_members::semantic::member_family(latest, doc, &target.entry, &|p| {
            crate::owns_current_path(&project, p)
        })
        .as_ref()
            != Some(&target.family)
        || crate::eloquent_members::semantic::member_rename_collision(
            latest,
            doc,
            &target.entry,
            &new,
            &|p| crate::owns_current_path(&project, p),
        ) != Some(false)
        || collect(latest, &project, doc, &target)? != before
    {
        return Err(Error::Stale);
    }
    Ok(FileAction {
        title: crate::ui_text::format(
            "member.result",
            "Rename {name} → {new}",
            &[("name", &target.name), ("new", &new)],
        ),
        edits,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const SOURCE: &str = "<?php\nnamespace App;\nclass Service\n{\n    public function send() { return 'result'; }\n    public function run() { return $this->send(); }\n}\n";
    fn setup() -> (MockHost, Project, Index, Document) {
        let host = MockHost::install();
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        host.add_file("artisan", "<?php");
        host.add_file("app/Service.php", SOURCE);
        let project = host.invocation().project().unwrap();
        let index = Index::build(&project);
        let doc = Document {
            path: "/project/app/Service.php".into(),
            language: "php".into(),
            version: 1,
            text: SOURCE.into(),
        };
        (host, project, index, doc)
    }
    fn caret(doc: &Document) -> Range {
        let position = text::position(&doc.text, doc.text.rfind("send()").unwrap() + 1);
        Range::new(position, position)
    }
    fn render(source: &str, edits: &[TextEdit]) -> String {
        let mut out = source.to_owned();
        let mut edits = edits.to_vec();
        edits.sort_by_key(|e| std::cmp::Reverse((e.range.start.line, e.range.start.character)));
        for edit in edits {
            let start = text::offset(source, edit.range.start).unwrap();
            let end = text::offset(source, edit.range.end).unwrap();
            out.replace_range(start..end, &edit.text);
        }
        out
    }
    #[test]
    fn method_rename_is_one_atomic_proven_transaction_and_does_not_write_before_apply() {
        let (host, project, index, doc) = setup();
        let action = actions(&index, &doc, caret(&doc)).pop().unwrap();
        host.answer_next_prompt(Some(&["dispatchMessage"]));
        host.answer_next_prompt(Some(&["yes"]));
        let result = resolve(&index, &doc, action).unwrap();
        assert_eq!(result.edits.len(), 1);
        assert_eq!(result.edits[0].1.len(), 2);
        let rendered = render(SOURCE, &result.edits[0].1);
        assert!(rendered.contains("function dispatchMessage()"));
        assert!(rendered.contains("$this->dispatchMessage()"));
        assert_eq!(project.read_file(&doc.path).unwrap(), SOURCE);
    }
    #[test]
    fn unresolved_receiver_string_and_existing_name_collisions_refuse_every_edit() {
        for extra in ["<?php $unknown->send();", "<?php $callback='send';"] {
            let (host, _project, index, doc) = setup();
            host.add_file("app/Other.php", extra);
            let action = actions(&index, &doc, caret(&doc)).pop().unwrap();
            assert!(resolve(&index, &doc, action).is_err(), "{extra}");
        }
        let (host, _project, index, doc) = setup();
        let action = actions(&index, &doc, caret(&doc)).pop().unwrap();
        host.answer_next_prompt(Some(&["run"]));
        assert!(resolve(&index, &doc, action).is_err());
    }
    #[test]
    fn stale_document_and_inherited_class_are_withheld() {
        let (_host, _project, index, mut doc) = setup();
        let action = actions(&index, &doc, caret(&doc)).pop().unwrap();
        doc.version += 1;
        assert!(resolve(&index, &doc, action).is_err());
        let (host, _project, _index, mut doc) = setup();
        doc.text = doc
            .text
            .replace("class Service", "class Service extends ParentService");
        host.add_file("app/Service.php", &doc.text);
        let index = Index::build(&host.invocation().project().unwrap());
        assert!(actions(&index, &doc, caret(&doc)).is_empty());
    }
    #[test]
    fn owned_parent_override_family_and_route_method_strings_change_together() {
        let (host, project, _index, mut doc) = setup();
        host.add_file("app/BaseService.php","<?php\nnamespace App;\nclass BaseService {\n public function send() {}\n public function reserved() {}\n}\n");
        doc.text = SOURCE.replace("class Service", "class Service extends BaseService");
        host.add_file("app/Service.php", &doc.text);
        host.add_file("routes/web.php","<?php\nuse App\\Service;\nRoute::post('/send', [Service::class, 'send']);\nRoute::post('/legacy', 'App\\Service@send');\n");
        let index = Index::build(&project);
        let action = actions(&index, &doc, caret(&doc)).pop().unwrap();
        host.answer_next_prompt(Some(&["dispatchMessage"]));
        host.answer_next_prompt(Some(&["yes"]));
        let result = resolve(&index, &doc, action).unwrap();
        assert_eq!(result.edits.len(), 3);
        assert_eq!(
            result
                .edits
                .iter()
                .find(|(path, _)| path.ends_with("routes/web.php"))
                .unwrap()
                .1
                .len(),
            2
        );
        assert!(
            result
                .edits
                .iter()
                .any(|(path, _)| path.ends_with("BaseService.php"))
        );
        let action = actions(&index, &doc, caret(&doc)).pop().unwrap();
        host.answer_next_prompt(Some(&["reserved"]));
        assert!(resolve(&index, &doc, action).is_err());
    }
    #[test]
    fn simple_trait_family_updates_original_declaration_and_consumer_usage() {
        let (host, project, _index, mut doc) = setup();
        host.add_file(
            "app/Traits/Sender.php",
            "<?php\nnamespace App\\Traits;\ntrait Sender {\npublic function send() {}\n}\n",
        );
        doc.text="<?php\nnamespace App;\nuse App\\Traits\\Sender;\nclass Service {\nuse Sender;\npublic function run() { return $this->send(); }\n}\n".into();
        host.add_file("app/Service.php", &doc.text);
        let index = Index::build(&project);
        let action = actions(&index, &doc, caret(&doc)).pop().unwrap();
        host.answer_next_prompt(Some(&["dispatchMessage"]));
        host.answer_next_prompt(Some(&["yes"]));
        let result = resolve(&index, &doc, action).unwrap();
        assert_eq!(result.edits.len(), 2);
        assert!(
            result
                .edits
                .iter()
                .any(|(path, _)| path.ends_with("Traits/Sender.php"))
        );
    }
    #[test]
    fn proven_unrelated_same_name_members_are_preserved() {
        let (host, project, _index, doc) = setup();
        host.add_file("app/Other.php","<?php\nnamespace App;\nclass Other {\npublic function send() {}\npublic function run() { return $this->send(); }\n}\n");
        let index = Index::build(&project);
        let action = actions(&index, &doc, caret(&doc)).pop().unwrap();
        host.answer_next_prompt(Some(&["dispatchMessage"]));
        host.answer_next_prompt(Some(&["yes"]));
        let result = resolve(&index, &doc, action).unwrap();
        assert_eq!(result.edits.len(), 1);
        assert_eq!(result.edits[0].0, doc.path);
    }

    #[test]
    fn plain_property_rename_keeps_dollar_prefix_and_casing_is_not_folded() {
        let (host, project, _index, mut doc) = setup();
        doc.text="<?php\nnamespace App;\nclass Service {\npublic string $label;\npublic function run() { return $this->label; }\n}\n".into();
        host.add_file("app/Service.php", &doc.text);
        let index = Index::build(&project);
        let position = text::position(&doc.text, doc.text.rfind("label;").unwrap() + 1);
        let action = actions(&index, &doc, Range::new(position, position))
            .pop()
            .unwrap();
        host.answer_next_prompt(Some(&["displayLabel"]));
        host.answer_next_prompt(Some(&["yes"]));
        let result = resolve(&index, &doc, action).unwrap();
        let rendered = render(&doc.text, &result.edits[0].1);
        assert!(rendered.contains("$displayLabel"));
        assert!(rendered.contains("$this->displayLabel"));
    }

    #[test]
    fn changed_definition_identity_and_reparented_source_invalidate_pending_actions() {
        let (host, project, _index, mut doc) = setup();
        host.add_file(
            "app/BaseService.php",
            "<?php\nnamespace App;\nclass BaseService {\npublic function send() {}\n}\n",
        );
        doc.text="<?php\nnamespace App;\nclass Service extends BaseService {\npublic function run() { return $this->send(); }\n}\n".into();
        host.add_file("app/Service.php", &doc.text);
        let index = Index::build(&project);
        let action = actions(&index, &doc, caret(&doc)).pop().unwrap();
        host.add_file(
            "app/BaseService.php",
            "<?php\nnamespace App;\n\nclass BaseService {\npublic function send() {}\n}\n",
        );
        assert!(matches!(resolve(&index, &doc, action), Err(Error::Stale)));
        let (host, _project, index, doc) = setup();
        let action = actions(&index, &doc, caret(&doc)).pop().unwrap();
        host.add_project("/project/app");
        assert!(resolve(&index, &doc, action).is_err());
    }

    #[test]
    fn japanese_action_and_review_keep_identifiers_and_exact_source_edits() {
        let (host, _project, index, doc) = setup();
        host.set_setting("uiLanguage", "\"ja\"");
        let action = actions(&index, &doc, caret(&doc)).pop().unwrap();
        assert_ne!(action.title, "Rename proven PHP method send…");
        assert!(action.title.contains("send"));
        host.answer_next_prompt(Some(&["dispatchMessage"]));
        host.answer_next_prompt(Some(&["yes"]));
        let result = resolve(&index, &doc, action).unwrap();
        assert_eq!(
            render(SOURCE, &result.edits[0].1),
            SOURCE.replace("send()", "dispatchMessage()")
        );
    }

    #[test]
    #[ignore = "requires php executable; lints actual reviewed member edits"]
    fn actual_member_rename_output_lints() {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let (host, _project, index, doc) = setup();
        let action = actions(&index, &doc, caret(&doc)).pop().unwrap();
        host.answer_next_prompt(Some(&["dispatchMessage"]));
        host.answer_next_prompt(Some(&["yes"]));
        let result = resolve(&index, &doc, action).unwrap();
        let rendered = render(SOURCE, &result.edits[0].1);
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
