//! "Entity from POJO": reviewed conversion of an existing plain Java or Kotlin class into a JPA entity.
//! Only the invoking document is edited. Annotations are written fully qualified so no import is guessed.
use super::*;
use crate::model::{closing, is_ident};
use persistence_core::generation::{IdentifierType, Namespace};

pub(super) const TITLE: &str = "Entity from POJO with preview…";
const PERSISTENCE_NAMES: [&str; 8] = ["Entity", "MappedSuperclass", "Embeddable", "Table", "Id", "EmbeddedId", "IdClass", "Access"];

struct Pojo {
    name: String,
    kotlin: bool,
    line_start: usize,
    open: usize,
    close: usize,
    members: Vec<(usize, usize)>,
    lombok_accessors: bool,
    masked: String,
}

fn depth_at(masked: &str, at: usize) -> i32 {
    masked[..at].bytes().fold(0, |depth, c| depth + i32::from(c == b'{') - i32::from(c == b'}'))
}

fn scan(doc: &Document) -> Result<Pojo, &'static str> {
    let kotlin = doc.language == "kotlin";
    if parse_class(&doc.text, &doc.path).is_some() {
        return Err("The file already declares a persistence class.");
    }
    let masked = mask(&doc.text);
    let found = annotations(&masked);
    if found.iter().any(|a| PERSISTENCE_NAMES.contains(&a.name)) {
        return Err("The file already uses persistence annotations.");
    }
    let mut declarations = Vec::new();
    for keyword in ["class", "interface", "enum", "record", "object"] {
        for at in words(&masked, keyword) {
            if depth_at(&masked, at) == 0 && !masked[..at].trim_end().ends_with("::") && !masked[..at].trim_end().ends_with('@') {
                declarations.push((at, keyword));
            }
        }
    }
    let [(class_at, "class")] = declarations.as_slice() else {
        return Err("Exactly one top-level class is supported.");
    };
    let class_at = *class_at;
    let line_start = masked[..class_at].rfind('\n').map_or(0, |i| i + 1);
    let allowed: &[&str] = if kotlin { &["public", "internal", "open", "abstract"] } else { &["public", "abstract"] };
    if !masked[line_start..class_at].split_whitespace().all(|word| allowed.contains(&word)) {
        return Err("Put annotations on their own line and use only public or abstract modifiers (no final, data or sealed class).");
    }
    let after = &masked[class_at + "class".len()..];
    let name_start = class_at + "class".len() + after.len() - after.trim_start().len();
    let name_len = masked[name_start..].bytes().take_while(|&c| is_ident(c)).count();
    let name = masked[name_start..name_start + name_len].to_owned();
    if name.is_empty() {
        return Err("The class name is not readable.");
    }
    let mut header_from = name_start + name_len;
    if kotlin && masked[header_from..].trim_start().starts_with('(') {
        let paren = header_from + masked[header_from..].find('(').ok_or("Stale source.")?;
        header_from = closing(masked.as_bytes(), paren).ok_or("Unbalanced constructor.")? + 1;
    }
    let open = header_from + masked[header_from..].find('{').ok_or("The class needs a body: add braces first.")?;
    let header = &masked[header_from..open];
    if header.contains('<') || (!kotlin && words(header, "extends").next().is_some()) || (kotlin && header.contains(':') && !header.contains("Serializable")) {
        return Err("Generic classes and classes with a superclass are not converted.");
    }
    let close = closing(masked.as_bytes(), open).ok_or("Unbalanced class body.")?;
    if masked[close + 1..].trim().len() > 0 || masked[..line_start].lines().any(|l| l.trim_start().starts_with("import ") && l.contains(" as ")) {
        return Err("Unsupported file layout.");
    }
    let members = crate::model::members(&masked, open + 1, close, kotlin, false);
    let lombok_accessors = found.iter().any(|a| matches!(a.name, "Data" | "Getter" | "Setter" | "Value"));
    Ok(Pojo { name, kotlin, line_start, open, close, members, lombok_accessors, masked })
}

