//! Read-only, opt-in native runs. Discovery never executes a build or wrapper.
use crate::BootTools;
use openidea_plugin::{PluginRunConfiguration, RunConfigurations, host};
const SETTING: &str = "springBoot.nativeRunProfiles";
const MAX_BUILD: usize = 128 * 1024;

fn namespace_valid<R>(
    reader: &quick_xml::reader::NsReader<R>,
    element: &quick_xml::events::BytesStart<'_>,
) -> bool {
    use quick_xml::name::ResolveResult;
    const XML: &str = "http://www.w3.org/XML/1998/namespace";
    const XMLNS: &str = "http://www.w3.org/2000/xmlns/";
    let qualified = |name: &str| {
        let parts: Vec<_> = name.split(':').collect();
        parts.len() <= 2 && parts.iter().all(|p| !p.is_empty())
    };
    if !qualified(element.name().as_ref())
        || element.name().as_ref().starts_with("xmlns:")
        || matches!(
            reader.resolver().resolve_element(element.name()).0,
            ResolveResult::Unknown(_)
        )
    {
        return false;
    }
    for attribute in element.attributes() {
        let Ok(attribute) = attribute else {
            return false;
        };
        let key = attribute.key.as_ref();
        if !qualified(key) {
            return false;
        }
        if key == "xmlns" || key.starts_with("xmlns:") {
            let Ok(value) = attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0) else {
                return false;
            };
            if key == "xmlns:xmlns"
                || key.starts_with("xmlns:") && value.is_empty()
                || key == "xmlns:xml" && value != XML
                || key != "xmlns:xml" && matches!(value.as_ref(), XML | XMLNS)
            {
                return false;
            }
        } else if matches!(
            reader.resolver().resolve_attribute(attribute.key).0,
            ResolveResult::Unknown(_)
        ) {
            return false;
        }
    }
    true
}
fn maven_boot(text: &str) -> bool {
    use quick_xml::{events::Event, name::ResolveResult};
    if text.len() > MAX_BUILD {
        return false;
    }
    let mut reader = quick_xml::reader::NsReader::from_str(text);
    reader.config_mut().check_comments = true;
    let mut stack = Vec::<String>::new();
    let mut namespace = None;
    let mut roots = 0;
    let mut found = 0;
    let mut group = String::new();
    let mut artifact = String::new();
    let mut fields = [0usize; 3];
    loop {
        let event = reader.read_event();
        let empty = matches!(&event, Ok(Event::Empty(_)));
        match event {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                let resolved = match reader.resolver().resolve_element(e.name()).0 {
                    ResolveResult::Unbound => "".to_owned(),
                    ResolveResult::Bound(ns)
                        if ns.as_ref() == "http://maven.apache.org/POM/4.0.0" =>
                    {
                        ns.as_ref().to_owned()
                    }
                    _ => return false,
                };
                if namespace.as_ref().is_some_and(|ns| ns != &resolved) {
                    return false;
                }
                namespace.get_or_insert(resolved);
                if !namespace_valid(&reader, &e)
                    || e.attributes().with_checks(true).any(|a| a.is_err())
                {
                    return false;
                }
                let name = e.local_name().as_ref().to_owned();
                if stack.is_empty() {
                    roots += 1;
                    if name != "project" || roots != 1 {
                        return false;
                    }
                }
                if stack.len() >= 32 {
                    return false;
                }
                if stack == ["project", "build", "plugins"] && name == "plugin" {
                    group.clear();
                    artifact.clear();
                    fields = [0; 3];
                }
                if stack == ["project", "build", "plugins", "plugin"] {
                    match name.as_str() {
                        "groupId" => fields[0] += 1,
                        "artifactId" => fields[1] += 1,
                        "version" => fields[2] += 1,
                        _ => {}
                    }
                }
                if !empty {
                    stack.push(name);
                }
            }
            Ok(Event::End(_)) => {
                if stack == ["project", "build", "plugins", "plugin"] {
                    if fields[0..2] == [1, 1]
                        && fields[2] <= 1
                        && group.trim() == "org.springframework.boot"
                        && artifact.trim() == "spring-boot-maven-plugin"
                    {
                        found += 1;
                    }
                }
                if stack.pop().is_none() {
                    return false;
                }
            }
            Ok(Event::Text(t)) => {
                let raw: &str = t.as_ref();
                if raw.contains('&') {
                    return false;
                }
                if stack == ["project", "build", "plugins", "plugin", "groupId"] {
                    group.push_str(&raw);
                } else if stack == ["project", "build", "plugins", "plugin", "artifactId"] {
                    artifact.push_str(&raw);
                } else if stack.is_empty() && !raw.trim().is_empty() {
                    return false;
                }
            }
            Ok(Event::DocType(_) | Event::CData(_) | Event::GeneralRef(_)) => return false,
            Ok(Event::Eof) => return roots == 1 && stack.is_empty() && found == 1,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
}
fn literal(token: &str) -> Option<&str> {
    if token.len() < 2 || token.contains(['\\', '\n', '\r', '$']) {
        return None;
    }
    let quote = token.as_bytes()[0];
    (matches!(quote, b'\'' | b'"') && token.as_bytes().last() == Some(&quote))
        .then(|| &token[1..token.len() - 1])
}
fn gradle_boot(text: &str, kotlin: bool) -> bool {
    if text.len() > MAX_BUILD || text.contains("'''") {
        return false;
    }
    let tokens = spring_text::java::tokens(text);
    if tokens.len() > 16384 || tokens.iter().any(|t| t.s == "/") {
        return false;
    }
    // An explicitly declared local/helper symbol must not borrow Kotlin DSL's
    // built-in plugin accessor identity.
    if kotlin
        && tokens.windows(2).any(|w| {
            matches!(
                w[0].s,
                "val" | "var" | "fun" | "class" | "object" | "interface" | "as"
            ) && matches!(w[1].s, "java" | "application" | "kotlin")
        })
    {
        return false;
    }
    if kotlin {
        for (i, _) in tokens.iter().enumerate().filter(|(_, t)| t.s == "import") {
            let mut end = i + 1;
            if end >= tokens.len() {
                return false;
            }
            while end + 1 < tokens.len()
                && !text[tokens[end].end()..tokens[end + 1].at].contains('\n')
                && tokens[end].s != ";"
            {
                end += 1;
            }
            if matches!(tokens[end].s, "java" | "application" | "kotlin") {
                return false;
            }
        }
    }
    let mut braces: Vec<(&str, usize)> = Vec::new();
    let mut plugin_block = None;
    let mut found = 0;
    let mut jvm = 0;
    for (i, t) in tokens.iter().enumerate() {
        if braces.is_empty() && t.s == "plugins" && tokens.get(i + 1).is_some_and(|t| t.s == "{") {
            if plugin_block.is_some()
                || i.checked_sub(1)
                    .and_then(|i| tokens.get(i))
                    .is_some_and(|t| t.s == ".")
            {
                return false;
            }
            plugin_block = Some(i + 1);
        }
        if braces.len() == 1 && plugin_block.is_some_and(|at| braces[0].1 == at) {
            if matches!(t.s, "def" | "val" | "var" | "fun" | "class") {
                return false;
            }
            let request = if t.s == "id" || kotlin && t.s == "kotlin" {
                let (value, next) = if tokens.get(i + 1).is_some_and(|t| t.s == "(") {
                    let Some(value) = tokens.get(i + 2).and_then(|t| literal(t.s)) else {
                        return false;
                    };
                    if tokens.get(i + 3).is_none_or(|t| t.s != ")") {
                        return false;
                    }
                    (value, i + 4)
                } else {
                    if t.s == "kotlin" {
                        return false;
                    }
                    let Some(value) = tokens.get(i + 1).and_then(|t| literal(t.s)) else {
                        return false;
                    };
                    (value, i + 2)
                };
                Some((
                    if t.s == "kotlin" && value == "jvm" {
                        "org.jetbrains.kotlin.jvm"
                    } else {
                        value
                    },
                    next,
                ))
            } else if kotlin && matches!(t.s, "java" | "application") {
                Some((t.s, i + 1))
            } else {
                None
            };
            if let Some((value, mut next)) = request {
                if value == "org.springframework.boot"
                    || matches!(
                        value,
                        "java" | "java-library" | "application" | "org.jetbrains.kotlin.jvm"
                    )
                {
                    if tokens.get(next).is_some_and(|t| t.s == "version") {
                        if tokens.get(next + 1).and_then(|t| literal(t.s)).is_none() {
                            return false;
                        }
                        next += 2;
                    }
                    if tokens.get(next).is_some_and(|t| t.s == "apply") {
                        if tokens.get(next + 1).is_some_and(|t| t.s == "(") {
                            if tokens.get(next + 2).is_none_or(|t| t.s != "true")
                                || tokens.get(next + 3).is_none_or(|t| t.s != ")")
                            {
                                return false;
                            }
                            next += 4;
                        } else {
                            if tokens.get(next + 1).is_none_or(|t| t.s != "true") {
                                return false;
                            }
                            next += 2;
                        }
                    }
                    if let Some(tail) = tokens.get(next) {
                        if !matches!(tail.s, ";" | "}")
                            && !text[tokens[next - 1].end()..tail.at].contains('\n')
                        {
                            return false;
                        }
                    }
                    if value == "org.springframework.boot" {
                        found += 1;
                    } else {
                        jvm += 1;
                    }
                }
            }
        }
        match t.s {
            "{" => braces.push(("}", i)),
            "(" => braces.push((")", i)),
            "[" => braces.push(("]", i)),
            "}" | ")" | "]" => {
                if braces.pop().is_none_or(|(expected, _)| expected != t.s) {
                    return false;
                }
            }
            _ => {}
        }
    }
    braces.is_empty() && found == 1 && jvm > 0
}

fn owned_project(root: &str) -> Option<openidea_plugin::Project> {
    if root.is_empty()
        || !root.starts_with('/')
        || root == "/"
        || root.contains('\\')
        || root.chars().any(char::is_control)
        || root.split('/').any(|p| matches!(p, "." | ".."))
    {
        return None;
    }
    if !host::projects()
        .ok()?
        .iter()
        .any(|p| p.root().ok().as_deref() == Some(root))
    {
        return None;
    }
    let project = host::project_of(root)?;
    (project.root().ok().as_deref() == Some(root)).then_some(project)
}
const MAX_MODULES: usize = 32;
fn module_name(raw: &str) -> Option<&str> {
    let name = raw.trim();
    let valid = !name.is_empty()
        && !name.starts_with(['.', '-'])
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    valid.then_some(name)
}
/// Literal direct `<modules>` of a root POM; profile-scoped modules are ignored.
fn maven_modules(text: &str) -> Vec<String> {
    use quick_xml::events::Event;
    if text.len() > MAX_BUILD || text.contains("<!DOCTYPE") || text.contains("<!ENTITY") {
        return vec![];
    }
    let mut reader = quick_xml::reader::Reader::from_str(text);
    let mut stack = Vec::<String>::new();
    let mut modules = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.name().as_ref().to_owned();
                if stack.is_empty() {
                    let maven = e.attributes().flatten().any(|a| {
                        a.key.as_ref() == "xmlns"
                            && a.value.as_ref() == "http://maven.apache.org/POM/4.0.0"
                    });
                    if name != "project" || !maven {
                        return vec![];
                    }
                }
                stack.push(name);
            }
            Ok(Event::End(_)) => {
                stack.pop();
            }
            Ok(Event::Text(t)) if stack == ["project", "modules", "module"] => {
                let raw = t.xml_content(quick_xml::XmlVersion::Implicit1_0);
                let Some(name) = module_name(&raw) else {
                    return vec![];
                };
                modules.push(name.to_owned());
                if modules.len() > MAX_MODULES {
                    return vec![];
                }
            }
            Ok(Event::Eof) => return modules,
            Err(_) => return vec![],
            _ => {}
        }
    }
}
/// Literal `include` subprojects of a settings script; refuses custom project dirs.
fn gradle_modules(text: &str) -> Vec<String> {
    if text.len() > MAX_BUILD || text.contains("/*") || text.contains("projectDir") {
        return vec![];
    }
    let mut modules = Vec::new();
    for line in text.lines().map(str::trim) {
        let Some(rest) = line.strip_prefix("include") else {
            continue;
        };
        if !rest.starts_with(['(', ' ']) {
            continue;
        }
        let rest = rest.trim().trim_start_matches('(').trim_end_matches(')');
        let names = rest
            .split(',')
            .map(|t| literal(t.trim()).and_then(|n| module_name(n.strip_prefix(':').unwrap_or(n))))
            .collect::<Option<Vec<_>>>();
        let Some(names) = names else {
            return vec![];
        };
        modules.extend(names.into_iter().map(str::to_owned));
        if modules.len() > MAX_MODULES {
            return vec![];
        }
    }
    modules
}
fn module_runs(project: &openidea_plugin::Project, root: &str) -> Vec<PluginRunConfiguration> {
    let mut runs = Vec::new();
    let maven = project
        .read_file("pom.xml")
        .ok()
        .map(|text| maven_modules(&text))
        .unwrap_or_default();
    for name in maven {
        let file = format!("{name}/pom.xml");
        if project.read_file(&file).ok().is_some_and(|t| maven_boot(&t)) {
            runs.push(PluginRunConfiguration {
                id: format!("source-maven-boot-run-module-{name}"),
                name: format!("Spring Boot: Maven module {name} spring-boot:run"),
                program: "mvn".into(),
                args: vec!["--batch-mode".into(), "--file".into(), file, "spring-boot:run".into()],
                cwd: Some(root.into()),
                ..Default::default()
            });
        }
    }
    let settings = ["settings.gradle", "settings.gradle.kts"]
        .into_iter()
        .filter_map(|path| project.read_file(path).ok())
        .collect::<Vec<_>>();
    if let [text] = settings.as_slice() {
        for name in gradle_modules(text) {
            let found = ["build.gradle", "build.gradle.kts"]
                .into_iter()
                .filter_map(|b| {
                    project
                        .read_file(&format!("{name}/{b}"))
                        .ok()
                        .map(|t| gradle_boot(&t, b.ends_with(".kts")))
                })
                .collect::<Vec<_>>();
            if found == [true] {
                runs.push(PluginRunConfiguration {
                    id: format!("source-gradle-boot-run-module-{name}"),
                    name: format!("Spring Boot: Gradle subproject {name} bootRun"),
                    program: "gradle".into(),
                    args: vec!["--console=plain".into(), format!(":{name}:bootRun")],
                    cwd: Some(root.into()),
                    ..Default::default()
                });
            }
        }
    }
    runs
}
pub(crate) fn configurations(root: &str) -> Vec<PluginRunConfiguration> {
    if host::setting_bool(SETTING) != Some(true) {
        return vec![];
    }
    let Some(project) = owned_project(root) else {
        return vec![];
    };
    let mut runs = Vec::new();
    if project
        .read_file("pom.xml")
        .ok()
        .is_some_and(|text| maven_boot(&text))
    {
        runs.push(PluginRunConfiguration {
            id: "source-maven-boot-run".into(),
            name: "Spring Boot: Maven spring-boot:run (project build)".into(),
            program: "mvn".into(),
            args: vec![
                "--batch-mode".into(),
                "--file".into(),
                "pom.xml".into(),
                "spring-boot:run".into(),
            ],
            cwd: Some(root.into()),
            ..Default::default()
        });
    }
    // Two competing Gradle build scripts do not establish one build owner.
    let gradle = ["build.gradle", "build.gradle.kts"]
        .into_iter()
        .filter_map(|path| project.read_file(path).ok().map(|text| (path, text)))
        .collect::<Vec<_>>();
    if matches!(gradle.as_slice(), [(path,text)] if gradle_boot(text,path.ends_with(".kts"))) {
        runs.push(PluginRunConfiguration {
            id: "source-gradle-boot-run".into(),
            name: "Spring Boot: Gradle bootRun (project build)".into(),
            program: "gradle".into(),
            args: vec!["--console=plain".into(), "bootRun".into()],
            cwd: Some(root.into()),
            ..Default::default()
        });
    }
    runs.extend(module_runs(&project, root));
    runs
}
impl RunConfigurations for BootTools {
    fn configurations(&mut self, project_root: &str) -> Vec<PluginRunConfiguration> {
        configurations(project_root)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const POM: &str = "<project xmlns=\"http://maven.apache.org/POM/4.0.0\"><build><plugins><plugin><groupId>org.springframework.boot</groupId><artifactId>spring-boot-maven-plugin</artifactId><version>4.0.0</version></plugin></plugins></build></project>";
    #[test]
    fn observed_direct_build_plugins_only_establish_source_owned_runs() {
        assert!(maven_boot(POM));
        for rejected in [
            POM.replace("<plugins>", "<pluginManagement><plugins>")
                .replace("</plugins>", "</plugins></pluginManagement>"),
            POM.replace("org.springframework.boot", "foreign"),
            POM.replace("POM/4.0.0", "foreign"),
            POM.replace("</groupId>", "</groupId><groupId>foreign</groupId>"),
            POM.replace("</project>", ""),
            format!("<!DOCTYPE project []>{POM}"),
            POM.replace("</version>", "</version><version>4.1.2</version>"),
            POM.replace("<build>", "<build foreign:bad=\"true\">"),
            POM.replace("<build>", "<build xmlns:xml=\"foreign\">"),
        ] {
            assert!(!maven_boot(&rejected), "{rejected}");
        }
        for valid in [
            "plugins { id 'java'; id 'org.springframework.boot' version '4.0.0' }",
            "plugins {\n id(\"org.springframework.boot\") version \"4.0.0\"\n id(\"java\")\n}",
            "plugins { id(\"java-library\"); id(\"org.springframework.boot\") }",
            "plugins { application; id(\"org.springframework.boot\") }",
            "plugins { kotlin(\"jvm\") version \"2.2.0\"; id(\"org.springframework.boot\") }",
        ] {
            assert!(gradle_boot(valid, true), "{valid}");
        }
        assert!(!gradle_boot(
            "plugins { java; id(\"org.springframework.boot\") }",
            false
        ));
        assert!(gradle_boot(
            "plugins { id 'application'; id 'org.springframework.boot' }",
            false
        ));
        for rejected in [
            "import",
            "plugins { val java = 1; java; id(\"org.springframework.boot\") }",
            "plugins { id 'org.springframework.boot' }",
            "plugins { java apply false; id(\"org.springframework.boot\") }",
            "import foreign.kotlin\nplugins { kotlin(\"jvm\"); id(\"org.springframework.boot\") }",
            "// plugins { id 'org.springframework.boot' }",
            "subprojects { plugins { id 'org.springframework.boot' } }",
            "plugins { id 'org.springframework.boot' apply false }",
            "plugins { id(\"org.springframework.boot\")\n apply(false) }",
            "plugins { alias(libs.plugins.boot) }",
            "plugins { id 'org.springframework.boot' version dynamic }",
            "plugins { id 'org.springframework.boot'",
            "def example = /\nplugins { id 'org.springframework.boot' }\n/",
        ] {
            assert!(!gradle_boot(rejected, true), "{rejected}");
        }
    }
    #[test]
    fn native_discovery_is_opt_in_owned_read_only_and_withdraws_on_source_changes() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("pom.xml", POM);
        assert!(configurations("/project").is_empty());
        host.set_setting(SETTING, "true");
        let runs = configurations("/project");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].program, "mvn");
        assert_eq!(
            runs[0].args,
            ["--batch-mode", "--file", "pom.xml", "spring-boot:run"]
        );
        assert_eq!(runs[0].cwd.as_deref(), Some("/project"));
        assert!(!runs[0].test && runs[0].framework.is_none());
        host.add_file(
            "build.gradle.kts",
            "plugins { java; id(\"org.springframework.boot\") version \"4.0.0\" }",
        );
        assert_eq!(configurations("/project").len(), 2);
        host.add_file("build.gradle", "plugins { id 'org.springframework.boot' }");
        assert_eq!(configurations("/project").len(), 1);
        host.add_file("pom.xml", "<project/>");
        assert!(configurations("/project").is_empty());
        assert!(configurations("/other").is_empty());
        assert!(configurations("/project/../project").is_empty());
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }
    #[test]
    fn module_runs_follow_literal_root_modules_and_subprojects_only() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.set_setting(SETTING, "true");
        let root = "<project xmlns=\"http://maven.apache.org/POM/4.0.0\"><modules><module>app</module><module>lib</module></modules><profiles><profile><modules><module>other</module></modules></profile></profiles></project>";
        host.add_file("pom.xml", root);
        host.add_file("app/pom.xml", POM);
        host.add_file("lib/pom.xml", "<project xmlns=\"http://maven.apache.org/POM/4.0.0\"/>");
        host.add_file("other/pom.xml", POM);
        let runs = configurations("/project");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].args, ["--batch-mode", "--file", "app/pom.xml", "spring-boot:run"]);
        assert_eq!(runs[0].cwd.as_deref(), Some("/project"));
        assert!(maven_modules("<project><modules><module>a</module></modules></project>").is_empty());
        assert!(maven_modules(&root.replace("app", "../x")).is_empty());
        assert!(maven_modules(&format!("<!DOCTYPE project []>{root}")).is_empty());

        host.add_file("pom.xml", "<project/>");
        host.add_file("settings.gradle", "rootProject.name = 'x'\ninclude ':web', 'lib'\n");
        host.add_file("web/build.gradle", "plugins { id 'java'; id 'org.springframework.boot' }");
        host.add_file("lib/build.gradle", "plugins { id 'java' }");
        let runs = configurations("/project");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].args, ["--console=plain", ":web:bootRun"]);
        for refused in [
            "include(\"web\", foo)",
            "include 'a/b'",
            "include 'web'\nproject(':web').projectDir = file('x')",
            "/* x */ include 'web'",
        ] {
            assert!(gradle_modules(refused).is_empty(), "{refused}");
        }
        host.add_file("settings.gradle.kts", "include(\"web\")");
        assert!(configurations("/project").is_empty());
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }
}
