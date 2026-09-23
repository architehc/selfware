use super::*;
use std::fs;
use tempfile::{tempdir, NamedTempFile};

#[test]
fn test_page_control_tool_name() {
    let tool = PageControlTool::new();
    assert_eq!(tool.name(), "page_control");
}

#[test]
fn test_page_control_tool_description() {
    let tool = PageControlTool::new();
    let desc = tool.description();
    assert!(desc.contains("Playwright"));
    assert!(desc.contains("navigation"));
    assert!(desc.contains("interaction"));
    assert!(desc.contains("multi-tab"));
}

#[test]
fn test_page_control_schema() {
    let tool = PageControlTool::new();
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"].get("action").is_some());
    assert!(schema["properties"].get("url").is_some());
    assert!(schema["properties"].get("selector").is_some());
    assert!(schema["properties"].get("text").is_some());
    assert!(schema["properties"].get("timeout_ms").is_some());
    assert!(schema["properties"].get("tab_index").is_some());
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("action")));
}

#[test]
fn test_page_control_schema_action_enum() {
    let tool = PageControlTool::new();
    let schema = tool.schema();
    let action_enum = schema["properties"]["action"]["enum"].as_array().unwrap();
    assert!(action_enum.contains(&json!("goto")));
    assert!(action_enum.contains(&json!("click")));
    assert!(action_enum.contains(&json!("type")));
    assert!(action_enum.contains(&json!("fill")));
    assert!(action_enum.contains(&json!("text")));
    assert!(action_enum.contains(&json!("screenshot")));
    assert!(action_enum.contains(&json!("evaluate")));
    assert!(action_enum.contains(&json!("new_tab")));
    assert!(action_enum.contains(&json!("shutdown")));
}

#[test]
fn test_page_control_rejects_unsafe_output_path() {
    crate::tools::file::reset_safety_config_for_tests();
    let err = validate_page_output_path("/etc/selfware-page.png", "page_control")
        .expect_err("absolute output outside allowed paths must be rejected");
    assert!(err.to_string().contains("output path validation failed"));
}

#[test]
fn test_valid_actions_completeness() {
    // Ensure all documented action categories are present
    let nav_actions = ["goto", "back", "forward", "reload", "wait_for"];
    let interaction_actions = [
        "click", "type", "fill", "select", "check", "uncheck", "hover", "press",
    ];
    let content_actions = ["text", "html", "attribute", "value", "count", "visible"];
    let page_info_actions = ["title", "url", "screenshot", "pdf"];
    let js_actions = ["evaluate", "evaluate_handle"];
    let tab_actions = ["new_tab", "switch_tab", "close_tab", "list_tabs"];
    let lifecycle_actions = ["shutdown"];

    for action in nav_actions
        .iter()
        .chain(interaction_actions.iter())
        .chain(content_actions.iter())
        .chain(page_info_actions.iter())
        .chain(js_actions.iter())
        .chain(tab_actions.iter())
        .chain(lifecycle_actions.iter())
    {
        assert!(VALID_ACTIONS.contains(action), "Missing action: {}", action);
    }
}

#[tokio::test]
async fn test_page_control_missing_action() {
    let tool = PageControlTool::new();
    let result = tool.execute(json!({})).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("action is required"));
}

