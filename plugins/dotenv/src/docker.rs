//! Variables declared for containers: `ENV` in Dockerfiles and `environment:`
//! in docker-compose files, after the original plugin's Docker module
//! (`DockerfilePsiElementsVisitor`, `DockerComposeYamlPsiElementsVisitor`).

use std::ops::Range;

/// A declared key, its value and the key's byte range.
pub type Entry = (String, String, Range<usize>);

fn unquote(s: &str) -> &str {
    let s = s.trim();
    for q in ['"', '\''] {
        if let Some(inner) = s.strip_prefix(q).and_then(|s| s.strip_suffix(q)) {
            return inner;
        }
    }
    s
}

/// `(start, word)` for each blank-separated word of `s`, quotes kept together.
fn words(s: &str) -> Vec<(usize, &str)> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let mut quote = None;
        while i < bytes.len() && (quote.is_some() || !bytes[i].is_ascii_whitespace()) {
            match (bytes[i], quote) {
                (b'\\', _) => i += 1,
                (b'"' | b'\'', None) => quote = Some(bytes[i]),
                (q, Some(open)) if q == open => quote = None,
                _ => {}
            }
            i += 1;
        }
        let end = i.min(bytes.len());
        out.push((start, &s[start..end]));
    }
    out
}

/// `ENV KEY=value OTHER="x y"`, `ENV KEY value`, continued with `\`.
pub fn dockerfile(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut continued = false;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let body = line.trim_end_matches(['\n', '\r']);
        let content = body.trim_start();
        let mut at = line_start + body.len() - content.len();
        let rest = if continued {
            content
        } else if content.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("env ")) {
            at += 4;
            &content[4..]
        } else {
            continue;
        };
        let rest = rest.trim_end();
        continued = rest.ends_with('\\');
        let rest = rest.trim_end_matches('\\');

        let words = words(rest);
        let Some(&(_, first)) = words.first() else { continue };
        if first.contains('=') {
            for (start, word) in words {
                let Some((key, value)) = word.split_once('=') else { continue };
                if !key.is_empty() {
                    out.push((key.to_owned(), unquote(value).to_owned(), at + start..at + start + key.len()));
                }
            }
        } else {
            let start = words[0].0;
            let value = rest[start + first.len()..].trim();
            out.push((first.to_owned(), unquote(value).to_owned(), at + start..at + start + first.len()));
        }
    }
    out
}

