//! Literal programmatic registrations: `RuntimeWiringConfigurer` bodies calling
//! `type("T", b -> b.dataFetcher("f", ..))` or `type(newTypeWiring("T").dataFetcher("f", ..))`.
//! Anything else touching the wiring API is reported as `closed`.

use crate::Handler;
use spring_text::range;

pub struct Wired {
    pub handlers: Vec<Handler>,
    /// Some use of the wiring API is not a recognised literal registration.
    pub closed: bool,
}

const FORBIDDEN: [&str; 6] = ["DataFetcher", "DataLoader", "GraphQlSource", "codeRegistry", "WiringFactory", "wiringFactory"];

fn mask(text: &str) -> Vec<u8> {
    let b = text.as_bytes();
    let mut m = b.to_vec();
    let blank = |m: &mut Vec<u8>, from: usize, to: usize| {
        for x in &mut m[from..to] {
            if *x != b'\n' {
                *x = b' ';
            }
        }
    };
    let mut i = 0;
    let mut line_start = true;
    while i < b.len() {
        let rest = &b[i..];
        if line_start && (rest.starts_with(b"import ") || rest.starts_with(b"package ")) {
            let end = b[i..].iter().position(|&c| c == b'\n').map_or(b.len(), |p| i + p);
            blank(&mut m, i, end);
            i = end;
            continue;
        }
        if !rest[0].is_ascii_whitespace() {
            line_start = false;
        }
        if rest[0] == b'\n' {
            line_start = true;
        }
        let (skip, close): (usize, &[u8]) = if rest.starts_with(b"//") {
            (2, b"\n")
        } else if rest.starts_with(b"/*") {
            (2, b"*/")
        } else if rest.starts_with(b"\"\"\"") {
            (3, b"\"\"\"")
        } else if rest[0] == b'"' {
            (1, b"\"")
        } else if rest[0] == b'\'' {
            (1, b"'")
        } else {
            i += 1;
            continue;
        };
        let mut j = i + skip;
        while j < b.len() && !b[j..].starts_with(close) {
            j += if b[j] == b'\\' && close != b"\n" { 2 } else { 1 };
        }
        let j = j.min(b.len());
        blank(&mut m, i + skip, j);
        i = (j + if close == b"\n" { 0 } else { close.len() }).min(b.len());
    }
    m
}

fn ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

fn ws(m: &[u8], mut i: usize) -> usize {
    while i < m.len() && m[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn eat(m: &[u8], i: usize, s: &str) -> Option<usize> {
    let i = ws(m, i);
    m[i..].starts_with(s.as_bytes()).then_some(i + s.len())
}

fn word(m: &[u8], i: usize, w: &str) -> Option<usize> {
    let i = ws(m, i);
    let end = i + w.len();
    (m[i..].starts_with(w.as_bytes()) && !m.get(end).is_some_and(|&c| ident(c))).then_some(end)
}

fn matching(m: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0;
    for (k, &c) in m.iter().enumerate().skip(open) {
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }
    None
}

fn gql_name(s: &str) -> bool {
    !s.is_empty() && s.bytes().enumerate().all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || i > 0 && b.is_ascii_digit())
}

/// A plain string literal at `i`: (value, content start, content end, index after the closing quote).
fn literal<'a>(m: &[u8], text: &'a str, i: usize) -> Option<(&'a str, usize, usize, usize)> {
    let p = ws(m, i);
    if m.get(p) != Some(&b'"') || m[p..].starts_with(b"\"\"\"") {
        return None;
    }
    let q = p + 1 + m[p + 1..].iter().position(|&c| c == b'"')?;
    let value = &text[p + 1..q];
    (gql_name(value)).then_some((value, p + 1, q, q + 1))
}

struct Parsed<'a> {
    text: &'a str,
    m: &'a [u8],
    out: Vec<Handler>,
    new_type_wirings: usize,
}

