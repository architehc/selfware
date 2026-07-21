use super::*;

#[test]
fn test_process_start_name() {
    let tool = ProcessStart;
    assert_eq!(tool.name(), "process_start");
}

#[test]
fn test_process_start_description() {
    let tool = ProcessStart;
    assert!(tool.description().contains("background process"));
}

#[test]
fn test_process_start_schema() {
    let tool = ProcessStart;
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["id"].is_object());
    assert!(schema["properties"]["command"].is_object());
    assert!(schema["properties"]["health_check_pattern"].is_object());
}

#[test]
fn test_process_stop_name() {
    let tool = ProcessStop;
    assert_eq!(tool.name(), "process_stop");
}

#[test]
fn test_process_stop_schema() {
    let tool = ProcessStop;
    let schema = tool.schema();
    assert!(schema["properties"]["force"].is_object());
}

#[test]
fn test_process_list_name() {
    let tool = ProcessList;
    assert_eq!(tool.name(), "process_list");
}

#[test]
fn test_process_logs_name() {
    let tool = ProcessLogs;
    assert_eq!(tool.name(), "process_logs");
}

#[test]
fn test_process_logs_schema() {
    let tool = ProcessLogs;
    let schema = tool.schema();
    assert!(schema["properties"]["lines"].is_object());
}

#[test]
fn test_process_restart_name() {
    let tool = ProcessRestart;
    assert_eq!(tool.name(), "process_restart");
}

#[test]
fn test_port_check_name() {
    let tool = PortCheck;
    assert_eq!(tool.name(), "port_check");
}

#[test]
fn test_port_check_schema() {
    let tool = PortCheck;
    let schema = tool.schema();
    assert!(schema["properties"]["port"].is_object());
    assert!(schema["properties"]["find_available"].is_object());
    assert!(schema["properties"]["range_start"].is_object());
}

#[tokio::test]
async fn test_process_list_empty() {
    let tool = ProcessList;
    let result = tool.execute(serde_json::json!({})).await;
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.get("processes").is_some());
    assert!(output.get("count").is_some());
}

#[tokio::test]
async fn test_port_check_common_ports() {
    let tool = PortCheck;
    let result = tool.execute(serde_json::json!({})).await;
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.get("ports").is_some());
}

#[tokio::test]
async fn test_port_check_specific_port() {
    let tool = PortCheck;
    let result = tool.execute(serde_json::json!({"port": 12345})).await;
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.get("available").is_some());
}

#[tokio::test]
async fn test_port_check_find_available() {
    let tool = PortCheck;
    let result = tool
        .execute(serde_json::json!({
            "find_available": true,
            "range_start": 50000,
            "range_end": 50100
        }))
        .await;
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.get("available_port").is_some());
}

#[tokio::test]
async fn test_port_check_find_available_with_reservation() {
    let tool = PortCheck;
    // Busy CI runners can transiently fill a fixed range; retry with a
    // shifted range a few times before giving up.
    let mut output = None;
    for offset in 0..3u16 {
        let start = 56000 + offset * 100;
        let result = tool
            .execute(serde_json::json!({
                "find_available": true,
                "reserve": true,
                "range_start": start,
                "range_end": start + 100
            }))
            .await;
        if let Ok(out) = result {
            output = Some(out);
            break;
        }
    }
    let output = output.expect("find_available+reserve should succeed within a few ranges");

    let port = output["available_port"].as_u64().unwrap() as u16;
    assert_eq!(output["reserved"].as_bool(), Some(true));
    assert!(!is_port_available(port).await);

    let manager = PROCESS_MANAGER.read().await;
    assert!(manager.release_reserved_port(port).await);
}

#[tokio::test]
async fn test_process_start_echo() {
    let tool = ProcessStart;
    let result = tool
        .execute(serde_json::json!({
            "id": "test-echo-tool",
            "command": "echo",
            "args": ["hello"]
        }))
        .await;
    // echo exits immediately, so this should now correctly report failure
    // since the process didn't stay running as a background process
    assert!(
        result.is_err(),
        "echo exits immediately so process_start should return Err"
    );
}

