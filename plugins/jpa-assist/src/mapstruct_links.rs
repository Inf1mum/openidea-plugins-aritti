//! MapStruct mappers as a third way to link a DTO to its entity (beside the name and Javadoc patterns), and the
//! `@Mapping` correspondences that attribute rename and DTO synchronization follow. Read by a bounded scan of
//! the project sources when an action runs, never in a provider. A type that cannot be proved from the mapper's
//! own imports and package is not linked and its literals are not edited.
use super::*;
use super::mapstruct_model::{Mapper, MAX_TEXT as MAPPER_TEXT, first_segment, parse_mapper, type_decls};
use super::projection_links::Linked;
use crate::model::{Edit, mask};

const SCAN_LIMIT: u32 = 1001;
const MAX_LINKED_TYPES: usize = 200;

pub(crate) struct MapperSource {
    pub path: String,
    pub text: String,
    pub mapper: Mapper,
}

pub(crate) fn java_sources(project: &Project, root: &str) -> Result<Vec<String>, Error> {
    let files = project.find_files("**/*.java", SCAN_LIMIT)?;
    if files.len() >= SCAN_LIMIT as usize {
        return Err("The project has too many sources to read MapStruct mappers.".into());
    }
    Ok(files
        .into_iter()
        .map(|file| if file.starts_with('/') { file } else { format!("{}/{file}", root.trim_end_matches('/')) })
        .filter(|file| path(file).as_deref() == Some(file.as_str()) && inside(root, file))
        .collect())
}

pub(crate) fn mappers(project: &Project, files: &[String]) -> Result<Vec<MapperSource>, Error> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut found = Vec::new();
    for file in files {
        if Instant::now() >= deadline {
            return Err("MapStruct mapper discovery timed out.".into());
        }
        let Ok(text) = project.read_file(file) else { continue };
        if !text.contains("@Mapper") {
            continue;
        }
        if let Some(mapper) = parse_mapper(&text) {
            found.push(MapperSource { path: file.clone(), text, mapper });
        }
    }
    Ok(found)
}

/// `(source, target)` qualified type pairs of every bean or collection method of `text`'s mapper.
pub(crate) fn mapped_types(text: &str) -> Vec<(String, String)> {
    let Some(mapper) = parse_mapper(text) else { return Vec::new() };
    let mut pairs: Vec<(String, String)> = Vec::new();
    for method in &mapper.methods {
        let Some((source, target)) = method.link_types() else { continue };
        let (Some(source), Some(target)) = (mapper.resolve(&source), mapper.resolve(&target)) else { continue };
        if source != target && !pairs.contains(&(source.clone(), target.clone())) {
            pairs.push((source, target));
        }
    }
    pairs
}

/// Qualified names of the types mapped to or from `entity`.
pub(crate) fn dto_names(text: &str, entity: &str) -> Vec<String> {
    let mut found = Vec::new();
    if !text.contains("@Mapper") {
        return found;
    }
    for (source, target) in mapped_types(text) {
        let other = if source == entity { target } else if target == entity { source } else { continue };
        if !found.contains(&other) {
            found.push(other);
        }
    }
    found
}

/// The one source file declaring `qualified`, when exactly one does.
pub(crate) fn locate(files: &[String], qualified: &str) -> Option<String> {
    let suffix = format!("/{}.java", qualified.replace('.', "/"));
    let mut matching = files.iter().filter(|file| file.ends_with(&suffix) || **file == suffix[1..]);
    let first = matching.next()?;
    matching.next().is_none().then(|| first.clone())
}

/// Linked-type shapes of the mapper-linked types that the name and Javadoc rules did not already find.
pub(super) fn extra_linked(project: &Project, files: &[String], names: &[String], own_path: &str, known: &[String]) -> Result<(Vec<Linked>, Vec<String>), Error> {
    let (mut linked, mut skipped) = (Vec::new(), Vec::new());
    for name in names.iter().take(MAX_LINKED_TYPES) {
        let Some(file) = locate(files, name) else { continue };
        if file == own_path || known.contains(&file) || linked.iter().any(|item: &Linked| item.path == file) {
            continue;
        }
        let text = project.read_file(&file)?;
        if text.len() > MAX_TEXT || ["@Entity", "@MappedSuperclass", "@Embeddable"].iter().any(|marker| text.contains(marker)) {
            continue;
        }
        let stem = name.rsplit('.').next().unwrap_or(name);
        match projection_links::parse(&file, &text, false) {
            Ok(Some((item, _))) => linked.push(item),
            Ok(None) => skipped.push(format!("{stem}: mapper-linked, but not an interface or record")),
            Err(reason) => skipped.push(reason),
        }
    }
    linked.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((linked, skipped))
}

