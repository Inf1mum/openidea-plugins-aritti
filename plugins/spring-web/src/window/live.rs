//! The registered wrapper owns one merged, current-text diagnostic response.
use super::*;
use crate::*;
use openidea_plugin::diagnostics::DiagnosticsProvider;
use openidea_plugin::hover::{Hover, HoverProvider};
use openidea_plugin::{Diagnostic, EventKind};
const MAX_TEXT: usize = 128 << 10;
const GLOBS: &[&str] = &[
    "**/*.java",
    "**/*.kt",
    "**/*.xml",
    "**/*.graphql",
    "**/*.graphqls",
    "**/*.properties",
    "**/*.yaml",
    "**/*.yml",
    "**/pom.xml",
    "**/*.gradle",
    "**/*.gradle.kts",
    "**/*.toml",
];
fn bounded(path: &str, text: &str) -> Option<bool> {
    Some(
        text.len() <= MAX_TEXT
            && (![".java", ".kt", ".graphql", ".graphqls"].iter().any(|extension| path.ends_with(extension)) || balanced(text)),
    )
}
pub(super) fn balanced(text: &str) -> bool {
    let mut stack = Vec::new();
    for token in java::tokens(text) {
        if token.s.starts_with('"') && (token.s.len() < 2 || !token.s.ends_with('"')) {
            return false;
        }
        match token.s {
            "(" => stack.push(")"),
            "[" => stack.push("]"),
            "{" => stack.push("}"),
            ")" | "]" | "}" => {
                if stack.pop() != Some(token.s) {
                    return false;
                }
            }
            _ => {}
        }
    }
    stack.is_empty()
}
pub(super) struct Live {
    bounds: Indexes<bool>,
    watching: bool,
}
impl Default for Live {
    fn default() -> Self {
        Self { bounds: Indexes::new(GLOBS, bounded), watching: false }
    }
}
impl Endpoints {
    fn cached_applies(&self, doc: &Document) -> Applies {
        project_of(&doc.path)
            .and_then(|p| p.root().ok())
            .and_then(|root| self.applies.get(&root))
            .filter(|(at, _)| self.live.watching || at.elapsed() < APPLIES_TTL)
            .map(|(_, a)| *a)
            .unwrap_or_default()
    }
    fn warm_live(&mut self, project: &Project, budget: Duration) {
        let deadline = Instant::now() + budget;
        let root = project.root().unwrap_or_default();
        if root.is_empty() {
            return;
        }
        let doc = Document { path: format!("{root}/__live.java"), language: "java".into(), version: 0, text: String::new() };
        let applies = self.applicable(&doc);
        if !(applies.web || applies.websocket || applies.messaging || applies.graphql || applies.webflow || applies.cloud) {
            return;
        }
        let part = || deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(120));
        self.live.bounds.get(project, part());
        if applies.web {
            self.web.versioning.get(project, part());
        }
        if applies.websocket {
            self.websocket.live_warm(project, part());
        }
        if applies.messaging {
            self.messaging.live_warm(project, part());
        }
        if applies.graphql {
            self.graphql.live_warm(project, part());
        }
        if applies.webflow {
            self.webflow.live_warm(project, part());
        }
        if applies.cloud {
            self.cloud.live_warm(project, part());
        }
    }
    pub(super) fn activate_live(&mut self) {
        self.live.watching = host::subscribe_paths("files-changed", GLOBS).is_ok();
        let _ = host::subscribe(EventKind::FileOpened);
        let _ = host::subscribe(EventKind::FileSaved);
        let deadline = Instant::now() + Duration::from_secs(3);
        for project in host::projects().unwrap_or_default() {
            let budget = deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(600));
            if budget.is_zero() {
                break;
            }
            self.warm_live(&project, budget);
        }
    }
    pub(super) fn live_events(&mut self, events: &[Event]) {
        let deadline = Instant::now() + Duration::from_secs(3);
        for event in events {
            let paths: Vec<_> = match event {
                Event::FileOpened(p) | Event::FileSaved(p) => vec![(p, false)],
                Event::FilesChanged(f) => {
                    f.removed.iter().map(|p| (p, true)).chain(f.created.iter().chain(&f.modified).map(|p| (p, false))).collect()
                }
                _ => Vec::new(),
            };
            for (path, removed) in paths {
                if Instant::now() >= deadline {
                    return;
                }
                if !GLOBS.iter().any(|g| spring_text::index::glob_matches(g, path)) {
                    continue;
                }
                let Some(project) = project_of(path) else {
                    continue;
                };
                let text = if removed {
                    None
                } else {
                    match project.read_file(path) {
                        Ok(text) => Some(text),
                        Err(_) => {
                            self.live.bounds.reset();
                            self.web.versioning.reset();
                            self.websocket.live_reset();
                            self.messaging.live_reset();
                            self.graphql.live_reset();
                            self.webflow.live_reset();
                            self.cloud.live_reset();
                            continue;
                        }
                    }
                };
                self.live.bounds.update_existing(&project, path, text.as_deref());
                self.web.versioning.update_existing(&project, path, text.as_deref());
                self.websocket.live_update(&project, path, text.as_deref());
                self.messaging.live_update(&project, path, text.as_deref());
                self.graphql.live_update(&project, path, text.as_deref());
                self.webflow.live_update(&project, path, text.as_deref());
                self.cloud.live_update(&project, path, text.as_deref());
                if path.ends_with("pom.xml") || path.ends_with(".gradle") || path.ends_with(".gradle.kts") {
                    if let Ok(root) = project.root() {
                        self.applies.remove(&root);
                    }
                }
                self.warm_live(&project, deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(600)));
            }
        }
    }
    fn allow_absence(&mut self, doc: &Document) -> bool {
        self.live.watching
            && self.live.bounds.for_cached_document(&doc.path, &doc.text).is_some_and(|i| i.fresh() && i.iter().all(|(_, ok)| *ok))
    }
}
impl DiagnosticsProvider for Endpoints {
    fn diagnostics(&mut self, doc: &Document) -> Vec<Diagnostic> {
        if doc.text.len() > MAX_TEXT || matches!(doc.language.as_str(), "java" | "kotlin" | "graphql") && !balanced(&doc.text) {
            return Vec::new();
        }
        let applicable = self.cached_applies(doc);
        if !(applicable.web || applicable.websocket || applicable.messaging || applicable.graphql || applicable.webflow || applicable.cloud)
        {
            return Vec::new();
        }
        if applicable.web {
            let _ = self.web.versioning.for_cached_document(&doc.path, &doc.text);
        }
        let complete = self.allow_absence(doc);
        let mut out = Vec::new();
        if applicable.web && WEB_LANGUAGES.contains(&doc.language.as_str()) {
            if is_xml(doc) {
                out.extend(xml_checks::check(&doc.text).iter().map(|f| f.diagnostic(&doc.text)));
            } else {
                out.extend(problems(doc).into_iter().map(|p| p.diagnostic));
                let (configured, standard, inserter_missing) = match self.web.versioning.for_cached_document(&doc.path, &doc.text) {
                    Some(index) if complete && index.fresh() => {
                        let files: Vec<_> = index.iter().map(|(_, f)| *f).collect();
                        (
                            checks::project_versioning(true, &files),
                            checks::project_standard_parser(true, &files),
                            Some(!files.iter().any(|f| f.inserter || f.is_config_file && f.configured)),
                        )
                    }
                    _ => (None, None, None),
                };
                out.extend(
                    checks::check_java(&parse_source(&doc.path, &doc.text), configured, standard, checks::Groups::from_settings())
                        .iter()
                        .map(|f| f.diagnostic(&doc.text)),
                );
                if inserter_missing == Some(true) && doc.path.contains("/src/test/") && doc.text.contains("apiVersion(") {
                    out.extend(source_calls(&doc.path,&doc.text).into_iter().filter(|c|c.name=="apiVersion" && !c.is_new).map(|c|Diagnostic::new(range(&doc.text,c.at,c.at+c.name.len()),Severity::Hint,"Cached source declares no ApiVersionInserter or spring.mvc.apiversion.* configuration; this test version may not reach the request.").code("api-version-without-inserter")));
                }
            }
        }
        if applicable.websocket && WEBSOCKET_LANGUAGES.contains(&doc.language.as_str()) {
            out.extend(self.websocket.live_diagnostics(doc, complete));
        }
        if applicable.messaging && MESSAGING_LANGUAGES.contains(&doc.language.as_str()) {
            out.extend(self.messaging.live_diagnostics(doc, complete));
        }
        if applicable.graphql && GRAPHQL_LANGUAGES.contains(&doc.language.as_str()) {
            out.extend(self.graphql.live_diagnostics(doc, complete));
        }
        if applicable.webflow && WEBFLOW_LANGUAGES.contains(&doc.language.as_str()) {
            out.extend(self.webflow.live_diagnostics(doc, complete));
        }
        if applicable.cloud && CLOUD_LANGUAGES.contains(&doc.language.as_str()) {
            out.extend(self.cloud.live_diagnostics(doc, complete));
        }
        out.truncate(500);
        out
    }
}
pub(super) fn imported(file: &JavaFile, name: &str) -> bool {
    let expected = if matches!(name, "Controller" | "RestController") {
        if name == "Controller" {
            "org.springframework.stereotype.Controller".into()
        } else {
            "org.springframework.web.bind.annotation.RestController".into()
        }
    } else {
        format!("org.springframework.web.bind.annotation.{name}")
    };
    !file.types.iter().any(|t| t.name == name)
        && file.imports.iter().any(|i| !i.is_static && i.path == expected)
        && !file.imports.iter().any(|i| !i.is_static && i.path.rsplit('.').next() == Some(name) && i.path != expected)
}
fn path_hover(doc: &Document, at: Position) -> Option<Hover> {
    if !matches!(doc.language.as_str(), "java" | "kotlin") {
        return None;
    }
    let off = offset(&doc.text, at)?;
    let file = parse_source(&doc.path, &doc.text);
    if !imported(&file, "PathVariable") {
        return None;
    }
    for class in file
        .types
        .iter()
        .filter(|t| t.annotations.iter().any(|a| matches!(a.name.as_str(), "Controller" | "RestController") && imported(&file, &a.name)))
    {
        if class.annotation("RequestMapping").is_some_and(|a| !literal_paths(a) || !imported(&file, "RequestMapping")) {
            continue;
        }
        for method in class.methods() {
            let maps: Vec<_> = method.annotations.iter().filter(|a| MAPPINGS.contains(&a.name.as_str())).collect();
            if maps.is_empty() || !maps.iter().all(|a| literal_paths(a) && imported(&file, &a.name)) {
                continue;
            }
            let paths = java::mappings(class, method);
            if paths.len() != 1 {
                continue;
            }
            let path = &paths[0].1;
            for param in &method.params {
                let Some(annotation) = param.annotation("PathVariable") else {
                    continue;
                };
                let name = match annotation.args.iter().find(|a| matches!(a.name.as_str(), "name" | "value")) {
                    None => param.name.clone(),
                    Some(a) if a.strings.len() == 1 && java::tokens(&a.text).len() == 1 => a.strings[0].value.clone(),
                    _ => continue,
                };
                if variables(path).iter().filter(|v| v.0 == name).count() != 1
                    || name.len() > 128
                    || name.contains('`')
                    || name.chars().any(char::is_control)
                    || param.ty.text.len() > 128
                    || param.ty.text.contains('`')
                    || param.ty.text.chars().any(char::is_control)
                {
                    continue;
                }
                let bindings = method
                    .params
                    .iter()
                    .filter_map(|p| {
                        p.annotation("PathVariable").map(|a| {
                            match a.args.iter().find(|arg| matches!(arg.name.as_str(), "name" | "value")) {
                                None => p.name.clone(),
                                Some(arg) if arg.strings.len() == 1 && java::tokens(&arg.text).len() == 1 => arg.strings[0].value.clone(),
                                _ => String::new(),
                            }
                        })
                    })
                    .filter(|bound| bound == &name)
                    .count();
                if bindings != 1 {
                    continue;
                }
                let parameter_hit = param.at <= off && off <= param.at + param.name.len() || annotation.at <= off && off <= annotation.end;
                let literal_hit = maps.iter().flat_map(|a| a.strings(&["value", "path"])).find_map(|s| {
                    variables(&doc.text[s.at..s.end])
                        .into_iter()
                        .find(|(n, from, to)| *n == name && s.at + *from <= off && off <= s.at + *to)
                        .map(|(_, from, to)| range(&doc.text, s.at + from, s.at + to))
                });
                if parameter_hit || literal_hit.is_some() {
                    return Some(Hover::new(format!("Path variable `{name}` binds parameter `{}` in this mapping.\n\nDeclared parameter type: `{}`.\n\nOne literal source route; runtime conversion and handler selection are not inferred.",param.name,param.ty.text)).range(literal_hit.unwrap_or_else(||if param.at<=off && off<=param.at+param.name.len(){range(&doc.text,param.at,param.at+param.name.len())}else{range(&doc.text,annotation.at,annotation.end)})));
                }
            }
        }
    }
    None
}
impl HoverProvider for Endpoints {
    fn hover(&mut self, doc: &Document, at: Position) -> Option<Hover> {
        if doc.text.len() > MAX_TEXT || matches!(doc.language.as_str(), "java" | "kotlin" | "graphql") && !balanced(&doc.text) {
            return None;
        }
        let applies = self.cached_applies(doc);
        if applies.web
            && let Some(hover) = path_hover(doc, at)
        {
            return Some(hover);
        }
        if applies.graphql
            && let Some(hover) = self.graphql.live_hover(doc, at)
        {
            return Some(hover);
        }
        if applies.messaging {
            return self.messaging.live_hover(doc, at);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const POM: &str = "<project><artifactId>spring-boot-starter-web</artifactId><artifactId>spring-boot-starter-graphql</artifactId><artifactId>spring-jms</artifactId></project>";
    const PATH: &str = "/project/C.java";
    fn doc(text: &str) -> Document {
        Document { path: PATH.into(), language: "java".into(), version: 11, text: text.into() }
    }
    fn setup() -> (MockHost, Endpoints) {
        let host = MockHost::install();
        host.add_file("pom.xml", POM);
        host.add_file("src/main/resources/graphql/schema.graphqls", "type Query { books: [String] }");
        let mut tools = Endpoints::default();
        tools.activate_live();
        (host, tools)
    }
    #[test]
    fn wrapper_current_edit_and_fix_returns_one_merged_response_without_manual_publication() {
        let (host, mut tools) = setup();
        let mut d = doc(
            "@Controller class C {@GetMapping(\"/u/{id}\") String get(@PathVariable(\"wrong\") Long id){return null;} @QueryMapping String nope(){return null;}}",
        );
        host.open_editor(&d.path, &d.language, &d.text, &[]);
        host.refresh_diagnostics(&mut tools);
        let problems = host.provider_diagnostics()[&d.path].clone();
        assert_eq!(problems.iter().filter(|d| d.code.as_deref() == Some("unknown-path-variable")).count(), 1);
        assert_eq!(problems.iter().filter(|d| d.code.as_deref() == Some("unknown-schema-field")).count(), 1);
        tools.line_markers(&d);
        assert!(host.diagnostics().is_empty());
        d.version += 1;
        d.text = d.text.replace("wrong", "id").replace("String nope()", "String books()");
        host.open_editor(&d.path, &d.language, &d.text, &[]);
        host.refresh_diagnostics(&mut tools);
        assert!(host.provider_diagnostics().get(&d.path).is_none_or(Vec::is_empty));
    }
    #[test]
    fn scoped_path_hover_uses_exact_imported_owner_and_utf16_range() {
        let (_host, mut tools) = setup();
        let text = "import org.springframework.web.bind.annotation.RestController; import org.springframework.web.bind.annotation.GetMapping; import org.springframework.web.bind.annotation.PathVariable; @RestController class C { /* 😀 */ @GetMapping(\"/u/{id}\") String get(@PathVariable Long id){return null;} }";
        let d = doc(text);
        let from = text.find("{id}").unwrap();
        let at = position(text, from + 1);
        let hover = tools.hover(&d, at).unwrap();
        assert_eq!(hover.range.unwrap(), range(text, from, from + 4));
        let wrong = doc(&text.replace("org.springframework.web.bind.annotation.PathVariable", "other.PathVariable"));
        assert!(tools.hover(&wrong, at).is_none());
        let dynamic = doc(&text.replace("\"/u/{id}\"", "PATH"));
        assert!(tools.hover(&dynamic, position(&dynamic.text, dynamic.text.rfind("id)").unwrap())).is_none());
        let duplicate = doc(&text.replace("Long id)", "Long id, @PathVariable(\"id\") Long second)"));
        assert!(tools.hover(&duplicate, position(&duplicate.text, duplicate.text.find("{id}").unwrap() + 1)).is_none());
    }
    #[test]
    fn merged_java_mapping_and_destination_docs_route_without_discovery_or_runtime_claims(){
        let (h,mut tools)=setup();
        for (text,needle,expected) in [
            ("import org.springframework.stereotype.Controller; import org.springframework.graphql.data.method.annotation.QueryMapping; @Controller class G { @QueryMapping Object books(){return null;} }","@QueryMapping","GraphQL @QueryMapping mapping source"),
            ("import org.springframework.jms.annotation.JmsListener; class L { @JmsListener(destination=\"orders\") void receive(String s){} }","orders","JMS destination annotation source"),
        ] {
            let d=doc(text);let at=position(text,text.find(needle).unwrap());let hover=tools.hover(&d,at).unwrap();assert!(hover.markdown.contains(expected));assert!(hover.markdown.contains("not established"));
        }
        assert!(h.process_runs().is_empty());assert_eq!(h.fetches(),0);
    }
    #[test]
    fn graphql_docs_are_current_schema_declarations_not_resolver_claims() {
        let (_host, mut tools) = setup();
        let mut d = Document {
            path: "/project/schema.graphqls".into(),
            language: "graphql".into(),
            version: 3,
            text: "# 😀\ntype Query { book(id: ID!): Book }".into(),
        };
        let at = position(&d.text, d.text.find("book").unwrap());
        let hover = tools.hover(&d, at).unwrap();
        assert_eq!(hover.range.unwrap().start, at);
        d.version += 1;
        d.text = d.text.replace("book", "novel");
        let at = position(&d.text, d.text.find("novel").unwrap());
        assert!(tools.hover(&d, at).is_some());
    }
    #[test]
    fn messaging_docs_require_exact_xml_namespace_and_literal_destination() {
        let (_host, mut tools) = setup();
        let mut d = Document {
            path: "/project/listeners.xml".into(),
            language: "xml".into(),
            version: 1,
            text: r#"<j:listener xmlns:j="http://www.springframework.org/schema/jms" destination="orders" ref="handler"/>"#.into(),
        };
        let at = position(&d.text, d.text.find("orders").unwrap());
        assert!(tools.hover(&d, at).is_some());
        d.text = d.text.replace("http://www.springframework.org/schema/jms", "urn:other/schema/jms");
        assert!(tools.hover(&d, position(&d.text, d.text.find("orders").unwrap())).is_none());
        d.text = d.text.replace("urn:other/schema/jms", "http://www.springframework.org/schema/jms").replace("orders", "${orders}");
        assert!(tools.hover(&d, position(&d.text, d.text.find("orders").unwrap())).is_none());
    }
    #[test]
    fn cold_oversized_and_malformed_facts_never_prove_schema_absence() {
        let host = MockHost::install();
        host.add_file("pom.xml", POM);
        let mut tools = Endpoints::default();
        let d = doc("@Controller class C {@QueryMapping String missing(){return null;}}");
        assert!(tools.diagnostics(&d).is_empty());
        host.add_file("Huge.java", &" ".repeat(MAX_TEXT + 1));
        tools.activate_live();
        assert!(tools.diagnostics(&d).iter().all(|d| d.code.as_deref() != Some("unknown-schema-field")));
        let mut broken = d.clone();
        broken.text.pop();
        assert!(tools.diagnostics(&broken).is_empty());
        broken.text = " ".repeat(MAX_TEXT + 1);
        assert!(tools.diagnostics(&broken).is_empty());
    }
    #[test]
    fn wrapper_keeps_other_embedded_checks_and_settings_without_marker_duplicates() {
        let (host, mut tools) = setup();
        host.add_file("pom.xml", format!("{POM}<module>child</module>").as_str());
        tools.live_events(&[Event::FileSaved("/project/pom.xml".into())]);
        let xml = Document {
            path: "/project/ws.xml".into(),
            language: "xml".into(),
            version: 1,
            text: r#"<w:message-broker xmlns:w="http://www.springframework.org/schema/websocket"><w:stomp-endpoint/></w:message-broker>"#
                .into(),
        };
        assert!(tools.diagnostics(&xml).iter().any(|d| d.code.as_deref() == Some("missing-attribute")));
        let flow = Document {
            path: "/project/flow.xml".into(),
            language: "xml".into(),
            version: 1,
            text: r#"<flow xmlns="http://www.springframework.org/schema/webflow"><view-state id="same"/><view-state id="same"/></flow>"#
                .into(),
        };
        assert!(tools.diagnostics(&flow).iter().any(|d| d.code.as_deref() == Some("duplicate-state")));
        let feign =
            doc("import org.springframework.cloud.openfeign.FeignClient; @FeignClient(name=\"api\") interface Api {String fetch();}");
        assert!(tools.diagnostics(&feign).iter().any(|d| d.code.as_deref() == Some("feign-method-without-mapping")));
        let d = doc("@Controller class C {@QueryMapping String missing(){return null;}}");
        assert!(tools.diagnostics(&d).iter().any(|d| d.code.as_deref() == Some("unknown-schema-field")));
        host.set_setting("springGraphql.checkUnmatchedMapping", "false");
        assert!(tools.diagnostics(&d).iter().all(|d| d.code.as_deref() != Some("unknown-schema-field")));
        tools.line_markers(&xml);
        tools.line_markers(&flow);
        tools.line_markers(&feign);
        assert!(host.diagnostics().is_empty());
    }
    #[test]
    fn unsaved_handler_snapshot_updates_later_schema_checks() {
        let (_host, mut tools) = setup();
        let mut controller = doc("@Controller class C {@QueryMapping String missing(){return null;}}");
        let schema = Document {
            path: "/project/src/main/resources/graphql/schema.graphqls".into(),
            language: "graphql".into(),
            version: 2,
            text: "type Query { books: [String] }".into(),
        };
        tools.diagnostics(&controller);
        assert!(tools.diagnostics(&schema).iter().any(|d| d.code.as_deref() == Some("unmapped-root-field")));
        controller.version += 1;
        controller.text = controller.text.replace("missing", "books");
        tools.diagnostics(&controller);
        assert!(tools.diagnostics(&schema).iter().all(|d| d.code.as_deref() != Some("unmapped-root-field")));
    }
}
