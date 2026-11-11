//! What a project declares and reads: keys of `.env` files, Dockerfiles and
//! docker-compose files, and where code (or `${KEY}`) uses them. The original
//! plugin keeps this in IntelliJ's file index; here a project walk builds it
//! and saved files update it.

use crate::{code, docker, ownership, parse};
use openidea_plugin::{Error, Location, Project, Range, text};
use std::collections::BTreeSet;
use std::time::Instant;

/// Env files read in each folder that looks like a project: the walk
/// discovers hidden paths in API 1.3, but normal ignore rules still apply.
pub const ENV_FILES: &[&str] = &[
    ".env",
    ".env.local",
    ".env.example",
    ".env.dist",
    ".env.sample",
    ".env.template",
    ".env.defaults",
    ".env.dev",
    ".env.development",
    ".env.development.local",
    ".env.test",
    ".env.test.local",
    ".env.testing",
    ".env.staging",
    ".env.prod",
    ".env.production",
    ".env.production.local",
    ".env.ci",
    ".env.docker",
    ".flaskenv",
];

/// Files whose folder gets [`ENV_FILES`] looked up, besides the root.
// Ignored, nonstandard env files elsewhere may remain unknown until opened or explicitly referenced.
const MARKERS: &[&str] = &[
    "package.json",
    "composer.json",
    "pyproject.toml",
    "requirements.txt",
    "manage.py",
    "go.mod",
    "Gemfile",
    "Cargo.toml",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "artisan",
    "Dockerfile",
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
];

const CODE: &[&str] = &[
    "php", "py", "js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts", "vue", "svelte", "go", "rb", "rake", "rs", "java", "kt", "kts",
    "scala", "groovy", "cs",
];

/// Paths the walk returns at most.
const MAX_FILES: u32 = 50_000;
pub(crate) const MAX_SOURCE_BYTES: usize = 256 << 10;
const MAX_FACTS: usize = 50_000;

#[derive(Clone, Debug)]
pub struct Declaration {
    pub key: String,
    pub value: String,
    pub location: Location,
    /// From `.env`, a Dockerfile or a compose file; `.env.example` and the
    /// like only count for a key when no primary file has it.
    pub primary: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Usage {
    pub key: String,
    pub location: Location,
}

#[derive(Default)]
pub struct Index {
    /// Built from the whole project, not just the root's env files.
    pub complete: bool,
    pub declarations: Vec<Declaration>,
    pub usages: Vec<Usage>,
    /// Folders of indexed Compose files: the project directory whose `.env` Compose interpolates from.
    pub compose_folders: std::collections::HashSet<String>,
    /// Files some Compose `env_file:` names; their keys reach containers, not interpolation.
    pub env_file_paths: std::collections::HashSet<String>,
    pub loaders: Vec<EnvLoader>,
    pub arg_links: Vec<ArgLink>,
}

/// A Compose service whose `env_file:` names `env_path`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvLoader {
    pub env_path: String,
    pub service: String,
    pub compose_path: String,
    pub line: usize,
}

/// A Dockerfile `ARG` that a service's `build.args` feeds from `${env_key}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgLink {
    pub env_key: String,
    pub arg: String,
    pub service: String,
    pub stage: Option<String>,
    pub dockerfile_path: String,
    pub dockerfile_line: usize,
    pub compose_path: String,
    pub compose_line: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    Env,
    Dockerfile,
    Compose,
    Code,
}

pub fn folder_of(path: &str) -> &str {
    path.rsplit_once(['/', '\\']).map_or("", |(folder, _)| folder)
}

pub fn is_compose_file(path: &str) -> bool {
    source(path) == Some(Source::Compose)
}

pub fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

pub fn is_env_file(path: &str) -> bool {
    let name = file_name(path);
    name == ".env" || name == ".flaskenv" || name.starts_with(".env.") || name.ends_with(".env")
}

fn source(path: &str) -> Option<Source> {
    let name = file_name(path);
    if is_env_file(path) {
        return Some(Source::Env);
    }
    if name == "Dockerfile" || name.starts_with("Dockerfile.") || name.to_ascii_lowercase().ends_with(".dockerfile") {
        return Some(Source::Dockerfile);
    }
    if matches!(name, "docker-compose.yml" | "docker-compose.yaml" | "compose.yml" | "compose.yaml") {
        return Some(Source::Compose);
    }
    let code = name.starts_with("phpunit.xml") || name.rsplit_once('.').is_some_and(|(_, ext)| CODE.contains(&ext));
    code.then_some(Source::Code)
}

