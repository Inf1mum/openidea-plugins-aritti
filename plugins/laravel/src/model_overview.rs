//! Source-owned model browsing. Rendering never executes PHP or captures runtime data.
use crate::index::{Entry, Index};
use openidea_plugin::{Project, views::Body};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Every navigable row refers to an indexed owning source; runtime names alone
/// cannot become editor destinations.
pub(crate) fn source_keys(index: &Index, project: &Project) -> Vec<Value> {
    crate::eloquent_members::model_inventory(&index.eloquent_models)
        .into_iter()
        .filter(|(_, path, _)| crate::owns_current_path(project, path))
        .map(|(_, path, _)| json!(format!("model-source:{}", serde_json::to_string(&[&path]).unwrap())))
        .collect()
}

pub(crate) fn rows(index: &Index, project: &Project) -> Vec<Value> {
    let root = project.root().unwrap_or_default();
    let mut rows = Vec::new();
    let related = crate::eloquent_members::related_facts::inventory(index, project);
    for (full, path, table) in crate::eloquent_members::model_inventory(&index.eloquent_models) {
        if !crate::owns_current_path(project, &path) {
            continue;
        }
        let file = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .trim_start_matches('/');
        rows.push(json!({"key":format!("model-source:{}",serde_json::to_string(&[&path]).unwrap()),"cells":[full,"Model",table,file],"icon":"class"}));
        let declared = declared_keys(project, &path, &full);
        if !declared
            .iter()
            .any(|(property, _, _, _)| property == "primaryKey")
        {
            rows.push(json!({"key":format!("model-key-unknown:{}",json!([full,path])),"cells":[full,"Primary key","No proven literal key in this model · inherited/runtime configuration is not inferred",file],"icon":"field","activationEnabled":false}));
        }
        for (property, value, offset, width) in declared {
            let label = match property.as_str() {
                "primaryKey" => "Primary key",
                "keyType" => "Key type",
                _ => "Incrementing",
            };
            rows.push(json!({"key":format!("model-primary:{}",json!([path,property,value,offset,width])),"cells":[full,"Primary key",format!("{label}: {value} · declared ${property}"),file],"icon":"field"}));
        }

        let facts = crate::eloquent_members::helper_facts::facts(index, &full, &|path| {
            crate::owns_current_path(project, path)
        });
        let accessors = crate::eloquent_members::related_facts::accessors(index, project, &full);
        for (name, entry) in index.columns.get(&table).into_iter().flatten() {
            if !crate::owns_current_path(project, &entry.path) {
                continue;
            }
            let mut entry = entry.clone();
            if let Some(accessor) = accessors.iter().find(|accessor| accessor.name == *name) {
                entry.detail = Some(format!("{} (accessor)", accessor.value_type));
            } else if let Some(cast) = facts
                .as_ref()
                .and_then(|facts| facts.casts.iter().find(|cast| cast.name == *name))
            {
                entry.detail = Some(format!(
                    "{} (cast)",
                    cast.resolved.as_deref().unwrap_or(&cast.label)
                ));
            }
            rows.push(row(&full, "Field", name, &entry, &root));
        }
        if let Some(facts) = facts {
            for relation in facts.relations {
                let detail = if relation.many {
                    format!("Collection<{}>", relation.target)
                } else {
                    format!("{} | null", relation.target)
                };
                if let Some(row) = fact_row(
                    project,
                    &root,
                    &full,
                    "Relation",
                    &relation.name,
                    &detail,
                    &relation.entry,
                    &relation.name,
                ) {
                    rows.push(row);
                }
            }
            for scope in facts.scopes {
                let spelling = if scope.entry.width > scope.name.encode_utf16().count() as u32 {
                    let mut chars = scope.name.chars();
                    format!(
                        "scope{}{}",
                        chars.next().unwrap_or('_').to_uppercase(),
                        chars.as_str()
                    )
                } else {
                    scope.name.clone()
                };
                let detail = project
                    .read_file(&scope.entry.path)
                    .ok()
                    .and_then(|source| {
                        crate::eloquent_members::helper_facts::scope_signature(
                            &source,
                            &scope.entry,
                            &full,
                        )
                    })
                    .map(|signature| {
                        format!(
                            "({}) → {}",
                            signature.parameters.join(", "),
                            signature.return_type
                        )
                    })
                    .unwrap_or_else(|| "Source scope; signature unresolved".into());
                if let Some(row) = fact_row(
                    project,
                    &root,
                    &full,
                    "Scope",
                    &scope.name,
                    &detail,
                    &scope.entry,
                    &spelling,
                ) {
                    rows.push(row);
                }
            }
            for cast in facts.casts {
                let detail = cast.resolved.as_deref().unwrap_or(&cast.label);
                if let Some(row) = fact_row(
                    project,
                    &root,
                    &full,
                    "Cast",
                    &cast.name,
                    detail,
                    &cast.entry,
                    &cast.name,
                ) {
                    rows.push(row);
                }
            }
        }
        for accessor in accessors {
            if let Some(row) = fact_row(
                project,
                &root,
                &full,
                "Accessor",
                &accessor.name,
                &accessor.value_type,
                &accessor.entry,
                &accessor.spelling,
            ) {
                rows.push(row);
            }
        }
        for class in related.iter().filter(|related| related.model == full) {
            if let Some(row) = fact_row(
                project,
                &root,
                &full,
                &class.kind,
                &class.label,
                "declared association",
                &class.entry,
                &class.spelling,
            ) {
                rows.push(row);
            }
        }
    }
    rows
}
/// Literal class-level properties only. No inherited defaults or accessor return
/// values are guessed, and nested methods/arrays/strings cannot create facts.
fn declared_keys(project: &Project, path: &str, full: &str) -> Vec<(String, String, usize, u32)> {
    if !crate::owns_current_path(project, path) {
        return Vec::new();
    }
    let Ok(source) = project.read_file(path) else {
        return Vec::new();
    };
    if source.len() > 128 * 1024
        || crate::eloquent_members::declaration(&source).as_deref() != Some(full)
    {
        return Vec::new();
    }
    primary_properties(&source)
}
fn primary_properties(source: &str) -> Vec<(String, String, usize, u32)> {
    let Some((_, body)) = crate::eloquent_members::class_layout(source) else {
        return Vec::new();
    };
    let masked = crate::php::mask_comments(source);
    let Some(end) = crate::php::closing(&masked, body - 1) else {
        return Vec::new();
    };
    let bytes = masked.as_bytes();
    let mut at = body;
    let mut declarations: BTreeMap<String, Option<(String, usize, u32)>> = BTreeMap::new();
    while at < end {
        match bytes[at] {
            b'\'' | b'"' | b'`' => {
                at = crate::php::string_end(bytes, at);
                continue;
            }
            b'(' | b'[' | b'{' => {
                let Some(close) = crate::php::closing(&masked, at) else {
                    return Vec::new();
                };
                at = close + 1;
                continue;
            }
            b'$' => {
                let start = at;
                at += 1;
                while at < end && (crate::php::is_identifier(bytes[at]) || bytes[at] >= 128) {
                    at += 1;
                }
                let property = &masked[start + 1..at];
                if !matches!(property, "primaryKey" | "keyType" | "incrementing") {
                    continue;
                }
                let rest = masked[at..end].trim_start();
                let mut value = None;
                if let Some(rest) = rest.strip_prefix('=').map(str::trim_start) {
                    let value_start = end - rest.len();
                    let value_end = if matches!(rest.as_bytes().first(), Some(b'\'' | b'"')) {
                        crate::php::string_end(bytes, value_start)
                    } else {
                        value_start
                            + rest
                                .bytes()
                                .take_while(|b| crate::php::is_identifier(*b))
                                .count()
                    };
                    if value_end <= end && masked[value_end..end].trim_start().starts_with(';') {
                        if property == "incrementing" {
                            let literal = &masked[value_start..value_end];
                            if literal.eq_ignore_ascii_case("true")
                                || literal.eq_ignore_ascii_case("false")
                            {
                                value = Some((
                                    literal.to_ascii_lowercase(),
                                    value_start,
                                    literal.encode_utf16().count() as u32,
                                ));
                            }
                        } else if let Some((literal, offset)) =
                            crate::php::literal(&masked, value_start..value_end)
                        {
                            if !literal.is_empty()
                                && !literal.contains(['\\', '$'])
                                && !literal.chars().any(char::is_control)
                            {
                                value = Some((
                                    literal.to_owned(),
                                    offset,
                                    literal.encode_utf16().count() as u32,
                                ));
                            }
                        }
                    }
                }
                if declarations.contains_key(property) {
                    declarations.insert(property.into(), None);
                } else {
                    declarations.insert(property.into(), value);
                }
                // The remainder of a property initializer cannot introduce
                // another declaration (e.g. a chained/dynamic assignment).
                while at < end {
                    if matches!(bytes[at], b'\'' | b'"' | b'`') {
                        at = crate::php::string_end(bytes, at);
                        continue;
                    }
                    if matches!(bytes[at], b'(' | b'[' | b'{') {
                        let Some(close) = crate::php::closing(&masked, at) else {
                            return Vec::new();
                        };
                        at = close + 1;
                        continue;
                    }
                    if bytes[at] == b';' {
                        at += 1;
                        break;
                    }
                    at += 1;
                }
            }
            _ => at += 1,
        }
    }
    declarations
        .into_iter()
        .filter_map(|(property, value)| {
            value.map(|(value, offset, width)| (property, value, offset, width))
        })
        .collect()
}

