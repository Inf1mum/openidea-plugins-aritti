//! Native model helper and field-migration actions. Every multi-file edit is
//! returned to the host as one undoable transaction, never written directly.
use crate::{eloquent_members, generator_plan, index::Index, php};
use openidea_plugin::{
    Error, Position, Range, TextEdit,
    file_actions::{FileAction, FileOperation},
    host,
    providers::Document,
    text,
};
use std::collections::BTreeSet;

#[path="model_helper_parameters.rs"]
pub(crate) mod parameters;
pub(crate) fn configure(ctx:&openidea_plugin::Invocation)->Result<(),Error>{parameters::configure(ctx)}
pub(crate) fn begin_project(ctx:&openidea_plugin::Invocation)->Result<(),Error>{
    let project=ctx.project().ok_or("Open a Laravel project first")?;let root=project.root()?;let index=Index::build(&project);
    if index.partial{return Err(Error::Stale);}
    let models=eloquent_members::model_inventory(&index.eloquent_models).into_iter().filter(|(full,path,_)|helper_codegen::eligible(&index,full)&&crate::owns_current_path(&project,path)).collect::<Vec<_>>();
    if models.is_empty(){return Err(Error::Invalid("No owned eligible Eloquent model is indexed".into()));}
    let items=models.iter().map(|(full,_,_)|openidea_plugin::PickItem::new(full,full)).collect::<Vec<_>>();
    let Some(chosen)=host::pick("Open a model for reviewed project helper regeneration",&items)?else{return Ok(())};
    let (_,path,_)=models.into_iter().find(|(full,_,_)|full==&chosen).ok_or(Error::Stale)?;
    if ctx.project().and_then(|project|project.root().ok()).as_deref()!=Some(&root)||!crate::owns_current_path(&project,&path){return Err(Error::Stale);}
    host::open_editor(&path,None)?;
    host::notify(openidea_plugin::NotificationLevel::Info,"Laravel","Choose ‘Regenerate reviewed helpers for indexed models…’ from this model's editor actions. Selected files are reviewed and applied as one undoable edit.");
    Ok(())
}

fn model(index: &Index, path: &str) -> Option<(String, String)> {
    eloquent_members::model_inventory(&index.eloquent_models)
        .into_iter()
        .find(|(_, p, _)| p == path)
        .map(|(full, _, table)| (full, table))
}
pub(crate) fn actions(index: &Index, doc: &Document) -> Vec<FileAction> {
    if doc.language != "php" || index.partial || model(index, &doc.path).is_none() {
        return Vec::new();
    }
    [
        ("helpers", "Generate reviewed model helpers…"),
        (
            "helpers-all",
            "Regenerate reviewed helpers for indexed models…",
        ),
        (
            "docs",
            "Refresh generated model PHPDoc (preserve handwritten)…",
        ),
        ("fields", "Create model fields and migration…"),
    ]
    .into_iter()
    .filter(|(kind, _)| {
        *kind == "fields"
            || model(index, &doc.path)
                .is_some_and(|(full, _)| helper_codegen::eligible(index, &full))
    })
    .map(|(kind, title)| FileAction {
        title: title.into(),
        kind: Some("refactor.rewrite".into()),
        data: Some(format!("model-native:{kind}:{}", doc.version)),
        ..Default::default()
    })
    .collect()
}
fn owner(doc: &Document) -> Result<openidea_plugin::Project, Error> {
    let project = host::project_of(&doc.path).ok_or(Error::Stale)?;
    if !crate::owns_current_path(&project, &doc.path) || project.read_file(&doc.path)? != doc.text {
        return Err(Error::Stale);
    }
    Ok(project)
}
#[path = "model_helper_codegen.rs"]
mod helper_codegen;
fn tags(
    index: &Index,
    project: &openidea_plugin::Project,
    full: &str,
    _table: &str,
) -> Result<String, Error> {
    let parameters=parameters::load(project)?;
    helper_codegen::tags(index, project, full, helper_codegen::Options{relation_counts:parameters.relation_counts,date_class:parameters.date_class})
}

