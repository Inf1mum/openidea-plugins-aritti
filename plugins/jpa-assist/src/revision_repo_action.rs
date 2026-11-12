//! Reviewed creation of Spring Data Envers revision repositories, the Blaze-Persistence entity view
//! repository, and the `@EnableEnversRepositories` configuration class. Every action creates new sibling
//! files only (`overwrite: false`); no existing file is edited.
//!
//! Facts (Spring Data JPA reference "Envers" chapter, javap of spring-data-envers 4.1.2,
//! spring-data-commons 4.1.2, hibernate-envers 6.6.49.Final, blaze-persistence-integration-spring-data-base-3.3 1.6.20
//! and the Blaze-Persistence entity view manual, chapter "Spring Data integration"):
//! - `org.springframework.data.repository.history.RevisionRepository<T, ID, N extends Number & Comparable<N>>`
//!   has `findLastChangeRevision`, `findRevisions(ID)`, `findRevisions(ID, Pageable)` and `findRevision(ID, N)`.
//! - The reference extends `RevisionRepository<Person, Long, Long>` next to `CrudRepository`, requires the entity
//!   to be `@Audited`, and says that for Envers in default configuration the revision number parameter should be
//!   `Integer` or `Long`. `DefaultRevisionEntity` has an `int` id and `DefaultRevisionMetadata` is a
//!   `RevisionMetadata<Integer>`, so `Integer` is used without a custom `@RevisionEntity`; a custom one is read
//!   reflectively from its `@RevisionNumber` field (`@RevisionNumber` also targets methods, which is refused here).
//! - `org.springframework.data.envers.repository.config.EnableEnversRepositories` is meta-annotated with
//!   `@EnableJpaRepositories`, its `repositoryFactoryBeanClass` defaults to `EnversRevisionRepositoryFactoryBean`
//!   and the reference states it "already supplies the repositoryFactoryBeanClass automatically".
//! - `com.blazebit.persistence.spring.data.repository.EntityViewRepository<T, ID extends Serializable>` is the
//!   documented base interface (`extends EntityViewRepository<SimpleCatView, Long>`); repositories are enabled with
//!   `com.blazebit.persistence.spring.data.repository.config.EnableBlazeRepositories`, and the manual lists the
//!   `blaze-persistence-integration-spring-data-<suffix>` artifacts.
use super::index_query_action as iq;
use super::repository_action as repo;
use super::*;

pub(super) const TITLE: &str = "Create revision repository…";
pub(super) const ENABLE_TITLE: &str = "Enable Envers repositories…";
pub(super) const VIEW_TITLE: &str = "Create entity view repository…";
const ENVERS: &str = "org.hibernate.envers";
const BLAZE_VIEW: &str = "com.blazebit.persistence.view";
const SPRING_DATA: &str = "org.springframework.data";
const MAX_SCANNED: u32 = 1001;
const BLAZE_SUFFIXES: [&str; 13] = ["1.x", "2.0", "2.1", "2.2", "2.3", "2.4", "2.5", "2.6", "2.7", "3.1", "3.3", "3.4", "4.0"];

fn imports_of(masked: &str) -> Vec<String> {
    masked
        .lines()
        .filter_map(|line| line.trim().strip_prefix("import "))
        .map(|rest| rest.trim().trim_end_matches(';').trim().to_owned())
        .filter(|rest| !rest.starts_with("static "))
        .collect()
}

fn owned(written: &str, package: &str, name: &str, imports: &[String]) -> bool {
    let full = format!("{package}.{name}");
    if written.contains(['.', ':']) {
        return written == full;
    }
    let explicit: Vec<_> = imports.iter().filter(|import| import.rsplit('.').next() == Some(name)).collect();
    if !explicit.is_empty() {
        return explicit.len() == 1 && *explicit[0] == full;
    }
    imports.iter().any(|import| *import == format!("{package}.*"))
}

fn directory(doc: &Document) -> Result<&str, Error> {
    Ok(doc.path.rsplit_once('/').ok_or(Error::Stale)?.0)
}

