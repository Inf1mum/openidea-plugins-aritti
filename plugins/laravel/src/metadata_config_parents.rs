//! Parent configuration links from declared installed service providers, never application execution.
use super::*;
use crate::{
    eloquent_members::metadata_inference as inference, package_integrations::ComposerRoot, php,
};
use openidea_plugin::providers::{Document, LineMarker};
const BYTES: usize = 128 * 1024;
#[derive(Clone, Debug)]
pub(super) struct Pending {
    pub provider: String,
    pub provider_path: String,
    pub key: String,
    pub defaults: String,
    pub adapter_proof: Vec<(String, String)>,
}
#[derive(Clone, Debug)]
pub(super) struct Parent {
    pub key: String,
    pub target: crate::index::Entry,
    pub proof: Vec<(String, String)>,
}
fn owned(project: &Project, path: &str) -> bool {
    crate::owns_current_path(project, path)
}
fn read(project: &Project, path: &str, limit: usize) -> Option<String> {
    if !owned(project, path) {
        return None;
    }
    let root = project.root().ok()?;
    let relative = path.strip_prefix(&format!("{root}/"))?;
    let mut prefix = String::new();
    for part in relative.split('/') {
        if part.is_empty() || matches!(part, "." | "..") {
            return None;
        }
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        if project
            .stat(&prefix)
            .is_ok_and(|stat| stat.kind == openidea_plugin::FileKind::Symlink)
        {
            return None;
        }
    }
    let stat = project.stat(path).ok()?;
    if stat.kind != openidea_plugin::FileKind::File || stat.size > limit as u64 {
        return None;
    }
    let source = project.read_file(path).ok()?;
    (source.len() <= limit && owned(project, path)).then_some(source)
}
fn read_recorded(project:&Project,metadata:&mut Metadata,path:&str,limit:usize)->Option<String>{
    let root=project.root().ok()?;let relative=path.strip_prefix(&format!("{root}/"))?;
    let source=read(project,path,limit);
    if let Some(source)=&source { evidence::record(&mut metadata.source_evidence,relative,source,&mut metadata.source_evidence_incomplete); }
    else if matches!(project.stat(path),Err(openidea_plugin::Error::NotFound(_))) { evidence::missing(&mut metadata.source_evidence,relative,&mut metadata.source_evidence_incomplete); }
    else { metadata.source_evidence_incomplete=true; }
    source
}
fn find_recorded(project:&Project,metadata:&mut Metadata,pattern:&str,limit:u32)->Vec<String>{
    match project.find_files(pattern,limit) {
        Ok(files)=>{let mut owned:Vec<_>=files.iter().filter(|path|owned(project,path)).cloned().collect();owned.sort();owned.dedup();evidence::merge(&mut metadata.source_queries,&mut metadata.source_evidence_incomplete,[((pattern.into(),limit),Some(owned))]);files},
        Err(_)=>{metadata.source_queries.insert((pattern.into(),limit),None);metadata.source_evidence_incomplete=true;Vec::new()}
    }
}

fn normal(path: &str, root: &str) -> Option<String> {
    if !path.starts_with('/') && !path.starts_with(&format!("{root}/")) {
        return None;
    }
    if path.contains('\\') || path.chars().any(char::is_control) {
        return None;
    }
    let absolute = path.starts_with('/');
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    let path = format!("{}{}", if absolute { "/" } else { "" }, parts.join("/"));
    path.strip_prefix(&format!("{root}/"))
        .filter(|relative| !relative.is_empty())
        .map(str::to_owned)
}
/// Compile-time magic paths and literal dirname calls only. Relative CWD is not inferred.
fn expression(
    source: &str,
    masked: &str,
    range: Range<usize>,
    origin: &str,
    root: &str,
    depth: usize,
) -> Option<String> {
    if depth > 8 {
        return None;
    }
    let range = php::trimmed(masked, range);
    let bytes = source.as_bytes();
    let mut pieces = Vec::new();
    let mut begin = range.start;
    let mut at = range.start;
    while at < range.end {
        if matches!(bytes[at], b'\'' | b'"') {
            at = php::string_end(bytes, at);
            continue;
        }
        if bytes[at] == b'(' {
            at = php::closing(masked, at)? + 1;
            continue;
        }
        if bytes[at] == b'.' {
            pieces.push(begin..at);
            begin = at + 1;
        }
        at += 1;
    }
    if !pieces.is_empty() {
        pieces.push(begin..range.end);
        if pieces.len() > 8 {
            return None;
        }
        let mut out = String::new();
        for piece in pieces {
            out.push_str(&expression(source, masked, piece, origin, root, depth + 1)?);
            if out.len() > 4096 {
                return None;
            }
        }
        return Some(out);
    }
    if let Some((value, offset)) = php::literal(source, range.clone()) {
        return inference::decode_literal(value, source.as_bytes()[offset - 1]);
    }
    let text = masked[range.clone()].trim();
    if text.eq_ignore_ascii_case("__DIR__") {
        return Some(format!("{root}/{}", origin.rsplit_once('/')?.0));
    }
    if text.eq_ignore_ascii_case("__FILE__") {
        return Some(format!("{root}/{origin}"));
    }
    let open = range.start + masked[range.clone()].find('(')?;
    let name = masked[range.start..open].trim();
    if !matches!(name.to_ascii_lowercase().as_str(), "dirname" | "\\dirname")
        || !inference::global_helper(source, open, "dirname")
        || php::closing(masked, open)? + 1 != range.end
    {
        return None;
    }
    let args = php::arguments(source, open);
    if args.is_empty() || args.len() > 2 {
        return None;
    }
    let mut path = expression(source, masked, args[0].clone(), origin, root, depth + 1)?;
    let count = if let Some(count) = args.get(1) {
        source[count.clone()]
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|count| *count > 0 && *count <= 16)?
    } else {
        1
    };
    for _ in 0..count {
        path = path.trim_end_matches('/').rsplit_once('/')?.0.to_owned();
    }
    Some(path)
}
fn config_return(source: &str) -> Option<usize> {
    inference::config_return(source)
}
pub(super) fn prepare(
    project: &Project,
    metadata: &mut Metadata,
    sources: &BTreeMap<String, String>,
    roots: &[ComposerRoot],
) {
    metadata.config_pending.clear();
    metadata.config_parents.clear();
    if sources.is_empty() {
        return;
    }
    let Ok(root) = project.root() else {
        return;
    };
    let mut candidates = 0usize;
    let mut tools_proof = None;
    for (origin, source) in sources {
        if source.len() > BYTES || candidates >= 64 {
            continue;
        }
        let path = format!("{root}/{origin}");
        if !owned(project, &path) {
            continue;
        }
        let Some(fact) = inference::class_fact(source, &path)
            .filter(|fact| fact.class_kind == "class" && !fact.abstract_type)
        else {
            continue;
        };
        let installed = roots.iter().any(|mapping| {
            mapping.package != "project"
                && fact
                    .full
                    .strip_prefix(&mapping.namespace)
                    .is_some_and(|tail| {
                        origin == &format!("{}/{}.php", mapping.directory, tail.replace('\\', "/"))
                    })
        });
        if !installed {
            continue;
        }
        let Some(code) = crate::package_facts::code(source) else {
            continue;
        };
        let masked = php::mask_comments(source);
        let mut parsed = Vec::new();
        for (offset, _) in code.to_ascii_lowercase().match_indices("mergeconfigfrom") {
            let before = code[..offset].trim_end();
            let Some(before) = before.strip_suffix("->").map(str::trim_end) else {
                continue;
            };
            let Some(prefix) = before.strip_suffix("$this") else {
                continue;
            };
            if prefix
                .as_bytes()
                .last()
                .is_some_and(|byte| php::is_identifier(*byte) || *byte == b'$')
            {
                continue;
            }
            let tail = code[offset + 15..].trim_start();
            if !tail.starts_with('(') {
                continue;
            }
            let open = code.len() - tail.len();
            if inference::call_identity(source, open)
                .and_then(|identity| identity.class)
                .as_deref()
                != Some(fact.full.as_str())
            {
                continue;
            }
            let args = php::arguments(source, open);
            if args.len() != 2 {
                continue;
            }
            let Some((key, key_offset)) = php::literal(source, args[1].clone()) else {
                parsed.push(Pending {
                    provider: fact.full.clone(),
                    provider_path: path.clone(),
                    key: String::new(),
                    defaults: String::new(),
                    adapter_proof: Vec::new(),
                });
                candidates += 1;
                continue;
            };
            let Some(key) = inference::decode_literal(key, source.as_bytes()[key_offset - 1])
            else {
                continue;
            };
            if key.is_empty()
                || key.len() > 256
                || !key.split('.').all(|part| {
                    !part.is_empty()
                        && part
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
                })
            {
                continue;
            }
            metadata.domain_roots.insert("config".into());
            let depth = code[..offset]
                .bytes()
                .fold(0isize, |depth, byte| match byte {
                    b'{' => depth + 1,
                    b'}' => depth - 1,
                    _ => depth,
                });
            let statement_prefix = prefix.rsplit([';', '{', '}']).next().unwrap_or("").trim();
            let unconditional = depth == 2 && statement_prefix.is_empty();
            let defaults = if unconditional {
                expression(source, &masked, args[0].clone(), origin, &root, 0)
                    .and_then(|path| normal(&path, &root))
                    .unwrap_or_default()
            } else {
                metadata.issues.push(format!("Conditional configuration defaults for {key} are not authoritative; defaults withheld"));
                String::new()
            };
            if !defaults.is_empty() {
                metadata.inputs.insert(defaults.clone());
            }
            metadata.inputs.insert(origin.clone());
            parsed.push(Pending {
                provider: fact.full.clone(),
                provider_path: path.clone(),
                key,
                defaults,
                adapter_proof: Vec::new(),
            });
            candidates += 1;
            if candidates >= 64 {
                break;
            }
        }
        let tools = package_tools(
            project,
            metadata,
            roots,
            origin,
            source,
            &fact,
            &mut tools_proof,
        );
        if tools.is_empty()
            && fact.base.as_deref() == Some("Spatie\\LaravelPackageTools\\PackageServiceProvider")
            && code.to_ascii_lowercase().contains("hasconfigfile")
        {
            // Unsupported package options may select any runtime config root.
            // Do not turn refusal to infer into evidence that keys are absent.
            parsed.push(Pending {
                provider: fact.full.clone(),
                provider_path: path.clone(),
                key: String::new(),
                defaults: String::new(),
                adapter_proof: vec![(path.clone(), crate::artisan_metadata::hash(source))],
            });
        }
        parsed.extend(tools);
        parsed.truncate(64usize.saturating_sub(metadata.config_pending.len()));
        if !parsed.is_empty() {
            inference::cache_class(metadata, source, &path);
            metadata.config_pending.extend(parsed);
        }
    }
    if candidates >= 64 {
        metadata.issues.push("Parent configuration discovery reached its 64-declaration budget; remaining roots withheld".into());
    }
}
fn provider_proof(
    metadata: &Metadata,
    full: &str,
    active: &mut BTreeSet<String>,
) -> Option<(bool, Vec<(String, String)>)> {
    if full.eq_ignore_ascii_case("Illuminate\\Support\\ServiceProvider") {
        return Some((true, Vec::new()));
    }
    let key = full.to_ascii_lowercase();
    if active.len() >= 32
        || !active.insert(key.clone())
        || metadata.ambiguous_classes.contains(&key)
    {
        return None;
    }
    let fact = metadata.class_facts.get(&key)?;
    if fact.methods.contains("mergeconfigfrom")
        || fact.adaptations
        || !matches!(fact.class_kind.as_str(), "class" | "trait")
    {
        return None;
    }
    let mut proof = vec![(fact.path.clone(), fact.source_hash.clone())];
    for used in &fact.traits {
        let (_, source) = provider_proof(metadata, used, &mut active.clone())?;
        proof.extend(source);
    }
    let framework = if let Some(base) = &fact.base {
        let (framework, source) = provider_proof(metadata, base, &mut active.clone())?;
        proof.extend(source);
        framework
    } else {
        false
    };
    Some((framework, proof))
}
fn pending_proof(metadata: &Metadata, pending: &Pending) -> Option<(bool, Vec<(String, String)>)> {
    if pending.adapter_proof.is_empty() {
        return provider_proof(metadata, &pending.provider, &mut BTreeSet::new());
    }
    let fact = metadata
        .class_facts
        .get(&pending.provider.to_ascii_lowercase())?;
    if fact.path != pending.provider_path
        || metadata
            .ambiguous_classes
            .contains(&pending.provider.to_ascii_lowercase())
    {
        return None;
    }
    let mut proof = pending.adapter_proof.clone();
    proof.push((fact.path.clone(), fact.source_hash.clone()));
    Some((true, proof))
}
fn compact(source: &str) -> String {
    php::mask_comments(source)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}
