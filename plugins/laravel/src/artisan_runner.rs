//! Shared explicit runner selection for forms, inspection and agent tools.
use openidea_plugin::{Error, Project, host};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Runner {
    #[default]
    Php,
    Docker,
}
impl Runner {
    pub fn name(self) -> &'static str {
        match self {
            Self::Php => "php",
            Self::Docker => "docker",
        }
    }
    pub fn catalog(self, project: &Project) -> Result<(Value, String), Error> {
        let snapshot = match self {
            Self::Php => crate::artisan_metadata::snapshot(project)
                .ok_or_else(|| Error::Invalid(crate::ui_text::text("native.artisan_runner.76c500d99d", "Refresh local PHP commands first.").into()))?,
            Self::Docker => crate::artisan_docker::snapshot(project)?,
        };
        let token = crate::artisan_metadata::hash(&snapshot.to_string());
        Ok((snapshot, token))
    }
    pub fn refresh(self, ctx: &openidea_plugin::Invocation) -> Result<Option<String>, Error> {
        match self {
            Self::Php => crate::artisan_metadata::refresh(ctx),
            Self::Docker => crate::artisan_docker::refresh(ctx),
        }
    }
    pub fn argv(
        self,
        project: &Project,
        args: &[String],
        terminal: bool,
    ) -> Result<Vec<String>, Error> {
        match self {
            Self::Php => Ok(args.to_vec()),
            Self::Docker => crate::artisan_docker::argv(project, args, terminal),
        }
    }
    pub fn verify(self, project: &Project) -> Result<(), Error> {
        match self {
            Self::Php => Ok(()),
            Self::Docker => crate::artisan_docker::verify(project),
        }
    }
}

pub(super) const WINDOWLESS_PHP: &str = "php-win";

fn is_windows_root(root: &str) -> bool {
    root.as_bytes().get(1) == Some(&b':') || root.starts_with("\\\\")
}

fn windowless_unavailable(error: &Error) -> bool {
    match error {
        Error::Denied(_) | Error::NotFound(_) => true,
        Error::Failed(message) => message.starts_with(&format!("process:{WINDOWLESS_PHP}")),
        _ => false,
    }
}

/// Starts a background process; PHP on a Windows project goes through `php-win`
/// (no console window) when `process:php-win` is granted, otherwise `program`.
pub(super) fn start_background(
    program: &str,
    args: &[String],
    project: &Project,
) -> Result<host::Child, Error> {
    if program == "php" && project.root().is_ok_and(|root| is_windows_root(&root)) {
        match host::start(&host::Command::new(WINDOWLESS_PHP).args(args), Some(project)) {
            Err(error) if windowless_unavailable(&error) => {}
            other => return other,
        }
    }
    host::start(&host::Command::new(program).args(args), Some(project))
}

#[cfg(test)]
mod windowless_tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    fn started_program(host: &MockHost, root: &str) -> String {
        host.set_project_root(Some(root));
        let project = host::projects().unwrap().remove(0);
        let child = start_background("php", &["-v".to_owned()], &project).unwrap();
        child.wait(0).unwrap().stdout
    }

    #[test]
    fn windows_project_uses_php_win_when_granted() {
        let host = MockHost::install().with_capabilities(&["project.read", "process:php", "process:php-win"]);
        assert_eq!(started_program(&host, "C:\\Users\\dev\\shop"), "started: php-win\n");
    }

    #[test]
    fn windows_project_falls_back_to_php_without_the_grant() {
        let host = MockHost::install().with_capabilities(&["project.read", "process:php"]);
        assert_eq!(started_program(&host, "C:\\Users\\dev\\shop"), "started: php\n");
    }

    #[test]
    fn unix_project_keeps_php() {
        let host = MockHost::install().with_capabilities(&["project.read", "process:php", "process:php-win"]);
        assert_eq!(started_program(&host, "/home/dev/app"), "started: php\n");
        assert!(is_windows_root("\\\\?\\UNC\\wsl\\app"));
    }
}
