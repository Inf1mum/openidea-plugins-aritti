//! Reviewed in-place edit of one existing Spring Data derived query method of the invoking
//! repository interface. Only that document is edited and only the declaration span (return type,
//! name and parameter list) is replaced; annotations and comments around it stay.
//!
//! The method is read back through `queries::parse`; names that the parser cannot reproduce exactly
//! from the parsed structure (mixed And/Or, `AllIgnoreCase`, relationship paths, several ordering
//! keys, `Distinct`, descriptive subject text, operators outside the editable set) are refused.
//! Keyword spellings follow the Spring Data keyword tables (query-keywords-reference). Spellings
//! already in the method are preserved when their condition is not changed.
use super::query_method_action::{
    Attr, DOMAIN, Facts, Keywords, MAX_LIMIT, Returns, Subject, attributes, condition_params, declaration_text,
    explicit_import_clash, find_entity, repository_imports, return_text, source,
};
use super::*;
use crate::{
    model::{Edit, annotations, capitalize, closing, import_edit, members, split_top_level, without_annotations},
    queries,
};

pub(super) const TITLE: &str = "Edit query method…";
const SAFE_ANNOTATIONS: [&str; 6] = ["Transactional", "EntityGraph", "QueryHints", "Lock", "Deprecated", "SuppressWarnings"];

pub(super) fn eligible(doc: &Document) -> bool {
    source(doc).is_ok() && candidates(doc).is_some_and(|found| !found.is_empty())
}

fn candidates(doc: &Document) -> Option<Vec<usize>> {
    let repo = queries::parse_repository(&doc.text, doc.language == "kotlin")?;
    Some(repo.methods.iter().enumerate().filter(|(_, method)| !method.declared_query && queries::split_subject(&method.name).is_some()).map(|(at, _)| at).collect())
}

#[derive(Clone)]
struct Condition {
    attr: String,
    key: &'static str,
    /// The operator spelling found in the name; `None` once the operator changed or for a new condition.
    text: Option<String>,
    /// The `(type, name)` parameters this condition had; names are reused when the count stays.
    params: Vec<(String, String)>,
}

#[derive(Clone, PartialEq, Debug)]
struct Order {
    attr: String,
    desc: bool,
    explicit: bool,
}

#[derive(Clone)]
struct Shape {
    verb: String,
    subject: Subject,
    all: bool,
    limit: Option<u32>,
    limit_text: String,
    conditions: Vec<Condition>,
    any: bool,
    order: Option<Order>,
    returns: Returns,
    return_text: Option<String>,
    extra: Option<(String, String)>,
    name: String,
}

struct Located {
    shape: Shape,
    span: (usize, usize),
    index: usize,
}

fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn key_of(raw: &str) -> Option<&'static str> {
    let (base, ignoring) = match raw.strip_suffix("IgnoreCase").or_else(|| raw.strip_suffix("IgnoringCase")) {
        Some(base) => (base, true),
        None => (raw, false),
    };
    Some(match (base, ignoring) {
        ("", false) | ("Is" | "Equals", false) => "is",
        ("", true) => "ignoreCase",
        ("Not" | "IsNot", false) => "not",
        ("Like" | "IsLike", false) => "like",
        ("Containing" | "Contains" | "IsContaining", false) => "containing",
        ("Containing" | "Contains" | "IsContaining", true) => "containingIgnoreCase",
        ("StartingWith" | "StartsWith" | "IsStartingWith", false) => "startingWith",
        ("StartingWith" | "StartsWith" | "IsStartingWith", true) => "startingWithIgnoreCase",
        ("EndingWith" | "EndsWith" | "IsEndingWith", false) => "endingWith",
        ("EndingWith" | "EndsWith" | "IsEndingWith", true) => "endingWithIgnoreCase",
        ("GreaterThan" | "IsGreaterThan" | "After" | "IsAfter", false) => "greaterThan",
        ("LessThan" | "IsLessThan" | "Before" | "IsBefore", false) => "lessThan",
        ("Between" | "IsBetween", false) => "between",
        ("True" | "IsTrue", false) => "true",
        ("False" | "IsFalse", false) => "false",
        ("In" | "IsIn", false) => "in",
        ("IsNull" | "Null", false) => "isNull",
        _ => return None,
    })
}

fn returns_of(text: &str, entity: &str, kotlin: bool) -> Option<Returns> {
    let wrapped = |wrapper: &str| text == format!("{wrapper}<{entity}>");
    if wrapped("List") {
        return Some(Returns::List);
    }
    if wrapped("Optional") {
        return Some(Returns::Optional);
    }
    if wrapped("Page") {
        return Some(Returns::Page);
    }
    if wrapped("Slice") {
        return Some(Returns::Slice);
    }
    if wrapped("Stream") {
        return Some(Returns::Stream);
    }
    if text == if kotlin { format!("{entity}?") } else { entity.to_owned() } {
        return Some(Returns::Single);
    }
    match (kotlin, text) {
        (false, "long" | "Long" | "boolean" | "Boolean") | (true, "Long" | "Boolean") => Some(Returns::Number),
        (false, "void") | (true, "") => Some(Returns::Nothing),
        _ => None,
    }
}

fn returns_valid(subject: Subject, returns: Returns) -> bool {
    match subject {
        Subject::Find => !matches!(returns, Returns::Number | Returns::Nothing),
        Subject::Count | Subject::Exists => returns == Returns::Number,
        Subject::Delete => matches!(returns, Returns::Number | Returns::Nothing),
    }
}

fn parse_order(segment: &str, attrs: &[Attr]) -> Option<Order> {
    attrs.iter().find_map(|attr| {
        let rest = segment.strip_prefix(&capitalize(&attr.name))?;
        let (desc, explicit) = match rest {
            "" => (false, false),
            "Asc" => (false, true),
            "Desc" => (true, true),
            _ => return None,
        };
        Some(Order { attr: attr.name.clone(), desc, explicit })
    })
}

