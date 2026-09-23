use super::*;
use tempfile::tempdir;

#[test]
fn test_task_checkpoint_new() {
    let checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
    assert_eq!(checkpoint.task_id, "task_123");
    assert_eq!(checkpoint.task_description, "Test task");
    assert_eq!(checkpoint.status, TaskStatus::InProgress);
    assert_eq!(checkpoint.current_step, 0);
    assert_eq!(checkpoint.current_iteration, 0);
}

#[test]
fn test_task_checkpoint_to_summary() {
    let checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
    let summary = checkpoint.to_summary();
    assert_eq!(summary.task_id, "task_123");
    assert_eq!(summary.task_description, "Test task");
    assert_eq!(summary.status, TaskStatus::InProgress);
}

#[test]
fn test_task_checkpoint_log_tool_call() {
    let mut checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
    let log = ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_read".to_string(),
        arguments: r#"{"path": "test.txt"}"#.to_string(),
        result: Some("content".to_string()),
        success: true,
        duration_ms: Some(100),
    };
    checkpoint.log_tool_call(log);
    assert_eq!(checkpoint.tool_calls.len(), 1);
}

#[test]
fn test_task_checkpoint_log_error() {
    let mut checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
    checkpoint.log_error(1, "Test error".to_string(), true);
    assert_eq!(checkpoint.errors.len(), 1);
    assert!(checkpoint.errors[0].recovered);
}

#[test]
fn test_task_checkpoint_set_step() {
    let mut checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
    checkpoint.set_step(5);
    assert_eq!(checkpoint.current_step, 5);
}

#[test]
fn test_task_checkpoint_set_iteration() {
    let mut checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
    checkpoint.set_iteration(12);
    assert_eq!(checkpoint.current_iteration, 12);
}

#[test]
fn test_task_checkpoint_set_status() {
    let mut checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
    checkpoint.set_status(TaskStatus::Completed);
    assert_eq!(checkpoint.status, TaskStatus::Completed);
}

#[test]
fn test_task_status_serde() {
    let status = TaskStatus::InProgress;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"in_progress\"");

    let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, TaskStatus::InProgress);
}

#[test]
#[cfg(unix)]
fn checkpoint_dir_0700_and_files_0600() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempdir().unwrap();
    let cpdir = dir.path().join("cps");
    let manager = CheckpointManager::new(cpdir.clone()).unwrap();

    let dmode = fs::metadata(&cpdir).unwrap().permissions().mode() & 0o777;
    assert_eq!(dmode, 0o700, "dir should be 0700, got {:o}", dmode);

    let cp = TaskCheckpoint::new("perm-test".to_string(), "d".to_string());
    manager.save(&cp).unwrap();

    let file = fs::read_dir(&cpdir)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().extension().is_some_and(|x| x == "json"))
        .expect("a checkpoint json file should exist");
    let fmode = fs::metadata(file.path()).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        fmode, 0o600,
        "checkpoint file should be 0600, got {:o}",
        fmode
    );
}

#[test]
fn test_checkpoint_manager_new() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    assert!(manager.checkpoints_dir().exists());
}

#[test]
fn test_checkpoint_manager_save_load() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
    manager.save(&checkpoint).unwrap();

    let loaded = manager.load("task_123").unwrap();
    assert_eq!(loaded.task_id, "task_123");
    assert_eq!(loaded.task_description, "Test task");
}

#[test]
fn test_checkpoint_manager_list_tasks() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let checkpoint1 = TaskCheckpoint::new("task_1".to_string(), "Task 1".to_string());
    let checkpoint2 = TaskCheckpoint::new("task_2".to_string(), "Task 2".to_string());

    manager.save(&checkpoint1).unwrap();
    manager.save(&checkpoint2).unwrap();

    let tasks = manager.list_tasks().unwrap();
    assert_eq!(tasks.len(), 2);
}

#[test]
fn test_checkpoint_manager_delete() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
    manager.save(&checkpoint).unwrap();
    assert!(manager.exists("task_123"));

    manager.delete("task_123").unwrap();
    assert!(!manager.exists("task_123"));
}

#[test]
fn test_checkpoint_manager_exists() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    assert!(!manager.exists("nonexistent"));

    let checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
    manager.save(&checkpoint).unwrap();
    assert!(manager.exists("task_123"));
}

#[test]
fn test_checkpoint_serialization_round_trip() {
    let mut checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
    checkpoint.set_step(5);
    checkpoint.set_iteration(9);
    checkpoint.set_status(TaskStatus::Paused);
    checkpoint.messages.push(Message::user("Hello"));
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_read".to_string(),
        arguments: "{}".to_string(),
        result: Some("content".to_string()),
        success: true,
        duration_ms: Some(50),
    });

    let json = serde_json::to_string_pretty(&checkpoint).unwrap();
    let loaded: TaskCheckpoint = serde_json::from_str(&json).unwrap();

    assert_eq!(loaded.task_id, checkpoint.task_id);
    assert_eq!(loaded.current_step, 5);
    assert_eq!(loaded.current_iteration, 9);
    assert_eq!(loaded.status, TaskStatus::Paused);
    assert_eq!(loaded.messages.len(), 1);
    assert_eq!(loaded.tool_calls.len(), 1);
}

#[test]
fn budget_caps_survive_serialization_round_trip() {
    let mut checkpoint = TaskCheckpoint::new("task_caps".to_string(), "Capped task".to_string());
    checkpoint.max_budget_tokens = Some(500_000);
    checkpoint.max_wall_secs = Some(21_600);
    checkpoint.max_cost_usd = Some(4.50);

    let json = serde_json::to_string(&checkpoint).unwrap();
    let loaded: TaskCheckpoint = serde_json::from_str(&json).unwrap();

    assert_eq!(loaded.max_budget_tokens, Some(500_000));
    assert_eq!(loaded.max_wall_secs, Some(21_600));
    assert_eq!(loaded.max_cost_usd, Some(4.50));
}

#[test]
fn budget_caps_default_to_none_on_legacy_checkpoint() {
    // A checkpoint written before the caps existed (no cap keys in the JSON)
    // must deserialize with the caps as None via serde(default), not fail.
    let checkpoint = TaskCheckpoint::new("old".to_string(), "d".to_string());
    let mut value = serde_json::to_value(&checkpoint).unwrap();
    let obj = value.as_object_mut().unwrap();
    obj.remove("max_budget_tokens");
    obj.remove("max_wall_secs");
    obj.remove("max_cost_usd");

    let loaded: TaskCheckpoint = serde_json::from_value(value).unwrap();
    assert_eq!(loaded.max_budget_tokens, None);
    assert_eq!(loaded.max_wall_secs, None);
    assert_eq!(loaded.max_cost_usd, None);
}

#[test]
fn test_checkpoint_deserialize_without_iteration_defaults_zero() {
    let json = r#"{
            "task_id":"task_old",
            "task_description":"legacy",
            "created_at":"2026-01-01T00:00:00Z",
            "updated_at":"2026-01-01T00:00:00Z",
            "status":"in_progress",
            "current_step":2,
            "messages":[],
            "memory_entries":[],
            "estimated_tokens":0,
            "tool_calls":[],
            "errors":[],
            "git_checkpoint":null
        }"#;

    let loaded: TaskCheckpoint = serde_json::from_str(json).unwrap();
    assert_eq!(loaded.current_step, 2);
    assert_eq!(loaded.current_iteration, 0);
}

#[test]
fn test_git_checkpoint_info_serde() {
    let info = GitCheckpointInfo {
        branch: "main".to_string(),
        commit_hash: "abc123".to_string(),
        dirty: true,
        staged_files: vec!["file1.rs".to_string()],
        modified_files: vec!["file2.rs".to_string()],
    };

    let json = serde_json::to_string(&info).unwrap();
    let loaded: GitCheckpointInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(loaded.branch, "main");
    assert_eq!(loaded.commit_hash, "abc123");
    assert!(loaded.dirty);
}

#[test]
fn test_task_status_completed_serde() {
    let status = TaskStatus::Completed;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"completed\"");
}

#[test]
fn test_task_status_failed_serde() {
    let status = TaskStatus::Failed;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"failed\"");
}

#[test]
fn test_task_status_paused_serde() {
    let status = TaskStatus::Paused;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"paused\"");
}

#[test]
fn test_memory_entry_struct() {
    let entry = MemoryEntry {
        timestamp: "2024-01-01T00:00:00Z".to_string(),
        role: "user".to_string(),
        content: "test content".to_string(),
        token_estimate: 100,
    };
    assert_eq!(entry.role, "user");
    assert_eq!(entry.token_estimate, 100);
}

#[test]
fn test_tool_call_log_struct() {
    let log = ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_read".to_string(),
        arguments: "{}".to_string(),
        result: None,
        success: false,
        duration_ms: None,
    };
    assert_eq!(log.tool_name, "file_read");
    assert!(!log.success);
}

#[test]
fn test_error_log_struct() {
    let log = ErrorLog {
        timestamp: Utc::now(),
        step: 5,
        error: "something failed".to_string(),
        recovered: false,
    };
    assert_eq!(log.step, 5);
    assert!(!log.recovered);
}

#[test]
fn test_task_summary_struct() {
    let summary = TaskSummary {
        task_id: "abc".to_string(),
        task_description: "desc".to_string(),
        status: TaskStatus::InProgress,
        current_step: 3,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        tool_call_count: 10,
        error_count: 2,
        project_root: None,
    };
    assert_eq!(summary.current_step, 3);
    assert_eq!(summary.tool_call_count, 10);
}

#[test]
fn test_checkpoint_set_messages() {
    let mut checkpoint = TaskCheckpoint::new("task_1".to_string(), "Test".to_string());
    let messages = vec![Message::system("system"), Message::user("user msg")];
    checkpoint.set_messages(messages);
    assert_eq!(checkpoint.messages.len(), 2);
}

#[test]
fn test_checkpoint_multiple_tool_calls() {
    let mut checkpoint = TaskCheckpoint::new("task_1".to_string(), "Test".to_string());
    for i in 0..5 {
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: format!("tool_{}", i),
            arguments: "{}".to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(i as u64 * 10),
        });
    }
    assert_eq!(checkpoint.tool_calls.len(), 5);
}

#[test]
fn test_checkpoint_multiple_errors() {
    let mut checkpoint = TaskCheckpoint::new("task_1".to_string(), "Test".to_string());
    checkpoint.log_error(1, "error 1".to_string(), true);
    checkpoint.log_error(2, "error 2".to_string(), false);
    checkpoint.log_error(3, "error 3".to_string(), true);
    assert_eq!(checkpoint.errors.len(), 3);
    assert!(!checkpoint.errors[1].recovered);
}

#[test]
fn test_checkpoint_manager_load_nonexistent_recovers_fresh() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    // Changed contract (external review sign-off): loading a nonexistent
    // task is RecoveryRequired, not a fresh checkpoint.
    let err = manager.load("nonexistent_task").unwrap_err().to_string();
    assert!(err.contains("unrecoverable"), "must name the state: {err}");
}

#[test]
fn test_checkpoint_manager_delete_nonexistent() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    // Should not error when deleting nonexistent
    let result = manager.delete("nonexistent_task");
    assert!(result.is_ok());
}

#[test]
fn test_checkpoint_manager_list_empty() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    let tasks = manager.list_tasks().unwrap();
    assert!(tasks.is_empty());
}

#[test]
fn test_git_checkpoint_info_empty_files() {
    let info = GitCheckpointInfo {
        branch: "feature".to_string(),
        commit_hash: "def456".to_string(),
        dirty: false,
        staged_files: vec![],
        modified_files: vec![],
    };
    assert!(!info.dirty);
    assert!(info.staged_files.is_empty());
    assert!(info.modified_files.is_empty());
}

#[test]
fn test_checkpoint_with_git_state() {
    let mut checkpoint = TaskCheckpoint::new("task_1".to_string(), "Test".to_string());
    checkpoint.git_checkpoint = Some(GitCheckpointInfo {
        branch: "main".to_string(),
        commit_hash: "abc123def456".to_string(),
        dirty: true,
        staged_files: vec!["src/main.rs".to_string()],
        modified_files: vec![],
    });
    assert!(checkpoint.git_checkpoint.is_some());
    assert_eq!(checkpoint.git_checkpoint.as_ref().unwrap().branch, "main");
}

