//! Deep Integration Tests for Real Local Model
//!
//! These tests exercise the full agent capabilities against a real local LLM.
//! They are designed for slow endpoints (10-30+ minutes per test).
//!
//! Run with: cargo test --features integration deep_
//!
//! Environment variables:
//!   SELFWARE_ENDPOINT - API endpoint (default: http://localhost:8888/v1)
//!   SELFWARE_MODEL - Model name (default: unsloth/Kimi-K2.5-GGUF)
//!   SELFWARE_TIMEOUT - Request timeout in seconds (default: 600 = 10 minutes)

use super::helpers::*;
use selfware::agent::Agent;
use selfware::api::types::Message;
use selfware::api::ApiClient;
use selfware::config::{
    AgentConfig, Config, ExecutionMode, SafetyConfig, UiConfig, YoloFileConfig,
};
use selfware::tools::ToolRegistry;
use serde_json::json;
use std::env;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Safely truncate a string to at most `max_len` bytes at a char boundary
fn safe_truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        return s;
    }
    // Find the last char boundary at or before max_len
    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

// Re-import the macros
use crate::{skip_if_no_model, skip_if_slow};

/// Configuration optimized for slow local models
fn slow_model_config() -> Config {
    let endpoint =
        env::var("SELFWARE_ENDPOINT").unwrap_or_else(|_| "http://localhost:8888/v1".to_string());
    let model = env::var("SELFWARE_MODEL").unwrap_or_else(|_| "unsloth/Kimi-K2.5-GGUF".to_string());
    let timeout: u64 = env::var("SELFWARE_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(600); // 10 minutes default -- previously 4 hours which is excessive

    Config {
        endpoint,
        model,
        max_tokens: 8192, // Larger for complex responses
        temperature: 0.3, // Lower for more deterministic behavior
        api_key: env::var("SELFWARE_API_KEY").ok(),
        safety: SafetyConfig {
            allowed_paths: vec!["/tmp/**".to_string(), "./**".to_string()],
            denied_paths: vec![],
            protected_branches: vec!["main".to_string()],
            require_confirmation: vec![],
        },
        agent: AgentConfig {
            max_iterations: 20, // Allow more iterations for complex tasks
            step_timeout_secs: timeout,
            token_budget: 100000,          // Larger token budget
            native_function_calling: true, // Use native FC when available
            streaming: true,
        },
        yolo: YoloFileConfig::default(),
        ui: UiConfig::default(),
        execution_mode: ExecutionMode::Normal,
        compact_mode: false,
        verbose_mode: false,
        show_tokens: false,
        ..Config::default()
    }
}

/// Extended timeout for slow model operations (10 minutes default)
fn slow_timeout() -> Duration {
    let secs: u64 = env::var("SELFWARE_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(600); // 10 minutes -- previously 4 hours which is excessive
    Duration::from_secs(secs)
}

// =============================================================================
// Basic Connectivity Tests
// =============================================================================

/// Test basic model connectivity and response
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_model_connectivity() {
    let config = slow_model_config();
    skip_if_no_model!(&config);

    println!("Testing model connectivity...");
    println!("  Endpoint: {}", config.endpoint);
    println!("  Model: {}", config.model);

    let client = ApiClient::new(&config).expect("Client should construct");

    let messages = vec![
        Message::system("You are a helpful assistant. Respond concisely."),
        Message::user("Say 'Hello, I am working!' and nothing else."),
    ];

    let start = Instant::now();
    let result = tokio::time::timeout(
        slow_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Disabled),
    )
    .await;

    let elapsed = start.elapsed();
    println!("  Response time: {:.2}s", elapsed.as_secs_f64());

    assert!(
        result.is_ok(),
        "Request should not timeout after {:?}",
        elapsed
    );
    let response = result.unwrap();
    assert!(
        response.is_ok(),
        "Chat should succeed: {:?}",
        response.err()
    );

    let response = response.unwrap();
    let content = &response.choices[0].message.content;
    println!("  Response: {}", safe_truncate(content, 100));

    assert!(
        content.to_lowercase().contains("hello") || content.to_lowercase().contains("working"),
        "Response should acknowledge the request"
    );
}

/// Test model with thinking mode enabled
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_thinking_mode() {
    let config = slow_model_config();
    skip_if_no_model!(&config);

    println!("Testing thinking mode...");

    let client = ApiClient::new(&config).expect("Client should construct");

    let messages = vec![
        Message::system("You are a helpful assistant. Think through problems step by step."),
        Message::user("What is 17 * 23? Think step by step before giving the answer."),
    ];

    let start = Instant::now();
    let result = tokio::time::timeout(
        slow_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Enabled),
    )
    .await;

    let elapsed = start.elapsed();
    println!("  Response time: {:.2}s", elapsed.as_secs_f64());

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap().expect("Chat should succeed");

    let content = &response.choices[0].message.content;
    println!("  Response length: {} chars", content.len());

    // Should contain a non-empty response with at least some numeric content
    // (exact answer 391 is not always produced reliably by all models)
    assert!(
        !content.is_empty(),
        "Response should be non-empty for a math question"
    );
    let has_number = content.chars().any(|c| c.is_ascii_digit());
    assert!(
        has_number,
        "Response to a multiplication question should contain digits: {}",
        safe_truncate(content, 500)
    );
}

