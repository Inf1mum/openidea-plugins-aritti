//! Migration-vs-database comparison and the schema diagram graph.
//! Built on this plugin's migration replay.
use crate::migrations::keys::{Place, Schema, Table};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

const INTERNAL: [&str; 3] = ["migrations", "sqlite_sequence", "spatial_ref_sys"];
pub(crate) const MAX_LIVE_TABLES: usize = 512;

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LiveIndex {
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LiveTable {
    pub name: String,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub indexes: Vec<LiveIndex>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Live {
    #[serde(default)]
    pub tables: Vec<LiveTable>,
    #[serde(default)]
    pub pending: Vec<String>,
    #[serde(default)]
    pub connection: String,
    #[serde(default)]
    pub driver: String,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, PartialEq)]
pub(crate) struct Diff {
    pub table: String,
    pub status: &'static str,
    pub details: Vec<String>,
    pub open: Option<Place>,
}

impl Diff {
    pub(crate) fn json(&self, root: &str) -> Value {
        json!({"table":self.table,"status":self.status,"details":self.details,"open":place_json(root, self.open.as_ref())})
    }
}

pub(crate) fn place_json(root: &str, place: Option<&Place>) -> Value {
    place.map_or(Value::Null, |place| {
        let path = place
            .path
            .strip_prefix(root)
            .unwrap_or(&place.path)
            .trim_start_matches('/');
        json!({"path":place.path,"relative":path,"line":place.at.line + 1})
    })
}

fn lowered<'a>(items: impl Iterator<Item = &'a String>) -> BTreeSet<String> {
    items.map(|name| name.to_ascii_lowercase()).collect()
}

fn index_keys<'a>(sets: impl Iterator<Item = &'a Vec<String>>) -> BTreeSet<String> {
    sets.filter(|columns| !columns.is_empty())
        .map(|columns| columns.join("+").to_ascii_lowercase())
        .collect()
}

fn listed(label: &str, items: &BTreeSet<String>) -> Option<String> {
    (!items.is_empty()).then(|| format!("{label}: {}", items.iter().cloned().collect::<Vec<_>>().join(", ")))
}

fn created(schema: &Schema) -> impl Iterator<Item = &Table> {
    schema.tables.iter().filter(|table| table.created.is_some())
}