pub(super) struct PojoField {
    pub(super) name: String,
    pub(super) ty: String,
    pub(super) line_start: usize,
    pub(super) indent: String,
}

/// Instance fields of a Java class that is eligible for conversion, with the offset of their line.
pub(super) fn fields(doc: &Document) -> Result<Vec<PojoField>, Error> {
    let pojo = scan(doc).map_err(Error::from)?;
    if pojo.kotlin {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for &(start, end) in &pojo.members {
        let clean = crate::model::without_annotations(&pojo.masked, start, end);
        let Some(declaration) = clean.trim().strip_suffix(';') else {
            continue;
        };
        let head = declaration.split('=').next().unwrap_or_default().trim();
        if head.contains('(') || head.split_whitespace().any(|word| matches!(word, "static" | "transient")) {
            continue;
        }
        let Some((left, name)) = head.rsplit_once(char::is_whitespace) else {
            continue;
        };
        let Some(ty) = left.split_whitespace().last() else {
            continue;
        };
        let first = start + pojo.masked[start..].len() - pojo.masked[start..].trim_start().len();
        let line_start = pojo.masked[..first].rfind('\n').map_or(0, |i| i + 1);
        found.push(PojoField { name: name.trim().to_owned(), ty: ty.to_owned(), line_start, indent: pojo.masked[line_start..first].to_owned() });
    }
    Ok(found)
}

pub(super) fn eligible(doc: &Document) -> bool {
    doc.text.len() <= MAX_TEXT && scan(doc).is_ok()
}

fn accepted(id: IdentifierType) -> &'static [&'static str] {
    match id {
        IdentifierType::Long => &["Long", "long", "java.lang.Long"],
        IdentifierType::Integer => &["Integer", "int", "java.lang.Integer"],
        IdentifierType::Short => &["Short", "short", "java.lang.Short"],
        IdentifierType::String => &["String", "java.lang.String"],
        IdentifierType::Uuid => &["UUID", "java.util.UUID"],
        IdentifierType::BigInteger => &["BigInteger", "java.math.BigInteger"],
    }
}

fn field_type(pojo: &Pojo, member: (usize, usize), name: &str) -> Option<String> {
    let clean = crate::model::without_annotations(&pojo.masked, member.0, member.1);
    let declaration = clean.trim().strip_suffix(';')?;
    let head = declaration.split('=').next()?.trim();
    let mut tokens = head.split_whitespace().rev();
    (tokens.next()? == name).then(|| tokens.next().map(str::to_owned)).flatten()
}

fn constructors(pojo: &Pojo) -> Vec<bool> {
    pojo.members
        .iter()
        .filter(|&&(s, f)| !pojo.masked[..f].ends_with(';') && s < f)
        .filter_map(|&(s, f)| {
            let clean = crate::model::without_annotations(&pojo.masked, s, f);
            let paren = clean.find('(')?;
            let words: Vec<_> = clean[..paren].split_whitespace().collect();
            (words.last() == Some(&pojo.name.as_str()) && words.len() <= 2).then(|| clean[paren..].trim_start().starts_with("()"))
        })
        .collect()
}

