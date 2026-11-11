//! Configuration files that are not named `application*` or `bootstrap*`: the ones
//! `spring.config.import`, `spring.config.location`, `spring.config.additional-location`
//! and `spring.config.name` of the standard files point at.

use spring_text::config::{self, canonical, is_config_file};
use std::collections::{BTreeSet, VecDeque};

pub const GLOBS: &[&str] = &[
    "**/application*.properties",
    "**/application*.yml",
    "**/application*.yaml",
    "**/bootstrap*.properties",
    "**/bootstrap*.yml",
    "**/bootstrap*.yaml",
];

const LOCATION_KEYS: &[&str] = &[
    "spring.config.import",
    "spring.config.location",
    "spring.config.additional-location",
];
const EXTENSIONS: &[&str] = &["properties", "yml", "yaml"];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Locations {
    /// Paths that end a file's path (`config/extra.yml`).
    pub tails: Vec<String>,
    /// Base names (`myapp` for `myapp.yml` and `myapp-dev.yml`).
    pub names: Vec<String>,
    /// Import-relative locations (`core/core.yml`): resolved against the importing file.
    pub relative: Vec<String>,
    /// Literal profile names from `spring.profiles.include`, `spring.profiles.group.*` and `spring.config.activate.on-profile`.
    pub profiles: Vec<String>,
}

pub const MAX_DEPTH: usize = 8;
pub const MAX_FILES: usize = 64;
const MAX_READS: usize = 512;

/// A literal file location: `(path, fixed)`. Per Spring Boot, a location starting with `/` or
/// a scheme is fixed; any other one is relative to the file that declares it.
fn tail_of(location: &str) -> Option<(String, bool)> {
    let mut one = location.trim().trim_matches(['"', '\'']);
    one = one.strip_prefix("optional:").unwrap_or(one);
    let mut fixed = one.starts_with('/');
    for scheme in ["classpath*:", "classpath:", "file:"] {
        if let Some(rest) = one.strip_prefix(scheme) {
            one = rest;
            fixed = true;
        }
    }
    if one.contains([':', '*', '$', '[']) {
        return None;
    }
    let one = if fixed {
        one.trim_start_matches("./").trim_start_matches('/')
    } else {
        one
    };
    let extension = one.rsplit_once('.')?.1;
    EXTENSIONS
        .contains(&extension)
        .then(|| (one.to_owned(), fixed))
}