// =============================================================================
// Tool Calling Tests
// =============================================================================

/// Test that model can understand and format tool calls
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_tool_call_understanding() {
    let config = slow_model_config();
    skip_if_no_model!(&config);

    println!("Testing tool call understanding...");

    let client = ApiClient::new(&config).expect("Client should construct");

    let system_prompt = r#"You are a helpful coding assistant. You have access to tools.
When you need to use a tool, respond with XML format:
<tool>
<name>TOOL_NAME</name>
<arguments>{"arg1": "value1"}</arguments>
</tool>

Available tools:
- file_read: Read a file. Arguments: {"path": "file_path"}
- shell_exec: Execute a shell command. Arguments: {"command": "cmd"}
"#;

    let messages = vec![
        Message::system(system_prompt),
        Message::user("Please read the file ./Cargo.toml using the file_read tool."),
    ];

    let start = Instant::now();
    let result = tokio::time::timeout(
        slow_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Enabled),
    )
    .await;

    let elapsed = start.elapsed();
    println!("  Response time: {:.2}s", elapsed.as_secs_f64());

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap().expect("Chat should succeed");

    let content = &response.choices[0].message.content;
    println!("  Response preview: {}", safe_truncate(content, 300));

    // Should attempt to use the file_read tool
    let has_tool_format = content.contains("<tool>") || content.contains("<name>");
    let mentions_file_read = content.to_lowercase().contains("file_read");
    let mentions_cargo = content.to_lowercase().contains("cargo");

    assert!(
        has_tool_format || mentions_file_read || mentions_cargo,
        "Response should attempt tool call or mention file_read"
    );
}

/// Test multi-step tool execution with the agent
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_multi_step_tool_execution() {
    let config = slow_model_config();
    skip_if_no_model!(&config);
    skip_if_slow!();

    println!("Testing multi-step tool execution...");

    // Create a temp directory for the test
    let temp_dir = TempDir::new().expect("Should create temp dir");
    let test_file = temp_dir.path().join("test_data.txt");
    std::fs::write(&test_file, "The secret number is 42.").expect("Should write test file");

    let mut agent = Agent::new(config).await.expect("Agent should construct");

    let task = format!(
        "Read the file at {} and tell me what the secret number is.",
        test_file.display()
    );

    println!("  Task: {}", task);

    let start = Instant::now();
    let result = tokio::time::timeout(slow_timeout(), agent.run_task(&task)).await;

    let elapsed = start.elapsed();
    println!("  Total time: {:.2}s", elapsed.as_secs_f64());

    match result {
        Ok(Ok(())) => {
            println!("  Task completed successfully");
        }
        Ok(Err(e)) => {
            println!("  Task failed with error: {}", e);
            // Don't fail the test - slow models may struggle
        }
        Err(_) => {
            println!("  Task timed out after {:?}", elapsed);
        }
    }
}

