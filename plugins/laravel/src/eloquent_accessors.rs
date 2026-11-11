//! Declared Laravel accessor read/write contracts. No callback body evaluation.
use super::*;

const ATTRIBUTE: &str = "Illuminate\\Database\\Eloquent\\Casts\\Attribute";
const LATE_MODEL: &str = "__OpenIDEA_Late_Model__";
const TRAIT_SELF: &str = "__OpenIDEA_Trait_Self__";
fn render_types(tokens: &[&Token<'_>], names: &Names, is_trait: bool) -> Option<String> {
    let types = tokens
        .iter()
        .map(|token| {
            let mut token = **token;
            if is_trait && token.value.eq_ignore_ascii_case("self") {
                token.value = "\\__OpenIDEA_Trait_Self__";
            }
            token
        })
        .collect::<Vec<_>>();
    helper_facts::declared_type(
        &types.iter().collect::<Vec<_>>(),
        names,
        LATE_MODEL,
        is_trait,
    )
}

#[derive(Clone, Debug)]
pub(super) struct Accessor {
    pub name: String,
    pub method_name: String,
    pub read_type: Option<String>,
    pub write_type: Option<String>,
    pub has_getter: Option<bool>,
    pub has_setter: Option<bool>,
    pub entry: Entry,
    pub getter_method: Option<String>,
    pub setter_method: Option<String>,
    pub getter_entry: Option<Entry>,
    pub setter_entry: Option<Entry>,
    pub modern: bool,
}

#[derive(Clone, Debug, Default)]
struct Contract {
    read_type: Option<String>,
    write_type: Option<String>,
    getter: Option<bool>,
    setter: Option<bool>,
}

fn modifiers<'a>(source: &'a Source, function: &Function) -> &'a [Token<'a>] {
    let mut start = function.start;
    while start > 0
        && matches!(
            source.tokens[start - 1].value.to_ascii_lowercase().as_str(),
            "public" | "protected" | "private" | "static" | "final" | "abstract" | "readonly"
        )
    {
        start -= 1;
    }
    &source.tokens[start..function.start]
}
fn declared_return(source: &Source, names: &Names, function: &Function) -> Option<String> {
    if !source.is(function.params.end + 1, ":") {
        return None;
    }
    let tokens = source
        .tokens
        .get(function.params.end + 2..function.body)?
        .iter()
        .collect::<Vec<_>>();
    render_types(
        &tokens,
        names,
        names
            .class
            .as_ref()
            .is_some_and(|(_, at, _, _)| source.is(*at, "trait")),
    )
}
fn first_parameter(source: &Source, names: &Names, function: &Function) -> Option<String> {
    let arguments = container::arguments(source, function.params.start.checked_sub(1)?)?;
    let first = arguments.first()?;
    let tokens = source.tokens.get(first.clone())?;
    let variable = tokens.iter().position(|token| token.kind == Kind::Var)?;
    if variable == 0
        || tokens
            .get(variable.wrapping_sub(1))
            .is_some_and(|token| matches!(token.value, "&" | "..."))
    {
        return None;
    }
    let types = tokens[..variable].iter().collect::<Vec<_>>();
    render_types(
        &types,
        names,
        names
            .class
            .as_ref()
            .is_some_and(|(_, at, _, _)| source.is(*at, "trait")),
    )
}
fn property(method: &str, prefix: &str) -> Option<String> {
    if !method.get(..prefix.len())?.eq_ignore_ascii_case(prefix)
        || !method
            .get(method.len().checked_sub("Attribute".len())?..)?
            .eq_ignore_ascii_case("Attribute")
    {
        return None;
    }
    let inner = method.get(prefix.len()..method.len().checked_sub("Attribute".len())?)?;
    (!inner.is_empty()).then(|| crate::index::separated(inner, '_'))
}
fn method_doc<'a>(source: &'a Source, function: &Function) -> Option<&'a str> {
    let mut begin = function.start;
    loop {
        if begin > 0
            && matches!(
                source.tokens[begin - 1].value.to_ascii_lowercase().as_str(),
                "public" | "protected" | "private" | "static" | "final" | "abstract" | "readonly"
            )
        {
            begin -= 1;
            continue;
        }
        if begin > 0 && source.is(begin - 1, "]") {
            let open = *source.pairs.get(&(begin - 1))?;
            if open > 0 && source.is(open - 1, "#") {
                begin = open - 1;
                continue;
            }
        }
        break;
    }
    let prefix = source.text.get(..source.tokens[begin].start)?.trim_end();
    if !prefix.ends_with("*/") {
        return None;
    }
    let begin = prefix.rfind("/*")?;
    if !prefix[begin..].starts_with("/**") {
        return None;
    }
    Some(&prefix[begin + 3..prefix.len() - 2])
}

