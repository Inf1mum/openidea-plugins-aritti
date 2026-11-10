//! Reviewed vendor overrides backed by literal provider registrations.
//! Never infer a Laravel namespace from a Composer package name.
use crate::{Main, php};
use openidea_plugin::{
    Error, FileKind, Project, Range,
    file_actions::{FileAction, FileOperation},
    host,
    providers::Document,
};
use serde_json::json;
const PREFIX: &str = "vendor-override:";
const LIMIT: usize = 128 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Registration {
    namespace: String,
    directory: String,
    translations: bool,
}
fn component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
fn clean_path(path: &str) -> Option<String> {
    if !path.starts_with('/') || path.contains(['\\', '\0', '\n', '\r']) {
        return None;
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(format!("/{}", parts.join("/")))
}
fn registration_directory(provider: &str, expression: &str) -> Option<String> {
    let expression = expression.trim();
    let tail = expression
        .strip_prefix("__DIR__")?
        .trim()
        .strip_prefix('.')?
        .trim();
    let (suffix, _) = php::literal(tail, 0..tail.len())?;
    if !suffix.starts_with('/') || suffix.contains(['\\', '$']) {
        return None;
    }
    clean_path(&format!("{}{suffix}", provider.rsplit_once('/')?.0))
}
fn registrations(provider: &str, source: &str) -> Vec<Registration> {
    if source.len() > LIMIT
        || !source.trim_start().starts_with("<?php")
        || source.contains("<<<")
        || source.contains("?>")
    {
        return vec![];
    }
    let masked = php::mask_comments(source);
    let bytes = masked.as_bytes();
    let mut at = 0;
    let mut out = vec![];
    while at < bytes.len() {
        if matches!(bytes[at], b'\'' | b'"') {
            at = php::string_end(bytes, at);
            continue;
        }
        for (method, translations) in [
            ("$this->loadViewsFrom", false),
            ("$this->loadTranslationsFrom", true),
        ] {
            if !masked[at..].starts_with(method) {
                continue;
            }
            let open = at + method.len();
            let spaces = masked[open..].len() - masked[open..].trim_start().len();
            let open = open + spaces;
            if bytes.get(open) != Some(&b'(') {
                continue;
            }
            let args = php::arguments(&masked, open);
            if args.len() != 2 {
                continue;
            }
            let Some((namespace, _)) = php::literal(&masked, args[1].clone()) else {
                continue;
            };
            if !component(namespace) {
                continue;
            }
            let Some(directory) = registration_directory(provider, &masked[args[0].clone()]) else {
                continue;
            };
            out.push(Registration {
                namespace: namespace.into(),
                directory,
                translations,
            });
        }
        at += 1;
        while at < bytes.len() && !masked.is_char_boundary(at) {
            at += 1;
        }
    }
    out
}
fn package(root: &str, path: &str) -> Option<String> {
    let relative = path.strip_prefix(&format!("{root}/vendor/"))?;
    let mut p = relative.split('/');
    let vendor = p.next()?;
    let name = p.next()?;
    if !component(vendor) || !component(name) || p.next().is_none() {
        return None;
    }
    Some(format!("{root}/vendor/{vendor}/{name}"))
}
fn target(root: &str, source: &str, reg: &Registration) -> Option<String> {
    let relative = source.strip_prefix(&format!("{}/", reg.directory))?;
    if relative
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    if reg.translations {
        let (locale, file) = relative.split_once('/')?;
        if !component(locale) || !file.ends_with(".php") {
            return None;
        }
        Some(format!(
            "{root}/lang/vendor/{}/{locale}/{file}",
            reg.namespace
        ))
    } else {
        if !relative.ends_with(".php") {
            return None;
        }
        Some(format!(
            "{root}/resources/views/vendor/{}/{relative}",
            reg.namespace
        ))
    }
}
fn owned(project: &Project, root: &str, path: &str) -> Result<(), Error> {
    if host::project_of(path)
        .and_then(|p| p.root().ok())
        .as_deref()
        != Some(root)
    {
        return Err(Error::Stale);
    }
    let mut ancestor = root.to_owned();
    for part in path
        .strip_prefix(&format!("{root}/"))
        .ok_or(Error::Stale)?
        .split('/')
    {
        ancestor.push('/');
        ancestor.push_str(part);
        match project.stat(&ancestor) {
            Ok(stat) if stat.kind == FileKind::Symlink => return Err(Error::Stale),
            Err(Error::NotFound(_)) => {}
            Err(e) => return Err(e),
            _ => {}
        }
    }
    Ok(())
}
fn fingerprint(source: &str) -> String {
    format!(
        "{:016x}",
        source
            .bytes()
            .fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(b))
                .wrapping_mul(0x100000001b3))
    )
}

