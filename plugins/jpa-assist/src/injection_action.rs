//! Reviewed injection of a Spring Data repository into a Spring-managed class. Only the invoking
//! document is edited.
//!
//! Spring Framework reference facts this follows: a class with a single constructor is used by the
//! container even without `@Autowired`; with several constructors one must be marked, so those are
//! refused here; the Spring team advocates constructor injection (the default style); fields may carry
//! `@Autowired` (the opt-in `field-autowired` style); `@Component` is the generic stereotype and
//! `@Service`, `@Controller` and `@Configuration` are (meta-annotated) specializations, `@RestController`
//! is composed of `@Controller`. The Lombok style needs `@RequiredArgsConstructor`, which generates a
//! constructor from uninitialized final fields.
//!
//! Scope: exactly one top-level class, annotated with a stereotype whose import is verbatim from
//! `org.springframework.*`. Repositories are interfaces in files named `*Repository.java|kt` whose
//! generics name an entity; the entity file itself is not resolved.
use super::*;
use super::query_method_action::repository_imports;
use crate::model::{closing, decapitalize, has_word, import_edit, members, split_top_level, without_annotations, Edit};

pub(super) const TITLE: &str = "Inject repository…";
const MAX_REPOSITORIES: u32 = 65;
const STEREOTYPES: [(&str, &str); 5] = [
    ("Component", "org.springframework.stereotype"),
    ("Service", "org.springframework.stereotype"),
    ("Controller", "org.springframework.stereotype"),
    ("RestController", "org.springframework.web.bind.annotation"),
    ("Configuration", "org.springframework.context.annotation"),
];
const LOMBOK_GENERATORS: [&str; 7] =
    ["AllArgsConstructor", "NoArgsConstructor", "RequiredArgsConstructor", "Data", "Value", "Builder", "SuperBuilder"];
const AUTOWIRED: (&str, &str) = ("Autowired", "org.springframework.beans.factory.annotation.Autowired");
const REQUIRED_ARGS: (&str, &str) = ("RequiredArgsConstructor", "lombok.RequiredArgsConstructor");
const LAYOUT: &str = "The class layout is not supported (code on the same line as the insertion point).";

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Style {
    Constructor,
    Field,
    Lombok,
}

#[derive(Clone, PartialEq, Debug)]
pub(super) struct Repo {
    pub(super) name: String,
    pub(super) entity: String,
    pub(super) package: String,
}

impl Repo {
    fn qualified(&self) -> String {
        format!("{}.{}", self.package, self.name)
    }
}

pub(super) struct Source {
    kotlin: bool,
    name: String,
    name_end: usize,
    class_at: usize,
    open: Option<usize>,
    primary: Option<(usize, usize)>,
    annotations: Vec<String>,
    imports: Vec<String>,
    masked: String,
    members: Vec<(usize, usize)>,
}

struct Ctor {
    open: usize,
    close: usize,
    body: (usize, usize),
    annotated: bool,
}

fn idents(text: &str) -> Vec<&str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$')).filter(|word| !word.is_empty()).collect()
}

pub(super) fn analyze(doc: &Document) -> Result<Source, String> {
    let kotlin = doc.language == "kotlin";
    let masked = mask(&doc.text);
    let imports = repository_imports(doc);
    if imports.iter().any(|import| import.contains(" as ")) {
        return Err("Import aliases are not supported.".into());
    }
    let single = "Invoke this inside a file with exactly one top-level class.";
    let classes: Vec<_> = words(&masked, "class").collect();
    let [class_at] = classes.as_slice() else {
        return Err(single.into());
    };
    let class_at = *class_at;
    if ["interface", "enum", "record", "object", "typealias"].iter().any(|word| has_word(&masked, word)) {
        return Err(single.into());
    }
    let after = class_at + "class".len();
    let rest = &masked[after..];
    let lead = rest.len() - rest.trim_start().len();
    let name_len = rest.trim_start().find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$')).unwrap_or(rest.trim_start().len());
    let name = rest.trim_start()[..name_len].to_owned();
    let name_end = after + lead + name_len;
    if !identifier(&name) || masked[name_end..].trim_start().starts_with('<') {
        return Err("Generic or unnamed classes are not supported.".into());
    }
    let bytes = masked.as_bytes();
    let (open, close, primary) = if kotlin {
        let mut cursor = name_end;
        let next = masked[cursor..].trim_start();
        let mut primary = None;
        if next.starts_with("constructor") || next.starts_with('@') {
            return Err("An explicit primary constructor keyword or annotation is not supported.".into());
        }
        if next.starts_with('(') {
            let open = masked.len() - next.len();
            let close = closing(bytes, open).ok_or("The primary constructor is not closed.")?;
            primary = Some((open, close));
            cursor = close + 1;
        }
        let mut depth = 0;
        let mut body = None;
        let mut header_end = masked.len();
        for (offset, byte) in bytes.iter().enumerate().skip(cursor) {
            match byte {
                b'(' => depth += 1,
                b')' => depth -= 1,
                b'{' if depth == 0 => {
                    body = Some(offset);
                    break;
                }
                b'\n' if depth == 0 => {
                    header_end = offset;
                    if masked[offset..].trim_start().starts_with('{') {
                        body = Some(masked.len() - masked[offset..].trim_start().len());
                    }
                    break;
                }
                _ => {}
            }
        }
        match body {
            Some(open) => (Some(open), Some(closing(bytes, open).ok_or("The class body is not closed.")?), primary),
            None => {
                if !masked[header_end..].trim().is_empty() {
                    return Err(single.into());
                }
                (None, None, primary)
            }
        }
    } else {
        let open = name_end + masked[name_end..].find('{').ok_or("The class body was not found.")?;
        (Some(open), Some(closing(bytes, open).ok_or("The class body is not closed.")?), None)
    };
    if let Some(close) = close {
        if !masked[close + 1..].trim().is_empty() {
            return Err(single.into());
        }
    }
    let tops = super::index_query_action::top(&masked, class_at);
    let verified = tops.iter().any(|annotation| {
        STEREOTYPES.iter().any(|(simple, package)| {
            let qualified = format!("{package}.{simple}");
            let written = &masked[annotation.start + 1..annotation.name_end];
            annotation.name == *simple
                && (written == qualified
                    || (written == *simple
                        && imports.contains(&qualified)
                        && imports.iter().all(|import| *import == qualified || import.rsplit('.').next() != Some(simple))))
        })
    });
    if !verified {
        return Err("The class needs a Spring stereotype (@Component, @Service, @Controller, @RestController or @Configuration) imported from org.springframework.*.".into());
    }
    let members = match (open, close) {
        (Some(open), Some(close)) => members(&masked, open + 1, close, kotlin, false),
        _ => Vec::new(),
    };
    let annotations = tops.iter().map(|annotation| annotation.name.to_owned()).collect();
    Ok(Source { kotlin, name, name_end, class_at, open, primary, annotations, imports, masked, members })
}

