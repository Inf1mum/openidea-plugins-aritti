//! Gutter markers between entity attributes and the converter or custom type they use.
//!
//! - On an attribute without an explicit `@Convert`: a marker to the Hibernate custom type named by `@Type(X.class)`, or to the
//!   one `@Converter(autoApply = true)` converter whose attribute type is exactly the attribute's type.
//! - On a converter or custom type class: a marker to the first attribute that uses it, the tooltip naming the others.
//!
//! Source is read lexically; a name is accepted only when the explicit import, the qualified name or the shared package proves
//! it. Anything else, an incomplete project scan included, stays without marker. The index is read in small slices.
use crate::model::{Kind, annotations, has_word, mask, parse_class};
use openidea_plugin::providers::{Document, LineMarker};
use openidea_plugin::text::{offset, position};
use openidea_plugin::{Event, Location, Position, Project, Range};
use regex_lite::Regex;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const MAX_TEXT: usize = 512 * 1024;
const MAX_FILES: usize = 5_000;
const PROVIDER_BUDGET: Duration = Duration::from_millis(20);
const EVENT_BUDGET: Duration = Duration::from_millis(200);
const MAX_NAMED_USERS: usize = 3;
const CUSTOM_TYPE_BASES: [&str; 5] = ["UserType", "CompositeUserType", "UserVersionType", "EnhancedUserType", "ParameterizedType"];
const JAVA_LANG: [&str; 9] = ["String", "Long", "Integer", "Short", "Byte", "Boolean", "Double", "Float", "Character"];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Role {
    Converter,
    CustomType,
}

#[derive(Clone, Debug)]
struct Declared {
    fqn: String,
    name: String,
    path: String,
    at: Position,
    role: Role,
    auto_apply: bool,
    converts: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum Explicit {
    Nothing,
    Convert(Option<String>),
    Type(Option<String>),
}

#[derive(Clone, Debug)]
struct Attr {
    path: String,
    entity: String,
    name: String,
    at: Position,
    explicit: Explicit,
    implicit_type: Option<String>,
}

#[derive(Default)]
struct Facts {
    declared: Vec<Declared>,
    attrs: Vec<Attr>,
}

#[derive(Default)]
struct Index {
    listed: bool,
    truncated: bool,
    unread: Vec<String>,
    files: HashMap<String, Facts>,
}

impl Index {
    fn complete(&self) -> bool {
        self.listed && !self.truncated && self.unread.is_empty()
    }

    fn declared(&self) -> impl Iterator<Item = &Declared> {
        self.files.values().flat_map(|facts| facts.declared.iter())
    }

    fn attrs(&self) -> impl Iterator<Item = &Attr> {
        self.files.values().flat_map(|facts| facts.attrs.iter())
    }

    fn auto_apply_for(&self, converts: &str) -> Vec<&Declared> {
        self.declared().filter(|d| d.role == Role::Converter && d.auto_apply && d.converts.as_deref() == Some(converts)).collect()
    }

    fn advance(&mut self, project: &Project, budget: Duration, limit: usize) {
        if !self.listed {
            self.listed = true;
            for pattern in ["**/*.java", "**/*.kt"] {
                let found = project.find_files(pattern, limit as u32).unwrap_or_default();
                self.truncated |= found.len() >= limit;
                self.unread.extend(found);
            }
        }

        let start = Instant::now();
        while start.elapsed() < budget {
            let Some(path) = self.unread.pop() else {
                break;
            };
            match project.read_file(&path) {
                Ok(text) => self.observe(&path, &text),
                Err(_) => {
                    self.files.remove(&path);
                }
            }
        }
    }

    fn observe(&mut self, path: &str, text: &str) {
        let facts = facts_of(path, text);
        if facts.declared.is_empty() && facts.attrs.is_empty() {
            self.files.remove(path);
            return;
        }
        self.files.insert(path.to_owned(), facts);
    }

    fn forget(&mut self, path: &str) {
        self.files.remove(path);
        self.unread.retain(|p| p != path);
    }
}

thread_local! {
    static INDEXES: RefCell<HashMap<String, Index>> = RefCell::new(HashMap::new());
}

fn declaration() -> &'static Regex {
    static DECLARATION: OnceLock<Regex> = OnceLock::new();
    DECLARATION.get_or_init(|| Regex::new(r"\b(class|object)\s+(\w+)").unwrap())
}

