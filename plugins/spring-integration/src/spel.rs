//! Strict syntax check of literal SpEL expressions (Spring Framework reference, "Spring
//! Expression Language": literals, property/method chains, indexers, `#var`, `@bean`/`&bean`,
//! `T(type)`, `new`, ternary/elvis, safe navigation, selection/projection, inline lists/maps,
//! textual operators incl. `matches`/`instanceof`/`between`). Checks syntax only; semantics
//! (types, variables, methods) are never judged.
use crate::*;

const MAX_LEN: usize = 4096;
const MAX_DEPTH: usize = 48;

#[derive(Debug, PartialEq)]
pub struct Parsed {
    pub error: Option<(usize, usize, String)>,
    /// Bean name and its byte range inside the expression.
    pub beans: Vec<(String, usize, usize)>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Ident,
    Num,
    Str,
    Op,
}

#[derive(Clone, Copy, Debug)]
struct Tok<'a> {
    kind: Kind,
    s: &'a str,
    at: usize,
    end: usize,
}

type Fail = Option<(usize, usize, String)>;

fn lex(src: &str) -> Result<Vec<Tok<'_>>, Fail> {
    let b = src.as_bytes();
    let mut out: Vec<Tok> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let after_dot = out.last().is_some_and(|t| t.kind == Kind::Op && matches!(t.s, "." | "?."));
        if c.is_ascii_alphabetic() || c == b'_' || (c == b'$' && !(after_dot && b.get(i + 1) == Some(&b'['))) || c >= 0x80 {
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$' || b[i] >= 0x80) {
                i += 1;
            }
            out.push(Tok { kind: Kind::Ident, s: &src[start..i], at: start, end: i });
            continue;
        }
        if c.is_ascii_digit() {
            if c == b'0' && matches!(b.get(i + 1), Some(b'x' | b'X')) {
                i += 2;
                let digits = i;
                while i < b.len() && b[i].is_ascii_hexdigit() {
                    i += 1;
                }
                if i == digits {
                    return Err(Some((start, i, "Hexadecimal literal has no digits.".into())));
                }
                if matches!(b.get(i), Some(b'l' | b'L')) {
                    i += 1;
                }
            } else {
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                if b.get(i) == Some(&b'.') && b.get(i + 1).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                if matches!(b.get(i), Some(b'e' | b'E')) {
                    let mut j = i + 1;
                    if matches!(b.get(j), Some(b'+' | b'-')) {
                        j += 1;
                    }
                    if b.get(j).is_some_and(u8::is_ascii_digit) {
                        while j < b.len() && b[j].is_ascii_digit() {
                            j += 1;
                        }
                        i = j;
                    }
                }
                if matches!(b.get(i), Some(b'l' | b'L' | b'f' | b'F' | b'd' | b'D')) {
                    i += 1;
                }
            }
            if b.get(i).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_') {
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                return Err(Some((start, i, "Malformed number literal.".into())));
            }
            out.push(Tok { kind: Kind::Num, s: &src[start..i], at: start, end: i });
            continue;
        }
        if c == b'\'' || c == b'"' {
            i += 1;
            loop {
                match b.get(i) {
                    None => return Err(Some((start, b.len(), "Unterminated string literal.".into()))),
                    Some(q) if *q == c => {
                        if b.get(i + 1) == Some(&c) {
                            i += 2;
                        } else {
                            i += 1;
                            break;
                        }
                    }
                    _ => i += 1,
                }
            }
            out.push(Tok { kind: Kind::Str, s: &src[start..i], at: start, end: i });
            continue;
        }
        let rest = &src[i..];
        let two = ["?:", "?.", "==", "!=", "<=", ">=", "&&", "||", "++", "--"].iter().find(|op| rest.starts_with(**op)).copied();
        let sel = after_dot && ["?[", "![", "^[", "$["].iter().any(|op| rest.starts_with(*op));
        let op = if sel { Some(&rest[..2]) } else { two };
        if let Some(op) = op {
            i += 2;
            out.push(Tok { kind: Kind::Op, s: op, at: start, end: i });
            continue;
        }
        if "+-*/%^()[]{},.:?!<>=#@&".contains(c as char) {
            i += 1;
            out.push(Tok { kind: Kind::Op, s: &src[start..i], at: start, end: i });
            continue;
        }
        let ch = src[i..].chars().next().unwrap_or(' ');
        return Err(Some((start, i + ch.len_utf8(), format!("Unexpected character '{ch}'."))));
    }
    Ok(out)
}

