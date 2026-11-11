//! Strict cached direct-property subset; no discovery or simple-name fallback.
use crate::{
    JpaAssist,
    model::{self, Class, Kind},
    queries::{self, ItemKind, Prop},
};
use openidea_plugin::{
    Position, Range, host,
    providers::{CompletionItem, CompletionKind, Document},
    text::{offset, position},
};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
const MAX_TEXT: usize = 128 << 10;
const TTL: Duration = Duration::from_secs(120);
#[derive(Default)]
pub(crate) struct State {
    roots: HashMap<String, HashMap<String, (Instant, Class)>>,
}
impl State {
    pub fn invalidate(&mut self, path: &str) {
        for files in self.roots.values_mut() {
            files.remove(path);
        }
    }
    pub fn observe(&mut self, root: &str, path: &str, text: &str) {
        self.invalidate(path);
        if text.len() > MAX_TEXT || !path.starts_with(&format!("{}/", root.trim_end_matches('/'))) {
            return;
        }
        let language = if path.ends_with(".java") {
            "java"
        } else if path.ends_with(".kt") {
            "kotlin"
        } else {
            return;
        };
        let doc = Document { path: path.into(), language: language.into(), version: 0, text: text.into() };
        let Some(class) = crate::attribute_info::snapshot(&doc) else {
            return;
        };
        if class.kind != Kind::Entity || class.superclass.is_some() {
            return;
        }
        if !self.roots.contains_key(root) && self.roots.len() >= 8 {
            self.roots.clear();
        }
        let files = self.roots.entry(root.into()).or_default();
        files.retain(|_, (at, _)| at.elapsed() < TTL);
        if files.len() >= 256 || files.values().map(|(_, c)| c.attributes.len()).sum::<usize>() + class.attributes.len() > 4096 {
            files.clear();
            return;
        }
        files.insert(path.into(), (Instant::now(), class));
    }
    pub(crate) fn entities(&self, root: &str) -> Vec<&Class> {
        let Some(files) = self.roots.get(root) else { return vec![] };
        let mut all: Vec<&Class> = files.values().filter(|(at, _)| at.elapsed() < TTL).map(|(_, c)| c).collect();
        all.sort_by(|a, b| a.name.cmp(&b.name));
        let unique: Vec<&Class> = all.iter().copied().filter(|c| all.iter().filter(|o| o.name == c.name).count() == 1).collect();
        unique
    }
    pub(crate) fn entity_by_name(&self, root: &str, name: &str) -> Option<&Class> {
        self.entities(root).into_iter().find(|c| c.name == name)
    }
    fn domain(&self, root: &str, fqcn: &str) -> Option<&Class> {
        let mut found = self
            .roots
            .get(root)?
            .iter()
            .filter(|(_, (at, c))| at.elapsed() < TTL && format!("{}.{}", c.package.as_deref().unwrap_or_default(), c.name) == fqcn);
        let (path, (_, class)) = found.next()?;
        if found.next().is_some()
            || host::project_of(path).and_then(|project| project.root().ok()).as_deref() != Some(root)
        { None } else { Some(class) }
    }
}
fn identifier(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && c.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}
fn statement(masked: &str, word: &str) -> Vec<String> {
    model::words(masked, word)
        .map(|at| masked[at + word.len()..].trim_start().split([';', '\r', '\n']).next().unwrap_or("").trim().to_owned())
        .collect()
}
fn qualified(s: &str) -> bool {
    !s.is_empty() && s.split('.').all(identifier)
}
fn owned_repository(text: &str, kotlin: bool) -> Option<(queries::Repository, String, usize)> {
    let masked = model::mask(text);
    if model::words(&masked, "interface").count() != 1
        || ["class", "object", "typealias", "enum"].iter().any(|w| model::words(&masked, w).next().is_some())
    {
        return None;
    }
    let interface = model::words(&masked, "interface").next()?;
    if model::words(&masked, "import").chain(model::words(&masked, "package")).any(|at| at >= interface) {
        return None;
    }
    let repo = queries::parse_repository(text, kotlin)?;
    let open = masked[repo.name_start..].find('{')? + repo.name_start;
    let tail = masked[repo.name_start + repo.name.len()..open].trim();
    let tail = if kotlin {
        tail.strip_prefix(':')?
    } else {
        let after = tail.strip_prefix("extends")?;
        if !after.starts_with(char::is_whitespace) {
            return None;
        }
        after
    }
    .trim_start();
    let end = tail.find('<')?;
    let base = tail[..end].trim();
    let qualified_base = match base {
        "JpaRepository" => "org.springframework.data.jpa.repository.JpaRepository",
        "Repository" => "org.springframework.data.repository.Repository",
        "CrudRepository" => "org.springframework.data.repository.CrudRepository",
        "ListCrudRepository" => "org.springframework.data.repository.ListCrudRepository",
        "PagingAndSortingRepository" => "org.springframework.data.repository.PagingAndSortingRepository",
        _ => return None,
    };
    let imports = statement(&masked, "import");
    if imports.iter().any(|i| i.contains(" as ")) {
        return None;
    }
    let base_imports: Vec<_> = imports.iter().filter(|i| i.rsplit('.').next() == Some(base)).collect();
    if base_imports.len() != 1 || base_imports[0] != qualified_base {
        return None;
    }
    let generic = tail[end + 1..].trim();
    let (args, rest) = generic.split_once('>')?;
    if !rest.trim().is_empty() {
        return None;
    }
    let args: Vec<_> = args.split(',').map(str::trim).collect();
    if args.len() != 2 || !identifier(args[0]) || !identifier(args[1].trim_end_matches('?')) || args[0] != repo.entity {
        return None;
    }
    let package = statement(&masked, "package");
    let [package] = package.as_slice() else {
        return None;
    };
    if !qualified(package) {
        return None;
    }
    let domain_imports: Vec<_> = imports.iter().filter(|i| i.rsplit('.').next() == Some(args[0])).collect();
    let owner = match domain_imports.as_slice() {
        [] => format!("{package}.{}", args[0]),
        [import] if qualified(import) => (*import).clone(),
        _ => return None,
    };
    Some((repo, owner, open))
}
impl JpaAssist {
    pub(crate) fn cached_repository_completion(&mut self, doc: &Document, at: Position) -> Vec<CompletionItem> {
        // Any observed replacement invalidates this path before source preflight.
        self.repository_completion.invalidate(&doc.path);
        if doc.text.len() > MAX_TEXT || !matches!(doc.language.as_str(), "java" | "kotlin") {
            self.repository_completion.invalidate(&doc.path);
            return vec![];
        }
        let Some(kotlin) = model::language_of(&doc.path) else {
            return vec![];
        };
        if kotlin != (doc.language == "kotlin") {
            return vec![];
        }
        let Some(at) = offset(&doc.text, at) else {
            return vec![];
        };
        let Some(project) = host::project_of(&doc.path) else {
            return vec![];
        };
        let Ok(root) = project.root() else {
            return vec![];
        };
        if !doc.path.starts_with(&format!("{}/", root.trim_end_matches('/'))) || doc.path.split('/').any(|p| matches!(p, "." | "..")) {
            return vec![];
        }
        self.repository_completion.observe(&root, &doc.path, &doc.text);
        let Some((repo, owner, open)) = owned_repository(&doc.text, kotlin) else {
            return vec![];
        };
        let start = at - doc.text.as_bytes()[..at].iter().rev().take_while(|&&c| model::is_ident(c)).count();
        if start == at
            || start <= open
            || at - start > 256
            || doc.text[..start].chars().next_back().is_some_and(|c| !c.is_ascii() && c.is_alphanumeric())
        {
            return vec![];
        }
        let end = at + doc.text.as_bytes()[at..].iter().take_while(|&&c| model::is_ident(c)).count();
        if doc.text[end..].chars().next().is_some_and(|c| !c.is_ascii() && c.is_alphanumeric()) {
            return vec![];
        }
        let masked = model::mask(&doc.text);
        if masked.get(start..at) != doc.text.get(start..at) {
            return vec![];
        }
        let after = masked[end..].trim_start();
        if after.starts_with([';', '=', ':', '<']) || after.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$') {
            return vec![];
        }
        let mut braces = 0i32;
        let mut parens = 0i32;
        let body_end = model::closing(masked.as_bytes(), open).unwrap_or(masked.len());
        let Some((statement_start, _)) =
            model::members(&masked, open + 1, body_end, kotlin, false).into_iter().find(|(from, to)| *from <= start && start < *to)
        else {
            return vec![];
        };
        for c in masked[open..start].chars() {
            match c {
                '{' => braces += 1,
                '}' => braces -= 1,
                '(' => parens += 1,
                ')' => parens -= 1,
                _ => {}
            }
        }
        if kotlin && !model::has_word(&masked[statement_start..start], "fun") {
            return vec![];
        }
        if braces != 1
            || parens != 0
            || masked[statement_start..start].contains('=')
            || model::annotations(&masked)
                .iter()
                .any(|a| a.start >= statement_start && a.start < start && matches!(a.name, "Query" | "NativeQuery" | "Procedure"))
        {
            return vec![];
        }
        let Some(class) = self.repository_completion.domain(&root, &owner) else {
            return vec![];
        };
        let mut counts = HashMap::new();
        for a in &class.attributes {
            *counts.entry(model::capitalize(&a.name)).or_insert(0usize) += 1;
        }
        let props: Vec<_> = class
            .attributes
            .iter()
            .filter(|a| counts[&model::capitalize(&a.name)] == 1 && !a.name.contains('_'))
            .map(|a| Prop { name: a.name.clone(), ty: a.ty.clone(), fqcn: None, target: None })
            .collect();
        if props.is_empty() {
            return vec![];
        }
        let resolve = |name: &str| (name == repo.entity).then(|| props.clone());
        queries::complete(&doc.text, at, &repo, &resolve, kotlin)
            .into_iter()
            .filter(|i| {
                i.kind != ItemKind::Method
                    && i.imports.is_none()
                    && !["findById", "findAllById", "existsById", "deleteById", "deleteAllById"].contains(&i.label.as_str())
            })
            .filter(|i| i.label.starts_with(&doc.text[start..at]))
            .take(256)
            .map(|i| {
                let kind = match i.kind {
                    ItemKind::Property => CompletionKind::Property,
                    ItemKind::Operator => CompletionKind::Operator,
                    _ => CompletionKind::Keyword,
                };
                CompletionItem {
                    insert_text: i.insert,
                    detail: i.detail,
                    replace: Some(Range::new(position(&doc.text, start), position(&doc.text, end))),
                    ..CompletionItem::new(&i.label, kind)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{diagnostics::DiagnosticsProvider, mock::MockHost, providers::Providers};
    const ENTITY: &str = "package app;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity class Person {\n@Id Long id;\nString name;\nAddress address;\nString qCode;\nString QCode;\n}";
    const IMPORT: &str = "import org.springframework.data.jpa.repository.JpaRepository;";
    fn document(path: &str, text: &str) -> Document {
        Document {
            path: path.into(),
            language: if path.ends_with(".kt") { "kotlin" } else { "java" }.into(),
            version: 1,
            text: text.into(),
        }
    }
    fn repo(body: &str) -> Document {
        document(
            "/project/PersonRepository.java",
            &format!("package app;\n{IMPORT}\ninterface PersonRepository extends JpaRepository<Person,Long> {{\n{body}\n}}"),
        )
    }
    fn names(p: &mut JpaAssist, d: &Document) -> Vec<String> {
        let byte = d.text.find("findBy").unwrap() + "findBy".len();
        p.completion(d, position(&d.text, byte), None).into_iter().map(|i| i.label).collect()
    }
    fn warm(p: &mut JpaAssist, path: &str, text: &str) {
        p.diagnostics(&document(path, text));
    }
    #[test]
    fn cached_exact_owner_keeps_useful_direct_fields_without_discovery_or_storage() {
        let h = MockHost::install();
        h.add_file("Person.java", ENTITY);
        let mut p = JpaAssist::default();
        let d = repo("findBy");
        assert!(names(&mut p, &d).is_empty());
        warm(&mut p, "/project/Person.java", ENTITY);
        let labels = names(&mut p, &d);
        assert!(labels.contains(&"findByName".into()) && labels.contains(&"findByAddress".into()));
        assert!(!labels.contains(&"findByAddressGuessed".into()));
        assert!(!labels.contains(&"findByQCode".into()));
        assert!(p.classes.is_empty() && p.sources.is_empty() && p.missing.is_empty());
        assert!(names(&mut JpaAssist::default(), &d).is_empty());
    }
    #[test]
    fn explicit_import_same_name_packages_and_duplicate_qualified_owners() {
        let _h = MockHost::install();
        let mut p = JpaAssist::default();
        warm(&mut p, "/project/Person.java", ENTITY);
        warm(
            &mut p,
            "/project/other/Person.java",
            &ENTITY.replace("package app;", "package other;").replace("String name;", "String foreign;"),
        );
        assert!(names(&mut p, &repo("findBy")).contains(&"findByName".into()));
        let foreign = Document { text: repo("findBy").text.replace(IMPORT, &format!("{IMPORT}\nimport other.Person;")), ..repo("findBy") };
        assert!(names(&mut p, &foreign).contains(&"findByForeign".into()));
        assert!(!names(&mut p, &foreign).contains(&"findByName".into()));
        warm(&mut p, "/project/duplicate/Person.java", ENTITY);
        assert!(names(&mut p, &repo("findBy")).is_empty());
    }
    #[test]
    fn nested_owner_reassignment_withholds_cached_repository_properties() {
        let host = MockHost::install();
        let mut plugin = JpaAssist::default();
        warm(&mut plugin, "/project/nested/Person.java", ENTITY);
        assert!(names(&mut plugin, &repo("findBy")).contains(&"findByName".into()));
        host.add_project("/project/nested");
        assert!(names(&mut plugin, &repo("findBy")).is_empty());
        assert!(plugin.classes.is_empty() && plugin.sources.is_empty());
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }
    #[test]
    fn unsaved_current_source_invalidation_and_root_switch_are_authoritative() {
        let h = MockHost::install();
        let mut p = JpaAssist::default();
        warm(&mut p, "/project/Person.java", ENTITY);
        let edited = document("/project/Person.java", &ENTITY.replace("String name;", "String renamed;"));
        p.completion(&edited, Position::new(0, 0), None);
        let labels = names(&mut p, &repo("findBy"));
        assert!(labels.contains(&"findByRenamed".into()) && !labels.contains(&"findByName".into()));
        h.set_project_root(Some("/other"));
        let other = Document { path: "/other/PersonRepository.java".into(), ..repo("findBy") };
        assert!(names(&mut p, &other).is_empty());
        h.set_project_root(Some("/project"));
        let oversized = document("/project/Person.java", &format!("{ENTITY}{}", " ".repeat(MAX_TEXT)));
        p.completion(&oversized, Position::new(0, 0), None);
        assert!(names(&mut p, &repo("findBy")).is_empty());
    }
    #[test]
    fn unowned_generic_wildcard_alias_and_unknown_hierarchy_remain_quiet() {
        let _h = MockHost::install();
        let mut p = JpaAssist::default();
        warm(&mut p, "/project/Person.java", ENTITY);
        for text in [
            repo("findBy").text.replace(IMPORT, ""),
            repo("findBy").text.replace(IMPORT, "import other.JpaRepository;"),
            repo("findBy").text.replace(IMPORT, "import org.springframework.data.jpa.repository.*;"),
            repo("findBy").text.replace("PersonRepository extends", "PersonRepository<T> extends"),
            repo("findBy").text.replace("JpaRepository<Person,Long>", "JpaRepository<unknown.Person,Long>"),
            repo("findBy").text.replace(IMPORT, &format!("{IMPORT}\nimport other.Person;\nimport another.Person;")),
        ] {
            assert!(names(&mut p, &Document { text, ..repo("findBy") }).is_empty());
        }
        warm(&mut p, "/project/Person.java", &ENTITY.replace("class Person", "class Person extends LibraryBase"));
        assert!(names(&mut p, &repo("findBy")).is_empty());
    }
    #[test]
    fn comments_strings_default_bodies_declared_queries_and_initializers_are_quiet() {
        let _h = MockHost::install();
        let mut p = JpaAssist::default();
        warm(&mut p, "/project/Person.java", ENTITY);
        for body in [
            "// findBy",
            "String text = \"findBy\";",
            "default void call(){ findBy(); }",
            "@Query(\"select p from Person p\")\nPerson findBy(String x);",
            "String value = findBy;",
            "String findBy;",
            "findByReturn existing();",
        ] {
            assert!(names(&mut p, &repo(body)).is_empty(), "{body}");
        }
    }
    #[test]
    fn kotlin_direct_fields_and_utf16_replacement_do_not_generate_guessed_signatures() {
        let _h = MockHost::install();
        let mut p = JpaAssist::default();
        warm(
            &mut p,
            "/project/Person.kt",
            "package app\nimport jakarta.persistence.Entity\nimport jakarta.persistence.Id\n@Entity class Person {\n@Id var id: Long = 1\nvar name: String = \"\"\n}",
        );
        let d = document(
            "/project/PersonRepository.kt",
            &format!("package app\n{IMPORT}\ninterface PersonRepository : JpaRepository<Person,Long> {{\n/* 😀 */ fun findBy\n}}"),
        );
        let byte = d.text.find("findBy").unwrap();
        let at = position(&d.text, byte + 6);
        let items = p.completion(&d, at, None);
        let name = items.iter().find(|i| i.label == "findByName").unwrap();
        assert_eq!(name.replace, Some(Range::new(position(&d.text, byte), at)));
        assert!(name.additional_edits.is_empty());
        assert!(items.iter().all(|i| i.kind != Some(CompletionKind::Method)));
    }
    #[test]
    fn reserved_ids_unicode_suffixes_and_output_bounds_do_not_fabricate_completion() {
        let _h = MockHost::install();
        let mut p = JpaAssist::default();
        warm(&mut p, "/project/Person.java", ENTITY);
        assert!(!names(&mut p, &repo("findBy")).contains(&"findById".into()));
        let d = repo("findByÄge");
        let at = position(&d.text, d.text.find("findByÄge").unwrap() + "findByÄge".len());
        assert!(p.completion(&d, at, None).is_empty());
        let many = ENTITY.replace("String name;", &(0..129).map(|i| format!("String field{i};\n")).collect::<String>());
        warm(&mut p, "/project/Person.java", &many);
        assert!(names(&mut p, &repo("findBy")).is_empty());
    }
    #[test]
    fn completion_replaces_whole_existing_word_and_never_appends_its_suffix_twice() {
        let _h = MockHost::install();
        let mut p = JpaAssist::default();
        warm(&mut p, "/project/Person.java", ENTITY);
        let d = repo("Person findByName(String value);");
        let byte = d.text.find("findByName").unwrap();
        let at = position(&d.text, byte + 6);
        let name = p.completion(&d, at, None).into_iter().find(|i| i.label == "findByName").unwrap();
        assert_eq!(name.replace, Some(Range::new(position(&d.text, byte), position(&d.text, byte + 10))));
        let d = repo("// comment ");
        let at = position(&d.text, d.text.find("// comment ").unwrap() + 11);
        assert!(p.completion(&d, at, None).is_empty());
    }
    #[test]
    fn expired_and_denied_facts_do_not_trigger_refresh() {
        let h = MockHost::install();
        let mut p = JpaAssist::default();
        warm(&mut p, "/project/Person.java", ENTITY);
        for (at, _) in p.repository_completion.roots.values_mut().flat_map(|f| f.values_mut()) {
            *at = Instant::now() - TTL - Duration::from_secs(1);
        }
        assert!(names(&mut p, &repo("findBy")).is_empty());
        let _h = h.with_capabilities(&[]);
        assert!(names(&mut p, &repo("findBy")).is_empty());
    }
}
