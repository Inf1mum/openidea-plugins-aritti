//! More MapStruct helpers around existing mappers: adding mapping methods, a generic base mapper
//! (and making one mapper extend it) and injecting a Spring-component-model mapper.
//!
//! MapStruct reference facts used (stable reference guide): mappers are interfaces or abstract
//! classes (3.3); only interfaces are handled here. `@Mapping(source, target, ignore)` (3.1, 5.3),
//! `@MappingTarget` update methods (3.6), collection mapping methods (6), `componentModel` values
//! `default`, `cdi`, `spring`, `jsr330`, `jakarta`, `jakarta-cdi` (2.4, 4.2) and `Mappers.getMapper`
//! for the default model (4.1). The guide has no section on mapper interfaces with type parameters;
//! the generic base mapper is only checked by compiling it with the MapStruct processor in the
//! opt-in generated_ test. Everything else is lexical: types are matched by simple name.
use super::*;
use super::mapper_action as base;
use crate::model::{closing, declaration_keyword, import_edit, members, split_top_level, without_annotations, Edit};

pub(super) const ADD_TITLE: &str = "Add mapping method to mapper…";
pub(super) const BASE_TITLE: &str = "Create generic base mapper…";
pub(super) const EXTEND_TITLE: &str = "Make mapper extend generic base mapper…";
pub(super) const INJECT_TITLE: &str = "Inject mapper…";
const DEFAULT_BASE: &str = "EntityMapper";
const MAX_PAIRS: usize = 64;
const MAX_MAPPERS: u32 = 65;
const RESERVED: [&str; 7] = ["Mapper", "Mapping", "Mappings", "MappingTarget", "ReportingPolicy", "List", "MappingConstants"];
const MODIFIERS: [&str; 6] = ["public", "private", "protected", "abstract", "default", "static"];
const DIRECTIONS: [(&str, &str); 3] = [
    ("toDto", "toDto(entity): entity to DTO"),
    ("toEntity", "toEntity(dto): DTO to new entity"),
    ("updateEntity", "updateEntity(dto, @MappingTarget entity): update an existing entity"),
];
const UNSUPPORTED: &str = "A mapper member has a layout that is not supported; nothing is changed.";

struct Mapper {
    kotlin: bool,
    name: String,
    name_end: usize,
    open: usize,
    close: usize,
    masked: String,
    imports: Vec<String>,
    extends: bool,
    component: Option<String>,
}

fn component_of(args: &str) -> Option<String> {
    let at = args.find("componentModel")?;
    let rest = args[at + "componentModel".len()..].trim_start().strip_prefix('=')?.trim_start().strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_owned())
}

fn analyze(doc: &Document) -> Result<Mapper, String> {
    let single = "Invoke this inside a file with exactly one top-level MapStruct mapper interface.";
    if doc.text.len() > MAX_TEXT {
        return Err("The source exceeds the supported bound.".into());
    }
    let kotlin = doc.language == "kotlin";
    let masked = mask(&doc.text);
    let imports = base::imports_of(&masked).ok_or("Import aliases and static imports are not supported.")?;
    let interfaces: Vec<_> = words(&masked, "interface").collect();
    let [at] = interfaces.as_slice() else {
        return Err(single.into());
    };
    let at = *at;
    if declaration_keyword(&masked, 0, &["class", "enum", "object", "record"]).is_some() {
        return Err(single.into());
    }
    let rest = masked[at + "interface".len()..].trim_start();
    let name: String = rest.chars().take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$')).collect();
    let name_end = masked.len() - rest.len() + name.len();
    if !identifier(&name) || file_stem(&doc.path) != name {
        return Err("The interface name must be a plain identifier equal to the file name.".into());
    }
    if masked[name_end..].trim_start().starts_with('<') {
        return Err("Generic mapper interfaces are not supported.".into());
    }
    let open = name_end + masked[name_end..].find('{').ok_or("The interface body was not found.")?;
    let close = closing(masked.as_bytes(), open).ok_or("The interface body is not closed.")?;
    if !masked[close + 1..].trim().is_empty() {
        return Err(single.into());
    }
    let extends = !masked[name_end..open].trim().is_empty();
    let tops = super::index_query_action::top(&masked, at);
    let annotation = tops.iter().find(|annotation| annotation.name == "Mapper").ok_or("The interface needs a @Mapper annotation.")?;
    let written = &masked[annotation.start + 1..annotation.name_end];
    let proven = written == "org.mapstruct.Mapper"
        || (written == "Mapper"
            && imports.iter().any(|import| import == "org.mapstruct.Mapper" || import == "org.mapstruct.*")
            && imports.iter().all(|import| import == "org.mapstruct.Mapper" || import.rsplit('.').next() != Some("Mapper")));
    if !proven {
        return Err("@Mapper must come from org.mapstruct.".into());
    }
    let component = annotation.args.and_then(|(start, end)| component_of(&doc.text[start..end]));
    Ok(Mapper { kotlin, name, name_end, open, close, masked, imports, extends, component })
}

pub(super) fn eligible(doc: &Document) -> bool {
    analyze(doc).is_ok()
}

pub(super) fn eligible_extend(doc: &Document) -> bool {
    analyze(doc).is_ok_and(|mapper| !mapper.extends)
}

struct Sig {
    name: String,
    params: Vec<String>,
    ret: String,
}

fn simple_type(ty: &str) -> String {
    let ty: String = ty.chars().filter(|ch| !ch.is_whitespace()).collect();
    if ty.contains('<') {
        return ty;
    }
    ty.rsplit('.').next().unwrap_or_default().trim_end_matches('?').to_owned()
}

fn signatures(mapper: &Mapper) -> Result<Vec<Sig>, String> {
    let mut found = Vec::new();
    for (start, end) in members(&mapper.masked, mapper.open + 1, mapper.close, mapper.kotlin, false) {
        let text = without_annotations(&mapper.masked, start, end);
        let Some(paren) = text.find('(') else {
            continue;
        };
        let close = closing(text.as_bytes(), paren).ok_or(UNSUPPORTED)?;
        let mut head = text[..paren].trim();
        let (name, ret) = if mapper.kotlin {
            head = head.strip_prefix("fun").ok_or(UNSUPPORTED)?.trim_start();
            if head.starts_with('<') {
                return Err("Mappers with generic methods are not supported.".into());
            }
            let after = text[close + 1..].trim_start();
            let ret = after
                .strip_prefix(':')
                .map(|rest| rest.split(['{', '=']).next().unwrap_or_default().trim().to_owned())
                .unwrap_or_default();
            (head.to_owned(), if ret.is_empty() || ret == "Unit" { "void".to_owned() } else { simple_type(&ret) })
        } else {
            while let Some(word) = MODIFIERS.iter().find(|word| head.strip_prefix(**word).is_some_and(|rest| rest.starts_with(char::is_whitespace))) {
                head = head[word.len()..].trim_start();
            }
            if head.starts_with('<') {
                return Err("Mappers with generic methods are not supported.".into());
            }
            let (ret, name) = head.rsplit_once(char::is_whitespace).ok_or(UNSUPPORTED)?;
            (name.to_owned(), simple_type(ret))
        };
        if !identifier(&name) {
            return Err(UNSUPPORTED.into());
        }
        let mut params = Vec::new();
        for (_, part) in split_top_level(&text[paren + 1..close]) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let ty = if mapper.kotlin {
                part.split_once(':').ok_or(UNSUPPORTED)?.1
            } else {
                part.trim_start_matches("final ").rsplit_once(char::is_whitespace).ok_or(UNSUPPORTED)?.0
            };
            params.push(simple_type(ty));
        }
        found.push(Sig { name, params, ret });
    }
    Ok(found)
}

fn clash(sigs: &[Sig], name: &str, params: &[&str], ret: &str) -> bool {
    sigs.iter().any(|sig| sig.params == params && (sig.name == name || sig.ret == ret))
}

