//! PHP 8.5 pipe flow from a proven input and source-declared callable. Cached only.
use super::*;
#[derive(Clone,Debug)]pub(crate) enum Return {Declared(String),Input,Receiver,Constructed(String)}
#[derive(Clone,Debug)]pub(crate) struct Callable {
    pub full:String,pub path:String,pub entry:Entry,pub ambiguous:bool,
    pub parameter:Option<String>,pub required:usize,pub parameters:usize,pub by_reference:bool,pub result:Return,
}
pub(crate) type Functions=BTreeMap<String,Callable>;
fn arrow_parameters(source:&Source,names:&Names,function:&Function,path:&str)->Option<Callable>{
    let args=container::arguments(source,function.params.start-1)?;let [range]=args.as_slice()else{return None};let tokens=&source.tokens[range.clone()];
    let variable=tokens.iter().position(|token|token.kind==Kind::Var)?;
    if variable+1!=tokens.len()||tokens.iter().any(|token|matches!(token.value,"="|"&"|"...")){return None;}
    let parameter=if variable==0{None}else{Some(helper_facts::declared_type(&tokens[..variable].iter().collect::<Vec<_>>(),names,"__OpenIDEA_Pipe_Receiver__",false)?)};
    Some(Callable{full:String::new(),path:path.into(),entry:entry(source,path,function.start),ambiguous:false,parameter,required:1,parameters:1,by_reference:false,result:Return::Input})
}
fn signature(source:&Source,names:&Names,function:&Function,path:&str,full:String)->Option<Callable>{
    let args=container::arguments(source,function.params.start.checked_sub(1)?)?;
    if args.is_empty()||args.len()>32{return None;}
    let mut required=0;let mut first=None;let mut variable=None;
    for(index,range)in args.iter().enumerate(){
        let tokens=&source.tokens[range.clone()];
        if tokens.iter().any(|token|matches!(token.value,"&"|"...")){return None;}
        let at=tokens.iter().position(|token|token.kind==Kind::Var)?;
        if !tokens.iter().any(|token|token.value=="="){required+=1;}
        if index==0{
            variable=Some(tokens[at].value);
            if at>0{let types=tokens[..at].iter().collect::<Vec<_>>();first=Some(helper_facts::declared_type(&types,names,"__OpenIDEA_Pipe_Receiver__",names.class.as_ref().is_some_and(|(_,at,_,_)|source.is(*at,"trait")))?);}
        }
    }
    if required>1{return None;}
    let result=if source.is(function.params.end+1,":"){
        let tokens=source.tokens[function.params.end+2..function.body].iter().collect::<Vec<_>>();
        Return::Declared(helper_facts::declared_type(&tokens,names,"__OpenIDEA_Pipe_Receiver__",names.class.as_ref().is_some_and(|(_,at,_,_)|source.is(*at,"trait")))?)
    }else{
        let b=function.body+1;
        let expression=if source.is(function.body,"=>"){b..function.end}else{if !source.is(b,"return")||!source.is(function.end.saturating_sub(1),";"){return None;}b+1..function.end-1};
        if expression.end==expression.start+1&&source.tokens[expression.start].value==variable?{Return::Input}
        else if expression.end==expression.start+1&&source.is(expression.start,"$this"){Return::Receiver}
        else if source.is(expression.start,"new")&&source.tokens.get(expression.start+1)?.kind==Kind::Name{
            let at=expression.start+1;let end=if source.is(at+1,"("){source.pairs.get(&(at+1)).copied()?+1}else{at+1};if end!=expression.end{return None;}Return::Constructed(names.resolve(source.tokens[at].value))
        }else{return None;}
    };
    Some(Callable{full,path:path.into(),entry:entry(source,path,function.name.unwrap_or(function.start)),ambiguous:false,parameter:first,required,parameters:args.len(),by_reference:false,result})
}
pub(crate) fn cache(functions:&mut Functions,text:&str,path:&str){
    let Some(source)=Source::new(text)else{return};let Some(names)=Names::parse(&source)else{return};
    for function in source.functions().into_iter().filter(|function|function.name.is_some()&&source.paths[function.start].is_empty()).take(256){
        let full=names.qualify(source.tokens[function.name.unwrap()].value);
        let Some(info)=signature(&source,&names,&function,path,full.clone())else{continue};
        functions.entry(full.to_ascii_lowercase()).and_modify(|old|{if old.path!=path||old.entry.at!=info.entry.at{old.ambiguous=true;}}).or_insert(info);
    }
}
pub(super) fn methods(source:&Source,names:&Names,path:&str)->BTreeMap<String,Callable>{
    let Some((full,_,body,_))=names.class.as_ref()else{return BTreeMap::new()};
    source.functions().into_iter().filter(|function|function.name.is_some()&&source.paths[function.start]==vec![*body]).filter_map(|function|{
        let name=source.tokens[function.name?].value;
        let info=signature(source,names,&function,path,format!("{full}::{name}"))?;Some((name.into(),info))
    }).collect()
}
impl Resolver<'_,'_>{
    fn pipe_input_class(&self,typed:&Typed)->Option<String>{Some(match &typed.shape{
        Shape::Static|Shape::HigherOrder(..)=>return None,
        Shape::Optional(_)=>"Illuminate\\Support\\Optional".into(),
        Shape::Conditional{..}=>"Illuminate\\Support\\HigherOrderWhenProxy".into(),
        Shape::Factory{class,..}|Shape::MappedCollection(class)|Shape::JoinClause(class)=>class.clone(),
        Shape::Query=>self.custom_class(typed).unwrap_or_else(||"Illuminate\\Database\\Eloquent\\Builder".into()),
        Shape::Collection=>self.custom_class(typed).unwrap_or_else(||"Illuminate\\Database\\Eloquent\\Collection".into()),
        Shape::Relation(kind)=>if kind.contains('\\'){kind.clone()}else{format!("Illuminate\\Database\\Eloquent\\Relations\\{}{}",kind.chars().next()?.to_uppercase(),&kind[1..])},
        _=>typed.model.clone(),
    })}
    pub(super) fn pipe_accepts(&self,signature:&Callable,input:&Typed)->bool{
        if signature.by_reference||signature.required>1||signature.parameters==0{return false;}
        let Some(parameter)=signature.parameter.as_deref()else{return true};
        let Some(actual)=self.pipe_input_class(input)else{return false};
        parameter.trim_start_matches('?').split('|').any(|part|{
            if matches!(part,"mixed"|"object"){return true;}
            part.split('&').all(|expected|{
                let expected=expected.trim_start_matches('\\');
                if actual.eq_ignore_ascii_case(expected)||self.subclass(&actual,expected){return true;}
                let mut pending=vec![actual.as_str()];let mut seen=BTreeSet::new();
                while let Some(full)=pending.pop(){if seen.len()>=32{return false;}if !seen.insert(full.to_ascii_lowercase()){continue;}if full.eq_ignore_ascii_case(expected){return true;}if let Some(object)=self.object(full){pending.extend(object.parents.iter().map(String::as_str));}}
                false
            })
        })
    }
    fn pipe_result(&self,signature:&Callable,input:&Typed,receiver:Option<&Typed>)->Option<Typed>{
        let mut signature=signature.clone();
        if let Some(parameter)=signature.parameter.as_mut(){*parameter=parameter.replace("__OpenIDEA_Pipe_Receiver__",receiver.map(|receiver|receiver.model.as_str()).unwrap_or(""));}
        if !self.pipe_accepts(&signature,input){return None;}
        match &signature.result{
            Return::Input=>Some(input.clone()),Return::Receiver=>receiver.cloned().filter(|receiver|receiver.shape!=Shape::Static),
            Return::Declared(target)=>{let replacement=receiver.map(|receiver|receiver.model.as_str()).unwrap_or("");let target=target.replace("__OpenIDEA_Pipe_Receiver__",replacement);self.accessor_result(&target,receiver.unwrap_or(input))},
            Return::Constructed(target)=>self.return_target(target,receiver.unwrap_or(input)),
        }
    }
    pub(super) fn pipe_binding(&self,function:&Function,name:&str,depth:u8)->Option<Typed>{
        let source=&self.context.source;
        let start=function.start.checked_sub(usize::from(function.start>0&&source.is(function.start-1,"static")))?;
        let wrapper=start.checked_sub(1)?;
        if !source.is(wrapper,"(")||wrapper==0||!source.is(wrapper-1,"|>"){return None;}
        let signature=signature(source,&self.context.names,function,self.path,String::new()).or_else(||arrow_parameters(source,&self.context.names,function,self.path));
        let args=container::arguments(source,function.params.start-1)?;let [first]=args.as_slice()else{return None};
        if !source.tokens[first.clone()].iter().any(|token|token.kind==Kind::Var&&token.value==name){return None;}
        let(input,_)=self.expr(wrapper-1,depth+1)?;
        if let Some(signature)=signature {return self.pipe_accepts(&signature,&input).then_some(input);}
        // Unfinished bodies still have an exact one-parameter signature.
        let tokens=&source.tokens[first.clone()];let variable=tokens.iter().position(|token|token.kind==Kind::Var)?;
        if tokens.iter().any(|token|matches!(token.value,"="|"&"|"...")){return None;}
        if variable==0{return Some(input);}
        let types=tokens[..variable].iter().collect::<Vec<_>>();let parameter=helper_facts::declared_type(&types,&self.context.names,"__OpenIDEA_Pipe_Receiver__",false)?;
        let stub=Callable{full:String::new(),path:self.path.into(),entry:entry(source,self.path,function.start),ambiguous:false,parameter:Some(parameter),required:1,parameters:1,by_reference:false,result:Return::Input};
        self.pipe_accepts(&stub,&input).then_some(input)
    }
    pub(super) fn pipe_expr(&self,end:usize,depth:u8)->Option<(Typed,usize)>{
        let source=&self.context.source;let last=end.checked_sub(1)?;
        if !source.is(last,")"){return None;}
        let open=*source.pairs.get(&last)?;
        if open>0&&source.is(open-1,"|>"){
            let function=source.functions().into_iter().find(|function|function.name.is_none()&&function.start>=open+1&&function.start<=open+2&&(function.end==last||source.is(function.body,"{")&&function.end+1==last))?;
            let(input,start)=self.expr(open-1,depth+1)?;
            let signature=signature(source,&self.context.names,&function,self.path,String::new()).or_else(||arrow_parameters(source,&self.context.names,&function,self.path))?;
            let result=if source.is(function.body,"=>"){
                let(result,first)=self.expr(function.end,depth+1)?;
                if first!=function.body+1{return None;}result
            }else{
                let b=function.body+1;if !source.is(b,"return")||!source.is(function.end-1,";"){return None;}
                let(result,first)=self.expr(function.end-1,depth+1)?;if first!=b+1{return None;}result
            };
            return self.pipe_accepts(&signature,&input).then_some((result,start));
        }
        if !source.is(open+1,"...")||open+2!=last{return None;}
        if open>=3&&matches!(source.tokens[open-2].value,"::"|"->"|"?->"){
            let(receiver,first)=self.expr(open-2,depth+1)?;
            if first==0||!source.is(first-1,"|>"){return None;}
            let(input,start)=self.expr(first-1,depth+1)?;
            let method=self.object_method(&receiver.model,source.tokens[open-1].value)?;
            if method.static_method!=(receiver.shape==Shape::Static){return None;}
            let callable=self.index.eloquent_models.values().filter(|model|!model.ambiguous&&self.owned(&model.path)).flat_map(|model|model.semantic.callables.values()).find(|callable|callable.entry.path==method.entry.path&&callable.entry.at==method.entry.at)?;
            return self.pipe_result(callable,&input,Some(&receiver)).map(|result|(result,start));
        }
        let first=open.checked_sub(1)?;
        if first==0||!source.is(first-1,"|>"){return None;}
        let(input,start)=self.expr(first-1,depth+1)?;
        let identity=metadata_inference::call_identity(source.text,source.tokens[open].start)?;
        if identity.class.is_some(){return None;}
        let full=identity.function_fqn.unwrap_or_else(||self.context.names.qualify(&identity.method));
        let mut current=Functions::new();cache(&mut current,source.text,self.path);
        if current.contains_key(&full.to_ascii_lowercase())&&self.index.eloquent_functions.get(&full.to_ascii_lowercase()).is_some_and(|fact|fact.ambiguous||fact.path!=self.path){return None;}
        let callable=current.get(&full.to_ascii_lowercase()).or_else(||self.index.eloquent_functions.get(&full.to_ascii_lowercase())).filter(|callable|!callable.ambiguous&&self.owned(&callable.path))?;
        self.pipe_result(callable,&input,None).map(|result|(result,start))
    }
}
#[cfg(test)]mod tests{
    use super::*;
    fn index()->Index{
        let mut index=Index::default();
        for(path,source)in [
            ("app/Models/User.php",r#"<?php namespace App\Models;class User extends \Illuminate\Database\Eloquent\Model{}"#),
            ("app/Services/Flow.php",r#"<?php namespace App\Services;use App\Models\User;class Flow{public static function keep(User $user):User{return $user;}public function keepInstance(User $user):User{return $user;}}"#),
        ]{let path=format!("/project/{path}");super::super::cache(&mut index.eloquent_models,source,&path);cache_object(&mut index.eloquent_objects,source,&path);}
        cache(&mut index.eloquent_functions,r#"<?php namespace Flow;use App\Models\User;function keep(User $user):User{return $user;}function identity($user){return $user;}function bad(User $user,$required):User{return $user;}function reference(User &$user):User{return $user;}"#,"/project/app/Support/pipe.php");
        index.columns.entry("users".into()).or_default().insert("name".into(),Entry{path:"/project/database/migrations/users.php".into(),at:Position::new(0,0),width:4,detail:Some("string".into())});index
    }
    fn labels(index:&Index,source:&str)->Vec<String>{let(text,at)=crate::tests::cursor(source);completion_checked(index,&crate::tests::doc("/project/Usage.php","php",&text),at,&|_|true).into_iter().map(|item|item.label).collect()}
    #[test]fn model_pipe_parameters_identities_and_chains_are_proven_without_php(){
        let index=index();
        for source in [
            r#"<?php \App\Models\User::first() |> (fn(\App\Models\User $u)=>$u->na^me);"#,
            r#"<?php \App\Models\User::first() |> (fn($u)=>$u->na^me);"#,
            r#"<?php (\App\Models\User::first() |> (fn($u)=>$u))->na^me"#,
            r#"<?php (\App\Models\User::first() |> (fn($u)=>$u->fresh()))->na^me"#,
            r#"<?php (\App\Models\User::first() |> (fn($u)=>$u) |> \Flow\keep(...))->na^me"#,
            r#"<?php use function Flow\keep as keepUser;(\App\Models\User::first() |> keepUser(...))->na^me"#,
            r#"<?php (\App\Models\User::first() |> \App\Services\Flow::keep(...))->na^me"#,
            r#"<?php (\App\Models\User::first() |> (new \App\Services\Flow)->keepInstance(...))->na^me"#,
        ]{assert!(labels(&index,source).contains(&"name".into()),"{source}");}
    }
    #[test]fn unknown_callable_inputs_wrong_arity_references_and_noninvoked_callables_are_withheld(){
        let index=index();
        for source in [
            r#"<?php unknown() |> (fn($u)=>$u->na^me);"#,
            r#"<?php (\App\Models\User::first() |> \Flow\bad(...))->na^me"#,
            r#"<?php (\App\Models\User::first() |> \Flow\reference(...))->na^me"#,
            r#"<?php $callable=\App\Models\User::first(...);$callable->na^me"#,
            r#"<?php \App\Models\User::first() |> (function($u){$u=null;return $u->na^me;});"#,
        ]{assert!(labels(&index,source).is_empty(),"{source}");}
        let(text,at)=crate::tests::cursor(r#"<?php (\App\Models\User::first() |> \Flow\keep(...))->na^me"#);
        assert!(completion_checked(&index,&crate::tests::doc("/project/Usage.php","php",&text),at,&|path|!path.ends_with("pipe.php")).is_empty());
    }
    #[test]fn current_and_cached_function_identity_returns_preserve_declaration_ownership(){
        let index=index();
        assert!(labels(&index,r#"<?php (\App\Models\User::first() |> \Flow\identity(...))->na^me"#).contains(&"name".into()));
        assert!(labels(&index,r#"<?php function keep($value){return $value;}(\App\Models\User::first() |> keep(...))->na^me"#).contains(&"name".into()));
        assert!(labels(&index,r#"<?php use function Other\keep;(\App\Models\User::first() |> keep(...))->na^me"#).is_empty());
    }
    #[test]fn registered_pipe_provider_refreshes_cached_functions_without_php_or_network(){
        use openidea_plugin::{Plugin,providers::Providers,Event,mock::MockHost};
        let host=MockHost::install();host.add_file("artisan","<?php");host.add_file("composer.json",r#"{"require":{"laravel/framework":"^12.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#);
        host.add_file("app/Models/User.php",r#"<?php namespace App\Models;class User extends \Illuminate\Database\Eloquent\Model{}"#);
        let helper=r#"<?php namespace Flow;use App\Models\User;function keep(User $user):User{return $user;}"#;
        host.add_file("app/Support/pipe.php",helper);host.add_file("database/migrations/users.php","<?php Schema::create('users',function($table){$table->string('name');});");
        let mut plugin=crate::Main::default();plugin.activate(&host.context("aritti.laravel")).unwrap();crate::tests::finish_indexing(&mut plugin);
        let(text,at)=crate::tests::cursor(r#"<?php (\App\Models\User::first() |> \Flow\keep(...))->na^me"#);let doc=crate::tests::doc("/project/Usage.php","php",&text);
        assert!(plugin.completion(&doc,at,None).iter().any(|item|item.label=="name"));assert_eq!(plugin.definition(&doc,at)[0].path,"/project/database/migrations/users.php");
        host.add_file("app/Support/pipe.php",&helper.replace(":User{return $user;}",":int{return 1;}"));
        plugin.handle_events(&[Event::FileSaved("/project/app/Support/pipe.php".into())],&host.invocation());
        assert!(!plugin.completion(&doc,at,None).iter().any(|item|item.label=="name"));
        assert!(host.process_runs().is_empty());assert!(host.requests().is_empty());
    }
}