// A bounded PHPDoc type grammar: declared names, unions/intersections,
// nullable types, generic arguments, parentheses and array suffixes.
struct DocType<'a, 'b> {
    source: Source<'a>,
    names: &'b Names,
    is_trait: bool,
    at: usize,
    depth: usize,
}
impl DocType<'_, '_> {
    fn union(&mut self) -> Option<String> {
        let mut out = self.atom()?;
        while self.source.is(self.at, "|") || self.source.is(self.at, "&") {
            out.push_str(self.source.tokens[self.at].value);
            self.at += 1;
            out.push_str(&self.atom()?);
        }
        Some(out)
    }
    fn atom(&mut self) -> Option<String> {
        if self.depth >= 8 {
            return None;
        }
        self.depth += 1;
        let nullable = self.source.is(self.at, "?");
        if nullable {
            self.at += 1;
        }
        let mut out = if self.source.is(self.at, "(") {
            self.at += 1;
            let inner = self.union()?;
            if !self.source.is(self.at, ")") {
                return None;
            }
            self.at += 1;
            format!("({inner})")
        } else {
            let token = *self.source.tokens.get(self.at)?;
            if token.kind != Kind::Name {
                return None;
            }
            self.at += 1;
            let mut raw = token.value.to_owned();
            while self.source.is(self.at, "-")
                && self
                    .source
                    .tokens
                    .get(self.at + 1)
                    .is_some_and(|token| token.kind == Kind::Name)
            {
                raw.push('-');
                raw.push_str(self.source.tokens[self.at + 1].value);
                self.at += 2;
            }
            if matches!(
                raw.as_str(),
                "array-key"
                    | "class-string"
                    | "non-empty-string"
                    | "positive-int"
                    | "non-negative-int"
                    | "negative-int"
                    | "non-empty-array"
                    | "list"
                    | "non-empty-list"
                    | "numeric-string"
                    | "scalar"
                    | "resource"
            ) {
                raw
            } else if raw != token.value {
                return None;
            } else {
                render_types(&[&token], self.names, self.is_trait)?
            }
        };
        if self.source.is(self.at, "<") {
            self.at += 1;
            let mut arguments = Vec::new();
            loop {
                if arguments.len() >= 8 {
                    return None;
                }
                arguments.push(self.union()?);
                if !self.source.is(self.at, ",") {
                    break;
                }
                self.at += 1;
            }
            if !self.source.is(self.at, ">") {
                return None;
            }
            self.at += 1;
            out.push('<');
            out.push_str(&arguments.join(","));
            out.push('>');
        }
        while self.source.is(self.at, "[") && self.source.is(self.at + 1, "]") {
            out.push_str("[]");
            self.at += 2;
        }
        self.depth -= 1;
        Some(if nullable { format!("?{out}") } else { out })
    }
}
fn doc_type(raw: &str, names: &Names, is_trait: bool) -> Option<String> {
    if raw.len() > 1024 || raw.contains(['\'', '"', '`', '$', ';', '{', '}']) {
        return None;
    }
    let mut parser = DocType {
        source: Source::new(raw)?,
        names,
        is_trait,
        at: 0,
        depth: 0,
    };
    let value = parser.union()?;
    (parser.at == parser.source.tokens.len()).then_some(value)
}
fn generic_contract(
    source: &Source,
    names: &Names,
    function: &Function,
) -> Option<(String, String)> {
    let doc = method_doc(source, function)?;
    let lines = doc
        .lines()
        .map(|line| line.trim().trim_start_matches('*').trim())
        .collect::<Vec<_>>();
    let returns = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            line.strip_prefix("@return")
                .is_some_and(|tail| tail.starts_with(char::is_whitespace))
        })
        .map(|(at, _)| at)
        .collect::<Vec<_>>();
    let [offset] = returns.as_slice() else {
        return None;
    };
    let clean = std::iter::once(lines[*offset].trim_start_matches("@return").trim())
        .chain(
            lines[*offset + 1..]
                .iter()
                .copied()
                .take_while(|line| !line.starts_with('@')),
        )
        .collect::<Vec<_>>()
        .join(" ");
    let tail = clean.trim_start();
    let open = tail.find('<')?;
    let class = tail[..open].trim();
    if !names.resolve(class).eq_ignore_ascii_case(ATTRIBUTE) {
        return None;
    }
    let mut depth = 0;
    let mut split = None;
    let mut end = None;
    for (at, ch) in tail[open + 1..].char_indices() {
        match ch {
            '<' | '(' | '[' => depth += 1,
            '>' if depth == 0 => {
                end = Some(open + 1 + at);
                break;
            }
            '>' | ')' | ']' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            ',' if depth == 0 => {
                if split.is_some() {
                    return None;
                }
                split = Some(open + 1 + at);
            }
            _ => {}
        }
    }
    let split = split?;
    let end = end?;
    let is_trait = names
        .class
        .as_ref()
        .is_some_and(|(_, at, _, _)| source.is(*at, "trait"));
    Some((
        doc_type(tail[open + 1..split].trim(), names, is_trait)?,
        doc_type(tail[split + 1..end].trim(), names, is_trait)?,
    ))
}
fn equivalent(a: &str, b: &str) -> bool {
    fn canonical(value: &str) -> String {
        let value = if let Some(rest) = value.strip_prefix('?') {
            format!("{rest}|null")
        } else {
            value.into()
        };
        let mut parts = value
            .split('|')
            .map(str::to_ascii_lowercase)
            .collect::<Vec<_>>();
        parts.sort();
        parts.join("|")
    }
    canonical(a) == canonical(b)
}
fn combine(doc: Option<String>, callback: Option<String>) -> Option<String> {
    fn parts(value: &str) -> Vec<String> {
        let value = if let Some(rest) = value.strip_prefix('?') {
            format!("{rest}|null")
        } else {
            value.into()
        };
        let mut depth = 0usize;
        let mut start = 0;
        let mut result = Vec::new();
        for (at, ch) in value.char_indices() {
            match ch {
                '<' | '(' | '[' => depth += 1,
                '>' | ')' | ']' => depth = depth.saturating_sub(1),
                '|' if depth == 0 => {
                    result.push(value[start..at].to_ascii_lowercase());
                    start = at + 1;
                }
                _ => {}
            }
        }
        result.push(value[start..].to_ascii_lowercase());
        result
    }
    fn compatible(doc: &str, native: &str) -> bool {
        if doc == native || native == "mixed" {
            return true;
        }
        if native == "object" && doc.starts_with('\\') {
            return true;
        }
        if let Some((base, _)) = doc.split_once('<') {
            if base == native && (doc.ends_with('>') || doc.ends_with(">[]")) {
                return true;
            }
        }
        match native {
            "string" => {
                matches!(doc, "non-empty-string" | "numeric-string")
                    || doc.starts_with("class-string<")
            }
            "int" => matches!(doc, "positive-int" | "non-negative-int" | "negative-int"),
            "array" => {
                doc.starts_with("list<")
                    || doc.starts_with("non-empty-list<")
                    || doc.starts_with("non-empty-array<")
            }
            _ => false,
        }
    }
    match (doc, callback) {
        (Some(doc), Some(callback)) => {
            if equivalent(&doc, &callback) {
                return Some(callback);
            }
            let native = parts(&callback);
            parts(&doc)
                .iter()
                .all(|part| native.iter().any(|native| compatible(part, native)))
                .then_some(doc)
        }
        (doc, callback) => callback.or(doc),
    }
}
fn callback(
    source: &Source,
    names: &Names,
    range: std::ops::Range<usize>,
    getter: bool,
) -> (Option<bool>, Option<String>) {
    let mut range = range;
    while source.is(range.start, "(") && source.pairs.get(&range.start) == Some(&(range.end - 1)) {
        range = range.start + 1..range.end - 1;
    }
    if range.end == range.start + 1 && source.is(range.start, "null") {
        return (Some(false), None);
    }
    let start = range.start + usize::from(source.is(range.start, "static"));
    let Some(function) = source.functions().into_iter().find(|function| {
        function.start == start
            && function.name.is_none()
            && (function.end == range.end || function.end + 1 == range.end)
    }) else {
        // A first-class callable is always a Closure when its reference resolves;
        // its return/input type still requires a separate indexed callable fact.
        if source.is(range.end.saturating_sub(1), ")") {
            if let Some(open) = source.pairs.get(&(range.end - 1)) {
                if source.is(open + 1, "...") && open + 2 == range.end - 1 {
                    return (Some(true), None);
                }
            }
        }
        return (None, None);
    };
    (
        Some(true),
        if getter {
            declared_return(source, names, &function)
        } else {
            first_parameter(source, names, &function)
        },
    )
}
fn factory(source: &Source, names: &Names, from: usize, to: usize) -> Option<(Contract, usize)> {
    if from >= to {
        return None;
    }
    if source.is(from, "(") {
        let close = *source.pairs.get(&from)?;
        if close >= to {
            return None;
        }
        let (contract, end) = factory(source, names, from + 1, close)?;
        if end != close {
            return None;
        }
        return Some((contract, close + 1));
    }
    let (open, mode) = if source.is(from, "new") {
        let class = source.tokens.get(from + 1)?;
        if class.kind != Kind::Name
            || !names.resolve(class.value).eq_ignore_ascii_case(ATTRIBUTE)
            || !source.is(from + 2, "(")
        {
            return None;
        }
        (from + 2, "make")
    } else {
        let class = source.tokens.get(from)?;
        if class.kind != Kind::Name
            || !names.resolve(class.value).eq_ignore_ascii_case(ATTRIBUTE)
            || !source.is(from + 1, "::")
            || !source.is(from + 3, "(")
        {
            return None;
        }
        let mode = source.tokens.get(from + 2)?.value;
        if !["make", "get", "set"]
            .iter()
            .any(|known| mode.eq_ignore_ascii_case(known))
        {
            return None;
        }
        (from + 3, mode)
    };
    let close = *source.pairs.get(&open)?;
    if close >= to {
        return None;
    }
    let args = container::arguments(source, open)?;
    let mut contract = Contract {
        getter: Some(false),
        setter: Some(false),
        ..Contract::default()
    };
    let mut seen = BTreeSet::new();
    for (position, range) in args.into_iter().enumerate() {
        let (name, value) = if source
            .tokens
            .get(range.start)
            .is_some_and(|token| token.kind == Kind::Name)
            && source.is(range.start + 1, ":")
        {
            (
                source.tokens[range.start].value.to_ascii_lowercase(),
                range.start + 2..range.end,
            )
        } else {
            (
                if mode.eq_ignore_ascii_case("get") {
                    "get"
                } else if mode.eq_ignore_ascii_case("set") {
                    "set"
                } else if position == 0 {
                    "get"
                } else if position == 1 {
                    "set"
                } else {
                    return None;
                }
                .into(),
                range,
            )
        };
        if !matches!(name.as_str(), "get" | "set") || !seen.insert(name.clone()) || value.is_empty()
        {
            return None;
        }
        if mode.eq_ignore_ascii_case("get") && name != "get"
            || mode.eq_ignore_ascii_case("set") && name != "set"
        {
            return None;
        }
        let (present, value_type) = callback(source, names, value, name == "get");
        if name == "get" {
            contract.getter = present;
            contract.read_type = value_type;
        } else {
            contract.setter = present;
            contract.write_type = value_type;
        }
    }
    Some((contract, close + 1))
}
fn body_contract(source: &Source, names: &Names, function: &Function) -> Option<Contract> {
    let start = function.body + 1;
    if !source.is(start, "return") || !source.is(function.end.checked_sub(1)?, ";") {
        return None;
    }
    let end = function.end - 1;
    let (contract, mut at) = factory(source, names, start + 1, end)?;
    while at < end {
        if !source.is(at, "->")
            || !["shouldCache", "withoutObjectCaching"]
                .iter()
                .any(|name| source.is(at + 1, name))
            || !source.is(at + 2, "(")
            || !source.is(at + 3, ")")
        {
            return None;
        }
        at += 4;
    }
    (at == end).then_some(contract)
}

