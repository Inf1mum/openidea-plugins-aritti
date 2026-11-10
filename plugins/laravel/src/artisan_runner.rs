//! Shared explicit runner selection for forms, inspection and agent tools.
use openidea_plugin::{Error, Project};
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
                .ok_or_else(|| Error::Invalid("Refresh local PHP commands first.".into()))?,
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
