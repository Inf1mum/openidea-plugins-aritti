//! Readonly current-document locations. No index anchors or rename authority.
use crate::{checks, generate, live};
use openidea_plugin::{Location, Range, providers::Document, text::position};
use spring_text::xml;

pub fn definition(doc: &Document, caret: usize) -> Vec<Location> {
    let Some(log) = live::parse(&doc.path, &doc.text) else { return Vec::new(); };
    if log.format == crate::changelog::Format::Sql { return sql_definition(doc,caret,&log); }
    let Ok(parent) = generate::parent_log(&doc.path, &doc.text) else { return Vec::new(); };
    if !doc.path.ends_with(".xml") { return array_definition(doc, caret, &log, &parent); }
    let elements = xml::elements(&doc.text);
    let mut target = None;
    if let Some((_, _, name)) = checks::placeholders(&doc.text, log.format).into_iter().find(|(a,b,_)| *a <= caret && caret <= *b) {
        // A source declaration location only: conditions and precedence are not evaluated.
        let declarations: Vec<_> = parent.property_owners.iter().filter(|(_, n)| n == name).collect();
        if declarations.len() == 1 {
            target = elements.iter().find(|e| e.start == declarations[0].0 && e.local() == "property")
                .and_then(|e| e.attrs.iter().find(|a| a.name == "name"));
        }
    } else if parent.includes.is_empty() && !parent.opaque && parent.complete_ids {
        for (owner, id, author) in &parent.rollback_refs {
            let Some(reference) = elements.iter().find(|e| e.start == *owner && e.local() == "rollback") else { continue; };
            let Some(container) = reference.parent.and_then(|i| elements.get(i)) else { continue; };
            if container.local() != "changeSet" || container.parent != Some(0)
                || !reference.attrs.iter().any(|a| a.name == "changeSetId" && a.holds(caret)) { continue; }
            let declarations: Vec<_> = parent.indexed_sets.iter().filter(|(_, i, a)| i == id && a == author).collect();
            if declarations.len() == 1 {
                target = elements.iter().find(|e| e.start == declarations[0].0)
                    .and_then(|e| e.attrs.iter().find(|a| a.name == "id"));
            }
        }
    }
    target.filter(|a| a.value_at < a.value_end).map(|a| vec![Location {
        path: doc.path.clone(), range: Range::new(position(&doc.text,a.value_at), position(&doc.text,a.value_end)),
    }]).unwrap_or_default()
}

fn sql_definition(doc: &Document, caret: usize, log: &crate::changelog::Changelog) -> Vec<Location> {
    if !log.header { return Vec::new(); }
    let Some((_,_,name))=checks::placeholders(&doc.text,log.format).into_iter().find(|(a,b,_)| *a<=caret && caret<=*b) else {return Vec::new();};
    if log.properties.iter().filter(|n| n.as_str()==name).count()!=1 {return Vec::new();}
    let mut targets=Vec::new(); let mut offset=0;
    for line in doc.text.split_inclusive('\n') {
        let start=offset; offset+=line.len();
        let Some(body)=line.trim_start().strip_prefix("--").map(str::trim_start) else {continue;};
        let Some(args)=body.get(..8).filter(|k| k.eq_ignore_ascii_case("property")).map(|_| &body[8..]) else {continue;};
        if !args.starts_with(char::is_whitespace) {continue;}
        for token in args.split_whitespace() {
            if token.strip_prefix("name:")==Some(name) {
                let at=start+line.len()-args.len()+args.find(token).unwrap()+5;
                targets.push(Location {path:doc.path.clone(),range:Range::new(position(&doc.text,at),position(&doc.text,at+name.len()))});
            }
        }
    }
    if targets.len()==1 {targets} else {Vec::new()}
}

