//! Reviewed insertion of one Spring Data derived query method into the invoking repository
//! interface. Only that document is edited. The entity comes only from a simple generic argument
//! that matches exactly one entity file named after it; conditions are plain direct scalar
//! attributes (relationship paths are out of scope).
//!
//! Grammar checked against the Spring Data reference (query-methods-details, Repository query
//! keywords, Repository query return types): subjects find/count/exists/delete, `Top`/`First`
//! limits, `Is`/`Equals`/no keyword, `Not`, `Like`, `StartingWith`, `EndingWith`, `Containing`,
//! `GreaterThan`, `LessThan`, `Between`, `In`, `IsNull`, `True`, `False`, `IgnoreCase`, `OrderBy`
//! with `Asc`/`Desc`, `Page`/`Slice` requiring `Pageable`, `delete` returning `void` or the count.
use super::*;
use crate::{
    model::{self, Relation, capitalize, closing, import_edit, parse_class, split_top_level},
    queries,
};

pub(super) const TITLE: &str = "Add query method with preview…";
pub(super) const OPTIONS_TITLE: &str = "Add query method with result and parameter options…";
pub(super) const DOMAIN: &str = "org.springframework.data.domain";
pub(super) const MAX_LIMIT: u32 = 10_000;

pub(super) struct Operator {
    pub(super) key: &'static str,
    pub(super) suffix: String,
    pub(super) label: &'static str,
}

fn op(key: &'static str, suffix: &str, label: &'static str) -> Operator {
    Operator { key, suffix: suffix.to_owned(), label }
}

/// Keyword spellings used when generating names. Every alternative is a spelling in the Spring Data
/// query predicate keyword table (query-keywords-reference): `find…By`/`read…By`/`get…By`/`query…By`/
/// `search…By` subjects, `Is`/`Equals`/no keyword, `Not`/`IsNot`, `True`/`IsTrue`, `Null`/`IsNull`,
/// `Containing`/`Contains`, `IgnoreCase`/`IgnoringCase`, `GreaterThan`/`After` and `LessThan`/`Before`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Keywords {
    pub(super) find: &'static str,
    equality: &'static str,
    negation: &'static str,
    boolean: &'static str,
    null: &'static str,
    contains: &'static str,
    ignore_case: &'static str,
    temporal_after_before: bool,
}

impl Keywords {
    pub(super) const DEFAULT: Self = Self {
        find: "find",
        equality: "",
        negation: "Not",
        boolean: "",
        null: "IsNull",
        contains: "Containing",
        ignore_case: "IgnoreCase",
        temporal_after_before: false,
    };

