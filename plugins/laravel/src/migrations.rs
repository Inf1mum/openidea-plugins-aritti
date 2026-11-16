//! Ordered, literal migration schema facts. Source offsets retain their owner.
use crate::Lines;
use crate::index::{AddName, Names, separated};
use crate::php;
use std::collections::BTreeMap;
#[path = "schema_sources.rs"]
mod schema_sources;
#[path = "schema_keys.rs"]
pub(crate) mod keys;

pub fn apply_sql(columns: &mut BTreeMap<String, Names>, text: &str, path: &str) {
    for statement in schema_sources::statements(text) {
        if statement.if_absent.as_ref().is_some_and(|table|columns.contains_key(table)){continue;}
        apply_events(columns,text,path,statement.events);
    }
}

/// Source-only diagnostics for unresolved/partial SQL DDL; offsets are UTF-8 bytes.
pub(crate) fn sql_issues(text:&str)->Vec<(usize,String)>{schema_sources::issues(text)}
/// Presentation adapter; the SQL parser itself remains host-free.
pub(crate) fn sql_issue_label(message:&str)->String{let key=match message{"SQL block comment is unterminated; schema inventory is incomplete."=>"process.sql.issue0","SQL dollar-quoted body is unterminated; schema inventory is incomplete."=>"process.sql.issue1","SQL quoted literal/identifier is unterminated; schema inventory is incomplete."=>"process.sql.issue2","SQL closing parenthesis is unmatched; schema inventory is incomplete."=>"process.sql.issue3","SQL table/constraint definition is unterminated; schema facts were withheld."=>"process.sql.issue4","SQL source exceeds the 2 MiB schema parsing bound; inventory is incomplete."=>"process.sql.issue5","SQL index declaration is incomplete or expression-based; index column facts were withheld."=>"process.sql.issue6","SQL statement cannot establish complete literal table/column/constraint evolution; dynamic, procedural or malformed DDL was withheld."=>"process.sql.issue7","SQL constraint declaration is incomplete or unsupported; constraint facts were withheld."=>"process.sql.issue8",_=>return message.to_owned(),};crate::ui_text::t(key,message)}
/// Source-only literal constraint inventory after ordered SQL evolution.
pub(crate) fn sql_constraints(text:&str)->Vec<serde_json::Value>{schema_sources::constraints(text)}

#[derive(Clone, Debug)]
pub struct Column {
    pub table: String,
    pub name: String,
    pub kind: String,
    pub offset: usize,
}

#[derive(Clone, Debug)]
pub enum Event {
    Create(String),
    Add(Column),
    /// A literal column definition followed by a proven fluent ->change().
    Change(Column),
    DropTable(String),
    RenameTable(String, String),
    DropColumn(String, String),
    RenameColumn {
        table: String,
        from: String,
        to: String,
        offset: usize,
    },
}

const NAMED: &[&str] = &[
    "string",
    "char",
    "text",
    "tinyText",
    "mediumText",
    "longText",
    "integer",
    "tinyInteger",
    "smallInteger",
    "mediumInteger",
    "bigInteger",
    "unsignedInteger",
    "unsignedTinyInteger",
    "unsignedSmallInteger",
    "unsignedMediumInteger",
    "unsignedBigInteger",
    "increments",
    "bigIncrements",
    "tinyIncrements",
    "smallIncrements",
    "mediumIncrements",
    "foreignId",
    "foreignUuid",
    "foreignUlid",
    "float",
    "double",
    "decimal",
    "unsignedDecimal",
    "boolean",
    "enum",
    "set",
    "json",
    "jsonb",
    "date",
    "dateTime",
    "dateTimeTz",
    "time",
    "timeTz",
    "timestamp",
    "timestampTz",
    "year",
    "binary",
    "uuid",
    "ulid",
    "ipAddress",
    "macAddress",
    "geometry",
    "point",
    "vector",
];
const MORPHS: &[&str] = &[
    "morphs",
    "nullableMorphs",
    "uuidMorphs",
    "nullableUuidMorphs",
    "ulidMorphs",
    "nullableUlidMorphs",
];