/// Merge keys, aliases, anchors and `${…}` interpolations aren't variable names.
fn is_anchor_syntax(key: &str) -> bool {
    let mut chars = key.chars();
    let starts_well = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    !starts_well || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_comment_or_empty(rest: &str) -> bool {
    rest.is_empty() || rest.starts_with('#')
}

/// Source slices of a bounded flow collection, including multi-line YAML.
/// Nested collections are not environment scalars and are rejected.
fn flow_items(s: &str) -> Option<Vec<(usize, &str)>> {
    if s.len() > 256 * 1024 { return None; }
    let close = match s.as_bytes().first()? { b'[' => b']', b'{' => b'}', _ => return None };
    let bytes = s.as_bytes();
    let mut items = Vec::new();
    let mut start = 1;
    let mut quote = None;
    let mut comment = false;
    let mut i = 1;
    while i < bytes.len() {
        let c = bytes[i];
        if comment { if c == b'\n' { comment = false; } i += 1; continue; }
        match (c, quote) {
            (b'\\', Some(b'"')) => { i += 2; continue; }
            (b'\'', Some(b'\'')) if bytes.get(i + 1) == Some(&b'\'') => { i += 2; continue; }
            (b'"' | b'\'', None) => quote = Some(c),
            (q, Some(open)) if q == open => quote = None,
            (b'#', None) if i > 0 && bytes[i - 1].is_ascii_whitespace() => comment = true,
            (b'{', None) if bytes.get(i.wrapping_sub(1)) == Some(&b'$') => {
                // An interpolation is not a nested YAML collection. Preserve
                // the item so entry_of can reject interpolated key spellings.
                i += bytes[i..].iter().position(|c| *c == b'}')? + 1;
                continue;
            }
            (b'[' | b'{', None) => return None,
            (q, None) if q == close => { items.push((start, &s[start..i])); return Some(items); }
            (b',' , None) => { items.push((start, &s[start..i])); start = i + 1; }
            _ => {}
        }
        i += 1;
    }
    None
}

fn scalar_slice(item: &str) -> Option<(usize, &str)> {
    let mut lead = item.len() - item.trim_start().len();
    while item[lead..].starts_with('#') {
        lead += item[lead..].find('\n')? + 1;
        lead += item[lead..].len() - item[lead..].trim_start().len();
    }
    let item = item[lead..].trim_end();
    // YAML comments outside scalar quotes do not form part of the value.
    let mut quote = None;
    let mut escaped = false;
    let mut comment_at = item.len();
    for (i, c) in item.char_indices() {
        if escaped { escaped = false; continue; }
        match (c, quote) {
            ('\\', Some('"')) => escaped = true,
            ('"' | '\'', None) => quote = Some(c),
            (q, Some(open)) if q == open => quote = None,
            ('#', None) if i > 0 && item.as_bytes()[i - 1].is_ascii_whitespace() => { comment_at = i; break; }
            _ => {}
        }
    }
    Some((lead, item[..comment_at].trim_end()))
}

fn entry_of(item: &str, item_at: usize, map: bool) -> Option<Entry> {
    let (lead, item) = scalar_slice(item)?;
    let (key, value) = if map {
        item.split_once(':').map(|(k, v)| (unquote(k), unquote(v)))?
    } else {
        let inner = unquote(item);
        let (k, v) = inner.split_once('=').unwrap_or((inner, ""));
        (k.trim(), v.trim())
    };
    if is_anchor_syntax(key) {
        return None;
    }
    let start = item_at + lead + item.find(key)?;
    let value = if value == "~" || value == "null" { "" } else { value };
    Some((key.to_owned(), value.to_owned(), start..start + key.len()))
}

/// `environment:` as a list (`- KEY=value`), a mapping (`KEY: value`) or a
/// flow collection (`[A=1, B]`, `{A: 1}`, including multiple lines). Anchors, aliases and merge
/// keys are resolved by the bounded local-anchor adapter in `compose`.
// Direct declarations remain separate from alias expansion below.
fn compose_direct(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut environment: Option<usize> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let body = line.trim_end_matches(['\n', '\r']);
        let content = body.trim_start();
        let indent = body.len() - content.len();
        if content.is_empty() || content.starts_with('#') {
            continue;
        }

        if let Some(parent) = environment {
            if indent > parent {
                let at = line_start + indent;
                let (item, item_at) = match content.strip_prefix('-') {
                    Some(item) => {
                        let item = item.trim_start();
                        let inner = unquote(item);
                        let (key, value) = inner.split_once('=').unwrap_or((inner, ""));
                        (Some((key.trim(), value.trim())), at + content.len() - item.len())
                    }
                    None => (content.split_once(':').map(|(k, v)| (unquote(k), unquote(v))), at),
                };
                if let Some((key, value)) = item.filter(|(key, _)| !is_anchor_syntax(key)) {
                    let start = item_at + content[item_at - at..].find(key).unwrap_or(0);
                    out.push((key.to_owned(), value.to_owned(), start..start + key.len()));
                }
                continue;
            }
            environment = None;
        }

        let Some(rest) = content.strip_prefix("environment:") else { continue };
        let rest = rest.trim();
        if is_comment_or_empty(rest) || (rest.starts_with('&') && !rest.contains(char::is_whitespace)) {
            environment = Some(indent);
            continue;
        }
        let rest_at = line_start + indent + "environment:".len() + (content["environment:".len()..].len() - content["environment:".len()..].trim_start().len());
        if let Some(items) = flow_items(&text[rest_at..]) {
            let map = rest.starts_with('{');
            out.extend(items.into_iter().filter_map(|(at, item)| entry_of(item, rest_at + at, map)));
        }
    }
    out
}

