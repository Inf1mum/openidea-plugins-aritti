//! Literal Thymeleaf fragment expressions. Dynamic selectors are not symbols.
use openidea_plugin::{Range, text};
use spring_text::xml;

pub struct Reference {
    pub template: String,
    pub fragment: Option<String>,
}
fn name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-'))
}
pub fn reference(source: &str, at: usize) -> Option<Reference> {
    let attr = xml::elements(source)
        .into_iter()
        .flat_map(|e| e.attrs)
        .find(|a| {
            a.holds(at)
                && matches!(
                    a.name.as_str(),
                    "th:insert"
                        | "th:replace"
                        | "th:include"
                        | "data-th-insert"
                        | "data-th-replace"
                )
        })?;
    let raw = attr.value.trim();
    let raw = raw
        .strip_prefix("~{")
        .and_then(|s| s.strip_suffix('}'))
        .unwrap_or(raw);
    let (template, fragment) = raw
        .split_once("::")
        .map_or((raw.trim(), None), |(t, f)| (t.trim(), Some(f.trim())));
    let fragment = match fragment {
        Some(f) if name(f) => Some(f.to_owned()),
        Some(_) => return None,
        None => None,
    };
    if template.split('/').any(|part| matches!(part, "." | ".."))
        || !template
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '/' | '_' | '-' | '.'))
    {
        return None;
    }
    if template.is_empty() && fragment.is_none() {
        return None;
    }
    Some(Reference {
        template: template.to_owned(),
        fragment,
    })
}
pub fn declarations(source: &str, wanted: &str) -> Vec<Range> {
    xml::elements(source)
        .into_iter()
        .flat_map(|e| e.attrs)
        .filter(|a| matches!(a.name.as_str(), "th:fragment" | "data-th-fragment"))
        .filter_map(|a| {
            let value = a.value.trim_start();
            let id = value.split(['(', ' ']).next()?;
            if id != wanted || !name(id) {
                return None;
            }
            let start = a.value_at + a.value.len() - value.len();
            Some(Range::new(
                text::position(source, start),
                text::position(source, start + id.len()),
            ))
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_fragment_references_skip_comments_and_dynamic_selectors() {
        let source = "<!-- <div th:replace='~{parts :: title}'/> --><div th:replace='~{parts :: title}'/><div th:replace='~{${view} :: title}'/>";
        assert!(reference(source, source.find("parts").unwrap()).is_none());
        let found = reference(source, source.rfind("parts").unwrap()).unwrap();
        assert_eq!(found.template, "parts");
        assert_eq!(found.fragment.as_deref(), Some("title"));
        assert!(reference(source, source.find("${view}").unwrap()).is_none());
    }
}

pub enum Slot {
    Template(Range),
    Fragment { template: String, replace: Range },
}
pub fn completion(source: &str, at: usize) -> Option<Slot> {
    let attr = xml::elements(source)
        .into_iter()
        .flat_map(|e| e.attrs)
        .find(|a| {
            a.holds(at)
                && matches!(
                    a.name.as_str(),
                    "th:insert"
                        | "th:replace"
                        | "th:include"
                        | "data-th-insert"
                        | "data-th-replace"
                )
        })?;
    let raw = attr.value.trim();
    let from = attr.value_at + attr.value.len() - attr.value.trim_start().len();
    let (content, from) = if let Some(inner) = raw.strip_prefix("~{") {
        (inner.strip_suffix('}').unwrap_or(inner), from + 2)
    } else {
        (raw, from)
    };
    if content.contains(['$', '#', '{', '}', '(', ')']) {
        return None;
    }
    let trimmed_range = |value: &str, base: usize| {
        let start = base + value.len() - value.trim_start().len();
        let end = base + value.trim_end().len();
        Range::new(
            text::position(source, start),
            text::position(source, end.max(start)),
        )
    };
    if let Some((template, fragment)) = content.split_once("::") {
        let separator = from + template.len();
        if at > separator + 1 {
            return Some(Slot::Fragment {
                template: template.trim().into(),
                replace: trimmed_range(fragment, separator + 2),
            });
        }
        return Some(Slot::Template(trimmed_range(template, from)));
    }
    Some(Slot::Template(trimmed_range(content, from)))
}
pub fn fragment_names(source: &str) -> Vec<String> {
    let mut names = xml::elements(source)
        .into_iter()
        .flat_map(|e| e.attrs)
        .filter(|a| matches!(a.name.as_str(), "th:fragment" | "data-th-fragment"))
        .filter_map(|a| {
            let id = a.value.trim().split(['(', ' ']).next()?.to_owned();
            name(&id).then_some(id)
        })
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

/// Literal Spring-dialect Thymeleaf expressions. Entity escapes have no exact
/// decoded-to-physical range mapping and are intentionally withheld.
pub fn injections(source: &str) -> Vec<openidea_plugin::language_features::Injection> {
    xml::elements(source)
        .into_iter()
        .flat_map(|e| e.attrs)
        .filter_map(|a| {
            if !(a.name.starts_with("th:") || a.name.starts_with("data-th-")) {
                return None;
            }
            let raw = a.value.trim();
            if !(raw.starts_with("${") || raw.starts_with("*{")) || !raw.ends_with('}') {
                return None;
            }
            let inner = &raw[2..raw.len() - 1];
            if inner.trim().is_empty() || inner.contains('&') {
                return None;
            }
            let mut quote = None;
            let mut escaped = false;
            let mut braces = 0i32;
            for ch in inner.chars() {
                if let Some(delimiter) = quote {
                    if escaped {
                        escaped = false;
                    } else if ch == '\\' {
                        escaped = true;
                    } else if ch == delimiter {
                        quote = None;
                    }
                } else if matches!(ch, '\'' | '"') {
                    quote = Some(ch);
                } else if ch == '{' {
                    braces += 1;
                } else if ch == '}' {
                    braces -= 1;
                    if braces < 0 {
                        return None;
                    }
                }
            }
            if quote.is_some() || braces != 0 {
                return None;
            }
            let start = a.value_at + a.value.len() - a.value.trim_start().len() + 2;
            Some(openidea_plugin::language_features::Injection {
                range: Range::new(
                    text::position(source, start),
                    text::position(source, start + inner.len()),
                ),
                language: "spel".into(),
            })
        })
        .take(256)
        .collect()
}

#[cfg(test)]
mod injection_tests {
    use super::*;
    #[test]
    fn registered_owner_requires_explicit_language_opt_in() {
        use openidea_plugin::language_features::{Document, Injections};
        let host = openidea_plugin::mock::MockHost::install();
        host.add_file("pom.xml", "spring-web");
        let doc = Document {
            path: "/project/src/main/resources/templates/home.html".into(),
            language: "html".into(),
            version: 1,
            text: "<p th:text=\"${user.name}\"/>".into(),
        };
        let mut owner = crate::window::Endpoints::default();
        assert!(owner.injections(&doc).is_empty());
        host.set_setting("springWeb.thymeleafSpelInjection", "true");
        assert_eq!(owner.injections(&doc).len(), 1);
    }
    #[test]
    fn only_exact_single_expression_attributes_are_injected() {
        let source = "<!-- <p th:text='${ignored}'/> --><p th:text=\"${user.name}\"/><p data-th-text=\"*{name}\"/><p th:text=\"${a &amp;&amp; b}\"/><p th:text=\"${a} + ${b}\"/>";
        let found = injections(source);
        assert_eq!(found.len(), 2);
        let snippets = found
            .iter()
            .map(|i| {
                &source[text::offset(source, i.range.start).unwrap()
                    ..text::offset(source, i.range.end).unwrap()]
            })
            .collect::<Vec<_>>();
        assert_eq!(snippets, ["user.name", "name"]);
        assert!(found.iter().all(|i| i.language == "spel"));
    }
}
