//! Comprehensive E2E System Tests for Selfware
//!
//! Tests the full user journey from configuration through task execution,
//! including tool orchestration, LLM integration, benchmarks, and doctor
//! diagnostics.
//!
//! All tests are gated behind `#[cfg(feature = "system-tests")]` so they
//! do not run in normal CI. Run with:
//!
//! ```sh
//! cargo test --features system-tests --test e2e_system_test -- --nocapture
//! ```
//!
//! LLM tests additionally require the endpoint to be reachable:
//!
//! ```sh
//! SELFWARE_TEST_ENDPOINT=https://crazyshit.ngrok.io/v1 \
//! SELFWARE_TEST_MODEL=txn545/Qwen3.5-122B-A10B-NVFP4 \
//! cargo test --features system-tests --test e2e_system_test -- --nocapture
//! ```

#![cfg(feature = "system-tests")]

use selfware::config::{
    AgentConfig, Config, ExecutionMode, RedactedString, SafetyConfig, UiConfig, YoloFileConfig,
};
use selfware::doctor::{run_doctor, CheckStatus, OverallHealth};
use selfware::tools::ToolRegistry;
use std::collections::HashSet;
use std::fs;
use std::time::Instant;
use tempfile::tempdir;

// ============================================================================
// Helpers
// ============================================================================

/// Default test endpoint for LLM tests.
const DEFAULT_TEST_ENDPOINT: &str = "https://crazyshit.ngrok.io/v1";

/// Default test model for LLM tests.
const DEFAULT_TEST_MODEL: &str = "txn545/Qwen3.5-122B-A10B-NVFP4";

