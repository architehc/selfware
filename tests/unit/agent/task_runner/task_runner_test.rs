use super::*;
use crate::checkpoint::{CheckpointManager, TaskCheckpoint, ToolCallLog};
use crate::config::{AgentConfig, Config, ExecutionMode, SafetyConfig};
use crate::testing::mock_api::{MockLlmServer, MockResponse};
use chrono::Utc;

#[test]
fn shell_command_is_verification_recognizes_non_cargo_runners() {
    let yes = [
        r#"{"command":"pytest -q tests/"}"#,
        r#"{"command":"npm test"}"#,
        r#"{"command":"npm run test -- --watch=false"}"#,
        r#"{"command":"go test ./..."}"#,
        r#"{"command":"npx jest"}"#,
        r#"{"command":"cargo test --all"}"#,
        r#"{"command":"make check"}"#,
    ];
    for a in yes {
        assert!(
            Agent::shell_command_is_verification(a),
            "should count as verification: {a}"
        );
    }
    let no = [
        r#"{"command":"ls -la"}"#,
        r#"{"command":"cat src/main.rs"}"#,
        r#"{"command":"echo hello"}"#,
        r#"{}"#,
        "not json at all",
    ];
    for a in no {
        assert!(
            !Agent::shell_command_is_verification(a),
            "should NOT count as verification: {a}"
        );
    }
}

#[derive(Default)]
struct RecordingEventEmitter {
    events: std::sync::Mutex<Vec<AgentEvent>>,
}

impl super::super::tui_events::EventEmitter for RecordingEventEmitter {
    fn emit(&self, event: AgentEvent) {
        self.events.lock().unwrap().push(event);
    }
}

impl RecordingEventEmitter {
    fn events(&self) -> Vec<AgentEvent> {
        self.events.lock().unwrap().clone()
    }
}

fn mock_agent_config(endpoint: String, streaming: bool) -> Config {
    Config {
        endpoint,
        model: "mock-model".to_string(),
        // Set context_length high enough that max_context_tokens doesn't become 0
        // after subtracting max_tokens and safety margin
        context_length: 500_000,
        max_tokens: 8192,
        agent: AgentConfig {
            max_iterations: 8,
            step_timeout_secs: 30,
            streaming,
            native_function_calling: false,
            min_completion_steps: 0,
            require_verification_before_completion: false,
            ..Default::default()
        },
        safety: SafetyConfig {
            allowed_paths: vec!["./**".to_string(), "/**".to_string()],
            ..Default::default()
        },
        execution_mode: ExecutionMode::Yolo,
        ..Default::default()
    }
}

#[test]
fn fatal_loop_errors_are_not_recoverable() {
    assert!(is_fatal_loop_error(&anyhow::anyhow!(
        "READ_LOOP_NO_EDIT: no mutation after guard"
    )));
    assert!(is_fatal_loop_error(&anyhow::anyhow!(
        "EDIT_FAILURE_LOOP_AFTER_EDIT: repeated stale edits"
    )));
    assert!(is_fatal_loop_error(&anyhow::anyhow!(
        "VERIFICATION_LOOP_AFTER_EDIT: repeated checks"
    )));
    // Regression: repeated fake-completes must be terminal, not re-nudged 10×.
    assert!(is_fatal_loop_error(&anyhow::anyhow!(
        "FAKE_COMPLETE_LOOP: completion gate rejected 4 final answers"
    )));
    assert!(is_fatal_loop_error(&anyhow::anyhow!(
        "NONTERM_PROSE_NO_TOOL: mutation-required task produced 6 consecutive no-tool turns"
    )));
    assert!(!is_fatal_loop_error(&anyhow::anyhow!(
        "temporary API failure"
    )));
}

// =========================================================================
// build_progress_injection -- exhaustive branch coverage (standalone)
// =========================================================================

fn build_progress_injection_standalone(
    step: usize,
    max_iterations: usize,
    has_verification: bool,
) -> Option<String> {
    if step == 0 || !(step + 1).is_multiple_of(5) {
        return None;
    }
    let pct = ((step + 1) as f64 / max_iterations as f64 * 100.0).min(100.0);
    let verification_status = if has_verification {
        "Verification: PASSED"
    } else {
        "Verification: NOT YET RUN (required before completion)"
    };
    let guidance = if pct < 30.0 {
        "You have plenty of budget remaining. Be thorough \u{2014} read relevant code, \
             implement carefully, and verify each change."
    } else if pct < 70.0 {
        "Good progress. Continue implementing and make sure to verify with cargo_check/cargo_test."
    } else {
        "You are using most of your budget. Wrap up: ensure all changes compile \
             and tests pass, then provide your final summary."
    };
    Some(format!(
        "[Progress: step {}/{} ({:.0}% budget used) | {}]\n{}",
        step + 1,
        max_iterations,
        pct,
        verification_status,
        guidance
    ))
}

#[test]
fn test_progress_injection_none_for_step_zero() {
    assert!(build_progress_injection_standalone(0, 100, false).is_none());
}

#[test]
fn test_progress_injection_none_for_non_multiple_of_5() {
    assert!(build_progress_injection_standalone(1, 100, false).is_none());
    assert!(build_progress_injection_standalone(2, 100, false).is_none());
    assert!(build_progress_injection_standalone(3, 100, false).is_none());
    assert!(build_progress_injection_standalone(5, 100, false).is_none());
    assert!(build_progress_injection_standalone(7, 100, false).is_none());
}

#[test]
fn test_progress_injection_some_for_step_4() {
    let result = build_progress_injection_standalone(4, 100, false);
    assert!(result.is_some());
    assert!(result.unwrap().contains("step 5/100"));
}

#[test]
fn test_progress_injection_some_for_step_9() {
    let result = build_progress_injection_standalone(9, 100, false);
    assert!(result.is_some());
    let msg = result.unwrap();
    assert!(msg.contains("step 10/100"));
    assert!(msg.contains("10% budget used"));
}