pub fn range(text: &str, bytes: std::ops::Range<usize>) -> Range {
    Range::new(text::position(text, bytes.start), text::position(text, bytes.end))
}

pub fn env_declarations(path: &str, text: &str) -> Vec<Declaration> {
    let primary = file_name(path) == ".env";
    let tokens = parse::lex(text);
    parse::properties(text, &tokens)
        .into_iter()
        .map(|p| Declaration {
            key: p.name(text),
            value: p.short_value(),
            location: Location { path: path.to_owned(), range: range(text, p.key.clone()) },
            primary,
        })
        .collect()
}

/// `${KEY}` references of a `.env` file.
pub fn nested_usages(path: &str, text: &str) -> Vec<Usage> {
    parse::lex(text)
        .into_iter()
        .filter(|t| t.kind == parse::Kind::NestedKey)
        .map(|t| Usage { key: text[t.range.clone()].to_owned(), location: Location { path: path.to_owned(), range: range(text, t.range) } })
        .collect()
}

fn code_usages(path: &str, text: &str) -> Vec<Usage> {
    let mut usages = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        // Minified code: one huge line, no `getenv` worth a quadratic scan.
        if body.len() <= 2000 {
            for key in code::keys_in_line(body) {
                usages.push(Usage {
                    key: body[key.clone()].to_owned(),
                    location: Location { path: path.to_owned(), range: range(text, offset + key.start..offset + key.end) },
                });
            }
        }
        offset += line.len();
    }
    usages
}

fn docker_declarations(path: &str, text: &str, entries: Vec<docker::Entry>) -> Vec<Declaration> {
    entries
        .into_iter()
        .map(|(key, value, r)| Declaration {
            key,
            value,
            location: Location { path: path.to_owned(), range: range(text, r) },
            primary: true,
        })
        .collect()
}

fn normalise(path: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts)
}

/// `file` (absolute, or relative to `folder`) with `.` and `..` resolved, or
/// `None` when it ends up outside `root`.
fn confined_path(root: &str, folder: &str, file: &str) -> Option<String> {
    let joined = if file.starts_with('/') { file.to_owned() } else { format!("{folder}/{file}") };
    let root = normalise(root)?;
    let parts = normalise(&joined)?;
    if parts.len() <= root.len() || parts[..root.len()] != root[..] {
        return None;
    }
    Some(format!("/{}", parts.join("/")))
}

impl Index {
    fn add(&mut self, path: &str, text: &str) {
        if text.len() > MAX_SOURCE_BYTES {
            self.complete = false;
            return;
        }
        match source(path) {
            Some(Source::Env) => {
                self.declarations.extend(env_declarations(path, text));
                self.usages.extend(nested_usages(path, text));
            }
            Some(Source::Dockerfile) => self.declarations.extend(docker_declarations(path, text, docker::dockerfile(text))),
            Some(Source::Compose) => {
                self.declarations.extend(docker_declarations(path, text, docker::compose(text)));
                self.usages.extend(docker::compose_interpolations(text).into_iter().map(|i| Usage {
                    key: i.name,
                    location: Location { path: path.to_owned(), range: range(text, i.range) },
                }));
                self.compose_folders.insert(folder_of(path).to_owned());
            }
            Some(Source::Code) => self.usages.extend(code_usages(path, text)),
            None => {}
        }
        self.bound_facts();
    }

    fn bound_facts(&mut self) {
        if self.declarations.len() > MAX_FACTS || self.usages.len() > MAX_FACTS {
            self.complete = false;
            self.declarations.truncate(MAX_FACTS);
            self.usages.truncate(MAX_FACTS);
        }
    }

    /// Reads the files a compose file names in `env_file:` and declares their
    /// keys, unless the index already has that file. Missing or unreadable
    /// explicitly referenced files leave discovery incomplete.
    pub fn add_compose_env_files(&mut self, project: &Project, root: &str, compose_path: &str, text: &str) {
        self.compose_env_files_before(project, root, compose_path, text, Instant::now() + std::time::Duration::from_secs(1));
    }