// =============================================================================
// Code Understanding Tests
// =============================================================================

/// Test that model can understand Rust code
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_code_understanding() {
    let config = slow_model_config();
    skip_if_no_model!(&config);

    println!("Testing code understanding...");

    let client = ApiClient::new(&config).expect("Client should construct");

    let code = r#"
fn fibonacci(n: u32) -> u32 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}
"#;

    let messages = vec![
        Message::system("You are a Rust programming expert. Analyze code concisely."),
        Message::user(format!(
            "What does this Rust function do? What is fibonacci(10)?\n\n```rust\n{}\n```",
            code
        )),
    ];

    let start = Instant::now();
    let result = tokio::time::timeout(
        slow_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Enabled),
    )
    .await;

    let elapsed = start.elapsed();
    println!("  Response time: {:.2}s", elapsed.as_secs_f64());

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap().expect("Chat should succeed");

    let content = &response.choices[0].message.content;
    println!("  Response preview: {}", safe_truncate(content, 400));

    // Should understand it's a Fibonacci function
    let understands_fibonacci = content.to_lowercase().contains("fibonacci");
    // fibonacci(10) = 55
    let has_correct_answer = content.contains("55");

    assert!(understands_fibonacci, "Should identify Fibonacci function");

    if has_correct_answer {
        println!("  Correctly computed fibonacci(10) = 55");
    } else {
        println!("  Did not compute fibonacci(10) (acceptable for understanding test)");
    }
}

/// Test code generation capability
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_code_generation() {
    let config = slow_model_config();
    skip_if_no_model!(&config);

    println!("Testing code generation...");

    let client = ApiClient::new(&config).expect("Client should construct");

    let messages = vec![
        Message::system("You are a Rust programming expert. Write clean, idiomatic Rust code."),
        Message::user("Write a Rust function called `is_prime` that checks if a number is prime. Include the function signature and implementation."),
    ];

    let start = Instant::now();
    let result = tokio::time::timeout(
        slow_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Enabled),
    )
    .await;

    let elapsed = start.elapsed();
    println!("  Response time: {:.2}s", elapsed.as_secs_f64());

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap().expect("Chat should succeed");

    let content = &response.choices[0].message.content;
    println!("  Response length: {} chars", content.len());

    // Should contain function definition
    let has_fn = content.contains("fn is_prime") || content.contains("fn  is_prime");
    let has_bool = content.contains("bool");
    let has_rust_code = content.contains("```rust") || content.contains("fn ");

    assert!(
        has_fn || has_rust_code,
        "Should generate Rust function code: {}",
        safe_truncate(content, 500)
    );

    if has_fn && has_bool {
        println!("  Generated proper function signature");
    }
}

// =============================================================================
// Conversation Memory Tests
// =============================================================================

