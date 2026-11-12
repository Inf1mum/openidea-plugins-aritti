//! Lexical reading of Java type layouts and MapStruct mappers for name links across files.
//!
//! Nothing here resolves a type through a compiler: a type name is proved only by a single-type import, a
//! qualified spelling, or (when neither exists) the package of the file, which wins over on-demand imports.
//! Whatever cannot be proved is `None` and the callers stay silent.
use crate::model::{Annotation, annotations, closing, mask, split_top_level};
use regex_lite::Regex;
use std::sync::OnceLock;

pub const MAX_TEXT: usize = 256 * 1024;
const MODIFIERS: [&str; 10] = ["public", "private", "protected", "static", "final", "transient", "volatile", "abstract", "default", "synchronized"];
const MAPSTRUCT: [&str; 1] = ["org.mapstruct"];
const COLLECTIONS: [&str; 5] = ["List", "Set", "Collection", "Iterable", "SortedSet"];

#[derive(Clone, Debug, PartialEq)]
pub struct Literal {
    pub value: String,
    /// Byte range of the characters between the quotes.
    pub start: usize,
    pub end: usize,
}

pub struct Imports {
    pub package: Option<String>,
    single: Vec<String>,
    wildcard: Vec<String>,
}

impl Imports {
    pub fn read(masked: &str) -> Self {
        let mut imports = Self { package: None, single: Vec::new(), wildcard: Vec::new() };
        for line in masked.lines() {
            let line = line.trim();
            if imports.package.is_none() {
                if let Some(package) = line.strip_prefix("package ") {
                    imports.package = Some(package.trim().trim_end_matches(';').trim().to_owned());
                    continue;
                }
            }
            let Some(import) = line.strip_prefix("import ") else { continue };
            let import = import.trim().trim_end_matches(';').trim();
            if import.starts_with("static ") {
                continue;
            }
            match import.strip_suffix(".*") {
                Some(package) => imports.wildcard.push(package.to_owned()),
                None => imports.single.push(import.to_owned()),
            }
        }
        imports
    }

    /// Whether annotation `a` is `name` of one of `packages`: qualified, single import, or on-demand import.
    pub fn owns(&self, masked: &str, a: &Annotation<'_>, packages: &[&str], name: &str) -> bool {
        if a.name != name {
            return false;
        }
        let spelled = masked[a.start + 1..a.name_end].trim();
        if spelled != name {
            return packages.iter().any(|package| spelled == format!("{package}.{name}"));
        }
        packages.iter().any(|package| self.single.iter().any(|import| *import == format!("{package}.{name}")) || self.wildcard.iter().any(|wild| wild == package))
    }

    /// The qualified name of the type spelled `ty` in this file, when it can be proved.
    pub fn resolve(&self, ty: &str, shadowed: &[String]) -> Option<String> {
        static SHAPE: OnceLock<Regex> = OnceLock::new();
        let shape = SHAPE.get_or_init(|| Regex::new(r"^[A-Za-z_]\w*(?:\.[A-Za-z_]\w*)*$").unwrap());
        if !shape.is_match(ty) {
            return None;
        }
        if ty.contains('.') {
            return ty.starts_with(|c: char| c.is_ascii_lowercase()).then(|| ty.to_owned());
        }
        if shadowed.iter().any(|name| name == ty) {
            return None;
        }
        let suffix = format!(".{ty}");
        let mut imported = self.single.iter().filter(|import| import.ends_with(&suffix));
        if let Some(first) = imported.next() {
            return imported.next().is_none().then(|| first.clone());
        }
        Some(self.package.as_ref().map_or_else(|| ty.to_owned(), |package| format!("{package}.{ty}")))
    }
}

pub struct TypeDecl {
    pub name: String,
    pub kind: String,
    pub keyword_start: usize,
    pub name_span: (usize, usize),
    pub components: Option<(usize, usize)>,
    pub body: (usize, usize),
}