/// A lexical token walk: comments and strings can never manufacture schema calls.
fn tokens(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if matches!(bytes[at], b'\'' | b'"') {
            at = php::string_end(bytes, at);
            continue;
        }
        if bytes[at..].starts_with(b"//")
            || bytes[at..].starts_with(b"/*")
            || bytes[at] == b'#' && bytes.get(at + 1) != Some(&b'[')
        {
            // mask_comments has retained byte positions, including newlines.
            let masked = php::mask_comments(&text[at..]);
            let skipped = masked
                .bytes()
                .zip(text[at..].bytes())
                .take_while(|(a, b)| a != b || a.is_ascii_whitespace())
                .count();
            at += skipped.max(1);
            continue;
        }
        if bytes[at].is_ascii_alphabetic() || matches!(bytes[at], b'_' | b'$') {
            let start = at;
            at += 1;
            while at < bytes.len() && php::is_identifier(bytes[at]) {
                at += 1;
            }
            out.push((start, &text[start..at]));
        } else {
            at += 1;
        }
    }
    out
}

/// Read only a declared up() body; tiny migration fixtures without a method
/// remain accepted. A down-only class yields no forward schema facts.
fn forward_range(text: &str) -> std::ops::Range<usize> {
    let ts = tokens(text);
    for pair in ts.windows(2) {
        if pair[0].1 == "function" && pair[1].1 == "up" {
            let start = pair[1].0 + 2;
            if let Some(brace) = text[start..].find('{').map(|n| start + n) {
                if let Some(end) = php::closing(text, brace) {
                    return brace + 1..end;
                }
            }
            return 0..0;
        }
    }
    if ts
        .windows(2)
        .any(|pair| pair[0].1 == "function" && pair[1].1 == "down")
    {
        0..0
    } else {
        0..text.len()
    }
}

/// Ordered literal schema operations, shared by indexing and model associations.
/// This parser is source-only and never executes migration code.
pub(crate) fn events(text: &str) -> Vec<Event> {
    events_with_custom(text, &[])
}

fn events_with_custom(text: &str, custom: &[serde_json::Value]) -> Vec<Event> {
    // Match the source inference boundary: heredocs and shell strings are opaque.
    if text.len() > 128 * 1024 || text.contains("<<<") || text.contains('`') {
        return Vec::new();
    }
    let masked = php::mask_comments(text);
    let text = masked.as_str();
    let range = forward_range(text);
    let mut out = Vec::new();
    for (offset, token) in tokens(&text[range.clone()]) {
        if token != "Schema" {
            continue;
        }
        let at = range.start + offset;
        // Qualified Schema names are not inferred as the Laravel facade.
        if at > 0 && matches!(text.as_bytes()[at - 1], b'\\' | b'$') {
            continue;
        }
        let rest = &text[at + 6..];
        let Some(rest) = rest.trim_start().strip_prefix("::") else {
            continue;
        };
        let method_len = rest.bytes().take_while(|b| php::is_identifier(*b)).count();
        let method = &rest[..method_len];
        let after = rest[method_len..].trim_start();
        if !after.starts_with('(') {
            continue;
        }
        let open = text.len() - after.len();
        let Some(close) = php::closing(text, open) else {
            continue;
        };
        let args = php::arguments(text, open);
        let first = args.first().and_then(|r| php::literal(text, r.clone()));
        let Some((table, _)) = first else { continue };
        match method {
            "drop" | "dropIfExists" => out.push(Event::DropTable(table.into())),
            "rename" => {
                if let Some((to, _)) = args.get(1).and_then(|r| php::literal(text, r.clone())) {
                    out.push(Event::RenameTable(table.into(), to.into()));
                }
            }
            "create" | "table" => {
                let Some(callback) = args.get(1) else {
                    continue;
                };
                let Some(brace) = text[callback.clone()].find('{').map(|n| callback.start + n)
                else {
                    continue;
                };
                let Some(end) = php::closing(text, brace).filter(|end| *end <= close) else {
                    continue;
                };
                let receiver = tokens(&text[callback.start..brace])
                    .into_iter()
                    .find(|(_, t)| t.starts_with('$'))
                    .map(|(_, t)| t);
                let Some(receiver) = receiver else { continue };
                if method == "create" {
                    out.push(Event::Create(table.into()));
                }
                body_events(text, table, receiver, brace + 1..end, &mut out, custom);
            }
            _ => {}
        }
    }
    out
}

