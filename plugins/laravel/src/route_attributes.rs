//! Laravel 13 controller Middleware/WithoutMiddleware/Authorize attribute contexts.
//! These attributes decorate registered controllers; they do not declare URIs.
use crate::{
    Index,
    index::{Entry, Names},
    php, refs,
};
use openidea_plugin::{
    Location, Position, Range,
    providers::{CompletionItem, CompletionKind, Document},
    text,
};
fn context(index: &Index, doc: &Document, at: Position) -> Option<(Names, std::ops::Range<usize>)> {
    if index.partial
        || doc.language != "php"
        || !index
            .metadata
            .framework_version
            .is_some_and(|v| v >= (13, 0, 0))
    {
        return None;
    }
    let offset = text::offset(&doc.text, at)?;
    let (literal, site) = refs::literal_site_at("php", &doc.text, offset)?;
    let code = crate::package_facts::code(&doc.text)?;
    let attribute = code[..site.callee_start].rfind("#[")?;
    let end = php::closing(&doc.text, attribute + 1).unwrap_or(doc.text.len());
    if offset > end {
        return None;
    }
    if !php::split(&doc.text, attribute + 2..end).iter().any(|r| {
        let r = php::trimmed(&doc.text, r.clone());
        r.start == site.callee_start
    }) {
        return None;
    }
    if !site.callee.contains('\\') && php::declares_class(&doc.text, &site.callee) {
        return None;
    }
    let full = crate::eloquent_members::source_class_name(&doc.text, &site.callee)?;
    let prefix = "Illuminate\\Routing\\Attributes\\Controllers\\";
    if !full
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    {
        return None;
    }
    let short = match full[prefix.len()..].to_ascii_lowercase().as_str() {
        "middleware" => "Middleware",
        "withoutmiddleware" => "WithoutMiddleware",
        "authorize" => "Authorize",
        _ => return None,
    };
    let after = site.callee_start + site.callee.len();
    let rest = doc.text[after..].trim_start();
    if !rest.starts_with('(') {
        return None;
    }
    let open = doc.text.len() - rest.len();
    let args = php::split(
        &doc.text,
        open + 1..php::closing(&doc.text, open).unwrap_or(doc.text.len()),
    );
    let argument = args.get(site.arg)?;
    let positional = if short == "Authorize" {
        ["ability", "models", "only", "except"]
            .get(site.arg)
            .copied()
    } else {
        ["middleware", "only", "except"].get(site.arg).copied()
    };
    let raw = code.get(argument.start..literal.start)?;
    let named = raw.split_once(':').and_then(|(name, _)| {
        let name = name.trim();
        (!name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
            .then_some(name)
    });
    let parameter = named.or(positional)?;
    let names = match parameter {
        "middleware" if short != "Authorize" && site.depth == 0 => index.middleware.clone(),
        "ability" if short == "Authorize" && site.depth == 0 => index.abilities.clone(),
        "only" | "except" if site.depth == 1 => {
            let mut names = Names::new();
            for (class, entry) in &index.controllers {
                if entry.path != doc.path {
                    continue;
                }
                if let Some(methods) = index.methods.get(class) {
                    for (name, entry) in methods {
                        let Some(begin) = text::offset(&doc.text, entry.at) else {
                            continue;
                        };
                        if code.get(begin..begin + name.len()) == Some(name)
                            && code[..begin].trim_end().ends_with("function")
                        {
                            names.insert(name.clone(), entry.clone());
                        }
                    }
                }
            }
            names
        }
        "models" if short == "Authorize" && site.depth <= 1 => {
            let mut names = Names::new();
            let classes = index
                .controllers
                .iter()
                .filter(|(_, e)| e.path == doc.path)
                .map(|(c, _)| c)
                .collect::<Vec<_>>();
            for route in &index.route_defs {
                let Some((class, _)) = &route.def.controller else {
                    continue;
                };
                if index
                    .controllers
                    .keys()
                    .filter(|cached| cached.rsplit('\\').next() == class.rsplit('\\').next())
                    .count()
                    != 1
                    || !classes
                        .iter()
                        .any(|cached| cached.rsplit('\\').next() == class.rsplit('\\').next())
                {
                    continue;
                }
                let mut tail = route.endpoint_uri.as_deref().unwrap_or(&route.def.uri);
                while let Some(open) = tail.find('{') {
                    tail = &tail[open + 1..];
                    let Some(close) = tail.find('}') else {
                        break;
                    };
                    let name = tail[..close]
                        .split(':')
                        .next()
                        .unwrap_or("")
                        .trim_end_matches('?');
                    if !name.is_empty() {
                        names.entry(name.into()).or_insert(Entry {
                            path: route.path.clone(),
                            at: route.at,
                            width: route.width,
                            detail: Some(crate::ui_text::t(
                                "routeAttribute.parameter",
                                "Controller route parameter",
                            )),
                        });
                    }
                    tail = &tail[close + 1..];
                }
            }
            names
        }
        _ => return None,
    };
    let selected = doc.text.get(literal.start..literal.end)?;
    if selected.contains(['$', '\\', '\n', '\r'])
        || parameter == "middleware" && selected.contains(':')
    {
        return None;
    }
    let project = crate::project_for(&doc.path)?;
    let names = names
        .into_iter()
        .filter(|(_, entry)| {
            entry.path.is_empty() || crate::owns_current_path(&project, &entry.path)
        })
        .collect();
    Some((names, literal.start..literal.end))
}
pub(crate) fn completion(index: &Index, doc: &Document, at: Position) -> Vec<CompletionItem> {
    let Some((names, range)) = context(index, doc, at) else {
        return vec![];
    };
    let replace = Range::new(
        text::position(&doc.text, range.start),
        text::position(&doc.text, range.end),
    );
    names
        .into_iter()
        .map(|(name, entry)| {
            let mut item = CompletionItem::new(&name, CompletionKind::Value);
            item.detail = entry.detail;
            item.replace = Some(replace);
            item
        })
        .collect()
}
pub(crate) fn definition(index: &Index, doc: &Document, at: Position) -> Vec<Location> {
    let Some((names, range)) = context(index, doc, at) else {
        return vec![];
    };
    let Some(entry) = names.get(&doc.text[range]) else {
        return vec![];
    };
    if entry.path.is_empty() {
        return vec![];
    }
    vec![Location {
        path: entry.path.clone(),
        range: Range::new(
            entry.at,
            Position::new(
                entry.at.line,
                entry
                    .at
                    .character
                    .checked_add(entry.width)
                    .unwrap_or(entry.at.character),
            ),
        ),
    }]
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn fixture() -> (MockHost, Index) {
        let host = MockHost::install();
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        host.add_file("artisan", "<?php");
        host.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"laravel/framework","version":"v13.0.0"}]}"#,
        );
        let index = Index::build(&host.invocation().project().unwrap());
        (host, index)
    }
    fn complete(index: &Index, marked: &str) -> Vec<CompletionItem> {
        let at = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        let doc = Document {
            path: "/project/app/Http/Controllers/PostController.php".into(),
            language: "php".into(),
            version: 1,
            text: source.clone(),
        };
        completion(index, &doc, text::position(&source, at))
    }
    #[test]
    fn genuine_aliased_and_fqn_middleware_attributes_complete_but_ordinary_calls_and_foreign_aliases_do_not()
     {
        let (_host, index) = fixture();
        for source in [
            "<?php use Illuminate\\Routing\\Attributes\\Controllers\\Middleware as Auth; #[Auth('|')] class C {}",
            "<?php #[\\Illuminate\\Routing\\Attributes\\Controllers\\WithoutMiddleware('|')] class C {}",
        ] {
            assert!(
                complete(&index, source)
                    .iter()
                    .any(|item| item.label == "auth"),
                "{source}"
            );
        }
        for source in [
            "<?php use Vendor\\Middleware; #[Middleware('|')] class C {}",
            "<?php use Illuminate\\Routing\\Attributes\\Controllers\\Middleware; Middleware('|');",
            "<?php // #[\\Illuminate\\Routing\\Attributes\\Controllers\\Middleware('|')] ",
        ] {
            assert!(complete(&index, source).is_empty(), "{source}");
        }
    }
    #[test]
    fn authorize_ability_and_named_middleware_parameters_use_distinct_inventories() {
        let (_host, mut index) = fixture();
        index.abilities.insert(
            "update".into(),
            Entry {
                path: "/project/app/Policies/PostPolicy.php".into(),
                at: Position::new(1, 0),
                width: 6,
                detail: None,
            },
        );
        assert_eq!(complete(&index,"<?php use Illuminate\\Routing\\Attributes\\Controllers\\Authorize; #[Authorize('|')] class C {}").iter().map(|i|i.label.as_str()).collect::<Vec<_>>(),["update"]);
        assert!(complete(&index,"<?php use Illuminate\\Routing\\Attributes\\Controllers\\Middleware; #[Middleware(middleware: '|')] class C {}").iter().any(|i|i.label=="auth"));
        index.metadata.framework_version = Some((12, 9, 0));
        assert!(complete(&index,"<?php #[\\Illuminate\\Routing\\Attributes\\Controllers\\Middleware('|')] class C {}").is_empty());
    }
    #[test]
    fn only_except_and_authorization_models_use_current_controller_methods_and_route_parameters() {
        let (host, _index) = fixture();
        let marked = "<?php\nnamespace App\\Http\\Controllers;\nuse Illuminate\\Routing\\Attributes\\Controllers\\Middleware as Filter;\n#[Filter('auth', only: ['|'])]\nclass PostController {\npublic function show() {}\npublic function store() {}\n}\n";
        let source = marked.replacen('|', "", 1);
        host.add_file("app/Http/Controllers/PostController.php", &source);
        host.add_file("routes/web.php","<?php use App\\Http\\Controllers\\PostController; Route::get('/posts/{post:slug}',[PostController::class,'show']);");
        let index = Index::build(&host.invocation().project().unwrap());
        assert_eq!(
            complete(&index, marked)
                .iter()
                .map(|i| i.label.as_str())
                .collect::<Vec<_>>(),
            ["show", "store"]
        );
        let marked = marked
            .replace("Middleware as Filter", "Authorize as Filter")
            .replace("'auth', only: ['|']", "'update', models: ['|']");
        let source = marked.replacen('|', "", 1);
        host.add_file("app/Http/Controllers/PostController.php", &source);
        let index = Index::build(&host.invocation().project().unwrap());
        assert_eq!(
            complete(&index, &marked)
                .iter()
                .map(|i| i.label.as_str())
                .collect::<Vec<_>>(),
            ["post"]
        );
    }
    #[test]
    fn unicode_replacement_and_foreign_ability_destinations_remain_exact() {
        let (host, mut index) = fixture();
        index.abilities.insert(
            "update".into(),
            Entry {
                path: "/project/app/Policies/PostPolicy.php".into(),
                at: Position::new(1, 0),
                width: 6,
                detail: None,
            },
        );
        let marked = "<?php /* 😀 */ use Illuminate\\Routing\\Attributes\\Controllers\\Authorize; #[Authorize('up|')] class C {}";
        let item = complete(&index, marked).remove(0);
        let source = marked.replacen('|', "", 1);
        let range = item.replace.unwrap();
        assert_eq!(
            &source[text::offset(&source, range.start).unwrap()
                ..text::offset(&source, range.end).unwrap()],
            "up"
        );
        host.add_project("/project/app/Policies");
        assert!(complete(&index, marked).is_empty());
    }
}