/// A Compose `$VAR` / `${VAR…}` reference: the variable name's byte range and
/// whether the braced form carries an operator (`-`, `:-`, `?`, `:?`, `+`, `:+`).
pub struct Interpolation {
    pub name: String,
    pub range: Range<usize>,
    pub has_operator: bool,
}

fn is_name_byte(b: u8, first: bool) -> bool {
    b == b'_' || b.is_ascii_alphabetic() || (!first && b.is_ascii_digit())
}

/// Interpolations of a Compose file, per
/// docs.docker.com/compose/how-tos/environment-variables/variable-interpolation.
/// `$$` is a literal dollar; comments and malformed braces are left alone.
pub fn compose_interpolations(text: &str) -> Vec<Interpolation> {
    let mut out = Vec::new();
    if text.len() > 256 * 1024 {
        return out;
    }
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let code = line.split(" #").next().unwrap_or(line);
        if code.trim_start().starts_with('#') {
            continue;
        }
        let bytes = code.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'$' {
                i += 1;
                continue;
            }
            let braced = bytes.get(i + 1) == Some(&b'{');
            let start = i + 1 + usize::from(braced);
            let mut end = start;
            while end < bytes.len() && is_name_byte(bytes[end], end == start) {
                end += 1;
            }
            i = if bytes.get(i + 1) == Some(&b'$') { i + 2 } else { end.max(i + 1) };
            if end == start {
                continue;
            }
            let has_operator = if braced {
                match &bytes[end..] {
                    [b'}', ..] => false,
                    [b':', b'-' | b'?' | b'+', ..] | [b'-' | b'?' | b'+', ..] => true,
                    _ => continue,
                }
            } else {
                false
            };
            out.push(Interpolation { name: code[start..end].to_owned(), range: line_start + start..line_start + end, has_operator });
        }
    }
    out
}

