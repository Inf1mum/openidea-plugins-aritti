//! Explicit new-project creation. Installers run in a keyed terminal, never a
//! synchronous provider callback. Dismissing a prompt never creates a directory.
use openidea_plugin::{Error, FileKind, Invocation, PickItem, TerminalExited, host};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
const HISTORY_KEY: &str = "laravel.project-installations.v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    program: String,
    args: Vec<String>,
    destination: String,
    tool_identity: String,
    #[serde(default)]
    tool_help: String,
}

#[derive(Default)]
pub(crate) struct State {
    pending: BTreeMap<String, (String, String)>,
    sequence: u64,
    history: Vec<Attempt>,
    loaded: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Attempt {
    key: String,
    id: String,
    plan: Plan,
    status: Status,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Status {
    Running,
    Failed,
    Closed,
    Completed,
    Incomplete,
}
impl Status {
    fn label(self) -> &'static str {
        match self {
            Self::Running => crate::ui_text::text(
                "project.wizard.status.running",
                "Unresolved installation (may still be running)",
            ),
            Self::Failed => {
                crate::ui_text::text("project.wizard.status.failed", "Installer failed")
            }
            Self::Closed => crate::ui_text::text("project.wizard.status.closed", "Terminal closed"),
            Self::Completed => {
                crate::ui_text::text("project.wizard.status.completed", "Installer completed")
            }
            Self::Incomplete => crate::ui_text::text(
                "project.wizard.status.incomplete",
                "Skeleton incomplete or unreadable",
            ),
        }
    }
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
        return Err(Error::Invalid(crate::ui_text::t(
            "project.wizard.error.85d8f36e80",
            "Choose an absolute parent directory and a project name containing letters, numbers, dashes or underscores.",
        )));
    }
    // Absolute paths ensure a project name cannot become an installer option.
    Ok(format!("{}/{name}", parent.trim_end_matches(['/', '\\'])))
}

fn inspect_destination(parent: &str, target: &str) -> Result<(), Error> {
    if host::fs::stat(parent)?.kind != FileKind::Directory {
        return Err(Error::Invalid(crate::ui_text::t(
            "project.wizard.error.7fbc21c8ee",
            "The selected parent must be an existing directory, not a symlink.",
        )));
    }
    match host::fs::stat(target) {
        Err(Error::NotFound(_)) => Ok(()),
        Err(error) => Err(error),
        Ok(stat) if stat.kind == FileKind::Directory => {
            if host::fs::read_dir(target)?.is_empty() {
                Ok(())
            } else {
                Err(Error::Invalid(crate::ui_text::t(
                    "project.wizard.error.b5a176400c",
                    "The destination is not empty. Choose a new name; no files have been changed.",
                )))
            }
        }
        Ok(_) => Err(Error::Invalid(crate::ui_text::t(
            "project.wizard.error.3b71c6951c",
            "The destination is a file or symlink. Choose another name.",
        ))),
    }
}

fn inspect_tool_destination(parent: &str, target: &str, program: &str) -> Result<(), Error> {
    inspect_destination(parent, target)?;
    if program == "laravel" {
        if !target.starts_with('/') {
            return Err(Error::Invalid(crate::ui_text::t(
                "project.wizard.error.60a40a3db1",
                "This installer adapter cannot prove Windows drive destination handling. Use Composer for this destination.",
            )));
        }
        match host::fs::stat(target) {
            Err(Error::NotFound(_)) => {}
            Err(error) => return Err(error),
            Ok(_) => {
                return Err(Error::Invalid(crate::ui_text::t(
                    "project.wizard.error.73abe1375d",
                    "The Laravel installer requires a new, absent destination. Choose another name or use Composer for an empty directory; force is never used.",
                )));
            }
        }
    }
    Ok(())
}

fn tool_output(program: &str, args: &[&str]) -> Result<String, Error> {
    let result = host::run_process(
        program,
        &args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
        None,
    )?;
    if result.status != Some(0) || result.stdout.len() + result.stderr.len() > 128 << 10 {
        return Err(Error::Invalid(crate::ui_text::format(
            "project.wizard.format.96083596d2",
            "{program} discovery failed; install/configure the tool and review its process permission.",
            &[("program", program)],
        )));
    }
    let output = format!("{}{}", result.stdout, result.stderr);
    if output.trim().is_empty() {
        return Err(Error::Invalid(crate::ui_text::format(
            "project.wizard.format.7c4f296315",
            "{program} returned no tool information.",
            &[("program", program)],
        )));
    }
    Ok(output)
}