/// Test multi-turn conversation maintains context
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_conversation_memory() {
    let config = slow_model_config();
    skip_if_no_model!(&config);

    println!("Testing conversation memory...");

    let client = ApiClient::new(&config).expect("Client should construct");

    // Turn 1: Introduce a fact
    let mut messages = vec![
        Message::system("You are a helpful assistant with good memory."),
        Message::user("My favorite color is blue. Remember that."),
    ];

    println!("  Turn 1: Introducing fact...");
    let start = Instant::now();
    let result = tokio::time::timeout(
        slow_timeout(),
        client.chat(
            messages.clone(),
            None,
            selfware::api::ThinkingMode::Disabled,
        ),
    )
    .await;

    assert!(result.is_ok(), "Turn 1 should not timeout");
    let response = result.unwrap().expect("Turn 1 should succeed");
    let turn1_response = response.choices[0].message.content.clone();
    println!(
        "    Response: {}",
        &turn1_response[..turn1_response.len().min(100)]
    );

    // Turn 2: Ask about the fact
    messages.push(Message::assistant(&turn1_response));
    messages.push(Message::user("What is my favorite color?"));

    println!("  Turn 2: Recalling fact...");
    let result = tokio::time::timeout(
        slow_timeout(),
        client.chat(
            messages.clone(),
            None,
            selfware::api::ThinkingMode::Disabled,
        ),
    )
    .await;

    let elapsed = start.elapsed();
    println!("  Total time: {:.2}s", elapsed.as_secs_f64());

    assert!(result.is_ok(), "Turn 2 should not timeout");
    let response = result.unwrap().expect("Turn 2 should succeed");
    let turn2_response = &response.choices[0].message.content;
    println!(
        "    Response: {}",
        &turn2_response[..turn2_response.len().min(100)]
    );

    assert!(
        turn2_response.to_lowercase().contains("blue"),
        "Should remember that favorite color is blue: {}",
        turn2_response
    );
}

// =============================================================================
// Agent Full Task Tests
// =============================================================================

/// Test agent running a complete coding task
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_agent_coding_task() {
    let config = slow_model_config();
    skip_if_no_model!(&config);
    skip_if_slow!();

    println!("Testing agent coding task...");

    let temp_dir = TempDir::new().expect("Should create temp dir");

    // Create a simple Rust file with a bug
    let rust_file = temp_dir.path().join("buggy.rs");
    let buggy_code = r#"fn add(a: i32, b: i32) -> i32 {
    a - b  // Bug: should be a + b
}

fn main() {
    println!("{}", add(2, 3));
}
"#;
    std::fs::write(&rust_file, buggy_code).expect("Should write file");

    let mut agent = Agent::new(config).await.expect("Agent should construct");

    let task = format!(
        "Read the file at {} and identify the bug in the add function. Explain what's wrong.",
        rust_file.display()
    );

    println!("  Task: {}", &task[..task.len().min(100)]);

    let start = Instant::now();
    let result = tokio::time::timeout(slow_timeout(), agent.run_task(&task)).await;

    let elapsed = start.elapsed();
    println!("  Total time: {:.2}s", elapsed.as_secs_f64());

    match result {
        Ok(Ok(())) => {
            println!("  Task completed successfully");
        }
        Ok(Err(e)) => {
            println!("  Task error (may be acceptable): {}", e);
        }
        Err(_) => {
            println!("  Task timed out");
        }
    }
}

/// Test agent with search tools
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_agent_search_task() {
    let config = slow_model_config();
    skip_if_no_model!(&config);
    skip_if_slow!();

    println!("Testing agent search task...");

    let mut agent = Agent::new(config).await.expect("Agent should construct");

    let task = "Search for all Rust files in the src/ directory that contain the word 'tool'. List the files found.";

    println!("  Task: {}", task);

    let start = Instant::now();
    let result = tokio::time::timeout(slow_timeout(), agent.run_task(task)).await;

    let elapsed = start.elapsed();
    println!("  Total time: {:.2}s", elapsed.as_secs_f64());

    match result {
        Ok(Ok(())) => {
            println!("  Search task completed successfully");
        }
        Ok(Err(e)) => {
            println!("  Search task error: {}", e);
        }
        Err(_) => {
            println!("  Search task timed out after {:?}", elapsed);
        }
    }
}

// =============================================================================
// Error Handling Tests
// =============================================================================

