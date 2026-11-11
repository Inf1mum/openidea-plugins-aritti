//! Source-only table navigation, deliberately separate from Rename identities.
use openidea_plugin::{Location, Position, Project, Range, host, providers::Document, text::position};
use std::{collections::{BTreeMap, BTreeSet}, time::{Duration, Instant}};
#[derive(Clone)]
struct Token { word: String, start: usize, end: usize }
fn tokens(text: &str) -> Option<Vec<Token>> {
    if text.len() > 128 << 10 { return None; }
    let bytes = text.as_bytes(); let mut at = 0; let mut out = Vec::new();
    while at < bytes.len() {
        if out.len() >= 16384 { return None; }
        if bytes[at].is_ascii_whitespace() { at += 1; continue; }
        if text[at..].starts_with("--") { at += text[at..].find('\n').unwrap_or(text.len()-at); continue; }
        if text[at..].starts_with("/*") {
            let end = text[at+2..].find("*/")? + at + 2;
            if text[at+2..end].contains("/*") { return None; }
            at = end + 2; continue;
        }
        if bytes[at] == b'\'' {
            at += 1;
            loop {
                if at >= bytes.len() { return None; }
                if bytes[at] == b'\'' {
                    at += 1;
                    if bytes.get(at) == Some(&b'\'') { at += 1; } else { break; }
                } else if bytes[at] == b'\\' { return None; } else { at += 1; }
            }
            out.push(Token { word: "<literal>".into(), start: at, end: at }); continue;
        }
        // Quoted identifiers/dollar quoting need dialect-specific ownership.
        if matches!(bytes[at], b'"' | b'`' | b'[' | b'$') { return None; }
        let start = at;
        if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' {
            at += 1;
            while bytes.get(at).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_') { at += 1; }
            out.push(Token { word: text[start..at].to_ascii_lowercase(), start, end: at });
        } else {
            let ch = text[at..].chars().next()?; at += ch.len_utf8();
            out.push(Token { word: ch.to_string(), start, end: at });
        }
    }
    Some(out)
}
fn identifier(word: &str) -> bool { word.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') && word.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') }
#[derive(Clone)]
struct Table { name: String, range: Range, definition: bool }
fn tables(text: &str) -> Option<Vec<Table>> {
    let tokens = tokens(text)?; let mut out = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        let previous = i.checked_sub(1).and_then(|j| tokens.get(j)).map(|t| t.word.as_str());
        let definition = token.word == "table" && previous == Some("create");
        let reference = (matches!(token.word.as_str(), "from" | "join" | "references") || token.word == "update" && previous != Some("on"))
            || token.word == "into" && previous == Some("insert")
            || token.word == "table" && matches!(previous, Some("alter" | "drop" | "truncate"));
        if !definition && !reference { continue; }
        let mut next = i + 1;
        if tokens.get(next).is_some_and(|t| t.word == "if") {
            next += 1;
            if tokens.get(next).is_some_and(|t| t.word == "not") { next += 1; }
            if !tokens.get(next).is_some_and(|t| t.word == "exists") { continue; }
            next += 1;
        }
        let Some(first) = tokens.get(next).filter(|t| identifier(&t.word)) else { continue; };
        let mut name = first.word.clone(); let mut end = first.end;
        while tokens.get(next + 1).is_some_and(|t| t.word == ".") {
            let Some(part) = tokens.get(next + 2).filter(|t| identifier(&t.word)) else { return None; };
            name.push('.'); name.push_str(&part.word); end = part.end; next += 2;
        }
        out.push(Table { name, range: Range::new(position(text, first.start), position(text, end)), definition });
    }
    Some(out)
}
fn persistence_annotation(file: &spring_text::java::JavaFile, ann: &spring_text::java::Annotation, text: &str) -> bool {
    let written = &text[ann.at + 1..ann.name_at + ann.name.len()];
    if written.contains('.') { return written == format!("jakarta.persistence.{}", ann.name) || written == format!("javax.persistence.{}", ann.name); }
    let exact: Vec<_> = file.imports.iter().filter(|import| !import.is_static && import.path.ends_with(&format!(".{}", ann.name))).collect();
    if !exact.is_empty() { return exact.len() == 1 && [format!("jakarta.persistence.{}", ann.name), format!("javax.persistence.{}", ann.name)].contains(&exact[0].path); }
    file.imports.iter().filter(|import| !import.is_static && import.path.ends_with(".*")).count() == 1
        && file.imports.iter().any(|import| !import.is_static && matches!(import.path.as_str(), "jakarta.persistence.*" | "javax.persistence.*"))
        && !file.types.iter().any(|ty| ty.name == ann.name)
}
fn entity_tables(text: &str) -> Vec<(String, Range)> {
    if text.len() > 128 << 10 { return vec![]; }
    let file = spring_text::java::parse(text); let mut out = Vec::new();
    // Constants are resolved from this file only; cross-file names stay quiet.
    let mut constants = spring_text::constants::Constants::default();
    constants.add(&file, text);
    for ty in &file.types {
        if ty.kind != "class" || !ty.annotation("Entity").is_some_and(|ann| persistence_annotation(&file, ann, text)) { continue; }
        let Some(table) = ty.annotation("Table").filter(|ann| persistence_annotation(&file, ann, text)) else { continue; };
        let literal = |key: &str| table.arg(key).and_then(|arg| {
            if arg.strings.len() == 1 && arg.text.trim() == format!("\"{}\"", arg.strings[0].value) { return Some(arg.strings[0].value.clone()); }
            constants.resolve(&arg.text, Some(&ty.name))
        });
        let Some(name) = literal("name").filter(|value| identifier(value)) else { continue; };
        let name = if table.arg("schema").is_some() {
            let Some(schema) = literal("schema").filter(|value| identifier(value)) else { continue; };
            format!("{schema}.{name}")
        } else { name };
        out.push((name.to_ascii_lowercase(), Range::new(position(text, ty.at), position(text, ty.at + ty.name.len()))));
    }
    out
}
/// Spring Boot's default physical naming for a plain `CamelCase` class name; none when the
/// conversion is ambiguous (digits, acronyms, non-ASCII).
fn snake_case(name: &str) -> Option<String> {
    let bytes = name.as_bytes();
    if !bytes.first()?.is_ascii_uppercase() || !bytes.last()?.is_ascii_lowercase() || !bytes.iter().all(u8::is_ascii_alphabetic)
        || bytes.windows(2).any(|pair| pair[0].is_ascii_uppercase() && pair[1].is_ascii_uppercase()) { return None; }
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if i > 0 && c.is_ascii_uppercase() { out.push('_'); }
        out.push(c.to_ascii_lowercase());
    }
    Some(out)
}
/// Top-level `@Entity` classes without `@Table`/`@Entity` arguments/superclass: the table is the
/// implicitly named one. Only valid under Spring Boot's default naming strategies (see `implicit_naming_default`).
fn implicit_entity_tables(text: &str) -> Vec<(String, Range)> {
    if text.len() > 128 << 10 { return vec![]; }
    let file = spring_text::java::parse(text);
    file.types.iter().filter(|ty| ty.kind == "class" && ty.outer.is_none() && ty.extends.is_empty() && !ty.has(&["Table", "SecondaryTable", "Inheritance", "MappedSuperclass", "Embeddable"]) && !ty.modifiers.iter().any(|m| m == "abstract"))
        .filter(|ty| ty.annotation("Entity").is_some_and(|ann| ann.args.is_empty() && persistence_annotation(&file, ann, text)))
        .filter_map(|ty| Some((snake_case(&ty.name)?, Range::new(position(text, ty.at), position(text, ty.at + ty.name.len())))))
        .collect()
}
/// True only with Spring Boot declared at the root and no naming/quoting override anywhere visible.
fn implicit_naming_default(project: &Project, root: &str) -> bool {
    let boot = ["pom.xml", "build.gradle", "build.gradle.kts"].iter()
        .any(|name| project.read_file(&format!("{root}/{name}")).is_ok_and(|text| text.contains("spring-boot")));
    if !boot { return false; }
    for glob in ["**/application*.properties", "**/application*.yml", "**/application*.yaml", "**/persistence.xml", "**/hibernate.properties", "**/hibernate.cfg.xml"] {
        let Ok(files) = project.find_files(glob, 33) else { return false; };
        if files.len() > 32 { return false; }
        for path in files {
            if path.ends_with("persistence.xml") || path.ends_with("hibernate.properties") || path.ends_with("hibernate.cfg.xml") { return false; }
            let Ok(text) = project.read_file(&path) else { return false; };
            let lower = text.to_ascii_lowercase();
            if lower.contains("naming") || lower.contains("quoted") { return false; }
        }
    }
    true
}
fn module(path: &str) -> Option<&str> { path.split_once("/src/").map(|(prefix, _)| prefix) }
struct Owner { files: BTreeSet<String>, definitions: BTreeMap<String, Vec<Location>>, at: Instant }
#[derive(Default)]
pub(crate) struct State { owners: BTreeMap<String, Owner> }
impl State {
    pub fn warm(&mut self, project: &Project, files: &[String]) {
        let Ok(root) = project.root() else { return; };
        self.owners.remove(&root);
        if files.len() > 128 { return; }
        let at = Instant::now(); let mut definitions: BTreeMap<String, Vec<Location>> = BTreeMap::new(); let mut bytes = 0;
        let mut accepted = BTreeSet::new();
        for path in files {
            if !path.ends_with(".sql") || !path.starts_with(&format!("{root}/")) { continue; }
            if at.elapsed() > Duration::from_millis(200) || host::project_of(path).and_then(|p| p.root().ok()).as_deref() != Some(&root) { return; }
            let Ok(text) = project.read_file(path) else { return; }; bytes += text.len();
            if bytes > 2 << 20 { return; }
            let Some(tables) = tables(&text) else { continue; };
            accepted.insert(path.clone());
            for table in tables.into_iter().filter(|table| table.definition) {
                definitions.entry(table.name).or_default().push(Location { path: path.clone(), range: table.range });
            }
        }
        // A source association is admitted only for a literal (or same-file constant) JPA @Entity/@Table model, or a
        // plainly named @Entity under unmodified Spring Boot naming; no classpath inference.
        if let Ok(java) = project.find_files("**/*.java", 257) {
            if java.len() <= 256 {
                let implicit_default = implicit_naming_default(project, &root);
                let mut custom_naming = false;
                let mut implicit: Vec<(String, String, Range)> = Vec::new();
                for path in java {
                    if at.elapsed() > Duration::from_millis(400) { break; }
                    if !path.starts_with(&format!("{root}/")) || host::project_of(&path).and_then(|p| p.root().ok()).as_deref() != Some(&root) { continue; }
                    let Ok(text) = project.read_file(&path) else { continue; }; bytes += text.len();
                    if bytes > 4 << 20 { break; }
                    custom_naming |= text.contains("NamingStrategy");
                    for (name, range) in entity_tables(&text) { definitions.entry(name).or_default().push(Location { path: path.clone(), range }); }
                    if implicit_default { implicit.extend(implicit_entity_tables(&text).into_iter().map(|(name, range)| (name, path.clone(), range))); }
                }
                if implicit_default && !custom_naming && at.elapsed() <= Duration::from_millis(400) {
                    for (name, path, range) in implicit { definitions.entry(name).or_default().push(Location { path, range }); }
                }
            }
        }
        if self.owners.len() >= 8 { self.owners.clear(); }
        self.owners.insert(root, Owner { files: accepted, definitions, at });
    }
    pub fn invalidate(&mut self, path: &str) { self.owners.retain(|root, _| !path.starts_with(&format!("{root}/"))); }
    pub fn definition(&self, doc: &Document, at: Position) -> Vec<Location> {
        if doc.language != "sql" { return vec![]; }
        let Some(root) = host::project_of(&doc.path).and_then(|p| p.root().ok()) else { return vec![]; };
        let Some(owner) = self.owners.get(&root).filter(|owner| owner.at.elapsed() < Duration::from_secs(120) && owner.files.contains(&doc.path)) else { return vec![]; };
        let Some(tables) = tables(&doc.text) else { return vec![]; };
        let Some(table) = tables.iter().find(|table| !table.definition && table.range.start <= at && at < table.range.end) else { return vec![]; };
        owner.definitions.get(&table.name).filter(|locations| locations.len() <= 32).map(|locations| locations.iter()
            .filter(|location| !location.path.ends_with(".java") || module(&doc.path).is_none() || module(&doc.path) == module(&location.path))
            .cloned().collect()).unwrap_or_default()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_tables_ignore_comments_literals_and_preserve_schema_and_utf16() {
        let text = "-- CREATE TABLE ghost(x int);\nSELECT 'CREATE TABLE fake(x int)'; CREATE TABLE IF NOT EXISTS public.users(id int); UPDATE public.users SET id=1;";
        let found = tables(text).unwrap();
        assert_eq!(found.len(), 2);
        assert!(found[0].definition && !found[1].definition);
        assert_eq!(found[0].name, "public.users");
        assert_eq!(found[1].name, "public.users");
        assert!(tables("select $$ CREATE TABLE hidden(x int) $$;").is_none());
        assert!(tables("CREATE TABLE \"Users\"(id int)").is_none());
        assert!(tables("/* unfinished").is_none());
    }
    #[test]
    fn provider_navigates_owned_migrations_but_never_publishes_rename_or_guesses_schema() {
        let host = openidea_plugin::mock::MockHost::install();
        let create = "/project/sql/V1__init.sql";
        let change = "/project/sql/V2__update.sql";
        host.add_file("sql/V1__init.sql", "CREATE TABLE public.users(id int);");
        host.add_file("sql/V2__update.sql", "UPDATE public.users SET id=2;");
        let project = host.invocation().project().unwrap();
        let mut state = State::default();
        state.warm(&project, &[create.into(), change.into()]);
        let doc = Document { path: change.into(), language: "sql".into(), version: 1, text: "UPDATE public.users SET id=2;".into() };
        let locations = state.definition(&doc, Position::new(0, 10));
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].path, create);
        assert_eq!(locations[0].range, Range::new(Position::new(0, 13), Position::new(0, 25)));
        let unqualified = Document { text: "UPDATE users SET id=2;".into(), ..doc.clone() };
        assert!(state.definition(&unqualified, Position::new(0, 8)).is_empty());
        let declaration = Document { path: create.into(), text: "CREATE TABLE public.users(id int);".into(), ..doc.clone() };
        assert!(state.definition(&declaration, Position::new(0, 16)).is_empty());
        let foreign = Document { path: "/other/sql/V2__update.sql".into(), ..doc.clone() };
        assert!(state.definition(&foreign, Position::new(0, 10)).is_empty());
        state.invalidate(create);
        assert!(state.definition(&doc, Position::new(0, 10)).is_empty());
    }

    #[test]
    fn entity_association_requires_qualified_literal_jpa_mapping() {
        let source = "import jakarta.persistence.Entity; import jakarta.persistence.Table; @Entity @Table(name=\"users\", schema=\"public\") class User {}";
        let found = entity_tables(source);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "public.users");
        assert!(entity_tables(&source.replace("jakarta.persistence.Table", "com.example.Table")).is_empty());
        assert!(entity_tables(&source.replace("name=\"users\"", "name=TABLE_NAME")).is_empty());
        assert!(entity_tables(&source.replace("@Table(name=\"users\", schema=\"public\")", "")).is_empty());
    }

    #[test]
    fn same_file_constants_resolve_table_and_schema_but_foreign_ones_stay_quiet() {
        let head = "import jakarta.persistence.*; ";
        let ok = format!("{head}@Entity @Table(name = User.TABLE, schema = SCHEMA) class User {{ static final String TABLE = \"users\"; public static final String SCHEMA = \"public\"; }}");
        assert_eq!(entity_tables(&ok)[0].0, "public.users");
        assert!(entity_tables(&format!("{head}@Entity @Table(name = Other.TABLE) class User {{}}")).is_empty());
        assert!(entity_tables(&format!("{head}@Entity @Table(name = T) class User {{ static final String T = prefix(); }}")).is_empty());
        assert!(entity_tables(&format!("{head}@Entity @Table(name = T) class User {{ String T = \"users\"; }}")).is_empty());
    }

    #[test]
    fn implicit_snake_case_names_are_plain_camel_case_only() {
        assert_eq!(snake_case("OrderLine").as_deref(), Some("order_line"));
        assert_eq!(snake_case("User").as_deref(), Some("user"));
        for ambiguous in ["HTMLPage", "Order2", "UserID", "user", "Ünder"] { assert!(snake_case(ambiguous).is_none(), "{ambiguous}"); }
        let head = "import jakarta.persistence.*; ";
        assert_eq!(implicit_entity_tables(&format!("{head}@Entity class OrderLine {{}}"))[0].0, "order_line");
        for skipped in ["@Entity(name = \"X\") class OrderLine {}", "@Entity @Table(name = \"a\") class OrderLine {}", "@Entity class OrderLine extends Base {}", "@Entity abstract class OrderLine {}", "@Entity @Inheritance class OrderLine {}", "class OrderLine {}"] {
            assert!(implicit_entity_tables(&format!("{head}{skipped}")).is_empty(), "{skipped}");
        }
        assert!(implicit_entity_tables("@Entity class OrderLine {}").is_empty());
    }

    #[test]
    fn implicit_names_require_spring_boot_and_no_naming_override() {
        let entity = "import jakarta.persistence.*; @Entity class OrderLine {}";
        let run = |setup: &dyn Fn(&openidea_plugin::mock::MockHost)| {
            let host = openidea_plugin::mock::MockHost::install();
            host.add_file("src/main/resources/db/migration/V1__a.sql", "CREATE TABLE order_line(id int);");
            host.add_file("src/main/resources/db/migration/V2__b.sql", "UPDATE order_line SET id=1;");
            host.add_file("src/main/java/OrderLine.java", entity);
            setup(&host);
            let mut state = State::default();
            let sql = ["/project/src/main/resources/db/migration/V1__a.sql".to_string(), "/project/src/main/resources/db/migration/V2__b.sql".to_string()];
            state.warm(&host.invocation().project().unwrap(), &sql);
            let doc = Document { path: sql[1].clone(), language: "sql".into(), version: 1, text: "UPDATE order_line SET id=1;".into() };
            state.definition(&doc, Position::new(0, 10)).len()
        };
        assert_eq!(run(&|h| h.add_file("pom.xml", "<artifactId>spring-boot-starter-data-jpa</artifactId>")), 2);
        assert_eq!(run(&|_| {}), 1);
        assert_eq!(run(&|h| { h.add_file("pom.xml", "spring-boot"); h.add_file("src/main/resources/application.properties", "spring.jpa.hibernate.naming.physical-strategy=x"); }), 1);
        assert_eq!(run(&|h| { h.add_file("pom.xml", "spring-boot"); h.add_file("src/main/java/Cfg.java", "class Cfg implements PhysicalNamingStrategy {}"); }), 1);
    }

    #[test]
    fn literal_entity_targets_stay_in_the_sql_source_module() {
        let host = openidea_plugin::mock::MockHost::install();
        host.add_file("a/src/main/resources/db/migration/V1__init.sql", "CREATE TABLE public.users(id int);");
        host.add_file("a/src/main/resources/db/migration/V2__update.sql", "UPDATE public.users SET id=2;");
        let entity = "import jakarta.persistence.*; @Entity @Table(name=\"users\", schema=\"public\") class User {}";
        host.add_file("a/src/main/java/User.java", entity);
        host.add_file("b/src/main/java/User.java", entity);
        let mut state = State::default();
        state.warm(&host.invocation().project().unwrap(), &["/project/a/src/main/resources/db/migration/V1__init.sql".into(), "/project/a/src/main/resources/db/migration/V2__update.sql".into()]);
        let doc = Document { path: "/project/a/src/main/resources/db/migration/V2__update.sql".into(), language: "sql".into(), version: 1, text: "UPDATE public.users SET id=2;".into() };
        let locations = state.definition(&doc, Position::new(0, 10));
        assert_eq!(locations.len(), 2);
        assert!(locations.iter().any(|location| location.path == "/project/a/src/main/java/User.java"));
        assert!(!locations.iter().any(|location| location.path.contains("/b/")));
        assert!(entity_tables(&entity.replace("import jakarta.persistence.*;", "import jakarta.persistence.*; import custom.*;")).is_empty());
    }

}
