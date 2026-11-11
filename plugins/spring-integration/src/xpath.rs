//! Copied from plugins/spring-ws/src/xpath_syntax.rs (plugins are separate crates; no shared crate).
//! Strict XPath 1.0 syntax check (W3C grammar, tokenizer + recursive descent).
//! Produces only a first syntax error; nothing is evaluated or resolved.
use crate::*;
const MAX_LEN: usize = 4096;
const MAX_DEPTH: usize = 48;
const AXES: [&str; 13] = [
    "ancestor", "ancestor-or-self", "attribute", "child", "descendant", "descendant-or-self",
    "following", "following-sibling", "namespace", "parent", "preceding", "preceding-sibling", "self",
];
const LEVELS: [&[&str]; 6] = [&["or"], &["and"], &["=", "!="], &["<", "<=", ">", ">="], &["+", "-"], &["*", "div", "mod"]];
const PUNCT: [&str; 21] = [
    "(", ")", "[", "]", ",", "@", "|", "+", "-", "=", "/", "//", "!=", "<", "<=", ">", ">=", "::", ".", "..", "*",
];

#[derive(Clone, Copy, PartialEq)]
enum K {
    Num,
    Lit,
    Var,
    Name,
    P(&'static str),
}
#[derive(Clone, Copy)]
struct T {
    k: K,
    a: usize,
    b: usize,
}
pub(crate) type E = (String, usize, usize);

fn ncname_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}
fn ncname_char(c: char) -> bool {
    ncname_start(c) || c.is_alphanumeric() || matches!(c, '-' | '.')
}
fn ncname(s: &str, at: usize) -> Option<usize> {
    let rest = s.get(at..)?;
    let mut chars = rest.char_indices();
    if !ncname_start(chars.next()?.1) {
        return None;
    }
    Some(at + chars.find(|(_, c)| !ncname_char(*c)).map_or(rest.len(), |(i, _)| i))
}
fn punct(s: &str) -> K {
    K::P(PUNCT.iter().chain(&["and", "or", "div", "mod"]).find(|p| **p == s).copied().unwrap_or(""))
}

fn lex(s: &str) -> Result<Vec<T>, E> {
    let mut out: Vec<T> = vec![];
    let mut i = 0;
    while i < s.len() {
        let c = s[i..].chars().next().unwrap_or(' ');
        if matches!(c, ' ' | '\t' | '\r' | '\n') {
            i += 1;
            continue;
        }
        let after_operand = out.last().is_some_and(|t| match t.k {
            K::P(p) => matches!(p, ")" | "]" | "." | ".."),
            _ => true,
        });
        let rest = &s[i..];
        let fail = |m: &str, n: usize| -> E { (m.into(), i, i + n.max(1)) };
        let (k, n) = match c {
            '/' | '<' | '>' | '!' | ':' | '.' if rest.len() > 1 => {
                let two = rest.get(..2).unwrap_or("");
                if matches!(two, "//" | "<=" | ">=" | "!=" | "::" | "..") {
                    (punct(two), 2)
                } else if c == '.' && rest.as_bytes()[1].is_ascii_digit() {
                    (K::Num, number(rest))
                } else if matches!(c, '!' | ':') {
                    return Err(fail("unexpected character", 1));
                } else {
                    (punct(&rest[..1]), 1)
                }
            }
            '!' | ':' => return Err(fail("unexpected character", 1)),
            '(' | ')' | '[' | ']' | ',' | '@' | '|' | '+' | '-' | '=' | '/' | '<' | '>' | '.' => (punct(&rest[..1]), 1),
            '*' if after_operand => (K::P("*"), 1),
            '*' => (K::Name, 1),
            '0'..='9' => (K::Num, number(rest)),
            '"' | '\'' => match rest[1..].find(c) {
                Some(end) => (K::Lit, end + 2),
                None => return Err(fail("unterminated string literal", rest.len())),
            },
            '$' => {
                let end = qname(s, i + 1).ok_or_else(|| fail("expected variable name after `$`", 1))?;
                (K::Var, end.0 - i)
            }
            c if ncname_start(c) => {
                let first = ncname(s, i).unwrap_or(i + 1);
                let word = &s[i..first];
                if after_operand && matches!(word, "and" | "or" | "div" | "mod") {
                    (punct(word), first - i)
                } else {
                    let (end, _) = qname(s, i).ok_or_else(|| fail("invalid name", 1))?;
                    (K::Name, end - i)
                }
            }
            _ => return Err(fail("unexpected character", c.len_utf8())),
        };
        out.push(T { k, a: i, b: i + n });
        i += n;
    }
    Ok(out)
}
fn number(s: &str) -> usize {
    let digits = |from: usize| s[from..].bytes().take_while(u8::is_ascii_digit).count();
    let mut n = digits(0);
    if s[n..].starts_with('.') {
        n += 1 + digits(n + 1);
    }
    n
}
fn qname(s: &str, at: usize) -> Option<(usize, bool)> {
    let first = ncname(s, at)?;
    if s[first..].starts_with(':') && !s[first..].starts_with("::") {
        if s[first + 1..].starts_with('*') {
            return Some((first + 2, true));
        }
        return ncname(s, first + 1).map(|e| (e, false));
    }
    Some((first, false))
}

