//! Phase 5: End-to-End Verification Tests
//!
//! Tests the complete system including:
//! - Checkpoint/resume cycles
//! - Verification gates
//! - Cognitive state management
//! - Error analyzer integration
//! - Multi-step coding scenarios

use chrono::Utc;
use selfware::analyzer::{ErrorAnalyzer, ErrorCategory};
use selfware::checkpoint::{
    capture_git_state, CheckpointManager, TaskCheckpoint, TaskStatus, ToolCallLog,
};
use selfware::cognitive::{
    CognitiveState, CognitiveStateBuilder, CyclePhase, EpisodicMemory, WorkingMemory,
};
use selfware::tools::ToolRegistry;
use selfware::verification::{VerificationConfig, VerificationGate};
use std::fs;
use tempfile::tempdir;

// ============================================================================
// CHECKPOINT/RESUME CYCLE TESTS
// ============================================================================

#[test]
fn test_checkpoint_full_cycle() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Create a task checkpoint
    let mut checkpoint = TaskCheckpoint::new(
        "test-task-001".to_string(),
        "Add a new function to the codebase".to_string(),
    );

    // Simulate steps
    checkpoint.set_step(1);
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_read".to_string(),
        arguments: r#"{"path": "src/main.rs"}"#.to_string(),
        result: Some("fn main() {}".to_string()),
        success: true,
        duration_ms: Some(50),
    });

    checkpoint.set_step(2);
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_edit".to_string(),
        arguments: r#"{"path": "src/main.rs", "old_str": "fn main", "new_str": "fn new_main"}"#
            .to_string(),
        result: Some("Success".to_string()),
        success: true,
        duration_ms: Some(100),
    });

    // Save checkpoint
    manager.save(&checkpoint).unwrap();

    // Simulate interruption - load checkpoint
    let loaded = manager.load("test-task-001").unwrap();
    assert_eq!(loaded.current_step, 2);
    assert_eq!(loaded.tool_calls.len(), 2);
    assert_eq!(loaded.status, TaskStatus::InProgress);

    // Continue from checkpoint
    let mut resumed = loaded;
    resumed.set_step(3);
    resumed.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_check".to_string(),
        arguments: "{}".to_string(),
        result: Some("Compiles successfully".to_string()),
        success: true,
        duration_ms: Some(2000),
    });
    resumed.set_status(TaskStatus::Completed);

    manager.save(&resumed).unwrap();

    // Verify final state
    let final_checkpoint = manager.load("test-task-001").unwrap();
    assert_eq!(final_checkpoint.current_step, 3);
    assert_eq!(final_checkpoint.status, TaskStatus::Completed);
    assert_eq!(final_checkpoint.tool_calls.len(), 3);
}

#[test]
fn test_checkpoint_error_recovery() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    let mut checkpoint =
        TaskCheckpoint::new("error-task".to_string(), "Task with errors".to_string());

    // Simulate error at step 2
    checkpoint.set_step(1);
    checkpoint.set_step(2);
    checkpoint.log_error(2, "Compilation failed: missing semicolon".to_string(), true);

    // Continue after recovery
    checkpoint.set_step(3);
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_edit".to_string(),
        arguments: "{}".to_string(),
        result: Some("Fixed".to_string()),
        success: true,
        duration_ms: Some(50),
    });

    manager.save(&checkpoint).unwrap();

    let loaded = manager.load("error-task").unwrap();
    assert_eq!(loaded.errors.len(), 1);
    assert!(loaded.errors[0].recovered);
}

#[test]
fn test_checkpoint_list_and_delete() {
    let dir = tempdir().unwrap();
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

    // Create multiple tasks
    for i in 0..5 {
        let checkpoint = TaskCheckpoint::new(format!("task-{}", i), format!("Test task {}", i));
        manager.save(&checkpoint).unwrap();
    }

    // List tasks
    let tasks = manager.list_tasks().unwrap();
    assert_eq!(tasks.len(), 5);

    // Delete one
    manager.delete("task-2").unwrap();
    let tasks = manager.list_tasks().unwrap();
    assert_eq!(tasks.len(), 4);
    assert!(!tasks.iter().any(|t| t.task_id == "task-2"));
}

