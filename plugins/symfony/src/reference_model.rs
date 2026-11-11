//! Literal-source models behind autowiring navigation: services loaded with
//! `resource:` (id = FQCN), `#[AsAlias]` aliases and constructor type-hints.
//! Symfony autowiring docs: a type-hint is matched to a service id; resource
//! loading makes each class id its FQCN; an interface needs an alias
//! (`Interface: '@Class'`, `alias:` or `#[AsAlias]`). Nothing here runs PHP.

use std::ops::Range;

use openidea_plugin::Position;

use crate::index::{self, yaml_keys, Entry, Lines, Names};
use crate::php_text::{attributes, class_model, mask, resolver, Target, BUILT_IN_TYPES};
use crate::php_checks::{arguments, named, trimmed};

pub(crate) const RESOURCE_DETAIL: &str = "Autowired service loaded by resource";
const AS_ALIAS: &str = "Symfony\\Component\\DependencyInjection\\Attribute\\AsAlias";

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ResourceDef {
    pub namespace: String,
    pub dir: String,
    pub excludes: Vec<String>,
    pub path: String,
    pub at: Position,
}

fn unquote(value: &str) -> &str {
    index::without_comment(value).trim().trim_matches(['\'', '"'])
}

pub(crate) fn normalize(base: &str, relative: &str) -> String {
    let mut parts: Vec<&str> = if relative.starts_with('/') { Vec::new() } else { base.split('/').collect() };
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    let joined = parts.join("/");
    if relative.starts_with('/') || base.starts_with('/') { format!("/{}", joined.trim_start_matches('/')) } else { joined }
}

/// `a{b,c}d` into `abd`, `acd`; `None` for nested braces or other glob syntax.
fn expand(pattern: &str) -> Option<Vec<String>> {
    if pattern.contains(['*', '?', '[']) {
        return None;
    }
    let Some(open) = pattern.find('{') else { return Some(vec![pattern.to_owned()]) };
    let close = open + pattern[open..].find('}')?;
    let inner = &pattern[open + 1..close];
    if inner.contains(['{', '}']) || pattern[close + 1..].contains(['{', '}']) {
        return None;
    }
    Some(inner.split(',').map(|item| format!("{}{}{}", &pattern[..open], item.trim(), &pattern[close + 1..])).collect())
}

/// `Namespace\: resource: '../src/'` entries of a service file whose resource
/// and `exclude` are plain paths (an unknown glob drops the entry).
pub(crate) fn resource_defs(path: &str, text: &str) -> Vec<ResourceDef> {
    let lines = Lines::new(text);
    let keys = yaml_keys(text);
    let base = path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let mut found = Vec::new();

    for key in keys.iter().filter(|key| key.parents == ["services"] && key.key.ends_with('\\')) {
        let child = |name: &str| keys.iter().find(|child| child.key == name && child.parents == ["services", key.key.as_str()]);
        let Some(resource) = child("resource").map(|resource| unquote(&resource.value)).filter(|value| !value.is_empty() && !value.contains(['*', '?', '[', '{'])) else {
            continue;
        };
        let excludes = match child("exclude") {
            None => Some(Vec::new()),
            Some(exclude) => expand(unquote(&exclude.value)).filter(|_| !exclude.value.is_empty()),
        };
        let Some(excludes) = excludes else { continue };

        found.push(ResourceDef {
            namespace: key.key.clone(),
            dir: normalize(base, resource).trim_end_matches('/').to_owned(),
            excludes: excludes.iter().map(|pattern| normalize(base, pattern).trim_end_matches('/').to_owned()).collect(),
            path: path.to_owned(),
            at: lines.position(key.offset),
        });
    }

    found
}