/// Read physical-source declarations and local YAML anchor references. Alias
/// expansion preserves declaration locations; it does not infer runtime scope.
pub fn compose(text: &str) -> Vec<Entry> {
    if text.len() > 256 * 1024 { return Vec::new(); }
    let mut out = compose_direct(text);
    let mut lines = Vec::new();
    let mut offset = 0;
    for raw in text.split_inclusive('\n') {
        let body = raw.trim_end_matches(['\r', '\n']);
        let content = body.trim_start();
        lines.push((offset + body.len() - content.len(), body.len() - content.len(), content));
        offset += raw.len();
    }
    if lines.len() > 4096 { return out; }
    // Only prior, syntactically local scalar collections are admitted. A
    // duplicate anchor shadows earlier definitions, matching YAML ordering.
    let mut anchors: std::collections::BTreeMap<&str, Vec<Entry>> = std::collections::BTreeMap::new();
    for (i, &(at, indent, content)) in lines.iter().enumerate() {
        if let Some((_, value)) = content.split_once(':') {
            if let Some(rest) = value.trim_start().strip_prefix('&') {
                let name_end = rest.find(|c: char| c.is_whitespace() || matches!(c, '[' | '{')).unwrap_or(rest.len());
                let name = &rest[..name_end];
                if anchor_name(name) && anchors.len() < 128 {
                    let tail = rest[name_end..].trim_start();
                    let entries = if tail.starts_with(['[', '{']) {
                        let start = at + content.len() - tail.len();
                        flow_items(&text[start..]).map(|items| items.into_iter().filter_map(|(off, item)| entry_of(item, start + off, tail.starts_with('{'))).collect())
                    } else if is_comment_or_empty(tail) {
                        let block: Vec<_> = lines[i + 1..].iter().take_while(|(_, child, body)| body.is_empty() || body.starts_with('#') || *child > indent).collect();
                        // Nested mappings, aliases and merge dependencies are
                        // unsupported definitions, never partial anchor values.
                        let first_indent = block.iter().find(|(_, _, body)| !body.is_empty() && !body.starts_with('#')).map(|(_, n, _)| *n);
                        let mut entries = Vec::new();
                        let mut valid = true;
                        for &&(off, child, body) in &block {
                            if body.is_empty() || body.starts_with('#') { continue; }
                            if Some(child) != first_indent || body.starts_with("<<:") { valid = false; break; }
                            let (item, item_at, map) = if let Some(list) = body.strip_prefix('-') {
                                (list, off + 1, false)
                            } else { (body, off, true) };
                            if let Some(entry) = entry_of(item, item_at, map) {
                                if entry.1.starts_with(['*', '&', '[', '{']) { valid = false; break; }
                                entries.push(entry);
                            } else { valid = false; break; }
                        }
                        valid.then_some(entries)
                    } else { None };
                    // A malformed shadowing definition also withdraws the old
                    // spelling so it cannot accidentally resolve an older one.
                    anchors.remove(name);
                    if let Some(entries) = entries { anchors.insert(name, entries); }
                }
            }
        }
        let Some(value) = content.strip_prefix("environment:") else { continue };
        let value = value.trim();
        let mut inherited = Vec::new();
        let mut explicit = std::collections::BTreeSet::new();
        if let Some(name) = alias_name(value) {
            if let Some(entries) = anchors.get(name) { inherited.extend(entries.iter().cloned()); }
        } else if is_comment_or_empty(value) || value.starts_with('&') {
            if let Some(definition) = value.strip_prefix('&') {
                let name = definition.split(|c: char| c.is_whitespace() || matches!(c, '[' | '{')).next().unwrap_or("");
                if let Some(entries) = anchors.get(name) { inherited.extend(entries.iter().cloned()); }
            }
            for &(off, child, body) in &lines[i + 1..] {
                if body.is_empty() || body.starts_with('#') { continue; }
                if child <= indent { break; }
                if let Some(merge) = body.strip_prefix("<<:") {
                    let merge = merge.trim();
                    let names: Vec<&str> = if let Some(name) = alias_name(merge) { vec![name] }
                        else if merge.starts_with('[') { flow_items(merge).unwrap_or_default().into_iter().filter_map(|(_, item)| alias_name(item.trim())).collect() }
                        else { Vec::new() };
                    for name in names {
                        if let Some(entries) = anchors.get(name) {
                            for entry in entries {
                                if !inherited.iter().any(|existing: &Entry| existing.0 == entry.0) { inherited.push(entry.clone()); }
                            }
                        }
                    }
                } else if let Some(entry) = entry_of(body, off, true) { explicit.insert(entry.0); }
            }
        }
        out.extend(inherited.into_iter().filter(|entry| !explicit.contains(&entry.0)));
    }
    out.sort_by_key(|entry| entry.2.start);
    out.dedup();
    out
}

fn anchor_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
fn alias_name(value: &str) -> Option<&str> {
    let name = value.strip_prefix('*')?.split(" #").next()?.trim_end();
    anchor_name(name).then_some(name)
}

/// Paths named by `env_file:` (scalar, list, or `- path: x` long form), as
/// written in the compose file.
fn compose_env_files_direct(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut parent: Option<usize> = None;
    let mut offset = 0;
    for raw in text.split_inclusive('\n') {
        let line_at = offset;
        offset += raw.len();
        let line = raw.trim_end_matches(['\r', '\n']);
        let content = line.trim_start();
        let indent = line.len() - content.len();
        if content.is_empty() || content.starts_with('#') {
            continue;
        }
        if let Some(p) = parent {
            if indent > p {
                let item = content.strip_prefix('-').unwrap_or(content).trim_start();
                let item = item.strip_prefix("path:").unwrap_or(item);
                let item = unquote(item.split(" #").next().unwrap_or(item));
                if !item.is_empty() && !item.starts_with(['*', '&']) && !item.contains(": ") && !item.ends_with(':') {
                    out.push(item.to_owned());
                }
                continue;
            }
            parent = None;
        }
        let Some(rest) = content.strip_prefix("env_file:") else { continue };
        let rest = rest.trim();
        if is_comment_or_empty(rest) {
            parent = Some(indent);
        } else if rest.starts_with('[') {
            let start = line_at + indent + content.find(rest).unwrap_or(content.len());
            if let Some(items) = flow_items(&text[start..]) {
                out.extend(items.into_iter().filter_map(|(_, item)| scalar_slice(item).map(|(_, value)| unquote(value).to_owned()))
                    .filter(|value| !value.is_empty() && !value.starts_with(['*', '&', '[', '{'])));
            }
        } else {
            let value = unquote(rest.split(" #").next().unwrap_or(rest));
            if !value.starts_with(['*', '&']) {
                out.push(value.to_owned());
            }
        }
    }
    out
}

