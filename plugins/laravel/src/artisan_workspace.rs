//! Project-bound command forms; rendering never discovers commands or executes PHP.
use crate::{artisan_execution::Job, artisan_metadata as metadata};
use openidea_plugin::{Error, Invocation, Project, host};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};

pub(super) const VIEW: &str = "laravel-artisan";
/// Linked packages retain their versioned web URL between builds. Version the
/// dynamic stylesheet too, so a refreshed view cannot reuse an older layout.
pub(super) fn html() -> String {
    let revision=metadata::hash(include_str!("../../../crates/ui-kit/web/command-workspace.css"));
    let components=metadata::hash(include_str!("../../../crates/ui-kit/web/components.css"));
    let html=include_str!("../../../crates/ui-kit/web/command-workspace.html").replace("./command-workspace.css",&format!("./command-workspace.css?v={revision}"));
    format!("<link rel=\"stylesheet\" data-ui-components href=\"./components.css?v={components}\">{html}")
}

#[derive(Default)]
pub(super) struct State {
    projects: BTreeMap<String, Session>,
}
#[derive(Default)]
struct Session {
    job: Option<Job>,
    history: Vec<Value>,
    seen: VecDeque<String>,
    selection: Option<String>,
    runner: crate::artisan_runner::Runner,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
enum Request {
    State,
    Refresh,
    DockerTarget,
    Runner { runner: crate::artisan_runner::Runner },
    Preview {
        command: String,
        token: String,
        values: BTreeMap<String, Value>,
        revision: u32,
        #[serde(default,rename="additionalOptions")] additional_options:String,
        #[serde(default,rename="additionalArguments")] additional_arguments:String,
    },
    CopyOutput {
        id: String,
    },
    Run {
        id: String,
        command: String,
        token: String,
        values: BTreeMap<String, Value>,
        #[serde(default)]
        terminal: bool,
        #[serde(default,rename="additionalOptions")] additional_options:String,
        #[serde(default,rename="additionalArguments")] additional_arguments:String,
    },
    Poll {
        id: String,
    },
    Cancel {
        id: String,
    },
}
fn owner(ctx: &Invocation) -> Result<(Project, String), Error> {
    let project = ctx.project().ok_or("Open a Laravel project first.")?;
    let root = project.root()?;
    if !crate::hover::canonical(&root)
        || !host::projects()?
            .iter()
            .any(|p| p.root().ok().as_deref() == Some(root.as_str()))
    {
        return Err(Error::Stale);
    }
    Ok((project, root))
}
#[cfg(test)]
fn catalog(project: &Project) -> Result<(Value, String), Error> {
    crate::artisan_runner::Runner::Php.catalog(project)
}
impl State {
    pub fn runner(&self, ctx: &Invocation) -> Result<crate::artisan_runner::Runner, Error> {
        let (_,root)=owner(ctx)?;
        Ok(self.projects.get(&root).map(|session|session.runner).unwrap_or_default())
    }

    pub(crate) fn runner_for_project(&self, project: &Project) -> Result<crate::artisan_runner::Runner, Error> {
        Ok(self.projects.get(&project.root()?).map(|session|session.runner).unwrap_or_default())
    }