fn needed_imports(mapper: &Mapper, own_package: &str, types: &[(&str, &str)], framework: &[&str]) -> Result<Vec<String>, String> {
    let mut wanted = Vec::new();
    let clashes = |simple: &str, qualified: &str| {
        mapper.imports.iter().any(|import| import.rsplit('.').next() == Some(simple) && import != qualified)
    };
    for qualified in framework {
        let simple = qualified.rsplit('.').next().unwrap_or_default();
        if clashes(simple, qualified) {
            return Err(format!("`{simple}` already names a different type in this file; no import is added."));
        }
        wanted.push((*qualified).to_owned());
    }
    for (simple, package) in types {
        let qualified = format!("{package}.{simple}");
        if *simple == mapper.name || RESERVED.contains(simple) || clashes(simple, &qualified) {
            return Err(format!("`{simple}` clashes with another type or import in this mapper; no change is made."));
        }
        if *package != own_package {
            if mapper.imports.iter().any(|import| import.ends_with(".*") && import != "org.mapstruct.*") {
                return Err("Wildcard imports other than org.mapstruct.* could make type names ambiguous; no change is made.".into());
            }
            wanted.push(qualified);
        }
    }
    Ok(wanted)
}

fn with_imports(doc: &Document, wanted: &[String], mut edits: Vec<Edit>, kotlin: bool) -> Vec<TextEdit> {
    let refs: Vec<&str> = wanted.iter().map(String::as_str).collect();
    if let Some(edit) = import_edit(&doc.text, &refs, kotlin) {
        edits.insert(0, edit);
    }
    let crlf = doc.text.contains("\r\n");
    edits
        .into_iter()
        .map(|mut edit| {
            if crlf {
                edit.text = edit.text.replace('\n', "\r\n");
            }
            edit.to_text_edit(&doc.text)
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Dir {
    ToDto,
    ToEntity,
    Update,
}

impl Dir {
    fn key(self) -> &'static str {
        DIRECTIONS[self as usize].0
    }
}

struct Methods<'a> {
    kotlin: bool,
    entity: &'a str,
    dto: &'a str,
    directions: &'a [Dir],
    shared: &'a [String],
    pairs: &'a [(String, String)],
    dto_fields: Vec<&'a str>,
    entity_attributes: Vec<&'a str>,
}

fn mapping_lines(kotlin: bool, pairs: &[(String, String)], ignores: &[String]) -> String {
    let each: Vec<String> = pairs
        .iter()
        .map(|(source, target)| format!("Mapping(source = \"{source}\", target = \"{target}\")"))
        .chain(ignores.iter().map(|name| format!("Mapping(target = \"{name}\", ignore = true)")))
        .collect();
    if each.is_empty() {
        return String::new();
    }
    if kotlin {
        return format!("    @Mappings({})\n", each.join(", "));
    }
    each.iter().map(|line| format!("    @{line}\n")).collect()
}

fn render_methods(spec: &Methods) -> (Vec<String>, Vec<&'static str>) {
    let shared = |name: &&str| spec.shared.iter().any(|known| known == name);
    let ignores = |names: &[&str], other: &[&str]| -> Vec<String> {
        names.iter().filter(|name| !shared(name) && other.contains(name)).map(|name| (*name).to_owned()).collect()
    };
    let to_dto_ignores = ignores(&spec.dto_fields, &spec.entity_attributes);
    let to_entity_ignores = ignores(&spec.entity_attributes, &spec.dto_fields);
    let swapped: Vec<(String, String)> = spec.pairs.iter().map(|(entity, dto)| (dto.clone(), entity.clone())).collect();
    let (entity, dto, kotlin) = (spec.entity, spec.dto, spec.kotlin);
    let mut imports = Vec::new();
    let mut methods = Vec::new();
    for direction in spec.directions {
        let (block, signature) = match direction {
            Dir::ToDto => (
                mapping_lines(kotlin, spec.pairs, &to_dto_ignores),
                if kotlin { format!("    fun toDto(entity: {entity}): {dto}") } else { format!("    {dto} toDto({entity} entity);") },
            ),
            Dir::ToEntity => (
                mapping_lines(kotlin, &swapped, &to_entity_ignores),
                if kotlin { format!("    fun toEntity(dto: {dto}): {entity}") } else { format!("    {entity} toEntity({dto} dto);") },
            ),
            Dir::Update => (
                mapping_lines(kotlin, &swapped, &to_entity_ignores),
                if kotlin {
                    format!("    fun updateEntity(dto: {dto}, @MappingTarget entity: {entity})")
                } else {
                    format!("    void updateEntity({dto} dto, @MappingTarget {entity} entity);")
                },
            ),
        };
        if !block.is_empty() {
            imports.push("org.mapstruct.Mapping");
            if kotlin {
                imports.push("org.mapstruct.Mappings");
            }
        }
        if *direction == Dir::Update {
            imports.push("org.mapstruct.MappingTarget");
        }
        methods.push(format!("{block}{signature}"));
    }
    (methods, imports)
}

fn method_edit(doc: &Document, mapper: &Mapper, methods: &[String]) -> Result<Edit, String> {
    let line_start = doc.text[..mapper.close].rfind('\n').map_or(0, |index| index + 1);
    if !doc.text[line_start..mapper.close].trim().is_empty() || line_start <= mapper.open {
        return Err("The interface closes on a line with code; this layout is not supported.".into());
    }
    let empty = mapper.masked[mapper.open + 1..line_start].trim().is_empty();
    let after_blank = doc.text[..line_start].replace('\r', "").ends_with("\n\n");
    let lead = if empty || after_blank { "" } else { "\n" };
    Ok(Edit::insert(line_start, &format!("{lead}{}\n", methods.join("\n\n"))))
}

pub(super) struct Candidate {
    pub(super) doc: Document,
    pub(super) class: Class,
    pub(super) scalars: Vec<(String, String)>,
}

struct Chosen {
    entity: Candidate,
    dto: base::Dto,
    shared: Vec<(String, String)>,
}

pub(super) fn nearby(project: &Project, root: &str, directory: &str, kotlin: bool) -> Result<Vec<String>, Error> {
    let relative = directory.strip_prefix(&format!("{}/", root.trim_end_matches('/'))).ok_or(Error::Stale)?;
    let extension = if kotlin { "kt" } else { "java" };
    let mut globs = vec![format!("{relative}/*.{extension}")];
    if let Some((parent, _)) = relative.rsplit_once('/') {
        globs.push(format!("{parent}/*/*.{extension}"));
    }
    let mut found = Vec::new();
    for glob in globs {
        let files = project.find_files(&glob, 65)?;
        if files.len() >= 65 {
            return Err("Discovery is incomplete in this folder; nothing was changed.".into());
        }
        for file in files {
            if path(&file).as_deref() != Some(file.as_str()) || !inside(root, &file) {
                return Err(Error::Stale);
            }
            if !found.contains(&file) {
                found.push(file);
            }
        }
    }
    Ok(found)
}

pub(super) fn entities(project: &Project, root: &str, mapper: &Document, kotlin: bool) -> Result<Vec<Candidate>, Error> {
    let directory = mapper.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut found = Vec::new();
    for file in nearby(project, root, directory, kotlin)?.into_iter().filter(|file| file != &mapper.path) {
        if Instant::now() >= deadline {
            return Err("Entity discovery timed out.".into());
        }
        let stem = file_stem(&file);
        if ["Dto", "DTO", "Mapper"].iter().any(|suffix| stem.ends_with(suffix)) {
            continue;
        }
        let text = project.read_file(&file)?;
        if text.len() > MAX_TEXT || !text.contains("Entity") {
            continue;
        }
        let document = Document { path: file.clone(), text, language: mapper.language.clone(), version: 0 };
        let Some(class) = dto_action::source(&document).filter(|class| class.name == stem && !base::uses_lombok(&document.text)) else {
            continue;
        };
        let scalars = base::scalars(&class);
        if scalars.is_empty() || scalars.len() > base::MAX_FIELDS || package(&document).is_none() {
            continue;
        }
        found.push(Candidate { doc: document, class, scalars });
    }
    found.sort_by(|a, b| a.doc.path.cmp(&b.doc.path));
    found.truncate(base::MAX_CANDIDATES);
    Ok(found)
}

fn choose(project: &Project, pending: &Pending, doc: &Document, kotlin: bool) -> Result<Chosen, Error> {
    let candidates = entities(project, &pending.root, doc, kotlin)?;
    if candidates.is_empty() {
        return Err("No @Entity class with bounded scalar fields (no Lombok) was found in this folder or a sibling folder.".into());
    }
    let items: Vec<_> = candidates
        .iter()
        .map(|candidate| PickItem::new(&candidate.doc.path, &candidate.class.name).description(candidate.class.package.as_deref().unwrap_or_default()))
        .collect();
    let picked = host::pick("Entity to map", &items)?.ok_or(Error::Cancelled)?;
    let entity = candidates.into_iter().find(|candidate| candidate.doc.path == picked).ok_or("Choose one of the offered entities.")?;
    let directory = entity.doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let dtos = base::discover(project, &pending.root, directory, &entity.doc, &entity.class, &entity.scalars)?;
    if dtos.is_empty() {
        return Err("No Java record or Kotlin data class named *Dto/*DTO next to the entity shares scalar fields with identical names and types.".into());
    }
    let items: Vec<_> = dtos
        .iter()
        .map(|dto| PickItem::new(&dto.path, &dto.name).description(&format!("{} - {} shared scalar fields", dto.package, base::common(&entity.class, &entity.scalars, dto).len())))
        .collect();
    let picked = host::pick("Existing DTO to map", &items)?.ok_or(Error::Cancelled)?;
    let dto = dtos.into_iter().find(|dto| dto.path == picked).ok_or("Choose one of the offered DTOs.")?;
    let shared = base::common(&entity.class, &entity.scalars, &dto);
    Ok(Chosen { entity, dto, shared })
}

fn unchanged(project: &Project, chosen: &Chosen) -> Result<(), Error> {
    if project.read_file(&chosen.entity.doc.path)? != chosen.entity.doc.text {
        return Err(Error::Stale);
    }
    base::reread(project, &chosen.dto)
}

fn rename_pairs(chosen: &Chosen) -> Vec<(String, String, String)> {
    let mut view = chosen.entity.class.clone();
    view.kotlin = chosen.dto.kotlin;
    view.imports = chosen.dto.imports.clone();
    let attributes: Vec<&str> = chosen.entity.class.attributes.iter().map(|attribute| attribute.name.as_str()).collect();
    let mut found = Vec::new();
    for (name, ty) in &chosen.entity.scalars {
        if chosen.dto.fields.iter().any(|(field, _)| field == name) {
            continue;
        }
        for (field, dto_ty) in &chosen.dto.fields {
            if dto_ty == ty && !attributes.contains(&field.as_str()) && dto_action::supported(&view, ty) && found.len() < MAX_PAIRS {
                found.push((name.clone(), field.clone(), ty.clone()));
            }
        }
    }
    found
}

fn directions() -> Result<Vec<Dir>, Error> {
    let items: Vec<_> = DIRECTIONS.iter().map(|(key, label)| PickItem::new(key, label)).collect();
    let chosen = host::pick_many("Mapping methods to add", &items)?.ok_or(Error::Cancelled)?;
    let mut found = Vec::new();
    for key in &chosen {
        let direction = [Dir::ToDto, Dir::ToEntity, Dir::Update].into_iter().find(|direction| direction.key() == key);
        match direction {
            Some(direction) if !found.contains(&direction) => found.push(direction),
            _ => return Err("Choose only offered methods, each once.".into()),
        }
    }
    if found.is_empty() {
        return Err("Choose at least one method.".into());
    }
    Ok(found)
}

fn explicit_pairs(chosen: &Chosen) -> Result<Vec<(String, String)>, Error> {
    let offered = rename_pairs(chosen);
    if offered.is_empty() {
        return Ok(Vec::new());
    }
    let items: Vec<_> = offered
        .iter()
        .map(|(entity, dto, ty)| PickItem::new(&format!("{entity}|{dto}"), &format!("{entity} -> {dto}")).description(ty))
        .collect();
    let picked = host::pick_many("Explicit @Mapping for differently named fields (identical types on both sides)", &items)?.ok_or(Error::Cancelled)?;
    let mut pairs: Vec<(String, String)> = Vec::new();
    for key in &picked {
        let (entity, dto, _) = offered.iter().find(|(entity, dto, _)| format!("{entity}|{dto}") == *key).ok_or("Choose only offered field pairs.")?;
        if pairs.iter().any(|(known_entity, known_dto)| known_entity == entity || known_dto == dto) {
            return Err("Each entity field and each DTO field can be in at most one pair.".into());
        }
        pairs.push((entity.clone(), dto.clone()));
    }
    Ok(pairs)
}

fn confirm(title: &str, doc: &Document, detail: &str, after: &str) -> Result<(), Error> {
    let preview = format!(
        "Target: {}\nSource version: {}\n{detail}\n\n{after}\nOnly this source is edited. Inherited members and the build are not read; types are matched by simple name.",
        doc.path, doc.version
    );
    if !ui_kit::prompts::confirm_exact(title, &preview)? {
        return Err(Error::Cancelled);
    }
    Ok(())
}

pub(super) fn resolve_add(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let mapper = analyze(doc).map_err(Error::from)?;
    if mapper.extends {
        return Err("This mapper extends another type; inherited mapping methods are not read, so a new method could be ambiguous. Nothing is changed.".into());
    }
    let sigs = signatures(&mapper).map_err(Error::from)?;
    let chosen = choose(project, pending, doc, mapper.kotlin)?;
    let directions = directions()?;
    let pairs = explicit_pairs(&chosen)?;
    current(pending, doc)?;
    unchanged(project, &chosen)?;
    let (entity, dto) = (chosen.entity.class.name.as_str(), chosen.dto.name.as_str());
    for direction in &directions {
        let (name, params, ret): (&str, Vec<&str>, &str) = match direction {
            Dir::ToDto => ("toDto", vec![entity], dto),
            Dir::ToEntity => ("toEntity", vec![dto], entity),
            Dir::Update => ("updateEntity", vec![dto, entity], "void"),
        };
        if clash(&sigs, name, &params, ret) {
            return Err(format!("A method with the same signature as {name} already exists in this mapper; nothing is changed.").into());
        }
    }
    let shared: Vec<String> = chosen.shared.iter().map(|(name, _)| name.clone()).collect();
    let mut accessor_names = shared.clone();
    accessor_names.extend(pairs.iter().map(|(entity, _)| entity.clone()));
    let writes = directions.iter().any(|direction| *direction != Dir::ToDto);
    base::accessors(&chosen.entity.doc, &chosen.entity.class, &chosen.entity.scalars, &accessor_names, writes)?;
    let own_package = package(doc).ok_or(Error::Stale)?;
    let entity_package = chosen.entity.class.package.clone().ok_or(Error::Stale)?;
    let spec = Methods {
        kotlin: mapper.kotlin,
        entity,
        dto,
        directions: &directions,
        shared: &shared,
        pairs: &pairs,
        dto_fields: chosen.dto.fields.iter().map(|(field, _)| field.as_str()).collect(),
        entity_attributes: chosen.entity.class.attributes.iter().map(|attribute| attribute.name.as_str()).collect(),
    };
    let (methods, framework) = render_methods(&spec);
    let mut framework: Vec<&str> = framework;
    framework.sort_unstable();
    framework.dedup();
    let wanted = needed_imports(&mapper, &own_package, &[(entity, &entity_package), (dto, &chosen.dto.package)], &framework).map_err(Error::from)?;
    let edits = with_imports(doc, &wanted, vec![method_edit(doc, &mapper, &methods).map_err(Error::from)?], mapper.kotlin);
    let after = patched(&doc.text, &edits)?;
    if after.len() > MAX_TEXT {
        return Err("Generated source exceeds the supported bound.".into());
    }
    let keys: Vec<&str> = directions.iter().map(|direction| direction.key()).collect();
    confirm(
        "Add mapping methods?",
        doc,
        &format!("Entity: {}\nDTO: {}\nMethods: {}", chosen.entity.doc.path, chosen.dto.path, keys.join(", ")),
        &after,
    )?;
    current(pending, doc)?;
    unchanged(project, &chosen)?;
    Ok(FileAction {
        title: format!("Add mapping methods to {}", mapper.name),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), edits)],
        ..FileAction::default()
    })
}

