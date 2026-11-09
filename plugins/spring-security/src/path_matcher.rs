//! Explicit factory contracts only: default strategy/version of requestMatchers is unknown.
use super::*;
use spring_text::java::Tok;
const CLASS: &str = "PathPatternRequestMatcher";
const FQN: &str = "org.springframework.security.web.servlet.util.matcher.PathPatternRequestMatcher";
const MAX_TEXT: usize = 128 << 10;
pub(crate) const CODE: &str = "path-pattern-leading-slash";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Issue {
    pub from: usize,
    pub to: usize,
    pub api: &'static str,
}
#[derive(Clone, Debug)]
pub(crate) struct Declaration {
    pub from: usize,
    pub to: usize,
    pub api: &'static str,
    pub path: String,
    pub method: Option<String>,
}
// Reject malformed lexical units, text blocks and pre-lexical Unicode escapes rather than
// treating source spelling as the Java runtime value. Comments and quoted data stay masked.
pub(crate) fn closed(text: &str) -> bool {
    if text.contains("\\u") {
        return false;
    }
    let b = text.as_bytes();
    let mut at = 0;
    let mut stack = Vec::new();
    while at < b.len() {
        if b[at..].starts_with(b"//") {
            at += b[at..]
                .iter()
                .position(|b| *b == b'\n')
                .unwrap_or(b.len() - at);
            continue;
        }
        if b[at..].starts_with(b"/*") {
            let Some(n) = b[at + 2..].windows(2).position(|s| s == b"*/") else {
                return false;
            };
            at += n + 4;
            continue;
        }
        if b[at..].starts_with(b"\"\"\"") {
            return false;
        }
        if matches!(b[at], b'\'' | b'"') {
            let quote = b[at];
            at += 1;
            let mut ended = false;
            while at < b.len() {
                if b[at] == quote {
                    at += 1;
                    ended = true;
                    break;
                }
                if b[at] == b'\n' || b[at] == b'\r' {
                    return false;
                }
                at += if b[at] == b'\\' { 2 } else { 1 };
            }
            if !ended {
                return false;
            }
            continue;
        }
        match b[at] {
            b'(' => stack.push(b')'),
            b'[' => stack.push(b']'),
            b'{' => stack.push(b'}'),
            b')' | b']' | b'}' if stack.pop() != Some(b[at]) => return false,
            _ => {}
        }
        if stack.len() > 128 {
            return false;
        }
        at += 1;
    }
    stack.is_empty()
}
fn literal_path(raw: &str) -> bool {
    if raw.is_empty() || raw.len() > 512 || raw.starts_with('/') {
        return false;
    }
    let parts: Vec<_> = raw.split('/').collect();
    parts.iter().enumerate().all(|(i, p)| {
        if *p == "**" {
            return i + 1 == parts.len();
        }
        if *p == "*" {
            return true;
        }
        if p.is_empty() {
            return i > 0 && i + 1 == parts.len();
        }
        !matches!(*p, "." | "..")
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    })
}
fn sequence(toks: &[Tok<'_>], at: usize, sequence: &[&str]) -> bool {
    toks.get(at..at.saturating_add(sequence.len()))
        .is_some_and(|t| t.iter().zip(sequence).all(|(t, s)| t.s == *s))
}
fn unshadowed(toks: &[Tok<'_>], file: &JavaFile, name: &str) -> bool {
    if file
        .imports
        .iter()
        .any(|i| i.is_static && i.path.rsplit('.').next() == Some(name))
    {
        return false;
    }
    toks.iter()
        .enumerate()
        .filter(|(_, t)| t.s == name)
        .all(|(i, t)| {
            file.imports
                .iter()
                .any(|imp| imp.at <= t.at && t.end() <= imp.end)
                || i > 0 && toks[i - 1].s == "package"
                || toks.get(i + 1).is_some_and(|t| t.s == ".")
        })
}
// Admit a documented nullable HttpMethod overload without guessing an arbitrary
// expression's type. The edit never changes the method argument or its trivia.
fn path_argument(toks: &[Tok<'_>], file: &JavaFile, argument: usize) -> Option<usize> {
    let first = toks.get(argument)?;
    if first.s.starts_with('"') { return Some(argument); }
    let method_end = if first.s == "null" {
        argument + 1
    } else {
        const METHOD: &str = "org.springframework.http.HttpMethod";
        let simple = file.imports.iter().filter(|i| !i.is_static && i.path.rsplit('.').next()==Some("HttpMethod")).count()==1
            && file.imports.iter().any(|i| !i.is_static && i.path==METHOD)
            && !file.types.iter().any(|t| t.name=="HttpMethod")
            && unshadowed(toks,file,"HttpMethod");
        let qualified = ["org", ".", "springframework", ".", "http", ".", "HttpMethod"];
        let after = if unshadowed(toks,file,"org") && sequence(toks,argument,&qualified) {
            argument+qualified.len()
        } else if simple && first.s=="HttpMethod" {argument+1} else {return None;};
        if toks.get(after)?.s!="." || !matches!(toks.get(after+1)?.s,"GET"|"HEAD"|"POST"|"PUT"|"PATCH"|"DELETE"|"OPTIONS"|"TRACE") {return None;}
        after+2
    };
    (toks.get(method_end)?.s==",").then_some(method_end+1)
}
pub(crate) fn declarations(doc: &Document) -> Vec<Declaration> {
    if doc.language != "java"
        || !doc.path.ends_with(".java")
        || doc.text.len() > MAX_TEXT
        || !closed(&doc.text)
    {
        return Vec::new();
    }
    let toks = java::tokens(&doc.text);
    let file = java::parse(&doc.text);
    if file.types.is_empty()
        || file
            .types
            .iter()
            .any(|t| t.kind != "class" || t.supers().next().is_some())
        || file.imports.iter().any(|i| i.path.ends_with(".*"))
        || toks.iter().any(|t| matches!(t.s, "extends" | "implements"))
    {
        return Vec::new();
    }
    // An anonymous class can inherit a shadowing field which this local scanner cannot resolve.
    if toks.iter().enumerate().any(|(i, t)| {
        t.s == "new"
            && toks[i + 1..]
                .iter()
                .take_while(|t| !matches!(t.s, ";" | "{" | "="))
                .position(|t| t.s == "(")
                .is_some_and(|open| {
                    toks.get(java::closing(&toks, i + 1 + open) + 1)
                        .is_some_and(|t| t.s == "{")
                })
    }) {
        return Vec::new();
    }
    let simple = file
        .imports
        .iter()
        .filter(|i| !i.is_static && i.path.rsplit('.').next() == Some(CLASS))
        .count()
        == 1
        && file.imports.iter().any(|i| !i.is_static && i.path == FQN)
        && !file.types.iter().any(|t| t.name == CLASS)
        && unshadowed(&toks, &file, CLASS);
    let qualified = unshadowed(&toks, &file, "org");
    let fqn: Vec<_> = FQN
        .split('.')
        .enumerate()
        .flat_map(|(i, p)| if i == 0 { vec![p] } else { vec![".", p] })
        .collect();
    let mut out = Vec::new();
    for i in 0..toks.len() {
        if out.len() >= 64 {
            break;
        }
        if i > 0 && matches!(toks[i - 1].s, "." | "@" | "new") {
            continue;
        }
        let after = if qualified && sequence(&toks, i, &fqn) {
            i + fqn.len()
        } else if simple && toks[i].s == CLASS {
            i + 1
        } else {
            continue;
        };
        let (argument, api) = if sequence(
            &toks,
            after,
            &[".", "withDefaults", "(", ")", ".", "matcher", "("],
        ) {
            (after + 7, "Spring Security 6.5+ withDefaults().matcher")
        } else if sequence(&toks, after, &[".", "pathPattern", "("]) {
            (after + 3, "Spring Security 7.0+ pathPattern factory")
        } else {
            continue;
        };
        let first_argument = argument;
        let Some(argument) = path_argument(&toks, &file, argument) else { continue; };
        let method = (argument > first_argument && toks[first_argument].s != "null")
            .then(|| toks[argument - 2].s.to_owned());
        let Some(token) = toks.get(argument) else { continue; };
        if toks.get(argument + 1).is_none_or(|t| t.s != ")")
            || !token.s.starts_with('"')
            || !token.s.ends_with('"')
        {
            continue;
        }
        let raw = &token.s[1..token.s.len() - 1];
        if literal_path(raw) || raw.strip_prefix('/').is_some_and(|p| p.is_empty() || literal_path(p)) {
            out.push(Declaration {
                from: token.at + 1,
                to: token.end() - 1,
                api,
                path: raw.into(),
                method,
            });
        }
    }
    out
}
pub(crate) fn issues(doc: &Document) -> Vec<Issue> {
    if !host::setting_bool("springSecurity.checkPathPatterns").unwrap_or(true) { return Vec::new(); }
    declarations(doc).into_iter().filter(|d| !d.path.starts_with('/'))
        .map(|d| Issue {from:d.from,to:d.to,api:d.api}).collect()
}
pub(crate) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    issues(doc).into_iter().map(|issue| problem(&doc.text,issue.from,issue.to,Severity::Warning,CODE,
        format!("The explicit {} requires its path literal to start with `/`. The project's implicit matcher strategy is not inferred.",issue.api))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn doc(expression: &str) -> Document {
        Document {
            path: "/project/Security.java".into(),
            language: "java".into(),
            version: 3,
            text: format!(
                "import {FQN}; class Security {{ Object matcher() {{ return {expression}; }} }}"
            ),
        }
    }
    #[test]
    fn explicit_http_method_overloads_keep_version_contract_and_literal_span() {
        let _h = openidea_plugin::mock::MockHost::install();
        for expression in [
            "PathPatternRequestMatcher.withDefaults().matcher(null, \"api/users\")",
            "PathPatternRequestMatcher.pathPattern(null, \"api/users\")",
            "PathPatternRequestMatcher.withDefaults().matcher(HttpMethod.GET, \"api/users\")",
            "PathPatternRequestMatcher.pathPattern(org.springframework.http.HttpMethod.POST, \"api/users\")",
        ] {
            let mut d = doc(expression);
            d.text = format!("import org.springframework.http.HttpMethod; {}",d.text);
            let found=issues(&d);
            assert_eq!(found.len(),1,"{expression}");
            assert_eq!(&d.text[found[0].from..found[0].to],"api/users");
        }
    }
    #[test]
    fn method_overload_shadowing_unknown_arguments_and_arity_stay_quiet() {
        let _h = openidea_plugin::mock::MockHost::install();
        for expression in [
            "PathPatternRequestMatcher.pathPattern(method, \"api/users\")",
            "PathPatternRequestMatcher.pathPattern(HttpMethod.GET, \"api/users\")",
            "PathPatternRequestMatcher.pathPattern(null, \"api/users\", extra)",
            "PathPatternRequestMatcher.pathPattern(null, \"api/users\" + suffix)",
        ] {assert!(issues(&doc(expression)).is_empty(),"{expression}");}
        let mut d=doc("PathPatternRequestMatcher.pathPattern(HttpMethod.GET, \"api/users\")");
        d.text=format!("import org.springframework.http.HttpMethod; {}",d.text.replace("Object matcher()","Object HttpMethod; Object matcher()"));
        assert!(issues(&d).is_empty());
    }
    #[test]
    fn explicit_65_builder_and_70_factory_have_distinct_contracts() {
        let _h = openidea_plugin::mock::MockHost::install();
        for (expression, api) in [
            (
                "PathPatternRequestMatcher.withDefaults().matcher(\"api/users\")",
                "6.5+",
            ),
            ("PathPatternRequestMatcher.pathPattern(\"api/**\")", "7.0+"),
        ] {
            let d = doc(expression);
            let found = issues(&d);
            assert_eq!(found.len(), 1);
            assert!(found[0].api.contains(api));
            assert!(!d.text[found[0].from..found[0].to].starts_with('/'));
        }
        let d = doc(&format!("{FQN}.withDefaults().matcher(\"api\")"));
        assert_eq!(issues(&d).len(), 1);
    }
    #[test]
    fn implicit_legacy_custom_dynamic_escaped_and_malformed_stay_quiet() {
        let _h = openidea_plugin::mock::MockHost::install();
        for expression in [
            "http.requestMatchers(\"api\")",
            "http.antMatchers(\"api\")",
            "other.matcher(\"api\")",
            "PathPatternRequestMatcher.withPathPatternParser(parser).matcher(\"api\")",
            "PathPatternRequestMatcher.withDefaults().basePath(\"/x\").matcher(\"api\")",
            "PathPatternRequestMatcher.withDefaults().matcher(path)",
            "PathPatternRequestMatcher.pathPattern(\"api\" + suffix)",
            "PathPatternRequestMatcher.pathPattern(\"api\\nusers\")",
            "PathPatternRequestMatcher.pathPattern(\"api/**/users\")",
            "PathPatternRequestMatcher.pathPattern(\"api/${value}\")",
            "PathPatternRequestMatcher.pathPattern(\"\")",
            "PathPatternRequestMatcher.pathPattern(\"/api\")",
        ] {
            assert!(issues(&doc(expression)).is_empty(), "{expression}");
        }
    }
    #[test]
    fn import_shadowing_inheritance_comments_and_quoted_source_stay_quiet() {
        let _h = openidea_plugin::mock::MockHost::install();
        let d = doc("PathPatternRequestMatcher.pathPattern(\"api\")");
        for text in [
            d.text.replace(FQN, "foreign.PathPatternRequestMatcher"),
            d.text.replace(
                FQN,
                "org.springframework.security.web.servlet.util.matcher.*",
            ),
            d.text
                .replace("class Security", "class Security extends Parent"),
            d.text.replace(
                "Object matcher()",
                "Object PathPatternRequestMatcher; Object matcher()",
            ),
            d.text
                .replace("class Security", "class PathPatternRequestMatcher"),
            format!("{} /* unfinished", d.text),
            d.text.replace(
                "return PathPatternRequestMatcher.pathPattern(\"api\");",
                "return null; // PathPatternRequestMatcher.pathPattern(\"api\");\n",
            ),
            d.text.replace(
                "return PathPatternRequestMatcher.pathPattern(\"api\");",
                "return \"PathPatternRequestMatcher.pathPattern(\\\"api\\\")\";",
            ),
        ] {
            assert!(issues(&Document { text, ..d.clone() }).is_empty());
        }
    }
    #[test]
    fn fully_qualified_package_root_shadow_and_unicode_escape_refuse() {
        let _h = openidea_plugin::mock::MockHost::install();
        let d = doc(&format!("{FQN}.pathPattern(\"api\")"));
        assert_eq!(issues(&d).len(), 1);
        let text = d
            .text
            .replace("Object matcher()", "Object org; Object matcher()");
        assert!(issues(&Document { text, ..d.clone() }).is_empty());
        let text = d.text.replace("api", "a\\u0070i");
        assert!(issues(&Document { text, ..d }).is_empty());
    }
    #[test]
    fn current_text_utf16_and_every_prefix_are_bounded() {
        let _h = openidea_plugin::mock::MockHost::install();
        let mut d = doc("PathPatternRequestMatcher.pathPattern(\"api/users\")");
        d.text = d.text.replace("return ", "/* 😀 */ return ");
        let issue = issues(&d)[0];
        let range = range(&d.text, issue.from, issue.to);
        assert_eq!(offset(&d.text, range.start), Some(issue.from));
        assert_eq!(offset(&d.text, range.end), Some(issue.to));
        for end in d
            .text
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(d.text.len()))
        {
            let partial = Document {
                text: d.text[..end].into(),
                ..d.clone()
            };
            assert!(
                issues(&partial)
                    .iter()
                    .all(|i| i.from < i.to && i.to <= end)
            );
        }
        d.text = d.text.replace("\"api/users\"", "\"/api/users\"");
        d.version += 1;
        assert!(issues(&d).is_empty());
    }
    #[test]
    fn feature_switch_language_size_and_literal_grammar_gate() {
        let h = openidea_plugin::mock::MockHost::install();
        let mut d = doc("PathPatternRequestMatcher.pathPattern(\"api\")");
        h.set_setting("springSecurity.checkPathPatterns", "false");
        assert!(issues(&d).is_empty());
        h.set_setting("springSecurity.checkPathPatterns", "true");
        d.language = "kotlin".into();
        assert!(issues(&d).is_empty());
        d.language = "java".into();
        d.text.push_str(&" ".repeat(MAX_TEXT));
        assert!(issues(&d).is_empty());
        for s in ["../admin", "api//x", "api/{id}", "api?x", "api%2Fx"] {
            assert!(!literal_path(s));
        }
        assert!(literal_path("api/*/item") && literal_path("api/**") && literal_path("api/"));
    }
}

#[cfg(test)]
mod scope_regressions {
    use super::*;
    #[test]
    fn static_fields_and_qualified_anonymous_inherited_scope_are_unknown() {
        let _h = openidea_plugin::mock::MockHost::install();
        let base = format!(
            "import {FQN}; class Security {{ Object f() {{ return PathPatternRequestMatcher.pathPattern(\"api\"); }} }}"
        );
        for text in [format!("import static custom.Fields.PathPatternRequestMatcher; {base}"),base.replace("return PathPatternRequestMatcher", "class Local extends Unknown {} return PathPatternRequestMatcher"),base.replace("return PathPatternRequestMatcher.pathPattern(\"api\");", "return new custom.Unknown<>() { Object f() { return PathPatternRequestMatcher.pathPattern(\"api\"); } };"),base.replace("Object f()", "Object f(PathPatternRequestMatcher PathPatternRequestMatcher)")] {
            let d=Document{path:"/project/Security.java".into(),language:"java".into(),version:1,text};
            assert!(issues(&d).is_empty(),"{}",d.text);
        }
    }
}
