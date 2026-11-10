//! Bounded package facts. Callers must resolve files through current project ownership.
//! No runtime execution, guessed Inertia page calls, or unrelated Passport classes.
use std::{collections::BTreeMap, ops::Range};
use crate::php;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Scope {
    pub name: String,
    pub description: String,
    pub range: Option<Range<usize>>,
}

/// Comments/strings cannot manufacture executable call sites. Heredoc and
/// template-heavy input is withheld rather than approximated as ordinary PHP.
pub(crate) fn code(text: &str) -> Option<String> {
    if text.len() > 512 * 1024 || text.contains("<<<") { return None; }
    let masked = php::mask_comments(text);
    let mut bytes = masked.into_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if matches!(bytes[at], b'\'' | b'"' | b'`') {
            let end = php::string_end(text.as_bytes(), at).min(bytes.len());
            for byte in &mut bytes[at..end] { if !matches!(*byte,b'\r'|b'\n') { *byte=b' '; } }
            at=end;
        } else { at+=1; }
    }
    String::from_utf8(bytes).ok()
}
fn word(byte: u8) -> bool { byte.is_ascii_alphanumeric() || matches!(byte,b'_'|b'\\'|b'$') || byte>=128 }
fn receivers(masked: &str) -> Vec<String> {
    let mut out=vec!["\\Laravel\\Passport\\Passport".into()];
    let mut imports: BTreeMap<String,Vec<String>>=BTreeMap::new();
    // Only one ordinary namespace: imports in different PHP namespaces must not mix.
    let namespaces=masked.match_indices("namespace ").count();
    if namespaces>1 {return out;}
    for (at,_) in masked.match_indices("use ") {
        if at>0 && word(masked.as_bytes()[at-1]) {continue;}
        let depth=masked[..at].bytes().fold(0i32,|depth,b|match b{b'{'=>depth+1,b'}'=>depth-1,_=>depth});
        if depth!=0 {continue;}
        let Some(end)=masked[at+4..].find(';').map(|n|at+4+n) else {continue;};
        let item=masked[at+4..end].trim();
        if item.contains(['{',',']) {continue;}
        let mut parts=item.split_whitespace();
        let Some(fqn)=parts.next().map(|s|s.trim_start_matches('\\')) else {continue;};
        let alias=match (parts.next(),parts.next(),parts.next()) {
            (None,None,None)=>fqn.rsplit('\\').next().unwrap_or(fqn),
            (Some("as"),Some(alias),None)=>alias,
            _=>continue,
        };
        imports.entry(alias.into()).or_default().push(fqn.into());
    }
    for (alias,classes) in imports {
        if classes==["Laravel\\Passport\\Passport"] && alias.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_') {
            if !masked.contains(&format!("class {alias}")) {out.push(alias);}
        }
    }
    out
}

/// Literal Passport::tokensCan(['scope' => 'description']) registrations. Source
/// takes precedence over metadata fallback. Aliased exact imports are supported.
pub(crate) fn passport_scopes(text: &str) -> Vec<Scope> {
    let Some(masked)=code(text) else {return vec![];};
    let mut scopes=BTreeMap::new();
    for receiver in receivers(&masked) {
        for (at,_) in masked.match_indices(&receiver) {
            if at>0 && word(masked.as_bytes()[at-1]) {continue;}
            let tail=&masked[at+receiver.len()..];
            let Some(tail)=tail.trim_start().strip_prefix("::") else {continue;};
            let Some(tail)=tail.trim_start().strip_prefix("tokensCan") else {continue;};
            let tail=tail.trim_start();
            if !tail.starts_with('(') {continue;}
            let open=masked.len()-tail.len();
            let args=php::arguments(text,open);
            if args.len()!=1 {continue;}
            let Some(array)=php::array_open(text,args[0].clone()) else {continue;};
            for item in php::items(text,array) {
                let Some((name,start))=item.key.and_then(|r|php::literal(text,r)) else {continue;};
                let Some((description,_))=php::literal(text,item.value) else {continue;};
                if !literal(name) || !literal(description) || name.is_empty() {continue;}
                scopes.entry(name.to_owned()).or_insert(Scope {name:name.into(),description:description.into(),range:Some(start..start+name.len())});
            }
        }
    }
    scopes.into_values().collect()
}
/// Source-owned navigation inventory for the index adapter.
pub(crate) fn passport_names(path: &str, text: &str) -> crate::index::Names {
    passport_scopes(text).into_iter().filter_map(|scope| {
        let range=scope.range?;
        let at=openidea_plugin::text::position(text,range.start);
        Some((scope.name,crate::index::Entry{detail:Some(scope.description),path:path.into(),at,width:text[range].encode_utf16().count() as u32}))
    }).collect()
}
fn literal(text:&str)->bool { !text.contains(['\\','$','\0','\r','\n']) }