/// `(entity attribute, DTO property)` pairs that the mappers between the two types match by `@Mapping`.
pub(crate) fn correspondences(sources: &[MapperSource], entity: &str, dto: &str) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    for source in sources {
        let mapper = &source.mapper;
        let resolved = |method: &mapstruct_model::Method| -> Option<(String, String)> {
            let (from, to) = method.bean_types()?;
            Some((mapper.resolve(from)?, mapper.resolve(to)?))
        };
        let pairs_of = |method: &mapstruct_model::Method| -> Vec<(String, String)> {
            method
                .mappings
                .iter()
                .filter_map(|mapping| {
                    let (source, target) = (mapping.source.as_ref()?, mapping.target.as_ref()?);
                    Some((first_segment(source)?.0.to_owned(), first_segment(target)?.0.to_owned()))
                })
                .collect()
        };
        for method in &mapper.methods {
            let Some((from, to)) = resolved(method) else { continue };
            let mut pairs = pairs_of(method);
            if method.inherits_inverse && method.mappings.is_empty() {
                let inverse = mapper.methods.iter().find(|other| resolved(other).is_some_and(|(a, b)| a == to && b == from));
                pairs = inverse.map(|other| pairs_of(other).into_iter().map(|(a, b)| (b, a)).collect()).unwrap_or_default();
            }
            let oriented = if from == entity && to == dto {
                pairs
            } else if from == dto && to == entity {
                pairs.into_iter().map(|(a, b)| (b, a)).collect()
            } else {
                continue;
            };
            for pair in oriented {
                if !found.contains(&pair) {
                    found.push(pair);
                }
            }
        }
    }
    found
}

pub(crate) struct Plan {
    files: Vec<(String, String, Vec<Edit>)>,
    lines: Vec<String>,
    note: String,
}

impl Plan {
    pub(crate) fn preview(&self) -> String {
        if !self.note.is_empty() {
            return format!("\n{}\n", self.note);
        }
        if self.lines.is_empty() {
            return String::new();
        }
        format!("\nMapStruct mapping literals renamed with it:\n{}\nNested path segments, expressions and mappers that cannot be proved are not edited.\n", self.lines.join("\n"))
    }

    pub(crate) fn verify(&self, project: &Project) -> Result<(), Error> {
        for (path, text, _) in &self.files {
            if project.read_file(path)? != *text {
                return Err(Error::Stale);
            }
        }
        Ok(())
    }

    pub(crate) fn extend(&self, action: &mut FileAction) -> Result<(), Error> {
        for (path, text, edits) in &self.files {
            let converted: Vec<_> = edits.iter().map(|edit| edit.to_text_edit(text)).collect();
            patched(text, &converted)?;
            action.edits.push((path.clone(), converted));
        }
        Ok(())
    }
}

/// Mapping literals that follow the renames `(qualified type, old attribute, new attribute)`. Never fails: a scan
/// that cannot finish yields a plan without edits and a note saying so.
pub(crate) fn rename_plan(project: &Project, root: &str, renames: &[(String, String, String)]) -> Plan {
    let mut plan = Plan { files: Vec::new(), lines: Vec::new(), note: String::new() };
    let sources = java_sources(project, root).and_then(|files| mappers(project, &files));
    let sources = match sources {
        Ok(sources) => sources,
        Err(error) => {
            plan.note = format!("MapStruct mapping literals were not scanned ({error}); they are not renamed.");
            return plan;
        }
    };
    for source in sources.iter().filter(|source| source.text.len() <= MAPPER_TEXT) {
        let mapper = &source.mapper;
        let mut edits: Vec<Edit> = Vec::new();
        for method in &mapper.methods {
            let (Some((from, to)), Some(param)) = (method.bean_types(), method.source_param()) else { continue };
            let (from, to) = (mapper.resolve(from), mapper.resolve(to));
            for mapping in &method.mappings {
                for (literal, owner, is_source) in [(&mapping.source, &from, true), (&mapping.target, &to, false)] {
                    let (Some(literal), Some(owner)) = (literal, owner) else { continue };
                    let Some((segment, start, end)) = first_segment(literal) else { continue };
                    if is_source && segment == param.name {
                        continue;
                    }
                    let Some((_, _, new)) = renames.iter().find(|(qualified, old, _)| qualified == owner && old == segment) else { continue };
                    if edits.iter().any(|edit| edit.start == start) {
                        continue;
                    }
                    let line = position(&source.text, start).line + 1;
                    plan.lines.push(format!("{} line {line}: {} = \"{segment}\" -> \"{new}\"", source.path, if is_source { "source" } else { "target" }));
                    edits.push(Edit { start, end, text: new.clone() });
                }
            }
        }
        if !edits.is_empty() {
            edits.sort_by_key(|edit| edit.start);
            plan.files.push((source.path.clone(), source.text.clone(), edits));
        }
    }
    plan
}

