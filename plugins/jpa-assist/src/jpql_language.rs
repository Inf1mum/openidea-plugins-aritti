//! Independent JPQL lexical syntax. This is not a persistence-unit/type resolver.
use crate::{JpaAssist, attribute_info, checks, model};
use openidea_plugin::{
    Range,
    injections::{Document, Injection, Injections},
    syntax::{Highlight, SyntaxProvider, Token},
    text::position,
};
const MAX_TEXT: usize = 128 * 1024;
const KEYWORDS: &[&str] = &[
    "select",
    "distinct",
    "from",
    "as",
    "where",
    "join",
    "left",
    "outer",
    "inner",
    "fetch",
    "on",
    "group",
    "by",
    "having",
    "order",
    "asc",
    "desc",
    "update",
    "set",
    "delete",
    "and",
    "or",
    "not",
    "is",
    "null",
    "empty",
    "true",
    "false",
    "between",
    "like",
    "in",
    "exists",
    "all",
    "any",
    "some",
    "member",
    "of",
    "escape",
    "case",
    "when",
    "then",
    "else",
    "end",
    "new",
    "type",
    "treat",
    "key",
    "value",
    "entry",
    "index",
    "object",
    "current_date",
    "current_time",
    "current_timestamp",
];
fn word(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '$')
}
fn tokens(text: &str) -> Vec<Token> {
    if text.len() > MAX_TEXT {
        return vec![];
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() && out.len() < 4096 {
        let ch = text[i..].chars().next().unwrap();
        if ch.is_whitespace() {
            i += ch.len_utf8();
            continue;
        }
        let start = i;
        let kind = if ch == '\'' {
            i += 1;
            while i < text.len() {
                if text.as_bytes()[i] == b'\'' {
                    i += 1;
                    if text.as_bytes().get(i) == Some(&b'\'') {
                        i += 1;
                        continue;
                    }
                    break;
                }
                i += text[i..].chars().next().unwrap().len_utf8();
            }
            Highlight::String
        } else if text[i..].starts_with("/*") {
            i = text[i + 2..]
                .find("*/")
                .map_or(text.len(), |n| i + 2 + n + 2);
            Highlight::Comment
        } else if text[i..].starts_with("--") {
            i = text[i..].find('\n').map_or(text.len(), |n| i + n);
            Highlight::Comment
        } else if matches!(ch, ':' | '?') && text[i + 1..].chars().next().is_some_and(word) {
            i += 1;
            while i < text.len() && text[i..].chars().next().is_some_and(word) {
                i += text[i..].chars().next().unwrap().len_utf8();
            }
            Highlight::Variable
        } else if ch.is_ascii_digit() {
            i += 1;
            while i < text.len()
                && (text.as_bytes()[i].is_ascii_digit() || text.as_bytes()[i] == b'.')
            {
                i += 1;
            }
            Highlight::Number
        } else if ch.is_alphabetic() || ch == '_' {
            i += ch.len_utf8();
            while i < text.len() && text[i..].chars().next().is_some_and(word) {
                i += text[i..].chars().next().unwrap().len_utf8();
            }
            if KEYWORDS.contains(&text[start..i].to_ascii_lowercase().as_str()) {
                Highlight::Keyword
            } else if text[i..].trim_start().starts_with('(') {
                Highlight::FunctionCall
            } else {
                Highlight::Property
            }
        } else {
            i += ch.len_utf8();
            if "().,[]".contains(ch) {
                Highlight::Punctuation
            } else {
                Highlight::Operator
            }
        };
        out.push(Token {
            range: start..i,
            kind,
        });
    }
    out
}
impl SyntaxProvider for JpaAssist {
    fn highlights(&mut self, language: &str, text: &str) -> Vec<Token> {
        if language == "jpql" {
            tokens(text)
        } else {
            vec![]
        }
    }
}
pub(crate) fn completion(
    doc: &Document,
    at: openidea_plugin::Position,
) -> Vec<openidea_plugin::providers::CompletionItem> {
    use openidea_plugin::providers::{CompletionItem, CompletionKind};
    if doc.language != "jpql" || doc.text.len() > MAX_TEXT {
        return vec![];
    }
    let Some(cursor) = openidea_plugin::text::offset(&doc.text, at) else {
        return vec![];
    };
    if tokens(&doc.text).iter().any(|t| {
        matches!(t.kind, Highlight::String | Highlight::Comment)
            && t.range.start < cursor
            && cursor < t.range.end
    }) {
        return vec![];
    }
    let mut from = cursor;
    let mut to = cursor;
    while from > 0 {
        let ch = doc.text[..from].chars().next_back().unwrap();
        if !word(ch) {
            break;
        }
        from -= ch.len_utf8();
    }
    while to < doc.text.len() {
        let ch = doc.text[to..].chars().next().unwrap();
        if !word(ch) {
            break;
        }
        to += ch.len_utf8();
    }
    if doc.text[..from].ends_with([':', '?', '.']) {
        return vec![];
    }
    let typed = doc.text[from..cursor].to_ascii_lowercase();
    KEYWORDS
        .iter()
        .filter(|name| name.starts_with(&typed))
        .map(|name| CompletionItem {
            replace: Some(Range::new(
                position(&doc.text, from),
                position(&doc.text, to),
            )),
            detail: Some("JPQL keyword; persistence-unit types unresolved".into()),
            ..CompletionItem::new(name, CompletionKind::Keyword)
        })
        .collect()
}
// Source-proven direct method-parameter receivers only. Flat bodies avoid
// nested scope/shadowing; all uses of the parameter must be member accesses.
fn api_queries(doc:&Document)->Vec<Injection>{
    if doc.language!="java"||doc.text.len()>MAX_TEXT{return vec![];}
    use spring_text::java;
    let file=java::parse(&doc.text);
    if file.types.len()>128||file.imports.iter().any(|i|i.path.ends_with(".*"))||file.types.iter().any(|t|matches!(t.name.as_str(),"jakarta"|"javax")){return vec![];}
    let all=java::tokens(&doc.text);
    if all.iter().enumerate().any(|(i,t)|matches!(t.s,"jakarta"|"javax")&&all.get(i+1).is_none_or(|next|next.s!=".")){return vec![];}
    let mut out=Vec::new();
    for ty in &file.types {for member in &ty.members {
        if member.kind!=java::MemberKind::Method {continue;}
        let Some((from,to))=member.body else{continue;};
        let Some(body)=doc.text.get(from..to) else{continue;};
        let tokens=java::tokens(body);
        let interior=tokens.iter().enumerate().filter(|(_,t)|matches!(t.s,"{"|"}")).collect::<Vec<_>>();
        if interior.len()!=2||interior[0].0!=0||interior[1].0+1!=tokens.len(){continue;}
        for param in &member.params {
            let qualified=param.ty.text.split_whitespace().collect::<String>();
            if !matches!(qualified.as_str(),"jakarta.persistence.EntityManager"|"javax.persistence.EntityManager")||param.ty.array||!param.ty.args.is_empty(){continue;}
            let usages=tokens.iter().enumerate().filter(|(_,t)|t.s==param.name).map(|(i,_)|i).collect::<Vec<_>>();
            if usages.iter().any(|i|tokens.get(i+1).is_none_or(|t|t.s!=".")||i.checked_sub(1).and_then(|i|tokens.get(i)).is_some_and(|t|matches!(t.s,"."|":"|"::"))){continue;}
            for i in usages {
                let Some(method)=tokens.get(i+2).filter(|t|matches!(t.s,"createQuery"|"createNativeQuery")) else{continue;};
                if tokens.get(i+3).is_none_or(|t|t.s!="("){continue;}
                let Some(literal)=tokens.get(i+4) else{continue;};
                if tokens.get(i+5).is_none_or(|t|!matches!(t.s,","|")")){continue;}
                let raw=literal.s;
                if raw.len()<3||!raw.starts_with('"')||!raw.ends_with('"')||raw.starts_with("\"\"\"")||raw.contains(['\\','\r','\n'])||raw.contains("#{")||raw.contains("${"){continue;}
                let at=from+literal.at+1;
                out.push(Injection{range:Range::new(position(&doc.text,at),position(&doc.text,at+raw.len()-2)),language:if method.s=="createNativeQuery"{"sql"}else{"jpql"}.into()});
                if out.len()>=128{return out;}
            }
        }
    }}
    out
}
impl Injections for JpaAssist {
    fn injections(&mut self, doc: &Document) -> Vec<Injection> {
        if doc.text.len() > MAX_TEXT {
            return vec![];
        }
        let mut out = api_queries(doc);
        let Some(class) = attribute_info::snapshot(doc) else {
            return out;
        };
        let masked = model::mask(&doc.text);
        let Some(class_at) = checks::owned_class_start(&doc.text, &masked, &class) else {
            return out;
        };
        let annotations = model::annotations(&masked);
        if annotations.len() > 256 {
            return out;
        }
        for annotation in annotations
            .iter()
            .filter(|a| a.start < class_at && matches!(a.name, "NamedQuery" | "NamedNativeQuery"))
        {
            // Only direct class annotations and canonical NamedQueries containers.
            if annotations.iter().any(|outer| {
                outer.start < annotation.start
                    && outer.args.is_some_and(|(from, to)| {
                        from <= annotation.start && annotation.end() <= to
                    })
                    && !matches!(outer.name, "NamedQueries" | "NamedNativeQueries")
            }) {
                continue;
            }
            let Some((from, to)) = annotation.args else {
                continue;
            };
            if model::split_top_level(&masked[from..to])
                .iter()
                .filter(|(_, part)| {
                    part.split_once('=')
                        .is_some_and(|(name, _)| name.trim() == "query")
                })
                .count()
                != 1
            {
                continue;
            }
            let Some(range) = model::value_range(&doc.text, &masked, annotation, "query") else {
                continue;
            };
            let Some((at, body)) = model::plain_literal(&doc.text, range, doc.language == "kotlin")
            else {
                continue;
            };
            if body.is_empty()
                || body.contains(['\\', '\r', '\n'])
                || body.contains("#{")
                || body.contains("${")
            {
                continue;
            }
            out.push(Injection {
                range: Range::new(
                    position(&doc.text, at),
                    position(&doc.text, at + body.len()),
                ),
                language: if annotation.name == "NamedNativeQuery" {
                    "sql"
                } else {
                    "jpql"
                }
                .into(),
            });
            if out.len() >= 128 {
                break;
            }
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, text::offset};
    #[test]
    fn direct_entity_manager_api_literals_have_physical_ranges_without_borrowing_receiver_identity(){
        let _host=MockHost::install();
        let text="package shop; class Queries { void run(jakarta.persistence.EntityManager em) { em.createQuery(\"select p from Person p\", Person.class); em.createNativeQuery(\"select * from people\"); } }";
        let source=doc(text);let ranges=JpaAssist::default().injections(&source);assert_eq!(ranges.len(),2);
        assert_eq!(ranges[0].language,"jpql");assert_eq!(ranges[1].language,"sql");
        let start=offset(text,ranges[0].range.start).unwrap();let end=offset(text,ranges[0].range.end).unwrap();assert_eq!(&text[start..end],"select p from Person p");
        for rejected in [text.replace("class Queries", "class Queries<jakarta>"),text.replace("package shop;", "package shop; import foreign.jakarta;"),text.replace("jakarta.persistence.EntityManager","foreign.EntityManager"),text.replace("em.createQuery","other.em.createQuery").replace("em.createNativeQuery","other.em.createNativeQuery"),text.replace("em.createQuery","em = replacement; em.createQuery"),text.replace("em.createQuery","if (ready) { em.createQuery").replace("Person.class);","Person.class); }"),text.replace("select p from Person p","select \\n p").replace("select * from people","select \\n p")] {
            assert!(JpaAssist::default().injections(&doc(&rejected)).is_empty(),"{rejected}");
        }
    }
    fn doc(text: &str) -> Document {
        Document {
            path: "/project/src/main/java/shop/Person.java".into(),
            text: text.into(),
            language: "java".into(),
            version: 1,
        }
    }
    fn bodies(d: &Document) -> Vec<(String, String)> {
        JpaAssist::default()
            .injections(d)
            .iter()
            .map(|i| {
                (
                    i.language.clone(),
                    d.text[offset(&d.text, i.range.start).unwrap()
                        ..offset(&d.text, i.range.end).unwrap()]
                        .into(),
                )
            })
            .collect()
    }
    #[test]
    fn named_jpql_and_native_queries_are_owned_physical_source_literals() {
        let _host = MockHost::install();
        let d = doc(
            "package shop;\n/* 🦉 */ @jakarta.persistence.Entity @jakarta.persistence.NamedQuery(name=\"Person.all\",query=\"select p from Person p where p.name=:name\") @jakarta.persistence.NamedNativeQuery(name=\"Person.sql\",query=\"select * from person\") class Person { @jakarta.persistence.Id long id; }",
        );
        assert_eq!(
            bodies(&d),
            vec![
                (
                    "jpql".into(),
                    "select p from Person p where p.name=:name".into()
                ),
                ("sql".into(), "select * from person".into())
            ]
        );
        for changed in [
            d.text
                .replace("jakarta.persistence.NamedQuery", "vendor.NamedQuery"),
            d.text.replace(
                r#"query="select p from Person p where p.name=:name""#,
                r#"query="select p " + "from Person p""#,
            ),
            d.text.replace(
                "select p from Person p where p.name=:name",
                r"select p from Person p where p.name='\u0061'",
            ),
        ] {
            let mut changed_doc = d.clone();
            changed_doc.text = changed;
            assert!(
                bodies(&changed_doc)
                    .iter()
                    .all(|(language, _)| language != "jpql")
            );
        }
    }
    #[test]
    fn injected_jpql_completion_preserves_full_java_document_physical_offsets(){
        let _host=MockHost::install();let source=doc("package shop;\n/* 🦉 */ @jakarta.persistence.Entity @jakarta.persistence.NamedQuery(name=\"Person.all\",query=\"sel \") class Person { @jakarta.persistence.Id long id; }");
        let mut plugin=JpaAssist::default();let ranges=plugin.injections(&source);assert_eq!(ranges.len(),1);let mut text=String::new();let mut at=openidea_plugin::Position::new(0,0);
        for ch in source.text.chars(){let inside=ranges.iter().any(|range|range.range.start<=at&&at<range.range.end);if inside||matches!(ch,'\n'|'\r'){text.push(ch);}else{for _ in 0..ch.len_utf16(){text.push(' ');}}if ch=='\n'{at.line+=1;at.character=0;}else{at.character+=ch.len_utf16() as u32;}}
        let virtual_doc=Document{language:"jpql".into(),text,..source.clone()};assert_eq!(virtual_doc.text.encode_utf16().count(),source.text.encode_utf16().count());
        let start=source.text.find("sel ").unwrap();let cursor=position(&source.text,start+3);assert!(ranges[0].range.start<=cursor&&cursor<ranges[0].range.end);
        let items=openidea_plugin::providers::Providers::completion(&mut plugin,&virtual_doc,cursor,None);let item=items.iter().find(|item|item.label=="select").unwrap();assert_eq!(item.replace,Some(Range::new(position(&source.text,start),cursor)));
        let outside=openidea_plugin::Position::new(0,1);assert!(!ranges.iter().any(|range|range.range.start<=outside&&outside<range.range.end));let foreign=openidea_plugin::providers::Providers::completion(&mut plugin,&virtual_doc,outside,None);assert!(foreign.iter().all(|item|!ranges.iter().any(|range|item.replace.is_some_and(|replace|range.range.start<=replace.start&&replace.end<=range.range.end))));
    }

    #[test]
    fn standalone_jpql_syntax_and_completion_do_not_invent_entity_types() {
        let _host = MockHost::install();
        let text =
            "SELECT p FROM Person p WHERE p.name = :name AND p.id=?1 AND p.name='O''Brien 🦉'";
        let mut plugin = JpaAssist::default();
        let tokens = plugin.highlights("jpql", text);
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == Highlight::Variable && &text[t.range.clone()] == ":name")
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == Highlight::String && &text[t.range.clone()] == "'O''Brien 🦉'")
        );
        assert!(
            tokens
                .windows(2)
                .all(|w| w[0].range.end <= w[1].range.start)
        );
        assert!(plugin.highlights("sql", text).is_empty());
        let mut d = doc("sel");
        d.language = "jpql".into();
        assert!(
            completion(&d, position(&d.text, 3))
                .iter()
                .any(|i| i.label == "select")
        );
        d.text = ":sel".into();
        assert!(completion(&d, position(&d.text, 4)).is_empty());
    }
}
