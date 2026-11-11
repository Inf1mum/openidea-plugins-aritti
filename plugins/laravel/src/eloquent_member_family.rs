//! Closed source member families for explicit reviewed refactors. No host calls.
use super::*;
#[derive(Default)]
struct Node{
    parents:Vec<String>,traits:Vec<String>,methods:BTreeMap<String,Option<Entry>>,properties:BTreeMap<String,Entry>,
    blocked:bool,path:String,reserved_properties:BTreeSet<String>,always_throwing:BTreeSet<String>,
}
fn same(a:&Entry,b:&Entry)->bool{a.path==b.path&&a.at==b.at&&a.width==b.width}
fn graph(index:&Index,doc:&Document,owns:&dyn Fn(&str)->bool)->Option<BTreeMap<String,Node>>{
    let mut nodes=BTreeMap::<String,Node>::new();
    for(full,info)in &index.metadata.class_facts{
        let node=nodes.entry(full.to_ascii_lowercase()).or_default();node.path=info.path.clone();
        node.parents=info.parents.iter().map(|name|name.to_ascii_lowercase()).collect();node.traits=info.traits.iter().map(|name|name.to_ascii_lowercase()).collect();
        node.blocked=index.metadata.ambiguous_classes.contains(full)||!owns(&info.path);
    }
    for(full,info)in &index.eloquent_models{
        let node=nodes.entry(full.clone()).or_default();node.path=info.path.clone();
        if node.parents.is_empty()&&!info.parent.is_empty(){node.parents.push(info.parent.to_ascii_lowercase());}
        node.traits=info.traits.iter().map(|name|name.to_ascii_lowercase()).collect();
        node.blocked|=info.ambiguous||!owns(&info.path)||!info.adaptations.is_empty()||info.trait_adaptations;
        node.reserved_properties=info.semantic.blocked_properties.clone();
        node.always_throwing=info.semantic.always_throwing.clone();
        for name in &info.semantic.source_methods{node.methods.insert(name.to_ascii_lowercase(),info.returns.get(name).filter(|method|method.entry.path==info.path).map(|method|method.entry.clone()));}
    }
    for(full,info)in &index.eloquent_objects{
        let node=nodes.entry(full.clone()).or_default();node.path=info.path.clone();node.blocked|=info.ambiguous||!owns(&info.path);
        node.parents=info.parents.iter().map(|name|name.to_ascii_lowercase()).collect();
        for(name,method)in &info.methods{if method.entry.path==info.path{node.methods.insert(name.to_ascii_lowercase(),Some(method.entry.clone()));}}
        for(name,entry)in &info.properties{if entry.path==info.path{node.properties.insert(name.clone(),entry.clone());}}
    }
    if let Some(current)=super::super::model(&doc.text,&doc.path){
        if !owns(&doc.path){return None;}
        let mut node=Node{path:doc.path.clone(),parents:if current.parent.is_empty(){vec![]}else{vec![current.parent.to_ascii_lowercase()]},traits:current.traits.iter().map(|name|name.to_ascii_lowercase()).collect(),blocked:!current.adaptations.is_empty()||current.trait_adaptations,reserved_properties:current.semantic.blocked_properties.clone(),always_throwing:current.semantic.always_throwing.clone(),..Node::default()};
        if let Some(source)=Source::new(&doc.text)&&let Some(names)=Names::parse(&source)&&let Some((_,class,body,_))=&names.class{
            let mut interfaces=false;
            for token in &source.tokens[class+2..*body]{if token.value=="implements"{interfaces=true;continue;}if interfaces&&token.kind==Kind::Name{node.parents.push(names.resolve(token.value).to_ascii_lowercase());}}
        }
        for name in &current.semantic.source_methods{node.methods.insert(name.to_ascii_lowercase(),current.returns.get(name).map(|method|method.entry.clone()));}
        if let Some(object)=objects::current(&doc.text,&doc.path){node.properties=object.properties;}
        if nodes.get(&current.full.to_ascii_lowercase()).is_some_and(|old|old.path!=doc.path||old.blocked){return None;}
        nodes.insert(current.full.to_ascii_lowercase(),node);
    }
    for(full,node)in &mut nodes{if node.path!=doc.path&&!index.eloquent_models.contains_key(full)&&!index.eloquent_objects.contains_key(full){node.blocked=true;}}
    Some(nodes)
}
fn ancestors(nodes:&BTreeMap<String,Node>,full:&str)->Option<BTreeSet<String>>{
    fn visit(nodes:&BTreeMap<String,Node>,full:&str,seen:&mut BTreeSet<String>,active:&mut BTreeSet<String>)->Option<()>{
        if active.contains(full){return None;}if seen.contains(full){return Some(());}if seen.len()+active.len()>=128{return None;}
        let node=nodes.get(full)?;if node.blocked{return None;}active.insert(full.into());
        for parent in node.parents.iter().chain(node.traits.iter()){visit(nodes,parent,seen,active)?;}
        active.remove(full);seen.insert(full.into());Some(())
    }
    let mut seen=BTreeSet::new();visit(nodes,full,&mut seen,&mut BTreeSet::new())?;Some(seen)
}
struct Family{entries:Vec<Entry>,name:String,method:bool,nodes:BTreeSet<String>}
fn family(index:&Index,doc:&Document,destination:&Entry,owns:&dyn Fn(&str)->bool)->Option<(BTreeMap<String,Node>,Family)>{
    if !owns(&destination.path){return None;}
    let nodes=graph(index,doc,owns)?;let mut origin=None;
    for(full,node)in &nodes{
        for(name,entry)in &node.methods{if entry.as_ref().is_some_and(|entry|same(entry,destination)){if origin.is_some(){return None;}origin=Some((full.clone(),name.clone(),true));}}
        for(name,entry)in &node.properties{if same(entry,destination){if origin.is_some(){return None;}origin=Some((full.clone(),name.clone(),false));}}
    }
    let (origin,name,method)=origin?;let lineage=ancestors(&nodes,&origin)?;
    let mut roots=BTreeSet::from([origin]);
    for full in &lineage{let node=nodes.get(full)?;if method&&node.methods.contains_key(&name)||!method&&node.properties.contains_key(&name){roots.insert(full.clone());}}
    let mut affected=BTreeSet::new();let mut entries=Vec::new();
    for(full,node)in &nodes{
        // Traverse descendants without requiring unrelated external graphs to
        // be complete. Once connected, every ancestor must be owned/proven.
        let mut pending=vec![full.as_str()];let mut seen=BTreeSet::new();let mut related=false;
        while let Some(current)=pending.pop(){if seen.len()>=128{return None;}if !seen.insert(current){continue;}if roots.contains(current){related=true;break;}if let Some(node)=nodes.get(current){pending.extend(node.parents.iter().chain(node.traits.iter()).map(String::as_str));}}
        if !related{continue;}
        affected.extend(ancestors(&nodes,full)?);
        if method{
            if let Some(entry)=node.methods.get(&name){entries.push(entry.clone()?);}
        }else if let Some(entry)=node.properties.get(&name){entries.push(entry.clone());}else if node.reserved_properties.contains(&name){return None;}
    }
    entries.sort_by(|a,b|(&a.path,a.at.line,a.at.character).cmp(&(&b.path,b.at.line,b.at.character)));entries.dedup_by(|a,b|same(a,b));
    if entries.is_empty()||entries.len()>128{return None;}
    Some((nodes,Family{entries,name,method,nodes:affected}))
}
pub(crate) fn member_family(index:&Index,doc:&Document,destination:&Entry,owns:&dyn Fn(&str)->bool)->Option<Vec<Entry>>{
    Some(family(index,doc,destination,owns)?.1.entries)
}
/// None means the family cannot be proven closed, so no rename is safe.
pub(crate) fn member_rename_collision(index:&Index,doc:&Document,destination:&Entry,new:&str,owns:&dyn Fn(&str)->bool)->Option<bool>{
    let (nodes,family)=family(index,doc,destination,owns)?;
    if family.method&&new.eq_ignore_ascii_case(&family.name)||!family.method&&new==family.name{return Some(false);}
    Some(family.nodes.iter().any(|full|{let node=&nodes[full];if family.method{node.methods.contains_key(&new.to_ascii_lowercase())}else{node.properties.contains_key(new)||node.reserved_properties.contains(new)}}))
}
/// Source declaration existence, not a promise of public route callability.
/// Absence requires a complete owned hierarchy and no returning magic dispatch.
pub(crate) fn method_presence(index:&Index,doc:&Document,full:&str,method:&str,owns:&dyn Fn(&str)->bool)->Option<bool>{
    let nodes=graph(index,doc,owns)?;let ancestors=ancestors(&nodes,&full.trim_start_matches('\\').to_ascii_lowercase())?;
    if ancestors.iter().any(|full|nodes[full].methods.contains_key(&method.to_ascii_lowercase())){return Some(true);}
    if ancestors.iter().any(|full|["__call","__callstatic"].iter().any(|method|nodes[full].methods.contains_key(*method)&&!nodes[full].always_throwing.contains(*method))){return None;}
    Some(false)
}
