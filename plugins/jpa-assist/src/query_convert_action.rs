//! Reviewed rewrites of one Spring Data repository method: async wrapping and dynamic projection.
//! Only the invoking Java repository document is edited.
//!
//! Verified against the Spring Data JPA reference (query-methods-details, projections,
//! query-return-types-reference) and the Spring Framework scheduling chapter:
//! - `@Async` plus `Future<T>`/`CompletableFuture<T>` is the documented async form; the return-type
//!   table says the capability must be enabled, and Spring Framework enables `@Async` through
//!   `@EnableAsync` on a `@Configuration` class (or `<task:annotation-driven>`). Nothing is detected.
//! - The documentation shows `Future<User>` only: `Page`, `Slice`, `Stream`, `Window` and `void`
//!   results are not documented as async and are refused. Kotlin suspend/Deferred are not documented.
//! - The dynamic projection example is `<T> Collection<T> findByLastname(String, Class<T> type)` on a
//!   derived method; no other return shape (`List`, `Optional`, `Page`) and no Kotlin form is
//!   documented, and `@Query` DTO results need constructor expressions, so those are refused.
use super::query_method_action::{explicit_import_clash, source};
use super::*;
use crate::{
    model::{self, closing, import_edit, members, split_top_level, without_annotations},
    queries,
};

pub(super) const TITLE_ASYNC: &str = "Make query method asynchronous…";
pub(super) const TITLE_DYNAMIC: &str = "Make projection dynamic…";
const ASYNC_ANNOTATION: &str = "org.springframework.scheduling.annotation.Async";
const COMPLETABLE_FUTURE: &str = "java.util.concurrent.CompletableFuture";

pub(super) fn eligible(doc: &Document) -> bool {
    doc.language == "java"
        && source(doc).is_ok()
        && queries::parse_repository(&doc.text, false).is_some_and(|repo| !repo.methods.is_empty())
}

#[derive(Debug)]
struct Located {
    member_start: usize,
    indent: String,
    return_span: (usize, usize),
    params_end: usize,
    params: Vec<String>,
    annotations: Vec<String>,
    declared_query: bool,
}

fn locate(doc: &Document, name: &str) -> Result<Located, String> {
    let facts = source(doc)?;
    if facts.kotlin {
        return Err("Kotlin is not converted: the Spring Data reference documents only the Java forms (suspend and Deferred results are not documented).".into());
    }
    let masked = mask(&doc.text);
    let repo = queries::parse_repository(&doc.text, false).ok_or("Not a repository.")?;
    let method = repo.methods.iter().find(|m| m.name == name).ok_or("The method is gone.")?;
    if repo.methods.iter().filter(|m| m.name == name).count() != 1 {
        return Err("Overloaded methods are not converted.".into());
    }
    let open = masked[repo.name_start..].find('{').map(|at| at + repo.name_start).ok_or("The interface has no body.")?;
    let close = closing(masked.as_bytes(), open).ok_or("The interface body is not closed.")?;
    let (start, end) = members(&masked, open + 1, close, false, false)
        .into_iter()
        .find(|&(start, end)| method.start >= start && method.start < end)
        .ok_or("The method declaration was not located.")?;
    let member_start = start + masked[start..end].len() - masked[start..end].trim_start().len();
    let line_start = doc.text[..member_start].rfind('\n').map_or(0, |at| at + 1);
    if !doc.text[line_start..member_start].trim().is_empty() {
        return Err("The method must start its own line.".into());
    }
    if !masked[..end].ends_with(';') {
        return Err("Only abstract methods are converted (default methods are not).".into());
    }
    let annotations: Vec<String> =
        model::annotations(&masked[member_start..method.start]).iter().map(|a| a.name.to_owned()).collect();

    let head = without_annotations(&masked, member_start, method.start);
    let mut return_start = member_start + head.len() - head.trim_start().len();
    let return_end = member_start + head.trim_end().len();
    loop {
        let rest = &masked[return_start..return_end];
        let Some(word) = ["public", "abstract"].into_iter().find(|w| {
            rest.strip_prefix(w).is_some_and(|after| after.starts_with(char::is_whitespace))
        }) else {
            break;
        };
        return_start += word.len() + rest[word.len()..].len() - rest[word.len()..].trim_start().len();
    }
    if return_start >= return_end {
        return Err("The return type was not located.".into());
    }
    if masked[return_start..].starts_with('<') {
        return Err("Generic methods are not converted.".into());
    }
    let paren = method.end + masked[method.end..].len() - masked[method.end..].trim_start().len();
    if masked.as_bytes().get(paren) != Some(&b'(') {
        return Err("Unusual method declarations are not converted.".into());
    }
    let paren_close = closing(masked.as_bytes(), paren).ok_or("Unbalanced parameter list.")?;
    let parts: Vec<_> =
        split_top_level(&masked[paren + 1..paren_close]).into_iter().filter(|(_, part)| !part.trim().is_empty()).collect();
    let params_end = parts.last().map_or(paren + 1, |(offset, part)| paren + 1 + offset + part.trim_end().len());
    Ok(Located {
        member_start,
        indent: doc.text[line_start..member_start].to_owned(),
        return_span: (return_start, return_end),
        params_end,
        params: parts.iter().map(|(_, part)| part.trim().to_owned()).collect(),
        annotations,
        declared_query: method.declared_query,
    })
}

