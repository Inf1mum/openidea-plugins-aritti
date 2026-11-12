//! Live checks that read other saved project files, each only on positive evidence.
//!
//! Every cross-file lookup resolves a class through the current file's imports or package, reads exactly the file the
//! package path names inside the current source root, and requires that file to declare that top-level class. Anything
//! unresolved (wildcard imports, library types, ambiguous or oversized files, XML mappings that may declare the class,
//! incomplete project scans) keeps the rule silent. XML mapping files are read through `orm_xml`.
//!
//! Wired here: the Kotlin build checks, external `@EntityListeners` classes without callbacks, association targets that are
//! not entities, `@AttributeOverride`/`@MapsId`/`@MapKey` names, Spring Data `@EntityGraph` and `@Query(name)` references,
//! `persistence.xml` class entries; new: `jpa-unsupported-type`, `jpa-entity-field-without-relationship`,
//! `jpa-incompatible-table`, `jpa-composite-id-not-serializable`.
use crate::{
    attribute_info, build,
    checks::{self, Finding},
    live, local_hierarchy, local_inheritance,
    model::{self, Annotation, Attribute, Class, Kind, Relation},
    orm_xml::{self, Orm},
    setting,
    xml::{self, Lookup},
};
use openidea_plugin::{Severity, host, providers::Document, text::offset};

const MAX_HOPS: usize = 8;
const MAX_TEXT: usize = 128 * 1024;
const MAX_SOURCES: u32 = 128;

fn statements(text: &str, keyword: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.trim().strip_prefix(keyword))
        .map(|r| r.trim().trim_end_matches(';').trim().to_owned())
        .collect()
}

fn identifier(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && name.bytes().enumerate().all(|(i, b)| model::is_ident(b) && !(i == 0 && b.is_ascii_digit()))
}

fn source_root(path: &str, package: &str) -> Option<String> {
    let directory = path.rsplit_once('/')?.0;
    directory.strip_suffix(&format!("/{}", package.replace('.', "/"))).map(str::to_owned)
}

fn file_of(root: &str, fqcn: &str, extension: &str) -> String {
    format!("{root}/{}.{extension}", fqcn.replace('.', "/"))
}

fn resolve_in(package: &str, imports: &[String], name: &str) -> Option<String> {
    if !identifier(name) || imports.iter().any(|i| i.ends_with(".*")) {
        return None;
    }
    let suffix = format!(".{name}");
    let mut imported = imports.iter().filter(|i| !i.starts_with("static ") && i.ends_with(&suffix));
    match (imported.next(), imported.next()) {
        (Some(_), Some(_)) => None,
        (Some(import), None) => Some(import.clone()),
        (None, _) => Some(format!("{package}.{name}")),
    }
}

fn read_class(root: &str, extension: &str, fqcn: &str) -> Option<(Class, String)> {
    let path = file_of(root, fqcn, extension);
    let text = local_hierarchy::read(&path)?;
    if !live::trusted_annotations(&text) {
        return None;
    }
    let class = model::parse_class(&text, &path)?;
    let (package, name) = fqcn.rsplit_once('.')?;
    (class.package.as_deref() == Some(package) && class.name == name && live::top_level_identity(&text, &class)).then_some((class, text))
}

fn member_ranges(masked: &str, class: &Class) -> Vec<(usize, usize)> {
    let Some((class_at, _)) = model::declaration_keyword(masked, 0, &["class"]) else { return vec![] };
    let (Some(body_end), Some(open)) = (class.body_end, masked[class_at..].find('{').map(|p| p + class_at)) else { return vec![] };
    let mut ranges = vec![];
    if class.kotlin
        && let Some(paren) = masked[class_at..open].find('(').map(|p| p + class_at)
        && let Some(close) = model::closing(masked.as_bytes(), paren)
    {
        ranges.extend(model::members(masked, paren + 1, close, true, true));
    }
    ranges.extend(model::members(masked, open + 1, body_end, class.kotlin, false));
    ranges.truncate(256);
    ranges
}

fn orm_unresolved(path: &str, fqcn: &str) -> bool {
    orm_xml::load(path).may_declare(fqcn)
}

pub(super) fn findings(doc: &Document) -> Vec<Finding> {
    if doc.text.len() > MAX_TEXT {
        return vec![];
    }
    let masked = model::mask(&doc.text);
    let mut found = vec![];
    if let Some(class) = model::parse_class(&doc.text, &doc.path)
        && live::top_level_identity(&doc.text, &class)
    {
        found.extend(kotlin_build(doc, &class, &masked));
        found.extend(listeners(doc, &class));
        found.extend(association_targets(doc, &class, &masked));
        if doc.language == "java" {
            found.extend(mapping_names(doc));
            found.extend(field_types(doc));
            found.extend(incompatible_table(doc));
            found.extend(composite_id(doc));
        }
    }
    if doc.language == "java" {
        found.extend(repository_references(doc, &masked));
    }
    found
}

fn kotlin_build(doc: &Document, class: &Class, masked: &str) -> Vec<Finding> {
    if !class.kotlin || class.kind != Kind::Entity || !setting("checkKotlinEntities") || model::words(masked, "class").count() != 1 {
        return vec![];
    }
    let Some(project) = host::project_of(&doc.path) else { return vec![] };
    let Ok(root) = project.root() else { return vec![] };
    let mut files = vec![];
    let mut directory = doc.path.rsplit_once('/').map_or("", |(d, _)| d).to_owned();
    for _ in 0..16 {
        if !directory.starts_with(&root) {
            break;
        }
        for name in build::BUILD_FILES {
            let candidate = format!("{directory}/{name}");
            if let Ok(text) = project.read_file(&candidate) {
                files.push((candidate, text));
            }
        }
        match directory.rsplit_once('/') {
            Some((parent, _)) if directory != root => directory = parent.to_owned(),
            _ => break,
        }
    }
    checks::inspect_kotlin_build(&doc.text, &build::analyse(&files))
        .into_iter()
        .filter(|f| matches!(f.code, "jpa-kotlin-plugin-missing" | "jpa-redundant-open" | "jpa-redundant-constructor"))
        .collect()
}

fn listeners(doc: &Document, class: &Class) -> Vec<Finding> {
    if !setting("checkListenerUsage") || !matches!(class.kind, Kind::Entity | Kind::MappedSuperclass) || !doc.text.contains("EntityListeners") {
        return vec![];
    }
    let references = checks::listener_references(&doc.text);
    let (Some(package), false) = (class.package.as_deref(), references.is_empty()) else { return vec![] };
    let Some(root) = source_root(&class.path, package) else { return vec![] };
    if !matches!(orm_xml::load(&doc.path), Orm::Absent) {
        return vec![];
    }
    let extension = if class.kotlin { "kt" } else { "java" };
    let mut found = vec![];
    for (name, start, end) in references.into_iter().take(16) {
        if doc.text[..start].trim_end().ends_with('.') {
            continue;
        }
        let Some(fqcn) = resolve_in(package, &class.imports, &name) else { continue };
        let Some(text) = local_hierarchy::read(&file_of(&root, &fqcn, extension)) else { continue };
        if local_hierarchy::converter_head(&text, &fqcn).is_some() && checks::is_empty_listener(&text, &name, class.kotlin) {
            found.push(checks::unused_listener(start, end, &name));
        }
    }
    found
}