fn package_of(masked: &str) -> Option<String> {
    let line = masked.lines().map(str::trim).find(|line| line.starts_with("package "))?;
    let name = line["package ".len()..].trim().trim_end_matches(';').trim();
    (!name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')).then(|| name.to_owned())
}

fn imports_of(masked: &str) -> Vec<String> {
    masked
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("import "))
        .map(|rest| rest.trim().trim_end_matches(';').trim().to_owned())
        .filter(|import| !import.starts_with("static "))
        .collect()
}

fn resolve(name: &str, package: Option<&str>, imports: &[String]) -> Option<String> {
    let name = name.trim();
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.') {
        return None;
    }
    if name.contains('.') {
        return Some(name.to_owned());
    }
    let suffix = format!(".{name}");
    if let Some(import) = imports.iter().find(|import| import.ends_with(&suffix)) {
        return Some(import.clone());
    }
    if JAVA_LANG.contains(&name) {
        return Some(format!("java.lang.{name}"));
    }
    package.map(|package| format!("{package}.{name}"))
}

fn depth_at(masked: &str, at: usize) -> i32 {
    masked[..at].bytes().fold(0, |depth, b| match b {
        b'{' => depth + 1,
        b'}' => depth - 1,
        _ => depth,
    })
}

fn declared_in(path: &str, text: &str, masked: &str) -> Vec<Declared> {
    let mentions_converter = text.contains("AttributeConverter");
    let mentions_type = CUSTOM_TYPE_BASES.iter().any(|base| text.contains(base));
    if !mentions_converter && !mentions_type {
        return Vec::new();
    }

    let package = package_of(masked);
    let imports = imports_of(masked);
    let mut found = Vec::new();
    for caps in declaration().captures_iter(masked) {
        let (keyword, name) = (caps.get(1).unwrap(), caps.get(2).unwrap());
        if depth_at(masked, keyword.start()) != 0 {
            continue;
        }
        let header_end = masked[name.end()..].find('{').map_or(masked.len(), |at| name.end() + at);
        let header = &masked[name.end()..header_end];
        let role = if mentions_converter && has_word(header, "AttributeConverter") {
            Role::Converter
        } else if mentions_type && CUSTOM_TYPE_BASES.iter().any(|base| has_word(header, base)) {
            Role::CustomType
        } else {
            continue;
        };

        let window = masked.get(keyword.start().saturating_sub(300)..keyword.start()).unwrap_or_default();
        let auto_apply = window.rfind("@Converter").is_some_and(|at| {
            let compact: String = window[at..].chars().filter(|c| !c.is_whitespace()).collect();
            compact.contains("autoApply=true")
        });
        let converts = (role == Role::Converter).then(|| first_type_argument(header, "AttributeConverter")).flatten().and_then(|ty| {
            resolve(&ty, package.as_deref(), &imports)
        });
        found.push(Declared {
            fqn: package.as_ref().map_or_else(|| name.as_str().to_owned(), |package| format!("{package}.{}", name.as_str())),
            name: name.as_str().to_owned(),
            path: path.to_owned(),
            at: position(text, name.start()),
            role,
            auto_apply,
            converts,
        });
    }
    found
}

fn first_type_argument(header: &str, base: &str) -> Option<String> {
    let rest = header[header.find(base)? + base.len()..].trim_start().strip_prefix('<')?;
    let end = rest.find([',', '>'])?;
    let ty = rest[..end].trim();
    (!ty.contains('<')).then(|| ty.to_owned())
}

pub(crate) fn member_start(masked: &str, name_at: usize) -> usize {
    let bytes = masked.as_bytes();
    let mut depth = 0;
    let mut i = name_at;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b')' => depth += 1,
            b'(' if depth > 0 => depth -= 1,
            b';' | b'{' | b'}' if depth == 0 => return i + 1,
            _ => {}
        }
    }
    0
}

fn class_literal(raw: &str) -> Option<&str> {
    let raw = raw.trim();
    let raw = raw.strip_suffix(".class").or_else(|| raw.strip_suffix("::class"))?;
    Some(raw.trim())
}

fn named_argument<'a>(args: &'a str, name: &str) -> Option<&'a str> {
    let at = args.find(name)?;
    let rest = args[at + name.len()..].trim_start().strip_prefix('=')?;
    let end = rest.find(',').unwrap_or(rest.len());
    Some(rest[..end].trim())
}

