//! Source for the New Entity and New Attribute wizards.

use crate::generate::callback_import;
use persistence_core::generation::InheritanceDraft;
use crate::model::{Attribute, Class, Edit, Relation, capitalize, decapitalize, import_edit};

pub struct EntitySpec {
    pub role: persistence_core::PersistenceRole,
    pub name: String,
    pub table: String,
    pub package: Option<String>,
    pub kotlin: bool,
    /// `none`, `IDENTITY`, `SEQUENCE`, `AUTO` or `UUID`.
    pub id: String,
    pub auditing: bool,
    pub version: bool,
    pub equals: bool,
}

pub struct AttributeSpec {
    pub name: String,
    pub transient: bool,
    /// The Java name of the type, or the target entity of an association.
    pub ty: String,
    pub relation: Relation,
    pub enumerated: bool,
    pub not_null: bool,
    pub unique: bool,
    pub options: RelationOptions,
    /// The attribute on the owning side that this one mirrors.
    pub mapped_by: Option<String>,
    pub imports: Vec<String>,
}

const CASCADE_TYPES: [&str; 6] = ["ALL", "PERSIST", "MERGE", "REMOVE", "REFRESH", "DETACH"];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct JoinTable {
    pub name: String,
    pub join_column: String,
    pub inverse_column: String,
}

/// Optional relationship mapping choices. Every value is a literal, validated by [`RelationOptions::validate`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RelationOptions {
    pub cascade: Vec<String>,
    /// `LAZY` or `EAGER`.
    pub fetch: Option<String>,
    pub orphan_removal: bool,
    /// `Some("")` is a bare `@OrderBy` (primary key order).
    pub order_by: Option<String>,
    /// `Some("")` is a bare `@OrderColumn`.
    pub order_column: Option<String>,
    pub join_column: Option<String>,
    pub join_column_not_null: bool,
    pub join_table: Option<JoinTable>,
}

pub(crate) fn column_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value.bytes().enumerate().all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || (i > 0 && b.is_ascii_digit()))
}

pub(crate) fn order_by_items(value: &str) -> Option<Vec<String>> {
    if value.is_empty() {
        return Some(Vec::new());
    }

    let items: Vec<&str> = value.split(',').map(str::trim).collect();
    if items.len() > 4 {
        return None;
    }

    items
        .into_iter()
        .map(|item| {
            let mut words = item.split_whitespace();
            let name = words.next().filter(|n| column_name(n))?;
            match (words.next().map(str::to_ascii_uppercase), words.next()) {
                (None, None) => Some(name.to_owned()),
                (Some(direction), None) if direction == "ASC" || direction == "DESC" => Some(format!("{name} {direction}")),
                _ => None,
            }
        })
        .collect()
}

impl RelationOptions {
    pub fn validate(&self, relation: Relation, transient: bool, mapped_by: bool) -> Result<(), &'static str> {
        if *self == Self::default() {
            return Ok(());
        }

        if transient || !matches!(relation, Relation::ManyToOne | Relation::OneToOne | Relation::OneToMany | Relation::ManyToMany) {
            return Err("Relationship options apply only to ManyToOne, OneToOne, OneToMany and ManyToMany attributes.");
        }

        let mut seen = Vec::new();
        for kind in &self.cascade {
            if !CASCADE_TYPES.contains(&kind.as_str()) || seen.contains(&kind) {
                return Err("Choose each supported cascade type at most once.");
            }
            seen.push(kind);
        }
        if self.cascade.len() > 1 && seen.iter().any(|k| *k == "ALL") {
            return Err("Cascade ALL already includes every other cascade type.");
        }

        if self.fetch.as_deref().is_some_and(|f| f != "LAZY" && f != "EAGER") {
            return Err("Choose LAZY or EAGER fetch.");
        }

        if self.orphan_removal && !matches!(relation, Relation::OneToOne | Relation::OneToMany) {
            return Err("Orphan removal applies only to OneToOne and OneToMany.");
        }

        if self.order_by.is_some() || self.order_column.is_some() {
            if !matches!(relation, Relation::OneToMany | Relation::ManyToMany) {
                return Err("Ordering applies only to OneToMany and ManyToMany collections.");
            }
            if self.order_by.is_some() && self.order_column.is_some() {
                return Err("Choose either @OrderBy or @OrderColumn, not both.");
            }
        }
        if self.order_by.as_deref().is_some_and(|o| order_by_items(o).is_none()) {
            return Err("Order by takes up to four property names, each optionally followed by ASC or DESC.");
        }
        if self.order_column.as_deref().is_some_and(|c| !c.is_empty() && !column_name(c)) {
            return Err("Choose a literal order column name.");
        }
        if self.order_column.is_some() && mapped_by {
            return Err("@OrderColumn belongs on the owning side of the relationship.");
        }

        if self.join_column.is_some() || self.join_column_not_null {
            if !relation.to_one() {
                return Err("@JoinColumn applies only to ManyToOne and OneToOne.");
            }
            if mapped_by {
                return Err("The mappedBy side takes no @JoinColumn.");
            }
            if !self.join_column.as_deref().is_some_and(column_name) {
                return Err("Choose a literal join column name.");
            }
        }

        if let Some(table) = &self.join_table {
            if relation != Relation::ManyToMany {
                return Err("@JoinTable applies only to ManyToMany.");
            }
            if mapped_by {
                return Err("The mappedBy side takes no @JoinTable.");
            }
            let names = [&table.name, &table.join_column, &table.inverse_column];
            if !names.iter().all(|n| column_name(n)) {
                return Err("Choose literal join table and join column names.");
            }
            if table.join_column == table.inverse_column {
                return Err("The join and inverse join columns must differ.");
            }
        }

        Ok(())
    }
}

pub(crate) fn plural(name: &str) -> String {
    if let Some(stem) = name.strip_suffix('y').filter(|s| s.ends_with(|c: char| !"aeiou".contains(c))) {
        return format!("{stem}ies");
    }

    if ["s", "x", "z", "ch", "sh"].iter().any(|e| name.ends_with(e)) {
        return format!("{name}es");
    }

    format!("{name}s")
}

