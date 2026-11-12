//! Reviewed Hibernate 6 scaffolding for one plain field-access Java entity: a `UserType<T>` sibling
//! class plus `@Type`, a `@JdbcTypeCode(SqlTypes.JSON)` mapping, and a sibling Hibernate event listener.
//! Verified against the Hibernate 6.6 Javadoc/user guide and (for 6.0) the 6.0 user guide, and with javap on
//! hibernate-core 6.6.49: `org.hibernate.usertype.UserType<J>` (`getSqlType` returns a `java.sql.Types`/`SqlTypes`
//! code, user guide example uses `Types.VARCHAR`), `@org.hibernate.annotations.Type(Class<? extends UserType<?>>)`,
//! `@org.hibernate.annotations.JdbcTypeCode(int)`, `org.hibernate.type.SqlTypes.JSON`, and the
//! `org.hibernate.event.spi` listener interfaces. Hibernate 5 (`@Type(type = ...)`) and 7 are refused.
use super::*;

pub(super) const TITLE_TYPE: &str = "Create Hibernate user type…";
pub(super) const TITLE_JSON: &str = "Add JSON column mapping";
pub(super) const TITLE_LISTENER: &str = "Create Hibernate event listener…";
#[path = "hibernate_parameters.rs"]
mod hibernate_parameters;
#[path = "hibernate_type_inspection.rs"]
mod hibernate_type_inspection;
pub(crate) use hibernate_type_inspection::findings as type_findings;
const PLAIN_ANNOTATIONS: [&str; 2] = ["Column", "Basic"];
const MAPPING_ANNOTATIONS: [&str; 14] = [
    "Id", "EmbeddedId", "Column", "Basic", "Temporal", "Lob", "Enumerated", "ManyToOne", "OneToMany", "OneToOne",
    "ManyToMany", "Embedded", "JoinColumn", "Version",
];
const PRIMITIVES: [&str; 8] = ["byte", "short", "int", "long", "float", "double", "boolean", "char"];
const JAVA_LANG: [&str; 12] =
    ["String", "Integer", "Long", "Boolean", "Double", "Float", "Short", "Byte", "Character", "Object", "Number", "Character"];
const SQL_TYPES: [&str; 7] = ["VARCHAR", "INTEGER", "BIGINT", "BOOLEAN", "DOUBLE", "NUMERIC", "TIMESTAMP"];

/// fully qualified type, JDBC read type, getter, setter, SQL type constant
const IDENTITY: [(&str, &str, &str, &str, &str); 6] = [
    ("java.lang.String", "String", "getString", "setString", "VARCHAR"),
    ("java.lang.Integer", "int", "getInt", "setInt", "INTEGER"),
    ("java.lang.Long", "long", "getLong", "setLong", "BIGINT"),
    ("java.lang.Boolean", "boolean", "getBoolean", "setBoolean", "BOOLEAN"),
    ("java.lang.Double", "double", "getDouble", "setDouble", "DOUBLE"),
    ("java.math.BigDecimal", "java.math.BigDecimal", "getBigDecimal", "setBigDecimal", "NUMERIC"),
];

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Listener {
    PostLoad,
    PreInsert,
    PostInsert,
    PreUpdate,
    PostUpdate,
    PreDelete,
    PostDelete,
}
impl Listener {
    const ALL: [Listener; 7] = [
        Self::PostLoad,
        Self::PreInsert,
        Self::PostInsert,
        Self::PreUpdate,
        Self::PostUpdate,
        Self::PreDelete,
        Self::PostDelete,
    ];
    fn event(self) -> &'static str {
        match self {
            Self::PostLoad => "PostLoad",
            Self::PreInsert => "PreInsert",
            Self::PostInsert => "PostInsert",
            Self::PreUpdate => "PreUpdate",
            Self::PostUpdate => "PostUpdate",
            Self::PreDelete => "PreDelete",
            Self::PostDelete => "PostDelete",
        }
    }
    fn post_action(self) -> bool {
        matches!(self, Self::PostInsert | Self::PostUpdate | Self::PostDelete)
    }
    fn vetoes(self) -> bool {
        matches!(self, Self::PreInsert | Self::PreUpdate | Self::PreDelete)
    }
    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.event() == key)
    }
}

struct Field {
    name: String,
    ty: String,
    span: (usize, usize),
}

struct Source {
    class: Class,
    masked: String,
    ns: &'static str,
    fields: Vec<Field>,
    skipped: Vec<String>,
}

fn squash(text: &str) -> String {
    text.split_whitespace().collect()
}

fn classify(doc: &Document, class: &Class, masked: &str, ns: &str, member: (usize, usize)) -> Option<Result<Field, String>> {
    let (start, finish) = member;
    let clean = crate::model::without_annotations(masked, start, finish);
    let declaration = clean.trim().strip_suffix(';')?;
    let head = declaration.split_once('=').map_or(declaration, |(head, _)| head).trim();
    if head.contains('(') {
        return None;
    }
    let (ty, name) = head.strip_prefix("private ")?.trim().rsplit_once(char::is_whitespace)?;
    let (ty, name) = (ty.trim(), name.trim());
    if !identifier(name) {
        return None;
    }
    let attribute = class.attributes.iter().find(|a| a.name == name)?;
    let reject = |reason: &str| Some(Err(format!("{name}: {reason}")));
    if attribute.relation != Relation::None
        || attribute.id
        || attribute.generated.is_some()
        || attribute.lob
        || attribute.enum_string.is_some()
        || squash(&attribute.ty) != squash(ty)
    {
        return reject("not a plain basic attribute");
    }
    for annotation in annotations(&masked[start..finish]) {
        if !PLAIN_ANNOTATIONS.contains(&annotation.name) || !annotation_is(&doc.text, class, annotation.name, ns) {
            return reject(&format!("annotation @{} is not a plain @Column/@Basic from {ns}", annotation.name));
        }
    }
    Some(Ok(Field { name: name.to_owned(), ty: ty.to_owned(), span: member }))
}

fn source(doc: &Document) -> Result<Source, String> {
    let class = current_class(doc).filter(|c| c.kind == Kind::Entity).ok_or("Select a plain entity with no superclass.")?;
    let masked = mask(&doc.text);
    let at = crate::checks::owned_class_start(&doc.text, &masked, &class).ok_or("Only a single source-owned entity class is supported.")?;
    if doc.language == "kotlin" || class.kotlin {
        return Err("Hibernate scaffolding is supported for Java entities only; Kotlin is refused.".into());
    }
    let ns = namespace(&doc.text, &class).ok_or("The entity's persistence namespace is not unambiguous.")?;
    if annotations(&masked).iter().any(|a| a.name == "Access") {
        return Err("@Access is present; only plain field access is supported.".into());
    }
    let name_end = offset(&doc.text, class.pos).ok_or("Stale source.")? + class.name.len();
    let open = name_end + masked[name_end..].find('{').ok_or("Stale source.")?;
    let end = class.body_end.ok_or("Stale source.")?;
    if at > name_end {
        return Err("Stale source.".into());
    }
    let members = crate::model::members(&masked, open + 1, end, false, false);
    for &(start, finish) in &members {
        if !masked[..finish].ends_with(';') && annotations(&masked[start..finish]).iter().any(|a| MAPPING_ANNOTATIONS.contains(&a.name)) {
            return Err("Mapping annotations on methods suggest property access; only field access is supported.".into());
        }
    }
    let mut fields = Vec::new();
    let mut skipped = Vec::new();
    for member in members {
        match classify(doc, &class, &masked, ns, member) {
            Some(Ok(field)) => fields.push(field),
            Some(Err(reason)) => skipped.push(reason),
            None => {}
        }
    }
    Ok(Source { class, masked, ns, fields, skipped })
}

