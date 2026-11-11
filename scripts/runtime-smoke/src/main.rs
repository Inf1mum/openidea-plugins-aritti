//! Isolated real-Wasm view smoke: no optional process/network/editor-write grants.
use std::{path::{Path,PathBuf},sync::{Arc,Mutex,Weak}};
use ide_host::{Host,HostConfig,plugins::{PluginHost,HostWorkspace,Consent,PluginEvent}};
use ide_plugin::{Invocation,Limits,EditorContext,ViewEvent,ProviderDocument,Prompt};
use sha2::{Digest,Sha256};

fn copy_package(source:&Path,target:&Path)->std::io::Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry=entry?;let name=entry.file_name();
        if ["target","dist",".git","node_modules","__pycache__"].iter().any(|v|name==*v){continue;}
        let to=target.join(&name);let kind=entry.file_type()?;
        if kind.is_symlink(){return Err(std::io::Error::other("source package contains a symlink"));}
        if kind.is_dir(){copy_package(&entry.path(),&to)?;}else{std::fs::copy(entry.path(),to)?;}
    }
    Ok(())
}

#[tokio::main]
async fn main()->Result<(),Box<dyn std::error::Error>> {
    let mut args=std::env::args().skip(1);
    let repo=PathBuf::from(args.next().ok_or("usage: openidea-parity-runtime-smoke REPO OUTPUT [PLUGIN ...]")?).canonicalize()?;
    let output=PathBuf::from(args.next().ok_or("missing output directory")?);
    let mut selected:Vec<_>=args.collect();
    let grant_project_read=selected.iter().any(|arg|arg=="--grant-project-read");
    selected.retain(|arg|arg!="--grant-project-read");
    let known:Vec<_>=std::fs::read_dir(repo.join("plugins"))?.filter_map(Result::ok).filter(|entry|entry.path().join("plugin.toml").exists()).map(|entry|entry.file_name().to_string_lossy().to_string()).collect();
    if selected.iter().any(|name|!known.contains(name)){return Err("unknown package selection".into());}
    if selected.iter().any(|p|p=="laravel"||p.contains('/')||p.contains('\\')||p=="..") {return Err("invalid/excluded package selection".into());}
    let output=std::path::absolute(output)?;
    if output==repo || repo.starts_with(&output) || ["plugins","crates","scripts",".git"].iter().any(|name|output.starts_with(repo.join(name))) {return Err("output overlaps repository sources".into());}
    std::fs::create_dir_all(&output)?;
    let temporary=tempfile::tempdir()?;
    let base=temporary.path().canonicalize()?;
    let project=base.join("project");std::fs::create_dir_all(&project)?;
    for (path,text) in [
        ("pom.xml","<project><modelVersion>4.0.0</modelVersion><groupId>fixture</groupId><artifactId>parity</artifactId><version>1</version><dependencies><dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-web</artifactId><version>3.5.0</version></dependency></dependencies></project>"),
        ("src/main/resources/application.properties","spring.application.name=parity\nserver.port=8080\nfixture.value=hello\n"),
        ("src/main/java/fixture/Controller.java","package fixture; import org.springframework.web.bind.annotation.*; @RestController class Controller { @GetMapping(\"/hello\") String hello(){return \"hello\";} }"),
        ("src/main/java/fixture/Person.java","package fixture; import jakarta.persistence.Entity; import jakarta.persistence.Id; import jakarta.persistence.ManyToOne; @Entity public class Person { @Id Long id; @ManyToOne Team team; }"),
        ("src/main/java/fixture/Team.java","package fixture; import jakarta.persistence.Entity; import jakarta.persistence.Id; @Entity public class Team { @Id Long id; }"),
        ("src/main/resources/messages.properties","hello=Hello\n"),
        ("src/main/resources/messages_fr.properties","hello=Bonjour\n"),
        ("package.json","{\"name\":\"parity-fixture\",\"version\":\"1.0.0\"}"),
    ] {
        let path=project.join(path);std::fs::create_dir_all(path.parent().unwrap())?;std::fs::write(path,text)?;
    }
    let undo_events=Arc::new(Mutex::new(Vec::<u32>::new()));
    let sink_events=undo_events.clone();
    let host=Arc::new(Host::new(HostConfig {
        config_dir:base.join("config"),data_dir:base.join("data"),local_data_dir:base.join("local"),
        languages:Arc::new(ide_language::LanguageRegistry::load(None)?),syntax:Arc::default(),
        sink_for:Box::new(move |_|{
            let events=sink_events.clone();
            Arc::new(move |event:ide_core::IdeEvent|{if let ide_core::IdeEvent::WorkspaceEditApplied{id,..}=event{events.lock().unwrap().push(id);}})
        }),owner:|w|w,
        runtime:tokio::runtime::Handle::current(),project_environment:false,
    }));
    host.trust.decide(&project,true,false)?;
    host.open_project("parity-smoke",&project)?;
    let controller=Arc::new(Mutex::new(Weak::<PluginHost>::new()));
    let prompt_controller=controller.clone();
    let plugins=Arc::new(PluginHost::new(base.join("plugins"),Arc::new(move |_:Option<&str>,event|{
        if let PluginEvent::PromptRequested{prompt_id,prompt,..}=event {
            let answer=match prompt {Prompt::Confirm{title,..} if title=="Apply staged locale grid?"=>Some(vec!["yes".into()]),_=>None};
            let weak=prompt_controller.lock().unwrap().clone();
            if let Some(plugins)=weak.upgrade(){plugins.answer_prompt(&prompt_id,answer);}
        }
    }),Limits::default())?);
    *controller.lock().unwrap()=Arc::downgrade(&plugins);
    plugins.set_workspace(Some(Arc::new(HostWorkspace(Arc::downgrade(&host)))));
    host.set_plugin_host(&plugins);
    plugins.load_all();plugins.startup_finished();
    let mut records=Vec::new();
    let mut dirs:Vec<_>=std::fs::read_dir(repo.join("plugins"))?.filter_map(Result::ok).map(|e|e.path()).filter(|p|p.join("plugin.toml").exists()).collect();dirs.sort();
    for source in dirs {
        let name=source.file_name().unwrap().to_string_lossy().to_string();
        if name=="laravel"||!selected.is_empty()&&!selected.contains(&name){continue;}
        let manifest=ide_extension::Manifest::load(&source)?;
        let mut views=manifest.contributes.get("views").and_then(|v|v.as_array()).cloned().unwrap_or_default();
        views.extend(manifest.contributes.get("customEditors").and_then(|v|v.as_array()).into_iter().flatten().filter(|view|view.get("web").is_none()).cloned());
        if views.is_empty(){records.push(serde_json::json!({"plugin":name,"status":"no-views"}));continue;}
        if manifest.capabilities.required.iter().any(|c|c.to_string().starts_with("net:")||c.to_string().starts_with("process:")) {
            records.push(serde_json::json!({"plugin":name,"status":"skipped-required-external-authority"}));continue;
        }
        let staged=base.join("packages").join(&name);copy_package(&source,&staged)?;
        let wasm_dir=source.join("target/wasm32-wasip2/release");
        let artifacts:Vec<_>=std::fs::read_dir(&wasm_dir)?.filter_map(Result::ok).map(|e|e.path()).filter(|p|p.extension().is_some_and(|e|e=="wasm")&&p.file_name().is_some_and(|n|n.to_string_lossy().starts_with("openidea_plugin_"))).collect();
        if artifacts.len()!=1{return Err(format!("{name}: expected one canonical fresh Wasm").into());}
        let relative=manifest.wasm.as_ref().ok_or("missing manifest Wasm")?;
        if relative.is_absolute()||relative.components().any(|c|matches!(c,std::path::Component::ParentDir)){return Err("unsafe Wasm path".into());}
        let destination=staged.join(relative);std::fs::create_dir_all(destination.parent().unwrap())?;std::fs::copy(&artifacts[0],&destination)?;
        let wasm_sha256=format!("{:x}",Sha256::digest(std::fs::read(&destination)?));
        if wasm_sha256!=format!("{:x}",Sha256::digest(std::fs::read(&artifacts[0])?)){return Err("Wasm changed while staging".into());}
        let consent=Consent{optional:if grant_project_read{vec!["project.read".into()]}else{vec![]},..Default::default()};
        plugins.install_from_dir(&staged,&consent)?;
        records.push(serde_json::json!({"plugin":name,"status":"installed","wasm_sha256":wasm_sha256,"wasm":artifacts[0]}));
        let editor=if name=="properties" {
            let path=project.join("src/main/resources/messages.properties");
            host.project("parity-smoke")?.documents.open(&path)?;
            Some(EditorContext{path,language:"properties".into(),selections:vec![]})
        }else{None};
        for view in views {
            let id=view.get("id").and_then(|v|v.as_str()).ok_or("view missing id")?;
            if view.get("when").is_some(){records.push(serde_json::json!({"plugin":name,"view":id,"status":"skipped-conditional"}));continue;}
            let invocation=Invocation{client:Some("parity-smoke".into()),workspace_root:Some(project.clone()),editor:editor.clone(),..Default::default()};
            let result=plugins.render_view(&manifest.id,id,None,invocation.clone());
            match result {
                Ok(model)=>{
                    std::fs::write(output.join(format!("{name}-{id}.json")),serde_json::to_vec_pretty(&model)?)?;
                    records.push(serde_json::json!({"plugin":name,"view":id,"status":"rendered"}));println!("{name}/{id}: rendered");
                    if name=="properties"&&id=="bundle" {
                        let before=std::fs::read(project.join("src/main/resources/messages_fr.properties"))?;
                        let base_before=std::fs::read(project.join("src/main/resources/messages.properties"))?;
                        plugins.view_event(&manifest.id,id,ViewEvent::Button{id:"bundle-stage-start".into()},invocation.clone())?;
                        plugins.view_event(&manifest.id,id,ViewEvent::Cell{row:0,column:2,value:"PARITY-STAGED-FR-123".into()},invocation.clone())?;
                        let staged=plugins.render_view(&manifest.id,id,None,invocation.clone())?;
                        let json=serde_json::to_value(&staged)?;
                        if !json.to_string().contains("PARITY-STAGED-FR-123") {return Err("staged locale cell is absent from rendered model".into());}
                        if before!=std::fs::read(project.join("src/main/resources/messages_fr.properties"))? || base_before!=std::fs::read(project.join("src/main/resources/messages.properties"))? {return Err("grid mutated a source before explicit atomic apply".into());}
                        std::fs::write(output.join("properties-bundle-staged.json"),serde_json::to_vec_pretty(&json)?)?;
                        records.push(serde_json::json!({"plugin":name,"view":id,"status":"staged-cell-source-unchanged"}));
                        let documents=&host.project("parity-smoke")?.documents;
                        let info=documents.open(&project.join("src/main/resources/messages.properties"))?;
                        let doc=ProviderDocument{path:info.path.to_path_buf(),language:"properties".into(),version:info.version as u64,text:info.text.clone()};
                        let actions=plugins.provide_code_actions(&doc,ide_core::Range::default());
                        let (_,action)=actions.into_iter().find(|(owner,action)|owner==&manifest.id&&action.title.starts_with("Apply staged bundle changes")).ok_or("real host did not dispatch staged FileAction")?;
                        let resolved=plugins.resolve_code_action(&manifest.id,&doc,action,invocation.clone())?;
                        if !resolved.operations.is_empty()||resolved.edits.is_empty(){return Err("unexpected grid action operations".into());}
                        let mut edit=ide_core::WorkspaceEdit{files:resolved.edits.into_iter().map(|(path,edits)|ide_core::FileEdit::Edit{path:path.into(),edits,version:None}).collect(),..Default::default()};
                        documents.prepare_workspace_edit(&mut edit)?;
                        documents.apply_workspace_edit(&edit)?;
                        let translated=documents.open(&project.join("src/main/resources/messages_fr.properties"))?;
                        if !translated.text.contains("PARITY-STAGED-FR-123"){return Err("native workspace edit did not apply staged translation".into());}
                        let undo_id=*undo_events.lock().unwrap().last().ok_or("missing native grouped Undo record")?;
                        documents.undo_workspace_edit(undo_id)?;
                        if documents.snapshot(translated.id)?.text!=String::from_utf8(before)?||documents.snapshot(info.id)?.text!=String::from_utf8(base_before)?{return Err("native grouped Undo did not restore both locale buffers".into());}
                        records.push(serde_json::json!({"plugin":name,"view":id,"status":"native-file-action-apply-undo","undo_id":undo_id}));
                    }
                },
                Err(error)=>{records.push(serde_json::json!({"plugin":name,"view":id,"status":"error","error":error.to_string()}));println!("{name}/{id}: {error}");},
            }
        }
    }
    std::fs::write(output.join("results.json"),serde_json::to_vec_pretty(&serde_json::json!({"scope":"actual released host/guest render with synthetic project; no GUI or external grants","optional_project_read":grant_project_read,"records":records}))?)?;
    if records.iter().any(|r|r["status"]=="error"){std::process::exit(1);}
    Ok(())
}