fn declared(build: &str, group: &str, artifacts: &[String]) -> bool {
    artifacts.iter().any(|artifact| {
        let needle = format!("{group}:{artifact}");
        build.match_indices(&needle).any(|(at, found)| {
            build[at + found.len()..].chars().next().is_none_or(|ch| !(ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.')))
        }) || (build.contains(&format!("<groupId>{group}</groupId>")) && build.contains(&format!("<artifactId>{artifact}</artifactId>")))
    })
}

fn proof(project: &Project, root: &str, doc: &Document, group: &str, artifacts: &[String], label: &str) -> Result<(), Error> {
    let build = super::mapper_action::build_text(project, root, directory(doc)?)?;
    if !declared(&build, group, artifacts) {
        return Err(format!(
            "{label} is not provably on this project: {group}:{} must be written literally in a pom.xml/build.gradle(.kts) above this source (version catalogs and indirect declarations are not verified); nothing was generated.",
            artifacts[0]
        )
        .into());
    }
    Ok(())
}

fn envers_proof(project: &Project, root: &str, doc: &Document) -> Result<(), Error> {
    proof(project, root, doc, SPRING_DATA, &["spring-data-envers".to_owned()], "Spring Data Envers")
}

pub(super) fn blaze_proof(project: &Project, root: &str, doc: &Document) -> Result<(), Error> {
    let artifacts: Vec<String> = BLAZE_SUFFIXES.iter().map(|suffix| format!("blaze-persistence-integration-spring-data-{suffix}")).collect();
    proof(project, root, doc, "com.blazebit", &artifacts, "The Blaze-Persistence Spring Data integration")
}

pub(super) fn scan_sources(project: &Project, root: &str, mut visit: impl FnMut(&str, &str) -> Result<(), Error>) -> Result<(), Error> {
    let deadline = Instant::now() + Duration::from_secs(5);
    for extension in ["java", "kt"] {
        let files = project.find_files(&format!("**/*.{extension}"), MAX_SCANNED)?;
        if files.len() >= MAX_SCANNED as usize {
            return Err("The project has too many sources to scan completely; nothing was generated.".into());
        }
        for file in files {
            if Instant::now() >= deadline {
                return Err("Project scan timed out; nothing was generated.".into());
            }
            let file = if file.starts_with('/') { file } else { format!("{}/{file}", root.trim_end_matches('/')) };
            if path(&file).as_deref() != Some(file.as_str()) || !inside(root, &file) {
                continue;
            }
            let text = match project.read_file(&file) {
                Ok(text) => text,
                Err(Error::NotFound(_) | Error::Invalid(_)) => continue,
                Err(error) => return Err(error),
            };
            visit(&file, &text)?;
        }
    }
    Ok(())
}

fn audited_source(doc: &Document) -> Result<iq::Source, String> {
    let s = iq::source(doc).ok_or("Select a plain entity with no superclass.")?;
    let verdict = {
        let tops = iq::top(&s.masked, s.class_at);
        let marks: Vec<_> = tops.iter().filter(|a| a.name == "Audited").collect();
        match marks.as_slice() {
            [] => Err("The entity has no class-level @Audited; Spring Data Envers requires an audited entity.".to_owned()),
            [mark] if owned(&s.masked[mark.start + 1..mark.name_end], ENVERS, "Audited", &s.class.imports) => Ok(()),
            [_] => Err(format!("@Audited here is not {ENVERS}.Audited; nothing was generated.")),
            _ => Err("The class repeats @Audited; edit manually.".to_owned()),
        }
    };
    verdict.map(|()| s)
}

pub(super) fn eligible(doc: &Document) -> bool {
    iq::source(doc).is_some_and(|s| iq::top(&s.masked, s.class_at).iter().any(|a| a.name == "Audited"))
}

fn revision_number_of(file: &str, masked: &str) -> Result<&'static str, Error> {
    let refuse = |why: &str| Err(Error::from(format!("{file}: {why}; the revision number type cannot be proven, nothing was generated.")));
    if file.ends_with(".kt") {
        return refuse("a Kotlin @RevisionEntity is not resolved");
    }
    let imports = imports_of(masked);
    let all = annotations(masked);
    for a in all.iter().filter(|a| matches!(a.name, "RevisionEntity" | "RevisionNumber")) {
        if !owned(&masked[a.start + 1..a.name_end], ENVERS, a.name, &imports) {
            return refuse(&format!("@{} is not {ENVERS}.{}", a.name, a.name));
        }
    }
    let classes: Vec<_> = words(masked, "class").collect();
    let [class_at] = classes.as_slice() else {
        return refuse("expected exactly one class");
    };
    let open = masked[*class_at..].find('{').map(|at| at + class_at).ok_or(Error::Stale)?;
    let end = masked.rfind('}').ok_or(Error::Stale)?;
    let numbers: Vec<_> = all.iter().filter(|a| a.name == "RevisionNumber").collect();
    match numbers.as_slice() {
        [] => {
            let header: Vec<_> = masked[*class_at..open].split_whitespace().skip(2).collect();
            let default_entity = imports.iter().any(|import| import == &format!("{ENVERS}.DefaultRevisionEntity"));
            match header.as_slice() {
                ["extends", "DefaultRevisionEntity"] if default_entity => Ok("Integer"),
                ["extends", full] if *full == format!("{ENVERS}.DefaultRevisionEntity") => Ok("Integer"),
                _ => refuse("no @RevisionNumber field and no DefaultRevisionEntity superclass"),
            }
        }
        [number] => {
            let members = crate::model::members(masked, open + 1, end, false, false);
            let Some(&(start, finish)) = members.iter().find(|(s, f)| *s <= number.start && number.start < *f) else {
                return refuse("@RevisionNumber is not on a field");
            };
            if !masked[start..finish].trim_end().ends_with(';') {
                return refuse("@RevisionNumber is not on a field (method placement is not resolved)");
            }
            let clean = crate::model::without_annotations(masked, start, finish);
            let Some((_, _, ty)) = crate::model::declaration(&clean, false) else {
                return refuse("the @RevisionNumber field is not understood");
            };
            match ty.as_str() {
                "int" | "Integer" => Ok("Integer"),
                "long" | "Long" => Ok("Long"),
                other => refuse(&format!("the @RevisionNumber type `{other}` is not int/long (Integer/Long)")),
            }
        }
        _ => refuse("several @RevisionNumber annotations"),
    }
}

/// (revision number type, human readable basis) from a bounded scan of every Java/Kotlin source.
fn revision_number(project: &Project, root: &str) -> Result<(&'static str, String), Error> {
    let mut found: Vec<(String, &'static str)> = Vec::new();
    scan_sources(project, root, |file, text| {
        if !text.contains("RevisionEntity") {
            return Ok(());
        }
        if text.len() > MAX_TEXT {
            return Err(format!("{file} is too large to inspect for @RevisionEntity; nothing was generated.").into());
        }
        let masked = mask(text);
        let marks = annotations(&masked).into_iter().filter(|a| a.name == "RevisionEntity").count();
        match marks {
            0 => Ok(()),
            1 => {
                found.push((file.to_owned(), revision_number_of(file, &masked)?));
                Ok(())
            }
            _ => Err(format!("{file} repeats @RevisionEntity; nothing was generated.").into()),
        }
    })?;
    match found.as_slice() {
        [] => Ok(("Integer", "no @RevisionEntity in the scanned sources, so Envers' DefaultRevisionEntity (int id; Spring Data Envers types its metadata as Integer) is assumed; a revision entity declared in XML or a dependency is not seen".into())),
        [(file, number)] => Ok((number, format!("custom @RevisionEntity in {file} with a {number} @RevisionNumber"))),
        many => Err(format!("Several @RevisionEntity classes ({}); Envers allows one, so no revision number type is guessed.", many.iter().map(|(f, _)| file_stem(f)).collect::<Vec<_>>().join(", ")).into()),
    }
}

fn crlf(doc: &Document, text: String) -> String {
    if doc.text.contains("\r\n") { text.replace('\n', "\r\n") } else { text }
}

fn checked_name(name: &str, taken: &[&str]) -> Result<(), Error> {
    if !identifier(name) || taken.contains(&name) {
        return Err("Choose a valid name that does not clash with the entity, view or imported types.".into());
    }
    Ok(())
}

fn render_revision(doc: &Document, facts: &repo::Facts, number: &str, name: &str) -> Result<(String, String), Error> {
    let package_name = package(doc).ok_or(Error::Stale)?;
    let mut imports = vec!["org.springframework.data.jpa.repository.JpaRepository".to_owned(), "org.springframework.data.repository.history.RevisionRepository".to_owned()];
    if facts.id_type == "UUID" {
        imports.push("java.util.UUID".into());
    }
    imports.sort();
    let mut taken: Vec<&str> = imports.iter().filter_map(|i| i.rsplit('.').next()).collect();
    taken.push(&facts.entity);
    checked_name(name, &taken)?;
    let revision = if facts.kotlin && number == "Integer" { "Int" } else { number };
    let (entity, id) = (&facts.entity, &facts.id_type);
    let supers = format!("RevisionRepository<{entity}, {id}, {revision}>, JpaRepository<{entity}, {id}>");
    let semicolon = if facts.kotlin { "" } else { ";" };
    let imports: String = imports.iter().map(|i| format!("import {i}{semicolon}\n")).collect();
    let text = if facts.kotlin {
        format!("package {package_name}\n\n{imports}\ninterface {name} : {supers}\n")
    } else {
        format!("package {package_name};\n\n{imports}\npublic interface {name} extends {supers} {{\n}}\n")
    };
    finish(directory(doc)?, name, if facts.kotlin { "kt" } else { "java" }, crlf(doc, text))
}

fn finish(directory: &str, name: &str, extension: &str, text: String) -> Result<(String, String), Error> {
    if text.len() > MAX_TEXT {
        return Err("Generated source exceeds the supported bound.".into());
    }
    Ok((format!("{directory}/{name}.{extension}"), text))
}

fn confirm_and_build(
    pending: &Pending,
    doc: &Document,
    title: &str,
    heading: &str,
    notes: &str,
    target: String,
    text: String,
    name: &str,
    recheck: impl Fn(&Project) -> Result<(), Error>,
) -> Result<FileAction, Error> {
    if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
        return Err(Error::Stale);
    }
    if !ui_kit::prompts::confirm_exact(
        title,
        &format!("Source: {}\nSource version: {}\nNew file: {target}\n\n{text}\n{notes}\nNo existing file is overwritten.", doc.path, doc.version),
    )? {
        return Err(Error::Cancelled);
    }
    let project = current(pending, doc)?;
    recheck(&project)?;
    no_collision(&project, &pending.root, directory(doc)?, name, None)?;
    Ok(FileAction {
        title: format!("{heading} {name}"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target, text, overwrite: false }],
        ..FileAction::default()
    })
}

