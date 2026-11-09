//! A source-only literal Gradle adapter, not a Groovy/Kotlin/effective-model evaluator.
use super::*;
#[derive(Clone)]
struct Token {
    value: String,
    start: usize,
    end: usize,
    quote: Option<u8>,
}
fn lex(text: &str) -> Option<Vec<Token>> {
    if text.len() > MAX_SOURCE
        || text.contains('\r') && !text.replace("\r\n", "").chars().all(|c| c != '\r')
    {
        return None;
    }
    let b = text.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b[i..].starts_with(b"//") {
            i += 2;
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        let start = i;
        let (value, quote) = if matches!(b[i], b'\'' | b'"') {
            let q = b[i];
            i += 1;
            let a = i;
            while i < b.len() && b[i] != q {
                if matches!(b[i], b'\\' | b'$' | b'\r' | b'\n') || b[i] < 32 {
                    return None;
                }
                i += 1;
            }
            if i == b.len() {
                return None;
            }
            let value = text.get(a..i)?.to_owned();
            i += 1;
            (value, Some(q))
        } else if b[i].is_ascii_alphanumeric() || b[i] == b'_' {
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            (text[start..i].into(), None)
        } else if b"{}().=<>".contains(&b[i]) {
            i += 1;
            (text[start..i].into(), None)
        } else {
            return None;
        };
        out.push(Token {
            value,
            start,
            end: i,
            quote,
        });
        if out.len() > 2048 {
            return None;
        }
    }
    Some(out)
}
struct Parser<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    at: usize,
    kotlin: bool,
}
impl Parser<'_> {
    fn peek(&self, v: &str) -> bool {
        self.tokens
            .get(self.at)
            .is_some_and(|t| t.quote.is_none() && t.value == v)
    }
    fn take(&mut self, v: &str) -> Option<()> {
        if !self.peek(v) {
            return None;
        }
        self.at += 1;
        Some(())
    }
    fn string(&mut self) -> Option<String> {
        let t = self.tokens.get(self.at)?;
        if t.quote.is_none() || self.kotlin && t.quote != Some(b'"') {
            return None;
        }
        let v = t.value.clone();
        self.at += 1;
        Some(v)
    }
    fn call_string(&mut self) -> Option<String> {
        if self.peek("(") {
            self.take("(")?;
            let v = self.string()?;
            self.take(")")?;
            Some(v)
        } else if !self.kotlin {
            self.string()
        } else {
            None
        }
    }
    fn one_line(&self, from: usize) -> Option<()> {
        let a = self.tokens.get(from)?.start;
        let z = self.tokens.get(self.at.checked_sub(1)?)?.end;
        (!self.text[a..z].contains('\n')).then_some(())
    }
    fn exact(&mut self, values: &[&str]) -> Option<()> {
        for v in values {
            self.take(v)?;
        }
        Some(())
    }
}
pub(super) fn proof(doc: &Document) -> Option<Proof> {
    let kotlin = match (doc.path.rsplit('/').next()?, doc.language.as_str()) {
        ("build.gradle", "groovy") => false,
        ("build.gradle.kts", "kotlin") => true,
        _ => return None,
    };
    let mut p = Parser {
        text: &doc.text,
        tokens: lex(&doc.text)?,
        at: 0,
        kotlin,
    };
    p.exact(&["plugins", "{"])?;
    let mut plugins = BTreeSet::new();
    let mut boot = None;
    while !p.peek("}") {
        let from = p.at;
        let id = if kotlin && p.peek("java") {
            p.take("java")?;
            "java".into()
        } else {
            p.take("id")?;
            p.call_string()?
        };
        if !plugins.insert(id.clone()) {
            return None;
        }
        match id.as_str() {
            "java" => {}
            "org.springframework.boot" => {
                p.take("version")?;
                let v = p.string()?;
                if !ga_version(&v) {
                    return None;
                }
                boot = Some(v);
            }
            "io.spring.dependency-management" => {
                p.take("version")?;
                if p.string()? != "1.1.7" {
                    return None;
                }
            }
            _ => return None,
        };
        p.one_line(from)?;
    }
    p.take("}")?;
    if plugins.len() != 3
        || !plugins.contains("java")
        || !plugins.contains("io.spring.dependency-management")
    {
        return None;
    }
    let boot = boot?;
    let mut seen = BTreeSet::new();
    let mut existing = BTreeSet::new();
    let mut insert = None;
    let mut inner = None;
    while p.at < p.tokens.len() {
        let key = p.tokens.get(p.at)?.value.clone();
        if p.tokens[p.at].quote.is_some() || !seen.insert(key.clone()) {
            return None;
        }
        let from = p.at;
        p.at += 1;
        match key.as_str() {
            "group" | "version" | "description" => {
                p.take("=")?;
                let v = p.string()?;
                if key != "description" && !literal(&v) {
                    return None;
                }
                p.one_line(from)?;
            }
            "repositories" => {
                p.exact(&["{", "mavenCentral", "(", ")", "}"])?;
            }
            "java" => {
                p.exact(&[
                    "{",
                    "toolchain",
                    "{",
                    "languageVersion",
                    "=",
                    "JavaLanguageVersion",
                    ".",
                    "of",
                    "(",
                ])?;
                let token = p.tokens.get(p.at)?;
                if token.quote.is_some() {
                    return None;
                }
                let number = &token.value;
                if number.len() > 2
                    || !number.bytes().all(|b| b.is_ascii_digit())
                    || number.parse::<u32>().ok()? < 8
                {
                    return None;
                }
                p.at += 1;
                p.exact(&[")", "}", "}"])?;
            }
            "dependencies" => {
                p.take("{")?;
                while !p.peek("}") {
                    let from = p.at;
                    let config = p.tokens.get(p.at)?;
                    if config.quote.is_some()
                        || !matches!(
                            config.value.as_str(),
                            "implementation"
                                | "runtimeOnly"
                                | "compileOnly"
                                | "testImplementation"
                                | "testRuntimeOnly"
                                | "annotationProcessor"
                        )
                    {
                        return None;
                    }
                    p.at += 1;
                    let coord = p.call_string()?;
                    p.one_line(from)?;
                    let parts = coord.split(':').collect::<Vec<_>>();
                    if !(parts.len() == 2 || parts.len() == 3)
                        || parts.iter().any(|v| !literal(v))
                        || !existing.insert((parts[0].into(), parts[1].into()))
                    {
                        return None;
                    }
                    let at = p.tokens[from].start;
                    let line = doc.text[..at].rfind('\n').map_or(0, |i| i + 1);
                    let indent = &doc.text[line..at];
                    if indent.is_empty() || !indent.chars().all(|c| matches!(c, ' ' | '\t')) {
                        return None;
                    }
                    if inner.is_none() {
                        inner = Some(indent.to_owned());
                    }
                }
                let close = p.tokens.get(p.at)?.start;
                let line = doc.text[..close].rfind('\n').map_or(0, |i| i + 1);
                let indent = &doc.text[line..close];
                if !indent.chars().all(|c| matches!(c, ' ' | '\t')) {
                    return None;
                }
                insert = Some(line);
                if inner.is_none() {
                    inner = Some(format!("{indent}    "));
                }
                p.take("}")?;
            }
            "tasks" => {
                p.take(".")?;
                if kotlin {
                    p.exact(&["withType", "<", "Test", ">"])?;
                } else {
                    p.exact(&["named", "("])?;
                    if p.string()? != "test" {
                        return None;
                    }
                    p.take(")")?;
                }
                p.exact(&["{", "useJUnitPlatform", "(", ")", "}"])?;
            }
            _ => return None,
        }
    }
    let at = insert?;
    Some(Proof {
        boot,
        management: "literal Boot plugin plus dependency-management 1.1.7; source-declared management only",
        at,
        end: at,
        before: String::new(),
        after: String::new(),
        inner: inner?,
        existing,
        newline: if doc.text.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        },
        gradle: Some(kotlin),
    })
}
pub(super) fn fresh(doc: &Document, root: &str) -> Result<(), Error> {
    let project = host::project_of(&doc.path).ok_or(Error::Stale)?;
    let kotlin = doc.path.ends_with(".kts");
    let name = if kotlin {
        "build.gradle.kts"
    } else {
        "build.gradle"
    };
    if doc.path != format!("{root}/{name}") {
        return Err("Gradle adapter supports only the exact project-root build file.".into());
    }
    for name in [
        if kotlin {
            "build.gradle"
        } else {
            "build.gradle.kts"
        },
        "pom.xml",
        "gradle.properties",
        "buildSrc",
        "build-logic",
        "gradle/libs.versions.toml",
        "init.gradle",
        "init.gradle.kts",
        ".mvn/maven.config",
        ".mvn/extensions.xml",
    ] {
        if project.exists(&format!("{root}/{name}"))? {
            return Err("Competing build, version catalog, properties, init/build logic or Maven configuration is unsupported.".into());
        }
    }
    for (name, expected) in [
        ("settings.gradle", !kotlin),
        ("settings.gradle.kts", kotlin),
    ] {
        let path = format!("{root}/{name}");
        if !project.exists(&path)? {
            continue;
        }
        if !expected {
            return Err("Competing Gradle settings dialect is unsupported.".into());
        }
        let text = project.read_file(&path)?;
        if text.len() > 2048 {
            return Err(Error::Stale);
        }
        let mut p = Parser {
            text: &text,
            tokens: lex(&text).ok_or(Error::Stale)?,
            at: 0,
            kotlin,
        };
        p.exact(&["rootProject", ".", "name", "="])
            .ok_or(Error::Stale)?;
        let value = p.string().ok_or(Error::Stale)?;
        p.one_line(0).ok_or(Error::Stale)?;
        if !literal(&value) || p.at != p.tokens.len() {
            return Err("Only a literal rootProject.name settings file is supported; plugin/repository/module rules are not inferred.".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::add_starters::tests::{COORDS, META};
    use openidea_plugin::{file_actions::FileActions, mock::MockHost, text::offset};
    const GROOVY: &str = "plugins {\n    id 'java'\n    id 'org.springframework.boot' version '3.5.16'\n    id 'io.spring.dependency-management' version '1.1.7'\n}\n// 😀 preserve exactly\ngroup = 'com.example'\nversion = '1.0.0'\njava {\n    toolchain {\n        languageVersion = JavaLanguageVersion.of(17)\n    }\n}\nrepositories { mavenCentral() }\ndependencies {\n    implementation 'org.springframework.boot:spring-boot-starter'\n}\ntasks.named('test') {\n    useJUnitPlatform()\n}\n";
    const KOTLIN: &str = "plugins {\n    java\n    id(\"org.springframework.boot\") version \"3.5.16\"\n    id(\"io.spring.dependency-management\") version \"1.1.7\"\n}\n// 😀 preserve exactly\ngroup = \"com.example\"\nversion = \"1.0.0\"\nrepositories { mavenCentral() }\ndependencies {\n    implementation(\"org.springframework.boot:spring-boot-starter\")\n}\ntasks.withType<Test> {\n    useJUnitPlatform()\n}\n";
    fn setup(text: &str, kotlin: bool) -> (MockHost, Launcher, Document) {
        let h = MockHost::install().with_capabilities(&[
            "storage",
            "project.read",
            "editor.read",
            "net:start.spring.io",
        ]);
        let name = if kotlin {
            "build.gradle.kts"
        } else {
            "build.gradle"
        };
        let path = format!("/project/{name}");
        let language = if kotlin { "kotlin" } else { "groovy" };
        h.add_file(name, text);
        h.open_editor(&path, language, text, &[]);
        h.on_fetch(|req| {
            Ok(openidea_plugin::HttpResponse {
                status: 200,
                body: if req.url.contains("metadata/client") {
                    META.as_bytes().to_vec()
                } else {
                    COORDS.as_bytes().to_vec()
                },
                ..Default::default()
            })
        });
        let version = h.invocation().editor().unwrap().document().unwrap().version;
        (
            h,
            Launcher::default(),
            Document {
                path,
                language: language.into(),
                text: text.into(),
                version,
            },
        )
    }
    fn ready(text: &str, kotlin: bool) -> (MockHost, Launcher, Document) {
        let (h, mut l, d) = setup(text, kotlin);
        l.run_command("add-starters", &h.invocation(), None)
            .unwrap();
        (h, l, d)
    }
    fn resolve(h: &MockHost, l: &mut Launcher, d: &Document, ids: &[&str]) -> FileAction {
        let a = l.file_actions(d, Range::default()).remove(0);
        h.answer_next_prompt(Some(ids));
        h.answer_next_prompt(Some(&["yes"]));
        l.resolve_file_action(d, a).unwrap()
    }
    fn after(d: &Document, a: &FileAction) -> String {
        assert!(a.operations.is_empty());
        assert_eq!(a.edits.len(), 1);
        assert_eq!(a.edits[0].0, d.path);
        assert_eq!(a.edits[0].1.len(), 1);
        let e = &a.edits[0].1[0];
        let from = offset(&d.text, e.range.start).unwrap();
        let to = offset(&d.text, e.range.end).unwrap();
        assert_eq!(from, to);
        let mut out = d.text.clone();
        out.replace_range(from..to, &e.text);
        out
    }
    #[test]
    fn groovy_and_kotlin_exact_preview_edits_preserve_utf16_crlf_and_scope() {
        for kotlin in [false, true] {
            let original = if kotlin { KOTLIN } else { GROOVY }.replace('\n', "\r\n");
            let (h, mut l, d) = ready(&original, kotlin);
            let fetches = h.fetches();
            let a = resolve(&h, &mut l, &d, &["web", "data-jpa"]);
            let insertion = if kotlin {
                "    implementation(\"org.springframework.boot:spring-boot-starter-web\")\r\n    runtimeOnly(\"org.springframework.boot:spring-boot-starter-data-jpa\")\r\n"
            } else {
                "    implementation 'org.springframework.boot:spring-boot-starter-web'\r\n    runtimeOnly 'org.springframework.boot:spring-boot-starter-data-jpa'\r\n"
            };
            let at = d.text.find("}\r\ntasks.").unwrap();
            let mut expected = d.text.clone();
            expected.insert_str(at, insertion);
            assert_eq!(after(&d, &a), expected);
            assert_eq!(h.editor_text(), original);
            assert_eq!(h.fetches(), fetches);
            assert!(h.process_runs().is_empty());
            assert!(h.opened_projects().is_empty());
        }
    }
    #[test]
    fn advertised_test_scope_maps_to_test_implementation_without_versions() {
        for kotlin in [false, true] {
            let text = if kotlin { KOTLIN } else { GROOVY };
            let (h, mut l, d) = setup(text, kotlin);
            h.on_fetch(|req| {
                Ok(openidea_plugin::HttpResponse {
                    status: 200,
                    body: if req.url.contains("metadata/client") {
                        META.as_bytes().to_vec()
                    } else {
                        COORDS
                            .replace("\"scope\":\"runtime\"", "\"scope\":\"test\"")
                            .into_bytes()
                    },
                    ..Default::default()
                })
            });
            l.run_command("add-starters", &h.invocation(), None)
                .unwrap();
            let a = resolve(&h, &mut l, &d, &["data-jpa"]);
            let new = after(&d, &a);
            assert!(new.contains(if kotlin {
                "testImplementation(\"org.springframework.boot:spring-boot-starter-data-jpa\")"
            } else {
                "testImplementation 'org.springframework.boot:spring-boot-starter-data-jpa'"
            }));
        }
    }
    #[test]
    fn cancelled_picker_and_full_preview_leave_gradle_bytes_unchanged() {
        for kotlin in [false, true] {
            for preview in [false, true] {
                let text = if kotlin { KOTLIN } else { GROOVY };
                let (h, mut l, d) = ready(text, kotlin);
                let a = l.file_actions(&d, Range::default()).remove(0);
                if preview {
                    h.answer_next_prompt(Some(&["web"]));
                }
                h.answer_next_prompt(None);
                assert!(l.resolve_file_action(&d, a).is_err());
                assert_eq!(h.editor_text(), text);
            }
        }
    }
    #[test]
    fn stale_source_language_owner_metadata_selector_and_settings_refuse() {
        for mode in 0..7 {
            let (h, mut l, mut d) = ready(GROOVY, false);
            let a = l.file_actions(&d, Range::default()).remove(0);
            match mode {
                0 => d.version += 1,
                1 => d.language = "plaintext".into(),
                2 => h.open_editor("/project/build.gradle", "groovy", "// changed", &[]),
                3 => l.owner = "project:/foreign".into(),
                4 => l.metadata = None,
                5 => {
                    host::storage::set(&session::locator(&l.owner, "selector"), "changed").unwrap();
                }
                _ => h.add_file("settings.gradle", "include 'module'"),
            };
            assert!(l.resolve_file_action(&d, a).is_err(), "{mode}");
            assert_eq!(
                h.editor_text(),
                if mode == 2 { "// changed" } else { GROOVY }
            );
        }
    }
    #[test]
    fn unsupported_dynamic_bom_repository_and_competing_sources_refuse_before_fetch() {
        let bad=[GROOVY.replace("version '3.5.16'","version bootVersion"),GROOVY.replace("mavenCentral()","mavenLocal()"),GROOVY.replace("implementation 'org.springframework.boot:spring-boot-starter'","implementation platform('org.springframework.boot:spring-boot-dependencies:3.5.16')"),format!("{GROOVY}apply from: 'other.gradle'\n"),GROOVY.replace("version '1.1.7'","version '1.0.0'"),GROOVY.replace("spring-boot-starter'","spring-boot-starter:$version'"),format!("{GROOVY}dependencyManagement {{ imports {{ mavenBom 'other:bom:1' }} }}\n")];
        for text in bad {
            let (h, mut l, d) = setup(&text, false);
            assert!(
                l.run_command("add-starters", &h.invocation(), None)
                    .is_err()
            );
            assert_eq!(h.fetches(), 0);
            assert!(l.file_actions(&d, Range::default()).is_empty());
        }
        for name in [
            "pom.xml",
            "build.gradle.kts",
            "gradle.properties",
            "buildSrc",
            "gradle/libs.versions.toml",
        ] {
            let (h, mut l, _) = setup(GROOVY, false);
            h.add_file(name, "fixture");
            assert!(
                l.run_command("add-starters", &h.invocation(), None)
                    .is_err(),
                "{name}"
            );
            assert_eq!(h.fetches(), 0);
        }
    }
    #[test]
    fn literal_settings_allowed_but_modules_custom_management_and_nested_source_refuse() {
        let (h, mut l, _) = setup(KOTLIN, true);
        h.add_file("settings.gradle.kts", "rootProject.name = \"demo\"\n");
        assert!(l.run_command("add-starters", &h.invocation(), None).is_ok());
        let (h, mut l, _) = setup(GROOVY, false);
        h.add_file(
            "settings.gradle",
            "pluginManagement { repositories { mavenLocal() } }",
        );
        assert!(
            l.run_command("add-starters", &h.invocation(), None)
                .is_err()
        );
        assert_eq!(h.fetches(), 0);
        let (h, mut l, _) = setup(GROOVY, false);
        h.add_project("/project");
        h.set_project_root(Some("/other"));
        assert!(
            l.run_command("add-starters", &h.invocation(), None)
                .is_err()
        );
        assert_eq!(h.fetches(), 0);
        let (h, mut l, _) = setup(GROOVY, false);
        h.add_file("sub/build.gradle", GROOVY);
        h.open_editor("/project/sub/build.gradle", "groovy", GROOVY, &[]);
        assert!(
            l.run_command("add-starters", &h.invocation(), None)
                .is_err()
        );
        assert_eq!(h.fetches(), 0);
    }
    #[test]
    fn exact_coordinate_collisions_are_skipped_without_conflating_other_groups() {
        let existing = GROOVY.replace("spring-boot-starter'", "spring-boot-starter-web'");
        let (h, mut l, d) = ready(&existing, false);
        let a = l.file_actions(&d, Range::default()).remove(0);
        h.answer_next_prompt(Some(&["web"]));
        assert!(l.resolve_file_action(&d, a).is_err());
        assert_eq!(h.editor_text(), existing);
        let other = GROOVY.replace(
            "org.springframework.boot:spring-boot-starter'",
            "com.example:spring-boot-starter-web'",
        );
        let (h, mut l, d) = ready(&other, false);
        let a = resolve(&h, &mut l, &d, &["web"]);
        assert!(after(&d, &a).contains("org.springframework.boot:spring-boot-starter-web"));
    }
    #[test]
    fn metadata_bom_repository_version_and_provided_mapping_are_not_inferred() {
        for field in ["bom", "repository", "version", "scope"] {
            let (h, mut l, d) = setup(GROOVY, false);
            let mut coords: serde_json::Value = serde_json::from_str(COORDS).unwrap();
            for dep in ["web", "data-jpa", "unknown-range"] {
                coords["dependencies"][dep][field] = if field == "scope" {
                    "provided".into()
                } else {
                    "unsupported".into()
                };
            }
            let body = coords.to_string();
            h.on_fetch(move |req| {
                Ok(openidea_plugin::HttpResponse {
                    status: 200,
                    body: if req.url.contains("metadata/client") {
                        META.as_bytes().to_vec()
                    } else {
                        body.as_bytes().to_vec()
                    },
                    ..Default::default()
                })
            });
            let prepared = l.run_command("add-starters", &h.invocation(), None);
            assert!(prepared.is_err());
            assert_eq!(h.editor_text(), GROOVY);
        }
    }
    #[test]
    fn cold_provider_does_no_fetch_and_resolved_action_cannot_replay() {
        let (h, mut l, d) = setup(KOTLIN, true);
        assert!(l.file_actions(&d, Range::default()).is_empty());
        assert_eq!(h.fetches(), 0);
        l.run_command("add-starters", &h.invocation(), None)
            .unwrap();
        let a = l.file_actions(&d, Range::default()).remove(0);
        let replay = a.clone();
        h.answer_next_prompt(Some(&["web"]));
        h.answer_next_prompt(Some(&["yes"]));
        l.resolve_file_action(&d, a).unwrap();
        assert!(l.resolve_file_action(&d, replay).is_err());
    }
}
