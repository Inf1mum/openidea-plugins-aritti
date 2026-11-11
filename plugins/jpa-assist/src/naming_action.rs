//! Configurable table/column name templates (settings `jpaAssist.tableName*` / `jpaAssist.columnName*`) and the
//! reviewed "Generate name constants" action. The action edits only the invoking document: names come from literal
//! `@Entity`/`@Table`/`@Column`/`@JoinColumn` names, everything else is derived and labelled as unverified.
use super::*;
use crate::model::{Annotation, value_range};

pub(super) const TITLE: &str = "Generate name constants with preview…";
const MAX_CONSTANTS: usize = 64;
const MAX_AFFIX: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Case {
    Snake,
    UpperSnake,
    Lower,
    Upper,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Template {
    case: Case,
    prefix: String,
    suffix: String,
}

fn affix(value: Option<&str>) -> String {
    let value = value.unwrap_or_default().trim();
    let valid = value.len() <= MAX_AFFIX && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    if valid { value.to_owned() } else { String::new() }
}

impl Template {
    /// Unknown case values and malformed prefixes/suffixes fall back to today's behavior.
    pub(super) fn new(case: Option<&str>, prefix: Option<&str>, suffix: Option<&str>) -> Self {
        let case = match case {
            Some("upper_snake_case") => Case::UpperSnake,
            Some("lowercase") => Case::Lower,
            Some("uppercase") => Case::Upper,
            _ => Case::Snake,
        };
        Self { case, prefix: affix(prefix), suffix: affix(suffix) }
    }

    fn from_settings(kind: &str) -> Self {
        let read = |key: &str| host::setting_string(&format!("jpaAssist.{kind}{key}"));
        Self::new(read("Case").as_deref(), read("Prefix").as_deref(), read("Suffix").as_deref())
    }

    pub(super) fn apply(&self, name: &str) -> String {
        let cased = match self.case {
            Case::Snake => snake_case(name),
            Case::UpperSnake => snake_case(name).to_uppercase(),
            Case::Lower => name.to_lowercase(),
            Case::Upper => name.to_uppercase(),
        };
        format!("{}{cased}{}", self.prefix, self.suffix)
    }
}

/// Default table name for an entity class name, per the configured template.
pub(crate) fn table_name(name: &str) -> String {
    Template::from_settings("tableName").apply(name)
}

/// Default column name for an attribute name, per the configured template.
pub(crate) fn column_name(name: &str) -> String {
    Template::from_settings("columnName").apply(name)
}

struct Constant {
    name: String,
    value: String,
    origin: String,
}

struct Source {
    class: Class,
    masked: String,
    class_at: usize,
}

fn source(doc: &Document) -> Result<Source, String> {
    let class = current_class(doc).filter(|c| c.kind == Kind::Entity).ok_or("Select a plain entity with no superclass.")?;
    let masked = mask(&doc.text);
    let class_at = crate::checks::owned_class_start(&doc.text, &masked, &class).ok_or("Only a single source-owned entity class is supported.")?;
    class.body_end.ok_or("Stale source.")?;
    Ok(Source { class, masked, class_at })
}

pub(super) fn eligible(doc: &Document) -> bool {
    source(doc).is_ok()
}

fn plain_name(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$'))
}

fn literal_name(doc: &Document, s: &Source, a: &Annotation) -> Result<Option<String>, Error> {
    let Some(range) = value_range(&doc.text, &s.masked, a, "name") else {
        return Ok(None);
    };
    let Some((_, inner)) = crate::model::plain_literal(&doc.text, range, s.class.kotlin) else {
        return Err(format!("A @{} name is not a plain literal; names cannot be proven.", a.name).into());
    };
    if !plain_name(inner) {
        return Err(format!("@{} name `{inner}` is quoted, empty or not a plain identifier; names cannot be proven.", a.name).into());
    }
    Ok(Some(inner.to_owned()))
}

fn constants(doc: &Document, s: &Source, derived: &str) -> Result<Vec<Constant>, Error> {
    let mut entity_name = None;
    let mut table_name_literal = None;
    for a in annotations(&s.masked) {
        match a.name {
            "JoinColumns" | "AttributeOverride" | "AttributeOverrides" | "SecondaryTable" | "SecondaryTables" => {
                return Err(format!("@{} is present; column names cannot be proven.", a.name).into());
            }
            "Entity" if a.start < s.class_at => entity_name = literal_name(doc, s, &a)?,
            "Table" if a.start < s.class_at => table_name_literal = literal_name(doc, s, &a)?,
            "Column" | "JoinColumn" => {
                literal_name(doc, s, &a)?;
            }
            _ => {}
        }
    }

    let mut found = vec![
        Constant {
            name: "ENTITY_NAME".into(),
            origin: if entity_name.is_some() { "literal @Entity name" } else { "default entity name = class name" }.into(),
            value: entity_name.unwrap_or_else(|| s.class.name.clone()),
        },
        Constant {
            name: "TABLE_NAME".into(),
            origin: if table_name_literal.is_some() { "literal @Table name".into() } else { derived.to_owned() },
            value: table_name_literal.unwrap_or_else(|| table_name(&s.class.name)),
        },
    ];
    for a in &s.class.attributes {
        let persistent = !a.mapped_by && a.join_table.is_none() && matches!(a.relation, Relation::None | Relation::ManyToOne | Relation::OneToOne);
        if !persistent {
            continue;
        }
        let (value, origin) = match &a.column {
            Some(column) => (column.clone(), "literal @Column/@JoinColumn name".to_owned()),
            None if a.relation.to_one() => (column_name(&format!("{}Id", a.name)), derived.to_owned()),
            None => (column_name(&a.name), derived.to_owned()),
        };
        if !plain_name(&value) {
            return Err(format!("Column name for `{}` is not a plain identifier.", a.name).into());
        }
        found.push(Constant { name: format!("COLUMN_{}", snake_case(&a.name).to_uppercase()), value, origin });
    }
    if found.len() > MAX_CONSTANTS {
        return Err("Too many constants for one reviewed edit.".into());
    }
    Ok(found)
}

fn render_block(found: &[Constant], kotlin: bool, nested: Option<&str>, indent: &str) -> String {
    let (head, inner, tail) = match (kotlin, nested) {
        (true, Some(name)) => (format!("{indent}object {name} {{\n"), format!("{indent}    "), format!("{indent}}}\n")),
        (true, None) => (format!("{indent}companion object {{\n"), format!("{indent}    "), format!("{indent}}}\n")),
        (false, Some(name)) => (format!("{indent}public static final class {name} {{\n"), format!("{indent}    "), format!("{indent}}}\n")),
        (false, None) => (String::new(), indent.to_owned(), String::new()),
    };
    let lines: String = found
        .iter()
        .map(|c| {
            if kotlin {
                format!("{inner}const val {} = \"{}\"\n", c.name, c.value)
            } else {
                format!("{inner}public static final String {} = \"{}\";\n", c.name, c.value)
            }
        })
        .collect();
    format!("{head}{lines}{tail}")
}

/// The constants block and its insertion edit; refuses collisions and unsupported shapes.
fn render(doc: &Document, s: &Source, found: &[Constant], nested: Option<&str>) -> Result<Vec<TextEdit>, Error> {
    let kotlin = s.class.kotlin;
    let mut seen = HashSet::new();
    for c in found {
        if !seen.insert(c.name.as_str()) {
            return Err(format!("Two attributes would both produce the constant {}.", c.name).into());
        }
        if has_word(&s.masked, &c.name) {
            return Err(format!("`{}` already exists in this file; existing constants are never overwritten or merged.", c.name).into());
        }
    }
    if let Some(name) = nested {
        if !identifier(name) || name == s.class.name || has_word(&s.masked, name) {
            return Err("Choose a valid holder name that does not occur in this source file.".into());
        }
    }
    if kotlin && nested.is_none() && has_word(&s.masked, "companion") {
        return Err("A companion object already exists; choose the nested holder instead.".into());
    }

    let close = s.class.body_end.ok_or(Error::Stale)?;
    let line_start = doc.text[..close].rfind('\n').map_or(0, |i| i + 1);
    if !doc.text[line_start..close].trim().is_empty() {
        return Err("The closing brace of the class must be on its own line.".into());
    }
    let crlf = doc.text.contains("\r\n");
    let mut block = render_block(found, kotlin, nested, "    ");
    block.insert(0, '\n');
    if crlf {
        block = block.replace('\n', "\r\n");
    }
    Ok(vec![TextEdit { range: Range::new(position(&doc.text, line_start), position(&doc.text, line_start)), text: block }])
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let s = source(doc).map_err(Error::from)?;
    let strategy = crate::naming_strategy::detect(project);
    let found = constants(doc, &s, &strategy.derived_label())?;
    let language = if s.class.kotlin { "Kotlin" } else { "Java" };
    let items = [
        PickItem::new("nested", "Nested holder class").description(if s.class.kotlin { "object inside the entity" } else { "static final class inside the entity" }),
        PickItem::new("direct", if s.class.kotlin { "Companion object" } else { "Entity class level" }).description("constants directly in the entity"),
    ];
    let destination = host::pick(&format!("Where to put the {language} name constants"), &items)?.ok_or(Error::Cancelled)?;
    let nested = match destination.as_str() {
        "nested" => Some(host::input("Holder name", Some("Names"))?.ok_or(Error::Cancelled)?.trim().to_owned()),
        "direct" => None,
        _ => return Err(Error::Stale),
    };
    let edits = render(doc, &s, &found, nested.as_deref())?;
    let after = checked_after(doc, &edits)?;
    let strategy_note = strategy.physical_label().map_or_else(|| "not verified".to_owned(), |label| format!("{label} ({})", strategy.source));
    let labels: String = found.iter().map(|c| format!("{} = \"{}\"  [{}]\n", c.name, c.value, c.origin)).collect();
    if !ui_kit::prompts::confirm_exact(
        "Generate name constants?",
        &format!(
            "Target: {}\nSource version: {}\n\n{labels}\n{after}\nNames come from literal annotation values or are derived with the configured naming templates; detected physical naming strategy: {strategy_note}. Derived values come from the templates, not from the strategy. Current source only. One undoable edit in this document; no imports.",
            doc.path, doc.version
        ),
    )? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    Ok(FileAction { title: "Generate name constants".into(), kind: Some("refactor.rewrite".into()), edits: vec![(doc.path.clone(), edits)], ..FileAction::default() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const JAVA: &str = "package shop;\n\nimport jakarta.persistence.Column;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.ManyToOne;\nimport jakarta.persistence.Table;\n\n@Entity\n@Table(name = \"invoices\")\npublic class Invoice {\n    @Id\n    private Long id;\n    @Column(name = \"invoice_no\")\n    private String number;\n    private String customerName;\n    @ManyToOne\n    private Customer customer;\n}\n";
    const KOTLIN: &str = "package shop\n\nimport jakarta.persistence.Column\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\nimport jakarta.persistence.Table\n\n@Entity\n@Table(name = \"invoices\")\nclass Invoice {\n    @Id var id: Long? = null\n    @Column(name = \"invoice_no\") var number: String? = null\n    var customerName: String? = null\n}\n";

    fn setup(text: &str, kotlin: bool) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let (dir, ext, language) = if kotlin { ("kotlin", "kt", "kotlin") } else { ("java", "java", "java") };
        let path = format!("/project/src/main/{dir}/shop/Invoice.{ext}");
        host.open_editor(&path, language, text, &[]);
        (host, JpaAssist::default(), Document { path, language: language.into(), text: text.into(), version: 1 })
    }

    fn run(text: &str, kotlin: bool, rows: &[&[&str]]) -> Result<String, Error> {
        let (host, mut plugin, doc) = setup(text, kotlin);
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).expect("offered");
        for row in rows {
            host.answer_next_prompt(Some(row));
        }
        let result = plugin.resolve_file_action(&doc, pending)?;
        assert_eq!(result.edits.len(), 1);
        assert!(result.operations.is_empty());
        patched(&doc.text, &result.edits[0].1)
    }

    #[test]
    fn templates_default_to_snake_case_and_apply_case_prefix_and_suffix() {
        assert_eq!(Template::new(None, None, None).apply("OrderLine"), "order_line");
        assert_eq!(Template::new(Some("bogus"), None, None).apply("OrderLine"), "order_line");
        assert_eq!(Template::new(Some("lowercase"), None, None).apply("OrderLine"), "orderline");
        assert_eq!(Template::new(Some("uppercase"), Some("T_"), None).apply("OrderLine"), "T_ORDERLINE");
        assert_eq!(Template::new(Some("upper_snake_case"), None, Some("_TBL")).apply("OrderLine"), "ORDER_LINE_TBL");
        assert_eq!(Template::new(None, Some("tbl_"), Some("_v")).apply("OrderLine"), "tbl_order_line_v");
    }

    #[test]
    fn malformed_affixes_fall_back_to_empty() {
        let bad = Template::new(None, Some("a b"), Some(&"x".repeat(33)));
        assert_eq!(bad, Template::new(None, None, None));
        assert_eq!(Template::new(None, Some("a-"), Some("\"")).apply("A"), "a");
    }

    #[test]
    fn settings_feed_table_and_column_defaults() {
        let host = MockHost::install();
        assert_eq!(table_name("OrderLine"), "order_line");
        assert_eq!(column_name("customerName"), "customer_name");
        host.set_setting("jpaAssist.tableNamePrefix", "\"app_\"");
        host.set_setting("jpaAssist.columnNameCase", "\"upper_snake_case\"");
        assert_eq!(table_name("OrderLine"), "app_order_line");
        assert_eq!(column_name("customerName"), "CUSTOMER_NAME");
    }

    #[test]
    fn java_nested_holder_uses_literal_and_derived_names() {
        let after = run(JAVA, false, &[&["nested"], &["Names"], &[]]).unwrap();
        assert!(after.contains("    public static final class Names {\n        public static final String ENTITY_NAME = \"Invoice\";\n        public static final String TABLE_NAME = \"invoices\";\n"), "{after}");
        assert!(after.contains("COLUMN_NUMBER = \"invoice_no\";"), "{after}");
        assert!(after.contains("COLUMN_CUSTOMER_NAME = \"customer_name\";"), "{after}");
        assert!(after.contains("COLUMN_CUSTOMER = \"customer_id\";"), "{after}");
        assert!(after.ends_with("    }\n}\n"), "{after}");
        assert!(!after.contains("import jakarta.persistence.Names"));
    }

    #[test]
    fn derived_label_names_the_detected_strategy() {
        let (host, mut plugin, doc) = setup(JAVA, false);
        host.add_file("src/main/resources/application.properties", "spring.jpa.hibernate.naming.physical-strategy=org.hibernate.boot.model.naming.PhysicalNamingStrategyStandardImpl\n");
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).expect("offered");
        host.answer_next_prompt(Some(&["direct"]));
        host.answer_next_prompt(Some(&[]));
        let seen = plugin.resolve_file_action(&doc, pending);
        assert!(seen.is_ok());
        let strategy = crate::naming_strategy::detect(&crate::host::project_of(&doc.path).unwrap());
        assert_eq!(strategy.derived_label(), "derived (PhysicalNamingStrategyStandardImpl)");
        let found = constants(&doc, &source(&doc).unwrap(), &strategy.derived_label()).unwrap();
        assert!(found.iter().any(|c| c.origin == "derived (PhysicalNamingStrategyStandardImpl)"));
        assert!(found.iter().all(|c| c.origin != "derived (naming strategy not verified)"));
    }

    #[test]
    fn java_direct_constants_sit_in_the_entity() {
        let after = run(JAVA, false, &[&["direct"], &[]]).unwrap();
        assert!(after.contains("\n    public static final String TABLE_NAME = \"invoices\";\n"), "{after}");
        assert!(!after.contains("class Names"));
    }

    #[test]
    fn kotlin_object_and_companion_render_const_vals() {
        let nested = run(KOTLIN, true, &[&["nested"], &["Names"], &[]]).unwrap();
        assert!(nested.contains("    object Names {\n        const val ENTITY_NAME = \"Invoice\"\n"), "{nested}");
        let companion = run(KOTLIN, true, &[&["direct"], &[]]).unwrap();
        assert!(companion.contains("    companion object {\n        const val ENTITY_NAME = \"Invoice\"\n        const val TABLE_NAME = \"invoices\"\n"), "{companion}");
        assert!(companion.contains("const val COLUMN_NUMBER = \"invoice_no\"\n"), "{companion}");
    }

    #[test]
    fn unspecified_table_is_derived_with_the_configured_template() {
        let text = JAVA.replace("@Table(name = \"invoices\")\n", "");
        let (host, mut plugin, doc) = setup(&text, false);
        host.set_setting("jpaAssist.tableNamePrefix", "\"t_\"");
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        for row in [&["direct"][..], &[]] {
            host.answer_next_prompt(Some(row));
        }
        let after = patched(&doc.text, &plugin.resolve_file_action(&doc, pending).unwrap().edits[0].1).unwrap();
        assert!(after.contains("TABLE_NAME = \"t_invoice\";"), "{after}");
    }

    #[test]
    fn crlf_is_preserved() {
        let text = JAVA.replace('\n', "\r\n");
        let after = run(&text, false, &[&["nested"], &["Names"], &[]]).unwrap();
        assert_eq!(after.matches('\n').count(), after.matches("\r\n").count(), "{after:?}");
    }

    #[test]
    fn existing_constants_and_holder_collisions_are_refused() {
        let with_constant = JAVA.replace("    private Long id;", "    public static final String TABLE_NAME = \"x\";\n    private Long id;");
        assert!(run(&with_constant, false, &[&["direct"], &[]]).is_err());
        let with_holder = JAVA.replace("    private Long id;", "    static final int Names = 1;\n    private Long id;");
        assert!(run(&with_holder, false, &[&["nested"], &["Names"], &[]]).is_err());
        let companion = KOTLIN.replace("    @Id var id", "    companion object { val x = 1 }\n    @Id var id");
        assert!(run(&companion, true, &[&["direct"], &[]]).is_err());
        assert!(run(JAVA, false, &[&["nested"], &["Invoice"], &[]]).is_err());
        assert!(run(JAVA, false, &[&["nested"], &["1x"], &[]]).is_err());
    }

    #[test]
    fn unprovable_names_are_refused() {
        let dynamic = JAVA.replace("name = \"invoice_no\"", "name = PREFIX + \"no\"");
        assert!(run(&dynamic, false, &[&["direct"], &[]]).is_err());
        let quoted = JAVA.replace("name = \"invoice_no\"", "name = \"\\\"No\\\"\"");
        assert!(run(&quoted, false, &[&["direct"], &[]]).is_err());
        let overrides = JAVA.replace("    private String customerName;", "    @AttributeOverride(name = \"a\", column = @Column(name = \"b\"))\n    private String customerName;");
        assert!(run(&overrides, false, &[&["direct"], &[]]).is_err());
    }

    #[test]
    fn two_attributes_with_one_constant_name_are_refused() {
        let text = JAVA.replace("    private String customerName;", "    private String customerName;\n    private String customer_name;");
        assert!(run(&text, false, &[&["direct"], &[]]).is_err());
    }

    #[test]
    fn cancelled_prompts_and_stale_documents_apply_nothing() {
        let (host, mut plugin, doc) = setup(JAVA, false);
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)));

        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        let stale = Document { version: 2, text: format!("{JAVA}\n"), ..doc };
        assert!(matches!(plugin.resolve_file_action(&stale, pending), Err(Error::Stale)));
    }

    #[test]
    fn subclass_and_non_entity_are_not_offered() {
        let sub = JAVA.replace("class Invoice", "class Invoice extends Base");
        let (_h, mut plugin, doc) = setup(&sub, false);
        assert!(plugin.file_actions(&doc, Range::default()).iter().all(|a| a.title != TITLE));
    }

    #[test]
    #[ignore = "requires javac/kotlinc and JPA_JAKARTA_API / JPA_JAVAX_API (and Kotlin env vars)"]
    fn generated_constants_compile_against_both_persistence_apis() {
        for kotlin in [false, true] {
            for ns in ["jakarta", "javax"] {
                for rows in [&[&["nested"][..], &["Names"], &[]][..], &[&["direct"], &[]]] {
                    let template = if kotlin { KOTLIN } else { JAVA };
                    let source = run(&template.replace("jakarta", ns), kotlin, rows).unwrap();
                    compile(kotlin, ns, &source);
                }
            }
        }
    }

    fn compile(kotlin: bool, ns: &str, source: &str) {
        let jar = std::env::var(if ns == "jakarta" { "JPA_JAKARTA_API" } else { "JPA_JAVAX_API" }).expect("explicit API jar path");
        let dir = std::env::temp_dir().join(format!("jpa-names-compile-{}-{kotlin}-{ns}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ext = if kotlin { "kt" } else { "java" };
        let stub = if kotlin {
            format!("package shop

import {ns}.persistence.Entity
import {ns}.persistence.Id

@Entity
open class Customer {{
    @Id
    open var id: Long? = null
}}
")
        } else {
            format!("package shop;\n\nimport {ns}.persistence.Entity;\nimport {ns}.persistence.Id;\n\n@Entity\npublic class Customer {{\n    @Id\n    private Long id;\n}}\n")
        };
        let files = [dir.join(format!("Customer.{ext}")), dir.join(format!("Invoice.{ext}"))];
        std::fs::write(&files[0], stub).unwrap();
        std::fs::write(&files[1], source).unwrap();
        let result = if kotlin {
            let compiler = std::env::var("KOTLIN_COMPILER_CLASSPATH").expect("explicit Kotlin compiler classpath");
            let stdlib = std::env::var("KOTLIN_STDLIB").expect("explicit Kotlin standard library jar");
            std::process::Command::new("java").args(["-classpath", &compiler, "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler", "-no-stdlib", "-no-reflect", "-jvm-target", "17", "-classpath", &format!("{jar}:{stdlib}"), "-d"]).arg(&dir).args(&files).output().expect("java installed")
        } else {
            std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &jar, "-d"]).arg(&dir).args(&files).output().expect("javac installed")
        };
        assert!(result.status.success(), "{ns} kotlin={kotlin}: {}\n{source}", String::from_utf8_lossy(&result.stderr));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
