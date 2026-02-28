//! Test helpers and utilities for integration tests

use anyhow::Result;
use selfware::api::types::Message;
use selfware::config::{
    AgentConfig, Config, ExecutionMode, SafetyConfig, UiConfig, YoloFileConfig,
};
use std::env;
use std::time::Duration;

/// Get test configuration from environment variables
pub fn test_config() -> Config {
    let endpoint =
        env::var("SELFWARE_ENDPOINT").unwrap_or_else(|_| "http://localhost:8888/v1".to_string());
    let model = env::var("SELFWARE_MODEL").unwrap_or_else(|_| "unsloth/Kimi-K2.5-GGUF".to_string());
    let timeout: u64 = env::var("SELFWARE_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);

    Config {
        endpoint,
        model,
        max_tokens: 4096, // Keep small for faster responses
        temperature: 0.7,
        api_key: env::var("SELFWARE_API_KEY")
            .ok()
            .map(selfware::config::RedactedString::new),
        safety: SafetyConfig {
            allowed_paths: vec!["/tmp/**".to_string(), "./**".to_string()],
            denied_paths: vec![],
            protected_branches: vec!["main".to_string()],
            require_confirmation: vec![],
            strict_permissions: false,
        },
        agent: AgentConfig {
            max_iterations: 10, // Limit for tests
            step_timeout_secs: timeout,
            token_budget: 50000,
            native_function_calling: false,
            streaming: false, // Disable for tests
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

/// Check if slow tests should be skipped
pub fn skip_slow_tests() -> bool {
    env::var("SELFWARE_SKIP_SLOW")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Check if the model endpoint is available
pub async fn check_model_health(config: &Config) -> Result<bool> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let response = client
        .get(format!("{}/models", config.endpoint))
        .send()
        .await;

    match response {
        Ok(r) => Ok(r.status().is_success()),
        Err(_) => Ok(false),
    }
}

/// Check connectivity to an LLM endpoint and return whether it is reachable.
///
/// This is a convenience wrapper around [`check_model_health`] intended for use
/// at the top of integration tests:
///
/// ```ignore
/// if !require_llm_endpoint(&config).await {
///     println!("Skipping: LLM endpoint not available");
///     return;
/// }
/// ```
pub async fn require_llm_endpoint(config: &Config) -> bool {
    check_model_health(config).await.unwrap_or(false)
}

/// Check connectivity to an arbitrary LLM endpoint URL and return whether it
/// is reachable.  Useful for test files that manage their own config (e.g.
/// qwen3_tests) and only have a raw endpoint string.
pub async fn require_llm_endpoint_url(endpoint: &str) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    match client.get(format!("{}/models", endpoint)).send().await {
        Ok(r) => r.status().is_success(),
        Err(_) => false,
    }
}

/// Skip test if model is not available.
///
/// Prints a clearly visible SKIPPED message so that CI output does not
/// silently report a pass when the backend was never exercised.
#[macro_export]
macro_rules! skip_if_no_model {
    ($config:expr) => {
        if !$crate::helpers::check_model_health($config)
            .await
            .unwrap_or(false)
        {
            if std::env::var("CI").is_ok() || std::env::var("REQUIRE_MODEL").is_ok() {
                panic!(
                    "Model endpoint not available at {} - REQUIRED in CI",
                    $config.endpoint
                );
            }
            let test_path = module_path!();
            println!(
                "test {} ... SKIPPED (model endpoint not available at {})",
                test_path, $config.endpoint
            );
            eprintln!(
                "SKIPPED: {} - model endpoint not available at {}",
                test_path, $config.endpoint
            );
            return;
        }
    };
}

/// Skip test if SELFWARE_SKIP_SLOW is set.
///
/// Prints a clearly visible SKIPPED message so that CI output does not
/// silently report a pass when slow tests were disabled.
#[macro_export]
macro_rules! skip_if_slow {
    () => {
        if skip_slow_tests() {
            let test_path = module_path!();
            println!("test {} ... SKIPPED (SELFWARE_SKIP_SLOW=1)", test_path);
            eprintln!(
                "SKIPPED: {} - slow tests disabled (SELFWARE_SKIP_SLOW=1)",
                test_path
            );
            return;
        }
    };
}

/// Simple prompt that should trigger a tool call
pub fn file_read_prompt() -> &'static str {
    "Read the file at ./Cargo.toml and tell me the package name. Use the file_read tool."
}

/// Simple prompt for shell execution
pub fn shell_prompt() -> &'static str {
    "Run 'echo hello' using the shell_exec tool and tell me the output."
}

/// Simple prompt that should complete without tools
pub fn simple_question() -> &'static str {
    "What is 2 + 2? Answer with just the number."
}

/// Create a minimal test message
pub fn user_message(content: &str) -> Message {
    Message::user(content)
}

/// Create system prompt for testing
pub fn test_system_prompt() -> Message {
    Message::system(
        "You are a helpful assistant being tested. Respond concisely. \
         When asked to use a tool, use the XML format: \
         <tool><name>TOOL_NAME</name><arguments>{...}</arguments></tool>",
    )
}

/// Timeout duration for tests (from env or default 600s for slow models)
pub fn test_timeout() -> Duration {
    let secs: u64 = env::var("SELFWARE_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(600); // 10 minutes default for slow local models
    Duration::from_secs(secs)
}

/// Extended timeout for particularly slow operations (2x normal timeout)
pub fn extended_timeout() -> Duration {
    Duration::from_secs(test_timeout().as_secs() * 2)
}

/// Assert that a response contains expected text (case-insensitive)
pub fn assert_contains(response: &str, expected: &str) {
    assert!(
        response.to_lowercase().contains(&expected.to_lowercase()),
        "Expected response to contain '{}', got: {}",
        expected,
        &response[..response.len().min(200)]
    );
}

/// Assert that a response contains a tool call
pub fn assert_has_tool_call(response: &str, tool_name: &str) {
    assert!(
        response.contains(&format!("<name>{}</name>", tool_name))
            || response.contains(&format!("\"name\": \"{}\"", tool_name)),
        "Expected tool call to '{}', got: {}",
        tool_name,
        &response[..response.len().min(300)]
    );
}
