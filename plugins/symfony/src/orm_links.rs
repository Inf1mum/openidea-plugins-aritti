//! Cross-file checks of Doctrine association names and form fields, from
//! literal source only. Doctrine ORM attributes reference: `mappedBy` and
//! `inversedBy` name a property on the `targetEntity`. Symfony Forms: a field
//! reads and writes the same-named property of the form's `data_class` unless
//! `mapped` is false, or `property_path`, `data` or `inherit_data` change it.
//! Every check stays quiet unless the whole class chain is readable.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use openidea_plugin::Severity;

use crate::checks::{Context, Finding};
use crate::php_checks::{arguments, named, trimmed};
use crate::php_text::{attributes, class_model, imports, mask, matching_close, read_word, resolver, Target};

const MAPPING: &str = "Doctrine\\ORM\\Mapping\\";
const MAGIC: [&str; 4] = ["__get", "__set", "__call", "__isset"];
const ACCESSORS: [&str; 7] = ["get", "is", "has", "can", "set", "add", "remove"];
const OPTIONS_THAT_REROUTE: [&str; 4] = ["mapped", "property_path", "data", "inherit_data"];

/// Names an entity class chain can answer to: `$variables` and method names.
struct Members {
    variables: BTreeSet<String>,
    methods: BTreeSet<String>,
    types: BTreeMap<String, String>,
}

impl Members {
    fn has(&self, name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        self.variables.contains(name) || ACCESSORS.iter().any(|prefix| self.methods.contains(&format!("{prefix}{lower}")))
    }

    fn has_property(&self, name: &str) -> bool {
        self.variables.contains(name)
    }
}

/// `None` when a parent cannot be read, a trait or magic accessor could add members.
fn members(ctx: &Context, fqcn: &str) -> Option<Members> {
    let mut found = Members { variables: BTreeSet::new(), methods: BTreeSet::new(), types: BTreeMap::new() };
    let mut next = Some(fqcn.to_owned());

    for _ in 0..6 {
        let Some(class) = next.take() else { return Some(found) };
        let entry = ctx.index.classes.get(&class).filter(|entry| !entry.path.is_empty())?;
        let text = ctx.source(&entry.path)?;
        let model = class_model(&text)?;
        if model.fqcn != class || model.has_traits {
            return None;
        }

        let masked = mask(&text);
        let body = &masked[model.body.clone()];
        for (at, _) in body.match_indices('$') {
            let name = body[read_word(body, at + 1)].to_owned();
            if !body[..at].ends_with("->") {
                if let Some(declared) = crate::doctrine::declared_type(body, at) {
                    found.types.entry(name.clone()).or_insert(declared);
                }
            }
            found.variables.insert(name);
        }
        for method in &model.methods {
            let name = method.name.to_ascii_lowercase();
            if MAGIC.iter().any(|magic| name == *magic) {
                return None;
            }
            found.methods.insert(name);
        }
        next = model.parent;
    }

    None
}

