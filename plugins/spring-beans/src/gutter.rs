//! Native multi-target source chooser. Tickets bind the origin document/version
//! and cached target models, then validate actual source before/after the picker.
use crate::*;
use openidea_plugin::language_features::{GutterAction,LanguageFeatures};
use sha2::{Digest,Sha256};
const COMMAND:&str="choose-source-gutter-target";
#[derive(Clone)] struct Choice {label:String,location:Location,model:[u8;32]}
#[derive(Clone)] struct Pending {root:String,doc:Document,choices:Vec<Choice>}
#[derive(Default)] pub(crate) struct State {next:u64,pending:BTreeMap<String,Pending>}
fn digest(model:&FileModel)->[u8;32]{Sha256::digest(format!("{model:?}").as_bytes()).into()}
fn groups(world:&World,model:&FileModel,doc:&Document)->BTreeMap<u32,Vec<(String,Location)>>{
    let mut groups:BTreeMap<u32,Vec<(String,Location)>>=BTreeMap::new();
    let mut add=|line:u32,label:String,location:Location|{let items=groups.entry(line).or_default();if items.len()<1000&&!items.iter().any(|(_,old)|old==&location){items.push((label,location));}};
    for injection in &model.injections {for (path,bean) in world.candidates(injection){add(injection.range.start.line,format!("Bean: {}",describe(bean)),Location{path:path.into(),range:bean.range});}}
    for bean in &model.beans {for (location,into) in world.users(&doc.path,bean){add(bean.range.start.line,format!("Injected into: {into}"),location);}}
    let resolver=ui_kit::navigation::Resolver::new(world.files.iter().map(|(path,model)|(path.as_str(),&model.navigation)));
    let mut checked=resolver.checked(|path|if path==doc.path{Some(model.navigation.clone())}else{world.files.get(path).map(|model|model.navigation.clone())});
    for (key,range) in &model.navigation.markers {for location in checked.targets(key){add(range.start.line,format!("Related source: {key}"),location);}}
    groups.retain(|_,items|items.len()>1);groups
}
impl LanguageFeatures for Navigator {
    fn gutter_actions(&mut self,doc:&Document)->Vec<GutterAction>{
        if !matches!(doc.language.as_str(),"java"|"kotlin"|"xml")||doc.text.len()>128*1024{return vec![];}
        let Some(root)=host::project_of(&doc.path).and_then(|p|p.root().ok())else{return vec![];};
        let groups=self.with_doc(doc,|world,model|{
            let mut hashes:BTreeMap<String,[u8;32]>=BTreeMap::new();
            groups(world,model,doc).into_iter().filter_map(|(line,targets)|{
                let mut choices=Vec::new();
                for (label,location) in targets {
                    let target=if location.path==doc.path{Some(model)}else{world.files.get(&location.path)}?;
                    let hash=*hashes.entry(location.path.clone()).or_insert_with(||digest(target));choices.push(Choice{label,location,model:hash});
                }Some((line,choices))
            }).collect::<Vec<_>>()
        });
        self.gutter.pending.retain(|_,pending|pending.doc.path!=doc.path);
        let mut actions=Vec::new();
        if self.gutter.pending.len()+groups.len().min(64)>64{self.gutter.pending.clear();}
        for (line,choices) in groups.into_iter().take(64){
            let Some(next)=self.gutter.next.checked_add(1)else{break;};self.gutter.next=next;
            let ticket=format!("spring-source:{next}:{}:{line}",doc.version);let count=choices.len();
            self.gutter.pending.insert(ticket.clone(),Pending{root:root.clone(),doc:doc.clone(),choices});
            actions.push(GutterAction{line,icon:Some("implemented".into()),tooltip:if count==1000 {"Choose Spring source target (1000 shown; bounded list)".into()}else{format!("Choose among {count} cached Spring source targets")},command:COMMAND.into(),argument:Some(ticket)});
        }actions
    }
}
fn current(ctx:&Invocation,pending:&Pending)->Result<openidea_plugin::Project,Error>{
    let editor=ctx.editor().ok_or(Error::Stale)?;let source=editor.document()?;
    let doc=Document{path:editor.path(),language:editor.language(),text:source.text,version:source.version};
    let project=ctx.project().ok_or(Error::Stale)?;
    if doc!=pending.doc||project.root()?!=pending.root||host::project_of(&doc.path).and_then(|p|p.root().ok()).as_deref()!=Some(pending.root.as_str()){return Err(Error::Stale);}Ok(project)
}
impl Navigator {
    pub(crate) fn choose_source_gutter_target(&mut self,ctx:&Invocation,argument:Option<&str>)->Result<Option<String>,Error>{
        let ticket=argument.ok_or("Use this chooser from a Spring source gutter icon.")?;
        let pending=self.gutter.pending.get(ticket).cloned().ok_or(Error::Stale)?;
        let project=current(ctx,&pending)?;let mut snapshots=BTreeMap::new();
        for choice in &pending.choices {
            let path=&choice.location.path;
            if host::project_of(path).and_then(|p|p.root().ok()).as_deref()!=Some(pending.root.as_str()){return Err(Error::Stale);}
            let text=if path==&pending.doc.path{pending.doc.text.clone()}else{project.read_file(path)?};
            if text.len()>512*1024 || model::parse(path,&text).is_none_or(|model|digest(&model)!=choice.model){return Err(Error::Stale);}
            snapshots.insert(path.clone(),text);
        }
        let items:Vec<_>=pending.choices.iter().enumerate().map(|(i,choice)|openidea_plugin::PickItem::new(&i.to_string(),&choice.label).description(&format!("{}:{} · source candidate, effective runtime context unresolved",choice.location.path.strip_prefix(&format!("{}/",pending.root)).unwrap_or(&choice.location.path),choice.location.range.start.line+1))).collect();
        let Some(selected)=host::pick("Choose Spring source target",&items)? else{return Ok(None);};
        let choice=selected.parse::<usize>().ok().and_then(|i|pending.choices.get(i)).ok_or(Error::Stale)?;
        current(ctx,&pending)?;
        for (path,text) in &snapshots {if host::project_of(path).and_then(|p|p.root().ok()).as_deref()!=Some(pending.root.as_str())||path!=&pending.doc.path&&project.read_file(path)?!=*text{return Err(Error::Stale);}}
        self.gutter.pending.remove(ticket);
        host::open_location(&choice.location.path,choice.location.range.start.line,choice.location.range.start.character)?;Ok(None)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const SOURCE:&str="package shop;\n@org.springframework.stereotype.Service class Consumer {\n @org.springframework.beans.factory.annotation.Autowired ServiceType service;\n}";
    fn setup()->(MockHost,Navigator,Document){
        let host=MockHost::install().with_capabilities(&["project.read","editor.read"]);
        host.add_file("src/main/java/shop/Consumer.java",SOURCE);host.add_file("src/main/java/shop/ServiceType.java","package shop; interface ServiceType {}");
        host.add_file("src/main/java/shop/First.java","package shop; @org.springframework.stereotype.Service(\"first\") class First implements ServiceType {}");
        host.add_file("src/main/java/shop/Second.java","package shop; @org.springframework.stereotype.Service(\"second\") class Second implements ServiceType {}");
        host.open_editor("/project/src/main/java/shop/Consumer.java","java",SOURCE,&[(2,2,2,2)]);
        let editor=host.invocation().editor().unwrap();let source=editor.document().unwrap();let doc=Document{path:editor.path(),language:editor.language(),text:source.text,version:source.version};
        let mut navigator=Navigator::default();for project in host::projects().unwrap(){navigator.indexes.get(&project,COMMAND_BUDGET);} (host,navigator,doc)
    }
    #[test]
    fn ambiguous_injection_uses_native_chooser_without_first_target_marker(){
        let(host,mut navigator,doc)=setup();let actions=navigator.gutter_actions(&doc);let action=actions.iter().find(|a|a.line==2).unwrap();
        assert_eq!(action.command,COMMAND);assert!(action.tooltip.contains("2 cached"));assert!(navigator.line_markers(&doc).iter().all(|m|m.line!=2));
        host.answer_next_prompt(Some(&["1"]));navigator.run_command(COMMAND,&host.invocation(),action.argument.as_deref()).unwrap();assert_eq!(host.opened_locations().len(),1);assert!(host.opened_locations()[0].0.ends_with("/Second.java"));assert!(host.requests().is_empty()&&host.process_runs().is_empty());
    }
    #[test]
    fn stale_origin_target_model_or_project_never_opens_a_captured_target(){
        let(host,mut navigator,doc)=setup();let action=navigator.gutter_actions(&doc).into_iter().find(|a|a.line==2).unwrap();host.type_in_editor(" ");host.answer_next_prompt(Some(&["0"]));assert!(navigator.run_command(COMMAND,&host.invocation(),action.argument.as_deref()).is_err());assert!(host.opened_locations().is_empty());
        let(host,mut navigator,doc)=setup();let action=navigator.gutter_actions(&doc).into_iter().find(|a|a.line==2).unwrap();host.add_file("src/main/java/shop/First.java","package shop; @org.springframework.stereotype.Service(\"renamed\") class First implements ServiceType {}");host.answer_next_prompt(Some(&["0"]));assert!(navigator.run_command(COMMAND,&host.invocation(),action.argument.as_deref()).is_err());assert!(host.opened_locations().is_empty());
        let(host,mut navigator,doc)=setup();let action=navigator.gutter_actions(&doc).into_iter().find(|a|a.line==2).unwrap();host.set_project_root(Some("/other"));host.answer_next_prompt(Some(&["0"]));assert!(navigator.run_command(COMMAND,&host.invocation(),action.argument.as_deref()).is_err());assert!(host.opened_locations().is_empty());
    }
    #[test]
    fn canceled_native_picker_and_missing_ticket_leave_source_unchanged(){
        let(host,mut navigator,doc)=setup();let action=navigator.gutter_actions(&doc).into_iter().find(|a|a.line==2).unwrap();host.answer_next_prompt(None);assert!(navigator.run_command(COMMAND,&host.invocation(),action.argument.as_deref()).unwrap().is_none());assert!(host.opened_locations().is_empty());assert!(navigator.run_command(COMMAND,&host.invocation(),Some("forged")).is_err());
    }
}