/// Project classes that a resource entry turns into services: the class name
/// must be exactly the namespace plus the path below the resource directory.
pub(crate) fn resource_services(defs: &[ResourceDef], classes: &Names, out: &mut Names) {
    for (fqcn, class) in classes.iter().filter(|(_, class)| !class.path.is_empty()) {
        for def in defs {
            let Some(relative) = class.path.strip_prefix(&format!("{}/", def.dir)).and_then(|rest| rest.strip_suffix(".php")) else { continue };
            let excluded = def.excludes.iter().any(|exclude| class.path == *exclude || class.path.starts_with(&format!("{exclude}/")));
            if excluded || *fqcn != format!("{}{}", def.namespace, relative.replace('/', "\\")) {
                continue;
            }
            out.entry(fqcn.clone()).or_insert(Entry { detail: Some(RESOURCE_DETAIL.into()), path: def.path.clone(), at: def.at });
            break;
        }
    }
}

/// Alias ids a class declares with `#[AsAlias]` (literal `X::class` or string
/// id, or the single implemented interface when no id is given); `when` and
/// `target` variants are skipped.
pub(crate) fn as_aliases(text: &str) -> Vec<(String, usize)> {
    if !text.contains("AsAlias") {
        return Vec::new();
    }
    let Some(model) = class_model(text) else { return Vec::new() };
    let masked = mask(text);
    let resolve = resolver(text);
    let mut found = Vec::new();

    for attribute in attributes(text) {
        if !matches!(attribute.target, Target::Class { .. }) || attribute.name != AS_ALIAS {
            continue;
        }

        let mut id = None;
        let mut skipped = false;
        let arguments = match attribute.arguments.start {
            0 => Vec::new(),
            start => arguments(&masked, start - 1),
        };
        for (position, argument) in arguments.into_iter().enumerate() {
            match named(&masked, argument.clone()) {
                Some((name, value)) => match &masked[name] {
                    "id" => id = Some(value),
                    "public" => {}
                    _ => skipped = true,
                },
                None if position == 0 => id = Some(trimmed(&masked, argument)),
                None => skipped = true,
            }
        }
        if skipped {
            continue;
        }

        let alias = match id {
            Some(value) => alias_id(&text[value], &resolve),
            None => (model.interfaces.len() == 1).then(|| model.interfaces[0].clone()),
        };
        found.extend(alias.map(|alias| (alias, attribute.range.start)));
    }

    found
}

fn alias_id(value: &str, resolve: &impl Fn(&str) -> String) -> Option<String> {
    let value = value.trim();
    if let Some(class) = value.strip_suffix("::class") {
        let class = class.trim();
        return (!class.is_empty() && class.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'\\')).then(|| resolve(class));
    }
    let inner = value.strip_prefix('\'').and_then(|rest| rest.strip_suffix('\'')).or_else(|| value.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')))?;
    (!inner.is_empty() && !inner.contains(['$', '{'])).then(|| inner.to_owned())
}

/// Class a constructor parameter's type-hint under `offset` names. Parameters
/// with attributes (`#[Autowire]`, `#[Target]`) are explicit: quiet.
pub(crate) fn constructor_type_at(text: &str, offset: usize) -> Option<String> {
    let model = class_model(text)?;
    let constructor = model.method("__construct")?;
    let param = constructor.params.iter().find(|param| param.range.contains(&offset) && param.range.start <= offset)?;
    if param.has_attribute || param.variadic {
        return None;
    }
    let type_name = param.type_name.as_deref().filter(|name| !BUILT_IN_TYPES.contains(&name.to_ascii_lowercase().as_str()))?;

    let masked = mask(text);
    let dollar = param.range.start + masked[param.range.clone()].find('$')?;
    if offset >= dollar {
        return None;
    }
    let word = word_around(&masked, offset)?;
    (resolver(text)(masked[word].trim_start_matches('?')) == type_name).then(|| type_name.to_owned())
}