fn fact_row(
    project: &Project,
    root: &str,
    model: &str,
    kind: &str,
    name: &str,
    detail: &str,
    entry: &Entry,
    spelling: &str,
) -> Option<Value> {
    if !crate::owns_current_path(project, &entry.path) {
        return None;
    }
    let source = project.read_file(&entry.path).ok()?;
    if source.len() > 128 * 1024 {
        return None;
    }
    if entry.width > 0 {
        let from = openidea_plugin::text::offset(&source, entry.at)?;
        let to = openidea_plugin::text::offset(
            &source,
            openidea_plugin::Position::new(entry.at.line, entry.at.character + entry.width),
        )?;
        if source.get(from..to)? != spelling
            || source
                .get(to..)
                .and_then(|rest| rest.chars().next())
                .is_some_and(|c| c == '_' || c.is_alphanumeric())
        {
            return None;
        }
    }
    Some(
        json!({"key":format!("model-fact:{}",json!([model,kind,name,entry.path,entry.at.line,entry.at.character,entry.width,spelling])),"cells":[model,kind,format!("{name} · {detail}"),entry.path.strip_prefix(root).unwrap_or(&entry.path).trim_start_matches('/')],"icon":if matches!(kind,"Factory"|"Resource"){"class"}else if kind=="Migration"{"file-php"}else if matches!(kind,"Field"|"Cast"|"Accessor"){"field"}else{"method"}}),
    )
}

