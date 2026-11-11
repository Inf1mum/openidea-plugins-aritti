//! Conservative default FK factories from current owned relation/model sources.
//! A field suffix alone is never evidence of a relationship or a factory target.
#[path = "generator_factory_contracts.rs"]
mod conventions;
use crate::{
    generator_plan::{Draft, Relation},
    index::Index,
};
use openidea_plugin::{FileKind, Project};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Suggestion {
    pub class: String,
    pub model: String,
    #[serde(skip)]
    pub evidence: String,
}
fn read(project: &Project, path: &str) -> Option<String> {
    let root = project.root().ok()?;
    if !super::owned(&root, path) || project.stat(path).ok()?.kind != FileKind::File {
        return None;
    }
    project
        .read_file(path)
        .ok()
        .filter(|text| text.len() <= 128 << 10)
}
fn eligible(index: &Index) -> bool {
    !index.partial
        && index
            .metadata
            .framework_version
            .is_some_and(|version| (10..=13).contains(&version.0))
}
fn sanitized_code(text: &str) -> String {
    let mut masked = crate::php::mask_comments(text);
    let mut at = 0;
    while at < masked.len() {
        let b = masked.as_bytes()[at];
        if matches!(b, b'\'' | b'"' | b'`') {
            let end = crate::php::string_end(masked.as_bytes(), at);
            masked.replace_range(at..end, &" ".repeat(end - at));
            at = end;
        } else {
            at += masked[at..].chars().next().unwrap().len_utf8();
        }
    }
    masked
}
fn source_words(text: &str) -> Vec<String> {
    let masked = sanitized_code(text);
    let Some((_, body)) = crate::eloquent_members::class_layout(&masked) else {
        return vec!["unsafe".into()];
    };
    masked[body..]
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .filter(|word| !word.is_empty())
        .map(|word| word.to_ascii_lowercase())
        .collect()
}
fn safe_factory_source(text: &str) -> bool {
    if sanitized_code(text).contains("#[") {
        return false;
    }
    let words = source_words(text);
    !words.iter().any(|word| {
        [
            "unsafe",
            "use",
            "eval",
            "__construct",
            "newmodel",
            "modelname",
            "appnamespace",
            "namespace",
            "modelnameresolver",
            "modelnameresolvers",
            "newinstance",
            "__call",
            "__callstatic",
        ]
        .contains(&word.as_str())
    }) && !words
        .windows(2)
        .any(|pair| pair[0] == "this" && pair[1] == "model")
        && !words.windows(2).any(|pair| {
            pair[0] == "function"
                && ["new", "create", "make", "configure"].contains(&pair[1].as_str())
        })
}
fn literal_factory_model(
    index: &Index,
    sources: &BTreeMap<String, String>,
    class: &str,
) -> Option<String> {
    let inventory = crate::eloquent_members::class_inventory(&index.eloquent_models);
    let mut name = class;
    for _ in 0..16 {
        let identity = inventory.iter().find(|identity| identity.full == name)?;
        let source = sources.get(&identity.path)?;
        if let Some(model) = crate::eloquent_members::related_facts::declared_model_class(source) {
            return Some(model);
        }
        if source_words(source).iter().any(|word| word == "model") {
            return None;
        }
        name = &identity.parent;
    }
    None
}
fn binding_snapshot(project: &Project, index: &Index) -> BTreeMap<String, Suggestion> {
    let Ok(root) = project.root() else {
        return BTreeMap::new();
    };
    let original = crate::eloquent_members::class_inventory(&index.eloquent_models);
    let paths = original
        .iter()
        .filter(|identity| super::owned(&root, &identity.path))
        .map(|identity| identity.path.clone())
        .collect::<BTreeSet<_>>();
    if paths.len() > 512 {
        return BTreeMap::new();
    }
    let mut sources = BTreeMap::new();
    let mut total = 0;
    for path in paths {
        let Some(source) = read(project, &path) else {
            return BTreeMap::new();
        };
        total += source.len();
        if total > 4 << 20 {
            return BTreeMap::new();
        }
        sources.insert(path, source);
    }
    let inventory = original;
    let mut bindings = BTreeMap::new();
    let mut evidence_total = 0;
    let mut convention = None;
    let mut convention_checked = false;
    for identity in &inventory {
        let Some(chain) =
            super::reference_chain(&inventory, &identity.full, super::FACTORY_BASE, &root)
        else {
            continue;
        };
        let captured = chain
            .iter()
            .filter_map(|item| {
                sources
                    .get(&item.path)
                    .map(|source| (item.path.clone(), source.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        if captured.len() != chain.len()
            || captured.values().any(|source| !safe_factory_source(source))
        {
            continue;
        }
        // An attribute-only UseFactory association does not establish the model
        // chosen by bare Factory::new(); require its literal $model lineage.
        let mut convention_evidence = String::new();
        let model = if let Some(model) = literal_factory_model(index, &captured, &identity.full) {
            model
        } else {
            if captured
                .values()
                .any(|source| source_words(source).iter().any(|word| word == "model"))
            {
                continue;
            }
            if !convention_checked {
                convention = conventions::Convention::capture(project, index, &inventory, &sources);
                convention_checked = true;
            }
            let Some(proof) = convention.as_ref() else {
                continue;
            };
            let Some(model) = proof.target(project, identity, &inventory) else {
                continue;
            };
            convention_evidence = proof.evidence.clone();
            model
        };
        let evidence = serde_json::to_string(&(&captured, convention_evidence)).unwrap();
        evidence_total += evidence.len();
        if evidence_total > 4 << 20 {
            return BTreeMap::new();
        }
        bindings.insert(
            identity.full.clone(),
            Suggestion {
                class: identity.full.clone(),
                model,
                evidence,
            },
        );
    }
    bindings
}
pub(crate) fn bindings(project: &Project, index: &Index) -> BTreeMap<String, Suggestion> {
    if !eligible(index) || !crate::index::index_sources_current(project, index) {
        return BTreeMap::new();
    }
    binding_snapshot(project, index)
}
fn candidates(project: &Project, index: &Index) -> BTreeMap<String, (String, String)> {
    let mut models = BTreeMap::<String, Vec<Suggestion>>::new();
    for binding in binding_snapshot(project, index).into_values() {
        models
            .entry(binding.model.clone())
            .or_default()
            .push(binding);
    }
    models
        .into_iter()
        .filter_map(|(model, mut factories)| {
            if factories.len() == 1 {
                let binding = factories.remove(0);
                Some((model, (binding.class, binding.evidence)))
            } else {
                None
            }
        })
        .collect()
}
fn suggest(
    project: &Project,
    index: &Index,
    relations: Vec<(Relation, String)>,
    fields: &BTreeSet<String>,
) -> BTreeMap<String, Suggestion> {
    if !eligible(index)
        || fields.is_empty()
        || !relations
            .iter()
            .any(|(relation, _)| relation.kind == "belongsTo")
    {
        return BTreeMap::new();
    }
    let factories = candidates(project, index);
    let mut choices = BTreeMap::<String, Vec<Suggestion>>::new();
    for (relation, relation_evidence) in relations {
        if relation.kind != "belongsTo"
            || !crate::generator_plan::identifier(&relation.name)
            || relation.arguments.len() > 3
        {
            continue;
        }
        let Some((class, factory_evidence)) = factories.get(&relation.related) else {
            continue;
        };
        let Some(key) =
            crate::eloquent_members::related_facts::primary_key(index, project, &relation.related)
        else {
            continue;
        };
        if relation
            .arguments
            .get(1)
            .is_some_and(|owner| owner != &key.name)
        {
            continue;
        }
        let relation_name = relation.arguments.get(2).unwrap_or(&relation.name);
        let foreign = if let Some(foreign) = relation.arguments.first() {
            foreign.clone()
        } else {
            let Ok(name) = crate::generator_presets::snake_name(relation_name) else {
                continue;
            };
            format!("{name}_{}", key.name)
        };
        if !crate::generator_plan::identifier(&foreign) || !fields.contains(&foreign) {
            continue;
        }
        let mut key_sources = BTreeMap::new();
        let mut total = 0;
        let mut valid = true;
        for path in key.evidence_paths {
            let Some(source) = read(project, &path) else {
                valid = false;
                break;
            };
            total += source.len();
            if total > 2 << 20 {
                valid = false;
                break;
            }
            key_sources.insert(path, source);
        }
        if !valid {
            continue;
        }
        let evidence = serde_json::to_string(&(
            relation_evidence,
            &relation.name,
            &relation.related,
            &relation.arguments,
            &key.name,
            key_sources,
            factory_evidence,
        ))
        .unwrap();
        choices.entry(foreign).or_default().push(Suggestion {
            class: class.clone(),
            model: relation.related,
            evidence,
        });
    }
    choices
        .into_iter()
        .filter_map(|(field, mut suggestions)| {
            (suggestions.len() == 1).then(|| (field, suggestions.remove(0)))
        })
        .collect()
}
pub(crate) fn for_draft(
    project: &Project,
    index: &Index,
    draft: &Draft,
) -> BTreeMap<String, Suggestion> {
    if !crate::index::index_sources_current(project, index) {
        return BTreeMap::new();
    }
    suggest(
        project,
        index,
        draft
            .relations
            .iter()
            .cloned()
            .map(|relation| (relation, String::new()))
            .collect(),
        &draft
            .fields
            .iter()
            .filter(|field| field.default.is_none())
            .map(|field| field.name.clone())
            .collect(),
    )
}
pub(crate) fn for_existing(
    project: &Project,
    index: &Index,
    model: &str,
    fields: &BTreeSet<String>,
) -> BTreeMap<String, Suggestion> {
    if !eligible(index) || !crate::index::index_sources_current(project, index) {
        return BTreeMap::new();
    }
    let Ok(root) = project.root() else {
        return BTreeMap::new();
    };
    let Some(facts) = crate::eloquent_members::helper_facts::facts(index, model, &|path| {
        super::owned(&root, path)
    }) else {
        return BTreeMap::new();
    };
    let mut relations = Vec::new();
    let owner_evidence = super::fresh_ancestor_evidence(
        project,
        &index.eloquent_models,
        model,
        "Illuminate\\Database\\Eloquent\\Model",
    )
    .ok();
    let Some(owner_evidence) = owner_evidence else {
        return BTreeMap::new();
    };
    for fact in facts
        .relations
        .into_iter()
        .filter(|fact| fact.kind == "belongsTo")
    {
        let Some(source) = read(project, &fact.entry.path) else {
            continue;
        };
        let Some(keys) =
            crate::eloquent_members::belongs_to_keys(index, model, &fact.name, &source, &|path| {
                super::owned(&root, path)
            })
        else {
            continue;
        };
        let Some(primary) =
            crate::eloquent_members::related_facts::primary_key(index, project, &keys.target)
        else {
            continue;
        };
        if keys
            .owner_key
            .as_ref()
            .is_some_and(|owner| owner != &primary.name)
        {
            continue;
        }
        let foreign = if let Some(foreign) = keys.foreign_key {
            foreign
        } else {
            let Ok(name) = crate::generator_presets::snake_name(&keys.relation_name) else {
                continue;
            };
            format!("{name}_{}", primary.name)
        };
        let relation = Relation {
            name: fact.name,
            kind: "belongsTo".into(),
            related: keys.target,
            through: None,
            arguments: vec![foreign, primary.name],
        };
        let evidence =
            serde_json::to_string(&(owner_evidence.as_str(), &fact.entry.path, source)).unwrap();
        relations.push((relation, evidence));
    }
    suggest(project, index, relations, fields)
}

#[cfg(test)]
pub(super) fn install_convention_fixture(h: &openidea_plugin::mock::MockHost) {
    let (factory, app) = conventions::fixture_sources();
    h.add_file(
        "vendor/laravel/framework/src/Illuminate/Database/Eloquent/Factories/Factory.php",
        &factory,
    );
    h.add_file(
        "vendor/laravel/framework/src/Illuminate/Foundation/Application.php",
        &app,
    );
    h.add_file("database/factories/UserFactory.php", "<?php namespace Database\\Factories;class UserFactory extends \\Illuminate\\Database\\Eloquent\\Factories\\Factory{public function definition():array{return [];}}");
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{host, mock::MockHost};
    fn setup() -> (MockHost, Project, Index) {
        let h = MockHost::install();
        h.add_dir("app");
        h.add_dir("app/Models");
        h.add_dir("database");
        h.add_dir("database/factories");
        h.add_file("artisan", "<?php");
        h.add_file("composer.json",r#"{"require":{"laravel/framework":"^12.0"},"autoload":{"psr-4":{"App\\":"app/","Database\\Factories\\":"database/factories/"}}}"#);
        h.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"laravel/framework","version":"v12.72.0"}]}"#,
        );
        h.add_file("app/Models/User.php","<?php namespace App\\Models; class User extends \\Illuminate\\Database\\Eloquent\\Model {}");
        h.add_file("app/Models/Post.php","<?php namespace App\\Models; class Post extends \\Illuminate\\Database\\Eloquent\\Model {public function author(){return $this->belongsTo(User::class);}}");
        h.add_file("database/factories/UserFactory.php","<?php namespace Database\\Factories; class UserFactory extends \\Illuminate\\Database\\Eloquent\\Factories\\Factory {protected $model=\\App\\Models\\User::class;public function definition():array{return [];}}");
        let project = host::projects().unwrap().remove(0);
        let index = Index::build(&project);
        assert_eq!(index.metadata.framework_version, Some((12, 72, 0)));
        (h, project, index)
    }
    fn native_fixture(h: &MockHost) {
        let (factory, app) = conventions::fixture_sources();
        h.add_file(
            "vendor/laravel/framework/src/Illuminate/Database/Eloquent/Factories/Factory.php",
            &factory,
        );
        h.add_file(
            "vendor/laravel/framework/src/Illuminate/Foundation/Application.php",
            &app,
        );
    }
    fn convention_factory(h: &MockHost) {
        h.add_file("database/factories/UserFactory.php","<?php namespace Database\\Factories;/** @extends \\Illuminate\\Database\\Eloquent\\Factories\\Factory<\\App\\Models\\User> */class UserFactory extends \\Illuminate\\Database\\Eloquent\\Factories\\Factory{public function definition():array{return [];}}");
    }
    #[test]
    fn ordinary_native_convention_factory_is_proven_without_model_property_or_phpdoc_trust() {
        let (h, p, _) = setup();
        native_fixture(&h);
        convention_factory(&h);
        let index = Index::build(&p);
        let defaults = for_draft(&p, &index, &draft());
        assert_eq!(
            defaults["author_id"].class,
            "Database\\Factories\\UserFactory"
        );
        h.add_file("app/Providers/FactoryProvider.php","<?php namespace App\\Providers;class FactoryProvider{public function boot(){\\Illuminate\\Database\\Eloquent\\Factories\\Factory::guessModelNamesUsing(fn()=>\\App\\Models\\Other::class);}}");
        assert!(for_draft(&p, &Index::build(&p), &draft()).is_empty());
    }
    #[test]
    fn convention_respects_configured_application_namespace_and_refuses_foreign_or_conflicting_paths(
    ) {
        let (h, p, _) = setup();
        native_fixture(&h);
        convention_factory(&h);
        h.add_file("composer.json",r#"{"require":{"laravel/framework":"^12.0"},"autoload":{"psr-4":{"Shop\\":"app/","Database\\Factories\\":"database/factories/"}}}"#);
        h.add_file("app/Models/User.php","<?php namespace Shop\\Models;class User extends \\Illuminate\\Database\\Eloquent\\Model{}");
        let index = Index::build(&p);
        let mut d = draft();
        d.relations[0].related = "Shop\\Models\\User".into();
        assert!(for_draft(&p, &index, &d).contains_key("author_id"));
        h.add_project("/project/vendor/laravel/framework");
        assert!(for_draft(&p, &Index::build(&p), &d).is_empty());
    }
    #[test]
    fn attributes_unknown_native_resolvers_and_unmapped_factories_withhold_convention_defaults() {
        let (h, p, _) = setup();
        native_fixture(&h);
        convention_factory(&h);
        let (factory, _) = conventions::fixture_sources();
        h.add_file(
            "vendor/laravel/framework/src/Illuminate/Database/Eloquent/Factories/Factory.php",
            &factory.replace("return $resolver($this);", "return 'Unexpected';"),
        );
        assert!(for_draft(&p, &Index::build(&p), &draft()).is_empty());
        native_fixture(&h);
        h.add_file("database/factories/UserFactory.php","<?php namespace Database\\Factories;#[\\Illuminate\\Database\\Eloquent\\Factories\\Attributes\\UseModel(\\App\\Models\\Post::class)]class UserFactory extends \\Illuminate\\Database\\Eloquent\\Factories\\Factory{protected $model=\\App\\Models\\User::class;}");
        assert!(for_draft(&p, &Index::build(&p), &draft()).is_empty());
        convention_factory(&h);
        h.add_file("composer.json",r#"{"require":{"laravel/framework":"^12.0"},"autoload":{"psr-4":{"App\\":"app/","Database\\Factories\\":"different/"}}}"#);
        assert!(for_draft(&p, &Index::build(&p), &draft()).is_empty());
    }
    fn draft() -> Draft {
        serde_json::from_value(serde_json::json!({"name":"Post","table":"posts","migrationStamp":"2026_11_10_120000","fields":[{"name":"author_id","kind":"unsignedBigInteger"}],"relations":[{"name":"author","kind":"belongsTo","related":"App\\Models\\User","arguments":[]}],"related":["factory"]})).unwrap()
    }
    #[test]
    #[ignore = "requires installed LARAVEL_FACTORY_FIXTURE with fakerphp/faker and PDO SQLite; temporary in-memory database only"]
    fn installed_laravel_factory_default_creates_the_proven_related_model_and_key() {
        for convention in [false, true] {
            let (h, project, mut index) = setup();
            if convention {
                native_fixture(&h);
                convention_factory(&h);
                index = Index::build(&project);
            }
            let mut draft = draft();
            draft.timestamps = false;
            let defaults = for_draft(&project, &index, &draft);
            let suggestion = &defaults["author_id"];
            draft.factory_mappings.insert(
                "author_id".into(),
                crate::generator_plan::FactoryMapping::Factory {
                    class: suggestion.class.clone(),
                    inferred: true,
                },
            );
            let references = crate::generator_plan::References {
                factories: BTreeSet::from([suggestion.class.clone()]),
                ..Default::default()
            };
            let plan = crate::generator_plan::plan_with_references(&draft, &references).unwrap();
            let root = std::env::temp_dir().join(format!(
                "laravel-factory-live-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir(&root).unwrap();
            let mut paths = Vec::new();
            for file in plan.files {
                let path = root.join(&file.path);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, file.text).unwrap();
                let lint = std::process::Command::new("php")
                    .arg("-l")
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(
                    lint.status.success(),
                    "{}",
                    String::from_utf8_lossy(&lint.stderr)
                );
                paths.push(path.to_string_lossy().to_string());
            }
            let fixture = std::env::var("LARAVEL_FACTORY_FIXTURE")
                .unwrap_or_else(|_| "/tmp/laravel-parity-filament-4-20261110".into());
            let script = root.join("accept.php");
            let php = r#"<?php
namespace {require $argv[1].'/vendor/autoload.php';$container=new \Illuminate\Foundation\Application(__DIR__);$container->instance(\Faker\Generator::class,\Faker\Factory::create('en_US'));$db=new \Illuminate\Database\Capsule\Manager($container);$db->addConnection(['driver'=>'sqlite','database'=>':memory:','foreign_key_constraints'=>true]);$db->setAsGlobal();$db->bootEloquent();$schema=$db->schema();$schema->create('users',function($table){$table->id();});$schema->create('posts',function($table){$table->id();$table->unsignedBigInteger('author_id');$table->foreign('author_id')->references('id')->on('users');});}
namespace App\Models {class User extends \Illuminate\Database\Eloquent\Model{protected $guarded=[];public $timestamps=false;}}
namespace Database\Factories {class UserFactory extends \Illuminate\Database\Eloquent\Factories\Factory{protected $model=\App\Models\User::class;public function definition():array{return [];}}}
namespace {foreach(json_decode($argv[2],true,512,JSON_THROW_ON_ERROR)as$path)require_once $path;$post=\Database\Factories\PostFactory::new()->create();$user=\App\Models\User::firstOrFail();if($post->author_id!==$user->getKey()||$post->author->getKey()!==$user->getKey()||\App\Models\User::count()!==1)throw new \RuntimeException('Factory did not create the declared related model/key');echo 'PASS installed Laravel factory: inferred belongsTo default creates related model and valid FK'.PHP_EOL;}
"#;
            std::fs::write(
                root.join("composer.json"),
                r#"{"autoload":{"psr-4":{"App\\":"app/"}}}"#,
            )
            .unwrap();
            let php = if convention {
                php.replace("protected $model=\\App\\Models\\User::class;", "")
            } else {
                php.to_owned()
            };
            std::fs::write(&script, php).unwrap();
            let output = std::process::Command::new("php")
                .arg(script)
                .arg(fixture)
                .arg(serde_json::to_string(&paths).unwrap())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            println!(
                "{} binding: {}",
                if convention { "convention" } else { "literal" },
                String::from_utf8_lossy(&output.stdout)
            );
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn default_factories_require_belongs_to_key_and_unique_literal_factory_binding() {
        let (_h, p, i) = setup();
        let d = draft();
        let defaults = for_draft(&p, &i, &d);
        assert_eq!(
            defaults["author_id"].class,
            "Database\\Factories\\UserFactory"
        );
        assert!(defaults["author_id"].evidence.contains("User.php"));
        let existing = for_existing(
            &p,
            &i,
            "App\\Models\\Post",
            &BTreeSet::from(["author_id".into()]),
        );
        assert_eq!(existing["author_id"].class, defaults["author_id"].class);
        let mut configured = d.clone();
        configured.fields[0].default = Some(serde_json::json!(7));
        assert!(
            for_draft(&p, &i, &configured).is_empty(),
            "an explicit field default remains an override"
        );
        let mut bare = d;
        bare.relations.clear();
        assert!(for_draft(&p, &i, &bare).is_empty());
    }
    #[test]
    fn ambiguous_factories_unknown_keys_and_alternate_owner_keys_withhold_defaults() {
        let (h, p, i) = setup();
        let mut d = draft();
        d.relations[0].arguments = vec!["author_id".into(), "email".into()];
        assert!(for_draft(&p, &i, &d).is_empty());
        h.add_file("database/factories/OtherUserFactory.php","<?php namespace Database\\Factories;class OtherUserFactory extends \\Illuminate\\Database\\Eloquent\\Factories\\Factory{protected $model=\\App\\Models\\User::class;}");
        assert!(for_draft(&p, &Index::build(&p), &draft()).is_empty());
        h.add_file(
            "database/factories/OtherUserFactory.php",
            "<?php // removed factory",
        );
        h.add_file("app/Models/User.php","<?php namespace App\\Models;class User extends \\Illuminate\\Database\\Eloquent\\Model{public function getKeyName(){return config('key');}}");
        assert!(for_draft(&p, &Index::build(&p), &draft()).is_empty());
    }
    #[test]
    fn custom_primary_keys_and_explicit_foreign_keys_are_proven_without_id_suffixes() {
        let (h, p, _i) = setup();
        h.add_file("app/Models/User.php","<?php namespace App\\Models;class User extends \\Illuminate\\Database\\Eloquent\\Model{protected $primaryKey='uuid';}");
        h.add_file("app/Models/Post.php","<?php namespace App\\Models;class Post extends \\Illuminate\\Database\\Eloquent\\Model{public function owner(){return $this->belongsTo(User::class,'owner_ref','uuid');}}");
        let i = Index::build(&p);
        let mut d = draft();
        d.fields[0].name = "author_uuid".into();
        assert!(for_draft(&p, &i, &d).contains_key("author_uuid"));
        let existing = for_existing(
            &p,
            &i,
            "App\\Models\\Post",
            &BTreeSet::from(["owner_ref".into()]),
        );
        assert!(existing.contains_key("owner_ref"));
        d.relations.push(d.relations[0].clone());
        assert!(for_draft(&p, &i, &d).is_empty());
    }
    #[test]
    fn dynamic_relation_keys_factory_overrides_attributes_and_foreign_sources_do_not_infer() {
        let (h, p, _i) = setup();
        h.add_file("app/Models/Post.php","<?php namespace App\\Models;class Post extends \\Illuminate\\Database\\Eloquent\\Model{public function author(){return $this->belongsTo(User::class,config('key'));}}");
        let i = Index::build(&p);
        assert!(for_existing(
            &p,
            &i,
            "App\\Models\\Post",
            &BTreeSet::from(["author_id".into()])
        )
        .is_empty());
        h.add_file("database/factories/UserFactory.php","<?php namespace Database\\Factories;class UserFactory extends \\Illuminate\\Database\\Eloquent\\Factories\\Factory{protected $model=\\App\\Models\\User::class;public function newModel(array $attributes=[]){return new \\App\\Models\\Post;}}");
        assert!(for_draft(&p, &Index::build(&p), &draft()).is_empty());
        h.add_project("/project/database/factories");
        assert!(for_draft(&p, &Index::build(&p), &draft()).is_empty());
    }
}