fn profile_name(raw: &str) -> Option<String> {
    let name = raw.trim().trim_matches(['"', '\'']);
    (!name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
    .then(|| name.to_owned())
}

/// `prod | staging` yields both; anything with `!`, `&` or parentheses is not a plain name list.
fn on_profile_names(expression: &str) -> Vec<String> {
    if expression.contains(['!', '&', '(', ')', '$']) {
        return Vec::new();
    }
    expression.split('|').filter_map(profile_name).collect()
}

fn is_profile_specific(path: &str) -> bool {
    crate::nav::profile_of(path).is_some()
}

pub fn read(path: &str, text: &str) -> Option<Locations> {
    if !is_config_file(path) {
        return None;
    }
    parse(path, text)
}

/// Like `read` for a file that is already known to be configuration (an imported one).
pub fn parse(path: &str, text: &str) -> Option<Locations> {
    let mut found = Locations::default();
    let inheriting = !is_profile_specific(path);
    for key in config::keys(path, text) {
        let base = key.key.split('[').next().unwrap_or(&key.key);
        let base = canonical(base);
        if LOCATION_KEYS.contains(&base.as_str()) {
            for (location, fixed) in key.value.split([',', ';']).filter_map(tail_of) {
                if fixed || base != "spring.config.import" {
                    found.tails.push(location);
                } else {
                    found.relative.push(location);
                }
            }
        } else if base == "spring.config.activate.onprofile" {
            found.profiles.extend(on_profile_names(&key.value));
        } else if inheriting && base == "spring.profiles.include" {
            found
                .profiles
                .extend(key.value.split(',').filter_map(profile_name));
        } else if let Some(group) = inheriting
            .then(|| {
                key.key
                    .split('[')
                    .next()
                    .and_then(|raw| raw.strip_prefix("spring.profiles.group."))
            })
            .flatten()
        {
            found.profiles.extend(profile_name(group));
            found
                .profiles
                .extend(key.value.split(',').filter_map(profile_name));
        } else if base == "spring.config.name" {
            found.names.extend(
                key.value
                    .split(',')
                    .map(|n| n.trim().to_owned())
                    .filter(|n| !n.is_empty() && !n.contains("${")),
            );
        }
    }
    found.profiles.sort();
    found.profiles.dedup();
    (found != Locations::default()).then_some(found)
}

/// `rel` resolved against the directory of the importing file; `..` above the root is refused.
pub fn resolve(importer: &str, rel: &str) -> Option<String> {
    let importer = importer.replace('\\', "/");
    let mut parts: Vec<&str> = importer.split('/').collect();
    parts.pop();
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.len() <= 1 {
                    return None;
                }
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// `dir/name.ext` as `dir/name-<profile>.ext`.
fn variant(path: &str, profile: &str) -> Option<String> {
    let (stem, extension) = path.rsplit_once('.')?;
    Some(format!("{stem}-{profile}.{extension}"))
}

/// Whether Spring Boot reads `path` as configuration, given what the standard files point at
/// (`sets` pairs each file's path with what it declares).
pub fn is_configuration<'a>(
    path: &str,
    sets: impl Iterator<Item = (&'a str, &'a Locations)>,
) -> bool {
    if is_config_file(path) {
        return true;
    }
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let Some((stem, extension)) = file.rsplit_once('.') else {
        return false;
    };
    if !EXTENSIONS.contains(&extension) {
        return false;
    }
    let normalized = path.replace('\\', "/");
    let sets: Vec<_> = sets.collect();
    let profiles: Vec<&str> = sets
        .iter()
        .flat_map(|(_, set)| set.profiles.iter().map(String::as_str))
        .collect();
    let ends = |tail: &str| normalized == tail || normalized.ends_with(&format!("/{tail}"));
    sets.iter().any(|(importer, set)| {
        set.tails.iter().any(|t| {
            ends(t)
                || profiles
                    .iter()
                    .any(|p| variant(t, p).is_some_and(|v| ends(&v)))
        }) || set
            .relative
            .iter()
            .filter_map(|r| resolve(importer, r))
            .any(|r| {
                normalized == r
                    || profiles
                        .iter()
                        .any(|p| variant(&r, p).is_some_and(|v| normalized == v))
            })
            || set.names.iter().any(|n| {
                stem == n
                    || stem
                        .strip_prefix(n.as_str())
                        .is_some_and(|rest| rest.starts_with('-'))
            })
    })
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Discovery {
    /// Imported files, and `<name>-<profile>` variants of them, that exist.
    pub files: Vec<String>,
    /// Literal profile names declared by the standard files and everything they import.
    pub profiles: BTreeSet<String>,
}

/// Follows literal `spring.config.import` chains (depth <= 8, files <= 64, cycle-safe) from the
/// standard files in `seeds`. `find` lists project files matching a glob; `read` returns a file's text.
/// Profile variants are only looked for under profiles that some file names literally.
pub fn discover(
    seeds: &[(String, Locations)],
    find: &dyn Fn(&str) -> Vec<String>,
    read: &dyn Fn(&str) -> Option<String>,
) -> Discovery {
    let mut out = Discovery::default();
    let mut seen: BTreeSet<String> = seeds.iter().map(|(p, _)| p.clone()).collect();
    let mut queue: VecDeque<(String, Locations, usize)> = VecDeque::new();
    let mut imported: Vec<(String, usize)> = Vec::new();
    let mut reads = 0;
    for (path, set) in seeds {
        out.profiles.extend(set.profiles.iter().cloned());
        queue.push_back((path.clone(), set.clone(), 1));
    }
    loop {
        while let Some((importer, set, depth)) = queue.pop_front() {
            if depth > MAX_DEPTH {
                continue;
            }
            let mut targets: Vec<String> = set.tails.iter().flat_map(|t| find(&format!("**/{t}"))).collect();
            targets.extend(set.relative.iter().filter_map(|r| resolve(&importer, r)));
            for target in targets {
                visit(&target, depth, &mut seen, &mut imported, &mut out, &mut queue, read, &mut reads);
            }
        }
        let known: Vec<String> = out.profiles.iter().cloned().collect();
        let bases = imported.clone();
        for (base, depth) in bases {
            for profile in &known {
                if let Some(candidate) = variant(&base, profile) {
                    visit(&candidate, depth, &mut seen, &mut imported, &mut out, &mut queue, read, &mut reads);
                }
            }
        }
        if queue.is_empty() {
            break;
        }
    }
    out.files.sort();
    out
}

pub fn discover_in(
    project: &openidea_plugin::Project,
    seeds: &[(String, Locations)],
) -> Discovery {
    discover(
        seeds,
        &|glob| project.find_files(glob, 20).unwrap_or_default(),
        &|path| project.read_file(path).ok(),
    )
}

#[allow(clippy::too_many_arguments)]
fn visit(
    target: &str,
    depth: usize,
    seen: &mut BTreeSet<String>,
    imported: &mut Vec<(String, usize)>,
    out: &mut Discovery,
    queue: &mut VecDeque<(String, Locations, usize)>,
    read: &dyn Fn(&str) -> Option<String>,
    reads: &mut usize,
) {
    if out.files.len() >= MAX_FILES || *reads >= MAX_READS || !seen.insert(target.to_owned()) {
        return;
    }
    *reads += 1;
    let Some(text) = read(target) else {
        return;
    };
    out.files.push(target.to_owned());
    imported.push((target.to_owned(), depth));
    let set = parse(target, &text).unwrap_or_default();
    out.profiles.extend(set.profiles.iter().cloned());
    queue.push_back((target.to_owned(), set, depth + 1));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_imports_locations_and_names() {
        let text = "spring.config.import=optional:classpath:extra.yml,file:./conf/more.properties,configtree:/run/x/,vault://s\nspring.config.name=myapp\n";
        let set = read("application.properties", text).unwrap();
        assert_eq!(set.tails, ["extra.yml", "conf/more.properties"]);
        assert!(set.relative.is_empty());
        assert_eq!(set.names, ["myapp"]);
        let sets = [set];
        assert!(is_configuration(
            "/p/src/main/resources/extra.yml",
            sets.iter().map(|l| ("/p/application.properties", l))
        ));
        assert!(is_configuration("/p/conf/more.properties", sets.iter().map(|l| ("/p/application.properties", l))));
        assert!(is_configuration("/p/myapp-dev.yaml", sets.iter().map(|l| ("/p/application.properties", l))));
        assert!(!is_configuration("/p/other.yml", sets.iter().map(|l| ("/p/application.properties", l))));
        assert!(is_configuration("/p/application.yml", std::iter::empty()));
        assert_eq!(read("custom.yml", text), None);
    }

    fn files(map: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: Vec<(String, String)> = map.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        move |p: &str| map.iter().find(|(k, _)| k == p).map(|(_, v)| v.clone())
    }

    fn seed(path: &str, text: &str) -> (String, Locations) {
        (path.to_owned(), read(path, text).unwrap())
    }

    #[test]
    fn reads_profile_declarations_and_refuses_expressions() {
        let text = "spring.profiles.include=common,${X}\nspring.profiles.group.prod[0]=db\nspring.profiles.group.prod[1]=mq\nspring.config.activate.on-profile=a | b\n";
        assert_eq!(read("application.properties", text).unwrap().profiles, ["a", "b", "common", "db", "mq", "prod"]);
        let negated = "spring.config.activate.on-profile=!dev & x\n";
        assert_eq!(read("application.properties", negated), None);
        let specific = "spring.profiles.include=x\n";
        assert_eq!(read("application-dev.properties", specific), None);
        assert_eq!(read("application.properties", "spring.config.import=optional:configtree:/run/s/\n"), None);
    }

    #[test]
    fn relative_imports_resolve_against_the_importer_and_fixed_ones_do_not() {
        let set = read("application.properties", "spring.config.import=optional:core/core.properties,file:./x.yml\n").unwrap();
        assert_eq!(set.relative, ["core/core.properties"]);
        assert_eq!(set.tails, ["x.yml"]);
        let sets = [("/p/demo/application.properties", &set)];
        assert!(is_configuration("/p/demo/core/core.properties", sets.iter().copied()));
        assert!(!is_configuration("/p/other/core/core.properties", sets.iter().copied()));
        assert_eq!(resolve("/p/a/b.yml", "../c/d.yml").as_deref(), Some("/p/c/d.yml"));
        assert_eq!(resolve("/a.yml", "../../x.yml"), None);
    }

    #[test]
    fn profile_variants_of_imports_count_only_for_literal_profiles() {
        let set = read("application.yml", "spring.profiles.include: dev\nspring.config.import: classpath:extra.yml\n").unwrap();
        let sets = [("/p/application.yml", &set)];
        assert!(is_configuration("/p/res/extra-dev.yml", sets.iter().copied()));
        assert!(!is_configuration("/p/res/extra-prod.yml", sets.iter().copied()));
    }

    #[test]
    fn discovery_follows_chains_variants_and_cycles_within_bounds() {
        let app = "/p/application.yml";
        let seeds = [seed(app, "spring.profiles.include: dev\nspring.config.import: optional:core/a.yml\n")];
        let read = files(&[
            ("/p/core/a.yml", "spring.config.import: b.yml,../application.yml\nx: 1\n"),
            ("/p/core/b.yml", "spring.config.import: optional:file:./a.yml\nspring.profiles.include: extra\n"),
            ("/p/core/a-dev.yml", "y: 1\n"),
            ("/p/core/b-extra.yml", "z: 1\n"),
        ]);
        let found = discover(&seeds, &|_| vec!["/p/core/a.yml".to_owned()], &read);
        assert_eq!(found.files, ["/p/core/a-dev.yml", "/p/core/a.yml", "/p/core/b-extra.yml", "/p/core/b.yml"]);
        assert!(found.profiles.contains("extra") && found.profiles.contains("dev"));
    }

    #[test]
    fn discovery_stops_at_the_file_and_depth_bounds() {
        let seeds = [seed("/p/application.yml", "spring.config.import: n0.yml\n")];
        let mut map = Vec::new();
        for i in 0..80 {
            map.push((format!("/p/n{i}.yml"), format!("spring.config.import: n{}.yml\n", i + 1)));
        }
        let read = |p: &str| map.iter().find(|(k, _)| k == p).map(|(_, v)| v.clone());
        let found = discover(&seeds, &|_| Vec::new(), &read);
        assert_eq!(found.files.len(), MAX_DEPTH);
    }

    #[test]
    fn reads_yaml_sequences() {
        let text =
            "spring:\n  config:\n    import:\n      - classpath:a.yml\n      - optional:b.yml\n";
        let set = read("application.yml", text).unwrap();
        assert_eq!(set.tails, ["a.yml"]);
        assert_eq!(set.relative, ["b.yml"]);
    }
}
