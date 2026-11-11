//! Cache-only route namespace, resource-action and URI-parameter completions.
use crate::{Index, php, refs};
use openidea_plugin::{
    Position, Range, host,
    providers::{CompletionItem, CompletionKind, Document},
    text,
};
use std::collections::BTreeSet;
fn parameters(uri: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut tail = uri;
    while let Some(open) = tail.find('{') {
        tail = &tail[open + 1..];
        let Some(close) = tail.find('}') else {
            break;
        };
        let name = tail[..close]
            .trim_end_matches('?')
            .split(':')
            .next()
            .unwrap_or("");
        if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            names.insert(name.into());
        }
        tail = &tail[close + 1..];
    }
    names
}
fn callable(source: &str, start: usize, name: &str) -> bool {
    if let Some((raw, method)) = name.rsplit_once("::") {
        let _ = method;
        return if raw == "Route" {
            php::insert_use(source, "Illuminate\\Support\\Facades\\Route").is_some()
        } else {
            crate::view_rename::facade(source, raw, "Route")
        };
    }
    if name.contains("->") {
        return refs::route_chain_at(source, start);
    }
    false
}
fn call_open(text: &str, site: &refs::Site) -> Option<usize> {
    let after = site.callee_start + site.callee.len();
    let tail = text.get(after..)?.trim_start();
    tail.starts_with('(').then_some(text.len() - tail.len())
}
fn global_helper(text: &str, site: &refs::Site, expected: &[&str]) -> bool {
    let bare = site.callee.strip_prefix('\\').unwrap_or(&site.callee);
    if bare.contains('\\') || !expected.iter().any(|name| bare.eq_ignore_ascii_case(name)) {
        return false;
    }
    let code = crate::package_facts::code(text).unwrap_or_default();
    let folded = code.to_ascii_lowercase();
    let name = site.callee.trim_start_matches('\\').to_ascii_lowercase();
    if folded.match_indices("function").any(|(at, _)| {
        folded[at + 8..]
            .trim_start()
            .strip_prefix(name.as_str())
            .is_some_and(|tail| tail.trim_start().starts_with('('))
    }) {
        return false;
    }
    if site.callee.starts_with('\\') {
        return true;
    }
    call_open(text, site)
        .and_then(|open| crate::eloquent_members::metadata_inference::call_identity(text, open))
        .is_none_or(|identity| {
            identity.class.is_none()
                && identity
                    .function_fqn
                    .as_deref()
                    .is_none_or(|full| full.eq_ignore_ascii_case(&name))
        })
}
fn route_helper(text: &str, site: &refs::Site) -> bool {
    global_helper(text, site, &["route", "to_route"])
}
fn action_helper(text: &str, site: &refs::Site) -> bool {
    global_helper(text, site, &["action", "link_to_action"])
}
/// Show concise class labels while inserting an exact runtime class string.
/// PHP imports do not apply to strings; source-qualified insertion avoids
/// creating an unresolved global class from a conventional App controller label.
pub(crate) fn short_controllers(index:&Index,doc:&Document,at:Position)->Option<Vec<CompletionItem>>{
    if doc.language!="php"||doc.text.len()>128*1024{return None;}
    let offset=text::offset(&doc.text,at)?;let(literal,site)=refs::literal_site_at("php",&doc.text,offset)?;
    let selected=doc.text.get(literal.start..literal.end)?;
    if selected.contains(['\\','$','\n','\r']){return None;}
    let reference=refs::reference_at("php",&doc.text,offset).or_else(||crate::metadata::route_reference(&index.metadata,&doc.text,offset))?;
    if !matches!(reference.kind,refs::Kind::Action|refs::Kind::Controller)||!(callable(&doc.text,site.callee_start,&site.callee)||action_helper(&doc.text,&site)){return None;}
    if index.partial||index.route_mount_unknown.contains(&doc.path){return Some(vec![]);}
    let project=crate::project_for(&doc.path)?;if !crate::owns_current_path(&project,&doc.path){return Some(vec![]);}
    let inventory=crate::eloquent_members::class_inventory(&index.eloquent_models);let mut options=std::collections::BTreeMap::new();
    for(key,entry)in &index.controllers{
        if !crate::owns_current_path(&project,&entry.path){continue;}
        let identities=inventory.iter().filter(|class|!class.ambiguous&&!class.is_trait&&class.path==entry.path).collect::<Vec<_>>();
        if identities.len()!=1{continue;}let full=&identities[0].full;
        if reference.kind==refs::Kind::Controller{options.insert(key.clone(),format!("\\{full}"));continue;}
        if let Some(methods)=index.methods.get(key){for method in methods.keys(){if method.eq_ignore_ascii_case("__construct"){continue;}if method.eq_ignore_ascii_case("__invoke"){options.insert(key.clone(),format!("\\{full}"));}else{options.insert(format!("{key}@{method}"),format!("\\{full}@{method}"));}}}
    }
    if options.len()>512{return Some(vec![]);}
    let replace=Range::new(text::position(&doc.text,literal.start),text::position(&doc.text,literal.end));
    let prefix=selected.to_ascii_lowercase();let double=doc.text.as_bytes().get(literal.start.checked_sub(1)?)==Some(&b'"');
    Some(options.into_iter().filter(|(label,_)|label.to_ascii_lowercase().starts_with(&prefix)).map(|(label,insert)|{let mut item=CompletionItem::new(&label,CompletionKind::Value);item.insert_text=if double{insert.replace('\\',"\\\\")}else{insert};item.replace=Some(replace);item.filter_text=Some(label);item.detail=Some(crate::ui_text::t("routeCompletion.qualifiedController","Qualified controller action"));item}).collect())
}

