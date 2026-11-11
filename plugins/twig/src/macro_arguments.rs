//! Named-argument completion for a physically resolved Twig macro declaration.
use std::{collections::BTreeSet, ops::Range};
use openidea_plugin::{providers::{CompletionItem, CompletionKind}, syntax::Highlight, text::position};

/// Known installed older Twig versions cannot execute named macro arguments.
/// Missing/unsupported version evidence keeps latest-language assistance available.
pub fn supported(project: Option<&openidea_plugin::Project>) -> bool {
    let Some(project)=project else {return true;};
    let Ok(stat)=project.stat("composer.lock") else {return true;};
    if stat.size>2<<20 {return true;}
    let Ok(text)=project.read_file("composer.lock") else {return true;};
    if text.len()>2<<20 {return true;}
    let Ok(lock)=serde_json::from_str::<serde_json::Value>(&text) else {return true;};
    let versions:Vec<_>=["packages","packages-dev"].into_iter()
        .filter_map(|key|lock.get(key).and_then(|v|v.as_array()))
        .flatten().filter(|v|v.get("name").and_then(|v|v.as_str())==Some("twig/twig"))
        .filter_map(|v|v.get("version").and_then(|v|v.as_str())).collect();
    if versions.len()!=1 {return true;}
    let version=versions[0].trim_start_matches('v');
    let mut pieces=version.split('.');
    let (Some(major),Some(minor))=(pieces.next().and_then(|v|v.parse::<u32>().ok()),pieces.next().and_then(|v|v.parse::<u32>().ok())) else {return true;};
    major>3 || major==3 && minor>=15
}

pub struct Slot {
    pub receiver: Option<String>,
    pub name: String,
    pub call: usize,
    replace: Range<usize>,
    used: BTreeSet<String>,
    positional: usize,
}

pub(crate) fn identifier(value: &str) -> bool {
    let mut bytes=value.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic() || b==b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b==b'_')
}

/// Split only commas at the current expression depth; quoted defaults and nested
/// arrays/calls do not introduce parameters. Incomplete nested syntax is unknown.
pub(crate) fn parts(text: &str) -> Option<Vec<&str>> {
    let mut result=Vec::new();
    let (mut start,mut quote,mut escaped)=(0,None,false);
    let mut stack=Vec::new();
    for (i,c) in text.char_indices() {
        if let Some(q)=quote {
            if escaped {escaped=false;} else if c=='\\' {escaped=true;} else if c==q {quote=None;}
            continue;
        }
        match c {
            '\''|'"'=>quote=Some(c),
            '('|'['|'{'=>stack.push(c),
            ')'|']'|'}'=>if stack.pop()!=Some(match c {')'=>'(',']'=>'[',_=>'{'}) {return None;},
            ',' if stack.is_empty()=>{result.push(&text[start..i]);start=i+1;},
            _=>{}
        }
        if stack.len()>64{return None;}
    }
    if quote.is_some() || !stack.is_empty(){return None;}
    result.push(&text[start..]);
    Some(result)
}

pub struct Call {pub receiver:Option<String>,pub name:String,pub call:usize,pub open:usize}