#[test]
fn test_checkpoint_estimated_tokens() {
    let mut checkpoint = TaskCheckpoint::new("task_1".to_string(), "Test".to_string());
    checkpoint.estimated_tokens = 5000;
    assert_eq!(checkpoint.estimated_tokens, 5000);
}

#[test]
fn test_checkpoint_delta_round_trip() {
    let mut base = TaskCheckpoint::new("task_delta".to_string(), "Delta test".to_string());
    base.set_messages(vec![Message::user("hello")]);
    base.set_step(1);

    let mut next = base.clone();
    next.set_iteration(2);
    next.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_read".to_string(),
        arguments: "{}".to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(10),
    });

    let delta = next.compute_delta(&base).unwrap();
    let mut hydrated = base.clone();
    hydrated.apply_delta(&delta).unwrap();

    assert_eq!(hydrated.current_iteration, next.current_iteration);
    assert_eq!(hydrated.tool_calls.len(), next.tool_calls.len());
    assert_eq!(hydrated.version, next.version);
}

#[test]
fn test_checkpoint_manager_replays_delta_log() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let mut checkpoint =
        TaskCheckpoint::new("task_delta_mgr".to_string(), "Delta manager".to_string());
    let mut large_messages = Vec::new();
    for i in 0..30 {
        large_messages.push(Message::user(format!("message-{} {}", i, "x".repeat(120))));
    }
    checkpoint.set_messages(large_messages);
    manager.save(&checkpoint).unwrap();

    checkpoint.set_step(2);
    checkpoint.set_iteration(3);
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "shell_exec".to_string(),
        arguments: "{\"command\":\"true\"}".to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(1),
    });
    manager.save(&checkpoint).unwrap();

    let delta_path = manager.checkpoint_delta_path("task_delta_mgr").unwrap();
    assert!(delta_path.exists(), "expected delta log to exist");

    let loaded = manager.load("task_delta_mgr").unwrap();
    assert_eq!(loaded.current_step, 2);
    assert_eq!(loaded.current_iteration, 3);
    assert_eq!(loaded.tool_calls.len(), 1);
}

#[test]
fn test_capture_git_state() {
    // We're in a git repo, so this should work
    let state = capture_git_state(".");
    assert!(state.is_some());
    let state = state.unwrap();
    assert!(!state.branch.is_empty());
    assert!(!state.commit_hash.is_empty());
}

#[test]
fn test_capture_git_state_nonexistent_repo() {
    // This should return None for a non-repo directory
    let state = capture_git_state("/tmp");
    // /tmp may or may not be a git repo, so just check it doesn't panic
    // The function should handle this gracefully
    let _ = state;
}

#[test]
fn test_dirs_home_function() {
    let home = dirs_home();
    // Should return a valid path
    assert!(!home.as_os_str().is_empty());
}

#[test]
fn test_checkpoint_manager_creates_nested_dir() {
    let dir = tempdir().unwrap();
    let nested = dir.path().join("a").join("b").join("c");
    let manager = CheckpointManager::new(nested.clone()).unwrap();
    assert!(nested.exists());
    assert!(manager.checkpoints_dir().exists());
}

#[test]
fn test_checkpoint_manager_path() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    let expected = dir.path().join("task_test.json");
    assert_eq!(manager.checkpoint_path("task_test").unwrap(), expected);
}

#[test]
fn test_checkpoint_list_tasks_sorted_by_date() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Create checkpoints with different times
    let mut cp1 = TaskCheckpoint::new("old".to_string(), "Old task".to_string());
    cp1.updated_at = chrono::DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    manager.save(&cp1).unwrap();

    let mut cp2 = TaskCheckpoint::new("new".to_string(), "New task".to_string());
    cp2.updated_at = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    manager.save(&cp2).unwrap();

    let tasks = manager.list_tasks().unwrap();
    assert_eq!(tasks.len(), 2);
    // Most recent should be first
    assert_eq!(tasks[0].task_id, "new");
    assert_eq!(tasks[1].task_id, "old");
}

#[test]
fn test_checkpoint_list_ignores_invalid_json() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Save a valid checkpoint
    let cp = TaskCheckpoint::new("valid".to_string(), "Valid".to_string());
    manager.save(&cp).unwrap();

    // Write invalid JSON file
    std::fs::write(dir.path().join("invalid.json"), "not valid json").unwrap();

    // Write non-JSON file (should be ignored by extension check)
    std::fs::write(dir.path().join("readme.txt"), "some text").unwrap();

    let tasks = manager.list_tasks().unwrap();
    // Should only have the valid checkpoint
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].task_id, "valid");
}

#[test]
fn test_checkpoint_list_tasks_nonexistent_dir() {
    // Create manager then remove the directory
    let dir = tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let manager = CheckpointManager::new(path.clone()).unwrap();

    // Remove the directory
    std::fs::remove_dir_all(&path).unwrap();

    // list_tasks should return empty, not error
    let tasks = manager.list_tasks().unwrap();
    assert!(tasks.is_empty());
}

#[test]
fn test_memory_entry_serde() {
    let entry = MemoryEntry {
        timestamp: "2024-01-01T00:00:00Z".to_string(),
        role: "assistant".to_string(),
        content: "Hello there".to_string(),
        token_estimate: 50,
    };
    let json = serde_json::to_string(&entry).unwrap();
    let loaded: MemoryEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.role, "assistant");
    assert_eq!(loaded.token_estimate, 50);
}

#[test]
fn test_tool_call_log_serde() {
    let log = ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "shell_exec".to_string(),
        arguments: r#"{"command": "ls"}"#.to_string(),
        result: Some("file1\nfile2".to_string()),
        success: true,
        duration_ms: Some(150),
    };
    let json = serde_json::to_string(&log).unwrap();
    let loaded: ToolCallLog = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.tool_name, "shell_exec");
    assert!(loaded.success);
}

#[test]
fn test_error_log_serde() {
    let log = ErrorLog {
        timestamp: Utc::now(),
        step: 10,
        error: "connection timeout".to_string(),
        recovered: true,
    };
    let json = serde_json::to_string(&log).unwrap();
    let loaded: ErrorLog = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.step, 10);
    assert!(loaded.recovered);
}

// ---- Checkpoint integrity tests ----

#[test]
fn test_checkpoint_envelope_round_trip() {
    let payload = serde_json::json!({"task_id": "test", "data": "hello"});
    let envelope = CheckpointEnvelope::wrap(payload.clone()).unwrap();
    assert!(!envelope.sha256.is_empty());
    assert_eq!(envelope.payload, payload);
    assert!(envelope.verify().is_ok());
}

#[test]
fn test_checkpoint_envelope_detects_tampering() {
    let payload = serde_json::json!({"task_id": "test", "data": "hello"});
    let mut envelope = CheckpointEnvelope::wrap(payload).unwrap();
    // Tamper with the payload
    envelope.payload = serde_json::json!({"task_id": "test", "data": "TAMPERED"});
    assert!(envelope.verify().is_err());
}

#[test]
fn test_checkpoint_envelope_detects_bad_hash() {
    let payload = serde_json::json!({"task_id": "test"});
    let mut envelope = CheckpointEnvelope::wrap(payload).unwrap();
    // Corrupt the hash
    envelope.sha256 =
        "0000000000000000000000000000000000000000000000000000000000000000".to_string();
    assert!(envelope.verify().is_err());
}

#[test]
fn test_save_load_with_integrity() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let checkpoint =
        TaskCheckpoint::new("integrity_test".to_string(), "Integrity test".to_string());
    manager.save(&checkpoint).unwrap();

    // Load should succeed and verify integrity
    let loaded = manager.load("integrity_test").unwrap();
    assert_eq!(loaded.task_id, "integrity_test");
    assert_eq!(loaded.task_description, "Integrity test");
}

#[test]
fn test_load_detects_corrupted_file_and_recovers() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Save a valid checkpoint
    let checkpoint = TaskCheckpoint::new("corrupt_test".to_string(), "Corruption test".to_string());
    manager.save(&checkpoint).unwrap();

    // Corrupt the file by modifying the payload while keeping envelope structure
    let path = dir.path().join("corrupt_test.json");
    let content = std::fs::read_to_string(&path).unwrap();
    let mut envelope: serde_json::Value = serde_json::from_str(&content).unwrap();
    envelope["payload"]["task_description"] = serde_json::Value::String("TAMPERED".to_string());
    std::fs::write(&path, serde_json::to_string_pretty(&envelope).unwrap()).unwrap();

    // Load should detect corruption and refuse (changed contract, external
    // review sign-off: no backup exists, so recovery cannot restore anything
    // — a fresh blank checkpoint is NOT a valid substitute for the resume).
    let err = manager.load("corrupt_test").unwrap_err().to_string();
    assert!(err.contains("unrecoverable"), "must name the state: {err}");
}

#[test]
fn test_try_load_from_path_detects_integrity_error() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Save a valid checkpoint
    let checkpoint = TaskCheckpoint::new("direct_test".to_string(), "Direct load test".to_string());
    manager.save(&checkpoint).unwrap();

    // Corrupt the file
    let path = dir.path().join("direct_test.json");
    let content = std::fs::read_to_string(&path).unwrap();
    let mut envelope: serde_json::Value = serde_json::from_str(&content).unwrap();
    envelope["payload"]["task_description"] = serde_json::Value::String("TAMPERED".to_string());
    std::fs::write(&path, serde_json::to_string_pretty(&envelope).unwrap()).unwrap();

    // try_load_from_path should fail with integrity error
    let result = manager.try_load_from_path(&path);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("integrity"),
        "Expected integrity error, got: {}",
        err_msg
    );
}

#[test]
fn test_load_legacy_format_backward_compatible() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Write a legacy-format checkpoint (bare JSON without envelope)
    let checkpoint = TaskCheckpoint::new("legacy_test".to_string(), "Legacy format".to_string());
    let bare_json = serde_json::to_string_pretty(&checkpoint).unwrap();
    let path = dir.path().join("legacy_test.json");
    std::fs::write(&path, bare_json).unwrap();

    // Load should succeed via legacy fallback
    let loaded = manager.load("legacy_test").unwrap();
    assert_eq!(loaded.task_id, "legacy_test");
    assert_eq!(loaded.task_description, "Legacy format");
}

#[test]
fn test_save_redacts_secrets_in_messages() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let mut checkpoint =
        TaskCheckpoint::new("redact_test".to_string(), "Secret redaction".to_string());
    checkpoint.messages.push(Message::user(
        "Use api_key=sk-secretkey12345678901234567890 to connect",
    ));
    manager.save(&checkpoint).unwrap();

    // Read the raw file and verify secrets are redacted
    let path = dir.path().join("redact_test.json");
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(
        !raw.contains("sk-secretkey12345678901234567890"),
        "API key should have been redacted in checkpoint file"
    );
    assert!(raw.contains("[REDACTED]"));
}

#[test]
fn test_list_tasks_handles_envelope_format() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Save with new envelope format
    let cp1 = TaskCheckpoint::new("env_task".to_string(), "Envelope task".to_string());
    manager.save(&cp1).unwrap();

    // Also write a legacy bare-format file
    let cp2 = TaskCheckpoint::new("bare_task".to_string(), "Bare task".to_string());
    let bare_json = serde_json::to_string_pretty(&cp2).unwrap();
    std::fs::write(dir.path().join("bare_task.json"), bare_json).unwrap();

    let tasks = manager.list_tasks().unwrap();
    assert_eq!(tasks.len(), 2);
    let ids: Vec<&str> = tasks.iter().map(|t| t.task_id.as_str()).collect();
    assert!(ids.contains(&"env_task"));
    assert!(ids.contains(&"bare_task"));
}

// ---- Corruption recovery tests ----

