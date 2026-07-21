use super::*;

#[tokio::test]
async fn test_mock_returns_responses_in_order() {
    let r1 = ChatResponse {
        id: "1".into(),
        object: "chat.completion".into(),
        created: 0,
        model: "mock".into(),
        choices: vec![],
        usage: Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            cost: None,
        },
    };
    let r2 = ChatResponse {
        id: "2".into(),
        ..r1.clone()
    };
    let client = MockLlmClient::with_responses(vec![r1, r2]);
    let first = client
        .chat(vec![], None, ThinkingMode::Enabled)
        .await
        .unwrap();
    assert_eq!(first.id, "1");
    let second = client
        .chat(vec![], None, ThinkingMode::Enabled)
        .await
        .unwrap();
    assert_eq!(second.id, "2");
}

#[tokio::test]
async fn test_mock_errors_when_empty() {
    let client = MockLlmClient::new();
    let result = client.chat(vec![], None, ThinkingMode::Enabled).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_mock_stream_not_supported() {
    let client = MockLlmClient::new();
    let result = client
        .chat_stream(vec![], None, ThinkingMode::Enabled)
        .await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Streaming not supported"));
}
