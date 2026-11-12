//! Additions to the SQL-file entity generation, one prompt each, all opt-in from the generation options:
//! editable attribute names, foreign keys as basic attributes, SQL-type overrides (Java type, JPA converter or
//! Hibernate custom type, with Hibernate Types suggestions), and a `@MappedSuperclass` parent.
use super::*;

#[derive(Clone, Debug)]
pub(super) enum Target {
    Plain,
    Converter(String),
    Hibernate(String),
}

#[derive(Clone, Debug)]
pub(super) struct TypeOverride {
    pub java: String,
    pub kotlin: String,
    pub imports: Vec<String>,
    pub target: Target,
    pub definition: Option<String>,
}

fn simple_name(fqcn: &str) -> &str {
    fqcn.rsplit('.').next().unwrap_or(fqcn)
}

impl TypeOverride {
    /// The annotation this target needs next to the field; registers its imports.
    pub(super) fn annotation(&self, ns: &str, kotlin: bool, imports: &mut BTreeSet<String>) -> Option<String> {
        imports.extend(self.imports.iter().cloned());
        let (annotation, class) = match &self.target {
            Target::Plain => return None,
            Target::Converter(class) => {
                imports.insert(format!("{ns}.Convert"));
                ("@Convert(converter = ", class)
            }
            Target::Hibernate(class) => {
                imports.insert("org.hibernate.annotations.Type".to_owned());
                ("@Type(", class)
            }
        };
        imports.insert(class.clone());
        let suffix = if kotlin { "::class" } else { ".class" };
        Some(format!("{annotation}{}{suffix})", simple_name(class)))
    }
}

/// `io.hypersistence.utils` types for SQL types the generator does not map by itself.
pub(super) fn suggestion(base: &str) -> Option<TypeOverride> {
    let hibernate = |class: &str| Target::Hibernate(format!("io.hypersistence.utils.hibernate.type.{class}"));
    let made = |java: &str, kotlin: &str, imports: &[&str], target: Target| TypeOverride {
        java: java.into(),
        kotlin: kotlin.into(),
        imports: imports.iter().map(|i| (*i).to_owned()).collect(),
        target,
        definition: Some(base.to_owned()),
    };
    Some(match base {
        "json" | "jsonb" => made("Map<String, Object>", "Map<String, Any>", &["java.util.Map"], hibernate("json.JsonType")),
        "hstore" => made("Map<String, String>", "Map<String, String>", &["java.util.Map"], hibernate("basic.PostgreSQLHStoreType")),
        "inet" => made("Inet", "Inet", &["io.hypersistence.utils.hibernate.type.basic.Inet"], hibernate("basic.PostgreSQLInetType")),
        "interval" => made("Duration", "Duration", &["java.time.Duration"], hibernate("interval.PostgreSQLIntervalType")),
        _ => return None,
    })
}

pub(super) fn base_type(raw: &str) -> String {
    raw.split('(').next().unwrap_or(raw).trim().to_ascii_lowercase()
}

pub(super) fn offers(items: &mut Vec<PickItem>, has_references: bool, has_superclass: bool) {
    if has_references {
        items.push(PickItem::new("basicReferences", "Replace ORM references with basic attributes").description("Foreign key columns stay plain attributes (customer_id -> Long customerId); no relations, inverse sides or join tables"));
    }
    items.push(PickItem::new("editNames", "Edit attribute names").description("One prompt per mapped column"));
    items.push(PickItem::new("typeOverrides", "Override SQL-to-Java type mappings").description("Java attribute type, JPA attribute converter or Hibernate custom type"));
    if has_superclass {
        items.push(PickItem::new("mappedSuperclass", "Extend a @MappedSuperclass").description("Parent chosen per table; same-named attributes are inherited"));
    }
}

fn qualified(text: &str) -> Result<(String, Vec<String>), Error> {
    let text = text.trim();
    if text.is_empty() || !text.split('.').all(identifier) {
        return Err(format!("`{text}` is not a valid class name.").into());
    }
    let simple = simple_name(text).to_owned();
    let imports = if text.contains('.') && !text.starts_with("java.lang.") { vec![text.to_owned()] } else { Vec::new() };
    Ok((simple, imports))
}

