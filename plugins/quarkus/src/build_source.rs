//! Bounded literal build declarations only, never an effective build resolver.
const MAX_TEXT: usize = 128 << 10;
fn literal(token: &str) -> Option<&str> {
    if token.len() < 2 { return None; }
    let quote = token.chars().next()?;
    if !matches!(quote, '\'' | '"') || !token.ends_with(quote) { return None; }
    let value = &token[1..token.len()-1];
    (!value.contains(['\\', '\n', '\r', '$', '{', '}']) && !value.contains(quote)).then_some(value)
}
pub(crate) fn declares(file: &str, text: &str) -> bool {
    if text.len() > MAX_TEXT { return false; }
    if file == "pom.xml" {
        return crate::qute::literal_maven_dependency(text, |artifact| !artifact.is_empty()
            && artifact.len() <= 256 && artifact.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte)));
    }
    if !matches!(file, "build.gradle" | "build.gradle.kts") { return false; }
    let tokens = spring_text::java::tokens(text);
    // The shared lexer does not model Groovy slashy/dollar-slashy or triple
    // single-quoted strings. Keep those scripts unknown instead of reading
    // their contents as plugin/dependency calls (division is conservative too).
    if file == "build.gradle" && (text.contains("'''") || tokens.iter().any(|token| token.s == "/")) { return false; }
    let mut blocks: Vec<&str> = Vec::new();
    let mut found = false;
    for (index, token) in tokens.iter().enumerate() {
        match token.s {
            "{" => blocks.push(if index > 0 && blocks.is_empty() { tokens[index-1].s } else { "unknown" }),
            "}" => { if blocks.pop().is_none() { return false; } },
            "id" | "implementation" | "api" | "compileOnly" | "runtimeOnly" if blocks.len() == 1 => {
                let plugin = token.s == "id" && blocks[0] == "plugins";
                let dependency = token.s != "id" && blocks[0] == "dependencies";
                if (!plugin && !dependency) || index > 0 && tokens[index-1].s == "." { continue; }
                let mut next = index + 1;
                let parentheses = tokens.get(next).is_some_and(|token| token.s == "(");
                if parentheses { next += 1; }
                let Some(value) = tokens.get(next).and_then(|token| literal(token.s)) else { continue; };
                if parentheses && !tokens.get(next+1).is_some_and(|token| token.s == ")") { continue; }
                if plugin {
                    let after = next + if parentheses { 2 } else { 1 };
                    let applied = if tokens.get(after).is_some_and(|t| t.s == "apply") {
                        tokens.get(after+1).is_some_and(|t| t.s == "true")
                    } else if tokens.get(after).is_some_and(|t| t.s == ".") && tokens.get(after+1).is_some_and(|t| t.s == "apply") { false }
                    else { true };
                    found |= applied && value == "io.quarkus";
                }
                if dependency {
                    let parts: Vec<_> = value.split(':').collect();
                    found |= matches!(parts.len(), 2 | 3) && parts[0] == "io.quarkus"
                        && parts.iter().all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte)));
                }
            },
            _ => {},
        }
    }
    found && blocks.is_empty()
}

/// Resolve only the conventional local Gradle `libs.plugins` catalogue. No script evaluation.
pub(crate) fn declares_catalog(file: &str, text: &str, catalog: &str) -> bool {
    if !matches!(file, "build.gradle" | "build.gradle.kts") || text.len() > MAX_TEXT || catalog.len() > MAX_TEXT { return false; }
    if file == "build.gradle" && (text.contains("'''") || spring_text::java::tokens(text).iter().any(|t| t.s == "/")) { return false; }
    let Ok(catalog) = catalog.parse::<toml_edit::DocumentMut>() else { return false; };
    let Some(plugins) = catalog.get("plugins").and_then(|p| p.as_table()) else { return false; };
    let aliases: Vec<String> = plugins.iter().filter_map(|(name, entry)| {
        let accessor = name.replace(['-', '_'], ".");
        if plugins.iter().filter(|(other, _)| other.replace(['-', '_'], ".") == accessor).count() != 1 { return None; }
        let id = entry.as_str().and_then(|coordinate| coordinate.split_once(':').map(|p| p.0))
            .or_else(|| entry.get("id").and_then(|id| id.as_str()));
        (id == Some("io.quarkus")).then_some(accessor)
    }).collect();
    let tokens = spring_text::java::tokens(text);
    let mut depth = 0usize;
    let mut plugin_block = false;
    let mut found = false;
    for (i, token) in tokens.iter().enumerate() {
        match token.s {
            "{" => { if depth == 0 { plugin_block = i > 0 && tokens[i-1].s == "plugins"; } depth += 1; },
            "}" => { if depth == 0 { return false; } depth -= 1; },
            "alias" if depth == 1 && plugin_block && (i == 0 || tokens[i-1].s != ".") => {
                let Some(tail) = tokens.get(i+1..) else { continue; };
                if tail.len() < 7 || tail[0].s != "(" || tail[1].s != "libs" || tail[2].s != "." || tail[3].s != "plugins" || tail[4].s != "." { continue; }
                let mut accessor = String::new();
                let mut expect_name = true;
                for (j, t) in tail[5..].iter().enumerate() {
                    if t.s == ")" && !expect_name {
                        let after = &tail[6+j..];
                        let applied = if after.first().is_some_and(|t| t.s == "apply") {
                            after.get(1).is_some_and(|t| t.s == "true")
                        } else if after.first().is_some_and(|t| t.s == ".") && after.get(1).is_some_and(|t| t.s == "apply") {
                            false
                        } else { true };
                        found |= applied && aliases.contains(&accessor);
                        break;
                    }
                    if expect_name {
                        if t.s.is_empty() || !t.s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') { break; }
                        accessor.push_str(t.s);
                    } else {
                        if t.s != "." { break; }
                        accessor.push('.');
                    }
                    expect_name = !expect_name;
                }
            },
            _ => {},
        }
    }
    found && depth == 0
}