#[test]
fn test_recover_from_corruption_uses_backup() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Save a valid checkpoint (this also creates the primary file)
    let checkpoint = TaskCheckpoint::new("recover_bak".to_string(), "Backup recovery".to_string());
    manager.save(&checkpoint).unwrap();

    // Manually create a backup copy of the valid file
    let primary = dir.path().join("recover_bak.json");
    let backup = dir.path().join("recover_bak.json.bak");
    std::fs::copy(&primary, &backup).unwrap();

    // Now corrupt the primary file
    std::fs::write(&primary, "THIS IS NOT JSON").unwrap();

    // Load should recover from the backup
    let loaded = manager.load("recover_bak").unwrap();
    assert_eq!(loaded.task_id, "recover_bak");
    assert_eq!(loaded.task_description, "Backup recovery");
}

#[test]
fn test_recover_from_corruption_creates_fresh_when_no_backup() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Write a corrupt primary file with no backup
    let primary = dir.path().join("no_bak.json");
    std::fs::write(&primary, "CORRUPT DATA").unwrap();

    // Changed contract (external review sign-off): RecoveryRequired error,
    // not a fresh checkpoint.
    let err = manager.load("no_bak").unwrap_err().to_string();
    assert!(err.contains("unrecoverable"), "must name the state: {err}");
}

#[test]
fn test_recover_from_corruption_creates_fresh_when_backup_also_corrupt() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Write corrupt primary and corrupt backup
    let primary = dir.path().join("both_bad.json");
    let backup = dir.path().join("both_bad.json.bak");
    std::fs::write(&primary, "CORRUPT").unwrap();
    std::fs::write(&backup, "ALSO CORRUPT").unwrap();

    // Changed contract (external review sign-off): RecoveryRequired error,
    // not a fresh checkpoint.
    let err = manager.load("both_bad").unwrap_err().to_string();
    assert!(err.contains("unrecoverable"), "must name the state: {err}");
}

#[test]
fn test_recover_from_corruption_resaves_recovered() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Save a valid checkpoint
    let checkpoint = TaskCheckpoint::new(
        "resave_test".to_string(),
        "Resave after recovery".to_string(),
    );
    manager.save(&checkpoint).unwrap();

    // Create backup, then corrupt primary
    let primary = dir.path().join("resave_test.json");
    let backup = dir.path().join("resave_test.json.bak");
    std::fs::copy(&primary, &backup).unwrap();
    std::fs::write(&primary, "CORRUPT").unwrap();

    // First load triggers recovery
    let loaded = manager.load("resave_test").unwrap();
    assert_eq!(loaded.task_description, "Resave after recovery");

    // Remove backup; second load should succeed from re-saved primary
    std::fs::remove_file(&backup).unwrap();
    let loaded2 = manager.load("resave_test").unwrap();
    assert_eq!(loaded2.task_description, "Resave after recovery");
}

#[test]
fn test_recover_detects_integrity_failure_and_falls_back() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Save a valid checkpoint
    let checkpoint = TaskCheckpoint::new(
        "integrity_recover".to_string(),
        "Integrity recovery".to_string(),
    );
    manager.save(&checkpoint).unwrap();

    // Create a good backup
    let primary = dir.path().join("integrity_recover.json");
    let backup = dir.path().join("integrity_recover.json.bak");
    std::fs::copy(&primary, &backup).unwrap();

    // Tamper with primary envelope payload (valid JSON but bad hash)
    let content = std::fs::read_to_string(&primary).unwrap();
    let mut envelope: serde_json::Value = serde_json::from_str(&content).unwrap();
    envelope["payload"]["task_description"] = serde_json::Value::String("TAMPERED".to_string());
    std::fs::write(&primary, serde_json::to_string_pretty(&envelope).unwrap()).unwrap();

    // Load should detect integrity failure and recover from backup
    let loaded = manager.load("integrity_recover").unwrap();
    assert_eq!(loaded.task_description, "Integrity recovery");
}

// ---- Retry logic tests ----

#[test]
fn test_save_with_retry_succeeds_immediately() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let checkpoint = TaskCheckpoint::new("retry_ok".to_string(), "Retry success".to_string());
    manager.save_with_retry(&checkpoint).unwrap();

    let loaded = manager.load("retry_ok").unwrap();
    assert_eq!(loaded.task_id, "retry_ok");
}

#[test]
#[cfg(not(target_os = "windows"))] // On Windows, set_readonly on directories doesn't prevent file creation inside them
fn test_save_with_retry_fails_on_readonly_dir() {
    // Create a directory and make it read-only so saves fail
    let dir = tempdir().unwrap();
    let readonly_dir = dir.path().join("readonly_checkpoints");
    std::fs::create_dir_all(&readonly_dir).unwrap();
    let manager = CheckpointManager::new(readonly_dir.clone()).unwrap();

    // Make directory read-only
    let mut perms = std::fs::metadata(&readonly_dir).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    {
        perms.set_readonly(true);
    }
    std::fs::set_permissions(&readonly_dir, perms.clone()).unwrap();

    let checkpoint = TaskCheckpoint::new(
        "retry_fail".to_string(),
        "Should fail all retries".to_string(),
    );
    let result = manager.save_with_retry(&checkpoint);
    assert!(result.is_err());

    // Restore permissions so tempdir cleanup works
    #[allow(clippy::permissions_set_readonly_false)]
    {
        perms.set_readonly(false);
    }
    std::fs::set_permissions(&readonly_dir, perms).unwrap();
}

#[test]
#[cfg(target_os = "windows")]
fn test_save_with_retry_fails_on_readonly_dir() {
    // On Windows, directory readonly attributes don't prevent file creation.
    // Instead, we create a CheckpointManager whose base_dir is under a
    // regular file (an impossible path), so saves physically cannot succeed.
    let dir = tempdir().unwrap();
    let blocker_file = dir.path().join("blocker");
    std::fs::write(&blocker_file, "not a directory").unwrap();

    // Manually construct a manager pointing to a path that can never work:
    // "blocker" is a file, so "blocker/checkpoints" can't be a directory.
    let impossible_dir = blocker_file.join("checkpoints");
    let manager = CheckpointManager::with_dir(impossible_dir);

    let checkpoint = TaskCheckpoint::new(
        "retry_fail".to_string(),
        "Should fail all retries".to_string(),
    );
    let result = manager.save_with_retry(&checkpoint);
    assert!(result.is_err());
}

// ---- Path traversal security tests ----

#[test]
fn test_sanitize_task_id_rejects_dotdot() {
    assert!(sanitize_task_id("../evil").is_err());
    assert!(sanitize_task_id("foo/../../bar").is_err());
    assert!(sanitize_task_id("..\\..\\evil").is_err());
    assert!(sanitize_task_id("..").is_err());
}

#[test]
fn test_sanitize_task_id_replaces_separators() {
    let id = sanitize_task_id("foo/bar").unwrap();
    assert_eq!(id, "foo_bar");
    assert!(!id.contains('/'));

    let id = sanitize_task_id("foo\\bar").unwrap();
    assert_eq!(id, "foo_bar");
    assert!(!id.contains('\\'));
}

#[test]
fn test_sanitize_task_id_preserves_safe_id() {
    let id = sanitize_task_id("task_123").unwrap();
    assert_eq!(id, "task_123");
}

#[test]
fn test_sanitize_task_id_rejects_empty() {
    assert!(sanitize_task_id("").is_err());
    assert!(sanitize_task_id("   ").is_err());
    assert!(sanitize_task_id("..").is_err());
}

#[test]
fn test_checkpoint_path_traversal_rejected() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // A task_id with "../" should be rejected, not produce a path outside dir.
    let result = manager.checkpoint_path("../evil");
    assert!(
        result.is_err(),
        "checkpoint_path should reject path traversal"
    );

    let result = manager.checkpoint_delta_path("../evil");
    assert!(
        result.is_err(),
        "checkpoint_delta_path should reject path traversal"
    );

    // Saving with a traversal task_id should also fail.
    let checkpoint = TaskCheckpoint::new("../evil".to_string(), "evil".to_string());
    let result = manager.save(&checkpoint);
    assert!(result.is_err(), "save should reject path traversal task_id");

    // Loading with a traversal task_id should also fail.
    let result = manager.load("../evil");
    assert!(result.is_err(), "load should reject path traversal task_id");

    // Deleting with a traversal task_id should also fail.
    let result = manager.delete("../evil");
    assert!(
        result.is_err(),
        "delete should reject path traversal task_id"
    );
}

#[test]
fn test_checkpoint_path_stays_in_dir() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let path = manager.checkpoint_path("normal_task").unwrap();
    assert!(
        path.starts_with(dir.path()),
        "path should stay within checkpoints dir"
    );
    assert_eq!(path.file_name().unwrap(), "normal_task.json");
}

// ---- Retention pruning tests ----

#[test]
fn test_prune_old_checkpoints_caps_files() {
    // Use a small cap by creating many checkpoints and verifying pruning
    // keeps at most MAX_CHECKPOINT_FILES.  We can't easily change the const,
    // but we can test the pruning logic directly by creating more files
    // than the cap and calling save (which triggers pruning).
    //
    // Since MAX_CHECKPOINT_FILES is 500, we test with a smaller number
    // by creating files directly and calling the private method via save.
    // Instead, we'll verify that the prune function correctly handles
    // the case where we have exactly the cap (no deletion) and more than
    // the cap (deletion of oldest).
    //
    // For a practical test, we create a few checkpoint files with
    // controlled mtimes and verify the oldest get pruned.

    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Save a few checkpoints normally.
    for i in 0..3 {
        let cp = TaskCheckpoint::new(format!("task_{}", i), format!("Task {}", i));
        manager.save(&cp).unwrap();
        // Small delay so mtimes differ.
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    // All 3 should still exist (well under cap of 500).
    for i in 0..3 {
        assert!(
            manager.exists(&format!("task_{}", i)),
            "task_{} should still exist after pruning",
            i
        );
    }
}

#[test]
fn test_prune_old_checkpoints_deletes_oldest() {
    // This test directly exercises the pruning logic by creating more
    // checkpoint files than MAX_CHECKPOINT_FILES would allow, but since
    // we can't change the const at runtime, we instead test that
    // prune_old_checkpoints correctly identifies and would delete old
    // files.  We test the core behavior: after saving, the directory
    // doesn't grow unboundedly.
    //
    // We create checkpoint files directly and verify the pruning
    // mechanism works by simulating it with a controlled setup:
    // create many .json files, then trigger save() and verify the
    // count stays at or below the cap.
    //
    // Given MAX_CHECKPOINT_FILES=500, we create 3 checkpoints and
    // verify none are pruned.  This validates the "under cap" path.

    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Create 3 checkpoints with staggered mtimes.
    for i in 0..3 {
        let cp = TaskCheckpoint::new(format!("prune_{}", i), format!("Prune {}", i));
        manager.save(&cp).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(15));
    }

    let tasks = manager.list_tasks().unwrap();
    assert_eq!(tasks.len(), 3, "all 3 checkpoints should exist");

    // Saving the same checkpoint again should not cause pruning of others.
    let cp = TaskCheckpoint::new("prune_0".to_string(), "Prune 0 updated".to_string());
    manager.save(&cp).unwrap();

    let tasks = manager.list_tasks().unwrap();
    assert_eq!(tasks.len(), 3, "still 3 checkpoints after re-save");
}

