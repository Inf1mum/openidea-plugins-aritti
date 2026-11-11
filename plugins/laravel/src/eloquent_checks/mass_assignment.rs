//! Source-proven model mass assignment. Cached declarations never establish receiver types.
use super::Finding;
use crate::{eloquent_members, index::Index, php};
use openidea_plugin::{Severity, providers::Document};
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, Default)]
pub(crate) struct CachedPolicy {
    fingerprint: u64,
    fillable: Option<Vec<String>>,
    guarded: Option<Vec<String>>,
    unknown: bool,
    parent: Option<String>,
    attribute_fillable: Option<Vec<String>>,
    attribute_guarded: Option<Vec<String>>,
    attribute_unguarded: bool,
    has_attributes: bool,
    attribute_variadic: bool,
}
pub(crate) type Policies = BTreeMap<String, CachedPolicy>;
fn fingerprint(source: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut h);
    h.finish()
}
#[derive(Clone)]
struct Token<'a> {
    value: &'a str,
    start: usize,
}
fn tokens(text: &str) -> Vec<Token<'_>> {
    let b = text.as_bytes();
    let mut result = Vec::new();
    let mut at = 0;
    while at < b.len() {
        if b[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        let start = at;
        if matches!(b[at], b'\'' | b'"') {
            at = php::string_end(b, at).min(b.len());
        } else if php::is_identifier(b[at]) || matches!(b[at], b'$' | b'\\') {
            at += 1;
            while at < b.len() && (php::is_identifier(b[at]) || b[at] == b'\\') {
                at += 1;
            }
        } else {
            at += 1;
        }
        // PHP identifiers can contain UTF-8; do not slice inside a scalar.
        while !text.is_char_boundary(at) {
            at += 1;
        }
        result.push(Token {
            value: &text[start..at],
            start,
        });
    }
    result
}
fn resolved_name(name: &str, source: &str) -> String {
    eloquent_members::source_class_name(source, name).unwrap_or_default()
}
fn safe_framework_trait(name: &str, source: &str) -> bool {
    matches!(
        resolved_name(name, source).as_str(),
        "Illuminate\\Database\\Eloquent\\Factories\\HasFactory"
            | "Illuminate\\Database\\Eloquent\\SoftDeletes"
            | "Illuminate\\Database\\Eloquent\\Concerns\\HasUuids"
            | "Illuminate\\Database\\Eloquent\\Concerns\\HasUlids"
    )
}

