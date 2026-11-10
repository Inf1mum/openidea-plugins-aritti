//! Bounded, declared related-class/accessor facts. Filename similarity is never
//! a model association, and every destination stays owned by the project.
use super::*;
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
fn subclass(index: &Index, full: &str, target: &str, project: &Project) -> bool {
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
        let Some(current) =
            read(project, &model.path).and_then(|source| super::model(&source, &model.path))
        else {
            return false;
        };
        if current.full != model.full || current.parent != model.parent || current.is_trait {
            return false;
        }
        name = &model.parent;
    }
    false
}
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
            ) {
                ("Factory", model_property(text))
            } else if subclass(
                index,
                full,
                "Illuminate\\Http\\Resources\\Json\\JsonResource",
                project,
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
    let doc = Document {
        path: String::new(),
        language: "php".into(),
        version: 0,
        text: format!("<?php \\{full}::query()->"),
    };
    let Some(context) = context(&doc, text::position(&doc.text, doc.text.len())) else {
        return vec![];
    };
    let resolver = Resolver {
        context: &context,
        index,
        overlay: None,
        path: "",
        owns: &|path| owned(project, path),
        owner_cache: RefCell::new(BTreeMap::new()),
    };
    let Some(lineage) = resolver.lineage(full) else {
        return vec![];
    };
    let mut blocked = BTreeSet::new();
    let mut result = Vec::new();
    for model in lineage {
        let mut candidates = BTreeMap::<String, Vec<Accessor>>::new();
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
            let Some((_, _, body, _)) = names.class.as_ref() else {
                continue;
            };
            for function in source
                .functions()
                .into_iter()
                .filter(|f| source.paths[f.start] == vec![*body])
            {
                let Some(at) = function.name else { continue };
                let method = source.tokens[at].value;
                if blocked.contains(method) || !model.declared.contains(method) {
                    continue;
                }
                let Some(property) = method
                    .strip_prefix("get")
                    .and_then(|name| name.strip_suffix("Attribute"))
                    .filter(|name| !name.is_empty() && name.starts_with(char::is_uppercase))
                else {
                    continue;
                };
                let previous = (0..function.start)
                    .rev()
                    .find(|at| matches!(source.tokens[*at].value, "{" | "}" | ";"))
                    .map_or(0, |at| at + 1);
                if source.tokens[previous..function.start]
                    .iter()
                    .any(|token| token.value == "private" || token.value == "static")
                {
                    continue;
                }
                if !source.is(function.params.end + 1, ":") {
                    continue;
                }
                let tokens = source.tokens[function.params.end + 2..function.body]
                    .iter()
                    .collect::<Vec<_>>();
                let Some(value_type) = super::helper_facts::declared_type(
                    &tokens,
                    &names,
                    full,
                    source.is(names.class.as_ref().unwrap().1, "trait"),
                ) else {
                    continue;
                };
                if matches!(value_type.as_str(), "void" | "never") {
                    continue;
                }
                candidates.entry(method.into()).or_default().push(Accessor {
                    name: crate::index::separated(property, '_'),
                    value_type,
                    entry: entry(&source, path, at),
                    spelling: method.into(),
                });
            }
        }
        for (_, mut entries) in candidates {
            entries.sort_by(|a, b| a.entry.path.cmp(&b.entry.path));
            entries.dedup_by(|a, b| a.entry.path == b.entry.path && a.entry.at == b.entry.at);
            if entries.len() == 1 {
                result.push(entries.remove(0));
            }
        }
        blocked.extend(model.declared.iter().cloned());
    }
    result
}

/// A source-proven explicit connection only. Absence is unknown: the configured
/// default and runtime tenant selection are not inferred from a model filename.
pub(crate) fn connection(index: &Index, project: &Project, full: &str) -> Option<String> {
    let doc = Document {
        path: String::new(),
        language: "php".into(),
        version: 0,
        text: format!("<?php \\{full}::query()->"),
    };
    let context = context(&doc, text::position(&doc.text, doc.text.len()))?;
    let resolver = Resolver {
        context: &context,
        index,
        overlay: None,
        path: "",
        owns: &|path| owned(project, path),
        owner_cache: RefCell::new(BTreeMap::new()),
    };
    let mut connection = None;
    for model in resolver.lineage(full)? {
        for path in std::iter::once(&model.path).chain(model.trait_sources.iter()) {
            let text = read(project, path)?;
            let source = Source::new(&text)?;
            let names = Names::parse(&source)?;
            let (declared, _, body, parent) = names.class.as_ref()?;
            if path == &model.path {
                if declared != &model.full || parent != &model.parent {
                    return None;
                }
                let current = super::model(&text, path)?;
                if current.traits != model.traits
                    || current.explicit_table != model.explicit_table
                    || (index.metadata.model_table(&model.full).is_none()
                        && current.table != model.table)
                {
                    return None;
                }
            }
            // These can select a connection at runtime; their behavior is outside
            // this deliberately literal source subset.
            if source.tokens.iter().any(|t| {
                matches!(
                    t.value.to_ascii_lowercase().as_str(),
                    "getconnectionname" | "getconnection" | "setconnection" | "__construct"
                )
            }) {
                return None;
            }
            if source
                .tokens
                .windows(3)
                .any(|t| t[0].value == "$this" && t[1].value == "->" && t[2].value == "connection")
            {
                return None;
            }
            let fields = source
                .tokens
                .iter()
                .enumerate()
                .filter(|(at, t)| t.value == "$connection" && source.paths[*at] == vec![*body])
                .collect::<Vec<_>>();
            if fields.is_empty() {
                continue;
            }
            // Trait property conflicts/adaptations cannot establish a unique model value.
            if model.is_trait || path != &model.path {
                return None;
            }
            let [(at, _)] = fields.as_slice() else {
                return None;
            };
            if !source.is(at + 1, "=") || !source.is(at + 3, ";") {
                return None;
            }
            let value = source
                .tokens
                .get(at + 2)
                .filter(|t| t.kind == Kind::String)?;
            let (value, _) = php::literal(&text, value.start..value.end)?;
            if value.is_empty()
                || value.chars().count() > 128
                || value.chars().any(char::is_control)
            {
                return None;
            }
            if connection.is_none() {
                connection = Some(value.to_owned());
            }
        }
    }
    connection
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