#[test]
fn test_prune_with_large_count_stays_within_cap() {
    // Create more than MAX_CHECKPOINT_FILES (500) checkpoint files and
    // verify that after a save, the count is at or below the cap.
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Create 502 checkpoint files directly (faster than full save).
    for i in 0..502 {
        let task_id = format!("bulk_{}", i);
        let path = dir.path().join(format!("{}.json", task_id));
        let cp = TaskCheckpoint::new(task_id, format!("Bulk {}", i));
        let json_value = serde_json::to_value(&cp).unwrap();
        let envelope = CheckpointEnvelope::wrap(json_value).unwrap();
        let json = serde_json::to_string_pretty(&envelope).unwrap();
        std::fs::write(&path, json).unwrap();
    }

    // Count files before pruning.
    let count_before: usize = std::fs::read_dir(dir.path())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .map(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
                .unwrap_or(false)
        })
        .count();
    assert_eq!(count_before, 502);

    // Save one more checkpoint — this triggers prune_old_checkpoints.
    let cp = TaskCheckpoint::new("trigger_prune".to_string(), "Trigger".to_string());
    manager.save(&cp).unwrap();

    // Count files after pruning — should be at most MAX_CHECKPOINT_FILES + 1
    // (the +1 is the just-saved "trigger_prune.json").
    let count_after: usize = std::fs::read_dir(dir.path())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .map(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
                .unwrap_or(false)
        })
        .count();

    assert!(
        count_after <= MAX_CHECKPOINT_FILES + 1,
        "after pruning, file count {} should be at most {} (cap + 1 for the just-saved file)",
        count_after,
        MAX_CHECKPOINT_FILES + 1
    );
    assert!(
        count_after >= MAX_CHECKPOINT_FILES,
        "after pruning, should still have at least {} files (the cap), got {}",
        MAX_CHECKPOINT_FILES,
        count_after
    );
}

#[test]
fn prune_caps_per_task_subdirs() {
    let dir = tempdir().unwrap();
    let cpdir = dir.path().join("cps");
    let manager = CheckpointManager::new(cpdir.clone()).unwrap();

    // Create more than the cap of stale per-task subdirs (as the
    // failure_mode.json artifact writer does — one per run). These are
    // exactly what the flat-.json prune ignored, so they leaked unbounded.
    let over = MAX_CHECKPOINT_FILES + 20;
    for i in 0..over {
        let d = cpdir.join(format!("task-{i:04}"));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("failure_mode.json"), "{}").unwrap();
    }

    // A save triggers prune_old_checkpoints -> prune_old_task_dirs even
    // though there are few .json files (the early-return case).
    let cp = TaskCheckpoint::new("live-task".to_string(), "d".to_string());
    manager.save(&cp).unwrap();

    let remaining = std::fs::read_dir(&cpdir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .count();
    assert!(
        remaining <= MAX_CHECKPOINT_FILES,
        "per-task subdirs must be capped at {}, found {}",
        MAX_CHECKPOINT_FILES,
        remaining
    );
}

#[test]
fn task_checkpoint_budget_fields_roundtrip_and_default() {
    let mut cp = TaskCheckpoint::new("t1".to_string(), "desc".to_string());
    cp.cumulative_tokens = 12345;
    cp.elapsed_wall_secs = 678;
    cp.cumulative_cost_usd = 1.2345;
    cp.guard_counters = GuardCounters {
        consecutive_no_action_prompts: 3,
        mutation_gate_rejections: 5,
        prefill_400_count: 2,
        mutation_sequence: 7,
        last_successful_verification_mutation_sequence: 4,
        last_failed_verification_mutation_sequence: 6,
        last_failed_verification_summary: Some("pytest: 2 failed".to_string()),
        verification_failures: Default::default(),
        verification_fingerprint: None,
    };
    let json = serde_json::to_string(&cp).unwrap();
    let back: TaskCheckpoint = serde_json::from_str(&json).unwrap();
    assert_eq!(back.cumulative_tokens, 12345);
    assert_eq!(back.elapsed_wall_secs, 678);
    assert_eq!(back.cumulative_cost_usd, 1.2345);
    assert_eq!(back.guard_counters.mutation_gate_rejections, 5);
    assert_eq!(back.guard_counters.prefill_400_count, 2);
    // Verification ledger survives the round-trip (stale-verification-after-
    // resume fix): unverified pre-checkpoint edits stay unverified.
    assert_eq!(back.guard_counters.mutation_sequence, 7);
    assert_eq!(
        back.guard_counters
            .last_successful_verification_mutation_sequence,
        4
    );
    assert_eq!(
        back.guard_counters
            .last_failed_verification_mutation_sequence,
        6
    );
    assert_eq!(
        back.guard_counters
            .last_failed_verification_summary
            .as_deref(),
        Some("pytest: 2 failed")
    );

    // Legacy checkpoints without these fields must default to 0, not fail.
    let mut legacy_value =
        serde_json::to_value(TaskCheckpoint::new("t2".to_string(), "d".to_string())).unwrap();
    // Remove the new budget fields to simulate a legacy checkpoint.
    if let serde_json::Value::Object(ref mut map) = legacy_value {
        map.remove("cumulative_tokens");
        map.remove("elapsed_wall_secs");
        map.remove("cumulative_cost_usd");
        map.remove("guard_counters");
    }
    let restored: TaskCheckpoint = serde_json::from_value(legacy_value).unwrap();
    assert_eq!(restored.cumulative_tokens, 0);
    assert_eq!(restored.elapsed_wall_secs, 0);
    assert_eq!(restored.cumulative_cost_usd, 0.0);
    assert_eq!(restored.guard_counters, GuardCounters::default());
}

#[test]
fn delta_carries_cumulative_budget_across_apply() {
    let mut base = TaskCheckpoint::new("t".to_string(), "d".to_string());
    base.cumulative_tokens = 100;
    base.elapsed_wall_secs = 30;
    base.cumulative_cost_usd = 0.10;
    // A newer checkpoint that consumed more budget.
    let mut newer = base.clone();
    newer.version = base.version + 1;
    newer.current_step = base.current_step + 1;
    newer.cumulative_tokens = 500;
    newer.elapsed_wall_secs = 90;
    newer.cumulative_cost_usd = 0.75;
    newer.guard_counters = GuardCounters {
        consecutive_no_action_prompts: 2,
        mutation_gate_rejections: 4,
        prefill_400_count: 1,
        ..GuardCounters::default()
    };
    let delta = newer.compute_delta(&base).expect("delta should exist");
    assert_eq!(delta.cumulative_tokens, Some(500));
    assert_eq!(delta.elapsed_wall_secs, Some(90));
    assert_eq!(delta.cumulative_cost_usd, Some(0.75));
    assert_eq!(
        delta
            .guard_counters
            .as_ref()
            .unwrap()
            .mutation_gate_rejections,
        4
    );
    // Applying the delta to the base must update the budget (not keep it stale).
    let mut reconstructed = base.clone();
    reconstructed.apply_delta(&delta).unwrap();
    assert_eq!(reconstructed.cumulative_tokens, 500);
    assert_eq!(reconstructed.elapsed_wall_secs, 90);
    assert_eq!(reconstructed.cumulative_cost_usd, 0.75);
    // Guard counters carry across the delta apply so resume can't reset them.
    assert_eq!(reconstructed.guard_counters.mutation_gate_rejections, 4);
    assert_eq!(reconstructed.guard_counters.prefill_400_count, 1);
    assert_eq!(
        reconstructed.guard_counters.consecutive_no_action_prompts,
        2
    );
}

#[test]
fn save_final_makes_base_reflect_terminal_state() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Base written mid-run: in-progress at step 1.
    let mut cp = TaskCheckpoint::new("t-final".to_string(), "d".to_string());
    cp.set_status(TaskStatus::InProgress);
    cp.set_step(1);
    manager.save(&cp).unwrap();

    // A delta advances progress (as periodic saves do).
    cp.set_step(4);
    manager.save(&cp).unwrap();

    // Finalize: full write so the base itself is terminal.
    cp.set_status(TaskStatus::Completed);
    cp.set_step(7);
    cp.set_iteration(3);
    manager.save_final(&cp).unwrap();

    let loaded = manager.load("t-final").unwrap();
    assert_eq!(loaded.status, TaskStatus::Completed);
    assert_eq!(loaded.current_step, 7);
    assert_eq!(loaded.current_iteration, 3);
}

#[test]
fn auto_continue_count_round_trips_through_save_and_delta() {
    // Probe: the auto-continue chain count must survive full writes, the
    // differential-save path, and load — including the detect-lost-field case
    // (a delta that does not mention the field must keep the base's value).
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let mut cp = TaskCheckpoint::new("t-chain".to_string(), "d".to_string());
    cp.auto_continue_count = 3;
    manager.save(&cp).unwrap();

    // A delta save advances unrelated fields; the count field stays put.
    cp.set_step(4);
    cp.set_iteration(7);
    manager.save(&cp).unwrap();

    let loaded = manager.load("t-chain").unwrap();
    assert_eq!(loaded.auto_continue_count, 3);
    assert_eq!(loaded.current_step, 4);
    assert_eq!(loaded.current_iteration, 7);

    // Full terminal write must also preserve it.
    cp.set_status(TaskStatus::Completed);
    manager.save_final(&cp).unwrap();
    let loaded = manager.load("t-chain").unwrap();
    assert_eq!(loaded.auto_continue_count, 3);
    assert_eq!(loaded.status, TaskStatus::Completed);
}

#[test]
fn test_unrecoverable_checkpoint_is_recovery_required_not_fresh_resume() {
    // Review finding: corrupt primary + corrupt backup used to return a
    // successful BLANK checkpoint, erasing the distinction between
    // continuation and a new task. The contract is now explicit.
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    // Save a real checkpoint, then corrupt both the primary and the backup.
    let cp = TaskCheckpoint::new("lost-task".to_string(), "real work".to_string());
    manager.save(&cp).unwrap();
    let primary = dir.path().join("lost-task.json");
    let backup = dir.path().join("lost-task.json.bak");
    std::fs::write(&primary, "{ not json !!!").unwrap();
    std::fs::write(&backup, "{ also not json !!!").unwrap();

    match manager.load_with_status("lost-task").unwrap() {
        crate::checkpoint::CheckpointLoad::RecoveryRequired { task_id, .. } => {
            assert_eq!(task_id, "lost-task");
        }
        other => panic!("expected RecoveryRequired, got {other:?}"),
    }
    // load() must refuse rather than report a blank resume.
    let err = manager.load("lost-task").unwrap_err().to_string();
    assert!(
        err.contains("unrecoverable"),
        "load() must name the recovery state: {err}"
    );
    // And no fresh checkpoint may have been saved over the evidence.
    let raw = std::fs::read_to_string(&primary).unwrap();
    assert_eq!(raw, "{ not json !!!");
}

#[test]
fn test_missing_task_is_recovery_required() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    match manager.load_with_status("no-such-task").unwrap() {
        crate::checkpoint::CheckpointLoad::RecoveryRequired { .. } => {}
        other => panic!("expected RecoveryRequired, got {other:?}"),
    }
    assert!(manager.load("no-such-task").is_err());
}

#[test]
fn test_backup_recovery_is_distinct_from_clean() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    let cp = TaskCheckpoint::new("recov-task".to_string(), "real work".to_string());
    // Backup is the PREVIOUS primary — save twice so one exists.
    manager.save(&cp).unwrap();
    manager.save(&cp).unwrap();
    // Corrupt only the primary; the backup must carry the resume.
    let primary = dir.path().join("recov-task.json");
    std::fs::write(&primary, "{ corrupt").unwrap();
    match manager.load_with_status("recov-task").unwrap() {
        crate::checkpoint::CheckpointLoad::RecoveredFromBackup(cp) => {
            assert_eq!(cp.task_description, "real work");
        }
        other => panic!("expected RecoveredFromBackup, got {other:?}"),
    }
}

// ── --autocontinue selection (latest_autoresumable_task) ────────────
//
// Regression tests for the startup auto-resume feature. Selection is safe
// only when BOTH gates hold: the checkpoint belongs to the CURRENT workspace
// (recorded `project_root` equals the workspace passed in) and the task is
// InProgress. Failed/Paused/Completed checkpoints — and checkpoints from
// other workspaces or with no recorded workspace — must never be
// auto-resumed (HIGH finding: a global "newest incomplete" pick resumed
// repo A's task inside repo B).

const WORKSPACE_A: &str = "/work/repo-a";
const WORKSPACE_B: &str = "/work/repo-b";