#[test]
fn test_checkpoint_with_git_state() {
    // Capture git state from our actual repo
    let git_state = capture_git_state(".");
    assert!(git_state.is_some());

    let state = git_state.unwrap();
    assert!(!state.branch.is_empty());
    assert!(!state.commit_hash.is_empty());
    // Commit hash should be 40 characters
    assert!(state.commit_hash.len() >= 7);
}

// ============================================================================
// COGNITIVE STATE TESTS
// ============================================================================

#[test]
fn test_cognitive_state_pdvr_cycle() {
    let mut state = CognitiveState::new();
    assert_eq!(state.cycle_phase, CyclePhase::Plan);

    // Progress through PDVR cycle
    state.advance_phase();
    assert_eq!(state.cycle_phase, CyclePhase::Do);

    state.advance_phase();
    assert_eq!(state.cycle_phase, CyclePhase::Verify);

    state.advance_phase();
    assert_eq!(state.cycle_phase, CyclePhase::Reflect);

    state.advance_phase();
    assert_eq!(state.cycle_phase, CyclePhase::Plan); // Cycle back
}

#[test]
fn test_cognitive_state_working_memory() {
    let mut wm = WorkingMemory::new();

    // Set a plan
    wm.set_plan(
        "Fix the authentication bug",
        vec![
            "Read the auth module".to_string(),
            "Identify the bug".to_string(),
            "Write a test".to_string(),
            "Fix the bug".to_string(),
            "Verify the fix".to_string(),
        ],
    );

    assert_eq!(wm.plan_steps.len(), 5);
    assert!(wm.current_plan.is_some());

    // Start working
    let step = wm.start_next_step();
    assert!(step.is_some());
    assert_eq!(step.unwrap().description, "Read the auth module");

    // Complete steps
    wm.complete_step(1, Some("Found the module at src/auth.rs".to_string()));
    wm.complete_step(2, Some("Bug in token validation".to_string()));
    wm.fail_step(3, "Test framework not configured");

    let summary = wm.progress_summary();
    assert!(summary.contains("2/5")); // 2 completed out of 5
    assert!(summary.contains("1 failed"));
}

#[test]
fn test_cognitive_state_episodic_memory() {
    let mut em = EpisodicMemory::new();

    // Record lessons
    em.what_worked(
        "debugging",
        "Using cargo test --nocapture shows print output",
    );
    em.what_failed(
        "refactoring",
        "Renaming without updating imports breaks build",
    );
    em.user_prefers("Always run cargo check before committing");

    assert_eq!(em.lessons.len(), 3);

    // Find relevant lessons (searches in content and tags)
    let relevant = em.find_relevant("cargo");
    assert!(!relevant.is_empty());

    // Recent lessons (most recent first)
    let recent = em.recent_lessons(2);
    assert_eq!(recent.len(), 2);
}

#[test]
fn test_cognitive_state_persistence() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("cognitive_state.json");

    // Create and save state
    let state = CognitiveStateBuilder::new()
        .with_plan(
            "Test plan",
            vec!["Step 1".to_string(), "Step 2".to_string()],
        )
        .with_hypothesis("The bug is in parser.rs")
        .with_phase(CyclePhase::Do)
        .build();

    state.save(&path).unwrap();

    // Load state
    let loaded = CognitiveState::load(&path).unwrap();
    assert_eq!(loaded.cycle_phase, CyclePhase::Do);
    assert!(loaded.working_memory.active_hypothesis.is_some());
    assert_eq!(loaded.working_memory.plan_steps.len(), 2);
}