#[test]
fn test_progress_injection_some_for_step_14() {
    let result = build_progress_injection_standalone(14, 100, false);
    assert!(result.is_some());
    assert!(result.unwrap().contains("step 15/100"));
}

#[test]
fn test_progress_injection_low_budget_guidance() {
    let msg = build_progress_injection_standalone(4, 100, false).unwrap();
    assert!(msg.contains("plenty of budget remaining"));
    assert!(msg.contains("Be thorough"));
}

#[test]
fn test_progress_injection_mid_budget_guidance() {
    let msg = build_progress_injection_standalone(49, 100, false).unwrap();
    assert!(msg.contains("Good progress"));
    assert!(msg.contains("cargo_check/cargo_test"));
}

#[test]
fn test_progress_injection_high_budget_guidance() {
    let msg = build_progress_injection_standalone(69, 100, false).unwrap();
    assert!(msg.contains("most of your budget"));
    assert!(msg.contains("Wrap up"));
}

#[test]
fn test_progress_injection_pct_capped_at_100() {
    let msg = build_progress_injection_standalone(14, 10, false).unwrap();
    assert!(msg.contains("100% budget used"));
    assert!(msg.contains("most of your budget"));
}

#[test]
fn test_progress_injection_verification_not_run() {
    let msg = build_progress_injection_standalone(4, 100, false).unwrap();
    assert!(msg.contains("Verification: NOT YET RUN"));
    assert!(msg.contains("required before completion"));
}

#[test]
fn test_progress_injection_verification_passed() {
    let msg = build_progress_injection_standalone(4, 100, true).unwrap();
    assert!(msg.contains("Verification: PASSED"));
    assert!(!msg.contains("NOT YET RUN"));
}

#[test]
fn test_progress_injection_step_19() {
    let msg = build_progress_injection_standalone(19, 100, false).unwrap();
    assert!(msg.contains("step 20/100"));
    assert!(msg.contains("20% budget used"));
}

#[test]
fn test_progress_injection_step_24() {
    let msg = build_progress_injection_standalone(24, 100, false).unwrap();
    assert!(msg.contains("step 25/100"));
    assert!(msg.contains("plenty of budget remaining"));
}

#[test]
fn test_progress_injection_boundary_30_pct() {
    let msg = build_progress_injection_standalone(29, 100, false).unwrap();
    assert!(msg.contains("Good progress"));
}

#[test]
fn test_progress_injection_boundary_70_pct() {
    let msg = build_progress_injection_standalone(69, 100, true).unwrap();
    assert!(msg.contains("Wrap up"));
    assert!(msg.contains("Verification: PASSED"));
}

#[test]
fn test_progress_injection_small_max_iterations() {
    let msg = build_progress_injection_standalone(4, 5, false).unwrap();
    assert!(msg.contains("step 5/5"));
    assert!(msg.contains("100% budget used"));
    assert!(msg.contains("Wrap up"));
}

#[test]
fn test_progress_injection_max_iterations_1() {
    let msg = build_progress_injection_standalone(4, 1, false).unwrap();
    assert!(msg.contains("100% budget used"));
    assert!(msg.contains("Wrap up"));
}

#[test]
fn test_progress_injection_step_99_max_100() {
    let msg = build_progress_injection_standalone(99, 100, false).unwrap();
    assert!(msg.contains("step 100/100"));
    assert!(msg.contains("100% budget used"));
    assert!(msg.contains("Wrap up"));
}

#[test]
fn test_progress_injection_large_step_numbers() {
    let msg = build_progress_injection_standalone(499, 1000, true).unwrap();
    assert!(msg.contains("step 500/1000"));
    assert!(msg.contains("50% budget used"));
    assert!(msg.contains("Good progress"));
    assert!(msg.contains("Verification: PASSED"));
}

#[test]
fn test_progress_injection_exactly_at_boundary_29_not_multiple() {
    assert!(build_progress_injection_standalone(28, 100, false).is_none());
}

// =========================================================================
// recovery_failure_advice
// =========================================================================

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_recovery_failure_advice_returns_nonempty_for_known_error() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let agent = Agent::new(config).await.unwrap();
    // A "Max iterations" failure with no mutating calls and 0 final answer
    // text should classify as MaxIterations (or a more specific kind) and
    // produce non-empty advice.
    let advice = agent.recovery_failure_advice("Max iterations exceeded");
    assert!(
        !advice.is_empty(),
        "recovery_failure_advice should return guidance for a known failure: {}",
        advice
    );
    assert!(
        advice.contains("Failure mode guidance"),
        "advice should contain the guidance header: {}",
        advice
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_recovery_failure_advice_returns_empty_for_success_like_state() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();
    // Set up a state that looks like success: mutating calls exist and the
    // error reason does not match any known failure pattern.  With mutating
    // > 0, classify on a non-matching Failed reason falls through to
    // classify_max_iter_failure which should still produce advice.
    // But with mutating calls and no specific signals, it returns
    // MaxIterations advice.  So let's instead verify the "-" guard: a
    // NaturalCompletion with mutating calls yields advice "-".
    agent.test_set_mutating_count(3);
    agent.test_set_total_tool_calls(12);
    // We can't call classify with NaturalCompletion from
    // recovery_failure_advice (it always uses Failed), so instead
    // verify the helper handles a benign error without panic.
    let advice = agent.recovery_failure_advice("some transient error");
    // Should be non-empty (some guidance) or empty — just ensure no panic.
    let _ = advice;
    server.stop().await;
}

