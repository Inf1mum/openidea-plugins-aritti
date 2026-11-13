//! Reviewed creation of a MapStruct mapper interface for an entity and an existing DTO.
//! One new sibling file (`overwrite: false`); no existing file is touched. Facts are lexical:
//! the DTO is a Java record or Kotlin data class found in the entity's folder or a sibling folder,
//! and only scalar fields with identical names and types on both sides are mapped. MapStruct must
//! be declared literally in a build file between the source folder and the project root.
use super::*;

pub(super) const TITLE: &str = "Create MapStruct mapper with preview…";
pub(super) const MAX_FIELDS: usize = 32;
pub(super) const MAX_CANDIDATES: usize = 16;
const COMPONENT_MODELS: [&str; 3] = ["default", "spring", "jakarta-cdi"];
const EXTEND_BASE: &str = "extendBase";
const OPTIONS: [(&str, &str); 4] = [
    ("collections", "List variants of the generated mapping methods (names follow the Mapper Declaration settings)"),
    ("toEntity", "toEntity(dto): DTO to new entity"),
    ("updateEntity", "updateEntity(dto, @MappingTarget entity): update an existing entity"),
    ("ignoreUnmapped", "unmappedTargetPolicy = ReportingPolicy.IGNORE"),
];

pub(super) struct Dto {
    pub(super) path: String,
    pub(super) name: String,
    pub(super) package: String,
    pub(super) kotlin: bool,
    pub(super) imports: Vec<String>,
    pub(super) fields: Vec<(String, String)>,
    pub(super) text: String,
}

pub(super) fn uses_lombok(text: &str) -> bool {
    text.lines().any(|line| line.trim_start().starts_with("import ") && line.contains("lombok"))
}

pub(super) fn eligible(doc: &Document) -> bool {
    !uses_lombok(&doc.text) && dto_action::source(doc).is_some_and(|class| !scalars(&class).is_empty())
}

pub(super) fn scalars(class: &Class) -> Vec<(String, String)> {
    let mut seen = HashSet::new();
    if class.attributes.iter().any(|attribute| !seen.insert(attribute.name.as_str())) {
        return Vec::new();
    }
    class
        .attributes
        .iter()
        .filter(|attribute| {
            attribute.relation == Relation::None && identifier(&attribute.name) && dto_action::supported(class, &attribute.ty)
        })
        .map(|attribute| (attribute.name.clone(), attribute.ty.clone()))
        .take(MAX_FIELDS + 1)
        .collect()
}