fn parse(source: &str) -> CachedPolicy {
    let mut out = CachedPolicy {
        fingerprint: fingerprint(source),
        ..Default::default()
    };
    let masked = php::mask_comments(source);
    let t = tokens(&masked);
    let Some(class) = t.iter().position(|t| matches!(t.value, "class" | "trait")) else {
        out.unknown = true;
        return out;
    };
    let Some(body) = t
        .iter()
        .enumerate()
        .skip(class)
        .find(|(_, t)| t.value == "{")
        .map(|(i, _)| i)
    else {
        out.unknown = true;
        return out;
    };
    out.parent = t[class..body]
        .iter()
        .position(|t| t.value == "extends")
        .and_then(|at| t.get(class + at + 1))
        .map(|parent| resolved_name(parent.value, &masked));
    let mut depth = 0;
    let mut seen = BTreeSet::new();
    for (i, token) in t.iter().enumerate().skip(body) {
        if token.value == "{" {
            depth += 1;
            continue;
        }
        if token.value == "}" {
            depth -= 1;
            if depth == 0 {
                break;
            }
            continue;
        }
        // Runtime mutations, attributes and overridden hooks make literal defaults insufficient.
        let declaration = i > 0
            && (t[i - 1].value == "function"
                || i > 1 && t[i - 1].value == "&" && t[i - 2].value == "function");
        if declaration
            && matches!(
                token.value,
                "__construct"
                    | "__call"
                    | "__callStatic"
                    | "getFillable"
                    | "getGuarded"
                    | "isFillable"
                    | "isGuarded"
                    | "isGuardableColumn"
                    | "initializeGuardsAttributes"
                    | "initializeTraits"
                    | "newInstance"
                    | "newEloquentBuilder"
                    | "newBaseQueryBuilder"
                    | "fillableFromArray"
                    | "fill"
                    | "update"
                    | "create"
                    | "make"
                    | "guard"
                    | "fillable"
                    | "mergeFillable"
                    | "mergeGuarded"
            )
        {
            out.unknown = true;
        }
        if matches!(token.value, "unguard" | "unguarded" | "Unguarded") {
            out.unknown = true;
        }
        let this_member =
            i >= 3 && t[i - 3].value == "$this" && t[i - 2].value == "-" && t[i - 1].value == ">";
        if this_member
            && matches!(
                token.value,
                "guarded" | "fillable" | "guard" | "mergeFillable" | "mergeGuarded"
            )
        {
            out.unknown = true;
        }
        if depth != 1 {
            continue;
        }
        if matches!(token.value, "$unguarded" | "$builder") {
            out.unknown = true;
        }
        // Unknown traits may override guard hooks or properties. Safe omission beats a false warning.
        if token.value == "use" {
            let end = (i + 1..t.len())
                .find(|at| matches!(t[*at].value, ";" | "{"))
                .unwrap_or(t.len());
            if t.get(end).is_none_or(|t| t.value != ";")
                || t[i + 1..end]
                    .iter()
                    .filter(|t| t.value != ",")
                    .any(|token| !safe_framework_trait(token.value, &masked))
            {
                out.unknown = true;
            }
        }
        if !matches!(token.value, "$fillable" | "$guarded") {
            continue;
        }
        if !seen.insert(token.value) || t.get(i + 1).is_none_or(|t| t.value != "=") {
            out.unknown = true;
            continue;
        }
        let Some(open) = t.get(i + 2).filter(|t| t.value == "[") else {
            out.unknown = true;
            continue;
        };
        let Some(close) = php::closing(&masked, open.start) else {
            out.unknown = true;
            continue;
        };
        let items = php::items(&masked, open.start);
        let mut values = Vec::new();
        for item in items {
            if item.key.is_some() {
                out.unknown = true;
                break;
            }
            let Some((value, _)) = php::literal(&masked, item.value) else {
                out.unknown = true;
                break;
            };
            if value.contains(['\\', '$']) {
                out.unknown = true;
                break;
            }
            values.push(value.to_owned());
        }
        if t.iter()
            .find(|t| t.start > close)
            .is_none_or(|t| t.value != ";")
        {
            out.unknown = true;
        }
        if token.value == "$fillable" {
            out.fillable = Some(values)
        } else {
            out.guarded = Some(values)
        }
    }
    for (at, _) in t[..class]
        .iter()
        .enumerate()
        .filter(|(_, t)| t.value == "#")
    {
        out.has_attributes = true;
        if t.get(at + 1).is_none_or(|t| t.value != "[") {
            out.unknown = true;
            continue;
        }
        let Some(end) = php::closing(&masked, t[at + 1].start) else {
            out.unknown = true;
            continue;
        };
        let mut cursor = at + 2;
        while cursor < class && t[cursor].start < end {
            let name = resolved_name(t[cursor].value, &masked);
            cursor += 1;
            let short = match name.as_str() {
                "Illuminate\\Database\\Eloquent\\Attributes\\Fillable" => "fillable",
                "Illuminate\\Database\\Eloquent\\Attributes\\Guarded" => "guarded",
                "Illuminate\\Database\\Eloquent\\Attributes\\Unguarded" => "unguarded",
                _ => {
                    out.unknown = true;
                    break;
                }
            };
            let mut values = Vec::new();
            if t.get(cursor).is_some_and(|t| t.value == "(") {
                let open = t[cursor].start;
                let Some(close) = php::closing(&masked, open) else {
                    out.unknown = true;
                    break;
                };
                let arguments = php::arguments(&masked, open);
                if short != "unguarded" && arguments.is_empty()
                    || short == "unguarded" && !arguments.is_empty()
                {
                    out.unknown = true;
                }
                for argument in arguments {
                    if let Some(array) = php::array_open(&masked, argument.clone()) {
                        for item in php::items(&masked, array) {
                            if item.key.is_some() {
                                out.unknown = true;
                                break;
                            }
                            if let Some((value, _)) = php::literal(&masked, item.value) {
                                if value.contains(['\\', '$']) {
                                    out.unknown = true;
                                    break;
                                }
                                values.push(value.to_owned())
                            } else {
                                out.unknown = true;
                                break;
                            }
                        }
                        if php::arguments(&masked, open).len() != 1 {
                            out.unknown = true;
                        }
                    } else if let Some((value, _)) = php::literal(&masked, argument) {
                        out.attribute_variadic = true;
                        if value.contains(['\\', '$']) {
                            out.unknown = true;
                            break;
                        }
                        values.push(value.to_owned())
                    } else {
                        out.unknown = true;
                        break;
                    }
                }
                while cursor < t.len() && t[cursor].start <= close {
                    cursor += 1;
                }
            } else if short != "unguarded" {
                out.unknown = true;
            }
            match short {
                "fillable" => {
                    if out.attribute_fillable.replace(values).is_some() {
                        out.unknown = true;
                    }
                }
                "guarded" => {
                    if out.attribute_guarded.replace(values).is_some() {
                        out.unknown = true;
                    }
                }
                _ => {
                    if out.attribute_unguarded || !values.is_empty() {
                        out.unknown = true;
                    }
                    out.attribute_unguarded = true;
                }
            }
            if t.get(cursor)
                .is_some_and(|t| t.start < end && t.value == ",")
            {
                cursor += 1;
            } else if t.get(cursor).is_none_or(|t| t.start != end) {
                out.unknown = true;
                break;
            }
        }
    }
    out
}
pub(crate) fn cache(policies: &mut Policies, source: &str, path: &str) {
    if source.len() <= 128 * 1024 {
        policies.insert(path.into(), parse(source));
    }
}

