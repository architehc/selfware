//! Qwen3-Coder-Next Integration Tests
//!
//! Tests for Qwen3-Coder-Next-FP8 model with native tool calling support.
//! Run with: SELFWARE_ENDPOINT=http://localhost:8000/v1 SELFWARE_MODEL="Qwen/Qwen3-Coder-Next-FP8" cargo test --features integration qwen3_

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

use selfware::api::types::Message;
use selfware::api::ApiClient;
use selfware::api::ThinkingMode;
use selfware::config::{
    AgentConfig, Config, ExecutionMode, SafetyConfig, UiConfig, YoloFileConfig,
};

use super::helpers::require_llm_endpoint_url;

/// Get Qwen3-Coder test configuration
fn qwen3_config() -> Config {
    Config {
        endpoint: std::env::var("SELFWARE_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:8000/v1".to_string()),
        model: std::env::var("SELFWARE_MODEL")
            .unwrap_or_else(|_| "Qwen/Qwen3-Coder-Next-FP8".to_string()),
        max_tokens: 65536,
        temperature: 1.0, // Recommended by Qwen3-Coder docs
        api_key: None,
        safety: SafetyConfig::default(),
        agent: AgentConfig {
            max_iterations: 20,
            step_timeout_secs: 120,
            token_budget: 100000,
            native_function_calling: true, // Use native FC with Qwen3
            streaming: true,
            ..Default::default()
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

/// Check if Qwen3 model is available
async fn qwen3_available() -> bool {
    let endpoint = std::env::var("SELFWARE_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:8000/v1".to_string());
    require_llm_endpoint_url(&endpoint).await
}

/// Macro-style helper: check Qwen3 availability and skip with a clear message
/// if the endpoint is unreachable.  Prints to both stdout (for `cargo test`
/// summary) and stderr (for CI log scanners).
macro_rules! skip_if_no_qwen3 {
    () => {
        if !qwen3_available().await {
            let test_path = module_path!();
            println!(
                "test {} ... SKIPPED (Qwen3 endpoint not available)",
                test_path
            );
            eprintln!("SKIPPED: {} - Qwen3 endpoint not available", test_path);
            return;
        }
    };
}

// ============================================================================
// Basic API Tests
// ============================================================================

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_health_check() {
    skip_if_no_qwen3!();

    let client = reqwest::Client::new();
    let endpoint = qwen3_config().endpoint;

    let response = client
        .get(format!("{}/models", endpoint))
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success());
    let body = response.text().await.unwrap();
    assert!(body.contains("Qwen3-Coder"));
    println!("Health check passed: {}", body);
}

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_simple_completion() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = ApiClient::new(&config).expect("Failed to create client");

    let messages = vec![Message::user("What is 2 + 2? Reply with just the number.")];

    let start = Instant::now();
    let response = client.chat(messages, None, ThinkingMode::Disabled).await;
    let elapsed = start.elapsed();

    println!("Response time: {:.2?}", elapsed);

    let response = response.expect("Failed to get response");
    let content = &response.choices[0].message.content;

    println!("Response: {}", content);
    assert!(
        content.contains("4"),
        "Expected '4' in response: {}",
        content
    );
    assert!(elapsed.as_secs() < 30, "Response too slow: {:.2?}", elapsed);
}

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_code_generation() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = ApiClient::new(&config).expect("Failed to create client");

    let messages = vec![
        Message::user("Write a Python function to check if a number is prime. Only output the code, no explanation."),
    ];

    let start = Instant::now();
    let response = client.chat(messages, None, ThinkingMode::Disabled).await;
    let elapsed = start.elapsed();

    println!("Code generation time: {:.2?}", elapsed);

    let response = response.expect("Failed to get response");
    let content = &response.choices[0].message.content;

    println!("Generated code:\n{}", content);

    // Should contain key Python elements
    assert!(content.contains("def"), "Expected function definition");
    assert!(
        content.contains("prime") || content.contains("Prime"),
        "Expected prime-related code"
    );
}