pub(super) fn eligible(doc: &Document) -> bool {
    analyze(doc).is_ok()
}

fn field_head(src: &Source, member: (usize, usize)) -> Option<String> {
    let clean = without_annotations(&src.masked, member.0, member.1);
    let text = clean.trim();
    let head = text.split('=').next().unwrap_or(text);
    if src.kotlin {
        return ((has_word(text, "val") || has_word(text, "var")) && !has_word(text, "fun")).then(|| head.to_owned());
    }
    (text.ends_with(';') && !head.contains('(')).then(|| head.to_owned())
}

fn constructors(src: &Source) -> Vec<Ctor> {
    let bytes = src.masked.as_bytes();
    let mut found = Vec::new();
    for &(start, end) in &src.members {
        let clean = without_annotations(&src.masked, start, end);
        let Some(paren) = clean.find('(') else {
            continue;
        };
        if clean[..paren].contains('=') {
            continue;
        }
        let tokens: Vec<_> = clean[..paren].split_whitespace().collect();
        let Some((last, modifiers)) = tokens.split_last() else {
            continue;
        };
        if *last != src.name || !modifiers.iter().all(|word| ["public", "protected", "private"].contains(word)) {
            continue;
        }
        let open = start + paren;
        let Some(close) = closing(bytes, open) else {
            continue;
        };
        let Some(brace) = src.masked[close + 1..end].find('{') else {
            continue;
        };
        let Some(body_close) = closing(bytes, close + 1 + brace) else {
            continue;
        };
        let annotated = crate::model::annotations(&src.masked[start..end]).iter().any(|a| a.start < paren);
        found.push(Ctor { open, close, body: (close + 1 + brace, body_close), annotated });
    }
    found
}

fn indent_of(text: &str, at: usize) -> String {
    let line_start = text[..at].rfind('\n').map_or(0, |index| index + 1);
    let prefix = &text[line_start..at];
    if prefix.trim().is_empty() { prefix.to_owned() } else { "        ".to_owned() }
}

fn parameter_insert(doc: &Document, open: usize, close: usize, parameter: &str) -> Result<Edit, String> {
    let inner = &doc.text[open + 1..close];
    let trimmed = inner.trim_end();
    if inner.trim().is_empty() {
        return Err("The constructor has no parameters; a no-argument constructor may be relied upon, so nothing is changed.".into());
    }
    if inner.contains("...") || inner.contains("vararg") {
        return Err("A varargs constructor parameter is not supported.".into());
    }
    let at = open + 1 + trimmed.len();
    if !inner.contains('\n') {
        return Ok(Edit::insert(at, &format!(", {parameter}")));
    }
    let masked = mask(&doc.text);
    let pieces = split_top_level(&masked[open + 1..close]);
    let (offset, piece) = pieces.iter().rev().find(|(_, piece)| !piece.trim().is_empty()).ok_or("The constructor has no parameters.")?;
    let first = open + 1 + offset + (piece.len() - piece.trim_start().len());
    let indent = indent_of(&doc.text, first);
    let comma = if trimmed.ends_with(',') { "" } else { "," };
    let tail = if trimmed.ends_with(',') { "," } else { "" };
    Ok(Edit::insert(at, &format!("{comma}\n{indent}{parameter}{tail}")))
}

fn insertion_line(src: &Source, fields: &[(usize, usize)]) -> Result<(usize, bool), String> {
    let open = src.open.ok_or("The class has no body to add a field to.")?;
    let from = fields.last().map_or(open + 1, |member| member.1);
    let newline = src.masked[from..].find('\n').map(|index| from + index).ok_or(LAYOUT)?;
    if !src.masked[from..newline].trim().is_empty() {
        return Err(LAYOUT.into());
    }
    Ok((newline + 1, !fields.is_empty()))
}

fn needs_blank_after(masked: &str, at: usize) -> bool {
    masked[at..].lines().next().is_some_and(|line| {
        let line = line.trim();
        !line.is_empty() && !line.starts_with('}')
    })
}

fn import_clash(src: &Source, (simple, qualified): (&str, &str)) -> Result<(), String> {
    if src.imports.iter().any(|import| import.rsplit('.').next() == Some(simple) && import != qualified) {
        return Err(format!("`{simple}` already names a different type in this file; no import is added."));
    }
    Ok(())
}

