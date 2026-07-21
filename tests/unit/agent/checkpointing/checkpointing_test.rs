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