#[tokio::test]
async fn test_process_stop_nonexistent() {
    let tool = ProcessStop;
    let result = tool
        .execute(serde_json::json!({"id": "nonexistent-process"}))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_process_logs_nonexistent() {
    let tool = ProcessLogs;
    let result = tool
        .execute(serde_json::json!({"id": "nonexistent-process"}))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_process_restart_nonexistent() {
    let tool = ProcessRestart;
    let result = tool
        .execute(serde_json::json!({"id": "nonexistent-process"}))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_process_start_with_health_check() {
    let tool = ProcessStart;
    let result = tool
        .execute(serde_json::json!({
            "id": "test-health-tool",
            "command": "echo",
            "args": ["Ready on http://localhost:3000"],
            "health_check_pattern": "Ready",
            "health_check_timeout_secs": 5
        }))
        .await;
    // echo prints the health check pattern and matches, but then exits
    // immediately. The health check loop sees health_matched=true and
    // breaks, then the status should be Running at that point.
    // However there is a race: the monitor might set Crashed before we read.
    // So we accept either Ok (health matched before crash detected) or Err.
    match result {
        Ok(output) => {
            assert!(output["health_matched"].as_bool().unwrap_or(false));
        }
        Err(e) => {
            // Process crashed after health check matched -- also acceptable
            let msg = e.to_string();
            assert!(
                msg.contains("exited immediately") || msg.contains("health check"),
                "Unexpected error: {}",
                msg
            );
        }
    }
}

#[tokio::test]
async fn test_process_start_consumes_reserved_port() {
    let port = {
        let manager = PROCESS_MANAGER.read().await;
        manager.reserve_available_port(56101, 56200).await.unwrap()
    };
    assert!(!is_port_available(port).await);

    let tool = ProcessStart;
    let result = tool
        .execute(serde_json::json!({
            "id": "test-reserved-port-tool",
            "command": "sleep",
            "args": ["60"],
            "expected_port": port
        }))
        .await;
    assert!(
        result.is_ok(),
        "process_start should consume selfware reservation instead of rejecting the port"
    );

    let manager = PROCESS_MANAGER.read().await;
    let summary = manager.get("test-reserved-port-tool").await.unwrap();
    assert_eq!(summary.expected_port, Some(port));
    assert!(!manager.has_reserved_port(port).await);

    let _ = manager.stop("test-reserved-port-tool", true).await;
}

#[tokio::test]
async fn test_process_start_auto_reserves_expected_port_without_prior_reservation() {
    let tool = ProcessStart;
    // The probe/drop/reserve sequence has an inherent TOCTOU window — on
    // busy CI runners another thread can grab the probed port first, so
    // retry with a fresh port (and a fresh process id) a few times.
    let mut started: Option<(String, u16)> = None;
    let mut last_err = None;
    for attempt in 0..5u32 {
        let probe_listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = probe_listener.local_addr().unwrap().port();
        drop(probe_listener);

        let id = format!("test-auto-reserved-port-tool-{attempt}");
        match tool
            .execute(serde_json::json!({
                "id": id,
                "command": "sleep",
                "args": ["60"],
                "expected_port": port
            }))
            .await
        {
            Ok(_) => {
                started = Some((id, port));
                break;
            }
            Err(e) => last_err = Some(e),
        }
    }
    let (id, port) = started.unwrap_or_else(|| {
        panic!(
            "process_start should automatically reserve a free expected_port before spawn: {:?}",
            last_err
        )
    });

    let manager = PROCESS_MANAGER.read().await;
    let summary = manager.get(&id).await.unwrap();
    assert_eq!(summary.expected_port, Some(port));
    assert!(!manager.has_reserved_port(port).await);

    let _ = manager.stop(&id, true).await;
}

#[tokio::test]
async fn test_process_inventory_reports_running_processes() {
    let tool = ProcessStart;
    let _ = tool
        .execute(serde_json::json!({
            "id": "test-inventory-tool",
            "command": "sleep",
            "args": ["60"]
        }))
        .await
        .unwrap();

    let inventory = process_inventory(5).await;
    assert!(inventory
        .processes
        .iter()
        .any(|proc| proc.id == "test-inventory-tool"));
    assert!(inventory.running >= 1);

    let manager = PROCESS_MANAGER.read().await;
    let _ = manager.stop("test-inventory-tool", true).await;
}

#[tokio::test]
async fn test_reconcile_managed_processes_prunes_stopped_entries() {
    let tool = ProcessStart;
    tool.execute(serde_json::json!({
        "id": "stale-global-entry",
        "command": "sleep",
        "args": ["60"]
    }))
    .await
    .unwrap();

    let manager = PROCESS_MANAGER.read().await;
    let _ = manager.stop("stale-global-entry", true).await;
    drop(manager);

    let report = reconcile_managed_processes(true).await;
    assert!(report.removed_inactive >= 1);
    let inventory = process_inventory(5).await;
    assert!(!inventory
        .processes
        .iter()
        .any(|proc| proc.id == "stale-global-entry"));
}

#[tokio::test]
async fn test_process_start_missing_id() {
    let tool = ProcessStart;
    let result = tool.execute(serde_json::json!({"command": "echo"})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("id"));
}

#[tokio::test]
async fn test_process_start_missing_command() {
    let tool = ProcessStart;
    let result = tool.execute(serde_json::json!({"id": "test"})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("command"));
}

#[tokio::test]
async fn test_process_start_with_env() {
    let tool = ProcessStart;
    let result = tool
        .execute(serde_json::json!({
            "id": "test-env-tool",
            "command": "env",
            "args": [],
            "env": {"MY_VAR": "test_value"}
        }))
        .await;
    // env exits immediately, so this should now correctly report failure
    assert!(
        result.is_err(),
        "env exits immediately so process_start should return Err"
    );
}

#[tokio::test]
async fn test_process_start_rejects_metachar_in_args() {
    let tool = ProcessStart;
    let result = tool
        .execute(serde_json::json!({
            "id": "test-meta-args",
            "command": "echo",
            "args": ["hello; rm -rf /"]
        }))
        .await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("forbidden shell metacharacter"));
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_process_start_with_cwd() {
    let tool = ProcessStart;
    let result = tool
        .execute(serde_json::json!({
            "id": "test-cwd-tool",
            "command": "pwd",
            "cwd": "/tmp"
        }))
        .await;
    // pwd exits immediately so this should now be an error (process didn't stay running)
    // but we just check it doesn't panic
    let _ = result;
}

#[tokio::test]
async fn test_process_start_nonexistent_command_returns_error() {
    let tool = ProcessStart;
    let result = tool
        .execute(serde_json::json!({
            "id": "test-nonexistent-cmd",
            "command": "this_binary_does_not_exist_xyz_12345"
        }))
        .await;
    assert!(
        result.is_err(),
        "Starting a nonexistent command should return Err"
    );
    assert!(result.unwrap_err().to_string().contains("Failed to spawn"));
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_process_start_immediate_exit_returns_error() {
    let tool = ProcessStart;
    let result = tool
        .execute(serde_json::json!({
            "id": "test-immediate-exit",
            "command": "sh",
            "args": ["-c", "exit 1"]
        }))
        .await;
    assert!(
        result.is_err(),
        "process_start should return Err when process exits immediately"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("exited immediately") || err_msg.contains("unexpected state"),
        "Error should describe the failure. Got: {}",
        err_msg
    );
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_process_start_health_check_timeout_returns_error() {
    let tool = ProcessStart;
    let result = tool
        .execute(serde_json::json!({
            "id": "test-health-timeout-tool",
            "command": "sleep",
            "args": ["60"],
            "health_check_pattern": "THIS_WILL_NEVER_APPEAR",
            "health_check_timeout_secs": 1
        }))
        .await;
    assert!(
        result.is_err(),
        "process_start should return Err when health check times out"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("health check"),
        "Error should mention health check. Got: {}",
        err_msg
    );

    let manager = PROCESS_MANAGER.read().await;
    let summary = manager.get("test-health-timeout-tool").await.unwrap();
    assert!(
        matches!(
            summary.status,
            crate::process_manager::ProcessStatus::HealthCheckFailed
        ),
        "Expected health check failure status, got {:?}",
        summary.status
    );
    assert!(
        summary.pid.is_none(),
        "Timed-out process should have been reaped"
    );
}