fn entire(text: &str, replacement: String) -> TextEdit {
    TextEdit {
        range: Range::new(Position::new(0, 0), text::position(text, text.len())),
        text: replacement,
    }
}
fn replace_docs(source: &str, tags: &str) -> Result<String, Error> {
    let (class, _) = eloquent_members::class_layout(source).ok_or(Error::Stale)?;
    let start_marker = " * @openidea-laravel-generated-start\n";
    let end_marker = " * @openidea-laravel-generated-end\n";
    let prefix = source[..class].trim_end();
    let manual = if prefix.ends_with("*/") {
        prefix
            .rfind("/**")
            .map(|begin| &prefix[begin..])
            .unwrap_or("")
    } else {
        ""
    };
    let mut manual = manual.to_owned();
    if let Some(begin) = manual.find(start_marker) {
        if let Some(end) = manual[begin..].find(end_marker) {
            manual.replace_range(begin..begin + end + end_marker.len(), "");
        }
    }
    let property_name = |line: &str| {
        line.split_whitespace()
            .find(|part| part.starts_with('$'))
            .map(|name| {
                name.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_')
                    .to_owned()
            })
    };
    let method_name = |line: &str| {
        line.split_once('(')
            .and_then(|(before, _)| before.split_whitespace().last())
            .map(str::to_owned)
    };
    let manual_properties = manual
        .lines()
        .filter(|line| line.contains("@property"))
        .filter_map(property_name)
        .collect::<BTreeSet<_>>();
    let manual_methods = manual
        .lines()
        .filter(|line| line.contains("@method"))
        .filter_map(method_name)
        .collect::<BTreeSet<_>>();
    let tags = tags
        .lines()
        .filter(|line| {
            if line.contains("@property") {
                !property_name(line).is_some_and(|name| manual_properties.contains(&name))
            } else if line.contains("@method") {
                !method_name(line).is_some_and(|name| manual_methods.contains(&name))
            } else {
                true
            }
        })
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    let block = format!("{start_marker}{tags}{end_marker}");
    let mut result = source.to_owned();
    if prefix.ends_with("*/") {
        let end = prefix.len() - 2;
        // Extend the immediately preceding PHPDoc only; arbitrary comments are
        // never interpreted as a generated documentation ownership marker.
        let begin = prefix.rfind("/**").ok_or(Error::Stale)?;
        if prefix[begin + 3..end].contains("*/") {
            return Err(Error::Stale);
        }
        if let Some(relative) = prefix[begin..end].find(start_marker) {
            let start = begin + relative;
            let finish = start
                + start_marker.len()
                + source[start + start_marker.len()..end]
                    .find(end_marker)
                    .ok_or(Error::Stale)?
                + end_marker.len();
            result.replace_range(start..finish, &block);
            return Ok(result);
        }
        result.insert_str(end, &format!("\n{block} "));
    } else {
        result.insert_str(class, &format!("/**\n{block} */\n"));
    }
    Ok(result)
}
fn fillable(source: &str, fields: &[generator_plan::Field]) -> Result<String, Error> {
    let (_, body, array) = eloquent_members::edit_layout(source).ok_or(Error::Stale)?;
    let mut result = source.to_owned();
    let mut additions = Vec::new();
    if let Some(range) = array {
        let mut existing = BTreeSet::new();
        for item in php::items(source, range.start) {
            let (name, _) = php::literal(source, item.value).ok_or_else(|| {
                Error::Invalid("Only literal $fillable arrays can be updated".into())
            })?;
            existing.insert(name.to_owned());
        }
        additions.extend(
            fields
                .iter()
                .filter(|f| !existing.contains(&f.name))
                .map(|f| format!("'{}'", f.name)),
        );
        if !additions.is_empty() {
            let body = source[range.start + 1..range.end - 1].trim();
            let separator = if body.is_empty() || body.ends_with(',') {
                ""
            } else {
                ","
            };
            result.insert_str(
                range.end - 1,
                &format!("{separator} {}", additions.join(", ")),
            );
        }
    } else {
        additions.extend(fields.iter().map(|f| format!("'{}'", f.name)));
        result.insert_str(
            body,
            &format!("\n    protected $fillable = [{}];\n", additions.join(", ")),
        );
    }
    Ok(result)
}