pub(super) fn plan(doc: &Document, namespace: Namespace, table: &str, draft: &persistence_core::generation::IdentifierDraft) -> Result<Vec<TextEdit>, Error> {
    let pojo = scan(doc).map_err(Error::from)?;
    draft.validate(namespace).map_err(Error::from)?;
    let ns = namespace.package();
    let name = &pojo.name;
    if ["java", "kotlin", "jakarta", "javax"].iter().any(|p| name == p) {
        return Err("The class name shadows a package used by the generated annotations.".into());
    }
    let imported = doc.text.lines().any(|l| {
        l.trim_start().strip_prefix("import ").is_some_and(|i| ["java", "kotlin", "jakarta", "javax"].contains(&i.trim().trim_end_matches(';').rsplit('.').next().unwrap_or("")))
    });
    if imported {
        return Err("An import named like a package would shadow the generated annotations.".into());
    }
    let table = table.trim();
    if table.is_empty() || table.len() > 128 || !table.split('.').all(identifier) {
        return Err("Use a literal table identifier (optionally schema-qualified).".into());
    }
    let to_text = |at: usize, text: String| {
        let text = if doc.text.contains("\r\n") { text.replace('\n', "\r\n") } else { text };
        TextEdit { range: Range::new(position(&doc.text, at), position(&doc.text, at)), text }
    };
    let mut edits = vec![to_text(pojo.line_start, format!("@{ns}.Entity\n@{ns}.Table(name = \"{table}\")\n"))];

    let body = &pojo.masked[pojo.open..pojo.close];
    let suffix = capitalize(&draft.name);
    let field = pojo.members.iter().find_map(|&m| field_type(&pojo, m, &draft.name).map(|ty| (m, ty)));
    let accessor_clash = [format!("get{suffix}"), format!("set{suffix}")].iter().any(|n| has_word(body, n));
    match field {
        Some(((start, _), ty)) => {
            if pojo.kotlin || !accepted(draft.id_type).contains(&ty.as_str()) {
                return Err(format!("`{}` already exists with type `{ty}`; choose another identifier name or type.", draft.name).into());
            }
            let rendered = crate::wizard::identifier_source(draft, namespace, false, false, false).map_err(Error::from)?;
            let annotations: Vec<_> = rendered.lines().collect();
            let annotations = annotations[..annotations.len() - 1].join("\n");
            let first = start + pojo.masked[start..].len() - pojo.masked[start..].trim_start().len();
            let at = pojo.masked[..first].rfind('\n').map_or(0, |i| i + 1);
            edits.push(to_text(at, format!("{annotations}\n")));
        }
        None => {
            if accessor_clash || has_word(body, &draft.name) {
                return Err("This identifier name already occurs in the class. Choose another name.".into());
            }
            let accessors = !pojo.lombok_accessors;
            let rendered = crate::wizard::identifier_source(draft, namespace, pojo.kotlin, accessors, false).map_err(Error::from)?;
            edits.push(to_text(pojo.open + 1, format!("\n{rendered}\n")));
        }
    }

    let ctors = constructors(&pojo);
    if !pojo.kotlin && !ctors.is_empty() && !ctors.contains(&true) {
        edits.push(to_text(pojo.close, format!("\n    protected {name}() {{\n    }}\n")));
    }
    Ok(edits)
}

