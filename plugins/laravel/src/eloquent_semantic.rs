//! Shared, cached source declarations and resolved model facts. No host calls.
use super::*;
#[path="eloquent_member_family.rs"]
mod family;
pub(crate) use family::{member_family,member_rename_collision,method_presence};

#[derive(Clone,Debug)]
pub(crate) struct ClassReference { pub full:String, pub entry:Entry }
#[derive(Clone,Debug,Default)]
pub(super) struct Declared {
    pub declaration:Option<Entry>,
    pub callables:BTreeMap<String,pipe::Callable>,
    pub properties:BTreeMap<String,ClassReference>,
    pub attributes:BTreeMap<String,ClassReference>,
    pub constructed:BTreeMap<String,ClassReference>,
    pub blocked_properties:BTreeSet<String>,
    pub factory_generic:Option<ClassReference>,
    pub external_relations:Vec<(String,String,Relation)>,
    pub container:container::Declared,
    pub relation_declarations:relations::Declared,
    pub package_queries:Option<std::rc::Rc<package_queries::Capabilities>>,
    pub query_macro_overrides:BTreeSet<String>,
    pub timestamps:Option<Option<bool>>,
    pub date_constants:BTreeMap<String,Option<Option<String>>>,
    pub source_methods:BTreeSet<String>,
    pub always_throwing:BTreeSet<String>,
    pub accessor_declarations:BTreeMap<String,accessors::Accessor>,
}
fn class_reference(source:&Source,names:&Names,path:&str,at:usize)->Option<ClassReference>{
    let token=source.tokens.get(at)?;
    if token.kind!=Kind::Name||!source.is(at+1,"::")||!source.is(at+2,"class"){return None;}
    let full=match token.value {
        "self"=>names.class.as_ref()?.0.clone(),
        "static"|"parent"=>return None,
        value=>names.resolve(value),
    };
    Some(ClassReference{full,entry:entry(source,path,at)})
}
pub(super) fn declared(source:&Source,names:&Names,path:&str)->Declared{
    let mut result=Declared::default();result.query_macro_overrides=package_queries::declared_overrides(source,names);result.container=container::declared(source,names,path);result.callables=pipe::methods(source,names,path);result.accessor_declarations=accessors::declared(source,names,path);result.relation_declarations=relations::declared(source,names,path);
    let Some((_,class,body,_))=names.class.as_ref()else{return result};
    result.declaration=Some(entry(source,path,class+1));
    if let Some((start,_))=super::class_layout(source.text){
        let prefix=source.text[..start].trim_end();
        if prefix.ends_with("*/")&&let Some(begin)=prefix.rfind("/**"){
            let doc=&prefix[begin+3..prefix.len()-2];let mut found=Vec::new();
            for line in doc.lines(){if let Some((_,tail))=line.split_once("@extends")&&let Some((base,tail))=tail.trim().split_once('<')&&names.resolve(base.trim())=="Illuminate\\Database\\Eloquent\\Factories\\Factory"&&let Some((target,_))=tail.split_once('>'){
                let target=target.trim();if crate::generator_plan::qualified(target){found.push(ClassReference{full:names.resolve(target),entry:Entry{path:path.into(),at:text::position(source.text,begin),width:0,detail:Some("Declared factory model generic".into())}});}
            }}
            if found.len()==1{result.factory_generic=found.pop();}
        }
    }
    for (at,token) in source.tokens.iter().enumerate(){
        if source.paths[at]==vec![*body]&&token.value=="$timestamps"{
            let value=if source.is(at+1,"=")&&source.is(at+3,";"){match source.tokens[at+2].value.to_ascii_lowercase().as_str(){"true"=>Some(true),"false"=>Some(false),_=>None}}else{None};
            result.timestamps=if result.timestamps.is_none(){Some(value)}else{Some(None)};
        }
        if source.paths[at]==vec![*body]&&at>0&&source.is(at-1,"const")&&matches!(token.value,"CREATED_AT"|"UPDATED_AT"|"DELETED_AT"){
            let value=if source.is(at+1,"=")&&source.is(at+3,";"){
                if source.is(at+2,"null"){Some(None)}else{source.tokens.get(at+2).filter(|t|t.kind==Kind::String).and_then(|t|php::literal(source.text,t.start..t.end).and_then(|(value,_)|metadata_inference::decode_literal(value,t.value.as_bytes()[0]))).map(Some)}
            }else{None};
            if result.date_constants.contains_key(token.value){result.date_constants.insert(token.value.into(),None);}else{result.date_constants.insert(token.value.into(),value);}
        }
        if token.kind==Kind::Var&&source.paths[at]==vec![*body]{
            let name=token.value.trim_start_matches('$').to_owned();
            // All declarations block an inherited literal, including null and dynamic values.
            if result.blocked_properties.insert(name.clone())&&source.is(at+1,"=")&&source.is(at+5,";"){
                if let Some(reference)=class_reference(source,names,path,at+2){result.properties.insert(name,reference);}
            }else{result.properties.remove(&name);}
        }
        if at<*class&&token.value=="#"&&source.is(at+1,"["){
            let Some(close)=source.pairs.get(&(at+1))else{continue};
            if *close>=*class||!source.is(at+3,"("){continue;}
            let Some(argument_end)=source.pairs.get(&(at+3))else{continue};
            if *argument_end!=at+7||*close!=at+8{continue;}
            let Some(reference)=class_reference(source,names,path,at+4)else{continue};
            let attribute=names.resolve(source.tokens[at+2].value);
            // Duplicate attributes are not a unique source declaration.
            if result.attributes.contains_key(&attribute){result.attributes.remove(&attribute);}else{result.attributes.insert(attribute,reference);}
        }
    }
    for function in source.functions().into_iter().filter(|f|source.paths[f.start]==vec![*body]){
        let Some(at)=function.name else{continue};let b=function.body+1;
        let name=source.tokens[at].value;
        result.source_methods.insert(name.into());
        if source.is(b,"throw"){
            let statements=(b..function.end).filter(|at|source.is(*at,";")&&source.paths[*at]==source.paths[b]).collect::<Vec<_>>();
            if statements.len()==1&&statements[0]+1==function.end{result.always_throwing.insert(name.to_ascii_lowercase());}
        }
        if !source.is(b,"return"){continue;}
        let target=if source.is(b+1,"new")&&source.tokens.get(b+2).is_some_and(|t|t.kind==Kind::Name){
            let class=source.tokens[b+2].value;
            if matches!(class,"self"|"static"|"parent"){continue;}
            let end=if source.is(b+3,"("){source.pairs.get(&(b+3)).copied().unwrap_or(usize::MAX)}else{b+2};
            if end==usize::MAX||!source.is(end+1,";")||end+2!=function.end{continue;}
            Some(ClassReference{full:names.resolve(class),entry:entry(source,path,b+2)})
        }else if source.tokens.get(b+1).is_some_and(|t|t.kind==Kind::Name)&&source.is(b+2,"::")&&source.is(b+3,"new")&&source.is(b+4,"("){
            let Some(end)=source.pairs.get(&(b+4))else{continue};
            if !source.is(end+1,";")||end+2!=function.end{continue;}
            Some(ClassReference{full:names.resolve(source.tokens[b+1].value),entry:entry(source,path,b+1)})
        }else{None};
        if let Some(target)=target{result.constructed.insert(source.tokens[at].value.into(),target);}
    }
    // Explicit provider boot registrations only; nested conditional registrations
    // and arbitrary callback evaluation do not establish unconditional facts.
    let functions=source.functions();
    if let Some(boot)=functions.iter().find(|f|f.name.is_some_and(|at|source.is(at,"boot"))&&source.paths[f.start]==vec![*body]){
        for at in boot.body+1..boot.end {
            if source.paths[at]!=vec![*body,boot.body]||(at!=boot.body+1&&!matches!(source.tokens[at-1].value,";"|"}"))||!source.is(at+1,"::")||!source.is(at+2,"resolveRelationUsing")||!source.is(at+3,"(")||!source.is(at+5,","){continue;}
            let Some(owner)=source.tokens.get(at).filter(|t|t.kind==Kind::Name)else{continue};
            let Some(name)=source.tokens.get(at+4).filter(|t|t.kind==Kind::String).and_then(|t|php::literal(source.text,t.start..t.end).map(|(value,_)|value.to_owned()))else{continue};
            if !crate::generator_plan::identifier(&name){continue;}
            let callback_start=at+6+usize::from(source.is(at+6,"static"));
            let Some(callback)=functions.iter().find(|f|f.start==callback_start&&f.name.is_none())else{continue};
            let Some(end)=source.pairs.get(&(at+3))else{continue};
            let arrow=source.is(callback.body,"=>");
            if if arrow{callback.end!=*end}else{callback.end+1!=*end}{continue;}
            let params=&source.tokens[callback.params.clone()];
            let vars=params.iter().filter(|t|t.kind==Kind::Var).collect::<Vec<_>>();
            let [receiver]=vars.as_slice()else{continue};
            if params.last().is_none_or(|last|last.value!=receiver.value)||params.iter().any(|t|matches!(t.value,","|"="|"&"|"...")){continue;}
            if params.len()>1{
                let Some(kind)=helper_facts::declared_type(&params[..params.len()-1].iter().collect::<Vec<_>>(),names,"__OpenIDEA_External_Receiver__",false)else{continue};
                if ![names.resolve(owner.value).as_str(),"Illuminate\\Database\\Eloquent\\Model"].iter().any(|full|kind.trim_start_matches('?').trim_start_matches('\\').eq_ignore_ascii_case(full)){continue;}
            }
            let begin=callback.body+1;
            let (begin,end)=if arrow{(begin,callback.end)}else{
                if !source.is(begin,"return")||!source.is(callback.end-1,";"){continue;}(begin+1,callback.end-1)
            };
            let Some((kind,many,target))=literal_relation(source,names,receiver.value,begin,end)else{continue};
            let name_token=&source.tokens[at+4];
            result.external_relations.push((names.resolve(owner.value),name,Relation{kind,many,target,source_type:None,evidence_paths:Vec::new(),entry:Entry{path:path.into(),at:text::position(source.text,name_token.start+1),width:(name_token.value.len()-2)as u32,detail:Some("Explicit resolveRelationUsing registration".into())}}));
        }
    }
    result
}
fn literal_relation(source:&Source,names:&Names,receiver:&str,b:usize,end:usize)->Option<(String,bool,String)>{
    if !source.is(b,receiver)||!source.is(b+1,"->")||!source.is(b+3,"("){return None;}
    let kind=source.tokens.get(b+2)?.value;
    let (_,many)=RELATIONS.iter().find(|(name,_)|*name==kind)?;
    let reference=class_reference(source,names,"",b+4)?;
    let close=*source.pairs.get(&(b+3))?;
    if close+1!=end{return None;}
    Some((kind.into(),*many,reference.full))
}

