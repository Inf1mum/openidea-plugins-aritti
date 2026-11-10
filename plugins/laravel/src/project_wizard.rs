//! Explicit new-project creation. Installers run in a keyed terminal, never a
//! synchronous provider callback. Dismissing a prompt never creates a directory.
use openidea_plugin::{Error, FileKind, Invocation, PickItem, TerminalExited, host};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Plan {
    program: String,
    args: Vec<String>,
    destination: String,
    tool_identity: String,
}

#[derive(Default)]
pub(crate) struct State {
    pending: BTreeMap<String, (String, String)>,
    sequence: u64,
}

fn valid_parent(parent: &str) -> bool {
    let windows = parent.as_bytes().get(1) == Some(&b':')
        && parent
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic)
        && matches!(parent.as_bytes().get(2), Some(b'/' | b'\\'));
    (parent.starts_with('/') || windows)
        && parent.len() <= 4096
        && !parent.chars().any(char::is_control)
        && !parent
            .split(['/', '\\'])
            .any(|part| matches!(part, "." | ".."))
}

fn destination(parent: &str, name: &str) -> Result<String, Error> {
    if !valid_parent(parent)
        || name.is_empty()
        || name.len() > 80
        || !name.as_bytes()[0].is_ascii_alphanumeric()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(Error::Invalid("Choose an absolute parent directory and a project name containing letters, numbers, dashes or underscores.".into()));
    }
    // Absolute paths ensure a project name cannot become an installer option.
    Ok(format!("{}/{name}", parent.trim_end_matches(['/', '\\'])))
}

fn inspect_destination(parent: &str, target: &str) -> Result<(), Error> {
    if host::fs::stat(parent)?.kind != FileKind::Directory {
        return Err(Error::Invalid(
            "The selected parent must be an existing directory, not a symlink.".into(),
        ));
    }
    match host::fs::stat(target) {
        Err(Error::NotFound(_)) => Ok(()),
        Err(error) => Err(error),
        Ok(stat) if stat.kind == FileKind::Directory => {
            if host::fs::read_dir(target)?.is_empty() {
                Ok(())
            } else {
                Err(Error::Invalid(
                    "The destination is not empty. Choose a new name; no files have been changed."
                        .into(),
                ))
            }
        }
        Ok(_) => Err(Error::Invalid(
            "The destination is a file or symlink. Choose another name.".into(),
        )),
    }
}

fn tool_output(program: &str, args: &[&str]) -> Result<String, Error> {
    let result = host::run_process(
        program,
        &args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
        None,
    )?;
    if result.status != Some(0) || result.stdout.len() + result.stderr.len() > 128 << 10 {
        return Err(Error::Invalid(format!(
            "{program} discovery failed; install/configure the tool and review its process permission."
        )));
    }
    let output = format!("{}{}", result.stdout, result.stderr);
    if output.trim().is_empty() {
        return Err(Error::Invalid(format!(
            "{program} returned no tool information."
        )));
    }
    Ok(output)
}

fn supported_option(help: &str, option: &str) -> bool {
    help.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with(option)
            && trimmed[option.len()..]
                .chars()
                .next()
                .is_none_or(|c| c.is_whitespace() || matches!(c, '=' | '['))
    })
}

fn plan(
    program: &str,
    target: String,
    identity: String,
    help: &str,
    options: &[String],
) -> Result<Plan, Error> {
    let args = match program {
        "composer" if options.is_empty() => vec![
            "create-project".into(),
            "--prefer-dist".into(),
            "--no-interaction".into(),
            "laravel/laravel".into(),
            target.clone(),
        ],
        "laravel" => {
            let allowed = ["--react", "--vue", "--livewire", "--svelte", "--boost"];
            if options.iter().any(|option| {
                !allowed.contains(&option.as_str()) || !supported_option(help, option)
            }) || options
                .iter()
                .filter(|option| {
                    ["--react", "--vue", "--livewire", "--svelte"].contains(&option.as_str())
                })
                .count()
                > 1
                || options
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != options.len()
            {
                return Err(Error::Invalid("Unsupported or conflicting installer options. Refresh the installed tool's help.".into()));
            }
            let mut args = vec!["new".into(), target.clone()];
            args.extend(options.iter().cloned());
            // Keep the installer interactive: new releases may need choices
            // which cannot safely be guessed from a help listing.
            args
        }
        _ => {
            return Err(Error::Invalid(
                "Choose Composer or the Laravel installer.".into(),
            ));
        }
    };
    Ok(Plan {
        program: program.into(),
        args,
        destination: target,
        tool_identity: identity,
    })
}