    /// Uses already captured definitions only; opening a generator never runs PHP.
    /// False preserves the static guided generator fallback when no catalog exists.
    pub fn open_generator(&mut self, ctx: &Invocation, fixed: Option<&str>) -> Result<bool, Error> {
        let (_, root) = owner(ctx)?;
        let project = ctx.project().ok_or(Error::Stale)?;
        let runner = self.projects.get(&root).map(|session| session.runner).unwrap_or_default();
        let (snapshot, token) = match runner.catalog(&project) {
            Ok(catalog) => catalog,
            Err(error) if runner==crate::artisan_runner::Runner::Docker => return Err(error),
            Err(_) => return Ok(false),
        };
        let commands: Vec<_> = snapshot["commands"].as_array().into_iter().flatten()
            .filter(|command| command["name"].as_str().is_some_and(|name| name.starts_with("make:")))
            .collect();
        if commands.is_empty() { return if runner==crate::artisan_runner::Runner::Docker {Err(Error::Invalid("The captured Docker target has no make commands.".into()))} else {Ok(false)}; }
        let chosen = if let Some(name) = fixed {
            if !commands.iter().any(|command| command["name"] == name) { return if runner==crate::artisan_runner::Runner::Docker {Err(Error::Invalid("This generator is not in the captured Docker catalog.".into()))} else {Ok(false)}; }
            name.to_owned()
        } else {
            let items: Vec<_> = commands.iter().filter_map(|command| {
                let name = command["name"].as_str()?;
                Some(openidea_plugin::PickItem::new(name, name).description(command["description"].as_str().unwrap_or("")))
            }).collect();
            let Some(chosen) = host::pick("Generate with Artisan", &items)? else { return Ok(true) };
            if !commands.iter().any(|command| command["name"] == chosen) { return Err(Error::Stale); }
            chosen
        };
        if !self.projects.contains_key(&root) && self.projects.len() >= 8 {
            return Err(Error::Invalid("Too many Artisan project sessions; reload the plugin to retire inactive sessions.".into()));
        }
        if owner(ctx)?.1 != root || runner.catalog(&project)?.1 != token { return Err(Error::Stale); }
        self.projects.entry(root).or_default().selection = Some(chosen);
        ui_kit::web::reveal_view(VIEW, include_str!("ui_config.json"))?;
        Ok(true)
    }

    /// Selects only an already captured exact command. Opening never discovers or executes PHP.
    pub fn select_command(&mut self, ctx:&Invocation, name:&str)->Result<bool,Error> {
        let (project,root)=owner(ctx)?;
        if name.is_empty() || name.len()>128 || name.starts_with('-') || !name.bytes().all(|byte|byte.is_ascii_alphanumeric() || b"_:-.".contains(&byte)) {return Err(Error::Invalid("Invalid Artisan command name".into()));}
        let runner=self.projects.get(&root).map(|session|session.runner).unwrap_or_default();
        let (snapshot,token)=match runner.catalog(&project) {Ok(value)=>value,Err(error) if runner==crate::artisan_runner::Runner::Docker=>return Err(error),Err(_)=>return Ok(false)};
        if !snapshot["commands"].as_array().is_some_and(|commands|commands.iter().any(|command|command["name"]==name)){return Ok(false);}
        if !self.projects.contains_key(&root) && self.projects.len()>=8 {return Err(Error::Invalid("Too many Artisan project sessions".into()));}
        if owner(ctx)?.1!=root || runner.catalog(&project)?.1!=token {return Err(Error::Stale);}
        self.projects.entry(root).or_default().selection=Some(name.into());
        ui_kit::web::reveal_view(VIEW,include_str!("ui_config.json"))?;
        Ok(true)
    }