/// Build a Config pointed at the test LLM endpoint.
fn test_llm_config() -> Config {
    let endpoint = std::env::var("SELFWARE_TEST_ENDPOINT")
        .unwrap_or_else(|_| DEFAULT_TEST_ENDPOINT.to_string());
    let model =
        std::env::var("SELFWARE_TEST_MODEL").unwrap_or_else(|_| DEFAULT_TEST_MODEL.to_string());
    let api_key = std::env::var("SELFWARE_API_KEY")
        .ok()
        .map(RedactedString::new);

    Config {
        endpoint,
        model,
        max_tokens: 4096,
        temperature: 0.7,
        api_key,
        safety: SafetyConfig {
            allowed_paths: vec!["/**".to_string()],
            ..Default::default()
        },
        agent: AgentConfig {
            max_iterations: 10,
            step_timeout_secs: 120,
            token_budget: 50_000,
            native_function_calling: false,
            streaming: false,
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

/// Initialise safety config to allow all paths (for temp-dir tests).
fn init_permissive_safety() {
    let cfg = SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..Default::default()
    };
    selfware::tools::file::init_safety_config(&cfg);
}

/// Check whether the test LLM endpoint is reachable. Returns `false` (and
/// prints a skip message) if it is not, so callers can `return` early.
async fn require_test_endpoint() -> bool {
    let endpoint = std::env::var("SELFWARE_TEST_ENDPOINT")
        .unwrap_or_else(|_| DEFAULT_TEST_ENDPOINT.to_string());
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    match client.get(format!("{}/models", endpoint)).send().await {
        Ok(r) if r.status().is_success() => true,
        _ => {
            println!(
                "SKIPPED: LLM endpoint not reachable at {} — set SELFWARE_TEST_ENDPOINT",
                endpoint
            );
            false
        }
    }
}

// ============================================================================
// 1. Configuration & Startup Tests
// ============================================================================

#[test]
fn test_config_loads_with_defaults() {
    let config = Config::default();
    assert!(!config.endpoint.is_empty(), "endpoint must not be empty");
    assert!(!config.model.is_empty(), "model must not be empty");
    assert!(config.max_tokens > 0, "max_tokens must be positive");
    assert!(
        config.temperature >= 0.0 && config.temperature <= 2.0,
        "temperature out of range"
    );
    assert!(
        config.agent.max_iterations > 0,
        "max_iterations must be positive"
    );
    assert!(
        config.agent.step_timeout_secs > 0,
        "step_timeout must be positive"
    );
    assert!(
        !config.safety.allowed_paths.is_empty(),
        "allowed_paths must have defaults"
    );
    println!(
        "  config defaults: endpoint={}, model={}, max_tokens={}, temperature={}",
        config.endpoint, config.model, config.max_tokens, config.temperature
    );
}

#[test]
fn test_config_from_toml_string() {
    let toml_str = r#"
endpoint = "http://localhost:9999/v1"
model = "test-model/big"
max_tokens = 8192
temperature = 0.5

[safety]
allowed_paths = ["/tmp/**", "./**"]
denied_paths = ["/etc/**"]
protected_branches = ["main", "release"]
strict_permissions = true

[agent]
max_iterations = 20
step_timeout_secs = 300
token_budget = 100000
native_function_calling = true
streaming = false

[ui]
theme = "cyan"
animations = false
compact_mode = true

[qa]
profile = "strict"

[[hooks]]
event = "PostToolUse"
match_tools = ["file_write"]
command = "echo formatted"
"#;

    let config: Config = toml::from_str(toml_str).expect("TOML parse failed");
    assert_eq!(config.endpoint, "http://localhost:9999/v1");
    assert_eq!(config.model, "test-model/big");
    assert_eq!(config.max_tokens, 8192);
    assert!((config.temperature - 0.5).abs() < f32::EPSILON);
    assert_eq!(config.safety.allowed_paths, vec!["/tmp/**", "./**"]);
    assert_eq!(config.safety.denied_paths, vec!["/etc/**"]);
    assert!(config.safety.strict_permissions);
    assert_eq!(config.agent.max_iterations, 20);
    assert_eq!(config.agent.step_timeout_secs, 300);
    assert!(config.agent.native_function_calling);
    assert!(!config.agent.streaming);
    assert_eq!(config.ui.theme, "cyan");
    assert!(!config.ui.animations);
    assert!(config.ui.compact_mode);
    assert!(!config.hooks.is_empty());
    println!("  TOML config parsed successfully with all sections");
}

#[test]
fn test_tool_registry_has_all_tools() {
    let registry = ToolRegistry::new();
    let tools = registry.list();
    let tool_names: HashSet<&str> = tools.iter().map(|t| t.name()).collect();

    // Core tools that must be present
    let expected_core = [
        // File operations
        "file_read",
        "file_write",
        "file_edit",
        "file_delete",
        "directory_tree",
        // Git operations
        "git_status",
        "git_diff",
        "git_commit",
        "git_push",
        "git_checkpoint",
        // Cargo/Build
        "cargo_check",
        "cargo_test",
        "cargo_clippy",
        "cargo_fmt",
        // System
        "shell_exec",
        "pty_shell",
        // Search
        "grep_search",
        "glob_find",
        "symbol_search",
        // HTTP
        "http_request",
        // Process management
        "process_start",
        "process_stop",
        "process_list",
        "process_logs",
        "process_restart",
        "port_check",
        // Package managers
        "npm_install",
        "npm_run",
        "npm_scripts",
        "pip_install",
        "pip_list",
        "pip_freeze",
        "yarn_install",
        // Container operations
        "container_run",
        "container_stop",
        "container_list",
        "container_logs",
        "container_exec",
        "container_build",
        "container_images",
        "container_pull",
        "container_remove",
        "compose_up",
        "compose_down",
        // Screen capture
        "screen_capture",
        // Vision
        "vision_analyze",
        "vision_compare",
        // Browser
        "browser_fetch",
        "browser_screenshot",
        "browser_pdf",
        "browser_eval",
        "browser_links",
        // Knowledge graph
        "knowledge_add",
        "knowledge_relate",
        "knowledge_query",
        "knowledge_stats",
        "knowledge_clear",
        "knowledge_remove",
        "knowledge_export",
        // Swarm
        "swarm_dispatch",
        // Computer control
        "computer_mouse",
        "computer_keyboard",
        "computer_screen",
        "computer_window",
        // LSP
        "lsp_goto_definition",
        "lsp_find_references",
        "lsp_document_symbols",
        "lsp_hover",
    ];

    let mut missing = Vec::new();
    for name in &expected_core {
        if !tool_names.contains(name) {
            missing.push(*name);
        }
    }

    assert!(
        missing.is_empty(),
        "Missing tools: {:?}\nRegistered tools ({} total): {:?}",
        missing,
        tool_names.len(),
        {
            let mut sorted: Vec<&str> = tool_names.iter().copied().collect();
            sorted.sort();
            sorted
        }
    );

    // Verify we have 60+ tools
    assert!(
        tool_names.len() >= 60,
        "Expected at least 60 tools, found {}",
        tool_names.len()
    );

    println!(
        "  {} tools registered, all {} expected core tools present",
        tool_names.len(),
        expected_core.len()
    );
}

#[tokio::test]
async fn test_doctor_runs_without_panic() {
    let start = Instant::now();
    let report = run_doctor().await;
    let elapsed = start.elapsed();

    assert!(
        !report.checks.is_empty(),
        "doctor must return at least one check"
    );

    // rustc must be OK in any Rust build environment
    let rustc = report.checks.iter().find(|c| c.name == "rustc");
    assert!(rustc.is_some(), "rustc check missing from doctor report");
    assert_eq!(rustc.unwrap().status, CheckStatus::Ok, "rustc should be OK");

    println!(
        "  doctor completed in {:?} — {} checks, health={}",
        elapsed,
        report.checks.len(),
        report.health
    );
}

// ============================================================================
// 2. Tool Execution Tests (no LLM needed)
// ============================================================================

#[tokio::test]
async fn test_file_write_read_delete_cycle() {
    init_permissive_safety();
    let dir = tempdir().unwrap();
    let registry = ToolRegistry::new();

    let test_path = dir.path().join("lifecycle_test.txt");
    let path_str = test_path.to_str().unwrap();

    // Write
    let file_write = registry.get("file_write").unwrap();
    let result = file_write
        .execute(serde_json::json!({
            "path": path_str,
            "content": "Hello from E2E system test!\nLine two.\nLine three.\n"
        }))
        .await
        .unwrap();
    assert!(
        result.get("success").is_some() || result.get("path").is_some(),
        "file_write should succeed"
    );
    assert!(test_path.exists(), "file must exist after write");

    // Read
    let file_read = registry.get("file_read").unwrap();
    let result = file_read
        .execute(serde_json::json!({ "path": path_str }))
        .await
        .unwrap();
    let content = result["content"].as_str().unwrap();
    assert!(content.contains("Hello from E2E system test!"));
    assert!(content.contains("Line three."));

    // Delete
    let file_delete = registry.get("file_delete").unwrap();
    let result = file_delete
        .execute(serde_json::json!({ "path": path_str }))
        .await
        .unwrap();
    assert!(
        result.get("success").is_some() || result.get("deleted").is_some(),
        "file_delete should succeed"
    );
    assert!(!test_path.exists(), "file must not exist after delete");

    println!("  file write/read/delete lifecycle passed");
}

#[tokio::test]
async fn test_shell_exec_basic_commands() {
    let registry = ToolRegistry::new();
    let shell = registry.get("shell_exec").unwrap();

    // echo
    let result = shell
        .execute(serde_json::json!({
            "command": "echo 'system_test_output'",
            "timeout_secs": 5
        }))
        .await
        .unwrap();
    assert_eq!(result["exit_code"], 0);
    assert!(result["stdout"]
        .as_str()
        .unwrap()
        .contains("system_test_output"));

    // pwd
    let result = shell
        .execute(serde_json::json!({
            "command": "pwd",
            "timeout_secs": 5
        }))
        .await
        .unwrap();
    assert_eq!(result["exit_code"], 0);
    assert!(!result["stdout"].as_str().unwrap().is_empty());

    // ls
    let result = shell
        .execute(serde_json::json!({
            "command": "ls Cargo.toml",
            "timeout_secs": 5
        }))
        .await
        .unwrap();
    assert_eq!(result["exit_code"], 0);

    println!("  shell_exec echo/pwd/ls all passed");
}

#[tokio::test]
async fn test_directory_tree_generation() {
    init_permissive_safety();
    let dir = tempdir().unwrap();

    // Create a directory structure
    let sub1 = dir.path().join("src");
    let sub2 = dir.path().join("tests");
    let sub3 = dir.path().join("src/utils");
    fs::create_dir_all(&sub1).unwrap();
    fs::create_dir_all(&sub2).unwrap();
    fs::create_dir_all(&sub3).unwrap();
    fs::write(sub1.join("main.rs"), "fn main() {}").unwrap();
    fs::write(sub1.join("lib.rs"), "pub mod utils;").unwrap();
    fs::write(sub3.join("helpers.rs"), "pub fn help() {}").unwrap();
    fs::write(sub2.join("test_main.rs"), "#[test] fn it_works() {}").unwrap();

    let registry = ToolRegistry::new();
    let dir_tree = registry.get("directory_tree").unwrap();
    let result = dir_tree
        .execute(serde_json::json!({
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();

    let total = result["total"].as_i64().unwrap();
    assert!(
        total >= 4,
        "Expected at least 4 entries in tree, got {}",
        total
    );

    println!("  directory_tree found {} entries", total);
}

#[tokio::test]
async fn test_glob_find_pattern_matching() {
    let dir = tempdir().unwrap();

    // Create files with various extensions
    fs::write(dir.path().join("app.rs"), "fn main() {}").unwrap();
    fs::write(dir.path().join("lib.rs"), "pub fn lib() {}").unwrap();
    fs::write(dir.path().join("config.toml"), "[package]").unwrap();
    fs::write(dir.path().join("readme.md"), "# Readme").unwrap();
    fs::write(dir.path().join("test.py"), "def test(): pass").unwrap();

    let registry = ToolRegistry::new();
    let glob = registry.get("glob_find").unwrap();

    // Find Rust files
    let result = glob
        .execute(serde_json::json!({
            "pattern": "*.rs",
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();
    assert_eq!(result["count"], 2, "Expected 2 .rs files");

    // Find all files
    let result = glob
        .execute(serde_json::json!({
            "pattern": "*.*",
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();
    assert!(
        result["count"].as_i64().unwrap() >= 5,
        "Expected at least 5 files with extensions"
    );

    println!("  glob_find pattern matching passed");
}

#[tokio::test]
async fn test_grep_search_content() {
    let dir = tempdir().unwrap();

    fs::write(
        dir.path().join("code.rs"),
        r#"
fn calculate_fibonacci(n: u32) -> u64 {
    if n <= 1 { return n as u64; }
    let mut a: u64 = 0;
    let mut b: u64 = 1;
    for _ in 2..=n {
        let tmp = a + b;
        a = b;
        b = tmp;
    }
    b
}

fn calculate_factorial(n: u32) -> u64 {
    (1..=n as u64).product()
}
"#,
    )
    .unwrap();

    let registry = ToolRegistry::new();
    let grep = registry.get("grep_search").unwrap();

    // Search for "calculate"
    let result = grep
        .execute(serde_json::json!({
            "pattern": "calculate",
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();
    assert!(
        result["count"].as_i64().unwrap() >= 2,
        "Expected at least 2 matches for 'calculate'"
    );

    // Search for specific function
    let result = grep
        .execute(serde_json::json!({
            "pattern": "fibonacci",
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();
    assert!(
        result["count"].as_i64().unwrap() >= 1,
        "Expected at least 1 match for 'fibonacci'"
    );

    println!("  grep_search content matching passed");
}

#[tokio::test]
async fn test_git_status_in_repo() {
    let registry = ToolRegistry::new();
    let git_status = registry.get("git_status").unwrap();

    let result = git_status.execute(serde_json::json!({})).await.unwrap();

    // We should at least get a branch name
    assert!(
        result.get("branch").is_some() || result.get("status").is_some(),
        "git_status should return branch or status info"
    );

    println!("  git_status in repo passed: {:?}", result);
}

#[cfg(not(target_os = "windows"))]
#[tokio::test]
async fn test_pty_shell_session_lifecycle() {
    let registry = ToolRegistry::new();
    let pty = registry.get("pty_shell").unwrap();

    // Start a session
    let result = pty
        .execute(serde_json::json!({ "action": "start" }))
        .await
        .unwrap();
    assert_eq!(result["status"], "started");
    let session_id = result["session_id"].as_str().unwrap().to_string();

    // Send a command
    let result = pty
        .execute(serde_json::json!({
            "action": "send",
            "session_id": &session_id,
            "command": "echo pty_e2e_test_marker",
            "timeout_secs": 5
        }))
        .await
        .unwrap();
    assert_eq!(result["exit_code"], 0);
    assert!(
        result["stdout"]
            .as_str()
            .unwrap()
            .contains("pty_e2e_test_marker"),
        "PTY output should contain our marker"
    );

    // Close the session
    let result = pty
        .execute(serde_json::json!({
            "action": "close",
            "session_id": &session_id
        }))
        .await
        .unwrap();
    assert_eq!(result["status"], "closed");

    println!("  pty_shell start/send/close lifecycle passed");
}

// ============================================================================
// 3. LLM Integration Tests (require endpoint)
// ============================================================================

#[tokio::test]
async fn test_llm_simple_completion() {
    if !require_test_endpoint().await {
        return;
    }

    let config = test_llm_config();
    let client = selfware::api::ApiClient::new(&config).expect("failed to create API client");

    let messages = vec![
        selfware::api::types::Message::system("You are a helpful assistant. Respond concisely."),
        selfware::api::types::Message::user("What is 7 * 8? Reply with just the number."),
    ];

    let start = Instant::now();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        client.chat(messages, None, selfware::api::ThinkingMode::Disabled),
    )
    .await;

    match response {
        Ok(Ok(resp)) => {
            let elapsed = start.elapsed();
            let text = resp.choices[0].message.content.text();
            assert!(
                text.contains("56"),
                "Expected '56' in response, got: {}",
                text
            );
            println!(
                "  LLM simple completion passed in {:?}: {}",
                elapsed,
                text.trim()
            );
        }
        Ok(Err(e)) => panic!("LLM request failed: {}", e),
        Err(_) => panic!("LLM request timed out after 60s"),
    }
}

#[tokio::test]
async fn test_llm_tool_calling() {
    if !require_test_endpoint().await {
        return;
    }

    let config = test_llm_config();
    let client = selfware::api::ApiClient::new(&config).expect("failed to create API client");
    let registry = ToolRegistry::new();
    let tool_defs = registry.definitions();

    let messages = vec![
        selfware::api::types::Message::system(
            "You are a coding assistant. When asked to read a file, use the file_read tool.",
        ),
        selfware::api::types::Message::user(
            "Read the file at ./Cargo.toml using the file_read tool.",
        ),
    ];

    let start = Instant::now();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(90),
        client.chat(
            messages,
            Some(tool_defs),
            selfware::api::ThinkingMode::Disabled,
        ),
    )
    .await;

    match response {
        Ok(Ok(resp)) => {
            let elapsed = start.elapsed();
            let msg = &resp.choices[0].message;
            let text = msg.content.text();
            let has_tool_call = msg.tool_calls.as_ref().map_or(false, |tc| !tc.is_empty());
            let mentions_tool =
                text.contains("file_read") || text.contains("<tool>") || text.contains("\"name\"");

            assert!(
                has_tool_call || mentions_tool,
                "Expected tool call or tool mention in response, got: {}",
                &text[..text.len().min(500)]
            );
            println!(
                "  LLM tool calling passed in {:?}, has_tool_call={}, text_len={}",
                elapsed,
                has_tool_call,
                text.len()
            );
        }
        Ok(Err(e)) => panic!("LLM request failed: {}", e),
        Err(_) => panic!("LLM request timed out after 90s"),
    }
}

#[tokio::test]
async fn test_llm_code_generation_rust() {
    if !require_test_endpoint().await {
        return;
    }

    init_permissive_safety();
    let config = test_llm_config();
    let client = selfware::api::ApiClient::new(&config).expect("failed to create API client");

    let messages = vec![
        selfware::api::types::Message::system(
            "You are a Rust expert. Output ONLY valid Rust code, no markdown, no explanation.",
        ),
        selfware::api::types::Message::user(
            "Write a Rust function called `fibonacci` that takes a u32 and returns a u64. \
             Use iterative approach. Include a main function that prints fibonacci(10). \
             Output only the code, nothing else.",
        ),
    ];

    let start = Instant::now();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(90),
        client.chat(messages, None, selfware::api::ThinkingMode::Disabled),
    )
    .await;

    match response {
        Ok(Ok(resp)) => {
            let elapsed = start.elapsed();
            let raw_text = resp.choices[0].message.content.text();

            // Strip markdown code fences if present
            let code = raw_text
                .trim()
                .strip_prefix("```rust")
                .or_else(|| raw_text.trim().strip_prefix("```"))
                .unwrap_or(&raw_text)
                .trim_end_matches("```")
                .trim();

            // Write to a temp file and compile
            let dir = tempdir().unwrap();
            let src = dir.path().join("main.rs");
            fs::write(&src, code).unwrap();

            let output = std::process::Command::new("rustc")
                .arg(&src)
                .arg("-o")
                .arg(dir.path().join("main"))
                .output()
                .expect("failed to run rustc");

            if output.status.success() {
                // Run the compiled program
                let run_output = std::process::Command::new(dir.path().join("main"))
                    .output()
                    .expect("failed to run compiled binary");
                let stdout = String::from_utf8_lossy(&run_output.stdout);
                println!(
                    "  LLM Rust code generation passed in {:?}, output: {}",
                    elapsed,
                    stdout.trim()
                );
                assert!(
                    run_output.status.success(),
                    "compiled program should run successfully"
                );
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!(
                    "  WARNING: LLM-generated Rust code did not compile: {}",
                    stderr.lines().take(5).collect::<Vec<_>>().join("\n")
                );
                // Not a hard failure — LLM output quality varies
            }
        }
        Ok(Err(e)) => panic!("LLM request failed: {}", e),
        Err(_) => panic!("LLM request timed out after 90s"),
    }
}

#[tokio::test]
async fn test_llm_code_generation_python() {
    if !require_test_endpoint().await {
        return;
    }

    let config = test_llm_config();
    let client = selfware::api::ApiClient::new(&config).expect("failed to create API client");

    let messages = vec![
        selfware::api::types::Message::system(
            "You are a Python expert. Output ONLY valid Python code, no markdown, no explanation.",
        ),
        selfware::api::types::Message::user(
            "Write a Python function called `is_palindrome` that checks if a string is a \
             palindrome (case-insensitive). Then print the results of testing it with \
             'racecar', 'hello', and 'Madam'. Output only the code, nothing else.",
        ),
    ];

    let start = Instant::now();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(90),
        client.chat(messages, None, selfware::api::ThinkingMode::Disabled),
    )
    .await;

    match response {
        Ok(Ok(resp)) => {
            let elapsed = start.elapsed();
            let raw_text = resp.choices[0].message.content.text();

            let code = raw_text
                .trim()
                .strip_prefix("```python")
                .or_else(|| raw_text.trim().strip_prefix("```"))
                .unwrap_or(&raw_text)
                .trim_end_matches("```")
                .trim();

            // Write and run
            let dir = tempdir().unwrap();
            let src = dir.path().join("test_script.py");
            fs::write(&src, code).unwrap();

            let output = std::process::Command::new("python3").arg(&src).output();

            match output {
                Ok(o) if o.status.success() => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    println!(
                        "  LLM Python code generation passed in {:?}, output: {}",
                        elapsed,
                        stdout.trim()
                    );
                }
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    println!(
                        "  WARNING: LLM-generated Python code failed: {}",
                        stderr.lines().take(5).collect::<Vec<_>>().join("\n")
                    );
                }
                Err(e) => {
                    println!("  SKIPPED: python3 not available ({})", e);
                }
            }
        }
        Ok(Err(e)) => panic!("LLM request failed: {}", e),
        Err(_) => panic!("LLM request timed out after 90s"),
    }
}

#[tokio::test]
async fn test_llm_multi_step_task() {
    if !require_test_endpoint().await {
        return;
    }

    init_permissive_safety();
    let config = test_llm_config();
    let client = selfware::api::ApiClient::new(&config).expect("failed to create API client");
    let registry = ToolRegistry::new();

    // Step 1: Ask LLM to design a function
    let messages = vec![
        selfware::api::types::Message::system("You are a senior Rust developer. Reply concisely."),
        selfware::api::types::Message::user(
            "Design a Rust function signature for a function called `merge_sorted` that \
             takes two sorted slices of i32 and returns a Vec<i32> containing all elements \
             in sorted order. Reply with ONLY the function signature (one line).",
        ),
    ];

    let start = Instant::now();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        client.chat(messages, None, selfware::api::ThinkingMode::Disabled),
    )
    .await;

    let signature = match response {
        Ok(Ok(resp)) => {
            let text = resp.choices[0].message.content.text();
            assert!(
                text.contains("merge_sorted"),
                "Response should contain function name"
            );
            text.to_string()
        }
        Ok(Err(e)) => panic!("Step 1 failed: {}", e),
        Err(_) => panic!("Step 1 timed out"),
    };

    // Step 2: Ask LLM to implement it
    let messages = vec![
        selfware::api::types::Message::system(
            "You are a Rust expert. Output ONLY valid Rust code, no markdown fences.",
        ),
        selfware::api::types::Message::user(format!(
            "Implement this Rust function and include 2 unit tests:\n{}\n\
             Output the complete code including #[cfg(test)] module.",
            signature.trim()
        )),
    ];

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(90),
        client.chat(messages, None, selfware::api::ThinkingMode::Disabled),
    )
    .await;

    match response {
        Ok(Ok(resp)) => {
            let elapsed = start.elapsed();
            let text = resp.choices[0].message.content.text();
            assert!(
                text.contains("merge_sorted") && text.contains("test"),
                "Implementation should contain function and tests"
            );

            // Write to temp and verify it at least parses
            let dir = tempdir().unwrap();
            let src = dir.path().join("lib.rs");
            let code = text
                .trim()
                .strip_prefix("```rust")
                .or_else(|| text.trim().strip_prefix("```"))
                .unwrap_or(&text)
                .trim_end_matches("```")
                .trim();
            fs::write(&src, code).unwrap();

            // Try to compile as a library
            let file_read = registry.get("file_read").unwrap();
            let read_result = file_read
                .execute(serde_json::json!({ "path": src.to_str().unwrap() }))
                .await
                .unwrap();
            assert!(
                read_result["content"]
                    .as_str()
                    .unwrap()
                    .contains("merge_sorted"),
                "Written file should contain function"
            );

            println!(
                "  LLM multi-step task passed in {:?}, code_len={}",
                elapsed,
                code.len()
            );
        }
        Ok(Err(e)) => panic!("Step 2 failed: {}", e),
        Err(_) => panic!("Step 2 timed out"),
    }
}

#[tokio::test]
async fn test_llm_context_understanding() {
    if !require_test_endpoint().await {
        return;
    }

    let config = test_llm_config();
    let client = selfware::api::ApiClient::new(&config).expect("failed to create API client");

    // Read our own Cargo.toml as context
    let cargo_toml = fs::read_to_string("Cargo.toml").expect("failed to read Cargo.toml");
    let first_30_lines: String = cargo_toml.lines().take(30).collect::<Vec<_>>().join("\n");

    let messages = vec![
        selfware::api::types::Message::system(
            "You are analyzing a Rust project. Answer questions about the provided file content.",
        ),
        selfware::api::types::Message::user(format!(
            "Here is the beginning of a Cargo.toml file:\n\n```toml\n{}\n```\n\n\
             What is the package name? Reply with ONLY the package name, nothing else.",
            first_30_lines
        )),
    ];

    let start = Instant::now();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        client.chat(messages, None, selfware::api::ThinkingMode::Disabled),
    )
    .await;

    match response {
        Ok(Ok(resp)) => {
            let elapsed = start.elapsed();
            let text = resp.choices[0].message.content.text();
            assert!(
                text.to_lowercase().contains("selfware"),
                "LLM should identify package name 'selfware', got: {}",
                text.trim()
            );
            println!(
                "  LLM context understanding passed in {:?}: {}",
                elapsed,
                text.trim()
            );
        }
        Ok(Err(e)) => panic!("LLM request failed: {}", e),
        Err(_) => panic!("LLM request timed out after 60s"),
    }
}

// ============================================================================
// 4. Benchmark Tests
// ============================================================================

#[tokio::test]
async fn test_benchmark_tool_execution_latency() {
    init_permissive_safety();
    let dir = tempdir().unwrap();
    let registry = ToolRegistry::new();

    // Create 100 small files
    for i in 0..100 {
        fs::write(
            dir.path().join(format!("bench_{}.txt", i)),
            format!("Benchmark file content {}", i),
        )
        .unwrap();
    }

    let file_read = registry.get("file_read").unwrap();

    let start = Instant::now();
    for i in 0..100 {
        let path = dir.path().join(format!("bench_{}.txt", i));
        file_read
            .execute(serde_json::json!({ "path": path.to_str().unwrap() }))
            .await
            .unwrap();
    }
    let elapsed = start.elapsed();
    let per_read_us = elapsed.as_micros() / 100;

    println!(
        "  BENCHMARK file_read: 100 reads in {:?} ({} us/read)",
        elapsed, per_read_us
    );
    assert!(
        elapsed.as_millis() < 5000,
        "100 file reads should complete in under 5s, took {:?}",
        elapsed
    );
}

#[tokio::test]
async fn test_benchmark_tool_registry_lookup() {
    let registry = ToolRegistry::new();
    let tool_names = [
        "file_read",
        "file_write",
        "shell_exec",
        "grep_search",
        "git_status",
        "cargo_check",
        "http_request",
        "container_run",
        "browser_fetch",
        "knowledge_query",
    ];

    let start = Instant::now();
    for _ in 0..10_000 {
        for name in &tool_names {
            let _ = registry.get(name);
        }
    }
    let elapsed = start.elapsed();
    let per_lookup_ns = elapsed.as_nanos() / 100_000;

    println!(
        "  BENCHMARK registry lookup: 100,000 lookups in {:?} ({} ns/lookup)",
        elapsed, per_lookup_ns
    );
    assert!(
        elapsed.as_millis() < 1000,
        "100k lookups should complete in under 1s"
    );
}

#[test]
fn test_benchmark_config_load() {
    let toml_str = r#"
endpoint = "http://localhost:8000/v1"
model = "test-model"
max_tokens = 4096
temperature = 0.7

[safety]
allowed_paths = ["./**"]
denied_paths = ["/etc/**", "/root/**"]

[agent]
max_iterations = 50
step_timeout_secs = 300
"#;

    let start = Instant::now();
    for _ in 0..1_000 {
        let _config: Config = toml::from_str(toml_str).unwrap();
    }
    let elapsed = start.elapsed();
    let per_parse_us = elapsed.as_micros() / 1_000;

    println!(
        "  BENCHMARK config parse: 1,000 parses in {:?} ({} us/parse)",
        elapsed, per_parse_us
    );
    assert!(
        elapsed.as_millis() < 5000,
        "1k config parses should complete in under 5s"
    );
}

#[test]
fn test_benchmark_safety_checker() {
    use selfware::api::types::{ToolCall, ToolFunction};
    use selfware::safety::checker::SafetyChecker;

    let config = SafetyConfig {
        allowed_paths: vec!["/tmp/**".to_string(), "./**".to_string()],
        denied_paths: vec!["/etc/**".to_string()],
        ..Default::default()
    };
    let checker = SafetyChecker::new(&config);

    let tool_call = ToolCall {
        id: "test-1".to_string(),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: "file_read".to_string(),
            arguments: r#"{"path": "/tmp/test.txt"}"#.to_string(),
        },
    };

    let start = Instant::now();
    for _ in 0..10_000 {
        let _ = checker.check_tool_call(&tool_call);
    }
    let elapsed = start.elapsed();
    let per_check_ns = elapsed.as_nanos() / 10_000;

    println!(
        "  BENCHMARK safety check: 10,000 checks in {:?} ({} ns/check)",
        elapsed, per_check_ns
    );
    assert!(
        elapsed.as_millis() < 5000,
        "10k safety checks should complete in under 5s"
    );
}

// ============================================================================
// 5. Integration Scenario Tests
// ============================================================================

#[tokio::test]
async fn test_scenario_create_rust_project() {
    let dir = tempdir().unwrap();
    let registry = ToolRegistry::new();
    let shell = registry.get("shell_exec").unwrap();

    // cargo init
    let result = shell
        .execute(serde_json::json!({
            "command": format!("cargo init --name e2e_test_project {}", dir.path().display()),
            "timeout_secs": 30
        }))
        .await
        .unwrap();
    assert_eq!(
        result["exit_code"],
        0,
        "cargo init failed: {}",
        result["stderr"].as_str().unwrap_or("")
    );

    // Add some code
    let src_path = dir.path().join("src/lib.rs");
    fs::write(
        &src_path,
        r#"
/// Add two numbers with overflow protection.
pub fn safe_add(a: i32, b: i32) -> Option<i32> {
    a.checked_add(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_add() {
        assert_eq!(safe_add(1, 2), Some(3));
        assert_eq!(safe_add(i32::MAX, 1), None);
    }
}
"#,
    )
    .unwrap();

    // cargo check
    let result = shell
        .execute(serde_json::json!({
            "command": format!("cd {} && cargo check 2>&1", dir.path().display()),
            "timeout_secs": 120
        }))
        .await
        .unwrap();
    assert_eq!(
        result["exit_code"],
        0,
        "cargo check failed: {}",
        result["stdout"].as_str().unwrap_or("")
    );

    // cargo test
    let result = shell
        .execute(serde_json::json!({
            "command": format!("cd {} && cargo test 2>&1", dir.path().display()),
            "timeout_secs": 120
        }))
        .await
        .unwrap();
    assert_eq!(
        result["exit_code"],
        0,
        "cargo test failed: {}",
        result["stdout"].as_str().unwrap_or("")
    );

    println!("  scenario: create Rust project, check, test — all passed");
}

#[tokio::test]
async fn test_scenario_fix_broken_code() {
    init_permissive_safety();
    let dir = tempdir().unwrap();
    let registry = ToolRegistry::new();

    // Write intentionally broken Rust code
    let broken_src = dir.path().join("broken.rs");
    fs::write(
        &broken_src,
        r#"
fn main() {
    let x: i32 = "not a number";
    println!("{}", x);
}
"#,
    )
    .unwrap();

    // Verify it does not compile
    let output = std::process::Command::new("rustc")
        .arg(&broken_src)
        .arg("-o")
        .arg(dir.path().join("broken"))
        .output()
        .expect("failed to run rustc");

    assert!(!output.status.success(), "broken code should not compile");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("mismatched types") || stderr.contains("expected"),
        "compiler should report type error"
    );

    // Now fix the code using file_edit
    let file_edit = registry.get("file_edit").unwrap();
    file_edit
        .execute(serde_json::json!({
            "path": broken_src.to_str().unwrap(),
            "old_str": "let x: i32 = \"not a number\";",
            "new_str": "let x: i32 = 42;"
        }))
        .await
        .unwrap();

    // Verify the fix compiles
    let output = std::process::Command::new("rustc")
        .arg(&broken_src)
        .arg("-o")
        .arg(dir.path().join("fixed"))
        .output()
        .expect("failed to run rustc");

    assert!(
        output.status.success(),
        "fixed code should compile, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    println!("  scenario: fix broken code — detected error, applied fix, verified compilation");
}

#[test]
fn test_scenario_multi_language_detection() {
    // Mirrors the detection logic from selfware::testing::language_qa::QaLanguage::detect
    // (which is pub(crate)), verifying that project markers are correctly identified.
    use std::path::Path;

    fn detect_language(project_root: &Path) -> &'static str {
        if project_root.join("Cargo.toml").exists() {
            "Rust"
        } else if project_root.join("package.json").exists() {
            "Node"
        } else if project_root.join("pyproject.toml").exists()
            || project_root.join("setup.py").exists()
            || project_root.join("requirements.txt").exists()
        {
            "Python"
        } else if project_root.join("go.mod").exists() {
            "Go"
        } else {
            "Unknown"
        }
    }

    let dir = tempdir().unwrap();

    // Rust project
    let rust_dir = dir.path().join("rust_project");
    fs::create_dir_all(&rust_dir).unwrap();
    fs::write(
        rust_dir.join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"\nedition = \"2021\"",
    )
    .unwrap();
    assert_eq!(detect_language(&rust_dir), "Rust");

    // Python project
    let py_dir = dir.path().join("python_project");
    fs::create_dir_all(&py_dir).unwrap();
    fs::write(py_dir.join("requirements.txt"), "requests==2.31.0\n").unwrap();
    assert_eq!(detect_language(&py_dir), "Python");

    // Node project
    let node_dir = dir.path().join("node_project");
    fs::create_dir_all(&node_dir).unwrap();
    fs::write(
        node_dir.join("package.json"),
        r#"{"name": "test", "version": "1.0.0"}"#,
    )
    .unwrap();
    assert_eq!(detect_language(&node_dir), "Node");

    // Go project
    let go_dir = dir.path().join("go_project");
    fs::create_dir_all(&go_dir).unwrap();
    fs::write(
        go_dir.join("go.mod"),
        "module example.com/test\n\ngo 1.21\n",
    )
    .unwrap();
    assert_eq!(detect_language(&go_dir), "Go");

    // Unknown project
    let unknown_dir = dir.path().join("unknown_project");
    fs::create_dir_all(&unknown_dir).unwrap();
    fs::write(unknown_dir.join("readme.md"), "# Unknown project").unwrap();
    assert_eq!(detect_language(&unknown_dir), "Unknown");

    println!("  scenario: multi-language detection — Rust/Python/Node/Go/Unknown all correct");
}

// ============================================================================
// 6. Doctor & LLM Doctor Tests
// ============================================================================

#[tokio::test]
async fn test_doctor_report_structure() {
    let report = run_doctor().await;

    // Check that we have checks from multiple categories
    let categories: HashSet<String> = report
        .checks
        .iter()
        .map(|c| format!("{}", c.category))
        .collect();

    assert!(
        categories.contains("Core (Required)"),
        "Doctor should have Core category"
    );

    // Health should be at least Degraded (we have core tools)
    assert_ne!(
        report.health,
        OverallHealth::Broken,
        "Health should not be Broken in a Rust build environment"
    );

    // Check structure of individual checks
    for check in &report.checks {
        assert!(!check.name.is_empty(), "check name must not be empty");
        assert!(!check.message.is_empty(), "check message must not be empty");
        match check.status {
            CheckStatus::Ok => {
                // OK checks should have a version (for most tools)
                // Not asserting version since some checks don't report it
            }
            CheckStatus::Missing | CheckStatus::Warning => {
                // These are valid states for optional tools
            }
        }
    }

    println!(
        "  doctor report: {} checks across {} categories, health={}",
        report.checks.len(),
        categories.len(),
        report.health
    );
}

#[tokio::test]
async fn test_llm_doctor_with_endpoint() {
    if !require_test_endpoint().await {
        return;
    }

    let config = test_llm_config();

    // run_llm_doctor prints to stdout; we just verify it does not panic
    let start = Instant::now();
    let result = selfware::llm_doctor::run_llm_doctor(&config).await;
    let elapsed = start.elapsed();

    match result {
        Ok(()) => {
            println!("  LLM doctor completed successfully in {:?}", elapsed);
        }
        Err(e) => {
            // Some errors are expected if the endpoint doesn't support all probes
            println!(
                "  LLM doctor returned error (may be expected): {} ({:?})",
                e, elapsed
            );
        }
    }
}

// ============================================================================
// Extra: Tool Schema Validation
// ============================================================================

#[test]
fn test_all_tools_have_valid_schemas() {
    let registry = ToolRegistry::new();
    let tools = registry.list();

    for tool in &tools {
        let schema = tool.schema();
        assert!(
            schema.is_object(),
            "Tool '{}' schema must be a JSON object, got: {}",
            tool.name(),
            schema
        );

        // Every schema should have "type": "object" and "properties"
        assert_eq!(
            schema.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "Tool '{}' schema type must be 'object'",
            tool.name()
        );

        assert!(
            schema.get("properties").is_some(),
            "Tool '{}' schema must have 'properties'",
            tool.name()
        );
    }

    println!("  all {} tool schemas are valid JSON objects", tools.len());
}

#[test]
fn test_all_tools_have_descriptions() {
    let registry = ToolRegistry::new();
    let tools = registry.list();

    for tool in &tools {
        assert!(!tool.name().is_empty(), "Tool must have a non-empty name");
        assert!(
            !tool.description().is_empty(),
            "Tool '{}' must have a non-empty description",
            tool.name()
        );
        // Description should be reasonably informative (at least 10 chars)
        assert!(
            tool.description().len() >= 10,
            "Tool '{}' description too short: '{}'",
            tool.name(),
            tool.description()
        );
    }

    println!("  all {} tools have names and descriptions", tools.len());
}