fn association_targets(doc: &Document, class: &Class, masked: &str) -> Vec<Finding> {
    if class.kind != Kind::Entity || !setting("checkAttributeSignatures") {
        return vec![];
    }
    let associations: Vec<&Attribute> = class
        .attributes
        .iter()
        .filter(|a| matches!(a.relation, Relation::ManyToOne | Relation::OneToOne | Relation::OneToMany | Relation::ManyToMany))
        .take(64)
        .collect();
    let Some(package) = class.package.as_deref().filter(|_| !associations.is_empty()) else { return vec![] };
    let Some(root) = source_root(&class.path, package) else { return vec![] };
    if !matches!(orm_xml::load(&doc.path), Orm::Absent) {
        return vec![];
    }
    let extension = if class.kotlin { "kt" } else { "java" };
    let ranges = member_ranges(masked, class);
    let mut found = vec![];
    for attribute in associations {
        let Some(start) = offset(&doc.text, attribute.pos) else { continue };
        if ranges.iter().any(|&(from, to)| from <= start && start < to && model::has_word(&masked[from..to], "targetEntity")) {
            continue;
        }
        let Some(fqcn) = resolve_in(package, &class.imports, &attribute.target) else { continue };
        let simple = &attribute.target;
        let path = file_of(&root, &fqcn, extension);
        let Some(text) = local_hierarchy::read(&path) else { continue };
        if !live::trusted_annotations(&text) {
            continue;
        }
        let described = match read_class(&root, extension, &fqcn) {
            Some((target, _)) => match target.kind {
                Kind::Entity => continue,
                Kind::Embeddable => format!("{simple} is an @Embeddable"),
                Kind::MappedSuperclass => format!("{simple} is a @MappedSuperclass"),
            },
            None if text.contains("Entity") || local_hierarchy::converter_head(&text, &fqcn).is_none() => continue,
            None => format!("{simple} is not annotated @Entity"),
        };
        found.push(Finding::new(
            start,
            start + attribute.name.len(),
            Severity::Info,
            "jpa-relationship-target",
            format!("{described}, so it can't be the target of an association. Make it an entity."),
        ));
    }
    found
}

fn attribute_names(path: &str, package: &str, imports: &[String], owner: &str, orm: &Orm) -> Option<Vec<String>> {
    let root = source_root(path, package)?;
    let mut current = resolve_in(package, imports, owner)?;
    let mut names = vec![];
    for _ in 0..MAX_HOPS {
        if orm.may_declare(&current) {
            return None;
        }
        let ancestor_path = file_of(&root, &current, "java");
        let text = local_hierarchy::read(&ancestor_path)?;
        let up = attribute_info::snapshot(&Document { path: ancestor_path, language: "java".into(), text: text.clone(), version: 0 })?;
        if format!("{}.{}", up.package.as_deref()?, up.name) != current {
            return None;
        }
        names.extend(up.attributes.iter().map(|a| a.name.clone()));
        let masked = model::mask(&text);
        let head = local_hierarchy::header(&masked, &up)?;
        if !model::has_word(head, "extends") {
            return Some(names);
        }
        current = resolve_in(up.package.as_deref()?, &up.imports, local_hierarchy::extended(head)?)?;
    }
    None
}

fn mapping_names(doc: &Document) -> Vec<Finding> {
    if !setting("checkMappingNames") || !["AttributeOverride", "MapsId", "MapKey"].iter().any(|n| doc.text.contains(n)) {
        return vec![];
    }
    let Some(class) = attribute_info::snapshot(doc) else { return vec![] };
    let references: Vec<_> = checks::name_references(&doc.text, false, &class)
        .into_iter()
        .filter(|r| matches!(r.code, "jpa-unknown-attribute-override" | "jpa-unknown-maps-id" | "jpa-unknown-map-key"))
        .collect();
    let Some(package) = class.package.as_deref().filter(|_| !references.is_empty()) else { return vec![] };
    let orm = orm_xml::load(&doc.path);
    let names = |owner: &str| attribute_names(&class.path, package, &class.imports, owner, &orm);
    checks::inspect_name_references(&references, &names)
}

const NOT_BASIC_EXEMPT: &[&str] = &[
    "ManyToOne", "OneToOne", "OneToMany", "ManyToMany", "EmbeddedId", "Embedded", "Lob", "Transient", "Convert", "ElementCollection", "Id", "MapsId", "Basic",
];
fn project_texts(path: &str) -> Option<Vec<String>> {
    let project = host::project_of(path)?;
    let java = project.find_files("**/*.java", MAX_SOURCES + 1).ok()?;
    let kotlin = project.find_files("**/*.kt", MAX_SOURCES + 1).ok()?;
    if java.len() > MAX_SOURCES as usize || kotlin.len() > MAX_SOURCES as usize {
        return None;
    }
    java.iter().chain(kotlin.iter()).map(|p| local_hierarchy::read(p)).collect()
}

