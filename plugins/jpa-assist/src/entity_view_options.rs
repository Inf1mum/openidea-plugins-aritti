//! Blaze-Persistence entity view with the options of the DTO window: Creatable and Updatable views and
//! inner entity views for associations (nested interface or separate file).
//!
//! Facts: `@EntityView`, `@IdMapping`, `@CreatableEntityView` and `@UpdatableEntityView` live in
//! `com.blazebit.persistence.view` of `com.blazebit:blaze-persistence-entity-view-api` (checked in the
//! 1.6.14 jar; the manual chapter "Updatable entity views" says an updatable view is annotated with
//! `@UpdatableEntityView` and a creatable one with `@CreatableEntityView`). Updatable and creatable views
//! get a setter for every attribute; inner views of such a view carry the same annotations. The id
//! attribute is the entity `@Id`. New files only (`overwrite: false`); nothing existing is edited.
use super::modelmapper_action as view;
use super::*;
use crate::model::Relation;

pub(super) const TITLE: &str = "Create Blaze entity view with options…";
const MAX_INNER: usize = 4;
const COLLECTIONS: [&str; 2] = ["List", "Set"];

#[derive(Clone, Copy)]
struct Flags {
    creatable: bool,
    updatable: bool,
}

struct Member {
    name: String,
    ty: String,
    id: bool,
}

struct Inner {
    nested: bool,
    name: String,
    entity: Class,
    members: Vec<Member>,
}

fn capitalised(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map(|c| c.to_ascii_uppercase().to_string() + chars.as_str()).unwrap_or_default()
}

fn base(ty: &str) -> &str {
    ty.split('<').next().unwrap_or(ty).rsplit('.').next().unwrap_or(ty)
}

fn imports(package_name: &str, flags: Flags, members: &[(&Class, &Member)], extra: &[String]) -> String {
    let mut found: Vec<String> = vec!["com.blazebit.persistence.view.EntityView".into(), "com.blazebit.persistence.view.IdMapping".into()];
    if flags.creatable {
        found.push("com.blazebit.persistence.view.CreatableEntityView".into());
    }
    if flags.updatable {
        found.push("com.blazebit.persistence.view.UpdatableEntityView".into());
    }
    for (class, member) in members {
        let (outer, inner) = member.ty.split_once('<').map_or((member.ty.as_str(), ""), |(outer, rest)| (outer, rest.trim_end_matches('>')));
        found.extend(class.qualified(base(outer)));
        if !inner.is_empty() && !inner.ends_with("View") {
            found.extend(class.qualified(base(inner)));
        }
    }
    found.extend(extra.iter().cloned());
    found.sort();
    found.dedup();
    found.retain(|import| import.rsplit_once('.').map(|(package, _)| package) != Some(package_name));
    found.iter().map(|import| format!("import {import};\n")).collect()
}

fn interface(indent: &str, public: bool, name: &str, entity: &str, members: &[&Member], flags: Flags, nested: &[String]) -> String {
    let mut out = String::new();
    if flags.creatable {
        out.push_str(&format!("{indent}@CreatableEntityView\n"));
    }
    if flags.updatable {
        out.push_str(&format!("{indent}@UpdatableEntityView\n"));
    }
    out.push_str(&format!("{indent}@EntityView({entity}.class)\n{indent}{}interface {name} {{\n", if public { "public " } else { "" }));
    for member in members {
        let getter = projection_action::getter(&member.name, &member.ty);
        out.push('\n');
        if member.id {
            out.push_str(&format!("{indent}    @IdMapping\n"));
        }
        out.push_str(&format!("{indent}    {} {getter}();\n", member.ty));
        if flags.creatable || flags.updatable {
            out.push_str(&format!("\n{indent}    void set{}({} {});\n", capitalised(&member.name), member.ty, member.name));
        }
    }
    for text in nested {
        out.push('\n');
        out.push_str(text);
    }
    out.push_str(&format!("{indent}}}\n"));
    out
}

fn members_of(id: &(String, String), picked: &[(String, String)]) -> Vec<Member> {
    let mut members = vec![Member { name: id.0.clone(), ty: id.1.clone(), id: true }];
    members.extend(picked.iter().map(|(name, ty)| Member { name: name.clone(), ty: ty.clone(), id: false }));
    members
}