fn string_literal(text: &str, range: Range<usize>) -> Option<&str> {
    let value = text[trimmed(text, range)].trim();
    let inner = value.strip_prefix('\'').and_then(|rest| rest.strip_suffix('\'')).or_else(|| value.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')))?;
    (!inner.is_empty() && inner.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')).then_some(inner)
}

pub(crate) fn check(ctx: &Context, language: &str, text: &str) -> Vec<Finding> {
    if language != "php" || ctx.index.partial {
        return Vec::new();
    }

    let mut found = associations(ctx, text);
    found.extend(form_fields(ctx, text));
    found.sort_by_key(|finding| finding.start);
    found
}

pub(crate) struct Link {
    pub definition: bool,
    pub key: String,
    pub range: Range<usize>,
}

/// Go to Declaration edges: `class#FQCN` references from `repositoryClass` and
/// `targetEntity`, and `orm-property#FQCN::name` between `mappedBy`/`inversedBy`
/// strings and the mapped property declarations of the same file.
pub(crate) fn links(text: &str) -> Vec<Link> {
    let mapped = imports(text).iter().any(|import| import.fqcn == "Doctrine\\ORM\\Mapping" || import.fqcn.starts_with(MAPPING));
    if !mapped {
        return Vec::new();
    }

    let masked = mask(text);
    let resolve = resolver(text);
    let owner = class_model(text).map(|model| model.fqcn);
    let mut found = Vec::new();

    for attribute in attributes(text) {
        if let (Target::Property { name }, Some(owner)) = (&attribute.target, &owner) {
            let needle = format!("${name}");
            let declared = masked[attribute.range.end..].match_indices(&needle).map(|(at, _)| attribute.range.end + at).find(|at| !masked.as_bytes().get(at + needle.len()).is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_'));
            if let Some(at) = declared {
                found.push(Link { definition: true, key: format!("orm-property#{owner}::{name}"), range: at + 1..at + needle.len() });
            }
        }

        let Some(kind) = attribute.name.strip_prefix(MAPPING) else { continue };
        let is_relation = matches!(kind, "ManyToOne" | "OneToMany" | "OneToOne" | "ManyToMany");
        if !(is_relation || kind == "Entity") || !masked[attribute.range.clone()].contains('(') {
            continue;
        }

        let mut target = None;
        let mut sides = Vec::new();
        for (position, argument) in arguments(&masked, attribute.arguments.start - 1).into_iter().enumerate() {
            let (label, value) = match named(&masked, argument.clone()) {
                Some((name, value)) => (&masked[name], value),
                None if position == 0 && is_relation => ("targetEntity", argument),
                None => continue,
            };
            match label {
                "targetEntity" if is_relation => target = class_literal(text, value.clone()).map(|class| (resolve(&class), value)),
                "repositoryClass" if kind == "Entity" => {
                    if let Some(class) = class_literal(text, value.clone()) {
                        found.push(class_link(text, &resolve(&class), value));
                    }
                }
                "mappedBy" | "inversedBy" if is_relation => sides.push(value),
                _ => {}
            }
        }

        let Some((target, range)) = target else { continue };
        found.push(class_link(text, &target, range));
        for value in sides {
            if let Some(name) = string_literal(text, value.clone()) {
                found.push(Link { definition: false, key: format!("orm-property#{target}::{name}"), range: trimmed(text, value) });
            }
        }
    }

    found
}

fn class_link(text: &str, class: &str, value: Range<usize>) -> Link {
    let range = trimmed(text, value);
    let end = range.start + text[range.clone()].trim_end().len() - "::class".len();
    Link { definition: false, key: format!("class#{class}"), range: range.start..end }
}

fn associations(ctx: &Context, text: &str) -> Vec<Finding> {
    let mapped = imports(text).iter().any(|import| import.fqcn == "Doctrine\\ORM\\Mapping" || import.fqcn.starts_with(MAPPING));
    if !mapped {
        return Vec::new();
    }

    let masked = mask(text);
    let resolve = resolver(text);
    let mut found = Vec::new();

    for attribute in attributes(text) {
        let relation = attribute.name.strip_prefix(MAPPING).filter(|name| matches!(*name, "ManyToOne" | "OneToMany" | "OneToOne" | "ManyToMany"));
        if relation.is_none() || !matches!(attribute.target, Target::Property { .. }) || !masked[attribute.range.clone()].contains('(') {
            continue;
        }

        let list = arguments(&masked, attribute.arguments.start - 1);
        let mut target = None;
        let mut sides = Vec::new();
        for (position, argument) in list.into_iter().enumerate() {
            match named(&masked, argument.clone()) {
                Some((name, value)) => match &masked[name] {
                    "targetEntity" => target = class_literal(text, value),
                    side @ ("mappedBy" | "inversedBy") => sides.push((side, value)),
                    _ => {}
                },
                None if position == 0 => target = class_literal(text, argument),
                None => {}
            }
        }

        let Some(target) = target.map(|class| resolve(&class)) else { continue };
        let Some(known) = members(ctx, &target) else { continue };
        for (side, value) in sides {
            let Some(name) = string_literal(text, value.clone()) else { continue };
            if !known.has_property(name) {
                let short = target.rsplit('\\').next().unwrap_or(&target);
                found.push(Finding::warn(value, "orm-association-missing-property", format!("`{side}` names `{name}`, but {short} declares no property `${name}`.")));
            }
        }
    }

    found
}

fn class_literal(text: &str, range: Range<usize>) -> Option<String> {
    let value = text[trimmed(text, range)].trim();
    let class = value.strip_suffix("::class")?.trim_start_matches('\\');
    (!class.is_empty() && class.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'\\'))).then(|| class.to_owned())
}

const ENTITY_TYPE: &str = "Symfony\\Bridge\\Doctrine\\Form\\Type\\EntityType";
const COLLECTION_TYPE: &str = "Symfony\\Component\\Form\\Extension\\Core\\Type\\CollectionType";
const CHECKBOX_TYPE: &str = "Symfony\\Component\\Form\\Extension\\Core\\Type\\CheckboxType";
const CHECKBOX_DOCS: &str = "https://symfony.com/doc/current/reference/forms/types/checkbox.html";

/// `'key' => value` entries of a literal array argument.
fn option_entries(text: &str, masked: &str, range: Range<usize>) -> Vec<(String, Range<usize>)> {
    let range = trimmed(masked, range);
    if !masked[range.clone()].starts_with('[') {
        return Vec::new();
    }
    let mut entries = Vec::new();
    for entry in arguments(masked, range.start) {
        let Some(arrow) = masked[entry.clone()].find("=>") else { continue };
        let Some(key) = string_literal(text, entry.start..entry.start + arrow) else { continue };
        entries.push((key.to_owned(), trimmed(text, entry.start + arrow + 2..entry.end)));
    }
    entries
}

fn form_fields(ctx: &Context, text: &str) -> Vec<Finding> {
    let Some(model) = class_model(text) else { return Vec::new() };
    if model.parent.as_deref() != Some("Symfony\\Component\\Form\\AbstractType") {
        return Vec::new();
    }

    let masked = mask(text);
    let resolve = resolver(text);
    let entity = crate::generators::data_class(text, &masked, &model).filter(|_| !text.contains("inherit_data"));
    let known = entity.as_ref().and_then(|entity| members(ctx, entity));
    let Some(build) = model.method("buildForm") else { return Vec::new() };
    let Some(body) = build.body.clone() else { return Vec::new() };
    let Some(builder) = build.params.first().map(|param| format!("${}", param.name)) else { return Vec::new() };

    let mut found = Vec::new();
    for (offset, _) in masked[body.clone()].match_indices("->add(") {
        let at = body.start + offset;
        let statement = masked[..at].rfind([';', '{', '}']).map_or(0, |found| found + 1).max(body.start);
        if !chain_of_adds(&masked[statement..at], &builder) {
            continue;
        }

        let open = at + "->add".len();
        let list = arguments(&masked, open);
        let Some(first) = list.first().cloned() else { continue };
        let options_unread = list.len() > 2 && matching_close(&masked, open).is_none();
        let options = list.get(2).map(|range| option_entries(text, &masked, range.clone())).unwrap_or_default();
        let type_class = list.get(1).and_then(|range| class_literal(text, range.clone())).map(|class| resolve(&class));
        let option = |key: &str| options.iter().find(|(name, _)| name == key).map(|(_, value)| value.clone());

        if !options_unread {
            found.extend(type_option_findings(ctx, text, &resolve, type_class.as_deref(), &option));
        }

        let Some(name) = string_literal(text, first.clone()) else { continue };
        let Some(known) = known.as_ref() else { continue };
        let reroutes = list.get(2).is_some_and(|options| OPTIONS_THAT_REROUTE.iter().any(|option| text[options.clone()].contains(&format!("'{option}'")) || text[options.clone()].contains(&format!("\"{option}\""))));
        if reroutes || options_unread {
            continue;
        }
        if !known.has(name) {
            let short = entity.as_deref().and_then(|entity| entity.rsplit('\\').next()).unwrap_or_default();
            found.push(Finding::with_severity_info(trimmed(text, first), "form-field-without-property", format!("Field `{name}` maps to no property of {short}; set `'mapped' => false` if it is not stored on the entity.")));
            continue;
        }

        let declared = known.types.get(name).and_then(|declared| crate::doctrine::scalar_type(declared));
        if type_class.as_deref() == Some(CHECKBOX_TYPE) && declared.as_deref().is_some_and(|declared| declared != "bool") {
            found.push(Finding::with_severity_info(trimmed(text, first), "form-field-type-mismatch", format!("CheckboxType is for a boolean value, but property `{name}` is declared `{}` ({CHECKBOX_DOCS}).", declared.unwrap_or_default())));
        }
    }

    found
}

/// `class`/`choice_label` of EntityType and `entry_type` of CollectionType.
fn type_option_findings(ctx: &Context, text: &str, resolve: &dyn Fn(&str) -> String, type_class: Option<&str>, option: &dyn Fn(&str) -> Option<Range<usize>>) -> Vec<Finding> {
    let class_option = match type_class {
        Some(ENTITY_TYPE) => "class",
        Some(COLLECTION_TYPE) => "entry_type",
        _ => return Vec::new(),
    };
    let Some(value) = option(class_option) else { return Vec::new() };
    let absolute = text[value.clone()].starts_with('\\');
    let Some(class) = class_literal(text, value.clone()).map(|class| if absolute { class } else { resolve(&class) }) else { return Vec::new() };
    if crate::checks::class_is_missing(ctx, &class) {
        return vec![Finding::with_severity_info(value, "form-class-option-missing", format!("Class `{class}` of the `{class_option}` option does not exist in the project."))];
    }

    let (Some(ENTITY_TYPE), Some(label)) = (type_class, option("choice_label")) else { return Vec::new() };
    let Some(name) = string_literal(text, label.clone()) else { return Vec::new() };
    match members(ctx, &class) {
        Some(known) if !known.has(name) => {
            let short = class.rsplit('\\').next().unwrap_or_default();
            vec![Finding::with_severity_info(label, "form-choice-label-missing", format!("`choice_label` `{name}` is no property or getter of {short}."))]
        }
        _ => Vec::new(),
    }
}

/// `segment` is `$builder` followed only by `->add(...)` calls at depth 0.
fn chain_of_adds(segment: &str, builder: &str) -> bool {
    let segment = segment.trim_start();
    let Some(rest) = segment.strip_prefix(builder) else { return false };
    let mut depth = 0usize;
    for (at, byte) in rest.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'-' if depth == 0 && rest[at..].starts_with("->") && !rest[at..].starts_with("->add(") => return false,
            _ => {}
        }
    }
    true
}

