//! With `-v`, `--output-format json` / `stream-json` stdout still carries
//! ONLY JSON (README "For scripts and CI"). `output::phase_transition`
//! printed its "🔄 … → Planning" chrome to stdout under `-v` in every mode,
//! ahead of the result object.
//!
//! Runs the real binary (libtest captures in-process `println!`, so only a
//! child process shows what reaches fd 1) against a mock LLM endpoint.
#![cfg(unix)]

use assert_cmd::Command;
use selfware::testing::mock_api::{MockLlmServer, MockResponse};

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

fn assert_only_json_lines(stdout: &str, stderr: &str, what: &str) -> Vec<serde_json::Value> {
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(!lines.is_empty(), "{what}: empty stdout\nstderr:\n{stderr}");
    lines
        .iter()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).unwrap_or_else(|e| {
                panic!(
                    "{what}: non-JSON stdout line {line:?} ({e})\nstdout:\n{stdout}\nstderr:\n{stderr}"
                )
            })
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn verbose_structured_output_stdout_is_only_json() {
    let server = MockLlmServer::builder()
        .with_default_response(MockResponse::Text(
            "Done: the greeting task is complete.".to_string(),
        ))
        .build()
        .await;
    let home = tempfile::tempdir().unwrap();
    let ws = home.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let config = home.path().join("selfware.toml");
    std::fs::write(
        &config,
        format!(
            r#"endpoint = "{}/v1"
model = "mock-model"

[agent]
max_iterations = 5
step_timeout_secs = 20
streaming = false
native_function_calling = false
min_completion_steps = 0
"#,
            server.url()
        ),
    )
    .unwrap();
    let config = config.to_str().unwrap();

    // json: exactly one result object.
    let (out, err) = run(
        home.path(),
        &ws,
        &[
            "-v",
            "--config",
            config,
            "--output-format",
            "json",
            "-p",
            "Say hello.",
        ],
    );
    let values = assert_only_json_lines(&out, &err, "-v json");
    assert_eq!(values.len(), 1, "-v json: one result object\n{out}");
    assert!(values[0].is_object(), "{out}");
    // The phase chrome still exists under -v — on stderr.
    assert!(
        err.contains("Planning"),
        "-v json: phase transition goes to stderr\nstderr:\n{err}"
    );

    // stream-json: every stdout line is a JSON event.
    let (out, err) = run(
        home.path(),
        &ws,
        &[
            "-v",
            "--config",
            config,
            "--output-format",
            "stream-json",
            "-p",
            "Say hello.",
        ],
    );
    let values = assert_only_json_lines(&out, &err, "-v stream-json");
    assert!(values.len() > 1, "-v stream-json: events + result\n{out}");
}