fn type_shape(ty: &str) -> bool {
    !ty.is_empty()
        && !PRIMITIVES.contains(&ty)
        && ty.split('.').all(identifier)
}

fn user_type_fields(src: &Source) -> Vec<&Field> {
    src.fields.iter().filter(|f| type_shape(&f.ty)).collect()
}

fn kotlin_entity(doc: &Document) -> bool {
    doc.language == "kotlin" && current_class(doc).is_some_and(|class| class.kind == Kind::Entity)
}

pub(super) fn eligible_type(doc: &Document) -> bool {
    match source(doc) {
        Ok(src) => !user_type_fields(&src).is_empty(),
        Err(_) => kotlin_entity(doc),
    }
}

pub(super) fn eligible_json(doc: &Document) -> bool {
    match source(doc) {
        Ok(src) => !src.fields.is_empty(),
        Err(_) => kotlin_entity(doc),
    }
}

pub(super) fn eligible_listener(doc: &Document) -> bool {
    source(doc).is_ok() || kotlin_entity(doc)
}

fn strip_version(text: &str) -> Option<u32> {
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    let rest = &text[digits.len()..];
    if digits.is_empty() || !(rest.is_empty() || rest.starts_with(['.', '-', '\'', '"', '<', ')', ' '])) {
        return None;
    }
    digits.parse().ok()
}

/// Majors of literal hibernate-core versions; 0 when a declaration has no literal version.
pub(crate) fn hibernate_core_majors(build: &str) -> Vec<u32> {
    let mut found = Vec::new();
    for needle in ["org.hibernate.orm:hibernate-core", "org.hibernate:hibernate-core"] {
        for (at, _) in build.match_indices(needle) {
            let rest = &build[at + needle.len()..];
            match rest.chars().next() {
                Some(':') => found.push(strip_version(&rest[1..]).unwrap_or(0)),
                Some(ch) if ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.') => {}
                _ => found.push(0),
            }
        }
    }
    let artifact = "<artifactId>hibernate-core</artifactId>";
    for (at, _) in build.match_indices(artifact) {
        let begin = build[..at].rfind("<dependency>").unwrap_or(0);
        let finish = build[at..].find("</dependency>").map_or(build.len(), |i| at + i);
        let block = &build[begin..finish];
        if !block.contains("<groupId>org.hibernate.orm</groupId>") && !block.contains("<groupId>org.hibernate</groupId>") {
            continue;
        }
        let version = block.split_once("<version>").and_then(|(_, rest)| strip_version(rest));
        found.push(version.unwrap_or(0));
    }
    found
}

/// Majors of literal Spring Boot plugin/parent versions.
pub(crate) fn spring_boot_majors(build: &str) -> Vec<u32> {
    let mut found = Vec::new();
    for (at, needle) in build.match_indices("org.springframework.boot") {
        let rest = &build[at + needle.len()..];
        let line = rest.lines().next().unwrap_or("");
        let Some((_, after)) = line.split_once("version").or_else(|| line.split_once("spring-boot-gradle-plugin:")) else {
            continue;
        };
        let value = after.trim_start_matches(|ch: char| ch.is_whitespace() || matches!(ch, '\'' | '"' | '(' | '='));
        if let Some(major) = strip_version(value) {
            found.push(major);
        }
    }
    for artifact in ["spring-boot-starter-parent", "spring-boot-dependencies"] {
        let tag = format!("<artifactId>{artifact}</artifactId>");
        for (at, _) in build.match_indices(&tag) {
            let window = &build[at..(at + 300).min(build.len())];
            if let Some(major) = window.split_once("<version>").and_then(|(_, rest)| strip_version(rest)) {
                found.push(major);
            }
        }
    }
    found
}

/// Ok with the evidence used; Err with the refusal reason when Hibernate 6 is not provable.
pub(crate) fn hibernate6(build: &str) -> Result<String, String> {
    if build.trim().is_empty() {
        return Err("No pom.xml, build.gradle or build.gradle.kts could be read above this source, so Hibernate 6 cannot be proven; nothing was generated.".into());
    }
    if build.contains("hibernate-entitymanager") {
        return Err("hibernate-entitymanager is a Hibernate 5 artifact; the Hibernate 6 shape is not generated.".into());
    }
    let direct: Vec<u32> = hibernate_core_majors(build).into_iter().filter(|major| *major != 0).collect();
    let boot = spring_boot_majors(build);
    if let Some(major) = direct.iter().find(|major| **major != 6) {
        return Err(format!("hibernate-core {major}.x is declared; only the Hibernate 6.x shape is verified, so nothing was generated."));
    }
    if !direct.is_empty() {
        if let Some(major) = boot.iter().find(|major| **major != 3) {
            return Err(format!("Spring Boot {major}.x is declared next to a Hibernate 6 version; the combination is not verified, so nothing was generated."));
        }
        return Ok("literal org.hibernate(.orm):hibernate-core 6.x version".into());
    }
    if boot.is_empty() {
        return Err("Hibernate 6 is not provable: write org.hibernate.orm:hibernate-core with a literal 6.x version, or Spring Boot 3.x with spring-boot-starter-data-jpa, literally in a build file above this source (properties, version catalogs and BOMs are not resolved); nothing was generated.".into());
    }
    if boot.iter().any(|major| *major != 3) || !build.contains("spring-boot-starter-data-jpa") {
        return Err("Hibernate 6 is not provable from Spring Boot: it needs a literal Spring Boot 3.x version and spring-boot-starter-data-jpa (Boot 2.x ships Hibernate 5, 4.x ships Hibernate 7); nothing was generated.".into());
    }
    Ok("literal Spring Boot 3.x with spring-boot-starter-data-jpa (Boot 3.0.0 manages Hibernate 6.1.5, 3.3.12 manages 6.5.3)".into())
}

fn build_proof(project: &Project, root: &str, directory: &str) -> Result<String, Error> {
    let build = mapper_action::build_text(project, root, directory)?;
    hibernate6(&build).map_err(Error::from)
}

fn import_edit(masked: &str, eol: &str, imports: &[&str]) -> Result<(usize, usize, String), Error> {
    let stale = || Error::from("Unsupported import layout.");
    let mut last_import = None;
    let mut package_end = None;
    let mut at = 0;
    for line in masked.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") {
            last_import = Some(at + line.len());
        } else if trimmed.starts_with("package ") {
            package_end = Some(at + line.len());
        }
        at += line.len();
    }
    let lines: String = imports.iter().map(|import| format!("import {import};{eol}")).collect();
    if let Some(end) = last_import {
        if !masked[..end].ends_with('\n') {
            return Err(stale());
        }
        return Ok((end, end, lines));
    }
    let end = package_end.ok_or_else(stale)?;
    if !masked[..end].ends_with('\n') {
        return Err(stale());
    }
    Ok((end, end, format!("{eol}{lines}")))
}