/// Adapter for the installed PackageTools literal configurePackage API. The
/// library's path/name/merge semantics are checked from its owned source first.
fn package_tools(
    project: &Project,
    metadata: &mut Metadata,
    roots: &[ComposerRoot],
    origin: &str,
    source: &str,
    fact: &ClassFact,
    cached_proof: &mut Option<Option<Vec<(String, String)>>>,
) -> Vec<Pending> {
    const BASE: &str = "Spatie\\LaravelPackageTools\\PackageServiceProvider";
    if fact.base.as_deref() != Some(BASE)
        || fact.adaptations
        || !fact.traits.is_empty()
        || [
            "register",
            "newpackage",
            "getpackagebasedir",
            "registerpackageconfigs",
            "mergeconfigfrom",
            "normalizeconfigpath",
            "normalizeconfigkey",
        ]
        .iter()
        .any(|name| fact.methods.contains(*name))
    {
        return vec![];
    }
    let Some(mapping) = roots.iter().find(|r| {
        r.package == "spatie/laravel-package-tools"
            && r.namespace == "Spatie\\LaravelPackageTools\\"
    }) else {
        return vec![];
    };
    let Ok(root) = project.root() else {
        return vec![];
    };
    let contracts: [(&str, &[&str]); 4] = [
        (
            "PackageServiceProvider.php",
            &[
                "extendsServiceProvider",
                "$this->package=$this->newPackage();",
                "$this->package->setBasePath($this->getPackageBaseDir());",
                "$this->configurePackage($this->package);",
                "$this->registerPackageConfigs();",
                "returnnewPackage();",
                "newReflectionClass(get_class($this))",
                "dirname($reflector->getFileName())",
                "DIRECTORY_SEPARATOR.'Providers'",
            ],
        ),
        (
            "Package.php",
            &[
                "useHasConfigs;",
                "Str::after($this->name,'laravel-')",
                "$this->basePath.DIRECTORY_SEPARATOR.ltrim($directory,DIRECTORY_SEPARATOR)",
            ],
        ),
        (
            "Concerns/Package/HasConfigs.php",
            &[
                "functionhasConfigFile($configFileName=null)",
                "$configFileName??=$this->shortName();",
                "$this->configFileNames=$configFileName;",
            ],
        ),
        (
            "Concerns/PackageServiceProvider/ProcessConfigs.php",
            &[
                "foreach($this->package->configFileNamesas$configFileName)",
                "$this->package->basePath(\"/../config/{$configFilePath}.php\")",
                "$this->mergeConfigFrom($vendorConfig,$normalizedConfigKey);",
                "str_replace(['/',",
                "'.',$configFileName)",
            ],
        ),
    ];
    let proof = if let Some(proof) = cached_proof {
        let Some(proof) = proof else {
            return vec![];
        };
        proof.clone()
    } else {
        let mut proof = Vec::new();
        *cached_proof = Some(None);
        for (relative, needles) in contracts {
            let relative = format!("{}/{relative}", mapping.directory);
            let path = format!("{root}/{relative}");
            let Some(library) = read_recorded(project, metadata, &path, BYTES) else {
                return vec![];
            };
            let expected = format!(
                "Spatie\\LaravelPackageTools\\{}",
                relative
                    .strip_prefix(&format!("{}/", mapping.directory))
                    .unwrap()
                    .trim_end_matches(".php")
                    .replace('/', "\\")
            );
            if inference::class_fact(&library, &path).is_none_or(|fact| fact.full != expected) {
                return vec![];
            }
            let code = compact(&library);
            if !needles.iter().all(|needle| code.contains(needle)) {
                return vec![];
            }
            metadata.inputs.insert(relative);
            proof.push((path, crate::artisan_metadata::hash(&library)));
        }
        *cached_proof = Some(Some(proof.clone()));
        proof
    };
    let code = crate::package_facts::code(source).unwrap_or_default();
    let lower = code.to_ascii_lowercase();
    let Some(offset) = lower.find("configurepackage") else {
        return vec![];
    };
    if !code[..offset].trim_end().ends_with("function") {
        return vec![];
    }
    let Some(open) = code[offset..].find('(').map(|n| n + offset) else {
        return vec![];
    };
    let Some(close) = php::closing(&code, open) else {
        return vec![];
    };
    let parameter = source[open + 1..close].trim();
    let Some((ty, variable)) = parameter.split_once('$') else {
        return vec![];
    };
    let variable = format!("${}", variable.trim());
    if !variable[1..].bytes().all(php::is_identifier)
        || inference::class_identity(source, ty.trim()).as_deref()
            != Some("Spatie\\LaravelPackageTools\\Package")
    {
        return vec![];
    }
    let Some(body) = code[close..].find('{').map(|n| n + close) else {
        return vec![];
    };
    let Some(end) = php::closing(&code, body) else {
        return vec![];
    };
    let body_text = &code[body + 1..end];
    // A single straight-line package chain. Never infer conditional/custom calls.
    let mut at = 0usize;
    let mut name = None;
    let mut config = None;
    while at < body_text.len() {
        let rest = body_text[at..].trim_start();
        at = body_text.len() - rest.len();
        if rest.is_empty() {
            break;
        }
        if !rest.starts_with(&variable) {
            return vec![];
        }
        at += variable.len();
        loop {
            let rest = body_text[at..].trim_start();
            at = body_text.len() - rest.len();
            if rest.starts_with(';') {
                at += 1;
                break;
            }
            let Some(rest) = rest.strip_prefix("->") else {
                return vec![];
            };
            at += 2;
            let method_len = rest.bytes().take_while(|b| php::is_identifier(*b)).count();
            if method_len == 0 {
                return vec![];
            }
            let method = &rest[..method_len];
            if ![
                "name",
                "hasConfigFile",
                "hasAssets",
                "hasBladeComponents",
                "hasCommands",
                "hasInertia",
                "hasInstallCommand",
                "hasMigrations",
                "hasRoutes",
                "hasServiceProviders",
                "hasTranslations",
                "hasViewComposers",
                "hasViews",
                "hasViewSharedData",
            ]
            .contains(&method)
            {
                return vec![];
            }
            at += method_len;
            let rest = body_text[at..].trim_start();
            at = body_text.len() - rest.len();
            if !rest.starts_with('(') {
                return vec![];
            }
            let absolute = body + 1 + at;
            let Some(call_end) = php::closing(&code, absolute) else {
                return vec![];
            };
            let args = php::arguments(source, absolute);
            if method == "name" {
                if name.is_some() || args.len() != 1 {
                    return vec![];
                }
                name = php::literal(source, args[0].clone())
                    .and_then(|(v, o)| inference::decode_literal(v, source.as_bytes()[o - 1]));
            }
            if method == "hasConfigFile" {
                if config.is_some() || args.len() > 1 {
                    return vec![];
                }
                let use_short_name =
                    args.is_empty() || source[args[0].clone()].trim().eq_ignore_ascii_case("null");
                let names = if use_short_name {
                    Vec::new()
                } else if let Some((v, o)) = php::literal(source, args[0].clone()) {
                    let Some(v) = inference::decode_literal(v, source.as_bytes()[o - 1]) else {
                        return vec![];
                    };
                    vec![v]
                } else if let Some(array) = php::array_open(&code, args[0].clone()) {
                    let mut names = Vec::new();
                    for item in php::items(source, array).into_iter().take(17) {
                        if item.key.is_some() {
                            return vec![];
                        }
                        let Some((v, o)) = php::literal(source, item.value) else {
                            return vec![];
                        };
                        let Some(v) = inference::decode_literal(v, source.as_bytes()[o - 1]) else {
                            return vec![];
                        };
                        names.push(v);
                    }
                    if names.len() > 16 {
                        return vec![];
                    }
                    names
                } else {
                    return vec![];
                };
                config = Some((names, use_short_name));
            }
            at = call_end + 1 - (body + 1);
        }
    }
    let Some((mut names, use_short_name)) = config else {
        return vec![];
    };
    if use_short_name {
        let Some(name) = name else {
            return vec![];
        };
        names.push(
            name.split_once("laravel-")
                .map_or(name.as_str(), |(_, tail)| tail)
                .to_owned(),
        );
    }
    let mut base = origin.rsplit_once('/').map(|p| p.0).unwrap_or("");
    if base.ends_with("/Providers") {
        base = base.rsplit_once('/').map(|p| p.0).unwrap_or("");
    }
    names
        .into_iter()
        .filter_map(|name| {
            if name.is_empty()
                || name.len() > 256
                || !name.split(['/', '\\', '.']).all(|part| {
                    !part.is_empty()
                        && part
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                })
            {
                return None;
            }
            let defaults = normal(
                &format!("{root}/{base}/../config/{}.php", name.replace('\\', "/")),
                &root,
            )?;
            metadata.inputs.insert(defaults.clone());
            metadata.inputs.insert(origin.into());
            metadata.domain_roots.insert("config".into());
            Some(Pending {
                provider: fact.full.clone(),
                provider_path: format!("{root}/{origin}"),
                key: name.replace(['/', '\\'], "."),
                defaults,
                adapter_proof: proof.clone(),
            })
        })
        .collect()
}