#[test]
fn test_cognitive_state_summary() {
    let mut state = CognitiveState::new();
    state
        .working_memory
        .set_plan("Fix bug", vec!["Step 1".to_string()]);
    state.working_memory.active_hypothesis = Some("Bug in auth".to_string());
    state.working_memory.add_question("What triggers the bug?");
    state.episodic_memory.what_worked("test", "Lesson learned");

    let summary = state.summary();
    assert!(summary.contains("COGNITIVE STATE"));
    assert!(summary.contains("Fix bug"));
    assert!(summary.contains("Bug in auth"));
}

// ============================================================================
// VERIFICATION GATE TESTS
// ============================================================================

#[tokio::test]
async fn test_verification_gate_config() {
    let config = VerificationConfig::default();
    assert!(config.check_on_edit);
    assert!(!config.test_on_edit); // Tests are slow, opt-in
    assert!(config.format_on_edit);

    let fast_config = VerificationConfig::fast();
    assert!(fast_config.check_on_edit);
    assert!(!fast_config.test_on_edit);
    assert!(!fast_config.lint_on_edit);

    let thorough_config = VerificationConfig::thorough();
    assert!(thorough_config.check_on_edit);
    assert!(thorough_config.test_on_edit);
    assert!(thorough_config.lint_on_edit);
}

#[tokio::test]
async fn test_verification_gate_exclude_patterns() {
    let config = VerificationConfig::default();
    let gate = VerificationGate::new(".", config);

    // README.md should be excluded
    assert!(gate.is_excluded("README.md"));
    assert!(gate.is_excluded("config.json"));
    assert!(gate.is_excluded("notes.txt"));

    // Rust files should not be excluded
    assert!(!gate.is_excluded("src/main.rs"));
    assert!(!gate.is_excluded("tests/test.rs"));
}

#[tokio::test]
async fn test_verification_gate_skip_non_code() {
    let config = VerificationConfig::default();
    let mut gate = VerificationGate::new(".", config);

    // Verify that non-code changes are skipped
    let report = gate
        .verify_change(
            &["README.md".to_string(), "notes.txt".to_string()],
            "documentation_update",
        )
        .await
        .unwrap();

    assert!(report.overall_passed);
    assert!(report.checks.is_empty());
    assert!(report.suggested_next_steps[0].contains("No code files"));
}

#[tokio::test]
async fn test_verification_gate_on_rust_project() {
    // Test against our actual project
    let config = VerificationConfig::fast(); // Just type check
    let mut gate = VerificationGate::new(".", config);

    let result = gate.quick_verify(&["src/main.rs".to_string()]).await;
    // Should succeed since our project compiles
    assert!(result.is_ok());
}

// ============================================================================
// ERROR ANALYZER TESTS
// ============================================================================

#[test]
fn test_error_analyzer_prioritization() {
    let analyzer = ErrorAnalyzer::new();

    let errors = analyzer.analyze_batch(&[
        (None, "unused variable: `x`", "src/main.rs", Some(5), None),
        (
            Some("E0308"),
            "mismatched types",
            "src/main.rs",
            Some(10),
            None,
        ),
        (
            Some("E0433"),
            "unresolved import",
            "src/main.rs",
            Some(1),
            None,
        ),
        (
            Some("E0382"),
            "use of moved value",
            "src/lib.rs",
            Some(20),
            None,
        ),
    ]);

    // Type error (E0308) should be first (priority 1)
    assert_eq!(errors[0].code.as_deref(), Some("E0308"));
    assert_eq!(errors[0].category, ErrorCategory::TypeError);

    // Unresolved import should be second (priority 2)
    assert_eq!(errors[1].code.as_deref(), Some("E0433"));

    // Borrow error should be third (priority 3)
    assert_eq!(errors[2].code.as_deref(), Some("E0382"));

    // Unused warning should be last (priority 20)
    assert_eq!(errors[3].category, ErrorCategory::UnusedWarning);
}