fn attrs_in(path: &str, text: &str, masked: &str, package: Option<&str>, imports: &[String]) -> Vec<Attr> {
    let Some(class) = parse_class(text, path) else {
        return Vec::new();
    };
    if class.kind != Kind::Entity {
        return Vec::new();
    }

    let all = annotations(masked);
    let mut attrs = Vec::new();
    let mut previous_end = 0;
    for attribute in &class.attributes {
        let Some(name_at) = offset(text, attribute.pos) else {
            continue;
        };
        let start = member_start(masked, name_at).max(previous_end);
        previous_end = name_at + attribute.name.len();
        let own: Vec<_> = all.iter().filter(|a| a.start >= start && a.start < name_at).collect();
        let args_of = |a: &crate::model::Annotation| a.args.map(|(open, close)| text[open..close].to_owned()).unwrap_or_default();

        let mut explicit = Explicit::Nothing;
        let mut blocks_implicit = false;
        for annotation in &own {
            match annotation.name {
                "Convert" => {
                    let args = args_of(annotation);
                    let compact: String = args.chars().filter(|c| !c.is_whitespace()).collect();
                    if compact.contains("disableConversion=true") {
                        explicit = Explicit::Convert(None);
                        continue;
                    }
                    let converter = named_argument(&args, "converter").and_then(class_literal);
                    explicit = Explicit::Convert(converter.and_then(|name| resolve(name, package, imports)));
                }
                "Converts" => explicit = Explicit::Convert(None),
                "Type" if explicit == Explicit::Nothing => {
                    let args = args_of(annotation);
                    let first = args.split(',').next().unwrap_or_default().trim();
                    let first = first.strip_prefix("value").map_or(first, |rest| rest.trim_start().strip_prefix('=').unwrap_or(rest).trim());
                    explicit = Explicit::Type(class_literal(first).and_then(|name| resolve(name, package, imports)));
                }
                "Temporal" | "Version" | "Enumerated" | "Lob" | "Id" | "EmbeddedId" => blocks_implicit = true,
                _ => {}
            }
        }

        let plain_type = !attribute.ty.contains(['<', '[', '?']) && !attribute.ty.is_empty();
        let implicit_type = (explicit == Explicit::Nothing
            && !blocks_implicit
            && plain_type
            && attribute.relation == crate::model::Relation::None
            && !attribute.id
            && !attribute.lob
            && attribute.enum_string.is_none())
        .then(|| resolve(&attribute.ty, package, imports))
        .flatten();

        attrs.push(Attr { path: path.to_owned(), entity: class.name.clone(), name: attribute.name.clone(), at: attribute.pos, explicit, implicit_type });
    }
    attrs
}

fn facts_of(path: &str, text: &str) -> Facts {
    if text.len() > MAX_TEXT || crate::model::language_of(path).is_none() {
        return Facts::default();
    }
    let masked = mask(text);
    let package = package_of(&masked);
    let imports = imports_of(&masked);
    let attrs = if text.contains("@Entity") { attrs_in(path, text, &masked, package.as_deref(), &imports) } else { Vec::new() };
    Facts { declared: declared_in(path, text, &masked), attrs }
}

fn location(path: &str, at: Position) -> Location {
    Location { path: path.to_owned(), range: Range::new(at, at) }
}

fn role_label(role: Role) -> &'static str {
    match role {
        Role::Converter => "converter",
        Role::CustomType => "custom type",
    }
}

fn users_of<'a>(index: &'a Index, declared: &Declared) -> Vec<&'a Attr> {
    let implicit_unique = declared.role == Role::Converter
        && declared.auto_apply
        && index.complete()
        && declared.converts.as_deref().is_some_and(|converts| index.auto_apply_for(converts).len() == 1);
    let mut users: Vec<&Attr> = index
        .attrs()
        .filter(|attr| match &attr.explicit {
            Explicit::Convert(Some(fqn)) | Explicit::Type(Some(fqn)) => *fqn == declared.fqn,
            Explicit::Convert(None) | Explicit::Type(None) => false,
            Explicit::Nothing => implicit_unique && attr.implicit_type.is_some() && attr.implicit_type == declared.converts,
        })
        .collect();
    users.sort_by(|a, b| (&a.path, a.at.line, a.at.character).cmp(&(&b.path, b.at.line, b.at.character)));
    users
}

