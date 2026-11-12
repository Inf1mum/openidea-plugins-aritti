//! Bounded, source-only PHP type inference for Twig variables: what a controller passes to
//! `render()` and the members Twig can read from it. Matching is textual (a call is
//! recognised by its shape, not its resolved class); anything uncertain yields no type.
use std::collections::BTreeMap;

use crate::php_text::{self, ClassModel, Visibility, BUILT_IN_TYPES};

const FINDERS: [&str; 3] = ["find", "findOneBy", "findOneById"];
const MAX_PARENTS: usize = 8;

/// What a value is, before project sources are read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeHint {
    Class(String),
    /// The entity a repository class manages.
    EntityOf(String),
}

/// A member Twig can read with `object.name`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub detail: String,
    /// A class (resolved through `use`) or lower-case built-in.
    pub type_name: Option<String>,
}

/// The source of a class by its fully qualified name.
pub type Loader<'a> = &'a dyn Fn(&str) -> Option<String>;

struct Scope<'a> {
    known: BTreeMap<String, TypeHint>,
    properties: BTreeMap<String, String>,
    resolve: &'a dyn Fn(&str) -> String,
}

fn is_class_type(name: &str) -> bool {
    !BUILT_IN_TYPES.contains(&name.to_ascii_lowercase().as_str())
}

fn identifier_end(text: &str) -> usize {
    text.bytes().position(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'\\'))).unwrap_or(text.len())
}

fn scope_at<'a>(model: &ClassModel, masked: &str, resolve: &'a dyn Fn(&str) -> String, at: usize) -> Scope<'a> {
    let mut scope = Scope {
        known: BTreeMap::new(),
        properties: model.properties.iter().filter(|property| !property.is_static).filter_map(|property| Some((property.name.clone(), property.type_name.clone().filter(|name| is_class_type(name))?))).collect(),
        resolve,
    };

    if let Some(method) = model.methods.iter().find(|method| method.body.as_ref().is_some_and(|body| body.contains(&at))) {
        for param in &method.params {
            if let Some(name) = param.type_name.clone().filter(|name| is_class_type(name)) {
                scope.known.insert(param.name.clone(), TypeHint::Class(name));
            }
        }

        let body = method.body.clone().unwrap_or(at..at);
        for statement in masked[body.start..at.max(body.start)].split(';') {
            let Some((name, rhs)) = assignment(statement) else { continue };
            match expression_hint(rhs, &scope) {
                Some(hint) => scope.known.insert(name.to_owned(), hint),
                None => scope.known.remove(name),
            };
        }
    }
    scope
}

/// Type hints of the values of a `render()` call's literal array, from the method holding the call at `at`.
pub fn render_variable_types(text: &str, at: usize, entries: &[(String, String)]) -> BTreeMap<String, TypeHint> {
    let mut types = BTreeMap::new();
    let Some(model) = php_text::class_model(text) else { return types };
    let masked = php_text::mask(text);
    let resolve = php_text::resolver(text);
    let scope = scope_at(&model, &masked, &resolve, at);

    for (key, expression) in entries {
        if let Some(hint) = expression_hint(&php_text::mask(expression), &scope) {
            types.insert(key.clone(), hint);
        }
    }
    types
}

/// The type of a `$variable` or `$this->property` receiver at `at` in the method holding it.
pub fn receiver_hint_at(text: &str, at: usize, receiver: &str) -> Option<TypeHint> {
    let model = php_text::class_model(text)?;
    let masked = php_text::mask(text);
    let resolve = php_text::resolver(text);
    receiver_hint(receiver, &scope_at(&model, &masked, &resolve, at))
}

/// `$receiver->partial` at `at` in masked PHP: the receiver (`$var` or `$this->property`) and the word being typed.
pub fn member_access_at(masked: &str, at: usize) -> Option<(String, usize, usize)> {
    let bytes = masked.as_bytes();
    let mut start = at.min(bytes.len());
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    let mut end = at.min(bytes.len());
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }

    let before = masked[..start].strip_suffix("?->").or_else(|| masked[..start].strip_suffix("->"))?;
    let length = before.bytes().rev().take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$' | b'>' | b'-')).count();
    let receiver = &before[before.len() - length..];
    let valid = receiver.starts_with('$') && receiver.matches('$').count() == 1 && (!receiver.contains("->") || receiver.starts_with("$this->") && receiver.matches("->").count() == 1);
    valid.then(|| (receiver.to_owned(), start, end))
}