struct P<'a> {
    t: Vec<Tok<'a>>,
    i: usize,
    len: usize,
    depth: usize,
    beans: Vec<(String, usize, usize)>,
}

fn word(t: &Tok, names: &[&str]) -> bool {
    t.kind == Kind::Ident && names.iter().any(|n| t.s.eq_ignore_ascii_case(n))
}

impl<'a> P<'a> {
    fn peek(&self) -> Option<Tok<'a>> {
        self.t.get(self.i).copied()
    }

    fn op(&self, s: &str) -> bool {
        self.peek().is_some_and(|t| t.kind == Kind::Op && t.s == s)
    }

    fn eat(&mut self, s: &str) -> bool {
        let hit = self.op(s);
        self.i += usize::from(hit);
        hit
    }

    fn word_at(&self, names: &[&str]) -> bool {
        self.peek().is_some_and(|t| word(&t, names))
    }

    fn fail<T>(&self, msg: &str) -> Result<T, Fail> {
        Err(Some(match self.peek() {
            Some(t) => (t.at, t.end, format!("{msg} (found '{}')", t.s)),
            None => (self.len.saturating_sub(1), self.len, format!("{msg} (unexpected end of expression)")),
        }))
    }

    fn expect(&mut self, s: &str) -> Result<(), Fail> {
        if self.eat(s) { Ok(()) } else { self.fail(&format!("Expected '{s}'")) }
    }

    fn ident(&mut self) -> Result<Tok<'a>, Fail> {
        match self.peek() {
            Some(t) if t.kind == Kind::Ident => {
                self.i += 1;
                Ok(t)
            }
            _ => self.fail("Expected an identifier"),
        }
    }

    fn enter(&mut self) -> Result<(), Fail> {
        self.depth += 1;
        if self.depth > MAX_DEPTH { Err(None) } else { Ok(()) }
    }

    fn expr(&mut self) -> Result<(), Fail> {
        self.enter()?;
        self.logical_or()?;
        if self.eat("=") || self.eat("?:") {
            self.expr()?;
        } else if self.eat("?") {
            self.expr()?;
            self.expect(":")?;
            self.expr()?;
        }
        self.depth -= 1;
        Ok(())
    }

    fn logical_or(&mut self) -> Result<(), Fail> {
        self.logical_and()?;
        while self.eat("||") || self.wordy(&["or"]) {
            self.logical_and()?;
        }
        Ok(())
    }

    fn wordy(&mut self, names: &[&str]) -> bool {
        let hit = self.word_at(names);
        self.i += usize::from(hit);
        hit
    }

    fn logical_and(&mut self) -> Result<(), Fail> {
        self.relational()?;
        while self.eat("&&") || self.wordy(&["and"]) {
            self.relational()?;
        }
        Ok(())
    }

    fn relational(&mut self) -> Result<(), Fail> {
        self.sum()?;
        const SYM: [&str; 6] = ["==", "!=", "<", "<=", ">", ">="];
        if SYM.iter().any(|s| self.op(s)) || self.word_at(&["eq", "ne", "lt", "le", "gt", "ge", "instanceof", "matches", "between"]) {
            self.i += 1;
            self.sum()?;
        }
        Ok(())
    }

    fn sum(&mut self) -> Result<(), Fail> {
        self.product()?;
        while self.eat("+") || self.eat("-") {
            self.product()?;
        }
        Ok(())
    }

    fn product(&mut self) -> Result<(), Fail> {
        self.power()?;
        while self.eat("*") || self.eat("/") || self.eat("%") || self.wordy(&["div", "mod"]) {
            self.power()?;
        }
        Ok(())
    }

    fn power(&mut self) -> Result<(), Fail> {
        self.unary()?;
        if self.eat("^") {
            self.unary()?;
        }
        Ok(())
    }

    fn unary(&mut self) -> Result<(), Fail> {
        self.enter()?;
        if self.eat("+") || self.eat("-") || self.eat("!") || self.eat("++") || self.eat("--") || self.wordy(&["not"]) {
            self.unary()?;
        } else {
            self.primary()?;
            let _ = self.eat("++") || self.eat("--");
        }
        self.depth -= 1;
        Ok(())
    }

    fn args(&mut self) -> Result<(), Fail> {
        self.expect("(")?;
        if self.eat(")") {
            return Ok(());
        }
        loop {
            self.expr()?;
            if !self.eat(",") {
                break;
            }
        }
        self.expect(")")
    }

    fn type_name(&mut self) -> Result<(), Fail> {
        self.ident()?;
        while self.eat(".") {
            self.ident()?;
        }
        while self.op("[") {
            self.i += 1;
            self.expect("]")?;
        }
        Ok(())
    }

    fn primary(&mut self) -> Result<(), Fail> {
        let Some(t) = self.peek() else {
            return self.fail("Expected an expression");
        };
        match (t.kind, t.s) {
            (Kind::Num | Kind::Str, _) => self.i += 1,
            (Kind::Op, "(") => {
                self.i += 1;
                self.expr()?;
                self.expect(")")?;
            }
            (Kind::Op, "{") => {
                self.i += 1;
                self.inline()?;
            }
            (Kind::Op, "#") => {
                self.i += 1;
                self.ident()?;
                if self.op("(") {
                    self.args()?;
                }
            }
            (Kind::Op, "@" | "&") => {
                self.i += 1;
                match self.peek() {
                    Some(n) if n.kind == Kind::Ident => {
                        self.beans.push((n.s.into(), n.at, n.end));
                        self.i += 1;
                    }
                    Some(n) if n.kind == Kind::Str && n.s.starts_with('\'') && !n.s[1..n.s.len() - 1].contains("''") => {
                        self.beans.push((n.s[1..n.s.len() - 1].into(), n.at + 1, n.end - 1));
                        self.i += 1;
                    }
                    _ => return self.fail("Expected a bean name after the reference prefix"),
                }
            }
            (Kind::Op, "[") => self.index()?,
            (Kind::Ident, "T") if self.t.get(self.i + 1).is_some_and(|n| n.s == "(") => {
                self.i += 2;
                self.type_name()?;
                self.expect(")")?;
            }
            (Kind::Ident, n) if n.eq_ignore_ascii_case("new") => {
                self.i += 1;
                self.type_name_no_dims()?;
                if self.op("(") {
                    self.args()?;
                } else if self.op("[") {
                    while self.eat("[") {
                        if !self.op("]") {
                            self.expr()?;
                        }
                        self.expect("]")?;
                    }
                    if self.eat("{") {
                        self.inline()?;
                    }
                } else {
                    return self.fail("Expected constructor arguments or array dimensions");
                }
            }
            (Kind::Ident, _) => {
                self.i += 1;
                if self.op("(") {
                    self.args()?;
                }
            }
            _ => return self.fail("Expected an expression"),
        }
        self.postfix()
    }

    fn type_name_no_dims(&mut self) -> Result<(), Fail> {
        self.ident()?;
        while self.eat(".") {
            self.ident()?;
        }
        Ok(())
    }

    fn index(&mut self) -> Result<(), Fail> {
        self.expect("[")?;
        self.expr()?;
        self.expect("]")
    }

    fn inline(&mut self) -> Result<(), Fail> {
        if self.eat(":") {
            return self.expect("}");
        }
        if self.eat("}") {
            return Ok(());
        }
        let map = {
            self.expr()?;
            self.eat(":")
        };
        if map {
            self.expr()?;
            while self.eat(",") {
                self.expr()?;
                self.expect(":")?;
                self.expr()?;
            }
        } else {
            while self.eat(",") {
                self.expr()?;
            }
        }
        self.expect("}")
    }

    fn postfix(&mut self) -> Result<(), Fail> {
        loop {
            if self.op("[") {
                self.index()?;
                continue;
            }
            let safe = self.op("?.");
            if !(safe || self.op(".")) {
                return Ok(());
            }
            self.i += 1;
            if safe && self.op("[") {
                self.index()?;
                continue;
            }
            if let Some(t) = self.peek().filter(|t| t.kind == Kind::Op && matches!(t.s, "?[" | "![" | "^[" | "$[")) {
                self.i += 1;
                let _ = t;
                self.expr()?;
                self.expect("]")?;
                continue;
            }
            self.ident()?;
            if self.op("(") {
                self.args()?;
            }
        }
    }
}