// ============================================================================
// Tool Calling Tests
// ============================================================================

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_tool_call_format() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = ApiClient::new(&config).expect("Failed to create client");

    // Use system prompt to request XML tool format (same as selfware uses)
    let messages = vec![
        Message::system(
            "You are an AI assistant with access to tools. When you need to use a tool, respond with:\n\
            <tool><name>TOOL_NAME</name><arguments>{JSON}</arguments></tool>\n\n\
            Available tools:\n\
            - file_read: Read a file. Arguments: {\"path\": \"string\"}\n\
            - shell_exec: Execute a shell command. Arguments: {\"command\": \"string\"}"
        ),
        Message::user("Read the contents of ./Cargo.toml"),
    ];

    let response = client.chat(messages, None, ThinkingMode::Disabled).await;
    let response = response.expect("Failed to get response");
    let content = &response.choices[0].message.content;

    println!("Tool call response:\n{}", content);

    // Check for XML-style tool call
    let has_xml_tool = content.contains("<tool>") && content.contains("<name>");

    // Or check for OpenAI-style tool call
    let has_openai_tool = response.choices[0].message.tool_calls.is_some();

    assert!(
        has_xml_tool || has_openai_tool,
        "Expected tool call in response: {}",
        content
    );
}

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_native_tool_calling() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = ApiClient::new(&config).expect("Failed to create client");

    // Define tools in OpenAI format
    let tools = vec![selfware::api::types::ToolDefinition {
        def_type: "function".to_string(),
        function: selfware::api::types::FunctionDefinition {
            name: "get_weather".to_string(),
            description: "Get current weather for a location".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "City name"
                    }
                },
                "required": ["location"]
            }),
        },
    }];

    let messages = vec![Message::user(
        "What's the weather in Tokyo? You must use the get_weather tool.",
    )];

    let response = client
        .chat(messages, Some(tools), ThinkingMode::Disabled)
        .await;
    let response = response.expect("Failed to get response");

    println!("Response: {:?}", response.choices[0]);

    // Check for tool calls - OpenAI format or XML format
    let has_openai_tool = response.choices[0].message.tool_calls.is_some();
    let content = &response.choices[0].message.content;
    let has_xml_tool = content.contains("<tool>") || content.contains("get_weather");

    if let Some(ref tool_calls) = response.choices[0].message.tool_calls {
        println!("Got {} OpenAI-style tool calls", tool_calls.len());
        for tc in tool_calls {
            println!(
                "  Tool: {} - Args: {}",
                tc.function.name, tc.function.arguments
            );
        }
    }

    if has_xml_tool {
        println!("Found XML-style tool call or tool mention in content");
    }

    println!("Content: {}", content);

    // Accept either format - some models use XML, some use native tool calling
    assert!(
        has_openai_tool || has_xml_tool,
        "Expected tool call in OpenAI or XML format"
    );
}