fn column_default(parsed: &ParsedDdl, ti: usize, dialect: SqlDialect, options: &Options, selected: &[usize]) -> Option<Vec<(String, String)>> {
    let found = gate(parsed, ti, dialect, options).ok()?;
    let linked: HashMap<usize, String> = selected.iter().map(|&t| (t, default_class(&parsed.snapshot.tables[t], options))).collect();
    let wanted = |column: &Column| column.name.eq_ignore_ascii_case(found.key) || options.columns.get(&ti).is_none_or(|set| set.contains(&column.name.to_ascii_lowercase()));
    let names = found
        .columns
        .iter()
        .filter(|c| wanted(c))
        .filter_map(|c| column_attr(parsed, ti, dialect, options, &found, c, &linked).ok().map(|b| (c.name.clone(), b.attr.name)))
        .collect();
    Some(names)
}

pub(super) fn prompt_names(parsed: &ParsedDdl, dialect: SqlDialect, selected: &[usize], options: &mut Options) -> Result<(), Error> {
    if !options.edit_names {
        return Ok(());
    }
    for &ti in selected {
        let Some(names) = column_default(parsed, ti, dialect, options, selected) else { continue };
        for (column, default) in names {
            let title = format!("Attribute name for {}.{column}", parsed.snapshot.tables[ti].name);
            let chosen = host::input(&title, Some(&default))?.ok_or(Error::Cancelled)?.trim().to_owned();
            if !identifier(&chosen) {
                return Err(format!("`{chosen}` is not a valid attribute name.").into());
            }
            if chosen != default {
                options.names.insert((ti, column.to_ascii_lowercase()), chosen);
            }
        }
    }
    Ok(())
}

pub(super) fn prompt_overrides(parsed: &ParsedDdl, dialect: SqlDialect, selected: &[usize], options: &mut Options) -> Result<(), Error> {
    if !options.type_overrides {
        return Ok(());
    }
    let mut by_type: BTreeMap<String, Vec<(usize, String)>> = BTreeMap::new();
    for &ti in selected {
        let Ok(found) = gate(parsed, ti, dialect, options) else { continue };
        let table = &parsed.snapshot.tables[ti].name;
        for column in found.columns {
            let wanted = options.columns.get(&ti).is_none_or(|set| set.contains(&column.name.to_ascii_lowercase()));
            let enumerated = parsed.column_enums.iter().any(|e| e.table == *table && e.column == column.name);
            if !wanted || enumerated || column.name.eq_ignore_ascii_case(found.key) {
                continue;
            }
            let raw = match &column.sql_type {
                Fact::Known(ty) => Some(ty.raw.clone()),
                Fact::Unknown(_) => unmapped_text(parsed, table, &column.name).map(str::to_owned),
            };
            if let Some(raw) = raw {
                by_type.entry(base_type(&raw)).or_default().push((ti, column.name.to_ascii_lowercase()));
            }
        }
    }
    if by_type.is_empty() {
        return Ok(());
    }
    let items: Vec<PickItem> = by_type.iter().map(|(base, columns)| PickItem::new(base, base).description(&format!("{} column(s)", columns.len()))).collect();
    let chosen = host::pick_many("SQL types to override (the key columns are not overridden)", &items)?.ok_or(Error::Cancelled)?;
    for base in chosen {
        let columns = by_type.get(&base).ok_or(Error::Stale)?;
        let kinds = [
            PickItem::new("javaType", "Java attribute type"),
            PickItem::new("converter", "JPA attribute converter").description("@Convert(converter = ...)"),
            PickItem::new("hibernateType", "Hibernate custom type").description("@Type(...); Hibernate 6.2+ only"),
        ];
        let kind = host::pick(&format!("Override target for SQL type {base}"), &kinds)?.ok_or(Error::Cancelled)?;
        let ask = |title: String| -> Result<String, Error> { Ok(host::input(&title, None)?.ok_or(Error::Cancelled)?) };
        let made = match kind.as_str() {
            "javaType" => {
                let (java, imports) = qualified(&ask(format!("Java type for SQL type {base} (fully qualified unless java.lang)"))?)?;
                TypeOverride { kotlin: java.clone(), java, imports, target: Target::Plain, definition: None }
            }
            "converter" | "hibernateType" => {
                let hibernate = kind == "hibernateType";
                if hibernate && !options.hibernate6 {
                    return Err("A Hibernate custom type needs Hibernate 6.2+ proven in the project.".into());
                }
                let label = if hibernate { "Hibernate type class" } else { "Converter class" };
                let class_text = ask(format!("{label} for SQL type {base} (fully qualified)"))?;
                let class = class_text.trim().to_owned();
                if !class.contains('.') {
                    return Err("The class name must be fully qualified.".into());
                }
                qualified(&class)?;
                let (java, mut imports) = qualified(&ask(format!("Java attribute type of {base} columns (fully qualified unless java.lang)"))?)?;
                imports.retain(|i| *i != class);
                TypeOverride { kotlin: java.clone(), java, imports, target: if hibernate { Target::Hibernate(class) } else { Target::Converter(class) }, definition: None }
            }
            _ => return Err(Error::Stale),
        };
        for key in columns {
            options.overrides.insert(key.clone(), made.clone());
        }
    }
    Ok(())
}