/// The attribute that makes `attribute` of `owner` bidirectional, for `target`; `None` when `target` already mirrors it.
pub fn inverse_spec(owner: &Class, attribute: &Attribute, target: &Class) -> Option<AttributeSpec> {
    let relation = match attribute.relation {
        Relation::ManyToOne => Relation::OneToMany,
        Relation::OneToOne => Relation::OneToOne,
        Relation::ManyToMany => Relation::ManyToMany,
        _ => return None,
    };

    if attribute.mapped_by || target.attributes.iter().any(|a| a.target == owner.name && a.mapped_by) {
        return None;
    }

    let single = decapitalize(&owner.name);
    let name = if relation.to_many() { plural(&single) } else { single };
    if target.attributes.iter().any(|a| a.name == name) {
        return None;
    }

    let imports = match (&owner.package, &target.package) {
        (Some(package), other) if Some(package) != other.as_ref() => vec![format!("{package}.{}", owner.name)],
        _ => Vec::new(),
    };

    Some(AttributeSpec {
        name,
        transient: false,
        ty: owner.name.clone(),
        relation,
        enumerated: false,
        not_null: false,
        unique: false,
        options: RelationOptions::default(),
        mapped_by: Some(attribute.name.clone()),
        imports,
    })
}

struct Field {
    annotations: Vec<String>,
    ty: String,
    name: String,
    init: Option<String>,
}

const PERSISTENCE: &str = "jakarta.persistence";

fn kotlin_type(ty: &str) -> &str {
    match ty {
        "Integer" => "Int",
        other => other,
    }
}

fn type_import(ty: &str) -> Option<&'static str> {
    match ty {
        "BigDecimal" => Some("java.math.BigDecimal"),
        "LocalDate" => Some("java.time.LocalDate"),
        "LocalDateTime" => Some("java.time.LocalDateTime"),
        "Instant" => Some("java.time.Instant"),
        "UUID" => Some("java.util.UUID"),
        _ => None,
    }
}

fn render(field: &Field, kotlin: bool, accessors: bool) -> String {
    render_with_openness(field, kotlin, accessors, false)
}

fn render_with_openness(field: &Field, kotlin: bool, accessors: bool, open: bool) -> String {
    render_accessors(field, kotlin, accessors, accessors, open)
}

fn render_accessors(field: &Field, kotlin: bool, getter: bool, setter: bool, open: bool) -> String {
    let mut out: String = field.annotations.iter().map(|a| format!("    {a}\n")).collect();
    let Field { ty, name, init, .. } = field;

    if kotlin {
        let modifier = if open { "open " } else { "" };
        match init {
            Some(init) => out.push_str(&format!("    {modifier}var {name}: {ty} = {init}")),
            None => out.push_str(&format!("    {modifier}var {name}: {ty}? = null")),
        }
        return out;
    }

    match init {
        Some(init) => out.push_str(&format!("    private {ty} {name} = {init};")),
        None => out.push_str(&format!("    private {ty} {name};")),
    }

    let cap = capitalize(name);
    if getter {
        out.push_str(&format!("\n\n    public {ty} get{cap}() {{\n        return {name};\n    }}"));
    }
    if setter {
        out.push_str(&format!("\n\n    public void set{cap}({ty} {name}) {{\n        this.{name} = {name};\n    }}"));
    }

    out
}

/// One plain field in the wizard's own layout (Java accessors when `accessors`, Kotlin `var`).
pub(crate) fn plain_field(annotations: Vec<String>, ty: &str, name: &str, kotlin: bool, accessors: bool) -> String {
    render(&Field { annotations, ty: ty.to_owned(), name: name.to_owned(), init: None }, kotlin, accessors)
}

pub fn entity_source(spec: &EntitySpec) -> String {
    entity_source_with(spec, false)
}

/// Lombok annotations a new Java entity can carry, in the order they are written.
pub const LOMBOK_ENTITY_ANNOTATIONS: [&str; 5] = ["Getter", "Setter", "NoArgsConstructor", "AllArgsConstructor", "ToString"];

/// `lombok` replaces generated Java accessors with `@Getter`, `@Setter` and `@NoArgsConstructor` on entities.
pub fn entity_source_with(spec: &EntitySpec, lombok: bool) -> String {
    entity_source_annotated(spec, if lombok { &LOMBOK_ENTITY_ANNOTATIONS[..3] } else { &[] })
}

/// `lombok_annotations` (names from `LOMBOK_ENTITY_ANNOTATIONS`) are written on a Java entity; generated accessors cover
/// only what `@Getter` and `@Setter` do not.
pub fn entity_source_annotated(spec: &EntitySpec, lombok_annotations: &[&str]) -> String {
    let kotlin = spec.kotlin;
    let name = &spec.name;
    let persistence = |what: &str| format!("{PERSISTENCE}.{what}");
    let entity = spec.role == persistence_core::PersistenceRole::Entity;
    let lombok_annotations: Vec<&str> = if entity && !kotlin {
        LOMBOK_ENTITY_ANNOTATIONS.into_iter().filter(|a| lombok_annotations.contains(a)).collect()
    } else {
        Vec::new()
    };
    let (getter, setter) = (lombok_annotations.contains(&"Getter"), lombok_annotations.contains(&"Setter"));
    let mut imports = vec![persistence(spec.role.annotation())];
    let mut class_annotations = vec![format!("@{}", spec.role.annotation())];
    if entity {
        imports.extend([persistence("Table"), persistence("Id")]);
        class_annotations.push(format!("@Table(name = \"{}\")", spec.table));
    }
    for annotation in &lombok_annotations {
        imports.push(format!("lombok.{annotation}"));
        class_annotations.push(format!("@{annotation}"));
    }
    let mut fields = Vec::new();

    let id_type = if spec.id == "UUID" { "UUID" } else { "Long" };
    let mut id_annotations = vec!["@Id".to_owned()];
    if entity && spec.id != "none" {
        imports.extend([persistence("GeneratedValue"), persistence("GenerationType")]);
        id_annotations.push(format!("@GeneratedValue(strategy = GenerationType.{})", spec.id));
    }
    if entity {
        imports.extend(type_import(id_type).map(str::to_owned));
        fields.push(Field { annotations: id_annotations, ty: id_type.into(), name: "id".into(), init: None });
    }

    if spec.version {
        imports.push(persistence("Version"));
        fields.push(Field { annotations: vec!["@Version".into()], ty: "Long".into(), name: "version".into(), init: None });
    }

    if spec.auditing {
        imports.extend([
            persistence("Column"),
            persistence("EntityListeners"),
            "org.springframework.data.annotation.CreatedDate".into(),
            "org.springframework.data.annotation.LastModifiedDate".into(),
            "org.springframework.data.jpa.domain.support.AuditingEntityListener".into(),
            "java.time.Instant".into(),
        ]);
        let listener = if kotlin { "AuditingEntityListener::class" } else { "AuditingEntityListener.class" };
        class_annotations.push(format!("@EntityListeners({listener})"));
        fields.push(Field {
            annotations: vec!["@CreatedDate".into(), "@Column(updatable = false)".into()],
            ty: "Instant".into(),
            name: "createdAt".into(),
            init: None,
        });
        fields.push(Field { annotations: vec!["@LastModifiedDate".into()], ty: "Instant".into(), name: "updatedAt".into(), init: None });
    }

    let mut members: Vec<String> = fields.iter().map(|f| render_accessors(f, kotlin, !getter, !setter, false)).collect();

    if spec.equals && entity {
        members.push(equals_and_hash_code(name, kotlin));
    }

    imports.sort();
    imports.dedup();
    let semicolon = if kotlin { "" } else { ";" };
    let package = spec.package.as_ref().map(|p| format!("package {p}{semicolon}\n\n")).unwrap_or_default();
    let imports: String = imports.iter().map(|i| format!("import {i}{semicolon}\n")).collect();
    let annotations = class_annotations.join("\n");
    let header = if kotlin { format!("class {name}") } else { format!("public class {name}") };

    format!("{package}{imports}\n{annotations}\n{header} {{\n\n{}\n}}\n", members.join("\n\n"))
}

