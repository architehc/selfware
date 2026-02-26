//! Integration tests for the Agent creation and loop control.
//!
//! The `test_agent_creation` test verifies that an Agent can be instantiated
//! with a default Config (requires no live LLM -- only constructs internal state).
//!
//! The `test_context_compression_integration` test is a placeholder for a future
//! test that exercises the full compression pipeline end-to-end against a real
//! model. It remains `#[ignore]` until the necessary mock/stub infrastructure
//! is available or a live LLM backend is provided.

use selfware::agent::Agent;
use selfware::config::Config;

#[tokio::test]
async fn test_agent_creation() {
    let config = Config::default();
    let agent = Agent::new(config).await;
    assert!(agent.is_ok(), "Agent::new with default config should succeed");
}

#[tokio::test]
async fn test_agent_creation_custom_config() {
    // Verify that Agent::new succeeds with non-default configuration values.
    let mut config = Config::default();
    config.agent.max_iterations = 50;
    config.agent.token_budget = 25_000;
    config.max_tokens = 2048;

    let agent = Agent::new(config).await;
    assert!(
        agent.is_ok(),
        "Agent::new with custom agent config should succeed"
    );
}

/// TODO: Implement a real integration test for the context compression pipeline.
///
/// This test should:
/// 1. Create an Agent with a test config pointing at a live or mock LLM endpoint.
/// 2. Feed it a conversation that exceeds the token budget.
/// 3. Verify that the context compressor triggers and reduces the conversation
///    size while preserving essential information.
///
/// Blocked on: either a mock LLM server or a standardized test fixture that
/// can supply canned responses without network access.
#[tokio::test]
#[ignore = "Requires live LLM or mock server; see TODO above for what this should test"]
async fn test_context_compression_integration() {
    unimplemented!("test_context_compression_integration not yet implemented");
}
