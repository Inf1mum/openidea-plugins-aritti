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
    pub indirect: bool,
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
    Some(Context { span, bundle: base.into(), indirect: false, prefix: format!("{module}/src/{set}/resources/{}", base.replace('.', "/")) })
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

fn eligible(doc: &Document) -> bool {
    let kotlin = doc.language == "kotlin";
    (doc.language == "java" && doc.path.ends_with(".java") || kotlin && doc.path.ends_with(".kt"))
        && doc.text.len() <= MAX_SOURCE
        && normalized(&doc.path)
}
/// Tokens and declarations of a bounded, balanced source with no supertypes
/// and no star imports; anything else is outside the proven subset.
fn parsed(doc: &Document) -> Option<(Vec<Tok<'_>>, java::JavaFile)> {
    let kotlin = doc.language == "kotlin";
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
    Some((toks, f))
}

pub(crate) fn context(doc: &Document, at: usize) -> Option<Context> {
    let kotlin = doc.language == "kotlin";
    if !eligible(doc) || !doc.text.is_char_boundary(at) {
        return None;
    }
    let (toks, f) = parsed(doc)?;
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
    if let Some(context) = constant_context(doc, &toks, at, kotlin, &qualifier, short_unshadowed, java_unshadowed) {
        return Some(context);
    }
    if let Some(context) = annotated(doc, &toks, &f, kotlin, Some(at)).into_iter().next() {
        return Some(context);
    }
    local_context(doc, &toks, &f, at, kotlin, short_unshadowed, java_unshadowed)
}
/// A same-file constant (`static final String K = "x"` / `const val K = "x"`)
/// whose every other mention is the sole argument of one proven direct
/// `ResourceBundle.getBundle("b").getString(K)` call. Anything else stays quiet.
fn constant_context(
    doc: &Document,
    toks: &[Tok],
    at: usize,
    kotlin: bool,
    qualifier: &dyn Fn(usize) -> bool,
    short: bool,
    full: bool,
) -> Option<Context> {
    let lit = toks.iter().position(|t| t.at < at && at < t.end() && plain(t, kotlin).is_some())?;
    let texts = |from: usize, to: usize| toks.get(from..to).map(|s| s.iter().map(|t| t.s).collect::<Vec<_>>());
    let name = if kotlin {
        match (lit.checked_sub(4).and_then(|f| texts(f, lit)), lit.checked_sub(6).and_then(|f| texts(f, lit))) {
            (Some(s), _) if s[0] == "const" && s[1] == "val" && s[3] == "=" => lit - 2,
            (_, Some(s)) if s[0] == "const" && s[1] == "val" && s[3] == ":" && s[4] == "String" && s[5] == "=" => lit - 4,
            _ => return None,
        }
    } else {
        let s = lit.checked_sub(5).and_then(|f| texts(f, lit))?;
        if s[..3] != ["static", "final", "String"] || s[4] != "=" {
            return None;
        }
        lit - 2
    };
    if !ident(toks[name].s) || toks.get(lit + 1).is_some_and(|t| !matches!(t.s, ";" | "}") && (!kotlin || !doc.text[toks[lit].end()..t.at].contains('\n'))) {
        return None;
    }
    let mut base = None;
    for i in (0..toks.len()).filter(|&i| toks[i].s == toks[name].s && i != name) {
        let q = i.checked_sub(9)?;
        let call = toks.get(q..=i + 1)?;
        let qualified = q >= 4 && toks[q - 4..q].iter().map(|t| t.s).eq(["java", ".", "util", "."]);
        let start = if qualified { q - 4 } else { q };
        if !qualifier(q)
            || start > 0 && matches!(toks[start - 1].s, "." | "@" | "import")
            || !(if qualified { full } else { short })
            || call[3].s != "("
            || call[5].s != ")"
            || call[6].s != "."
            || !["getString", "getObject", "getStringArray"].contains(&call[7].s)
            || call[8].s != "("
            || call[10].s != ")"
        {
            return None;
        }
        let this = plain(&call[4], kotlin).filter(|b| bundle_name(b))?;
        if base.replace(this).is_some_and(|b| b != this) {
            return None;
        }
    }
    let mut context = owned_context(doc, kotlin, base?, toks[lit].at + 1..toks[lit].end() - 1)?;
    context.indirect = true;
    Some(context)
}

