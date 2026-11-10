//! Reviewed filename-derived view/anonymous-component Rename. Names derive from
//! one currently owned indexed root; all edits come from framework references.
use std::collections::{BTreeMap,BTreeSet};
use crate::{Index,refs::{self,Kind}};
use openidea_plugin::{Error,Project,Range,TextEdit,FileKind,host,text,providers::Document,file_actions::{FileAction,FileOperation}};
const LIMIT:usize=512*1024;
#[derive(Clone,Debug,PartialEq,Eq,serde::Serialize,serde::Deserialize)]
struct Candidate {name:String,path:String,directory:String,extension:String,namespace:Option<String>}
#[derive(serde::Serialize,serde::Deserialize)]
struct Request{root:String,doc:String,version:u64,hash:String,selection:Range,candidate:Candidate}
fn hash(source:&str)->String{crate::artisan_metadata::hash(source)}
fn parts(name:&str)->Option<(Option<&str>,&str)>{
    let (namespace,local)=name.split_once("::").map_or((None,name),|(a,b)|(Some(a),b));
    if namespace.is_some_and(|n|n.is_empty()||!n.chars().all(|c|c.is_alphanumeric()||matches!(c,'_'|'-'|'.'))||n.contains(".."))||local.is_empty()||local.len()>512||local.split('.').any(|part|part.is_empty()||!part.chars().all(|c|c.is_alphanumeric()||matches!(c,'_'|'-'))){return None;}
    Some((namespace,local))
}
fn class_tags(project:&Project)->Result<BTreeSet<String>,Error>{
    let root=project.root()?;let prefix=format!("{root}/app/View/Components/");let paths=project.find_files("app/View/Components/**/*.php",257)?;
    if paths.len()>=257{return Err(Error::Invalid("Component class enumeration exceeds its 256-file bound.".into()));}
    Ok(paths.into_iter().filter_map(|path|{let name=path.strip_prefix(&prefix)?.strip_suffix(".php")?;Some(name.split('/').map(|part|crate::index::separated(part,'-')).collect::<Vec<_>>().join("."))}).collect())
}
fn unique_source(index:&Index,project:&Project,name:&str,expected:&str)->bool{
    let Some((namespace,local))=parts(name)else{return false;};let Ok(root)=project.root()else{return false;};
    let mut roots:Vec<(Option<String>,String)>=["resources/views","app/views","resources/templates"].into_iter().map(|p|(None,p.into())).collect();
    roots.extend(index.metadata.view_roots.clone());
    roots.extend(crate::index::ide_blade_roots(|path|{
        if !crate::owns_current_path(project,&format!("{root}/{path}")){return Err(Error::Stale);}project.read_file(path)
    }));
    if let Some(paths)=host::setting_string("viewPaths"){roots.extend(paths.split(',').map(str::trim).filter(|p|!p.is_empty()).map(|p|(None,p.into())));}
    let mut found=BTreeSet::new();
    for (declared,directory) in roots{if directory.starts_with('/')||directory.split('/').any(|p|p==".."||p=="."){continue;}
        let logical=match (declared.as_deref(),namespace){(Some(a),Some(b))if a==b=>local.replace('.',"/"),(None,None)=>local.replace('.',"/"),(None,Some(namespace))=>format!("vendor/{namespace}/{}",local.replace('.',"/")),_=>continue};
        for extension in [".blade.php",".php"]{let path=format!("{root}/{directory}/{logical}{extension}");
            if project.exists(&path).unwrap_or(false){if !crate::owns_current_path(project,&path){return false;}found.insert(path);}
        }
    }
    found.len()==1&&found.contains(expected)
}
fn word(byte:u8)->bool{byte.is_ascii_alphanumeric()||byte==b'_'||byte>=128}
fn declared(source:&str,keyword:&str,name:&str)->bool{
    let Some(code)=crate::package_facts::code(source)else{return true;};
    for(at,_)in code.to_ascii_lowercase().match_indices(keyword){if at>0&&word(code.as_bytes()[at-1])||code.as_bytes().get(at+keyword.len()).is_some_and(|b|word(*b)){continue;}
        let tail=code[at+keyword.len()..].trim_start().trim_start_matches('&').trim_start();let length=tail.bytes().take_while(|b|word(*b)).count();
        if tail[..length].eq_ignore_ascii_case(name){return true;}
    }false
}
pub(crate) fn facade(source:&str,raw:&str,expected:&str)->bool{
    let full=if raw.starts_with('\\'){raw.trim_start_matches('\\').to_owned()}else{
        // Reuse the PHP binding parser, including grouped/case-insensitive and
        // newline-separated imports; a short-name suffix is not class proof.
        let probe=format!("{source}\n{raw}::__laravel_binding_probe();");
        let Some(open)=probe.rfind('(')else{return false;};
        let Some(full)=crate::eloquent_members::metadata_inference::call_identity(&probe,open).and_then(|identity|identity.class)else{return false;};full
    };
    full.eq_ignore_ascii_case(&format!("Illuminate\\Support\\Facades\\{expected}"))
        ||full.eq_ignore_ascii_case(expected)&&!["class","interface","trait","enum"].iter().any(|keyword|declared(source,keyword,expected))
}

