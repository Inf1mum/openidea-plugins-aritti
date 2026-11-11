//! Literal package model declarations. Sources:
//! https://github.com/WendellAdriel/laravel-lift (ManageRelations/Relations attributes)
//! https://octobercms.com/docs/api/october/rain/database/concerns/hasrelationships
//! https://github.com/staudenmeir/laravel-adjacency-list (tree/graph traits)
//! The caller supplies installed source proof;
//! this module never executes attributes, resolves runtime closures, or guesses targets.
use super::{DeclaredRelation, Entry, Kind, Names, Source};
use crate::php;
use std::collections::BTreeMap;
#[path = "package_inflector.rs"]
pub(crate) mod inflector;
#[derive(Default)]
pub(crate) struct Capabilities {
    pub lift: BTreeMap<String, Vec<String>>,
    pub inflector: Option<inflector::Inflector>,
    pub october: Vec<String>,
    pub recursive: BTreeMap<String, Vec<(String, String, bool, Entry, Vec<String>)>>,
}
fn class_value(source: &Source, names: &Names, range: std::ops::Range<usize>) -> Option<String> {
    let tokens: Vec<_> = source
        .tokens
        .iter()
        .filter(|t| t.start >= range.start && t.end <= range.end)
        .collect();
    match tokens.as_slice() {
        [class, op, member]
            if class.kind == Kind::Name
                && op.value == "::"
                && member.value.eq_ignore_ascii_case("class") =>
        {
            Some(names.resolve(class.value))
        }
        [literal] if literal.kind == Kind::String => {
            let (raw, _) = php::literal(source.text, literal.start..literal.end)?;
            let value =
                super::metadata_inference::decode_literal(raw, literal.value.as_bytes()[0])?;
            let full = value.trim_start_matches('\\');
            (full.contains('\\') && full.split('\\').all(crate::generator_plan::identifier))
                .then(|| full.into())
        }
        _ => None,
    }
}
fn traits(source: &Source, names: &Names, body: usize) -> Vec<String> {
    let mut result = Vec::new();
    for (at, token) in source.tokens.iter().enumerate() {
        if token.value != "use" || source.paths[at] != vec![body] {
            continue;
        }
        let mut cursor = at + 1;
        loop {
            let Some(token) = source.tokens.get(cursor) else {
                break;
            };
            if token.kind != Kind::Name {
                break;
            }
            result.push(names.resolve(token.value));
            cursor += 1;
            if source.is(cursor, "{") {
                return vec![];
            }
            if !source.is(cursor, ",") {
                break;
            }
            cursor += 1;
        }
    }
    result
}
pub(crate) fn collect(
    text: &str,
    path: &str,
    caps: &Capabilities,
    october_owner: bool,
) -> Vec<DeclaredRelation> {
    let Some(source) = Source::new(text) else {
        return vec![];
    };
    let Some(names) = Names::parse(&source) else {
        return vec![];
    };
    let Some((owner, class, body, _)) = names.class.as_ref() else {
        return vec![];
    };
    if !source.is(*class, "class") {
        return vec![];
    }
    let traits = traits(&source, &names, *body);
    let mut result = Vec::new();
    let mut add = |name: String,
                   target: String,
                   kind: String,
                   many: bool,
                   entry: Entry,
                   evidence_paths: Vec<String>| {
        if crate::generator_plan::identifier(&name) && !target.is_empty() {
            result.push(DeclaredRelation {
                owner: owner.clone(),
                owner_path: path.into(),
                name,
                target,
                kind,
                many,
                entry,
                evidence_paths,
            });
        }
    };
    // Only attributes on the class declaration, never method/property attributes.
    if traits.iter().any(|t| t == "WendellAdriel\\Lift\\Lift") {
        let mut class_attributes = Vec::new();
        let mut before = *class;
        loop {
            if before > 0
                && matches!(
                    source.tokens[before - 1].value,
                    "final" | "abstract" | "readonly"
                )
            {
                before -= 1;
                continue;
            }
            if before > 0 && source.is(before - 1, "]") {
                let open = source.pairs.iter().find_map(|(open, close)| {
                    (*close == before - 1 && *open > 0 && source.is(*open - 1, "#"))
                        .then_some(*open)
                });
                if let Some(open) = open {
                    class_attributes.push(open - 1);
                    before = open - 1;
                    continue;
                }
            }
            break;
        }
        class_attributes.reverse();
        for at in class_attributes {
            if !source.is(at, "#") || !source.is(at + 1, "[") {
                continue;
            }
            let Some(end) = source.pairs.get(&(at + 1)).copied() else {
                continue;
            };
            let mut cursor = at + 2;
            while cursor < end {
                let attribute = names.resolve(source.tokens[cursor].value);
                let Some(evidence) = caps.lift.get(&attribute) else {
                    break;
                };
                let Some(kind) = attribute.rsplit('\\').next() else {
                    break;
                };
                if !source.is(cursor + 1, "(") {
                    break;
                }
                let open = cursor + 1;
                let Some(close) = source.pairs.get(&open).copied() else {
                    break;
                };
                let args = php::arguments(text, source.tokens[open].start);
                let mut target = None;
                let mut name = None;
                let mut name_entry = None;
                let mut explicit_name = false;
                let extra_parameter = match kind {
                    "HasOneThrough" | "HasManyThrough" => Some("through"),
                    "MorphOne" | "MorphMany" | "MorphToMany" | "MorphedByMany" => Some("morphName"),
                    _ => None,
                };
                let name_position = if extra_parameter.is_some() { 2 } else { 1 };
                let mut extra_present = extra_parameter.is_none();
                let mut seen_keys = std::collections::BTreeSet::new();
                let mut named_started = false;
                let mut invalid = false;
                for (position, range) in args.into_iter().enumerate() {
                    let token_at = source
                        .tokens
                        .iter()
                        .position(|t| t.start >= range.start && t.end <= range.end);
                    let Some(token_at) = token_at else { continue };
                    let named = source.is(token_at + 1, ":");
                    let key = if named {
                        source.tokens[token_at].value
                    } else if position == 0 {
                        "related"
                    } else if position == name_position {
                        "name"
                    } else {
                        ""
                    };
                    if (!named && named_started)
                        || (!key.is_empty() && !seen_keys.insert(key.to_owned()))
                    {
                        invalid = true;
                    }
                    named_started |= named;
                    let value = if named {
                        source.tokens[token_at + 1].end..range.end
                    } else {
                        range
                    };
                    if extra_parameter == Some(key)
                        || (!named && position == 1 && extra_parameter.is_some())
                    {
                        extra_present = true;
                    }
                    if key == "related" {
                        target = class_value(&source, &names, value);
                    } else if key == "name" {
                        let value_tokens = source
                            .tokens
                            .iter()
                            .filter(|token| token.start >= value.start && token.end <= value.end)
                            .collect::<Vec<_>>();
                        explicit_name =
                            !matches!(value_tokens.as_slice(),[token] if token.value=="null");
                        if let Some((literal, start)) = php::literal(text, value) {
                            name = Some(literal.to_owned());
                            name_entry = Some(Entry {
                                path: path.into(),
                                at: openidea_plugin::text::position(text, start),
                                width: literal.encode_utf16().count() as u32,
                                detail: None,
                            });
                        }
                    }
                }
                if invalid || !extra_present {
                    cursor = close + 1;
                    if source.is(cursor, ",") {
                        cursor += 1;
                        continue;
                    }
                    break;
                }
                let many = matches!(
                    kind,
                    "HasMany"
                        | "HasManyThrough"
                        | "BelongsToMany"
                        | "MorphMany"
                        | "MorphToMany"
                        | "MorphedByMany"
                );
                let mut naming_evidence = Vec::new();
                if !explicit_name
                    && let (Some(target), Some(inflector)) =
                        (target.as_ref(), caps.inflector.as_ref())
                {
                    name = inflector.name(target, many);
                    name_entry = Some(super::entry(&source, path, cursor));
                    naming_evidence = inflector.evidence.clone();
                }

                if let (Some(target), Some(name), Some(entry)) = (target, name, name_entry) {
                    let mut native = kind.to_owned();
                    native[..1].make_ascii_lowercase();
                    let many = matches!(
                        kind,
                        "HasMany"
                            | "HasManyThrough"
                            | "BelongsToMany"
                            | "MorphMany"
                            | "MorphToMany"
                            | "MorphedByMany"
                    );
                    add(
                        name,
                        target,
                        native,
                        many,
                        entry,
                        evidence.iter().cloned().chain(naming_evidence).collect(),
                    );
                }
                cursor = close + 1;
                if source.is(cursor, ",") {
                    cursor += 1;
                } else {
                    break;
                }
            }
        }
    }
    if october_owner && !caps.october.is_empty() {
        for (at, token) in source.tokens.iter().enumerate() {
            if token.kind != Kind::Var
                || source.paths[at] != vec![*body]
                || !source.is(at + 1, "=")
                || !source.is(at + 2, "[")
            {
                continue;
            }
            let kind = token.value.trim_start_matches('$');
            if !matches!(
                kind,
                "hasOne"
                    | "hasMany"
                    | "belongsTo"
                    | "belongsToMany"
                    | "hasOneThrough"
                    | "hasManyThrough"
                    | "morphOne"
                    | "morphMany"
                    | "morphToMany"
                    | "morphedByMany"
            ) {
                continue;
            }
            for item in php::items(text, source.tokens[at + 2].start) {
                let Some((name, start)) = item.key.and_then(|r| php::literal(text, r)) else {
                    continue;
                };
                let target = class_value(&source, &names, item.value.clone()).or_else(|| {
                    let first = source
                        .tokens
                        .iter()
                        .find(|t| t.start >= item.value.start && t.end <= item.value.end)?;
                    if first.value != "[" {
                        return None;
                    }
                    let values = php::items(text, first.start);
                    let first = values.first()?;
                    if first.key.is_some() {
                        return None;
                    }
                    class_value(&source, &names, first.value.clone())
                });
                if let Some(target) = target {
                    add(
                        name.into(),
                        target,
                        kind.into(),
                        matches!(
                            kind,
                            "hasMany"
                                | "belongsToMany"
                                | "hasManyThrough"
                                | "morphMany"
                                | "morphToMany"
                                | "morphedByMany"
                        ),
                        Entry {
                            path: path.into(),
                            at: openidea_plugin::text::position(text, start),
                            width: name.encode_utf16().count() as u32,
                            detail: None,
                        },
                        caps.october.clone(),
                    );
                }
            }
        }
    }
    for used in traits {
        if let Some(relations) = caps.recursive.get(&used) {
            for (name, kind, many, entry, evidence) in relations {
                add(
                    name.clone(),
                    owner.clone(),
                    kind.clone(),
                    *many,
                    entry.clone(),
                    evidence.clone(),
                );
            }
        }
    }
    result
}
pub(crate) fn literal_lift_naming(text:&str,many:bool)->bool{
 let Some(source)=Source::new(text)else{return false};let Some(names)=Names::parse(&source)else{return false};if names.resolve("Str")!="Illuminate\\Support\\Str"||source.tokens.windows(2).any(|pair|pair[0].value=="use"&&pair[1].value=="function"){return false;}
 let functions=source.functions();if functions.iter().any(|f|f.name.is_some_and(|at|source.tokens[at].value=="class_basename")){return false;}
 let matching=functions.iter().filter(|f|f.name.is_some_and(|at|source.tokens[at].value=="relationName")).collect::<Vec<_>>();let [function]=matching.as_slice()else{return false};
 let expected=["return","$this","->","name","??","Str","::",if many{"plural"}else{"singular"},"(","Str","::","camel","(","class_basename","(","$this","->","related",")",")",")",";"];
 source.tokens[function.body+1..function.end].iter().map(|t|t.value).eq(expected)
}
pub(crate) fn constructor_parameters(text: &str) -> Option<Vec<String>> {
    let source = Source::new(text)?;
    let mut functions = source.functions().into_iter().filter(|f| {
        f.name
            .is_some_and(|at| source.tokens[at].value == "__construct")
    });
    let f = functions.next()?;
    if functions.next().is_some() {
        return None;
    }
    let depth = source.paths.get(f.params.start)?.clone();
    Some(
        source.tokens[f.params.clone()]
            .iter()
            .enumerate()
            .filter(|(at, t)| t.kind == Kind::Var && source.paths[f.params.start + *at] == depth)
            .map(|(_, t)| t.value.trim_start_matches('$').into())
            .collect(),
    )
}
/// Exact public method declarations from the installed package's known trait.
pub(crate) fn methods(text: &str, path: &str) -> BTreeMap<String, Entry> {
    let Some(source) = Source::new(text) else {
        return BTreeMap::new();
    };
    let Some(names) = Names::parse(&source) else {
        return BTreeMap::new();
    };
    let Some((_, _, body, _)) = names.class else {
        return BTreeMap::new();
    };
    source
        .functions()
        .into_iter()
        .filter(|f| source.paths[f.start] == vec![body])
        .filter_map(|f| {
            let at = f.name?;
            Some((
                source.tokens[at].value.into(),
                super::entry(&source, path, at),
            ))
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_lift_and_october_are_binding_and_scope_aware() {
        let mut caps = Capabilities::default();
        caps.lift.insert(
            "WendellAdriel\\Lift\\Attributes\\Relations\\HasMany".into(),
            vec!["/p/vendor/lift.php".into()],
        );
        caps.october = vec!["/p/vendor/october.php".into()];
        let source = "<?php namespace App; use WendellAdriel\\Lift\\Attributes\\Relations\\HasMany as Children; use WendellAdriel\\Lift\\Lift; #[Children(Post::class, name: 'posts')] class User extends Model {use Lift; public $hasOne=['avatar'=>[Avatar::class,'key'=>'user_id']];public function f(){ $hasMany=['fake'=>Post::class];}}";
        let relations = collect(source, "/p/app/User.php", &caps, true);
        assert_eq!(relations.len(), 2);
        assert_eq!(
            (&relations[0].name, &relations[0].target),
            (&"posts".into(), &"App\\Post".into())
        );
        assert_eq!(relations[1].name, "avatar");
        assert!(
            collect(
                &source.replace(
                    "WendellAdriel\\Lift\\Attributes\\Relations\\HasMany",
                    "Vendor\\HasMany"
                ),
                "/p/app/User.php",
                &caps,
                false
            )
            .is_empty()
        );
        assert!(
            collect(
                &source.replace("name: 'posts'", "name: dynamic()"),
                "/p/app/User.php",
                &caps,
                false
            )
            .is_empty()
        );
        assert!(
            collect(
                &source.replace(
                    "#[Children(Post::class, name: 'posts')]",
                    "#[Children(Post::class,name:'fake')] function unrelated(){}"
                ),
                "/p/app/User.php",
                &caps,
                false
            )
            .is_empty()
        );
    }
    #[test]
    fn october_string_targets_decode_without_namespacing_and_dynamic_targets_withheld() {
        let caps = Capabilities {
            october: vec!["/p/vendor/october.php".into()],
            ..Default::default()
        };
        let source = "<?php namespace App; class Blog extends Model {public $hasMany=['posts'=>['Domain\\\\Post','key'=>'blog_id'],'fake'=>resolve()];}";
        let facts = collect(source, "/p/app/Blog.php", &caps, true);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].target, "Domain\\Post");
        assert!(collect(source, "/p/app/Blog.php", &caps, false).is_empty());
    }
}
#[cfg(test)]
mod installed_tests {
    use crate::{
        Index,
        package_integrations::{ComposerRoot, declared_relations},
    };
    use openidea_plugin::{
        mock::MockHost,
        providers::{Document, Providers},
        text,
    };
    #[test]
    #[ignore = "requires disposable installed relation package fixtures"]
    fn installed_packages_publish_literal_relations_into_eloquent_inventory() {
        let folder = std::env::var("OPENIDEA_RELATION_FIXTURE").unwrap();
        let october = std::env::var("OPENIDEA_OCTOBER_FIXTURE").unwrap();
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("artisan", "<?php");
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        let mut roots = Vec::new();
        for (package, namespace, directory, base) in [
            (
                "wendelladriel/laravel-lift",
                "WendellAdriel\\Lift\\",
                "vendor/wendelladriel/laravel-lift/src",
                &folder,
            ),
            (
                "staudenmeir/laravel-adjacency-list",
                "Staudenmeir\\LaravelAdjacencyList\\",
                "vendor/staudenmeir/laravel-adjacency-list/src",
                &folder,
            ),
            (
                "doctrine/inflector",
                "Doctrine\\Inflector\\",
                "vendor/doctrine/inflector/src",
                &folder,
            ),
            (
                "illuminate/database",
                "Illuminate\\Database\\",
                "vendor/illuminate/database",
                &folder,
            ),
            (
                "illuminate/support",
                "Illuminate\\Support\\",
                "vendor/illuminate/support",
                &folder,
            ),
            (
                "october/rain",
                "October\\Rain\\",
                "vendor/october/rain/src",
                &october,
            ),
        ] {
            roots.push(ComposerRoot {
                namespace: namespace.into(),
                directory: directory.into(),
                package: package.into(),
            });
            fn copy(host: &MockHost, base: &std::path::Path, directory: &str) {
                for entry in std::fs::read_dir(base.join(directory)).unwrap() {
                    let entry = entry.unwrap();
                    let path = entry.path();
                    let relative = path.strip_prefix(base).unwrap().to_string_lossy();
                    if path.is_dir() {
                        copy(host, base, &relative);
                    } else if path.extension().is_some_and(|x| x == "php") {
                        host.add_file(&relative, &std::fs::read_to_string(&path).unwrap());
                    }
                }
            }
            copy(&host, std::path::Path::new(base), directory);
        }
        roots.push(ComposerRoot {
            namespace: "App\\".into(),
            directory: "app".into(),
            package: "project".into(),
        });
        let sources = [
            (
                "app/User.php",
                "<?php namespace App;use WendellAdriel\\Lift\\Lift;use WendellAdriel\\Lift\\Attributes\\Relations\\HasMany;#[HasMany(Post::class)]#[HasMany(Post::class,name:'save')]#[HasMany(Post::class,name:'getAttribute')]class User extends \\Illuminate\\Database\\Eloquent\\Model{use Lift;}",
            ),
            (
                "app/Post.php",
                "<?php namespace App;class Post extends \\Illuminate\\Database\\Eloquent\\Model{}",
            ),
            (
                "app/Category.php",
                "<?php namespace App;class Category extends \\Illuminate\\Database\\Eloquent\\Model{use \\Staudenmeir\\LaravelAdjacencyList\\Eloquent\\HasRecursiveRelationships;}",
            ),
            (
                "app/Article.php",
                "<?php namespace App;class Article extends \\October\\Rain\\Database\\Model{public $belongsTo=['author'=>User::class];}",
            ),
        ];
        let project = host.invocation().project().unwrap();
        let root = project.root().unwrap();
        let mut index = Index::default();
        for (path, source) in sources {
            host.add_file(path, source);
            super::super::cache(
                &mut index.eloquent_models,
                source,
                &format!("{root}/{path}"),
            );
        }
        declared_relations(&project, &mut index, &roots);
        assert_eq!(
            index.eloquent_models["app\\user"].relations["posts"].target,
            "App\\Post"
        );
        assert!(
            !index.eloquent_models["app\\user"]
                .relations
                .contains_key("save")
        );
        assert!(
            !index.eloquent_models["app\\user"]
                .relations
                .contains_key("getAttribute")
        );
        assert_eq!(
            index.eloquent_models["app\\article"].relations["author"].target,
            "App\\User"
        );
        let category = &index.eloquent_models["app\\category"].relations;
        assert_eq!(category["parent"].target, "App\\Category");
        assert!(!category["parent"].many);
        assert!(category["parentAndSelf"].many);
        assert!(category["descendants"].many);
        super::super::finish(&mut index.eloquent_models);
        let mut main = crate::Main::default();
        main.indexes.insert(root.clone(), index);
        let source = "<?php use App\\User; User::first()->po";
        let doc = Document {
            path: format!("{root}/app/Use.php"),
            language: "php".into(),
            text: source.into(),
            version: 1,
        };
        assert!(
            main.completion(
                &doc,
                text::position(source, source.find("po").unwrap() + 2),
                None
            )
            .iter()
            .any(|item| item.label == "posts")
        );
        let source = "<?php use App\\Article; Article::first()->aut";
        let doc = Document {
            path: format!("{root}/app/Use.php"),
            language: "php".into(),
            text: source.into(),
            version: 2,
        };
        assert!(
            main.completion(&doc, text::position(source, source.len()), None)
                .iter()
                .any(|item| item.label == "author")
        );
        host.add_file("app/Providers/PluralizerProvider.php","<?php namespace App\\Providers;use Illuminate\\Support\\Pluralizer;class PluralizerProvider{public function boot(){Pluralizer::useLanguage('french');}}");
        let mut language_override = Index::default();
        for (path, source) in sources {
            super::super::cache(
                &mut language_override.eloquent_models,
                source,
                &format!("{root}/{path}"),
            );
        }
        declared_relations(&project, &mut language_override, &roots);
        assert!(
            language_override.eloquent_models["app\\user"]
                .relations
                .is_empty()
        );
        // Losing exact package ownership/proof must not republish its relations.
        let mut absent = Index::default();
        for (path, source) in sources {
            super::super::cache(
                &mut absent.eloquent_models,
                source,
                &format!("{root}/{path}"),
            );
        }
        declared_relations(
            &project,
            &mut absent,
            &roots
                .into_iter()
                .filter(|r| r.package == "project")
                .collect::<Vec<_>>(),
        );
        assert!(absent.eloquent_models["app\\user"].relations.is_empty());
    }
}