fn strip(text: &str, open: &str, close: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        out.push_str(&rest[..start]);
        match rest[start..].find(close) {
            Some(end) => rest = &rest[start + end + close.len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

fn declares(text: &str, artifact: &str) -> bool {
    let needle = format!("org.mapstruct:{artifact}");
    let gradle = text.match_indices(&needle).any(|(at, found)| {
        text[at + found.len()..].chars().next().is_none_or(|ch| !(ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.')))
    });
    gradle || (text.contains("<groupId>org.mapstruct</groupId>") && text.contains(&format!("<artifactId>{artifact}</artifactId>")))
}

pub(crate) fn build_text(project: &Project, root: &str, directory: &str) -> Result<String, Error> {
    let mut gathered = String::new();
    let mut dir = directory.to_owned();
    for _ in 0..24 {
        if !dir.starts_with(root) {
            break;
        }
        for name in BUILD_FILES {
            match project.read_file(&format!("{dir}/{name}")) {
                Ok(text) if text.len() <= MAX_TEXT => {
                    let text = strip(&strip(&text, "<!--", "-->"), "/*", "*/");
                    gathered.push_str(&text.lines().filter(|line| !line.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n"));
                    gathered.push('\n');
                }
                Ok(_) | Err(Error::NotFound(_)) | Err(Error::Invalid(_)) => {}
                Err(error) => return Err(error),
            }
        }
        if dir == root {
            break;
        }
        let Some((parent, _)) = dir.rsplit_once('/') else {
            break;
        };
        dir = parent.to_owned();
    }
    Ok(gathered)
}

pub(super) fn build_proof(project: &Project, root: &str, directory: &str, kotlin: bool) -> Result<(), Error> {
    let gathered = build_text(project, root, directory)?;
    if gathered.trim().is_empty() {
        return Err("No pom.xml, build.gradle or build.gradle.kts could be read above this source, so MapStruct cannot be proven; no mapper was generated.".into());
    }
    if !declares(&gathered, "mapstruct") || !declares(&gathered, "mapstruct-processor") {
        return Err("MapStruct is not provably on this project: both org.mapstruct:mapstruct and org.mapstruct:mapstruct-processor must be written literally in a pom.xml/build.gradle(.kts) above this source (version catalogs and indirect declarations are not verified); no mapper was generated.".into());
    }
    if kotlin && !gathered.contains("kapt") {
        return Err("A Kotlin mapper needs the MapStruct processor on kapt (documented in the mapstruct-examples Kotlin sample); no kapt was found in the build files, so no mapper was generated.".into());
    }
    Ok(())
}

pub(super) fn imports_of(masked: &str) -> Option<Vec<String>> {
    let mut found = Vec::new();
    for line in masked.lines() {
        let Some(rest) = line.trim().strip_prefix("import ") else {
            continue;
        };
        let rest = rest.trim().trim_end_matches(';').trim();
        if rest.contains(" as ") || rest.starts_with("static ") || rest.contains(';') {
            return None;
        }
        found.push(rest.to_owned());
    }
    Some(found)
}

fn split_top(text: &str) -> Vec<&str> {
    let mut depth = 0i32;
    let mut parts = Vec::new();
    let mut start = 0;
    for (at, ch) in text.char_indices() {
        match ch {
            '<' | '(' | '[' | '{' => depth += 1,
            '>' | ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&text[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

fn parse_dto(path: &str, text: &str, kotlin: bool) -> Option<Dto> {
    if text.len() > MAX_TEXT {
        return None;
    }
    let doc = Document { path: path.into(), text: text.into(), language: if kotlin { "kotlin" } else { "java" }.into(), version: 0 };
    let package = package(&doc)?;
    let masked = mask(text);
    let imports = imports_of(&masked)?;
    if uses_lombok(text) {
        return None;
    }
    let (keyword, forbidden): (&str, &[&str]) =
        if kotlin { ("class", &["interface", "object", "enum"]) } else { ("record", &["class", "interface", "enum"]) };
    if words(&masked, keyword).count() != 1 || forbidden.iter().any(|word| has_word(&masked, word)) {
        return None;
    }
    let at = words(&masked, keyword).next()?;
    if kotlin && !masked[..at].trim_end().ends_with("data") {
        return None;
    }
    let rest = masked[at + keyword.len()..].trim_start();
    let name: String = rest.chars().take_while(|ch| ch.is_alphanumeric() || *ch == '_').collect();
    let after = &rest[name.len()..];
    if !identifier(&name) || !after.starts_with('(') || file_stem(path) != name {
        return None;
    }
    let mut depth = 0;
    let close = after.char_indices().find_map(|(index, ch)| {
        depth += i32::from(ch == '(') - i32::from(ch == ')');
        (depth == 0).then_some(index)
    })?;
    let tail = after[close + 1..].trim_start();
    let tail_ok = if kotlin { tail.is_empty() || tail.starts_with('{') } else { tail.starts_with('{') || tail.starts_with("implements") };
    if !tail_ok {
        return None;
    }
    let inner = &after[1..close];
    let mut fields = Vec::new();
    for part in split_top(inner) {
        let cleaned = crate::model::without_annotations(part, 0, part.len());
        let cleaned = cleaned.trim();
        if cleaned.is_empty() {
            continue;
        }
        let (field, ty) = if kotlin {
            let declaration = cleaned.split('=').next()?.trim();
            let declaration = declaration.strip_prefix("val ").or_else(|| declaration.strip_prefix("var "))?;
            let (field, ty) = declaration.split_once(':')?;
            (field.trim().to_owned(), ty.trim().trim_end_matches('?').trim().to_owned())
        } else {
            if cleaned.contains("...") {
                return None;
            }
            let (ty, field) = cleaned.rsplit_once(char::is_whitespace)?;
            (field.to_owned(), ty.trim().trim_start_matches("final ").trim().to_owned())
        };
        if !identifier(&field) || ty.is_empty() || fields.iter().any(|(known, _)| known == &field) {
            return None;
        }
        fields.push((field, ty));
    }
    (!fields.is_empty() && fields.len() <= 128).then_some(Dto { path: path.into(), name, package, kotlin, imports, fields, text: text.into() })
}

pub(super) fn common(entity: &Class, scalars: &[(String, String)], dto: &Dto) -> Vec<(String, String)> {
    let mut view = entity.clone();
    view.kotlin = dto.kotlin;
    view.imports = dto.imports.clone();
    scalars
        .iter()
        .filter(|(name, ty)| dto.fields.iter().any(|(field, dto_ty)| field == name && dto_ty == ty) && dto_action::supported(&view, ty))
        .cloned()
        .collect()
}

pub(super) fn discover(project: &Project, root: &str, directory: &str, doc: &Document, entity: &Class, scalars: &[(String, String)]) -> Result<Vec<Dto>, Error> {
    let relative = directory.strip_prefix(&format!("{}/", root.trim_end_matches('/'))).ok_or(Error::Stale)?;
    let extension = if entity.kotlin { "kt" } else { "java" };
    let mut globs = vec![format!("{relative}/*.{extension}")];
    if let Some((parent, _)) = relative.rsplit_once('/') {
        globs.push(format!("{parent}/*/*.{extension}"));
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut found: Vec<Dto> = Vec::new();
    for glob in globs {
        let files = project.find_files(&glob, 65)?;
        if files.len() >= 65 {
            return Err("DTO discovery is incomplete in this folder; no mapper was generated.".into());
        }
        for file in files.into_iter().filter(|file| file != &doc.path) {
            if Instant::now() >= deadline {
                return Err("DTO discovery timed out.".into());
            }
            if path(&file).as_deref() != Some(file.as_str()) || !inside(root, &file) {
                return Err(Error::Stale);
            }
            let stem = file_stem(&file);
            if !(stem.ends_with("Dto") || stem.ends_with("DTO")) || found.iter().any(|dto| dto.path == file) {
                continue;
            }
            let text = project.read_file(&file)?;
            let Some(dto) = parse_dto(&file, &text, entity.kotlin) else {
                continue;
            };
            if dto.name != entity.name && !common(entity, scalars, &dto).is_empty() {
                found.push(dto);
            }
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found.truncate(MAX_CANDIDATES);
    Ok(found)
}

fn declared_before(masked: &str, at: usize) -> bool {
    let head = &masked[..at];
    let line = &head[head.rfind(['\n', ';', '{', '}', ',', '(']).map_or(0, |position| position + 1)..];
    has_word(line, "private") || has_word(line, "protected")
}

fn java_method(masked: &str, name: &str) -> bool {
    masked.match_indices(&format!("{name}(")).any(|(at, _)| {
        let head = masked[..at].trim_end();
        !masked[..at].chars().next_back().is_some_and(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '$' | '.'))
            && head.chars().next_back().is_some_and(|ch| ch.is_alphanumeric() || matches!(ch, '>' | ']'))
            && !declared_before(masked, at)
    })
}

fn kotlin_property(masked: &str, name: &str, writable: bool) -> bool {
    let keywords: &[&str] = if writable { &["var"] } else { &["var", "val"] };
    keywords.iter().any(|keyword| {
        words(masked, keyword).any(|at| {
            masked[at + keyword.len()..]
                .trim_start()
                .strip_prefix(name)
                .is_some_and(|rest| rest.chars().next().is_none_or(|ch| !ch.is_alphanumeric() && ch != '_'))
                && !declared_before(masked, at)
        })
    })
}

pub(super) fn accessors(doc: &Document, class: &Class, scalars: &[(String, String)], chosen: &[String], writes: bool) -> Result<(), Error> {
    let masked = mask(&doc.text);
    for name in chosen {
        let ty = &scalars.iter().find(|(field, _)| field == name).ok_or(Error::Stale)?.1;
        let ok = if class.kotlin {
            kotlin_property(&masked, name, writes)
        } else {
            let cap = capitalize(name);
            (java_method(&masked, &format!("get{cap}")) || (ty == "boolean" && java_method(&masked, &format!("is{cap}"))))
                && (!writes || java_method(&masked, &format!("set{cap}")))
        };
        if !ok {
            return Err(format!("Entity field {name} has no visible {}; MapStruct needs accessors, so no mapper was generated.", if writes { "getter and setter (or Kotlin var)" } else { "getter (or Kotlin property)" }).into());
        }
    }
    Ok(())
}

struct Spec<'a> {
    kotlin: bool,
    package: &'a str,
    entity: &'a str,
    dto: &'a str,
    dto_package: Option<&'a str>,
    mapper: &'a str,
    component: &'a str,
    chosen: &'a [String],
    dto_fields: Vec<&'a str>,
    entity_attributes: Vec<&'a str>,
    to_entity: bool,
    update: bool,
    ignore_unmapped: bool,
    error_unmapped: bool,
    names: &'a mapper_naming::Names,
    collections: bool,
    base: Option<(&'a str, Option<&'a str>)>,
}

fn configured_component() -> Result<Option<String>, Error> {
    match host::setting_string("mapperComponentModel") {
        None => Ok(None),
        Some(model) if COMPONENT_MODELS.contains(&model.as_str()) => Ok(Some(model)),
        Some(other) => Err(format!("Unknown mapperComponentModel `{other}`; use default, spring or jakarta-cdi.").into()),
    }
}

fn configured_policy() -> Result<Option<&'static str>, Error> {
    match host::setting_string("mapperUnmappedTargetPolicy").as_deref() {
        None => Ok(None),
        Some("IGNORE") => Ok(Some("IGNORE")),
        Some("WARN") => Ok(Some("WARN")),
        Some("ERROR") => Ok(Some("ERROR")),
        Some(other) => Err(format!("Unknown mapperUnmappedTargetPolicy `{other}`; use IGNORE, WARN or ERROR (ReportingPolicy).").into()),
    }
}

fn render(spec: &Spec) -> String {
    let semi = if spec.kotlin { "" } else { ";" };
    let chosen = |name: &str| spec.chosen.iter().any(|field| field == name);
    let ignores = |names: &[&str], other: &[&str]| -> Vec<String> {
        names.iter().filter(|name| !chosen(name) && other.contains(name)).map(|name| (*name).to_owned()).collect()
    };
    let to_dto_ignores = ignores(&spec.dto_fields, &spec.entity_attributes);
    let to_entity_ignores = ignores(&spec.entity_attributes, &spec.dto_fields);
    let mapping = |names: &[String]| -> String {
        if names.is_empty() {
            return String::new();
        }
        let each: Vec<String> = names.iter().map(|name| format!("Mapping(target = \"{name}\", ignore = true)")).collect();
        if spec.kotlin {
            return format!("    @Mappings({})\n", each.join(", "));
        }
        names.iter().map(|name| format!("    @Mapping(target = \"{name}\", ignore = true)\n")).collect()
    };
    let mut imports = vec!["org.mapstruct.Mapper".to_owned()];
    if !to_dto_ignores.is_empty() || ((spec.to_entity || spec.update) && !to_entity_ignores.is_empty()) {
        imports.push("org.mapstruct.Mapping".to_owned());
        if spec.kotlin {
            imports.push("org.mapstruct.Mappings".to_owned());
        }
    }
    if spec.update {
        imports.push("org.mapstruct.MappingTarget".to_owned());
    }
    if spec.collections && !spec.kotlin {
        imports.push("java.util.List".to_owned());
    }
    if spec.ignore_unmapped || spec.error_unmapped {
        imports.push("org.mapstruct.ReportingPolicy".to_owned());
    }
    if let Some(dto_package) = spec.dto_package {
        imports.push(format!("{dto_package}.{}", spec.dto));
    }
    if let Some((base, Some(base_package))) = spec.base {
        imports.push(format!("{base_package}.{base}"));
    }
    imports.sort();
    let mut arguments = Vec::new();
    if spec.component != "default" {
        arguments.push(format!("componentModel = \"{}\"", spec.component));
    }
    if spec.ignore_unmapped {
        arguments.push("unmappedTargetPolicy = ReportingPolicy.IGNORE".to_owned());
    } else if spec.error_unmapped {
        arguments.push("unmappedTargetPolicy = ReportingPolicy.ERROR".to_owned());
    }
    let annotation = if arguments.is_empty() { "@Mapper".to_owned() } else { format!("@Mapper({})", arguments.join(", ")) };
    let (entity, dto) = (spec.entity, spec.dto);
    let mut methods = vec![format!(
        "{}{}",
        mapping(&to_dto_ignores),
        if spec.kotlin { format!("    fun {}(entity: {entity}): {dto}", spec.names.to_dto) } else { format!("    {dto} {}({entity} entity);", spec.names.to_dto) }
    )];
    if spec.to_entity {
        methods.push(format!(
            "{}{}",
            mapping(&to_entity_ignores),
            if spec.kotlin { format!("    fun {}(dto: {dto}): {entity}", spec.names.to_entity) } else { format!("    {entity} {}({dto} dto);", spec.names.to_entity) }
        ));
    }
    if spec.collections {
        methods.extend(spec.names.collection_methods(spec.kotlin, entity, dto, true, spec.to_entity));
    }
    if spec.update {
        methods.push(format!(
            "{}{}",
            mapping(&to_entity_ignores),
            if spec.kotlin {
                format!("    fun {}(dto: {dto}, @MappingTarget entity: {entity})", spec.names.update)
            } else {
                format!("    void {}({dto} dto, @MappingTarget {entity} entity);", spec.names.update)
            }
        ));
    }
    let header = if spec.kotlin { "interface" } else { "public interface" };
    let (parent, body) = match spec.base {
        Some((base, _)) => (if spec.kotlin { format!(" : {base}<{dto}, {entity}>") } else { format!(" extends {base}<{dto}, {entity}>") }, String::new()),
        None => (String::new(), format!("{}\n", methods.join("\n\n"))),
    };
    format!(
        "package {}{semi}\n\n{}\n\n{annotation}\n{header} {}{parent} {{\n{body}}}\n",
        spec.package,
        imports.iter().map(|import| format!("import {import}{semi}")).collect::<Vec<_>>().join("\n"),
        spec.mapper,
    )
}

pub(super) fn reread(project: &Project, dto: &Dto) -> Result<(), Error> {
    if project.read_file(&dto.path)? != dto.text {
        return Err(Error::Stale);
    }
    Ok(())
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let class = dto_action::source(doc).ok_or(Error::Stale)?;
    if uses_lombok(&doc.text) {
        return Err("The entity uses Lombok; MapStruct/Lombok processor ordering is not verified by this action, so no mapper was generated.".into());
    }
    let available = scalars(&class);
    if available.is_empty() || available.len() > MAX_FIELDS {
        return Err("No bounded unambiguous scalar entity fields are available for a mapper.".into());
    }
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    build_proof(project, &pending.root, directory, class.kotlin)?;
    let dtos = discover(project, &pending.root, directory, doc, &class, &available)?;
    if dtos.is_empty() {
        return Err("No Java record or Kotlin data class named *Dto/*DTO in this folder or a sibling folder shares scalar fields with identical names and types (nested, generic, inherited and Lombok DTOs are refused).".into());
    }
    let mut dtos = dtos;
    dtos.sort_by_key(|dto| dto_convert_action::compatibility(&available, &dto.fields).0);
    let items: Vec<_> = dtos
        .iter()
        .map(|dto| {
            let badge = dto_convert_action::compatibility(&available, &dto.fields).1;
            PickItem::new(&dto.path, &dto.name).description(&format!("[{badge}] {} - {} shared scalar fields", dto.package, common(&class, &available, dto).len()))
        })
        .collect();
    let Some(chosen_dto) = host::pick("Existing DTO to map", &items)? else {
        return Err(Error::Cancelled);
    };
    let dto = dtos.iter().find(|dto| dto.path == chosen_dto).ok_or("Choose one of the offered DTOs.")?;
    current(pending, doc)?;
    reread(project, dto)?;
    let shared = common(&class, &available, dto);
    let default = format!("{}Mapper", class.name);
    let Some(name) = host::input("Mapper type name (same package as the entity)", Some(&default))? else {
        return Err(Error::Cancelled);
    };
    let imported = |simple: &str| class.imports.iter().chain(dto.imports.iter()).any(|import| import.rsplit('.').next() == Some(simple));
    if !identifier(&name)
        || name == class.name
        || name == dto.name
        || ["Mapper", "Mapping", "Mappings", "MappingTarget", "ReportingPolicy"].contains(&name.as_str())
        || imported(&name)
        || (dto.package != class.package.clone().unwrap_or_default() && imported(&dto.name))
    {
        return Err("Use a non-conflicting Java/Kotlin identifier for the mapper (at most 128 bytes).".into());
    }
    current(pending, doc)?;
    let configured = configured_component()?;
    let mut ordered = COMPONENT_MODELS.to_vec();
    ordered.sort_by_key(|model| Some(*model) != configured.as_deref());
    let models: Vec<_> = ordered
        .iter()
        .map(|model| if Some(*model) == configured.as_deref() { PickItem::new(model, model).description("setting mapperComponentModel") } else { PickItem::new(model, model) })
        .collect();
    let Some(component) = host::pick("MapStruct componentModel", &models)? else {
        return Err(Error::Cancelled);
    };
    if !COMPONENT_MODELS.contains(&component.as_str()) {
        return Err("Choose one of the offered component models.".into());
    }
    current(pending, doc)?;
    let field_items: Vec<_> = shared.iter().map(|(field, ty)| PickItem::new(field, field).description(ty)).collect();
    let Some(chosen) = host::pick_many("Fields mapped explicitly (identical name and scalar type on entity and DTO)", &field_items)? else {
        return Err(Error::Cancelled);
    };
    let mut seen = HashSet::new();
    if chosen.is_empty() || chosen.iter().any(|field| !shared.iter().any(|(known, _)| known == field) || !seen.insert(field.as_str())) {
        return Err("Choose one or more offered shared fields exactly once.".into());
    }
    current(pending, doc)?;
    let bases = mapper_more_action::base_mappers(project, &pending.root, doc, class.kotlin)?;
    let mut option_items: Vec<_> = OPTIONS.iter().map(|(key, label)| PickItem::new(key, label)).collect();
    if !bases.is_empty() {
        option_items.push(PickItem::new(EXTEND_BASE, "Extend a generic base mapper (inherits its methods; none is declared here)"));
    }
    let Some(options) = host::pick_many("Extra mapper options (toDto is always generated)", &option_items)? else {
        return Err(Error::Cancelled);
    };
    let mut seen = HashSet::new();
    if options.iter().any(|option| !option_items.iter().any(|item| item.key == *option) || !seen.insert(option.as_str())) {
        return Err("Choose only offered options, each once.".into());
    }
    let has = |key: &str| options.iter().any(|option| option == key);
    let base = if has(EXTEND_BASE) {
        if has("toEntity") || has("updateEntity") || has("collections") {
            return Err("The generic base mapper already declares the entity/DTO, list and update methods; choose either the base or those methods.".into());
        }
        let picked = if bases.len() == 1 {
            0
        } else {
            let items: Vec<_> = bases.iter().enumerate().map(|(index, found)| PickItem::new(&index.to_string(), &found.name).description(&found.package)).collect();
            host::pick("Generic base mapper", &items)?.ok_or(Error::Cancelled)?.parse::<usize>().map_err(|_| Error::Stale)?
        };
        Some(bases.get(picked).ok_or("Choose one of the offered base mappers.")?)
    } else {
        None
    };
    let policy = configured_policy()?;
    let names = mapper_naming::Names::load()?;
    let ignore_unmapped = has("ignoreUnmapped") || policy == Some("IGNORE");
    current(pending, doc)?;
    reread(project, dto)?;
    accessors(doc, &class, &available, &chosen, has("toEntity") || has("updateEntity"))?;
    let declared = package(doc).ok_or(Error::Stale)?;
    let entity_attributes: Vec<_> = class.attributes.iter().map(|attribute| attribute.name.as_str()).collect();
    let spec = Spec {
        kotlin: class.kotlin,
        package: &declared,
        entity: &class.name,
        dto: &dto.name,
        dto_package: (dto.package != declared).then_some(dto.package.as_str()),
        mapper: &name,
        component: &component,
        chosen: &chosen,
        dto_fields: dto.fields.iter().map(|(field, _)| field.as_str()).collect(),
        entity_attributes,
        to_entity: has("toEntity"),
        update: has("updateEntity"),
        ignore_unmapped,
        error_unmapped: policy == Some("ERROR") && !ignore_unmapped,
        names: &names,
        collections: has("collections"),
        base: base.map(|found| (found.name.as_str(), (found.package != declared).then_some(found.package.as_str()))),
    };
    let mut generated = render(&spec);
    if doc.text.contains("\r\n") {
        generated = generated.replace('\n', "\r\n");
    }
    if generated.len() > MAX_TEXT {
        return Err("Mapper output exceeds the validated source bound.".into());
    }
    let target = format!("{directory}/{name}.{}", if class.kotlin { "kt" } else { "java" });
    if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
        return Err(Error::Stale);
    }
    no_collision(project, &pending.root, directory, &name, None)?;
    let note = if class.kotlin {
        "Kotlin interface mapper: requires MapStruct on kapt; Kotlin compile/processing was not verified here."
    } else {
        "Java interface mapper: MapStruct generates the implementation at compile time."
    };
    let base_note = base.map_or_else(String::new, |found| format!("Extends the generic base mapper {} ({}); the methods are inherited and none is declared. ", found.name, found.package));
    let preview = format!(
        "Source: {}\nSource version: {}\nDTO: {}\nTarget: {target}\n\n{note}\nDTO fields not selected but present on both sides get explicit ignore mappings. {base_note}No existing mapper was searched or reused.\n\nExact new file:\n{generated}\nOne atomic host-managed Create; no existing file is overwritten.",
        doc.path, doc.version, dto.path
    );
    if !ui_kit::prompts::confirm_exact("Create MapStruct mapper?", &preview)? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    reread(project, dto)?;
    build_proof(project, &pending.root, directory, class.kotlin)?;
    no_collision(project, &pending.root, directory, &name, None)?;
    Ok(FileAction {
        title: format!("Create MapStruct mapper {name}"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target.clone(), text: generated, overwrite: false }],
        open: Some(target),
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ENTITY: &str = "package shop;\nimport jakarta.persistence.*;\nimport java.time.LocalDate;\n@Entity\npublic class Customer {\n @Id Long id;\n String title;\n LocalDate joined;\n @ManyToOne Other parent;\n public Long getId() { return id; }\n public void setId(Long id) { this.id = id; }\n public String getTitle() { return title; }\n public void setTitle(String title) { this.title = title; }\n public LocalDate getJoined() { return joined; }\n public void setJoined(LocalDate joined) { this.joined = joined; }\n}\n";
    const DTO: &str = "package shop;\nimport java.time.LocalDate;\npublic record CustomerDto(Long id, String title, LocalDate joined, String extra) {}\n";
    const POM: &str = "<project><dependencies><dependency><groupId>org.mapstruct</groupId><artifactId>mapstruct</artifactId></dependency></dependencies><build><annotationProcessorPaths><path><groupId>org.mapstruct</groupId><artifactId>mapstruct-processor</artifactId></path></annotationProcessorPaths></build></project>";
    const KOTLIN_ENTITY: &str = "package shop\nimport jakarta.persistence.*\n@Entity\nclass Customer(@Id var id: Long? = null, var title: String = \"\", var joined: String = \"\") {}\n";
    const KOTLIN_DTO: &str = "package shop\ndata class CustomerDto(val id: Long? = null, val title: String? = null, val joined: String? = null)\n";
    const GRADLE_KAPT: &str = "dependencies {\n implementation(\"org.mapstruct:mapstruct:1.6.3\")\n kapt(\"org.mapstruct:mapstruct-processor:1.6.3\")\n}\n";
    const ENTITY_PATH: &str = "/project/src/main/java/shop/Customer.java";
    const DTO_PATH: &str = "/project/src/main/java/shop/CustomerDto.java";

    fn setup(path: &str, source: &str, language: &str, build: Option<(&str, &str)>) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(path, language, source, &[]);
        if let Some((name, text)) = build {
            host.add_file(name, text);
        }
        let doc = Document { path: path.into(), text: source.into(), language: language.into(), version: 1 };
        (host, JpaAssist::default(), doc)
    }
    fn java() -> (MockHost, JpaAssist, Document) {
        let (host, plugin, doc) = setup(ENTITY_PATH, ENTITY, "java", Some(("pom.xml", POM)));
        host.add_file("src/main/java/shop/CustomerDto.java", DTO);
        (host, plugin, doc)
    }
    fn action(plugin: &mut JpaAssist, doc: &Document) -> FileAction {
        let action = plugin.file_actions(doc, Range::default()).into_iter().find(|action| action.title == TITLE).unwrap();
        assert!(action.operations.is_empty() && action.edits.is_empty());
        action
    }
    fn answer(host: &MockHost, dto: &str, name: &str, model: &str, fields: &[&str], options: &[&str], confirm: bool) {
        host.answer_next_prompt(Some(&[dto]));
        host.answer_next_prompt(Some(&[name]));
        host.answer_next_prompt(Some(&[model]));
        host.answer_next_prompt(Some(fields));
        host.answer_next_prompt(Some(options));
        host.answer_next_prompt(confirm.then_some(&[]));
    }
    fn created(action: &FileAction) -> (&str, &str) {
        let [FileOperation::Create { path, text, overwrite: false }] = action.operations.as_slice() else {
            panic!("one non-overwriting create")
        };
        assert!(action.edits.is_empty());
        (path, text)
    }
    fn refused(host: &MockHost, plugin: &mut JpaAssist, doc: &Document, dto: &str, name: &str, fields: &[&str], options: &[&str]) {
        let pending = action(plugin, doc);
        answer(host, dto, name, "default", fields, options, true);
        assert!(plugin.resolve_file_action(doc, pending).is_err());
        assert_eq!(host.editor_text(), doc.text);
    }

    #[test]
    fn java_same_package_mapper_is_exact_with_ignores_for_unselected_shared_fields() {
        let (host, mut plugin, doc) = java();
        let pending = action(&mut plugin, &doc);
        answer(&host, DTO_PATH, "CustomerMapper", "spring", &["id", "title"], &["toEntity", "updateEntity", "ignoreUnmapped"], true);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let (path, text) = created(&resolved);
        assert_eq!(path, "/project/src/main/java/shop/CustomerMapper.java");
        assert_eq!(resolved.open.as_deref(), Some(path));
        let expected = "package shop;\n\nimport org.mapstruct.Mapper;\nimport org.mapstruct.Mapping;\nimport org.mapstruct.MappingTarget;\nimport org.mapstruct.ReportingPolicy;\n\n@Mapper(componentModel = \"spring\", unmappedTargetPolicy = ReportingPolicy.IGNORE)\npublic interface CustomerMapper {\n    @Mapping(target = \"joined\", ignore = true)\n    CustomerDto toDto(Customer entity);\n\n    @Mapping(target = \"joined\", ignore = true)\n    Customer toEntity(CustomerDto dto);\n\n    @Mapping(target = \"joined\", ignore = true)\n    void updateEntity(CustomerDto dto, @MappingTarget Customer entity);\n}\n";
        assert_eq!(text, expected);
        assert_eq!(host.editor_text(), ENTITY);
        assert!(host.requests().is_empty());
    }

    #[test]
    fn minimal_default_component_model_has_bare_mapper_and_only_to_dto() {
        let (host, mut plugin, doc) = java();
        let pending = action(&mut plugin, &doc);
        answer(&host, DTO_PATH, "CustomerMapper", "default", &["id", "title", "joined"], &[], true);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let (_, text) = created(&resolved);
        assert_eq!(text, "package shop;\n\nimport org.mapstruct.Mapper;\n\n@Mapper\npublic interface CustomerMapper {\n    CustomerDto toDto(Customer entity);\n}\n");
    }

    #[test]
    fn sibling_package_dto_is_imported_and_jakarta_cdi_is_offered() {
        let (host, mut plugin, doc) = java();
        host.add_file("src/main/java/dto/CustomerDto.java", &DTO.replace("package shop;", "package dto;"));
        host.add_file("src/main/java/shop/CustomerDto.java", "package shop;\npublic class Unrelated {}");
        let pending = action(&mut plugin, &doc);
        answer(&host, "/project/src/main/java/dto/CustomerDto.java", "CustomerMapper", "jakarta-cdi", &["id"], &[], true);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let (_, text) = created(&resolved);
        assert!(text.contains("import dto.CustomerDto;\n") && text.contains("@Mapper(componentModel = \"jakarta-cdi\")"), "{text}");
    }

    #[test]
    fn kotlin_mapper_uses_interface_funs_and_mappings_container_for_ignores() {
        let (host, mut plugin, doc) = setup("/project/src/main/kotlin/shop/Customer.kt", KOTLIN_ENTITY, "kotlin", Some(("build.gradle.kts", GRADLE_KAPT)));
        host.add_file("src/main/kotlin/shop/CustomerDto.kt", KOTLIN_DTO);
        let pending = action(&mut plugin, &doc);
        answer(&host, "/project/src/main/kotlin/shop/CustomerDto.kt", "CustomerMapper", "default", &["id", "title"], &["toEntity", "updateEntity"], true);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let (path, text) = created(&resolved);
        assert!(path.ends_with("/CustomerMapper.kt"));
        let expected = "package shop\n\nimport org.mapstruct.Mapper\nimport org.mapstruct.Mapping\nimport org.mapstruct.MappingTarget\nimport org.mapstruct.Mappings\n\n@Mapper\ninterface CustomerMapper {\n    @Mappings(Mapping(target = \"joined\", ignore = true))\n    fun toDto(entity: Customer): CustomerDto\n\n    @Mappings(Mapping(target = \"joined\", ignore = true))\n    fun toEntity(dto: CustomerDto): Customer\n\n    @Mappings(Mapping(target = \"joined\", ignore = true))\n    fun updateEntity(dto: CustomerDto, @MappingTarget entity: Customer)\n}\n";
        assert_eq!(text, expected);
    }

    #[test]
    fn build_proof_refuses_unprovable_or_incomplete_mapstruct_setups() {
        let cases: [(Option<(&str, &str)>, &str); 5] = [
            (None, ENTITY_PATH),
            (Some(("pom.xml", "<project/>")), ENTITY_PATH),
            (Some(("pom.xml", &POM.replace("<artifactId>mapstruct-processor</artifactId>", ""))), ENTITY_PATH),
            (Some(("build.gradle", "dependencies { implementation libs.mapstruct\n annotationProcessor libs.mapstruct.processor }")), ENTITY_PATH),
            (Some(("pom.xml", &format!("<!-- {POM} -->"))), ENTITY_PATH),
        ];
        for (build, path) in cases {
            let (host, mut plugin, doc) = setup(path, ENTITY, "java", build);
            host.add_file("src/main/java/shop/CustomerDto.java", DTO);
            refused(&host, &mut plugin, &doc, DTO_PATH, "CustomerMapper", &["id"], &[]);
        }
        let (host, mut plugin, doc) = setup("/project/src/main/kotlin/shop/Customer.kt", KOTLIN_ENTITY, "kotlin", Some(("pom.xml", POM)));
        host.add_file("src/main/kotlin/shop/CustomerDto.kt", KOTLIN_DTO);
        refused(&host, &mut plugin, &doc, "/project/src/main/kotlin/shop/CustomerDto.kt", "CustomerMapper", &["id"], &[]);
    }

    #[test]
    fn module_build_file_above_the_source_folder_is_found() {
        let (host, mut plugin, doc) = setup(ENTITY_PATH, ENTITY, "java", Some(("pom.xml", POM)));
        host.add_file("src/main/java/shop/CustomerDto.java", DTO);
        host.add_file("src/pom.xml", "<project/>");
        let pending = action(&mut plugin, &doc);
        answer(&host, DTO_PATH, "CustomerMapper", "default", &["id"], &[], true);
        assert!(plugin.resolve_file_action(&doc, pending).is_ok());
    }

    #[test]
    fn unsupported_dto_shapes_and_mismatched_fields_yield_no_candidate() {
        let bad = [
            "package shop;\npublic class CustomerDto { private Long id; }",
            "package shop;\npublic record CustomerDto<T>(Long id) {}",
            "package shop;\npublic record CustomerDto(Integer id, Long title, java.time.LocalDate joined) {}",
            "package shop;\nimport lombok.Data;\npublic record CustomerDto(Long id) {}",
            "package shop;\npublic record CustomerDto(Long id) { record Inner(Long id) {} }",
            "package shop;\nimport java.util.List;\npublic record CustomerDto(List<Long> id) {}",
            "package wrong;\npublic record CustomerDto(Long id) {}",
        ];
        for text in bad {
            let (host, mut plugin, doc) = java();
            host.add_file("src/main/java/shop/CustomerDto.java", text);
            let pending = action(&mut plugin, &doc);
            assert!(plugin.resolve_file_action(&doc, pending).is_err(), "{text}");
        }
        let (host, mut plugin, doc) = setup("/project/src/main/kotlin/shop/Customer.kt", KOTLIN_ENTITY, "kotlin", Some(("build.gradle.kts", GRADLE_KAPT)));
        for text in [
            "package shop\ndata class CustomerDto(val id: Long? = null) : Base()\n",
            "package shop\nclass CustomerDto(val id: Long? = null)\n",
            "package shop\ndata class CustomerDto(id: Long? = null)\n",
        ] {
            host.add_file("src/main/kotlin/shop/CustomerDto.kt", text);
            let pending = action(&mut plugin, &doc);
            assert!(plugin.resolve_file_action(&doc, pending).is_err(), "{text}");
        }
    }

    #[test]
    fn language_mismatched_dto_and_unbounded_discovery_are_refused() {
        let (host, mut plugin, doc) = setup(ENTITY_PATH, ENTITY, "java", Some(("pom.xml", POM)));
        host.add_file("src/main/java/shop/CustomerDto.kt", KOTLIN_DTO);
        let pending = action(&mut plugin, &doc);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        let (host, mut plugin, doc) = java();
        for n in 0..65 {
            host.add_file(&format!("src/main/java/shop/Type{n}.java"), &format!("package shop; class Type{n} {{}}"));
        }
        let pending = action(&mut plugin, &doc);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
    }

    #[test]
    fn missing_accessors_lombok_and_unsupported_entities_have_no_output() {
        let no_setter = ENTITY.replace("public void setTitle(String title) { this.title = title; }\n", "");
        let (host, mut plugin, doc) = setup(ENTITY_PATH, &no_setter, "java", Some(("pom.xml", POM)));
        host.add_file("src/main/java/shop/CustomerDto.java", DTO);
        refused(&host, &mut plugin, &doc, DTO_PATH, "CustomerMapper", &["title"], &["toEntity"]);
        let (host, mut plugin, doc) = setup(ENTITY_PATH, &no_setter, "java", Some(("pom.xml", POM)));
        host.add_file("src/main/java/shop/CustomerDto.java", DTO);
        let pending = action(&mut plugin, &doc);
        answer(&host, DTO_PATH, "CustomerMapper", "default", &["title"], &[], true);
        assert!(plugin.resolve_file_action(&doc, pending).is_ok());
        let private_getter = ENTITY.replace("public Long getId()", "private Long getId()");
        let (host, mut plugin, doc) = setup(ENTITY_PATH, &private_getter, "java", Some(("pom.xml", POM)));
        host.add_file("src/main/java/shop/CustomerDto.java", DTO);
        refused(&host, &mut plugin, &doc, DTO_PATH, "CustomerMapper", &["id"], &[]);
        let (host, mut plugin, doc) = setup(ENTITY_PATH, &ENTITY.replace("import java.time", "import lombok.Data;\nimport java.time"), "java", Some(("pom.xml", POM)));
        host.add_file("src/main/java/shop/CustomerDto.java", DTO);
        assert!(!plugin.file_actions(&doc, Range::default()).iter().any(|action| action.title == TITLE));
        for source in [
            ENTITY.replace("class Customer", "class Customer<T>"),
            ENTITY.replace("class Customer", "class Customer extends Base"),
            "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Customer {\n @Id @ManyToOne Other id;\n}\n".to_owned(),
        ] {
            let (_host, mut plugin, doc) = setup(ENTITY_PATH, &source, "java", Some(("pom.xml", POM)));
            assert!(!plugin.file_actions(&doc, Range::default()).iter().any(|action| action.title == TITLE));
        }
        let kotlin_val = KOTLIN_ENTITY.replace("var title", "val title");
        let (host, mut plugin, doc) = setup("/project/src/main/kotlin/shop/Customer.kt", &kotlin_val, "kotlin", Some(("build.gradle.kts", GRADLE_KAPT)));
        host.add_file("src/main/kotlin/shop/CustomerDto.kt", KOTLIN_DTO);
        refused(&host, &mut plugin, &doc, "/project/src/main/kotlin/shop/CustomerDto.kt", "CustomerMapper", &["title"], &["updateEntity"]);
    }

    #[test]
    fn name_collisions_and_forged_selections_are_refused() {
        for name in ["../EscapeMapper", "Customer", "CustomerDto", "Mapper", "class", "X;", "wrong.Mapper"] {
            let (host, mut plugin, doc) = java();
            refused(&host, &mut plugin, &doc, DTO_PATH, name, &["id"], &[]);
        }
        for (extension, body) in [("java", "package shop; public class CustomerMapper {}"), ("kt", "package shop\nclass CustomerMapper")] {
            let (host, mut plugin, doc) = java();
            host.add_file(&format!("src/main/java/shop/CustomerMapper.{extension}"), body);
            refused(&host, &mut plugin, &doc, DTO_PATH, "CustomerMapper", &["id"], &[]);
        }
        let (host, mut plugin, doc) = java();
        host.add_file("src/main/java/shop/Other.java", "package shop; interface CustomerMapper {}");
        refused(&host, &mut plugin, &doc, DTO_PATH, "CustomerMapper", &["id"], &[]);
        for (dto, fields, options) in [
            ("/project/src/main/java/shop/Missing.java", &["id"][..], &[][..]),
            (DTO_PATH, &["extra"][..], &[][..]),
            (DTO_PATH, &["parent"][..], &[][..]),
            (DTO_PATH, &["id", "id"][..], &[][..]),
            (DTO_PATH, &[][..], &[][..]),
            (DTO_PATH, &["id"][..], &["bogus"][..]),
            (DTO_PATH, &["id"][..], &["toEntity", "toEntity"][..]),
        ] {
            let (host, mut plugin, doc) = java();
            refused(&host, &mut plugin, &doc, dto, "CustomerMapper", fields, options);
        }
        let (host, mut plugin, doc) = java();
        let pending = action(&mut plugin, &doc);
        answer(&host, DTO_PATH, "CustomerMapper", "cdi", &["id"], &[], true);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
    }

    #[test]
    fn every_cancellation_stage_and_stale_source_leave_files_untouched() {
        for stage in 0..6 {
            let (host, mut plugin, doc) = java();
            let pending = action(&mut plugin, &doc);
            let replies: [&[&str]; 5] = [&[DTO_PATH], &["CustomerMapper"], &["default"], &["id"], &[]];
            for reply in replies.iter().take(stage) {
                host.answer_next_prompt(Some(reply));
            }
            assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)), "stage {stage}");
            assert_eq!(host.editor_text(), ENTITY);
        }
        let (host, mut plugin, doc) = java();
        let pending = action(&mut plugin, &doc);
        let mut changed = doc.clone();
        changed.version += 1;
        assert!(matches!(plugin.resolve_file_action(&changed, pending.clone()), Err(Error::Stale)));
        host.type_in_editor("// changed");
        assert!(matches!(plugin.resolve_file_action(&doc, pending.clone()), Err(Error::Stale)));
        host.set_project_root(Some("/other"));
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
    }

    #[test]
    fn stale_dto_file_is_detected_by_the_fresh_reread() {
        let (host, _, _) = java();
        let project = host.invocation().project().unwrap();
        let dto = parse_dto(DTO_PATH, DTO, false).unwrap();
        reread(&project, &dto).unwrap();
        host.add_file("src/main/java/shop/CustomerDto.java", &DTO.replace("extra", "other"));
        assert!(matches!(reread(&project, &dto), Err(Error::Stale)));
    }

    #[test]
    #[ignore = "requires javac, JPA_JAKARTA_API, MAPSTRUCT_JAR and MAPSTRUCT_PROCESSOR_JAR (mapstruct 1.6.x) jar paths"]
    fn generated_java_mapper_compiles_and_processes_with_mapstruct() {
        let api = std::env::var("JPA_JAKARTA_API").expect("explicit API jar path");
        let jar = std::env::var("MAPSTRUCT_JAR").expect("mapstruct jar");
        let processor = std::env::var("MAPSTRUCT_PROCESSOR_JAR").expect("mapstruct-processor jar");
        let spec = |component: &str, to_entity, update, ignore_unmapped| {
            render(&Spec {
                kotlin: false,
                package: "shop",
                entity: "Customer",
                dto: "CustomerDto",
                dto_package: None,
                mapper: "CustomerMapper",
                component,
                chosen: &["id".to_owned(), "title".to_owned()],
                dto_fields: vec!["id", "title", "joined", "extra"],
                entity_attributes: vec!["id", "title", "joined", "parent"],
                to_entity,
                update,
                ignore_unmapped,
                error_unmapped: false,
                names: &mapper_naming::Names::standard(),
                collections: to_entity,
                base: None,
            })
        };
        let ent = ENTITY.replace("@ManyToOne Other parent;", "String parent;");
        for (component, to_entity, update, ignore) in [("default", false, false, false), ("spring", true, true, true), ("jakarta-cdi", true, true, false)] {
            let dir = std::env::temp_dir().join(format!("jpa-mapper-{}-{component}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let mapper = spec(component, to_entity, update, ignore);
            let mut files = vec![("Customer.java", ent.clone()), ("CustomerDto.java", DTO.to_owned()), ("CustomerMapper.java", mapper.clone())];
            files.push(("org/springframework/stereotype/Component.java", "package org.springframework.stereotype; public @interface Component {}".into()));
            files.push(("org/springframework/beans/factory/annotation/Autowired.java", "package org.springframework.beans.factory.annotation; public @interface Autowired {}".into()));
            files.push(("jakarta/inject/Inject.java", "package jakarta.inject; public @interface Inject {}".into()));
            files.push(("jakarta/enterprise/context/ApplicationScoped.java", "package jakarta.enterprise.context; public @interface ApplicationScoped {}".into()));
            let paths: Vec<_> = files.iter().map(|(name, body)| {
                let path = dir.join(name);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, body).unwrap();
                path
            }).collect();
            let classpath = format!("{api}:{jar}");
            let result = std::process::Command::new("javac")
                .args(["--release", "17", "-classpath", &classpath, "-processorpath", &processor, "-d"])
                .arg(&dir)
                .args(&paths)
                .output()
                .expect("javac installed");
            let stderr = String::from_utf8_lossy(&result.stderr);
            assert!(result.status.success(), "{component}: {stderr}\n{mapper}");
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn mapper_settings_default_the_policy_and_component_model_and_unset_is_unchanged() {
        let run = |settings: &[(&str, &str)], model: &str, options: &[&str]| {
            let (host, mut plugin, doc) = java();
            for (key, value) in settings {
                host.set_setting(key, value);
            }
            let pending = action(&mut plugin, &doc);
            answer(&host, DTO_PATH, "CustomerMapper", model, &["id", "title", "joined"], options, true);
            plugin.resolve_file_action(&doc, pending).map(|resolved| created(&resolved).1.to_owned())
        };
        let plain = run(&[], "default", &[]).unwrap();
        assert_eq!(plain, "package shop;\n\nimport org.mapstruct.Mapper;\n\n@Mapper\npublic interface CustomerMapper {\n    CustomerDto toDto(Customer entity);\n}\n");
        assert_eq!(run(&[("mapperUnmappedTargetPolicy", "\"WARN\"")], "default", &[]).unwrap(), plain);
        let ignored = run(&[("mapperUnmappedTargetPolicy", "\"IGNORE\"")], "default", &[]).unwrap();
        assert!(ignored.contains("import org.mapstruct.ReportingPolicy;") && ignored.contains("@Mapper(unmappedTargetPolicy = ReportingPolicy.IGNORE)"), "{ignored}");
        let error = run(&[("mapperUnmappedTargetPolicy", "\"ERROR\"")], "spring", &[]).unwrap();
        assert!(error.contains("@Mapper(componentModel = \"spring\", unmappedTargetPolicy = ReportingPolicy.ERROR)"), "{error}");
        let explicit_wins = run(&[("mapperUnmappedTargetPolicy", "\"ERROR\"")], "default", &["ignoreUnmapped"]).unwrap();
        assert!(explicit_wins.contains("ReportingPolicy.IGNORE") && !explicit_wins.contains("ReportingPolicy.ERROR"), "{explicit_wins}");
        assert!(run(&[("mapperUnmappedTargetPolicy", "\"FAIL\"")], "default", &[]).is_err());
        assert!(run(&[("mapperComponentModel", "\"cdi\"")], "default", &[]).is_err());
        assert!(run(&[("mapperComponentModel", "\"spring\"")], "default", &[]).is_ok());
    }

    #[test]
    fn crlf_entities_produce_crlf_only_mappers_in_java_and_kotlin() {
        let (host, mut plugin, doc) = setup(ENTITY_PATH, &ENTITY.replace('\n', "\r\n"), "java", Some(("pom.xml", POM)));
        host.add_file("src/main/java/shop/CustomerDto.java", DTO);
        let pending = action(&mut plugin, &doc);
        answer(&host, DTO_PATH, "CustomerMapper", "spring", &["id", "title"], &["toEntity", "updateEntity"], true);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let text = created(&resolved).1;
        assert!(text.contains("\r\n") && !text.replace("\r\n", "").contains('\n'), "{text:?}");

        let kotlin_path = "/project/src/main/kotlin/shop/Customer.kt";
        let (host, mut plugin, doc) = setup(kotlin_path, &KOTLIN_ENTITY.replace('\n', "\r\n"), "kotlin", Some(("build.gradle.kts", GRADLE_KAPT)));
        host.add_file("src/main/kotlin/shop/CustomerDto.kt", KOTLIN_DTO);
        let pending = action(&mut plugin, &doc);
        answer(&host, "/project/src/main/kotlin/shop/CustomerDto.kt", "CustomerMapper", "default", &["id", "title"], &["toEntity", "updateEntity"], true);
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let text = created(&resolved).1;
        assert!(text.contains("\r\n") && !text.replace("\r\n", "").contains('\n'), "{text:?}");
    }

    #[test]
    #[ignore = "requires Kotlin compiler (KOTLIN_COMPILER_CLASSPATH, KOTLIN_STDLIB), JPA_JAKARTA_API and MAPSTRUCT_JAR; annotations only, kapt is not run"]
    fn generated_kotlin_mapper_compiles_against_mapstruct_annotations() {
        let api = std::env::var("JPA_JAKARTA_API").expect("explicit API jar path");
        let jar = std::env::var("MAPSTRUCT_JAR").expect("mapstruct jar");
        let compiler = std::env::var("KOTLIN_COMPILER_CLASSPATH").expect("explicit Kotlin compiler classpath");
        let stdlib = std::env::var("KOTLIN_STDLIB").expect("explicit Kotlin standard library jar");
        let dir = std::env::temp_dir().join(format!("jpa-kotlin-mapper-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (component, to_entity, update, ignore_unmapped) in [("default", false, false, false), ("spring", true, true, true)] {
            let mapper = render(&Spec {
                kotlin: true,
                package: "shop",
                entity: "Customer",
                dto: "CustomerDto",
                dto_package: None,
                mapper: "CustomerMapper",
                component,
                chosen: &["id".to_owned(), "title".to_owned()],
                dto_fields: vec!["id", "title", "joined", "extra"],
                entity_attributes: vec!["id", "title", "joined", "parent"],
                to_entity,
                update,
                ignore_unmapped,
                error_unmapped: false,
                names: &mapper_naming::Names::standard(),
                collections: to_entity,
                base: None,
            });
            let files: Vec<_> = [("Customer.kt", KOTLIN_ENTITY.to_owned()), ("CustomerDto.kt", KOTLIN_DTO.to_owned()), ("CustomerMapper.kt", mapper.clone())]
                .iter()
                .map(|(name, body)| {
                    let path = dir.join(format!("{component}-{name}"));
                    std::fs::write(&path, body).unwrap();
                    path
                })
                .collect();
            let out = dir.join(component);
            let result = std::process::Command::new("java")
                .args(["-classpath", &compiler, "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler", "-no-stdlib", "-no-reflect", "-jvm-target", "17", "-classpath", &format!("{api}:{jar}:{stdlib}"), "-d"])
                .arg(&out)
                .args(&files)
                .output()
                .expect("java installed");
            assert!(result.status.success(), "{component}: {}\n{mapper}", String::from_utf8_lossy(&result.stderr));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn generated(settings: &[(&str, &str)], options: &[&str], files: &[(&str, String)]) -> Result<String, Error> {
        let (host, mut plugin, doc) = java();
        for (key, value) in settings {
            host.set_setting(key, value);
        }
        for (name, text) in files {
            host.add_file(name, text);
        }
        let pending = action(&mut plugin, &doc);
        answer(&host, DTO_PATH, "CustomerMapper", "default", &["id", "title", "joined"], options, true);
        plugin.resolve_file_action(&doc, pending).map(|resolved| created(&resolved).1.to_owned())
    }

    #[test]
    fn collection_option_writes_list_methods_named_by_the_mapper_declaration_settings() {
        let plural = generated(&[], &["toEntity", "collections"], &[]).unwrap();
        for line in ["    List<CustomerDto> toDtos(List<Customer> entities);", "    List<Customer> toEntities(List<CustomerDto> dtos);", "import java.util.List;"] {
            assert!(plural.contains(line), "{plural}");
        }
        let same = generated(&[("mapperCollectionNaming", "\"same\"")], &["collections"], &[]).unwrap();
        assert!(same.contains("    List<CustomerDto> toDto(List<Customer> entity);"), "{same}");
        let typed = generated(&[("mapperCollectionNaming", "\"collection-type\"")], &["collections"], &[]).unwrap();
        assert!(typed.contains("    List<CustomerDto> toDtoList(List<Customer> entity);"), "{typed}");
        let renamed = generated(&[("mapperToDtoPattern", "\"asModel\""), ("mapperToEntityPattern", "\"asEntity\""), ("mapperUpdatePattern", "\"patch\"")], &["toEntity", "updateEntity", "collections"], &[]).unwrap();
        for line in ["CustomerDto asModel(Customer entity);", "Customer asEntity(CustomerDto dto);", "void patch(CustomerDto dto, @MappingTarget Customer entity);", "List<CustomerDto> asModels(List<Customer> entities);", "List<Customer> asEntities(List<CustomerDto> dtos);"] {
            assert!(renamed.contains(line), "{renamed}");
        }
        assert!(generated(&[("mapperToDtoPattern", "\"not valid\"")], &[], &[]).is_err());
    }

    #[test]
    fn extending_a_generic_base_mapper_declares_no_methods_and_excludes_the_method_options() {
        let base = ("src/main/java/shop/EntityMapper.java", mapper_more_action::render_base(false, "shop", "EntityMapper"));
        let text = generated(&[], &[EXTEND_BASE], &[base.clone()]).unwrap();
        assert_eq!(text, "package shop;\n\nimport org.mapstruct.Mapper;\n\n@Mapper\npublic interface CustomerMapper extends EntityMapper<CustomerDto, Customer> {\n}\n");
        assert!(generated(&[], &[EXTEND_BASE, "toEntity"], &[base]).is_err());
        assert!(generated(&[], &[EXTEND_BASE], &[]).is_err(), "the option is offered only when a generic base mapper exists");
    }
}