fn parse_predicate(predicate: &str, entity: &str, attrs: &[Attr], lookup: queries::Props) -> Option<(Vec<(String, &'static str, String)>, bool, Option<Order>)> {
    let mut splits: Vec<Option<usize>> = predicate.match_indices("OrderBy").map(|(at, _)| Some(at)).collect();
    splits.push(None);
    for split in splits {
        let (conditions, order) = match split {
            Some(at) => (&predicate[..at], parse_order(&predicate[at + "OrderBy".len()..], attrs)),
            None => (predicate, None),
        };
        if split.is_some() && order.is_none() {
            continue;
        }
        let parsed = queries::parse(conditions, entity, lookup);
        if !parsed.complete(conditions) && !conditions.is_empty() {
            continue;
        }
        let mut found = Vec::new();
        for clause in &parsed.clauses {
            let [prop] = clause.path.as_slice() else { return None };
            let key = key_of(&clause.operator)?;
            found.push((prop.name.clone(), key, clause.operator.clone()));
        }
        let rebuilt = |separator: &str| found.iter().map(|(name, _, text)| format!("{}{text}", capitalize(name))).collect::<Vec<_>>().join(separator);
        let any = if rebuilt("And") == conditions {
            false
        } else if rebuilt("Or") == conditions {
            true
        } else {
            continue;
        };
        return Some((found, any, order));
    }
    None
}

fn split_parameters(declaration: &str, name_end: usize, kotlin: bool) -> Result<(Vec<(String, String)>, usize), String> {
    let open = name_end + declaration[name_end..].find('(').ok_or("The method has no parameter list.")?;
    let close = closing(declaration.as_bytes(), open).ok_or("The parameter list is not closed.")?;
    let mut found = Vec::new();
    for (_, part) in split_top_level(&declaration[open + 1..close]) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if part.contains('@') {
            return Err("Annotated parameters are not edited.".into());
        }
        let pair = if kotlin {
            part.split_once(':').map(|(name, ty)| (compact(ty), name.trim().to_owned()))
        } else {
            part.rsplit_once(char::is_whitespace).map(|(ty, name)| (compact(ty), name.trim().to_owned()))
        };
        found.push(pair.ok_or("A parameter could not be read.")?);
    }
    Ok((found, close))
}

fn locate(doc: &Document, facts: &Facts, attrs: &[Attr], index: usize) -> Result<Located, String> {
    let kotlin = facts.kotlin;
    let repo = queries::parse_repository(&doc.text, kotlin).ok_or("The repository could not be read.")?;
    let method = repo.methods.get(index).ok_or("The method no longer exists.")?;
    if method.declared_query {
        return Err("Methods with @Query are not edited here.".into());
    }
    let masked = mask(&doc.text);
    let open = masked[repo.name_start..].find('{').map(|at| at + repo.name_start).ok_or("The interface has no body.")?;
    let body_end = closing(masked.as_bytes(), open).ok_or("The interface body is not closed.")?;
    let (start, end) = members(&masked, open + 1, body_end, kotlin, false)
        .into_iter()
        .find(|(start, end)| *start <= method.start && method.end <= *end)
        .ok_or("The method declaration could not be isolated.")?;
    let blanked = without_annotations(&masked, start, end);
    let span = (start + blanked.len() - blanked.trim_start().len(), start + blanked.trim_end().len());
    for annotation in annotations(&masked).iter().filter(|annotation| annotation.start >= start && annotation.start < end) {
        if annotation.start >= span.0 {
            return Err("Annotated parameters are not edited.".into());
        }
        if !SAFE_ANNOTATIONS.contains(&annotation.name) {
            return Err(format!("The method has @{}; only {} are preserved by this edit.", annotation.name, SAFE_ANNOTATIONS.join(", ")));
        }
    }
    let declaration = &doc.text[span.0..span.1];
    if masked[span.0..span.1] != *declaration {
        return Err("Comments inside the declaration would be lost; edit it by hand.".into());
    }
    let name_at = method.start - span.0;
    let (declaration, tail_ok) = match (kotlin, declaration.strip_suffix(';')) {
        (false, Some(inner)) => (inner, true),
        (false, None) => (declaration, false),
        (true, _) => (declaration, true),
    };
    if !tail_ok {
        return Err("The declaration is not a plain abstract method.".into());
    }
    let before = declaration[..name_at].trim();
    if before.starts_with('<') || before.starts_with("fun <") {
        return Err("Generic methods are not edited.".into());
    }
    let (params, close) = split_parameters(declaration, name_at + method.name.len(), kotlin)?;
    let rest = declaration[close + 1..].trim();
    let return_type = if kotlin {
        if before != "fun" {
            return Err("Only plain `fun` declarations are edited.".into());
        }
        match rest {
            "" => String::new(),
            _ => compact(rest.strip_prefix(':').ok_or("Expression bodies and other tails are not edited.")?),
        }
    } else {
        if !rest.is_empty() {
            return Err("A throws clause is not edited.".into());
        }
        compact(before)
    };
    let returns = returns_of(&return_type, &facts.entity, kotlin).ok_or_else(|| format!("The return type `{return_type}` is not one of List, Optional, Page, Slice, Stream or the entity."))?;

    let name = &method.name;
    let (verb, by_end) = queries::split_subject(name).ok_or("The name has no subject.")?;
    let subject = match verb {
        "find" | "read" | "get" | "query" | "search" | "stream" => Subject::Find,
        "count" => Subject::Count,
        "exists" => Subject::Exists,
        _ => Subject::Delete,
    };
    if !returns_valid(subject, returns) {
        return Err("The return type does not suit the subject of the name.".into());
    }
    let head = &name[verb.len()..by_end - 2];
    let (all, head) = head.strip_prefix("All").map_or((false, head), |rest| (true, rest));
    let (limit, limit_text) = match head.strip_prefix("First").or_else(|| head.strip_prefix("Top")) {
        Some(digits) if digits.is_empty() => (Some(1), head.to_owned()),
        Some(digits) => (Some(digits.parse::<u32>().map_err(|_| "The Top/First limit is not a number.")?), head.to_owned()),
        None if head.is_empty() => (None, String::new()),
        None => return Err(format!("`{head}` between the subject and By is descriptive text or Distinct; it is not edited.")),
    };

    let props: Vec<queries::Prop> = attrs.iter().map(|a| queries::Prop { name: a.name.clone(), ty: a.ty.clone(), fqcn: None, target: None }).collect();
    let lookup = |owner: &str| (owner == facts.entity).then(|| props.clone());
    let (found, any, order) = parse_predicate(&name[by_end..], &facts.entity, attrs, &lookup)
        .ok_or("The name cannot be reproduced from its parsed structure (relationship path, mixed And/Or, AllIgnoreCase, several orderings or an operator this edit does not know).")?;

    let mut remaining = params.as_slice();
    let mut conditions = Vec::new();
    for (attr_name, key, text) in found {
        let attr = attrs.iter().find(|attr| attr.name == attr_name).ok_or("The attribute is not a plain scalar attribute.")?;
        if !attr.operators_with(&facts.keywords).iter().any(|operator| operator.key == key) {
            return Err(format!("`{text}` is not valid for `{}` of type {}.", attr.name, attr.ty));
        }
        let expected = condition_params(attr, key, kotlin, &mut Vec::new());
        if remaining.len() < expected.len() || expected.iter().zip(remaining).any(|((_, ty), (actual, _))| compact(ty) != compact(actual)) {
            return Err("The parameters do not match the method name; fix them first.".into());
        }
        let (taken, rest) = remaining.split_at(expected.len());
        remaining = rest;
        conditions.push(Condition { attr: attr_name, key, text: Some(text), params: taken.iter().map(|(ty, name)| (ty.clone(), name.clone())).collect() });
    }
    let extra = match remaining {
        [] => None,
        [(ty, name)] if ty == "Pageable" || ty == "Sort" => Some((ty.clone(), name.clone())),
        _ => return Err("Only a trailing Pageable or Sort parameter besides the conditions is supported.".into()),
    };
    if matches!(returns, Returns::Page | Returns::Slice) && extra.as_ref().is_none_or(|(ty, _)| ty != "Pageable") {
        return Err("A Page or Slice result needs a Pageable parameter.".into());
    }
    let return_text = if kotlin { declaration[close + 1..].trim().strip_prefix(':').map(|ty| ty.trim().to_owned()) } else { Some(before.to_owned()) };
    let shape = Shape { verb: verb.to_owned(), subject, all, limit, limit_text, conditions, any, order, returns, return_text, extra, name: name.clone() };
    Ok(Located { shape, span, index })
}

