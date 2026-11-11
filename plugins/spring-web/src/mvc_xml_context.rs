//! Effective XML context of an MVC config: bounded local `<import>` closure and `<beans profile>` blocks.
//! Quiet unless every file of the closure resolved locally and contains only beans/mvc elements.
use super::{BEANS, MVC, closed, model, plain, ref_sites, xml};
use openidea_plugin::Severity;
use spring_text::finding::Finding;
use std::collections::{BTreeMap, BTreeSet};

const MAX_DEPTH: usize = 4;
const MAX_FILES: usize = 16;
type Defs = BTreeMap<String, Vec<Option<String>>>;

struct Context<'a> {
    read: &'a dyn Fn(&str) -> Option<String>,
    seen: BTreeSet<String>,
    defs: Defs,
    complete: bool,
}

fn resolve(from: &str, resource: &str) -> Option<String> {
    let resource = resource.trim();
    if resource.is_empty() || resource.contains("${") || resource.contains("#{") {
        return None;
    }
    if let Some(rest) = resource.strip_prefix("classpath:") {
        if rest.contains(':') {
            return None;
        }
        let at = from.find("/src/main/resources/").or_else(|| from.find("/src/main/webapp/"))?;
        return normalise(&format!("{}/src/main/resources/{}", &from[..at], rest.trim_start_matches('/')));
    }
    if resource.contains(':') || resource.starts_with('/') {
        return None;
    }
    let dir = &from[..from.rfind('/')?];
    normalise(&format!("{dir}/{resource}"))
}

fn normalise(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = vec![];
    for part in path.split('/') {
        match part {
            "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

impl Context<'_> {
    fn visit(&mut self, path: &str, text: &str, inherited: Option<&str>, depth: usize) {
        if !self.seen.insert(path.to_string()) {
            return;
        }
        let es = xml::elements(text);
        let beans = |i: usize| xml::namespace(&es, i).is_none_or(|n| n == BEANS) && es[i].local() == "beans";
        if self.seen.len() > MAX_FILES || !closed(text, &es) || !beans(0) {
            self.complete = false;
            return;
        }
        for (i, e) in es.iter().enumerate() {
            if !xml::namespace(&es, i).is_none_or(|n| n == BEANS || n == MVC) {
                self.complete = false;
            }
            let Some(parent) = e.parent.filter(|p| beans(*p)) else {
                continue;
            };
            if xml::namespace(&es, i).is_some_and(|n| n != BEANS) {
                continue;
            }
            let mut profile = inherited.map(str::to_string);
            let mut at = Some(parent);
            while let Some(a) = at {
                if let Some(p) = es[a].value("profile") {
                    profile = Some(p.trim().to_string());
                    break;
                }
                at = es[a].parent;
            }
            match e.local() {
                "bean" => {
                    let names = e.value("id").into_iter().chain(e.value("name").into_iter().flat_map(|n| n.split([',', ';', ' '])));
                    for name in names.filter(|n| !n.is_empty()) {
                        self.defs.entry(name.to_string()).or_default().push(profile.clone());
                    }
                }
                "alias" => {
                    if let Some(alias) = e.value("alias") {
                        self.defs.entry(alias.to_string()).or_default().push(profile.clone());
                    }
                }
                "import" => {
                    let target = e.value("resource").and_then(|r| resolve(path, r));
                    let text = target.as_deref().filter(|_| depth < MAX_DEPTH).and_then(|t| (self.read)(t));
                    match (target, text) {
                        (Some(t), Some(text)) => self.visit(&t, &text, profile.as_deref(), depth + 1),
                        _ => self.complete = false,
                    }
                }
                _ => {}
            }
        }
    }
}

/// Bean references of an MVC XML file checked against its effective context.
/// `imported_elsewhere` is asked only when a reference is unknown: another file may import this one and declare the bean.
pub(super) fn check(
    path: &str,
    text: &str,
    read: &dyn Fn(&str) -> Option<String>,
    imported_elsewhere: &dyn Fn() -> bool,
) -> Vec<Finding> {
    let Some((es, profile)) = model(text) else {
        return vec![];
    };
    let sites = ref_sites(&es, profile);
    if sites.iter().all(|(v, ..)| v.is_empty()) {
        return vec![];
    }
    let mut context = Context { read, seen: BTreeSet::new(), defs: Defs::new(), complete: true };
    context.visit(path, text, None, 0);
    let mut out = vec![];
    let mut elsewhere = None;
    for (name, start, end) in sites.into_iter().filter(|(v, ..)| plain(v)) {
        match context.defs.get(&name) {
            Some(profiles) if profiles.iter().any(Option::is_none) => {}
            Some(profiles) => {
                let mut names: Vec<_> = profiles.iter().flatten().cloned().collect();
                names.dedup();
                out.push(
                    Finding::new(
                        start,
                        end,
                        "mvc-xml-bean-in-profile",
                        format!("Bean \"{name}\" is defined in profile {} only; it is absent when that profile is inactive.", names.join(", ")),
                    )
                    .with_severity(Severity::Info),
                );
            }
            None if context.complete && !*elsewhere.get_or_insert_with(imported_elsewhere) => out.push(
                Finding::new(
                    start,
                    end,
                    "mvc-xml-unknown-bean",
                    format!("Bean \"{name}\" is not declared in this XML context ({} file(s) resolved); beans from Java config or scanning are not considered.", context.seen.len()),
                )
                .with_severity(Severity::Info),
            ),
            None => {}
        }
    }
    out
}

/// `mvc:resources` locations as (location, start, end) bytes of each comma-separated item.
pub(super) fn resource_locations(text: &str) -> Vec<(String, usize, usize)> {
    let Some((es, _)) = model(text) else {
        return vec![];
    };
    let mut out = vec![];
    for (i, e) in es.iter().enumerate() {
        if super::path(&es, i).as_deref() != Some("resources") {
            continue;
        }
        let Some(a) = e.attr("location") else {
            continue;
        };
        let mut at = a.value_at;
        for item in a.value.split(',') {
            let trimmed = item.trim();
            if !trimmed.is_empty() {
                out.push((trimmed.to_string(), at + item.find(trimmed).unwrap_or(0), at + item.find(trimmed).unwrap_or(0) + trimmed.len()));
            }
            at += item.len() + 1;
        }
    }
    out
}

/// Globs (relative to any module) that prove a `classpath:` or web-root location exists; `None` when it cannot be judged.
pub(super) fn location_globs(location: &str) -> Option<Vec<String>> {
    if location.contains(['$', '#', '*', '?', '{', '[', '\\']) {
        return None;
    }
    let (roots, rest): (&[&str], &str) = match location.strip_prefix("classpath:") {
        Some(rest) => (&["src/main/resources"], rest),
        None if location.contains(':') => return None,
        None => (&["src/main/webapp"], location),
    };
    let rest = rest.trim_matches('/');
    if rest.is_empty() || rest.starts_with("META-INF") || rest.split('/').any(|p| p == ".." || p == ".") {
        return None;
    }
    Some(roots.iter().flat_map(|r| [format!("**/{r}/{rest}"), format!("**/{r}/{rest}/**")]).collect())
}