/// Every instance member reachable as `$object->name`: public properties and public methods.
pub fn instance_members(load: Loader, fqcn: &str) -> Vec<Member> {
    let mut found: BTreeMap<String, Member> = BTreeMap::new();
    for model in chain(load, fqcn).iter().rev() {
        let own = |name: &Option<String>| match name.as_deref() {
            Some("self" | "static") => Some(model.fqcn.clone()),
            other => other.map(str::to_owned),
        };
        for property in model.properties.iter().filter(|property| property.visibility == Visibility::Public && !property.is_static) {
            found.insert(property.name.clone(), Member { name: property.name.clone(), detail: format!("${}", property.name), type_name: own(&property.type_name) });
        }
        for method in model.methods.iter().filter(|method| method.visibility == Visibility::Public && !method.is_static && !method.name.starts_with("__")) {
            found.insert(method.name.clone(), Member { name: method.name.clone(), detail: format!("{}()", method.name), type_name: own(&method.return_class) });
        }
    }
    found.into_values().collect()
}

/// `$name = rhs` (not `==`, not a compound assignment) of one statement.
fn assignment(statement: &str) -> Option<(&str, &str)> {
    let statement = statement.trim().rsplit(['{', '}']).next()?.trim();
    let rest = statement.strip_prefix('$')?;
    let end = identifier_end(rest);
    let (name, after) = rest.split_at(end);
    let after = after.trim_start();
    let rhs = after.strip_prefix('=').filter(|rhs| !rhs.starts_with(['=', '>']))?;
    (!name.is_empty()).then_some((name, rhs.trim()))
}

fn receiver_hint(receiver: &str, scope: &Scope) -> Option<TypeHint> {
    let receiver = receiver.trim();
    if let Some(property) = receiver.strip_prefix("$this->") {
        return scope.properties.get(property).map(|name| TypeHint::Class(name.clone()));
    }
    scope.known.get(receiver.strip_prefix('$')?).cloned()
}

fn class_constant(argument: &str, scope: &Scope) -> Option<String> {
    let name = argument.trim().strip_suffix("::class")?;
    (!name.is_empty() && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'\\'))).then(|| (scope.resolve)(name))
}

fn expression_hint(expression: &str, scope: &Scope) -> Option<TypeHint> {
    let expression = expression.split("??").next()?.trim().trim_start_matches('(').trim_end_matches(')').trim();

    if let Some(rest) = expression.strip_prefix("new ") {
        let rest = rest.trim_start();
        let name = &rest[..identifier_end(rest)];
        return (!name.is_empty() && !matches!(name, "static" | "self" | "parent" | "class")).then(|| TypeHint::Class((scope.resolve)(name)));
    }

    if let Some(variable) = expression.strip_prefix('$') {
        if variable.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
            return scope.known.get(variable).cloned();
        }
    }

    // `$container->get(X::class)`: the service id is its class.
    if let Some((receiver, rest)) = expression.split_once("->get(") {
        let close = rest.find(')').unwrap_or(rest.len());
        let service = class_constant(&rest[..close], scope)?;
        let container = receiver.to_ascii_lowercase().contains("container")
            || matches!(receiver_hint(receiver, scope), Some(TypeHint::Class(name)) if name.ends_with("ContainerInterface") || name.ends_with("ServiceLocator"));
        return (container && rest.get(close + 1..).unwrap_or_default().trim().is_empty()).then_some(TypeHint::Class(service));
    }

    // `<receiver>->getRepository(X::class)->find…(` and `<receiver>->find(X::class, …)`.
    if let Some((before, rest)) = expression.split_once("->getRepository(") {
        let close = rest.find(')')?;
        let entity = class_constant(&rest[..close], scope)?;
        let call = rest[close + 1..].trim_start().strip_prefix("->")?;
        let name = &call[..identifier_end(call)];
        return (receiver_hint(before, scope).is_some() && FINDERS.contains(&name)).then_some(TypeHint::Class(entity));
    }

    let (at, name) = FINDERS.iter().filter_map(|name| Some((expression.find(&format!("->{name}("))?, *name))).min_by_key(|(at, _)| *at)?;
    let receiver = &expression[..at];
    let arguments = &expression[at + "->".len() + name.len() + 1..];
    let first = arguments.split([',', ')']).next().unwrap_or_default();
    if let Some(entity) = class_constant(first, scope) {
        return receiver_hint(receiver, scope).map(|_| TypeHint::Class(entity));
    }
    match receiver_hint(receiver, scope)? {
        TypeHint::Class(repository) => Some(TypeHint::EntityOf(repository)),
        TypeHint::EntityOf(_) => None,
    }
}