pub fn call_at(text: &str, offset: usize,allow_string:bool) -> Option<Call> {
    if text.len()>128<<10 || !text.is_char_boundary(offset) || !crate::navigate::code_at(text,offset,allow_string) {return None;}
    let mut stack=Vec::new();
    let parsed=crate::lex(text);
    let mut boundary=0usize;
    for token in parsed.tokens {
        if token.range.start>=offset {break;}
        if token.kind!=Highlight::Punctuation {continue;}
        let raw=&text[token.range.clone()];
        if parsed.boundaries.get(boundary).is_some_and(|(range,_)|*range==token.range) {boundary+=1;stack.clear();continue;}
        for (i,c) in raw.char_indices() {
            if token.range.start+i>=offset {break;}
            match c {
                '('|'['|'{'=>stack.push((c,token.range.start+i)),
                ')'|']'|'}'=>{if stack.pop().map(|p|p.0)!=Some(match c {')'=>'(',']'=>'[',_=>'{'}) {return None;}},
                _=>{}
            }
        }
        if stack.len()>64{return None;}
    }
    let (_,open)=*stack.iter().rev().find(|(kind,_)|*kind=='(')?;
    let call_end=text[..open].trim_end().len();
    let (receiver,name,range)=crate::navigate::dotted_at(text,call_end)?;
    if !identifier(&name) || range.end!=call_end {return None;}
    if receiver.is_none() && text[..range.start].ends_with('.') {return None;}
    if let Some(receiver)=&receiver {
        let start=range.start.checked_sub(receiver.len()+1)?;
        if text[..start].chars().last().is_some_and(|c|c=='.'||c=='_'||c.is_alphanumeric()){return None;}
    }
    let syntax=crate::delimiters::Syntax::current();
    if let Some(start)=text[..open].rfind(&syntax.values[0]) {
        let body=&text[start+syntax.values[0].len()..open];
        if !body.contains(&syntax.values[1]) && body.trim_start_matches(['-','~']).trim_start().starts_with("macro "){return None;}
    }
    Some(Call{receiver,name,call:range.start,open})
}

pub fn slot(text:&str,offset:usize)->Option<Slot> {
    let Call{receiver,name,call,open}=call_at(text,offset,false)?;
    let arguments=parts(&text[open+1..offset])?;
    let current=arguments.last()?.trim_start();
    if !current.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_') {return None;}
    let start=offset-current.len();
    let end=offset+text[offset..].bytes().take_while(|b|b.is_ascii_alphanumeric()||*b==b'_').count();
    let (mut used,mut positional)=(BTreeSet::new(),0);
    for argument in &arguments[..arguments.len()-1] {
        let argument=argument.trim();
        if argument.is_empty(){return None;}
        if let Some((key,_))=argument.split_once([':', '=']).filter(|(key,_)|identifier(key.trim())) {
            if !used.insert(key.trim().to_owned()){return None;}
        } else {
            if !used.is_empty(){return None;}
            positional+=1;
        }
    }
    Some(Slot{receiver,name,call,replace:start..end,used,positional})
}