/// Load already-owned ide.json scope data, or one declared scopeFiles object.
/// JSON paths are not read here; callers apply project ownership and size limits.
pub(crate) fn passport_metadata(value:&serde_json::Value, external: bool)->Vec<Scope> {
    let object=if external {Some(value)} else {value.pointer("/packages/passport/scopes")};
    object.and_then(|v|v.as_object()).into_iter().flatten().filter_map(|(name,value)| {
        let description=value.as_str()?;
        (!name.is_empty() && literal(name) && literal(description)).then(||Scope{name:name.clone(),description:description.into(),range:None})
    }).collect()
}

/// Literal middleware `scope:read,write` / `scopes:read,write` suffix under the
/// cursor. The caller must establish a middleware call context first.
pub(crate) fn passport_scope_part(value:&str, base:usize, at:usize)->Option<Range<usize>> {
    let colon=value.find(':')?;
    if !matches!(&value[..colon],"scope"|"scopes") {return None;}
    let mut start=colon+1;
    for part in value[start..].split(',') {
        let end=start+part.len();
        if base+start<=at && at<=base+end && part.chars().all(|c|!c.is_whitespace()&&!c.is_control()&&!matches!(c,'\\'|'$'|'\''|'"')) { return Some(base+start..base+end); }
        start=end+1;
    }
    None
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WayfinderImport {
    pub module: String,
    /// Named/default import as spelled in this file.
    pub local: String,
    pub exported: String,
    pub range: Range<usize>,
}
/// Resolve declarations of generated Wayfinder imports only. No controller name
/// is guessed from its generated module: navigation targets the generated source.
/// Imports are restricted to the configured alias's actions/routes roots.
pub(crate) fn wayfinder_imports(text:&str, alias:&str)->Vec<WayfinderImport> {
    if text.len()>512*1024 || alias.is_empty() {return vec![];}
    let Some(masked)=code(text) else {return vec![];};
    let mut out=Vec::new();
    for (at,_) in masked.match_indices("import") {
        if at>0 && word(masked.as_bytes()[at-1]) {continue;}
        if masked.as_bytes().get(at+6).is_some_and(|b|word(*b)) {continue;}
        let end=text[at..].find(';').map(|n|at+n).unwrap_or_else(||text[at..].find('\n').map(|n|at+n).unwrap_or(text.len()));
        let Some(statement)=text.get(at+6..end) else {continue;};
        let Some(from)=masked.get(at+6..end).and_then(|s|s.rfind("from")) else {continue;};
        let bindings=statement[..from].trim();
        let module_text=statement[from+4..].trim();
        if module_text.len()<2 {continue;}
        let quote=module_text.as_bytes()[0];
        if !matches!(quote,b'\''|b'"') || module_text.as_bytes().last()!=Some(&quote) {continue;}
        let module=&module_text[1..module_text.len()-1];
        let Some(relative)=module.strip_prefix(alias).and_then(|s|s.strip_prefix('/')) else {continue;};
        if !(relative.starts_with("actions/")||relative.starts_with("routes/")) || relative.split('/').any(|p|p.is_empty()||p=="."||p=="..") || !relative.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'/'|b'_'|b'-'|b'.')) {continue;}
        let pairs:Vec<(&str,&str)>=if let Some(named)=bindings.strip_prefix('{').and_then(|s|s.strip_suffix('}')) {
            named.split(',').filter_map(|name| {
                let mut words=name.split_whitespace();let exported=words.next()?;
                match (words.next(),words.next(),words.next()) { (None,None,None)=>Some((exported,exported)),(Some("as"),Some(local),None)=>Some((exported,local)),_=>None }
            }).collect()
        } else if !bindings.contains([',',' ','*']) {vec![("default",bindings)]} else {vec![]};
        for (exported,local) in pairs {
            if local.is_empty() || !local.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'_'|b'$')) {continue;}
            let Some(start)=text[at+6..at+6+from].find(local).map(|n|at+6+n) else {continue;};
            out.push(WayfinderImport {module:module.into(),local:local.into(),exported:exported.into(),range:start..start+local.len()});
        }
    }
    out
}