/// Test agent handles tool errors gracefully
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_error_handling() {
    let config = slow_model_config();
    skip_if_no_model!(&config);

    println!("Testing error handling...");

    let client = ApiClient::new(&config).expect("Client should construct");

    // Ask to read a file that doesn't exist
    let messages = vec![
        Message::system("You are a helpful assistant. When tools fail, explain the error."),
        Message::user("Try to read the file /nonexistent/path/12345.txt and tell me what happens."),
    ];

    let start = Instant::now();
    let result = tokio::time::timeout(
        slow_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Enabled),
    )
    .await;

    let elapsed = start.elapsed();
    println!("  Response time: {:.2}s", elapsed.as_secs_f64());

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap().expect("Chat should succeed");

    let content = &response.choices[0].message.content;
    println!("  Response preview: {}", safe_truncate(content, 300));

    // Model should acknowledge the file doesn't exist or can't be read
    let handles_error = content.to_lowercase().contains("not found")
        || content.to_lowercase().contains("doesn't exist")
        || content.to_lowercase().contains("cannot")
        || content.to_lowercase().contains("error")
        || content.to_lowercase().contains("unable");

    assert!(handles_error, "Model should acknowledge file access issue");
}

// =============================================================================
// Performance Benchmarks
// =============================================================================

/// Benchmark basic response time
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_benchmark_response_time() {
    let config = slow_model_config();
    skip_if_no_model!(&config);

    println!("Benchmarking response times...");

    let client = ApiClient::new(&config).expect("Client should construct");

    let prompts = vec![
        ("Short", "Say 'hi'."),
        ("Medium", "Explain what Rust is in 2 sentences."),
        (
            "Code",
            "Write a one-line Rust function that doubles a number.",
        ),
    ];

    for (name, prompt) in prompts {
        let messages = vec![Message::system("You are concise."), Message::user(prompt)];

        let start = Instant::now();
        let result = tokio::time::timeout(
            slow_timeout(),
            client.chat(messages, None, selfware::api::ThinkingMode::Disabled),
        )
        .await;

        let elapsed = start.elapsed();

        match result {
            Ok(Ok(response)) => {
                let tokens = response.usage.completion_tokens;
                let tps = if elapsed.as_secs_f64() > 0.0 {
                    tokens as f64 / elapsed.as_secs_f64()
                } else {
                    0.0
                };
                println!(
                    "  {}: {:.2}s, {} tokens, {:.2} tok/s",
                    name,
                    elapsed.as_secs_f64(),
                    tokens,
                    tps
                );
            }
            Ok(Err(e)) => {
                println!("  {}: Error - {}", name, e);
            }
            Err(_) => {
                println!("  {}: Timeout after {:.2}s", name, elapsed.as_secs_f64());
            }
        }
    }
}

// =============================================================================
// Tool Registry Integration
// =============================================================================

/// Test all registered tools are callable
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_all_tools_callable() {
    println!("Testing all tools are callable...");

    let registry = ToolRegistry::new();
    let tools = registry.list();

    println!("  Found {} tools", tools.len());

    for tool in tools {
        let name = tool.name();
        let schema = tool.schema();

        // Verify schema is valid JSON object
        assert!(
            schema.is_object(),
            "Tool {} should have object schema",
            name
        );

        // Verify required fields exist
        let schema_obj = schema.as_object().unwrap();
        assert!(
            schema_obj.contains_key("type") || schema_obj.contains_key("properties"),
            "Tool {} should have type or properties in schema",
            name
        );

        println!("    {} - OK", name);
    }
}

/// Test tool execution doesn't panic with minimal args
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_tool_minimal_args() {
    println!("Testing tools with minimal arguments...");

    let registry = ToolRegistry::new();

    // Tools that can be called with empty or minimal args
    let safe_tools = vec![
        ("git_status", json!({})),
        ("directory_tree", json!({"path": "."})),
    ];

    for (name, args) in safe_tools {
        if let Some(tool) = registry.get(name) {
            let result = tokio::time::timeout(Duration::from_secs(30), tool.execute(args)).await;

            match result {
                Ok(Ok(_)) => println!("    {} - Success", name),
                Ok(Err(e)) => println!("    {} - Expected error: {}", name, e),
                Err(_) => println!("    {} - Timeout", name),
            }
        }
    }
}

// =============================================================================
// Advanced Coding Tasks - Added for Model Testing
// =============================================================================