fn option_line<'a>(help: &'a str, option: &str) -> Option<&'a str> {
    help.lines().find_map(|line| {
        // Symfony help can prefix long options with a short alias.
        let line = line.trim_start();
        let line = if line.starts_with('-') && !line.starts_with("--") {
            line.split_once(", ")?.1.trim_start()
        } else {
            line
        };
        line.strip_prefix(option)
            .filter(|suffix| {
                suffix
                    .chars()
                    .next()
                    .is_none_or(|c| c.is_whitespace() || matches!(c, '=' | '['))
            })
            .map(|_| line)
    })
}
fn supported_option(help: &str, option: &str) -> bool {
    option_line(help, option).is_some()
}
fn has_value(help: &str, option: &str) -> bool {
    option_line(help, option).is_some_and(|line| {
        line[option.len()..].starts_with('=') || line[option.len()..].starts_with("[=")
    })
}
fn package(value: &str) -> bool {
    let (name, version) = value
        .split_once(':')
        .map_or((value, None), |(name, version)| (name, Some(version)));
    let parts: Vec<_> = name.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 100
                && part
                    .bytes()
                    .next()
                    .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                && part.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'-')
                })
        })
        && version.is_none_or(|v| {
            !v.is_empty()
                && v.len() <= 80
                && v.bytes().all(|b| {
                    b.is_ascii_alphanumeric()
                        || matches!(b, b'.' | b'_' | b'-' | b'+' | b'^' | b'~' | b'*' | b'@')
                })
        })
}
fn branch(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && !value.starts_with(['-', '/', '.'])
        && !value.ends_with(['.', '/'])
        && !value.contains("..")
        && !value.contains("//")
        && value.split('/').all(|part| {
            !part.is_empty()
                && !part.ends_with(".lock")
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        })
}
fn installer_options(help: &str, options: &[String]) -> Result<(), Error> {
    let flags = [
        "--react",
        "--vue",
        "--livewire",
        "--svelte",
        "--boost",
        "--no-boost",
        "--pest",
        "--phpunit",
        "--npm",
        "--pnpm",
        "--bun",
        "--yarn",
        "--no-node",
        "--git",
        "--dev",
        "--workos",
        "--teams",
        "--no-authentication",
        "--livewire-class-components",
    ];
    let mut seen = BTreeSet::new();
    for option in options {
        let (name, value) = option
            .split_once('=')
            .map_or((option.as_str(), None), |(name, value)| (name, Some(value)));
        if !seen.insert(name) || !supported_option(help, name) {
            return Err(Error::Invalid(crate::ui_text::t(
                "project.wizard.error.e5151f9c04",
                "Unsupported or duplicate installer option",
            )));
        }
        match (name, value) {
            (name, None) if flags.contains(&name) && !has_value(help, name) => {}
            ("--database", Some(value))
                if has_value(help, name)
                    && ["mysql", "mariadb", "pgsql", "sqlite", "sqlsrv"].contains(&value)
                    && option_line(help, name).is_some_and(|line| {
                        line.split(|c: char| !c.is_ascii_alphanumeric())
                            .any(|word| word == value)
                    }) => {}
            ("--using", Some(value)) if has_value(help, name) && package(value) => {}
            ("--branch", Some(value)) if has_value(help, name) && branch(value) => {}
            _ => {
                return Err(Error::Invalid(crate::ui_text::t(
                    "project.wizard.error.82b85d9250",
                    "Unsupported installer option value or option kind",
                )));
            }
        }
    }
    for group in [
        &["--react", "--vue", "--livewire", "--svelte", "--using"][..],
        &["--boost", "--no-boost"][..],
        &["--pest", "--phpunit"][..],
        &["--npm", "--pnpm", "--bun", "--yarn", "--no-node"][..],
        &["--workos", "--no-authentication"][..],
    ] {
        if group.iter().filter(|name| seen.contains(**name)).count() > 1 {
            return Err(Error::Invalid(crate::ui_text::t(
                "project.wizard.error.49364a1461",
                "Conflicting installer choices",
            )));
        }
    }
    if seen.contains("--branch") && !seen.contains("--git")
        || seen.contains("--livewire-class-components") && !seen.contains("--livewire")
        || seen.contains("--livewire-class-components")
            && (seen.contains("--teams") || seen.contains("--workos"))
        || seen.contains("--teams")
            && (seen.contains("--workos") || seen.contains("--no-authentication"))
    {
        return Err(Error::Invalid(crate::ui_text::t(
            "project.wizard.error.f75c0e5616",
            "The selected branch/authentication/component options are incompatible",
        )));
    }
    Ok(())
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
            installer_options(help, options)?;
            let mut args = vec!["new".into(), target.clone()];
            args.extend(options.iter().cloned());
            // Keep the installer interactive: new releases may need choices
            // which cannot safely be guessed from a help listing.
            args
        }
        _ => {
            return Err(Error::Invalid(crate::ui_text::t(
                "project.wizard.error.0ba980e88b",
                "Choose Composer or the Laravel installer.",
            )));
        }
    };
    Ok(Plan {
        program: program.into(),
        args,
        destination: target,
        tool_identity: identity,
        tool_help: help.into(),
    })
}

fn ui_slug(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_end_matches('_').into()
}

