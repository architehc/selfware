//! End-to-end integration tests for the agent
//!
//! These tests verify complete agent workflows with the local model.
//! They are designed to be tolerant of slow response times.

use super::helpers::*;
use selfware::agent::Agent;
use selfware::api::types::Message;
use selfware::api::ApiClient;
use tokio::time::timeout;

// Re-import the macros from the test crate root
use crate::{skip_if_no_model, skip_if_slow};

/// Test that the agent can be constructed with valid config
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_agent_construction() {
    let config = test_config();
    skip_if_no_model!(&config);

    let result = Agent::new(config).await;
    assert!(result.is_ok(), "Agent should construct successfully");
}

/// Test simple math question (no tools needed)
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_simple_question_no_tools() {
    let config = test_config();
    skip_if_no_model!(&config);
    skip_if_slow!();

    let client = ApiClient::new(&config).expect("Client should construct");

    let messages = vec![test_system_prompt(), user_message(simple_question())];

    let result = timeout(
        test_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Disabled),
    )
    .await;

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap();
    assert!(response.is_ok(), "Chat should succeed");

    let response = response.unwrap();
    assert!(
        !response.choices.is_empty(),
        "Should have at least one choice"
    );

    let content = &response.choices[0].message.content;
    assert!(content.contains("4"), "Response should contain '4' for 2+2");
}

/// Test that agent can read a file via tool call
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_agent_file_read_task() {
    let config = test_config();
    skip_if_no_model!(&config);
    skip_if_slow!();

    let client = ApiClient::new(&config).expect("Client should construct");

    let messages = vec![test_system_prompt(), user_message(file_read_prompt())];

    let result = timeout(
        test_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Enabled),
    )
    .await;

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap();
    assert!(response.is_ok(), "Chat should succeed");

    let response = response.unwrap();
    let content = &response.choices[0].message.content;

    // The model should either call the tool or reference the package
    let has_tool_call = content.contains("<tool>") || content.contains("file_read");
    let mentions_package = content.to_lowercase().contains("selfware");

    assert!(
        has_tool_call || mentions_package,
        "Response should contain tool call or package info: {}",
        &content[..content.len().min(500)]
    );
}

/// Test that agent can execute shell commands
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_agent_shell_exec_task() {
    let config = test_config();
    skip_if_no_model!(&config);
    skip_if_slow!();

    let client = ApiClient::new(&config).expect("Client should construct");

    let messages = vec![test_system_prompt(), user_message(shell_prompt())];

    let result = timeout(
        extended_timeout(), // Use extended timeout for tool-calling tests
        client.chat(messages, None, selfware::api::ThinkingMode::Enabled),
    )
    .await;

    assert!(
        result.is_ok(),
        "Request should not timeout (using extended timeout)"
    );
    let response = result.unwrap();
    assert!(response.is_ok(), "Chat should succeed");

    let response = response.unwrap();
    let content = &response.choices[0].message.content;

    // The model should either call the tool or give the answer
    let has_tool_call = content.contains("<tool>") || content.contains("shell_exec");
    let has_hello = content.to_lowercase().contains("hello");

    assert!(
        has_tool_call || has_hello,
        "Response should contain tool call or 'hello': {}",
        &content[..content.len().min(500)]
    );
}

/// Test ApiClient health check endpoint
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_client_health() {
    let config = test_config();

    let healthy = check_model_health(&config).await.unwrap_or(false);

    if !healthy {
        eprintln!(
            "Warning: Model endpoint at {} is not healthy",
            config.endpoint
        );
    }
    // This test just logs status, doesn't fail if unhealthy
}

/// Test that reasoning content is returned when thinking mode is enabled
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_thinking_mode_returns_reasoning() {
    let config = test_config();
    skip_if_no_model!(&config);
    skip_if_slow!();

    let client = ApiClient::new(&config).expect("Client should construct");

    let messages = vec![
        test_system_prompt(),
        user_message("Think step by step: what is 15 * 7?"),
    ];

    let result = timeout(
        test_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Enabled),
    )
    .await;

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap();
    assert!(response.is_ok(), "Chat should succeed");

    let response = response.unwrap();
    let choice = &response.choices[0];

    // Model may or may not return reasoning content depending on implementation
    // Just verify we got a response
    assert!(!choice.message.content.is_empty(), "Should have content");

    // If reasoning is present, it should be non-empty
    if let Some(reasoning) = &choice.message.reasoning_content {
        assert!(
            !reasoning.is_empty(),
            "Reasoning content should not be empty if present"
        );
    }
}

/// Test config loading from environment
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_config_from_environment() {
    let config = test_config();

    // Verify defaults work
    assert!(!config.endpoint.is_empty(), "Endpoint should not be empty");
    assert!(!config.model.is_empty(), "Model should not be empty");
    assert!(
        config.agent.step_timeout_secs > 0,
        "Timeout should be positive"
    );
}

/// Test ThinkingMode::Budget works correctly
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_thinking_mode_budget() {
    let config = test_config();
    skip_if_no_model!(&config);
    skip_if_slow!();

    let client = ApiClient::new(&config).expect("Client should construct");

    let messages = vec![
        Message::system("You are a helpful assistant."),
        Message::user("What is 2+2?"),
    ];

    // Use Budget mode with limited thinking tokens
    let result = timeout(
        test_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Budget(512)),
    )
    .await;

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap();
    assert!(response.is_ok(), "Chat with Budget mode should succeed");
}

/// Test token usage is reported correctly
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_token_usage_reported() {
    let config = test_config();
    skip_if_no_model!(&config);
    skip_if_slow!();

    let client = ApiClient::new(&config).expect("Client should construct");

    let messages = vec![
        Message::system("You are a helpful assistant."),
        Message::user("Say 'test' and nothing else."),
    ];

    let result = timeout(
        test_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Disabled),
    )
    .await;

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap();
    assert!(response.is_ok(), "Chat should succeed");

    let response = response.unwrap();

    // Check usage is reported
    assert!(
        response.usage.prompt_tokens > 0,
        "Should have prompt tokens"
    );
    assert!(
        response.usage.completion_tokens > 0,
        "Should have completion tokens"
    );
    assert!(response.usage.total_tokens > 0, "Should have total tokens");
    assert_eq!(
        response.usage.total_tokens,
        response.usage.prompt_tokens + response.usage.completion_tokens,
        "Total should equal prompt + completion"
    );
}
