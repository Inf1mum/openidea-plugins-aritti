//! Long-running Artisan commands opened in terminal tabs. Browsing never starts PHP.
use crate::artisan_runner::Runner;
use openidea_plugin::{Error, Invocation, Project, host};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};

pub(super) const VIEW: &str = "laravel-processes";
#[derive(Default)]
pub(super) struct State { projects: BTreeMap<String, Session> }
#[derive(Default)]
struct Session { jobs: Vec<Launch>, seen: VecDeque<String>, preview: Option<Preview> }
#[derive(Clone)]
struct Preview { command: String, runner: Runner, argv: Vec<String>, executable: Vec<String>, token: String, target: Option<String> }
struct Launch { id: String, command: String, runner: Runner, argv: Vec<String>, target: Option<String> }
#[derive(Deserialize)]
#[serde(tag="op", rename_all="kebab-case", deny_unknown_fields)]
enum Request {
    State,
    Preview { command: String, #[serde(default)] runner: Runner, #[serde(default)] options: Options },
    Run { id: String, token: String },
}
#[derive(Default, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
struct Options { host: Option<String>, port: Option<u16>, connection: Option<String>, queue: Option<String> }
fn owner(ctx:&Invocation)->Result<(Project,String),Error> {
    let project=ctx.project().ok_or_else(||crate::ui_text::t("process.error0","Open a Laravel project first."))?;
    let root=project.root()?;
    if !crate::hover::canonical(&root) || !host::projects()?.iter().any(|p|p.root().ok().as_deref()==Some(&root)) {return Err(Error::Stale);}
    let path=format!("{}/artisan",root.trim_end_matches('/'));
    if host::project_of(&path).and_then(|p|p.root().ok()).as_deref()!=Some(&root) {return Err(Error::Stale);}
    Ok((project,root))
}
fn short(value:&str)->bool { !value.is_empty() && !value.starts_with('-') && value.len()<=128 && value.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'_'|b'-'|b'.'|b',')) }
fn args(command:&str,options:&Options)->Result<Vec<String>,Error> {
    let mut args=vec!["artisan".into(),command.into()];
    match command {
        "serve" => {
            if options.connection.is_some()||options.queue.is_some() {return Err(Error::Invalid(crate::ui_text::t("process.error1","Queue options do not apply to serve.")));}
            let host=options.host.as_deref().unwrap_or("127.0.0.1");
            if !short(host)||host.contains(',') {return Err(Error::Invalid(crate::ui_text::t("process.error2","Enter an IPv4 address or hostname.")));}
            let port=options.port.unwrap_or(8000);if port==0 {return Err(Error::Invalid(crate::ui_text::t("process.error3","Port must be between 1 and 65535.")));}
            args.extend([format!("--host={host}"),format!("--port={port}")]);
        },
        "queue:work" => {
            if options.host.is_some()||options.port.is_some(){return Err(Error::Invalid(crate::ui_text::t("process.error4","Host and port do not apply to workers.")));}
            if let Some(connection)=&options.connection {if !short(connection)||connection.contains(','){return Err(Error::Invalid(crate::ui_text::t("process.error5","Invalid connection name.")));} args.push(connection.clone());}
            if let Some(queue)=&options.queue {if !short(queue){return Err(Error::Invalid(crate::ui_text::t("process.error6","Invalid queue names.")));}args.push(format!("--queue={queue}"));}
            args.push("--no-interaction".into());
        },
        "schedule:work" => if options.host.is_some()||options.port.is_some()||options.connection.is_some()||options.queue.is_some(){return Err(Error::Invalid(crate::ui_text::t("process.error7","Schedule worker accepts no options here.")));},
        _=>return Err(Error::Invalid(crate::ui_text::t("process.error8","Choose serve, queue:work or schedule:work."))),
    }
    Ok(args)
}
fn preview(project:&Project,root:&str,command:&str,runner:Runner,argv:Vec<String>)->Result<Preview,Error> {
    runner.verify(project)?;
    let artisan=project.read_file("artisan")?;
    if artisan.len()>64*1024 {return Err(Error::Invalid(crate::ui_text::t("process.error9","Artisan entry point is too large.")));}
    let executable=runner.argv(project,&argv,false)?;
    let target=if runner==Runner::Docker {crate::artisan_docker::target_label(project)?} else {None};
    let token=crate::artisan_metadata::hash(&json!([root,runner.name(),executable,target,artisan]).to_string());
    Ok(Preview{command:command.into(),runner,argv,executable,token,target})
}
impl Launch {
    fn json(&self)->Value {json!({"id":self.id,"command":self.command,"runner":self.runner.name(),"argv":self.argv,"targetLabel":self.target})}
}
impl State {
    pub fn handle(&mut self,ctx:&Invocation,message:&str)->Result<Value,Error> {
        if message.len()>8192{return Err(Error::Invalid(crate::ui_text::t("process.error10","Process request is too large.")));}
        let request:Request=serde_json::from_str(message).map_err(|e|Error::Invalid(e.to_string()))?;
        let(project,root)=owner(ctx)?;
        if !self.projects.contains_key(&root)&&self.projects.len()>=8{return Err(Error::Invalid(crate::ui_text::t("process.error11","Too many process sessions; stop processes and reload the plugin.")));}
        let session=self.projects.entry(root.clone()).or_default();
        let mut response=json!({"root":root,"sessionOnly":true});
        match request {
            Request::State=>{},
            Request::Preview{command,runner,options}=>{
                session.preview=None;
                let draft=preview(&project,&root,&command,runner,args(&command,&options)?)?;
                response["preview"]=json!({"token":draft.token,"program":runner.name(),"argv":draft.executable,"targetLabel":draft.target});
                session.preview=Some(draft);
            },
            Request::Run{id,token}=>{
                if id.len()<16||id.len()>128||!id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'){return Err(Error::Invalid(crate::ui_text::t("process.error12","Invalid process operation id.")));}
                if session.seen.contains(&id){return Err(Error::Invalid(crate::ui_text::t("process.error13","This start request was already used.")));}
                let draft=session.preview.as_ref().filter(|p|p.token==token).ok_or(Error::Stale)?.clone();
                if preview(&project,&root,&draft.command,draft.runner,draft.argv.clone())?.token!=token{session.preview=None;return Err(Error::Stale);}
                let command=host::Command::new(draft.runner.name()).args(&draft.executable);
                host::open_terminal_session(&format!("laravel-process-{id}"),&format!("artisan {}",draft.command),&command)?;
                session.preview=None;
                session.seen.push_back(id.clone());if session.seen.len()>128{session.seen.pop_front();}
                if session.jobs.len()>=12 {session.jobs.remove(0);}
                session.jobs.push(Launch{id,command:draft.command,runner:draft.runner,argv:draft.argv,target:draft.target});
            },
        }
        response["jobs"]=json!(session.jobs.iter().map(Launch::json).collect::<Vec<_>>());
        Ok(response)
    }
}
pub(super) fn html()->String {r#"<section data-managed-processes aria-label="Managed Laravel processes" data-i18n-aria-label="process.panel"><style>[data-managed-processes]{padding:12px;display:grid;gap:12px;min-width:0}[data-managed-processes] [hidden]{display:none!important}[data-managed-processes] form{display:flex;gap:8px;align-items:end;flex-wrap:wrap}[data-managed-processes] label{display:grid;gap:4px}[data-managed-processes] pre{white-space:pre-wrap;overflow-wrap:anywhere;font-family:var(--ui-font-mono,monospace);max-height:240px;overflow:auto}[data-managed-processes] article{border-top:1px solid var(--ui-border,#80808055);padding-top:8px}[data-managed-processes] .process-output{display:grid;gap:4px;min-width:0}[data-managed-processes] .process-actions{display:flex;flex-wrap:wrap;gap:var(--ui-space-2,8px)}[data-managed-processes] .process-stop{margin-inline-start:var(--ui-space-2,8px)}</style><p><span data-i18n="process.terminalHelp">Commands open in a terminal tab. Stop or restart them from that tab.</span></p><form autocomplete="off"><label><span data-i18n="process.command">Command</span><select id="process-command"><option value="serve" data-i18n="process.developmentServer">Development server</option><option value="queue:work" data-i18n="process.queueWorker">Queue worker</option><option value="schedule:work" data-i18n="process.scheduleWorker">Schedule worker</option></select></label><label><span data-i18n="process.runner">Runner</span><select id="process-runner"><option value="php" data-i18n="process.localPhp">Local PHP</option><option value="docker" data-i18n="process.dockerTarget">Selected Docker target</option></select></label><label data-process-serve><span data-i18n="process.host">Host</span><input id="process-host" value="127.0.0.1"></label><label data-process-serve><span data-i18n="process.port">Port</span><input id="process-port" type="number" min="1" max="65535" value="8000"></label><label data-process-queue hidden><span data-i18n="process.connection">Connection</span><input id="process-connection" placeholder="Application default" data-i18n-placeholder="process.applicationDefault"></label><label data-process-queue hidden><span data-i18n="process.queues">Queues</span><input id="process-queue" placeholder="Application default" data-i18n-placeholder="process.applicationDefault"></label></form><div class="process-actions"><button type="button" id="process-preview" class="ui-button"><span data-ui-icon="eye" aria-hidden="true"></span><span data-i18n="process.preview">Preview</span></button><button type="button" id="process-start" class="ui-button" disabled><span data-ui-icon="run" aria-hidden="true"></span><span data-i18n="process.start">Start</span></button></div><pre id="process-argv" aria-label="Exact process command" data-i18n-aria-label="process.exactCommand"></pre><p id="process-error" role="alert"></p></section>"#.into()}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn setup()->MockHost {let host=MockHost::install().with_capabilities(&["project.read","process:php"]);host.add_file("artisan","<?php // artisan");host}
    fn send(state:&mut State,host:&MockHost,v:Value)->Result<Value,Error>{state.handle(&host.invocation(),&v.to_string())}
    fn token(state:&mut State,host:&MockHost,command:&str)->Value {send(state,host,json!({"op":"preview","command":command})).unwrap()["preview"]["token"].clone()}
    #[test] fn browsing_and_preview_do_not_execute_and_options_are_exact(){let host=setup();let mut state=State::default();send(&mut state,&host,json!({"op":"state"})).unwrap();let reply=send(&mut state,&host,json!({"op":"preview","command":"queue:work","options":{"connection":"redis","queue":"high,default"}})).unwrap();assert_eq!(reply["preview"]["argv"],json!(["artisan","queue:work","redis","--queue=high,default","--no-interaction"]));assert!(host.process_runs().is_empty());assert!(host.terminal_writes().is_empty());}
    #[test] fn start_opens_a_terminal_tab_and_rejects_duplicate_submission(){let host=setup();let mut state=State::default();let token=token(&mut state,&host,"serve");let request=json!({"op":"run","id":"process-123456789","token":token});let reply=send(&mut state,&host,request.clone()).unwrap();assert_eq!(reply["jobs"][0]["command"],"serve");assert!(host.process_runs().is_empty());assert!(send(&mut state,&host,request).is_err());}
    #[test] fn stale_entrypoint_and_invalid_options_never_start(){let host=setup();let mut state=State::default();let token=token(&mut state,&host,"serve");host.add_file("artisan","<?php changed");assert!(send(&mut state,&host,json!({"op":"run","id":"process-123456789","token":token})).is_err());assert!(send(&mut state,&host,json!({"op":"preview","command":"queue:work","options":{"connection":"--evil"}})).is_err());assert!(send(&mut state,&host,json!({"op":"preview","command":"schedule:work","options":{"port":8000}})).is_err());assert!(host.process_runs().is_empty());}
    #[test] fn denied_terminal_is_reported(){let host=MockHost::install().with_capabilities(&["project.read"]);host.add_file("artisan","<?php");let mut state=State::default();let token=token(&mut state,&host,"serve");assert!(matches!(send(&mut state,&host,json!({"op":"run","id":"process-123456789","token":token})),Err(Error::Denied(_))));}
    #[test] fn running_workers_are_not_tracked_by_the_plugin(){let host=setup();let mut state=State::default();let token=token(&mut state,&host,"queue:work");let reply=send(&mut state,&host,json!({"op":"run","id":"process-123456789","token":token})).unwrap();assert!(reply["jobs"][0].get("running").is_none());assert!(send(&mut state,&host,json!({"op":"stop","id":"process-123456789"})).is_err());}
}