struct Rendered {
    name: String,
    params: Vec<(String, String)>,
    returned: Option<String>,
    imports: Vec<String>,
}

fn render(facts: &Facts, attrs: &[Attr], old: &Shape, new: &Shape) -> Result<Rendered, String> {
    let kotlin = facts.kotlin;
    if new.conditions.is_empty() && !(new.subject == Subject::Find && new.order.is_some()) {
        return Err("Keep at least one condition (an ordering alone is allowed for find).".into());
    }
    if new.subject != Subject::Find && (new.order != old.order || new.limit != old.limit) {
        return Err("Ordering and Top/First apply to find methods only.".into());
    }
    if new.limit.is_some_and(|limit| limit == 0 || limit > MAX_LIMIT) {
        return Err(format!("Top/First must be between 1 and {MAX_LIMIT}."));
    }
    if matches!(new.returns, Returns::Single | Returns::Optional) && new.limit.is_some_and(|limit| limit > 1) {
        return Err("A single or Optional result cannot combine with Top/First greater than 1.".into());
    }
    if !returns_valid(new.subject, new.returns) {
        return Err("This return type is not valid for the subject of the name.".into());
    }
    let mut predicate = String::new();
    let mut params: Vec<(String, String)> = Vec::new();
    let mut imports = Vec::new();
    let mut used: Vec<String> = Vec::new();
    for (index, condition) in new.conditions.iter().enumerate() {
        let attr = attrs.iter().find(|attr| attr.name == condition.attr).ok_or("The attribute is not a plain scalar attribute.")?;
        let operator = attr
            .operators_with(&facts.keywords)
            .into_iter()
            .find(|operator| operator.key == condition.key)
            .ok_or_else(|| format!("`{}` is not valid for `{}` of type {}.", condition.key, attr.name, attr.ty))?;
        if index > 0 {
            predicate.push_str(if new.any { "Or" } else { "And" });
        }
        predicate.push_str(&capitalize(&attr.name));
        predicate.push_str(condition.text.as_deref().unwrap_or(&operator.suffix));
        if condition.text.is_some() {
            used.extend(condition.params.iter().map(|(_, name)| name.clone()));
            params.extend(condition.params.iter().cloned());
            continue;
        }
        let generated = condition_params(attr, condition.key, kotlin, &mut imports);
        let reuse = generated.len() == condition.params.len();
        for (slot, (generated_name, ty)) in generated.into_iter().enumerate() {
            let mut name = if reuse { condition.params[slot].1.clone() } else { generated_name };
            if used.contains(&name) {
                name = format!("{name}{}", used.len() + 1);
            }
            used.push(name.clone());
            params.push((ty, name));
        }
    }
    if let Some(order) = &new.order {
        let attr = attrs.iter().find(|attr| attr.name == order.attr).ok_or("The ordering attribute is not a plain scalar attribute.")?;
        let direction = match (order.explicit, order.desc) {
            (false, _) => "",
            (true, true) => "Desc",
            (true, false) => "Asc",
        };
        predicate.push_str(&format!("OrderBy{}{direction}", capitalize(&attr.name)));
    }
    let limit_text = if new.limit == old.limit {
        old.limit_text.clone()
    } else {
        match new.limit {
            None => String::new(),
            Some(1) => "First".to_owned(),
            Some(limit) => format!("Top{limit}"),
        }
    };
    let name = format!("{}{}{limit_text}By{predicate}", new.verb, if new.all { "All" } else { "" });

    let extra = match (new.returns, &old.extra) {
        (Returns::Page | Returns::Slice, Some((ty, name))) if ty == "Pageable" => Some((ty.clone(), name.clone())),
        (Returns::Page | Returns::Slice, _) => {
            imports.push(format!("{DOMAIN}.Pageable"));
            Some(("Pageable".to_owned(), "pageable".to_owned()))
        }
        (Returns::List | Returns::Stream, extra) => extra.clone(),
        (_, Some((ty, _))) if ty == "Pageable" => None,
        (_, extra) => extra.clone(),
    };
    if let Some((ty, extra_name)) = extra {
        if used.contains(&extra_name) {
            return Err("A generated parameter name would collide with the paging parameter.".into());
        }
        params.push((ty, extra_name));
    }
    let returned = if new.returns == old.returns {
        old.return_text.clone()
    } else {
        return_text(new.subject, new.returns, &facts.entity, kotlin, &mut imports)
    };
    imports.sort();
    imports.dedup();
    Ok(Rendered { name, params, returned, imports })
}

