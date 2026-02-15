//! Multi-turn conversation integration tests
//!
//! These tests verify the agent's ability to maintain context
//! across multiple turns and handle complex interactions.

use super::helpers::*;
use selfware::api::types::Message;
use selfware::api::ApiClient;
#[allow(unused_imports)]
use std::time::Duration;
use tokio::time::timeout;

// Re-import the macros from the test crate root
use crate::{skip_if_no_model, skip_if_slow};

/// Test that the model maintains context across turns
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_multi_turn_context() {
    let config = test_config();
    skip_if_no_model!(&config);
    skip_if_slow!();

    let client = ApiClient::new(&config).expect("Client should construct");

    // First turn: introduce a fact
    let messages1 = vec![
        Message::system("You are a helpful assistant. Remember what the user tells you."),
        Message::user("My favorite color is blue. Remember this."),
    ];

    let result1 = timeout(
        test_timeout(),
        client.chat(
            messages1.clone(),
            None,
            selfware::api::ThinkingMode::Disabled,
        ),
    )
    .await;

    assert!(result1.is_ok(), "First request should not timeout");
    let response1 = result1.unwrap().expect("First chat should succeed");
    let assistant_msg1 = response1.choices[0].message.clone();

    // Second turn: ask about the fact
    let mut messages2 = messages1;
    messages2.push(Message {
        role: "assistant".to_string(),
        content: assistant_msg1.content,
        reasoning_content: None,
        tool_calls: None,
        tool_call_id: None,
        name: None,
    });
    messages2.push(Message::user("What is my favorite color?"));

    let result2 = timeout(
        test_timeout(),
        client.chat(messages2, None, selfware::api::ThinkingMode::Disabled),
    )
    .await;

    assert!(result2.is_ok(), "Second request should not timeout");
    let response2 = result2.unwrap().expect("Second chat should succeed");

    let content = response2.choices[0].message.content.to_lowercase();
    assert!(
        content.contains("blue"),
        "Model should remember the favorite color was blue: {}",
        content
    );
}

/// Test conversation with tool result injection
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_conversation_with_tool_results() {
    let config = test_config();
    skip_if_no_model!(&config);
    skip_if_slow!();

    let client = ApiClient::new(&config).expect("Client should construct");

    // Simulate a conversation where the model asked for a tool and got a result
    let messages = vec![
        test_system_prompt(),
        Message::user("What does the Cargo.toml say about this project's name?"),
        Message {
            role: "assistant".to_string(),
            content: "<tool><name>file_read</name><arguments>{\"path\": \"./Cargo.toml\"}</arguments></tool>".to_string(),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
        Message::user("<tool_result>{\"content\": \"[package]\\nname = \\\"selfware\\\"\\nversion = \\\"0.1.0\\\"\"}</tool_result>"),
    ];

    let result = timeout(
        test_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Disabled),
    )
    .await;

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap().expect("Chat should succeed");

    let content = response.choices[0].message.content.to_lowercase();
    assert!(
        content.contains("selfware"),
        "Model should reference the project name from tool result: {}",
        content
    );
}

/// Test that the model can follow complex multi-step instructions
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_multi_step_instructions() {
    let config = test_config();
    skip_if_no_model!(&config);
    skip_if_slow!();

    let client = ApiClient::new(&config).expect("Client should construct");

    let messages = vec![
        Message::system("You are a helpful assistant. Follow instructions precisely."),
        Message::user("I'm going to give you three numbers. Add them up and tell me the sum. First number: 10"),
    ];

    let result1 = timeout(
        test_timeout(),
        client.chat(
            messages.clone(),
            None,
            selfware::api::ThinkingMode::Disabled,
        ),
    )
    .await;

    assert!(result1.is_ok(), "Request should not timeout");
    let response1 = result1.unwrap().expect("Chat should succeed");

    // Continue the conversation
    let mut messages2 = messages;
    messages2.push(Message {
        role: "assistant".to_string(),
        content: response1.choices[0].message.content.clone(),
        reasoning_content: None,
        tool_calls: None,
        tool_call_id: None,
        name: None,
    });
    messages2.push(Message::user(
        "Second number: 20. Third number: 30. Now tell me the sum.",
    ));

    let result2 = timeout(
        test_timeout(),
        client.chat(messages2, None, selfware::api::ThinkingMode::Disabled),
    )
    .await;

    assert!(result2.is_ok(), "Request should not timeout");
    let response2 = result2.unwrap().expect("Chat should succeed");

    let content = &response2.choices[0].message.content;
    assert!(
        content.contains("60"),
        "Model should calculate sum as 60: {}",
        content
    );
}

