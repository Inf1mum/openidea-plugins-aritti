//! Explicit literal root-POM Maven-cache metadata snapshots, never classpath discovery.
use openidea_plugin::{Error, FileKind, Invocation, PickItem, host};
use sha2::{Digest, Sha256};
use spring_text::{
    config::canonical,
    metadata::{Catalog, Meta},
    xml,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};
const CACHE: &str = "~/.m2/repository";
#[derive(Clone, Debug, PartialEq, Eq)]
struct Dependency {
    group: String,
    artifact: String,
    version: String,
    scope: String,
}
impl Dependency {
    fn gav(&self) -> String {
        format!("{}:{}:{}", self.group, self.artifact, self.version)
    }
    fn path(&self, cache: &str) -> String {
        format!(
            "{}/{}/{}/{}/{}-{}.jar",
            cache,
            self.group.replace('.', "/"),
            self.artifact,
            self.version,
            self.artifact,
            self.version
        )
    }
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn literal(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && !matches!(s, "." | "..")
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}
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
fn pom_namespace<R>(
    reader: &quick_xml::reader::NsReader<R>,
    element: &quick_xml::events::BytesStart<'_>,
) -> Result<String, Error> {
    use quick_xml::name::ResolveResult;
    if !namespace_valid(reader, element) {
        return Err(invalid("Invalid POM namespace binding or qualified name."));
    }
    match reader.resolver().resolve_element(element.name()).0 {
        ResolveResult::Unbound => Ok(String::new()),
        ResolveResult::Bound(ns) if ns.as_ref() == "http://maven.apache.org/POM/4.0.0" => {
            Ok(ns.as_ref().to_owned())
        }
        _ => Err(invalid("Unbound or foreign POM element namespace.")),
    }
}
fn dependencies(text: &str) -> Result<Vec<Dependency>, Error> {
    if text.len() > 256 * 1024 {
        return Err(invalid(
            "Use a bounded literal root POM of at most 256 KiB.",
        ));
    }
    let mut reader = quick_xml::reader::NsReader::from_str(text);
    reader.config_mut().check_comments = true;
    let mut expected_namespace: Option<String> = None;
    let mut depth = 0usize;
    let mut roots = 0;
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Start(e)) => {
                let namespace = pom_namespace(&reader, &e)?;
                if let Some(expected) = &expected_namespace {
                    if expected != &namespace {
                        return Err(invalid("Mixed POM element namespaces are unsupported."));
                    }
                } else {
                    expected_namespace = Some(namespace);
                }
                if depth == 0 {
                    roots += 1;
                }
                depth += 1;
                if depth > 32 {
                    return Err(invalid("POM nesting limit exceeded."));
                }
                for a in e.attributes().with_checks(true) {
                    a.map_err(|_| invalid("Invalid POM attributes."))?;
                }
            }
            Ok(quick_xml::events::Event::Empty(e)) => {
                let namespace = pom_namespace(&reader, &e)?;
                if let Some(expected) = &expected_namespace {
                    if expected != &namespace {
                        return Err(invalid("Mixed POM element namespaces are unsupported."));
                    }
                } else {
                    expected_namespace = Some(namespace);
                }
                if depth == 0 {
                    roots += 1;
                }
                for a in e.attributes().with_checks(true) {
                    a.map_err(|_| invalid("Invalid POM attributes."))?;
                }
            }
            Ok(quick_xml::events::Event::End(_)) => {
                depth = depth.checked_sub(1).ok_or("Invalid POM nesting.")?;
            }
            Ok(quick_xml::events::Event::DocType(_) | quick_xml::events::Event::CData(_)) => {
                return Err(invalid("DTD/CDATA POM forms are unsupported."));
            }
            Ok(quick_xml::events::Event::Text(text)) if depth == 0 => {
                if text.as_ref().bytes().any(|b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n')) {
                    return Err(invalid("Non-whitespace text outside the POM root."));
                }
            }
            Ok(quick_xml::events::Event::GeneralRef(_)) if depth == 0 => {
                return Err(invalid("Entity references outside the POM root."));
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(_) => {}
            Err(_) => return Err(invalid("The POM is not well-formed XML.")),
        }
    }
    if roots != 1 || depth != 0 {
        return Err(invalid("The POM must have one complete project root."));
    }
    let elements = xml::elements(text);
    if elements.iter().enumerate().any(|(i, _)| {
        xml::namespace(&elements, i).is_some_and(|n| n != "http://maven.apache.org/POM/4.0.0")
    }) {
        return Err(invalid("Foreign POM element namespaces are unsupported."));
    }
    if xml::root(&elements) != Some("project")
        || xml::namespace(&elements, 0).is_some_and(|n| n != "http://maven.apache.org/POM/4.0.0")
    {
        return Err(invalid("The root pom.xml is not a supported Maven POM."));
    }
    if elements.len() > 4096 {
        return Err(invalid("POM element limit exceeded."));
    }
    let mut result = vec![];
    let mut keys = BTreeSet::new();
    for (i, e) in elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.local() == "dependency")
    {
        let Some(parent) = e.parent else {
            continue;
        };
        if elements[parent].local() != "dependencies" || elements[parent].parent != Some(0) {
            continue;
        }
        let mut fields = BTreeMap::new();
        let mut supported = true;
        for child in elements.iter().filter(|child| child.parent == Some(i)) {
            if !matches!(
                child.local(),
                "groupId" | "artifactId" | "version" | "scope" | "type" | "optional" | "exclusions"
            ) {
                supported = false;
                break;
            }
            if child.local() == "exclusions" {
                continue;
            }
            let Some(close) = text
                .get(child.tag_end..child.end)
                .and_then(|s| s.rfind("</").map(|at| child.tag_end + at))
            else {
                supported = false;
                break;
            };
            let value = text[child.tag_end..close].trim();
            if !literal(value) || fields.insert(child.local(), value).is_some() {
                supported = false;
                break;
            }
        }
        if !supported
            || fields.get("type").is_some_and(|v| *v != "jar")
            || fields.get("optional").is_some_and(|v| *v != "false")
        {
            continue;
        }
        let (Some(group), Some(artifact), Some(version)) = (
            fields.get("groupId"),
            fields.get("artifactId"),
            fields.get("version"),
        ) else {
            continue;
        };
        if group
            .split('.')
            .any(|p| p.is_empty() || p == "." || p == "..")
        {
            continue;
        }
        let scope = fields.get("scope").copied().unwrap_or("compile");
        if !matches!(scope, "compile" | "runtime" | "provided" | "test") {
            continue;
        }
        let dep = Dependency {
            group: (*group).into(),
            artifact: (*artifact).into(),
            version: (*version).into(),
            scope: scope.into(),
        };
        if !keys.insert(format!("{}:{}", group, artifact)) {
            return Err(invalid("Duplicate direct dependency owners are ambiguous."));
        }
        result.push(dep);
        if result.len() > 128 {
            return Err(invalid("Direct dependency limit exceeded."));
        }
    }
    Ok(result)
}
struct Snapshot {
    at: Instant,
    pom: String,
    pom_digest: String,
    jar: String,
    hash: String,
    dep: Dependency,
    meta: Meta,
    entries: Vec<String>,
}
#[derive(Default)]
pub struct State {
    snapshots: BTreeMap<String, Snapshot>,
}
impl State {
    pub fn invalidate(&mut self, path: &str) {
        self.snapshots.retain(|_, s| s.pom != path);
    }
    pub fn catalog(&self, path: &str) -> Catalog {
        let mut c = Catalog::default();
        if let Some(root) = host::project_of(path).and_then(|p| p.root().ok()) {
            if let Some(s) = self
                .snapshots
                .get(&root)
                .filter(|s| s.at.elapsed() < Duration::from_secs(600))
            {
                c.add(&s.meta);
            }
        }
        c
    }
    pub fn provenance(&self, path: &str) -> Option<String> {
        let root = host::project_of(path)?.root().ok()?;
        let s = self
            .snapshots
            .get(&root)
            .filter(|s| s.at.elapsed() < Duration::from_secs(600))?;
        Some(format!(
            "Selected Maven dependency snapshot: {} ({} scope), {} · SHA-256 {} · entries {}. Declared source provenance; resolved/runtime classpath is unverified.",
            s.dep.gav(),
            s.dep.scope,
            s.jar,
            s.hash,
            s.entries.join(", ")
        ))
    }
    pub fn import(&mut self, ctx: &Invocation) -> Result<Option<String>, Error> {
        let result = self.import_at(ctx, CACHE);
        if result.is_err()
            && ctx
                .project()
                .and_then(|p| p.root().ok())
                .is_some_and(|root| {
                    self.snapshots
                        .get(&root)
                        .is_some_and(|s| s.at.elapsed() < Duration::from_secs(600))
                })
        {
            host::notify(
                openidea_plugin::NotificationLevel::Warning,
                "Spring Boot JAR metadata",
                "Read failed. The previous selected dependency snapshot remains available until expiry or source invalidation; no new metadata was installed.",
            );
        }
        result
    }
    fn import_at(&mut self, ctx: &Invocation, cache: &str) -> Result<Option<String>, Error> {
        let project = ctx
            .project()
            .ok_or("Open the owning Maven project first.")?;
        let root = project.root()?;
        let pom = format!("{root}/pom.xml");
        if root.len() > 4096 || root.chars().any(char::is_control) {
            return Err(invalid("Unreviewable project identity."));
        }
        let source = project.read_file(&pom)?;
        let pom_digest = format!("{:x}", Sha256::digest(source.as_bytes()));
        if self
            .snapshots
            .get(&root)
            .is_some_and(|snapshot| snapshot.pom_digest != pom_digest)
        {
            self.snapshots.remove(&root);
        }
        let dependencies = dependencies(&source)?;
        if dependencies.is_empty() {
            return Err(invalid(
                "No supported direct literal JAR dependencies in the current root POM.",
            ));
        }
        let choices: Vec<_> = dependencies
            .iter()
            .enumerate()
            .map(|(i, d)| PickItem::new(&i.to_string(), &format!("{} ({})", d.gav(), d.scope)))
            .collect();
        let Some(key) = host::pick("Read metadata from one declared Maven dependency", &choices)?
        else {
            return Ok(None);
        };
        let selected = key
            .parse::<usize>()
            .ok()
            .and_then(|i| dependencies.get(i))
            .ok_or("Unknown dependency selection.")?
            .clone();
        let jar = selected.path(cache);
        let grant_probe = host::fs::stat(&jar)?;
        if grant_probe.kind != FileKind::File
            || grant_probe.size > super::jar_archive::MAX_ARCHIVE as u64
        {
            return Err(invalid("Selected JAR must be a file of at most 8 MiB."));
        }
        let review = format!(
            "Read local dependency JAR metadata?\n\nProject: {root}\nPOM: {pom}\nDeclared dependency: {}\nDeclared scope: {}\nExact archive: {jar}\n\nReads only META-INF/spring-configuration-metadata.json and additional-spring-configuration-metadata.json from this selected archive. No extraction, code execution, build or network. This is a source-declared snapshot, not a resolved/runtime classpath. Replaces this project's previous selected JAR snapshot only on success.",
            selected.gav(),
            selected.scope
        );
        if !ui_kit::prompts::confirm_exact("Read selected Maven JAR metadata", &review)? {
            return Ok(None);
        }
        let ensure = || -> Result<(), Error> {
            if ctx.project().and_then(|p| p.root().ok()).as_deref() != Some(&root)
                || host::project_of(&pom)
                    .and_then(|p| p.root().ok())
                    .as_deref()
                    != Some(&root)
                || project.read_file(&pom)? != source
            {
                return Err(invalid("Project/POM changed. Select the dependency again."));
            }
            Ok(())
        };
        ensure()?;
        let before = host::fs::stat(&jar)?;
        if before.kind != FileKind::File || before.size > super::jar_archive::MAX_ARCHIVE as u64 {
            return Err(invalid("Selected JAR must be a file of at most 8 MiB."));
        }
        let bytes = host::fs::read_bytes(&jar)?;
        let after = host::fs::stat(&jar)?;
        if before != after || bytes.len() as u64 != before.size {
            return Err(invalid("The selected JAR changed during reading."));
        }
        let entries = super::jar_archive::metadata(&bytes)?;
        let meta = parse_entries(&entries)?;
        ensure()?;
        let count = meta.props.len();
        let entries = entries.into_iter().map(|(name, _)| name).collect();
        self.snapshots
            .retain(|_, snapshot| snapshot.at.elapsed() < Duration::from_secs(600));
        if !self.snapshots.contains_key(&root) && self.snapshots.len() >= 8 {
            return Err(invalid(
                "Eight project snapshots are already retained; reload before importing another project.",
            ));
        }
        self.snapshots.insert(
            root,
            Snapshot {
                at: Instant::now(),
                pom,
                pom_digest,
                jar,
                hash: format!("{:x}", Sha256::digest(&bytes)),
                dep: selected,
                meta,
                entries,
            },
        );
        host::notify(
            openidea_plugin::NotificationLevel::Info,
            "Spring Boot JAR metadata",
            &format!(
                "Loaded {count} properties from one selected Maven dependency snapshot. Runtime classpath is unverified."
            ),
        );
        Ok(None)
    }
}
fn parse_entries(entries: &[(String, String)]) -> Result<Meta, Error> {
    if entries.is_empty()
        || entries.len() > 2
        || entries.iter().any(|(name, text)| {
            !matches!(
                name.as_str(),
                "META-INF/spring-configuration-metadata.json"
                    | "META-INF/additional-spring-configuration-metadata.json"
            ) || text.len() > 512 * 1024
        })
        || entries.iter().map(|(_, text)| text.len()).sum::<usize>() > 768 * 1024
    {
        return Err(invalid("Metadata document budget exceeded."));
    }
    let mut result = Meta::default();
    let mut names = BTreeSet::new();
    let mut hint_names = BTreeSet::new();
    for (_, text) in entries {
        let v: serde_json::Value =
            serde_json::from_str(text).map_err(|_| invalid("Metadata is not valid JSON."))?;
        if !v.is_object()
            || v.get("properties").is_some_and(|p| !p.is_array())
            || v.get("hints").is_some_and(|p| !p.is_array())
        {
            return Err(invalid("Unsupported configuration metadata structure."));
        }
        if v.get("properties")
            .and_then(|v| v.as_array())
            .is_some_and(|props| {
                props.iter().any(|p| {
                    !p.is_object()
                        || p.get("name").and_then(|n| n.as_str()).is_none()
                        || ["type", "description"]
                            .iter()
                            .any(|key| p.get(key).is_some_and(|v| !v.is_string()))
                        || p.get("deprecation")
                            .is_some_and(|v| !v.is_object() && !v.is_null())
                        || p.get("deprecated").is_some_and(|v| !v.is_boolean())
                })
            })
        {
            return Err(invalid("Unsupported metadata property record."));
        }
        let m = super::meta::parse_json(text).ok_or("Invalid configuration metadata.")?;
        for p in m.props {
            if p.ty.len() > 256
                || p.default.len() > 1024
                || p.ty.chars().chain(p.default.chars()).any(char::is_control)
                || p.deprecated.as_ref().is_some_and(|d| {
                    d.replacement.len() > 256 || d.replacement.chars().any(char::is_control)
                })
                || p.name.len() > 256
                || canonical(&p.name).is_empty()
                || !p.name.bytes().all(|b| {
                    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'[' | b']')
                })
                || !names.insert(canonical(&p.name))
            {
                return Err(invalid("Ambiguous or unsupported metadata property names."));
            }
            result.props.push(p);
        }
        for (name, values) in m.hints {
            if canonical(&name).is_empty()
                || !name.bytes().all(|b| {
                    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'[' | b']')
                })
                || name.len() > 256
                || values.len() > 128
                || values
                    .iter()
                    .any(|v| v.len() > 1024 || v.chars().any(char::is_control))
                || !hint_names.insert(canonical(&name))
            {
                return Err(invalid("Ambiguous or oversized metadata hints."));
            }
            result.hints.push((name, values));
        }
        if result.props.len() > 4096 || result.hints.len() > 1024 {
            return Err(invalid("Configuration metadata record limit exceeded."));
        }
    }
    if result.props.is_empty() {
        return Err(invalid(
            "The selected metadata contains no supported properties.",
        ));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::hover::HoverProvider;
    use openidea_plugin::mock::MockHost;
    use openidea_plugin::providers::Document;
    const POM: &str = "<project xmlns='http://maven.apache.org/POM/4.0.0'><modelVersion>4.0.0</modelVersion><dependencies><dependency><groupId>org.example</groupId><artifactId>config</artifactId><version>1.2.3</version></dependency></dependencies></project>";
    const JSON: &str = r#"{"properties":[{"name":"custom.jar.feature","type":"java.lang.Boolean","defaultValue":true,"description":"Selected archive fact."}],"hints":[{"name":"custom.jar.mode","values":[{"value":"safe"}]}]}"#;
    const PATH: &str = "/m2/org/example/config/1.2.3/config-1.2.3.jar";
    fn setup() -> MockHost {
        let h = MockHost::install().with_capabilities(&[
            "project.read",
            "fs.write:/m2",
            "fs.read:/m2",
            "editor.read",
        ]);
        h.add_file("pom.xml", POM);
        let bytes = super::super::jar_archive::fixture(&[(
            "META-INF/spring-configuration-metadata.json",
            JSON.as_bytes(),
            8,
        )]);
        h.add_external_file(PATH, "fixture placeholder");
        host::fs::write_bytes(PATH, &bytes).unwrap();
        h
    }
    fn answer(h: &MockHost, confirm: bool) {
        h.answer_next_prompt(Some(&["0"]));
        h.answer_next_prompt(if confirm { Some(&["ok"]) } else { None });
    }
    #[test]
    fn commented_pom_selects_only_live_literal_dependency() {
        let text = format!("<!-- <project><dependencies><dependency>fake</dependency></dependencies></project> -->\n{}\n<!-- trailing comment -->", POM.replace("<dependencies>", "<!-- dependency inventory --><dependencies><!-- <dependency><groupId>evil</groupId><artifactId>fake</artifactId><version>1</version></dependency> -->"));
        assert_eq!(dependencies(&text).unwrap(), dependencies(POM).unwrap());
        let h = setup();
        h.add_file("pom.xml", &text);
        answer(&h, true);
        let mut state = State::default();
        state.import_at(&h.invocation(), "/m2").unwrap();
        assert!(state.provenance("/project/application.properties").unwrap().contains("org.example:config:1.2.3"));
        assert!(h.requests().is_empty() && h.process_runs().is_empty());
    }
    #[test]
    fn malformed_or_coordinate_embedded_comments_do_not_authorize_archive() {
        for text in [
            format!("bad{POM}"),
            format!("{POM}bad"),
            format!("<!-- bad--comment -->{POM}"),
            format!("<!-- unterminated {POM}"),
            format!("<!DOCTYPE project SYSTEM 'https://example.invalid/pom.dtd'>{POM}"),
            POM.replace("<groupId>org.example</groupId>", "<groupId><![CDATA[org.example]]></groupId>"),
        ] {
            assert!(dependencies(&text).is_err(), "{text}");
        }
        for value in ["org.<!--comment-->example", "org.<?p x?>example", "org.&#101;xample"] {
            assert!(dependencies(&POM.replace("org.example", value)).unwrap().is_empty());
        }
    }
    #[test]
    fn literal_direct_root_dependencies_only_no_profiles_bom_properties_or_path_injection() {
        let d = dependencies(POM).unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(
            d[0].path(CACHE),
            "~/.m2/repository/org/example/config/1.2.3/config-1.2.3.jar"
        );
        for replacement in ["${version}", "../escape", "[1,2)", ".."] {
            assert!(
                dependencies(&POM.replace("1.2.3", replacement))
                    .unwrap()
                    .is_empty()
            );
        }
        assert!(
            dependencies(&POM.replace(
                "<version>1.2.3</version>",
                "<version>1.2.3</version><classifier>sources</classifier>"
            ))
            .unwrap()
            .is_empty()
        );
        assert!(
            dependencies(
                &POM.replace("<dependencies>", "<profiles><profile><dependencies>")
                    .replace("</dependencies>", "</dependencies></profile></profiles>")
            )
            .unwrap()
            .is_empty()
        );
        assert!(dependencies(&POM.replace("</project>","<dependencies><dependency><groupId>org.example</groupId><artifactId>config</artifactId><version>2</version></dependency></dependencies></project>")).is_err());
        assert!(dependencies(&POM.replace("</dependencies>", "")).is_err());
        assert!(
            dependencies(&POM.replace("http://maven.apache.org/POM/4.0.0", "urn:foreign")).is_err()
        );
    }
    #[test]
    fn explicit_import_is_root_owned_and_supplies_hover_provenance_without_requests() {
        let h = setup();
        answer(&h, true);
        let mut state = State::default();
        state.import_at(&h.invocation(), "/m2").unwrap();
        let catalog = state.catalog("/project/application.properties");
        assert!(
            spring_text::metadata::Lookup(vec![&catalog])
                .prop("custom.jar.feature")
                .is_some()
        );
        let other = state.catalog("/another/application.properties");
        assert!(
            spring_text::metadata::Lookup(vec![&other])
                .prop("custom.jar.feature")
                .is_none()
        );
        let mut plugin = crate::BootTools {
            jars: state,
            ..Default::default()
        };
        let doc = Document {
            path: "/project/application.properties".into(),
            language: "properties".into(),
            text: "custom.jar.feature=true".into(),
            version: 3,
        };
        let hover = plugin
            .hover(&doc, openidea_plugin::Position::new(0, 5))
            .unwrap();
        assert!(
            hover
                .markdown
                .contains("Selected Maven dependency snapshot")
                && hover.markdown.contains("SHA-256")
                && hover.markdown.contains("classpath is unverified")
        );
        assert!(h.requests().is_empty() && h.process_runs().is_empty());
    }
    #[test]
    fn cancelled_denied_missing_and_failed_reimport_keep_the_previous_snapshot() {
        let h = setup();
        let mut state = State::default();
        answer(&h, true);
        state.import_at(&h.invocation(), "/m2").unwrap();
        let previous = state.provenance("/project/application.properties").unwrap();
        answer(&h, false);
        state.import_at(&h.invocation(), "/m2").unwrap();
        assert_eq!(
            state.provenance("/project/application.properties").unwrap(),
            previous
        );
        host::fs::write_bytes(PATH, b"invalid archive").unwrap();
        answer(&h, true);
        assert!(state.import_at(&h.invocation(), "/m2").is_err());
        assert_eq!(
            state.provenance("/project/application.properties").unwrap(),
            previous
        );
        let h = h.with_capabilities(&["project.read"]);
        h.answer_next_prompt(Some(&["0"]));
        assert!(matches!(
            state.import_at(&h.invocation(), "/m2"),
            Err(Error::Denied(_))
        ));
        assert_eq!(
            state.provenance("/project/application.properties").unwrap(),
            previous
        );
        assert!(h.requests().is_empty() && h.process_runs().is_empty());
    }
    #[test]
    fn current_unsaved_pom_drives_selection_and_events_expiry_remove_facts() {
        let h = setup();
        h.open_editor(
            "/project/pom.xml",
            "xml",
            &POM.replace("<version>1.2.3</version>", "<version>${unsaved}</version>"),
            &[],
        );
        assert!(State::default().import_at(&h.invocation(), "/m2").is_err());
        h.open_editor("/project/pom.xml", "xml", POM, &[]);
        answer(&h, true);
        let mut state = State::default();
        state.import_at(&h.invocation(), "/m2").unwrap();
        state.snapshots.get_mut("/project").unwrap().at = Instant::now() - Duration::from_secs(600);
        assert!(
            state
                .provenance("/project/application.properties")
                .is_none()
        );
        answer(&h, true);
        state.import_at(&h.invocation(), "/m2").unwrap();
        state.invalidate("/other/pom.xml");
        assert!(
            state
                .provenance("/project/application.properties")
                .is_some()
        );
        state.invalidate("/project/pom.xml");
        assert!(
            state
                .provenance("/project/application.properties")
                .is_none()
        );
    }
    #[test]
    fn ambiguous_or_oversized_metadata_is_not_silently_merged() {
        let entry = |text: &str| {
            vec![(
                "META-INF/spring-configuration-metadata.json".into(),
                text.into(),
            )]
        };
        assert!(parse_entries(&entry(JSON)).is_ok());
        assert!(parse_entries(&entry("{bad")).is_err());
        assert!(parse_entries(&entry(r#"{"properties":{}}"#)).is_err());
        assert!(
            parse_entries(&entry(r#"{"properties":[{"name":"a-b"},{"name":"a_b"}]}"#)).is_err()
        );
        assert!(
            parse_entries(&entry(
                &JSON.replace("Selected archive fact.", &"x".repeat(512 * 1024))
            ))
            .is_err()
        );
        assert!(
            parse_entries(&entry(
                r#"{"properties":[{"name":"a","defaultValue":"line\nbreak"}]}"#
            ))
            .is_err()
        );
    }
    #[test]
    fn explicit_observed_pom_changes_do_not_reuse_last_good_from_another_source_scope() {
        let h = setup();
        let mut state = State::default();
        answer(&h, true);
        state.import_at(&h.invocation(), "/m2").unwrap();
        assert!(
            state
                .provenance("/project/application.properties")
                .is_some()
        );
        h.add_file("pom.xml", &POM.replace("1.2.3", "${new.version}"));
        assert!(state.import_at(&h.invocation(), "/m2").is_err());
        assert!(
            state
                .provenance("/project/application.properties")
                .is_none()
        );
    }
    #[test]
    fn malformed_namespace_source_cannot_authorize_selected_metadata_archive() {
        for binding in [
            "xmlns:xml=\"urn:foreign\"",
            "xmlns:xmlns=\"urn:foreign\"",
            "xmlns:a=\"http://www.w3.org/XML/1998/namespace\"",
            "bad:flag=\"x\"",
        ] {
            let h = setup();
            h.add_file(
                "pom.xml",
                &POM.replacen("<project ", &format!("<project {binding} "), 1),
            );
            answer(&h, true);
            let mut state = State::default();
            assert!(
                state.import_at(&h.invocation(), "/m2").is_err(),
                "{binding}"
            );
            assert!(state.snapshots.is_empty());
        }
    }

    #[test]
    fn coherent_maven_prefixes_and_legal_xml_attributes_preserve_source_contract() {
        let legal = POM.replacen(
            "<project ",
            "<project xmlns:xml=\"http://www.w3.org/XML/1998/namespace\" xml:lang=\"en\" ",
            1,
        );
        assert_eq!(dependencies(&legal).unwrap().len(), 1);
        let prefixed = "<p:project xmlns:p=\"http://maven.apache.org/POM/4.0.0\"><p:dependencies><p:dependency><p:groupId>org.example</p:groupId><p:artifactId>config</p:artifactId><p:version>1.2.3</p:version></p:dependency></p:dependencies></p:project>";
        assert_eq!(dependencies(prefixed).unwrap().len(), 1);
        for text in [
            POM.replace("<dependencies>", "<dependencies xmlns=\"\">"),
            POM.replace(
                "<project ",
                "<p:q:project xmlns:p=\"http://maven.apache.org/POM/4.0.0\" ",
            )
            .replace("</project>", "</p:q:project>"),
            POM.replace("<project ", "<project a:b:c=\"x\" "),
        ] {
            assert!(dependencies(&text).is_err());
        }
    }
    #[test]
    fn public_hover_reports_actual_selected_origin_and_preserves_precedence_without_runtime_values()
    {
        let h = setup();
        answer(&h, true);
        let mut state = State::default();
        state.import_at(&h.invocation(), "/m2").unwrap();
        let mut tools = crate::BootTools {
            jars: state,
            ..Default::default()
        };
        let doc = |text: &str| Document {
            path: "/project/application.properties".into(),
            language: "properties".into(),
            version: 1,
            text: text.into(),
        };
        let bundled = tools
            .hover(
                &doc("server.port=4321"),
                openidea_plugin::Position::new(0, 3),
            )
            .unwrap()
            .markdown;
        assert!(
            bundled.contains("Metadata origin: bundled fallback snapshot")
                && bundled.contains("Spring Boot 3.5.16")
                && bundled.contains("not an inferred project dependency version")
        );
        assert!(!bundled.contains("4321"));
        let selected = tools
            .hover(
                &doc("custom.jar.feature=configured-sensitive"),
                openidea_plugin::Position::new(0, 4),
            )
            .unwrap()
            .markdown;
        assert!(
            selected.contains("Metadata origin: selected dependency JAR snapshot")
                && selected.contains("org.example:config:1.2.3")
                && selected.contains("SHA-256")
        );
        assert!(!selected.contains("configured-sensitive"));
        h.add_file("src/main/resources/META-INF/spring-configuration-metadata.json",r#"{"properties":[{"name":"custom.jar.feature","type":"java.lang.String","defaultValue":"declared-local"}]}"#);
        let current = doc("custom.jar.feature=configured-sensitive");
        tools.warm_live(&current);
        let local = tools
            .hover(&current, openidea_plugin::Position::new(0, 4))
            .unwrap()
            .markdown;
        assert!(
            local.contains("Metadata origin: project-local current/cached declarations")
                && local.contains("declared-local")
        );
        assert!(
            !local.contains("Selected Maven dependency snapshot")
                && !local.contains("configured-sensitive")
        );
        assert!(h.requests().is_empty() && h.process_runs().is_empty());
    }
}
