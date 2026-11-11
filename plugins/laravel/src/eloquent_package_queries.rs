//! Installed-source-proven CTE base-query forwarding and PowerJoins macros.
//! Explicit index preparation only; completion/definition never read files.
use super::*;
use openidea_plugin::Project;
use std::rc::Rc;
const CTE:&str="Staudenmeir\\LaravelCte\\Eloquent\\QueriesExpressions";
const BUILD_EXPRESSIONS:&str="Staudenmeir\\LaravelCte\\Query\\Traits\\BuildsExpressionQueries";
#[derive(Clone,Debug)]pub(super) struct Method{pub entry:Entry,pub cte:bool,pub evidence:Rc<Vec<String>>}
#[derive(Clone,Debug,Default)]pub(crate) struct Capabilities{pub(super) methods:BTreeMap<String,Method>,join_methods:BTreeMap<String,Entry>,join_keep:BTreeSet<String>,join_evidence:Vec<String>,cte_global:Vec<String>}
fn own_functions(source:&Source)->Option<(Names,Vec<Function>)>{
    let names=Names::parse(source)?;let(_,_,body,_)=names.class.as_ref()?;
    if !source.pairs.contains_key(body){return None;}
    let functions=source.functions().into_iter().filter(|f|f.name.is_some()&&source.paths[f.start]==vec![*body]).collect();Some((names,functions))
}
fn public(source:&Source,function:&Function)->bool{
    let begin=(0..function.start).rev().find(|at|matches!(source.tokens[*at].value,";"|"{"|"}")).map_or(0,|at|at+1);
    !source.tokens[begin..function.start].iter().any(|token|["private","protected","static","abstract"].iter().any(|modifier|token.value.eq_ignore_ascii_case(modifier)))
}
fn returned_function(source:&Source,outer:&Function)->Option<Function>{
    let b=outer.body+1;if !source.is(b,"return"){return None;}
    let start=b+1+usize::from(source.is(b+1,"static"));
    source.functions().into_iter().find(|f|f.start==start&&f.name.is_none()&&source.is(f.body,"{")&&source.is(f.end+1,";")&&f.end+2==outer.end)
}
fn self_expression(source:&Source,start:usize,end:usize,known:&BTreeSet<String>)->bool{
    if !source.is(start,"$this"){return false;}let mut at=start+1;
    while at<end{
        if !source.is(at,"->")||!source.is(at+2,"(")||!known.contains(&source.tokens.get(at+1).map(|t|t.value.to_ascii_lowercase()).unwrap_or_default()){return false;}
        let Some(close)=source.pairs.get(&(at+2)).filter(|close|**close<end)else{return false};at=close+1;
    }at==end
}
fn preserves_self(source:&Source,function:&Function,known:&BTreeSet<String>)->bool{
    let nested=source.functions().into_iter().filter(|f|f.start>function.start&&f.end<function.end).collect::<Vec<_>>();
    let returns=(function.body+1..function.end).filter(|at|source.is(*at,"return")&&(*at==0||!matches!(source.tokens[at-1].value,"->"|"?->"|"::"))&&!nested.iter().any(|f|f.body<*at&&*at<f.end)).collect::<Vec<_>>();
    if returns.is_empty()||(function.body+1..function.end).any(|at|source.is(at,"yield")&&!nested.iter().any(|f|f.body<at&&at<f.end)){return false;}
    let mut final_return=false;
    for at in returns{
        let mut end=at+1;while end<function.end&&!source.is(end,";"){
            if let Some(close)=source.pairs.get(&end).filter(|close|**close>end){end=close+1;}else{end+=1;}
        }
        if !self_expression(source,at+1,end,known){return false;}
        if end+1==function.end&&source.paths[at]==source.paths[function.body+1]&&(at==function.body+1||matches!(source.tokens[at-1].value,";"|"}")){final_return=true;}
    }final_return
}
fn methods(text:&str,path:&str,macros:bool)->BTreeMap<String,Entry>{
    let Some(source)=Source::new(text)else{return BTreeMap::new()};let Some((_,functions))=own_functions(&source)else{return BTreeMap::new()};
    let candidates=functions.into_iter().filter(|f|public(&source,f)).filter_map(|outer|{
        let name=source.tokens[outer.name?].value.to_owned();if name.starts_with("__"){return None;}
        let body=if macros{returned_function(&source,&outer)?}else{outer.clone()};Some((name,outer,body))
    }).collect::<Vec<_>>();
    let mut known=BTreeSet::new();let mut result=BTreeMap::new();
    for _ in 0..8{let before=known.len();for(name,outer,body)in &candidates{
        if preserves_self(&source,body,&known){known.insert(name.to_ascii_lowercase());result.insert(name.clone(),entry(&source,path,outer.name.unwrap()));}
    }if known.len()==before{break;}}
    result
}
fn static_calls(text:&str,method:&str,member:&str)->Vec<(String,Vec<String>)>{
    let Some(source)=Source::new(text)else{return vec![]};let Some((names,functions))=own_functions(&source)else{return vec![]};
    let Some(function)=functions.into_iter().find(|f|f.name.is_some_and(|at|source.tokens[at].value.eq_ignore_ascii_case(method)))else{return vec![]};
    let mut result=vec![];
    for at in function.body+1..function.end{
        if source.paths[at]!=source.paths[function.body+1]||at!=function.body+1&&!matches!(source.tokens[at-1].value,";"|"}")||source.tokens[at].kind!=Kind::Name||!source.is(at+1,"::")||!source.is(at+2,member)||!source.is(at+3,"("){continue;}
        let Some(close)=source.pairs.get(&(at+3))else{continue};
        result.push((names.resolve(source.tokens[at].value),source.tokens[at+4..*close].iter().map(|t|t.value.into()).collect()));
    }result
}
fn cte_hook(text:&str,builders:&BTreeSet<String>)->bool{
    let Some(source)=Source::new(text)else{return false};let Some((names,functions))=own_functions(&source)else{return false};
    let functions=functions.into_iter().filter(|f|f.name.is_some_and(|at|source.is(at,"newBaseQueryBuilder"))).collect::<Vec<_>>();let[function]=functions.as_slice()else{return false};
    let b=function.body+1;let prefix=["$connection","=","$this","->","getConnection","(",")",";","return","match","(","$connection","->","getDriverName","(",")",")","{"];
    if !prefix.iter().enumerate().all(|(at,token)|source.is(b+at,token)){return false;}
    let open=b+prefix.len()-1;let Some(close)=source.pairs.get(&open).copied()else{return false};
    if !source.is(close+1,";")||close+2!=function.end{return false;}
    let mut at=open+1;let mut default=false;let mut arms=0;
    while at<close{
        if source.is(at,"default"){if default{return false;}default=true;}else if source.tokens[at].kind!=Kind::String{return false;}
        if !source.is(at+1,"=>")||!source.is(at+2,"new")||!builders.contains(&names.resolve(source.tokens.get(at+3).map(|t|t.value).unwrap_or("")))||!source.is(at+4,"(")||!source.is(at+5,"$connection")||!source.is(at+6,")"){return false;}
        at+=7;arms+=1;if source.is(at,","){at+=1;}else if at!=close{return false;}
    }default&&arms>0
}
fn class_method_body(text:&str,name:&str)->Option<Vec<String>>{
    let source=Source::new(text)?;let(_,functions)=own_functions(&source)?;
    let matching=functions.iter().filter(|f|f.name.is_some_and(|at|source.is(at,name))).collect::<Vec<_>>();let[f]=matching.as_slice()else{return None};
    Some(source.tokens[f.body+1..f.end].iter().map(|t|t.value.to_owned()).collect())
}
fn provider_factory(text:&str)->bool{
    let Some(source)=Source::new(text)else{return false};let Some((names,functions))=own_functions(&source)else{return false};
    if names.class.as_ref().is_none_or(|(_,_,_,parent)|parent!="Illuminate\\Support\\ServiceProvider"){return false;}
    let functions=functions.iter().filter(|f|f.name.is_some_and(|at|source.is(at,"register"))).collect::<Vec<_>>();let[f]=functions.as_slice()else{return false};
    let body=&source.tokens[f.body+1..f.end];let expected=["$this","->","app","->","singleton","(","",",","function","(","","$app",")","{","return","new","","(","$app",")",";","}",")",";"];
    body.len()==expected.len()&&expected.iter().enumerate().all(|(at,value)|value.is_empty()||body[at].value==*value)&&php::literal(text,body[6].start..body[6].end).is_some_and(|(key,_)|key=="db.factory")&&names.resolve(body[10].value)=="Illuminate\\Contracts\\Container\\Container"&&names.resolve(body[16].value)=="Staudenmeir\\LaravelCte\\Connectors\\ConnectionFactory"
}
fn creates_builder(text:&str,class:&str)->bool{
    let Some(source)=Source::new(text)else{return false};let Some(names)=Names::parse(&source)else{return false};
    class_method_body(text,"query").is_some_and(|tokens|tokens.len()==7&&tokens[0]=="return"&&tokens[1]=="new"&&names.resolve(&tokens[2])==class&&tokens[3..]==["(","$this",")",";"])
}
fn factory_connections(text:&str)->Option<BTreeSet<String>>{
    let source=Source::new(text)?;let(names,functions)=own_functions(&source)?;
    if names.class.as_ref()?.3!="Illuminate\\Database\\Connectors\\ConnectionFactory"{return None;}
    let f=functions.iter().find(|f|f.name.is_some_and(|at|source.is(at,"createConnection")))?;
    let at=(f.body+1..f.end).find(|at|source.is(*at,"return")&&source.is(at+1,"match")&&source.paths[*at]==source.paths[f.body+1])?;
    if !source.is(at+2,"(")||!source.is(at+3,"$driver")||!source.is(at+4,")")||!source.is(at+5,"{"){return None;}
    let close=*source.pairs.get(&(at+5))?;if !source.is(close+1,";")||close+2!=f.end{return None;}
    let mut cursor=at+6;let mut classes=BTreeSet::new();let mut default=false;
    while cursor<close{
        let key=&source.tokens[cursor];if !source.is(cursor+1,"=>"){return None;}
        if key.value=="default"{
            if default||!source.is(cursor+2,"throw")||!source.is(cursor+3,"new")||names.resolve(source.tokens.get(cursor+4)?.value)!="InvalidArgumentException"{return None;}default=true;cursor+=5;
            if !source.is(cursor,"("){return None;}cursor=source.pairs.get(&cursor)?+1;
        }else{
            let(driver,_)=php::literal(text,key.start..key.end)?;let short=match driver{"mysql"=>"MySqlConnection","mariadb"=>"MariaDbConnection","pgsql"=>"PostgresConnection","sqlite"=>"SQLiteConnection","sqlsrv"=>"SqlServerConnection","singlestore"=>"SingleStoreConnection",_=>return None};
            let full=format!("Staudenmeir\\LaravelCte\\Connections\\{short}");
            if !source.is(cursor+2,"new")||names.resolve(source.tokens.get(cursor+3)?.value)!=full||!source.is(cursor+4,"("){return None;}
            let args=container::arguments(&source,cursor+4)?;let expected=["$connection","$database","$prefix","$config"];
            if args.len()!=4||args.iter().zip(expected).any(|(arg,name)|arg.end!=arg.start+1||!source.is(arg.start,name)){return None;}
            if !classes.insert(full){return None;}cursor=source.pairs.get(&(cursor+4))?+1;
        }
        if source.is(cursor,","){cursor+=1;}else if cursor!=close{return None;}
    }(default&&classes.len()==6).then_some(classes)
}
fn discovery_enabled(project:&Project,index:&mut Index,package:&str)->bool{
    let Ok(root)=project.root()else{return false};let Ok(text)=project.read_file("composer.json")else{return false};
    if text.len()>128*1024{return false;}
    index.dependency_files.insert("composer.json".into());let evidence=Some((text.len(),crate::artisan_metadata::hash(&text)));
    if index.source_evidence.get("composer.json").is_some_and(|old|old!=&evidence){index.source_evidence_incomplete=true;}
    index.source_evidence.insert("composer.json".into(),evidence);let _=root;
    let Ok(value)=serde_json::from_str::<serde_json::Value>(&text)else{return false};
    !value.pointer("/extra/laravel/dont-discover").and_then(|v|v.as_array()).is_some_and(|values|values.iter().filter_map(|v|v.as_str()).any(|name|name=="*"||name==package))
}
/// Literal native Eloquent macro registrations outside the installed registrar
/// can replace global macro results; unknown names/mixins withhold all adapters.
pub(super) fn declared_overrides(source:&Source,names:&Names)->BTreeSet<String>{
    let mut result=BTreeSet::new();
    for at in 1..source.tokens.len(){
        if !source.is(at,"macro")||!matches!(source.tokens[at-1].value,"->"|"?->")||!source.is(at+1,"("){continue;}
        let value=source.tokens.get(at+2).and_then(|token|php::literal(source.text,token.start..token.end)).map(|(value,_)|value.to_ascii_lowercase()).unwrap_or_else(||"*".into());result.insert(value);
    }
    for(at,token)in source.tokens.iter().enumerate(){
        if token.value.eq_ignore_ascii_case("resolverFor")&&at>0&&matches!(source.tokens[at-1].value,"::"|"->"|"?->")&&source.is(at+1,"("){result.insert("!cte".into());}
        if ["bind","singleton","instance","scoped","extend"].iter().any(|name|token.value.eq_ignore_ascii_case(name))&&source.is(at+1,"(")&&source.tokens.get(at+2).and_then(|token|php::literal(source.text,token.start..token.end)).is_some_and(|(value,_)|value=="db.factory"){result.insert("!cte".into());}
    }
    for at in 0..source.tokens.len(){
        if source.tokens[at].kind!=Kind::Name||names.resolve(source.tokens[at].value)!="Illuminate\\Database\\Eloquent\\Builder"||!source.is(at+1,"::")||!source.is(at+3,"("){continue;}
        if source.is(at+2,"mixin"){
            let literal=source.is(at+4,"new")&&source.tokens.get(at+5).is_some_and(|t|t.kind==Kind::Name)&&source.is(at+6,"(")&&source.is(at+7,")")&&source.is(at+8,")");
            result.insert(if literal{format!("mixin:{}",names.resolve(source.tokens[at+5].value))}else{"*".into()});
        }
        if source.is(at+2,"macro"){
            let value=source.tokens.get(at+4).and_then(|token|php::literal(source.text,token.start..token.end)).map(|(value,_)|value.to_ascii_lowercase()).unwrap_or_else(||"*".into());result.insert(value);
        }
    }result
}
/// Prepare bounded package facts from the descriptor-filtered installed roots.
pub(crate) fn apply(project:&Project,index:&mut Index){
    if !index.packages.roots.iter().any(|root|matches!(root.package.as_str(),"staudenmeir/laravel-cte"|"kirschbaum-development/eloquent-power-joins")){return;}
    let Ok(root)=project.root()else{return};let mut caps=Capabilities::default();
    let mut bytes=0usize;let mut calls=0usize;
    let mut snapshot=|index:&mut Index,package:&str,full:&str|->Option<(String,String)>{
        calls+=1;if calls>32||bytes>2*1024*1024{return None;}
        let(path,text)=crate::package_integrations::snapshot_class(project,index,package,full)?;bytes+=text.len();
        (bytes<=2*1024*1024).then(||(format!("{root}/{}",path.trim_start_matches('/')),text))
    };
    if let(Some((trait_path,trait_text)),Some((methods_path,methods_text)))=(snapshot(index,"staudenmeir/laravel-cte",CTE),snapshot(index,"staudenmeir/laravel-cte",BUILD_EXPRESSIONS)){
        let mut evidence=vec![trait_path.clone(),methods_path.clone()];let mut builders=BTreeSet::new();
        for short in ["Builder","OracleBuilder","SingleStoreBuilder"]{
            let full=format!("Staudenmeir\\LaravelCte\\Query\\{short}");
            if let Some((path,text))=snapshot(index,"staudenmeir/laravel-cte",&full)&&let Some(model)=super::model(&text,&path)&&model.traits.iter().any(|t|t==BUILD_EXPRESSIONS){builders.insert(full);evidence.push(path);}
        }
        if cte_hook(&trait_text,&builders){
            let evidence=Rc::new(evidence);for(name,entry)in methods(&methods_text,&methods_path,false){caps.methods.insert(name,Method{entry,cte:true,evidence:evidence.clone()});}
            super::cache(&mut index.eloquent_models,&trait_text,&trait_path);
        }
    }
    if caps.methods.values().any(|method|method.cte){
        let provider_full="Staudenmeir\\LaravelCte\\DatabaseServiceProvider";
        if let(Some((provider_path,provider)),Some((factory_path,factory)),Some((trait_path,trait_text)))=(snapshot(index,"staudenmeir/laravel-cte",provider_full),snapshot(index,"staudenmeir/laravel-cte","Staudenmeir\\LaravelCte\\Connectors\\ConnectionFactory"),snapshot(index,"staudenmeir/laravel-cte","Staudenmeir\\LaravelCte\\Connections\\CreatesQueryBuilder")){
            let discovered=index.packages.provider_sources.values().any(|source|source==&provider)&&discovery_enabled(project,index,"staudenmeir/laravel-cte");
            let binding=provider_factory(&provider);
            let creates=creates_builder(&trait_text,"Staudenmeir\\LaravelCte\\Query\\Builder");
            if discovered&&binding&&creates{
                let mut evidence=vec![provider_path,factory_path,trait_path];let mut valid=true;
                let classes=["MySqlConnection","MariaDbConnection","PostgresConnection","SQLiteConnection","SqlServerConnection","SingleStoreConnection"];
                let factory_classes=factory_connections(&factory).unwrap_or_default();
                for short in classes{
                    let full=format!("Staudenmeir\\LaravelCte\\Connections\\{short}");
                    let Some((path,text))=snapshot(index,"staudenmeir/laravel-cte",&full)else{valid=false;break};
                    let Some(model)=super::model(&text,&path)else{valid=false;break};
                    let constructor=factory_classes.contains(&full);
                    let creates=model.traits.iter().any(|trait_|trait_=="Staudenmeir\\LaravelCte\\Connections\\CreatesQueryBuilder")||short=="SingleStoreConnection"&&creates_builder(&text,"Staudenmeir\\LaravelCte\\Query\\SingleStoreBuilder");
                    if !constructor||!creates{valid=false;break;}evidence.push(path);
                }
                if valid{caps.cte_global=evidence.clone();for method in caps.methods.values_mut().filter(|method|method.cte){let mut all=method.evidence.as_ref().clone();all.extend(evidence.clone());method.evidence=Rc::new(all);}}
            }
        }
    }
    if let(Some((provider_path,provider)),Some((registrar_path,registrar)))=(snapshot(index,"kirschbaum-development/eloquent-power-joins","Kirschbaum\\PowerJoins\\PowerJoinsServiceProvider"),snapshot(index,"kirschbaum-development/eloquent-power-joins","Kirschbaum\\PowerJoins\\EloquentJoins")){
        let discovered=index.packages.provider_sources.values().any(|source|source==&provider);
        let registered=static_calls(&provider,"register","registerEloquentMacros").iter().any(|(class,args)|class=="Kirschbaum\\PowerJoins\\EloquentJoins"&&args.is_empty());
        if discovered&&registered&&discovery_enabled(project,index,"kirschbaum-development/eloquent-power-joins"){
            let mut family_evidence=vec![provider_path.clone(),registrar_path.clone()];let mut complete=true;
            for(class,args)in static_calls(&registrar,"registerEloquentMacros","mixin"){
                if !["Illuminate\\Database\\Eloquent\\Builder","Illuminate\\Database\\Query\\Builder","Illuminate\\Database\\Eloquent\\Relations\\Relation"].contains(&class.as_str()){continue;}
                if args.len()!=4||args[0]!="new"||args[2]!="("||args[3]!=")"{complete=false;continue;}
                let Some(source)=Source::new(&registrar)else{continue};let Some(names)=Names::parse(&source)else{continue};let full=names.resolve(&args[1]);
                if let Some((path,text))=snapshot(index,"kirschbaum-development/eloquent-power-joins",&full){
                    family_evidence.push(path.clone());
                    if class!="Illuminate\\Database\\Eloquent\\Builder"{continue;}
                    super::cache(&mut index.eloquent_models,&text,&path);
                    let evidence=Rc::new(vec![provider_path.clone(),registrar_path.clone(),path.clone()]);
                    for(name,entry)in methods(&text,&path,true){caps.methods.insert(name,Method{entry,cte:false,evidence:evidence.clone()});}
                }else{complete=false;}
            }
            if !complete{caps.methods.retain(|_,method|method.cte);}
            let evidence=Rc::new(family_evidence);for method in caps.methods.values_mut().filter(|method|!method.cte){method.evidence=evidence.clone();}
        }
    }
    if caps.methods.values().any(|method|!method.cte)&&let Some((path,text))=snapshot(index,"kirschbaum-development/eloquent-power-joins","Kirschbaum\\PowerJoins\\PowerJoinClause")&&let Some(source)=Source::new(&text)&&let Some((names,functions))=own_functions(&source)&&names.class.as_ref().is_some_and(|(_,_,_,parent)|parent=="Illuminate\\Database\\Query\\JoinClause"){
        let mut keep=methods(&text,&path,false).keys().cloned().collect::<BTreeSet<_>>();
        for function in functions.into_iter().filter(|function|public(&source,function)){
            let at=function.name.unwrap();let name=source.tokens[at].value;if name.starts_with("__"){continue;}
            caps.join_methods.insert(name.into(),entry(&source,&path,at));
            if source.is(function.params.end+1,":")&&source.is(function.params.end+2,"self")&&function.params.end+3==function.body{keep.insert(name.into());}
        }
        caps.join_keep=keep;caps.join_evidence=std::iter::once(path.clone()).chain(caps.methods.values().filter(|method|!method.cte).flat_map(|method|method.evidence.iter().cloned())).collect();caps.join_evidence.sort();caps.join_evidence.dedup();super::cache_object(&mut index.eloquent_objects,&text,&path);super::cache(&mut index.eloquent_models,&text,&path);
    }
    if caps.methods.is_empty(){return;}
    let caps=Rc::new(caps);for model in index.eloquent_models.values_mut(){model.semantic.package_queries=Some(caps.clone());}
}
impl Resolver<'_,'_>{
    fn package_caps(&self,full:&str)->Option<&Capabilities>{
        self.raw(full)?.semantic.package_queries.as_deref().or_else(||self.index.eloquent_models.get(&full.to_ascii_lowercase()).filter(|m|!m.ambiguous&&self.owned(&m.path)).and_then(|m|m.semantic.package_queries.as_deref()))
    }
    fn package_trait(&self,full:&str,required:&str,seen:&mut BTreeSet<String>,depth:u8)->bool{
        if depth>8||!seen.insert(full.to_ascii_lowercase()){return false;}
        let Some(model)=self.raw(full).filter(|model|!model.ambiguous&&self.owned(&model.path))else{return false};
        model.traits.iter().any(|t|if t.eq_ignore_ascii_case(required){self.raw(t).is_some_and(|trait_|trait_.is_trait&&!trait_.ambiguous&&self.owned(&trait_.path))}else{self.package_trait(t,required,seen,depth+1)})||!model.parent.is_empty()&&self.package_trait(&model.parent,required,seen,depth+1)
    }
    fn macro_override(&self,overrides:&BTreeSet<String>,name:&str)->bool{
        for declared in overrides{
            if declared=="*"||declared.eq_ignore_ascii_case(name){return true;}
            if let Some(full)=declared.strip_prefix("mixin:"){
                let mut full=full;let mut seen=BTreeSet::new();
                for depth in 0..8{
                    if !seen.insert(full.to_ascii_lowercase()){return true;}
                    let Some(mixin)=self.raw(full).filter(|m|!m.ambiguous&&self.owned(&m.path))else{return true};
                    if mixin.declared.iter().any(|method|method.eq_ignore_ascii_case(name)){return true;}
                    if mixin.parent.is_empty(){break;}full=&mixin.parent;if depth==7{return true;}
                }
            }
        }false
    }
    pub(super) fn package_methods(&self,full:&str)->BTreeMap<&str,&Method>{
        let Some(caps)=self.package_caps(full)else{return BTreeMap::new()};
        let global=!caps.cte_global.is_empty()&&caps.cte_global.iter().all(|path|self.owned(path))&&!self.index.eloquent_models.values().any(|model|!caps.cte_global.contains(&model.path)&&model.semantic.query_macro_overrides.contains("!cte"))&&!declared_overrides(&self.context.source,&self.context.names).contains("!cte");
        let cte=(global||self.package_trait(full,CTE,&mut BTreeSet::new(),0))&&self.lineage(full).is_some_and(|lineage|!lineage.iter().any(|model|model.full!=CTE&&model.semantic.source_methods.iter().any(|name|["newBaseQueryBuilder","getConnection","getConnectionName"].iter().any(|hook|name.eq_ignore_ascii_case(hook)))));
        let current=declared_overrides(&self.context.source,&self.context.names);
        let overrides=self.index.eloquent_models.values().chain(self.overlay.iter()).filter(|model|!model.semantic.query_macro_overrides.is_empty()).collect::<Vec<_>>();
        caps.methods.iter().filter(|(name,method)|(!method.cte||cte)&&!self.macro_override(&current,name)&&method.evidence.iter().all(|path|self.owned(path))&&!overrides.iter().any(|model|!method.evidence.contains(&model.path)&&self.macro_override(&model.semantic.query_macro_overrides,name))).map(|(name,method)|(name.as_str(),method)).collect()
    }
    pub(super) fn package_call(&self,receiver:&Typed,name:&str)->Option<Typed>{
        if !matches!(receiver.shape,Shape::Static|Shape::Model|Shape::Query|Shape::Relation(_))||self.get(&receiver.model).is_none()||matches!(receiver.shape,Shape::Static|Shape::Model)&&self.declared(&receiver.model,name){return None;}
        if !self.package_caps(&receiver.model)?.methods.keys().any(|method|method.eq_ignore_ascii_case(name)){return None;}
        if matches!(receiver.shape,Shape::Static|Shape::Model)&&self.declared(&receiver.model,if receiver.shape==Shape::Static{"__callStatic"}else{"__call"}){return None;}
        if self.custom_class(receiver).is_some_and(|class|self.object_method(&class,name).is_some()){return None;}
        self.package_methods(&receiver.model).iter().any(|(method,proof)|if proof.cte{method.eq_ignore_ascii_case(name)}else{*method==name}).then(||Typed{model:receiver.model.clone(),shape:if matches!(receiver.shape,Shape::Static|Shape::Model){Shape::Query}else{receiver.shape.clone()}})
    }
    pub(super) fn package_join_methods(&self,receiver:&Typed)->BTreeMap<String,Entry>{
        let Shape::JoinClause(_)=receiver.shape else{return BTreeMap::new()};let Some(caps)=self.package_caps(&receiver.model)else{return BTreeMap::new()};
        if !caps.join_evidence.iter().all(|path|self.owned(path)){return BTreeMap::new();}caps.join_methods.clone()
    }
    pub(super) fn package_join_call(&self,receiver:&Typed,name:&str)->Option<Typed>{
        let caps=self.package_caps(&receiver.model)?;
        if !caps.join_evidence.iter().all(|path|self.owned(path))||!caps.join_keep.iter().any(|method|method.eq_ignore_ascii_case(name)){return None;}
        Some(receiver.clone())
    }
    pub(super) fn package_join_binding(&self,function:&Function,receiver:&Typed,open:usize,name:&str)->Option<Typed>{
        let source=&self.context.source;let method=source.tokens[open-1].value;
        if !["joinRelationship","joinRelation","leftJoinRelationship","leftJoinRelation","rightJoinRelationship","rightJoinRelation","joinRelationshipUsingAlias","leftJoinRelationshipUsingAlias","rightJoinRelationshipUsingAlias"].contains(&method){return None;}
        if self.package_call(receiver,method).is_none(){return None;}
        let caps=self.package_caps(&receiver.model)?;if caps.join_methods.is_empty()||!caps.join_evidence.iter().all(|path|self.owned(path)){return None;}
        let args=container::arguments(source,open)?;let callback=args.get(1)?;
        if callback.start+usize::from(source.is(callback.start,"static"))!=function.start||args.is_empty(){return None;}
        let token=source.tokens.get(args[0].start)?;if args[0].end!=args[0].start+1{return None;}
        let(path,_)=php::literal(source.text,token.start..token.end)?;let mut model=receiver.model.clone();
        for relation in path.split('.') {model=self.get(&self.relation(&model,relation)?.target)?.full.clone();}
        let params=&source.tokens[function.params.clone()];let variable=params.iter().position(|token|token.kind==Kind::Var&&token.value==name)?;
        if params[..variable].iter().any(|token|matches!(token.value,","|"&"|"...")){return None;}
        let typed=Typed{model,shape:Shape::JoinClause("Kirschbaum\\PowerJoins\\PowerJoinClause".into())};
        if variable>0{
            let parameter=helper_facts::declared_type(&params[..variable].iter().collect::<Vec<_>>(),&self.context.names,"__OpenIDEA_Pipe_Receiver__",false)?;
            let signature=pipe::Callable{full:String::new(),path:self.path.into(),entry:entry(source,self.path,function.start),ambiguous:false,parameter:Some(parameter),required:1,parameters:1,by_reference:false,result:pipe::Return::Input};
            if !self.pipe_accepts(&signature,&typed){return None;}
        }Some(typed)
    }

}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn fluent_method_proof_rejects_scalar_generator_and_unknown_callback_returns(){
        let source=r#"<?php trait Expressions{public function withExpression(){return $this;}public function withRecursiveExpression(){return $this->withExpression();}public function count(){return 4;}public function generator(){yield 1;return $this;}public function conditional(){if($flag)return $this;}}"#;
        let found=methods(source,"/project/Expressions.php",false);assert!(found.contains_key("withExpression"));assert!(found.contains_key("withRecursiveExpression"));assert!(!found.contains_key("count"));assert!(!found.contains_key("generator"));assert!(!found.contains_key("conditional"));
        let source=r#"<?php class Mixins{public function joinRelationship(){return function(){return $this;};}public function leftJoinRelationship(){return function(){return $this->joinRelationship();};}public function scalar(){return function(){return 3;};}}"#;
        let found=methods(source,"/project/Mixins.php",true);assert_eq!(found.keys().cloned().collect::<Vec<_>>(),vec!["joinRelationship","leftJoinRelationship"]);
    }
    fn fixture()->(openidea_plugin::mock::MockHost,Project,Index){
        use openidea_plugin::mock::MockHost;
        let base=std::env::var("OPENIDEA_PACKAGE_RELATIONS_FIXTURE").unwrap_or_else(|_|"/tmp/openidea-package-relations-lift".into());
        let host=MockHost::install();host.add_file("artisan","<?php");
        host.add_file("composer.json",r#"{"require":{"staudenmeir/laravel-cte":"^1.13","kirschbaum-development/eloquent-power-joins":"^4.3"},"autoload":{"psr-4":{"App\\":"app/"}}}"#);
        let installed:serde_json::Value=serde_json::from_str(&std::fs::read_to_string(format!("{base}/vendor/composer/installed.json")).unwrap()).unwrap();
        let packages=installed["packages"].as_array().unwrap().iter().filter(|package|["staudenmeir/laravel-cte","kirschbaum-development/eloquent-power-joins"].contains(&package["name"].as_str().unwrap_or(""))).cloned().collect::<Vec<_>>();assert_eq!(packages.len(),2);
        host.add_file("vendor/composer/installed.json",&serde_json::json!({"packages":packages}).to_string());
        for relative in [
            "vendor/staudenmeir/laravel-cte/src/DatabaseServiceProvider.php","vendor/staudenmeir/laravel-cte/src/Connectors/ConnectionFactory.php","vendor/staudenmeir/laravel-cte/src/Connections/CreatesQueryBuilder.php","vendor/staudenmeir/laravel-cte/src/Connections/MySqlConnection.php","vendor/staudenmeir/laravel-cte/src/Connections/MariaDbConnection.php","vendor/staudenmeir/laravel-cte/src/Connections/PostgresConnection.php","vendor/staudenmeir/laravel-cte/src/Connections/SQLiteConnection.php","vendor/staudenmeir/laravel-cte/src/Connections/SqlServerConnection.php","vendor/staudenmeir/laravel-cte/src/Connections/SingleStoreConnection.php","vendor/staudenmeir/laravel-cte/src/Eloquent/QueriesExpressions.php","vendor/staudenmeir/laravel-cte/src/Query/Traits/BuildsExpressionQueries.php","vendor/staudenmeir/laravel-cte/src/Query/Builder.php","vendor/staudenmeir/laravel-cte/src/Query/OracleBuilder.php","vendor/staudenmeir/laravel-cte/src/Query/SingleStoreBuilder.php",
            "vendor/kirschbaum-development/eloquent-power-joins/src/PowerJoinsServiceProvider.php","vendor/kirschbaum-development/eloquent-power-joins/src/EloquentJoins.php","vendor/kirschbaum-development/eloquent-power-joins/src/Mixins/JoinRelationship.php","vendor/kirschbaum-development/eloquent-power-joins/src/Mixins/QueryRelationshipExistence.php","vendor/kirschbaum-development/eloquent-power-joins/src/PowerJoinClause.php","vendor/kirschbaum-development/eloquent-power-joins/src/Mixins/QueryBuilderExtraMethods.php","vendor/kirschbaum-development/eloquent-power-joins/src/Mixins/RelationshipsExtraMethods.php",
        ]{host.add_file(relative,&std::fs::read_to_string(format!("{base}/{relative}")).unwrap());}
        host.add_file("app/Post.php",r#"<?php namespace App;class Post extends \Illuminate\Database\Eloquent\Model{use \Staudenmeir\LaravelCte\Eloquent\QueriesExpressions;public function author(){return $this->belongsTo(User::class);}}"#);
        host.add_file("app/User.php",r#"<?php namespace App;class User extends \Illuminate\Database\Eloquent\Model{}"#);
        host.add_file("database/migrations/posts.php","<?php Schema::create('posts',function($t){$t->string('title');});Schema::create('users',function($t){$t->string('email');});");
        let project=host.invocation().project().unwrap();let index=Index::build(&project);(host,project,index)
    }
    fn complete(index:&Index,source:&str)->Vec<String>{let(text,at)=crate::tests::cursor(source);super::super::completion_checked(index,&crate::tests::doc("/project/Usage.php","php",&text),at,&|_|true).into_iter().map(|item|item.label).collect()}
    #[test]#[ignore="Requires disposable installed CTE1.13/PowerJoins4.3 source fixture"]
    fn installed_cte_and_powerjoins_reach_native_completion_definition_and_source_guards(){
        use openidea_plugin::providers::Providers;
        let(host,project,index)=fixture();
        let caps=index.eloquent_models["app\\post"].semantic.package_queries.as_ref().unwrap();assert!(caps.methods.contains_key("withExpression"),"{:?}",caps.methods.keys());assert!(caps.methods.contains_key("joinRelationship"),"{:?}",caps.methods.keys());assert!(caps.methods.contains_key("leftJoinRelationship"));
        let mut main=crate::Main::default();main.indexes.insert(project.root().unwrap(),index.clone());
        for source in [r#"<?php \App\Post::query()->withExpression('rows',fn($q)=>$q)->first()->tit^le"#,r#"<?php \App\Post::from('tree')->withRecursiveExpression('tree',\App\Post::select('posts.*')->unionAll(\App\Post::select('posts.*')->join('tree','tree.id','=','posts.id')))->get()->first()->tit^le"#,r#"<?php \App\Post::withRecursiveExpression('rows',fn($q)=>$q)->get()->first()->tit^le"#,r#"<?php \App\Post::joinRelationship('author')->first()->tit^le"#,r#"<?php \App\Post::leftJoinRelationship('author')->get()->first()->tit^le"#]{
            assert!(complete(&index,source).contains(&"title".into()),"{source}");let(text,at)=crate::tests::cursor(source);let doc=crate::tests::doc("/project/Usage.php","php",&text);assert!(main.completion(&doc,at,None).iter().any(|item|item.label=="title"),"{source}");
        }
        for source in [r#"<?php \App\Post::query()->withExpression('rows',fn($q)=>$q)->where('tit^le',1)"#,r#"<?php \App\Post::leftJoinRelationship('author')->where('tit^le',1)"#,r#"<?php \App\Post::joinRelationship('author',fn($join)=>$join->where('em^ail',1))"#,r#"<?php \App\Post::joinRelationship('author',fn($join)=>$join->where('users.em^ail',1))"#]{
            let(text,at)=crate::tests::cursor(source);let doc=crate::tests::doc("/project/Usage.php","php",&text);let expected=if source.contains("users.em^"){"users.email"}else if source.contains("em^"){"email"}else{"title"};assert!(main.completion(&doc,at,None).iter().any(|item|item.label==expected),"{source}");
        }
        assert!(complete(&index,r#"<?php \App\User::query()->withExpression('rows',fn($q)=>$q)->first()->em^ail"#).contains(&"email".into()));
        let(text,at)=crate::tests::cursor(r#"<?php \App\Post::query()->withExpr^ession('rows',fn($q)=>$q)"#);let doc=crate::tests::doc("/project/Usage.php","php",&text);let target=super::super::definition_checked(&index,&doc,at,&|_|true).unwrap();assert!(target.path.ends_with("BuildsExpressionQueries.php"));
        assert!(super::super::completion_checked(&index,&doc,at,&|path|!path.contains("laravel-cte")).is_empty());
        for source in [r#"<?php use Illuminate\Database\Eloquent\Builder;Builder::macro('joinRelationship',fn()=>0);\App\Post::joinRelationship('author')->first()->tit^le"#,r#"<?php $query=\App\Post::query();$query->macro('joinRelationship',fn()=>0);$query->joinRelationship('author')->first()->tit^le"#]{assert!(!complete(&index,source).contains(&"title".into()),"{source}");}
        host.add_file("app/Post.php",r#"<?php namespace App;class Post extends \Illuminate\Database\Eloquent\Model{use \Staudenmeir\LaravelCte\Eloquent\QueriesExpressions;protected function newBaseQueryBuilder(){return parent::newBaseQueryBuilder();}}"#);
        let overridden=Index::build(&project);assert!(!complete(&overridden,r#"<?php \App\Post::withExpression('rows',fn($q)=>$q)->first()->tit^le"#).contains(&"title".into()));
        host.add_file("composer.json",r#"{"extra":{"laravel":{"dont-discover":["kirschbaum-development/eloquent-power-joins","staudenmeir/laravel-cte"]}},"require":{"staudenmeir/laravel-cte":"^1.13","kirschbaum-development/eloquent-power-joins":"^4.3"},"autoload":{"psr-4":{"App\\":"app/"}}}"#);
        host.add_file("app/Post.php",r#"<?php namespace App;class Post extends \Illuminate\Database\Eloquent\Model{use \Staudenmeir\LaravelCte\Eloquent\QueriesExpressions;}"#);
        let index=Index::build(&project);assert!(!complete(&index,r#"<?php \App\Post::joinRelationship('author')->first()->tit^le"#).contains(&"title".into()));
        assert!(!complete(&index,r#"<?php \App\User::withExpression('rows',fn($q)=>$q)->first()->em^ail"#).contains(&"email".into()));
        assert!(complete(&index,r#"<?php \App\Post::withExpression('rows',fn($q)=>$q)->first()->tit^le"#).contains(&"title".into()));
        assert!(host.process_runs().is_empty());assert!(host.requests().is_empty());
    }
}