// ============================================================================
// Multi-Agent / Concurrent Tests
// ============================================================================

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_concurrent_requests() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = Arc::new(ApiClient::new(&config).expect("Failed to create client"));

    // Run 3 concurrent requests
    let prompts = vec![
        "What is the capital of France? One word answer.",
        "What is 10 * 10? Just the number.",
        "Name a programming language. One word.",
    ];

    let start = Instant::now();

    let handles: Vec<_> = prompts
        .into_iter()
        .map(|prompt| {
            let client = Arc::clone(&client);
            tokio::spawn(async move {
                let messages = vec![Message::user(prompt)];
                let result = client.chat(messages, None, ThinkingMode::Disabled).await;
                (prompt, result)
            })
        })
        .collect();

    let mut results = Vec::new();
    for handle in handles {
        let (prompt, result) = handle.await.expect("Task panicked");
        results.push((prompt, result));
    }

    let elapsed = start.elapsed();
    println!("3 concurrent requests completed in {:.2?}", elapsed);

    for (prompt, result) in &results {
        match result {
            Ok(response) => {
                println!(
                    "  {} -> {}",
                    prompt,
                    response.choices[0].message.content.trim()
                );
            }
            Err(e) => {
                println!("  {} -> ERROR: {}", prompt, e);
            }
        }
    }

    // All should succeed
    let successes = results.iter().filter(|(_, r)| r.is_ok()).count();
    assert_eq!(successes, 3, "Expected all 3 requests to succeed");
}

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_parallel_agents_simulation() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = Arc::new(ApiClient::new(&config).expect("Failed to create client"));

    // Simulate 5 agents working on different tasks
    let tasks = vec![
        (
            "Agent-1",
            "Write a one-line Python function to add two numbers.",
        ),
        (
            "Agent-2",
            "Write a one-line JavaScript function to multiply two numbers.",
        ),
        (
            "Agent-3",
            "Write a one-line Rust function to subtract two numbers.",
        ),
        (
            "Agent-4",
            "Write a one-line Go function to divide two numbers.",
        ),
        (
            "Agent-5",
            "Write a one-line TypeScript function to check if number is even.",
        ),
    ];

    let semaphore = Arc::new(Semaphore::new(3)); // Limit concurrency to 3
    let start = Instant::now();

    let handles: Vec<_> = tasks
        .into_iter()
        .map(|(agent_name, task)| {
            let client = Arc::clone(&client);
            let sem = Arc::clone(&semaphore);
            tokio::spawn(async move {
                let _permit = sem.acquire().await.expect("Semaphore error");
                let agent_start = Instant::now();

                let messages = vec![
                    Message::system(format!("You are {}. Complete tasks concisely.", agent_name)),
                    Message::user(task),
                ];

                let result = client.chat(messages, None, ThinkingMode::Disabled).await;
                let elapsed = agent_start.elapsed();

                (agent_name, task, result, elapsed)
            })
        })
        .collect();

    println!("\n=== Parallel Agent Results ===");

    let mut all_succeeded = true;
    for handle in handles {
        let (agent, task, result, elapsed) = handle.await.expect("Task panicked");
        match result {
            Ok(response) => {
                let content = response.choices[0].message.content.trim();
                println!("\n{} ({:.2?}):", agent, elapsed);
                println!("  Task: {}", task);
                println!("  Result: {}", &content[..content.len().min(200)]);
            }
            Err(e) => {
                println!("\n{} FAILED: {}", agent, e);
                all_succeeded = false;
            }
        }
    }

    let total_elapsed = start.elapsed();
    println!("\nTotal time for 5 agents: {:.2?}", total_elapsed);

    assert!(all_succeeded, "Some agents failed");
}

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_conversation_memory() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = ApiClient::new(&config).expect("Failed to create client");

    // Multi-turn conversation
    let mut messages = vec![
        Message::system("You are a helpful assistant. Remember what the user tells you."),
        Message::user("My name is Alice and my favorite color is blue."),
    ];

    // First turn
    let response1 = client
        .chat(messages.clone(), None, ThinkingMode::Disabled)
        .await
        .expect("First request failed");
    let assistant_reply1 = response1.choices[0].message.content.clone();
    println!("Turn 1 - User: My name is Alice...");
    println!("Turn 1 - Assistant: {}", assistant_reply1);

    // Add assistant response and ask follow-up
    messages.push(Message::assistant(&assistant_reply1));
    messages.push(Message::user("What is my name and favorite color?"));

    // Second turn
    let response2 = client
        .chat(messages, None, ThinkingMode::Disabled)
        .await
        .expect("Second request failed");
    let assistant_reply2 = response2.choices[0].message.content.clone();
    println!("\nTurn 2 - User: What is my name and favorite color?");
    println!("Turn 2 - Assistant: {}", assistant_reply2);

    // Should remember the name and color
    let reply_lower = assistant_reply2.to_lowercase();
    assert!(
        reply_lower.contains("alice") || reply_lower.contains("blue"),
        "Expected model to remember name or color: {}",
        assistant_reply2
    );
}