fn equals_and_hash_code(name: &str, kotlin: bool) -> String {
    if kotlin {
        return format!(
            "    override fun equals(other: Any?): Boolean {{\n        if (this === other) return true\n        if (other == null || other::class != this::class) return false\n        other as {name}\n        return id != null && id == other.id\n    }}\n\n    override fun hashCode(): Int = this::class.hashCode()"
        );
    }

    format!(
        "    @Override\n    public boolean equals(Object o) {{\n        if (this == o) {{\n            return true;\n        }}\n        if (o == null || getClass() != o.getClass()) {{\n            return false;\n        }}\n        {name} other = ({name}) o;\n        return id != null && id.equals(other.id);\n    }}\n\n    @Override\n    public int hashCode() {{\n        return getClass().hashCode();\n    }}"
    )
}

/// Fully qualified generated names avoid guessing imports or borrowing a local
/// annotation/type. Configuration is validated before any source is rendered.
pub fn identifier_source(
    id: &persistence_core::generation::IdentifierDraft,
    namespace: persistence_core::generation::Namespace,
    kotlin: bool,
    accessors: bool,
    open: bool,
) -> Result<String, &'static str> {
    id.validate(namespace)?;
    let ns = namespace.package();
    let mut annotations = vec![format!("@{ns}.Id")];
    if let Some(strategy) = id.strategy.constant() {
        let generator = id.sequence.as_ref().map(|s| s.generator.as_str()).or_else(|| id.table.as_ref().map(|t| t.generator.as_str()));
        let generator = generator.map(|g| format!(", generator = \"{g}\"")).unwrap_or_default();
        annotations.push(format!("@{ns}.GeneratedValue(strategy = {ns}.GenerationType.{strategy}{generator})"));
    }
    if let Some(s) = &id.sequence {
        annotations.push(format!("@{ns}.SequenceGenerator(name = \"{}\", sequenceName = \"{}\", initialValue = {}, allocationSize = {})", s.generator, s.sequence_name, s.initial_value, s.allocation_size));
    }
    if let Some(t) = &id.table {
        annotations.push(format!("@{ns}.TableGenerator(name = \"{}\", table = \"{}\", pkColumnName = \"{}\", valueColumnName = \"{}\", pkColumnValue = \"{}\", initialValue = {}, allocationSize = {})", t.generator, t.table_name, t.pk_column_name, t.value_column_name, t.pk_column_value, t.initial_value, t.allocation_size));
    }
    let name = if id.column.is_empty() { String::new() } else { format!("name = \"{}\", ", id.column) };
    annotations.push(format!("@{ns}.Column({name}nullable = false)"));
    Ok(render_with_openness(&Field { annotations, ty: id.id_type.source_type(kotlin).into(), name: id.name.clone(), init: None }, kotlin, accessors, open))
}

fn inheritance_annotations(inheritance: &InheritanceDraft, ns: &str) -> String {
    let mut out = String::new();
    match inheritance {
        InheritanceDraft::Root { strategy, discriminator, discriminator_value } => {
            out.push_str(&format!("@{ns}.Inheritance(strategy = {ns}.InheritanceType.{})\n", strategy.constant()));
            if let Some(column) = discriminator {
                let mut args = vec![
                    format!("name = \"{}\"", column.name),
                    format!("discriminatorType = {ns}.DiscriminatorType.{}", column.discriminator_type.constant()),
                ];
                if column.length > 0 {
                    args.push(format!("length = {}", column.length));
                }
                out.push_str(&format!("@{ns}.DiscriminatorColumn({})\n", args.join(", ")));
            }
            if let Some(value) = discriminator_value {
                out.push_str(&format!("@{ns}.DiscriminatorValue(\"{value}\")\n"));
            }
        }
        InheritanceDraft::Subclass { discriminator_value, primary_key_join_column, .. } => {
            if let Some(value) = discriminator_value {
                out.push_str(&format!("@{ns}.DiscriminatorValue(\"{value}\")\n"));
            }
            if let Some(column) = primary_key_join_column {
                out.push_str(&format!("@{ns}.PrimaryKeyJoinColumn(name = \"{column}\")\n"));
            }
        }
    }
    out
}

pub fn configured_class_source(
    draft: &persistence_core::generation::ManagedClassDraft,
    package: &str,
    kotlin: bool,
) -> Result<String, &'static str> {
    draft.validate()?;
    if package.len() > 512 || !package.split('.').all(persistence_core::generation::identifier) {
        return Err("Choose a named source package.");
    }
    let ns = draft.namespace.package();
    let end = if kotlin { "" } else { ";" };
    let mut source = format!("package {package}{end}\n\n@{ns}.{}\n", draft.role.annotation());
    if draft.role == persistence_core::PersistenceRole::Entity && !draft.table_name.is_empty() { source.push_str(&format!("@{ns}.Table(name = \"{}\")\n", draft.table_name)); }
    if let Some(inheritance) = &draft.inheritance { source.push_str(&inheritance_annotations(inheritance, ns)); }
    let open = kotlin && draft.role != persistence_core::PersistenceRole::Embeddable;
    let extends = match &draft.inheritance {
        Some(InheritanceDraft::Subclass { superclass, .. }) if kotlin => format!(" : {superclass}()"),
        Some(InheritanceDraft::Subclass { superclass, .. }) => format!(" extends {superclass}"),
        _ => String::new(),
    };
    source.push_str(&format!("{}class {}{extends} {{\n\n", if open { "open " } else if kotlin { "" } else { "public " }, draft.name));
    if let Some(id) = &draft.identifier { source.push_str(&identifier_source(id, draft.namespace, kotlin, true, open)?); source.push('\n'); }
    if draft.versioned {
        let field = Field { annotations:vec![format!("@{ns}.Version")], ty:if kotlin {"kotlin.Long"} else {"java.lang.Long"}.into(), name:"version".into(),init:None };
        source.push_str(&format!("\n{}\n", render_with_openness(&field, kotlin, true, open)));
    }
    source.push_str("}\n");
    Ok(source)
}

