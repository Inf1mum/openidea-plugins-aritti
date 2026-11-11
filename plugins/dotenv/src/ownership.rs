//! Which Compose services load an env file, and which Dockerfile `ARG`s a
//! service's `build.args` feeds from a `${VAR}`. Line-based and conservative:
//! aliases, merges, anchors and anything unusual are skipped, never guessed.

use std::ops::Range;

#[derive(Debug, Default, PartialEq)]
pub struct Service {
    pub name: String,
    /// `(path as written, 1-based line)`.
    pub env_files: Vec<(String, usize)>,
    pub context: Option<String>,
    pub dockerfile: Option<String>,
    /// `(arg, interpolated variable, 1-based line)` for `K: ${VAR}` / `- K=${VAR}`.
    pub build_args: Vec<(String, String, usize)>,
}

struct Line<'a> {
    no: usize,
    indent: usize,
    text: &'a str,
}

fn clean(value: &str) -> Option<String> {
    let value = value.split(" #").next().unwrap_or(value).trim();
    let value = ['"', '\''].iter().find_map(|q| value.strip_prefix(*q)?.strip_suffix(*q)).unwrap_or(value);
    (!value.is_empty() && !value.starts_with(['*', '&', '[', '{', '!', '|', '>'])).then(|| value.to_owned())
}

fn pair(text: &str) -> Option<(&str, &str)> {
    let (key, rest) = text.split_once(':')?;
    let plain = !key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'));
    (plain && (rest.is_empty() || rest.starts_with(' '))).then_some((key, rest.trim()))
}

fn children<'a, 'b>(lines: &'b [Line<'a>], i: usize) -> &'b [Line<'a>] {
    let parent = lines[i].indent;
    let len = lines[i + 1..].iter().take_while(|l| l.indent > parent).count();
    &lines[i + 1..i + 1 + len]
}

/// `$VAR`, `${VAR}`, `${VAR:-x}`: the whole value is one interpolation of `VAR`.
fn sole_variable(value: &str) -> Option<String> {
    let rest = value.strip_prefix('$')?;
    let (name, tail) = match rest.strip_prefix('{') {
        Some(braced) => {
            let end = braced.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(braced.len());
            let tail = &braced[end..];
            let inner = tail.strip_suffix('}')?;
            if !(inner.is_empty() || inner.starts_with([':', '-', '?', '+'])) || inner.contains("${") {
                return None;
            }
            (&braced[..end], "")
        }
        None => (rest, ""),
    };
    let valid = name.bytes().enumerate().all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()));
    (valid && !name.is_empty() && tail.is_empty()).then(|| name.to_owned())
}

pub fn compose_services(text: &str) -> Vec<Service> {
    if text.len() > 256 << 10 {
        return Vec::new();
    }
    let lines: Vec<Line> = text
        .lines()
        .enumerate()
        .filter_map(|(n, raw)| {
            let t = raw.trim_start();
            (!t.is_empty() && !t.starts_with('#') && !raw.contains('\t')).then(|| Line { no: n + 1, indent: raw.len() - t.len(), text: t.trim_end() })
        })
        .collect();
    let Some(root) = lines.iter().position(|l| l.indent == 0 && l.text == "services:") else { return Vec::new() };
    let mut out = Vec::new();
    for (si, sline) in children(&lines, root).iter().enumerate().map(|(i, l)| (root + 1 + i, l)) {
        let first = lines[root + 1].indent;
        if sline.indent != first {
            continue;
        }
        let Some((name, rest)) = pair(sline.text) else { continue };
        if !rest.is_empty() || name.contains('.') {
            continue;
        }
        let mut service = Service { name: name.to_owned(), ..Default::default() };
        let body = children(&lines, si);
        for (bi, l) in body.iter().enumerate() {
            let at = si + 1 + bi;
            if l.indent != body[0].indent {
                continue;
            }
            match pair(l.text) {
                Some(("env_file", rest)) => env_files(&lines, at, rest, &mut service),
                Some(("build", rest)) => build(&lines, at, rest, &mut service),
                _ => {}
            }
        }
        out.push(service);
    }
    out
}

fn env_files(lines: &[Line], at: usize, rest: &str, service: &mut Service) {
    let line = lines[at].no;
    if !rest.is_empty() && !rest.starts_with('#') {
        if let Some(inner) = rest.strip_prefix('[').and_then(|r| r.split(']').next()).filter(|_| rest.contains(']')) {
            service.env_files.extend(inner.split(',').filter_map(clean).map(|p| (p, line)));
        } else if let Some(path) = clean(rest) {
            service.env_files.push((path, line));
        }
        return;
    }
    let kids = children(lines, at);
    let Some(first) = kids.first() else { return };
    for kid in kids.iter().filter(|k| k.indent == first.indent) {
        let item = kid.text.strip_prefix('-').map(str::trim_start);
        let path = match item {
            Some(item) => match pair(item) {
                Some(("path", v)) => clean(v),
                Some(_) => None,
                None => clean(item),
            },
            None => pair(kid.text).filter(|(k, _)| *k == "path").and_then(|(_, v)| clean(v)),
        };
        if let Some(path) = path {
            service.env_files.push((path, kid.no));
        }
    }
}