/// The offset of the top-level type `stem` declared in `text`.
pub(crate) fn declaration_offset(text: &str, stem: &str) -> Option<usize> {
    let masked = mask(text);
    let (tops, _) = type_decls(&masked);
    tops.into_iter().find(|decl| decl.name == stem).map(|decl| decl.name_span.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const MAPPER_PATH: &str = "/project/src/main/java/shop/web/CustomerMapper.java";
    const MAPPER: &str = "package shop.web;\nimport org.mapstruct.*;\nimport shop.Customer;\n@Mapper\npublic interface CustomerMapper {\n    @Mapping(source = \"title\", target = \"name\")\n    @Mapping(source = \"title.length\", target = \"size\")\n    CustomerDto toDto(Customer customer);\n    @Mapping(source = \"name\", target = \"title\")\n    Customer toEntity(CustomerDto dto);\n    @InheritInverseConfiguration\n    void back(CustomerDto dto, @MappingTarget Customer customer);\n}\n";

    fn parsed(text: &str) -> Vec<MapperSource> {
        vec![MapperSource { path: MAPPER_PATH.into(), text: text.into(), mapper: parse_mapper(text).unwrap() }]
    }

    #[test]
    fn dto_names_follow_mapper_methods_in_either_direction_and_through_collections() {
        assert_eq!(dto_names(MAPPER, "shop.Customer"), ["shop.web.CustomerDto"]);
        assert!(dto_names(MAPPER, "shop.Other").is_empty());
        let lists = "package a;\nimport org.mapstruct.Mapper;\nimport b.E;\n@Mapper\ninterface M { List<D> all(List<E> e); }\n";
        assert_eq!(dto_names(lists, "b.E"), ["a.D"]);
        let wild_and_dup = "package a;\nimport org.mapstruct.Mapper;\nimport b.E;\nimport c.D;\nimport d.D;\n@Mapper\ninterface M { D one(E e); E same(E e); }\n";
        assert!(dto_names(wild_and_dup, "b.E").is_empty());
        assert!(dto_names("package a;\ninterface M { D one(E e); }\n", "a.E").is_empty());
    }

    #[test]
    fn correspondences_read_mapping_pairs_and_inherit_inverse_pairs() {
        let pairs = correspondences(&parsed(MAPPER), "shop.Customer", "shop.web.CustomerDto");
        assert_eq!(pairs, [("title".to_owned(), "name".to_owned()), ("title".to_owned(), "size".to_owned())]);
        assert!(correspondences(&parsed(MAPPER), "shop.Customer", "shop.web.Unrelated").is_empty());
    }

    fn host_with_mapper(text: &str) -> (MockHost, Project) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file(MAPPER_PATH, text);
        host.add_file("/project/src/main/java/shop/Customer.java", "package shop;\npublic class Customer {}\n");
        let project = host.invocation().project().unwrap();
        (host, project)
    }

    #[test]
    fn rename_plan_edits_only_provably_typed_first_segments() {
        let (_host, project) = host_with_mapper(MAPPER);
        let renames = [("shop.Customer".to_owned(), "title".to_owned(), "heading".to_owned())];
        let plan = rename_plan(&project, "/project", &renames);
        let mut action = FileAction::default();
        plan.extend(&mut action).unwrap();
        let [(path, edits)] = action.edits.as_slice() else { panic!("one mapper") };
        assert_eq!(path, MAPPER_PATH);
        let after = patched(MAPPER, edits).unwrap();
        assert!(after.contains("source = \"heading\", target = \"name\""));
        assert!(after.contains("source = \"heading.length\""));
        assert!(after.contains("source = \"name\", target = \"heading\""));
        assert_eq!(edits.len(), 3);
        assert!(plan.preview().contains("line 6"));
        plan.verify(&project).unwrap();
        let none = rename_plan(&project, "/project", &[("shop.Other".to_owned(), "title".to_owned(), "x".to_owned())]);
        assert!(none.preview().is_empty());
        let dto = rename_plan(&project, "/project", &[("shop.web.CustomerDto".to_owned(), "name".to_owned(), "label".to_owned())]);
        let mut action = FileAction::default();
        dto.extend(&mut action).unwrap();
        assert_eq!(action.edits[0].1.len(), 2, "toDto target and toEntity source");
    }

    #[test]
    fn a_parameter_named_like_the_attribute_and_unknown_types_stay_untouched() {
        let text = "package s;\nimport org.mapstruct.Mapper;\nimport org.mapstruct.Mapping;\nimport x.*;\nimport y.Dup;\nimport z.Dup;\n@Mapper\ninterface M {\n @Mapping(source = \"title\", target = \"t\") T a(Dup title);\n @Mapping(source = \"title\", target = \"t\") T b(Item item);\n}\n";
        let (_host, project) = host_with_mapper(text);
        let plan = rename_plan(&project, "/project", &[("s.Item".to_owned(), "title".to_owned(), "x".to_owned()), ("y.Dup".to_owned(), "title".to_owned(), "x".to_owned())]);
        let mut action = FileAction::default();
        plan.extend(&mut action).unwrap();
        assert_eq!(action.edits.len(), 1);
        assert_eq!(action.edits[0].1.len(), 1, "only Item's source literal is provable");
    }

    #[test]
    fn a_changed_mapper_makes_the_plan_stale_and_too_many_sources_are_a_note_not_an_error() {
        let (host, project) = host_with_mapper(MAPPER);
        let plan = rename_plan(&project, "/project", &[("shop.Customer".to_owned(), "title".to_owned(), "heading".to_owned())]);
        host.add_file(MAPPER_PATH, &MAPPER.replace("toEntity", "fromDto"));
        assert!(matches!(plan.verify(&project), Err(Error::Stale)));
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        for index in 0..1001 {
            host.add_file(&format!("/project/src/main/java/p{index}/A.java"), "package p;\nclass A {}\n");
        }
        let project = host.invocation().project().unwrap();
        let plan = rename_plan(&project, "/project", &[("a.B".to_owned(), "x".to_owned(), "y".to_owned())]);
        assert!(plan.preview().contains("were not scanned"));
    }

    #[test]
    fn locate_needs_exactly_one_file_for_the_qualified_name() {
        let files = vec!["/p/src/a/B.java".to_owned(), "/p/test/a/B.java".to_owned(), "/p/src/a/C.java".to_owned()];
        assert_eq!(locate(&files, "a.C").as_deref(), Some("/p/src/a/C.java"));
        assert_eq!(locate(&files, "a.B"), None);
        assert_eq!(locate(&files, "a.Missing"), None);
    }

    const CUSTOMER_PATH: &str = "/project/src/main/java/shop/Customer.java";
    const CUSTOMER: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.Column;\nimport jakarta.persistence.NamedEntityGraph;\nimport jakarta.persistence.NamedAttributeNode;\nimport java.lang.Long;\nimport java.lang.String;\n@Entity\n@NamedEntityGraph(name=\"g\",attributeNodes={@NamedAttributeNode(\"title\")})\npublic class Customer {\n@Id private Long id;\n@Column(name=\"title\") private String title;\n@Column(name=\"joined\") private String joined;\n}\n";
    const NAMED_MAPPER: &str = "package shop.web;\nimport org.mapstruct.Mapper;\nimport org.mapstruct.Mapping;\nimport shop.Customer;\n@Mapper\npublic interface CustomerMapper {\n    @Mapping(source = \"title\", target = \"label\")\n    CustomerDto toDto(Customer customer);\n    @Mapping(source = \"label\", target = \"title\")\n    Customer toEntity(CustomerDto dto);\n}\n";

    fn editor(host: &MockHost, title: &str, anchor: &str) -> (JpaAssist, Document, FileAction) {
        host.open_editor(CUSTOMER_PATH, "java", CUSTOMER, &[]);
        let doc = Document { path: CUSTOMER_PATH.into(), language: "java".into(), text: CUSTOMER.into(), version: 1 };
        let at = CUSTOMER.find(anchor).unwrap() + anchor.len() - 6;
        let selection = Range::new(position(CUSTOMER, at), position(CUSTOMER, at + 5));
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, selection).into_iter().find(|a| a.title == title).expect("offered");
        (plugin, doc, pending)
    }

    fn project_host(files: &[(&str, &str)]) -> MockHost {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/Customer.java", CUSTOMER);
        for (path, text) in files {
            host.add_file(path, text);
        }
        host
    }

    #[test]
    fn attribute_rename_also_renames_mapping_literals_on_the_mapper_after_review() {
        let host = project_host(&[("src/main/java/shop/web/CustomerMapper.java", NAMED_MAPPER)]);
        let (mut plugin, doc, pending) = editor(&host, attribute_refactor::TITLE, "String title;");
        host.answer_next_prompt(Some(&["name"]));
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        let [(entity, _), (mapper, edits)] = action.edits.as_slice() else { panic!("{:?}", action.edits.iter().map(|e| &e.0).collect::<Vec<_>>()) };
        assert_eq!(entity, CUSTOMER_PATH);
        assert!(mapper.ends_with("CustomerMapper.java"));
        let after = patched(NAMED_MAPPER, edits).unwrap();
        assert!(after.contains("source = \"name\", target = \"label\"") && after.contains("source = \"label\", target = \"name\""), "{after}");
    }

    #[test]
    fn declining_the_review_edits_nothing_and_a_project_without_mappers_edits_only_the_entity() {
        let host = project_host(&[("src/main/java/shop/web/CustomerMapper.java", NAMED_MAPPER)]);
        let (mut plugin, doc, pending) = editor(&host, attribute_refactor::TITLE, "String title;");
        host.answer_next_prompt(Some(&["name"]));
        host.answer_next_prompt(None);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
        let host = project_host(&[]);
        let (mut plugin, doc, pending) = editor(&host, attribute_refactor::TITLE, "String title;");
        host.answer_next_prompt(Some(&["name"]));
        host.answer_next_prompt(Some(&[]));
        assert_eq!(plugin.resolve_file_action(&doc, pending).unwrap().edits.len(), 1);
    }

    #[test]
    fn rename_with_dtos_renames_both_sides_of_a_mapping_when_the_dto_property_follows() {
        let dto = "package shop.web;\n\npublic record CustomerDto(String title) {\n}\n";
        let mapper = "package shop.web;\nimport org.mapstruct.*;\nimport shop.Customer;\n@Mapper\ninterface CustomerMapper {\n    @Mapping(source = \"title\", target = \"title\")\n    CustomerDto toDto(Customer c);\n}\n";
        let host = project_host(&[("src/main/java/shop/web/CustomerDto.java", dto), ("src/main/java/shop/web/CustomerMapper.java", mapper)]);
        let (mut plugin, doc, pending) = editor(&host, projection_links::RENAME_DTO_TITLE, "String title;");
        host.answer_next_prompt(Some(&["name"]));
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        assert_eq!(action.edits.len(), 3);
        let (_, edits) = action.edits.iter().find(|(path, _)| path.ends_with("CustomerMapper.java")).unwrap();
        assert_eq!(edits.len(), 2);
        assert!(patched(mapper, edits).unwrap().contains("@Mapping(source = \"name\", target = \"name\")"));
    }

    const SYNC_ENTITY_DTO: &str = "package shop.web;\n\npublic record CustomerDto(Long id, String name, String joined) {\n}\n";

    #[test]
    fn sync_matches_differently_named_properties_through_mapping_pairs() {
        let host = project_host(&[("src/main/java/shop/web/CustomerDto.java", SYNC_ENTITY_DTO), ("src/main/java/shop/web/CustomerMapper.java", NAMED_MAPPER.replace("label", "name").as_str())]);
        let (mut plugin, doc, pending) = editor(&host, projection_links::SYNC_DTO_TITLE, "String title;");
        host.answer_next_prompt(Some(&["toDtos"]));
        let error = format!("{:?}", plugin.resolve_file_action(&doc, pending).unwrap_err());
        assert!(error.contains("already in sync"), "{error}");
    }

    #[test]
    fn sync_without_mapping_pairs_still_adds_and_removes_by_name() {
        let host = project_host(&[("src/main/java/shop/web/CustomerDto.java", SYNC_ENTITY_DTO)]);
        let (mut plugin, doc, pending) = editor(&host, projection_links::SYNC_DTO_TITLE, "String title;");
        host.answer_next_prompt(Some(&["toDtos"]));
        host.answer_next_prompt(Some(&["add:0:title", "remove:0:name"]));
        host.answer_next_prompt(Some(&[]));
        assert_eq!(plugin.resolve_file_action(&doc, pending).unwrap().edits.len(), 1);
    }

    #[test]
    fn a_dto_outside_the_name_pattern_is_linked_by_its_mapper_method_for_sync() {
        let summary = "package shop.web;\n\npublic record Summary(Long id) {\n}\n";
        let mapper = "package shop.web;\nimport org.mapstruct.Mapper;\nimport shop.Customer;\n@Mapper\ninterface M {\n    Summary toSummary(Customer c);\n}\n";
        let host = project_host(&[("src/main/java/shop/web/Summary.java", summary), ("src/main/java/shop/web/M.java", mapper)]);
        let (mut plugin, doc, pending) = editor(&host, projection_links::SYNC_DTO_TITLE, "String title;");
        host.answer_next_prompt(Some(&["toDtos"]));
        host.answer_next_prompt(Some(&["add:0:title"]));
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        let [(path, edits)] = action.edits.as_slice() else { panic!("one DTO") };
        assert!(path.ends_with("Summary.java"));
        assert!(patched(summary, edits).unwrap().contains("Long id, String title"));
    }

    #[test]
    fn reuse_candidates_and_the_tree_group_list_the_mapper_linked_dto_once() {
        let summary = "package shop.web;\n\npublic record Summary(Long id) {\n}\n";
        let mapper = "package shop.web;\nimport org.mapstruct.Mapper;\nimport shop.Customer;\n@Mapper\ninterface M {\n    Summary toSummary(Customer c);\n    Customer back(Summary s);\n}\n";
        let entity_like = "package shop.web;\n@Entity\npublic class Row {}\n";
        let other = "package shop.web;\nimport org.mapstruct.Mapper;\nimport shop.Customer;\n@Mapper\ninterface N {\n    Row toRow(Customer c);\n}\n";
        let host = project_host(&[("src/main/java/shop/web/Summary.java", summary), ("src/main/java/shop/web/M.java", mapper), ("src/main/java/shop/web/Row.java", entity_like), ("src/main/java/shop/web/N.java", other)]);
        let project = host.invocation().project().unwrap();
        let candidates = dto_links::linked(&project, "/project", CUSTOMER_PATH, false).unwrap();
        assert_eq!(candidates.iter().map(|c| (c.name.as_str(), c.badge)).collect::<Vec<_>>(), [("Summary", "linked by mapper method")]);
        assert!(dto_links::linked(&project, "/project", CUSTOMER_PATH, true).unwrap().is_empty());

        let links = crate::structure_artifacts::Links::load();
        let mut artifacts = crate::structure_artifacts::Artifacts::default();
        for (path, text) in [("/project/src/main/java/shop/web/M.java", mapper), ("/project/src/main/java/shop/web/N.java", other)] {
            artifacts.observe(&links, path, text);
        }
        let files: Vec<String> = ["Summary", "Row", "M", "N"].iter().map(|name| format!("/project/src/main/java/shop/web/{name}.java")).collect();
        let entities = ["Customer".to_owned(), "Row".to_owned()].into_iter().collect();
        artifacts.link_mapped(&project, &files, &entities);
        let listed: Vec<_> = artifacts.linked.get("Customer").into_iter().flatten().map(|found| (found.name.as_str(), found.detail.as_str())).collect();
        assert_eq!(listed, [("Summary", "DTO \u{b7} linked by mapper method")]);
    }
}
