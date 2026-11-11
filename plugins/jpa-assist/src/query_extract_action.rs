//! Reviewed conversion of one Spring Data derived query method into an explicit `@Query` (JPQL with
//! named parameters and `@Param`). Only the invoking repository document is edited.
//!
//! Verified against the Spring Data JPA reference (query-methods, query keywords reference) and the
//! Jakarta Persistence 3.1 specification (chapter 4 grammar):
//! - `@Query` named parameters bind through `@Param("name")`; `@Modifying` is required for
//!   update/delete queries; `countQuery` is the documented attribute for `Page` results.
//! - Derived `Is`/`Not` turn a null argument into `IS NULL`/`IS NOT NULL`; JPQL `=`/`<>` against a
//!   null parameter is unknown. Such conditions are converted only for arguments that cannot be null.
//! - Derived `Containing`/`StartingWith`/`EndingWith` escape wildcards in the argument. The reference
//!   documents no escaping for `like %:p%` in `@Query` (only a SpEL `escape()` form), so they are refused.
//! - Derived `IgnoreCase` is `UPPER(x) = UPPER(?1)`; derived `Like` binds the argument as written.
//! - JPQL: `<>`, `BETWEEN`, `IN` with a collection-valued parameter without parentheses, `TRUE`/`FALSE`
//!   literals, `DELETE FROM Entity alias` and `ORDER BY` are grammar productions. The exists form
//!   `case when count(e) > 0 then true else false end` is grammar-valid (CASE with boolean_expression
//!   result) but the specification has no example of it.
//! - Derived `deleteBy` loads and removes entities one by one; a bulk JPQL delete does not.
use super::query_method_action::{Attr, AttrKind, Facts, attributes, explicit_import_clash, find_entity, repository_imports};
use super::*;
use crate::{
    model::{self, capitalize, closing, import_edit, members, split_top_level, without_annotations},
    queries,
};

pub(super) const TITLE: &str = "Convert derived query to @Query…";
const JPA: &str = "org.springframework.data.jpa.repository";
const PARAM: &str = "org.springframework.data.repository.query.Param";
const PRIMITIVES: [&str; 8] = ["int", "long", "short", "byte", "boolean", "double", "float", "char"];

fn verb_of(name: &str) -> Option<(&'static str, usize)> {
    let (verb, predicate) = queries::split_subject(name)?;
    matches!(verb, "find" | "count" | "exists" | "delete").then_some((verb, predicate))
}

pub(super) fn eligible(doc: &Document) -> bool {
    super::query_method_action::source(doc).is_ok()
        && queries::parse_repository(&doc.text, doc.language == "kotlin")
            .is_some_and(|repo| repo.methods.iter().any(|m| !m.declared_query && verb_of(&m.name).is_some()))
}

struct Param {
    name: String,
    ty: String,
    insert_at: usize,
}

struct Plan {
    query: String,
    count_query: Option<String>,
    modifying: bool,
    params: Vec<Param>,
    member_start: usize,
    name_span: (usize, usize),
    has_modifying: bool,
    notes: Vec<String>,
}

fn keyword(rest: &str, word: &str) -> bool {
    rest.starts_with(word) && rest[word.len()..].chars().next().is_none_or(|c| c.is_uppercase())
}

type Conditions<'a> = Vec<(&'a Attr, String)>;

fn split_predicate<'a>(predicate: &str, attrs: &'a [Attr]) -> Result<(Conditions<'a>, Vec<(&'a Attr, bool)>, bool), String> {
    let props: Vec<queries::Prop> =
        attrs.iter().map(|a| queries::Prop { name: a.name.clone(), ty: a.ty.clone(), fqcn: None, target: None }).collect();
    let lookup = |owner: &str| (owner == "ENTITY").then(|| props.clone());
    if predicate.contains("AllIgnoreCase") || predicate.contains("AllIgnoringCase") {
        return Err("AllIgnoreCase is not converted.".into());
    }
    let whole = queries::parse(predicate, "ENTITY", &lookup);
    if !whole.complete(predicate) {
        return Err("The method name does not parse into direct scalar attributes of the entity (relationship paths are refused).".into());
    }
    let cut = predicate.match_indices("OrderBy").map(|(at, _)| at).find(|&at| {
        let head = &predicate[..at];
        at == 0 || {
            let parsed = queries::parse(head, "ENTITY", &lookup);
            parsed.complete(head) && !parsed.ordered
        }
    });
    let (head, tail) = match cut {
        Some(at) => (&predicate[..at], Some(&predicate[at + "OrderBy".len()..])),
        None => (predicate, None),
    };
    let mut conditions: Conditions = Vec::new();
    let mut any = false;
    if !head.is_empty() {
        let parsed = queries::parse(head, "ENTITY", &lookup);
        for clause in &parsed.clauses {
            let [prop] = clause.path.as_slice() else {
                return Err("Only direct attributes are converted.".into());
            };
            let attr = attrs.iter().find(|a| a.name == prop.name).ok_or("Unknown attribute.")?;
            conditions.push((attr, clause.operator.clone()));
        }
        let texts: Vec<String> = conditions.iter().map(|(a, op)| format!("{}{op}", capitalize(&a.name))).collect();
        match ["And", "Or"].iter().position(|joiner| texts.join(joiner) == head) {
            Some(at) => any = at == 1,
            None => return Err("Mixing And and Or is not converted: operator precedence is not verified.".into()),
        }
    }
    let mut order = Vec::new();
    let mut rest = tail.unwrap_or_default();
    while !rest.is_empty() {
        let attr = attrs
            .iter()
            .filter(|a| keyword(rest, &capitalize(&a.name)))
            .max_by_key(|a| a.name.len())
            .ok_or("The ordering is not made of direct attributes.")?;
        rest = &rest[attr.name.len()..];
        let descending = keyword(rest, "Desc");
        if descending || keyword(rest, "Asc") {
            rest = &rest[if descending { 4 } else { 3 }..];
        }
        order.push((attr, descending));
    }
    Ok((conditions, order, any))
}

