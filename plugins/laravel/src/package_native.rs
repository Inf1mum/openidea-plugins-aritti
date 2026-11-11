//! Literal NativePHP child component registrations. Element aliases win, and
//! ambiguous or runtime-generated registrations are explicitly incomplete.
use super::*;
fn child(read: &mut Read<'_>, roots: &[ComposerRoot], class: &str) -> Option<Entry> {
    let (path, source) = source_for(read, roots, class, None)?;
    let code = crate::package_facts::code(&source)?;
    let at = code.find("extends ")?;
    let parent = code[at + 8..]
        .trim_start()
        .split(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, '_' | '\\'))
        .next()?;
    if crate::eloquent_members::metadata_inference::class_identity(&source, parent).as_deref()
        != Some("Native\\Mobile\\Edge\\NativeComponent")
    {
        return None;
    }
    Some(Entry {
        path: format!("{}/{path}", read.root),
        at: Position::new(0, 0),
        width: 0,
        detail: Some(format!("NativePHP child component {class}")),
    })
}
fn tag(value: &str) -> Option<String> {
    (!value.is_empty()
        && value.len() < 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')))
    .then(|| format!("native:{}", value.replace('_', "-").to_ascii_lowercase()))
}
pub(super) fn apply(
    read: &mut Read<'_>,
    index: &mut Index,
    roots: &[ComposerRoot],
    facts: &mut Facts,
) {
    if index.metadata.package_major("nativephp/mobile") != Some(4) {
        return;
    }
    let Some((registry_path, registry)) = source_for(
        read,
        roots,
        "Native\\Mobile\\Edge\\ComponentRegistry",
        Some("nativephp/mobile"),
    ) else {
        return;
    };
    let Some(registry_code) = crate::package_facts::code(&registry) else {
        return;
    };
    if !registry_code.contains("function components(")
        || !registry_code.contains("function register(")
    {
        return;
    }
    index.dependency_files.insert(registry_path);
    let mut explicit: BTreeMap<String, BTreeMap<String, Entry>> = BTreeMap::new();
    for (path, source) in app_sources(read, index) {
        for method in ["components", "register"] {
            for open in static_calls(&source, method) {
                if !class_call(&source, open, "Native\\Mobile\\Edge\\ComponentRegistry") {
                    continue;
                }
                let args = php::arguments(&source, open);
                let mut registrations = Vec::new();
                let mut opaque = false;
                if method == "components" {
                    if let Some(array) = args
                        .first()
                        .and_then(|r| php::array_open(&source, r.clone()))
                    {
                        let items = php::items(&source, array);
                        if items.len() > 128 {
                            opaque = true;
                        } else {
                            for item in items {
                                let Some((name, _)) = item.key.and_then(|r| literal(&source, r))
                                else {
                                    opaque = true;
                                    continue;
                                };
                                let Some(class) = php::class_constant(&source, item.value)
                                    .and_then(|c| {
                                        crate::eloquent_members::metadata_inference::class_identity(
                                            &source, c,
                                        )
                                    })
                                else {
                                    opaque = true;
                                    continue;
                                };
                                registrations.push((name.to_owned(), class));
                            }
                        }
                    } else {
                        opaque = true;
                    }
                } else if args.len() == 2 {
                    if let (Some((name, _)), Some(class)) = (
                        literal(&source, args[0].clone()),
                        php::class_constant(&source, args[1].clone()).and_then(|c| {
                            crate::eloquent_members::metadata_inference::class_identity(&source, c)
                        }),
                    ) {
                        registrations.push((name.into(), class));
                    } else {
                        opaque = true;
                    }
                } else {
                    opaque = true;
                }
                for (name, class) in registrations {
                    let Some(tag) = tag(&name) else {
                        opaque = true;
                        continue;
                    };
                    if facts.native.contains_key(&tag) {
                        continue;
                    }
                    let Some(entry) = child(read, roots, &class) else {
                        opaque = true;
                        continue;
                    };
                    index.dependency_files.insert(
                        entry
                            .path
                            .trim_start_matches(&format!("{}/", read.root))
                            .into(),
                    );
                    explicit
                        .entry(tag)
                        .or_default()
                        .insert(entry.path.clone(), entry);
                }
                if opaque {
                    facts.native_partial = true;
                    facts.issues.push(format!("{path}: NativePHP child registration uses unresolved runtime values; the static Native component inventory is partial"));
                }
            }
        }
    }
    let mut blocked = BTreeSet::new();
    for (tag, entries) in explicit {
        if entries.len() == 1 {
            facts
                .native
                .insert(tag, entries.into_values().next().unwrap());
        } else {
            blocked.insert(tag.clone());
            facts.native_partial = true;
            facts.issues.push(format!("NativePHP child alias {tag} has multiple source registrations; navigation is withheld"));
        }
    }
    // Auto-discovery is enabled only when the actual installed provider declares
    // the documented child-component discovery method and App namespace contract.
    let Some((provider_path, provider)) = source_for(
        read,
        roots,
        "Native\\Mobile\\NativeServiceProvider",
        Some("nativephp/mobile"),
    ) else {
        return;
    };
    let Some(code) = crate::package_facts::code(&provider) else {
        return;
    };
    if !auto_declared(&provider, &code) {
        return;
    }
    index.dependency_files.insert(provider_path);
    index.dependency_roots.insert("app/NativeComponents".into());
    let mut auto: BTreeMap<String, BTreeMap<String, Entry>> = BTreeMap::new();
    for path in read.find("app/NativeComponents/**/*.php") {
        let Some(source) = read.read(&path) else {
            continue;
        };
        let Some(class) = crate::eloquent_members::declaration(&source) else {
            continue;
        };
        if !class.starts_with("App\\NativeComponents\\") {
            continue;
        }
        let Some(entry) = child(read, roots, &class) else {
            continue;
        };
        let short = class.rsplit('\\').next().unwrap();
        let kebab = short
            .chars()
            .enumerate()
            .flat_map(|(i, c)| {
                let mut chars = Vec::new();
                if i > 0 && c.is_ascii_uppercase() {
                    chars.push('-');
                }
                chars.push(c.to_ascii_lowercase());
                chars
            })
            .collect::<String>();
        let Some(tag) = tag(&kebab) else {
            continue;
        };
        if facts.native.contains_key(&tag) || blocked.contains(&tag) {
            continue;
        }
        auto.entry(tag)
            .or_default()
            .insert(entry.path.clone(), entry);
    }
    for (tag, entries) in auto {
        if entries.len() == 1 {
            facts
                .native
                .insert(tag, entries.into_values().next().unwrap());
        } else {
            facts.native_partial = true;
            facts.issues.push(format!(
                "NativePHP auto-discovered alias {tag} is ambiguous; navigation is withheld"
            ));
        }
    }
}

fn auto_declared(source: &str, code: &str) -> bool {
    let Some(method) = code.find("function registerChildComponents(") else {
        return false;
    };
    let Some(body) = code[method..].find('{').map(|n| n + method) else {
        return false;
    };
    let Some(end) = php::closing(source, body) else {
        return false;
    };
    let body_code = &code[body + 1..end];
    let path = body_code.match_indices("app_path").any(|(at, _)| {
        let at = body + 1 + at;
        let rest = code[at + 8..].trim_start();
        if !rest.starts_with('(') {
            return false;
        }
        let open = code.len() - rest.len();
        let Some(call) = crate::eloquent_members::metadata_inference::call_identity(source, open)
        else {
            return false;
        };
        if call
            .function_fqn
            .as_deref()
            .is_some_and(|f| f != "app_path")
        {
            return false;
        }
        php::arguments(source, open)
            .first()
            .and_then(|r| php::literal(source, r.clone()))
            .is_some_and(|(v, _)| v == "NativeComponents")
    });
    let prefix = body_code.match_indices("$componentClass").any(|(at, _)| {
        let at = body + 1 + at + "$componentClass".len();
        let raw = source[at..].trim_start();
        let Some(raw) = raw.strip_prefix('=').map(str::trim_start) else {
            return false;
        };
        let start = source.len() - raw.len();
        if !matches!(source.as_bytes().get(start), Some(b'\'' | b'"')) {
            return false;
        }
        let finish = php::string_end(source.as_bytes(), start);
        php::literal(source, start..finish)
            .and_then(|(v, at)| {
                crate::eloquent_members::metadata_inference::decode_literal(
                    v,
                    source.as_bytes()[at - 1],
                )
            })
            .as_deref()
            == Some("App\\NativeComponents\\")
    });
    path && prefix
}
pub(super) fn element_props(source: &str, path: &str) -> Names {
    let mut out = Names::new();
    let Some(code) = crate::package_facts::code(source) else {
        return out;
    };
    let Some(method) = code.find("function applyAttributes(") else {
        return out;
    };
    let Some(open) = code[method..].find('(').map(|n| method + n) else {
        return out;
    };
    let params = php::arguments(source, open);
    let Some(param) = params.first() else {
        return out;
    };
    let Some(dollar) = code[param.clone()].find('$').map(|n| param.start + n) else {
        return out;
    };
    let len = code[dollar + 1..]
        .bytes()
        .take_while(|b| php::is_identifier(*b))
        .count();
    if len == 0 {
        return out;
    }
    let variable = &code[dollar..dollar + 1 + len];
    let Some(close) = php::closing(source, open) else {
        return out;
    };
    let Some(body) = code[close + 1..].find('{').map(|n| close + 1 + n) else {
        return out;
    };
    let Some(end) = php::closing(source, body) else {
        return out;
    };
    let needle = format!("{variable}[");
    for (at, _) in code[body..end].match_indices(&needle).take(256) {
        let bracket = body + at + variable.len();
        let Some(close) = php::closing(source, bracket) else {
            continue;
        };
        let Some((name, start)) = literal(source, bracket + 1..close) else {
            continue;
        };
        if !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            continue;
        }
        let normalized = crate::components::attribute_name(name);
        out.entry(normalized)
            .or_insert_with(|| entry(path, source, start, name, "NativePHP element attribute"));
    }
    out
}
pub(crate) fn props(
    index: &Index,
    project: &Project,
    tag: &str,
) -> Option<Vec<crate::components::Prop>> {
    let names = index.packages.native_element_props.get(tag)?;
    let mut read = Read::new(project)?;
    let mut props = Vec::new();
    let mut sources = BTreeMap::new();
    for (name, entry) in names {
        if !sources.contains_key(&entry.path) {
            sources.insert(entry.path.clone(), read.read(&entry.path));
        }
        let Some(Some(source)) = sources.get(&entry.path) else {
            continue;
        };
        let Some(start) = text::offset(&source, entry.at) else {
            continue;
        };
        let end = source[start..]
            .char_indices()
            .scan(0u32, |width, (at, ch)| {
                if *width >= entry.width {
                    return None;
                }
                *width += ch.len_utf16() as u32;
                Some(start + at + ch.len_utf8())
            })
            .last();
        let Some(end) = end else {
            continue;
        };
        if crate::components::attribute_name(&source[start..end]) != *name {
            continue;
        }
        props.push(crate::components::Prop {
            name: name.clone(),
            default: None,
        });
    }
    Some(props)
}
pub(super) fn attribute_definition(index: &Index, doc: &Document, at: Position) -> Vec<Location> {
    let run = || {
        if doc.language != "blade" {
            return None;
        }
        let at = text::offset(&doc.text, at)?;
        let crate::components::Context::Attribute {
            component, start, ..
        } = crate::components::context_with_metadata(index, &doc.text, at)?
        else {
            return None;
        };
        let end = start
            + doc.text[start..]
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                .count();
        if at > end {
            return None;
        }
        let name = crate::components::attribute_name(&doc.text[start..end]);
        let names = index.packages.native_element_props.get(&component)?;
        let entry = names.get(&name)?;
        let project = crate::project_for(&doc.path)?;
        if !props(index, &project, &component)?
            .iter()
            .any(|p| p.name == name)
        {
            return None;
        }
        crate::scoped_location(&project, entry)
    };
    run().into_iter().collect()
}
