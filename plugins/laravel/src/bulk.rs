//! Work on many files at once, and generators that write their own files: models picked in the
//! Project view, IDE helper PHPDoc removal for all models, and factory and resource bodies filled
//! from the migration columns. Generators use native project writes; bulk edits use one undoable FileAction.

use std::path::Path;

use openidea_plugin::{Error, Invocation, NotificationLevel, Project, host};

use crate::index::{Index, Names};
use crate::unused;

const SOURCE: &str = "Laravel";

pub fn write_file(project: &Project, relative: &str, content: &str) -> Result<(), Error> {
    let root = project.root()?;
    let clean = !relative.is_empty() && !relative.starts_with('/') && Path::new(relative).components().all(|c| matches!(c, std::path::Component::Normal(_)));
    if !clean || relative.contains(['\n', '\0']) {
        return Err(Error::Invalid(format!("{relative:?} is not a path inside the project")));
    }

    let path = format!("{}/{relative}", root.trim_end_matches('/'));
    if host::project_of(&path).and_then(|p| p.root().ok()).as_deref() != Some(&root) {
        return Err(Error::Stale);
    }
    project.write_file(relative, content)
}

/// Model class names of the model files among the selected Project view entries (a folder selects the models below it).
pub fn selected_models(index: &Index, selected: &[String]) -> Vec<(String, String)> {
    index
        .model_files
        .keys()
        .filter(|path| selected.iter().any(|s| path.as_str() == s || path.starts_with(&format!("{}/", s.trim_end_matches('/')))))
        .filter_map(|path| {
            let model = Path::new(path).file_stem()?.to_str()?.to_owned();
            index.models.contains_key(&model).then(|| (model, path.clone()))
        })
        .collect()
}

pub fn remove_phpdocs(_ctx: &Invocation, _index: Option<&Index>) -> Result<Option<String>, Error> {
    host::notify(NotificationLevel::Info, SOURCE, "Open a model and choose ‘Remove IDE helper PHPDoc from indexed models’ from editor actions. The native workspace edit is atomic and undoable.");
    Ok(None)
}

fn cleanup_plan(main: &crate::Main, doc: &openidea_plugin::providers::Document) -> Result<Vec<(String, String, String)>, Error> {
    let (project, root) = crate::view_creation::owner(doc, None)?;
    let index = main.indexes.get(&root).filter(|i| !i.partial).ok_or(Error::Stale)?;
    if !index.model_files.contains_key(&doc.path) || project.read_file(&doc.path)? != doc.text || index.model_files.len() > 256 { return Err(Error::Stale); }
    let mut plan = Vec::new();
    let mut bytes = 0usize;
    for path in index.model_files.keys() {
        if host::project_of(path).and_then(|p|p.root().ok()).as_deref() != Some(&root) { return Err(Error::Stale); }
        let text = project.read_file(path)?;
        bytes += text.len();
        if text.len() > 128 << 10 || bytes > 4 << 20 { return Err(Error::Stale); }
        if let Some((range, replacement)) = text.match_indices("class ").find_map(|(at,_)| unused::remove_model_phpdocs(&text,at)) {
            let mut cleaned = text.clone();
            cleaned.replace_range(range, &replacement);
            plan.push((path.clone(), text, cleaned));
        }
    }
    Ok(plan)
}

pub fn actions(main: &crate::Main, doc: &openidea_plugin::providers::Document) -> Vec<openidea_plugin::file_actions::FileAction> {
    let admitted = crate::view_creation::owner(doc,None).ok().and_then(|(_,root)| main.indexes.get(&root)).is_some_and(|index| !index.partial && index.model_files.len() <= 256 && index.model_files.contains_key(&doc.path));
    if admitted && doc.text.len() <= 128 << 10 && doc.text.match_indices("class ").any(|(at,_)| unused::remove_model_phpdocs(&doc.text,at).is_some()) {
        vec![openidea_plugin::file_actions::FileAction { title: "Remove IDE helper PHPDoc from indexed models".into(), kind: Some("refactor.rewrite".into()), data: Some(format!("bulk-phpdocs:{}",doc.version)), ..Default::default() }]
    } else { Vec::new() }
}

pub fn resolve(main: &crate::Main, doc: &openidea_plugin::providers::Document, action: openidea_plugin::file_actions::FileAction) -> Result<openidea_plugin::file_actions::FileAction, Error> {
    if action.data.as_deref() != Some(&format!("bulk-phpdocs:{}",doc.version)) { return Err(Error::Stale); }
    let plan = cleanup_plan(main,doc)?;
    if plan.is_empty() { return Err(Error::Stale); }
    let preview = plan.iter().map(|(path,_,_)|path.as_str()).collect::<Vec<_>>().join("\n");
    if !ui_kit::prompts::confirm_exact("Remove IDE helper PHPDoc?", &format!("Remove helper tags from these {} indexed model files in one undoable workspace edit:\n{preview}",plan.len()))? { return Err(Error::Cancelled); }
    if cleanup_plan(main,doc)? != plan { return Err(Error::Stale); }
    Ok(openidea_plugin::file_actions::FileAction {
        title: action.title,
        edits: plan.into_iter().map(|(path,text,cleaned)| (path,vec![openidea_plugin::TextEdit { range: openidea_plugin::Range::new(openidea_plugin::Position::new(0,0),openidea_plugin::text::position(&text,text.len())),text:cleaned }])).collect(),
        ..Default::default()
    })
}

