use super::restore_budget_caps_from_checkpoint;
use crate::checkpoint::TaskCheckpoint;
use crate::config::Config;

fn checkpoint_with_caps(
    tokens: Option<usize>,
    secs: Option<u64>,
    cost: Option<f64>,
) -> TaskCheckpoint {
    let mut cp = TaskCheckpoint::new("t".to_string(), "d".to_string());
    cp.max_budget_tokens = tokens;
    cp.max_wall_secs = secs;
    cp.max_cost_usd = cost;
    cp
}

#[test]
fn restores_caps_when_config_has_none() {
    let mut config = Config::default();
    let cp = checkpoint_with_caps(Some(500_000), Some(3600), Some(4.0));
    restore_budget_caps_from_checkpoint(&mut config, &cp);
    assert_eq!(config.agent.max_budget_tokens, Some(500_000));
    assert_eq!(config.agent.max_wall_secs, Some(3600));
    assert_eq!(config.agent.max_cost_usd, Some(4.0));
}

#[test]
fn cli_override_wins_over_persisted_cap() {
    let mut config = Config::default();
    config.agent.max_budget_tokens = Some(10); // as if re-passed on resume
    let cp = checkpoint_with_caps(Some(500_000), None, None);
    restore_budget_caps_from_checkpoint(&mut config, &cp);
    assert_eq!(config.agent.max_budget_tokens, Some(10)); // CLI value kept
}

#[test]
fn no_persisted_caps_leaves_config_uncapped() {
    let mut config = Config::default();
    let cp = checkpoint_with_caps(None, None, None);
    restore_budget_caps_from_checkpoint(&mut config, &cp);
    assert_eq!(config.agent.max_budget_tokens, None);
    assert_eq!(config.agent.max_wall_secs, None);
    assert_eq!(config.agent.max_cost_usd, None);
}

#[tokio::test]
async fn cancellation_bypasses_continuous_checkpoint_cadence() {
    let mut config = crate::test_support::mock_agent_config("http://127.0.0.1:1");
    config.continuous_work.enabled = true;
    config.continuous_work.checkpoint_interval_tools = 1000;
    config.continuous_work.checkpoint_interval_secs = 3600;
    let mut agent = crate::agent::Agent::new(config).await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    agent.checkpoint_manager =
        Some(crate::checkpoint::CheckpointManager::new(directory.path().to_path_buf()).unwrap());
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "cancel-cadence".into(),
        "Review files".into(),
    ));
    agent.save_checkpoint("Review files").unwrap();
    assert!(!agent.should_persist_checkpoint());
    agent.messages.push(crate::api::types::Message::user(
        "latest resumable evidence",
    ));
    agent
        .cancel_token()
        .store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(agent.should_persist_checkpoint());
    agent.save_checkpoint("Review files").unwrap();
    let saved = agent
        .checkpoint_manager
        .as_ref()
        .unwrap()
        .load("cancel-cadence")
        .unwrap();
    assert!(saved
        .messages
        .iter()
        .any(|message| message.content.contains("latest resumable evidence")));
}

/// Review fix: `save_checkpoint_forced` (the auto-continue boundary write)
/// must FAIL with a typed error when no checkpoint manager is configured —
/// reporting Ok there would let a chained run claim "checkpointed" with
/// nothing on disk (honest status, AGENTS.md rule 3). The best-effort
/// periodic `save_checkpoint` keeps its silent no-op: it is not a boundary
/// guarantee, and its cadence/cancel callers are fine with skipping.
/// (The auto-continue consequence — the chain must not fire — is asserted
/// in auto_continue_aborts_without_claim_when_checkpoint_save_fails and
/// forced_checkpoint_without_manager_fails_typed in task_runner_test.rs.)
#[tokio::test]
async fn forced_checkpoint_without_manager_fails_typed() {
    let mut agent =
        crate::agent::Agent::new(crate::test_support::mock_agent_config("http://127.0.0.1:1"))
            .await
            .unwrap();
    agent.checkpoint_manager = None; // Agent::new may have defaulted one
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "forced-no-mgr".into(),
        "Review files".into(),
    ));
    let err = agent
        .save_checkpoint_forced("Review files")
        .expect_err("a forced checkpoint without a manager must be a typed error");
    assert!(
        err.to_string().contains("no checkpoint manager"),
        "the error must name the missing manager, got: {}",
        err
    );

    // The regular periodic save still no-ops, unchanged.
    agent.save_checkpoint("Review files").unwrap();
}

// ---- W8a: checkpoint on every mutation + resume workspace refresh ----

fn w8a_call(tool: &str, args: serde_json::Value, success: bool) -> crate::checkpoint::ToolCallLog {
    crate::checkpoint::ToolCallLog {
        timestamp: chrono::Utc::now(),
        tool_name: tool.to_string(),
        arguments: args.to_string(),
        result: Some("{}".to_string()),
        success,
        duration_ms: Some(1),
    }
}