/// Test that error messages are handled gracefully in conversation
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_error_recovery_in_conversation() {
    let config = test_config();
    skip_if_no_model!(&config);
    skip_if_slow!();

    let client = ApiClient::new(&config).expect("Client should construct");

    // Simulate a conversation where a tool failed
    let messages = vec![
        test_system_prompt(),
        Message::user("Read the file at /nonexistent/path/file.txt"),
        Message {
            role: "assistant".to_string(),
            content: "<tool><name>file_read</name><arguments>{\"path\": \"/nonexistent/path/file.txt\"}</arguments></tool>".to_string(),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
        Message::user("<tool_result><error>File not found: /nonexistent/path/file.txt</error></tool_result>"),
    ];

    let result = timeout(
        test_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Disabled),
    )
    .await;

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap().expect("Chat should succeed");

    let content = response.choices[0].message.content.to_lowercase();
    // Model should acknowledge the error or try a different approach
    assert!(
        content.contains("error")
            || content.contains("not found")
            || content.contains("does not exist")
            || content.contains("couldn't")
            || content.contains("unable")
            || content.contains("sorry"),
        "Model should acknowledge or handle the error: {}",
        content
    );
}

/// Test long conversation doesn't break context handling
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_longer_conversation() {
    let config = test_config();
    skip_if_no_model!(&config);
    skip_if_slow!();

    let client = ApiClient::new(&config).expect("Client should construct");

    let mut messages = vec![Message::system("You are a helpful math tutor. Be concise.")];

    // Build up a conversation
    let exchanges = [
        ("What is 5 + 3?", "8"),
        ("Multiply that by 2", "16"),
        ("Subtract 10", "6"),
        ("What number are we at now?", "6"),
    ];

    for (question, expected_answer) in exchanges {
        messages.push(Message::user(question));

        let result = timeout(
            test_timeout(),
            client.chat(
                messages.clone(),
                None,
                selfware::api::ThinkingMode::Disabled,
            ),
        )
        .await;

        if result.is_err() {
            // Timeout is acceptable for slow models
            eprintln!("Conversation turn timed out, which is acceptable");
            return;
        }

        let response = result.unwrap().expect("Chat should succeed");
        let content = response.choices[0].message.content.clone();

        // Add assistant response to context
        messages.push(Message {
            role: "assistant".to_string(),
            content: content.clone(),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });

        assert!(
            content.contains(expected_answer),
            "For '{}', expected answer containing '{}', got: {}",
            question,
            expected_answer,
            content
        );
    }
}

/// Test that the model can handle code in conversation
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_code_in_conversation() {
    let config = test_config();
    skip_if_no_model!(&config);
    skip_if_slow!();

    let client = ApiClient::new(&config).expect("Client should construct");

    let messages = vec![
        Message::system("You are a helpful coding assistant."),
        Message::user(
            "What does this Rust code do?\n```rust\nfn add(a: i32, b: i32) -> i32 { a + b }\n```",
        ),
    ];

    let result = timeout(
        extended_timeout(), // Use extended timeout for code analysis
        client.chat(messages, None, selfware::api::ThinkingMode::Disabled),
    )
    .await;

    assert!(
        result.is_ok(),
        "Request should not timeout (using extended timeout)"
    );
    let response = result.unwrap().expect("Chat should succeed");

    let content = response.choices[0].message.content.to_lowercase();

    // Model should understand the code
    assert!(
        content.contains("add")
            || content.contains("sum")
            || content.contains("return")
            || content.contains("function")
            || content.contains("integers"),
        "Model should explain the addition function: {}",
        content
    );
}