pub(super) fn prompt_suggestion(options: &mut Options, key: (usize, String), raw: &str, table: &str, column: &str) -> Result<bool, Error> {
    let Some(found) = suggestion(&base_type(raw)).filter(|_| options.hibernate_types) else { return Ok(false) };
    let Target::Hibernate(class) = &found.target else { return Ok(false) };
    let items = [
        PickItem::new("suggested", &format!("Hibernate Types: {} ({})", simple_name(class), found.java)).description("Suggested by the hypersistence-utils library found in the build files"),
        PickItem::new("other", "Decide otherwise"),
    ];
    let chosen = host::pick(&format!("Column {column} of {table} has the unmapped SQL type {raw}"), &items)?.ok_or(Error::Cancelled)?;
    if chosen == "suggested" {
        options.overrides.insert(key, found);
        return Ok(true);
    }
    Ok(false)
}

#[derive(Clone, Debug)]
pub(super) struct SuperAttr {
    pub name: String,
    pub ty: String,
    pub id: bool,
    pub column: String,
}

#[derive(Clone, Debug)]
pub(super) struct Super {
    pub name: String,
    pub package: Option<String>,
    pub attrs: Vec<SuperAttr>,
}

pub(super) fn mapped_superclasses(project: &Project, files: &[String]) -> Vec<Super> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut found = Vec::new();
    for file in files.iter().take(300) {
        if Instant::now() >= deadline {
            break;
        }
        let Ok(text) = project.read_file(file) else { continue };
        if text.len() > MAX_TEXT || !text.contains("MappedSuperclass") {
            continue;
        }
        let Some(class) = parse_class(&text, file).filter(|c| c.kind == Kind::MappedSuperclass) else { continue };
        let attrs = class
            .attributes
            .iter()
            .filter(|a| a.relation == crate::model::Relation::None)
            .map(|a| SuperAttr {
                name: a.name.clone(),
                ty: a.ty.clone(),
                id: a.id,
                column: a.column.clone().unwrap_or_else(|| crate::file_actions::naming_column_name(&a.name)),
            })
            .collect();
        found.push(Super { name: class.name, package: class.package, attrs });
    }
    found.sort_by(|a, b| (&a.package, &a.name).cmp(&(&b.package, &b.name)));
    found
}

impl Super {
    pub(super) fn qualified(&self) -> String {
        self.package.as_ref().map_or(self.name.clone(), |p| format!("{p}.{}", self.name))
    }
}

