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

// ── per-run guard and measurement state across resume ────────────────────

/// A prior segment that wrote `written` once and whose post-edit
/// verification was entirely NOT-RUN (no toolchain on this host): no credit,
/// no failure, only the D9 waiver at mutation #1.
async fn not_run_prior_segment(endpoint: &str, task_id: &str, written: &str) -> Agent {
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
    prior.note_total_tool_call();
    prior.note_mutating_tool_call();
    prior.last_not_run_verification_mutation_sequence = prior.mutation_sequence;
    assert!(prior.only_unrunnable_verification_at_current_revision());
    prior
}

/// Review finding (D9 waiver across resume): the not-run waiver was not in
/// `GuardCounters`, so a resumed run on a host without the toolchain got a
/// bogus StaleVerification refusal demanding a check that cannot run.
/// Rule-5 sibling: the lifetime mutating-call count restarted at 0, so the
/// same resume (outside git) was refused as EmptyDiff.
#[tokio::test]
async fn resume_on_a_toolchain_less_host_keeps_the_not_run_waiver() {
    let fake_home = tempfile::tempdir().unwrap();
    let env = crate::test_support::EnvGuard::capture(&["HOME"]);
    env.set("HOME", fake_home.path().as_os_str());
    let workspace = tempfile::tempdir().unwrap();
    let file = workspace.path().join("lib.rs");
    std::fs::write(&file, "pub fn a() {}").unwrap();

    let server = MockLlmServer::builder().with_response("done").build().await;
    let endpoint = format!("{}/v1", server.url());
    let prior = not_run_prior_segment(&endpoint, "not-run", &file.to_string_lossy()).await;
    save_under_fake_home(
        fake_home.path(),
        &prior.to_checkpoint("not-run", "fix the bug in lib.rs"),
    );

    let mut config = crate::test_support::mock_agent_config(&endpoint);
    config.agent.min_completion_steps = 0;
    let mut agent = Agent::resume(config, "not-run").await.unwrap();
    // The gate diffs the task's own (non-git) workspace, not the process cwd.
    let root = crate::tools::workspace_root::WorkspaceRoot::fixed(workspace.path());
    agent.tools.set_workspace_root(root.clone());
    assert_eq!(agent.mutation_sequence, 1);
    assert_eq!(agent.last_successful_verification_mutation_sequence, 0);
    assert!(
        agent.only_unrunnable_verification_at_current_revision(),
        "the waiver must survive resume"
    );
    assert_eq!(agent.mutating_tool_call_count(), 1);
    let refusal = crate::tools::workspace_root::scope(root, agent.check_completion_gate())
        .await
        .unwrap_or_default();
    assert!(
        !refusal.contains("StaleVerification") && !refusal.contains("EmptyDiff"),
        "bogus refusal after resume: {refusal}"
    );
    server.stop().await;
}