#[test]
fn test_error_analyzer_suggestions() {
    let analyzer = ErrorAnalyzer::new();

    // Test E0425 - cannot find value
    let error = analyzer.analyze(
        Some("E0425"),
        "cannot find value `config` in this scope",
        "src/main.rs",
        Some(10),
        Some(5),
    );
    assert!(error.suggestion.is_some());
    let suggestion = error.suggestion.unwrap();
    assert!(suggestion.notes.unwrap().contains("use")); // "Add 'use' statement"

    // Test E0382 - moved value
    let error = analyzer.analyze(
        Some("E0382"),
        "use of moved value: `data`",
        "src/lib.rs",
        Some(20),
        None,
    );
    assert!(error.suggestion.is_some());
    let suggestion = error.suggestion.unwrap();
    assert!(suggestion.fix_code.is_some());
    assert!(suggestion.fix_code.unwrap().contains("clone"));
}

#[test]
fn test_error_analyzer_grouping() {
    let analyzer = ErrorAnalyzer::new();

    let errors = vec![
        analyzer.analyze(Some("E0308"), "mismatched types", "a.rs", None, None),
        analyzer.analyze(Some("E0308"), "mismatched types", "b.rs", None, None),
        analyzer.analyze(Some("E0433"), "unresolved import", "c.rs", None, None),
        analyzer.analyze(None, "unused variable", "d.rs", None, None),
    ];

    let groups = analyzer.group_by_category(&errors);

    assert_eq!(
        groups.get(&ErrorCategory::TypeError).map(|v| v.len()),
        Some(2)
    );
    assert_eq!(
        groups
            .get(&ErrorCategory::UnresolvedImport)
            .map(|v| v.len()),
        Some(1)
    );
    assert_eq!(
        groups.get(&ErrorCategory::UnusedWarning).map(|v| v.len()),
        Some(1)
    );
}

#[test]
fn test_error_analyzer_summary() {
    let analyzer = ErrorAnalyzer::new();

    let errors = vec![
        analyzer.analyze(Some("E0308"), "type error", "a.rs", None, None),
        analyzer.analyze(None, "unused variable", "b.rs", None, None),
    ];

    let summary = analyzer.summary(&errors);
    assert!(summary.contains("Total errors: 2"));
    assert!(summary.contains("By category:"));
    assert!(summary.contains("Fix first:"));
}

// ============================================================================
// MULTI-STEP SCENARIO TESTS
// ============================================================================

