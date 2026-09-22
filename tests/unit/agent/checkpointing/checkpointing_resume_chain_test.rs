//! Resume-segment accounting (2026-09-22 long-horizon findings):
//! - The adaptive iteration cap earned before a checkpoint must survive the
//!   checkpoint → resume round-trip (it used to reset to the configured cap).
//! - A resumed run must emit the same progress events as a fresh run, and the
//!   end-of-run summary must count the WHOLE task chain (iterations, files
//!   changed), not just the resumed segment.

use crate::agent::loop_control::{AgentLoop, MAX_ITERATIONS_STOP_REASON};
use crate::agent::Agent;
use crate::api::types::Message;
use crate::checkpoint::{CheckpointManager, TaskCheckpoint, ToolCallLog};
use crate::testing::mock_api::MockLlmServer;
use chrono::Utc;

/// Persist `checkpoint` into the checkpoint store under the (fake) `$HOME`,
/// exactly where `Agent::resume`'s `CheckpointManager::default_path()` reads.
fn save_under_fake_home(fake_home: &std::path::Path, checkpoint: &TaskCheckpoint) {
    let manager = CheckpointManager::new(fake_home.join(".selfware").join("checkpoints")).unwrap();
    manager.save_final(checkpoint).unwrap();
}

/// A prior segment's checkpoint: cap already extended once (12 → 15),
/// 13 chain-wide iterations, and one successful file_write as evidence.
fn prior_segment_checkpoint(task_id: &str, task_description: &str) -> TaskCheckpoint {
    let mut cp = TaskCheckpoint::new(task_id.to_string(), task_description.to_string());
    cp.set_step(3);
    cp.set_iteration(12);
    cp.cumulative_iterations = 13;
    cp.effective_max_iterations = Some(15);
    cp.extensions_granted = 1;
    cp.set_messages(vec![Message::user(task_description.to_string())]);
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_write".to_string(),
        arguments: serde_json::json!({"path": "src/foo.rs", "content": "pub fn x() {}"})
            .to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(1),
    });
    // A FAILED write must not enter the files-changed evidence.
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_write".to_string(),
        arguments: serde_json::json!({"path": "src/bar.rs", "content": "broken"}).to_string(),
        result: Some("permission denied".to_string()),
        success: false,
        duration_ms: Some(1),
    });
    cp
}

#[tokio::test]
async fn extended_cap_survives_checkpoint_round_trip_into_resume() {
    let fake_home = tempfile::tempdir().unwrap();
    let env = crate::test_support::EnvGuard::capture(&["HOME"]);
    env.set("HOME", fake_home.path().as_os_str());

    let server = MockLlmServer::builder().with_response("done").build().await;
    let endpoint = format!("{}/v1", server.url());

    // Prior segment: configured cap 12, one +25% grant earned (cap 15).
    let mut prior = Agent::new(crate::test_support::mock_agent_config(&endpoint))
        .await
        .unwrap();
    prior.loop_control = AgentLoop::new(12);
    assert_eq!(prior.loop_control.extend_budget_once(), Some(3));
    assert_eq!(prior.loop_control.max_iterations(), 15);
    prior.loop_control.restore_progress(3, 12);
    prior.messages.push(Message::user("prior work".to_string()));

    let checkpoint = prior.to_checkpoint("cap-task", "a long task");
    assert_eq!(
        checkpoint.effective_max_iterations,
        Some(15),
        "to_checkpoint must persist the extended cap"
    );
    assert_eq!(checkpoint.extensions_granted, 1);
    save_under_fake_home(fake_home.path(), &checkpoint);

    // Resume with the ORIGINAL configured cap: the earned extension must
    // come back instead of resetting to 12.
    let mut config = crate::test_support::mock_agent_config(&endpoint);
    config.agent.max_iterations = 12;
    let resumed = Agent::resume(config, "cap-task").await.unwrap();
    assert_eq!(
        resumed.loop_control.max_iterations(),
        15,
        "resume must restore the earned extension, not the configured cap"
    );
    assert_eq!(resumed.loop_control.extensions_granted(), 1);
    assert!(!resumed.loop_control.extension_ceiling_reached());

    // An operator re-passing a LARGER cap wins (extensions only ever grow).
    let mut bigger = crate::test_support::mock_agent_config(&endpoint);
    bigger.agent.max_iterations = 20;
    let resumed_bigger = Agent::resume(bigger, "cap-task").await.unwrap();
    assert_eq!(resumed_bigger.loop_control.max_iterations(), 20);
    assert_eq!(resumed_bigger.loop_control.extensions_granted(), 1);
    server.stop().await;
}

