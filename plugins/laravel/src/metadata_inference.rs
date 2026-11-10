//! Source-backed application of declarative helpers to editor inference.
use super::{Kind, Models, Names, Objects, Source, entry, objects};
use crate::metadata::{ForwardClass, Forwarded, Metadata};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn cache(metadata: &mut Metadata, text: &str, path: &str) {
    if metadata.forward_methods.is_empty() {
        return;
    }
    let Some(source) = Source::new(text) else {
        return;
    };
    let Some(names) = Names::parse(&source) else {
        return;
    };
    let Some((full, start, body, parent)) = &names.class else {
        return;
    };
    let Some(model) = super::model(text, path) else {
        return;
    };
    let mut parents = model.traits.clone();
    if !parent.is_empty() {
        parents.push(parent.clone());
    }
    let mut constructor = None;
    let mut blocked = model.trait_adaptations || !model.adaptations.is_empty();
    for function in source.functions() {
        if source.paths[function.start] != vec![*body]
            || function
                .name
                .is_none_or(|name| !source.is(name, "__construct"))
        {
            continue;
        }
        if objects::method(&source, &names, &function, path).is_none() {
            blocked = true;
            break;
        }
        let mut cursor = function.params.start;
        let mut parameters = Vec::new();
        while cursor < function.params.end {
            let begin = cursor;
            while cursor < function.params.end && !source.is(cursor, ",") {
                if let Some(&end) = source.pairs.get(&cursor).filter(|&&end| end > cursor) {
                    cursor = end + 1;
                } else {
                    cursor += 1;
                }
            }
            let tokens = &source.tokens[begin..cursor];
            if !tokens.iter().any(|t| t.kind == Kind::Var) {
                blocked = true;
                break;
            }
            // Preserve declared defaults and PHP parameter syntax verbatim; strip
            // constructor-only promotion modifiers from the callable signature.
            let mut pieces = Vec::new();
            let mut variable = false;
            for token in tokens {
                if matches!(token.value, "public" | "protected" | "private" | "readonly") {
                    continue;
                }
                if token.kind == Kind::Var {
                    variable = true;
                }
                let text = if !variable
                    && token.kind == Kind::Name
                    && !matches!(
                        token.value,
                        "string"
                            | "int"
                            | "bool"
                            | "float"
                            | "array"
                            | "mixed"
                            | "object"
                            | "callable"
                            | "iterable"
                            | "null"
                            | "false"
                            | "true"
                    ) {
                    format!("\\{}", names.resolve(token.value))
                } else {
                    token.value.into()
                };
                pieces.push(text);
            }
            parameters.push(pieces.join(" "));
            cursor += 1;
        }
        constructor = Some(parameters);
    }
    let info = ForwardClass {
        full: full.clone(),
        path: path.into(),
        parents,
        constructor,
        constructor_blocked: blocked,
        declared_methods: model.declared.clone(),
        entry: entry(&source, path, start + 1),
    };
    let key = full.to_ascii_lowercase();
    if metadata.ambiguous_forward_classes.contains(&key) {
        return;
    }
    if metadata
        .forward_classes
        .get(&key)
        .is_some_and(|old| old.path != path)
    {
        metadata.forward_classes.remove(&key);
        metadata.ambiguous_forward_classes.insert(key);
        metadata
            .issues
            .push(format!("Ambiguous constructor-forwarded class {full}"));
        return;
    }
    metadata.forward_classes.insert(key, info);
}
fn inherits(
    classes: &BTreeMap<String, ForwardClass>,
    full: &str,
    base: &str,
    seen: &mut BTreeSet<String>,
    depth: usize,
) -> bool {
    if full.eq_ignore_ascii_case(base) {
        return true;
    }
    if depth >= 16 || !seen.insert(full.to_ascii_lowercase()) {
        return false;
    }
    classes
        .get(&full.to_ascii_lowercase())
        .is_some_and(|class| {
            class
                .parents
                .iter()
                .any(|parent| inherits(classes, parent, base, seen, depth + 1))
        })
}
fn constructor(
    classes: &BTreeMap<String, ForwardClass>,
    full: &str,
    seen: &mut BTreeSet<String>,
    depth: usize,
) -> Option<Vec<String>> {
    if depth >= 16 || !seen.insert(full.to_ascii_lowercase()) {
        return None;
    }
    let class = classes.get(&full.to_ascii_lowercase())?;
    if class.constructor_blocked {
        return None;
    }
    if let Some(parameters) = &class.constructor {
        return Some(parameters.clone());
    }
    // Traits/interfaces do not establish an inherited constructor. An exact,
    // unique known parent constructor is accepted; otherwise use no parameters.
    let candidates = class
        .parents
        .iter()
        .filter_map(|parent| classes.get(&parent.to_ascii_lowercase()))
        .filter(|parent| parent.constructor.is_some() || !parent.parents.is_empty())
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [] => Some(vec![]),
        [parent] => constructor(classes, &parent.full, seen, depth + 1),
        _ => None,
    }
}
fn evidence(
    classes: &BTreeMap<String, ForwardClass>,
    full: &str,
    seen: &mut BTreeSet<String>,
    depth: usize,
    out: &mut Vec<String>,
) {
    if depth >= 16 || seen.len() >= 128 || !seen.insert(full.to_ascii_lowercase()) {
        return;
    }
    if let Some(class) = classes.get(&full.to_ascii_lowercase()) {
        out.push(class.path.clone());
        for parent in &class.parents {
            evidence(classes, parent, seen, depth + 1, out);
        }
    }
}
fn mixed_object(
    full: &str,
    objects: &Objects,
    mixins: &[(String, String)],
    seen: &mut BTreeSet<String>,
    depth: usize,
    remaining: &mut usize,
    metadata_sources: &BTreeMap<String, String>,
) -> Option<objects::Object> {
    if *remaining == 0 {
        return None;
    }
    *remaining -= 1;
    if depth >= 16 || !seen.insert(full.to_ascii_lowercase()) {
        return None;
    }
    let mut target = objects
        .get(&full.to_ascii_lowercase())
        .filter(|o| !o.ambiguous)
        .cloned();
    let own_methods = target
        .as_ref()
        .map(|o| o.methods.keys().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default();
    let own_properties = target
        .as_ref()
        .map(|o| o.properties.keys().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default();
    let mut conflicts = BTreeSet::new();
    for (_, donor) in mixins
        .iter()
        .filter(|(class, _)| class.eq_ignore_ascii_case(full))
    {
        let mut branch = seen.clone();
        let donor = mixed_object(
            donor,
            objects,
            mixins,
            &mut branch,
            depth + 1,
            remaining,
            metadata_sources,
        )?;
        if target.is_none() {
            let mut synthetic = donor.clone();
            synthetic.full = full.into();
            synthetic.methods.clear();
            synthetic.properties.clear();
            synthetic.cast_target = None;
            synthetic.is_interface = false;
            target = Some(synthetic);
        }
        let current = target.as_mut()?;
        current
            .metadata_sources
            .extend(donor.metadata_sources.clone());
        current.metadata_sources.push(donor.path.clone());
        if let Some(path) = metadata_sources.get(&full.to_ascii_lowercase()) {
            current.metadata_sources.push(path.clone());
        }
        current.metadata_sources.sort();
        current.metadata_sources.dedup();
        for (name, method) in donor.methods {
            if own_methods.contains(&name) || conflicts.contains(&format!("m:{name}")) {
                continue;
            }
            if current
                .methods
                .get(&name)
                .is_some_and(|old| old.entry != method.entry)
            {
                current.methods.remove(&name);
                conflicts.insert(format!("m:{name}"));
            } else {
                current.methods.insert(name, method);
            }
        }
        for (name, entry) in donor.properties {
            if own_properties.contains(&name) || conflicts.contains(&format!("p:{name}")) {
                continue;
            }
            if current
                .properties
                .get(&name)
                .is_some_and(|old| old != &entry)
            {
                current.properties.remove(&name);
                conflicts.insert(format!("p:{name}"));
            } else {
                current.properties.insert(name, entry);
            }
        }
    }
    target
}
pub(crate) fn finish(models: &mut Models, objects: &mut Objects, metadata: &mut Metadata) {
    let original = objects.clone();
    let metadata_sources = metadata
        .mixin_sources
        .iter()
        .map(|(name, path)| (name.clone(), format!("{}/{}", metadata.project_root, path)))
        .collect::<BTreeMap<_, _>>();
    for (target, _) in &metadata.mixins {
        if let Some(mut object) = mixed_object(
            target,
            &original,
            &metadata.mixins,
            &mut BTreeSet::new(),
            0,
            &mut 512,
            &metadata_sources,
        ) {
            if let Some(model) = models
                .get_mut(&target.to_ascii_lowercase())
                .filter(|m| !m.ambiguous)
            {
                for (name, method) in &object.methods {
                    if !model.declared.contains(name) {
                        model.declared.insert(name.clone());
                        model.returns.insert(name.clone(), method.clone());
                    }
                }
                model.trait_sources.extend(object.metadata_sources.clone());
                model.trait_sources.sort();
                model.trait_sources.dedup();
                // A private/protected declaration must still shadow a mixin member.
                object.methods.retain(|name, _| {
                    !model.declared.contains(name) || model.returns.contains_key(name)
                });
            }
            objects.insert(target.to_ascii_lowercase(), object);
        } else {
            let source = metadata
                .mixin_sources
                .get(&target.to_ascii_lowercase())
                .map(String::as_str)
                .unwrap_or("ide.json");
            metadata.issues.push(format!(
                "{source}: unresolved or cyclic helperCode mixin {target}"
            ));
        }
    }
    for (key, class) in &metadata.forward_classes {
        for method in &metadata.forward_methods {
            if class
                .declared_methods
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&method.name))
            {
                continue;
            }
            if !inherits(
                &metadata.forward_classes,
                &class.full,
                &method.base,
                &mut BTreeSet::new(),
                0,
            ) {
                continue;
            }
            let Some(mut parameters) = constructor(
                &metadata.forward_classes,
                &class.full,
                &mut BTreeSet::new(),
                0,
            ) else {
                continue;
            };
            let mut all = method.additional.clone();
            all.append(&mut parameters);
            let mut sources = vec![format!("{}/{}", metadata.project_root, method.source)];
            evidence(
                &metadata.forward_classes,
                &class.full,
                &mut BTreeSet::new(),
                0,
                &mut sources,
            );
            sources.sort();
            sources.dedup();
            metadata.forwarded.insert(
                (key.clone(), method.name.to_ascii_lowercase()),
                Forwarded {
                    sources: sources.clone(),
                    parameters: all,
                    target: method.target.clone(),
                    entry: class.entry.clone(),
                },
            );
            if let Some(object) = objects.get_mut(key).filter(|o| !o.ambiguous) {
                object.metadata_sources.extend(sources);
                object.metadata_sources.sort();
                object.metadata_sources.dedup();
                object
                    .methods
                    .entry(method.name.clone())
                    .or_insert(objects::Method {
                        target: method.target.clone(),
                        static_method: true,
                        entry: class.entry.clone(),
                    });
            }
        }
    }
}