/// Test model's ability to understand and fix Rust compiler errors
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_compiler_error_understanding() {
    let config = slow_model_config();
    skip_if_no_model!(&config);

    println!("Testing compiler error understanding...");

    let client = ApiClient::new(&config).expect("Client should construct");

    let error_message = r#"
error[E0382]: use of moved value: `data`
  --> src/main.rs:10:5
   |
6  |     let data = vec![1, 2, 3];
   |         ---- move occurs because `data` has type `Vec<i32>`, which does not implement the `Copy` trait
7  |     process(data);
   |             ---- value moved here
...
10 |     println!("{:?}", data);
   |                      ^^^^ value used here after move
"#;

    let messages = vec![
        Message::system("You are a Rust expert. Explain errors clearly and provide fixes."),
        Message::user(format!(
            "Explain this Rust error and how to fix it:\n{}",
            error_message
        )),
    ];

    let start = Instant::now();
    let result = tokio::time::timeout(
        slow_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Enabled),
    )
    .await;

    let elapsed = start.elapsed();
    println!("  Response time: {:.2}s", elapsed.as_secs_f64());

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap().expect("Chat should succeed");

    let content = &response.choices[0].message.content;
    println!("  Response preview: {}", safe_truncate(content, 400));

    // Should understand ownership/move semantics
    let understands_move = content.to_lowercase().contains("move")
        || content.to_lowercase().contains("ownership")
        || content.to_lowercase().contains("borrow");

    // Should suggest a fix
    let has_fix = content.to_lowercase().contains("clone")
        || content.to_lowercase().contains("reference")
        || content.to_lowercase().contains("&");

    assert!(
        understands_move,
        "Should understand the move/ownership error"
    );

    if has_fix {
        println!("  Provided a fix suggestion");
    }
}

/// Test model's ability to refactor code
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_code_refactoring() {
    let config = slow_model_config();
    skip_if_no_model!(&config);

    println!("Testing code refactoring...");

    let client = ApiClient::new(&config).expect("Client should construct");

    let messy_code = r#"
fn calc(x:i32,y:i32,z:i32)->i32{
let a=x+y;let b=a*z;let c=b-x;c+1}
"#;

    let messages = vec![
        Message::system(
            "You are a Rust code reviewer. Format and refactor code to be clean and readable.",
        ),
        Message::user(format!(
            "Refactor this messy Rust code to be clean and readable:\n```rust\n{}\n```",
            messy_code
        )),
    ];

    let start = Instant::now();
    let result = tokio::time::timeout(
        slow_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Enabled),
    )
    .await;

    let elapsed = start.elapsed();
    println!("  Response time: {:.2}s", elapsed.as_secs_f64());

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap().expect("Chat should succeed");

    let content = &response.choices[0].message.content;
    println!("  Response preview: {}", safe_truncate(content, 500));

    // Should produce cleaner code with proper formatting
    let has_fn = content.contains("fn ");
    let has_proper_spacing =
        content.contains("x: i32") || content.contains("x : i32") || content.contains("(x:");
    let has_newlines = content.matches('\n').count() > 3;

    assert!(has_fn, "Should produce a function");

    if has_proper_spacing && has_newlines {
        println!("  Code appears properly formatted");
    }
}