#[tokio::test]
async fn test_page_control_invalid_action() {
    let tool = PageControlTool::new();
    let result = tool.execute(json!({"action": "nonexistent"})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Unknown action"));
}

#[test]
fn test_validate_url_allows_workspace_file() {
    // Hermetic workspace: a fresh tempdir becomes the cwd (under the shared
    // cwd lock, restored on drop) instead of creating the file in whatever
    // the process cwd happens to be — that raced with tests that call
    // set_current_dir, and littered the checkout.
    let workspace = tempdir().unwrap();
    let _guard = crate::test_support::CwdGuard::enter(workspace.path());
    let workspace_file = NamedTempFile::new_in(workspace.path()).unwrap();
    fs::write(workspace_file.path(), "<html><body>ok</body></html>").unwrap();
    let url = format!("file://{}", workspace_file.path().display());

    let result = validate_url(&url);
    assert!(result.is_ok());
}

#[test]
fn test_validate_url_blocks_file_outside_workspace() {
    let _guard = crate::test_support::CwdGuard::hold();
    let outside = tempdir().unwrap();
    let file = outside.path().join("external.html");
    fs::write(&file, "<html><body>blocked</body></html>").unwrap();

    let result = validate_url(&format!("file://{}", file.display()));
    assert!(result.is_err());
}

#[test]
fn test_validate_url_blocks_private_ip() {
    let result = validate_url("http://192.168.1.10/test");
    assert!(result.is_err());
}

#[test]
fn test_validate_url_allows_localhost() {
    let result = validate_url("http://localhost/test");
    assert!(result.is_ok());
}

#[test]
fn test_validate_url_private_ip_allowed_with_opt_in() {
    let result = validate_url_with_allow_private("http://192.168.1.10/test", true);
    assert!(result.is_ok());
}

#[test]
fn test_validate_url_allows_public() {
    let result = validate_url("https://example.com");
    assert!(result.is_ok());
}

#[test]
fn test_validate_url_allows_public_ip() {
    let result = validate_url("https://1.1.1.1/");
    assert!(result.is_ok());
}

#[test]
fn test_validate_url_blocks_ftp() {
    let result = validate_url("ftp://example.com/file");
    assert!(result.is_err());
}

#[test]
fn test_validate_url_allows_zero_ip_binding() {
    let result = validate_url("http://0.0.0.0/");
    assert!(result.is_ok());
}

#[test]
fn test_is_private_ip_v4() {
    assert!(net_policy::is_private_or_internal_ip(
        &"127.0.0.1".parse().unwrap()
    ));
    assert!(net_policy::is_private_or_internal_ip(
        &"10.0.0.1".parse().unwrap()
    ));
    assert!(net_policy::is_private_or_internal_ip(
        &"192.168.1.1".parse().unwrap()
    ));
    assert!(net_policy::is_private_or_internal_ip(
        &"172.16.0.1".parse().unwrap()
    ));
    assert!(net_policy::is_private_or_internal_ip(
        &"169.254.0.1".parse().unwrap()
    ));
    assert!(net_policy::is_private_or_internal_ip(
        &"0.0.0.0".parse().unwrap()
    ));
    assert!(!net_policy::is_private_or_internal_ip(
        &"8.8.8.8".parse().unwrap()
    ));
    assert!(!net_policy::is_private_or_internal_ip(
        &"1.1.1.1".parse().unwrap()
    ));
}

#[test]
fn test_is_private_ip_v6() {
    assert!(net_policy::is_private_or_internal_ip(
        &"::1".parse().unwrap()
    ));
    assert!(net_policy::is_private_or_internal_ip(
        &"::".parse().unwrap()
    ));
    assert!(!net_policy::is_private_or_internal_ip(
        &"2606:4700::1111".parse().unwrap()
    ));
}

#[test]
fn test_page_controller_default() {
    let _controller = PageController::default();
}

#[test]
fn test_page_control_tool_default() {
    let _tool = PageControlTool::default();
}

#[test]
fn test_page_control_schema_has_all_params() {
    let tool = PageControlTool::new();
    let schema = tool.schema();
    let props = schema["properties"].as_object().unwrap();

    // Verify all expected parameters exist
    let expected_params = vec![
        "action",
        "url",
        "selector",
        "text",
        "value",
        "values",
        "key",
        "name",
        "expression",
        "timeout_ms",
        "tab_index",
        "path",
        "full_page",
        "all",
        "outer",
        "wait_until",
        "load_state",
        "state",
        "button",
        "click_count",
        "delay",
        "format",
    ];

    for param in &expected_params {
        assert!(
            props.contains_key(*param),
            "Missing schema param: {}",
            param
        );
    }
}

#[tokio::test]
async fn test_page_control_url_validation_on_goto() {
    let tool = PageControlTool::new();
    // This should fail URL validation before even trying to spawn the bridge
    let result = tool
        .execute(json!({"action": "goto", "url": "file:///etc/passwd"}))
        .await;
    assert!(result.is_err());
}

#[test]
fn test_bridge_response_deserialization() {
    let json_str = r#"{"id":1,"success":true,"result":{"url":"https://example.com"},"error":null}"#;
    let resp: BridgeResponse = serde_json::from_str(json_str).unwrap();
    assert_eq!(resp.id, Some(1));
    assert!(resp.success);
    assert!(resp.result.is_some());
    assert!(resp.error.is_none());
}

#[test]
fn test_bridge_response_error() {
    let json_str = r#"{"id":2,"success":false,"result":null,"error":"something broke"}"#;
    let resp: BridgeResponse = serde_json::from_str(json_str).unwrap();
    assert_eq!(resp.id, Some(2));
    assert!(!resp.success);
    assert_eq!(resp.error, Some("something broke".to_string()));
}

#[test]
fn embedded_bridge_is_present() {
    assert!(!EMBEDDED_BRIDGE_JS.is_empty());
    assert!(EMBEDDED_BRIDGE_JS.contains("playwright"));
}

#[test]
fn extract_embedded_bridge_writes_and_is_idempotent() {
    let dir = std::env::temp_dir().join(format!("sw_bridge_test_{}", std::process::id()));
    let script = PlaywrightBridge::extract_embedded_bridge_to(&dir).unwrap();
    assert!(script.exists());
    assert_eq!(
        std::fs::read_to_string(&script).unwrap(),
        EMBEDDED_BRIDGE_JS
    );
    assert!(dir.join("package.json").exists());
    // second call is a no-op (content already matches)
    let script2 = PlaywrightBridge::extract_embedded_bridge_to(&dir).unwrap();
    assert_eq!(script, script2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Regression (review finding P1): the Playwright dependency install
/// (`npm install` / `npx playwright install`) runs against project-controlled
/// package metadata, so the constructed command must not inherit host
/// credentials. Inspects the command's env table directly (no spawn — a real
/// install would hit the network).
#[test]
fn bridge_installer_command_sanitizes_env() {
    let _env = crate::test_support::EnvGuard::capture(&["SELFWARE_BRIDGE_MARKER"]);
    _env.set("SELFWARE_BRIDGE_MARKER", "synthetic-leak-marker");

    let dir = std::env::temp_dir().join(format!("sw_bridge_env_{}", std::process::id()));
    let cmd = PlaywrightBridge::bridge_installer_command("npm", &dir);
    let envs: Vec<_> = cmd
        .get_envs()
        .map(|(k, _)| k.to_string_lossy().into_owned())
        .collect();
    assert!(
        !envs.iter().any(|k| k == "SELFWARE_BRIDGE_MARKER"),
        "synthetic marker must not be forwarded to the npm child; saw: {envs:?}"
    );
    assert!(
        envs.iter().any(|k| k == "PATH"),
        "the shared allowlist (PATH) must still reach the child; saw: {envs:?}"
    );
}

// ── Bridge stderr saturation + Chromium orphan (2026-09-21 review) ──────
//
// Synthetic-probe tests only — never a real browser. The stub bridge dumps
// >64KB to stderr (the pipe buffer) BEFORE writing its pidfile, spawns a
// long-lived sleeper child that inherits the bridge's process group, then
// hangs on stdin without ever answering on stdout.

const STUB_BRIDGE_JS: &str = r#"
const fs = require('fs');
const pidfile = process.argv[2];

// ~147KB of stderr, written before anything else. Pre-fix the parent never
// drained stderr, so node blocked on write(2) at the ~64KB pipe limit and
// never reached the pidfile write below.
const line = 'x'.repeat(48) + '\n';
for (let i = 0; i < 3000; i++) { fs.writeSync(2, line); }

// A long-lived child that inherits the bridge's process group.
const { spawn } = require('child_process');
const sleeper = spawn('sleep', ['60']);
fs.writeFileSync(pidfile, JSON.stringify({ node: process.pid, sleeper: sleeper.pid }));

// Hang on stdin without ever responding on stdout.
process.stdin.resume();
setInterval(() => {}, 1000);
"#;

#[cfg(unix)]
fn pid_is_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(unix)]
async fn wait_for_pidfile(
    pidfile: &std::path::Path,
    wait: std::time::Duration,
) -> Option<(u32, u32)> {
    let deadline = std::time::Instant::now() + wait;
    while std::time::Instant::now() < deadline {
        if let Ok(contents) = std::fs::read_to_string(pidfile) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&contents) {
                if let (Some(n), Some(s)) = (v["node"].as_u64(), v["sleeper"].as_u64()) {
                    return Some((n as u32, s as u32));
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    None
}

#[cfg(unix)]
async fn wait_until_pid_dead(pid: u32, wait: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + wait;
    while std::time::Instant::now() < deadline {
        if !pid_is_alive(pid) {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    !pid_is_alive(pid)
}

/// The bridge's stderr must be drained concurrently (the pipe never stalls
/// Node even with a >64KB burst) and teardown must kill the whole process
/// group (Node AND its children, not an orphaned descendant).
#[cfg(unix)]
#[tokio::test]
async fn test_bridge_stderr_saturation_and_group_termination() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("stub-bridge.js");
    std::fs::write(&script, STUB_BRIDGE_JS).unwrap();
    let pidfile = dir.path().join("bridge-pids.json");

    let bridge = PlaywrightBridge::spawn_with_script(&script, &[pidfile.to_str().unwrap()])
        .await
        .expect("stub bridge must spawn");

    // The stub floods stderr BEFORE writing the pidfile: with a dead stderr
    // pipe the parent stalls right here (the pidfile never appears and this
    // 10s wait times out). With the concurrent drain it appears promptly.
    let (node_pid, sleeper_pid) = wait_for_pidfile(&pidfile, std::time::Duration::from_secs(10))
        .await
        .expect("stub bridge must drain its stderr flood and reach the pidfile write");
    assert!(
        pid_is_alive(node_pid),
        "bridge node process should be alive"
    );

    // The stub hangs on stdin and never answers: a command must report a
    // clean timeout (the parent loop stays functional — no deadlock), and
    // the pending entry must not leak.
    let err = bridge
        .send(json!({"action": "goto", "url": "https://example.com"}), 500)
        .await
        .expect_err("a bridge that never answers must time out");
    assert!(
        err.to_string().contains("timed out"),
        "expected a bridge timeout, got: {err}"
    );

    // Dropping the bridge must terminate the whole process group: Node and
    // the sleeper it spawned. Pre-fix, start_kill signaled only Node, and
    // the sleeper (the "Chromium") kept running reparented.
    drop(bridge);
    assert!(
        wait_until_pid_dead(node_pid, std::time::Duration::from_secs(5)).await,
        "bridge node process must be dead after drop"
    );
    assert!(
        wait_until_pid_dead(sleeper_pid, std::time::Duration::from_secs(5)).await,
        "the bridge's child must be killed together with Node (group kill), not orphaned"
    );
}

// ── Bridge death: fail fast with the real cause ────────────────────────
//
// `sh` stubs stand in for Node so the transport is exercised without a
// browser: the bridge dying must fail pending commands immediately (not after
// timeout_ms + 5s) with the exit status and stderr tail.

#[cfg(unix)]
#[tokio::test]
async fn bridge_that_exits_immediately_fails_fast_with_cause() {
    let bridge = PlaywrightBridge::spawn_program(
        "sh",
        &[
            "-c",
            "echo \"Error: Cannot find module 'playwright'\" >&2; exit 1",
        ],
    )
    .await
    .expect("spawn sh stub");

    let start = std::time::Instant::now();
    let err = bridge
        .send(json!({"action": "title"}), 30_000)
        .await
        .expect_err("a dead bridge must fail the command");
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(1),
        "must fail well under the 35s command timeout, took {elapsed:?}: {err:#}"
    );
    let typed = err
        .downcast_ref::<BridgeTransportError>()
        .unwrap_or_else(|| panic!("expected typed bridge error, got {err:#}"));
    match typed {
        // The write may race the exit and hit EPIPE instead of the EOF path;
        // both are fast, typed, fatal causes that carry the exit detail.
        BridgeTransportError::Exited { detail } | BridgeTransportError::BrokenPipe { detail } => {
            assert!(
                detail.contains("Cannot find module 'playwright'"),
                "stderr tail expected: {typed}"
            );
        }
        other => panic!("unexpected cause: {other:?}"),
    }

    // Dead now: the next command fails immediately with the same cause.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while bridge.dead_cause().is_none() && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let cause = bridge.dead_cause().expect("bridge marked dead");
    let start = std::time::Instant::now();
    let again = bridge
        .send(json!({"action": "url"}), 30_000)
        .await
        .unwrap_err();
    assert!(start.elapsed() < std::time::Duration::from_millis(200));
    assert_eq!(again.downcast_ref::<BridgeTransportError>(), Some(&cause));
}

#[cfg(unix)]
#[tokio::test]
async fn bridge_dying_mid_command_fails_pending_command_fast() {
    // Reads the command, never answers, then dies.
    let bridge = PlaywrightBridge::spawn_program(
        "sh",
        &[
            "-c",
            "read line; sleep 0.5; echo 'chromium crashed' >&2; exit 4",
        ],
    )
    .await
    .expect("spawn sh stub");

    let start = std::time::Instant::now();
    let err = bridge
        .send(
            json!({"action": "goto", "url": "https://example.com"}),
            30_000,
        )
        .await
        .expect_err("bridge dies with the command pending");
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "pending command must fail when the bridge exits, took {elapsed:?}"
    );
    let msg = err.to_string();
    assert!(
        matches!(
            err.downcast_ref::<BridgeTransportError>(),
            Some(BridgeTransportError::Exited { .. })
        ),
        "{err:#}"
    );
    assert!(msg.contains('4'), "exit status expected: {msg}");
    assert!(
        msg.contains("chromium crashed"),
        "stderr tail expected: {msg}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn bridge_write_to_closed_stdin_is_typed_broken_pipe() {
    let bridge = PlaywrightBridge::spawn_program("sh", &["-c", "exec 0<&-; sleep 30"])
        .await
        .expect("spawn sh stub");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let mut broken = None;
    for _ in 0..5 {
        // timeout_ms 0 => waits 5s at most per attempt.
        let err = bridge
            .send(json!({"action": "title"}), 0)
            .await
            .unwrap_err();
        match err.downcast_ref::<BridgeTransportError>() {
            Some(e @ BridgeTransportError::BrokenPipe { .. }) => {
                broken = Some(e.clone());
                break;
            }
            Some(BridgeTransportError::TimedOut { .. }) => continue,
            other => panic!("unexpected error: {other:?} ({err:#})"),
        }
    }
    let broken = broken.expect("write to closed stdin must surface as BrokenPipe");
    assert!(broken.to_string().contains("broken pipe"), "{broken}");
    let start = std::time::Instant::now();
    let again = bridge.send(json!({"action": "url"}), 0).await.unwrap_err();
    assert!(start.elapsed() < std::time::Duration::from_millis(200));
    assert_eq!(again.downcast_ref::<BridgeTransportError>(), Some(&broken));
}