fn row(model: &str, kind: &str, name: &str, entry: &Entry, root: &str) -> Value {
    json!({"key":format!("model-entry:{}",json!([entry.path,entry.at.line,entry.at.character,entry.width])),"cells":[model,kind,format!("{name}{}",entry.detail.as_ref().map(|s|format!(" · {s}")).unwrap_or_default()),entry.path.strip_prefix(root).unwrap_or(&entry.path).trim_start_matches('/')],"icon":if kind=="Field"{"field"}else{"method"}})
}
pub(crate) fn open(
    index: &Index,
    project: &Project,
    key: &str,
) -> Result<(), openidea_plugin::Error> {
    use openidea_plugin::{Error, Position, Range, host};
    let decoded;
    let key = if let Some(payload) = key.strip_prefix("model-member:") {
        let (model, source): (String, String) =
            serde_json::from_str(payload).map_err(|_| Error::Stale)?;
        if !rows(index, project).iter().any(|row| {
            row["cells"][0].as_str() == Some(&model) && row["key"].as_str() == Some(&source)
        }) {
            return Err(Error::Stale);
        }
        decoded = source;
        decoded.as_str()
    } else {
        key
    };
    if !rows(index, project)
        .iter()
        .any(|row| row["key"].as_str() == Some(key))
    {
        return Err(Error::Stale);
    }
    if let Some(payload) = key.strip_prefix("model-fact:") {
        let (_, _, _, path, line, character, width, _): (
            String,
            String,
            String,
            String,
            u32,
            u32,
            u32,
            String,
        ) = serde_json::from_str(payload).map_err(|_| Error::Stale)?;
        if !crate::owns_current_path(project, &path) {
            return Err(Error::Stale);
        }
        let at = Position::new(line, character);
        return host::open_editor(
            &path,
            Some(Range::new(at, Position::new(line, character + width))),
        );
    }
    if let Some(payload) = key.strip_prefix("model-primary:") {
        let (path, _, _, offset, width): (String, String, String, usize, u32) =
            serde_json::from_str(payload).map_err(|_| Error::Stale)?;
        if !crate::owns_current_path(project, &path) {
            return Err(Error::Stale);
        }
        let source = project.read_file(&path)?;
        if offset > source.len() || !source.is_char_boundary(offset) {
            return Err(Error::Stale);
        }
        let at = openidea_plugin::text::position(&source, offset);
        return host::open_editor(
            &path,
            Some(Range::new(at, Position::new(at.line, at.character + width))),
        );
    }
    let (path, at, width) = if let Some(value) = key.strip_prefix("model-source:") {
        let paths: Vec<String> = serde_json::from_str(value).map_err(|_| Error::Stale)?;
        let [path] = paths.as_slice() else {
            return Err(Error::Stale);
        };
        (path.clone(), Position::new(0, 0), 0)
    } else if let Some(value) = key.strip_prefix("model-entry:") {
        let (path, line, character, width): (String, u32, u32, u32) =
            serde_json::from_str(value).map_err(|_| Error::Stale)?;
        (path, Position::new(line, character), width)
    } else {
        return Err(Error::Stale);
    };
    if !crate::owns_current_path(project, &path) {
        return Err(Error::Stale);
    }
    let text = project.read_file(&path)?;
    // Only open cached exact spans while the destination still has the indexed
    // spelling. A source-level row opens the file without a fabricated symbol.
    if width > 0 {
        let found = index
            .columns
            .values()
            .flat_map(|names| names.iter())
            .find(|(_, e)| e.path == path && e.at == at && e.width == width)
            .map(|(name, _)| name.clone())
            .or_else(|| {
                crate::eloquent_members::member_inventory(&index.eloquent_models)
                    .into_iter()
                    .find(|(_, _, _, e)| e.path == path && e.at == at && e.width == width)
                    .map(|(_, kind, name, entry)| {
                        if kind == "Scope" && entry.width > name.encode_utf16().count() as u32 {
                            let mut chars = name.chars();
                            format!(
                                "scope{}{}",
                                chars.next().unwrap().to_uppercase(),
                                chars.as_str()
                            )
                        } else {
                            name
                        }
                    })
            });
        let Some(name) = found else {
            return Err(Error::Stale);
        };
        let line = text.lines().nth(at.line as usize).ok_or(Error::Stale)?;
        let mut utf16 = 0;
        let mut offset = None;
        for (byte, ch) in line.char_indices() {
            if utf16 == at.character {
                offset = Some(byte);
                break;
            }
            utf16 += ch.len_utf16() as u32;
        }
        if !offset.is_some_and(|offset| line[offset..].starts_with(&name)) {
            return Err(Error::Stale);
        }
    }
    host::open_editor(
        &path,
        Some(Range::new(at, Position::new(at.line, at.character + width))),
    )
}

pub(crate) fn source_path(
    index: &Index,
    project: &Project,
    key: &str,
) -> Result<String, openidea_plugin::Error> {
    use openidea_plugin::Error;
    if !rows(index, project)
        .iter()
        .any(|row| row["key"] == key && row["cells"][1] == "Model")
    {
        return Err(Error::Stale);
    }
    let payload = key.strip_prefix("model-source:").ok_or(Error::Stale)?;
    let paths: Vec<String> = serde_json::from_str(payload).map_err(|_| Error::Stale)?;
    let [path] = paths.as_slice() else {
        return Err(Error::Stale);
    };
    if !crate::owns_current_path(project, path) {
        return Err(Error::Stale);
    }
    Ok(path.clone())
}