/// A callable identity proven entirely from the current buffer. This intentionally
/// excludes arbitrary expression receivers and flow merges.
#[derive(Debug)]
pub(crate) struct CallIdentity {
    pub class: Option<String>,
    pub method: String,
    pub constructor: bool,
    pub function_names: Vec<String>,
    pub function_fqn: Option<String>,
}
fn named_class(names: &Names, name: &str) -> Option<String> {
    match name.to_ascii_lowercase().as_str() {
        "self" => Some(names.class.as_ref()?.0.clone()),
        "parent" => Some(names.class.as_ref()?.3.clone()).filter(|n| !n.is_empty()),
        "static" | "int" | "string" | "bool" | "float" | "array" | "object" | "mixed"
        | "callable" | "iterable" | "null" | "false" | "true" => None,
        _ => Some(names.resolve(name)),
    }
}
fn new_receiver(
    source: &Source,
    names: &Names,
    end: usize,
    depth: usize,
) -> Option<(String, usize)> {
    if depth >= 4 {
        return None;
    }
    if source.is(end, ")") {
        let open = *source.pairs.get(&end)?;
        if open >= 2 && source.is(open - 2, "new") && source.tokens[open - 1].kind == Kind::Name {
            return Some((named_class(names, source.tokens[open - 1].value)?, open - 2));
        }
        let (full, start) = new_receiver(source, names, end.checked_sub(1)?, depth + 1)?;
        return (start == open + 1).then_some((full, open));
    }
    if source.tokens.get(end)?.kind == Kind::Name && end > 0 && source.is(end - 1, "new") {
        return Some((named_class(names, source.tokens[end].value)?, end - 1));
    }
    None
}
fn typed_parameter(
    source: &Source,
    names: &Names,
    function: &super::Function,
    variable: &str,
) -> Option<String> {
    let mut cursor = function.params.start;
    let mut found = None;
    while cursor < function.params.end {
        let begin = cursor;
        while cursor < function.params.end && !source.is(cursor, ",") {
            if let Some(&end) = source.pairs.get(&cursor).filter(|&&end| end > cursor) {
                cursor = end + 1;
            } else {
                cursor += 1;
            }
        }
        let params = &source.tokens[begin..cursor];
        if let Some(at) = params
            .iter()
            .position(|token| token.kind == Kind::Var && token.value == variable)
        {
            if found.is_some() {
                return None;
            }
            let before = params[..at]
                .iter()
                .filter(|token| {
                    !matches!(
                        token.value,
                        "public" | "protected" | "private" | "readonly" | "?"
                    )
                })
                .collect::<Vec<_>>();
            let kind = match before.as_slice() {
                [kind] if kind.kind == Kind::Name => *kind,
                [first, separator, last]
                    if separator.value == "|"
                        && first.value.eq_ignore_ascii_case("null")
                        && last.kind == Kind::Name =>
                {
                    *last
                }
                [first, separator, last]
                    if separator.value == "|"
                        && last.value.eq_ignore_ascii_case("null")
                        && first.kind == Kind::Name =>
                {
                    *first
                }
                _ => return None,
            };
            found = Some(named_class(names, kind.value)?);
        }
        cursor += 1;
    }
    found
}
fn variable_receiver(source: &Source, names: &Names, at: usize) -> Option<String> {
    let name = source.tokens.get(at)?.value;
    let functions = source.functions();
    let function = functions
        .iter()
        .filter(|f| f.body < at && at < f.end)
        .max_by_key(|f| f.body);
    let start = function.map_or(0, |f| f.body + 1);
    if (start..at).any(|i| source.is(i, "fn")) {
        return None;
    }
    if name == "$this" {
        let function = function?;
        if function.name.is_none()
            || (function.start.saturating_sub(4)..function.start).any(|i| source.is(i, "static"))
        {
            return None;
        }
        let class = names.class.as_ref()?;
        if source.is(class.1, "trait") {
            return None;
        }
        return Some(class.0.clone());
    }
    let mut binding = function.and_then(|f| typed_parameter(source, names, f, name));
    for i in start..at {
        if source.tokens[i].kind != Kind::Var || source.tokens[i].value != name {
            continue;
        }
        if source.is(i + 1, "=") {
            // Assignments in conditions/other blocks are not dominance evidence.
            let php_open =
                i >= 3 && source.is(i - 1, "php") && source.is(i - 2, "?") && source.is(i - 3, "<");
            if source.paths[i] != source.paths[at]
                || i > 0 && !matches!(source.tokens[i - 1].value, ";" | "{" | ">") && !php_open
            {
                return None;
            }
            let mut end = i + 2;
            while end < at && !source.is(end, ";") {
                if let Some(&close) = source.pairs.get(&end).filter(|&&close| close > end) {
                    end = close + 1;
                } else {
                    end += 1;
                }
            }
            if end >= at {
                return None;
            }
            let (full, first) = new_receiver(source, names, end.checked_sub(1)?, 0)?;
            if first != i + 2 {
                return None;
            }
            binding = Some(full);
        } else if !matches!(
            source.tokens.get(i + 1).map(|t| t.value),
            Some("->" | "?->")
        ) {
            // Includes reference aliases, unknown by-reference calls, foreach,
            // catch/global/unset and variable writes. Never skip a nearer mutation.
            return None;
        }
    }
    binding
}
fn function_imports(source: &Source) -> Option<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    let mut at = 0;
    while at < source.tokens.len() {
        if !source.is(at, "use") || !source.is(at + 1, "function") || !source.paths[at].is_empty() {
            at += 1;
            continue;
        }
        let mut cursor = at + 2;
        let mut prefix = String::new();
        if source.is(cursor + 1, "{") {
            prefix = source.tokens.get(cursor)?.value.into();
            cursor += 2;
        }
        loop {
            let token = source.tokens.get(cursor)?;
            if token.kind != Kind::Name {
                return None;
            }
            let full = format!("{prefix}{}", token.value)
                .trim_start_matches('\\')
                .to_owned();
            cursor += 1;
            let alias = if source.is(cursor, "as") {
                cursor += 1;
                let alias = source.tokens.get(cursor)?;
                if alias.kind != Kind::Name {
                    return None;
                }
                cursor += 1;
                alias.value
            } else {
                token.value.rsplit('\\').next()?
            };
            if result.insert(alias.to_ascii_lowercase(), full).is_some() {
                return None;
            }
            if source.is(cursor, ",") {
                cursor += 1;
                continue;
            }
            if !prefix.is_empty() {
                if !source.is(cursor, "}") {
                    return None;
                }
                cursor += 1;
            }
            if !source.is(cursor, ";") {
                return None;
            }
            at = cursor;
            break;
        }
        at += 1;
    }
    Some(result)
}
pub(crate) fn call_identity(text: &str, open: usize) -> Option<CallIdentity> {
    let source = Source::new(text)?;
    let names = Names::parse(&source)?;
    let open = source
        .tokens
        .iter()
        .position(|token| token.start == open && token.value == "(")?;
    let method = source.tokens.get(open.checked_sub(1)?)?;
    if method.kind != Kind::Name {
        return None;
    }
    if open >= 2 && source.is(open - 2, "new") {
        return Some(CallIdentity {
            class: Some(named_class(&names, method.value)?),
            method: "__construct".into(),
            constructor: true,
            function_names: vec![],
            function_fqn: None,
        });
    }
    if open >= 3 && source.is(open - 2, "::") {
        let class = source.tokens.get(open - 3)?;
        if class.kind != Kind::Name
            || open >= 4 && matches!(source.tokens[open - 4].value, "::" | "->" | "?->")
        {
            return None;
        }
        return Some(CallIdentity {
            class: Some(named_class(&names, class.value)?),
            method: method.value.into(),
            constructor: false,
            function_names: vec![],
            function_fqn: None,
        });
    }
    if open >= 3 && matches!(source.tokens[open - 2].value, "->" | "?->") {
        let end = open - 3;
        let class = if source.tokens[end].kind == Kind::Var {
            variable_receiver(&source, &names, end)?
        } else {
            new_receiver(&source, &names, end, 0)?.0
        };
        return Some(CallIdentity {
            class: Some(class),
            method: method.value.into(),
            constructor: false,
            function_names: vec![],
            function_fqn: None,
        });
    }
    if open >= 2
        && matches!(
            source.tokens[open - 2].value,
            "function" | "new" | "->" | "?->" | "::"
        )
    {
        return None;
    }
    let imports = function_imports(&source)?;
    let raw = method.value;
    let first = raw.trim_start_matches('\\').split('\\').next()?;
    let imported = (!raw.starts_with('\\') && !raw.contains('\\'))
        .then(|| imports.get(&first.to_ascii_lowercase()))
        .flatten();
    let full = if let Some(full) = imported {
        Some(full.clone())
    } else if raw.starts_with('\\') {
        Some(raw.trim_start_matches('\\').into())
    } else if let Some(tail) = raw.strip_prefix("namespace\\") {
        Some(if names.namespace.is_empty() {
            tail.into()
        } else {
            format!("{}\\{tail}", names.namespace)
        })
    } else if raw.contains('\\') {
        Some(names.resolve(raw))
    } else if names.namespace.is_empty()
        || source.functions().iter().any(|f| {
            f.name
                .is_some_and(|name| source.tokens[name].value.eq_ignore_ascii_case(raw))
                && source.paths[f.start].is_empty()
        })
    {
        Some(names.qualify(raw))
    } else {
        None
    };
    let mut shorts = vec![raw.rsplit('\\').next()?.into()];
    if let Some(full) = &full {
        let short = full.rsplit('\\').next()?.to_owned();
        if !shorts.contains(&short) {
            shorts.push(short);
        }
    }
    Some(CallIdentity {
        class: None,
        method: raw.into(),
        constructor: false,
        function_names: shorts,
        function_fqn: full,
    })
}

