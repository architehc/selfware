use selfware::agent::Agent;
use selfware::config::Config;

#[tokio::test]
async fn test_agent_creation() {
    let config = Config::default();
    let agent = Agent::new(config).await;
    assert!(agent.is_ok());
}

#[tokio::test]
async fn test_context_compression_integration() {
    // Tests the full compression pipeline
}