struct P<'a> {
    s: &'a str,
    t: Vec<T>,
    i: usize,
    depth: usize,
}
impl P<'_> {
    fn err<R>(&self, m: &str) -> Result<R, E> {
        let t = self.t.get(self.i).or(self.t.last()).copied().unwrap_or(T { k: K::Num, a: 0, b: 0 });
        Err((m.into(), t.a, t.b))
    }
    fn at(&self, n: usize) -> Option<K> {
        self.t.get(self.i + n).map(|t| t.k)
    }
    fn is(&self, p: &str) -> bool {
        matches!(self.at(0), Some(K::P(x)) if x == p)
    }
    fn text(&self, n: usize) -> &str {
        self.t.get(self.i + n).map_or("", |t| &self.s[t.a..t.b])
    }
    fn eat(&mut self, p: &str) -> bool {
        let hit = self.is(p);
        self.i += usize::from(hit);
        hit
    }
    fn expect(&mut self, p: &str) -> Result<(), E> {
        if self.eat(p) {
            return Ok(());
        }
        self.err(&format!("expected `{p}`"))
    }
    fn expr(&mut self) -> Result<(), E> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return self.err("expression nested too deeply");
        }
        self.binary(0)?;
        self.depth -= 1;
        Ok(())
    }
    fn binary(&mut self, level: usize) -> Result<(), E> {
        if level == LEVELS.len() {
            return self.unary();
        }
        self.binary(level + 1)?;
        while LEVELS[level].iter().any(|op| self.is(op)) {
            self.i += 1;
            self.binary(level + 1)?;
        }
        Ok(())
    }
    fn unary(&mut self) -> Result<(), E> {
        while self.eat("-") {}
        self.path()?;
        while self.eat("|") {
            self.path()?;
        }
        Ok(())
    }
    fn node_type(&self, n: usize) -> bool {
        matches!(self.text(n), "comment" | "text" | "processing-instruction" | "node")
    }
    fn path(&mut self) -> Result<(), E> {
        match self.at(0) {
            Some(K::P("/")) => {
                self.i += 1;
                if matches!(self.at(0), Some(K::Name) | Some(K::P("@" | "." | ".."))) {
                    self.relative()?;
                }
                Ok(())
            }
            Some(K::P("//")) => {
                self.i += 1;
                self.relative()
            }
            Some(K::Name) if self.at(1) == Some(K::P("(")) && !self.node_type(0) => self.filter(),
            Some(K::Name | K::P("@" | "." | "..")) => self.relative(),
            Some(K::Var | K::Lit | K::Num | K::P("(")) => self.filter(),
            _ => self.err("expected an expression"),
        }
    }
    fn filter(&mut self) -> Result<(), E> {
        match self.at(0) {
            Some(K::Var | K::Lit | K::Num) => self.i += 1,
            Some(K::P("(")) => {
                self.i += 1;
                self.expr()?;
                self.expect(")")?;
            }
            _ => {
                if self.text(0).ends_with('*') {
                    return self.err("a function name cannot be a wildcard");
                }
                self.i += 2;
                if !self.eat(")") {
                    loop {
                        self.expr()?;
                        if !self.eat(",") {
                            break;
                        }
                    }
                    self.expect(")")?;
                }
            }
        }
        while self.is("[") {
            self.predicate()?;
        }
        if self.eat("/") || self.eat("//") {
            self.relative()?;
        }
        Ok(())
    }
    fn relative(&mut self) -> Result<(), E> {
        self.step()?;
        while self.eat("/") || self.eat("//") {
            self.step()?;
        }
        Ok(())
    }
    fn step(&mut self) -> Result<(), E> {
        if self.eat(".") || self.eat("..") {
            return Ok(());
        }
        if !self.eat("@") && self.at(0) == Some(K::Name) && self.at(1) == Some(K::P("::")) {
            if !AXES.contains(&self.text(0)) {
                return self.err("unknown axis");
            }
            self.i += 2;
        }
        if self.at(0) != Some(K::Name) {
            return self.err("expected a node test");
        }
        if self.at(1) == Some(K::P("(")) {
            if !self.node_type(0) {
                return self.err("function calls are not allowed as location steps");
            }
            let pi = self.text(0) == "processing-instruction";
            self.i += 2;
            if pi && self.at(0) == Some(K::Lit) {
                self.i += 1;
            }
            self.expect(")")?;
        } else {
            self.i += 1;
        }
        while self.is("[") {
            self.predicate()?;
        }
        Ok(())
    }
    fn predicate(&mut self) -> Result<(), E> {
        self.expect("[")?;
        self.expr()?;
        self.expect("]")
    }
}