fn profile(
    index: &Index,
    doc: &Document,
    full: &str,
    owns: &dyn Fn(&str) -> bool,
    read: &dyn Fn(&str) -> Option<String>,
) -> Option<(Vec<String>, Vec<String>)> {
    let classes = eloquent_members::class_inventory(&index.eloquent_models);
    let mut current = full.to_owned();
    let mut seen = BTreeSet::new();
    let mut fillable = None;
    let mut guarded = None;
    let mut attribute_fillable = None;
    let mut attribute_guarded = None;
    let mut attribute_unguarded = false;
    for _ in 0..16 {
        if matches!(
            current.as_str(),
            "Illuminate\\Database\\Eloquent\\Model"
                | "Illuminate\\Foundation\\Auth\\User"
                | "Illuminate\\Database\\Eloquent\\Relations\\Pivot"
                | "Illuminate\\Database\\Eloquent\\Relations\\MorphPivot"
        ) {
            let default = if current.ends_with("\\Pivot") || current.ends_with("\\MorphPivot") {
                Vec::new()
            } else {
                vec!["*".into()]
            };
            let mut fillable = fillable.unwrap_or_else(Vec::new);
            let mut guarded = guarded.unwrap_or_else(|| default.clone());
            let version = index.metadata.framework_version.unwrap_or((0, 0, 0));
            if let Some(fields) = attribute_fillable {
                if fillable.is_empty() || version >= (13, 3, 0) {
                    for field in fields {
                        if !fillable.contains(&field) {
                            fillable.push(field);
                        }
                    }
                }
            }
            let attribute_default = if version >= (13, 24, 0) {
                default
            } else {
                vec!["*".into()]
            };
            if guarded == attribute_default {
                if attribute_unguarded {
                    guarded.clear();
                } else if let Some(fields) = attribute_guarded {
                    guarded = fields;
                }
            }
            return Some((fillable, guarded));
        }
        if !seen.insert(current.to_ascii_lowercase()) {
            return None;
        }
        let identity = classes.iter().find(|c| {
            c.full.eq_ignore_ascii_case(&current) && !c.ambiguous && !c.is_trait && owns(&c.path)
        })?;
        let overlay;
        let policy = if identity.path == doc.path {
            overlay = parse(&doc.text);
            &overlay
        } else {
            let cached = index.mass_assignment_sources.get(&identity.path)?;
            let source = read(&identity.path).filter(|text| text.len() <= 128 * 1024)?;
            if fingerprint(&source) != cached.fingerprint {
                return None;
            }
            cached
        };
        if policy.unknown
            || policy.has_attributes
                && index
                    .metadata
                    .framework_version
                    .is_none_or(|version| version < (13, 0, 0))
            || policy.attribute_variadic
                && index
                    .metadata
                    .framework_version
                    .is_none_or(|version| version < (13, 3, 0))
        {
            return None;
        }
        if attribute_fillable.is_none() {
            attribute_fillable = policy.attribute_fillable.clone();
        }
        if attribute_guarded.is_none() {
            attribute_guarded = policy.attribute_guarded.clone();
        }
        attribute_unguarded |= policy.attribute_unguarded;
        if fillable.is_none() {
            fillable = policy.fillable.clone()
        }
        if guarded.is_none() {
            guarded = policy.guarded.clone()
        }
        current = policy
            .parent
            .clone()
            .unwrap_or_else(|| identity.parent.clone());
    }
    None
}
/// JSON is an explicit model => exempt field list map. `*` exempts that model.
pub(crate) fn checked(
    index: &Index,
    doc: &Document,
    owns: &dyn Fn(&str) -> bool,
    read: &dyn Fn(&str) -> Option<String>,
    exceptions_json: &str,
) -> Vec<Finding> {
    if index.partial || doc.text.len() > 32 * 1024 || !owns(&doc.path) {
        return Vec::new();
    }
    let masked = php::mask_comments(&doc.text);
    let t = tokens(&masked);
    if t.iter().any(|t| {
        matches!(
            t.value,
            "unguard" | "unguarded" | "mergeFillable" | "mergeGuarded" | "fillable" | "guard"
        )
    }) {
        return Vec::new();
    }
    let exceptions =
        serde_json::from_str::<BTreeMap<String, Vec<String>>>(exceptions_json).unwrap_or_default();
    let mut found = Vec::new();
    let mut profiles = BTreeMap::new();
    let mut calls = 0;
    for (i, token) in t.iter().enumerate() {
        let constructor = i > 0
            && t[i - 1].value == "new"
            && !token.value.starts_with('$')
            && !matches!(token.value, "static" | "self" | "parent" | "class");
        if (!constructor && !matches!(token.value, "create" | "make" | "fill" | "update"))
            || t.get(i + 1).is_none_or(|t| t.value != "(")
        {
            continue;
        }
        let object = i >= 2 && t[i - 2].value == "-" && t[i - 1].value == ">";
        let static_call = i >= 2 && t[i - 2].value == ":" && t[i - 1].value == ":";
        if !(constructor
            || matches!(token.value, "fill" | "update") && object
            || matches!(token.value, "create" | "make") && static_call)
        {
            continue;
        }
        calls += 1;
        if calls > 128 {
            break;
        }
        let open = t[i + 1].start;
        // receiver_class preserves query/builder shapes; receiver_model alone would warn on builders.
        let full = if constructor {
            resolved_name(token.value, &masked)
        } else {
            let Some(full) = eloquent_members::semantic::receiver_class(index, doc, open, owns)
            else {
                continue;
            };
            full
        };
        if !eloquent_members::model_identities(&index.eloquent_models)
            .iter()
            .any(|(name, path)| name.eq_ignore_ascii_case(&full) && owns(path))
        {
            continue;
        }
        let exemptions = exceptions
            .iter()
            .find(|(name, _)| name.trim_start_matches('\\').eq_ignore_ascii_case(&full))
            .map(|(_, fields)| fields);
        if exemptions.is_some_and(|fields| fields.iter().any(|field| field == "*")) {
            continue;
        }
        let Some((fillable, guarded)) = profiles
            .entry(full.clone())
            .or_insert_with(|| profile(index, doc, &full, owns, read))
            .as_ref()
        else {
            continue;
        };
        let Some(first) = php::arguments(&masked, open).into_iter().next() else {
            continue;
        };
        let argument = php::trimmed(&masked, first.clone());
        let class = full.rsplit('\\').next().unwrap_or(&full);
        if super::ALL_INPUT.contains(&&masked[argument.clone()]) {
            if fillable.is_empty() && guarded.is_empty() {
                found.push(Finding{start:argument.start,end:argument.end,code:"guarded-attribute-assignment",message:crate::ui_text::format("mass_assignment.unguarded_request", "{class} has `$guarded = []`, so every field of the request is mass assigned. Pass validated() data or list the attributes in `$fillable`.", &[("class",class)]),severity:Severity::Warning,replacement:None});
            }
            continue;
        }
        let Some(array) = php::array_open(&masked, first) else {
            continue;
        };
        for item in php::items(&masked, array) {
            let Some((key, offset)) = item.key.and_then(|range| php::literal(&masked, range))
            else {
                continue;
            };
            if fillable.iter().any(|field| field == key)
                || exemptions.is_some_and(|fields| fields.iter().any(|field| field == key))
            {
                continue;
            }
            let blocked = !fillable.is_empty()
                || guarded == &["*".to_owned()]
                || guarded.iter().any(|field| field.eq_ignore_ascii_case(key));
            if blocked {
                found.push(Finding{start:offset,end:offset+key.len(),code:"guarded-attribute-assignment",message:crate::ui_text::format("mass_assignment.guarded_field", "\"{key}\" is guarded by {class}: mass assignment may discard it or throw a mass assignment exception.", &[("key",key),("class",class)]),severity:Severity::Warning,replacement:None});
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use crate::*;
    use openidea_plugin::{diagnostics::DiagnosticsProvider, mock::MockHost};
    fn setup(host: &MockHost) -> Main {
        let mut plugin = crate::tests::activated(host);
        host.add_file("composer.json",r#"{"require":{"laravel/framework":"^11.0"},"autoload":{"psr-4":{"App\\":"app/","Domain\\":"modules/Domain/"}}}"#);
        host.add_file("modules/Domain/BaseRecord.php",r#"<?php namespace Domain; abstract class BaseRecord extends \Illuminate\Database\Eloquent\Model { protected $fillable = ['title']; }"#);
        host.add_file(
            "modules/Domain/Record.php",
            r#"<?php namespace Domain; class Record extends BaseRecord {}"#,
        );
        plugin.index_project(&host.invocation().project().unwrap());crate::tests::finish_indexing(&mut plugin);
        plugin
    }
    fn check(plugin: &mut Main, source: &str) -> Vec<Diagnostic> {
        plugin
            .diagnostics(&crate::tests::doc("/project/usage.php", "php", source))
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("guarded-attribute-assignment"))
            .collect()
    }
    #[test]
    fn mass_assignment_main_typed_module_inheritance_and_query_negatives() {
        let host = MockHost::install();
        let mut plugin = setup(&host);
        for source in [
            r#"<?php \Domain\Record::create(['secret'=>1]);"#,
            r#"<?php function save(\Domain\Record $record) { $record->fill(['secret'=>1]); }"#,
            r#"<?php (new \Domain\Record)->fill(['secret'=>1]);"#,
        ] {
            assert_eq!(check(&mut plugin, source).len(), 1, "{source}");
        }
        for source in [
            r#"<?php \Domain\Record::query()->create(['secret'=>1]);"#,
            r#"<?php \Illuminate\Support\Facades\DB::table('records')->insert(['secret'=>1]);"#,
            r#"<?php function save($record) { $record->fill(['secret'=>1]); }"#,
            r#"<?php \Domain\Record::create(['title'=>1]);"#,
            r#"<?php $type::create(['secret'=>1]);"#,
        ] {
            assert!(check(&mut plugin, source).is_empty(), "{source}");
        }
        host.add_file("modules/Domain/Record.php",r#"<?php namespace Domain; use Illuminate\Database\Eloquent\Factories\HasFactory; class Record extends BaseRecord {use HasFactory; public function unrelated(){return Other::create([]);}}"#);
        plugin.index_project(&host.invocation().project().unwrap());crate::tests::finish_indexing(&mut plugin);
        assert_eq!(
            check(
                &mut plugin,
                r#"<?php \Domain\Record::create(['secret'=>1]);"#
            )
            .len(),
            1,
            "framework factory trait and unrelated calls preserve guard evidence"
        );
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }
    #[test]
    fn mass_assignment_grouped_imports_resolve_model_attributes_traits_and_constructor_aliases() {
        let host = MockHost::install();
        let mut plugin = setup(&host);
        host.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"laravel/framework","version":"v13.40.0"}]}"#,
        );
        host.add_file("modules/Domain/Record.php",r#"<?php namespace Domain; use Illuminate\Database\Eloquent\{Model, Attributes\Fillable as Allowed, Factories\HasFactory}; #[Allowed(['title'])] class Record extends Model { use HasFactory; }"#);
        plugin.index_project(&host.invocation().project().unwrap());crate::tests::finish_indexing(&mut plugin);
        assert_eq!(
            check(
                &mut plugin,
                r#"<?php use Domain\{Record as Entry}; new Entry(['secret'=>1]);"#
            )
            .len(),
            1
        );
        assert_eq!(
            check(
                &mut plugin,
                r#"<?php use Domain\{Record as Éntry}; new Éntry(['secret'=>1]);"#
            )
            .len(),
            1
        );
        assert!(
            check(
                &mut plugin,
                r#"<?php use Domain\{Record as Entry}; new Entry(['title'=>1]);"#
            )
            .is_empty()
        );
        assert!(
            check(
                &mut plugin,
                r#"<?php use Foreign\{Record as Entry}; new Entry(['secret'=>1]);"#
            )
            .is_empty()
        );
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }
    #[test]
    fn mass_assignment_main_constructor_instance_update_and_native_attributes() {
        let host = MockHost::install();
        let mut plugin = setup(&host);
        for source in [
            r#"<?php new \Domain\Record(['secret'=>1]);"#,
            r#"<?php function run(\Domain\Record $record){$record->update(['secret'=>1]);}"#,
        ] {
            assert_eq!(check(&mut plugin, source).len(), 1, "{source}");
        }
        for source in [
            r#"<?php \Domain\Record::update(['secret'=>1]);"#,
            r#"<?php \Domain\Record::query()->update(['secret'=>1]);"#,
            r#"<?php new $class(['secret'=>1]);"#,
        ] {
            assert!(check(&mut plugin, source).is_empty(), "{source}");
        }
        for (attribute, property, expected) in [
            ("#[Fillable(['title'])]", "", 1),
            ("#[Fillable('title', 'secret')]", "", 0),
            ("#[Guarded(['secret'])]", "", 1),
            ("#[Unguarded]", "", 0),
            ("#[Fillable(['title']), Unguarded]", "", 1),
            ("#[Guarded(['secret'])]", "protected $guarded=['role'];", 0),
            (
                "#[Fillable(['secret'])]",
                "protected $fillable=['title'];",
                0,
            ),
        ] {
            let source = format!(
                "<?php namespace Domain;use Illuminate\\Database\\Eloquent\\Attributes\\Fillable;use Illuminate\\Database\\Eloquent\\Attributes\\Guarded;use Illuminate\\Database\\Eloquent\\Attributes\\Unguarded;{attribute} class Record extends \\Illuminate\\Database\\Eloquent\\Model {{{property}}}"
            );
            host.add_file("modules/Domain/Record.php", &source);
            for version in ["v12.72.0", "v13.0.0", "v13.2.0", "v13.3.0", "v13.40.0"] {
                host.add_file(
                    "composer.lock",
                    &format!(
                        r#"{{"packages":[{{"name":"laravel/framework","version":"{version}"}}]}}"#
                    ),
                );
                plugin.index_project(&host.invocation().project().unwrap());crate::tests::finish_indexing(&mut plugin);
                let expected = if !version.starts_with("v13")
                    || matches!(version, "v13.0.0" | "v13.2.0")
                        && attribute.contains("'title', 'secret'")
                {
                    0
                } else if matches!(version, "v13.0.0" | "v13.2.0")
                    && attribute == "#[Fillable(['secret'])]"
                {
                    1
                } else {
                    expected
                };
                assert_eq!(
                    check(
                        &mut plugin,
                        r#"<?php \Domain\Record::create(['title'=>1,'secret'=>2]);"#
                    )
                    .len(),
                    expected,
                    "{version}: {attribute} {property}"
                );
            }
        }
        host.add_file("modules/Domain/Record.php",r#"<?php namespace Domain;#[\Illuminate\Database\Eloquent\Attributes\Guarded(['secret'])] class Record extends \Illuminate\Database\Eloquent\Relations\Pivot {}"#);
        for (version, expected) in [("v13.23.0", 0), ("v13.24.0", 1)] {
            host.add_file(
                "composer.lock",
                &format!(
                    r#"{{"packages":[{{"name":"laravel/framework","version":"{version}"}}]}}"#
                ),
            );
            plugin.index_project(&host.invocation().project().unwrap());crate::tests::finish_indexing(&mut plugin);
            assert_eq!(
                check(&mut plugin, r#"<?php new \Domain\Record(['secret'=>1]);"#).len(),
                expected,
                "pivot attribute default {version}"
            );
        }
        host.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"laravel/framework","version":"v13.40.0"}]}"#,
        );
        host.add_file("modules/Domain/BaseRecord.php",r#"<?php namespace Domain;#[\Illuminate\Database\Eloquent\Attributes\Fillable(['body'])] abstract class BaseRecord extends \Illuminate\Database\Eloquent\Model {protected $fillable=['title'];}"#);
        host.add_file("modules/Domain/Record.php",r#"<?php namespace Domain;class Record extends BaseRecord {protected $fillable=['secret'];}"#);
        plugin.index_project(&host.invocation().project().unwrap());crate::tests::finish_indexing(&mut plugin);
        assert_eq!(
            check(
                &mut plugin,
                r#"<?php new \Domain\Record(['title'=>1,'secret'=>1,'body'=>1]);"#
            )
            .len(),
            1,
            "nearest inherited attribute merges with effective property"
        );
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }
    #[test]
    fn mass_assignment_main_exceptions_settings_stale_and_foreign_sources() {
        let host = MockHost::install();
        let mut plugin = setup(&host);
        let source = r#"<?php \Domain\Record::create(['secret'=>1,'role'=>2]);"#;
        assert_eq!(check(&mut plugin, source).len(), 2);
        host.set_setting(
            "massAssignmentExceptions",
            r#"{"Domain\\Record":["secret"]}"#,
        );
        assert_eq!(check(&mut plugin, source).len(), 1);
        host.set_setting("massAssignmentExceptions", r#"{"Domain\\Record":["*"]}"#);
        assert!(check(&mut plugin, source).is_empty());
        host.set_setting("massAssignmentExceptions", "{}");
        host.add_file("modules/Domain/BaseRecord.php",r#"<?php namespace Domain; abstract class BaseRecord extends \Illuminate\Database\Eloquent\Model { protected $guarded = []; }"#);
        assert!(
            check(&mut plugin, source).is_empty(),
            "changed source must not reuse old fillable"
        );
        plugin.index_project(&host.invocation().project().unwrap());crate::tests::finish_indexing(&mut plugin);
        assert!(check(&mut plugin, source).is_empty());
        let all = r#"<?php function save(\Domain\Record $record){$record->fill($request->all());}"#;
        assert_eq!(check(&mut plugin, all).len(), 1);
        host.add_project("/project/modules/Domain");
        assert!(check(&mut plugin, all).is_empty());
    }
    #[test]
    fn mass_assignment_main_current_policy_overlay_guarded_and_dynamic_negatives() {
        let host = MockHost::install();
        let mut plugin = setup(&host);
        let source = r#"<?php namespace Domain; class Record extends BaseRecord {protected $fillable=['secret'];function run(){ $this->fill(['secret'=>1,'title'=>2]); }}"#;
        let doc = crate::tests::doc("/project/modules/Domain/Record.php", "php", source);
        let found = plugin.diagnostics(&doc);
        assert_eq!(
            found
                .iter()
                .filter(|d| d.code.as_deref() == Some("guarded-attribute-assignment"))
                .count(),
            1
        );
        assert!(found.iter().any(|d| d.message.contains("title")));
        let source = r#"<?php namespace Domain; class Record extends \Illuminate\Database\Eloquent\Model {protected $guarded=[];function run(){ $this->fill(['secret'=>1]); }}"#;
        let doc = crate::tests::doc("/project/modules/Domain/Record.php", "php", source);
        assert!(
            plugin
                .diagnostics(&doc)
                .iter()
                .all(|d| d.code.as_deref() != Some("guarded-attribute-assignment")),
            "current overlay parent supersedes indexed inheritance"
        );
        for declaration in [
            "protected $fillable = SOME_FIELDS;",
            "protected $fillable = ['title', ...FIELDS];",
            "protected $fillable = ['title']; public function isFillable($key){return true;}",
            "use CustomGuards; protected $fillable=['title'];",
        ] {
            host.add_file("modules/Domain/BaseRecord.php",&format!("<?php namespace Domain; abstract class BaseRecord extends \\Illuminate\\Database\\Eloquent\\Model {{{declaration}}}"));
            plugin.index_project(&host.invocation().project().unwrap());crate::tests::finish_indexing(&mut plugin);
            assert!(
                check(
                    &mut plugin,
                    r#"<?php \Domain\Record::create(['secret'=>1]);"#
                )
                .is_empty(),
                "{declaration}"
            );
        }
        host.add_file("modules/Domain/BaseRecord.php",r#"<?php namespace Domain; abstract class BaseRecord extends \Illuminate\Database\Eloquent\Model {protected $guarded=['secret'];}"#);
        plugin.index_project(&host.invocation().project().unwrap());crate::tests::finish_indexing(&mut plugin);
        assert_eq!(
            check(
                &mut plugin,
                r#"<?php \Domain\Record::create(['secret'=>1,'title'=>2]);"#
            )
            .len(),
            1
        );
    }
}
