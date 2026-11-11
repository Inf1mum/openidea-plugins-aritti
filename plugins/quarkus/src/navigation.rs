//! Read-only configuration navigation; no synthetic or transformed Rename anchors.
use super::*;
use openidea_plugin::Location;
use spring_text::java as source;
#[derive(Clone)]
pub(crate) struct Target {
    pub key: String,
    pub range: Range,
}
pub(crate) fn literal_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 256
        && key.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')))
}
pub(crate) fn config_targets(path: &str, text: &str) -> Vec<Target> {
    if !is_quarkus_config(path) || text.len() > live::MAX_TEXT {
        return vec![];
    }
    let keys = config::keys(path, text);
    if keys.len() > 512 {
        return vec![];
    }
    keys.iter()
        .filter(|k| !keys.iter().any(|child| child.key.len() > k.key.len() && child.key.starts_with(&format!("{}.", k.key))))
        .filter(|k| literal_key(&k.key) && k.at < k.end && k.end <= text.len())
        .map(|k| Target { key: k.key.clone(), range: range(text, k.at, k.end) })
        .collect()
}
pub(crate) fn java_targets(path: &str, text: &str) -> Vec<Target> {
    if !path.ends_with(".java") || text.len() > live::MAX_TEXT {
        return vec![];
    }
    let tokens = source::tokens(text);
    let mut stack = Vec::new();
    for t in &tokens {
        match t.s {
            "{" | "(" | "[" => {
                if stack.len() >= 128 {
                    return vec![];
                }
                stack.push(t.s);
            }
            "}" | ")" | "]" => {
                let expected = match t.s {
                    "}" => "{",
                    ")" => "(",
                    _ => "[",
                };
                if stack.pop() != Some(expected) {
                    return vec![];
                }
            }
            _ => {}
        }
    }
    if !stack.is_empty() {
        return vec![];
    }
    let file = source::parse(text);
    let qualified = "org.eclipse.microprofile.config.inject.ConfigProperty";
    if file.types.len() > 64
        || file.types.iter().any(|t| t.name == "ConfigProperty")
        || !file.imports.iter().any(|i| !i.is_static && i.path == qualified)
        || file.imports.iter().any(|i| !i.is_static && i.path.rsplit('.').next() == Some("ConfigProperty") && i.path != qualified)
    {
        return vec![];
    }
    // Only direct literal injection names. ConfigMapping method/group names are
    // derived, not source keys; their owner/path model is not established here.
    let mut targets = Vec::new();
    for t in &file.types {
        let generic =
            source::tokens(&text[t.at + t.name.len()..t.end]).iter().take_while(|t| t.s != "{").any(|t| t.s == "<" && t.at < t.end());
        if generic {
            continue;
        }
        for m in &t.members {
            if let Some(a) = m.annotation("ConfigProperty") {
                if let Some(target) = annotation_target(a, text) {
                    targets.push(target);
                }
            }
            for p in &m.params {
                if let Some(a) = p.annotation("ConfigProperty") {
                    if let Some(target) = annotation_target(a, text) {
                        targets.push(target);
                    }
                }
            }
        }
        for c in &t.components {
            if let Some(a) = c.annotation("ConfigProperty") {
                if let Some(target) = annotation_target(a, text) {
                    targets.push(target);
                }
            }
        }
    }
    if targets.len() > 512 { vec![] } else { targets }
}
fn annotation_target(a: &source::Annotation, text: &str) -> Option<Target> {
    let written: String = source::tokens(text.get(a.at + 1..a.name_at + a.name.len())?).iter().map(|t| t.s).collect();
    if written != "ConfigProperty" {
        return None;
    }
    let arg = a.arg("name")?;
    let [literal] = arg.strings.as_slice() else {
        return None;
    };
    if arg.text.trim() != format!("\"{}\"", literal.value)
        || literal.value.len() != literal.end - literal.at
        || !literal_key(&literal.value)
    {
        return None;
    }
    Some(Target { key: literal.value.clone(), range: range(text, literal.at, literal.end) })
}
impl QuarkusTools {
    pub(crate) fn source_definition(&mut self, doc: &Document, at: Position) -> Vec<Location> {
        let java = doc.language == "java" && doc.path.ends_with(".java");
        let config = matches!(doc.language.as_str(), "properties" | "quarkus-properties" | "yaml") && is_quarkus_config(&doc.path);
        if !(java || config) || doc.path.split('/').any(|p| matches!(p, "." | "..")) {
            return vec![];
        }
        // Observe even unsupported/oversized current source before preflight:
        // a rejected new document must not leave its previous destination alive.
        let _ = self.sources.for_cached_document(&doc.path, &doc.text);
        let _ = self.configs.for_cached_document(&doc.path, &doc.text);
        let Some(root) = self.cached_module(doc) else {
            return vec![];
        };
        if !self.live.watching {
            return vec![];
        }
        let candidates = if java {
            java_targets(&doc.path, &doc.text)
        } else if config {
            config_targets(&doc.path, &doc.text)
        } else {
            return vec![];
        };
        let Some(selected) = candidates.iter().find(|t| t.range.start <= at && at < t.range.end) else {
            return vec![];
        };
        let Some(module) = module(&doc.path) else {
            return vec![];
        };
        let mut locations = Vec::new();
        let mut records = 0usize;
        if java {
            let Some(index) = self.configs.for_cached_document(&doc.path, &doc.text) else {
                return vec![];
            };
            if !index.fresh() || index.iter().take(513).count() > 512 || index.iter().any(|(_, i)| !i.complete) {
                return vec![];
            }
            for (path, info) in index.iter() {
                records += info.locations.len();
                if records > 4096 {
                    return vec![];
                }
                if !owned(&root, path) || self::module(path).as_deref() != Some(module.as_str()) {
                    continue;
                }
                for target in &info.locations {
                    if target.key == selected.key {
                        locations.push(Location { path: path.into(), range: target.range });
                    }
                }
                if locations.len() > 1 {
                    return vec![];
                }
            }
        } else {
            let Some(index) = self.sources.for_cached_document(&doc.path, &doc.text) else {
                return vec![];
            };
            if !index.fresh() || index.iter().take(513).count() > 512 || index.iter().any(|(_, i)| !i.complete) {
                return vec![];
            }
            for (path, info) in index.iter() {
                records += info.locations.len();
                if records > 4096 {
                    return vec![];
                }
                if !owned(&root, path) || self::module(path).as_deref() != Some(module.as_str()) {
                    continue;
                }
                for target in &info.locations {
                    if target.key == selected.key {
                        locations.push(Location { path: path.into(), range: target.range });
                    }
                }
                if locations.len() > 1 {
                    return vec![];
                }
            }
        }
        locations
    }
}
fn owned(root: &str, path: &str) -> bool {
    crate::source_owned(root, path)
}

