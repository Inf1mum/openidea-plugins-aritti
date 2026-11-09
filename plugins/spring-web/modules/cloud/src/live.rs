use super::*;
impl CloudTools {
    fn cached_indexes(&mut self, doc: &Document) -> Option<(&mut Index<JavaFacts>, &mut Index<StreamConfig>)> {
        let java = self.java.for_cached_document(&doc.path, &doc.text)?;
        let config = self.config.for_cached_document(&doc.path, &doc.text)?;
        if !java.fresh() || !config.fresh() {
            return None;
        }
        Some((java, config))
    }
    pub fn live_diagnostics(&mut self, doc: &Document, allow_absence: bool) -> Vec<Diagnostic> {
        if !is_jvm(doc) {
            return if allow_absence { self.live_config_diagnostics(doc) } else { Vec::new() };
        }
        let facts = facts_in(&doc.path, &doc.text);
        let mut out: Vec<_> = facts
            .unmapped
            .iter()
            .map(|&(range, custom)| {
                Diagnostic::new(
                    range,
                    if custom { Severity::Info } else { Severity::Warning },
                    "Source Feign client method has no request mapping; a custom runtime contract may supply it.",
                )
                .code("feign-method-without-mapping")
            })
            .collect();
        let groups = checks::Groups::from_settings();
        if allow_absence && let Some((java, config)) = self.cached_indexes(doc) {
            if groups.stream_bindings {
                out.extend(checks::stream_channel_unbound(&doc.path, &doc.text, java, config).iter().map(|f| f.diagnostic(&doc.text)));
            }
            if groups.legacy_stream {
                out.extend(checks::legacy_stream(&doc.path, &doc.text, java).iter().map(|f| f.diagnostic(&doc.text)));
            }
        }
        out
    }
    fn live_config_diagnostics(&mut self, doc: &Document) -> Vec<Diagnostic> {
        if !config::is_config_file(&doc.path) {
            return Vec::new();
        }
        let keys = config::keys(&doc.path, &doc.text);
        let mine = stream_config(&doc.text, &keys);
        let groups = checks::Groups::from_settings();
        let mut problems = Vec::new();
        if groups.bootstrap && checks::is_bootstrap_file(&doc.path) && project_of(&doc.path).is_some() {
            let Some(builds) = self.builds.for_cached_document(&doc.path, &doc.text).filter(|i| i.fresh()) else {
                return Vec::new();
            };
            let missing = checks::bootstrap_config_missing_dependency(&doc.path, &doc.text, builds);
            problems.extend(missing.iter().map(|f| f.diagnostic(&doc.text)));
        }
        let Some((java, config)) = self.cached_indexes(doc) else {
            return problems;
        };
        if groups.stream_bindings {
            for b in mine.binder_bindings.iter().filter(|b| checks::binder_binding_unbound(&b.name, java, config)) {
                let message = format!(
                    "Binder properties for binding `{}`, which spring.cloud.stream.bindings and the project's functions do not declare",
                    b.name
                );
                problems.push(Diagnostic::new(b.range, Severity::Info, message).code("stream-channel-unbound"));
            }
        }
        // Functions may come from libraries or Kotlin: only check projects that declare some.
        if java.complete() && java.iter().any(|(_, f)| !f.functions.is_empty()) {
            for b in mine.bindings.iter().filter(|b| !b.name.contains("${")) {
                if let Some(function) = function_of(&b.name)
                    && !known_binding(java, config, &b.name, function)
                {
                    let message = format!(
                        "Binding `{}`: no function bean, function definition or StreamBridge destination is named `{function}`",
                        b.name
                    );
                    problems.push(Diagnostic::new(b.range, Severity::Warning, message).code("unknown-binding-function"));
                }
            }
            for n in mine.definitions.iter().flatten().filter(|n| !n.name.contains("${")) {
                if functions(java, &n.name).next().is_none() {
                    let message = format!("No Function, Consumer or Supplier bean is named `{}`", n.name);
                    problems.push(Diagnostic::new(n.range, Severity::Warning, message).code("unknown-function"));
                }
            }
        }
        if java.complete() {
            for k in &keys {
                let Some((list, mvc)) = route_list(&k.key) else { continue };
                let name = k.value.split('=').next().unwrap_or("").trim_end();
                if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    continue;
                }
                let shipped: &[&str] = if list == "predicates" { &PREDICATES } else { &FILTERS };
                if shipped.contains(&name) || java.iter().any(|(_, f)| f.factories.iter().any(|n| n == name)) {
                    continue;
                }
                let kind = if list == "predicates" { "route predicate" } else { "gateway filter" };
                let message = format!("Unknown {kind} `{name}`: neither shipped with Spring Cloud Gateway nor a factory of this project");
                let severity = if mvc { Severity::Info } else { Severity::Warning };
                let at = range(&doc.text, k.value_at, k.value_at + name.len());
                problems.push(Diagnostic::new(at, severity, message).code(format!("unknown-gateway-{}", &list[..list.len() - 1])));
            }
        }
        problems
    }
}
