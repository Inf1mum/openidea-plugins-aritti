//! Explicit read-only literal source matches; never semantic usages/Rename.
use crate::{
    hover::canonical,
    refs::{self, Kind, Reference},
};
use openidea_plugin::{
    Error, FileKind, Invocation, NotificationLevel, PickItem, Project, host, text,
};
use std::{collections::BTreeSet, rc::Rc};
const PATHS: usize = 2048;
const SOURCE: usize = 128 << 10;
const TOTAL: usize = 8 << 20;
const MATCHES: usize = 512;
#[derive(Clone)]
struct Query {
    path: String,
    language: String,
    version: u64,
    text: String,
    kind: Kind,
    name: String,
}
#[derive(Clone)]
struct Match {
    path: String,
    start: usize,
    end: usize,
    source: Rc<str>,
}
struct Scan {
    matches: Vec<Match>,
    incomplete: bool,
}
fn label(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Route => Some("route"),
        Kind::View => Some("view"),
        Kind::Config => Some("config"),
        Kind::Translation => Some("translation"),
        _ => None,
    }
}
fn source_path(path: &str, root: &str) -> bool {
    canonical(path)
        && path.len() <= 4096
        && !path.chars().any(char::is_control)
        && crate::under(path, root)
        && path.ends_with(".php")
        && path[root.len() + 1..]
            .split('/')
            .all(|p| !p.starts_with('.') && !["vendor", "node_modules", "storage"].contains(&p))
        && !path[root.len() + 1..].starts_with("bootstrap/cache/")
}
fn literal<'a>(source: &'a str, r: &Reference) -> Option<&'a str> {
    if !r.closed || label(r.kind).is_none() {
        return None;
    }
    let name = source.get(r.start..r.end)?;
    if name.is_empty()
        || name.len() > 512
        || name.contains(['\\', '$'])
        || name.chars().any(char::is_control)
    {
        return None;
    }
    let before = source.get(..r.start.checked_sub(1)?)?;
    let quote = *source.as_bytes().get(r.start - 1)?;
    if !matches!(quote, b'\'' | b'"')
        || source.as_bytes().get(r.end) != Some(&quote)
        || !before.trim_end().ends_with(['(', ',', '['])
        || !source
            .get(r.end + 1..)?
            .trim_start()
            .starts_with([',', ')', ']'])
    {
        return None;
    }
    Some(name)
}
fn owned(project: &Project, root: &str, path: &str) -> bool {
    project.root().ok().as_deref() == Some(root)
        && source_path(path, root)
        && crate::owns_current_path(project, path)
}
fn read(project: &Project, root: &str, path: &str) -> Result<String, Error> {
    if !owned(project, root, path) {
        return Err(Error::Stale);
    }
    let stat = project.stat(path)?;
    if stat.kind != FileKind::File || stat.size > SOURCE as u64 {
        return Err(Error::Invalid(
            crate::ui_text::text("native.sourceArtisan.de5cc7c1e8","Source is outside the bounded regular-file scan.").into(),
        ));
    }
    let text = project.read_file(path)?;
    if text.len() > SOURCE {
        return Err(Error::Invalid(
            crate::ui_text::text("native.sourceArtisan.220bda4096","Source exceeds the literal-match limit.").into(),
        ));
    }
    Ok(text)
}
fn scan(project: &Project, root: &str, q: &Query) -> Result<Scan, Error> {
    if q.text.len() > SOURCE || !owned(project, root, &q.path) {
        return Err(Error::Stale);
    }
    let mut paths = BTreeSet::from([q.path.clone()]);
    for glob in [
        "app/**/*.php",
        "routes/**/*.php",
        "resources/views/**/*.php",
        "config/**/*.php",
        "lang/**/*.php",
        "resources/lang/**/*.php",
        "src/**/*.php",
    ] {
        let found = project.find_files(glob, (PATHS + 1) as u32)?;
        if found.len() > PATHS {
            return Err(Error::Invalid(
                crate::ui_text::text("native.sourceArtisan.4c5a0a4879","Literal source-match discovery is incomplete (path limit).").into(),
            ));
        }
        paths.extend(found);
        if paths.len() > PATHS {
            return Err(Error::Invalid(
                crate::ui_text::text("native.sourceArtisan.4c5a0a4879","Literal source-match discovery is incomplete (path limit).").into(),
            ));
        }
    }
    let mut result = Scan {
        matches: vec![],
        incomplete: false,
    };
    let mut total = 0usize;
    for path in paths {
        // No metadata or source reads are made across current ownership boundaries.
        if !owned(project, root, &path) {
            result.incomplete = true;
            continue;
        }
        let source = if path == q.path {
            q.text.clone()
        } else {
            match read(project, root, &path) {
                Ok(s) => s,
                Err(_) => {
                    result.incomplete = true;
                    continue;
                }
            }
        };
        total = total.checked_add(source.len()).ok_or(Error::Stale)?;
        if total > TOTAL {
            return Err(Error::Invalid(
                crate::ui_text::text("native.sourceArtisan.4639f02ce7","Literal source-match scan is incomplete (total byte limit).").into(),
            ));
        }
        let language = if path.ends_with(".blade.php") {
            "blade"
        } else {
            "php"
        };
        let source: Rc<str> = source.into();
        for r in refs::references(language, &source) {
            if r.kind == q.kind && literal(&source, &r) == Some(q.name.as_str()) {
                if result.matches.len() >= MATCHES {
                    return Err(Error::Invalid(
                        crate::ui_text::text("native.sourceArtisan.5977c6c911","Literal source-match scan is incomplete (match limit).").into(),
                    ));
                }
                result.matches.push(Match {
                    path: path.clone(),
                    start: r.start,
                    end: r.end,
                    source: source.clone(),
                });
            }
        }
    }
    Ok(result)
}
fn target(
    project: &Project,
    root: &str,
    q: &Query,
    m: &Match,
) -> Result<openidea_plugin::Position, Error> {
    if !owned(project, root, &m.path) {
        return Err(Error::Stale);
    }
    let current = if m.path == q.path {
        q.text.clone()
    } else {
        read(project, root, &m.path)?
    };
    if current != m.source.as_ref() {
        return Err(Error::Stale);
    }
    let language = if m.path.ends_with(".blade.php") {
        "blade"
    } else {
        "php"
    };
    let r = refs::reference_at(language, &current, m.start).ok_or(Error::Stale)?;
    if r.kind != q.kind
        || r.start != m.start
        || r.end != m.end
        || literal(&current, &r) != Some(q.name.as_str())
    {
        return Err(Error::Stale);
    }
    Ok(text::position(&current, m.start))
}
fn unchanged(
    editor: &openidea_plugin::TextEditor,
    project: &Project,
    root: &str,
    q: &Query,
) -> Result<(), Error> {
    let current = editor.document()?;
    if editor.path() != q.path
        || crate::language_of(&q.path, &editor.language()) != Some(q.language.as_str())
        || current.version != q.version
        || current.text != q.text
        || !owned(project, root, &q.path)
    {
        return Err(Error::Stale);
    }
    Ok(())
}
pub(crate) fn run(ctx: &Invocation) -> Result<Option<String>, Error> {
    let editor = ctx
        .editor()
        .ok_or(crate::ui_text::text("native.sourceArtisan.db276fb86a","Open a PHP or Blade literal reference first."))?;
    let path = editor.path();
    let project = crate::project_for(&path).ok_or(Error::Stale)?;
    let root = project.root()?;
    if !owned(&project, &root, &path) {
        return Err(Error::Stale);
    }
    let language = crate::language_of(&path, &editor.language())
        .ok_or(Error::Stale)?
        .to_owned();
    let doc = editor.document()?;
    if doc.text.len() > SOURCE {
        return Err(Error::Invalid(
            crate::ui_text::text("native.sourceArtisan.2a6a336d2b","Active source exceeds the literal-match limit.").into(),
        ));
    }
    let cursor = editor
        .selections()?
        .first()
        .map(|r| r.start)
        .ok_or(Error::Stale)?;
    let at = text::offset(&doc.text, cursor).ok_or(Error::Stale)?;
    let r = refs::reference_at(&language, &doc.text, at)
        .ok_or(crate::ui_text::text("native.sourceArtisan.82ce0df122","Choose a closed route/view/config/translation literal."))?;
    if at < r.start || at >= r.end {
        return Err(Error::Stale);
    }
    let name = literal(&doc.text, &r)
        .ok_or(crate::ui_text::text("native.sourceArtisan.558f0fd6ce","Choose a plain closed route/view/config/translation literal."))?
        .to_owned();
    let q = Query {
        path,
        language,
        version: doc.version,
        text: doc.text,
        kind: r.kind,
        name,
    };
    let result = scan(&project, &root, &q)?;
    let scope = crate::ui_text::text("native.sourceArtisan.ed40884a6d","Conventional app/routes/views/config/lang/src source paths only; literal spelling matches, not semantic usages or Rename. Custom roots, effective registrations and whole-project unsaved-buffer coverage are unresolved.");
    if result.matches.is_empty() {
        host::notify(
            NotificationLevel::Info,
            crate::ui_text::text("native.sourceArtisan.022414fbc5","Laravel literal source matches"),
            &crate::ui_text::format("native.sourceArtisan.f45dd46643","No matches recorded in supported sources. {scope}",&[("scope",&(scope).to_string())]),
        );
        return Ok(None);
    }
    if result.incomplete {
        host::notify(
            NotificationLevel::Warning,
            crate::ui_text::text("native.sourceArtisan.022414fbc5","Laravel literal source matches"),
            &crate::ui_text::format("native.sourceArtisan.35fe0ee606","Some candidate sources could not be safely inspected; results are incomplete. {scope}",&[("scope",&(scope).to_string())]),
        );
    }
    if !result.incomplete {
        host::notify(
            NotificationLevel::Info,
            crate::ui_text::text("native.sourceArtisan.022414fbc5","Laravel literal source matches"),
            &crate::ui_text::format("native.sourceArtisan.ba5e5044ee","Explicit bounded source scan. {scope}",&[("scope",&(scope).to_string())]),
        );
    }
    let items: Vec<_> = result
        .matches
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let at = text::position(&m.source, m.start);
            PickItem::new(
                &i.to_string(),
                &format!(
                    "{}:{}:{}",
                    &m.path[root.len() + 1..],
                    at.line + 1,
                    at.character + 1
                ),
            )
            .description(&crate::ui_text::format("native.sourceArtisan.b3f0c0b6dd","{kind} literal source candidate; conventional roots; no semantic binding",&[("kind",&(label(q.kind).unwrap_or("unknown")).to_string())]))
        })
        .collect();
    let title = crate::ui_text::format("native.sourceArtisan.c98c11a626","{count} recorded {kind} literal matches{incomplete} — {name}",&[("count",&(result.matches.len()).to_string()),("kind",&(label(q.kind).unwrap_or("unknown")).to_string()),("incomplete",&(if result.incomplete{crate::ui_text::text("native.sourceArtisan.e711c2534b"," (incomplete)")}else{""}).to_string()),("name",&(q.name).to_string())]);
    let Some(chosen) = host::pick(&title, &items)? else {
        return Ok(None);
    };
    unchanged(&editor, &project, &root, &q)?;
    let selected = chosen
        .parse::<usize>()
        .ok()
        .and_then(|i| result.matches.get(i))
        .ok_or(Error::Stale)?;
    let at = target(&project, &root, &q, selected)?;
    host::open_location(&selected.path, at.line, at.character)?;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Main;
    use openidea_plugin::{Plugin, mock::MockHost, text::position};
    fn setup(source: &str) -> (MockHost, Main) {
        let h = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let at = position(source, source.find("same").unwrap());
        h.add_file("routes/Query.php", source);
        h.open_editor(
            "/project/routes/Query.php",
            "php",
            source,
            &[(at.line, at.character, at.line, at.character)],
        );
        (h, Main::default())
    }
    #[test]
    fn registered_literal_source_match_command_opens_current_utf16_target_for_four_kinds() {
        for call in ["route", "view", "config", "__"] {
            let query = format!("<?php {call}('same');");
            let (h, mut main) = setup(&query);
            let target = format!("<?php /* 😀 */ {call}('same');\r\n");
            h.add_file("app/Target.php", &target);
            h.answer_next_prompt(Some(&["0"]));
            let result = main.run_command("find-literal-source-matches", &h.invocation(), None);
            assert!(
                result.is_ok(),
                "literal source-match command is missing: {result:?}"
            );
            let at = position(&target, target.find("same").unwrap());
            assert_eq!(
                h.opened_locations(),
                [("/project/app/Target.php".into(), at.line, at.character)]
            );
            assert!(h.process_runs().is_empty() && h.terminals().is_empty());
        }
    }
    fn query(source: &str) -> Query {
        Query {
            path: "/project/routes/Query.php".into(),
            language: "php".into(),
            version: 1,
            text: source.into(),
            kind: Kind::Route,
            name: "same".into(),
        }
    }
    #[test]
    fn explicit_scan_separates_literal_kinds_and_ignores_opaque_blade_dynamic_and_comment_facts() {
        let source = "<?php route('same');";
        let (h, _) = setup(source);
        h.add_file("app/Source.php","<?php config('same'); view('same'); __('same'); // route('same')\n route('same' . $suffix); route('other'); route('same');");
        h.add_file("resources/views/Target.blade.php","<?php $x=\"@include('same')\"; ?>\n{{-- {{ route('same') }} --}}\n<div>{{ route('same') }}</div>");
        let project = h.invocation().project().unwrap();
        let q = query(source);
        let found = scan(&project, "/project", &q).unwrap();
        assert!(!found.incomplete);
        assert_eq!(found.matches.len(), 3);
        assert_eq!(
            found
                .matches
                .iter()
                .map(|m| m.path.as_str())
                .collect::<Vec<_>>(),
            [
                "/project/app/Source.php",
                "/project/resources/views/Target.blade.php",
                "/project/routes/Query.php"
            ]
        );
        assert!(include_str!("../plugin.toml").contains("id = \"find-literal-source-matches\""));
    }
    #[test]
    fn explicit_scan_withholds_foreign_hidden_oversize_sources_and_target_revalidation_refuses_stale_generation()
     {
        let source = "<?php route('same');";
        let (h, _) = setup(source);
        h.add_file("app/Target.php", source);
        h.add_file("app/private/Secret.php", source);
        h.add_project("/project/app/private");
        h.add_file("app/.hidden.php", source);
        h.add_file("app/Big.php", &" ".repeat(SOURCE + 1));
        let project = h.invocation().project().unwrap();
        let q = query(source);
        let result = scan(&project, "/project", &q).unwrap();
        assert!(result.incomplete);
        assert_eq!(result.matches.len(), 2);
        assert!(
            result
                .matches
                .iter()
                .all(|m| !m.path.contains("private") && !m.path.contains("hidden"))
        );
        let selected = result
            .matches
            .iter()
            .find(|m| m.path.ends_with("Target.php"))
            .unwrap();
        assert!(target(&project, "/project", &q, selected).is_ok());
        h.add_file("app/Target.php", "<?php\n route('same');");
        assert!(target(&project, "/project", &q, selected).is_err());
        h.add_file("app/Target.php", source);
        h.add_project("/project/app");
        assert!(target(&project, "/project", &q, selected).is_err());
    }
    #[test]
    fn explicit_scan_refuses_path_match_and_total_byte_incompleteness() {
        for case in 0..3 {
            let source = "<?php route('same');";
            let (h, _) = setup(source);
            let project = h.invocation().project().unwrap();
            let q = query(source);
            match case {
                0 => {
                    for n in 0..PATHS {
                        h.add_file(&format!("app/F{n}.php"), "<?php");
                    }
                }
                1 => h.add_file(
                    "app/Many.php",
                    &format!("<?php {}", "route('same');".repeat(MATCHES + 1)),
                ),
                _ => {
                    for n in 0..65 {
                        h.add_file(&format!("app/F{n}.php"), &" ".repeat(SOURCE));
                    }
                }
            }
            assert!(scan(&project, "/project", &q).is_err(), "cap case {case}");
        }
    }
    #[test]
    fn command_cancel_and_unsupported_queries_never_navigate_or_execute() {
        for source in [
            "<?php route('same' . $suffix);",
            "<?php route(\"same$value\");",
            "<?php route('same\\n');",
            "<?php env('same');",
        ] {
            let (h, mut main) = setup(source);
            assert!(
                main.run_command("find-literal-source-matches", &h.invocation(), None)
                    .is_err()
            );
            assert!(h.opened_locations().is_empty() && h.process_runs().is_empty());
        }
        let (h, mut main) = setup("<?php route('same');");
        h.answer_next_prompt(None);
        assert!(
            main.run_command("find-literal-source-matches", &h.invocation(), None)
                .is_ok()
        );
        assert!(h.opened_locations().is_empty());
    }
    #[test]
    fn query_generation_and_current_nested_project_identity_control_navigation() {
        let source = "<?php route('same');";
        let (h, mut main) = setup(source);
        let project = h.invocation().project().unwrap();
        let editor = h.invocation().editor().unwrap();
        let mut q = query(source);
        q.version = h.editor_version();
        assert!(unchanged(&editor, &project, "/project", &q).is_ok());
        h.type_in_editor("<?php route('other');");
        assert!(unchanged(&editor, &project, "/project", &q).is_err());
        let at = text::position(source, source.find("same").unwrap());
        h.add_project("/project/app/Nested");
        h.add_file("/project/app/Nested/routes/Query.php", source);
        h.add_file("/project/app/Nested/app/Target.php", source);
        h.add_file("/project/app/Outer.php", source);
        h.open_editor(
            "/project/app/Nested/routes/Query.php",
            "php",
            source,
            &[(at.line, at.character, at.line, at.character)],
        );
        h.answer_next_prompt(Some(&["0"]));
        main.run_command("find-literal-source-matches", &h.invocation(), None)
            .unwrap();
        assert_eq!(h.opened_locations().len(), 1);
        assert_eq!(
            h.opened_locations()[0].0,
            "/project/app/Nested/app/Target.php"
        );
        let h = h.with_capabilities(&["editor.read"]);
        assert!(
            main.run_command("find-literal-source-matches", &h.invocation(), None)
                .is_err()
        );
        assert_eq!(h.opened_locations().len(), 1);
    }
}