/// Fabricate a checkpoint pinned to `project_root` with a deterministic
/// `updated_at`. `TaskCheckpoint::new` records the real test-process cwd as
/// its workspace, so every fabricated checkpoint must override it explicitly.
fn cp_in(
    task_id: &str,
    desc: &str,
    status: TaskStatus,
    project_root: &str,
    rfc3339: &str,
) -> TaskCheckpoint {
    let mut cp = TaskCheckpoint::new(task_id.to_string(), desc.to_string());
    cp.project_root = Some(project_root.to_string());
    // A real task checkpoint always carries the user's instruction; implicit
    // resume refuses entries without one (see implicit_resume_blocker).
    cp.messages = vec![Message::system("sys"), Message::user(desc)];
    // set_status() bumps updated_at to now; override AFTER so the fabricated
    // time ordering is deterministic.
    cp.set_status(status);
    cp.updated_at = chrono::DateTime::parse_from_rfc3339(rfc3339)
        .unwrap()
        .with_timezone(&Utc);
    cp
}

#[test]
fn autocontinue_selects_latest_inprogress_in_matching_workspace() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Newest overall is Completed → skipped. Older InProgress → picked.
    let done = cp_in(
        "done-task",
        "a finished task",
        TaskStatus::Completed,
        WORKSPACE_A,
        "2024-01-03T00:00:00Z",
    );
    manager.save_final(&done).unwrap();
    let running = cp_in(
        "running-task",
        "interrupted long task",
        TaskStatus::InProgress,
        WORKSPACE_A,
        "2024-01-01T00:00:00Z",
    );
    manager.save(&running).unwrap();

    let latest = manager
        .latest_autoresumable_task(WORKSPACE_A)
        .unwrap()
        .expect("an eligible checkpoint must be discovered");
    assert_eq!(latest.task_id, "running-task");
    assert_eq!(latest.task_description, "interrupted long task");
    assert_eq!(latest.status, TaskStatus::InProgress);
    assert_eq!(latest.project_root.as_deref(), Some(WORKSPACE_A));
}

#[test]
fn autocontinue_newer_failed_in_other_repository_never_leaks() {
    // HIGH finding, case 1: a NEWER FAILED checkpoint recorded in repo A must
    // never be auto-resumed by `--autocontinue` run in repo B — and not even
    // by a run in A itself (Failed is excluded from AUTOMATIC resumption).
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Repo A: newest checkpoint is Failed; older one is InProgress.
    let failed_a = cp_in(
        "failed-a",
        "A task that failed",
        TaskStatus::Failed,
        WORKSPACE_A,
        "2024-01-03T00:00:00Z",
    );
    manager.save_final(&failed_a).unwrap();
    let running_a = cp_in(
        "running-a",
        "A task still running",
        TaskStatus::InProgress,
        WORKSPACE_A,
        "2024-01-02T00:00:00Z",
    );
    manager.save(&running_a).unwrap();

    // Running --autocontinue in repo B: nothing of A's may be resumed.
    assert!(
        manager
            .latest_autoresumable_task(WORKSPACE_B)
            .unwrap()
            .is_none(),
        "a FAILED checkpoint from repo A must never be auto-resumed in repo B"
    );

    // Running --autocontinue in repo A: the newer Failed is not eligible
    // either; the older InProgress is what gets resumed.
    let in_a = manager
        .latest_autoresumable_task(WORKSPACE_A)
        .unwrap()
        .expect("repo A's InProgress task is eligible there");
    assert_eq!(in_a.task_id, "running-a");
}

#[test]
fn autocontinue_newer_inprogress_in_other_repository_never_leaks() {
    // HIGH finding, case 2: even a NEWER InProgress checkpoint in repo A must
    // not be auto-resumed while running in repo B — B's own older InProgress
    // wins instead.
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let running_b = cp_in(
        "running-b",
        "B task still running",
        TaskStatus::InProgress,
        WORKSPACE_B,
        "2024-01-01T00:00:00Z",
    );
    manager.save(&running_b).unwrap();
    let running_a_newer = cp_in(
        "running-a",
        "A task still running",
        TaskStatus::InProgress,
        WORKSPACE_A,
        "2024-01-03T00:00:00Z",
    );
    manager.save(&running_a_newer).unwrap();

    let latest = manager
        .latest_autoresumable_task(WORKSPACE_B)
        .unwrap()
        .expect("B's own InProgress task is eligible");
    assert_eq!(
        latest.task_id, "running-b",
        "repo A's newer InProgress checkpoint must not leak into repo B"
    );
}

#[test]
fn autocontinue_failed_and_paused_in_current_workspace_not_autoresumed() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Same workspace: newest Failed, mid Paused, oldest InProgress. Only the
    // InProgress task may be auto-resumed; Failed/Paused need explicit resume.
    let failed = cp_in(
        "failed",
        "failed task",
        TaskStatus::Failed,
        WORKSPACE_A,
        "2024-01-03T00:00:00Z",
    );
    manager.save_final(&failed).unwrap();
    let paused = cp_in(
        "paused",
        "paused task",
        TaskStatus::Paused,
        WORKSPACE_A,
        "2024-01-02T00:00:00Z",
    );
    manager.save(&paused).unwrap();
    let running = cp_in(
        "running",
        "running task",
        TaskStatus::InProgress,
        WORKSPACE_A,
        "2024-01-01T00:00:00Z",
    );
    manager.save(&running).unwrap();

    let latest = manager
        .latest_autoresumable_task(WORKSPACE_A)
        .unwrap()
        .expect("the InProgress task is eligible");
    assert_eq!(latest.task_id, "running");
}

#[test]
fn autocontinue_only_failed_or_paused_returns_none() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let failed = cp_in(
        "failed",
        "failed task",
        TaskStatus::Failed,
        WORKSPACE_A,
        "2024-01-02T00:00:00Z",
    );
    manager.save_final(&failed).unwrap();
    let paused = cp_in(
        "paused",
        "paused task",
        TaskStatus::Paused,
        WORKSPACE_A,
        "2024-01-01T00:00:00Z",
    );
    manager.save(&paused).unwrap();

    assert!(
        manager
            .latest_autoresumable_task(WORKSPACE_A)
            .unwrap()
            .is_none(),
        "Failed and Paused tasks require explicit `resume <id>`, never auto-resume"
    );
}

#[test]
fn autocontinue_skips_legacy_checkpoint_without_workspace_identity() {
    // A checkpoint written before the project_root field existed cannot be
    // validated against the current workspace → never auto-resumed (explicit
    // `resume <id>` remains the only path to it).
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let mut legacy = TaskCheckpoint::new("legacy".to_string(), "pre-feature task".to_string());
    legacy.project_root = None;
    // Carry a real user task so ONLY the missing workspace identity excludes it.
    legacy.messages = vec![Message::user("pre-feature task")];
    legacy.set_status(TaskStatus::InProgress);
    manager.save(&legacy).unwrap();

    assert!(
        manager
            .latest_autoresumable_task(WORKSPACE_A)
            .unwrap()
            .is_none(),
        "legacy checkpoints without a workspace identity must not be auto-resumed"
    );
}

#[test]
fn autocontinue_all_completed_returns_none() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let one = cp_in(
        "finished-a",
        "done",
        TaskStatus::Completed,
        WORKSPACE_A,
        "2024-01-01T00:00:00Z",
    );
    manager.save_final(&one).unwrap();
    let two = cp_in(
        "finished-b",
        "done too",
        TaskStatus::Completed,
        WORKSPACE_A,
        "2024-01-02T00:00:00Z",
    );
    manager.save_final(&two).unwrap();

    assert!(
        manager
            .latest_autoresumable_task(WORKSPACE_A)
            .unwrap()
            .is_none(),
        "completed checkpoints must never be auto-resumed"
    );
}

#[test]
fn autocontinue_empty_or_missing_dir_returns_none() {
    // Missing directory → normal startup, no auto-resume.
    let dir = tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let manager = CheckpointManager::new(path.clone()).unwrap();
    std::fs::remove_dir_all(&path).unwrap();
    assert!(manager
        .latest_autoresumable_task(WORKSPACE_A)
        .unwrap()
        .is_none());

    // Empty directory → same.
    let dir2 = tempdir().unwrap();
    let manager2 = CheckpointManager::new(dir2.path().to_path_buf()).unwrap();
    assert!(manager2
        .latest_autoresumable_task(WORKSPACE_A)
        .unwrap()
        .is_none());
}

#[test]
fn autocontinue_sees_completed_status_carried_by_delta() {
    // A task flipped to Completed via an INCREMENTAL delta save leaves the
    // base .json at InProgress. Selection must hydrate deltas, otherwise a
    // completed task would be auto-resumed after a crash. (The manual
    // --continue/journal paths already hydrate; this keeps the new helper
    // consistent with them.)
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Matching workspace, so ONLY the Completed status can exclude it.
    let mut cp = TaskCheckpoint::new("delta-done".to_string(), "done via delta".to_string());
    cp.project_root = Some(WORKSPACE_A.to_string());
    cp.messages = vec![Message::user("done via delta")];
    manager.save(&cp).unwrap(); // base: InProgress
    cp.set_status(TaskStatus::Completed); // touch() bumps version → delta
    manager.save(&cp).unwrap();

    // Sanity: the base file itself still says InProgress.
    let loaded = manager.load("delta-done").unwrap();
    assert_eq!(
        loaded.status,
        TaskStatus::Completed,
        "hydrated status must be Completed"
    );
    let base_raw = std::fs::read_to_string(dir.path().join("delta-done.json")).unwrap();
    assert!(
        base_raw.contains("in_progress"),
        "base file keeps stale InProgress: {base_raw}"
    );

    assert!(
        manager
            .latest_autoresumable_task(WORKSPACE_A)
            .unwrap()
            .is_none(),
        "delta-completed task must not be auto-resumed"
    );
}

// ── Crash-resilience: torn delta-log tails and the primary/backup policy ──

/// Helpers to build a checkpoint whose delta writes are efficient (the save
/// path only writes deltas when `delta_size + 128 < full_size`).
fn big_message_set(n: usize) -> Vec<Message> {
    (0..n)
        .map(|i| Message::user(format!("message-{i} {}", "x".repeat(120))))
        .collect()
}

#[test]
fn delta_log_with_truncated_final_line_still_applies_valid_deltas() {
    // Power loss mid-append_delta leaves a truncated FINAL line in the delta
    // log. All COMPLETE deltas must still apply; the torn tail is skipped
    // with a warning and healed (truncated away) so the log keeps replaying.
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let mut cp = TaskCheckpoint::new("torn-task".to_string(), "d".to_string());
    cp.set_messages(big_message_set(30));
    manager.save(&cp).unwrap(); // full write: no delta log yet

    cp.set_iteration(5);
    manager.save(&cp).unwrap(); // VALID delta (line 1)
    cp.set_step(2);
    manager.save(&cp).unwrap(); // delta (line 2) — will be torn

    let delta_path = manager.checkpoint_delta_path("torn-task").unwrap();
    let content = std::fs::read_to_string(&delta_path).unwrap();
    assert!(content.ends_with('\n'), "sanity: log is newline-terminated");
    // Simulate the crash: drop the trailing newline and chop the final
    // record mid-write, leaving a partial line with NO trailing newline.
    let no_final_nl = content.len() - 1;
    let truncated = content[..no_final_nl.saturating_sub(12)].to_string();
    std::fs::write(&delta_path, &truncated).unwrap();

    // All VALID deltas still apply (iteration came from line 1); the torn
    // line held the step change and is not committed.
    let loaded = manager.load("torn-task").unwrap();
    assert_eq!(loaded.messages.len(), 30, "base state must survive");
    assert_eq!(
        loaded.current_iteration, 5,
        "the valid delta that was committed BEFORE the torn write must still apply"
    );
    assert_eq!(
        loaded.current_step, 0,
        "the torn write's state is not committed"
    );

    // The log healed itself: a further save appends and replays cleanly —
    // had the torn record been left in the middle, the load would FAIL.
    cp.set_step(3);
    manager.save(&cp).unwrap();
    let loaded2 = manager.load("torn-task").unwrap();
    assert_eq!(loaded2.current_step, 3);
    assert_eq!(loaded2.current_iteration, 5);
}