fn body_events(
    text: &str,
    table: &str,
    receiver: &str,
    body: std::ops::Range<usize>,
    out: &mut Vec<Event>,
    custom: &[serde_json::Value],
) {
    for (relative, token) in tokens(&text[body.clone()]) {
        if token != receiver {
            continue;
        }
        let at = body.start + relative + token.len();
        let Some(rest) = text[at..body.end].trim_start().strip_prefix("->") else {
            continue;
        };
        let len = rest.bytes().take_while(|b| php::is_identifier(*b)).count();
        let method = &rest[..len];
        let after = rest[len..].trim_start();
        if !after.starts_with('(') {
            continue;
        }
        let open = body.end - after.len();
        let arguments = php::arguments(text, open);
        let first = arguments
            .first()
            .and_then(|r| php::literal(text, r.clone()));
        let name_start = body.end - rest.len();
        let is_change = fluent_change(text, open, body.end);
        let mut add = |name: String, offset: usize| {
            let column = Column {
                table: table.into(),
                name,
                kind: method.into(),
                offset,
            };
            out.push(if is_change {
                Event::Change(column)
            } else {
                Event::Add(column)
            });
        };
        match method {
            "id" => add(
                first.map_or("id".into(), |(n, _)| n.into()),
                first.map_or(name_start, |(_, at)| at),
            ),
            "softDeletes" | "softDeletesTz" => add(
                first.map_or("deleted_at".into(), |(n, _)| n.into()),
                first.map_or(name_start, |(_, at)| at),
            ),
            "timestamps" | "timestampsTz" | "nullableTimestamps" => {
                add("created_at".into(), name_start);
                add("updated_at".into(), name_start);
            }
            "rememberToken" => add("remember_token".into(), name_start),
            "foreignIdFor" => {
                if let Some((name, at)) =
                    arguments.get(1).and_then(|r| php::literal(text, r.clone()))
                {
                    add(name.into(), at);
                } else if let Some(class) = arguments
                    .first()
                    .and_then(|r| php::class_constant(text, r.clone()))
                {
                    add(
                        format!(
                            "{}_id",
                            separated(class.rsplit('\\').next().unwrap_or(class), '_')
                        ),
                        name_start,
                    );
                }
            }
            _ if MORPHS.contains(&method) => {
                if let Some((name, at)) = first {
                    add(format!("{name}_id"), at);
                    add(format!("{name}_type"), at);
                }
            }
            _ if NAMED.contains(&method) => {
                if let Some((name, at)) = first {
                    add(name.into(), at);
                }
            }
            "renameColumn" => {
                if let (Some((from, _)), Some((to, offset))) = (
                    first,
                    arguments.get(1).and_then(|r| php::literal(text, r.clone())),
                ) {
                    out.push(Event::RenameColumn {
                        table: table.into(),
                        from: from.into(),
                        to: to.into(),
                        offset,
                    });
                }
            }
            "dropConstrainedForeignId" => {
                if let Some((name, _)) = first {
                    out.push(Event::DropColumn(table.into(), name.into()));
                }
            }
            "dropColumn" => {
                if let Some((name, _)) = first {
                    out.push(Event::DropColumn(table.into(), name.into()));
                } else if let Some(arg) = arguments.first() {
                    for item in php::items(text, arg.start) {
                        if let Some((name, _)) = php::literal(text, item.value) {
                            out.push(Event::DropColumn(table.into(), name.into()));
                        }
                    }
                }
            }
            "dropTimestamps" => {
                for name in ["created_at", "updated_at"] {
                    out.push(Event::DropColumn(table.into(), name.into()));
                }
            }
            "dropSoftDeletes" | "dropSoftDeletesTz" => out.push(Event::DropColumn(
                table.into(),
                first.map_or("deleted_at".into(), |(name, _)| name.into()),
            )),
            "dropRememberToken" => {
                out.push(Event::DropColumn(table.into(), "remember_token".into()))
            }
            "dropMorphs" => {
                if let Some((name, _)) = first {
                    for suffix in ["id", "type"] {
                        out.push(Event::DropColumn(table.into(), format!("{name}_{suffix}")));
                    }
                }
            }
            _ => {
                for (name, kind, offset) in
                    custom_columns(text, method, &arguments, name_start, custom)
                {
                    let column = Column {
                        table: table.into(),
                        name,
                        kind,
                        offset,
                    };
                    out.push(if is_change {
                        Event::Change(column)
                    } else {
                        Event::Add(column)
                    });
                }
            }
        }
    }
}

