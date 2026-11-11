//! Dusk selector shortcuts retain their explicit page/component receiver scope.
use super::*;
fn selector_names(source: &str, path: &str) -> Names {
    let mut names = Names::new();
    let Some(code) = crate::package_facts::code(source) else {
        return names;
    };
    let methods: Vec<_> = code.match_indices("function elements").collect();
    if methods.len() != 1 {
        return names;
    }
    let at = methods[0].0;
    let Some(open) = code[at..].find('{').map(|n| at + n) else {
        return names;
    };
    let Some(close) = php::closing(source, open) else {
        return names;
    };
    let body = source[open + 1..close].trim();
    let Some(returned) = body
        .strip_prefix("return")
        .and_then(|s| s.trim().strip_suffix(';'))
        .map(str::trim)
    else {
        return names;
    };
    let start = open + 1 + source[open + 1..close].find(returned).unwrap_or(0);
    let Some(array) = php::array_open(source, start..start + returned.len()) else {
        return names;
    };
    if php::closing(source, array) != Some(start + returned.len() - 1) {
        return names;
    }
    for item in php::items(source, array) {
        let Some((name, at)) = item.key.and_then(|r| literal(source, r)) else {
            continue;
        };
        if !name.starts_with('@') || name.len() < 2 || php::literal(source, item.value).is_none() {
            continue;
        }
        names
            .entry(name.into())
            .or_insert_with(|| entry(path, source, at, name, "Dusk scoped selector"));
    }
    names
}
pub(super) fn apply(
    read: &mut Read<'_>,
    index: &mut Index,
    roots: &[ComposerRoot],
    facts: &mut Facts,
) {
    if !enabled(index, "laravel/dusk") {
        return;
    }
    for (folder, parent, component) in [
        ("tests/Browser/Pages", "Laravel\\Dusk\\Page", false),
        ("tests/Browser/Components", "Laravel\\Dusk\\Component", true),
    ] {
        index.dependency_roots.insert(folder.into());
        for path in read.find(&format!("{folder}/**/*.php")) {
            let Some(source) = read.read(&path) else {
                continue;
            };
            let Some(full) = crate::eloquent_members::declaration(&source) else {
                continue;
            };
            let Some(code) = crate::package_facts::code(&source) else {
                continue;
            };
            let Some(at) = code.find("extends ") else {
                continue;
            };
            let spelling = code[at + 8..]
                .trim_start()
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '\\')
                .next()
                .unwrap_or("");
            if crate::eloquent_members::metadata_inference::class_identity(&source, spelling)
                .as_deref()
                != Some(parent)
            {
                continue;
            }
            let Some((verified, _)) = source_for(read, roots, &full, Some("project")) else {
                continue;
            };
            let path = format!("{}/{verified}", read.root);
            let e = Entry {
                path: path.clone(),
                at: Position::new(0, 0),
                width: 0,
                detail: Some("Dusk page/component".into()),
            };
            if component {
                facts.dusk_components.insert(full.clone(), e);
            } else {
                facts.dusk_pages.insert(full.clone(), e);
            }
            facts
                .dusk_selectors
                .insert(full, selector_names(&source, &path));
        }
    }
}
pub(super) fn site<'a>(
    index: &'a Index,
    doc: &Document,
    at: usize,
) -> Option<(std::ops::Range<usize>, &'a Names)> {
    if !enabled(index, "laravel/dusk") {
        return None;
    }
    let (literal, site) = crate::refs::literal_site_at(&doc.language, &doc.text, at)?;
    let selector_method = site.callee.rsplit("->").next()?;
    if site.arg != 0
        || site.depth != 0
        || ![
            "type",
            "click",
            "press",
            "assertVisible",
            "assertSeeIn",
            "waitFor",
            "value",
            "select",
            "check",
            "uncheck",
        ]
        .contains(&selector_method)
    {
        return None;
    }
    let code = crate::package_facts::code(&doc.text)?;
    let before = code[..site.callee_start].trim_end();
    if !before.ends_with(')') {
        let full = scoped_target(index, doc, &code, &site, at, 0)?;
        return Some((
            literal.start..literal.end,
            index.packages.dusk_selectors.get(&full)?,
        ));
    }
    let close = before.len() - 1;
    let open = code[..close]
        .match_indices('(')
        .rev()
        .find_map(|(open, _)| (php::closing(&doc.text, open) == Some(close)).then_some(open))?;
    let prefix = code[..open].trim_end();
    if !prefix.ends_with("->visit") && !prefix.ends_with("->on") {
        return None;
    }
    let call = crate::eloquent_members::metadata_inference::call_identity(&doc.text, open)?;
    if call.class.as_deref() != Some("Laravel\\Dusk\\Browser") {
        return None;
    }
    let arg = php::arguments(&doc.text, open).first()?.clone();
    let value = doc.text[arg].trim();
    let class = value
        .strip_prefix("new ")?
        .trim()
        .trim_end_matches("()")
        .trim();
    if !class
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'\\'))
    {
        return None;
    }
    let full = crate::eloquent_members::metadata_inference::class_identity(&doc.text, class)?;
    let names = index.packages.dusk_selectors.get(&full)?;
    Some((literal.start..literal.end, names))
}
fn target_class(source: &str, range: std::ops::Range<usize>) -> Option<String> {
    let raw = source[range].trim();
    let raw = raw.strip_prefix("new ")?.trim();
    let n = raw
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'\\'))
        .count();
    if n == 0 {
        return None;
    }
    let tail = raw[n..].trim();
    if !tail.is_empty()
        && !(tail.starts_with('(') && crate::php::closing(tail, 0) == Some(tail.len() - 1))
    {
        return None;
    }
    crate::eloquent_members::metadata_inference::class_identity(source, &raw[..n])
}
fn scoped_target(
    index: &Index,
    doc: &Document,
    code: &str,
    site: &crate::refs::Site,
    at: usize,
    depth: usize,
) -> Option<String> {
    if depth >= 8 {
        return None;
    }
    let open = doc.text[site.callee_start..].find('(')? + site.callee_start;
    let identity = crate::eloquent_members::metadata_inference::call_identity(&doc.text, open)?;
    if identity.class.as_deref() != Some("Laravel\\Dusk\\Browser") {
        return None;
    }
    let callee = code[site.callee_start..open].trim();
    let (variable, _) = callee.split_once("->")?;
    if !variable.starts_with('$') || !variable[1..].bytes().all(|b| crate::php::is_identifier(b)) {
        return None;
    }
    // A current scoped closure's explicit component wins over its outer page.
    let calls: Vec<_> = code[..site.callee_start].match_indices("->").collect();
    if calls.len() > 128 || code.len().saturating_mul(calls.len()) > 8 * 1024 * 1024 {
        return None;
    }
    for (arrow, _) in calls.iter().rev() {
        let tail = code[*arrow + 2..].trim_start();
        let name = tail
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .next()?;
        if !["within", "with"].contains(&name) {
            continue;
        }
        let after = *arrow + 2 + (code[*arrow + 2..].len() - tail.len()) + name.len();
        let rest = code[after..].trim_start();
        if !rest.starts_with('(') {
            continue;
        }
        let open = code.len() - rest.len();
        let args = php::arguments(&doc.text, open);
        if args.len() != 2 || !(args[1].start < at && at < args[1].end) {
            continue;
        }
        let closure = doc.text[args[1].clone()]
            .trim_start()
            .trim_start_matches("static ");
        if !closure.starts_with("function") {
            return None;
        }
        if crate::eloquent_members::metadata_inference::call_identity(&doc.text, open)?
            .class
            .as_deref()
            != Some("Laravel\\Dusk\\Browser")
        {
            return None;
        }
        let full = if let Some(full) = target_class(&doc.text, args[0].clone()) {
            full
        } else {
            php::literal(&doc.text, args[0].clone())?;
            let mut start = *arrow;
            while start > 0
                && (crate::php::is_identifier(code.as_bytes()[start - 1])
                    || code.as_bytes()[start - 1] == b'$')
            {
                start -= 1;
            }
            if start == *arrow || !code[start..*arrow].starts_with('$') {
                return None;
            }
            let mut outer = site.clone();
            outer.callee_start = start;
            return scoped_target(index, doc, code, &outer, start, depth + 1);
        };
        return index
            .packages
            .dusk_components
            .contains_key(&full)
            .then_some(full);
    }
    // Straight-line page state is scoped to the innermost balanced function body.
    let mut braces = Vec::new();
    for (i, b) in code[..site.callee_start].bytes().enumerate() {
        match b {
            b'{' => braces.push(i),
            b'}' => {
                braces.pop();
            }
            _ => {}
        }
    }
    let body = *braces.last()?;
    let prefix = format!("{variable}->");
    let mut selected = None;
    for (call, _) in code[body + 1..site.callee_start].match_indices(&prefix) {
        let call = body + 1 + call;
        let tail = &code[call + prefix.len()..];
        let name = tail
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .next()?;
        if !["visit", "on", "onWithoutAssert"].contains(&name) {
            continue;
        }
        let rest = tail[name.len()..].trim_start();
        if !rest.starts_with('(') {
            return None;
        }
        let open = code.len() - rest.len();
        let args = php::arguments(&doc.text, open);
        if args.len() != 1 {
            return None;
        }
        if crate::eloquent_members::metadata_inference::call_identity(&doc.text, open)?
            .class
            .as_deref()
            != Some("Laravel\\Dusk\\Browser")
        {
            return None;
        }
        let statement = code[..call]
            .rfind([';', '{', '}'])
            .map_or(body + 1, |i| i + 1);
        if !code[statement..call].trim().is_empty() {
            return None;
        }
        let class = target_class(&doc.text, args[0].clone());
        let close = php::closing(&doc.text, open)?;
        let tail = code[close + 1..site.callee_start]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>();
        if tail.contains(&format!("{variable}=")) {
            return None;
        }
        // No conditional/nested scope may prove a page state for a later sibling.
        if code[body + 1..call].bytes().filter(|b| *b == b'{').count()
            != code[body + 1..call].bytes().filter(|b| *b == b'}').count()
            || code[close + 1..site.callee_start].contains(['{', '}'])
        {
            return None;
        }
        selected = class.filter(|class| index.packages.dusk_pages.contains_key(class));
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selector_arrays_are_literal_and_method_scoped() {
        let source = "<?php class Login {function elements(){ return ['@email'=>'input[type=email]','@submit'=>'button']; }}";
        assert_eq!(selector_names(source, "x").len(), 2);
        assert!(selector_names(&source.replace("return [", "$x=1; return ["), "x").is_empty());
        assert!(
            selector_names(&source.replace("'button'", "$button"), "x")
                .get("@submit")
                .is_none()
        );
    }
}
#[cfg(test)]
mod provider_tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, providers::Providers};
    #[test]
    fn dusk_page_selectors_follow_only_explicit_owned_browser_visit() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("artisan", "<?php");
        host.add_file("composer.json",r#"{"require":{"laravel/framework":"^13"},"autoload":{"psr-4":{"App\\":"app/"}},"autoload-dev":{"psr-4":{"Tests\\":"tests/"}}}"#);
        host.add_file(
            "composer.lock",
            r#"{"packages":[],"packages-dev":[{"name":"laravel/dusk","version":"v8.8.0"}]}"#,
        );
        host.add_file("tests/Browser/Pages/Login.php","<?php namespace Tests\\Browser\\Pages; use Laravel\\Dusk\\Page; class Login extends Page {function elements(){return ['@email'=>'input[name=email]'];}}");
        let project = host.invocation().project().unwrap();
        let index = Index::build(&project);
        assert!(
            index.packages.dusk_selectors["Tests\\Browser\\Pages\\Login"].contains_key("@email")
        );
        let mut main = crate::Main::default();
        main.indexes.insert("/project".into(), index);
        let source = "<?php use Laravel\\Dusk\\Browser; use Tests\\Browser\\Pages\\Login; function check(Browser $browser){$browser->visit(new Login)->type('@em');}";
        let doc = Document {
            path: "/project/tests/Browser/LoginTest.php".into(),
            language: "php".into(),
            version: 1,
            text: source.into(),
        };
        let at = text::position(source, source.find("@em").unwrap() + 3);
        assert!(
            main.completion(&doc, at, None)
                .iter()
                .any(|i| i.label == "@email")
        );
        host.add_file(
            "tests/Browser/Pages/Login.php",
            "<?php namespace Tests\\Browser\\Pages; class Login {} ",
        );
        assert!(
            !main
                .completion(&doc, at, None)
                .iter()
                .any(|i| i.label == "@email")
        );
        let foreign = source.replace("Laravel\\Dusk\\Browser", "App\\Browser");
        let doc = Document {
            text: foreign.clone(),
            ..doc
        };
        assert!(
            !main
                .completion(
                    &doc,
                    text::position(&foreign, foreign.find("@em").unwrap() + 3),
                    None
                )
                .iter()
                .any(|i| i.label == "@email")
        );
    }
}
#[cfg(test)]
mod scoped_provider_tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, providers::Providers};
    #[test]
    fn page_state_and_component_within_closures_keep_their_own_cached_selector_scope() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("artisan", "<?php");
        host.add_file("composer.json",r#"{"require":{"laravel/framework":"^13"},"autoload":{"psr-4":{"App\\":"app/"}},"autoload-dev":{"psr-4":{"Tests\\":"tests/"}}}"#);
        host.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"laravel/dusk","version":"v8.8.0"}]}"#,
        );
        host.add_file("tests/Browser/Pages/Login.php","<?php namespace Tests\\Browser\\Pages; class Login extends \\Laravel\\Dusk\\Page {function elements(){return ['@email'=>'input[name=email]'];}}");
        host.add_file("tests/Browser/Components/Card.php","<?php namespace Tests\\Browser\\Components; class Card extends \\Laravel\\Dusk\\Component {function elements(){return ['@number'=>'input[name=number]'];}}");
        let index = Index::build(&host.invocation().project().unwrap());
        let mut main = crate::Main::default();
        main.indexes.insert("/project".into(), index);
        for (body, needle, label) in [
            (
                "$browser->visit(new Login);$browser->type('@em');",
                "@em",
                "@email",
            ),
            (
                "$browser->within(new Card,function(Browser $scope){$scope->type('@nu');});",
                "@nu",
                "@number",
            ),
            (
                "$browser->visit(new Login);$browser->within('#form',function(Browser $scope){$scope->type('@em');});",
                "@em",
                "@email",
            ),
        ] {
            let source = format!(
                "<?php use Laravel\\Dusk\\Browser;use Tests\\Browser\\Pages\\Login;use Tests\\Browser\\Components\\Card;function check(Browser $browser){{{body}}}"
            );
            let doc = Document {
                path: "/project/tests/Browser/CardTest.php".into(),
                language: "php".into(),
                version: 1,
                text: source.clone(),
            };
            let found = main.completion(
                &doc,
                text::position(&source, source.find(needle).unwrap() + needle.len()),
                None,
            );
            assert!(found.iter().any(|i| i.label == label), "{source}");
            let source = source.replace("Laravel\\Dusk\\Browser", "App\\Browser");
            let doc = Document {
                text: source.clone(),
                ..doc
            };
            assert!(
                !main
                    .completion(
                        &doc,
                        text::position(&source, source.find(needle).unwrap() + needle.len()),
                        None
                    )
                    .iter()
                    .any(|i| i.label == label)
            );
        }
        let source = "<?php use Laravel\\Dusk\\Browser;use Tests\\Browser\\Pages\\Login;function check(Browser $browser){if($ok)$browser->visit(new Login);$browser->type('@em');}";
        let doc = Document {
            path: "/project/tests/Browser/CardTest.php".into(),
            language: "php".into(),
            version: 1,
            text: source.into(),
        };
        assert!(
            !main
                .completion(
                    &doc,
                    text::position(source, source.find("@em").unwrap() + 3),
                    None
                )
                .iter()
                .any(|i| i.label == "@email")
        );
    }
}
