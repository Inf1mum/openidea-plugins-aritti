//! Cross-file checks of Doctrine association names and form fields, from
//! literal source only. Doctrine ORM attributes reference: `mappedBy` and
//! `inversedBy` name a property on the `targetEntity`. Symfony Forms: a field
//! reads and writes the same-named property of the form's `data_class` unless
//! `mapped` is false, or `property_path`, `data` or `inherit_data` change it.
//! Every check stays quiet unless the whole class chain is readable.

use std::collections::BTreeSet;
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
    let mut found = Members { variables: BTreeSet::new(), methods: BTreeSet::new() };
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
            found.variables.insert(body[read_word(body, at + 1)].to_owned());
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

fn form_fields(ctx: &Context, text: &str) -> Vec<Finding> {
    let Some(model) = class_model(text) else { return Vec::new() };
    if model.parent.as_deref() != Some("Symfony\\Component\\Form\\AbstractType") || text.contains("inherit_data") {
        return Vec::new();
    }

    let masked = mask(text);
    let Some(entity) = crate::generators::data_class(text, &masked, &model) else { return Vec::new() };
    let Some(known) = members(ctx, &entity) else { return Vec::new() };
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
        let Some(name) = string_literal(text, first.clone()) else { continue };
        let reroutes = list.get(2).is_some_and(|options| OPTIONS_THAT_REROUTE.iter().any(|option| text[options.clone()].contains(&format!("'{option}'")) || text[options.clone()].contains(&format!("\"{option}\""))));
        let options_unread = list.len() > 2 && matching_close(&masked, open).is_none();
        if reroutes || options_unread || known.has(name) {
            continue;
        }

        let short = entity.rsplit('\\').next().unwrap_or(&entity);
        found.push(Finding::with_severity_info(trimmed(text, first), "form-field-without-property", format!("Field `{name}` maps to no property of {short}; set `'mapped' => false` if it is not stored on the entity.")));
    }

    found
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
    fn with_severity_info(range: Range<usize>, code: &'static str, message: String) -> Finding {
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
}
