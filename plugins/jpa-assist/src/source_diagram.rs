//! Explicit source-declared ER graph. No effective ORM/database model is inferred.
use crate::*;
use spring_text::{diagram::{self,Node,Edge},java};
use std::collections::{BTreeMap,BTreeSet};
pub(crate) const VIEW:&str="source-diagram";
const MAX_FILES:u32=256;
const MAX_TEXT:usize=128*1024;
#[derive(Default)] pub(crate) struct State {
    serial:u64,root:Option<String>,paths:Vec<String>,snapshots:BTreeMap<String,String>,
    nodes:Vec<Node>,edges:Vec<Edge>,locations:BTreeMap<String,(String,Position)>,status:String,stale:bool,
}
fn refresh_view(){ui_kit::web::refresh_view(VIEW,include_str!("ui_config.json"));let _=ui_kit::web::post_view_message(VIEW,r#"{"channel":"ui-kit-refresh"}"#,include_str!("ui_config.json"));}
fn scope(path:&str)->Option<String>{
    for segment in ["/src/main/java/","/src/main/kotlin/"]{if let Some((base,_))=path.split_once(segment){return Some(format!("{base}/src/main"));}}
    for segment in ["/src/test/java/","/src/test/kotlin/"]{if let Some((base,_))=path.split_once(segment){return Some(format!("{base}/src/test"));}}None
}
fn names(value:&str)->bool{!value.is_empty()&&value.split('.').all(|p|!p.is_empty()&&p.bytes().enumerate().all(|(i,b)|b.is_ascii_alphabetic()||b"_$".contains(&b)||i>0&&b.is_ascii_digit()))}
fn qualified(class:&Class,name:&str)->Option<String>{
    if !names(name){return None;}if name.contains('.') {let root=name.split('.').next()?;if class.name==root||class.imports.iter().any(|import|import.rsplit('.').next()==Some(root)){return None;}return Some(name.into());}
    let imports:Vec<_>=class.imports.iter().filter(|i|i.rsplit('.').next()==Some(name)).collect();
    match imports.as_slice(){[import]=>Some((*import).clone()),[]=>Some(format!("{}.{}",class.package.as_ref()?,name)),_=>None}
}
fn target(class:&Class,attribute:&model::Attribute,text:&str)->Option<String>{
    let masked=mask(text);let position=offset(text,attribute.pos)?;
    let all=annotations(&masked);
    // A local targetEntity/targetClass override needs its own resolved model.
    let (class_at,_)=model::declaration_keyword(&masked,0,&["class"])?;let open=class_at+masked[class_at..].find('{')?;
    let (from,to)=model::members(&masked,open+1,class.body_end?,class.kotlin,false).into_iter().find(|(from,to)|*from<=position&&position<*to)?;
    let declaration=model::without_annotations(&masked,from,to);let stop=from+declaration.find(['(','=','{',';']).unwrap_or(declaration.len());
    let mapping=match attribute.relation{Relation::ManyToOne=>"ManyToOne",Relation::OneToMany=>"OneToMany",Relation::OneToOne=>"OneToOne",Relation::ManyToMany=>"ManyToMany",Relation::Embedded=>"Embedded",Relation::ElementCollection=>"ElementCollection",Relation::None=>return None};
    let mappings:Vec<_>=all.iter().filter(|a|a.name==mapping&&from<=a.start&&a.end()<=stop&&!all.iter().any(|outer|outer.start<a.start&&outer.args.is_some_and(|(from,to)|from<=a.start&&a.end()<=to))).collect();
    let [mapping]=mappings.as_slice()else{return None;};
    if mapping.args.is_some_and(|(from,to)|masked[from..to].contains("targetEntity")||masked[from..to].contains("targetClass")){return None;}
    let raw=attribute.ty.trim().trim_end_matches('?');
    let reference=if attribute.relation.to_many()||attribute.relation==Relation::ElementCollection {
        let (container,arguments)=raw.split_once('<')?;let container=qualified(class,container.trim())?;
        if !matches!(container.as_str(),"java.util.Collection"|"java.util.List"|"java.util.Set"|"java.util.Map"|"kotlin.collections.Collection"|"kotlin.collections.List"|"kotlin.collections.Set"|"kotlin.collections.Map"|"kotlin.collections.MutableList"|"kotlin.collections.MutableSet"|"kotlin.collections.MutableMap"){return None;}
        model::split_top_level(arguments.strip_suffix('>')?).last()?.1.trim().to_owned()
    }else{raw.to_owned()};qualified(class,&reference)
}
fn paths(project:&Project,root:&str)->Result<(Vec<String>,bool),Error>{
    let mut paths=BTreeSet::new();let mut limited=false;
    for glob in ["**/*.java","**/*.kt"]{let found=project.find_files(glob,MAX_FILES+1)?;limited|=found.len()>MAX_FILES as usize;for path in found.into_iter().take(MAX_FILES as usize){if host::project_of(&path).and_then(|p|p.root().ok()).as_deref()==Some(root){paths.insert(path);}}}
    limited|=paths.len()>MAX_FILES as usize;Ok((paths.into_iter().take(MAX_FILES as usize).collect(),limited))
}
fn closed(text:&str)->bool {
    if text.contains("\\u"){return false;}
    let mut stack=Vec::new();for token in java::tokens(text){if token.s.starts_with('"')&&!token.s.ends_with('"'){return false;}match token.s{"("=>stack.push(")"),"["=>stack.push("]"),"{"=>stack.push("}"),")"|"]"|"}"=>if stack.pop()!=Some(token.s){return false;},_=>{}}if stack.len()>128{return false;}}stack.is_empty()
}
impl JpaAssist {
    pub(crate) fn refresh_source_diagram(&mut self,ctx:&Invocation)->Result<(),Error>{
        let project=ctx.project().ok_or(Error::Stale)?;let root=project.root()?;
        let serial=self.diagram.serial.checked_add(1).ok_or(Error::Stale)?;
        let (paths,limited)=paths(&project,&root)?;
        let mut snapshots=BTreeMap::new();let mut models=Vec::new();let mut definitions:BTreeMap<(String,String),Vec<String>>=BTreeMap::new();let mut incomplete=BTreeSet::new();let mut unknown=0usize;let mut total=0usize;
        for path in &paths {
            let group=scope(path);
            let text=match project.read_file(path){Ok(text) if text.len()<=MAX_TEXT=>text,_=>{unknown+=1;if let Some(group)=group{incomplete.insert(group);}continue;}};
            total+=text.len();if total>8*1024*1024{return Err("Source graph inventory exceeds 8 MiB; open a narrower source project.".into());}
            if !closed(&text) || path.ends_with(".kt")&&java::tokens(&text).iter().any(|token|token.s=="typealias"){if let Some(group)=&group{incomplete.insert(group.clone());}}
            let source=spring_text::parse_source(path,&text);
            if let Some(group)=&group {for ty in source.types.iter().filter(|t|t.outer.is_none()) {let fq=format!("{}.{}",source.package,ty.name);if !source.package.is_empty()&&names(&fq){definitions.entry((group.clone(),fq)).or_default().push(path.clone());}}}
            let doc=Document{path:path.clone(),text:text.clone(),language:if path.ends_with(".kt"){"kotlin"}else{"java"}.into(),version:0};
            if let Some(class)=attribute_info::snapshot(&doc){models.push((class,group));}else{
                unknown+=1;
                if let Some(mut class)=parse_class(&text,path).filter(|class|live::trusted_annotations(&text)&&live::top_level_identity(&text,class)&&checks::owned_class_start(&text,&mask(&text),class).is_some()&&class.package.as_deref()==Some(source.package.as_str())) {
                    // Read-only direct mapping contracts can still be shown. No
                    // generated/inherited fields or effective access facts are inferred.
                    let mut counts=BTreeMap::new();for attribute in &class.attributes{*counts.entry(attribute.name.clone()).or_insert(0usize)+=1;}
                    class.attributes.retain(|attribute|attribute.relation!=Relation::None&&counts.get(&attribute.name)==Some(&1));models.push((class,group));
                }
            }
            snapshots.insert(path.clone(),text);
        }
        let mut nodes=Vec::new();let mut locations=BTreeMap::new();let mut ids=BTreeMap::new();let mut duplicate=0;
        for (class,group) in &models {
            if nodes.len()>=128{break;}
            let fq=format!("{}.{}",class.package.as_deref().unwrap_or(""),class.name);
            let id=format!("source:{serial}:{}",nodes.len());
            nodes.push(Node::new(&id,&class.name).detail(&format!("{} · {} · {}",fq,class.path.strip_prefix(&format!("{root}/")).unwrap_or(&class.path),match class.kind{Kind::Entity=>"Declared Entity",Kind::Embeddable=>"Declared Embeddable",Kind::MappedSuperclass=>"Declared MappedSuperclass"})).accent(class.kind==Kind::Entity));
            locations.insert(id.clone(),(class.path.clone(),class.pos));
            if let Some(group)=group {let key=(group.clone(),fq);if definitions.get(&key).is_some_and(|paths|paths.len()==1){ids.insert(key,id);}else{duplicate+=1;}}
        }
        let mut edges=Vec::new();let mut unresolved=0;
        for (class,group) in &models {
            let Some(group)=group else{unresolved+=class.attributes.iter().filter(|a|a.relation!=Relation::None).count();continue;};
            if limited||incomplete.contains(group){unresolved+=class.attributes.iter().filter(|a|a.relation!=Relation::None).count();continue;}
            let text=&snapshots[&class.path];let masked=mask(text);let header_generic=model::declaration_keyword(&masked,0,&["class"]).and_then(|(at,_)|masked[at+5..].find('{').map(|end|&masked[at+5..at+5+end])).is_some_and(|header|header.trim_start().strip_prefix(&class.name).is_some_and(|tail|tail.trim_start().starts_with('<')));
            if header_generic || class.kotlin&&text.lines().any(|line|line.trim_start().starts_with("import ")&&line.contains(" as ")){unresolved+=class.attributes.iter().filter(|a|a.relation!=Relation::None).count();continue;}
            let fq=format!("{}.{}",class.package.as_deref().unwrap_or(""),class.name);let Some(from)=ids.get(&(group.clone(),fq)) else{continue;};
            if let Some(parent)=&class.superclass {if let Some(to)=qualified(class,parent).and_then(|fq|ids.get(&(group.clone(),fq))){if edges.len()<512{edges.push(Edge::new(from,to).label("declared extends"));}}else{unresolved+=1;}}
            for attribute in class.attributes.iter().filter(|a|a.relation!=Relation::None){
                let found=target(class,attribute,&snapshots[&class.path]).and_then(|fq|ids.get(&(group.clone(),fq)));
                let Some(to)=found else{unresolved+=1;continue;};if edges.len()>=512{unresolved+=1;continue;}
                let kind=match attribute.relation{Relation::ManyToOne=>"many → one",Relation::OneToMany=>"one → many",Relation::OneToOne=>"one → one",Relation::ManyToMany=>"many ↔ many",Relation::Embedded=>"embedded",Relation::ElementCollection=>"elements",Relation::None=>continue};
                edges.push(Edge::new(from,to).label(&format!("{} · {kind}",attribute.name)));
            }
        }
        let status=format!("Source declarations only · {} nodes / {} declared edges · {} unadmitted source files · {unresolved} unresolved source links · {duplicate} ambiguous identities · {} incomplete source sets{} · no effective XML/library/provider/database mapping",nodes.len(),edges.len(),unknown,incomplete.len(),if limited||models.len()>128{" · PARTIAL bounded inventory/presentation"}else{""});
        self.diagram=State{serial,root:Some(root),paths,snapshots,nodes,edges,locations,status,stale:false};refresh_view();Ok(())
    }
    pub(crate) fn render_source_diagram(&mut self,ctx:&Invocation)->Result<View,Error>{
        let root=ctx.project().and_then(|p|p.root().ok());
        if root!=self.diagram.root {self.diagram.root=None;self.diagram.nodes.clear();self.diagram.edges.clear();self.diagram.locations.clear();self.diagram.snapshots.clear();self.diagram.paths.clear();self.diagram.stale=true;}
        if root.is_none(){return Ok(View::message("Open a project to inspect its source-declared persistence graph."));}
        let refresh=Button::new("diagram-refresh","Refresh Source Diagram").icon("refresh");
        if self.diagram.nodes.is_empty()||self.diagram.stale{return Ok(View::message("Refresh to capture a bounded, qualified source-declared entity/embeddable relationship graph. Effective XML, library and database mappings are not inferred.").button(refresh));}
        Ok(View::new(Body::Html(diagram::html(&self.diagram.nodes,&self.diagram.edges,"No admitted source entities."))).button(refresh).status(&self.diagram.status))
    }
    pub(crate) fn handle_source_diagram(&mut self,event:ViewEvent,ctx:&Invocation)->Result<(),Error>{
        match event {
            ViewEvent::Button(button) if button=="diagram-refresh"=>self.refresh_source_diagram(ctx),
            ViewEvent::Message(key)|ViewEvent::Activate(key)=>{
                let project=ctx.project().ok_or(Error::Stale)?;let root=project.root()?;
                if self.diagram.stale||self.diagram.root.as_deref()!=Some(root.as_str()){return Err(Error::Stale);}
                let (path,position)=self.diagram.locations.get(&key).ok_or(Error::Stale)?;
                let (current,_)=paths(&project,&root)?;
                if current!=self.diagram.paths||self.diagram.snapshots.iter().any(|(file,text)|host::project_of(file).and_then(|p|p.root().ok()).as_deref()!=Some(root.as_str())||project.read_file(file).ok().as_deref()!=Some(text.as_str())){return Err(Error::Stale);}
                host::open_location(path,position.line,position.character)
            },_=>Ok(())
        }
    }
    pub(crate) fn source_diagram_events(&mut self,events:&[Event]){
        if self.diagram.root.is_none(){return;}
        if events.iter().any(|event|match event {Event::FileOpened(path)|Event::FileSaved(path)=>self.diagram.paths.contains(path),Event::FilesChanged(changes)=>changes.created.iter().chain(&changes.modified).chain(&changes.removed).any(|path|path.ends_with(".java")||path.ends_with(".kt")),_=>false}){self.diagram.stale=true;refresh_view();}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const CUSTOMER:&str="package shop;\n@jakarta.persistence.Entity class Customer { @jakarta.persistence.Id long id; }";
    const ORDER:&str="package shop;\n@jakarta.persistence.Entity class Order { @jakarta.persistence.Id long id; @jakarta.persistence.ManyToOne Customer customer; }";
    fn setup()->(MockHost,JpaAssist){let host=MockHost::install().with_capabilities(&["project.read","editor.read"]);host.add_file("src/main/java/shop/Customer.java",CUSTOMER);host.add_file("src/main/java/shop/Order.java",ORDER);(host,JpaAssist::default())}
    #[test]
    fn qualified_source_relationship_graph_is_visual_navigable_and_snapshot_pinned(){
        let(host,mut plugin)=setup();plugin.refresh_source_diagram(&host.invocation()).unwrap();assert_eq!(plugin.diagram.nodes.len(),2);assert_eq!(plugin.diagram.edges.len(),1);assert!(plugin.diagram.status.contains("no effective XML/library/provider/database mapping"));
        let view=plugin.render_source_diagram(&host.invocation()).unwrap();let Body::Html(html)=view.body else{panic!("graph")};assert!(html.contains("<svg")&&html.contains("customer"));
        let key=plugin.diagram.locations.iter().find(|(_, (path,_))|path.ends_with("/Customer.java")).unwrap().0.clone();plugin.handle_source_diagram(ViewEvent::Message(key.clone()),&host.invocation()).unwrap();assert_eq!(host.opened_locations().len(),1);
        host.add_file("src/main/java/shop/Customer.java",&CUSTOMER.replace("long id","int id"));assert!(plugin.handle_source_diagram(ViewEvent::Message(key),&host.invocation()).is_err());assert_eq!(host.opened_locations().len(),1);
        assert!(host.requests().is_empty()&&host.process_runs().is_empty());
    }
    #[test]
    fn duplicate_generic_foreign_and_unknown_type_targets_do_not_manufacture_edges(){
        let(host,mut plugin)=setup();host.add_file("src/main/kotlin/shop/Customer.kt","package shop\n@jakarta.persistence.Entity class Customer { @jakarta.persistence.Id var id: Long = 0 }");plugin.refresh_source_diagram(&host.invocation()).unwrap();assert!(plugin.diagram.edges.is_empty());
        let(host,mut plugin)=setup();host.add_file("src/main/java/shop/Order.java",&ORDER.replace("class Order {","class Order<Customer> {"));plugin.refresh_source_diagram(&host.invocation()).unwrap();assert!(plugin.diagram.edges.is_empty());
        let(host,mut plugin)=setup();host.add_file("src/main/java/shop/Order.java",&ORDER.replace("@jakarta.persistence.ManyToOne","@vendor.ManyToOne"));plugin.refresh_source_diagram(&host.invocation()).unwrap();assert!(plugin.diagram.edges.is_empty());
    }
    #[test]
    fn packaged_kit_protocol_refreshes_renders_and_dispatches_graph_navigation(){
        let(host,mut plugin)=setup();
        let request=r#"{"channel":"ui-kit","sequence":1,"frameId":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","project":"/project","op":"event","event":{"kind":"button","key":"diagram-refresh"}}"#;
        openidea_plugin::views::Views::handle(&mut plugin,VIEW,ViewEvent::Message(request.into()),&host.invocation()).unwrap();assert_eq!(plugin.diagram.nodes.len(),2);
        assert!(host.view_messages().iter().any(|(view,message)|view==VIEW&&message.contains("\"kind\":\"html\"")&&message.contains("<svg")),"packaged bridge publishes actual model HTML");
        let key=plugin.diagram.locations.keys().next().unwrap();let request=format!(r#"{{"channel":"ui-kit","sequence":2,"frameId":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","project":"/project","op":"event","event":{{"kind":"message","value":"{key}"}}}}"#);
        openidea_plugin::views::Views::handle(&mut plugin,VIEW,ViewEvent::Message(request),&host.invocation()).unwrap();assert_eq!(host.opened_locations().len(),1);
    }

    #[test]
    fn additional_canonical_lombok_metadata_does_not_hide_direct_source_relationships(){
        let(host,mut plugin)=setup();host.add_file("src/main/java/shop/Order.java",&ORDER.replace("@jakarta.persistence.Entity","@jakarta.persistence.Entity @lombok.Data"));plugin.refresh_source_diagram(&host.invocation()).unwrap();assert_eq!(plugin.diagram.nodes.len(),2);assert_eq!(plugin.diagram.edges.len(),1);assert!(plugin.diagram.status.contains("unadmitted source files"));
    }

    #[test]
    fn source_sets_membership_events_and_root_changes_invalidate_navigation(){
        let(host,mut plugin)=setup();host.add_file("src/test/java/shop/Other.java",&ORDER.replace("class Order","class Other"));plugin.refresh_source_diagram(&host.invocation()).unwrap();assert_eq!(plugin.diagram.edges.len(),1,"test sources do not borrow main target scope");
        let key=plugin.diagram.locations.keys().next().unwrap().clone();host.add_file("src/main/java/shop/New.java","package shop; class New {}");assert!(plugin.handle_source_diagram(ViewEvent::Message(key.clone()),&host.invocation()).is_err());
        plugin.source_diagram_events(&[Event::FileSaved("/project/src/main/java/shop/Order.java".into())]);assert!(plugin.diagram.stale);assert!(matches!(plugin.render_source_diagram(&host.invocation()).unwrap().body,Body::Message(_)));
        host.set_project_root(Some("/other"));assert!(plugin.handle_source_diagram(ViewEvent::Message(key),&host.invocation()).is_err());assert!(matches!(plugin.render_source_diagram(&host.invocation()).unwrap().body,Body::Message(_)));
    }
}