#[derive(Clone,Debug)]
pub(crate) struct Field {pub name:String,pub value_type:Option<String>,pub entry:Entry,pub nullable:Option<bool>,pub read_only:bool}
#[derive(Clone,Debug)]
pub(crate) struct ReadType {pub value_type:String,pub nullable:Option<bool>,pub read_only:bool}
/// Independent source-proven read and write contracts; absent callbacks do not
/// remove raw storage semantics for actual database columns.
#[derive(Clone,Debug)]
pub(crate) struct AccessorFact {pub name:String,pub read_type:Option<String>,pub write_type:Option<String>,pub has_getter:Option<bool>,pub has_setter:Option<bool>,pub getter_entry:Option<Entry>,pub setter_entry:Option<Entry>,pub entry:Entry,pub method_name:String,pub modern:bool}
pub(crate) fn accessor_facts(index:&Index,full:&str,owns:&dyn Fn(&str)->bool)->Option<Vec<AccessorFact>>{
    let doc=Document{path:String::new(),language:"php".into(),version:0,text:format!("<?php \\{full}::query()->")};
    let context=context(&doc,text::position(&doc.text,doc.text.len()))?;
    let resolver=Resolver{context:&context,index,overlay:None,object_overlay:None,path:"",owns,owner_cache:RefCell::new(BTreeMap::new())};
    Some(resolver.accessor_contracts(full)?.into_values().map(|a|AccessorFact{name:a.name,read_type:a.read_type,write_type:a.write_type,has_getter:a.has_getter,has_setter:a.has_setter,getter_entry:a.getter_entry,setter_entry:a.setter_entry,entry:a.entry,method_name:a.method_name,modern:a.modern}).collect())
}
#[derive(Clone,Debug)]
pub(crate) struct ResolvedModel {
    pub full:String,pub path:String,pub table:Option<String>,pub fields:Vec<Field>,
    pub relations:Vec<helper_facts::RelationFact>,pub casts:Vec<helper_facts::CastFact>,pub scopes:Vec<helper_facts::ScopeFact>,
}
/// Consumer-facing model inventory. Ownership, ambiguity, source `$table` and
/// explicit metadata overrides use the same resolver as the editor providers.
pub(crate) fn model_facts(index:&Index,full:&str,owns:&dyn Fn(&str)->bool)->Option<ResolvedModel>{
    let facts=helper_facts::facts(index,full,owns)?;
    let identity=index.eloquent_models.get(&full.to_ascii_lowercase())?;
    let profile=read_profile(index,full,owns)?;
    let mut fields=BTreeMap::new();
    if let Some(columns)=facts.table.as_ref().and_then(|table|index.columns.get(table)){
        for(name,column)in columns.iter().filter(|(_,entry)|owns(&entry.path)){
            let read=read_with(&facts,&profile,name,column.detail.as_deref());
            fields.insert(name.clone(),Field{name:name.clone(),value_type:read.as_ref().map(|read|read.value_type.clone()),nullable:read.as_ref().and_then(|read|read.nullable),read_only:read.as_ref().is_some_and(|read|read.read_only),entry:column.clone()});
        }
    }
    for cast in &facts.casts {
        let read=read_with(&facts,&profile,&cast.name,None);
        fields.insert(cast.name.clone(),Field{name:cast.name.clone(),value_type:read.as_ref().map(|read|read.value_type.clone()),nullable:read.as_ref().and_then(|read|read.nullable),read_only:read.as_ref().is_some_and(|read|read.read_only),entry:cast.entry.clone()});
    }
    for(name,accessor)in &profile.accessors{
        let stored=fields.contains_key(name);
        let raw=index.columns.get(facts.table.as_deref().unwrap_or("")).and_then(|columns|columns.get(name)).and_then(|entry|entry.detail.as_deref());
        let read=read_with(&facts,&profile,name,raw);
        fields.insert(name.clone(),Field{name:name.clone(),value_type:read.as_ref().map(|r|r.value_type.clone()),nullable:read.as_ref().and_then(|r|r.nullable),read_only:!stored&&accessor.has_getter==Some(true)&&accessor.has_setter==Some(false),entry:accessor.getter_entry.clone().or_else(||accessor.setter_entry.clone()).unwrap_or_else(||accessor.entry.clone())});
    }
    if let Some(object)=index.eloquent_objects.get(&full.to_ascii_lowercase()).filter(|o|!o.ambiguous&&owns(&o.path)&&o.metadata_sources.iter().all(|path|owns(path))){
        for(name,entry)in &object.properties{if owns(&entry.path){fields.entry(name.clone()).or_insert(Field{name:name.clone(),value_type:None,nullable:None,read_only:false,entry:entry.clone()});}}
    }
    Some(ResolvedModel{full:identity.full.clone(),path:identity.path.clone(),table:facts.table,fields:fields.into_values().collect(),relations:facts.relations,casts:facts.casts,scopes:facts.scopes})
}
struct ReadProfile{dates:Option<BTreeSet<String>>,possible_dates:BTreeSet<String>,raw_unknown:bool,accessors:BTreeMap<String,accessors::Accessor>}
fn read_profile(index:&Index,full:&str,owns:&dyn Fn(&str)->bool)->Option<ReadProfile>{
    let doc=Document{path:String::new(),language:"php".into(),version:0,text:format!("<?php \\{full}::query()->")};
    let context=context(&doc,text::position(&doc.text,doc.text.len()))?;
    let resolver=Resolver{context:&context,index,overlay:None,object_overlay:None,path:"",owns,owner_cache:RefCell::new(BTreeMap::new())};
    let lineage=resolver.lineage(full)?;
    let mut accessors=resolver.accessor_contracts(full)?;
    if lineage.iter().any(|class|class.declared.iter().any(|name|["getAttribute","getAttributeValue"].iter().any(|candidate|name.eq_ignore_ascii_case(candidate)))){accessors.clear();}
    let mut raw_unknown=lineage.iter().any(|class|class.declared.iter().any(|name|["getDates","usesTimestamps","getCreatedAtColumn","getUpdatedAtColumn","getDeletedAtColumn","getAttribute","getAttributeValue"].iter().any(|candidate|name.eq_ignore_ascii_case(candidate))));
    let enabled=lineage.iter().find_map(|class|class.semantic.timestamps).unwrap_or(Some(true));
    let mut possible_dates=BTreeSet::new();
    for(name,default,active)in [("CREATED_AT","created_at",enabled!=Some(false)),("UPDATED_AT","updated_at",enabled!=Some(false)),("DELETED_AT","deleted_at",lineage.iter().any(|class|class.soft_deletes))]{
        if !active{continue;}
        let value=match lineage.iter().find_map(|class|class.semantic.date_constants.get(name)){Some(Some(value))=>value.clone(),Some(None)=>{raw_unknown=true;None},None=>Some(default.into())};
        if let Some(value)=value{possible_dates.insert(value);}
    }
    let dates=(!raw_unknown&&enabled.is_some()&&index.metadata.framework_version.is_some_and(|version|version>=(10,0,0))).then(||possible_dates.clone());
    Some(ReadProfile{dates,possible_dates,raw_unknown,accessors})
}