const PROPERTY_KEY: &str = "org.jetbrains.annotations.PropertyKey";

struct Decl {
    owner: String,
    owner_span: Range<usize>,
    name: Option<String>,
    arity: usize,
    index: usize,
    bundle: String,
}
/// The bundle of an exact, unshadowed `@PropertyKey(resourceBundle = "b")` on a `String` parameter.
fn property_key_bundle(text: &str, f: &java::JavaFile, p: &java::Param) -> Option<String> {
    let marked: Vec<_> = p.annotations.iter().filter(|a| a.name == "PropertyKey").collect();
    let [a] = marked[..] else {
        return None;
    };
    let alias = |i: &java::Import| text[i.end..].split('\n').next().unwrap_or("").trim_start().starts_with("as ");
    let named: Vec<_> = f.imports.iter().filter(|i| i.path.rsplit('.').next() == Some("PropertyKey")).collect();
    let spelled = match &text[a.at + 1..a.name_at] {
        "" => matches!(named[..], [i] if i.path == PROPERTY_KEY && !i.is_static && !alias(i)),
        "org.jetbrains.annotations." => named.iter().all(|i| i.path == PROPERTY_KEY && !i.is_static && !alias(i)),
        _ => false,
    };
    if !spelled
        || f.types.iter().any(|t| t.name == "PropertyKey")
        || f.imports.iter().any(|i| text[i.end..].split('\n').next().unwrap_or("").trim_start().starts_with("as PropertyKey"))
        || !p.ty.args.is_empty()
        || p.ty.array
        || !["String", "java.lang.String"].contains(&p.ty.text.trim_end_matches('?'))
        || a.args.len() != 1
    {
        return None;
    }
    let arg = a.arg("resourceBundle")?;
    let [s] = &arg.strings[..] else {
        return None;
    };
    let exact = text.get(s.at - 1..s.end + 1) == Some(arg.text.as_str()) && text.get(s.at..s.end) == Some(s.value.as_str());
    (exact && bundle_name(&s.value)).then(|| s.value.clone())
}
/// A parameter list without varargs or defaults, so arity equals position count.
fn fixed_arity(toks: &[Tok], name_at: usize) -> bool {
    let Some(i) = toks.iter().position(|t| t.at == name_at) else {
        return false;
    };
    if toks.get(i + 1).is_none_or(|t| t.s != "(") || i > 0 && toks[i - 1].s == "." {
        return false;
    }
    let list = &toks[i + 2..java::closing(toks, i + 1).max(i + 2)];
    let mut depth = 0i32;
    for (k, t) in list.iter().enumerate() {
        match t.s {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth -= 1,
            "=" | "vararg" if depth == 0 => return false,
            "." if depth == 0 && list.get(k + 1).is_some_and(|n| n.s == ".") => return false,
            _ => {}
        }
    }
    true
}
/// Top-level, non-overloaded methods/constructors with exactly one annotated parameter.
fn declarations(text: &str, toks: &[Tok], f: &java::JavaFile) -> Vec<Decl> {
    let mut out = Vec::new();
    for t in f.types.iter().filter(|t| t.outer.is_none()) {
        for m in t.members.iter().filter(|m| m.kind != java::MemberKind::Field) {
            let marked: Vec<_> = m.params.iter().enumerate().filter(|(_, p)| p.annotation("PropertyKey").is_some()).collect();
            let [(index, param)] = marked[..] else {
                continue;
            };
            let ctor = m.kind == java::MemberKind::Constructor;
            let twins = t.members.iter().filter(|o| o.kind == m.kind && (ctor || o.name == m.name)).count();
            let Some(bundle) = property_key_bundle(text, f, param) else {
                continue;
            };
            if twins == 1 && fixed_arity(toks, m.at) {
                out.push(Decl {
                    owner: t.name.clone(),
                    owner_span: t.start..t.end,
                    name: (!ctor).then(|| m.name.clone()),
                    arity: m.params.len(),
                    index,
                    bundle,
                });
            }
        }
    }
    out
}
fn module_of(path: &str) -> Option<(&str, &'static str)> {
    let marks = [("/src/main/java/", "main"), ("/src/main/kotlin/", "main"), ("/src/test/java/", "test"), ("/src/test/kotlin/", "test")];
    let mut found = marks.iter().filter_map(|(m, set)| path.rsplit_once(m).map(|(root, _)| (root, *set)));
    let owner = found.next()?;
    found.next().is_none().then_some(owner)
}
/// Declarations of one project class found by import/package path in the same module; exactly one file or none.
fn external_declarations(doc: &Document, f: &java::JavaFile, ty: &str) -> Vec<Decl> {
    let qualified = f.resolve(ty);
    let (Some((module, set)), Some(project)) = (module_of(&doc.path), openidea_plugin::host::project_of(&doc.path)) else {
        return Vec::new();
    };
    let relative = qualified.replace('.', "/");
    let sets: &[&str] = if set == "test" { &["test", "main"] } else { &["main"] };
    let mut hits = Vec::new();
    for set in sets {
        for (dir, ext) in [("java", "java"), ("kotlin", "kt")] {
            let path = format!("{module}/src/{set}/{dir}/{relative}.{ext}");
            if let Ok(text) = project.read_file(&path) {
                hits.push((path, text));
            }
        }
    }
    let [(path, text)] = &hits[..] else {
        return Vec::new();
    };
    let declaring = Document {
        path: path.clone(),
        language: if path.ends_with(".kt") { "kotlin" } else { "java" }.into(),
        version: 0,
        text: text.clone(),
    };
    let Some((toks, df)) = eligible(&declaring).then(|| parsed(&declaring)).flatten() else {
        return Vec::new();
    };
    let package = qualified.rsplit_once('.').map_or("", |(package, _)| package);
    if df.package != package || df.types.iter().filter(|t| t.name == ty).count() != 1 {
        return Vec::new();
    }
    declarations(text, &toks, &df).into_iter().filter(|d| d.owner == ty).collect()
}
/// String literals passed to the `@PropertyKey` parameter of a proven project
/// method/constructor: unqualified same-class calls, `Type.method(..)` and
/// `new Type(..)`/`Type(..)` where `Type` is declared in this file or by import/package
/// in the same module. Instance receivers, overloads, varargs and defaults stay quiet.
fn annotated(doc: &Document, toks: &[Tok], f: &java::JavaFile, kotlin: bool, only: Option<usize>) -> Vec<Context> {
    let mut found = Vec::new();
    let local = declarations(&doc.text, toks, f);
    let mut external: std::collections::HashMap<String, Vec<Decl>> = std::collections::HashMap::new();
    let calls = if kotlin { spring_text::kotlin::calls(&doc.text) } else { java::calls(&doc.text) };
    for call in calls.iter().filter(|c| !c.chained) {
        if only.is_some_and(|at| !call.args.iter().any(|a| a.string.as_ref().is_some_and(|s| s.at <= at && at <= s.end))) {
            continue;
        }
        let Ok(ci) = toks.binary_search_by_key(&call.at, |t| t.at) else {
            continue;
        };
        let before = |n: usize| ci.checked_sub(n).map(|i| toks[i].s);
        let (qualifier, ctor) = match (&call.receiver, call.is_new) {
            (None, true) if before(1) != Some(".") => (Some(call.name.as_str()), true),
            (None, false) if before(1) != Some(".") => (None, false),
            (Some(r), false)
                if before(1) == Some(".")
                    && before(2) == Some(r.as_str())
                    && before(3) != Some(".")
                    && r.starts_with(|c: char| c.is_uppercase()) =>
            {
                (Some(r.as_str()), false)
            }
            _ => continue,
        };
        let in_file = |ty: &str| f.types.iter().filter(|t| t.name == ty).count();
        let decl = match qualifier {
            Some(ty) if in_file(ty) == 1 => local.iter().find(|d| d.owner == ty && d.name.as_deref() == (!ctor).then_some(call.name.as_str())),
            Some(ty) if in_file(ty) == 0 && external.len() < 16 => external
                .entry(ty.to_owned())
                .or_insert_with(|| external_declarations(doc, f, ty))
                .iter()
                .find(|d| d.name.as_deref() == (!ctor).then_some(call.name.as_str())),
            Some(_) => None,
            None => {
                let named = f.types.iter().flat_map(|t| &t.members).filter(|m| m.name == call.name).count();
                local.iter().find(|d| named == 1 && d.name.as_deref() == Some(call.name.as_str()) && d.owner_span.contains(&call.at))
            }
        };
        let Some(decl) = decl.filter(|d| d.arity == call.args.len()) else {
            continue;
        };
        let Some(string) = call.args[decl.index].string.as_ref() else {
            continue;
        };
        if only.is_some_and(|at| !(string.at <= at && at <= string.end)) {
            continue;
        }
        let Ok(ti) = toks.binary_search_by_key(&(string.at - 1), |t| t.at) else {
            continue;
        };
        let arg = &call.args[decl.index];
        if plain(&toks[ti], kotlin).is_none() || arg.at != toks[ti].at || arg.end != toks[ti].end() {
            continue;
        }
        if let Some(mut context) = owned_context(doc, kotlin, &decl.bundle, string.at..string.end) {
            context.indirect = true;
            found.push(context);
        }
    }
    found
}
/// Every proven `@PropertyKey` literal of a source, for diagnostics.
pub(crate) fn property_key_contexts(doc: &Document) -> Vec<Context> {
    if !eligible(doc) || !doc.text.contains("(") {
        return Vec::new();
    }
    let Some((toks, f)) = parsed(doc) else {
        return Vec::new();
    };
    annotated(doc, &toks, &f, doc.language == "kotlin", None)
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
    /// Every physical file of this bundle was indexed: no skipped, oversized or truncated source.
    pub(crate) fn resolved(&self, index: &KeyIndex) -> bool {
        index.complete && index.files.iter().any(|p| self.contains(p)) && !index.skipped.iter().any(|p| self.contains(p))
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
    fn same_file_constant_keys_resolve_only_when_every_use_is_one_proven_getter() {
        let _h = host();
        let mut p = Properties::default();
        let java = "import java.util.ResourceBundle; class App { static final String KEY = \"owned.key\"; String f(){return ResourceBundle.getBundle(\"pkg.messages\").getString(KEY);} }";
        let kotlin = "import java.util.ResourceBundle\nconst val KEY: String = \"owned.key\"\nfun f() = ResourceBundle.getBundle(\"pkg.messages\").getString(KEY)";
        let kotlin_plain = "const val KEY = \"owned.key\"\nfun f() = java.util.ResourceBundle.getBundle(\"pkg.messages\").getObject(KEY)";
        for (source, is_kotlin) in [(java, false), (kotlin, true), (kotlin_plain, true)] {
            let d = doc(source, is_kotlin);
            p.key_index(&d.path).unwrap();
            let c = context(&d, text::offset(&d.text, at(&d)).unwrap()).unwrap();
            assert!(c.indirect);
            assert_eq!(&d.text[c.span.clone()], "owned.key");
            assert_eq!(c.bundle, "pkg.messages");
            assert_eq!(p.definition(&d, at(&d)).len(), 2);
        }
    }
    #[test]
    fn constant_keys_with_other_uses_mixed_bundles_or_no_use_stay_quiet() {
        let _h = host();
        for (source, kotlin) in [
            ("class App { static final String KEY = \"owned.key\"; }", false),
            ("import java.util.ResourceBundle; class App { static final String KEY = \"owned.key\"; String f(){return ResourceBundle.getBundle(\"pkg.messages\").getString(KEY);} String g(){return KEY;} }", false),
            ("import java.util.ResourceBundle; class App { static final String KEY = \"owned.key\"; String f(){return ResourceBundle.getBundle(\"pkg.messages\").getString(KEY);} String g(){return ResourceBundle.getBundle(\"pkg.other\").getString(KEY);} }", false),
            ("import java.util.ResourceBundle; class App { final String KEY = \"owned.key\"; String f(){return ResourceBundle.getBundle(\"pkg.messages\").getString(KEY);} }", false),
            ("import java.util.ResourceBundle; class App { static final String KEY = \"owned.key\"; String f(String KEY){return ResourceBundle.getBundle(\"pkg.messages\").getString(KEY);} }", false),
            ("import java.util.ResourceBundle\nval KEY = \"owned.key\"\nfun f() = ResourceBundle.getBundle(\"pkg.messages\").getString(KEY)", true),
            ("import other.ResourceBundle\nconst val KEY = \"owned.key\"\nfun f() = ResourceBundle.getBundle(\"pkg.messages\").getString(KEY)", true),
        ] {
            let d = doc(source, kotlin);
            assert!(context(&d, d.text.find("owned.key").unwrap() + 1).is_none(), "{source}");
        }
    }
    const PK_IMPORT: &str = "import org.jetbrains.annotations.PropertyKey;";
    const PK_PARAM: &str = "@PropertyKey(resourceBundle = \"pkg.messages\") String key";
    fn warm(_h: &MockHost, source: &Document) -> Properties {
        let mut p = Properties::default();
        p.key_index(&source.path).unwrap();
        p
    }
    #[test]
    fn property_key_literals_resolve_definition_hover_completion_for_same_file_calls() {
        let h = host();
        let java = format!(
            "{PK_IMPORT} class Msg {{ static String get({PK_PARAM}) {{ return key; }} static String two({PK_PARAM}, Object a) {{ return key; }} String f(){{ return Msg.get(\"owned.key\") + get(\"owned.key\") + new Msg().ctor(1) ; }} }}"
        );
        let d = doc(&java, false);
        let mut p = warm(&h, &d);
        for needle in ["Msg.get(\"owned.key", " get(\"owned.key"] {
            let pos = text::position(&d.text, d.text.find(needle).unwrap() + needle.len() - 3);
            assert_eq!(p.definition(&d, pos).len(), 2, "{needle}");
            assert!(p.hover(&d, pos).unwrap().markdown.contains("Right"));
            assert_eq!(p.completion(&d, pos, None).len(), 1);
        }
        let kotlin = "import org.jetbrains.annotations.PropertyKey\nclass Msg {\n fun get(@PropertyKey(resourceBundle = \"pkg.messages\") key: String): String = key\n fun f() = get(\"owned.key\")\n}";
        let d = doc(kotlin, true);
        assert_eq!(p.definition(&d, at(&d)).len(), 2);
        let fqn = "class Msg { static String get(@org.jetbrains.annotations.PropertyKey(resourceBundle = \"pkg.messages\") String key){return key;} String f(){return get(\"owned.key\");} }";
        let d = doc(fqn, false);
        assert_eq!(p.definition(&d, at(&d)).len(), 2);
    }
    #[test]
    fn property_key_calls_resolve_declarations_in_other_project_files_exactly_once() {
        let h = host();
        h.add_file(
            "module/src/main/java/app/Msg.java",
            &format!("package app; {PK_IMPORT} public class Msg {{ public Msg({PK_PARAM}) {{}} public static String get({PK_PARAM}) {{ return key; }} }}"),
        );
        let mut d = doc("package app; class Use { Object f(){ new Msg(\"owned.key\"); return Msg.get(\"owned.key\"); } }", false);
        d.path = "/project/module/src/main/java/app/Use.java".into();
        let mut p = warm(&h, &d);
        let call = text::position(&d.text, d.text.find("Msg.get(\"owned").unwrap() + 12);
        assert_eq!(p.definition(&d, call).len(), 2);
        let ctor = text::position(&d.text, d.text.find("Msg(\"owned").unwrap() + 7);
        assert_eq!(p.definition(&d, ctor).len(), 2);
        h.add_file("module/src/test/java/app/Msg.java", "package app; class Msg {}");
        let t = Document { path: "/project/module/src/test/java/app/Use.java".into(), language: "java".into(), version: 2, text: d.text.clone() };
        assert!(p.definition(&t, call).is_empty(), "main and test declarations are ambiguous");
        h.add_file("module/src/main/java/app/Msg.java", &format!("package other; {PK_IMPORT} class Msg {{ static String get({PK_PARAM}) {{ return key; }} }}"));
        assert!(p.definition(&d, call).is_empty(), "package mismatch");
    }
    #[test]
    fn property_key_stays_quiet_for_shadowed_aliased_ambiguous_or_instance_shapes() {
        let h = host();
        let _ = &h;
        let call = "String f(){return Msg.get(\"owned.key\");}";
        let cases = [
            format!("class Msg {{ static String get({PK_PARAM}) {{return key;}} {call} }}"),
            format!("import foo.PropertyKey; class Msg {{ static String get({PK_PARAM}) {{return key;}} {call} }}"),
            format!("import org.jetbrains.annotations.*; class Msg {{ static String get({PK_PARAM}) {{return key;}} {call} }}"),
            format!("{PK_IMPORT} @interface PropertyKey {{}} class Msg {{ static String get({PK_PARAM}) {{return key;}} {call} }}"),
            format!("{PK_IMPORT} class Msg {{ static String get({PK_PARAM}) {{return key;}} static String get({PK_PARAM}, int n) {{return key;}} {call} }}"),
            format!("{PK_IMPORT} class Msg {{ static String get({PK_PARAM}, Object... rest) {{return key;}} String f(){{return Msg.get(\"owned.key\", 1);}} }}"),
            format!("{PK_IMPORT} class Msg {{ static String get({PK_PARAM}) {{return key;}} String f(Msg msg){{return msg.get(\"owned.key\");}} }}"),
            format!("{PK_IMPORT} class Msg {{ static String get({PK_PARAM}) {{return key;}} String f(){{return Msg.get(\"owned.key\", 2);}} }}"),
            format!("{PK_IMPORT} class Msg {{ static String get(@PropertyKey(resourceBundle = \"pkg.\" + \"messages\") String key) {{return key;}} {call} }}"),
            format!("{PK_IMPORT} class Msg {{ static String get(@PropertyKey(resourceBundle = \"missing.bundle\") String key) {{return key;}} {call} }}")
                .replace("\"missing.bundle\"", "\"bad-name\""),
            format!("{PK_IMPORT} class Msg {{ static String get(@PropertyKey(resourceBundle = \"pkg.messages\") Object key) {{return key;}} {call} }}"),
            format!("{PK_IMPORT} class Msg {{ static String get({PK_PARAM}) {{return key;}} String f(String key){{return Msg.get(key + \"owned.key\");}} }}"),
        ];
        for source in &cases {
            let d = doc(source, false);
            assert!(context(&d, d.text.rfind("owned.key").unwrap() + 1).is_none_or(|c| !c.indirect), "{source}");
        }
        let alias = "import org.jetbrains.annotations.PropertyKey as PK\nclass Msg { fun get(@PK(resourceBundle = \"pkg.messages\") key: String) = key\n fun f() = get(\"owned.key\") }";
        let d = doc(alias, true);
        assert!(context(&d, d.text.rfind("owned.key").unwrap() + 1).is_none());
        let defaults = "import org.jetbrains.annotations.PropertyKey\nclass Msg { fun get(@PropertyKey(resourceBundle = \"pkg.messages\") key: String, n: Int = 1) = key\n fun f() = get(\"owned.key\") }";
        let d = doc(defaults, true);
        assert!(context(&d, d.text.rfind("owned.key").unwrap() + 1).is_none());
    }
    #[test]
    fn property_key_diagnostics_require_a_completely_indexed_owned_bundle() {
        use openidea_plugin::diagnostics::DiagnosticsProvider;
        let h = host();
        let source = format!("{PK_IMPORT} class Msg {{ static String get({PK_PARAM}) {{return key;}} String f(){{return get(\"owned.key\") + get(\"gone.key\");}} }}");
        let d = doc(&source, false);
        let mut p = Properties::default();
        assert!(p.diagnostics(&d).is_empty(), "cold index");
        p.key_index(&d.path).unwrap();
        let found = p.diagnostics(&d);
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("gone.key") && found[0].message.contains("pkg.messages"));
        assert_eq!(found[0].code.as_deref(), Some("PropertyKeyUnresolved"));
        h.add_file("module/src/main/resources/pkg/messages_fr.properties", &"a=b\n".repeat(40000));
        let mut p = Properties::default();
        p.key_index(&d.path).unwrap();
        assert!(p.diagnostics(&d).is_empty(), "an unreadable locale file makes the bundle unresolved");
        let missing = format!("{PK_IMPORT} class Msg {{ static String get(@PropertyKey(resourceBundle = \"pkg.absent\") String key) {{return key;}} String f(){{return get(\"gone.key\");}} }}");
        let d = doc(&missing, false);
        let mut p = Properties::default();
        p.key_index(&d.path).unwrap();
        assert!(p.diagnostics(&d).is_empty(), "no physical bundle file");
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
