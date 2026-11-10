//! Proven framework literal ranges for a reviewed, atomic Rename FileAction.
//! These ranges are never published as host anchors: the pinned host inserts
//! unvalidated new names directly into source and has no rename veto hook.
use std::collections::{BTreeMap,BTreeSet};
use std::ops::Range;
use crate::{Index,index::{Entry,Names},php,refs::{self,Kind}};
use openidea_plugin::{Project,Range as TextRange,text,indexing::{Anchor,Document,Edges}};
const MAX_SOURCE:usize=512*1024;
const MAX_FILES:usize=32;
const MAX_TOTAL:usize=2*1024*1024;
const MAX_ANCHORS:usize=4096;
#[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord)]
enum Category {Route,Config,Translation,Component}
impl Category {fn name(self)->&'static str {match self {Self::Route=>"route",Self::Config=>"config",Self::Translation=>"translation",Self::Component=>"component"}}}
#[derive(Clone,Debug)]
struct Token {category:Category,id:String,spelling:String,path:String,range:Range<usize>}
fn valid(name:&str)->bool {!name.is_empty()&&name.len()<=1024&&!name.chars().any(char::is_control)&&!name.contains(['\\','$','*','\'','"'])}
fn quoted(source:&str)->Option<BTreeMap<usize,usize>> {
    if source.len()>MAX_SOURCE||source.contains("<<<"){return None;}
    let masked=php::mask_comments(source);let bytes=masked.as_bytes();let mut at=0;let mut out=BTreeMap::new();
    while at<bytes.len(){if matches!(bytes[at],b'\''|b'"'|b'`'){
        let end=php::string_end(bytes,at);if end>bytes.len()||bytes.get(end.saturating_sub(1))!=Some(&bytes[at]){return None;}
        if bytes[at]!=b'`'&&end>=at+2{out.insert(at+1,end-1);}at=end;
    }else{at+=1;}}
    Some(out)
}
fn token(category:Category,id:&str,path:&str,source:&str,start:usize,end:usize,quotes:&BTreeMap<usize,usize>)->Option<Token>{
    let spelling=source.get(start..end)?;
    if !valid(id)||!valid(spelling)||quotes.get(&start)!=Some(&end){return None;}
    if id!=spelling&&!id.strip_suffix(spelling).is_some_and(|head|head.ends_with('.')){return None;}
    Some(Token{category,id:id.into(),spelling:spelling.into(),path:path.into(),range:start..end})
}
fn agrees(names:&Names,id:&str,path:&str,source:&str,start:usize)->bool {
    names.get(id).is_some_and(|entry|entry.path==path&&entry.at==text::position(source,start))
}
fn definitions(index:&Index,root:&str,path:&str,source:&str)->Vec<Token>{
    let Some(quotes)=quoted(source)else{return vec![];};let mut out=Vec::new();
    let relative=path.strip_prefix(root).and_then(|s|s.strip_prefix('/')).unwrap_or("");
    if let Some(file)=relative.strip_prefix("config/").and_then(|s|s.strip_suffix(".php")){
        let prefix=file.replace('/',".");let masked=php::mask_comments(source);
        let (leaves,complete)=crate::config_arrays::paths(&masked,&prefix);
        if complete {let mut counts=BTreeMap::new();for leaf in &leaves{*counts.entry(leaf.path.as_str()).or_insert(0usize)+=1;}let duplicate:BTreeSet<_>=counts.into_iter().filter(|(_,count)|*count>1).map(|(name,_)|name.to_owned()).collect();for leaf in leaves {if !duplicate.contains(&leaf.path)&&agrees(&index.config,&leaf.path,path,source,leaf.offset){if let Some(t)=token(Category::Config,&leaf.path,path,source,leaf.offset,leaf.offset+leaf.width,&quotes){out.push(t);}}}}
    }
    let translation_prefix=index.translation_files.iter().find(|(_,paths)|paths.iter().any(|p|p==path)).map(|(key,_)|key);
    if let Some(prefix)=translation_prefix {
        let masked=php::mask_comments(source);
        if let Some(open)=php::returned_array(&masked){let mut leaves=Vec::new();php::array_paths(&masked,open,prefix,&mut leaves,0);
            let mut counts=BTreeMap::new();for leaf in &leaves{*counts.entry(leaf.path.as_str()).or_insert(0usize)+=1;}let duplicate:BTreeSet<_>=counts.into_iter().filter(|(_,count)|*count>1).map(|(name,_)|name.to_owned()).collect();
            for leaf in leaves {if !duplicate.contains(&leaf.path)&&index.translations_by_locale.values().any(|names|agrees(names,&leaf.path,path,source,leaf.offset)){
                if let Some(t)=token(Category::Translation,&leaf.path,path,source,leaf.offset,leaf.offset+leaf.width,&quotes){out.push(t);}
            }}
        }
    }else if path.ends_with(".json")&&index.translations_by_locale.values().any(|names|names.values().any(|e|e.path==path)) {
        if let Ok(value)=crate::strict_json::parse(source,MAX_SOURCE){if value.as_object().is_some_and(|v|v.values().all(|value|value.is_string())){
            for (&start,&end) in &quotes {let Some(name)=source.get(start..end)else{continue;};
                if source[end+1..].trim_start().starts_with(':')&&value.get(name).is_some()&&index.translations_by_locale.values().any(|names|agrees(names,name,path,source,start)){
                    if let Some(t)=token(Category::Translation,name,path,source,start,end,&quotes){out.push(t);}
                }
            }
        }}
    }
    if index.route_defs.iter().any(|r|r.path==path){for route in crate::routes::parse(&crate::view_rename::route_source(source).unwrap_or_default()){
        let Some(name)=route.name else{continue;};
        if index.route_defs.iter().filter(|r|r.def.name.as_deref()==Some(name.as_str())).count()!=1{continue;}
        if agrees(&index.routes,&name,path,source,route.offset){if let Some(t)=token(Category::Route,&name,path,source,route.offset,route.offset+route.width,&quotes){out.push(t);}}
    }}
    if crate::view_rename::facade(source,"Blade","Blade") {if let Some(structure)=crate::package_facts::code(source){for (at,_) in structure.match_indices("Blade::component"){
        if at>0&&matches!(structure.as_bytes()[at-1],b'\\'|b'$'|b'_'|b'a'..=b'z'|b'A'..=b'Z'){continue;}
        let tail=structure[at+"Blade::component".len()..].trim_start();if !tail.starts_with('('){continue;}
        let open=structure.len()-tail.len();let args=php::arguments(source,open);if args.len()!=2{continue;}
        if php::class_constant(source,args[0].clone()).is_none(){continue;}
        let Some((alias,start))=php::literal(source,args[1].clone())else{continue;};
        if index.components.get(alias).is_some_and(|entry|entry.detail.as_deref()==Some("Blade::component")){if let Some(t)=token(Category::Component,alias,path,source,start,start+alias.len(),&quotes){out.push(t);}}
    }}}
    out
}
fn category(kind:Kind)->Option<Category>{match kind{Kind::Route=>Some(Category::Route),Kind::Config=>Some(Category::Config),Kind::Translation=>Some(Category::Translation),Kind::Component=>Some(Category::Component),_=>None}}
fn prefixes(name:&str)->Vec<&str>{name.match_indices('.').map(|(at,_)|&name[..at]).chain(std::iter::once(name)).collect()}
fn entries<'a>(index:&'a Index,category:Category,id:&str)->Vec<&'a Entry>{match category{
    Category::Route=>index.routes.get(id).into_iter().collect(),Category::Config=>index.config.get(id).into_iter().collect(),
    Category::Translation=>index.translations_by_locale.values().filter_map(|names|names.get(id)).collect(),Category::Component=>index.components.get(id).into_iter().collect(),
}}
fn key(root:&str,category:Category,id:&str)->String{format!("laravel:{root}:{}#{id}",category.name())}
fn range(source:&str,range:Range<usize>)->TextRange{TextRange::new(text::position(source,range.start),text::position(source,range.end))}