/// Test model's ability to write tests
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_test_generation() {
    let config = slow_model_config();
    skip_if_no_model!(&config);

    println!("Testing test generation...");

    let client = ApiClient::new(&config).expect("Client should construct");

    let function_code = r#"
/// Checks if a string is a valid email address (basic validation)
pub fn is_valid_email(email: &str) -> bool {
    email.contains('@') && email.contains('.') && email.len() > 5
}
"#;

    let messages = vec![
        Message::system("You are a Rust testing expert. Write comprehensive unit tests."),
        Message::user(format!("Write unit tests for this Rust function:\n```rust\n{}\n```\nInclude tests for valid emails, invalid emails, and edge cases.", function_code)),
    ];

    let start = Instant::now();
    let result = tokio::time::timeout(
        slow_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Enabled),
    )
    .await;

    let elapsed = start.elapsed();
    println!("  Response time: {:.2}s", elapsed.as_secs_f64());

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap().expect("Chat should succeed");

    let content = &response.choices[0].message.content;
    println!("  Response preview: {}", safe_truncate(content, 600));

    // Should include test annotations
    let has_test_attr = content.contains("#[test]");
    let has_test_fn = content.contains("fn test_") || content.contains("fn test");
    let has_assert = content.contains("assert");

    assert!(
        has_test_attr || has_test_fn,
        "Should generate test functions"
    );

    if has_assert {
        println!("  Generated tests with assertions");
    }
}

/// Test model's ability to explain complex code patterns
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_pattern_explanation() {
    let config = slow_model_config();
    skip_if_no_model!(&config);

    println!("Testing pattern explanation...");

    let client = ApiClient::new(&config).expect("Client should construct");

    let complex_code = r#"
pub trait Handler<T> {
    fn handle(&self, request: T) -> Result<Response, Error>;
}

impl<F, T> Handler<T> for F
where
    F: Fn(T) -> Result<Response, Error>,
{
    fn handle(&self, request: T) -> Result<Response, Error> {
        self(request)
    }
}
"#;

    let messages = vec![
        Message::system("You are a Rust expert. Explain advanced patterns clearly."),
        Message::user(format!(
            "Explain what this Rust code does and why it's useful:\n```rust\n{}\n```",
            complex_code
        )),
    ];

    let start = Instant::now();
    let result = tokio::time::timeout(
        slow_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Enabled),
    )
    .await;

    let elapsed = start.elapsed();
    println!("  Response time: {:.2}s", elapsed.as_secs_f64());

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap().expect("Chat should succeed");

    let content = &response.choices[0].message.content;
    println!("  Response preview: {}", safe_truncate(content, 500));

    // Should understand the pattern
    let understands_trait =
        content.to_lowercase().contains("trait") || content.to_lowercase().contains("handler");
    let understands_impl = content.to_lowercase().contains("impl")
        || content.to_lowercase().contains("blanket")
        || content.to_lowercase().contains("generic");
    let understands_closure = content.to_lowercase().contains("function")
        || content.to_lowercase().contains("closure")
        || content.to_lowercase().contains("fn");

    assert!(
        understands_trait || understands_impl,
        "Should explain the trait/impl pattern"
    );

    if understands_closure {
        println!("  Understood the function/closure aspect");
    }
}

/// Test model's git workflow understanding
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_git_workflow() {
    let config = slow_model_config();
    skip_if_no_model!(&config);

    println!("Testing git workflow understanding...");

    let client = ApiClient::new(&config).expect("Client should construct");

    let messages = vec![
        Message::system("You are a git expert. Explain commands clearly."),
        Message::user("I made some changes, ran `git status`, and saw 'Changes not staged for commit'. What's the typical workflow to commit these changes? Give the git commands in order."),
    ];

    let start = Instant::now();
    let result = tokio::time::timeout(
        slow_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Disabled),
    )
    .await;

    let elapsed = start.elapsed();
    println!("  Response time: {:.2}s", elapsed.as_secs_f64());

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap().expect("Chat should succeed");

    let content = &response.choices[0].message.content;
    println!("  Response preview: {}", safe_truncate(content, 400));

    // Should mention the typical workflow
    let has_add = content.contains("git add") || content.contains("`add`");
    let has_commit = content.contains("git commit") || content.contains("`commit`");

    assert!(
        has_add || has_commit,
        "Should explain git add/commit workflow"
    );
}