fn field_types(doc: &Document) -> Vec<Finding> {
    let (unsupported, entity_valued) = (setting("checkUnsupportedFieldType"), setting("checkEntityFieldRelationship"));
    if !(unsupported || entity_valued) {
        return vec![];
    }
    let Some(class) = attribute_info::snapshot(doc) else { return vec![] };
    let Some(package) = class.package.as_deref() else { return vec![] };
    let Some(root) = source_root(&class.path, package) else { return vec![] };
    let masked = model::mask(&doc.text);
    let annotations = model::annotations(&masked);
    let ranges = member_ranges(&masked, &class);
    let mut fields = vec![];
    for &(start, end) in &ranges {
        let decl = model::without_annotations(&masked, start, end);
        let own: Vec<&Annotation> = annotations.iter().filter(|a| start <= a.start && a.start < end).collect();
        if decl.split('=').next().unwrap_or("").contains('(') {
            if own.is_empty() {
                continue;
            }
            return vec![];
        }
        fields.push((start, decl, own));
    }
    if !matches!(orm_xml::load(&doc.path), Orm::Absent) {
        return vec![];
    }
    let mut texts: Option<Option<Vec<String>>> = None;
    let mut found = vec![];
    for (start, decl, own) in fields {
        let head = decl.split('=').next().unwrap_or("");
        if model::has_word(head, "static") || model::has_word(head, "transient") || own.iter().any(|a| a.name == "Transient") {
            continue;
        }
        let Some((_, _, ty)) = model::declaration(&decl, false) else { continue };
        if !identifier(&ty) || !ty.starts_with(|c: char| c.is_ascii_uppercase()) {
            continue;
        }
        let Some(fqcn) = resolve_in(package, &class.imports, &ty) else { continue };
        let Some(at) = model::words(&decl, &ty).next() else { continue };
        let range = (start + at, start + at + ty.len());
        let has = |names: &[&str]| own.iter().any(|a| names.contains(&a.name));
        if entity_valued && !has(NOT_BASIC_EXEMPT) {
            if let Some((target, _)) = read_class(&root, "java", &fqcn) {
                if target.kind == Kind::Entity {
                    found.push(Finding::new(
                        range.0,
                        range.1,
                        Severity::Warning,
                        "jpa-entity-field-without-relationship",
                        format!("{ty} is an entity, but this attribute carries no @ManyToOne, @OneToOne, @OneToMany or @ManyToMany, so it cannot be mapped as a basic attribute."),
                    ));
                }
                continue;
            }
        }
        if !unsupported || has(NOT_BASIC_EXEMPT) {
            continue;
        }
        let Some(text) = local_hierarchy::read(&file_of(&root, &fqcn, "java")) else { continue };
        let plain = live::trusted_annotations(&text)
            && local_hierarchy::converter_head(&text, &fqcn).is_some_and(|head| head.is_empty())
            && !model::annotations(&model::mask(&text)).iter().any(|a| matches!(a.name, "Entity" | "Embeddable" | "MappedSuperclass"));
        if !plain {
            continue;
        }
        let scanned = texts.get_or_insert_with(|| project_texts(&doc.path));
        let Some(scanned) = scanned else { continue };
        if scanned.iter().any(|t| t.contains("AttributeConverter") && model::has_word(&model::mask(t), &ty)) {
            continue;
        }
        found.push(Finding::new(
            range.0,
            range.1,
            Severity::Warning,
            "jpa-unsupported-type",
            format!("{ty} is not a basic type, an embeddable, an entity or Serializable, and no converter is declared for it. Add an AttributeConverter, mark the attribute transient or map it."),
        ));
    }
    found
}

fn incompatible_table(doc: &Document) -> Vec<Finding> {
    if !setting("checkIncompatibleTable") || !doc.text.contains("Table") {
        return vec![];
    }
    let Some(class) = attribute_info::snapshot(doc) else { return vec![] };
    if class.kind != Kind::Entity || class.superclass.is_none() {
        return vec![];
    }
    let masked = model::mask(&doc.text);
    let all = model::annotations(&masked);
    let Some(own) = local_inheritance::class_annotations(&masked, &all) else { return vec![] };
    let Some(table) = own.iter().find(|a| a.name == "Table") else { return vec![] };
    if !matches!(orm_xml::load(&doc.path), Orm::Absent) || local_inheritance::root_strategy(doc, &local_hierarchy::read).as_deref() != Some("SINGLE_TABLE") {
        return vec![];
    }
    vec![Finding::new(
        table.start,
        table.end(),
        Severity::Warning,
        "jpa-incompatible-table",
        "This entity extends the root of a SINGLE_TABLE hierarchy, which stores every subclass in the root's table, so @Table has no effect here. Remove it.",
    )]
}

fn composite_id(doc: &Document) -> Vec<Finding> {
    if !setting("checkCompositeIdSerializable") || !doc.text.contains("EmbeddedId") {
        return vec![];
    }
    let Some(class) = attribute_info::snapshot(doc) else { return vec![] };
    if !matches!(class.kind, Kind::Entity | Kind::MappedSuperclass) || !class.imports.iter().any(|i| i.starts_with("javax.persistence.")) {
        return vec![];
    }
    let Some(package) = class.package.as_deref() else { return vec![] };
    let Some(root) = source_root(&class.path, package) else { return vec![] };
    let masked = model::mask(&doc.text);
    let annotations = model::annotations(&masked);
    let mut found = vec![];
    for (start, end) in member_ranges(&masked, &class) {
        if !annotations.iter().any(|a| a.name == "EmbeddedId" && start <= a.start && a.start < end) {
            continue;
        }
        let decl = model::without_annotations(&masked, start, end);
        let Some((_, _, ty)) = model::declaration(&decl, false) else { continue };
        let Some(fqcn) = resolve_in(package, &class.imports, &ty) else { continue };
        let Some((id, text)) = read_class(&root, "java", &fqcn) else { continue };
        let id_masked = model::mask(&text);
        let serializable = local_hierarchy::header(&id_masked, &id).is_none_or(|head| !head.is_empty());
        if id.kind != Kind::Embeddable || serializable || orm_unresolved(&doc.path, &fqcn) {
            continue;
        }
        let Some(at) = model::words(&decl, &ty).next() else { continue };
        found.push(Finding::new(
            start + at,
            start + at + ty.len(),
            Severity::Error,
            "jpa-composite-id-not-serializable",
            format!("The composite id class {ty} does not implement java.io.Serializable, which a composite primary key class must (Jakarta Persistence 2.x section 2.4)."),
        ));
    }
    found
}

struct Declared {
    queries: Vec<String>,
    graphs: Vec<String>,
}

fn declared_names(path: &str) -> Option<Declared> {
    let project = host::project_of(path)?;
    if !matches!(orm_xml::load(path), Orm::Absent) || !project.find_files("**/jpa-named-queries.properties", 1).ok()?.is_empty() {
        return None;
    }
    let mut declared = Declared { queries: vec![], graphs: vec![] };
    for source in project_sources(&project)? {
        let (file, text) = source;
        if !["NamedQuery", "NamedNativeQuery", "NamedStoredProcedureQuery", "NamedEntityGraph"].iter().any(|n| text.contains(n)) {
            continue;
        }
        let kotlin = file.ends_with(".kt");
        declared.queries.extend(checks::declared_named_queries(&text, kotlin)?);
        declared.graphs.extend(checks::declared_entity_graphs(&text, kotlin)?);
    }
    Some(declared)
}

fn project_sources(project: &openidea_plugin::Project) -> Option<Vec<(String, String)>> {
    let java = project.find_files("**/*.java", MAX_SOURCES + 1).ok()?;
    let kotlin = project.find_files("**/*.kt", MAX_SOURCES + 1).ok()?;
    if java.len() > MAX_SOURCES as usize || kotlin.len() > MAX_SOURCES as usize {
        return None;
    }
    java.into_iter().chain(kotlin).map(|p| Some((p.clone(), project.read_file(&p).ok().filter(|t| t.len() <= MAX_TEXT)?))).collect()
}

const SPRING_BASES: &[(&str, &str)] = &[
    ("JpaRepository", "org.springframework.data.jpa.repository.JpaRepository"),
    ("CrudRepository", "org.springframework.data.repository.CrudRepository"),
    ("ListCrudRepository", "org.springframework.data.repository.ListCrudRepository"),
    ("PagingAndSortingRepository", "org.springframework.data.repository.PagingAndSortingRepository"),
    ("Repository", "org.springframework.data.repository.Repository"),
];