async fn w8a_agent_with_cadence(task_id: &str) -> (crate::agent::Agent, tempfile::TempDir) {
    let mut config = crate::test_support::mock_agent_config("http://127.0.0.1:1");
    // The old cadence: nothing short of 1000 calls / an hour would persist.
    config.continuous_work.enabled = true;
    config.continuous_work.checkpoint_interval_tools = 1000;
    config.continuous_work.checkpoint_interval_secs = 3600;
    let mut agent = crate::agent::Agent::new(config).await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    agent.checkpoint_manager =
        Some(crate::checkpoint::CheckpointManager::new(directory.path().to_path_buf()).unwrap());
    agent.current_checkpoint = Some(TaskCheckpoint::new(task_id.into(), "Build app".into()));
    agent.save_checkpoint("Build app").unwrap();
    (agent, directory)
}

fn w8a_log(agent: &mut crate::agent::Agent, call: crate::checkpoint::ToolCallLog) {
    agent
        .current_checkpoint
        .as_mut()
        .expect("active checkpoint")
        .log_tool_call(call);
}

fn w8a_loaded(agent: &crate::agent::Agent, task_id: &str) -> TaskCheckpoint {
    agent
        .checkpoint_manager
        .as_ref()
        .unwrap()
        .load(task_id)
        .unwrap()
}

/// A successful mutation is on disk immediately — not after 10 calls / 300 s
/// (the e2e kill at 110 s lost 5 steps and the resumed run rewrote
/// `src/entry.rs` blind).
#[tokio::test]
async fn w8a_successful_mutation_is_persisted_immediately() {
    let (mut agent, _dir) = w8a_agent_with_cadence("w8a-mutation").await;
    assert!(
        !agent.should_persist_checkpoint(),
        "cadence throttles idle saves"
    );

    w8a_log(
        &mut agent,
        w8a_call(
            "file_write",
            serde_json::json!({"path": "src/entry.rs", "content": "fn main() {}"}),
            true,
        ),
    );
    assert!(agent.has_unpersisted_mutation());
    assert!(
        agent.should_persist_checkpoint(),
        "the step-end save must persist a pending mutation regardless of cadence"
    );
    let cadence_mark = agent.last_checkpoint_tool_calls;
    agent.persist_checkpoint_after_mutation();
    assert!(!agent.has_unpersisted_mutation());
    assert_eq!(
        agent.last_checkpoint_tool_calls, cadence_mark,
        "a light persist must not postpone the regular (snapshot-refreshing) save"
    );
    let loaded = w8a_loaded(&agent, "w8a-mutation");
    assert!(
        loaded
            .tool_calls
            .iter()
            .any(|c| c.arguments.contains("src/entry.rs")),
        "the write must be in the persisted checkpoint"
    );
    assert!(
        !agent.should_persist_checkpoint(),
        "after persisting, the cadence throttles again"
    );
}

/// Read-only and FAILED calls do not trigger the per-mutation persist — the
/// cadence still throttles them.
#[tokio::test]
async fn w8a_read_only_and_failed_calls_do_not_persist() {
    let (mut agent, _dir) = w8a_agent_with_cadence("w8a-readonly").await;
    let persisted_calls = w8a_loaded(&agent, "w8a-readonly").tool_calls.len();
    w8a_log(
        &mut agent,
        w8a_call("file_read", serde_json::json!({"path": "src/lib.rs"}), true),
    );
    w8a_log(
        &mut agent,
        w8a_call(
            "file_write",
            serde_json::json!({"path": "src/x.rs", "content": ""}),
            false,
        ),
    );
    assert!(!agent.has_unpersisted_mutation());
    assert!(!agent.should_persist_checkpoint());
    agent.persist_checkpoint_after_mutation();
    assert_eq!(
        w8a_loaded(&agent, "w8a-readonly").tool_calls.len(),
        persisted_calls,
        "nothing new may be written for read-only / failed calls"
    );
}

/// Shell mutations count too (they carry no path list).
#[tokio::test]
async fn w8a_shell_mutation_is_persisted() {
    let (mut agent, _dir) = w8a_agent_with_cadence("w8a-shell").await;
    w8a_log(
        &mut agent,
        w8a_call(
            "shell_exec",
            serde_json::json!({"command": "mkdir -p src && touch src/a.py"}),
            true,
        ),
    );
    assert!(agent.has_unpersisted_mutation());
    agent.persist_checkpoint_after_mutation();
    assert!(w8a_loaded(&agent, "w8a-shell")
        .tool_calls
        .iter()
        .any(|c| c.arguments.contains("touch src/a.py")));
}