#[tokio::test]
async fn test_multi_step_file_workflow() {
    let mut cfg = selfware::config::SafetyConfig::default();
    cfg.allowed_paths = vec!["/**".to_string()];
    selfware::tools::file::init_safety_config(&cfg);
    let dir = tempdir().unwrap();
    let registry = ToolRegistry::new();

    // Step 1: Create initial file
    let file_write = registry.get("file_write").unwrap();
    file_write
        .execute(serde_json::json!({
            "path": dir.path().join("src/lib.rs").to_str().unwrap(),
            "content": r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#
        }))
        .await
        .unwrap();

    // Step 2: Search for function
    let grep = registry.get("grep_search").unwrap();
    let result = grep
        .execute(serde_json::json!({
            "pattern": "fn add",
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();
    assert!(result["count"].as_i64().unwrap() >= 1);

    // Step 3: Edit the function
    let file_edit = registry.get("file_edit").unwrap();
    file_edit
        .execute(serde_json::json!({
            "path": dir.path().join("src/lib.rs").to_str().unwrap(),
            "old_str": "a + b",
            "new_str": "a.saturating_add(b)"
        }))
        .await
        .unwrap();

    // Step 4: Verify the change
    let file_read = registry.get("file_read").unwrap();
    let result = file_read
        .execute(serde_json::json!({
            "path": dir.path().join("src/lib.rs").to_str().unwrap()
        }))
        .await
        .unwrap();
    assert!(result["content"]
        .as_str()
        .unwrap()
        .contains("saturating_add"));
}

#[tokio::test]
async fn test_symbol_discovery_workflow() {
    let dir = tempdir().unwrap();
    let registry = ToolRegistry::new();

    // Create a Rust file with multiple symbols
    fs::write(
        dir.path().join("main.rs"),
        r#"
struct Config {
    name: String,
    value: i32,
}

impl Config {
    fn new(name: &str) -> Self {
        Config {
            name: name.to_string(),
            value: 0,
        }
    }
}

fn process_config(config: &Config) -> bool {
    config.value > 0
}

fn main() {
    let config = Config::new("test");
    process_config(&config);
}
"#,
    )
    .unwrap();

    // Find all structs
    let symbol = registry.get("symbol_search").unwrap();
    let result = symbol
        .execute(serde_json::json!({
            "name": "Config",
            "path": dir.path().to_str().unwrap(),
            "symbol_type": "struct"
        }))
        .await
        .unwrap();
    assert!(!result["symbols"].as_array().unwrap().is_empty());

    // Find all functions
    let result = symbol
        .execute(serde_json::json!({
            "name": "",
            "path": dir.path().to_str().unwrap(),
            "symbol_type": "function"
        }))
        .await
        .unwrap();
    // Should find: new, process_config, main
    assert!(result["symbols"].as_array().unwrap().len() >= 3);

    // Find impl blocks
    let result = symbol
        .execute(serde_json::json!({
            "name": "Config",
            "path": dir.path().to_str().unwrap(),
            "symbol_type": "impl"
        }))
        .await
        .unwrap();
    assert!(!result["symbols"].as_array().unwrap().is_empty());
}

// ============================================================================
// INTEGRATION SCENARIO: COMPLETE CODING TASK
// ============================================================================

#[tokio::test]
async fn test_complete_coding_scenario() {
    let mut cfg = selfware::config::SafetyConfig::default();
    cfg.allowed_paths = vec!["/**".to_string()];
    selfware::tools::file::init_safety_config(&cfg);
    let dir = tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    let registry = ToolRegistry::new();
    let manager = CheckpointManager::new(dir.path().join(".checkpoints")).unwrap();

    // === STEP 1: Initialize checkpoint ===
    let mut checkpoint = TaskCheckpoint::new(
        "coding-task-001".to_string(),
        "Add a multiply function to the math module".to_string(),
    );

    // === STEP 2: Read existing code ===
    fs::write(
        src_dir.join("math.rs"),
        r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn subtract(a: i32, b: i32) -> i32 {
    a - b
}
"#,
    )
    .unwrap();

    let file_read = registry.get("file_read").unwrap();
    let start = std::time::Instant::now();
    let result = file_read
        .execute(serde_json::json!({
            "path": src_dir.join("math.rs").to_str().unwrap()
        }))
        .await
        .unwrap();

    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_read".to_string(),
        arguments: r#"{"path": "src/math.rs"}"#.to_string(),
        result: Some(
            result["content"]
                .as_str()
                .unwrap()
                .chars()
                .take(100)
                .collect(),
        ),
        success: true,
        duration_ms: Some(start.elapsed().as_millis() as u64),
    });
    checkpoint.set_step(1);
    manager.save(&checkpoint).unwrap();

    // === STEP 3: Search for existing functions ===
    let symbol = registry.get("symbol_search").unwrap();
    let start = std::time::Instant::now();
    let result = symbol
        .execute(serde_json::json!({
            "name": "",
            "path": src_dir.to_str().unwrap(),
            "symbol_type": "function"
        }))
        .await
        .unwrap();

    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "symbol_search".to_string(),
        arguments: r#"{"symbol_type": "function"}"#.to_string(),
        result: Some(format!(
            "Found {} functions",
            result["symbols"].as_array().unwrap().len()
        )),
        success: true,
        duration_ms: Some(start.elapsed().as_millis() as u64),
    });
    checkpoint.set_step(2);
    manager.save(&checkpoint).unwrap();

    // === STEP 4: Add new function ===
    let file_edit = registry.get("file_edit").unwrap();
    let start = std::time::Instant::now();
    file_edit.execute(serde_json::json!({
        "path": src_dir.join("math.rs").to_str().unwrap(),
        "old_str": "pub fn subtract(a: i32, b: i32) -> i32 {\n    a - b\n}",
        "new_str": "pub fn subtract(a: i32, b: i32) -> i32 {\n    a - b\n}\n\npub fn multiply(a: i32, b: i32) -> i32 {\n    a * b\n}"
    })).await.unwrap();

    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_edit".to_string(),
        arguments: "...".to_string(),
        result: Some("Added multiply function".to_string()),
        success: true,
        duration_ms: Some(start.elapsed().as_millis() as u64),
    });
    checkpoint.set_step(3);
    manager.save(&checkpoint).unwrap();

    // === STEP 5: Verify the change ===
    let result = file_read
        .execute(serde_json::json!({
            "path": src_dir.join("math.rs").to_str().unwrap()
        }))
        .await
        .unwrap();

    let content = result["content"].as_str().unwrap();
    assert!(content.contains("pub fn multiply"));
    assert!(content.contains("a * b"));

    checkpoint.set_step(4);
    checkpoint.set_status(TaskStatus::Completed);
    manager.save(&checkpoint).unwrap();

    // === VERIFY: Final checkpoint state ===
    let final_checkpoint = manager.load("coding-task-001").unwrap();
    assert_eq!(final_checkpoint.status, TaskStatus::Completed);
    assert_eq!(final_checkpoint.current_step, 4);
    assert_eq!(final_checkpoint.tool_calls.len(), 3); // read, symbol_search, edit
    assert!(final_checkpoint.tool_calls.iter().all(|tc| tc.success));
}