fn repository_references(doc: &Document, masked: &str) -> Vec<Finding> {
    let (graphs, queries) = (setting("checkEntityGraphs"), setting("checkQueryReferences"));
    if !(graphs || queries) || !(doc.text.contains("@EntityGraph") || doc.text.contains("@Query")) {
        return vec![];
    }
    if model::words(masked, "interface").count() != 1 || model::words(masked, "class").next().is_some() {
        return vec![];
    }
    let (package, imports) = (statements(&doc.text, "package ").into_iter().next(), statements(&doc.text, "import "));
    let Some(package) = package else { return vec![] };
    if imports.iter().any(|i| i.ends_with(".*")) {
        return vec![];
    }
    let Some((at, _)) = model::declaration_keyword(masked, 0, &["interface"]) else { return vec![] };
    let Some(open) = masked[at..].find('{').map(|p| p + at) else { return vec![] };
    let Some((_, supertype)) = masked[at + "interface".len()..open].split_once("extends") else { return vec![] };
    let Some((base, arguments)) = supertype.trim().split_once('<') else { return vec![] };
    let imported = |simple: &str| {
        let mut found = imports.iter().filter(|i| i.rsplit('.').next() == Some(simple));
        match (found.next(), found.next()) {
            (Some(i), None) => Some(i.as_str()),
            _ => None,
        }
    };
    if !SPRING_BASES.iter().any(|(simple, full)| base.trim() == *simple && imported(simple) == Some(*full)) {
        return vec![];
    }
    let Some((entity, _)) = arguments.trim().strip_suffix('>').and_then(|a| a.split_once(',')) else { return vec![] };
    let entity = entity.trim();
    let Some(entity_fqcn) = resolve_in(&package, &imports, entity) else { return vec![] };
    let Some(root) = source_root(&doc.path, &package) else { return vec![] };
    if read_class(&root, "java", &entity_fqcn).is_none_or(|(c, _)| c.kind != Kind::Entity) {
        return vec![];
    }
    let all = model::annotations(masked);
    let mut found = vec![];
    let spring_graph = imported("EntityGraph") == Some("org.springframework.data.jpa.repository.EntityGraph");
    if graphs && spring_graph {
        let references: Vec<_> = all
            .iter()
            .filter(|a| a.name == "EntityGraph")
            .filter_map(|a| model::value_range(&doc.text, masked, a, "value").and_then(|r| model::plain_literal(&doc.text, r, false)))
            .filter(|(_, name)| !name.is_empty())
            .map(|(at, name)| (name.to_owned(), at))
            .collect();
        if !references.is_empty()
            && let Some(declared) = declared_names(&doc.path)
        {
            found.extend(
                references
                    .iter()
                    .filter(|(name, _)| !declared.graphs.contains(name))
                    .map(|(name, at)| checks::unknown_entity_graph(name, *at, at + name.len())),
            );
        }
        let paths = checks::entity_graph_paths(&doc.text, false, entity);
        if !paths.is_empty() {
            let orm = orm_xml::load(&doc.path);
            let names = |owner: &str| attribute_names(&doc.path, &package, &imports, owner, &orm);
            found.extend(checks::inspect_name_references(&paths, &names));
        }
    }
    if queries {
        let references: Vec<_> = all
            .iter()
            .filter(|a| a.name == "Query")
            .filter_map(|a| model::value_range(&doc.text, masked, a, "name").and_then(|r| model::plain_literal(&doc.text, r, false)))
            .map(|(at, name)| (name.to_owned(), at))
            .collect();
        if !references.is_empty()
            && let Some(declared) = declared_names(&doc.path)
        {
            found.extend(
                references
                    .iter()
                    .filter(|(name, _)| !declared.queries.contains(name))
                    .map(|(name, at)| checks::unknown_named_query(name, *at, at + name.len())),
            );
        }
    }
    found
}

const XML_MARKERS: &[&str] = &["@Entity", "@MappedSuperclass", "@Embeddable", "@Converter"];

fn xml_lookup(fqcn: &str, path: &str) -> Lookup {
    let Some((package, _)) = fqcn.rsplit_once('.').filter(|_| !fqcn.contains('$')) else { return Lookup::Unknown };
    let Some(project) = host::project_of(path) else { return Lookup::Unknown };
    let relative = fqcn.replace('.', "/");
    let mut files = vec![];
    for extension in ["java", "kt"] {
        let Ok(found) = project.find_files(&format!("**/{relative}.{extension}"), 2) else { return Lookup::Unknown };
        files.extend(found);
    }
    match files.as_slice() {
        [] => {
            let directory = package.replace('.', "/");
            let owned = ["java", "kt"].iter().any(|e| project.find_files(&format!("**/{directory}/*.{e}"), 1).is_ok_and(|f| !f.is_empty()));
            if owned { Lookup::NotInProject } else { Lookup::Unknown }
        }
        [file] => match local_hierarchy::read(file) {
            Some(text) if statements(&text, "package ").first().map(String::as_str) == Some(package) => {
                Lookup::Found { annotated: XML_MARKERS.iter().any(|m| text.contains(m)) }
            }
            _ => Lookup::Unknown,
        },
        _ => Lookup::Unknown,
    }
}

pub(super) fn xml_findings(doc: &Document) -> Vec<Finding> {
    if !doc.text.contains("<class") && !doc.text.contains("<entity") && !doc.text.contains("<mapped-superclass") && !doc.text.contains("<embeddable") {
        return xml::inspect_owned(&doc.text);
    }
    let orm_present = !matches!(orm_xml::load(&doc.path), Orm::Absent);
    let looked = std::cell::Cell::new(0);
    xml::inspect_owned_with(&doc.text, orm_present, &|fqcn| {
        looked.set(looked.get() + 1);
        if looked.get() > 64 { Lookup::Unknown } else { xml_lookup(fqcn, &doc.path) }
    })
}

#[cfg(test)]
mod tests {
    use crate::JpaAssist;
    use openidea_plugin::{Severity, diagnostics::DiagnosticsProvider, mock::MockHost, providers::Document};

    const NS: &str = "xmlns=\"https://jakarta.ee/xml/ns/persistence\"";

    fn imports(names: &[&str]) -> String {
        let lines: String = names.iter().map(|n| format!("import jakarta.persistence.{n};\n")).collect();
        format!("package shop;\n{lines}")
    }

    fn run(host: &MockHost, path: &str, text: &str, code: &str) -> Vec<(Severity, String)> {
        host.add_file(path, text);
        let language = if path.ends_with(".kt") { "kotlin" } else if path.ends_with(".xml") { "xml" } else { "java" };
        let doc = Document { path: format!("/project/{path}"), language: language.into(), text: text.into(), version: 1 };
        JpaAssist::default().diagnostics(&doc).into_iter().filter(|d| d.code.as_deref() == Some(code)).map(|d| (d.severity, d.message)).collect()
    }

