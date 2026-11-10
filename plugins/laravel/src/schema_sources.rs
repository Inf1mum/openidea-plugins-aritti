//! Bounded SQL CREATE TABLE schema facts, without executing SQL.
use crate::migrations::Column;
#[derive(Debug)]
struct Token {
    text: String,
    at: usize,
    word: bool,
}
fn lex(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if bytes[at..].starts_with(b"--") {
            at = text[at..].find('\n').map_or(bytes.len(), |n| at + n);
            continue;
        }
        if bytes[at..].starts_with(b"/*") {
            at = text[at + 2..]
                .find("*/")
                .map_or(bytes.len(), |n| at + 2 + n + 2);
            continue;
        }
        if matches!(bytes[at], b'\'' | b'"' | b'`' | b'[') {
            let quote = bytes[at];
            let close = if quote == b'[' { b']' } else { quote };
            let start = at + 1;
            at += 1;
            let mut value = String::new();
            let mut last = at;
            let mut closed = false;
            while at < bytes.len() {
                if bytes[at] == close {
                    value.push_str(&text[last..at]);
                    if bytes.get(at + 1) == Some(&close) {
                        value.push(close as char);
                        at += 2;
                        last = at;
                        continue;
                    }
                    at += 1;
                    closed = true;
                    break;
                }
                if quote == b'\'' && bytes[at] == b'\\' {
                    at = (at + 2).min(bytes.len());
                } else {
                    at += 1;
                }
            }
            if !closed {
                break;
            }
            out.push(Token {
                text: value,
                at: start,
                word: quote != b'\'',
            });
            continue;
        }
        let start = at;
        if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' {
            at += 1;
            while at < bytes.len()
                && (bytes[at].is_ascii_alphanumeric() || matches!(bytes[at], b'_' | b'$'))
            {
                at += 1;
            }
            out.push(Token {
                text: text[start..at].into(),
                at: start,
                word: true,
            });
        } else {
            at += text[at..].chars().next().unwrap().len_utf8();
            out.push(Token {
                text: text[start..at].into(),
                at: start,
                word: false,
            });
        }
    }
    out
}
fn is(ts: &[Token], at: usize, name: &str) -> bool {
    ts.get(at)
        .is_some_and(|t| t.word && t.text.eq_ignore_ascii_case(name))
}
/// CREATE TABLE definitions from conventional MySQL/Postgres/SQLite dumps.
/// Constraints, LIKE/AS SELECT and arbitrary ALTER expressions are withheld.
pub(crate) fn parse(text: &str) -> Vec<Column> {
    if text.len() > 2 << 20 {
        return Vec::new();
    }
    let ts = lex(text);
    let mut out = Vec::new();
    let mut at = 0;
    while at < ts.len() {
        if !is(&ts, at, "CREATE") {
            at += 1;
            continue;
        }
        let mut i = at + 1;
        if is(&ts, i, "TEMP") || is(&ts, i, "TEMPORARY") {
            i += 1;
        }
        if !is(&ts, i, "TABLE") {
            at += 1;
            continue;
        }
        i += 1;
        if is(&ts, i, "IF") && is(&ts, i + 1, "NOT") && is(&ts, i + 2, "EXISTS") {
            i += 3;
        }
        let Some(table_token) = ts.get(i).filter(|t| t.word) else {
            at += 1;
            continue;
        };
        let mut table = table_token.text.clone();
        i += 1;
        if ts.get(i).is_some_and(|t| t.text == ".") {
            if let Some(t) = ts.get(i + 1).filter(|t| t.word) {
                table = t.text.clone();
                i += 2;
            }
        }
        if !ts.get(i).is_some_and(|t| t.text == "(") {
            at = i;
            continue;
        }
        i += 1;
        let mut depth = 1;
        let mut segment = i;
        while i < ts.len() && depth > 0 {
            match ts[i].text.as_str() {
                "(" => depth += 1,
                ")" => depth -= 1,
                _ => {}
            }
            if depth == 0 || depth == 1 && ts[i].text == "," {
                if i > segment + 1
                    && ts[segment].word
                    && ts[segment + 1].word
                    && !matches!(
                        ts[segment].text.to_ascii_uppercase().as_str(),
                        "PRIMARY"
                            | "FOREIGN"
                            | "UNIQUE"
                            | "CONSTRAINT"
                            | "CHECK"
                            | "KEY"
                            | "INDEX"
                            | "EXCLUDE"
                    )
                {
                    let name = &ts[segment];
                    out.push(Column {
                        table: table.clone(),
                        name: name.text.clone(),
                        kind: format!("SQL {}", ts[segment + 1].text),
                        offset: name.at,
                    });
                }
                segment = i + 1;
            }
            i += 1;
        }
        // Incomplete definitions cannot establish schema facts.
        if depth != 0 {
            out.retain(|c| c.table != table);
        }
        at = i;
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dialects_constraints_defaults_and_qualified_names() {
        let sql = "-- CREATE TABLE fake (x text);\nCREATE TABLE `posts` (`id` bigint PRIMARY KEY, `title` varchar(255) DEFAULT 'a,b)', CONSTRAINT `posts_unique` UNIQUE (`title`)); CREATE TABLE public.\"users\" (\"email\" TEXT, [active] INTEGER);";
        let columns = parse(sql);
        assert_eq!(
            columns
                .iter()
                .map(|c| (c.table.as_str(), c.name.as_str()))
                .collect::<Vec<_>>(),
            [
                ("posts", "id"),
                ("posts", "title"),
                ("users", "email"),
                ("users", "active")
            ]
        );
        for c in columns {
            assert_eq!(&sql[c.offset..c.offset + c.name.len()], c.name);
        }
    }
    #[test]
    fn strings_and_incomplete_statements_are_not_definitions() {
        assert!(
            parse("SELECT 'CREATE TABLE fake (x text)'; CREATE TABLE broken (x text").is_empty()
        );
    }
}