fn render_base(kotlin: bool, package: &str, name: &str) -> String {
    if kotlin {
        return format!(
            "package {package}\n\nimport org.mapstruct.MappingTarget\n\ninterface {name}<D, E> {{\n    fun toEntity(dto: D): E\n\n    fun toDto(entity: E): D\n\n    fun toEntity(dtoList: List<D>): List<E>\n\n    fun toDto(entityList: List<E>): List<D>\n\n    fun updateEntity(dto: D, @MappingTarget entity: E)\n}}\n"
        );
    }
    format!(
        "package {package};\n\nimport java.util.List;\nimport org.mapstruct.MappingTarget;\n\npublic interface {name}<D, E> {{\n    E toEntity(D dto);\n\n    D toDto(E entity);\n\n    List<E> toEntity(List<D> dtoList);\n\n    List<D> toDto(List<E> entityList);\n\n    void updateEntity(D dto, @MappingTarget E entity);\n}}\n"
    )
}

pub(super) fn resolve_base(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let mapper = analyze(doc).map_err(Error::from)?;
    let name = host::input("Generic base mapper type name (same package as this mapper)", Some(DEFAULT_BASE))?.ok_or(Error::Cancelled)?;
    let clashes = mapper.imports.iter().any(|import| import.rsplit('.').next() == Some(name.as_str()));
    if !identifier(&name) || name == mapper.name || RESERVED.contains(&name.as_str()) || clashes {
        return Err("Use a non-conflicting Java/Kotlin identifier for the base mapper (at most 128 bytes).".into());
    }
    current(pending, doc)?;
    let declared = package(doc).ok_or(Error::Stale)?;
    let mut generated = render_base(mapper.kotlin, &declared, &name);
    if doc.text.contains("\r\n") {
        generated = generated.replace('\n', "\r\n");
    }
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let target = format!("{directory}/{name}.{}", if mapper.kotlin { "kt" } else { "java" });
    if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
        return Err(Error::Stale);
    }
    no_collision(project, &pending.root, directory, &name, None)?;
    let note = if mapper.kotlin {
        "Kotlin: interface type parameters with kapt are not verified here."
    } else {
        "Java: a generic mapper supertype is not covered by the MapStruct reference guide; it is verified only by an opt-in compile test."
    };
    let preview = format!(
        "Source: {}\nSource version: {}\nTarget: {target}\n\n{note}\nThe base has no @Mapper; use 'Make mapper extend generic base mapper' on a mapper to inherit it.\n\nExact new file:\n{generated}\nOne atomic host-managed Create; no existing file is overwritten.",
        doc.path, doc.version
    );
    if !ui_kit::prompts::confirm_exact("Create generic base mapper?", &preview)? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    no_collision(project, &pending.root, directory, &name, None)?;
    Ok(FileAction {
        title: format!("Create generic base mapper {name}"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target.clone(), text: generated, overwrite: false }],
        open: Some(target),
        ..FileAction::default()
    })
}