pub(super) fn plan(doc: &Document, src: &Source, repo: &Repo, style: Style, lombok: bool) -> Result<Vec<TextEdit>, String> {
    let name = decapitalize(&repo.name);
    let kotlin = src.kotlin;
    if !identifier(&name) || repo.name == src.name {
        return Err("The repository name cannot be used as a field name or clashes with this class.".into());
    }
    let own_package = package(doc).ok_or("The package of this source is not unambiguous.")?;
    let repo_qualified = repo.qualified();
    import_clash(src, (&repo.name, &repo_qualified))?;
    let mut wanted = Vec::new();
    if repo.package != own_package {
        wanted.push(repo_qualified.clone());
    }

    let fields: Vec<_> = src.members.iter().copied().filter(|member| field_head(src, *member).is_some()).collect();
    let ctors = if kotlin { Vec::new() } else { constructors(src) };
    let mut heads: Vec<String> = fields.iter().filter_map(|member| field_head(src, *member)).collect();
    heads.extend(ctors.iter().map(|ctor| src.masked[ctor.open + 1..ctor.close].to_owned()));
    if let Some((open, close)) = src.primary {
        heads.push(src.masked[open + 1..close].to_owned());
    }
    for head in &heads {
        let words = idents(head);
        if words.contains(&repo.name.as_str()) {
            return Err(format!("`{}` is already injected or declared in this class; nothing is changed.", repo.name));
        }
        if words.contains(&name.as_str()) {
            return Err(format!("The field name `{name}` is already used in this class; nothing is changed."));
        }
    }

    let generators: Vec<_> = src.annotations.iter().filter(|a| LOMBOK_GENERATORS.contains(&a.as_str())).collect();
    let mut edits = Vec::new();
    let ty = &repo.name;
    match style {
        Style::Constructor => {
            if !generators.is_empty() {
                return Err("Lombok constructor annotations are present; the generated constructor cannot be verified.".into());
            }
            if kotlin {
                if has_word(&src.masked, "constructor") {
                    return Err("The class declares a constructor keyword (secondary constructor); only a primary constructor is extended.".into());
                }
                let parameter = format!("private val {name}: {ty}");
                match src.primary {
                    Some((open, close)) => edits.push(parameter_insert(doc, open, close, &parameter)?),
                    None => edits.push(Edit::insert(src.name_end, &format!("({parameter})"))),
                }
            } else {
                match ctors.as_slice() {
                    [] => {
                        let (at, _) = insertion_line(src, &fields)?;
                        let mut text = format!(
                            "    private final {ty} {name};\n\n    public {class}({ty} {name}) {{\n        this.{name} = {name};\n    }}\n",
                            class = src.name
                        );
                        if needs_blank_after(&src.masked, at) {
                            text.push('\n');
                        }
                        edits.push(Edit::insert(at, &text));
                    }
                    [ctor] => {
                        if ctor.annotated {
                            return Err("The constructor carries annotations (for example @Autowired) that cannot be verified.".into());
                        }
                        let body = &src.masked[ctor.body.0 + 1..ctor.body.1];
                        if body.trim_start().strip_prefix("this").is_some_and(|rest| rest.trim_start().starts_with('(')) {
                            return Err("The constructor delegates to another constructor with this(...).".into());
                        }
                        edits.push(parameter_insert(doc, ctor.open, ctor.close, &format!("{ty} {name}"))?);
                        let line_start = doc.text[..ctor.body.1].rfind('\n').map_or(0, |index| index + 1);
                        let indent = &doc.text[line_start..ctor.body.1];
                        if !indent.trim().is_empty() {
                            return Err("The constructor body closes on a line with code; this layout is not supported.".into());
                        }
                        edits.push(Edit::insert(line_start, &format!("{indent}    this.{name} = {name};\n")));
                        let (at, after_field) = insertion_line(src, &fields)?;
                        let blank = !after_field && needs_blank_after(&src.masked, at);
                        edits.push(Edit::insert(at, &format!("    private final {ty} {name};\n{}", if blank { "\n" } else { "" })));
                    }
                    _ => return Err("The class has several constructors; which one Spring uses cannot be verified, so nothing is changed.".into()),
                }
            }
        }
        Style::Field => {
            import_clash(src, AUTOWIRED)?;
            wanted.push(AUTOWIRED.1.to_owned());
            let (at, after_field) = insertion_line(src, &fields)?;
            let declaration = if kotlin {
                format!("    @Autowired\n    private lateinit var {name}: {ty}\n")
            } else {
                format!("    @Autowired\n    private {ty} {name};\n")
            };
            let blank = !after_field && needs_blank_after(&src.masked, at);
            edits.push(Edit::insert(at, &format!("{declaration}{}", if blank { "\n" } else { "" })));
        }
        Style::Lombok => {
            if kotlin {
                return Err("The Lombok style is for Java sources; Kotlin classes use a primary constructor.".into());
            }
            if !lombok {
                return Err("Lombok is not provable from this file's imports or the build files; the Lombok style is refused.".into());
            }
            if !ctors.is_empty() {
                return Err("The class declares explicit constructors; @RequiredArgsConstructor would not be used consistently.".into());
            }
            if generators.iter().any(|a| a.as_str() != "RequiredArgsConstructor") {
                return Err("Another Lombok constructor generator (@Data, @Value, @AllArgsConstructor, ...) is present.".into());
            }
            let present = generators.iter().any(|a| a.as_str() == "RequiredArgsConstructor");
            if present {
                if !src.imports.iter().any(|import| import == REQUIRED_ARGS.1 || import == "lombok.*") {
                    return Err("@RequiredArgsConstructor is present but not provably from Lombok.".into());
                }
            } else {
                import_clash(src, REQUIRED_ARGS)?;
                wanted.push(REQUIRED_ARGS.1.to_owned());
                let line_start = src.masked[..src.class_at].rfind('\n').map_or(0, |index| index + 1);
                if src.masked[line_start..src.class_at].contains('@') {
                    return Err(LAYOUT.into());
                }
                edits.push(Edit::insert(line_start, "@RequiredArgsConstructor\n"));
            }
            let (at, after_field) = insertion_line(src, &fields)?;
            let blank = !after_field && needs_blank_after(&src.masked, at);
            edits.push(Edit::insert(at, &format!("    private final {ty} {name};\n{}", if blank { "\n" } else { "" })));
        }
    }

    let refs: Vec<&str> = wanted.iter().map(String::as_str).collect();
    if let Some(edit) = import_edit(&doc.text, &refs, kotlin) {
        edits.insert(0, edit);
    }
    edits.sort_by_key(|edit| edit.start);
    let mut merged: Vec<Edit> = Vec::new();
    for edit in edits {
        match merged.last_mut() {
            Some(previous) if previous.end == edit.start && previous.start == previous.end && edit.start == edit.end => {
                previous.text.push_str(&edit.text);
            }
            Some(previous) if previous.end > edit.start => return Err("Edits overlap; nothing is changed.".into()),
            _ => merged.push(edit),
        }
    }
    let crlf = doc.text.contains("\r\n");
    Ok(merged
        .into_iter()
        .map(|mut edit| {
            if crlf {
                edit.text = edit.text.replace('\n', "\r\n");
            }
            edit.to_text_edit(&doc.text)
        })
        .collect())
}

