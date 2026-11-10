//! Declarative Blade component sources. No config/PHP expressions are evaluated.
use crate::{Index,index::{Entry,Names,AddName},php};
use openidea_plugin::{Project,Position,host,text};
use serde_json::Value;
use std::{collections::{BTreeSet,BTreeMap},cell::RefCell,time::{Instant,Duration}};
struct Budget{started:Instant,files:usize,bytes:usize,exhausted:bool,cache:BTreeMap<String,Option<String>>}
thread_local!{static BUDGET:RefCell<Option<Budget>>=const{RefCell::new(None)};}
struct Guard(Option<Budget>);
impl Guard{fn begin()->Self{Self(BUDGET.with(|b|b.replace(Some(Budget{started:Instant::now(),files:0,bytes:0,exhausted:false,cache:BTreeMap::new()}))))}}
impl Drop for Guard{fn drop(&mut self){BUDGET.with(|b|{b.replace(self.0.take());});}}
fn active()->bool{BUDGET.with(|state|{let mut state=state.borrow_mut();let Some(b)=state.as_mut()else{return true;};if b.started.elapsed()>Duration::from_millis(750){b.exhausted=true;}!b.exhausted})}
fn exhaust(){BUDGET.with(|state|{if let Some(b)=state.borrow_mut().as_mut(){b.exhausted=true;}});}
#[derive(Clone,Debug)]
pub(crate) struct ViewPath{pub path:String,pub namespace:String,pub prefix:String}
#[derive(Clone,Debug)]
pub(crate) struct ClassNamespace{pub namespace:String,pub tag_namespace:String,pub prefix:String,pub bare:bool}
#[derive(Clone,Debug)]
pub(crate) struct Explicit{pub name:String,pub namespace:String,pub class:Option<String>}
#[derive(Clone,Debug)]
pub(crate) enum Fetch{String,Key(String),Empty}
#[derive(Clone,Debug)]
pub(crate) struct ConfigSource{pub file:String,pub key:String,pub prefix_key:String,pub namespace_key:String,pub alias_key:Option<String>,pub class:Fetch,pub view:Fetch}
#[derive(Clone,Debug,Default)]
pub(crate) struct Metadata{
    pub view_paths:Vec<ViewPath>,pub php_namespaces:Vec<ClassNamespace>,pub explicit:Vec<Explicit>,pub from_config:Vec<ConfigSource>,pub ignore_view_paths:Vec<String>,
    pub issues:Vec<String>,pub resolved_views:Names,pub bare_tags:BTreeSet<String>,
}
impl Metadata{
    pub(crate) fn prepend(&mut self,mut other:Self)->Vec<String>{
        other.view_paths.append(&mut self.view_paths);other.php_namespaces.append(&mut self.php_namespaces);other.explicit.append(&mut self.explicit);other.from_config.append(&mut self.from_config);other.ignore_view_paths.append(&mut self.ignore_view_paths);
        let issues=std::mem::take(&mut other.issues);*self=other;issues
    }
}
fn list<'a>(value:&'a Value,key:&str)->impl Iterator<Item=&'a Value>{value.get(key).and_then(Value::as_array).into_iter().flatten()}
fn class(name:&str)->Option<String>{let name=name.trim_matches('\\');(!name.is_empty()&&name.split('\\').all(|p|!p.is_empty()&&p.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'))).then(||name.into())}
fn tag(name:&str)->bool{crate::refs::valid_tag_name(name,false)}
fn prefix(name:&str)->bool{name.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'.'|b'-'|b'_'))}
fn text_field(value:&Value,key:&str)->Option<String>{match value.get(key){None=>Some(String::new()),Some(value)=>value.as_str().map(str::to_owned)}}
fn fetch(value:Option<&Value>,default:Fetch)->Option<Fetch>{match value{None=>Some(default),Some(value)=>match value.get("type").and_then(Value::as_str)?{"string"=>Some(Fetch::String),"empty"=>Some(Fetch::Empty),"arrayKey"=>Some(Fetch::Key(value.get("key")?.as_str()?.into())),_=>None}}}
pub(crate) fn parse_metadata(source:&str,value:&Value,resolve:&dyn Fn(&str)->Option<String>)->Metadata{
    let mut out=Metadata::default();let Some(components)=value.pointer("/blade/components")else{return out;};
    let Some(object)=components.as_object()else{out.issues.push(format!("{source}: blade.components must be an object"));return out;};
    for (key,value) in object{if !["viewPaths","phpNamespaces","fromConfig","list","ignoreViewPaths"].contains(&key.as_str()){out.issues.push(format!("{source}: unsupported blade.components.{key}"));}else if !value.is_array(){out.issues.push(format!("{source}: blade.components.{key} must be an array"));}}
    for entry in list(components,"viewPaths"){
        let parsed=||{let path=resolve(entry.get("path")?.as_str()?)?;let namespace=text_field(entry,"namespace")?;let pref=text_field(entry,"prefix")?;
            if !prefix(&namespace)||!prefix(&pref){return None;}Some(ViewPath{path,namespace,prefix:pref})};
        if let Some(entry)=parsed(){out.view_paths.push(entry);}else{out.issues.push(format!("{source}: invalid blade.components.viewPaths entry"));}
    }
    for entry in list(components,"phpNamespaces"){
        let parsed=||{let namespace=class(entry.get("phpNamespace")?.as_str()?)?;let tag_namespace=text_field(entry,"tagNamespace")?;let pref=text_field(entry,"prefix")?;
            if !prefix(&tag_namespace)||!prefix(&pref)||entry.get("ignoreBladeComponentPrefix").is_some_and(|v|!v.is_boolean()){return None;}
            Some(ClassNamespace{namespace,tag_namespace,prefix:pref,bare:entry["ignoreBladeComponentPrefix"].as_bool().unwrap_or(false)})};
        if let Some(entry)=parsed(){out.php_namespaces.push(entry);}else{out.issues.push(format!("{source}: invalid blade.components.phpNamespaces entry"));}
    }
    for entry in list(components,"list"){
        let parsed=||{let name=entry.get("name")?.as_str()?.to_owned();let namespace=text_field(entry,"namespace")?;if !tag(&name)||name.contains("::")||!prefix(&namespace){return None;}
            let target=match entry.get("className"){None=>None,Some(v)=>Some(class(v.as_str()?)?)};Some(Explicit{name,namespace,class:target})};
        if let Some(entry)=parsed(){out.explicit.push(entry);}else{out.issues.push(format!("{source}: invalid blade.components.list entry"));}
    }
    for entry in list(components,"fromConfig"){
        let parsed=||{let file=entry.get("configFileName")?.as_str()?.to_owned();let key=entry.get("componentsKey")?.as_str()?.to_owned();if !tag(&file)||!tag(&key)||file.contains("::")||key.contains("::"){return None;}
            let optional=|name:&str,default:&str|match entry.get(name){None=>Some(default.into()),Some(v)=>v.as_str().filter(|v|tag(v)&&!v.contains("::")).map(str::to_owned)};
            Some(ConfigSource{file,key,prefix_key:optional("prefixKey","prefix")?,namespace_key:optional("namespaceKey","namespace")?,alias_key:match entry.get("aliasKey"){None=>None,Some(v)=>Some(v.as_str()?.to_owned())},class:fetch(entry.get("classFetch"),Fetch::String)?,view:fetch(entry.get("viewFetch"),Fetch::Empty)?})};
        if let Some(entry)=parsed(){out.from_config.push(entry);}else{out.issues.push(format!("{source}: invalid blade.components.fromConfig entry"));}
    }
    for entry in list(components,"ignoreViewPaths"){if let Some(path)=entry.as_str().and_then(resolve){out.ignore_view_paths.push(path);}else{out.issues.push(format!("{source}: invalid blade.components.ignoreViewPaths entry"));}}
    out
}
#[derive(Clone)]
struct Root{namespace:String,directory:String}
fn owned(project:&Project,path:&str)->bool{let Ok(root)=project.root()else{return false;};host::project_of(path).and_then(|p|p.root().ok()).as_deref()==Some(root.as_str())}
fn path(path:&str)->bool{!path.is_empty()&&!path.starts_with('/')&&!path.contains('\\')&&path.split('/').all(|p|!p.is_empty()&&!matches!(p,"."|".."))}
fn read(project:&Project,relative:&str)->Option<String>{
    if !active(){return None;}
    if let Some(cached)=BUDGET.with(|state|state.borrow().as_ref().and_then(|b|b.cache.get(relative).cloned())){return cached;}
    let permitted=BUDGET.with(|state|{let mut state=state.borrow_mut();if let Some(b)=state.as_mut(){if b.files>=128{b.exhausted=true;return false;}b.files+=1;}true});if !permitted{return None;}
    let load=||{let root=project.root().ok()?;let full=format!("{root}/{relative}");if !path(relative)||!owned(project,&full){return None;}let stat=project.stat(&full).ok()?;if stat.size>256*1024{return None;}project.read_file(&full).ok().filter(|s|s.len()<=256*1024)};
    let mut source=load();BUDGET.with(|state|{let mut state=state.borrow_mut();if let Some(b)=state.as_mut(){b.bytes+=source.as_ref().map_or(0,String::len);if b.bytes>2*1024*1024{b.exhausted=true;source=None;}b.cache.insert(relative.into(),source.clone());}});source
}
fn roots(project:&Project,index:&mut Index)->Vec<Root>{
    index.dependency_files.insert("composer.json".into());
    let Some(source)=read(project,"composer.json")else{return vec![];};let Ok(value)=crate::strict_json::parse(&source,256*1024)else{return vec![];};let mut out=Vec::new();
    for key in ["autoload","autoload-dev"]{if let Some(map)=value.pointer(&format!("/{key}/psr-4")).and_then(Value::as_object){for(prefix,value)in map{if !active(){break;}let Some(namespace)=class(prefix)else{continue;};let dirs:Vec<_>=value.as_str().into_iter().chain(value.as_array().into_iter().flatten().filter_map(Value::as_str)).collect();for directory in dirs{if !active(){break;}if out.len()>=128{exhaust();break;}let directory=directory.trim_end_matches('/');if path(directory){out.push(Root{namespace:format!("{namespace}\\"),directory:directory.into()});}}}}}
    if out.is_empty()&&value.get("autoload").is_none()&&value.get("autoload-dev").is_none(){out.push(Root{namespace:"App\\".into(),directory:"app".into()});}out
}
fn class_entry(project:&Project,roots:&[Root],full:&str)->Option<Entry>{
    let root=project.root().ok()?;let mut found=Vec::new();for mapping in roots{let Some(tail)=full.strip_prefix(&mapping.namespace)else{continue;};let relative=format!("{}/{}.php",mapping.directory,tail.replace('\\',"/"));let Some(source)=read(project,&relative)else{continue;};
        if crate::eloquent_members::declaration(&source).as_deref()!=Some(full){continue;}
        let code=crate::package_facts::code(&source)?;let stem=full.rsplit('\\').next()?;
        let offset=code.match_indices("class").find_map(|(at,_)|{if at>0&&(code.as_bytes()[at-1].is_ascii_alphanumeric()||matches!(code.as_bytes()[at-1],b'_'|b':')){return None;}let after=at+5;let tail=code.get(after..)?;if !tail.starts_with(char::is_whitespace){return None;}let start=code.len()-tail.trim_start().len();(code[start..].starts_with(stem)&&!code.as_bytes().get(start+stem.len()).is_some_and(|b|php::is_identifier(*b))).then_some(start)})?;
        found.push(Entry{detail:Some("ide.json component class".into()),path:format!("{root}/{relative}"),at:text::position(&source,offset),width:stem.encode_utf16().count()as u32});
    }if found.len()==1{found.pop()}else{None}
}
fn name(namespace:&str,prefix:&str,local:&str)->Option<String>{let local=format!("{prefix}{local}");let result=if namespace.is_empty(){local}else{format!("{namespace}::{local}")};tag(&result).then_some(result)}
fn class_tag(full:&str)->String{full.split('\\').map(|s|crate::index::separated(s,'-')).collect::<Vec<_>>().join(".")}
fn string(source:&str,range:std::ops::Range<usize>)->Option<String>{let(value,at)=php::literal(source,range)?;if source.as_bytes()[at-1]==b'\''{Some(value.replace("\\\\","\\").replace("\\'","'"))}else{(!value.contains(['\\','$'])).then(||value.into())}}
fn value_at(source:&str,open:usize,key:&str)->Option<std::ops::Range<usize>>{let mut open=open;let mut parts=key.split('.').peekable();while let Some(part)=parts.next(){let item=php::items(source,open).into_iter().find(|item|item.key.clone().and_then(|r|string(source,r)).as_deref()==Some(part))?;if parts.peek().is_none(){return Some(item.value);}open=php::array_open(source,item.value)?;}None}
fn returned_array(source:&str)->Option<usize>{
    let code=crate::package_facts::code(source)?;let mut at=0;let bytes=code.as_bytes();
    while at<bytes.len(){if matches!(bytes[at],b'('|b'['|b'{'){at=php::closing(source,at)?+1;continue;}
        if code[at..].starts_with("return")&&(at==0||!php::is_identifier(bytes[at-1]))&&!bytes.get(at+6).is_some_and(|b|php::is_identifier(*b)){
            let start=at+6;let mut end=start;while end<bytes.len()&&bytes[end]!=b';'{if matches!(bytes[end],b'('|b'['|b'{'){end=php::closing(source,end)?+1;}else{end+=1;}}
            let mut range=php::trimmed(source,start..end);while source.as_bytes().get(range.start)==Some(&b'(')&&php::closing(source,range.start)==Some(range.end.checked_sub(1)?){range=php::trimmed(source,range.start+1..range.end-1);}
            let open=php::array_open(source,range.clone())?;return(php::closing(source,open)==Some(range.end.checked_sub(1)?)).then_some(open);
        }at+=1;
    }None
}
fn extract(source:&str,item:&php::Item,fetch:&Fetch)->Option<std::ops::Range<usize>>{match fetch{Fetch::Empty=>None,Fetch::String=>Some(item.value.clone()),Fetch::Key(key)=>value_at(source,php::array_open(source,item.value.clone())?,key)}}
fn insert(index:&mut Index,tag:&str,entry:Entry,is_class:bool,bare:bool){
    let existing_class=index.components.get(tag).is_some_and(|e|!e.path.is_empty()&&e.path.ends_with(".php")&&!e.path.ends_with(".blade.php"));
    if is_class||!existing_class{index.components.insert(tag.into(),entry);}
    if bare{index.metadata.blade_components.bare_tags.insert(tag.into());}
}
pub(crate) fn apply_metadata(project:&Project,index:&mut Index){
    let _guard=Guard::begin();let Ok(root)=project.root()else{return;};let config=index.metadata.blade_components.clone();if config.view_paths.len()+config.php_namespaces.len()+config.explicit.len()+config.from_config.len()+config.ignore_view_paths.len()>128{index.partial=true;index.metadata.issues.push("Blade component metadata exceeds the 128-declaration bound".into());return;}
    let roots=roots(project,index);
    index.metadata.blade_components.resolved_views.clear();index.metadata.blade_components.bare_tags.clear();
    for ignored in &config.ignore_view_paths{if !active(){break;}index.dependency_roots.insert(ignored.clone());let prefix=format!("{root}/{}/",ignored.trim_end_matches('/'));index.components.retain(|_,entry|!entry.path.ends_with(".blade.php")||!entry.path.starts_with(&prefix));}
    // Explicit class identities are installed before all anonymous paths.
    for declared in &config.explicit{if !active(){break;}let Some(tag)=name(&declared.namespace,"",&declared.name)else{continue;};if let Some(full)=&declared.class{if let Some(entry)=class_entry(project,&roots,full){index.dependency_files.insert(entry.path.trim_start_matches(&format!("{root}/")).into());insert(index,&tag,entry,true,false);}else{index.metadata.issues.push(format!("unresolved owned component class {full}"));}}else{index.components.add(&tag,Some("ide.json declared component".into()),"",Position::new(0,0),0);}}
    for declared in &config.php_namespaces{if !active(){break;}for mapping in &roots{if !active(){break;}let directory=if declared.namespace==mapping.namespace.trim_end_matches('\\'){Some(mapping.directory.clone())}else{declared.namespace.strip_prefix(&mapping.namespace).map(|tail|format!("{}/{}",mapping.directory,tail.replace('\\',"/"))).or_else(||mapping.namespace.starts_with(&format!("{}\\",declared.namespace)).then(||mapping.directory.clone()))};let Some(directory)=directory else{continue;};index.dependency_roots.insert(directory.clone());let Ok(paths)=project.find_files(&format!("{directory}/**/*.php"),513)else{continue;};if paths.len()>=513{index.partial=true;continue;}
            for file in paths{if !active(){break;}if !owned(project,&file){continue;}let relative=file.trim_start_matches(&format!("{root}/"));let Some(source)=read(project,relative)else{continue;};let Some(full)=crate::eloquent_members::declaration(&source)else{continue;};let Some(local)=full.strip_prefix(&format!("{}\\",declared.namespace))else{continue;};let Some(tag)=name(&declared.tag_namespace,&declared.prefix,&class_tag(local))else{continue;};if let Some(entry)=class_entry(project,&roots,&full){insert(index,&tag,entry,true,declared.bare);}}
        }}
    for declared in &config.view_paths{if !active(){break;}index.dependency_roots.insert(declared.path.clone());let Ok(files)=project.find_files(&format!("{}/**/*.blade.php",declared.path),513)else{continue;};if files.len()>=513{index.partial=true;continue;}
        for file in files{if !active(){break;}if !owned(project,&file)||config.ignore_view_paths.iter().any(|p|file.starts_with(&format!("{root}/{}/",p.trim_end_matches('/')))){continue;}
            let Some(local)=file.strip_prefix(&format!("{root}/{}/",declared.path.trim_end_matches('/'))).and_then(|s|s.strip_suffix(".blade.php"))else{continue;};let full=local.replace('/',".");let primary=full.strip_suffix(".index").unwrap_or(&full);
            for local in BTreeSet::from([full.as_str(),primary]){let Some(tag)=name(&declared.namespace,&declared.prefix,local)else{continue;};let entry=Entry{detail:Some("ide.json anonymous component".into()),path:file.clone(),at:Position::new(0,0),width:0};index.metadata.blade_components.resolved_views.insert(tag.clone(),entry.clone());insert(index,&tag,entry,false,false);}
        }}
    for declared in &config.from_config{if !active(){break;}let relative=format!("config/{}.php",declared.file.replace('.',"/"));index.dependency_files.insert(relative.clone());let Some(source)=read(project,&relative)else{continue;};let masked=php::mask_comments(&source);let Some(open)=returned_array(&masked)else{continue;};let Some(array)=value_at(&masked,open,&declared.key).and_then(|range|php::array_open(&masked,range))else{continue;};
        for item in php::items(&masked,array){if !active(){break;}let component=php::array_open(&masked,item.value.clone());let field=|key:&str|->Result<Option<String>,()>{match component.and_then(|c|value_at(&masked,c,key)).or_else(||value_at(&masked,open,key)){None=>Ok(None),Some(range)=>string(&masked,range).map(Some).ok_or(())}};
            let class_range=extract(&masked,&item,&declared.class);
            let full=class_range.clone().and_then(|range|{string(&masked,range.clone()).and_then(|s|class(&s)).or_else(||{let range=php::trimmed(&masked,range);let raw=masked[range].strip_suffix("::class")?.trim();let probe=format!("{source}\n{raw}::__component_binding_probe();");let open=probe.rfind('(')?;crate::eloquent_members::metadata_inference::call_identity(&probe,open)?.class})});
            if class_range.is_some()&&full.is_none(){index.metadata.issues.push(format!("{relative}: dynamic component class withheld"));continue;}
            let class_target=full.as_ref().and_then(|full|class_entry(project,&roots,full));
            if full.is_some()&&class_target.is_none(){index.metadata.issues.push(format!("{relative}: component class has no unambiguous owned Composer source"));continue;}
            let explicit_alias=match declared.alias_key.as_ref(){Some(key)=>match field(key){Ok(Some(alias))=>Some(alias),_=>continue},None=>None};
            let alias=explicit_alias.or_else(||item.key.clone().and_then(|range|string(&masked,range))).or_else(||full.as_ref().and_then(|f|f.rsplit('\\').next()).map(class_tag));let Some(alias)=alias else{continue;};
            let (Ok(namespace),Ok(prefix))=(field(&declared.namespace_key),field(&declared.prefix_key))else{index.metadata.issues.push(format!("{relative}: dynamic Blade component prefix/namespace withheld"));continue;};
            let Some(tag)=name(&namespace.unwrap_or_default(),&prefix.unwrap_or_default(),&alias)else{continue;};
            if let Some(entry)=class_target{insert(index,&tag,entry,true,false);}
            if let Some(view)=extract(&masked,&item,&declared.view).and_then(|range|string(&masked,range)){if let Some(entry)=index.views.get(&view).filter(|e|!e.path.is_empty()&&owned(project,&e.path)).cloned(){index.metadata.blade_components.resolved_views.insert(tag.clone(),entry.clone());insert(index,&tag,entry,false,false);}}
        }
    }
    if !active(){index.partial=true;index.metadata.issues.push("Blade component metadata source discovery reached its 750ms / 128-source / 2MiB aggregate bound".into());}
}

#[cfg(test)]
mod tests{
    use super::*;use openidea_plugin::{mock::MockHost,providers::{Providers,Document}};
    fn setup()->(MockHost,Project){let h=MockHost::install().with_capabilities(&["project.read","editor.read"]);
        h.add_file("composer.json",r#"{"require":{"laravel/framework":"^12"},"autoload":{"psr-4":{"App\\":"app/","Acme\\UI\\":"ui/"}}}"#);h.add_file("artisan","<?php");
        h.add_file("ui/Components/Card.php","<?php\nnamespace Acme\\UI\\Components;\nclass Card { public function __construct(public string $heading) {} }");
        h.add_file("ui/Bare/Button.php","<?php\nnamespace Acme\\UI\\Bare;\nclass Button { public function __construct(public string $label) {} }");
        h.add_file("custom/views/card.blade.php","@props(['tone']) {{ $footer }} {{ $slot }}");
        h.add_file("resources/views/shared/card.blade.php","@props(['style']) {{ $header }}");
        h.add_file("resources/views/components/ignored.blade.php","ignored");let p=h.invocation().project().unwrap();(h,p)
    }
    fn source(text:&str)->Document{Document{path:"/project/resources/views/page.blade.php".into(),language:"blade".into(),version:1,text:text.into()}}
    fn complete(main:&mut crate::Main,text:&str)->Vec<String>{main.completion(&source(text),text::position(text,text.len()),None).into_iter().map(|i|i.label).collect()}
    #[test]fn all_four_metadata_sources_feed_actual_providers_without_php_execution(){
        let(h,p)=setup();h.add_file("ide.json",r#"{"blade":{"components":{"viewPaths":[{"path":"/custom/views","namespace":"pkg","prefix":"a-"}],"phpNamespaces":[{"phpNamespace":"Acme\\UI\\Components","tagNamespace":"pkg"},{"phpNamespace":"Acme\\UI\\Bare","tagNamespace":"flux","ignoreBladeComponentPrefix":true}],"list":[{"name":"special","namespace":"pkg","className":"Acme\\UI\\Components\\Card"}],"fromConfig":[{"configFileName":"ui","componentsKey":"components","aliasKey":"alias","classFetch":{"type":"arrayKey","key":"class"},"viewFetch":{"type":"arrayKey","key":"view"}}],"ignoreViewPaths":["/resources/views/components"]}}}"#);
        h.add_file("config/ui.php","<?php return ['namespace'=>'cfg', 'prefix'=>'b-', 'components'=>['card'=>['alias'=>'box','class'=>'Acme\\UI\\Components\\Card','view'=>'shared.card']]];");
        let index=Index::build(&p);assert!(!index.partial,"{:?}",index.metadata.issues);for name in ["pkg::a-card","pkg::card","pkg::special","cfg::b-box","flux::button"]{assert!(index.components.contains_key(name),"{name}: {:?}",index.metadata.issues);}
        assert!(!index.components.contains_key("ignored"));assert!(index.metadata.blade_components.bare_tags.contains("flux::button"));
        let mut main=crate::Main::default();main.indexes.insert("/project".into(),index);
        assert!(complete(&mut main,"<x-pkg::a-card to").contains(&"tone".into()));
        assert!(complete(&mut main,"<x-pkg::card he").contains(&"heading".into()));
        assert!(complete(&mut main,"<x-cfg::b-box he").contains(&"heading".into()));
        assert!(complete(&mut main,"<x-cfg::b-box><x-slot:he").contains(&"header".into()));
        assert!(complete(&mut main,"<flux::bu").contains(&"flux::button".into()));
        assert!(complete(&mut main,"<flux::button la").contains(&"label".into()));
        for text in ["<x-pkg::card/>","</x-pkg::card>","<flux::button/>","</flux::button>"]{let needle=if text.contains("card"){ "card" }else{"button"};let at=text::position(text,text.find(needle).unwrap()+2);let found=main.definition(&source(text),at);assert!(found.iter().any(|entry|entry.path.starts_with("/project/ui/")),"{text}");}
        assert!(h.process_runs().is_empty());
        h.add_project("/project/ui");assert!(complete(&mut main,"<x-pkg::card he").is_empty());assert!(main.definition(&source("<flux::button/>"),Position::new(0,9)).is_empty());
    }
    #[test]fn metadata_class_sources_keep_precedence_and_declarations_match_owned_paths(){
        let(h,p)=setup();h.add_file("ide.json",r#"{"blade":{"components":{"viewPaths":[{"path":"/custom/views","namespace":"pkg"}],"phpNamespaces":[{"phpNamespace":"Acme\\UI\\Components","tagNamespace":"pkg"}]}}}"#);
        let index=Index::build(&p);assert!(index.components["pkg::card"].path.ends_with("/ui/Components/Card.php"));
        h.add_file("ui/Components/Card.php","<?php namespace Wrong; class Card {}");let index=Index::build(&p);assert!(index.components["pkg::card"].path.ends_with("/custom/views/card.blade.php"));
    }
    #[test]fn config_fetch_never_uses_quoted_returns_or_dynamic_class_prefix_bindings(){
        let(h,p)=setup();h.add_file("ide.json",r#"{"blade":{"components":{"fromConfig":[{"configFileName":"ui","componentsKey":"components","classFetch":{"type":"arrayKey","key":"class"},"viewFetch":{"type":"arrayKey","key":"view"}}]}}}"#);
        for source in [
            "<?php $fake=\"return ['components'=>['fake'=>['class'=>'Acme\\UI\\Components\\Card','view'=>'shared.card']]];\"; return [];",
            "<?php return ['components'=>['fake'=>['class'=>$unknown,'view'=>'shared.card']]];",
            "<?php return ['prefix'=>$unknown,'components'=>['fake'=>['class'=>'Acme\\UI\\Components\\Card','view'=>'shared.card']]]];",
        ]{h.add_file("config/ui.php",source);let index=Index::build(&p);assert!(!index.components.contains_key("fake"),"{source}");}
        h.add_file("config/ui.php","<?php use Acme\\UI\\Components\\Card as ComponentClass; return ['components'=>['safe'=>['class'=>ComponentClass::class,'view'=>'shared.card']]];");
        let index=Index::build(&p);assert!(index.components["safe"].path.ends_with("/ui/Components/Card.php"));
    }
    #[test]fn invalid_declarations_and_dynamic_config_do_not_invent_component_sources(){
        let(h,p)=setup();h.add_file("ide.json",r#"{"blade":{"components":{"viewPaths":[{"path":"../../outside","namespace":"bad"}],"phpNamespaces":[{"phpNamespace":5}],"list":[{"name":"bad'quote"}],"fromConfig":[{"configFileName":"ui","componentsKey":"components","classFetch":{"type":"eval"}}]}}}"#);
        let index=Index::build(&p);assert!(!index.metadata.issues.is_empty());assert!(!index.components.contains_key("bad"));
        h.add_file("ide.json",r#"{"blade":{"components":{"fromConfig":[{"configFileName":"ui","componentsKey":"components"}]}}}"#);h.add_file("config/ui.php","<?php return ['components' => $dynamic];");let index=Index::build(&p);assert!(!index.components.contains_key("dynamic"));
    }
}