/// First syntax error as (message, start, end) byte offsets within `s`.
pub fn check(s: &str) -> Result<(), E> {
    if s.len() > MAX_LEN {
        return Ok(());
    }
    let t = lex(s)?;
    if t.is_empty() {
        return Err(("empty XPath expression".into(), 0, 0));
    }
    let mut p = P { s, t, i: 0, depth: 0 };
    p.expr()?;
    if p.i < p.t.len() {
        return p.err("unexpected token");
    }
    Ok(())
}

/// A syntax-error reference for a literal XPath written at byte `at` of `text`; placeholders stay quiet.
pub fn refs(text: &str, at: usize, value: &str, what: &str, owner: usize) -> Vec<Ref> {
    if beans::is_expression(value) || value.contains('\\') || text.get(at..at + value.len()) != Some(value) {
        return Vec::new();
    }
    match check(value) {
        Err((message, from, to)) => vec![Ref { name: message, role: Role::XPathError, what: what.into(), range: range(text, at + from, at + to), checked: false, owner }],
        Ok(()) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_valid_grammar() {
        for ok in [
            "/", "/a", "//a", "a/b//c", ".", "..", "./a", "../a", "@id", "a/@id", "@*", "*", "p:*", "p:a", "/p:a/p:b",
            "child::a", "descendant-or-self::node()", "ancestor::*", "self::node()", "following-sibling::p:x[1]",
            "preceding::text()", "namespace::*", "attribute::id", "parent::a", "comment()", "processing-instruction()",
            "processing-instruction('x')", "a[1]", "a[b][c='d']", "a[@id=\"x\"]", "a[position()=last()]", "(a|b)[1]",
            "(/a)[2]/b", "$v", "$p:v/a", "id('x')/a", "count(//a)", "concat('a', 'b', c)", "f()", "1", "1.", ".5", "1.5", "'x'",
            "\"x'y\"", "1+2*3", "1 - 2", "a-b", "a - b", "a-b-c", "a.b", "a mod b", "a div b", "a or b and c", "a and b or c",
            "a = b", "a != b", "a < b", "a <= b", "a > b", "a >= b",
            "- 1", "--1", "a | b | c", "2 * 3", "* * *", "a/* * 2", "div div div", "and and and", "or or or", "mod mod mod",
            "a/and", "/and", "/div/mod", "a[1] * 2", "(1)", "((a))", "a//b[c]/@d", "//*[@id]", "text()", "node()/a",
            "a[b[c[d]]]", "@a:b", "child::*[1][2]", "$x[1]/a", "f(1)[2]/a", "a/b|c/d", "1 = 2 = 3",
            "p:f(1)", "élan/ñ", "_a", "a1", "div", "mod", "and", "or",
        ] {
            assert!(check(ok).is_ok(), "{ok}: {:?}", check(ok));
        }
    }
    #[test]
    fn rejects_invalid_grammar() {
        for bad in [
            "", "   ", "a[", "a[]", "a]", "(a", "a)", "a b", "a//", "a/", "//", "a +", "+ a", "a ++ b", "* 2", "a ! b", "a !",
            "a : b", ":a", "a:", "a:b:c", "1.2.3", "'x", "\"x", "$", "$ x", "a/$b/c", "foo::a", "child::", "child:: a::b",
            "a/b()", "text(a)", "node(", "f(,)", "f(1,)", "f(,1)", "p:*()", ".[1]", "..[1]", ".a", "@", "@@a", "a/@", "a|", "|a",
            "1 +", "a = ", "= a", "a #", "a ? b", "{a}", "a/(b)", "a/1", "a/'x'", "a//$x", "processing-instruction(1)",
            "processing-instruction('x','y')", "a[1", "[1]", "a[1]]", "a<", "a=>b", "a &", "1a", "-", "a / / b", "a b c", "@*:x",
            "::a", "child::::a", "(", ")", "()", "(1", "f(", "f(1", "f(1,", "a/*b", "1e3",
        ] {
            assert!(check(bad).is_err(), "{bad:?} should fail");
        }
    }
    #[test]
    fn reports_byte_range_of_offending_token() {
        assert_eq!(check("a/b[").unwrap_err().1..check("a/b[").unwrap_err().2, 3..4);
        assert_eq!(check("a ! b").unwrap_err().1, 2);
        assert_eq!(check("a b").unwrap_err().1, 2);
        assert_eq!(check("élan b").unwrap_err().1, 6);
    }
    #[test]
    fn bounds_depth_and_length() {
        assert!(check(&format!("{}a{}", "(".repeat(100), ")".repeat(100))).is_err());
        assert!(check(&format!("{}a{}", "(".repeat(20), ")".repeat(20))).is_ok());
        assert!(check(&format!("{}(", "a".repeat(5000))).is_ok());
    }
    #[test]
    fn refs_flag_exact_range_and_skip_placeholders() {
        let text = "<x a=\"/a/b[\"/>";
        let r = refs(text, 6, "/a/b[", "<int-xml:xpath-router>", 1);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].role, Role::XPathError);
        assert!(refs("<x a=\"${p}\"/>", 6, "${p}", "x", 1).is_empty());
        assert!(refs(text, 6, "/a/b", "x", 1).is_empty());
    }
}
