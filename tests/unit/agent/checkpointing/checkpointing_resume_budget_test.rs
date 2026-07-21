//! Regression: `max_budget_tokens` silently reset across resume. The
//! checkpoint persists only the cumulative TOTAL (the format has no
//! input/output split), so `.input`/`.output` restart at 0 on resume;
//! the first `total = input + output` recompute then erased the restored
//! total and the run kept spending past its cap. All recompute sites now
//! delta-add each step's tokens onto the restored total.

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

    // Resume restores the total alone (no persisted split), exactly like
    // `Agent::resume` does at checkpointing.rs:105.
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