fn squeeze(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn base(ty: &str) -> &str {
    ty.split('<').next().unwrap_or(ty).rsplit('.').next().unwrap_or(ty)
}

fn eol(doc: &Document) -> &'static str {
    if doc.text.contains("\r\n") { "\r\n" } else { "\n" }
}

fn import_edits(doc: &Document, entity: &str, imports: &[&str]) -> Result<Vec<TextEdit>, Error> {
    let owned: Vec<String> = imports.iter().map(|i| (*i).to_owned()).collect();
    explicit_import_clash(doc, &owned, entity)?;
    let mut found = Vec::new();
    if let Some(mut import) = import_edit(&doc.text, imports, false) {
        import.text = import.text.replace('\n', eol(doc)).replace("\r\r", "\r");
        found.push(import.to_text_edit(&doc.text));
    }
    Ok(found)
}

fn async_edits(doc: &Document, name: &str) -> Result<(Vec<TextEdit>, Vec<String>), Error> {
    let facts = source(doc)?;
    let located = locate(doc, name)?;
    if located.annotations.iter().any(|a| a == "Async") {
        return Err("The method is already annotated with @Async.".into());
    }
    if name.starts_with("delete") || name.starts_with("remove") {
        return Err("Delete methods are not converted: asynchronous deletes are not documented.".into());
    }
    let (from, to) = located.return_span;
    let written = doc.text[from..to].to_owned();
    let returned = squeeze(&written);
    match base(&returned) {
        "Future" | "CompletableFuture" | "ListenableFuture" | "Flux" | "Mono" | "Publisher" => {
            return Err("The method already returns an asynchronous or reactive type.".into());
        }
        "void" | "Void" => return Err("A void result is not converted: asynchronous void queries are not documented for Spring Data.".into()),
        "Page" | "Slice" | "Stream" | "Window" | "Streamable" => {
            return Err(format!(
                "`{}` results are not converted: the Spring Data reference documents async only as `Future<User>`/`CompletableFuture<User>`, not for Page, Slice, Stream or Window.",
                base(&returned)
            )
            .into());
        }
        shape
            if shape == facts.entity
                || matches!(
                    shape,
                    "List" | "Collection" | "Set" | "Optional" | "long" | "Long" | "int" | "Integer" | "boolean" | "Boolean"
                ) => {}
        other => return Err(format!("Return type `{other}` is not converted for async.").into()),
    }
    let eol = eol(doc);
    let mut edits = import_edits(doc, &facts.entity, &[ASYNC_ANNOTATION, COMPLETABLE_FUTURE])?;
    edits.push(
        model::Edit::insert(located.member_start, &format!("@Async{eol}{}", located.indent)).to_text_edit(&doc.text),
    );
    let wrapped = format!("CompletableFuture<{}>", queries::boxed(written.trim()));
    edits.push(model::Edit { start: from, end: to, text: wrapped }.to_text_edit(&doc.text));
    let notes = vec![
        "@EnableAsync (or <task:annotation-driven>) must be present in the application for @Async to take effect; the Spring Data return-type table states the async capability must be enabled. Nothing is detected here.".into(),
        "Callers of this method must be updated for the CompletableFuture result.".into(),
    ];
    Ok((edits, notes))
}