    fn compose_env_files_before(&mut self, project: &Project, root: &str, compose_path: &str, text: &str, deadline: Instant) {
        if source(compose_path) != Some(Source::Compose) {
            return;
        }
        // Resolve env_file aliases/merges with explicit parser budgets. The
        // completeness bit also checks that every expanded environment key has
        // a physical declaration covered by the source-range model.
        let folder = compose_path.rsplit_once(['/', '\\']).map_or("", |(folder, _)| folder);
        self.add_ownership(project, root, compose_path, text);
        let (files, graph_complete, optional) = docker::compose_env_file_graph(text);
        if !graph_complete { self.complete=false; }
        if files.len() > 128 {
            self.complete = false;
        }
        for file in files.into_iter().take(128) {
            if Instant::now() >= deadline {
                self.complete = false;
                break;
            }
            if file.contains('$') || file.len() > 4096 {
                self.complete = false;
                continue;
            }
            let Some(path) = confined_path(root, folder, &file) else {
                self.complete = false;
                continue;
            };
            self.env_file_paths.insert(path.clone());
            if self.declarations.iter().any(|d| d.location.path == path) {
                continue;
            }
            match project.read_file(&path) {
                Ok(env) if env.len() <= MAX_SOURCE_BYTES => {
                    self.declarations.extend(env_declarations(&path, &env).into_iter().map(|d| Declaration { primary: true, ..d }))
                }
                Err(openidea_plugin::Error::NotFound(_)) if optional.contains(&file) => {},
                _ => self.complete = false,
            }
            self.bound_facts();
        }
    }

    fn add_ownership(&mut self, project: &Project, root: &str, compose_path: &str, text: &str) {
        let folder = folder_of(compose_path);
        for service in ownership::compose_services(text) {
            for (file, line) in &service.env_files {
                let Some(env_path) = confined_path(root, folder, file).filter(|_| !file.contains('$')) else { continue };
                let loader = EnvLoader { env_path, service: service.name.clone(), compose_path: compose_path.to_owned(), line: *line };
                if !self.loaders.contains(&loader) {
                    self.loaders.push(loader);
                }
            }
            if service.build_args.is_empty() {
                continue;
            }
            let context = service.context.as_deref().unwrap_or(".");
            let dockerfile = service.dockerfile.as_deref().unwrap_or("Dockerfile");
            if [context, dockerfile].iter().any(|p| p.contains('$') || p.contains("://") || p.starts_with("git@")) {
                continue;
            }
            let relative = if dockerfile.starts_with('/') { dockerfile.to_owned() } else { format!("{context}/{dockerfile}") };
            let Some(dockerfile_path) = confined_path(root, folder, &relative) else { continue };
            let Ok(dockerfile_text) = project.read_file(&dockerfile_path) else { continue };
            if dockerfile_text.len() > MAX_SOURCE_BYTES {
                continue;
            }
            let args = ownership::dockerfile_args(&dockerfile_text);
            for (arg, env_key, compose_line) in &service.build_args {
                for declared in args.iter().filter(|a| &a.name == arg && a.stage.is_some()) {
                    let location = Location { path: dockerfile_path.clone(), range: range(&dockerfile_text, declared.range.clone()) };
                    let link = ArgLink {
                        env_key: env_key.clone(),
                        arg: arg.clone(),
                        service: service.name.clone(),
                        stage: declared.stage.clone(),
                        dockerfile_path: dockerfile_path.clone(),
                        dockerfile_line: location.range.start.line as usize + 1,
                        compose_path: compose_path.to_owned(),
                        compose_line: *compose_line,
                    };
                    let usage = Usage { key: env_key.clone(), location };
                    if !self.usages.contains(&usage) {
                        self.usages.push(usage);
                    }
                    if !self.arg_links.contains(&link) {
                        self.arg_links.push(link);
                    }
                }
            }
        }
        self.bound_facts();
    }

    /// Replaces what `path` declares and uses with `text` (`None`: it is gone).
    pub fn update(&mut self, path: &str, text: Option<&str>) {
        self.declarations.retain(|d| d.location.path != path);
        self.usages.retain(|u| u.location.path != path);
        self.loaders.retain(|l| l.compose_path != path);
        self.arg_links.retain(|l| l.compose_path != path);
        if let Some(text) = text {
            self.add(path, text);
        }
    }

    pub fn indexes(path: &str) -> bool {
        source(path).is_some()
    }

    /// The env files at the root only: quick enough for an editor request.
    pub fn quick(project: &Project, root: &str) -> Self {
        let mut index = Self::default();
        for name in ENV_FILES {
            let path = format!("{}/{name}", root.trim_end_matches(['/', '\\']));
            if let Ok(text) = project.read_file(&path) {
                index.add(&path, &text);
            }
        }
        index
    }