    pub(super) fn load() -> Result<Self, String> {
        let pick = |name: &str, allowed: &[&'static str]| -> Result<&'static str, String> {
            let Some(value) = host::setting_string(name) else {
                return Ok(allowed[0]);
            };
            allowed
                .iter()
                .copied()
                .find(|candidate| *candidate == value)
                .ok_or_else(|| format!("Unknown {name} `{value}`; use {}.", allowed.join(", ")))
        };
        let equality = pick("queryEqualityKeyword", &["none", "Is", "Equals"])?;
        let boolean = pick("queryBooleanStyle", &["True", "IsTrue"])?;
        Ok(Self {
            find: pick("queryFindPrefix", &["find", "get", "read", "query", "search"])?,
            equality: if equality == "none" { "" } else { equality },
            negation: pick("queryNegationKeyword", &["Not", "IsNot"])?,
            boolean: if boolean == "True" { "" } else { "Is" },
            null: pick("queryNullKeyword", &["IsNull", "Null"])?,
            contains: pick("queryContainsKeyword", &["Containing", "IsContaining", "Contains"])?,
            ignore_case: pick("queryIgnoreCaseKeyword", &["IgnoreCase", "IgnoringCase"])?,
            temporal_after_before: pick("queryDateKeywords", &["GreaterThanLessThan", "AfterBefore"])? == "AfterBefore",
        })
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum AttrKind {
    Text,
    Boolean,
    Comparable,
    Other,
}

#[derive(Clone)]
pub(super) struct Attr {
    pub(super) name: String,
    pub(super) ty: String,
    pub(super) kind: AttrKind,
    pub(super) nullable: bool,
    pub(super) import: Option<String>,
}

impl Attr {
    #[cfg_attr(not(test), allow(dead_code))]
    fn operators(&self) -> Vec<Operator> {
        self.operators_with(&Keywords::DEFAULT)
    }

    fn temporal(&self) -> bool {
        matches!(self.ty.as_str(), "LocalDate" | "LocalDateTime" | "LocalTime" | "Instant" | "OffsetDateTime" | "ZonedDateTime" | "Date")
    }

    pub(super) fn operators_with(&self, keywords: &Keywords) -> Vec<Operator> {
        let ignore = keywords.ignore_case;
        let contains = keywords.contains;
        let mut found = vec![
            op("is", keywords.equality, "Equals (no keyword)"),
            op("not", keywords.negation, "Not equal"),
        ];
        match self.kind {
            AttrKind::Text => found.extend([
                op("ignoreCase", ignore, "Equals ignoring case"),
                op("like", "Like", "Like (caller supplies wildcards)"),
                op("containing", contains, "Containing"),
                op("containingIgnoreCase", &format!("{contains}{ignore}"), "Containing ignoring case"),
                op("startingWith", "StartingWith", "Starting with"),
                op("startingWithIgnoreCase", &format!("StartingWith{ignore}"), "Starting with ignoring case"),
                op("endingWith", "EndingWith", "Ending with"),
                op("endingWithIgnoreCase", &format!("EndingWith{ignore}"), "Ending with ignoring case"),
            ]),
            AttrKind::Comparable => {
                let (greater, less) = if keywords.temporal_after_before && self.temporal() {
                    ("After", "Before")
                } else {
                    ("GreaterThan", "LessThan")
                };
                found.extend([
                    op("greaterThan", greater, "Greater than"),
                    op("lessThan", less, "Less than"),
                    op("between", "Between", "Between (two parameters)"),
                ]);
            }
            AttrKind::Boolean => found.extend([
                op("true", &format!("{}True", keywords.boolean), "Is true"),
                op("false", &format!("{}False", keywords.boolean), "Is false"),
            ]),
            AttrKind::Other => {}
        }
        if self.kind != AttrKind::Boolean {
            found.push(op("in", "In", "In collection"));
        }
        if self.nullable {
            found.push(op("isNull", keywords.null, "Is null"));
        }
        found
    }
}

pub(super) struct Facts {
    pub(super) entity: String,
    pub(super) kotlin: bool,
    body: Body,
    pub(super) existing: Vec<(String, Vec<String>)>,
    pub(super) keywords: Keywords,
    /// A projection or DTO type that replaces the entity in the return type (query window "+").
    pub(super) result: Option<String>,
    /// Add a `Pageable` parameter even when the return type is not a `Page` or `Slice`.
    pub(super) pageable: bool,
}

enum Body {
    Braces(usize),
    Bodyless(usize),
}

fn header_entity_is_simple(masked: &str, repo: &queries::Repository) -> bool {
    let open = masked[repo.name_start..].find('{').map_or(masked.len(), |at| at + repo.name_start);
    let header = &masked[repo.name_start..open];
    let Some(after) = header.match_indices("Repository").map(|(at, _)| at + "Repository".len()).find(|&at| header[at..].trim_start().starts_with('<')) else {
        return false;
    };
    let inner = &header[after..].trim_start()[1..];
    inner.split(',').next().map(str::trim) == Some(repo.entity.as_str()) && identifier(&repo.entity)
}

fn parameter_types(masked: &str, from: usize, kotlin: bool) -> Option<Vec<String>> {
    let open = from + masked[from..].find('(')?;
    let close = closing(masked.as_bytes(), open)?;
    let inner = &masked[open + 1..close];
    let compact = |text: &str| text.chars().filter(|ch| !ch.is_whitespace()).collect::<String>();
    Some(
        split_top_level(inner)
            .into_iter()
            .map(|(_, part)| part.trim())
            .filter(|part| !part.is_empty())
            .map(|part| {
                if kotlin {
                    return compact(part.split_once(':').map_or(part, |(_, ty)| ty));
                }
                let part = part.rsplit_once(char::is_whitespace).map_or(part, |(ty, _)| ty);
                compact(part)
            })
            .collect(),
    )
}

pub(super) fn source(doc: &Document) -> Result<Facts, String> {
    let kotlin = doc.language == "kotlin";
    let masked = mask(&doc.text);
    let single_interface = words(&masked, "interface").count() == 1
        && ["class", "object", "enum", "typealias"].iter().all(|word| words(&masked, word).next().is_none());
    let repo = queries::parse_repository(&doc.text, kotlin).filter(|_| single_interface).ok_or(
        "Invoke this inside a Spring Data repository interface declared alone in its file.",
    )?;
    if !header_entity_is_simple(&masked, &repo) {
        return Err("The entity must be a simple (unqualified) generic argument of the repository.".into());
    }
    let open = masked[repo.name_start..].find('{').map(|at| at + repo.name_start);
    let body = match open {
        Some(open) => Body::Braces(closing(masked.as_bytes(), open).ok_or("The interface body is not closed.")?),
        None if kotlin => Body::Bodyless(doc.text.trim_end().len()),
        None => return Err("The interface has no body.".into()),
    };
    let existing = repo
        .methods
        .iter()
        .map(|method| (method.name.clone(), parameter_types(&masked, method.end, kotlin).unwrap_or_default()))
        .collect();
    Ok(Facts { entity: repo.entity, kotlin, body, existing, keywords: Keywords::DEFAULT, result: None, pageable: false })
}

pub(super) fn eligible(doc: &Document) -> bool {
    source(doc).is_ok()
}

fn declared_nullable(masked: &str, name: &str, ty: &str) -> bool {
    words(masked, name).any(|at| {
        masked[at + name.len()..]
            .trim_start()
            .strip_prefix(':')
            .and_then(|rest| rest.trim_start().strip_prefix(ty))
            .is_some_and(|rest| rest.starts_with('?'))
    })
}

pub(super) fn attributes(class: &Class, text: &str) -> Vec<Attr> {
    let masked = mask(text);
    class
        .attributes
        .iter()
        .filter(|attribute| attribute.relation == Relation::None && identifier(&attribute.name) && super::dto_action::supported(class, &attribute.ty))
        .filter_map(|attribute| {
            let simple = attribute.ty.rsplit('.').next().unwrap_or(&attribute.ty);
            let kind = match simple {
                "String" => AttrKind::Text,
                "Boolean" | "boolean" => AttrKind::Boolean,
                "Long" | "Integer" | "Short" | "Byte" | "Int" | "long" | "int" | "short" | "byte" | "Double" | "Float" | "double" | "float"
                | "Character" | "Char" | "char" | "BigDecimal" | "BigInteger" | "LocalDate" | "LocalDateTime" | "LocalTime" | "Instant"
                | "OffsetDateTime" | "ZonedDateTime" | "Duration" | "Date" => AttrKind::Comparable,
                "UUID" => AttrKind::Other,
                _ => return None,
            };
            let import = if attribute.ty.contains('.') {
                Some(attribute.ty.clone())
            } else {
                class.imports.iter().find(|import| import.rsplit('.').next() == Some(simple) && !import.ends_with('*')).cloned()
            }
            .filter(|import| !import.starts_with("java.lang.") && !import.starts_with("kotlin."));
            let primitive = !class.kotlin && simple.chars().next().is_some_and(char::is_lowercase);
            let nullable = !attribute.not_null
                && if class.kotlin { declared_nullable(&masked, &attribute.name, &attribute.ty) } else { !primitive };
            Some(Attr { name: attribute.name.clone(), ty: simple.to_owned(), kind, nullable, import })
        })
        .collect()
}

pub(super) fn find_entity(project: &Project, root: &str, name: &str, repository_package: Option<&str>, imports: &[String]) -> Result<(Class, String), Error> {
    let mut found = Vec::new();
    for extension in ["java", "kt"] {
        for file in project.find_files(&format!("**/{name}.{extension}"), 9)? {
            if path(&file).as_deref() != Some(file.as_str()) || !inside(root, &file) {
                return Err(Error::Stale);
            }
            let text = project.read_file(&file)?;
            if text.len() > MAX_TEXT {
                continue;
            }
            if let Some(class) = parse_class(&text, &file).filter(|class| class.kind == Kind::Entity && class.name == name) {
                found.push((class, text));
            }
        }
    }
    let unresolved = || format!("`{name}` does not match exactly one entity file named {name}.java or {name}.kt in this project; no entity is guessed.");
    if found.len() != 1 {
        return Err(unresolved().into());
    }
    let (class, text) = found.remove(0);
    let qualified = format!("{}.{name}", class.package.as_deref().unwrap_or_default());
    let explicit: Vec<_> = imports.iter().filter(|import| import.rsplit('.').next() == Some(name)).collect();
    let in_scope = match explicit.as_slice() {
        [] => class.package.as_deref() == repository_package,
        [import] => **import == qualified,
        _ => false,
    };
    if !in_scope {
        return Err(format!("`{name}` in this repository is not provably the entity found in {}.", class.path).into());
    }
    Ok((class, text))
}

pub(super) fn repository_imports(doc: &Document) -> Vec<String> {
    mask(&doc.text)
        .lines()
        .filter_map(|line| line.trim().strip_prefix("import "))
        .map(|import| import.trim_end_matches(';').trim().to_owned())
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Subject {
    Find,
    Count,
    Exists,
    Delete,
}

impl Subject {
    fn word(self) -> &'static str {
        match self {
            Self::Find => "find",
            Self::Count => "count",
            Self::Exists => "exists",
            Self::Delete => "delete",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Returns {
    List,
    Optional,
    Single,
    Page,
    Slice,
    Stream,
    Number,
    Nothing,
}

pub(super) struct Spec<'a> {
    subject: Subject,
    conditions: Vec<(&'a Attr, &'static str)>,
    any: bool,
    order: Option<(&'a Attr, bool)>,
    limit: Option<u32>,
    returns: Returns,
}

pub(super) struct Rendered {
    name: String,
    declaration: String,
    imports: Vec<String>,
}

/// Parameters one condition adds, and the imports they need.
pub(super) fn condition_params(attr: &Attr, key: &str, kotlin: bool, imports: &mut Vec<String>) -> Vec<(String, String)> {
    imports.extend(attr.import.clone());
    let ty = queries::convert_type(&attr.ty, kotlin);
    let base = &attr.name;
    match key {
        "isNull" | "true" | "false" => Vec::new(),
        "between" => vec![(format!("{base}Start"), ty.clone()), (format!("{base}End"), ty)],
        "in" => {
            if !kotlin {
                imports.push("java.util.Collection".into());
            }
            vec![(format!("{base}s"), format!("Collection<{}>", if kotlin { ty } else { queries::boxed(&ty) }))]
        }
        _ => vec![(base.clone(), ty)],
    }
}

pub(super) fn return_text(subject: Subject, returns: Returns, entity: &str, kotlin: bool, imports: &mut Vec<String>) -> Option<String> {
    match returns {
        Returns::List => Some(format!("List<{entity}>")).inspect(|_| {
            if !kotlin {
                imports.push("java.util.List".into());
            }
        }),
        Returns::Optional => Some(format!("Optional<{entity}>")).inspect(|_| imports.push("java.util.Optional".into())),
        Returns::Single => Some(if kotlin { format!("{entity}?") } else { entity.to_owned() }),
        Returns::Page => Some(format!("Page<{entity}>")).inspect(|_| imports.push(format!("{DOMAIN}.Page"))),
        Returns::Slice => Some(format!("Slice<{entity}>")).inspect(|_| imports.push(format!("{DOMAIN}.Slice"))),
        Returns::Stream => Some(format!("Stream<{entity}>")).inspect(|_| imports.push("java.util.stream.Stream".into())),
        Returns::Number | Returns::Nothing if subject == Subject::Exists => Some(if kotlin { "Boolean" } else { "boolean" }.into()),
        Returns::Number => Some(if kotlin { "Long" } else { "long" }.into()),
        Returns::Nothing => (!kotlin).then(|| "void".to_owned()),
    }
}

pub(super) fn declaration_text(kotlin: bool, name: &str, parameter_list: &str, returned: Option<String>) -> String {
    match (kotlin, returned) {
        (true, Some(returned)) => format!("fun {name}({parameter_list}): {returned}"),
        (true, None) => format!("fun {name}({parameter_list})"),
        (false, returned) => format!("{} {name}({parameter_list});", returned.unwrap_or_default()),
    }
}

fn render_method(facts: &Facts, spec: &Spec) -> Result<Rendered, String> {
    let kotlin = facts.kotlin;
    let entity = facts.result.as_ref().unwrap_or(&facts.entity);
    if spec.conditions.is_empty() && !(spec.subject == Subject::Find && spec.order.is_some()) {
        return Err("Choose at least one condition (an ordering alone is allowed for find).".into());
    }
    if spec.subject != Subject::Find && (spec.order.is_some() || spec.limit.is_some()) {
        return Err("Ordering and Top/First apply to find methods only.".into());
    }
    if spec.limit.is_some_and(|limit| limit == 0 || limit > MAX_LIMIT) {
        return Err(format!("Top/First must be between 1 and {MAX_LIMIT}."));
    }
    if matches!(spec.returns, Returns::Single | Returns::Optional) && spec.limit.is_some_and(|limit| limit > 1) {
        return Err("A single or Optional result cannot combine with Top/First greater than 1.".into());
    }
    let valid = match spec.subject {
        Subject::Find => !matches!(spec.returns, Returns::Number | Returns::Nothing),
        Subject::Count | Subject::Exists => spec.returns == Returns::Number,
        Subject::Delete => matches!(spec.returns, Returns::Number | Returns::Nothing),
    };
    if !valid {
        return Err("This return type is not valid for the chosen subject.".into());
    }

    let mut predicate = String::new();
    let mut params: Vec<(String, String)> = Vec::new();
    let mut imports: Vec<String> = Vec::new();
    for (index, (attr, key)) in spec.conditions.iter().enumerate() {
        let Some(operator) = attr.operators_with(&facts.keywords).into_iter().find(|candidate| candidate.key == *key) else {
            return Err(format!("`{key}` is not valid for `{}` of type {}.", attr.name, attr.ty));
        };
        if index > 0 {
            predicate.push_str(if spec.any { "Or" } else { "And" });
        }
        predicate.push_str(&capitalize(&attr.name));
        predicate.push_str(&operator.suffix);
        params.extend(condition_params(attr, key, kotlin, &mut imports));
    }
    if let Some((attr, descending)) = spec.order {
        predicate.push_str(&format!("OrderBy{}{}", capitalize(&attr.name), if descending { "Desc" } else { "Asc" }));
    }

    let mut subject = if spec.subject == Subject::Find { facts.keywords.find } else { spec.subject.word() }.to_owned();
    match spec.limit {
        Some(1) => subject.push_str("First"),
        Some(limit) => subject.push_str(&format!("Top{limit}")),
        None => {}
    }
    let name = format!("{subject}By{predicate}");

    if facts.pageable && !matches!(spec.returns, Returns::List | Returns::Stream) && spec.subject == Subject::Find {
        return Err("A Pageable parameter without a Page or Slice result is supported for List and Stream results only.".into());
    }
    if matches!(spec.returns, Returns::Page | Returns::Slice) || facts.pageable {
        imports.push(format!("{DOMAIN}.Pageable"));
        params.push(("pageable".into(), "Pageable".into()));
    }
    let mut seen = std::collections::HashSet::new();
    if params.iter().any(|(param, _)| !seen.insert(param.clone())) {
        return Err("Generated parameter names would collide; choose other attributes.".into());
    }
    let parameter_list = params
        .iter()
        .map(|(param, ty)| if kotlin { format!("{param}: {ty}") } else { format!("{ty} {param}") })
        .collect::<Vec<_>>()
        .join(", ");

    let returned = return_text(spec.subject, spec.returns, entity, kotlin, &mut imports);
    let declaration = declaration_text(kotlin, &name, &parameter_list, returned);
    imports.sort();
    imports.dedup();
    Ok(Rendered { name, declaration, imports })
}

pub(super) fn explicit_import_clash(doc: &Document, imports: &[String], entity: &str) -> Result<(), String> {
    let existing = repository_imports(doc);
    for import in imports {
        let simple = import.rsplit('.').next().unwrap_or(import);
        if simple == entity || existing.iter().any(|known| known.rsplit('.').next() == Some(simple) && known != import) {
            return Err(format!("`{simple}` already names a different type in this file; no import is added."));
        }
    }
    Ok(())
}

pub(super) fn edits(doc: &Document, facts: &Facts, spec: &Spec) -> Result<Vec<TextEdit>, Error> {
    let rendered = render_method(facts, spec)?;
    let parameters = parameter_types_of(&rendered.declaration, facts.kotlin);
    if facts.existing.iter().any(|(name, existing)| *name == rendered.name && *existing == parameters) {
        return Err(format!("`{}` with the same parameters already exists in this interface.", rendered.name).into());
    }
    explicit_import_clash(doc, &rendered.imports, &facts.entity)?;
    let line = format!("    {}", rendered.declaration);
    let (at, mut method_text) = match facts.body {
        Body::Bodyless(end) => (end, format!(" {{\n{line}\n}}")),
        Body::Braces(close) => {
            let line_start = doc.text[..close].rfind('\n').map_or(0, |at| at + 1);
            let own_line = doc.text[line_start..close].trim().is_empty();
            let empty = mask(&doc.text)[..close].trim_end().ends_with('{');
            match (own_line, empty) {
                (true, true) => (line_start, format!("{line}\n")),
                (true, false) => (line_start, format!("\n{line}\n")),
                (false, _) => (close, format!("\n{line}\n")),
            }
        }
    };
    let crlf = doc.text.contains("\r\n");
    if crlf {
        method_text = method_text.replace('\n', "\r\n");
    }
    let import_refs: Vec<&str> = rendered.imports.iter().map(String::as_str).collect();
    let mut edits = Vec::new();
    if let Some(mut import) = import_edit(&doc.text, &import_refs, facts.kotlin) {
        if crlf {
            import.text = import.text.replace('\n', "\r\n");
        }
        edits.push(import.to_text_edit(&doc.text));
    }
    edits.push(model::Edit::insert(at, &method_text).to_text_edit(&doc.text));
    Ok(edits)
}

fn parameter_types_of(declaration: &str, kotlin: bool) -> Vec<String> {
    parameter_types(declaration, 0, kotlin).unwrap_or_default()
}

fn pick_one(title: &str, items: &[PickItem]) -> Result<String, Error> {
    host::pick(title, items)?.ok_or(Error::Cancelled)
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    resolve_flow(pending, doc, project, false, None)
}

pub(super) fn resolve_for_attribute(pending: &Pending, doc: &Document, project: &Project, attribute: &str) -> Result<FileAction, Error> {
    resolve_flow(pending, doc, project, false, Some(attribute))
}

pub(super) fn resolve_with_options(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    resolve_flow(pending, doc, project, true, None)
}

fn resolve_flow(pending: &Pending, doc: &Document, project: &Project, options: bool, preselected: Option<&str>) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let mut facts = source(doc).map_err(Error::from)?;
    facts.keywords = Keywords::load().map_err(Error::from)?;
    let imports = repository_imports(doc);
    let (class, entity_text) = find_entity(project, &pending.root, &facts.entity, package(doc).as_deref(), &imports)?;
    let attrs = attributes(&class, &entity_text);
    if attrs.is_empty() {
        return Err("The entity has no plain direct scalar attribute to query.".into());
    }

    let subjects = [("find", "find"), ("count", "count"), ("exists", "exists"), ("delete", "delete")]
        .map(|(key, label)| PickItem::new(key, label));
    let subject = match pick_one("Query subject", &subjects)?.as_str() {
        "find" => Subject::Find,
        "count" => Subject::Count,
        "exists" => Subject::Exists,
        "delete" => Subject::Delete,
        _ => return Err("Choose one of the offered subjects.".into()),
    };

    let items: Vec<_> = attrs.iter().map(|attr| PickItem::new(&attr.name, &attr.name).description(&attr.ty)).collect();
    let chosen = match preselected {
        Some(name) if attrs.iter().any(|attr| attr.name == name) => vec![name.to_owned()],
        Some(_) => return Err("The attribute is not a plain scalar attribute the query method flow supports.".into()),
        None => host::pick_many("Condition attributes (plain scalar attributes only; relationship paths are not offered)", &items)?
            .ok_or(Error::Cancelled)?,
    };
    let mut conditions = Vec::new();
    for name in &chosen {
        let Some(attr) = attrs.iter().find(|attr| attr.name == *name).filter(|_| !conditions.iter().any(|(seen, _): &(&Attr, &str)| seen.name == *name)) else {
            return Err("Choose offered attributes, each once.".into());
        };
        let operators = attr.operators_with(&facts.keywords);
        let choices: Vec<_> = operators.iter().map(|operator| PickItem::new(operator.key, operator.label)).collect();
        let key = pick_one(&format!("Operator for {} ({})", attr.name, attr.ty), &choices)?;
        let Some(operator) = operators.iter().find(|operator| operator.key == key) else {
            return Err("Choose one of the offered operators.".into());
        };
        conditions.push((attr, operator.key));
    }

    let mut any = false;
    if conditions.len() > 1 {
        let combinators = [PickItem::new("and", "And (all conditions)"), PickItem::new("or", "Or (any condition)")];
        any = pick_one("Combine all conditions with", &combinators)? == "or";
    }

    let mut order = None;
    let mut limit = None;
    let returns = if subject == Subject::Find {
        let mut choices = vec![PickItem::new("none", "No ordering")];
        choices.extend(attrs.iter().map(|attr| PickItem::new(&attr.name, &attr.name)));
        let picked = pick_one("Order by", &choices)?;
        if picked != "none" {
            let attr = attrs.iter().find(|attr| attr.name == picked).ok_or("Choose an offered attribute.")?;
            let directions = [PickItem::new("asc", "Ascending"), PickItem::new("desc", "Descending")];
            order = Some((attr, pick_one("Direction", &directions)? == "desc"));
        }
        let typed = host::input("Top/First limit (blank for none)", None)?.ok_or(Error::Cancelled)?;
        let typed = typed.trim();
        if !typed.is_empty() {
            limit = Some(typed.parse::<u32>().map_err(|_| Error::from("Enter a whole number for the limit, or leave it blank."))?);
        }
        let returns = [
            ("list", "List", Returns::List),
            ("optional", "Optional", Returns::Optional),
            ("single", "Single entity (null when absent)", Returns::Single),
            ("page", "Page (adds a Pageable parameter)", Returns::Page),
            ("slice", "Slice (adds a Pageable parameter)", Returns::Slice),
            ("stream", "Stream", Returns::Stream),
        ];
        let choices: Vec<_> = returns.iter().map(|(key, label, _)| PickItem::new(key, label)).collect();
        let key = pick_one("Return type", &choices)?;
        returns.iter().find(|(candidate, ..)| *candidate == key).map(|(.., value)| *value).ok_or("Choose an offered return type.")?
    } else if subject == Subject::Delete {
        let choices = [PickItem::new("number", "Delete count (long)"), PickItem::new("void", "No result (void)")];
        if pick_one("Return type", &choices)? == "void" { Returns::Nothing } else { Returns::Number }
    } else {
        Returns::Number
    };

    let spec = Spec { subject, conditions, any, order, limit, returns };
    let choices = if options { Some(query_options::ask(pending, doc, project, &class, subject, returns)?) } else { None };
    if let Some(choices) = &choices {
        facts.result = choices.created.as_ref().map(|created| created.name.clone());
        facts.pageable = choices.pageable;
    }
    current(pending, doc)?;
    let mut edits = edits(doc, &facts, &spec)?;
    let mut after = patched(&doc.text, &edits)?;
    let name = render_method(&facts, &spec)?.name;
    if let Some(choices) = choices.as_ref().filter(|choices| choices.binding != query_options::Binding::Derived) {
        (edits, after) = query_options::bind_parameters(doc, &facts, &attrs, &entity_text, &after, &name, choices.binding)?;
    }
    if after.len() > MAX_TEXT {
        return Err("Generated source exceeds the supported bound.".into());
    }
    let created = choices.and_then(|choices| choices.created);
    let new_type = created.as_ref().map(|created| format!("\nNew file: {}\n{}", created.path, created.text)).unwrap_or_default();
    if !ui_kit::prompts::confirm_exact(
        "Add query method?",
        &format!(
            "Target: {}\nSource version: {}\n\n{after}{new_type}\nOnly this repository source is edited. The entity {} was found by file name in this project; mappings in XML or inherited attributes are not resolved, and no Spring Data version is read.",
            doc.path, doc.version, facts.entity
        ),
    )? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    Ok(FileAction {
        title: format!("Add query method {name}"),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), edits)],
        operations: created.into_iter().map(|created| FileOperation::Create { path: created.path, text: created.text, overwrite: false }).collect(),
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::Prop;
    use openidea_plugin::mock::MockHost;

    const REPO_PATH: &str = "/project/src/main/java/shop/OrderRepository.java";
    const KT_REPO_PATH: &str = "/project/src/main/kotlin/shop/OrderRepository.kt";
    const JAVA_ENTITY: &str = "package shop;\nimport jakarta.persistence.*;\nimport java.math.BigDecimal;\nimport java.time.LocalDate;\n@Entity\npublic class Order {\n    @Id Long id;\n    String number;\n    BigDecimal total;\n    boolean paid;\n    Boolean flagged;\n    LocalDate placedOn;\n    @ManyToOne Customer customer;\n    @Column(nullable = false) String code;\n}\n";
    const KT_ENTITY: &str = "package shop\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\nimport jakarta.persistence.ManyToOne\n@Entity\nclass Order {\n    @Id var id: Long? = null\n    var number: String? = null\n    var total: Int = 0\n    var paid: Boolean = false\n    @ManyToOne var customer: Customer? = null\n}\n";
    const JAVA_REPO: &str = "package shop;\n\nimport org.springframework.data.jpa.repository.JpaRepository;\n\npublic interface OrderRepository extends JpaRepository<Order, Long> {\n}\n";
    const KT_REPO: &str = "package shop\n\nimport org.springframework.data.jpa.repository.JpaRepository\n\ninterface OrderRepository : JpaRepository<Order, Long> {\n}\n";

    fn apply(doc: &Document, result: Result<FileAction, Error>) -> String {
        let action = result.expect("action");
        assert!(action.operations.is_empty());
        let [(path, edits)] = action.edits.as_slice() else { panic!("one document") };
        assert_eq!(path, &doc.path);
        patched(&doc.text, edits).unwrap()
    }

    fn multi(host: &MockHost, values: &[&str]) {
        host.answer_next_prompt(Some(values));
    }

    fn drive(kotlin: bool, repo: &str, subject: &str, conditions: &[&str], operators: &[&str], rest: &[&str]) -> (Result<FileAction, Error>, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let (path, language, entity_path, entity) = if kotlin {
            (KT_REPO_PATH, "kotlin", "src/main/kotlin/shop/Order.kt", KT_ENTITY)
        } else {
            (REPO_PATH, "java", "src/main/java/shop/Order.java", JAVA_ENTITY)
        };
        host.add_file(entity_path, entity);
        host.open_editor(path, language, repo, &[]);
        let doc = Document { path: path.into(), language: language.into(), text: repo.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).expect("menu entry");
        host.answer_next_prompt(Some(&[subject]));
        multi(&host, conditions);
        for operator in operators {
            host.answer_next_prompt(Some(&[*operator]));
        }
        for answer in rest {
            host.answer_next_prompt(Some(&[*answer]));
        }
        host.answer_next_prompt(Some(&[]));
        (plugin.resolve_file_action(&doc, pending), doc)
    }

    #[test]
    fn java_find_with_and_order_limit_and_page_renders_method_and_imports() {
        let (result, doc) = drive(false, JAVA_REPO, "find", &["number", "total"], &["containing", "between"], &["and", "total", "desc", "3", "page"]);
        let text = apply(&doc, result);
        assert_eq!(
            text,
            "package shop;\n\nimport org.springframework.data.jpa.repository.JpaRepository;\nimport java.math.BigDecimal;\nimport org.springframework.data.domain.Page;\nimport org.springframework.data.domain.Pageable;\n\npublic interface OrderRepository extends JpaRepository<Order, Long> {\n    Page<Order> findTop3ByNumberContainingAndTotalBetweenOrderByTotalDesc(String number, BigDecimal totalStart, BigDecimal totalEnd, Pageable pageable);\n}\n"
        );
    }

    #[test]
    fn java_variants_render_exact_declarations() {
        let cases: &[(&str, &[&str], &[&str], &[&str], &str)] = &[
            ("find", &["number"], &["ignoreCase"], &["none", "", "optional"], "Optional<Order> findByNumberIgnoreCase(String number);"),
            ("find", &["paid"], &["true"], &["none", "", "list"], "List<Order> findByPaidTrue();"),
            ("find", &["flagged"], &["isNull"], &["none", "", "stream"], "Stream<Order> findByFlaggedIsNull();"),
            ("find", &["number"], &["in"], &["none", "1", "single"], "Order findFirstByNumberIn(Collection<String> numbers);"),
            ("find", &["number"], &["startingWithIgnoreCase"], &["none", "", "slice"], "Slice<Order> findByNumberStartingWithIgnoreCase(String number, Pageable pageable);"),
            ("count", &["placedOn"], &["greaterThan"], &[], "long countByPlacedOnGreaterThan(LocalDate placedOn);"),
            ("exists", &["number"], &["not"], &[], "boolean existsByNumberNot(String number);"),
            ("delete", &["number"], &["endingWith"], &["number"], "long deleteByNumberEndingWith(String number);"),
            ("delete", &["number"], &["like"], &["void"], "void deleteByNumberLike(String number);"),
        ];
        for (subject, conditions, operators, rest, expected) in cases {
            let (result, doc) = drive(false, JAVA_REPO, subject, conditions, operators, rest);
            let text = apply(&doc, result);
            assert!(text.contains(&format!("    {expected}\n}}\n")), "{expected}\n{text}");
        }
    }

    #[test]
    fn is_containing_is_an_approved_spelling_and_the_unlisted_one_is_refused() {
        let host = MockHost::install();
        host.set_setting("queryContainsKeyword", "\"IsContaining\"");
        assert_eq!(Keywords::load().unwrap().contains, "IsContaining");
        let attr = Attr { name: "name".into(), ty: "String".into(), kind: AttrKind::Text, nullable: false, import: None };
        let suffixes: Vec<_> = attr.operators_with(&Keywords::load().unwrap()).into_iter().map(|operator| operator.suffix).collect();
        assert!(suffixes.contains(&"IsContaining".to_owned()) && suffixes.contains(&"IsContainingIgnoreCase".to_owned()));
        host.set_setting("queryContainsKeyword", "\"Includes\"");
        assert!(Keywords::load().is_err());
    }

    #[test]
    fn or_combinator_and_order_only_find() {
        let (result, doc) = drive(false, JAVA_REPO, "find", &["number", "paid"], &["is", "false"], &["or", "number", "asc", "", "list"]);
        assert!(apply(&doc, result).contains("List<Order> findByNumberOrPaidFalseOrderByNumberAsc(String number);"));
        let (result, doc) = drive(false, JAVA_REPO, "find", &[], &[], &["number", "desc", "1", "optional"]);
        assert!(apply(&doc, result).contains("Optional<Order> findFirstByOrderByNumberDesc();"));
    }

    #[test]
    fn kotlin_renders_nullable_single_unit_delete_and_bodyless_interface() {
        let (result, doc) = drive(true, KT_REPO, "find", &["number", "total"], &["isNull", "lessThan"], &["and", "none", "", "single"]);
        assert!(apply(&doc, result).contains("    fun findByNumberIsNullAndTotalLessThan(total: Int): Order?\n}\n"));
        let (result, doc) = drive(true, KT_REPO, "find", &["total"], &["in"], &["none", "", "list"]);
        assert!(apply(&doc, result).contains("    fun findByTotalIn(totals: Collection<Int>): List<Order>\n"));
        let (result, doc) = drive(true, KT_REPO, "delete", &["paid"], &["true"], &["void"]);
        assert!(apply(&doc, result).contains("    fun deleteByPaidTrue()\n}"));
        let (result, doc) = drive(true, KT_REPO, "exists", &["number"], &["is"], &[]);
        assert!(apply(&doc, result).contains("fun existsByNumber(number: String): Boolean"));
        let (result, doc) = drive(true, KT_REPO, "find", &["number"], &["is"], &["none", "", "optional"]);
        let text = apply(&doc, result);
        assert!(text.contains("import java.util.Optional\n") && !text.contains("import java.util.Optional;"), "{text}");
        let (result, doc) = drive(true, KT_REPO, "find", &["number"], &["is"], &["none", "", "page"]);
        let text = apply(&doc, result);
        assert!(text.contains("fun findByNumber(number: String, pageable: Pageable): Page<Order>") && text.contains("import org.springframework.data.domain.Pageable\n"), "{text}");
        let bodyless = "package shop\n\nimport org.springframework.data.jpa.repository.JpaRepository\n\ninterface OrderRepository : JpaRepository<Order, Long>\n";
        let (result, doc) = drive(true, bodyless, "count", &["paid"], &["is"], &[]);
        assert!(apply(&doc, result).ends_with("interface OrderRepository : JpaRepository<Order, Long> {\n    fun countByPaid(paid: Boolean): Long\n}\n"));
    }

    #[test]
    fn kotlin_not_null_property_does_not_offer_is_null() {
        let (result, _) = drive(true, KT_REPO, "find", &["total"], &["isNull"], &[]);
        assert!(result.is_err());
        let (result, _) = drive(false, JAVA_REPO, "find", &["code"], &["isNull"], &[]);
        assert!(result.is_err(), "@Column(nullable=false)");
    }

    #[test]
    fn existing_members_non_empty_body_crlf_and_existing_imports_are_respected() {
        let repo = "package shop;\n\nimport java.util.List;\nimport org.springframework.data.jpa.repository.JpaRepository;\n\npublic interface OrderRepository extends JpaRepository<Order, Long> {\n    List<Order> findByNumber(String number);\n}\n";
        let (result, doc) = drive(false, repo, "find", &["paid"], &["true"], &["none", "", "list"]);
        let text = apply(&doc, result);
        assert_eq!(text.matches("import java.util.List;").count(), 1, "{text}");
        assert!(text.contains("findByNumber(String number);\n\n    List<Order> findByPaidTrue();\n}\n"), "{text}");
        let crlf = repo.replace('\n', "\r\n");
        let (result, doc) = drive(false, &crlf, "find", &["paid"], &["true"], &["none", "", "optional"]);
        let text = apply(&doc, result);
        assert!(text.contains("import java.util.Optional;\r\n") && text.contains("    Optional<Order> findByPaidTrue();\r\n}"), "{text:?}");
        assert!(!text.replace("\r\n", "").contains('\n'));
        let wildcard = repo.replace("import java.util.List;", "import java.util.*;");
        let (result, doc) = drive(false, &wildcard, "find", &["paid"], &["true"], &["none", "", "optional"]);
        assert_eq!(apply(&doc, result).matches("java.util").count(), 1);
    }

    #[test]
    fn refusals_name_the_reason() {
        let with_existing = JAVA_REPO.replace("{\n}", "{\n    List<Order> findByPaidTrue();\n}");
        let (result, _) = drive(false, &with_existing, "find", &["paid"], &["true"], &["none", "", "list"]);
        assert!(format!("{:?}", result.err().unwrap()).contains("already exists"));
        let clash = JAVA_REPO.replace("import org", "import foo.Optional;\nimport org");
        let (result, _) = drive(false, &clash, "find", &["paid"], &["true"], &["none", "", "optional"]);
        assert!(format!("{:?}", result.err().unwrap()).contains("different type"));
        let (result, _) = drive(false, JAVA_REPO, "find", &["number"], &["is"], &["none", "5", "optional"]);
        assert!(format!("{:?}", result.err().unwrap()).contains("Top/First"));
        let (result, _) = drive(false, JAVA_REPO, "find", &["paid"], &["containing"], &[]);
        assert!(result.is_err(), "Containing is not offered for boolean");
        let (result, _) = drive(false, JAVA_REPO, "find", &["number"], &["between"], &[]);
        assert!(result.is_err(), "Between is not offered for String");
        let (result, _) = drive(false, JAVA_REPO, "find", &["customer"], &[], &[]);
        assert!(result.is_err(), "relationship paths are not offered");
        let (result, _) = drive(false, JAVA_REPO, "count", &[], &[], &[]);
        assert!(format!("{:?}", result.err().unwrap()).contains("at least one condition"));
        let (result, _) = drive(false, JAVA_REPO, "find", &["number"], &["is"], &["none", "abc", "list"]);
        assert!(result.is_err());
        let (result, _) = drive(false, JAVA_REPO, "find", &["number", "number"], &["is", "is"], &["and"]);
        assert!(result.is_err(), "an attribute twice");
    }

    #[test]
    fn entity_resolution_refuses_unknown_ambiguous_qualified_and_foreign_package() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(REPO_PATH, "java", JAVA_REPO, &[]);
        let doc = Document { path: REPO_PATH.into(), language: "java".into(), text: JAVA_REPO.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        let message = format!("{:?}", plugin.resolve_file_action(&doc, pending).err().unwrap());
        assert!(message.contains("exactly one entity"), "unknown entity: {message}");
        host.add_file("src/main/java/shop/Order.java", JAVA_ENTITY);
        host.add_file("src/main/java/other/Order.java", &JAVA_ENTITY.replace("package shop;", "package other;"));
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        let message = format!("{:?}", plugin.resolve_file_action(&doc, pending).err().unwrap());
        assert!(message.contains("exactly one entity"), "ambiguous entity: {message}");
        let qualified = JAVA_REPO.replace("<Order,", "<shop.Order,");
        assert!(source(&Document { text: qualified, ..doc.clone() }).is_err());
        let foreign = JAVA_REPO.replace("import org", "import other.Order;\nimport org");
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/Order.java", JAVA_ENTITY);
        let doc = Document { text: foreign, ..doc };
        host.open_editor(REPO_PATH, "java", &doc.text, &[]);
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        assert!(format!("{:?}", plugin.resolve_file_action(&doc, pending).err().unwrap()).contains("not provably"));
    }

    #[test]
    fn menu_entry_only_in_a_repository_interface_and_cancellation_edits_nothing() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let plain = "package shop;\n\npublic interface Plain {\n}\n";
        host.open_editor("/project/src/main/java/shop/Plain.java", "java", plain, &[]);
        let doc = Document { path: "/project/src/main/java/shop/Plain.java".into(), language: "java".into(), text: plain.into(), version: 1 };
        assert!(JpaAssist::default().file_actions(&doc, Range::default()).iter().all(|a| a.title != TITLE));
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/Order.java", JAVA_ENTITY);
        host.open_editor(REPO_PATH, "java", JAVA_REPO, &[]);
        let doc = Document { path: REPO_PATH.into(), language: "java".into(), text: JAVA_REPO.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)));
    }

    #[test]
    fn generated_names_round_trip_through_the_derived_query_parser() {
        let class = parse_class(JAVA_ENTITY, "/project/src/main/java/shop/Order.java").unwrap();
        let attrs = attributes(&class, JAVA_ENTITY);
        let props: Vec<Prop> = attrs.iter().map(|a| Prop { name: a.name.clone(), ty: a.ty.clone(), fqcn: None, target: None }).collect();
        let lookup = |owner: &str| (owner == "Order").then(|| props.clone());
        let facts = Facts { entity: "Order".into(), kotlin: false, body: Body::Braces(0), existing: vec![], keywords: Keywords::DEFAULT, result: None, pageable: false };
        let mut generated = 0;
        for attr in &attrs {
            for operator in attr.operators() {
                for (subject, order, limit) in [(Subject::Find, Some((attr, true)), Some(2)), (Subject::Count, None, None)] {
                    let returns = if subject == Subject::Find { Returns::List } else { Returns::Number };
                    let spec = Spec { subject, conditions: vec![(attr, operator.key)], any: false, order, limit, returns };
                    let name = render_method(&facts, &spec).unwrap().name;
                    assert_eq!(queries::problem(&name, "Order", &lookup), None, "{name}");
                    let (_, start) = queries::split_subject(&name).unwrap();
                    let parsed = queries::parse(&name[start..], "Order", &lookup);
                    assert!(parsed.complete(&name[start..]), "{name}");
                    assert_eq!(parsed.clauses[0].operator, operator.suffix, "{name}");
                    generated += 1;
                }
            }
        }
        assert!(generated > 40);
        let two = [(&attrs[1], "containing"), (&attrs[2], "between")];
        let spec = Spec { subject: Subject::Find, conditions: two.to_vec(), any: true, order: Some((&attrs[2], false)), limit: Some(1), returns: Returns::Optional };
        let name = render_method(&facts, &spec).unwrap().name;
        assert_eq!(name, "findFirstByNumberContainingOrTotalBetweenOrderByTotalAsc");
        assert_eq!(queries::problem(&name, "Order", &lookup), None);
        let order_only = Spec { subject: Subject::Find, conditions: vec![], any: false, order: Some((&attrs[1], false)), limit: None, returns: Returns::List };
        let name = render_method(&facts, &order_only).unwrap().name;
        assert_eq!(queries::problem(&name, "Order", &lookup), None, "{name}");
    }

    fn keyword_settings(pairs: &[(&str, &str)]) -> Keywords {
        let host = MockHost::install();
        for (name, value) in pairs {
            host.set_setting(name, &format!("\"{value}\""));
        }
        Keywords::load().unwrap()
    }

    #[test]
    fn default_keywords_are_the_historic_output_and_unknown_values_are_refused() {
        let host = MockHost::install();
        assert_eq!(Keywords::load().unwrap(), Keywords::DEFAULT);
        host.set_setting("queryFindPrefix", "\"fetch\"");
        assert!(Keywords::load().unwrap_err().contains("queryFindPrefix"));
        host.set_setting("queryFindPrefix", "\"stream\"");
        assert!(Keywords::load().is_err(), "stream is a Stream-only prefix and is not offered");
    }

    #[test]
    fn configured_keywords_render_documented_spellings_that_parse_back() {
        let keywords = keyword_settings(&[
            ("queryFindPrefix", "read"),
            ("queryEqualityKeyword", "Equals"),
            ("queryNegationKeyword", "IsNot"),
            ("queryBooleanStyle", "IsTrue"),
            ("queryNullKeyword", "Null"),
            ("queryContainsKeyword", "Contains"),
            ("queryIgnoreCaseKeyword", "IgnoringCase"),
            ("queryDateKeywords", "AfterBefore"),
        ]);
        let class = parse_class(JAVA_ENTITY, "/project/src/main/java/shop/Order.java").unwrap();
        let attrs = attributes(&class, JAVA_ENTITY);
        let props: Vec<Prop> = attrs.iter().map(|a| Prop { name: a.name.clone(), ty: a.ty.clone(), fqcn: None, target: None }).collect();
        let lookup = |owner: &str| (owner == "Order").then(|| props.clone());
        let facts = Facts { entity: "Order".into(), kotlin: false, body: Body::Braces(0), existing: vec![], keywords, result: None, pageable: false };
        let named = |attr: &str, key: &'static str| {
            let attr = attrs.iter().find(|a| a.name == attr).unwrap();
            let spec = Spec { subject: Subject::Find, conditions: vec![(attr, key)], any: false, order: None, limit: None, returns: Returns::List };
            render_method(&facts, &spec).unwrap().name
        };
        for (attr, key, expected) in [
            ("number", "is", "readByNumberEquals"),
            ("number", "not", "readByNumberIsNot"),
            ("number", "containingIgnoreCase", "readByNumberContainsIgnoringCase"),
            ("number", "ignoreCase", "readByNumberIgnoringCase"),
            ("paid", "true", "readByPaidIsTrue"),
            ("paid", "false", "readByPaidIsFalse"),
            ("flagged", "isNull", "readByFlaggedNull"),
            ("placedOn", "greaterThan", "readByPlacedOnAfter"),
            ("placedOn", "lessThan", "readByPlacedOnBefore"),
            ("total", "greaterThan", "readByTotalGreaterThan"),
        ] {
            let name = named(attr, key);
            assert_eq!(name, expected);
            assert_eq!(queries::problem(&name, "Order", &lookup), None, "{name}");
            let (verb, start) = queries::split_subject(&name).unwrap();
            assert_eq!(verb, "read");
            assert!(queries::parse(&name[start..], "Order", &lookup).complete(&name[start..]), "{name}");
        }
        let count = Spec { subject: Subject::Count, conditions: vec![(&attrs[1], "is")], any: false, order: None, limit: None, returns: Returns::Number };
        assert_eq!(render_method(&facts, &count).unwrap().name, "countByNumberEquals", "only find uses the find prefix");
    }

    #[test]
    fn rendered_method_is_a_clean_repository_per_the_diagnostic_inspector() {
        let (result, doc) = drive(false, JAVA_REPO, "find", &["number", "paid"], &["containing", "true"], &["and", "total", "asc", "5", "list"]);
        let text = apply(&doc, result);
        let class = parse_class(JAVA_ENTITY, "/project/src/main/java/shop/Order.java").unwrap();
        let props: Vec<Prop> = attributes(&class, JAVA_ENTITY).iter().map(|a| Prop { name: a.name.clone(), ty: a.ty.clone(), fqcn: None, target: None }).collect();
        let lookup = |owner: &str| (owner == "Order").then(|| props.clone());
        let repo = queries::parse_repository(&text, false).unwrap();
        assert!(repo.methods.iter().any(|m| m.name == "findTop5ByNumberContainingAndPaidTrueOrderByTotalAsc"));
        assert!(crate::checks::inspect_repository(&repo, Some("Long"), &lookup).is_empty());
    }
}
