//! Existing Dockerfile references for reviewed config creation. No parsing,
//! Dockerfile writes or build. Source/stat/innermost owner are checked twice.
use openidea_plugin::{Error, FileKind, FileStat, Project};

const MAX_BYTES: usize = 8 * 1024;
pub(crate) struct Snapshot {
    root: String,
    path: String,
    source: String,
    metadata: FileStat,
}

impl Snapshot {
    pub(crate) fn capture(project: &Project, input: &str) -> Result<Self, Error> {
        if input.len() > 512 || input.chars().any(char::is_control) {
            return Err(
                "Use a project Dockerfile path up to 512 bytes without control characters.".into(),
            );
        }
        let root = project.root()?;
        let path = crate::configs::relative(&root, input)?;
        crate::configs::ensure_owned(project, &path)?;
        let metadata = project.stat(&path)?;
        if metadata.kind != FileKind::File || metadata.size > MAX_BYTES as u64 {
            return Err("Choose an existing Dockerfile no larger than 8 KiB so its complete text can be reviewed.".into());
        }
        let source = project.read_file(&path)?;
        if source.is_empty()
            || source.len() > MAX_BYTES
            || source
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            return Err("Dockerfile review requires nonempty text up to 8 KiB without unsupported control characters.".into());
        }
        Ok(Self {
            root,
            path,
            source,
            metadata,
        })
    }

    pub(crate) fn validate(&self, project: &Project) -> Result<(), Error> {
        crate::configs::ensure_owned(project, &self.path)?;
        if project.root()? != self.root
            || project.stat(&self.path)? != self.metadata
            || project.read_file(&self.path)? != self.source
        {
            return Err(Error::Stale);
        }
        Ok(())
    }

    pub(crate) fn configuration(&self, target: &str, name: &str) -> Result<String, Error> {
        let mut value: serde_json::Value =
            serde_json::from_str(&crate::create::source("base", name)?)
                .map_err(|_| Error::Failed("Bundled base template is invalid.".into()))?;
        let folder: Vec<_> = target.split('/').collect();
        let folder = &folder[..folder.len().saturating_sub(1)];
        let file: Vec<_> = self.path.split('/').collect();
        let common = folder
            .iter()
            .zip(file.iter())
            .take_while(|(a, b)| a == b)
            .count();
        let mut relative = vec![".."; folder.len() - common];
        relative.extend_from_slice(&file[common..]);
        let context = if folder.is_empty() {
            ".".to_owned()
        } else {
            vec![".."; folder.len()].join("/")
        };
        value
            .as_object_mut()
            .ok_or("Bundled base template must be an object.")?
            .remove("image");
        value["build"] = serde_json::json!({"dockerfile":relative.join("/"),"context":context});
        Ok(format!(
            "{}\n",
            serde_json::to_string_pretty(&value).map_err(|_| "Cannot render the configuration.")?
        ))
    }

    pub(crate) fn preview(&self) -> String {
        format!(
            "Existing Dockerfile snapshot: {}\n--- complete current text ---\n{}\n--- end Dockerfile ---\nBuild context is the entire project root, subject to Docker's own ignore rules. Only the Dockerfile text/owner/stat are rechecked; included files, build stages/args, base images, syntax, compatibility and build success are not verified. Save Dockerfile/editor changes before Start. Creating this configuration does not build or edit the Dockerfile.",
            serde_json::to_string(&format!(
                "{}/{}",
                self.root.trim_end_matches('/'),
                self.path
            ))
            .unwrap_or_default(),
            self.source
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    #[test]
    fn paths_are_relative_to_config_not_context_and_reference_existing_file_only() {
        let host = MockHost::install();
        host.add_file(".devcontainer/Dockerfile", "FROM ubuntu:24.04\n");
        let p = host.invocation().project().unwrap();
        let snap = Snapshot::capture(&p, ".devcontainer/Dockerfile").unwrap();
        for (target, file, context) in [
            (".devcontainer.json", ".devcontainer/Dockerfile", "."),
            (".devcontainer/devcontainer.json", "Dockerfile", ".."),
            (
                ".devcontainer/team/devcontainer.json",
                "../Dockerfile",
                "../..",
            ),
        ] {
            let v: serde_json::Value =
                serde_json::from_str(&snap.configuration(target, "Project").unwrap()).unwrap();
            assert_eq!(v["build"]["dockerfile"], file);
            assert_eq!(v["build"]["context"], context);
            assert!(v.get("image").is_none());
        }
        assert!(snap.preview().contains("FROM ubuntu:24.04") && host.process_runs().is_empty());
    }
    #[test]
    fn source_owner_stat_and_project_changes_invalidate_snapshot() {
        let host = MockHost::install();
        host.add_file("containers/Dockerfile", "FROM scratch\n");
        let p = host.invocation().project().unwrap();
        let snap = Snapshot::capture(&p, "containers/Dockerfile").unwrap();
        snap.validate(&p).unwrap();
        host.add_file("containers/Dockerfile", "FROM busybox\n");
        assert!(snap.validate(&p).is_err());
        host.add_file("containers/Dockerfile", "FROM scratch\n");
        host.open_editor(
            "/project/containers/Dockerfile",
            "dockerfile",
            "FROM unsaved\n",
            &[],
        );
        assert!(snap.validate(&p).is_err());
        host.add_project("/project/containers");
        assert!(Snapshot::capture(&p, "containers/Dockerfile").is_err());
        assert!(snap.validate(&p).is_err());
        host.set_project_root(Some("/other"));
        assert!(
            snap.validate(&host.invocation().project().unwrap())
                .is_err()
        );
    }
    #[test]
    fn only_bounded_owned_existing_regular_text_is_admitted() {
        let host = MockHost::install();
        let p = host.invocation().project().unwrap();
        for (path, text) in [
            ("empty", ""),
            ("large", &"x".repeat(MAX_BYTES + 1)),
            ("control", "FROM scratch\u{1b}[31m"),
        ] {
            host.add_file(path, text);
            assert!(Snapshot::capture(&p, path).is_err());
        }
        for path in [
            "../Dockerfile",
            "/other/Dockerfile",
            "missing",
            "Dockerfile\n",
        ] {
            assert!(Snapshot::capture(&p, path).is_err());
        }
        host.add_file("folder/file", "x");
        assert!(Snapshot::capture(&p, "folder").is_err());
        assert!(host.process_runs().is_empty() && host.terminals().is_empty());
    }
}
