//! Reviewed ModelMapper and Blaze-Persistence entity view helpers (Java only).
//!
//! Facts (modelmapper.org getting-started, configuration and validation pages; Spring Framework
//! `@Bean` reference; persistence.blazebit.com entity view manual and Javadoc): ModelMapper is
//! `org.modelmapper:modelmapper`, used as `new ModelMapper()` and `map(source, Destination.class)`;
//! `getConfiguration().setMatchingStrategy(MatchingStrategies.X)` with `STANDARD` (default), `LOOSE`
//! and `STRICT` from `org.modelmapper.convention`; default matching uses JavaBeans-style public
//! methods, so DTOs here are plain getter/setter classes (record support is not documented and
//! records are not offered). `@Bean`/`@Configuration` live in `org.springframework.context.annotation`.
//! Blaze entity views are interfaces `@EntityView(Entity.class)` with an `@IdMapping` getter, from
//! `com.blazebit.persistence.view`, shipped in `com.blazebit:blaze-persistence-entity-view-api`
//! (`-jakarta` variant for Jakarta). Everything else is lexical and nothing was compiled against the
//! real libraries.
use super::mapper_action as base;
use super::mapper_more_action as more;
use super::*;
use crate::model::{capitalize, decapitalize, has_word, import_edit, Edit};

pub(super) const CONFIG_TITLE: &str = "Create ModelMapper configuration…";
pub(super) const CONVERT_TITLE: &str = "Add ModelMapper conversion methods…";
pub(super) const VIEW_TITLE: &str = "Create Blaze entity view…";
const STRATEGIES: [(&str, &str); 3] = [
    ("STANDARD", "STANDARD: intelligently matches properties (the default)"),
    ("LOOSE", "LOOSE: only the last destination property, all its tokens required"),
    ("STRICT", "STRICT: source and destination tokens match precisely and in order"),
];
pub(super) const BLAZE_ARTIFACTS: [&str; 2] = ["blaze-persistence-entity-view-api", "blaze-persistence-entity-view-api-jakarta"];
const MAX_SCAN: u32 = 65;

pub(super) fn eligible_service(doc: &Document) -> bool {
    doc.language == "java" && super::injection_action::eligible(doc)
}

pub(super) fn eligible_view(doc: &Document) -> bool {
    doc.language == "java" && !base::uses_lombok(&doc.text) && dto_action::source(doc).is_some_and(|class| view_fields(&class).is_some())
}