fn word_around(masked: &str, offset: usize) -> Option<Range<usize>> {
    let part = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'\\';
    let bytes = masked.as_bytes();
    let mut start = offset;
    while start > 0 && part(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset;
    while end < bytes.len() && part(bytes[end]) {
        end += 1;
    }
    (start < end).then_some(start..end)
}

/// A concrete class: not an interface, trait, enum or abstract class.
pub(crate) fn is_instantiable(text: &str) -> bool {
    let masked = mask(text);
    let Some(model) = class_model(text) else { return false };
    let declaration = &masked[..model.name_range.start];
    let line = declaration.rsplit('\n').next().unwrap_or("");
    let words: Vec<&str> = line.split_whitespace().collect();
    words.last() == Some(&"class") && !words.contains(&"abstract")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes(pairs: &[(&str, &str)]) -> Names {
        pairs.iter().map(|(class, path)| ((*class).to_owned(), Entry { detail: None, path: (*path).to_owned(), at: Position::new(0, 0) })).collect()
    }

    #[test]
    fn resource_prefix_maps_only_exact_psr4_classes_and_honours_exclude() {
        let yaml = "services:\n    _defaults:\n        autowire: true\n    App\\:\n        resource: '../src/'\n        exclude: '../src/{Entity,Kernel.php}'\n";
        let defs = resource_defs("/p/config/services.yaml", yaml);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].dir, "/p/src");
        assert_eq!(defs[0].excludes, ["/p/src/Entity", "/p/src/Kernel.php"]);
        assert_eq!(defs[0].at, Position::new(3, 4));

        let known = classes(&[
            ("App\\Mailer", "/p/src/Mailer.php"),
            ("App\\Entity\\User", "/p/src/Entity/User.php"),
            ("App\\Kernel", "/p/src/Kernel.php"),
            ("App\\Sub\\Wrong", "/p/src/Sub/Other.php"),
            ("Lib\\Thing", "/p/lib/Thing.php"),
        ]);
        let mut out = Names::new();
        resource_services(&defs, &known, &mut out);
        assert_eq!(out.keys().collect::<Vec<_>>(), ["App\\Mailer"]);
        assert_eq!(out["App\\Mailer"].path, "/p/config/services.yaml");
    }

    #[test]
    fn globbed_or_listed_resource_and_exclude_are_quiet() {
        for yaml in [
            "services:\n    App\\:\n        resource: '../src/*'\n",
            "services:\n    App\\:\n        resource: '../src/'\n        exclude: '../src/**/Entity'\n",
            "services:\n    App\\:\n        resource: '../src/'\n        exclude:\n            - '../src/Entity'\n",
        ] {
            assert!(resource_defs("/p/config/services.yaml", yaml).is_empty(), "{yaml}");
        }
    }

    #[test]
    fn as_alias_reads_class_string_or_single_interface() {
        let base = "<?php\nnamespace App;\nuse Symfony\\Component\\DependencyInjection\\Attribute\\AsAlias;\n";
        let with = |attribute: &str, tail: &str| format!("{base}{attribute}\nclass Text{tail} {{}}\n");
        assert_eq!(as_aliases(&with("#[AsAlias(FormatterInterface::class)]", ""))[0].0, "App\\FormatterInterface");
        assert_eq!(as_aliases(&with("#[AsAlias(id: 'app.fmt', public: true)]", ""))[0].0, "app.fmt");
        assert_eq!(as_aliases(&with("#[AsAlias]", " implements Fmt"))[0].0, "App\\Fmt");
        assert!(as_aliases(&with("#[AsAlias]", " implements A, B")).is_empty());
        assert!(as_aliases(&with("#[AsAlias(A::class, when: 'dev')]", "")).is_empty());
        assert!(as_aliases(&with("#[AsAlias(A::class, target: 'x')]", "")).is_empty());
        assert!(as_aliases(&with("#[AsAlias($dyn)]", "")).is_empty());
    }

    #[test]
    fn constructor_type_hint_resolves_only_plain_typed_parameters() {
        let text = "<?php\nnamespace App;\nuse App\\Fmt\\FormatterInterface;\nclass N {\n    public function __construct(private FormatterInterface $f, #[Target('x')] FormatterInterface $g, int $n, string $s) {}\n}\n";
        let at = |needle: &str, delta: usize| text.find(needle).unwrap() + delta;
        assert_eq!(constructor_type_at(text, at("FormatterInterface $f", 3)).as_deref(), Some("App\\Fmt\\FormatterInterface"));
        assert_eq!(constructor_type_at(text, at("$f", 1)), None);
        assert_eq!(constructor_type_at(text, at("FormatterInterface $g", 3)), None);
        assert_eq!(constructor_type_at(text, at("int $n", 1)), None);
        assert_eq!(constructor_type_at(text, at("class N", 1)), None);
    }

    #[test]
    fn instantiable_rejects_interface_abstract_trait_enum() {
        assert!(is_instantiable("<?php\nnamespace A;\nfinal class X {}\n"));
        assert!(!is_instantiable("<?php\nnamespace A;\ninterface X {}\n"));
        assert!(!is_instantiable("<?php\nnamespace A;\nabstract class X {}\n"));
        assert!(!is_instantiable("<?php\nnamespace A;\ntrait X {}\n"));
        assert!(!is_instantiable("<?php\nnamespace A;\nenum X {}\n"));
    }
}