pub(super) fn finish(project: &Project, metadata: &mut Metadata) {
    metadata.config_parents.clear();
    if metadata.config_pending.is_empty() {
        return;
    }
    let Ok(root) = project.root() else {
        return;
    };
    let mut authority = Vec::new();
    for path in ["composer.json", "vendor/composer/installed.json"] {
        let full = format!("{root}/{path}");
        let Some(source) = read_recorded(project, metadata, &full, 2 * 1024 * 1024) else {
            return;
        };
        metadata.inputs.insert(path.into());
        authority.push((full, crate::artisan_metadata::hash(&source)));
    }
    let mut configs = BTreeMap::new();
    for pattern in ["config/*.php", "config/**/*.php"] {
        for path in find_recorded(project,metadata,pattern,129) {
            if configs.len() >= 128 {
                break;
            }
            if let Some(relative) = path
                .strip_prefix(&format!("{root}/config/"))
                .and_then(|path| path.strip_suffix(".php"))
            {
                configs.insert(path.clone(), relative.replace('/', "."));
            }
        }
    }
    let mut read_bytes = 0;
    let mut links = 0;
    for pending in metadata.config_pending.clone() {
        // Empty defaults represent a deliberately unresolved runtime path, not
        // a request to consume the project root as a source file.
        if pending.defaults.is_empty(){continue;}
        let Some((true, mut proof)) = pending_proof(metadata, &pending) else {
            continue;
        };
        if !proof.iter().any(|(path, _)| path == &pending.provider_path) {
            continue;
        }
        let defaults = format!("{root}/{}", pending.defaults);
        let Some(source) = read_recorded(project, metadata, &defaults, BYTES) else {
            continue;
        };
        read_bytes += source.len();
        if read_bytes > 2 * 1024 * 1024 || links >= 64 {
            break;
        }
        let Some(anchor) = config_return(&source) else {
            continue;
        };
        proof.extend(authority.clone());
        proof.push((defaults.clone(), crate::artisan_metadata::hash(&source)));
        proof.sort();
        proof.dedup();
        let target = crate::index::Entry {
            path: defaults,
            at: text::position(&source, anchor),
            width: 6,
            detail: Some(format!("Package defaults for {}", pending.key)),
        };
        for (app, key) in &configs {
            if pending.key != *key && !pending.key.starts_with(&format!("{key}.")) {
                continue;
            }
            if *app == target.path {
                continue;
            }
            let Some(app_source) = read_recorded(project, metadata, app, BYTES) else {
                continue;
            };
            read_bytes += app_source.len();
            if read_bytes > 2 * 1024 * 1024 {
                break;
            }
            if config_return(&app_source).is_none() {
                continue;
            }
            if let Some(relative) = app.strip_prefix(&format!("{root}/")) {
                metadata.inputs.insert(relative.into());
            }
            let parents = metadata.config_parents.entry(app.clone()).or_default();
            if !parents
                .iter()
                .any(|parent| parent.target.path == target.path && parent.key == pending.key)
            {
                parents.push(Parent {
                    key: pending.key.clone(),
                    target: target.clone(),
                    proof: proof.clone(),
                });
                links += 1;
            }
        }
    }
    if read_bytes > 2 * 1024 * 1024 || links >= 64 {
        metadata.issues.push(
            "Parent configuration cache reached its 2-MiB/64-link budget; remaining roots withheld"
                .into(),
        );
    }
}
fn app_config_incomplete(index: &Index, root: &str, key: &str) -> bool {
    let directory = format!("{root}/config");
    index.config_incomplete.iter().any(|path| {
        if path == &directory {
            return true;
        }
        let Some(relative) = path.strip_prefix(&format!("{directory}/")) else {
            return false;
        };
        let file_key = relative
            .strip_suffix(".php")
            .unwrap_or(relative)
            .replace('/', ".");
        file_key == key
            || file_key.starts_with(&format!("{key}."))
            || key.starts_with(&format!("{file_key}."))
    })
}