fn annotate_field(doc: &Document, src: &Source, field: &Field, annotation: &str, imports: &[&str]) -> Result<Vec<TextEdit>, Error> {
    let eol = if doc.text.contains("\r\n") { "\r\n" } else { "\n" };
    let slice = &doc.text[field.span.0..field.span.1];
    let first = slice.find(|c: char| !c.is_whitespace()).ok_or(Error::Stale)?;
    let insert_at = field.span.0 + first;
    let line_start = doc.text[..insert_at].rfind('\n').map_or(0, |i| i + 1);
    let indent = &doc.text[line_start..insert_at];
    if !indent.chars().all(|c| c == ' ' || c == '\t') {
        return Err("The attribute declaration shares a line with other code; edit it manually first.".into());
    }
    let (start, end, text) = import_edit(&src.masked, eol, imports)?;
    let edits = vec![(insert_at, insert_at, format!("{annotation}{eol}{indent}")), (start, end, text)];
    Ok(edits
        .into_iter()
        .map(|(s, e, text)| TextEdit { range: Range::new(position(&doc.text, s), position(&doc.text, e)), text })
        .collect())
}

fn finish(doc: &Document, mut text: String, edits: &[TextEdit], name: &str) -> Result<(String, String), Error> {
    if doc.text.contains("\r\n") {
        text = text.replace('\n', "\r\n");
    }
    let directory = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let after = checked_after(doc, edits)?;
    if text.len() > MAX_TEXT || after.len() > MAX_TEXT {
        return Err("Generated source exceeds the supported bound.".into());
    }
    Ok((format!("{directory}/{name}.java"), text))
}

fn free_name(src: &Source, name: &str, reserved: &[&str]) -> Result<(), Error> {
    if !identifier(name) || name == src.class.name || reserved.contains(&name) || has_word(&src.masked, name) {
        return Err("Choose a valid class name that does not occur in this source file.".into());
    }
    Ok(())
}

/// Where the attribute type is written in the new sibling file.
struct Resolved {
    written: String,
    simple: String,
    fqn: Option<String>,
    import: Option<String>,
    same_package: bool,
}

fn resolve_type(src: &Source, ty: &str) -> Result<Resolved, Error> {
    if ty.contains('.') {
        return Ok(Resolved { written: ty.to_owned(), simple: ty.rsplit('.').next().unwrap_or(ty).to_owned(), fqn: Some(ty.to_owned()), import: None, same_package: false });
    }
    let explicit: Vec<_> = src.class.imports.iter().filter(|i| i.rsplit('.').next() == Some(ty)).collect();
    if explicit.len() > 1 {
        return Err("The attribute type matches more than one import.".into());
    }
    if let Some(import) = explicit.first() {
        let lang = import.strip_prefix("java.lang.").is_some_and(|rest| rest == ty);
        return Ok(Resolved { written: ty.to_owned(), simple: ty.to_owned(), fqn: Some((*import).clone()), import: (!lang).then(|| (*import).clone()), same_package: false });
    }
    if JAVA_LANG.contains(&ty) {
        return Ok(Resolved { written: ty.to_owned(), simple: ty.to_owned(), fqn: Some(format!("java.lang.{ty}")), import: None, same_package: false });
    }
    if src.class.imports.iter().any(|i| i.ends_with(".*") && i != &format!("{}.*", src.ns) && i != "java.util.*" && i != "java.lang.*") {
        return Err("The attribute type may come from a wildcard import, which is not resolved; import it explicitly first.".into());
    }
    Ok(Resolved { written: ty.to_owned(), simple: ty.to_owned(), fqn: None, import: None, same_package: true })
}

const USER_TYPE_RESERVED: [&str; 16] = [
    "Serializable", "PreparedStatement", "ResultSet", "SQLException", "SharedSessionContractImplementor", "UserType", "Override", "Class",
    "Object", "Objects", "Types", "UnsupportedOperationException", "Integer", "Long", "Boolean", "Double",
];

pub(super) struct Rendered {
    pub target: String,
    pub text: String,
    pub edits: Vec<TextEdit>,
    pub unimplemented: Vec<&'static str>,
}

pub(super) fn default_type_name(attribute_type: &str) -> String {
    let simple = attribute_type.rsplit('.').next().unwrap_or(attribute_type);
    format!("{simple}UserType")
}

fn user_type_text(package: &str, name: &str, resolved: &Resolved, sql: &str, identity: Option<&(&str, &str, &str, &str, &str)>) -> String {
    let t = &resolved.written;
    let mut imports = vec![
        "java.io.Serializable".to_owned(),
        "java.sql.PreparedStatement".to_owned(),
        "java.sql.ResultSet".to_owned(),
        "java.sql.SQLException".to_owned(),
    ];
    if let Some(import) = &resolved.import {
        imports.push(import.clone());
    }
    imports.push("org.hibernate.engine.spi.SharedSessionContractImplementor".to_owned());
    imports.push("org.hibernate.usertype.UserType".to_owned());
    imports.sort();
    let imports: String = imports.iter().map(|i| format!("import {i};\n")).collect();
    let (get, set, immutable) = match identity {
        Some((_, read, getter, setter, constant)) => (
            format!("        {read} value = rs.{getter}(position);\n\n        return rs.wasNull() ? null : value;\n"),
            format!(
                "        if (value == null) {{\n            st.setNull(index, java.sql.Types.{constant});\n\n            return;\n        }}\n\n        st.{setter}(index, value);\n"
            ),
            true,
        ),
        None => (
            format!("        throw new UnsupportedOperationException(\"Reading {t} from a JDBC column is not implemented\");\n"),
            format!("        throw new UnsupportedOperationException(\"Binding {t} to a JDBC parameter is not implemented\");\n"),
            false,
        ),
    };
    let unsupported = |what: &str| format!("        throw new UnsupportedOperationException(\"{what} for {t} is not implemented\");\n");
    let (deep, mutable, disassemble, assemble, replace) = if immutable {
        (
            "        return value;\n".to_owned(),
            "        return false;\n".to_owned(),
            "        return value;\n".to_owned(),
            format!("        return ({t}) cached;\n"),
            "        return detached;\n".to_owned(),
        )
    } else {
        (
            unsupported("deepCopy"),
            unsupported("isMutable"),
            unsupported("disassemble"),
            unsupported("assemble"),
            unsupported("replace"),
        )
    };
    format!(
        "package {package};\n\n{imports}\npublic class {name} implements UserType<{t}> {{\n    @Override\n    public int getSqlType() {{\n        return java.sql.Types.{sql};\n    }}\n\n    @Override\n    public Class<{t}> returnedClass() {{\n        return {t}.class;\n    }}\n\n    @Override\n    public boolean equals({t} x, {t} y) {{\n        return java.util.Objects.equals(x, y);\n    }}\n\n    @Override\n    public int hashCode({t} x) {{\n        return java.util.Objects.hashCode(x);\n    }}\n\n    @Override\n    public {t} nullSafeGet(ResultSet rs, int position, SharedSessionContractImplementor session, Object owner) throws SQLException {{\n{get}    }}\n\n    @Override\n    public void nullSafeSet(PreparedStatement st, {t} value, int index, SharedSessionContractImplementor session) throws SQLException {{\n{set}    }}\n\n    @Override\n    public {t} deepCopy({t} value) {{\n{deep}    }}\n\n    @Override\n    public boolean isMutable() {{\n{mutable}    }}\n\n    @Override\n    public Serializable disassemble({t} value) {{\n{disassemble}    }}\n\n    @Override\n    public {t} assemble(Serializable cached, Object owner) {{\n{assemble}    }}\n\n    @Override\n    public {t} replace({t} detached, {t} managed, Object owner) {{\n{replace}    }}\n}}\n"
    )
}