/// Rule-5 sweep: every per-run guard counter that decides a gate or an
/// abort survives resume, and a checkpoint written before these fields
/// existed still loads (serde defaults; the mutating-call count then comes
/// from the persisted tool log).
#[tokio::test]
async fn per_run_guard_counters_survive_resume() {
    let fake_home = tempfile::tempdir().unwrap();
    let env = crate::test_support::EnvGuard::capture(&["HOME"]);
    env.set("HOME", fake_home.path().as_os_str());
    let workspace = tempfile::tempdir().unwrap();
    let file = workspace.path().join("lib.rs");
    std::fs::write(&file, "pub fn a() {}").unwrap();
    let server = MockLlmServer::builder().with_response("done").build().await;
    let endpoint = format!("{}/v1", server.url());

    let mut prior = not_run_prior_segment(&endpoint, "guards", &file.to_string_lossy()).await;
    for _ in 0..4 {
        prior.note_total_tool_call();
    }
    prior.consecutive_stale_verification = 2;
    prior.progress_guard_fire_count = 3;
    prior.rigor_mode = true;
    prior.prefill_400_count = 3;
    prior.citation_gate.lock().unwrap().rejections = 1;
    let checkpoint = prior.to_checkpoint("guards", "fix the bug in lib.rs");
    save_under_fake_home(fake_home.path(), &checkpoint);

    let agent = Agent::resume(crate::test_support::mock_agent_config(&endpoint), "guards")
        .await
        .unwrap();
    assert_eq!(agent.mutating_tool_call_count(), 1);
    assert_eq!(agent.total_tool_call_count, 5);
    assert_eq!(agent.consecutive_stale_verification, 2);
    assert_eq!(agent.progress_guard_fire_count(), 3);
    assert!(agent.rigor_mode, "careful mode survives resume");
    assert!(
        agent.prefill_breaker_open(),
        "the breaker the count tripped"
    );
    assert_eq!(agent.citation_gate.lock().unwrap().rejections, 1);

    // Legacy: a checkpoint without the new fields.
    let mut json = serde_json::to_value(&checkpoint).unwrap();
    let gc = json["guard_counters"].as_object_mut().unwrap();
    gc.retain(|k, _| {
        [
            "consecutive_no_action_prompts",
            "mutation_gate_rejections",
            "prefill_400_count",
            "mutation_sequence",
            "last_successful_verification_mutation_sequence",
            "last_failed_verification_mutation_sequence",
            "last_failed_verification_summary",
            "verification_failures",
            "verification_fingerprint",
        ]
        .contains(&k.as_str())
    });
    json["task_id"] = serde_json::json!("guards-legacy");
    let legacy: TaskCheckpoint = serde_json::from_value(json).expect("legacy loads");
    save_under_fake_home(fake_home.path(), &legacy);
    let agent = Agent::resume(
        crate::test_support::mock_agent_config(&endpoint),
        "guards-legacy",
    )
    .await
    .unwrap();
    assert_eq!(
        agent.mutating_tool_call_count(),
        1,
        "legacy: counted from the persisted tool log"
    );
    assert_eq!(agent.consecutive_stale_verification, 0);
    assert!(!agent.rigor_mode);
    server.stop().await;
}

/// Review finding (minor): measured call shapes lived in memory only, so a
/// resumed run re-forecast the wrap-up from the fallbacks. The forecast
/// after resume uses the persisted measurements.
#[tokio::test]
async fn forecast_after_resume_uses_the_persisted_measurements() {
    let fake_home = tempfile::tempdir().unwrap();
    let env = crate::test_support::EnvGuard::capture(&["HOME"]);
    env.set("HOME", fake_home.path().as_os_str());
    let server = MockLlmServer::builder().with_response("done").build().await;
    let endpoint = format!("{}/v1", server.url());

    let prior = Agent::new(crate::test_support::mock_agent_config(&endpoint))
        .await
        .unwrap();
    // b2_350000's slowest long call and a short prefill call, plus a draft
    // above the floor.
    prior.client.record_call_shape(120_000, 150, 30_000);
    prior.client.record_call_shape(153_934, 3_495, 231_163);
    prior.wrap_up.lock().unwrap().draft_completion_tokens = Some(8_000);
    let before = prior.call_forecast();
    let fallback = crate::agent::call_forecast::CallForecast::from_calls(&[], None);
    assert_ne!(before, fallback);
    save_under_fake_home(fake_home.path(), &prior.to_checkpoint("fc", "review src"));

    let agent = Agent::resume(crate::test_support::mock_agent_config(&endpoint), "fc")
        .await
        .unwrap();
    assert_eq!(agent.call_forecast(), before);
    assert_eq!(agent.client.call_shapes().len(), 2);
    server.stop().await;
}

/// The persisted call shapes are bounded, and the bound keeps the slowest
/// decode sample (the forecast's decode rate) however old it is.
#[tokio::test]
async fn persisted_call_shapes_are_bounded_and_keep_the_slowest_decode_sample() {
    let prior = Agent::new(crate::config::Config::default()).await.unwrap();
    prior.client.record_call_shape(153_934, 3_495, 231_163); // 15.1 tok/s
    for i in 0..2_000u64 {
        prior.client.record_call_shape(30_000 + i, 200, 10_000);
    }
    prior.client.record_call_shape(40_000, 3_000, 60_000); // 50 tok/s
    let before = prior.call_forecast();
    let cp = prior.to_checkpoint("bounded", "review src");
    let shapes = &cp.guard_counters.forecast.call_shapes;
    assert!(
        shapes.len() <= crate::checkpoint::FORECAST_CALL_SHAPES_MAX,
        "{}",
        shapes.len()
    );
    let after = crate::agent::call_forecast::CallForecast::from_calls(
        shapes,
        cp.guard_counters.forecast.draft_completion_tokens,
    );
    assert_eq!(after.decode_tok_per_sec, before.decode_tok_per_sec);
    assert_eq!(after.prompt_tokens, before.prompt_tokens);
}