pub(crate) fn resolve(
    index: &Index,
    doc: &Document,
    action: FileAction,
) -> Result<FileAction, Error> {
    let data = action.data.as_deref().ok_or(Error::Stale)?;
    let kind = ["helpers", "docs", "fields", "helpers-all"]
        .into_iter()
        .find(|kind| data == format!("model-native:{kind}:{}", doc.version))
        .ok_or(Error::Stale)?;
    if kind == "helpers-all" {
        return resolve_batch(doc, action);
    }
    let project = owner(doc)?;
    let root = project.root()?;
    let current = Index::build(&project);
    let (full, table) = model(&current, &doc.path).ok_or(Error::Stale)?;
    if model(index, &doc.path) != Some((full.clone(), table.clone())) || current.partial {
        return Err(Error::Stale);
    }
    if kind != "fields" && !helper_codegen::eligible(&current, &full) {
        return Err(Error::Invalid(
            "Model is excluded from helper generation by project metadata".into(),
        ));
    }
    let generated_tags = tags(&current, &project, &full, &table)?;
    let mut operations = Vec::new();
    let mut edits = Vec::new();
    let mut expected = Vec::new();
    let mut open = Some(doc.path.clone());
    let preferences=parameters::load(&project)?;
    if kind == "helpers" && preferences.storage=="helpers" {
        let short = full.rsplit('\\').next().ok_or(Error::Stale)?;
        let namespace = full.rsplit_once('\\').map(|(ns, _)| ns).unwrap_or("");
        let path = format!("{root}/.openidea/helpers/{}.php", full.replace('\\', "_"));
        if !crate::owns_current_path(&project, &path) {
            return Err(Error::Stale);
        }
        let marker = "// Generated by OpenIDEA Laravel. Do not edit this helper.\n";
        let namespace_line = if namespace.is_empty() {
            String::new()
        } else {
            format!("namespace {namespace};\n")
        };
        let text = format!(
            "<?php\n{marker}\n{namespace_line}\n\n/**\n{generated_tags} */\nclass {short} extends \\Illuminate\\Database\\Eloquent\\Model {{}}\n"
        );
        match project.read_file(&path) {
            Ok(previous)
                if previous.starts_with(&format!("<?php\n{marker}"))
                    && eloquent_members::declaration(&previous).as_deref() == Some(&full) =>
            {
                expected.push((path.clone(), previous.clone()));
                edits.push((path.clone(), vec![entire(&previous, text)]));
            }
            Ok(_) => {
                return Err(Error::Invalid(
                    "Helper destination is not owned by this plugin".into(),
                ));
            }
            Err(Error::NotFound(_)) => operations.push(FileOperation::Create {
                path: path.clone(),
                text,
                overwrite: false,
            }),
            Err(e) => return Err(e),
        }
        open = Some(path);
    } else if kind == "docs" || kind=="helpers" {
        edits.push((
            doc.path.clone(),
            vec![entire(&doc.text, if kind=="helpers"&&preferences.storage=="overwrite"{replace_all_docs(&doc.text,&generated_tags)?}else{replace_docs(&doc.text, &generated_tags)?})],
        ));
    } else {
        let raw = host::input(
            "Fields JSON (name, kind, nullable, length, precision, scale, default)",
            Some(r#"[{"name":"title","kind":"string"}]"#),
        )?
        .ok_or(Error::Cancelled)?;
        if raw.len() > 32 * 1024 {
            return Err(Error::Invalid("Field specification too large".into()));
        }
        let fields: Vec<generator_plan::Field> =
            serde_json::from_str(&raw).map_err(|e| Error::Invalid(e.to_string()))?;
        if fields.is_empty() || fields.len() > 64 {
            return Err(Error::Invalid("Choose 1–64 fields".into()));
        }
        let mut seen = BTreeSet::new();
        let mut lines = String::new();
        for field in &fields {
            if !generator_plan::identifier(&field.name)
                || !seen.insert(field.name.clone())
                || current
                    .columns
                    .get(&table)
                    .is_some_and(|names| names.contains_key(&field.name))
            {
                return Err(Error::Invalid(
                    "Field names must be unique new column identifiers".into(),
                ));
            }
            if field.cast.is_some() {
                return Err(Error::Invalid(
                    "Create Fields currently requires casts to be edited separately".into(),
                ));
            }
            lines.push_str(&generator_plan::field_line(field).map_err(Error::Invalid)?);
        }
        let relative = host::input(
            "New migration path",
            Some(&format!(
                "database/migrations/{}_add_fields_to_{table}.php",
                migration_stamp()
            )),
        )?
        .ok_or(Error::Cancelled)?;
        if !generator_plan::relative(&relative) || !relative.ends_with(".php") {
            return Err(Error::Invalid(
                "Choose a relative PHP migration path".into(),
            ));
        }
        let path = format!("{root}/{relative}");
        if !crate::owns_current_path(&project, &path) {
            return Err(Error::Stale);
        }
        match project.stat(&path) {
            Err(Error::NotFound(_)) => {}
            Ok(_) => return Err(Error::Invalid("Migration already exists".into())),
            Err(e) => return Err(e),
        }
        if !generator_plan::identifier(&table) {
            return Err(Error::Invalid(
                "Table must be an ordinary identifier".into(),
            ));
        }
        let down = fields
            .iter()
            .map(|f| format!("'{}'", f.name))
            .collect::<Vec<_>>()
            .join(", ");
        let migration = format!(
            "<?php\n\nuse Illuminate\\Database\\Migrations\\Migration;\nuse Illuminate\\Database\\Schema\\Blueprint;\nuse Illuminate\\Support\\Facades\\Schema;\n\nreturn new class extends Migration {{\n    public function up(): void {{\n        Schema::table('{table}', function (Blueprint $table): void {{\n{lines}        }});\n    }}\n    public function down(): void {{\n        Schema::table('{table}', function (Blueprint $table): void {{\n            $table->dropColumn([{down}]);\n        }});\n    }}\n}};\n"
        );
        operations.push(FileOperation::Create {
            path: path.clone(),
            text: migration,
            overwrite: false,
        });
        let update = host::pick(
            "Model mass-assignment update",
            &[
                openidea_plugin::PickItem::new("fillable", "Add fields to $fillable"),
                openidea_plugin::PickItem::new("keep", "Keep current model configuration"),
            ],
        )?
        .ok_or(Error::Cancelled)?;
        match update.as_str() {
            "fillable" => edits.push((
                doc.path.clone(),
                vec![entire(&doc.text, fillable(&doc.text, &fields)?)],
            )),
            "keep" => {}
            _ => return Err(Error::Stale),
        }
        open = Some(path);
    }
    let preview = format!(
        "Project: {root}\nModel: {full}\n{}\n{}\n{}\nApply as one undoable workspace edit?",
        if kind=="helpers"&&preferences.storage=="overwrite" { "Saved overwrite mode: replaces attached model PHPDoc, including handwritten documentation. Review the entire resulting model below." } else { "" },
        operations
            .iter()
            .map(|op| match op {
                FileOperation::Create { path, text, .. } =>
                    format!("Create (no overwrite): {path}\n{text}"),
                _ => unreachable!("model actions only create files"),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        edits
            .iter()
            .map(|(path, edits)| format!(
                "{path}\n{}",
                edits
                    .iter()
                    .map(|e| e.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if !ui_kit::prompts::confirm_exact("Apply model generation?", &preview)? {
        return Err(Error::Cancelled);
    }
    owner(doc)?;
    // Regenerate facts after review to avoid applying a cached schema plan.
    let fresh = Index::build(&project);
    if parameters::load(&project)?!=preferences || model(&fresh, &doc.path) != Some((full.clone(), table.clone()))
        || kind != "fields" && !helper_codegen::eligible(&fresh, &full)
        || tags(&fresh, &project, &full, &table)? != generated_tags
    {
        return Err(Error::Stale);
    }
    for (path, _) in &edits {
        check_target(&project, path)?;
    }
    for (path, source) in &expected {
        if project.read_file(path)? != *source {
            return Err(Error::Stale);
        }
    }
    for op in &operations {
        if let FileOperation::Create { path, .. } = op {
            check_target(&project, path)?;
            if !matches!(project.stat(path), Err(Error::NotFound(_))) {
                return Err(Error::Stale);
            }
        }
    }
    Ok(FileAction {
        title: action.title,
        operations,
        edits,
        open,
        ..Default::default()
    })
}

fn check_target(project: &openidea_plugin::Project, path: &str) -> Result<(), Error> {
    if !crate::owns_current_path(project, path) {
        return Err(Error::Stale);
    }
    let root = project.root()?;
    let relative = path.strip_prefix(&format!("{root}/")).ok_or(Error::Stale)?;
    if !generator_plan::relative(relative) {
        return Err(Error::Stale);
    }
    let mut parts = relative.split('/').collect::<Vec<_>>();
    parts.pop();
    while !parts.is_empty() {
        let parent = parts.join("/");
        match project.stat(&parent) {
            Ok(stat) if stat.kind == openidea_plugin::FileKind::Directory => {}
            Err(Error::NotFound(_)) => {}
            Ok(_) => {
                return Err(Error::Invalid(format!(
                    "Target ancestor is not a directory: {parent}"
                )));
            }
            Err(e) => return Err(e),
        }
        parts.pop();
    }
    Ok(())
}

fn migration_stamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Proleptic Gregorian conversion, UTC, independent of desktop locale.
    let days = (seconds / 86400) as i64 + 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    format!(
        "{year:04}_{month:02}_{day:02}_{:02}{:02}{:02}",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn setup() -> (MockHost, Index, Document) {
        let host = MockHost::install();
        let source = "<?php\nnamespace App\\Models;\nuse Illuminate\\Database\\Eloquent\\Model;\n/** Human-written description. */\nfinal class Post extends Model { protected $fillable = ['title']; }";
        host.add_file("app/Models/Post.php", source);
        host.add_file(
            "database/migrations/001.php",
            "<?php Schema::create('posts',function($t){$t->string('title');});",
        );
        let project = host::projects().unwrap().remove(0);
        let index = Index::build(&project);
        (
            host,
            index,
            Document {
                path: "/project/app/Models/Post.php".into(),
                language: "php".into(),
                version: 1,
                text: source.into(),
            },
        )
    }
    #[test]
    fn phpdoc_preserves_handwritten_content_and_refresh_is_idempotent() {
        let source = "<?php /** Human-written description. */ final class Post extends Model {}";
        let tags = " * @property string $title\n";
        let first = replace_docs(source, tags).unwrap();
        assert!(first.contains("Human-written description."));
        assert!(first.contains("*/ final class"));
        assert_eq!(replace_docs(&first, tags).unwrap(), first);
    }
    #[test]
    fn helper_action_creates_owned_file_without_process_or_direct_write() {
        let (h, index, doc) = setup();
        let action = actions(&index, &doc)
            .into_iter()
            .find(|a| a.data.as_deref() == Some("model-native:helpers:1"))
            .unwrap();
        h.answer_next_prompt(Some(&["yes"]));
        let resolved = resolve(&index, &doc, action).unwrap();
        assert_eq!(resolved.operations.len(), 1);
        let FileOperation::Create {
            path,
            text,
            overwrite,
        } = &resolved.operations[0]
        else {
            panic!()
        };
        assert_eq!(path, "/project/.openidea/helpers/App_Models_Post.php");
        assert!(!overwrite);
        assert!(text.contains("@property string $title"));
        assert!(h.process_runs().is_empty());
    }
    #[test]
    fn fields_return_migration_and_literal_fillable_edit_in_one_action() {
        let (h, index, doc) = setup();
        let action = actions(&index, &doc)
            .into_iter()
            .find(|a| a.data.as_deref() == Some("model-native:fields:1"))
            .unwrap();
        h.answer_next_prompt(Some(&[
            r#"[{"name":"published","kind":"boolean","default":false}]"#,
        ]));
        h.answer_next_prompt(Some(&[
            "database/migrations/2026_11_10_120000_add_fields_to_posts.php",
        ]));
        h.answer_next_prompt(Some(&["fillable"]));
        h.answer_next_prompt(Some(&["yes"]));
        let resolved = resolve(&index, &doc, action).unwrap();
        assert_eq!(resolved.operations.len(), 1);
        assert_eq!(resolved.edits.len(), 1);
        assert!(
            resolved.edits[0].1[0]
                .text
                .contains("['title', 'published']")
        );
        assert!(h.process_runs().is_empty());
    }
    #[test]
    fn helper_exclusions_keep_existing_model_fields_workflow() {
        let (h, _, doc) = setup();
        h.add_file(
            "ide.json",
            r#"{"helperCode":{"eloquent":{"ignoreNamespaces":["App\\Models"]}}}"#,
        );
        let index = Index::build(&host::projects().unwrap().remove(0));
        let available = actions(&index, &doc);
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].data.as_deref(), Some("model-native:fields:1"));
    }
    #[test]
    fn cancelled_generation_is_not_a_partial_edit() {
        let (h, index, doc) = setup();
        let action = actions(&index, &doc).remove(0);
        h.answer_next_prompt(None);
        assert_eq!(resolve(&index, &doc, action), Err(Error::Cancelled));
        assert!(h.process_runs().is_empty());
    }
}

fn replace_all_docs(source: &str, tags: &str) -> Result<String, Error> {
    let (class, _) = eloquent_members::class_layout(source).ok_or(Error::Stale)?;
    let prefix = source[..class].trim_end();
    let block = format!("/**\n{tags} */");
    let mut output = source.to_owned();
    if prefix.ends_with("*/") {
        let begin = prefix.rfind("/**").ok_or(Error::Stale)?;
        if prefix[begin + 3..prefix.len() - 2].contains("*/") {
            return Err(Error::Stale);
        }
        output.replace_range(begin..prefix.len(), &block);
    } else {
        output.insert_str(class, &format!("{block}\n"));
    }
    Ok(output)
}
fn helper_text(full: &str, tags: &str) -> Result<String, Error> {
    if !generator_plan::qualified(full) {
        return Err(Error::Stale);
    }
    let short = full.rsplit('\\').next().ok_or(Error::Stale)?;
    let namespace = full
        .rsplit_once('\\')
        .map(|(namespace, _)| format!("namespace {namespace};\n"))
        .unwrap_or_default();
    Ok(format!(
        "<?php\n// Generated by OpenIDEA Laravel. Do not edit this helper.\n{namespace}\n/**\n{tags} */\nclass {short} extends \\Illuminate\\Database\\Eloquent\\Model {{}}\n"
    ))
}

/// Selection and storage mode precede exact per-file reviews. No source changes
/// happen until the host applies the returned single workspace transaction.
fn resolve_batch(doc: &Document, action: FileAction) -> Result<FileAction, Error> {
    let project = owner(doc)?;
    let root = project.root()?;
    let index = Index::build(&project);
    if index.partial {
        return Err(Error::Stale);
    }
    let models = eloquent_members::model_inventory(&index.eloquent_models)
        .into_iter()
        .filter(|(full, path, _)| {
            helper_codegen::eligible(&index, full) && crate::owns_current_path(&project, path)
        })
        .collect::<Vec<_>>();
    let items = models
        .iter()
        .map(|(full, _, _)| openidea_plugin::PickItem::new(full, full))
        .collect::<Vec<_>>();
    let chosen = host::pick_many(
        "Models to regenerate in one undoable edit (up to 32)",
        &items,
    )?
    .ok_or(Error::Cancelled)?;
    let chosen = chosen.into_iter().collect::<BTreeSet<_>>();
    if chosen.is_empty()
        || chosen.len() > 32
        || chosen
            .iter()
            .any(|full| !models.iter().any(|(model, _, _)| model == full))
    {
        return Err(Error::Invalid("Select 1–32 indexed, owned models".into()));
    }
    let preferences=parameters::load(&project)?;
    let selected_mode = host::pick(
        "Generated helper storage (or use saved parameters)",
        &[
            openidea_plugin::PickItem::new("saved",&format!("Use saved parameters ({})",preferences.storage)),
            openidea_plugin::PickItem::new("helpers", "Only in plugin-owned helper files"),
            openidea_plugin::PickItem::new(
                "preserve",
                "In model PHPDoc, preserving handwritten documentation",
            ),
            openidea_plugin::PickItem::new(
                "overwrite",
                "In model PHPDoc, replacing all attached PHPDoc",
            ),
        ],
    )?
    .ok_or(Error::Cancelled)?;
    let mode=if selected_mode=="saved"{preferences.storage.clone()}else{selected_mode};
    if !matches!(mode.as_str(), "helpers" | "preserve" | "overwrite") {
        return Err(Error::Stale);
    }
    let flags = host::pick_many(
        "Additional annotations",
        &[openidea_plugin::PickItem::new("saved","Use saved relationship count setting"),openidea_plugin::PickItem::new(
            "counts",
            "Relationship count attributes (populated by withCount)",
        )],
    )?
    .ok_or(Error::Cancelled)?;
    if flags.iter().any(|flag| !matches!(flag.as_str(),"counts"|"saved")) || flags.len()>1 {
        return Err(Error::Stale);
    }
    let options = helper_codegen::Options {
        relation_counts: flags.iter().any(|flag| flag == "counts")||flags.iter().any(|flag|flag=="saved")&&preferences.relation_counts,
        date_class:preferences.date_class.clone(),
    };
    let mut operations = Vec::new();
    let mut edits = Vec::new();
    let mut expected = Vec::new();
    let mut snapshots = Vec::new();
    let mut previews = Vec::new();
    let mut targets = BTreeSet::new();
    for (full, source_path, _) in models.iter().filter(|(full, _, _)| chosen.contains(full)) {
        let source = project.read_file(source_path)?;
        if source.len() > 128 * 1024
            || eloquent_members::declaration(&source).as_deref() != Some(full)
        {
            return Err(Error::Stale);
        }
        let tags = helper_codegen::tags(&index, &project, full, options.clone())?;
        let (path, contents) = if mode == "helpers" {
            (
                format!("{root}/.openidea/helpers/{}.php", full.replace('\\', "_")),
                helper_text(full, &tags)?,
            )
        } else {
            (
                source_path.clone(),
                if mode == "overwrite" {
                    replace_all_docs(&source, &tags)?
                } else {
                    replace_docs(&source, &tags)?
                },
            )
        };
        if !targets.insert(path.clone()) {
            return Err(Error::Invalid(
                "Helper output paths collide for selected class names; use model PHPDoc mode"
                    .into(),
            ));
        }
        check_target(&project, &path)?;
        match project.read_file(&path) {
            Ok(previous) => {
                if mode == "helpers"
                    && (!previous.starts_with(
                        "<?php\n// Generated by OpenIDEA Laravel. Do not edit this helper.\n",
                    ) || eloquent_members::declaration(&previous).as_deref() != Some(full))
                {
                    return Err(Error::Invalid(format!(
                        "Helper destination is not owned by this model: {path}"
                    )));
                }
                expected.push((path.clone(), previous.clone()));
                edits.push((path.clone(), vec![entire(&previous, contents.clone())]));
            }
            Err(Error::NotFound(_)) if mode == "helpers" => {
                operations.push(FileOperation::Create {
                    path: path.clone(),
                    text: contents.clone(),
                    overwrite: false,
                })
            }
            Err(error) => return Err(error),
        }
        let preview = format!(
            "Project: {root}\nModel: {full}\nStorage mode: {mode}\nTarget: {path}\n{}\n\nComplete resulting file:\n{contents}\n\nInclude this file in the final atomic edit?",
            if mode == "overwrite" {
                "All attached handwritten PHPDoc will be replaced."
            } else {
                ""
            }
        );
        if preview.len() > ui_kit::prompts::MAX_CONFIRM_BYTES {
            return Err(Error::Invalid(format!(
                "Complete file preview exceeds the host review limit: {path}; select a smaller model"
            )));
        }
        previews.push(preview);
        snapshots.push((full.clone(), source_path.clone(), source, tags));
    }
    for preview in &previews {
        if !ui_kit::prompts::confirm_exact("Review model helper file?", preview)? {
            return Err(Error::Cancelled);
        }
    }
    let summary = format!(
        "Apply {} reviewed files in one undoable workspace edit?\nProject: {root}\nStorage: {mode}\n{}",
        targets.len(),
        targets.iter().cloned().collect::<Vec<_>>().join("\n")
    );
    if !ui_kit::prompts::confirm_exact("Regenerate reviewed model helpers?", &summary)? {
        return Err(Error::Cancelled);
    }
    owner(doc)?;
    if parameters::load(&project)?!=preferences{return Err(Error::Stale);}
    let fresh = Index::build(&project);
    if fresh.partial {
        return Err(Error::Stale);
    }
    for (full, path, source, tags) in snapshots {
        if !helper_codegen::eligible(&fresh, &full)
            || !crate::owns_current_path(&project, &path)
            || project.read_file(&path)? != source
            || helper_codegen::tags(&fresh, &project, &full, options.clone())? != tags
        {
            return Err(Error::Stale);
        }
    }
    for (path, source) in &expected {
        check_target(&project, path)?;
        if project.read_file(path)? != *source {
            return Err(Error::Stale);
        }
    }
    for operation in &operations {
        if let FileOperation::Create { path, .. } = operation {
            check_target(&project, path)?;
            if !matches!(project.stat(path), Err(Error::NotFound(_))) {
                return Err(Error::Stale);
            }
        }
    }
    Ok(FileAction {
        title: action.title,
        operations,
        edits,
        open: Some(doc.path.clone()),
        ..Default::default()
    })
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn setup() -> (MockHost, Index, Document) {
        let mock = MockHost::install();
        let source = "<?php namespace App\\Models;use Illuminate\\Database\\Eloquent\\Model;/** Handwritten Post documentation. */class Post extends Model {}";
        mock.add_file("app/Models/Post.php", source);
        mock.add_file("app/Models/User.php","<?php namespace App\\Models;use Illuminate\\Database\\Eloquent\\Model;/** Handwritten User documentation. */class User extends Model {}");
        mock.add_file("database/migrations/001.php","<?php Schema::create('posts',function($t){$t->string('title');});Schema::create('users',function($t){$t->string('email');});");
        let project = host::projects().unwrap().remove(0);
        let index = Index::build(&project);
        (
            mock,
            index,
            Document {
                path: "/project/app/Models/Post.php".into(),
                language: "php".into(),
                version: 1,
                text: source.into(),
            },
        )
    }
    fn lazy(index: &Index, doc: &Document) -> FileAction {
        actions(index, doc)
            .into_iter()
            .find(|a| a.data.as_deref() == Some("model-native:helpers-all:1"))
            .unwrap()
    }
    fn answers(mock: &MockHost, mode: &str) {
        mock.answer_next_prompt(Some(&["App\\Models\\Post", "App\\Models\\User"]));
        mock.answer_next_prompt(Some(&[mode]));
        mock.answer_next_prompt(Some(&[]));
    }
    fn export(label: &str, text: &str) {
        if let Ok(dir) = std::env::var("LARAVEL_HELPER_FIXTURES_DIR") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                std::path::Path::new(&dir).join(format!("{label}.php")),
                text,
            )
            .unwrap();
        }
    }
    #[test]
    fn reviewed_batch_creates_helpers_and_both_doc_modes_as_one_transaction() {
        for mode in ["helpers", "preserve", "overwrite"] {
            let (mock, index, doc) = setup();
            answers(&mock, mode);
            for _ in 0..3 {
                mock.answer_next_prompt(Some(&["yes"]));
            }
            let result = resolve(&index, &doc, lazy(&index, &doc)).unwrap();
            assert_eq!(result.operations.len() + result.edits.len(), 2);
            if mode == "helpers" {
                for (i, op) in result.operations.iter().enumerate() {
                    let FileOperation::Create {
                        text, overwrite, ..
                    } = op
                    else {
                        panic!()
                    };
                    assert!(!overwrite);
                    export(&format!("helpers_{i}"), text);
                }
            } else {
                assert!(result.operations.is_empty());
                for (i, (_, edits)) in result.edits.iter().enumerate() {
                    let text = &edits[0].text;
                    assert_eq!(text.contains("Handwritten"), mode == "preserve");
                    assert!(text.contains("@property string"));
                    export(&format!("{mode}_{i}"), text);
                }
            }
            assert!(
                !host::projects()
                    .unwrap()
                    .remove(0)
                    .exists(".openidea/helpers/App_Models_Post.php")
                    .unwrap()
            );
            assert!(mock.process_runs().is_empty());
        }
    }
    #[test]
    fn cancellation_after_first_review_returns_no_partial_transaction() {
        let (mock, index, doc) = setup();
        answers(&mock, "helpers");
        mock.answer_next_prompt(Some(&["yes"]));
        mock.answer_next_prompt(None);
        assert_eq!(
            resolve(&index, &doc, lazy(&index, &doc)),
            Err(Error::Cancelled)
        );
        assert!(
            !host::projects()
                .unwrap()
                .remove(0)
                .exists(".openidea/helpers/App_Models_Post.php")
                .unwrap()
        );
        assert!(mock.process_runs().is_empty());
    }
    #[test]
    fn batch_refuses_existing_unowned_helper_or_another_models_marker() {
        for existing in [
            "<?php // user file",
            "<?php\n// Generated by OpenIDEA Laravel. Do not edit this helper.\nnamespace Other;class Model {}",
        ] {
            let (mock, index, doc) = setup();
            mock.add_file(".openidea/helpers/App_Models_Post.php", existing);
            answers(&mock, "helpers");
            assert!(resolve(&index, &doc, lazy(&index, &doc)).is_err());
        }
    }
    #[test]
    fn preserve_mode_does_not_add_conflicting_handwritten_property_or_method_annotations() {
        let source = "<?php\n/**\n * User notes.\n * @property int $title\n * @method static string active(bool $enabled)\n */\nclass Post extends Model {}";
        let tags = " * @property string $title\n * @property string $other\n * @method static object active()\n";
        let updated = replace_docs(source, tags).unwrap();
        assert_eq!(updated.matches("$title").count(), 1);
        assert_eq!(updated.matches("active(").count(), 1);
        assert!(updated.contains("@property int $title"));
        assert!(updated.contains("@property string $other"));
        assert_eq!(replace_docs(&updated, tags).unwrap(), updated);
    }
}