#[cfg(test)]
mod project_tests {
    use crate::index::Index;
    use crate::Main;
    use openidea_plugin::mock::MockHost;
    use openidea_plugin::providers::{Document, Providers};
    use openidea_plugin::{Plugin, Position};

    const COMPOSER: &str = r#"{"require":{"symfony/framework-bundle":"^7.0"},"autoload":{"psr-4":{"App\\":"src/"}}}"#;
    const SERVICES: &str = "services:\n    _defaults:\n        autowire: true\n    App\\:\n        resource: '../src/'\n        exclude: '../src/Entity'\n    App\\Fmt\\FormatterInterface: '@App\\Fmt\\TextFormatter'\n";
    const ROUTES: &str = "controllers:\n    resource:\n        path: ../src/Controller/\n        namespace: App\\Controller\n    type: attribute\n";

    fn project(host: &MockHost) {
        host.add_file("composer.json", COMPOSER);
        host.add_file("config/services.yaml", SERVICES);
        host.add_file("config/routes.yaml", ROUTES);
        host.add_file("src/Fmt/TextFormatter.php", "<?php\nnamespace App\\Fmt;\nclass TextFormatter implements FormatterInterface {}\n");
        host.add_file("src/Fmt/FormatterInterface.php", "<?php\nnamespace App\\Fmt;\ninterface FormatterInterface {}\n");
        host.add_file("src/Fmt/HtmlFormatter.php", "<?php\nnamespace App\\Fmt;\nuse Symfony\\Component\\DependencyInjection\\Attribute\\AsAlias;\n#[AsAlias(HtmlInterface::class)]\nclass HtmlFormatter {}\n");
        host.add_file("src/Entity/User.php", "<?php\nnamespace App\\Entity;\nclass User {}\n");
        host.add_file("src/Controller/HomeController.php", "<?php\nnamespace App\\Controller;\nclass HomeController\n{\n    #[Route('/', name: 'app_home')]\n    public function index() {}\n}\n");
    }

    fn build(host: &MockHost) -> Index {
        Index::build(&host.invocation().project().unwrap())
    }

    #[test]
    fn index_lists_resource_services_and_as_aliases_but_not_excluded_classes() {
        let host = MockHost::install();
        project(&host);
        let index = build(&host);
        assert!(index.autowired.contains_key("App\\Fmt\\TextFormatter"));
        assert!(!index.autowired.contains_key("App\\Entity\\User"));
        assert_eq!(index.autowired["App\\Fmt\\HtmlInterface"].detail.as_deref(), Some("#[AsAlias] on App\\Fmt\\HtmlFormatter"));
        assert_eq!(index.services["App\\Fmt\\FormatterInterface"].detail.as_deref(), Some("alias of App\\Fmt\\TextFormatter"));
    }