impl State {
    pub(crate) fn run(&mut self, _ctx: &Invocation) -> Result<Option<String>, Error> {
        if self.pending.len() >= 8 {
            return Err(Error::Invalid(
                "Finish an existing project installation first.".into(),
            ));
        }
        let Some(parent) =
            host::pick_directory("New Laravel project: choose parent directory", None)?
        else {
            return Ok(None);
        };
        let Some(name) = host::input("New Laravel project name", Some("laravel-app"))? else {
            return Ok(None);
        };
        let target = destination(&parent, name.trim())?;
        inspect_destination(&parent, &target)?;
        if self
            .pending
            .values()
            .any(|(_, existing)| existing == &target)
        {
            return Err(Error::Invalid(
                "A project installation is already running for that destination.".into(),
            ));
        }
        let Some(program) = host::pick(
            "Create Laravel project with",
            &[
                PickItem::new("composer", "Composer: Laravel application skeleton"),
                PickItem::new(
                    "laravel",
                    "Laravel installer: available starter kits and packages",
                ),
            ],
        )?
        else {
            return Ok(None);
        };
        let identity = tool_output(&program, &["--version"])?;
        let help = if program == "laravel" {
            tool_output("laravel", &["new", "--help"])?
        } else {
            String::new()
        };
        let mut options = Vec::new();
        if program == "laravel" {
            let mut choices = vec![PickItem::new(
                "none",
                "No preselected starter kit (choose in installer)",
            )];
            for (key, label) in [
                ("--react", "React"),
                ("--vue", "Vue"),
                ("--livewire", "Livewire"),
                ("--svelte", "Svelte"),
            ] {
                if supported_option(&help, key) {
                    choices.push(PickItem::new(key, label));
                }
            }
            let Some(starter) = host::pick("Starter kit supported by this installer", &choices)?
            else {
                return Ok(None);
            };
            if starter != "none" {
                options.push(starter);
            }
            if supported_option(&help, "--boost") {
                let Some(boost) = host::pick(
                    "Laravel Boost",
                    &[
                        PickItem::new("no", "Do not preselect Boost"),
                        PickItem::new("yes", "Install Boost"),
                    ],
                )?
                else {
                    return Ok(None);
                };
                if boost == "yes" {
                    options.push("--boost".into());
                }
            }
        }
        let plan = plan(&program, target, identity, &help, &options)?;
        let preview = format!(
            "Destination: {}\nTool: {}\nProgram: {}\nExact arguments:\n{}\n\nThis installs dependencies and runs the selected installer's project scripts in a terminal. Other installer choices remain interactive. Existing files are never intentionally overwritten. Failed installations and full terminal output are retained for inspection; no automatic cleanup occurs. On successful completion the new project's normal trust/open flow opens in a new window.\n\nStart installation?",
            plan.destination,
            plan.tool_identity.trim(),
            plan.program,
            serde_json::to_string_pretty(&plan.args).unwrap_or_default()
        );
        if !ui_kit::prompts::confirm_exact("Create Laravel project?", &preview)? {
            return Ok(None);
        }
        inspect_destination(&parent, &plan.destination)?;
        if tool_output(&plan.program, &["--version"])? != plan.tool_identity {
            return Err(Error::Stale);
        }
        self.sequence = self.sequence.checked_add(1).ok_or(Error::Stale)?;
        use std::hash::BuildHasher;
        let nonce = std::collections::hash_map::RandomState::new().hash_one((
            &plan.destination,
            self.sequence,
            std::time::SystemTime::now(),
        ));
        let key = format!("laravel-project-{nonce:016x}-{}", self.sequence);
        let id = host::open_terminal_session(
            &key,
            &format!("Create Laravel project: {}", name.trim()),
            &host::Command {
                program: plan.program,
                args: plan.args,
                env: Vec::new(),
            },
        )?;
        self.pending.insert(key, (id, plan.destination));
        Ok(Some("Installation started in its terminal. Output and any failed installation are retained.".into()))
    }