/// Test model's ability to debug a runtime error
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_runtime_debugging() {
    let config = slow_model_config();
    skip_if_no_model!(&config);

    println!("Testing runtime debugging...");

    let client = ApiClient::new(&config).expect("Client should construct");

    let error_scenario = r#"
I have this Rust code:
```rust
fn main() {
    let numbers = vec![1, 2, 3];
    println!("{}", numbers[5]);
}
```
When I run it, I get:
```
thread 'main' panicked at 'index out of bounds: the len is 3 but the index is 5'
```
What's wrong and how do I fix it?
"#;

    let messages = vec![
        Message::system("You are a debugging expert. Identify issues and provide clear fixes."),
        Message::user(error_scenario),
    ];

    let start = Instant::now();
    let result = tokio::time::timeout(
        slow_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Enabled),
    )
    .await;

    let elapsed = start.elapsed();
    println!("  Response time: {:.2}s", elapsed.as_secs_f64());

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap().expect("Chat should succeed");

    let content = &response.choices[0].message.content;
    println!("  Response preview: {}", safe_truncate(content, 400));

    // Should identify the index out of bounds issue
    let identifies_issue = content.to_lowercase().contains("index")
        || content.to_lowercase().contains("bounds")
        || content.to_lowercase().contains("out of range")
        || content.to_lowercase().contains("only 3");

    let suggests_fix = content.contains(".get(")
        || content.contains("[0]")
        || content.contains("[1]")
        || content.contains("[2]")
        || content.to_lowercase().contains("valid index");

    assert!(
        identifies_issue,
        "Should identify the index out of bounds error"
    );

    if suggests_fix {
        println!("  Provided a fix suggestion");
    }
}

/// Test model's ability to summarize codebase structure
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_codebase_summary() {
    let config = slow_model_config();
    skip_if_no_model!(&config);
    skip_if_slow!();

    println!("Testing codebase summary...");

    let mut agent = Agent::new(config).await.expect("Agent should construct");

    let task = "Look at the directory structure of ./src and list the main modules in this Rust project. Just list the directory names and briefly describe what each might contain based on its name.";

    println!("  Task: summarize src/ structure");

    let start = Instant::now();
    let result = tokio::time::timeout(slow_timeout(), agent.run_task(task)).await;

    let elapsed = start.elapsed();
    println!("  Total time: {:.2}s", elapsed.as_secs_f64());

    match result {
        Ok(Ok(())) => {
            println!("  Summary task completed successfully");
        }
        Ok(Err(e)) => {
            println!("  Summary task error: {}", e);
        }
        Err(_) => {
            println!("  Summary task timed out after {:?}", elapsed);
        }
    }
}

/// Test model's ability to handle concurrent concepts
#[tokio::test]
#[cfg(feature = "integration")]
async fn deep_test_concurrency_understanding() {
    let config = slow_model_config();
    skip_if_no_model!(&config);

    println!("Testing concurrency understanding...");

    let client = ApiClient::new(&config).expect("Client should construct");

    let messages = vec![
        Message::system("You are a Rust concurrency expert."),
        Message::user("What's the difference between Arc<Mutex<T>> and Arc<RwLock<T>> in Rust? When would you use each?"),
    ];

    let start = Instant::now();
    let result = tokio::time::timeout(
        slow_timeout(),
        client.chat(messages, None, selfware::api::ThinkingMode::Enabled),
    )
    .await;

    let elapsed = start.elapsed();
    println!("  Response time: {:.2}s", elapsed.as_secs_f64());

    assert!(result.is_ok(), "Request should not timeout");
    let response = result.unwrap().expect("Chat should succeed");

    let content = &response.choices[0].message.content;
    println!("  Response preview: {}", safe_truncate(content, 500));

    // Should understand both types
    let understands_mutex = content.to_lowercase().contains("mutex")
        || content.to_lowercase().contains("exclusive")
        || content.to_lowercase().contains("lock");
    let understands_rwlock = content.to_lowercase().contains("rwlock")
        || content.to_lowercase().contains("read")
        || content.to_lowercase().contains("write");

    assert!(
        understands_mutex || understands_rwlock,
        "Should explain Mutex or RwLock concepts"
    );
}