#[tokio::test]
async fn resume_restores_chain_wide_iterations_and_files_changed() {
    let fake_home = tempfile::tempdir().unwrap();
    let env = crate::test_support::EnvGuard::capture(&["HOME"]);
    env.set("HOME", fake_home.path().as_os_str());

    save_under_fake_home(
        fake_home.path(),
        &prior_segment_checkpoint("chain-task", "Review the module and report"),
    );

    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    let agent = Agent::resume(config, "chain-task").await.unwrap();

    // The per-segment counter resets (budget fairness) while the chain-wide
    // total carries the prior segments.
    assert_eq!(agent.loop_control.current_iteration(), 0);
    assert_eq!(
        agent.cumulative_iterations(),
        13,
        "the chain-wide iteration total must restore from the checkpoint"
    );
    // The chain summary reports the accumulated count even before the
    // resumed segment runs a turn.
    assert_eq!(agent.chain_run_summary().iterations, 13);

    // Files-changed evidence accumulates across segments: the successful
    // write is restored, the failed one is not.
    let files = agent.run_summary().files_changed;
    assert!(
        files.iter().any(|f| f == "src/foo.rs"),
        "prior segment's write must be in the summary evidence: {files:?}"
    );
    assert!(
        !files.iter().any(|f| f == "src/bar.rs"),
        "a failed write is not evidence: {files:?}"
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn resumed_run_emits_progress_events_and_accumulates_chain_totals() {
    // Driving the real loop touches the process-global cwd (completion gate)
    // and the codemap budget atomics — take the shared exec guard.
    let _exec = crate::test_support::ExecGuard::hold();
    let fake_home = tempfile::tempdir().unwrap();
    let env = crate::test_support::EnvGuard::capture(&["HOME"]);
    env.set("HOME", fake_home.path().as_os_str());

    save_under_fake_home(
        fake_home.path(),
        &prior_segment_checkpoint("chain-task", "Review the module and report"),
    );

    let server = MockLlmServer::builder()
        .with_response("Review complete: the module is sound and the report is written.")
        .build()
        .await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.agent.max_iterations = 12;

    // Attach a recording emitter exactly like the CLI resume paths now do
    // (they used to leave the default no-op emitter in place).
    let recorder = crate::agent::progress::RecordingProgressEmitter::new();
    let mut agent = Agent::resume(config, "chain-task")
        .await
        .unwrap()
        .with_progress_emitter(std::sync::Arc::new(recorder.clone()));

    let result = agent.continue_execution().await;
    server.stop().await;
    assert!(result.is_ok(), "resumed run completes: {:?}", result.err());

    // Finding 4: a resumed run emits the same progress events as a fresh
    // run, including a terminal one.
    let kinds = recorder.kinds();
    assert!(
        !kinds.is_empty(),
        "a resumed run must emit progress events, not run silent"
    );
    assert!(
        kinds
            .iter()
            .any(|k| *k == "task_completed" || *k == "task_failed"),
        "a terminal progress event must fire: {kinds:?}"
    );

    // Finding 4: the end-of-run summary counts the WHOLE chain — the
    // resumed segment added to the restored 13, not a per-segment reset.
    assert!(
        agent.cumulative_iterations() > 13,
        "the resumed segment must accumulate onto the prior total, got {}",
        agent.cumulative_iterations()
    );
    let summary = agent.chain_run_summary();
    assert_eq!(summary.iterations, agent.cumulative_iterations());
    // Finding 3, end-to-end: the restored extension is what the summary
    // reports as the cap, and the grant shows as consumed.
    assert_eq!(summary.max_iterations, 15);
    assert!(summary.budget_extended);
}

#[tokio::test]
async fn cap_stop_reason_constant_matches_the_loop_failure_reason() {
    // The autocontinue policy gate (session/checkpoint.rs) matches on
    // MAX_ITERATIONS_STOP_REASON; it must stay byte-identical with the
    // reason the loop actually parks in Failed.
    let mut loop_ctrl = AgentLoop::new(1);
    loop_ctrl
        .transition_to(crate::agent::AgentState::Executing { step: 0 })
        .unwrap();
    loop_ctrl.next_state();
    let state = loop_ctrl.next_state();
    match state {
        Some(crate::agent::AgentState::Failed { reason }) => {
            assert_eq!(reason, MAX_ITERATIONS_STOP_REASON)
        }
        other => panic!("expected the cap trip, got {other:?}"),
    }
}