pub(crate) fn module(path: &str) -> Option<String> {
    ["/src/main/java/", "/src/main/resources/"].iter().find_map(|segment| path.split_once(segment).map(|(base, _)| base.to_owned()))
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{Event, FilesChanged, mock::MockHost};
    const JAVA_PATH: &str = "/project/src/main/java/app/Greeting.java";
    const CONFIG_PATH: &str = "/project/src/main/resources/application.properties";
    const JAVA: &str = "package app;\nimport org.eclipse.microprofile.config.inject.ConfigProperty;\nclass Greeting {\n/* 😀 */ @ConfigProperty(name=\"greeting.message\") String message;\n}";
    fn doc(path: &str, text: &str) -> Document {
        Document {
            path: path.into(),
            language: if path.ends_with(".java") {
                "java"
            } else if path.ends_with(".yaml") {
                "yaml"
            } else {
                "properties"
            }
            .into(),
            version: 1,
            text: text.into(),
        }
    }
    fn at(d: &Document, needle: &str) -> Position {
        range_point(&d.text, d.text.find(needle).unwrap())
    }
    fn setup() -> (MockHost, QuarkusTools) {
        let h = MockHost::install();
        h.add_file("pom.xml", "<project><dependencies><dependency><groupId>io.quarkus</groupId><artifactId>quarkus-rest</artifactId></dependency></dependencies></project>");
        h.add_file(JAVA_PATH, JAVA);
        h.add_file(CONFIG_PATH, "greeting.message=hello\n");
        let mut p = QuarkusTools::default();
        p.activate_live();
        (h, p)
    }
    #[test]
    fn exact_source_navigation_is_bidirectional_with_utf16_tokens() {
        let (_h, mut p) = setup();
        let j = doc(JAVA_PATH, JAVA);
        let c = doc(CONFIG_PATH, "greeting.message=hello\n");
        let found = p.definition(&c, at(&c, "greeting.message"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, JAVA_PATH);
        let byte = JAVA.find("greeting.message").unwrap();
        assert_eq!(found[0].range, range(JAVA, byte, byte + 16));
        let back = p.definition(&j, at(&j, "greeting.message"));
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].path, CONFIG_PATH);
        assert_eq!(back[0].range, range(&c.text, 0, 16));
    }
    #[test]
    fn yaml_leaf_navigation_preserves_composite_owner_without_parent_or_rename() {
        let (_h, mut p) = setup();
        let d = doc("/project/src/main/resources/application.yaml", "greeting:\n  message: hello\n");
        let targets = config_targets(&d.path, &d.text);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].key, "greeting.message");
        assert_eq!(targets[0].range.start, Position::new(1, 2));
        assert_eq!(p.definition(&d, Position::new(1, 2)).len(), 1);
        assert!(p.definition(&d, Position::new(0, 2)).is_empty());
    }
    #[test]
    fn stale_module_proof_suppresses_navigation_even_with_fresh_source_facts() {
        let (_h, mut p) = setup();
        let c = doc(CONFIG_PATH, "greeting.message=x\n");
        assert_eq!(p.definition(&c, at(&c, "greeting.message")).len(), 1);
        for proof in p.modules.values_mut() {
            proof.at = std::time::Instant::now() - live::MODULE_TTL - std::time::Duration::from_secs(1);
        }
        assert!(p.sources.for_cached_document(&c.path, &c.text).unwrap().fresh());
        assert!(p.definition(&c, at(&c, "greeting.message")).is_empty());
        assert!(p.modules.values().all(|proof| proof.at.elapsed() > live::MODULE_TTL));
    }
    #[test]
    fn cold_provider_does_not_discover_and_late_files_are_not_loaded() {
        let (h, _p) = setup();
        let mut p = QuarkusTools::default();
        let c = doc(CONFIG_PATH, "greeting.message=x\n");
        assert!(p.definition(&c, at(&c, "greeting.message")).is_empty());
        p.activate_live();
        assert_eq!(p.definition(&c, at(&c, "greeting.message")).len(), 1);
        h.add_file("/project/src/main/java/app/Late.java", &JAVA.replace("greeting.message", "late.key"));
        let c = doc(CONFIG_PATH, "late.key=x\n");
        assert!(p.definition(&c, at(&c, "late.key")).is_empty());
    }
    #[test]
    fn unsaved_current_java_source_replaces_cached_destination() {
        let (_h, mut p) = setup();
        let j = doc(JAVA_PATH, &JAVA.replace("greeting.message", "renamed.message"));
        p.definition(&j, at(&j, "renamed.message"));
        let old = doc(CONFIG_PATH, "greeting.message=x\n");
        assert!(p.definition(&old, at(&old, "greeting.message")).is_empty());
        let new = doc(CONFIG_PATH, "renamed.message=x\n");
        assert_eq!(p.definition(&new, at(&new, "renamed.message")).len(), 1);
    }
    #[test]
    fn observed_oversized_source_invalidates_previous_navigation_fact() {
        let (_h, mut p) = setup();
        let c = doc(CONFIG_PATH, "greeting.message=x\n");
        assert_eq!(p.definition(&c, at(&c, "greeting.message")).len(), 1);
        let oversized = doc(JAVA_PATH, &format!("{JAVA}{}", " ".repeat(live::MAX_TEXT)));
        assert!(p.definition(&oversized, Position::new(0, 0)).is_empty());
        assert!(p.definition(&c, at(&c, "greeting.message")).is_empty());
    }
    #[test]
    fn duplicate_targets_and_foreign_modules_are_not_guessed() {
        let (h, mut p) = setup();
        h.add_file("src/main/java/app/Duplicate.java", &JAVA.replace("Greeting", "Duplicate"));
        p.live_events(&[Event::FileSaved("/project/src/main/java/app/Duplicate.java".into())]);
        let c = doc(CONFIG_PATH, "greeting.message=x\n");
        assert!(p.definition(&c, at(&c, "greeting.message")).is_empty());
        let foreign = doc("/project/child/src/main/resources/application.properties", "greeting.message=x\n");
        assert!(p.definition(&foreign, at(&foreign, "greeting.message")).is_empty());
    }
    #[test]
    fn source_scope_denial_switch_removal_and_events_are_conservative() {
        let (h, mut p) = setup();
        let c = doc(CONFIG_PATH, "greeting.message=x\n");
        h.set_project_root(Some("/other"));
        assert!(p.definition(&c, at(&c, "greeting.message")).is_empty());
        h.set_project_root(Some("/project"));
        p.live_events(&[Event::FilesChanged(FilesChanged { created: vec![], modified: vec![], removed: vec![JAVA_PATH.into()] })]);
        assert!(p.definition(&c, at(&c, "greeting.message")).is_empty());
        let _h = h.with_capabilities(&[]);
        assert!(p.definition(&c, at(&c, "greeting.message")).is_empty());
    }
    #[test]
    fn only_literal_owned_configproperty_injection_targets_are_retained() {
        assert_eq!(java_targets(JAVA_PATH, JAVA).len(), 1);
        for text in [
            JAVA.replace("import org.eclipse.microprofile.config.inject.ConfigProperty;", ""),
            JAVA.replace("org.eclipse.microprofile.config.inject.ConfigProperty", "custom.ConfigProperty"),
            JAVA.replace("class Greeting", "class ConfigProperty {} class Greeting"),
            JAVA.replace("\"greeting.message\"", "\"greeting.\" + NAME"),
            JAVA.replace("\"greeting.message\"", "\"greeting.\\u006dessage\""),
            JAVA.replace("class Greeting", "class Greeting<T>"),
            JAVA.trim_end_matches('}').into(),
            format!("{JAVA}{}", " ".repeat(live::MAX_TEXT)),
        ] {
            assert!(java_targets(JAVA_PATH, &text).is_empty(), "{text}");
        }
        assert!(java_targets(JAVA_PATH, "@ConfigMapping(prefix=\"server\") interface S {String hostName();}").is_empty());
    }
    #[test]
    fn profiles_named_placeholders_and_qute_expressions_do_not_infer_owners() {
        let (_h, mut p) = setup();
        for text in ["%dev.greeting.message=x\n", "quarkus.datasource.\"named\".jdbc.url=x\n", "${greeting.message}=x\n"] {
            let d = doc(CONFIG_PATH, text);
            assert!(p.definition(&d, Position::new(0, 1)).is_empty());
        }
        let d = doc("/project/src/main/resources/templates/hello.html", "{@app.Greeting greeting} {greeting.message}");
        assert!(p.definition(&d, Position::new(0, 5)).is_empty());
    }
}