/// Resolve an imported Wayfinder binding under the cursor, including call-site
/// identifiers. Reassignment/shadowing of that local name makes it ambiguous.
pub(crate) fn wayfinder_reference_at(text:&str, at:usize, alias:&str)->Option<WayfinderImport> {
    if at>text.len() || !text.is_char_boundary(at) {return None;}
    let masked=code(text)?;
    let imports=wayfinder_imports(text,alias);
    let bytes=masked.as_bytes();
    let mut start=at;while start>0 && js_word(bytes[start-1]) {start-=1;}
    let mut end=at;while end<bytes.len() && js_word(bytes[end]) {end+=1;}
    let name=masked.get(start..end)?;
    if name.is_empty() || masked[..start].trim_end().ends_with('.') {return None;}
    let mut matches=imports.iter().filter(|i|i.local==name);
    let item=matches.next()?.clone();if matches.next().is_some(){return None;}
    // Any additional binding/declaration or assignment is withheld. General JS
    // lexical scope resolution belongs to the native language server.
    for (offset,_) in masked.match_indices(name) {
        if offset>0 && js_word(bytes[offset-1]) || bytes.get(offset+name.len()).is_some_and(|b|js_word(*b)) {continue;}
        if item.range.contains(&offset) {continue;}
        let before=masked[..offset].trim_end();let after=masked[offset+name.len()..].trim_start();
        if ["const","let","var","function","class","catch"].iter().any(|keyword| before.split_whitespace().last()==Some(keyword))
            || (after.starts_with('=') && !after.starts_with("==") && !after.starts_with("=>"))
            || after.starts_with("=>")
            || (before.ends_with(['(',',']) && after.starts_with([')',',',':']))
        {return None;}
    }
    Some(item)
}
fn js_word(byte:u8)->bool {byte.is_ascii_alphanumeric()||matches!(byte,b'_'|b'$')||byte>=128}

/// Go to a proven generated Wayfinder export. The @ alias is resolved from an
/// explicit ordinary tsconfig/jsconfig paths map, otherwise Laravel's default
/// resources/js root. Inertia page roots are not JavaScript module aliases.
pub(crate) fn wayfinder_definition(
    project:&openidea_plugin::Project,
    index:&crate::Index,
    doc:&openidea_plugin::providers::Document,
    at:openidea_plugin::Position,
)->Vec<openidea_plugin::Location> {
    use openidea_plugin::{Location,Range,host,text};
    let run=|| -> Option<Location> {
        if !index.metadata.package_enabled("laravel/wayfinder",true) {return None;}
        let root=project.root().ok()?;
        if !crate::owns_current_path(project,&doc.path) {return None;}
        let offset=text::offset(&doc.text,at)?;
        let reference=wayfinder_reference_at(&doc.text,offset,"@")?;
        let directory=javascript_root(project)?;
        let relative=reference.module.strip_prefix("@/")?;
        let base=format!("{directory}/{relative}");
        let mut candidates=Vec::new();
        for suffix in [".ts",".js","/index.ts","/index.js"] {
            let path=format!("{root}/{base}{suffix}");
            if !crate::owns_current_path(project,&path) {continue;}
            let Ok(source)=project.read_file(&path) else {continue;};
            if source.len()>512*1024 {return None;}
            candidates.push((path,source));
        }
        if candidates.len()!=1 {return None;}
        let (path,source)=candidates.pop()?;
        // A same-project target can become nested-project-owned during reads.
        if !crate::owns_current_path(project,&path) || host::project_of(&doc.path)?.root().ok()?!=root {return None;}
        let masked=code(&source)?;
        let (begin,end)=if reference.exported=="default" {
            let at=masked.find("export default ")?;(at,at+"export default".len())
        } else {
            let mut declarations=Vec::new();
            for prefix in ["export const ","export let ","export function ","export async function "] {
                let needle=format!("{prefix}{}",reference.exported);
                for (at,_) in masked.match_indices(&needle) {
                    let begin=at+prefix.len();let end=begin+reference.exported.len();
                    if at>0 && js_word(masked.as_bytes()[at-1]) || masked.as_bytes().get(end).is_some_and(|b|js_word(*b)) {continue;}
                    declarations.push((begin,end));
                }
            }
            if declarations.len()!=1 {return None;}
            declarations[0]
        };
        Some(Location {path,range:Range::new(text::position(&source,begin),text::position(&source,end))})
    };
    run().into_iter().collect()
}
fn javascript_root(project:&openidea_plugin::Project)->Option<String> {
    let root=project.root().ok()?;
    let mut aliases=std::collections::BTreeSet::new();
    for filename in ["tsconfig.json","jsconfig.json"] {
        let path=format!("{root}/{filename}");
        if !crate::owns_current_path(project,&path) {return None;}
        let Ok(source)=project.read_file(&path) else {continue;};
        if source.len()>128*1024 {return None;}
        let value=crate::strict_json::parse(&source,128*1024).ok()?;
        let Some(paths)=value.pointer("/compilerOptions/paths/@~1*") else {
            if value.get("extends").is_some() {return None;}continue;
        };
        let entries=paths.as_array()?;
        if entries.len()!=1 {return None;}
        let target=entries[0].as_str()?.strip_suffix("/*")?;
        let base=value.pointer("/compilerOptions/baseUrl").and_then(|s|s.as_str()).unwrap_or(".");
        let combined=format!("{base}/{target}");
        let parts:Vec<_>=combined.split('/').filter(|s|*s!=".").collect();
        if parts.is_empty()||parts.iter().any(|p|p.is_empty()||*p==".."||!p.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'_'|b'-'|b'.'))) {return None;}
        aliases.insert(parts.join("/"));
    }
    if aliases.len()>1 {None} else {Some(aliases.into_iter().next().unwrap_or_else(||"resources/js".into()))}
}

