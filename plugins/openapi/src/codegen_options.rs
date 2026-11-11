//! Installed generator option discovery and project-owned custom templates.
use crate::*;
use openidea_plugin::host::Command;
#[derive(Default)]
pub(crate) struct Options {
    pub values: Vec<(String, String)>,
    pub templates: Option<String>,
    pub engine: Option<String>,
}
fn catalog(output: &str) -> Result<Vec<(String, String)>, Error> {
    if output.len() > 128 << 10 {
        return Err("Generator option help exceeds128KiB.".into());
    }
    let (_, body) = output
        .split_once("CONFIG OPTIONS")
        .ok_or("Unknown installed generator option-help format.")?;
    let mut options: Vec<(String, String)> = Vec::new();
    for line in body.lines() {
        if let Some(name) = line
            .strip_prefix('\t')
            .filter(|name| !name.is_empty() && !name.starts_with(char::is_whitespace))
        {
            if name.len() > 128
                || !name.starts_with(|c: char| c.is_ascii_alphabetic())
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                || options.iter().any(|(key, _)| key == name)
            {
                return Err("Ambiguous generator option names.".into());
            }
            options.push((name.into(), String::new()));
            if options.len() > 256 {
                return Err("Generator declares more than256 options.".into());
            }
        } else if let Some((_, description)) = options.last_mut() {
            if description.len() < 4096 {
                description.push_str(line.trim());
                description.push(' ');
            }
        }
    }
    Ok(options)
}
fn relative_template(root: &str, input: &str) -> Result<String, Error> {
    if input.is_empty()
        || input.len() > 1024
        || input.starts_with('/')
        || input.split('/').any(|part| {
            part.is_empty()
                || matches!(part, "." | "..")
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        })
    {
        return Err("Template directory must be a project-relative physical path.".into());
    }
    Ok(format!("{}/{input}", root.trim_end_matches('/')))
}
pub(crate) fn collect(project: &Project, root: &str, generator: &str) -> Result<Options, Error> {
    let help = project.run_process(
        "swagger-codegen",
        &["config-help".into(), "-l".into(), generator.into()],
        None,
    )?;
    if help.status != Some(0) {
        return Err("Installed generator could not describe its configuration.".into());
    }
    let available = catalog(&help.stdout)?;
    let items: Vec<_> = available
        .iter()
        .map(|(name, description)| {
            let description: String = description.chars().take(500).collect();
            PickItem::new(name, name).description(&description)
        })
        .collect();
    let keys = host::pick_many(
        "Installed generator options to set (empty uses defaults)",
        &items,
    )?
    .ok_or(Error::Cancelled)?;
    let mut options = Options::default();
    let mut selected = std::collections::HashSet::new();
    for key in keys {
        let (_, description) = available
            .iter()
            .find(|(name, _)| name == &key)
            .filter(|_| selected.insert(key.clone()))
            .ok_or(Error::Stale)?;
        let description: String = description.chars().take(300).collect();
        let value = host::input(&format!("{key}: {description}"), None)?.ok_or(Error::Cancelled)?;
        if value.len() > 2048 || value.chars().any(char::is_control) || value.contains([',', '=']) {
            return Err("Option values must be at most2048 bytes without controls, comma or equals; use a native CLI config file for more complex values.".into());
        }
        options.values.push((key, value));
    }
    options.values.sort();
    let directory = host::input(
        "Custom template directory, relative to project (empty uses bundled templates)",
        Some(""),
    )?
    .ok_or(Error::Cancelled)?;
    if !directory.is_empty() {
        let path = relative_template(root, &directory)?;
        if host::project_of(&path)
            .and_then(|p| p.root().ok())
            .as_deref()
            != Some(root)
            || project.stat(&path)?.kind != openidea_plugin::FileKind::Directory
        {
            return Err("Custom template directory is not owned by this project.".into());
        }
        let items = [
            PickItem::new("default", "Generator default template engine"),
            PickItem::new("mustache", "Mustache"),
            PickItem::new("handlebars", "Handlebars"),
        ];
        let engine =
            host::pick("Installed CLI template engine", &items)?.ok_or(Error::Cancelled)?;
        if !items.iter().any(|item| item.key == engine) {
            return Err(Error::Stale);
        }
        if engine != "default" {
            options.engine = Some(engine);
        }
        options.templates = Some(path);
    }
    Ok(options)
}
impl Options {
    pub(crate) fn apply(&self, mut command: Command) -> Command {
        if !self.values.is_empty() {
            command = command.arg("--additional-properties").arg(
                &self
                    .values
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        if let Some(path) = &self.templates {
            command = command.arg("--template-dir").arg(path);
        }
        if let Some(engine) = &self.engine {
            command = command.arg("--template-engine").arg(engine);
        }
        command
    }
    pub(crate) fn summary(&self) -> String {
        format!(
            "Options: {}\nTemplates: {}\nTemplate engine: {}",
            if self.values.is_empty() {
                "generator defaults".into()
            } else {
                self.values
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            },
            self.templates.as_deref().unwrap_or("bundled"),
            self.engine.as_deref().unwrap_or("generator default")
        )
    }
    pub(crate) fn validate(&self, project: &Project, root: &str) -> Result<(), Error> {
        if let Some(path) = &self.templates {
            if host::project_of(path)
                .and_then(|p| p.root().ok())
                .as_deref()
                != Some(root)
                || project.stat(path)?.kind != openidea_plugin::FileKind::Directory
            {
                return Err(Error::Stale);
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{ProcessOutput, mock::MockHost};
    #[test]
    fn discovers_known_options_and_constructs_separate_cli_arguments() {
        let h = MockHost::install().with_capabilities(&["project.read", "process:swagger-codegen"]);
        h.add_dir("/project/codegen-templates");
        h.on_process("swagger-codegen",|args,_|{assert_eq!(args,["config-help","-l","java"]);Ok(ProcessOutput{status:Some(0),stdout:"log\nCONFIG OPTIONS\n\tapiPackage\n\t    Generated API package\n\tlibrary\n\t    Library template (jersey2, okhttp-gson)\n".into(),stderr:String::new()})});
        h.answer_next_prompt(Some(&["apiPackage"]));
        h.answer_next_prompt(Some(&["com.example.api"]));
        h.answer_next_prompt(Some(&["codegen-templates"]));
        h.answer_next_prompt(Some(&["handlebars"]));
        let project = host::projects().unwrap().remove(0);
        let options = collect(&project, "/project", "java").unwrap();
        assert_eq!(
            options.apply(Command::new("swagger-codegen")).args,
            [
                "--additional-properties",
                "apiPackage=com.example.api",
                "--template-dir",
                "/project/codegen-templates",
                "--template-engine",
                "handlebars"
            ]
        );
    }
    #[test]
    fn ambiguous_catalog_and_escaping_templates_are_refused() {
        assert!(catalog("CONFIG OPTIONS\n\tapiPackage\n\tapiPackage\n").is_err());
        assert!(catalog("not help").is_err());
        for path in [
            "../templates",
            "/tmp/templates",
            "templates/../outside",
            "a;echo",
        ] {
            assert!(relative_template("/project", path).is_err());
        }
    }
}