// ============================================================================
// E2E Coding Workflow Tests
// ============================================================================

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_code_review() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = ApiClient::new(&config).expect("Failed to create client");

    let code = r#"
fn calculate_average(numbers: Vec<i32>) -> f64 {
    let sum: i32 = numbers.iter().sum();
    sum as f64 / numbers.len() as f64
}
"#;

    let messages = vec![
        Message::system("You are a code reviewer. Identify bugs and suggest improvements."),
        Message::user(format!("Review this Rust code for bugs:\n{}", code)),
    ];

    let response = client
        .chat(messages, None, ThinkingMode::Disabled)
        .await
        .expect("Request failed");
    let content = &response.choices[0].message.content;

    println!("Code review result:\n{}", content);

    // Should identify the division by zero risk
    let content_lower = content.to_lowercase();
    assert!(
        content_lower.contains("empty")
            || content_lower.contains("zero")
            || content_lower.contains("panic")
            || content_lower.contains("check"),
        "Expected review to mention empty/zero/panic risk"
    );
}

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_bug_fix_suggestion() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = ApiClient::new(&config).expect("Failed to create client");

    let buggy_code = r#"
def get_user_age(users, name):
    for user in users:
        if user["name"] == name:
            return user["age"]
    # Bug: no return statement for not found case
"#;

    let messages = vec![Message::user(format!(
        "Fix this Python bug and show the corrected code:\n{}",
        buggy_code
    ))];

    let response = client
        .chat(messages, None, ThinkingMode::Disabled)
        .await
        .expect("Request failed");
    let content = &response.choices[0].message.content;

    println!("Bug fix suggestion:\n{}", content);

    // Should suggest a fix (return None, raise exception, or similar)
    assert!(
        content.contains("return") || content.contains("None") || content.contains("raise"),
        "Expected fix suggestion"
    );
}

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_test_generation() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = ApiClient::new(&config).expect("Failed to create client");

    let function = r#"
pub fn fibonacci(n: u32) -> u32 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}
"#;

    let messages = vec![Message::user(format!(
        "Write unit tests for this Rust function:\n{}\nUse #[test] attribute.",
        function
    ))];

    let response = client
        .chat(messages, None, ThinkingMode::Disabled)
        .await
        .expect("Request failed");
    let content = &response.choices[0].message.content;

    println!("Generated tests:\n{}", content);

    // Should generate test functions
    assert!(
        content.contains("#[test]") || content.contains("fn test"),
        "Expected test functions"
    );
    assert!(content.contains("assert"), "Expected assertions");
}

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_refactoring() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = ApiClient::new(&config).expect("Failed to create client");

    let messy_code = r#"
function processData(d) {
    var r = [];
    for (var i = 0; i < d.length; i++) {
        if (d[i].active == true) {
            if (d[i].age > 18) {
                r.push({name: d[i].name, status: 'adult'});
            }
        }
    }
    return r;
}
"#;

    let messages = vec![Message::user(format!(
        "Refactor this JavaScript to modern ES6+ with filter/map:\n{}",
        messy_code
    ))];

    let response = client
        .chat(messages, None, ThinkingMode::Disabled)
        .await
        .expect("Request failed");
    let content = &response.choices[0].message.content;

    println!("Refactored code:\n{}", content);

    // Should use modern syntax
    assert!(
        content.contains("filter") || content.contains("map") || content.contains("=>"),
        "Expected modern JavaScript syntax"
    );
}