fn gather_inner(pending: &Pending, doc: &Document, project: &Project, class: &Class, taken: &mut Vec<String>) -> Result<(Vec<Inner>, Vec<Member>), Error> {
    let eligible: Vec<_> = class
        .attributes
        .iter()
        .filter(|a| identifier(&a.name) && identifier(&a.target))
        .filter(|a| match a.relation {
            Relation::ManyToOne | Relation::OneToOne => true,
            Relation::OneToMany | Relation::ManyToMany => COLLECTIONS.iter().any(|c| a.ty.starts_with(&format!("{c}<"))),
            _ => false,
        })
        .collect();
    if eligible.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let items: Vec<_> = eligible.iter().map(|a| PickItem::new(&a.name, &a.name).description(&a.ty)).collect();
    let picked = host::pick_many("Associations as inner entity views (none is allowed)", &items)?.ok_or(Error::Cancelled)?;
    let mut seen = HashSet::new();
    if picked.len() > MAX_INNER || picked.iter().any(|n| !eligible.iter().any(|a| a.name == *n) || !seen.insert(n.as_str())) {
        return Err(format!("Choose at most {MAX_INNER} offered associations, each once.").into());
    }
    let package_name = package(doc).ok_or(Error::Stale)?;
    let mut inners = Vec::new();
    let mut outer_members = Vec::new();
    for association in eligible.iter().filter(|a| picked.contains(&a.name)) {
        let (target, _) = query_method_action::find_entity(project, &pending.root, &association.target, Some(&package_name), &class.imports)?;
        if target.kotlin {
            return Err(format!("Association `{}` targets a Kotlin entity; Java entity views only.", association.name).into());
        }
        let (target_id, rest) = view::view_fields(&target).ok_or_else(|| Error::from(format!("Entity {} needs exactly one plain scalar @Id attribute with a bean-safe name.", target.name)))?;
        let forms = [PickItem::new("nested", "New nested interface inside this view"), PickItem::new("new-class", "New entity view in a separate file")];
        let form = host::pick(&format!("Inner entity view for `{}`", association.name), &forms)?.ok_or(Error::Cancelled)?;
        if !forms.iter().any(|f| f.key == form) {
            return Err("Choose one of the offered forms.".into());
        }
        let default = format!("{}View", target.name);
        let name = host::input(&format!("Inner entity view name for `{}`", association.name), Some(&default))?.ok_or(Error::Cancelled)?.trim().to_owned();
        if !view::type_name(&name, &["EntityView", "IdMapping", "Mapping"]) || taken.contains(&name) || class.imports.iter().any(|i| i.rsplit('.').next() == Some(name.as_str())) {
            return Err("Use a Java identifier that does not clash with the other views, imports or the entity.".into());
        }
        taken.push(name.clone());
        let items: Vec<_> = rest.iter().map(|(n, t)| PickItem::new(n, n).description(t)).collect();
        let chosen = host::pick_many(&format!("Attributes of {} for the inner view of `{}` (besides the id)", target.name, association.name), &items)?.ok_or(Error::Cancelled)?;
        let mut seen = HashSet::new();
        if chosen.iter().any(|n| !rest.iter().any(|(f, _)| f == n) || !seen.insert(n.as_str())) {
            return Err("Choose only offered attributes, each once.".into());
        }
        let picked_fields: Vec<_> = rest.iter().filter(|(n, _)| chosen.contains(n)).cloned().collect();
        let ty = match association.ty.split_once('<') {
            Some((collection, _)) => format!("{collection}<{name}>"),
            None => name.clone(),
        };
        outer_members.push(Member { name: association.name.clone(), ty, id: false });
        let members = members_of(&target_id, &picked_fields);
        inners.push(Inner { nested: form == "nested", name, entity: target, members });
    }
    Ok((inners, outer_members))
}

