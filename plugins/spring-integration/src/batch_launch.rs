//! Reviewed generation of a Spring Boot batch launch command. Nothing runs before confirmation; no datasource access.
use openidea_plugin::{Error, FileKind, Invocation, PickItem, host};

const JOB_NAME: &str = "spring.batch.job.name";
const MAX_PARAMS: usize = 16;

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

fn jar_path(path: &str) -> bool {
    path.ends_with(".jar")
        && path.len() <= 512
        && !path.starts_with('/')
        && !path.contains(['\\', ':', '*', '?', '[', ']', '{', '}', ' '])
        && !path.chars().any(char::is_control)
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

fn job_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('-')
        && !name.chars().any(|c| c.is_whitespace() || c.is_control() || "'\"`$;&|<>\\*".contains(c))
}

/// Spring Boot docs: batch arguments use the plain `name=value` form, without `--`; `--` arguments become Environment properties.
fn parameters(typed: &str) -> Result<Vec<String>, Error> {
    let tokens: Vec<_> = typed.split_whitespace().collect();
    if tokens.len() > MAX_PARAMS {
        return Err(invalid("At most 16 job parameters."));
    }
    for token in &tokens {
        let Some((key, value)) = token.split_once('=') else {
            return Err(invalid("Job parameters use the plain name=value form."));
        };
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) || key.starts_with('-') {
            return Err(invalid("Job parameter names use letters, digits, '.', '_' and '-' and must not start with '-'."));
        }
        if value.is_empty() || value.len() > 256 || value.chars().any(|c| c.is_control() || "'\"`$;&|<>\\*".contains(c)) {
            return Err(invalid("Job parameter values must be non-empty without quotes, shell characters or whitespace."));
        }
    }
    Ok(tokens.into_iter().map(str::to_owned).collect())
}

fn arguments(as_system_property: bool, jar: &str, job: &str, params: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    if as_system_property {
        out.push(format!("-D{JOB_NAME}={job}"));
    }
    out.push("-jar".into());
    out.push(jar.into());
    if !as_system_property {
        out.push(format!("--{JOB_NAME}={job}"));
    }
    out.extend(params.iter().cloned());
    out
}

pub(crate) fn run(ctx: &Invocation, jobs: &[String]) -> Result<Option<String>, Error> {
    let project = ctx.project().ok_or(Error::Stale)?;
    let root = project.root()?;
    if jobs.is_empty() {
        return Err(invalid("No literal Batch job name is proved in this project's source."));
    }
    let items: Vec<_> = jobs.iter().map(|j| PickItem::new(j, j)).collect();
    let Some(job) = host::pick("Launch which Batch job?", &items)? else {
        return Ok(None);
    };
    if !jobs.contains(&job) || !job_name(&job) {
        return Err(Error::Stale);
    }
    let forms = [
        PickItem::new("argument", "--spring.batch.job.name=<job> (program argument)"),
        PickItem::new("property", "-Dspring.batch.job.name=<job> (JVM system property)"),
    ];
    let Some(form) = host::pick("How should the job name be passed?", &forms)? else {
        return Ok(None);
    };
    let Some(relative) = host::input("Executable Spring Boot JAR (project-relative, e.g. target/app.jar)", None)? else {
        return Ok(None);
    };
    if !jar_path(&relative) {
        return Err(invalid("Use a literal project-relative .jar path."));
    }
    let jar = format!("{root}/{relative}");
    if project.stat(&jar)?.kind != FileKind::File {
        return Err(invalid("The JAR does not exist in this project."));
    }
    let Some(typed) = host::input("Job parameters (optional, name=value; to restart, repeat every parameter of the failed run)", None)? else {
        return Ok(None);
    };
    let params = parameters(&typed)?;
    let launch = arguments(form == "property", &jar, &job, &params);
    let line = format!("java {}", launch.join(" "));
    if !host::confirm(
        "Launch this Spring Batch job?",
        &format!("{line}\n\nRestarting a failed job needs the same job name and all parameters; non-identifying ones are not copied from the previous run. Runs in a terminal tab through your approved process grant."),
    )? {
        return Ok(None);
    }
    host::open_terminal("Spring Batch", "java", &launch)?;
    Ok(Some(line))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    #[test]
    fn forms_follow_documented_flags_and_keep_plain_parameters() {
        let params = parameters("run.id=3 date=2026-01-01,java.time.LocalDate,false").unwrap();
        assert_eq!(
            arguments(false, "/p/app.jar", "etl", &params),
            ["-jar", "/p/app.jar", "--spring.batch.job.name=etl", "run.id=3", "date=2026-01-01,java.time.LocalDate,false"]
        );
        assert_eq!(arguments(true, "/p/app.jar", "etl", &[])[..2], ["-Dspring.batch.job.name=etl", "-jar"]);
    }

    #[test]
    fn rejects_unsafe_input() {
        for bad in ["--server.port=1", "a", "a=", "a=$(x)", "-x=1", "a=b;c"] {
            assert!(parameters(bad).is_err(), "{bad}");
        }
        assert!(parameters(&"a=1 ".repeat(17)).is_err());
        for bad in ["../a.jar", "/a.jar", "a.war", "a b.jar", "a//b.jar"] {
            assert!(!jar_path(bad), "{bad}");
        }
        assert!(jar_path("target/app.jar"));
        assert!(!job_name("-x") && !job_name("a b") && job_name("endOfDay"));
    }

    #[test]
    fn confirmed_command_opens_terminal_and_cancel_does_not() {
        let host = MockHost::install().with_capabilities(&["project.read", "process:java"]);
        host.add_file("target/app.jar", "x");
        let ctx = host.invocation();
        let go = |answers: &[Option<&[&str]>]| {
            for a in answers {
                host.answer_next_prompt(*a);
            }
            run(&ctx, &["etl".to_owned()])
        };
        assert!(run(&ctx, &[]).is_err());
        assert!(go(&[Some(&["etl"]), Some(&["property"]), Some(&["target/app.jar"]), Some(&["id=1"]), None]).unwrap().is_none());
        assert!(host.terminals().is_empty());
        let line = go(&[Some(&["etl"]), Some(&["argument"]), Some(&["target/app.jar"]), Some(&["id=1"]), Some(&[])]).unwrap().unwrap();
        assert!(line.contains("--spring.batch.job.name=etl id=1"), "{line}");
        assert_eq!(host.terminals().len(), 1);
        assert_eq!(host.terminals()[0].program, "java");
        assert!(go(&[Some(&["etl"]), Some(&["argument"]), Some(&["../x.jar"])]).is_err());
    }
}
