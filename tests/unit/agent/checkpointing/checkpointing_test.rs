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