fn markers_with(index: &Index, doc: &Document) -> Vec<LineMarker> {
    let Some(facts) = index.files.get(&doc.path) else {
        return Vec::new();
    };
    let mut markers = Vec::new();

    for attr in &facts.attrs {
        let target = match &attr.explicit {
            Explicit::Type(Some(fqn)) => index.declared().find(|d| d.fqn == *fqn),
            Explicit::Nothing if index.complete() => {
                let candidates = attr.implicit_type.as_deref().map(|ty| index.auto_apply_for(ty)).unwrap_or_default();
                match candidates.as_slice() {
                    [only] => Some(*only),
                    _ => None,
                }
            }
            _ => None,
        };
        let Some(target) = target else {
            continue;
        };
        markers.push(LineMarker {
            line: attr.at.line,
            icon: Some("navigate".into()),
            tooltip: format!("Uses {} {}", role_label(target.role), target.name),
            target: Some(location(&target.path, target.at)),
        });
    }

    for declared in facts.declared.iter().filter(|d| d.path == doc.path) {
        let users = users_of(index, declared);
        let Some(first) = users.first() else {
            continue;
        };
        let named: Vec<String> = users.iter().take(MAX_NAMED_USERS).map(|a| format!("{}.{}", a.entity, a.name)).collect();
        let more = users.len().saturating_sub(MAX_NAMED_USERS);
        let rest = if more > 0 { format!(" and {more} more") } else { String::new() };
        let partial = if index.complete() { "" } else { " (project scan incomplete)" };
        markers.push(LineMarker {
            line: declared.at.line,
            icon: Some("navigate".into()),
            tooltip: format!("Used by {}{rest}{partial}", named.join(", ")),
            target: Some(location(&first.path, first.at)),
        });
    }

    markers.sort_by_key(|marker| marker.line);
    markers
}

fn relevant(text: &str) -> bool {
    text.contains("@Entity") || text.contains("AttributeConverter") || CUSTOM_TYPE_BASES.iter().any(|base| text.contains(base))
}

/// Markers of `doc`; reads at most one provider budget's worth of unread project files first.
pub(crate) fn markers(doc: &Document) -> Vec<LineMarker> {
    if !relevant(&doc.text) || crate::model::language_of(&doc.path).is_none() {
        return Vec::new();
    }
    let Some(project) = crate::project_of(&doc.path) else {
        return Vec::new();
    };
    let Ok(root) = project.root() else {
        return Vec::new();
    };
    INDEXES.with(|indexes| {
        let mut indexes = indexes.borrow_mut();
        let index = indexes.entry(root).or_default();
        index.advance(&project, PROVIDER_BUDGET, MAX_FILES);
        index.unread.retain(|path| *path != doc.path);
        index.observe(&doc.path, &doc.text);
        markers_with(index, doc)
    })
}