pub(super) fn declared(source: &Source, names: &Names, path: &str) -> BTreeMap<String, Accessor> {
    let Some((_, _, body, _)) = names.class.as_ref() else {
        return BTreeMap::new();
    };
    let mut getters = BTreeMap::new();
    let mut setters = BTreeMap::new();
    let mut modern = BTreeMap::new();
    let mut duplicates = BTreeSet::new();
    let mut methods = BTreeSet::new();
    for function in source
        .functions()
        .into_iter()
        .filter(|function| function.name.is_some() && source.paths[function.start] == vec![*body])
    {
        let at = function.name.unwrap();
        let method = source.tokens[at].value;
        if !methods.insert(method.to_ascii_lowercase()) {
            duplicates.insert(method.to_ascii_lowercase());
        }
        if modifiers(source, &function).iter().any(|token| {
            token.value.eq_ignore_ascii_case("private")
                || token.value.eq_ignore_ascii_case("static")
        }) {
            continue;
        }
        if let Some(name) = property(method, "get") {
            getters.insert(
                name,
                (
                    method.to_owned(),
                    declared_return(source, names, &function)
                        .filter(|kind| !matches!(kind.as_str(), "void" | "never")),
                    entry(source, path, at),
                ),
            );
            continue;
        }
        if let Some(name) = property(method, "set") {
            setters.insert(
                name,
                (
                    method.to_owned(),
                    first_parameter(source, names, &function),
                    entry(source, path, at),
                ),
            );
            continue;
        }
        if declared_return(source, names, &function)
            .as_deref()
            .is_none_or(|kind| {
                !kind
                    .trim_start_matches('\\')
                    .eq_ignore_ascii_case(ATTRIBUTE)
            })
        {
            continue;
        }
        let doc = generic_contract(source, names, &function);
        let contract = body_contract(source, names, &function).unwrap_or_default();
        let name = crate::index::separated(method, '_');
        modern.insert(
            name.clone(),
            Accessor {
                name,
                method_name: method.into(),
                read_type: combine(
                    doc.as_ref().map(|(read, _)| read.clone()),
                    contract.read_type,
                ),
                write_type: combine(doc.map(|(_, write)| write), contract.write_type),
                has_getter: contract.getter,
                has_setter: contract.setter,
                entry: entry(source, path, at),
                getter_method: Some(method.into()),
                setter_method: Some(method.into()),
                getter_entry: Some(entry(source, path, at)),
                setter_entry: Some(entry(source, path, at)),
                modern: true,
            },
        );
    }
    let names = modern
        .keys()
        .chain(getters.keys())
        .chain(setters.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut result = BTreeMap::new();
    for name in names {
        let mut accessor = modern.remove(&name).unwrap_or_else(|| {
            let primary = getters.get(&name).or_else(|| setters.get(&name)).unwrap();
            Accessor {
                name: name.clone(),
                method_name: primary.0.clone(),
                read_type: None,
                write_type: None,
                has_getter: Some(false),
                has_setter: Some(false),
                entry: primary.2.clone(),
                getter_method: None,
                setter_method: None,
                getter_entry: None,
                setter_entry: None,
                modern: false,
            }
        });
        if let Some((method, kind, location)) = getters.remove(&name) {
            accessor.method_name = method.clone();
            accessor.entry = location.clone();
            accessor.getter_method = Some(method.clone());
            accessor.getter_entry = Some(location);
            accessor.has_getter =
                (!duplicates.contains(&method.to_ascii_lowercase())).then_some(true);
            accessor.read_type = if accessor.has_getter.is_some() {
                kind
            } else {
                None
            };
        }
        if let Some((method, kind, location)) = setters.remove(&name) {
            accessor.setter_method = Some(method.clone());
            accessor.setter_entry = Some(location);
            accessor.has_setter =
                (!duplicates.contains(&method.to_ascii_lowercase())).then_some(true);
            accessor.write_type = if accessor.has_setter.is_some() {
                kind
            } else {
                None
            };
        }
        if accessor.modern && duplicates.contains(&accessor.method_name.to_ascii_lowercase()) {
            accessor.read_type = None;
            accessor.write_type = None;
            accessor.has_getter = None;
            accessor.has_setter = None;
        }
        result.insert(name, accessor);
    }
    result
}

#[derive(Clone, Copy)]
enum SideKind {
    Get,
    Set,
}
#[derive(Clone)]
struct Side {
    method: String,
    kind: Option<String>,
    present: Option<bool>,
    entry: Option<Entry>,
    proof: Option<Entry>,
    modern: bool,
}
impl Side {
    fn unknown(method: &str, proof: Option<Entry>, modern: bool) -> Self {
        Self {
            method: method.into(),
            kind: None,
            present: None,
            entry: None,
            proof,
            modern,
        }
    }
}
fn same_entry(left: &Entry, right: &Entry) -> bool {
    left.path == right.path && left.at == right.at && left.width == right.width
}
fn side_from(model: &Model, name: &str, method: &str, side: SideKind, modern: bool) -> Side {
    let proof = model
        .returns
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(method))
        .map(|(_, method)| method.entry.clone());
    let Some(accessor) = model.semantic.accessor_declarations.get(name) else {
        return Side::unknown(method, proof, modern);
    };
    let (declared, kind, present, location) = match side {
        SideKind::Get => (
            accessor.getter_method.as_deref(),
            &accessor.read_type,
            accessor.has_getter,
            &accessor.getter_entry,
        ),
        SideKind::Set => (
            accessor.setter_method.as_deref(),
            &accessor.write_type,
            accessor.has_setter,
            &accessor.setter_entry,
        ),
    };
    if declared.is_none_or(|declared| !declared.eq_ignore_ascii_case(method)) {
        return Side::unknown(method, proof, modern);
    }
    Side {
        method: declared.unwrap().into(),
        kind: kind.clone(),
        present,
        entry: location.clone(),
        proof: location.clone().or(proof),
        modern,
    }
}
fn camel(name: &str) -> String {
    let mut words = name.split('_');
    let mut result = words.next().unwrap_or_default().to_owned();
    for word in words {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            result.extend(first.to_uppercase());
            result.push_str(chars.as_str());
        }
    }
    result
}
fn classic(name: &str, side: SideKind) -> String {
    let value = camel(name);
    let mut chars = value.chars();
    let title = chars
        .next()
        .map(|first| format!("{}{}", first.to_uppercase(), chars.as_str()))
        .unwrap_or_default();
    format!(
        "{}{title}Attribute",
        match side {
            SideKind::Get => "get",
            SideKind::Set => "set",
        }
    )
}
fn known_framework_trait(name: &str) -> bool {
    [
        "Illuminate\\Database\\Eloquent\\Factories\\HasFactory",
        "Illuminate\\Database\\Eloquent\\SoftDeletes",
        "Illuminate\\Database\\Eloquent\\HasBuilder",
        "Illuminate\\Database\\Eloquent\\HasCollection",
        "Illuminate\\Database\\Eloquent\\Concerns\\HasUuids",
        "Illuminate\\Database\\Eloquent\\Concerns\\HasUlids",
        "Illuminate\\Database\\Eloquent\\Concerns\\HasUniqueStringIds",
        "Illuminate\\Database\\Eloquent\\Concerns\\HasVersion7Uuids",
    ]
    .iter()
    .any(|known| name.eq_ignore_ascii_case(known))
}
impl Resolver<'_, '_> {
    fn accessor_traits<'a>(&'a self, model: &Model) -> Option<Vec<&'a Model>> {
        fn walk<'a>(
            resolver: &'a Resolver<'_, '_>,
            model: &Model,
            active: &mut BTreeSet<String>,
            seen: &mut BTreeSet<String>,
            out: &mut Vec<&'a Model>,
            depth: usize,
        ) -> Option<()> {
            if depth >= 8 || out.len() > 32 {
                return None;
            }
            for name in &model.traits {
                if !resolver
                    .index
                    .eloquent_models
                    .contains_key(&name.to_ascii_lowercase())
                    && known_framework_trait(name)
                {
                    continue;
                }
                let info = resolver
                    .raw(name)
                    .filter(|info| info.is_trait && !info.ambiguous)?;
                if !active.insert(name.to_ascii_lowercase()) {
                    return None;
                }
                if seen.insert(name.to_ascii_lowercase()) {
                    out.push(info);
                    walk(resolver, info, active, seen, out, depth + 1)?;
                }
                active.remove(&name.to_ascii_lowercase());
            }
            Some(())
        }
        let mut out = Vec::new();
        walk(
            self,
            model,
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
            &mut out,
            0,
        )?;
        Some(out)
    }
    fn accessor_method_side(
        &self,
        model: &Model,
        traits: &[&Model],
        name: &str,
        method: &str,
        side: SideKind,
        modern: bool,
    ) -> Option<Side> {
        let own = model
            .semantic
            .source_methods
            .iter()
            .filter(|candidate| candidate.eq_ignore_ascii_case(method))
            .collect::<Vec<_>>();
        if !own.is_empty() {
            return Some(if own.len() == 1 {
                side_from(model, name, own[0], side, modern)
            } else {
                Side::unknown(method, None, modern)
            });
        }
        let mut candidates = traits
            .iter()
            .filter_map(|info| {
                let methods = info
                    .semantic
                    .source_methods
                    .iter()
                    .filter(|candidate| candidate.eq_ignore_ascii_case(method))
                    .collect::<Vec<_>>();
                if methods.is_empty() {
                    return None;
                }
                Some(if methods.len() == 1 {
                    side_from(info, name, methods[0], side, modern)
                } else {
                    Side::unknown(method, None, modern)
                })
            })
            .collect::<Vec<_>>();
        candidates.dedup_by(|a, b| {
            a.proof
                .as_ref()
                .zip(b.proof.as_ref())
                .is_some_and(|(a, b)| same_entry(a, b))
        });
        if candidates.is_empty() {
            let alias=std::iter::once(model).chain(traits.iter().copied()).any(|info|info.adaptations.iter().any(|rule|matches!(rule,traits::Adaptation::Alias{alias:Some(alias),..}if alias.eq_ignore_ascii_case(method))));
            return alias.then(|| Side::unknown(method, None, modern));
        }
        if model.adapted {
            if !model
                .declared
                .iter()
                .any(|name| name.eq_ignore_ascii_case(method))
            {
                return None;
            }
            if let Some(selected) = model
                .returns
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(method))
                .map(|(_, method)| &method.entry)
            {
                let proven = candidates
                    .iter()
                    .filter(|candidate| {
                        candidate
                            .proof
                            .as_ref()
                            .is_some_and(|proof| same_entry(proof, selected))
                    })
                    .collect::<Vec<_>>();
                if proven.len() == 1 {
                    return Some(proven[0].clone());
                }
            }
            let affected = model.adaptations.iter().any(|rule| match rule {
                traits::Adaptation::Prefer {
                    method: changed, ..
                } => changed.eq_ignore_ascii_case(method),
                traits::Adaptation::Alias {
                    method: changed,
                    alias,
                    ..
                } => {
                    changed.eq_ignore_ascii_case(method)
                        || alias
                            .as_ref()
                            .is_some_and(|alias| alias.eq_ignore_ascii_case(method))
                }
            });
            if affected || model.trait_adaptations {
                return Some(Side::unknown(method, None, modern));
            }
        }
        (candidates.len() == 1)
            .then(|| candidates.remove(0))
            .or_else(|| Some(Side::unknown(method, None, modern)))
    }
    /// Resolved accessor contracts share the editor resolver's ownership and
    /// ancestry proof. Classical mutators have precedence across the complete
    /// lineage, independently for reads and writes.
    pub(super) fn accessor_contracts(&self, full: &str) -> Option<BTreeMap<String, Accessor>> {
        let lineage = self.lineage(full)?;
        let classes = lineage
            .iter()
            .copied()
            .filter(|model| !model.is_trait)
            .collect::<Vec<_>>();
        let mut sources = Vec::new();
        let mut names = BTreeSet::new();
        for class in classes {
            let traits = self.accessor_traits(class)?;
            if !class.adapted && !class.traits.is_empty() && self.traits(class).is_none() {
                return None;
            }
            for source in std::iter::once(class).chain(traits.iter().copied()) {
                names.extend(source.semantic.accessor_declarations.keys().cloned());
                for method in &source.semantic.source_methods {
                    if let Some(name) = property(method, "get").or_else(|| property(method, "set"))
                    {
                        names.insert(name);
                    }
                }
                for method in &source.declared {
                    if let Some(name) = property(method, "get").or_else(|| property(method, "set"))
                    {
                        names.insert(name);
                    }
                }
                for (name, method) in &source.returns {
                    if method
                        .target
                        .trim_start_matches('\\')
                        .eq_ignore_ascii_case(ATTRIBUTE)
                    {
                        names.insert(crate::index::separated(name, '_'));
                    }
                }
            }
            sources.push((class, traits));
        }
        let mut result = BTreeMap::new();
        for name in names {
            let resolve = |side: SideKind| -> Option<Side> {
                for modern in [false, true] {
                    let method = if modern {
                        camel(&name)
                    } else {
                        classic(&name, side)
                    };
                    for (class, traits) in &sources {
                        if let Some(mut selected) =
                            self.accessor_method_side(class, traits, &name, &method, side, modern)
                        {
                            if let Some(kind) = selected.kind.as_mut() {
                                *kind = kind
                                    .replace(LATE_MODEL, full)
                                    .replace(TRAIT_SELF, &class.full);
                            }
                            if selected
                                .entry
                                .as_ref()
                                .is_some_and(|entry| !self.owned(&entry.path))
                            {
                                return None;
                            }
                            return Some(selected);
                        }
                    }
                }
                None
            };
            let get = resolve(SideKind::Get);
            let set = resolve(SideKind::Set);
            if get.is_none() && set.is_none() {
                continue;
            }
            let entry = get
                .as_ref()
                .and_then(|side| side.entry.clone())
                .or_else(|| set.as_ref().and_then(|side| side.entry.clone()))
                .or_else(|| {
                    sources
                        .first()
                        .and_then(|(class, _)| class.semantic.declaration.clone())
                })?;
            let primary = get.as_ref().or(set.as_ref())?;
            result.insert(
                name.clone(),
                Accessor {
                    name,
                    method_name: primary.method.clone(),
                    read_type: get
                        .as_ref()
                        .filter(|side| side.present != Some(false))
                        .and_then(|side| side.kind.clone()),
                    write_type: set
                        .as_ref()
                        .filter(|side| side.present != Some(false))
                        .and_then(|side| side.kind.clone()),
                    has_getter: get.as_ref().map(|side| side.present).unwrap_or(Some(false)),
                    has_setter: set.as_ref().map(|side| side.present).unwrap_or(Some(false)),
                    entry,
                    getter_method: get.as_ref().map(|side| side.method.clone()),
                    setter_method: set.as_ref().map(|side| side.method.clone()),
                    getter_entry: get.as_ref().and_then(|side| side.entry.clone()),
                    setter_entry: set.as_ref().and_then(|side| side.entry.clone()),
                    modern: get.as_ref().is_some_and(|side| side.modern)
                        || set.as_ref().is_some_and(|side| side.modern),
                },
            );
        }
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(text: &str) -> BTreeMap<String, Accessor> {
        let source = Source::new(text).unwrap();
        let names = Names::parse(&source).unwrap();
        declared(&source, &names, "/project/app/Models/Post.php")
    }
    #[test]
    fn classical_getter_and_setter_merge_without_guessing_callback_bodies() {
        let facts = parse(
            r#"<?php namespace App\Models;class Post {public function getTitleAttribute():?string{return null;}protected function setTitleAttribute(string $value):void {}public function getOpaqueAttribute(){return guess();}private function getPrivateAttribute():string{return 'x';}public static function getStaticAttribute():int{return 1;}}"#,
        );
        let title = &facts["title"];
        assert_eq!(title.read_type.as_deref(), Some("?string"));
        assert_eq!(title.write_type.as_deref(), Some("string"));
        assert_eq!(title.has_getter, Some(true));
        assert_eq!(title.has_setter, Some(true));
        assert!(!title.modern);
        assert_eq!(facts["opaque"].read_type, None);
        assert!(!facts.contains_key("private"));
        assert!(!facts.contains_key("static"));
    }
    #[test]
    fn modern_named_and_positional_callbacks_record_read_only_write_only_and_both() {
        let facts = parse(
            r#"<?php namespace App\Models;use Illuminate\Database\Eloquent\Casts\Attribute as Field;class Post {
protected function title():Field{return Field::get(fn(string $value):string=>strtoupper($value));}
protected function password():Field{return Field::set(fn(string $value)=>hash_it($value));}
protected function count():Field{return Field::make(get:fn($value):int=>1,set:fn(int $value)=>$value)->shouldCache();}
protected function address():Field{return new Field(fn($value):?\App\Address=>null,fn(\App\Address $value)=>[]);}
}"#,
        );
        assert_eq!(facts["title"].read_type.as_deref(), Some("string"));
        assert_eq!(facts["title"].has_setter, Some(false));
        assert_eq!(facts["password"].has_getter, Some(false));
        assert_eq!(facts["password"].write_type.as_deref(), Some("string"));
        assert_eq!(facts["count"].read_type.as_deref(), Some("int"));
        assert_eq!(facts["count"].write_type.as_deref(), Some("int"));
        assert_eq!(facts["count"].has_setter, Some(true));
        assert_eq!(
            facts["address"].read_type.as_deref(),
            Some("?\\App\\Address")
        );
        assert_eq!(
            facts["address"].write_type.as_deref(),
            Some("\\App\\Address")
        );
    }
    #[test]
    fn generic_contracts_resolve_imports_nested_types_and_trait_late_binding() {
        let facts = parse(
            r#"<?php namespace App\Models;use Illuminate\Database\Eloquent\Casts\Attribute;use App\Value\Money;trait Attributes {
/** @return Attribute<Money|null, Money> */protected function price():Attribute{return attribute_factory();}
/** @return Attribute<array<string,Money>, never> */protected function amounts():Attribute{return Attribute::make(get:fn($value)=>decode($value));}
/** @return Attribute<array<string,Money>, never> */protected function typedAmounts():Attribute{return Attribute::make(get:fn($value):array=>decode($value));}
/** @return Attribute<self,static> */protected function owner():Attribute{return dynamic();}
}"#,
        );
        assert_eq!(
            facts["price"].read_type.as_deref(),
            Some("\\App\\Value\\Money|null")
        );
        assert_eq!(
            facts["price"].write_type.as_deref(),
            Some("\\App\\Value\\Money")
        );
        assert_eq!(facts["price"].has_getter, None);
        assert_eq!(
            facts["amounts"].read_type.as_deref(),
            Some("array<string,\\App\\Value\\Money>")
        );
        assert_eq!(facts["amounts"].has_setter, Some(false));
        assert_eq!(
            facts["typed_amounts"].read_type.as_deref(),
            Some("array<string,\\App\\Value\\Money>")
        );
        assert_eq!(
            facts["owner"].read_type.as_deref(),
            Some("\\__OpenIDEA_Trait_Self__")
        );
        assert_eq!(
            facts["owner"].write_type.as_deref(),
            Some("\\__OpenIDEA_Late_Model__")
        );
    }
    #[test]
    fn foreign_attribute_alias_and_nonattached_docs_do_not_create_contracts() {
        let facts = parse(
            r#"<?php namespace App\Models;use Foreign\Attribute;class Post {/** @return Attribute<string,string> */protected function foreign():Attribute{return Attribute::make(get:fn():string=>'x');} }"#,
        );
        assert!(facts.is_empty());
        let facts = parse(
            r#"<?php use Illuminate\Database\Eloquent\Casts\Attribute;class Post {/** @return Attribute<string,string> */public const OTHER='x';protected function opaque():Attribute{return unknown();}}"#,
        );
        assert_eq!(facts["opaque"].read_type, None);
        assert_eq!(facts["opaque"].has_setter, None);
        let facts = parse(
            r#"<?php use Illuminate\Database\Eloquent\Casts\Attribute;class Post {/** @return Attribute<string,string> *//* A separate comment */protected function separated():Attribute{return unknown();}/** Example text containing @return Attribute<string,string> */protected function description():Attribute{return unknown();}}"#,
        );
        assert_eq!(facts["separated"].read_type, None);
        assert_eq!(facts["description"].read_type, None);
    }
    #[test]
    fn opaque_callbacks_conflicts_duplicate_parameters_and_strings_never_invent_types() {
        let facts = parse(
            r#"<?php use Illuminate\Database\Eloquent\Casts\Attribute;class Post {
/** @return Attribute<string,never> */protected function conflict():Attribute{return Attribute::get(fn():int=>1);}
protected function opaque():Attribute{return Attribute::make(get:$this->factory(),set:null);}
protected function fake():Attribute{return '\\Illuminate\\Database\\Eloquent\\Casts\\Attribute::get(fn():string=>"fake")';}
protected function duplicate():Attribute{return Attribute::make(get:fn():string=>'a',get:fn():int=>1);}
}"#,
        );
        assert_eq!(facts["conflict"].read_type, None);
        assert_eq!(facts["opaque"].has_getter, None);
        assert_eq!(facts["opaque"].has_setter, Some(false));
        assert_eq!(facts["fake"].has_getter, None);
        assert_eq!(facts["duplicate"].read_type, None);
    }
    #[test]
    fn classical_sides_override_modern_independently_and_destinations_are_utf16_exact() {
        let source = "<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Casts\\Attribute; // préface 💡\nclass Post {protected function title():Attribute{return Attribute::make(get:fn():int=>1,set:fn(int $value)=>$value);}public function getTitleAttribute():string{return 'x';}protected function setTitleAttribute(bool $value):void{}}";
        let facts = parse(source);
        let title = &facts["title"];
        assert_eq!(title.read_type.as_deref(), Some("string"));
        assert_eq!(title.write_type.as_deref(), Some("bool"));
        assert_eq!(title.getter_method.as_deref(), Some("getTitleAttribute"));
        assert_eq!(
            &source[text::offset(source, title.getter_entry.as_ref().unwrap().at).unwrap()..][..17],
            "getTitleAttribute"
        );
        assert_eq!(
            title.getter_entry.as_ref().unwrap().width,
            "getTitleAttribute".encode_utf16().count() as u32
        );
    }
    fn inventory(sources: &[(&str, &str)]) -> Index {
        let mut index = Index::default();
        for (path, source) in sources {
            super::super::cache(&mut index.eloquent_models, source, path);
            cache_object(&mut index.eloquent_objects, source, path);
        }
        finish(&mut index.eloquent_models);
        index
    }
    fn contracts(
        index: &Index,
        full: &str,
        owns: &dyn Fn(&str) -> bool,
    ) -> Option<BTreeMap<String, Accessor>> {
        let doc = Document {
            path: String::new(),
            language: "php".into(),
            version: 0,
            text: format!("<?php \\{full}::query()->"),
        };
        let context = context(&doc, text::position(&doc.text, doc.text.len()))?;
        let resolver = Resolver {
            context: &context,
            index,
            overlay: None,
            object_overlay: None,
            path: "",
            owns,
            owner_cache: RefCell::new(BTreeMap::new()),
        };
        resolver.accessor_contracts(full)
    }
    #[test]
    fn inherited_classic_sides_precede_modern_while_other_sides_resolve_independently() {
        let index = inventory(&[
            (
                "/project/app/Models/BasePost.php",
                r#"<?php namespace App\Models;use Illuminate\Database\Eloquent\Casts\Attribute;class BasePost extends \Illuminate\Database\Eloquent\Model{
public function getLegacyAttribute():string{return 'x';}protected function setLegacyAttribute(bool $value):void{}
protected function mixed():Attribute{return Attribute::make(get:fn():string=>'x',set:fn(string $value)=>$value);}
protected function overridden():Attribute{return Attribute::get(fn():string=>'x');}
}"#,
            ),
            (
                "/project/app/Models/Post.php",
                r#"<?php namespace App\Models;use Illuminate\Database\Eloquent\Casts\Attribute;class Post extends BasePost{
protected function legacy():Attribute{return Attribute::make(get:fn():int=>1,set:fn(int $value)=>$value);}
protected function setMixedAttribute(int $value):void{}
protected function overridden():Attribute{return Attribute::set(fn(bool $value)=>$value);}
}"#,
            ),
        ]);
        let facts = contracts(&index, "App\\Models\\Post", &|_| true).unwrap();
        assert_eq!(facts["legacy"].read_type.as_deref(), Some("string"));
        assert_eq!(facts["legacy"].write_type.as_deref(), Some("bool"));
        assert_eq!(
            facts["legacy"].getter_entry.as_ref().unwrap().path,
            "/project/app/Models/BasePost.php"
        );
        assert_eq!(facts["mixed"].read_type.as_deref(), Some("string"));
        assert_eq!(facts["mixed"].write_type.as_deref(), Some("int"));
        assert_eq!(
            facts["mixed"].setter_entry.as_ref().unwrap().path,
            "/project/app/Models/Post.php"
        );
        assert_eq!(facts["overridden"].has_getter, Some(false));
        assert_eq!(facts["overridden"].read_type, None);
        assert_eq!(facts["overridden"].write_type.as_deref(), Some("bool"));
    }
    #[test]
    fn private_and_opaque_overrides_block_inherited_types_without_suppressing_known_other_side() {
        let index = inventory(&[
            (
                "/project/app/Models/BasePost.php",
                r#"<?php namespace App\Models;use Illuminate\Database\Eloquent\Casts\Attribute;class BasePost extends \Illuminate\Database\Eloquent\Model{public function getTitleAttribute():string{return 'x';}protected function setTitleAttribute(string $value):void{}protected function opaque():Attribute{return Attribute::get(fn():string=>'x');}}"#,
            ),
            (
                "/project/app/Models/Post.php",
                r#"<?php namespace App\Models;use Illuminate\Database\Eloquent\Casts\Attribute;class Post extends BasePost{private function getTitleAttribute():int{return 1;}protected function opaque():Attribute{return unknown();}}"#,
            ),
        ]);
        let facts = contracts(&index, "App\\Models\\Post", &|_| true).unwrap();
        assert_eq!(facts["title"].read_type, None);
        assert_eq!(facts["title"].has_getter, None);
        assert_eq!(facts["title"].write_type.as_deref(), Some("string"));
        assert_eq!(facts["title"].getter_entry, None);
        assert_eq!(facts["opaque"].read_type, None);
        assert_eq!(facts["opaque"].has_getter, None);
    }
    #[test]
    fn trait_contracts_keep_lexical_self_late_static_ownership_and_alias_uncertainty() {
        let index = inventory(&[
            (
                "/project/app/Traits/Values.php",
                r#"<?php namespace App\Traits;trait Values{protected function getSelfValueAttribute():self{return $this;}public function getLateValueAttribute():static{return $this;}public function getTitleAttribute():string{return 'x';}}"#,
            ),
            (
                "/project/app/Models/BasePost.php",
                r#"<?php namespace App\Models;class BasePost extends \Illuminate\Database\Eloquent\Model{use \App\Traits\Values;public function getLexicalAttribute():self{return $this;}}"#,
            ),
            (
                "/project/app/Models/Post.php",
                r#"<?php namespace App\Models;class Post extends BasePost{}"#,
            ),
            (
                "/project/app/Models/Aliased.php",
                r#"<?php namespace App\Models;class Aliased extends \Illuminate\Database\Eloquent\Model{use \App\Traits\Values{getTitleAttribute as getLabelAttribute;}}"#,
            ),
        ]);
        let facts = contracts(&index, "App\\Models\\Post", &|_| true).unwrap();
        assert_eq!(
            facts["self_value"].read_type.as_deref(),
            Some("\\App\\Models\\BasePost")
        );
        assert_eq!(
            facts["late_value"].read_type.as_deref(),
            Some("\\App\\Models\\Post")
        );
        assert_eq!(
            facts["lexical"].read_type.as_deref(),
            Some("\\App\\Models\\BasePost")
        );
        assert!(
            contracts(&index, "App\\Models\\Post", &|path| !path
                .contains("/Traits/"))
            .is_none()
        );
        let facts = contracts(&index, "App\\Models\\Aliased", &|_| true).unwrap();
        assert_eq!(facts["title"].read_type.as_deref(), Some("string"));
        assert_eq!(facts["label"].read_type, None);
        assert_eq!(facts["label"].has_getter, None);
        assert_eq!(
            facts["self_value"].read_type.as_deref(),
            Some("\\App\\Models\\Aliased")
        );
    }
    #[test]
    fn conflicting_and_unknown_traits_never_inherit_an_unproven_accessor_contract() {
        let index = inventory(&[
            (
                "/project/app/Traits/One.php",
                r#"<?php namespace App\Traits;trait One{protected function getTitleAttribute():string{return 'x';}}"#,
            ),
            (
                "/project/app/Traits/Two.php",
                r#"<?php namespace App\Traits;trait Two{protected function getTitleAttribute():int{return 1;}}"#,
            ),
            (
                "/project/app/Models/Post.php",
                r#"<?php namespace App\Models;class Post extends \Illuminate\Database\Eloquent\Model{use \App\Traits\One,\App\Traits\Two;}"#,
            ),
            (
                "/project/app/Models/Unknown.php",
                r#"<?php namespace App\Models;class Unknown extends \Illuminate\Database\Eloquent\Model{use \App\Traits\Missing;}"#,
            ),
        ]);
        assert!(contracts(&index, "App\\Models\\Post", &|_| true).is_none());
        assert!(contracts(&index, "App\\Models\\Unknown", &|_| true).is_none());
    }
}