    fn entity(extra: &[&str], body: &str) -> String {
        let mut names = vec!["Entity", "Id"];
        names.extend_from_slice(extra);
        format!("{}@Entity\npublic class E {{\n    @Id Long id;\n{body}\n}}\n", imports(&names))
    }

    const E: &str = "src/main/java/shop/E.java";

    fn kotlin(build: &str, text: &str) -> MockHost {
        let host = MockHost::install();
        host.add_file("build.gradle.kts", build);
        host.add_file("src/main/kotlin/shop/K.kt", text);
        host
    }

    const KOTLIN_HEAD: &str = "package shop\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\n";
    const PLAIN: &str = "plugins {\n    kotlin(\"jvm\") version \"2.0.21\"\n}\n";
    const WITH_JPA: &str = "plugins {\n    kotlin(\"jvm\") version \"2.0.21\"\n    kotlin(\"plugin.jpa\") version \"2.0.21\"\n}\n";

    #[test]
    fn kotlin_entity_without_the_jpa_plugin_is_noted_and_only_for_literal_builds() {
        let text = format!("{KOTLIN_HEAD}@Entity\nclass K(\n  @Id var id: Long?,\n  var name: String,\n)\n");
        let host = kotlin(PLAIN, &text);
        let found = run(&host, "src/main/kotlin/shop/K.kt", &text, "jpa-kotlin-plugin-missing");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, Severity::Info);
        assert!(run(&kotlin(WITH_JPA, &text), "src/main/kotlin/shop/K.kt", &text, "jpa-kotlin-plugin-missing").is_empty());
        assert!(run(&kotlin("plugins {\n    alias(libs.plugins.kotlin.jvm)\n}\n", &text), "src/main/kotlin/shop/K.kt", &text, "jpa-kotlin-plugin-missing").is_empty());
        let defaults = format!("{KOTLIN_HEAD}@Entity\nclass K(@Id var id: Long? = null, var name: String = \"\")\n");
        assert!(run(&kotlin(PLAIN, &defaults), "src/main/kotlin/shop/K.kt", &defaults, "jpa-kotlin-plugin-missing").is_empty());
        let unowned = text.replace("import jakarta.persistence.Entity\n", "import vendor.Entity\n");
        assert!(run(&kotlin(PLAIN, &unowned), "src/main/kotlin/shop/K.kt", &unowned, "jpa-kotlin-plugin-missing").is_empty());
        let off = kotlin(PLAIN, &text);
        off.set_setting("jpaAssist.checkKotlinEntities", "false");
        assert!(run(&off, "src/main/kotlin/shop/K.kt", &text, "jpa-kotlin-plugin-missing").is_empty());
    }

    #[test]
    fn redundant_kotlin_constructor_needs_the_plugin_named_in_the_build() {
        let text = format!("{KOTLIN_HEAD}@Entity\nclass K(@Id var id: Long?) {{\n    constructor() {{}}\n\n    var name: String = \"\"\n}}\n");
        assert_eq!(run(&kotlin(WITH_JPA, &text), "src/main/kotlin/shop/K.kt", &text, "jpa-redundant-constructor").len(), 1);
        assert!(run(&kotlin(PLAIN, &text), "src/main/kotlin/shop/K.kt", &text, "jpa-redundant-constructor").is_empty());
        let delegating = format!("{KOTLIN_HEAD}@Entity\nclass K(@Id var id: Long?) {{\n    constructor() : this(null)\n}}\n");
        assert!(run(&kotlin(WITH_JPA, &delegating), "src/main/kotlin/shop/K.kt", &delegating, "jpa-redundant-constructor").is_empty());
    }

    #[test]
    fn redundant_open_needs_an_allopen_entity_configuration() {
        let text = format!("{KOTLIN_HEAD}@Entity\nopen class K(@Id var id: Long? = null)\n");
        let open = "plugins { kotlin(\"plugin.jpa\") }\nallOpen {\n  annotation(\"jakarta.persistence.Entity\")\n}\n";
        assert_eq!(run(&kotlin(open, &text), "src/main/kotlin/shop/K.kt", &text, "jpa-redundant-open").len(), 1);
        assert!(run(&kotlin("plugins { kotlin(\"plugin.jpa\") }\n", &text), "src/main/kotlin/shop/K.kt", &text, "jpa-redundant-open").is_empty());
        assert!(run(&kotlin("allOpen {\n  annotation(\"x.Y\")\n}\n", &text), "src/main/kotlin/shop/K.kt", &text, "jpa-redundant-open").is_empty());
    }

    fn listened(listener: &str) -> MockHost {
        let host = MockHost::install();
        host.add_file("src/main/java/shop/AuditListener.java", listener);
        host
    }

    fn listening_entity() -> String {
        entity(&["EntityListeners"], "").replace("@Entity\n", "@Entity\n@EntityListeners(AuditListener.class)\n")
    }

    #[test]
    fn external_listener_without_callbacks_warns_on_the_reference() {
        let host = listened("package shop;\nclass AuditListener {\n  void touch() {}\n}\n");
        let text = listening_entity();
        let found = run(&host, E, &text, "jpa-listener-without-callbacks");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, Severity::Warning);
        let callbacks = listened("package shop;\nimport jakarta.persistence.PrePersist;\nclass AuditListener {\n  @PrePersist void touch() {}\n}\n");
        assert!(run(&callbacks, E, &text, "jpa-listener-without-callbacks").is_empty());
        let inherited = listened("package shop;\nclass AuditListener extends Base {\n}\n");
        assert!(run(&inherited, E, &text, "jpa-listener-without-callbacks").is_empty());
    }

    #[test]
    fn external_listener_stays_silent_when_unknown_or_xml_may_map_it() {
        let host = MockHost::install();
        let text = listening_entity();
        assert!(run(&host, E, &text, "jpa-listener-without-callbacks").is_empty());
        let wildcard = listened("package shop;\nclass AuditListener {\n}\n");
        let other = text.replace("import jakarta.persistence.Id;\n", "import jakarta.persistence.Id;\nimport vendor.*;\n");
        assert!(run(&wildcard, E, &other, "jpa-listener-without-callbacks").is_empty());
        let elsewhere = listened("package other;\nclass AuditListener {\n}\n");
        assert!(run(&elsewhere, E, &text, "jpa-listener-without-callbacks").is_empty());
        let mapped = listened("package shop;\nclass AuditListener {\n}\n");
        mapped.add_file("src/main/resources/META-INF/orm.xml", "<entity-mappings/>");
        assert!(run(&mapped, E, &text, "jpa-listener-without-callbacks").is_empty());
        let off = listened("package shop;\nclass AuditListener {\n}\n");
        off.set_setting("jpaAssist.checkListenerUsage", "false");
        assert!(run(&off, E, &text, "jpa-listener-without-callbacks").is_empty());
    }

    fn target(kind: &str, body: &str) -> (MockHost, String) {
        let host = MockHost::install();
        let head = if kind.is_empty() { "package shop;\n".to_owned() } else { imports(&["Entity", "Embeddable", "MappedSuperclass", "Id"]) };
        host.add_file("src/main/java/shop/Other.java", &format!("{head}{kind}\nclass Other {{\n{body}\n}}\n"));
        (host, entity(&["ManyToOne"], "    @ManyToOne Other other;"))
    }

    #[test]
    fn association_target_that_is_not_an_entity_is_noted() {
        for (kind, expected) in [("", "is not annotated @Entity"), ("@Embeddable", "@Embeddable"), ("@MappedSuperclass", "@MappedSuperclass")] {
            let (host, text) = target(kind, "    int x;");
            let found = run(&host, E, &text, "jpa-relationship-target");
            assert_eq!(found.len(), 1, "{kind}");
            assert_eq!(found[0].0, Severity::Info);
            assert!(found[0].1.contains(expected), "{}", found[0].1);
        }
    }

    #[test]
    fn association_target_is_quiet_for_entities_unknown_and_unresolved_types() {
        let (host, text) = target("@Entity", "    @Id Long id;");
        assert!(run(&host, E, &text, "jpa-relationship-target").is_empty());
        let (host, text) = target("", "    int x;");
        let wildcard = text.replace("import jakarta.persistence.Id;\n", "import jakarta.persistence.Id;\nimport vendor.*;\n");
        assert!(run(&host, E, &wildcard, "jpa-relationship-target").is_empty());
        let targeted = text.replace("@ManyToOne Other other;", "@ManyToOne(targetEntity = Real.class) Other other;");
        assert!(run(&host, E, &targeted, "jpa-relationship-target").is_empty());
        host.add_file("src/main/resources/META-INF/orm.xml", "<entity-mappings/>");
        assert!(run(&host, E, &text, "jpa-relationship-target").is_empty());
        let missing = MockHost::install();
        assert!(run(&missing, E, &text, "jpa-relationship-target").is_empty());
    }

    fn address(body: &str) -> MockHost {
        let host = MockHost::install();
        host.add_file("src/main/java/shop/Address.java", &format!("{}@Embeddable\nclass Address {{\n{body}\n}}\n", imports(&["Embeddable"])));
        host
    }

    fn overriding(name: &str) -> String {
        entity(&["Embedded", "AttributeOverride", "Column"], &format!("    @Embedded @AttributeOverride(name = \"{name}\", column = @Column(name = \"c\")) Address address;"))
    }

    #[test]
    fn attribute_override_naming_a_missing_embeddable_attribute_warns() {
        let host = address("    String street;");
        let found = run(&host, E, &overriding("missing"), "jpa-unknown-attribute-override");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, Severity::Warning);
        assert!(run(&host, E, &overriding("street"), "jpa-unknown-attribute-override").is_empty());
        let dotted = overriding("street.deeper");
        assert!(run(&host, E, &dotted, "jpa-unknown-attribute-override").is_empty());
    }

    #[test]
    fn attribute_override_is_silent_when_the_owner_is_not_provable() {
        let host = MockHost::install();
        assert!(run(&host, E, &overriding("missing"), "jpa-unknown-attribute-override").is_empty());
        let inherited = address("    String street;").tap_extends();
        assert!(run(&inherited, E, &overriding("missing"), "jpa-unknown-attribute-override").is_empty());
        let mapped = address("    String street;");
        mapped.add_file("src/main/resources/META-INF/orm.xml", "<entity-mappings><embeddable class=\"shop.Address\"/></entity-mappings>");
        assert!(run(&mapped, E, &overriding("missing"), "jpa-unknown-attribute-override").is_empty());
        let off = address("    String street;");
        off.set_setting("jpaAssist.checkMappingNames", "false");
        assert!(run(&off, E, &overriding("missing"), "jpa-unknown-attribute-override").is_empty());
    }

    trait Tap {
        fn tap_extends(self) -> Self;
    }

    impl Tap for MockHost {
        fn tap_extends(self) -> Self {
            self.add_file("src/main/java/shop/Address.java", &format!("{}@Embeddable\nclass Address extends Base {{\n    String street;\n}}\n", imports(&["Embeddable"])));
            self
        }
    }

    #[test]
    fn map_key_names_the_target_entity_attributes() {
        let host = MockHost::install();
        host.add_file("src/main/java/shop/Item.java", &format!("{}@Entity\nclass Item {{\n    @Id Long id;\n    String sku;\n}}\n", imports(&["Entity", "Id"])));
        let owner = |name: &str| {
            let text = entity(&["OneToMany", "MapKey"], &format!("    @OneToMany @MapKey(name = \"{name}\") java.util.Map<String, Item> items;"));
            text
        };
        assert_eq!(run(&host, E, &owner("nope"), "jpa-unknown-map-key").len(), 1);
        assert!(run(&host, E, &owner("sku"), "jpa-unknown-map-key").is_empty());
        let none = MockHost::install();
        assert!(run(&none, E, &owner("nope"), "jpa-unknown-map-key").is_empty());
    }

    #[test]
    fn maps_id_names_an_attribute_of_the_embedded_id() {
        let host = MockHost::install();
        host.add_file("src/main/java/shop/Key.java", &format!("{}@Embeddable\nclass Key {{\n    Long ownerId;\n}}\n", imports(&["Embeddable"])));
        let owner = |name: &str| {
            format!(
                "{}@Entity\npublic class E {{\n    @EmbeddedId Key id;\n    @MapsId(\"{name}\") @OneToOne Other other;\n}}\n",
                imports(&["Entity", "EmbeddedId", "MapsId", "OneToOne"])
            )
        };
        assert_eq!(run(&host, E, &owner("missing"), "jpa-unknown-maps-id").len(), 1);
        assert!(run(&host, E, &owner("ownerId"), "jpa-unknown-maps-id").is_empty());
        assert!(run(&MockHost::install(), E, &owner("missing"), "jpa-unknown-maps-id").is_empty());
    }

    const REPO: &str = "src/main/java/shop/ERepo.java";

    fn repository(body: &str) -> String {
        format!(
            "package shop;\nimport java.util.List;\nimport org.springframework.data.jpa.repository.EntityGraph;\nimport org.springframework.data.jpa.repository.JpaRepository;\nimport org.springframework.data.jpa.repository.Query;\ninterface ERepo extends JpaRepository<E, Long> {{\n{body}\n}}\n"
        )
    }

    fn with_entity(extra: &str) -> MockHost {
        let host = MockHost::install();
        host.add_file(
            E,
            &format!(
                "{}{extra}@Entity\npublic class E {{\n    @Id Long id;\n    String name;\n}}\n",
                imports(&["Entity", "Id", "NamedQuery", "NamedEntityGraph"])
            ),
        );
        host
    }

    #[test]
    fn unknown_entity_graph_name_warns_only_with_a_complete_project_scan() {
        let host = with_entity("@NamedEntityGraph(name = \"summary\")\n");
        let missing = repository("    @EntityGraph(\"missing\") List<E> a();");
        let found = run(&host, REPO, &missing, "jpa-unknown-entity-graph");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, Severity::Warning);
        assert!(run(&host, REPO, &repository("    @EntityGraph(\"summary\") List<E> a();"), "jpa-unknown-entity-graph").is_empty());
        let mapped = with_entity("@NamedEntityGraph(name = \"summary\")\n");
        mapped.add_file("src/main/resources/META-INF/orm.xml", "<entity-mappings/>");
        assert!(run(&mapped, REPO, &missing, "jpa-unknown-entity-graph").is_empty());
        let dynamic = with_entity("@NamedEntityGraph(name = NAME)\n");
        assert!(run(&dynamic, REPO, &missing, "jpa-unknown-entity-graph").is_empty());
        let nobody = MockHost::install();
        assert!(run(&nobody, REPO, &missing, "jpa-unknown-entity-graph").is_empty());
        let unowned = repository("    @EntityGraph(\"missing\") List<E> a();").replace("org.springframework.data.jpa.repository.EntityGraph", "vendor.EntityGraph");
        assert!(run(&host, REPO, &unowned, "jpa-unknown-entity-graph").is_empty());
        let off = with_entity("");
        off.set_setting("jpaAssist.checkEntityGraphs", "false");
        assert!(run(&off, REPO, &missing, "jpa-unknown-entity-graph").is_empty());
    }

    #[test]
    fn entity_graph_path_must_start_with_an_attribute_of_the_repository_entity() {
        let host = with_entity("");
        let path = |p: &str| repository(&format!("    @EntityGraph(attributePaths = {{\"{p}\"}}) List<E> a();"));
        assert_eq!(run(&host, REPO, &path("nope"), "jpa-unknown-entity-graph-path").len(), 1);
        assert!(run(&host, REPO, &path("name"), "jpa-unknown-entity-graph-path").is_empty());
        assert!(run(&host, REPO, &path("name.deeper"), "jpa-unknown-entity-graph-path").is_empty());
        assert!(run(&MockHost::install(), REPO, &path("nope"), "jpa-unknown-entity-graph-path").is_empty());
    }

    #[test]
    fn unknown_named_query_warns_only_with_a_complete_project_scan() {
        let host = with_entity("@NamedQuery(name = \"E.byName\", query = \"select e from E e\")\n");
        let query = |n: &str| repository(&format!("    @Query(name = \"{n}\") List<E> a();"));
        let found = run(&host, REPO, &query("E.gone"), "jpa-unknown-named-query");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, Severity::Warning);
        assert!(run(&host, REPO, &query("E.byName"), "jpa-unknown-named-query").is_empty());
        host.add_file("src/main/resources/META-INF/jpa-named-queries.properties", "E.gone=select 1");
        assert!(run(&host, REPO, &query("E.gone"), "jpa-unknown-named-query").is_empty());
        let nobody = MockHost::install();
        assert!(run(&nobody, REPO, &query("E.gone"), "jpa-unknown-named-query").is_empty());
        let off = with_entity("");
        off.set_setting("jpaAssist.checkQueryReferences", "false");
        assert!(run(&off, REPO, &query("E.gone"), "jpa-unknown-named-query").is_empty());
    }

    fn persistence(classes: &[&str]) -> String {
        let list: String = classes.iter().map(|c| format!("<class>{c}</class>")).collect();
        format!("<persistence {NS} version=\"3.0\"><persistence-unit name=\"main\">{list}</persistence-unit></persistence>")
    }

    const PERSISTENCE: &str = "src/main/resources/META-INF/persistence.xml";

    #[test]
    fn persistence_xml_class_that_is_not_persistent_or_missing_is_noted() {
        let host = MockHost::install();
        host.add_file("src/main/java/shop/Plain.java", "package shop;\nclass Plain {}\n");
        host.add_file("src/main/java/shop/Order.java", &format!("{}@Entity\nclass Order {{ @Id Long id; }}\n", imports(&["Entity", "Id"])));
        let xml = persistence(&["shop.Plain", "shop.Order", "shop.Gone", "vendor.Lib"]);
        let plain = run(&host, PERSISTENCE, &xml, "jpa-xml-class-not-persistent");
        assert_eq!(plain.len(), 1);
        assert!(plain[0].1.contains("shop.Plain"));
        let unknown = run(&host, PERSISTENCE, &xml, "jpa-xml-class-unknown");
        assert_eq!(unknown.len(), 1, "only the class in an owned package: {unknown:?}");
        assert!(unknown[0].1.contains("shop.Gone"));
    }

    #[test]
    fn persistence_xml_stays_quiet_when_mapping_files_or_ownership_are_unresolved() {
        let host = MockHost::install();
        host.add_file("src/main/java/shop/Plain.java", "package shop;\nclass Plain {}\n");
        host.add_file("src/main/resources/META-INF/orm.xml", "<entity-mappings/>");
        assert!(run(&host, PERSISTENCE, &persistence(&["shop.Plain"]), "jpa-xml-class-not-persistent").is_empty());
        let nested = MockHost::install();
        assert!(run(&nested, PERSISTENCE, &persistence(&["shop.Plain", "shop.Outer$Inner"]), "jpa-xml-class-unknown").is_empty());
        let off = MockHost::install();
        off.set_setting("jpaAssist.checkPersistenceXml", "false");
        assert!(run(&off, PERSISTENCE, &persistence(&["shop.Plain"]), "jpa-xml-class-unknown").is_empty());
    }

    fn typed(ty: &str, annotations: &[&str]) -> String {
        let marks: String = annotations.iter().map(|a| format!("@{a} ")).collect();
        let mut names = vec!["Entity", "Id", "ManyToOne", "Convert", "Transient", "Lob", "Embedded"];
        names.dedup();
        format!("{}@Entity\npublic class E {{\n    @Id Long id;\n    {marks}{ty} value;\n}}\n", imports(&names))
    }

    fn money(header: &str) -> MockHost {
        let host = MockHost::install();
        host.add_file("src/main/java/shop/Money.java", &format!("package shop;\nclass Money{header} {{\n    long cents;\n}}\n"));
        host
    }

    #[test]
    fn project_class_without_a_mapping_story_is_an_unsupported_attribute_type() {
        let found = run(&money(""), E, &typed("Money", &[]), "jpa-unsupported-type");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, Severity::Warning);
        for exempt in [&["Convert"][..], &["Transient"], &["Lob"], &["Embedded"], &["ManyToOne"]] {
            assert!(run(&money(""), E, &typed("Money", exempt), "jpa-unsupported-type").is_empty(), "{exempt:?}");
        }
        assert!(run(&money(" implements java.io.Serializable"), E, &typed("Money", &[]), "jpa-unsupported-type").is_empty());
        let transient = typed("Money", &[]).replace("Money value;", "transient Money value;");
        assert!(run(&money(""), E, &transient, "jpa-unsupported-type").is_empty());
        let statics = typed("Money", &[]).replace("Money value;", "static Money value;");
        assert!(run(&money(""), E, &statics, "jpa-unsupported-type").is_empty());
    }

    #[test]
    fn unsupported_attribute_type_is_silent_when_a_converter_or_unknown_type_may_cover_it() {
        let host = money("");
        host.add_file(
            "src/main/java/shop/MoneyConverter.java",
            "package shop;\nimport jakarta.persistence.AttributeConverter;\nclass MoneyConverter implements AttributeConverter<Money, Long> {\n}\n",
        );
        assert!(run(&host, E, &typed("Money", &[]), "jpa-unsupported-type").is_empty());
        assert!(run(&MockHost::install(), E, &typed("Money", &[]), "jpa-unsupported-type").is_empty());
        assert!(run(&money(""), E, &typed("String", &[]), "jpa-unsupported-type").is_empty());
        let mapped = money("");
        mapped.add_file("src/main/resources/META-INF/orm.xml", "<entity-mappings/>");
        assert!(run(&mapped, E, &typed("Money", &[]), "jpa-unsupported-type").is_empty());
        let embeddable = MockHost::install();
        embeddable.add_file("src/main/java/shop/Money.java", &format!("{}@Embeddable\nclass Money {{ long cents; }}\n", imports(&["Embeddable"])));
        assert!(run(&embeddable, E, &typed("Money", &[]), "jpa-unsupported-type").is_empty());
        let off = money("");
        off.set_setting("jpaAssist.checkUnsupportedFieldType", "false");
        assert!(run(&off, E, &typed("Money", &[]), "jpa-unsupported-type").is_empty());
    }

    #[test]
    fn entity_typed_attribute_needs_a_relationship_annotation() {
        let host = MockHost::install();
        host.add_file("src/main/java/shop/Customer.java", &format!("{}@Entity\nclass Customer {{ @Id Long id; }}\n", imports(&["Entity", "Id"])));
        let found = run(&host, E, &typed("Customer", &[]), "jpa-entity-field-without-relationship");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, Severity::Warning);
        assert!(run(&host, E, &typed("Customer", &["ManyToOne"]), "jpa-entity-field-without-relationship").is_empty());
        assert!(run(&host, E, &typed("Customer", &["Transient"]), "jpa-entity-field-without-relationship").is_empty());
        host.set_setting("jpaAssist.checkEntityFieldRelationship", "false");
        assert!(run(&host, E, &typed("Customer", &[]), "jpa-entity-field-without-relationship").is_empty());
        assert!(run(&MockHost::install(), E, &typed("Customer", &[]), "jpa-entity-field-without-relationship").is_empty());
    }

    fn subclass(root_annotations: &str, table: &str) -> (MockHost, String) {
        let host = MockHost::install();
        host.add_file(
            "src/main/java/shop/Base.java",
            &format!("{}@Entity\n{root_annotations}\nclass Base {{ @Id Long id; }}\n", imports(&["Entity", "Id", "Inheritance", "InheritanceType"])),
        );
        let text = format!("{}@Entity\n{table}\nclass Sub extends Base {{\n}}\n", imports(&["Entity", "Table"]));
        (host, text)
    }

    const SUB: &str = "src/main/java/shop/Sub.java";

    #[test]
    fn table_on_a_single_table_subclass_warns() {
        for root in ["", "@Inheritance(strategy = InheritanceType.SINGLE_TABLE)"] {
            let (host, text) = subclass(root, "@Table(name = \"sub\")");
            let found = run(&host, SUB, &text, "jpa-incompatible-table");
            assert_eq!(found.len(), 1, "{root}");
            assert_eq!(found[0].0, Severity::Warning);
        }
    }

    #[test]
    fn table_on_a_joined_subclass_unknown_root_or_root_entity_is_quiet() {
        let (host, text) = subclass("@Inheritance(strategy = InheritanceType.JOINED)", "@Table(name = \"sub\")");
        assert!(run(&host, SUB, &text, "jpa-incompatible-table").is_empty());
        let (host, text) = subclass("", "");
        assert!(run(&host, SUB, &text, "jpa-incompatible-table").is_empty());
        let (host, text) = subclass("", "@Table(name = \"sub\")");
        host.add_file("src/main/resources/META-INF/orm.xml", "<entity-mappings/>");
        assert!(run(&host, SUB, &text, "jpa-incompatible-table").is_empty());
        let lonely = MockHost::install();
        assert!(run(&lonely, SUB, &text, "jpa-incompatible-table").is_empty());
        let root = format!("{}@Entity\n@Table(name = \"base\")\nclass Base {{ @Id Long id; }}\n", imports(&["Entity", "Id", "Table"]));
        assert!(run(&MockHost::install(), "src/main/java/shop/Base.java", &root, "jpa-incompatible-table").is_empty());
        let (off, text) = subclass("", "@Table(name = \"sub\")");
        off.set_setting("jpaAssist.checkIncompatibleTable", "false");
        assert!(run(&off, SUB, &text, "jpa-incompatible-table").is_empty());
    }

    fn composite(namespace: &str, key_header: &str) -> (MockHost, String) {
        let host = MockHost::install();
        host.add_file(
            "src/main/java/shop/Key.java",
            &format!("package shop;\nimport {namespace}.Embeddable;\n@Embeddable\nclass Key{key_header} {{\n    Long ownerId;\n}}\n"),
        );
        let text = format!(
            "package shop;\nimport {namespace}.Entity;\nimport {namespace}.EmbeddedId;\n@Entity\npublic class E {{\n    @EmbeddedId Key id;\n}}\n"
        );
        (host, text)
    }

    #[test]
    fn embedded_id_class_that_is_not_serializable_is_an_error_for_javax_sources() {
        let (host, text) = composite("javax.persistence", "");
        let found = run(&host, E, &text, "jpa-composite-id-not-serializable");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, Severity::Error);
        let (host, text) = composite("javax.persistence", " implements java.io.Serializable");
        assert!(run(&host, E, &text, "jpa-composite-id-not-serializable").is_empty());
        let (host, text) = composite("jakarta.persistence", "");
        assert!(run(&host, E, &text, "jpa-composite-id-not-serializable").is_empty());
        let (_, text) = composite("javax.persistence", "");
        assert!(run(&MockHost::install(), E, &text, "jpa-composite-id-not-serializable").is_empty());
        let (host, text) = composite("javax.persistence", "");
        host.set_setting("jpaAssist.checkCompositeIdSerializable", "false");
        assert!(run(&host, E, &text, "jpa-composite-id-not-serializable").is_empty());
    }

    #[test]
    fn original_equivalent_codes_default_to_warning_and_the_composite_id_error_stays() {
        let (host, text) = composite("javax.persistence", "");
        assert_eq!(run(&host, E, &text, "jpa-composite-id-not-serializable")[0].0, Severity::Error);
        let found = run(&money(""), E, &typed("Money", &[]), "jpa-unsupported-type");
        assert_eq!(found[0].0, Severity::Warning);
    }
}
