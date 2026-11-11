//! Source-only connection-selection policy. Names come only from explicit
//! literals; a configured-default policy still requires captured app provenance.
use super::*;
#[derive(Clone,Debug,PartialEq,Eq)]
pub(crate) enum ConnectionSelection{Named(String),ConfiguredDefault}
fn complete_php(text:&str,source:&Source)->bool{
    if source.tokens.iter().enumerate().any(|(at,token)|matches!(token.value,"("|"["|"{")&&!source.pairs.contains_key(&at)){return false;}
    let bytes=text.as_bytes();let mut at=0;
    while at<bytes.len(){
        if matches!(bytes[at],b'\''|b'"'|b'`'){
            let quote=bytes[at];at+=1;let mut closed=false;
            while at<bytes.len(){if bytes[at]==b'\\'{at=(at+2).min(bytes.len());}else if bytes[at]==quote{at+=1;closed=true;break;}else{at+=1;}}
            if !closed{return false;}
        }else if bytes[at..].starts_with(b"/*"){
            let Some(end)=text[at+2..].find("*/")else{return false};at+=end+4;
        }else if bytes[at..].starts_with(b"//")||bytes[at]==b'#'&&bytes.get(at+1)!=Some(&b'['){
            at=text[at..].find('\n').map_or(bytes.len(),|end|at+end);
        }else{at+=1;}
    }true
}
pub(crate) fn cache_connection_overrides(paths:&mut BTreeSet<String>,text:&str,path:&str){
    paths.remove(path);
    let Some(source)=Source::new(text).filter(|source|complete_php(text,source))else{paths.insert(path.into());return};
    // Framework bootstrap's standard resolver wiring is the trusted baseline;
    // project/package overrides are not assumed to preserve it.
    let framework=path.replace('\\',"/").contains("/vendor/laravel/framework/");
    for(at,token)in source.tokens.iter().enumerate(){
        let name=token.value.to_ascii_lowercase();
        if ["setconnectionresolver","unsetconnectionresolver","setdefaultconnection"].contains(&name.as_str())&&source.is(at+1,"(")&&at>0&&matches!(source.tokens[at-1].value,"::"|"->"|"?->"){
            if !framework{paths.insert(path.into());}return;
        }
        if !matches!(name.as_str(),"config"|"set")||!source.is(at+1,"("){continue;}
        let Some(args)=container::arguments(&source,at+1)else{paths.insert(path.into());return};
        let Some(first)=args.first()else{continue};
        let from=&source.tokens[first.start];let to=&source.tokens[first.end-1];
        let literal=php::literal(text,from.start..to.end).is_some_and(|(key,_)|matches!(key,"database.default"|"database"));
        let array=source.is(first.start,"[")&&php::items(text,from.start).iter().any(|item|item.key.as_ref().and_then(|range|php::literal(text,range.clone())).is_some_and(|(key,_)|matches!(key,"database.default"|"database")));
        let mutable=if name=="config"{array}else{literal||array};
        if mutable&&!framework{paths.insert(path.into());return;}
    }
}
fn native_trait(full:&str)->bool{
    ["Illuminate\\Database\\Eloquent\\Factories\\HasFactory","Illuminate\\Database\\Eloquent\\SoftDeletes","Illuminate\\Database\\Eloquent\\HasBuilder","Illuminate\\Database\\Eloquent\\HasCollection","Illuminate\\Database\\Eloquent\\Concerns\\HasUuids","Illuminate\\Database\\Eloquent\\Concerns\\HasUlids","Illuminate\\Database\\Eloquent\\Concerns\\HasUniqueStringIds","Illuminate\\Database\\Eloquent\\Concerns\\HasVersion7Uuids","Illuminate\\Database\\Eloquent\\Prunable","Illuminate\\Database\\Eloquent\\MassPrunable"].iter().any(|name|full.eq_ignore_ascii_case(name))
}
fn native_attribute(source:&Source,names:&Names)->bool{
    source.tokens.iter().enumerate().any(|(at,token)|{
        if token.kind!=Kind::Name{return false;}
        if !["Illuminate\\Database\\Eloquent\\Attributes\\Connection","Illuminate\\Database\\Eloquent\\Attributes\\Table","Illuminate\\Database\\Eloquent\\Attributes\\Boot","Illuminate\\Database\\Eloquent\\Attributes\\Initialize"].iter().any(|full|names.resolve(token.value).eq_ignore_ascii_case(full)){return false;}
        source.pairs.iter().any(|(open,close)|*open<at&&at<*close&&source.is(*open,"[")&&*open>0&&source.is(open-1,"#"))
    })
}
fn safe_source(source:&Source,names:&Names)->bool{
    if native_attribute(source,names){return false;}
    if source.tokens.iter().any(|token|{
        let name=token.value.to_ascii_lowercase();
        ["getconnectionname","getconnection","setconnection","setconnectionresolver","unsetconnectionresolver","__construct","boot","booted","booting","boottraits","initializetraits","initializemodelattributes","gettable","newinstance","newfrombuilder","newmodelinstance"].contains(&name.as_str())||name.starts_with("initialize")||name.starts_with("boot")
    }){return false;}
    !source.tokens.windows(3).any(|tokens|tokens[0].value=="$this"&&matches!(tokens[1].value,"->"|"?->")&&tokens[2].value.eq_ignore_ascii_case("connection"))
}
fn trait_sources(index:&Index,project:&Project,model:&Model,active:&mut BTreeSet<String>,out:&mut BTreeSet<String>,depth:u8)->Option<()>{
    if depth>=8||out.len()>32{return None;}
    for full in &model.traits{
        let Some(info)=index.eloquent_models.get(&full.to_ascii_lowercase())else{if native_trait(full){continue;}return None};
        if !info.is_trait||info.ambiguous||!owned(project,&info.path)||!active.insert(full.to_ascii_lowercase()){return None;}
        let text=read(project,&info.path)?;let current=super::super::model(&text,&info.path)?;
        if current.full!=info.full||current.traits!=info.traits||!current.is_trait||!current.adaptations.is_empty()||current.trait_adaptations{return None;}
        out.insert(info.path.clone());trait_sources(index,project,&current,active,out,depth+1)?;active.remove(&full.to_ascii_lowercase());
    }Some(())
}
pub(crate) fn connection_selection(index:&Index,project:&Project,full:&str)->Option<ConnectionSelection>{
    if index.partial||!index.connection_override_sources.is_empty(){return None;}
    let doc=Document{path:String::new(),language:"php".into(),version:0,text:format!("<?php \\{full}::query()->")};
    let context=context(&doc,text::position(&doc.text,doc.text.len()))?;
    let resolver=Resolver{context:&context,index,overlay:None,object_overlay:None,path:"",owns:&|path|owned(project,path),owner_cache:RefCell::new(BTreeMap::new())};
    let mut selected=None;
    for model in resolver.lineage(full)?.into_iter().filter(|model|!model.is_trait){
        let text=read(project,&model.path)?;let source=Source::new(&text).filter(|source|complete_php(&text,source))?;let names=Names::parse(&source)?;let (declared,_,body,parent)=names.class.as_ref()?;
        let current=super::super::model(&text,&model.path)?;
        if declared!=&model.full||parent!=&model.parent||current.traits!=model.traits||current.trait_adaptations||!current.adaptations.is_empty()||!source.pairs.contains_key(body)||!safe_source(&source,&names){return None;}
        let mut traits=BTreeSet::new();trait_sources(index,project,&current,&mut BTreeSet::new(),&mut traits,0)?;
        for path in traits{let text=read(project,&path)?;let source=Source::new(&text).filter(|source|complete_php(&text,source))?;let names=Names::parse(&source)?;if !safe_source(&source,&names)||source.tokens.iter().any(|token|token.value=="$connection"){return None;}}
        if selected.is_some(){continue;}
        let declarations=source.tokens.iter().enumerate().filter(|(at,token)|token.value=="$connection"&&source.paths[*at]==vec![*body]&&!source.pairs.iter().any(|(open,close)|*open<*at&&*at<*close&&matches!(source.tokens[*open].value,"("|"["))).collect::<Vec<_>>();
        if declarations.is_empty(){continue;}let[(at,_)]=declarations.as_slice()else{return None};
        if source.is(at+1,";")||source.is(at+1,"=")&&source.is(at+2,"null")&&source.is(at+3,";"){selected=Some(ConnectionSelection::ConfiguredDefault);continue;}
        if !source.is(at+1,"=")||!source.is(at+3,";"){return None;}
        let token=source.tokens.get(at+2).filter(|token|token.kind==Kind::String)?;let(value,_)=php::literal(&text,token.start..token.end)?;let value=metadata_inference::decode_literal(value,token.value.as_bytes()[0])?;
        if value.is_empty()||value.chars().count()>128||value.chars().any(char::is_control){return None;}selected=Some(ConnectionSelection::Named(value));
    }
    let selected=selected.unwrap_or(ConnectionSelection::ConfiguredDefault);
    if selected==ConnectionSelection::ConfiguredDefault&&index.metadata.framework_version.is_none_or(|version|version<(10,0,0)){return None;}
    Some(selected)
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn source_policy_distinguishes_named_default_and_unsafe_runtime_hooks(){
        use openidea_plugin::{host,mock::MockHost};
        let mock=MockHost::install();mock.add_file("artisan","<?php");
        mock.add_file("app/Models/Base.php","<?php namespace App\\Models;class Base extends \\Illuminate\\Database\\Eloquent\\Model{protected $connection='tenant';}");
        let cases=[("Plain","",Some(ConnectionSelection::ConfiguredDefault)),("Named","protected $connection='analytics';",Some(ConnectionSelection::Named("analytics".into()))),("Null","protected $connection=null;",Some(ConnectionSelection::ConfiguredDefault)),("Dynamic","protected $connection=ENV_CONNECTION;",None),("Hook","protected static function booted(){}",None),("Init","public function initializeTenant(){}",None),("Ctor","public function __construct(){}",None)];
        for(name,body,_)in &cases{mock.add_file(&format!("app/Models/{name}.php"),&format!("<?php namespace App\\Models;class {name} extends \\Illuminate\\Database\\Eloquent\\Model{{{body}}}"));}
        mock.add_file("app/Models/Child.php","<?php namespace App\\Models;class Child extends Base{protected $connection=null;}");
        mock.add_file("app/Models/Attributed.php","<?php namespace App\\Models;use Illuminate\\Database\\Eloquent\\Attributes\\Connection;#[Connection('other')]class Attributed extends \\Illuminate\\Database\\Eloquent\\Model{}");
        let project=host::projects().unwrap().remove(0);let mut index=Index::build(&project);index.metadata.framework_version=Some((13,40,0));
        for(name,_,expected)in cases{assert_eq!(connection_selection(&index,&project,&format!("App\\Models\\{name}")),expected,"{name}");}
        assert_eq!(connection_selection(&index,&project,"App\\Models\\Child"),Some(ConnectionSelection::ConfiguredDefault));
        assert_eq!(connection_selection(&index,&project,"App\\Models\\Attributed"),None);
        mock.open_editor("/project/app/Models/Named.php","php","<?php namespace App\\Models;class Named extends \\Illuminate\\Database\\Eloquent\\Model{protected $connection=config('tenant');}",&[]);
        assert_eq!(connection_selection(&index,&project,"App\\Models\\Named"),None);
        index.partial=true;assert_eq!(connection_selection(&index,&project,"App\\Models\\Plain"),None);
    }
    #[test]
    fn global_mutations_withhold_default_provenance_but_reads_and_decoys_do_not(){
        let mut paths=BTreeSet::new();
        for source in ["<?php DB::setDefaultConnection('tenant');","<?php Model::setConnectionResolver($resolver);","<?php config(['database.default'=>'tenant']);","<?php Config::set('database.default','tenant');","<?php config(['database'=>['default'=>'tenant']]);","<?php Config::set('database',['default'=>'tenant']);","<?php function incomplete(){","<?php /* incomplete","<?php $value='incomplete"]{
            cache_connection_overrides(&mut paths,source,"/project/app/Provider.php");assert!(paths.contains("/project/app/Provider.php"),"{source}");
        }
        for source in ["<?php config('database.default');","<?php $note='DB::setDefaultConnection(tenant)'; // config(['database.default'=>'tenant'])\n"]{
            cache_connection_overrides(&mut paths,source,"/project/app/Provider.php");assert!(paths.is_empty(),"{source}");
        }
    }
}

