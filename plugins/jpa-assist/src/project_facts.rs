//! Build-derived project facts (Kotlin/Java, persistence namespace, libraries) per module, cached.
//!
//! Sources: Kotlin docs (gradle-configure-project, maven-configure-project, kotlin-jpa-plugin) for the plugin
//! markers and the `src/main/kotlin` / `src/main/java` roots. Only literal build text is read; version
//! catalogs, BOMs and parents yield `Unknown`, never `No`.
use crate::build::{APPLIED, BUILD_FILES, KOTLIN, NOT_LITERAL, is_build_file};
use crate::file_actions::{build_text, hibernate6, hibernate_core_majors, spring_boot_majors};
use openidea_plugin::{Error, Invocation, Project, host};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(120);
const MAX_CACHED: usize = 64;
const MAX_WALK: usize = 24;
const INDIRECT: &[&str] = &["libs.", "platform(", "versionCatalogs", "<scope>import</scope>"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fact {
    Yes,
    No,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Namespace {
    Jakarta,
    Javax,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectFacts {
    pub module: String,
    pub kotlin: Fact,
    pub java: Fact,
    pub kotlin_jpa: Fact,
    pub namespace: Namespace,
    pub spring_data_jpa: Fact,
    pub hibernate6: Fact,
    pub hibernate_major: Option<u32>,
    pub lombok: Fact,
    pub mapstruct: Fact,
    pub flyway: Fact,
    pub liquibase: Fact,
    pub jackson: Fact,
    pub bean_validation: Fact,
    /// Proven Java language level (release/source/target, toolchain, Spring Boot 3 implies 17); `None` when not provable.
    pub java_level: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    SpringDataJpa,
    Flyway,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Gate {
    Allowed,
    Refused(String),
}

impl Gate {
    pub fn into_result(self) -> Result<(), Error> {
        match self {
            Gate::Allowed => Ok(()),
            Gate::Refused(reason) => Err(Error::from(reason)),
        }
    }
}

fn mentions(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn feature_present(feature: Feature, build: &str) -> bool {
    match feature {
        Feature::SpringDataJpa => mentions(build, &["spring-data-jpa", "spring-boot-starter-data-jpa"]),
        Feature::Flyway => build.contains("flyway-core"),
    }
}

/// Consistent refusal for a feature that needs a literally declared library in the build text above a source.
pub fn gate(feature: Feature, build: &str) -> Gate {
    let (empty, missing) = match feature {
        Feature::SpringDataJpa => (
            "No pom.xml, build.gradle or build.gradle.kts could be read above this source, so Spring Data JPA cannot be proven.",
            "Spring Data JPA is not provably on this project: spring-data-jpa or spring-boot-starter-data-jpa must be written literally in a build file above this source (version catalogs are not verified).",
        ),
        Feature::Flyway => (
            "No pom.xml, build.gradle or build.gradle.kts could be read above this file, so Flyway cannot be proven; nothing was generated.",
            "Flyway is not provably on this project: a literal flyway-core must be written in a pom.xml/build.gradle(.kts) above this file (version catalogs and indirect declarations are not verified); nothing was generated.",
        ),
    };
    if build.trim().is_empty() {
        return Gate::Refused(empty.into());
    }
    if !feature_present(feature, build) {
        return Gate::Refused(missing.into());
    }
    Gate::Allowed
}

fn dependency(all: &str, indirect: bool, present: bool) -> Fact {
    if present {
        return Fact::Yes;
    }
    if all.trim().is_empty() || indirect {
        return Fact::Unknown;
    }
    Fact::No
}

/// `own`: the module's build files; `all`: module plus ancestors up to the root.
pub fn from_text(module: &str, own: &str, all: &str, kotlin_dir: bool, java_dir: bool) -> ProjectFacts {
    let indirect = mentions(all, NOT_LITERAL) || mentions(all, INDIRECT);
    let dep = |needles: &[&str]| dependency(all, indirect, mentions(all, needles));

    let kotlin = if mentions(own, KOTLIN) || kotlin_dir {
        Fact::Yes
    } else if mentions(all, KOTLIN) || all.trim().is_empty() || indirect {
        Fact::Unknown
    } else {
        Fact::No
    };
    let java = if java_dir {
        Fact::Yes
    } else if kotlin_dir {
        Fact::No
    } else {
        Fact::Unknown
    };

    let boot = spring_boot_majors(all);
    let jakarta = all.contains("jakarta.persistence") || boot.iter().any(|major| *major >= 3);
    let javax = all.contains("javax.persistence") || boot.iter().any(|major| *major <= 2);
    let namespace = match (jakarta, javax) {
        (true, false) => Namespace::Jakarta,
        (false, true) => Namespace::Javax,
        _ => Namespace::Unknown,
    };

    let majors: Vec<u32> = hibernate_core_majors(all).into_iter().filter(|major| *major != 0).collect();
    let hibernate_major = majors.first().copied().filter(|first| majors.iter().all(|major| major == first));
    let hibernate6_fact = match (hibernate6(all).is_ok(), hibernate_major) {
        (true, _) => Fact::Yes,
        (false, Some(major)) if major != 6 => Fact::No,
        _ => Fact::Unknown,
    };

    ProjectFacts {
        module: module.into(),
        kotlin,
        java,
        kotlin_jpa: if mentions(own, APPLIED) { Fact::Yes } else if kotlin == Fact::No { Fact::No } else { Fact::Unknown },
        namespace,
        spring_data_jpa: dependency(all, indirect, feature_present(Feature::SpringDataJpa, all)),
        hibernate6: hibernate6_fact,
        hibernate_major,
        lombok: dep(&["org.projectlombok"]),
        mapstruct: dep(&["org.mapstruct"]),
        flyway: dependency(all, indirect, feature_present(Feature::Flyway, all)),
        liquibase: dep(&["liquibase-core"]),
        jackson: dep(&["com.fasterxml.jackson", "jackson-databind"]),
        bean_validation: dep(&["spring-boot-starter-validation", "hibernate-validator"]),
        java_level: java_level(own).or_else(|| java_level(all)),
    }
}

fn level_of(value: &str) -> Option<u32> {
    let value = value.trim().trim_matches(['\'', '"']).trim();
    let value = value.split_once("VERSION_").map_or(value, |(_, version)| version).replace('_', ".");
    let value = value.strip_prefix("1.").unwrap_or(&value);
    let digits: String = value.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|level| (5..=60).contains(level))
}

fn tag_values(text: &str, tag: &str) -> Vec<String> {
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(&open) {
        rest = &rest[at + open.len()..];
        let Some(end) = rest.find(&close) else {
            break;
        };
        let value = rest[..end].trim();
        let resolved = match value.strip_prefix("${").and_then(|name| name.strip_suffix('}')) {
            Some(name) => tag_values(text, name).first().cloned().unwrap_or_default(),
            None => value.to_owned(),
        };
        found.push(resolved);
        rest = &rest[end + close.len()..];
    }
    found
}

fn after_key(text: &str, key: &str) -> Vec<String> {
    text.match_indices(key)
        .map(|(at, _)| text[at + key.len()..].trim_start_matches([' ', '\t', '=', '(']).lines().next().unwrap_or_default().to_owned())
        .collect()
}

/// Explicit release wins over source/target; the lowest of several declarations is used. Version catalogs
/// and BOM-managed values do not parse and leave the level unknown.
fn java_level(text: &str) -> Option<u32> {
    let mut release: Vec<u32> = ["maven.compiler.release", "release"].iter().flat_map(|tag| tag_values(text, tag)).filter_map(|v| level_of(&v)).collect();
    release.extend(["options.release", "release.set(", "JavaLanguageVersion.of(", "jvmToolchain("].iter().flat_map(|key| after_key(text, key)).filter_map(|v| level_of(&v)));
    if let Some(level) = release.into_iter().min() {
        return Some(level);
    }
    let mut other: Vec<u32> = ["maven.compiler.source", "maven.compiler.target", "java.version"].iter().flat_map(|tag| tag_values(text, tag)).filter_map(|v| level_of(&v)).collect();
    other.extend(["sourceCompatibility", "targetCompatibility"].iter().flat_map(|key| after_key(text, key)).filter_map(|v| level_of(&v)));
    if let Some(level) = other.into_iter().min() {
        return Some(level);
    }
    spring_boot_majors(text).iter().any(|major| *major >= 3).then_some(17)
}

/// Java record DTO forms need Java 16: `Some(level)` below 16 is refused, `None` (unproven) is allowed with a preview note.
pub fn java_level_below_record(project: &Project, directory: &str) -> Option<u32> {
    detect_cached(project, directory).java_level.filter(|level| *level < 16)
}

fn inside(root: &str, path: &str) -> bool {
    path == root || path.strip_prefix(root).is_some_and(|rest| rest.starts_with('/'))
}

/// Nearest ancestor of `start` (inclusive) holding a pom.xml or build.gradle(.kts), bounded by the root.
pub fn module_of(project: &Project, root: &str, start: &str) -> String {
    let root = root.trim_end_matches('/');
    let mut dir = start.trim_end_matches('/').to_owned();
    for _ in 0..MAX_WALK {
        if !inside(root, &dir) {
            break;
        }
        let has_build = BUILD_FILES.iter().any(|name| project.exists(&format!("{dir}/{name}")).unwrap_or(false));
        if has_build {
            return dir;
        }
        let Some((parent, _)) = dir.rsplit_once('/') else {
            break;
        };
        dir = parent.to_owned();
    }
    root.to_owned()
}

pub fn detect(project: &Project, module_dir: &str) -> ProjectFacts {
    let Ok(root) = project.root() else {
        return from_text(module_dir, "", "", false, false);
    };
    let module = module_of(project, &root, module_dir);
    let read = |directory: &str, top: &str| build_text(project, top, directory).unwrap_or_default();
    let own = read(&module, &module);
    let all = read(&module, &root);
    let exists = |relative: &str| project.exists(&format!("{module}/{relative}")).unwrap_or(false);

    from_text(&module, &own, &all, exists("src/main/kotlin"), exists("src/main/java"))
}

type Key = (String, String);

thread_local! {
    static CACHE: RefCell<HashMap<Key, (Instant, ProjectFacts)>> = RefCell::new(HashMap::new());
}

/// Cached for two minutes per (root, module); see [`invalidate`].
pub fn detect_cached(project: &Project, module_dir: &str) -> ProjectFacts {
    let Ok(root) = project.root() else {
        return detect(project, module_dir);
    };
    let module = module_of(project, &root, module_dir);
    let key = (root, module.clone());
    let hit = CACHE.with(|cache| {
        cache.borrow().get(&key).filter(|(at, _)| at.elapsed() < TTL).map(|(_, facts)| facts.clone())
    });
    if let Some(facts) = hit {
        return facts;
    }
    let facts = detect(project, &module);
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= MAX_CACHED {
            cache.clear();
        }
        cache.insert(key, (Instant::now(), facts.clone()));
    });
    facts
}

/// Drops cached facts when a build file or a source-root path changes.
pub fn invalidate(path: &str) {
    if is_build_file(path) || path.contains("/src/main/") {
        invalidate_all();
    }
}

pub fn invalidate_all() {
    CACHE.with(|cache| cache.borrow_mut().clear());
}

/// `entityLanguage`: `kotlin` always Kotlin; `auto` is Kotlin only when the module is Kotlin and not Java;
/// anything else (including the default `java` and Unknown facts) is Java.
pub fn prefers_kotlin(setting: Option<&str>, facts: &ProjectFacts) -> bool {
    match setting {
        Some("kotlin") => true,
        Some("auto") => facts.kotlin == Fact::Yes && facts.java == Fact::No,
        _ => false,
    }
}

/// Language without asking, or `None` when the picker is needed. The picker appears only where Kotlin is
/// detected or unknown (G014-C1): Kotlin `No` is Java; Kotlin `Yes` and Java `No` is Kotlin under `auto`.
/// An explicit `kotlin` setting always keeps the picker (its default is then Kotlin).
pub fn decided_language(setting: Option<&str>, facts: &ProjectFacts) -> Option<bool> {
    match (setting, facts.kotlin, facts.java) {
        (Some("kotlin"), _, _) => None,
        (_, Fact::No, _) => Some(false),
        (Some("auto"), Fact::Yes, Fact::No) => Some(true),
        _ => None,
    }
}

pub fn kotlin_default(ctx: &Invocation) -> bool {
    let setting = host::setting_string("entityLanguage");
    match setting.as_deref() {
        Some("auto") => {}
        other => return other == Some("kotlin"),
    }
    let Some(project) = ctx.project() else {
        return false;
    };
    let editor_dir = ctx.editor().and_then(|editor| editor.path().rsplit_once('/').map(|(dir, _)| dir.to_owned()));
    let Some(start) = editor_dir.or_else(|| project.root().ok()) else {
        return false;
    };

    prefers_kotlin(setting.as_deref(), &detect_cached(&project, &start))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const KTS: &str = "plugins { id(\"org.springframework.boot\") version \"3.3.0\"\n kotlin(\"jvm\") version \"2.4.21\"\n kotlin(\"plugin.jpa\") version \"2.4.21\" }\ndependencies { implementation(\"org.springframework.boot:spring-boot-starter-data-jpa:3.3.0\") }";
    const GROOVY: &str = "plugins { id 'org.jetbrains.kotlin.jvm' version '2.4.21'\n id 'org.jetbrains.kotlin.plugin.jpa' version '2.4.21' }\ndependencies { implementation 'javax.persistence:javax.persistence-api:2.2'\n implementation 'org.projectlombok:lombok:1.18.30' }";
    const POM: &str = "<project><build><plugins><plugin><groupId>org.jetbrains.kotlin</groupId><artifactId>kotlin-maven-plugin</artifactId><configuration><compilerPlugins><plugin>jpa</plugin></compilerPlugins></configuration></plugin></plugins></build><dependencies><dependency><groupId>jakarta.persistence</groupId><artifactId>jakarta.persistence-api</artifactId></dependency><dependency><groupId>org.flywaydb</groupId><artifactId>flyway-core</artifactId></dependency><dependency><groupId>org.mapstruct</groupId><artifactId>mapstruct</artifactId></dependency><dependency><groupId>org.liquibase</groupId><artifactId>liquibase-core</artifactId></dependency></dependencies></project>";

    fn facts(own: &str, all: &str, kotlin_dir: bool, java_dir: bool) -> ProjectFacts {
        from_text("/project", own, all, kotlin_dir, java_dir)
    }

    #[test]
    fn gradle_kts_detects_kotlin_jpa_and_jakarta_through_boot_three() {
        let f = facts(KTS, KTS, false, false);
        assert_eq!((f.kotlin, f.kotlin_jpa, f.spring_data_jpa), (Fact::Yes, Fact::Yes, Fact::Yes));
        assert_eq!(f.namespace, Namespace::Jakarta);
        assert_eq!(f.java, Fact::Unknown);
    }

    #[test]
    fn gradle_groovy_detects_javax_lombok_and_missing_libraries() {
        let f = facts(GROOVY, GROOVY, true, false);
        assert_eq!((f.kotlin, f.java, f.namespace, f.lombok), (Fact::Yes, Fact::No, Namespace::Javax, Fact::Yes));
        assert_eq!((f.flyway, f.mapstruct, f.jackson), (Fact::No, Fact::No, Fact::No));
    }

    #[test]
    fn maven_detects_plugin_and_literal_libraries() {
        let f = facts(POM, POM, false, true);
        assert_eq!((f.kotlin, f.java, f.kotlin_jpa), (Fact::Yes, Fact::Yes, Fact::Yes));
        assert_eq!((f.flyway, f.liquibase, f.mapstruct, f.namespace), (Fact::Yes, Fact::Yes, Fact::Yes, Namespace::Jakarta));
    }

    #[test]
    fn version_catalogs_boms_and_parents_are_unknown_never_no() {
        for build in [
            "plugins { alias(libs.plugins.kotlin.jvm) }\ndependencies { implementation(libs.flyway) }",
            "dependencies { implementation(platform(\"x:y:1\")) }",
            "<project><parent><artifactId>corp</artifactId></parent></project>",
        ] {
            let f = facts(build, build, false, false);
            assert_eq!((f.kotlin, f.flyway, f.lombok, f.spring_data_jpa), (Fact::Unknown, Fact::Unknown, Fact::Unknown, Fact::Unknown), "{build}");
        }
    }

    #[test]
    fn empty_build_and_garbage_never_panic_and_stay_unknown() {
        let garbage = "\u{0}\u{1} <<<>>> \"'(( ${{ org.hibernate:hibernate-core: <artifactId>hibernate-core</artifactId> <version>é";
        for build in ["", garbage] {
            let f = facts(build, build, false, false);
            assert_eq!(f.namespace, Namespace::Unknown);
            assert_ne!(f.hibernate6, Fact::Yes);
        }
        assert_eq!(facts("", "", false, false).kotlin, Fact::Unknown);
    }

    #[test]
    fn plain_java_build_is_kotlin_no_and_ancestor_only_kotlin_is_unknown() {
        let java = "<project><dependencies><dependency><artifactId>spring-data-jpa</artifactId></dependency></dependencies></project>";
        assert_eq!(facts(java, java, false, true).kotlin, Fact::No);
        assert_eq!(facts(java, &format!("{java}{KTS}"), false, true).kotlin, Fact::Unknown);
    }

    #[test]
    fn hibernate_major_and_boot_two_javax() {
        let five = "implementation 'org.hibernate:hibernate-core:5.6.15.Final'";
        let f = facts(five, five, false, false);
        assert_eq!((f.hibernate_major, f.hibernate6), (Some(5), Fact::No));
        let six = "implementation 'org.hibernate.orm:hibernate-core:6.4.1.Final'";
        assert_eq!(facts(six, six, false, false).hibernate6, Fact::Yes);
        let boot2 = "plugins { id 'org.springframework.boot' version '2.7.18' }";
        assert_eq!(facts(boot2, boot2, false, false).namespace, Namespace::Javax);
    }

    #[test]
    fn java_level_is_read_from_maven_gradle_and_boot_and_stays_unknown_otherwise() {
        let level = |build: &str| facts(build, build, false, true).java_level;
        assert_eq!(level("<properties><java.version>11</java.version></properties>"), Some(11));
        assert_eq!(level("<properties><java.version>1.8</java.version></properties>"), Some(8));
        assert_eq!(level("<properties><maven.compiler.source>11</maven.compiler.source><maven.compiler.target>11</maven.compiler.target></properties>"), Some(11));
        assert_eq!(level("<properties><maven.compiler.release>17</maven.compiler.release><maven.compiler.source>11</maven.compiler.source></properties>"), Some(17));
        assert_eq!(level("<properties><v>21</v><maven.compiler.release>${v}</maven.compiler.release></properties>"), Some(21));
        assert_eq!(level("<plugin><artifactId>maven-compiler-plugin</artifactId><configuration><release>11</release></configuration></plugin>"), Some(11));
        assert_eq!(level("<properties><java.version>11</java.version></properties><!-- <java.version>21</java.version> -->"), Some(11));
        assert_eq!(level("sourceCompatibility = '1.8'"), Some(8));
        assert_eq!(level("sourceCompatibility = JavaVersion.VERSION_17"), Some(17));
        assert_eq!(level("java { sourceCompatibility = JavaVersion.VERSION_1_8 }"), Some(8));
        assert_eq!(level("java { toolchain { languageVersion = JavaLanguageVersion.of(21) } }"), Some(21));
        assert_eq!(level("java { toolchain { languageVersion.set(JavaLanguageVersion.of(11)) } }"), Some(11));
        assert_eq!(level("tasks.withType<JavaCompile> { options.release.set(11) }"), Some(11));
        assert_eq!(level("kotlin { compilerOptions { jvmTarget = JvmTarget.JVM_11 } }"), None);
        assert_eq!(level("<parent><artifactId>spring-boot-starter-parent</artifactId><version>3.3.0</version></parent>"), Some(17));
        assert_eq!(level("plugins { id 'org.springframework.boot' version '3.2.1' }"), Some(17));
        assert_eq!(level("<parent><artifactId>spring-boot-starter-parent</artifactId><version>2.7.18</version></parent><properties><java.version>11</java.version></properties>"), Some(11));
        assert_eq!(level("<parent><artifactId>spring-boot-starter-parent</artifactId><version>2.7.18</version></parent>"), None);
        assert_eq!(level("java { sourceCompatibility = JavaVersion.toVersion(libs.versions.java.get()) }"), None);
        assert_eq!(level(""), None);
    }

    #[test]
    fn java_level_prefers_the_module_over_its_ancestors() {
        let own = "<properties><java.version>11</java.version></properties>";
        let parent = "<parent><artifactId>spring-boot-starter-parent</artifactId><version>3.3.0</version></parent>";
        assert_eq!(from_text("m", own, &format!("{own}{parent}"), false, true).java_level, Some(11));
        assert_eq!(from_text("m", "<project/>", &format!("<project/>{parent}"), false, true).java_level, Some(17));
    }

    #[test]
    fn bean_validation_needs_hibernate_validator_or_the_starter() {
        let level = |build: &str| facts(build, build, false, true).bean_validation;
        assert_eq!(level("<artifactId>spring-boot-starter-validation</artifactId>"), Fact::Yes);
        assert_eq!(level("implementation 'org.hibernate.validator:hibernate-validator:8.0.1.Final'"), Fact::Yes);
        assert_eq!(level("<artifactId>jakarta.validation-api</artifactId>"), Fact::No);
        assert_eq!(level("dependencies { implementation(libs.validation) }"), Fact::Unknown);
    }

    #[test]
    fn language_selection_matrix() {
        let with = |kotlin, java| ProjectFacts { kotlin, java, ..facts("", "", false, false) };
        let kotlin_only = with(Fact::Yes, Fact::No);
        let both = with(Fact::Yes, Fact::Yes);
        let unknown = with(Fact::Unknown, Fact::Unknown);
        let java_only = with(Fact::No, Fact::Yes);
        assert!(prefers_kotlin(Some("auto"), &kotlin_only));
        assert!(!prefers_kotlin(Some("auto"), &both));
        assert!(!prefers_kotlin(Some("auto"), &unknown));
        assert!(!prefers_kotlin(Some("auto"), &java_only));
        for facts in [&kotlin_only, &java_only, &unknown] {
            assert!(prefers_kotlin(Some("kotlin"), facts));
            assert!(!prefers_kotlin(Some("java"), facts));
            assert!(!prefers_kotlin(None, facts));
        }
    }

    fn project_with(files: &[(&str, &str)], dirs: &[&str]) -> (MockHost, Project) {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        for (path, text) in files {
            host.add_file(path, text);
        }
        for dir in dirs {
            host.add_dir(dir);
        }
        let project = host.invocation().project().unwrap();
        (host, project)
    }

    #[test]
    fn nested_modules_resolve_to_their_own_build_and_dirs() {
        let (_host, project) = project_with(
            &[("build.gradle.kts", "plugins { }"), ("api/build.gradle.kts", KTS), ("web/pom.xml", "<project/>")],
            &["api/src/main/kotlin", "web/src/main/java"],
        );
        assert_eq!(module_of(&project, "/project", "/project/api/src/main/kotlin/shop"), "/project/api");
        assert_eq!(module_of(&project, "/project", "/project/web/src/main/java"), "/project/web");
        assert_eq!(module_of(&project, "/project", "/project/docs"), "/project");
        let api = detect(&project, "/project/api/src/main/kotlin");
        assert_eq!((api.kotlin, api.java), (Fact::Yes, Fact::No));
        let web = detect(&project, "/project/web/src/main/java");
        assert_eq!((web.kotlin, web.java), (Fact::No, Fact::Yes));
    }

    #[test]
    fn cache_serves_stale_until_invalidated() {
        invalidate_all();
        let (host, project) = project_with(&[("pom.xml", "<project/>")], &[]);
        assert_eq!(detect_cached(&project, "/project").flyway, Fact::No);
        host.add_file("pom.xml", POM);
        assert_eq!(detect_cached(&project, "/project").flyway, Fact::No);
        invalidate("/project/docs/x.txt");
        assert_eq!(detect_cached(&project, "/project").flyway, Fact::No);
        invalidate("/project/pom.xml");
        assert_eq!(detect_cached(&project, "/project").flyway, Fact::Yes);
        invalidate_all();
    }

    #[test]
    fn gate_messages_and_decisions() {
        assert_eq!(gate(Feature::Flyway, "<artifactId>flyway-core</artifactId>"), Gate::Allowed);
        assert!(matches!(gate(Feature::Flyway, ""), Gate::Refused(m) if m.contains("above this file")));
        assert!(matches!(gate(Feature::SpringDataJpa, "x"), Gate::Refused(m) if m.starts_with("Spring Data JPA is not provably")));
    }

    #[test]
    fn decided_language_asks_only_where_kotlin_is_detected_or_unknown() {
        let facts = |kotlin, java| ProjectFacts { kotlin, java, ..from_text("m", "", "", false, false) };
        let (yes, no, unknown) = (Fact::Yes, Fact::No, Fact::Unknown);
        for setting in [None, Some("java"), Some("auto")] {
            assert_eq!(decided_language(setting, &facts(no, yes)), Some(false), "{setting:?}");
            assert_eq!(decided_language(setting, &facts(no, unknown)), Some(false), "{setting:?}");
            assert_eq!(decided_language(setting, &facts(yes, yes)), None, "{setting:?}");
            assert_eq!(decided_language(setting, &facts(unknown, yes)), None, "{setting:?}");
            assert_eq!(decided_language(setting, &facts(unknown, unknown)), None, "{setting:?}");
        }
        assert_eq!(decided_language(Some("auto"), &facts(yes, no)), Some(true));
        assert_eq!(decided_language(Some("java"), &facts(yes, no)), None);
        assert_eq!(decided_language(None, &facts(yes, no)), None);
        assert_eq!(decided_language(Some("kotlin"), &facts(no, yes)), None);
    }

    #[test]
    fn kotlin_default_reads_setting_and_project() {
        invalidate_all();
        let (host, _project) = project_with(&[("pom.xml", POM)], &["src/main/kotlin"]);
        let ctx = host.invocation();
        assert!(!kotlin_default(&ctx));
        host.set_setting("entityLanguage", "\"kotlin\"");
        assert!(kotlin_default(&ctx));
        host.set_setting("entityLanguage", "\"auto\"");
        assert!(kotlin_default(&ctx));
        host.add_dir("src/main/java");
        invalidate_all();
        assert!(!kotlin_default(&ctx));
    }
}