#[test]
fn broken_delta_log_never_triggers_backup_overwrite() {
    // Genuine delta-log corruption (a COMPLETE line that fails to parse — not
    // a torn tail) must fail the load WITHOUT touching the healthy primary:
    // no recovery, no .bak restore, no older-backup-over-newer-primary.
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let mut cp = TaskCheckpoint::new("delta-broken".to_string(), "d".to_string());
    cp.set_messages(big_message_set(30));
    manager.save(&cp).unwrap();
    cp.set_status(TaskStatus::Paused);
    manager.save(&cp).unwrap(); // delta logged

    let delta_path = manager.checkpoint_delta_path("delta-broken").unwrap();
    let raw = std::fs::read_to_string(&delta_path).unwrap();
    let mut lines: Vec<&str> = raw.lines().collect();
    lines[0] = "{{{ not a delta";
    std::fs::write(&delta_path, format!("{}\n", lines.join("\n"))).unwrap();

    // The delta failure surfaces honestly…
    let err = manager.load("delta-broken").unwrap_err().to_string();
    assert!(
        err.contains("delta"),
        "the failure must be reported as a delta-log failure, got: {err}"
    );

    // …but the healthy primary is preserved byte-for-byte and no backup was
    // written over it (the old cascade would have recovered from .bak and
    // rolled the primary back to an older state, discarding valid deltas).
    let primary_raw = std::fs::read_to_string(dir.path().join("delta-broken.json")).unwrap();
    assert!(
        primary_raw.contains("in_progress"),
        "primary must keep its own (pre-delta) state, got: {primary_raw}"
    );
    assert!(
        !dir.path().join("delta-broken.json.bak").exists(),
        "a broken delta log must not cause a backup to be consulted or written"
    );
}

#[test]
fn recover_from_corruption_never_prefers_backup_over_healthy_primary() {
    // The explicit preference (Rule 2): recovery consults the .bak backup
    // ONLY when the primary itself is unreadable. A parseable primary wins
    // over any backup, however old or new the backup is.
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let mut cp = TaskCheckpoint::new("fresh-primary".to_string(), "state-one".to_string());
    manager.save_final(&cp).unwrap(); // full write → creates .bak next round
    cp.task_description = "state-two newest".to_string();
    manager.save_final(&cp).unwrap(); // primary = newest, .bak = older

    assert!(dir.path().join("fresh-primary.json.bak").exists());
    let recovered = manager
        .recover_from_corruption("fresh-primary")
        .unwrap()
        .expect("a healthy primary must be returned, not None");
    assert_eq!(
        recovered.task_description, "state-two newest",
        "the healthy NEWER primary must win over the older backup"
    );
    // The primary on disk was not replaced by backup content.
    let primary_raw = std::fs::read_to_string(dir.path().join("fresh-primary.json")).unwrap();
    assert!(primary_raw.contains("state-two newest"));
}

// ── Atomic replace fallback (Windows rename semantics) ──────────────

#[test]
fn atomic_replace_overwrites_existing_destination_directly() {
    // Unix path: rename over an existing destination succeeds first try —
    // the fallback must not be (and is not) triggered.
    let dir = tempdir().unwrap();
    let tmp = dir.path().join("src.tmp");
    let dest = dir.path().join("dest.json");
    std::fs::write(&tmp, "new bytes").unwrap();
    std::fs::write(&dest, "old bytes").unwrap();

    replace_atomically(&tmp, &dest).unwrap();
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "new bytes");
    assert!(!tmp.exists(), "tmp must be consumed by the rename");
}

#[test]
fn atomic_replace_retries_after_removing_existing_destination() {
    // Windows rename semantics: rename fails while the destination exists.
    // replace_atomically must remove the destination and retry — asserted
    // via the injectable rename seam (no Windows machine needed).
    let dir = tempdir().unwrap();
    let tmp = dir.path().join("src.tmp");
    let dest = dir.path().join("dest.json");
    std::fs::write(&tmp, "new bytes").unwrap();
    std::fs::write(&dest, "old bytes").unwrap();

    let calls = std::cell::Cell::new(0u32);
    let result = replace_atomically_with(&tmp, &dest, |src, dst| {
        calls.set(calls.get() + 1);
        if calls.get() == 1 {
            Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "destination exists (Windows rename semantics)",
            ))
        } else {
            std::fs::rename(src, dst)
        }
    });
    result.unwrap_or_else(|e| panic!("the retry after removing the destination must succeed: {e}"));
    assert_eq!(calls.get(), 2, "first attempt fails, second succeeds");
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "new bytes");
    assert!(
        !tmp.exists(),
        "tmp must be consumed by the successful retry"
    );
}

#[test]
fn atomic_replace_gives_up_when_retry_also_fails_and_cleans_tmp() {
    let dir = tempdir().unwrap();
    let tmp = dir.path().join("src.tmp");
    let dest = dir.path().join("dest.json");
    std::fs::write(&tmp, "new bytes").unwrap();
    std::fs::write(&dest, "old bytes").unwrap();

    let calls = std::cell::Cell::new(0u32);
    let result = replace_atomically_with(&tmp, &dest, |_, _| {
        calls.set(calls.get() + 1);
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        ))
    });
    assert!(result.is_err());
    assert_eq!(calls.get(), 2, "exactly one remove-then-retry attempt");
    assert!(
        !tmp.exists(),
        "tmp must be cleaned up after a double failure"
    );
    assert!(
        !dest.exists(),
        "the remove-then-retry convention removes the destination for the retry; \
         after a failed retry NO partial file may be left behind"
    );
}

// ── Advisory file locking (finding: concurrent writers clobber state) ──

#[cfg(unix)]
#[test]
fn file_lock_serializes_writers() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("serial.json");

    // Writer A holds the lock.
    let a = FileLock::acquire(&target).unwrap();

    // Writer B — a separate thread, so a real flock (the reentrancy
    // registry is thread-local) — must be EXCLUDED while A holds it.
    let t2 = target.clone();
    let b = std::thread::spawn(move || FileLock::try_acquire(&t2).is_err());
    assert!(
        b.join().unwrap(),
        "a second writer must be excluded while the lock is held"
    );

    drop(a);

    // After release, the same writer acquires fine.
    let t3 = target;
    let c = std::thread::spawn(move || FileLock::try_acquire(&t3).is_ok());
    assert!(c.join().unwrap(), "a writer must acquire after release");
}

#[cfg(unix)]
#[test]
fn file_lock_is_reentrant_within_a_thread() {
    // Nested acquires from the same thread (load → recovery → save) must be
    // no-ops, not deadlocks.
    let dir = tempdir().unwrap();
    let target = dir.path().join("reentrant.json");
    let outer = FileLock::acquire(&target).unwrap();
    let inner = FileLock::acquire(&target).unwrap();
    drop(inner);
    drop(outer);

    // And the lock is genuinely free again for other threads afterwards.
    let t = std::thread::spawn(move || FileLock::try_acquire(&target).is_ok());
    assert!(t.join().unwrap());
}

#[cfg(unix)]
#[test]
fn checkpoint_save_holds_advisory_lock_during_writes() {
    use std::time::Duration;

    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    manager
        .save(&TaskCheckpoint::new(
            "locked-task".to_string(),
            "d".to_string(),
        ))
        .unwrap();

    // A foreign writer pins the task's advisory lock file.
    let lock_target = manager.checkpoint_path("locked-task").unwrap();
    let _foreign = FileLock::acquire(&lock_target).unwrap();

    // A save from another thread must BLOCK until the lock is released.
    let (tx, rx) = std::sync::mpsc::channel();
    let dir2 = dir.path().to_path_buf();
    let handle = std::thread::spawn(move || {
        let m = CheckpointManager::new(dir2).unwrap();
        let mut cp2 = TaskCheckpoint::new("locked-task".to_string(), "d".to_string());
        cp2.set_step(9);
        m.save(&cp2).unwrap();
        tx.send(()).unwrap();
    });

    assert!(
        rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "save must block while the advisory lock is held by another writer"
    );
    drop(_foreign);
    rx.recv_timeout(Duration::from_secs(5))
        .expect("save must complete once the lock is released");
    handle.join().unwrap();

    let loaded = manager.load("locked-task").unwrap();
    assert_eq!(
        loaded.current_step, 9,
        "the blocked writer's state must land"
    );
}

// ── --autocontinue: iteration-cap-failed checkpoints may chain ─────────────
//
// 2026-09-22 long-horizon finding: a productive run that died at the
// iteration cap was persisted as Failed with the "Max iterations exceeded"
// stop reason, and --autocontinue refused to pick it up (Failed was excluded
// wholesale), so the chain could only be continued by a manual
// `selfware resume <id>`. The policy now chains a Failed checkpoint ONLY
// when its terminal stop is the iteration/step-cap family; crashes, safety
// stops, guard aborts and the typed AUTO_CONTINUE_LIMIT stop stay
// explicit-resume-only.

/// Fabricate a Failed checkpoint in `project_root` whose terminal stop is
/// `reason` (logged unrecovered, as `fail_checkpoint` does).
fn failed_with_stop_reason(
    task_id: &str,
    project_root: &str,
    rfc3339: &str,
    reason: &str,
) -> TaskCheckpoint {
    let mut cp = cp_in(
        task_id,
        "task that hit a stop",
        TaskStatus::Failed,
        project_root,
        rfc3339,
    );
    cp.log_error(12, reason.to_string(), false);
    cp
}

#[test]
fn autocontinue_chains_iteration_cap_failed_checkpoint() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let capped = failed_with_stop_reason(
        "capped-task",
        WORKSPACE_A,
        "2024-01-03T00:00:00Z",
        crate::agent::loop_control::MAX_ITERATIONS_STOP_REASON,
    );
    manager.save_final(&capped).unwrap();

    let latest = manager
        .latest_autoresumable_task(WORKSPACE_A)
        .unwrap()
        .expect("an iteration-cap stop is exactly what --autocontinue exists to chain");
    assert_eq!(latest.task_id, "capped-task");
}

#[test]
fn autocontinue_never_chains_arbitrary_failures() {
    // Every non-cap terminal stop stays explicit-`resume`-only: a safety
    // stop, a guard abort, a provider failure, and the typed
    // AUTO_CONTINUE_LIMIT (its per-task chain budget is already spent —
    // re-chaining it every startup would defeat MAX_AUTO_CONTINUES).
    for (task_id, reason) in [
        ("safety-stop", "killswitch engaged: writes to /etc are forbidden"),
        (
            "guard-abort",
            "WORKSPACE_STAGNATION: 20 consecutive tool calls with no workspace change",
        ),
        ("provider-error", "HTTP 401 Unauthorized: invalid API key"),
        (
            "chain-exhausted",
            "AUTO_CONTINUE_LIMIT: automatic continuation chained 3 times on this task and the iteration cap was reached again",
        ),
    ] {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
        let failed = failed_with_stop_reason(task_id, WORKSPACE_A, "2024-01-03T00:00:00Z", reason);
        manager.save_final(&failed).unwrap();
        assert!(
            manager
                .latest_autoresumable_task(WORKSPACE_A)
                .unwrap()
                .is_none(),
            "'{reason}' must not auto-chain"
        );
    }
}

#[test]
fn autocontinue_cap_failure_still_scoped_to_its_workspace() {
    // The workspace gate is unchanged by the policy extension: a cap-failed
    // checkpoint from repo A is never chained by a startup in repo B.
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let capped_a = failed_with_stop_reason(
        "capped-a",
        WORKSPACE_A,
        "2024-01-03T00:00:00Z",
        crate::agent::loop_control::MAX_ITERATIONS_STOP_REASON,
    );
    manager.save_final(&capped_a).unwrap();

    assert!(
        manager
            .latest_autoresumable_task(WORKSPACE_B)
            .unwrap()
            .is_none(),
        "repo A's cap-failed checkpoint must never chain into repo B"
    );
}

#[test]
fn autocontinue_picks_newest_eligible_across_status_classes() {
    // A newer cap-failed checkpoint is eligible over an older InProgress
    // one: "newest eligible by updated_at" is the standing rule.
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let running = cp_in(
        "running-old",
        "older interrupted task",
        TaskStatus::InProgress,
        WORKSPACE_A,
        "2024-01-01T00:00:00Z",
    );
    manager.save(&running).unwrap();
    let capped = failed_with_stop_reason(
        "capped-new",
        WORKSPACE_A,
        "2024-01-02T00:00:00Z",
        crate::agent::loop_control::MAX_ITERATIONS_STOP_REASON,
    );
    manager.save_final(&capped).unwrap();

    let latest = manager
        .latest_autoresumable_task(WORKSPACE_A)
        .unwrap()
        .expect("both candidates are eligible");
    assert_eq!(latest.task_id, "capped-new", "newest eligible wins");
}