#[cfg(test)]
mod tests {
    use crate::index::Index;
    use openidea_plugin::{host, providers::Document, text};
    fn fixture(metadata: &str) -> openidea_plugin::mock::MockHost {
        let host = openidea_plugin::mock::MockHost::install();
        host.add_file(
            "composer.json",
            r#"{"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        host.add_file("ide.json", metadata);
        host
    }
    fn index() -> Index {
        Index::build(&host::projects().unwrap().remove(0))
    }
    fn document(source: &str) -> Document {
        Document {
            path: "/project/app/Http/Controllers/Demo.php".into(),
            language: "php".into(),
            text: source.into(),
            version: 1,
        }
    }
    #[test]
    fn helper_exclusions_respect_php_names_and_namespace_boundaries() {
        let _host = fixture(
            r#"{"helperCode":{"eloquent":{"ignoreClasses":["\\App\\Models\\Secret"],"ignoreNamespaces":["\\Tests\\"]}}}"#,
        );
        let metadata = index().metadata;
        assert!(metadata.helper_ignored("App\\Models\\Secret"));
        assert!(metadata.helper_ignored("tests\\Models\\User"));
        assert!(!metadata.helper_ignored("TestsOther\\User"));
        assert!(!metadata.helper_ignored("App\\Models\\Secrets"));
    }
    #[test]
    fn class_mixins_complete_and_navigate_real_donor_members() {
        let host = fixture(
            r#"{"helperCode":{"classMixins":[{"classFqn":"\\Illuminate\\Http\\Request","mixinFqn":"\\App\\RequestMacros"}]}}"#,
        );
        host.add_file("app/RequestMacros.php","<?php namespace App; class RequestMacros { public string $tenant; public function tenantData(): \\App\\Data\\Tenant {return new \\App\\Data\\Tenant;} private function hidden(): string {return '';} }");
        host.add_file(
            "app/Data/Tenant.php",
            "<?php namespace App\\Data; class Tenant { public string $name; }",
        );
        let index = index();
        let doc = document(
            "<?php namespace App\\Http\\Controllers; use Illuminate\\Http\\Request; function run(Request $request) { $request-> }",
        );
        let at = text::position(&doc.text, doc.text.find("->").unwrap() + 2);
        let items = crate::eloquent_members::completion_checked(&index, &doc, at, &|_| true);
        assert!(
            items.iter().any(|i| i.label == "tenant"),
            "items={items:?} objects={:?} metadata={:?}",
            index.eloquent_objects,
            index.metadata
        );
        assert!(items.iter().any(|i| i.label == "tenantData()"));
        assert!(!items.iter().any(|i| i.label == "hidden()"));
        assert!(
            crate::eloquent_members::completion_checked(&index, &doc, at, &|p| p
                != "/project/ide.json")
            .is_empty()
        );
        assert!(
            crate::eloquent_members::completion_checked(&index, &doc, at, &|p| p
                != "/project/app/RequestMacros.php")
            .is_empty()
        );
        let doc = document(
            "<?php use Illuminate\\Http\\Request; function run(Request $request) { $request->tenantData(); }",
        );
        let at = text::position(&doc.text, doc.text.find("tenantData").unwrap() + 2);
        assert_eq!(
            crate::eloquent_members::definition_checked(&index, &doc, at, &|_| true)
                .unwrap()
                .path,
            "/project/app/RequestMacros.php"
        );
        let doc = document(
            "<?php use Illuminate\\Http\\Request; function run(Request $request) { $request->tenantData()-> }",
        );
        let at = text::position(&doc.text, doc.text.rfind("->").unwrap() + 2);
        assert!(
            crate::eloquent_members::completion_checked(&index, &doc, at, &|_| true)
                .iter()
                .any(|i| i.label == "name")
        );
    }
    #[test]
    fn forwarded_constructor_parameters_have_completion_signature_and_return_inference() {
        let host = fixture(
            r#"{"helperCode":{"methodsWithConstructorParameters":[{"baseFqn":"\\App\\Bus","methods":[{"name":"dispatch","returnType":"\\App\\Data\\Pending","additionalParameters":[{"name":"delay","type":"int"}]}]}]}}"#,
        );
        host.add_file("app/Bus.php", "<?php namespace App; trait Bus {}");
        host.add_file("app/Jobs/Send.php","<?php namespace App\\Jobs; use App\\Bus; class Send { use Bus; public function __construct(public string $message, public int $count = 1) {} }");
        host.add_file("app/Data/Pending.php","<?php namespace App\\Data; class Pending {public function done(): string {return '';}}");
        let index = index();
        assert!(
            index.metadata.issues.is_empty(),
            "{:?}",
            index.metadata.issues
        );
        let doc = document("<?php use App\\Jobs\\Send; Send::");
        let at = text::position(&doc.text, doc.text.len());
        let items = crate::eloquent_members::completion_checked(&index, &doc, at, &|_| true);
        let item = items
            .iter()
            .find(|i| i.label == "dispatch()")
            .expect("forwarded completion");
        assert!(item.detail.as_ref().unwrap().contains("int $delay"));
        assert!(item.insert_text.contains("message"));
        let doc = document("<?php use App\\Jobs\\Send; Send::dispatch(2, 'hello', 1);");
        let at = text::position(&doc.text, doc.text.find("'hello'").unwrap());
        let signature =
            crate::eloquent_members::forwarded_signature_help_checked(&index, &doc, at, &|_| true)
                .unwrap();
        assert_eq!(signature.active_parameter, 1);
        assert_eq!(signature.signatures[0].parameters.len(), 3);
        assert!(
            crate::eloquent_members::forwarded_signature_help_checked(&index, &doc, at, &|p| p
                != "/project/ide.json")
            .is_none()
        );
        assert!(
            crate::eloquent_members::forwarded_signature_help_checked(&index, &doc, at, &|p| p
                != "/project/app/Bus.php")
            .is_none()
        );
        let doc = document("<?php use App\\Jobs\\Send; Send::dispatch(2,'hello')->");
        let at = text::position(&doc.text, doc.text.len());
        assert!(
            crate::eloquent_members::completion_checked(&index, &doc, at, &|_| true)
                .iter()
                .any(|i| i.label == "done()")
        );
        assert!(
            crate::eloquent_members::forwarded_signature_help_checked(
                &index,
                &document("<?php \\App\\Jobs\\Send::dispatch('x');"),
                openidea_plugin::Position::new(0, 33),
                &|_| false
            )
            .is_none()
        );
    }
    #[test]
    fn mixin_cycles_are_reported_and_private_constructors_do_not_forward() {
        let host = fixture(
            r#"{"helperCode":{"classMixins":[{"classFqn":"\\App\\A","mixinFqn":"\\App\\B"},{"classFqn":"\\App\\B","mixinFqn":"\\App\\A"}],"methodsWithConstructorParameters":[{"baseFqn":"\\App\\A","methods":[{"name":"dispatch","returnType":"\\App\\B"}]}]}}"#,
        );
        host.add_file(
            "app/A.php",
            "<?php namespace App; class A {private function __construct(string $secret) {}}",
        );
        host.add_file("app/B.php", "<?php namespace App; class B {}");
        let index = index();
        assert!(index.metadata.issues.iter().any(|i| i.contains("cyclic")));
        assert!(index.metadata.forwarded.is_empty());
    }
}

/// Static Route facade calls proven by the bounded PHP name/import parser.
/// Returns (receiver start, method start); strings/comments never become calls.
pub(crate) fn route_calls(text: &str) -> Vec<(usize, usize)> {
    let Some(source) = Source::new(text) else {
        return Vec::new();
    };
    let Some(names) = Names::parse(&source) else {
        return Vec::new();
    };
    let mut calls = Vec::new();
    let mut shadows = std::collections::BTreeMap::new();
    for i in 3..source.tokens.len() {
        if !source.is(i, "(") || !source.is(i - 2, "::") {
            continue;
        }
        let (class, method) = (&source.tokens[i - 3], &source.tokens[i - 1]);
        if class.kind != Kind::Name || method.kind != Kind::Name {
            continue;
        }
        if i >= 4 && matches!(source.tokens[i - 4].value, "::" | "->" | "?->" | "new") {
            continue;
        }
        let short = class.value.rsplit('\\').next().unwrap_or(class.value);
        if *shadows
            .entry(short.to_ascii_lowercase())
            .or_insert_with(|| crate::php::declares_class(text, short))
        {
            continue;
        }
        let full = names.resolve(class.value);
        if full.eq_ignore_ascii_case("Illuminate\\Support\\Facades\\Route")
            || full.eq_ignore_ascii_case("Route")
        {
            calls.push((class.start, method.start));
        }
    }
    calls
}

pub(crate) fn class_identity(text: &str, name: &str) -> Option<String> {
    let source = Source::new(text)?;
    let names = Names::parse(&source)?;
    named_class(&names, name)
}
