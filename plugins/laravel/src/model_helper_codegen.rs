//! Reviewable helper annotations from typed cached facts and exact owned sources.
use crate::{
    eloquent_members::helper_facts,
    generator_plan,
    index::{Entry, Index},
};
use openidea_plugin::{Error, Project};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Options {
    pub relation_counts: bool,
    pub date_class: String,
    pub source: String,
    pub schema: Option<serde_json::Value>,
    pub collection_proxies: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            relation_counts: false,
            date_class: "Illuminate\\Support\\Carbon".into(),
            source: "migrations".into(),
            schema: None,
            collection_proxies: false,
        }
    }
}
fn cast_type(cast: &helper_facts::CastFact, date_class: &str) -> String {
    if matches!(
        cast.label.split(':').next(),
        Some("date" | "datetime" | "custom_datetime")
    ) {
        return format!("\\{date_class}");
    }
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
fn read_annotation(kind: &str, date_class: &str) -> String {
    kind.split('|')
        .map(|atom| {
            let atom = atom.trim();
            if atom.trim_start_matches('\\') == "Illuminate\\Support\\Carbon" {
                format!("\\{date_class}")
            } else if atom.contains('\\') && !atom.starts_with('\\') {
                format!("\\{atom}")
            } else {
                atom.into()
            }
        })
        .collect::<Vec<_>>()
        .join("|")
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
    if options.source != "database" {
        if let Some(columns) = columns {
            for (name, entry) in columns {
                if generator_plan::identifier(name)
                    && crate::owns_current_path(project, &entry.path)
                {
                    let read = crate::eloquent_members::semantic::read_type(
                        index,
                        full,
                        name,
                        entry.detail.as_deref(),
                        &|path| crate::owns_current_path(project, path),
                    );
                    let kind = read
                        .as_ref()
                        .map(|read| read_annotation(&read.value_type, &options.date_class))
                        .unwrap_or_else(|| "mixed".into());
                    properties.insert(
                        name.clone(),
                        (
                            union_null(
                                kind,
                                read.as_ref()
                                    .and_then(|read| read.nullable)
                                    .or_else(|| nullable(project, Some(entry))),
                            ),
                            read.as_ref().is_some_and(|read| read.read_only),
                        ),
                    );
                }
            }
        }
    }
    let mut database_nullability = BTreeMap::new();
    if options.source != "migrations" {
        let schema=options.schema.as_ref().ok_or_else(||Error::Invalid(crate::ui_text::text("final.model_helper_codegen.error0","Refresh model connection and matching database metadata before generating database helpers").into()))?;
        for column in schema["columns"].as_array().ok_or(Error::Stale)? {
            let name = column["name"].as_str().ok_or(Error::Stale)?;
            if !generator_plan::identifier(name) {
                continue;
            }
            let nullable = column["nullable"].as_bool().ok_or(Error::Stale)?;
            let raw = column["type"]
                .as_str()
                .ok_or(Error::Stale)?
                .to_ascii_lowercase();
            let base = raw.split(['(', ' ']).next().unwrap_or("");
            let kind = match base {
                "tinyint" | "smallint" | "mediumint" | "int" | "integer" | "bigint" | "int2"
                | "int4" | "int8" | "serial" | "bigserial" => "int".into(),
                "bool" | "boolean" => "bool".into(),
                "float" | "double" | "real" | "float4" | "float8" => "float".into(),
                "decimal" | "numeric" | "varchar" | "nvarchar" | "char" | "text" | "tinytext"
                | "mediumtext" | "longtext" | "uuid" | "enum" | "set" | "blob" | "binary"
                | "varbinary" => "string".into(),
                "json" | "jsonb" | "date" | "time" | "datetime" | "timestamp" | "timestamptz" => {
                    "string".into()
                }
                _ => "mixed".into(),
            };
            let read = crate::eloquent_members::semantic::read_type(
                index,
                full,
                name,
                Some(base),
                &|path| crate::owns_current_path(project, path),
            );
            let kind = read
                .as_ref()
                .map(|read| read_annotation(&read.value_type, &options.date_class))
                .unwrap_or(kind);
            properties.insert(
                name.into(),
                (
                    union_null(
                        kind,
                        read.as_ref()
                            .and_then(|read| read.nullable)
                            .or(Some(nullable)),
                    ),
                    read.as_ref().is_some_and(|read| read.read_only),
                ),
            );
            database_nullability.insert(name.to_owned(), nullable);
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
                        cast_type(cast, &options.date_class),
                        if cast.resolved.is_some() {
                            None
                        } else {
                            database_nullability.get(&cast.name).copied().or_else(|| {
                                nullable(
                                    project,
                                    columns.and_then(|columns| columns.get(&cast.name)),
                                )
                            })
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
    for field in crate::eloquent_members::related_facts::resource_fields(index, project, full) {
        if generator_plan::identifier(&field.name) && !properties.contains_key(&field.name) {
            let kind = crate::eloquent_members::semantic::read_type(
                index,
                full,
                &field.name,
                None,
                &|path| crate::owns_current_path(project, path),
            )
            .map(|read| read_annotation(&read.value_type, &options.date_class))
            .unwrap_or_else(|| "mixed".into());
            properties.insert(field.name, (kind, true));
        }
    }
    let accessors = crate::eloquent_members::semantic::accessor_facts(index, full, &|path| {
        crate::owns_current_path(project, path)
    })
    .unwrap_or_default()
    .into_iter()
    .filter(|fact| generator_plan::identifier(&fact.name))
    .map(|fact| (fact.name.clone(), fact))
    .collect::<BTreeMap<_, _>>();
    let stored = columns
        .into_iter()
        .flat_map(|columns| columns.keys())
        .filter(|_| options.source != "database")
        .chain(database_nullability.keys())
        .cloned()
        .chain(facts.casts.iter().map(|cast| cast.name.clone()))
        .collect::<BTreeSet<_>>();
    for name in accessors.keys() {
        properties
            .entry(name.clone())
            .or_insert(("mixed".into(), true));
    }
    let mut out = properties
        .into_iter()
        .map(|(name, (kind, read_only))| {
            let Some(accessor) = accessors.get(&name) else {
                return format!(
                    " * @property{} {kind} ${name}\n",
                    if read_only { "-read" } else { "" }
                );
            };
            let stored = stored.contains(&name);
            let read = match accessor.has_getter {
                Some(true) => Some(
                    accessor
                        .read_type
                        .as_deref()
                        .map(|kind| read_annotation(kind, &options.date_class))
                        .unwrap_or_else(|| "mixed".into()),
                ),
                Some(false) => stored.then_some(kind),
                None => Some("mixed".into()),
            };
            let write = match accessor.has_setter {
                Some(true) => Some(
                    accessor
                        .write_type
                        .as_deref()
                        .map(|kind| read_annotation(kind, &options.date_class))
                        .unwrap_or_else(|| "mixed".into()),
                ),
                Some(false) => stored.then(|| "mixed".into()),
                None => Some("mixed".into()),
            };
            match (read, write) {
                (Some(read), Some(write)) if read == write => {
                    format!(" * @property {read} ${name}\n")
                }
                (read, write) => format!(
                    "{}{}",
                    read.map(|kind| format!(" * @property-read {kind} ${name}\n"))
                        .unwrap_or_default(),
                    write
                        .map(|kind| format!(" * @property-write {kind} ${name}\n"))
                        .unwrap_or_default()
                ),
            }
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
    if options.collection_proxies {
        let (collection, _) = proxy_names(full);
        out.push_str(&format!(" * @method static \\{collection} all(array|string $columns = ['*'])\n * @method static \\{collection} get(array|string $columns = ['*'])\n"));
    }
    Ok(out)
}
pub(super) fn proxy_names(full: &str) -> (String, String) {
    let token = crate::artisan_metadata::hash(full);
    let token = &token[..16];
    (
        format!("OpenIDEA\\Laravel\\Generated\\Model{token}Collection"),
        format!("OpenIDEA\\Laravel\\Generated\\Model{token}Proxy"),
    )
}
pub(super) fn proxy_code(full: &str) -> String {
    let (collection_full, proxy) = proxy_names(full);
    let collection = collection_full.rsplit('\\').next().unwrap();
    let proxy_short = proxy.rsplit('\\').next().unwrap();
    let properties = [
        "average",
        "avg",
        "contains",
        "each",
        "every",
        "filter",
        "first",
        "flatMap",
        "groupBy",
        "keyBy",
        "map",
        "max",
        "min",
        "partition",
        "reject",
        "some",
        "sortBy",
        "sortByDesc",
        "sum",
        "unique",
    ]
    .into_iter()
    .map(|name| format!(" * @property-read \\{proxy} ${name}\n"))
    .collect::<String>();
    format!(
        "\nnamespace OpenIDEA\\Laravel\\Generated;\n\n/**\n * @extends \\Illuminate\\Database\\Eloquent\\Collection<int,\\{full}>\n{properties} */\nclass {collection} extends \\Illuminate\\Database\\Eloquent\\Collection {{}}\n\n/**\n * @mixin \\{full}\n * @extends \\Illuminate\\Support\\HigherOrderCollectionProxy<string,\\{full},\\{collection_full}>\n */\nclass {proxy_short} extends \\Illuminate\\Support\\HigherOrderCollectionProxy {{}}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{host, mock::MockHost};
    fn fixture() -> (MockHost, Project, Index) {
        let mock = MockHost::install();
        mock.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"laravel/framework","version":"v13.40.0"}]}"#,
        );
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
    fn modern_and_classic_accessor_helpers_keep_independent_read_and_write_contracts() {
        let (mock, project, _) = fixture();
        mock.add_file("app/Models/Post.php",r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model;use Illuminate\Database\Eloquent\Casts\Attribute;
class Post extends Model {
 protected function title(): Attribute {return Attribute::make(get:fn(?string $value): string => strtoupper($value ?? ''),set:fn(int $value): string => (string)$value);}
 protected function virtualCode(): Attribute {return Attribute::get(fn(): int => 1);}
 public function getEnabledAttribute($value): bool {return (bool)$value;}
 public function setSecretAttribute(string $value): void {$this->attributes['secret']=$value;}
}"#);
        let index = Index::build(&project);
        let generated = tags(&index, &project, "App\\Models\\Post", Options::default()).unwrap();
        for expected in [
            "@property-read string $title",
            "@property-write int $title",
            "@property-read int $virtual_code",
            "@property-read bool $enabled",
            "@property-write mixed $enabled",
            "@property-write string $secret",
        ] {
            assert!(
                generated.contains(expected),
                "missing {expected}: {generated}"
            );
        }
        assert!(!generated.contains("@property-write mixed $virtual_code"));
        assert!(mock.process_runs().is_empty());
    }
    #[test]
    fn database_fields_are_authoritative_and_migrations_only_supplement_when_selected() {
        let (mock, project, _) = fixture();
        mock.add_file(
            "database/migrations/002.php",
            "<?php Schema::table('posts',function($t){$t->string('migration_only');});",
        );
        let index = Index::build(&project);
        let schema = serde_json::json!({"columns":[{"name":"title","type":"integer","nullable":false},{"name":"runtime_only","type":"text","nullable":true}]});
        for source in ["both", "database"] {
            let generated = tags(
                &index,
                &project,
                "App\\Models\\Post",
                Options {
                    source: source.into(),
                    schema: Some(schema.clone()),
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(
                generated.contains("@property int|string $title"),
                "{generated}"
            );
            assert!(generated.contains("@property string|null $runtime_only"));
            assert_eq!(generated.contains("$migration_only"), source == "both");
        }
        assert!(
            tags(
                &index,
                &project,
                "App\\Models\\Post",
                Options {
                    source: "database".into(),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(mock.process_runs().is_empty());
    }
    #[test]
    fn collection_proxy_classes_are_stable_valid_owned_support_output() {
        let (_mock, project, index) = fixture();
        let full = "App\\Models\\Post";
        let generated = tags(
            &index,
            &project,
            full,
            Options {
                collection_proxies: true,
                ..Default::default()
            },
        )
        .unwrap();
        let (collection, proxy) = proxy_names(full);
        assert!(generated.contains(&format!("@method static \\{collection} all")));
        let code = format!("<?php\n{}", proxy_code(full));
        assert!(code.contains(&format!("@property-read \\{proxy} $map")));
        assert!(code.contains(&format!(
            "HigherOrderCollectionProxy<string,\\{full},\\{collection}>"
        )));
        assert_eq!(proxy_names(full), proxy_names(full));
        assert_ne!(proxy_names(full), proxy_names("Other\\Models\\Post"));
        if let Ok(directory) = std::env::var("LARAVEL_HELPER_FIXTURES_DIR") {
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(
                std::path::Path::new(&directory).join("collection_proxy.php"),
                &code,
            )
            .unwrap();
        }
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