/// The entity class a repository manages: `@extends ServiceEntityRepository<Post>` or `parent::__construct($registry, Post::class)`.
fn repository_entity(source: &str) -> Option<String> {
    let resolve = php_text::resolver(source);
    if let Some(at) = source.find("@extends") {
        let rest = &source[at..];
        if let Some(open) = rest.find('<').filter(|open| rest[..*open].lines().count() == 1) {
            let inner = &rest[open + 1..];
            let name = inner[..inner.find(['>', ',']).unwrap_or(inner.len())].trim();
            if !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'\\')) {
                return Some(resolve(name));
            }
        }
    }

    let call = &source[source.find("parent::__construct(")? + "parent::__construct(".len()..];
    let arguments = &call[..call.find(')')?];
    arguments.split(',').find_map(|argument| {
        let name = argument.trim().strip_suffix("::class")?;
        (!name.is_empty() && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'\\'))).then(|| resolve(name))
    })
}

/// The class a hint stands for, reading repository sources when needed.
pub fn hint_class(hint: &TypeHint, load: Loader) -> Option<String> {
    match hint {
        TypeHint::Class(name) => Some(name.clone()),
        TypeHint::EntityOf(repository) => repository_entity(&load(repository)?),
    }
}

fn chain(load: Loader, fqcn: &str) -> Vec<ClassModel> {
    let mut models = Vec::new();
    let mut next = Some(fqcn.trim_start_matches('\\').to_owned());
    while let Some(name) = next.take() {
        if models.len() > MAX_PARENTS {
            break;
        }
        let Some(model) = load(&name).and_then(|source| php_text::class_model(&source)) else { break };
        next = model.parent.clone().map(|parent| parent.trim_start_matches('\\').to_owned());
        models.push(model);
    }
    models
}

fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|first| first.to_lowercase().chain(chars).collect()).unwrap_or_default()
}

/// What `object.name` can read on instances of `fqcn`: public properties and no-argument public
/// methods, with `getX`/`isX`/`hasX`/`canX` also reachable as `x`. Parents come from project sources.
pub fn members(load: Loader, fqcn: &str) -> Vec<Member> {
    let mut found: BTreeMap<String, Member> = BTreeMap::new();
    let models = chain(load, fqcn);

    for model in models.iter().rev() {
        let own = |name: &Option<String>| match name.as_deref() {
            Some("self" | "static") => Some(model.fqcn.clone()),
            other => other.map(str::to_owned),
        };

        for property in model.properties.iter().filter(|property| property.visibility == Visibility::Public && !property.is_static) {
            found.insert(property.name.clone(), Member { name: property.name.clone(), detail: format!("${}", property.name), type_name: own(&property.type_name) });
        }

        for method in model.methods.iter().filter(|method| method.visibility == Visibility::Public && !method.is_static && !method.name.starts_with("__")) {
            if method.params.iter().any(|param| !param.has_default && !param.variadic) {
                continue;
            }
            let type_name = own(&method.return_class);
            let accessor = ["get", "is", "has", "can"].iter().find_map(|prefix| {
                let rest = method.name.strip_prefix(prefix)?;
                rest.chars().next().filter(|first| first.is_uppercase())?;
                Some(lower_first(rest))
            });
            let name = accessor.unwrap_or_else(|| method.name.clone());
            found.insert(name.clone(), Member { name, detail: format!("{}()", method.name), type_name });
        }
    }

    found.into_values().collect()
}

