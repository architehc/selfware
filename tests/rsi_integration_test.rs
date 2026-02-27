use selfware::analysis::vector_store::{EmbeddingBackend, MockEmbeddingProvider};
use selfware::api::ApiClient;
use selfware::cognitive::cognitive_system::CognitiveSystem;
use selfware::config::Config;
use std::sync::Arc;

#[tokio::test]
async fn test_1m_token_context_initialization() {
    let config = Config {
        max_tokens: 1_000_000,
        ..Config::default()
    };

    let api_client = Arc::new(ApiClient::new(&config).unwrap());
    let embedding = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::new(1536)));

    let system = CognitiveSystem::new(&config, api_client, embedding)
        .await
        .unwrap();
    let budget = system.memory.read().await.budget.clone();

    let total = budget.working_memory
        + budget.episodic_memory
        + budget.semantic_memory
        + budget.response_reserve;
    assert!(total >= 1_000_000, "Should have at least 1M token budget");
    assert!(budget.working_memory > 0);
    assert!(budget.semantic_memory > 0);
    assert!(budget.episodic_memory > 0);
}