    /// Every file that declares or reads keys, declarations first, until `deadline`.
    // ponytail: a huge project is cut at the deadline (usages first); persist the index if that bites.
    pub fn full(project: &Project, root: &str, deadline: Instant) -> Self {
        if Instant::now() >= deadline {
            return Self::default();
        }
        let mut files = match project.find_files("**/*", MAX_FILES + 1) {
            Ok(files) => files,
            Err(_) => return Self::default(),
        };
        let discovery_complete = files.len() <= MAX_FILES as usize;
        files.truncate(MAX_FILES as usize);
        let discovered: BTreeSet<_> = files.iter().cloned().collect();
        // Inventory obeys ignore rules. A configured ignore file means an
        // unlisted nonstandard env source may exist, so absence is unknown.
        // These probes run only during the warm project scan, never diagnostics.
        let mut ignore_unknown = files.iter().any(|file| matches!(file_name(file), ".gitignore" | ".ignore"));
        for name in [".gitignore", ".ignore", ".git/info/exclude"] {
            if Instant::now() >= deadline {
                return Self::default();
            }
            let path = format!("{}/{name}", root.trim_end_matches(['/', '\\']));
            match project.read_file(&path) {
                Ok(text) => {
                    if text.len() > MAX_SOURCE_BYTES
                        || text.lines().any(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
                    {
                        ignore_unknown = true;
                    }
                }
                Err(Error::NotFound(_)) => {}
                Err(_) => ignore_unknown = true,
            }
        }
        let mut folders = BTreeSet::from([root.trim_end_matches(['/', '\\']).to_owned()]);
        for file in &files {
            if MARKERS.contains(&file_name(file)) {
                if let Some((folder, _)) = file.rsplit_once(['/', '\\']) {
                    folders.insert(folder.to_owned());
                }
            }
        }
        // Known root/marker env filenames are optional probes; disappearance of
        // a file actually discovered by the walk is an incomplete/racing scan.
        let mut paths: Vec<(Source, String)> = folders
            .iter()
            .flat_map(|folder| ENV_FILES.iter().map(move |name| (Source::Env, format!("{folder}/{name}"))))
            .chain(files.into_iter().filter_map(|f| Some((source(&f)?, f))))
            .collect();
        paths.sort();
        paths.dedup();
        let mut index = Self { complete: discovery_complete && !ignore_unknown, ..Self::default() };
        for (_, path) in paths {
            if Instant::now() >= deadline {
                index.complete = false;
                break;
            }
            match project.read_file(&path) {
                Ok(text) => {
                    if Instant::now() >= deadline {
                        index.complete = false;
                        break;
                    }
                    index.add(&path, &text);
                    if text.len() <= MAX_SOURCE_BYTES {
                        index.compose_env_files_before(project, root, &path, &text, deadline);
                    }
                }
                Err(Error::NotFound(_)) if !discovered.contains(&path) => {}
                Err(_) => index.complete = false,
            }
            if index.declarations.len() >= MAX_FACTS || index.usages.len() >= MAX_FACTS {
                index.complete = false;
                break;
            }
        }
        if Instant::now() >= deadline {
            index.complete = false;
        }
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::Position;
    use openidea_plugin::mock::MockHost;
    use std::time::Duration;

    #[test]
    fn indexes_the_project() {
        let host = MockHost::install();
        host.set_project_root(Some("/p"));
        host.add_file(".env", "A=1\nB=\"${A}\"\n");
        host.add_file("api/composer.json", "{}");
        host.add_file("api/.env.example", "C=3\n");
        host.add_file("Dockerfile", "ENV D=4\n");
        host.add_file(
            "deploy/docker-compose.yml",
            "services:\n  x:\n    env_file: [\"./conf/vars\", missing.env]\n    environment:\n      - E=5\n",
        );
        host.add_file("deploy/conf/vars", "F=6\n");
        host.add_file("api/index.php", "<?php\necho getenv('A') . $_ENV['C'];\n");
        host.add_file("README.md", "getenv('NOPE')");
        let project = host.invocation().project().unwrap();

        let index = Index::full(&project, "/p", Instant::now() + Duration::from_secs(5));
        let mut keys: Vec<_> = index.declarations.iter().map(|d| (d.key.as_str(), d.primary)).collect();
        keys.sort();
        assert_eq!(keys, [("A", true), ("B", true), ("C", false), ("D", true), ("E", true), ("F", true)]);
        let usages: Vec<_> = index.usages.iter().map(|u| (u.key.as_str(), file_name(&u.location.path))).collect();
        assert_eq!(usages, [("A", ".env"), ("A", "index.php"), ("C", "index.php")]);
        assert_eq!(index.usages[1].location.range, Range::new(Position::new(1, 13), Position::new(1, 14)));

        let quick = Index::quick(&project, "/p");
        assert_eq!(quick.declarations.len(), 2, "only the root's .env");
    }

    #[test]
    fn completeness_requires_discovery_budget_and_readable_supported_sources() {
        let host = MockHost::install();
        host.set_project_root(Some("/p"));
        let project = host.invocation().project().unwrap();
        assert!(
            Index::full(&project, "/p", Instant::now() + Duration::from_secs(5)).complete,
            "missing optional conventional files are normal"
        );
        assert!(!Index::full(&project, "/p", Instant::now() - Duration::from_secs(1)).complete);
        host.add_file(".env", &"#".repeat(MAX_SOURCE_BYTES + 1));
        assert!(!Index::full(&project, "/p", Instant::now() + Duration::from_secs(5)).complete);
        let _denied = host.with_capabilities(&[]);
        assert!(!Index::full(&project, "/p", Instant::now() + Duration::from_secs(5)).complete);
    }

    #[test]
    fn ignored_inventory_does_not_prove_absence_but_keeps_known_declarations() {
        let host = MockHost::install();
        host.set_project_root(Some("/p"));
        host.add_file(".env", "A=1\n");
        host.add_file(".gitignore", "*.env\n");
        let project = host.invocation().project().unwrap();
        let index = Index::full(&project, "/p", Instant::now() + Duration::from_secs(5));
        assert!(!index.complete);
        assert!(index.declarations.iter().any(|d| d.key == "A"));
    }

    #[test]
    fn truncated_inventory_never_proves_absence() {
        let host = MockHost::install();
        host.set_project_root(Some("/p"));
        for n in 0..=MAX_FILES {
            host.add_file(&format!("unrelated/{n}.txt"), "");
        }
        let project = host.invocation().project().unwrap();
        let index = Index::full(&project, "/p", Instant::now() + Duration::from_secs(10));
        assert!(!index.complete);
    }

    #[test]
    fn unresolved_compose_inputs_leave_discovery_unknown() {
        for name in ["missing.env", "${CONFIG}", "../../outside.env", "*shared"] {
            let index = compose_index(name);
            assert!(!index.complete, "{name}");
            assert!(index.declarations.iter().any(|d| d.key == "A"), "retain known declarations");
        }
        let index = compose_index("../.env");
        assert!(index.complete, "resolvable explicitly named files are complete");
    }

    #[test]
    fn env_file_paths_are_confined_to_the_root() {
        assert_eq!(confined_path("/p", "/p/deploy", "../.env").as_deref(), Some("/p/.env"));
        assert_eq!(confined_path("/p/", "/p/deploy", "./a/../x.env").as_deref(), Some("/p/deploy/x.env"));
        assert_eq!(confined_path("/p", "/p/deploy", "/p/y.env").as_deref(), Some("/p/y.env"));
        assert_eq!(confined_path("/p", "/p/deploy", "../../outside.env"), None);
        assert_eq!(confined_path("/p", "/p/deploy", "/etc/passwd"), None);
        assert_eq!(confined_path("/p", "/p/deploy", "../../../x"), None);
    }

    fn compose_index(env_file: &str) -> Index {
        let host = MockHost::install();
        host.set_project_root(Some("/p"));
        host.add_file(".env", "A=1\n");
        host.add_file("/outside.env", "OUT=1\n");
        host.add_file("deploy/docker-compose.yml", &format!("services:\n  x:\n    env_file: {env_file}\n"));
        let project = host.invocation().project().unwrap();
        Index::full(&project, "/p", Instant::now() + Duration::from_secs(5))
    }

    #[test]
    fn env_file_parent_dir_path_is_normalised_and_deduped() {
        let index = compose_index("../.env");
        let found: Vec<_> = index.declarations.iter().map(|d| (d.key.as_str(), d.location.path.as_str())).collect();
        assert_eq!(found, [("A", "/p/.env")]);
    }

    #[test]
    fn env_file_escaping_the_root_is_ignored() {
        for path in ["../../outside.env", "/outside.env", "/p/../outside.env"] {
            let index = compose_index(path);
            assert!(index.declarations.iter().all(|d| d.key != "OUT"), "{path}");
        }
    }
}

#[cfg(test)]
mod optional_input_tests {
    use super::*;
    use std::time::Duration;
    use openidea_plugin::mock::MockHost;
    #[test]
    fn resolved_alias_and_missing_optional_files_are_complete_source_discovery() {
        let h=MockHost::install();h.set_project_root(Some("/p"));h.add_file(".env","A=1\n");
        h.add_file("compose.yml","x-file: &file {path: missing.env, required: false}\nx-service: &service {env_file: [*file]}\nservices: {app: {<<: *service}}\n");
        let project=h.invocation().project().unwrap();let index=Index::full(&project,"/p",Instant::now()+Duration::from_secs(5));
        assert!(index.complete);assert!(index.declarations.iter().any(|d|d.key=="A"));
    }
}