    pub fn handle(
        &mut self,
        ctx: &Invocation,
        message: &str,
        terminals: &crate::generate::Terminals,
    ) -> Result<Value, Error> {
        if message.len() > 128 * 1024 {
            return Err(Error::Invalid("Artisan request is too large.".into()));
        }
        let request: Request =
            serde_json::from_str(message).map_err(|e| Error::Invalid(e.to_string()))?;
        let (project, root) = owner(ctx)?;
        if !self.projects.contains_key(&root) && self.projects.len() >= 8 {
            return Err(Error::Invalid(
                "Too many Artisan project sessions; reload the plugin to retire inactive sessions."
                    .into(),
            ));
        }
        let session = self.projects.entry(root.clone()).or_default();
        if matches!(request,Request::DockerTarget) {
            if session.runner!=crate::artisan_runner::Runner::Docker || session.job.as_ref().is_some_and(Job::running) {return Err(Error::Invalid("Select Docker and finish the current run before changing target.".into()));}
            crate::artisan_docker::discover(ctx)?;
        }
        if let Request::Runner { runner } = &request {
            if session.job.as_ref().is_some_and(Job::running) { return Err(Error::Invalid("Wait for or cancel the current run before changing runner.".into())); }
            session.runner = *runner;
        }
        let runner = session.runner;
        let target_label=if runner==crate::artisan_runner::Runner::Docker {crate::artisan_docker::target_label(&project).ok().flatten()} else {None};
        let mut response = json!({"root":root,"runner":runner.name(),"targetLabel":target_label,"sessionHistory":true,"additionalArgv":true});
        if matches!(request, Request::Refresh) {
            runner.refresh(ctx)?;
        }
        if matches!(request, Request::State | Request::Refresh | Request::DockerTarget | Request::Runner { .. }) {
            if let Ok((snapshot, token)) = runner.catalog(&project) {
                response["commands"] = snapshot["commands"].clone();
                response["token"] = token.into();
                response["capturedAt"] = snapshot["capturedAt"].clone();
            } else {
                response["commands"] = json!([]);
                response["token"] = Value::Null;
            }
        }
        if matches!(request, Request::State | Request::Refresh | Request::DockerTarget | Request::Runner { .. }) {
            if let Some(selection) = session.selection.take().filter(|name| response["commands"].as_array().is_some_and(|commands| commands.iter().any(|command| command["name"] == *name))) {
                response["selectCommand"] = selection.into();
            }
        }
        let cancelling = matches!(&request, Request::Cancel { .. });
        match request {
            Request::Preview {
                command,
                token,
                values,
                revision, additional_options, additional_arguments,
            } => {
                let (snapshot, current) = runner.catalog(&project)?;
                if token != current {
                    return Err(Error::Stale);
                }
                let definition = snapshot["commands"]
                    .as_array()
                    .and_then(|a| a.iter().find(|c| c["name"] == command))
                    .ok_or(Error::Stale)?;
                response["argv"] = json!(runner.argv(&project, &crate::artisan_arguments::merge(arguments(definition, &values, true)?,&additional_options,&additional_arguments,true)?, false)?);
                response["revision"] = revision.into();
            }
            Request::CopyOutput { id } => {
                let job = session
                    .job
                    .as_ref()
                    .filter(|job| job.id == id)
                    .ok_or(Error::Stale)?;
                let value = job.json();
                host::copy_to_clipboard(&format!(
                    "{}{}",
                    value["stdout"].as_str().unwrap_or_default(),
                    value["stderr"].as_str().unwrap_or_default()
                ))?;
                response["copied"] = true.into();
            }
            Request::Run {
                id,
                command,
                token,
                values,
                terminal, additional_options, additional_arguments,
            } => {
                if id.len() < 16
                    || id.len() > 128
                    || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                {
                    return Err(Error::Invalid("Invalid execution identity.".into()));
                }
                if session.seen.contains(&id) || session.job.as_ref().is_some_and(Job::running) {
                    return Err(Error::Invalid("An Artisan command is already running or this request was already submitted.".into()));
                }
                let (snapshot, current) = runner.catalog(&project)?;
                if token != current {
                    return Err(Error::Stale);
                }
                let definition = snapshot["commands"]
                    .as_array()
                    .and_then(|a| a.iter().find(|c| c["name"] == command))
                    .ok_or(Error::Stale)?;
                let argv = crate::artisan_arguments::merge(arguments(definition, &values, !terminal)?,&additional_options,&additional_arguments,!terminal)?;
                // Recheck source/project identity immediately before starting.
                if runner.catalog(&project)?.1 != current
                    || ctx.project().and_then(|p| p.root().ok()) != project.root().ok()
                {
                    return Err(Error::Stale);
                }
                let values =
                    serde_json::to_value(values).map_err(|e| Error::Invalid(e.to_string()))?;
                session.seen.push_back(id.clone());
                if session.seen.len() > 64 {
                    session.seen.pop_front();
                }
                if terminal {
                    crate::generate::run_with(terminals, ctx, &command, argv, runner)?;
                    response["terminal"] = true.into();
                } else {
                    let mut job = Job::start_with(&project, id.clone(), command, values, argv, runner)?;
                    job.additional_options=additional_options;job.additional_arguments=additional_arguments;
                    if let Some(previous) = session.job.take() {
                        let mut record = previous.json();
                        record.as_object_mut().unwrap().remove("stdout");
                        record.as_object_mut().unwrap().remove("stderr");
                        session.history.insert(0, record);
                        session.history.truncate(20);
                        while serde_json::to_string(&session.history)
                            .unwrap_or_default()
                            .len()
                            > 128 * 1024
                        {
                            session.history.pop();
                        }
                    }
                    session.job = Some(job);
                }
            }
            Request::Poll { id } | Request::Cancel { id } => {
                let job = session
                    .job
                    .as_mut()
                    .filter(|j| j.id == id)
                    .ok_or(Error::Stale)?;
                let was_running = job.running();
                job.poll(cancelling)?;
                if was_running && !job.running() && job.json()["cancelled"] == false && job.json()["status"] != 0 {
                    host::notify(openidea_plugin::NotificationLevel::Error, "Artisan", &format!("artisan {} exited with status {}. See command output for details.",job.command,job.json()["status"]));
                }
                if was_running
                    && !job.running()
                    && job.json()["status"] == 0
                    && job.json()["cancelled"] == false
                {
                    response["completed"] = true.into();
                }
            }
            _ => {}
        }
        response["job"] = session.job.as_ref().map_or(Value::Null, Job::json);
        response["history"] = json!(session.history);
        Ok(response)
    }
}
fn flag(item: &Value, key: &str) -> bool {
    item[key].as_bool().unwrap_or(false)
}
fn input<'a>(values: &'a BTreeMap<String, Value>, name: &str) -> Option<&'a Value> {
    values.get(name)
}
fn texts(value: Option<&Value>) -> Result<Vec<String>, Error> {
    match value {
        None | Some(Value::Null) => Ok(vec![]),
        Some(Value::String(s)) if s.len() <= 8192 && !s.contains('\0') => Ok(if s.is_empty() {
            vec![]
        } else {
            vec![s.clone()]
        }),
        Some(Value::Array(a)) if a.len() <= 128 => a
            .iter()
            .map(|v| match v {
                Value::String(s) if s.len() <= 8192 && !s.contains('\0') => Ok(s.clone()),
                _ => Err(Error::Invalid("Expected bounded string values.".into())),
            })
            .collect(),
        _ => Err(Error::Invalid(
            "Expected a string or repeated strings.".into(),
        )),
    }
}
pub(super) fn arguments(
    command: &Value,
    values: &BTreeMap<String, Value>,
    noninteractive: bool,
) -> Result<Vec<String>, Error> {
    let name = command["name"].as_str().ok_or(Error::Stale)?;
    let args = command["definition"]["arguments"]
        .as_array()
        .ok_or(Error::Stale)?;
    let options = command["definition"]["options"]
        .as_array()
        .ok_or(Error::Stale)?;
    if values.keys().any(|key| {
        !args
            .iter()
            .chain(options)
            .any(|item| item["name"].as_str() == Some(key))
    }) {
        return Err(Error::Invalid("Unknown argument or option.".into()));
    }
    let mut result = vec!["artisan".into(), name.into()];
    if noninteractive {
        result.extend(["--no-interaction".into(), "--no-ansi".into()]);
    }
    for item in options {
        let key = item["name"].as_str().ok_or(Error::Stale)?;
        let option = format!("--{}", key.trim_start_matches('-'));
        if ["--no-interaction", "--ansi", "--no-ansi"].contains(&option.as_str()) {
            continue;
        }
        let value = input(values, key);
        if !flag(item, "accept_value") {
            match value {
                Some(Value::Bool(true)) => result.push(option),
                None | Some(Value::Bool(false)) | Some(Value::Null) => {}
                _ => return Err(Error::Invalid(format!("{key} is a checkbox."))),
            }
        } else {
            if value == Some(&Value::Bool(true)) && !flag(item, "is_value_required") {
                result.push(option);
                continue;
            }
            let entries = texts(value)?;
            if entries.len() > 1 && !flag(item, "is_multiple") {
                return Err(Error::Invalid(format!("{key} accepts one value.")));
            }
            for entry in entries {
                result.push(format!("{option}={entry}"));
            }
        }
    }
    // End options before positional input, so a leading '-' remains data.
    let mut positional = vec![];
    let mut gap = false;
    for item in args {
        let key = item["name"].as_str().ok_or(Error::Stale)?;
        if key == "command" {
            continue;
        }
        let entries = texts(input(values, key))?;
        if entries.is_empty() {
            if flag(item, "is_required") {
                return Err(Error::Invalid(format!("{key} is required.")));
            }
            gap = true;
            continue;
        }
        if gap {
            return Err(Error::Invalid(
                "Fill preceding optional arguments before later positional arguments.".into(),
            ));
        }
        if entries.len() > 1 && !flag(item, "is_array") {
            return Err(Error::Invalid(format!("{key} accepts one value.")));
        }
        positional.extend(entries);
    }
    if !positional.is_empty() {
        result.push("--".into());
        result.extend(positional);
    }
    if result.len() > 512 || result.iter().map(String::len).sum::<usize>() > 64 * 1024 {
        return Err(Error::Invalid("Command arguments exceed limits.".into()));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn definition() -> Value {
        json!({"name":"app:probe","description":"Probe","definition":{"arguments":[{"name":"name","is_required":true},{"name":"tags","is_array":true}],"options":[{"name":"--force","accept_value":false},{"name":"--label","accept_value":true,"is_value_required":true},{"name":"--notify","accept_value":true,"is_value_required":false},{"name":"--item","accept_value":true,"is_multiple":true}]}})
    }
    fn values(value: Value) -> BTreeMap<String, Value> {
        serde_json::from_value(value).unwrap()
    }
    fn seed(host: &MockHost) -> (Project, Value, String) {
        host.add_file("artisan", "<?php");
        host.add_file("composer.json", "{}");
        let project = host.invocation().project().unwrap();
        let owner = metadata::identity(&project).unwrap();
        let commands = metadata::parse(&json!({"commands":[definition()]}).to_string()).unwrap();
        host::storage::set(
            &format!("artisan-metadata-v2:{}", metadata::hash("/project")),
            &json!({"schema":2,"identity":owner,"capturedAt":1,"commands":commands}).to_string(),
        )
        .unwrap();
        let (snapshot, token) = catalog(&project).unwrap();
        (project, snapshot, token)
    }
    fn seed_generators(host: &MockHost) {
        let (project, mut snapshot, _) = seed(host);
        let command = |name: &str| json!({"name":name,"description":"Generator","definition":{"arguments":[{"name":"name","is_required":true}],"options":[{"name":"--force","accept_value":false}]}});
        snapshot["commands"] = metadata::parse(&json!({"commands":[command("make:model"),command("make:widget"),definition()]}).to_string()).unwrap();
        host::storage::set(&format!("artisan-metadata-v2:{}", metadata::hash(&project.root().unwrap())), &snapshot.to_string()).unwrap();
    }
    #[test]
    fn generator_form_selection_is_explicit_once_and_project_bound() {
        let host = MockHost::install();
        seed_generators(&host);
        let mut state = State::default();
        host.answer_next_prompt(Some(&["make:widget"]));
        assert!(state.open_generator(&host.invocation(), None).unwrap());
        assert!(host.process_runs().is_empty());
        let terminals = crate::generate::Terminals::default();
        let reply = state.handle(&host.invocation(), r#"{"op":"state"}"#, &terminals).unwrap();
        assert_eq!(reply["selectCommand"], "make:widget");
        assert!(reply["commands"].as_array().unwrap().iter().any(|command| command["name"] == "make:widget"));
        let preview = state.handle(&host.invocation(), &json!({"op":"preview","command":"make:widget","token":reply["token"],"values":{"name":"NewWidget","--force":true},"revision":1}).to_string(), &terminals).unwrap();
        assert_eq!(preview["argv"], json!(["artisan","make:widget","--no-interaction","--no-ansi","--force","--","NewWidget"]));
        assert!(state.handle(&host.invocation(), r#"{"op":"state"}"#, &terminals).unwrap().get("selectCommand").is_none());
        assert!(state.open_generator(&host.invocation(), Some("make:model")).unwrap());
        assert!(!state.open_generator(&host.invocation(), Some("make:unknown")).unwrap());
        host.add_project("/project");
        host.set_project_root(Some("/other"));
        assert!(state.handle(&host.invocation(), r#"{"op":"state"}"#, &terminals).unwrap().get("selectCommand").is_none());
        host.set_project_root(Some("/project"));
        assert_eq!(state.handle(&host.invocation(), r#"{"op":"state"}"#, &terminals).unwrap()["selectCommand"], "make:model");
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn generator_form_requires_current_catalog_and_valid_picker_choice() {
        let host = MockHost::install();
        let mut state = State::default();
        assert!(!state.open_generator(&host.invocation(), None).unwrap());
        seed(&host);
        assert!(!state.open_generator(&host.invocation(), None).unwrap());
        seed_generators(&host);
        host.answer_next_prompt(Some(&["app:probe"]));
        assert!(state.open_generator(&host.invocation(), None).is_err());
        host.answer_next_prompt(None);
        assert!(state.open_generator(&host.invocation(), None).unwrap());
        assert!(state.projects.is_empty());
        state.open_generator(&host.invocation(), Some("make:model")).unwrap();
        host.add_file("composer.json", "{\"changed\":true}");
        let reply = state.handle(&host.invocation(), r#"{"op":"state"}"#, &crate::generate::Terminals::default()).unwrap();
        assert!(reply.get("selectCommand").is_none());
        assert_eq!(reply["commands"], json!([]));
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn builds_ordered_argv_without_shell_splitting_and_ends_option_parsing() {
        let argv=arguments(&definition(),&values(json!({"name":"-a b;$(touch nope)","tags":["one two","three"],"--force":true,"--label":"x=y z","--notify":true,"--item":["a","b"]})),true).unwrap();
        assert_eq!(
            argv,
            [
                "artisan",
                "app:probe",
                "--no-interaction",
                "--no-ansi",
                "--force",
                "--label=x=y z",
                "--notify",
                "--item=a",
                "--item=b",
                "--",
                "-a b;$(touch nope)",
                "one two",
                "three"
            ]
        );
    }
    #[test]
    fn refuses_missing_unknown_wrong_types_and_optional_gaps() {
        for value in [
            json!({}),
            json!({"name":"x","unknown":"y"}),
            json!({"name":["a","b"]}),
            json!({"name":"x","--force":"true"}),
            json!({"name":"x","--label":true}),
            json!({"name":"x\u{0000}"}),
        ] {
            assert!(arguments(&definition(), &values(value), true).is_err());
        }
        let def = json!({"name":"test","definition":{"arguments":[{"name":"first"},{"name":"second"}],"options":[]}});
        assert!(arguments(&def, &values(json!({"second":"x"})), true).is_err());
    }
    #[test]
    fn state_does_not_execute_discovery_and_keeps_runtime_provenance() {
        let host = MockHost::install();
        seed(&host);
        let mut state = State::default();
        let result = state
            .handle(
                &host.invocation(),
                r#"{"op":"state"}"#,
                &crate::generate::Terminals::default(),
            )
            .unwrap();
        assert_eq!(result["commands"][0]["name"], "app:probe");
        assert!(result["token"].is_string());
        assert!(host.process_runs().is_empty());
        host.add_file("artisan", "<?php changed");
        let result = state
            .handle(
                &host.invocation(),
                r#"{"op":"state"}"#,
                &crate::generate::Terminals::default(),
            )
            .unwrap();
        assert_eq!(result["commands"], json!([]));
        assert!(result["token"].is_null());
    }
    #[test]
    fn run_cancel_rerun_history_and_replay_are_project_bound() {
        let host =
            MockHost::install().with_capabilities(&["project.read", "storage", "process:php"]);
        let (_, _, token) = seed(&host);
        host.on_process("php", |_, _| {
            Ok(openidea_plugin::ProcessOutput {
                status: Some(2),
                stdout: "partial".into(),
                stderr: "bad input".into(),
            })
        });
        let mut state = State::default();
        let terminals = crate::generate::Terminals::default();
        let run = |id: &str| {
            json!({"op":"run","id":id,"command":"app:probe","token":token,"values":{"name":"hello world"}}).to_string()
        };
        let id = "execution-request-0001";
        assert!(
            state
                .handle(&host.invocation(), &run(id), &terminals)
                .unwrap()["job"]["running"]
                .as_bool()
                .unwrap()
        );
        assert!(
            state
                .handle(&host.invocation(), &run(id), &terminals)
                .is_err()
        );
        let result = state
            .handle(
                &host.invocation(),
                &json!({"op":"cancel","id":id}).to_string(),
                &terminals,
            )
            .unwrap();
        assert_eq!(result["job"]["status"], -1);
        assert_eq!(result["job"]["cancelled"], true);
        assert!(
            state
                .handle(&host.invocation(), &run(id), &terminals)
                .is_err()
        );
        let result = state
            .handle(
                &host.invocation(),
                &run("execution-request-0002"),
                &terminals,
            )
            .unwrap();
        assert_eq!(result["history"].as_array().unwrap().len(), 1);
        assert!(result["history"][0].get("stdout").is_none());
        assert!(
            state
                .handle(
                    &host.invocation(),
                    &json!({"op":"cancel","id":id}).to_string(),
                    &terminals
                )
                .is_err()
        );
        host.set_project_root(Some("/other"));
        host.add_file("/other/artisan", "<?php");
        assert!(
            state
                .handle(
                    &host.invocation(),
                    &json!({"op":"poll","id":"execution-request-0002"}).to_string(),
                    &terminals
                )
                .is_err()
        );
        assert!(
            host.process_runs().is_empty(),
            "kept handles do not use the synchronous runner"
        );
    }
    #[test]
    fn preview_is_exact_and_stale_token_never_runs() {
        let host = MockHost::install();
        let (_, _, token) = seed(&host);
        let mut state = State::default();
        let terminals = crate::generate::Terminals::default();
        let reply=state.handle(&host.invocation(),&json!({"op":"preview","command":"app:probe","token":token,"values":{"name":"two words"},"revision":4}).to_string(),&terminals).unwrap();
        assert_eq!(
            reply["argv"],
            json!([
                "artisan",
                "app:probe",
                "--no-interaction",
                "--no-ansi",
                "--",
                "two words"
            ])
        );
        assert_eq!(reply["revision"], 4);
        assert!(host.process_runs().is_empty());
        host.add_file("composer.json", "{\"changed\":true}");
        assert!(state.handle(&host.invocation(),&json!({"op":"run","id":"execution-request-0001","command":"app:probe","token":token,"values":{"name":"x"}}).to_string(),&terminals).is_err());
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn additional_argv_preview_run_and_history_preserve_exact_grouping() {
        let h=MockHost::install().with_capabilities(&["project.read","storage","process:php"]);let(_,_,token)=seed(&h);
        h.on_process("php",|_,_|Ok(openidea_plugin::ProcessOutput{status:Some(0),stdout:String::new(),stderr:String::new()}));
        let mut state=State::default();let terminals=crate::generate::Terminals::default();let ctx=h.invocation();
        let mut request=json!({"op":"preview","command":"app:probe","token":token,"values":{"name":"-équipe","tags":["structured"]},"revision":1,"additionalOptions":"--label=\"extra option\"","additionalArguments":"\"trailing value\" '$HOME'"});
        let preview=state.handle(&ctx,&request.to_string(),&terminals).unwrap();
        assert_eq!(preview["argv"],json!(["artisan","app:probe","--no-interaction","--no-ansi","--label=extra option","--","-équipe","structured","trailing value","$HOME"]));
        request["op"]=json!("run");request.as_object_mut().unwrap().remove("revision");request["id"]=json!("additional-argv-fixture-001");
        let run=state.handle(&ctx,&request.to_string(),&terminals).unwrap();assert_eq!(run["job"]["argv"],preview["argv"]);assert_eq!(run["job"]["additionalOptions"],request["additionalOptions"]);assert_eq!(run["job"]["additionalArguments"],request["additionalArguments"]);
        request["id"]=json!("additional-argv-invalid-002");request["additionalOptions"]=json!("--ansi");assert!(state.handle(&ctx,&request.to_string(),&terminals).is_err());
    }

}
