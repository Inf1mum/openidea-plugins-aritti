//! Opt-in integration test against a running disposable DDEV project.
//! The SDK/UI host stays mocked; DDEV processes and containers are real.
use super::*;
use openidea_plugin::mock::MockHost;
use openidea_plugin::views::{Body, Views};
use std::path::Path;

fn execute(root: &Path, args: &[String], env: &[(String, String)]) -> std::process::Output {
    let output = std::process::Command::new("ddev")
        .args(args)
        .envs(env.iter().cloned())
        .current_dir(root)
        .output()
        .expect("execute real DDEV");
    assert!(
        output.status.success(),
        "DDEV process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
#[ignore = "requires DDEV_LIVE_PROJECT pointing to the running disposable live-smoke fixture"]
fn live_ddev_commands_and_description() {
    let root =
        std::fs::canonicalize(std::env::var("DDEV_LIVE_PROJECT").expect("set DDEV_LIVE_PROJECT"))
            .unwrap();
    let config = std::fs::read_to_string(root.join(CONFIG)).unwrap();
    assert!(
        root.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("ddev-live-parity-")
    );
    assert!(
        config
            .lines()
            .any(|line| line.starts_with("name: ddev-live-parity-"))
    );
    assert!(root.join("app/composer.json").is_file());
    let host = MockHost::install().with_capabilities(&["project.read", "process:ddev"]);
    host.set_project_root(root.to_str());
    host.set_setting("checkForUpdates", "false");
    host.set_setting("autoConfigureDataSource", "false");
    host.add_file(CONFIG, &config);
    let process_root = root.clone();
    host.on_process("ddev", move |args, cwd| {
        // MockHost's run-command callback omits the project-derived cwd.
        // Real host routing is covered by the Wasm workflow harness; these
        // processes explicitly execute in the disposable project below.
        assert!(cwd.is_none() || cwd == process_root.to_str());
        assert!(matches!(args[0].as_str(), "describe" | "--version"));
        let result = execute(
            &process_root,
            args,
            &[("DDEV_NONINTERACTIVE".into(), "true".into())],
        );
        Ok(openidea_plugin::ProcessOutput {
            status: result.status.code(),
            stdout: String::from_utf8(result.stdout).unwrap(),
            stderr: String::from_utf8(result.stderr).unwrap(),
        })
    });
    let mut plugin = Main::default();
    plugin
        .run_command("refresh", &host.invocation(), None)
        .unwrap();
    let project = host.invocation().project().unwrap();
    let description = plugin.description(&project).unwrap().clone();
    assert_eq!(description.status, "running");
    assert!(description.service("service:web").is_some());
    assert!(description.service("service:db").is_some());
    let source = database::source(&project, &description).unwrap();
    assert_eq!(source.host.as_deref(), Some("127.0.0.1"));
    assert!(source.port.is_some_and(|port| port > 0));
    assert!(matches!(
        plugin.render(VIEW, None, &host.invocation()).unwrap().body,
        Body::Tree(_)
    ));
    let literal = "$HOME;`literal`";
    for (tool, input) in [
        (
            "php",
            r#"-r 'echo json_encode(array_slice($argv, 1));' '' '$HOME;`literal`'"#,
        ),
        (
            "node",
            r#"-e 'console.log(JSON.stringify(process.argv.slice(1)))' '' '$HOME;`literal`'"#,
        ),
        ("composer", "run-script parity"),
    ] {
        host.answer_next_prompt(Some(&[input]));
        plugin.run_command(tool, &host.invocation(), None).unwrap();
        let command = plugin.terminal_launches.last().unwrap();
        assert_eq!(command.program, "ddev");
        let result = execute(&root, &command.args, &command.env);
        let stdout = String::from_utf8(result.stdout).unwrap();
        if tool == "composer" {
            assert!(stdout.contains("composer-root:app"));
            assert!(root.join(".parity-pre-composer").is_file());
            assert!(root.join(".parity-post-composer").is_file());
        } else {
            let argv: Vec<String> = serde_json::from_str(stdout.trim()).unwrap();
            assert_eq!(argv, ["", literal]);
        }
    }
    let result = execute(
        &root,
        &["mysql".into(), "-N".into(), "-e".into(), "SELECT 1".into()],
        &[],
    );
    assert_eq!(String::from_utf8(result.stdout).unwrap().trim(), "1");
}
