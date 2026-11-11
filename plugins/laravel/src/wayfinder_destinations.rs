//! Wayfinder navigation follows a current generated export's documented target.
//! No controller name or filesystem path is inferred from a generated module name.
use crate::{Index, index::Entry, package_facts};
use openidea_plugin::{Location, Position, Project, Range, providers::Document, text};
fn word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$') || b >= 128
}
fn reference(
    doc: &Document,
    at: Position,
) -> Option<(package_facts::WayfinderImport, Position, Option<String>)> {
    let offset = text::offset(&doc.text, at)?;
    if let Some(found) = package_facts::wayfinder_reference_at(&doc.text, offset, "@") {
        return Some((found, at, None));
    }
    let code = package_facts::code(&doc.text)?;
    let bytes = code.as_bytes();
    let mut start = offset;
    let mut end = offset;
    while start > 0 && word(bytes[start - 1]) {
        start -= 1;
    }
    while end < bytes.len() && word(bytes[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    let before = code[..start].trim_end().strip_suffix('.')?.trim_end();
    let owner_end = before.len();
    let mut owner_start = owner_end;
    while owner_start > 0 && word(bytes[owner_start - 1]) {
        owner_start -= 1;
    }
    let owner = text::position(&doc.text, owner_start);
    let found = package_facts::wayfinder_reference_at(&doc.text, owner_start, "@")?;
    (found.exported == "default").then(|| (found, owner, Some(code[start..end].into())))
}
fn export_doc<'a>(source: &'a str, export: &str) -> Option<&'a str> {
    let masked = package_facts::code(source)?;
    let target = if export == "default" {
        let mut defaults = masked.match_indices("export default ");
        let (at, _) = defaults.next()?;
        if defaults.next().is_some() {
            return None;
        }
        let rest = masked[at + 15..].trim_start();
        let len = rest.bytes().take_while(|b| word(*b)).count();
        if len == 0 {
            return None;
        }
        rest[..len].to_owned()
    } else {
        export.to_owned()
    };
    let mut declarations = Vec::new();
    for prefix in ["export const ", "export function ", "const ", "function "] {
        let needle = format!("{prefix}{target}");
        for (at, _) in masked.match_indices(&needle) {
            if at > 0 && word(masked.as_bytes()[at - 1])
                || masked
                    .as_bytes()
                    .get(at + needle.len())
                    .is_some_and(|b| word(*b))
            {
                continue;
            }
            // The shorter const/function pattern may be inside an export.
            if !prefix.starts_with("export ") && masked[..at].trim_end().ends_with("export") {
                continue;
            }
            let begin = at;
            let before = source[..begin].trim_end();
            if !before.ends_with("*/") {
                continue;
            }
            let start = before.rfind("/**")?;
            declarations.push((start, before.len()));
        }
    }
    declarations.sort();
    declarations.dedup();
    if declarations.len() != 1 {
        return None;
    }
    source.get(declarations[0].0..declarations[0].1)
}
#[derive(Debug, PartialEq, Eq)]
struct Target {
    controller: String,
    method: String,
    uri: String,
}
fn target(doc: &str) -> Option<Target> {
    let mut controllers = std::collections::BTreeSet::new();
    let mut uris = std::collections::BTreeSet::new();
    for line in doc.lines() {
        let line = line
            .trim()
            .trim_start_matches("/**")
            .trim_start_matches('*')
            .trim();
        if let Some(value) = line
            .strip_prefix("@see ")
            .and_then(|v| v.split_whitespace().next())
        {
            if let Some((class, method)) = value.split_once("::") {
                let class = class.trim_start_matches('\\');
                if !class.is_empty()
                    && class.split('\\').all(|p| {
                        !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    })
                    && !method.is_empty()
                    && method
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    controllers.insert((class.to_owned(), method.to_owned()));
                }
            }
        }
        if let Some(value) = line
            .strip_prefix("@route ")
            .and_then(|v| v.split_whitespace().next())
        {
            if value.starts_with('/') && !value.contains(['\\', '$', '\0']) {
                uris.insert(value.into());
            }
        }
    }
    if controllers.len() != 1 || uris.len() != 1 {
        return None;
    }
    let (controller, method) = controllers.pop_first()?;
    Some(Target {
        controller,
        method,
        uri: uris.pop_first()?,
    })
}
fn checked_entry(
    project: &Project,
    entry: &Entry,
    kind: &str,
    name: &str,
) -> Option<(Location, String)> {
    if !crate::owns_current_path(project, &entry.path) {
        return None;
    }
    let source = project.read_file(&entry.path).ok()?;
    if source.len() > 512 * 1024 {
        return None;
    }
    let at = text::offset(&source, entry.at)?;
    let code = package_facts::code(&source)?;
    let token = code.get(at..at + name.len())?;
    if !token.eq_ignore_ascii_case(name)
        || code
            .as_bytes()
            .get(at + name.len())
            .is_some_and(|b| word(*b))
    {
        return None;
    }
    let before = code[..at].trim_end().trim_end_matches('&').trim_end();
    if !before.ends_with(kind) {
        return None;
    }
    let end = text::position(&source, at + name.len());
    Some((
        Location {
            path: entry.path.clone(),
            range: Range::new(entry.at, end),
        },
        source,
    ))
}
fn controller(project: &Project, index: &Index, target: &Target) -> Option<Location> {
    let mut found = Vec::new();
    for (key, entry) in &index.controllers {
        let short = key.rsplit('\\').next()?;
        let Some((_, source)) = checked_entry(project, entry, "class", short) else {
            continue;
        };
        if !crate::eloquent_members::source_class_name(&source, short)
            .is_some_and(|full| full.eq_ignore_ascii_case(&target.controller))
        {
            continue;
        }
        let methods = index.methods.get(key)?;
        let matching = methods
            .iter()
            .filter(|(method, _)| method.eq_ignore_ascii_case(&target.method))
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return None;
        }
        let (method, entry) = matching[0];
        let (location, current) = checked_entry(project, entry, "function", method)?;
        if source != current
            || project.read_file(&entry.path).ok()? != current
            || !crate::owns_current_path(project, &entry.path)
        {
            return None;
        }
        found.push(location);
    }
    (found.len() == 1).then(|| found.remove(0))
}
pub(crate) fn definition(
    project: &Project,
    index: &Index,
    doc: &Document,
    at: Position,
) -> Vec<Location> {
    let run = || -> Option<Vec<Location>> {
        if index.partial
            || doc.text.len() > 512 * 1024
            || !crate::owns_current_path(project, &doc.path)
        {
            return None;
        }
        let (reference, owner, member) = reference(doc, at)?;
        let generated = package_facts::wayfinder_definition(project, index, doc, owner);
        if generated.len() != 1 {
            return None;
        }
        let generated = &generated[0];
        let source = project.read_file(&generated.path).ok()?;
        if source.len() > 512 * 1024 {
            return None;
        }
        let export = member.as_deref().unwrap_or(&reference.exported);
        let destination = target(export_doc(&source, export)?)?;
        let mut result = Vec::new();
        if reference.module.starts_with("@/routes/") {
            let entries = index
                .route_defs
                .iter()
                .filter(|entry| {
                    entry.endpoint_uri.as_deref().unwrap_or(&entry.def.uri) == destination.uri
                })
                .collect::<Vec<_>>();
            let expected = reference
                .module
                .strip_prefix("@/routes/")?
                .replace('/', ".");
            let name = format!("{expected}.{}", reference.exported);
            let matches = entries
                .into_iter()
                .filter(|entry| entry.def.name.as_deref() == Some(name.as_str()))
                .collect::<Vec<_>>();
            if matches.len() == 1 {
                let entry = matches[0];
                if crate::owns_current_path(project, &entry.path) {
                    let current = project.read_file(&entry.path).ok()?;
                    let defs =
                        index.parse_routes(&entry.path,&current);
                    if defs
                        .iter()
                        .filter(|def| {
                            def.name == entry.def.name
                                && def.uri == entry.def.uri
                                && def.action == entry.def.action
                        })
                        .count()
                        == 1
                    {
                        let def = defs.iter().find(|def| {
                            def.name == entry.def.name
                                && def.uri == entry.def.uri
                                && def.action == entry.def.action
                        })?;
                        result.push(Location {
                            path: entry.path.clone(),
                            range: Range::new(
                                text::position(&current, def.offset),
                                text::position(&current, def.offset + def.width),
                            ),
                        });
                    }
                }
            }
        }
        if let Some(location) = controller(project, index, &destination) {
            result.push(location);
        }
        if project.read_file(&generated.path).ok()? != source
            || !crate::owns_current_path(project, &generated.path)
        {
            return None;
        }
        (!result.is_empty()).then_some(result)
    };
    run().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const CONTROLLER: &str = "<?php\nnamespace App\\Http\\Controllers;\nclass PostController\n{\n    public function show() {}\n    public function __invoke() {}\n}\n";
    const GENERATED: &str = "/**\n * @see App\\Http\\Controllers\\PostController::show\n * @see app/Http/Controllers/PostController.php:5\n * @route /posts/{post}\n */\nexport const show = (post: number) => ({url: '/posts/'+post});\nconst PostController = {show};\nexport default PostController;\n";
    fn setup() -> (MockHost, Project, Index) {
        let host = MockHost::install();
        host.add_file("composer.json",r#"{"require":{"laravel/framework":"^13.0","laravel/wayfinder":"^0.1"},"autoload":{"psr-4":{"App\\":"app/"}}}"#);
        host.add_file("artisan", "<?php");
        host.add_file("app/Http/Controllers/PostController.php", CONTROLLER);
        host.add_file("routes/web.php","<?php\nuse App\\Http\\Controllers\\PostController;\nRoute::get('/posts/{post}', [PostController::class,'show'])->name('posts.show');\n");
        host.add_file(
            "resources/js/actions/App/Http/Controllers/PostController.ts",
            GENERATED,
        );
        host.add_file("resources/js/routes/posts/index.ts", GENERATED);
        let project = host.invocation().project().unwrap();
        let index = Index::build(&project);
        (host, project, index)
    }
    fn doc(source: &str) -> Document {
        Document {
            path: "/project/resources/js/Use.ts".into(),
            language: "typescript".into(),
            version: 1,
            text: source.into(),
        }
    }
    #[test]
    fn named_alias_and_default_member_navigate_to_actual_php_method() {
        let (_host, project, index) = setup();
        for (source, marker) in [
            (
                "import {show as display} from '@/actions/App/Http/Controllers/PostController';\ndisplay(1);",
                "display(1)",
            ),
            (
                "import Posts from '@/actions/App/Http/Controllers/PostController';\nPosts.show(1);",
                "show(1)",
            ),
        ] {
            let d = doc(source);
            let found = definition(
                &project,
                &index,
                &d,
                text::position(source, source.find(marker).unwrap() + 2),
            );
            assert_eq!(found.len(), 1, "{source}");
            assert_eq!(
                found[0].path,
                "/project/app/Http/Controllers/PostController.php"
            );
            assert_eq!(
                found[0].range.start,
                text::position(CONTROLLER, CONTROLLER.find("show()").unwrap())
            );
        }
    }
    #[test]
    fn named_route_export_returns_route_declaration_and_php_handler() {
        let (_host, project, index) = setup();
        let source = "import {show} from '@/routes/posts';\nshow(1);";
        let found = definition(
            &project,
            &index,
            &doc(source),
            text::position(source, source.rfind("show").unwrap() + 1),
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].path, "/project/routes/web.php");
        assert_eq!(
            found[1].path,
            "/project/app/Http/Controllers/PostController.php"
        );
    }
    #[test]
    fn stale_class_or_method_targets_shadowed_bindings_and_ambiguous_docs_do_not_navigate() {
        let (host, project, index) = setup();
        let source =
            "import {show} from '@/actions/App/Http/Controllers/PostController';\nshow(1);";
        let at = text::position(source, source.rfind("show").unwrap() + 1);
        host.add_file(
            "app/Http/Controllers/PostController.php",
            &CONTROLLER.replace("function show", "function changed"),
        );
        assert!(definition(&project, &index, &doc(source), at).is_empty());
        host.add_file("app/Http/Controllers/PostController.php", CONTROLLER);
        host.add_file(
            "resources/js/actions/App/Http/Controllers/PostController.ts",
            &GENERATED.replace(
                " * @route",
                " * @see App\\Http\\Controllers\\OtherController::show\n * @route",
            ),
        );
        assert!(definition(&project, &index, &doc(source), at).is_empty());
        host.add_file(
            "resources/js/actions/App/Http/Controllers/PostController.ts",
            GENERATED,
        );
        let shadowed = source.replace("show(1)", "const show = other; show(1)");
        assert!(
            definition(
                &project,
                &index,
                &doc(&shadowed),
                text::position(&shadowed, shadowed.rfind("show").unwrap() + 1)
            )
            .is_empty()
        );
    }
    #[test]
    fn documentation_is_adjacent_to_the_exact_export_and_invokable_default_is_supported() {
        assert!(
            export_doc(
                "/** @see Wrong::show */\nconst other=1;\nexport const show=()=>1;",
                "show"
            )
            .is_none()
        );
        let source = "/**\n * @see App\\Http\\Controllers\\PostController::__invoke\n * @route /posts\n */\nconst Invoke=()=>1;\nexport default Invoke;\n";
        let target = target(export_doc(source, "default").unwrap()).unwrap();
        assert_eq!(target.method, "__invoke");
    }
}