    /// Match both the host operation id and the key; late or foreign exits do
    /// not consume a newer installation or open an unrelated directory.
    pub(crate) fn complete(&mut self, event: &TerminalExited) -> Result<bool, Error> {
        let Some((id, _)) = self.pending.get(&event.key) else {
            return Ok(false);
        };
        if id != &event.id {
            return Ok(false);
        }
        let (_, destination) = self.pending.remove(&event.key).ok_or(Error::Stale)?;
        if event.status == Some(0) && !event.closed {
            if host::fs::stat(&destination)?.kind != FileKind::Directory {
                return Err(Error::Stale);
            }
            for file in ["artisan", "composer.json"] {
                if host::fs::stat(&format!("{destination}/{file}"))?.kind != FileKind::File {
                    return Err(Error::Invalid("The installer exited successfully but no Laravel application skeleton was found. Inspect its retained terminal output.".into()));
                }
            }
            host::open_project(&destination, true)?;
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_is_absolute_and_never_an_option_or_traversal() {
        assert_eq!(
            destination("/Users/a/My Projects/", "my-app").unwrap(),
            "/Users/a/My Projects/my-app"
        );
        assert_eq!(
            destination("C:\\Projects", "app").unwrap(),
            "C:\\Projects/app"
        );
        for (parent, name) in [
            ("relative", "app"),
            ("/tmp/../x", "app"),
            ("/tmp", "../app"),
            ("/tmp", "--force"),
            ("/tmp", "a/b"),
            ("/tmp", ""),
            ("/tmp", "app\n"),
        ] {
            assert!(destination(parent, name).is_err());
        }
    }

    #[test]
    fn composer_argv_keeps_spaced_destination_as_one_argument() {
        let plan = plan(
            "composer",
            "/My Projects/app".into(),
            "Composer 2".into(),
            "",
            &[],
        )
        .unwrap();
        assert_eq!(
            plan.args,
            [
                "create-project",
                "--prefer-dist",
                "--no-interaction",
                "laravel/laravel",
                "/My Projects/app"
            ]
        );
    }

    #[test]
    fn installer_options_are_proven_by_installed_help() {
        let help =
            "Options:\n  --react  React kit\n  --boost  Install Boost\n  --vue[=VERSION] Vue kit\n";
        assert!(supported_option(help, "--react"));
        assert!(!supported_option(help, "--re"));
        assert!(
            plan(
                "laravel",
                "/tmp/app".into(),
                "v5".into(),
                help,
                &["--react".into(), "--boost".into()]
            )
            .is_ok()
        );
        for options in [
            vec!["--svelte".into()],
            vec!["--react".into(), "--vue".into()],
            vec!["--react".into(), "--react".into()],
            vec!["--force".into()],
        ] {
            assert!(plan("laravel", "/tmp/app".into(), "v5".into(), help, &options).is_err());
        }
    }

    fn fixture() -> openidea_plugin::mock::MockHost {
        let mock = openidea_plugin::mock::MockHost::install()
            .with_capabilities(&["fs.read:/tmp/projects", "process:composer"]);
        mock.add_external_file("/tmp/projects/existing/keep", "existing user file");
        mock.on_process("composer", |args, _| {
            assert_eq!(args, &["--version"]);
            Ok(openidea_plugin::ProcessOutput {
                status: Some(0),
                stdout: "Composer 2\n".into(),
                stderr: String::new(),
            })
        });
        mock
    }

    fn answers(mock: &openidea_plugin::mock::MockHost, name: &str, confirm: bool) {
        mock.answer_next_prompt(Some(&["/tmp/projects"]));
        mock.answer_next_prompt(Some(&[name]));
        mock.answer_next_prompt(Some(&["composer"]));
        mock.answer_next_prompt(if confirm { Some(&["yes"]) } else { None });
    }

    #[test]
    fn nonempty_destination_and_cancellation_never_launch_installation() {
        let mock = fixture();
        answers(&mock, "existing", true);
        let mut state = State::default();
        assert!(state.run(&mock.invocation()).is_err());
        assert!(mock.process_runs().is_empty());
        assert!(state.pending.is_empty());

        let mock = fixture();
        answers(&mock, "cancelled", false);
        assert!(State::default().run(&mock.invocation()).unwrap().is_none());
        assert_eq!(mock.process_runs().len(), 1, "only tool version discovery");
    }

    #[test]
    fn installation_requires_matching_successful_exit_and_real_skeleton() {
        let mock = fixture();
        answers(&mock, "new-app", true);
        let mut state = State::default();
        assert!(state.run(&mock.invocation()).unwrap().is_some());
        assert_eq!(
            mock.process_runs().len(),
            2,
            "discovery and final tool identity check only"
        );
        let (key, (id, _)) = state
            .pending
            .iter()
            .next()
            .map(|(key, value)| (key.clone(), value.clone()))
            .unwrap();
        let mut exit = TerminalExited {
            key: key.clone(),
            id: "foreign".into(),
            status: Some(0),
            closed: false,
        };
        assert!(!state.complete(&exit).unwrap());
        exit.id = id.clone();
        mock.add_external_file("/tmp/projects/new-app/artisan", "<?php");
        mock.add_external_file("/tmp/projects/new-app/composer.json", "{}");
        assert!(state.complete(&exit).unwrap());
        assert_eq!(
            mock.opened_projects(),
            [("/tmp/projects/new-app".into(), true)]
        );
        assert!(
            !state.complete(&exit).unwrap(),
            "duplicate exit cannot reopen a project"
        );
    }

    #[test]
    fn failed_installation_is_not_opened_or_deleted() {
        let mock = fixture();
        answers(&mock, "failed-app", true);
        let mut state = State::default();
        state.run(&mock.invocation()).unwrap();
        let (key, (id, _)) = state
            .pending
            .iter()
            .next()
            .map(|(key, value)| (key.clone(), value.clone()))
            .unwrap();
        let exit = TerminalExited {
            key: key.clone(),
            id: id.clone(),
            status: Some(1),
            closed: false,
        };
        mock.add_external_file("/tmp/projects/failed-app/partial", "retained");
        assert!(state.complete(&exit).unwrap());
        assert!(mock.opened_projects().is_empty());
        assert_eq!(
            host::fs::read_file("/tmp/projects/failed-app/partial").unwrap(),
            "retained"
        );
    }

    #[test]
    fn unreadable_destination_fails_before_discovery() {
        let mock =
            openidea_plugin::mock::MockHost::install().with_capabilities(&["process:composer"]);
        answers(&mock, "app", true);
        assert!(State::default().run(&mock.invocation()).is_err());
        assert!(mock.process_runs().is_empty());
    }

    #[test]
    fn tool_identity_change_after_review_refuses_launch() {
        let mock = fixture();
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let captured = calls.clone();
        mock.on_process("composer", move |_, _| {
            captured.set(captured.get() + 1);
            Ok(openidea_plugin::ProcessOutput {
                status: Some(0),
                stdout: format!("Composer version {}", captured.get()),
                stderr: String::new(),
            })
        });
        answers(&mock, "app", true);
        let mut state = State::default();
        assert!(matches!(state.run(&mock.invocation()), Err(Error::Stale)));
        assert!(state.pending.is_empty());
    }
}