/// Runtime metadata is display-only. It is never promoted to a source location
/// or a missing-field diagnostic, and remains distinct from migration facts.
pub(crate) fn inventory_with_schema(
    index: &Index,
    project: &Project,
    snapshot: Option<&Value>,
    expanded: &BTreeMap<String, bool>,
) -> Body {
    let mut items = rows(index, project);
    if let Some(tables) = snapshot.and_then(|s| s["tables"].as_array()) {
        for (full, _, table) in crate::eloquent_members::model_inventory(&index.eloquent_models) {
            let captured_connection = snapshot.and_then(|s| s["connection"].as_str());
            let source_connection =
                crate::eloquent_members::related_facts::connection(index, project, &full);
            if source_connection.as_deref().is_none()
                || source_connection.as_deref() != captured_connection
            {
                continue;
            }
            for runtime in tables.iter().filter(|t| {
                let name = t["name"].as_str().unwrap_or("");
                let schema = t["schema"].as_str().unwrap_or("");
                if table.contains('.') {
                    table == format!("{schema}.{name}")
                } else {
                    table == name
                }
            }) {
                let provenance = runtime["schema"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(|schema| {
                        crate::ui_text::format(
                            "final.models.schemaNamed",
                            "Captured database schema · {schema}",
                            &[("schema", schema)],
                        )
                    })
                    .unwrap_or_else(|| {
                        crate::ui_text::text("final.models.schema", "Captured database schema")
                            .into()
                    });
                let provenance = match snapshot.and_then(|snapshot| snapshot["connection"].as_str())
                {
                    Some(connection) => crate::ui_text::format(
                        "final.models.connection",
                        "{provenance} · connection {connection}",
                        &[("provenance", &provenance), ("connection", connection)],
                    ),
                    None => provenance,
                };
                for primary in runtime["indexes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|i| {
                        i["primary"].as_bool() == Some(true) && i["unique"].as_bool() == Some(true)
                    })
                {
                    let Some(columns) = primary["columns"].as_array().filter(|c| !c.is_empty())
                    else {
                        continue;
                    };
                    let names = columns.iter().filter_map(Value::as_str).collect::<Vec<_>>();
                    if names.len() != columns.len()
                        || names.iter().any(|name| {
                            !runtime["columns"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .any(|c| c["name"].as_str() == Some(name))
                        })
                    {
                        continue;
                    }
                    items.push(json!({"key":format!("model-runtime-primary:{}",json!([full,table,runtime["schema"],primary["name"],names])),"cells":[full,"Database primary key",crate::ui_text::format("final.models.capturedPrimary","{names} · captured primary index",&[("names",&names.join(", "))]),provenance],"icon":"field","activationEnabled":false}));
                }
                for column in runtime["columns"].as_array().into_iter().flatten() {
                    let Some(name) = column["name"].as_str() else {
                        continue;
                    };
                    let kind = column["type"].as_str().unwrap_or_else(|| {
                        crate::ui_text::text("final.models.unknownType", "unknown")
                    });
                    let nullable = if column["nullable"].as_bool() == Some(true) {
                        crate::ui_text::text("final.models.nullable", " · nullable")
                    } else {
                        ""
                    };
                    items.push(json!({"key":format!("model-runtime:{}",json!([full,table,runtime["schema"],name])),"cells":[full,"Database field",format!("{name} · {kind}{nullable}"),crate::ui_text::format("final.models.schemaSuffix","Captured database schema{suffix}",&[("suffix",&runtime["schema"].as_str().map(|schema|format!(" · {schema}")).unwrap_or_default())])],"icon":"field","activationEnabled":false}));
                }
            }
        }
    }
    let mut rendered = tree(&items, expanded);
    present_tree(&mut rendered, &items);
    Body::Other { kind: "tree".into(), data: json!({"backgroundPoll":crate::model_helper_parameters::load(project).ok().filter(|preferences|preferences.on_save || !preferences.post_commands.is_empty()).map(|_|json!({"intervalMs":1000,"message":{"op":"helper-automation-tick"}})),"searchPlaceholder":crate::ui_text::text("native.model_overview.b8bd4fdaeb", "Filter models, fields, related classes and scopes…"),"items":rendered}).to_string() }
}

fn tree(rows: &[Value], expanded: &BTreeMap<String, bool>) -> Vec<Value> {
    let mut items = Vec::new();
    for model in rows.iter().filter(|row| row["cells"][1] == "Model") {
        let full = model["cells"][0].as_str().unwrap_or_default();
        let key = model["key"].as_str().unwrap_or_default();
        let members: Vec<_> = rows
            .iter()
            .filter(|row| row["cells"][0] == full && row["cells"][1] != "Model")
            .collect();
        items.push(json!({"key":key,"label":full,"detail":format!("{} · {}",model["cells"][2].as_str().unwrap_or_default(),model["cells"][3].as_str().unwrap_or_default()),"icon":"class","expandable":!members.is_empty(),"expanded":expanded.get(key).copied().unwrap_or(false)}));
        for (kind, label) in [
            ("Primary key", "Key configuration"),
            ("Database primary key", "Database primary keys"),
            ("Field", "Fields"),
            ("Cast", "Casts"),
            ("Accessor", "Accessors"),
            ("Factory", "Factories"),
            ("Resource", "Resources"),
            ("Migration", "Migrations"),
            ("Relation", "Relations"),
            ("Scope", "Scopes"),
            ("Database field", "Database fields"),
        ] {
            let children: Vec<_> = members
                .iter()
                .filter(|row| row["cells"][1] == kind)
                .collect();
            if children.is_empty() {
                continue;
            }
            let group = format!("model-group:{}", json!([full, kind]));
            items.push(json!({"key":group,"parent":key,"searchText":full,"label":format!("{label} ({})",children.len()),"icon":"folder","expandable":true,"expanded":expanded.get(&group).copied().unwrap_or(true)}));
            for member in children {
                let definition = member["cells"][2].as_str().unwrap_or_default();
                let (name, detail) = definition.split_once(" · ").unwrap_or((definition, ""));
                let file = member["cells"][3].as_str().unwrap_or_default();
                items.push(json!({"key":format!("model-member:{}",json!([full,member["key"]])),"parent":group,"searchText":full,"label":name,"detail":if detail.is_empty(){file.to_owned()}else{format!("{detail} · {file}")},"icon":member["icon"],"activationEnabled":member.get("activationEnabled").and_then(Value::as_bool).unwrap_or(true)}));
            }
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tree_groups_members_without_duplicate_model_names_and_keeps_source_navigation() {
        use openidea_plugin::{
            host,
            mock::MockHost,
            views::{ViewEvent, Views},
        };
        let mock = MockHost::install();
        mock.add_file("artisan", "<?php");
        for name in ["Post", "Article"] {
            mock.add_file(&format!("app/Models/{name}.php"), &format!("<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class {name} extends Model {{ protected $table = 'posts'; public function scopeActive($query) {{return $query;}} }}"));
        }
        mock.add_file(
            "database/migrations/001.php",
            "<?php Schema::create('posts',function($t){$t->string('title');});",
        );
        let project = host::projects().unwrap().remove(0);
        let index = Index::build(&project);
        let body = inventory_with_schema(&index, &project, None, &BTreeMap::new());
        let Body::Other { kind, data } = body else {
            panic!("expected tree")
        };
        assert_eq!(kind, "tree");
        let data: Value = serde_json::from_str(&data).unwrap();
        let items = data["items"].as_array().unwrap();
        let roots: Vec<_> = items
            .iter()
            .filter(|row| row.get("parent").is_none())
            .collect();
        assert_eq!(roots.len(), 2);
        assert!(
            roots
                .iter()
                .all(|row| row["expanded"] == false && row["expandable"] == true)
        );
        assert_eq!(
            items
                .iter()
                .filter_map(|row| row["key"].as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            items.len()
        );
        let title = items.iter().find(|row| row["label"] == "title").unwrap();
        assert!(
            title["parent"]
                .as_str()
                .unwrap()
                .starts_with("model-group:")
        );
        assert!(!title["label"].as_str().unwrap().contains("App\\Models"));
        open(&index, &project, title["key"].as_str().unwrap()).unwrap();
        let root = roots[0]["key"].as_str().unwrap().to_owned();
        let mut plugin = crate::Main::default();
        plugin.indexes.insert(project.root().unwrap(), index);
        plugin
            .handle(
                "laravel-models",
                ViewEvent::Expand(root.clone()),
                &mock.invocation(),
            )
            .unwrap();
        let Body::Other { data, .. } = plugin
            .render("laravel-models", None, &mock.invocation())
            .unwrap()
            .body
        else {
            panic!("expected tree")
        };
        let data: Value = serde_json::from_str(&data).unwrap();
        assert!(
            data["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["key"] == root && item["expanded"] == true)
        );
        assert!(
            plugin
                .handle(
                    "laravel-models",
                    ViewEvent::Expand("forged".into()),
                    &mock.invocation()
                )
                .is_err()
        );
        assert!(mock.process_runs().is_empty());
    }
    #[test]
    fn cached_model_inventory_owns_sources_and_rejects_changed_field_destination() {
        let host = openidea_plugin::mock::MockHost::install();
        host.add_file("app/Models/Post.php","<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class Post extends Model {public function scopeActive($query) {return $query;} }");
        host.add_file(
            "database/migrations/001.php",
            "<?php Schema::create('posts',function($t){$t->string('title');});",
        );
        let project = openidea_plugin::host::projects().unwrap().remove(0);
        let index = Index::build(&project);
        let rows = rows(&index, &project);
        assert!(rows.iter().any(|row| row["cells"][1] == "Model"));
        let field = rows.iter().find(|row| row["cells"][1] == "Field").unwrap()["key"]
            .as_str()
            .unwrap()
            .to_owned();
        open(&index, &project, &field).unwrap();
        host.open_editor(
            "/project/database/migrations/001.php",
            "php",
            "<?php // source replaced",
            &[],
        );
        assert!(open(&index, &project, &field).is_err());
        assert!(open(&index, &project, "model-runtime:[]").is_err());
        assert!(host.process_runs().is_empty());
    }
}

#[cfg(test)]
mod primary_tests {
    use super::*;
    use openidea_plugin::{host, mock::MockHost};
    fn index(source: &str) -> (MockHost, Project, Index) {
        let mock = MockHost::install();
        mock.add_file("app/Models/Post.php", source);
        let project = host::projects().unwrap().remove(0);
        let index = Index::build(&project);
        (mock, project, index)
    }
    #[test]
    fn literal_key_configuration_is_source_owned_navigable_and_utf16_exact() {
        let source = "<?php\nnamespace App\\Models; use Illuminate\\Database\\Eloquent\\Model;\n// préface 💡\nclass Post extends Model { protected $primaryKey = 'clé💡'; protected $keyType='string'; public $incrementing = FALSE; function example(){ $primaryKey='fake'; } }";
        let (mock, project, index) = index(source);
        let values = rows(&index, &project);
        let config = values
            .iter()
            .filter(|row| row["cells"][1] == "Primary key")
            .collect::<Vec<_>>();
        assert_eq!(config.len(), 3);
        let primary = config
            .iter()
            .find(|row| {
                row["cells"][2]
                    .as_str()
                    .unwrap()
                    .contains("Primary key: clé💡")
            })
            .unwrap();
        let payload = primary["key"]
            .as_str()
            .unwrap()
            .strip_prefix("model-primary:")
            .unwrap();
        let (_, property, value, offset, width): (String, String, String, usize, u32) =
            serde_json::from_str(payload).unwrap();
        assert_eq!(property, "primaryKey");
        assert_eq!(value, "clé💡");
        assert_eq!(offset, source.find("clé💡").unwrap());
        assert_eq!(width, "clé💡".encode_utf16().count() as u32);
        open(&index, &project, primary["key"].as_str().unwrap()).unwrap();
        mock.open_editor(
            "/project/app/Models/Post.php",
            "php",
            &source.replace("clé💡", "other"),
            &[],
        );
        assert!(open(&index, &project, primary["key"].as_str().unwrap()).is_err());
        assert!(mock.process_runs().is_empty());
    }
    #[test]
    fn dynamic_duplicate_nested_and_inherited_values_are_never_claimed_as_literal_keys() {
        for body in [
            "protected $primaryKey = config('key');",
            "protected $primaryKey = $keyType = 'wrong';",
            "function value(){ $primaryKey='wrong'; }",
            "protected $primaryKey='first'; protected $primaryKey='second';",
            "protected $message = '$primaryKey = fake';",
        ] {
            let source = format!(
                "<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class Post extends Model {{ {body} }}"
            );
            let (_mock, project, index) = index(&source);
            let config = rows(&index, &project)
                .into_iter()
                .filter(|row| row["cells"][1] == "Primary key")
                .collect::<Vec<_>>();
            assert_eq!(config.len(), 1, "{body}");
            assert_eq!(config[0]["activationEnabled"], false);
            assert!(
                config[0]["cells"][2]
                    .as_str()
                    .unwrap()
                    .contains("No proven literal key")
            );
        }
    }
    #[test]
    fn captured_primary_indexes_stay_separate_and_keep_exact_schema_provenance() {
        let source = "<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class Post extends Model {protected $connection='analytics';protected $table='analytics.posts';protected $primaryKey='uuid';}";
        let (_mock, project, index) = index(source);
        let snapshot = json!({"connection":"analytics","tables":[{"name":"posts","schema":"analytics","columns":[{"name":"id","type":"integer","nullable":false}],"indexes":[{"name":"pk","primary":true,"unique":true,"columns":["id"]}]},{"name":"posts","schema":"public","columns":[{"name":"wrong","type":"integer","nullable":false}],"indexes":[{"name":"pk","primary":true,"unique":true,"columns":["wrong"]}]}]});
        let Body::Other { data, .. } =
            inventory_with_schema(&index, &project, Some(&snapshot), &BTreeMap::new())
        else {
            panic!()
        };
        let data: Value = serde_json::from_str(&data).unwrap();
        let items = data["items"].as_array().unwrap();
        assert!(
            items
                .iter()
                .any(|item| item["label"] == "Primary key: uuid")
        );
        let captured = items
            .iter()
            .find(|item| {
                item["label"] == "id"
                    && item["detail"]
                        .as_str()
                        .unwrap_or("")
                        .contains("captured primary index")
            })
            .unwrap();
        assert_eq!(captured["activationEnabled"], false);
        assert!(captured["detail"].as_str().unwrap().contains("analytics"));
        assert!(!items.iter().any(|item| item["label"] == "wrong"));
    }
    #[test]
    fn sources_newly_owned_by_nested_projects_do_not_publish_key_properties() {
        let source = "<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class Post extends Model {protected $primaryKey='uuid';}";
        let (mock, project, index) = index(source);
        assert!(!rows(&index, &project).is_empty());
        mock.add_project("/project/app");
        assert!(rows(&index, &project).is_empty());
    }
    #[test]
    fn captured_schema_is_bound_to_proven_current_model_connection() {
        use openidea_plugin::{host, mock::MockHost};
        let mock = MockHost::install();
        mock.add_file("artisan", "<?php");
        for (name, connection) in [("MainPost", "main"), ("TenantPost", "tenant")] {
            mock.add_file(&format!("app/Models/{name}.php"),&format!("<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class {name} extends Model {{protected $table='posts';protected $connection='{connection}';}}"));
        }
        mock.add_file("app/Models/DefaultPost.php","<?php namespace App\\Models; class DefaultPost extends \\Illuminate\\Database\\Eloquent\\Model {protected $table='posts';}");
        let project = host::projects().unwrap().remove(0);
        let index = Index::build(&project);
        for connection in ["main", "tenant"] {
            let snapshot = json!({"connection":connection,"tables":[{"name":"posts","columns":[{"name":"only_this_connection","type":"string"}]}]});
            let Body::Other { data, .. } =
                inventory_with_schema(&index, &project, Some(&snapshot), &BTreeMap::new())
            else {
                panic!()
            };
            let data: Value = serde_json::from_str(&data).unwrap();
            let captured = data["items"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row["label"] == "only_this_connection")
                .collect::<Vec<_>>();
            assert_eq!(captured.len(), 1);
            let parent = captured[0]["parent"].as_str().unwrap();
            assert!(parent.contains(if connection == "main" {
                "MainPost"
            } else {
                "TenantPost"
            }));
            assert!(!parent.contains("DefaultPost"));
        }
        let snapshot = json!({"tables":[{"name":"posts","columns":[{"name":"unknown_capture","type":"string"}]}]});
        let Body::Other { data, .. } =
            inventory_with_schema(&index, &project, Some(&snapshot), &BTreeMap::new())
        else {
            panic!()
        };
        assert!(!data.contains("unknown_capture"));
        mock.open_editor("/project/app/Models/TenantPost.php","php","<?php namespace App\\Models; class TenantPost extends \\Illuminate\\Database\\Eloquent\\Model {protected $table='posts';protected $connection=config('tenant');}",&[]);
        assert!(
            crate::eloquent_members::related_facts::connection(
                &index,
                &project,
                "App\\Models\\TenantPost"
            )
            .is_none()
        );
        assert!(mock.process_runs().is_empty());
    }

    #[test]
    fn related_groups_and_inherited_typed_facts_use_actual_owned_sources() {
        let mock = MockHost::install();
        mock.add_file("artisan", "<?php");
        mock.add_file("app/Models/BasePost.php","<?php namespace App\\Models; class BasePost extends \\Illuminate\\Database\\Eloquent\\Model {protected $table='posts';protected $casts=['enabled'=>'boolean'];public function getTitleAttribute(): string {return 'title';}public function scopeActive(\\Illuminate\\Database\\Eloquent\\Builder $query,bool $enabled=true): void {}} ");
        mock.add_file(
            "app/Models/Post.php",
            "<?php namespace App\\Models; class Post extends BasePost {} ",
        );
        mock.add_file("app/PostMaker.php","<?php namespace App; class PostMaker extends \\Illuminate\\Database\\Eloquent\\Factories\\Factory {protected $model=\\App\\Models\\Post::class;}");
        mock.add_file("app/PostFactory.php","<?php namespace App; class PostFactory extends \\Illuminate\\Database\\Eloquent\\Factories\\Factory {}");
        mock.add_file("app/PostResource.php","<?php namespace App; /** @mixin \\App\\Models\\Post */ class PostResource extends \\Illuminate\\Http\\Resources\\Json\\JsonResource {}");
        mock.add_file("database/migrations/001.php","<?php Schema::create('posts',function($table){$table->string('title');$table->boolean('enabled');});");
        let project = host::projects().unwrap().remove(0);
        let index = Index::build(&project);
        let model_rows = rows(&index, &project)
            .into_iter()
            .filter(|row| row["cells"][0] == "App\\Models\\Post")
            .collect::<Vec<_>>();
        for kind in [
            "Factory",
            "Resource",
            "Migration",
            "Accessor",
            "Cast",
            "Scope",
        ] {
            assert!(
                model_rows.iter().any(|row| row["cells"][1] == kind),
                "missing {kind}: {model_rows:?}"
            );
        }
        assert!(
            !model_rows
                .iter()
                .any(|row| row["cells"][2] == "App\\PostFactory")
        );
        assert!(model_rows.iter().any(|row| {
            row["cells"][1] == "Field"
                && row["cells"][2]
                    .as_str()
                    .unwrap()
                    .contains("title · string (accessor)")
        }));
        let factory = model_rows
            .iter()
            .find(|row| row["cells"][1] == "Factory")
            .unwrap();
        mock.open_editor("/project/app/PostMaker.php","php","<?php namespace App; class PostMaker extends OtherFactory {protected $model=\\App\\Models\\Post::class;}",&[]);
        assert!(open(&index, &project, factory["key"].as_str().unwrap()).is_err());
        let accessor = model_rows
            .iter()
            .find(|row| row["cells"][1] == "Accessor")
            .unwrap();
        open(&index, &project, accessor["key"].as_str().unwrap()).unwrap();
        mock.open_editor("/project/app/Models/BasePost.php","php","<?php namespace App\\Models; class BasePost extends \\Illuminate\\Database\\Eloquent\\Model {}",&[]);
        assert!(open(&index, &project, accessor["key"].as_str().unwrap()).is_err());
        assert!(mock.process_runs().is_empty());
    }
}

// Presentation only; the raw rows are still used by machine kind filters,
// freshness checks and source-action identities.
fn present_tree(nodes: &mut [Value], rows: &[Value]) {
    let by_key = rows
        .iter()
        .filter_map(|row| Some((row["key"].as_str()?, row)))
        .collect::<BTreeMap<_, _>>();
    for node in nodes {
        let key = node["key"].as_str().unwrap_or_default().to_owned();
        if let Some(raw) = key.strip_prefix("model-group:") {
            if let Ok((_, kind)) = serde_json::from_str::<(String, String)>(raw) {
                if let Some(count) = node["label"]
                    .as_str()
                    .and_then(|text| text.rsplit_once(" ("))
                    .and_then(|(_, tail)| tail.strip_suffix(')'))
                    .and_then(|tail| tail.parse::<usize>().ok())
                {
                    let label = group_label(&kind);
                    node["label"] = crate::ui_text::format(
                        "final.models.groupCount",
                        "{label} ({count})",
                        &[("label", &label), ("count", &count.to_string())],
                    )
                    .into();
                }
            }
            continue;
        }
        let Some(raw) = key.strip_prefix("model-member:") else {
            continue;
        };
        let Ok((_, source_key)) = serde_json::from_str::<(String, String)>(raw) else {
            continue;
        };
        let Some(row) = by_key.get(source_key.as_str()) else {
            continue;
        };
        let kind = row["cells"][1].as_str().unwrap_or_default();
        let definition = row["cells"][2].as_str().unwrap_or_default();
        let file = row["cells"][3].as_str().unwrap_or_default();
        let (_, detail) = definition.split_once(" · ").unwrap_or((definition, ""));
        let mut display = detail.to_owned();
        if source_key.starts_with("model-key-unknown:") {
            node["label"]=crate::ui_text::t("final.models.unknownKey","No proven literal key in this model · inherited/runtime configuration is not inferred").into();
            display.clear();
        } else if let Some(raw) = source_key.strip_prefix("model-primary:") {
            if let Ok((_, property, value, _, _)) =
                serde_json::from_str::<(String, String, String, u64, u64)>(raw)
            {
                let label = match property.as_str() {
                    "primaryKey" => crate::ui_text::text("final.models.primaryKey", "Primary key"),
                    "keyType" => crate::ui_text::text("final.models.keyType", "Key type"),
                    _ => crate::ui_text::text("final.models.incrementing", "Incrementing"),
                };
                node["label"] = crate::ui_text::format(
                    "final.models.primaryValue",
                    "{label}: {value}",
                    &[("label", label), ("value", &value)],
                )
                .into();
                display = crate::ui_text::format(
                    "final.models.declaredProperty",
                    "declared ${property}",
                    &[("property", &property)],
                );
            }
        } else if kind == "Field" {
            if let Some(value) = detail.strip_suffix(" (accessor)") {
                display = crate::ui_text::format(
                    "final.models.accessorType",
                    "{value} (accessor)",
                    &[("value", value)],
                );
            } else if let Some(value) = detail.strip_suffix(" (cast)") {
                display = crate::ui_text::format(
                    "final.models.castType",
                    "{value} (cast)",
                    &[("value", value)],
                );
            }
        } else if kind == "Scope" && detail == "Source scope; signature unresolved" {
            display = crate::ui_text::t("final.models.unresolvedScope", detail);
        } else if matches!(kind, "Factory" | "Resource" | "Migration")
            && detail == "declared association"
        {
            display = crate::ui_text::t("final.models.declaredAssociation", detail);
        }
        node["detail"] = if display.is_empty() {
            file.into()
        } else {
            format!("{display} · {file}").into()
        };
    }
}
fn group_label(kind: &str) -> String {
    match kind {
        "Primary key" => crate::ui_text::t("final.models.group.primary-key", "Key configuration"),
        "Database primary key" => crate::ui_text::t(
            "final.models.group.database-primary-key",
            "Database primary keys",
        ),
        "Field" => crate::ui_text::t("final.models.group.field", "Fields"),
        "Cast" => crate::ui_text::t("final.models.group.cast", "Casts"),
        "Accessor" => crate::ui_text::t("final.models.group.accessor", "Accessors"),
        "Factory" => crate::ui_text::t("final.models.group.factory", "Factories"),
        "Resource" => crate::ui_text::t("final.models.group.resource", "Resources"),
        "Migration" => crate::ui_text::t("final.models.group.migration", "Migrations"),
        "Relation" => crate::ui_text::t("final.models.group.relation", "Relations"),
        "Scope" => crate::ui_text::t("final.models.group.scope", "Scopes"),
        "Database field" => {
            crate::ui_text::t("final.models.group.database-field", "Database fields")
        }
        _ => kind.into(),
    }
}

#[cfg(test)]
mod presentation_tests {
    use super::*;
    #[test]
    fn localized_tree_preserves_raw_rows_source_names_and_action_keys() {
        let host = openidea_plugin::mock::MockHost::install();
        let model = "App\\Models\\Fields {class} 日本";
        let raw = vec![
            json!({"key":"model:opaque","cells":[model,"Model","opaque_table","app/Models/Fields.php"],"icon":"class"}),
            json!({"key":"field:opaque","cells":[model,"Field","raw_{value} · string (accessor)","app/Models/Fields.php"],"icon":"field"}),
        ];
        let snapshot = raw.clone();
        for locale in ["ja", "ko", "zh-CN"] {
            host.set_setting("uiLanguage", &serde_json::to_string(locale).unwrap());
            let mut nodes = tree(&raw, &BTreeMap::new());
            let before = nodes.clone();
            present_tree(&mut nodes, &raw);
            assert_eq!(raw, snapshot);
            assert_eq!(nodes[0]["label"], model);
            assert_ne!(nodes[1]["label"], before[1]["label"]);
            assert_eq!(nodes[2]["label"], "raw_{value}");
            assert!(nodes[2]["detail"].as_str().unwrap().contains("string"));
            assert!(!nodes[2]["detail"].as_str().unwrap().contains("accessor"));
            for (after, before) in nodes.iter().zip(before.iter()) {
                assert_eq!(after["key"], before["key"]);
                assert_eq!(after["parent"], before["parent"]);
            }
        }
    }
}
