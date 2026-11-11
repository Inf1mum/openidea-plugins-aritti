//! Per-request lexer configuration. Original source and physical byte ranges stay intact.
use std::cell::RefCell;
const DEFAULTS: [&str; 6] = ["{%", "%}", "{{", "}}", "{#", "#}"];
const KEYS: [&str; 6] = [
    "tagOpen",
    "tagClose",
    "variableOpen",
    "variableClose",
    "commentOpen",
    "commentClose",
];
#[derive(Clone)]
pub struct Syntax {
    pub values: [String; 6],
}
impl Default for Syntax {
    fn default() -> Self {
        Self {
            values: DEFAULTS.map(str::to_owned),
        }
    }
}
thread_local! {static ACTIVE:RefCell<Syntax>=RefCell::new(Syntax::default());}
pub struct Guard(Syntax);
impl Drop for Guard {
    fn drop(&mut self) {
        ACTIVE.with(|v| *v.borrow_mut() = self.0.clone());
    }
}
impl Syntax {
    pub fn current() -> Self {
        ACTIVE.with(|v| v.borrow().clone())
    }
    pub fn expand<'a>(&'a self, value: &'a str) -> &'a str {
        DEFAULTS
            .iter()
            .position(|d| *d == value)
            .map_or(value, |i| self.values[i].as_str())
    }
    pub fn tag_body<'a>(&self, raw: &'a str) -> Option<&'a str> {
        raw.strip_prefix(&self.values[0])?
            .strip_suffix(&self.values[1])
    }
    pub fn valid(&self) -> bool {
        self.values.iter().all(|v| {
            !v.is_empty()
                && v.len() <= 16
                && !v
                    .chars()
                    .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '\'' | '"' | '\\'))
        }) && [0, 2, 4].iter().all(|a| {
            [0, 2, 4]
                .iter()
                .all(|b| a == b || !self.values[*a].starts_with(&self.values[*b]))
        })
    }
    pub fn enter(self) -> Guard {
        let old = ACTIVE.with(|v| v.replace(self));
        Guard(old)
    }
    pub fn scope() -> Option<Guard> {
        let mut syntax = Self::default();
        for (i, key) in KEYS.iter().enumerate() {
            if let Some(raw) = crate::host::setting(key) {
                if raw.len() > 128 {
                    return None;
                }
                syntax.values[i] = serde_json::from_str::<String>(&raw).ok()?;
            }
        }
        syntax.valid().then(|| syntax.enter())
    }
    pub fn snippet(&self, text: &str) -> String {
        let mut out = String::new();
        let mut at = 0;
        while at < text.len() {
            if let Some(i) = DEFAULTS.iter().position(|d| text[at..].starts_with(d)) {
                out.push_str(&self.values[i]);
                at += DEFAULTS[i].len();
            } else {
                let c = text[at..].chars().next().unwrap();
                out.push(c);
                at += c.len_utf8();
            }
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use openidea_plugin::{
        diagnostics::DiagnosticsProvider, hover::HoverProvider,
        injections::Injections, mock::MockHost,
    };
    fn configured() -> MockHost {
        let host = MockHost::install();
        for (key, value) in KEYS.iter().zip(["<%", "%>", "[[[", "]]]", "<#", "#>"]) {
            host.set_setting(key, &serde_json::to_string(value).unwrap());
        }
        host
    }
    fn doc(text: &str) -> Document {
        Document {
            path: "/project/templates/a.twig".into(),
            language: "twig".into(),
            text: text.into(),
            version: 1,
        }
    }
    #[test]
    fn custom_delimiters_drive_highlighting_folding_diagnostics_and_exact_unsaved_ranges() {
        let _host = configured();
        let source =
            "😀 <% block title %>\n[[[ item|upper ]]]\n<% endblock %><# note #> {{ ordinary }}";
        let mut plugin = Twig::default();
        let tokens = plugin.highlights("twig", source);
        assert!(tokens.iter().any(|t| &source[t.range.clone()] == "[[["));
        assert!(
            !tokens
                .iter()
                .any(|t| &source[t.range.clone()] == "ordinary")
        );
        assert_eq!(plugin.outline("twig", source).len(), 1);
        assert_eq!(plugin.folding("twig", source).len(), 1);
        assert!(plugin.diagnostics(&doc(source)).is_empty());
        let broken = "😀 <% if active %>[[[ item ]]]";
        let findings = plugin.diagnostics(&doc(broken));
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].range.start,
            position(broken, broken.find("<%").unwrap())
        );
        let at = position(source, source.find("upper").unwrap() + 2);
        assert!(plugin.hover(&doc(source), at).is_some());
    }
    #[test]
    fn custom_delimiters_cover_completion_macros_injections_and_indexed_binding_ranges() {
        let _host = configured();
        let mut plugin = Twig::default();
        let source = "😀 <% for key, item in items %>[[[ item ]]]<% endfor %>";
        let edges = crate::indexing::source_edges(&openidea_plugin::indexing::Document {
            path: doc(source).path,
            text: source.into(),
            symbols: vec![],
        });
        assert_eq!(edges.definitions.len(), 2);
        assert_eq!(edges.references.len(), 1);
        let span = &edges.references[0].range;
        assert_eq!(
            &source[offset(source, span.start).unwrap()..offset(source, span.end).unwrap()],
            "item"
        );
        let marked = "<% macro badge(label='x') %><% endmacro %> [[[ _self.badge(la) ]]]";
        let at = position(marked, marked.find("(la)").unwrap() + 3);
        let items = plugin.completion(&doc(marked), at, None);
        assert!(
            items.iter().any(|i| i.label == "label"),
            "{:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
        let source = "😀 <% block javascript %>const café=1;<% endblock %>";
        let injected = plugin.injections(&openidea_plugin::injections::Document {
            path: doc(source).path,
            language: "twig".into(),
            text: source.into(),
            version: 1,
        });
        assert_eq!(injected.len(), 1);
        assert_eq!(
            injected[0].range.start,
            position(source, source.find("const").unwrap())
        );
        let source = "<div>b";
        let items = plugin.completion(&doc(source), position(source, source.len()), None);
        let block = items.iter().find(|i| i.label == "blk").unwrap();
        assert!(block.insert_text.starts_with("<% block"));
    }
    #[test]
    fn arbitrary_width_unicode_delimiters_and_nested_brackets_keep_physical_source() {
        let host = MockHost::install();
        for (key, value) in KEYS.iter().zip(["⟨%", "%⟩", "⟨⟨", "⟩⟩", "⟨#", "#⟩"]) {
            host.set_setting(key, &serde_json::to_string(value).unwrap());
        }
        let source = "😀 ⟨% macro badge(value) %⟩⟨⟨ value ⟩⟩⟨% endmacro %⟩ ⟨⟨ _self.badge(va) ⟩⟩";
        let mut plugin = Twig::default();
        let tokens = plugin.highlights("twig", source);
        assert!(tokens.iter().all(
            |t| source.is_char_boundary(t.range.start) && source.is_char_boundary(t.range.end)
        ));
        let at = position(source, source.find("(va)").unwrap() + 3);
        let items = plugin.completion(&doc(source), at, None);
        assert!(items.iter().any(|i| i.label == "value"));
        assert!(plugin.diagnostics(&doc(source)).is_empty());
        host.set_setting("variableOpen", r#""[""#);
        host.set_setting("variableClose", r#""]""#);
        let source = "[ foo([1,2], {key: 'value'}) ]";
        assert!(plugin.diagnostics(&doc(source)).is_empty());
        assert!(
            plugin
                .highlights("twig", source)
                .iter()
                .any(|t| &source[t.range.clone()] == "foo")
        );
    }
    #[test]
    fn custom_imports_template_navigation_and_bracket_checks_share_one_configuration() {
        let host = configured();
        host.set_setting("expressionErrors", "true");
        host.add_file(
            "templates/forms.twig",
            "<% macro badge(label='x') %><% endmacro %>",
        );
        let source = "😀 <% import 'forms.twig' as forms %> [[[ forms.badge(la) ]]]";
        let mut plugin = Twig::default();
        let at = position(source, source.find("(la)").unwrap() + 3);
        assert!(
            plugin
                .completion(&doc(source), at, None)
                .iter()
                .any(|i| i.label == "label")
        );
        let target = plugin.definition(
            &doc(source),
            position(source, source.find("badge").unwrap() + 2),
        );
        assert_eq!(target.len(), 1);
        assert_eq!(target[0].path, "/project/templates/forms.twig");
        let bad = "😀 [[[ f(a] ]]]";
        assert!(
            plugin
                .diagnostics(&doc(bad))
                .iter()
                .any(|d| d.code.as_deref() == Some(crate::checks::BRACKET))
        );
        let source = "<# [[[ f(a] ]]] #><% verbatim %>[[[ f(a] ]]]<% endverbatim %>";
        assert!(plugin.diagnostics(&doc(source)).is_empty());
    }
    #[test]
    fn ambiguous_and_invalid_delimiters_withhold_provider_results_and_scope_does_not_leak() {
        let host = configured();
        host.set_setting("variableOpen", r#""<%extra""#);
        let mut plugin = Twig::default();
        assert!(plugin.highlights("twig", "<% if x %>").is_empty());
        assert!(
            plugin
                .completion(&doc("<%"), openidea_plugin::Position::new(0, 2), None)
                .is_empty()
        );
        assert_eq!(
            Syntax::current().values[0],
            "{%",
            "request configuration must be restored"
        );
    }
}
