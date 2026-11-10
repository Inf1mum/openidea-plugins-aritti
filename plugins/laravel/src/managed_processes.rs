//! Session-owned long-running Artisan jobs. Browsing never starts PHP.
use crate::artisan_runner::Runner;
use openidea_plugin::{Error, Invocation, Project, host};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

pub(super) const VIEW: &str = "laravel-processes";
#[derive(Default)]
pub(super) struct State { projects: BTreeMap<String, Session> }
#[derive(Default)]
struct Session { jobs: Vec<Process>, seen: VecDeque<String>, preview: Option<Preview> }
#[derive(Clone)]
struct Preview { command: String, runner: Runner, argv: Vec<String>, executable: Vec<String>, token: String, target: Option<String> }
struct Process {
    child: Option<host::Child>, id: String, command: String, runner: Runner,
    argv: Vec<String>, target: Option<String>, stdout: String, stderr: String,
    status: Option<i32>, stopped: bool, truncated: bool, started: Instant,
}
#[derive(Deserialize)]
#[serde(tag="op", rename_all="kebab-case", deny_unknown_fields)]
enum Request {
    State,
    Preview { command: String, #[serde(default)] runner: Runner, #[serde(default)] options: Options },
    Run { id: String, token: String },
    Poll,
    Stop { id: String },
}
#[derive(Default, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
struct Options { host: Option<String>, port: Option<u16>, connection: Option<String>, queue: Option<String> }
fn owner(ctx:&Invocation)->Result<(Project,String),Error> {
    let project=ctx.project().ok_or("Open a Laravel project first.")?;
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
            if options.connection.is_some()||options.queue.is_some() {return Err(Error::Invalid("Queue options do not apply to serve.".into()));}
            let host=options.host.as_deref().unwrap_or("127.0.0.1");
            if !short(host)||host.contains(',') {return Err(Error::Invalid("Enter an IPv4 address or hostname.".into()));}
            let port=options.port.unwrap_or(8000);if port==0 {return Err(Error::Invalid("Port must be between 1 and 65535.".into()));}
            args.extend([format!("--host={host}"),format!("--port={port}")]);
        },
        "queue:work" => {
            if options.host.is_some()||options.port.is_some(){return Err(Error::Invalid("Host and port do not apply to workers.".into()));}
            if let Some(connection)=&options.connection {if !short(connection)||connection.contains(','){return Err(Error::Invalid("Invalid connection name.".into()));} args.push(connection.clone());}
            if let Some(queue)=&options.queue {if !short(queue){return Err(Error::Invalid("Invalid queue names.".into()));}args.push(format!("--queue={queue}"));}
            args.push("--no-interaction".into());
        },
        "schedule:work" => if options.host.is_some()||options.port.is_some()||options.connection.is_some()||options.queue.is_some(){return Err(Error::Invalid("Schedule worker accepts no options here.".into()));},
        _=>return Err(Error::Invalid("Choose serve, queue:work or schedule:work.".into())),
    }
    Ok(args)
}
fn preview(project:&Project,root:&str,command:&str,runner:Runner,argv:Vec<String>)->Result<Preview,Error> {
    runner.verify(project)?;
    let artisan=project.read_file("artisan")?;
    if artisan.len()>64*1024 {return Err(Error::Invalid("Artisan entry point is too large.".into()));}
    let executable=runner.argv(project,&argv,false)?;
    let target=if runner==Runner::Docker {crate::artisan_docker::target_label(project)?} else {None};
    let token=crate::artisan_metadata::hash(&json!([root,runner.name(),executable,target,artisan]).to_string());
    Ok(Preview{command:command.into(),runner,argv,executable,token,target})
}
impl Process {
    fn poll(&mut self,stop:bool)->Result<(),Error> {
        let Some(child)=&self.child else{return Ok(())};
        if stop {child.kill()?;self.stopped=true;}
        // Drain bounded chunks even when the child has exited; release only at EOF.
        for _ in 0..8 {
            let chunk=match self.child.as_ref().unwrap().poll(16*1024){Ok(c)=>c,Err(error)=>{let _=self.child.as_ref().unwrap().kill();self.child=None;self.status=Some(-1);return Err(error)}};
            let empty=chunk.stdout.is_empty()&&chunk.stderr.is_empty();
            append(&mut self.stdout,&chunk.stdout,64*1024,&mut self.truncated);
            append(&mut self.stderr,&chunk.stderr,16*1024,&mut self.truncated);
            if let Some(status)=chunk.status {self.status=Some(status);}
            if self.status.is_some()&&empty {self.child=None;break;}
            if self.status.is_none(){break;}
        }
        Ok(())
    }
    fn json(&self)->Value {json!({"id":self.id,"command":self.command,"runner":self.runner.name(),"argv":self.argv,"targetLabel":self.target,"running":self.child.is_some(),"status":self.status,"stopped":self.stopped,"stdout":self.stdout,"stderr":self.stderr,"truncated":self.truncated,"elapsedMs":self.started.elapsed().as_millis()})}
}
fn append(out:&mut String,value:&str,limit:usize,truncated:&mut bool){let mut end=value.len().min(limit.saturating_sub(out.len()));while !value.is_char_boundary(end){end-=1;}out.push_str(&value[..end]);*truncated|=end<value.len();}
impl State {
    pub fn handle(&mut self,ctx:&Invocation,message:&str)->Result<Value,Error> {
        if message.len()>8192{return Err(Error::Invalid("Process request is too large.".into()));}
        let request:Request=serde_json::from_str(message).map_err(|e|Error::Invalid(e.to_string()))?;
        let(project,root)=owner(ctx)?;
        if !self.projects.contains_key(&root)&&self.projects.len()>=8{return Err(Error::Invalid("Too many process sessions; stop processes and reload the plugin.".into()));}
        let active=self.projects.values().flat_map(|s|&s.jobs).filter(|j|j.child.is_some()).count();
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
                if id.len()<16||id.len()>128||!id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'){return Err(Error::Invalid("Invalid process operation id.".into()));}
                if session.seen.contains(&id){return Err(Error::Invalid("This start request was already used.".into()));}
                if active>=6||session.jobs.iter().filter(|j|j.child.is_some()).count()>=4{return Err(Error::Invalid("Stop an existing process before starting another.".into()));}
                let draft=session.preview.as_ref().filter(|p|p.token==token).ok_or(Error::Stale)?.clone();
                if preview(&project,&root,&draft.command,draft.runner,draft.argv.clone())?.token!=token{session.preview=None;return Err(Error::Stale);}
                if session.jobs.iter().any(|j|j.child.is_some()&&j.runner==draft.runner&&j.argv==draft.argv&&j.target==draft.target){return Err(Error::Invalid("This process is already running.".into()));}
                let child=host::start(&host::Command::new(draft.runner.name()).args(&draft.executable),Some(&project))?;
                if let Err(error)=child.close_stdin(){let _=child.kill();return Err(error);}
                session.preview=None;
                session.seen.push_back(id.clone());if session.seen.len()>128{session.seen.pop_front();}
                if session.jobs.len()>=12 {if let Some(at)=session.jobs.iter().position(|j|j.child.is_none()){session.jobs.remove(at);}}
                session.jobs.push(Process{child:Some(child),id,command:draft.command,runner:draft.runner,argv:draft.argv,target:draft.target,stdout:String::new(),stderr:String::new(),status:None,stopped:false,truncated:false,started:Instant::now()});
            },
            Request::Poll=>{for job in &mut session.jobs {job.poll(false)?;}},
            Request::Stop{id}=>{session.jobs.iter_mut().find(|j|j.id==id).ok_or(Error::Stale)?.poll(true)?;},
        }
        response["jobs"]=json!(session.jobs.iter().map(Process::json).collect::<Vec<_>>());
        Ok(response)
    }
}
pub(super) fn html()->String {r#"<section data-managed-processes aria-label="Managed Laravel processes"><style>[data-managed-processes]{padding:12px;display:grid;gap:12px;min-width:0}[data-managed-processes] [hidden]{display:none!important}[data-managed-processes] form{display:flex;gap:8px;align-items:end;flex-wrap:wrap}[data-managed-processes] label{display:grid;gap:4px}[data-managed-processes] pre{white-space:pre-wrap;overflow-wrap:anywhere;font-family:var(--ui-font-mono,monospace);max-height:240px;overflow:auto}[data-managed-processes] article{border-top:1px solid var(--ui-border,#80808055);padding-top:8px}[data-managed-processes] .process-output{display:grid;gap:4px;min-width:0}[data-managed-processes] .process-actions{display:flex;flex-wrap:wrap;gap:var(--ui-space-2,8px)}[data-managed-processes] .process-stop{margin-inline-start:var(--ui-space-2,8px)}</style><p>Processes are managed for this plugin session. They do not restart automatically or persist after reload. Stop them before closing the project.</p><form autocomplete="off"><label>Command<select id="process-command"><option value="serve">Development server</option><option value="queue:work">Queue worker</option><option value="schedule:work">Schedule worker</option></select></label><label>Runner<select id="process-runner"><option value="php">Local PHP</option><option value="docker">Selected Docker target</option></select></label><label data-process-serve>Host<input id="process-host" value="127.0.0.1"></label><label data-process-serve>Port<input id="process-port" type="number" min="1" max="65535" value="8000"></label><label data-process-queue hidden>Connection<input id="process-connection" placeholder="Application default"></label><label data-process-queue hidden>Queues<input id="process-queue" placeholder="Application default"></label></form><div class="process-actions"><button type="button" id="process-preview" class="ui-button"><span data-ui-icon="eye" aria-hidden="true"></span>Preview</button><button type="button" id="process-start" class="ui-button" disabled><span data-ui-icon="run" aria-hidden="true"></span>Start</button><button type="button" id="process-refresh" class="ui-button"><span data-ui-icon="refresh" aria-hidden="true"></span>Refresh</button></div><pre id="process-argv" aria-label="Exact process command"></pre><p id="process-error" role="alert"></p><div id="process-jobs" aria-live="polite"></div></section>"#.into()}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn setup()->MockHost {let host=MockHost::install().with_capabilities(&["project.read","process:php"]);host.add_file("artisan","<?php // artisan");host}
    fn send(state:&mut State,host:&MockHost,v:Value)->Result<Value,Error>{state.handle(&host.invocation(),&v.to_string())}
    #[test] fn browsing_and_preview_do_not_execute_and_options_are_exact(){let host=setup();let mut state=State::default();send(&mut state,&host,json!({"op":"state"})).unwrap();let reply=send(&mut state,&host,json!({"op":"preview","command":"queue:work","options":{"connection":"redis","queue":"high,default"}})).unwrap();assert_eq!(reply["preview"]["argv"],json!(["artisan","queue:work","redis","--queue=high,default","--no-interaction"]));assert!(host.process_runs().is_empty());}
    #[test] fn start_poll_stop_and_duplicate_submission(){let host=setup();let mut state=State::default();let token=send(&mut state,&host,json!({"op":"preview","command":"serve"})).unwrap()["preview"]["token"].clone();let request=json!({"op":"run","id":"process-123456789","token":token});let reply=send(&mut state,&host,request.clone()).unwrap();assert_eq!(reply["jobs"][0]["running"],true);assert!(send(&mut state,&host,request).is_err());let reply=send(&mut state,&host,json!({"op":"poll"})).unwrap();assert!(reply["jobs"][0]["stdout"].as_str().unwrap().contains("started: php"));let reply=send(&mut state,&host,json!({"op":"stop","id":"process-123456789"})).unwrap();assert_eq!(reply["jobs"][0]["running"],false);assert_eq!(reply["jobs"][0]["stopped"],true);send(&mut state,&host,json!({"op":"stop","id":"process-123456789"})).unwrap();}
    #[test] fn stale_entrypoint_and_invalid_options_never_start(){let host=setup();let mut state=State::default();let token=send(&mut state,&host,json!({"op":"preview","command":"serve"})).unwrap()["preview"]["token"].clone();host.add_file("artisan","<?php changed");assert!(send(&mut state,&host,json!({"op":"run","id":"process-123456789","token":token})).is_err());assert!(send(&mut state,&host,json!({"op":"preview","command":"queue:work","options":{"connection":"--evil"}})).is_err());assert!(send(&mut state,&host,json!({"op":"preview","command":"schedule:work","options":{"port":8000}})).is_err());assert!(host.process_runs().is_empty());}
    #[test] fn denied_execution_is_reported_and_foreign_ids_cannot_stop(){let host=MockHost::install().with_capabilities(&["project.read"]);host.add_file("artisan","<?php");let mut state=State::default();let token=send(&mut state,&host,json!({"op":"preview","command":"serve"})).unwrap()["preview"]["token"].clone();assert!(matches!(send(&mut state,&host,json!({"op":"run","id":"process-123456789","token":token})),Err(Error::Denied(_))));assert!(send(&mut state,&host,json!({"op":"stop","id":"foreign"})).is_err());}
    #[test] fn long_running_worker_has_no_artisan_command_timeout(){let host=setup();let mut state=State::default();let token=send(&mut state,&host,json!({"op":"preview","command":"queue:work"})).unwrap()["preview"]["token"].clone();send(&mut state,&host,json!({"op":"run","id":"process-123456789","token":token})).unwrap();let root=host.invocation().project().unwrap().root().unwrap();state.projects.get_mut(&root).unwrap().jobs[0].started=Instant::now()-std::time::Duration::from_secs(600);let reply=send(&mut state,&host,json!({"op":"poll"})).unwrap();assert_eq!(reply["jobs"][0]["running"],true);assert_eq!(reply["jobs"][0]["stopped"],false);}
    #[test] fn clipping_is_bounded_and_unicode_safe(){let mut text=String::new();let mut truncated=false;append(&mut text,"a💡b",4,&mut truncated);assert_eq!(text,"a");assert!(truncated);}
}