pub(super) fn prompt_superclasses(parsed: &ParsedDdl, dialect: SqlDialect, selected: &[usize], candidates: &[Super], options: &mut Options) -> Result<(), Error> {
    if !options.mapped_superclass {
        return Ok(());
    }
    for &ti in selected {
        if options.parents.contains_key(&ti) || gate(parsed, ti, dialect, options).is_err() {
            continue;
        }
        let mut items = vec![PickItem::new("\u{0}none", "No parent")];
        for candidate in candidates {
            let attrs: Vec<&str> = candidate.attrs.iter().map(|a| a.name.as_str()).collect();
            items.push(PickItem::new(&candidate.qualified(), &candidate.name).description(&attrs.join(", ")));
        }
        let chosen = host::pick(&format!("Parent @MappedSuperclass for table {}", parsed.snapshot.tables[ti].name), &items)?.ok_or(Error::Cancelled)?;
        if chosen == "\u{0}none" {
            continue;
        }
        let found = candidates.iter().find(|c| c.qualified() == chosen).ok_or(Error::Stale)?;
        options.superclasses.insert(ti, found.clone());
    }
    Ok(())
}

/// Removes the attributes the superclass already declares (same name and type). Returns the column overrides for
/// inherited attributes whose column differs, and the warnings for inherited attributes the table lacks.
pub(super) fn inherit(attrs: &mut Vec<Attr>, parent: &Super, kotlin: bool, notes: &mut Vec<String>) -> Result<Vec<(String, String)>, String> {
    let mut overrides = Vec::new();
    for inherited in &parent.attrs {
        let Some(at) = attrs.iter().position(|a| a.name == inherited.name) else {
            notes.push(format!(
                "warning: inherited attribute `{}` of {} (column `{}`) has no matching column in the table",
                inherited.name, parent.name, inherited.column
            ));
            continue;
        };
        let own = &attrs[at];
        let same_type = own.target.is_none() && (own.java == inherited.ty || kotlin && own.kotlin == inherited.ty);
        if !same_type || own.key != inherited.id {
            return Err(format!("attribute `{}` of the table differs from the one inherited from {} (type {}, id {})", own.name, parent.name, inherited.ty, inherited.id));
        }
        let own = attrs.remove(at);
        if !own.column.eq_ignore_ascii_case(&inherited.column) {
            overrides.push((own.name, literal(&own.column, own.quoted)));
        }
    }
    Ok(overrides)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const SQL: &str = "CREATE TABLE customers (id bigserial PRIMARY KEY, created_at timestamp, name varchar(40) NOT NULL);\n\
        CREATE TABLE orders (id bigserial PRIMARY KEY, customer_id bigint NOT NULL REFERENCES customers (id), created_at timestamp, price numeric(8,2));\n\
        CREATE TABLE docs (id bigserial PRIMARY KEY, body jsonb);\n";
    const NS: &str = "jakarta.persistence";

    fn rendered(options: &Options, table: &str) -> (String, Plan) {
        let parsed = ddl_parse::parse_ddl(SQL, SqlDialect::PostgreSql).unwrap();
        let selected: Vec<usize> = (0..3).collect();
        let built = build_with(&parsed, SqlDialect::PostgreSql, &selected, options).unwrap();
        let plan = built.plans.into_iter().find(|p| p.table == table).unwrap();
        (render(&plan, "shop.db", NS, false), plan)
    }
    fn at(name: &str) -> usize {
        ddl_parse::parse_ddl(SQL, SqlDialect::PostgreSql).unwrap().snapshot.tables.iter().position(|t| t.name == name).unwrap()
    }
    fn base() -> Super {
        Super {
            name: "Auditable".into(),
            package: Some("shop.base".into()),
            attrs: vec![
                SuperAttr { name: "id".into(), ty: "Long".into(), id: true, column: "id".into() },
                SuperAttr { name: "createdAt".into(), ty: "LocalDateTime".into(), id: false, column: "created".into() },
                SuperAttr { name: "version".into(), ty: "Long".into(), id: false, column: "version".into() },
            ],
        }
    }

    #[test]
    fn basic_references_keep_foreign_keys_as_plain_attributes() {
        let _host = MockHost::install();
        let (text, plan) = rendered(&Options { basic_references: true, ..Options::default() }, "orders");
        assert!(text.contains("private Long customerId;") && !text.contains("ManyToOne"), "{text}");
        assert!(plan.attrs.iter().all(|a| a.target.is_none()));
        let (customers, _) = rendered(&Options { basic_references: true, ..Options::default() }, "customers");
        assert!(!customers.contains("OneToMany"), "{customers}");
        let (linked, _) = rendered(&Options::default(), "orders");
        assert!(linked.contains("private Customers customer;"), "{linked}");
    }

    #[test]
    fn edited_attribute_names_replace_the_derived_ones() {
        let _host = MockHost::install();
        let options = Options { names: HashMap::from([((at("orders"), "price".to_owned()), "amount".to_owned())]), ..Options::default() };
        let (text, _) = rendered(&options, "orders");
        assert!(text.contains("private BigDecimal amount;") && text.contains("@Column(name = \"price\"") && !text.contains("BigDecimal price"), "{text}");
    }

    #[test]
    fn override_targets_java_type_converter_and_hibernate_type() {
        let _host = MockHost::install();
        let target = |t| TypeOverride { java: "Instant".into(), kotlin: "Instant".into(), imports: vec!["java.time.Instant".into()], target: t, definition: None };
        let java = Options { overrides: HashMap::from([((at("orders"), "created_at".to_owned()), target(Target::Plain))]), ..Options::default() };
        let (text, _) = rendered(&java, "orders");
        assert!(text.contains("import java.time.Instant;") && text.contains("private Instant createdAt;") && !text.contains("@Convert"), "{text}");
        let converter = Options { overrides: HashMap::from([((at("orders"), "created_at".to_owned()), target(Target::Converter("shop.conv.InstantConverter".into())))]), ..Options::default() };
        let (text, _) = rendered(&converter, "orders");
        assert!(text.contains("@Convert(converter = InstantConverter.class)") && text.contains("import jakarta.persistence.Convert;") && text.contains("import shop.conv.InstantConverter;"), "{text}");
        let custom = Options { overrides: HashMap::from([((at("orders"), "price".to_owned()), target(Target::Hibernate("shop.types.MoneyType".into())))]), ..Options::default() };
        let (text, _) = rendered(&custom, "orders");
        assert!(text.contains("@Type(MoneyType.class)") && text.contains("import org.hibernate.annotations.Type;") && text.contains("private Instant price;"), "{text}");
    }

    #[test]
    fn hibernate_types_suggestions_map_unsupported_sql_types() {
        let _host = MockHost::install();
        let found = suggestion("jsonb").unwrap();
        assert!(matches!(&found.target, Target::Hibernate(c) if c.ends_with("json.JsonType")));
        assert!(suggestion("geometry").is_none());
        let options = Options { overrides: HashMap::from([((at("docs"), "body".to_owned()), found)]), ..Options::default() };
        let (text, _) = rendered(&options, "docs");
        assert!(text.contains("@Type(JsonType.class)") && text.contains("columnDefinition = \"jsonb\"") && text.contains("private Map<String, Object> body;"), "{text}");
    }

    #[test]
    fn mapped_superclass_inherits_same_name_and_type_overrides_columns_and_warns() {
        let _host = MockHost::install();
        let options = Options { superclasses: HashMap::from([(at("orders"), base())]), ..Options::default() };
        let (text, plan) = rendered(&options, "orders");
        assert!(text.contains("public class Orders extends Auditable {") && text.contains("import shop.base.Auditable;"), "{text}");
        assert!(!text.contains("private Long id;") && !text.contains("private LocalDateTime createdAt;"), "{text}");
        assert!(text.contains("@AttributeOverride(name = \"createdAt\", column = @Column(name = \"created_at\"))"), "{text}");
        assert!(plan.notes.iter().any(|n| n.contains("`version`") && n.contains("no matching column")), "{:?}", plan.notes);
    }

    #[test]
    fn mapped_superclass_with_a_different_type_refuses_the_table() {
        let _host = MockHost::install();
        let mut parent = base();
        parent.attrs[1].ty = "String".into();
        let parsed = ddl_parse::parse_ddl(SQL, SqlDialect::PostgreSql).unwrap();
        let options = Options { superclasses: HashMap::from([(at("orders"), parent)]), ..Options::default() };
        let built = build_with(&parsed, SqlDialect::PostgreSql, &[at("customers"), at("orders")], &options);
        let built = built.unwrap();
        assert!(built.plans.iter().all(|p| p.table != "orders") && built.refused.iter().any(|r| r.contains("differs from the one inherited")), "{:?}", built.refused);
    }
}