fn dynamic_edits(doc: &Document, name: &str) -> Result<(Vec<TextEdit>, Vec<String>), Error> {
    let facts = source(doc)?;
    let located = locate(doc, name)?;
    if located.declared_query {
        return Err("@Query methods are not converted: the reference documents DTO projections for @Query only through JPQL constructor expressions, not dynamic Class parameters.".into());
    }
    if !matches!(queries::split_subject(name), Some(("find", _))) {
        return Err("Only derived find methods are converted.".into());
    }
    let (from, to) = located.return_span;
    if squeeze(&doc.text[from..to]) != format!("Collection<{}>", facts.entity) {
        return Err(format!(
            "Only a `Collection<{}>` result is converted: the reference shows dynamic projections only as `<T> Collection<T> …(…, Class<T> type)`.",
            facts.entity
        )
        .into());
    }
    if facts.entity == "T" {
        return Err("The entity is named T, which clashes with the new type variable.".into());
    }
    for param in &located.params {
        let ty = squeeze(param.rsplit_once(char::is_whitespace).map_or("", |(ty, _)| ty));
        if base(&ty) == "Class" {
            return Err("The method already has a Class parameter.".into());
        }
        if matches!(base(&ty), "Pageable" | "Sort") {
            return Err("Pageable and Sort parameters are not converted: their order relative to the Class parameter is not documented.".into());
        }
    }
    let taken = |candidate: &str| located.params.iter().any(|p| p.rsplit(char::is_whitespace).next() == Some(candidate));
    let parameter = ["type", "projectionType"].into_iter().find(|n| !taken(n)).ok_or("Parameter names `type` and `projectionType` are taken.")?;
    let addition = if located.params.is_empty() { format!("Class<T> {parameter}") } else { format!(", Class<T> {parameter}") };
    let edits = vec![
        model::Edit { start: from, end: to, text: "<T> Collection<T>".into() }.to_text_edit(&doc.text),
        model::Edit::insert(located.params_end, &addition).to_text_edit(&doc.text),
    ];
    let notes = vec![
        format!("Callers must now pass the type: `Class<T> {parameter}` is trailing, as in the documented example; pass {}.class for the entity itself.", facts.entity),
        "The projection type must be an interface projection or a DTO with a single constructor (or @PersistenceCreator); this is not checked.".into(),
    ];
    Ok((edits, notes))
}

fn resolve(
    pending: &Pending,
    doc: &Document,
    title: &str,
    prompt: &str,
    build: fn(&Document, &str) -> Result<(Vec<TextEdit>, Vec<String>), Error>,
) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let facts = source(doc).map_err(Error::from)?;
    let repo = queries::parse_repository(&doc.text, facts.kotlin).ok_or(Error::Stale)?;
    let items: Vec<_> = repo.methods.iter().map(|m| PickItem::new(&m.name, &m.name)).collect();
    let name = host::pick(prompt, &items)?.ok_or(Error::Cancelled)?;
    let (edits, notes) = build(doc, &name)?;
    let after = patched(&doc.text, &edits)?;
    if after.len() > MAX_TEXT {
        return Err("Generated source exceeds the supported source bound.".into());
    }
    if !ui_kit::prompts::confirm_exact(
        &format!("{title}?"),
        &format!(
            "Target: {}\nSource version: {}\n\n{after}\n{}\nOnly this repository source is edited; nothing was compiled or run.",
            doc.path,
            doc.version,
            notes.join("\n")
        ),
    )? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    Ok(FileAction {
        title: format!("{title}: {name}"),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), edits)],
        ..FileAction::default()
    })
}