pub(super) fn resolve(pending: &Pending, doc: &Document) -> Result<FileAction, Error> {
    let project = current(pending, doc)?;
    let pojo = scan(doc).map_err(Error::from)?;
    let items = [PickItem::new("jakarta", "Jakarta Persistence"), PickItem::new("javax", "Javax Persistence")];
    let namespace = match host::pick("Persistence namespace for the new entity", &items)?.ok_or(Error::Cancelled)?.as_str() {
        "jakarta" => Namespace::Jakarta,
        "javax" => Namespace::Javax,
        _ => return Err(Error::Stale),
    };
    let table = host::input("Table name", Some(&naming_table_name(&pojo.name)))?.ok_or(Error::Cancelled)?;
    let draft = super::identifier_action::prompt(namespace, &pojo.name)?;
    let (relation_edits, relation_notes) = pojo_relations::choose(&project, &pending.root, doc, namespace)?;
    let mut edits = plan(doc, namespace, &table, &draft)?;
    edits.extend(relation_edits);
    let after = checked_after(doc, &edits)?;
    if !ui_kit::prompts::confirm_exact(
        "Convert class to JPA entity?",
        &format!(
            "Target: {}\nSource version: {}\n\n{after}\nCurrent source only: the existing members are not mapped or changed (except the relationships chosen above); review relationships, column types and the no-argument constructor.{relation_notes}",
            doc.path, doc.version
        ),
    )? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    Ok(FileAction {
        title: format!("Convert {} to JPA entity", pojo.name),
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), edits)],
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const JAVA: &str = "package shop;\n\nimport java.util.Date;\n\npublic class Invoice {\n\n    private String title;\n\n    private Date issued;\n\n    public String getTitle() {\n        return title;\n    }\n}\n";
    const KOTLIN: &str = "package shop\n\nclass Invoice(val title: String) {\n    var note: String? = null\n}\n";

    fn setup(text: &str, language: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let path = format!("/project/src/main/{language}/shop/Invoice.{}", if language == "java" { "java" } else { "kt" });
        host.open_editor(&path, language, text, &[]);
        (host, JpaAssist::default(), Document { path, language: language.into(), text: text.into(), version: 1 })
    }

    fn convert(text: &str, language: &str, answers: &[&[&str]]) -> Result<String, Error> {
        let (host, mut plugin, doc) = setup(text, language);
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).expect("offered");
        for row in answers {
            host.answer_next_prompt(Some(row));
        }
        let result = plugin.resolve_file_action(&doc, pending)?;
        assert_eq!(host.editor_text(), text);
        assert!(result.operations.is_empty());
        Ok(patched(text, &result.edits[0].1).unwrap())
    }

    fn jakarta_assigned(name: &str) -> Vec<Vec<&str>> {
        vec![vec!["jakarta"], vec!["invoices"], vec![name], vec![""], vec!["long"], vec!["identity"], vec![]]
    }

    fn run(text: &str, language: &str, rows: Vec<Vec<&str>>) -> Result<String, Error> {
        let rows: Vec<&[&str]> = rows.iter().map(Vec::as_slice).collect();
        convert(text, language, &rows)
    }

    #[test]
    fn java_class_becomes_an_entity_with_a_new_identifier() {
        let after = run(JAVA, "java", jakarta_assigned("id")).unwrap();
        for expected in [
            "@jakarta.persistence.Entity\n@jakarta.persistence.Table(name = \"invoices\")\npublic class Invoice {",
            "@jakarta.persistence.Id\n    @jakarta.persistence.GeneratedValue(strategy = jakarta.persistence.GenerationType.IDENTITY)",
            "private java.lang.Long id;",
            "public java.lang.Long getId()",
            "private String title;",
        ] {
            assert!(after.contains(expected), "{expected}\n{after}");
        }
        assert!(!after.contains("protected Invoice()"));
    }

    #[test]
    fn existing_identifier_field_is_annotated_instead_of_duplicated() {
        let text = JAVA.replace("private String title;", "private long id;\n\n    private String title;");
        let after = run(&text, "java", jakarta_assigned("id")).unwrap();
        assert!(after.contains("    @jakarta.persistence.Id\n") && after.contains("\n    private long id;"), "{after}");
        assert_eq!(after.matches(" id;").count(), 1, "{after}");
        let text = JAVA.replace("private String title;", "private String id;");
        let error = run(&text, "java", jakarta_assigned("id")).unwrap_err();
        assert!(format!("{error:?}").contains("already exists with type"), "{error:?}");
    }

    #[test]
    fn argument_only_constructors_get_a_protected_no_arg_constructor() {
        let text = JAVA.replace("    public String getTitle()", "    public Invoice(String title) {\n        this.title = title;\n    }\n\n    public String getTitle()");
        let after = run(&text, "java", jakarta_assigned("id")).unwrap();
        assert!(after.contains("    protected Invoice() {\n    }\n}"), "{after}");
        let with_default = text.replace("    public Invoice(String title)", "    public Invoice() {\n    }\n\n    public Invoice(String title)");
        assert!(!run(&with_default, "java", jakarta_assigned("id")).unwrap().contains("protected Invoice()"));
    }

    #[test]
    fn kotlin_class_with_a_body_becomes_an_entity() {
        let after = run(KOTLIN, "kotlin", vec![vec!["javax"], vec!["invoices"], vec!["key"], vec![""], vec!["uuid"], vec!["assigned"], vec![]]).unwrap();
        for expected in ["@javax.persistence.Entity\n@javax.persistence.Table(name = \"invoices\")\nclass Invoice(val title: String) {", "var key: java.util.UUID? = null"] {
            assert!(after.contains(expected), "{expected}\n{after}");
        }
    }

    #[test]
    fn lombok_accessor_annotations_suppress_generated_accessors() {
        let text = JAVA.replace("public class", "@lombok.Data\npublic class");
        let after = run(&text, "java", jakarta_assigned("id")).unwrap();
        assert!(after.contains("private java.lang.Long id;") && !after.contains("getId"), "{after}");
    }

    #[test]
    fn classes_that_are_not_plain_single_classes_are_not_offered() {
        for text in [
            JAVA.replace("public class", "@jakarta.persistence.Entity\npublic class"),
            JAVA.replace("public class Invoice {", "public final class Invoice {"),
            JAVA.replace("public class Invoice {", "public class Invoice extends Base {"),
            JAVA.replace("public class Invoice {", "public class Invoice<T> {"),
            JAVA.replace("public class Invoice {", "public interface Invoice {"),
            format!("{JAVA}\nclass Other {{\n}}\n"),
            KOTLIN.replace(" {\n    var note: String? = null\n}", ""),
        ] {
            let kotlin = text.contains("package shop\n");
            let (_host, mut plugin, doc) = setup(&text, if kotlin { "kotlin" } else { "java" });
            assert!(!plugin.file_actions(&doc, Range::default()).iter().any(|a| a.title == TITLE), "{text}");
        }
    }

    #[test]
    fn cancelling_any_prompt_or_editing_the_source_applies_nothing() {
        let (host, mut plugin, doc) = setup(JAVA, "java");
        for answered in 0..6 {
            let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
            for row in jakarta_assigned("id").iter().take(answered) {
                host.answer_next_prompt(Some(row));
            }
            host.answer_next_prompt(None);
            assert!(matches!(plugin.resolve_file_action(&doc, pending), Err(Error::Cancelled)), "{answered}");
        }
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.type_in_editor("// changed");
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
    }

    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_API / JPA_JAVAX_API jar paths"]
    fn converted_classes_compile_against_both_persistence_apis() {
        for (ns, variable) in [("jakarta", "JPA_JAKARTA_API"), ("javax", "JPA_JAVAX_API")] {
            let jar = std::env::var(variable).expect("explicit API jar path");
            let with_ctor = JAVA.replace("    public String getTitle()", "    public Invoice(String title) {\n        this.title = title;\n    }\n\n    public String getTitle()");
            let existing = JAVA.replace("private String title;", "private long id;\n\n    private String title;");
            let dir = std::env::temp_dir().join(format!("jpa-entity-from-class-{}-{ns}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            for (index, text) in [JAVA.to_owned(), with_ctor, existing].iter().enumerate() {
                let rows = vec![vec![ns], vec!["invoices"], vec!["id"], vec![""], vec!["long"], vec!["identity"], vec![]];
                let after = run(text, "java", rows).unwrap();
                let package_dir = dir.join(format!("case{index}"));
                std::fs::create_dir_all(&package_dir).unwrap();
                let file = package_dir.join("Invoice.java");
                std::fs::write(&file, after.replace("package shop;", &format!("package shop{index};"))).unwrap();
                let result = std::process::Command::new("javac")
                    .args(["--release", "17", "-proc:none", "-classpath", &jar, "-d"])
                    .arg(&dir)
                    .arg(&file)
                    .output()
                    .expect("javac installed");
                assert!(result.status.success(), "{ns} case {index}: {}\n{after}", String::from_utf8_lossy(&result.stderr));
            }
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
