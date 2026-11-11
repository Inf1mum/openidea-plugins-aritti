//! Framework conditional and optional forwarding with explicit callback results.
use super::*;
impl Resolver<'_,'_>{
    pub(super) fn conditionable_base(&self,receiver:&Typed)->Option<Typed>{
        Some(match &receiver.shape{
            Shape::Static|Shape::Model if self.get(&receiver.model).is_some()&&!self.declared(&receiver.model,if receiver.shape==Shape::Static{"__callStatic"}else{"__call"})=>Typed{shape:Shape::Query,..receiver.clone()},
            Shape::Query|Shape::Relation(_)|Shape::Collection|Shape::MappedCollection(_)|Shape::Factory{..}=>receiver.clone(),
            Shape::Object if self.raw(&receiver.model).is_some_and(|class|class.traits.iter().any(|name|name=="Illuminate\\Support\\Traits\\Conditionable"))=>receiver.clone(),
            _=>return None,
        })
    }
    pub(super) fn conditional_input(&self,receiver:&Typed)->Option<Typed>{
        let mut base=self.conditionable_base(receiver)?;
        if matches!(base.shape,Shape::Relation(_)){base.shape=Shape::Query;}
        Some(base)
    }
    fn conditional_truth(&self,range:&std::ops::Range<usize>)->Option<bool>{
        let source=&self.context.source;
        if range.end==range.start+1{match source.tokens[range.start].value.to_ascii_lowercase().as_str(){"true"=>return Some(true),"false"|"null"=>return Some(false),_=>{}}}
        let first=source.tokens.get(range.start)?;let last=source.tokens.get(range.end.checked_sub(1)?)?;
        let raw=&source.text[first.start..last.end];
        if let Some((value,_))=php::literal(source.text,first.start..last.end){let value=metadata_inference::decode_literal(value,first.value.as_bytes()[0])?;return Some(!value.is_empty()&&value!="0");}
        raw.parse::<f64>().ok().filter(|value|value.is_finite()).map(|value|value!=0.0)
    }
    fn callback_function(&self,range:&std::ops::Range<usize>)->Option<Function>{
        let source=&self.context.source;let start=range.start+usize::from(source.is(range.start,"static"));
        source.functions().into_iter().find(|function|function.name.is_none()&&function.start==start&&if source.is(function.body,"=>"){function.end==range.end}else{function.end+1==range.end})
    }
    /// Some(None) is an explicit null/void result, which Conditionable replaces
    /// with the original receiver. None is an unproven result and is withheld.
    pub(super) fn callback_result(&self,range:std::ops::Range<usize>,input:&Typed,arguments:usize)->Option<Option<Typed>>{
        let source=&self.context.source;
        if range.end==range.start+1&&source.is(range.start,"null"){return Some(None);}
        let function=self.callback_function(&range)?;
        let parameters=container::arguments(source,function.params.start-1)?;
        if parameters.len()>arguments{return None;}
        if let Some(first)=parameters.first(){
            let tokens=&source.tokens[first.clone()];let variable=tokens.iter().position(|token|token.kind==Kind::Var)?;
            if tokens[..=variable].iter().any(|token|matches!(token.value,"&"|"...")){return None;}
            if variable>0{
                let parameter=helper_facts::declared_type(&tokens[..variable].iter().collect::<Vec<_>>(),&self.context.names,"__OpenIDEA_Pipe_Receiver__",false)?;
                let signature=pipe::Callable{full:String::new(),path:self.path.into(),entry:entry(source,self.path,function.start),ambiguous:false,parameter:Some(parameter),required:1,parameters:1,by_reference:false,result:pipe::Return::Input};
                if !self.pipe_accepts(&signature,input){return None;}
            }
        }
        let expression=if source.is(function.body,"=>"){function.body+1..function.end}else{
            let functions=source.functions();
            let own_returns=(function.body+1..function.end).filter(|at|source.is(*at,"return")&&(at==&0||!matches!(source.tokens[at-1].value,"->"|"?->"|"::"))&&!functions.iter().any(|nested|nested.start>function.start&&nested.body<*at&&*at<nested.end)).collect::<Vec<_>>();
            if (function.body+1..function.end).any(|at|source.is(at,"yield")&&!functions.iter().any(|nested|nested.start>function.start&&nested.body<at&&at<nested.end)){return None;}
            if own_returns.is_empty(){return Some(None);}
            let [ret]=own_returns.as_slice()else{return None};
            if source.paths[*ret]!=source.paths[function.body+1]||!source.is(function.end-1,";")||(*ret!=function.body+1&&!matches!(source.tokens[ret-1].value,";"|"}")){return None;}
            ret+1..function.end-1
        };
        if expression.is_empty()||expression.end==expression.start+1&&source.is(expression.start,"null"){return Some(None);}
        let(result,start)=self.expr(expression.end,1)?;
        (start==expression.start).then_some(Some(result))
    }
    fn conditional_return(&self,base:&Typed,input:&Typed,result:Option<Typed>)->Typed{
        let result=result.unwrap_or_else(||input.clone());
        if result.model.eq_ignore_ascii_case(&input.model)&&result.shape==input.shape{base.clone()}else{result}
    }
    pub(super) fn conditional_call(&self,receiver:&Typed,name:&str,from:usize,to:usize)->Option<Option<Typed>>{
        if !matches!(name,"when"|"unless"){return None;}
        let base=self.conditionable_base(receiver)?;
        if matches!(receiver.shape,Shape::Static|Shape::Model)&&self.declared(&receiver.model,name){return None;}
        if let Some(class)=self.custom_class(receiver)&&self.object_method(&class,name).is_some(){return Some(self.custom_call(receiver,name));}
        let source=&self.context.source;let args=container::arguments(source,from.checked_sub(1)?)?;
        if source.pairs.get(&(from-1))!=Some(&to)||args.len()>3{return Some(None);}
        if args.len()<2{
            let truth=args.first().and_then(|range|self.conditional_truth(range)).map(|truth|if name=="unless"{!truth}else{truth});
            return Some(Some(Typed{shape:Shape::Conditional{target:Box::new(base.shape),truth,capture:args.is_empty(),invert:name=="unless"},model:base.model}));
        }
        let input=self.conditional_input(receiver)?;
        let truth=self.conditional_truth(&args[0]).map(|truth|if name=="unless"{!truth}else{truth});
        let callback=||self.callback_result(args[1].clone(),&input,2).map(|result|self.conditional_return(&base,&input,result));
        let fallback=||if let Some(range)=args.get(2){self.callback_result(range.clone(),&input,2).map(|result|self.conditional_return(&base,&input,result))}else{Some(base.clone())};
        let result=match truth{
            Some(true)=>callback(),Some(false)=>fallback(),None=>{
                let a=callback()?;let b=fallback()?;
                if a.model.eq_ignore_ascii_case(&b.model)&&a.shape==b.shape{Some(a)}else if let(Shape::Factory{class:a_class,..},Shape::Factory{class:b_class,..})=(&a.shape,&b.shape){(a.model.eq_ignore_ascii_case(&b.model)&&a_class==b_class).then(||Typed{model:a.model,shape:Shape::Factory{class:a_class.clone(),many:None}})}else{None}
            }
        };Some(result)
    }
    fn proxy_method_known(&self,target:&Typed,name:&str)->bool{
        if self.custom_class(target).is_some_and(|class|self.object_method(&class,name).is_some()){return true;}
        match target.shape{
            Shape::Query|Shape::Relation(_)=>QUERY.iter().any(|(method,_)|*method==name)||["first","firstOrFail","get","count","exists","doesntExist","create","find"].contains(&name),
            Shape::Collection|Shape::MappedCollection(_)=>COLLECTION_KEEP.contains(&name)||["first","last","count","isEmpty","isNotEmpty","contains","map"].contains(&name),
            Shape::Factory{..}=>["state","count","create","make","sequence"].contains(&name),
            Shape::Model|Shape::Object=>self.object_method(&target.model,name).is_some(),_=>false,
        }
    }
    pub(super) fn proxy_call(&self,receiver:&Typed,name:&str,from:usize,to:usize)->Option<Option<Typed>>{
        match &receiver.shape{
            Shape::Optional(target)=>Some(self.call(Typed{model:receiver.model.clone(),shape:*target.clone()},name,from,to)),
            Shape::Conditional{target,truth,capture,invert}=>{
                let base=Typed{model:receiver.model.clone(),shape:*target.clone()};
                if *capture{return Some(self.proxy_method_known(&base,name).then(||Typed{model:base.model,shape:Shape::Conditional{target:target.clone(),truth:None,capture:false,invert:*invert}}));}
                match truth{Some(false)=>Some(Some(base)),Some(true)=>Some(self.call(base,name,from,to)),None=>{let result=self.call(base.clone(),name,from,to)?;Some((result.model.eq_ignore_ascii_case(&base.model)&&result.shape==base.shape).then_some(result))}}
            }
            _=>None,
        }
    }
    fn optional_open(&self,open:usize)->bool{
        let source=&self.context.source;
        let Some(identity)=metadata_inference::call_identity(source.text,source.tokens[open].start)else{return false};
        if identity.class.is_some()||identity.constructor||identity.function_fqn.as_deref().unwrap_or(&identity.method)!="optional"{return false;}
        let shadow=identity.function_fqn.as_deref().unwrap_or_else(||"optional");
        let local=if identity.function_fqn.is_none(){self.context.names.qualify("optional")}else{shadow.into()};
        if self.index.eloquent_functions.contains_key(&local.to_ascii_lowercase()){return false;}
        !source.functions().iter().any(|function|function.name.is_some_and(|at|source.tokens[at].value.eq_ignore_ascii_case("optional"))&&source.paths[function.start].is_empty()&&self.context.names.qualify("optional").eq_ignore_ascii_case(&local))
    }
    pub(super) fn optional_target(&self,open:usize,close:usize,depth:u8)->Option<Typed>{
        let source=&self.context.source;if !self.optional_open(open){return None;}
        let args=container::arguments(source,open)?;if args.is_empty()||args.len()>2||source.pairs.get(&open)!=Some(&close){return None;}
        let(input,start)=self.expr(args[0].end,depth+1)?;if start!=args[0].start||input.shape==Shape::Static{return None;}
        if let Some(callback)=args.get(1){if callback.end==callback.start+1&&source.is(callback.start,"null"){return Some(Typed{shape:Shape::Optional(Box::new(input.shape)),model:input.model});}return self.callback_result(callback.clone(),&input,1)?;}
        Some(Typed{shape:Shape::Optional(Box::new(input.shape)),model:input.model})
    }
    pub(super) fn optional_binding(&self,function:&Function)->Option<Typed>{
        let source=&self.context.source;
        let open=source.pairs.iter().filter(|(open,close)|**open<function.start&&**close>=function.end&&source.is(**open,"(")&&self.optional_open(**open)).map(|(open,_)|*open).max()?;
        let args=container::arguments(source,open)?;if args.len()!=2||args[1].start+usize::from(source.is(args[1].start,"static"))!=function.start{return None;}
        let(input,start)=self.expr(args[0].end,1)?;(start==args[0].start).then_some(input)
    }
}
