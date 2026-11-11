//! Explicit relation return generics and fluent through declarations.
use super::*;
#[derive(Clone,Debug)]pub(super) struct Annotated{pub name:String,pub class:String,pub target:String,pub entry:Entry}
#[derive(Clone,Debug)]pub(super) struct Through{pub name:String,pub via:String,pub distant:String,pub factory:String,pub entry:Entry}
#[derive(Clone,Debug,Default)]pub(super) struct Declared{pub annotated:Vec<Annotated>,pub through:Vec<Through>}
pub(super) fn native(full:&str)->Option<(&'static str,bool)>{
    let class=full.strip_prefix("Illuminate\\Database\\Eloquent\\Relations\\")?;
    Some(match class{"HasOne"=>("hasOne",false),"HasMany"=>("hasMany",true),"BelongsTo"=>("belongsTo",false),"BelongsToMany"=>("belongsToMany",true),"HasOneThrough"=>("hasOneThrough",false),"HasManyThrough"=>("hasManyThrough",true),"MorphOne"=>("morphOne",false),"MorphMany"=>("morphMany",true),"MorphTo"=>("morphTo",false),"MorphToMany"=>("morphToMany",true),_=>return None})
}
pub(super) fn declared(source:&Source,names:&Names,path:&str)->Declared{
    let mut result=Declared::default();let Some((full,_,body,_))=names.class.as_ref()else{return result};
    for function in source.functions().into_iter().filter(|function|source.paths[function.start]==vec![*body]){
        let Some(at)=function.name else{continue};let name=source.tokens[at].value;let mut begin=function.start;
        while begin>0&&matches!(source.tokens[begin-1].value,"public"|"protected"|"private"|"final"|"abstract"|"static"){begin-=1;}
        if source.tokens[begin..function.start].iter().any(|token|token.value=="private"||token.value=="static"){continue;}
        let prefix=source.text[..source.tokens[begin].start].trim_end();
        if prefix.ends_with("*/")&&let Some(start)=prefix.rfind("/**"){
            let doc=&prefix[start+3..prefix.len()-2];let declarations=doc.lines().filter_map(|line|{
                let (_,tail)=line.split_once("@return")?;let (class,tail)=tail.trim().split_once('<')?;
                let class=names.resolve(class.trim());
                if source.is(function.params.end+1,":"){
                    let ty=function.params.end+2;
                    if ty+1!=function.body||source.tokens.get(ty)?.kind!=Kind::Name{return None;}
                    let declared=names.resolve(source.tokens[ty].value);
                    if declared!=class&&declared!="Illuminate\\Database\\Eloquent\\Relations\\Relation"{return None;}
                }
                let target=tail.split([',','>']).next()?.trim();
                if !crate::generator_plan::qualified(target)||matches!(target,"static"|"parent"){return None;}
                let target=if target=="self"{full.clone()}else{names.resolve(target)};
                Some(Annotated{name:name.into(),class,target,entry:entry(source,path,at)})
            }).collect::<Vec<_>>();
            if declarations.len()==1{result.annotated.extend(declarations);}
        }
        let b=function.body+1;
        if !source.is(b,"return")||!source.is(b+1,"$this")||!source.is(b+2,"->"){continue;}
        let method=source.tokens.get(b+3).map(|token|token.value).unwrap_or("");
        let (via,close)=if method=="through"&&source.is(b+4,"(")&&source.is(b+6,")"){
            let token=&source.tokens[b+5];let Some((name,_))=php::literal(source.text,token.start..token.end)else{continue};(name.to_owned(),b+6)
        }else if let Some(via)=method.strip_prefix("through").filter(|via|!via.is_empty()&&via.starts_with(char::is_uppercase)){
            if !source.is(b+4,"(")||!source.is(b+5,")"){continue;}let mut chars=via.chars();(format!("{}{}",chars.next().unwrap().to_lowercase(),chars.as_str()),b+5)
        }else{continue};
        if !source.is(close+1,"->"){continue;}
        let factory=method.to_owned();
        let method=source.tokens.get(close+2).map(|token|token.value).unwrap_or("");
        let (distant,end)=if method=="has"&&source.is(close+3,"(")&&source.is(close+5,")"){
            let token=&source.tokens[close+4];let Some((name,_))=php::literal(source.text,token.start..token.end)else{continue};(name.to_owned(),close+5)
        }else if let Some(distant)=method.strip_prefix("has").filter(|name|!name.is_empty()&&name.starts_with(char::is_uppercase)){
            if !source.is(close+3,"(")||!source.is(close+4,")"){continue;}let mut chars=distant.chars();(format!("{}{}",chars.next().unwrap().to_lowercase(),chars.as_str()),close+4)
        }else{continue};
        if !source.is(end+1,";")||end+2!=function.end{continue;}
        result.through.push(Through{name:name.into(),via,distant,factory,entry:entry(source,path,at)});
    }result
}
pub(super) fn apply_native(model:&mut Model){
    for annotation in &model.semantic.relation_declarations.annotated{if let Some((kind,many))=native(&annotation.class){model.relations.entry(annotation.name.clone()).or_insert(Relation{target:annotation.target.clone(),kind:kind.into(),many,entry:annotation.entry.clone(),source_type:Some(annotation.class.clone()),evidence_paths:Vec::new()});}}
}
fn declared_relation<'a>(models:&'a Models,full:&str,name:&str)->Option<&'a Relation>{
    let mut full=full;let mut seen=BTreeSet::new();
    for _ in 0..8{if !seen.insert(full.to_ascii_lowercase()){return None;}let model=models.get(&full.to_ascii_lowercase()).filter(|model|!model.ambiguous)?;if model.declared.contains(name){return model.relations.get(name);}full=&model.parent;}None
}
pub(super) fn augment(model:&mut Model,models:&Models){
    for annotation in &model.semantic.relation_declarations.annotated{
        if model.relations.contains_key(&annotation.name){continue;}
        let mut current=annotation.class.as_str();let mut evidence=Vec::new();let mut seen=BTreeSet::new();let mut kind=None;
        for _ in 0..8{
            if let Some(native)=native(current){kind=Some(native);break;}
            if !seen.insert(current.to_ascii_lowercase()){break;}
            let Some(class)=models.get(&current.to_ascii_lowercase()).filter(|class|!class.ambiguous&&!class.is_trait)else{break};
            evidence.push(class.path.clone());current=&class.parent;
        }
        if let Some((_,many))=kind{model.relations.insert(annotation.name.clone(),Relation{target:annotation.target.clone(),kind:annotation.class.clone(),many,entry:annotation.entry.clone(),source_type:Some(annotation.class.clone()),evidence_paths:evidence});}
    }
    for through in &model.semantic.relation_declarations.through{
        if model.relations.contains_key(&through.name)||model.declared.contains("through")||model.declared.contains(&through.factory){continue;}
        let local=model.relations.get(&through.via).or_else(||declared_relation(models,&model.parent,&through.via));
        let Some(local)=local.filter(|relation|["hasOne","hasMany","morphOne","morphMany"].contains(&relation.kind.as_str()))else{continue};
        let Some(distant)=declared_relation(models,&local.target,&through.distant).filter(|relation|["hasOne","hasMany","morphOne","morphMany"].contains(&relation.kind.as_str()))else{continue};
        let many=local.many||distant.many;
        model.relations.insert(through.name.clone(),Relation{target:distant.target.clone(),kind:if many{"hasManyThrough"}else{"hasOneThrough"}.into(),many,entry:through.entry.clone(),source_type:None,evidence_paths:std::iter::once(local.entry.path.clone()).chain(std::iter::once(distant.entry.path.clone())).chain(local.evidence_paths.iter().cloned()).chain(distant.evidence_paths.iter().cloned()).chain(models.get(&local.target.to_ascii_lowercase()).map(|model|model.path.clone())).collect()});
    }
}
pub(super) fn finish(models:&mut Models){
    for _ in 0..4{
        let mut updates=Vec::new();
        for(key,model)in models.iter().filter(|(_,model)|!model.ambiguous&&(!model.semantic.relation_declarations.through.is_empty()||!model.semantic.relation_declarations.annotated.is_empty())){
            let mut model=model.clone();let before=model.relations.len();augment(&mut model,models);if model.relations.len()!=before{updates.push((key.clone(),model));}
        }
        if updates.is_empty(){break;}for(key,model)in updates{models.insert(key,model);}
    }
}