fn declares(text: &str, group: &str, artifacts: &[&str]) -> bool {
    artifacts.iter().any(|artifact| {
        let needle = format!("{group}:{artifact}");
        text.match_indices(&needle).any(|(at, found)| {
            text[at + found.len()..].chars().next().is_none_or(|ch| !(ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.')))
        }) || (text.contains(&format!("<groupId>{group}</groupId>")) && text.contains(&format!("<artifactId>{artifact}</artifactId>")))
    })
}

pub(super) fn build_proof(project: &Project, root: &str, doc: &Document, group: &str, artifacts: &[&str], label: &str) -> Result<(), Error> {
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let gathered = base::build_text(project, root, directory)?;
    if !declares(&gathered, group, artifacts) {
        return Err(format!(
            "{label} is not provably on this project: {group}:{} must be written literally in a pom.xml/build.gradle(.kts) above this source (version catalogs and indirect declarations are not verified); nothing was generated.",
            artifacts[0]
        )
        .into());
    }
    Ok(())
}

pub(super) fn crlf(doc: &Document, text: String) -> String {
    if doc.text.contains("\r\n") { text.replace('\n', "\r\n") } else { text }
}

pub(super) fn java_only(doc: &Document) -> Result<(), Error> {
    if doc.language == "kotlin" {
        return Err("This action supports Java sources only; Kotlin output was not verified.".into());
    }
    Ok(())
}

pub(super) fn type_name(name: &str, reserved: &[&str]) -> bool {
    identifier(name) && !reserved.contains(&name)
}

fn render_config(package: &str, name: &str, strategy: &str) -> String {
    format!(
        "package {package};\n\nimport org.modelmapper.ModelMapper;\nimport org.modelmapper.convention.MatchingStrategies;\nimport org.springframework.context.annotation.Bean;\nimport org.springframework.context.annotation.Configuration;\n\n@Configuration\npublic class {name} {{\n\n    @Bean\n    public ModelMapper modelMapper() {{\n        ModelMapper modelMapper = new ModelMapper();\n        modelMapper.getConfiguration().setMatchingStrategy(MatchingStrategies.{strategy});\n        return modelMapper;\n    }}\n}}\n"
    )
}

fn existing_model_mapper_bean(project: &Project, root: &str) -> Result<bool, Error> {
    let deadline = Instant::now() + Duration::from_secs(2);
    for extension in ["java", "kt"] {
        for glob in ["Config", "Configuration", "ModelMapper"] {
            let files = project.find_files(&format!("**/*{glob}*.{extension}"), MAX_SCAN)?;
            if files.len() >= MAX_SCAN as usize {
                return Err("ModelMapper bean discovery is incomplete: too many matching files in this project.".into());
            }
            for file in files {
                if Instant::now() >= deadline {
                    return Err("ModelMapper bean discovery timed out.".into());
                }
                if path(&file).as_deref() != Some(file.as_str()) || !inside(root, &file) {
                    return Err(Error::Stale);
                }
                if file_stem(&file).contains("ModelMapper") {
                    return Ok(true);
                }
                let text = project.read_file(&file)?;
                let masked = mask(&text);
                if has_word(&masked, "ModelMapper") && masked.contains("@Bean") {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

pub(super) fn resolve_config(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    java_only(doc)?;
    super::injection_action::analyze(doc).map_err(Error::from)?;
    build_proof(project, &pending.root, doc, "org.modelmapper", &["modelmapper"], "ModelMapper")?;
    if existing_model_mapper_bean(project, &pending.root)? {
        return Err("A ModelMapper bean class already exists in this project; no configuration was generated.".into());
    }
    let items: Vec<_> = STRATEGIES.iter().map(|(key, label)| PickItem::new(key, label)).collect();
    let strategy = host::pick("Matching strategy", &items)?.ok_or(Error::Cancelled)?;
    if !STRATEGIES.iter().any(|(key, _)| *key == strategy) {
        return Err("Choose one of the offered matching strategies.".into());
    }
    let name = host::input("Configuration class name (same package)", Some("ModelMapperConfig"))?.ok_or(Error::Cancelled)?.trim().to_owned();
    if !type_name(&name, &["ModelMapper", "MatchingStrategies", "Bean", "Configuration"]) {
        return Err("Use a non-conflicting Java identifier for the configuration class.".into());
    }
    let declared = package(doc).ok_or(Error::Stale)?;
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let target = format!("{directory}/{name}.java");
    if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
        return Err(Error::Stale);
    }
    no_collision(project, &pending.root, directory, &name, None)?;
    let text = crlf(doc, render_config(&declared, &name, &strategy));
    if !ui_kit::prompts::confirm_exact(
        "Create ModelMapper configuration?",
        &format!(
            "Source: {}\nSource version: {}\nTarget: {target}\n\nSpring @Configuration exposing a ModelMapper @Bean; the strategy is written explicitly. Component scanning, other ModelMapper configuration and the library version are not checked.\n\nExact new file:\n{text}\nOne atomic host-managed Create; no existing file is overwritten.",
            doc.path, doc.version
        ),
    )? {
        return Err(Error::Cancelled);
    }
    let project = current(pending, doc)?;
    build_proof(&project, &pending.root, doc, "org.modelmapper", &["modelmapper"], "ModelMapper")?;
    if existing_model_mapper_bean(&project, &pending.root)? {
        return Err(Error::Stale);
    }
    no_collision(&project, &pending.root, directory, &name, None)?;
    Ok(FileAction {
        title: format!("Create ModelMapper configuration {name}"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target.clone(), text, overwrite: false }],
        open: Some(target),
        ..FileAction::default()
    })
}

struct Dto {
    path: String,
    name: String,
    package: String,
    text: String,
    shared: Vec<(String, String)>,
}

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn parse_dto(file: &str, text: &str, entity: &more::Candidate) -> Option<Dto> {
    if text.len() > MAX_TEXT || base::uses_lombok(text) {
        return None;
    }
    let doc = Document { path: file.into(), text: text.into(), language: "java".into(), version: 0 };
    let package = package(&doc)?;
    let masked = mask(text);
    let name = file_stem(file).to_owned();
    let at = words(&masked, "class").next()?;
    if words(&masked, "class").count() != 1
        || ["interface", "enum", "record"].iter().any(|word| has_word(&masked, word))
        || !has_word(&masked[..at], "public")
        || !masked[at + 5..].trim_start().strip_prefix(name.as_str())?.trim_start().starts_with('{')
        || name == entity.class.name
    {
        return None;
    }
    let flat = compact(&masked);
    let shared: Vec<_> = entity
        .scalars
        .iter()
        .filter(|(field, ty)| {
            let cap = capitalize(field);
            let getter = format!("public{ty}get{cap}(){{");
            let boolean_getter = format!("public{ty}is{cap}(){{");
            flat.contains(&format!("private{ty}{field};"))
                && (flat.contains(&getter) || (ty == "boolean" && flat.contains(&boolean_getter)))
                && flat.contains(&format!("publicvoidset{cap}({ty}{field}){{"))
        })
        .cloned()
        .collect();
    (!shared.is_empty()).then_some(Dto { path: file.into(), name, package, text: text.into(), shared })
}

fn dtos(project: &Project, root: &str, entity: &more::Candidate) -> Result<Vec<Dto>, Error> {
    let directory = entity.doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut found = Vec::new();
    for file in more::nearby(project, root, directory, false)? {
        let stem = file_stem(&file);
        if !(stem.ends_with("Dto") || stem.ends_with("DTO")) {
            continue;
        }
        if Instant::now() >= deadline {
            return Err("DTO discovery timed out.".into());
        }
        let text = project.read_file(&file)?;
        if let Some(dto) = parse_dto(&file, &text, entity) {
            found.push(dto);
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found.truncate(base::MAX_CANDIDATES);
    Ok(found)
}

fn render_methods(entity: &str, dto: &str) -> Result<String, Error> {
    let (e, d) = (decapitalize(entity), decapitalize(dto));
    if !identifier(&e) || !identifier(&d) || e == d || e == "modelMapper" || d == "modelMapper" {
        return Err("The entity or DTO name cannot be used as a parameter name here.".into());
    }
    Ok(format!(
        "    public {dto} toDto({entity} {e}) {{\n        return modelMapper.map({e}, {dto}.class);\n    }}\n\n    public {entity} toEntity({dto} {d}) {{\n        return modelMapper.map({d}, {entity}.class);\n    }}\n"
    ))
}

fn edit_span(doc: &Document, edit: &TextEdit) -> Option<(usize, usize)> {
    Some((offset(&doc.text, edit.range.start)?, offset(&doc.text, edit.range.end)?))
}

fn only_imports(text: &str) -> bool {
    !text.trim().is_empty() && text.lines().all(|line| line.trim().starts_with("import "))
}

fn insert_merged(doc: &Document, edits: &mut Vec<TextEdit>, extra: TextEdit) -> Result<(), Error> {
    let span = edit_span(doc, &extra).ok_or(Error::Stale)?;
    let Some(existing) = edits.iter_mut().find(|edit| edit_span(doc, edit) == Some(span)) else {
        edits.push(extra);
        return Ok(());
    };
    if only_imports(&existing.text) && only_imports(&extra.text) {
        let mut lines: Vec<_> = existing.text.lines().chain(extra.text.lines()).map(str::trim).collect();
        lines.sort_unstable();
        lines.dedup();
        let eol = if doc.text.contains("\r\n") { "\r\n" } else { "\n" };
        existing.text = lines.iter().map(|line| format!("{line}{eol}")).collect();
    } else {
        existing.text.push_str(&extra.text);
    }
    Ok(())
}

fn plan_conversion(doc: &Document, entity: &more::Candidate, dto: &Dto, style: super::injection_action::Style, lombok: bool) -> Result<Vec<TextEdit>, Error> {
    let src = super::injection_action::analyze(doc).map_err(Error::from)?;
    let masked = mask(&doc.text);
    if ["toDto", "toEntity", "modelMapper"].iter().any(|word| has_word(&masked, word)) {
        return Err("This class already uses the name toDto, toEntity or modelMapper; nothing is changed.".into());
    }
    let injected = super::injection_action::Repo { name: "ModelMapper".into(), entity: String::new(), package: "org.modelmapper".into() };
    let mut edits = super::injection_action::plan(doc, &src, &injected, style, lombok).map_err(Error::from)?;

    let own = package(doc).ok_or(Error::Stale)?;
    let entity_package = package(&entity.doc).ok_or(Error::Stale)?;
    let mut wanted = Vec::new();
    for (simple, package) in [(&entity.class.name, &entity_package), (&dto.name, &dto.package)] {
        let qualified = format!("{package}.{simple}");
        if doc.text.contains(&format!("import {qualified};")) || *package == own {
            continue;
        }
        if base::imports_of(&masked).is_some_and(|imports| imports.iter().any(|import| import.rsplit('.').next() == Some(simple.as_str()))) {
            return Err(format!("`{simple}` already names a different type in this file; nothing is changed.").into());
        }
        wanted.push(qualified);
    }
    let refs: Vec<&str> = wanted.iter().map(String::as_str).collect();
    if let Some(edit) = import_edit(&doc.text, &refs, false) {
        let mut import = edit.to_text_edit(&doc.text);
        import.text = crlf(doc, import.text);
        insert_merged(doc, &mut edits, import)?;
    }

    let close = masked.rfind('}').ok_or(Error::Stale)?;
    let line_start = masked[..close].rfind('\n').map_or(0, |index| index + 1);
    if !masked[close + 1..].trim().is_empty() || !doc.text[line_start..close].trim().is_empty() {
        return Err("The class layout is not supported (code on the closing brace line).".into());
    }
    let collides = edits.iter().any(|edit| edit_span(doc, edit) == Some((line_start, line_start)));
    let lead = if collides || !masked[..line_start].trim_end().ends_with('{') { "\n" } else { "" };
    let methods = crlf(doc, format!("{lead}{}", render_methods(&entity.class.name, &dto.name)?));
    insert_merged(doc, &mut edits, Edit::insert(line_start, &methods).to_text_edit(&doc.text))?;
    Ok(edits)
}

pub(super) fn resolve_conversion(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    java_only(doc)?;
    super::injection_action::analyze(doc).map_err(Error::from)?;
    let style = super::injection_action::style()?;
    build_proof(project, &pending.root, doc, "org.modelmapper", &["modelmapper"], "ModelMapper")?;
    let candidates = more::entities(project, &pending.root, doc, false)?;
    if candidates.is_empty() {
        return Err("No @Entity class with bounded scalar fields (no Lombok) was found in this folder or a sibling folder.".into());
    }
    let items: Vec<_> = candidates.iter().map(|c| PickItem::new(&c.doc.path, &c.class.name).description(c.class.package.as_deref().unwrap_or_default())).collect();
    let picked = host::pick("Entity to convert", &items)?.ok_or(Error::Cancelled)?;
    let entity = candidates.into_iter().find(|c| c.doc.path == picked).ok_or("Choose one of the offered entities.")?;
    let found = dtos(project, &pending.root, &entity)?;
    if found.is_empty() {
        return Err("No public Java class named *Dto/*DTO next to the entity has private fields with public getters and setters matching entity scalar fields (records, Lombok and inheritance are not supported).".into());
    }
    let items: Vec<_> = found.iter().map(|d| PickItem::new(&d.path, &d.name).description(&format!("{} - {} shared scalar fields", d.package, d.shared.len()))).collect();
    let picked = host::pick("Existing DTO", &items)?.ok_or(Error::Cancelled)?;
    let dto = found.iter().find(|d| d.path == picked).ok_or("Choose one of the offered DTOs.")?;
    let shared_names: Vec<String> = dto.shared.iter().map(|(name, _)| name.clone()).collect();
    base::accessors(&entity.doc, &entity.class, &entity.scalars, &shared_names, true)?;
    let lombok = style == super::injection_action::Style::Lombok && super::injection_action::lombok_provable(doc, project, &pending.root)?;
    let project = current(pending, doc)?;
    if project.read_file(&entity.doc.path)? != entity.doc.text || project.read_file(&dto.path)? != dto.text {
        return Err(Error::Stale);
    }
    let edits = plan_conversion(doc, &entity, dto, style, lombok)?;
    let after = patched(&doc.text, &edits)?;
    if after.len() > MAX_TEXT {
        return Err("Generated source exceeds the supported bound.".into());
    }
    if !ui_kit::prompts::confirm_exact(
        "Add ModelMapper conversion methods?",
        &format!(
            "Target: {}\nSource version: {}\nStyle: {style:?} (setting injectionStyle)\nEntity: {}\nDTO: {} ({} shared scalar properties by name and type: {})\n\n{after}\nOnly this source is edited. Whether a ModelMapper @Bean exists, the matching strategy and nested properties are not checked; the ModelMapper API usage was not compiled here.",
            doc.path, doc.version, entity.doc.path, dto.path, dto.shared.len(), shared_names.join(", ")
        ),
    )? {
        return Err(Error::Cancelled);
    }
    let project = current(pending, doc)?;
    build_proof(&project, &pending.root, doc, "org.modelmapper", &["modelmapper"], "ModelMapper")?;
    if project.read_file(&entity.doc.path)? != entity.doc.text || project.read_file(&dto.path)? != dto.text {
        return Err(Error::Stale);
    }
    Ok(FileAction {
        title: "Add ModelMapper conversion methods".into(),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), edits)],
        ..FileAction::default()
    })
}

pub(super) fn view_fields(class: &Class) -> Option<((String, String), Vec<(String, String)>)> {
    if class.kotlin {
        return None;
    }
    let mut ids = class.attributes.iter().filter(|attribute| attribute.id);
    let id = ids.next()?;
    if ids.next().is_some() {
        return None;
    }
    let offered = projection_action::offered(class, projection_action::Form::Interface);
    let id = offered.iter().find(|(name, _)| *name == id.name)?.clone();
    let rest = offered.into_iter().filter(|(name, ty)| *name != id.0 && ty != "boolean").collect();
    Some((id, rest))
}

fn render_view(class: &Class, package_name: &str, name: &str, id: &(String, String), fields: &[(String, String)]) -> String {
    let mut imports: Vec<String> = std::iter::once(id).chain(fields).filter_map(|(_, ty)| class.qualified(ty.split('<').next().unwrap_or(ty))).collect();
    imports.push("com.blazebit.persistence.view.EntityView".into());
    imports.push("com.blazebit.persistence.view.IdMapping".into());
    imports.sort();
    imports.dedup();
    imports.retain(|import| import.rsplit_once('.').map(|(pk, _)| pk) != Some(package_name));
    let imports: String = imports.iter().map(|import| format!("import {import};\n")).collect();
    let getters: String = fields.iter().map(|(n, t)| format!("\n    {t} {}();\n", projection_action::getter(n, t))).collect();
    format!(
        "package {package_name};\n\n{imports}\n@EntityView({entity}.class)\npublic interface {name} {{\n\n    @IdMapping\n    {id_type} {id_getter}();\n{getters}}}\n",
        entity = class.name,
        id_type = id.1,
        id_getter = projection_action::getter(&id.0, &id.1),
    )
}

pub(super) fn resolve_view(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    java_only(doc)?;
    let class = dto_action::source(doc).ok_or(Error::Stale)?;
    let (id, available) = view_fields(&class).ok_or("The entity needs exactly one plain scalar @Id attribute with a bean-safe name.")?;
    build_proof(project, &pending.root, doc, "com.blazebit", &BLAZE_ARTIFACTS, "Blaze-Persistence entity views")?;
    let default = format!("{}View", class.name);
    let name = host::input("Entity view interface name (same package)", Some(&default))?.ok_or(Error::Cancelled)?.trim().to_owned();
    if !type_name(&name, &["EntityView", "IdMapping", "Mapping"])
        || name == class.name
        || class.imports.iter().any(|import| import.rsplit('.').next() == Some(name.as_str()))
        || class.attributes.iter().any(|attribute| attribute.ty.rsplit('.').next() == Some(name.as_str()))
    {
        return Err("Use a non-conflicting Java identifier for the entity view.".into());
    }
    current(pending, doc)?;
    let items: Vec<_> = available.iter().map(|(n, t)| PickItem::new(n, n).description(t)).collect();
    let chosen = host::pick_many("Entity view attributes besides the id (plain scalars; primitive boolean is not offered)", &items)?.ok_or(Error::Cancelled)?;
    let mut seen = HashSet::new();
    if chosen.iter().any(|n| !available.iter().any(|(f, _)| f == n) || !seen.insert(n.as_str())) {
        return Err("Choose only offered attributes, each once.".into());
    }
    let fields: Vec<_> = available.iter().filter(|(n, _)| chosen.contains(n)).cloned().collect();
    let declared = package(doc).ok_or(Error::Stale)?;
    let text = render_view(&class, &declared, &name, &id, &fields);
    if text.lines().filter_map(|line| line.strip_prefix("import ")).any(|import| import.trim_end_matches(';').rsplit('.').next() == Some(name.as_str())) {
        return Err("The view name conflicts with an imported type.".into());
    }
    let text = crlf(doc, text);
    if text.len() > MAX_TEXT {
        return Err("Entity view output exceeds the supported bound.".into());
    }
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let target = format!("{directory}/{name}.java");
    if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
        return Err(Error::Stale);
    }
    no_collision(project, &pending.root, directory, &name, None)?;
    if !ui_kit::prompts::confirm_exact(
        "Create Blaze entity view?",
        &format!(
            "Source: {}\nSource version: {}\nTarget: {target}\n\nInterface @EntityView({}.class) with an @IdMapping getter; attributes map to same-named entity attributes. No subviews, @Mapping paths, EntityViewSetting or repository integration are generated, and the interface was not compiled against Blaze-Persistence.\n\nExact new file:\n{text}\nOne atomic host-managed Create; no existing file is overwritten.",
            doc.path, doc.version, class.name
        ),
    )? {
        return Err(Error::Cancelled);
    }
    let project = current(pending, doc)?;
    build_proof(&project, &pending.root, doc, "com.blazebit", &BLAZE_ARTIFACTS, "Blaze-Persistence entity views")?;
    no_collision(&project, &pending.root, directory, &name, None)?;
    Ok(FileAction {
        title: format!("Create entity view {name}"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target.clone(), text, overwrite: false }],
        open: Some(target),
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const POM: &str = "<project><dependencies><dependency><groupId>org.modelmapper</groupId><artifactId>modelmapper</artifactId><version>3.2.4</version></dependency></dependencies></project>";
    const BLAZE_POM: &str = "<project><dependencies><dependency><groupId>com.blazebit</groupId><artifactId>blaze-persistence-entity-view-api-jakarta</artifactId></dependency></dependencies></project>";
    const ENTITY: &str = "package shop;\nimport jakarta.persistence.*;\nimport java.time.LocalDate;\n@Entity\npublic class Customer {\n @Id Long id;\n String title;\n boolean active;\n LocalDate joined;\n @ManyToOne Other parent;\n public Long getId() { return id; }\n public void setId(Long id) { this.id = id; }\n public String getTitle() { return title; }\n public void setTitle(String title) { this.title = title; }\n}\n";
    const DTO: &str = "package shop;\n\npublic class CustomerDto {\n    private Long id;\n    private String title;\n\n    public Long getId() { return id; }\n    public void setId(Long id) { this.id = id; }\n    public String getTitle() { return title; }\n    public void setTitle(String title) { this.title = title; }\n}\n";
    const SERVICE: &str = "package shop;\n\nimport org.springframework.stereotype.Service;\n\n@Service\npublic class OrderService {\n}\n";
    const SERVICE_PATH: &str = "/project/src/main/java/shop/OrderService.java";
    const ENTITY_PATH: &str = "/project/src/main/java/shop/Customer.java";
    const DTO_PATH: &str = "/project/src/main/java/shop/CustomerDto.java";

    fn setup(path: &str, text: &str, files: &[(&str, &str)]) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(path, "java", text, &[]);
        for (name, body) in files {
            host.add_file(name, body);
        }
        (host, JpaAssist::default(), Document { path: path.into(), text: text.into(), language: "java".into(), version: 1 })
    }

    fn pending(plugin: &mut JpaAssist, doc: &Document, title: &str) -> FileAction {
        let action = plugin.file_actions(doc, Range::default()).into_iter().find(|a| a.title == title).unwrap();
        assert!(action.operations.is_empty() && action.edits.is_empty());
        action
    }

    fn config(service: &str, files: &[(&str, &str)], strategy: &str, name: &str) -> Result<(String, String), Error> {
        let (host, mut plugin, doc) = setup(SERVICE_PATH, service, files);
        let action = pending(&mut plugin, &doc, CONFIG_TITLE);
        host.answer_next_prompt(Some(&[strategy]));
        host.answer_next_prompt(Some(&[name]));
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&doc, action)?;
        let [FileOperation::Create { path, text, overwrite: false }] = result.operations.as_slice() else { panic!("one non-overwriting create") };
        assert!(result.edits.is_empty());
        Ok((path.clone(), text.clone()))
    }

    fn convert(service: &str, path: &str, files: &[(&str, &str)], setting: Option<&str>) -> Result<String, Error> {
        let (host, mut plugin, doc) = setup(path, service, files);
        if let Some(style) = setting {
            host.set_setting("injectionStyle", &format!("\"{style}\""));
        }
        let action = pending(&mut plugin, &doc, CONVERT_TITLE);
        host.answer_next_prompt(Some(&[ENTITY_PATH]));
        host.answer_next_prompt(Some(&[DTO_PATH]));
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&doc, action)?;
        let [(edited, edits)] = result.edits.as_slice() else { panic!("one edited document") };
        assert_eq!(edited, path);
        assert!(result.operations.is_empty());
        patched(&doc.text, edits)
    }

    fn view(entity: &str, files: &[(&str, &str)], name: &str, fields: &[&str]) -> Result<String, Error> {
        let (host, mut plugin, doc) = setup(ENTITY_PATH, entity, files);
        let action = pending(&mut plugin, &doc, VIEW_TITLE);
        host.answer_next_prompt(Some(&[name]));
        host.answer_next_prompt(Some(fields));
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&doc, action)?;
        let [FileOperation::Create { path, text, overwrite: false }] = result.operations.as_slice() else { panic!("one non-overwriting create") };
        assert_eq!(path, &format!("/project/src/main/java/shop/{name}.java"));
        Ok(text.clone())
    }

    const FILES: [(&str, &str); 3] = [("pom.xml", POM), ("src/main/java/shop/Customer.java", ENTITY), ("src/main/java/shop/CustomerDto.java", DTO)];

    #[test]
    fn configuration_is_an_exact_spring_bean_with_the_chosen_strategy() {
        let (path, text) = config(SERVICE, &[("pom.xml", POM)], "STRICT", "ModelMapperConfig").unwrap();
        assert_eq!(path, "/project/src/main/java/shop/ModelMapperConfig.java");
        assert_eq!(
            text,
            "package shop;\n\nimport org.modelmapper.ModelMapper;\nimport org.modelmapper.convention.MatchingStrategies;\nimport org.springframework.context.annotation.Bean;\nimport org.springframework.context.annotation.Configuration;\n\n@Configuration\npublic class ModelMapperConfig {\n\n    @Bean\n    public ModelMapper modelMapper() {\n        ModelMapper modelMapper = new ModelMapper();\n        modelMapper.getConfiguration().setMatchingStrategy(MatchingStrategies.STRICT);\n        return modelMapper;\n    }\n}\n"
        );
        for strategy in ["STANDARD", "LOOSE"] {
            assert!(config(SERVICE, &[("pom.xml", POM)], strategy, "AppMapperConfig").unwrap().1.contains(&format!("MatchingStrategies.{strategy});")));
        }
        let crlf_text = config(&SERVICE.replace('\n', "\r\n"), &[("build.gradle", "dependencies { implementation 'org.modelmapper:modelmapper:3.2.4' }")], "LOOSE", "C").unwrap().1;
        assert!(crlf_text.contains("\r\n") && !crlf_text.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn configuration_is_refused_without_a_literal_dependency_or_with_an_existing_bean_or_bad_names() {
        for files in [
            vec![],
            vec![("pom.xml", "<project><groupId>org.modelmapper</groupId></project>")],
            vec![("build.gradle", "implementation 'org.modelmapper:modelmapper-spring:1'")],
            vec![("build.gradle", "implementation libs.modelmapper")],
        ] {
            assert!(config(SERVICE, &files, "STRICT", "ModelMapperConfig").is_err());
        }
        let existing = "package x;\nimport org.modelmapper.ModelMapper;\n@Configuration\nclass Beans {\n @Bean ModelMapper mm() { return new ModelMapper(); }\n}\n";
        assert!(config(SERVICE, &[("pom.xml", POM), ("src/main/java/x/Beans.java", existing), ("src/main/java/x/AppConfig.java", existing)], "STRICT", "ModelMapperConfig").is_err());
        assert!(config(SERVICE, &[("pom.xml", POM), ("src/main/java/x/ModelMapperProvider.java", "class P {}")], "STRICT", "ModelMapperConfig").is_err());
        assert!(config(SERVICE, &[("pom.xml", POM), ("src/main/java/shop/Taken.java", "class Taken {}")], "STRICT", "Taken").is_err());
        for name in ["", "class", "ModelMapper", "Bean", "9x", "A B"] {
            assert!(config(SERVICE, &[("pom.xml", POM)], "STRICT", name).is_err(), "{name}");
        }
        assert!(config(SERVICE, &[("pom.xml", POM)], "FUZZY", "ModelMapperConfig").is_err());
    }

    #[test]
    fn configuration_cancellation_and_drift_change_nothing() {
        let (host, mut plugin, doc) = setup(SERVICE_PATH, SERVICE, &[("pom.xml", POM)]);
        let action = pending(&mut plugin, &doc, CONFIG_TITLE);
        host.answer_next_prompt(Some(&["LOOSE"]));
        host.answer_next_prompt(Some(&["ModelMapperConfig"]));
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, action), Err(Error::Cancelled)));
        let action = pending(&mut plugin, &doc, CONFIG_TITLE);
        host.type_in_editor("// changed");
        assert!(matches!(plugin.resolve_file_action(&doc, action), Err(Error::Stale)));
        let plain = SERVICE.replace("@Service\n", "");
        let (_host, mut plugin, doc) = setup(SERVICE_PATH, &plain, &[("pom.xml", POM)]);
        assert!(!plugin.file_actions(&doc, Range::default()).iter().any(|a| a.title == CONFIG_TITLE || a.title == CONVERT_TITLE));
    }

    #[test]
    fn conversion_injects_the_mapper_and_appends_both_methods() {
        let text = convert(SERVICE, SERVICE_PATH, &FILES, None).unwrap();
        assert_eq!(
            text,
            "package shop;\n\nimport org.springframework.stereotype.Service;\nimport org.modelmapper.ModelMapper;\n\n@Service\npublic class OrderService {\n    private final ModelMapper modelMapper;\n\n    public OrderService(ModelMapper modelMapper) {\n        this.modelMapper = modelMapper;\n    }\n\n    public CustomerDto toDto(Customer customer) {\n        return modelMapper.map(customer, CustomerDto.class);\n    }\n\n    public Customer toEntity(CustomerDto customerDto) {\n        return modelMapper.map(customerDto, Customer.class);\n    }\n}\n"
        );
        let field = convert(SERVICE, SERVICE_PATH, &FILES, Some("field-autowired")).unwrap();
        assert!(field.contains("@Autowired\n    private ModelMapper modelMapper;\n\n    public CustomerDto toDto("), "{field}");
    }

    #[test]
    fn conversion_imports_types_from_other_packages_and_keeps_existing_members() {
        let service = "package other;\n\nimport org.springframework.stereotype.Service;\n\n@Service\npublic class OrderService {\n    private int count;\n\n    public int count() {\n        return count;\n    }\n}\n";
        let path = "/project/src/main/java/other/OrderService.java";
        let text = convert(service, path, &FILES, Some("field-autowired")).unwrap();
        for expected in [
            "import org.springframework.stereotype.Service;\nimport org.modelmapper.ModelMapper;\nimport org.springframework.beans.factory.annotation.Autowired;\nimport shop.Customer;\nimport shop.CustomerDto;\n",
            "    public int count() {\n        return count;\n    }\n\n    public CustomerDto toDto(Customer customer) {",
        ] {
            assert!(text.contains(expected), "{expected}\n{text}");
        }
        let crlf = convert(&service.replace('\n', "\r\n"), path, &FILES, None).unwrap();
        assert!(!crlf.replace("\r\n", "").contains('\n'), "{crlf:?}");
    }

    #[test]
    fn conversion_refusals_leave_the_source_alone() {
        let without_pom = [("src/main/java/shop/Customer.java", ENTITY), ("src/main/java/shop/CustomerDto.java", DTO)];
        assert!(convert(SERVICE, SERVICE_PATH, &without_pom, None).is_err());
        let with = |dto: &str, entity: &str| vec![("pom.xml", POM), ("src/main/java/shop/Customer.java", entity.to_owned().leak() as &str), ("src/main/java/shop/CustomerDto.java", dto.to_owned().leak() as &str)];
        let record = "package shop;\npublic record CustomerDto(Long id, String title) {}\n";
        assert!(convert(SERVICE, SERVICE_PATH, &with(record, ENTITY), None).is_err());
        let no_setter = DTO.replace("    public void setId(Long id) { this.id = id; }\n", "").replace("    public void setTitle(String title) { this.title = title; }\n", "");
        assert!(convert(SERVICE, SERVICE_PATH, &with(&no_setter, ENTITY), None).is_err());
        let lombok = DTO.replace("package shop;\n", "package shop;\nimport lombok.Data;\n");
        assert!(convert(SERVICE, SERVICE_PATH, &with(&lombok, ENTITY), None).is_err());
        let inherited = DTO.replace("CustomerDto {", "CustomerDto extends Base {");
        assert!(convert(SERVICE, SERVICE_PATH, &with(&inherited, ENTITY), None).is_err());
        let entity_without_setters = ENTITY.replace(" public void setId(Long id) { this.id = id; }\n", "").replace(" public void setTitle(String title) { this.title = title; }\n", "");
        assert!(convert(SERVICE, SERVICE_PATH, &with(DTO, &entity_without_setters), None).is_err());
        for taken in [SERVICE.replace("{\n}", "{\n    Object toDto() { return null; }\n}"), SERVICE.replace("{\n}", "{\n    private ModelMapper modelMapper;\n}")] {
            assert!(convert(&taken, SERVICE_PATH, &FILES, None).is_err());
        }
        let clash = SERVICE.replace("import org.springframework", "import other.Customer;\nimport org.springframework");
        let moved = "/project/src/main/java/other/OrderService.java";
        assert!(convert(&clash.replace("package shop;", "package other;"), moved, &FILES, None).is_err());
        let kotlin = Document { path: "/project/src/main/kotlin/shop/OrderService.kt".into(), language: "kotlin".into(), text: "package shop\n".into(), version: 1 };
        assert!(!eligible_service(&kotlin));
    }

    #[test]
    fn conversion_cancellation_and_drift_change_nothing() {
        let (host, mut plugin, doc) = setup(SERVICE_PATH, SERVICE, &FILES);
        let action = pending(&mut plugin, &doc, CONVERT_TITLE);
        host.answer_next_prompt(Some(&[ENTITY_PATH]));
        host.answer_next_prompt(Some(&[DTO_PATH]));
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, action), Err(Error::Cancelled)));
        assert_eq!(host.editor_text(), SERVICE);
        let action = pending(&mut plugin, &doc, CONVERT_TITLE);
        host.type_in_editor("// changed");
        assert!(matches!(plugin.resolve_file_action(&doc, action), Err(Error::Stale)));
    }

    #[test]
    fn view_is_an_exact_interface_with_an_id_mapping() {
        let files = [("pom.xml", BLAZE_POM)];
        let text = view(ENTITY, &files, "CustomerView", &["title", "joined"]).unwrap();
        assert_eq!(
            text,
            "package shop;\n\nimport com.blazebit.persistence.view.EntityView;\nimport com.blazebit.persistence.view.IdMapping;\nimport java.time.LocalDate;\n\n@EntityView(Customer.class)\npublic interface CustomerView {\n\n    @IdMapping\n    Long getId();\n\n    String getTitle();\n\n    LocalDate getJoined();\n}\n"
        );
        let only_id = view(ENTITY, &files, "CustomerIdView", &[]).unwrap();
        assert!(only_id.contains("@IdMapping\n    Long getId();\n}\n"), "{only_id}");
        let gradle = [("build.gradle.kts", "implementation(\"com.blazebit:blaze-persistence-entity-view-api:1.6.20\")")];
        assert!(view(ENTITY, &gradle, "CustomerView", &["title"]).is_ok());
        let crlf = view(&ENTITY.replace('\n', "\r\n"), &files, "CustomerView", &["title"]).unwrap();
        assert!(!crlf.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn view_refusals() {
        let files = [("pom.xml", BLAZE_POM)];
        for bad in [vec![], vec![("pom.xml", POM)], vec![("pom.xml", "<groupId>com.blazebit</groupId>")], vec![("build.gradle", "implementation 'com.blazebit:blaze-persistence-core-impl'")]] {
            assert!(view(ENTITY, &bad, "CustomerView", &["title"]).is_err());
        }
        for (name, fields) in [("Customer", &["title"][..]), ("EntityView", &["title"]), ("", &["title"]), ("CustomerView", &["parent"]), ("CustomerView", &["active"]), ("CustomerView", &["id"]), ("CustomerView", &["title", "title"])] {
            assert!(view(ENTITY, &files, name, fields).is_err(), "{name} {fields:?}");
        }
        assert!(view(ENTITY, &[("pom.xml", BLAZE_POM), ("src/main/java/shop/CustomerView.kt", "class CustomerView")], "CustomerView", &["title"]).is_err());
        let no_id = ENTITY.replace(" @Id Long id;\n", " Long id;\n");
        let (_host, mut plugin, doc) = setup(ENTITY_PATH, &no_id, &files);
        assert!(!plugin.file_actions(&doc, Range::default()).iter().any(|a| a.title == VIEW_TITLE));
        let two = ENTITY.replace(" String title;", " @Id String title;");
        let (_host, mut plugin, doc) = setup(ENTITY_PATH, &two, &files);
        assert!(!plugin.file_actions(&doc, Range::default()).iter().any(|a| a.title == VIEW_TITLE));
        let (host, mut plugin, doc) = setup(ENTITY_PATH, ENTITY, &files);
        let action = pending(&mut plugin, &doc, VIEW_TITLE);
        host.answer_next_prompt(Some(&["CustomerView"]));
        host.answer_next_prompt(Some(&["title"]));
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, action), Err(Error::Cancelled)));
    }

    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_API; compiles against hand-written API stubs, not the real ModelMapper/Blaze/Spring jars (none in ~/.m2)"]
    fn generated_sources_compile_against_api_stubs() {
        let api = std::env::var("JPA_JAKARTA_API").expect("explicit API jar path");
        let (_, config_text) = config(SERVICE, &[("pom.xml", POM)], "STRICT", "ModelMapperConfig").unwrap();
        let service = convert(&SERVICE.replace("package shop;", "package other;"), "/project/src/main/java/other/OrderService.java", &FILES, None).unwrap();
        let entity = ENTITY.replace("@ManyToOne Other parent;", "String parent;");
        let view_text = view(&entity, &[("pom.xml", BLAZE_POM)], "CustomerView", &["title", "joined"]).unwrap();
        let files: Vec<(&str, String)> = vec![
            ("shop/Customer.java", entity),
            ("shop/CustomerDto.java", DTO.to_owned()),
            ("shop/ModelMapperConfig.java", config_text),
            ("shop/CustomerView.java", view_text),
            ("other/OrderService.java", service),
            ("org/modelmapper/ModelMapper.java", "package org.modelmapper; public class ModelMapper { public <D> D map(Object s, Class<D> d) { return null; } public org.modelmapper.config.Configuration getConfiguration() { return null; } }".into()),
            ("org/modelmapper/config/Configuration.java", "package org.modelmapper.config; public interface Configuration { Configuration setMatchingStrategy(org.modelmapper.spi.MatchingStrategy s); }".into()),
            ("org/modelmapper/spi/MatchingStrategy.java", "package org.modelmapper.spi; public interface MatchingStrategy {}".into()),
            ("org/modelmapper/convention/MatchingStrategies.java", "package org.modelmapper.convention; public class MatchingStrategies { public static final org.modelmapper.spi.MatchingStrategy STANDARD = null, LOOSE = null, STRICT = null; }".into()),
            ("org/springframework/context/annotation/Bean.java", "package org.springframework.context.annotation; public @interface Bean {}".into()),
            ("org/springframework/context/annotation/Configuration.java", "package org.springframework.context.annotation; public @interface Configuration {}".into()),
            ("org/springframework/stereotype/Service.java", "package org.springframework.stereotype; public @interface Service {}".into()),
            ("com/blazebit/persistence/view/EntityView.java", "package com.blazebit.persistence.view; public @interface EntityView { Class<?> value(); }".into()),
            ("com/blazebit/persistence/view/IdMapping.java", "package com.blazebit.persistence.view; public @interface IdMapping {}".into()),
        ];
        let dir = std::env::temp_dir().join(format!("jpa-modelmapper-{}", std::process::id()));
        let paths: Vec<_> = files
            .iter()
            .map(|(file, body)| {
                let path = dir.join(file);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, body).unwrap();
                path
            })
            .collect();
        let result = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &api, "-d"]).arg(&dir).args(&paths).output().expect("javac installed");
        assert!(result.status.success(), "{}\n{files:?}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