/// `sql` is only used for types that are not identity-convertible; `same_package_ok` is the fresh proof that
/// an unqualified, unimported attribute type exists as a sibling source.
pub(super) fn render_type(doc: &Document, chosen: &str, name: &str, sql: Option<&str>, same_package_ok: bool) -> Result<Rendered, Error> {
    let src = source(doc).map_err(Error::from)?;
    let field = user_type_fields(&src).into_iter().find(|f| f.name == chosen).ok_or("Choose an offered attribute.")?;
    let resolved = resolve_type(&src, &field.ty)?;
    if resolved.same_package && !same_package_ok {
        return Err("The attribute type is not imported and no sibling source declares it, so it cannot be proven to resolve from the new class; import it explicitly first.".into());
    }
    if USER_TYPE_RESERVED.contains(&resolved.simple.as_str()) && !resolved.fqn.as_deref().is_some_and(|f| f.starts_with("java.lang.")) {
        return Err("The attribute type's simple name clashes with a type the generated class uses.".into());
    }
    free_name(&src, name, &USER_TYPE_RESERVED)?;
    if has_word(&src.masked, "Type") {
        return Err("This source already mentions `Type`; edit it manually first.".into());
    }
    let identity = resolved.fqn.as_deref().and_then(|fqn| IDENTITY.iter().find(|row| row.0 == fqn));
    let sql = match identity {
        Some(row) => row.4,
        None => sql.filter(|s| SQL_TYPES.contains(s)).ok_or("Choose a SQL type from the offered java.sql.Types constants.")?,
    };
    let edits = annotate_field(doc, &src, field, &format!("@Type({name}.class)"), &["org.hibernate.annotations.Type"])?;
    let text = user_type_text(&package(doc).ok_or(Error::Stale)?, name, &resolved, sql, identity);
    let (target, text) = finish(doc, text, &edits, name)?;
    let unimplemented = if identity.is_some() {
        Vec::new()
    } else {
        vec!["nullSafeGet", "nullSafeSet", "deepCopy", "isMutable", "disassemble", "assemble", "replace"]
    };
    Ok(Rendered { target, text, edits, unimplemented })
}

pub(super) fn render_json(doc: &Document, chosen: &str) -> Result<Vec<TextEdit>, Error> {
    let src = source(doc).map_err(Error::from)?;
    let field = src.fields.iter().find(|f| f.name == chosen).ok_or("Choose an offered attribute.")?;
    if has_word(&src.masked, "JdbcTypeCode") || has_word(&src.masked, "SqlTypes") {
        return Err("This source already mentions `JdbcTypeCode` or `SqlTypes`; edit it manually first.".into());
    }
    let edits = annotate_field(
        doc,
        &src,
        field,
        "@JdbcTypeCode(SqlTypes.JSON)",
        &["org.hibernate.annotations.JdbcTypeCode", "org.hibernate.type.SqlTypes"],
    )?;
    checked_after(doc, &edits)?;
    Ok(edits)
}

const LISTENER_RESERVED: [&str; 6] = ["Override", "EntityPersister", "Object", "Class", "Integer", "String"];

pub(super) fn default_listener_name(entity: &str, listener: Listener) -> String {
    format!("{entity}{}Listener", listener.event())
}

pub(super) fn render_listener(doc: &Document, listener: Listener, name: &str) -> Result<(String, String), Error> {
    let src = source(doc).map_err(Error::from)?;
    let event = listener.event();
    let interface = format!("{event}EventListener");
    let event_class = format!("{event}Event");
    if name == interface || name == event_class || has_word(&src.masked, name) || !identifier(name) || name == src.class.name || LISTENER_RESERVED.contains(&name) {
        return Err("Choose a valid listener class name that does not occur in this source file.".into());
    }
    let mut imports = vec![
        format!("org.hibernate.event.spi.{event_class}"),
        format!("org.hibernate.event.spi.{interface}"),
    ];
    if listener.post_action() {
        imports.push("org.hibernate.persister.entity.EntityPersister".to_owned());
    }
    let imports: String = imports.iter().map(|i| format!("import {i};\n")).collect();
    let returns = if listener.vetoes() { "boolean" } else { "void" };
    let body = if listener.vetoes() { "        return false;\n" } else { "" };
    let mut methods = format!("    @Override\n    public {returns} on{event}({event_class} event) {{\n{body}    }}\n");
    if listener.post_action() {
        methods.push_str("\n    @Override\n    public boolean requiresPostCommitHandling(EntityPersister persister) {\n        return false;\n    }\n");
    }
    let text = format!("package {};\n\n{imports}\npublic class {name} implements {interface} {{\n{methods}}}\n", package(doc).ok_or(Error::Stale)?);
    let (target, text) = finish(doc, text, &[], name)?;
    Ok((target, text))
}

fn directory_of(doc: &Document) -> Result<&str, Error> {
    Ok(doc.path.rsplit_once('/').ok_or(Error::Stale)?.0)
}

