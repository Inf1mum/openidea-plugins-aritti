//! Queue metadata, observed worker processes, selected-log tails and reviewed retry.
//! All probes are explicit. Runtime arguments remain argv data, never interpolated PHP.
use super::{
    MAX, State, execute, key, owner, parse as parse_runtime, stamp, string, target_binding,
};
use crate::artisan_runner::Runner;
use openidea_plugin::{Error, FileKind, Invocation, Project};
use serde_json::{Value, json};

pub(super) fn probe_args(section: &str, options: &Value) -> Result<Vec<String>, Error> {
    if !matches!(section, "queue-status" | "workers" | "logs" | "schema" | "database-sources") {
        return Err(Error::Stale);
    }
    Ok(vec![
        "-r".into(),
        include_str!("runtime_probe.php")
            .trim_start_matches("<?php")
            .into(),
        section.into(),
        options.to_string(),
    ])
}
fn row(section: &str, index: usize, title: &str, resource: String, details: String) -> Value {
    json!({"key":format!("runtime:{section}:{index}"),"cells":[title,resource,details],"activationEnabled":false})
}
fn count(value: &Value) -> Result<String, Error> {
    if value.is_null() {
        return Ok("unavailable".into());
    }
    let count = value
        .as_u64()
        .ok_or("Queue counts must be nonnegative integers or null.")?;
    Ok(count.to_string())
}
pub(super) fn parse(section: &str, raw: &str) -> Result<Vec<Value>, Error> {
    let value = crate::strict_json::parse(raw, MAX)?;
    let mut rows = vec![];
    if section == "queue-status" {
        let queues = value["queues"]
            .as_array()
            .filter(|queues| queues.len() <= 32)
            .ok_or("Expected up to 32 queue connections.")?;
        for (index, queue) in queues.iter().enumerate() {
            let status = queue["status"]
                .as_str()
                .filter(|status| {
                    ["available", "partial", "unavailable", "unsupported"].contains(status)
                })
                .ok_or("Invalid queue availability.")?;
            let connection = string(queue.get("connection").ok_or("Missing queue connection.")?)?;
            let name = string(queue.get("queue").ok_or("Missing queue name.")?)?;
            let driver = string(queue.get("driver").ok_or("Missing queue driver.")?)?;
            let details = format!(
                "{driver} · {status} · total {} · pending {} · reserved {} · delayed {} · running {} · {}",
                count(&queue["total"])?,
                count(&queue["pending"])?,
                count(&queue["reserved"])?,
                count(&queue["delayed"])?,
                count(&queue["running"])?,
                string(&queue["note"])?
            );
            rows.push(row(
                section,
                index,
                "Queues",
                format!("{connection} / {name}"),
                details,
            ));
        }
    } else if section == "workers" {
        let status = value["status"]
            .as_str()
            .filter(|status| ["available", "unavailable", "incomplete"].contains(status))
            .ok_or("Invalid worker visibility.")?;
        let workers = value["workers"]
            .as_array()
            .filter(|workers| workers.len() <= 256)
            .ok_or("Expected up to 256 observed workers.")?;
        rows.push(row(
            section,
            0,
            "Workers",
            "Visibility".into(),
            format!("{status} · {}", string(&value["note"])?),
        ));
        for worker in workers {
            let pid = worker["pid"]
                .as_u64()
                .filter(|pid| *pid > 0)
                .ok_or("Invalid worker PID.")?;
            let command = worker["command"]
                .as_str()
                .filter(|command| {
                    [
                        "queue:work",
                        "queue:listen",
                        "horizon",
                        "horizon:work",
                        "horizon:supervisor",
                    ]
                    .contains(command)
                })
                .ok_or("Invalid worker command.")?;
            rows.push(row(
                section,
                rows.len(),
                "Workers",
                format!("PID {pid}"),
                format!("{command} · observed in the selected PHP runner"),
            ));
        }
        if workers.is_empty() && status == "available" {
            rows.push(row(
                section,
                rows.len(),
                "Workers",
                "No observed project workers".into(),
                "No matching readable Artisan worker process was present at capture time.".into(),
            ));
        }
    } else if section == "logs" {
        let file = string(value.get("file").ok_or("Missing selected log file.")?)?;
        let lines = value["lines"]
            .as_array()
            .filter(|lines| lines.len() <= 200)
            .ok_or("Expected at most 200 selected log lines.")?;
        let truncated = value["truncated"]
            .as_bool()
            .ok_or("Missing log truncation state.")?;
        rows.push(row(
            section,
            0,
            "Logs",
            file.clone(),
            format!(
                "Selected file tail · at most 64 KiB / 200 lines{}",
                if truncated {
                    " · earlier content omitted"
                } else {
                    ""
                }
            ),
        ));
        for (index, line) in lines.iter().enumerate() {
            let line = line
                .as_str()
                .filter(|line| line.len() <= 65536)
                .ok_or("Log line exceeds the tail limit.")?;
            let mut end = line.len().min(4096);
            while !line.is_char_boundary(end) {
                end -= 1;
            }
            let mut display: String = line[..end]
                .chars()
                .map(|character| {
                    if character.is_control() && character != '\t' {
                        '�'
                    } else {
                        character
                    }
                })
                .collect();
            if end < line.len() {
                display.push_str("… [line truncated]");
            }
            rows.push(row(
                section,
                index + 1,
                "Logs",
                format!("{file} · tail line {}", index + 1),
                display,
            ));
        }
        let text: Vec<String> = lines.iter().filter_map(|line|line.as_str().map(str::to_owned)).collect();
        rows.extend(super::errors::rows(&text, truncated));
    } else {
        return Err(Error::Stale);
    }
    Ok(rows)
}
fn selected_log(project: &Project, file: &str) -> Result<String, Error> {
    if file.len() > 1024
        || !file.starts_with("storage/logs/")
        || !file.ends_with(".log")
        || file.chars().any(char::is_control)
        || file
            .split(['/', '\\'])
            .any(|part| matches!(part, "." | ".." | ""))
        || file.contains('\\')
    {
        return Err(Error::Invalid(
            "Select one project storage/logs/*.log file.".into(),
        ));
    }
    let path = format!("{}/{file}", project.root()?);
    if !crate::owns_current_path(project, &path) || project.stat(&path)?.kind != FileKind::File {
        return Err(Error::Stale);
    }
    Ok(path)
}
impl State {
    pub fn capture_queue_status(
        &mut self,
        ctx: &Invocation,
        runner: Runner,
        connection: Option<&str>,
        queue: Option<&str>,
    ) -> Result<(), Error> {
        let mut options = json!({});
        for (name, value, limit) in [("connection", connection, 128), ("queue", queue, 256)] {
            if let Some(value) = value {
                if value.is_empty() || value.len() > limit || value.chars().any(char::is_control) {
                    return Err(Error::Invalid("Invalid queue target.".into()));
                }
                options[name] = json!(value);
            }
        }
        self.capture_options_with(ctx, runner, "queue-status", options, |project, args| {
            execute(project, runner, args)
        })
    }
    pub fn log_files(&self, ctx: &Invocation) -> Result<Vec<String>, Error> {
        let (project, root) = owner(ctx)?;
        let paths = project.find_files("storage/logs/**/*.log", 129)?;
        if paths.len() >= 129 {
            return Err(Error::Invalid(
                "More than 128 project log files; specify one log path explicitly.".into(),
            ));
        }
        let mut files = vec![];
        for path in paths {
            let Some(relative) = path.strip_prefix(&format!("{root}/")) else {
                return Err(Error::Stale);
            };
            if selected_log(&project, relative).is_ok() {
                files.push(relative.to_owned());
            }
        }
        files.sort();
        files.dedup();
        Ok(files)
    }
    pub fn capture_log(
        &mut self,
        ctx: &Invocation,
        runner: Runner,
        file: &str,
    ) -> Result<(), Error> {
        let (project, _) = owner(ctx)?;
        let path = selected_log(&project, file)?;
        let before = project.stat(&path)?;
        self.capture_options_with(
            ctx,
            runner,
            "logs",
            json!({"file":file}),
            |project, args| {
                let output = execute(project, runner, args)?;
                if selected_log(project, file)? != path || project.stat(&path)? != before {
                    return Err(Error::Stale);
                }
                Ok(output)
            },
        )
    }
    /// The selected key belongs to one captured failed-job row. No bulk or arbitrary IDs.
    pub fn retry_failed(
        &mut self,
        ctx: &Invocation,
        runner: Runner,
        row_key: &str,
    ) -> Result<String, Error> {
        self.retry_failed_with(
            ctx,
            runner,
            row_key,
            |project, args| execute(project, runner, args),
            |title, body| ui_kit::prompts::confirm_exact(title, body),
        )
    }
    fn retry_failed_with(
        &mut self,
        ctx: &Invocation,
        runner: Runner,
        row_key: &str,
        mut run: impl FnMut(&Project, &[String]) -> Result<String, Error>,
        confirm: impl FnOnce(&str, &str) -> Result<bool, Error>,
    ) -> Result<String, Error> {
        let (project, root) = owner(ctx)?;
        if !row_key.starts_with("failedjob:") {
            return Err(Error::Stale);
        }
        let snapshot = self
            .snapshots
            .get(&key(&root, runner, "queues"))
            .ok_or(Error::Stale)?
            .clone();
        let selected = snapshot
            .rows
            .iter()
            .find(|row| row["key"] == row_key)
            .cloned()
            .ok_or(Error::Stale)?;
        let id = selected["cells"][1]
            .as_str()
            .filter(|id| {
                !id.is_empty()
                    && id.len() <= 128
                    && *id != "all"
                    && id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                    && !id.starts_with('-')
            })
            .ok_or("The selected failed job has an unsupported retry identity.")?
            .to_owned();
        let before = stamp(&project)?;
        if before != snapshot.identity {
            return Err(Error::Stale);
        }
        let docker = target_binding(&project, runner)?;
        if docker != snapshot.target {
            return Err(Error::Stale);
        }
        let list = [
            "artisan",
            "queue:failed",
            "--json",
            "--no-interaction",
            "--no-ansi",
        ]
        .map(str::to_owned);
        let fresh = run(&project, &list)?;
        let same = |raw: &str| -> Result<bool, Error> {
            Ok(parse_runtime("queues", raw)?
                .iter()
                .any(|row| row["cells"] == selected["cells"]))
        };
        if !same(&fresh)? {
            return Err(Error::Stale);
        }
        let args = [
            "artisan",
            "queue:retry",
            &id,
            "--no-interaction",
            "--no-ansi",
        ]
        .map(str::to_owned);
        let preview = runner.argv(&project, &args, false)?;
        let body = format!(
            "Retry this selected failed job? Requeued work can execute immediately when a worker is running.\n\nProject: {root}\nRunner: {}\nFailed job: {id}\n{}\n\nExecutable: {}\nArgv: {}",
            runner.name(),
            selected["cells"][2].as_str().unwrap_or(""),
            runner.name(),
            json!(preview)
        );
        if !confirm("Retry selected Laravel failed job?", &body)? {
            return Err(Error::Cancelled);
        }
        if owner(ctx)?.1 != root
            || stamp(&project)? != before
            || self
                .snapshots
                .get(&key(&root, runner, "queues"))
                .is_none_or(|current| current.rows != snapshot.rows)
            || target_binding(&project, runner)? != docker
        {
            return Err(Error::Stale);
        }
        if !same(&run(&project, &list)?)? {
            return Err(Error::Stale);
        }
        if owner(ctx)?.1 != root
            || stamp(&project)? != before
            || target_binding(&project, runner)? != docker
        {
            return Err(Error::Stale);
        }
        let output = run(&project, &args)?;
        self.snapshots.remove(&key(&root, runner, "queues"));
        self.snapshots.remove(&key(&root, runner, "queue-status"));
        let mut end = output.len().min(8192);
        while !output.is_char_boundary(end) {
            end -= 1;
        }
        Ok(format!(
            "Requested retry for failed job {id}.\n{}{}",
            &output[..end],
            if end < output.len() {
                "\nOutput truncated."
            } else {
                ""
            }
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const FAILED: &str = r#"[{"id":"job-123","class":"App\\Jobs\\Work","connection":"database","queue":"default","failed_at":"2026-11-10"}]"#;
    fn setup() -> (MockHost, State, String) {
        let host = MockHost::install();
        host.add_file("artisan", "<?php");
        host.add_file("composer.json", "{}");
        let mut state = State::default();
        state
            .capture_with(&host.invocation(), Runner::Php, "queues", |_, _| {
                Ok(FAILED.into())
            })
            .unwrap();
        let cached = state.cached(&host.invocation(), Runner::Php).unwrap();
        let key = cached["snapshots"][0]["rows"][0]["key"]
            .as_str()
            .unwrap()
            .to_owned();
        (host, state, key)
    }
    #[test]
    fn queue_metrics_keep_unknown_distinct_from_empty() {
        let rows=parse("queue-status",r#"{"queues":[{"connection":"sync","driver":"sync","queue":"default","status":"unsupported","total":null,"pending":null,"reserved":null,"delayed":null,"running":null,"note":"No persistent queue"},{"connection":"database","driver":"database","queue":"default","status":"available","total":0,"pending":0,"reserved":0,"delayed":0,"running":null,"note":"Running is unknown"}]}"#).unwrap();
        assert!(
            rows[0]["cells"][2]
                .as_str()
                .unwrap()
                .contains("pending unavailable")
        );
        assert!(rows[1]["cells"][2].as_str().unwrap().contains("pending 0"));
        assert!(
            rows[1]["cells"][2]
                .as_str()
                .unwrap()
                .contains("running unavailable")
        );
        assert!(parse("queue-status",r#"{"queues":[{"connection":"x","driver":"redis","queue":"default","status":"available","total":-1}]}"#).is_err());
    }
    #[test]
    fn unavailable_worker_visibility_is_never_an_empty_worker_claim() {
        let rows = parse(
            "workers",
            r#"{"status":"unavailable","note":"No Linux proc","workers":[]}"#,
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert!(
            rows[0]["cells"][2]
                .as_str()
                .unwrap()
                .starts_with("unavailable")
        );
        let rows=parse("workers",r#"{"status":"available","note":"Observed","workers":[{"pid":42,"command":"queue:work"}]}"#).unwrap();
        assert_eq!(rows[1]["cells"][1], "PID 42");
        assert!(
            parse(
                "workers",
                r#"{"status":"available","workers":[{"pid":42,"command":"arbitrary-process"}]}"#
            )
            .is_err()
        );
    }
    #[test]
    fn log_tail_is_bounded_text_from_one_selected_file() {
        let rows=parse("logs",&json!({"file":"storage/logs/laravel.log","truncated":true,"lines":["<script>test</script>","x".repeat(6000)]}).to_string()).unwrap();
        assert!(
            rows[0]["cells"][2]
                .as_str()
                .unwrap()
                .contains("earlier content omitted")
        );
        assert_eq!(rows[1]["cells"][2], "<script>test</script>");
        assert!(
            rows[2]["cells"][2]
                .as_str()
                .unwrap()
                .ends_with("[line truncated]")
        );
        assert!(rows.iter().all(|row| row["activationEnabled"] == false));
        let host = MockHost::install();
        host.add_file("storage/logs/laravel.log", "log");
        let project = host.invocation().project().unwrap();
        assert!(selected_log(&project, "storage/logs/laravel.log").is_ok());
        for path in [
            ".env",
            "storage/logs/../.env",
            "storage/logs/../../.env",
            "storage/logs/laravel.log\n",
        ] {
            assert!(selected_log(&project, path).is_err());
        }
        let script = probe_args("logs", &json!({"file":"storage/logs/laravel.log"})).unwrap();
        assert_eq!(script[0], "-r");
        assert!(!script[1].starts_with("<?php"));
        assert_eq!(script[2], "logs");
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn selected_retry_revalidates_then_reviews_only_the_captured_job() {
        let (host, mut state, key) = setup();
        assert!(key.starts_with("failedjob:"));
        state
            .capture_with(&host.invocation(), Runner::Php, "events", |_, _| {
                Ok("[]".into())
            })
            .unwrap();
        let mut calls = vec![];
        let output = state
            .retry_failed_with(
                &host.invocation(),
                Runner::Php,
                &key,
                |_, args| {
                    calls.push(args.to_vec());
                    Ok(if args[1] == "queue:failed" {
                        FAILED.into()
                    } else {
                        "Retried".into()
                    })
                },
                |title, body| {
                    assert!(title.contains("selected"));
                    assert!(body.contains("job-123"));
                    assert!(body.contains("queue:retry"));
                    Ok(true)
                },
            )
            .unwrap();
        assert!(output.contains("job-123"));
        assert_eq!(calls.len(), 3);
        assert_eq!(
            calls[2],
            [
                "artisan",
                "queue:retry",
                "job-123",
                "--no-interaction",
                "--no-ansi"
            ]
        );
        let cached = state.cached(&host.invocation(), Runner::Php).unwrap();
        assert_eq!(cached["snapshots"].as_array().unwrap().len(), 1);
        assert_eq!(cached["snapshots"][0]["section"], "events");
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn failed_job_snapshot_from_an_unavailable_docker_target_is_hidden_and_cannot_retry() {
        let (host, mut state, row_key) = setup();
        let mut snapshot = state
            .snapshots
            .get(&key("/project", Runner::Php, "queues"))
            .unwrap()
            .clone();
        snapshot.target =
            Some(json!({"identity":{"container":"old-target"},"proof":{"StartedAt":"old-start"}}));
        state
            .snapshots
            .insert(key("/project", Runner::Docker, "queues"), snapshot);
        assert!(
            state.cached(&host.invocation(), Runner::Docker).unwrap()["snapshots"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            state
                .retry_failed_with(
                    &host.invocation(),
                    Runner::Docker,
                    &row_key,
                    |_, _| panic!("old target must not execute"),
                    |_, _| panic!("old target must not review")
                )
                .is_err()
        );
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn retry_cancel_stale_source_and_foreign_keys_never_mutate() {
        let (host, mut state, key) = setup();
        let mut calls = 0;
        assert_eq!(
            state.retry_failed_with(
                &host.invocation(),
                Runner::Php,
                &key,
                |_, args| {
                    calls += 1;
                    assert_eq!(args[1], "queue:failed");
                    Ok(FAILED.into())
                },
                |_, _| Ok(false)
            ),
            Err(Error::Cancelled)
        );
        assert_eq!(calls, 1);
        assert!(
            state
                .retry_failed_with(
                    &host.invocation(),
                    Runner::Php,
                    &key,
                    |_, args| {
                        assert_eq!(args[1], "queue:failed");
                        Ok(FAILED.into())
                    },
                    |_, _| {
                        host.add_file("config/changed.php", "<?php return []; ");
                        Ok(true)
                    }
                )
                .is_err()
        );
        assert!(
            state
                .retry_failed_with(
                    &host.invocation(),
                    Runner::Docker,
                    &key,
                    |_, _| panic!("foreign runner must not execute"),
                    |_, _| panic!("foreign runner must not review")
                )
                .is_err()
        );
        assert!(
            state
                .retry_failed_with(
                    &host.invocation(),
                    Runner::Php,
                    "failedjob:old:0",
                    |_, _| panic!("stale row must not execute"),
                    |_, _| panic!("stale row must not review")
                )
                .is_err()
        );
        assert!(host.process_runs().is_empty());
    }
}
