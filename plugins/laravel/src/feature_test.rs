//! A feature test for the route that a controller method serves, created as a new file.
use crate::{Main, view_creation};
use openidea_plugin::{
    Error,
    file_actions::{FileAction, FileOperation},
    providers::Document,
    text::offset,
    Range,
};

const DATA_PREFIX: &str = "feature-test:";

pub(crate) fn actions(main: &Main, doc: &Document, selection: Range) -> Vec<FileAction> {
    let Some((_, route)) = route_at(main, doc, selection) else {
        return Vec::new();
    };

    vec![FileAction {
        title: crate::ui_text::format("featureTest.action", "Generate a feature test for {route}…", &[("route", &format!("{} {}", route.verb, route.uri))]),
        kind: Some("quickfix".into()),
        data: Some(format!("{DATA_PREFIX}{}", route.method)),
        ..Default::default()
    }]
}

pub(crate) fn resolve(main: &Main, doc: &Document, action: FileAction) -> Result<FileAction, Error> {
    let method = action.data.as_deref().and_then(|data| data.strip_prefix(DATA_PREFIX)).ok_or(Error::Stale)?;
    let position = openidea_plugin::text::position(&doc.text, doc.text.find(&format!("function {method}(")).ok_or(Error::Stale)?);
    let at = Range::new(position, position);
    let (root, mut route) = route_at(main, doc, at).ok_or(Error::Stale)?;
    let (project, _) = view_creation::owner(doc, Some(&root))?;
    route.fields = form_request_fields(&project, &doc.text, &route.method);

    let pest = project.read_file("composer.json").is_ok_and(|composer| composer.contains("pestphp/pest"));
    let target = format!("{root}/tests/Feature/{}{}Test.php", route.controller, capitalized(&route.method));
    let text = content(&route, pest);

    let preview = crate::ui_text::format("featureTest.review", "Create (no overwrite): {target}\n\nExact new contents:\n{contents}\nOne undoable file creation. No PHP or Artisan is executed.", &[("target", &target), ("contents", &text)]);
    if !ui_kit::prompts::confirm_exact(crate::ui_text::text("featureTest.confirm", "Create feature test?"), &preview)? {
        return Err(Error::Cancelled);
    }

    Ok(FileAction {
        title: crate::ui_text::format("featureTest.applied", "Create feature test for {route}", &[("route", &format!("{} {}", route.verb, route.uri))]),
        operations: vec![FileOperation::Create { path: target.clone(), text, overwrite: false }],
        open: Some(target),
        ..Default::default()
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RouteTarget {
    controller: String,
    method: String,
    verb: String,
    uri: String,
    fields: Vec<(String, String)>,
}

/// The project root and the first route that serves the controller method the selection is in.
fn route_at(main: &Main, doc: &Document, selection: Range) -> Option<(String, RouteTarget)> {
    if doc.language != "php" || !doc.path.contains("/app/Http/Controllers/") {
        return None;
    }

    let (_, root) = view_creation::owner(doc, None).ok()?;
    let index = main.indexes.get(&root).filter(|index| !index.partial)?;
    let at = offset(&doc.text, selection.start)?;
    let method = method_at(&doc.text, at)?;
    let controller = doc.path.rsplit('/').next()?.strip_suffix(".php")?.to_owned();

    let entry = index.route_defs.iter().find(|entry| entry.def.controller.as_ref().is_some_and(|(class, name)| *class == controller && *name == method))?;
    let verb = entry.def.methods.split('|').next()?.to_ascii_uppercase();

    Some((root, RouteTarget { controller, method, verb, uri: normalized_uri(&entry.def.uri), fields: Vec::new() }))
}

/// The name of the method whose `function name(` line the offset is on.
fn method_at(text: &str, at: usize) -> Option<String> {
    let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[at..].find('\n').map_or(text.len(), |i| at + i);
    let line = &text[line_start..line_end];
    let name = line.split_once("function ")?.1.split('(').next()?.trim();

    (!name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')).then(|| name.to_owned())
}

fn normalized_uri(uri: &str) -> String {
    let mut out = String::new();
    let mut in_parameter = false;

    for c in uri.chars() {
        match c {
            '{' => {
                in_parameter = true;
                out.push('1');
            }
            '}' => in_parameter = false,
            _ if !in_parameter => out.push(c),
            _ => {}
        }
    }

    if out.starts_with('/') { out } else { format!("/{out}") }
}

/// Request fields and a valid-looking value each, from the `rules()` of the method's Form Request.
pub(crate) fn form_request_fields(project: &openidea_plugin::Project, controller: &str, method: &str) -> Vec<(String, String)> {
    let Some(class) = form_request_class(controller, method) else {
        return Vec::new();
    };
    let Ok(paths) = project.find_files(&format!("**/{class}.php"), 5) else {
        return Vec::new();
    };
    let Some(source) = paths.iter().find_map(|path| project.read_file(path).ok()) else {
        return Vec::new();
    };

    rule_fields(&source)
}

/// The Form Request type of `method`'s parameter named `$request` (or any parameter), when it is not the plain `Request`.
fn form_request_class(controller: &str, method: &str) -> Option<String> {
    let at = controller.find(&format!("function {method}("))?;
    let rest = &controller[at..];
    let signature = &rest[..rest.find(')')?];

    signature.split(',').find_map(|parameter| {
        let class = parameter.split('(').next_back()?.split_whitespace().next()?;
        let short = class.rsplit('\\').next()?;
        (short.len() > "Request".len() && short.ends_with("Request")).then(|| short.to_owned())
    })
}

pub(crate) fn rule_fields(source: &str) -> Vec<(String, String)> {
    let masked = crate::php::mask_comments(source);
    let Some(rules) = masked.find("function rules(") else {
        return Vec::new();
    };
    let Some(open) = masked[rules..].find("return").and_then(|r| masked[rules + r..].find('[').map(|b| rules + r + b)) else {
        return Vec::new();
    };
    let Some(close) = crate::php::closing(&masked, open) else {
        return Vec::new();
    };

    let bytes = masked.as_bytes();
    let mut fields = Vec::new();
    let mut depth = 0i32;
    let mut at = open;

    while at < close {
        match bytes[at] {
            b'[' | b'(' | b'{' => depth += 1,
            b']' | b')' | b'}' => depth -= 1,
            b'\'' | b'"' if depth == 1 => {
                let end = crate::php::string_end(bytes, at);
                let after = masked[end..close].trim_start();
                if after.starts_with("=>") {
                    let key = masked[at + 1..end - 1].to_owned();
                    let value_end = value_end(&masked, end + (masked[end..close].len() - after.len()) + 2, close);
                    let rule = &masked[end..value_end];
                    if !key.contains('*') && !key.contains('.') {
                        fields.push((key, sample_value(rule)));
                    }
                }
                at = end;
                continue;
            }
            b'\'' | b'"' => {
                at = crate::php::string_end(bytes, at);
                continue;
            }
            _ => {}
        }
        at += 1;
    }

    fields
}

/// The end of the rule value that starts at `from`: the next comma outside brackets.
fn value_end(text: &str, from: usize, close: usize) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut at = from;

    while at < close {
        match bytes[at] {
            b'\'' | b'"' => {
                at = crate::php::string_end(bytes, at);
                continue;
            }
            b'[' | b'(' | b'{' => depth += 1,
            b']' | b')' | b'}' => depth -= 1,
            b',' if depth == 0 => return at,
            _ => {}
        }
        at += 1;
    }

    close
}

fn sample_value(rule: &str) -> String {
    let has = |word: &str| rule.contains(word);
    match () {
        _ if has("email") => "'user@example.com'".into(),
        _ if has("boolean") => "true".into(),
        _ if has("integer") || has("numeric") => "1".into(),
        _ if has("array") => "[]".into(),
        _ if has("date") => "'2030-01-01'".into(),
        _ if has("url") => "'https://example.com'".into(),
        _ => "'text'".into(),
    }
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

fn content(route: &RouteTarget, pest: bool) -> String {
    let body = if route.fields.is_empty() {
        "[]".to_owned()
    } else {
        let lines: String = route.fields.iter().map(|(key, value)| format!("            '{key}' => {value},\n")).collect();
        format!("[\n{lines}        ]")
    };
    let call = match route.verb.as_str() {
        "POST" | "PUT" | "PATCH" => format!("$this->{}('{}', {body})", route.verb.to_lowercase(), route.uri),
        "DELETE" => format!("$this->delete('{}')", route.uri),
        _ => format!("$this->get('{}')", route.uri),
    };

    if pest {
        return format!("<?php\n\ntest('{} {}', function () {{\n    $response = {call};\n\n    $response->assertStatus(200);\n}});\n", route.verb, route.uri);
    }

    format!(
        "<?php\n\nnamespace Tests\\Feature;\n\nuse Tests\\TestCase;\n\nclass {}{}Test extends TestCase\n{{\n    public function test_{}(): void\n    {{\n        $response = {call};\n\n        $response->assertStatus(200);\n    }}\n}}\n",
        route.controller,
        capitalized(&route.method),
        route.method
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route() -> RouteTarget {
        RouteTarget { controller: "ProductsController".into(), method: "store".into(), verb: "POST".into(), uri: "/products".into(), fields: Vec::new() }
    }

    #[test]
    fn parameters_become_ids_and_slashes_are_normalized() {
        assert_eq!(normalized_uri("products/{product}/edit"), "/products/1/edit");
        assert_eq!(normalized_uri("/"), "/");
    }

    #[test]
    fn method_is_read_from_the_function_line() {
        assert_eq!(method_at("class A {\n    public function store(Request $r)\n", 20).as_deref(), Some("store"));
        assert_eq!(method_at("$x = 1;", 2), None);
    }

    #[test]
    fn pest_and_phpunit_bodies_post_to_the_route() {
        assert!(content(&route(), true).contains("$this->post('/products', [])"));
        let phpunit = content(&route(), false);
        assert!(phpunit.contains("class ProductsControllerStoreTest extends TestCase"));
        assert!(phpunit.contains("test_store"));
    }

    #[test]
    fn form_request_rules_become_the_request_body() {
        let source = "<?php class StoreUserRequest { public function rules(): array { return [ 'email' => ['required', 'email'], 'age' => 'integer|min:1', 'tags.*' => 'string', 'name' => 'required' ]; } }";
        assert_eq!(
            rule_fields(source),
            [("email".to_owned(), "'user@example.com'".to_owned()), ("age".to_owned(), "1".to_owned()), ("name".to_owned(), "'text'".to_owned())]
        );
        assert_eq!(form_request_class("public function store(StoreUserRequest $request)", "store").as_deref(), Some("StoreUserRequest"));
        assert_eq!(form_request_class("public function store(Request $request)", "store"), None);
    }
}