pub(super) fn style() -> Result<Style, Error> {
    match host::setting_string("injectionStyle").as_deref() {
        None | Some("constructor") => Ok(Style::Constructor),
        Some("field-autowired") => Ok(Style::Field),
        Some("lombok-required-args") => Ok(Style::Lombok),
        Some(other) => Err(format!("Unknown injectionStyle `{other}`; use constructor, field-autowired or lombok-required-args.").into()),
    }
}

fn discover(project: &Project, root: &str, own: &str) -> Result<Vec<Repo>, Error> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut found: Vec<Repo> = Vec::new();
    for extension in ["java", "kt"] {
        let files = project.find_files(&format!("**/*Repository.{extension}"), MAX_REPOSITORIES)?;
        if files.len() >= MAX_REPOSITORIES as usize {
            return Err("Repository discovery is incomplete: too many *Repository files in this project.".into());
        }
        for file in files.into_iter().filter(|file| file != own) {
            if Instant::now() >= deadline {
                return Err("Repository discovery timed out.".into());
            }
            if path(&file).as_deref() != Some(file.as_str()) || !inside(root, &file) {
                return Err(Error::Stale);
            }
            let text = project.read_file(&file)?;
            if text.len() > MAX_TEXT {
                continue;
            }
            let masked = mask(&text);
            let kotlin = extension == "kt";
            let single = words(&masked, "interface").count() == 1
                && ["class", "object", "enum", "typealias"].iter().all(|word| !has_word(&masked, word));
            let Some(parsed) = crate::queries::parse_repository(&text, kotlin).filter(|_| single) else {
                continue;
            };
            let document = Document { path: file.clone(), language: if kotlin { "kotlin" } else { "java" }.into(), text, version: 0 };
            let Some(package) = package(&document) else {
                continue;
            };
            let name = file_stem(&file);
            let repo = Repo { name: name.to_owned(), entity: parsed.entity, package };
            if !found.contains(&repo) && identifier(&repo.entity) {
                found.push(repo);
            }
        }
    }
    found.sort_by(|a, b| a.qualified().cmp(&b.qualified()));
    Ok(found)
}