/// The field (and its imports) at the end of the class body; `None` when the class has no body.
pub fn attribute_edits(class: &Class, text: &str, masked: &str, spec: &AttributeSpec) -> Option<Vec<Edit>> {
    let at = class.body_end?;
    spec.options.validate(spec.relation, spec.transient, spec.mapped_by.is_some()).ok()?;
    let kotlin = class.kotlin;
    let mut imports: Vec<String> = Vec::new();
    let mut annotations = Vec::new();
    let mut ty = spec.ty.clone();
    let mut init = None;

    let mut annotate = |annotation: &str, args: Vec<String>, imports: &mut Vec<String>| {
        imports.push(callback_import(class, annotation));
        annotations.push(if args.is_empty() { format!("@{annotation}") } else { format!("@{annotation}({})", args.join(", ")) });
    };

    if spec.transient {
        annotate("Transient", Vec::new(), &mut imports);
        imports.extend(type_import(&ty).map(str::to_owned));
    } else if spec.relation == Relation::None {
        let mut args = Vec::new();
        if spec.not_null {
            args.push("nullable = false".to_owned());
        }
        if spec.unique {
            args.push("unique = true".to_owned());
        }
        if !args.is_empty() {
            annotate("Column", args, &mut imports);
        }
        if spec.enumerated {
            imports.push(callback_import(class, "EnumType"));
            annotate("Enumerated", vec!["EnumType.STRING".into()], &mut imports);
        }
        imports.extend(type_import(&ty).map(str::to_owned));
    } else {
        let mut args: Vec<String> = spec.mapped_by.iter().map(|m| format!("mappedBy = \"{m}\"")).collect();
        let options = &spec.options;
        if let Some(kind) = &options.fetch {
            imports.push(callback_import(class, "FetchType"));
            args.push(format!("fetch = FetchType.{kind}"));
        }
        if !options.cascade.is_empty() {
            imports.push(callback_import(class, "CascadeType"));
            let kinds: Vec<String> = options.cascade.iter().map(|k| format!("CascadeType.{k}")).collect();
            args.push(match (kinds.as_slice(), kotlin) {
                (_, true) => format!("cascade = [{}]", kinds.join(", ")),
                ([one], false) => format!("cascade = {one}"),
                (_, false) => format!("cascade = {{{}}}", kinds.join(", ")),
            });
        }
        if options.orphan_removal {
            args.push("orphanRemoval = true".into());
        }
        if spec.not_null && spec.relation.to_one() {
            args.push("optional = false".into());
        }
        let relation = format!("{:?}", spec.relation);
        annotate(&relation, args, &mut imports);

        if let Some(column) = &options.join_column {
            imports.push(callback_import(class, "JoinColumn"));
            let mut args = vec![format!("name = \"{column}\"")];
            if options.join_column_not_null {
                args.push("nullable = false".into());
            }
            annotate("JoinColumn", args, &mut imports);
        }
        if let Some(table) = &options.join_table {
            imports.extend([callback_import(class, "JoinTable"), callback_import(class, "JoinColumn")]);
            let column = |name: &str| {
                let nested = format!("JoinColumn(name = \"{name}\")");
                match kotlin {
                    true => format!("[{nested}]"),
                    false => format!("@{nested}"),
                }
            };
            let args = vec![
                format!("name = \"{}\"", table.name),
                format!("joinColumns = {}", column(&table.join_column)),
                format!("inverseJoinColumns = {}", column(&table.inverse_column)),
            ];
            annotate("JoinTable", args, &mut imports);
        }
        if let Some(order) = &options.order_by {
            imports.push(callback_import(class, "OrderBy"));
            let items = order_by_items(order).unwrap_or_default();
            let args = if items.is_empty() { Vec::new() } else { vec![format!("\"{}\"", items.join(", "))] };
            annotate("OrderBy", args, &mut imports);
        }
        if let Some(column) = &options.order_column {
            imports.push(callback_import(class, "OrderColumn"));
            let args = if column.is_empty() { Vec::new() } else { vec![format!("name = \"{column}\"")] };
            annotate("OrderColumn", args, &mut imports);
        }

        if spec.relation == Relation::ElementCollection && spec.enumerated {
            imports.push(callback_import(class, "EnumType"));
            annotate("Enumerated", vec!["EnumType.STRING".into()], &mut imports);
        }

        if spec.relation.to_many() {
            if kotlin && spec.relation == Relation::ElementCollection { ty = kotlin_type(&ty).to_owned(); }
            ty = if kotlin { format!("MutableList<{ty}>") } else { format!("List<{ty}>") };
            init = Some(if kotlin { "mutableListOf()" } else { "new ArrayList<>()" }.to_owned());
            if !kotlin {
                imports.extend(["java.util.List".to_owned(), "java.util.ArrayList".to_owned()]);
            }
        }
    }

    if kotlin && spec.relation == Relation::None {
        ty = kotlin_type(&ty).to_owned();
    }

    let accessors = class.attributes.iter().any(|a| crate::model::has_word(masked, &format!("get{}", capitalize(&a.name))));
    let field = Field { annotations, ty, name: spec.name.clone(), init };
    let mut edits: Vec<Edit> = {
        imports.extend(spec.imports.iter().cloned());
        let refs: Vec<&str> = imports.iter().map(String::as_str).collect();
        import_edit(text, &refs, kotlin).into_iter().collect()
    };
    edits.push(Edit::insert(at, &format!("\n{}\n", render(&field, kotlin, accessors))));

    if text.contains("\r\n") {
        edits.iter_mut().for_each(|e| e.text = e.text.replace('\n', "\r\n"));
    }

    Some(edits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{mask, parse_class};

    fn base(kotlin: bool, ns: &str) -> (String, String) {
        match kotlin {
            true => ("/p/src/shop/Order.kt".into(), format!("package shop\n\nimport {ns}.persistence.Entity\nimport {ns}.persistence.Id\n\n@Entity\nopen class Order {{\n    @Id\n    var id: Long? = null\n}}\n")),
            false => ("/p/src/shop/Order.java".into(), format!("package shop;\n\nimport {ns}.persistence.Entity;\nimport {ns}.persistence.Id;\n\n@Entity\npublic class Order {{\n    @Id\n    private Long id;\n}}\n")),
        }
    }

    fn spec(name: &str, ty: &str, relation: Relation, options: RelationOptions) -> AttributeSpec {
        AttributeSpec {
            name: name.into(),
            transient: false,
            ty: ty.into(),
            relation,
            enumerated: false,
            not_null: false,
            unique: false,
            options,
            mapped_by: None,
            imports: Vec::new(),
        }
    }

    fn render_in(kotlin: bool, ns: &str, spec: &AttributeSpec) -> Option<String> {
        let (path, text) = base(kotlin, ns);
        let class = parse_class(&text, &path).unwrap();
        let edits = attribute_edits(&class, &text, &mask(&text), spec)?;
        let edits: Vec<_> = edits.iter().map(|e| e.to_text_edit(&text)).collect();
        Some(crate::file_actions::patched(&text, &edits).unwrap())
    }

    fn cascade(kinds: &[&str]) -> RelationOptions {
        RelationOptions { cascade: kinds.iter().map(|k| (*k).into()).collect(), ..Default::default() }
    }

    #[test]
    #[ignore = "requires javac/kotlinc and JPA_JAKARTA_API / JPA_JAVAX_API (and Kotlin env vars)"]
    fn generated_relationships_compile_against_both_persistence_apis() {
        for kotlin in [false, true] {
            for ns in ["jakarta", "javax"] {
                compile_relationships(kotlin, ns);
            }
        }
    }

    fn compile_relationships(kotlin: bool, ns: &str) {
        let jar = std::env::var(if ns == "jakarta" { "JPA_JAKARTA_API" } else { "JPA_JAVAX_API" }).expect("explicit API jar path");
        let dir = std::env::temp_dir().join(format!("jpa-relationship-compile-{}-{kotlin}-{ns}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ext = if kotlin { "kt" } else { "java" };
        let mut files = Vec::new();
        for name in ["Customer", "Tag", "Line", "Profile"] {
            let body = match kotlin {
                true => format!("package shop\n\nimport {ns}.persistence.Entity\nimport {ns}.persistence.Id\n\n@Entity\nopen class {name} {{\n    @Id\n    open var id: Long? = null\n}}\n"),
                false => format!("package shop;\n\nimport {ns}.persistence.Entity;\nimport {ns}.persistence.Id;\n\n@Entity\npublic class {name} {{\n    @Id\n    private Long id;\n}}\n"),
            };
            let path = dir.join(format!("{name}.{ext}"));
            std::fs::write(&path, body).unwrap();
            files.push(path);
        }
        let join_table = JoinTable { name: "order_tag".into(), join_column: "order_id".into(), inverse_column: "tag_id".into() };
        let specs = vec![
            spec("customer", "Customer", Relation::ManyToOne, RelationOptions { fetch: Some("LAZY".into()), join_column: Some("customer_id".into()), join_column_not_null: true, ..cascade(&["PERSIST", "MERGE"]) }),
            spec("profile", "Profile", Relation::OneToOne, RelationOptions { orphan_removal: true, join_column: Some("profile_id".into()), ..cascade(&["ALL"]) }),
            spec("lines", "Line", Relation::OneToMany, RelationOptions { orphan_removal: true, order_by: Some("id DESC".into()), fetch: Some("EAGER".into()), ..cascade(&["REMOVE", "REFRESH", "DETACH"]) }),
            spec("ranked", "Line", Relation::OneToMany, RelationOptions { order_column: Some("rank_index".into()), ..Default::default() }),
            spec("tags", "Tag", Relation::ManyToMany, RelationOptions { join_table: Some(join_table), ..Default::default() }),
        ];
        let (path, mut text) = base(kotlin, ns);
        for spec in &specs {
            let class = parse_class(&text, &path).unwrap();
            let edits = attribute_edits(&class, &text, &mask(&text), spec).unwrap();
            let edits: Vec<_> = edits.iter().map(|e| e.to_text_edit(&text)).collect();
            text = crate::file_actions::patched(&text, &edits).unwrap();
        }
        let order = dir.join(format!("Order.{ext}"));
        std::fs::write(&order, &text).unwrap();
        files.push(order);
        let result = if kotlin {
            let compiler = std::env::var("KOTLIN_COMPILER_CLASSPATH").expect("explicit Kotlin compiler classpath");
            let stdlib = std::env::var("KOTLIN_STDLIB").expect("explicit Kotlin standard library jar");
            std::process::Command::new("java").args(["-classpath", &compiler, "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler", "-no-stdlib", "-no-reflect", "-jvm-target", "17", "-classpath", &format!("{jar}:{stdlib}"), "-d"]).arg(&dir).args(&files).output().expect("java installed")
        } else {
            std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &jar, "-d"]).arg(&dir).args(&files).output().expect("javac installed")
        };
        assert!(result.status.success(), "{ns} kotlin={kotlin}: {}\n{text}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "requires explicit javac/Kotlin compiler and JPA API jars"]
    fn generated_inheritance_hierarchies_compile_against_both_persistence_apis() {
        use persistence_core::{
            PersistenceRole,
            generation::{
                DiscriminatorColumn, DiscriminatorType, GenerationStrategy, IdentifierDraft, IdentifierType,
                InheritanceDraft, InheritanceStrategy, ManagedClassDraft, Namespace,
            },
        };
        for kotlin in [false, true] {
            for (ns, namespace) in [("jakarta", Namespace::Jakarta), ("javax", Namespace::Javax)] {
                for (strategy, kind, value) in [
                    (InheritanceStrategy::SingleTable, DiscriminatorType::String, "VEHICLE"),
                    (InheritanceStrategy::Joined, DiscriminatorType::Integer, "1"),
                    (InheritanceStrategy::TablePerClass, DiscriminatorType::String, ""),
                ] {
                    let table_per_class = strategy == InheritanceStrategy::TablePerClass;
                    let root = ManagedClassDraft {
                        name: "Vehicle".into(),
                        role: PersistenceRole::Entity,
                        namespace,
                        table_name: "vehicle".into(),
                        identifier: Some(IdentifierDraft {
                            name: "id".into(),
                            column: String::new(),
                            id_type: IdentifierType::Long,
                            strategy: if table_per_class { GenerationStrategy::Assigned } else { GenerationStrategy::Identity },
                            sequence: None,
                            table: None,
                        }),
                        versioned: false,
                        inheritance: Some(InheritanceDraft::Root {
                            strategy,
                            discriminator: (!table_per_class).then(|| DiscriminatorColumn {
                                name: "kind".into(),
                                discriminator_type: kind,
                                length: if kind == DiscriminatorType::String { 16 } else { 0 },
                            }),
                            discriminator_value: (!table_per_class).then(|| value.to_owned()),
                        }),
                    };
                    let subclass = ManagedClassDraft {
                        name: "Car".into(),
                        table_name: if strategy == InheritanceStrategy::SingleTable { String::new() } else { "car".into() },
                        identifier: None,
                        inheritance: Some(InheritanceDraft::Subclass {
                            superclass: "Vehicle".into(),
                            discriminator_value: (!table_per_class)
                                .then(|| if kind == DiscriminatorType::Integer { "2".to_owned() } else { "CAR".to_owned() }),
                            primary_key_join_column: (strategy == InheritanceStrategy::Joined).then(|| "vehicle_id".to_owned()),
                        }),
                        ..root.clone()
                    };
                    let jar = std::env::var(if ns == "jakarta" { "JPA_JAKARTA_API" } else { "JPA_JAVAX_API" }).expect("explicit API jar path");
                    let dir = std::env::temp_dir().join(format!("jpa-inheritance-compile-{}-{kotlin}-{ns}-{strategy:?}", std::process::id()));
                    std::fs::create_dir_all(&dir).unwrap();
                    let ext = if kotlin { "kt" } else { "java" };
                    let mut files = Vec::new();
                    for draft in [&root, &subclass] {
                        let source = super::configured_class_source(draft, "shop", kotlin).unwrap();
                        let path = dir.join(format!("{}.{ext}", draft.name));
                        std::fs::write(&path, source).unwrap();
                        files.push(path);
                    }
                    let result = if kotlin {
                        let compiler = std::env::var("KOTLIN_COMPILER_CLASSPATH").expect("explicit Kotlin compiler classpath");
                        let stdlib = std::env::var("KOTLIN_STDLIB").expect("explicit Kotlin standard library jar");
                        std::process::Command::new("java").args(["-classpath", &compiler, "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler", "-no-stdlib", "-no-reflect", "-jvm-target", "17", "-classpath", &format!("{jar}:{stdlib}"), "-d"]).arg(&dir).args(&files).output().expect("java installed")
                    } else {
                        std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &jar, "-d"]).arg(&dir).args(&files).output().expect("javac installed")
                    };
                    assert!(result.status.success(), "{ns} kotlin={kotlin} {strategy:?}: {}", String::from_utf8_lossy(&result.stderr));
                    std::fs::remove_dir_all(dir).unwrap();
                }
            }
        }
    }

    #[test]
    fn cascade_types_render_singly_and_as_arrays_in_java_and_kotlin() {
        let one = spec("customer", "Customer", Relation::ManyToOne, cascade(&["PERSIST"]));
        assert!(render_in(false, "jakarta", &one).unwrap().contains("@ManyToOne(cascade = CascadeType.PERSIST)\n    private Customer customer;"));
        let many = spec("lines", "Line", Relation::OneToMany, cascade(&["PERSIST", "MERGE", "DETACH"]));
        let java = render_in(false, "jakarta", &many).unwrap();
        assert!(java.contains("@OneToMany(cascade = {CascadeType.PERSIST, CascadeType.MERGE, CascadeType.DETACH})"), "{java}");
        assert!(java.contains("import jakarta.persistence.CascadeType;"), "{java}");
        let kotlin = render_in(true, "javax", &one).unwrap();
        assert!(kotlin.contains("@ManyToOne(cascade = [CascadeType.PERSIST])"), "{kotlin}");
        let kotlin = render_in(true, "javax", &many).unwrap();
        assert!(kotlin.contains("@OneToMany(cascade = [CascadeType.PERSIST, CascadeType.MERGE, CascadeType.DETACH])"), "{kotlin}");
        assert!(kotlin.contains("import javax.persistence.CascadeType"), "{kotlin}");
    }

    #[test]
    fn orphan_removal_fetch_and_order_render_for_collections() {
        let options = RelationOptions {
            fetch: Some("EAGER".into()),
            orphan_removal: true,
            order_by: Some("position asc, id DESC".into()),
            ..cascade(&["REMOVE", "REFRESH"])
        };
        let java = render_in(false, "javax", &spec("lines", "Line", Relation::OneToMany, options.clone())).unwrap();
        assert!(java.contains("@OneToMany(fetch = FetchType.EAGER, cascade = {CascadeType.REMOVE, CascadeType.REFRESH}, orphanRemoval = true)\n    @OrderBy(\"position ASC, id DESC\")\n    private List<Line> lines = new ArrayList<>();"), "{java}");
        assert!(java.contains("import javax.persistence.OrderBy;"), "{java}");
        let kotlin = render_in(true, "jakarta", &spec("lines", "Line", Relation::OneToMany, options)).unwrap();
        assert!(kotlin.contains("@OrderBy(\"position ASC, id DESC\")\n    var lines: MutableList<Line> = mutableListOf()"), "{kotlin}");
        let bare = RelationOptions { order_by: Some(String::new()), ..Default::default() };
        assert!(render_in(false, "jakarta", &spec("lines", "Line", Relation::ManyToMany, bare)).unwrap().contains("    @OrderBy\n    private List<Line>"));
        let one = RelationOptions { orphan_removal: true, fetch: Some("LAZY".into()), ..Default::default() };
        assert!(render_in(false, "jakarta", &spec("profile", "Profile", Relation::OneToOne, one)).unwrap().contains("@OneToOne(fetch = FetchType.LAZY, orphanRemoval = true)"));
    }

    #[test]
    fn order_column_renders_named_and_bare() {
        let named = RelationOptions { order_column: Some("line_index".into()), ..Default::default() };
        let java = render_in(false, "jakarta", &spec("lines", "Line", Relation::OneToMany, named)).unwrap();
        assert!(java.contains("@OneToMany\n    @OrderColumn(name = \"line_index\")\n    private List<Line> lines"), "{java}");
        let bare = RelationOptions { order_column: Some(String::new()), ..Default::default() };
        assert!(render_in(true, "javax", &spec("tags", "Tag", Relation::ManyToMany, bare)).unwrap().contains("    @OrderColumn\n    var tags: MutableList<Tag>"));
    }

    #[test]
    fn join_column_renders_for_owning_to_one() {
        let options = RelationOptions { join_column: Some("customer_id".into()), join_column_not_null: true, ..Default::default() };
        let mut spec = spec("customer", "Customer", Relation::ManyToOne, options);
        let java = render_in(false, "jakarta", &spec).unwrap();
        assert!(java.contains("@ManyToOne\n    @JoinColumn(name = \"customer_id\", nullable = false)\n    private Customer customer;"), "{java}");
        assert!(java.contains("import jakarta.persistence.JoinColumn;"));
        spec.options.join_column_not_null = false;
        spec.relation = Relation::OneToOne;
        assert!(render_in(true, "javax", &spec).unwrap().contains("@OneToOne\n    @JoinColumn(name = \"customer_id\")\n    var customer: Customer? = null"));
    }

    #[test]
    fn join_table_renders_nested_annotations_per_language() {
        let table = JoinTable { name: "order_tag".into(), join_column: "order_id".into(), inverse_column: "tag_id".into() };
        let options = RelationOptions { join_table: Some(table), ..Default::default() };
        let spec = spec("tags", "Tag", Relation::ManyToMany, options);
        let java = render_in(false, "jakarta", &spec).unwrap();
        assert!(java.contains("@JoinTable(name = \"order_tag\", joinColumns = @JoinColumn(name = \"order_id\"), inverseJoinColumns = @JoinColumn(name = \"tag_id\"))\n    private List<Tag> tags"), "{java}");
        assert!(java.contains("import jakarta.persistence.JoinTable;") && java.contains("import jakarta.persistence.JoinColumn;"));
        let kotlin = render_in(true, "javax", &spec).unwrap();
        assert!(kotlin.contains("@JoinTable(name = \"order_tag\", joinColumns = [JoinColumn(name = \"order_id\")], inverseJoinColumns = [JoinColumn(name = \"tag_id\")])\n    var tags: MutableList<Tag>"), "{kotlin}");
    }

    #[test]
    fn invalid_relationship_combinations_are_refused_without_edits() {
        let table = || JoinTable { name: "t".into(), join_column: "a".into(), inverse_column: "b".into() };
        let refused: Vec<(Relation, bool, RelationOptions)> = vec![
            (Relation::ManyToOne, false, RelationOptions { orphan_removal: true, ..Default::default() }),
            (Relation::ManyToMany, false, RelationOptions { orphan_removal: true, ..Default::default() }),
            (Relation::ManyToOne, false, RelationOptions { order_by: Some(String::new()), ..Default::default() }),
            (Relation::OneToOne, false, RelationOptions { order_column: Some(String::new()), ..Default::default() }),
            (Relation::OneToMany, false, RelationOptions { order_by: Some(String::new()), order_column: Some(String::new()), ..Default::default() }),
            (Relation::OneToMany, false, RelationOptions { order_by: Some("name; drop".into()), ..Default::default() }),
            (Relation::OneToMany, false, RelationOptions { order_by: Some("name SIDEWAYS".into()), ..Default::default() }),
            (Relation::OneToMany, false, RelationOptions { order_by: Some("a,b,c,d,e".into()), ..Default::default() }),
            (Relation::OneToMany, false, RelationOptions { order_column: Some("bad\"name".into()), ..Default::default() }),
            (Relation::OneToMany, true, RelationOptions { order_column: Some(String::new()), ..Default::default() }),
            (Relation::OneToMany, false, RelationOptions { join_column: Some("c".into()), ..Default::default() }),
            (Relation::ManyToOne, true, RelationOptions { join_column: Some("c".into()), ..Default::default() }),
            (Relation::ManyToOne, false, RelationOptions { join_column: Some("a b".into()), ..Default::default() }),
            (Relation::ManyToOne, false, RelationOptions { join_column_not_null: true, ..Default::default() }),
            (Relation::OneToMany, false, RelationOptions { join_table: Some(table()), ..Default::default() }),
            (Relation::ManyToMany, true, RelationOptions { join_table: Some(table()), ..Default::default() }),
            (Relation::ManyToMany, false, RelationOptions { join_table: Some(JoinTable { join_column: "a".into(), inverse_column: "a".into(), ..table() }), ..Default::default() }),
            (Relation::ManyToMany, false, RelationOptions { join_table: Some(JoinTable { name: "x y".into(), ..table() }), ..Default::default() }),
            (Relation::ManyToOne, false, cascade(&["ALL", "MERGE"])),
            (Relation::ManyToOne, false, cascade(&["MERGE", "MERGE"])),
            (Relation::ManyToOne, false, cascade(&["SAVE"])),
            (Relation::ManyToOne, false, RelationOptions { fetch: Some("SOMETIMES".into()), ..Default::default() }),
            (Relation::Embedded, false, cascade(&["ALL"])),
            (Relation::ElementCollection, false, RelationOptions { order_by: Some(String::new()), ..Default::default() }),
            (Relation::None, false, RelationOptions { orphan_removal: true, ..Default::default() }),
        ];
        for (relation, mapped_by, options) in refused {
            assert!(options.validate(relation, false, mapped_by).is_err(), "{relation:?} {mapped_by} {options:?}");
            let mut spec = spec("x", "X", relation, options);
            spec.mapped_by = mapped_by.then(|| "owner".to_owned());
            assert!(render_in(false, "jakarta", &spec).is_none());
            assert!(render_in(true, "jakarta", &spec).is_none());
        }
        let transient = cascade(&["ALL"]);
        assert!(transient.validate(Relation::ManyToOne, true, false).is_err());
    }

    #[test]
    fn mapped_by_side_keeps_collection_options_but_no_join_mapping() {
        let options = RelationOptions { orphan_removal: true, order_by: Some("id".into()), ..cascade(&["ALL"]) };
        let mut spec = spec("lines", "Line", Relation::OneToMany, options);
        spec.mapped_by = Some("order".into());
        let java = render_in(false, "jakarta", &spec).unwrap();
        assert!(java.contains("@OneToMany(mappedBy = \"order\", cascade = CascadeType.ALL, orphanRemoval = true)\n    @OrderBy(\"id\")"), "{java}");
    }

    /// Run explicitly with pinned API jars; compilation is separate from provider/runtime acceptance.
    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_API / JPA_JAVAX_API jar paths"]
    fn generated_java_compiles_against_both_persistence_apis() {
        compile_generated_sources(false);
    }

    #[test]
    #[ignore = "requires API jars, KOTLIN_COMPILER_CLASSPATH and KOTLIN_STDLIB"]
    fn generated_kotlin_compiles_against_both_persistence_apis() {
        compile_generated_sources(true);
    }

    fn compile_generated_sources(kotlin: bool) {
        use persistence_core::{PersistenceRole, generation::*};
        let base = std::env::temp_dir().join(format!("jpa-generation-compile-{}-{kotlin}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let mut total = 0;
        for namespace in [Namespace::Jakarta, Namespace::Javax] {
            let jar = std::env::var(if namespace == Namespace::Jakarta { "JPA_JAKARTA_API" } else { "JPA_JAVAX_API" }).expect("explicit API jar path");
            assert!(std::path::Path::new(&jar).is_file());
            let dir = base.join(namespace.package());
            std::fs::create_dir_all(&dir).unwrap();
            let mut files = Vec::new();
            let mut non_final_names = Vec::new();
            for role in [PersistenceRole::Entity, PersistenceRole::MappedSuperclass, PersistenceRole::Embeddable] {
                for id_type in [IdentifierType::Long, IdentifierType::Integer, IdentifierType::Short, IdentifierType::String, IdentifierType::Uuid, IdentifierType::BigInteger] {
                    for strategy in [GenerationStrategy::Assigned, GenerationStrategy::Identity, GenerationStrategy::Sequence, GenerationStrategy::Table, GenerationStrategy::Auto, GenerationStrategy::Uuid] {
                        let id = IdentifierDraft {
                            name: "key".into(), column: "record_id".into(), id_type, strategy,
                            sequence: (strategy == GenerationStrategy::Sequence).then(|| SequenceGenerator { generator: "recordIds".into(), sequence_name: "record_seq".into(), initial_value: -5, allocation_size: 10 }),
                            table: (strategy == GenerationStrategy::Table).then(|| TableGenerator { generator: "recordIds".into(), table_name: "id_store".into(), pk_column_name: "entity_key".into(), value_column_name: "next_id".into(), pk_column_value: "record".into(), initial_value: 0, allocation_size: 25 }),
                        };
                        if id.validate(namespace).is_err() { continue; }
                        // One embeddable without an identifier is sufficient; others exercise ID/version combinations.
                        if role == PersistenceRole::Embeddable && (id_type != IdentifierType::Long || strategy != GenerationStrategy::Assigned) { continue; }
                        let name = format!("Generated{total}");
                        let draft = ManagedClassDraft { name: name.clone(), role: role.clone(), namespace,
                            table_name: if role == PersistenceRole::Entity { "records".into() } else { String::new() },
                            identifier: (role != PersistenceRole::Embeddable).then_some(id), versioned: role != PersistenceRole::Embeddable, inheritance: None };
                        let source = super::configured_class_source(&draft, "shop", kotlin).unwrap();
                        let path = dir.join(format!("{name}.{}", if kotlin { "kt" } else { "java" }));
                        std::fs::write(&path, source).unwrap();
                        files.push(path);
                        if role != PersistenceRole::Embeddable { non_final_names.push(format!("shop.{name}")); }
                        total += 1;
                    }
                }
            }
            let result = if kotlin {
                let compiler = std::env::var("KOTLIN_COMPILER_CLASSPATH").expect("explicit Kotlin compiler classpath");
                let stdlib = std::env::var("KOTLIN_STDLIB").expect("explicit Kotlin standard library jar");
                std::process::Command::new("java").args(["-classpath", &compiler, "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler", "-no-stdlib", "-no-reflect", "-jvm-target", "17", "-classpath", &format!("{jar}:{stdlib}"), "-d"]).arg(&dir).args(&files).output().expect("java installed")
            } else {
                std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &jar, "-d"]).arg(&dir).args(&files).output().expect("javac installed")
            };
            assert!(result.status.success(), "{}: {}", namespace.package(), String::from_utf8_lossy(&result.stderr));
            let signatures = std::process::Command::new("javap").args(["-public", "-classpath"]).arg(&dir).args(&non_final_names).output().expect("javap installed");
            assert!(signatures.status.success(), "{}", String::from_utf8_lossy(&signatures.stderr));
            let signatures = String::from_utf8(signatures.stdout).unwrap();
            assert!(!signatures.contains(" final "), "Entity/mapped-superclass public methods and classes must remain overridable: {signatures}");
            for name in non_final_names { assert!(signatures.contains(&format!("public {name}();")), "Missing public no-argument constructor for {name}"); }
        }
        assert!(total >= 60, "compile all supported ID combinations: {total}");
        eprintln!("Compiled {total} generated {} persistence classes against Jakarta 3.2 and javax 2.2 APIs.", if kotlin { "Kotlin" } else { "Java" });
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn plural_names_follow_english_endings() {
        assert_eq!(plural("order"), "orders");
        assert_eq!(plural("category"), "categories");
        assert_eq!(plural("day"), "days");
        assert_eq!(plural("address"), "addresses");
        assert_eq!(plural("box"), "boxes");
    }

    #[test]
    fn inheritance_root_and_subclass_render_in_both_languages() {
        use persistence_core::{
            PersistenceRole,
            generation::{
                DiscriminatorColumn, DiscriminatorType, GenerationStrategy, IdentifierDraft, IdentifierType,
                InheritanceDraft, InheritanceStrategy, ManagedClassDraft, Namespace,
            },
        };
        let root = ManagedClassDraft {
            name: "Vehicle".into(),
            role: PersistenceRole::Entity,
            namespace: Namespace::Jakarta,
            table_name: "vehicle".into(),
            identifier: Some(IdentifierDraft {
                name: "id".into(),
                column: String::new(),
                id_type: IdentifierType::Long,
                strategy: GenerationStrategy::Identity,
                sequence: None,
                table: None,
            }),
            versioned: false,
            inheritance: Some(InheritanceDraft::Root {
                strategy: InheritanceStrategy::SingleTable,
                discriminator: Some(DiscriminatorColumn {
                    name: "kind".into(),
                    discriminator_type: DiscriminatorType::String,
                    length: 16,
                }),
                discriminator_value: Some("VEHICLE".into()),
            }),
        };
        let java = super::configured_class_source(&root, "shop", false).unwrap();
        assert!(java.contains("@jakarta.persistence.Inheritance(strategy = jakarta.persistence.InheritanceType.SINGLE_TABLE)\n"));
        assert!(java.contains("@jakarta.persistence.DiscriminatorColumn(name = \"kind\", discriminatorType = jakarta.persistence.DiscriminatorType.STRING, length = 16)\n"));
        assert!(java.contains("@jakarta.persistence.DiscriminatorValue(\"VEHICLE\")\npublic class Vehicle {"));

        let sub = ManagedClassDraft {
            name: "Car".into(),
            table_name: String::new(),
            identifier: None,
            inheritance: Some(InheritanceDraft::Subclass {
                superclass: "Vehicle".into(),
                discriminator_value: Some("CAR".into()),
                primary_key_join_column: Some("vehicle_id".into()),
            }),
            ..root
        };
        let java = super::configured_class_source(&sub, "shop", false).unwrap();
        assert!(java.contains("public class Car extends Vehicle {"));
        assert!(!java.contains("@jakarta.persistence.Table") && !java.contains("@jakarta.persistence.Id"));
        assert!(java.contains("@jakarta.persistence.PrimaryKeyJoinColumn(name = \"vehicle_id\")"));
        let kotlin = super::configured_class_source(&sub, "shop", true).unwrap();
        assert!(kotlin.contains("open class Car : Vehicle() {"));

        let mut table_per_class = sub.clone();
        table_per_class.inheritance = Some(InheritanceDraft::Subclass {
            superclass: "Vehicle".into(),
            discriminator_value: None,
            primary_key_join_column: None,
        });
        table_per_class.table_name = "car".into();
        let java = super::configured_class_source(&table_per_class, "shop", false).unwrap();
        assert!(java.contains("@jakarta.persistence.Table(name = \"car\")"));
    }
}