/// Top-level declarations and the names of every nested one.
pub fn type_decls(masked: &str) -> (Vec<TypeDecl>, Vec<String>) {
    static DECL: OnceLock<Regex> = OnceLock::new();
    let decl = DECL.get_or_init(|| Regex::new(r"\b(class|interface|record|enum)\s+([A-Za-z_]\w*)").unwrap());
    let bytes = masked.as_bytes();
    let (mut tops, mut nested) = (Vec::new(), Vec::new());
    let (mut depth, mut scanned) = (0i32, 0usize);
    for caps in decl.captures_iter(masked) {
        let whole = caps.get(0).unwrap();
        for &byte in &bytes[scanned..whole.start()] {
            depth += i32::from(byte == b'{') - i32::from(byte == b'}');
        }
        scanned = whole.start();
        let before = masked[..whole.start()].trim_end();
        if before.ends_with('.') || before.ends_with('@') || before.ends_with("::") {
            continue;
        }
        let name = caps.get(2).unwrap();
        if depth != 0 {
            nested.push(name.as_str().to_owned());
            continue;
        }
        let kind = caps.get(1).unwrap().as_str();
        let mut at = name.end();
        let mut components = None;
        if kind == "record" {
            let Some(open) = masked[at..].find('(').map(|i| i + at) else { continue };
            let Some(close) = closing(bytes, open) else { continue };
            components = Some((open + 1, close));
            at = close + 1;
        }
        let Some(open) = masked[at..].find('{').map(|i| i + at) else { continue };
        let Some(close) = closing(bytes, open) else { continue };
        tops.push(TypeDecl { name: name.as_str().to_owned(), kind: kind.to_owned(), keyword_start: whole.start(), name_span: (name.start(), name.end()), components, body: (open + 1, close) });
    }
    (tops, nested)
}

/// Direct members of a type body: ranges up to and including the closing `;` or the method body.
pub fn members(masked: &str, body: (usize, usize)) -> Vec<(usize, usize)> {
    let bytes = masked.as_bytes();
    let mut found = Vec::new();
    let (mut start, mut parens, mut braces, mut equals) = (body.0, 0i32, 0i32, false);
    for at in body.0..body.1 {
        match bytes[at] {
            b'(' | b'[' => parens += 1,
            b')' | b']' => parens -= 1,
            b'{' => braces += 1,
            b'}' => {
                braces -= 1;
                if braces == 0 && parens == 0 && !equals {
                    found.push((start, at + 1));
                    start = at + 1;
                }
            }
            b'=' if parens == 0 && braces == 0 => {
                let neighbour = |i: Option<usize>| i.and_then(|i| bytes.get(i)).copied();
                let touching = matches!(neighbour(at.checked_sub(1)), Some(b'=' | b'<' | b'>' | b'!')) || neighbour(Some(at + 1)) == Some(b'=');
                equals |= !touching;
            }
            b';' if parens == 0 && braces == 0 => {
                found.push((start, at + 1));
                start = at + 1;
                equals = false;
            }
            _ => {}
        }
    }
    found.retain(|(from, to)| !masked[*from..*to].trim().trim_end_matches(';').trim().is_empty());
    found
}

