//! Literal Vite configuration and build-manifest facts. Never evaluates JavaScript.
//! Dynamic expressions are deliberately withheld, rather than guessed as paths.
use std::collections::BTreeSet;

#[derive(Default, Debug, PartialEq, Eq)]
pub(crate) struct Config {
    pub inputs: Vec<(String, usize)>,
    pub build_directory: Option<String>,
    /// Manifest path relative to the build directory, if explicitly configured.
    pub manifest: Option<String>,
    pub complete: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ManifestAsset {
    pub source: String,
    pub output: String,
    pub is_entry: bool,
}

/// Project-relative source/output paths only. Vite URLs and aliases need explicit
/// resolution; accepting them as files would manufacture navigation destinations.
pub(crate) fn path(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && !value.starts_with('/')
        && !value.contains(['\\', ':', '?', '#']) && !value.chars().any(char::is_control)
        && value.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

#[derive(Debug)]
struct Token { value: String, at: usize, string: bool }

fn tokens(text: &str) -> Option<Vec<Token>> {
    if text.len() > 256 << 10 { return None; }
    let bytes = text.as_bytes();
    let mut result = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() { at += 1; continue; }
        if bytes[at..].starts_with(b"//") {
            at += text[at..].find('\n').unwrap_or(text.len() - at); continue;
        }
        if bytes[at..].starts_with(b"/*") {
            at += text[at + 2..].find("*/")? + 4; continue;
        }
        // A slash here may begin a regular expression. Refuse the opaque config
        // rather than interpreting identifiers inside a regex as executable calls.
        if bytes[at] == b'/' { return None; }
        let start = at;
        let mut string = matches!(bytes[at], b'\'' | b'"');
        let value = if string || bytes[at] == b'`' {
            let quote = bytes[at];
            at += 1;
            let mut literal = true;
            while at < bytes.len() && bytes[at] != quote {
                if bytes[at] == b'\\' { literal = false; at += 1; }
                if at < bytes.len() { at += 1; }
            }
            if at == bytes.len() { return None; }
            at += 1;
            // Escapes and template strings are opaque; do not interpret JS values.
            if literal && string { text[start + 1..at - 1].to_owned() } else { string = false; "<expression>".into() }
        } else if bytes[at].is_ascii_alphabetic() || matches!(bytes[at], b'_' | b'$') {
            at += 1;
            while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || matches!(bytes[at], b'_' | b'$')) { at += 1; }
            text[start..at].to_owned()
        } else {
            let c = text[at..].chars().next()?;
            at += c.len_utf8();
            c.to_string()
        };
        result.push(Token { value, at: start + usize::from(string), string });
        if result.len() > 40_000 { return None; }
    }
    Some(result)
}

fn end(tokens: &[Token], open: usize) -> Option<usize> {
    end_at(tokens, open, 0)
}
fn end_at(tokens: &[Token], open: usize, depth: usize) -> Option<usize> {
    if depth > 128 { return None; }
    let closing = match tokens.get(open)?.value.as_str() { "(" => ")", "[" => "]", "{" => "}", _ => return None };
    let mut at = open + 1;
    while at < tokens.len() {
        if tokens[at].string { at += 1; continue; }
        match tokens[at].value.as_str() {
            "(" | "[" | "{" => at = end_at(tokens, at, depth + 1)? + 1,
            ")" | "]" | "}" => return (tokens[at].value == closing).then_some(at),
            _ => at += 1,
        }
    }
    None
}

fn literal_inputs(tokens: &[Token], start: usize, limit: usize) -> Option<Vec<(String, usize)>> {
    let token = tokens.get(start)?;
    if token.string && path(&token.value) {
        return (start + 1 == limit).then(|| vec![(token.value.clone(), token.at)]);
    }
    if token.value != "[" || end(tokens, start)? + 1 != limit { return None; }
    let mut at = start + 1;
    let mut result = Vec::new();
    while at < limit - 1 {
        let token = &tokens[at];
        if !token.string || !path(&token.value) { return None; }
        result.push((token.value.clone(), token.at));
        at += 1;
        if at < limit - 1 {
            if tokens[at].value != "," || tokens[at].string { return None; }
            at += 1;
        }
    }
    Some(result)
}