fn integer_key(value: &str) -> bool {
    value
        .parse::<i64>()
        .is_ok_and(|number| number.to_string() == value)
}
/// PHP array_merge preserves string keys, but renumbers every integer key in
/// source iteration order. Retain each entry's original source navigation.
fn renumber_merge(index: &mut Index, leaves: &mut [php::Leaf], prefix: &str) {
    let mut defaults = BTreeMap::<String, String>::new();
    for leaf in leaves.iter() {
        let Some(tail) = leaf.path.strip_prefix(prefix) else {
            continue;
        };
        if !tail.contains('.') && integer_key(tail) && !defaults.contains_key(tail) {
            defaults.insert(tail.to_owned(), defaults.len().to_string());
        }
    }
    let mut roots: Vec<_> = index
        .config
        .iter()
        .filter_map(|(name, entry)| {
            let tail = name.strip_prefix(prefix)?;
            (!tail.contains('.') && integer_key(tail)).then_some((tail.to_owned(), entry.clone()))
        })
        .collect();
    roots.sort_by(|a, b| {
        (&a.1.path, a.1.at.line, a.1.at.character).cmp(&(&b.1.path, b.1.at.line, b.1.at.character))
    });
    let app: BTreeMap<_, _> = roots
        .into_iter()
        .enumerate()
        .map(|(i, (name, _))| (name, (i + defaults.len()).to_string()))
        .collect();
    let mut moved = Vec::new();
    for (name, entry) in &index.config {
        let Some(tail) = name.strip_prefix(prefix) else {
            continue;
        };
        let (top, suffix) = tail.split_once('.').map_or((tail, ""), |(top, tail)| {
            (top, &name[name.len() - tail.len() - 1..])
        });
        if let Some(number) = app.get(top) {
            moved.push((
                name.clone(),
                format!("{prefix}{number}{suffix}"),
                entry.clone(),
                index.config_expressions.get(name).cloned(),
            ));
        }
    }
    for (old, _, _, _) in &moved {
        index.config.remove(old);
        index.config_expressions.remove(old);
    }
    for (_, new, entry, expression) in moved {
        index.config.insert(new.clone(), entry);
        if let Some(expression) = expression {
            index.config_expressions.insert(new, expression);
        }
    }
    for leaf in leaves {
        let Some(tail) = leaf.path.strip_prefix(prefix) else {
            continue;
        };
        let (top, suffix) = tail.split_once('.').map_or((tail, ""), |(top, tail)| {
            (top, &leaf.path[leaf.path.len() - tail.len() - 1..])
        });
        if let Some(number) = defaults.get(top) {
            leaf.path = format!("{prefix}{number}{suffix}");
        }
    }
}