    #[test]
    fn constructor_type_hint_navigates_to_alias_or_resource_and_skips_interfaces() {
        let host = MockHost::install();
        project(&host);
        let mut plugin = Main::default();
        plugin.activate(&host.context("aritti.symfony")).unwrap();
        let source = "<?php\nnamespace App;\nuse App\\Fmt\\{TextFormatter};\nuse App\\Fmt\\FormatterInterface;\nuse App\\Fmt\\HtmlInterface;\nuse App\\Entity\\User;\nclass N {\n    public function __construct(TextFormatter $a, FormatterInterface $b, HtmlInterface $c, User $d) {}\n}\n";
        let doc = Document { path: "/project/src/N.php".into(), language: "php".into(), version: 1, text: source.into() };
        let column = |needle: &str| (source.lines().nth(7).unwrap().find(needle).unwrap() + 2) as u32;

        let resource = plugin.definition(&doc, Position::new(7, column("TextFormatter $a")));
        assert_eq!((resource[0].path.as_str(), resource[0].range.start), ("/project/config/services.yaml", Position::new(3, 4)));
        let alias = plugin.definition(&doc, Position::new(7, column("FormatterInterface $b")));
        assert_eq!((alias[0].path.as_str(), alias[0].range.start.line), ("/project/config/services.yaml", 6));
        assert_eq!(plugin.definition(&doc, Position::new(7, column("HtmlInterface $c")))[0].path, "/project/src/Fmt/HtmlFormatter.php");
        assert!(plugin.definition(&doc, Position::new(7, column("User $d"))).is_empty());
    }

    #[test]
    fn literal_route_set_warns_on_unknown_php_route_only_from_url_generators() {
        let host = MockHost::install();
        project(&host);
        let index = build(&host);
        assert!(index.routes_complete);
        let run = |call: &str| crate::problems(&index, "/project/src/A.php", "php", &format!("<?php\n{call};\n")).into_iter().map(|d| d.code.unwrap()).collect::<Vec<_>>();
        assert_eq!(run("$this->generateUrl('app_ghost')"), ["missing-route"]);
        assert_eq!(run("$this->router->generate('app_ghost')"), ["missing-route"]);
        for quiet in [
            "$this->generateUrl('app_home')",
            "$this->generateUrl('_profiler')",
            "$this->generateUrl('App\\Controller\\HomeController::index')",
            "$this->generateUrl($name)",
            "$uuid->generate('app_ghost')",
        ] {
            assert!(run(quiet).is_empty(), "{quiet}");
        }
    }

    #[test]
    fn route_set_is_not_literal_when_names_or_imports_are_unknown() {
        for (path, text) in [
            ("src/Controller/B.php", "<?php\nnamespace App\\Controller;\nclass B {\n    #[Route('/b')]\n    public function b() {}\n}\n"),
            ("src/Controller/B.php", "<?php\nnamespace App\\Controller;\nclass B {\n    #[Route('/b', name: self::NAME)]\n    public function b() {}\n}\n"),
            ("config/routes.yaml", "bundle:\n    resource: '@AdminBundle/config/routes.yaml'\n"),
            ("config/routes.yaml", "controllers:\n    resource: routing.controllers\n"),
            ("config/routes.yaml", "controllers:\n    resource: ../src/Controller/\n    type: attribute\n    name_prefix: 'x_'\n"),
            ("config/routes/extra.php", "<?php\nreturn function () {};\n"),
        ] {
            let host = MockHost::install();
            project(&host);
            host.add_file(path, text);
            let index = build(&host);
            assert!(!index.routes_complete, "{path}: {text}");
            let found = crate::problems(&index, "/project/src/A.php", "php", "<?php\n$this->generateUrl('app_ghost');\n");
            assert!(found.is_empty());
        }
    }

    #[test]
    fn php_translation_key_navigates_inside_its_literal_domain() {
        let host = MockHost::install();
        project(&host);
        host.add_file("translations/admin.en.yaml", "menu:\n    home: Home\n");
        let mut plugin = Main::default();
        plugin.activate(&host.context("aritti.symfony")).unwrap();
        let source = "<?php\n$translator->trans('menu.home', [], 'admin');\n$translator->trans('menu.home', [], 'messages');\n";
        let doc = Document { path: "/project/src/A.php".into(), language: "php".into(), version: 1, text: source.into() };
        assert_eq!(plugin.definition(&doc, Position::new(1, 22))[0].path, "/project/translations/admin.en.yaml");
        assert!(plugin.definition(&doc, Position::new(2, 22)).is_empty());
    }
}