/// `masked[range]` with every annotation blanked; offsets are those of the range.
fn blanked(masked: &str, range: (usize, usize), all: &[Annotation<'_>]) -> String {
    let mut text = masked[range.0..range.1].as_bytes().to_vec();
    for a in all.iter().filter(|a| a.start >= range.0 && a.end() <= range.1) {
        for byte in &mut text[a.start - range.0..a.end() - range.0] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }
    String::from_utf8(text).expect("ASCII blanks replace whole annotations")
}

fn strip_modifiers(mut text: &str) -> (&str, bool) {
    let mut is_static = false;
    loop {
        text = text.trim_start();
        let word = text.split(|c: char| !c.is_ascii_alphanumeric()).next().unwrap_or("");
        if !MODIFIERS.contains(&word) {
            return (text, is_static);
        }
        is_static |= word == "static";
        text = &text[word.len()..];
    }
}

#[derive(Debug)]
pub struct Field {
    pub name: String,
    pub ty: String,
    pub name_span: (usize, usize),
    pub member: (usize, usize),
}

/// A declared type spelled as one name, possibly with generic arguments; `None` for anything else.
fn type_and_name(declaration: &str, offset: usize) -> Option<(String, String, (usize, usize))> {
    static NAME: OnceLock<Regex> = OnceLock::new();
    let name = NAME.get_or_init(|| Regex::new(r"([A-Za-z_]\w*)\s*$").unwrap());
    let caps = name.captures(declaration)?;
    let found = caps.get(1)?;
    let ty = declaration[..found.start()].trim();
    if ty.is_empty() || split_top_level(ty).len() > 1 || ty.contains("...") {
        return None;
    }
    Some((ty.to_owned(), found.as_str().to_owned(), (offset + found.start(), offset + found.end())))
}

pub fn fields(masked: &str, type_decl: &TypeDecl, all: &[Annotation<'_>]) -> Vec<Field> {
    let mut found = Vec::new();
    if let Some(components) = type_decl.components {
        for (offset, part) in split_top_level(&masked[components.0..components.1]) {
            let range = (components.0 + offset, components.0 + offset + part.len());
            let blank = blanked(masked, range, all);
            let lead = blank.len() - blank.trim_start().len();
            if let Some((ty, name, span)) = type_and_name(blank.trim(), range.0 + lead) {
                found.push(Field { name, ty, name_span: span, member: range });
            }
        }
        return found;
    }
    for member in members(masked, type_decl.body) {
        let blank = blanked(masked, member, all);
        let mut left = blank.as_str();
        let mut depth = 0i32;
        let mut cut = left.len();
        for (at, byte) in left.bytes().enumerate() {
            match byte {
                b'(' | b'<' | b'[' | b'{' => depth += 1,
                b')' | b'>' | b']' | b'}' => depth -= 1,
                b'=' if depth == 0 => {
                    cut = at;
                    break;
                }
                b';' if depth == 0 => {
                    cut = at;
                    break;
                }
                _ => {}
            }
        }
        left = &left[..cut];
        if left.contains('(') || left.contains('{') {
            continue;
        }
        let (stripped, is_static) = strip_modifiers(left);
        if is_static {
            continue;
        }
        let lead = left.len() - stripped.len();
        if let Some((ty, name, span)) = type_and_name(stripped, member.0 + lead) {
            if !matches!(ty.as_str(), "class" | "interface" | "enum" | "record" | "return" | "import" | "package") {
                found.push(Field { name, ty, name_span: span, member });
            }
        }
    }
    found
}

/// The literal string held by an annotation argument written as `"text"` (no escapes, no concatenation).
pub fn string_arguments(text: &str, masked: &str, a: &Annotation<'_>) -> Vec<(String, Option<Literal>)> {
    let Some((from, to)) = a.args else { return Vec::new() };
    split_top_level(&masked[from..to])
        .into_iter()
        .filter_map(|(offset, part)| {
            let eq = part.find('=')?;
            let key = part[..eq].trim().to_owned();
            let value_start = from + offset + eq + 1;
            let raw = &text[value_start..from + offset + part.len()];
            let trimmed = raw.trim();
            let lead = value_start + raw.len() - raw.trim_start().len();
            let literal = (trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') && !trimmed[1..trimmed.len() - 1].contains(['"', '\\', '\n']))
                .then(|| Literal { value: trimmed[1..trimmed.len() - 1].to_owned(), start: lead + 1, end: lead + trimmed.len() - 1 });
            Some((key, literal))
        })
        .collect()
}

#[derive(Debug)]
pub struct Param {
    pub ty: String,
    pub name: String,
    pub target: bool,
    pub context: bool,
}

#[derive(Debug)]
pub struct Mapping {
    pub source: Option<Literal>,
    pub target: Option<Literal>,
}

#[derive(Debug)]
pub struct Method {
    pub ret: String,
    pub params: Vec<Param>,
    pub mappings: Vec<Mapping>,
    pub inherits_inverse: bool,
}

impl Method {
    /// `(source, target)` spelled types of a bean-to-bean method, when exactly one source parameter exists.
    pub fn bean_types(&self) -> Option<(&str, &str)> {
        let mut sources = self.params.iter().filter(|p| !p.target && !p.context);
        let source = sources.next()?;
        if sources.next().is_some() {
            return None;
        }
        let target = match self.params.iter().find(|p| p.target) {
            Some(param) => param.ty.as_str(),
            None if self.ret != "void" => self.ret.as_str(),
            None => return None,
        };
        Some((source.ty.as_str(), target))
    }

    pub fn source_param(&self) -> Option<&Param> {
        self.params.iter().find(|p| !p.target && !p.context)
    }

    /// Like [`bean_types`](Self::bean_types), but a collection of beans counts as the bean.
    pub fn link_types(&self) -> Option<(String, String)> {
        let (source, target) = self.bean_types()?;
        let unwrap = |ty: &str| -> String {
            COLLECTIONS
                .iter()
                .find_map(|name| ty.strip_prefix(name)?.trim_start().strip_prefix('<')?.strip_suffix('>').map(|inner| inner.trim().to_owned()))
                .unwrap_or_else(|| ty.to_owned())
        };
        Some((unwrap(source), unwrap(target)))
    }
}

pub struct Mapper {
    pub imports: Imports,
    pub nested: Vec<String>,
    pub methods: Vec<Method>,
}

impl Mapper {
    pub fn resolve(&self, ty: &str) -> Option<String> {
        self.imports.resolve(ty, &self.nested)
    }
}

fn mapper_method(text: &str, masked: &str, member: (usize, usize), all: &[Annotation<'_>], imports: &Imports) -> Option<Method> {
    let blank = blanked(masked, member, all);
    let open_local = blank.find('(')?;
    let before = blank[..open_local].trim_end();
    let name_at = before.rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))? + 1;
    let name = &before[name_at..];
    let (ret, _) = strip_modifiers(before[..name_at].trim());
    let ret = ret.trim();
    if name.is_empty() || ret.is_empty() || ret.starts_with('<') {
        return None;
    }
    let open = member.0 + open_local;
    let close = closing(masked.as_bytes(), open)?;
    let mut params = Vec::new();
    for (offset, part) in split_top_level(&masked[open + 1..close]) {
        let range = (open + 1 + offset, open + 1 + offset + part.len());
        let flag = |name: &str| all.iter().any(|a| a.start >= range.0 && a.end() <= range.1 && imports.owns(masked, a, &MAPSTRUCT, name));
        let (target, context) = (flag("MappingTarget"), flag("Context"));
        let blank = blanked(masked, range, all);
        let (stripped, _) = strip_modifiers(&blank);
        let (ty, param, _) = type_and_name(stripped.trim(), 0)?;
        params.push(Param { ty, name: param, target, context });
    }
    let own = |a: &&Annotation<'_>| a.start >= member.0 && a.start < open;
    let mappings = all
        .iter()
        .filter(own)
        .filter(|a| imports.owns(masked, a, &MAPSTRUCT, "Mapping"))
        .map(|a| {
            let arguments = string_arguments(text, masked, a);
            let pick = |key: &str| arguments.iter().find(|(name, _)| name == key).and_then(|(_, literal)| literal.clone());
            Mapping { source: pick("source"), target: pick("target") }
        })
        .collect();
    let inherits_inverse = all.iter().filter(own).any(|a| imports.owns(masked, a, &MAPSTRUCT, "InheritInverseConfiguration"));
    Some(Method { ret: ret.to_owned(), params, mappings, inherits_inverse })
}

/// The `@Mapper` interface or class of `text` (Java). `None` for any other file.
pub fn parse_mapper(text: &str) -> Option<Mapper> {
    if text.len() > MAX_TEXT || !text.contains("org.mapstruct") {
        return None;
    }
    let masked = mask(text);
    let imports = Imports::read(&masked);
    let all = annotations(&masked);
    let (tops, nested) = type_decls(&masked);
    let mut previous_end = 0;
    for decl in &tops {
        let owned = all.iter().any(|a| a.start >= previous_end && a.start < decl.keyword_start && imports.owns(&masked, a, &MAPSTRUCT, "Mapper"));
        previous_end = decl.body.1;
        if !owned || !matches!(decl.kind.as_str(), "interface" | "class") {
            continue;
        }
        let methods = members(&masked, decl.body).into_iter().filter_map(|member| mapper_method(text, &masked, member, &all, &imports)).collect();
        return Some(Mapper { imports, nested, methods });
    }
    None
}

/// The first dotted segment of a mapping path and its byte range inside the literal's text.
pub fn first_segment(literal: &Literal) -> Option<(&str, usize, usize)> {
    let segment = literal.value.split('.').next()?;
    let valid = segment.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') && segment.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then_some((segment, literal.start, literal.start + segment.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAPPER: &str = "package shop.web;\nimport org.mapstruct.*;\nimport shop.Customer;\n@Mapper(componentModel = \"spring\")\npublic interface CustomerMapper {\n    @Mapping(source = \"title\", target = \"name\")\n    @Mapping(target = \"id\", ignore = true)\n    CustomerDto toDto(Customer customer);\n\n    @Mappings({@Mapping(source = \"name\", target = \"title\")})\n    void update(CustomerDto dto, @MappingTarget Customer customer, @Context Ctx ctx);\n    default String x(Customer c) { return \"a;b\"; }\n}\n";

    #[test]
    fn mapper_methods_types_and_literals_are_read_with_exact_ranges() {
        let mapper = parse_mapper(MAPPER).unwrap();
        let [to_dto, update, default] = mapper.methods.as_slice() else { panic!("{:?}", mapper.methods) };
        assert_eq!(to_dto.bean_types(), Some(("Customer", "CustomerDto")));
        assert_eq!(update.bean_types(), Some(("CustomerDto", "Customer")));
        assert_eq!(default.mappings.len(), 0);
        let source = to_dto.mappings[0].source.as_ref().unwrap();
        assert_eq!(&MAPPER[source.start..source.end], "title");
        assert_eq!(update.mappings.len(), 1);
        assert_eq!(mapper.resolve("Customer").as_deref(), Some("shop.Customer"));
        assert_eq!(mapper.resolve("CustomerDto").as_deref(), Some("shop.web.CustomerDto"));
    }

    #[test]
    fn unproved_or_foreign_mappers_are_not_read() {
        assert!(parse_mapper("package a;\n@Mapper\ninterface M { B f(A a); }\n").is_none());
        assert!(parse_mapper("package a;\nimport foo.Mapper;\n@Mapper\ninterface M {}\n").is_none());
        let wild = parse_mapper("package a;\nimport org.mapstruct.Mapper;\nimport x.*;\nimport y.Dup;\nimport z.Dup;\n@Mapper\ninterface M { Dup f(A a); }\n").unwrap();
        assert_eq!(wild.resolve("A").as_deref(), Some("a.A"));
        assert_eq!(wild.resolve("Dup"), None);
        assert_eq!(wild.resolve("java.util.List").as_deref(), Some("java.util.List"));
        assert_eq!(wild.resolve("Outer.Inner"), None);
    }

    #[test]
    fn two_source_parameters_have_no_bean_types_and_collections_link_by_element() {
        let mapper = parse_mapper("package a;\nimport org.mapstruct.Mapper;\n@Mapper\npublic abstract class M {\n abstract B f(A a, C c);\n abstract List<B> g(List<A> a);\n}\n").unwrap();
        assert_eq!(mapper.methods[0].bean_types(), None);
        assert_eq!(mapper.methods[1].bean_types(), Some(("List<A>", "List<B>")));
        assert_eq!(mapper.methods[1].link_types(), Some(("A".into(), "B".into())));
    }

    #[test]
    fn fields_and_record_components_have_exact_name_spans() {
        let text = "package a;\n@Entity\nclass E {\n @Id private Long id;\n static int skip;\n List<Order> orders = new ArrayList<>();\n int a, b;\n void m() { int local; }\n}\nrecord R(Long x, @Foo(\"1\") String y) {}\n";
        let masked = mask(text);
        let all = annotations(&masked);
        let (tops, _) = type_decls(&masked);
        assert_eq!(tops.len(), 2);
        let names: Vec<_> = fields(&masked, &tops[0], &all).into_iter().map(|f| (f.name, f.ty)).collect();
        assert_eq!(names, [("id".into(), "Long".into()), ("orders".into(), "List<Order>".into())]);
        let record = fields(&masked, &tops[1], &all);
        assert_eq!(record.iter().map(|f| &text[f.name_span.0..f.name_span.1]).collect::<Vec<_>>(), ["x", "y"]);
    }
}