pub(super) fn resolve_revision(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    audited_source(doc)?;
    let facts = repo::source(doc).map_err(Error::from)?;
    envers_proof(project, &pending.root, doc)?;
    let (number, basis) = revision_number(project, &pending.root)?;
    let default = format!("{}RevisionRepository", facts.entity);
    let name = host::input("Revision repository name (same package)", Some(&default))?.ok_or(Error::Cancelled)?.trim().to_owned();
    current(pending, doc)?;
    let (target, text) = render_revision(doc, &facts, number, &name)?;
    no_collision(project, &pending.root, directory(doc)?, &name, None)?;
    let notes = format!(
        "Revision number type {number}: {basis}.\nThe ID type comes from the @Id field of this source only. The entity is class-level @Audited (org.hibernate.envers) and spring-data-envers is declared literally in the build.\n@EnableEnversRepositories (or repositoryFactoryBeanClass = EnversRevisionRepositoryFactoryBean.class on @EnableJpaRepositories) is required for this repository to work; use \"{ENABLE_TITLE}\" to create a configuration class. Not compiled against your project's Spring Data version."
    );
    let root = pending.root.clone();
    let doc_for_check = doc.clone();
    confirm_and_build(pending, doc, "Create revision repository?", "Create revision repository", &notes, target, text, &name, move |project| {
        audited_source(&doc_for_check)?;
        envers_proof(project, &root, &doc_for_check)?;
        match revision_number(project, &root)? {
            (again, _) if again == number => Ok(()),
            _ => Err(Error::Stale),
        }
    })
}

fn render_enable(doc: &Document, name: &str) -> Result<(String, String), Error> {
    let package_name = package(doc).ok_or(Error::Stale)?;
    checked_name(name, &["Configuration", "EnableEnversRepositories"])?;
    let text = format!(
        "package {package_name};\n\nimport org.springframework.context.annotation.Configuration;\nimport org.springframework.data.envers.repository.config.EnableEnversRepositories;\n\n@Configuration\n@EnableEnversRepositories\npublic class {name} {{\n}}\n"
    );
    finish(directory(doc)?, name, "java", crlf(doc, text))
}

fn no_existing_enablement(project: &Project, root: &str) -> Result<(), Error> {
    scan_sources(project, root, |file, text| {
        if !["EnableEnversRepositories", "EnversRevisionRepositoryFactoryBean", "EnableJpaRepositories"].iter().any(|word| text.contains(word)) {
            return Ok(());
        }
        let masked = mask(text);
        if has_word(&masked, "EnableEnversRepositories") || has_word(&masked, "EnversRevisionRepositoryFactoryBean") {
            return Err(format!("{file} already enables Envers repositories; nothing was generated.").into());
        }
        if has_word(&masked, "EnableJpaRepositories") {
            return Err(format!("{file} has @EnableJpaRepositories; a second scan would overlap it. Add repositoryFactoryBeanClass = EnversRevisionRepositoryFactoryBean.class there instead; nothing was generated.").into());
        }
        Ok(())
    })
}