pub fn parse(src: &str) -> Option<Parsed> {
    if src.trim().is_empty() || src.len() > MAX_LEN {
        return None;
    }
    let run = || -> Result<Vec<(String, usize, usize)>, Fail> {
        let t = lex(src)?;
        let mut p = P { t, i: 0, len: src.len(), depth: 0, beans: Vec::new() };
        p.expr()?;
        if p.peek().is_some() {
            return p.fail("Unexpected token after the end of the expression");
        }
        Ok(p.beans)
    };
    match run() {
        Ok(beans) => Some(Parsed { error: None, beans }),
        Err(Some(e)) => Some(Parsed { error: Some(e), beans: Vec::new() }),
        Err(None) => None,
    }
}

/// References for a literal expression written at byte `at` of `text`, with `value == text[at..at+len]`.
/// Placeholders and anything not byte-identical in the source are withheld.
pub fn refs(text: &str, at: usize, value: &str, what: &str, owner: usize) -> Vec<Ref> {
    if beans::is_expression(value) || value.contains('\\') || text.get(at..at + value.len()) != Some(value) {
        return Vec::new();
    }
    let Some(parsed) = parse(value) else {
        return Vec::new();
    };
    if let Some((from, to, message)) = parsed.error {
        return vec![Ref { name: message, role: Role::SpelError, what: what.into(), range: range(text, at + from, at + to), checked: false, owner }];
    }
    parsed
        .beans
        .into_iter()
        .map(|(name, from, to)| Ref { name, role: Role::BeanRef, what: format!("SpEL @bean in {what}"), range: range(text, at + from, at + to), checked: false, owner })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(src: &str) {
        let p = parse(src).expect("parsed");
        assert_eq!(p.error, None, "{src}");
    }

    fn bad(src: &str) -> (usize, usize) {
        let p = parse(src).expect("parsed");
        let (a, b, _) = p.error.unwrap_or_else(|| panic!("accepted: {src}"));
        (a, b)
    }

    #[test]
    fn accepts_reference_grammar() {
        for s in [
            "payload.size() > 0",
            "headers['type']",
            "payload.toUpperCase()",
            "#root.payload?.name ?: 'x'",
            "a ? b : c",
            "@orderService.find(payload.id)",
            "@'my.bean'.go()",
            "&factory",
            "T(java.lang.Math).PI * 2",
            "T(String[])",
            "payload.items.?[price > 10].![name]",
            "payload.items.^[#this > 1] + payload.items.$[true]",
            "list?.?[x]",
            "map?.[1]",
            "{1,2,3}",
            "{{1,2},{3}}",
            "{name:'x', age:3}",
            "{:}",
            "{}",
            "5 between {1, 10}",
            "payload instanceof T(String) and not headers.isEmpty()",
            "payload MATCHES '^\\\\d+$' OR payload == null",
            "'it''s' + \"a\"\"b\"",
            "0x7FFFFFFF + 6.0221415E+23 + 3L + 1.5f",
            "new java.util.Date().time",
            "new int[4]",
            "new int[]{1,2}",
            "#f(1, 2).x[0]",
            "-2^31",
            "1 div 2 mod 3 + 1 eq 2",
            "name = 'x'",
            "counter++ + 2",
            "[0]",
            "payload.get(\"a\")",
            "!(a)",
            "a.b?.c(d)[1]",
        ] {
            ok(s);
        }
    }

    #[test]
    fn rejects_with_exact_range() {
        assert_eq!(bad("payload.size( > 0"), (14, 15));
        assert_eq!(bad("a +"), (2, 3));
        assert_eq!(bad("'abc"), (0, 4));
        assert_eq!(bad("a ? b"), (4, 5));
        assert_eq!(bad("(a"), (1, 2));
        assert_eq!(bad("a b"), (2, 3));
        assert_eq!(bad("#"), (0, 1));
        assert_eq!(bad("@"), (0, 1));
        assert_eq!(bad("a..b"), (2, 3));
        assert_eq!(bad("a ; b"), (2, 3));
        assert_eq!(bad("payload.?[x"), (10, 11));
        assert_eq!(bad("{1,}"), (3, 4));
        assert_eq!(bad("1abc"), (0, 4));
        assert_eq!(bad("T(a.)"), (4, 5));
        assert_eq!(bad("a == == b"), (5, 7));
        assert_eq!(bad("headers[]"), (8, 9));
    }

    #[test]
    fn collects_bean_references() {
        let p = parse("@a.x() + @'b.c'.y + &f").expect("parsed");
        assert_eq!(p.beans.iter().map(|b| b.0.as_str()).collect::<Vec<_>>(), ["a", "b.c", "f"]);
        assert_eq!(p.beans[0].1..p.beans[0].2, 1..2);
    }

    #[test]
    fn stays_quiet_on_placeholders_empty_and_unmatched_text() {
        assert!(refs("x", 0, "#{a +", "w", 1).is_empty());
        assert!(refs("x", 0, "${a +", "w", 1).is_empty());
        assert!(parse("  ").is_none());
        assert!(refs("zzz", 0, "a +", "w", 1).is_empty());
        let r = refs("ab a +", 3, "a +", "w", 1);
        assert_eq!(r[0].role, Role::SpelError);
    }
}