#[derive(Clone,Debug,PartialEq,Eq)]
pub(crate) struct Subscription {
    /// Class spelling in this PHP source; caller resolves its namespace/imports.
    pub event_class:String,
    pub method:String,
    pub event_range:Range<usize>,
    pub method_range:Range<usize>,
}
/// Literal array returned directly by a subscribe() method. Dynamic registration,
/// nested functions and callable arrays remain outside this facts adapter.
pub(crate) fn subscriber_events(text:&str)->Vec<Subscription> {
    let Some(masked)=code(text) else {return vec![];};
    let mut facts=Vec::new();
    for (at,_) in masked.match_indices("function subscribe") {
        if at>0&&word(masked.as_bytes()[at-1]) {continue;}
        let after=at+"function subscribe".len();
        let tail=masked[after..].trim_start();if !tail.starts_with('('){continue;}
        let open=masked.len()-tail.len();let Some(end)=php::closing(text,open) else {continue;};
        let Some(brace)=masked[end+1..].find('{').map(|n|end+1+n) else {continue;};
        // Only ordinary array return signatures, no intervening statement/body.
        if !matches!(masked[end+1..brace].trim(),""|": array") {continue;}
        let Some(close)=php::closing(text,brace) else {continue;};
        let body=masked[brace+1..close].trim();
        let Some(body)=body.strip_prefix("return") else {continue;};
        let body=body.trim_start();if !body.starts_with('['){continue;}
        let Some(array)=masked[brace+1..close].find('[').map(|n|brace+1+n) else {continue;};
        let Some(array_end)=php::closing(text,array) else {continue;};
        if masked[array_end+1..close].trim()!=";" {continue;}
        for item in php::items(text,array) {
            let Some(key)=item.key else {continue;};
            let Some(class)=php::class_constant(text,key.clone()) else {continue;};
            let Some((method,_))=php::literal(text,item.value) else {continue;};
            if method.is_empty()||!method.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'){continue;}
            let needle=format!("function {method}(");
            let methods:Vec<_>=masked.match_indices(&needle).collect();if methods.len()!=1 {continue;}
            let begin=methods[0].0+"function ".len();
            let key=php::trimmed(text,key);let event_start=key.start+usize::from(text[key.clone()].starts_with('\\'));
            facts.push(Subscription{event_class:class.into(),method:method.into(),event_range:event_start..event_start+class.len(),method_range:begin..begin+method.len()});
        }
    }
    facts
}