impl Finding {
    pub(crate) fn with_severity_info(range: Range<usize>, code: &'static str, message: String) -> Finding {
        Finding { start: range.start, end: range.end, code, message, severity: Severity::Info, replacement: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{Entry, Index};
    use openidea_plugin::Position;
    use std::collections::BTreeMap;

    const USER: &str = "<?php\nnamespace App\\Entity;\n\nclass User\n{\n    private ?string $name = null;\n    private $email;\n    public function getAge(): int { return 1; }\n}\n";
    const FORM: &str = "<?php\nnamespace App\\Form;\n\nuse App\\Entity\\User;\nuse Symfony\\Component\\Form\\AbstractType;\nuse Symfony\\Component\\OptionsResolver\\OptionsResolver;\n\nclass UserType extends AbstractType\n{\n    public function buildForm(FormBuilderInterface $builder, array $options): void\n    {\n        BUILD\n    }\n\n    public function configureOptions(OptionsResolver $resolver): void\n    {\n        $resolver->setDefaults(['data_class' => User::class]);\n    }\n}\n";

    fn run(files: &[(&str, &str, &str)], text: &str) -> Vec<(String, String)> {
        let mut index = Index { framework_version: Some((7, 0)), ..Index::default() };
        let mut sources = BTreeMap::new();
        for (class, path, source) in files {
            index.classes.insert((*class).into(), Entry { detail: None, path: (*path).into(), at: Position::new(0, 0) });
            sources.insert((*path).to_owned(), (*source).to_owned());
        }
        let read = |path: &str| sources.get(path).cloned();
        let ctx = Context::new(&index, &read);
        check(&ctx, "php", text).into_iter().map(|finding| (finding.code.to_owned(), text[finding.start..finding.end].to_owned())).collect()
    }

    fn form(build: &str) -> String {
        FORM.replace("BUILD", build)
    }

    #[test]
    fn form_field_without_property_is_info() {
        let files = [("App\\Entity\\User", "/p/User.php", USER)];
        let text = form("$builder->add('name', TextType::class)->add('age')->add('nope', TextType::class);");
        assert_eq!(run(&files, &text), [("form-field-without-property".into(), "'nope'".into())]);
    }

    #[test]
    fn form_field_quiet_when_rerouted_or_unprovable() {
        let files = [("App\\Entity\\User", "/p/User.php", USER)];
        for build in ["$builder->add('x', TextType::class, ['mapped' => false]);", "$builder->add('x', TextType::class, ['property_path' => 'name']);", "$builder->get('name')->add('x');", "$builder->add($name);"] {
            assert!(run(&files, &form(build)).is_empty(), "{build}");
        }
        assert!(run(&[], &form("$builder->add('x');")).is_empty());
        let magic = USER.replace("private $email;", "public function __get($n) {}");
        assert!(run(&[("App\\Entity\\User", "/p/User.php", &magic)], &form("$builder->add('x');")).is_empty());
        let child = USER.replace("class User", "class User extends Base");
        assert!(run(&[("App\\Entity\\User", "/p/User.php", &child)], &form("$builder->add('x');")).is_empty());
    }

    const ORM: &str = "<?php\nnamespace App\\Entity;\n\nuse Doctrine\\ORM\\Mapping as ORM;\n\nclass Post\n{\n    BODY\n}\n";

    #[test]
    fn links_cover_repository_target_and_sides() {
        let text = "<?php\nnamespace App\\Entity;\n\nuse Doctrine\\ORM\\Mapping as ORM;\nuse App\\Repo\\PostRepository;\n\n#[ORM\\Entity(repositoryClass: PostRepository::class)]\nclass Post\n{\n    #[ORM\\ManyToOne(targetEntity: Author::class, inversedBy: 'posts')]\n    private $author;\n}\n";
        let at = |link: &Link| text[link.range.clone()].to_owned();
        let found: Vec<_> = links(text).iter().map(|link| (link.definition, link.key.clone(), at(link))).collect();
        assert!(found.contains(&(false, "class#App\\Repo\\PostRepository".into(), "PostRepository".into())));
        assert!(found.contains(&(false, "class#App\\Entity\\Author".into(), "Author".into())));
        assert!(found.contains(&(false, "orm-property#App\\Entity\\Author::posts".into(), "'posts'".into())));
        assert!(found.contains(&(true, "orm-property#App\\Entity\\Post::author".into(), "author".into())));
        assert!(links("<?php\nclass A { private $x; }").is_empty());
    }

    #[test]
    fn association_side_must_name_existing_property() {
        let author = "<?php\nnamespace App\\Entity;\nclass Author\n{\n    private $posts;\n}\n";
        let files = [("App\\Entity\\Author", "/p/Author.php", author)];
        let ok = ORM.replace("BODY", "#[ORM\\ManyToOne(targetEntity: Author::class, inversedBy: 'posts')]\n    private $author;");
        assert!(run(&files, &ok).is_empty());
        let bad = ORM.replace("BODY", "#[ORM\\ManyToOne(targetEntity: Author::class, inversedBy: 'post')]\n    private $author;");
        assert_eq!(run(&files, &bad), [("orm-association-missing-property".into(), "'post'".into())]);
        let unknown = ORM.replace("BODY", "#[ORM\\ManyToOne(targetEntity: Missing::class, inversedBy: 'post')]\n    private $author;");
        assert!(run(&files, &unknown).is_empty());
    }

    #[test]
    fn entity_type_class_and_choice_label_are_checked() {
        let author = "<?php\nnamespace App\\Entity;\nclass Author\n{\n    private string $name;\n    public function getTitle(): string { return ''; }\n}\n";
        let mut index = Index { framework_version: Some((7, 0)), psr4_prefixes: vec!["App\\".into()], ..Index::default() };
        index.classes.insert("App\\Entity\\Author".into(), Entry { detail: None, path: "/p/Author.php".into(), at: Position::new(0, 0) });
        let read = |path: &str| (path == "/p/Author.php").then(|| author.to_owned());
        let ctx = Context::new(&index, &read);
        let run = |build: &str| {
            let text = form(build).replace("use App\\Entity\\User;", "use App\\Entity\\Author;\nuse Symfony\\Bridge\\Doctrine\\Form\\Type\\EntityType;\nuse Symfony\\Component\\Form\\Extension\\Core\\Type\\CollectionType;\nuse Symfony\\Component\\Form\\Extension\\Core\\Type\\CheckboxType;").replace("'data_class' => User::class", "'data_class' => Author::class");
            check(&ctx, "php", &text).into_iter().map(|finding| (finding.code, text[finding.start..finding.end].to_owned())).collect::<Vec<_>>()
        };
        assert!(run("$builder->add('name', EntityType::class, ['class' => Author::class, 'choice_label' => 'name'])->add('x', EntityType::class, ['class' => Author::class, 'choice_label' => 'title', 'mapped' => false]);").is_empty());
        assert_eq!(run("$builder->add('x', EntityType::class, ['class' => Author::class, 'choice_label' => 'nope', 'mapped' => false]);"), [("form-choice-label-missing", "'nope'".into())]);
        assert_eq!(run("$builder->add('x', EntityType::class, ['class' => Gone::class, 'mapped' => false]);"), [("form-class-option-missing", "Gone::class".into())]);
        assert!(run("$builder->add('x', EntityType::class, ['class' => \\Vendor\\Thing::class, 'mapped' => false]);").is_empty());
        assert_eq!(run("$builder->add('x', CollectionType::class, ['entry_type' => Gone::class, 'mapped' => false]);"), [("form-class-option-missing", "Gone::class".into())]);
        assert!(run("$builder->add('x', ChoiceType::class, ['choices' => Gone::cases(), 'mapped' => false]);").is_empty());
        assert_eq!(run("$builder->add('name', CheckboxType::class);"), [("form-field-type-mismatch", "'name'".into())]);
    }
}