// ============================================================================
// Stress / Performance Tests
// ============================================================================

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_rapid_requests() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = Arc::new(ApiClient::new(&config).expect("Failed to create client"));

    // Send 10 rapid-fire requests
    let start = Instant::now();

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let client = Arc::clone(&client);
            tokio::spawn(async move {
                let messages = vec![Message::user(format!("Reply with just the number: {}", i))];
                let req_start = Instant::now();
                let result = client.chat(messages, None, ThinkingMode::Disabled).await;
                (i, result, req_start.elapsed())
            })
        })
        .collect();

    let mut successes = 0;
    let mut failures = 0;
    let mut total_time = Duration::ZERO;

    for handle in handles {
        let (i, result, elapsed) = handle.await.expect("Task panicked");
        match result {
            Ok(response) => {
                successes += 1;
                total_time += elapsed;
                println!(
                    "Request {} succeeded in {:.2?}: {}",
                    i,
                    elapsed,
                    response.choices[0].message.content.trim()
                );
            }
            Err(e) => {
                failures += 1;
                println!("Request {} failed: {}", i, e);
            }
        }
    }

    let total_elapsed = start.elapsed();
    println!("\n=== Rapid Request Summary ===");
    println!("Total time: {:.2?}", total_elapsed);
    println!("Successes: {}, Failures: {}", successes, failures);
    if successes > 0 {
        println!("Avg response time: {:.2?}", total_time / successes as u32);
    }

    assert!(successes >= 8, "At least 8/10 requests should succeed");
}

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_long_context() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = ApiClient::new(&config).expect("Failed to create client");

    // Create a moderately long context (~2000 tokens)
    let long_text = "This is line of text for testing long context handling. ".repeat(100);

    let messages = vec![Message::user(format!(
        "Here's a long text:\n\n{}\n\nWhat is the first word of this text?",
        long_text
    ))];

    let start = Instant::now();
    let response = client
        .chat(messages, None, ThinkingMode::Disabled)
        .await
        .expect("Request failed");
    let elapsed = start.elapsed();

    let content = &response.choices[0].message.content;
    println!("Long context response ({:.2?}): {}", elapsed, content);

    // Should identify "This" as the first word
    assert!(
        content.to_lowercase().contains("this"),
        "Expected 'this' as first word"
    );
}

// ============================================================================
// Integration with Agent System
// ============================================================================

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_tool_sequence() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = ApiClient::new(&config).expect("Failed to create client");

    // Simulate a multi-tool sequence (like an agent would do)
    let system_prompt = r#"You are a coding agent with these tools:
- file_read(path): Read file contents
- file_edit(path, old, new): Replace text in file
- shell_exec(command): Run shell command

When you need a tool, respond with:
<tool><name>TOOL_NAME</name><arguments>{"arg": "value"}</arguments></tool>

After using tools, summarize what you did."#;

    let messages = vec![
        Message::system(system_prompt),
        Message::user("Check the current directory and list Rust source files."),
    ];

    let response = client
        .chat(messages.clone(), None, ThinkingMode::Disabled)
        .await
        .expect("Request failed");
    let content = &response.choices[0].message.content;

    println!("Tool sequence response:\n{}", content);

    // Should request a shell command or file listing
    assert!(
        content.contains("<tool>")
            || content.contains("shell")
            || content.contains("ls")
            || content.contains("find"),
        "Expected tool usage or command reference"
    );
}

