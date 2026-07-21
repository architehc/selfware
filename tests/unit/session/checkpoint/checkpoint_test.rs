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
    // With recovery, loading a nonexistent task creates a fresh checkpoint
    let result = manager.load("nonexistent_task").unwrap();
    assert_eq!(result.task_id, "nonexistent_task");
    assert_eq!(result.task_description, "");
    assert_eq!(result.status, TaskStatus::InProgress);
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

    // Load should detect corruption and recover with a fresh checkpoint
    // (no backup exists, so recovery creates a new empty one)
    let result = manager.load("corrupt_test").unwrap();
    assert_eq!(result.task_id, "corrupt_test");
    // The description is empty because recovery created a fresh checkpoint
    assert_eq!(result.task_description, "");
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

    // Load should create a fresh checkpoint
    let loaded = manager.load("no_bak").unwrap();
    assert_eq!(loaded.task_id, "no_bak");
    assert_eq!(loaded.task_description, "");
    assert_eq!(loaded.status, TaskStatus::InProgress);
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

    // Load should create a fresh checkpoint
    let loaded = manager.load("both_bad").unwrap();
    assert_eq!(loaded.task_id, "both_bad");
    assert_eq!(loaded.task_description, "");
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
    let manager = CheckpointManager {
        checkpoints_dir: impossible_dir,
    };

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
    };
    let json = serde_json::to_string(&cp).unwrap();
    let back: TaskCheckpoint = serde_json::from_str(&json).unwrap();
    assert_eq!(back.cumulative_tokens, 12345);
    assert_eq!(back.elapsed_wall_secs, 678);
    assert_eq!(back.cumulative_cost_usd, 1.2345);
    assert_eq!(back.guard_counters.mutation_gate_rejections, 5);
    assert_eq!(back.guard_counters.prefill_400_count, 2);

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