fn parameter_list(params: &[(String, String)], kotlin: bool) -> String {
    params.iter().map(|(ty, name)| if kotlin { format!("{name}: {ty}") } else { format!("{ty} {name}") }).collect::<Vec<_>>().join(", ")
}

fn edits(doc: &Document, facts: &Facts, attrs: &[Attr], located: &Located, new: &Shape) -> Result<Vec<TextEdit>, String> {
    let old = &located.shape;
    let rendered = render(facts, attrs, old, new)?;
    let types = |params: &[(String, String)]| params.iter().map(|(ty, _)| compact(ty)).collect::<Vec<_>>();
    let old_params: Vec<(String, String)> = old.conditions.iter().flat_map(|c| c.params.clone()).chain(old.extra.clone()).collect();
    if rendered.name == old.name && rendered.params == old_params && rendered.returned == old.return_text {
        return Err("There are no changes to apply.".into());
    }
    let new_types = types(&rendered.params);
    let others = facts.existing.iter().enumerate().filter(|(at, _)| *at != located.index);
    if others.into_iter().any(|(_, (name, existing))| *name == rendered.name && *existing == new_types) {
        return Err(format!("`{}` with the same parameters already exists in this interface.", rendered.name));
    }
    explicit_import_clash(doc, &rendered.imports, &facts.entity)?;
    let kotlin = facts.kotlin;
    let mut text = declaration_text(kotlin, &rendered.name, &parameter_list(&rendered.params, kotlin), rendered.returned.clone());
    let crlf = doc.text.contains("\r\n");
    let mut found = Vec::new();
    let refs: Vec<&str> = rendered.imports.iter().map(String::as_str).collect();
    if let Some(mut import) = import_edit(&doc.text, &refs, kotlin) {
        if crlf {
            import.text = import.text.replace('\n', "\r\n");
        }
        found.push(import.to_text_edit(&doc.text));
    }
    if crlf {
        text = text.replace('\n', "\r\n");
    }
    found.push(Edit { start: located.span.0, end: located.span.1, text }.to_text_edit(&doc.text));
    Ok(found)
}

fn pick_one(title: &str, items: &[PickItem]) -> Result<String, Error> {
    host::pick(title, items)?.ok_or(Error::Cancelled)
}

fn describe(condition: &Condition, index: usize) -> PickItem {
    PickItem::new(&index.to_string(), &format!("{} {}", condition.attr, condition.text.as_deref().filter(|text| !text.is_empty()).unwrap_or("(equals)")))
}

fn choose_operator(attr: &Attr, keywords: &Keywords) -> Result<&'static str, Error> {
    let operators = attr.operators_with(keywords);
    let choices: Vec<_> = operators.iter().map(|operator| PickItem::new(operator.key, operator.label)).collect();
    let key = pick_one(&format!("Operator for {} ({})", attr.name, attr.ty), &choices)?;
    operators.iter().find(|operator| operator.key == key).map(|operator| operator.key).ok_or_else(|| "Choose one of the offered operators.".into())
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    resolve_method(pending, doc, project, None)
}