pub(super) fn resolve_enable(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    if doc.language != "java" {
        return Err("The configuration class is generated for Java projects only; Kotlin is refused (a final @Configuration class would need the kotlin-spring plugin).".into());
    }
    audited_source(doc)?;
    envers_proof(project, &pending.root, doc)?;
    no_existing_enablement(project, &pending.root)?;
    let name = host::input("Configuration class name (same package)", Some("EnversRepositoriesConfiguration"))?.ok_or(Error::Cancelled)?.trim().to_owned();
    current(pending, doc)?;
    let (target, text) = render_enable(doc, &name)?;
    no_collision(project, &pending.root, directory(doc)?, &name, None)?;
    let package_name = package(doc).ok_or(Error::Stale)?;
    let notes = format!(
        "@EnableEnversRepositories sets no attributes: it is meta-annotated with @EnableJpaRepositories and its repositoryFactoryBeanClass defaults to EnversRevisionRepositoryFactoryBean. With no base package Spring Data scans the annotated class's package ({package_name}) and its subpackages; repositories elsewhere need basePackages. No other @EnableEnversRepositories, EnversRevisionRepositoryFactoryBean or @EnableJpaRepositories exists in the scanned Java/Kotlin sources; XML repository configuration and Spring Boot auto-configuration were not inspected. Not compiled against your project's Spring Data version."
    );
    let root = pending.root.clone();
    let doc_for_check = doc.clone();
    confirm_and_build(pending, doc, "Enable Envers repositories?", "Create Envers repositories configuration", &notes, target, text, &name, move |project| {
        audited_source(&doc_for_check)?;
        envers_proof(project, &root, &doc_for_check)?;
        no_existing_enablement(project, &root)
    })
}

struct View {
    name: String,
    id_type: String,
    uuid: bool,
}

fn view_source(doc: &Document) -> Result<View, String> {
    if doc.language != "java" {
        return Err("Entity view repositories are generated for Java entity views only.".into());
    }
    let masked = mask(&doc.text);
    let imports = imports_of(&masked);
    let interfaces: Vec<_> = words(&masked, "interface").filter(|at| *at == 0 || masked.as_bytes()[at - 1] != b'@').collect();
    let [at] = interfaces.as_slice() else {
        return Err("Expected exactly one top-level interface.".into());
    };
    if words(&masked, "class").any(|at| at == 0 || masked.as_bytes()[at - 1] != b'.') || words(&masked, "record").next().is_some() || words(&masked, "enum").next().is_some() {
        return Err("Expected a plain @EntityView interface.".into());
    }
    let open = masked[*at..].find('{').map(|i| i + at).ok_or("Stale source.")?;
    let header: Vec<_> = masked[*at..open].split_whitespace().collect();
    let ["interface", name] = header.as_slice() else {
        return Err("The view must have no generics or extends clause.".into());
    };
    let end = masked.rfind('}').ok_or("Stale source.")?;
    if !masked[end + 1..].trim().is_empty() || !identifier(name) {
        return Err("Unsupported interface layout.".into());
    }
    let tops = iq::top(&masked, *at);
    let views: Vec<_> = tops.iter().filter(|a| a.name == "EntityView").collect();
    let [view] = views.as_slice() else {
        return Err("The interface needs exactly one @EntityView.".into());
    };
    let all = annotations(&masked);
    let ids: Vec<_> = all.iter().filter(|a| a.name == "IdMapping").collect();
    let [id] = ids.as_slice() else {
        return Err("The view needs exactly one @IdMapping.".into());
    };
    for (a, label) in [(view, "EntityView"), (id, "IdMapping")] {
        if !owned(&masked[a.start + 1..a.name_end], BLAZE_VIEW, label, &imports) {
            return Err(format!("@{label} is not {BLAZE_VIEW}.{label}."));
        }
    }
    let members = crate::model::members(&masked, open + 1, end, false, false);
    let &(start, finish) = members.iter().find(|(s, f)| *s <= id.start && id.start < *f).ok_or("@IdMapping is not on a member.")?;
    if !masked[start..finish].trim_end().ends_with(';') {
        return Err("@IdMapping must be on an abstract getter.".into());
    }
    let clean = crate::model::without_annotations(&masked, start, finish);
    let tokens: Vec<_> = clean.trim().trim_end_matches(';').split_whitespace().filter(|t| !matches!(*t, "public" | "abstract")).collect();
    let [ty, getter] = tokens.as_slice() else {
        return Err("The @IdMapping getter shape is not understood.".into());
    };
    if !getter.ends_with("()") || getter.len() <= 2 {
        return Err("The @IdMapping member must be a getter without parameters.".into());
    }
    let (id_type, uuid) = match *ty {
        "Long" | "Integer" | "Short" | "Byte" | "Character" | "String" => ((*ty).to_owned(), false),
        "long" => ("Long".into(), false),
        "int" => ("Integer".into(), false),
        "short" => ("Short".into(), false),
        "byte" => ("Byte".into(), false),
        "char" => ("Character".into(), false),
        "UUID" if imports.iter().any(|import| import == "java.util.UUID") => ("UUID".into(), true),
        other => return Err(format!("The view id type `{other}` is not a supported simple type (wrapper, primitive, String or imported UUID).")),
    };
    Ok(View { name: (*name).to_owned(), id_type, uuid })
}

pub(super) fn view_ok(doc: &Document) -> bool {
    view_source(doc).is_ok()
}

pub(super) fn eligible_view(doc: &Document) -> bool {
    doc.text.contains("EntityView") && view_source(doc).is_ok()
}