fn callable(code:&str,open:usize)->(&str,usize){
    let before=code[..open].trim_end();let start=before.rfind(|c:char|!(c.is_alphanumeric()||matches!(c,'_'|'\\'|'$'|':'|'>'|'-'|'?'|'@'))).map_or(0,|at|at+1);
    (&before[start..],start)
}
pub(crate) fn binding_budget(bytes:usize,references:usize)->bool{references<=128&&bytes.checked_mul(references.max(1)).is_some_and(|work|work<=8*1024*1024)}
/// Source-level binding gate shared with reviewed scalar Rename. Unknown object
/// methods are deliberately withheld; conventional unimported helpers retain PHP
/// global fallback only when this source declares/imports no conflicting name.
pub(crate) fn binding_safe(index:&Index,doc:&Document,reference:&refs::Reference)->bool{
    if reference.kind==Kind::Component{return true;}
    let Some(code)=crate::package_facts::code(&doc.text)else{return false;};
    let mut stack=Vec::new();for(at,b)in code.bytes().enumerate().take(reference.start){match b{b'('=>stack.push((b,at)),b'['|b'{'=>stack.push((b,at)),b')'|b']'|b'}'=>{stack.pop();},_=>{}}}
    let Some((open,raw,start))=stack.into_iter().rev().filter(|(b,_)|*b==b'(').find_map(|(_,open)|{let(raw,start)=callable(&code,open);(!raw.is_empty()&&raw!="array").then_some((open,raw,start))})else{return false;};
    if let Some(name)=raw.strip_prefix('@'){return crate::language_of(&doc.path,&doc.language)==Some("blade") && !index.directives.keys().any(|custom|custom.eq_ignore_ascii_case(name));}
    if let Some((class,_method))=raw.rsplit_once("::"){
        return match reference.kind{Kind::View=>["View","Route","Mail"].iter().any(|name|facade(&doc.text,class,name)),Kind::Config=>facade(&doc.text,class,"Config"),Kind::Translation=>facade(&doc.text,class,"Lang"),Kind::Route=>["Route","URL","Redirect"].iter().any(|name|facade(&doc.text,class,name)),_=>false};
    }
    if raw.contains("->"){return false;}
    if code[..start].trim_end().ends_with("new"){
        return reference.kind==Kind::View&&crate::eloquent_members::metadata_inference::call_identity(&doc.text,open).and_then(|identity|identity.class).as_deref()==Some("Illuminate\\Mail\\Mailables\\Content");
    }
    let expected:&[&str]=match reference.kind{Kind::View=>&["view"],Kind::Config=>&["config"],Kind::Translation=>&["__","trans","trans_choice"],Kind::Route=>&["route","to_route","link_to_route"],_=>return false};
    let global=raw.strip_prefix('\\').unwrap_or(raw);if global.contains('\\')||!expected.iter().any(|name|global.eq_ignore_ascii_case(name))||declared(&doc.text,"function",global){return false;}
    if raw.starts_with('\\'){return true;}
    if let Some(identity)=crate::eloquent_members::metadata_inference::call_identity(&doc.text,open){
        return identity.class.is_none()&&identity.function_fqn.as_deref().is_none_or(|name|name.eq_ignore_ascii_case(global));
    }
    // If the binding parser cannot prove imported function names, withhold the
    // bare call. Explicit globals above do not inherit these aliases.
    !code.to_ascii_lowercase().contains("use function")
}
/// Remove foreign short Route calls before parsing route declarations. Source
/// positions and true fully qualified Laravel calls remain unchanged.
pub(crate) fn route_source(source:&str)->Option<String>{
    let code=crate::package_facts::code(source)?;let mut out=crate::php::mask_comments(source).into_bytes();
    for(at,_)in code.match_indices("Route::"){
        let start=code[..at].rfind(|c:char|!(c.is_alphanumeric()||matches!(c,'_'|'\\'))).map_or(0,|n|n+1);
        let raw=&code[start..at+"Route".len()];if !facade(source,raw,"Route"){for byte in &mut out[at..at+"Route".len()]{*byte=b' ';}}
    }String::from_utf8(out).ok()
}
fn candidate(index:&Index,project:&Project,doc:&Document,selection:Range)->Option<Candidate>{
    if index.partial||doc.text.len()>LIMIT||!crate::owns_current_path(project,&doc.path){return None;}
    let language=crate::language_of(&doc.path,&doc.language)?;let at=text::offset(&doc.text,selection.start)?;
    let path=if let Some(reference)=refs::reference_at(language,&doc.text,at).filter(|r|r.closed){
        if text::offset(&doc.text,selection.end)?>reference.end||!binding_safe(index,doc,&reference){return None;}
        let name=doc.text.get(reference.start..reference.end)?;
        match reference.kind{Kind::View=>index.views.get(name)?.path.clone(),Kind::Component=>{if index.open_components||class_tags(project).ok()?.contains(name){return None;}index.components.get(name)?.path.clone()},_=>doc.path.clone()}
    }else{doc.path.clone()};
    if !crate::owns_current_path(project,&path){return None;}
    let mut names=index.views.iter().filter(|(_,entry)|entry.path==path);let (name,_)=names.next()?;if names.next().is_some(){return None;}
    let (namespace,local)=parts(name)?;
    if !unique_source(index,project,name,&path){return None;}
    let extension=if path.ends_with(".blade.php"){".blade.php"}else if path.ends_with(".php"){".php"}else{return None;};
    let suffix=format!("{}{extension}",local.replace('.',"/"));let directory=path.strip_suffix(&suffix)?;
    if !directory.ends_with('/')||!crate::owns_current_path(project,directory.trim_end_matches('/')){return None;}
    // Every published anonymous alias must be one this adapter can scan. PHP
    // class components never share a filename identity with an indexed view.
    if index.components.iter().any(|(alias,entry)|entry.path==path&&(alias.contains("::")||parts(alias).is_none())){return None;}
    let directory=directory.to_owned();Some(Candidate{name:name.clone(),path,directory,extension:extension.into(),namespace:namespace.map(str::to_owned)})
}
pub(crate) fn actions(index:&Index,doc:&Document,selection:Range)->Vec<FileAction>{
    let run=||->Option<FileAction>{let project=crate::project_for(&doc.path)?;let root=project.root().ok()?;let candidate=candidate(index,&project,doc,selection)?;
        let request=Request{root,doc:doc.path.clone(),version:doc.version,hash:hash(&doc.text),selection,candidate:candidate.clone()};
        let data=format!("view-rename:{}",serde_json::to_string(&request).ok()?);
        Some(FileAction{title:format!("Rename or move Blade/PHP view {}…",candidate.name),kind:Some("refactor.rename".into()),data:Some(data),..Default::default()})
    };run().into_iter().collect()
}
fn new_target(old:&Candidate,input:&str)->Result<(String,String),Error>{
    let (namespace,local)=parts(input).ok_or_else(||Error::Invalid("Use a dotted view name with nonempty letter/digit/underscore/hyphen segments.".into()))?;
    if namespace.is_some()&&namespace!=old.namespace.as_deref(){return Err(Error::Invalid("Keep the existing view namespace; moving between roots is not supported by this action.".into()));}
    let name=old.namespace.as_ref().map_or_else(||local.to_owned(),|n|format!("{n}::{local}"));
    let target=format!("{}{}{}",old.directory,local.replace('.',"/"),old.extension);
    Ok((name,target))
}
fn target_clear(index:&Index,project:&Project,old:&Candidate,new_name:&str,target:&str)->Result<(),Error>{
    if new_name==old.name||target==old.path{return Err(Error::Cancelled);}
    if !crate::owns_current_path(project,&old.path)||!crate::owns_current_path(project,target){return Err(Error::Stale);}
    if index.views.contains_key(new_name)||project.exists(target)?{return Err(Error::Invalid("A view or file already exists at the selected target.".into()));}
    let parent=target.rsplit_once('/').map(|p|p.0).ok_or(Error::Stale)?;
    if !project.stat(parent).ok().is_some_and(|stat|stat.kind==FileKind::Directory){return Err(Error::Invalid("Choose an existing target directory inside this view root.".into()));}
    Ok(())
}
fn tags(index:&Index,project:&Project,old:&Candidate,target:&str)->Result<BTreeMap<String,String>,Error>{
    let mut map=BTreeMap::new();let classes=class_tags(project)?;let Some((_,relative))=old.path.split_once("/resources/views/components/")else{return Ok(map);};
    let old_suffix=relative.strip_suffix(".blade.php").ok_or(Error::Stale)?;
    let root=old.path.strip_suffix(relative).ok_or(Error::Stale)?;
    let next=target.strip_prefix(root).and_then(|s|s.strip_suffix(".blade.php")).ok_or_else(||Error::Invalid("Move anonymous components only within resources/views/components.".into()))?;
    let full=next.replace('/',".");let primary=full.strip_suffix(".index").unwrap_or(&full).to_owned();
    let mut next_aliases=BTreeSet::from([full.clone(),primary.clone()]);
    for (alias,entry) in &index.components {if entry.path!=old.path||classes.contains(alias){continue;}
        let old_full=old_suffix.replace('/',".");let old_primary=old_full.strip_suffix(".index").unwrap_or(&old_full);
        let replacement=if alias==&old_full{full.clone()}else if alias==old_primary{primary.clone()}else{return Err(Error::Stale);};
        next_aliases.insert(replacement.clone());map.insert(alias.clone(),replacement);
    }
    if map.is_empty(){return Ok(map);}if index.open_components{return Err(Error::Invalid("Dynamic component registrations prevent proving anonymous tag ownership.".into()));}
    for alias in next_aliases{if classes.contains(&alias)||index.components.get(&alias).is_some_and(|entry|entry.path!=old.path){return Err(Error::Invalid(format!("Component tag {alias} is already defined by another source.")));}}
    Ok(map)
}
#[derive(Debug,PartialEq,Eq)]
struct Change {path:String,source:String,edits:Vec<(Range,String)>}
fn references(doc:&Document)->Vec<refs::Reference>{
    let Some(language)=crate::language_of(&doc.path,&doc.language)else{return vec![];};let mut refs=refs::references(language,&doc.text);
    if language=="blade"{refs.extend(refs::tag_references(&doc.text));let mut closing=doc.text.as_bytes().to_vec();for(at,_)in doc.text.match_indices("</x-"){closing[at]=b'/';closing[at+1]=b'<';}
        if let Ok(closing)=String::from_utf8(closing){refs.extend(refs::tag_references(&closing));}
    }
    refs.sort_by_key(|r|(r.start,r.end));refs.dedup_by(|a,b|a.kind==b.kind&&a.start==b.start&&a.end==b.end);refs
}
fn plan(index:&Index,project:&Project,doc:&Document,old:&Candidate,new_name:&str,target:&str)->Result<Vec<Change>,Error>{
    target_clear(index,project,old,new_name,target)?;let component_tags=tags(index,project,old,target)?;
    let root=project.root()?;let found=project.find_files("**/*.php",1025)?;if found.len()>=1025{return Err(Error::Invalid("View rename source enumeration exceeds its 1024-file bound; no partial rename is offered.".into()));}
    let mut paths:BTreeSet<_>=found.into_iter().filter(|path|{let rel=path.strip_prefix(&root).and_then(|s|s.strip_prefix('/')).unwrap_or(path);!["vendor/","storage/","node_modules/","bootstrap/cache/"].iter().any(|p|rel.starts_with(p))}).collect();paths.insert(doc.path.clone());paths.insert(old.path.clone());
    let mut out=Vec::new();let mut total=0usize;
    for path in paths{if !crate::owns_current_path(project,&path){continue;}
        let source=if path==doc.path{doc.text.clone()}else{project.read_file(&path)?};total=total.checked_add(source.len()).ok_or(Error::Stale)?;
        if source.len()>LIMIT||total>4*1024*1024{return Err(Error::Invalid("View rename source scan exceeds its review bound.".into()));}
        let source_doc=Document{path:path.clone(),text:source.clone(),language:crate::language_of(&path,"").unwrap_or("php").into(),version:0};
        let refs=references(&source_doc);if !binding_budget(source.len(),refs.len()){return Err(Error::Invalid("View rename binding analysis exceeds its per-source 128-reference / 8 MiB work bound; no partial rename is offered.".into()));}
        let mut edits=Vec::new();for reference in refs{if !reference.closed||!binding_safe(index,&source_doc,&reference){continue;}let Some(name)=source.get(reference.start..reference.end)else{continue;};
            let replacement=match reference.kind {Kind::View if name==old.name=>Some(new_name.to_owned()),Kind::Component=>component_tags.get(name).cloned(),_=>None};
            if let Some(replacement)=replacement{edits.push((Range::new(text::position(&source,reference.start),text::position(&source,reference.end)),replacement));}
        }
        if !edits.is_empty()||path==old.path{out.push(Change{path,source,edits});}
    }
    if !out.iter().any(|c|c.path==old.path){return Err(Error::Stale);}Ok(out)
}
pub(crate) fn resolve(_index:&Index,doc:&Document,action:FileAction)->Result<FileAction,Error>{
    let data=action.data.as_deref().and_then(|s|s.strip_prefix("view-rename:")).filter(|s|s.len()<8192).ok_or(Error::Stale)?;
    let request:Request=serde_json::from_str(data).map_err(|_|Error::Stale)?;
    if request.doc!=doc.path||request.version!=doc.version||request.hash!=hash(&doc.text){return Err(Error::Stale);}
    let project=crate::project_for(&doc.path).ok_or(Error::Stale)?;if project.root()?!=request.root||project.read_file(&doc.path)?!=doc.text{return Err(Error::Stale);}
    let index=Index::build(&project);if candidate(&index,&project,doc,request.selection).as_ref()!=Some(&request.candidate){return Err(Error::Stale);}
    let input=host::input("New dotted view name (existing namespace/root retained)",Some(&request.candidate.name))?.ok_or(Error::Cancelled)?;
    let (name,target)=new_target(&request.candidate,&input)?;
    let before=plan(&index,&project,doc,&request.candidate,&name,&target)?;
    let mut preview=format!("Project: {}\nRename view {} → {}\nMove {} → {}\n\nOnly proven PHP/Blade view references and indexed anonymous-component tags are updated. Dynamic references and PHP class names/imports are unchanged.\n\n",request.root,request.candidate.name,name,request.candidate.path,target);
    let mut edits=Vec::new();
    for change in &before{let mut after=change.source.clone();for(range,replacement)in change.edits.iter().rev(){let start=text::offset(&change.source,range.start).ok_or(Error::Stale)?;let end=text::offset(&change.source,range.end).ok_or(Error::Stale)?;after.replace_range(start..end,replacement);}
        preview.push_str(&format!("{}\nComplete resulting contents:\n{}\n\n",change.path,ui_kit::prompts::readable_source(&after)));
        if !change.edits.is_empty(){edits.push((if change.path==request.candidate.path{target.clone()}else{change.path.clone()},change.edits.iter().map(|(range,text)|TextEdit{range:*range,text:text.clone()}).collect()));}
    }
    if !ui_kit::prompts::confirm_exact("Rename view and its proven references?",&preview)?{return Err(Error::Cancelled);}
    let latest=Index::build(&project);if project.read_file(&doc.path)?!=doc.text||candidate(&latest,&project,doc,request.selection).as_ref()!=Some(&request.candidate)||plan(&latest,&project,doc,&request.candidate,&name,&target)?!=before{return Err(Error::Stale);}
    Ok(FileAction{title:format!("Rename view {} → {name}",request.candidate.name),operations:vec![FileOperation::Rename{from:request.candidate.path,to:target.clone()}],edits,open:Some(target),..Default::default()})
}

