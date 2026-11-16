//! Tables with columns, indexes and foreign keys replayed from literal migration sources.
//! Source-only: migration code is never executed, and dynamic definitions are skipped.
use super::{Column, Event, MORPHS, NAMED, events, forward_range, tokens};
use crate::{Lines, php};
use openidea_plugin::Position;
use std::collections::{BTreeMap, VecDeque};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Place {
    pub path: String,
    pub at: Position,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Index {
    pub columns: Vec<String>,
    /// `primary`, `unique` or `index`.
    pub kind: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Foreign {
    pub column: String,
    pub on: Option<String>,
    /// Model class written in `foreignIdFor(Model::class)`; resolved to a table by the caller.
    pub model: Option<String>,
    pub references: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Table {
    pub name: String,
    pub columns: Vec<(String, String)>,
    pub indexes: Vec<Index>,
    pub foreigns: Vec<Foreign>,
    pub created: Option<Place>,
}

#[derive(Default)]
pub(crate) struct Schema {
    pub tables: Vec<Table>,
}

enum Key {
    Create { table: String, offset: usize },
    Index { table: String, index: Index },
    Foreign { table: String, foreign: Foreign },
    DropIndex { table: String, target: Target },
    DropForeign { table: String, target: Target },
}

enum Target {
    Columns(Vec<String>),
    Name(String),
}

struct Call {
    method: String,
    arguments: Vec<Range<usize>>,
}

fn literal(text: &str, range: &Range<usize>) -> Option<String> {
    php::literal(text, range.clone()).map(|(value, _)| value.to_owned())
}

fn columns(text: &str, range: Option<&Range<usize>>) -> Vec<String> {
    let Some(range) = range else {
        return Vec::new();
    };
    match php::array_open(text, range.clone()) {
        Some(open) => php::strings(text, open, false),
        None => literal(text, range).into_iter().collect(),
    }
}

fn plural(word: &str) -> String {
    let consonant_y = word.ends_with('y')
        && word.len() > 1
        && !matches!(word.as_bytes()[word.len() - 2], b'a' | b'e' | b'i' | b'o' | b'u');
    if consonant_y {
        return format!("{}ies", &word[..word.len() - 1]);
    }
    if ["s", "x", "z", "ch", "sh"].iter().any(|end| word.ends_with(end)) {
        return format!("{word}es");
    }
    format!("{word}s")
}

fn conventional_name(table: &str, columns: &[String], suffix: &str) -> String {
    format!("{table}_{}_{suffix}", columns.join("_"))
        .to_ascii_lowercase()
        .replace(['-', '.'], "_")
}

fn chain(text: &str, first_open: usize, method: &str, end: usize) -> Vec<Call> {
    let mut calls = vec![Call {
        method: method.into(),
        arguments: php::arguments(text, first_open),
    }];
    let Some(mut at) = php::closing(text, first_open).map(|close| close + 1) else {
        return calls;
    };
    loop {
        let Some(rest) = text[at..end].trim_start().strip_prefix("->") else {
            return calls;
        };
        let rest = rest.trim_start();
        let length = rest.bytes().take_while(|byte| php::is_identifier(*byte)).count();
        let after = rest[length..].trim_start();
        if length == 0 || !after.starts_with('(') {
            return calls;
        }
        let open = end - after.len();
        let Some(close) = php::closing(text, open).filter(|close| *close < end) else {
            return calls;
        };
        calls.push(Call {
            method: rest[..length].into(),
            arguments: php::arguments(text, open),
        });
        at = close + 1;
    }
}

fn foreign_from(text: &str, table_chain: &[Call], column: String, model: Option<String>) -> Option<Foreign> {
    let mut on = None;
    let mut references = None;
    let mut constrained = false;
    for call in table_chain {
        match call.method.as_str() {
            "constrained" => {
                constrained = true;
                on = call.arguments.first().and_then(|range| literal(text, range));
                references = call.arguments.get(1).and_then(|range| literal(text, range));
            }
            "references" => references = call.arguments.first().and_then(|range| literal(text, range)),
            "on" => on = call.arguments.first().and_then(|range| literal(text, range)),
            _ => {}
        }
    }
    if !constrained && on.is_none() {
        return None;
    }
    if on.is_none() && model.is_none() {
        on = column.strip_suffix("_id").map(plural);
    }
    Some(Foreign {
        column,
        on,
        model,
        references: references.unwrap_or_else(|| "id".into()),
    })
}

fn modifier_indexes(calls: &[Call], table: &str, column: &str, out: &mut Vec<Key>) {
    for call in &calls[1..] {
        let kind = match call.method.as_str() {
            "primary" => "primary",
            "unique" => "unique",
            "index" | "fullText" | "spatialIndex" => "index",
            _ => continue,
        };
        out.push(Key::Index {
            table: table.into(),
            index: Index {
                columns: vec![column.into()],
                kind,
            },
        });
    }
}

fn statement(text: &str, table: &str, calls: &[Call], out: &mut Vec<Key>) {
    let first = &calls[0];
    let argument = first.arguments.first();
    let name = argument.and_then(|range| literal(text, range));
    let mut index = |columns: Vec<String>, kind: &'static str| {
        if !columns.is_empty() {
            out.push(Key::Index {
                table: table.into(),
                index: Index { columns, kind },
            });
        }
    };
    match first.method.as_str() {
        "id" => index(vec![name.unwrap_or_else(|| "id".into())], "primary"),
        "increments" | "bigIncrements" | "tinyIncrements" | "smallIncrements" | "mediumIncrements" => {
            index(name.into_iter().collect(), "primary")
        }
        "primary" => index(columns(text, argument), "primary"),
        "unique" => index(columns(text, argument), "unique"),
        "index" | "fullText" | "spatialIndex" | "rawIndex" => index(columns(text, argument), "index"),
        method if MORPHS.contains(&method) => {
            if let Some(name) = name {
                index(vec![format!("{name}_type"), format!("{name}_id")], "index");
            }
        }
        "foreign" => {
            let Some(column) = columns(text, argument).into_iter().next() else {
                return;
            };
            if let Some(foreign) = foreign_from(text, calls, column, None) {
                out.push(Key::Foreign {
                    table: table.into(),
                    foreign,
                });
            }
        }
        "foreignId" | "foreignUuid" | "foreignUlid" => {
            let Some(column) = name else { return };
            modifier_indexes(calls, table, &column, out);
            if let Some(foreign) = foreign_from(text, calls, column, None) {
                out.push(Key::Foreign {
                    table: table.into(),
                    foreign,
                });
            }
        }
        "foreignIdFor" => {
            let class = argument.and_then(|range| php::class_constant(text, range.clone()));
            let short = class.and_then(|class| class.rsplit('\\').next()).map(str::to_owned);
            let column = first
                .arguments
                .get(1)
                .and_then(|range| literal(text, range))
                .or_else(|| short.as_ref().map(|short| format!("{}_id", crate::index::separated(short, '_'))));
            let Some(column) = column else { return };
            modifier_indexes(calls, table, &column, out);
            if let Some(foreign) = foreign_from(text, calls, column, class.map(str::to_owned)) {
                out.push(Key::Foreign {
                    table: table.into(),
                    foreign,
                });
            }
        }
        "dropForeign" | "dropIndex" | "dropUnique" | "dropPrimary" | "dropFullText" | "dropSpatialIndex" => {
            let target = match argument.and_then(|range| php::array_open(text, range.clone())) {
                Some(open) => Some(Target::Columns(php::strings(text, open, false))),
                None => name.map(Target::Name),
            };
            let Some(target) = target else { return };
            let table = table.to_owned();
            out.push(if first.method == "dropForeign" {
                Key::DropForeign { table, target }
            } else {
                Key::DropIndex { table, target }
            });
        }
        "dropConstrainedForeignId" => {
            if let Some(column) = name {
                out.push(Key::DropForeign {
                    table: table.into(),
                    target: Target::Columns(vec![column]),
                });
            }
        }
        method if NAMED.contains(&method) => {
            if let Some(column) = name {
                modifier_indexes(calls, table, &column, out);
            }
        }
        _ => {}
    }
}

fn scan(text: &str) -> Vec<Key> {
    let mut out = Vec::new();
    let range = forward_range(text);
    for (offset, token) in tokens(&text[range.clone()]) {
        if token != "Schema" {
            continue;
        }
        let at = range.start + offset;
        if at > 0 && matches!(text.as_bytes()[at - 1], b'\\' | b'$') {
            continue;
        }
        let Some(rest) = text[at + 6..].trim_start().strip_prefix("::") else {
            continue;
        };
        let length = rest.bytes().take_while(|byte| php::is_identifier(*byte)).count();
        let method = &rest[..length];
        let after = rest[length..].trim_start();
        if !matches!(method, "create" | "table") || !after.starts_with('(') {
            continue;
        }
        let open = text.len() - after.len();
        let Some(close) = php::closing(text, open) else {
            continue;
        };
        let arguments = php::arguments(text, open);
        let Some(table) = arguments.first().and_then(|range| literal(text, range)) else {
            continue;
        };
        let Some(callback) = arguments.get(1) else {
            continue;
        };
        let Some(brace) = text[callback.clone()].find('{').map(|n| callback.start + n) else {
            continue;
        };
        let Some(end) = php::closing(text, brace).filter(|end| *end <= close) else {
            continue;
        };
        let Some(receiver) = tokens(&text[callback.start..brace])
            .into_iter()
            .find(|(_, token)| token.starts_with('$'))
            .map(|(_, token)| token)
        else {
            continue;
        };
        if method == "create" {
            out.push(Key::Create {
                table: table.clone(),
                offset: at,
            });
        }
        let body = brace + 1..end;
        for (relative, token) in tokens(&text[body.clone()]) {
            if token != receiver {
                continue;
            }
            let from = body.start + relative + token.len();
            let Some(rest) = text[from..body.end].trim_start().strip_prefix("->") else {
                continue;
            };
            let rest = rest.trim_start();
            let length = rest.bytes().take_while(|byte| php::is_identifier(*byte)).count();
            let after = rest[length..].trim_start();
            if length == 0 || !after.starts_with('(') {
                continue;
            }
            let calls = chain(text, body.end - after.len(), &rest[..length], body.end);
            statement(text, &table, &calls, &mut out);
        }
    }
    out
}

impl Table {
    fn drop_column(&mut self, column: &str) {
        self.columns.retain(|(name, _)| name != column);
        self.indexes.retain(|index| !index.columns.iter().any(|name| name == column));
        self.foreigns.retain(|foreign| foreign.column != column);
    }

    fn drop_index(&mut self, target: &Target) {
        let table = self.name.clone();
        self.indexes.retain(|index| {
            let matched = match target {
                Target::Columns(columns) => &index.columns == columns,
                Target::Name(name) => {
                    let suffix = match index.kind {
                        "primary" => "primary",
                        "unique" => "unique",
                        _ => "index",
                    };
                    name.eq_ignore_ascii_case(&conventional_name(&table, &index.columns, suffix))
                }
            };
            !matched
        });
    }

    fn drop_foreign(&mut self, target: &Target) {
        let table = self.name.clone();
        self.foreigns.retain(|foreign| {
            let matched = match target {
                Target::Columns(columns) => columns.first() == Some(&foreign.column),
                Target::Name(name) => name
                    .eq_ignore_ascii_case(&conventional_name(&table, std::slice::from_ref(&foreign.column), "foreign")),
            };
            !matched
        });
    }
}

impl Schema {
    fn table_mut(&mut self, name: &str) -> Option<&mut Table> {
        self.tables.iter_mut().find(|table| table.name == name)
    }

    /// Apply one migration file; call in lexical file order.
    pub(crate) fn apply(&mut self, path: &str, text: &str) {
        let masked = php::mask_comments(text);
        let lines = Lines::new(text);
        let keys = scan(&masked);
        let mut creates: BTreeMap<String, VecDeque<usize>> = BTreeMap::new();
        for key in &keys {
            if let Key::Create { table, offset } = key {
                creates.entry(table.clone()).or_default().push_back(*offset);
            }
        }
        for event in events(text) {
            match event {
                Event::Create(name) => {
                    let created = creates.get_mut(&name).and_then(VecDeque::pop_front).map(|offset| Place {
                        path: path.into(),
                        at: lines.position(offset),
                    });
                    let table = Table {
                        name: name.clone(),
                        created,
                        ..Table::default()
                    };
                    match self.table_mut(&name) {
                        Some(existing) => *existing = table,
                        None => self.tables.push(table),
                    }
                }
                Event::DropTable(name) => self.tables.retain(|table| table.name != name),
                Event::RenameTable(from, to) => {
                    if let Some(table) = self.table_mut(&from) {
                        table.name = to;
                    }
                }
                Event::DropColumn(name, column) => {
                    if let Some(table) = self.table_mut(&name) {
                        table.drop_column(&column);
                    }
                }
                Event::RenameColumn { table, from, to, .. } => {
                    if let Some(table) = self.table_mut(&table) {
                        for (name, _) in table.columns.iter_mut().filter(|(name, _)| *name == from) {
                            name.clone_from(&to);
                        }
                        for index in &mut table.indexes {
                            index.columns.iter_mut().filter(|name| **name == from).for_each(|name| name.clone_from(&to));
                        }
                        for foreign in table.foreigns.iter_mut().filter(|foreign| foreign.column == from) {
                            foreign.column.clone_from(&to);
                        }
                    }
                }
                Event::Add(Column { table, name, kind, .. }) | Event::Change(Column { table, name, kind, .. }) => {
                    if let Some(table) = self.table_mut(&table) {
                        match table.columns.iter_mut().find(|(existing, _)| *existing == name) {
                            Some(existing) => existing.1 = kind,
                            None => table.columns.push((name, kind)),
                        }
                    }
                }
            }
        }
        for key in keys {
            match key {
                Key::Create { .. } => {}
                Key::Index { table, index } => {
                    if let Some(table) = self.table_mut(&table) {
                        if index.kind == "primary" {
                            table.indexes.retain(|existing| existing.kind != "primary");
                        }
                        if !table.indexes.contains(&index) {
                            table.indexes.push(index);
                        }
                    }
                }
                Key::Foreign { table, foreign } => {
                    if let Some(table) = self.table_mut(&table) {
                        table.foreigns.retain(|existing| existing.column != foreign.column);
                        table.foreigns.push(foreign);
                    }
                }
                Key::DropIndex { table, target } => {
                    if let Some(table) = self.table_mut(&table) {
                        table.drop_index(&target);
                    }
                }
                Key::DropForeign { table, target } => {
                    if let Some(table) = self.table_mut(&table) {
                        table.drop_foreign(&target);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migration(body: &str) -> String {
        format!("<?php\nreturn new class extends Migration {{\n    public function up(): void\n    {{\n{body}\n    }}\n}};\n")
    }

    #[test]
    fn replays_keys_across_create_alter_and_drop() {
        let mut schema = Schema::default();
        schema.apply(
            "/p/database/migrations/1.php",
            &migration(
                "Schema::create('orders', function (Blueprint $table) {\n$table->id();\n$table->string('code')->unique();\n$table->string('status')->index();\n$table->foreignId('user_id')->constrained();\n$table->foreignId('owner_id')->constrained('people', 'uid');\n$table->unsignedBigInteger('tag_id');\n$table->foreign('tag_id')->references('id')->on('tags');\n$table->index(['status', 'code']);\n$table->morphs('taggable');\n});",
            ),
        );
        let orders = &schema.tables[0];
        assert_eq!(orders.created.as_ref().unwrap().at.line, 4);
        assert_eq!(
            orders.indexes.iter().map(|index| (index.kind, index.columns.join("+"))).collect::<Vec<_>>(),
            [
                ("primary", "id".to_owned()),
                ("unique", "code".into()),
                ("index", "status".into()),
                ("index", "status+code".into()),
                ("index", "taggable_type+taggable_id".into()),
            ]
        );
        assert_eq!(
            orders.foreigns.iter().map(|f| (f.column.as_str(), f.on.as_deref(), f.references.as_str())).collect::<Vec<_>>(),
            [("user_id", Some("users"), "id"), ("owner_id", Some("people"), "uid"), ("tag_id", Some("tags"), "id")]
        );

        schema.apply(
            "/p/database/migrations/2.php",
            &migration(
                "Schema::table('orders', function (Blueprint $table) {\n$table->dropIndex(['status']);\n$table->dropForeign('orders_tag_id_foreign');\n$table->dropColumn('code');\n$table->renameColumn('user_id', 'customer_id');\n});",
            ),
        );
        let orders = &schema.tables[0];
        assert!(orders.indexes.iter().all(|index| index.columns != ["status"] && !index.columns.contains(&"code".to_owned())));
        assert_eq!(orders.foreigns.iter().map(|f| f.column.as_str()).collect::<Vec<_>>(), ["customer_id", "owner_id"]);
        assert!(orders.foreigns.iter().all(|f| f.column != "tag_id"));
        assert!(orders.columns.iter().any(|(name, _)| name == "customer_id"));
    }

    #[test]
    fn foreign_id_for_keeps_the_model_for_later_resolution() {
        let mut schema = Schema::default();
        schema.apply(
            "/p/database/migrations/1.php",
            &migration("Schema::create('posts', function (Blueprint $table) {\n$table->id();\n$table->foreignIdFor(Category::class)->constrained();\n});"),
        );
        let foreign = &schema.tables[0].foreigns[0];
        assert_eq!((foreign.column.as_str(), foreign.model.as_deref(), foreign.on.as_deref()), ("category_id", Some("Category"), None));
    }

    #[test]
    fn dropped_tables_and_down_methods_do_not_leave_facts() {
        let mut schema = Schema::default();
        schema.apply(
            "/p/database/migrations/1.php",
            "<?php\nreturn new class extends Migration {\n public function up(): void { Schema::create('a', function ($t) { $t->id(); $t->string('x')->unique(); }); }\n public function down(): void { Schema::table('a', function ($t) { $t->dropUnique(['x']); }); }\n};\n",
        );
        assert_eq!(schema.tables[0].indexes.len(), 2);
        schema.apply("/p/database/migrations/2.php", &migration("Schema::dropIfExists('a');"));
        assert!(schema.tables.is_empty());
    }
}
