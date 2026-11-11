//! Source-proven provider bindings, factory expressions and facade targets. Indexed only.
use super::*;
#[derive(Clone,Debug)]
pub(super) enum Target{Class(String),Alias(String),Unknown}
#[derive(Clone,Debug)]
pub(super) struct Binding{pub key:String,pub target:Target,pub context:Option<String>,pub entry:Entry}
#[derive(Clone,Debug,Default)]
pub(super) struct Declared{pub bindings:Vec<Binding>,pub facade:Option<String>,pub attribute:Option<Target>}
fn key(source:&Source,names:&Names,from:usize,to:usize)->Option<String>{
    if to==from+1&&source.tokens.get(from)?.kind==Kind::String{
        let token=&source.tokens[from];let (value,_)=php::literal(source.text,token.start..token.end)?;
        return metadata_inference::decode_literal(value,token.value.as_bytes()[0]);
    }
    if to==from+3&&source.is(from+1,"::")&&source.is(from+2,"class")&&source.tokens.get(from)?.kind==Kind::Name{return Some(names.resolve(source.tokens[from].value));}
    None
}
pub(super) fn arguments(source:&Source,open:usize)->Option<Vec<std::ops::Range<usize>>>{
    let close=*source.pairs.get(&open)?;let mut at=open+1;let mut begin=at;let mut result=Vec::new();
    while at<close{if source.is(at,","){result.push(begin..at);begin=at+1;at+=1;}else if let Some(end)=source.pairs.get(&at).filter(|end|**end>at){at=end+1;}else{at+=1;}}
    if begin<close{result.push(begin..close);}Some(result)
}
// Factories are inspected as expressions, never executed. A returned string is a
// value rather than a class name; only binding arguments accept class strings.
fn expression(source:&Source,names:&Names,range:std::ops::Range<usize>,app:Option<&str>)->Option<Target>{
    let from=range.start;let to=range.end;
    if source.is(from,"(")&&source.pairs.get(&from)==Some(&(to-1)){return expression(source,names,from+1..to-1,app);}
    if source.is(from,"new")&&source.tokens.get(from+1)?.kind==Kind::Name{
        if to==from+2||source.is(from+2,"(")&&source.pairs.get(&(from+2))==Some(&(to-1)){
            let full=names.resolve(source.tokens[from+1].value);
            return (!matches!(source.tokens[from+1].value,"self"|"static"|"parent")).then_some(Target::Class(full));
        }
    }
    if source.tokens.get(from)?.kind==Kind::Name&&source.is(from+1,"(")&&source.pairs.get(&(from+1))==Some(&(to-1)){
        let identity=metadata_inference::call_identity(source.text,source.tokens[from+1].start)?;
        let full=identity.function_fqn.as_deref().unwrap_or(&identity.method);
        if identity.class.is_none()&&!identity.constructor&&matches!(full,"app"|"resolve")&&
            !source.functions().iter().any(|function|function.name.is_some_and(|at|source.tokens[at].value.eq_ignore_ascii_case(full))&&source.paths[function.start].is_empty()){
            let args=arguments(source,from+1)?;
            if args.len()==1{return key(source,names,args[0].start,args[0].end).map(Target::Alias);}
        }
    }
    if app.is_some_and(|app|source.is(from,app))&&source.is(from+1,"->")&&matches!(source.tokens.get(from+2)?.value,"make"|"get")&&source.is(from+3,"(")&&source.pairs.get(&(from+3))==Some(&(to-1)){
        let args=arguments(source,from+3)?;
        if args.len()==1{return key(source,names,args[0].start,args[0].end).map(Target::Alias);}
    }
    None
}
fn concrete(source:&Source,names:&Names,range:std::ops::Range<usize>)->Option<Target>{
    if let Some(value)=key(source,names,range.start,range.end){return Some(Target::Alias(value));}
    if let Some(target)=expression(source,names,range.clone(),None){return Some(target);}
    let start=range.start+usize::from(source.is(range.start,"static"));
    let function=source.functions().into_iter().find(|function|function.start==start&&function.name.is_none()&&
        if source.is(start,"fn"){function.end==range.end}else{function.end+1==range.end})?;
    let args=arguments(source,function.params.start-1)?;
    let app=args.first().and_then(|range|source.tokens[range.clone()].iter().find(|token|token.kind==Kind::Var)).map(|token|token.value);
    if source.is(start,"fn"){return expression(source,names,function.body+1..function.end,app);}
    let b=function.body+1;
    if !source.is(b,"return")||!source.is(function.end-1,";"){return None;}
    expression(source,names,b+1..function.end-1,app)
}
fn contexts(source:&Source,names:&Names,range:std::ops::Range<usize>)->Option<Vec<String>>{
    if !source.is(range.start,"["){return key(source,names,range.start,range.end).map(|key|vec![key]);}
    if source.pairs.get(&range.start)!=Some(&(range.end-1)){return None;}
    let items=arguments(source,range.start)?;
    let mut result=Vec::new();
    for item in items{let value=key(source,names,item.start,item.end)?;if !result.contains(&value){result.push(value);}}
    (!result.is_empty()).then_some(result)
}
fn binding_attribute(source:&Source,names:&Names,class:usize)->Option<Target>{
    let mut found=Vec::new();
    for at in 0..class{
        if !source.is(at,"#")||!source.is(at+1,"["){continue;}
        for item in arguments(source,at+1).unwrap_or_default(){
            let attribute=names.resolve(source.tokens[item.start].value);
            if !matches!(attribute.as_str(),"Illuminate\\Container\\Attributes\\Bind"|"Illuminate\\Container\\Attributes\\BindWhen"){continue;}
            let target=if attribute.ends_with("\\Bind")&&source.is(item.start+1,"("){
                arguments(source,item.start+1).filter(|args|args.len()==1).and_then(|args|key(source,names,args[0].start,args[0].end)).map(Target::Alias).unwrap_or(Target::Unknown)
            }else{Target::Unknown};
            found.push(target);
        }
    }
    if found.len()==1{found.pop()}else if found.is_empty(){None}else{Some(Target::Unknown)}
}
pub(super) fn declared(source:&Source,names:&Names,path:&str)->Declared{
    let mut result=Declared::default();let Some((_,class,body,_))=names.class.as_ref()else{return result};
    result.attribute=binding_attribute(source,names,*class);
    for(at,token)in source.tokens.iter().enumerate().filter(|(at,token)|source.paths[*at]==vec![*body]&&matches!(token.value,"$bindings"|"$singletons")){
        if !source.is(at+1,"=")||!source.is(at+2,"["){continue;}
        for item in php::items(source.text,source.tokens[at+2].start){
            let Some(key_range)=item.key else{continue};
            let key_tokens=source.tokens.iter().enumerate().filter(|(_,token)|token.start>=key_range.start&&token.end<=key_range.end).map(|(at,_)|at).collect::<Vec<_>>();
            let value_tokens=source.tokens.iter().enumerate().filter(|(_,token)|token.start>=item.value.start&&token.end<=item.value.end).map(|(at,_)|at).collect::<Vec<_>>();
            let(Some(first),Some(last),Some(value_first),Some(value_last))=(key_tokens.first(),key_tokens.last(),value_tokens.first(),value_tokens.last())else{continue};
            if let(Some(key),Some(target))=(key(source,names,*first,*last+1),concrete(source,names,*value_first..*value_last+1)){
                result.bindings.push(Binding{key,target,context:None,entry:entry(source,path,at)});
            }
        }
        let _=token;
    }
    for function in source.functions().into_iter().filter(|function|source.paths[function.start]==vec![*body]){
        let Some(name)=function.name else{continue};
        if source.is(name,"getFacadeAccessor"){
            let from=function.body+2;let to=function.end.checked_sub(1).unwrap_or(0);
            if source.is(function.body+1,"return")&&source.is(to,";"){result.facade=key(source,names,from,to);}
        }
        if !matches!(source.tokens[name].value,"register"|"boot"){continue;}
        for at in function.body+1..function.end{
            if source.paths[at]!=vec![*body,function.body]{continue;}
            let method=if source.is(at,"$this")&&source.is(at+1,"->")&&source.is(at+2,"app")&&source.is(at+3,"->"){at+4}
                else if source.tokens[at].kind==Kind::Name&&names.resolve(source.tokens[at].value)=="Illuminate\\Support\\Facades\\App"&&source.is(at+1,"::"){at+2}else{continue};
            if !source.is(method+1,"("){continue;}
            let Some(args)=arguments(source,method+1)else{continue};
            if source.is(method,"when")&&args.len()==1{
                let contexts=contexts(source,names,args[0].clone()).unwrap_or_else(||vec!["*".into()]);
                let Some(close)=source.pairs.get(&(method+1)).copied()else{continue};
                if !source.is(close+1,"->")||!source.is(close+2,"needs")||!source.is(close+3,"("){continue;}
                let Some(needs)=arguments(source,close+3).filter(|args|args.len()==1)else{continue};
                let Some(key)=key(source,names,needs[0].start,needs[0].end)else{continue};
                let Some(close)=source.pairs.get(&(close+3)).copied()else{continue};
                if !source.is(close+1,"->")||!matches!(source.tokens.get(close+2).map(|token|token.value),Some("give"|"giveTagged"|"giveConfig"))||!source.is(close+3,"("){continue;}
                let Some(gives)=arguments(source,close+3).filter(|args|args.len()==1)else{continue};
                let target=if source.is(close+2,"give")&&!contexts.iter().any(|context|context=="*"){concrete(source,names,gives[0].clone()).unwrap_or(Target::Unknown)}else{Target::Unknown};
                for context in contexts{result.bindings.push(Binding{key:key.clone(),target:target.clone(),context:Some(context),entry:entry(source,path,method)});}
                continue;
            }
            if args.len()!=2{continue;}
            let Some(first)=key(source,names,args[0].start,args[0].end)else{continue};
            let target=match source.tokens[method].value{
                "bind"|"singleton"|"scoped"=>Some(concrete(source,names,args[1].clone()).unwrap_or(Target::Unknown)),
                "instance"=>Some(expression(source,names,args[1].clone(),None).filter(|target|matches!(target,Target::Class(_))).unwrap_or(Target::Unknown)),
                "alias"=>key(source,names,args[1].start,args[1].end).map(|alias|{result.bindings.push(Binding{key:alias,target:Target::Alias(first.clone()),context:None,entry:entry(source,path,method)});Target::Alias(String::new())}),
                _=>None,
            };
            if let Some(target)=target{if !matches!(&target,Target::Alias(value)if value.is_empty()){result.bindings.push(Binding{key:first,target,context:None,entry:entry(source,path,method)});}}
        }
    }result
}
impl Resolver<'_,'_>{
    pub(super) fn container_target(&self,key:&str,context:Option<&str>)->Option<String>{
        let mut key=key.to_owned();let mut seen=BTreeSet::new();
        for _ in 0..8{
            if !seen.insert(key.clone()){return None;}
            let mut bindings=Vec::new();
            let mut providers=self.index.eloquent_models.values().filter(|class|!self.overlay.as_ref().is_some_and(|overlay|overlay.full.eq_ignore_ascii_case(&class.full))).filter_map(|class|self.raw(&class.full)).collect::<Vec<_>>();
            if let Some(overlay)=&self.overlay{if let Some(current)=self.raw(&overlay.full){providers.push(current);}}
            for class in providers.into_iter().filter(|class|!class.ambiguous&&self.subclass(&class.full,"Illuminate\\Support\\ServiceProvider")){
                for binding in &class.semantic.container.bindings{if binding.key==key&&(binding.context.as_deref()==context||context.is_some()&&binding.context.as_deref()==Some("*"))&&self.owned(&binding.entry.path){bindings.push(binding);}}
            }
            if bindings.is_empty()&&context.is_some(){return self.container_target(&key,None);}
            match bindings.as_slice(){
                []=>{
                    let attribute=self.object(&key).and_then(|object|object.container.attribute.as_ref()).or_else(||self.raw(&key).and_then(|class|class.semantic.container.attribute.as_ref()));
                    if let Some(attribute)=attribute{
                        if !self.index.metadata.framework_version.is_some_and(|version|version>=(12,22,0)){return None;}
                        match attribute{Target::Alias(alias)=>{key=alias.clone();continue;},_=>return None}
                    }
                    return self.class_for_target(&key);
                },
                [binding]=>match &binding.target{Target::Class(full)=>return self.class_for_target(full),Target::Alias(alias)=>{if alias==&key{return self.class_for_target(&key);}key=alias.clone()},Target::Unknown=>return None},
                _=>return None,
            }
        }None
    }
    fn class_for_target(&self,full:&str)->Option<String>{self.get(full).map(|class|class.full.clone()).or_else(||self.object(full).filter(|object|!object.is_interface).map(|object|object.full.clone()))}
    pub(super) fn facade_target(&self,full:&str)->Option<String>{
        if let Some(target)=full.strip_prefix("Facades\\"){return self.class_for_target(target);}
        if !self.subclass(full,"Illuminate\\Support\\Facades\\Facade"){return None;}
        let mut current=full;
        for _ in 0..8{
            let class=self.raw(current)?;
            if class.declared.contains("getFacadeAccessor"){return self.container_target(class.semantic.container.facade.as_deref()?,None);}
            current=&class.parent;
        }None
    }
    // A recognized contextual attribute owns resolution; an unknown target must
    // not fall through to an unrelated default interface binding.
    pub(super) fn container_parameter(&self,function:&Function,name:&str)->Option<Option<Typed>>{
        let source=&self.context.source;
        let parameter=arguments(source,function.params.start.checked_sub(1)?)?.into_iter().find(|range|source.tokens[range.clone()].iter().any(|token|token.kind==Kind::Var&&token.value==name))?;
        let mut attributes=Vec::new();
        for at in parameter.clone(){
            if !source.is(at,"#")||!source.is(at+1,"["){continue;}
            for item in arguments(source,at+1).unwrap_or_default(){
                let full=self.context.names.resolve(source.tokens[item.start].value);
                if full.starts_with("Illuminate\\Container\\Attributes\\"){
                    attributes.push((full,item));
                }
            }
        }
        if attributes.is_empty(){return None;}
        let [(attribute,item)]=attributes.as_slice()else{return Some(None)};
        if attribute!="Illuminate\\Container\\Attributes\\Give"||!self.index.metadata.framework_version.is_some_and(|version|version>=(12,17,0)){return Some(None);}
        let target=(||{
            if !source.is(item.start+1,"("){return None;}
            let args=arguments(source,item.start+1)?;
            if args.len()!=1{return None;}
            let key=key(source,&self.context.names,args[0].start,args[0].end)?;
            let model=self.container_target(&key,None)?;
            Some(Typed{shape:if self.get(&model).is_some(){Shape::Model}else{Shape::Object},model})
        })();
        Some(target)
    }
    pub(super) fn helper_target(&self,open:usize,close:usize)->Option<Typed>{
        let source=&self.context.source;
        let identity=metadata_inference::call_identity(source.text,source.tokens[open].start)?;
        if identity.class.is_some()||identity.constructor{return None;}
        let function=identity.function_fqn.as_deref().unwrap_or(&identity.method);
        if !matches!(function,"app"|"resolve"){return None;}
        if source.functions().iter().any(|function_decl|function_decl.name.is_some_and(|at|source.tokens[at].value.eq_ignore_ascii_case(function))&&source.paths[function_decl.start].is_empty()){return None;}
        let args=arguments(source,open)?;let first=args.first()?;
        if args.len()>2||close!=*source.pairs.get(&open)?{return None;}
        let key=key(source,&self.context.names,first.start,first.end)?;
        let full=self.container_target(&key,None)?;
        Some(Typed{shape:if self.get(&full).is_some(){Shape::Model}else{Shape::Object},model:full})
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    fn cache_source(index:&mut Index,path:&str,source:&str){index.eloquent_models.retain(|_,class|class.path!=path);index.eloquent_objects.retain(|_,class|class.path!=path);cache(&mut index.eloquent_models,source,path);cache_object(&mut index.eloquent_objects,source,path);}
    fn index(provider:&str)->Index{
        let mut index=Index::default();index.metadata.framework_version=Some((13,40,0));
        cache_source(&mut index,"/project/app/Services/Posts.php",r#"<?php namespace App\Services;class Posts{public function selected():\App\Models\Post{return new \App\Models\Post;}}"#);
        cache_source(&mut index,"/project/app/Models/Post.php",r#"<?php namespace App\Models;class Post extends \Illuminate\Database\Eloquent\Model{}"#);
        cache_source(&mut index,"/project/app/Contracts/Posts.php",r#"<?php namespace App\Contracts;interface Posts{}"#);
        cache_source(&mut index,"/project/app/Providers/Bindings.php",provider);
        index.columns.entry("posts".into()).or_default().insert("title".into(),Entry{path:"/project/database/migrations/posts.php".into(),at:Position::new(0,0),width:5,detail:Some("string".into())});
        finish(&mut index.eloquent_models);index
    }
    fn labels(index:&Index,source:&str)->Vec<String>{let(text,at)=crate::tests::cursor(source);completion_checked(index,&crate::tests::doc("/project/Test.php","php",&text),at,&|_|true).into_iter().map(|item|item.label).collect()}
    #[test]
    fn container_context_arrays_factories_and_unresolved_bindings_fail_closed(){
        let index=index(r#"<?php namespace App\Providers;class Bindings extends \Illuminate\Support\ServiceProvider{public function register(){
            $this->app->bind('base',static fn()=>new \App\Services\Posts);
            $this->app->singleton('alias',fn($app)=>$app->make('base'));
            $this->app->scoped('closure',function($app){return $app->make('base');});
            $this->app->bind('helper',fn()=>app('base'));
            $this->app->bind('literal.alias','base');
            $this->app->instance('instance',new \App\Services\Posts);
            $this->app->instance('invalid.instance',fn()=>new \App\Services\Posts);
            $this->app->when([\App\Http\First::class,\App\Http\Second::class])->needs(\App\Contracts\Posts::class)->give(fn()=>new \App\Services\Posts());
            $this->app->bind('dynamic',fn()=>config('service'));
            $this->app->bind('string',fn()=>\App\Services\Posts::class);
        }}"#);
        for key in ["base","alias","closure","helper","literal.alias","instance"]{assert!(labels(&index,&format!("<?php app('{key}')->selected()->tit^le")).contains(&"title".into()),"{key}");}
        for name in ["First","Second"]{assert!(labels(&index,&format!(r#"<?php namespace App\Http;class {name}{{public function __construct(\App\Contracts\Posts $posts){{$posts->selected()->tit^le;}}}}"#)).contains(&"title".into()),"{name}");}
        for key in ["dynamic","string","invalid.instance"]{assert!(labels(&index,&format!("<?php app('{key}')->selected()->tit^le")).is_empty(),"{key}");}
        assert!(labels(&index,r#"<?php namespace App\Http;class Third{public function __construct(\App\Contracts\Posts $posts){$posts->selected()->tit^le;}}"#).is_empty());
    }
    #[test]
    fn container_attributes_are_owned_literal_and_version_gated(){
        let mut index=index("<?php");
        cache_source(&mut index,"/project/app/Contracts/Posts.php",r#"<?php namespace App\Contracts;#[\Illuminate\Container\Attributes\Bind(\App\Services\Posts::class)] interface Posts{}"#);
        assert!(labels(&index,r#"<?php app(\App\Contracts\Posts::class)->selected()->tit^le"#).contains(&"title".into()));
        let give=r#"<?php namespace App\Http;class Consumer{public function __construct(#[\Illuminate\Container\Attributes\Give(\App\Services\Posts::class)] \App\Contracts\Posts $posts){$posts->selected()->tit^le;}}"#;
        assert!(labels(&index,give).contains(&"title".into()));
        index.metadata.framework_version=Some((12,21,0));assert!(labels(&index,r#"<?php app(\App\Contracts\Posts::class)->selected()->tit^le"#).is_empty());
        assert!(labels(&index,give).contains(&"title".into()));
        index.metadata.framework_version=Some((12,16,0));assert!(labels(&index,give).is_empty());
        index.metadata.framework_version=None;assert!(labels(&index,give).is_empty());
        index.metadata.framework_version=Some((13,40,0));
        cache_source(&mut index,"/project/app/Contracts/Posts.php",r#"<?php namespace App\Contracts;#[\Illuminate\Container\Attributes\Bind(\App\Services\Posts::class,['local'])] interface Posts{}"#);
        assert!(labels(&index,r#"<?php app(\App\Contracts\Posts::class)->selected()->tit^le"#).is_empty());
        assert!(labels(&index,&give.replace("Give(\\App\\Services\\Posts::class)","Give(config('service'))")).is_empty());
    }
    #[test]
    fn container_foreign_dynamic_ambiguous_stale_and_unsaved_facts_withhold_guesses(){
        for body in [
            r#"$this->app->bind('posts',fn()=>\Foreign\app('base'));"#,
            r#"$this->app->bind('posts',fn()=>new \App\Services\Posts);$this->app->bind('posts',fn()=>config('service'));"#,
            r#"$this->app->alias('posts','cycle');$this->app->alias('cycle','posts');"#,
        ]{
            let index=index(&format!(r#"<?php namespace App\Providers;class Bindings extends \Illuminate\Support\ServiceProvider{{public function register(){{{body}}}}}"#));
            assert!(labels(&index,r#"<?php app('posts')->selected()->tit^le"#).is_empty(),"{body}");
        }
        let index=index(r#"<?php namespace App\Providers;class Bindings extends \Illuminate\Support\ServiceProvider{public function register(){$this->app->bind(\App\Contracts\Posts::class,\App\Services\Posts::class);$this->app->when([\App\Http\Other::class,$dynamic])->needs(\App\Contracts\Posts::class)->give(\App\Services\Posts::class);}}"#);
        assert!(labels(&index,r#"<?php namespace App\Http;class Consumer{public function __construct(\App\Contracts\Posts $posts){$posts->selected()->tit^le;}}"#).is_empty());
        let(text,at)=crate::tests::cursor(r#"<?php app(\App\Contracts\Posts::class)->selected()->tit^le"#);let doc=crate::tests::doc("/project/Test.php","php",&text);
        assert!(completion_checked(&index,&doc,at,&|path|!path.ends_with("Bindings.php")).is_empty());
        let(text,at)=crate::tests::cursor(r#"<?php namespace App\Providers;class Bindings extends \Illuminate\Support\ServiceProvider{public function register(){$this->app->bind('unsaved',fn()=>new \App\Services\Posts);app('unsaved')->selected()->tit^le;}}"#);
        let doc=crate::tests::doc("/project/app/Providers/Bindings.php","php",&text);
        assert!(completion_checked(&index,&doc,at,&|_|true).iter().any(|item|item.label=="title"));
        let(text,at)=crate::tests::cursor(r#"<?php namespace App\Providers;class Bindings extends \Illuminate\Support\ServiceProvider{public function register(){$this->app->bind('unsaved',fn()=>config('service'));app('unsaved')->selected()->tit^le;}}"#);
        let doc=crate::tests::doc("/project/app/Providers/Bindings.php","php",&text);
        assert!(completion_checked(&index,&doc,at,&|_|true).is_empty());
    }
    #[test]
    fn registered_container_provider_refresh_and_navigation_use_cached_source_only(){
        use openidea_plugin::{Plugin,providers::Providers,Event,mock::MockHost};
        let host=MockHost::install();host.add_file("artisan","<?php");host.add_file("composer.json",r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#);
        host.add_file("composer.lock",r#"{"packages":[{"name":"laravel/framework","version":"v13.40.0"}]}"#);
        host.add_file("app/Models/Post.php",r#"<?php namespace App\Models;class Post extends \Illuminate\Database\Eloquent\Model{}"#);
        host.add_file("app/Services/Posts.php",r#"<?php namespace App\Services;class Posts{public function selected():\App\Models\Post{return new \App\Models\Post;}}"#);
        host.add_file("app/Contracts/Posts.php",r#"<?php namespace App\Contracts;#[\Illuminate\Container\Attributes\Bind(\App\Services\Posts::class)] interface Posts{}"#);
        let provider=r#"<?php namespace App\Providers;class Bindings extends \Illuminate\Support\ServiceProvider{public function register(){$this->app->bind('posts',static fn()=>new \App\Services\Posts);$this->app->when([\App\Http\Consumer::class])->needs(\App\Contracts\Posts::class)->give(fn()=>new \App\Services\Posts);}}"#;
        host.add_file("app/Providers/Bindings.php",provider);host.add_file("database/migrations/posts.php","<?php Schema::create('posts',function($table){$table->string('title');});");
        let mut plugin=crate::Main::default();plugin.activate(&host.context("aritti.laravel")).unwrap();crate::tests::finish_indexing(&mut plugin);
        for source in [r#"<?php app('posts')->selected()->tit^le"#,r#"<?php app(\App\Contracts\Posts::class)->selected()->tit^le"#,r#"<?php namespace App\Http;class Given{public function __construct(#[\Illuminate\Container\Attributes\Give(\App\Services\Posts::class)] \App\Contracts\Posts $posts){$posts->selected()->tit^le;}}"#,r#"<?php namespace App\Http;class Consumer{public function __construct(\App\Contracts\Posts $posts){$posts->selected()->tit^le;}}"#]{
            let(text,at)=crate::tests::cursor(source);let doc=crate::tests::doc("/project/Test.php","php",&text);
            assert!(plugin.completion(&doc,at,None).iter().any(|item|item.label=="title"));assert_eq!(plugin.definition(&doc,at)[0].path,"/project/database/migrations/posts.php");
        }
        host.add_file("app/Providers/Bindings.php",&provider.replace("new \\App\\Services\\Posts","config('service')"));plugin.handle_events(&[Event::FileSaved("/project/app/Providers/Bindings.php".into())],&host.invocation());
        let(text,at)=crate::tests::cursor(r#"<?php app('posts')->selected()->tit^le"#);assert!(plugin.completion(&crate::tests::doc("/project/Test.php","php",&text),at,None).is_empty());
        assert!(host.process_runs().is_empty());assert!(host.requests().is_empty());
    }
}
