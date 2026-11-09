//! Start/Rebuild command preview. No process runs until the concrete target and
//! argument vector are accepted and the project/configuration snapshot rechecked.
use openidea_plugin::{Error, FileKind, FileStat, Invocation, Project, host};

pub(crate) struct Plan {
    root: String,
    config: String,
    source: String,
    metadata: FileStat,
    rebuild: bool,
    args: Vec<String>,
}

impl Plan {
    pub(crate) fn prepare(project: &Project, kind: &str, config: &str) -> Result<Self, Error> {
        if !matches!(kind, "up" | "rebuild") {
            return Err(Error::Invalid("Only Start and Rebuild have this preview.".into()));
        }
        let root = project.root()?;
        let config = crate::configs::relative(&root, config)?;
        crate::configs::ensure_owned(project, &config)?;
        let metadata = project.stat(&config)?;
        if metadata.kind != FileKind::File {
            return Err(Error::Invalid("The selected configuration is not a file.".into()));
        }
        if metadata.size > crate::MAX_BYTES as u64 {
            return Err(Error::Invalid("Command preview supports configurations up to 256 KiB.".into()));
        }
        let source = project.read_file(&config)?;
        if source.len() > crate::MAX_BYTES {
            return Err(Error::Invalid("Command preview supports configurations up to 256 KiB.".into()));
        }
        let rebuild = kind == "rebuild";
        let mut args = vec!["up".into(), "--workspace-folder".into(), root.clone(), "--config".into(), config.clone()];
        if rebuild {
            args.push("--remove-existing-container".into());
        }
        Ok(Self { root, config, source, metadata, rebuild, args })
    }