fn array_definition(doc: &Document, caret: usize, log: &crate::changelog::Changelog, parent: &generate::ParentLog) -> Vec<Location> {
    use crate::changelog::Format;
    let keys = match log.format {
        Format::Yaml => spring_text::config::yaml(&doc.text),
        Format::Json => crate::json::keys(&doc.text),
        _ => return Vec::new(),
    };
    // The semantic parser proves structure; the scanner supplies a unique exact scalar span.
    // Flattened path collisions and unsupported flow/block scalars fail closed.
    let scalar = |path: &str, leaf: &str| {
        let matches: Vec<_> = keys.iter().filter(|k| k.key == path).collect();
        if matches.len() != 1 { return None; }
        let k = matches[0];
        let raw = doc.text.get(k.at..k.end)?;
        if raw.trim_matches(['\'', '"']) != leaf || k.value_at >= k.value_end { return None; }
        Some(k)
    };
    let mut target = None;
    if let Some((a,b,name)) = checks::placeholders(&doc.text,log.format).into_iter().find(|(a,b,_)| *a <= caret && caret <= *b) {
        if !keys.iter().any(|k| k.value_at <= a && b <= k.value_end) { return Vec::new(); }
        let declarations: Vec<_> = parent.property_owners.iter().filter(|(_,n)| n==name).collect();
        if declarations.len()==1 {
            target=scalar(&format!("databaseChangeLog[{}].property.name",declarations[0].0),"name");
        }
    } else if parent.includes.is_empty() && !parent.opaque && parent.complete_ids {
        for (index,id,author) in &parent.rollback_refs {
            let Some(reference)=scalar(&format!("databaseChangeLog[{index}].changeSet.rollback.changeSetId"),"changeSetId") else {continue;};
            if !(reference.value_at <= caret && caret <= reference.value_end) {continue;}
            let declarations: Vec<_>=parent.indexed_sets.iter().filter(|(_,i,a)| i==id && a==author).collect();
            if declarations.len()==1 {target=scalar(&format!("databaseChangeLog[{}].changeSet.id",declarations[0].0),"id");}
        }
    }
    target.map(|k|vec![Location {path:doc.path.clone(),range:Range::new(position(&doc.text,k.value_at),position(&doc.text,k.value_end))}]).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(text: &str, needle: &str) -> Vec<Location> {
        let doc = Document { path: "/p/db.xml".into(), language: "xml".into(), text: text.into(), version: 1 };
        definition(&doc, text.rfind(needle).unwrap())
    }
    #[test]
    fn formatted_sql_local_property_is_readonly_and_duplicate_directives_are_quiet() {
        let text="--liquibase formatted sql\r\n--property name:schema value:private\r\n--changeset a:one\r\nselect '${schema}';\r\n";
        let doc=Document {path:"/p/a.sql".into(),language:"sql".into(),text:text.into(),version:1};
        let found=definition(&doc,text.find("schema}").unwrap());
        assert_eq!(found.len(),1);
        assert_eq!(found[0].range.start,position(text,text.find("name:schema").unwrap()+5));
        let doubled=Document {text:text.replace("--property name:schema value:private","--property name:schema value:private\r\n--property name:schema value:other"),..doc};
        assert!(definition(&doubled,doubled.text.find("schema}").unwrap()).is_empty());
    }
    #[test]
    fn arrays_use_semantic_owners_and_exact_scalar_spans() {
        for (path,text) in [
            ("/p/a.yaml", "databaseChangeLog:\n  - property:\n      name: schema\n      value: public\n  - changeSet:\n      id: old\n      author: a\n  - changeSet:\n      id: new\n      author: a\n      comment: ${schema}\n      rollback:\n        changeSetId: old\n        changeSetAuthor: a\n"),
            ("/p/a.json", r#"{"databaseChangeLog":[{"property":{"name":"schema","value":"public"}},{"changeSet":{"id":"old","author":"a"}},{"changeSet":{"id":"new","author":"a","comment":"${schema}","rollback":{"changeSetId":"old","changeSetAuthor":"a"}}}]}"#),
        ] {
            let doc=Document {path:path.into(),language:"".into(),text:text.into(),version:1};
            assert_eq!(definition(&doc,text.find("schema}").unwrap()).len(),1,"{path}");
            assert_eq!(definition(&doc,text.rfind("old").unwrap()).len(),1,"{path}");
        }
    }
    #[test]
    fn readonly_literal_property_and_same_file_rollback() {
        let text = "<databaseChangeLog xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\"><property name=\"schema\" value=\"秘密\"/><changeSet id=\"old\" author=\"a\"/><changeSet id=\"new\" author=\"a\"><sql>${schema}</sql><rollback changeSetId=\"old\" changeSetAuthor=\"a\"/></changeSet></databaseChangeLog>";
        assert_eq!(run(text,"schema}").len(),1);
        let found=run(text,"old\""); assert_eq!(found.len(),1);
        assert_eq!(found[0].range.start,position(text,text.find("id=\"old").unwrap()+4));
        assert!(run(&text.replace("<changeSet id=\"old\" author=\"a\"/>","<include file=\"x.xml\"/><changeSet id=\"old\" author=\"a\"/>"),"old\"").is_empty());
        assert!(run(&text.replace("changeSetAuthor=\"a\"","changeSetPath=\"x.xml\" changeSetAuthor=\"a\""),"old\"").is_empty());
        assert!(run(&text.replace("<property name=\"schema\" value=\"秘密\"/>","<property name=\"schema\"/><property name=\"schema\"/>"),"schema}").is_empty());
        assert!(run(&text.replace("http://www.liquibase.org/xml/ns/dbchangelog","urn:foreign"),"schema}").is_empty());
    }
}
