//! Verification credit across a pause: a pass recorded before the checkpoint
//! only counts on resume while the files the task wrote are byte-identical
//! (and HEAD has not moved). Another process editing them while the task was
//! paused must send the completion gate back to StaleVerification.

use crate::agent::Agent;
use crate::api::types::Message;
use crate::checkpoint::{CheckpointManager, TaskCheckpoint, ToolCallLog};
use crate::testing::mock_api::MockLlmServer;
use chrono::Utc;

fn save_under_fake_home(fake_home: &std::path::Path, checkpoint: &TaskCheckpoint) {
    let manager = CheckpointManager::new(fake_home.join(".selfware").join("checkpoints")).unwrap();
    manager.save_final(checkpoint).unwrap();
}

fn directive_mentions_reverification(agent: &Agent) -> bool {
    agent.messages.iter().any(|m| {
        let text = m.content.text_all();
        text.contains("verification passed before the pause") && text.contains("re-run")
    })
}

/// Build a prior segment whose single edit (`written`, absolute) was verified
/// green, and checkpoint it through `to_checkpoint` so the fingerprint is
/// whatever production code records.
async fn verified_prior_segment(endpoint: &str, task_id: &str, written: &str) -> TaskCheckpoint {
    let mut prior = Agent::new(crate::test_support::mock_agent_config(endpoint))
        .await
        .unwrap();
    let mut cp = TaskCheckpoint::new(task_id.to_string(), "fix the bug in lib.rs".to_string());
    cp.set_messages(vec![Message::user("fix the bug in lib.rs".to_string())]);
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_write".to_string(),
        arguments: serde_json::json!({"path": written, "content": "pub fn a() {}"}).to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(1),
    });
    prior.current_checkpoint = Some(cp);
    prior.mutation_sequence = 1;
    prior.last_successful_verification_mutation_sequence = 1;
    let checkpoint = prior.to_checkpoint(task_id, "fix the bug in lib.rs");
    assert!(
        checkpoint.guard_counters.verification_fingerprint.is_some(),
        "a checkpoint carrying verification credit must carry its fingerprint"
    );
    checkpoint
}

#[tokio::test]
async fn file_changed_while_paused_revokes_verification_credit() {
    let fake_home = tempfile::tempdir().unwrap();
    let env = crate::test_support::EnvGuard::capture(&["HOME"]);
    env.set("HOME", fake_home.path().as_os_str());
    let workspace = tempfile::tempdir().unwrap();
    let file = workspace.path().join("lib.rs");
    std::fs::write(&file, "pub fn a() {}").unwrap();

    let server = MockLlmServer::builder().with_response("done").build().await;
    let endpoint = format!("{}/v1", server.url());
    let checkpoint =
        verified_prior_segment(&endpoint, "paused-edit", &file.to_string_lossy()).await;
    save_under_fake_home(fake_home.path(), &checkpoint);

    // Another process edits the verified file while the task is paused.
    std::fs::write(&file, "pub fn a() { unimplemented!() }").unwrap();

    let agent = Agent::resume(
        crate::test_support::mock_agent_config(&endpoint),
        "paused-edit",
    )
    .await
    .unwrap();
    assert_eq!(agent.mutation_sequence, 1);
    assert!(
        agent.last_successful_verification_mutation_sequence < agent.mutation_sequence,
        "the gate's StaleVerification condition must hold: the pass no longer covers the tree"
    );
    assert!(
        agent.fresh_authoritative_pass().is_none(),
        "accept-with-proof must have no pass to build on after revocation"
    );
    assert!(
        directive_mentions_reverification(&agent),
        "the model must be told verification has to be re-run"
    );
    server.stop().await;
}

#[tokio::test]
async fn unchanged_files_keep_verification_credit_across_resume() {
    let fake_home = tempfile::tempdir().unwrap();
    let env = crate::test_support::EnvGuard::capture(&["HOME"]);
    env.set("HOME", fake_home.path().as_os_str());
    let workspace = tempfile::tempdir().unwrap();
    let file = workspace.path().join("lib.rs");
    std::fs::write(&file, "pub fn a() {}").unwrap();

    let server = MockLlmServer::builder().with_response("done").build().await;
    let endpoint = format!("{}/v1", server.url());
    let checkpoint =
        verified_prior_segment(&endpoint, "paused-clean", &file.to_string_lossy()).await;
    save_under_fake_home(fake_home.path(), &checkpoint);

    let agent = Agent::resume(
        crate::test_support::mock_agent_config(&endpoint),
        "paused-clean",
    )
    .await
    .unwrap();
    assert_eq!(agent.mutation_sequence, 1);
    assert_eq!(
        agent.last_successful_verification_mutation_sequence, 1,
        "an untouched workspace keeps the credit"
    );
    assert!(!directive_mentions_reverification(&agent));
    server.stop().await;
}

#[tokio::test]
async fn legacy_checkpoint_without_fingerprint_revokes_credit_conservatively() {
    let fake_home = tempfile::tempdir().unwrap();
    let env = crate::test_support::EnvGuard::capture(&["HOME"]);
    env.set("HOME", fake_home.path().as_os_str());
    let workspace = tempfile::tempdir().unwrap();
    let file = workspace.path().join("lib.rs");
    std::fs::write(&file, "pub fn a() {}").unwrap();

    let server = MockLlmServer::builder().with_response("done").build().await;
    let endpoint = format!("{}/v1", server.url());
    let mut checkpoint =
        verified_prior_segment(&endpoint, "legacy-credit", &file.to_string_lossy()).await;
    // Written before the fingerprint existed.
    checkpoint.guard_counters.verification_fingerprint = None;
    save_under_fake_home(fake_home.path(), &checkpoint);

    let agent = Agent::resume(
        crate::test_support::mock_agent_config(&endpoint),
        "legacy-credit",
    )
    .await
    .unwrap();
    assert_eq!(agent.last_successful_verification_mutation_sequence, 0);
    assert!(directive_mentions_reverification(&agent));
    server.stop().await;
}

#[tokio::test]
async fn fingerprint_survives_delta_only_saves() {
    // The fingerprint rides in `guard_counters`, which the delta carries
    // whole — so a credit earned between full saves still has its
    // fingerprint on the next load.
    let dir = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let file = workspace.path().join("lib.rs");
    std::fs::write(&file, "pub fn a() {}").unwrap();
    let server = MockLlmServer::builder().with_response("done").build().await;
    let endpoint = format!("{}/v1", server.url());

    let mut base = verified_prior_segment(&endpoint, "delta-fp", &file.to_string_lossy()).await;
    let fingerprint = base.guard_counters.verification_fingerprint.take();
    base.guard_counters
        .last_successful_verification_mutation_sequence = 0;
    base.set_messages(
        (0..40)
            .map(|i| Message::user(format!("msg {i} {}", "y".repeat(200))))
            .collect(),
    );
    let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
    manager.save_final(&base).unwrap();

    let mut next = base.clone();
    next.guard_counters
        .last_successful_verification_mutation_sequence = 1;
    next.guard_counters.verification_fingerprint = fingerprint.clone();
    next.set_step(1);
    manager.save(&next).unwrap();
    assert!(dir.path().join("delta-fp.delta.jsonl").exists());

    let loaded = CheckpointManager::new(dir.path().to_path_buf())
        .unwrap()
        .load("delta-fp")
        .unwrap();
    assert_eq!(loaded.guard_counters.verification_fingerprint, fingerprint);
    server.stop().await;
}