fn build(lines: &[Line], at: usize, rest: &str, service: &mut Service) {
    if !rest.is_empty() && !rest.starts_with('#') {
        service.context = clean(rest);
        return;
    }
    let kids = children(lines, at);
    let Some(first) = kids.first() else { return };
    for (k, kid) in kids.iter().enumerate().filter(|(_, k)| k.indent == first.indent) {
        match pair(kid.text) {
            Some(("context", v)) => service.context = clean(v),
            Some(("dockerfile", v)) => service.dockerfile = clean(v),
            Some(("args", v)) if v.is_empty() => {
                let sub = &kids[k + 1..];
                let Some(item_indent) = sub.first().map(|l| l.indent).filter(|i| *i > kid.indent) else { continue };
                for arg in sub.iter().take_while(|l| l.indent > kid.indent).filter(|l| l.indent == item_indent) {
                    let (key, value) = match arg.text.strip_prefix('-') {
                        Some(item) => match item.trim_start().split_once('=') {
                            Some(kv) => kv,
                            None => continue,
                        },
                        None => match pair(arg.text) {
                            Some(kv) => kv,
                            None => continue,
                        },
                    };
                    let Some(value) = clean(value) else { continue };
                    if let Some(var) = sole_variable(&value) {
                        service.build_args.push((key.trim().to_owned(), var, arg.no));
                    }
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct DockerArg {
    pub name: String,
    /// `None` before the first `FROM`: usable in `FROM` lines only.
    pub stage: Option<String>,
    pub range: Range<usize>,
    /// `ARG K` without a value: inside a stage it re-imports a global `ARG K`.
    pub reimports_global: bool,
    /// `${K}` / `$K` expansions after the declaration, in the same stage.
    pub uses: Vec<Range<usize>>,
    /// Stage this one is `FROM`: it inherits the ARGs declared in that stage.
    pub based_on: Option<String>,
}

/// Names expanded in `line` (`$K`, `${K}`, `${K:-d}`) with their byte ranges.
fn expansions(line: &str, base: usize) -> Vec<(&str, Range<usize>)> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'\'' {
            i += line[i + 1..].find('\'').map_or(bytes.len(), |end| end + 2);
            continue;
        }
        if bytes[i] != b'$' {
            i += 1;
            continue;
        }
        let start = if bytes.get(i + 1) == Some(&b'{') { i + 2 } else { i + 1 };
        let len = line[start..].bytes().take_while(|b| b.is_ascii_alphanumeric() || *b == b'_').count();
        if len > 0 && !bytes[start].is_ascii_digit() {
            out.push((&line[start..start + len], base + start..base + start + len));
        }
        i = start + len.max(1);
    }
    out
}

/// Instructions whose variables Docker itself expands (FROM is handled apart).
const DOCKER_EXPANDS: [&str; 9] = ["ADD", "COPY", "ENV", "EXPOSE", "LABEL", "STOPSIGNAL", "USER", "VOLUME", "WORKDIR"];
/// Shell-form instructions: expansion is the shell's, so only unquoted/double-quoted `$K` counts.
const SHELL_EXPANDS: [&str; 3] = ["RUN", "CMD", "ENTRYPOINT"];

/// `ARG` declarations with the build stage they sit in, per
/// docs.docker.com/reference/dockerfile/#arg: an `ARG` before `FROM` is outside
/// every stage; one after a `FROM` belongs to that stage only.
pub fn dockerfile_args(text: &str) -> Vec<DockerArg> {
    let mut out = Vec::new();
    let mut stage: Option<String> = None;
    let mut stages = 0;
    let mut stage_names: Vec<String> = Vec::new();
    // (first arg index, parent stage index) per stage
    let mut stage_info: Vec<(usize, Option<usize>)> = Vec::new();
    let mut based_on: Option<String> = None;
    let mut offset = 0;
    for raw in text.split_inclusive('\n') {
        let line_at = offset;
        offset += raw.len();
        let body = raw.trim_end_matches(['\r', '\n']);
        let content = body.trim_start();
        let at = line_at + body.len() - content.len();
        let mut parts = content.split_whitespace();
        let Some(instruction) = parts.next() else { continue };
        if instruction.eq_ignore_ascii_case("FROM") {
            stages += 1;
            let words: Vec<_> = parts.collect();
            let alias = words.iter().position(|w| w.eq_ignore_ascii_case("AS")).and_then(|i| words.get(i + 1));
            let name = alias.map_or_else(|| stages.to_string(), |a| (*a).to_owned());
            let parent = words
                .iter()
                .find(|w| !w.starts_with("--"))
                .and_then(|image| stage_names.iter().position(|n| n.eq_ignore_ascii_case(image)));
            based_on = parent.map(|p| stage_names[p].clone());
            stage_names.push(name.clone());
            stage = Some(name);
            stage_info.push((out.len(), parent));
        } else if instruction.eq_ignore_ascii_case("ARG") {
            let mut cursor = at + instruction.len();
            for word in parts {
                let start = cursor + text[cursor..].find(word).unwrap_or(0);
                cursor = start + word.len();
                let name = word.split('=').next().unwrap_or(word);
                if !name.is_empty() && !name.contains('$') {
                    out.push(DockerArg {
                        name: name.to_owned(),
                        stage: stage.clone(),
                        range: start..start + name.len(),
                        reimports_global: !word.contains('='),
                        uses: Vec::new(),
                        based_on: based_on.clone(),
                    });
                }
            }
        } else if stage.is_some() {
            let (word, rest) = if instruction.eq_ignore_ascii_case("ONBUILD") {
                let rest = content[instruction.len()..].trim_start();
                (rest.split_whitespace().next().unwrap_or(""), rest)
            } else {
                (instruction, content)
            };
            let docker_expands = DOCKER_EXPANDS.iter().any(|k| word.eq_ignore_ascii_case(k));
            let shell_form = SHELL_EXPANDS.iter().any(|k| word.eq_ignore_ascii_case(k))
                && !rest[word.len()..].trim_start().starts_with('[');
            if docker_expands || shell_form {
                let at = at + (content.len() - rest.len());
                for (name, used) in expansions(rest, at) {
                    let mut current = Some(stage_info.len() - 1);
                    while let Some(index) = current {
                        let (first, parent) = stage_info[index];
                        let end = stage_info.get(index + 1).map_or(out.len(), |next| next.0);
                        if let Some(arg) = out[first..end].iter_mut().rev().find(|a: &&mut DockerArg| a.name == name) {
                            arg.uses.push(used);
                            break;
                        }
                        current = parent;
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn services_env_files_in_every_form() {
        let text = "services:\n  web:\n    env_file: a.env\n  api:\n    env_file:\n      - b.env\n      - path: c.env\n        required: false\n  db:\n    env_file: [d.env, \"e.env\"]\n  x:\n    env_file: *anchor\n";
        let s = compose_services(text);
        let got: Vec<_> = s.iter().map(|s| (s.name.as_str(), s.env_files.clone())).collect();
        assert_eq!(got, [
            ("web", vec![("a.env".to_owned(), 3)]),
            ("api", vec![("b.env".to_owned(), 6), ("c.env".to_owned(), 7)]),
            ("db", vec![("d.env".to_owned(), 10), ("e.env".to_owned(), 10)]),
            ("x", vec![]),
        ]);
    }

    #[test]
    fn build_args_keep_only_sole_variables() {
        let text = "services:\n  web:\n    build:\n      context: ./app\n      dockerfile: Dockerfile.dev\n      args:\n        PHP: ${PHP_VERSION:-8}\n        LIT: 1\n        MIX: x${A}\n        SH:\n";
        let s = &compose_services(text)[0];
        assert_eq!(s.context.as_deref(), Some("./app"));
        assert_eq!(s.dockerfile.as_deref(), Some("Dockerfile.dev"));
        assert_eq!(s.build_args, [("PHP".to_owned(), "PHP_VERSION".to_owned(), 7)]);
        let list = "services:\n  w:\n    build:\n      args:\n        - PHP=$V\n        - OTHER\n";
        assert_eq!(compose_services(list)[0].build_args, [("PHP".to_owned(), "V".to_owned(), 5)]);
    }

    #[test]
    fn arg_stages_follow_from_lines() {
        let text = "ARG BASE=1\nFROM php:$BASE AS build\nARG PHP\nFROM x\nARG A=1 B\n";
        let got: Vec<_> = dockerfile_args(text).into_iter().map(|a| (a.name, a.stage)).collect();
        assert_eq!(got, [
            ("BASE".to_owned(), None),
            ("PHP".to_owned(), Some("build".to_owned())),
            ("A".to_owned(), Some("2".to_owned())),
            ("B".to_owned(), Some("2".to_owned())),
        ]);
    }

    #[test]
    fn arg_uses_reimport_and_stage_base() {
        let text = "ARG V=1\nFROM php AS base\nFROM base AS app\nARG V\nRUN echo ${V:-x} $V $VV '$V' \\$V\nENV A=$V\nCMD [\"echo\", \"$V\"]\nARG W=$V\nHEALTHCHECK CMD echo $V\n";
        let args = dockerfile_args(text);
        let stage = args.iter().find(|a| a.stage.is_some()).unwrap();
        assert!(stage.reimports_global);
        assert_eq!(stage.based_on.as_deref(), Some("base"));
        let names: Vec<_> = stage.uses.iter().map(|r| &text[r.clone()]).collect();
        assert_eq!(names, ["V", "V", "V"]);
        assert!(args[0].uses.is_empty() && args[0].stage.is_none());
    }

    #[test]
    fn arg_declared_in_parent_stage_is_inherited() {
        let text = "FROM php AS base\nARG P=1\nFROM base AS app\nCOPY $P /x\nFROM php AS other\nCOPY $P /y\n";
        let args = dockerfile_args(text);
        assert_eq!(args.len(), 1);
        let names: Vec<_> = args[0].uses.iter().map(|r| &text[r.clone()]).collect();
        assert_eq!(names, ["P"]);
        assert_eq!(text[..args[0].uses[0].start].matches('\n').count(), 3);
    }
}