fn render_condition(attr: &Attr, operator: &str, params: &mut std::vec::IntoIter<&Param>, kotlin: bool) -> Result<String, String> {
    let (core, ignoring) = ["IgnoreCase", "IgnoringCase"]
        .iter()
        .find_map(|suffix| operator.strip_suffix(suffix).map(|core| (core, true)))
        .unwrap_or((operator, false));
    let core = match core.strip_prefix("Is") {
        Some(rest) if rest.is_empty() || rest.starts_with(char::is_uppercase) => rest,
        _ => core,
    };
    let path = format!("e.{}", attr.name);
    let mut next = || params.next().ok_or_else(|| "The method has fewer parameters than conditions.".to_owned());
    let nullable = |ty: &str| if kotlin { ty.ends_with('?') } else { !PRIMITIVES.contains(&ty) };
    let simple = |attr: &Attr, symbol: &str, param: &Param, null_sensitive: bool| {
        if null_sensitive && nullable(&param.ty) {
            return Err(format!(
                "`{}` may be null: a derived Is/Not condition turns null into IS NULL/IS NOT NULL, but JPQL `{symbol}` against a null parameter matches nothing. Use a primitive or a non-null Kotlin type.",
                param.name
            ));
        }
        Ok(format!("{} {symbol} :{}", path_of(attr), param.name))
    };
    if ignoring && !matches!(core, "" | "Equals") {
        return Err(format!("IgnoreCase is converted only for plain equality, not `{operator}`."));
    }
    if ignoring && attr.kind != AttrKind::Text {
        return Err(format!("IgnoreCase on `{}` ({}) is not a String comparison.", attr.name, attr.ty));
    }
    match core {
        "" | "Equals" if ignoring => Ok(format!("upper({path}) = upper(:{})", next()?.name)),
        "" | "Equals" => simple(attr, "=", next()?, true),
        "Not" => simple(attr, "<>", next()?, true),
        "Like" if attr.kind == AttrKind::Text => simple(attr, "like", next()?, false),
        "GreaterThan" if attr.kind == AttrKind::Comparable => simple(attr, ">", next()?, false),
        "LessThan" if attr.kind == AttrKind::Comparable => simple(attr, "<", next()?, false),
        "Between" if attr.kind == AttrKind::Comparable => {
            let (low, high) = (next()?.name.clone(), next()?.name.clone());
            Ok(format!("{path} between :{low} and :{high}"))
        }
        "In" if attr.kind != AttrKind::Boolean => {
            let param = next()?;
            let ty = param.ty.split('<').next().unwrap_or_default();
            if !matches!(ty, "Collection" | "List" | "Set") || !param.ty.ends_with('>') {
                return Err(format!("`{}` must be a Collection, List or Set parameter.", param.name));
            }
            Ok(format!("{path} in :{}", param.name))
        }
        "Null" => Ok(format!("{path} is null")),
        "True" if attr.kind == AttrKind::Boolean => Ok(format!("{path} = true")),
        "False" if attr.kind == AttrKind::Boolean => Ok(format!("{path} = false")),
        "StartingWith" | "StartsWith" | "EndingWith" | "EndsWith" | "Containing" | "Contains" => Err(format!(
            "`{operator}` is refused: derived queries escape wildcards in the argument, and the Spring Data reference documents no equivalent escaping for `like %:param%` in @Query (only a SpEL escape() form)."
        )),
        _ => Err(format!("Operator `{operator}` on `{}` ({}) is not converted.", attr.name, attr.ty)),
    }
}

fn path_of(attr: &Attr) -> String {
    format!("e.{}", attr.name)
}