#[derive(Clone,Debug)]
pub(crate) struct BelongsToKeys{
    pub target:String,pub foreign_key:Option<String>,pub owner_key:Option<String>,pub relation_name:String,pub entry:Entry,
}
/// Literal relation arguments only. None keys explicitly mean Laravel's default,
/// not an invented id column; callers must prove the target key separately.
pub(crate) fn belongs_to_key_source(text:&str,path:&str,method:&str)->Option<BelongsToKeys>{
    let source=Source::new(text)?;let names=Names::parse(&source)?;let(_,_,body,_)=names.class.as_ref()?;
    let functions=source.functions().into_iter().filter(|f|source.paths[f.start]==vec![*body]&&f.name.is_some_and(|at|source.tokens[at].value.eq_ignore_ascii_case(method))).collect::<Vec<_>>();
    let[f]=functions.as_slice()else{return None};let at=f.name?;let b=f.body+1;
    if !source.is(b,"return")||!source.is(b+1,"$this")||!source.is(b+2,"->")||!source.is(b+3,"belongsTo")||!source.is(b+4,"("){return None;}
    let close=*source.pairs.get(&(b+4))?;if !source.is(close+1,";")||close+2!=f.end{return None;}
    let args=container::arguments(&source,b+4)?;if args.is_empty()||args.len()>4{return None;}
    let target=&args[0];if target.end!=target.start+3||source.tokens[target.start].kind!=Kind::Name||!source.is(target.start+1,"::")||!source.is(target.start+2,"class"){return None;}
    let target=names.resolve(source.tokens[target.start].value);
    fn key(source:&Source,range:Option<&std::ops::Range<usize>>)->Option<Option<String>>{
        let Some(range)=range else{return Some(None)};
        if range.end!=range.start+1{return None;}
        if source.is(range.start,"null"){return Some(None);}
        let token=&source.tokens[range.start];let(value,_)=php::literal(source.text,token.start..token.end)?;
        let value=metadata_inference::decode_literal(value,token.value.as_bytes()[0])?;
        crate::generator_plan::identifier(&value).then_some(Some(value))
    }
    let foreign_key=key(&source,args.get(1))?;let owner_key=key(&source,args.get(2))?;let relation_name=key(&source,args.get(3))?.unwrap_or_else(||source.tokens[at].value.into());
    Some(BelongsToKeys{target,foreign_key,owner_key,relation_name,entry:entry(&source,path,at)})
}
pub(crate) fn belongs_to_keys(index:&Index,full:&str,name:&str,source:&str,owns:&dyn Fn(&str)->bool)->Option<BelongsToKeys>{
    let doc=Document{path:String::new(),language:"php".into(),version:0,text:format!("<?php \\{full}::query()->")};
    let context=context(&doc,text::position(&doc.text,doc.text.len()))?;
    let resolver=Resolver{context:&context,index,overlay:None,object_overlay:None,path:"",owns,owner_cache:RefCell::new(BTreeMap::new())};
    let relation=resolver.relation(full,name)?;if relation.kind!="belongsTo"||resolver.declared(full,"belongsTo"){return None;}
    let parsed=belongs_to_key_source(source,&relation.entry.path,name)?;
    (parsed.target.eq_ignore_ascii_case(&relation.target)&&parsed.entry.at==relation.entry.at&&parsed.entry.width==relation.entry.width).then_some(parsed)
}
#[cfg(test)]mod key_tests{
    use super::*;
    #[test]fn belongs_to_arguments_preserve_unknown_defaults_and_refuse_dynamic_or_fluent_overrides(){
        let source=r#"<?php namespace Domain;use App\Models\User;class Post{public function author(){return $this->belongsTo(User::class,'author_uuid','uuid','writer');}public function user(){return $this->belongsTo(User::class);}public function dynamic(){return $this->belongsTo(User::class,key());}public function filtered(){return $this->belongsTo(User::class)->where('x',1);}}"#;
        let explicit=belongs_to_key_source(source,"/project/Post.php","author").unwrap();assert_eq!(explicit.target,"App\\Models\\User");assert_eq!(explicit.foreign_key.as_deref(),Some("author_uuid"));assert_eq!(explicit.owner_key.as_deref(),Some("uuid"));assert_eq!(explicit.relation_name,"writer");
        let default=belongs_to_key_source(source,"/project/Post.php","user").unwrap();assert!(default.foreign_key.is_none());assert!(default.owner_key.is_none());assert_eq!(default.relation_name,"user");
        assert!(belongs_to_key_source(source,"/project/Post.php","dynamic").is_none());assert!(belongs_to_key_source(source,"/project/Post.php","filtered").is_none());
    }
}
