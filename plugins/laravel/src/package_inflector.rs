//! Bounded interpreter for installed Doctrine English literal rules. Unsupported
//! PHP statements/PCRE constructs withhold inference rather than approximating it.
use super::super::{Kind, Names, Source, metadata_inference};
use regex::{Captures, Regex, RegexBuilder};
use std::collections::BTreeMap;
#[derive(Debug)]
struct Rule {
    pattern: Regex,
    replacement: String,
    regatta: bool,
}
#[derive(Debug)]
pub(crate) struct Inflector {
    singular: Vec<Rule>,
    plural: Vec<Rule>,
    singular_uninflected: Regex,
    plural_uninflected: Regex,
    irregular: BTreeMap<String, String>,
    singular_irregular: BTreeMap<String, String>,
    pub evidence: Vec<String>,
}
fn body<'a>(source: &Source<'a>, name: &str) -> Option<std::ops::Range<usize>> {
    let mut matches = source
        .functions()
        .into_iter()
        .filter(|f| f.name.is_some_and(|at| source.tokens[at].value == name));
    let f = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(f.body + 1..f.end)
}
fn literal(source: &Source, at: usize) -> Option<String> {
    let t = source.tokens.get(at)?;
    if t.kind != Kind::String {
        return None;
    }
    let (value, _) = crate::php::literal(source.text, t.start..t.end)?;
    metadata_inference::decode_literal(value, t.value.as_bytes()[0])
}
fn regex(pattern: &str) -> Option<Regex> {
    if pattern.len() > 512 || pattern.starts_with('/') || pattern.contains(['\n', '\r']) {
        return None;
    }
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .unicode(true)
        .size_limit(128 * 1024)
        .build()
        .ok()
}
fn rules(source: &Source, name: &str) -> Option<Vec<Rule>> {
    let mut at = body(source, name)?.start;
    let end = body(source, name)?.end;
    let mut result = Vec::new();
    while at < end {
        let expected = ["yield", "new", "Transformation", "(", "new", "Pattern", "("];
        for value in expected {
            if !source.is(at, value) {
                return None;
            }
            at += 1;
        }
        let mut pattern = literal(source, at)?;
        at += 1;
        for value in [")", ","] {
            if !source.is(at, value) {
                return None;
            }
            at += 1;
        }
        let replacement = literal(source, at)?;
        if !replacement.is_ascii()
            || replacement.as_bytes().iter().enumerate().any(|(at, byte)| {
                matches!(byte, b'\\' | b'$')
                    && !replacement
                        .as_bytes()
                        .get(at + 1)
                        .is_some_and(u8::is_ascii_digit)
            })
        {
            return None;
        }
        at += 1;
        for value in [")", ";"] {
            if !source.is(at, value) {
                return None;
            }
            at += 1;
        }
        let regatta = pattern == "([ti])a(?<!regatta)$";
        if regatta {
            pattern = "([ti])a$".into();
        }
        result.push(Rule {
            pattern: regex(&pattern)?,
            replacement,
            regatta,
        });
        if result.len() > 128 {
            return None;
        }
    }
    Some(result)
}
fn patterns(source: &Source, name: &str, defaults: &[String]) -> Option<Vec<String>> {
    let range = body(source, name)?;
    let mut at = range.start;
    let mut result = Vec::new();
    while at < range.end {
        if source.is(at, "yield") && source.is(at + 1, "from") {
            for (offset, value) in ["yield", "from", "self", "::", "getDefault", "(", ")", ";"]
                .iter()
                .enumerate()
            {
                if !source.is(at + offset, value) {
                    return None;
                }
            }
            result.extend_from_slice(defaults);
            at += 8;
            continue;
        }
        for value in ["yield", "new", "Pattern", "("] {
            if !source.is(at, value) {
                return None;
            }
            at += 1;
        }
        result.push(literal(source, at)?);
        at += 1;
        for value in [")", ";"] {
            if !source.is(at, value) {
                return None;
            }
            at += 1;
        }
        if result.len() > 512 {
            return None;
        }
    }
    Some(result)
}
impl Inflector {
    pub(crate) fn parse(
        inflectible: &str,
        uninflected: &str,
        evidence: Vec<String>,
    ) -> Option<Self> {
        let source = Source::new(inflectible)?;
        let names = Names::parse(&source)?;
        if ["Pattern", "Transformation", "Substitution", "Word"]
            .iter()
            .any(|name| names.resolve(name) != format!("Doctrine\\Inflector\\Rules\\{name}"))
        {
            return None;
        }

        let singular = rules(&source, "getSingular")?;
        let plural = rules(&source, "getPlural")?;
        let range = body(&source, "getIrregular")?;
        let mut at = range.start;
        let mut irregular = BTreeMap::new();
        let mut singular_irregular = BTreeMap::new();
        while at < range.end {
            for value in ["yield", "new", "Substitution", "(", "new", "Word", "("] {
                if !source.is(at, value) {
                    return None;
                }
                at += 1;
            }
            let from = literal(&source, at)?;
            at += 1;
            for value in [")", ",", "new", "Word", "("] {
                if !source.is(at, value) {
                    return None;
                }
                at += 1;
            }
            let to = literal(&source, at)?;
            at += 1;
            for value in [")", ")", ";"] {
                if !source.is(at, value) {
                    return None;
                }
                at += 1;
            }
            singular_irregular.insert(to.clone(), from.clone());
            irregular.insert(from, to);
            if irregular.len() > 512 {
                return None;
            }
        }
        let source = Source::new(uninflected)?;
        if Names::parse(&source)?.resolve("Pattern") != "Doctrine\\Inflector\\Rules\\Pattern" {
            return None;
        }

        let defaults = patterns(&source, "getDefault", &[])?;
        let singular_patterns = patterns(&source, "getSingular", &defaults)?;
        let plural_patterns = patterns(&source, "getPlural", &defaults)?;
        // Aggregate known English uninflected pattern inventories exceed the per-rule bound.
        let compile = |patterns: Vec<String>| {
            RegexBuilder::new(&format!("^(?:{})$", patterns.join("|")))
                .case_insensitive(true)
                .unicode(true)
                .size_limit(256 * 1024)
                .build()
                .ok()
        };
        Some(Self {
            singular,
            plural,
            singular_uninflected: compile(singular_patterns)?,
            plural_uninflected: compile(plural_patterns)?,
            irregular,
            singular_irregular,
            evidence,
        })
    }
    pub(crate) fn name(&self, full: &str, many: bool) -> Option<String> {
        let base = full.rsplit('\\').next()?;
        if base.len() > 128
            || !base.is_ascii()
            || !base.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return None;
        }
        // Str::studly(...,false), then PHP lcfirst; internal acronym case is retained.
        let mut word = String::new();
        for part in base.split('_').filter(|part| !part.is_empty()) {
            let mut bytes = part.bytes();
            word.push(bytes.next()?.to_ascii_uppercase() as char);
            word.push_str(&part[1..]);
        }
        if word.is_empty() {
            return None;
        }
        word[..1].make_ascii_lowercase();
        if many && ["recommended", "related"].contains(&word.as_str()) {
            return Some(word);
        }
        if (if many {
            &self.plural_uninflected
        } else {
            &self.singular_uninflected
        })
        .is_match(&word)
        {
            return Some(word);
        }
        let lower = word.to_ascii_lowercase();
        let irregular = if many {
            self.irregular.get(&lower).cloned()
        } else {
            self.singular_irregular.get(&lower).cloned()
        };
        let mut result = irregular.unwrap_or_else(|| word.clone());
        if result == word {
            for rule in if many { &self.plural } else { &self.singular } {
                if rule.regatta && lower.ends_with("regatta") {
                    continue;
                }
                if rule.pattern.is_match(&word) {
                    result = rule
                        .pattern
                        .replace_all(&word, |captures: &Captures| {
                            let mut result = String::new();
                            let bytes = rule.replacement.as_bytes();
                            let mut at = 0;
                            while at < bytes.len() {
                                if (bytes[at] == b'\\' || bytes[at] == b'$')
                                    && bytes.get(at + 1).is_some_and(u8::is_ascii_digit)
                                {
                                    at += 1;
                                    let mut group = 0;
                                    while at < bytes.len() && bytes[at].is_ascii_digit() {
                                        group = group * 10 + (bytes[at] - b'0') as usize;
                                        at += 1;
                                    }
                                    result.push_str(captures.get(group).map_or("", |m| m.as_str()));
                                } else {
                                    result.push(bytes[at] as char);
                                    at += 1;
                                }
                            }
                            result
                        })
                        .into_owned();
                    break;
                }
            }
        }
        // Laravel Pluralizer::matchCase applies lowercase when comparison is lowercase.
        if lower == word {
            result.make_ascii_lowercase();
        }
        Some(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires installed Doctrine/Laravel PHP differential fixture"]
    fn installed_english_rules_match_native_php_for_all_lift_default_cardinalities() {
        let folder = std::env::var("OPENIDEA_RELATION_FIXTURE").unwrap();
        let root = std::path::Path::new(&folder);
        let rules = std::fs::read_to_string(
            root.join("vendor/doctrine/inflector/src/Rules/English/Inflectible.php"),
        )
        .unwrap();
        let uninflected = std::fs::read_to_string(
            root.join("vendor/doctrine/inflector/src/Rules/English/Uninflected.php"),
        )
        .unwrap();
        let parsed = Source::new(&rules).expect("bounded installed rule source");
        assert!(
            super::rules(&parsed, "getSingular").is_some(),
            "singular rule grammar/PCRE"
        );
        assert!(
            super::rules(&parsed, "getPlural").is_some(),
            "plural rule grammar/PCRE"
        );
        let inflector = Inflector::parse(&rules, &uninflected, vec![])
            .expect("complete installed rule grammar");
        let words = [
            "Person",
            "People",
            "Child",
            "Children",
            "Mouse",
            "Mice",
            "Status",
            "Statuses",
            "News",
            "Series",
            "HTTPLog",
            "BlogPost",
            "User",
            "Category",
            "Address",
            "Fish",
            "Sheep",
            "Foot",
            "Feet",
            "Quiz",
            "Matrix",
            "Index",
            "Analysis",
            "Criterion",
            "Taxon",
            "Axis",
            "Axes",
            "Regatta",
            "Photo",
            "Recommended",
            "Related",
            "API_Client",
            "StudlyWords",
            "Equipment",
            "Data",
            "Media",
            "Information",
        ];
        let output=std::process::Command::new("php").arg("-r").arg("require $argv[1].'/vendor/autoload.php'; $result=[];foreach(json_decode($argv[2]) as $word){$camel=Illuminate\\Support\\Str::camel($word);$result[$word]=[Illuminate\\Support\\Str::singular($camel),Illuminate\\Support\\Str::plural($camel)];}echo json_encode($result);").arg(&folder).arg(serde_json::to_string(&words.as_slice()).unwrap()).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        for word in words {
            for many in [false, true] {
                assert_eq!(
                    inflector.name(word, many).as_deref(),
                    expected[word][many as usize].as_str(),
                    "{word}, many={many}"
                );
            }
        }
        for (kind, many) in [
            ("HasOne", false),
            ("HasMany", true),
            ("BelongsTo", false),
            ("BelongsToMany", true),
            ("HasOneThrough", false),
            ("HasManyThrough", true),
            ("MorphOne", false),
            ("MorphMany", true),
            ("MorphToMany", true),
            ("MorphedByMany", true),
        ] {
            let mut caps = super::super::Capabilities::default();
            let full = format!("WendellAdriel\\Lift\\Attributes\\Relations\\{kind}");
            caps.lift.insert(full.clone(), vec![]);
            caps.inflector = Inflector::parse(&rules, &uninflected, vec![]);
            let extra = if kind.ends_with("Through") {
                ", through: Other::class"
            } else if kind.starts_with("Morph") || kind == "MorphedByMany" {
                ", morphName: 'owner'"
            } else {
                ""
            };
            for word in words {
                let source = format!(
                    "<?php namespace App;use WendellAdriel\\Lift\\Lift; #[\\{full}(\\Domain\\{word}::class{extra})]class Subject extends Model{{use Lift;}}"
                );
                let facts =
                    super::super::collect(&source, "/project/app/Subject.php", &caps, false);
                assert_eq!(facts.len(), 1, "{kind}/{word}");
                assert_eq!(
                    facts[0].name,
                    expected[word][many as usize].as_str().unwrap(),
                    "{kind}/{word}"
                );
                assert_eq!(facts[0].many, many);
            }
        }
        assert!(inflector.name("État", true).is_none());
        let mut dynamic = rules.clone();
        dynamic = dynamic.replace(
            "yield new Transformation",
            "return custom(); yield new Transformation",
        );
        assert!(Inflector::parse(&dynamic, &uninflected, vec![]).is_none());
    }
}