fn same_package_exists(project: &Project, directory: &str, ty: &str) -> Result<bool, Error> {
    for extension in ["java", "kt"] {
        match project.read_file(&format!("{directory}/{ty}.{extension}")) {
            Ok(_) => return Ok(true),
            Err(Error::NotFound(_)) | Err(Error::Invalid(_)) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

fn confirm_and_check(pending: &Pending, doc: &Document, project: &Project, directory: &str, name: Option<&str>, title: &str, body: String) -> Result<(), Error> {
    if !ui_kit::prompts::confirm_exact(title, &body)? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    build_proof(project, &pending.root, directory)?;
    if let Some(name) = name {
        no_collision(project, &pending.root, directory, name, None)?;
    }
    Ok(())
}

pub(super) fn resolve_type_action(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let directory = directory_of(doc)?;
    let evidence = build_proof(project, &pending.root, directory)?;
    let src = source(doc).map_err(Error::from)?;
    let fields = user_type_fields(&src);
    if fields.is_empty() {
        return Err(format!("No attribute is eligible. Skipped: {}", src.skipped.join("; ")).into());
    }
    let items: Vec<_> = fields.iter().map(|f| PickItem::new(&f.name, &f.name).description(&f.ty)).collect();
    let chosen = host::pick("Attribute to map with a user type (identifiers, versions, enums, temporals, LOBs, relationships and annotated attributes are not offered)", &items)?
        .ok_or(Error::Cancelled)?;
    let field = fields.into_iter().find(|f| f.name == chosen).ok_or("Choose an offered attribute.")?;
    let resolved = resolve_type(&src, &field.ty)?;
    let default = default_type_name(&field.ty);
    let name = host::input(&format!("User type class name (same package), implements UserType<{}>", field.ty), Some(&default))?
        .ok_or(Error::Cancelled)?;
    let name = name.trim().to_owned();
    let parameterized = hibernate_parameters::ask()?;
    let identity = resolved.fqn.as_deref().is_some_and(|fqn| IDENTITY.iter().any(|row| row.0 == fqn));
    let sql = if identity {
        None
    } else {
        let sqls: Vec<_> = SQL_TYPES.iter().map(|s| PickItem::new(s, s)).collect();
        Some(host::pick(&format!("SQL type code returned by getSqlType() (java.sql.Types constant); {} is not identity-convertible", field.ty), &sqls)?.ok_or(Error::Cancelled)?)
    };
    let same_package_ok = !resolved.same_package || same_package_exists(project, directory, &resolved.simple)?;
    let mut rendered = render_type(doc, &chosen, &name, sql.as_deref(), same_package_ok)?;
    if parameterized {
        rendered.text = hibernate_parameters::parameterize(&rendered.text, &field.ty, &name)?;
    }
    if path(&rendered.target).as_deref() != Some(rendered.target.as_str()) || !inside(&pending.root, &rendered.target) {
        return Err(Error::Stale);
    }
    no_collision(project, &pending.root, directory, &name, None)?;
    let after = patched(&doc.text, &rendered.edits)?;
    let status = if rendered.unimplemented.is_empty() {
        format!("Every method is implemented: {} is read and bound with plain JDBC and treated as immutable, null in and null out.", field.ty)
    } else {
        format!(
            "NOT IMPLEMENTED: {} throw UnsupportedOperationException because {} is not identity-convertible; implement them before use. The type must resolve from package {}, which is assumed from a sibling source or an explicit import.",
            rendered.unimplemented.join(", "),
            field.ty,
            package(doc).unwrap_or_default()
        )
    };
    let body = format!(
        "Source: {}\nSource version: {}\nNew file: {}\nHibernate 6 proof: {evidence}\n\nNew user type:\n{}\nUpdated entity:\n{after}\n{status}\nShape: org.hibernate.usertype.UserType<J> with @org.hibernate.annotations.Type({name}.class), the Hibernate 6 form (Hibernate 5 used @Type(type = ...) and is refused). getSqlType() returns a java.sql.Types code as in the Hibernate user guide example; @JdbcTypeCode is not used because a UserType supplies its own SQL type. Current source only: other files, existing column data and any other mapping of this attribute are not checked. One undoable host edit plus one new file; no existing file is overwritten.",
        doc.path, doc.version, rendered.target, rendered.text
    );
    confirm_and_check(pending, doc, project, directory, Some(&name), "Create Hibernate user type?", body)?;
    Ok(FileAction {
        title: format!("Create Hibernate user type {name}"),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), rendered.edits)],
        operations: vec![FileOperation::Create { path: rendered.target, text: rendered.text, overwrite: false }],
        ..FileAction::default()
    })
}

pub(super) fn resolve_json_action(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let directory = directory_of(doc)?;
    let evidence = build_proof(project, &pending.root, directory)?;
    let src = source(doc).map_err(Error::from)?;
    if src.fields.is_empty() {
        return Err(format!("No attribute is eligible. Skipped: {}", src.skipped.join("; ")).into());
    }
    let items: Vec<_> = src.fields.iter().map(|f| PickItem::new(&f.name, &f.name).description(&f.ty)).collect();
    let chosen = host::pick("Attribute to map as a JSON column (identifiers, versions, enums, temporals, LOBs, relationships and annotated attributes are not offered)", &items)?
        .ok_or(Error::Cancelled)?;
    let edits = render_json(doc, &chosen)?;
    let after = patched(&doc.text, &edits)?;
    let body = format!(
        "Target: {}\nSource version: {}\nHibernate 6 proof: {evidence}\n\n{after}\nAdds @JdbcTypeCode(SqlTypes.JSON) to `{chosen}`, the form shown in the Hibernate 6.0-6.6 user guide (JSON mapping). Per the guide, the JSON serialization library is detected automatically and can be overridden with hibernate.type.json_format_mapper; its presence, dialect JSON support, the attribute type's serializability and the existing column type are not checked. One undoable host edit.",
        doc.path, doc.version
    );
    confirm_and_check(pending, doc, project, directory, None, "Add JSON column mapping?", body)?;
    Ok(FileAction {
        title: format!("Map {chosen} as JSON"),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), edits)],
        ..FileAction::default()
    })
}