pub(super) fn resolve_async(pending: &Pending, doc: &Document) -> Result<FileAction, Error> {
    resolve(pending, doc, "Make query method asynchronous", "Query method to make asynchronous", async_edits)
}

pub(super) fn resolve_dynamic(pending: &Pending, doc: &Document) -> Result<FileAction, Error> {
    resolve(pending, doc, "Make projection dynamic", "Query method to make projection dynamic", dynamic_edits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const PATH: &str = "/project/src/main/java/shop/OrderRepository.java";
    const KT_PATH: &str = "/project/src/main/kotlin/shop/OrderRepository.kt";

    fn repo(imports: &str, method: &str) -> String {
        format!("package shop;\n\n{imports}import org.springframework.data.jpa.repository.JpaRepository;\n\npublic interface OrderRepository extends JpaRepository<Order, Long> {{\n    {method}\n}}\n")
    }

    fn run(title: &str, path: &str, language: &str, text: &str, name: &str) -> Result<String, String> {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(path, language, text, &[]);
        let doc = Document { path: path.into(), language: language.into(), text: text.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == title).ok_or("no menu entry")?;
        host.answer_next_prompt(Some(&[name]));
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).map_err(|e| format!("{e:?}"))?;
        let [(_, edits)] = action.edits.as_slice() else { return Err("one document".into()) };
        patched(&doc.text, edits).map_err(|e| format!("{e:?}"))
    }

    fn asynced(imports: &str, method: &str, name: &str) -> Result<String, String> {
        run(TITLE_ASYNC, PATH, "java", &repo(imports, method), name)
    }

    fn dynamic(imports: &str, method: &str, name: &str) -> Result<String, String> {
        run(TITLE_DYNAMIC, PATH, "java", &repo(imports, method), name)
    }

    #[test]
    fn async_wraps_list_and_adds_annotation_and_imports() {
        let after = asynced("import java.util.List;\n", "List<Order> findByNumber(String number);", "findByNumber").unwrap();
        assert!(after.contains("    @Async\n    CompletableFuture<List<Order>> findByNumber(String number);"), "{after}");
        assert!(after.contains("import org.springframework.scheduling.annotation.Async;"), "{after}");
        assert!(after.contains("import java.util.concurrent.CompletableFuture;"), "{after}");
        assert!(after.contains("import java.util.List;"), "{after}");
    }

    #[test]
    fn async_boxes_primitives_and_keeps_query_annotation() {
        let after = asynced("", "@Query(\"select count(o) from Order o\")\n    long countAll();", "countAll").unwrap();
        assert!(after.contains("@Async\n    @Query(\"select count(o) from Order o\")\n    CompletableFuture<Long> countAll();"), "{after}");
    }

    #[test]
    fn async_does_not_duplicate_existing_imports() {
        let imports = "import java.util.concurrent.CompletableFuture;\nimport org.springframework.scheduling.annotation.Async;\n";
        let after = asynced(imports, "Order findByNumber(String number);", "findByNumber").unwrap();
        assert_eq!(after.matches("import java.util.concurrent.CompletableFuture;").count(), 1, "{after}");
        assert_eq!(after.matches("annotation.Async;").count(), 1, "{after}");
    }

    #[test]
    fn async_keeps_crlf() {
        let text = repo("", "Order findByNumber(String number);").replace('\n', "\r\n");
        let after = run(TITLE_ASYNC, PATH, "java", &text, "findByNumber").unwrap();
        assert!(after.contains("    @Async\r\n    CompletableFuture<Order> findByNumber"), "{after:?}");
        assert!(!after.replace("\r\n", "").contains('\n'), "{after:?}");
    }

    #[test]
    fn async_refusals() {
        let page = asynced("import org.springframework.data.domain.*;\n", "Page<Order> findByNumber(String n, Pageable p);", "findByNumber");
        assert!(page.unwrap_err().contains("Page"));
        let slice = asynced("import org.springframework.data.domain.*;\n", "Slice<Order> findByNumber(String n, Pageable p);", "findByNumber");
        assert!(slice.unwrap_err().contains("Slice"));
        let stream = asynced("import java.util.stream.Stream;\n", "Stream<Order> findByNumber(String n);", "findByNumber");
        assert!(stream.unwrap_err().contains("Stream"));
        assert!(asynced("", "void deleteByNumber(String n);", "deleteByNumber").unwrap_err().contains("Delete"));
        assert!(asynced("", "void archiveByNumber(String n);", "archiveByNumber").unwrap_err().contains("void"));
        let already = asynced("import java.util.concurrent.CompletableFuture;\n", "CompletableFuture<Order> findByNumber(String n);", "findByNumber");
        assert!(already.unwrap_err().contains("already"));
        let annotated = asynced("", "@Async\n    Order findByNumber(String n);", "findByNumber");
        assert!(annotated.unwrap_err().contains("already"));
        let overloaded = asynced("", "Order findByNumber(String n);\n    Order findByNumber(int n);", "findByNumber");
        assert!(overloaded.unwrap_err().contains("Overloaded"));
        let unknown = asynced("", "Iterable<Order> findByNumber(String n);", "findByNumber");
        assert!(unknown.unwrap_err().contains("not converted"));
    }

    #[test]
    fn async_refuses_a_clashing_import() {
        let text = asynced("import foo.Async;\n", "Order findByNumber(String n);", "findByNumber");
        assert!(text.unwrap_err().contains("different type"));
    }

    #[test]
    fn kotlin_is_refused_for_both_actions() {
        let kt = "package shop\n\nimport org.springframework.data.jpa.repository.JpaRepository\n\ninterface OrderRepository : JpaRepository<Order, Long> {\n    fun findByNumber(number: String): List<Order>\n}\n";
        for title in [TITLE_ASYNC, TITLE_DYNAMIC] {
            let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
            host.open_editor(KT_PATH, "kotlin", kt, &[]);
            let doc = Document { path: KT_PATH.into(), language: "kotlin".into(), text: kt.into(), version: 1 };
            let mut plugin = JpaAssist::default();
            assert!(plugin.file_actions(&doc, Range::default()).iter().all(|a| a.title != title));
        }
        let doc = Document { path: KT_PATH.into(), language: "kotlin".into(), text: kt.into(), version: 1 };
        assert!(locate(&doc, "findByNumber").unwrap_err().contains("Kotlin"));
    }

    #[test]
    fn dynamic_rewrites_collection_result_with_trailing_class_parameter() {
        let after = dynamic("import java.util.Collection;\n", "Collection<Order> findByNumber(String number);", "findByNumber").unwrap();
        assert!(after.contains("    <T> Collection<T> findByNumber(String number, Class<T> type);"), "{after}");
    }

    #[test]
    fn dynamic_handles_no_parameters_and_taken_name() {
        let none = dynamic("import java.util.Collection;\n", "Collection<Order> findAllBy();", "findAllBy").unwrap();
        assert!(none.contains("<T> Collection<T> findAllBy(Class<T> type);"), "{none}");
        let taken = dynamic("import java.util.Collection;\n", "Collection<Order> findByType(String type);", "findByType").unwrap();
        assert!(taken.contains("findByType(String type, Class<T> projectionType);"), "{taken}");
    }

    #[test]
    fn dynamic_keeps_crlf_and_annotations() {
        let text = repo("import java.util.Collection;\n", "@Transactional(readOnly = true)\n    Collection<Order> findByNumber(String n);").replace('\n', "\r\n");
        let after = run(TITLE_DYNAMIC, PATH, "java", &text, "findByNumber").unwrap();
        assert!(after.contains("@Transactional(readOnly = true)\r\n    <T> Collection<T> findByNumber(String n, Class<T> type);"), "{after:?}");
    }

    #[test]
    fn dynamic_refusals() {
        let list = dynamic("import java.util.List;\n", "List<Order> findByNumber(String n);", "findByNumber");
        assert!(list.unwrap_err().contains("Collection<Order>"));
        let query = dynamic("import java.util.Collection;\n", "@Query(\"select o from Order o\")\n    Collection<Order> findAllOrders();", "findAllOrders");
        assert!(query.unwrap_err().contains("@Query"));
        let class = dynamic("import java.util.Collection;\n", "Collection<Order> findByNumber(String n, Class<?> k);", "findByNumber");
        assert!(class.unwrap_err().contains("Class parameter"));
        let paged = dynamic("import java.util.Collection;\nimport org.springframework.data.domain.Pageable;\n", "Collection<Order> findByNumber(String n, Pageable p);", "findByNumber");
        assert!(paged.unwrap_err().contains("Pageable"));
        let sorted = dynamic("import java.util.Collection;\nimport org.springframework.data.domain.Sort;\n", "Collection<Order> findByNumber(Sort s);", "findByNumber");
        assert!(sorted.unwrap_err().contains("Sort"));
        let dto = dynamic("import java.util.Collection;\n", "Collection<OrderView> findByNumber(String n);", "findByNumber");
        assert!(dto.unwrap_err().contains("Collection<Order>"));
        let count = dynamic("", "long countByNumber(String n);", "countByNumber");
        assert!(count.unwrap_err().contains("find"));
        let generic = dynamic("import java.util.Collection;\n", "<T> Collection<T> findByNumber(String n);", "findByNumber");
        assert!(generic.is_err());
    }

    #[test]
    #[ignore = "requires javac and SPRING_DATA_CLASSPATH; the async output also needs SPRING_CONTEXT_CLASSPATH (spring-context jar), else it is skipped"]
    fn generated_conversions_compile_against_spring_data() {
        let classpath = std::env::var("SPRING_DATA_CLASSPATH").expect("explicit classpath");
        let context = std::env::var("SPRING_CONTEXT_CLASSPATH").ok();
        let base = std::env::temp_dir().join(format!("jpa-convert-compile-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("Order.java"), "package shop;\npublic class Order {}\n").unwrap();
        let mut sources = vec![
            ("RDynamic", dynamic("import java.util.Collection;\n", "Collection<Order> findByNumber(String number);", "findByNumber").unwrap(), false),
            ("RDynamicEmpty", dynamic("import java.util.Collection;\n", "Collection<Order> findAllBy();", "findAllBy").unwrap(), false),
        ];
        sources.push(("RAsync", asynced("import java.util.List;\n", "List<Order> findByNumber(String number);", "findByNumber").unwrap(), true));
        sources.push(("RAsyncOne", asynced("", "Order findByNumber(String number);", "findByNumber").unwrap(), true));
        let mut files = vec![base.join("Order.java")];
        for (name, text, needs_context) in sources {
            if needs_context && context.is_none() {
                continue;
            }
            let file = base.join(format!("{name}.java"));
            std::fs::write(&file, text.replace("OrderRepository", name)).unwrap();
            files.push(file);
        }
        let joined = match &context {
            Some(extra) => format!("{classpath}:{extra}"),
            None => classpath,
        };
        let result = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &joined, "-d"]).arg(&base).args(&files).output().expect("javac installed");
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(base).unwrap();
    }
}