fn read_with(facts:&helper_facts::Facts,profile:&ReadProfile,name:&str,raw_kind:Option<&str>)->Option<ReadType>{
    if let Some(accessor)=profile.accessors.get(name){
        if accessor.has_getter!=Some(false){
            let value_type=accessor.read_type.clone()?;
            return Some(ReadType{nullable:Some(value_type.starts_with('?')||value_type.split('|').any(|kind|kind=="null")),value_type,read_only:raw_kind.is_none()&&accessor.has_getter==Some(true)&&accessor.has_setter==Some(false)});
        }
    }
    if let Some(cast)=facts.casts.iter().find(|cast|cast.name==name){return Some(ReadType{value_type:cast.resolved.clone().or_else(||scalar_cast(&cast.label))?,nullable:None,read_only:false});}
    if !facts.casts_complete{return None;}
    if profile.raw_unknown||profile.dates.is_none()&&profile.possible_dates.contains(name){return None;}
    let value_type=if profile.dates.as_ref().is_some_and(|dates|dates.contains(name)){"Illuminate\\Support\\Carbon".into()}else{match raw_kind?{
        "json"|"jsonb"|"date"|"dateTime"|"dateTimeTz"|"datetime"|"timestamp"|"timestampTz"|"timestamps"|"timestampsTz"|"softDeletes"|"softDeletesTz"|"string"|"char"|"text"|"tinyText"|"mediumText"|"longText"|"uuid"|"ulid"|"decimal"|"unsignedDecimal"=>"string".into(),
        "boolean"=>"bool|int|string".into(),
        "float"|"double"=>"float|string".into(),
        "integer"|"tinyInteger"|"smallInteger"|"mediumInteger"|"bigInteger"|"unsignedInteger"|"unsignedTinyInteger"|"unsignedSmallInteger"|"unsignedMediumInteger"|"unsignedBigInteger"|"foreignId"|"id"|"increments"|"bigIncrements"=>"int|string".into(),
        _=>return None,
    }};
    Some(ReadType{value_type,nullable:None,read_only:false})
}
/// Layer model read semantics over a schema column. Nullability remains unknown
/// unless proven by an accessor signature; consumers retain captured/schema nulls.
pub(crate) fn read_type(index:&Index,full:&str,name:&str,raw_kind:Option<&str>,owns:&dyn Fn(&str)->bool)->Option<ReadType>{
    let facts=helper_facts::facts(index,full,owns)?;let profile=read_profile(index,full,owns)?;read_with(&facts,&profile,name,raw_kind)
}
fn scalar_cast(label:&str)->Option<String>{Some(match label.split(':').next()?{
    "int"|"integer"|"timestamp"=>"int","real"|"float"|"double"=>"float","bool"|"boolean"=>"bool","string"|"decimal"=>"string","array"|"json"=>"array","object"=>"object","date"|"datetime"|"custom_datetime"=>"Illuminate\\Support\\Carbon","immutable_date"|"immutable_datetime"=>"Carbon\\CarbonImmutable",_=>return None,
}.into())}
pub(crate) fn property_class(index:&Index,full:&str,property:&str,owns:&dyn Fn(&str)->bool)->Option<ClassReference>{
    let mut full=full.to_owned();let mut seen=BTreeSet::new();
    for _ in 0..8 {
        if !seen.insert(full.to_ascii_lowercase()){return None;}
        let class=index.eloquent_models.get(&full.to_ascii_lowercase()).filter(|class|!class.ambiguous&&owns(&class.path))?;
        if class.semantic.blocked_properties.contains(property){return class.semantic.properties.get(property).filter(|reference|owns(&reference.entry.path)).cloned();}
        full=class.parent.clone();
    }None
}
/// Exact receiver-resolved member usages in one current buffer. Unsupported
/// dynamic receivers are omitted; this is evidence, not a whole-project promise.
pub(crate) fn member_references(index:&Index,doc:&Document,destination:&Entry,owns:&dyn Fn(&str)->bool)->Option<Vec<Range>>{
    let source=Source::new(&doc.text)?;
    let candidates=source.tokens.iter().enumerate().filter(|(at,token)|*at>0&&token.kind==Kind::Name&&matches!(source.tokens[at-1].value,"->"|"?->"|"::")).collect::<Vec<_>>();
    if candidates.len()>512{return None;}
    let mut result=Vec::new();
    for(_,token)in candidates{
        let position=text::position(&doc.text,token.start);
        if let Some(resolved)=definition_checked(index,doc,position,owns)&&resolved.path==destination.path&&resolved.at==destination.at&&resolved.width==destination.width{
            result.push(Range::new(position,text::position(&doc.text,token.end)));
        }
    }Some(result)
}
/// Resolve the actual receiver of a method-call opening parenthesis. The caller
/// supplies the current document identity and ownership predicate; no PHP or
/// filesystem reads are performed here.
pub(crate) fn receiver_class(index:&Index,doc:&Document,call_open:usize,owns:&dyn Fn(&str)->bool)->Option<String>{
    let source=Source::new(&doc.text)?;
    let open=source.tokens.iter().position(|token|token.start==call_open&&token.value=="(")?;
    let member=source.tokens.get(open.checked_sub(1)?)?;
    let context=context(doc,text::position(&doc.text,member.end))?;
    let mut overlay=super::model(&doc.text,&doc.path);
    if let Some(model)=overlay.as_mut(){let _=traits::compose(model,&index.eloquent_models,&mut BTreeSet::new(),0);}
    let resolver=Resolver{context:&context,index,overlay,object_overlay:objects::current(&doc.text,&doc.path),path:&doc.path,owns,owner_cache:RefCell::new(BTreeMap::new())};
    let receiver=super::receiver(&resolver)?;
    Some(match &receiver.shape{
        Shape::Factory{class,..}|Shape::MappedCollection(class)|Shape::JoinClause(class)=>class.clone(),
        Shape::Query=>resolver.custom_class(&receiver).unwrap_or_else(||"Illuminate\\Database\\Eloquent\\Builder".into()),
        Shape::Collection=>resolver.custom_class(&receiver).unwrap_or_else(||"Illuminate\\Database\\Eloquent\\Collection".into()),
        Shape::Relation(kind) if kind.contains('\\')=>kind.clone(),
        Shape::Relation(kind)=>format!("Illuminate\\Database\\Eloquent\\Relations\\{}{}",kind.chars().next()?.to_uppercase(),&kind[1..]),
        _=>receiver.model,
    })
}
pub(crate) fn receiver_model(index:&Index,doc:&Document,call_open:usize,owns:&dyn Fn(&str)->bool)->Option<String>{
    let source=Source::new(&doc.text)?;
    let open=source.tokens.iter().position(|token|token.start==call_open&&token.value=="(")?;
    let member=source.tokens.get(open.checked_sub(1)?)?;
    let context=context(doc,text::position(&doc.text,member.end))?;
    let mut overlay=super::model(&doc.text,&doc.path);
    if let Some(model)=overlay.as_mut(){let _=traits::compose(model,&index.eloquent_models,&mut BTreeSet::new(),0);}
    let resolver=Resolver{context:&context,index,overlay,object_overlay:objects::current(&doc.text,&doc.path),path:&doc.path,owns,owner_cache:RefCell::new(BTreeMap::new())};
    let receiver=super::receiver(&resolver)?;
    resolver.get(&receiver.model).map(|model|model.full.clone())
}
/// Full name of the owned backed enum the receiver model of the call at `call_open` casts `column` to.
pub(crate) fn column_enum(index:&Index,doc:&Document,call_open:usize,owns:&dyn Fn(&str)->bool,column:&str)->Option<String>{
    let source=Source::new(&doc.text)?;
    let open=source.tokens.iter().position(|token|token.start==call_open&&token.value=="(")?;
    let member=source.tokens.get(open.checked_sub(1)?)?;
    let context=context(doc,text::position(&doc.text,member.end))?;
    let mut overlay=super::model(&doc.text,&doc.path);
    if let Some(model)=overlay.as_mut(){let _=traits::compose(model,&index.eloquent_models,&mut BTreeSet::new(),0);}
    let resolver=Resolver{context:&context,index,overlay,object_overlay:objects::current(&doc.text,&doc.path),path:&doc.path,owns,owner_cache:RefCell::new(BTreeMap::new())};
    let receiver=super::receiver(&resolver)?;
    let model=resolver.get(&receiver.model)?.full.clone();
    let cast=resolver.cast(&model,column)?;
    Some(resolver.enumeration(cast.target.as_deref()?)?.full.clone())
}
/// Source-resolved column receiver for native string-reference consumers.
/// Some(empty) means a proven model has no proven owned table/columns; callers
/// must not replace that with a conventional table-name guess.
pub(crate) fn column_names(index:&Index,doc:&Document,offset:usize,owns:&dyn Fn(&str)->bool)->Option<crate::index::Names>{
    let source=Source::new(&doc.text)?;
    let open=source.tokens.iter().enumerate().filter(|(at,token)|token.value=="("&&token.start<offset&&source.pairs.get(at).is_none_or(|close|source.tokens[*close].start>=offset)&&*at>0&&source.tokens[at-1].kind==Kind::Name).map(|(at,_)|at).max()?;
    let full=receiver_model(index,doc,source.tokens[open].start,owns)?;
    let facts=helper_facts::facts(index,&full,owns)?;
    let qualifier=source.tokens.iter().find(|token|token.kind==Kind::String&&token.start<offset&&offset<token.end).and_then(|token|php::literal(&doc.text,token.start..token.end)).and_then(|(value,_)|value.rsplit_once('.').map(|(prefix,_)|prefix.to_owned())).filter(|prefix|prefix.split('.').all(crate::generator_plan::identifier));
    if let Some(prefix)=&qualifier&&facts.table.as_deref().is_none_or(|table|table!=prefix&&table.rsplit('.').next()!=Some(prefix.as_str())){return Some(BTreeMap::new());}
    Some(facts.table.as_ref().and_then(|table|index.columns.get(table)).map(|columns|columns.iter().filter(|(_,entry)|owns(&entry.path)).map(|(name,entry)|(qualifier.as_ref().map_or_else(||name.clone(),|prefix|format!("{prefix}.{name}")),entry.clone())).collect()).unwrap_or_default())
}

pub(crate) fn receiver_class_checked(index:&Index,doc:&Document,at:Position,owns:&dyn Fn(&str)->bool)->Option<String>{
    let context=context(doc,at)?;let mut overlay=super::model(&doc.text,&doc.path);
    if let Some(model)=overlay.as_mut(){let _=traits::compose(model,&index.eloquent_models,&mut BTreeSet::new(),0);relations::augment(model,&index.eloquent_models);}
    let resolver=Resolver{context:&context,index,overlay,object_overlay:objects::current(&doc.text,&doc.path),path:&doc.path,owns,owner_cache:RefCell::new(BTreeMap::new())};
    let receiver=super::receiver(&resolver)?;
    matches!(receiver.shape,Shape::Model|Shape::Object).then_some(receiver.model)
}
/// Proven member result declarations; for a collection without an owned
/// collection class, its explicitly resolved model element is the destination.
pub(crate) fn type_definition_checked(index:&Index,doc:&Document,at:Position,owns:&dyn Fn(&str)->bool)->Vec<Entry>{
    let Some(context)=context(doc,at)else{return vec![]};
    let Some(member)=context.source.tokens.iter().position(|token|token.start==context.start&&token.end==context.end)else{return vec![]};
    let end=if context.source.is(member+1,"("){let Some(close)=context.source.pairs.get(&(member+1))else{return vec![]};close+1}else{member+1};
    let mut overlay=super::model(&doc.text,&doc.path);
    if let Some(model)=overlay.as_mut(){let _=traits::compose(model,&index.eloquent_models,&mut BTreeSet::new(),0);relations::augment(model,&index.eloquent_models);}
    let resolver=Resolver{context:&context,index,overlay,object_overlay:objects::current(&doc.text,&doc.path),path:&doc.path,owns,owner_cache:RefCell::new(BTreeMap::new())};
    let Some((typed,_))=resolver.expr(end,0)else{return vec![]};
    let full=match &typed.shape{
        Shape::Factory{class,..}|Shape::MappedCollection(class)|Shape::JoinClause(class)=>class.clone(),
        Shape::Query=>resolver.custom_class(&typed).unwrap_or_else(||"Illuminate\\Database\\Eloquent\\Builder".into()),
        Shape::Collection=>resolver.custom_class(&typed).unwrap_or_else(||typed.model.clone()),
        Shape::HigherOrder(..)=>return vec![],
        _=>typed.model,
    };
    if let Some(entry)=resolver.raw(&full).and_then(|model|model.semantic.declaration.as_ref()).filter(|entry|owns(&entry.path)){return vec![entry.clone()];}
    if let Some(info)=resolver.enumeration(&full){return vec![info.declaration.clone()];}
    resolver.object(&full).filter(|object|object.is_interface).map(|object|vec![object.declaration.clone()]).unwrap_or_default()
}