    pub(crate) fn title(&self) -> &'static str {
        if self.rebuild { "Rebuild Dev Container" } else { "Start Dev Container" }
    }

    pub(crate) fn preview(&self) -> String {
        let quote = |s: &str| serde_json::to_string(s).unwrap_or_default();
        format!(
            "Project: {}\nConfiguration: {}\nConfiguration snapshot: {} bytes\nProgram: devcontainer\nArguments (JSON array):\n{}\n\n{}\nThe CLI reads saved files from disk. Save configuration changes first. Referenced Dockerfiles, Compose files, features and lifecycle commands can also execute. Continuing runs this CLI command.",
            quote(&self.root),
            quote(&self.config),
            self.source.len(),
            serde_json::to_string_pretty(&self.args).unwrap_or_default(),
            if self.rebuild {
                "The existing container will be removed and built again."
            } else {
                "Build or start the development container."
            },
        )
    }

    fn validate(&self, ctx: &Invocation) -> Result<(), Error> {
        let project = ctx.project().ok_or(Error::Stale)?;
        crate::configs::ensure_owned(&project, &self.config)?;
        if project.root()? != self.root || project.stat(&self.config)? != self.metadata || project.read_file(&self.config)? != self.source {
            return Err(Error::Invalid("The project or configuration changed after preview. Run the command again.".into()));
        }
        Ok(())
    }

    pub(crate) fn launch(self, ctx: &Invocation) -> Result<(), Error> {
        self.validate(ctx)?;
        let title = if self.rebuild { "Dev Container: Rebuild Container" } else { "Dev Container: Start Container" };
        host::open_terminal(title, "devcontainer", &self.args)
    }

    pub(crate) fn launch_managed(self, ctx: &Invocation, operations: &mut crate::operations::Operations) -> Result<(), Error> {
        self.validate(ctx)?;
        let project = ctx.project().ok_or(Error::Stale)?;
        operations.start(&project, self.config, self.args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    fn setup() -> MockHost {
        let host = MockHost::install().with_capabilities(&["project.read", "process:devcontainer"]);
        host.add_file(".devcontainer/devcontainer.json", "{}");
        host
    }

    #[test]
    fn concrete_preview_and_argv_preserve_spaces_quotes_and_rebuild_effect() {
        let host = setup();
        host.set_project_root(Some("/work/project 'one'"));
        host.add_file(".devcontainer/team \"a\"/devcontainer.json", "{}");
        let plan = Plan::prepare(&host.invocation().project().unwrap(), "rebuild", ".devcontainer/team \"a\"/devcontainer.json").unwrap();
        let preview = plan.preview();
        assert!(preview.contains("/work/project 'one'"));
        assert!(preview.contains("team \\\"a\\\""));
        assert!(preview.contains("--remove-existing-container") && preview.contains("removed"));
        plan.launch(&host.invocation()).unwrap();
        assert_eq!(host.terminals()[0].args[2], "/work/project 'one'");
        assert_eq!(host.terminals()[0].args[4], ".devcontainer/team \"a\"/devcontainer.json");
        assert!(host.process_runs().is_empty());
    }

    #[test]
    fn changed_config_or_project_after_preview_starts_nothing() {
        let host = setup();
        let plan = Plan::prepare(&host.invocation().project().unwrap(), "up", ".devcontainer/devcontainer.json").unwrap();
        // Same-length replacement exercises content comparison, not merely size.
        host.add_file(".devcontainer/devcontainer.json", "[]");
        assert!(plan.launch(&host.invocation()).is_err());
        let plan = Plan::prepare(&host.invocation().project().unwrap(), "up", ".devcontainer/devcontainer.json").unwrap();
        host.set_project_root(Some("/second"));
        assert!(plan.launch(&host.invocation()).is_err());
        assert!(host.terminals().is_empty());
    }

    #[test]
    fn current_unsaved_configuration_changes_invalidate_preview() {
        let host = setup();
        let plan = Plan::prepare(&host.invocation().project().unwrap(), "up", ".devcontainer/devcontainer.json").unwrap();
        host.open_editor("/project/.devcontainer/devcontainer.json", "jsonc", "{\"image\":\"other\"}", &[]);
        assert!(plan.launch(&host.invocation()).is_err());
        assert!(host.terminals().is_empty());
    }

    #[test]
    fn rejects_outside_project_missing_and_oversized_configurations() {
        let host = setup();
        let project = host.invocation().project().unwrap();
        for path in ["../outside.json", "/elsewhere/devcontainer.json", "missing.json"] {
            assert!(Plan::prepare(&project, "up", path).is_err());
        }
        host.add_file("large.json", &"x".repeat(crate::MAX_BYTES + 1));
        assert!(Plan::prepare(&project, "up", "large.json").is_err());
        assert!(Plan::prepare(&project, "unknown", ".devcontainer/devcontainer.json").is_err());
        assert!(host.terminals().is_empty());
    }
    #[test]
    fn closing_project_or_revoking_read_permission_prevents_launch() {
        let host = setup();
        let plan = Plan::prepare(&host.invocation().project().unwrap(), "up", ".devcontainer/devcontainer.json").unwrap();
        host.set_project_root(None);
        assert!(plan.launch(&host.invocation()).is_err());
        host.set_project_root(Some("/project"));
        let plan = Plan::prepare(&host.invocation().project().unwrap(), "up", ".devcontainer/devcontainer.json").unwrap();
        let host = host.with_capabilities(&[]);
        assert!(matches!(plan.launch(&host.invocation()), Err(Error::Denied(_))));
        assert!(host.terminals().is_empty());
    }

    #[test]
    fn nested_project_cannot_supply_or_reown_a_reviewed_configuration() {
        let host = setup();
        host.add_file(".devcontainer/team/devcontainer.json", "{}");
        let project = host.invocation().project().unwrap();
        let plan = Plan::prepare(&project, "up", ".devcontainer/team/devcontainer.json").unwrap();
        host.add_project("/project/.devcontainer/team");
        assert!(Plan::prepare(&project, "up", ".devcontainer/team/devcontainer.json").is_err());
        assert!(plan.launch(&host.invocation()).is_err());
        assert!(host.terminals().is_empty() && host.process_runs().is_empty());
    }
}