/// `preselected`: the method to edit when the flow starts from a call of it, so no method is asked for.
pub(super) fn resolve_method(pending: &Pending, doc: &Document, project: &Project, preselected: Option<&str>) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let mut facts = source(doc).map_err(Error::from)?;
    facts.keywords = Keywords::load().map_err(Error::from)?;
    let imports = repository_imports(doc);
    let (class, entity_text) = find_entity(project, &pending.root, &facts.entity, package(doc).as_deref(), &imports)?;
    let attrs = attributes(&class, &entity_text);
    if attrs.is_empty() {
        return Err("The entity has no plain direct scalar attribute to query.".into());
    }
    let repo = queries::parse_repository(&doc.text, facts.kotlin).ok_or("The repository could not be read.")?;
    let found = candidates(doc).unwrap_or_default();
    let index = match found.as_slice() {
        _ if preselected.is_some() => found.iter().copied().find(|at| Some(repo.methods[*at].name.as_str()) == preselected).ok_or("The called method is not a derived query method this editor can change.")?,
        [] => return Err("The interface has no derived query method without @Query.".into()),
        [only] => *only,
        many => {
            let items: Vec<_> = many.iter().map(|at| PickItem::new(&at.to_string(), &repo.methods[*at].name)).collect();
            pick_one("Query method to edit", &items)?.parse::<usize>().ok().filter(|at| many.contains(at)).ok_or("Choose an offered method.")?
        }
    };
    let located = locate(doc, &facts, &attrs, index)?;
    let old = &located.shape;
    let mut new = old.clone();

    if !old.conditions.is_empty() {
        let items: Vec<_> = old.conditions.iter().enumerate().map(|(at, condition)| describe(condition, at)).collect();
        let keep = host::pick_many("Conditions to keep (deselect to remove)", &items)?.ok_or(Error::Cancelled)?;
        new.conditions = old.conditions.iter().enumerate().filter(|(at, _)| keep.contains(&at.to_string())).map(|(_, condition)| condition.clone()).collect();
    }

    let addable: Vec<_> = attrs.iter().map(|attr| PickItem::new(&attr.name, &attr.name).description(&attr.ty)).collect();
    let added = host::pick_many("Add conditions (plain scalar attributes)", &addable)?.ok_or(Error::Cancelled)?;
    let mut seen = Vec::new();
    for name in &added {
        let attr = attrs.iter().find(|attr| attr.name == *name).filter(|_| !seen.contains(name)).ok_or("Choose offered attributes, each once.")?;
        seen.push(name.clone());
        let key = choose_operator(attr, &facts.keywords)?;
        new.conditions.push(Condition { attr: attr.name.clone(), key, text: None, params: Vec::new() });
    }

    let existing: Vec<usize> = (0..new.conditions.len()).filter(|at| new.conditions[*at].text.is_some()).collect();
    if !existing.is_empty() {
        let items: Vec<_> = existing.iter().map(|at| describe(&new.conditions[*at], *at)).collect();
        let change = host::pick_many("Change operator of", &items)?.ok_or(Error::Cancelled)?;
        for at in &existing {
            if !change.contains(&at.to_string()) {
                continue;
            }
            let attr = attrs.iter().find(|attr| attr.name == new.conditions[*at].attr).ok_or("The attribute is not offered.")?;
            let key = choose_operator(attr, &facts.keywords)?;
            if key != new.conditions[*at].key {
                new.conditions[*at].key = key;
                new.conditions[*at].text = None;
            }
        }
    }

    if new.conditions.len() > 1 {
        let mut items = vec![PickItem::new("keep", "Keep the condition order")];
        items.extend(new.conditions.iter().enumerate().map(|(at, condition)| PickItem::new(&at.to_string(), &format!("Move {} first", condition.attr))));
        let moved = pick_one("Condition order", &items)?;
        if moved != "keep" {
            let at = moved.parse::<usize>().ok().filter(|at| *at < new.conditions.len()).ok_or("Choose an offered condition.")?;
            let condition = new.conditions.remove(at);
            new.conditions.insert(0, condition);
        }
        let current = if old.any { "or" } else { "and" };
        let mut combinators = [PickItem::new("and", "And (all conditions)"), PickItem::new("or", "Or (any condition)")];
        combinators.sort_by_key(|item| item.key != current);
        new.any = pick_one("Combine all conditions with", &combinators)? == "or";
    }

    if new.subject == Subject::Find {
        let mut items = vec![PickItem::new("~keep", "Keep the ordering"), PickItem::new("~clear", "No ordering")];
        items.extend(attrs.iter().map(|attr| PickItem::new(&format!("attr:{}", attr.name), &format!("Order by {}", attr.name))));
        let picked = pick_one("Ordering", &items)?;
        match picked.as_str() {
            "~keep" => {}
            "~clear" => new.order = None,
            other => {
                let attr = other.strip_prefix("attr:").and_then(|name| attrs.iter().find(|attr| attr.name == name)).ok_or("Choose an offered attribute.")?;
                let directions = [PickItem::new("asc", "Ascending"), PickItem::new("desc", "Descending")];
                new.order = Some(Order { attr: attr.name.clone(), desc: pick_one("Direction", &directions)? == "desc", explicit: true });
            }
        }
        let shown = old.limit.map(|limit| limit.to_string());
        let typed = host::input("Top/First limit (blank for none)", shown.as_deref())?.ok_or(Error::Cancelled)?;
        let typed = typed.trim();
        new.limit = match typed {
            "" => None,
            _ => Some(typed.parse::<u32>().map_err(|_| Error::from("Enter a whole number for the limit, or leave it blank."))?),
        };
        let returns = [
            ("list", "List", Returns::List),
            ("optional", "Optional", Returns::Optional),
            ("single", "Single entity (null when absent)", Returns::Single),
            ("page", "Page (needs a Pageable parameter)", Returns::Page),
            ("slice", "Slice (needs a Pageable parameter)", Returns::Slice),
            ("stream", "Stream", Returns::Stream),
        ];
        let mut choices: Vec<_> = returns.iter().map(|(key, label, value)| (*value == old.returns, PickItem::new(key, label))).collect();
        choices.sort_by_key(|(current, _)| !current);
        let choices: Vec<_> = choices.into_iter().map(|(_, item)| item).collect();
        let key = pick_one("Return type", &choices)?;
        new.returns = returns.iter().find(|(candidate, ..)| *candidate == key).map(|(.., value)| *value).ok_or("Choose an offered return type.")?;
    } else if new.subject == Subject::Delete {
        let choices = [PickItem::new("number", "Delete count (long)"), PickItem::new("void", "No result (void)")];
        let returns = if pick_one("Return type", &choices)? == "void" { Returns::Nothing } else { Returns::Number };
        if returns != old.returns {
            new.returns = returns;
        }
    }

    current(pending, doc)?;
    let edits = edits(doc, &facts, &attrs, &located, &new)?;
    let after = patched(&doc.text, &edits)?;
    if after.len() > MAX_TEXT {
        return Err("Generated source exceeds the supported bound.".into());
    }
    let rendered = render(&facts, &attrs, old, &new)?;
    if !ui_kit::prompts::confirm_exact(
        "Edit query method?",
        &format!(
            "Target: {}\nSource version: {}\n{} becomes {}\n\n{after}\nOnly the declaration of this method is replaced; annotations, comments and imports stay. The entity {} was found by file name in this project; mappings in XML or inherited attributes are not resolved.",
            doc.path, doc.version, old.name, rendered.name, facts.entity
        ),
    )? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    Ok(FileAction {
        title: format!("Edit query method {}", old.name),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), edits)],
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::Prop;
    use openidea_plugin::mock::MockHost;

    const JAVA_ENTITY: &str = "package shop;\nimport jakarta.persistence.*;\nimport java.math.BigDecimal;\nimport java.time.LocalDate;\n@Entity\npublic class Order {\n    @Id Long id;\n    String number;\n    BigDecimal total;\n    boolean paid;\n    Boolean flagged;\n    LocalDate placedOn;\n    @ManyToOne Customer customer;\n    @Column(nullable = false) String code;\n}\n";
    const KT_ENTITY: &str = "package shop\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\nimport jakarta.persistence.ManyToOne\n@Entity\nclass Order {\n    @Id var id: Long? = null\n    var number: String? = null\n    var total: Int = 0\n    var paid: Boolean = false\n    @ManyToOne var customer: Customer? = null\n}\n";

    fn java_repo(body: &str) -> String {
        format!("package shop;\n\nimport java.util.List;\nimport org.springframework.data.jpa.repository.JpaRepository;\n\npublic interface OrderRepository extends JpaRepository<Order, Long> {{\n{body}}}\n")
    }

    fn kt_repo(body: &str) -> String {
        format!("package shop\n\nimport org.springframework.data.jpa.repository.JpaRepository\n\ninterface OrderRepository : JpaRepository<Order, Long> {{\n{body}}}\n")
    }

    fn drive(kotlin: bool, repo: &str, answers: &[&[&str]]) -> (Result<FileAction, Error>, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let (path, language, entity_path, entity) = if kotlin {
            ("/project/src/main/kotlin/shop/OrderRepository.kt", "kotlin", "src/main/kotlin/shop/Order.kt", KT_ENTITY)
        } else {
            ("/project/src/main/java/shop/OrderRepository.java", "java", "src/main/java/shop/Order.java", JAVA_ENTITY)
        };
        host.add_file(entity_path, entity);
        host.open_editor(path, language, repo, &[]);
        let doc = Document { path: path.into(), language: language.into(), text: repo.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).expect("menu entry");
        for answer in answers {
            host.answer_next_prompt(Some(answer));
        }
        host.answer_next_prompt(Some(&[]));
        (plugin.resolve_file_action(&doc, pending), doc)
    }

    fn applied(doc: &Document, result: Result<FileAction, Error>) -> String {
        let action = result.expect("action");
        let [(path, edits)] = action.edits.as_slice() else { panic!("one document") };
        assert_eq!(path, &doc.path);
        patched(&doc.text, edits).unwrap()
    }

    fn refusal(result: Result<FileAction, Error>) -> String {
        format!("{:?}", result.err().expect("refusal"))
    }

    #[test]
    fn unchanged_answers_refuse_with_no_changes_including_odd_spellings() {
        let cases: &[(&str, &[&[&str]])] = &[
            ("    List<Order> findByNumberAndPaidTrue(String number);\n", &[&["0", "1"], &[], &[], &["keep"], &["and"], &["~keep"], &[""], &["list"]]),
            ("    List<Order> readByNumberEqualsOrTotalGreaterThan(String n, BigDecimal t);\n", &[&["0", "1"], &[], &[], &["keep"], &["or"], &["~keep"], &[""], &["list"]]),
            ("    Optional<Order> findFirstByNumberIsNotOrderByTotalDesc(String number);\n", &[&["0"], &[], &[], &["~keep"], &["1"], &["optional"]]),
            ("    List<Order> findAllByPaidIsTrue();\n", &[&["0"], &[], &[], &["~keep"], &[""], &["list"]]),
        ];
        for (body, answers) in cases {
            let (result, _) = drive(false, &java_repo(body), answers);
            assert!(refusal(result).contains("no changes"), "{body}");
        }
    }

    #[test]
    fn adds_a_condition_switches_to_or_and_keeps_existing_parameter_names() {
        let repo = java_repo("    List<Order> findByNumber(String n);\n");
        let (result, doc) = drive(false, &repo, &[&["0"], &["total"], &["between"], &[], &["keep"], &["or"], &["~keep"], &[""], &["list"]]);
        let text = applied(&doc, result);
        assert!(text.contains("    List<Order> findByNumberOrTotalBetween(String n, BigDecimal totalStart, BigDecimal totalEnd);\n}"), "{text}");
        assert!(text.contains("import java.math.BigDecimal;\n"), "{text}");
        assert_eq!(text.matches("import java.util.List;").count(), 1);
    }

    #[test]
    fn removes_a_condition_and_its_parameters() {
        let repo = java_repo("    List<Order> findByNumberAndTotalBetween(String number, BigDecimal a, BigDecimal b);\n");
        let (result, doc) = drive(false, &repo, &[&["0"], &[], &[], &["~keep"], &[""], &["list"]]);
        assert!(applied(&doc, result).contains("    List<Order> findByNumber(String number);\n"));
    }

    #[test]
    fn changes_an_operator_keeping_the_name_when_the_parameter_count_stays_and_regenerating_otherwise() {
        let repo = java_repo("    List<Order> findByNumber(String needle);\n");
        let (result, doc) = drive(false, &repo, &[&["0"], &[], &["0"], &["containingIgnoreCase"], &["~keep"], &[""], &["list"]]);
        assert!(applied(&doc, result).contains("    List<Order> findByNumberContainingIgnoreCase(String needle);\n"));
        let (result, doc) = drive(false, &repo, &[&["0"], &[], &["0"], &["in"], &["~keep"], &[""], &["list"]]);
        let text = applied(&doc, result);
        assert!(text.contains("    List<Order> findByNumberIn(Collection<String> needle);\n") && text.contains("import java.util.Collection;"), "{text}");
        let repo = java_repo("    List<Order> findByTotalBetween(BigDecimal a, BigDecimal b);\n");
        let (result, doc) = drive(false, &repo, &[&["0"], &[], &["0"], &["greaterThan"], &["~keep"], &[""], &["list"]]);
        assert!(applied(&doc, result).contains("findByTotalGreaterThan(BigDecimal total);"));
    }

    #[test]
    fn nullability_and_case_choices_follow_the_attribute() {
        let repo = java_repo("    List<Order> findByCode(String code);\n");
        let (result, _) = drive(false, &repo, &[&["0"], &[], &["0"], &["isNull"], &["~keep"], &[""], &["list"]]);
        assert!(result.is_err(), "code is not nullable");
        let repo = java_repo("    List<Order> findByFlagged(Boolean flagged);\n");
        let (result, doc) = drive(false, &repo, &[&["0"], &[], &["0"], &["isNull"], &["~keep"], &[""], &["list"]]);
        assert!(applied(&doc, result).contains("List<Order> findByFlaggedIsNull();"));
    }

    #[test]
    fn reorders_conditions_by_moving_one_first() {
        let repo = java_repo("    List<Order> findByNumberAndPaidTrue(String number);\n");
        let (result, doc) = drive(false, &repo, &[&["0", "1"], &[], &[], &["1"], &["and"], &["~keep"], &[""], &["list"]]);
        assert!(applied(&doc, result).contains("List<Order> findByPaidTrueAndNumber(String number);"));
    }

    #[test]
    fn sets_clears_orders_limits_and_changes_the_wrapper_with_pageable_handling() {
        let repo = java_repo("    List<Order> findByNumber(String number);\n");
        let (result, doc) = drive(false, &repo, &[&["0"], &[], &[], &["attr:total"], &["desc"], &["5"], &["page"]]);
        let text = applied(&doc, result);
        assert!(text.contains("    Page<Order> findTop5ByNumberOrderByTotalDesc(String number, Pageable pageable);\n"), "{text}");
        assert!(text.contains("import org.springframework.data.domain.Page;\n") && text.contains("import org.springframework.data.domain.Pageable;\n"));

        let paged = java_repo("    Page<Order> findTop5ByNumberOrderByTotalDesc(String number, Pageable page);\n");
        let (result, doc) = drive(false, &paged, &[&["0"], &[], &[], &["~clear"], &[""], &["optional"]]);
        assert!(applied(&doc, result).contains("    Optional<Order> findByNumber(String number);\n"));
        let (result, doc) = drive(false, &paged, &[&["0"], &[], &[], &["~keep"], &["5"], &["slice"]]);
        assert!(applied(&doc, result).contains("Slice<Order> findTop5ByNumberOrderByTotalDesc(String number, Pageable page);"));
        let (result, doc) = drive(false, &paged, &[&["0"], &[], &[], &["~keep"], &["5"], &["list"]]);
        assert!(applied(&doc, result).contains("List<Order> findTop5ByNumberOrderByTotalDesc(String number, Pageable page);"), "list keeps a Pageable");
        let (result, doc) = drive(false, &paged, &[&["0"], &[], &[], &["~keep"], &["1"], &["single"]]);
        assert!(applied(&doc, result).contains("Order findFirstByNumberOrderByTotalDesc(String number);"), "single drops Pageable");
        let (result, doc) = drive(false, &repo, &[&["0"], &[], &[], &["attr:number"], &["asc"], &["1"], &["single"]]);
        assert!(applied(&doc, result).contains("Order findFirstByNumberOrderByNumberAsc(String number);"));
        let (result, _) = drive(false, &repo, &[&["0"], &[], &[], &["~keep"], &["3"], &["single"]]);
        assert!(refusal(result).contains("Top/First"));
    }

    #[test]
    fn delete_and_count_subjects_edit_conditions_and_delete_result() {
        let repo = java_repo("    long deleteByNumber(String number);\n");
        let (result, doc) = drive(false, &repo, &[&["0"], &["paid"], &["true"], &[], &["keep"], &["or"], &["void"]]);
        assert!(applied(&doc, result).contains("    void deleteByNumberOrPaidTrue(String number);\n"));
        let repo = java_repo("    long countByNumber(String number);\n");
        let (result, doc) = drive(false, &repo, &[&["0"], &[], &["0"], &["startingWith"]]);
        assert!(applied(&doc, result).contains("    long countByNumberStartingWith(String number);\n"));
    }

    #[test]
    fn annotations_comments_and_other_members_around_the_declaration_stay() {
        let repo = java_repo("    // keep me\n    @Transactional(readOnly = true)\n    List<Order> findByNumber(String number); // trailing\n\n    List<Order> findByPaidTrue();\n");
        let (result, doc) = drive(false, &repo, &[&["0"], &["0"], &[], &["0"], &["like"], &["~keep"], &[""], &["list"]]);
        let text = applied(&doc, result);
        assert!(text.contains("    List<Order> findByPaidTrue();\n") && text.contains("    // keep me\n    @Transactional(readOnly = true)\n    List<Order> findByNumberLike(String number); // trailing\n"), "{text}");
    }

    #[test]
    fn kotlin_edits_and_crlf_are_preserved() {
        let repo = kt_repo("    fun findByNumber(number: String): List<Order>\n    fun findByTotal(total: Int): Order?\n");
        let (result, doc) = drive(true, &repo, &[&["0"], &["0"], &[], &["0"], &["containing"], &["~keep"], &[""], &["page"]]);
        let text = applied(&doc, result);
        assert!(text.contains("    fun findByNumberContaining(number: String, pageable: Pageable): Page<Order>\n"), "{text}");
        assert!(text.contains("import org.springframework.data.domain.Pageable\n") && !text.contains(";"), "{text}");
        let repo = kt_repo("    fun findByTotal(total: Int): Order?\n");
        let (result, doc) = drive(true, &repo, &[&["0"], &[], &["0"], &["between"], &["~keep"], &[""], &["single"]]);
        assert!(applied(&doc, result).contains("    fun findByTotalBetween(totalStart: Int, totalEnd: Int): Order?\n"));
        let repo = kt_repo("    fun deleteByPaid(paid: Boolean)\n");
        let (result, doc) = drive(true, &repo, &[&["0"], &[], &["0"], &["true"], &["void"]]);
        assert!(applied(&doc, result).contains("    fun deleteByPaidTrue()\n}"));

        let repo = java_repo("    List<Order> findByNumber(String number);\n").replace('\n', "\r\n");
        let (result, doc) = drive(false, &repo, &[&["0"], &["paid"], &["true"], &[], &["keep"], &["and"], &["~keep"], &[""], &["optional"]]);
        let text = applied(&doc, result);
        assert!(text.contains("    Optional<Order> findByNumberAndPaidTrue(String number);\r\n}"), "{text:?}");
        assert!(text.contains("import java.util.Optional;\r\n"));
        assert!(!text.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn refuses_what_it_cannot_round_trip_or_preserve() {
        let cases: &[(&str, &str)] = &[
            ("    @Modifying\n    long deleteByNumber(String number);\n", "@Modifying"),
            ("    List<Order> findByNumberAndPaidTrueOrTotalBetween(String n, BigDecimal a, BigDecimal b);\n", "parsed structure"),
            ("    List<Order> findByCustomerName(String n);\n", "parsed structure"),
            ("    List<Order> findByNumberAndPaidTrueAllIgnoreCase(String n);\n", "parsed structure"),
            ("    List<Order> findByNumberOrderByTotalAscCodeDesc(String n);\n", "parsed structure"),
            ("    List<Order> findDistinctByNumber(String n);\n", "Distinct"),
            ("    List<Order> findByNumber(Integer wrong);\n", "do not match"),
            ("    List<Order> findByNumber(String n, String extra);\n", "trailing Pageable"),
            ("    List<Order> findByNumber(@Param(\"n\") String n);\n", "Annotated parameters"),
            ("    Collection<Order> findByNumber(String n);\n", "return type"),
            ("    <T> List<T> findByNumber(String n);\n", "Generic"),
            ("    List<Order> findByNumber(/* why */ String n);\n", "Comments"),
            ("    List<Order> findByNumberGreaterThanEqual(String n);\n", "parsed structure"),
        ];
        for (body, expected) in cases {
            let repo = java_repo(body);
            let (result, _) = drive(false, &repo, &[&["0"], &[], &[], &["~keep"], &[""], &["list"]]);
            let message = refusal(result);
            assert!(message.contains(expected), "{body}: {message}");
        }
        let (result, _) = drive(false, &java_repo("    default List<Order> findByNumber(String n) { return null; }\n    List<Order> findByPaidTrue();\n"), &[&["0"], &[], &["0"], &["false"]]);
        assert!(result.is_err(), "default body method is not a candidate");
    }

    #[test]
    fn refuses_collisions_empty_results_and_bad_limits_and_cancellation() {
        let repo = java_repo("    List<Order> findByNumber(String number);\n    List<Order> findByNumberLike(String number);\n");
        let (result, _) = drive(false, &repo, &[&["0"], &["0"], &[], &["0"], &["like"], &["~keep"], &[""], &["list"]]);
        assert!(refusal(result).contains("already exists"));
        let one = java_repo("    List<Order> findByNumber(String number);\n");
        let (result, _) = drive(false, &one, &[&[], &[], &["~keep"], &[""], &["list"]]);
        assert!(refusal(result).contains("at least one condition"));
        let (result, _) = drive(false, &one, &[&["0"], &[], &[], &["~keep"], &["abc"], &["list"]]);
        assert!(refusal(result).contains("whole number"));
        let (result, _) = drive(false, &one, &[&["0"], &[], &[], &["~keep"], &["0"], &["list"]]);
        assert!(refusal(result).contains("between 1"));
    }

    #[test]
    fn not_offered_outside_a_repository_with_derived_methods() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let repo = java_repo("    @Query(\"x\")\n    List<Order> custom();\n");
        host.open_editor("/project/src/main/java/shop/OrderRepository.java", "java", &repo, &[]);
        let doc = Document { path: "/project/src/main/java/shop/OrderRepository.java".into(), language: "java".into(), text: repo, version: 1 };
        assert!(JpaAssist::default().file_actions(&doc, Range::default()).iter().all(|a| a.title != TITLE));
    }

    #[test]
    fn edited_names_round_trip_through_the_parser_and_the_repository_inspector() {
        let repo = java_repo("    List<Order> findByNumber(String number);\n");
        let (result, doc) = drive(false, &repo, &[&["0"], &["paid", "total"], &["false"], &["lessThan"], &["0"], &["containing"], &["1"], &["or"], &["attr:total"], &["asc"], &["4"], &["list"]]);
        let text = applied(&doc, result);
        assert!(text.contains("List<Order> findTop4ByPaidFalseOrNumberContainingOrTotalLessThanOrderByTotalAsc(String number, BigDecimal total);"), "{text}");
        let class = parse_class(JAVA_ENTITY, "/project/src/main/java/shop/Order.java").unwrap();
        let props: Vec<Prop> = attributes(&class, JAVA_ENTITY).iter().map(|a| Prop { name: a.name.clone(), ty: a.ty.clone(), fqcn: None, target: None }).collect();
        let lookup = |owner: &str| (owner == "Order").then(|| props.clone());
        let parsed = queries::parse_repository(&text, false).unwrap();
        assert!(crate::checks::inspect_repository(&parsed, Some("Long"), &lookup).is_empty());
        let located_again = parsed.methods.iter().find(|m| m.name.starts_with("findTop4")).unwrap();
        assert_eq!(queries::problem(&located_again.name, "Order", &lookup), None);
    }

    #[test]
    fn configured_keywords_apply_to_new_conditions_only() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.set_setting("queryNegationKeyword", "\"IsNot\"");
        host.set_setting("queryBooleanStyle", "\"IsTrue\"");
        let repo = java_repo("    List<Order> findByNumberNot(String number);\n");
        host.add_file("src/main/java/shop/Order.java", JAVA_ENTITY);
        host.open_editor("/project/src/main/java/shop/OrderRepository.java", "java", &repo, &[]);
        let doc = Document { path: "/project/src/main/java/shop/OrderRepository.java".into(), language: "java".into(), text: repo, version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        for answer in [&["0"] as &[&str], &["paid"], &["true"], &[], &["keep"], &["and"], &["~keep"], &[""], &["list"], &[]] {
            host.answer_next_prompt(Some(answer));
        }
        let text = applied(&doc, plugin.resolve_file_action(&doc, pending));
        assert!(text.contains("findByNumberNotAndPaidIsTrue(String number)"), "{text}");
    }
}