/// The class of `object.name`, when it is a class.
pub fn member_class(load: Loader, fqcn: &str, name: &str) -> Option<String> {
    members(load, fqcn).into_iter().find(|member| member.name == name)?.type_name.filter(|name| is_class_type(name))
}

/// `{# @var name Type #}` comments before `offset`: variable name to class.
pub fn var_comment_types(text: &str, offset: usize) -> BTreeMap<String, String> {
    let mut types = BTreeMap::new();
    for (at, _) in text[..offset.min(text.len())].match_indices("{#") {
        let Some(end) = text[at..].find("#}") else { continue };
        let body = text[at + 2..at + end].trim();
        let Some(rest) = body.strip_prefix("@var") else { continue };
        let words: Vec<&str> = rest.split_whitespace().collect();
        let [first, second, ..] = words[..] else { continue };
        let (name, class) = match (first.starts_with('$') || !first.contains('\\') && second.contains('\\') || first.chars().next().is_some_and(char::is_lowercase), second) {
            (true, class) => (first.trim_start_matches('$'), class),
            (false, name) => (second.trim_start_matches('$'), first),
        };
        let class = class.trim_start_matches('\\').trim_end_matches("[]");
        if is_class_type(class) && !class.is_empty() && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
            types.insert(name.to_owned(), class.to_owned());
        }
    }
    types
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(text: &str, entries: &[(&str, &str)]) -> BTreeMap<String, TypeHint> {
        let at = text.find("$this->render(").unwrap();
        render_variable_types(text, at, &entries.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect::<Vec<_>>())
    }

    const CONTROLLER: &str = "<?php\nnamespace App\\Controller;\nuse App\\Entity\\Post;\nuse App\\Repository\\PostRepository;\nuse Doctrine\\ORM\\EntityManagerInterface;\nclass BlogController\n{\n    public function __construct(private PostRepository $posts) {}\n    public function show(Post $typed, PostRepository $repo, EntityManagerInterface $em, int $id)\n    {\n        $fresh = new Post();\n        $found = $repo->find($id);\n        $via = $this->posts->findOneBy(['id' => $id]);\n        $direct = $em->find(Post::class, $id);\n        $chained = $em->getRepository(Post::class)->findOneBy([]) ?? throw new \\Exception();\n        $number = 5;\n        return $this->render('blog/show.html.twig', []);\n    }\n}\n";

    #[test]
    fn variables_are_typed_from_parameters_constructions_and_finders() {
        let found = types(CONTROLLER, &[("typed", "$typed"), ("fresh", "$fresh"), ("found", "$found"), ("via", "$via"), ("direct", "$direct"), ("chained", "$chained"), ("number", "$number"), ("literal", "'x'"), ("inline", "new Post()")]);
        let class = |name: &str| TypeHint::Class(name.into());
        assert_eq!(found["typed"], class("App\\Entity\\Post"));
        assert_eq!(found["fresh"], class("App\\Entity\\Post"));
        assert_eq!(found["found"], TypeHint::EntityOf("App\\Repository\\PostRepository".into()));
        assert_eq!(found["via"], TypeHint::EntityOf("App\\Repository\\PostRepository".into()));
        assert_eq!(found["direct"], class("App\\Entity\\Post"));
        assert_eq!(found["chained"], class("App\\Entity\\Post"));
        assert_eq!(found["inline"], class("App\\Entity\\Post"));
        assert!(!found.contains_key("number") && !found.contains_key("literal"));
    }

    #[test]
    fn a_reassigned_variable_loses_its_type_when_the_new_value_is_unknown() {
        let text = "<?php\nuse App\\Entity\\Post;\nclass C { public function a() { $x = new Post(); $x = compute(); return $this->render('t', []); } }\n";
        assert!(types(text, &[("x", "$x")]).is_empty());
    }

    #[test]
    fn repository_entities_come_from_the_extends_docblock_or_the_constructor() {
        let docblock = "<?php\nnamespace App\\Repository;\nuse App\\Entity\\Post;\n/**\n * @extends ServiceEntityRepository<Post>\n */\nclass PostRepository extends ServiceEntityRepository {}\n";
        assert_eq!(repository_entity(docblock).as_deref(), Some("App\\Entity\\Post"));
        let constructor = "<?php\nnamespace App\\Repository;\nuse App\\Entity\\Tag;\nclass TagRepository extends ServiceEntityRepository\n{\n    public function __construct(ManagerRegistry $registry)\n    {\n        parent::__construct($registry, Tag::class);\n    }\n}\n";
        assert_eq!(repository_entity(constructor).as_deref(), Some("App\\Entity\\Tag"));
        assert_eq!(repository_entity("<?php\nclass R {}\n"), None);
    }

    #[test]
    fn members_follow_twig_attribute_rules_across_project_parents() {
        let sources: BTreeMap<&str, &str> = BTreeMap::from([
            ("App\\Entity\\Base", "<?php\nnamespace App\\Entity;\nabstract class Base\n{\n    public function getId(): ?int { return 1; }\n    protected function hidden(): string { return ''; }\n}\n"),
            ("App\\Entity\\Post", "<?php\nnamespace App\\Entity;\nuse App\\Entity\\User;\nclass Post extends Base\n{\n    public string $slug = '';\n    private string $secret = '';\n    public function getAuthor(): ?User { return null; }\n    public function isPublished(): bool { return true; }\n    public function summary(int $length = 10): string { return ''; }\n    public function withArgument(int $required): string { return ''; }\n    public static function make(): self { return new self(); }\n}\n"),
        ]);
        let load = |name: &str| sources.get(name).map(|source| (*source).to_owned());
        let found: BTreeMap<_, _> = members(&load, "App\\Entity\\Post").into_iter().map(|member| (member.name.clone(), member)).collect();
        let names: Vec<_> = found.keys().map(String::as_str).collect();
        assert_eq!(names, ["author", "id", "published", "slug", "summary"]);
        assert_eq!(found["author"].type_name.as_deref(), Some("App\\Entity\\User"));
        assert_eq!(found["author"].detail, "getAuthor()");
        assert_eq!(member_class(&load, "App\\Entity\\Post", "author").as_deref(), Some("App\\Entity\\User"));
        assert_eq!(member_class(&load, "App\\Entity\\Post", "published"), None);
    }

    #[test]
    fn var_comments_name_variables_in_either_order() {
        let text = "{# @var post \\App\\Entity\\Post #}\n{# @var \\App\\Entity\\User author #}\n{# @var $count int #}\n{# plain comment #}\n{{ post. }}";
        let found = var_comment_types(text, text.len());
        assert_eq!(found["post"], "App\\Entity\\Post");
        assert_eq!(found["author"], "App\\Entity\\User");
        assert!(!found.contains_key("count"));
    }

    #[test]
    fn container_get_with_a_class_constant_is_that_class() {
        let text = "<?php\nnamespace App;\nuse App\\Service\\Mailer;\nuse Psr\\Container\\ContainerInterface;\nclass C\n{\n    public function __construct(private ContainerInterface $locator) {}\n    public function a(ContainerInterface $box)\n    {\n        $one = $box->get(Mailer::class);\n        $two = $this->container->get(Mailer::class);\n        $three = $this->locator->get(Mailer::class);\n        $four = $box->get('app.mailer');\n        return $this->render('t', []);\n    }\n}\n";
        let at = text.find("$this->render(").unwrap();
        let found = render_variable_types(text, at, &[("one".into(), "$one".into()), ("two".into(), "$two".into()), ("three".into(), "$three".into()), ("four".into(), "$four".into())]);
        let mailer = TypeHint::Class("App\\Service\\Mailer".into());
        assert_eq!(found.get("one"), Some(&mailer));
        assert_eq!(found.get("two"), Some(&mailer));
        assert_eq!(found.get("three"), Some(&mailer));
        assert_eq!(found.get("four"), None);
    }

    #[test]
    fn member_access_finds_the_receiver_and_the_partial_word() {
        let masked = "$post->ti";
        assert_eq!(member_access_at(masked, masked.len()), Some(("$post".into(), 7, 9)));
        let masked = "$this->posts->";
        assert_eq!(member_access_at(masked, masked.len()), Some(("$this->posts".into(), 14, 14)));
        assert_eq!(member_access_at("$a->b->", 7), None);
        assert_eq!(member_access_at("foo()->x", 8), None);
    }
}