impl Resolver<'_,'_>{
    pub(super) fn construction_override(&self,class:&Model,hook:&str)->bool{
        if class.semantic.source_methods.iter().any(|name|name.eq_ignore_ascii_case(hook))||class.semantic.constructed.contains_key(hook){return true;}
        let native=match hook{"newFactory"|"factory"=>"Illuminate\\Database\\Eloquent\\Factories\\HasFactory","newCollection"=>"Illuminate\\Database\\Eloquent\\HasCollection","newEloquentBuilder"=>"Illuminate\\Database\\Eloquent\\HasBuilder",_=>""};
        class.trait_sources.iter().any(|path|self.index.eloquent_models.values().any(|trait_|trait_.is_trait&&trait_.path==*path&&trait_.full!=native&&trait_.semantic.source_methods.iter().any(|name|name.eq_ignore_ascii_case(hook))))
    }
    pub(super) fn native_factory_method(&self,full:&str)->bool{
        self.lineage(full).is_some_and(|lineage|!lineage.iter().any(|class|self.construction_override(class,"factory")))
    }
    pub(super) fn accessor_type(&self,full:&str,name:&str)->Option<Option<String>>{
        if self.lineage(full)?.iter().any(|class|class.declared.iter().any(|name|["getAttribute","getAttributeValue"].iter().any(|candidate|candidate.eq_ignore_ascii_case(name)))){return Some(None);}
        let Some(contracts)=self.accessor_contracts(full)else{return Some(None)};
        let accessor=contracts.get(name)?;
        if accessor.has_getter==Some(false){return None;}
        Some(accessor.read_type.clone())
    }
    pub(super) fn accessor_result(&self,kind:&str,receiver:&Typed)->Option<Typed>{
        let mut kinds=kind.trim_start_matches('?').split('|').filter(|kind|*kind!="null").collect::<Vec<_>>();
        if kinds.len()!=1{return None;}let kind=kinds.remove(0).split('&').map(|kind|kind.trim_start_matches('\\')).collect::<Vec<_>>().join("&");self.return_target(&kind,receiver)
    }
    pub(super) fn object_method(&self,full:&str,name:&str)->Option<&objects::Method>{
        let methods=self.object_methods(full);
        let found=methods.into_iter().filter(|(candidate,_)|candidate.eq_ignore_ascii_case(name)).map(|(_,method)|method).collect::<Vec<_>>();
        (found.len()==1).then(||found[0])
    }
    pub(super) fn object_methods(&self,full:&str)->BTreeMap<&str,&objects::Method>{
        let mut methods=BTreeMap::new();let mut blocked=BTreeSet::new();let mut seen=BTreeSet::new();let mut current=full;
        for _ in 0..8{
            if !seen.insert(current.to_ascii_lowercase()){return BTreeMap::new();}
            let object=self.object(current);
            let class=self.raw(current).filter(|class|!class.ambiguous);
            if object.is_none()&&class.is_none(){break;}
            if let Some(class)=class{
                // Composed trait aliases/visibility are already represented by
                // the cached Model's selected return and declaration maps.
                for(name,method)in &class.returns{if !blocked.contains(&name.to_ascii_lowercase())&&self.owned(&method.entry.path){methods.entry(name.as_str()).or_insert(method);}}
            }
            if let Some(object)=object{
                for(name,method)in &object.methods{if !blocked.contains(&name.to_ascii_lowercase())&&self.owned(&method.entry.path){methods.entry(name.as_str()).or_insert(method);}}
            }
            let Some(class)=class else{break};
            blocked.extend(class.declared.iter().map(|name|name.to_ascii_lowercase()));
            if class.parent.is_empty(){break;}current=&class.parent;
        }methods
    }
    pub(super) fn external_relations(&self,full:&str)->BTreeMap<&str,&Relation>{
        let mut found=BTreeMap::<&str,Vec<&Relation>>::new();
        for provider in self.index.eloquent_models.values().filter(|class|!class.ambiguous&&!class.is_trait&&self.owned(&class.path)&&self.subclass(&class.full,"Illuminate\\Support\\ServiceProvider")){
            for(owner,name,relation)in &provider.semantic.external_relations{
                if owner.eq_ignore_ascii_case(full)&&!self.declared(full,"resolveRelationUsing")&&!self.declared(full,&relation.kind)&&self.owned(&relation.entry.path)&&self.get(&relation.target).is_some(){found.entry(name.as_str()).or_default().push(relation);}
            }
        }
        found.into_iter().filter_map(|(name,entries)|(entries.len()==1).then(||(name,entries[0]))).collect()
    }
    pub(super) fn factory_count(&self,from:usize,to:usize)->Option<bool>{
        let source=&self.context.source;
        if from==to{return Some(false);}
        let mut end=from;
        while end<to&&!source.is(end,","){
            if let Some(close)=source.pairs.get(&end).filter(|close|**close>end){end=close+1;}else{end+=1;}
        }
        if end==from+1&&source.is(from,"null"){return Some(false);}
        if source.is(from,"["){return Some(false);}
        let tokens=&source.tokens[from..end];
        let first=tokens.first()?;let last=tokens.last()?;
        source.text[first.start..last.end].parse::<u32>().ok().map(|_|true)
    }
    pub(super) fn native_collection_operation(&self,receiver:&Typed,name:&str)->bool{
        fn closed(resolver:&Resolver<'_,'_>,full:&str,name:&str,active:&mut BTreeSet<String>,done:&mut BTreeSet<String>,depth:u8)->bool{
            if depth>8||done.len()>32{return false;}
            if ["Illuminate\\Database\\Eloquent\\Collection","Illuminate\\Support\\Collection"].iter().any(|native|full.eq_ignore_ascii_case(native)){return true;}
            let key=full.to_ascii_lowercase();if done.contains(&key){return true;}if !active.insert(key.clone()){return false;}
            let Some(class)=resolver.raw(full).filter(|class|!class.ambiguous)else{return false};
            if class.declared.iter().any(|method|method.eq_ignore_ascii_case(name))||class.semantic.source_methods.iter().any(|method|method.eq_ignore_ascii_case(name))||class.trait_adaptations{return false;}
            if class.adaptations.iter().any(|rule|match rule{traits::Adaptation::Prefer{method,..}=>method.eq_ignore_ascii_case(name),traits::Adaptation::Alias{method,alias,..}=>method.eq_ignore_ascii_case(name)||alias.as_ref().is_some_and(|alias|alias.eq_ignore_ascii_case(name))}){return false;}
            for trait_ in &class.traits{if !closed(resolver,trait_,name,active,done,depth+1){return false;}}
            if !class.is_trait&&(class.parent.is_empty()||!closed(resolver,&class.parent,name,active,done,depth+1)){return false;}
            active.remove(&key);done.insert(key);true
        }
        self.collection_class(receiver).is_some_and(|class|closed(self,&class,name,&mut BTreeSet::new(),&mut BTreeSet::new(),0))
    }
    pub(super) fn collection_class(&self,receiver:&Typed)->Option<String>{
        match &receiver.shape{
            Shape::MappedCollection(class)=>Some(class.clone()),
            Shape::Collection=>{
                if let Some(class)=self.custom_class(receiver){return Some(class);}
                if self.lineage(&receiver.model)?.iter().any(|class|self.construction_override(class,"newCollection")){return None;}
                Some("Illuminate\\Database\\Eloquent\\Collection".into())
            }
            _=>None,
        }
    }
    pub(super) fn custom_class(&self,receiver:&Typed)->Option<String>{
        let reference=match &receiver.shape{
            Shape::Factory{class,..}|Shape::MappedCollection(class)|Shape::JoinClause(class)=>return Some(class.clone()),
            Shape::Relation(class) if class.contains('\\')=>return self.raw(class).map(|class|class.full.clone()),
            Shape::Query|Shape::Relation(_)=>self.configured(&receiver.model,"builder","Illuminate\\Database\\Eloquent\\Attributes\\UseEloquentBuilder","newEloquentBuilder","Illuminate\\Database\\Eloquent\\Builder"),
            Shape::Collection=>self.configured(&receiver.model,"collectionClass","Illuminate\\Database\\Eloquent\\Attributes\\CollectedBy","newCollection","Illuminate\\Database\\Eloquent\\Collection"),
            _=>None,
        }?;
        Some(reference.full)
    }
    pub(super) fn custom_call(&self,receiver:&Typed,name:&str)->Option<Typed>{
        let full=self.custom_class(receiver)?;
        self.object(&full)?;
        let method=self.object_method(&full,name).filter(|method|!method.static_method&&self.owned(&method.entry.path))?;
        if method.target=="static"||method.target.eq_ignore_ascii_case(&full){return Some(receiver.clone());}
        self.return_target(&method.target,receiver)
    }
    pub(super) fn subclass(&self,full:&str,target:&str)->bool{
        let mut full=full;let mut seen=BTreeSet::new();
        for _ in 0..8{
            if full.eq_ignore_ascii_case(target){return true;}
            if !seen.insert(full.to_ascii_lowercase()){return false;}
            let Some(class)=self.raw(full).filter(|class|!class.ambiguous&&!class.is_trait)else{return false};full=&class.parent;
        }false
    }
    pub(super) fn configured(&self,full:&str,property:&str,attribute:&str,hook:&str,base:&str)->Option<ClassReference>{
        let lineage=self.lineage(full)?;
        // A source override controls the complete construction hook. Dynamic
        // implementations block attribute/property guesses.
        if let Some(class)=lineage.iter().find(|class|class.declared.contains(hook)&&self.construction_override(class,hook)){
            let candidate=class.semantic.constructed.get(hook)?;
            return (self.owned(&candidate.entry.path)&&self.subclass(&candidate.full,base)).then(||candidate.clone());
        }
        let version=self.index.metadata.framework_version;
        let attribute_floor=match property{"factory"=>(11,39,0),"builder"=>(12,19,0),_ =>(11,28,0)};
        let property_floor=match property{"factory"=>(11,39,0),"builder"=>(11,15,0),_=>(11,17,0)};
        let inherited_attribute=property=="collectionClass"&&version.is_some_and(|version|version>=(13,40,0))||property=="builder"&&version.is_some_and(|version|version>=(13,31,0));
        let attrs=lineage.iter().enumerate().filter(|(at,_)|*at==0||inherited_attribute).find_map(|(_,class)|class.semantic.attributes.get(attribute));
        let props=lineage.iter().find(|class|class.semantic.blocked_properties.contains(property));
        let candidate=if property=="factory"&&props.is_some(){
            if !version.is_some_and(|version|version>=property_floor){return None;}
            props?.semantic.properties.get(property)?
        }else if let Some(candidate)=attrs{
            if !version.is_some_and(|version|version>=attribute_floor){return None;}
            candidate
        }else{
            if !version.is_some_and(|version|version>=property_floor){return None;}
            props?.semantic.properties.get(property)?
        };
        (self.owned(&candidate.entry.path)&&self.subclass(&candidate.full,base)).then(||candidate.clone())
    }
    pub(super) fn factory_model(&self,factory:&str)->Option<String>{
        if !self.subclass(factory,"Illuminate\\Database\\Eloquent\\Factories\\Factory"){return None;}
        let mut current=factory;
        for _ in 0..8{
            let class=self.raw(current)?;
            if class.semantic.blocked_properties.contains("model"){
                let reference=class.semantic.properties.get("model")?;
                return self.get(&reference.full).map(|model|model.full.clone());
            }
            if let Some(reference)=&class.semantic.factory_generic {return self.get(&reference.full).map(|model|model.full.clone());}
            current=&class.parent;
        }None
    }
    pub(super) fn model_factory(&self,full:&str)->Option<ClassReference>{
        if !self.lineage(full)?.iter().any(|class|class.traits.iter().any(|name|name=="Illuminate\\Database\\Eloquent\\Factories\\HasFactory")){return None;}
        if let Some(reference)=self.configured(full,"factory","Illuminate\\Database\\Eloquent\\Attributes\\UseFactory","newFactory","Illuminate\\Database\\Eloquent\\Factories\\Factory"){return Some(reference);}
        let lineage=self.lineage(full)?;
        if lineage.iter().any(|class|class.declared.contains("newFactory")&&self.construction_override(class,"newFactory")||class.semantic.blocked_properties.contains("factory")){return None;}
        if !lineage.iter().any(|class|class.traits.iter().any(|name|name=="Illuminate\\Database\\Eloquent\\Factories\\HasFactory")){return None;}
        let tail=full.strip_prefix("App\\Models\\")?;
        let conventional=format!("Database\\Factories\\{tail}Factory");
        let found=self.index.eloquent_models.values().filter(|class|class.full==conventional&&!class.ambiguous&&!class.is_abstract&&self.owned(&class.path)&&self.factory_model(&class.full).as_deref()==Some(full)).collect::<Vec<_>>();
        let [factory]=found.as_slice()else{return None};
        Some(ClassReference{full:factory.full.clone(),entry:factory.semantic.properties.get("model").or(factory.semantic.factory_generic.as_ref())?.entry.clone()})
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    fn index()->Index{
        let mut index=Index::default();index.metadata.framework_version=Some((12,20,0));
        for(path,source)in [
            ("app/Models/Post.php",r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model;use Illuminate\Database\Eloquent\Factories\HasFactory;class Post extends Model {use HasFactory;public function newEloquentBuilder($query){return new \App\Queries\Posts($query);}public function newCollection(array $models=[]){return new \App\Collections\Posts($models);}}"#),
            ("app/Queries/Posts.php",r#"<?php namespace App\Queries;class Posts extends \Illuminate\Database\Eloquent\Builder {public function published(): static{return $this;}public function model(): \App\Models\Post{return new \App\Models\Post;}}"#),
            ("app/Collections/Posts.php",r#"<?php namespace App\Collections;class Posts extends \Illuminate\Database\Eloquent\Collection {public function reviewed():static{return $this;}public function selected():\App\Models\Post{return new \App\Models\Post;}}"#),
            ("database/factories/PostFactory.php",r#"<?php namespace Database\Factories;/** @extends \Illuminate\Database\Eloquent\Factories\Factory<\App\Models\Post> */ class PostFactory extends \Illuminate\Database\Eloquent\Factories\Factory {public function reviewed():static{return $this;}}"#),
        ]{
            let path=format!("/project/{path}");cache(&mut index.eloquent_models,source,&path);cache_object(&mut index.eloquent_objects,source,&path);
        }
        index.columns.entry("posts".into()).or_default().insert("title".into(),Entry{path:"/project/database/migrations/posts.php".into(),at:Position::new(0,0),width:5,detail:Some("string".into())});
        finish(&mut index.eloquent_models);index
    }
    fn labels(index:&Index,source:&str)->Vec<String>{
        let(text,at)=crate::tests::cursor(source);completion_checked(index,&crate::tests::doc("/project/Test.php","php",&text),at,&|_|true).into_iter().map(|item|item.label).collect()
    }
    #[test]
    fn factory_results_preserve_single_multiple_and_custom_states_without_php(){
        let index=index();
        for source in [r#"<?php \App\Models\Post::factory()->create()->^"#,r#"<?php \App\Models\Post::factory()->reviewed()->make()->^"#,r#"<?php \Database\Factories\PostFactory::new()->makeOne()->^"#]{assert!(labels(&index,source).contains(&"title".into()),"{source}");}
        for source in [r#"<?php \App\Models\Post::factory(3)->create()->^"#,r#"<?php \App\Models\Post::factory()->count(1)->make()->^"#,r#"<?php \Database\Factories\PostFactory::new()->createMany([])->^"#]{let names=labels(&index,source);assert!(names.contains(&"first()".into()),"{source}: {names:?}");assert!(!names.contains(&"title".into()));}
        assert!(labels(&index,r#"<?php \App\Models\Post::factory($unknown)->make()->^"#).is_empty());
        assert!(labels(&index,r#"<?php \App\Models\Post::factory()->^"#).contains(&"reviewed()".into()));
    }
    #[test]
    fn custom_builders_collections_and_navigation_keep_model_shape(){
        let index=index();
        for source in [r#"<?php \App\Models\Post::query()->published()->first()->^"#,r#"<?php \App\Models\Post::all()->reviewed()->selected()->^"#,r#"<?php \App\Models\Post::query()->model()->^"#]{assert!(labels(&index,source).contains(&"title".into()),"{source}");}
        assert!(labels(&index,r#"<?php \App\Models\Post::query()->^"#).contains(&"published()".into()));
        assert!(labels(&index,r#"<?php \App\Models\Post::all()->^"#).contains(&"reviewed()".into()));
        let(text,at)=crate::tests::cursor(r#"<?php \App\Models\Post::query()->publ^ished()"#);
        let doc=crate::tests::doc("/project/Test.php","php",&text);
        assert_eq!(definition_checked(&index,&doc,at,&|_|true).unwrap().path,"/project/app/Queries/Posts.php");
        assert!(completion_checked(&index,&doc,at,&|path|!path.contains("/Queries/")).is_empty());
    }
    #[test]
    fn shared_model_facts_use_explicit_table_and_literal_class_properties(){
        let mut index=index();let source=r#"<?php namespace App\Models; class Other extends \Illuminate\Database\Eloquent\Model{protected $table='posts';protected $model=Post::class;protected $casts=['active'=>'boolean'];}"#;
        cache(&mut index.eloquent_models,source,"/project/app/Models/Other.php");
        let facts=model_facts(&index,"App\\Models\\Other",&|_|true).unwrap();
        assert_eq!(facts.table.as_deref(),Some("posts"));assert!(facts.fields.iter().any(|field|field.name=="title"));assert!(facts.fields.iter().any(|field|field.name=="active"&&field.value_type.as_deref()==Some("bool")));
        assert_eq!(property_class(&index,"App\\Models\\Other","model",&|_|true).unwrap().full,"App\\Models\\Post");
        assert!(model_facts(&index,"App\\Models\\Other",&|path|!path.contains("Other.php")).is_none());
    }
    #[test]
    fn provider_external_relations_are_owned_unambiguous_and_navigate_registration(){
        let mut index=index();let source=r#"<?php namespace App\Providers;use App\Models\Post;class Relations extends \Illuminate\Support\ServiceProvider {public function boot(){Post::resolveRelationUsing('featured',function($post){return $post->hasOne(Post::class);});if(config('dynamic')){Post::resolveRelationUsing('conditional',function($post){return $post->hasMany(Post::class);});}}}"#;
        cache(&mut index.eloquent_models,source,"/project/app/Providers/Relations.php");
        assert!(labels(&index,r#"<?php \App\Models\Post::first()->featured->^"#).contains(&"title".into()));
        assert!(!labels(&index,r#"<?php \App\Models\Post::first()->^"#).contains(&"conditional".into()));
        let(text,at)=crate::tests::cursor(r#"<?php \App\Models\Post::first()->feat^ured"#);let doc=crate::tests::doc("/project/Test.php","php",&text);
        let entry=definition_checked(&index,&doc,at,&|_|true).unwrap();assert_eq!(entry.path,"/project/app/Providers/Relations.php");
        assert_eq!(&source[text::offset(source,entry.at).unwrap()..][..entry.width as usize],"featured");
        assert!(completion_checked(&index,&doc,at,&|path|!path.contains("/Providers/")).is_empty());
        cache(&mut index.eloquent_models,&source.replace("class Relations","class Duplicate"),"/project/app/Providers/Duplicate.php");
        assert!(labels(&index,r#"<?php \App\Models\Post::first()->featured->^"#).is_empty());
    }
    #[test]
    fn arrow_external_relations_are_owned_unconditional_and_reject_overridden_factories(){
        let mut index=index();let source=r#"<?php namespace App\Providers;use App\Models\Post;class ArrowRelations extends \Illuminate\Support\ServiceProvider{public function boot(){Post::resolveRelationUsing('featured',static fn($post)=>$post->hasOne(Post::class));Post::resolveRelationUsing('related',fn($post)=>$post->hasMany(Post::class));if(config('dynamic'))Post::resolveRelationUsing('conditional',fn($post)=>$post->hasOne(Post::class));}}"#;
        cache(&mut index.eloquent_models,source,"/project/app/Providers/ArrowRelations.php");
        for source in [r#"<?php \App\Models\Post::first()->featured->tit^le"#,r#"<?php \App\Models\Post::first()->related->first()->tit^le"#]{assert!(labels(&index,source).contains(&"title".into()),"{source}");}
        assert!(!labels(&index,r#"<?php \App\Models\Post::first()->^"#).contains(&"conditional".into()));
        let(text,at)=crate::tests::cursor(r#"<?php \App\Models\Post::first()->feat^ured"#);let doc=crate::tests::doc("/project/Usage.php","php",&text);let entry=definition_checked(&index,&doc,at,&|_|true).unwrap();assert_eq!(entry.path,"/project/app/Providers/ArrowRelations.php");assert_eq!(entry.width,8);
        assert!(completion_checked(&index,&doc,at,&|path|!path.contains("Providers")).is_empty());
        let post=r#"<?php namespace App\Models;class Post extends \Illuminate\Database\Eloquent\Model{public function hasOne($target){return null;}}"#;index.eloquent_models.remove("app\\models\\post");cache(&mut index.eloquent_models,post,"/project/app/Models/Post.php");assert!(labels(&index,r#"<?php \App\Models\Post::first()->featured->tit^le"#).is_empty());
    }
    #[test]
    fn registered_external_arrow_provider_refreshes_target_cardinality_without_php(){
        use openidea_plugin::{Plugin,providers::Providers,Event,mock::MockHost};let host=MockHost::install();host.add_file("artisan","<?php");host.add_file("composer.json",r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#);
        host.add_file("app/Models/Post.php",r#"<?php namespace App\Models;class Post extends \Illuminate\Database\Eloquent\Model{}"#);host.add_file("app/Models/User.php",r#"<?php namespace App\Models;class User extends \Illuminate\Database\Eloquent\Model{}"#);
        let provider=r#"<?php namespace App\Providers;use App\Models\Post;use App\Models\User;class Relations extends \Illuminate\Support\ServiceProvider{public function boot(){Post::resolveRelationUsing('featured',static fn($post)=>$post->hasOne(User::class));}}"#;host.add_file("app/Providers/Relations.php",provider);host.add_file("database/migrations/users.php","<?php Schema::create('users',function($t){$t->string('email');});");
        let mut plugin=crate::Main::default();plugin.activate(&host.context("aritti.laravel")).unwrap();crate::tests::finish_indexing(&mut plugin);
        let(text,at)=crate::tests::cursor(r#"<?php \App\Models\Post::first()->featured->em^ail"#);let doc=crate::tests::doc("/project/Usage.php","php",&text);assert!(plugin.completion(&doc,at,None).iter().any(|item|item.label=="email"));assert_eq!(plugin.definition(&doc,at)[0].path,"/project/database/migrations/users.php");
        host.add_file("app/Providers/Relations.php",&provider.replace("hasOne","hasMany"));plugin.handle_events(&[Event::FileSaved("/project/app/Providers/Relations.php".into())],&host.invocation());crate::tests::finish_indexing(&mut plugin);assert!(plugin.completion(&doc,at,None).is_empty());
        let(text,at)=crate::tests::cursor(r#"<?php \App\Models\Post::first()->featured->first()->em^ail"#);let doc=crate::tests::doc("/project/Usage.php","php",&text);assert!(plugin.completion(&doc,at,None).iter().any(|item|item.label=="email"));
        assert!(host.process_runs().is_empty());assert!(host.requests().is_empty());
    }
    #[test]
    fn native_instance_and_collection_generics_follow_proven_receiver_models(){
        let index=index();
        for source in [r#"<?php (new \App\Models\Post)->where('active',true)->first()->tit^le"#,r#"<?php (new \App\Models\Post)->newQuery()->published()->first()->tit^le"#,r#"<?php (new \App\Models\Post)->newInstance()->tit^le"#,r#"<?php (new \App\Models\Post)->newFromBuilder([])->tit^le"#,r#"<?php \App\Models\Post::query()->newModelInstance()->tit^le"#,r#"<?php (new \App\Models\Post)->newCollection()->reviewed()->first()->tit^le"#,r#"<?php (new \App\Models\Post)->newCollection([new \App\Models\Post])->first()->tit^le"#]{assert!(labels(&index,source).contains(&"title".into()),"{source}");}
        assert!(labels(&index,r#"<?php (new \App\Models\Post)->newCollection($unknown)->first()->tit^le"#).is_empty());
    }
    #[test]
    fn instance_scope_forwarding_respects_php_method_case_and_magic_override_boundaries(){
        let mut index=index();
        cache(&mut index.eloquent_models,r#"<?php namespace App\Models;class ScopedPost extends \Illuminate\Database\Eloquent\Model{protected $table='posts';public function scopeActive($query){return $query;}}"#,"/project/app/Models/ScopedPost.php");
        assert!(labels(&index,r#"<?php (new \App\Models\ScopedPost)->active()->first()->tit^le"#).contains(&"title".into()));
        for(name,method)in [("MagicPost","public function __call($name,$arguments){return null;}"),("CasePost","public function WHERE($key,$value=null){return null;}")]{
            let source=format!("<?php namespace App\\Models;class {name} extends \\Illuminate\\Database\\Eloquent\\Model{{protected $table='posts';{method}}}");cache(&mut index.eloquent_models,&source,&format!("/project/app/Models/{name}.php"));
            assert!(labels(&index,&format!("<?php (new \\App\\Models\\{name})->where('active',true)->first()->tit^le")).is_empty());
        }
        assert!(labels(&index,r#"<?php namespace App;function optional($value){return null;}\optional(\App\Models\Post::first())->tit^le"#).contains(&"title".into()));
        assert!(labels(&index,r#"<?php namespace App;optional(\App\Models\Post::first())->tit^le"#).contains(&"title".into()));
    }
    #[test]
    fn object_interfaces_static_donors_and_inherited_traits_preserve_original_targets(){
        let mut index=index();
        for (path,source)in [
            ("app/Contracts/Posts.php",r#"<?php namespace App\Contracts;interface Posts {public function selected(): \App\Models\Post;}"#),
            ("app/Services/BasePosts.php",r#"<?php namespace App\Services;class BasePosts {public static function selected():\App\Models\Post{return new \App\Models\Post;}public function first():\App\Models\Post{return new \App\Models\Post;}}"#),
            ("app/Services/Posts.php",r#"<?php namespace App\Services;class Posts extends BasePosts {}"#),
            ("app/Traits/Selects.php",r#"<?php namespace App\Traits;trait Selects {public function selected():\App\Models\Post{return new \App\Models\Post;}}"#),
            ("app/Services/Selects.php",r#"<?php namespace App\Services;class Selects {use \App\Traits\Selects {selected as pick;}}"#),
        ]{let path=format!("/project/{path}");cache(&mut index.eloquent_models,source,&path);cache_object(&mut index.eloquent_objects,source,&path);}
        finish(&mut index.eloquent_models);
        for source in [r#"<?php function show(\App\Contracts\Posts $posts){$posts->selected()->^;}"#,r#"<?php \App\Services\Posts::selected()->^"#,r#"<?php (new \App\Services\Posts)->first()->^"#,r#"<?php (new \App\Services\Selects)->pick()->^"#]{assert!(labels(&index,source).contains(&"title".into()),"{source}");}
        let(text,at)=crate::tests::cursor(r#"<?php \App\Services\Posts::sel^ected()"#);let doc=crate::tests::doc("/project/Test.php","php",&text);
        assert_eq!(definition_checked(&index,&doc,at,&|_|true).unwrap().path,"/project/app/Services/BasePosts.php");
    }
    #[test]
    fn registered_provider_factory_chains_refresh_and_do_not_execute_php(){
        use openidea_plugin::{Plugin,providers::Providers,Event,mock::MockHost};
        let host=MockHost::install();host.add_file("artisan","<?php");
        host.add_file("composer.json",r#"{"require":{"laravel/framework":"^12.0"},"autoload":{"psr-4":{"App\\":"app/","Database\\Factories\\":"database/factories/"}}}"#);
        host.add_file("app/Models/Post.php",r#"<?php namespace App\Models;class Post extends \Illuminate\Database\Eloquent\Model {use \Illuminate\Database\Eloquent\Factories\HasFactory;}"#);
        let factory=r#"<?php namespace Database\Factories;class PostFactory extends \Illuminate\Database\Eloquent\Factories\Factory {protected $model=\App\Models\Post::class;}"#;
        host.add_file("database/factories/PostFactory.php",factory);
        host.add_file("database/migrations/posts.php","<?php Schema::create('posts',function($table){$table->string('title');});");
        let mut plugin=crate::Main::default();plugin.activate(&host.context("aritti.laravel")).unwrap();crate::tests::finish_indexing(&mut plugin);
        let(text,at)=crate::tests::cursor(r#"<?php \App\Models\Post::factory(2)->make()->first()->tit^le"#);let doc=crate::tests::doc("/project/Test.php","php",&text);
        assert!(plugin.completion(&doc,at,None).iter().any(|item|item.label=="title"));assert_eq!(plugin.definition(&doc,at)[0].path,"/project/database/migrations/posts.php");
        host.add_file("database/factories/PostFactory.php",&factory.replace("$model=\\App\\Models\\Post::class","$model=config('dynamic')"));
        plugin.handle_events(&[Event::FileSaved("/project/database/factories/PostFactory.php".into())],&host.invocation());crate::tests::finish_indexing(&mut plugin);
        assert!(plugin.completion(&doc,at,None).is_empty());
        assert!(host.process_runs().is_empty());assert!(host.requests().is_empty());
    }
    #[test]
    fn closed_member_families_include_parent_interface_overrides_and_check_descendant_collisions(){
        let mut index=index();
        for(path,source)in [
            ("app/Contracts/Sends.php",r#"<?php namespace App\Contracts;interface Sends{public function send():void;}"#),
            ("app/Services/BaseService.php",r#"<?php namespace App\Services;class BaseService implements \App\Contracts\Sends {public function send():void{}}"#),
            ("app/Services/ChildService.php",r#"<?php namespace App\Services;class ChildService extends BaseService {public function send():void{}public function occupied():void{}}"#),
        ]{let path=format!("/project/{path}");cache(&mut index.eloquent_models,source,&path);cache_object(&mut index.eloquent_objects,source,&path);cache_metadata(&mut index.metadata,source,&path);}
        let destination=index.eloquent_models["app\\services\\baseservice"].returns["send"].entry.clone();
        let doc=crate::tests::doc("/project/Usage.php","php","<?php");
        let family=member_family(&index,&doc,&destination,&|_|true).unwrap();assert_eq!(family.len(),3,"{family:?}");
        assert_eq!(member_rename_collision(&index,&doc,&destination,"OCCUPIED",&|_|true),Some(true));assert_eq!(member_rename_collision(&index,&doc,&destination,"dispatch",&|_|true),Some(false));
        assert!(member_family(&index,&doc,&destination,&|path|!path.contains("/Contracts/")).is_none());
    }
    #[test]
    fn read_types_distinguish_uncast_database_strings_from_casts_accessors_and_proven_dates(){
        let mut index=index();let source=r#"<?php namespace App\Models;class ReadPost extends \Illuminate\Database\Eloquent\Model{protected $casts=['options'=>'array','price'=>'decimal:2'];public function getTitleAttribute():?string{return null;}}"#;
        cache(&mut index.eloquent_models,source,"/project/app/Models/ReadPost.php");
        for(name,raw,expected)in [("options","json","array"),("price","decimal","string"),("payload","json","string"),("published_at","timestamp","string"),("created_at","timestamp","Illuminate\\Support\\Carbon"),("title","string","?string")]{assert_eq!(read_type(&index,"App\\Models\\ReadPost",name,Some(raw),&|_|true).unwrap().value_type,expected,"{name}");}
        let getter=read_type(&index,"App\\Models\\ReadPost","title",Some("string"),&|_|true).unwrap();assert_eq!(getter.nullable,Some(true));assert!(!getter.read_only);
        index.metadata.framework_version=None;assert!(read_type(&index,"App\\Models\\ReadPost","created_at",Some("timestamp"),&|_|true).is_none());
        assert_eq!(read_type(&index,"App\\Models\\ReadPost","options",Some("json"),&|_|true).unwrap().value_type,"array");
    }
    #[test]
    fn modern_accessor_contracts_keep_independent_reads_writes_and_owned_destinations(){
        let mut index=index();
        let source=r#"<?php namespace App\Models;use Illuminate\Database\Eloquent\Casts\Attribute;class ModernPost extends \Illuminate\Database\Eloquent\Model{
            protected function label():Attribute{return Attribute::get(fn():?string=>null);}
            protected function secret():Attribute{return Attribute::set(fn(string $value)=>$value);}
            protected function related():Attribute{return Attribute::make(get:fn():Post=>new Post,set:fn(Post $value)=>$value);}
            protected function stored():Attribute{return Attribute::get(fn():string=>'x');}
        }"#;
        cache(&mut index.eloquent_models,source,"/project/app/Models/ModernPost.php");
        index.columns.entry("modern_posts".into()).or_default().insert("stored".into(),Entry{path:"/project/database/migrations/posts.php".into(),at:Position::new(0,0),width:6,detail:Some("string".into())});
        let accessors=accessor_facts(&index,"App\\Models\\ModernPost",&|_|true).unwrap();
        let label=accessors.iter().find(|a|a.name=="label").unwrap();assert_eq!(label.read_type.as_deref(),Some("?string"));assert_eq!(label.has_setter,Some(false));
        let secret=accessors.iter().find(|a|a.name=="secret").unwrap();assert_eq!(secret.write_type.as_deref(),Some("string"));assert_eq!(secret.has_getter,Some(false));
        let facts=model_facts(&index,"App\\Models\\ModernPost",&|_|true).unwrap();assert!(facts.fields.iter().find(|f|f.name=="label").unwrap().read_only);assert!(!facts.fields.iter().find(|f|f.name=="stored").unwrap().read_only);
        assert!(labels(&index,r#"<?php \App\Models\ModernPost::first()->rel^ated"#).contains(&"related".into()));
        assert!(labels(&index,r#"<?php \App\Models\ModernPost::first()->related->tit^le"#).contains(&"title".into()));
        let(text,at)=crate::tests::cursor(r#"<?php \App\Models\ModernPost::first()->rel^ated"#);let doc=crate::tests::doc("/project/Test.php","php",&text);assert_eq!(definition_checked(&index,&doc,at,&|_|true).unwrap().path,"/project/app/Models/ModernPost.php");
        assert!(accessor_facts(&index,"App\\Models\\ModernPost",&|path|!path.ends_with("ModernPost.php")).is_none());
    }
    #[test]
    fn conditionable_optional_and_keyed_eager_callbacks_preserve_only_proven_model_flow(){
        let mut index=index();
        let source=r#"<?php namespace App\Models;class User extends \Illuminate\Database\Eloquent\Model{public function posts(){return $this->hasMany(Post::class);}public function followers(){return $this->hasMany(User::class);}}"#;
        cache(&mut index.eloquent_models,source,"/project/app/Models/User.php");finish(&mut index.eloquent_models);
        for source in [
            r#"<?php \App\Models\Post::query()->when(true,fn($q)=>$q->published())->first()->tit^le"#,
            r#"<?php \App\Models\Post::query()->unless(false,fn($q)=>$q->first())->tit^le"#,
            r#"<?php \App\Models\Post::query()->when(true)->published()->first()->tit^le"#,
            r#"<?php \App\Models\Post::query()->when()->exists()->published()->first()->tit^le"#,
            r#"<?php \App\Models\Post::factory()->when(true,fn($factory)=>$factory->count(1))->make()->first()->tit^le"#,
            r#"<?php \App\Models\Post::query()->when($flag,function($q){$q->published();})->first()->tit^le"#,
            r#"<?php optional(\App\Models\Post::first())->fresh()->tit^le"#,
            r#"<?php optional(\App\Models\Post::first(),fn($post)=>$post->fresh())->tit^le"#,
            r#"<?php optional(\App\Models\Post::first(),fn($post)=>$post->tit^le)"#,
            r#"<?php \App\Models\User::with(['posts'=>fn($query)=>$query->first()->tit^le]);"#,
            r#"<?php \App\Models\User::with(['followers'=>['posts'=>fn($query)=>$query->first()->tit^le]]);"#,
            r#"<?php \App\Models\User::with(['posts'=>fn(\Illuminate\Database\Eloquent\Builder $query)=>$query->first()->tit^le]);"#,
            r#"<?php \App\Models\User::first()->load(['posts'=>function($query){$query->first()->tit^le;}]);"#,
        ]{assert!(labels(&index,source).contains(&"title".into()),"{source}");}
        for source in [r#"<?php \App\Models\Post::query()->when(true,function($q){if($flag)return $q->first();})->tit^le"#,r#"<?php \App\Models\Post::query()->when($flag,fn($q)=>$q->first())->tit^le"#,r#"<?php use function Other\optional;optional(\App\Models\Post::first())->tit^le"#,r#"<?php \App\Models\User::with(['missing'=>fn($query)=>$query->first()->tit^le]);"#]{assert!(labels(&index,source).is_empty(),"{source}");}
    }
    #[test]
    fn source_declared_collection_callback_results_keep_model_items_and_reject_scalars(){
        let mut index=index();
        for(path,source)in [
            ("/project/app/Models/Row.php",r#"<?php namespace App\Models;class Row extends \Illuminate\Database\Eloquent\Model{public function posts(){return $this->hasMany(OtherRow::class);}}"#),
            ("/project/app/Models/OtherRow.php",r#"<?php namespace App\Models;class OtherRow extends \Illuminate\Database\Eloquent\Model{}"#),
        ]{cache(&mut index.eloquent_models,source,path);}
        index.columns.insert("other_rows".into(),index.columns["posts"].clone());finish(&mut index.eloquent_models);
        for source in [r#"<?php \App\Models\Post::all()->map(fn($post)=>$post->fresh())->first()->tit^le"#,r#"<?php \App\Models\Row::all()->flatMap(fn($row)=>$row->posts)->first()->tit^le"#,r#"<?php \App\Models\Row::all()->map(fn($row)=>new \App\Models\OtherRow)->first()->tit^le"#]{assert!(labels(&index,source).contains(&"title".into()),"{source}");}
        assert!(labels(&index,r#"<?php \App\Models\Post::all()->map(fn($post)=>'scalar')->first()->tit^le"#).is_empty());
    }
    #[test]
    fn mapped_model_generics_preserve_receiver_collection_and_flatmap_base_class(){
        let mut index=index();
        for(path,source)in [
            ("/project/app/Models/User.php",r#"<?php namespace App\Models;class User extends \Illuminate\Database\Eloquent\Model{public function newCollection(array $models=[]){return new \App\Collections\Users($models);}}"#),
            ("/project/app/Collections/Users.php",r#"<?php namespace App\Collections;class Users extends \Illuminate\Database\Eloquent\Collection{public function userOnly():static{return $this;}}"#),
        ]{cache(&mut index.eloquent_models,source,path);cache_object(&mut index.eloquent_objects,source,path);}
        index.columns.entry("users".into()).or_default().insert("email".into(),Entry{path:"/project/database/migrations/users.php".into(),at:Position::new(0,0),width:5,detail:Some("string".into())});finish(&mut index.eloquent_models);
        for source in [r#"<?php \App\Models\Post::all()->map(fn($post)=>new \App\Models\User)->first()->em^ail"#,r#"<?php \App\Models\Post::all()->map(fn($post)=>new \App\Models\User)->reviewed()->first()->em^ail"#,r#"<?php \App\Models\Post::all()->flatMap(fn($post)=>\App\Models\User::all())->first()->em^ail"#,r#"<?php \App\Models\Post::all()->flatMap(fn($post)=>\App\Models\User::all())->map(fn($user)=>$user)->first()->em^ail"#]{assert!(labels(&index,source).contains(&"email".into()),"{source}");}
        for source in [r#"<?php \App\Models\Post::all()->map(fn($post)=>new \App\Models\User)->userOnly()->first()->em^ail"#,r#"<?php \App\Models\Post::all()->flatMap(fn($post)=>\App\Models\User::all())->reviewed()->first()->em^ail"#]{assert!(labels(&index,source).is_empty(),"{source}");}
        for(source,expected)in [(r#"<?php \App\Models\Post::all()->map(fn($post)=>new \App\Models\User)->fir^st()"#,"App\\Collections\\Posts"),(r#"<?php \App\Models\Post::all()->flatMap(fn($post)=>\App\Models\User::all())->fir^st()"#,"Illuminate\\Support\\Collection")]{let(text,at)=crate::tests::cursor(source);let doc=crate::tests::doc("/project/Usage.php","php",&text);let open=text[text::offset(&text,at).unwrap()..].find('(').unwrap()+text::offset(&text,at).unwrap();assert_eq!(receiver_class(&index,&doc,open,&|_|true).as_deref(),Some(expected));}
    }
    #[test]
    fn registered_modern_accessors_and_eager_conditionals_refresh_from_owned_sources_without_php(){
        use openidea_plugin::{Plugin,providers::Providers,Event,mock::MockHost};
        let host=MockHost::install();host.add_file("artisan","<?php");
        host.add_file("composer.json",r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#);
        let post=r#"<?php namespace App\Models;use Illuminate\Database\Eloquent\Casts\Attribute;class Post extends \Illuminate\Database\Eloquent\Model{protected function featured():Attribute{return Attribute::get(fn():User=>new User);}}"#;
        host.add_file("app/Models/Post.php",post);
        host.add_file("app/Models/User.php",r#"<?php namespace App\Models;class User extends \Illuminate\Database\Eloquent\Model{public function posts(){return $this->hasMany(Post::class);}}"#);
        host.add_file("database/migrations/posts.php","<?php Schema::create('posts',function($t){$t->string('title');});Schema::create('users',function($t){$t->string('email');});");
        let mut plugin=crate::Main::default();plugin.activate(&host.context("aritti.laravel")).unwrap();crate::tests::finish_indexing(&mut plugin);
        for source in [r#"<?php \App\Models\Post::first()->featured->em^ail"#,r#"<?php \App\Models\User::with(['posts'=>fn($q)=>$q->when(true,fn($query)=>$query)->first()->tit^le]);"#]{
            let(text,at)=crate::tests::cursor(source);let doc=crate::tests::doc("/project/Usage.php","php",&text);assert!(!plugin.completion(&doc,at,None).is_empty(),"{source}");assert!(!plugin.definition(&doc,at).is_empty(),"{source}");
        }
        host.add_file("app/Models/Post.php",&post.replace("fn():User=>new User","fn():string=>'unknown'"));plugin.handle_events(&[Event::FileSaved("/project/app/Models/Post.php".into())],&host.invocation());crate::tests::finish_indexing(&mut plugin);
        let(text,at)=crate::tests::cursor(r#"<?php \App\Models\Post::first()->featured->em^ail"#);let doc=crate::tests::doc("/project/Usage.php","php",&text);assert!(plugin.completion(&doc,at,None).is_empty());assert!(plugin.definition(&doc,at).is_empty());
        assert!(host.process_runs().is_empty());assert!(host.requests().is_empty());
    }
    #[test]
    fn member_named_namespace_does_not_change_php_declaration_identity(){
        let source=Source::new(r#"<?php namespace App;use Illuminate\Support\Facades\Route as Router;Router::prefix('v2')->namespace('App\Http')->group('routes.php');class Example{public function namespace(){}}"#).unwrap();
        let names=Names::parse(&source).unwrap();assert_eq!(names.namespace,"App");assert_eq!(names.class.unwrap().0,"App\\Example");
    }
    #[test]
    fn literal_bindings_aliases_facades_and_contextual_constructor_targets_are_proven(){
        let mut index=index();
        for(path,source)in [
            ("app/Services/Posts.php",r#"<?php namespace App\Services;class Posts {public function selected():\App\Models\Post{return new \App\Models\Post;}}"#),
            ("app/Contracts/Posts.php",r#"<?php namespace App\Contracts;interface Posts {public function contractOnly():void;}"#),
            ("app/Facades/Posts.php",r#"<?php namespace App\Facades;class Posts extends \Illuminate\Support\Facades\Facade {protected static function getFacadeAccessor(){return 'posts.service';}}"#),
            ("app/Providers/Bindings.php",r#"<?php namespace App\Providers;class Bindings extends \Illuminate\Support\ServiceProvider{public function register(){$this->app->singleton('posts.service',\App\Services\Posts::class);$this->app->alias('posts.service','posts.alias');$this->app->when(\App\Http\Consumer::class)->needs(\App\Contracts\Posts::class)->give(\App\Services\Posts::class);}}"#),
        ]{let path=format!("/project/{path}");cache(&mut index.eloquent_models,source,&path);cache_object(&mut index.eloquent_objects,source,&path);}
        for source in [r#"<?php app('posts.alias')->selected()->^"#,r#"<?php \resolve('posts.service')->selected()->^"#,r#"<?php \App\Facades\Posts::selected()->^"#,r#"<?php \Facades\App\Services\Posts::selected()->^"#,r#"<?php namespace App\Http;class Consumer {public function __construct(\App\Contracts\Posts $posts){$posts->selected()->^;}}"#]{assert!(labels(&index,source).contains(&"title".into()),"{source}");}
        assert!(labels(&index,r#"<?php use function Foreign\app;app('posts.service')->selected()->^"#).is_empty());
        assert!(labels(&index,r#"<?php function app($key){return null;}app('posts.service')->selected()->^"#).is_empty());
    }
    #[test]
    fn framework_closures_arrow_callbacks_and_higher_order_proxies_keep_model_generics(){
        let index=index();
        for source in [r#"<?php \App\Models\Post::query()->where(function($query){$query->published()->first()->tit^le;});"#,r#"<?php \App\Models\Post::all()->map(fn($post)=>$post->tit^le);"#,r#"<?php \App\Models\Post::all()->each(function($post){$post->tit^le;});"#,r#"<?php \App\Models\Post::all()->map->fresh()->first()->tit^le"#,r#"<?php \App\Models\Post::all()->each->refresh()->first()->tit^le"#]{assert!(labels(&index,source).contains(&"title".into()),"{source}");}
        assert!(labels(&index,r#"<?php \App\Models\Post::all()->map(fn(\Other\Post $post)=>$post->tit^le);"#).is_empty());
        assert!(labels(&index,r#"<?php unknown(fn($post)=>$post->tit^le);"#).is_empty());
    }
    #[test]
    fn fluent_through_and_declared_polymorphic_generics_share_owned_relation_facts(){
        let mut index=index();
        for(path,source)in [
            ("app/Models/Car.php",r#"<?php namespace App\Models;class Car extends \Illuminate\Database\Eloquent\Model{public function owner(){return $this->hasOne(Post::class);}}"#),
            ("app/Models/Mechanic.php",r#"<?php namespace App\Models;class Mechanic extends \Illuminate\Database\Eloquent\Model{public function cars(){return $this->hasMany(Car::class);}public function car(){return $this->hasOne(Car::class);}public function owners(){return $this->through('cars')->has('owner');}public function owner(){return $this->throughCar()->hasOwner();}}"#),
            ("app/Models/Activity.php",r#"<?php namespace App\Models;use Illuminate\Database\Eloquent\Relations\MorphTo;class Activity extends \Illuminate\Database\Eloquent\Model{/** @return MorphTo<Post, $this> */public function subject():MorphTo{return $this->morphTo();}}"#),
        ]{cache(&mut index.eloquent_models,source,&format!("/project/{path}"));}
        finish(&mut index.eloquent_models);
        for source in [r#"<?php \App\Models\Mechanic::first()->owners->first()->tit^le"#,r#"<?php \App\Models\Mechanic::first()->owner->tit^le"#,r#"<?php \App\Models\Activity::first()->subject->tit^le"#]{assert!(labels(&index,source).contains(&"title".into()),"{source}");}
        let facts=model_facts(&index,"App\\Models\\Mechanic",&|_|true).unwrap();assert!(facts.relations.iter().any(|relation|relation.name=="owners"&&relation.many&&relation.kind=="hasManyThrough"));
        let(text,at)=crate::tests::cursor(r#"<?php \App\Models\Mechanic::first()->owner->tit^le"#);let doc=crate::tests::doc("/project/Test.php","php",&text);
        assert!(completion_checked(&index,&doc,at,&|path|!path.ends_with("Car.php")).is_empty());
    }
    #[test]
    fn eloquent_identity_and_cast_facts_do_not_require_a_source_table(){
        let mut index=index();cache(&mut index.eloquent_models,r#"<?php namespace App\Models;class TenantPost extends \Illuminate\Database\Eloquent\Model{protected $table;public function getTable(){return config('tenant.table');}protected $casts=['active'=>'boolean'];}"#,"/project/app/Models/TenantPost.php");
        assert!(super::super::model_identities(&index.eloquent_models).iter().any(|(full,_)|full=="App\\Models\\TenantPost"));
        assert!(!super::super::model_inventory(&index.eloquent_models).iter().any(|(full,_,_)|full=="App\\Models\\TenantPost"));
        let facts=model_facts(&index,"App\\Models\\TenantPost",&|_|true).unwrap();assert!(facts.table.is_none());assert!(facts.fields.iter().any(|field|field.name=="active"&&field.value_type.as_deref()==Some("bool")));
    }
    #[test]
    fn indexed_framework_factory_trait_does_not_shadow_native_factory_resolution(){
        let mut index=index();let source=r#"<?php namespace Illuminate\Database\Eloquent\Factories;trait HasFactory {public static function factory($count=null,$state=[]){return static::newFactory();}protected static function newFactory(){return static::getUseFactoryAttribute();}}"#;
        cache(&mut index.eloquent_models,source,"/project/vendor/laravel/framework/src/Illuminate/Database/Eloquent/Factories/HasFactory.php");finish(&mut index.eloquent_models);
        assert!(labels(&index,r#"<?php \App\Models\Post::factory()->make()->tit^le"#).contains(&"title".into()));
    }
    #[test]
    fn result_type_navigation_and_declared_accessor_class_chains_use_actual_target_sources(){
        let mut index=index();
        for(path,source)in [
            ("app/ValueObjects/Money.php",r#"<?php namespace App\ValueObjects;class Money{public int $amount;}"#),
            ("app/Models/Purchase.php",r#"<?php namespace App\Models;class Purchase extends \Illuminate\Database\Eloquent\Model{public function getMoneyAttribute():?\App\ValueObjects\Money{return null;}public function selected():Post{return new Post;}}"#),
            ("app/Relations/Deep.php",r#"<?php namespace App\Relations;class Deep extends \Illuminate\Database\Eloquent\Relations\HasManyThrough{}"#),
            ("app/Models/Team.php",r#"<?php namespace App\Models;use App\Relations\Deep;class Team extends \Illuminate\Database\Eloquent\Model{/** @return Deep<Post, $this> */public function posts():Deep{return custom_relation();}}"#),
        ]{let path=format!("/project/{path}");cache(&mut index.eloquent_models,source,&path);cache_object(&mut index.eloquent_objects,source,&path);}
        finish(&mut index.eloquent_models);
        assert!(labels(&index,r#"<?php \App\Models\Purchase::first()->money->am^ount"#).contains(&"amount".into()));
        assert!(labels(&index,r#"<?php \App\Models\Team::first()->posts->first()->tit^le"#).contains(&"title".into()));
        for(source,path)in [
            (r#"<?php \App\Models\Purchase::first()->mon^ey"#,"/project/app/ValueObjects/Money.php"),
            (r#"<?php \App\Models\Purchase::first()->sel^ected()"#,"/project/app/Models/Post.php"),
            (r#"<?php \App\Models\Post::query()->publ^ished()"#,"/project/app/Queries/Posts.php"),
            (r#"<?php \App\Models\Post::factory()->ma^ke()"#,"/project/app/Models/Post.php"),
        ]{let(text,at)=crate::tests::cursor(source);let doc=crate::tests::doc("/project/Usage.php","php",&text);assert_eq!(type_definition_checked(&index,&doc,at,&|_|true)[0].path,path,"{source}");}
        let(text,at)=crate::tests::cursor(r#"<?php \App\Models\Team::first()->posts->first()->tit^le"#);let doc=crate::tests::doc("/project/Usage.php","php",&text);
        assert!(completion_checked(&index,&doc,at,&|path|!path.contains("/Relations/")).is_empty());
    }
    #[test]
    fn enum_case_inventory_retains_literal_values_utf16_and_declines_ambiguous_owners(){
        let mut index=index();let source="<?php namespace App\\Enums; // préface 💡\nenum State:string {case Draft='draft';case Published='publié';}";
        cache_enum(&mut index.eloquent_enums,source,"/project/app/Enums/State.php");
        let(_,cases,kind)=super::super::enum_values(&index,"App\\Enums\\State",&|_|true).unwrap();assert_eq!(kind.as_deref(),Some("string"));assert_eq!(cases[1].1,Some(serde_json::json!("publié")));assert_eq!(&source[text::offset(source,cases[1].2.at).unwrap()..][..9],"Published");
        assert!(super::super::enum_values(&index,"App\\Enums\\State",&|_|false).is_none());
        cache_enum(&mut index.eloquent_enums,source,"/project/other/State.php");assert!(super::super::enum_values(&index,"App\\Enums\\State",&|_|true).is_none());
    }
}