fn render_view(doc: &Document, view: &View, name: &str) -> Result<(String, String), Error> {
    let package_name = package(doc).ok_or(Error::Stale)?;
    let mut imports = vec!["com.blazebit.persistence.spring.data.repository.EntityViewRepository".to_owned()];
    if view.uuid {
        imports.push("java.util.UUID".into());
    }
    imports.sort();
    let mut taken: Vec<&str> = imports.iter().filter_map(|i| i.rsplit('.').next()).collect();
    taken.push(&view.name);
    checked_name(name, &taken)?;
    let imports: String = imports.iter().map(|i| format!("import {i};\n")).collect();
    let text = format!(
        "package {package_name};\n\n{imports}\npublic interface {name} extends EntityViewRepository<{}, {}> {{\n}}\n",
        view.name, view.id_type
    );
    finish(directory(doc)?, name, "java", crlf(doc, text))
}

fn blaze_enabled(project: &Project, root: &str) -> Option<bool> {
    let mut found = false;
    scan_sources(project, root, |_, text| {
        found |= text.contains("EnableBlazeRepositories") && has_word(&mask(text), "EnableBlazeRepositories");
        Ok(())
    })
    .ok()?;
    Some(found)
}

pub(super) fn resolve_view(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let view = view_source(doc)?;
    blaze_proof(project, &pending.root, doc)?;
    let default = format!("{}Repository", view.name);
    let name = host::input("Entity view repository name (same package)", Some(&default))?.ok_or(Error::Cancelled)?.trim().to_owned();
    current(pending, doc)?;
    let (target, text) = render_view(doc, &view, &name)?;
    no_collision(project, &pending.root, directory(doc)?, &name, None)?;
    let enabled = match blaze_enabled(project, &pending.root) {
        Some(true) => "@EnableBlazeRepositories was found in the scanned sources.",
        Some(false) => "@EnableBlazeRepositories was NOT found in the scanned sources; the manual says to annotate a configuration or application class with it.",
        None => "Whether @EnableBlazeRepositories exists could not be determined.",
    };
    let notes = format!(
        "ID type {} comes from the @IdMapping getter of this view. EntityViewRepository<V, ID> is the base interface documented in the Blaze-Persistence manual (Spring Data integration); it also needs CriteriaBuilderFactory and EntityViewManager beans. {enabled} The manual's example adds @Transactional(readOnly = true); it is not generated. A blaze-persistence-integration-spring-data artifact is declared literally in the build; not compiled against your project's versions.",
        view.id_type
    );
    let root = pending.root.clone();
    let doc_for_check = doc.clone();
    confirm_and_build(pending, doc, "Create entity view repository?", "Create entity view repository", &notes, target, text, &name, move |project| {
        view_source(&doc_for_check)?;
        blaze_proof(project, &root, &doc_for_check)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ENTITY_PATH: &str = "/project/src/main/java/shop/Order.java";
    const KOTLIN_PATH: &str = "/project/src/main/kotlin/shop/Order.kt";
    const VIEW_PATH: &str = "/project/src/main/java/shop/OrderView.java";
    const ENVERS_POM: &str = "<project><dependencies><dependency><groupId>org.springframework.data</groupId><artifactId>spring-data-envers</artifactId></dependency></dependencies></project>";
    const BLAZE_POM: &str = "<project><dependencies><dependency><groupId>com.blazebit</groupId><artifactId>blaze-persistence-integration-spring-data-3.4</artifactId></dependency></dependencies></project>";
    const ENTITY: &str = "package shop;\nimport jakarta.persistence.*;\nimport org.hibernate.envers.Audited;\n@Entity\n@Audited\npublic class Order {\n    @Id Long id;\n}\n";
    const KOTLIN_ENTITY: &str = "package shop\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\nimport org.hibernate.envers.Audited\n@Entity\n@Audited\nclass Order { @Id var id: Int? = null }\n";
    const VIEW: &str = "package shop;\n\nimport com.blazebit.persistence.view.EntityView;\nimport com.blazebit.persistence.view.IdMapping;\n\n@EntityView(Order.class)\npublic interface OrderView {\n\n    @IdMapping\n    Long getId();\n\n    String getTitle();\n}\n";
    const CUSTOM_REVISION: &str = "package shop;\nimport jakarta.persistence.*;\nimport org.hibernate.envers.*;\n@Entity\n@RevisionEntity\npublic class AppRevision {\n    @Id @GeneratedValue @RevisionNumber private long id;\n    @RevisionTimestamp private long timestamp;\n}\n";

    fn setup(path: &str, text: &str, language: &str, files: &[(&str, &str)]) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(path, language, text, &[]);
        for (name, body) in files {
            host.add_file(name, body);
        }
        (host, JpaAssist::default(), Document { path: path.into(), text: text.into(), language: language.into(), version: 1 })
    }

    fn run(path: &str, text: &str, language: &str, files: &[(&str, &str)], title: &str, name: &str) -> Result<(String, String), Error> {
        let (host, mut plugin, doc) = setup(path, text, language, files);
        let action = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == title).expect("action offered");
        assert!(action.operations.is_empty() && action.edits.is_empty());
        host.answer_next_prompt(Some(&[name]));
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&doc, action)?;
        let [FileOperation::Create { path, text, overwrite: false }] = result.operations.as_slice() else { panic!("one non-overwriting create") };
        assert!(result.edits.is_empty());
        Ok((path.clone(), text.clone()))
    }

    fn revision(entity: &str, files: &[(&str, &str)], name: &str) -> Result<(String, String), Error> {
        run(ENTITY_PATH, entity, "java", files, TITLE, name)
    }

    #[test]
    fn default_revision_number_is_integer() {
        let (path, text) = revision(ENTITY, &[("pom.xml", ENVERS_POM)], "OrderRevisionRepository").unwrap();
        assert_eq!(path, "/project/src/main/java/shop/OrderRevisionRepository.java");
        assert_eq!(
            text,
            "package shop;\n\nimport org.springframework.data.jpa.repository.JpaRepository;\nimport org.springframework.data.repository.history.RevisionRepository;\n\npublic interface OrderRevisionRepository extends RevisionRepository<Order, Long, Integer>, JpaRepository<Order, Long> {\n}\n"
        );
        let gradle = [("build.gradle.kts", "implementation(\"org.springframework.data:spring-data-envers\")")];
        assert!(revision(ENTITY, &gradle, "OrderRevisionRepository").is_ok());
    }

    #[test]
    fn custom_revision_entity_decides_the_number_type() {
        let files = [("pom.xml", ENVERS_POM), ("src/main/java/shop/AppRevision.java", CUSTOM_REVISION)];
        let (_, text) = revision(ENTITY, &files, "OrderRevisionRepository").unwrap();
        assert!(text.contains("RevisionRepository<Order, Long, Long>"), "{text}");
        let int_field = CUSTOM_REVISION.replace("private long id", "private Integer id");
        let files = [("pom.xml", ENVERS_POM), ("src/main/java/shop/AppRevision.java", int_field.as_str())];
        assert!(revision(ENTITY, &files, "R").unwrap().1.contains("<Order, Long, Integer>"));
        let extending = "package shop;\nimport jakarta.persistence.Entity;\nimport org.hibernate.envers.DefaultRevisionEntity;\nimport org.hibernate.envers.RevisionEntity;\n@Entity\n@RevisionEntity\npublic class AppRevision extends DefaultRevisionEntity {\n}\n";
        let files = [("pom.xml", ENVERS_POM), ("src/main/java/shop/AppRevision.java", extending)];
        assert!(revision(ENTITY, &files, "R").unwrap().1.contains("<Order, Long, Integer>"));
    }

    #[test]
    fn revision_scan_refusals() {
        let second = CUSTOM_REVISION.replace("AppRevision", "OtherRevision");
        let short = CUSTOM_REVISION.replace("private long id", "private short id");
        let getter = "package shop;\nimport org.hibernate.envers.*;\n@RevisionEntity\npublic class AppRevision {\n    @RevisionNumber public int getId() { return 0; }\n}\n";
        let unknown = "package shop;\nimport org.hibernate.envers.RevisionEntity;\n@RevisionEntity\npublic class AppRevision extends Base {\n}\n";
        let foreign = CUSTOM_REVISION.replace("import org.hibernate.envers.*;", "import other.*;");
        let kotlin = "package shop\nimport org.hibernate.envers.RevisionEntity\n@RevisionEntity\nclass AppRevision\n";
        let cases: Vec<Vec<(&str, &str)>> = vec![
            vec![("pom.xml", ENVERS_POM), ("src/main/java/shop/A.java", CUSTOM_REVISION), ("src/main/java/shop/B.java", second.as_str())],
            vec![("pom.xml", ENVERS_POM), ("src/main/java/shop/A.java", short.as_str())],
            vec![("pom.xml", ENVERS_POM), ("src/main/java/shop/A.java", getter)],
            vec![("pom.xml", ENVERS_POM), ("src/main/java/shop/A.java", unknown)],
            vec![("pom.xml", ENVERS_POM), ("src/main/java/shop/A.java", foreign.as_str())],
            vec![("pom.xml", ENVERS_POM), ("src/main/kotlin/shop/A.kt", kotlin)],
        ];
        for (index, files) in cases.iter().enumerate() {
            assert!(revision(ENTITY, files, "R").is_err(), "case {index}");
        }
    }

    #[test]
    fn revision_dependency_audit_and_name_refusals() {
        for files in [vec![], vec![("pom.xml", "<project/>")], vec![("pom.xml", "<artifactId>spring-data-envers</artifactId>")], vec![("build.gradle", "implementation 'org.springframework.data:spring-data-envers-extras:1'")]] {
            assert!(revision(ENTITY, &files, "R").is_err());
        }
        let files = [("pom.xml", ENVERS_POM)];
        for name in ["", "Order", "JpaRepository", "RevisionRepository", "class", "a b"] {
            assert!(revision(ENTITY, &files, name).is_err(), "{name}");
        }
        assert!(revision(ENTITY, &[("pom.xml", ENVERS_POM), ("src/main/java/shop/R.java", "class R {}")], "R").is_err());
        let not_audited = ENTITY.replace("@Audited\n", "");
        let (_host, mut plugin, doc) = setup(ENTITY_PATH, &not_audited, "java", &files);
        assert!(!plugin.file_actions(&doc, Range::default()).iter().any(|a| a.title == TITLE || a.title == ENABLE_TITLE));
        let foreign = ENTITY.replace("import org.hibernate.envers.Audited;", "import other.Audited;");
        assert!(audited_source(&Document { path: ENTITY_PATH.into(), language: "java".into(), text: foreign, version: 1 }).is_err());
        let composite = ENTITY.replace("@Id Long id;", "@EmbeddedId OrderKey id;");
        assert!(revision(&composite, &files, "R").is_err());
    }

    #[test]
    fn revision_uuid_kotlin_and_crlf() {
        let uuid = ENTITY.replace("import org.hibernate", "import java.util.UUID;\nimport org.hibernate").replace("Long id", "UUID id");
        let (_, text) = revision(&uuid, &[("pom.xml", ENVERS_POM)], "R").unwrap();
        assert!(text.contains("import java.util.UUID;\nimport org.springframework.data.jpa") && text.contains("<Order, UUID, Integer>, JpaRepository<Order, UUID>"), "{text}");
        let (path, text) = run(KOTLIN_PATH, KOTLIN_ENTITY, "kotlin", &[("pom.xml", ENVERS_POM)], TITLE, "OrderRevisionRepository").unwrap();
        assert_eq!(path, "/project/src/main/kotlin/shop/OrderRevisionRepository.kt");
        assert_eq!(
            text,
            "package shop\n\nimport org.springframework.data.jpa.repository.JpaRepository\nimport org.springframework.data.repository.history.RevisionRepository\n\ninterface OrderRevisionRepository : RevisionRepository<Order, Int, Int>, JpaRepository<Order, Int>\n"
        );
        for (path, source, language) in [(ENTITY_PATH, ENTITY.replace('\n', "\r\n"), "java"), (KOTLIN_PATH, KOTLIN_ENTITY.replace('\n', "\r\n"), "kotlin")] {
            let text = run(path, &source, language, &[("pom.xml", ENVERS_POM)], TITLE, "R").unwrap().1;
            assert!(text.contains("\r\n") && !text.replace("\r\n", "").contains('\n'), "{language}");
        }
    }

    #[test]
    fn revision_cancel_and_drift_change_nothing() {
        let (host, mut plugin, doc) = setup(ENTITY_PATH, ENTITY, "java", &[("pom.xml", ENVERS_POM)]);
        let action = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.answer_next_prompt(Some(&["R"]));
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, action), Err(Error::Cancelled)));
        let action = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        let drifted = Document { version: 2, ..doc.clone() };
        assert!(matches!(plugin.resolve_file_action(&drifted, action), Err(Error::Stale)));
    }

    #[test]
    fn enable_class_is_exact_and_refuses_existing_enablement() {
        let files = [("pom.xml", ENVERS_POM)];
        let (path, text) = run(ENTITY_PATH, ENTITY, "java", &files, ENABLE_TITLE, "EnversConfig").unwrap();
        assert_eq!(path, "/project/src/main/java/shop/EnversConfig.java");
        assert_eq!(
            text,
            "package shop;\n\nimport org.springframework.context.annotation.Configuration;\nimport org.springframework.data.envers.repository.config.EnableEnversRepositories;\n\n@Configuration\n@EnableEnversRepositories\npublic class EnversConfig {\n}\n"
        );
        let already = "package app;\n@EnableEnversRepositories\nclass A {}\n";
        let factory = "package app;\n@EnableJpaRepositories(repositoryFactoryBeanClass = EnversRevisionRepositoryFactoryBean.class)\nclass A {}\n";
        let plain_jpa = "package app;\n@EnableJpaRepositories\nclass A {}\n";
        for existing in [already, factory, plain_jpa] {
            let files = [("pom.xml", ENVERS_POM), ("src/main/java/app/A.java", existing)];
            assert!(run(ENTITY_PATH, ENTITY, "java", &files, ENABLE_TITLE, "EnversConfig").is_err());
        }
        let commented = "package app;\n// @EnableEnversRepositories\nclass A {}\n";
        let files = [("pom.xml", ENVERS_POM), ("src/main/java/app/A.java", commented)];
        assert!(run(ENTITY_PATH, ENTITY, "java", &files, ENABLE_TITLE, "EnversConfig").is_ok());
        assert!(run(ENTITY_PATH, ENTITY, "java", &[], ENABLE_TITLE, "EnversConfig").is_err());
        assert!(run(ENTITY_PATH, ENTITY, "java", &files, ENABLE_TITLE, "Configuration").is_err());
        assert!(run(KOTLIN_PATH, KOTLIN_ENTITY, "kotlin", &[("pom.xml", ENVERS_POM)], ENABLE_TITLE, "EnversConfig").is_err());
        let crlf = run(ENTITY_PATH, &ENTITY.replace('\n', "\r\n"), "java", &[("pom.xml", ENVERS_POM)], ENABLE_TITLE, "EnversConfig").unwrap().1;
        assert!(!crlf.replace("\r\n", "").contains('\n'));
    }

    fn view(text: &str, files: &[(&str, &str)], name: &str) -> Result<(String, String), Error> {
        run(VIEW_PATH, text, "java", files, VIEW_TITLE, name)
    }

    #[test]
    fn view_repository_is_exact_and_boxes_ids() {
        let files = [("pom.xml", BLAZE_POM)];
        let (path, text) = view(VIEW, &files, "OrderViewRepository").unwrap();
        assert_eq!(path, "/project/src/main/java/shop/OrderViewRepository.java");
        assert_eq!(
            text,
            "package shop;\n\nimport com.blazebit.persistence.spring.data.repository.EntityViewRepository;\n\npublic interface OrderViewRepository extends EntityViewRepository<OrderView, Long> {\n}\n"
        );
        assert!(view(&VIEW.replace("Long getId", "int getId"), &files, "R").unwrap().1.contains("<OrderView, Integer>"));
        let uuid = VIEW.replace("import com.blazebit.persistence.view.IdMapping;", "import com.blazebit.persistence.view.IdMapping;\nimport java.util.UUID;").replace("Long getId", "UUID getId");
        assert!(view(&uuid, &files, "R").unwrap().1.contains("import java.util.UUID;\n\npublic interface R extends EntityViewRepository<OrderView, UUID>"));
        let gradle = [("build.gradle", "implementation 'com.blazebit:blaze-persistence-integration-spring-data-4.0:1.6.20'")];
        assert!(view(VIEW, &gradle, "R").is_ok());
        let crlf = view(&VIEW.replace('\n', "\r\n"), &files, "R").unwrap().1;
        assert!(!crlf.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn view_repository_refusals() {
        let files = [("pom.xml", BLAZE_POM)];
        for bad in [vec![], vec![("pom.xml", "<project/>")], vec![("build.gradle", "implementation 'com.blazebit:blaze-persistence-entity-view-api:1'")]] {
            assert!(view(VIEW, &bad, "R").is_err());
        }
        for name in ["", "OrderView", "EntityViewRepository", "1x"] {
            assert!(view(VIEW, &files, name).is_err(), "{name}");
        }
        assert!(view(VIEW, &[("pom.xml", BLAZE_POM), ("src/main/java/shop/R.java", "class R {}")], "R").is_err());
        let ineligible = [
            VIEW.replace("import com.blazebit.persistence.view.EntityView;", "import other.EntityView;"),
            VIEW.replace("    @IdMapping\n    Long getId();", "    Long getId();"),
            VIEW.replace("public interface OrderView", "public interface OrderView extends Base"),
            VIEW.replace("Long getId()", "BigDecimal getId()"),
            VIEW.replace("Long getId()", "Long getId(int x)"),
            VIEW.replace("Long getId();", "default Long getId() { return null; }"),
            VIEW.replace("public interface OrderView", "public abstract class OrderView"),
            VIEW.replace("@EntityView(Order.class)\n", ""),
            VIEW.replace("Long getId()", "UUID getId()"),
        ];
        for (index, text) in ineligible.iter().enumerate() {
            let doc = Document { path: VIEW_PATH.into(), language: "java".into(), text: text.clone(), version: 1 };
            assert!(view_source(&doc).is_err(), "case {index}");
        }
        let (_host, mut plugin, doc) = setup(VIEW_PATH, &ineligible[1], "java", &files);
        assert!(!plugin.file_actions(&doc, Range::default()).iter().any(|a| a.title == VIEW_TITLE));
        let (host, mut plugin, doc) = setup(VIEW_PATH, VIEW, "java", &files);
        let action = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == VIEW_TITLE).unwrap();
        host.answer_next_prompt(Some(&["R"]));
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, action), Err(Error::Cancelled)));
    }

    #[test]
    #[ignore = "requires javac and SPRING_DATA_ENVERS_CLASSPATH (spring-data-envers 4.1.2, spring-data-commons 4.1.2, spring-data-jpa 4.1.2, spring-core/context/beans 7.0.x, jakarta.persistence-api) and BLAZE_SPRING_DATA_CLASSPATH (blaze-persistence-integration-spring-data-base-3.3 plus spring-data-commons)"]
    fn generated_sources_compile_against_spring_data_envers_and_blaze() {
        let envers = std::env::var("SPRING_DATA_ENVERS_CLASSPATH").expect("explicit Envers classpath");
        let blaze = std::env::var("BLAZE_SPRING_DATA_CLASSPATH").expect("explicit Blaze classpath");
        let base = std::env::temp_dir().join(format!("jpa-envers-compile-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let entity = ENTITY.replace("import org.hibernate.envers.Audited;\n", "").replace("@Audited\n", "");
        let envers_files = [
            ("Order.java", entity),
            ("A.java", revision(ENTITY, &[("pom.xml", ENVERS_POM)], "A").unwrap().1),
            ("B.java", revision(ENTITY, &[("pom.xml", ENVERS_POM), ("src/main/java/shop/AppRevision.java", CUSTOM_REVISION)], "B").unwrap().1),
            ("C.java", run(ENTITY_PATH, ENTITY, "java", &[("pom.xml", ENVERS_POM)], ENABLE_TITLE, "C").unwrap().1),
        ];
        let blaze_files = [
            ("OrderView.java", VIEW.to_owned()),
            ("D.java", view(VIEW, &[("pom.xml", BLAZE_POM)], "D").unwrap().1),
        ];
        let stubs = [
            ("com/blazebit/persistence/view/EntityView.java", "package com.blazebit.persistence.view; public @interface EntityView { Class<?> value(); }"),
            ("com/blazebit/persistence/view/IdMapping.java", "package com.blazebit.persistence.view; public @interface IdMapping {}"),
        ];
        let write = |dir: &std::path::Path, name: &str, body: &str| {
            let file = dir.join(name);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, body).unwrap();
            file
        };
        let compile = |dir: &std::path::Path, classpath: &str, files: Vec<std::path::PathBuf>| {
            let result = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", classpath, "-d"]).arg(dir).args(&files).output().expect("javac installed");
            assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        };
        let dir = base.join("envers");
        compile(&dir, &envers, envers_files.iter().map(|(n, b)| write(&dir, &format!("shop/{n}"), b)).collect());
        let dir = base.join("blaze");
        let mut files: Vec<_> = blaze_files.iter().map(|(n, b)| write(&dir, &format!("shop/{n}"), b)).collect();
        files.push(write(&dir, "shop/Order.java", "package shop; public class Order {}"));
        files.extend(stubs.iter().map(|(n, b)| write(&dir, n, b)));
        compile(&dir, &blaze, files);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    #[ignore = "requires the Kotlin compiler (KOTLIN_COMPILER_CLASSPATH, KOTLIN_STDLIB) and SPRING_DATA_ENVERS_CLASSPATH"]
    fn generated_kotlin_revision_repository_compiles() {
        let envers = std::env::var("SPRING_DATA_ENVERS_CLASSPATH").expect("explicit Envers classpath");
        let compiler = std::env::var("KOTLIN_COMPILER_CLASSPATH").expect("explicit Kotlin compiler classpath");
        let stdlib = std::env::var("KOTLIN_STDLIB").expect("explicit Kotlin standard library jar");
        let base = std::env::temp_dir().join(format!("jpa-envers-kotlin-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let text = run(KOTLIN_PATH, KOTLIN_ENTITY, "kotlin", &[("pom.xml", ENVERS_POM)], TITLE, "OrderRevisionRepository").unwrap().1;
        let (entity, repository) = (base.join("Order.kt"), base.join("OrderRevisionRepository.kt"));
        std::fs::write(&entity, "package shop\nclass Order\n").unwrap();
        std::fs::write(&repository, text).unwrap();
        let result = std::process::Command::new("java")
            .args(["-classpath", &compiler, "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler", "-no-stdlib", "-no-reflect", "-jvm-target", "17", "-classpath", &format!("{envers}:{stdlib}"), "-d"])
            .arg(&base)
            .args([&entity, &repository])
            .output()
            .expect("java installed");
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(base).unwrap();
    }
}