pub(super) fn lombok_provable(doc: &Document, project: &Project, root: &str) -> Result<bool, Error> {
    if super::mapper_action::uses_lombok(&doc.text) {
        return Ok(true);
    }
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let build = super::mapper_action::build_text(project, root, directory)?;
    Ok(build.contains("org.projectlombok"))
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let src = analyze(doc).map_err(Error::from)?;
    let style = style()?;
    let repositories = discover(project, &pending.root, &doc.path)?;
    if repositories.is_empty() {
        return Err("No Spring Data repository interface (a *Repository.java or *Repository.kt file naming an entity) was found in this project.".into());
    }
    let items: Vec<_> = repositories
        .iter()
        .enumerate()
        .map(|(index, repo)| PickItem::new(&index.to_string(), &repo.name).description(&format!("entity {} · {}", repo.entity, repo.package)))
        .collect();
    let picked = host::pick("Repository to inject", &items)?.ok_or(Error::Cancelled)?;
    let repo = repositories.get(picked.parse::<usize>().map_err(|_| Error::Stale)?).ok_or(Error::Stale)?.clone();
    let lombok = style == Style::Lombok && lombok_provable(doc, project, &pending.root)?;
    let project = current(pending, doc)?;
    if !discover(&project, &pending.root, &doc.path)?.contains(&repo) {
        return Err(Error::Stale);
    }
    let edits = plan(doc, &src, &repo, style, lombok).map_err(Error::from)?;
    let after = patched(&doc.text, &edits)?;
    if after.len() > MAX_TEXT {
        return Err("Generated source exceeds the supported bound.".into());
    }
    let label = match style {
        Style::Constructor => "constructor",
        Style::Field => "field-autowired",
        Style::Lombok => "lombok-required-args",
    };
    if !ui_kit::prompts::confirm_exact(
        "Inject repository?",
        &format!(
            "Target: {}\nSource version: {}\nStyle: {label} (setting injectionStyle)\nRepository: {} (entity {})\n\n{after}\nOnly this source is edited. Inherited members, other beans and the build are not read; the entity file is not resolved.",
            doc.path, doc.version, repo.qualified(), repo.entity
        ),
    )? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    Ok(FileAction {
        title: format!("Inject {}", repo.name),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), edits)],
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const PATH: &str = "/project/src/main/java/shop/OrderService.java";
    const KT_PATH: &str = "/project/src/main/kotlin/shop/OrderService.kt";
    const REPO: &str = "package shop;\n\nimport org.springframework.data.jpa.repository.JpaRepository;\n\npublic interface OrderRepository extends JpaRepository<Order, Long> {\n}\n";
    const SERVICE: &str = "package shop;\n\nimport org.springframework.stereotype.Service;\n\n@Service\npublic class OrderService {\n}\n";
    const KT_SERVICE: &str = "package shop\n\nimport org.springframework.stereotype.Service\n\n@Service\nclass OrderService\n";

    fn repo(package: &str) -> Repo {
        Repo { name: "OrderRepository".into(), entity: "Order".into(), package: package.into() }
    }

    fn render(path: &str, text: &str, style: Style, package: &str, lombok: bool) -> Result<String, String> {
        let language = if path.ends_with(".kt") { "kotlin" } else { "java" };
        let doc = Document { path: path.into(), language: language.into(), text: text.into(), version: 1 };
        let src = analyze(&doc)?;
        let edits = plan(&doc, &src, &repo(package), style, lombok)?;
        patched(&doc.text, &edits).map_err(|_| "patch".to_owned())
    }

    fn java(style: Style, text: &str) -> Result<String, String> {
        render(PATH, text, style, "shop", false)
    }

    fn refused(result: Result<String, String>, needle: &str) {
        let error = result.expect_err("refusal");
        assert!(error.contains(needle), "{error}");
    }

    #[test]
    fn java_constructor_is_created_in_an_empty_class() {
        assert_eq!(
            java(Style::Constructor, SERVICE).unwrap(),
            "package shop;\n\nimport org.springframework.stereotype.Service;\n\n@Service\npublic class OrderService {\n    private final OrderRepository orderRepository;\n\n    public OrderService(OrderRepository orderRepository) {\n        this.orderRepository = orderRepository;\n    }\n}\n"
        );
    }

    #[test]
    fn java_constructor_is_created_after_existing_fields_and_before_methods() {
        let text = SERVICE.replace("{\n}", "{\n    private int count;\n\n    void run() {\n    }\n}");
        assert_eq!(
            java(Style::Constructor, &text).unwrap(),
            "package shop;\n\nimport org.springframework.stereotype.Service;\n\n@Service\npublic class OrderService {\n    private int count;\n    private final OrderRepository orderRepository;\n\n    public OrderService(OrderRepository orderRepository) {\n        this.orderRepository = orderRepository;\n    }\n\n    void run() {\n    }\n}\n"
        );
    }

    #[test]
    fn java_single_explicit_constructor_gets_parameter_assignment_and_field() {
        let text = SERVICE.replace(
            "{\n}",
            "{\n    private final Clock clock;\n\n    public OrderService(Clock clock) {\n        this.clock = clock;\n    }\n}",
        );
        assert_eq!(
            java(Style::Constructor, &text).unwrap(),
            "package shop;\n\nimport org.springframework.stereotype.Service;\n\n@Service\npublic class OrderService {\n    private final Clock clock;\n    private final OrderRepository orderRepository;\n\n    public OrderService(Clock clock, OrderRepository orderRepository) {\n        this.clock = clock;\n        this.orderRepository = orderRepository;\n    }\n}\n"
        );
        let multiline = SERVICE.replace(
            "{\n}",
            "{\n    private final Clock clock;\n\n    public OrderService(\n            Clock clock\n    ) {\n        this.clock = clock;\n    }\n}",
        );
        let out = java(Style::Constructor, &multiline).unwrap();
        assert!(out.contains("            Clock clock,\n            OrderRepository orderRepository\n    ) {"), "{out}");
    }

    #[test]
    fn java_constructor_refusals() {
        let with = |body: &str| SERVICE.replace("{\n}", &format!("{{\n{body}\n}}"));
        refused(java(Style::Constructor, &with("    A(int a) {}\n    public OrderService(Clock c) {}").replace("A(int a) {}", "public OrderService(int a) {}")), "several constructors");
        refused(java(Style::Constructor, &with("    public OrderService() {\n    }")), "no-argument");
        refused(java(Style::Constructor, &with("    @Autowired\n    public OrderService(Clock c) {\n    }")), "annotations");
        refused(java(Style::Constructor, &with("    public OrderService(Clock c) {\n        this();\n    }\n    ")), "delegates");
        refused(java(Style::Constructor, &with("    public OrderService(Clock c) { }")), "closes on a line");
        refused(java(Style::Constructor, &with("    public OrderService(Clock... c) {\n    }")), "varargs");
        let lombok = SERVICE.replace("@Service\n", "@Service\n@AllArgsConstructor\n");
        refused(java(Style::Constructor, &lombok), "Lombok");
    }

    #[test]
    fn collisions_by_name_or_type_are_refused() {
        let named = SERVICE.replace("{\n}", "{\n    private String orderRepository;\n}");
        refused(java(Style::Constructor, &named), "field name `orderRepository`");
        let typed = SERVICE.replace("{\n}", "{\n    private final OrderRepository repo;\n}");
        refused(java(Style::Field, &typed), "already injected");
        let param = SERVICE.replace("{\n}", "{\n    public OrderService(OrderRepository other) {\n    }\n}");
        refused(java(Style::Constructor, &param), "already injected");
        let kotlin = KT_SERVICE.replace("class OrderService", "class OrderService(private val orderRepository: String)");
        refused(render(KT_PATH, &kotlin, Style::Constructor, "shop", false), "field name");
    }

    #[test]
    fn java_field_style_adds_autowired_and_import_without_duplicates() {
        assert_eq!(
            java(Style::Field, SERVICE).unwrap(),
            "package shop;\n\nimport org.springframework.stereotype.Service;\nimport org.springframework.beans.factory.annotation.Autowired;\n\n@Service\npublic class OrderService {\n    @Autowired\n    private OrderRepository orderRepository;\n}\n"
        );
        let imported = SERVICE.replace("import org.springframework.stereotype.Service;", "import org.springframework.beans.factory.annotation.*;\nimport org.springframework.stereotype.Service;");
        let out = java(Style::Field, &imported).unwrap();
        assert_eq!(out.matches("import ").count(), 2, "{out}");
        let after_field = SERVICE.replace("{\n}", "{\n    private int count;\n\n    void run() {\n    }\n}");
        let out = java(Style::Field, &after_field).unwrap();
        assert!(out.contains("    private int count;\n    @Autowired\n    private OrderRepository orderRepository;\n\n    void run()"), "{out}");
    }

    #[test]
    fn java_lombok_style_requires_proof_and_adds_annotation_and_import() {
        refused(render(PATH, SERVICE, Style::Lombok, "shop", false), "Lombok is not provable");
        let out = render(PATH, SERVICE, Style::Lombok, "shop", true).unwrap();
        assert_eq!(
            out,
            "package shop;\n\nimport org.springframework.stereotype.Service;\nimport lombok.RequiredArgsConstructor;\n\n@Service\n@RequiredArgsConstructor\npublic class OrderService {\n    private final OrderRepository orderRepository;\n}\n"
        );
        let existing = "package shop;\n\nimport lombok.RequiredArgsConstructor;\nimport org.springframework.stereotype.Service;\n\n@Service\n@RequiredArgsConstructor\npublic class OrderService {\n    private final Clock clock;\n}\n";
        let out = render(PATH, existing, Style::Lombok, "shop", true).unwrap();
        assert!(out.contains("    private final Clock clock;\n    private final OrderRepository orderRepository;\n}"), "{out}");
        assert_eq!(out.matches("RequiredArgsConstructor").count(), 2);
        let explicit = SERVICE.replace("{\n}", "{\n    public OrderService(Clock c) {\n    }\n}");
        refused(render(PATH, &explicit, Style::Lombok, "shop", true), "explicit constructors");
        let data = SERVICE.replace("@Service\n", "@Service\n@Data\n");
        refused(render(PATH, &data, Style::Lombok, "shop", true), "Another Lombok");
        let inline = SERVICE.replace("@Service\npublic class", "@Service public class");
        refused(render(PATH, &inline, Style::Lombok, "shop", true), "layout");
        refused(render(KT_PATH, KT_SERVICE, Style::Lombok, "shop", true), "Java sources");
    }

    #[test]
    fn repository_import_is_added_for_other_packages_and_clashes_are_refused() {
        let out = render(PATH, SERVICE, Style::Constructor, "shop.data", false).unwrap();
        assert!(out.contains("import org.springframework.stereotype.Service;\nimport shop.data.OrderRepository;"), "{out}");
        let known = SERVICE.replace("import org.springframework", "import shop.data.OrderRepository;\nimport org.springframework");
        let out = render(PATH, &known, Style::Constructor, "shop.data", false).unwrap();
        assert_eq!(out.matches("import shop.data.OrderRepository;").count(), 1);
        let clash = SERVICE.replace("import org.springframework", "import other.OrderRepository;\nimport org.springframework");
        refused(render(PATH, &clash, Style::Constructor, "shop.data", false), "different type");
        refused(render(PATH, &clash, Style::Constructor, "shop", false), "different type");
        let same_name = SERVICE.replace("OrderService", "OrderRepository");
        refused(render("/project/src/main/java/shop/OrderRepository.java", &same_name, Style::Constructor, "shop", false), "clashes");
    }

    #[test]
    fn stereotypes_need_exact_imports_and_one_top_level_class() {
        let doc = |text: &str| Document { path: PATH.into(), language: "java".into(), text: text.into(), version: 1 };
        assert!(eligible(&doc(SERVICE)));
        for (annotation, import) in [
            ("Component", "org.springframework.stereotype.Component"),
            ("Controller", "org.springframework.stereotype.Controller"),
            ("RestController", "org.springframework.web.bind.annotation.RestController"),
            ("Configuration", "org.springframework.context.annotation.Configuration"),
        ] {
            let text = format!("package shop;\n\nimport {import};\n\n@{annotation}\npublic class OrderService {{\n}}\n");
            assert!(eligible(&doc(&text)), "{annotation}");
        }
        let qualified = "package shop;\n\n@org.springframework.stereotype.Service\npublic class OrderService {\n}\n";
        assert!(eligible(&doc(qualified)));
        assert!(!eligible(&doc("package shop;\n\n@Service\npublic class OrderService {\n}\n")));
        assert!(!eligible(&doc(&SERVICE.replace("org.springframework.stereotype.Service", "com.acme.Service"))));
        assert!(!eligible(&doc(&SERVICE.replace("org.springframework.stereotype.Service", "org.springframework.stereotype.*"))));
        assert!(!eligible(&doc(&SERVICE.replace("@Service\n", ""))));
        assert!(!eligible(&doc(&SERVICE.replace("{\n}", "{\n    static class Inner {\n    }\n}"))));
        assert!(!eligible(&doc(&format!("{SERVICE}class Other {{\n}}\n"))));
        assert!(!eligible(&doc(&SERVICE.replace("public class OrderService", "public interface OrderService"))));
        assert!(!eligible(&doc(&SERVICE.replace("OrderService {", "OrderService<T> {"))));
        assert!(!eligible(&doc(&SERVICE.replace("import org.springframework.stereotype.Service;", "import org.springframework.stereotype.Service;\nimport x.Service;"))));
    }

    #[test]
    fn kotlin_primary_constructor_is_created_or_extended() {
        assert_eq!(
            render(KT_PATH, KT_SERVICE, Style::Constructor, "shop", false).unwrap(),
            "package shop\n\nimport org.springframework.stereotype.Service\n\n@Service\nclass OrderService(private val orderRepository: OrderRepository)\n"
        );
        let with_super = KT_SERVICE.replace("class OrderService", "class OrderService : Base()");
        let out = render(KT_PATH, &with_super, Style::Constructor, "shop", false).unwrap();
        assert!(out.contains("class OrderService(private val orderRepository: OrderRepository) : Base()"), "{out}");
        let extended = KT_SERVICE.replace("class OrderService", "class OrderService(private val clock: Clock) {\n    fun run() {}\n}");
        let out = render(KT_PATH, &extended, Style::Constructor, "shop.data", false).unwrap();
        assert!(out.contains("class OrderService(private val clock: Clock, private val orderRepository: OrderRepository) {"), "{out}");
        assert!(out.contains("import org.springframework.stereotype.Service\nimport shop.data.OrderRepository"), "{out}");
        let multiline = KT_SERVICE.replace("class OrderService", "class OrderService(\n    private val clock: Clock,\n)");
        let out = render(KT_PATH, &multiline, Style::Constructor, "shop", false).unwrap();
        assert!(out.contains("(\n    private val clock: Clock,\n    private val orderRepository: OrderRepository,\n)"), "{out}");
    }

    #[test]
    fn kotlin_refusals_and_field_style() {
        refused(render(KT_PATH, &KT_SERVICE.replace("class OrderService", "class OrderService {\n    constructor(a: Int)\n}"), Style::Constructor, "shop", false), "secondary");
        refused(render(KT_PATH, &KT_SERVICE.replace("class OrderService", "class OrderService(val a: Int) {\n    constructor() : this(1)\n}"), Style::Constructor, "shop", false), "secondary");
        refused(render(KT_PATH, &KT_SERVICE.replace("class OrderService", "class OrderService()"), Style::Constructor, "shop", false), "no-argument");
        refused(render(KT_PATH, &KT_SERVICE.replace("class OrderService", "class OrderService @Autowired constructor(a: Int)"), Style::Constructor, "shop", false), "explicit primary");
        refused(render(KT_PATH, KT_SERVICE, Style::Field, "shop", false), "no body");
        let body = KT_SERVICE.replace("class OrderService", "class OrderService {\n    val count = 1\n\n    fun run() {}\n}");
        let out = render(KT_PATH, &body, Style::Field, "shop", false).unwrap();
        assert_eq!(
            out,
            "package shop\n\nimport org.springframework.stereotype.Service\nimport org.springframework.beans.factory.annotation.Autowired\n\n@Service\nclass OrderService {\n    val count = 1\n    @Autowired\n    private lateinit var orderRepository: OrderRepository\n\n    fun run() {}\n}\n"
        );
    }

    #[test]
    fn crlf_sources_get_crlf_only_edits() {
        for (path, text, style) in [
            (PATH, SERVICE, Style::Constructor),
            (PATH, SERVICE, Style::Field),
            (KT_PATH, KT_SERVICE, Style::Constructor),
        ] {
            let out = render(path, &text.replace('\n', "\r\n"), style, "shop.data", false).unwrap();
            assert!(!out.replace("\r\n", "").contains('\n'), "{out:?}");
        }
        let out = render(PATH, &SERVICE.replace('\n', "\r\n"), Style::Lombok, "shop", true).unwrap();
        assert!(!out.replace("\r\n", "").contains('\n'), "{out:?}");
    }

    fn setup(path: &str, text: &str, language: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(path, language, text, &[]);
        let doc = Document { path: path.into(), language: language.into(), text: text.into(), version: 1 };
        (host, JpaAssist::default(), doc)
    }

    fn entry(plugin: &mut JpaAssist, doc: &Document) -> FileAction {
        plugin.file_actions(doc, Range::default()).into_iter().find(|a| a.title == TITLE).expect("menu entry")
    }

    fn edited(doc: &Document, result: Result<FileAction, Error>) -> String {
        let action = result.expect("action");
        assert!(action.operations.is_empty());
        let [(path, edits)] = action.edits.as_slice() else { panic!("one document") };
        assert_eq!(path, &doc.path);
        patched(&doc.text, edits).unwrap()
    }

    #[test]
    fn menu_entry_is_only_offered_for_spring_managed_classes() {
        let (_host, mut plugin, doc) = setup(PATH, SERVICE, "java");
        assert!(plugin.file_actions(&doc, Range::default()).iter().any(|a| a.title == TITLE));
        let (_host, mut plugin, doc) = setup(PATH, &SERVICE.replace("@Service\n", ""), "java");
        assert!(!plugin.file_actions(&doc, Range::default()).iter().any(|a| a.title == TITLE));
    }

    #[test]
    fn flow_lists_repositories_and_applies_the_default_constructor_style() {
        let (host, mut plugin, doc) = setup(PATH, SERVICE, "java");
        host.add_file("src/main/java/shop/OrderRepository.java", REPO);
        host.add_file("src/main/java/shop/data/CustomerRepository.java", &REPO.replace("package shop;", "package shop.data;").replace("OrderRepository", "CustomerRepository").replace("Order,", "Customer,"));
        host.add_file("src/main/java/shop/NotRepository.java", "package shop;\npublic class NotRepository {}\n");
        let pending = entry(&mut plugin, &doc);
        host.answer_next_prompt(Some(&["1"]));
        host.answer_next_prompt(Some(&[]));
        let out = edited(&doc, plugin.resolve_file_action(&doc, pending));
        assert!(out.contains("import shop.data.CustomerRepository;"), "{out}");
        assert!(out.contains("public OrderService(CustomerRepository customerRepository) {"), "{out}");
    }

    #[test]
    fn flow_uses_the_injection_style_setting_and_lombok_build_proof() {
        let (host, mut plugin, doc) = setup(PATH, SERVICE, "java");
        host.add_file("src/main/java/shop/OrderRepository.java", REPO);
        host.set_setting("injectionStyle", "\"field-autowired\"");
        let pending = entry(&mut plugin, &doc);
        host.answer_next_prompt(Some(&["0"]));
        host.answer_next_prompt(Some(&[]));
        let out = edited(&doc, plugin.resolve_file_action(&doc, pending));
        assert!(out.contains("    @Autowired\n    private OrderRepository orderRepository;"), "{out}");

        let (host, mut plugin, doc) = setup(PATH, SERVICE, "java");
        host.add_file("src/main/java/shop/OrderRepository.java", REPO);
        host.set_setting("injectionStyle", "\"lombok-required-args\"");
        let pending = entry(&mut plugin, &doc);
        host.answer_next_prompt(Some(&["0"]));
        assert!(plugin.resolve_file_action(&doc, pending.clone()).is_err());
        host.add_file("pom.xml", "<project><dependency><groupId>org.projectlombok</groupId><artifactId>lombok</artifactId></dependency></project>");
        host.answer_next_prompt(Some(&["0"]));
        host.answer_next_prompt(Some(&[]));
        let out = edited(&doc, plugin.resolve_file_action(&doc, pending));
        assert!(out.contains("@RequiredArgsConstructor\npublic class"), "{out}");

        let (host, mut plugin, doc) = setup(PATH, SERVICE, "java");
        host.add_file("src/main/java/shop/OrderRepository.java", REPO);
        host.set_setting("injectionStyle", "\"bogus\"");
        let pending = entry(&mut plugin, &doc);
        host.answer_next_prompt(Some(&["0"]));
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
    }

    #[test]
    fn flow_refuses_without_repositories_and_when_cancelled_or_stale() {
        let (host, mut plugin, doc) = setup(PATH, SERVICE, "java");
        let pending = entry(&mut plugin, &doc);
        let error = plugin.resolve_file_action(&doc, pending).unwrap_err();
        assert!(format!("{error:?}").contains("No Spring Data repository"), "{error:?}");

        host.add_file("src/main/java/shop/OrderRepository.java", REPO);
        let pending = entry(&mut plugin, &doc);
        host.answer_next_prompt(Some(&["0"]));
        host.answer_next_prompt(Some(&[]));
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&doc, pending.clone());
        assert!(result.is_ok() || matches!(result, Err(Error::Cancelled)));
        let mut changed = doc.clone();
        changed.version = 2;
        host.answer_next_prompt(Some(&["0"]));
        assert!(matches!(plugin.resolve_file_action(&changed, pending), Err(Error::Stale)));
    }

    fn workspace(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("jpa-injection-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    #[ignore = "requires javac, SPRING_DATA_CLASSPATH and SPRING_CONTEXT_CLASSPATH (scripts/jpa-compile-env.sh); Lombok only when LOMBOK_JAR or ~/.m2 has a jar"]
    fn generated_java_injection_compiles_against_spring() {
        let data = std::env::var("SPRING_DATA_CLASSPATH").expect("spring data classpath");
        let context = std::env::var("SPRING_CONTEXT_CLASSPATH").expect("spring context classpath");
        let lombok = std::env::var("LOMBOK_JAR").ok().or_else(|| {
            let root = std::path::PathBuf::from(std::env::var("HOME").ok()?).join(".m2/repository/org/projectlombok/lombok");
            std::fs::read_dir(root).ok()?.flatten().find_map(|version| {
                std::fs::read_dir(version.path()).ok()?.flatten().map(|file| file.path()).find(|file| {
                    file.extension().is_some_and(|ext| ext == "jar") && !file.to_string_lossy().contains("sources")
                })
            }).map(|jar| jar.to_string_lossy().into_owned())
        });
        let entity = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Order {\n    @Id Long id;\n}\n";
        let existing = SERVICE.replace("{\n}", "{\n    private final java.time.Clock clock;\n\n    public OrderService(java.time.Clock clock) {\n        this.clock = clock;\n    }\n}");
        let mut cases = vec![
            ("constructor", Style::Constructor, SERVICE.to_owned(), "shop"),
            ("existing", Style::Constructor, existing, "shop.data"),
            ("field", Style::Field, SERVICE.to_owned(), "shop.data"),
        ];
        if lombok.is_some() {
            cases.push(("lombok", Style::Lombok, SERVICE.to_owned(), "shop"));
        } else {
            eprintln!("lombok case skipped: no jar");
        }
        for (label, style, source, package) in cases {
            let dir = workspace(label);
            let rendered = render(PATH, &source, style, package, true).unwrap();
            let repo_source = format!("package {package};\npublic interface OrderRepository extends org.springframework.data.jpa.repository.JpaRepository<shop.Order, Long> {{\n}}\n");
            let files = [
                write(&dir, "Order.java", entity),
                write(&dir, "OrderService.java", &rendered),
                write(&dir, "OrderRepository.java", &repo_source),
            ];
            let mut classpath = format!("{data}:{context}");
            if style == Style::Lombok {
                classpath = format!("{classpath}:{}", lombok.as_deref().unwrap());
            }
            let mut command = std::process::Command::new("javac");
            command.args(["--release", "17", "-proc:full", "-classpath", &classpath, "-d"]).arg(&dir).args(&files);
            if style == Style::Lombok {
                command.args(["-processorpath", lombok.as_deref().unwrap()]);
            }
            let result = command.output().expect("javac installed");
            assert!(result.status.success(), "{label}: {}\n{rendered}", String::from_utf8_lossy(&result.stderr));
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    #[ignore = "requires Kotlin compiler (KOTLIN_COMPILER_CLASSPATH, KOTLIN_STDLIB), SPRING_DATA_CLASSPATH and SPRING_CONTEXT_CLASSPATH"]
    fn generated_kotlin_injection_compiles_against_spring() {
        let data = std::env::var("SPRING_DATA_CLASSPATH").expect("spring data classpath");
        let context = std::env::var("SPRING_CONTEXT_CLASSPATH").expect("spring context classpath");
        let compiler = std::env::var("KOTLIN_COMPILER_CLASSPATH").expect("explicit Kotlin compiler classpath");
        let stdlib = std::env::var("KOTLIN_STDLIB").expect("explicit Kotlin standard library jar");
        let entity = "package shop\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\n@Entity\nopen class Order {\n    @Id var id: Long? = null\n}\n";
        let repo = "package shop.data\ninterface OrderRepository : org.springframework.data.jpa.repository.JpaRepository<shop.Order, Long>\n";
        let body = KT_SERVICE.replace("class OrderService", "class OrderService {\n    val count = 1\n}");
        let cases = [
            ("constructor", Style::Constructor, KT_SERVICE.to_owned()),
            ("extended", Style::Constructor, KT_SERVICE.replace("class OrderService", "class OrderService(private val count: Int)")),
            ("field", Style::Field, body),
        ];
        for (label, style, source) in cases {
            let dir = workspace(&format!("kt-{label}"));
            let rendered = render(KT_PATH, &source, style, "shop.data", false).unwrap();
            let files = [write(&dir, "Order.kt", entity), write(&dir, "OrderRepository.kt", repo), write(&dir, "OrderService.kt", &rendered)];
            let result = std::process::Command::new("java")
                .args(["-classpath", &compiler, "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler", "-no-stdlib", "-no-reflect", "-jvm-target", "17", "-classpath", &format!("{data}:{context}:{stdlib}"), "-d"])
                .arg(dir.join("out"))
                .args(&files)
                .output()
                .expect("java installed");
            assert!(result.status.success(), "{label}: {}\n{rendered}", String::from_utf8_lossy(&result.stderr));
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
