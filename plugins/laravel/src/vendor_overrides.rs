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
    json: bool,
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
pub(crate) fn registration_directory(provider: &str, expression: &str) -> Option<String> {
    fn evaluate(provider: &str, expression: &str, depth: usize) -> Option<String> {
        if depth > 8 {
            return None;
        }
        let expression = expression.trim();
        let (mut base, mut tail) = if let Some(tail) = expression.strip_prefix("__DIR__") {
            (provider.rsplit_once('/')?.0.to_owned(), tail)
        } else if let Some(tail) = expression.strip_prefix("__FILE__") {
            (provider.to_owned(), tail)
        } else if expression.starts_with("dirname(") {
            let close = php::closing(expression, 7)?;
            let args = php::arguments(expression, 7);
            if args.is_empty() || args.len() > 2 {
                return None;
            }
            let mut base = evaluate(provider, &expression[args[0].clone()], depth + 1)?;
            let count = if args.len() == 2 {
                expression[args[1].clone()].trim().parse::<usize>().ok()?
            } else {
                1
            };
            if !(1..=8).contains(&count) {
                return None;
            }
            for _ in 0..count {
                base = base.rsplit_once('/')?.0.to_owned();
            }
            (base, &expression[close + 1..])
        } else if let Some((path, _)) = php::literal(expression, 0..expression.len()) {
            return clean_path(path);
        } else {
            return None;
        };
        loop {
            tail = tail.trim();
            if tail.is_empty() {
                return clean_path(&base);
            }
            tail = tail.strip_prefix('.')?.trim_start();
            let quote = *tail.as_bytes().first()?;
            if !matches!(quote, b'\'' | b'"') {
                return None;
            }
            let end = php::string_end(tail.as_bytes(), 0);
            let (suffix, _) = php::literal(tail, 0..end)?;
            if !suffix.starts_with('/') || suffix.contains(['\\', '$']) {
                return None;
            }
            base.push_str(suffix);
            tail = &tail[end..];
        }
    }
    evaluate(provider, expression, 0)
}
fn provider_files(project: &Project, root: &str, package: &str) -> Result<Vec<String>, Error> {
    let relative = package
        .strip_prefix(&format!("{root}/"))
        .ok_or(Error::Stale)?;
    let mut paths = std::collections::BTreeSet::new();
    for path in project.find_files(&format!("{relative}/**/*Provider.php"), 129)? {
        paths.insert(if path.starts_with('/') {
            path
        } else {
            format!("{root}/{path}")
        });
    }
    if paths.len() > 128 {
        return Err(crate::ui_text::text(
            "native.vendor_overrides.82bd2c2b2d",
            "Package has too many provider candidates.",
        )
        .into());
    }
    let composer_path = format!("{package}/composer.json");
    owned(project, root, &composer_path)?;
    if let Ok(source) = project.read_file(&composer_path) {
        let metadata = crate::strict_json::parse(&source, LIMIT)?;
        for provider in metadata["extra"]["laravel"]["providers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
        {
            if !provider.split('\\').all(|part| {
                !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            }) {
                continue;
            }
            for (namespace, roots) in metadata["autoload"]["psr-4"]
                .as_object()
                .into_iter()
                .flatten()
            {
                let Some(tail) = provider.strip_prefix(namespace) else {
                    continue;
                };
                let roots: Vec<_> = roots
                    .as_str()
                    .into_iter()
                    .chain(
                        roots
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(serde_json::Value::as_str),
                    )
                    .collect();
                for source_root in roots {
                    let Some(path) = clean_path(&format!(
                        "{package}/{source_root}/{}.php",
                        tail.replace('\\', "/")
                    )) else {
                        continue;
                    };
                    if path.starts_with(&format!("{package}/")) {
                        paths.insert(path);
                    }
                }
            }
        }
    }
    if paths.is_empty() {
        for path in project.find_files(&format!("{relative}/**/*.php"), 65)? {
            paths.insert(if path.starts_with('/') {
                path
            } else {
                format!("{root}/{path}")
            });
        }
        if paths.len() > 64 {
            return Err(crate::ui_text::text(
                "native.vendor_overrides.d25e401537",
                "No explicit providers were discoverable within the package scan budget.",
            )
            .into());
        }
    }
    if paths.len() > 128 {
        return Err(crate::ui_text::text(
            "native.vendor_overrides.21af692f07",
            "Package provider discovery exceeds the supported bound.",
        )
        .into());
    }
    Ok(paths.into_iter().collect())
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
        for (method, translations, json) in [
            ("$this->loadViewsFrom", false, false),
            ("$this->loadTranslationsFrom", true, false),
            ("$this->loadJsonTranslationsFrom", true, true),
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
            if args.len() != if json { 1 } else { 2 } {
                continue;
            }
            let namespace = if json {
                ""
            } else {
                let Some((namespace, _)) = php::literal(&masked, args[1].clone()) else {
                    continue;
                };
                if !component(namespace) {
                    continue;
                }
                namespace
            };
            let directories = if let Some(open) = php::array_open(&masked, args[0].clone()) {
                php::items(&masked, open)
                    .into_iter()
                    .filter(|item| item.key.is_none())
                    .filter_map(|item| registration_directory(provider, &masked[item.value]))
                    .collect::<Vec<_>>()
            } else {
                registration_directory(provider, &masked[args[0].clone()])
                    .into_iter()
                    .collect()
            };
            for directory in directories {
                out.push(Registration {
                    namespace: namespace.into(),
                    directory,
                    translations,
                    json,
                });
            }
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
    if reg.json {
        let locale = relative.strip_suffix(".json")?;
        if !component(locale) {
            return None;
        }
        Some(format!("{root}/lang/{locale}.json"))
    } else if reg.translations {
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

#[cfg(test)]
fn merge_json_override(original: Option<&str>, vendor: &str) -> Result<String, Error> {
    merge_json_override_with(original, vendor, &mut |key| {
        Err(Error::Invalid(format!(
            "Conflicting application translation key: {key}. Existing values are never overwritten."
        )))
    })
}
fn merge_json_override_with(
    original: Option<&str>,
    vendor: &str,
    conflict: &mut dyn FnMut(&str) -> Result<bool, Error>,
) -> Result<String, Error> {
    let object = |source: &str| -> Result<serde_json::Map<String, serde_json::Value>, Error> {
        crate::strict_json::parse(source, LIMIT)?
            .as_object()
            .filter(|o| o.values().all(serde_json::Value::is_string))
            .cloned()
            .ok_or_else(|| {
                Error::Invalid(
                    "Translation JSON must contain unique string keys and string values.".into(),
                )
            })
    };
    let mut application = original.map(object).transpose()?.unwrap_or_default();
    let incoming = object(vendor)?;
    for (key, value) in incoming {
        if let Some(existing) = application.get(&key) {
            if *existing != value {
                if conflict(&key)? {
                    application.insert(key, value);
                }
            }
        } else {
            application.insert(key, value);
        }
    }
    Ok(format!(
        "{}\n",
        serde_json::to_string_pretty(&application).map_err(|e| Error::Invalid(e.to_string()))?
    ))
}
pub(crate) fn actions(_main: &mut Main, doc: &Document, _selection: Range) -> Vec<FileAction> {
    let Ok((_, root)) = crate::view_creation::owner(doc, None) else {
        return vec![];
    };
    if doc.text.len() > LIMIT
        || package(&root, &doc.path).is_none()
        || !(doc.path.ends_with(".php") || doc.path.ends_with(".json"))
    {
        return vec![];
    }
    vec![FileAction {
        title: crate::ui_text::t(
            "action.vendor-override",
            "Create reviewed application override from vendor file…",
        ),
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
    let files = provider_files(&project, &root, &package)?;
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
            return Err(crate::ui_text::text(
                "native.vendor_overrides.275aad7902",
                "Package source exceeds the bounded provider scan.",
            )
            .into());
        }
        for reg in registrations(&path, &source) {
            if !reg.directory.starts_with(&format!("{package}/")) {
                continue;
            }
            if let Some(destination) = target(&root, &doc.path, &reg) {
                candidates.push((
                    destination,
                    path.clone(),
                    source.clone(),
                    reg.json,
                    reg.translations,
                ));
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    if candidates.is_empty() {
        return Err(crate::ui_text::text(
            "native.vendor_overrides.3cb3a645ad",
            "A literal provider registration must prove the override namespace/root.",
        )
        .into());
    }
    let selected = if candidates.len() == 1 {
        0
    } else {
        let choices: Vec<_> = candidates
            .iter()
            .enumerate()
            .map(|(index, (destination, ..))| {
                openidea_plugin::PickItem::new(&index.to_string(), destination)
            })
            .collect();
        host::pick(
            crate::ui_text::text(
                "native.vendor_overrides.b310dd27a5",
                "Choose proven override destination",
            ),
            &choices,
        )?
        .and_then(|key| key.parse::<usize>().ok())
        .filter(|index| *index < candidates.len())
        .ok_or(Error::Cancelled)?
    };
    let (mut destination, provider, provider_source, json, translations) =
        candidates.remove(selected);
    let language_setting = host::setting_string("languageDirectory");
    if translations {
        let directory = crate::lang_actions::language_directory(&project, &root)?;
        destination =
            destination.replacen(&format!("{root}/lang/"), &format!("{root}/{directory}/"), 1);
    }
    owned(&project, &root, &destination)?;
    let original = if json {
        match project.read_file(&destination) {
            Ok(source) => Some(source),
            Err(Error::NotFound(_)) => {
                crate::view_creation::missing(&project, &destination)?;
                None
            }
            Err(error) => return Err(error),
        }
    } else {
        crate::view_creation::missing(&project, &destination)?;
        None
    };
    let contents = if json {
        merge_json_override_with(original.as_deref(), &doc.text, &mut |key| {
            let title = crate::ui_text::format(
                "vendor.chooseConflict",
                "Resolve application translation conflict: {key}",
                &[("key", key)],
            );
            match host::pick(
                &title,
                &[
                    openidea_plugin::PickItem::new(
                        "keep",
                        crate::ui_text::text(
                            "vendor.keepApplication",
                            "Keep the existing application value",
                        ),
                    ),
                    openidea_plugin::PickItem::new(
                        "vendor",
                        crate::ui_text::text(
                            "vendor.usePackage",
                            "Use the package value in the application override",
                        ),
                    ),
                    openidea_plugin::PickItem::new(
                        "cancel",
                        crate::ui_text::text("locale.cancelExtraction", "Cancel extraction"),
                    ),
                ],
            )?
            .as_deref()
            {
                Some("keep") => Ok(false),
                Some("vendor") => Ok(true),
                _ => Err(Error::Cancelled),
            }
        })
        .map_err(vendor_json_error)?
    } else {
        doc.text.clone()
    };
    let preview = crate::ui_text::format(
        "vendor.review",
        "Source: {source}\nRegistration: {provider}\nApplication target: {destination}\nVendor sources remain unchanged. Conflicting application JSON values follow your explicit choices.\n\nComplete application file contents:\n{contents}\n\nApply one undoable application override transaction?",
        &[
            ("source", &doc.path),
            ("provider", &provider),
            ("destination", &destination),
            ("contents", &contents),
        ],
    );
    if !ui_kit::prompts::confirm_exact(
        &crate::ui_text::t("prompt.vendor-confirm", "Create vendor override?"),
        &preview,
    )? {
        return Err(Error::Cancelled);
    }
    crate::view_creation::owner(doc, Some(&root))?;
    if translations && host::setting_string("languageDirectory") != language_setting {
        return Err(Error::Stale);
    }
    owned(&project, &root, &doc.path)?;
    owned(&project, &root, &provider)?;
    owned(&project, &root, &destination)?;
    if project.read_file(&doc.path)? != doc.text || project.read_file(&provider)? != provider_source
    {
        return Err(Error::Stale);
    }
    let mut fixed = FileAction {
        title: action.title,
        open: Some(destination.clone()),
        ..Default::default()
    };
    if let Some(original) = original {
        if project.read_file(&destination)? != original {
            return Err(Error::Stale);
        }
        fixed.edits.push((
            destination,
            vec![openidea_plugin::TextEdit {
                range: Range::new(
                    openidea_plugin::Position::new(0, 0),
                    openidea_plugin::text::position(&original, original.len()),
                ),
                text: contents,
            }],
        ));
    } else {
        crate::view_creation::missing(&project, &destination)?;
        fixed.operations.push(FileOperation::Create {
            path: destination,
            text: contents,
            overwrite: false,
        });
    }
    Ok(fixed)
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
            json: false,
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
    #[test]
    fn dirname_and_multiple_literal_view_roots_retain_exact_namespace_proof() {
        let source = "<?php class P { function boot() { $this->loadViewsFrom([dirname(__DIR__).'/resources/views', __DIR__.'/views'], 'billing'); } }";
        let regs = registrations("/project/vendor/acme/pkg/src/P.php", source);
        assert_eq!(regs.len(), 2);
        assert_eq!(
            regs[0].directory,
            "/project/vendor/acme/pkg/resources/views"
        );
        assert_eq!(regs[1].directory, "/project/vendor/acme/pkg/src/views");
        assert_eq!(
            registration_directory(
                "/project/vendor/a/b/src/P.php",
                "dirname(__FILE__, 2).'/lang'"
            )
            .as_deref(),
            Some("/project/vendor/a/b/lang")
        );
        assert!(
            registration_directory("/project/vendor/a/b/src/P.php", "dirname($path).'/lang'")
                .is_none()
        );
    }
    #[test]
    fn provider_discovery_does_not_scan_large_unrelated_package_contents() {
        let (h, mut main, doc) = fixture();
        h.add_file("vendor/acme/pkg/src/BillingServiceProvider.php","<?php class P { function boot() { $this->loadViewsFrom(dirname(__DIR__).'/resources/views', 'billing'); } }");
        for index in 0..100 {
            h.add_file(
                &format!("vendor/acme/pkg/src/Unrelated{index}.php"),
                "<?php class Other {}",
            );
        }
        let action = actions(
            &mut main,
            &doc,
            Range::new(
                openidea_plugin::Position::new(0, 0),
                openidea_plugin::Position::new(0, 0),
            ),
        )
        .remove(0);
        h.answer_next_prompt(Some(&["yes"]));
        assert_eq!(
            resolve(&mut main, &doc, action).unwrap().operations.len(),
            1
        );
    }
    #[test]
    fn json_provider_override_merges_nonconflicting_application_keys_as_one_edit() {
        let h = openidea_plugin::mock::MockHost::install();
        h.add_file("vendor/acme/pkg/src/LocaleServiceProvider.php","<?php class P { function boot() { $this->loadJsonTranslationsFrom(dirname(__DIR__).'/lang'); } }");
        h.add_file("vendor/acme/pkg/lang/fr.json", r#"{"Welcome":"Bienvenue"}"#);
        h.add_file("lang/fr.json", r#"{"Existing":"Garder"}"#);
        let doc = Document {
            path: "/project/vendor/acme/pkg/lang/fr.json".into(),
            language: "json".into(),
            version: 1,
            text: r#"{"Welcome":"Bienvenue"}"#.into(),
        };
        let mut main = Main::default();
        let range = Range::new(
            openidea_plugin::Position::new(0, 0),
            openidea_plugin::Position::new(0, 0),
        );
        let lazy = actions(&mut main, &doc, range).remove(0);
        h.answer_next_prompt(Some(&["yes"]));
        let fixed = resolve(&mut main, &doc, lazy).unwrap();
        assert!(fixed.operations.is_empty());
        assert_eq!(fixed.edits.len(), 1);
        assert_eq!(fixed.edits[0].0, "/project/lang/fr.json");
        assert!(fixed.edits[0].1[0].text.contains("Bienvenue"));
        assert!(fixed.edits[0].1[0].text.contains("Garder"));
        assert_eq!(
            h.invocation()
                .project()
                .unwrap()
                .read_file(&doc.path)
                .unwrap(),
            doc.text
        );
    }
    #[test]
    fn json_override_never_discards_conflicts_or_accepts_duplicate_json_keys() {
        let _host = openidea_plugin::mock::MockHost::install();
        assert!(
            merge_json_override(Some(r#"{"Same":"Original"}"#), r#"{"Same":"Different"}"#).is_err()
        );
        assert!(merge_json_override(None, r#"{"Same":"A","Same":"B"}"#).is_err());
        assert!(merge_json_override(None, r#"{"Key":{"nested":"value"}}"#).is_err());
        assert!(
            merge_json_override(Some(r#"{"Same":"Original"}"#), r#"{"Same":"Original"}"#)
                .unwrap()
                .contains("Original")
        );
    }
    #[test]
    fn conflicting_json_values_require_an_explicit_per_key_choice_and_exact_review() {
        for choice in ["keep", "vendor", "cancel"] {
            let host = openidea_plugin::mock::MockHost::install();
            host.add_file("vendor/acme/pkg/src/LocaleServiceProvider.php","<?php class P { function boot() { $this->loadJsonTranslationsFrom(dirname(__DIR__).'/lang'); } }");
            let source = r#"{"Welcome":"Package","New":"Added"}"#;
            let original = r#"{"Welcome":"Application","Existing":"Keep"}"#;
            host.add_file("vendor/acme/pkg/lang/fr.json", source);
            host.add_file("lang/fr.json", original);
            let doc = Document {
                path: "/project/vendor/acme/pkg/lang/fr.json".into(),
                language: "json".into(),
                version: 1,
                text: source.into(),
            };
            let mut main = Main::default();
            let lazy = actions(
                &mut main,
                &doc,
                Range::new(
                    openidea_plugin::Position::new(0, 0),
                    openidea_plugin::Position::new(0, 0),
                ),
            )
            .remove(0);
            host.answer_next_prompt(Some(&[choice]));
            host.answer_next_prompt(Some(&["yes"]));
            let result = resolve(&mut main, &doc, lazy);
            if choice == "cancel" {
                assert!(matches!(result, Err(Error::Cancelled)));
                continue;
            }
            let fixed = result.unwrap();
            assert!(fixed.operations.is_empty());
            assert_eq!(fixed.edits.len(), 1);
            let values: serde_json::Value =
                serde_json::from_str(&fixed.edits[0].1[0].text).unwrap();
            assert_eq!(
                values["Welcome"],
                if choice == "keep" {
                    "Application"
                } else {
                    "Package"
                }
            );
            assert_eq!(values["Existing"], "Keep");
            assert_eq!(values["New"], "Added");
            let project = host.invocation().project().unwrap();
            assert_eq!(project.read_file("lang/fr.json").unwrap(), original);
            assert_eq!(project.read_file(&doc.path).unwrap(), source);
        }
    }
}

fn vendor_json_error(error: openidea_plugin::Error) -> openidea_plugin::Error {
    let translate = |message: String| match message.as_str() {
        "Translation JSON must contain unique string keys and string values." => {
            crate::ui_text::t("native.vendor_overrides.c76ee1cc0c", &message)
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
