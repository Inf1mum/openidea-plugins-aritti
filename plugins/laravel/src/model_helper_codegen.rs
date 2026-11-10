//! Reviewable helper annotations from typed cached facts and exact owned sources.
use crate::{
    eloquent_members::helper_facts,
    generator_plan,
    index::{Entry, Index},
};
use openidea_plugin::{Error, Project};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone)]
pub(super) struct Options {
    pub relation_counts: bool,
    pub date_class:String,
}
impl Default for Options{fn default()->Self{Self{relation_counts:false,date_class:"Illuminate\\Support\\Carbon".into()}}}
fn scalar(kind: &str) -> &'static str {
    match kind {
        "boolean" => "bool",
        "integer"
        | "tinyInteger"
        | "smallInteger"
        | "mediumInteger"
        | "bigInteger"
        | "unsignedInteger"
        | "unsignedTinyInteger"
        | "unsignedSmallInteger"
        | "unsignedMediumInteger"
        | "unsignedBigInteger"
        | "foreignId"
        | "id"
        | "increments"
        | "tinyIncrements"
        | "smallIncrements"
        | "mediumIncrements"
        | "bigIncrements" => "int",
        "float" | "double" => "float",
        "decimal" | "unsignedDecimal" => "string",
        "json" | "jsonb" => "array",
        "date" | "dateTime" | "dateTimeTz" | "timestamp" | "timestampTz" | "timestamps"
        | "timestampsTz" | "softDeletes" | "softDeletesTz" => "\\Illuminate\\Support\\Carbon",
        "string" | "char" | "text" | "tinyText" | "mediumText" | "longText" | "uuid" | "ulid" => {
            "string"
        }
        _ => "mixed",
    }
}
fn cast_type(cast: &helper_facts::CastFact,date_class:&str) -> String {
    if matches!(cast.label.split(':').next(),Some("date"|"datetime"|"custom_datetime")){return format!("\\{date_class}");}
    if let Some(resolved) = cast
        .resolved
        .as_ref()
        .filter(|class| generator_plan::qualified(class))
    {
        return format!("\\{resolved}");
    }
    match cast.label.split(':').next().unwrap_or("") {
        "int" | "integer" | "timestamp" => "int",
        "real" | "float" | "double" => "float",
        "decimal" | "string" | "hashed" => "string",
        "bool" | "boolean" => "bool",
        "array" | "json" => "array",
        "object" => "\\stdClass",
        "collection" => "\\Illuminate\\Support\\Collection<array-key,mixed>",
        "date" | "datetime" | "custom_datetime" => "\\Illuminate\\Support\\Carbon",
        "immutable_date" | "immutable_datetime" | "immutable_custom_datetime" => {
            "\\Carbon\\CarbonImmutable"
        }
        "encrypted" => match cast.label.strip_prefix("encrypted:") {
            None => "string",
            Some("array" | "json") => "array",
            Some("object") => "\\stdClass",
            Some("collection") => "\\Illuminate\\Support\\Collection<array-key,mixed>",
            _ => "mixed",
        },
        _ => match cast.label.as_str() {
            "Illuminate\\Database\\Eloquent\\Casts\\AsArrayObject" => {
                "\\ArrayObject<array-key,mixed>"
            }
            "Illuminate\\Database\\Eloquent\\Casts\\AsCollection" => {
                "\\Illuminate\\Support\\Collection<array-key,mixed>"
            }
            _ => "mixed",
        },
    }
    .into()
}
fn nullable(project: &Project, entry: Option<&Entry>) -> Option<bool> {
    let entry = entry?;
    if !crate::owns_current_path(project, &entry.path) {
        return None;
    }
    let source = project.read_file(&entry.path).ok()?;
    helper_facts::column_nullable(&source, entry)
}
fn union_null(kind: String, nullable: Option<bool>) -> String {
    if kind == "mixed" || nullable == Some(false) || kind.split('|').any(|part| part == "null") {
        kind
    } else {
        format!("{kind}|null")
    }
}
pub(super) fn eligible(index: &Index, full: &str) -> bool {
    let lower = full.trim_start_matches('\\').to_ascii_lowercase();
    !lower.starts_with("tests\\") && lower != "tests" && !index.metadata.helper_ignored(full)
}
pub(super) fn tags(
    index: &Index,
    project: &Project,
    full: &str,
    options: Options,
) -> Result<String, Error> {
    let facts = helper_facts::facts(index, full, &|path| crate::owns_current_path(project, path))
        .ok_or(Error::Stale)?;
    let columns = facts
        .table
        .as_ref()
        .and_then(|table| index.columns.get(table));
    let mut properties = BTreeMap::<String, (String, bool)>::new();
    if let Some(columns) = columns {
        for (name, entry) in columns {
            if generator_plan::identifier(name) && crate::owns_current_path(project, &entry.path) {
                let kind = if facts.casts_complete {
                    let kind=scalar(entry.detail.as_deref().unwrap_or(""));
                    if kind=="\\Illuminate\\Support\\Carbon"{format!("\\{}",options.date_class)}else{kind.into()}
                } else {
                    "mixed".into()
                };
                properties.insert(
                    name.clone(),
                    (union_null(kind, nullable(project, Some(entry))), false),
                );
            }
        }
    }
    for cast in &facts.casts {
        if generator_plan::identifier(&cast.name)
            && crate::owns_current_path(project, &cast.entry.path)
        {
            properties.insert(
                cast.name.clone(),
                (
                    union_null(
                        cast_type(cast,&options.date_class),
                        if cast.resolved.is_some() {
                            None
                        } else {
                            nullable(project, columns.and_then(|columns| columns.get(&cast.name)))
                        },
                    ),
                    false,
                ),
            );
        }
    }
    for relation in &facts.relations {
        if !generator_plan::identifier(&relation.name)
            || !generator_plan::qualified(&relation.target)
            || !crate::owns_current_path(project, &relation.entry.path)
        {
            continue;
        }
        let kind = if relation.many {
            format!(
                "\\Illuminate\\Database\\Eloquent\\Collection<int,\\{}>",
                relation.target
            )
        } else {
            format!("\\{}|null", relation.target)
        };
        properties
            .entry(relation.name.clone())
            .or_insert((kind, true));
        if options.relation_counts && relation.many {
            let name = format!("{}_count", crate::index::separated(&relation.name, '_'));
            properties.entry(name).or_insert(("int|null".into(), true));
        }
    }
    for accessor in crate::eloquent_members::related_facts::accessors(index,project,full){if generator_plan::identifier(&accessor.name){properties.insert(accessor.name,(accessor.value_type,true));}}
    let mut out = properties
        .into_iter()
        .map(|(name, (kind, read_only))| {
            format!(
                " * @property{} {kind} ${name}\n",
                if read_only { "-read" } else { "" }
            )
        })
        .collect::<String>();
    let mut methods = BTreeSet::new();
    for scope in &facts.scopes {
        if !generator_plan::identifier(&scope.name)
            || !crate::owns_current_path(project, &scope.entry.path)
        {
            continue;
        }
        let source = project.read_file(&scope.entry.path)?;
        if let Some(signature) = helper_facts::scope_signature(&source, &scope.entry, full) {
            if methods.insert(scope.name.clone()) {
                out.push_str(&format!(
                    " * @method static {} {}({})\n",
                    signature.return_type,
                    scope.name,
                    signature.parameters.join(", ")
                ));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{host, mock::MockHost};
    fn fixture() -> (MockHost, Project, Index) {
        let mock = MockHost::install();
        mock.add_file("app/Models/Post.php",r#"<?php namespace App\Models;
use Illuminate\Database\Eloquent\Model; use Illuminate\Database\Eloquent\Builder;
use App\Enums\Status; use App\Casts\MoneyCast;
class Post extends Model {
 protected $casts=['enabled'=>'boolean','price'=>'decimal:3','status'=>Status::class,'money'=>MoneyCast::class];
 public function comments(){return $this->hasMany(Comment::class);}
 public function author(){return $this->belongsTo(User::class);}
 public function unknown(){return $this->hasMany(Unindexed::class);}
 public function scopeActive(Builder $query, bool $enabled=true, ?string $needle=null): void {}
 public function scopeUnproven($query){return 42;}
 public function scopeWrong(int $query){return $query;}
}"#);
        for class in ["Comment", "User"] {
            mock.add_file(&format!("app/Models/{class}.php"),&format!("<?php namespace App\\Models;use Illuminate\\Database\\Eloquent\\Model;class {class} extends Model {{}}"));
        }
        mock.add_file(
            "app/Enums/Status.php",
            "<?php namespace App\\Enums; enum Status: string {case Draft='draft';}",
        );
        mock.add_file(
            "app/ValueObjects/Money.php",
            "<?php namespace App\\ValueObjects; class Money {public int $amount;}",
        );
        mock.add_file("app/Casts/MoneyCast.php","<?php namespace App\\Casts; use Illuminate\\Contracts\\Database\\Eloquent\\CastsAttributes; use App\\ValueObjects\\Money; class MoneyCast implements CastsAttributes {public function get($model,$key,$value,$attributes): ?Money {return null;}}");
        mock.add_file("database/migrations/001.php","<?php Schema::create('posts',function($t){$t->boolean('enabled');$t->decimal('price')->nullable();$t->string('status');$t->integer('money');$t->string('title')->nullable(false);});");
        let project = host::projects().unwrap().remove(0);
        let index = Index::build(&project);
        (mock, project, index)
    }
    #[test]
    fn typed_helpers_preserve_relations_counts_casts_and_proven_scope_parameters() {
        let (mock, project, index) = fixture();
        let generated = tags(
            &index,
            &project,
            "App\\Models\\Post",
            Options {
                relation_counts: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(generated.contains("@property bool $enabled"), "{generated}");
        assert!(generated.contains("@property string|null $price"));
        assert!(generated.contains("@property \\App\\Enums\\Status|null $status"));
        assert!(generated.contains("@property \\App\\ValueObjects\\Money|null $money"));
        assert!(generated.contains("@property string $title"));
        assert!(generated.contains("@property-read \\Illuminate\\Database\\Eloquent\\Collection<int,\\App\\Models\\Comment> $comments"));
        assert!(generated.contains("@property-read \\App\\Models\\User|null $author"));
        assert!(generated.contains("@property-read int|null $comments_count"));
        assert!(!generated.contains("$author_count"));
        assert!(
            generated.contains("active(bool $enabled = true, ?string $needle = null)"),
            "{generated}"
        );
        assert!(!generated.contains("unproven("));
        assert!(!generated.contains("wrong("));
        assert!(!generated.contains("$unknown"));
        if let Ok(dir) = std::env::var("LARAVEL_HELPER_FIXTURES_DIR") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                std::path::Path::new(&dir).join("typed_relations_casts_scopes.php"),
                super::super::helper_text("App\\Models\\Post", &generated).unwrap(),
            )
            .unwrap();
        }
        assert!(mock.process_runs().is_empty());
    }
    #[test]
    fn counts_are_opt_in_and_dynamic_casts_do_not_claim_schema_scalar_types() {
        let (mock, project, index) = fixture();
        assert!(
            !tags(&index, &project, "App\\Models\\Post", Options::default())
                .unwrap()
                .contains("$comments_count")
        );
        mock.add_file("app/Models/Post.php","<?php namespace App\\Models;use Illuminate\\Database\\Eloquent\\Model;class Post extends Model {protected function casts(): array {return config('casts');}}");
        let index = Index::build(&project);
        let generated = tags(&index, &project, "App\\Models\\Post", Options::default()).unwrap();
        assert!(generated.contains("@property mixed $title"));
    }
}