pub(crate) fn completion(index: &Index, doc: &Document, at: Position) -> Vec<CompletionItem> {
    let run = || -> Option<(BTreeSet<String>, std::ops::Range<usize>, &'static str)> {
        if index.partial || doc.language != "php" || doc.text.len() > 256 * 1024 {
            return None;
        }
        let offset = text::offset(&doc.text, at)?;
        let (literal, site) = refs::literal_site_at("php", &doc.text, offset)?;
        let selected = doc.text.get(literal.start..literal.end)?;
        if selected.contains(['$', '\n', '\r']) {
            return None;
        }
        let range = literal.start..literal.end;
        let normalized = site
            .callee
            .rsplit(['>', ':'])
            .next()?
            .trim()
            .to_ascii_lowercase();
        let method = normalized.as_str();
        if callable(&doc.text, site.callee_start, &site.callee) || action_helper(&doc.text, &site) {
            let single_resource = matches!(
                method,
                "resource" | "apiresource" | "singleton" | "apisingleton"
            );
            let bulk_resource = matches!(
                method,
                "resources" | "apiresources" | "singletons" | "apisingletons"
            );
            if selected.contains('\\')
                && site.depth <= 1
                && refs::reference_at("php", &doc.text, offset)
                    .or_else(|| {
                        crate::metadata::route_reference(&index.metadata, &doc.text, offset)
                    })
                    .is_some_and(|r| matches!(r.kind, refs::Kind::Action | refs::Kind::Controller))
            {
                let resource = single_resource || bulk_resource || method == "controller";
                let inventory = crate::eloquent_members::class_inventory(&index.eloquent_models);
                let mut names = BTreeSet::new();
                for (key, entry) in &index.controllers {
                    let matching = inventory
                        .iter()
                        .filter(|class| {
                            !class.ambiguous && !class.is_trait && class.path == entry.path
                        })
                        .collect::<Vec<_>>();
                    if matching.len() != 1 {
                        continue;
                    }
                    let full = &matching[0].full;
                    let full = if selected.starts_with('\\') {
                        format!("\\{full}")
                    } else {
                        full.clone()
                    };
                    if resource {
                        names.insert(full);
                        continue;
                    }
                    if let Some(methods) = index.methods.get(key) {
                        for name in methods.keys() {
                            if name == "__invoke" {
                                names.insert(full.clone());
                            }
                            names.insert(format!("{full}@{name}"));
                        }
                    }
                }
                return Some((names, range, "Qualified controller action"));
            }
            if method == "namespace" && site.arg == 0 && site.depth == 0 {
                let root = host::setting_string("controllersNamespace")
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "App\\Http\\Controllers".into());
                let mut names = BTreeSet::from([root.clone()]);
                let inventory = crate::eloquent_members::class_inventory(&index.eloquent_models);
                for (controller, entry) in &index.controllers {
                    let actual = inventory
                        .iter()
                        .filter(|class| {
                            !class.ambiguous && !class.is_trait && class.path == entry.path
                        })
                        .collect::<Vec<_>>();
                    if actual.len() == 1 {
                        if let Some((namespace, _)) = actual[0].full.rsplit_once('\\') {
                            names.insert(namespace.to_owned());
                        }
                        continue;
                    }
                    // Conventional indexed controller folders retain the explicitly
                    // configured namespace when no class cache is available.
                    if !entry.path.contains("/app/Http/Controllers/") {
                        continue;
                    }
                    let mut parts = controller.split('\\').collect::<Vec<_>>();
                    parts.pop();
                    let mut prefix = root.clone();
                    for part in parts {
                        prefix.push('\\');
                        prefix.push_str(part);
                        names.insert(prefix.clone());
                    }
                }
                return Some((names, range, "Controller namespace"));
            }
            if method == "parameters" && site.arg == 0 && site.depth == 1 && site.key.is_none()
                || method == "parameter" && site.arg == 0 && site.depth == 0
            {
                return Some((
                    crate::routes::resource_names_at(&doc.text, offset)
                        .into_iter()
                        .collect(),
                    range,
                    "Resource parameter name",
                ));
            }
            if ["only", "except"].contains(&method) && site.depth <= 1 {
                let code = crate::package_facts::code(&doc.text)?;
                let begin = code[..site.callee_start].rfind(';').map_or(0, |n| n + 1);
                let chain = code[begin..site.callee_start]
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
                    .to_ascii_lowercase();
                if chain.contains("::resource(")
                    || chain.contains("::apiresource(")
                    || chain.contains("->resource(")
                    || chain.contains("->apiresource(")
                    || chain.contains("::singleton(")
                    || chain.contains("::apisingleton(")
                    || chain.contains("->singleton(")
                    || chain.contains("->apisingleton(")
                {
                    let api = chain.contains("apiresource(") || chain.contains("apisingleton(");
                    let singleton = chain.contains("singleton(");
                    let creatable = chain.contains("->creatable(");
                    let destroyable = creatable || chain.contains("->destroyable(");
                    let names = [
                        "index", "create", "store", "show", "edit", "update", "destroy",
                    ]
                    .into_iter()
                    .filter(|name| {
                        if singleton {
                            match *name {
                                "index" => false,
                                "create" => creatable && !api,
                                "store" => creatable,
                                "destroy" => destroyable,
                                "edit" => !api,
                                _ => true,
                            }
                        } else {
                            !api || method == "only" || !matches!(*name, "create" | "edit")
                        }
                    })
                    .map(str::to_owned)
                    .collect();
                    return Some((names, range, "Resource action"));
                }
            }
            if [
                "where",
                "wherenumber",
                "wherealpha",
                "wherealphanumeric",
                "whereuuid",
                "whereulid",
                "wherein",
                "defaults",
                "missing",
            ]
            .contains(&method)
                && site.arg == 0
                && site.depth <= 1
            {
                let names = crate::routes::parameters_at(&doc.text, offset)
                    .into_iter()
                    .collect();
                return Some((names, range, "Route parameter"));
            }
        }
        if route_helper(&doc.text, &site) && site.arg == 1 && site.depth == 1 && site.key.is_none()
        {
            let open = call_open(&doc.text, &site)?;
            let args = php::split(
                &doc.text,
                open + 1..php::closing(&doc.text, open).unwrap_or(doc.text.len()),
            );
            let (name, _) = php::literal(&doc.text, args.first()?.clone())?;
            let matching = index
                .route_defs
                .iter()
                .filter(|route| route.def.name.as_deref() == Some(name))
                .collect::<Vec<_>>();
            if matching.len() != 1 {
                return None;
            }
            return Some((
                parameters(&matching[0].def.uri),
                range,
                "Named route parameter",
            ));
        }
        None
    };
    let Some((names, range, detail)) = run() else {
        return vec![];
    };
    let replace = Range::new(
        text::position(&doc.text, range.start),
        text::position(&doc.text, range.end),
    );
    names
        .into_iter()
        .map(|name| {
            let mut item = CompletionItem::new(&name, CompletionKind::Value);
            item.replace = Some(replace);
            if doc.text.as_bytes().get(range.start.saturating_sub(1)) == Some(&b'"') {
                item.insert_text = name.replace('\\', "\\\\");
                item.filter_text = Some(item.insert_text.clone());
            }
            let key = match detail {
                "Controller namespace" => "routeCompletion.namespace",
                "Resource action" => "routeCompletion.action",
                "Route parameter" => "routeCompletion.parameter",
                "Named route parameter" => "routeCompletion.namedParameter",
                "Resource parameter name" => "routeCompletion.resourceParameter",
                _ => "routeCompletion.qualifiedController",
            };
            item.detail = Some(crate::ui_text::t(key, detail));
            item
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn complete(index: &Index, marked: &str) -> Vec<CompletionItem> {
        let at = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        completion(
            index,
            &Document {
                path: "/project/routes/web.php".into(),
                language: "php".into(),
                version: 1,
                text: source.clone(),
            },
            text::position(&source, at),
        )
    }
    fn labels(items: Vec<CompletionItem>) -> Vec<String> {
        items.into_iter().map(|i| i.label).collect()
    }
    #[test]
    fn resource_only_except_arrays_variadics_and_api_actions() {
        let _host = MockHost::install();
        let index = Index::default();
        assert_eq!(
            labels(complete(
                &index,
                "<?php Route::resource('posts', PostController::class)->only(['|']);"
            )),
            [
                "create", "destroy", "edit", "index", "show", "store", "update"
            ]
        );
        assert_eq!(
            labels(complete(
                &index,
                "<?php Route::apiResource('posts', PostController::class)->except('show','|');"
            )),
            ["destroy", "index", "show", "store", "update"]
        );
        for source in [
            "<?php $other->only('|');",
            "<?php use Vendor\\Route; Route::resource('x', X::class)->only('|');",
            "<?php Route::get('/x', fn()=>1)->only('|');",
        ] {
            assert!(complete(&index, source).is_empty(), "{source}");
        }
    }
    #[test]
    fn fluent_constraint_names_include_group_parameters_and_resource_parameter_keys() {
        let _host = MockHost::install();
        let index = Index::default();
        assert_eq!(
            labels(complete(
                &index,
                "<?php Route::prefix('/teams/{team:slug}')->group(function(){ Route::get('/posts/{post?}',fn()=>1)->whereNumber('|'); });"
            )),
            ["post", "team"]
        );
        assert_eq!(
            labels(complete(
                &index,
                "<?php Route::resource('teams.posts', PostController::class)->parameters(['|' => 'item']);"
            )),
            ["posts", "teams"]
        );
        assert_eq!(
            labels(complete(
                &index,
                "<?php Route::resource('teams.posts', PostController::class)->parameter('|','item');"
            )),
            ["posts", "teams"]
        );
    }

    #[test]
    fn singleton_and_explicit_api_only_actions_match_supported_lifecycle() {
        let _host = MockHost::install();
        let index = Index::default();
        assert_eq!(
            labels(complete(
                &index,
                "<?php Route::singleton('profile', ProfileController::class)->only(['|']);"
            )),
            ["edit", "show", "update"]
        );
        assert_eq!(
            labels(complete(
                &index,
                "<?php Route::apiSingleton('profile', ProfileController::class)->creatable()->except(['|']);"
            )),
            ["destroy", "show", "store", "update"]
        );
        assert!(
            labels(complete(
                &index,
                "<?php Route::apiResource('posts', PostController::class)->only(['|']);"
            ))
            .contains(&"create".to_owned())
        );
        assert!(
            complete(
                &index,
                "<?php consume(Route::resource('x',X::class),(new Other)->only('|'));"
            )
            .is_empty()
        );
    }

    #[test]
    fn controller_namespace_uses_configured_root_and_cached_subnamespaces() {
        let host = MockHost::install();
        host.set_setting("controllersNamespace", r#""Domain\\Http""#);
        let mut index = Index::default();
        index.controllers.insert(
            "Admin\\Users".into(),
            crate::index::Entry {
                path: "/project/app/Http/Controllers/Admin/Users.php".into(),
                detail: None,
                at: Position::new(0, 0),
                width: 0,
            },
        );
        assert_eq!(
            labels(complete(
                &index,
                "<?php Route::namespace('|')->group(function(){});"
            )),
            ["Domain\\Http", "Domain\\Http\\Admin"]
        );
    }
    #[test]
    fn named_route_keys_respect_binding_optional_and_model_keys_and_ignore_values() {
        let _host = MockHost::install();
        let mut index = Index::default();
        let source =
            "<?php Route::get('/teams/{team:slug}/posts/{post?}',fn()=>1)->name('posts.show');";
        let def = crate::routes::parse(source).remove(0);
        index.route_defs.push(crate::index::RouteEntry {
            def,
            path: "/project/routes/web.php".into(),
            at: Position::new(0, 0),
            width: 0,
            endpoint_uri: None,
        });
        assert_eq!(
            labels(complete(&index, "<?php route('posts.show', ['|' => 1]);")),
            ["post", "team"]
        );
        assert!(complete(&index, "<?php route('posts.show', ['post' => '|']);").is_empty());
        assert!(
            complete(
                &index,
                "<?php use function Vendor\\route; route('posts.show', ['|' => 1]);"
            )
            .is_empty()
        );
    }
    #[test]
    fn qualified_controller_strings_use_cached_actual_namespace_and_escape_double_quotes() {
        let host = MockHost::install();
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        host.add_file("artisan", "<?php");
        host.add_file("app/Http/Controllers/Admin/PostController.php","<?php\nnamespace App\\Http\\Controllers\\Admin;\nclass PostController {\npublic function show() {}\npublic function __invoke() {}\n}\n");
        let index = Index::build(&host.invocation().project().unwrap());
        let items = complete(&index, "<?php Route::get('/posts', 'App\\Http\\|');");
        assert!(
            items
                .iter()
                .any(|item| item.label == "App\\Http\\Controllers\\Admin\\PostController@show")
        );
        let items = complete(&index, "<?php Route::get('/posts', \"App\\\\Http\\\\|\");");
        let item = items
            .iter()
            .find(|item| item.label.ends_with("PostController@show"))
            .unwrap();
        assert_eq!(
            item.insert_text,
            "App\\\\Http\\\\Controllers\\\\Admin\\\\PostController@show"
        );
    }
    #[test]
    fn qualified_targets_cover_match_bulk_controller_groups_and_action_helpers() {
        let host = MockHost::install();
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        host.add_file("artisan", "<?php");
        host.add_file("app/Http/Controllers/PostController.php","<?php\nnamespace App\\Http\\Controllers;\nclass PostController {\npublic function show() {}\n}\n");
        let index = Index::build(&host.invocation().project().unwrap());
        for source in [
            "<?php Route::match(['GET'],'/x','App\\Http\\|');",
            "<?php action('App\\Http\\|');",
            "<?php Route::resources(['posts'=>'App\\Http\\|']);",
            "<?php Route::controller('App\\Http\\|')->group(function(){});",
        ] {
            assert!(
                complete(&index, source).iter().any(|item| item
                    .label
                    .starts_with("App\\Http\\Controllers\\PostController")),
                "{source}"
            );
        }
        assert!(
            complete(
                &index,
                "<?php use function Vendor\\action; action('App\\Http\\|');"
            )
            .is_empty()
        );
    }
}