/// Reads only bounded, currently-owned definition files. No source evaluation,
/// unscoped text search, file-name definitions or transformed rename anchors.
pub(crate) fn edges(index:&Index,project:&Project,doc:&Document)->Edges{
    let run=||->Option<Edges>{
        if index.partial||doc.text.len()>MAX_SOURCE||!crate::owns_current_path(project,&doc.path){return None;}
        let root=project.root().ok()?;
        if root.len()>2048{return None;}
        let mut sources=BTreeMap::from([(doc.path.clone(),doc.text.clone())]);
        let local=definitions(index,&root,&doc.path,&doc.text);
        let mut wanted:BTreeSet<(Category,String)>=local.iter().map(|t|(t.category,t.id.clone())).collect();
        let language=crate::language_of(&doc.path,"");
        let mut references=language.map(|lang|refs::references(lang,&doc.text)).unwrap_or_default();
        if language==Some("blade") {
            references.extend(refs::tag_references(&doc.text));
            // Preserve byte positions while presenting closing tags to the same
            // opaque-region/comment-aware component scanner.
            let mut closing=doc.text.as_bytes().to_vec();
            for (at,_) in doc.text.match_indices("</x-"){closing[at]=b'/';closing[at+1]=b'<';}
            if let Ok(closing)=String::from_utf8(closing){references.extend(refs::tag_references(&closing));}
            references.sort_by_key(|r|(r.start,r.end));references.dedup_by(|a,b|a.kind==b.kind&&a.start==b.start&&a.end==b.end);
        }
        if !crate::view_rename::binding_budget(doc.text.len(),references.len()){return None;}
        let binding_doc=openidea_plugin::providers::Document{path:doc.path.clone(),text:doc.text.clone(),language:language.unwrap_or("php").into(),version:0};
        for reference in &references{if !reference.closed||!crate::view_rename::binding_safe(index,&binding_doc,reference){continue;}let Some(category)=category(reference.kind)else{continue;};
            let Some(name)=doc.text.get(reference.start..reference.end).filter(|name|valid(name))else{continue;};
            for id in if matches!(category,Category::Config|Category::Translation){prefixes(name)}else{vec![name]}{wanted.insert((category,id.into()));}
        }
        let mut paths=BTreeSet::new();for (category,id) in &wanted{for entry in entries(index,*category,id){if !entry.path.is_empty(){paths.insert(entry.path.clone());}}}
        if wanted.iter().any(|(category,_)|*category==Category::Component){
            let providers=project.find_files("app/Providers/**/*.php",(MAX_FILES+1) as u32).ok()?;
            if providers.len()>MAX_FILES{return None;}paths.extend(providers);
        }
        if paths.len()>MAX_FILES{return None;}let mut total=doc.text.len();
        for path in paths{if sources.contains_key(&path){continue;}if !crate::owns_current_path(project,&path){return None;}
            let source=project.read_file(&path).ok()?;total=total.checked_add(source.len())?;
            if source.len()>MAX_SOURCE||total>MAX_TOTAL{return None;}sources.insert(path,source);
        }
        let mut tokens=Vec::new();for (path,source) in &sources{if !crate::owns_current_path(project,path){return None;}tokens.extend(definitions(index,&root,path,source));}
        if tokens.len()>MAX_ANCHORS{return None;}
        // Multiple locales may define the same exact spelling; contradictory
        // spellings or duplicate definitions in one source cannot be renamed.
        let mut counts=BTreeMap::new();let mut spellings:BTreeMap<(Category,&str),BTreeSet<&str>>=BTreeMap::new();
        for token in &tokens{*counts.entry((token.category,token.id.as_str(),token.path.as_str())).or_insert(0usize)+=1;spellings.entry((token.category,token.id.as_str())).or_default().insert(&token.spelling);}
        let allowed=|category:Category,id:&str|spellings.get(&(category,id)).is_some_and(|s|s.len()==1)&&!counts.iter().any(|((c,name,_),count)|*c==category&&*name==id&&*count>1)&&!(category==Category::Component&&tokens.iter().filter(|t|t.category==category&&t.id==id).count()>1);
        let mut result=Edges::default();
        for token in tokens.iter().filter(|t|t.path==doc.path&&allowed(t.category,&t.id)){
            result.definitions.push(Anchor{key:key(&root,token.category,&token.id),range:range(&doc.text,token.range.clone())});
        }
        for reference in references{if !reference.closed||!crate::view_rename::binding_safe(index,&binding_doc,&reference){continue;}let Some(category)=category(reference.kind)else{continue;};
            let Some(name)=doc.text.get(reference.start..reference.end).filter(|name|valid(name))else{continue;};
            // The whole canonical name must be proven before parent segments
            // participate, avoiding nested/literal-dot key ambiguity.
            if !allowed(category,name){continue;}
            let ids=if matches!(category,Category::Config|Category::Translation){prefixes(name)}else{vec![name]};
            for id in ids {if !allowed(category,id){continue;}let spelling=*spellings.get(&(category,id))?.first()?;
                if id!=spelling&&!id.strip_suffix(spelling).is_some_and(|head|head.ends_with('.')){continue;}
                let start=reference.start+id.len().checked_sub(spelling.len())?;let end=start+spelling.len();
                if doc.text.get(start..end)!=Some(spelling){continue;}
                result.references.push(Anchor{key:key(&root,category,id),range:range(&doc.text,start..end)});
            }
        }
        if result.references.len()+result.definitions.len()>MAX_ANCHORS{return None;}
        Some(result)
    };
    run().unwrap_or_default()
}

