//! Prepared acceptance cases; execution intentionally deferred at the user's request.
use super::*;
use crate::tests::{cursor, doc};
fn source(index: &mut Index, path: &str, text: &str) {
    cache(&mut index.eloquent_models, text, path);
    cache_object(&mut index.eloquent_objects, text, path);
}
fn inventory() -> Index {
    let mut index = Index::default();
    for (path, text) in [
        (
            "app/Models/User.php",
            r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; class User extends Model {}"#,
        ),
        (
            "app/Models/Other.php",
            r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; class Other extends Model {}"#,
        ),
        (
            "app/Traits/Primary.php",
            r#"<?php namespace App\Traits; use App\Models\User; trait Primary { public function owner(){return $this->belongsTo(User::class);} public function scopeActive($q){return $q;} public function typed(): User{return new User;} }"#,
        ),
        (
            "app/Traits/Secondary.php",
            r#"<?php namespace App\Traits; use App\Models\Other; trait Secondary { public function owner(){return $this->belongsTo(Other::class);} }"#,
        ),
        (
            "app/Models/Order.php",
            r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; use App\Traits\Primary; use App\Traits\Secondary; class Order extends Model {use Primary,Secondary {Primary::owner insteadof Secondary; Secondary::owner as otherOwner; Primary::scopeActive as scopePublished; Primary::typed as customer;} }"#,
        ),
    ] {
        source(&mut index, &format!("/project/{path}"), text);
    }
    for (table, names) in [
        ("users", vec!["email"]),
        ("others", vec!["title"]),
        ("orders", vec!["total"]),
    ] {
        for name in names {
            index.columns.entry(table.into()).or_default().insert(
                name.into(),
                Entry {
                    path: format!("/project/database/migrations/{table}.php"),
                    at: Position::new(1, 0),
                    width: name.len() as u32,
                    detail: None,
                },
            );
        }
    }
    finish(&mut index.eloquent_models);
    index
}
fn labels(index: &Index, text: &str) -> Vec<String> {
    let (text, at) = cursor(text);
    completion(index, &doc("/project/Test.php", "php", &text), at)
        .into_iter()
        .map(|item| item.label)
        .collect()
}
#[test]
fn trait_precedence_alias_scope_and_typed_method_preserve_source_targets() {
    let index = inventory();
    assert!(
        labels(
            &index,
            r#"<?php use App\Models\Order; Order::first()->owner->^"#
        )
        .contains(&"email".into())
    );
    assert!(
        labels(
            &index,
            r#"<?php use App\Models\Order; Order::first()->otherOwner->^"#
        )
        .contains(&"title".into())
    );
    assert!(
        labels(
            &index,
            r#"<?php use App\Models\Order; Order::first()->customer()->^"#
        )
        .contains(&"email".into())
    );
    assert!(
        labels(&index, r#"<?php use App\Models\Order; Order::^"#).contains(&"published()".into())
    );
    let (text, at) = cursor(r#"<?php use App\Models\Order; Order::first()->otherOw^ner"#);
    assert_eq!(
        definition(&index, &doc("/project/Test.php", "php", &text), at)
            .unwrap()
            .path,
        "/project/app/Models/Order.php"
    );
    let names = completion_checked(
        &index,
        &doc("/project/Test.php", "php", &text),
        at,
        &|path| !path.contains("/Traits/Secondary"),
    );
    assert!(names.is_empty());
}
#[test]
fn private_adaptation_and_unresolved_conflicts_withhold_members() {
    let mut index = inventory();
    index.eloquent_models.remove("app\\models\\order");
    cache(
        &mut index.eloquent_models,
        r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; use App\Traits\Primary; class Order extends Model {use Primary {owner as private; typed as private hidden;} }"#,
        "/project/app/Models/Order.php",
    );
    finish(&mut index.eloquent_models);
    let names = labels(&index, r#"<?php use App\Models\Order; Order::first()->^"#);
    assert!(!names.contains(&"owner".into()));
    assert!(!names.contains(&"hidden()".into()));
    assert!(names.contains(&"total".into()));
    index.eloquent_models.remove("app\\models\\order");
    cache(
        &mut index.eloquent_models,
        r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; use App\Traits\Primary; use App\Traits\Secondary; class Order extends Model {use Primary,Secondary {Primary::owner insteadof Secondary; Secondary::owner insteadof Primary;}}"#,
        "/project/app/Models/Order.php",
    );
    finish(&mut index.eloquent_models);
    assert!(
        labels(
            &index,
            r#"<?php use App\Models\Order; Order::first()->owner->^"#
        )
        .is_empty()
    );
}
#[test]
fn nullable_union_and_proven_concrete_intersection_keep_safe_return_members() {
    let mut index = inventory();
    source(
        &mut index,
        "/project/app/Contracts/Tagged.php",
        r#"<?php namespace App\Contracts; interface Tagged {}"#,
    );
    index.eloquent_models.remove("app\\models\\order");
    cache(
        &mut index.eloquent_models,
        r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; use App\Contracts\Tagged; class Order extends Model {public function nullable(): User|null{return null;}public function intersection(): User&Tagged{return new User;}public function ambiguous(): User|Other{return new Other;}}"#,
        "/project/app/Models/Order.php",
    );
    finish(&mut index.eloquent_models);
    for method in ["nullable", "intersection"] {
        assert!(
            labels(
                &index,
                &format!("<?php use App\\Models\\Order; Order::first()->{method}()->^")
            )
            .contains(&"email".into())
        );
    }
    assert!(
        labels(
            &index,
            r#"<?php use App\Models\Order; Order::first()->ambiguous()->^"#
        )
        .is_empty()
    );
}
#[test]
fn composer_source_roots_match_declarations_and_respect_nested_project_ownership() {
    use openidea_plugin::mock::MockHost;
    let host = MockHost::install();
    host.add_file(
        "composer.json",
        r#"{"require":{"laravel/framework":"^12.0"},"autoload":{"psr-4":{"Domain\\":"domain/"}}}"#,
    );
    host.add_file("domain/Models/Invoice.php",r#"<?php namespace Domain\Models; use Illuminate\Database\Eloquent\Model; use Domain\Concerns\HasOwner; class Invoice extends Model {use HasOwner;}"#);
    host.add_file("domain/Concerns/HasOwner.php",r#"<?php namespace Domain\Concerns; use Domain\Models\User; trait HasOwner {public function owner(){return $this->belongsTo(User::class);}}"#);
    host.add_file("domain/Models/User.php",r#"<?php namespace Domain\Models; use Illuminate\Database\Eloquent\Model; class User extends Model {}"#);
    host.add_file(
        "domain/Models/Wrong.php",
        r#"<?php namespace Unmapped; class Wrong {}"#,
    );
    host.add_file(
        "database/migrations/2026_users.php",
        r#"<?php Schema::create('users',function($table){$table->string('email');});"#,
    );
    let index = Index::build(&host.invocation().project().unwrap());
    assert!(
        index
            .eloquent_models
            .contains_key("domain\\models\\invoice")
    );
    assert!(!index.eloquent_models.contains_key("unmapped\\wrong"));
    assert!(
        labels(
            &index,
            r#"<?php use Domain\Models\Invoice; Invoice::first()->owner->^"#
        )
        .contains(&"email".into())
    );
    host.add_project("/project/domain/Concerns");
    let index = Index::build(&host.invocation().project().unwrap());
    assert!(
        !index
            .eloquent_models
            .contains_key("domain\\concerns\\hasowner")
    );
}
#[test]
fn direct_with_count_chains_complete_only_known_relationship_counts() {
    let index = inventory();
    let names = labels(
        &index,
        r#"<?php use App\Models\Order; Order::withCount(['owner','otherOwner as alternate_count'])->first()->^"#,
    );
    assert!(names.contains(&"owner_count".into()));
    assert!(names.contains(&"alternate_count".into()));
    assert!(
        !labels(
            &index,
            r#"<?php use App\Models\Order; Order::withCount('unknown')->first()->^"#
        )
        .contains(&"unknown_count".into())
    );
    assert!(
        !labels(
            &index,
            r#"<?php use App\Models\Order; Order::withCount('owner')->first()->owner->^"#
        )
        .contains(&"owner_count".into())
    );
}