pub(super) fn resolve_listener_action(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let directory = directory_of(doc)?;
    let evidence = build_proof(project, &pending.root, directory)?;
    let src = source(doc).map_err(Error::from)?;
    let items: Vec<_> = Listener::ALL
        .iter()
        .map(|l| PickItem::new(l.event(), &format!("{}EventListener", l.event())))
        .collect();
    let picked = host::pick("Hibernate event listener interface (org.hibernate.event.spi)", &items)?.ok_or(Error::Cancelled)?;
    let listener = Listener::from_key(&picked).ok_or("Choose an offered listener.")?;
    let default = default_listener_name(&src.class.name, listener);
    let name = host::input("Listener class name (same package)", Some(&default))?.ok_or(Error::Cancelled)?;
    let name = name.trim().to_owned();
    let (target, text) = render_listener(doc, listener, &name)?;
    if path(&target).as_deref() != Some(target.as_str()) || !inside(&pending.root, &target) {
        return Err(Error::Stale);
    }
    no_collision(project, &pending.root, directory, &name, None)?;
    let veto = if listener.vetoes() {
        " The Javadoc says returning true vetoes the operation; the skeleton returns false."
    } else {
        ""
    };
    let body = format!(
        "Source: {}\nSource version: {}\nNew file: {target}\nHibernate 6 proof: {evidence}\n\nNew listener:\n{text}\nThe `{name}` class is NOT registered, so Hibernate will not call it. Register it with an org.hibernate.integrator.spi.Integrator that appends it through EventListenerRegistry (the Hibernate 6 user guide, Event Listener registration); Spring wiring is not generated and not verified. This is a Hibernate native event listener, separate from JPA entity callbacks such as @PostLoad.{veto} The entity source is not edited. One new file; no existing file is overwritten.",
        doc.path, doc.version
    );
    confirm_and_check(pending, doc, project, directory, Some(&name), "Create Hibernate event listener?", body)?;
    Ok(FileAction {
        title: format!("Create Hibernate event listener {name}"),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target, text, overwrite: false }],
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ENTITY: &str = "package shop;\n\nimport java.math.BigDecimal;\nimport java.util.Map;\nimport jakarta.persistence.Column;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n\n@Entity\npublic class Invoice {\n    @Id\n    private Long id;\n\n    @Column(name = \"title_text\")\n    private String title;\n\n    private BigDecimal total;\n\n    private Money price;\n\n    private Map<String, Object> attributes;\n\n    private int count;\n}\n";
    const PATH: &str = "/project/src/main/java/shop/Invoice.java";
    const GRADLE: &str = "dependencies {\n    implementation 'org.hibernate.orm:hibernate-core:6.5.3.Final'\n}\n";

    fn doc(text: &str) -> Document {
        Document { path: PATH.into(), language: "java".into(), text: text.into(), version: 1 }
    }
    fn setup(text: &str, build: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.open_editor(PATH, "java", text, &[]);
        host.add_file("build.gradle", build);
        host.add_file("src/main/java/shop/Money.java", "package shop;\npublic class Money {}\n");
        (host, JpaAssist::default(), doc(text))
    }
    fn refuse_type(text: &str, attribute: &str, name: &str, sql: Option<&str>, same: bool) -> String {
        match render_type(&doc(text), attribute, name, sql, same) {
            Ok(rendered) => panic!("expected refusal, got\n{}", rendered.text),
            Err(error) => format!("{error:?}"),
        }
    }
    fn typed(text: &str, attribute: &str, name: &str, sql: Option<&str>) -> (Rendered, String) {
        let rendered = render_type(&doc(text), attribute, name, sql, true).unwrap();
        let entity = patched(text, &rendered.edits).unwrap();
        (rendered, entity)
    }

    #[test]
    fn identity_string_type_is_null_safe_and_fully_implemented() {
        let (rendered, entity) = typed(ENTITY, "title", "TitleType", None);
        let text = &rendered.text;
        for expected in [
            "package shop;",
            "import org.hibernate.usertype.UserType;",
            "public class TitleType implements UserType<String> {",
            "return java.sql.Types.VARCHAR;",
            "public Class<String> returnedClass() {\n        return String.class;",
            "String value = rs.getString(position);\n\n        return rs.wasNull() ? null : value;",
            "st.setNull(index, java.sql.Types.VARCHAR);",
            "st.setString(index, value);",
            "public String replace(String detached, String managed, Object owner) {\n        return detached;",
            "return (String) cached;",
        ] {
            assert!(text.contains(expected), "{expected}\n{text}");
        }
        assert!(!text.contains("UnsupportedOperationException") && rendered.unimplemented.is_empty());
        assert_eq!(rendered.target, "/project/src/main/java/shop/TitleType.java");
        assert!(entity.contains("import jakarta.persistence.Id;\nimport org.hibernate.annotations.Type;\n"), "{entity}");
        assert!(entity.contains("    @Type(TitleType.class)\n    @Column(name = \"title_text\")\n    private String title;"), "{entity}");
    }

    #[test]
    fn identity_wrapper_and_bigdecimal_types_use_the_matching_jdbc_calls() {
        let text = ENTITY.replace("private int count;", "private Integer count;\n    private Boolean paid;");
        let (rendered, _) = typed(&text, "count", "CountType", None);
        assert!(rendered.text.contains("int value = rs.getInt(position);") && rendered.text.contains("st.setInt(index, value);") && rendered.text.contains("Types.INTEGER"));
        let (rendered, _) = typed(&text, "paid", "PaidType", None);
        assert!(rendered.text.contains("rs.getBoolean(position)") && rendered.text.contains("Types.BOOLEAN"));
        let (rendered, _) = typed(&text, "total", "TotalType", None);
        assert!(rendered.text.contains("import java.math.BigDecimal;") && rendered.text.contains("UserType<BigDecimal>") && rendered.text.contains("rs.getBigDecimal(position)"));
        assert!(rendered.text.contains("Types.NUMERIC"));
    }

    #[test]
    fn non_identity_type_marks_unimplemented_methods_and_needs_a_sql_type() {
        let (rendered, entity) = typed(ENTITY, "price", "MoneyType", Some("NUMERIC"));
        assert_eq!(rendered.unimplemented, ["nullSafeGet", "nullSafeSet", "deepCopy", "isMutable", "disassemble", "assemble", "replace"]);
        assert!(rendered.text.contains("UserType<Money>") && rendered.text.contains("return Money.class;") && rendered.text.contains("return java.sql.Types.NUMERIC;"));
        assert_eq!(rendered.text.matches("throw new UnsupportedOperationException").count(), 7);
        assert!(entity.contains("@Type(MoneyType.class)\n    private Money price;"));
        for sql in [None, Some("CLOB"), Some("")] {
            assert!(refuse_type(ENTITY, "price", "MoneyType", sql, true).contains("SQL type"), "{sql:?}");
        }
        assert!(refuse_type(ENTITY, "price", "MoneyType", Some("NUMERIC"), false).contains("cannot be proven"));
    }

    #[test]
    fn imported_type_is_imported_and_foreign_wildcards_are_refused() {
        let text = ENTITY.replace("import java.util.Map;", "import java.util.Map;\nimport other.Money;");
        let (rendered, _) = typed(&text, "price", "MoneyType", Some("VARCHAR"));
        assert!(rendered.text.contains("import other.Money;"), "{}", rendered.text);
        let wildcard = ENTITY.replace("import java.util.Map;", "import java.util.Map;\nimport other.*;");
        assert!(refuse_type(&wildcard, "price", "MoneyType", Some("VARCHAR"), true).contains("wildcard"));
    }

    #[test]
    fn generics_primitives_ids_and_annotated_attributes_are_not_offered() {
        let text = ENTITY.replace(
            "    private int count;",
            "    @jakarta.persistence.Version\n    private Long version;\n    @jakarta.persistence.Enumerated\n    private String kind;\n    @org.hibernate.annotations.Type(Other.class)\n    private String typed;\n    @jakarta.persistence.ManyToOne\n    private Customer customer;\n    private int count;\n    private String[] tags;",
        );
        let src = source(&doc(&text)).unwrap();
        let offered: Vec<_> = user_type_fields(&src).iter().map(|f| f.name.clone()).collect();
        assert_eq!(offered, ["title", "total", "price"]);
        let json: Vec<_> = src.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(json.contains(&"attributes") && json.contains(&"count"));
        for skipped in ["version", "kind", "typed", "customer"] {
            assert!(src.skipped.iter().any(|s| s.starts_with(skipped)), "{skipped}: {:?}", src.skipped);
        }
        assert!(refuse_type(&text, "attributes", "AttributesType", Some("VARCHAR"), true).contains("offered"));
        assert!(refuse_type(&text, "id", "IdType", None, true).contains("offered"));
    }

    #[test]
    fn invalid_or_colliding_type_names_and_existing_type_mentions_are_refused() {
        for name in ["", "class", "Invoice", "UserType", "Override", "9X", "A B", "Column", "Entity", "Id"] {
            let reason = refuse_type(ENTITY, "title", name, None, true);
            assert!(reason.contains("valid class name"), "{name}: {reason}");
        }
        let mentions = ENTITY.replace("private int count;", "private int Type;");
        assert!(refuse_type(&mentions, "title", "TitleType", None, true).contains("`Type`"));
    }

    #[test]
    fn crlf_sources_keep_crlf_in_edits_and_new_files() {
        let text = ENTITY.replace('\n', "\r\n");
        let (rendered, entity) = typed(&text, "title", "TitleType", None);
        assert!(!rendered.text.replace("\r\n", "").contains('\n') && !entity.replace("\r\n", "").contains('\n'));
        assert!(entity.contains("@Type(TitleType.class)\r\n    @Column"));
        let edits = render_json(&doc(&text), "title").unwrap();
        assert!(!patched(&text, &edits).unwrap().replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn kotlin_access_property_access_and_superclass_entities_are_refused() {
        let kotlin = "package shop\n\n@jakarta.persistence.Entity\nopen class Invoice {\n    var title: kotlin.String? = null\n}\n";
        let document = Document { path: "/project/src/main/kotlin/shop/Invoice.kt".into(), language: "kotlin".into(), text: kotlin.into(), version: 1 };
        assert!(eligible_type(&document) && eligible_json(&document) && eligible_listener(&document));
        let Err(message) = source(&document) else { panic!("kotlin must be refused") };
        assert!(message.contains("Java entities only"), "{message}");
        for text in [
            ENTITY.replace("@Entity\n", "@Entity\n@jakarta.persistence.Access(jakarta.persistence.AccessType.FIELD)\n"),
            ENTITY.replace("    private int count;", "    @Column\n    public Boolean getPaid() { return null; }"),
            ENTITY.replace("class Invoice {", "class Invoice extends Base {"),
        ] {
            assert!(source(&doc(&text)).is_err() && !eligible_type(&doc(&text)) && !eligible_listener(&doc(&text)), "{text}");
        }
    }

    #[test]
    fn json_mapping_adds_annotation_and_both_imports_for_generic_types() {
        let edits = render_json(&doc(ENTITY), "attributes").unwrap();
        let entity = patched(ENTITY, &edits).unwrap();
        assert!(entity.contains("import jakarta.persistence.Id;\nimport org.hibernate.annotations.JdbcTypeCode;\nimport org.hibernate.type.SqlTypes;\n"), "{entity}");
        assert!(entity.contains("    @JdbcTypeCode(SqlTypes.JSON)\n    private Map<String, Object> attributes;"), "{entity}");
        let bare = "package shop;\n\n@jakarta.persistence.Entity\npublic class Invoice {\n    private String title;\n}\n";
        let entity = patched(bare, &render_json(&doc(bare), "title").unwrap()).unwrap();
        assert!(entity.starts_with("package shop;\n\nimport org.hibernate.annotations.JdbcTypeCode;\nimport org.hibernate.type.SqlTypes;\n\n@jakarta"), "{entity}");
    }

    #[test]
    fn json_mapping_refuses_unoffered_attributes_and_existing_mentions() {
        for attribute in ["id", "missing", ""] {
            assert!(format!("{:?}", render_json(&doc(ENTITY), attribute).unwrap_err()).contains("offered"), "{attribute}");
        }
        for mention in ["private int JdbcTypeCode;", "private int SqlTypes;"] {
            let text = ENTITY.replace("private int count;", mention);
            assert!(render_json(&doc(&text), "title").is_err(), "{mention}");
        }
    }

    #[test]
    fn listeners_render_the_verified_signatures() {
        let cases = [
            (Listener::PostLoad, "implements PostLoadEventListener {\n    @Override\n    public void onPostLoad(PostLoadEvent event) {\n    }\n}\n", false),
            (Listener::PreInsert, "public boolean onPreInsert(PreInsertEvent event) {\n        return false;\n    }\n}\n", false),
            (Listener::PreUpdate, "public boolean onPreUpdate(PreUpdateEvent event) {\n        return false;\n    }\n}\n", false),
            (Listener::PreDelete, "public boolean onPreDelete(PreDeleteEvent event) {\n        return false;\n    }\n}\n", false),
            (Listener::PostInsert, "public void onPostInsert(PostInsertEvent event) {\n    }\n\n    @Override\n    public boolean requiresPostCommitHandling(EntityPersister persister) {\n        return false;\n    }\n}\n", true),
            (Listener::PostUpdate, "public void onPostUpdate(PostUpdateEvent event) {\n    }\n", true),
            (Listener::PostDelete, "public void onPostDelete(PostDeleteEvent event) {\n    }\n", true),
        ];
        for (listener, expected, persister) in cases {
            let name = default_listener_name("Invoice", listener);
            let (target, text) = render_listener(&doc(ENTITY), listener, &name).unwrap();
            assert_eq!(target, format!("/project/src/main/java/shop/{name}.java"));
            assert!(text.contains(expected), "{expected}\n{text}");
            assert!(text.contains(&format!("import org.hibernate.event.spi.{}Event;\nimport org.hibernate.event.spi.{}EventListener;\n", listener.event(), listener.event())));
            assert_eq!(text.contains("import org.hibernate.persister.entity.EntityPersister;"), persister, "{text}");
        }
        assert_eq!(default_listener_name("Invoice", Listener::PostLoad), "InvoicePostLoadListener");
    }

    #[test]
    fn listener_names_and_crlf_are_checked() {
        for name in ["", "Invoice", "PostLoadEventListener", "PostLoadEvent", "class", "Override", "Column", "Entity"] {
            assert!(render_listener(&doc(ENTITY), Listener::PostLoad, name).is_err(), "{name}");
        }
        let crlf = ENTITY.replace('\n', "\r\n");
        let (_, text) = render_listener(&doc(&crlf), Listener::PostInsert, "Audit").unwrap();
        assert!(!text.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn hibernate_6_is_proven_only_from_literal_versions() {
        let proven = [
            "implementation 'org.hibernate.orm:hibernate-core:6.5.3.Final'",
            "implementation(\"org.hibernate.orm:hibernate-core:6.0.0.Final\")",
            "api 'org.hibernate:hibernate-core:6.2.7.Final'",
            "<dependency><groupId>org.hibernate.orm</groupId><artifactId>hibernate-core</artifactId><version>6.4.1.Final</version></dependency>",
            "plugins { id 'org.springframework.boot' version '3.2.1' }\nimplementation 'org.springframework.boot:spring-boot-starter-data-jpa'",
            "plugins { id(\"org.springframework.boot\") version \"3.0.0\" }\nimplementation(\"org.springframework.boot:spring-boot-starter-data-jpa\")",
            "<parent><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-parent</artifactId><version>3.3.12</version></parent>\n<artifactId>spring-boot-starter-data-jpa</artifactId>",
        ];
        for build in proven {
            assert!(hibernate6(build).is_ok(), "{build}: {:?}", hibernate6(build));
        }
        let refused = [
            ("", "cannot be proven"),
            ("implementation 'org.hibernate:hibernate-core:5.6.15.Final'", "5.x"),
            ("implementation 'org.hibernate.orm:hibernate-core:7.2.12.Final'", "7.x"),
            ("implementation 'org.hibernate.orm:hibernate-core'", "not provable"),
            ("implementation \"org.hibernate.orm:hibernate-core:$hibernateVersion\"", "not provable"),
            ("<dependency><groupId>org.hibernate.orm</groupId><artifactId>hibernate-core</artifactId><version>${hibernate.version}</version></dependency>", "not provable"),
            ("implementation 'org.hibernate:hibernate-entitymanager:5.4.0'\nimplementation 'org.hibernate.orm:hibernate-core:6.5.3.Final'", "Hibernate 5"),
            ("plugins { id 'org.springframework.boot' version '2.7.18' }\nimplementation 'org.springframework.boot:spring-boot-starter-data-jpa'", "not provable from Spring Boot"),
            ("plugins { id 'org.springframework.boot' version '4.0.6' }\nimplementation 'org.springframework.boot:spring-boot-starter-data-jpa'", "not provable from Spring Boot"),
            ("plugins { id 'org.springframework.boot' version '3.2.1' }", "not provable from Spring Boot"),
            ("plugins { id 'org.springframework.boot' version '3.2.1' }\nimplementation 'org.hibernate.orm:hibernate-core:5.6.0'", "5.x"),
            ("plugins { id 'org.springframework.boot' version '2.7.1' }\nimplementation 'org.hibernate.orm:hibernate-core:6.1.0'", "not verified"),
            ("implementation 'org.springframework.boot:spring-boot-starter-data-jpa'", "not provable"),
        ];
        for (build, reason) in refused {
            let message = hibernate6(build).expect_err(build);
            assert!(message.contains(reason), "{build}: {message}");
        }
    }

    fn pick(plugin: &mut JpaAssist, document: &Document, title: &str) -> FileAction {
        plugin.file_actions(document, Range::default()).into_iter().find(|a| a.title == title).unwrap()
    }

    #[test]
    fn menu_entries_are_offered_for_plain_entities_only() {
        let (_host, mut plugin, document) = setup(ENTITY, GRADLE);
        for title in [TITLE_TYPE, TITLE_JSON, TITLE_LISTENER] {
            assert!(plugin.file_actions(&document, Range::default()).iter().any(|a| a.title == title), "{title}");
        }
    }

    #[test]
    fn resolve_type_returns_one_versioned_edit_and_a_no_overwrite_create() {
        let (host, mut plugin, document) = setup(ENTITY, GRADLE);
        let action = pick(&mut plugin, &document, TITLE_TYPE);
        for value in ["title", "TitleType", "fixed"] {
            host.answer_next_prompt(Some(&[value]));
        }
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&document, action).unwrap();
        assert_eq!(result.edits.len(), 1);
        assert_eq!(result.edits[0].0, PATH);
        let [FileOperation::Create { path, text, overwrite }] = result.operations.as_slice() else { panic!("one create") };
        assert_eq!(path, "/project/src/main/java/shop/TitleType.java");
        assert!(!overwrite && text.contains("UserType<String>") && !text.contains("ParameterizedType"));
        assert_eq!(host.editor_text(), ENTITY);
    }

    #[test]
    fn resolve_type_with_the_parameter_option_creates_a_parameterized_type() {
        let (host, mut plugin, document) = setup(ENTITY, GRADLE);
        let action = pick(&mut plugin, &document, TITLE_TYPE);
        for value in ["title", "TitleType", "parameters"] {
            host.answer_next_prompt(Some(&[value]));
        }
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&document, action).unwrap();
        let [FileOperation::Create { text, .. }] = result.operations.as_slice() else { panic!("one create") };
        assert!(text.contains("implements UserType<String>, ParameterizedType") && text.contains("setParameterValues(Properties parameters)"), "{text}");
    }

    #[test]
    fn resolve_json_returns_only_an_edit() {
        let (host, mut plugin, document) = setup(ENTITY, GRADLE);
        let action = pick(&mut plugin, &document, TITLE_JSON);
        host.answer_next_prompt(Some(&["attributes"]));
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&document, action).unwrap();
        assert_eq!(result.edits.len(), 1);
        assert!(result.operations.is_empty());
    }

    #[test]
    fn resolve_listener_returns_only_a_no_overwrite_create() {
        let (host, mut plugin, document) = setup(ENTITY, GRADLE);
        let action = pick(&mut plugin, &document, TITLE_LISTENER);
        for value in ["PostLoad", "InvoicePostLoadListener"] {
            host.answer_next_prompt(Some(&[value]));
        }
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&document, action).unwrap();
        assert!(result.edits.is_empty());
        let [FileOperation::Create { path, text, overwrite }] = result.operations.as_slice() else { panic!("one create") };
        assert_eq!(path, "/project/src/main/java/shop/InvoicePostLoadListener.java");
        assert!(!overwrite && text.contains("implements PostLoadEventListener"));
    }

    #[test]
    fn unprovable_hibernate_refuses_before_any_prompt() {
        for build in ["", "implementation 'org.hibernate:hibernate-core:5.6.15.Final'", "dependencies {}"] {
            for title in [TITLE_TYPE, TITLE_JSON, TITLE_LISTENER] {
                let (_host, mut plugin, document) = setup(ENTITY, if build.is_empty() { "// nothing" } else { build });
                let action = pick(&mut plugin, &document, title);
                assert!(plugin.resolve_file_action(&document, action).is_err(), "{build} {title}");
            }
        }
    }

    #[test]
    fn cancellation_collision_and_source_drift_never_apply_changes() {
        let (host, mut plugin, document) = setup(ENTITY, GRADLE);
        let action = pick(&mut plugin, &document, TITLE_TYPE);
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&document, action), Err(Error::Cancelled)));

        let action = pick(&mut plugin, &document, TITLE_TYPE);
        for value in ["title", "TitleType"] {
            host.answer_next_prompt(Some(&[value]));
        }
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&document, action), Err(Error::Cancelled)));

        host.add_file("src/main/java/shop/TitleType.kt", "class TitleType");
        let action = pick(&mut plugin, &document, TITLE_TYPE);
        for value in ["title", "TitleType"] {
            host.answer_next_prompt(Some(&[value]));
        }
        assert!(plugin.resolve_file_action(&document, action).is_err());

        let action = pick(&mut plugin, &document, TITLE_LISTENER);
        host.type_in_editor("// changed");
        assert!(plugin.resolve_file_action(&document, action).is_err());
    }

    #[test]
    fn oversized_sources_are_not_offered() {
        let text = format!("{ENTITY}// {}\n", "x".repeat(MAX_TEXT));
        let (_host, mut plugin, document) = setup(&text, GRADLE);
        let titles: Vec<_> = plugin.file_actions(&document, Range::default()).into_iter().map(|a| a.title).collect();
        assert!(![TITLE_TYPE, TITLE_JSON, TITLE_LISTENER].iter().any(|t| titles.iter().any(|a| a == t)));
    }

    fn javac(label: &str, classpath: &str, files: &[(String, String)]) {
        let dir = std::env::temp_dir().join(format!("jpa-hibernate-{}-{label}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths: Vec<_> = files
            .iter()
            .map(|(file, body)| {
                let path = dir.join(file);
                std::fs::write(&path, body).unwrap();
                path
            })
            .collect();
        let result = std::process::Command::new("javac")
            .args(["--release", "17", "-proc:none", "-classpath", classpath, "-d"])
            .arg(&dir)
            .args(&paths)
            .output()
            .expect("javac installed");
        assert!(result.status.success(), "{label}: {}\n{files:?}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_API / HIBERNATE_CORE_JAR jar paths"]
    fn generated_hibernate_sources_compile_against_hibernate_6() {
        let api = std::env::var("JPA_JAKARTA_API").expect("explicit API jar path");
        let core = std::env::var("HIBERNATE_CORE_JAR").expect("hibernate-core 6.x jar path");
        let classpath = format!("{api}:{core}");
        let money = ("Money.java".to_owned(), "package shop;\npublic class Money {}\n".to_owned());
        let mut count = 0;
        let text = ENTITY.replace("private int count;", "private Integer count;\n    private Boolean paid;\n    private Double ratio;\n    private Long size;");
        for (attribute, sql) in [("title", None), ("count", None), ("paid", None), ("ratio", None), ("size", None), ("total", None), ("price", Some("NUMERIC"))] {
            count += 1;
            let name = format!("{attribute}Type");
            let name = format!("{}{}", name[..1].to_uppercase(), &name[1..]);
            let (rendered, entity) = typed(&text, attribute, &name, sql);
            javac(&format!("type-{attribute}"), &classpath, &[(format!("{name}.java"), rendered.text), ("Invoice.java".into(), entity), money.clone()]);
        }
        for attribute in ["title", "attributes", "count", "price"] {
            count += 1;
            let entity = patched(&text, &render_json(&doc(&text), attribute).unwrap()).unwrap();
            javac(&format!("json-{attribute}"), &classpath, &[("Invoice.java".into(), entity), money.clone()]);
        }
        for listener in Listener::ALL {
            count += 1;
            let name = default_listener_name("Invoice", listener);
            let (_, source) = render_listener(&doc(&text), listener, &name).unwrap();
            javac(&format!("listener-{}", listener.event()), &classpath, &[(format!("{name}.java"), source)]);
        }
        assert_eq!(count, 18);
    }
}