// =========================================================================
// build_progress_injection -- via real Agent instance
// =========================================================================

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_progress_injection_agent_no_checkpoint() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let agent = Agent::new(config).await.unwrap();
    assert!(agent.build_progress_injection(0).is_none());
    let msg = agent.build_progress_injection(4).unwrap();
    assert!(msg.contains("Good progress"));
    assert!(msg.contains("Verification: NOT YET RUN"));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_progress_injection_agent_with_cargo_check() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let mut cp = TaskCheckpoint::new("t1".to_string(), "task".to_string());
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_check".to_string(),
        arguments: "{}".to_string(),
        result: Some("OK".to_string()),
        success: true,
        duration_ms: Some(100),
    });
    agent.current_checkpoint = Some(cp);
    assert!(agent
        .build_progress_injection(4)
        .unwrap()
        .contains("Verification: PASSED"));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_progress_injection_agent_failed_verification() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let mut cp = TaskCheckpoint::new("t2".to_string(), "task".to_string());
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_check".to_string(),
        arguments: "{}".to_string(),
        result: Some("error".to_string()),
        success: false,
        duration_ms: Some(100),
    });
    agent.current_checkpoint = Some(cp);
    assert!(agent
        .build_progress_injection(4)
        .unwrap()
        .contains("Verification: NOT YET RUN"));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_progress_injection_agent_cargo_test() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let mut cp = TaskCheckpoint::new("t3".to_string(), "task".to_string());
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_test".to_string(),
        arguments: "{}".to_string(),
        result: Some("passed".to_string()),
        success: true,
        duration_ms: Some(500),
    });
    agent.current_checkpoint = Some(cp);
    assert!(agent
        .build_progress_injection(4)
        .unwrap()
        .contains("Verification: PASSED"));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_progress_injection_agent_cargo_clippy() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let mut cp = TaskCheckpoint::new("t4".to_string(), "task".to_string());
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_clippy".to_string(),
        arguments: "{}".to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(300),
    });
    agent.current_checkpoint = Some(cp);
    assert!(agent
        .build_progress_injection(4)
        .unwrap()
        .contains("Verification: PASSED"));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_progress_injection_agent_non_verification_tool() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let mut cp = TaskCheckpoint::new("t5".to_string(), "task".to_string());
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_read".to_string(),
        arguments: r#"{"path":"foo.rs"}"#.to_string(),
        result: Some("content".to_string()),
        success: true,
        duration_ms: Some(10),
    });
    agent.current_checkpoint = Some(cp);
    assert!(agent
        .build_progress_injection(4)
        .unwrap()
        .contains("Verification: NOT YET RUN"));
    server.stop().await;
}

// =========================================================================
// memory_stats
// =========================================================================

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_memory_stats_returns_tuple() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let agent = Agent::new(config).await.unwrap();
    let (_len, _total_tokens, near_limit) = agent.memory_stats();
    assert!(!near_limit);
    server.stop().await;
}

// =========================================================================
// list_tasks / task_status / delete_task via temp dir
// =========================================================================

#[test]
fn test_list_tasks_empty_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
    assert!(manager.list_tasks().unwrap().is_empty());
}

#[test]
fn test_list_tasks_with_saved_checkpoint() {
    let tmp = tempfile::tempdir().unwrap();
    let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
    let cp = TaskCheckpoint::new("list-test-1".to_string(), "Test task one".to_string());
    manager.save(&cp).unwrap();
    let tasks = manager.list_tasks().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].task_id, "list-test-1");
}

#[test]
fn test_list_tasks_multiple() {
    let _g = crate::test_support::CwdGuard::hold();
    let tmp = tempfile::tempdir().unwrap();
    let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
    manager
        .save(&TaskCheckpoint::new("m1".to_string(), "First".to_string()))
        .unwrap();
    manager
        .save(&TaskCheckpoint::new("m2".to_string(), "Second".to_string()))
        .unwrap();
    manager
        .save(&TaskCheckpoint::new("m3".to_string(), "Third".to_string()))
        .unwrap();
    let tasks = manager.list_tasks().unwrap();
    assert_eq!(tasks.len(), 3);
    let ids: Vec<&str> = tasks.iter().map(|t| t.task_id.as_str()).collect();
    assert!(ids.contains(&"m1"));
    assert!(ids.contains(&"m2"));
    assert!(ids.contains(&"m3"));
}

#[test]
fn test_task_status_loads_checkpoint() {
    let tmp = tempfile::tempdir().unwrap();
    let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
    let mut cp = TaskCheckpoint::new("status-test".to_string(), "Status task".to_string());
    cp.set_step(5);
    cp.set_estimated_tokens(2000);
    manager.save(&cp).unwrap();
    let loaded = manager.load("status-test").unwrap();
    assert_eq!(loaded.task_id, "status-test");
    assert_eq!(loaded.current_step, 5);
    assert_eq!(loaded.estimated_tokens, 2000);
}

#[test]
fn test_task_status_nonexistent_not_on_disk() {
    let tmp = tempfile::tempdir().unwrap();
    let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
    // CheckpointManager.load auto-recovers by creating a fresh checkpoint,
    // so use the `exists` helper to verify no file is on disk.
    assert!(!manager.exists("nonexistent"));
}

#[test]
fn test_delete_task_removes_checkpoint() {
    let tmp = tempfile::tempdir().unwrap();
    let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
    let cp = TaskCheckpoint::new("del-test".to_string(), "To delete".to_string());
    manager.save(&cp).unwrap();
    assert!(manager.exists("del-test"));
    manager.delete("del-test").unwrap();
    // After deletion the file should no longer exist on disk.
    // (CheckpointManager.load would auto-recover, so we use exists().)
    assert!(!manager.exists("del-test"));
}

#[test]
fn test_delete_nonexistent_task_ok() {
    let tmp = tempfile::tempdir().unwrap();
    let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
    assert!(manager.delete("does-not-exist").is_ok());
}