#[test]
fn autocontinue_ignores_recovered_cap_errors() {
    // A cap trip that was RECOVERED mid-run (the adaptive grant rescued it)
    // is not the terminal stop; a later arbitrary failure is.
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let mut cp = cp_in(
        "recovered-then-crashed",
        "cap recovered, then failed for real",
        TaskStatus::Failed,
        WORKSPACE_A,
        "2024-01-03T00:00:00Z",
    );
    // Earlier cap trip was recovered (grant fired); the later failure is terminal.
    cp.log_error(
        10,
        crate::agent::loop_control::MAX_ITERATIONS_STOP_REASON.to_string(),
        true,
    );
    cp.log_error(14, "HTTP 500 from provider".to_string(), false);
    manager.save_final(&cp).unwrap();

    assert!(
        manager
            .latest_autoresumable_task(WORKSPACE_A)
            .unwrap()
            .is_none(),
        "the TERMINAL stop is the provider failure, not the recovered cap trip"
    );
}

// ── Adaptive-budget + chain-iteration fields survive save/load ─────────────

#[test]
fn checkpoint_persists_adaptive_budget_and_chain_iterations() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let mut cp = TaskCheckpoint::new("budget-fields".to_string(), "d".to_string());
    cp.effective_max_iterations = Some(24);
    cp.extensions_granted = 4;
    cp.cumulative_iterations = 42;
    manager.save(&cp).unwrap();

    let loaded = manager.load("budget-fields").unwrap();
    assert_eq!(loaded.effective_max_iterations, Some(24));
    assert_eq!(loaded.extensions_granted, 4);
    assert_eq!(loaded.cumulative_iterations, 42);
}

#[test]
fn legacy_checkpoint_without_budget_fields_defaults_cleanly() {
    // Checkpoints written before the fields existed deserialize with no
    // earned extension: resume then keeps the configured cap, unchanged.
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    let cp = TaskCheckpoint::new("legacy".to_string(), "d".to_string());
    manager.save(&cp).unwrap();

    let loaded = manager.load("legacy").unwrap();
    assert_eq!(loaded.effective_max_iterations, None);
    assert_eq!(loaded.extensions_granted, 0);
    assert_eq!(loaded.cumulative_iterations, 0);
}

#[test]
fn delta_round_trip_carries_adaptive_budget_and_chain_iterations() {
    let mut base = TaskCheckpoint::new("delta-budget".to_string(), "d".to_string());
    base.effective_max_iterations = Some(12);
    base.extensions_granted = 0;
    base.cumulative_iterations = 4;

    // A version bump alone must NOT carry the budget fields (unchanged).
    let mut step_only = base.clone();
    step_only.set_step(1);
    let delta = step_only
        .compute_delta(&base)
        .expect("the step change produces a delta");
    assert_eq!(delta.effective_max_iterations, None);
    assert_eq!(delta.extensions_granted, None);
    assert_eq!(delta.cumulative_iterations, None);

    // A grant firing between saves MUST ride the delta, or the resumed run
    // would rebuild at the configured cap and drop the earned extension.
    let mut updated = base.clone();
    updated.set_step(2);
    updated.effective_max_iterations = Some(15);
    updated.extensions_granted = 1;
    updated.cumulative_iterations = 7;
    let delta = updated
        .compute_delta(&base)
        .expect("budget fields changed, so a delta exists");
    assert_eq!(delta.effective_max_iterations, Some(15));
    assert_eq!(delta.extensions_granted, Some(1));
    assert_eq!(delta.cumulative_iterations, Some(7));

    let mut hydrated = base.clone();
    hydrated.apply_delta(&delta).unwrap();
    assert_eq!(hydrated.effective_max_iterations, Some(15));
    assert_eq!(hydrated.extensions_granted, 1);
    assert_eq!(hydrated.cumulative_iterations, 7);
}

#[test]
fn checkpoint_manager_delta_log_carries_adaptive_budget_fields() {
    // End-to-end through the on-disk delta log: a mid-run grant between two
    // incremental saves must be visible after hydration (the base file keeps
    // the stale configured cap).
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let mut cp = TaskCheckpoint::new("delta-mgr-budget".to_string(), "d".to_string());
    cp.set_messages(big_message_set(30));
    cp.effective_max_iterations = Some(12);
    manager.save(&cp).unwrap();

    cp.set_step(2);
    cp.effective_max_iterations = Some(15);
    cp.extensions_granted = 1;
    cp.cumulative_iterations = 7;
    manager.save(&cp).unwrap();

    let delta_path = manager.checkpoint_delta_path("delta-mgr-budget").unwrap();
    assert!(
        delta_path.exists(),
        "expected a delta log for the second save"
    );

    let loaded = manager.load("delta-mgr-budget").unwrap();
    assert_eq!(loaded.effective_max_iterations, Some(15));
    assert_eq!(loaded.extensions_granted, 1);
    assert_eq!(loaded.cumulative_iterations, 7);
}

#[test]
fn test_in_place_message_change_forces_full_save() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let mut cp = TaskCheckpoint::new("inplace-test".to_string(), "desc".to_string());
    let mut msgs = big_message_set(10);
    cp.set_messages(msgs.clone());
    manager.save(&cp).unwrap();

    let base = manager.load("inplace-test").unwrap();

    // Modify an existing message in-place AND append new messages
    msgs[2] = Message::user("modified content in place");
    msgs.extend(big_message_set(5));
    cp.set_messages(msgs);
    cp.version += 1;

    // compute_delta must return None because prefix changed
    assert!(
        cp.compute_delta(&base).is_none(),
        "in-place message modification must prevent delta generation and force full save"
    );

    // Save should perform full save without losing the in-place change
    manager.save(&cp).unwrap();

    let loaded = manager.load("inplace-test").unwrap();
    assert_eq!(loaded.messages.len(), 15);
    assert_eq!(
        loaded.messages[2],
        Message::user("modified content in place")
    );
}

#[test]
fn test_crashed_compaction_recovers_and_cleans_obsolete_deltas() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let mut cp = TaskCheckpoint::new("crash-compact".to_string(), "desc".to_string());
    cp.set_messages(big_message_set(30));
    manager.save(&cp).unwrap();

    // Multiple incremental saves that write deltas to .delta.jsonl
    for i in 1..=4 {
        cp.set_step(i);
        manager.save(&cp).unwrap();
    }

    let delta_path = manager.checkpoint_delta_path("crash-compact").unwrap();
    assert!(
        delta_path.exists(),
        "delta log exists from incremental saves"
    );

    // Verify normal delta loading works
    let loaded = manager.load("crash-compact").unwrap();
    assert_eq!(loaded.current_step, 4);

    // Now simulate a crash during compaction:
    // Compaction wrote the full base checkpoint at the latest state (e.g. step 10),
    // but the process crashed before clear_delta_log could delete the old delta log.
    cp.set_step(10);
    manager.save_full_checkpoint(&cp).unwrap();

    assert!(
        delta_path.exists(),
        "delta log still present after crash before clear_delta_log"
    );

    // Loading must skip obsolete deltas without error and clean the obsolete delta log
    let loaded = manager.load("crash-compact").unwrap();
    assert_eq!(loaded.version, cp.version);
    assert_eq!(loaded.current_step, 10);
    assert!(
        !delta_path.exists(),
        "obsolete delta log should be cleaned after successful recovery"
    );
}

// ── --continue selection (latest_continuable_task) ───────────────────
//
// Regression: `--continue` resumed `list_tasks().first()` unfiltered, so a
// session-exit placeholder ("interactive session exit", from earlier builds)
// or a step-0 entry with no user message was resumed and ran an
// instruction-less agent turn that mutated files.

#[test]
fn placeholder_descriptions_are_recognized() {
    for d in [
        "",
        "   ",
        "interactive session exit",
        "interactive basic session exit",
        "TUI session exit",
        "interactive session",
    ] {
        assert!(is_placeholder_task_description(d), "{d:?} is a placeholder");
    }
    for d in ["fix the build", "interactive session exit handling bug"] {
        assert!(!is_placeholder_task_description(d), "{d:?} is a real task");
    }
}

#[test]
fn continue_skips_placeholder_and_userless_entries_and_reports_them() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Oldest: a real task → the one --continue must pick.
    let real = cp_in(
        "real-task",
        "refactor the parser",
        TaskStatus::InProgress,
        WORKSPACE_A,
        "2024-01-01T00:00:00Z",
    );
    manager.save_final(&real).unwrap();

    // Newer: step-0 save with a real-looking description but no user message.
    let mut step0 = cp_in(
        "step0-task",
        "something",
        TaskStatus::InProgress,
        WORKSPACE_A,
        "2024-01-02T00:00:00Z",
    );
    step0.messages = vec![Message::system("sys")];
    manager.save_final(&step0).unwrap();

    // Newest: legacy session-exit placeholder.
    let mut exit = cp_in(
        "exit-task",
        "interactive session exit",
        TaskStatus::InProgress,
        WORKSPACE_A,
        "2024-01-03T00:00:00Z",
    );
    exit.messages = vec![Message::system("sys")];
    manager.save_final(&exit).unwrap();

    let selection = manager.latest_continuable_task().unwrap();
    assert_eq!(
        selection.selected.as_ref().map(|s| s.task_id.as_str()),
        Some("real-task")
    );
    let skipped: Vec<(&str, NotResumableReason)> = selection
        .skipped
        .iter()
        .map(|(s, r)| (s.task_id.as_str(), *r))
        .collect();
    assert_eq!(
        skipped,
        vec![
            ("exit-task", NotResumableReason::PlaceholderDescription),
            ("step0-task", NotResumableReason::NoUserMessage),
        ]
    );
}

#[test]
fn continue_with_only_placeholders_selects_nothing() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    let mut exit = cp_in(
        "exit-only",
        "TUI session exit",
        TaskStatus::InProgress,
        WORKSPACE_A,
        "2024-01-03T00:00:00Z",
    );
    exit.messages.clear();
    manager.save_final(&exit).unwrap();

    let selection = manager.latest_continuable_task().unwrap();
    assert!(selection.selected.is_none());
    assert_eq!(selection.skipped.len(), 1);
}

#[test]
fn autocontinue_skips_placeholder_and_userless_checkpoints() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let mut exit = cp_in(
        "exit-task",
        "interactive basic session exit",
        TaskStatus::InProgress,
        WORKSPACE_A,
        "2024-01-03T00:00:00Z",
    );
    exit.messages = vec![Message::system("sys")];
    manager.save_final(&exit).unwrap();
    let mut step0 = cp_in(
        "step0-task",
        "real description",
        TaskStatus::InProgress,
        WORKSPACE_A,
        "2024-01-02T00:00:00Z",
    );
    step0.messages = vec![Message::system("sys")];
    manager.save_final(&step0).unwrap();

    assert!(
        manager
            .latest_autoresumable_task(WORKSPACE_A)
            .unwrap()
            .is_none(),
        "entries without a user task must never be auto-resumed"
    );

    let real = cp_in(
        "real-task",
        "finish the migration",
        TaskStatus::InProgress,
        WORKSPACE_A,
        "2024-01-01T00:00:00Z",
    );
    manager.save_final(&real).unwrap();
    let picked = manager
        .latest_autoresumable_task(WORKSPACE_A)
        .unwrap()
        .expect("the real task behind the placeholders must be found");
    assert_eq!(picked.task_id, "real-task");
}

// ---- W8a: checkpoint-on-every-mutation (cached incremental append) ----