pub(crate) fn actions(_main: &mut Main, doc: &Document, _selection: Range) -> Vec<FileAction> {
    let Ok((_, root)) = crate::view_creation::owner(doc, None) else {
        return vec![];
    };
    if doc.text.len() > LIMIT || package(&root, &doc.path).is_none() || !doc.path.ends_with(".php")
    {
        return vec![];
    }
    vec![FileAction {
        title: "Create reviewed application override from vendor file…".into(),
        kind: Some("refactor.rewrite".into()),
        data: Some(format!(
            "{PREFIX}{}",
            json!([root, doc.path, doc.version, fingerprint(&doc.text)])
        )),
        ..Default::default()
    }]
}
pub(crate) fn handles(action: &FileAction) -> bool {
    action
        .data
        .as_deref()
        .is_some_and(|data| data.starts_with(PREFIX))
}
pub(crate) fn resolve(
    _main: &mut Main,
    doc: &Document,
    action: FileAction,
) -> Result<FileAction, Error> {
    let encoded = action
        .data
        .as_deref()
        .and_then(|s| s.strip_prefix(PREFIX))
        .filter(|s| s.len() < 16384)
        .ok_or(Error::Stale)?;
    let saved: serde_json::Value = serde_json::from_str(encoded).map_err(|_| Error::Stale)?;
    let (project, root) = crate::view_creation::owner(doc, saved[0].as_str())?;
    if saved != json!([root, doc.path, doc.version, fingerprint(&doc.text)])
        || doc.text.len() > LIMIT
    {
        return Err(Error::Stale);
    }
    owned(&project, &root, &doc.path)?;
    if project.read_file(&doc.path)? != doc.text {
        return Err(Error::Stale);
    }
    let package = package(&root, &doc.path).ok_or(Error::Stale)?;
    let files = project.find_files(
        &format!(
            "{}/**/*.php",
            package
                .strip_prefix(&format!("{root}/"))
                .ok_or(Error::Stale)?
        ),
        65,
    )?;
    if files.len() > 64 {
        return Err("Package exceeds the bounded provider scan; use an explicit configured override workflow.".into());
    }
    let mut candidates = Vec::new();
    let mut total = 0;
    for path in files {
        let path = if path.starts_with('/') {
            path
        } else {
            format!("{root}/{path}")
        };
        if !path.starts_with(&format!("{package}/")) {
            return Err(Error::Stale);
        }
        owned(&project, &root, &path)?;
        let source = project.read_file(&path)?;
        total += source.len();
        if total > 512 * 1024 {
            return Err("Package source exceeds the bounded provider scan.".into());
        }
        for reg in registrations(&path, &source) {
            if !reg.directory.starts_with(&format!("{package}/")) {
                continue;
            }
            if let Some(destination) = target(&root, &doc.path, &reg) {
                candidates.push((destination, path.clone(), source.clone()));
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    if candidates.len() != 1 {
        return Err("A single literal loadViewsFrom/loadTranslationsFrom registration must prove this file's override namespace and root.".into());
    }
    let (destination, provider, provider_source) = candidates.remove(0);
    owned(&project, &root, &destination)?;
    crate::view_creation::missing(&project, &destination)?;
    let preview = format!(
        "Source: {}\nRegistration: {provider}\nCreate (no overwrite): {destination}\n\nExact file contents:\n{}\n\nApply one undoable file creation?",
        doc.path, doc.text
    );
    if !ui_kit::prompts::confirm_exact("Create vendor override?", &preview)? {
        return Err(Error::Cancelled);
    }
    crate::view_creation::owner(doc, Some(&root))?;
    owned(&project, &root, &doc.path)?;
    owned(&project, &root, &provider)?;
    owned(&project, &root, &destination)?;
    if project.read_file(&doc.path)? != doc.text || project.read_file(&provider)? != provider_source
    {
        return Err(Error::Stale);
    }
    crate::view_creation::missing(&project, &destination)?;
    Ok(FileAction {
        title: action.title,
        operations: vec![FileOperation::Create {
            path: destination.clone(),
            text: doc.text.clone(),
            overwrite: false,
        }],
        open: Some(destination),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_registration_proves_view_and_translation_destinations() {
        let source = "<?php class P { function boot() { $this->loadViewsFrom(__DIR__.'/../resources/views', 'billing'); $this->loadTranslationsFrom(__DIR__.'/../lang', 'billing'); } }";
        let regs = registrations("/project/vendor/acme/pkg/src/P.php", source);
        assert_eq!(regs.len(), 2);
        assert_eq!(
            target(
                "/project",
                "/project/vendor/acme/pkg/resources/views/invoice.blade.php",
                &regs[0]
            )
            .as_deref(),
            Some("/project/resources/views/vendor/billing/invoice.blade.php")
        );
        assert_eq!(
            target(
                "/project",
                "/project/vendor/acme/pkg/lang/fr/messages.php",
                &regs[1]
            )
            .as_deref(),
            Some("/project/lang/vendor/billing/fr/messages.php")
        );
    }
    #[test]
    fn strings_comments_dynamic_paths_and_namespaces_do_not_prove_registration() {
        for source in [
            "<?php // $this->loadViewsFrom(__DIR__.'/views', 'x');",
            "<?php $s=\"$this->loadViewsFrom(__DIR__.'/views', 'x');\";",
            "<?php $this->loadViewsFrom($path, 'x');",
            "<?php $this->loadViewsFrom(__DIR__.'/views', '../x');",
        ] {
            assert!(registrations("/project/vendor/a/b/P.php", source).is_empty());
        }
    }
    #[test]
    fn matching_requires_directory_boundary_and_valid_locale() {
        let reg = Registration {
            namespace: "x".into(),
            directory: "/project/vendor/a/b/lang".into(),
            translations: true,
        };
        assert!(target("/project", "/project/vendor/a/b/language/en/foo.php", &reg).is_none());
        assert!(target("/project", "/project/vendor/a/b/lang/../foo.php", &reg).is_none());
        assert!(clean_path("/../../x").is_none());
    }
    fn fixture() -> (openidea_plugin::mock::MockHost, Main, Document) {
        let h = openidea_plugin::mock::MockHost::install();
        h.add_file("vendor/acme/pkg/src/P.php", "<?php class P { function boot() { $this->loadViewsFrom(__DIR__.'/../resources/views', 'billing'); } }");
        h.add_file(
            "vendor/acme/pkg/resources/views/invoice.blade.php",
            "<p>Invoice 😀</p>\n",
        );
        let doc = Document {
            path: "/project/vendor/acme/pkg/resources/views/invoice.blade.php".into(),
            language: "blade".into(),
            version: 1,
            text: "<p>Invoice 😀</p>\n".into(),
        };
        (h, Main::default(), doc)
    }
    #[test]
    fn reviewed_override_returns_one_non_overwriting_create_without_writing() {
        let (h, mut main, doc) = fixture();
        let range = Range::new(
            openidea_plugin::Position::new(0, 0),
            openidea_plugin::Position::new(0, 0),
        );
        let action = actions(&mut main, &doc, range).remove(0);
        h.answer_next_prompt(Some(&["yes"]));
        let fixed = resolve(&mut main, &doc, action).unwrap();
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
        assert_eq!(
            path,
            "/project/resources/views/vendor/billing/invoice.blade.php"
        );
        assert_eq!(text, &doc.text);
        assert!(!overwrite);
        assert!(fixed.edits.is_empty());
        assert!(!h.invocation().project().unwrap().exists(path).unwrap());
    }
    #[test]
    fn collisions_stale_sources_and_cancelled_review_never_return_operations() {
        for mode in ["collision", "stale", "cancel"] {
            let (h, mut main, doc) = fixture();
            let range = Range::new(
                openidea_plugin::Position::new(0, 0),
                openidea_plugin::Position::new(0, 0),
            );
            let action = actions(&mut main, &doc, range).remove(0);
            if mode == "collision" {
                h.add_file(
                    "resources/views/vendor/billing/invoice.blade.php",
                    "existing",
                );
            }
            if mode == "stale" {
                h.add_file(
                    "vendor/acme/pkg/resources/views/invoice.blade.php",
                    "changed",
                );
            }
            h.answer_next_prompt(None);
            assert!(resolve(&mut main, &doc, action).is_err(), "{mode}");
        }
    }
}