fn append_choice(
    options: &mut Vec<String>,
    help: &str,
    title: &str,
    choices: &[(&str, &str)],
) -> Result<bool, Error> {
    let mut items = vec![PickItem::new(
        "interactive",
        &crate::ui_text::t(
            "project.wizard.option.3ad6df3a1e",
            "Leave this choice to the installer",
        ),
    )];
    for (flag, label) in choices {
        let name = flag.split('=').next().unwrap_or(flag);
        if supported_option(help, name) {
            items.push(PickItem::new(
                *flag,
                &crate::ui_text::t(&format!("project.wizard.flag.{}", ui_slug(flag)), label),
            ));
        }
    }
    if items.len() == 1 {
        return Ok(true);
    }
    let title = crate::ui_text::t(&format!("project.wizard.choice.{}", ui_slug(title)), title);
    let Some(chosen) = host::pick(&title, &items)? else {
        return Ok(false);
    };
    if chosen != "interactive" {
        options.push(chosen);
    }
    Ok(true)
}
fn choose_options(help: &str) -> Result<Option<Vec<String>>, Error> {
    let mut options = Vec::new();
    if !append_choice(
        &mut options,
        help,
        "Starter kit supported by this installer",
        &[
            ("--react", "React"),
            ("--vue", "Vue"),
            ("--livewire", "Livewire"),
            ("--svelte", "Svelte"),
            ("--using", "Custom Composer starter package"),
        ],
    )? {
        return Ok(None);
    }
    if options.last().map(String::as_str) == Some("--using") {
        let Some(package_name) = host::input(
            &crate::ui_text::t(
                "project.wizard.prompt.f698fe4f2a",
                "Custom starter Composer package (vendor/package or vendor/package:version)",
            ),
            None,
        )?
        else {
            return Ok(None);
        };
        let package_name = package_name.trim();
        if !package(package_name) {
            return Err(Error::Invalid(crate::ui_text::t(
                "project.wizard.error.bda2913c22",
                "Enter a Composer vendor/package name with an optional simple version constraint",
            )));
        }
        *options.last_mut().ok_or(Error::Stale)? = format!("--using={package_name}");
    }
    let builtin = options
        .iter()
        .any(|option| ["--react", "--vue", "--livewire", "--svelte"].contains(&option.as_str()));
    if builtin {
        if !append_choice(
            &mut options,
            help,
            "Starter authentication",
            &[
                ("--workos", "WorkOS (requires an account)"),
                ("--no-authentication", "No authentication scaffolding"),
            ],
        )? {
            return Ok(None);
        }
        if !options
            .iter()
            .any(|option| ["--workos", "--no-authentication"].contains(&option.as_str()))
            && !append_choice(
                &mut options,
                help,
                "Team support",
                &[("--teams", "Include team support")],
            )?
        {
            return Ok(None);
        }
        if options.iter().any(|option| option == "--livewire")
            && !options
                .iter()
                .any(|option| ["--teams", "--workos"].contains(&option.as_str()))
            && !append_choice(
                &mut options,
                help,
                "Livewire component format",
                &[(
                    "--livewire-class-components",
                    "Separate Livewire class components",
                )],
            )?
        {
            return Ok(None);
        }
    }
    let database_choices = [
        ("--database=sqlite", "SQLite"),
        ("--database=mysql", "MySQL"),
        ("--database=mariadb", "MariaDB"),
        ("--database=pgsql", "PostgreSQL"),
        ("--database=sqlsrv", "SQL Server"),
    ];
    let database_choices: Vec<_> = database_choices
        .into_iter()
        .filter(|(flag, _)| {
            option_line(help, "--database").is_some_and(|line| {
                line.split(|c: char| !c.is_ascii_alphanumeric())
                    .any(|word| Some(word) == flag.split_once('=').map(|(_, value)| value))
            })
        })
        .collect();
    if !append_choice(
        &mut options,
        help,
        "Application database",
        &database_choices,
    )? {
        return Ok(None);
    }
    if !append_choice(
        &mut options,
        help,
        "Testing framework",
        &[("--pest", "Pest"), ("--phpunit", "PHPUnit")],
    )? {
        return Ok(None);
    }
    if !append_choice(
        &mut options,
        help,
        "Laravel Boost",
        &[
            ("--boost", "Install Laravel Boost"),
            ("--no-boost", "Skip Laravel Boost"),
        ],
    )? {
        return Ok(None);
    }
    if builtin || options.iter().any(|option| option.starts_with("--using=")) {
        if !append_choice(
            &mut options,
            help,
            "Frontend dependencies",
            &[
                ("--npm", "Install/build with npm"),
                ("--pnpm", "Install/build with pnpm"),
                ("--bun", "Install/build with Bun"),
                ("--yarn", "Install/build with Yarn"),
                ("--no-node", "Skip frontend dependencies"),
            ],
        )? {
            return Ok(None);
        }
    }
    if !append_choice(
        &mut options,
        help,
        "Local Git repository",
        &[("--git", "Initialize Git locally")],
    )? {
        return Ok(None);
    }
    if options.iter().any(|option| option == "--git") && has_value(help, "--branch") {
        let Some(name) = host::input(
            &crate::ui_text::t("project.wizard.prompt.bafc2b2043", "Initial Git branch"),
            Some("main"),
        )?
        else {
            return Ok(None);
        };
        let name = name.trim();
        if !branch(name) {
            return Err(Error::Invalid(crate::ui_text::t(
                "project.wizard.error.fcaabcdd98",
                "Enter a simple valid Git branch name",
            )));
        }
        options.push(format!("--branch={name}"));
    }
    if !append_choice(
        &mut options,
        help,
        "Laravel release channel",
        &[("--dev", "Latest development release")],
    )? {
        return Ok(None);
    }
    installer_options(help, &options)?;
    Ok(Some(options))
}