/// Agent-level cost of the per-mutation persist (build the checkpoint +
/// cached delta append, no git snapshot) on a ~200 KB conversation. Run with
/// `--nocapture` for the numbers.
#[tokio::test]
async fn w8a_per_mutation_persist_cost_is_measured() {
    const SAVES: u32 = 20;
    let (mut agent, _dir) = w8a_agent_with_cadence("w8a-cost").await;
    let body = "y".repeat(2_000);
    for i in 0..100 {
        agent
            .messages
            .push(crate::api::types::Message::user(format!(
                "turn {i}: {body}"
            )));
    }
    agent.save_checkpoint_forced("Build app").unwrap();
    let started = std::time::Instant::now();
    for i in 0..SAVES {
        w8a_log(
            &mut agent,
            w8a_call(
                "file_write",
                serde_json::json!({"path": format!("src/m{i}.rs"), "content": "x"}),
                true,
            ),
        );
        agent.persist_checkpoint_after_mutation();
    }
    let per_save = started.elapsed() / SAVES;
    eprintln!("W8a per-mutation persist (agent level): {per_save:?}/mutation");
    assert_eq!(
        w8a_loaded(&agent, "w8a-cost")
            .tool_calls
            .iter()
            .filter(|c| c.arguments.contains("src/m"))
            .count(),
        SAVES as usize
    );
}

#[test]
fn w8a_prior_segment_paths_are_deduplicated_successful_writes() {
    let calls = vec![
        w8a_call(
            "file_write",
            serde_json::json!({"path": "src/entry.rs", "content": "a"}),
            true,
        ),
        w8a_call("file_read", serde_json::json!({"path": "src/lib.rs"}), true),
        w8a_call(
            "file_edit",
            serde_json::json!({"path": "src/entry.rs", "old_str": "a", "new_str": "b"}),
            true,
        ),
        w8a_call(
            "file_write",
            serde_json::json!({"path": "src/failed.rs", "content": "a"}),
            false,
        ),
        w8a_call(
            "file_write",
            serde_json::json!({"path": "tests/test_entry.py", "content": "a"}),
            true,
        ),
    ];
    assert_eq!(
        super::prior_segment_written_paths(&calls),
        vec![
            "src/entry.rs".to_string(),
            "tests/test_entry.py".to_string()
        ]
    );
}

#[test]
fn w8a_workspace_refresh_note_names_files_and_demands_reread() {
    assert!(super::workspace_refresh_note(&[]).is_none());
    let note = super::workspace_refresh_note(&["src/entry.rs".to_string()]).unwrap();
    assert!(note.contains("src/entry.rs"));
    assert!(note.contains("Re-read"));
    assert!(note.contains("selfware_system_directive"));

    let many: Vec<String> = (0..45).map(|i| format!("src/f{i}.rs")).collect();
    let note = super::workspace_refresh_note(&many).unwrap();
    assert!(note.contains("src/f29.rs"));
    assert!(!note.contains("src/f30.rs"), "the listing is capped");
    assert!(
        note.contains("15 more"),
        "the overflow is counted, not hidden"
    );
}

#[test]
fn resume_warns_when_the_backend_differs_from_the_checkpoint() {
    // Review (0.9.1): checkpoints recorded no backend identity, so a task
    // started on one model resumed under another with history and cost
    // silently carried over.
    use crate::session::checkpoint::endpoint_identity;
    let mut cp = crate::checkpoint::TaskCheckpoint::new("t".into(), "task".into());
    let mut config = crate::config::Config {
        endpoint: "https://llm.selfware.design/v1".into(),
        model: "qwen38-flash-next".into(),
        ..crate::config::Config::default()
    };
    // Legacy checkpoint (no identity): nothing to compare, no warning.
    assert!(super::backend_mismatch_warning(&cp, &config).is_none());
    cp.run_endpoint = Some(endpoint_identity(&config.endpoint));
    cp.run_model = Some(config.model.clone());
    assert!(super::backend_mismatch_warning(&cp, &config).is_none());
    // Trailing slash is the same endpoint.
    config.endpoint = "https://llm.selfware.design/v1/".into();
    assert!(super::backend_mismatch_warning(&cp, &config).is_none());
    config.model = "glm-5.2".into();
    let w = super::backend_mismatch_warning(&cp, &config).expect("model differs");
    assert!(w.contains("model qwen38-flash-next -> glm-5.2"), "{w}");
    config.endpoint = "https://openrouter.ai/api/v1".into();
    let w = super::backend_mismatch_warning(&cp, &config).expect("both differ");
    assert!(
        w.contains("endpoint https://llm.selfware.design/v1 -> https://openrouter.ai/api/v1"),
        "{w}"
    );
}

#[test]
fn endpoint_identity_never_stores_userinfo_or_query() {
    use crate::session::checkpoint::endpoint_identity;
    let id = endpoint_identity("https://user:s3cret@api.example.com:8443/v1/?key=abc");
    assert_eq!(id, "https://api.example.com:8443/v1");
    assert!(!id.contains("s3cret") && !id.contains("abc"), "{id}");
}
