//! Proven framework callback parameters and higher-order collection receivers.
use super::*;
impl Resolver<'_,'_>{
    fn eager_callback_path(&self,function:&Function,range:&std::ops::Range<usize>)->Option<String>{
        let source=&self.context.source;let open=source.tokens.get(range.start)?.start;
        if !source.is(range.start,"[")||source.pairs.get(&range.start)!=Some(&(range.end-1)){return None;}
        let from=source.tokens[function.start].start;
        let end=if source.is(function.body,"=>"){source.tokens.get(function.end).map_or(source.text.len(),|token|token.start)}else{source.tokens.get(function.end).map_or(source.text.len(),|token|token.end)};
        fn nested(text:&str,open:usize,from:usize,end:usize,prefix:&str,depth:u8)->Option<String>{
            if depth>8{return None;}
            for item in php::items(text,open){
                if item.value.start>from||item.value.end<end{continue;}
                let (name,_)=php::literal(text,item.key?)?;let name=name.split(':').next()?;
                if name.split('.').any(|part|!crate::generator_plan::identifier(part)){return None;}
                let path=if prefix.is_empty(){name.into()}else{format!("{prefix}.{name}")};
                let value=&text[item.value.clone()];let first=item.value.start+value.len()-value.trim_start().len();
                return if text.as_bytes().get(first)==Some(&b'['){nested(text,first,from,end,&path,depth+1)}else{Some(path)};
            }None
        }
        nested(source.text,open,from,end,"",0)
    }
    fn eager_model(&self,full:&str,path:&str)->Option<String>{
        let mut full=full.to_owned();
        for name in path.split('.'){let target=self.relation(&full,name)?;full=self.get(&target.target)?.full.clone();}
        Some(full)
    }
    pub(super) fn callback_binding(&self,function:&Function,name:&str)->Option<Typed>{
        if function.name.is_some(){return None;}
        if let Some(input)=self.pipe_binding(function,name,1){return Some(input);}
        let source=&self.context.source;let params=&source.tokens[function.params.clone()];
        let variable=params.iter().position(|token|token.kind==Kind::Var&&token.value==name)?;
        if params[..variable].iter().any(|token|matches!(token.value,","|"&"|"=")){return None;}
        if let Some(input)=self.optional_binding(function){
            let expected=if variable==0{None}else{Some(helper_facts::declared_type(&params[..variable].iter().collect::<Vec<_>>(),&self.context.names,"__OpenIDEA_Pipe_Receiver__",false)?)};
            let signature=pipe::Callable{full:String::new(),path:self.path.into(),entry:entry(source,self.path,function.start),ambiguous:false,parameter:expected,required:1,parameters:1,by_reference:false,result:pipe::Return::Input};
            return self.pipe_accepts(&signature,&input).then_some(input);
        }
        let open=source.pairs.iter().filter(|(open,close)|**open<function.start&&**close>=function.end&&source.is(**open,"(")&&**open>=2&&source.tokens[**open-1].kind==Kind::Name&&matches!(source.tokens[**open-2].value,"->"|"?->"|"::")).map(|(open,_)|*open).max()?;
        let (receiver,_)=self.expr(open-2,1)?;let method=source.tokens[open-1].value;
        if let Some(typed)=self.package_join_binding(function,&receiver,open,name){return Some(typed);}
        if self.declared(&receiver.model,method)||self.custom_class(&receiver).is_some_and(|full|self.object_method(&full,method).is_some()){return None;}
        let args=container::arguments(source,open)?;
        let callback_arg=args.iter().position(|range|range.start<=function.start&&range.end>=function.end)?;
        let eager=["with","withOnly","withWhereHas","whereHas","orWhereHas","whereDoesntHave","orWhereDoesntHave"].contains(&method)&&matches!(receiver.shape,Shape::Static|Shape::Query|Shape::Relation(_))||["load","loadMissing"].contains(&method)&&matches!(receiver.shape,Shape::Model|Shape::Collection|Shape::MappedCollection(_));
        let typed=if ["when","unless"].contains(&method)&&callback_arg<=2{
            self.conditional_input(&receiver)?
        }else if eager&&callback_arg==0&&let Some(path)=self.eager_callback_path(function,&args[0]){
            Typed{model:self.eager_model(&receiver.model,&path)?,shape:Shape::Query}
        }else{match &receiver.shape{
            Shape::Static|Shape::Query|Shape::Relation(_)=>{
                if ["where","orWhere","whereNot","orWhereNot"].contains(&method)&&callback_arg==0{Typed{shape:Shape::Query,..receiver}}
                else if ["whereHas","orWhereHas","whereDoesntHave","orWhereDoesntHave","withWhereHas","with"].contains(&method)&&callback_arg==1{
                    let range=args.first()?;let from=&source.tokens[range.start];let to=&source.tokens[range.end-1];let (path,_)=php::literal(source.text,from.start..to.end)?;
                    Typed{model:self.eager_model(&receiver.model,path)?,shape:Shape::Query}
                }else{return None;}
            }
            Shape::Model|Shape::Collection|Shape::MappedCollection(_) if eager&&callback_arg==1=>{
                let range=args.first()?;let from=&source.tokens[range.start];let to=&source.tokens[range.end-1];let(path,_)=php::literal(source.text,from.start..to.end)?;
                Typed{model:self.eager_model(&receiver.model,path)?,shape:Shape::Query}
            }
            Shape::Collection|Shape::MappedCollection(_) if ["each","map","filter","reject","flatMap","sortBy","sortByDesc","keyBy","groupBy","contains","first","last"].contains(&method)&&callback_arg==0=>Typed{shape:Shape::Model,..receiver},
            Shape::Factory{..} if ["afterMaking","afterCreating"].contains(&method)&&callback_arg==0=>Typed{shape:Shape::Model,..receiver},
            _=>return None,
        }};
        let before=&params[..variable];
        if !before.is_empty(){
            let ty=if before.len()==1{&before[0]}else if before.len()==2&&before[0].value=="?"{&before[1]}else{return None;};
            let _=ty;
            let parameter=helper_facts::declared_type(&before.iter().collect::<Vec<_>>(),&self.context.names,"__OpenIDEA_Pipe_Receiver__",false)?;
            let signature=pipe::Callable{full:String::new(),path:self.path.into(),entry:entry(source,self.path,function.start),ambiguous:false,parameter:Some(parameter),required:1,parameters:1,by_reference:false,result:pipe::Return::Input};
            if !self.pipe_accepts(&signature,&typed){return None;}
        }
        Some(typed)
    }
}

impl Resolver<'_,'_>{
    pub(super) fn mapped_collection(&self,receiver:&Typed,name:&str,from:usize,to:usize)->Option<Option<Typed>>{
        if !matches!(receiver.shape,Shape::Collection|Shape::MappedCollection(_))||!matches!(name,"map"|"flatMap"){return None;}
        if !self.native_collection_operation(receiver,name){return Some(None);}
        let source=&self.context.source;let args=container::arguments(source,from.checked_sub(1)?)?;
        if args.len()!=1||source.pairs.get(&(from-1))!=Some(&to){return Some(None);}
        let input=Typed{model:receiver.model.clone(),shape:Shape::Model};
        let result=self.callback_result(args[0].clone(),&input,2)??;
        let physical=self.collection_class(receiver)?;
        if name=="map"&&result.shape!=Shape::Model||name=="flatMap"&&!matches!(result.shape,Shape::Collection|Shape::MappedCollection(_)){return Some(None);}
        let physical=if name=="flatMap"&&self.subclass(&physical,"Illuminate\\Database\\Eloquent\\Collection"){"Illuminate\\Support\\Collection".into()}else{physical};
        Some(Some(Typed{model:result.model,shape:Shape::MappedCollection(physical)}))
    }
}