impl State {
    fn load_history(&mut self) -> Result<(), Error> {
        if self.loaded {
            return Ok(());
        }
        let stored = match host::storage::get(HISTORY_KEY) {
            Ok(value) => value,
            Err(Error::Denied(_)) => {
                self.loaded = true;
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        if let Some(text) = stored {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct History {
                version: u32,
                attempts: Vec<Attempt>,
            }
            let value = crate::strict_json::parse(&text, 2 << 20)?;
            let history: History = serde_json::from_value(value).map_err(|_| {
                Error::Invalid(crate::ui_text::t(
                    "project.wizard.error.04b3f75741",
                    "Stored project recovery data is invalid",
                ))
            })?;
            let mut keys = BTreeSet::new();
            if history.version != 1
                || history.attempts.len() > 16
                || history.attempts.iter().any(|attempt| {
                    let valid_path =
                        attempt
                            .plan
                            .destination
                            .rsplit_once('/')
                            .is_some_and(|(parent, name)| {
                                destination(if parent.is_empty() { "/" } else { parent }, name)
                                    .is_ok_and(|path| path == attempt.plan.destination)
                            });
                    !valid_path
                        || attempt.key.is_empty()
                        || attempt.key.len() > 128
                        || !keys.insert(attempt.key.clone())
                        || attempt.id.len() > 256
                        || !matches!(attempt.plan.program.as_str(), "composer" | "laravel")
                        || attempt.plan.tool_identity.len() > 128 << 10
                        || attempt.plan.tool_help.len() > 128 << 10
                        || attempt.plan.args.len() > 32
                        || attempt
                            .plan
                            .args
                            .iter()
                            .any(|arg| arg.len() > 8192 || arg.contains('\0'))
                })
            {
                return Err(Error::Invalid(crate::ui_text::t(
                    "project.wizard.error.cc0cce6ab9",
                    "Stored project recovery data exceeds supported limits",
                )));
            }
            self.history = history.attempts;
        }
        self.loaded = true;
        Ok(())
    }
    fn save_history(&self) -> bool {
        host::storage::set(
            HISTORY_KEY,
            &serde_json::json!({"version":1,"attempts":self.history}).to_string(),
        )
        .is_ok()
    }
    fn remember(&mut self, key: String, id: String, plan: Plan) {
        while self.history.len() >= 16 {
            let at = self
                .history
                .iter()
                .position(|attempt| attempt.status != Status::Running)
                .unwrap_or(0);
            self.history.remove(at);
        }
        self.history.push(Attempt {
            key,
            id,
            plan,
            status: Status::Running,
        });
    }
    /// Recovery never replays a prior command into a partial directory. It can
    /// open that directory for inspection or launch a wholly new reviewed draft.
    pub(crate) fn recover(&mut self, ctx: &Invocation) -> Result<Option<String>, Error> {
        self.load_history()?;
        if self.history.is_empty() {
            return Ok(Some(
                crate::ui_text::t(
                    "project.wizard.status.179a78d5a6",
                    "No retained project installations are available.",
                )
                .into(),
            ));
        }
        let choices: Vec<_> = self
            .history
            .iter()
            .rev()
            .map(|attempt| {
                PickItem::new(
                    &attempt.key,
                    &format!("{} · {}", attempt.plan.destination, attempt.status.label()),
                )
            })
            .collect();
        let Some(key) = host::pick(
            &crate::ui_text::t(
                "project.wizard.prompt.162b11dc06",
                "Laravel project installation recovery",
            ),
            &choices,
        )?
        else {
            return Ok(None);
        };
        let attempt = self
            .history
            .iter()
            .find(|attempt| attempt.key == key)
            .cloned()
            .ok_or(Error::Stale)?;
        let Some(action) = host::pick(
            &crate::ui_text::t("project.wizard.prompt.6ebb2a4d92", "Recovery action"),
            &[
                PickItem::new(
                    "inspect",
                    &crate::ui_text::t(
                        "project.wizard.option.d72feeb7da",
                        "Open retained directory for inspection",
                    ),
                ),
                PickItem::new(
                    "fresh",
                    &crate::ui_text::t(
                        "project.wizard.option.3044f53cf3",
                        "Create a fresh installation in another destination",
                    ),
                ),
                PickItem::new(
                    "forget",
                    &crate::ui_text::t(
                        "project.wizard.option.ca1c9e2574",
                        "Confirm stopped and remove recovery record",
                    ),
                ),
            ],
        )?
        else {
            return Ok(None);
        };
        match action.as_str() {
            "inspect" => {
                if host::fs::stat(&attempt.plan.destination)?.kind != FileKind::Directory {
                    return Err(Error::Invalid(crate::ui_text::t(
                        "project.wizard.error.eb21a34a5e",
                        "The retained destination is unavailable or is not a regular directory",
                    )));
                }
                let preview = crate::ui_text::format(
                    "project.wizard.format.68d51accc2",
                    "Destination: {destination}\nStatus: {status}\nOriginal program: {program}\nOriginal arguments: {argv}\n\nOpen the retained directory through the normal project trust flow? This does not retry commands or remove files. Inspect the original terminal output; an unresolved installation may still be running.",
                    &[
                        ("destination", &attempt.plan.destination),
                        ("status", attempt.status.label()),
                        ("program", &attempt.plan.program),
                        (
                            "argv",
                            &serde_json::to_string_pretty(&attempt.plan.args).unwrap_or_default(),
                        ),
                    ],
                );
                if !ui_kit::prompts::confirm_exact(
                    &crate::ui_text::t(
                        "project.wizard.prompt.0a00eae80f",
                        "Inspect retained Laravel installation?",
                    ),
                    &preview,
                )? {
                    return Ok(None);
                }
                if host::fs::stat(&attempt.plan.destination)?.kind != FileKind::Directory {
                    return Err(Error::Stale);
                }
                host::open_project(&attempt.plan.destination, true)?;
                Ok(Some(
                    crate::ui_text::t("project.wizard.status.de68480e78", "Opened the retained destination for inspection; nothing was rerun or deleted.")
                        .into(),
                ))
            }
            "fresh" => self.run(ctx),
            "forget" => {
                if self.pending.contains_key(&key) {
                    return Err(Error::Invalid(crate::ui_text::t(
                        "project.wizard.error.b302852f48",
                        "This terminal operation is still tracked as running. Stop or finish its original terminal first.",
                    )));
                }
                if !ui_kit::prompts::confirm_exact(
                    &crate::ui_text::t(
                        "project.wizard.prompt.0299d486d5",
                        "Remove installation recovery record?",
                    ),
                    &crate::ui_text::format(
                        "project.wizard.format.75f39e6ede",
                        "Destination: {destination}\n\nVerify the original installer process has stopped before removing this record. Files and terminal output are retained. Removing the record does not stop a process or make a partial directory safe to reuse. Confirm the process has stopped and forget this record?",
                        &[("destination", &attempt.plan.destination)],
                    ),
                )? {
                    return Ok(None);
                }
                self.history.retain(|existing| existing.key != key);
                let saved = self.save_history();
                Ok(Some(
                    if saved {
                        crate::ui_text::t("project.wizard.status.c6264b8d20", "Recovery record removed; project files are retained.")
                    } else {
                        crate::ui_text::t("project.wizard.status.46f2242d13", "Recovery record removed for this session; private storage is unavailable.")
                    }
                    .into(),
                ))
            }
            _ => Err(Error::Invalid(crate::ui_text::t(
                "project.wizard.error.387711b648",
                "Unknown recovery action",
            ))),
        }
    }
    pub(crate) fn run(&mut self, _ctx: &Invocation) -> Result<Option<String>, Error> {
        self.load_history()?;
        if self.pending.len() >= 8
            || self
                .history
                .iter()
                .filter(|attempt| attempt.status == Status::Running)
                .count()
                >= 8
        {
            return Err(Error::Invalid(crate::ui_text::t(
                "project.wizard.error.fd42434767",
                "Finish an existing project installation first.",
            )));
        }
        let Some(parent) = host::pick_directory(
            &crate::ui_text::t(
                "project.wizard.prompt.740d20e270",
                "New Laravel project: choose parent directory",
            ),
            None,
        )?
        else {
            return Ok(None);
        };
        let Some(name) = host::input(
            &crate::ui_text::t(
                "project.wizard.prompt.abbf22582b",
                "New Laravel project name",
            ),
            Some("laravel-app"),
        )?
        else {
            return Ok(None);
        };
        let target = destination(&parent, name.trim())?;
        if self
            .history
            .iter()
            .any(|attempt| attempt.status == Status::Running && attempt.plan.destination == target)
        {
            return Err(Error::Invalid(crate::ui_text::t(
                "project.wizard.error.bc83d44149",
                "An unresolved installation owns this destination. Inspect installation recovery and verify the prior process has stopped; never retry into a partial directory.",
            )));
        }
        inspect_destination(&parent, &target)?;
        if self
            .pending
            .values()
            .any(|(_, existing)| existing == &target)
        {
            return Err(Error::Invalid(crate::ui_text::t(
                "project.wizard.error.acefa7b867",
                "A project installation is already running for that destination.",
            )));
        }
        let Some(program) = host::pick(
            &crate::ui_text::t(
                "project.wizard.prompt.2e8c3651db",
                "Create Laravel project with",
            ),
            &[
                PickItem::new(
                    "composer",
                    &crate::ui_text::t(
                        "project.wizard.option.188701587d",
                        "Composer: Laravel application skeleton",
                    ),
                ),
                PickItem::new(
                    "laravel",
                    &crate::ui_text::t(
                        "project.wizard.option.3981b8707c",
                        "Laravel installer: available starter kits and packages",
                    ),
                ),
            ],
        )?
        else {
            return Ok(None);
        };
        inspect_tool_destination(&parent, &target, &program)?;
        let identity = tool_output(&program, &["--version"])?;
        let help = if program == "laravel" {
            tool_output("laravel", &["new", "--help"])?
        } else {
            String::new()
        };
        let options = if program == "laravel" {
            let Some(options) = choose_options(&help)? else {
                return Ok(None);
            };
            options
        } else {
            Vec::new()
        };
        let plan = plan(&program, target, identity, &help, &options)?;
        let preview = crate::ui_text::format(
            "project.wizard.format.7a5725b9ec",
            "Destination: {destination}\nTool: {identity}\nProgram: {program}\nExact arguments:\n{argv}\n\nThis installs dependencies and runs the selected installer's project scripts in a terminal. Other installer choices remain interactive. Existing files are never intentionally overwritten. Failed installations and full terminal output are retained for inspection; no automatic cleanup occurs. On successful completion the new project's normal trust/open flow opens in a new window.\n\nStart installation?",
            &[
                ("destination", &plan.destination),
                ("identity", plan.tool_identity.trim()),
                ("program", &plan.program),
                (
                    "argv",
                    &serde_json::to_string_pretty(&plan.args).unwrap_or_default(),
                ),
            ],
        );
        if !ui_kit::prompts::confirm_exact(
            &crate::ui_text::t(
                "project.wizard.prompt.b6576746bf",
                "Create Laravel project?",
            ),
            &preview,
        )? {
            return Ok(None);
        }
        if tool_output(&plan.program, &["--version"])? != plan.tool_identity
            || plan.program == "laravel"
                && tool_output("laravel", &["new", "--help"])? != plan.tool_help
        {
            return Err(Error::Stale);
        }
        inspect_tool_destination(&parent, &plan.destination, &plan.program)?;
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
            &crate::ui_text::format(
                "project.wizard.format.7bea49e8ee",
                "Create Laravel project: {name}",
                &[("name", name.trim())],
            ),
            &host::Command {
                program: plan.program.clone(),
                args: plan.args.clone(),
                env: Vec::new(),
            },
        )?;
        self.pending
            .insert(key.clone(), (id.clone(), plan.destination.clone()));
        self.remember(key, id, plan);
        let persisted = self.save_history();
        Ok(Some(if persisted {crate::ui_text::t("project.wizard.status.bac5cba1bd", "Installation started in its terminal. Output and recovery information are retained.")}else{crate::ui_text::t("project.wizard.status.9d2054113e", "Installation started in its terminal. Output is retained; recovery tracking is available only in this session because private storage is unavailable.")}.into()))
    }

    /// Match both the host operation id and the key; late or foreign exits do
    /// not consume a newer installation or open an unrelated directory.
    pub(crate) fn complete(&mut self, event: &TerminalExited) -> Result<bool, Error> {
        self.load_history()?;
        if !self.pending.contains_key(&event.key) {
            if let Some(attempt) = self.history.iter().find(|attempt| {
                attempt.key == event.key
                    && attempt.id == event.id
                    && attempt.status == Status::Running
            }) {
                self.pending.insert(
                    attempt.key.clone(),
                    (attempt.id.clone(), attempt.plan.destination.clone()),
                );
            }
        }
        let Some((id, _)) = self.pending.get(&event.key) else {
            return Ok(false);
        };
        if id != &event.id {
            return Ok(false);
        }
        let (_, destination) = self.pending.remove(&event.key).ok_or(Error::Stale)?;
        let result = (|| {
            if event.status == Some(0) && !event.closed {
                if host::fs::stat(&destination)?.kind != FileKind::Directory {
                    return Err(Error::Stale);
                }
                for file in ["artisan", "composer.json"] {
                    if host::fs::stat(&format!("{destination}/{file}"))?.kind != FileKind::File {
                        return Err(Error::Invalid(crate::ui_text::t(
                            "project.wizard.error.0c3de226a8",
                            "The installer exited successfully but no Laravel application skeleton was found. Inspect its retained terminal output through installation recovery.",
                        )));
                    }
                }
                host::open_project(&destination, true)?;
            }
            Ok(true)
        })();
        if let Some(attempt) = self
            .history
            .iter_mut()
            .find(|attempt| attempt.key == event.key && attempt.id == event.id)
        {
            attempt.status = if result.is_err() {
                Status::Incomplete
            } else if event.closed {
                Status::Closed
            } else if event.status == Some(0) {
                Status::Completed
            } else {
                Status::Failed
            };
        }
        self.save_history();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_is_absolute_and_never_an_option_or_traversal() {
        let _locale_host = openidea_plugin::mock::MockHost::install();
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
        let _locale_host = openidea_plugin::mock::MockHost::install();
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

    // Option spelling/arity captured from installed Laravel Installer 5.28.1
    // using `laravel new --help`; no application creation was performed.
    const INSTALLER_HELP: &str = "Options:\n --dev Development release\n --git Initialize Git\n --branch=BRANCH Branch\n --database=DATABASE Possible values are: mysql, mariadb, pgsql, sqlite, sqlsrv\n --react React\n --svelte Svelte\n --vue Vue\n --livewire Livewire\n --livewire-class-components Class components\n --workos WorkOS\n --teams Teams\n --no-authentication No auth\n --pest Pest\n --phpunit PHPUnit\n --npm npm\n --pnpm pnpm\n --bun Bun\n --yarn Yarn\n --boost Boost\n --no-boost Skip Boost\n --using[=USING] Custom starter\n -f, --force Force\n";

    #[test]
    fn installed_help_option_shapes_accept_values_and_refuse_unsafe_or_conflicting_options() {
        let _locale_host = openidea_plugin::mock::MockHost::install();
        let options = [
            "--livewire",
            "--livewire-class-components",
            "--database=pgsql",
            "--pest",
            "--pnpm",
            "--no-boost",
            "--git",
            "--branch=feature/setup",
            "--dev",
        ]
        .map(String::from);
        assert!(
            plan(
                "laravel",
                "/tmp/app".into(),
                "5.28.1".into(),
                INSTALLER_HELP,
                &options
            )
            .is_ok()
        );
        assert!(
            plan(
                "laravel",
                "/tmp/app".into(),
                "5.28.1".into(),
                INSTALLER_HELP,
                &["--using=example/starter:^2.0".into()]
            )
            .is_ok()
        );
        for values in [
            vec!["--force"],
            vec!["--github"],
            vec!["--database=oracle"],
            vec!["--database"],
            vec!["--react=value"],
            vec!["--branch=main"],
            vec!["--pest", "--phpunit"],
            vec!["--boost", "--no-boost"],
            vec!["--npm", "--pnpm"],
            vec!["--livewire", "--workos", "--teams"],
            vec!["--react", "--livewire-class-components"],
            vec!["--using=example/package;touch_bad"],
            vec!["--git", "--branch=../bad"],
        ] {
            assert!(
                installer_options(
                    INSTALLER_HELP,
                    &values.into_iter().map(String::from).collect::<Vec<_>>()
                )
                .is_err()
            );
        }
        assert!(
            supported_option(INSTALLER_HELP, "--force"),
            "short aliases are parsed but force is still forbidden"
        );
        assert!(
            !supported_option(INSTALLER_HELP, "--no-node"),
            "newer unsupported flags cannot be guessed"
        );
    }

    #[test]
    fn guided_installer_choices_use_only_discovered_options() {
        let mock = openidea_plugin::mock::MockHost::install();
        for answer in [
            "--svelte",
            "--no-authentication",
            "--database=sqlite",
            "--phpunit",
            "--no-boost",
            "--pnpm",
            "--git",
            "feature/setup",
            "interactive",
        ] {
            mock.answer_next_prompt(Some(&[answer]));
        }
        assert_eq!(
            choose_options(INSTALLER_HELP).unwrap().unwrap(),
            [
                "--svelte",
                "--no-authentication",
                "--database=sqlite",
                "--phpunit",
                "--no-boost",
                "--pnpm",
                "--git",
                "--branch=feature/setup"
            ]
        );
        assert!(mock.process_runs().is_empty());
    }

    #[test]
    fn help_changes_after_review_refuse_installer_launch() {
        let mock = fixture().with_capabilities(&["fs.read:/tmp/projects", "process:laravel"]);
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let captured = calls.clone();
        mock.on_process("laravel", move |args, _| {
            let stdout = if args == ["--version"] {
                "Laravel Installer fixture".into()
            } else {
                captured.set(captured.get() + 1);
                if captured.get() == 1 {
                    "Options:\n --react React\n".into()
                } else {
                    "Options:\n --react Changed\n".into()
                }
            };
            Ok(openidea_plugin::ProcessOutput {
                status: Some(0),
                stdout,
                stderr: String::new(),
            })
        });
        for answer in ["/tmp/projects", "app", "laravel", "interactive", "yes"] {
            mock.answer_next_prompt(Some(&[answer]));
        }
        let mut state = State::default();
        assert!(matches!(state.run(&mock.invocation()), Err(Error::Stale)));
        assert!(state.pending.is_empty());
    }

    #[test]
    fn failed_project_can_be_inspected_after_reload_without_replaying_installation() {
        let mock =
            fixture().with_capabilities(&["fs.read:/tmp/projects", "process:composer", "storage"]);
        answers(&mock, "failed-app", true);
        let mut original = State::default();
        original.run(&mock.invocation()).unwrap();
        let (key, (id, _)) = original
            .pending
            .iter()
            .next()
            .map(|(key, value)| (key.clone(), value.clone()))
            .unwrap();
        mock.add_external_file("/tmp/projects/failed-app/partial", "keep");
        original
            .complete(&TerminalExited {
                key: key.clone(),
                id,
                status: Some(1),
                closed: false,
            })
            .unwrap();
        let runs = mock.process_runs().len();
        for answer in [key.as_str(), "inspect", "yes"] {
            mock.answer_next_prompt(Some(&[answer]));
        }
        State::default().recover(&mock.invocation()).unwrap();
        assert_eq!(
            mock.process_runs().len(),
            runs,
            "inspection must never rerun an installer"
        );
        assert_eq!(
            mock.opened_projects(),
            [("/tmp/projects/failed-app".into(), true)]
        );
        assert_eq!(
            host::fs::read_file("/tmp/projects/failed-app/partial").unwrap(),
            "keep"
        );
    }

    #[test]
    fn unresolved_records_block_destination_reuse_and_matching_exit_survives_reload() {
        let mock =
            fixture().with_capabilities(&["fs.read:/tmp/projects", "process:composer", "storage"]);
        answers(&mock, "app", true);
        let mut original = State::default();
        original.run(&mock.invocation()).unwrap();
        let (key, (id, _)) = original
            .pending
            .iter()
            .next()
            .map(|(key, value)| (key.clone(), value.clone()))
            .unwrap();
        mock.answer_next_prompt(Some(&["/tmp/projects"]));
        mock.answer_next_prompt(Some(&["app"]));
        let mut reloaded = State::default();
        assert!(reloaded.run(&mock.invocation()).is_err());
        assert_eq!(mock.process_runs().len(), 2);
        mock.add_external_file("/tmp/projects/app/artisan", "<?php");
        mock.add_external_file("/tmp/projects/app/composer.json", "{}");
        assert!(
            reloaded
                .complete(&TerminalExited {
                    key,
                    id,
                    status: Some(0),
                    closed: false
                })
                .unwrap()
        );
        assert_eq!(mock.opened_projects(), [("/tmp/projects/app".into(), true)]);
    }

    #[test]
    fn recovery_forget_never_deletes_retained_files() {
        let mock =
            fixture().with_capabilities(&["fs.read:/tmp/projects", "process:composer", "storage"]);
        answers(&mock, "app", true);
        let mut original = State::default();
        original.run(&mock.invocation()).unwrap();
        let key = original.pending.keys().next().unwrap().clone();
        mock.add_external_file("/tmp/projects/app/partial", "keep");
        let mut reloaded = State::default();
        for answer in [key.as_str(), "forget", "yes"] {
            mock.answer_next_prompt(Some(&[answer]));
        }
        reloaded.recover(&mock.invocation()).unwrap();
        assert!(reloaded.history.is_empty());
        assert_eq!(
            host::fs::read_file("/tmp/projects/app/partial").unwrap(),
            "keep"
        );
        assert!(mock.opened_projects().is_empty());
        assert_eq!(mock.process_runs().len(), 2);
    }

    #[test]
    #[ignore = "requires a local Laravel installer; reads version/help only and never creates a project"]
    fn installed_laravel_help_roundtrip_without_installing() {
        let identity = std::process::Command::new("laravel")
            .arg("--version")
            .output()
            .unwrap();
        assert!(identity.status.success());
        let output = std::process::Command::new("laravel")
            .args(["new", "--help", "--no-ansi"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let help = String::from_utf8(output.stdout).unwrap();
        let options = [
            "--react",
            "--database=sqlite",
            "--pest",
            "--no-boost",
            "--git",
            "--branch=main",
        ]
        .map(String::from);
        let plan = plan(
            "laravel",
            "/tmp/laravel-wizard-read-only-fixture".into(),
            String::from_utf8(identity.stdout).unwrap(),
            &help,
            &options,
        )
        .unwrap();
        assert!(plan.args.contains(&"--database=sqlite".into()));
        assert!(plan.args.iter().all(|arg| arg != "--force"));
    }
}
