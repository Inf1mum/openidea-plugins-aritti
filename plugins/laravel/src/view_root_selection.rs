//! Explicit destination selection from current owned declarative view roots.
//! This module never evaluates application PHP or guesses dynamic registrations.
use super::{fingerprint, missing, owner, valid_name};
use crate::php;
use openidea_plugin::{Error, FileKind, Project, host, providers::Document};
use std::collections::BTreeSet;
#[derive(Clone)]
struct Roots {
    paths: Vec<(Option<String>, String)>,
    proof: String,
}
fn namespace(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !matches!(value, "." | "..")
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}
pub(crate) fn split(name: &str) -> Option<(Option<&str>, &str)> {
    if let Some((ns, name)) = name.split_once("::") {
        if namespace(ns) && valid_name(name) {
            Some((Some(ns), name))
        } else {
            None
        }
    } else {
        valid_name(name).then_some((None, name))
    }
}
fn path_owned(project: &Project, root: &str, path: &str) -> Result<(), Error> {
    if host::project_of(path)
        .and_then(|p| p.root().ok())
        .as_deref()
        != Some(root)
    {
        return Err(Error::Stale);
    }
    let mut parent = root.to_owned();
    let relative = path.strip_prefix(&format!("{root}/")).ok_or(Error::Stale)?;
    if relative
        .split('/')
        .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err(Error::Stale);
    }
    for part in relative.split('/') {
        parent.push('/');
        parent.push_str(part);
        match project.stat(&parent) {
            Ok(stat) if stat.kind == FileKind::Symlink => return Err(Error::Stale),
            Err(Error::NotFound(_)) => {}
            Err(error) => return Err(error),
            _ => {}
        }
    }
    Ok(())
}
fn configured_path(source: &str, path: &str) -> Option<String> {
    crate::metadata::resolve_path(source, path)
}
fn php_path(root: &str, expression: &str) -> Option<String> {
    let expression = expression.trim();
    for (function, base) in [("resource_path", "resources"), ("base_path", "")] {
        if let Some(tail) = expression.strip_prefix(function) {
            let tail = tail.trim();
            let open = tail.starts_with('(').then_some(0)?;
            let close = php::closing(tail, open)?;
            if !tail[close + 1..].trim().is_empty() {
                return None;
            }
            let args = php::arguments(tail, open);
            if args.len() > 1 {
                return None;
            }
            let suffix = if let Some(arg) = args.first() {
                php::literal(tail, arg.clone())?.0
            } else {
                ""
            };
            return configured_path("config/view.php", &format!("/{base}/{suffix}"));
        }
    }
    if let Some((value, _)) = php::literal(expression, 0..expression.len()) {
        return value
            .strip_prefix(&format!("{root}/"))
            .and_then(|relative| configured_path("ide.json", &format!("/{relative}")));
    }
    let absolute = crate::vendor_overrides::registration_directory(
        &format!("{root}/config/view.php"),
        expression,
    )?;
    absolute
        .strip_prefix(&format!("{root}/"))
        .and_then(|relative| configured_path("ide.json", &format!("/{relative}")))
}
fn config_roots(root: &str, source: &str) -> Result<Vec<String>, Error> {
    let code = php::mask_comments(source);
    if code.contains("<<<") {
        return Err("Dynamic view configuration requires explicit runtime evidence.".into());
    }
    let open =
        php::returned_array(&code).ok_or("View configuration must return a literal array.")?;
    let prefix: String = code[..open]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if !matches!(
        prefix.as_str(),
        "<?phpreturn" | "<?phpdeclare(strict_types=1);return"
    ) {
        return Err("Executable view configuration is not a declarative destination proof.".into());
    }
    let end = php::closing(&code, open).ok_or(Error::Stale)?;
    if !matches!(code[end + 1..].trim(), ";" | ";?>") {
        return Err(Error::Stale);
    }
    let mut paths = None;
    for item in php::items(&code, open) {
        let key = item
            .key
            .and_then(|key| php::literal(&code, key))
            .ok_or("Computed view configuration keys are not destination proofs.")?
            .0;
        if key == "paths" {
            if paths.is_some() {
                return Err("Duplicate view path declarations are ambiguous.".into());
            }
            paths = Some(item.value);
        }
    }
    let range = paths.ok_or("No literal view paths declaration.")?;
    let array = php::array_open(&code, range.clone())
        .ok_or("Computed view roots require explicit runtime evidence.")?;
    if php::closing(&code, array).map(|end| end + 1) != Some(php::trimmed(&code, range).end) {
        return Err(Error::Stale);
    }
    let mut roots = vec![];
    for item in php::items(&code, array) {
        if item.key.is_some() {
            return Err("Named view root entries are unsupported.".into());
        }
        roots.push(
            php_path(root, &code[item.value])
                .ok_or("Computed view roots require explicit runtime evidence.")?,
        );
    }
    Ok(roots)
}
fn load(project: &Project, root: &str) -> Result<Roots, Error> {
    let mut paths = Vec::new();
    let mut evidence = Vec::new();
    let setting = host::setting_string("viewPaths").unwrap_or_default();
    for value in setting.split(',').map(str::trim).filter(|v| !v.is_empty()) {
        paths.push((
            None,
            configured_path("ide.json", &format!("/{value}")).ok_or(crate::ui_text::text(
                "native.view_root_selection.24008916a8",
                "Invalid configured view root.",
            ))?,
        ));
    }
    let metadata = crate::metadata::Metadata::load(project);
    paths.extend(metadata.view_roots.clone());
    evidence.push(fingerprint(&format!("{metadata:?}")));
    for source in ["ide-blade.json", ".idea/ide-blade.json"] {
        let absolute = format!("{root}/{source}");
        path_owned(project, root, &absolute)?;
        match project.read_file(&absolute) {
            Ok(text) => {
                let json = crate::strict_json::parse(&text, 128 * 1024)?;
                let entries = json["namespaces"].as_array().ok_or(crate::ui_text::text(
                    "native.view_root_selection.7bc8aa247f",
                    "ide-blade.json namespaces must be a literal array.",
                ))?;
                for entry in entries {
                    let ns = entry
                        .get("namespace")
                        .and_then(serde_json::Value::as_str)
                        .filter(|s| !s.is_empty());
                    if ns.is_some_and(|ns| !namespace(ns)) {
                        return Err(crate::ui_text::text(
                            "native.view_root_selection.c3dc366902",
                            "Invalid view namespace.",
                        )
                        .into());
                    }
                    let path = entry
                        .get("path")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|value| configured_path("ide-blade.json", &format!("/{value}")))
                        .ok_or(crate::ui_text::text(
                            "native.view_root_selection.7e0ebffc79",
                            "Invalid declared view root.",
                        ))?;
                    paths.push((ns.map(str::to_owned), path));
                }
                evidence.push(fingerprint(&text));
            }
            Err(Error::NotFound(_)) => evidence.push("absent".into()),
            Err(error) => return Err(error),
        }
    }
    let config = format!("{root}/config/view.php");
    path_owned(project, root, &config)?;
    match project.read_file(&config) {
        Ok(source) => {
            if source.len() > 128 * 1024 {
                return Err(crate::ui_text::text(
                    "native.view_root_selection.ea66946b03",
                    "View configuration exceeds the review bound.",
                )
                .into());
            }
            paths.extend(
                config_roots(root, &source)
                    .map_err(config_ui_error)?
                    .into_iter()
                    .map(|p| (None, p)),
            );
            evidence.push(fingerprint(&source));
        }
        Err(Error::NotFound(_)) => {
            for directory in ["resources/views", "app/views", "resources/templates"] {
                let path = format!("{root}/{directory}");
                path_owned(project, root, &path)?;
                if project
                    .stat(&path)
                    .is_ok_and(|s| s.kind == FileKind::Directory)
                {
                    paths.push((None, directory.into()));
                }
            }
            evidence.push("default".into());
        }
        Err(error) => return Err(error),
    }
    paths.sort();
    paths.dedup();
    if paths.len() > 64 {
        return Err(crate::ui_text::text(
            "native.view_root_selection.ff33e74476",
            "Too many view roots for one reviewed creation.",
        )
        .into());
    }
    for (ns, path) in &paths {
        if ns.as_deref().is_some_and(|value| !namespace(value)) {
            return Err(crate::ui_text::text(
                "native.view_root_selection.c3dc366902",
                "Invalid view namespace.",
            )
            .into());
        }
        path_owned(project, root, &format!("{root}/{path}"))?;
    }
    let proof = fingerprint(&serde_json::json!([paths, evidence, setting]).to_string());
    Ok(Roots { paths, proof })
}
fn candidates(
    project: &Project,
    root: &str,
    doc: &Document,
    name: &str,
) -> Result<(Vec<String>, String), Error> {
    owner(doc, Some(root))?;
    if project.read_file(&doc.path)? != doc.text {
        return Err(Error::Stale);
    }
    let (namespace, name) = split(name).ok_or(crate::ui_text::text(
        "native.view_root_selection.fc4be19d2e",
        "Invalid view name or namespace.",
    ))?;
    let roots = load(project, root)?;
    let leaf = name.replace('.', "/");
    let mut result = BTreeSet::new();
    if namespace.is_none() {
        for directory in ["resources/views", "app/views", "resources/templates"] {
            for extension in ["blade.php", "php"] {
                missing(project, &format!("{root}/{directory}/{leaf}.{extension}"))?;
            }
        }
    }
    for (ns, path) in roots.paths {
        if ns.as_deref() != namespace {
            continue;
        }
        for extension in ["blade.php", "php"] {
            missing(project, &format!("{root}/{path}/{leaf}.{extension}"))?;
        }
        let target = format!("{root}/{path}/{leaf}.blade.php");
        path_owned(project, root, &target)?;
        let mut parent = root.to_owned();
        for part in target
            .strip_prefix(&format!("{root}/"))
            .ok_or(Error::Stale)?
            .split('/')
            .collect::<Vec<_>>()
            .split_last()
            .ok_or(Error::Stale)?
            .1
        {
            parent.push('/');
            parent.push_str(part);
            match project.stat(&parent) {
                Ok(stat) if stat.kind != FileKind::Directory => {
                    return Err(crate::ui_text::text(
                        "native.view_root_selection.2f5e244eef",
                        "A view destination ancestor is not a plain directory.",
                    )
                    .into());
                }
                Err(Error::NotFound(_)) => {}
                Err(error) => return Err(error),
                _ => {}
            }
        }
        result.insert(target);
    }
    if result.is_empty() {
        return Err(crate::ui_text::text(
            "native.view_root_selection.817f7fc260",
            "No declared owned view root matches this namespace.",
        )
        .into());
    }
    Ok((result.into_iter().collect(), roots.proof))
}
pub(crate) fn select(
    project: &Project,
    root: &str,
    doc: &Document,
    name: &str,
) -> Result<(String, String), Error> {
    let (choices, proof) = candidates(project, root, doc, name)?;
    let target = if choices.len() == 1 {
        choices[0].clone()
    } else {
        let items: Vec<_> = choices
            .iter()
            .map(|path| openidea_plugin::PickItem::new(path, path))
            .collect();
        host::pick(
            crate::ui_text::text(
                "native.view_root_selection.2b472a463f",
                "Choose declared view destination",
            ),
            &items,
        )?
        .filter(|path| choices.contains(path))
        .ok_or(Error::Cancelled)?
    };
    Ok((target, proof))
}
pub(crate) fn revalidate(
    project: &Project,
    root: &str,
    doc: &Document,
    name: &str,
    target: &str,
    proof: &str,
) -> Result<(String, String), Error> {
    let (choices, current) = candidates(project, root, doc, name)?;
    if current != proof || !choices.iter().any(|path| path == target) {
        return Err(Error::Stale);
    }
    Ok((target.into(), current))
}
pub(super) fn known_namespace(index: &crate::Index, doc: &Document, namespace: &str) -> bool {
    if index
        .metadata
        .view_roots
        .iter()
        .any(|(ns, _)| ns.as_deref() == Some(namespace))
    {
        return true;
    }
    let Ok((project, root)) = owner(doc, None) else {
        return false;
    };
    ["ide-blade.json", ".idea/ide-blade.json"]
        .iter()
        .any(|source| {
            let path = format!("{root}/{source}");
            if path_owned(&project, &root, &path).is_err() {
                return false;
            }
            project
                .read_file(&path)
                .ok()
                .filter(|source| source.len() <= 128 * 1024)
                .and_then(|source| crate::strict_json::parse(&source, 128 * 1024).ok())
                .is_some_and(|value| {
                    value["namespaces"].as_array().is_some_and(|entries| {
                        entries.iter().any(|entry| entry["namespace"] == namespace)
                    })
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Main;
    use openidea_plugin::{
        Position, Range,
        file_actions::{FileActions, FileOperation},
        mock::MockHost,
    };
    fn fixture(source: &str) -> (MockHost, Document) {
        let host = MockHost::install();
        host.add_dir("resources/views");
        host.add_file("app/Page.php", source);
        (
            host,
            Document {
                path: "/project/app/Page.php".into(),
                language: "php".into(),
                version: 1,
                text: source.into(),
            },
        )
    }
    #[test]
    fn explicit_multiple_roots_choose_and_revalidate_exact_current_metadata() {
        let (host, doc) = fixture("<?php return view('reports.new');");
        host.set_setting("viewPaths", "\"custom/views\"");
        let project = host.invocation().project().unwrap();
        host.answer_next_prompt(Some(&["/project/custom/views/reports/new.blade.php"]));
        let (target, proof) = select(&project, "/project", &doc, "reports.new").unwrap();
        assert_eq!(target, "/project/custom/views/reports/new.blade.php");
        assert!(revalidate(&project, "/project", &doc, "reports.new", &target, &proof).is_ok());
        host.set_setting("viewPaths", "\"different/views\"");
        assert!(revalidate(&project, "/project", &doc, "reports.new", &target, &proof).is_err());
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn literal_config_paths_support_framework_helpers_without_evaluating_php() {
        let (host, doc) = fixture("<?php return view('reports.new');");
        host.add_file("config/view.php","<?php return ['paths'=>[base_path('modules/Blog/views')], 'compiled'=>env('VIEW_CACHE')];");
        let (target, _) = select(
            &host.invocation().project().unwrap(),
            "/project",
            &doc,
            "reports.new",
        )
        .unwrap();
        assert_eq!(target, "/project/modules/Blog/views/reports/new.blade.php");
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn declared_namespace_missing_view_action_creates_only_in_its_proven_owned_root() {
        let source = "<?php return view('blog::posts.create');";
        let (host, doc) = fixture(source);
        host.add_file(
            "ide-blade.json",
            r#"{"namespaces":[{"namespace":"blog","path":"modules/blog/views"}]}"#,
        );
        let mut main = Main::default();
        main.indexes
            .insert("/project".into(), crate::Index::default());
        let at = openidea_plugin::text::position(source, source.find("posts.create").unwrap() + 2);
        let lazy = main
            .file_actions(&doc, Range::new(at, at))
            .into_iter()
            .find(|action| action.title.contains("missing literal Blade view"))
            .unwrap();
        host.answer_next_prompt(Some(&["yes"]));
        let action = main.resolve_file_action(&doc, lazy).unwrap();
        let [
            FileOperation::Create {
                path, overwrite, ..
            },
        ] = action.operations.as_slice()
        else {
            panic!("create");
        };
        assert_eq!(path, "/project/modules/blog/views/posts/create.blade.php");
        assert!(!overwrite);
    }
    #[test]
    fn dynamic_roots_foreign_owners_and_target_collisions_do_not_produce_choices() {
        for mode in 0..3 {
            let (host, doc) = fixture("<?php return view('reports.new');");
            let project = host.invocation().project().unwrap();
            match mode {
                0 => host.add_file(
                    "config/view.php",
                    "<?php return ['paths'=>dynamic_paths()];",
                ),
                1 => {
                    host.set_setting("viewPaths", "\"nested/views\"");
                    host.add_project("/project/nested");
                }
                _ => host.add_file("resources/views/reports/new.php", "existing"),
            }
            assert!(select(&project, "/project", &doc, "reports.new").is_err());
        }
        let _ = Position::new(0, 0);
    }
}

fn config_ui_error(error: openidea_plugin::Error) -> openidea_plugin::Error {
    let translate = |message: String| match message.as_str() {
        "Dynamic view configuration requires explicit runtime evidence." => {
            crate::ui_text::t("native.view_root_selection.1138028f05", &message)
        }
        "View configuration must return a literal array." => {
            crate::ui_text::t("native.view_root_selection.f6baa02e22", &message)
        }
        "Executable view configuration is not a declarative destination proof." => {
            crate::ui_text::t("native.view_root_selection.3232a79c37", &message)
        }
        "Computed view configuration keys are not destination proofs." => {
            crate::ui_text::t("native.view_root_selection.0c64d504c9", &message)
        }
        "Duplicate view path declarations are ambiguous." => {
            crate::ui_text::t("native.view_root_selection.f195fb7b75", &message)
        }
        "No literal view paths declaration." => {
            crate::ui_text::t("native.view_creation.5d610aed1f", &message)
        }
        "Computed view roots require explicit runtime evidence." => {
            crate::ui_text::t("native.view_root_selection.0ecec39304", &message)
        }
        "Named view root entries are unsupported." => {
            crate::ui_text::t("native.view_root_selection.f1108e0951", &message)
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
