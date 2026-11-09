//! Explicit JDK bundle/key source contexts. These are read-only conventional
//! source candidates, not resolved runtime/classpath identity or Rename symbols.
use crate::{
    props,
    usages::{Declaration, KeyIndex},
};
use openidea_plugin::providers::Document;
use spring_text::java::{self, Tok};
use std::ops::Range;
const MAX_SOURCE: usize = 128 << 10;
pub(crate) struct Context {
    pub span: Range<usize>,
    pub bundle: String,
    prefix: String,
}
fn plain<'a>(t: &Tok<'a>, kotlin: bool) -> Option<&'a str> {
    if t.s.starts_with("\"\"\"") {
        return None;
    }
    let value = t.s.strip_prefix('"')?.strip_suffix('"')?;
    (!value.contains('\\') && !value.chars().any(char::is_control) && (!kotlin || !value.contains('$'))).then_some(value)
}
fn normalized(path: &str) -> bool {
    path.starts_with('/') && !path.contains(['\\', '\n', '\r']) && !path.split('/').any(|p| matches!(p, "." | ".."))
}
fn bundle_name(base: &str) -> bool {
    !base.is_empty()
        && base.len() <= 512
        && base.split('.').all(|part| {
            !part.is_empty()
                && part.as_bytes().first().is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
}
fn owned_context(doc: &Document, kotlin: bool, base: &str, span: Range<usize>) -> Option<Context> {
    if !bundle_name(base) {
        return None;
    }
    let roots = if kotlin { ["/src/main/kotlin/", "/src/test/kotlin/"] } else { ["/src/main/java/", "/src/test/java/"] };
    let mut owners = roots
        .into_iter()
        .filter_map(|marker| doc.path.rsplit_once(marker).map(|(root, _)| (root, if marker.contains("/test/") { "test" } else { "main" })));
    let (module, set) = owners.next()?;
    if owners.next().is_some() {
        return None;
    }
    Some(Context { span, bundle: base.into(), prefix: format!("{module}/src/{set}/resources/{}", base.replace('.', "/")) })
}
fn ident(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic() || b == b'_') && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
/// Recognize an explicit JDK type spelling, not an arbitrary same-named value.
fn jdk_type(toks: &[Tok], from: usize, to: usize) -> bool {
    let Some(slice) = toks.get(from..to) else {
        return false;
    };
    slice.iter().map(|t| t.s).eq(["ResourceBundle"]) || slice.iter().map(|t| t.s).eq(["java", ".", "util", ".", "ResourceBundle"])
}
fn type_position(toks: &[Tok], i: usize, kotlin: bool) -> bool {
    let start = if i >= 4 && toks[i - 4..i].iter().map(|t| t.s).eq(["java", ".", "util", "."]) { i - 4 } else { i };
    if kotlin {
        start >= 3
            && toks[start - 3].s == "val"
            && ident(toks[start - 2].s)
            && toks[start - 1].s == ":"
            && toks.get(i + 1).is_some_and(|t| t.s == "=")
    } else {
        start > 0
            && toks[start - 1].s == "final"
            && toks.get(i + 1).is_some_and(|t| ident(t.s))
            && toks.get(i + 2).is_some_and(|t| t.s == "=")
    }
}
fn ordinary_block(toks: &[Tok], open: usize, kotlin: bool) -> bool {
    let Some(previous) = open.checked_sub(1).and_then(|i| toks.get(i)) else {
        return false;
    };
    if matches!(previous.s, "else" | "try" | "finally" | "do") {
        return true;
    }
    if !kotlin && matches!(previous.s, "{" | ";" | "}") {
        return true;
    }
    if previous.s != ")" {
        return false;
    }
    let mut depth = 1usize;
    for i in (0..open - 1).rev() {
        match toks[i].s {
            ")" => depth += 1,
            "(" => {
                depth -= 1;
                if depth == 0 {
                    return i > 0 && matches!(toks[i - 1].s, "if" | "for" | "while" | "switch" | "catch" | "synchronized");
                }
            }
            _ => {}
        }
    }
    false
}
fn local_context(doc: &Document, toks: &[Tok], f: &java::JavaFile, at: usize, kotlin: bool, short: bool, full: bool) -> Option<Context> {
    let key_call = (0..toks.len()).find(|&i| {
        toks.get(i..i + 6).is_some_and(|s| {
            ident(s[0].s)
                && s[1].s == "."
                && ["getString", "getObject", "getStringArray"].contains(&s[2].s)
                && s[3].s == "("
                && s[5].s == ")"
                && plain(&s[4], kotlin).is_some()
                && s[4].at + 1 <= at
                && at <= s[4].end() - 1
        })
    })?;
    let name = toks[key_call].s;
    if key_call > 0 && matches!(toks[key_call - 1].s, "." | "@" | ":") {
        return None;
    }
    // Require one unambiguous enclosing named function block. Fields, init
    // blocks, constructors, nested functions/classes and expression bodies stay quiet.
    let mut functions: Vec<(usize, usize, usize)> = f
        .types
        .iter()
        .flat_map(|ty| &ty.members)
        .filter(|m| m.kind == java::MemberKind::Method)
        .filter_map(|m| m.body.map(|(from, to)| (m.start, from, to)))
        .filter(|(_, from, to)| *from < toks[key_call].at && toks[key_call].at < *to && doc.text.as_bytes().get(*from) == Some(&b'{'))
        .collect();
    if kotlin {
        let mut depth = 0usize;
        for i in 0..toks.len() {
            if toks[i].s == "fun"
                && depth == 0
                && toks.get(i + 1).is_some_and(|t| ident(t.s))
                && toks.get(i + 2).is_some_and(|t| t.s == "(")
            {
                let close = java::closing(toks, i + 2);
                let mut body = close + 1;
                if toks.get(body).is_some_and(|t| t.s == ":") {
                    body += 1;
                    while toks.get(body).is_some_and(|t| ident(t.s) || matches!(t.s, "." | "?" | "<" | ">" | ",")) {
                        body += 1;
                    }
                }
                if toks.get(body).is_some_and(|t| t.s == "{") {
                    let end = java::closing(toks, body);
                    if body < key_call && key_call < end {
                        functions.push((toks[i].at, toks[body].at, toks[end].end()));
                    }
                }
            }
            match toks[i].s {
                "{" => depth += 1,
                "}" => depth = depth.checked_sub(1)?,
                _ => {}
            }
        }
    }
    if functions.len() != 1 {
        return None;
    }
    let (start, from, to) = functions[0];
    let body_start = toks.iter().position(|t| t.at == from)?;
    let body_end = toks.iter().position(|t| t.end() == to)?;
    if toks.iter().any(|t| start <= t.at && t.at < from && t.s == name)
        || f.types.iter().flat_map(|ty| &ty.members).any(|m| m.kind == java::MemberKind::Field && m.name == name)
    {
        return None;
    }
    if body_end - body_start > 4096 {
        return None;
    }
    let body = &toks[body_start + 1..body_end];
    if body.iter().any(|t| matches!(t.s, "class" | "interface" | "record" | "object" | "fun"))
        || body.windows(2).any(|s| s[0].s == "-" && s[1].s == ">")
    {
        return None;
    }
    // Determine lexical ancestry, rather than borrowing a declaration from a
    // sibling/closed block or another method with the same variable spelling.
    let mut scopes: Vec<Vec<usize>> = Vec::with_capacity(body_end - body_start + 1);
    let mut stack = Vec::new();
    for (i, t) in toks.iter().enumerate().take(body_end + 1).skip(body_start) {
        scopes.push(stack.clone());
        if t.s == "{" {
            if stack.len() == 64 {
                return None;
            }
            stack.push(i);
        } else if t.s == "}" {
            stack.pop();
        }
    }
    if scopes[key_call - body_start].iter().copied().any(|open| open != body_start && !ordinary_block(toks, open, kotlin)) {
        return None;
    }
    let mut declarations = Vec::new();
    for i in body_start + 1..key_call {
        if toks[i].s != if kotlin { "val" } else { "final" } || i == 0 || !matches!(toks[i - 1].s, "{" | ";" | "}") {
            continue;
        }
        let (var, eq) = if kotlin {
            let var = i + 1;
            if toks.get(var).is_none_or(|t| t.s != name) {
                continue;
            }
            let mut eq = var + 1;
            if toks.get(eq).is_some_and(|t| t.s == ":") {
                let ty = eq + 1;
                eq = if jdk_type(toks, ty, ty + 1) {
                    ty + 1
                } else if jdk_type(toks, ty, ty + 5) {
                    ty + 5
                } else {
                    continue;
                };
            }
            (var, eq)
        } else {
            let ty = i + 1;
            let var = if jdk_type(toks, ty, ty + 1) || toks.get(ty).is_some_and(|t| t.s == "var") {
                ty + 1
            } else if jdk_type(toks, ty, ty + 5) {
                ty + 5
            } else {
                continue;
            };
            if toks.get(var).is_none_or(|t| t.s != name) {
                continue;
            }
            (var, var + 1)
        };
        if toks.get(eq).is_none_or(|t| t.s != "=") {
            continue;
        }
        let init = eq + 1;
        let qualifier = if toks.get(init..init + 5).is_some_and(|s| s.iter().map(|t| t.s).eq(["java", ".", "util", ".", "ResourceBundle"]))
        {
            if !full {
                continue;
            }
            init + 4
        } else {
            if !short || toks.get(init).is_none_or(|t| t.s != "ResourceBundle") {
                continue;
            }
            init
        };
        let Some(call) = toks.get(qualifier + 1..qualifier + 6) else {
            continue;
        };
        if !call.iter().take(3).map(|t| t.s).eq([".", "getBundle", "("]) || call[4].s != ")" {
            continue;
        }
        let Some(base) = plain(&call[3], kotlin).filter(|base| bundle_name(base)) else {
            continue;
        };
        let end = qualifier + 5;
        if !kotlin && toks.get(end + 1).is_none_or(|t| t.s != ";") {
            continue;
        }
        if kotlin {
            let next = toks.get(end + 1)?;
            if next.s != ";"
                && next.s != "}"
                && (!doc.text[toks[end].end()..next.at].contains('\n')
                    || !ident(next.s)
                    || matches!(next.s, "as" | "is" | "in" | "else" | "catch" | "finally"))
            {
                continue;
            }
        }
        if !scopes[key_call - body_start].starts_with(&scopes[var - body_start]) {
            continue;
        }
        declarations.push((var, base));
    }
    if declarations.len() != 1 {
        return None;
    }
    let (var, base) = declarations[0];
    // Every occurrence must be the declaration or a direct literal getter.
    // Reassignment, rebinding, aliases, arguments, returns and member-qualified
    // shadowing invalidate the whole function, including changes after this use.
    for i in body_start + 1..body_end {
        if toks[i].s != name || i == var {
            continue;
        }
        let Some(call) = toks.get(i..i + 6) else {
            return None;
        };
        if i < var
            || i > 0 && matches!(toks[i - 1].s, "." | "@" | ":")
            || call[1].s != "."
            || !["getString", "getObject", "getStringArray"].contains(&call[2].s)
            || call[3].s != "("
            || plain(&call[4], kotlin).is_none()
            || call[5].s != ")"
        {
            return None;
        }
    }
    let key = &toks[key_call + 4];
    owned_context(doc, kotlin, base, key.at + 1..key.end() - 1)
}

pub(crate) fn context(doc: &Document, at: usize) -> Option<Context> {
    let kotlin = doc.language == "kotlin";
    if !(doc.language == "java" && doc.path.ends_with(".java") || kotlin && doc.path.ends_with(".kt"))
        || doc.text.len() > MAX_SOURCE
        || !normalized(&doc.path)
        || !doc.text.is_char_boundary(at)
    {
        return None;
    }
    if !kotlin && doc.text.contains("\\u") {
        return None;
    }
    // Nested Kotlin block comments are compiler-aware; the shared token scanner
    // is intentionally non-nesting, so refuse that unsupported context.
    if kotlin {
        let mut tail = doc.text.as_str();
        while let Some(open) = tail.find("/*") {
            let body = &tail[open + 2..];
            let end = body.find("*/")?;
            if body[..end].contains("/*") {
                return None;
            }
            tail = &body[end + 2..];
        }
    }
    let toks = java::tokens(&doc.text);
    if toks.len() > 16384 {
        return None;
    }
    let mut stack = Vec::new();
    for t in &toks {
        match t.s {
            "(" | "[" | "{" => {
                if stack.len() == 64 {
                    return None;
                }
                stack.push(t.s);
            }
            ")" | "]" | "}" => {
                let expected = match t.s {
                    ")" => "(",
                    "]" => "[",
                    _ => "{",
                };
                if stack.pop() != Some(expected) {
                    return None;
                }
            }
            _ => {}
        }
    }
    if !stack.is_empty() {
        return None;
    }
    let f = if kotlin { spring_text::kotlin::parse(&doc.text) } else { java::parse(&doc.text) };
    if f.types.iter().any(|t| t.supers().next().is_some()) || f.imports.len() > 256 || f.imports.iter().any(|i| i.path.ends_with(".*")) {
        return None;
    }
    let in_import = |t: &Tok| f.imports.iter().any(|i| i.at <= t.at && t.at < i.end);
    let imports: Vec<_> = f.imports.iter().filter(|i| !i.is_static && i.path.rsplit('.').next() == Some("ResourceBundle")).collect();
    let imported = imports.len() == 1 && imports[0].path == "java.util.ResourceBundle" && {
        let tail = doc.text[imports[0].end..].split('\n').next().unwrap_or("").trim_start();
        !tail.starts_with("as ")
    };
    let qualifier = |i: usize| {
        toks.get(i).is_some_and(|t| t.s == "ResourceBundle")
            && toks.get(i + 1).is_some_and(|t| t.s == ".")
            && toks.get(i + 2).is_some_and(|t| t.s == "getBundle")
    };
    let short_unshadowed = imported
        && toks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.s == "ResourceBundle" && !in_import(t))
            .all(|(i, _)| qualifier(i) || type_position(&toks, i, kotlin));
    // A local value/type/type-variable named java can shadow the package root.
    // Imports are known declarations; all other java tokens must belong to the
    // exact JDK qualified chain, never a guessed receiver field.
    let java_unshadowed = toks.iter().enumerate().filter(|(_, t)| t.s == "java" && !in_import(t)).all(|(i, _)| {
        toks.get(i..i + 5).is_some_and(|s| s.iter().map(|t| t.s).eq(["java", ".", "util", ".", "ResourceBundle"]))
            && (i == 0 || toks[i - 1].s != ".")
    });
    for i in 0..toks.len() {
        if !qualifier(i) {
            continue;
        }
        let full = i >= 4 && toks[i - 4..i].iter().map(|t| t.s).eq(["java", ".", "util", "."]);
        let start = if full { i - 4 } else { i };
        if start > 0 && matches!(toks[start - 1].s, "." | "@" | "import") {
            continue;
        }
        if full && !java_unshadowed || !full && !short_unshadowed {
            continue;
        }
        let Some(call) = toks.get(i + 3..i + 11) else {
            continue;
        };
        if call[0].s != "("
            || call[2].s != ")"
            || call[3].s != "."
            || !["getString", "getObject", "getStringArray"].contains(&call[4].s)
            || call[5].s != "("
            || call[7].s != ")"
        {
            continue;
        }
        let Some(base) = plain(&call[1], kotlin) else {
            continue;
        };
        let Some(_key) = plain(&call[6], kotlin) else {
            continue;
        };
        if !bundle_name(base) {
            continue;
        }
        let span = call[6].at + 1..call[6].end() - 1;
        if !(span.start <= at && at <= span.end) {
            continue;
        }
        return owned_context(doc, kotlin, base, span);
    }
    local_context(doc, &toks, &f, at, kotlin, short_unshadowed, java_unshadowed)
}
impl Context {
    pub(crate) fn contains(&self, path: &str) -> bool {
        if !path.ends_with(".properties") {
            return false;
        }
        let Some(rest) = path.strip_suffix(".properties").and_then(|p| p.strip_prefix(&self.prefix)) else {
            return false;
        };
        if rest.is_empty() {
            return true;
        }
        let Some(locale) = rest.strip_prefix('_') else {
            return false;
        };
        let test = format!("x_{locale}");
        props::split_locale(&test).0 == "x"
    }
    pub(crate) fn keys<'a>(&self, index: &'a KeyIndex) -> Option<std::collections::BTreeSet<&'a str>> {
        let mut keys = std::collections::BTreeSet::new();
        for (path, names) in &index.file_keys {
            if self.contains(path) {
                for name in names {
                    keys.insert(name.as_str());
                    if keys.len() > 4096 {
                        return None;
                    }
                }
            }
        }
        Some(keys)
    }
    pub(crate) fn is_default(&self, path: &str) -> bool {
        path == format!("{}.properties", self.prefix)
    }
    pub(crate) fn declarations<'a>(&self, index: &'a KeyIndex, key: &str) -> Vec<&'a Declaration> {
        index.keys.get(key).into_iter().flatten().filter(|d| self.contains(&d.location.path)).take(129).collect()
    }
    pub(crate) fn scope_known(&self, index: &KeyIndex, ownership: &crate::usages::SourceOwnership) -> bool {
        let files: Vec<_> = index.files.iter().filter(|p| self.contains(p)).take(129).collect();
        !index.root.is_empty()
            && self.prefix.strip_prefix(&format!("{}/", index.root.trim_end_matches('/'))).is_some()
            && files.len() <= 128
            && files.iter().all(|path| ownership.contains(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Properties;
    use openidea_plugin::{Position, hover::HoverProvider, mock::MockHost, providers::Providers, text};
    fn doc(source: &str, kotlin: bool) -> Document {
        Document {
            path: format!("/project/module/src/main/{}/App.{}", if kotlin { "kotlin" } else { "java" }, if kotlin { "kt" } else { "java" }),
            language: if kotlin { "kotlin" } else { "java" }.into(),
            version: 2,
            text: source.into(),
        }
    }
    fn at(doc: &Document) -> Position {
        text::position(&doc.text, doc.text.rfind("owned.key").unwrap() + 2)
    }
    fn host() -> MockHost {
        let h = MockHost::install();
        for (p, v) in [
            ("module/src/main/resources/pkg/messages.properties", "Right"),
            ("module/src/main/resources/pkg/messages_fr.properties", "French"),
            ("module/src/main/resources/pkg/other.properties", "Wrong bundle"),
            ("module/src/main/resources/application.properties", "Config"),
            ("other/src/main/resources/pkg/messages.properties", "Other module"),
            ("module/src/test/resources/pkg/messages.properties", "Test"),
        ] {
            h.add_file(p, &format!("owned.key={v}\n"));
        }
        h
    }
    #[test]
    fn exact_import_and_fqn_calls_keep_bundle_package_module_and_source_set_ownership() {
        let h = host();
        let mut p = Properties::default();
        p.key_index("/project/module/src/main/java/App.java").unwrap();
        for source in [
            "import java.util.ResourceBundle; class App { String f(){return ResourceBundle.getBundle(\"pkg.messages\").getString(\"owned.key\");}}",
            "class App { Object f(){return java.util.ResourceBundle.getBundle(\"pkg.messages\").getObject(\"owned.key\");}}",
        ] {
            let d = doc(source, false);
            let found = p.definition(&d, at(&d));
            assert_eq!(found.len(), 2, "{source}");
            assert!(found.iter().all(|l| l.path.starts_with("/project/module/src/main/resources/pkg/messages")));
            let hover = p.hover(&d, at(&d)).unwrap();
            assert!(hover.markdown.contains("Right"));
            for foreign in ["Wrong bundle", "Config", "Other module", "Value: Test"] {
                assert!(!hover.markdown.contains(foreign));
            }
        }
        let mut test = doc("java.util.ResourceBundle.getBundle(\"pkg.messages\").getString(\"owned.key\")", false);
        test.path = test.path.replace("/main/", "/test/");
        let found = p.definition(&test, at(&test));
        assert_eq!(found.len(), 1);
        assert!(found[0].path.contains("/src/test/resources/"));
        assert!(h.process_runs().is_empty());
    }
    #[test]
    fn quarkus_boot_arbitrary_strings_unknown_helpers_and_dynamic_bundles_are_quiet_even_warm() {
        let _h = host();
        let mut p = Properties::default();
        p.key_index("/project/module/src/main/java/App.java").unwrap();
        for source in [
            "@ConfigProperty(name=\"owned.key\") String field;",
            "@Value(\"owned.key\") String field;",
            "String s=\"owned.key\";",
            "bundle.getString(\"owned.key\");",
            "messageSource.getMessage(\"owned.key\",null,locale);",
            "helper.message(\"owned.key\");",
            "ResourceBundle.getBundle(\"pkg.messages\").getString(\"owned.key\");",
            "java.util.ResourceBundle.getBundle(BASE).getString(\"owned.key\");",
            "java.util.ResourceBundle.getBundle(\"pkg.messages\",locale).getString(\"owned.key\");",
            "java.util.ResourceBundle.getBundle(\"pkg.other\").getString(\"unavailable\");",
        ] {
            let d = doc(source, false);
            let pos = text::position(&d.text, d.text.rfind('"').unwrap().saturating_sub(2));
            assert!(p.definition(&d, pos).is_empty(), "{source}");
            assert!(p.completion(&d, pos, None).is_empty(), "{source}");
            assert!(p.hover(&d, pos).is_none(), "{source}");
        }
    }
    #[test]
    fn foreign_wildcard_alias_shadow_and_masked_contexts_do_not_acquire_jdk_identity() {
        for source in [
            "import other.ResourceBundle; ResourceBundle.getBundle(\"pkg.messages\").getString(\"owned.key\");",
            "import java.util.*; ResourceBundle.getBundle(\"pkg.messages\").getString(\"owned.key\");",
            "import java.util.ResourceBundle; class ResourceBundle {} ResourceBundle.getBundle(\"pkg.messages\").getString(\"owned.key\");",
            "class java {} java.util.ResourceBundle.getBundle(\"pkg.messages\").getString(\"owned.key\");",
            "class C extends Unknown { String f(){return java.util.ResourceBundle.getBundle(\"pkg.messages\").getString(\"owned.key\");}}",
            "// java.util.ResourceBundle.getBundle(\"pkg.messages\").getString(\"owned.key\");",
            "/* java.util.ResourceBundle.getBundle(\"pkg.messages\").getString(\"owned.key\"); */",
            "String s=\"java.util.ResourceBundle.getBundle(\\\"pkg.messages\\\").getString(\\\"owned.key\\\")\";",
        ] {
            let d = doc(source, false);
            assert!(context(&d, text::offset(&d.text, at(&d)).unwrap()).is_none(), "{source}");
        }
        let d = doc("import java.util.ResourceBundle as Other\nResourceBundle.getBundle(\"pkg.messages\").getString(\"owned.key\")", true);
        assert!(context(&d, text::offset(&d.text, at(&d)).unwrap()).is_none());
    }
    #[test]
    fn kotlin_utf16_current_text_bounds_and_no_rename_exports_are_explicit() {
        let _h = host();
        let mut p = Properties::default();
        let d = doc(
            "import java.util.ResourceBundle\n// 😀\nval s=ResourceBundle.getBundle(\"pkg.messages\").getStringArray(\"owned.key\")",
            true,
        );
        p.key_index(&d.path).unwrap();
        assert_eq!(p.definition(&d, at(&d)).len(), 2);
        let c = context(&d, text::offset(&d.text, at(&d)).unwrap()).unwrap();
        let range = crate::props::range_of(&d.text, &c.span);
        assert_eq!(&d.text[text::offset(&d.text, range.start).unwrap()..text::offset(&d.text, range.end).unwrap()], "owned.key");
        let changed = Document { text: d.text.replace("pkg.messages", "pkg.unknown"), version: 3, ..d.clone() };
        assert!(p.definition(&changed, at(&changed)).is_empty());
        let huge = Document { text: format!("{}{}", " ".repeat(MAX_SOURCE), d.text), ..d.clone() };
        assert!(context(&huge, huge.text.rfind("owned.key").unwrap() + 1).is_none());
        let wrong = Document { language: "java".into(), ..d };
        assert!(p.definition(&wrong, at(&wrong)).is_empty());
        assert!(!include_str!("../plugin.toml").contains("contributes.documentIndexers"));
    }
    #[test]
    fn exact_quarkus_annotation_never_borrows_a_proven_bundle_call_in_the_same_document() {
        let h = host();
        h.add_file("module/src/main/resources/application.properties", "owned.quarkus.key=framework-value\n");
        h.add_file("module/src/main/resources/pkg/messages.properties", "owned.quarkus.key=bundle-value\n");
        let source = "import org.eclipse.microprofile.config.inject.ConfigProperty; class C { @ConfigProperty(name=\"owned.quarkus.key\") String value; String f(){return java.util.ResourceBundle.getBundle(\"pkg.messages\").getString(\"owned.quarkus.key\");} }";
        let d = doc(source, false);
        let mut p = Properties::default();
        p.key_index(&d.path).unwrap();
        let annotation = text::position(source, source.find("owned.quarkus.key").unwrap() + 2);
        assert!(p.definition(&d, annotation).is_empty());
        assert!(p.completion(&d, annotation, None).is_empty());
        assert!(p.hover(&d, annotation).is_none());
        let key = text::position(source, source.rfind("owned.quarkus.key").unwrap() + 2);
        let found = p.definition(&d, key);
        assert_eq!(found.len(), 1);
        assert!(found[0].path.ends_with("pkg/messages.properties"));
    }
    #[test]
    fn unicode_literal_spans_and_explicit_basename_locale_suffixes_are_exact() {
        let h = host();
        h.add_file("module/src/main/resources/pkg/messages_en.properties", "naïve.😀=default\n");
        h.add_file("module/src/main/resources/pkg/messages_en_fr.properties", "naïve.😀=translated\n");
        let d = doc("// 😀\njava.util.ResourceBundle.getBundle(\"pkg.messages_en\").getString(\"naïve.😀\")", false);
        let at = text::position(&d.text, d.text.find("naïve").unwrap() + 2);
        let mut p = Properties::default();
        p.key_index(&d.path).unwrap();
        assert_eq!(p.definition(&d, at).len(), 2);
        let hover = p.hover(&d, at).unwrap();
        let span = hover.range.unwrap();
        assert_eq!(&d.text[text::offset(&d.text, span.start).unwrap()..text::offset(&d.text, span.end).unwrap()], "naïve.😀");
        let completion = p.completion(&d, text::position(&d.text, d.text.find("naïve").unwrap() + "naïve.😀".len()), None);
        assert_eq!(completion.len(), 1);
        assert!(completion[0].detail.as_ref().unwrap().ends_with("default"));
    }
    #[test]
    fn completion_does_not_insert_unescaped_java_or_kotlin_string_syntax() {
        let h = host();
        h.add_file(
            "module/src/main/resources/pkg/messages.properties",
            "safe=ok\nhas\\\"quote=value\nhas\\\\slash=value\nprice$tag=value\n",
        );
        let mut p = Properties::default();
        p.key_index("/project/module/src/main/java/App.java").unwrap();
        for kotlin in [false, true] {
            let d = doc("java.util.ResourceBundle.getBundle(\"pkg.messages\").getString(\"\")", kotlin);
            let pos = text::position(&d.text, d.text.rfind("\"\"").unwrap() + 1);
            let items = p.completion(&d, pos, None);
            assert!(items.iter().any(|i| i.label == "safe"));
            assert!(items.iter().all(|i| !i.label.contains(['"', '\\']) && (!kotlin || !i.label.contains('$'))));
        }
    }
    #[test]
    fn final_java_bundle_locals_reuse_exact_cached_bundle_ownership() {
        let _h = host();
        let mut p = Properties::default();
        p.key_index("/project/module/src/main/java/App.java").unwrap();
        for declaration in [
            "final ResourceBundle bundle = ResourceBundle.getBundle(\"pkg.messages\");",
            "final java.util.ResourceBundle bundle = java.util.ResourceBundle.getBundle(\"pkg.messages\");",
            "final var bundle = java.util.ResourceBundle.getBundle(\"pkg.messages\");",
        ] {
            for getter in ["getString", "getObject", "getStringArray"] {
                let d = doc(
                    &format!(
                        "import java.util.ResourceBundle; class App {{ Object f(){{ {declaration} return bundle.{getter}(\"owned.key\"); }} }}"
                    ),
                    false,
                );
                assert_eq!(p.definition(&d, at(&d)).len(), 2, "{}", d.text);
                assert!(p.hover(&d, at(&d)).is_some());
                assert!(p.completion(&d, at(&d), None).iter().any(|item| item.label == "owned.key"));
            }
        }
    }
    #[test]
    fn kotlin_val_inferred_typed_and_top_level_function_contexts_are_proven() {
        let _h = host();
        let mut p = Properties::default();
        p.key_index("/project/module/src/main/kotlin/App.kt").unwrap();
        for source in [
            "import java.util.ResourceBundle\nfun f(): String { val bundle = ResourceBundle.getBundle(\"pkg.messages\")\nreturn bundle.getString(\"owned.key\") }",
            "import java.util.ResourceBundle\nclass App { fun f(): Any { val bundle: ResourceBundle = ResourceBundle.getBundle(\"pkg.messages\")\nreturn bundle.getObject(\"owned.key\") } }",
            "fun f() { val bundle: java.util.ResourceBundle = java.util.ResourceBundle.getBundle(\"pkg.messages\"); bundle.getStringArray(\"owned.key\") }",
        ] {
            let d = doc(source, true);
            assert_eq!(p.definition(&d, at(&d)).len(), 2, "{source}");
        }
    }
    #[test]
    fn nested_lexical_blocks_keep_or_reject_declarations_by_ancestry() {
        let _h = host();
        let d = doc(
            "class App { String f(){ final var bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); if(true){ return bundle.getString(\"owned.key\"); } return null; }}",
            false,
        );
        assert!(context(&d, text::offset(&d.text, at(&d)).unwrap()).is_some());
        for source in [
            "class App { String f(){ { final var bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); } return bundle.getString(\"owned.key\"); }}",
            "class App { void f(){ final var bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); } String g(){ return bundle.getString(\"owned.key\"); }}",
        ] {
            let d = doc(source, false);
            assert!(context(&d, text::offset(&d.text, at(&d)).unwrap()).is_none(), "{source}");
        }
    }
    #[test]
    fn assignment_shadow_alias_parameter_field_and_unknown_loaders_stay_quiet() {
        let _h = host();
        for source in [
            "class App { Object f(){ ResourceBundle bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); return bundle.getString(\"owned.key\"); }}",
            "class App { Object f(){ final var bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); bundle=other; return bundle.getString(\"owned.key\"); }}",
            "class App { Object f(){ final var bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); Object alias=bundle; return bundle.getString(\"owned.key\"); }}",
            "class App { Object f(){ final var bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); Object alias=bundle; return alias.getString(\"owned.key\"); }}",
            "class App { Object f(Object bundle){ final var bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); return bundle.getString(\"owned.key\"); }}",
            "class App { Object bundle; Object f(){ final var bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); return bundle.getString(\"owned.key\"); }}",
            "class App { Object f(){ final var bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); { Object bundle=other; } return bundle.getString(\"owned.key\"); }}",
            "class App { Object f(){ final var bundle=java.util.ResourceBundle.getBundle(BASE); return bundle.getString(\"owned.key\"); }}",
            "class App { Object f(){ final var bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\",locale); return bundle.getString(\"owned.key\"); }}",
            "class App { Object f(){ final var bundle=helper.getBundle(\"pkg.messages\"); return bundle.getString(\"owned.key\"); }}",
            "class App { Object f(){ final var bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); consume(bundle); return bundle.getString(\"owned.key\"); }}",
            "class App { Object f(){ final var bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); Runnable r=()->bundle.getString(\"owned.key\"); return r; }}",
        ] {
            let d = doc(source, false);
            assert!(context(&d, text::offset(&d.text, at(&d)).unwrap()).is_none(), "{source}");
        }
    }
    #[test]
    fn mutable_kotlin_fields_aliases_and_dynamic_types_stay_quiet() {
        let _h = host();
        for source in [
            "fun f(){ var bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); bundle.getString(\"owned.key\") }",
            "val bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\")\nfun f(){ bundle.getString(\"owned.key\") }",
            "class App { val bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); fun f(){ bundle.getString(\"owned.key\") } }",
            "fun f(bundle: Any){ val bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); bundle.getString(\"owned.key\") }",
            "fun f(){ val bundle: Any=java.util.ResourceBundle.getBundle(\"pkg.messages\"); bundle.getString(\"owned.key\") }",
            "fun f(){ val bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); val alias=bundle; bundle.getString(\"owned.key\") }",
            "fun f(){ val bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\").also { println(it) }; bundle.getString(\"owned.key\") }",
        ] {
            let d = doc(source, true);
            assert!(context(&d, text::offset(&d.text, at(&d)).unwrap()).is_none(), "{source}");
        }
    }
    #[test]
    fn local_sources_are_current_utf16_bounded_and_cache_only() {
        let _h = host();
        let d = doc(
            "class App { Object f(){ final var bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); /*😀*/return bundle.getString(\"owned.key\"); }}",
            false,
        );
        let mut cold = Properties::default();
        assert!(cold.definition(&d, at(&d)).is_empty());
        assert!(cold.index.is_none());
        let mut p = Properties::default();
        p.key_index(&d.path).unwrap();
        assert_eq!(p.definition(&d, at(&d)).len(), 2);
        let context = context(&d, text::offset(&d.text, at(&d)).unwrap()).unwrap();
        let span = crate::props::range_of(&d.text, &context.span);
        assert_eq!(span.start.character, d.text[..context.span.start].encode_utf16().count() as u32);
        let changed = Document { text: d.text.replace("pkg.messages", "pkg.unknown"), version: 3, ..d.clone() };
        assert!(p.definition(&changed, at(&changed)).is_empty());
        let huge = doc(
            &format!(
                "class App {{ Object f(){{ final var bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\");{}return bundle.getString(\"owned.key\"); }} }}",
                "x();".repeat(1500)
            ),
            false,
        );
        assert!(super::context(&huge, text::offset(&huge.text, at(&huge)).unwrap()).is_none());
    }
    #[test]
    fn unknown_lambda_receivers_anonymous_classes_and_nested_functions_stay_quiet() {
        let _h = host();
        for (source, kotlin) in [
            (
                "fun f(){ val bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); with(other){ bundle.getString(\"owned.key\") }}",
                true,
            ),
            ("fun f(){ val bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); run { bundle.getString(\"owned.key\") }}", true),
            (
                "class App { Object f(){ final var bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); return new Unknown(){ Object g(){return bundle.getString(\"owned.key\");}}; }}",
                false,
            ),
            (
                "fun f(){ val bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); fun nested(){ bundle.getString(\"owned.key\") }}",
                true,
            ),
            (
                "class App { Object f(){ final var bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); Object s=bundle.getString(\"owned.key\"); bundle=other; return s; }}",
                false,
            ),
        ] {
            let d = doc(source, kotlin);
            assert!(context(&d, text::offset(&d.text, at(&d)).unwrap()).is_none(), "{source}");
        }
        let d = doc(
            "fun f(){ val bundle=java.util.ResourceBundle.getBundle(\"pkg.messages\"); if(true){ bundle.getString(\"owned.key\") }}",
            true,
        );
        assert!(context(&d, text::offset(&d.text, at(&d)).unwrap()).is_some());
    }
}