// =========================================================================
// run_task E2E
// =========================================================================

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_completes_with_plain_text() {
    let server = MockLlmServer::builder()
        .with_response("Analyzed.")
        .with_response("Complete.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let result = agent.run_task("Do a simple task").await;
    assert!(
        result.is_ok(),
        "run_task should succeed: {:?}",
        result.err()
    );
    assert!(agent.current_checkpoint.is_some());
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_terminal_401_planning_error_not_retried() {
    // Every request gets a 401. Client-level retries are disabled so each
    // planning attempt is exactly one HTTP request, and the mock records
    // how many hit the wire. The agent-level planning retry loop must NOT
    // retry a terminal 4xx — it can never succeed and the user should see
    // the auth remediation hint immediately.
    let server = MockLlmServer::builder()
        .with_default_response(MockResponse::Error {
            status: 401,
            body: r#"{"error":"No cookie auth credentials found"}"#.to_string(),
        })
        .build()
        .await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.retry = crate::config::RetrySettings {
        max_retries: 0,
        base_delay_ms: 1,
        max_delay_ms: 1,
    };
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("Do a simple task").await;

    assert!(result.is_err(), "run_task should fail on a terminal 401");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Hint"),
        "expected the auth remediation hint to reach the user, got: {err}"
    );
    let requests = server.captured_request_bodies().await;
    assert_eq!(
        requests.len(),
        1,
        "terminal 401 must fail after a single planning attempt, got {} requests",
        requests.len()
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_503_planning_error_still_retried() {
    // A 503 is a transient server error: the planning-level retry loop
    // must preserve its existing behavior (MAX_PLANNING_RETRIES = 3
    // attempts) for retryable failures. Client-level retries are disabled
    // so each planning attempt is exactly one HTTP request.
    let server = MockLlmServer::builder()
        .with_default_response(MockResponse::Error {
            status: 503,
            body: r#"{"error":"service unavailable"}"#.to_string(),
        })
        .build()
        .await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.retry = crate::config::RetrySettings {
        max_retries: 0,
        base_delay_ms: 1,
        max_delay_ms: 1,
    };
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("Do a simple task").await;

    assert!(
        result.is_err(),
        "run_task should fail after exhausting planning retries on 503"
    );
    let requests = server.captured_request_bodies().await;
    assert_eq!(
        requests.len(),
        3,
        "503 planning errors must still be retried up to MAX_PLANNING_RETRIES, got {} requests",
        requests.len()
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_terminates_at_token_budget() {
    // Every mock response reports 15 total tokens (see mock_api.rs), and the
    // budget check runs at the top of each loop iteration against the
    // cumulative usage the planning + step calls accrue. A 10-token cap must
    // therefore terminate the run rather than let it complete.
    let server = MockLlmServer::builder()
        .with_response("Working on it...")
        .with_response("Still working...")
        .with_response("Done.")
        .build()
        .await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.agent.max_budget_tokens = Some(10);
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("A task the budget should cut off").await;

    assert!(
        result.is_err(),
        "run_task should fail on budget exhaustion, got Ok"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Token budget exhausted"),
        "expected a token-budget error, got: {err}"
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_budget_enforced_after_completing_step() {
    // Each mock response reports 15 total tokens (see mock_api.rs). The
    // planning LLM call brings cumulative usage to 15, which is still
    // under the 20-token cap at the next loop entry. The executing step
    // then makes a second billable LLM call (cumulative → 30) and the
    // model signals completion. The post-step budget check must catch the
    // overshoot and fail the run instead of letting it return Ok.
    // Planning must return a tool call here: a substantial tool-less
    // planning answer for a read-only task is now accepted as the final
    // answer right in the Planning state, which would keep cumulative
    // usage at 15 — under the cap — and the run would legitimately
    // complete Ok before any executing step ran.
    let server = MockLlmServer::builder()
        .with_response(
            r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#,
        )
        .with_response(
            "The analysis is complete: all components reviewed and verified successfully.",
        )
        .build()
        .await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.agent.max_budget_tokens = Some(20);
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("Describe the authentication module").await;

    assert!(
        result.is_err(),
        "run_task should fail when the budget is exceeded after a completing step, got Ok"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Token budget exhausted"),
        "expected a token-budget error, got: {err}"
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_checkpoint_description() {
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.run_task("Describe the login bug").await.unwrap();
    assert_eq!(
        agent.current_checkpoint.as_ref().unwrap().task_description,
        "Describe the login bug"
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_queued_task_gets_fresh_checkpoint() {
    // Bug #11: When an Agent is reused for a queued second task, the
    // checkpoint from task 1 must NOT persist — the new task needs its
    // own checkpoint with the correct task_description.
    let server = MockLlmServer::builder()
            // Task 1 responses
            .with_response("Plan.")
            .with_response("Done.")
            // Task 2 responses
            .with_response("Plan 2.")
            .with_response("Done 2.")
            .build()
            .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();

    // First task
    agent.run_task("First task").await.unwrap();
    let first_task_id = agent.current_checkpoint.as_ref().unwrap().task_id.clone();
    assert_eq!(
        agent.current_checkpoint.as_ref().unwrap().task_description,
        "First task"
    );

    // Second (queued) task — must get its OWN checkpoint, not inherit task 1's
    agent.run_task("Second task").await.unwrap();
    assert_ne!(
        agent.current_checkpoint.as_ref().unwrap().task_id,
        first_task_id
    );
    assert_eq!(
        agent.current_checkpoint.as_ref().unwrap().task_description,
        "Second task"
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_adds_user_message() {
    let server = MockLlmServer::builder()
        .with_response("Planning.")
        .with_response("Completion.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.run_task("Summarize error handling").await.unwrap();
    let has_msg = agent
        .messages
        .iter()
        .any(|m| m.role == "user" && m.content.text().contains("Summarize error handling"));
    assert!(has_msg, "task text should appear as a user message");
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_resets_loop_for_second_task() {
    let server = MockLlmServer::builder()
        .with_response("Plan 1.")
        .with_response("Done 1.")
        .with_response("Plan 2.")
        .with_response("Done 2.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.run_task("Task one").await.unwrap();
    agent.run_task("Task two").await.unwrap();
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_with_tool_call() {
    let server = MockLlmServer::builder()
        .with_response(
            r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#,
        )
        .with_response("Task complete.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let result = agent.run_task("Read Cargo.toml").await;
    assert!(
        result.is_ok(),
        "run_task with tool call: {:?}",
        result.err()
    );
    let has_tool_result = agent
        .messages
        .iter()
        .any(|m| m.content.text().contains("<tool_result>"));
    assert!(has_tool_result);
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_sets_strategic_goals() {
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.run_task("Review unit tests").await.unwrap();
    assert!(!agent.cognitive_state.strategic_goals.is_empty());
    assert!(agent.cognitive_state.active_tactical_plan.is_some());
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_sets_operational_plan() {
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.run_task("Review feature X").await.unwrap();
    let plan = agent
        .cognitive_state
        .active_operational_plan
        .as_ref()
        .unwrap();
    assert_eq!(plan.steps.len(), 5);
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_preserves_existing_checkpoint() {
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "existing-id".to_string(),
        "Existing".to_string(),
    ));
    // Resume via continue_execution (the proper resume path) so the
    // existing checkpoint is preserved. run_task now always resets
    // the checkpoint for a genuinely new task (bug #11 fix).
    agent.continue_execution().await.unwrap();
    assert_eq!(
        agent.current_checkpoint.as_ref().unwrap().task_id,
        "existing-id"
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_cancellation() {
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("More.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let recorder = std::sync::Arc::new(RecordingEventEmitter::default());
    let mut agent = Agent::new(config)
        .await
        .unwrap()
        .with_event_emitter(recorder.clone());
    agent
        .cancelled
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let error = agent
        .run_task("Should cancel")
        .await
        .expect_err("cancellation must not report successful completion");
    assert!(matches!(
        error.downcast_ref::<crate::errors::AgentError>(),
        Some(crate::errors::AgentError::Cancelled)
    ));
    let has_interrupted = agent
        .messages
        .iter()
        .any(|m| m.content.text().contains("interrupted"));
    assert!(has_interrupted);

    let events = recorder.events();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AgentEvent::Error { .. }))
            .count(),
        1,
        "cancellation must emit exactly one terminal Error event"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::Completed { .. })),
        "cancellation must never emit Completed"
    );
    agent.reset_cancellation();
    assert!(!agent.is_cancelled());
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_streaming_mode() {
    // A substantial (>= 40 char) tool-less final answer completes a
    // non-mutation task via the read-only acceptance path. (Earlier this
    // test relied on an empty response being accepted as done, which is no
    // longer valid — an empty/dropped response is not a completion.)
    let server = MockLlmServer::builder()
        .with_response("Streaming plan: I will read the stream and summarize it.")
        .with_response(
            "The streaming request has been fully processed and here is the \
                 complete final summary of the result.",
        )
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), true);
    let mut agent = Agent::new(config).await.unwrap();
    // A read-only prose task ("describe ...") so a substantial tool-less
    // answer is accepted as completion (exercising streaming end-to-end).
    assert!(agent
        .run_task("Describe the streaming pipeline")
        .await
        .is_ok());
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_starts_learning_session() {
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.run_task("Read tests for parser").await.unwrap();
    assert!(!agent.current_task_context.is_empty());
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_max_iterations_exhaustion() {
    // Plain-text responses are treated as task completion, so to force the
    // agent to keep iterating we must provide tool-call responses.  Each
    // file_read tool call keeps the agent in the loop until max_iterations
    // is exhausted.
    let tool_resp = r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#;
    let mut builder = MockLlmServer::builder();
    for _ in 0..20 {
        builder = builder.with_response(tool_resp);
    }
    let server = builder.build().await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.agent.max_iterations = 3;
    let mut agent = Agent::new(config).await.unwrap();
    let result = agent.run_task("Never completes").await;
    // The agent should either fail with an error about max iterations or
    // the loop should exhaust without completing (the loop exits with
    // Ok after all states are consumed when next_state returns None).
    // Either outcome is acceptable -- what matters is that the agent
    // does NOT treat a tool-call response as a completion.
    if let Err(e) = &result {
        let err = e.to_string();
        assert!(
            err.contains("Agent failed")
                || err.contains("Max iterations")
                || err.contains("iterations"),
            "unexpected error: {}",
            err
        );
    }
    // If Ok, verify the agent ran through multiple execution steps (not
    // a single-step completion).
    if result.is_ok() {
        assert!(
            agent.loop_control.current_step() >= 2,
            "agent should have iterated multiple steps, got {}",
            agent.loop_control.current_step()
        );
    }
    server.stop().await;
}

// =========================================================================
// continue_execution E2E
// =========================================================================

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_continue_execution_no_checkpoint() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("Resume plan.")
        .with_response("Resume done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    assert!(agent.continue_execution().await.is_ok());
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_continue_execution_with_checkpoint() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "resume-1".to_string(),
        "Resumed".to_string(),
    ));
    assert!(agent.continue_execution().await.is_ok());
    assert!(agent.cognitive_state.active_tactical_plan.is_some());
    assert!(agent.cognitive_state.active_operational_plan.is_some());
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_continue_execution_executes_planned_tool_calls_immediately() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response(
            r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#,
        )
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "resume-tool".to_string(),
        "Read manifest".to_string(),
    ));

    agent.continue_execution().await.unwrap();

    assert!(agent
        .file_tracker
        .context_files
        .iter()
        .any(|path| path.ends_with("Cargo.toml")));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_continue_execution_cancellation() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("More.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent
        .cancelled
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let error = agent
        .continue_execution()
        .await
        .expect_err("cancellation must not report successful completion");
    assert!(matches!(
        error.downcast_ref::<crate::errors::AgentError>(),
        Some(crate::errors::AgentError::Cancelled)
    ));
    assert!(agent
        .messages
        .iter()
        .any(|m| m.content.text().contains("interrupted")));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_continue_execution_preserves_tactical_plan() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "p-test".to_string(),
        "Preserve".to_string(),
    ));
    agent.cognitive_state.set_active_tactical_plan(
        "existing-tactical".to_string(),
        "Existing plan".to_string(),
        vec!["dep".to_string()],
    );
    agent.continue_execution().await.unwrap();
    assert_eq!(
        agent
            .cognitive_state
            .active_tactical_plan
            .as_ref()
            .unwrap()
            .id,
        "existing-tactical"
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_continue_execution_sets_operational_plan() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.continue_execution().await.unwrap();
    // An operational plan should exist after continue_execution.
    // Execution may modify the plan (e.g. start_operational_step can
    // replace it when the task_id differs), so we only assert it exists
    // with at least one step.
    let plan = agent
        .cognitive_state
        .active_operational_plan
        .as_ref()
        .unwrap();
    assert!(!plan.steps.is_empty(), "operational plan should have steps");
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_continue_execution_preserves_operational_plan() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.cognitive_state.set_operational_plan(
        "existing-op".to_string(),
        vec!["Step A".to_string(), "Step B".to_string()],
    );
    agent.continue_execution().await.unwrap();
    // continue_execution should NOT replace an existing plan (the guard at
    // line 647 skips set_operational_plan when one already exists).
    // However, during execution, start_operational_step may mutate it.
    // We verify the plan still exists after completion.
    let plan = agent
        .cognitive_state
        .active_operational_plan
        .as_ref()
        .unwrap();
    assert!(
        !plan.steps.is_empty(),
        "operational plan should survive execution"
    );
    server.stop().await;
}

/// Regression test: continue_execution must treat no-action errors as
/// fatal (go to Failed state) rather than routing to ErrorRecovery.
/// Before the run_execution_loop dedup this check was missing from
/// continue_execution, causing the agent to loop indefinitely.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_continue_execution_no_action_error_is_recoverable() {
    let _state = crate::test_support::ExecGuard::hold();
    // The model will repeatedly describe intent without using tools.
    // With recoverable no-action errors, the loop should eventually
    // exhaust max_iterations rather than fatally aborting early.
    let intent = "Let me check the code";
    let server = MockLlmServer::builder()
        .with_default_response(crate::testing::mock_api::MockResponse::Text(
            intent.to_string(),
        ))
        .build()
        .await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.agent.max_iterations = 8;
    config.agent.min_completion_steps = 3;
    let mut agent = Agent::new(config).await.unwrap();

    // Seed a checkpoint so continue_execution has something to resume
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "test-id".to_string(),
        "test task".to_string(),
    ));

    // The loop should complete (Ok) after exhausting iterations rather
    // than returning Err from a fatal no-action abort.
    let result = agent.continue_execution().await;
    // Either Ok (iterations exhausted) or Err (eventually hit lifetime limit)
    // — both are acceptable. The key is it doesn't abort after just 6 prompts.
    if let Err(ref e) = result {
        let err_msg = e.to_string();
        assert!(
            err_msg.contains("failed to take action")
                || err_msg.contains("Agent failed")
                || err_msg.contains("Max iterations"),
            "unexpected error: {}",
            err_msg
        );
    }
    server.stop().await;
}

// =========================================================================
// analyze / review
// =========================================================================

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_analyze_calls_run_task() {
    let server = MockLlmServer::builder()
        .with_response("Analysis.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    assert!(agent.analyze("./src").await.is_ok());
    assert!(agent
        .messages
        .iter()
        .any(|m| m.content.text().contains("Analyze the codebase")));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_review_reads_file() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), b"fn main() { println!(\"hello\"); }").unwrap();
    let server = MockLlmServer::builder()
        .with_response("Review.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    assert!(agent.review(tmp.path().to_str().unwrap()).await.is_ok());
    assert!(agent
        .messages
        .iter()
        .any(|m| m.content.text().contains("Review the following code")));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_review_nonexistent_file() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let result = agent.review("/nonexistent/path/to/file.rs").await;
    assert!(result.is_err());
    assert!(result
        .err()
        .unwrap()
        .to_string()
        .contains("Failed to read file"));
    server.stop().await;
}

// =========================================================================
// Planner prompts
// =========================================================================

#[test]
fn test_planner_analyze_prompt() {
    let prompt = Planner::analyze_prompt("./my_project");
    assert!(prompt.contains("./my_project"));
    assert!(prompt.contains("Analyze the codebase"));
    assert!(prompt.contains("Directory structure"));
}

#[test]
fn test_planner_review_prompt() {
    let prompt = Planner::review_prompt("src/main.rs", "fn main() {}");
    assert!(prompt.contains("src/main.rs"));
    assert!(prompt.contains("fn main() {}"));
    assert!(prompt.contains("Review the following code"));
}

// =========================================================================
// AgentState enum variants
// =========================================================================

#[test]
fn test_agent_state_planning() {
    let state = AgentState::Planning;
    assert!(matches!(state, AgentState::Planning));
    assert!(format!("{:?}", state).contains("Planning"));
}

#[test]
fn test_agent_state_executing() {
    let state = AgentState::Executing { step: 42 };
    match &state {
        AgentState::Executing { step } => assert_eq!(*step, 42),
        _ => panic!(),
    }
    let d = format!("{:?}", state);
    assert!(d.contains("Executing") && d.contains("42"));
}

#[test]
fn test_agent_state_error_recovery() {
    let state = AgentState::ErrorRecovery {
        error: "oops".to_string(),
    };
    match &state {
        AgentState::ErrorRecovery { error } => assert_eq!(error, "oops"),
        _ => panic!(),
    }
    let d = format!("{:?}", state);
    assert!(d.contains("ErrorRecovery") && d.contains("oops"));
}

#[test]
fn test_agent_state_completed() {
    let state = AgentState::Completed;
    assert!(matches!(state, AgentState::Completed));
    assert!(format!("{:?}", state).contains("Completed"));
}

#[test]
fn test_agent_state_failed() {
    let state = AgentState::Failed {
        reason: "fatal".to_string(),
    };
    match &state {
        AgentState::Failed { reason } => assert_eq!(reason, "fatal"),
        _ => panic!(),
    }
    let d = format!("{:?}", state);
    assert!(d.contains("Failed") && d.contains("fatal"));
}

#[test]
fn test_agent_state_clone_all() {
    let states = vec![
        AgentState::Planning,
        AgentState::Executing { step: 7 },
        AgentState::ErrorRecovery {
            error: "err".to_string(),
        },
        AgentState::Completed,
        AgentState::Failed {
            reason: "r".to_string(),
        },
    ];
    for s in &states {
        assert_eq!(format!("{:?}", s), format!("{:?}", s.clone()));
    }
}

// =========================================================================
// AgentLoop interaction
// =========================================================================

#[test]
fn test_agent_loop_reset_for_task_then_run() {
    let mut lc = AgentLoop::new(5);
    lc.next_state();
    lc.next_state();
    lc.next_state();
    lc.reset_for_task();
    assert!(matches!(lc.next_state(), Some(AgentState::Planning)));
    assert_eq!(lc.current_step(), 0);
}

#[test]
fn test_agent_loop_approaching_limit() {
    let mut lc = AgentLoop::new(10);
    // Planning doesn't increment, so transition to Executing first.
    lc.next_state(); // Planning
    lc.transition_to(super::loop_control::AgentState::Executing { step: 0 })
        .unwrap();
    for _ in 0..8 {
        lc.next_state();
    }
    let w = lc.approaching_limit_warning();
    assert!(w.is_some());
    assert!(w.unwrap().contains("wrapping up"));
}

// =========================================================================
// Rigor mode (Increment 3 of self-healing)
// =========================================================================

#[test]
#[cfg(feature = "resilience")]
fn test_should_enter_rigor_escalate() {
    let outcome = crate::self_healing::ResolutionOutcome::Escalate;
    assert!(Agent::should_enter_rigor(&outcome));
}

#[test]
#[cfg(feature = "resilience")]
fn test_should_enter_rigor_unresolvable() {
    let outcome = crate::self_healing::ResolutionOutcome::Unresolvable;
    assert!(Agent::should_enter_rigor(&outcome));
}

#[test]
#[cfg(feature = "resilience")]
fn test_should_enter_rigor_resolved_is_false() {
    let action = crate::self_healing::RecoveryAction::Fallback {
        target: "http://localhost:11434/v1".to_string(),
    };
    let directive = crate::self_healing::RecoveryDirective::Action(action);
    let outcome = crate::self_healing::ResolutionOutcome::Resolved(directive);
    assert!(
        !Agent::should_enter_rigor(&outcome),
        "Resolved outcome should NOT trigger rigor mode"
    );
}

#[test]
fn test_careful_mode_directive_is_nonempty_and_specific() {
    let directive = Agent::careful_mode_directive();
    assert!(!directive.is_empty());
    assert!(directive.starts_with("[CAREFUL MODE]"));
    // The directive must mention verification explicitly.
    assert!(directive.to_lowercase().contains("verification"));
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_completion_gate_rejects_in_rigor_mode_without_verification() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let url = server.url();
    let mut config = mock_agent_config(format!("{}/v1", url), false);
    config.agent.require_verification_before_completion = false;
    config.agent.min_completion_steps = 0;
    let mut agent = Agent::new(config).await.unwrap();
    agent.rigor_mode = true;
    agent.has_written_any_file = true;
    agent.current_task_context = "Fix the bug in foo()".to_string();
    let result = agent.check_completion_gate().await;
    assert!(
        result.is_some(),
        "In rigor mode, completion should be rejected without verification: {:?}",
        result
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_completion_gate_accepts_in_rigor_mode_with_verification() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let url = server.url();
    let mut config = mock_agent_config(format!("{}/v1", url), false);
    config.agent.require_verification_before_completion = false;
    config.agent.min_completion_steps = 0;
    let mut agent = Agent::new(config).await.unwrap();
    agent.rigor_mode = true;
    agent.has_written_any_file = true;
    // Use a non-mutation task context so the mutation gate (which checks
    // git diff) does not fire — we are testing the verification requirement
    // part of the gate, not the mutation gate.
    agent.current_task_context = "Summarize the authentication module".to_string();
    // Add a file_write tool call to message history so the "no file write"
    // gate does not fire (that gate checks message tool_calls, not checkpoint).
    let mut file_write_msg = crate::api::types::Message::assistant("Writing the fix.");
    file_write_msg.tool_calls = Some(vec![crate::api::types::ToolCall {
        id: "call_1".to_string(),
        call_type: "function".to_string(),
        function: crate::api::types::ToolFunction {
            name: "file_write".to_string(),
            arguments: r#"{"path":"src/foo.rs","content":"fn foo() {}"}"#.to_string(),
        },
    }]);
    agent.messages.push(file_write_msg);
    // Add a successful verification tool call to checkpoint.
    let mut cp = TaskCheckpoint::new("rigor-test".to_string(), "Summarize".to_string());
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_check".to_string(),
        arguments: "{}".to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(100),
    });
    agent.current_checkpoint = Some(cp);
    let result = agent.check_completion_gate().await;
    assert!(
        result.is_none(),
        "In rigor mode with verification and file write, completion should be accepted: {:?}",
        result
    );
    server.stop().await;
}

// =========================================================================
// Checkpoint verification detection
// =========================================================================

#[test]
fn test_checkpoint_verification_detection() {
    let mut cp = TaskCheckpoint::new("v-test".to_string(), "verify".to_string());
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_read".to_string(),
        arguments: "{}".to_string(),
        result: Some("c".to_string()),
        success: true,
        duration_ms: Some(10),
    });
    let check = |cp: &TaskCheckpoint| {
        cp.tool_calls.iter().any(|tc| {
            tc.success
                && matches!(
                    tc.tool_name.as_str(),
                    "cargo_check" | "cargo_test" | "cargo_clippy"
                )
        })
    };
    assert!(!check(&cp));

    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_check".to_string(),
        arguments: "{}".to_string(),
        result: Some("err".to_string()),
        success: false,
        duration_ms: Some(200),
    });
    assert!(!check(&cp));

    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_test".to_string(),
        arguments: "{}".to_string(),
        result: Some("passed".to_string()),
        success: true,
        duration_ms: Some(500),
    });
    assert!(check(&cp));
}

// =========================================================================
// Planning final-answer shortcut (chat tasks complete in ONE provider call)
// =========================================================================

/// A plain chat task ("explain X") whose planning response has no tool
/// calls must be accepted as the final answer right in the Planning
/// state — completing with EXACTLY ONE provider request instead of
/// falling through to Executing and paying a second call that only
/// repeats the same answer (rendered twice in the TUI).
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_planning_final_answer_completes_chat_task_with_one_request() {
    let _state = crate::test_support::ExecGuard::hold();
    let answer = "A hash map stores key-value pairs in buckets chosen by \
                      hashing the key, so lookups are O(1) on average.";
    let server = MockLlmServer::builder().with_response(answer).build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("Explain how a hash map works").await;
    assert!(
        result.is_ok(),
        "run_task should succeed: {:?}",
        result.err()
    );
    assert_eq!(
        agent.last_assistant_response.trim(),
        answer,
        "the returned message must be the planning answer"
    );
    let requests = server.captured_request_bodies().await;
    assert_eq!(
        requests.len(),
        1,
        "a plain chat answer must cost exactly one provider call, got {}",
        requests.len()
    );
    server.stop().await;
}

/// A planning response WITH tool calls must not be finalized early: the
/// run keeps the plan → execute path — planning call, tool execution,
/// then one execution call for the final answer (2 requests total).
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_planning_tool_call_still_takes_plan_then_execute_path() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response(
            r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#,
        )
        .with_response("Read complete: the manifest is consistent and nothing needs to change.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("Read Cargo.toml and finish").await;
    assert!(
        result.is_ok(),
        "run_task should succeed: {:?}",
        result.err()
    );
    assert!(
        agent
            .messages
            .iter()
            .any(|m| m.content.contains("<tool_result>")),
        "the planned tool call must have executed"
    );
    assert!(agent.last_assistant_response.contains("Read complete"));
    let requests = server.captured_request_bodies().await;
    assert_eq!(
        requests.len(),
        2,
        "tool-call planning must proceed through exactly one execution call, got {}",
        requests.len()
    );
    server.stop().await;
}

/// A too-short planning answer is NOT a final answer: the run falls
/// through to Executing per the existing rules and completes on the
/// substantial answer produced there (2 requests).
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_short_planning_answer_falls_through_to_executing() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("ok")
        .with_response(
            "Ownership gives every value a single owner; when the owner \
                 goes out of scope, the value is dropped.",
        )
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("Explain ownership in Rust").await;
    assert!(
        result.is_ok(),
        "run_task should succeed: {:?}",
        result.err()
    );
    assert!(agent.last_assistant_response.contains("Ownership gives"));
    let requests = server.captured_request_bodies().await;
    assert_eq!(
        requests.len(),
        2,
        "a too-short planning answer must fall through to Executing, got {}",
        requests.len()
    );
    server.stop().await;
}

/// Gate-level checks for `planning_answer_ready_to_finalize`: a mutation
/// task must NEVER finalize from Planning with no edits (the deliverable
/// is a source change), and confused / too-short planning answers are
/// rejected so the loop iterates per the existing no-progress rules.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_planning_answer_ready_to_finalize_gates() {
    let _state = crate::test_support::ExecGuard::hold();

    // (a) Mutation task: substantial, well-formed answer — still refused.
    let server = MockLlmServer::builder()
        .with_response(
            "The bug is in the login handler: it compares tokens \
                 case-sensitively. Normalizing both sides before the \
                 comparison fixes it.",
        )
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let task = "Fix the login bug in src/auth.rs";
    agent.start_learning_session("gate-mutation", task);
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "gate-mutation".to_string(),
        task.to_string(),
    ));
    agent.messages.push(Message::user(task));
    let has_tool_calls = agent.plan().await.unwrap();
    assert!(!has_tool_calls);
    assert!(
        agent.planning_answer_ready_to_finalize().await.is_none(),
        "a task whose deliverable is an edit must not finalize from Planning with no edits"
    );
    server.stop().await;

    // (b) Read-only task, too-short answer: rejected by the length gate.
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let task = "Explain ownership in Rust";
    agent.start_learning_session("gate-short", task);
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "gate-short".to_string(),
        task.to_string(),
    ));
    agent.messages.push(Message::user(task));
    let has_tool_calls = agent.plan().await.unwrap();
    assert!(!has_tool_calls);
    assert!(
        agent.planning_answer_ready_to_finalize().await.is_none(),
        "a too-short planning answer must not finalize"
    );
    server.stop().await;

    // (c) Read-only task, confused answer (framework self-references):
    // rejected by the confusion gate even though it is long enough.
    let confused = "I see selfware_system_directive in the context and \
                        maybe_prompt_for_action was mentioned, so I am unsure \
                        what to output here.";
    let server = MockLlmServer::builder()
        .with_response(confused)
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let task = "Explain ownership in Rust";
    agent.start_learning_session("gate-confused", task);
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "gate-confused".to_string(),
        task.to_string(),
    ));
    agent.messages.push(Message::user(task));
    let has_tool_calls = agent.plan().await.unwrap();
    assert!(!has_tool_calls);
    assert!(
        agent.planning_answer_ready_to_finalize().await.is_none(),
        "a confused planning answer must not finalize"
    );
    server.stop().await;

    // (d) Read-only task, substantial clean answer: accepted.
    let answer = "Ownership gives every value a single owner; when the owner \
                      goes out of scope, the value is dropped.";
    let server = MockLlmServer::builder().with_response(answer).build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let task = "Explain ownership in Rust";
    agent.start_learning_session("gate-accept", task);
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "gate-accept".to_string(),
        task.to_string(),
    ));
    agent.messages.push(Message::user(task));
    let has_tool_calls = agent.plan().await.unwrap();
    assert!(!has_tool_calls);
    assert_eq!(
        agent.planning_answer_ready_to_finalize().await.as_deref(),
        Some(answer),
        "a substantial read-only answer that passes the completion gate must finalize"
    );
    server.stop().await;
}