/// Unknown spreads/getters/computed keys withhold output-path inference.
fn properties_at(tokens: &[Token], open: usize) -> Option<Vec<(&str, usize, usize)>> {
    if tokens.get(open)?.value != "{" || tokens[open].string { return None; }
    let close = end(tokens, open)?;
    let mut item = open + 1;
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    while item < close {
        let key = &tokens[item].value;
        if !seen.insert(key.as_str()) || tokens.get(item + 1)?.value != ":" || tokens[item + 1].string { return None; }
        let value = item + 2;
        let mut next = value;
        while next < close && (tokens[next].value != "," || tokens[next].string) {
            if !tokens[next].string && matches!(tokens[next].value.as_str(), "(" | "[" | "{") {
                next = end(tokens, next)? + 1;
            } else { next += 1; }
        }
        if next == value { return None; }
        result.push((key.as_str(), value, next));
        item = next + 1;
    }
    Some(result)
}

/// Supports the literal laravel-vite-plugin default import (including aliases),
/// its string/array shorthand and `input`/`buildDirectory` object properties.
pub(crate) fn config(text: &str) -> Config {
    let mut result = Config { complete: true, ..Config::default() };
    let Some(tokens) = tokens(text) else { result.complete = false; return result; };
    let mut imports = BTreeSet::new();
    for window in tokens.windows(4) {
        if window[0].value == "import" && !window[0].string && !window[1].string
            && window[2].value == "from" && !window[2].string && window[3].string && window[3].value == "laravel-vite-plugin" {
            imports.insert(window[1].value.clone());
        }
    }
    // Shadowed import bindings have no safely inferred callee identity.
    for pair in tokens.windows(2) {
        if !pair[0].string && matches!(pair[0].value.as_str(), "function" | "const" | "let" | "var" | "class")
            && imports.contains(&pair[1].value) {
            result.complete = false;
            return result;
        }
    }
    for (at, token) in tokens.iter().enumerate() {
        if !token.string && imports.contains(&token.value)
            && !at.checked_sub(1).is_some_and(|before| matches!(tokens[before].value.as_str(), "import" | "."))
            && !tokens.get(at + 1).is_some_and(|next| next.value == "(") {
            // An imported identifier passed around, assigned or shadowed by a
            // parameter needs binding analysis that this literal adapter lacks.
            return Config { complete: false, ..Config::default() };
        }
    }
    let mut at = 0;
    while at + 1 < tokens.len() {
        if (at > 0 && tokens[at - 1].value == ".") || tokens[at].string || !imports.contains(&tokens[at].value) || tokens[at + 1].value != "(" {
            at += 1; continue;
        }
        let Some(close) = end(&tokens, at + 1) else { result.complete = false; break; };
        let start = at + 2;
        if tokens.get(start).is_some_and(|token| token.value == "{" && !token.string) {
            let Some(object_end) = end(&tokens, start) else { result.complete = false; break; };
            if object_end + 1 != close { result.complete = false; at = close + 1; continue; }
            let mut item = start + 1;
            let mut keys = BTreeSet::new();
            while item < object_end {
                let key = &tokens[item].value;
                if matches!(key.as_str(), "input" | "buildDirectory") && !keys.insert(key.clone()) {
                    return Config { complete: false, ..Config::default() };
                }
                if tokens.get(item + 1).is_none_or(|t| t.value != ":" || t.string) {
                    result.complete = false; break;
                }
                let value = item + 2;
                let mut next = value;
                while next < object_end && (tokens[next].value != "," || tokens[next].string) {
                    if !tokens[next].string && matches!(tokens[next].value.as_str(), "(" | "[" | "{") {
                        let Some(last) = end(&tokens, next) else { result.complete = false; return result; };
                        next = last + 1;
                    } else { next += 1; }
                }
                match key.as_str() {
                    "input" => match literal_inputs(&tokens, value, next) {
                        Some(inputs) => result.inputs.extend(inputs), None => result.complete = false,
                    },
                    "buildDirectory" if next == value + 1 && tokens[value].string && path(&tokens[value].value) => {
                        result.build_directory = Some(tokens[value].value.clone());
                    },
                    "buildDirectory" => result.complete = false,
                    _ => {},
                }
                item = next + 1;
            }
        } else {
            match literal_inputs(&tokens, start, close) {
                Some(inputs) => result.inputs.extend(inputs), None => result.complete = false,
            }
        }
        at = close + 1;
    }
    // A nested plugin option named `build` is not Vite's output configuration.
    let exported = tokens.windows(2).position(|pair| pair[0].value == "export" && !pair[0].string && pair[1].value == "default" && !pair[1].string)
        .and_then(|at| {
            if tokens.get(at + 2)?.value == "{" { Some(at + 2) }
            else if tokens.get(at + 3)?.value == "(" && tokens.get(at + 4)?.value == "{" { Some(at + 4) }
            else { None }
        });
    if let Some(root) = exported {
        if let Some(properties) = properties_at(&tokens, root) {
            for (key, value, limit) in properties {
                if key != "build" || tokens[value].value != "{" || end(&tokens, value).map(|at| at + 1) != Some(limit) { continue; }
                if let Some(build) = properties_at(&tokens, value) {
                    for (key, value, limit) in build {
                        if key == "manifest" && value + 1 == limit && tokens[value].string && path(&tokens[value].value) {
                            result.manifest = Some(tokens[value].value.clone());
                        }
                    }
                }
            }
        }
    }
    let mut seen = BTreeSet::new();
    result.inputs.retain(|(name, _)| seen.insert(name.clone()));
    result
}