#[cfg(test)]
mod tests{
    use super::*;use openidea_plugin::mock::MockHost;
    fn setup()->(MockHost,Project,Index){let h=MockHost::install().with_capabilities(&["project.read"]);
        h.add_file("composer.json",r#"{"require":{"laravel/framework":"^12.0"}}"#);h.add_file("artisan","<?php");
        h.add_dir("resources/views/posts");h.add_dir("resources/views/components");
        h.add_file("resources/views/posts/show.blade.php","<p>😀 {{ __('Hello') }}</p>");
        h.add_file("app/Http/Controllers/PostController.php","<?php class PostController { function show(){ return view('posts.show'); }}");
        let p=h.invocation().project().unwrap();let index=Index::build(&p);assert!(!index.partial);(h,p,index)
    }
    fn controller(h:&MockHost)->Document{let source=h.invocation().project().unwrap().read_file("app/Http/Controllers/PostController.php").unwrap();Document{path:"/project/app/Http/Controllers/PostController.php".into(),language:"php".into(),version:1,text:source}}
    fn caret(doc:&Document,needle:&str)->Range{let at=text::position(&doc.text,doc.text.find(needle).unwrap()+1);Range::new(at,at)}
    fn run(h:&MockHost,index:&Index,doc:&Document,old:&str,new:&str)->Result<FileAction,Error>{let action=actions(index,doc,caret(doc,old)).remove(0);h.answer_next_prompt(Some(&[new]));h.answer_next_prompt(Some(&["yes"]));resolve(index,doc,action)}
    #[test]fn view_rename_returns_one_move_and_only_proven_literal_reference_edits(){
        let(h,p,index)=setup();let doc=controller(&h);h.add_file("app/Other.php","<?php $unrelated='posts.show'; /* view('posts.show') */");
        let result=run(&h,&index,&doc,"posts.show","posts.detail").unwrap();assert_eq!(result.operations.len(),1);
        assert!(matches!(&result.operations[0],FileOperation::Rename{from,to}if from=="/project/resources/views/posts/show.blade.php"&&to=="/project/resources/views/posts/detail.blade.php"));
        assert_eq!(result.edits.len(),1);assert_eq!(result.edits[0].1.len(),1);assert_eq!(result.edits[0].1[0].text,"posts.detail");
        assert!(p.exists("resources/views/posts/show.blade.php").unwrap());assert!(!p.exists("resources/views/posts/detail.blade.php").unwrap());
    }
    #[test]fn namespace_and_unicode_root_remain_exact_and_foreign_namespace_is_refused(){
        let(h,p,_)=setup();h.add_file("ide-blade.json",r#"{"namespaces":[{"namespace":"admin","path":"modules/admin/views"}]}"#);
        h.add_dir("modules/admin/views/états");h.add_file("modules/admin/views/états/show.blade.php","<p>État</p>");
        h.add_file("app/Http/Controllers/PostController.php","<?php /* 😀 */ class PostController { function show(){return view('admin::états.show');}}");
        let index=Index::build(&p);let doc=controller(&h);let result=run(&h,&index,&doc,"admin::états.show","états.détail").unwrap();
        assert_eq!(result.edits[0].1[0].text,"admin::états.détail");
        assert!(matches!(&result.operations[0],FileOperation::Rename{to,..}if to=="/project/modules/admin/views/états/détail.blade.php"));
        let old=candidate(&index,&p,&doc,caret(&doc,"admin::états.show")).unwrap();assert!(new_target(&old,"other::états.detail").is_err());
    }
    #[test]fn target_collision_and_stale_old_source_do_not_return_partial_moves(){
        let(h,p,index)=setup();let doc=controller(&h);let action=actions(&index,&doc,caret(&doc,"posts.show")).remove(0);
        h.add_file("resources/views/posts/detail.blade.php","existing");h.answer_next_prompt(Some(&["posts.detail"]));assert!(resolve(&index,&doc,action).is_err());
        let action=actions(&index,&doc,caret(&doc,"posts.show")).remove(0);h.add_project("/project/resources/views/posts");h.answer_next_prompt(Some(&["posts.another"]));assert!(resolve(&index,&doc,action).is_err());
        assert_eq!(p.read_file("resources/views/posts/detail.blade.php").unwrap(),"existing");
    }
    #[test]fn anonymous_component_open_close_and_self_view_refs_follow_atomic_move(){
        let(h,p,_)=setup();h.add_file("resources/views/components/card.blade.php","@include('components.card') <p>{{ $slot }}</p>");
        h.add_file("resources/views/page.blade.php","<x-card>Text</x-card> {{-- <x-card /> --}} @php $x='<x-card />'; @endphp");
        let source=p.read_file("resources/views/page.blade.php").unwrap();let doc=Document{path:"/project/resources/views/page.blade.php".into(),language:"blade".into(),version:2,text:source};
        let index=Index::build(&p);let result=run(&h,&index,&doc,"card","components.notice").unwrap();
        let page=result.edits.iter().find(|(path,_)|path.ends_with("/page.blade.php")).unwrap();assert_eq!(page.1.len(),2);assert!(page.1.iter().all(|e|e.text=="notice"));
        let moved=result.edits.iter().find(|(path,_)|path.ends_with("/components/notice.blade.php")).unwrap();assert_eq!(moved.1[0].text,"components.notice");
    }
    #[test]fn aliases_path_traversal_and_class_components_are_withheld(){
        let(h,p,_)=setup();h.add_file("resources/views/components/card.blade.php","card");
        h.add_file("app/View/Components/Card.php","<?php class Card extends Component { public function render(){return view('components.card');}}");
        let index=Index::build(&p);let doc=Document{path:"/project/resources/views/page.blade.php".into(),language:"blade".into(),version:1,text:"<x-card/>".into()};assert!(actions(&index,&doc,caret(&doc,"card")).is_empty());
        let old=candidate(&index,&p,&controller(&h),caret(&controller(&h),"posts.show")).unwrap();
        for name in ["../other","posts..other","posts/other","posts.bad'","bad::name"]{assert!(new_target(&old,name).is_err(),"{name}");}
    }
    #[test]fn duplicate_view_roots_and_component_class_targets_are_refused(){
        let(h,p,_)=setup();h.add_file("app/views/posts/show.blade.php","shadowed same logical name");let index=Index::build(&p);let doc=controller(&h);
        assert!(actions(&index,&doc,caret(&doc,"posts.show")).is_empty());
        h.add_file("resources/views/components/card.blade.php","card");h.add_file("app/View/Components/Notice.php","<?php class Notice extends Component {}");
        let source="<x-card/>";h.add_file("resources/views/page.blade.php",source);let doc=Document{path:"/project/resources/views/page.blade.php".into(),language:"blade".into(),version:1,text:source.into()};let index=Index::build(&p);
        let action=actions(&index,&doc,caret(&doc,"card")).remove(0);h.answer_next_prompt(Some(&["components.notice"]));assert!(resolve(&index,&doc,action).is_err());
        assert!(p.exists("resources/views/components/card.blade.php").unwrap());assert!(!p.exists("resources/views/components/notice.blade.php").unwrap());
    }
    #[test]fn foreign_helpers_facades_and_unknown_methods_are_never_selected_or_edited(){
        let(h,p,index)=setup();
        for source in [
            "<?php use function Vendor\\view; view('posts.show');",
            "<?php namespace App; function view($name) {} view('posts.show');",
            "<?php use App\\View; View::make('posts.show');",
            "<?php USE App\\View; View::make('posts.show');",
            "<?php use\nApp\\View; View::make('posts.show');",
            "<?php FUNCTION view($name) {} \\view('posts.show');",
            "<?php class View {static function make($name){}} View::make('posts.show');",
            "<?php $unknown->view('posts.show');",
        ] {let doc=Document{path:"/project/app/Foreign.php".into(),language:"php".into(),version:1,text:source.into()};assert!(actions(&index,&doc,caret(&doc,"posts.show")).is_empty(),"{source}");}
        h.add_file("app/Foreign.php","<?php use function Vendor\\view; view('posts.show');");
        h.add_file("app/ForeignFacade.php","<?php use App\\View; View::make('posts.show');");
        h.add_file("app/Explicit.php","<?php use function Vendor\\view; \\view('posts.show');");
        h.add_file("app/ExplicitFacade.php","<?php use App\\View; \\Illuminate\\Support\\Facades\\View::make('posts.show');");
        let doc=controller(&h);let result=run(&h,&index,&doc,"posts.show","posts.detail").unwrap();
        assert!(!result.edits.iter().any(|(path,_)|path.ends_with("Foreign.php")||path.ends_with("ForeignFacade.php")));
        assert!(result.edits.iter().any(|(path,_)|path.ends_with("Explicit.php")));
        assert!(result.edits.iter().any(|(path,_)|path.ends_with("ExplicitFacade.php")));
        assert_eq!(p.read_file("app/Foreign.php").unwrap(),"<?php use function Vendor\\view; view('posts.show');");
    }
    #[test]fn cancellation_and_changed_invoking_buffer_are_refused(){
        let(h,p,index)=setup();let doc=controller(&h);let action=actions(&index,&doc,caret(&doc,"posts.show")).remove(0);
        h.answer_next_prompt(None);assert!(resolve(&index,&doc,action).is_err());
        let action=actions(&index,&doc,caret(&doc,"posts.show")).remove(0);h.add_file("app/Http/Controllers/PostController.php","<?php changed");assert!(resolve(&index,&doc,action).is_err());
        assert!(p.exists("resources/views/posts/show.blade.php").unwrap());
    }
    #[test]fn custom_blade_directives_do_not_become_framework_rename_references(){
        let(h,_p,mut index)=setup();
        index.directives.insert("include".into(),index.views["posts.show"].clone());
        let source="@include('posts.show')";
        h.add_file("resources/views/caller.blade.php",source);
        let doc=Document{path:"/project/resources/views/caller.blade.php".into(),language:"blade".into(),version:1,text:source.into()};
        assert!(actions(&index,&doc,caret(&doc,"posts.show")).is_empty());
        let r=refs::reference_at("blade",source,source.find("posts.show").unwrap()).unwrap();
        assert!(!binding_safe(&index,&doc,&r));
        index.directives.clear();assert!(binding_safe(&index,&doc,&r));
    }

}