/// Reads the project index outside the provider budget and drops what changed on disk.
pub(crate) fn events(events: &[Event], invocation: &openidea_plugin::Invocation) {
    let Some(project) = invocation.project() else {
        return;
    };
    let Ok(root) = project.root() else {
        return;
    };
    let interesting = |path: &str| path.ends_with(".java") || path.ends_with(".kt");
    let mut opened = false;
    INDEXES.with(|indexes| {
        let mut indexes = indexes.borrow_mut();
        let Some(index) = indexes.get_mut(&root) else {
            opened = events.iter().any(|e| matches!(e, Event::FileOpened(_) | Event::EditorFocused(_)));
            return;
        };
        for event in events {
            match event {
                Event::FilesChanged(batch) => {
                    for path in batch.created.iter().chain(&batch.modified).chain(&batch.removed).filter(|p| interesting(p)) {
                        index.forget(path);
                        index.unread.push(path.clone());
                    }
                }
                Event::FileSaved(path) if interesting(path) => {
                    index.forget(path);
                    index.unread.push(path.clone());
                }
                _ => {}
            }
        }
        index.advance(&project, EVENT_BUDGET, MAX_FILES);
    });
    if opened {
        INDEXES.with(|indexes| indexes.borrow_mut().entry(root).or_default().advance(&project, EVENT_BUDGET, MAX_FILES));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const MONEY_CONVERTER: &str = "package shop;\nimport jakarta.persistence.*;\n@Converter(autoApply = true)\npublic class MoneyConverter implements AttributeConverter<Money, Long> {\n}\n";
    const TAG_CONVERTER: &str = "package shop;\nimport jakarta.persistence.*;\n@Converter\npublic class TagConverter implements AttributeConverter<Tag, String> {\n}\n";
    const JSON_TYPE: &str = "package shop;\npublic class JsonType implements UserType<Object> {\n}\n";
    const ORDER: &str = "package shop;\nimport jakarta.persistence.*;\nimport org.hibernate.annotations.Type;\n@Entity\npublic class Order {\n    @Id Long id;\n    Money total;\n    @Convert(converter = TagConverter.class)\n    Tag tag;\n    @Type(JsonType.class)\n    Object payload;\n    @Convert(converter = Missing.class)\n    Tag other;\n    String note;\n}\n";

    fn host() -> MockHost {
        INDEXES.with(|i| i.borrow_mut().clear());
        let host = MockHost::install().with_capabilities(&["events", "project.read", "editor.read"]);
        host.add_file("src/shop/MoneyConverter.java", MONEY_CONVERTER);
        host.add_file("src/shop/TagConverter.java", TAG_CONVERTER);
        host.add_file("src/shop/JsonType.java", JSON_TYPE);
        host.add_file("src/shop/Order.java", ORDER);
        host
    }

    fn doc(path: &str, text: &str) -> Document {
        Document { path: path.into(), language: if path.ends_with(".kt") { "kotlin" } else { "java" }.into(), version: 1, text: text.into() }
    }

    fn lines(markers: &[LineMarker]) -> Vec<u32> {
        markers.iter().map(|m| m.line).collect()
    }

    #[test]
    fn attributes_point_at_their_custom_type_and_auto_apply_converter() {
        let _host = host();
        let markers = markers(&doc("/project/src/shop/Order.java", ORDER));
        assert_eq!(lines(&markers), vec![6, 10], "{markers:?}");
        assert_eq!(markers[0].tooltip, "Uses converter MoneyConverter");
        assert_eq!(markers[0].target.as_ref().unwrap().path, "/project/src/shop/MoneyConverter.java");
        assert_eq!(markers[0].icon.as_deref(), Some("navigate"));
        assert_eq!(markers[1].tooltip, "Uses custom type JsonType");
        assert_eq!(markers[1].target.as_ref().unwrap().path, "/project/src/shop/JsonType.java");
    }

    #[test]
    fn converters_point_at_the_attributes_using_them() {
        let _host = host();
        let converter = markers(&doc("/project/src/shop/MoneyConverter.java", MONEY_CONVERTER));
        assert_eq!(converter.len(), 1);
        assert_eq!(converter[0].line, 3);
        assert_eq!(converter[0].tooltip, "Used by Order.total");
        let target = converter[0].target.as_ref().unwrap();
        assert_eq!((target.path.as_str(), target.range.start.line), ("/project/src/shop/Order.java", 6));

        let explicit = markers(&doc("/project/src/shop/TagConverter.java", TAG_CONVERTER));
        assert_eq!(explicit[0].tooltip, "Used by Order.tag");
        let custom = markers(&doc("/project/src/shop/JsonType.java", JSON_TYPE));
        assert_eq!(custom[0].tooltip, "Used by Order.payload");
    }

    #[test]
    fn explicit_convert_gets_no_attribute_marker_and_unresolved_names_stay_silent() {
        let _host = host();
        let markers = markers(&doc("/project/src/shop/Order.java", ORDER));
        assert!(!lines(&markers).contains(&8), "explicit @Convert has no attribute marker");
        assert!(!lines(&markers).contains(&12), "unknown converter class stays silent");
        assert!(!lines(&markers).contains(&13), "String attribute has no converter");
    }

    #[test]
    fn same_simple_name_in_another_package_is_not_the_same_type() {
        let host = host();
        host.add_file("src/other/Money.java", "package other;\npublic class Money {}\n");
        let order = ORDER.replace("import jakarta.persistence.*;", "import jakarta.persistence.*;\nimport other.Money;");
        host.add_file("src/shop/Order.java", &order);
        let on_order = markers(&doc("/project/src/shop/Order.java", &order));
        assert!(!lines(&on_order).contains(&7), "{on_order:?}");
        assert!(markers(&doc("/project/src/shop/MoneyConverter.java", MONEY_CONVERTER)).is_empty());
    }

    #[test]
    fn two_auto_apply_converters_for_one_type_are_ambiguous() {
        let host = host();
        host.add_file("src/shop/OtherMoneyConverter.java", &MONEY_CONVERTER.replace("MoneyConverter", "OtherMoneyConverter"));
        assert!(markers(&doc("/project/src/shop/Order.java", ORDER)).iter().all(|m| m.line != 6));
        assert!(markers(&doc("/project/src/shop/MoneyConverter.java", MONEY_CONVERTER)).is_empty());
    }

    #[test]
    fn conversion_blocked_attributes_and_non_entities_stay_silent() {
        let host = host();
        let source = "package shop;\nimport jakarta.persistence.*;\n@Entity\nclass Line {\n    @Id Long id;\n    @Convert(disableConversion = true)\n    Money a;\n    @Enumerated\n    Money b;\n    @Version\n    Money c;\n    java.util.List<Money> d;\n}\n";
        host.add_file("src/shop/Line.java", source);
        assert!(markers(&doc("/project/src/shop/Line.java", source)).is_empty());
        let plain = "package shop;\nclass Holder {\n    Money total;\n}\n";
        assert!(markers(&doc("/project/src/shop/Holder.java", plain)).is_empty());
    }

    #[test]
    fn an_incomplete_scan_keeps_implicit_markers_quiet_and_marks_explicit_ones_partial() {
        let host = host();
        INDEXES.with(|i| i.borrow_mut().clear());
        let project = host.invocation().project().unwrap();
        let mut index = Index::default();
        index.advance(&project, Duration::ZERO, 1_000);
        assert!(!index.complete(), "a zero budget reads nothing");
        index.observe("/project/src/shop/Order.java", ORDER);
        let order = markers_with(&index, &doc("/project/src/shop/Order.java", ORDER));
        assert!(order.is_empty(), "{order:?}");

        index.observe("/project/src/shop/JsonType.java", JSON_TYPE);
        let order = markers_with(&index, &doc("/project/src/shop/Order.java", ORDER));
        assert_eq!(lines(&order), vec![10], "the explicit type is proven by what was read");
        let custom = markers_with(&index, &doc("/project/src/shop/JsonType.java", JSON_TYPE));
        assert!(custom[0].tooltip.ends_with("(project scan incomplete)"), "{custom:?}");
    }

    #[test]
    fn the_listing_is_bounded_and_a_truncated_listing_never_completes() {
        let host = host();
        let project = host.invocation().project().unwrap();
        let mut index = Index::default();
        index.advance(&project, Duration::from_secs(5), 2);
        assert!(index.truncated && !index.complete());
        assert!(index.unread.is_empty() || index.files.len() <= 8);
    }

    #[test]
    fn saved_files_are_read_again_and_removed_ones_forgotten() {
        let host = host();
        let order = doc("/project/src/shop/Order.java", ORDER);
        assert_eq!(markers(&order).len(), 2);

        host.add_file("src/shop/MoneyConverter.java", &MONEY_CONVERTER.replace("autoApply = true", "autoApply = false"));
        events(&[Event::FileSaved("/project/src/shop/MoneyConverter.java".into())], &host.invocation());
        assert_eq!(lines(&markers(&order)), vec![10]);
    }

    #[test]
    fn kotlin_sources_resolve_class_literals_and_nullable_types() {
        INDEXES.with(|i| i.borrow_mut().clear());
        let host = MockHost::install().with_capabilities(&["events", "project.read", "editor.read"]);
        let converter = "package shop\nimport jakarta.persistence.*\n@Converter(autoApply = true)\nclass MoneyConverter : AttributeConverter<Money, Long> {\n}\n";
        let entity = "package shop\nimport jakarta.persistence.*\n@Entity\nclass Order {\n    @Id var id: Long? = null\n    var total: Money? = null\n}\n";
        host.add_file("src/shop/MoneyConverter.kt", converter);
        host.add_file("src/shop/Order.kt", entity);
        let markers = markers(&doc("/project/src/shop/Order.kt", entity));
        assert_eq!(lines(&markers), vec![5], "{markers:?}");
        assert_eq!(markers[0].target.as_ref().unwrap().path, "/project/src/shop/MoneyConverter.kt");
    }

    #[test]
    fn the_provider_keeps_the_repository_marker_next_to_the_converter_markers() {
        let host = host();
        host.add_file("src/shop/OrderRepository.java", "package shop;\ninterface OrderRepository extends JpaRepository<Order, Long> {}\n");
        let mut plugin = crate::JpaAssist::default();
        let markers = crate::Providers::line_markers(&mut plugin, &doc("/project/src/shop/Order.java", ORDER));
        let icons: Vec<_> = markers.iter().map(|m| (m.line, m.icon.clone().unwrap_or_default())).collect();
        assert_eq!(icons.iter().filter(|(_, icon)| icon == "navigate").count(), 2, "{icons:?}");
        assert!(markers.iter().any(|m| m.tooltip.starts_with("Repositories:")), "{icons:?}");
        assert!(markers.windows(2).all(|w| w[0].line <= w[1].line));
    }
}
