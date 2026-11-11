//! Independent source-only SpEL highlighting and physical host-string injections.
//! No evaluation, type resolution or embedded server behavior is supplied.
use crate::{Navigator, annotation_owners::Owners};
use openidea_plugin::{
    Range,
    injections::{Document, Injection, Injections},
    syntax::{Highlight, SyntaxProvider, Token},
    text::position,
};
const MAX_TEXT: usize = 128 * 1024;
fn word(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '$')
}
fn quoted(text: &str, at: usize) -> usize {
    let quote = text.as_bytes()[at];
    let mut i = at + 1;
    while i < text.len() {
        if text.as_bytes()[i] == quote {
            if text.as_bytes().get(i + 1) == Some(&quote) {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += text[i..].chars().next().unwrap().len_utf8();
    }
    text.len()
}
fn lex(text: &str) -> Vec<Token> {
    if text.len() > MAX_TEXT {
        return vec![];
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() && out.len() < 4096 {
        let ch = text[i..].chars().next().unwrap();
        if ch.is_whitespace() {
            i += ch.len_utf8();
            continue;
        }
        let from = i;
        let kind = if matches!(ch, '\'' | '"') {
            i = quoted(text, i);
            Highlight::String
        } else if ch.is_ascii_digit() {
            i += 1;
            while i < text.len()
                && (text.as_bytes()[i].is_ascii_digit() || text.as_bytes()[i] == b'.')
            {
                i += 1;
            }
            Highlight::Number
        } else if matches!(ch, '#' | '@') && text[i + 1..].chars().next().is_some_and(word) {
            i += 1;
            while i < text.len() && text[i..].chars().next().is_some_and(word) {
                i += text[i..].chars().next().unwrap().len_utf8();
            }
            Highlight::Variable
        } else if ch.is_alphabetic() || ch == '_' {
            i += ch.len_utf8();
            while i < text.len() && text[i..].chars().next().is_some_and(word) {
                i += text[i..].chars().next().unwrap().len_utf8();
            }
            match &text[from..i] {
                "true" | "false" | "null" => Highlight::Constant,
                "and" | "or" | "not" | "eq" | "ne" | "lt" | "le" | "gt" | "ge" | "div" | "mod"
                | "instanceof" | "matches" | "between" | "new" | "T" => Highlight::Keyword,
                _ => {
                    if text[i..].trim_start().starts_with('(') {
                        Highlight::FunctionCall
                    } else {
                        Highlight::Property
                    }
                }
            }
        } else {
            i += ch.len_utf8();
            if "()[]{},.".contains(ch) {
                Highlight::Punctuation
            } else {
                Highlight::Operator
            }
        };
        out.push(Token {
            range: from..i,
            kind,
        });
    }
    out
}
pub(crate) fn completion(
    doc: &openidea_plugin::providers::Document,
    at: openidea_plugin::Position,
) -> Vec<openidea_plugin::providers::CompletionItem> {
    use openidea_plugin::providers::{CompletionItem, CompletionKind};
    if doc.language != "spel" || doc.text.len() > MAX_TEXT {
        return vec![];
    }
    let Some(cursor) = openidea_plugin::text::offset(&doc.text, at) else {
        return vec![];
    };
    if lex(&doc.text)
        .iter()
        .any(|t| t.kind == Highlight::String && t.range.start < cursor && cursor < t.range.end)
    {
        return vec![];
    }
    let mut from = cursor;
    let mut to = cursor;
    while from > 0 {
        let ch = doc.text[..from].chars().next_back().unwrap();
        if !word(ch) {
            break;
        }
        from -= ch.len_utf8();
    }
    while to < doc.text.len() {
        let ch = doc.text[to..].chars().next().unwrap();
        if !word(ch) {
            break;
        }
        to += ch.len_utf8();
    }
    let typed = &doc.text[from..cursor];
    let variables = doc.text[..from].ends_with('#');
    let labels: &[&str] = if variables {
        &["root", "this"]
    } else {
        &[
            "true",
            "false",
            "null",
            "and",
            "or",
            "not",
            "eq",
            "ne",
            "lt",
            "le",
            "gt",
            "ge",
            "div",
            "mod",
            "instanceof",
            "matches",
            "between",
            "new",
            "T",
        ]
    };
    labels
        .iter()
        .filter(|label| label.starts_with(typed))
        .map(|label| CompletionItem {
            replace: Some(Range::new(
                position(&doc.text, from),
                position(&doc.text, to),
            )),
            detail: Some(
                if variables {
                    "SpEL context variable"
                } else {
                    "SpEL keyword"
                }
                .into(),
            ),
            ..CompletionItem::new(
                label,
                if variables {
                    CompletionKind::Variable
                } else {
                    CompletionKind::Keyword
                },
            )
        })
        .collect()
}

impl SyntaxProvider for Navigator {
    fn highlights(&mut self, language: &str, text: &str) -> Vec<Token> {
        if language == "spel" {
            lex(text)
        } else {
            vec![]
        }
    }
}
fn templates(body: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(start) = body[cursor..].find("#{").map(|n| n + cursor) {
        let mut depth = 1usize;
        let mut i = start + 2;
        while i < body.len() {
            let ch = body[i..].chars().next().unwrap();
            if matches!(ch, '\'' | '"') {
                i = quoted(body, i);
                continue;
            }
            if ch == '{' {
                depth += 1;
                if depth > 64 {
                    return vec![];
                }
            }
            if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            i += ch.len_utf8();
        }
        if depth != 0 {
            return vec![];
        }
        if i > start + 2 && !body[start + 2..i].contains("${") {
            out.push((start + 2, i));
        }
        cursor = i + 1;
        if cursor >= body.len() {
            break;
        }
    }
    out
}
fn contract(annotation: &str, argument: &str) -> Option<(&'static str, bool)> {
    match (annotation, argument) {
        ("Value", "value") => Some(("org.springframework.beans.factory.annotation.Value", true)),
        ("Cacheable", "key" | "condition" | "unless") => {
            Some(("org.springframework.cache.annotation.Cacheable", false))
        }
        ("CachePut", "key" | "condition" | "unless") => {
            Some(("org.springframework.cache.annotation.CachePut", false))
        }
        ("CacheEvict", "key" | "condition") => {
            Some(("org.springframework.cache.annotation.CacheEvict", false))
        }
        ("EventListener", "condition") => {
            Some(("org.springframework.context.event.EventListener", false))
        }
        ("TransactionalEventListener", "condition") => Some((
            "org.springframework.transaction.event.TransactionalEventListener",
            false,
        )),
        ("ConditionalOnExpression", "value") => Some((
            "org.springframework.boot.autoconfigure.condition.ConditionalOnExpression",
            true,
        )),
        _ => None,
    }
}
impl Injections for Navigator {
    fn injections(&mut self, doc: &Document) -> Vec<Injection> {
        if doc.language == "xml" {
            if doc.text.len() > MAX_TEXT
                || !doc.path.ends_with(".xml")
                || !crate::xml_namespace::beans_document(&doc.text)
            {
                return vec![];
            }
            let mut out = Vec::new();
            for hit in crate::sites::xml(&doc.text)
                .into_iter()
                .filter(|hit| matches!(hit.site, crate::sites::Site::Spel(_)))
            {
                let Some(body) = doc.text.get(hit.from..hit.to) else {
                    continue;
                };
                if body.contains(['&', '\r', '\n', '\t']) {
                    continue;
                }
                for (from, to) in templates(body) {
                    out.push(Injection {
                        range: Range::new(
                            position(&doc.text, hit.from + from),
                            position(&doc.text, hit.from + to),
                        ),
                        language: "spel".into(),
                    });
                    if out.len() >= 128 {
                        return out;
                    }
                }
            }
            return out;
        }
        if doc.text.len() > MAX_TEXT
            || !matches!(doc.language.as_str(), "java" | "kotlin")
            || (doc.language == "java" && !doc.path.ends_with(".java"))
            || (doc.language == "kotlin" && !doc.path.ends_with(".kt"))
        {
            return vec![];
        }
        let kotlin = doc.language == "kotlin";
        let file = spring_text::parse_source(&doc.path, &doc.text);
        let owners = Owners::new(&file, &doc.text, kotlin);
        let mut annotations = Vec::new();
        for ty in &file.types {
            annotations.extend(ty.annotations.iter());
            for component in &ty.components {
                annotations.extend(component.annotations.iter());
            }
            for member in &ty.members {
                annotations.extend(member.annotations.iter());
                for parameter in &member.params {
                    annotations.extend(parameter.annotations.iter());
                }
            }
        }
        if annotations.len() > 512 {
            return vec![];
        }
        let mut out = Vec::new();
        for annotation in annotations {
            for argument in &annotation.args {
                let Some((owner, template)) = contract(&annotation.name, &argument.name) else {
                    continue;
                };
                if !owners.known(&doc.text, annotation, owner)
                    || annotation
                        .args
                        .iter()
                        .filter(|a| a.name == argument.name)
                        .count()
                        != 1
                {
                    continue;
                }
                let [literal] = argument.strings.as_slice() else {
                    continue;
                };
                let Some(body) = doc.text.get(literal.at..literal.end) else {
                    continue;
                };
                let Some(raw) = doc.text.get(argument.at..argument.end) else {
                    continue;
                };
                if body.is_empty()
                    || body != literal.value
                    || body.contains(['\\', '\r', '\n'])
                    || raw.trim() != format!("\"{body}\"")
                    || kotlin && body.contains('$')
                {
                    continue;
                }
                let spans = if template {
                    if annotation.name == "ConditionalOnExpression" && !body.contains("#{") {
                        if body.contains("${") {
                            vec![]
                        } else {
                            vec![(0, body.len())]
                        }
                    } else {
                        templates(body)
                    }
                } else if body.contains("${") {
                    vec![]
                } else {
                    vec![(0, body.len())]
                };
                for (from, to) in spans {
                    out.push(Injection {
                        range: Range::new(
                            position(&doc.text, literal.at + from),
                            position(&doc.text, literal.at + to),
                        ),
                        language: "spel".into(),
                    });
                    if out.len() >= 128 {
                        return out;
                    }
                }
            }
        }
        out.sort_by_key(|i| (i.range.start.line, i.range.start.character));
        out.dedup_by(|a, b| a.range == b.range);
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, text::offset};
    fn doc(text: &str) -> Document {
        Document {
            path: "/project/Bean.java".into(),
            language: "java".into(),
            text: text.into(),
            version: 1,
        }
    }
    fn bodies(doc: &Document) -> Vec<String> {
        Navigator::default()
            .injections(doc)
            .iter()
            .map(|i| {
                doc.text[offset(&doc.text, i.range.start).unwrap()
                    ..offset(&doc.text, i.range.end).unwrap()]
                    .into()
            })
            .collect()
    }
    #[test]
    fn exact_owned_annotation_expressions_map_physical_utf16_spans() {
        let _host = MockHost::install();
        let d = doc(
            "// 🦉\r\nimport org.springframework.beans.factory.annotation.Value;\nclass Bean { @Value(\"prefix #{@clock.now()} and #{ {1,2}[0] }\") String value; @org.springframework.cache.annotation.Cacheable(key=\"#id\",condition=\"#id > 0\") String load(long id){return null;} }",
        );
        assert_eq!(
            bodies(&d),
            vec!["@clock.now()", " {1,2}[0] ", "#id", "#id > 0"]
        );
        assert!(
            Navigator::default()
                .injections(&d)
                .iter()
                .all(|i| i.language == "spel")
        );
        assert_eq!(
            bodies(&doc(
                "@org.springframework.boot.autoconfigure.condition.ConditionalOnExpression(\"true and !false\") class Bean {}"
            )),
            vec!["true and !false"]
        );
    }
    #[test]
    fn foreign_escaped_concatenated_dynamic_and_ambiguous_sources_stay_quiet() {
        let _host = MockHost::install();
        for source in [
            "class Bean { @vendor.Value(\"#{1}\") String value; }",
            "class Bean { @Value(\"#{1}\") String value; }",
            "import org.springframework.beans.factory.annotation.*; class Bean { @Value(\"#{1}\") String value; }",
            "class Bean { @org.springframework.beans.factory.annotation.Value(\"#{1}\"+\"#{2}\") String value; }",
            "class Bean { @org.springframework.beans.factory.annotation.Value(\"#{\\u0031}\") String value; }",
            "class Bean { @org.springframework.beans.factory.annotation.Value(\"#{${id}}\") String value; }",
            "class Bean { @org.springframework.beans.factory.annotation.Value(\"#{1\") String value; }",
            "class Bean { @org.springframework.beans.factory.annotation.Value(value=\"#{1}\",value=\"#{2}\") String value; }",
        ] {
            assert!(bodies(&doc(source)).is_empty(), "{source}");
        }
    }
    fn virtual_document(source:&Document,ranges:&[Injection])->Document {
        let mut text=String::new();let mut at=openidea_plugin::Position::new(0,0);
        for ch in source.text.chars(){let inside=ranges.iter().any(|range|range.range.start<=at&&at<range.range.end);if inside||matches!(ch,'\n'|'\r'){text.push(ch);}else{for _ in 0..ch.len_utf16(){text.push(' ');}}
            if ch=='\n'{at.line+=1;at.character=0;}else{at.character+=ch.len_utf16() as u32;}
        }
        Document{language:"spel".into(),text,..source.clone()}
    }
    #[test]
    fn injected_completion_uses_full_physical_java_and_xml_documents_and_host_bounds(){
        let _host=MockHost::install();
        let java=doc("// 🦉\r\nclass Bean { @org.springframework.beans.factory.annotation.Value(\"prefix #{tr }\") String value; }");
        let mut xml=doc("<!-- 🦉 -->\r\n<beans xmlns=\"http://www.springframework.org/schema/beans\"><bean id=\"x\"><property name=\"x\" value=\"#{tr }\"/></bean></beans>");xml.path="/project/beans.xml".into();xml.language="xml".into();
        for source in [java,xml] {
            let mut plugin=Navigator::default();let ranges=plugin.injections(&source);assert_eq!(ranges.len(),1);
            let virtual_doc=virtual_document(&source,&ranges);assert_eq!(virtual_doc.text.encode_utf16().count(),source.text.encode_utf16().count());assert_eq!(virtual_doc.path,source.path);assert_eq!(virtual_doc.version,source.version);
            let start=source.text.find("tr }").unwrap();let cursor=position(&source.text,start+2);assert!(ranges.iter().any(|range|range.range.start<=cursor&&cursor<range.range.end));
            let items=openidea_plugin::providers::Providers::completion(&mut plugin,&virtual_doc,cursor,None);let item=items.iter().find(|item|item.label=="true").unwrap();assert_eq!(item.replace,Some(Range::new(position(&source.text,start),cursor)));assert!(ranges.iter().any(|range|range.range.start<=item.replace.unwrap().start&&item.replace.unwrap().end<=range.range.end));
            let outside=openidea_plugin::Position::new(0,1);assert!(!ranges.iter().any(|range|range.range.start<=outside&&outside<range.range.end),"native feature_documents does not route spel outside its physical regions");
            let foreign=openidea_plugin::providers::Providers::completion(&mut plugin,&virtual_doc,outside,None);assert!(foreign.iter().all(|item|!ranges.iter().any(|range|item.replace.is_some_and(|replace|range.range.start<=replace.start&&replace.end<=range.range.end))),"native Bounds drops any result outside injection spans");
        }
    }

    #[test]
    fn embedded_spel_completion_keeps_physical_keyword_and_context_variable_ranges() {
        let _host = MockHost::install();
        let mut d = doc("#ro");
        d.language = "spel".into();
        let items = completion(&d, position(&d.text, d.text.len()));
        assert!(items.iter().any(|i| i.label == "root"));
        assert_eq!(items[0].replace.unwrap().start, position(&d.text, 1));
        d.text = "tr".into();
        assert!(
            completion(&d, position(&d.text, 2))
                .iter()
                .any(|i| i.label == "true")
        );
        d.text = "'tr'".into();
        assert!(completion(&d, position(&d.text, 3)).is_empty());
    }

    #[test]
    fn spring_xml_expressions_require_owned_untransformed_attribute_ranges() {
        let _host = MockHost::install();
        let mut d = doc(
            "<beans xmlns=\"http://www.springframework.org/schema/beans\"><bean id=\"x\"><property name=\"x\" value=\"#{@clock.now()}\"/></bean></beans>",
        );
        d.path = "/project/beans.xml".into();
        d.language = "xml".into();
        assert_eq!(bodies(&d), vec!["@clock.now()"]);
        for text in [
            d.text.replace("#{@clock.now()}", "#{1 &gt; 0}"),
            d.text.replace("schema/beans", "schema/foreign"),
            d.text
                .replace("<property name", "<property xmlns=\"urn:foreign\" name"),
        ] {
            let mut changed = d.clone();
            changed.text = text;
            assert!(bodies(&changed).is_empty());
        }
    }

    #[test]
    fn kotlin_plain_expressions_are_owned_without_string_template_remapping() {
        let _host = MockHost::install();
        let mut d = doc(
            "import org.springframework.beans.factory.annotation.Value\nclass Bean { @Value(\"#{@clock.now()}\") val value: String = \"\" }",
        );
        d.language = "kotlin".into();
        d.path = "/project/Bean.kt".into();
        assert_eq!(bodies(&d), vec!["@clock.now()"]);
        d.text = d.text.replace("#{@clock.now()}", "#{${dynamic}}");
        assert!(bodies(&d).is_empty());
    }
    #[test]
    fn independent_spel_language_tokens_are_bounded_nonoverlapping_physical_bytes() {
        let _host = MockHost::install();
        let source = "@clock.now() > 2 and #user.name == 'it''s 🦉' ? true : null";
        let mut plugin = Navigator::default();
        let tokens = plugin.highlights("spel", source);
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == Highlight::Variable && &source[t.range.clone()] == "@clock")
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == Highlight::String && &source[t.range.clone()] == "'it''s 🦉'")
        );
        assert!(
            tokens
                .windows(2)
                .all(|w| w[0].range.end <= w[1].range.start)
        );
        assert!(tokens.iter().all(
            |t| source.is_char_boundary(t.range.start) && source.is_char_boundary(t.range.end)
        ));
        assert!(plugin.highlights("java", source).is_empty());
        assert!(
            plugin
                .highlights("spel", &"x".repeat(MAX_TEXT + 1))
                .is_empty()
        );
    }
}