/// Vite build manifests include non-entry assets too: these are valid arguments
/// to Vite::asset(). Internal chunk keys are withheld unless they name a source.
pub(crate) fn manifest(text: &str) -> Result<Vec<ManifestAsset>, String> {
    let value = crate::strict_json::parse(text, 2 << 20).map_err(|_| "Invalid or oversized Vite manifest".to_owned())?;
    let object = value.as_object().ok_or("Vite manifest must be an object")?;
    let mut result = Vec::new();
    for (key, value) in object {
        let entry = value.as_object().ok_or("Vite manifest record must be an object")?;
        let output = entry.get("file").and_then(|v| v.as_str()).ok_or("Vite manifest record needs a file")?;
        if !path(output) { return Err("Vite manifest output must stay in the build directory".into()); }
        let source = match entry.get("src") {
            Some(v) => v.as_str().ok_or("Vite manifest src must be a string")?,
            None if key.starts_with('_') => continue,
            None => key,
        };
        if !path(source) { return Err("Vite manifest source must stay in the project".into()); }
        let is_entry = match entry.get("isEntry") {
            None => false,
            Some(v) => v.as_bool().ok_or("Vite manifest isEntry must be boolean")?,
        };
        result.push(ManifestAsset { source: source.to_owned(), output: output.to_owned(), is_entry });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn laravel_import_alias_and_literal_options_keep_original_ranges() {
        let source = "import framework from 'laravel-vite-plugin';\nexport default defineConfig({plugins:[framework({ input: ['resources/css/app.css', 'resources/js/app.ts'], refresh: true, buildDirectory: 'assets' })], build: { manifest: 'files.json' }});";
        let result = config(source);
        assert!(result.complete);
        assert_eq!(result.inputs.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["resources/css/app.css", "resources/js/app.ts"]);
        for (name, at) in &result.inputs { assert_eq!(&source[*at..*at + name.len()], name); }
        assert_eq!(result.build_directory.as_deref(), Some("assets"));
        assert_eq!(result.manifest.as_deref(), Some("files.json"));
    }

    #[test]
    fn shorthand_supports_arrays_and_single_inputs_but_not_expressions() {
        for input in ["'resources/app.js'", "['resources/app.js',]"] {
            let result = config(&format!("import laravel from 'laravel-vite-plugin'; laravel({input})"));
            assert!(result.complete);
            assert_eq!(result.inputs[0].0, "resources/app.js");
        }
        for input in ["entries", "['resources/app.js', ...other]", "`resources/${name}.js`", "'resources/' + name", "['../outside.js']", "'resources/\\u0061.js'"] {
            let result = config(&format!("import laravel from 'laravel-vite-plugin'; laravel({{input:{input}}})"));
            assert!(!result.complete, "{input}");
            assert!(result.inputs.is_empty());
        }
    }

    #[test]
    fn comments_strings_and_unimported_names_cannot_manufacture_inputs() {
        let result = config("// import laravel from 'laravel-vite-plugin';\nlaravel(['resources/fake.js']); const decoy = `laravel(['resources/fake2.js'])`; ");
        assert!(result.inputs.is_empty());
        let result = config("import laravel from 'laravel-vite-plugin'; /* laravel(['resources/no.js']) */ laravel({input: ['resources/yes.js'], refresh: ['app/**']})");
        assert_eq!(result.inputs.len(), 1);
        assert_eq!(result.inputs[0].0, "resources/yes.js");
    }

    #[test]
    fn escaped_inputs_regex_decoys_shadowed_parameters_and_duplicate_keys_are_withheld() {
        for expression in [
            "laravel({input: 'resources/\\u0061.js'})",
            "const decoy = /laravel('resources\\/fake.js')/;",
            "function nested(laravel) { return laravel('resources/fake.js'); }",
            "laravel({input: 'resources/a.js', input: 'resources/b.js'})",
        ] {
            let result = config(&format!("import laravel from 'laravel-vite-plugin'; {expression}"));
            assert!(result.inputs.is_empty(), "{expression}");
            assert!(!result.complete, "{expression}");
        }
        let result = config("import laravel from 'laravel-vite-plugin'; other.laravel('resources/fake.js')");
        assert!(result.inputs.is_empty());
    }

    #[test]
    fn unrelated_nested_build_options_do_not_change_the_manifest_path() {
        let result = config("export default { plugins: [{ build: { manifest: 'wrong.json' } }], build: {manifest: 'right.json'} }");
        assert_eq!(result.manifest.as_deref(), Some("right.json"));
        let result = config("const decoy = {build: {manifest:'wrong.json'}}; export default {plugins: []}");
        assert_eq!(result.manifest, None);
        let result = config("export default {build: {manifest:'first.json',manifest:'second.json'}}");
        assert_eq!(result.manifest, None);
    }

    #[test]
    fn manifest_includes_assets_not_just_entry_points_and_ignores_virtual_chunks() {
        let result = manifest(r#"{"resources/js/app.js":{"file":"assets/app-hash.js","src":"resources/js/app.js","isEntry":true},"resources/images/logo.png":{"file":"assets/logo-hash.png","src":"resources/images/logo.png"},"_chunk.js":{"file":"assets/chunk.js"}}"#).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|a| a.source == "resources/images/logo.png" && !a.is_entry));
        assert!(result.iter().any(|a| a.source == "resources/js/app.js" && a.is_entry));
    }

    #[test]
    fn invalid_duplicate_dynamic_or_traversing_manifests_are_rejected() {
        for text in ["[]", "{}{}", r#"{"a":{"file":"x"},"a":{"file":"y"}}"#, r#"{"a":{"file":"../x"}}"#, r#"{"../a":{"file":"x"}}"#, r#"{"a":{"file":"x","isEntry":"true"}}"#, r#"{"a":{"src":null,"file":"x"}}"#] {
            assert!(manifest(text).is_err(), "{text}");
        }
    }
}
