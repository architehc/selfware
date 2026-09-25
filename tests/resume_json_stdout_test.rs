//! D8 follow-up: in `--output-format json`, a RESUMED run's stdout carries
//! exactly one JSON result object, like a fresh run. `Agent::resume` used to
//! print its "Resuming task" / "Current step" / "Budget caps" chrome with
//! raw `println!`, ahead of the result object.
//!
//! Runs the real binary (libtest captures in-process `println!`, so only a
//! child process shows what reaches fd 1), with an isolated HOME holding the
//! checkpoint journal and a mock LLM endpoint.
#![cfg(unix)]

use assert_cmd::Command;
use selfware::testing::mock_api::{MockLlmServer, MockResponse};

fn write_config(dir: &std::path::Path, endpoint: &str) -> std::path::PathBuf {
    let path = dir.join("selfware.toml");
    std::fs::write(
        &path,
        format!(
            r#"endpoint = "{endpoint}"
model = "mock-model"

[agent]
max_iterations = 5
step_timeout_secs = 20
streaming = false
native_function_calling = false
min_completion_steps = 0
"#
        ),
    )
    .unwrap();
    path
}

/// Run the binary with the isolated HOME and return (stdout, stderr).
#[allow(deprecated)]
fn run(home: &std::path::Path, ws: &std::path::Path, args: &[&str]) -> (String, String) {
    let output = Command::cargo_bin("selfware")
        .unwrap()
        .current_dir(ws)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env_remove("SELFWARE_ENDPOINT")
        .env_remove("SELFWARE_MODEL")
        .env_remove("SELFWARE_API_KEY")
        .args(["-m", "yolo"])
        .args(args)
        .timeout(std::time::Duration::from_secs(180))
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// stdout is exactly one line, and that line is a JSON object.
fn single_json_object(stdout: &str, stderr: &str, what: &str) -> serde_json::Value {
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        1,
        "{what}: stdout must be ONE JSON line.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let v: serde_json::Value = serde_json::from_str(lines[0])
        .unwrap_or_else(|e| panic!("{what}: not JSON ({e}): {}\nstderr:\n{stderr}", lines[0]));
    assert!(v.is_object(), "{what}: {v}");
    v
}

#[tokio::test(flavor = "multi_thread")]
async fn json_resume_and_continue_stdout_is_exactly_one_result_object() {
    let server = MockLlmServer::builder()
        .with_default_response(MockResponse::Text(
            "Done: the greeting task is complete.".to_string(),
        ))
        .build()
        .await;
    let home = tempfile::tempdir().unwrap();
    let ws = home.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let config = write_config(home.path(), &format!("{}/v1", server.url()));
    let config = config.to_str().unwrap();

    let (out, err) = run(
        home.path(),
        &ws,
        &[
            "--config",
            config,
            "--output-format",
            "json",
            "-p",
            "Say hello.",
        ],
    );
    let fresh = single_json_object(&out, &err, "fresh -p run");
    let task_id = fresh["session_id"].as_str().unwrap_or_default().to_string();
    assert!(!task_id.is_empty(), "fresh run has a task id: {fresh}");

    let (out, err) = run(
        home.path(),
        &ws,
        &[
            "--config",
            config,
            "--output-format",
            "json",
            "resume",
            &task_id,
        ],
    );
    let resumed = single_json_object(&out, &err, "resume <id>");
    assert_eq!(resumed["session_id"], fresh["session_id"], "{resumed}");

    // stream-json: every stdout line is a JSON event, the last the result.
    let (out, err) = run(
        home.path(),
        &ws,
        &[
            "--config",
            config,
            "--output-format",
            "stream-json",
            "resume",
            &task_id,
        ],
    );
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(!lines.is_empty(), "stream-json resume: empty stdout\n{err}");
    for line in &lines {
        assert!(
            serde_json::from_str::<serde_json::Value>(line).is_ok(),
            "stream-json resume: non-JSON stdout line {line:?}\nstdout:\n{out}\nstderr:\n{err}"
        );
    }
    let last: serde_json::Value = serde_json::from_str(lines[lines.len() - 1]).unwrap();
    assert_eq!(last["session_id"], fresh["session_id"], "{out}");

    let (out, err) = run(
        home.path(),
        &ws,
        &["--config", config, "--output-format", "json", "--continue"],
    );
    single_json_object(&out, &err, "--continue");

    // --autocontinue next to -p is announced as ignored: a note, never
    // stdout text ahead of the -p run's result object.
    let (out, err) = run(
        home.path(),
        &ws,
        &[
            "--config",
            config,
            "--output-format",
            "json",
            "--autocontinue",
            "-p",
            "Say hello.",
        ],
    );
    single_json_object(&out, &err, "--autocontinue with -p");
    assert!(err.contains("--autocontinue ignored"), "{err}");

    server.stop().await;
}