struct BaseMapper {
    name: String,
    package: String,
}

fn base_mappers(project: &Project, root: &str, doc: &Document, kotlin: bool) -> Result<Vec<BaseMapper>, Error> {
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let mut found = Vec::new();
    for file in nearby(project, root, directory, kotlin)?.into_iter().filter(|file| file != &doc.path) {
        let text = project.read_file(&file)?;
        if text.len() > MAX_TEXT {
            continue;
        }
        let document = Document { path: file.clone(), text: text.clone(), language: doc.language.clone(), version: 0 };
        let Some(package) = package(&document) else {
            continue;
        };
        let stem = file_stem(&file);
        if render_base(kotlin, &package, stem) == text.replace("\r\n", "\n") {
            found.push(BaseMapper { name: stem.to_owned(), package });
        }
    }
    Ok(found)
}

pub(super) fn resolve_extend(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let mapper = analyze(doc).map_err(Error::from)?;
    if mapper.extends {
        return Err("This mapper already extends another type; nothing is changed.".into());
    }
    let sigs = signatures(&mapper).map_err(Error::from)?;
    if sigs.iter().any(|sig| ["toDto", "toEntity", "updateEntity"].contains(&sig.name.as_str())) {
        return Err("This mapper already declares toDto, toEntity or updateEntity; inheriting the base could make them ambiguous. Nothing is changed.".into());
    }
    let bases = base_mappers(project, &pending.root, doc, mapper.kotlin)?;
    if bases.is_empty() {
        return Err("No generic base mapper generated by 'Create generic base mapper' (byte-identical) was found in this folder or a sibling folder.".into());
    }
    let items: Vec<_> = bases.iter().enumerate().map(|(index, found)| PickItem::new(&index.to_string(), &found.name).description(&found.package)).collect();
    let picked = host::pick("Generic base mapper", &items)?.ok_or(Error::Cancelled)?;
    let base = bases.get(picked.parse::<usize>().map_err(|_| Error::Stale)?).ok_or(Error::Stale)?;
    let chosen = choose(project, pending, doc, mapper.kotlin)?;
    current(pending, doc)?;
    unchanged(project, &chosen)?;
    if base_mappers(project, &pending.root, doc, mapper.kotlin)?.iter().all(|found| found.name != base.name || found.package != base.package) {
        return Err(Error::Stale);
    }
    let shared: Vec<String> = chosen.shared.iter().map(|(name, _)| name.clone()).collect();
    base::accessors(&chosen.entity.doc, &chosen.entity.class, &chosen.entity.scalars, &shared, true)?;
    let own_package = package(doc).ok_or(Error::Stale)?;
    let entity_package = chosen.entity.class.package.clone().ok_or(Error::Stale)?;
    let (entity, dto) = (chosen.entity.class.name.as_str(), chosen.dto.name.as_str());
    let wanted = needed_imports(&mapper, &own_package, &[(entity, &entity_package), (dto, &chosen.dto.package), (&base.name, &base.package)], &[]).map_err(Error::from)?;
    let header = if mapper.kotlin { format!(" : {}<{dto}, {entity}>", base.name) } else { format!(" extends {}<{dto}, {entity}>", base.name) };
    let edits = with_imports(doc, &wanted, vec![Edit::insert(mapper.name_end, &header)], mapper.kotlin);
    let after = patched(&doc.text, &edits)?;
    if after.len() > MAX_TEXT {
        return Err("Generated source exceeds the supported bound.".into());
    }
    confirm(
        "Make mapper extend the generic base mapper?",
        doc,
        &format!("Base: {}\nEntity: {}\nDTO: {}", base.name, chosen.entity.doc.path, chosen.dto.path),
        &after,
    )?;
    current(pending, doc)?;
    unchanged(project, &chosen)?;
    Ok(FileAction {
        title: format!("Make {} extend {}", mapper.name, base.name),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), edits)],
        ..FileAction::default()
    })
}

fn spring_mappers(project: &Project, root: &str, own: &str) -> Result<(Vec<injection_action::Repo>, usize), Error> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut found: Vec<injection_action::Repo> = Vec::new();
    let mut other = 0;
    for extension in ["java", "kt"] {
        let files = project.find_files(&format!("**/*Mapper.{extension}"), MAX_MAPPERS)?;
        if files.len() >= MAX_MAPPERS as usize {
            return Err("Mapper discovery is incomplete: too many *Mapper files in this project.".into());
        }
        for file in files.into_iter().filter(|file| file != own) {
            if Instant::now() >= deadline {
                return Err("Mapper discovery timed out.".into());
            }
            if path(&file).as_deref() != Some(file.as_str()) || !inside(root, &file) {
                return Err(Error::Stale);
            }
            let text = project.read_file(&file)?;
            let document = Document { path: file.clone(), text, language: if extension == "kt" { "kotlin" } else { "java" }.into(), version: 0 };
            let Ok(mapper) = analyze(&document) else {
                continue;
            };
            let Some(package) = package(&document) else {
                continue;
            };
            if mapper.component.as_deref() != Some("spring") {
                other += 1;
                continue;
            }
            found.push(injection_action::Repo { name: mapper.name, entity: String::new(), package });
        }
    }
    found.sort_by(|a, b| (&a.package, &a.name).cmp(&(&b.package, &b.name)));
    found.dedup();
    Ok((found, other))
}

