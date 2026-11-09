//! Bounded source-backed YAML mapping locations; no usages/Rename authority.
use crate::spec;
use openidea_plugin::Range;
use std::collections::BTreeMap;
use yaml_rust2::{Yaml,YamlLoader,parser::{Event,Parser}};
const MAX_BYTES:usize=512<<10;
const MAX_NODES:usize=8192;
const MAX_DEPTH:usize=48;

pub(crate) fn model(text:&str)->Option<Yaml> {
    if text.len()>MAX_BYTES { return None; }
    // Reject aliases/anchors/tags before loading, not after possible expansion.
    let mut parser=Parser::new_from_str(text);
    let (mut documents,mut nodes,mut depth,mut complete)=(0usize,0usize,0usize,false);
    for _ in 0..MAX_NODES*4 {
        match parser.next_token().ok()?.0 {
            Event::DocumentStart => { documents+=1; if documents>1 { return None; } },
            Event::Alias(_) => return None,
            Event::Scalar(_,_,anchor,tag) => {
                if anchor!=0 || tag.is_some() { return None; } nodes+=1;
            },
            Event::MappingStart(anchor,tag)|Event::SequenceStart(anchor,tag) => {
                if anchor!=0 || tag.is_some() || depth>=MAX_DEPTH { return None; }
                depth+=1;nodes+=1;
            },
            Event::MappingEnd|Event::SequenceEnd => depth=depth.checked_sub(1)?,
            Event::StreamEnd => {complete=true;break;},
            _=>{},
        }
        if nodes>MAX_NODES {return None;}
    }
    if !complete || documents!=1 || depth!=0 { return None; }
    // yaml-rust2 rejects duplicate mapping keys, including semantic aliases
    // such as quoted/unquoted spellings. This is stricter than a flat scanner.
    let mut docs=YamlLoader::load_from_str(text).ok()?;
    if docs.len()!=1 {return None;}
    let root=docs.pop()?;
    fn unsupported(value:&Yaml)->bool {
        match value {
            Yaml::Hash(map)=>map.iter().any(|(key,value)|matches!(key.as_str(),Some("$id"|"<<"))||unsupported(key)||unsupported(value)),
            Yaml::Array(items)=>items.iter().any(unsupported),
            _=>false,
        }
    }
    (!unsupported(&root)).then_some(root)
}

pub(crate) type Source<'a>=BTreeMap<&'a str,Option<&'a spec::Key>>;
pub(crate) fn source(keys:&[spec::Key])->Source<'_> {
    let mut source=BTreeMap::new();
    for key in keys {
        source.entry(key.key.as_str()).and_modify(|value|*value=None).or_insert(Some(key));
    }
    source
}
pub(crate) fn resolve<'a,'k>(root:&'a Yaml,source:&Source<'k>,text:&str,pointer:&str)->Option<(&'a Yaml,&'k spec::Key)> {
    let mut value=root;
    let mut path=String::new();
    let mut target=None;
    for token in pointer.strip_prefix('/')?.split('/') {
        if token.contains('%') {return None;}
        let name=token.replace("~1","/").replace("~0","~");
        if name.replace('~',"~0").replace('/',"~1")!=token {return None;}
        value=value.as_hash()?.get(&Yaml::String(name.clone()))?;
        if !path.is_empty() {path.push('.');} path.push_str(&name);
        let key=source.get(path.as_str()).copied().flatten()?;
        // Plain/simple quoted block keys only: escape transformations, flow
        // spans, complex/numeric keys and flattened impostors fail closed.
        if key.at>=key.end || text.get(key.at..key.end)?!=name {return None;}
        target=Some(key);
    }
    Some((value,target?))
}
pub(crate) fn definition(text:&str,pointer:&str)->Option<Range> {
    let root=model(text)?;
    let keys=crate::yaml_scan::keys(text,MAX_NODES,4<<20,MAX_DEPTH)?;
    if keys.len()>MAX_NODES {return None;}
    let source=source(&keys);
    let (_,key)=resolve(&root,&source,text,pointer)?;
    Some(spring_text::range(text,key.at,key.end))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_yaml_forms_and_excess_structure_are_quiet_before_source_mapping() {
        for (text,pointer) in [
            ("Pet: {}\n---\nPet: {}\n","/Pet"),
            ("Pet: {}\n'Pet': {}\n","/Pet"),
            ("Pet: &pet {}\nOther: *pet\n","/Pet"),
            ("Pet: !!map {}\n","/Pet"),
            ("Pet:\n  $id: other.yaml\n","/Pet"),
            ("Pet:\n  <<: {}\n","/Pet"),
            ("Pet: {type: string}\n","/Pet/type"),
            ("Pet:\n  - type: string\n","/Pet/0/type"),
            ("200: {}\n","/200"),
            ("'A''B': {}\n","/A'B"),
            ("Pet: [\n","/Pet"),
            ("Pet: {}\n","/P%65t"),
            ("P~2t: {}\n","/P~2t"),
        ] { assert!(definition(text,pointer).is_none(),"{text} {pointer}"); }
        assert!(definition(&" ".repeat(MAX_BYTES+1),"/Pet").is_none());
        let wide=(0..5000).map(|n|format!("k{n}: v\n")).collect::<String>();
        assert!(definition(&wide,"/k0").is_none());
        let deep=(0..60).map(|n|format!("{}k{n}:\n"," ".repeat(n*2))).collect::<String>();
        assert!(definition(&deep,"/k0").is_none());
    }
}