fn compose_options()->serde_saphyr::Options {
    let mut options=serde_saphyr::Options::default();
    let mut budget=serde_saphyr::Budget::default();
    budget.max_depth=32; budget.max_nodes=20_000; budget.max_events=100_000;
    budget.max_aliases=128; budget.max_anchors=128; budget.max_documents=1;
    budget.max_total_scalar_bytes=1<<20; budget.max_recorded_anchor_bytes=1<<20;
    options.budget=Some(budget);options.reject_unsupported_tags=true;
    options.alias_limits.max_replay_stack_depth=32;
    options.alias_limits.max_total_replayed_events=20_000;
    options.alias_limits.max_alias_expansions_per_anchor=128;
    options
}

/// Source-file discovery, with alias/merge expansion and a completeness bit.
/// This is not a proof that a Compose service is active or its runtime environment.
pub fn compose_env_file_graph(text: &str) -> (Vec<String>, bool, std::collections::BTreeSet<String>) {
    if text.len() > 256 << 10 { return (Vec::new(), false, Default::default()); }
    let direct = compose_env_files_direct(text);
    let Ok(value)=serde_saphyr::from_str_with_options::<serde_json::Value>(text,compose_options()) else {return (direct,false,Default::default());};
    fn paths(value:&serde_json::Value, out:&mut Vec<String>, complete:&mut bool, required:bool, requirements:&mut std::collections::BTreeMap<String,bool>) {
        match value {
            serde_json::Value::String(path) => if !path.is_empty()&&path.len()<=4096&&!path.chars().any(char::is_control) {out.push(path.clone());requirements.entry(path.clone()).and_modify(|previous|*previous|=required).or_insert(required);}else{*complete=false;},
            serde_json::Value::Array(items) => for item in items {
                match item {serde_json::Value::String(_)=>paths(item,out,complete,true,requirements),serde_json::Value::Object(map)=>{
                    if let Some(path)=map.get("path"){let required=match map.get("required"){None=>true,Some(serde_json::Value::Bool(flag))=>*flag,_=>{*complete=false;true}};if path.is_string(){paths(path,out,complete,required,requirements);}else{*complete=false;}}else{*complete=false;}
                },_=>*complete=false,}
            },
            serde_json::Value::Object(map) => if let Some(path)=map.get("path") {let required=match map.get("required"){None=>true,Some(serde_json::Value::Bool(flag))=>*flag,_=>{*complete=false;true}};if path.is_string(){paths(path,out,complete,required,requirements);}else{*complete=false;}}else{*complete=false;},
            _=>*complete=false,
        }
    }
    fn walk(value:&serde_json::Value,out:&mut Vec<String>,complete:&mut bool,requirements:&mut std::collections::BTreeMap<String,bool>) {
        match value {
            serde_json::Value::Object(map)=>for (key,value) in map {if key=="env_file"{paths(value,out,complete,true,requirements);}else{walk(value,out,complete,requirements);}},
            serde_json::Value::Array(items)=>for item in items {walk(item,out,complete,requirements);},
            _=>{},
        }
    }
    fn environment_names(value:&serde_json::Value,names:&mut std::collections::BTreeSet<String>,complete:&mut bool) {
        match value {
            serde_json::Value::Object(map)=>for (key,value) in map {
                if key=="environment" {
                    match value {
                        serde_json::Value::Object(env)=>for (key,value) in env {
                            if key.is_empty()||key.contains(['$','\0'])||matches!(value,serde_json::Value::Array(_)|serde_json::Value::Object(_)) {*complete=false;}else{names.insert(key.clone());}
                        },
                        serde_json::Value::Array(env)=>for item in env {
                            if let Some(item)=item.as_str(){let key=item.split_once('=').map_or(item,|p|p.0);if key.is_empty()||key.contains(['$','\0']){*complete=false;}else{names.insert(key.into());}}
                            else{*complete=false;}
                        },
                        _=>*complete=false,
                    }
                }else{environment_names(value,names,complete);}
            },
            serde_json::Value::Array(items)=>for item in items {environment_names(item,names,complete);},
            _=>{},
        }
    }
    let mut resolved=Vec::new();let mut complete=true;let mut requirements=std::collections::BTreeMap::new();walk(&value,&mut resolved,&mut complete,&mut requirements);
    let mut expected=std::collections::BTreeSet::new();environment_names(&value,&mut expected,&mut complete);
    let physical:std::collections::BTreeSet<_>=compose(text).into_iter().map(|entry|entry.0).collect();
    complete &= expected==physical;

    let mut seen=std::collections::BTreeSet::new();resolved.retain(|path|seen.insert(path.clone()));
    if resolved.len()>128 {complete=false;resolved.truncate(128);}
    // Keep physical-source order where it exists; expanded aliases are source
    // discovery candidates, not an invented runtime precedence order.
    let mut ordered=Vec::new();
    for path in direct {if resolved.contains(&path)&&!ordered.contains(&path){ordered.push(path);}}
    for path in resolved {if !ordered.contains(&path){ordered.push(path);}}
    let optional=requirements.into_iter().filter_map(|(path,required)|(!required).then_some(path)).collect();
    (ordered,complete,optional)
}
#[cfg(test)]
pub fn compose_env_files(text: &str) -> Vec<String> { compose_env_file_graph(text).0 }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_compose_interpolation_forms_and_skips_the_rest() {
        let text = "a: ${A}\nb: ${B:-x} ${C-x} ${D:?e} ${E?e} ${F:+r} ${G+r}\nc: $H $$I $${J} ${K%x} ${}\n# ${L}\nd: ${M:-${N}} # ${O}\n";
        let found = compose_interpolations(text);
        assert!(found.iter().all(|i| text[i.range.clone()] == i.name));
        let names: Vec<_> = found.iter().map(|i| (i.name.as_str(), i.has_operator)).collect();
        assert_eq!(
            names,
            [("A", false), ("B", true), ("C", true), ("D", true), ("E", true), ("F", true), ("G", true), ("H", false), ("M", true), ("N", false)]
        );
    }

    fn keys(text: &str, entries: Vec<Entry>) -> Vec<(String, String, String)> {
        entries.into_iter().map(|(k, v, r)| (k, v, text[r].to_owned())).collect()
    }

    fn e(k: &str, v: &str) -> (String, String, String) {
        (k.into(), v.into(), k.into())
    }

    #[test]
    fn reads_dockerfile_env() {
        let text = "FROM php\nENV APP_ENV=prod NAME=\"a b\" \\\n    DEBUG=0\nenv   LEGACY some value\nRUN echo $X\n";
        assert_eq!(
            keys(text, dockerfile(text)),
            [e("APP_ENV", "prod"), e("NAME", "a b"), e("DEBUG", "0"), e("LEGACY", "some value")]
        );
    }

    #[test]
    fn reads_compose_environment() {
        let text = "services:\n  app:\n    environment:\n      - DB_HOST=db\n      - \"QUOTED=1\"\n      - BARE\n    ports:\n      - \"80:80\"\n  worker:\n    environment:\n      QUEUE: redis\n      'TIMEOUT': \"30\"\n";
        assert_eq!(
            keys(text, compose(text)),
            [e("DB_HOST", "db"), e("QUOTED", "1"), e("BARE", ""), e("QUEUE", "redis"), e("TIMEOUT", "30")]
        );
    }

    #[test]
    fn compose_env_file_list_and_scalar_are_declared() {
        let text = "services:\n  a:\n    env_file: .env\n  b:\n    env_file:\n      - ./x.env\n      - path: \"./y.env\"\n        required: false\n  c:\n    env_file: [z.env, \"w.env\"]\n";
        assert_eq!(compose_env_files(text), [".env", "./x.env", "./y.env", "z.env", "w.env"]);
    }

    #[test]
    fn multiline_env_file_flow_paths_handle_comments_and_quoted_commas() {
        let text = "services:\n  app:\n    env_file: [\n      # input files\n      .env,\n      \"x,y.env\", # comma in filename\n      './z.env'\n    ]\n";
        assert_eq!(compose_env_files(text), [".env", "x,y.env", "./z.env"]);
    }

    #[test]
    fn flow_style_environment_array_is_read() {
        let text = "services:\n  a:\n    environment: [A=1, \"B\", 'C=x y' , ]\n  b:\n    environment: {A: 1, B: ~, \"C\": \"two words\"}\n";
        assert_eq!(
            keys(text, compose(text)),
            [e("A", "1"), e("B", ""), e("C", "x y"), e("A", "1"), e("B", ""), e("C", "two words")]
        );
    }

    #[test]
    fn multiline_flow_environment_preserves_physical_key_ranges() {
        let text = "services:\r\n  a:\r\n    environment: [\r\n      # comment, ignored\r\n      A=1,\r\n      \"B=x,y\",\r\n      C=3 # end comment\r\n    ]\r\n  b:\r\n    environment: {\r\n      D: \"four, five\",\r\n      E: null\r\n    }\r\n";
        assert_eq!(keys(text, compose(text)), [e("A", "1"), e("B", "x,y"), e("C", "3"), e("D", "four, five"), e("E", "")]);
    }

    #[test]
    fn flow_quoted_closer_and_escaped_quotes_do_not_end_the_collection() {
        let text = r#"environment: {A: "a}b", B: "say \"hi\", again", C: 'it''s, fine'}"#;
        assert_eq!(compose(text).iter().map(|e| e.0.as_str()).collect::<Vec<_>>(), ["A", "B", "C"]);
        assert!(compose("environment: {A: {nested: value}}").is_empty());
        assert!(compose("environment: [A=1").is_empty());
    }

    #[test]
    fn local_anchor_merge_keys_preserve_declaration_ranges() {
        let text = "x-env: &env\n  X: 1\nservices:\n  a:\n    environment: *env\n  b:\n    environment:\n      <<: *env\n      Y: 2\n  c:\n    environment: &local\n      - *env\n      - Z=3\n";
        assert_eq!(keys(text, compose(text)), [e("X", "1"), e("Y", "2"), e("Z", "3")]);
    }

    #[test]
    fn environment_can_declare_and_reuse_an_inline_anchor() {
        let text = "services:\n  app:\n    environment: &env {A: one}\n  worker:\n    environment: *env\n";
        assert_eq!(keys(text, compose(text)), [e("A", "one")]);
    }

    #[test]
    fn merge_precedence_is_first_anchor_then_explicit_keys() {
        let text = "x-first: &first {A: one, B: first}\nx-second: &second {A: two, C: second}\nservices:\n  app:\n    environment:\n      <<: [*first, *second]\n      B: explicit\n";
        assert_eq!(keys(text, compose(text)), [e("A", "one"), e("C", "second"), e("B", "explicit")]);
    }

    #[test]
    fn anchored_lists_shadowing_and_unknown_aliases_are_bounded_source_candidates() {
        let text = "x-old: &env [A=old]\nx-latest: &env\n  - A=new\n  - B\nservices:\n  one:\n    environment: *env\n  two:\n    environment: *missing\n";
        assert_eq!(keys(text, compose(text)), [e("A", "new"), e("B", "")]);
        let forward = "environment: *later\nx-later: &later {A: value}\n";
        assert!(compose(forward).is_empty());
        let nested = "x-values: &env\n  A:\n    nested: value\nenvironment: *env\n";
        assert!(compose(nested).is_empty());
        let cycle = "x-values: &env\n  <<: *env\nenvironment: *env\n";
        assert!(compose(cycle).is_empty());
    }

    #[test]
    fn interpolated_items_are_not_declared() {
        let text = "services:\n  a:\n    environment: [${A}=1, \"${VAR}\", OK=1]\n  b:\n    environment:\n      - ${B}=2\n      - C=3\n";
        assert_eq!(keys(text, compose(text)), [e("OK", "1"), e("C", "3")]);
    }
}