/// A realistically sized in-flight checkpoint: `messages` conversation turns
/// of ~2 KB each plus `calls` logged tool calls.
fn w8a_large_checkpoint(task_id: &str, messages: usize, calls: usize) -> TaskCheckpoint {
    let mut cp = TaskCheckpoint::new(task_id.to_string(), "W8a cost probe".to_string());
    let body = "x".repeat(2_000);
    cp.set_messages(
        (0..messages)
            .map(|i| Message::user(format!("turn {i}: {body}")))
            .collect(),
    );
    for i in 0..calls {
        cp.log_tool_call(w8a_write_call(i));
    }
    cp
}

fn w8a_write_call(i: usize) -> ToolCallLog {
    ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_write".to_string(),
        arguments: serde_json::json!({"path": format!("src/f{i}.rs"), "content": "fn main() {}"})
            .to_string(),
        result: Some("{\"success\":true}".to_string()),
        success: true,
        duration_ms: Some(3),
    }
}

/// The cached append path must persist exactly what the full load-and-replay
/// path would: across many saves (crossing the 24-delta compaction), a fresh
/// manager loading from disk sees the final in-memory state.
#[test]
fn w8a_cached_incremental_saves_roundtrip_through_compaction() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    let mut cp = w8a_large_checkpoint("w8a-roundtrip", 20, 5);
    manager.save(&cp).unwrap();
    for i in 0..40 {
        cp.log_tool_call(w8a_write_call(100 + i));
        manager.save(&cp).unwrap();
        let loaded = CheckpointManager::new(dir.path().to_path_buf())
            .unwrap()
            .load("w8a-roundtrip")
            .unwrap();
        assert_eq!(loaded.tool_calls.len(), cp.tool_calls.len(), "save {i}");
        assert_eq!(loaded.version, cp.version, "save {i}");
        assert_eq!(loaded.messages.len(), cp.messages.len(), "save {i}");
    }
}

/// The cache only skips work it can prove redundant: a write by ANOTHER
/// manager (another process) changes the on-disk stamp, so the next save
/// takes the full load path and still lands the caller's state intact.
#[test]
fn w8a_foreign_write_invalidates_the_cached_base() {
    let dir = tempdir().unwrap();
    let a = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    let b = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    let mut cp = w8a_large_checkpoint("w8a-foreign", 10, 2);
    a.save(&cp).unwrap();

    // Another writer appends its own (divergent) state.
    let mut foreign = cp.clone();
    foreign.log_tool_call(w8a_write_call(900));
    b.save(&foreign).unwrap();

    // A's next save must not compute its delta against its stale memory.
    cp.log_tool_call(w8a_write_call(1));
    cp.log_tool_call(w8a_write_call(2));
    a.save(&cp).unwrap();
    let loaded = CheckpointManager::new(dir.path().to_path_buf())
        .unwrap()
        .load("w8a-foreign")
        .unwrap();
    assert_eq!(loaded.tool_calls.len(), cp.tool_calls.len());
    assert_eq!(loaded.version, cp.version);
    assert_eq!(
        loaded.tool_calls.last().unwrap().arguments,
        cp.tool_calls.last().unwrap().arguments
    );
}

/// Cost measurement for the per-mutation cadence (W8a). Reports the mean
/// wall time of one incremental save on a ~200 KB checkpoint, for the cached
/// path (this manager wrote last) and the uncached path (a fresh manager
/// must re-read, verify and replay — the pre-W8a cost). Run with
/// `--nocapture` to see the numbers. The hard assertions are correctness
/// plus a generous ceiling that catches a regression to full rewrites.
#[test]
fn w8a_incremental_save_cost_is_measured() {
    const SAVES: usize = 20;
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    let mut cp = w8a_large_checkpoint("w8a-cost", 100, 60);
    manager.save(&cp).unwrap();
    let base_bytes = std::fs::metadata(manager.checkpoint_path("w8a-cost").unwrap())
        .unwrap()
        .len();

    let started = std::time::Instant::now();
    for i in 0..SAVES {
        cp.log_tool_call(w8a_write_call(1_000 + i));
        manager.save(&cp).unwrap();
    }
    let cached = started.elapsed() / SAVES as u32;

    let started = std::time::Instant::now();
    for i in 0..SAVES {
        cp.log_tool_call(w8a_write_call(2_000 + i));
        CheckpointManager::new(dir.path().to_path_buf())
            .unwrap()
            .save(&cp)
            .unwrap();
    }
    let uncached = started.elapsed() / SAVES as u32;

    eprintln!(
        "W8a checkpoint cost: base file {base_bytes} bytes; cached delta append {cached:?}/save, \
         uncached load+replay+append {uncached:?}/save ({SAVES} saves each)"
    );
    let loaded = manager.load("w8a-cost").unwrap();
    assert_eq!(loaded.tool_calls.len(), cp.tool_calls.len());
    assert!(
        cached < std::time::Duration::from_millis(250),
        "an incremental save must stay cheap, measured {cached:?}"
    );
}

// The task's baseline HEAD (completion-gate commit attribution) survives a
// save/resume round trip, is absent on legacy checkpoints, and is never
// silently dropped by an incremental (delta) save.
#[test]
fn task_start_head_persists_and_defaults_to_none_on_legacy_checkpoints() {
    let mut checkpoint = TaskCheckpoint::new("t_head".to_string(), "task".to_string());
    assert_eq!(checkpoint.task_start_head, None);
    checkpoint.task_start_head = Some("0123456789abcdef0123456789abcdef01234567".to_string());

    let json = serde_json::to_value(&checkpoint).unwrap();
    let restored: TaskCheckpoint = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(restored.task_start_head, checkpoint.task_start_head);

    let mut legacy = json;
    legacy.as_object_mut().unwrap().remove("task_start_head");
    let legacy: TaskCheckpoint = serde_json::from_value(legacy).unwrap();
    assert_eq!(legacy.task_start_head, None, "legacy: no baseline known");

    let base = TaskCheckpoint::new("t_head".to_string(), "task".to_string());
    let mut next = base.clone();
    next.set_iteration(1);
    assert!(
        next.compute_delta(&base).is_some(),
        "control: the same change without a baseline edit is a delta"
    );
    next.task_start_head = Some("abc1234".to_string());
    assert!(
        next.compute_delta(&base).is_none(),
        "a baseline change forces a full save instead of a lossy delta"
    );
}

// ---------------------------------------------------------------------------
// Delta coverage of the fields the full save writes (tightened budget caps,
// auto-continue chain count, cleared pending visual assertion).
// ---------------------------------------------------------------------------

/// A base checkpoint big enough that a small change is always written as a
/// delta (the manager falls back to a full write when the delta is not
/// meaningfully smaller than the checkpoint).
fn sizeable_checkpoint(task_id: &str) -> TaskCheckpoint {
    let mut cp = TaskCheckpoint::new(task_id.to_string(), "budgeted task".to_string());
    cp.set_messages(
        (0..40)
            .map(|i| Message::user(format!("prior message {i}: {}", "x".repeat(200))))
            .collect(),
    );
    cp
}

fn delta_log_len(dir: &std::path::Path, task_id: &str) -> usize {
    std::fs::read_to_string(dir.join(format!("{task_id}.delta.jsonl")))
        .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0)
}

#[test]
fn tightened_budget_caps_survive_delta_only_saves() {
    let dir = tempdir().unwrap();
    let task = "caps-task";

    // Segment 1: full save with the original, higher caps.
    {
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
        let mut cp = sizeable_checkpoint(task);
        cp.max_budget_tokens = Some(100_000);
        cp.max_wall_secs = Some(3_600);
        cp.max_cost_usd = Some(10.0);
        cp.auto_continue_count = 1;
        manager.save_final(&cp).unwrap();
    }

    // Segment 2 (a new process): resume with LOWER caps, make progress, and
    // persist only incrementally before being interrupted.
    {
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
        let mut cp = manager.load(task).unwrap();
        cp.max_budget_tokens = Some(20_000);
        cp.max_wall_secs = Some(600);
        cp.max_cost_usd = Some(1.5);
        cp.auto_continue_count = 2;
        cp.set_step(cp.current_step + 1);
        manager.save(&cp).unwrap();
        assert_eq!(
            delta_log_len(dir.path(), task),
            1,
            "the cap change must be persisted as a delta for this test to exercise the delta path"
        );
        // A second, cap-neutral delta on top (cached fast path).
        cp.set_step(cp.current_step + 1);
        manager.save(&cp).unwrap();
        assert_eq!(delta_log_len(dir.path(), task), 2);
    }

    // Segment 3: the next resume must see the tightened caps, not the base
    // file's older, higher ones.
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    let loaded = manager.load(task).unwrap();
    assert_eq!(loaded.max_budget_tokens, Some(20_000));
    assert_eq!(loaded.max_wall_secs, Some(600));
    assert_eq!(loaded.max_cost_usd, Some(1.5));
    assert_eq!(
        loaded.auto_continue_count, 2,
        "the auto-continue chain count must survive delta-only saves too"
    );
}

#[test]
fn removing_a_budget_cap_forces_a_full_write() {
    let dir = tempdir().unwrap();
    let task = "cap-removed";
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    let mut cp = sizeable_checkpoint(task);
    cp.max_cost_usd = Some(5.0);
    manager.save_final(&cp).unwrap();

    let mut next = cp.clone();
    next.max_cost_usd = None;
    next.set_step(1);
    assert!(
        next.compute_delta(&cp).is_none(),
        "the delta cannot encode Some -> None for a cap; it must force a full write"
    );
    manager.save(&next).unwrap();
    assert_eq!(delta_log_len(dir.path(), task), 0);
    let loaded = CheckpointManager::new(dir.path().to_path_buf())
        .unwrap()
        .load(task)
        .unwrap();
    assert_eq!(loaded.max_cost_usd, None);
}

#[test]
fn clearing_pending_visual_assertion_is_not_resurrected_by_the_delta_log() {
    let dir = tempdir().unwrap();
    let task = "pending-clear";
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    let mut cp = sizeable_checkpoint(task);
    cp.set_pending_visual_assertion(VisualAssertion {
        id: "a1".to_string(),
        description: "window shows OK".to_string(),
        screenshot_path: None,
        verified: false,
        verification_result: None,
        created_at: Utc::now(),
        verified_at: None,
        step: None,
        tool_name: None,
        expected: None,
        observed: None,
        passed: None,
        confidence: None,
        screenshot_hash_legacy: None,
        timestamp: None,
    });
    manager.save_final(&cp).unwrap();

    cp.pending_visual_assertion = None;
    cp.set_step(1);
    manager.save(&cp).unwrap();
    let loaded = CheckpointManager::new(dir.path().to_path_buf())
        .unwrap()
        .load(task)
        .unwrap();
    assert!(
        loaded.pending_visual_assertion.is_none(),
        "a cleared pending assertion must stay cleared after a reload"
    );
}

#[test]
fn identity_fields_changing_force_a_full_write() {
    let base = sizeable_checkpoint("ident");
    let mutations: [fn(&mut TaskCheckpoint); 4] = [
        |cp| cp.task_description = "other".to_string(),
        |cp| cp.project_root = Some("/elsewhere".to_string()),
        |cp| cp.created_at += chrono::Duration::seconds(1),
        |cp| cp.task_start_head = Some("abc".to_string()),
    ];
    for mutate in mutations {
        let mut next = base.clone();
        mutate(&mut next);
        next.set_step(1);
        assert!(next.compute_delta(&base).is_none());
    }
}

#[test]
fn workspace_fingerprint_tracks_written_file_contents() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("lib.rs");
    std::fs::write(&file, "pub fn a() {}").unwrap();
    let written = vec![
        "lib.rs".to_string(),
        dir.path().join("gone.rs").to_string_lossy().to_string(),
    ];
    let before = WorkspaceFingerprint::capture(dir.path(), &written);
    assert_eq!(before.files[1].1, WorkspaceFingerprint::ABSENT);
    assert_eq!(before, WorkspaceFingerprint::capture(dir.path(), &written));

    std::fs::write(&file, "pub fn a() { panic!() }").unwrap();
    let after = WorkspaceFingerprint::capture(dir.path(), &written);
    assert_ne!(before, after);
    assert_eq!(
        before.differences(&after),
        vec!["lib.rs changed".to_string()]
    );
}