fn passport_reference(doc:&openidea_plugin::providers::Document, at:openidea_plugin::Position)->Option<Range<usize>> {
    let language=crate::language_of(&doc.path,&doc.language)?;
    let at=openidea_plugin::text::offset(&doc.text,at)?;
    if language=="blade" && crate::blade_php::opaque_at(&doc.text,at) {return None;}
    let reference=crate::refs::reference_at(language,&doc.text,at)?;
    if reference.kind!=crate::refs::Kind::Middleware {return None;}
    let literal=if let Some((_,literal))=crate::refs::literal_at(language,&doc.text,at) {literal} else {
        if reference.closed {return None;}
        let end=doc.text[reference.start..].find(['\n','\r']).map_or(doc.text.len(),|n|reference.start+n);
        reference.start..end
    };
    if !literal.contains(&at)&&at!=literal.end{return None;}
    passport_scope_part(&doc.text[literal.clone()],literal.start,at)
}
pub(crate) fn passport_completion(index:&crate::Index,doc:&openidea_plugin::providers::Document,at:openidea_plugin::Position)->Vec<openidea_plugin::providers::CompletionItem> {
    use openidea_plugin::{providers::{CompletionItem,CompletionKind},text,Range};
    if !index.metadata.package_enabled("laravel/passport",true) {return vec![];}
    let Some(range)=passport_reference(doc,at) else {return vec![];};
    let Some(project)=crate::project_for(&doc.path) else {return vec![];};
    index.metadata.passport.iter().filter(|(_,entry)|entry.path.is_empty()||crate::owns_current_path(&project,&entry.path)).map(|(name,entry)| {
        let mut item=CompletionItem::new(name,CompletionKind::Value);item.detail=entry.detail.clone();
        item.replace=Some(Range::new(text::position(&doc.text,range.start),text::position(&doc.text,range.end)));item
    }).collect()
}
pub(crate) fn passport_definition(index:&crate::Index,doc:&openidea_plugin::providers::Document,at:openidea_plugin::Position)->Vec<openidea_plugin::Location> {
    use openidea_plugin::{Location,Range,Position};
    let run=||->Option<Location> {
        if !index.metadata.package_enabled("laravel/passport",true){return None;}
        let range=passport_reference(doc,at)?;let name=&doc.text[range];
        let entry=index.metadata.passport.get(name)?;
        let project=crate::project_for(&doc.path)?;
        if entry.path.is_empty()||!crate::owns_current_path(&project,&entry.path){return None;}
        let source=project.read_file(&entry.path).ok()?;if source.len()>256*1024{return None;}
        let current=if entry.path.ends_with(".php") {
            passport_names(&entry.path,&source).remove(name)?
        } else {
            let value=crate::strict_json::parse(&source,256*1024).ok()?;
            if !passport_metadata(&value,false).into_iter().chain(passport_metadata(&value,true)).any(|s|s.name==name){return None;}
            // Metadata entries intentionally navigate to their source document.
            crate::index::Entry{path:entry.path.clone(),at:Position::new(0,0),width:0,detail:None}
        };
        if !crate::owns_current_path(&project,&current.path){return None;}
        Some(Location{path:current.path,range:Range::new(current.at,Position::new(current.at.line,current.at.character+current.width))})
    };
    run().into_iter().collect()
}