#[cfg(test)]
mod env_file_graph_tests {
    use super::*;
    #[test]
    fn aliases_and_nested_service_merges_resolve_real_env_file_discovery() {
        let text="x-files: &files [base.env, {path: regional.env, required: false}]\nx-service: &service\n  env_file: *files\nx-nested: &nested\n  <<: *service\nservices:\n  app:\n    <<: *nested\n  worker:\n    env_file: [worker.env]\n";
        let (paths,complete,_optional)=compose_env_file_graph(text);
        assert!(complete);assert_eq!(paths,["worker.env","base.env","regional.env"]);
        let (paths,complete,_optional)=compose_env_file_graph("x-path: &path \"config/\\u0062ase.env\"\nservices: {app: {env_file: *path}}\n");
        assert!(complete);assert_eq!(paths,["config/base.env"]);
    }
    #[test]
    fn invalid_duplicate_dynamic_and_cyclic_graphs_never_prove_completeness() {
        for text in ["env_file: *undefined", "x: &cycle [*cycle]\nenv_file: *cycle", "env_file: one.env\nenv_file: two.env", "env_file: [one.env, [two.env]]", "env_file: !custom one.env", "services: {app: {environment: {NAME: {nested: value}}}}"] {
            assert!(!compose_env_file_graph(text).1,"{text}");
        }
    }
    #[test]
    fn explicit_bytes_depth_nodes_and_alias_replay_budgets_refuse_adversarial_graphs() {
        assert!(!compose_env_file_graph(&"x".repeat((256<<10)+1)).1);
        let deep=format!("env_file: {}one.env{}","[".repeat(64),"]".repeat(64));assert!(!compose_env_file_graph(&deep).1);
        let nodes=format!("values:\n{}","- x\n".repeat(20_001));assert!(!compose_env_file_graph(&nodes).1);
        let aliases=format!("x: &one one.env\nenv_file: [{}]",vec!["*one";129].join(","));assert!(!compose_env_file_graph(&aliases).1);
        let mut bomb="x0: &a0 [one.env]\n".to_owned();
        for n in 1..16 {bomb.push_str(&format!("x{n}: &a{n} [*a{},*a{}]\n",n-1,n-1));}
        bomb.push_str("env_file: *a15\n");assert!(!compose_env_file_graph(&bomb).1);
    }
}

#[cfg(test)]
mod expanded_budget_tests {
    use super::*;
    #[test]
    fn replayed_scalar_bytes_are_charged_before_json_tree_expansion() {
        let yaml=format!("large: &large {}\nuses: [{}]\n","x".repeat(64<<10),vec!["*large";32].join(","));
        assert!(yaml.len()<256<<10);
        let error=serde_saphyr::from_str_with_options::<serde_json::Value>(&yaml,compose_options()).unwrap_err();
        assert!(error.to_string().to_ascii_lowercase().contains("scalar"),"{error}");
    }
    #[test]
    fn optional_paths_stay_optional_only_if_no_source_requires_the_same_path() {
        let (paths,complete,optional)=compose_env_file_graph("services: {app: {env_file: [{path: missing.env, required: false}]}}\n");
        assert!(complete);assert_eq!(paths,["missing.env"]);assert!(optional.contains("missing.env"));
        let (_,complete,optional)=compose_env_file_graph("services: {app: {env_file: [{path: missing.env, required: false}]}, worker: {env_file: missing.env}}\n");
        assert!(complete);assert!(optional.is_empty());
        assert!(!compose_env_file_graph("env_file: [{path: [one.env], required: false}]\n").1);
    }
}