impl Parsed<'_> {
    /// `.dataFetcher("f", ..)` repeated, from `i` to `end` exactly; at least one.
    fn calls(&mut self, ty: &str, mut i: usize, end: usize) -> Option<usize> {
        let mut any = false;
        while let Some(dot) = eat(self.m, i, ".") {
            let name = word(self.m, dot, "dataFetcher")?;
            let open = eat(self.m, name, "(")? - 1;
            let close = matching(self.m, open)?;
            let (field, from, to, after) = literal(self.m, self.text, open + 1)?;
            eat(self.m, after, ",")?;
            let at = ws(self.m, dot);
            self.out.push(Handler {
                op: None,
                ty: ty.to_owned(),
                field: field.to_owned(),
                range: range(self.text, from, to),
                method: range(self.text, at, name),
                annotation: range(self.text, at, close + 1),
                label: format!("dataFetcher(\"{ty}.{field}\")"),
                args: Vec::new(),
            });
            any = true;
            i = close + 1;
        }
        (any && ws(self.m, i) == end).then_some(end)
    }

    /// The arguments of `type(` between `open + 1` and `close`.
    fn type_call(&mut self, open: usize, close: usize) -> Option<()> {
        let m = self.m;
        if let Some((ty, _, _, after)) = literal(m, self.text, open + 1) {
            let mut i = eat(m, after, ",")?;
            let param;
            if let Some(p) = eat(m, i, "(") {
                let s = ws(m, p);
                let e = s + m[s..].iter().take_while(|&&c| ident(c)).count();
                param = (s, e);
                i = eat(m, eat(m, e, ")")?, "->")?;
            } else {
                let s = ws(m, i);
                let e = s + m[s..].iter().take_while(|&&c| ident(c)).count();
                param = (s, e);
                i = eat(m, e, "->")?;
            }
            if param.0 == param.1 {
                return None;
            }
            let name = &m[param.0..param.1];
            let (mut i, mut end) = (ws(m, i), close);
            if m.get(i) == Some(&b'{') {
                if matching(m, i)? + 1 != ws_back(m, end) {
                    return None;
                }
                end = ws_back(m, end) - 1;
                i += 1;
            }
            let mut any = false;
            loop {
                while m.get(ws(m, i)) == Some(&b';') {
                    i = ws(m, i) + 1;
                }
                if ws(m, i) >= end {
                    break;
                }
                let s = ws(m, i);
                if !m[s..].starts_with(name) {
                    return None;
                }
                let stop = statement_end(m, s + name.len(), end);
                self.calls(ty, s + name.len(), stop)?;
                any = true;
                i = stop;
            }
            return any.then_some(());
        }
        let mut i = ws(self.m, open + 1);
        if let Some(a) = word(m, i, "TypeRuntimeWiring") {
            i = eat(m, a, ".")?;
        }
        let a = word(m, i, "newTypeWiring")?;
        let paren = eat(m, a, "(")? - 1;
        let (ty, _, _, after) = literal(m, self.text, paren + 1)?;
        let rparen = eat(m, after, ")")?;
        self.calls(ty, rparen, close)?;
        self.new_type_wirings += 1;
        Some(())
    }
}

fn ws_back(m: &[u8], mut end: usize) -> usize {
    while end > 0 && m[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    end
}

/// End of the statement starting at `from`: the next `;` at depth 0, else `end`.
fn statement_end(m: &[u8], from: usize, end: usize) -> usize {
    let mut depth = 0i32;
    for k in from..end {
        match m[k] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b';' if depth == 0 => return k,
            _ => {}
        }
    }
    end
}

fn find_words(m: &[u8], w: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut k = 0;
    while let Some(p) = m[k..].windows(w.len()).position(|x| x == w.as_bytes()) {
        let at = k + p;
        if at == 0 || !ident(m[at - 1]) {
            out.push(at);
        }
        k = at + w.len();
    }
    out
}

fn count(m: &[u8], w: &str) -> usize {
    m.windows(w.len()).filter(|x| *x == w.as_bytes()).count()
}

pub fn wired(text: &str) -> Wired {
    let masked = mask(text);
    let m = masked.as_slice();
    let mut p = Parsed { text, m, out: Vec::new(), new_type_wirings: 0 };
    let type_calls: Vec<usize> = find_words(m, "type")
        .into_iter()
        .filter(|&k| ws_back(m, k) > 0 && m[ws_back(m, k) - 1] == b'.' && m.get(ws(m, k + 4)) == Some(&b'('))
        .collect();
    let mut recognised = 0;
    for k in &type_calls {
        let open = ws(m, k + 4);
        let Some(close) = matching(m, open) else { continue };
        let (saved, saved_new) = (p.out.len(), p.new_type_wirings);
        if p.type_call(open, close).is_some() {
            recognised += 1;
        } else {
            p.out.truncate(saved);
            p.new_type_wirings = saved_new;
        }
    }
    let Parsed { out, new_type_wirings, .. } = p;
    let runtime_wiring = find_words(m, "RuntimeWiring");
    let stray_wiring = runtime_wiring.iter().any(|&k| {
        let rest = &m[k + "RuntimeWiring".len()..];
        !rest.starts_with(b"Configurer") && !rest.starts_with(b".Builder")
    });
    let mentions = !runtime_wiring.is_empty() || count(m, "TypeRuntimeWiring") > 0 || count(m, "newTypeWiring") > 0;
    let closed = FORBIDDEN.iter().any(|w| count(m, w) > 0)
        || stray_wiring
        || count(m, "TypeRuntimeWiring") != find_words(m, "TypeRuntimeWiring").iter().filter(|&&k| m[k + 17..].starts_with(b".newTypeWiring")).count()
        || count(m, "newTypeWiring") != new_type_wirings
        || count(m, "dataFetcher") != out.len()
        || mentions && (out.is_empty() || type_calls.len() != recognised);
    Wired { handlers: out, closed }
}