// ============================================================================
// Heavy Multi-Agent Stress Tests
// ============================================================================

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_10_concurrent_agents() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = Arc::new(ApiClient::new(&config).expect("Failed to create client"));

    // 10 diverse coding tasks
    let tasks = vec![
        (
            "Agent-01",
            "Write a Rust function to reverse a string. One line only.",
        ),
        (
            "Agent-02",
            "Write a Python function to find max in a list. One line.",
        ),
        (
            "Agent-03",
            "Write a TypeScript function to check if string is palindrome.",
        ),
        (
            "Agent-04",
            "Write a Go function to calculate factorial. One line.",
        ),
        (
            "Agent-05",
            "Write a Rust function to check if number is even. One line.",
        ),
        (
            "Agent-06",
            "Write a JavaScript arrow function to double a number.",
        ),
        (
            "Agent-07",
            "Write a Python list comprehension to get squares of 1-10.",
        ),
        (
            "Agent-08",
            "Write a Rust match expression to convert 1-3 to 'one'-'three'.",
        ),
        (
            "Agent-09",
            "Write a TypeScript function to merge two arrays.",
        ),
        (
            "Agent-10",
            "Write a Python function to count vowels in a string.",
        ),
    ];

    let start = Instant::now();

    let handles: Vec<_> = tasks
        .into_iter()
        .map(|(name, task)| {
            let client = Arc::clone(&client);
            tokio::spawn(async move {
                let req_start = Instant::now();
                let messages = vec![
                    Message::system(format!(
                        "You are {}. Complete tasks with minimal code.",
                        name
                    )),
                    Message::user(task),
                ];
                let result = client.chat(messages, None, ThinkingMode::Disabled).await;
                (name, task, result, req_start.elapsed())
            })
        })
        .collect();

    println!("\n=== 10 Concurrent Agents Test ===");

    let mut successes = 0;
    let mut failures = 0;
    let mut total_response_time = Duration::ZERO;

    for handle in handles {
        let (name, _task, result, elapsed) = handle.await.expect("Task panicked");
        match result {
            Ok(response) => {
                successes += 1;
                total_response_time += elapsed;
                let content = response.choices[0].message.content.trim();
                let preview = if content.chars().count() > 80 {
                    format!("{}...", content.chars().take(80).collect::<String>())
                } else {
                    content.to_string()
                };
                println!("{} ({:.2?}): {}", name, elapsed, preview);
            }
            Err(e) => {
                failures += 1;
                println!("{} FAILED: {}", name, e);
            }
        }
    }

    let total_elapsed = start.elapsed();
    println!("\n=== Summary ===");
    println!("Total wall time: {:.2?}", total_elapsed);
    println!("Successes: {}/10, Failures: {}", successes, failures);
    if successes > 0 {
        println!(
            "Avg response: {:.2?}",
            total_response_time / successes as u32
        );
        println!(
            "Throughput: {:.2} req/s",
            10.0 / total_elapsed.as_secs_f64()
        );
    }

    assert!(successes >= 8, "At least 8/10 should succeed");
}

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_agent_tool_loop() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = ApiClient::new(&config).expect("Failed to create client");

    // Simulate a complete agent loop with tool calls and results
    let system_prompt = r#"You are a coding agent. Use tools in XML format:
<tool><name>TOOL</name><arguments>{JSON}</arguments></tool>

Available tools:
- file_read(path): Read file
- shell_exec(command): Run command

Complete tasks step by step."#;

    let mut messages = vec![
        Message::system(system_prompt),
        Message::user("Check if a file called test.txt exists, then create it with 'hello world'."),
    ];

    println!("\n=== Agent Tool Loop Simulation ===");

    // Turn 1: Agent should request file check
    let response1 = client
        .chat(messages.clone(), None, ThinkingMode::Disabled)
        .await
        .expect("Turn 1 failed");
    let content1 = &response1.choices[0].message.content;
    println!("Turn 1 - Agent: {}", &content1[..content1.len().min(200)]);

    // Add assistant response and simulate tool result
    messages.push(Message::assistant(content1));
    messages.push(Message::user("Tool result: File test.txt does not exist."));

    // Turn 2: Agent should request file creation
    let response2 = client
        .chat(messages.clone(), None, ThinkingMode::Disabled)
        .await
        .expect("Turn 2 failed");
    let content2 = &response2.choices[0].message.content;
    println!("Turn 2 - Agent: {}", &content2[..content2.len().min(200)]);

    // Add assistant response and simulate tool result
    messages.push(Message::assistant(content2));
    messages.push(Message::user("Tool result: File created successfully."));

    // Turn 3: Agent should summarize
    let response3 = client
        .chat(messages.clone(), None, ThinkingMode::Disabled)
        .await
        .expect("Turn 3 failed");
    let content3 = &response3.choices[0].message.content;
    println!("Turn 3 - Agent: {}", &content3[..content3.len().min(200)]);

    // Verify the loop worked
    let all_content = format!("{}\n{}\n{}", content1, content2, content3);
    assert!(
        all_content.contains("<tool>")
            || all_content.to_lowercase().contains("create")
            || all_content.to_lowercase().contains("shell")
            || all_content.to_lowercase().contains("echo"),
        "Expected tool usage throughout the loop"
    );
}

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_complex_coding_task() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = ApiClient::new(&config).expect("Failed to create client");

    let task = r#"Write a complete Rust module with:
1. A struct `Counter` with a `count: i32` field
2. An impl block with `new()`, `increment()`, and `get()` methods
3. Unit tests for all methods

Output only the code, no explanations."#;

    let messages = vec![Message::user(task)];

    let start = Instant::now();
    let response = client
        .chat(messages, None, ThinkingMode::Disabled)
        .await
        .expect("Request failed");
    let elapsed = start.elapsed();

    let content = &response.choices[0].message.content;
    println!("Complex coding task ({:.2?}):\n{}", elapsed, content);

    // Should have struct, impl, and tests
    assert!(
        content.contains("struct Counter"),
        "Expected struct Counter"
    );
    assert!(
        content.contains("impl") || content.contains("fn new"),
        "Expected impl block"
    );
    assert!(
        content.contains("#[test]") || content.contains("fn test"),
        "Expected tests"
    );
}

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_error_recovery() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = ApiClient::new(&config).expect("Failed to create client");

    let system_prompt =
        "You are a helpful coding assistant. If you encounter errors, explain how to fix them.";

    // Simulate a failed tool execution scenario
    let messages = vec![
        Message::system(system_prompt),
        Message::user("I ran `cargo build` and got this error: 'error[E0425]: cannot find value `foo` in this scope'. How do I fix it?"),
    ];

    let response = client
        .chat(messages.clone(), None, ThinkingMode::Disabled)
        .await
        .expect("Request failed");
    let content = &response.choices[0].message.content;

    println!("Error recovery response:\n{}", content);

    // Should provide helpful guidance
    let content_lower = content.to_lowercase();
    assert!(
        content_lower.contains("define")
            || content_lower.contains("declare")
            || content_lower.contains("import")
            || content_lower.contains("variable")
            || content_lower.contains("scope")
            || content_lower.contains("let")
            || content_lower.contains("const"),
        "Expected helpful guidance for E0425 error"
    );
}

#[tokio::test]
#[cfg(feature = "integration")]
async fn qwen3_test_continuous_dialogue() {
    skip_if_no_qwen3!();

    let config = qwen3_config();
    let client = ApiClient::new(&config).expect("Failed to create client");

    let mut messages = vec![Message::system(
        "You are a Rust tutor. Help the student learn step by step.",
    )];

    let questions = [
        "What is ownership in Rust?",
        "Can you give me a simple example?",
        "What happens if I try to use a moved value?",
    ];

    println!("\n=== Continuous Dialogue Test ===");

    for (i, question) in questions.iter().enumerate() {
        messages.push(Message::user(*question));

        let response = client
            .chat(messages.clone(), None, ThinkingMode::Disabled)
            .await
            .unwrap_or_else(|_| panic!("Turn {} failed", i + 1));

        let content = response.choices[0].message.content.clone();
        let preview = if content.chars().count() > 150 {
            format!("{}...", content.chars().take(150).collect::<String>())
        } else {
            content.clone()
        };

        println!("\nQ{}: {}", i + 1, question);
        println!("A{}: {}", i + 1, preview);

        messages.push(Message::assistant(&content));
    }

    // Final message should still have context from earlier
    let last_response = &messages.last().unwrap().content;
    let content_lower = last_response.to_lowercase();

    // Should mention moved value, ownership, or error concepts
    assert!(
        content_lower.contains("move")
            || content_lower.contains("owner")
            || content_lower.contains("borrow")
            || content_lower.contains("error")
            || content_lower.contains("use")
            || content_lower.contains("value"),
        "Expected context-aware response about moved values"
    );
}