/// Laravel 11+ bootstrap defaults are a separate merge contract: only named
/// framework options receive one additional array_merge level.
fn merge_framework_inventory(project: &Project, index: &mut Index) {
    if index
        .metadata
        .framework_version
        .is_none_or(|version| version.0 < 11)
    {
        return;
    }
    let Some(mapping) = index
        .packages
        .roots
        .iter()
        .find(|r| r.package == "laravel/framework" && r.namespace == "Illuminate\\")
        .cloned()
    else {
        return;
    };
    let Ok(root) = project.root() else {
        return;
    };
    let loader = format!(
        "{}/Foundation/Bootstrap/LoadConfiguration.php",
        mapping.directory
    );
    let application = format!("{}/Foundation/Application.php", mapping.directory);
    let Some(source) = read_recorded(project, &mut index.metadata, &format!("{root}/{loader}"), BYTES) else {
        return;
    };
    let code = compact(&source);
    if ![
        "$config=array_merge($base[$name],$config);",
        "foreach($this->mergeableOptions($name)as$option)",
        "$config[$option]=array_merge($base[$name][$option],$config[$option]);",
        "->in(__DIR__.'/../../../../config')",
    ]
    .iter()
    .all(|needle| code.contains(needle))
    {
        return;
    }
    let Some(app) = read_recorded(project, &mut index.metadata, &format!("{root}/{application}"), BYTES) else {
        return;
    };
    if !compact(&app).contains("$mergeFrameworkConfiguration=true;")
        && !code.contains("$base=$this->getBaseConfiguration();")
    {
        return;
    }
    let Some(bootstrap) = read_recorded(project, &mut index.metadata, &format!("{root}/bootstrap/app.php"), BYTES) else {
        index
            .config_incomplete
            .extend(index.config.values().map(|entry| entry.path.clone()));
        index.metadata.issues.push(
            "Framework bootstrap ownership/source unavailable; base defaults withheld".into(),
        );
        return;
    };
    {
        let code = compact(&bootstrap);
        if [
            "dontMergeFrameworkConfiguration",
            "bootstrapWith",
            "LoadConfiguration",
            "alwaysUse",
        ]
        .iter()
        .any(|needle| code.contains(needle))
        {
            index
                .metadata
                .issues
                .push("Custom framework configuration bootstrap: base defaults withheld".into());
            return;
        }
        index.metadata.inputs.insert("bootstrap/app.php".into());
    }
    // Resolve the actual source-relative framework config directory, never scan
    // arbitrary vendor trees. The verified loader above uses this exact path.
    let Some(directory) = normal(
        &format!(
            "{root}/{}/Foundation/Bootstrap/../../../../config",
            mapping.directory
        ),
        &root,
    ) else {
        return;
    };
    index.metadata.inputs.insert(loader);
    index.metadata.inputs.insert(application);
    index.metadata.domain_roots.insert(directory.clone());
    let mut files = find_recorded(project,&mut index.metadata,&format!("{directory}/*.php"),65);
    files.sort();
    files.dedup();
    if files.len() > 64 {
        index
            .metadata
            .issues
            .push("Framework config inventory exceeded 64-file bound; defaults withheld".into());
        return;
    }
    let mut bytes = 0;
    let mut remaining_leaves = 32768usize;
    for path in files {
        let Some(relative) = path.strip_prefix(&format!("{root}/{directory}/")) else {
            continue;
        };
        let Some(key) = relative.strip_suffix(".php").filter(|s| !s.contains('/')) else {
            continue;
        };
        let prefix = format!("{key}.");
        let app_entries: Vec<_> = index
            .config
            .iter()
            .filter(|(name, _)| *name == key || name.starts_with(&prefix))
            .map(|(n, e)| (n.clone(), e.clone()))
            .collect();
        if app_config_incomplete(index, &root, key)
            || app_entries
                .iter()
                .any(|(_, e)| index.config_incomplete.contains(&e.path))
        {
            for (_, entry) in app_entries {
                index.config_incomplete.insert(entry.path);
            }
            continue;
        }
        let Some(source) = read_recorded(project, &mut index.metadata, &path, BYTES) else {
            for (_, entry) in app_entries {
                index.config_incomplete.insert(entry.path);
            }
            continue;
        };
        bytes += source.len();
        if bytes > 2 * 1024 * 1024 {
            index.metadata.issues.push(
                "Framework config inventory reached 2-MiB read bound; remaining defaults withheld"
                    .into(),
            );
            index
                .config_incomplete
                .extend(index.config.values().map(|entry| entry.path.clone()));
            break;
        }
        let masked = php::mask_comments(&source);
        let (mut leaves, complete) = crate::config_arrays::paths(&masked, key);
        if leaves.len() > remaining_leaves {
            index
                .config_incomplete
                .extend(index.config.values().map(|entry| entry.path.clone()));
            index.metadata.issues.push(
                "Framework config inventory exceeded 32768-key bound; remaining defaults withheld"
                    .into(),
            );
            break;
        }
        remaining_leaves -= leaves.len();
        let Some(anchor) = config_return(&source) else {
            continue;
        };
        let mergeable: &[&str] = match key {
            "auth" => &["guards", "providers", "passwords"],
            "broadcasting" | "database" | "queue" => &["connections"],
            "cache" => &["stores"],
            "filesystems" => &["disks"],
            "logging" => &["channels"],
            "mail" => &["mailers"],
            _ => &[],
        };
        // Prove the per-option contract from the installed loader, rather than
        // applying current framework behavior to every version/config name.
        let mergeable = if mergeable.is_empty() {
            mergeable
        } else {
            let snippet = format!(
                "'{key}'=>[{}]",
                mergeable
                    .iter()
                    .map(|s| format!("'{s}'"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            if code.contains(&snippet) {
                mergeable
            } else {
                &[]
            }
        };
        renumber_merge(index, &mut leaves, &prefix);
        for option in mergeable {
            renumber_merge(index, &mut leaves, &format!("{prefix}{option}."));
        }
        let current: BTreeMap<_, _> = index
            .config
            .iter()
            .filter(|(name, _)| *name == key || name.starts_with(&prefix))
            .map(|(n, e)| (n.clone(), e.clone()))
            .collect();
        if !complete {
            index.config_incomplete.insert(path.clone());
            for (_, e) in &app_entries {
                index.config_incomplete.insert(e.path.clone());
            }
        }
        index
            .metadata
            .inputs
            .insert(format!("{directory}/{relative}"));
        index
            .config
            .entry(key.into())
            .or_insert(crate::index::Entry {
                path: path.clone(),
                at: text::position(&source, anchor),
                width: 6,
                detail: Some("config file".into()),
            });
        for leaf in leaves {
            let Some(tail) = leaf.path.strip_prefix(&prefix) else {
                continue;
            };
            let top = tail.split('.').next().unwrap();
            let topkey = format!("{key}.{top}");
            let shadowed = if current.contains_key(&topkey) {
                if mergeable.contains(&top)
                    && current
                        .get(&topkey)
                        .is_some_and(|e| e.detail.as_deref() == Some("array"))
                {
                    match tail.strip_prefix(&format!("{top}.")) {
                        Some(rest) => current
                            .contains_key(&format!("{topkey}.{}", rest.split('.').next().unwrap())),
                        None => true,
                    }
                } else {
                    true
                }
            } else {
                false
            };
            if shadowed {
                continue;
            }
            if !leaf.array && !index.config.contains_key(&leaf.path) {
                let mut preview: String = masked[leaf.expression.clone()]
                    .trim()
                    .chars()
                    .take(512)
                    .collect();
                if masked[leaf.expression.clone()].trim().chars().count() > 512 {
                    preview.push('…');
                }
                index.config_expressions.insert(leaf.path.clone(), preview);
            }
            index
                .config
                .entry(leaf.path)
                .or_insert(crate::index::Entry {
                    path: path.clone(),
                    at: text::position(&source, leaf.offset),
                    width: source[leaf.offset..leaf.offset + leaf.width]
                        .encode_utf16()
                        .count() as u32,
                    detail: if leaf.array {
                        Some("array".into())
                    } else {
                        leaf.value
                    },
                });
        }
    }
}

/// ServiceProvider::mergeConfigFrom uses array_merge(defaults, app), not recursive merge.
/// Reuse provider ownership/ancestry facts: there is no filesystem walk in this phase.
pub(super) fn merge_inventory(project: &Project, index: &mut Index) {
    merge_framework_inventory(project, index);
    let Ok(root) = project.root() else { return };
    let global_uncertain = index.metadata.config_pending.len() >= 64
        || index.metadata.config_pending.iter().any(|pending| {
            pending.key.is_empty()
                && pending_proof(&index.metadata, pending).is_some_and(|(framework, _)| framework)
        });
    let mut groups = BTreeMap::<String, Vec<Pending>>::new();
    for pending in &index.metadata.config_pending {
        groups
            .entry(pending.key.clone())
            .or_default()
            .push(pending.clone());
    }
    let mut bytes = 0usize;
    let mut remaining_leaves = 32768usize;
    for (key, declarations) in groups {
        if key.is_empty() {
            if declarations.iter().any(|pending| {
                pending_proof(&index.metadata, pending).is_some_and(|(framework, _)| framework)
            }) {
                index
                    .config_incomplete
                    .extend(index.config.values().map(|entry| entry.path.clone()));
                index.metadata.issues.push("Dynamic package configuration key prevents authoritative missing-key diagnostics".into());
            }
            continue;
        }
        let prefix = format!("{key}.");
        let app_entries: Vec<_> = index
            .config
            .iter()
            .filter(|(name, _)| *name == &key || name.starts_with(&prefix))
            .map(|(name, entry)| (name.clone(), entry.clone()))
            .collect();
        // Composer declaration iteration order is not Laravel provider registration order.
        // Do not pick one of several potentially different defaults for the same key.
        let distinct: BTreeSet<_> = declarations.iter().map(|p| p.defaults.as_str()).collect();
        if distinct.len() != 1 {
            for (_, entry) in app_entries {
                index.config_incomplete.insert(entry.path);
            }
            index.metadata.issues.push(format!(
                "Config defaults for {key} have ambiguous provider order; defaults withheld"
            ));
            continue;
        }
        let Some(pending) = declarations.iter().find(|pending| {
            pending_proof(&index.metadata, pending).is_some_and(|(framework, proof)| {
                framework && proof.iter().any(|(path, _)| path == &pending.provider_path)
            })
        }) else {
            continue;
        };
        if pending.defaults.is_empty(){
            for (_,entry) in app_entries{index.config_incomplete.insert(entry.path);}
            continue;
        }
        let path = format!("{root}/{}", pending.defaults);
        let Some(source) = read_recorded(project, &mut index.metadata, &path, BYTES) else {
            for (_, entry) in app_entries {
                index.config_incomplete.insert(entry.path);
            }
            continue;
        };
        bytes += source.len();
        if bytes > 2 * 1024 * 1024 {
            index.metadata.issues.push("Config default inventory reached its 2-MiB read budget; remaining defaults withheld".into());
            index
                .config_incomplete
                .extend(index.config.values().map(|entry| entry.path.clone()));
            break;
        }
        let masked = php::mask_comments(&source);
        let (mut leaves, complete) = crate::config_arrays::paths(&masked, &key);
        if leaves.len() > remaining_leaves {
            index
                .config_incomplete
                .extend(index.config.values().map(|entry| entry.path.clone()));
            index.metadata.issues.push(
                "Package config inventory exceeded 32768-key bound; remaining defaults withheld"
                    .into(),
            );
            break;
        }
        remaining_leaves -= leaves.len();
        let app_incomplete = app_config_incomplete(index, &root, &key)
            || app_entries
                .iter()
                .any(|(_, entry)| index.config_incomplete.contains(&entry.path));
        let app_scalar = index
            .config
            .get(&key)
            .is_some_and(|entry| !matches!(entry.detail.as_deref(), Some("array" | "config file")));
        if app_incomplete || app_scalar {
            for (_, entry) in app_entries {
                index.config_incomplete.insert(entry.path);
            }
            index.metadata.issues.push(format!(
                "Config defaults for {key} have an unresolved app override; defaults withheld"
            ));
            continue;
        }
        renumber_merge(index, &mut leaves, &prefix);
        // Any application top-level key shadows its complete default subtree, including
        // an empty array. A missing nested leaf must never leak from vendor defaults.
        let overridden: BTreeSet<_> = app_entries
            .iter()
            .filter_map(|(name, _)| {
                name.strip_prefix(&prefix)
                    .map(|tail| tail.split('.').next().unwrap().to_owned())
                    .filter(|top| !integer_key(top))
            })
            .collect();
        if !complete {
            index.config_incomplete.insert(path.clone());
            for (_, entry) in &app_entries {
                index.config_incomplete.insert(entry.path.clone());
            }
        }
        if config_return(&source).is_none() {
            continue;
        }
        let anchor = config_return(&source).unwrap();
        index
            .config
            .entry(key.clone())
            .or_insert(crate::index::Entry {
                path: path.clone(),
                at: text::position(&source, anchor),
                width: 6,
                detail: Some("config file".into()),
            });
        for leaf in leaves {
            let Some(tail) = leaf.path.strip_prefix(&prefix) else {
                continue;
            };
            if overridden.contains(tail.split('.').next().unwrap()) {
                continue;
            }
            if !leaf.array && !index.config.contains_key(&leaf.path) {
                let expression = masked[leaf.expression.clone()].trim();
                let mut preview: String = expression.chars().take(512).collect();
                if expression.chars().count() > 512 {
                    preview.push('…');
                }
                if preview.is_empty() {
                    preview = "Expression unavailable".into();
                }
                index.config_expressions.insert(leaf.path.clone(), preview);
            }
            index
                .config
                .entry(leaf.path)
                .or_insert(crate::index::Entry {
                    path: path.clone(),
                    at: text::position(&source, leaf.offset),
                    width: source[leaf.offset..leaf.offset + leaf.width]
                        .encode_utf16()
                        .count() as u32,
                    detail: if leaf.array {
                        Some("array".into())
                    } else {
                        leaf.value
                    },
                });
        }
    }
    if global_uncertain {
        index
            .config_incomplete
            .extend(index.config.values().map(|entry| entry.path.clone()));
    }
}

fn current<'a>(index: &'a Index, project: &Project, doc: &Document) -> Vec<&'a Parent> {
    let Some(parents) = index.metadata.config_parents.get(&doc.path) else {
        return Vec::new();
    };
    if !owned(project, &doc.path)
        || project.stat(&doc.path).is_err()
        || config_return(&doc.text).is_none()
    {
        return Vec::new();
    }
    let mut checked = BTreeMap::new();
    let mut total = 0;
    let mut out = Vec::new();
    for parent in parents.iter().take(8) {
        let mut valid = true;
        for (path, expected) in &parent.proof {
            let fresh = if let Some(fresh) = checked.get(path) {
                *fresh
            } else {
                let source = read(project, path, 2 * 1024 * 1024);
                total += source.as_ref().map_or(0, String::len);
                let fresh = total <= 4 * 1024 * 1024
                    && source
                        .is_some_and(|source| crate::artisan_metadata::hash(&source) == *expected);
                checked.insert(path.clone(), fresh);
                fresh
            };
            if !fresh {
                valid = false;
                break;
            }
        }
        if valid && owned(project, &parent.target.path) {
            out.push(parent);
        }
    }
    out
}
pub(super) fn markers(index: &Index, project: &Project, doc: &Document) -> Vec<LineMarker> {
    let Some(anchor) = config_return(&doc.text) else {
        return Vec::new();
    };
    let line = text::position(&doc.text, anchor).line;
    current(index, project, doc)
        .into_iter()
        .map(|parent| LineMarker {
            line,
            icon: Some("arrow-up".into()),
            tooltip: crate::ui_text::format("final.metadata.defaultsTooltip","Go to package defaults for {key}",&[("key",&parent.key)]),
            target: Some(Location {
                path: parent.target.path.clone(),
                range: TextRange::new(
                    parent.target.at,
                    Position::new(
                        parent.target.at.line,
                        parent.target.at.character + parent.target.width,
                    ),
                ),
            }),
        })
        .collect()
}
pub(super) fn definition(
    index: &Index,
    project: &Project,
    doc: &Document,
    at: usize,
) -> Vec<Location> {
    let Some(anchor) = config_return(&doc.text) else {
        return Vec::new();
    };
    if !(anchor..=anchor + 6).contains(&at) {
        return Vec::new();
    }
    markers(index, project, doc)
        .into_iter()
        .filter_map(|marker| marker.target)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{activated, doc};
    use openidea_plugin::{mock::MockHost, providers::Providers};
    const PROVIDER: &str = "<?php namespace Acme; use Illuminate\\Support\\ServiceProvider as Base; class ConfigProvider extends Base {public function register(){ $this->mergeConfigFrom(__DIR__.'/../config/demo.php','demo'); }}";
    const PROVIDER_PATH: &str = "vendor/acme/config/src/ConfigProvider.php";
    const DEFAULTS: &str = "<?php\n/* 😀 */\nreturn ['enabled'=>true];\n";
    fn setup(source: &str) -> (MockHost, crate::Main) {
        setup_with_app(source, true)
    }
    fn setup_with_app(source: &str, app: bool) -> (MockHost, crate::Main) {
        let host = MockHost::install();
        host.add_file("vendor/composer/installed.json",r#"{"packages":[{"name":"acme/config","install-path":"../acme/config","autoload":{"psr-4":{"Acme\\":"src/"}},"extra":{"laravel":{"providers":["Acme\\ConfigProvider"]}}}]}"#);
        host.add_file(PROVIDER_PATH, source);
        host.add_file("vendor/acme/config/config/demo.php", DEFAULTS);
        if app {
            host.add_file("config/demo.php", "<?php\nreturn ['enabled'=>false];\n");
        }
        let plugin = activated(&host);
        (host, plugin)
    }
    fn document() -> Document {
        doc(
            "/project/config/demo.php",
            "php",
            "<?php\nreturn ['enabled'=>false];\n",
        )
    }
    #[test]
    fn real_parent_config_gutter_and_return_navigation_follow_declared_provider() {
        let (host, mut plugin) = setup(PROVIDER);
        let doc = document();
        let markers = plugin.line_markers(&doc);
        let marker = markers
            .iter()
            .find(|marker| {
                marker.target.as_ref().is_some_and(|target| {
                    target.path.ends_with("/vendor/acme/config/config/demo.php")
                })
            })
            .expect("registered parent config marker");
        assert_eq!(marker.line, 1);
        assert_eq!(
            marker.target.as_ref().unwrap().range.start,
            text::position(DEFAULTS, DEFAULTS.find("return").unwrap())
        );
        let found = plugin.definition(&doc, Position::new(1, 2));
        assert_eq!(found[0].path, "/project/vendor/acme/config/config/demo.php");
        assert!(plugin.indexes["/project"].metadata.affects(PROVIDER_PATH));
        assert!(
            plugin.indexes["/project"]
                .metadata
                .affects("vendor/acme/config/config/demo.php")
        );
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }
    #[test]
    fn parent_config_markers_revalidate_provider_defaults_registry_and_nested_ownership() {
        let (host, mut plugin) = setup(PROVIDER);
        let doc = document();
        assert!(!plugin.line_markers(&doc).is_empty());
        host.add_file(PROVIDER_PATH, &PROVIDER.replace("'demo'", "'other'"));
        assert!(plugin.line_markers(&doc).is_empty());
        host.add_file(PROVIDER_PATH, PROVIDER);
        host.add_file(
            "vendor/acme/config/config/demo.php",
            "<?php return ['enabled'=>false];",
        );
        assert!(plugin.line_markers(&doc).is_empty());
        host.add_file("vendor/acme/config/config/demo.php", DEFAULTS);
        host.add_file("vendor/composer/installed.json", r#"{"packages":[]}"#);
        assert!(plugin.line_markers(&doc).is_empty());
        let (host, mut plugin) = setup(PROVIDER);
        host.add_project("/project/vendor/acme/config");
        assert!(plugin.line_markers(&document()).is_empty());
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }
    #[test]
    fn fake_receivers_overridden_methods_dynamic_paths_and_unregistered_sources_have_no_parent() {
        for source in [
            "<?php namespace Acme; class ConfigProvider extends \\Vendor\\ServiceProvider {function register(){$this->mergeConfigFrom(__DIR__.'/../config/demo.php','demo');}}",
            "<?php namespace Acme; class ConfigProvider extends \\Illuminate\\Support\\ServiceProvider {function register(){$other->mergeConfigFrom(__DIR__.'/../config/demo.php','demo');}}",
            "<?php namespace Acme; class ConfigProvider extends \\Illuminate\\Support\\ServiceProvider {protected function mergeConfigFrom($path,$key){} function register(){$this->mergeConfigFrom(__DIR__.'/../config/demo.php','demo');}}",
            "<?php namespace Acme; class ConfigProvider extends \\Illuminate\\Support\\ServiceProvider {function register(){$this->mergeConfigFrom($path,'demo');}}",
            "<?php namespace Acme; class ConfigProvider extends \\Illuminate\\Support\\ServiceProvider {function register(){$x=\"$this->mergeConfigFrom(__DIR__.'/../config/demo.php','demo');\";}}",
            "<?php namespace Acme; class ConfigProvider extends \\Illuminate\\Support\\ServiceProvider {function register(){$this->mergeConfigFrom('/foreign/config/demo.php','demo');}}",
        ] {
            let (_host, mut plugin) = setup(source);
            assert!(plugin.line_markers(&document()).is_empty(), "{source}");
        }
        let (host, mut plugin) = setup(
            "<?php namespace Acme; class ConfigProvider extends \\Illuminate\\Support\\ServiceProvider {}",
        );
        host.add_file(
            "vendor/acme/config/src/Unregistered.php",
            &PROVIDER.replace("ConfigProvider", "Unregistered"),
        );
        plugin.indexes.insert(
            "/project".into(),
            Index::build(&host.invocation().project().unwrap()),
        );
        assert!(plugin.line_markers(&document()).is_empty());
    }
    #[test]
    fn inherited_provider_and_literal_dirname_defaults_are_proven_without_php_execution() {
        let provider = "<?php namespace Acme; class ConfigProvider extends BaseProvider {function register(){$this->mergeConfigFrom(dirname(__DIR__).'/config/demo.php','demo');}}";
        let (host, mut plugin) = setup(provider);
        host.add_file("vendor/acme/config/src/BaseProvider.php","<?php namespace Acme; class BaseProvider extends \\Illuminate\\Support\\ServiceProvider {}");
        plugin.indexes.insert(
            "/project".into(),
            Index::build(&host.invocation().project().unwrap()),
        );
        assert!(!plugin.line_markers(&document()).is_empty());
        host.add_file("vendor/acme/config/src/BaseProvider.php","<?php namespace Acme; class BaseProvider extends \\Illuminate\\Support\\ServiceProvider {function mergeConfigFrom($path,$key){}}");
        assert!(plugin.line_markers(&document()).is_empty());
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }
    #[test]
    fn package_defaults_inventory_includes_unpublished_keys_and_exact_utf16_sources() {
        let (host, _) = setup_with_app(PROVIDER, false);
        let defaults =
            "<?php /* 😀 */ return ['enabled'=>true,'nested'=>['timeout'=>30],'emoji😀'=>42];";
        host.add_file("vendor/acme/config/config/demo.php", defaults);
        let index = Index::build(&host.invocation().project().unwrap());
        assert!(index.config.contains_key("demo.enabled"));
        assert!(index.config.contains_key("demo.nested.timeout"));
        assert_eq!(
            index
                .config_expressions
                .get("demo.nested.timeout")
                .map(String::as_str),
            Some("30")
        );
        let entry = &index.config["demo.emoji😀"];
        assert_eq!(
            entry.at,
            text::position(defaults, defaults.find("emoji😀").unwrap())
        );
        assert_eq!(entry.width, "emoji😀".encode_utf16().count() as u32);
        assert!(entry.path.ends_with("vendor/acme/config/config/demo.php"));
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }
    #[test]
    fn package_config_app_overrides_shallow_top_level_subtrees_even_empty_arrays() {
        let (host, _) = setup(PROVIDER);
        host.add_file("vendor/acme/config/config/demo.php","<?php return ['enabled'=>true,'nested'=>['timeout'=>30,'retries'=>2],'untouched'=>['value'=>9],'empty'=>['hidden'=>1]];");
        host.add_file(
            "config/demo.php",
            "<?php return ['enabled'=>false,'nested'=>['timeout'=>7],'empty'=>[]];",
        );
        let index = Index::build(&host.invocation().project().unwrap());
        assert_eq!(index.config_expressions["demo.enabled"], "false");
        assert!(
            index.config["demo.nested.timeout"]
                .path
                .ends_with("/config/demo.php")
        );
        assert!(!index.config.contains_key("demo.nested.retries"));
        assert!(!index.config.contains_key("demo.empty.hidden"));
        assert!(index.config.contains_key("demo.untouched.value"));
        assert!(crate::config_inspections::missing(
            &index,
            "demo.nested.retries"
        ));
    }
    #[test]
    fn dynamic_app_and_defaults_refuse_false_missing_key_diagnostics() {
        let (host, _) = setup(PROVIDER);
        host.add_file("config/demo.php", "<?php return ['known'=>1,...$runtime];");
        let index = Index::build(&host.invocation().project().unwrap());
        assert!(!index.config.contains_key("demo.enabled"));
        assert!(!crate::config_inspections::missing(&index, "demo.unknown"));
        let (host, _) = setup_with_app(PROVIDER, false);
        host.add_file(
            "vendor/acme/config/config/demo.php",
            "<?php return ['known'=>1,...$runtime];",
        );
        let index = Index::build(&host.invocation().project().unwrap());
        assert!(index.config.contains_key("demo.known"));
        assert!(!crate::config_inspections::missing(&index, "demo.unknown"));
    }
    #[test]
    fn ambiguous_provider_defaults_never_choose_lexical_registration_order() {
        let (host, _) = setup(PROVIDER);
        host.add_file(
            PROVIDER_PATH,
            &PROVIDER
                .replace("$this->mergeConfigFrom", "$this->mergeConfigFrom")
                .replace(
                    "; }}",
                    "; $this->mergeConfigFrom(__DIR__.'/../config/other.php','demo'); }}",
                ),
        );
        host.add_file(
            "vendor/acme/config/config/other.php",
            "<?php return ['other'=>2];",
        );
        let index = Index::build(&host.invocation().project().unwrap());
        assert!(!index.config.contains_key("demo.other"));
        assert!(!crate::config_inspections::missing(&index, "demo.unknown"));
        assert!(
            index
                .metadata
                .issues
                .iter()
                .any(|issue| issue.contains("ambiguous provider order"))
        );
    }

    #[test]
    #[ignore = "requires installed Laravel13 and PackageTools source fixtures under /tmp"]
    fn installed_framework_and_package_tools_config_contracts_match_runtime() {
        let host = MockHost::install();
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13.0"}}"#,
        );
        host.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"laravel/framework","version":"v13.40.0"}]}"#,
        );
        host.add_file("vendor/composer/installed.json",r#"{"packages":[{"name":"laravel/framework","version":"v13.40.0","install-path":"../laravel/framework","autoload":{"psr-4":{"Illuminate\\":"src/Illuminate/"}}},{"name":"spatie/laravel-package-tools","install-path":"../spatie/laravel-package-tools","autoload":{"psr-4":{"Spatie\\LaravelPackageTools\\":"src/"}}},{"name":"acme/config","install-path":"../acme/config","autoload":{"psr-4":{"Acme\\":"src/"}},"extra":{"laravel":{"providers":["Acme\\ConfigProvider"]}}}]}"#);
        host.add_file("bootstrap/app.php",r#"<?php return \Illuminate\Foundation\Application::configure(basePath: dirname(__DIR__))->create();"#);
        for path in [
            "src/Illuminate/Foundation/Bootstrap/LoadConfiguration.php",
            "src/Illuminate/Foundation/Application.php",
        ] {
            let source = std::fs::read_to_string(format!(
                "/tmp/laravel-artisan-acceptance-20261110/vendor/laravel/framework/{path}"
            ))
            .unwrap();
            host.add_file(&format!("vendor/laravel/framework/{path}"), &source);
        }
        for path in [
            "PackageServiceProvider.php",
            "Package.php",
            "Concerns/Package/HasConfigs.php",
            "Concerns/PackageServiceProvider/ProcessConfigs.php",
        ] {
            let source=std::fs::read_to_string(format!("/tmp/laravel-parity-filament-3-20261110/vendor/spatie/laravel-package-tools/src/{path}")).unwrap();
            host.add_file(
                &format!("vendor/spatie/laravel-package-tools/src/{path}"),
                &source,
            );
        }
        host.add_file("vendor/laravel/framework/config/cache.php","<?php return ['default'=>'database','stores'=>['file'=>['driver'=>'file'],'redis'=>['driver'=>'redis']],'prefix'=>'laravel'];");
        host.add_file(
            "config/cache.php",
            "<?php return ['stores'=>['file'=>['driver'=>'array']]];",
        );
        host.add_file(PROVIDER_PATH,"<?php namespace Acme; use Spatie\\LaravelPackageTools\\Package; use Spatie\\LaravelPackageTools\\PackageServiceProvider; class ConfigProvider extends PackageServiceProvider { public function configurePackage(Package $package):void { $package->name('acme-laravel-demo')->hasConfigFile(); }}");
        host.add_file(
            "vendor/acme/config/config/demo.php",
            "<?php return ['enabled'=>true,'nested'=>['timeout'=>30]];",
        );
        let index = Index::build(&host.invocation().project().unwrap());
        assert!(
            index.config.contains_key("demo.enabled"),
            "{:?}",
            index.metadata.issues
        );
        assert!(index.config.contains_key("cache.default"));
        assert!(index.config.contains_key("cache.stores.redis.driver"));
        assert_eq!(
            index.config_expressions["cache.stores.file.driver"],
            "'array'"
        );
        assert!(
            index.config["cache.stores.redis.driver"]
                .path
                .contains("vendor/laravel/framework/config")
        );
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
        // PackageTools' explicit multi-file list and Providers-directory base rule.
        host.add_file(PROVIDER_PATH,"<?php namespace Acme; use Spatie\\LaravelPackageTools\\Package; use Spatie\\LaravelPackageTools\\PackageServiceProvider; class ConfigProvider extends PackageServiceProvider { public function configurePackage(Package $package):void { $package->name('laravel-demo')->hasConfigFile(['demo','extra']); }}");
        host.add_file(
            "vendor/acme/config/config/extra.php",
            "<?php return ['key'=>2];",
        );
        let index = Index::build(&host.invocation().project().unwrap());
        assert!(index.config.contains_key("extra.key"));
        // A custom base-dir override invalidates the package adapter.
        host.add_file(PROVIDER_PATH,"<?php namespace Acme; use Spatie\\LaravelPackageTools\\Package; use Spatie\\LaravelPackageTools\\PackageServiceProvider; class ConfigProvider extends PackageServiceProvider { public function getPackageBaseDir(): string{return '/other';} public function configurePackage(Package $package):void { $package->name('acme-laravel-demo')->hasConfigFile(); }}");
        let index = Index::build(&host.invocation().project().unwrap());
        assert!(!index.config.contains_key("demo.enabled"));
        for override_method in ["normalizeConfigKey", "normalizeConfigPath"] {
            host.add_file(PROVIDER_PATH,&format!("<?php namespace Acme; use Spatie\\LaravelPackageTools\\Package; use Spatie\\LaravelPackageTools\\PackageServiceProvider; class ConfigProvider extends PackageServiceProvider {{ protected function {override_method}(string $name): string {{return 'other';}} public function configurePackage(Package $package):void {{ $package->name('demo')->hasConfigFile(); }} }}"));
            let index = Index::build(&host.invocation().project().unwrap());
            assert!(
                !index.config.contains_key("demo.enabled"),
                "{override_method}"
            );
        }
        host.add_file("config/demo.php", "<?php return ['local'=>1];");
        host.add_file(PROVIDER_PATH,"<?php namespace Acme; use Spatie\\LaravelPackageTools\\Package; use Spatie\\LaravelPackageTools\\PackageServiceProvider; class ConfigProvider extends PackageServiceProvider { public function configurePackage(Package $package):void { $package->name('demo')->hasConfigFile($runtime); }}");
        let index = Index::build(&host.invocation().project().unwrap());
        assert!(!crate::config_inspections::missing(
            &index,
            "demo.dynamic_default"
        ));
        // A separately owned bootstrap cannot prove that base merging is enabled.
        host.add_project("/project/bootstrap");
        let index = Index::build(&host.invocation().project().unwrap());
        assert!(!index.config.contains_key("cache.default"));
    }
    #[test]
    fn dynamic_registered_config_key_and_path_cannot_prove_absence() {
        for provider in [
            PROVIDER.replace("'demo'", "$key"),
            PROVIDER.replace("__DIR__.'/../config/demo.php'", "$path"),
        ] {
            let (host, _) = setup(&provider);
            let index = Index::build(&host.invocation().project().unwrap());
            assert!(!crate::config_inspections::missing(&index, "demo.unproven"));
            assert!(!index.config.contains_key("demo.enabled.missing"));
            assert!(crate::index::index_sources_current(&host.invocation().project().unwrap(),&index),"unresolved paths must preserve exact captured source proof: {:?}",index.metadata.issues);
            assert!(!index.metadata.source_evidence.contains_key(""));
        }
    }
    #[test]
    fn config_array_merge_renumbers_integer_keys_in_source_order() {
        let (host, _) = setup(PROVIDER);
        host.add_file(
            "vendor/acme/config/config/demo.php",
            "<?php return [5=>['value'=>1],2=>'second','label'=>'base'];",
        );
        host.add_file(
            "config/demo.php",
            "<?php return [9=>'app','label'=>'override'];",
        );
        let index = Index::build(&host.invocation().project().unwrap());
        assert_eq!(index.config_expressions["demo.0.value"], "1");
        assert_eq!(index.config_expressions["demo.1"], "'second'");
        assert_eq!(index.config_expressions["demo.2"], "'app'");
        assert_eq!(index.config_expressions["demo.label"], "'override'");
        assert!(!index.config.contains_key("demo.9"));
        assert!(
            index.config["demo.0.value"]
                .path
                .contains("vendor/acme/config")
        );
        assert!(index.config["demo.2"].path.ends_with("/config/demo.php"));
    }
    #[test]
    #[ignore = "requires installed Laravel10-13 source fixtures under /tmp"]
    fn installed_framework_config_merge_version_boundaries() {
        for (fixture, version) in [
            ("/tmp/laravel-native-compat-10-first", "10.0.0"),
            ("/tmp/laravel-native-compat-11-first", "11.0.0"),
            ("/tmp/laravel-native-compat-11-latest", "11.57.1"),
            ("/tmp/laravel-native-compat-12-first", "12.0.0"),
            ("/tmp/laravel-native-compat-12-latest", "12.72.0"),
            ("/tmp/laravel-artisan-acceptance-20261110", "13.40.0"),
        ] {
            let host = MockHost::install();
            host.add_file("composer.json", r#"{"require":{"laravel/framework":"*"}}"#);
            host.add_file(
                "composer.lock",
                &format!(
                    r#"{{"packages":[{{"name":"laravel/framework","version":"v{version}"}}]}}"#
                ),
            );
            host.add_file("vendor/composer/installed.json",r#"{"packages":[{"name":"laravel/framework","install-path":"../laravel/framework","autoload":{"psr-4":{"Illuminate\\":"src/Illuminate/"}}}]}"#);
            host.add_file("bootstrap/app.php",r#"<?php return \Illuminate\Foundation\Application::configure(basePath: dirname(__DIR__))->create();"#);
            for path in [
                "src/Illuminate/Foundation/Bootstrap/LoadConfiguration.php",
                "src/Illuminate/Foundation/Application.php",
            ] {
                let source =
                    std::fs::read_to_string(format!("{fixture}/vendor/laravel/framework/{path}"))
                        .unwrap();
                host.add_file(&format!("vendor/laravel/framework/{path}"), &source);
            }
            host.add_file("vendor/laravel/framework/config/cache.php","<?php return ['default'=>'database','stores'=>['file'=>['driver'=>'file','path'=>'default'],'redis'=>['driver'=>'redis']]];");
            host.add_file(
                "config/cache.php",
                "<?php return ['stores'=>['file'=>['driver'=>'array']]];",
            );
            let index = Index::build(&host.invocation().project().unwrap());
            assert_eq!(
                index.config.contains_key("cache.default"),
                !version.starts_with("10."),
                "{version}"
            );
            if !version.starts_with("10.") {
                assert!(
                    index.config.contains_key("cache.stores.redis.driver"),
                    "{version}"
                );
                assert!(
                    !index.config.contains_key("cache.stores.file.path"),
                    "{version}"
                );
                assert_eq!(
                    index.config_expressions["cache.stores.file.driver"], "'array'",
                    "{version}"
                );
            }
        }
    }
    #[test]
    fn unread_app_config_without_an_entry_withholds_complete_vendor_root() {
        for incomplete in ["/project/config/demo.php", "/project/config"] {
            let (host, _) = setup(PROVIDER);
            let mut index = Index::build(&host.invocation().project().unwrap());
            index
                .config
                .retain(|name, _| name != "demo" && !name.starts_with("demo."));
            index.config_incomplete.insert(incomplete.into());
            merge_inventory(&host.invocation().project().unwrap(), &mut index);
            assert!(!index.config.contains_key("demo"));
            assert!(!crate::config_inspections::missing(&index, "demo.unread"));
        }
    }
    #[test]
    fn conditional_literal_provider_defaults_do_not_assert_presence_or_absence() {
        for body in [
            "if ($flag) $this->mergeConfigFrom(__DIR__.'/../config/demo.php','demo');",
            "if ($flag) {$this->mergeConfigFrom(__DIR__.'/../config/demo.php','demo');}",
        ] {
            let provider = format!(
                "<?php namespace Acme; class ConfigProvider extends \\Illuminate\\Support\\ServiceProvider {{function register(){{{body}}}}}"
            );
            let (host, _) = setup(&provider);
            host.add_file(
                "vendor/acme/config/config/demo.php",
                "<?php return ['conditional'=>1];",
            );
            let index = Index::build(&host.invocation().project().unwrap());
            assert!(!index.config.contains_key("demo.conditional"));
            assert!(!crate::config_inspections::missing(
                &index,
                "demo.conditional"
            ));
        }
    }
}