fn plan(doc: &Document, facts: &Facts, attrs: &[Attr], entity_text: &str, name: &str) -> Result<Plan, String> {
    let kotlin = facts.kotlin;
    let masked = mask(&doc.text);
    let repo = queries::parse_repository(&doc.text, kotlin).ok_or("Not a repository.")?;
    let method = repo.methods.iter().find(|m| m.name == name).ok_or("The method is gone.")?;
    if repo.methods.iter().filter(|m| m.name == name).count() != 1 {
        return Err("Overloaded methods are not converted.".into());
    }
    if method.declared_query {
        return Err("The method already has @Query (or another query annotation).".into());
    }
    let (verb, predicate_at) = verb_of(name).ok_or("Only find/count/exists/delete derived methods are converted.")?;
    if name[verb.len()..predicate_at - 2].len() > 0 {
        return Err("Top/First/Distinct subjects are not converted (their JPQL equivalence is not verified).".into());
    }
    let entity_masked = mask(entity_text);
    if model::annotations(&entity_masked).iter().any(|a| {
        a.name == "Entity" && a.args.is_some_and(|(start, end)| entity_masked[start..end].contains("name"))
    }) {
        return Err("The entity declares an explicit @Entity name; the JPQL entity name is not guessed.".into());
    }

    let open = masked[repo.name_start..].find('{').map(|at| at + repo.name_start).ok_or("The interface has no body.")?;
    let close = closing(masked.as_bytes(), open).ok_or("The interface body is not closed.")?;
    let (member_start, member_end) = members(&masked, open + 1, close, kotlin, false)
        .into_iter()
        .find(|&(start, end)| method.start >= start && method.start < end)
        .ok_or("The method declaration was not located.")?;
    let leading = masked[member_start..member_end].len() - masked[member_start..member_end].trim_start().len();
    let member_start = member_start + leading;
    let line_start = doc.text[..member_start].rfind('\n').map_or(0, |at| at + 1);
    if !doc.text[line_start..member_start].trim().is_empty() {
        return Err("The method must start its own line.".into());
    }
    let paren = method.end + masked[method.end..].len() - masked[method.end..].trim_start().len();
    if masked.as_bytes().get(paren) != Some(&b'(') {
        return Err("Generic or unusual method declarations are not converted.".into());
    }
    let paren_close = closing(masked.as_bytes(), paren).ok_or("Unbalanced parameter list.")?;

    let mut params = Vec::new();
    for (offset, part) in split_top_level(&masked[paren + 1..paren_close]).into_iter().filter(|(_, p)| !p.trim().is_empty()) {
        if part.contains('@') {
            return Err("A parameter is already annotated; nothing is rewritten.".into());
        }
        let insert_at = paren + 1 + offset + part.len() - part.trim_start().len();
        let compact = |text: &str| text.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        let (pname, ty) = if kotlin {
            let (pname, ty) = part.split_once(':').ok_or("Unsupported parameter.")?;
            if pname.trim().contains(char::is_whitespace) {
                return Err("Modified (val/var/vararg) parameters are not converted.".into());
            }
            (pname.trim().to_owned(), compact(ty))
        } else {
            let (ty, pname) = part.trim().rsplit_once(char::is_whitespace).ok_or("Unsupported parameter.")?;
            let ty = ty.trim().strip_prefix("final ").unwrap_or(ty.trim());
            if ty.contains("...") {
                return Err("Varargs are not converted.".into());
            }
            (pname.trim().to_owned(), compact(ty))
        };
        if !identifier(&pname) {
            return Err(format!("Parameter name `{pname}` is not a plain identifier."));
        }
        params.push(Param { name: pname, ty, insert_at });
    }
    let (special, fixed) = match params.split_last() {
        Some((last, rest)) if matches!(last.ty.rsplit('.').next(), Some("Pageable" | "Sort")) => (Some(last), rest),
        _ => (None, params.as_slice()),
    };
    if let [.., before, last] = params.as_slice() {
        let kind = |p: &Param| p.ty.rsplit('.').next().map(str::to_owned);
        if matches!((kind(before).as_deref(), kind(last).as_deref()), (Some("Pageable"), Some("Sort")) | (Some("Sort"), Some("Pageable"))) {
            return Err("Pageable and Sort together are not converted.".into());
        }
    }

    let (conditions, order, any) = split_predicate(&name[predicate_at..], attrs)?;
    if conditions.is_empty() && !(verb == "find" && !order.is_empty()) {
        return Err("The method has no conditions to convert.".into());
    }
    if verb != "find" && (!order.is_empty() || special.is_some()) {
        return Err("Ordering, Pageable and Sort are converted for find methods only.".into());
    }
    let mut remaining = fixed.iter().collect::<Vec<_>>().into_iter();
    let mut rendered = Vec::new();
    for (attr, operator) in &conditions {
        rendered.push(render_condition(attr, operator, &mut remaining, kotlin)?);
    }
    if remaining.next().is_some() {
        return Err("The method has more parameters than its conditions consume.".into());
    }

    let tail_type = if kotlin {
        masked[paren_close + 1..member_end].trim().trim_start_matches(':').trim().trim_end_matches(';').trim().to_owned()
    } else {
        let head = without_annotations(&masked, member_start, method.start);
        head.split_whitespace().filter(|w| !matches!(*w, "public" | "default" | "abstract")).collect::<Vec<_>>().join("")
    };
    let returned: String = tail_type.chars().filter(|c| !c.is_whitespace()).collect();
    let entity = &facts.entity;
    let wraps = |wrapper: &str| returned == format!("{wrapper}<{entity}>");
    let (is_page, valid) = match verb {
        "find" => (
            wraps("Page"),
            wraps("List") || wraps("Optional") || wraps("Page") || wraps("Slice") || wraps("Stream") || returned == *entity || returned == format!("{entity}?"),
        ),
        "count" => (false, matches!(returned.as_str(), "long" | "Long")),
        "exists" => (false, matches!(returned.as_str(), "boolean" | "Boolean")),
        _ => (false, matches!(returned.as_str(), "void" | "" | "Unit" | "int" | "Integer" | "Int")),
    };
    if !valid {
        return Err(format!("Return type `{returned}` is not converted for {verb} (long delete counts are not verified for @Modifying)."));
    }
    if (is_page || wraps("Slice")) && special.is_none_or(|p| !p.ty.ends_with("Pageable")) {
        return Err("A Page or Slice result needs a Pageable parameter.".into());
    }

    let joiner = if any { " or " } else { " and " };
    let predicate = if rendered.is_empty() { String::new() } else { format!(" where {}", rendered.join(joiner)) };
    let order_by = if order.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = order.iter().map(|(a, desc)| format!("e.{} {}", a.name, if *desc { "desc" } else { "asc" })).collect();
        format!(" order by {}", items.join(", "))
    };
    let from = format!("from {entity} e{predicate}");
    let count = format!("select count(e) {from}");
    let (query, mut notes) = match verb {
        "count" => (count.clone(), vec![]),
        "exists" => (format!("select case when count(e) > 0 then true else false end {from}"), vec![
            "The exists form is grammar-valid in Jakarta Persistence 3.1 but the specification has no example of it; it was not run against a provider.".into(),
        ]),
        "delete" => (format!("delete {from}"), vec![
            "@Modifying is added. A derived deleteBy loads the entities and removes them one by one (Spring Data reference); this bulk JPQL delete does not run lifecycle callbacks or cascades, and does not clear the persistence context.".into(),
        ]),
        _ => (format!("select e {from}{order_by}"), vec![]),
    };
    notes.push("Parameters are bound by name through @Param; positional and SpEL forms are not used.".into());
    if rendered.iter().any(|c| c.contains(" like ")) {
        notes.push("Like binds the argument as written (wildcards supplied by the caller), as the derived Like does.".into());
    }
    Ok(Plan {
        query,
        count_query: is_page.then_some(count),
        modifying: verb == "delete",
        params: params.into_iter().filter(|p| !matches!(p.ty.rsplit('.').next(), Some("Pageable" | "Sort"))).collect(),
        member_start,
        name_span: (method.start, method.end),
        has_modifying: model::annotations(&masked[member_start..member_end]).iter().any(|a| a.name == "Modifying"),
        notes,
    })
}