#[derive(Clone,Debug)]
pub(crate) struct PrimaryKey{pub name:String,pub evidence_paths:Vec<String>}
/// Fresh, closed source ancestry proof of the key name, independent of captured
/// row values. Native Table attributes and lifecycle/key overrides are unknown.
pub(crate) fn primary_key(index:&Index,project:&Project,full:&str)->Option<PrimaryKey>{
    if index.partial||index.metadata.framework_version.is_none_or(|version|version<(10,0,0)){return None;}
    let doc=Document{path:String::new(),language:"php".into(),version:0,text:format!("<?php \\{full}::query()->")};let context=context(&doc,text::position(&doc.text,doc.text.len()))?;
    let resolver=Resolver{context:&context,index,overlay:None,object_overlay:None,path:"",owns:&|path|owned(project,path),owner_cache:RefCell::new(BTreeMap::new())};
    let mut selected=None;let mut evidence=BTreeSet::new();
    for model in resolver.lineage(full)?.into_iter().filter(|model|!model.is_trait){
        let text=read(project,&model.path)?;let source=Source::new(&text).filter(|source|complete_php(&text,source))?;let names=Names::parse(&source)?;let(declared,_,body,parent)=names.class.as_ref()?;
        let current=super::super::model(&text,&model.path)?;
        if declared!=&model.full||parent!=&model.parent||current.traits!=model.traits||current.trait_adaptations||!current.adaptations.is_empty()||!source.pairs.contains_key(body)||!safe_source(&source,&names)||!safe_key_source(&source){return None;}
        evidence.insert(model.path.clone());let mut traits=BTreeSet::new();trait_sources(index,project,&current,&mut BTreeSet::new(),&mut traits,0)?;
        for path in traits{
            let text=read(project,&path)?;let source=Source::new(&text).filter(|source|complete_php(&text,source))?;let names=Names::parse(&source)?;
            if names.class.as_ref().is_none_or(|(full,_,_,_)|!native_trait(full))&&(!safe_source(&source,&names)||!safe_key_source(&source)||source.tokens.iter().any(|token|token.value=="$primaryKey")){return None;}
            evidence.insert(path);
        }
        if selected.is_some(){continue;}
        let declarations=source.tokens.iter().enumerate().filter(|(at,token)|token.value=="$primaryKey"&&source.paths[*at]==vec![*body]&&!source.pairs.iter().any(|(open,close)|*open<*at&&*at<*close&&matches!(source.tokens[*open].value,"("|"["))).collect::<Vec<_>>();
        if declarations.is_empty(){continue;}let[(at,_)]=declarations.as_slice()else{return None};
        if !source.is(at+1,"=")||!source.is(at+3,";"){return None;}
        let token=source.tokens.get(at+2).filter(|token|token.kind==Kind::String)?;let(value,_)=php::literal(&text,token.start..token.end)?;let value=metadata_inference::decode_literal(value,token.value.as_bytes()[0])?;
        if !crate::generator_plan::identifier(&value){return None;}selected=Some(value);
    }
    Some(PrimaryKey{name:selected.unwrap_or_else(||"id".into()),evidence_paths:evidence.into_iter().collect()})
}
fn safe_key_source(source:&Source)->bool{
    !source.tokens.iter().any(|token|["getkeyname","setkeyname"].contains(&token.value.to_ascii_lowercase().as_str()))&&!source.tokens.windows(3).any(|tokens|tokens[0].value=="$this"&&matches!(tokens[1].value,"->"|"?->")&&tokens[2].value=="primaryKey")
}
#[cfg(test)]mod key_tests{
    use super::*;
    #[test]fn key_name_proof_requires_fresh_owned_closed_lifecycle_and_literal_declarations(){
        use openidea_plugin::{host,mock::MockHost};let mock=MockHost::install();mock.add_file("artisan","<?php");
        for(name,body)in [("Plain",""),("Uuid","protected $primaryKey='uuid';"),("Dynamic","protected $primaryKey=KEY_NAME;"),("Hook","public function getKeyName(){return config('key');}"),("Boot","protected static function booted(){}")]{mock.add_file(&format!("app/Models/{name}.php"),&format!("<?php namespace App\\Models;class {name} extends \\Illuminate\\Database\\Eloquent\\Model{{{body}}}"));}
        let project=host::projects().unwrap().remove(0);let mut index=Index::build(&project);index.metadata.framework_version=Some((13,40,0));
        assert_eq!(primary_key(&index,&project,"App\\Models\\Plain").unwrap().name,"id");let uuid=primary_key(&index,&project,"App\\Models\\Uuid").unwrap();assert_eq!(uuid.name,"uuid");assert_eq!(uuid.evidence_paths,vec!["/project/app/Models/Uuid.php"]);
        for name in ["Dynamic","Hook","Boot"]{assert!(primary_key(&index,&project,&format!("App\\Models\\{name}")).is_none());}
        mock.open_editor("/project/app/Models/Uuid.php","php","<?php namespace App\\Models;class Uuid extends \\Illuminate\\Database\\Eloquent\\Model{protected $primaryKey=null;}",&[]);assert!(primary_key(&index,&project,"App\\Models\\Uuid").is_none());
        index.metadata.framework_version=None;assert!(primary_key(&index,&project,"App\\Models\\Plain").is_none());
    }
}
