use super::*;

#[test]
fn test_process_config_creation() {
    let config = ProcessConfig {
        id: "test".to_string(),
        command: "echo".to_string(),
        args: vec!["hello".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    assert_eq!(config.id, "test");
    assert_eq!(config.command, "echo");
}

#[test]
fn test_process_status_serde() {
    let status = ProcessStatus::Running;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"running\"");

    let crashed = ProcessStatus::Crashed { exit_code: Some(1) };
    let json = serde_json::to_string(&crashed).unwrap();
    assert!(json.contains("crashed"));
}

#[test]
fn test_log_line_creation() {
    let log = LogLine {
        timestamp: Utc::now(),
        stream: LogStream::Stdout,
        content: "test output".to_string(),
    };

    assert_eq!(log.stream, LogStream::Stdout);
    assert_eq!(log.content, "test output");
}

#[test]
fn test_managed_process_log_buffer() {
    let config = ProcessConfig {
        id: "test".to_string(),
        command: "echo".to_string(),
        args: vec![],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let mut proc = ManagedProcess::new(config);

    // Add some logs
    proc.add_log(LogStream::Stdout, "line 1".to_string());
    proc.add_log(LogStream::Stderr, "error 1".to_string());
    proc.add_log(LogStream::Stdout, "line 2".to_string());

    assert_eq!(proc.log_buffer.len(), 3);
}

#[test]
fn test_managed_process_to_summary() {
    let config = ProcessConfig {
        id: "test-server".to_string(),
        command: "npm".to_string(),
        args: vec!["run".to_string(), "dev".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: Some("Ready".to_string()),
        health_check_timeout_secs: Some(30),
        expected_port: Some(3000),
        auto_restart: true,
        max_restart_attempts: 3,
    };

    let mut proc = ManagedProcess::new(config);
    proc.status = ProcessStatus::Running;
    proc.pid = Some(12345);
    proc.started_at = Some(Utc::now());
    proc.health_matched = true;

    let summary = proc.to_summary(10);

    assert_eq!(summary.id, "test-server");
    assert_eq!(summary.command, "npm");
    assert_eq!(summary.status, ProcessStatus::Running);
    assert_eq!(summary.pid, Some(12345));
    assert!(summary.health_matched);
    assert_eq!(summary.expected_port, Some(3000));
}

#[tokio::test]
async fn test_port_availability() {
    // Port 0 lets the OS assign a free port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    // Port should be in use
    assert!(!is_port_available(port).await);

    // Drop the listener
    drop(listener);

    // Port should now be available (might need a small delay on some systems)
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    assert!(is_port_available(port).await);
}

#[tokio::test(start_paused = true)]
async fn test_find_available_port() {
    // Find a port in a high range that's likely free
    let port = find_available_port(50000, 50100).await;
    assert!(port.is_some());
}

#[tokio::test(start_paused = true)]
async fn test_process_manager_new() {
    let manager = ProcessManager::new();
    let list = manager.list().await;
    assert!(list.is_empty());
}

#[tokio::test(start_paused = true)]
#[cfg(unix)]
async fn test_process_manager_start_simple() {
    let manager = ProcessManager::new();

    // Use sleep which stays alive, unlike echo which exits immediately
    let config = ProcessConfig {
        id: "echo-test".to_string(),
        command: "sleep".to_string(),
        args: vec!["60".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let result = manager.start(config).await;
    assert!(result.is_ok());

    let summary = result.unwrap();
    assert_eq!(summary.id, "echo-test");
    assert!(summary.pid.is_some());

    // Cleanup
    let _ = manager.stop("echo-test", true).await;
}

#[tokio::test(start_paused = true)]
#[cfg(unix)]
async fn test_process_manager_list() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "list-test".to_string(),
        command: "sleep".to_string(),
        args: vec!["0.1".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let _ = manager.start(config).await;

    let list = manager.list().await;
    assert!(!list.is_empty());
    assert!(list.iter().any(|p| p.id == "list-test"));
}

#[tokio::test(start_paused = true)]
#[cfg(unix)]
async fn test_process_manager_stop() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "stop-test".to_string(),
        command: "sleep".to_string(),
        args: vec!["60".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let _ = manager.start(config).await;

    let result = manager.stop("stop-test", false).await;
    assert!(result.is_ok());

    let summary = result.unwrap();
    assert_eq!(summary.status, ProcessStatus::Stopped);
}

#[tokio::test(start_paused = true)]
#[cfg(unix)]
async fn test_process_manager_get() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "get-test".to_string(),
        command: "echo".to_string(),
        args: vec!["test".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let _ = manager.start(config).await;

    let result = manager.get("get-test").await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().id, "get-test");
}

#[tokio::test(start_paused = true)]
async fn test_process_manager_get_not_found() {
    let manager = ProcessManager::new();
    let result = manager.get("nonexistent").await;
    assert!(result.is_err());
}

#[tokio::test(start_paused = true)]
#[cfg(unix)]
async fn test_process_manager_logs() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "logs-test".to_string(),
        command: "echo".to_string(),
        args: vec!["hello world".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let _ = manager.start(config).await;

    // Give it time to capture output
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let logs = manager.logs("logs-test", 10).await;
    assert!(logs.is_ok());
}

#[tokio::test(start_paused = true)]
#[cfg(unix)]
async fn test_process_manager_duplicate_start_reuses_matching_config() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "dup-test".to_string(),
        command: "sleep".to_string(),
        args: vec!["60".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let first = manager.start(config.clone()).await.unwrap();

    // Try to start again with same ID
    let second = manager.start(config).await.unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(first.pid, second.pid);

    // Cleanup
    let _ = manager.stop("dup-test", true).await;
}

#[tokio::test(start_paused = true)]
#[cfg(unix)]
async fn test_process_manager_duplicate_start_different_config_errors() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "dup-config-test".to_string(),
        command: "sleep".to_string(),
        args: vec!["60".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let _ = manager.start(config.clone()).await.unwrap();

    let mut changed = config;
    changed.args = vec!["30".to_string()];

    let result = manager.start(changed).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("different configuration"));

    let _ = manager.stop("dup-config-test", true).await;
}

#[tokio::test]
async fn test_port_reservation_round_trip() {
    let manager = ProcessManager::new();
    let port = manager.reserve_available_port(55000, 55100).await.unwrap();
    assert!(manager.has_reserved_port(port).await);
    assert!(!is_port_available(port).await);

    let released = manager.release_reserved_port(port).await;
    assert!(released);

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    assert!(is_port_available(port).await);
}

#[tokio::test]
#[cfg(unix)]
async fn test_start_consumes_matching_port_reservation() {
    let manager = ProcessManager::new();
    let port = manager.reserve_available_port(55101, 55200).await.unwrap();
    assert!(manager.has_reserved_port(port).await);

    let config = ProcessConfig {
        id: "reserved-port-start-test".to_string(),
        command: "sleep".to_string(),
        args: vec!["60".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: Some(port),
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let summary = manager.start(config).await.unwrap();
    assert_eq!(summary.expected_port, Some(port));
    assert!(!manager.has_reserved_port(port).await);

    let _ = manager.stop("reserved-port-start-test", true).await;
}

#[tokio::test]
async fn test_acquire_startup_port_listener_auto_reserves_unreserved_port() {
    // Retry with fresh ports to avoid TOCTOU races: between dropping the
    // probe listener and calling acquire_startup_port_listener, another
    // process (or parallel test) may grab the port.
    let mut last_err = None;
    for _ in 0..5 {
        let manager = ProcessManager::new();
        let (probe_listener, port) = bind_available_port().await.unwrap();
        drop(probe_listener);

        match manager.acquire_startup_port_listener(port).await {
            Ok(startup_listener) => {
                assert!(!manager.has_reserved_port(port).await);
                assert!(!is_port_available(port).await);

                drop(startup_listener);
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                assert!(is_port_available(port).await);
                return; // success
            }
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        }
    }
    panic!(
        "Failed after 5 attempts to acquire an unreserved port: {}",
        last_err.unwrap()
    );
}

#[tokio::test]
async fn test_invalid_health_check_regex_preserves_existing_port_reservation() {
    let manager = ProcessManager::new();
    let port = manager.reserve_available_port(55201, 55300).await.unwrap();
    assert!(manager.has_reserved_port(port).await);

    let config = ProcessConfig {
        id: "invalid-regex-reservation-test".to_string(),
        command: "sleep".to_string(),
        args: vec!["1".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: Some("(".to_string()),
        health_check_timeout_secs: None,
        expected_port: Some(port),
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let error = manager.start(config).await.unwrap_err();
    assert!(error
        .to_string()
        .contains("Invalid health check regex pattern"));
    assert!(manager.has_reserved_port(port).await);

    assert!(manager.release_reserved_port(port).await);
}

#[tokio::test]
#[cfg(unix)]
async fn test_process_manager_with_env() {
    let manager = ProcessManager::new();

    let mut env = HashMap::new();
    env.insert("TEST_VAR".to_string(), "test_value".to_string());

    let config = ProcessConfig {
        id: "env-test".to_string(),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), "echo $TEST_VAR".to_string()],
        cwd: None,
        env,
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    // Process exits immediately after echo, so start returns Err
    let _ = manager.start(config).await;

    // Give it time to capture output
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // The process entry still exists and logs were captured before it exited
    let logs = manager.logs("env-test", 10).await.unwrap();
    assert!(logs.iter().any(|l| l.content.contains("test_value")));
}

#[tokio::test]
#[cfg(unix)]
async fn test_process_manager_remove() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "remove-test".to_string(),
        command: "echo".to_string(),
        args: vec!["test".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let _ = manager.start(config).await;

    // Wait for it to finish
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let result = manager.remove("remove-test").await;
    assert!(result.is_ok());

    // Should not be in list anymore
    let list = manager.list().await;
    assert!(!list.iter().any(|p| p.id == "remove-test"));
}

#[test]
fn test_log_stream_serde() {
    let stdout = LogStream::Stdout;
    let json = serde_json::to_string(&stdout).unwrap();
    assert_eq!(json, "\"stdout\"");

    let stderr = LogStream::Stderr;
    let json = serde_json::to_string(&stderr).unwrap();
    assert_eq!(json, "\"stderr\"");
}

#[test]
fn test_process_status_variants() {
    let starting = ProcessStatus::Starting;
    assert!(matches!(starting, ProcessStatus::Starting));

    let restarting = ProcessStatus::Restarting { attempt: 2 };
    if let ProcessStatus::Restarting { attempt } = restarting {
        assert_eq!(attempt, 2);
    }

    let health_failed = ProcessStatus::HealthCheckFailed;
    assert!(matches!(health_failed, ProcessStatus::HealthCheckFailed));
}

#[test]
fn test_process_summary_serde() {
    let summary = ProcessSummary {
        id: "test".to_string(),
        command: "node".to_string(),
        args: vec!["server.js".to_string()],
        status: ProcessStatus::Running,
        pid: Some(12345),
        started_at: Some(Utc::now()),
        uptime_secs: Some(60),
        health_matched: true,
        restart_count: 0,
        expected_port: Some(3000),
        recent_logs: vec![],
    };

    let json = serde_json::to_string(&summary).unwrap();
    assert!(json.contains("test"));
    assert!(json.contains("node"));
    assert!(json.contains("3000"));
}

#[test]
fn test_process_status_clone() {
    let status = ProcessStatus::Running;
    let cloned = status.clone();
    assert_eq!(status, cloned);

    let crashed = ProcessStatus::Crashed { exit_code: Some(1) };
    let cloned = crashed.clone();
    assert_eq!(crashed, cloned);
}

#[test]
fn test_process_status_debug() {
    let status = ProcessStatus::Starting;
    let debug_str = format!("{:?}", status);
    assert!(debug_str.contains("Starting"));

    let restarting = ProcessStatus::Restarting { attempt: 3 };
    let debug_str = format!("{:?}", restarting);
    assert!(debug_str.contains("Restarting"));
    assert!(debug_str.contains("3"));
}

#[test]
fn test_process_status_all_variants() {
    let variants = [
        ProcessStatus::Starting,
        ProcessStatus::Running,
        ProcessStatus::HealthCheckFailed,
        ProcessStatus::Stopped,
        ProcessStatus::Crashed { exit_code: None },
        ProcessStatus::Crashed {
            exit_code: Some(127),
        },
        ProcessStatus::Restarting { attempt: 1 },
    ];
    for v in variants {
        let _ = serde_json::to_string(&v).unwrap();
    }
}

#[test]
fn test_process_status_eq() {
    assert_eq!(ProcessStatus::Running, ProcessStatus::Running);
    assert_ne!(ProcessStatus::Running, ProcessStatus::Stopped);
    assert_ne!(
        ProcessStatus::Crashed { exit_code: Some(1) },
        ProcessStatus::Crashed { exit_code: Some(2) }
    );
}

#[test]
fn test_process_config_clone() {
    let config = ProcessConfig {
        id: "test".to_string(),
        command: "echo".to_string(),
        args: vec!["hello".to_string()],
        cwd: Some(PathBuf::from("/tmp")),
        env: HashMap::new(),
        health_check_pattern: Some("Ready".to_string()),
        health_check_timeout_secs: Some(30),
        expected_port: Some(8080),
        auto_restart: true,
        max_restart_attempts: 5,
    };

    let cloned = config.clone();
    assert_eq!(config.id, cloned.id);
    assert_eq!(config.command, cloned.command);
    assert_eq!(config.expected_port, cloned.expected_port);
}

#[test]
fn test_process_config_debug() {
    let config = ProcessConfig {
        id: "debug-test".to_string(),
        command: "node".to_string(),
        args: vec![],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("ProcessConfig"));
    assert!(debug_str.contains("debug-test"));
}

#[test]
fn test_process_config_with_all_options() {
    let mut env = HashMap::new();
    env.insert("PORT".to_string(), "3000".to_string());
    env.insert("NODE_ENV".to_string(), "development".to_string());

    let config = ProcessConfig {
        id: "full-config".to_string(),
        command: "npm".to_string(),
        args: vec!["run".to_string(), "start".to_string()],
        cwd: Some(PathBuf::from("/home/user/project")),
        env,
        health_check_pattern: Some(r"Listening on port \d+".to_string()),
        health_check_timeout_secs: Some(120),
        expected_port: Some(3000),
        auto_restart: true,
        max_restart_attempts: 10,
    };

    assert!(config.auto_restart);
    assert_eq!(config.max_restart_attempts, 10);
    assert_eq!(config.env.len(), 2);
}

#[test]
fn test_log_line_clone() {
    let log = LogLine {
        timestamp: Utc::now(),
        stream: LogStream::Stdout,
        content: "test output".to_string(),
    };

    let cloned = log.clone();
    assert_eq!(log.stream, cloned.stream);
    assert_eq!(log.content, cloned.content);
}

#[test]
fn test_log_line_debug() {
    let log = LogLine {
        timestamp: Utc::now(),
        stream: LogStream::Stderr,
        content: "error message".to_string(),
    };

    let debug_str = format!("{:?}", log);
    assert!(debug_str.contains("LogLine"));
    assert!(debug_str.contains("Stderr"));
}

#[test]
fn test_log_line_serde_roundtrip() {
    let log = LogLine {
        timestamp: Utc::now(),
        stream: LogStream::Stdout,
        content: "test line".to_string(),
    };

    let json = serde_json::to_string(&log).unwrap();
    let parsed: LogLine = serde_json::from_str(&json).unwrap();

    assert_eq!(log.stream, parsed.stream);
    assert_eq!(log.content, parsed.content);
}

#[test]
fn test_log_stream_clone() {
    let stream = LogStream::Stdout;
    let cloned = stream.clone();
    assert_eq!(stream, cloned);
}

#[test]
fn test_log_stream_debug() {
    let stdout = LogStream::Stdout;
    assert!(format!("{:?}", stdout).contains("Stdout"));

    let stderr = LogStream::Stderr;
    assert!(format!("{:?}", stderr).contains("Stderr"));
}

#[test]
fn test_log_stream_eq() {
    assert_eq!(LogStream::Stdout, LogStream::Stdout);
    assert_ne!(LogStream::Stdout, LogStream::Stderr);
}

#[test]
fn test_process_summary_clone() {
    let summary = ProcessSummary {
        id: "clone-test".to_string(),
        command: "cargo".to_string(),
        args: vec!["run".to_string()],
        status: ProcessStatus::Running,
        pid: Some(999),
        started_at: Some(Utc::now()),
        uptime_secs: Some(100),
        health_matched: true,
        restart_count: 2,
        expected_port: Some(8000),
        recent_logs: vec![],
    };

    let cloned = summary.clone();
    assert_eq!(summary.id, cloned.id);
    assert_eq!(summary.restart_count, cloned.restart_count);
}

#[test]
fn test_process_summary_debug() {
    let summary = ProcessSummary {
        id: "debug-test".to_string(),
        command: "python".to_string(),
        args: vec!["app.py".to_string()],
        status: ProcessStatus::Stopped,
        pid: None,
        started_at: None,
        uptime_secs: None,
        health_matched: false,
        restart_count: 0,
        expected_port: None,
        recent_logs: vec![],
    };

    let debug_str = format!("{:?}", summary);
    assert!(debug_str.contains("ProcessSummary"));
}

#[test]
fn test_process_summary_deserialize() {
    let json = r#"{
            "id": "test",
            "command": "echo",
            "args": [],
            "status": "running",
            "pid": 12345,
            "started_at": null,
            "uptime_secs": null,
            "health_matched": true,
            "restart_count": 0,
            "expected_port": null,
            "recent_logs": []
        }"#;

    let summary: ProcessSummary = serde_json::from_str(json).unwrap();
    assert_eq!(summary.id, "test");
    assert_eq!(summary.status, ProcessStatus::Running);
}

#[test]
fn test_managed_process_debug() {
    let config = ProcessConfig {
        id: "debug".to_string(),
        command: "ls".to_string(),
        args: vec![],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let proc = ManagedProcess::new(config);
    let debug_str = format!("{:?}", proc);
    assert!(debug_str.contains("ManagedProcess"));
}

#[test]
fn test_managed_process_log_buffer_overflow() {
    let config = ProcessConfig {
        id: "overflow".to_string(),
        command: "test".to_string(),
        args: vec![],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let mut proc = ManagedProcess::new(config);

    // Add more than MAX_LOG_LINES
    for i in 0..600 {
        proc.add_log(LogStream::Stdout, format!("line {}", i));
    }

    // Should not exceed MAX_LOG_LINES
    assert!(proc.log_buffer.len() <= MAX_LOG_LINES);
}

#[test]
fn test_process_summary_with_logs() {
    let config = ProcessConfig {
        id: "logs".to_string(),
        command: "echo".to_string(),
        args: vec![],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let mut proc = ManagedProcess::new(config);
    proc.add_log(LogStream::Stdout, "line 1".to_string());
    proc.add_log(LogStream::Stdout, "line 2".to_string());
    proc.add_log(LogStream::Stdout, "line 3".to_string());

    let summary = proc.to_summary(2);
    assert_eq!(summary.recent_logs.len(), 2);
    // Should be last 2 logs
    assert_eq!(summary.recent_logs[1].content, "line 3");
}

#[tokio::test(start_paused = true)]
async fn test_process_manager_stop_nonexistent() {
    let manager = ProcessManager::new();
    let result = manager.stop("nonexistent", false).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test(start_paused = true)]
async fn test_process_manager_logs_nonexistent() {
    let manager = ProcessManager::new();
    let result = manager.logs("nonexistent", 10).await;
    assert!(result.is_err());
}

#[tokio::test(start_paused = true)]
async fn test_process_manager_remove_nonexistent() {
    let manager = ProcessManager::new();
    let result = manager.remove("nonexistent").await;
    assert!(result.is_err());
}

#[test]
fn test_process_config_serde_roundtrip() {
    let mut env = HashMap::new();
    env.insert("KEY".to_string(), "value".to_string());

    let config = ProcessConfig {
        id: "serde-test".to_string(),
        command: "cargo".to_string(),
        args: vec!["build".to_string(), "--release".to_string()],
        cwd: Some(PathBuf::from("/home/user/project")),
        env,
        health_check_pattern: Some("Finished".to_string()),
        health_check_timeout_secs: Some(60),
        expected_port: Some(8080),
        auto_restart: true,
        max_restart_attempts: 3,
    };

    let json = serde_json::to_string(&config).unwrap();
    let parsed: ProcessConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(config.id, parsed.id);
    assert_eq!(config.command, parsed.command);
    assert_eq!(config.args, parsed.args);
    assert_eq!(config.expected_port, parsed.expected_port);
    assert_eq!(config.auto_restart, parsed.auto_restart);
}

#[test]
fn test_process_config_minimal_serde() {
    let config = ProcessConfig {
        id: "minimal".to_string(),
        command: "ls".to_string(),
        args: vec![],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let json = serde_json::to_string(&config).unwrap();
    let parsed: ProcessConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(config.id, parsed.id);
    assert!(parsed.cwd.is_none());
    assert!(parsed.health_check_pattern.is_none());
}

#[test]
fn test_process_status_deserialize_all_variants() {
    let json_starting = r#""starting""#;
    let parsed: ProcessStatus = serde_json::from_str(json_starting).unwrap();
    assert!(matches!(parsed, ProcessStatus::Starting));

    let json_running = r#""running""#;
    let parsed: ProcessStatus = serde_json::from_str(json_running).unwrap();
    assert!(matches!(parsed, ProcessStatus::Running));

    let json_stopped = r#""stopped""#;
    let parsed: ProcessStatus = serde_json::from_str(json_stopped).unwrap();
    assert!(matches!(parsed, ProcessStatus::Stopped));

    let json_failed = r#""health_check_failed""#;
    let parsed: ProcessStatus = serde_json::from_str(json_failed).unwrap();
    assert!(matches!(parsed, ProcessStatus::HealthCheckFailed));
}

#[test]
fn test_process_status_crashed_serde() {
    let crashed = ProcessStatus::Crashed { exit_code: Some(1) };
    let json = serde_json::to_string(&crashed).unwrap();
    let parsed: ProcessStatus = serde_json::from_str(&json).unwrap();

    if let ProcessStatus::Crashed { exit_code } = parsed {
        assert_eq!(exit_code, Some(1));
    } else {
        panic!("Expected Crashed variant");
    }
}

#[test]
fn test_process_status_crashed_none_exit_code() {
    let crashed = ProcessStatus::Crashed { exit_code: None };
    let json = serde_json::to_string(&crashed).unwrap();
    let parsed: ProcessStatus = serde_json::from_str(&json).unwrap();

    if let ProcessStatus::Crashed { exit_code } = parsed {
        assert!(exit_code.is_none());
    } else {
        panic!("Expected Crashed variant");
    }
}

#[test]
fn test_process_status_restarting_serde() {
    let restarting = ProcessStatus::Restarting { attempt: 5 };
    let json = serde_json::to_string(&restarting).unwrap();
    let parsed: ProcessStatus = serde_json::from_str(&json).unwrap();

    if let ProcessStatus::Restarting { attempt } = parsed {
        assert_eq!(attempt, 5);
    } else {
        panic!("Expected Restarting variant");
    }
}

#[tokio::test(start_paused = true)]
async fn test_process_manager_default() {
    let manager = ProcessManager::default();
    let list = manager.list().await;
    assert!(list.is_empty());
}

#[tokio::test]
async fn test_process_manager_reconcile_marks_orphaned_running_entry() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "orphaned".to_string(),
        command: "sleep".to_string(),
        args: vec!["60".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let mut proc = ManagedProcess::new(config);
    proc.status = ProcessStatus::Running;
    proc.pid = Some(4242);

    manager
        .processes
        .write()
        .await
        .insert("orphaned".to_string(), proc);

    let report = manager.reconcile(false).await;
    assert_eq!(report.orphaned_entries, 1);

    let summary = manager.get("orphaned").await.unwrap();
    assert!(matches!(
        summary.status,
        ProcessStatus::Crashed { exit_code: None }
    ));
    assert!(summary.pid.is_none());
}

#[tokio::test]
async fn test_process_manager_reconcile_prunes_inactive_entries() {
    let manager = ProcessManager::new();

    let mut proc = ManagedProcess::new(ProcessConfig {
        id: "stopped-entry".to_string(),
        command: "sleep".to_string(),
        args: vec!["60".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    });
    proc.status = ProcessStatus::Stopped;

    manager
        .processes
        .write()
        .await
        .insert("stopped-entry".to_string(), proc);

    let report = manager.reconcile(true).await;
    assert_eq!(report.removed_inactive, 1);
    assert!(manager.list().await.is_empty());
}

#[tokio::test]
async fn test_process_manager_inventory_includes_reserved_ports() {
    let manager = ProcessManager::new();
    let port = manager.reserve_available_port(56201, 56300).await.unwrap();

    let inventory = manager.inventory(5).await;
    assert_eq!(inventory.total, 0);
    assert_eq!(inventory.reserved_ports, vec![port]);

    assert!(manager.release_reserved_port(port).await);
}

/// Sentinel secret in the parent env must NOT leak into the child.
#[tokio::test]
#[cfg(unix)]
async fn start_does_not_leak_parent_env_to_child() {
    // A sentinel secret present in the PARENT environment.
    std::env::set_var("SELFWARE_SENTINEL_SECRET", "leaked-value-123");

    let mgr = ProcessManager::new();
    let config = ProcessConfig {
        id: "env-leak-test".to_string(),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), "env".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    // `env` exits immediately, so start() returns Err — that's fine; the
    // output is captured before the process exits.
    let _ = mgr.start(config).await;

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let logs = mgr.logs("env-leak-test", 200).await.unwrap();
    let output: String = logs
        .iter()
        .map(|l| l.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    std::env::remove_var("SELFWARE_SENTINEL_SECRET");

    assert!(
        !output.contains("leaked-value-123"),
        "child environment leaked the parent secret: {output}"
    );
    // The minimal allowlist still passes PATH through.
    assert!(
        output.contains("PATH="),
        "PATH should be present in child env, got: {output}"
    );
}

#[tokio::test(start_paused = true)]
#[cfg(unix)]
async fn test_process_manager_force_stop() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "force-stop-test".to_string(),
        command: "sleep".to_string(),
        args: vec!["60".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let _ = manager.start(config).await;

    // Force stop
    let result = manager.stop("force-stop-test", true).await;
    assert!(result.is_ok());

    let summary = result.unwrap();
    assert_eq!(summary.status, ProcessStatus::Stopped);
}

#[tokio::test(start_paused = true)]
#[cfg(unix)]
async fn test_process_manager_stop_already_stopped() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "already-stopped".to_string(),
        command: "echo".to_string(),
        args: vec!["done".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let _ = manager.start(config).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Stop should return ok even if already stopped
    let result = manager.stop("already-stopped", false).await;
    assert!(result.is_ok());
}

#[tokio::test(start_paused = true)]
#[cfg(unix)]
async fn test_process_manager_restart() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "restart-test".to_string(),
        command: "sleep".to_string(),
        args: vec!["60".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let _ = manager.start(config).await;

    // Restart
    let result = manager.restart("restart-test").await;
    assert!(result.is_ok());

    let summary = result.unwrap();
    assert_eq!(summary.id, "restart-test");

    // Cleanup
    let _ = manager.stop("restart-test", true).await;
}

#[tokio::test(start_paused = true)]
async fn test_process_manager_restart_nonexistent() {
    let manager = ProcessManager::new();
    let result = manager.restart("nonexistent").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test(start_paused = true)]
#[cfg(unix)]
async fn test_process_manager_remove_running() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "remove-running".to_string(),
        command: "sleep".to_string(),
        args: vec!["60".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let _ = manager.start(config).await;

    // Try to remove while running
    let result = manager.remove("remove-running").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Stop it first"));

    // Cleanup
    let _ = manager.stop("remove-running", true).await;
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_process_manager_with_working_directory() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "cwd-test".to_string(),
        command: "pwd".to_string(),
        args: vec![],
        cwd: Some(PathBuf::from("/tmp")),
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    // pwd exits immediately, so start returns Err
    let _ = manager.start(config).await;

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // The process entry still exists and logs were captured
    let logs = manager.logs("cwd-test", 10).await.unwrap();
    assert!(logs.iter().any(|l| l.content.contains("/tmp")));
}

#[tokio::test(start_paused = true)]
#[cfg(unix)]
async fn test_process_manager_with_health_check() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "health-check-test".to_string(),
        command: "echo".to_string(),
        args: vec!["Server ready".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: Some("ready".to_string()),
        health_check_timeout_secs: Some(5),
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let result = manager.start(config).await;
    // echo prints "Server ready" matching the health check, then exits.
    // Depending on task scheduling, health_matched may be set to true
    // before or after the monitor marks the process as Crashed.
    match result {
        Ok(summary) => {
            // Health check matched before crash was detected
            assert!(summary.health_matched);
            assert_eq!(summary.status, ProcessStatus::Running);
        }
        Err(e) => {
            // Process crashed before/after health check was evaluated
            let msg = e.to_string();
            assert!(
                msg.contains("exited immediately") || msg.contains("health check"),
                "Unexpected error: {}",
                msg
            );
        }
    }
}

#[tokio::test(start_paused = true)]
async fn test_process_summary_uptime() {
    let config = ProcessConfig {
        id: "uptime-test".to_string(),
        command: "sleep".to_string(),
        args: vec!["1".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let mut proc = ManagedProcess::new(config);
    proc.started_at = Some(Utc::now() - chrono::Duration::seconds(10));
    proc.status = ProcessStatus::Running;

    let summary = proc.to_summary(10);
    assert!(summary.uptime_secs.is_some());
    assert!(summary.uptime_secs.unwrap() >= 10);
}

#[test]
fn test_log_stream_serde_roundtrip() {
    let stdout = LogStream::Stdout;
    let json = serde_json::to_string(&stdout).unwrap();
    let parsed: LogStream = serde_json::from_str(&json).unwrap();
    assert_eq!(stdout, parsed);

    let stderr = LogStream::Stderr;
    let json = serde_json::to_string(&stderr).unwrap();
    let parsed: LogStream = serde_json::from_str(&json).unwrap();
    assert_eq!(stderr, parsed);
}

#[test]
fn test_process_summary_with_all_fields() {
    let log = LogLine {
        timestamp: Utc::now(),
        stream: LogStream::Stdout,
        content: "log message".to_string(),
    };

    let summary = ProcessSummary {
        id: "full-summary".to_string(),
        command: "cargo".to_string(),
        args: vec!["run".to_string(), "--release".to_string()],
        status: ProcessStatus::Crashed { exit_code: Some(1) },
        pid: Some(54321),
        started_at: Some(Utc::now()),
        uptime_secs: Some(3600),
        health_matched: false,
        restart_count: 2,
        expected_port: Some(9000),
        recent_logs: vec![log],
    };

    let json = serde_json::to_string(&summary).unwrap();
    assert!(json.contains("full-summary"));
    assert!(json.contains("crashed"));
    assert!(json.contains("9000"));
    assert!(json.contains("log message"));
}

#[test]
fn test_managed_process_new_initial_state() {
    let config = ProcessConfig {
        id: "initial".to_string(),
        command: "test".to_string(),
        args: vec![],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let proc = ManagedProcess::new(config);

    assert_eq!(proc.status, ProcessStatus::Stopped);
    assert!(proc.pid.is_none());
    assert!(proc.started_at.is_none());
    assert!(proc.log_buffer.is_empty());
    assert!(!proc.health_matched);
    assert_eq!(proc.restart_count, 0);
    assert!(proc.child_handle.is_none());
}

#[test]
fn test_managed_process_add_log_alternating_streams() {
    let config = ProcessConfig {
        id: "alt".to_string(),
        command: "test".to_string(),
        args: vec![],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let mut proc = ManagedProcess::new(config);
    proc.add_log(LogStream::Stdout, "out1".to_string());
    proc.add_log(LogStream::Stderr, "err1".to_string());
    proc.add_log(LogStream::Stdout, "out2".to_string());
    proc.add_log(LogStream::Stderr, "err2".to_string());

    assert_eq!(proc.log_buffer.len(), 4);
    assert_eq!(proc.log_buffer[0].stream, LogStream::Stdout);
    assert_eq!(proc.log_buffer[1].stream, LogStream::Stderr);
    assert_eq!(proc.log_buffer[2].stream, LogStream::Stdout);
    assert_eq!(proc.log_buffer[3].stream, LogStream::Stderr);
}

#[test]
fn test_managed_process_to_summary_empty_logs() {
    let config = ProcessConfig {
        id: "empty".to_string(),
        command: "test".to_string(),
        args: vec![],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let proc = ManagedProcess::new(config);
    let summary = proc.to_summary(100);

    assert!(summary.recent_logs.is_empty());
    assert!(summary.uptime_secs.is_none());
}

#[test]
fn test_managed_process_to_summary_request_more_logs_than_available() {
    let config = ProcessConfig {
        id: "few-logs".to_string(),
        command: "test".to_string(),
        args: vec![],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let mut proc = ManagedProcess::new(config);
    proc.add_log(LogStream::Stdout, "line1".to_string());
    proc.add_log(LogStream::Stdout, "line2".to_string());

    let summary = proc.to_summary(100); // Request 100 but only 2 available
    assert_eq!(summary.recent_logs.len(), 2);
}

#[tokio::test]
async fn test_is_port_available_high_port() {
    // Bind to port 0 to get an OS-assigned free port, then release it and
    // verify is_port_available() agrees it's free.  This avoids hardcoded
    // port collisions when many tests run in parallel.
    //
    // Note: There is an inherent TOCTOU race between dropping the listener
    // and probing the port.  We retry a few times to tolerate OS timing.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0u16))
        .await
        .expect("Should bind to ephemeral port");
    let port = listener.local_addr().unwrap().port();
    drop(listener); // release port

    // Give the OS a moment to fully release the socket
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let mut available = false;
    for _ in 0..3 {
        if is_port_available(port).await {
            available = true;
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    assert!(
        available,
        "Recently released port {} should be available after retries",
        port
    );
}

#[tokio::test(start_paused = true)]
async fn test_find_available_port_narrow_range() {
    // Use a range of high ephemeral ports unlikely to be in use
    let port = find_available_port(59100, 59200).await;
    // Should find at least one free port in a 100-port range
    assert!(port.is_some());
    assert!(port.unwrap() >= 59100 && port.unwrap() <= 59200);
}

#[tokio::test(start_paused = true)]
async fn test_find_available_port_all_used() {
    // Bind a port, then search only that port range
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    // Search for just that port (which is in use)
    let result = find_available_port(port, port).await;
    assert!(result.is_none());
}

#[tokio::test(start_paused = true)]
#[cfg(unix)]
async fn test_port_info_unused_port() {
    // Port info for an unused high port
    let info = port_info(59998).await;
    // Should return None since nothing is listening
    assert!(info.is_none() || info.as_ref().map(|s| s.is_empty()).unwrap_or(true));
}

#[tokio::test(start_paused = true)]
#[cfg(unix)]
async fn test_process_manager_multiple_processes() {
    let manager = ProcessManager::new();

    let config1 = ProcessConfig {
        id: "multi-1".to_string(),
        command: "sleep".to_string(),
        args: vec!["0.5".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let config2 = ProcessConfig {
        id: "multi-2".to_string(),
        command: "sleep".to_string(),
        args: vec!["0.5".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let _ = manager.start(config1).await;
    let _ = manager.start(config2).await;

    let list = manager.list().await;
    assert_eq!(list.len(), 2);
    assert!(list.iter().any(|p| p.id == "multi-1"));
    assert!(list.iter().any(|p| p.id == "multi-2"));
}

#[tokio::test]
#[cfg(unix)]
async fn test_process_manager_stderr_capture() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "stderr-test".to_string(),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), "echo error >&2".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let _ = manager.start(config).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let logs = manager.logs("stderr-test", 10).await.unwrap();
    assert!(logs
        .iter()
        .any(|l| l.stream == LogStream::Stderr && l.content.contains("error")));
}

#[test]
fn test_process_config_env_multiple_vars() {
    let mut env = HashMap::new();
    env.insert("VAR1".to_string(), "value1".to_string());
    env.insert("VAR2".to_string(), "value2".to_string());
    env.insert("VAR3".to_string(), "value3".to_string());

    let config = ProcessConfig {
        id: "env-multi".to_string(),
        command: "test".to_string(),
        args: vec![],
        cwd: None,
        env: env.clone(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    assert_eq!(config.env.len(), 3);
    assert_eq!(config.env.get("VAR1"), Some(&"value1".to_string()));
    assert_eq!(config.env.get("VAR2"), Some(&"value2".to_string()));
    assert_eq!(config.env.get("VAR3"), Some(&"value3".to_string()));
}

#[test]
fn test_max_log_lines_constant() {
    // Verify the constant value
    assert_eq!(MAX_LOG_LINES, 500);
}

#[test]
fn test_health_check_timeout_constant() {
    // Verify the constant value
    assert_eq!(HEALTH_CHECK_TIMEOUT_SECS, 60);
}

#[test]
fn test_process_status_partial_eq() {
    let running1 = ProcessStatus::Running;
    let running2 = ProcessStatus::Running;
    assert!(running1 == running2);

    let crashed1 = ProcessStatus::Crashed { exit_code: Some(1) };
    let crashed2 = ProcessStatus::Crashed { exit_code: Some(1) };
    assert!(crashed1 == crashed2);

    let restarting1 = ProcessStatus::Restarting { attempt: 3 };
    let restarting2 = ProcessStatus::Restarting { attempt: 3 };
    assert!(restarting1 == restarting2);

    let restarting3 = ProcessStatus::Restarting { attempt: 4 };
    assert!(restarting1 != restarting3);
}

#[test]
fn test_log_line_partial_eq() {
    let timestamp = Utc::now();
    let log1 = LogLine {
        timestamp,
        stream: LogStream::Stdout,
        content: "test".to_string(),
    };
    let log2 = LogLine {
        timestamp,
        stream: LogStream::Stdout,
        content: "test".to_string(),
    };
    // LogLine doesn't derive PartialEq but we can compare fields
    assert_eq!(log1.stream, log2.stream);
    assert_eq!(log1.content, log2.content);
}

#[test]
fn test_process_config_args_multiple() {
    let config = ProcessConfig {
        id: "multi-args".to_string(),
        command: "cargo".to_string(),
        args: vec![
            "test".to_string(),
            "--lib".to_string(),
            "--".to_string(),
            "--nocapture".to_string(),
        ],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    assert_eq!(config.args.len(), 4);
    assert_eq!(config.args[0], "test");
    assert_eq!(config.args[3], "--nocapture");
}

#[tokio::test(start_paused = true)]
async fn test_process_manager_invalid_command() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "invalid-cmd".to_string(),
        command: "nonexistent_command_xyz_123".to_string(),
        args: vec![],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let result = manager.start(config).await;
    assert!(result.is_err());
}

#[test]
fn test_process_summary_with_restarting_status() {
    let summary = ProcessSummary {
        id: "restarting-summary".to_string(),
        command: "server".to_string(),
        args: vec![],
        status: ProcessStatus::Restarting { attempt: 3 },
        pid: None,
        started_at: Some(Utc::now()),
        uptime_secs: None,
        health_matched: false,
        restart_count: 3,
        expected_port: Some(8080),
        recent_logs: vec![],
    };

    let json = serde_json::to_string(&summary).unwrap();
    assert!(json.contains("restarting"));
    assert!(json.contains("attempt"));
}

#[test]
fn test_process_config_cwd_pathbuf() {
    let config = ProcessConfig {
        id: "pathbuf".to_string(),
        command: "ls".to_string(),
        args: vec![],
        cwd: Some(PathBuf::from("/home/user/project/src")),
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    assert!(config.cwd.is_some());
    assert_eq!(config.cwd.unwrap().to_str(), Some("/home/user/project/src"));
}

/// A process that exits with a non-zero code immediately should return an error
/// from start() instead of silently reporting success.
#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_start_returns_error_when_process_exits_with_nonzero() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "exit-fail-test".to_string(),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), "exit 42".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let result = manager.start(config).await;
    assert!(
        result.is_err(),
        "start() should return Err when process exits immediately with non-zero code"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("exited immediately") || err_msg.contains("42"),
        "Error should mention exit code. Got: {}",
        err_msg
    );
}

/// A process with a health check that times out should return an error.
#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_start_returns_error_on_health_check_timeout() {
    let manager = ProcessManager::new();

    // Process that runs but never prints the health check pattern
    let config = ProcessConfig {
        id: "health-timeout-test".to_string(),
        command: "sleep".to_string(),
        args: vec!["60".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: Some("READY_PATTERN_THAT_NEVER_APPEARS".to_string()),
        health_check_timeout_secs: Some(1), // 1 second timeout
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let result = manager.start(config).await;
    assert!(
        result.is_err(),
        "start() should return Err when health check times out"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("health check timed out"),
        "Error should mention health check timeout. Got: {}",
        err_msg
    );

    // Cleanup
    let _ = manager.stop("health-timeout-test", true).await;
}

/// A process that exits during health check wait should return an error
/// with the exit code.
#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_start_returns_error_when_process_crashes_during_health_check() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "crash-health-test".to_string(),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), "echo 'starting up'; exit 7".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: Some("READY".to_string()),
        health_check_timeout_secs: Some(3),
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let result = manager.start(config).await;
    assert!(
        result.is_err(),
        "start() should return Err when process crashes during health check wait"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("exited immediately") || err_msg.contains("7"),
        "Error should mention exit code. Got: {}",
        err_msg
    );
}

/// Spawning a nonexistent command should return an error from start().
#[tokio::test]
async fn test_start_returns_error_for_nonexistent_command() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "no-such-cmd".to_string(),
        command: "this_command_surely_does_not_exist_anywhere_12345".to_string(),
        args: vec![],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let result = manager.start(config).await;
    assert!(
        result.is_err(),
        "start() should return Err for nonexistent command"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Failed to spawn"),
        "Error should mention spawn failure. Got: {}",
        err_msg
    );
}

/// After health check timeout, the process status should be HealthCheckFailed
/// (not Starting or Running).
#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_health_check_timeout_sets_failed_status() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "status-check-test".to_string(),
        command: "sleep".to_string(),
        args: vec!["60".to_string()],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: Some("WILL_NEVER_MATCH".to_string()),
        health_check_timeout_secs: Some(1),
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    // start() returns error, but the process entry still exists
    let _ = manager.start(config).await;

    // Verify the stored status is HealthCheckFailed
    let get_result = manager.get("status-check-test").await;
    assert!(get_result.is_ok());
    let summary = get_result.unwrap();
    assert!(
        matches!(summary.status, ProcessStatus::HealthCheckFailed),
        "Status should be HealthCheckFailed but was {:?}",
        summary.status
    );

    // Cleanup
    let _ = manager.stop("status-check-test", true).await;
}

/// Process that exits with code 0 immediately (no health check) should
/// report crashed status since it didn't stay alive.
#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_start_returns_error_when_process_exits_zero_immediately() {
    let manager = ProcessManager::new();

    let config = ProcessConfig {
        id: "exit-zero-test".to_string(),
        command: "true".to_string(),
        args: vec![],
        cwd: None,
        env: HashMap::new(),
        health_check_pattern: None,
        health_check_timeout_secs: None,
        expected_port: None,
        auto_restart: false,
        max_restart_attempts: 0,
    };

    let result = manager.start(config).await;
    // Even exit code 0 means the process didn't stay running, which is
    // a failure for a background process manager.
    assert!(
        result.is_err(),
        "start() should return Err when process exits immediately even with code 0"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("exited immediately"),
        "Error should mention immediate exit. Got: {}",
        err_msg
    );
}