fn edits(doc: &Document, facts: &Facts, plan: &Plan, new_name: &str) -> Result<Vec<TextEdit>, Error> {
    let masked_line_start = doc.text[..plan.member_start].rfind('\n').map_or(0, |at| at + 1);
    let indent = &doc.text[masked_line_start..plan.member_start];
    let eol = if doc.text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut annotation = match &plan.count_query {
        Some(count) => format!("@Query(value = \"{}\", countQuery = \"{count}\")", plan.query),
        None => format!("@Query(\"{}\")", plan.query),
    };
    if plan.modifying && !plan.has_modifying {
        annotation = format!("@Modifying{eol}{indent}{annotation}");
    }
    let mut imports = vec![format!("{JPA}.Query")];
    if plan.modifying && !plan.has_modifying {
        imports.push(format!("{JPA}.Modifying"));
    }
    if !plan.params.is_empty() {
        imports.push(PARAM.to_owned());
    }
    explicit_import_clash(doc, &imports, &facts.entity)?;
    let refs: Vec<&str> = imports.iter().map(String::as_str).collect();
    let mut found = Vec::new();
    if let Some(mut import) = import_edit(&doc.text, &refs, facts.kotlin) {
        import.text = import.text.replace('\n', eol).replace("\r\r", "\r");
        found.push(import.to_text_edit(&doc.text));
    }
    found.push(model::Edit::insert(plan.member_start, &format!("{annotation}{eol}{indent}")).to_text_edit(&doc.text));
    for param in &plan.params {
        found.push(model::Edit::insert(param.insert_at, &format!("@Param(\"{}\") ", param.name)).to_text_edit(&doc.text));
    }
    if new_name != &doc.text[plan.name_span.0..plan.name_span.1] {
        found.push(model::Edit { start: plan.name_span.0, end: plan.name_span.1, text: new_name.into() }.to_text_edit(&doc.text));
    }
    Ok(found)
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let facts = super::query_method_action::source(doc).map_err(Error::from)?;
    let (class, entity_text) = find_entity(project, &pending.root, &facts.entity, package(doc).as_deref(), &repository_imports(doc))?;
    let attrs = attributes(&class, &entity_text);
    let repo = queries::parse_repository(&doc.text, facts.kotlin).ok_or(Error::Stale)?;
    let items: Vec<_> = repo.methods.iter().filter(|m| !m.declared_query && verb_of(&m.name).is_some()).map(|m| PickItem::new(&m.name, &m.name)).collect();
    let name = host::pick("Derived query method to convert", &items)?.ok_or(Error::Cancelled)?;
    let typed = host::input("Method name after conversion (keep the default to leave callers untouched)", Some(&name))?.ok_or(Error::Cancelled)?;
    let new_name = typed.trim();
    if !identifier(new_name) {
        return Err("The new method name is not a plain identifier.".into());
    }
    if new_name != name && repo.methods.iter().any(|m| m.name == new_name) {
        return Err(format!("`{new_name}` already names a method in this interface.").into());
    }
    let plan = plan(doc, &facts, &attrs, &entity_text, &name)?;
    super::index_query_action::query_sanity(&plan.query)?;
    let edits = edits(doc, &facts, &plan, new_name)?;
    let after = patched(&doc.text, &edits)?;
    if after.len() > MAX_TEXT || !crate::checks::inspect_query_parameters_owned(&after, facts.kotlin).is_empty() {
        return Err("The generated source failed the query-parameter check; nothing is changed.".into());
    }
    let notes = plan.notes.join("\n");
    if !ui_kit::prompts::confirm_exact(
        "Convert derived query to @Query?",
        &format!(
            "Target: {}\nSource version: {}\n\n{after}\n{notes}\nOnly this repository source is edited. The entity {} was found by file name; XML mappings, inheritance and the Spring Data version are not read, and nothing was run against a database.",
            doc.path, doc.version, facts.entity
        ),
    )? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    Ok(FileAction {
        title: format!("Convert {name} to @Query"),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), edits)],
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const REPO_PATH: &str = "/project/src/main/java/shop/OrderRepository.java";
    const KT_REPO_PATH: &str = "/project/src/main/kotlin/shop/OrderRepository.kt";
    const JAVA_ENTITY: &str = "package shop;\nimport jakarta.persistence.*;\nimport java.math.BigDecimal;\n@Entity\npublic class Order {\n    @Id Long id;\n    String number;\n    BigDecimal total;\n    boolean paid;\n    int age;\n    Boolean flagged;\n    @ManyToOne Customer customer;\n}\n";
    const KT_ENTITY: &str = "package shop\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\n@Entity\nclass Order {\n    @Id var id: Long? = null\n    var number: String = \"\"\n    var total: Int = 0\n    var paid: Boolean = false\n    var note: String? = null\n}\n";

    fn java_repo(method: &str) -> String {
        format!("package shop;\n\nimport java.util.List;\nimport org.springframework.data.jpa.repository.JpaRepository;\n\npublic interface OrderRepository extends JpaRepository<Order, Long> {{\n    {method}\n}}\n")
    }

    fn kt_repo(method: &str) -> String {
        format!("package shop\n\nimport org.springframework.data.jpa.repository.JpaRepository\n\ninterface OrderRepository : JpaRepository<Order, Long> {{\n    {method}\n}}\n")
    }

    fn drive(kotlin: bool, repo: &str, name: &str, rename: &str, entity: Option<&str>) -> Result<String, String> {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let (path, language, entity_path, default_entity) = if kotlin {
            (KT_REPO_PATH, "kotlin", "src/main/kotlin/shop/Order.kt", KT_ENTITY)
        } else {
            (REPO_PATH, "java", "src/main/java/shop/Order.java", JAVA_ENTITY)
        };
        host.add_file(entity_path, entity.unwrap_or(default_entity));
        host.open_editor(path, language, repo, &[]);
        let doc = Document { path: path.into(), language: language.into(), text: repo.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).ok_or("no menu entry")?;
        host.answer_next_prompt(Some(&[name]));
        host.answer_next_prompt(Some(&[rename]));
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).map_err(|e| format!("{e:?}"))?;
        let [(_, edits)] = action.edits.as_slice() else { return Err("one document".into()) };
        patched(&doc.text, edits).map_err(|e| format!("{e:?}"))
    }

    fn java(method: &str, name: &str) -> Result<String, String> {
        drive(false, &java_repo(method), name, name, None)
    }

    fn query_of(text: &str) -> String {
        let at = text.find("@Query(").unwrap();
        text[at..].lines().next().unwrap().to_owned()
    }

    fn assert_clean(after: &str, kotlin: bool) {
        assert!(crate::checks::inspect_query_parameters_owned(after, kotlin).is_empty(), "{after}");
        let repo = queries::parse_repository(after, kotlin).unwrap();
        assert!(repo.methods.iter().any(|m| m.declared_query), "{after}");
        for literal in after.split('"').skip(1).step_by(2) {
            if literal.starts_with("select") || literal.starts_with("delete") {
                super::super::index_query_action::query_sanity(literal).unwrap();
                let tokens = crate::jpql_language::tokens(literal);
                assert!(tokens.iter().all(|t| t.kind != openidea_plugin::syntax::Highlight::Comment), "{literal}");
            }
        }
    }

    #[test]
    fn java_renders_each_supported_operator_with_param_and_clean_jpql() {
        let cases: &[(&str, &str, &str)] = &[
            ("List<Order> findByAge(int age);", "findByAge", "select e from Order e where e.age = :age"),
            ("List<Order> findByAgeNot(int age);", "findByAgeNot", "select e from Order e where e.age <> :age"),
            ("List<Order> findByNumberLike(String number);", "findByNumberLike", "select e from Order e where e.number like :number"),
            ("List<Order> findByTotalGreaterThan(BigDecimal total);", "findByTotalGreaterThan", "select e from Order e where e.total > :total"),
            ("List<Order> findByTotalLessThan(BigDecimal total);", "findByTotalLessThan", "select e from Order e where e.total < :total"),
            ("List<Order> findByTotalBetween(BigDecimal totalStart, BigDecimal totalEnd);", "findByTotalBetween", "select e from Order e where e.total between :totalStart and :totalEnd"),
            ("List<Order> findByNumberIn(Collection<String> numbers);", "findByNumberIn", "select e from Order e where e.number in :numbers"),
            ("List<Order> findByFlaggedIsNull();", "findByFlaggedIsNull", "select e from Order e where e.flagged is null"),
            ("List<Order> findByPaidTrue();", "findByPaidTrue", "select e from Order e where e.paid = true"),
            ("List<Order> findByPaidFalse();", "findByPaidFalse", "select e from Order e where e.paid = false"),
            ("List<Order> findByNumberIgnoreCase(String number);", "findByNumberIgnoreCase", "select e from Order e where upper(e.number) = upper(:number)"),
            ("List<Order> findByAgeAndPaidTrue(int age);", "findByAgeAndPaidTrue", "select e from Order e where e.age = :age and e.paid = true"),
            ("List<Order> findByAgeOrPaidTrue(int age);", "findByAgeOrPaidTrue", "select e from Order e where e.age = :age or e.paid = true"),
            ("List<Order> findByPaidTrueOrderByTotalDescNumberAsc();", "findByPaidTrueOrderByTotalDescNumberAsc", "select e from Order e where e.paid = true order by e.total desc, e.number asc"),
            ("List<Order> findByOrderByTotal();", "findByOrderByTotal", "select e from Order e order by e.total asc"),
            ("long countByPaidTrue();", "countByPaidTrue", "select count(e) from Order e where e.paid = true"),
            ("boolean existsByAge(int age);", "existsByAge", "select case when count(e) > 0 then true else false end from Order e where e.age = :age"),
            ("void deleteByAge(int age);", "deleteByAge", "delete from Order e where e.age = :age"),
            ("int deleteByPaidTrue();", "deleteByPaidTrue", "delete from Order e where e.paid = true"),
        ];
        for (method, name, jpql) in cases {
            let after = java(method, name).unwrap_or_else(|e| panic!("{method}: {e}"));
            assert!(query_of(&after).contains(&format!("@Query(\"{jpql}\")")), "{method}\n{after}");
            assert_clean(&after, false);
            assert!(after.contains("import org.springframework.data.jpa.repository.Query;"), "{after}");
        }
    }

    #[test]
    fn java_adds_param_modifying_and_imports_once() {
        let after = java("void deleteByAge(int age);", "deleteByAge").unwrap();
        assert!(after.contains("    @Modifying\n    @Query(\"delete from Order e where e.age = :age\")\n    void deleteByAge(@Param(\"age\") int age);"), "{after}");
        assert_eq!(after.matches("import org.springframework.data.jpa.repository.Modifying;").count(), 1);
        assert_eq!(after.matches("import org.springframework.data.repository.query.Param;").count(), 1);
        let existing = java_repo("void deleteByAge(int age);").replace("import java.util.List;", "import org.springframework.data.jpa.repository.Query;\nimport org.springframework.data.repository.query.Param;");
        let after = drive(false, &existing, "deleteByAge", "deleteByAge", None).unwrap();
        assert_eq!(after.matches("repository.Query;").count(), 1);
        assert_eq!(after.matches("query.Param;").count(), 1);
        let wild = java_repo("void deleteByAge(int age);").replace("import java.util.List;", "import org.springframework.data.jpa.repository.*;");
        let after = drive(false, &wild, "deleteByAge", "deleteByAge", None).unwrap();
        assert!(!after.contains("repository.Query;") && !after.contains("repository.Modifying;"), "{after}");
    }

    #[test]
    fn page_gets_count_query_without_order_and_rename_is_optional() {
        let method = "Page<Order> findByAgeOrderByTotalDesc(int age, Pageable pageable);";
        let after = java(method, "findByAgeOrderByTotalDesc").unwrap();
        assert!(after.contains("@Query(value = \"select e from Order e where e.age = :age order by e.total desc\", countQuery = \"select count(e) from Order e where e.age = :age\")"), "{after}");
        assert!(after.contains("Page<Order> findByAgeOrderByTotalDesc(@Param(\"age\") int age, Pageable pageable);"), "{after}");
        let after = drive(false, &java_repo(method), "findByAgeOrderByTotalDesc", "pageOfAge", None).unwrap();
        assert!(after.contains("Page<Order> pageOfAge(@Param(\"age\") int age, Pageable pageable);"), "{after}");
        let slice = java("Slice<Order> findByAge(int age, Pageable pageable);", "findByAge").unwrap();
        assert!(slice.contains("@Query(\"select e from Order e where e.age = :age\")") && !slice.contains("countQuery"));
        let sorted = java("List<Order> findByAge(int age, Sort sort);", "findByAge").unwrap();
        assert!(sorted.contains("findByAge(@Param(\"age\") int age, Sort sort);"), "{sorted}");
    }

    #[test]
    fn crlf_is_preserved() {
        let repo = java_repo("void deleteByAge(int age);").replace('\n', "\r\n");
        let after = drive(false, &repo, "deleteByAge", "deleteByAge", None).unwrap();
        assert!(!after.replace("\r\n", "").contains('\n'), "{after:?}");
        assert!(after.contains("    @Modifying\r\n    @Query("), "{after:?}");
    }

    #[test]
    fn kotlin_renders_param_syntax_and_non_null_equality() {
        let after = drive(true, &kt_repo("fun findByNumber(number: String): List<Order>"), "findByNumber", "findByNumber", None).unwrap();
        assert!(after.contains("    @Query(\"select e from Order e where e.number = :number\")\n    fun findByNumber(@Param(\"number\") number: String): List<Order>"), "{after}");
        assert!(after.contains("import org.springframework.data.repository.query.Param\n") && !after.contains("Param;"), "{after}");
        assert_clean(&after, true);
        let after = drive(true, &kt_repo("fun deleteByPaidTrue()"), "deleteByPaidTrue", "deleteByPaidTrue", None).unwrap();
        assert!(after.contains("@Modifying\n    @Query(\"delete from Order e where e.paid = true\")\n    fun deleteByPaidTrue()"), "{after}");
        let after = drive(true, &kt_repo("fun findByNoteIsNullOrderByTotal(): Order?"), "findByNoteIsNullOrderByTotal", "findByNoteIsNullOrderByTotal", None).unwrap();
        assert!(after.contains("select e from Order e where e.note is null order by e.total asc"), "{after}");
        let after = drive(true, &kt_repo("fun existsByTotal(total: Int): Boolean"), "existsByTotal", "existsByTotal", None).unwrap();
        assert!(after.contains("fun existsByTotal(@Param(\"total\") total: Int): Boolean"), "{after}");
        let after = drive(true, &kt_repo("fun findByTotalBetween(low: Int, high: Int): List<Order>"), "findByTotalBetween", "findByTotalBetween", None).unwrap();
        assert!(after.contains("where e.total between :low and :high") && after.contains("@Param(\"low\") low: Int, @Param(\"high\") high: Int"), "{after}");
        let after = drive(true, &kt_repo("fun findByTotal(total: Int, pageable: Pageable): Page<Order>"), "findByTotal", "findByTotal", None).unwrap();
        assert!(after.contains("countQuery = \"select count(e) from Order e where e.total = :total\""), "{after}");
    }

    fn refused(result: Result<String, String>, reason: &str) {
        let message = result.expect_err("should refuse");
        assert!(message.contains(reason), "expected `{reason}` in `{message}`");
    }

    #[test]
    fn refusals_name_their_reason() {
        refused(java("List<Order> findByNumberContaining(String number);", "findByNumberContaining"), "escape wildcards");
        refused(java("List<Order> findByNumberStartingWith(String number);", "findByNumberStartingWith"), "escape wildcards");
        refused(java("List<Order> findByNumberEndingWith(String number);", "findByNumberEndingWith"), "escape wildcards");
        refused(java("List<Order> findByNumber(String number);", "findByNumber"), "may be null");
        refused(java("List<Order> findByNumberNot(String number);", "findByNumberNot"), "may be null");
        refused(java("List<Order> findByAgeIgnoreCase(int age);", "findByAgeIgnoreCase"), "not a String comparison");
        refused(java("List<Order> findByAgeGreaterThanIgnoreCase(int age);", "findByAgeGreaterThanIgnoreCase"), "plain equality");
        refused(java("List<Order> findByNumberAllIgnoreCase(String number);", "findByNumberAllIgnoreCase"), "AllIgnoreCase");
        refused(java("List<Order> findTop3ByAge(int age);", "findTop3ByAge"), "Top/First");
        refused(java("List<Order> findDistinctByAge(int age);", "findDistinctByAge"), "Distinct");
        refused(java("List<Order> findByAgeAndPaidTrueOrTotalLessThan(int age, BigDecimal total);", "findByAgeAndPaidTrueOrTotalLessThan"), "Mixing");
        refused(java("List<Order> findByCustomerName(String name);", "findByCustomerName"), "does not parse");
        refused(java("List<Order> findByAge(int age, Pageable pageable, Sort sort);", "findByAge"), "Pageable and Sort");
        refused(java("Page<Order> findByAge(int age);", "findByAge"), "Pageable");
        refused(java("long deleteByAge(int age);", "deleteByAge"), "not converted for delete");
        refused(java("List<Order> findByAge(int age, int extra);", "findByAge"), "more parameters");
        refused(java("List<Order> findByAgeBetween(int age);", "findByAgeBetween"), "fewer parameters");
        refused(java("List<Order> findByNumberIn(String[] numbers);", "findByNumberIn"), "Collection");
        refused(java("List<Order> findByPaidTrue(@Nullable String x);", "findByPaidTrue"), "annotated");
        refused(java("List<Order> findByAge(int age);\n    List<Order> findByAge(int age, Sort sort);", "findByAge"), "Overloaded");
        refused(java("OrderSummary findByAge(int age);", "findByAge"), "Return type");
        refused(java("Order readByAge(int age);", "readByAge"), "no menu entry");
    }

    #[test]
    fn existing_query_unknown_entity_named_entity_and_bad_rename_are_refused() {
        let queried = java_repo("@Query(\"select o from Order o\")\n    List<Order> findByAge(int age);");
        assert!(drive(false, &queried, "findByAge", "findByAge", None).is_err());
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let repo = java_repo("List<Order> findByAge(int age);");
        host.open_editor(REPO_PATH, "java", &repo, &[]);
        let doc = Document { path: REPO_PATH.into(), language: "java".into(), text: repo, version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        let message = format!("{:?}", plugin.resolve_file_action(&doc, pending).err().unwrap());
        assert!(message.contains("exactly one entity"), "{message}");
        let named = JAVA_ENTITY.replace("@Entity\n", "@Entity(name = \"Purchase\")\n");
        refused(drive(false, &java_repo("List<Order> findByAge(int age);"), "findByAge", "findByAge", Some(&named)), "explicit @Entity name");
        refused(drive(false, &java_repo("List<Order> findByAge(int age);"), "findByAge", "not valid", None), "plain identifier");
        let two = java_repo("List<Order> findByAge(int age);\n    List<Order> other();");
        refused(drive(false, &two, "findByAge", "other", None), "already names");
    }

    #[test]
    fn menu_entry_requires_a_convertible_derived_method() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        for (repo, expected) in [
            (java_repo("List<Order> findByAge(int age);"), true),
            (java_repo("@Query(\"select o from Order o\")\n    List<Order> findByAge(int age);"), false),
            (java_repo("List<Order> customThing();"), false),
            ("package shop;\n\npublic interface Plain {\n}\n".to_owned(), false),
        ] {
            host.open_editor(REPO_PATH, "java", &repo, &[]);
            let doc = Document { path: REPO_PATH.into(), language: "java".into(), text: repo, version: 1 };
            assert_eq!(JpaAssist::default().file_actions(&doc, Range::default()).iter().any(|a| a.title == TITLE), expected);
        }
    }

    #[test]
    fn cancellation_edits_nothing_and_stale_source_is_refused() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/Order.java", JAVA_ENTITY);
        let repo = java_repo("List<Order> findByAge(int age);");
        host.open_editor(REPO_PATH, "java", &repo, &[]);
        let doc = Document { path: REPO_PATH.into(), language: "java".into(), text: repo, version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, pending.clone()), Err(Error::Cancelled)));
        let changed = Document { version: 2, ..doc };
        assert!(matches!(plugin.resolve_file_action(&changed, pending), Err(Error::Stale)));
    }
}