// ============================================================================
// BENCHMARK: TOOL EXECUTION TIMING
// ============================================================================

#[tokio::test]
async fn test_tool_execution_timing() {
    let mut cfg = selfware::config::SafetyConfig::default();
    cfg.allowed_paths = vec!["/**".to_string()];
    selfware::tools::file::init_safety_config(&cfg);
    let dir = tempdir().unwrap();
    let registry = ToolRegistry::new();

    // Create test files
    for i in 0..10 {
        fs::write(
            dir.path().join(format!("file{}.rs", i)),
            format!("fn function_{}() {{ /* content */ }}\n", i),
        )
        .unwrap();
    }

    // Benchmark file operations
    let file_read = registry.get("file_read").unwrap();
    let start = std::time::Instant::now();
    for i in 0..10 {
        file_read
            .execute(serde_json::json!({
                "path": dir.path().join(format!("file{}.rs", i)).to_str().unwrap()
            }))
            .await
            .unwrap();
    }
    let file_read_duration = start.elapsed();
    println!("10 file reads: {:?}", file_read_duration);
    assert!(file_read_duration.as_millis() < 1000); // Should be fast

    // Benchmark search operations
    let grep = registry.get("grep_search").unwrap();
    let start = std::time::Instant::now();
    let result = grep
        .execute(serde_json::json!({
            "pattern": "function",
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();
    let grep_duration = start.elapsed();
    println!(
        "Grep search ({} matches): {:?}",
        result["count"], grep_duration
    );
    assert!(grep_duration.as_millis() < 500); // Should be fast

    // Benchmark glob operations
    let glob = registry.get("glob_find").unwrap();
    let start = std::time::Instant::now();
    let result = glob
        .execute(serde_json::json!({
            "pattern": "*.rs",
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();
    let glob_duration = start.elapsed();
    println!("Glob find ({} files): {:?}", result["count"], glob_duration);
    assert!(glob_duration.as_millis() < 500); // Should be fast
}