/// Resolve one class spelling through an ordinary, unambiguous PHP header.
/// Grouped imports/multiple or braced namespaces are withheld.
pub(crate) fn resolve_class(source:&str,name:&str)->Option<String> {
    if name.is_empty()||!name.trim_start_matches('\\').split('\\').all(|p|!p.is_empty()&&p.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_')) {return None;}
    if name.starts_with('\\'){return Some(name.trim_start_matches('\\').into());}
    let masked=code(source)?;
    let mut namespace=String::new();let mut aliases:BTreeMap<String,Vec<String>>=BTreeMap::new();let mut count=0;
    for (at,_) in masked.match_indices("namespace ") {
        if at>0&&word(masked.as_bytes()[at-1]){continue;}
        let rest=&masked[at+10..];let end=rest.find([';','{'])?;
        if rest.as_bytes()[end]!=b';'{return None;}
        namespace=rest[..end].trim().into();count+=1;
    }
    if count>1{return None;}
    for (at,_) in masked.match_indices("use ") {
        if at>0&&word(masked.as_bytes()[at-1]){continue;}
        let depth=masked[..at].bytes().fold(0i32,|d,b|match b{b'{'=>d+1,b'}'=>d-1,_=>d});if depth!=0{continue;}
        let end=masked[at+4..].find(';')?+at+4;let item=&masked[at+4..end];
        if item.contains(['{',',']){return None;}
        let mut words=item.split_whitespace();let fqn=words.next()?.trim_start_matches('\\');
        if matches!(fqn,"function"|"const"){continue;}
        let alias=match (words.next(),words.next(),words.next()) {(None,None,None)=>fqn.rsplit('\\').next()?,(Some("as"),Some(alias),None)=>alias,_=>return None};
        aliases.entry(alias.into()).or_default().push(fqn.into());
    }
    let (first,suffix)=name.split_once('\\').map_or((name,None),|(a,b)|(a,Some(b)));
    if let Some(values)=aliases.get(first) {
        if values.len()!=1{return None;}
        return Some(suffix.map_or_else(||values[0].clone(),|s|format!("{}\\{s}",values[0])));
    }
    if matches!(first,"self"|"static"|"parent"){return None;}
    Some(if namespace.is_empty(){name.into()}else{format!("{namespace}\\{name}")})
}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(crate) struct AttributeClass {pub class:String,pub range:Range<usize>}
pub(crate) fn attribute_classes(source:&str)->Vec<AttributeClass> {
    let Some(masked)=code(source) else{return vec![];};let mut out=Vec::new();
    for (at,_) in masked.match_indices("#[") {
        let Some(end)=php::closing(source,at+1) else{continue;};
        for part in php::split(source,at+2..end) {
            let contents=masked[part.clone()].trim();let Some(paren)=contents.find('(') else{continue;};
            let attribute=contents[..paren].trim();
            let Some(attribute)=resolve_class(source,attribute) else{continue;};
            if !["UseFactory","UsePolicy","CollectedBy","UseEloquentBuilder"].iter().any(|name|attribute==format!("Illuminate\\Database\\Eloquent\\Attributes\\{name}")){continue;}
            let Some(open)=masked[part.clone()].find('(').map(|p|part.start+p) else{continue;};
            let args=php::arguments(source,open);if args.len()!=1{continue;}
            let arg=php::trimmed(source,args[0].clone());let Some(raw)=source[arg.clone()].strip_suffix("::class") else{continue;};
            let raw=raw.trim();let Some(class)=resolve_class(source,raw) else{continue;};
            out.push(AttributeClass{class,range:arg.start..arg.start+raw.len()});
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn passport_exact_imports_and_unicode_offsets() {
        let source="<?php use Laravel\\Passport\\Passport as OAuth; // é\n OAuth::tokensCan(['orders:read'=>'Read orders','write'=>'Write']);";
        let scopes=passport_scopes(source);assert_eq!(scopes.len(),2);
        assert_eq!(scopes[0].name,"orders:read");assert_eq!(&source[scopes[0].range.clone().unwrap()],"orders:read");
        assert_eq!(passport_scopes("<?php \\Laravel\\Passport\\Passport::tokensCan(['a'=>'A']);").len(),1);
    }
    #[test] fn fake_ambiguous_and_dynamic_passport_calls_are_withheld() {
        for source in ["<?php Passport::tokensCan(['a'=>'A']);","<?php use App\\Passport; Passport::tokensCan(['a'=>'A']);","<?php // \\Laravel\\Passport\\Passport::tokensCan(['a'=>'A']);", "<?php $x=\"\\\\Laravel\\\\Passport\\\\Passport::tokensCan(['a'=>'A']);\";", "<?php \\Laravel\\Passport\\Passport::tokensCan(['a'=>$description]);", "<?php \\Laravel\\Passport\\Passport::tokensCan(['a'=>'$description']);"] {assert!(passport_scopes(source).is_empty(),"{source}");}
    }
    #[test] fn passport_import_collision_and_nested_use_are_withheld() {
        for source in [
            "<?php use Laravel\\Passport\\Passport; use App\\Other as Passport; Passport::tokensCan(['a'=>'A']);",
            "<?php class Other { use Laravel\\Passport\\Passport; } Passport::tokensCan(['a'=>'A']);",
            "<?php namespace A; use Laravel\\Passport\\Passport; namespace B; Passport::tokensCan(['a'=>'A']);",
        ] {assert!(passport_scopes(source).is_empty(),"{source}");}
        let text="<?php /* é */ \\Laravel\\Passport\\Passport::tokensCan(['café'=>'description']);";
        let names=passport_names("/app/provider.php",text);
        assert_eq!(names["café"].width,4);
        assert_eq!(names["café"].at,openidea_plugin::text::position(text,text.find("café").unwrap()));
    }
    #[test] fn metadata_and_middleware_parts() {
        let value=serde_json::json!({"packages":{"passport":{"scopes":{"read":"Read"}}}});
        assert_eq!(passport_metadata(&value,false)[0].name,"read");
        assert_eq!(passport_scope_part("scopes:read,orders:write",10,25),Some(22..34));
        assert_eq!(passport_scope_part("other:read",0,8),None);
    }
    #[test] fn wayfinder_imports_preserve_export_local_and_destinations() {
        let source="import { index as list, show } from '@/actions/App/Http/Controllers/PostController';\nimport Posts from '@/routes/posts';";
        let found=wayfinder_imports(source,"@");assert_eq!(found.len(),3);
        assert_eq!(found[0].exported,"index");assert_eq!(found[0].local,"list");assert_eq!(&source[found[0].range.clone()],"list");assert_eq!(found[2].exported,"default");
    }
    #[test] fn wayfinder_reference_rechecks_shadowing_and_source() {
        let text="import { show } from '@/routes/posts'; show(1);";
        assert_eq!(wayfinder_reference_at(text,text.rfind("show").unwrap()+2,"@").unwrap().module,"@/routes/posts");
        let shadowed="import { show } from '@/routes/posts'; const show = other; show(1);";
        assert!(wayfinder_reference_at(shadowed,shadowed.rfind("show").unwrap()+2,"@").is_none());
        assert!(wayfinder_reference_at(text,text.find("routes").unwrap()+2,"@").is_none());
        for tail in ["function f(show, other) {show(1)}", "const f = show => show(1)", "object.show(1)"] {
            let source=format!("import {{ show }} from '@/routes/posts'; {tail}");
            assert!(wayfinder_reference_at(&source,source.rfind("show").unwrap()+2,"@").is_none(),"{tail}");
        }
    }
    #[test] fn wayfinder_definition_uses_current_owned_generated_export() {
        use openidea_plugin::{mock::MockHost,providers::Document,text};
        let host=MockHost::install().with_capabilities(&["project.read"]);
        let source="import { show as display } from '@/routes/posts'; display(1);";
        host.add_file("resources/js/app.ts",source);
        let generated="// é\nexport const show = (id: number) => id;";
        host.add_file("resources/js/routes/posts/index.ts",generated);
        let project=host.invocation().project().unwrap();
        let doc=Document {path:"/project/resources/js/app.ts".into(),language:"typescript".into(),version:1,text:source.into()};
        let at=text::position(source,source.rfind("display").unwrap()+2);
        let index=crate::Index::default();
        let found=wayfinder_definition(&project,&index,&doc,at);
        assert_eq!(found.len(),1);assert!(found[0].path.ends_with("posts/index.ts"));
        assert_eq!(found[0].range.start,text::position(generated,generated.find("show").unwrap()));
        host.add_project("/project/resources/js/routes");
        assert!(wayfinder_definition(&project,&index,&doc,at).is_empty());
    }
    #[test] fn wayfinder_definition_resolves_configured_alias_and_refuses_ambiguity() {
        use openidea_plugin::{mock::MockHost,providers::Document,text};
        let host=MockHost::install().with_capabilities(&["project.read"]);
        let source="import { show } from '@/actions/Posts'; show(1);";
        host.add_file("frontend/app.ts",source);
        host.add_file("tsconfig.json",r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["./frontend/*"]}}}"#);
        host.add_file("frontend/actions/Posts.ts","export const show = () => 1;");
        let project=host.invocation().project().unwrap();
        let doc=Document {path:"/project/frontend/app.ts".into(),language:"typescript".into(),version:1,text:source.into()};
        let at=text::position(source,source.rfind("show").unwrap()+2);
        assert_eq!(wayfinder_definition(&project,&crate::Index::default(),&doc,at).len(),1);
        host.add_file("frontend/actions/Posts/index.ts","export const show = () => 1;");
        assert!(wayfinder_definition(&project,&crate::Index::default(),&doc,at).is_empty());
        host.add_file("tsconfig.json",r#"{"compilerOptions":{"paths":{"@/*":["../secret/*"]}}}"#);
        assert!(wayfinder_definition(&project,&crate::Index::default(),&doc,at).is_empty());
    }
    #[test] fn literal_event_subscriber_returns_methods_with_source_ranges() {
        let text="<?php class Subscriber { public function subscribe(): array { return [ Login::class => 'onLogin' ]; } public function onLogin(Login $event) {} }";
        let found=subscriber_events(text);assert_eq!(found.len(),1);
        assert_eq!(found[0].event_class,"Login");assert_eq!(&text[found[0].method_range.clone()],"onLogin");
        assert!(subscriber_events("<?php class A {function subscribe() { return $map; }}").is_empty());
        assert!(subscriber_events("<?php /* function subscribe() { return [Login::class=>'fake']; } */").is_empty());
    }
    #[test] fn passport_middleware_completion_and_navigation_revalidate_source() {
        use openidea_plugin::{mock::MockHost,providers::Document,text};
        let host=MockHost::install().with_capabilities(&["project.read"]);
        let registration="<?php use Laravel\\Passport\\Passport; Passport::tokensCan(['read'=>'Read documents','orders:write'=>'Write orders']);";
        host.add_file("app/Providers/AuthServiceProvider.php",registration);
        let mut index=crate::Index::default();index.metadata.passport=passport_names("/project/app/Providers/AuthServiceProvider.php",registration);
        for (source,language,path) in [("<?php Route::get('/', fn()=>1)->middleware('scopes:read,orders:write');","php","/project/routes/web.php"),("{{ route('home') }}","blade","/project/resources/views/app.blade.php")] {
            let doc=Document{path:path.into(),language:language.into(),version:1,text:source.into()};
            let offset=source.find("orders:write").map(|n|n+3).unwrap_or(4);let at=text::position(source,offset);
            let found=passport_completion(&index,&doc,at);
            if language=="blade" {assert!(found.is_empty());continue;}
            assert_eq!(found.len(),2);
            let replace=found[0].replace.unwrap();assert_eq!(&source[text::offset(source,replace.start).unwrap()..text::offset(source,replace.end).unwrap()],"orders:write");
            assert_eq!(passport_definition(&index,&doc,at).len(),1);
            host.add_file("app/Providers/AuthServiceProvider.php","<?php // removed registration");
            assert!(passport_definition(&index,&doc,at).is_empty());
        }
        let source="<?php Route::middleware('scope:rea";
        let doc=Document{path:"/project/routes/web.php".into(),language:"php".into(),version:1,text:source.into()};
        assert_eq!(passport_completion(&index,&doc,text::position(source,source.len())).len(),2);
        index.metadata.disabled_packages.insert("laravel/passport".into());
        assert!(passport_completion(&index,&doc,text::position(source,source.len())).is_empty());
    }
    #[test] fn attributes_require_framework_identity_and_resolve_exact_imports() {
        let source="<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Attributes\\UsePolicy as Policy; use App\\Policies\\Custom; #[Policy(Custom::class)] class Post extends Model {}";
        let facts=attribute_classes(source);assert_eq!(facts.len(),1);assert_eq!(facts[0].class,"App\\Policies\\Custom");
        assert_eq!(&source[facts[0].range.clone()],"Custom");
        let lookalike=source.replace("Illuminate\\Database\\Eloquent\\Attributes\\UsePolicy","App\\UsePolicy");
        assert!(attribute_classes(&lookalike).is_empty());
        assert!(attribute_classes("<?php $x = '#[UsePolicy(Custom::class)]';").is_empty());
    }
    #[test] fn wayfinder_ignores_comments_strings_and_unowned_module_shapes() {
        for source in ["// import {x} from '@/routes/posts';", "const x=\"import {x} from '@/routes/posts';\";", "import {x} from '@/routes/../secret';", "import {x} from 'other/routes/posts';", "import {x} from '@/Pages/Post';"] {assert!(wayfinder_imports(source,"@").is_empty(),"{source}");}
    }
}