pub fn items(text: &str, slot: &Slot, arguments: &str) -> Vec<CompletionItem> {
    let Some(parts)=parts(arguments) else {return Vec::new();};
    let mut names=Vec::new();
    for part in parts {
        let name=part.split_once('=').map_or(part,|p|p.0).trim();
        if !identifier(name) || names.contains(&name){return Vec::new();}
        names.push(name);
    }
    if slot.positional>names.len() || slot.used.iter().any(|name|!names.contains(&name.as_str())) {return Vec::new();}
    let tail=text[slot.replace.end..].trim_start();
    let separator_exists=tail.starts_with(':')||tail.starts_with('=');
    names.into_iter().skip(slot.positional).filter(|name|!slot.used.contains(*name)).map(|name|{
        let mut item=CompletionItem::new(name,CompletionKind::Variable);
        item.insert_text=if separator_exists{name.to_owned()}else{format!("{name}: ")};
        item.detail=Some(format!("{} macro argument",slot.name));
        item.replace=Some(openidea_plugin::Range::new(position(text,slot.replace.start),position(text,slot.replace.end)));
        item
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn labels(marked:&str,parameters:&str)->Vec<String>{
        let offset=marked.find('^').unwrap();let text=marked.replacen('^',"",1);
        slot(&text,offset).map(|s|items(&text,&s,parameters).into_iter().map(|i|i.label).collect()).unwrap_or_default()
    }
    #[test]
    fn named_slots_preserve_nested_values_and_prior_positional_parameters(){
        assert_eq!(labels("{{ forms.input('x', options: {a: [1,2]}, va^) }}","name, value = '', options = {}"),["value"]);
        assert_eq!(labels("{{ _self.input(value = 'a,b', na^) }}","name, value = '', options = {}"),["name","options"]);
        assert!(labels("{{ _self.input(value: 'x', 'positional', ^) }}","name, value").is_empty());
        assert!(labels("{{ _self.input(value: 'x', value: 'y', ^) }}","name, value").is_empty());
    }
    #[test]
    fn expressions_strings_comments_and_invalid_parameter_models_are_quiet(){
        for text in ["plain foo(^)","{# {{ foo(^) }} #}","{{ foo('^') }}","{{ foo(value: other^) }}","{{ foo([^) }}"]{
            assert!(labels(text,"name").is_empty(),"{text}");
        }
        assert!(labels("{{ foo(^) }}","name, name").is_empty());
    }
    #[test]
    fn existing_named_separator_and_utf16_ranges_are_preserved(){
        let text="😀 {{ forms.input(va: '') }}";let offset=text.find("va:").unwrap()+1;
        let s=slot(text,offset).unwrap();let result=items(text,&s,"value = ''");
        assert_eq!(result[0].insert_text,"value");
        assert_eq!(result[0].replace,Some(openidea_plugin::Range::new(position(text,offset-1),position(text,offset+1))));
    }
}

#[cfg(test)]
mod provider_tests {
    use super::*;
    use openidea_plugin::{mock::MockHost,providers::{Document,Providers}};

    fn complete(marked:&str, external:&str, symfony:bool)->Vec<CompletionItem>{
        let host=MockHost::install();host.set_project_root(Some("/project"));
        host.add_file("templates/forms.twig",external);
        host.add_file("composer.json",if symfony {r#"{"require":{"symfony/framework-bundle":"^7"}}"#}else{"{}"});
        let cursor=marked.find('^').unwrap();let text=marked.replacen('^',"",1);let at=position(&text,cursor);
        let doc=Document{path:"/project/templates/current.twig".into(),language:"twig".into(),version:1,text};
        crate::Twig::default().completion(&doc,at,None)
    }
    #[test]
    fn provider_resolves_literal_alias_and_current_unsaved_self_arguments(){
        let declarations="{% macro input(name, value = '') %}{% endmacro %}";
        let items=complete("{% import 'forms.twig' as forms %}😀 {{ forms.input('x', va^) }}",declarations,false);
        let value=items.iter().find(|i|i.label=="value").unwrap();
        assert_eq!(value.insert_text,"value: ");assert!(!items.iter().any(|i|i.label=="name"));
        let marked="{% macro input(name, value = '') %}{% endmacro %}{{ _self.input(va^) }}";
        assert!(complete(marked,"",true).iter().any(|i|i.label=="value"));
        let names=complete("{% from 'forms.twig' import input as field %}{{ field(va^) }}",declarations,false);
        assert!(names.iter().any(|i|i.label=="value"));
    }
    #[test]
    fn duplicate_external_owners_and_dynamic_receivers_do_not_offer_arguments(){
        let duplicate="{% macro input(name) %}{% endmacro %}{% macro input(value) %}{% endmacro %}";
        let marked="{% import 'forms.twig' as forms %}{{ forms.input(va^) }}";
        assert!(!complete(marked,duplicate,false).iter().any(|i|i.detail.as_deref()==Some("input macro argument")));
        for expression in ["object.forms.input(va^)","get().input(va^)"] {
            let marked=format!("{{% from 'forms.twig' import input %}}{{{{ {expression} }}}}");
            assert!(!complete(&marked,"{% macro input(value) %}{% endmacro %}",false).iter().any(|i|i.detail.as_deref()==Some("input macro argument")));
        }
    }
}

#[cfg(test)]
mod version_tests {
    use super::*;
    use openidea_plugin::{mock::MockHost,host};
    #[test]
    fn known_old_installed_versions_withhold_latest_only_macro_arguments(){
        let h=MockHost::install();h.set_project_root(Some("/project"));
        let project=host::project_of("/project/a.twig").unwrap();
        for (version,expected) in [("v2.16.1",false),("3.14.9",false),("3.15.0",true),("3.30.0",true),("dev-main",true)] {
            h.add_file("composer.lock",&format!(r#"{{"packages":[{{"name":"twig/twig","version":"{version}"}}]}}"#));
            assert_eq!(supported(Some(&project)),expected,"{version}");
        }
    }
}