#[derive(serde::Serialize,serde::Deserialize)]
struct RenameRequest {root:String,path:String,version:u64,hash:String,key:String,range:TextRange,name:String}
fn hash(source:&str)->String{crate::artisan_metadata::hash(source)}
fn provider_doc(doc:&openidea_plugin::providers::Document)->Document{Document{path:doc.path.clone(),text:doc.text.clone(),symbols:vec![]}}
fn picked(index:&Index,project:&Project,doc:&openidea_plugin::providers::Document,selection:TextRange)->Option<(String,TextRange,String)>{
    let edges=edges(index,project,&provider_doc(doc));
    let mut found=edges.definitions.into_iter().chain(edges.references).filter(|a|a.range.start<=selection.start&&selection.end<=a.range.end);
    let anchor=found.next()?;if found.next().is_some(){return None;}
    let start=text::offset(&doc.text,anchor.range.start)?;let end=text::offset(&doc.text,anchor.range.end)?;
    Some((anchor.key,anchor.range,doc.text.get(start..end)?.into()))
}
pub(crate) fn actions(index:&Index,doc:&openidea_plugin::providers::Document,selection:TextRange)->Vec<openidea_plugin::file_actions::FileAction>{
    use openidea_plugin::file_actions::FileAction;
    let run=||->Option<FileAction>{let project=crate::project_for(&doc.path)?;let root=project.root().ok()?;
        let (key,range,name)=picked(index,&project,doc,selection)?;
        let request=RenameRequest{root,path:doc.path.clone(),version:doc.version,hash:hash(&doc.text),key,range,name:name.clone()};
        let data=format!("semantic-rename:{}",serde_json::to_string(&request).ok()?);
        Some(FileAction{title:format!("Rename proven Laravel name segment {name}…"),kind:Some("refactor.rename".into()),data:Some(data),..Default::default()})
    };run().into_iter().collect()
}
#[derive(Debug,PartialEq,Eq)]
struct Change {path:String,source:String,ranges:Vec<TextRange>}
fn collect(index:&Index,project:&Project,doc:&openidea_plugin::providers::Document,key:&str)->Result<Vec<Change>,openidea_plugin::Error>{
    use openidea_plugin::Error;
    let root=project.root()?;let found=project.find_files("**/*.php",513)?;
    if found.len()>=513{return Err(Error::Invalid("Framework rename source enumeration is incomplete at its 512-file bound.".into()));}
    let mut paths:BTreeSet<String>=found.into_iter().filter(|path|{
        let relative=path.strip_prefix(&root).and_then(|p|p.strip_prefix('/')).unwrap_or(path);
        !["vendor/","node_modules/","storage/","bootstrap/cache/",".git/"].iter().any(|p|relative.starts_with(p))
    }).collect();
    for entry in index.translations_by_locale.values().flat_map(|names|names.values()){paths.insert(entry.path.clone());}
    paths.insert(doc.path.clone());if paths.len()>512{return Err(Error::Invalid("Framework rename exceeds the 512-source review bound; narrow the project before retrying.".into()));}
    let mut bytes=0usize;let mut changes=Vec::new();
    for path in paths{if !crate::owns_current_path(project,&path){continue;}
        let source=if path==doc.path{doc.text.clone()}else{project.read_file(&path)?};bytes=bytes.checked_add(source.len()).ok_or(Error::Stale)?;
        if source.len()>MAX_SOURCE||bytes>4*1024*1024{return Err(Error::Invalid("Framework rename source scan exceeds its review bound.".into()));}
        let document=Document{path:path.clone(),text:source.clone(),symbols:vec![]};
        let count=crate::language_of(&path,"").map(|lang|refs::references(lang,&source).len()+if lang=="blade"{2*refs::tag_references(&source).len()}else{0}).unwrap_or(0);
        if !crate::view_rename::binding_budget(source.len(),count){return Err(Error::Invalid("Framework rename binding analysis exceeds its per-source 128-reference / 8 MiB work bound; no partial rename is offered.".into()));}
        let facts=edges(index,project,&document);
        let mut ranges:Vec<_>=facts.definitions.into_iter().chain(facts.references).filter(|a|a.key==key).map(|a|a.range).collect();
        ranges.sort_by_key(|r|(r.start.line,r.start.character,r.end.line,r.end.character));ranges.dedup();
        if !ranges.is_empty(){changes.push(Change{path,source,ranges});}
    }
    if changes.is_empty()||!key.starts_with(&format!("laravel:{root}:")){return Err(Error::Stale);}
    Ok(changes)
}
fn safe_new_name(name:&str)->bool{
    !name.is_empty()&&name.len()<=128&&name.chars().all(|c|c.is_alphanumeric()||matches!(c,'_'|'-'))
}
pub(crate) fn resolve(index:&Index,doc:&openidea_plugin::providers::Document,action:openidea_plugin::file_actions::FileAction)->Result<openidea_plugin::file_actions::FileAction,openidea_plugin::Error>{
    use openidea_plugin::{Error,TextEdit,host,file_actions::FileAction};
    let data=action.data.as_deref().and_then(|s|s.strip_prefix("semantic-rename:")).filter(|s|s.len()<=8192).ok_or(Error::Stale)?;
    let request:RenameRequest=serde_json::from_str(data).map_err(|_|Error::Stale)?;
    if doc.path!=request.path||doc.version!=request.version||hash(&doc.text)!=request.hash{return Err(Error::Stale);}
    let project=crate::project_for(&doc.path).ok_or(Error::Stale)?;
    if project.root()?!=request.root||project.read_file(&doc.path)?!=doc.text{return Err(Error::Stale);}
    let picked=picked(index,&project,doc,request.range).ok_or(Error::Stale)?;
    if picked!=(request.key.clone(),request.range,request.name.clone()){return Err(Error::Stale);}
    let new=host::input("New Laravel name segment (letters, digits, underscore or hyphen)",Some(&request.name))?.ok_or(Error::Cancelled)?;
    if !safe_new_name(&new){return Err(Error::Invalid("Use a nonempty name segment containing only letters, digits, underscore or hyphen; dots, quotes, slashes and escapes are not accepted.".into()));}
    if new==request.name{return Err(Error::Cancelled);}
    // Rebuild before the scan so source identities and ambiguous definitions are
    // not inherited from an earlier editor cache.
    let current=Index::build(&project);if current.partial{return Err(Error::Stale);}
    let before=collect(&current,&project,doc,&request.key)?;
    for change in &before {for range in &change.ranges {let start=text::offset(&change.source,range.start).ok_or(Error::Stale)?;let end=text::offset(&change.source,range.end).ok_or(Error::Stale)?;
        if change.source.get(start..end)!=Some(request.name.as_str()){return Err(Error::Stale);}
    }}
    let replacement_key=request.key.strip_suffix(&request.name).map(|prefix|format!("{prefix}{new}")).ok_or(Error::Stale)?;
    if collect(&current,&project,doc,&replacement_key).is_ok(){return Err(Error::Invalid("The new name already resolves to a proven definition or reference.".into()));}
    let mut preview=format!("Project: {}\nRename only proven framework name segments {} → {}.\nFilename-derived views/components, dynamic names, JavaScript and unrelated text are excluded.\n\n",request.root,request.name,new);
    let mut edits=Vec::new();
    for change in &before{let mut after=change.source.clone();for range in change.ranges.iter().rev(){let start=text::offset(&change.source,range.start).ok_or(Error::Stale)?;let end=text::offset(&change.source,range.end).ok_or(Error::Stale)?;after.replace_range(start..end,&new);}
        preview.push_str(&format!("{}\nComplete resulting contents:\n{}\n\n",change.path,ui_kit::prompts::readable_source(&after)));
        edits.push((change.path.clone(),change.ranges.iter().map(|r|TextEdit{range:*r,text:new.clone()}).collect()));
    }
    if !ui_kit::prompts::confirm_exact("Rename proven Laravel framework segments?",&preview)?{return Err(Error::Cancelled);}
    let latest=Index::build(&project);if latest.partial||project.read_file(&doc.path)?!=doc.text||collect(&latest,&project,doc,&request.key)?!=before{return Err(Error::Stale);}
    Ok(FileAction{title:format!("Rename Laravel segment {} → {new}",request.name),edits,..Default::default()})
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{mock::MockHost,providers::Document as SourceDocument};
    fn setup()->(MockHost,Index,Project){
        let h=MockHost::install().with_capabilities(&["project.read"]);
        h.add_file("composer.json",r#"{"require":{"laravel/framework":"^12.0"}}"#);
        h.add_file("artisan","<?php");
        h.add_file("config/app.php","<?php return ['ui' => ['title' => 'Same value'], 'other' => 1];");
        h.add_file("routes/web.php","<?php Route::get('/', fn()=>1)->name('home');");
        h.add_file("lang/en/messages.php","<?php return ['welcome' => 'Welcome'];");
        h.add_file("lang/fr/messages.php","<?php return ['welcome' => 'Bienvenue'];");
        let p=h.invocation().project().unwrap();let index=Index::build(&p);assert!(!index.partial);(h,index,p)
    }
    fn doc(source:&str)->SourceDocument{SourceDocument{path:"/project/app/Use.php".into(),language:"php".into(),version:1,text:source.into()}}
    fn caret(source:&str,name:&str)->TextRange{let at=text::position(source,source.find(name).unwrap()+1);TextRange::new(at,at)}
    fn spans(source:&str,anchors:&[Anchor])->Vec<String>{anchors.iter().map(|a|{let a=text::offset(source,a.range.start).unwrap()..text::offset(source,a.range.end).unwrap();source[a].to_owned()}).collect()}
    #[test] fn config_segments_preserve_file_and_parent_spelling() {
        let (_h,index,p)=setup();let source="<?php /* 😀 */ config('app.ui.title'); $unrelated = 'app.ui.title';";
        let d=doc(source);let found=edges(&index,&p,&provider_doc(&d));
        assert_eq!(spans(source,&found.references),["ui","title"]);
        assert!(found.references.iter().all(|a|a.key.starts_with("laravel:/project:config#app.ui")));
        let source=p.read_file("config/app.php").unwrap();let d=Document{path:"/project/config/app.php".into(),text:source.clone(),symbols:vec![]};
        let edges=edges(&index,&p,&d);assert_eq!(spans(&source,&edges.definitions),["ui","title","other"]);
    }
    #[test] fn duplicate_and_literal_dot_ambiguity_withhold_rename() {
        let (h,_,p)=setup();h.add_file("config/app.php","<?php return ['ui'=>['title'=>'A'], 'ui.title'=>'B'];");
        let index=Index::build(&p);let d=doc("<?php config('app.ui.title');");assert!(edges(&index,&p,&provider_doc(&d)).references.is_empty());
        h.add_file("config/app.php","<?php return ['title'=>'A','title'=>'B'];");let index=Index::build(&p);
        let d=doc("<?php config('app.title');");assert!(edges(&index,&p,&provider_doc(&d)).references.is_empty());
    }
    #[test] fn full_group_route_names_rename_only_the_defined_suffix() {
        let (h,_,p)=setup();h.add_file("routes/web.php","<?php Route::name('admin.')->group(function(){ Route::get('/', fn()=>1)->name('home'); });");
        let index=Index::build(&p);let source="<?php route('admin.home');";let d=doc(source);
        assert_eq!(spans(source,&edges(&index,&p,&provider_doc(&d)).references),["home"]);
        h.add_file("routes/extra.php","<?php Route::get('/another',fn()=>1)->name('admin.home');");let index=Index::build(&p);
        assert!(edges(&index,&p,&provider_doc(&d)).references.is_empty());
    }
    #[test] fn translation_locale_segments_are_consistent_and_json_conflict_is_withheld() {
        let (h,index,p)=setup();let source="<?php __('messages.welcome');";let d=doc(source);
        assert_eq!(spans(source,&edges(&index,&p,&provider_doc(&d)).references),["welcome"]);
        let changes=collect(&index,&p,&d,&key("/project",Category::Translation,"messages.welcome")).unwrap();
        assert_eq!(changes.len(),3);
        h.add_file("lang/en.json",r#"{"messages.welcome":"JSON overrides PHP"}"#);let index=Index::build(&p);
        assert!(edges(&index,&p,&provider_doc(&d)).references.is_empty());
    }
    #[test] fn filename_names_and_foreign_scopes_do_not_gain_host_rename_anchors() {
        let (h,index,p)=setup();let d=doc("<?php view('welcome');");assert!(edges(&index,&p,&provider_doc(&d)).references.is_empty());
        let d=doc("<?php config('app.ui.title');");h.add_project("/project/config");assert!(edges(&index,&p,&provider_doc(&d)).references.is_empty());
        let source="<?php $x = \"config('app.ui.title')\"; // config('app.ui.title')";assert!(edges(&index,&p,&provider_doc(&doc(source))).references.is_empty());
    }
    #[test] fn reviewed_rename_is_atomic_scoped_and_rejects_unsafe_new_names() {
        let (h,index,_p)=setup();let source="<?php /* 😀 */ config('app.ui.title'); $unrelated='app.ui.title';";h.add_file("app/Use.php",source);let d=doc(source);
        let action=actions(&index,&d,caret(source,"title")).remove(0);
        h.answer_next_prompt(Some(&["heading"]));h.answer_next_prompt(Some(&["yes"]));
        let result=resolve(&index,&d,action).unwrap();assert_eq!(result.edits.len(),2);
        for (path,edits) in result.edits{assert_eq!(edits.len(),1);let body=h.invocation().project().unwrap().read_file(&path).unwrap();let r=edits[0].range;assert_eq!(&body[text::offset(&body,r.start).unwrap()..text::offset(&body,r.end).unwrap()],"title");assert_eq!(edits[0].text,"heading");}
        for invalid in ["new.name","quote'","escape\\","space name","","\"","../file"] {assert!(!safe_new_name(invalid));}
        assert!(safe_new_name("café_title-2"));
    }
    #[test] fn explicit_component_aliases_have_same_spelling_and_duplicates_are_withheld() {
        let (h,_,p)=setup();
        h.add_file("app/Providers/AppServiceProvider.php","<?php use Illuminate\\Support\\Facades\\Blade; Blade::component(Notice::class, 'notice');");
        let index=Index::build(&p);let d=SourceDocument{path:"/project/resources/views/app.blade.php".into(),language:"blade".into(),version:1,text:"<x-notice />".into()};
        let found=edges(&index,&p,&provider_doc(&d));assert_eq!(spans(&d.text,&found.references),["notice"]);
        h.add_file("app/Providers/OtherProvider.php","<?php use Illuminate\\Support\\Facades\\Blade; Blade::component(Other::class, 'notice');");
        let index=Index::build(&p);assert!(edges(&index,&p,&provider_doc(&d)).references.is_empty());
    }
    #[test] fn json_translation_review_and_existing_new_name_collision() {
        let (h,_,p)=setup();h.add_file("lang/en.json",r#"{"Hello":"Hello","Title":"Title"}"#);
        let index=Index::build(&p);let source="<?php __('Hello');";h.add_file("app/Use.php",source);let d=doc(source);
        let action=actions(&index,&d,caret(source,"Hello")).remove(0);
        h.answer_next_prompt(Some(&["Title"]));assert!(resolve(&index,&d,action).is_err());
        let action=actions(&index,&d,caret(source,"Hello")).remove(0);
        h.answer_next_prompt(Some(&["Greeting"]));h.answer_next_prompt(Some(&["yes"]));
        let result=resolve(&index,&d,action).unwrap();assert_eq!(result.edits.len(),2);
        let json=result.edits.iter().find(|(path,_)|path.ends_with("en.json")).unwrap();assert_eq!(json.1.len(),1);assert_eq!(json.1[0].text,"Greeting");
        assert_eq!(p.read_file("lang/en.json").unwrap(),r#"{"Hello":"Hello","Title":"Title"}"#);
    }
    #[test] fn foreign_route_blade_definitions_and_scalar_helper_bindings_are_withheld() {
        let(h,_,p)=setup();
        for source in ["<?php use App\\Route; Route::get('/',fn()=>1)->name('home');","<?php class Route {} Route::get('/',fn()=>1)->name('home');"] {
            h.add_file("routes/web.php",source);let index=Index::build(&p);let d=doc("<?php route('home');");assert!(edges(&index,&p,&provider_doc(&d)).references.is_empty(),"{source}");
        }
        h.add_file("routes/web.php","<?php Route::get('/',fn()=>1)->name('home');");let index=Index::build(&p);
        for source in ["<?php use function Vendor\\config; config('app.ui.title');","<?php function config($name){} config('app.ui.title');","<?php use App\\Config; Config::get('app.ui.title');"] {
            assert!(edges(&index,&p,&provider_doc(&doc(source))).references.is_empty(),"{source}");
        }
        for source in ["<?php use function Vendor\\config; \\config('app.ui.title');","<?php use App\\Config; \\Illuminate\\Support\\Facades\\Config::get('app.ui.title');"] {
            assert_eq!(edges(&index,&p,&provider_doc(&doc(source))).references.len(),2,"{source}");
        }
        for source in ["<?php use App\\Blade; Blade::component(Notice::class,'notice');","<?php class Blade {} Blade::component(Notice::class,'notice');"] {
            h.add_file("app/Providers/AppServiceProvider.php",source);let index=Index::build(&p);
            let d=SourceDocument{path:"/project/resources/views/test.blade.php".into(),language:"blade".into(),version:1,text:"<x-notice/>".into()};assert!(edges(&index,&p,&provider_doc(&d)).references.is_empty(),"{source}");
        }
    }
    #[test] fn cancellation_collision_and_stale_document_return_no_edits() {
        let (h,index,_p)=setup();let source="<?php config('app.ui.title');";h.add_file("app/Use.php",source);let mut d=doc(source);
        let a=actions(&index,&d,caret(source,"title")).remove(0);h.answer_next_prompt(None);assert!(resolve(&index,&d,a).is_err());
        let a=actions(&index,&d,caret(source,"title")).remove(0);h.answer_next_prompt(Some(&["bad'quote"]));assert!(resolve(&index,&d,a).is_err());
        let a=actions(&index,&d,caret(source,"title")).remove(0);d.version+=1;assert!(resolve(&index,&d,a).is_err());
        assert_eq!(h.invocation().project().unwrap().read_file("app/Use.php").unwrap(),source);
    }
}
