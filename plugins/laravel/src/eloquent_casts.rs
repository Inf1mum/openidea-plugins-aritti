//! Literal cast declarations and enum members. No runtime cast execution.
use super::{Entry, Names, Source, entry};
use crate::php;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(super) struct Cast {
    pub label: String,
    pub target: Option<String>,
    pub entry: Entry,
}
pub(super) type Casts = BTreeMap<String, Cast>;
pub(super) fn array(source: &Source, names: &Names, open: usize, path: &str) -> Option<Casts> {
    let token = source.tokens.get(open)?;
    if token.value != "[" {
        return None;
    }
    let mut result = BTreeMap::new();
    for item in php::items(source.text, token.start) {
        let (key, offset) = php::literal(source.text, item.key?)?;
        let declaration = Entry {
            path: path.into(),
            at: openidea_plugin::text::position(source.text, offset),
            width: key.encode_utf16().count() as u32,
            detail: None,
        };
        let value = if let Some((label, _)) = php::literal(source.text, item.value.clone()) {
            Cast {
                label: label.into(),
                target: label
                    .split(':')
                    .next()
                    .filter(|class| {
                        class.contains('\\')
                            && class.trim_start_matches('\\').split('\\').all(|part| {
                                !part.is_empty()
                                    && part
                                        .bytes()
                                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                            })
                    })
                    .map(|class| class.trim_start_matches('\\').to_owned()),
                entry: declaration,
            }
        } else if let Some(class) = {
            let tokens: Vec<_> = source
                .tokens
                .iter()
                .filter(|t| t.start >= item.value.start && t.end <= item.value.end)
                .collect();
            match tokens.as_slice() {
                [class, op, member]
                    if class.kind == super::Kind::Name
                        && op.value == "::"
                        && member.value.eq_ignore_ascii_case("class") =>
                {
                    Some((class.value, String::new()))
                }
                [class, op, member, dot, parameter]
                    if class.kind == super::Kind::Name
                        && op.value == "::"
                        && member.value.eq_ignore_ascii_case("class")
                        && dot.value == "."
                        && parameter.kind == super::Kind::String =>
                {
                    php::literal(source.text, parameter.start..parameter.end)
                        .filter(|(suffix, _)| suffix.starts_with(':'))
                        .map(|(suffix, _)| (class.value, suffix.to_owned()))
                }
                _ => None,
            }
        } {
            let target = names.resolve(class.0);
            Cast {
                label: format!("{target}{}", class.1),
                target: Some(target),
                entry: declaration,
            }
        } else {
            return None;
        };
        result.insert(key.into(), value);
    }
    Some(result)
}
#[derive(Clone, Debug)]
pub(crate) struct EnumInfo {
    pub(super) full: String,
    pub(super) path: String,
    pub(super) ambiguous: bool,
    pub(super) declaration: Entry,
    pub(super) value_type: Option<String>,
    pub(super) methods: BTreeMap<String, Entry>,
    pub(super) cases:Option<Vec<(String,Option<serde_json::Value>,Entry)>>,
}
pub(crate) type Enums = BTreeMap<String, EnumInfo>;
pub(crate) fn cache(enums: &mut Enums, text: &str, path: &str) {
    let Some(source) = Source::new(text) else {
        return;
    };
    let Some(names) = Names::parse(&source) else {
        return;
    };
    let Some((full, start, body, _)) = &names.class else {
        return;
    };
    if !source.is(*start, "enum") {
        return;
    }
    let value_type = (*start + 2..*body)
        .find(|&i| source.is(i, ":"))
        .and_then(|i| source.tokens.get(i + 1))
        .filter(|t| ["int", "string"].contains(&t.value))
        .map(|t| t.value.to_owned());
    let mut methods = BTreeMap::new();
    for function in source.functions() {
        if source.paths[function.start] != vec![*body] {
            continue;
        }
        let Some(name) = function.name else {
            continue;
        };
        let before = (0..function.start)
            .rev()
            .find(|&i| source.is(i, ";") || source.is(i, "}") || source.is(i, "{"))
            .map_or(0, |i| i + 1);
        if (before..function.start)
            .any(|i| source.is(i, "private") || source.is(i, "protected") || source.is(i, "static"))
        {
            continue;
        }
        methods.insert(source.tokens[name].value.into(), entry(&source, path, name));
    }
    let mut cases=Vec::new();let mut case_names=std::collections::BTreeSet::new();let mut cases_complete=true;
    for(at,_)in source.tokens.iter().enumerate().filter(|(at,token)|token.value=="case"&&source.paths[*at]==vec![*body]){
        let Some(name)=source.tokens.get(at+1).filter(|token|token.kind==super::Kind::Name)else{cases_complete=false;break};
        if cases.len()>=256||!case_names.insert(name.value){cases_complete=false;break;}
        let Some(end)=(at+2..source.tokens.len()).find(|index|source.is(*index,";")&&source.paths[*index]==vec![*body])else{cases_complete=false;break};
        let backing=if source.is(at+2,"="){
            match value_type.as_deref(){
                Some("string") if end==at+4=>source.tokens.get(at+3).filter(|token|token.kind==super::Kind::String).and_then(|token|php::literal(text,token.start..token.end).and_then(|(value,_)|super::metadata_inference::decode_literal(value,token.value.as_bytes()[0]))).map(serde_json::Value::String),
                Some("int")=>{let from=source.tokens[at+2].end;let to=source.tokens[end].start;super::integer_literal(php::mask_comments(&text[from..to]).trim()).map(serde_json::Value::from)},
                _=>None,
            }
        }else{None};
        cases.push((name.value.into(),backing,entry(&source,path,at+1)));
    }
    let info = EnumInfo {
        full: full.clone(),
        path: path.into(),
        ambiguous: false,
        declaration: entry(&source, path, start + 1),
        value_type,
        methods,
        cases:cases_complete.then_some(cases),
    };
    enums
        .entry(full.to_ascii_lowercase())
        .and_modify(|old| {
            if old.path != path {
                old.ambiguous = true;
            }
        })
        .or_insert(info);
}