pub(crate) fn compare(schema: &Schema, live: &Live) -> Vec<Diff> {
    let mut rows = Vec::new();
    for name in &live.pending {
        rows.push(Diff {
            table: String::new(),
            status: "pending",
            details: vec![format!("{name} has not run on this database")],
            open: None,
        });
    }
    for table in created(schema) {
        let Some(found) = live.tables.iter().find(|found| found.name.eq_ignore_ascii_case(&table.name)) else {
            rows.push(Diff {
                table: table.name.clone(),
                status: "only-migrations",
                details: vec!["The migrations create this table but the database does not have it".into()],
                open: table.created.clone(),
            });
            continue;
        };
        let ours = lowered(table.columns.iter().map(|(name, _)| name));
        let theirs = lowered(found.columns.iter());
        let missing_columns: BTreeSet<String> = ours.difference(&theirs).cloned().collect();
        let extra_columns: BTreeSet<String> = theirs.difference(&ours).cloned().collect();
        let our_indexes = index_keys(table.indexes.iter().filter(|index| index.kind != "primary").map(|index| &index.columns));
        let their_indexes = index_keys(found.indexes.iter().filter(|index| !index.primary).map(|index| &index.columns));
        let missing_indexes: BTreeSet<String> = our_indexes.difference(&their_indexes).cloned().collect();
        let details: Vec<String> = [
            listed("Missing columns", &missing_columns),
            listed("Columns only in the database", &extra_columns),
            listed("Missing indexes", &missing_indexes),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !details.is_empty() {
            rows.push(Diff {
                table: table.name.clone(),
                status: "differs",
                details,
                open: table.created.clone(),
            });
        }
    }
    let ours: BTreeSet<String> = created(schema).map(|table| table.name.to_ascii_lowercase()).collect();
    for table in &live.tables {
        let lower = table.name.to_ascii_lowercase();
        if !ours.contains(&lower) && !INTERNAL.contains(&lower.as_str()) {
            rows.push(Diff {
                table: table.name.clone(),
                status: "only-database",
                details: vec!["No migration of the app creates it; a package or raw SQL may have".into()],
                open: None,
            });
        }
    }
    rows
}

pub(crate) struct Relation {
    pub model: String,
    pub method: String,
    pub kind: String,
    pub target: String,
}

fn short(class: &str) -> &str {
    class.rsplit('\\').next().unwrap_or(class)
}

/// `models` are `(class, table)` pairs; `relations` use the same class names.
pub(crate) fn diagram(root: &str, schema: &Schema, models: &[(String, String)], relations: &[Relation]) -> Value {
    let known: BTreeSet<&str> = created(schema).map(|table| table.name.as_str()).collect();
    let table_of = |class: &str| {
        let class = class.trim_start_matches('\\');
        let exact = models.iter().find(|(full, _)| full == class);
        let suffix = || {
            let mut matching = models.iter().filter(|(full, _)| short(full) == short(class));
            matching.next().filter(|_| !class.contains('\\') && matching.next().is_none())
        };
        exact.or_else(suffix).map(|(_, table)| table.as_str())
    };
    let mut tables = Vec::new();
    let mut links = Vec::new();
    let mut joined: BTreeSet<(String, String)> = BTreeSet::new();
    for table in created(schema) {
        let columns: Vec<Value> = table
            .columns
            .iter()
            .map(|(name, kind)| {
                let key = if table.indexes.iter().any(|index| index.kind == "primary" && index.columns.contains(name)) {
                    "pk"
                } else if table.foreigns.iter().any(|foreign| &foreign.column == name) {
                    "fk"
                } else if table.indexes.iter().any(|index| index.kind == "unique" && index.columns == [name.clone()]) {
                    "uk"
                } else {
                    ""
                };
                json!({"name":name,"kind":kind,"key":key})
            })
            .collect();
        let mut names: Vec<&str> = models.iter().filter(|(_, owner)| owner == &table.name).map(|(full, _)| short(full)).collect();
        names.sort_unstable();
        names.dedup();
        let mut foreigns = Vec::new();
        for foreign in &table.foreigns {
            let target = foreign.on.as_deref().or_else(|| foreign.model.as_deref().and_then(table_of));
            let target = target.filter(|target| known.contains(target));
            foreigns.push(json!({"column":foreign.column,"on":target.or(foreign.on.as_deref()),"references":foreign.references}));
            let Some(target) = target else { continue };
            joined.insert((table.name.clone(), target.to_owned()));
            joined.insert((target.to_owned(), table.name.clone()));
            links.push(json!({"from":table.name,"column":foreign.column,"to":target,"references":foreign.references,"kind":"foreign key","dashed":false}));
        }
        let indexes: Vec<Value> = table.indexes.iter().map(|index| json!({"columns":index.columns,"kind":index.kind})).collect();
        tables.push(json!({"name":table.name,"columns":columns,"models":names,"indexes":indexes,"foreigns":foreigns,"open":place_json(root, table.created.as_ref())}));
    }
    for relation in relations {
        let (Some(from), Some(to)) = (table_of(&relation.model), table_of(&relation.target)) else {
            continue;
        };
        if from == to || !known.contains(from) || !known.contains(to) || !joined.insert((from.to_owned(), to.to_owned())) {
            continue;
        }
        joined.insert((to.to_owned(), from.to_owned()));
        links.push(json!({"from":from,"column":"","to":to,"references":"","kind":format!("{}::{}() {}", short(&relation.model), relation.method, relation.kind),"dashed":true}));
    }
    json!({"tables":tables,"links":links})
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOP: &str = "<?php\nreturn new class extends Migration {\n    public function up(): void\n    {\n        Schema::create('users', function (Blueprint $table) {\n            $table->id();\n            $table->string('email')->unique();\n        });\n        Schema::create('orders', function (Blueprint $table) {\n            $table->id();\n            $table->foreignId('user_id')->constrained();\n        });\n        Schema::create('tags', function (Blueprint $table) {\n            $table->id();\n            $table->unsignedBigInteger('order_id');\n        });\n    }\n};\n";

    fn schema(text: &str) -> Schema {
        let mut schema = Schema::default();
        schema.apply("/p/database/migrations/2024_01_01_000000_create_shop.php", text);
        schema
    }

    #[test]
    fn draws_tables_with_their_keys_and_both_kinds_of_links() {
        let models = [("App\\User", "users"), ("App\\Order", "orders"), ("App\\Tag", "tags")].map(|(m, t)| (m.to_owned(), t.to_owned()));
        let relations = [("App\\Order", "user", "belongsTo", "App\\User"), ("App\\Order", "tags", "hasMany", "App\\Tag")]
            .map(|(model, method, kind, target)| Relation { model: model.into(), method: method.into(), kind: kind.into(), target: target.into() });
        let d = diagram("/p", &schema(SHOP), &models, &relations);
        let keys: Vec<String> = d["tables"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|t| {
                let name = t["name"].as_str().unwrap().to_owned();
                t["columns"].as_array().unwrap().iter().map(move |c| format!("{name}.{}:{}", c["name"].as_str().unwrap(), c["key"].as_str().unwrap()))
            })
            .collect();
        assert_eq!(keys, ["users.id:pk", "users.email:uk", "orders.id:pk", "orders.user_id:fk", "tags.id:pk", "tags.order_id:"]);
        let links: Vec<String> = d["links"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| format!("{}->{} {} {}", l["from"].as_str().unwrap(), l["to"].as_str().unwrap(), l["kind"].as_str().unwrap(), l["dashed"]))
            .collect();
        assert_eq!(links, ["orders->users foreign key false", "orders->tags Order::tags() hasMany true"]);
        assert_eq!(d["tables"][1]["models"], json!(["Order"]));
        assert_eq!(d["tables"][0]["open"]["relative"], "database/migrations/2024_01_01_000000_create_shop.php");
        assert_eq!(d["tables"][0]["open"]["line"], 5);
    }

    #[test]
    fn foreign_keys_to_unknown_tables_are_not_drawn() {
        let text = "<?php\nreturn new class extends Migration { public function up(): void {\n Schema::create('posts', function ($t) { $t->id(); $t->foreignId('author_id')->constrained(); });\n} };\n";
        let d = diagram("/p", &schema(text), &[], &[]);
        assert!(d["links"].as_array().unwrap().is_empty());
        assert_eq!(d["tables"][0]["columns"][1]["key"], "fk");
    }

    #[test]
    fn model_hint_resolves_foreign_id_for_to_the_models_table() {
        let text = "<?php\nreturn new class extends Migration { public function up(): void {\n Schema::create('categories', function ($t) { $t->id(); });\n Schema::create('posts', function ($t) { $t->id(); $t->foreignIdFor(Category::class)->constrained(); });\n} };\n";
        let models = [("App\\Models\\Category".to_owned(), "categories".to_owned())];
        let d = diagram("/p", &schema(text), &models, &[]);
        assert_eq!(d["links"][0]["to"], "categories");
        assert_eq!(d["links"][0]["column"], "category_id");
    }

    #[test]
    fn compares_the_migrations_with_the_live_database() {
        let text = "<?php\nreturn new class extends Migration {\n    public function up(): void\n    {\n        Schema::create('orders', function (Blueprint $table) {\n            $table->id();\n            $table->string('code')->unique();\n            $table->string('status')->index();\n        });\n        Schema::create('invoices', function (Blueprint $table) {\n            $table->id();\n        });\n        Schema::create('tags', function (Blueprint $table) {\n            $table->id();\n        });\n    }\n};\n";
        let live: Live = serde_json::from_value(json!({
            "tables": [
                {"name": "orders", "columns": ["id", "code", "legacy"], "indexes": [{"columns": ["id"], "primary": true}, {"columns": ["code"], "primary": false}]},
                {"name": "tags", "columns": ["id"], "indexes": []},
                {"name": "migrations", "columns": []},
                {"name": "telescope_entries", "columns": []}
            ],
            "pending": ["2024_02_01_000000_add_total"]
        }))
        .unwrap();
        let rows = compare(&schema(text), &live);
        let summary: Vec<(String, &str, Vec<String>)> = rows.iter().map(|row| (row.table.clone(), row.status, row.details.clone())).collect();
        assert_eq!(
            summary,
            [
                (String::new(), "pending", vec!["2024_02_01_000000_add_total has not run on this database".to_owned()]),
                (
                    "orders".into(),
                    "differs",
                    vec!["Missing columns: status".to_owned(), "Columns only in the database: legacy".into(), "Missing indexes: status".into()]
                ),
                ("invoices".into(), "only-migrations", vec!["The migrations create this table but the database does not have it".to_owned()]),
                ("telescope_entries".into(), "only-database", vec!["No migration of the app creates it; a package or raw SQL may have".to_owned()]),
            ]
        );
        assert_eq!(rows[1].json("/p")["open"]["line"], 5);
    }
}