fn render(class: &Class, package_name: &str, name: &str, flags: Flags, outer: &[Member], inners: &[Inner]) -> Result<Vec<(String, String)>, Error> {
    let outer_refs: Vec<&Member> = outer.iter().collect();
    let mut types: Vec<(&Class, &Member)> = outer.iter().map(|m| (class, m)).collect();
    let mut extra: Vec<String> = Vec::new();
    for inner in inners {
        extra.extend(inner.entity.package.as_deref().filter(|p| *p != package_name).map(|p| format!("{p}.{}", inner.entity.name)));
        if inner.nested {
            types.extend(inner.members.iter().map(|m| (&inner.entity, m)));
        }
    }
    let nested: Vec<String> = inners
        .iter()
        .filter(|inner| inner.nested)
        .map(|inner| interface("    ", false, &inner.name, &inner.entity.name, &inner.members.iter().collect::<Vec<_>>(), flags, &[]))
        .collect();
    let mut files = Vec::new();
    let body = interface("", true, name, &class.name, &outer_refs, flags, &nested);
    files.push((name.to_owned(), format!("package {package_name};\n\n{}\n{body}", imports(package_name, flags, &types, &extra))));
    for inner in inners.iter().filter(|inner| !inner.nested) {
        let own: Vec<(&Class, &Member)> = inner.members.iter().map(|m| (&inner.entity, m)).collect();
        let own_extra: Vec<String> = inner.entity.package.as_deref().filter(|p| *p != package_name).map(|p| format!("{p}.{}", inner.entity.name)).into_iter().collect();
        let body = interface("", true, &inner.name, &inner.entity.name, &inner.members.iter().collect::<Vec<_>>(), flags, &[]);
        files.push((inner.name.clone(), format!("package {package_name};\n\n{}\n{body}", imports(package_name, flags, &own, &own_extra))));
    }
    Ok(files)
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    view::java_only(doc)?;
    let class = dto_action::source(doc).ok_or(Error::Stale)?;
    let (id, available) = view::view_fields(&class).ok_or("The entity needs exactly one plain scalar @Id attribute with a bean-safe name.")?;
    view::build_proof(project, &pending.root, doc, "com.blazebit", &view::BLAZE_ARTIFACTS, "Blaze-Persistence entity views")?;
    let default = format!("{}View", class.name);
    let name = host::input("Entity view interface name (same package)", Some(&default))?.ok_or(Error::Cancelled)?.trim().to_owned();
    if !view::type_name(&name, &["EntityView", "IdMapping", "Mapping"])
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
    let kinds = [PickItem::new("creatable", "Creatable: @CreatableEntityView (setters for all attributes)"), PickItem::new("updatable", "Updatable: @UpdatableEntityView (setters for all attributes)")];
    let kind = host::pick_many("Entity view kind (none is a read-only view)", &kinds)?.ok_or(Error::Cancelled)?;
    let mut seen = HashSet::new();
    if kind.iter().any(|k| !kinds.iter().any(|item| item.key == *k) || !seen.insert(k.as_str())) {
        return Err("Choose only the offered kinds, each once.".into());
    }
    let flags = Flags { creatable: kind.iter().any(|k| k == "creatable"), updatable: kind.iter().any(|k| k == "updatable") };
    current(pending, doc)?;
    let mut taken = vec![name.clone(), class.name.clone()];
    let (inners, inner_members) = gather_inner(pending, doc, project, &class, &mut taken)?;
    let fields: Vec<_> = available.iter().filter(|(n, _)| chosen.contains(n)).cloned().collect();
    let mut outer = members_of(&id, &fields);
    if let Some(clash) = inner_members.iter().find(|m| outer.iter().any(|o| o.name == m.name)) {
        return Err(format!("`{}` is both a scalar attribute and an inner view.", clash.name).into());
    }
    outer.extend(inner_members);
    let declared = package(doc).ok_or(Error::Stale)?;
    let files = render(&class, &declared, &name, flags, &outer, &inners)?;
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let mut operations = Vec::new();
    let mut shown = String::new();
    for (file_name, text) in files {
        let text = view::crlf(doc, text);
        if text.len() > MAX_TEXT {
            return Err("Entity view output exceeds the supported bound.".into());
        }
        let target = format!("{directory}/{file_name}.java");
        if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
            return Err(Error::Stale);
        }
        no_collision(project, &pending.root, directory, &file_name, None)?;
        shown.push_str(&format!("New file: {target}\n{text}\n"));
        operations.push(FileOperation::Create { path: target, text, overwrite: false });
    }
    let note = "@IdMapping comes from the entity @Id. Creatable/updatable views get a setter for every attribute and inner views carry the same annotations; cascading (@UpdatableMapping) and @Mapping paths are not generated, and the views were not compiled against your Blaze-Persistence version.";
    if !ui_kit::prompts::confirm_exact(
        "Create Blaze entity view?",
        &format!("Source: {}\nSource version: {}\n\n{note}\n\nExact new files:\n{shown}One atomic host-managed Create; no existing file is overwritten.", doc.path, doc.version),
    )? {
        return Err(Error::Cancelled);
    }
    let project = current(pending, doc)?;
    view::build_proof(&project, &pending.root, doc, "com.blazebit", &view::BLAZE_ARTIFACTS, "Blaze-Persistence entity views")?;
    for operation in &operations {
        if let FileOperation::Create { path, .. } = operation {
            let stem = path.rsplit('/').next().and_then(|n| n.strip_suffix(".java")).unwrap_or_default();
            no_collision(&project, &pending.root, directory, stem, None)?;
        }
    }
    let open = match operations.first() {
        Some(FileOperation::Create { path, .. }) => Some(path.clone()),
        _ => None,
    };
    Ok(FileAction { title: format!("Create entity view {name}"), kind: Some("refactor.rewrite".into()), operations, open, ..FileAction::default() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const PATH: &str = "/project/src/main/java/shop/Order.java";
    const ORDER: &str = "package shop;\nimport jakarta.persistence.*;\nimport java.time.LocalDate;\nimport java.util.List;\n@Entity\npublic class Order {\n @Id Long id;\n String note;\n LocalDate placed;\n @ManyToOne Customer customer;\n @OneToMany(mappedBy = \"order\") List<Line> lines;\n}\n";
    const CUSTOMER: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Customer {\n @Id Long id;\n String name;\n}\n";
    const LINE: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Line {\n @Id Integer id;\n String sku;\n}\n";
    const POM: &str = "<project><dependencies><dependency><groupId>com.blazebit</groupId><artifactId>blaze-persistence-entity-view-api-jakarta</artifactId></dependency></dependencies></project>";

    fn run(build: bool, answers: &[&[&str]]) -> Result<Vec<(String, String)>, Error> {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(PATH, "java", ORDER, &[]);
        host.add_file(PATH, ORDER);
        host.add_file("/project/src/main/java/shop/Customer.java", CUSTOMER);
        host.add_file("/project/src/main/java/shop/Line.java", LINE);
        if build {
            host.add_file("pom.xml", POM);
        }
        let doc = Document { path: PATH.into(), text: ORDER.into(), language: "java".into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).ok_or(Error::Stale)?;
        for answer in answers {
            host.answer_next_prompt(Some(answer));
        }
        let action = plugin.resolve_file_action(&doc, pending)?;
        assert!(action.edits.is_empty());
        Ok(action
            .operations
            .into_iter()
            .map(|operation| match operation {
                FileOperation::Create { path, text, overwrite: false } => (path, text),
                _ => panic!("only non-overwriting creates"),
            })
            .collect())
    }

    #[test]
    fn read_only_view_without_options_matches_the_plain_view_action() {
        let files = run(true, &[&["OrderView"], &["note", "placed"], &[], &[], &[]]).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].1, "package shop;\n\nimport com.blazebit.persistence.view.EntityView;\nimport com.blazebit.persistence.view.IdMapping;\nimport java.time.LocalDate;\n\n@EntityView(Order.class)\npublic interface OrderView {\n\n    @IdMapping\n    Long getId();\n\n    String getNote();\n\n    LocalDate getPlaced();\n}\n");
    }

    #[test]
    fn creatable_and_updatable_views_have_annotations_and_setters_for_every_attribute() {
        let files = run(true, &[&["OrderView"], &["note"], &["creatable", "updatable"], &[], &[]]).unwrap();
        let text = &files[0].1;
        assert!(text.contains("@CreatableEntityView\n@UpdatableEntityView\n@EntityView(Order.class)\npublic interface OrderView {"), "{text}");
        for member in ["    @IdMapping\n    Long getId();\n\n    void setId(Long id);\n", "    String getNote();\n\n    void setNote(String note);\n"] {
            assert!(text.contains(member), "{text}");
        }
        let updatable = run(true, &[&["OrderView"], &[], &["updatable"], &[], &[]]).unwrap();
        assert!(updatable[0].1.contains("@UpdatableEntityView\n@EntityView") && !updatable[0].1.contains("Creatable"), "{}", updatable[0].1);
    }

    #[test]
    fn inner_views_are_nested_or_separate_files_and_carry_the_same_annotations() {
        let files = run(
            true,
            &[&["OrderView"], &["note"], &["updatable"], &["customer", "lines"], &["nested"], &["CustomerView"], &["name"], &["new-class"], &["LineView"], &["sku"], &[]],
        )
        .unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(
            files[0].1,
            "package shop;\n\nimport com.blazebit.persistence.view.EntityView;\nimport com.blazebit.persistence.view.IdMapping;\nimport com.blazebit.persistence.view.UpdatableEntityView;\nimport java.util.List;\n\n@UpdatableEntityView\n@EntityView(Order.class)\npublic interface OrderView {\n\n    @IdMapping\n    Long getId();\n\n    void setId(Long id);\n\n    String getNote();\n\n    void setNote(String note);\n\n    CustomerView getCustomer();\n\n    void setCustomer(CustomerView customer);\n\n    List<LineView> getLines();\n\n    void setLines(List<LineView> lines);\n\n    @UpdatableEntityView\n    @EntityView(Customer.class)\n    interface CustomerView {\n\n        @IdMapping\n        Long getId();\n\n        void setId(Long id);\n\n        String getName();\n\n        void setName(String name);\n    }\n}\n"
        );
        assert_eq!(files[1].0, "/project/src/main/java/shop/LineView.java");
        assert_eq!(
            files[1].1,
            "package shop;\n\nimport com.blazebit.persistence.view.EntityView;\nimport com.blazebit.persistence.view.IdMapping;\nimport com.blazebit.persistence.view.UpdatableEntityView;\n\n@UpdatableEntityView\n@EntityView(Line.class)\npublic interface LineView {\n\n    @IdMapping\n    Integer getId();\n\n    void setId(Integer id);\n\n    String getSku();\n\n    void setSku(String sku);\n}\n"
        );
    }

    #[test]
    fn refusals_leave_nothing_behind() {
        assert!(run(false, &[&["OrderView"]]).is_err(), "Blaze must be declared in the build");
        assert!(run(true, &[&["Order"]]).is_err());
        assert!(run(true, &[&["OrderView"], &["note"], &["sealed"]]).is_err());
        assert!(run(true, &[&["OrderView"], &["note"], &[], &["customer"], &["nested"], &["OrderView"]]).is_err(), "inner name clashes with the outer view");
        assert!(run(true, &[&["OrderView"], &["note"], &[], &["ghost"]]).is_err());
        assert!(matches!(run(true, &[&["OrderView"], &["note"], &[]]), Err(Error::Cancelled)));
    }

    #[test]
    #[ignore = "requires javac, JPA_JAKARTA_API and BLAZE_ENTITY_VIEW_API_JAR (com.blazebit:blaze-persistence-entity-view-api); compiles the generated views against the real annotations"]
    fn generated_views_compile_against_the_real_entity_view_api() {
        let api = std::env::var("JPA_JAKARTA_API").expect("explicit API jar path");
        let blaze = std::env::var("BLAZE_ENTITY_VIEW_API_JAR").expect("entity view api jar");
        let answers: [&[&str]; 11] = [&["OrderView"], &["note", "placed"], &["creatable", "updatable"], &["customer", "lines"], &["nested"], &["CustomerView"], &["name"], &["new-class"], &["LineView"], &["sku"], &[]];
        let mut files = run(true, &answers).unwrap();
        files.push(("/project/src/main/java/shop/Order.java".into(), ORDER.into()));
        files.push(("/project/src/main/java/shop/Customer.java".into(), CUSTOMER.into()));
        files.push(("/project/src/main/java/shop/Line.java".into(), LINE.into()));
        let dir = std::env::temp_dir().join(format!("jpa-entity-view-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths: Vec<_> = files
            .iter()
            .map(|(path, body)| {
                let target = dir.join(path.rsplit('/').next().unwrap());
                std::fs::write(&target, body).unwrap();
                target
            })
            .collect();
        let classpath = format!("{api}:{blaze}");
        let result = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &classpath, "-d"]).arg(&dir).args(&paths).output().expect("javac installed");
        assert!(result.status.success(), "{}\n{files:?}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
