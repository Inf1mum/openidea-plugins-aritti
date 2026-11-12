//! Bounded, declared related-class/accessor facts. Filename similarity is never
//! a model association, and every destination stays owned by the project.
use super::*;
#[path="eloquent_connection_proof.rs"]
mod connection_proof;
pub(crate) use connection_proof::{ConnectionSelection,connection_selection,cache_connection_overrides,PrimaryKey,primary_key};
use openidea_plugin::Project;
#[derive(Clone, Debug)]
pub(crate) struct Related {
    pub model: String,
    pub kind: String,
    pub label: String,
    pub entry: Entry,
    pub spelling: String,
}
#[derive(Clone, Debug)]
pub(crate) struct Accessor {
    pub name: String,
    pub value_type: String,
    pub entry: Entry,
    pub spelling: String,
}
fn owned(project: &Project, path: &str) -> bool {
    crate::owns_current_path(project, path)
}
fn read(project: &Project, path: &str) -> Option<String> {
    if !owned(project, path) {
        return None;
    }
    project
        .read_file(path)
        .ok()
        .filter(|source| source.len() <= MAX_TEXT)
}
type ParsedModels = std::cell::RefCell<BTreeMap<String, Option<(String, String, bool)>>>;

fn subclass(index: &Index, full: &str, target: &str, project: &Project, parsed: &ParsedModels) -> bool {
    let mut name = full;
    let mut seen = BTreeSet::new();
    for _ in 0..8 {
        if name.eq_ignore_ascii_case(target) {
            return true;
        }
        if !seen.insert(name.to_ascii_lowercase()) {
            return false;
        }
        let Some(model) = index
            .eloquent_models
            .get(&name.to_ascii_lowercase())
            .filter(|m| !m.ambiguous && !m.is_trait && owned(project, &m.path))
        else {
            return false;
        };
        let current = parsed
            .borrow_mut()
            .entry(model.path.clone())
            .or_insert_with(|| {
                read(project, &model.path)
                    .and_then(|source| super::model(&source, &model.path))
                    .map(|current| (current.full, current.parent, current.is_trait))
            })
            .clone();
        let Some((current_full, current_parent, current_is_trait)) = current else {
            return false;
        };
        if current_full != model.full || current_parent != model.parent || current_is_trait {
            return false;
        }
        name = &model.parent;
    }
    false
}
pub(crate) fn declared_model_class(source:&str)->Option<String>{model_property(source)}
fn model_property(text: &str) -> Option<String> {
    let source = Source::new(text)?;
    let names = Names::parse(&source)?;
    let (_, _, body, _) = names.class.as_ref()?;
    let fields = source
        .tokens
        .iter()
        .enumerate()
        .filter(|(at, t)| t.value == "$model" && source.paths[*at] == vec![*body])
        .collect::<Vec<_>>();
    let [(at, _)] = fields.as_slice() else {
        return None;
    };
    if !source.is(at + 1, "=")
        || !source.is(at + 3, "::")
        || !source.is(at + 4, "class")
        || !source.is(at + 5, ";")
    {
        return None;
    }
    let token = source.tokens.get(at + 2)?;
    if token.kind != Kind::Name {
        return None;
    }
    Some(names.resolve(token.value))
}
fn documentation_model(text: &str) -> Option<String> {
    let source = Source::new(text)?;
    let names = Names::parse(&source)?;
    let (start, _) = super::class_layout(text)?;
    let prefix = text[..start].trim_end();
    if !prefix.ends_with("*/") {
        return None;
    }
    let begin = prefix.rfind("/**")?;
    let doc = &prefix[begin + 3..prefix.len() - 2];
    let mut models = BTreeSet::new();
    for line in doc.lines() {
        if let Some((_, tail)) = line.split_once("@mixin") {
            if let Some(value) = tail
                .split_whitespace()
                .next()
                .filter(|name| crate::generator_plan::qualified(name))
            {
                models.insert(names.resolve(value));
            }
        }
        if let Some((_, tail)) = line.split_once("@extends") {
            let declaration = tail.trim();
            if let Some((base, tail)) = declaration.split_once('<') {
                if names.resolve(base.trim()) == "Illuminate\\Http\\Resources\\Json\\JsonResource" {
                    if let Some((value, _)) = tail.split_once('>') {
                        let value = value.trim();
                        if crate::generator_plan::qualified(value) {
                            models.insert(names.resolve(value));
                        }
                    }
                }
            }
        }
    }
    (models.len() == 1).then(|| models.into_iter().next().unwrap())
}
fn factory_attribute(text: &str) -> Vec<String> {
    let Some(source) = Source::new(text) else {
        return vec![];
    };
    let Some(names) = Names::parse(&source) else {
        return vec![];
    };
    let Some((start, _)) = super::class_layout(text) else {
        return vec![];
    };
    let Some((_, class, _, _)) = names.class.as_ref() else {
        return vec![];
    };
    let mut out = Vec::new();
    for (at, token) in source
        .tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| t.start >= start && t.start < source.tokens[*class].start)
    {
        if names.resolve(token.value) != "Illuminate\\Database\\Eloquent\\Attributes\\UseFactory"
            || !source.is(at + 1, "(")
        {
            continue;
        }
        let Some(bracket) = source.paths[at]
            .last()
            .copied()
            .filter(|open| *open > 0 && source.is(*open, "[") && source.is(*open - 1, "#"))
        else {
            continue;
        };
        let Some(close) = source.pairs.get(&(at + 1)) else {
            continue;
        };
        if *close > source.pairs.get(&bracket).copied().unwrap_or(0) {
            continue;
        }
        let Some(value) = source.tokens.get(at + 2).filter(|t| t.kind == Kind::Name) else {
            continue;
        };
        if source.is(at + 3, "::") && source.is(at + 4, "class") && *close == at + 5 {
            out.push(names.resolve(value.value));
        }
    }
    out
}
pub(crate) fn inventory(index: &Index, project: &Project) -> Vec<Related> {
    let parsed = ParsedModels::default();
    let models = model_inventory(&index.eloquent_models)
        .into_iter()
        .filter(|(_, path, _)| owned(project, path))
        .collect::<Vec<_>>();
    let full_models = models
        .iter()
        .map(|(full, _, _)| full.as_str())
        .collect::<BTreeSet<_>>();
    let mut paths = index
        .eloquent_models
        .values()
        .filter(|model| !model.ambiguous && !model.is_trait && owned(project, &model.path))
        .map(|model| model.path.clone())
        .collect::<BTreeSet<_>>();
    paths.extend(
        index
            .navigation
            .keys()
            .filter(|path| path.contains("/migrations/") && owned(project, path))
            .cloned(),
    );
    let mut source_by_path = BTreeMap::new();
    let mut total = 0;
    for path in paths.into_iter().take(512) {
        if let Some(text) = read(project, &path) {
            total += text.len();
            if total > 4 * 1024 * 1024 {
                break;
            }
            source_by_path.insert(path, text);
        }
    }
    let classes = source_by_path
        .iter()
        .filter_map(|(path, text)| {
            let source = Source::new(text)?;
            let names = Names::parse(&source)?;
            let (full, at, _, _) = names.class.as_ref()?;
            Some((full.clone(), (path.clone(), entry(&source, path, at + 1))))
        })
        .collect::<BTreeMap<_, _>>();
    let mut out = Vec::new();
    for (path, text) in &source_by_path {
        let full = declaration(text);
        if let Some(full) = full.as_ref() {
            let (kind, model) = if subclass(
                index,
                full,
                "Illuminate\\Database\\Eloquent\\Factories\\Factory",
                project,
                &parsed,
            ) {
                ("Factory", model_property(text))
            } else if subclass(
                index,
                full,
                "Illuminate\\Http\\Resources\\Json\\JsonResource",
                project,
                &parsed,
            ) {
                ("Resource", documentation_model(text))
            } else {
                ("", None)
            };
            if let Some(model) = model.filter(|model| full_models.contains(model.as_str())) {
                if let Some((_, location)) = classes.get(full) {
                    out.push(Related {
                        model,
                        kind: kind.into(),
                        label: full.clone(),
                        entry: location.clone(),
                        spelling: full.rsplit('\\').next().unwrap_or(full).into(),
                    });
                }
            }
        }
        if path.contains("/migrations/") {
            let Some(source) = Source::new(text) else {
                continue;
            };
            let Some(names) = Names::parse(&source) else {
                continue;
            };
            let schema = names.resolve("Schema");
            if schema != "Schema" && schema != "Illuminate\\Support\\Facades\\Schema" {
                continue;
            }
            let events = crate::migrations::events(text);
            let tables = events
                .into_iter()
                .filter_map(|event| match event {
                    crate::migrations::Event::Create(table)
                    | crate::migrations::Event::DropTable(table)
                    | crate::migrations::Event::DropColumn(table, _)
                    | crate::migrations::Event::RenameColumn { table, .. } => Some(table),
                    crate::migrations::Event::Add(column)
                    | crate::migrations::Event::Change(column) => Some(column.table),
                    crate::migrations::Event::RenameTable(_, to) => Some(to),
                })
                .collect::<BTreeSet<_>>();
            for (model, _, table) in &models {
                if tables.contains(table) {
                    out.push(Related {
                        model: model.clone(),
                        kind: "Migration".into(),
                        label: path.rsplit('/').next().unwrap_or(path).into(),
                        entry: Entry {
                            path: path.clone(),
                            at: Position::new(0, 0),
                            width: 0,
                            detail: Some("literal schema table".into()),
                        },
                        spelling: String::new(),
                    });
                }
            }
        }
    }
    for (model, path, _) in &models {
        let Some(source) = source_by_path.get(path) else {
            continue;
        };
        for factory in factory_attribute(source) {
            if let Some((path, location)) = classes.get(&factory) {
                if owned(project, path)
                    && subclass(
                        index,
                        &factory,
                        "Illuminate\\Database\\Eloquent\\Factories\\Factory",
                        project,
                        &parsed,
                    )
                {
                    out.push(Related {
                        model: model.clone(),
                        kind: "Factory".into(),
                        label: factory.clone(),
                        entry: location.clone(),
                        spelling: factory.rsplit('\\').next().unwrap_or(&factory).into(),
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| (&a.model, &a.kind, &a.entry.path).cmp(&(&b.model, &b.kind, &b.entry.path)));
    out.dedup_by(|a, b| a.model == b.model && a.kind == b.kind && a.entry.path == b.entry.path);
    out
}

pub(crate) fn accessors(index: &Index, project: &Project, full: &str) -> Vec<Accessor> {
    // UI/generation callers may read sources. Rebuild only the bounded current
    // ancestry/trait closure so changed accessor bodies cannot leak cached types.
    fn collect(index:&Index,project:&Project,full:&str,out:&mut Models,active:&mut BTreeSet<String>,depth:u8)->Option<()>{
        if full.eq_ignore_ascii_case("Illuminate\\Database\\Eloquent\\Model"){return Some(());}
        if depth>8||out.len()>32{return None;}
        let key=full.to_ascii_lowercase();if out.contains_key(&key){return Some(());}if !active.insert(key.clone()){return None;}
        let previous=index.eloquent_models.get(&key).filter(|model|!model.ambiguous&&owned(project,&model.path))?;
        let text=read(project,&previous.path)?;let current=super::model(&text,&previous.path)?;
        if !current.full.eq_ignore_ascii_case(full)||current.is_trait!=previous.is_trait{return None;}
        if !current.is_trait{collect(index,project,&current.parent,out,active,depth+1)?;}
        for trait_ in &current.traits{
            if index.eloquent_models.contains_key(&trait_.to_ascii_lowercase()){collect(index,project,trait_,out,active,depth+1)?;}
        }
        out.insert(key.clone(),current);active.remove(&key);Some(())
    }
    let mut current=Index::default();
    if collect(index,project,full,&mut current.eloquent_models,&mut BTreeSet::new(),0).is_none(){return vec![];}
    super::finish(&mut current.eloquent_models);
    semantic::accessor_facts(&current,full,&|path|owned(project,path)).unwrap_or_default().into_iter().filter_map(|fact|{
        if fact.has_getter==Some(false){return None;}
        Some(Accessor{name:fact.name,value_type:fact.read_type?,entry:fact.getter_entry.unwrap_or(fact.entry),spelling:fact.method_name})
    }).collect()
}

/// A source-proven explicit connection only. Absence is unknown: the configured
/// default and runtime tenant selection are not inferred from a model filename.
pub(crate) fn connection(index:&Index,project:&Project,full:&str)->Option<String>{
    match connection_selection(index,project,full)?{ConnectionSelection::Named(name)=>Some(name),ConnectionSelection::ConfiguredDefault=>None}
}

// Require a real attached PHPDoc token, excluding comment-looking data.
fn resource_doc(text: &str, start: usize) -> Option<(usize, usize)> {
    let prefix = text.get(..start)?.trim_end();
    let bytes = prefix.as_bytes();
    let mut at = 0;
    let mut last = None;
    while at < bytes.len() {
        match bytes[at] {
            b'\'' | b'"' | b'`' => at = php::string_end(bytes, at),
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                at = prefix[at..].find('\n').map_or(bytes.len(), |end| at + end)
            }
            b'#' if bytes.get(at + 1) != Some(&b'[') => {
                at = prefix[at..].find('\n').map_or(bytes.len(), |end| at + end)
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                let end = prefix[at + 2..].find("*/").map(|end| at + end + 4)?;
                if bytes.get(at + 2) == Some(&b'*') {
                    last = Some((at, end));
                }
                at = end;
            }
            _ => at += 1,
        }
    }
    last.filter(|(_, end)| *end == bytes.len())
}
/// Readable model fields for resource generation, without treating virtual
/// accessor/appended/PHPDoc values as writable factory columns.
#[derive(Clone, Debug)]
pub(crate) struct ResourceField {
    pub name: String,
    pub entry: Entry,
    pub kind: String,
}
pub(crate) fn resource_fields(index: &Index, project: &Project, full: &str) -> Vec<ResourceField> {
    let mut result = accessors(index, project, full)
        .into_iter()
        .map(|fact| ResourceField {
            name: fact.name,
            entry: fact.entry,
            kind: format!("Accessor · {}", fact.value_type),
        })
        .collect::<Vec<_>>();
    if let Some(facts) = helper_facts::facts(index, full, &|path| owned(project, path)) {
        result.extend(facts.casts.into_iter().map(|fact| ResourceField {
            name: fact.name,
            entry: fact.entry,
            kind: format!("Cast · {}", fact.label),
        }));
    }
    let doc = Document {
        path: String::new(),
        language: "php".into(),
        version: 0,
        text: format!("<?php \\{full}::query()->"),
    };
    let Some(context) = context(&doc, text::position(&doc.text, doc.text.len())) else {
        return result;
    };
    let resolver = Resolver {
        context: &context,
        index,
        overlay: None,
        object_overlay:None,
        path: "",
        owns: &|path| owned(project, path),
        owner_cache: RefCell::new(BTreeMap::new()),
    };
    let Some(lineage) = resolver.lineage(full) else {
        return result;
    };
    for model in lineage {
        for path in std::iter::once(&model.path).chain(model.trait_sources.iter()) {
            let Some(text) = read(project, path) else {
                continue;
            };
            let Some(source) = Source::new(&text) else {
                continue;
            };
            let Some(names) = Names::parse(&source) else {
                continue;
            };
            let Some((declared, _, body, parent)) = names.class.as_ref() else {
                continue;
            };
            if path == &model.path && (declared != &model.full || parent != &model.parent) {
                continue;
            }
            let Some((start, _)) = super::class_layout(&text) else {
                continue;
            };
            if let Some((begin, end)) = resource_doc(&text, start) {
                for line in text[begin + 3..end - 2].lines() {
                    let words = line
                        .trim()
                        .trim_start_matches('*')
                        .split_whitespace()
                        .collect::<Vec<_>>();
                    if !words
                        .first()
                        .is_some_and(|word| matches!(*word, "@property" | "@property-read"))
                    {
                        continue;
                    }
                    let Some(name) = words
                        .iter()
                        .skip(1)
                        .find_map(|word| word.strip_prefix('$'))
                        .filter(|name| crate::generator_plan::identifier(name))
                    else {
                        continue;
                    };
                    result.push(ResourceField {
                        name: name.into(),
                        entry: Entry {
                            path: path.clone(),
                            at: text::position(&text, begin),
                            width: 0,
                            detail: None,
                        },
                        kind: "PHPDoc property".into(),
                    });
                }
            }
            let properties = source
                .tokens
                .iter()
                .enumerate()
                .filter(|(at, t)| {
                    t.value == "$appends" && t.kind == Kind::Var && source.paths[*at] == vec![*body]
                })
                .collect::<Vec<_>>();
            let [(at, _)] = properties.as_slice() else {
                continue;
            };
            if !source.is(at + 1, "=") || !source.is(at + 2, "[") {
                continue;
            }
            let Some(close) = source
                .pairs
                .get(&(at + 2))
                .copied()
                .filter(|close| source.is(close + 1, ";"))
            else {
                continue;
            };
            let tokens = &source.tokens[at + 3..close];
            let mut expect_value = true;
            let mut valid = true;
            for token in tokens {
                if expect_value && token.kind == Kind::String {
                    expect_value = false;
                } else if !expect_value && token.value == "," {
                    expect_value = true;
                } else {
                    valid = false;
                    break;
                }
            }
            if !valid {
                continue;
            }
            for token in tokens.iter().filter(|token| token.kind == Kind::String) {
                let Some((name, _)) = php::literal(&text, token.start..token.end)
                    .filter(|(name, _)| crate::generator_plan::identifier(name))
                else {
                    continue;
                };
                result.push(ResourceField {
                    name: name.into(),
                    entry: Entry {
                        path: path.clone(),
                        at: text::position(&text, token.start),
                        width: 0,
                        detail: None,
                    },
                    kind: "Declared appends".into(),
                });
            }
        }
    }
    result.retain(|fact| {
        owned(project, &fact.entry.path) && crate::generator_plan::identifier(&fact.name)
    });
    result.sort_by(|a, b| (&a.name, &a.entry.path).cmp(&(&b.name, &b.entry.path)));
    result.dedup_by(|a, b| a.name == b.name);
    result
}

#[cfg(test)]mod modern_accessor_tests{
    use super::*;
    #[test]fn model_overview_accessor_facts_use_current_modern_and_inherited_contracts(){
        use openidea_plugin::{host,mock::MockHost};let mock=MockHost::install();mock.add_file("artisan","<?php");
        mock.add_file("app/Models/Base.php",r#"<?php namespace App\Models;class Base extends \Illuminate\Database\Eloquent\Model{public function getAliasAttribute():string{return 'base';}}"#);
        let source=r#"<?php namespace App\Models;use Illuminate\Database\Eloquent\Casts\Attribute;class Post extends Base{protected function title():Attribute{return Attribute::get(fn():?string=>null);}protected function secret():Attribute{return Attribute::set(fn(string $value)=>$value);}}"#;
        mock.add_file("app/Models/Post.php",source);let project=host::projects().unwrap().remove(0);let index=Index::build(&project);
        let facts=accessors(&index,&project,"App\\Models\\Post");assert_eq!(facts.len(),2);assert!(facts.iter().any(|a|a.name=="title"&&a.value_type=="?string"));assert!(facts.iter().any(|a|a.name=="alias"&&a.value_type=="string"&&a.entry.path.ends_with("Base.php")));
        mock.open_editor("/project/app/Models/Post.php","php",&source.replace("fn():?string=>null","fn():int=>1"),&[]);
        assert!(accessors(&index,&project,"App\\Models\\Post").iter().any(|a|a.name=="title"&&a.value_type=="int"));
        mock.add_project("/project/app");assert!(accessors(&index,&project,"App\\Models\\Post").is_empty());
    }
}
