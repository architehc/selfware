//! Regression: `max_budget_tokens` silently reset across resume. A LEGACY
//! checkpoint persists only the cumulative TOTAL (no input/output split), so
//! `.input`/`.output` restart at 0 on resume; the first
//! `total = input + output` recompute then erased the restored total and the
//! run kept spending past its cap. All recompute sites now delta-add each
//! step's tokens onto the restored total.
//!
//! N7 (0.8.3 validation, runs/b3_resume): current checkpoints also persist the
//! input/output/reasoning split, so a resumed run's counters add up.

use crate::agent::compression::{CompressionMethod, CompressionMetrics};
use crate::agent::Agent;
use crate::config::Config;

#[tokio::test]
async fn budget_total_survives_resume_and_next_recompute() {
    // Prior run: 10_000 tokens billed, then checkpointed.
    let mut prior = Agent::new(Config::default()).await.unwrap();
    prior.cumulative_token_usage.input = 7_000;
    prior.cumulative_token_usage.output = 3_000;
    prior.cumulative_token_usage.total = 10_000;
    let checkpoint = prior.to_checkpoint("budget-resume", "desc");
    assert_eq!(checkpoint.cumulative_tokens, 10_000);

    // A LEGACY resume restores the total alone (no persisted split), exactly
    // like `Agent::resume` does when `cumulative_token_split` is `None`.
    let mut agent = Agent::new(Config::default()).await.unwrap();
    agent.cumulative_token_usage.total = checkpoint.cumulative_tokens;
    assert_eq!(agent.cumulative_token_usage.input, 0);
    assert_eq!(agent.cumulative_token_usage.output, 0);

    // The first billable accounting after resume must ADD to the restored
    // budget — pre-fix this recompute reset total to 150.
    let metrics =
        CompressionMetrics::new(CompressionMethod::Auto, 0, 0, 0, 0, 0).with_llm_tokens(100, 50);
    agent.account_compression_tokens(&metrics);

    assert_eq!(
        agent.cumulative_token_usage.total, 10_150,
        "restored budget total must survive the next recompute"
    );
    assert_eq!(agent.cumulative_token_usage.input, 100);
    assert_eq!(agent.cumulative_token_usage.output, 50);
}

#[tokio::test]
async fn fresh_run_total_still_equals_input_plus_output() {
    // The delta-add change must not regress the normal (non-resume)
    // invariant: total == input + output.
    let mut agent = Agent::new(Config::default()).await.unwrap();
    let metrics =
        CompressionMetrics::new(CompressionMethod::Auto, 0, 0, 0, 0, 0).with_llm_tokens(42, 58);
    agent.account_compression_tokens(&metrics);
    let usage = agent.cumulative_token_usage();
    assert_eq!(usage.input, 42);
    assert_eq!(usage.output, 58);
    assert_eq!(usage.total, 100);
    assert_eq!(usage.total, usage.input + usage.output);
}

/// N7: `Agent::resume` restored only `usage.total`. The resumed result then
/// showed total 1,828,444 against input + output 1,502,325 — short by exactly
/// the first segment's 326,119. Every counter must survive the round trip.
#[tokio::test]
async fn resume_restores_every_usage_counter_and_the_cost() {
    let fake_home = tempfile::tempdir().unwrap();
    let env = crate::test_support::EnvGuard::capture(&["HOME"]);
    env.set("HOME", fake_home.path().as_os_str());

    // The b3_resume first segment's usage.
    let mut prior = Agent::new(Config::default()).await.unwrap();
    prior.cumulative_token_usage.input = 324_918;
    prior.cumulative_token_usage.output = 1_201;
    prior.cumulative_token_usage.total = 326_119;
    prior.cumulative_token_usage.reasoning = Some(400);
    prior.cumulative_cost_usd = 0.25;
    let checkpoint = prior.to_checkpoint("usage-task", "desc");
    assert_eq!(
        checkpoint.cumulative_token_split,
        Some(crate::checkpoint::CumulativeTokenSplit {
            input: 324_918,
            output: 1_201,
            reasoning: Some(400),
        })
    );
    crate::checkpoint::CheckpointManager::new(
        fake_home.path().join(".selfware").join("checkpoints"),
    )
    .unwrap()
    .save_final(&checkpoint)
    .unwrap();

    // A token cap below the first segment's usage: the restored budget
    // floor must already trip it (tokens carried across resume).
    let mut config = Config::default();
    config.agent.max_budget_tokens = Some(326_000);
    let mut agent = Agent::resume(config, "usage-task").await.unwrap();
    assert!(
        agent.client.budget_stop().is_some(),
        "the token budget counts the earlier segment"
    );
    let usage = agent.cumulative_token_usage().clone();
    assert_eq!(usage.input, 324_918);
    assert_eq!(usage.output, 1_201);
    assert_eq!(usage.total, 326_119);
    assert_eq!(usage.reasoning, Some(400));
    assert_eq!(usage.total, usage.input + usage.output, "counters add up");
    assert!(
        (agent.cumulative_cost_usd - 0.25).abs() < 1e-12,
        "cost carried"
    );

    // The next segment's usage keeps them adding up, and the token budget
    // floor still carries the whole chain.
    let metrics =
        CompressionMetrics::new(CompressionMethod::Auto, 0, 0, 0, 0, 0).with_llm_tokens(100, 50);
    agent.account_compression_tokens(&metrics);
    let usage = agent.cumulative_token_usage();
    assert_eq!(usage.total, 326_269);
    assert_eq!(usage.total, usage.input + usage.output);
}

/// N7: the split must ride in incremental (delta) saves too — a resume from a
/// delta-only save must not fall back to the legacy total-only restore.
#[test]
fn token_split_rides_in_checkpoint_deltas() {
    let base = crate::checkpoint::TaskCheckpoint::new("t".to_string(), "d".to_string());
    let mut next = base.clone();
    next.set_step(1);
    next.cumulative_tokens = 150;
    next.cumulative_token_split = Some(crate::checkpoint::CumulativeTokenSplit {
        input: 100,
        output: 50,
        reasoning: None,
    });
    let delta = next.compute_delta(&base).expect("changes produce a delta");
    let mut replayed = base.clone();
    replayed.apply_delta(&delta).unwrap();
    assert_eq!(replayed.cumulative_token_split, next.cumulative_token_split);
    assert_eq!(replayed.cumulative_tokens, 150);
}
