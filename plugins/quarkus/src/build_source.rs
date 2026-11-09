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
                if plugin { found |= value == "io.quarkus"; }
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