pub(super) fn resolve_inject(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let src = injection_action::analyze(doc).map_err(Error::from)?;
    let style = injection_action::style()?;
    let (mappers, other) = spring_mappers(project, &pending.root, &doc.path)?;
    if mappers.is_empty() {
        return Err(format!(
            "No MapStruct mapper interface with a literal componentModel = \"spring\" was found ({other} other mapper(s) skipped). Mappers with the default component model are obtained through Mappers.getMapper (reference 4.1); no static field is generated, and other component models are not injected."
        )
        .into());
    }
    let items: Vec<_> = mappers.iter().enumerate().map(|(index, mapper)| PickItem::new(&index.to_string(), &mapper.name).description(&mapper.package)).collect();
    let picked = host::pick("Mapper to inject", &items)?.ok_or(Error::Cancelled)?;
    let mapper = mappers.get(picked.parse::<usize>().map_err(|_| Error::Stale)?).ok_or(Error::Stale)?.clone();
    let lombok = style == injection_action::Style::Lombok && injection_action::lombok_provable(doc, project, &pending.root)?;
    let project = current(pending, doc)?;
    if !spring_mappers(&project, &pending.root, &doc.path)?.0.contains(&mapper) {
        return Err(Error::Stale);
    }
    let edits = injection_action::plan(doc, &src, &mapper, style, lombok).map_err(Error::from)?;
    let after = patched(&doc.text, &edits)?;
    if after.len() > MAX_TEXT {
        return Err("Generated source exceeds the supported bound.".into());
    }
    confirm("Inject mapper?", doc, &format!("Style: {style:?} (setting injectionStyle)\nMapper: {}.{} (componentModel spring)", mapper.package, mapper.name), &after)?;
    current(pending, doc)?;
    Ok(FileAction {
        title: format!("Inject {}", mapper.name),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), edits)],
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const MAPPER_PATH: &str = "/project/src/main/java/shop/CustomerMapper.java";
    const ENTITY_PATH: &str = "/project/src/main/java/shop/Customer.java";
    const DTO_PATH: &str = "/project/src/main/java/shop/CustomerDto.java";
    const MAPPER: &str = "package shop;\n\nimport org.mapstruct.Mapper;\n\n@Mapper\npublic interface CustomerMapper {\n}\n";
    const ENTITY: &str = "package shop;\nimport jakarta.persistence.*;\nimport java.time.LocalDate;\n@Entity\npublic class Customer {\n @Id Long id;\n String title;\n LocalDate joined;\n @ManyToOne Other parent;\n public Long getId() { return id; }\n public void setId(Long id) { this.id = id; }\n public String getTitle() { return title; }\n public void setTitle(String title) { this.title = title; }\n public LocalDate getJoined() { return joined; }\n public void setJoined(LocalDate joined) { this.joined = joined; }\n}\n";
    const DTO: &str = "package shop;\nimport java.time.LocalDate;\npublic record CustomerDto(Long id, String name, LocalDate joined, String parent) {}\n";
    const K_MAPPER_PATH: &str = "/project/src/main/kotlin/shop/CustomerMapper.kt";
    const K_MAPPER: &str = "package shop\n\nimport org.mapstruct.Mapper\n\n@Mapper\ninterface CustomerMapper {\n}\n";
    const K_ENTITY: &str = "package shop\nimport jakarta.persistence.*\n@Entity\nclass Customer(@Id var id: Long? = null, var title: String = \"\", var joined: String = \"\") {}\n";
    const K_DTO: &str = "package shop\ndata class CustomerDto(val id: Long? = null, val name: String? = null, val joined: String? = null)\n";

    fn setup(path: &str, source: &str, language: &str, files: &[(&str, &str)]) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(path, language, source, &[]);
        for (name, text) in files {
            host.add_file(name, text);
        }
        let doc = Document { path: path.into(), text: source.into(), language: language.into(), version: 1 };
        (host, JpaAssist::default(), doc)
    }

    fn java(mapper: &str) -> (MockHost, JpaAssist, Document) {
        setup(MAPPER_PATH, mapper, "java", &[("src/main/java/shop/Customer.java", ENTITY), ("src/main/java/shop/CustomerDto.java", DTO)])
    }

    fn kotlin() -> (MockHost, JpaAssist, Document) {
        setup(K_MAPPER_PATH, K_MAPPER, "kotlin", &[("src/main/kotlin/shop/Customer.kt", K_ENTITY), ("src/main/kotlin/shop/CustomerDto.kt", K_DTO)])
    }

    fn action(plugin: &mut JpaAssist, doc: &Document, title: &str) -> FileAction {
        let action = plugin.file_actions(doc, Range::default()).into_iter().find(|action| action.title == title).unwrap();
        assert!(action.operations.is_empty() && action.edits.is_empty());
        action
    }

    fn answer_add(host: &MockHost, entity: &str, dto: &str, directions: &[&str], pairs: Option<&[&str]>, confirm: bool) {
        host.answer_next_prompt(Some(&[entity]));
        host.answer_next_prompt(Some(&[dto]));
        host.answer_next_prompt(Some(directions));
        if let Some(pairs) = pairs {
            host.answer_next_prompt(Some(pairs));
        }
        host.answer_next_prompt(confirm.then_some(&[]));
    }

    fn apply(doc: &Document, action: &FileAction) -> String {
        let [(path, edits)] = action.edits.as_slice() else {
            panic!("one edited document")
        };
        assert_eq!(path, &doc.path);
        assert!(action.operations.is_empty());
        patched(&doc.text, edits).unwrap()
    }

    fn add(mapper: &str, directions: &[&str], pairs: Option<&[&str]>) -> Result<String, Error> {
        let (host, mut plugin, doc) = java(mapper);
        let pending = action(&mut plugin, &doc, ADD_TITLE);
        answer_add(&host, ENTITY_PATH, DTO_PATH, directions, pairs, true);
        plugin.resolve_file_action(&doc, pending).map(|resolved| apply(&doc, &resolved))
    }

    #[test]
    fn java_methods_with_rename_pair_and_ignores_are_exact() {
        let text = add(MAPPER, &["toDto", "toEntity", "updateEntity"], Some(&["title|name"])).unwrap();
        let expected = "package shop;\n\nimport org.mapstruct.Mapper;\nimport org.mapstruct.Mapping;\nimport org.mapstruct.MappingTarget;\n\n@Mapper\npublic interface CustomerMapper {\n    @Mapping(source = \"title\", target = \"name\")\n    @Mapping(target = \"parent\", ignore = true)\n    CustomerDto toDto(Customer entity);\n\n    @Mapping(source = \"name\", target = \"title\")\n    @Mapping(target = \"parent\", ignore = true)\n    Customer toEntity(CustomerDto dto);\n\n    @Mapping(source = \"name\", target = \"title\")\n    @Mapping(target = \"parent\", ignore = true)\n    void updateEntity(CustomerDto dto, @MappingTarget Customer entity);\n}\n";
        assert_eq!(text, expected);
    }

    #[test]
    fn no_pairs_selected_keeps_only_ignores_and_existing_members_are_separated_by_a_blank_line() {
        let mapper = MAPPER.replace("{\n}", "{\n    String hello();\n}").replace("import org.mapstruct.Mapper;", "import org.mapstruct.Mapper;\nimport org.mapstruct.Mapping;");
        let text = add(&mapper, &["toDto"], Some(&[])).unwrap();
        let expected = "package shop;\n\nimport org.mapstruct.Mapper;\nimport org.mapstruct.Mapping;\n\n@Mapper\npublic interface CustomerMapper {\n    String hello();\n\n    @Mapping(target = \"parent\", ignore = true)\n    CustomerDto toDto(Customer entity);\n}\n";
        assert_eq!(text, expected);
    }

    #[test]
    fn dto_in_a_sibling_package_is_imported_once_and_wildcard_framework_imports_are_not_repeated() {
        let (host, mut plugin, doc) = setup(
            MAPPER_PATH,
            &MAPPER.replace("org.mapstruct.Mapper;", "org.mapstruct.*;").replace("@Mapper\n", "@Mapper\n"),
            "java",
            &[("src/main/java/shop/Customer.java", ENTITY), ("src/main/java/dto/CustomerDto.java", &DTO.replace("package shop;", "package dto;"))],
        );
        let pending = action(&mut plugin, &doc, ADD_TITLE);
        answer_add(&host, ENTITY_PATH, "/project/src/main/java/dto/CustomerDto.java", &["toDto", "updateEntity"], Some(&[]), true);
        let text = apply(&doc, &plugin.resolve_file_action(&doc, pending).unwrap());
        assert!(text.contains("import org.mapstruct.*;\nimport dto.CustomerDto;\n"), "{text}");
        assert_eq!(text.matches("import ").count(), 2, "{text}");
        assert!(text.contains("void updateEntity(CustomerDto dto, @MappingTarget Customer entity);"));
    }

    #[test]
    fn kotlin_methods_use_a_mappings_container_and_expression_free_funs() {
        let (host, mut plugin, doc) = kotlin();
        let pending = action(&mut plugin, &doc, ADD_TITLE);
        answer_add(&host, "/project/src/main/kotlin/shop/Customer.kt", "/project/src/main/kotlin/shop/CustomerDto.kt", &["toDto", "toEntity", "updateEntity"], Some(&["title|name"]), true);
        let text = apply(&doc, &plugin.resolve_file_action(&doc, pending).unwrap());
        let expected = "package shop\n\nimport org.mapstruct.Mapper\nimport org.mapstruct.Mapping\nimport org.mapstruct.MappingTarget\nimport org.mapstruct.Mappings\n\n@Mapper\ninterface CustomerMapper {\n    @Mappings(Mapping(source = \"title\", target = \"name\"))\n    fun toDto(entity: Customer): CustomerDto\n\n    @Mappings(Mapping(source = \"name\", target = \"title\"))\n    fun toEntity(dto: CustomerDto): Customer\n\n    @Mappings(Mapping(source = \"name\", target = \"title\"))\n    fun updateEntity(dto: CustomerDto, @MappingTarget entity: Customer)\n}\n";
        assert_eq!(text, expected);
    }

    #[test]
    fn crlf_mapper_edits_are_crlf_only() {
        let text = add(&MAPPER.replace('\n', "\r\n"), &["toDto", "updateEntity"], Some(&[])).unwrap();
        assert!(text.contains("\r\n") && !text.replace("\r\n", "").contains('\n'), "{text:?}");
        assert!(text.contains("    CustomerDto toDto(Customer entity);\r\n"));
    }

    #[test]
    fn same_signature_existing_methods_inheritance_generics_and_import_clashes_are_refused() {
        let with = |member: &str| MAPPER.replace("{\n}", &format!("{{\n    {member}\n}}"));
        for existing in [
            "CustomerDto toDto(Customer entity);",
            "CustomerDto convert(Customer c);",
            "default CustomerDto toDto(pkg.Customer c) { return null; }",
            "void updateEntity(CustomerDto d, @MappingTarget Customer c);",
        ] {
            let refused = add(&with(existing), &["toDto", "updateEntity"], Some(&[]));
            assert!(refused.is_err(), "{existing}");
        }
        assert!(add(&with("<T> T make(Class<T> type);"), &["toDto"], Some(&[])).is_err());
        assert!(add(&with("List<CustomerDto> many(List<Customer> in);"), &["toDto"], Some(&[])).is_ok());
        assert!(add(&MAPPER.replace("CustomerMapper {", "CustomerMapper extends Base {"), &["toDto"], Some(&[])).is_err());
        assert!(add(&MAPPER.replace("import org.mapstruct.Mapper;", "import org.mapstruct.Mapper;\nimport other.Customer;"), &["toDto"], Some(&[])).is_err());
        assert!(add(&MAPPER.replace("import org.mapstruct.Mapper;", "import org.mapstruct.Mapper;\nimport other.*;"), &["toDto"], Some(&[])).is_ok());
        let (_host, mut plugin, doc) = java(&MAPPER.replace("CustomerMapper {", "CustomerMapper<T> {"));
        assert!(!plugin.file_actions(&doc, Range::default()).iter().any(|a| a.title == ADD_TITLE));
    }

    #[test]
    fn non_mapstruct_and_non_interface_sources_are_not_offered() {
        for source in [
            MAPPER.replace("import org.mapstruct.Mapper;", "import other.Mapper;"),
            MAPPER.replace("@Mapper\n", ""),
            MAPPER.replace("public interface", "public abstract class"),
            MAPPER.replace("import org.mapstruct.Mapper;", "import org.mapstruct.Mapper;\nimport org.mapstruct.Mapper;\nimport static x.Y.z;"),
            format!("{MAPPER}interface Second {{}}\n"),
        ] {
            let (_host, mut plugin, doc) = java(&source);
            let titles: Vec<_> = plugin.file_actions(&doc, Range::default()).into_iter().map(|a| a.title).collect();
            assert!(![ADD_TITLE, BASE_TITLE, EXTEND_TITLE].iter().any(|t| titles.iter().any(|x| x == t)), "{source}");
        }
        let (_host, mut plugin, doc) = java(&MAPPER.replace("@Mapper", "@Mapper(uses = Other.class)"));
        assert!(plugin.file_actions(&doc, Range::default()).iter().any(|a| a.title == ADD_TITLE));
    }

    #[test]
    fn forged_or_unproven_selections_and_missing_accessors_are_refused() {
        let attempts: [(&[&str], Option<&[&str]>); 7] = [
            (&["bogus"], Some(&[])),
            (&["toDto", "toDto"], Some(&[])),
            (&[], Some(&[])),
            (&["toDto"], Some(&["id|name"])),
            (&["toDto"], Some(&["title|parent"])),
            (&["toDto"], Some(&["title|name", "title|name"])),
            (&["toDto"], Some(&["name|title"])),
        ];
        for (directions, pairs) in attempts {
            assert!(add(MAPPER, directions, pairs).is_err(), "{directions:?} {pairs:?}");
        }
        let no_setter = ENTITY.replace("public void setTitle(String title) { this.title = title; }\n", "");
        let (host, mut plugin, doc) = setup(MAPPER_PATH, MAPPER, "java", &[("src/main/java/shop/Customer.java", &no_setter), ("src/main/java/shop/CustomerDto.java", DTO)]);
        let pending = action(&mut plugin, &doc, ADD_TITLE);
        answer_add(&host, ENTITY_PATH, DTO_PATH, &["toEntity"], Some(&["title|name"]), true);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        let mismatched = DTO.replace("String name", "Long name");
        let (host, mut plugin, doc) = setup(MAPPER_PATH, MAPPER, "java", &[("src/main/java/shop/Customer.java", ENTITY), ("src/main/java/shop/CustomerDto.java", &mismatched)]);
        let pending = action(&mut plugin, &doc, ADD_TITLE);
        answer_add(&host, ENTITY_PATH, DTO_PATH, &["toDto"], None, true);
        assert!(plugin.resolve_file_action(&doc, pending).is_ok());
        assert_eq!(host.editor_text(), MAPPER);
    }

    #[test]
    fn cancellation_at_every_stage_and_stale_sources_leave_the_mapper_untouched() {
        for stage in 0..5 {
            let (host, mut plugin, doc) = java(MAPPER);
            let pending = action(&mut plugin, &doc, ADD_TITLE);
            let replies: [&[&str]; 4] = [&[ENTITY_PATH], &[DTO_PATH], &["toDto"], &[]];
            for reply in replies.iter().take(stage) {
                host.answer_next_prompt(Some(reply));
            }
            assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)), "stage {stage}");
            assert_eq!(host.editor_text(), MAPPER);
        }
        let (host, mut plugin, doc) = java(MAPPER);
        let pending = action(&mut plugin, &doc, ADD_TITLE);
        let mut changed = doc.clone();
        changed.version += 1;
        assert!(matches!(plugin.resolve_file_action(&changed, pending.clone()), Err(Error::Stale)));
        host.type_in_editor("// changed");
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Stale)));
    }

    #[test]
    fn base_mapper_is_a_new_non_overwriting_sibling_file_in_java_and_kotlin() {
        let (host, mut plugin, doc) = java(MAPPER);
        let pending = action(&mut plugin, &doc, BASE_TITLE);
        host.answer_next_prompt(Some(&["EntityMapper"]));
        host.answer_next_prompt(Some(&[]));
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let [FileOperation::Create { path, text, overwrite: false }] = resolved.operations.as_slice() else {
            panic!("one create")
        };
        assert_eq!(path, "/project/src/main/java/shop/EntityMapper.java");
        assert_eq!(text, "package shop;\n\nimport java.util.List;\nimport org.mapstruct.MappingTarget;\n\npublic interface EntityMapper<D, E> {\n    E toEntity(D dto);\n\n    D toDto(E entity);\n\n    List<E> toEntity(List<D> dtoList);\n\n    List<D> toDto(List<E> entityList);\n\n    void updateEntity(D dto, @MappingTarget E entity);\n}\n");
        assert!(resolved.edits.is_empty());

        let (host, mut plugin, doc) = kotlin();
        let pending = action(&mut plugin, &doc, BASE_TITLE);
        host.answer_next_prompt(Some(&["Base"]));
        host.answer_next_prompt(Some(&[]));
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let [FileOperation::Create { path, text, .. }] = resolved.operations.as_slice() else {
            panic!("one create")
        };
        assert!(path.ends_with("/shop/Base.kt"));
        assert!(text.starts_with("package shop\n\nimport org.mapstruct.MappingTarget\n\ninterface Base<D, E> {\n    fun toEntity(dto: D): E\n"), "{text}");

        let (host, mut plugin, doc) = java(&MAPPER.replace('\n', "\r\n"));
        let pending = action(&mut plugin, &doc, BASE_TITLE);
        host.answer_next_prompt(Some(&["EntityMapper"]));
        host.answer_next_prompt(Some(&[]));
        let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
        let [FileOperation::Create { text, .. }] = resolved.operations.as_slice() else {
            panic!("one create")
        };
        assert!(!text.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn base_mapper_name_collisions_and_cancellation_are_refused() {
        for name in ["CustomerMapper", "Mapper", "List", "class", "../X", "a.b", "Customer"] {
            let (host, mut plugin, doc) = java(MAPPER);
            let pending = action(&mut plugin, &doc, BASE_TITLE);
            host.answer_next_prompt(Some(&[name]));
            host.answer_next_prompt(Some(&[]));
            let result = plugin.resolve_file_action(&doc, pending);
            assert!(result.is_err(), "{name}");
        }
        let (host, mut plugin, doc) = java(MAPPER);
        let pending = action(&mut plugin, &doc, BASE_TITLE);
        host.answer_next_prompt(Some(&["EntityMapper"]));
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)));
        let (_host, mut plugin, doc) = setup(MAPPER_PATH, MAPPER, "java", &[("src/main/java/shop/EntityMapper.kt", "package shop\ninterface EntityMapper")]);
        let pending = action(&mut plugin, &doc, BASE_TITLE);
        _host.answer_next_prompt(Some(&["EntityMapper"]));
        _host.answer_next_prompt(Some(&[]));
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
    }

    fn base_file(kotlin: bool) -> String {
        render_base(kotlin, "shop", "EntityMapper")
    }

    #[test]
    fn extend_action_edits_only_the_invoking_mapper_header_and_needs_the_exact_generated_base() {
        let (host, mut plugin, doc) = setup(
            MAPPER_PATH,
            MAPPER,
            "java",
            &[("src/main/java/shop/Customer.java", ENTITY), ("src/main/java/shop/CustomerDto.java", DTO), ("src/main/java/shop/EntityMapper.java", &base_file(false))],
        );
        let pending = action(&mut plugin, &doc, EXTEND_TITLE);
        host.answer_next_prompt(Some(&["0"]));
        host.answer_next_prompt(Some(&[ENTITY_PATH]));
        host.answer_next_prompt(Some(&[DTO_PATH]));
        host.answer_next_prompt(Some(&[]));
        let text = apply(&doc, &plugin.resolve_file_action(&doc, pending).unwrap());
        assert_eq!(text, MAPPER.replace("CustomerMapper {", "CustomerMapper extends EntityMapper<CustomerDto, Customer> {"));

        let (host, mut plugin, doc) = setup(
            K_MAPPER_PATH,
            K_MAPPER,
            "kotlin",
            &[("src/main/kotlin/shop/Customer.kt", K_ENTITY), ("src/main/kotlin/shop/CustomerDto.kt", K_DTO), ("src/main/kotlin/shop/EntityMapper.kt", &base_file(true))],
        );
        let pending = action(&mut plugin, &doc, EXTEND_TITLE);
        host.answer_next_prompt(Some(&["0"]));
        host.answer_next_prompt(Some(&["/project/src/main/kotlin/shop/Customer.kt"]));
        host.answer_next_prompt(Some(&["/project/src/main/kotlin/shop/CustomerDto.kt"]));
        host.answer_next_prompt(Some(&[]));
        let text = apply(&doc, &plugin.resolve_file_action(&doc, pending).unwrap());
        assert_eq!(text, K_MAPPER.replace("CustomerMapper {", "CustomerMapper : EntityMapper<CustomerDto, Customer> {"));
    }

    #[test]
    fn extend_action_refuses_existing_supertypes_conflicting_methods_and_foreign_bases() {
        let files = |base: &str| vec![("src/main/java/shop/Customer.java", ENTITY.to_owned()), ("src/main/java/shop/CustomerDto.java", DTO.to_owned()), ("src/main/java/shop/EntityMapper.java", base.to_owned())];
        let run = |mapper: &str, base: &str| {
            let files = files(base);
            let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (*a, b.as_str())).collect();
            let (host, mut plugin, doc) = setup(MAPPER_PATH, mapper, "java", &refs);
            if !plugin.file_actions(&doc, Range::default()).iter().any(|a| a.title == EXTEND_TITLE) {
                return false;
            }
            let pending = action(&mut plugin, &doc, EXTEND_TITLE);
            host.answer_next_prompt(Some(&["0"]));
            host.answer_next_prompt(Some(&[ENTITY_PATH]));
            host.answer_next_prompt(Some(&[DTO_PATH]));
            host.answer_next_prompt(Some(&[]));
            plugin.resolve_file_action(&doc, pending).is_ok()
        };
        assert!(run(MAPPER, &base_file(false)));
        assert!(!run(&MAPPER.replace("CustomerMapper {", "CustomerMapper extends Other {"), &base_file(false)));
        assert!(!run(&MAPPER.replace("{\n}", "{\n    CustomerDto toDto(Customer c);\n}"), &base_file(false)));
        assert!(!run(MAPPER, &base_file(false).replace("E toEntity(D dto);", "E toEntity(D dto, int extra);")));
        assert!(!run(MAPPER, "package shop;\npublic interface EntityMapper<D, E> {}\n"));
    }

    const SERVICE: &str = "package shop;\n\nimport org.springframework.stereotype.Service;\n\n@Service\npublic class OrderService {\n}\n";
    const SERVICE_PATH: &str = "/project/src/main/java/shop/OrderService.java";
    const SPRING_MAPPER: &str = "package maps;\n\nimport org.mapstruct.Mapper;\n\n@Mapper(componentModel = \"spring\")\npublic interface CustomerMapper {\n}\n";

    fn inject(files: &[(&str, &str)], service: &str, path: &str, language: &str) -> Result<String, Error> {
        let (host, mut plugin, doc) = setup(path, service, language, files);
        let pending = action(&mut plugin, &doc, INJECT_TITLE);
        host.answer_next_prompt(Some(&["0"]));
        host.answer_next_prompt(Some(&[]));
        plugin.resolve_file_action(&doc, pending).map(|resolved| apply(&doc, &resolved))
    }

    #[test]
    fn spring_mapper_is_injected_through_the_repository_rendering_styles() {
        let files = [("src/main/java/maps/CustomerMapper.java", SPRING_MAPPER)];
        let text = inject(&files, SERVICE, SERVICE_PATH, "java").unwrap();
        assert_eq!(
            text,
            "package shop;\n\nimport org.springframework.stereotype.Service;\nimport maps.CustomerMapper;\n\n@Service\npublic class OrderService {\n    private final CustomerMapper customerMapper;\n\n    public OrderService(CustomerMapper customerMapper) {\n        this.customerMapper = customerMapper;\n    }\n}\n"
        );
        let kotlin_files = [("src/main/kotlin/maps/CustomerMapper.kt", "package maps\n\nimport org.mapstruct.Mapper\n\n@Mapper(componentModel = \"spring\")\ninterface CustomerMapper {\n}\n")];
        let text = inject(
            &kotlin_files,
            "package shop\n\nimport org.springframework.stereotype.Service\n\n@Service\nclass OrderService\n",
            "/project/src/main/kotlin/shop/OrderService.kt",
            "kotlin",
        )
        .unwrap();
        assert!(text.contains("class OrderService(private val customerMapper: CustomerMapper)") && text.contains("import maps.CustomerMapper\n"), "{text}");
        let (host, mut plugin, doc) = setup(SERVICE_PATH, SERVICE, "java", &files);
        host.set_setting("injectionStyle", "\"field-autowired\"");
        let pending = action(&mut plugin, &doc, INJECT_TITLE);
        host.answer_next_prompt(Some(&["0"]));
        host.answer_next_prompt(Some(&[]));
        let text = apply(&doc, &plugin.resolve_file_action(&doc, pending).unwrap());
        assert!(text.contains("    @Autowired\n    private CustomerMapper customerMapper;\n"), "{text}");
    }

    #[test]
    fn only_literal_spring_component_model_mappers_are_injected() {
        for mapper in [
            SPRING_MAPPER.replace("(componentModel = \"spring\")", ""),
            SPRING_MAPPER.replace("\"spring\"", "\"jakarta-cdi\""),
            SPRING_MAPPER.replace("\"spring\"", "MappingConstants.ComponentModel.SPRING").replace("componentModel = ", "componentModel = "),
            SPRING_MAPPER.replace("componentModel = \"spring\"", "config = Shared.class"),
            SPRING_MAPPER.replace("public interface", "public abstract class"),
        ] {
            let files = [("src/main/java/maps/CustomerMapper.java", mapper.as_str())];
            let refused = inject(&files, SERVICE, SERVICE_PATH, "java");
            assert!(refused.is_err(), "{mapper}");
        }
        assert!(inject(&[], SERVICE, SERVICE_PATH, "java").is_err());
        let same_class = [("src/main/java/shop/OrderService.java", SERVICE)];
        assert!(inject(&same_class, SERVICE, SERVICE_PATH, "java").is_err());
    }

    #[test]
    fn mapper_already_injected_or_name_clash_and_cancellation_change_nothing() {
        let files = [("src/main/java/maps/CustomerMapper.java", SPRING_MAPPER)];
        let already = SERVICE.replace("{\n}", "{\n    private final CustomerMapper customerMapper;\n}");
        assert!(inject(&files, &already, SERVICE_PATH, "java").is_err());
        let (host, mut plugin, doc) = setup(SERVICE_PATH, SERVICE, "java", &files);
        let pending = action(&mut plugin, &doc, INJECT_TITLE);
        host.answer_next_prompt(Some(&["0"]));
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)));
        assert_eq!(host.editor_text(), SERVICE);
        let (host, mut plugin, doc) = setup(SERVICE_PATH, SERVICE, "java", &files);
        let pending = action(&mut plugin, &doc, INJECT_TITLE);
        host.answer_next_prompt(Some(&["7"]));
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        let (_host, mut plugin, doc) = setup(SERVICE_PATH, &SERVICE.replace("@Service\n", ""), "java", &files);
        assert!(!plugin.file_actions(&doc, Range::default()).iter().any(|a| a.title == INJECT_TITLE));
    }

    fn compile(name: &str, files: &[(&str, String)], processor: bool) {
        let api = std::env::var("JPA_JAKARTA_API").expect("explicit API jar path");
        let jar = std::env::var("MAPSTRUCT_JAR").expect("mapstruct jar");
        let processor_jar = std::env::var("MAPSTRUCT_PROCESSOR_JAR").expect("mapstruct-processor jar");
        let dir = std::env::temp_dir().join(format!("jpa-mapper-more-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut all: Vec<(&str, String)> = files.to_vec();
        all.push(("org/springframework/stereotype/Component.java", "package org.springframework.stereotype; public @interface Component {}".into()));
        all.push(("org/springframework/beans/factory/annotation/Autowired.java", "package org.springframework.beans.factory.annotation; public @interface Autowired {}".into()));
        let paths: Vec<_> = all
            .iter()
            .map(|(file, body)| {
                let path = dir.join(file);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, body).unwrap();
                path
            })
            .collect();
        let mut command = std::process::Command::new("javac");
        command.args(["--release", "17", "-classpath", &format!("{api}:{jar}")]);
        if processor {
            command.args(["-processorpath", &processor_jar]);
        } else {
            command.arg("-proc:none");
        }
        let result = command.arg("-d").arg(&dir).args(&paths).output().expect("javac installed");
        assert!(result.status.success(), "{name}: {}\n{files:?}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "requires javac, JPA_JAKARTA_API, MAPSTRUCT_JAR and MAPSTRUCT_PROCESSOR_JAR (mapstruct 1.6.x) jar paths"]
    fn generated_java_methods_base_mapper_extension_and_injection_compile_with_mapstruct() {
        let entity = ENTITY.replace("@ManyToOne Other parent;", "String parent;\n public String getParent() { return parent; }\n public void setParent(String parent) { this.parent = parent; }");
        let mapper = MAPPER.replace("@Mapper", "@Mapper(componentModel = \"spring\")");
        let run = |source: &str, directions: &[&str], pairs: &[&str]| {
            let (host, mut plugin, doc) = setup(MAPPER_PATH, source, "java", &[("src/main/java/shop/Customer.java", &entity), ("src/main/java/shop/CustomerDto.java", DTO)]);
            let pending = action(&mut plugin, &doc, ADD_TITLE);
            answer_add(&host, ENTITY_PATH, DTO_PATH, directions, Some(pairs), true);
            apply(&doc, &plugin.resolve_file_action(&doc, pending).unwrap())
        };
        let methods = run(&mapper, &["toDto", "toEntity", "updateEntity"], &["title|name"]);
        let common = |extra: String| vec![("shop/Customer.java", entity.clone()), ("shop/CustomerDto.java", DTO.to_owned()), ("shop/CustomerMapper.java", extra)];
        compile("methods", &common(methods.clone()), true);
        let (host, mut plugin, doc) = setup(MAPPER_PATH, &mapper, "java", &[("src/main/java/shop/Customer.java", &entity), ("src/main/java/shop/CustomerDto.java", DTO), ("src/main/java/shop/EntityMapper.java", &base_file(false))]);
        let pending = action(&mut plugin, &doc, EXTEND_TITLE);
        host.answer_next_prompt(Some(&["0"]));
        host.answer_next_prompt(Some(&[ENTITY_PATH]));
        host.answer_next_prompt(Some(&[DTO_PATH]));
        host.answer_next_prompt(Some(&[]));
        let extended = apply(&doc, &plugin.resolve_file_action(&doc, pending).unwrap());
        let mut with_base = common(extended);
        with_base.push(("shop/EntityMapper.java", base_file(false)));
        compile("extended", &with_base, true);
        let service = inject(&[("src/main/java/shop/CustomerMapper.java", &mapper)], &SERVICE.replace("package shop;", "package other;"), "/project/src/main/java/other/OrderService.java", "java");
        let mut files = common(mapper.clone());
        files.push(("other/OrderService.java", service.unwrap()));
        files.push(("org/springframework/stereotype/Service.java", "package org.springframework.stereotype; public @interface Service {}".into()));
        compile("injected", &files, true);
    }
}