fn faker(column: &str, kind: &str) -> Option<String> {
    let by_name = match column {
        "id" | "created_at" | "updated_at" | "deleted_at" | "remember_token" => return None,
        "email" => Some("fake()->unique()->safeEmail()"),
        "name" => Some("fake()->name()"),
        "password" => Some("bcrypt('password')"),
        "slug" => Some("fake()->slug()"),
        "title" => Some("fake()->sentence()"),
        "phone" => Some("fake()->phoneNumber()"),
        "url" => Some("fake()->url()"),
        "email_verified_at" => Some("now()"),
        _ => None,
    };
    if let Some(value) = by_name {
        return Some(value.to_owned());
    }
    if column.ends_with("_id") {
        let model: String = column.trim_end_matches("_id").split('_').map(|w| w[..1].to_uppercase() + &w[1..]).collect();
        return Some(format!("{model}::factory()"));
    }

    let value = match kind {
        "string" | "char" => "fake()->word()",
        "text" | "tinyText" | "mediumText" | "longText" => "fake()->paragraph()",
        "integer" | "tinyInteger" | "smallInteger" | "mediumInteger" | "bigInteger" | "unsignedInteger" | "unsignedTinyInteger"
        | "unsignedSmallInteger" | "unsignedMediumInteger" | "unsignedBigInteger" => "fake()->numberBetween(1, 1000)",
        "float" | "double" | "decimal" | "unsignedDecimal" => "fake()->randomFloat(2, 1, 1000)",
        "boolean" => "fake()->boolean()",
        "date" => "fake()->date()",
        "dateTime" | "dateTimeTz" | "timestamp" | "timestampTz" => "fake()->dateTime()",
        "time" | "timeTz" => "fake()->time()",
        "uuid" => "fake()->uuid()",
        "ulid" => "(string) \\Illuminate\\Support\\Str::ulid()",
        "ipAddress" => "fake()->ipv4()",
        "json" | "jsonb" => "[]",
        _ => return None,
    };
    Some(value.to_owned())
}

fn columns_of<'a>(index: &'a Index, model: &str) -> Option<&'a Names> {
    index.columns.get(index.models.get(model)?)
}

pub fn factory_source(index: &Index, model: &str, name: &str) -> String {
    let rows: String = columns_of(index, model)
        .into_iter()
        .flatten()
        .filter_map(|(column, entry)| Some(format!("            '{column}' => {},\n", faker(column, entry.detail.as_deref()?)?)))
        .collect();
    let imports: String = columns_of(index, model)
        .into_iter()
        .flatten()
        .filter(|(column, _)| column.ends_with("_id") && column.len() > 3)
        .filter_map(|(column, _)| {
            let related: String = column.trim_end_matches("_id").split('_').map(|w| w[..1].to_uppercase() + &w[1..]).collect();
            index.models.contains_key(&related).then(|| format!("use App\\Models\\{related};\n"))
        })
        .collect();

    format!(
        "<?php\n\nnamespace Database\\Factories;\n\n{imports}use Illuminate\\Database\\Eloquent\\Factories\\Factory;\n\n/**\n * @extends Factory<\\App\\Models\\{model}>\n */\nclass {name} extends Factory\n{{\n    public function definition(): array\n    {{\n        return [\n{rows}        ];\n    }}\n}}\n"
    )
}

pub fn resource_source(index: &Index, model: &str, name: &str) -> String {
    let rows: String = columns_of(index, model)
        .into_iter()
        .flatten()
        .filter(|(column, _)| !matches!(column.as_str(), "password" | "remember_token"))
        .map(|(column, _)| format!("            '{column}' => $this->{column},\n"))
        .collect();

    format!(
        "<?php\n\nnamespace App\\Http\\Resources;\n\nuse Illuminate\\Http\\Request;\nuse Illuminate\\Http\\Resources\\Json\\JsonResource;\n\nclass {name} extends JsonResource\n{{\n    public function toArray(Request $request): array\n    {{\n        return [\n{rows}        ];\n    }}\n}}\n"
    )
}

/// Writes a factory or resource with the model's columns in it; `None` when the kind has no such body or the name is nested.
pub fn model_file(project: &Project, index: &Index, kind: &str, model: &str, name: &str) -> Result<Option<String>, Error> {
    let (directory, source) = match kind {
        "factory" => ("database/factories", factory_source(index, model, name)),
        "resource" => ("app/Http/Resources", resource_source(index, model, name)),
        _ => return Ok(None),
    };
    if name.contains(['/', '\\']) || columns_of(index, model).is_none_or(|columns| columns.is_empty()) {
        return Ok(None);
    }

    let relative = format!("{directory}/{name}.php");
    if project.exists(&relative)? {
        return Err(Error::Invalid(format!("{relative} already exists.")));
    }

    match write_file(project, &relative, &source) {
        Ok(()) => {
            let root = project.root()?;
            host::open_location(&format!("{}/{relative}", root.trim_end_matches('/')), 0, 0)?;
            Ok(Some(relative))
        }
        Err(Error::Denied(_)) => Ok(None),
        Err(error) => Err(error),
    }
}