/// Follow only the original receiver's fluent chain. Parenthesis matching
/// skips quoted arguments/comments, so string contents and later statements
/// cannot manufacture a change modifier. Incomplete chains establish no edit.
fn fluent_change(text: &str, open: usize, end: usize) -> bool {
    let Some(close) = php::closing(text, open).filter(|close| *close < end) else {
        return false;
    };
    let mut at = close + 1;
    let mut changed = false;
    loop {
        let rest = text[at..end].trim_start();
        if rest.starts_with(';') {
            return changed;
        }
        let Some(rest) = rest.strip_prefix("->").map(str::trim_start) else {
            return false;
        };
        let length = rest
            .bytes()
            .take_while(|byte| php::is_identifier(*byte))
            .count();
        if length == 0 {
            return false;
        }
        let method = &rest[..length];
        let after = rest[length..].trim_start();
        if !after.starts_with('(') {
            return false;
        }
        let call = end - after.len();
        let Some(close) = php::closing(text, call).filter(|close| *close < end) else {
            return false;
        };
        if method.eq_ignore_ascii_case("change") && php::arguments(text, call).is_empty() {
            changed = true;
        }
        at = close + 1;
    }
}

fn custom_columns(
    text: &str,
    method: &str,
    args: &[std::ops::Range<usize>],
    method_offset: usize,
    definitions: &[serde_json::Value],
) -> Vec<(String, String, usize)> {
    let matches: Vec<_> = definitions
        .iter()
        .filter(|d| d["name"].as_str() == Some(method))
        .collect();
    let [definition] = matches.as_slice() else {
        return Vec::new();
    };
    let expand = |template: &str| -> Option<String> {
        let mut name = template.to_owned();
        while let Some(start) = name.find("${PARAMETER_") {
            let end = start + name[start..].find('}')?;
            let position: usize = name[start + 12..end].parse().ok()?;
            let arg = args.get(position.checked_sub(1)?)?;
            let (value, _) = php::literal(text, arg.clone())?;
            name.replace_range(start..=end, value);
        }
        (!name.is_empty() && !name.contains(['$', '{', '}'])).then_some(name)
    };
    let valid_type = |value: &str| {
        matches!(
            value,
            "string" | "integer" | "float" | "boolean" | "array" | "timestamp" | "other"
        )
    };
    if let Some(columns) = definition["columns"].as_array() {
        let mut out = Vec::new();
        for column in columns {
            let Some(kind) = column["type"].as_str().filter(|kind| valid_type(kind)) else {
                return Vec::new();
            };
            let Some(name) = column["name"].as_str().and_then(expand) else {
                return Vec::new();
            };
            out.push((name, format!("{method} ({kind})"), method_offset));
        }
        out
    } else {
        let Some(kind) = definition["type"].as_str().filter(|kind| valid_type(kind)) else {
            return Vec::new();
        };
        let value = if let Some(template) = definition["columnNameTemplate"].as_str() {
            expand(template).map(|name| (name, method_offset))
        } else if let Some(index) = definition["nameParameterIndex"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .and_then(|n| n.checked_sub(1))
        {
            if let Some(arg) = args.get(index) {
                php::literal(text, arg.clone()).map(|(name, at)| (name.to_owned(), at))
            } else {
                definition["defaultName"]
                    .as_str()
                    .filter(|name| !name.is_empty())
                    .map(|name| (name.to_owned(), method_offset))
            }
        } else {
            None
        };
        value
            .map(|(name, offset)| vec![(name, format!("{method} ({kind})"), offset)])
            .unwrap_or_default()
    }
}

/// Legacy per-file column inventory, now excludes rollback methods and applies
/// same-file schema removals and renames before returning surviving columns.
#[cfg(test)]
pub fn parse(text: &str) -> Vec<Column> {
    let mut columns: Vec<Column> = Vec::new();
    for event in events(text) {
        match event {
            Event::Add(column) => {
                if !columns
                    .iter()
                    .any(|c| c.table == column.table && c.name == column.name)
                {
                    columns.push(column);
                }
            }
            Event::Change(column) => {
                if let Some(existing) = columns
                    .iter_mut()
                    .find(|c| c.table == column.table && c.name == column.name)
                {
                    *existing = column;
                }
            }
            Event::Create(table) | Event::DropTable(table) => columns.retain(|c| c.table != table),
            Event::RenameTable(from, to) => {
                for c in &mut columns {
                    if c.table == from {
                        c.table = to.clone();
                    }
                }
            }
            Event::DropColumn(table, name) => {
                columns.retain(|c| c.table != table || c.name != name)
            }
            Event::RenameColumn {
                table,
                from,
                to,
                offset,
            } => {
                for c in &mut columns {
                    if c.table == table && c.name == from {
                        c.name = to.clone();
                        c.offset = offset;
                    }
                }
            }
        }
    }
    columns
}

/// Replay lexically sorted migration files into the shared completion inventory.
/// A rename destination navigates to the new literal; untouched columns keep
/// their original defining source.
#[cfg(test)]
pub fn apply(columns: &mut BTreeMap<String, Names>, text: &str, path: &str) {
    apply_custom(columns, text, path, &[]);
}

pub fn apply_custom(
    columns: &mut BTreeMap<String, Names>,
    text: &str,
    path: &str,
    custom: &[serde_json::Value],
) {
    apply_events(columns,text,path,events_with_custom(text,custom));
}
fn apply_events(columns:&mut BTreeMap<String,Names>,text:&str,path:&str,events:Vec<Event>){
    let lines = Lines::new(text);
    for event in events {
        match event {
            Event::Create(table) => {
                columns.insert(table, Names::default());
            }
            Event::DropTable(table) => {
                columns.remove(&table);
            }
            Event::RenameTable(from, to) => {
                if let Some(names) = columns.remove(&from) {
                    columns.insert(to, names);
                }
            }
            Event::DropColumn(table, name) => {
                if let Some(names) = columns.get_mut(&table) {
                    names.remove(&name);
                }
            }
            Event::RenameColumn {
                table,
                from,
                to,
                offset,
            } => {
                if let Some(names) = columns.get_mut(&table) {
                    if let Some(mut def) = names.remove(&from) {
                        def.path = path.into();
                        def.at = lines.position(offset);
                        def.width = lines.width(offset, to.len()) as u32;
                        names.insert(to, def);
                    }
                }
            }
            Event::Change(column) => {
                if let Some(existing) = columns
                    .get_mut(&column.table)
                    .and_then(|names| names.get_mut(&column.name))
                {
                    *existing = crate::index::Entry {
                        detail: Some(column.kind),
                        path: path.into(),
                        at: lines.position(column.offset),
                        width: if text
                            .get(column.offset..)
                            .is_some_and(|rest| rest.starts_with(&column.name))
                        {
                            lines.width(column.offset, column.name.len()) as u32
                        } else {
                            0
                        },
                    };
                }
            }
            Event::Add(column) => columns.entry(column.table).or_default().add(
                &column.name,
                Some(column.kind),
                path,
                lines.position(column.offset),
                if text
                    .get(column.offset..)
                    .is_some_and(|rest| rest.starts_with(&column.name))
                {
                    lines.width(column.offset, column.name.len())
                } else {
                    0
                },
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_columns_of_created_and_altered_tables() {
        let text = "<?php\nreturn new class extends Migration {\n    public function up(): void\n    {\n        Schema::create('posts', function (Blueprint $table) {\n            $table->id();\n            $table->string('title')->nullable()->index();\n            $table->foreignId('user_id')->constrained();\n            $table->foreignIdFor(Category::class);\n            $table->morphs('taggable');\n            $table->softDeletes();\n            $table->timestamps();\n        });\n\n        Schema::table('users', function (Blueprint $table) {\n            $table->boolean('is_admin')->default(false)->after('email');\n            $table->rememberToken();\n        });\n    }\n};\n";
        let columns = parse(text);
        let found: Vec<_> = columns
            .iter()
            .map(|c| (c.table.as_str(), c.name.as_str(), c.kind.as_str()))
            .collect();
        assert_eq!(
            found,
            [
                ("posts", "id", "id"),
                ("posts", "title", "string"),
                ("posts", "user_id", "foreignId"),
                ("posts", "category_id", "foreignIdFor"),
                ("posts", "taggable_id", "morphs"),
                ("posts", "taggable_type", "morphs"),
                ("posts", "deleted_at", "softDeletes"),
                ("posts", "created_at", "timestamps"),
                ("posts", "updated_at", "timestamps"),
                ("users", "is_admin", "boolean"),
                ("users", "remember_token", "rememberToken"),
            ]
        );
        assert_eq!(&text[columns[1].offset..columns[1].offset + 5], "title");
    }

    #[test]
    fn survives_half_typed_migrations() {
        let text =
            "Schema::create('a', function (Blueprint $table) { $table->string('b')->foreignId(";
        for end in 0..=text.len() {
            parse(&text[..end]);
        }
    }
}

#[cfg(test)]
mod ordered_tests {
    use super::*;
    #[test]
    fn rollback_and_quoted_code_do_not_create_columns() {
        let source = r#"<?php return new class { function up() {
            $literal = "Schema::create('fake', function ($t) { $t->string('fake'); });";
            Schema::create('posts', function ($blueprint) { $blueprint->id(); $blueprint->string('title'); $other->string('wrong'); });
        } function down() { Schema::create('rollback', function ($t) { $t->string('wrong'); }); } };"#;
        assert_eq!(
            parse(source)
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            ["id", "title"]
        );
    }
    #[test]
    fn drop_constrained_foreign_id_removes_the_column() {
        let source = "<?php return new class { function up() { Schema::create('posts', function ($t) { $t->id(); $t->foreignId('user_id')->constrained(); }); Schema::table('posts', function ($t) { $t->dropConstrainedForeignId('user_id'); }); } };";
        assert_eq!(
            parse(source)
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            ["id"]
        );
    }
    #[test]
    fn rename_and_drop_replay_across_sources_with_destination_ownership() {
        let mut columns = BTreeMap::new();
        apply(
            &mut columns,
            "Schema::create('posts', function ($t) { $t->id(); $t->string('title'); $t->string('obsolete'); });",
            "/old.php",
        );
        let rename = "Schema::rename('posts','articles'); Schema::table('articles', function ($t) { $t->renameColumn('title','headline'); $t->dropColumn(['obsolete']); });";
        apply(&mut columns, rename, "/new.php");
        assert!(!columns.contains_key("posts"));
        assert_eq!(
            columns["articles"]
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["headline", "id"]
        );
        assert_eq!(columns["articles"]["headline"].path, "/new.php");
        assert_eq!(columns["articles"]["id"].path, "/old.php");
        apply(
            &mut columns,
            "Schema::dropIfExists('articles');",
            "/drop.php",
        );
        assert!(columns.is_empty());
    }
    #[test]
    fn same_file_events_follow_statement_order_and_explicit_foreign_key() {
        let source = "Schema::create('posts',function($t){$t->string('old');$t->renameColumn('old','new');$t->foreignIdFor(User::class,'author_id');$t->dropColumn('new');});";
        assert_eq!(
            parse(source)
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            ["author_id"]
        );
    }
}

#[cfg(test)]
mod custom_tests {
    use super::*;
    #[test]
    fn custom_blueprint_single_multi_and_dynamic_argument_refusal() {
        let definitions = vec![
            serde_json::json!({"name":"tenantId","type":"integer","nameParameterIndex":1,"defaultName":"tenant_id"}),
            serde_json::json!({"name":"customMorphs","columns":[{"name":"${PARAMETER_1}_id","type":"integer"},{"name":"${PARAMETER_1}_type","type":"string"}]}),
        ];
        let mut columns = BTreeMap::new();
        apply_custom(
            &mut columns,
            "Schema::create('users',function($t){$t->tenantId();$t->tenantId('organization_id');$t->customMorphs('owner');$t->customMorphs($dynamic);});",
            "/m.php",
            &definitions,
        );
        assert_eq!(
            columns["users"]
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["organization_id", "owner_id", "owner_type", "tenant_id"]
        );
        assert_eq!(columns["users"]["organization_id"].width, 15);
        assert_eq!(columns["users"]["owner_id"].width, 0);
    }
}

#[cfg(test)]
mod integration_tests {
    #[test]
    fn schema_sources_and_metadata_macros_feed_ordered_completion_inventory() {
        let host = openidea_plugin::mock::MockHost::install();
        host.add_file("ide.json",r#"{"database":{"columnMigrationMethods":[{"name":"tenantId","type":"integer","nameParameterIndex":1,"defaultName":"tenant_id"}]}}"#);
        host.add_file(
            "database/schema/base.sql",
            "CREATE TABLE posts (id bigint, obsolete text);",
        );
        host.add_file("database/migrations/002.php","<?php Schema::table('articles',function($t){$t->renameColumn('id','article_id');$t->dropColumn('obsolete');$t->tenantId();});");
        host.add_file(
            "database/migrations/001.php",
            "<?php Schema::rename('posts','articles');",
        );
        let index =
            crate::index::Index::build(&openidea_plugin::host::projects().unwrap().remove(0));
        assert!(!index.columns.contains_key("posts"));
        assert_eq!(
            index.columns["articles"]
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["article_id", "tenant_id"]
        );
        assert_eq!(
            index.columns["articles"]["article_id"].path,
            "/project/database/migrations/002.php"
        );
        assert!(host.process_runs().is_empty());
    }
}

#[cfg(test)]
mod change_tests {
    use super::*;
    #[test]
    fn ordered_create_rename_change_drop_preserves_exact_change_destination() {
        let mut columns = BTreeMap::new();
        apply(
            &mut columns,
            "Schema::create('posts',function($t){$t->string('title');$t->integer('obsolete');});",
            "/01.php",
        );
        apply(
            &mut columns,
            "Schema::rename('posts','articles');Schema::table('articles',function($t){$t->renameColumn('title','libellé💡');});",
            "/02.php",
        );
        let changed = "// préface 💡\nSchema::table('articles',function($t){$t->decimal('libellé💡',12,3)->nullable()->comment('not a ->change()')->change();$t->integer('libellé💡');$t->dropColumn('obsolete');$t->integer('missing')->change();});";
        apply(&mut columns, changed, "/03.php");
        assert_eq!(columns["articles"].len(), 1);
        let entry = &columns["articles"]["libellé💡"];
        assert_eq!(entry.detail.as_deref(), Some("decimal"));
        assert_eq!(entry.path, "/03.php");
        let offset = changed.find("libellé💡").unwrap();
        let lines = Lines::new(changed);
        assert_eq!(entry.at, lines.position(offset));
        assert_eq!(entry.width, "libellé💡".encode_utf16().count() as u32);
        apply(
            &mut columns,
            "Schema::table('articles',function($t){$t->dropColumn('libellé💡');$t->string('libellé💡')->change();});",
            "/04.php",
        );
        assert!(columns["articles"].is_empty());
    }
    #[test]
    fn only_same_proven_fluent_chain_can_change_a_column() {
        let mut columns = BTreeMap::new();
        apply(
            &mut columns,
            "Schema::create('posts',function($t){$t->string('title');});",
            "/old.php",
        );
        for source in [
            "Schema::table('posts',function($t){$t->integer('title')->comment('->change()');});",
            "Schema::table('posts',function($t){$t->integer('title');$other->change();});",
            "Schema::table('posts',function($t){$t->integer('title')->default('string with ->change()')->nullable();});",
            "Schema::table('posts',function($t){$t->integer('title')->change(true);});",
            "Schema::table('posts',function($t){$t->integer('title')->change(});",
        ] {
            apply(&mut columns, source, "/not-change.php");
            assert_eq!(columns["posts"]["title"].detail.as_deref(), Some("string"));
            assert_eq!(columns["posts"]["title"].path, "/old.php");
        }
        apply(
            &mut columns,
            "Schema::table('posts',function($t){$t->integer('title') /* ->junk() */ ->nullable()->change()->comment('ok');});",
            "/change.php",
        );
        assert_eq!(columns["posts"]["title"].detail.as_deref(), Some("integer"));
        assert_eq!(columns["posts"]["title"].path, "/change.php");
    }
    #[test]
    fn per_file_inventory_uses_first_add_and_explicit_change_only() {
        let source = "Schema::create('posts',function($t){$t->string('title');$t->integer('title');$t->decimal('unknown')->change();$t->text('title')->change();});";
        let columns = parse(source);
        assert_eq!(columns.len(), 1);
        assert_eq!(columns[0].name, "title");
        assert_eq!(columns[0].kind, "text");
    }
}
