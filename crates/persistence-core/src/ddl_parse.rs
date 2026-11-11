//! Bounded reader for DDL text (PostgreSQL and MySQL) into a [`SchemaSnapshot`].
//!
//! Supported: `CREATE TABLE` (column constraints, PRIMARY KEY, UNIQUE, FOREIGN KEY, MySQL
//! `INDEX`/`KEY`), `CREATE [UNIQUE] INDEX`, and `ALTER TABLE ... ADD [CONSTRAINT] PRIMARY KEY |
//! UNIQUE | FOREIGN KEY` (plus PostgreSQL `ALTER COLUMN ... ADD GENERATED ... AS IDENTITY`).
//! Everything else is reported in [`ParsedDdl::skipped`] with its line number. A table touched by
//! something this reader cannot model keeps `Unknown` facts instead of guessing. No I/O, no panics.
use crate::schema::{
    Column, Fact, ForeignKey, Index, LogicalType, MAX_COLUMNS, Provenance, SchemaError, SchemaSnapshot, SqlType, Table,
    UniqueConstraint,
};

pub const MAX_SQL_BYTES: usize = 1 << 20;
pub const MAX_STATEMENTS: usize = 5000;
const MAX_TOKENS: usize = 300_000;
const MAX_SKIPPED: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    PostgreSql,
    MySql,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub line: usize,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedDdl {
    pub snapshot: SchemaSnapshot,
    pub skipped: Vec<Skipped>,
    /// Skipped entries beyond `MAX_SKIPPED` that are counted but not listed.
    pub skipped_overflow: usize,
    /// PostgreSQL `CREATE TYPE name AS ENUM ('a', ...)` types whose labels were all readable.
    pub enum_types: Vec<EnumType>,
    /// Columns whose type is a readable enumeration: a MySQL `ENUM('a','b')` column or a PostgreSQL
    /// column of a `CREATE TYPE ... AS ENUM` type. Their `sql_type` stays `Unknown`.
    pub column_enums: Vec<ColumnEnum>,
    /// `COMMENT ON TABLE/COLUMN` (PostgreSQL) and inline/table `COMMENT '...'` (MySQL).
    pub comments: Vec<Comment>,
    /// Known non-unique indexes whose key parts carry `DESC` or other ordering/operator-class text
    /// that `Index` cannot express; the index stays listed with plain column names.
    pub ordered_indexes: Vec<OrderedIndex>,
    /// Views whose column names and types are derivable without guessing (explicit column list and/or
    /// a single-table `SELECT` of plain columns). Every other view is listed in `skipped`.
    pub views: Vec<View>,
    /// Columns whose SQL type has no Java mapping here (`sql_type` is `Unknown`), with the type text as written.
    pub unmapped_columns: Vec<UnmappedColumn>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub name: String,
    pub materialized: bool,
    /// Types are inherited from the underlying table; every column is nullable (views carry no constraints).
    pub columns: Vec<Column>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnmappedColumn {
    pub table: String,
    pub column: String,
    pub sql_type: String,
}

struct RawView {
    line: usize,
    name: String,
    materialized: bool,
    or_replace: bool,
    list: Option<Vec<String>>,
    items: Vec<(Option<String>, String, Option<String>)>,
    from: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderedIndex {
    pub table: String,
    pub name: Option<String>,
    pub columns: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumType {
    /// Unqualified type name (lower-cased unless it was quoted).
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnEnum {
    pub table: String,
    pub column: String,
    /// The PostgreSQL type name; `None` for a MySQL inline `ENUM(...)`.
    pub type_name: Option<String>,
    pub values: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    pub table: String,
    /// `None` for a table comment.
    pub column: Option<String>,
    pub text: String,
}

struct RawComment {
    parts: Vec<String>,
    column: bool,
    text: Option<String>,
}

struct RawTyped {
    table: String,
    column: String,
    type_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DdlError {
    TooLarge { len: usize },
    TooManyStatements,
    Schema(SchemaError),
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Word(String),
    Quoted(String),
    Num,
    Str,
    Punct(char),
}

#[derive(Clone, Debug)]
struct T {
    tok: Tok,
    line: usize,
    start: usize,
    end: usize,
}

/// Guesses the dialect from syntax only; `None` when neither or both families show up.
pub fn detect_dialect(sql: &str) -> Option<Dialect> {
    let lower = sql.to_lowercase();
    let mysql = ["`", "auto_increment", "engine=", "engine =", " unsigned"].iter().any(|m| lower.contains(m));
    let postgres = ["bigserial", "smallserial", "::", " as identity", "timestamptz", "bytea", "character varying", "$$"]
        .iter()
        .any(|m| lower.contains(m));
    match (mysql, postgres) {
        (true, false) => Some(Dialect::MySql),
        (false, true) => Some(Dialect::PostgreSql),
        _ => None,
    }
}

fn lex(sql: &str, d: Dialect) -> (Vec<T>, Option<(usize, String)>) {
    let cs: Vec<(usize, char)> = sql.char_indices().collect();
    let at = |i: usize| cs.get(i).map_or('\0', |c| c.1);
    let pos = |i: usize| cs.get(i).map_or(sql.len(), |c| c.0);
    let mut out = Vec::new();
    let (mut i, mut line) = (0usize, 1usize);
    let pg = d == Dialect::PostgreSql;
    while i < cs.len() {
        if out.len() > MAX_TOKENS {
            return (out, Some((line, "token limit exceeded".into())));
        }
        let c = at(i);
        let start = i;
        let begin_line = line;
        let push = |out: &mut Vec<T>, tok: Tok, to: usize| out.push(T { tok, line: begin_line, start: pos(start), end: pos(to) });
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if (c == '-' && at(i + 1) == '-' && (pg || at(i + 2).is_whitespace() || at(i + 2).is_control() || i + 2 >= cs.len()))
            || (c == '#' && !pg)
        {
            while i < cs.len() && at(i) != '\n' {
                i += 1;
            }
        } else if c == '/' && at(i + 1) == '*' {
            let mut depth = 1;
            i += 2;
            while i < cs.len() && depth > 0 {
                if at(i) == '*' && at(i + 1) == '/' {
                    depth -= 1;
                    i += 2;
                } else if pg && at(i) == '/' && at(i + 1) == '*' {
                    depth += 1;
                    i += 2;
                } else {
                    if at(i) == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
            if depth > 0 {
                return (out, Some((begin_line, "unterminated block comment".into())));
            }
        } else if c == '\'' || (c == '"' && !pg) {
            let backslash = !pg;
            match string_end(&cs, i, c, backslash, &mut line) {
                Some(end) => {
                    i = end;
                    push(&mut out, Tok::Str, i);
                }
                None => return (out, Some((begin_line, "unterminated string".into()))),
            }
        } else if (c == '"' && pg) || (c == '`' && !pg) {
            let mut text = String::new();
            i += 1;
            loop {
                if i >= cs.len() {
                    return (out, Some((begin_line, "unterminated quoted identifier".into())));
                }
                if at(i) == c {
                    if at(i + 1) == c {
                        text.push(c);
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                if at(i) == '\n' {
                    line += 1;
                }
                text.push(at(i));
                i += 1;
            }
            push(&mut out, Tok::Quoted(text), i);
        } else if c == '$' && pg && dollar_tag(&cs, i).is_some() {
            let tag: String = dollar_tag(&cs, i).unwrap_or_default();
            let open_len = tag.chars().count() + 2;
            let closing: Vec<char> = format!("${tag}$").chars().collect();
            let mut j = i + open_len;
            let mut found = false;
            while j < cs.len() {
                if (0..closing.len()).all(|k| at(j + k) == closing[k]) && j + closing.len() <= cs.len() {
                    found = true;
                    j += closing.len();
                    break;
                }
                if at(j) == '\n' {
                    line += 1;
                }
                j += 1;
            }
            if !found {
                return (out, Some((begin_line, "unterminated dollar-quoted string".into())));
            }
            i = j;
            push(&mut out, Tok::Str, i);
        } else if c.is_ascii_digit() {
            while at(i).is_ascii_alphanumeric() || at(i) == '.' {
                i += 1;
            }
            push(&mut out, Tok::Num, i);
        } else if c.is_alphabetic() || c == '_' {
            while at(i).is_alphanumeric() || at(i) == '_' || at(i) == '$' {
                i += 1;
            }
            let word: String = cs[start..i].iter().map(|c| c.1).collect();
            if pg && (word == "E" || word == "e") && at(i) == '\'' {
                match string_end(&cs, i, '\'', true, &mut line) {
                    Some(end) => {
                        i = end;
                        push(&mut out, Tok::Str, i);
                    }
                    None => return (out, Some((begin_line, "unterminated string".into()))),
                }
            } else {
                push(&mut out, Tok::Word(word), i);
            }
        } else {
            i += 1;
            push(&mut out, Tok::Punct(c), i);
        }
    }
    (out, None)
}

fn string_end(cs: &[(usize, char)], open: usize, quote: char, backslash: bool, line: &mut usize) -> Option<usize> {
    let at = |i: usize| cs.get(i).map_or('\0', |c| c.1);
    let mut i = open + 1;
    while i < cs.len() {
        let c = at(i);
        if c == '\n' {
            *line += 1;
        }
        if backslash && c == '\\' {
            if at(i + 1) == '\n' {
                *line += 1;
            }
            i += 2;
            continue;
        }
        if c == quote {
            if at(i + 1) == quote {
                i += 2;
                continue;
            }
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

fn dollar_tag(cs: &[(usize, char)], i: usize) -> Option<String> {
    let mut j = i + 1;
    let mut tag = String::new();
    while let Some(&(_, c)) = cs.get(j) {
        if c == '$' {
            return Some(tag);
        }
        let first = tag.is_empty();
        if !(c.is_alphabetic() || c == '_' || (!first && c.is_ascii_digit())) {
            return None;
        }
        tag.push(c);
        j += 1;
    }
    None
}

struct Cur<'a> {
    t: &'a [T],
    i: usize,
}

impl<'a> Cur<'a> {
    fn new(t: &'a [T]) -> Self {
        Self { t, i: 0 }
    }

    fn peek(&self) -> Option<&'a T> {
        self.t.get(self.i)
    }

    fn kw_at(&self, n: usize, k: &str) -> bool {
        matches!(self.t.get(self.i + n), Some(T { tok: Tok::Word(w), .. }) if w.eq_ignore_ascii_case(k))
    }

    fn kw(&self, k: &str) -> bool {
        self.kw_at(0, k)
    }

    fn eat(&mut self, k: &str) -> bool {
        let hit = self.kw(k);
        if hit {
            self.i += 1;
        }
        hit
    }

    fn is_punct(&self, c: char) -> bool {
        matches!(self.t.get(self.i), Some(T { tok: Tok::Punct(p), .. }) if *p == c)
    }

    fn eat_punct(&mut self, c: char) -> bool {
        let hit = self.is_punct(c);
        if hit {
            self.i += 1;
        }
        hit
    }

    fn word(&self) -> Option<String> {
        match self.peek() {
            Some(T { tok: Tok::Word(w), .. }) => Some(w.to_ascii_lowercase()),
            _ => None,
        }
    }

    fn line(&self) -> usize {
        self.t.get(self.i).or(self.t.last()).map_or(0, |t| t.line)
    }

    fn rest_empty(&self) -> bool {
        self.i >= self.t.len()
    }

    /// Tokens inside the parenthesis group at the cursor; the cursor ends after the closing `)`.
    fn group(&mut self) -> Option<&'a [T]> {
        if !self.is_punct('(') {
            return None;
        }
        let open = self.i;
        let mut depth = 0usize;
        while let Some(t) = self.t.get(self.i) {
            match t.tok {
                Tok::Punct('(') => depth += 1,
                Tok::Punct(')') => {
                    depth -= 1;
                    if depth == 0 {
                        self.i += 1;
                        return Some(&self.t[open + 1..self.i - 1]);
                    }
                }
                _ => {}
            }
            self.i += 1;
        }
        None
    }

    fn ident(&mut self, d: Dialect) -> Option<String> {
        let name = name_of(self.peek()?, d)?;
        self.i += 1;
        Some(name)
    }

    fn qname(&mut self, d: Dialect) -> Option<String> {
        let mut parts = vec![self.ident(d)?];
        while self.is_punct('.') {
            self.i += 1;
            parts.push(self.ident(d)?);
        }
        (parts.len() <= 2).then(|| parts.join("."))
    }
}

fn name_of(t: &T, d: Dialect) -> Option<String> {
    match &t.tok {
        Tok::Word(w) => Some(if d == Dialect::PostgreSql { w.to_lowercase() } else { w.clone() }),
        Tok::Quoted(q) if !q.is_empty() => Some(q.clone()),
        _ => None,
    }
}

fn split_commas(inner: &[T]) -> Vec<&[T]> {
    let mut parts = Vec::new();
    let (mut depth, mut from) = (0i32, 0usize);
    for (i, t) in inner.iter().enumerate() {
        match t.tok {
            Tok::Punct('(') => depth += 1,
            Tok::Punct(')') => depth -= 1,
            Tok::Punct(',') if depth == 0 => {
                parts.push(&inner[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&inner[from..]);
    parts
}

struct KeyParts {
    columns: Vec<String>,
    /// An expression or a prefix length: the real key is not just these columns.
    lossy: bool,
    /// False when a part has anything after its column name except `ASC` (e.g. `DESC`, an operator class).
    plain: bool,
}

struct Ty {
    logical: Result<LogicalType, String>,
    raw: String,
    length: Option<u32>,
    precision: Option<u32>,
    scale: Option<u32>,
    serial: bool,
    enum_values: Option<Vec<String>>,
}

const COLUMN_WORDS: [&str; 24] = [
    "not", "null", "primary", "unique", "references", "constraint", "check", "generated", "collate", "auto_increment", "comment",
    "on", "deferrable", "initially", "visible", "invisible", "key", "column_format", "storage", "character", "charset", "as",
    "enforced", "engine_attribute",
];

struct Parser<'s> {
    d: Dialect,
    src: &'s str,
    skipped: Vec<Skipped>,
    overflow: usize,
    tables: Vec<Table>,
    incomplete: bool,
    enum_types: Vec<EnumType>,
    column_enums: Vec<ColumnEnum>,
    typed: Vec<RawTyped>,
    raw_comments: Vec<RawComment>,
    ordered_indexes: Vec<OrderedIndex>,
    raw_views: Vec<RawView>,
    unmapped: Vec<UnmappedColumn>,
}

fn push<X>(fact: &mut Fact<Vec<X>>, item: X) {
    if let Fact::Known(items) = fact {
        items.push(item);
    }
}

impl<'s> Parser<'s> {
    fn skip(&mut self, line: usize, reason: impl Into<String>) {
        if self.skipped.len() < MAX_SKIPPED {
            self.skipped.push(Skipped { line, reason: reason.into() });
        } else {
            self.overflow += 1;
        }
    }

    fn find(&self, name: &str) -> Option<usize> {
        find_in(&self.tables, name)
    }

    fn unknown_table(&mut self, ti: usize, reason: &str) {
        let name = self.tables[ti].name.clone();
        self.tables[ti] = Table::unknown(&name, Provenance::Literal, reason);
    }

    fn statements(&mut self, tokens: &[T]) {
        let mut deferred: Vec<(&[T], usize)> = Vec::new();
        for st in tokens.split(|t| t.tok == Tok::Punct(';')) {
            let Some(first) = st.first() else { continue };
            let line = first.line;
            let c = Cur::new(st);
            if c.kw("create") {
                self.create(st, line, &mut deferred);
            } else if c.kw("alter") && c.kw_at(1, "table") {
                deferred.push((st, line));
            } else if c.kw("comment") && c.kw_at(1, "on") && self.d == Dialect::PostgreSql && self.comment_on(st, line) {
            } else {
                let head: Vec<String> = st
                    .iter()
                    .take(2)
                    .map(|t| match &t.tok {
                        Tok::Word(w) => w.to_uppercase(),
                        _ => "?".into(),
                    })
                    .collect();
                self.skip(line, format!("unsupported statement {}", head.join(" ")));
            }
        }
        for (st, line) in deferred {
            if Cur::new(st).kw("alter") {
                self.alter(st, line);
            } else {
                self.create_index(st, line);
            }
        }
    }

    fn create<'a>(&mut self, st: &'a [T], line: usize, deferred: &mut Vec<(&'a [T], usize)>) {
        if self.view(st, line) {
            return;
        }
        let mut c = Cur::new(st);
        c.eat("create");
        let temporary;
        if self.d == Dialect::PostgreSql {
            c.eat("global");
            c.eat("local");
            temporary = c.eat("temporary") || c.eat("temp");
            c.eat("unlogged");
        } else {
            temporary = c.eat("temporary");
        }
        if c.eat("table") {
            if temporary {
                self.incomplete = true;
                self.skip(line, "temporary table");
                return;
            }
            self.create_table(c, line);
        } else if c.kw("index") || (c.kw("unique") && c.kw_at(1, "index")) {
            deferred.push((st, line));
        } else if self.d == Dialect::PostgreSql && c.kw("type") && self.create_enum_type(Cur { t: c.t, i: c.i + 1 }) {
        } else {
            let what = c.word().unwrap_or_default().to_uppercase();
            self.skip(line, format!("unsupported statement CREATE {what}"));
        }
    }

    /// `CREATE [OR REPLACE] [MATERIALIZED] VIEW` (PostgreSQL 18 CREATE VIEW / CREATE MATERIALIZED VIEW;
    /// MySQL 8.4 CREATE VIEW with ALGORITHM/DEFINER/SQL SECURITY, which cannot change the columns).
    /// True when the statement is a view statement (recorded or skipped with a reason).
    fn view(&mut self, st: &[T], line: usize) -> bool {
        let Some(at) = st.iter().take(40).position(|t| matches!(&t.tok, Tok::Word(w) if w.eq_ignore_ascii_case("view"))) else { return false };
        let prefix: Vec<String> = st[1..at].iter().filter_map(|t| if let Tok::Word(w) = &t.tok { Some(w.to_ascii_lowercase()) } else { None }).collect();
        let pg = self.d == Dialect::PostgreSql;
        let known: &[&str] = if pg { &["or", "replace", "materialized", "temp", "temporary", "recursive", "global", "local", "unlogged"] } else { &[] };
        let blocked = ["table", "index", "unique", "type", "function", "procedure", "trigger", "event", "database", "schema", "user", "role", "fulltext", "spatial", "sequence", "tablespace", "materialized"];
        let fine = if pg { prefix.iter().all(|w| known.contains(&w.as_str())) } else { !prefix.iter().any(|w| blocked.contains(&w.as_str())) };
        if !fine {
            return false;
        }
        let has = |w: &str| prefix.iter().any(|p| p == w);
        let materialized = has("materialized");
        let mut c = Cur { t: st, i: at + 1 };
        if materialized && c.eat("if") {
            c.eat("not");
            c.eat("exists");
        }
        let Some(name) = c.qname(self.d) else {
            self.skip(line, "CREATE VIEW: unreadable view name");
            return true;
        };
        if has("temp") || has("temporary") || has("recursive") || (has("or") && materialized) {
            self.skip(line, format!("CREATE VIEW {name}: temporary, recursive and OR REPLACE MATERIALIZED forms are not read"));
            return true;
        }
        let mut list = None;
        if c.is_punct('(') {
            let Some(inner) = c.group() else { return self.skip_view(line, &name, "unreadable column list") };
            let mut names = Vec::new();
            for part in split_commas(inner) {
                match part {
                    [one] => match name_of(one, self.d) {
                        Some(n) => names.push(n),
                        None => return self.skip_view(line, &name, "unreadable column list"),
                    },
                    _ => return self.skip_view(line, &name, "unreadable column list"),
                }
            }
            list = Some(names);
        }
        while !c.rest_empty() && !c.kw("as") {
            if c.is_punct('(') {
                if c.group().is_none() {
                    return self.skip_view(line, &name, "unreadable options");
                }
            } else {
                c.i += 1;
            }
        }
        if !c.eat("as") || !c.kw("select") {
            return self.skip_view(line, &name, "only a plain SELECT query is read");
        }
        c.i += 1;
        let rest = &c.t[c.i..];
        let mut depth = 0i32;
        let mut from_at = None;
        for (i, t) in rest.iter().enumerate() {
            match &t.tok {
                Tok::Punct('(') => depth += 1,
                Tok::Punct(')') => depth -= 1,
                Tok::Word(w) if depth == 0 && w.eq_ignore_ascii_case("from") => {
                    from_at = Some(i);
                    break;
                }
                _ => {}
            }
        }
        let Some(from_at) = from_at else { return self.skip_view(line, &name, "the SELECT has no FROM table") };
        let mut items = Vec::new();
        for part in split_commas(&rest[..from_at]) {
            let word = |t: &T| name_of(t, self.d);
            let is_as = |t: &T| matches!(&t.tok, Tok::Word(w) if w.eq_ignore_ascii_case("as"));
            let item = match part {
                [a] => word(a).map(|a| (None, a, None)),
                [q, T { tok: Tok::Punct('.'), .. }, a] => word(q).zip(word(a)).map(|(q, a)| (Some(q), a, None)),
                [a, k, alias] if is_as(k) => word(a).zip(word(alias)).map(|(a, alias)| (None, a, Some(alias))),
                [q, T { tok: Tok::Punct('.'), .. }, a, k, alias] if is_as(k) => match (word(q), word(a), word(alias)) {
                    (Some(q), Some(a), Some(alias)) => Some((Some(q), a, Some(alias))),
                    _ => None,
                },
                _ => None,
            };
            let Some(item) = item else { return self.skip_view(line, &name, "the select list holds more than plain columns (expressions, *, functions or DISTINCT are not read)") };
            items.push(item);
        }
        let mut f = Cur { t: &rest[from_at + 1..], i: 0 };
        let Some(from) = f.qname(self.d) else { return self.skip_view(line, &name, "unreadable FROM table") };
        let stop = ["where", "with", "order", "limit", "group", "having", "union", "intersect", "except", "window", "join", "cross", "natural", "inner", "left", "right", "full", "on", "using", "offset", "fetch", "for"];
        let mut alias = None;
        if f.eat("as") {
            alias = f.ident(self.d);
        } else if f.word().is_some_and(|w| !stop.contains(&w.as_str())) || matches!(f.peek(), Some(T { tok: Tok::Quoted(_), .. })) {
            alias = f.ident(self.d);
        }
        if !f.rest_empty() && !(f.kw("where") || f.kw("with") || f.kw("order") || f.kw("limit")) {
            return self.skip_view(line, &name, "only a single FROM table is read (joins, subqueries, grouping and set operations are not)");
        }
        let mut depth = 0i32;
        for t in &f.t[f.i..] {
            match &t.tok {
                Tok::Punct('(') => depth += 1,
                Tok::Punct(')') => depth -= 1,
                Tok::Word(w) if depth == 0 && ["union", "intersect", "except", "group", "having", "window", "join"].iter().any(|k| w.eq_ignore_ascii_case(k)) => {
                    return self.skip_view(line, &name, "only a single FROM table is read (joins, subqueries, grouping and set operations are not)");
                }
                _ => {}
            }
        }
        let bare = from.rsplit('.').next().unwrap_or(&from).to_owned();
        for (qualifier, _, _) in &items {
            let ok = qualifier.as_ref().is_none_or(|q| match &alias {
                Some(a) => q.eq_ignore_ascii_case(a),
                None => q.eq_ignore_ascii_case(&bare) || q.eq_ignore_ascii_case(&from),
            });
            if !ok {
                return self.skip_view(line, &name, "a column qualifier does not name the FROM table");
            }
        }
        self.raw_views.push(RawView { line, name, materialized, or_replace: has("or"), list, items, from });
        true
    }

    fn skip_view(&mut self, line: usize, name: &str, why: &str) -> bool {
        self.skip(line, format!("CREATE VIEW {name}: {why}"));
        true
    }

    /// `name AS ENUM ('a', ...)` after `CREATE TYPE`; false (nothing recorded) for any other form.
    fn create_enum_type(&mut self, mut c: Cur) -> bool {
        let Some(name) = c.qname(self.d) else { return false };
        if !(c.eat("as") && c.eat("enum")) {
            return false;
        }
        let Some(inner) = c.group() else { return false };
        let mut values = Vec::new();
        if !inner.is_empty() {
            for part in split_commas(inner) {
                let [one] = part else { return false };
                let Some(value) = self.string_value(one) else { return false };
                values.push(value);
            }
        }
        let bare = name.rsplit('.').next().unwrap_or(&name).to_owned();
        if !c.rest_empty() || self.enum_types.iter().any(|e| e.name == bare) {
            return false;
        }
        self.enum_types.push(EnumType { name: bare, values });
        true
    }

    /// `COMMENT ON TABLE|COLUMN name IS 'text' | NULL`; false for other objects or unreadable forms.
    fn comment_on(&mut self, st: &[T], line: usize) -> bool {
        let mut c = Cur::new(st);
        c.i = 2;
        let column = c.eat("column");
        if !column && !c.eat("table") {
            return false;
        }
        let mut parts = vec![];
        loop {
            let Some(part) = c.ident(self.d) else { return false };
            parts.push(part);
            if !c.eat_punct('.') {
                break;
            }
        }
        if !c.eat("is") || parts.len() > if column { 3 } else { 2 } || (column && parts.len() < 2) {
            return false;
        }
        let text = match c.peek() {
            Some(t) if c.kw("null") && c.t.len() == c.i + 1 => {
                let _ = t;
                None
            }
            Some(t) => match self.string_value(t) {
                Some(text) if c.t.len() == c.i + 1 => Some(text),
                _ => {
                    self.skip(line, "COMMENT ON text with an escape sequence or unreadable form is not read");
                    return true;
                }
            },
            None => return false,
        };
        self.raw_comments.push(RawComment { parts, column, text });
        true
    }

    /// Text of a plain quoted string token. `None` for PostgreSQL `E''`/dollar strings and for MySQL
    /// strings with a backslash (PostgreSQL treats backslash literally in plain strings, MySQL as an
    /// escape unless NO_BACKSLASH_ESCAPES; the latter is not guessed).
    fn string_value(&self, t: &T) -> Option<String> {
        if t.tok != Tok::Str {
            return None;
        }
        let text = &self.src[t.start..t.end];
        let quote = text.chars().next()?;
        if !(quote == '\'' || (quote == '"' && self.d == Dialect::MySql)) || text.len() < 2 || !text.ends_with(quote) {
            return None;
        }
        let inner = &text[1..text.len() - 1];
        if self.d == Dialect::MySql && inner.contains('\\') {
            return None;
        }
        Some(inner.replace(&format!("{quote}{quote}"), &quote.to_string()))
    }

    fn create_table(&mut self, mut c: Cur, line: usize) {
        let if_not_exists = c.eat("if") && c.eat("not") && c.eat("exists");
        let Some(name) = c.qname(self.d) else {
            self.incomplete = true;
            return self.skip(line, "unreadable table name");
        };
        let Some(body) = c.group() else {
            self.incomplete = true;
            return self.skip(line, format!("table {name}: unsupported CREATE TABLE form (OF, PARTITION OF, LIKE or AS)"));
        };
        if self.find(&name).is_some_and(|i| self.tables[i].name.eq_ignore_ascii_case(&name)) {
            if !if_not_exists {
                self.skip(line, format!("table {name}: defined twice, the second definition is ignored"));
            }
            return;
        }
        self.tables.push(Table::new(&name));
        let ti = self.tables.len() - 1;
        for item in split_commas(body) {
            self.item(item, ti, line);
        }
        if self.d == Dialect::MySql {
            let mut rest = Cur { t: c.t, i: c.i };
            while rest.peek().is_some() {
                if rest.kw("comment") {
                    rest.i += 1;
                    rest.eat_punct('=');
                    match rest.peek().and_then(|t| self.string_value(t)) {
                        Some(text) => self.raw_comments.push(RawComment { parts: vec![name.clone()], column: false, text: Some(text) }),
                        None => self.skip(line, format!("table {name}: COMMENT with an escape sequence is not read")),
                    }
                }
                rest.i += 1;
            }
        }
        let inherited = (self.d == Dialect::PostgreSql && c.kw("inherits")) || (self.d == Dialect::MySql && (c.kw("select") || c.kw("as")));
        let trailing_select = c.t[c.i..].iter().any(|t| matches!(&t.tok, Tok::Word(w) if w.eq_ignore_ascii_case("select")));
        if inherited || (self.d == Dialect::MySql && trailing_select) {
            self.skip(line, format!("table {name}: columns also come from INHERITS or AS SELECT"));
            if let Fact::Known(_) = self.tables[ti].columns {
                self.tables[ti].columns = Fact::unknown("columns also come from INHERITS or AS SELECT");
            }
        }
    }

    fn item(&mut self, item: &[T], ti: usize, stmt_line: usize) {
        let mut c = Cur::new(item);
        let line = c.line().max(stmt_line.min(c.line()));
        if c.rest_empty() {
            return self.skip(stmt_line, "empty definition");
        }
        let mut name = None;
        let mut constrained = false;
        if c.eat("constraint") {
            constrained = true;
            let keyword = ["primary", "unique", "foreign", "check", "exclude", "not"].iter().any(|k| c.kw(k));
            if !keyword {
                name = c.ident(self.d);
            }
        }
        if self.constraint(&mut c, ti, name, line) {
            return;
        }
        if constrained {
            let table = self.tables[ti].name.clone();
            return self.skip(line, format!("table {table}: unsupported table constraint"));
        }
        if self.d == Dialect::MySql && (c.kw("index") || c.kw("key") || c.kw("fulltext") || c.kw("spatial")) {
            let kind = c.word().unwrap_or_default();
            let table = self.tables[ti].name.clone();
            if kind == "fulltext" || kind == "spatial" {
                return self.skip(line, format!("table {table}: {} index not modeled", kind.to_uppercase()));
            }
            c.i += 1;
            return self.index_tail(c, ti, line, false);
        }
        if c.kw("like") {
            let table = self.tables[ti].name.clone();
            self.skip(line, format!("table {table}: LIKE copies columns that are not read"));
            return self.unknown_table(ti, "columns copied by LIKE");
        }
        self.column(c, ti, line);
    }

    /// PRIMARY KEY / UNIQUE / FOREIGN KEY table constraints, shared by CREATE and ALTER TABLE.
    fn constraint(&mut self, c: &mut Cur, ti: usize, name: Option<String>, line: usize) -> bool {
        let table = self.tables[ti].name.clone();
        if c.eat("primary") {
            c.eat("key");
            if c.eat("using") {
                c.i += 1;
            }
            let Some(parts) = self.key_parts(c) else {
                self.skip(line, format!("table {table}: PRIMARY KEY without a readable column list"));
                return true;
            };
            if parts.lossy {
                self.skip(line, format!("table {table}: PRIMARY KEY with expression or prefix"));
                self.tables[ti].primary_key = Fact::unknown("primary key uses an expression or prefix length");
            } else {
                self.set_primary_key(ti, parts.columns, line);
            }
            return true;
        }
        if c.kw("unique") {
            c.i += 1;
            let mut name = name;
            if self.d == Dialect::MySql {
                c.eat("index");
                c.eat("key");
                if !c.is_punct('(') && !c.kw("using") {
                    let index_name = c.ident(self.d);
                    name = name.or(index_name);
                }
                if c.eat("using") {
                    c.i += 1;
                }
            } else if c.eat("nulls") {
                c.eat("not");
                c.eat("distinct");
            }
            let Some(parts) = self.key_parts(c) else {
                self.skip(line, format!("table {table}: UNIQUE without a readable column list"));
                return true;
            };
            if parts.lossy {
                self.skip(line, format!("table {table}: UNIQUE with expression or prefix"));
                self.tables[ti].unique_constraints = Fact::unknown("unique key uses an expression or prefix length");
            } else {
                push(&mut self.tables[ti].unique_constraints, UniqueConstraint { name, columns: parts.columns });
            }
            return true;
        }
        if c.kw("foreign") && c.kw_at(1, "key") {
            c.i += 2;
            if self.d == Dialect::MySql && !c.is_punct('(') {
                c.ident(self.d);
            }
            let Some(parts) = self.key_parts(c) else {
                self.skip(line, format!("table {table}: FOREIGN KEY without a readable column list"));
                return true;
            };
            if !c.eat("references") {
                self.skip(line, format!("table {table}: FOREIGN KEY without REFERENCES"));
                return true;
            }
            if parts.lossy {
                self.skip(line, format!("table {table}: FOREIGN KEY with expression or prefix"));
                self.tables[ti].foreign_keys = Fact::unknown("foreign key uses an expression");
                return true;
            }
            self.foreign_key(c, ti, name, parts.columns, line);
            return true;
        }
        if c.kw("check") || c.kw("exclude") {
            let kind = c.word().unwrap_or_default().to_uppercase();
            self.skip(line, format!("table {table}: {kind} constraint not modeled"));
            return true;
        }
        false
    }

    fn set_primary_key(&mut self, ti: usize, columns: Vec<String>, line: usize) {
        let table = self.tables[ti].name.clone();
        match &self.tables[ti].primary_key {
            Fact::Known(existing) if !existing.is_empty() => self.skip(line, format!("table {table}: second PRIMARY KEY ignored")),
            Fact::Known(_) => self.tables[ti].primary_key = Fact::Known(columns),
            Fact::Unknown(_) => {}
        }
    }

    fn foreign_key(&mut self, c: &mut Cur, ti: usize, name: Option<String>, columns: Vec<String>, line: usize) {
        let table = self.tables[ti].name.clone();
        let Some(ref_table) = c.qname(self.d) else {
            self.skip(line, format!("table {table}: unreadable REFERENCES target"));
            self.tables[ti].foreign_keys = Fact::unknown("unreadable REFERENCES target");
            return;
        };
        let ref_columns = if c.is_punct('(') {
            match self.key_parts(c) {
                Some(p) if !p.lossy => Fact::Known(p.columns),
                _ => Fact::unknown("unreadable referenced column list"),
            }
        } else {
            Fact::unknown("referenced columns omitted: the referenced primary key is implied")
        };
        push(&mut self.tables[ti].foreign_keys, ForeignKey { name, columns, ref_table, ref_columns });
    }

    fn key_parts(&mut self, c: &mut Cur) -> Option<KeyParts> {
        let inner = c.group()?;
        let mut parts = KeyParts { columns: Vec::new(), lossy: false, plain: true };
        for part in split_commas(inner) {
            let mut p = Cur::new(part);
            if p.is_punct('(') {
                parts.lossy = true;
                continue;
            }
            match p.ident(self.d) {
                Some(column) => parts.columns.push(column),
                None => parts.lossy = true,
            }
            if p.is_punct('(') {
                parts.lossy = true;
            }
            if p.i < part.len() && !(p.kw("asc") && p.i + 1 == part.len()) {
                parts.plain = false;
            }
        }
        (!parts.columns.is_empty() || parts.lossy).then_some(parts)
    }

    /// MySQL `INDEX|KEY [name] [USING x] (parts)` after the introducing keyword.
    fn index_tail(&mut self, mut c: Cur, ti: usize, line: usize, unique: bool) {
        let table = self.tables[ti].name.clone();
        let name = if c.is_punct('(') || c.kw("using") { None } else { c.ident(self.d) };
        if c.eat("using") {
            c.i += 1;
        }
        let Some(parts) = self.key_parts(&mut c) else {
            return self.skip(line, format!("table {table}: index without a readable column list"));
        };
        self.add_index(ti, name, parts, unique, line);
    }

    fn add_index(&mut self, ti: usize, name: Option<String>, parts: KeyParts, unique: bool, line: usize) {
        let table = self.tables[ti].name.clone();
        if !unique && !parts.lossy && !parts.plain {
            self.ordered_indexes.push(OrderedIndex { table: table.clone(), name: name.clone(), columns: parts.columns.clone() });
        }
        if parts.lossy {
            self.skip(line, format!("table {table}: expression, prefix or partial index not modeled"));
            if unique {
                self.tables[ti].unique_constraints = Fact::unknown("expression, prefix or partial unique index");
            } else {
                self.tables[ti].indexes = Fact::unknown("expression, prefix or partial index");
            }
        } else if unique {
            push(&mut self.tables[ti].unique_constraints, UniqueConstraint { name, columns: parts.columns });
        } else {
            push(&mut self.tables[ti].indexes, Index { name, columns: parts.columns, unique: false });
        }
    }

    fn column(&mut self, mut c: Cur, ti: usize, line: usize) {
        let table = self.tables[ti].name.clone();
        let Some(name) = c.ident(self.d) else {
            return self.skip(line, format!("table {table}: unreadable column definition"));
        };
        let type_line = c.line();
        let type_name = {
            let mut look = Cur { t: c.t, i: c.i };
            look.qname(self.d).map(|q| q.rsplit('.').next().unwrap_or(&q).to_owned())
        };
        let Some(ty) = self.parse_type(&mut c) else {
            return self.skip(line, format!("table {table}: column {name} has no readable type"));
        };
        let mut column = Column::new(&name, LogicalType::Text);
        column.length = ty.length;
        column.precision = ty.precision;
        column.scale = ty.scale;
        match ty.logical {
            Ok(logical) => column.sql_type = Fact::Known(SqlType { logical, raw: ty.raw.clone() }),
            Err(reason) => {
                self.skip(type_line, format!("table {table}: column {name}: {reason}"));
                if let Some(values) = ty.enum_values.clone() {
                    self.column_enums.push(ColumnEnum { table: table.clone(), column: name.clone(), type_name: None, values });
                } else if self.d == Dialect::PostgreSql {
                    self.typed.push(RawTyped { table: table.clone(), column: name.clone(), type_name });
                }
                self.unmapped.push(UnmappedColumn { table: table.clone(), column: name.clone(), sql_type: ty.raw.clone() });
                column.sql_type = Fact::Unknown(reason);
            }
        }
        if ty.serial {
            column.generated = Fact::Known(true);
            column.nullable = Fact::Known(false);
        }
        self.column_constraints(&mut c, ti, &mut column, line);
        let duplicate = match &self.tables[ti].columns {
            Fact::Known(columns) => columns.iter().any(|x| x.name.eq_ignore_ascii_case(&name)),
            Fact::Unknown(_) => false,
        };
        if duplicate {
            self.skip(line, format!("table {table}: duplicate column {name}"));
            return self.tables[ti].columns = Fact::unknown("duplicate column name");
        }
        if let Fact::Known(columns) = &self.tables[ti].columns {
            if columns.len() >= MAX_COLUMNS {
                self.skip(line, format!("table {table}: more than {MAX_COLUMNS} columns"));
                return self.tables[ti].columns = Fact::unknown("too many columns");
            }
        }
        push(&mut self.tables[ti].columns, column);
    }

    fn column_constraints(&mut self, c: &mut Cur, ti: usize, column: &mut Column, line: usize) {
        let table = self.tables[ti].name.clone();
        let mut pending: Option<String> = None;
        while let Some(t) = c.peek() {
            let Some(word) = c.word() else {
                return self.unrecognised(column, &table, t, line);
            };
            match word.as_str() {
                "constraint" => {
                    c.i += 1;
                    pending = if COLUMN_WORDS.iter().any(|k| c.kw(k)) { None } else { c.ident(self.d) };
                    continue;
                }
                "not" => {
                    c.i += 1;
                    if c.eat("null") {
                        column.nullable = Fact::Known(false);
                    } else if !(c.eat("deferrable") || c.eat("enforced")) {
                        return self.unrecognised(column, &table, t, line);
                    }
                }
                "null" => {
                    c.i += 1;
                    column.nullable = Fact::Known(true);
                }
                "default" => {
                    c.i += 1;
                    let from = c.peek().map(|t| t.start);
                    c.i += 1;
                    while let Some(next) = c.peek() {
                        if c.is_punct('(') {
                            c.group();
                            continue;
                        }
                        if matches!(&next.tok, Tok::Word(w) if COLUMN_WORDS.contains(&w.to_ascii_lowercase().as_str())) {
                            break;
                        }
                        c.i += 1;
                    }
                    let to = c.t.get(c.i.wrapping_sub(1)).map(|t| t.end);
                    column.default = match (from, to) {
                        (Some(a), Some(b)) if a < b => Fact::Known(Some(self.src[a..b].to_owned())),
                        _ => Fact::unknown("unreadable default expression"),
                    };
                }
                "primary" | "key" => {
                    c.i += 1;
                    if word == "primary" {
                        c.eat("key");
                    }
                    column.nullable = Fact::Known(false);
                    self.set_primary_key(ti, vec![column.name.clone()], line);
                }
                "unique" => {
                    c.i += 1;
                    c.eat("key");
                    c.eat("index");
                    if c.eat("nulls") {
                        c.eat("not");
                        c.eat("distinct");
                    }
                    push(
                        &mut self.tables[ti].unique_constraints,
                        UniqueConstraint { name: pending.take(), columns: vec![column.name.clone()] },
                    );
                }
                "references" => {
                    c.i += 1;
                    if self.d == Dialect::MySql {
                        self.skip(line, format!("table {table}: column {}: MySQL parses but ignores inline REFERENCES", column.name));
                        c.qname(self.d);
                        c.group();
                    } else {
                        self.foreign_key(c, ti, pending.take(), vec![column.name.clone()], line);
                    }
                    while c.kw("match") || (c.kw("on") && (c.kw_at(1, "delete") || c.kw_at(1, "update")) && self.d == Dialect::PostgreSql)
                        || (c.kw("on") && (c.kw_at(1, "delete") || c.kw_at(1, "update")) && self.d == Dialect::MySql)
                    {
                        c.i += 2;
                        if c.eat("set") || c.eat("no") {
                            c.i += 1;
                            c.group();
                        } else {
                            c.i += 1;
                        }
                    }
                }
                "generated" => {
                    c.i += 1;
                    let always = c.eat("always");
                    if !always && c.eat("by") {
                        c.eat("default");
                    }
                    c.eat("as");
                    if c.eat("identity") {
                        c.group();
                        column.generated = Fact::Known(true);
                        column.nullable = Fact::Known(false);
                    } else {
                        c.group();
                        column.generated = Fact::unknown("computed column");
                    }
                }
                "as" => {
                    c.i += 1;
                    c.group();
                    column.generated = Fact::unknown("computed column");
                }
                "stored" | "virtual" | "deferrable" | "enforced" | "visible" | "invisible" => c.i += 1,
                "initially" | "collate" | "charset" | "storage" | "column_format" => c.i += 2,
                "auto_increment" => {
                    c.i += 1;
                    column.generated = Fact::Known(true);
                }
                "check" => {
                    c.i += 1;
                    c.group();
                    self.skip(line, format!("table {table}: column {}: CHECK constraint not modeled", column.name));
                }
                "character" => {
                    c.i += 1;
                    c.eat("set");
                    c.i += 1;
                }
                "comment" => {
                    c.i += 1;
                    match c.peek().and_then(|t| self.string_value(t)) {
                        Some(text) => self.raw_comments.push(RawComment { parts: vec![table.clone(), column.name.clone()], column: true, text: Some(text) }),
                        None => self.skip(line, format!("table {table}: column {}: COMMENT with an escape sequence is not read", column.name)),
                    }
                    c.i += 1;
                }
                "on" if self.d == Dialect::MySql => {
                    c.i += 1;
                    if !c.eat("update") {
                        return self.unrecognised(column, &table, t, line);
                    }
                    c.i += 1;
                    c.group();
                }
                "engine_attribute" | "secondary_engine_attribute" => {
                    c.i += 1;
                    c.eat_punct('=');
                    c.i += 1;
                }
                _ => return self.unrecognised(column, &table, t, line),
            }
        }
    }

    fn unrecognised(&mut self, column: &mut Column, table: &str, at: &T, line: usize) {
        let text = &self.src[at.start..at.end];
        let reason = format!("unrecognised column clause '{text}'");
        self.skip(line, format!("table {table}: column {}: {reason}", column.name));
        column.nullable = Fact::Unknown(reason);
    }

    fn parse_type(&mut self, c: &mut Cur) -> Option<Ty> {
        let first = c.peek()?;
        let from = first.start;
        let pg = self.d == Dialect::PostgreSql;
        let Some(mut base) = c.word() else {
            c.i += 1;
            while c.is_punct('.') {
                c.i += 2;
            }
            return Some(self.unmapped(from, c, "user-defined type"));
        };
        c.i += 1;
        if c.is_punct('.') {
            while c.eat_punct('.') {
                c.i += 1;
            }
            return Some(self.unmapped(from, c, "user-defined type"));
        }
        if base == "double" && c.eat("precision") {
            base = "double precision".into();
        } else if base == "character" || base == "char" {
            if c.eat("varying") {
                base = "varchar".into();
            }
        } else if (base == "bit" && c.kw("varying")) || base == "interval" || base == "national" || base == "long" {
            while c.word().is_some_and(|w| !COLUMN_WORDS.contains(&w.as_str())) {
                c.i += 1;
            }
            c.group();
            return Some(self.unmapped(from, c, &format!("type '{base}'")));
        }
        let enum_values = if !pg && base == "enum" && c.is_punct('(') {
            let mut look = Cur { t: c.t, i: c.i };
            look.group().and_then(|inner| {
                split_commas(inner)
                    .into_iter()
                    .map(|part| match part {
                        [one] => self.string_value(one),
                        _ => None,
                    })
                    .collect::<Option<Vec<String>>>()
            })
        } else {
            None
        };
        let mut args = Vec::new();
        let mut args_ok = true;
        if let Some(inner) = c.group() {
            for part in split_commas(inner) {
                match part {
                    [T { tok: Tok::Num, start, end, .. }] => match self.src[*start..*end].parse::<u32>() {
                        Ok(n) => args.push(n),
                        Err(_) => args_ok = false,
                    },
                    _ => args_ok = false,
                }
            }
        }
        let (mut tz, mut unsigned) = (false, false);
        if pg && (base == "time" || base == "timestamp") {
            if c.eat("with") {
                c.eat("time");
                c.eat("zone");
                tz = true;
            } else if c.eat("without") {
                c.eat("time");
                c.eat("zone");
            }
        }
        if !pg {
            loop {
                if c.eat("unsigned") {
                    unsigned = true;
                } else if c.eat("signed") || c.eat("zerofill") {
                } else {
                    break;
                }
            }
        }
        if pg && (c.is_punct('[') || c.kw("array")) {
            while c.is_punct('[') || c.kw("array") || c.is_punct(']') || matches!(c.peek(), Some(T { tok: Tok::Num, .. })) {
                c.i += 1;
            }
            return Some(self.unmapped(from, c, "array type"));
        }
        let to = c.t.get(c.i.wrapping_sub(1)).map_or(from, |t| t.end);
        let raw = self.src[from..to].split_whitespace().collect::<Vec<_>>().join(" ");
        let mut ty = Ty { logical: Err(String::new()), raw: raw.clone(), length: None, precision: None, scale: None, serial: false, enum_values: None };
        if !args_ok {
            ty.logical = Err(format!("unmapped SQL type '{raw}'"));
            ty.enum_values = enum_values;
            return Some(ty);
        }
        let mapped = if pg { map_postgres(&base, &args, tz) } else { map_mysql(&base, &args, unsigned) };
        match mapped {
            Some((logical, length, precision, scale, serial)) => {
                ty.logical = Ok(logical);
                ty.length = length;
                ty.precision = precision;
                ty.scale = scale;
                ty.serial = serial;
            }
            None => ty.logical = Err(format!("unmapped SQL type '{raw}'")),
        }
        Some(ty)
    }

    fn unmapped(&self, from: usize, c: &Cur, why: &str) -> Ty {
        let to = c.t.get(c.i.wrapping_sub(1)).map_or(from, |t| t.end).max(from);
        let raw = self.src[from..to].split_whitespace().collect::<Vec<_>>().join(" ");
        Ty { logical: Err(format!("unmapped SQL type '{raw}' ({why})")), raw, length: None, precision: None, scale: None, serial: false, enum_values: None }
    }

    fn create_index(&mut self, st: &[T], line: usize) {
        let mut c = Cur::new(st);
        c.eat("create");
        let unique = c.eat("unique");
        c.eat("index");
        if self.d == Dialect::PostgreSql {
            c.eat("concurrently");
            if c.eat("if") {
                c.eat("not");
                c.eat("exists");
            }
        }
        let name = if c.kw("on") { None } else { c.ident(self.d) };
        if self.d == Dialect::MySql && c.eat("using") {
            c.i += 1;
        }
        if !c.eat("on") {
            return self.skip(line, "CREATE INDEX without ON");
        }
        c.eat("only");
        let Some(table) = c.qname(self.d) else {
            return self.skip(line, "CREATE INDEX: unreadable table name");
        };
        let Some(ti) = self.find(&table) else {
            return self.skip(line, format!("CREATE INDEX on table {table}, which is not defined in this file"));
        };
        if c.eat("using") {
            c.i += 1;
        }
        let Some(mut parts) = self.key_parts(&mut c) else {
            return self.skip(line, format!("CREATE INDEX on {table}: unreadable column list"));
        };
        if c.t[c.i.min(c.t.len())..].iter().any(|t| matches!(&t.tok, Tok::Word(w) if w.eq_ignore_ascii_case("where"))) {
            parts.lossy = true;
        }
        self.add_index(ti, name, parts, unique, line);
    }

    fn alter(&mut self, st: &[T], line: usize) {
        let mut c = Cur::new(st);
        c.i = 2;
        if c.eat("if") {
            c.eat("exists");
        }
        c.eat("only");
        let Some(table) = c.qname(self.d) else {
            return self.skip(line, "ALTER TABLE: unreadable table name");
        };
        c.eat_punct('*');
        let Some(ti) = self.find(&table) else {
            return self.skip(line, format!("ALTER TABLE {table}: table is not defined in this file"));
        };
        let rest = &c.t[c.i.min(c.t.len())..];
        for action in split_commas(rest) {
            let mut a = Cur::new(action);
            let al = a.line().max(line);
            let name = self.tables[ti].name.clone();
            if a.eat("add") {
                let mut constraint_name = None;
                if a.eat("constraint") {
                    let keyword = ["primary", "unique", "foreign", "check", "exclude"].iter().any(|k| a.kw(k));
                    if !keyword {
                        constraint_name = a.ident(self.d);
                    }
                }
                if self.constraint(&mut a, ti, constraint_name, al) {
                    continue;
                }
                if self.d == Dialect::MySql && (a.kw("index") || a.kw("key")) {
                    a.i += 1;
                    self.index_tail(a, ti, al, false);
                    continue;
                }
            }
            if self.d == Dialect::PostgreSql && a.eat("alter") {
                a.eat("column");
                let column = a.ident(self.d);
                if let (Some(column), true) = (column, a.kw("add") && a.kw_at(1, "generated")) {
                    a.i += 2;
                    let by_default = a.eat("by");
                    if by_default || a.eat("always") {
                        a.eat("default");
                    }
                    if a.eat("as") && a.eat("identity") {
                        if let Fact::Known(columns) = &mut self.tables[ti].columns {
                            if let Some(col) = columns.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&column)) {
                                col.generated = Fact::Known(true);
                                col.nullable = Fact::Known(false);
                            }
                        }
                        continue;
                    }
                }
            }
            let head = Cur::new(action).word().unwrap_or_default();
            let harmless = ["owner", "enable", "disable", "force", "no", "replica", "cluster", "engine", "auto_increment", "comment", "row_format", "validate"];
            let shown: Vec<String> = action
                .iter()
                .take(3)
                .map(|t| self.src[t.start..t.end].to_uppercase())
                .collect();
            self.skip(al, format!("ALTER TABLE {name}: unsupported action {}", shown.join(" ")));
            if !harmless.contains(&head.as_str()) {
                self.unknown_table(ti, &format!("changed by an ALTER TABLE action at line {al} that is not read"));
            }
        }
    }

    fn finish(mut self) -> Result<ParsedDdl, DdlError> {
        for ti in 0..self.tables.len() {
            let pk = self.tables[ti].primary_key.known().cloned().unwrap_or_default();
            if let Fact::Known(columns) = &mut self.tables[ti].columns {
                for column in columns.iter_mut().filter(|c| pk.iter().any(|k| k.eq_ignore_ascii_case(&c.name))) {
                    column.nullable = Fact::Known(false);
                }
            }
            let Fact::Known(fks) = self.tables[ti].foreign_keys.clone() else { continue };
            let mut resolved = Vec::new();
            for mut fk in fks {
                let target = self.find(&fk.ref_table);
                if let Some(target) = target {
                    fk.ref_table = self.tables[target].name.clone();
                    if matches!(fk.ref_columns, Fact::Unknown(_)) && fk.ref_columns == Fact::unknown("referenced columns omitted: the referenced primary key is implied") {
                        fk.ref_columns = match &self.tables[target].primary_key {
                            Fact::Known(pk) if !pk.is_empty() => Fact::Known(pk.clone()),
                            _ => Fact::unknown("referenced table has no readable primary key"),
                        };
                    }
                }
                resolved.push(fk);
            }
            self.tables[ti].foreign_keys = Fact::Known(resolved);
        }
        let mut column_enums = std::mem::take(&mut self.column_enums);
        for typed in std::mem::take(&mut self.typed) {
            let Some(found) = typed.type_name.as_ref().and_then(|n| self.enum_types.iter().find(|e| &e.name == n)) else { continue };
            column_enums.push(ColumnEnum { table: typed.table, column: typed.column, type_name: Some(found.name.clone()), values: found.values.clone() });
        }
        let mut comments: Vec<Comment> = Vec::new();
        for raw in std::mem::take(&mut self.raw_comments) {
            let (table_parts, column) = match (raw.column, raw.parts.split_last()) {
                (true, Some((last, rest))) => (rest.join("."), Some(last.clone())),
                _ => (raw.parts.join("."), None),
            };
            let Some(ti) = self.find(&table_parts) else {
                self.skip(0, format!("COMMENT on {table_parts}, which is not defined in this file"));
                continue;
            };
            let table = self.tables[ti].name.clone();
            let column = match (&column, &self.tables[ti].columns) {
                (Some(wanted), Fact::Known(columns)) => match columns.iter().find(|c| c.name.eq_ignore_ascii_case(wanted)) {
                    Some(found) => Some(found.name.clone()),
                    None => {
                        self.skip(0, format!("COMMENT on column {wanted} of {table}, which is not a known column"));
                        continue;
                    }
                },
                (Some(_), Fact::Unknown(_)) => continue,
                (None, _) => None,
            };
            comments.retain(|c| !(c.table == table && c.column == column));
            if let Some(text) = raw.text.filter(|t| !t.is_empty()) {
                comments.push(Comment { table, column, text });
            }
        }
        let mut views: Vec<View> = Vec::new();
        let mut unmapped = std::mem::take(&mut self.unmapped);
        for raw in std::mem::take(&mut self.raw_views) {
            let fail = |this: &mut Self, why: String| this.skip(raw.line, format!("CREATE VIEW {}: {why}", raw.name));
            let exists = views.iter().position(|v| v.name.eq_ignore_ascii_case(&raw.name));
            if self.find(&raw.name).is_some_and(|i| self.tables[i].name.eq_ignore_ascii_case(&raw.name)) || (exists.is_some() && !raw.or_replace) {
                fail(&mut self, "the name is already used by a table or view in this file".into());
                continue;
            }
            let Some(ti) = self.find(&raw.from) else {
                fail(&mut self, format!("FROM {} is not a table defined in this file, so the column types are not known", raw.from));
                continue;
            };
            let Fact::Known(base) = self.tables[ti].columns.clone() else {
                let name = self.tables[ti].name.clone();
                fail(&mut self, format!("the columns of {name} are not fully known"));
                continue;
            };
            if raw.list.as_ref().is_some_and(|l| l.len() != raw.items.len()) {
                fail(&mut self, "the column list and the select list differ in length".into());
                continue;
            }
            let table_name = self.tables[ti].name.clone();
            let mut columns: Vec<Column> = Vec::new();
            let mut why = None;
            for (i, (_, source, alias)) in raw.items.iter().enumerate() {
                let Some(found) = base.iter().find(|c| c.name.eq_ignore_ascii_case(source)) else {
                    why = Some(format!("column {source} is not a column of {table_name}"));
                    break;
                };
                let name = raw.list.as_ref().map(|l| l[i].clone()).or_else(|| alias.clone()).unwrap_or_else(|| found.name.clone());
                if columns.iter().any(|c| c.name.eq_ignore_ascii_case(&name)) {
                    why = Some(format!("duplicate view column {name}"));
                    break;
                }
                let mut column = Column::new(&name, LogicalType::Text);
                column.sql_type = found.sql_type.clone();
                column.length = found.length;
                column.precision = found.precision;
                column.scale = found.scale;
                if let Some(raw_type) = unmapped.iter().find(|u| u.table == table_name && u.column == found.name).map(|u| u.sql_type.clone()) {
                    unmapped.push(UnmappedColumn { table: raw.name.clone(), column: name.clone(), sql_type: raw_type });
                }
                columns.push(column);
            }
            if let Some(why) = why {
                fail(&mut self, why);
                continue;
            }
            let view = View { name: raw.name.clone(), materialized: raw.materialized, columns };
            match exists {
                Some(i) => views[i] = view,
                None => views.push(view),
            }
        }
        let mut snapshot = SchemaSnapshot { complete: !self.incomplete, tables: self.tables, ..SchemaSnapshot::default() };
        snapshot.normalize();
        snapshot.validate().map_err(DdlError::Schema)?;
        Ok(ParsedDdl { snapshot, skipped: self.skipped, skipped_overflow: self.overflow, enum_types: self.enum_types, column_enums, comments, ordered_indexes: self.ordered_indexes, views, unmapped_columns: unmapped })
    }
}

/// PostgreSQL 18.6 documentation, Appendix C "SQL Key Words" (Table C.1): every key word whose
/// PostgreSQL column starts with "reserved" (plain, "can be function or type", "requires AS").
/// None of these may be an unquoted table or column name.
const POSTGRES_RESERVED: [&str; 101] = [
    "ALL", "ANALYSE", "ANALYZE", "AND", "ANY", "ARRAY", "AS", "ASC",
    "ASYMMETRIC", "AUTHORIZATION", "BINARY", "BOTH", "CASE", "CAST", "CHECK", "COLLATE",
    "COLLATION", "COLUMN", "CONCURRENTLY", "CONSTRAINT", "CREATE", "CROSS", "CURRENT_CATALOG", "CURRENT_DATE",
    "CURRENT_ROLE", "CURRENT_SCHEMA", "CURRENT_TIME", "CURRENT_TIMESTAMP", "CURRENT_USER", "DEFAULT", "DEFERRABLE", "DESC",
    "DISTINCT", "DO", "ELSE", "END", "EXCEPT", "FALSE", "FETCH", "FOR",
    "FOREIGN", "FREEZE", "FROM", "FULL", "GRANT", "GROUP", "HAVING", "ILIKE",
    "IN", "INITIALLY", "INNER", "INTERSECT", "INTO", "IS", "ISNULL", "JOIN",
    "LATERAL", "LEADING", "LEFT", "LIKE", "LIMIT", "LOCALTIME", "LOCALTIMESTAMP", "NATURAL",
    "NOT", "NOTNULL", "NULL", "OFFSET", "ON", "ONLY", "OR", "ORDER",
    "OUTER", "OVERLAPS", "PLACING", "PRIMARY", "REFERENCES", "RETURNING", "RIGHT", "SELECT",
    "SESSION_USER", "SIMILAR", "SOME", "SYMMETRIC", "SYSTEM_USER", "TABLE", "TABLESAMPLE", "THEN",
    "TO", "TRAILING", "TRUE", "UNION", "UNIQUE", "USER", "USING", "VARIADIC",
    "VERBOSE", "WHEN", "WHERE", "WINDOW", "WITH",
];

/// MySQL 8.4 Reference Manual, section 11.3 "Keywords and Reserved Words": every keyword marked (R).
/// Reserved words need backtick quoting as identifiers.
const MYSQL_RESERVED: [&str; 266] = [
    "ACCESSIBLE", "ADD", "ALL", "ALTER", "ANALYZE", "AND", "AS", "ASC",
    "ASENSITIVE", "BEFORE", "BETWEEN", "BIGINT", "BINARY", "BLOB", "BOTH", "BY",
    "CALL", "CASCADE", "CASE", "CHANGE", "CHAR", "CHARACTER", "CHECK", "COLLATE",
    "COLUMN", "CONDITION", "CONSTRAINT", "CONTINUE", "CONVERT", "CREATE", "CROSS", "CUBE",
    "CUME_DIST", "CURRENT_DATE", "CURRENT_TIME", "CURRENT_TIMESTAMP", "CURRENT_USER", "CURSOR", "DATABASE", "DATABASES",
    "DAY_HOUR", "DAY_MICROSECOND", "DAY_MINUTE", "DAY_SECOND", "DEC", "DECIMAL", "DECLARE", "DEFAULT",
    "DELAYED", "DELETE", "DENSE_RANK", "DESC", "DESCRIBE", "DETERMINISTIC", "DISTINCT", "DISTINCTROW",
    "DIV", "DOUBLE", "DROP", "DUAL", "EACH", "ELSE", "ELSEIF", "EMPTY",
    "ENCLOSED", "ESCAPED", "EXCEPT", "EXISTS", "EXIT", "EXPLAIN", "FALSE", "FETCH",
    "FIRST_VALUE", "FLOAT", "FLOAT4", "FLOAT8", "FOR", "FORCE", "FOREIGN", "FROM",
    "FULLTEXT", "FUNCTION", "GENERATED", "GET", "GRANT", "GROUP", "GROUPING", "GROUPS",
    "HAVING", "HIGH_PRIORITY", "HOUR_MICROSECOND", "HOUR_MINUTE", "HOUR_SECOND", "IF", "IGNORE", "IN",
    "INDEX", "INFILE", "INNER", "INOUT", "INSENSITIVE", "INSERT", "INT", "INT1",
    "INT2", "INT3", "INT4", "INT8", "INTEGER", "INTERSECT", "INTERVAL", "INTO",
    "IO_AFTER_GTIDS", "IO_BEFORE_GTIDS", "IS", "ITERATE", "JOIN", "JSON_TABLE", "KEY", "KEYS",
    "KILL", "LAG", "LAST_VALUE", "LATERAL", "LEAD", "LEADING", "LEAVE", "LEFT",
    "LIKE", "LIMIT", "LINEAR", "LINES", "LOAD", "LOCALTIME", "LOCALTIMESTAMP", "LOCK",
    "LONG", "LONGBLOB", "LONGTEXT", "LOOP", "LOW_PRIORITY", "MANUAL", "MASTER_BIND", "MASTER_SSL_VERIFY_SERVER_CERT",
    "MATCH", "MAXVALUE", "MEDIUMBLOB", "MEDIUMINT", "MEDIUMTEXT", "MIDDLEINT", "MINUTE_MICROSECOND", "MINUTE_SECOND",
    "MOD", "MODIFIES", "NATURAL", "NOT", "NO_WRITE_TO_BINLOG", "NTH_VALUE", "NTILE", "NULL",
    "NUMERIC", "OF", "ON", "OPTIMIZE", "OPTIMIZER_COSTS", "OPTION", "OPTIONALLY", "OR",
    "ORDER", "OUT", "OUTER", "OUTFILE", "OVER", "PARALLEL", "PARTITION", "PERCENT_RANK",
    "PRECISION", "PRIMARY", "PROCEDURE", "PURGE", "QUALIFY", "RANGE", "RANK", "READ",
    "READS", "READ_WRITE", "REAL", "RECURSIVE", "REFERENCES", "REGEXP", "RELEASE", "RENAME",
    "REPEAT", "REPLACE", "REQUIRE", "RESIGNAL", "RESTRICT", "RETURN", "REVOKE", "RIGHT",
    "RLIKE", "ROW", "ROWS", "ROW_NUMBER", "SCHEMA", "SCHEMAS", "SECOND_MICROSECOND", "SELECT",
    "SENSITIVE", "SEPARATOR", "SET", "SHOW", "SIGNAL", "SMALLINT", "SPATIAL", "SPECIFIC",
    "SQL", "SQLEXCEPTION", "SQLSTATE", "SQLWARNING", "SQL_BIG_RESULT", "SQL_CALC_FOUND_ROWS", "SQL_SMALL_RESULT", "SSL",
    "STARTING", "STORED", "STRAIGHT_JOIN", "SYSTEM", "TABLE", "TABLESAMPLE", "TERMINATED", "THEN",
    "TINYBLOB", "TINYINT", "TINYTEXT", "TO", "TRAILING", "TRIGGER", "TRUE", "UNDO",
    "UNION", "UNIQUE", "UNLOCK", "UNSIGNED", "UPDATE", "USAGE", "USE", "USING",
    "UTC_DATE", "UTC_TIME", "UTC_TIMESTAMP", "VALUES", "VARBINARY", "VARCHAR", "VARCHARACTER", "VARYING",
    "VIRTUAL", "WHEN", "WHERE", "WHILE", "WINDOW", "WITH", "WRITE", "XOR",
    "YEAR_MONTH", "ZEROFILL",
];

/// True when `name` is a reserved word of the dialect's keyword list above (case-insensitive).
pub fn is_reserved(dialect: Dialect, name: &str) -> bool {
    let list: &[&str] = if dialect == Dialect::PostgreSql { &POSTGRES_RESERVED } else { &MYSQL_RESERVED };
    list.iter().any(|word| word.eq_ignore_ascii_case(name))
}

/// Index of the table a spelled name refers to: an exact (case-insensitive) match, else a unique
/// match that differs only by a schema qualifier on one side.
pub fn find_table(snapshot: &SchemaSnapshot, name: &str) -> Option<usize> {
    find_in(&snapshot.tables, name)
}

fn find_in(tables: &[Table], name: &str) -> Option<usize> {
    let lower = name.to_lowercase();
    let bare = lower.rsplit('.').next().unwrap_or(&lower).to_owned();
    if let Some(i) = tables.iter().position(|t| t.name.to_lowercase() == lower) {
        return Some(i);
    }
    let hits: Vec<usize> = (0..tables.len())
        .filter(|&i| {
            let table = tables[i].name.to_lowercase();
            if lower.contains('.') { table == bare } else { table.rsplit('.').next() == Some(bare.as_str()) }
        })
        .collect();
    (hits.len() == 1).then(|| hits[0])
}

type Mapped = (LogicalType, Option<u32>, Option<u32>, Option<u32>, bool);

fn map_postgres(base: &str, args: &[u32], tz: bool) -> Option<Mapped> {
    use LogicalType::*;
    let plain = |t| Some((t, None, None, None, false));
    let serial = |t| Some((t, None, None, None, true));
    match (base, args) {
        ("smallint" | "int2", []) => plain(Smallint),
        ("smallserial" | "serial2", []) => serial(Smallint),
        ("integer" | "int" | "int4", []) => plain(Integer),
        ("serial" | "serial4", []) => serial(Integer),
        ("bigint" | "int8", []) => plain(Bigint),
        ("bigserial" | "serial8", []) => serial(Bigint),
        ("boolean" | "bool", []) => plain(Boolean),
        ("real" | "float4", []) => plain(Real),
        ("double precision" | "float8" | "float", []) => plain(Double),
        ("float", [p]) if (1..=24).contains(p) => plain(Real),
        ("float", [p]) if (25..=53).contains(p) => plain(Double),
        ("numeric" | "decimal", []) => plain(Decimal),
        ("numeric" | "decimal", [p]) => Some((Decimal, None, Some(*p), Some(0), false)),
        ("numeric" | "decimal", [p, s]) => Some((Decimal, None, Some(*p), Some(*s), false)),
        ("char" | "character" | "bpchar", []) => plain(Char),
        ("char" | "character" | "bpchar", [n]) => Some((Char, Some(*n), None, None, false)),
        ("varchar", []) => plain(Varchar),
        ("varchar", [n]) => Some((Varchar, Some(*n), None, None, false)),
        ("text", []) => plain(Text),
        ("date", []) => plain(Date),
        ("time", [] | [_]) if !tz => plain(Time),
        ("timestamp", [] | [_]) => plain(if tz { TimestampTz } else { Timestamp }),
        ("timestamptz", [] | [_]) => plain(TimestampTz),
        ("uuid", []) => plain(Uuid),
        ("bytea", []) => plain(Binary),
        _ => None,
    }
}

fn map_mysql(base: &str, args: &[u32], unsigned: bool) -> Option<Mapped> {
    use LogicalType::*;
    let plain = |t| Some((t, None, None, None, false));
    match (base, args) {
        ("tinyint", _) => plain(if unsigned { Smallint } else { Tinyint }),
        ("smallint", _) => plain(if unsigned { Integer } else { Smallint }),
        ("mediumint", _) => plain(Integer),
        ("int" | "integer", _) => plain(if unsigned { Bigint } else { Integer }),
        ("bigint", _) if unsigned => Some((Decimal, None, Some(20), Some(0), false)),
        ("bigint", _) => plain(Bigint),
        ("serial", []) => Some((Decimal, None, Some(20), Some(0), true)),
        ("bool" | "boolean", []) => plain(Boolean),
        ("bit", [] | [1]) => plain(Boolean),
        ("float", []) => plain(Real),
        ("double" | "double precision" | "real", _) => plain(Double),
        ("decimal" | "dec" | "numeric" | "fixed", []) => plain(Decimal),
        ("decimal" | "dec" | "numeric" | "fixed", [p]) => Some((Decimal, None, Some(*p), Some(0), false)),
        ("decimal" | "dec" | "numeric" | "fixed", [p, s]) => Some((Decimal, None, Some(*p), Some(*s), false)),
        ("char", []) => Some((Char, Some(1), None, None, false)),
        ("char", [n]) => Some((Char, Some(*n), None, None, false)),
        ("varchar", [n]) => Some((Varchar, Some(*n), None, None, false)),
        ("tinytext" | "text" | "mediumtext" | "longtext", _) => plain(Text),
        ("tinyblob" | "blob" | "mediumblob" | "longblob", _) => plain(Binary),
        ("binary", []) => Some((Binary, Some(1), None, None, false)),
        ("binary" | "varbinary", [n]) => Some((Binary, Some(*n), None, None, false)),
        ("date", []) => plain(Date),
        ("time", [] | [_]) => plain(Time),
        ("datetime" | "timestamp", [] | [_]) => plain(Timestamp),
        _ => None,
    }
}

/// Parses DDL text. Unsupported statements, clauses and types land in `skipped`; facts that
/// depend on them stay `Unknown`.
pub fn parse_ddl(sql: &str, dialect: Dialect) -> Result<ParsedDdl, DdlError> {
    if sql.len() > MAX_SQL_BYTES {
        return Err(DdlError::TooLarge { len: sql.len() });
    }
    let (tokens, error) = lex(sql, dialect);
    let mut parser = Parser { d: dialect, src: sql, skipped: Vec::new(), overflow: 0, tables: Vec::new(), incomplete: false, enum_types: Vec::new(), column_enums: Vec::new(), typed: Vec::new(), raw_comments: Vec::new(), ordered_indexes: Vec::new(), raw_views: Vec::new(), unmapped: Vec::new() };
    let complete_to = match &error {
        Some(_) => tokens.iter().rposition(|t| t.tok == Tok::Punct(';')).map_or(0, |p| p + 1),
        None => tokens.len(),
    };
    if tokens.iter().filter(|t| t.tok == Tok::Punct(';')).count() > MAX_STATEMENTS {
        return Err(DdlError::TooManyStatements);
    }
    parser.statements(&tokens[..complete_to]);
    if let Some((line, reason)) = error {
        parser.incomplete = true;
        parser.skip(line, format!("lexing stopped: {reason}; the rest of the file is not read"));
    }
    parser.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PG: &str = r#"
-- shop schema
SET search_path = public;
CREATE TABLE customers (
    id bigserial PRIMARY KEY,
    email character varying(120) NOT NULL UNIQUE,
    "Display Name" text,
    balance numeric(12,2) DEFAULT 0,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    active boolean DEFAULT true,
    token uuid,
    avatar bytea,
    tags text[],
    meta jsonb
);
CREATE TABLE public.orders (
    id integer GENERATED ALWAYS AS IDENTITY,
    customer_id bigint NOT NULL REFERENCES customers (id) ON DELETE CASCADE,
    note varchar(40),
    placed date,
    CONSTRAINT orders_pk PRIMARY KEY (id)
);
CREATE TABLE order_tags (
    order_id integer NOT NULL,
    tag text NOT NULL,
    PRIMARY KEY (order_id, tag),
    CONSTRAINT fk_ot FOREIGN KEY (order_id) REFERENCES public.orders(id)
);
CREATE UNIQUE INDEX uq_note ON orders (note);
CREATE INDEX idx_placed ON ONLY orders USING btree (placed DESC NULLS LAST);
CREATE INDEX idx_expr ON customers ((lower(email)));
CREATE INDEX idx_partial ON customers (email) WHERE active;
ALTER TABLE ONLY customers ADD CONSTRAINT customers_token_key UNIQUE (token);
ALTER TABLE customers ADD CONSTRAINT fk_x FOREIGN KEY (id) REFERENCES orders;
ALTER TABLE orders OWNER TO me;
CREATE VIEW v AS SELECT 1;
COMMENT ON TABLE orders IS 'a; b';
SELECT $$ ; $$;
"#;

    const MY: &str = r#"
# dump
CREATE TABLE `customers` (
  `id` bigint unsigned NOT NULL AUTO_INCREMENT,
  `email` varchar(120) NOT NULL,
  `age` tinyint(3) unsigned DEFAULT NULL,
  `flag` bool,
  `kind` enum('a','b') NOT NULL,
  `body` mediumtext,
  `price` decimal(10,2) DEFAULT '0.00',
  `seen` datetime(6) NULL ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `uq_email` (`email`),
  KEY `idx_age` (`age`),
  KEY `idx_prefix` (`body`(10))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='x;y';
CREATE TABLE orders (
  id int NOT NULL AUTO_INCREMENT PRIMARY KEY,
  customer_id bigint unsigned REFERENCES customers (id),
  `code` char(8),
  CONSTRAINT fk_c FOREIGN KEY (customer_id) REFERENCES customers (id)
);
ALTER TABLE orders ADD UNIQUE INDEX uq_code (code);
ALTER TABLE orders ADD COLUMN x int;
"#;

    fn table<'a>(p: &'a ParsedDdl, name: &str) -> &'a Table {
        p.snapshot.tables.iter().find(|t| t.name == name).unwrap_or_else(|| panic!("{name}: {:?}", p.snapshot.tables))
    }
    fn column<'a>(t: &'a Table, name: &str) -> &'a Column {
        t.columns.known().unwrap().iter().find(|c| c.name == name).unwrap()
    }
    fn logical(c: &Column) -> Option<LogicalType> {
        c.sql_type.known().map(|t| t.logical)
    }

    #[test]
    fn postgres_tables_columns_and_constraints() {
        let p = parse_ddl(PG, Dialect::PostgreSql).unwrap();
        let names: Vec<_> = p.snapshot.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["customers", "order_tags", "public.orders"]);
        let customers = table(&p, "customers");
        let id = column(customers, "id");
        assert_eq!((logical(id), id.generated.clone(), id.nullable.clone()), (Some(LogicalType::Bigint), Fact::Known(true), Fact::Known(false)));
        assert_eq!(customers.primary_key, Fact::Known(vec!["id".into()]));
        let email = column(customers, "email");
        assert_eq!((logical(email), email.length, email.nullable.clone()), (Some(LogicalType::Varchar), Some(120), Fact::Known(false)));
        assert_eq!(email.sql_type.known().unwrap().raw, "character varying(120)");
        let balance = column(customers, "balance");
        assert_eq!((balance.precision, balance.scale, balance.default.clone()), (Some(12), Some(2), Fact::Known(Some("0".into()))));
        assert_eq!(logical(column(customers, "created_at")), Some(LogicalType::TimestampTz));
        assert_eq!(column(customers, "created_at").default, Fact::Known(Some("now()".into())));
        assert_eq!(logical(column(customers, "avatar")), Some(LogicalType::Binary));
        assert!(column(customers, "Display Name").nullable == Fact::Known(true));
        assert!(matches!(column(customers, "tags").sql_type, Fact::Unknown(ref r) if r.contains("array")));
        assert!(matches!(column(customers, "meta").sql_type, Fact::Unknown(ref r) if r.contains("jsonb")));
        let uniques = customers.unique_constraints.known().unwrap();
        assert!(uniques.iter().any(|u| u.columns == ["email"]) && uniques.iter().any(|u| u.columns == ["token"] && u.name.as_deref() == Some("customers_token_key")));
        assert!(matches!(customers.indexes, Fact::Unknown(_)), "expression and partial indexes make indexes unknown");

        let orders = table(&p, "public.orders");
        assert_eq!(column(orders, "id").generated, Fact::Known(true));
        assert_eq!(orders.primary_key, Fact::Known(vec!["id".into()]));
        let fks = orders.foreign_keys.known().unwrap();
        assert_eq!(fks[0].ref_table, "customers");
        assert_eq!(fks[0].ref_columns, Fact::Known(vec!["id".into()]));
        assert_eq!(orders.indexes.known().unwrap()[0].columns, ["placed"]);
        assert!(orders.unique_constraints.known().unwrap().iter().any(|u| u.columns == ["note"]));

        let tags = table(&p, "order_tags");
        assert_eq!(tags.primary_key, Fact::Known(vec!["order_id".into(), "tag".into()]));
        assert_eq!(tags.foreign_keys.known().unwrap()[0].ref_table, "public.orders");
        let reasons: Vec<_> = p.skipped.iter().map(|s| (s.line, s.reason.as_str())).collect();
        assert!(reasons.iter().any(|(_, r)| r.contains("CREATE VIEW")), "{reasons:?}");
        assert!(!reasons.iter().any(|(_, r)| r.contains("COMMENT ON")), "{reasons:?}");
        assert_eq!(p.comments, [Comment { table: "public.orders".into(), column: None, text: "a; b".into() }]);
        assert!(reasons.iter().any(|(_, r)| r.contains("SET")));
        assert!(reasons.iter().any(|(_, r)| r.contains("owner") || r.contains("OWNER")));
        assert!(reasons.iter().any(|(_, r)| r.contains("jsonb")));
        let view = p.skipped.iter().find(|s| s.reason.contains("CREATE VIEW")).unwrap();
        assert_eq!(view.line, PG.lines().position(|l| l.starts_with("CREATE VIEW")).unwrap() + 1);
        assert!(p.snapshot.complete);
    }

    #[test]
    fn omitted_referenced_columns_resolve_to_the_primary_key() {
        let p = parse_ddl(PG, Dialect::PostgreSql).unwrap();
        let customers = table(&p, "customers");
        let fk = customers.foreign_keys.known().unwrap().iter().find(|f| f.name.as_deref() == Some("fk_x")).unwrap();
        assert_eq!(fk.ref_table, "public.orders");
        assert_eq!(fk.ref_columns, Fact::Known(vec!["id".into()]));
    }

    #[test]
    fn mysql_tables_columns_and_constraints() {
        let p = parse_ddl(MY, Dialect::MySql).unwrap();
        let customers = table(&p, "customers");
        let id = column(customers, "id");
        assert_eq!((logical(id), id.precision, id.generated.clone()), (Some(LogicalType::Decimal), Some(20), Fact::Known(true)));
        assert_eq!(id.sql_type.known().unwrap().raw, "bigint unsigned");
        assert_eq!(logical(column(customers, "age")), Some(LogicalType::Smallint));
        assert_eq!(column(customers, "age").default, Fact::Known(Some("NULL".into())));
        assert_eq!(logical(column(customers, "flag")), Some(LogicalType::Boolean));
        assert!(matches!(column(customers, "kind").sql_type, Fact::Unknown(ref r) if r.contains("enum")));
        assert_eq!(logical(column(customers, "body")), Some(LogicalType::Text));
        assert_eq!(column(customers, "seen").nullable, Fact::Known(true));
        assert_eq!(customers.primary_key, Fact::Known(vec!["id".into()]));
        assert_eq!(customers.unique_constraints.known().unwrap()[0].name.as_deref(), Some("uq_email"));
        assert!(matches!(customers.indexes, Fact::Unknown(_)), "prefix index is not modeled");
        let orders = table(&p, "orders");
        assert!(matches!(orders.columns, Fact::Unknown(_)), "ADD COLUMN is not applied, so columns are unknown");
        assert!(p.skipped.iter().any(|s| s.reason.contains("inline REFERENCES")));
        assert!(p.skipped.iter().any(|s| s.reason.contains("ADD COLUMN")));
        let t2 = parse_ddl(&MY.replace("ALTER TABLE orders ADD COLUMN x int;", ""), Dialect::MySql).unwrap();
        let orders = table(&t2, "orders");
        assert_eq!(column(orders, "customer_id").nullable, Fact::Known(true));
        assert_eq!(orders.foreign_keys.known().unwrap().len(), 1, "inline REFERENCES is ignored by MySQL");
        assert_eq!(orders.primary_key, Fact::Known(vec!["id".into()]));
        assert_eq!(column(orders, "code").length, Some(8));
        assert!(orders.unique_constraints.known().unwrap().iter().any(|u| u.name.as_deref() == Some("uq_code")));
    }

    #[test]
    fn comments_strings_and_quoted_identifiers_do_not_confuse_statements() {
        let sql = "/* a /* nested */ CREATE TABLE no (x int); */ CREATE TABLE \"a;b\" (\"x\"\"y\" int DEFAULT 'it''s; ok', n int); -- tail\nCREATE TABLE t2 (a int);";
        let p = parse_ddl(sql, Dialect::PostgreSql).unwrap();
        assert_eq!(p.snapshot.tables.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["a;b", "t2"]);
        assert_eq!(column(table(&p, "a;b"), "x\"y").default, Fact::Known(Some("'it''s; ok'".into())));
        let mysql = "CREATE TABLE `a``b` (c int) /* x */; -- c\nCREATE TABLE t(a int);--x\n";
        let p = parse_ddl(mysql, Dialect::MySql).unwrap();
        assert_eq!(p.snapshot.tables.len(), 2);
        assert!(p.snapshot.tables.iter().any(|t| t.name == "a`b"));
    }

    #[test]
    fn unterminated_input_is_reported_not_swallowed() {
        let p = parse_ddl("CREATE TABLE a (x int);\nCREATE TABLE b (y text DEFAULT 'oops", Dialect::PostgreSql).unwrap();
        assert_eq!(p.snapshot.tables.len(), 1);
        assert!(!p.snapshot.complete);
        assert!(p.skipped.iter().any(|s| s.line == 2 && s.reason.contains("unterminated string")));
    }

    #[test]
    fn unsupported_create_table_forms_are_skipped_and_mark_the_snapshot_incomplete() {
        let sql = "CREATE TEMP TABLE t (a int);\nCREATE TABLE c (LIKE a);\nCREATE TABLE d AS SELECT 1;\nCREATE TABLE e (a int) INHERITS (f);";
        let p = parse_ddl(sql, Dialect::PostgreSql).unwrap();
        assert!(!p.snapshot.complete);
        assert!(matches!(table(&p, "e").columns, Fact::Unknown(_)));
        assert!(matches!(table(&p, "c").columns, Fact::Unknown(_)));
        assert_eq!(p.skipped.iter().filter(|s| s.reason.contains("temporary") || s.reason.contains("unsupported CREATE TABLE form")).count(), 2);
    }

    #[test]
    fn pg_dump_identity_via_alter_and_computed_columns() {
        let sql = "CREATE TABLE t (id integer NOT NULL, c integer GENERATED ALWAYS AS (id + 1) STORED);\nALTER TABLE public.t ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (START WITH 1);\nALTER TABLE ONLY t ADD CONSTRAINT t_pkey PRIMARY KEY (id);";
        let p = parse_ddl(sql, Dialect::PostgreSql).unwrap();
        let t = table(&p, "t");
        assert_eq!(column(t, "id").generated, Fact::Known(true));
        assert!(matches!(column(t, "c").generated, Fact::Unknown(_)));
        assert_eq!(t.primary_key, Fact::Known(vec!["id".into()]));
    }

    #[test]
    fn dialect_is_detected_only_when_unambiguous() {
        assert_eq!(detect_dialect("CREATE TABLE `a` (id int AUTO_INCREMENT)"), Some(Dialect::MySql));
        assert_eq!(detect_dialect("CREATE TABLE a (id bigserial)"), Some(Dialect::PostgreSql));
        assert_eq!(detect_dialect("CREATE TABLE a (id int)"), None);
        assert_eq!(detect_dialect("CREATE TABLE `a` (id bigserial)"), None);
    }

    #[test]
    fn size_and_statement_bounds() {
        assert!(matches!(parse_ddl(&" ".repeat(MAX_SQL_BYTES + 1), Dialect::MySql), Err(DdlError::TooLarge { .. })));
        assert_eq!(parse_ddl(&";".repeat(MAX_STATEMENTS + 1), Dialect::MySql), Err(DdlError::TooManyStatements));
        let many: String = (0..501).map(|i| format!("CREATE TABLE t{i} (a int);")).collect();
        assert!(matches!(parse_ddl(&many, Dialect::MySql), Err(DdlError::Schema(SchemaError::TooManyTables { .. }))));
    }

    #[test]
    fn random_input_never_panics() {
        let pieces = [
            "CREATE ", "TABLE ", "INDEX ", "UNIQUE ", "ALTER ", "ADD ", "CONSTRAINT ", "PRIMARY ", "KEY ", "FOREIGN ", "REFERENCES ", "(", ")", ",", ";",
            "'", "\"", "`", "$$", "$a$", "--", "/*", "*/", "#", "\n", " ", "x", "é", "😀", "int", "varchar(", "12", "[", "]", "DEFAULT ", "NOT ", "NULL ",
            "GENERATED ", "ALWAYS ", "AS ", "IDENTITY ", "ON ", "ONLY ", "USING ", "WHERE ", "ENGINE=", "\\", "E'", "::", ".", "unsigned ", "timestamp ",
            "with ", "time ", "zone ", "double ", "precision ",
        ];
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for round in 0..3000 {
            let mut sql = String::new();
            for _ in 0..(next() % 60) {
                sql.push_str(pieces[(next() % pieces.len() as u64) as usize]);
            }
            for dialect in [Dialect::PostgreSql, Dialect::MySql] {
                let _ = parse_ddl(&sql, dialect);
                let cut = sql.char_indices().nth((round % 17) as usize).map_or(sql.len(), |c| c.0);
                let _ = parse_ddl(&sql[..cut], dialect);
            }
        }
        let valid = format!("{PG}{MY}");
        for cut in (0..valid.len()).step_by(7).filter(|c| valid.is_char_boundary(*c)) {
            let _ = parse_ddl(&valid[..cut], Dialect::PostgreSql);
            let _ = parse_ddl(&valid[..cut], Dialect::MySql);
        }
    }
    #[test]
    fn postgres_enum_types_comments_and_ordered_indexes() {
        let sql = "CREATE TYPE mood AS ENUM ('sad', 'it''s ok', 'happy');\nCREATE TYPE public.\"Odd\" AS ENUM ();\nCREATE TYPE bad AS ENUM (E'a\\n');\nCREATE TYPE pt AS (x int);\nCREATE TABLE person (id int PRIMARY KEY, m mood NOT NULL, o public.\"Odd\", other mood2);\nCOMMENT ON TABLE person IS 'People';\nCOMMENT ON COLUMN public.person.m IS 'Current mood';\nCOMMENT ON COLUMN person.id IS 'gone';\nCOMMENT ON COLUMN person.id IS NULL;\nCOMMENT ON COLUMN person.missing IS 'x';\nCOMMENT ON INDEX i IS 'x';\nCREATE INDEX i_m ON person (m DESC);\n";
        let p = parse_ddl(sql, Dialect::PostgreSql).unwrap();
        let names: Vec<_> = p.enum_types.iter().map(|e| (e.name.as_str(), e.values.len())).collect();
        assert_eq!(names, [("mood", 3), ("Odd", 0)]);
        assert_eq!(p.enum_types[0].values, ["sad", "it's ok", "happy"]);
        let columns: Vec<_> = p.column_enums.iter().map(|e| (e.column.as_str(), e.type_name.as_deref())).collect();
        assert_eq!(columns, [("m", Some("mood")), ("o", Some("Odd"))]);
        assert!(matches!(p.snapshot.tables[0].columns.known().unwrap()[1].sql_type, Fact::Unknown(_)));
        let comments: Vec<_> = p.comments.iter().map(|c| (c.table.as_str(), c.column.as_deref(), c.text.as_str())).collect();
        assert_eq!(comments, [("person", None, "People"), ("person", Some("m"), "Current mood")]);
        let reasons: Vec<_> = p.skipped.iter().map(|s| s.reason.as_str()).collect();
        assert!(reasons.iter().any(|r| r.contains("CREATE TYPE")), "{reasons:?}");
        assert!(reasons.iter().any(|r| r.contains("not a known column")), "{reasons:?}");
        assert!(reasons.iter().any(|r| r.contains("COMMENT ON")), "{reasons:?}");
        assert_eq!(p.ordered_indexes, [OrderedIndex { table: "person".into(), name: Some("i_m".into()), columns: vec!["m".into()] }]);
    }

    #[test]
    fn mysql_enum_columns_and_comments() {
        let sql = "CREATE TABLE `t` (\n  `id` int PRIMARY KEY COMMENT 'Key',\n  `k` enum('a','b''c',\"d\") NOT NULL COMMENT 'it''s',\n  `s` set('x','y'),\n  `e` enum('a\\n') COMMENT 'back\\slash'\n) ENGINE=InnoDB COMMENT='Things';\n";
        let p = parse_ddl(sql, Dialect::MySql).unwrap();
        assert_eq!(p.column_enums.len(), 1);
        assert_eq!((p.column_enums[0].column.as_str(), p.column_enums[0].type_name.clone()), ("k", None));
        assert_eq!(p.column_enums[0].values, ["a", "b'c", "d"]);
        let comments: Vec<_> = p.comments.iter().map(|c| (c.column.as_deref(), c.text.as_str())).collect();
        assert_eq!(comments, [(Some("id"), "Key"), (Some("k"), "it's"), (None, "Things")]);
        assert!(p.skipped.iter().any(|s| s.reason.contains("COMMENT with an escape sequence")), "{:?}", p.skipped);
    }

    #[test]
    fn reserved_words_follow_the_documented_lists() {
        assert!(is_reserved(Dialect::PostgreSql, "ORDER") && is_reserved(Dialect::PostgreSql, "user") && is_reserved(Dialect::PostgreSql, "left"));
        assert!(!is_reserved(Dialect::PostgreSql, "name") && !is_reserved(Dialect::PostgreSql, "key"));
        assert!(is_reserved(Dialect::MySql, "key") && is_reserved(Dialect::MySql, "Order") && is_reserved(Dialect::MySql, "rank"));
        assert!(!is_reserved(Dialect::MySql, "name") && !is_reserved(Dialect::MySql, "user"));
    }

    #[test]
    fn views_are_read_only_when_columns_and_types_are_derivable() {
        let sql = "CREATE TABLE t (id integer PRIMARY KEY, name varchar(40) NOT NULL, body jsonb, n numeric(8,2));\n\
CREATE VIEW v1 AS SELECT id, name FROM t;\n\
CREATE OR REPLACE VIEW public.v2 (a, b) AS SELECT x.id, x.name AS other FROM t x WHERE id > 1;\n\
CREATE MATERIALIZED VIEW IF NOT EXISTS mv (k) AS SELECT n FROM t WITH NO DATA;\n\
CREATE VIEW v3 AS SELECT * FROM t;\n\
CREATE VIEW v4 AS SELECT id + 1 AS i FROM t;\n\
CREATE VIEW v5 AS SELECT t.id FROM t JOIN t u ON true;\n\
CREATE VIEW v6 AS SELECT id FROM missing;\n\
CREATE VIEW v7 AS SELECT nope FROM t;\n\
CREATE VIEW v8 (a) AS SELECT id, name FROM t;\n\
CREATE TEMP VIEW v9 AS SELECT id FROM t;\n\
CREATE OR REPLACE MATERIALIZED VIEW v10 AS SELECT id FROM t;\n\
CREATE VIEW v1 AS SELECT id FROM t;\n\
CREATE VIEW v11 AS SELECT id FROM t UNION SELECT id FROM t;\n\
CREATE VIEW v12 AS SELECT body FROM t;\n";
        let p = parse_ddl(sql, Dialect::PostgreSql).unwrap();
        let names: Vec<_> = p.views.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, ["v1", "public.v2", "mv", "v12"], "{:?}", p.skipped);
        let v2 = &p.views[1];
        assert_eq!(v2.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(logical(&v2.columns[1]), Some(LogicalType::Varchar));
        assert_eq!(v2.columns[1].length, Some(40));
        assert_eq!(v2.columns[1].nullable, Fact::Known(true));
        assert!(p.views[2].materialized && !p.views[0].materialized);
        assert_eq!(p.views[2].columns[0].precision, Some(8));
        assert!(matches!(p.views[3].columns[0].sql_type, Fact::Unknown(_)));
        assert!(p.unmapped_columns.contains(&UnmappedColumn { table: "v12".into(), column: "body".into(), sql_type: "jsonb".into() }));
        assert!(p.unmapped_columns.contains(&UnmappedColumn { table: "t".into(), column: "body".into(), sql_type: "jsonb".into() }));
        for needle in ["v3:", "v4:", "v5:", "v6:", "v7:", "v8:", "v9:", "v10:", "v1:", "v11:"] {
            assert!(p.skipped.iter().any(|s| s.reason.starts_with(&format!("CREATE VIEW {needle}"))), "{needle} {:?}", p.skipped);
        }
        assert_eq!(p.snapshot.tables.len(), 1);
        let my = "CREATE TABLE `t` (`id` int PRIMARY KEY, `name` varchar(10));\nCREATE OR REPLACE ALGORITHM=MERGE DEFINER=`root`@`localhost` SQL SECURITY INVOKER VIEW `v` (`i`) AS SELECT `id` FROM `t` WITH CASCADED CHECK OPTION;\nCREATE MATERIALIZED VIEW m AS SELECT id FROM t;\n";
        let p = parse_ddl(my, Dialect::MySql).unwrap();
        assert_eq!(p.views.len(), 1, "{:?}", p.skipped);
        assert_eq!(p.views[0].columns[0].name, "i");
        assert!(p.skipped.iter().any(|s| s.reason.contains("CREATE MATERIALIZED")));
    }

    #[test]
    fn unmapped_column_types_keep_the_text_as_written() {
        let p = parse_ddl("CREATE TABLE g (id int PRIMARY KEY, shape geometry(Point,4326), tags text[], m mood, s varchar(5));", Dialect::PostgreSql).unwrap();
        let raws: Vec<_> = p.unmapped_columns.iter().map(|u| (u.column.as_str(), u.sql_type.as_str())).collect();
        assert_eq!(raws, [("shape", "geometry(Point,4326)"), ("tags", "text[]"), ("m", "mood")]);
    }
}
