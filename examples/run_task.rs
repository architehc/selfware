//! Run Task Example
//!
//! This example demonstrates running a specific coding task with Selfware.
//! It shows how to:
//! 1. Configure the agent programmatically
//! 2. Run a task that involves file operations
//! 3. Use checkpointing for task persistence
//!
//! # Running this example
//!
//! ```bash
//! # Ensure your LLM backend is running
//! cargo run --example run_task
//! ```
//!
//! # What this example does
//!
//! The agent will create a simple Rust function, demonstrating:
//! - File creation with `file_write` tool
//! - Code verification with `cargo_check` tool
//! - Test execution with `cargo_test` tool (if tests are created)

use anyhow::Result;
use selfware::agent::Agent;
use selfware::config::{AgentConfig, Config, ExecutionMode, SafetyConfig};

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Selfware Run Task Example ===\n");

    // Create configuration programmatically
    // This is useful when you want to override defaults
    let config = Config {
        // LLM endpoint - change this to your backend
        endpoint: std::env::var("SELFWARE_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:8000/v1".to_string()),

        // Model name
        model: std::env::var("SELFWARE_MODEL")
            .unwrap_or_else(|_| "Qwen/Qwen3-Coder-Next-FP8".to_string()),

        // Token limits
        max_tokens: 65536,
        temperature: 0.7,

        // API key (if required by your backend)
        api_key: std::env::var("SELFWARE_API_KEY").ok(),

        // Safety configuration
        safety: SafetyConfig {
            // Allow operations in current directory and subdirectories
            allowed_paths: vec!["./**".to_string()],
            // Prevent access to sensitive files
            denied_paths: vec!["**/.env".to_string(), "**/secrets/**".to_string()],
            // Protect main branches from direct pushes
            protected_branches: vec!["main".to_string(), "master".to_string()],
            // Tools that require explicit confirmation in normal mode
            require_confirmation: vec![
                "git_push".to_string(),
                "file_delete".to_string(),
                "shell_exec".to_string(),
            ],
        },

        // Agent behavior
        agent: AgentConfig {
            // Maximum iterations before stopping
            max_iterations: 50,
            // Timeout per LLM request (5 minutes for slow models)
            step_timeout_secs: 300,
            // Total token budget for the session
            token_budget: 500000,
            // Use XML-based tool parsing (works with all backends)
            native_function_calling: false,
            // Enable streaming for real-time output
            streaming: true,
        },

        // Use AutoEdit mode for this example (auto-approve file edits)
        execution_mode: ExecutionMode::AutoEdit,

        // YOLO mode settings (not used in AutoEdit mode)
        yolo: Default::default(),

        // CLI-only flags (not persisted in config)
        compact_mode: false,
        verbose_mode: false,
        show_tokens: false,
    };

    println!("Configuration:");
    println!("  Endpoint: {}", config.endpoint);
    println!("  Model: {}", config.model);
    println!("  Mode: {:?}", config.execution_mode);
    println!("  Max iterations: {}", config.agent.max_iterations);
    println!();

    // Create the agent with our configuration
    let mut agent = Agent::new(config).await?;

    // Define the task
    let task = r#"
        Create a simple Rust module that implements a basic calculator.
        The module should:
        1. Create a file called 'calculator.rs' in the current directory
        2. Implement add, subtract, multiply, and divide functions
        3. Include proper error handling for division by zero
        4. Add documentation comments for each function

        After creating the file, verify it compiles correctly.
    "#;

    println!("Running task:\n{}\n", task.trim());
    println!("--- Agent Output ---\n");

    // Run the task
    // The agent will:
    // 1. Plan the approach
    // 2. Create the calculator.rs file
    // 3. Verify compilation
    // 4. Report completion
    let start = std::time::Instant::now();
    agent.run_task(task).await?;
    let duration = start.elapsed();

    println!("\n--- Task Complete ---");
    println!("Duration: {:.2}s", duration.as_secs_f64());

    // Clean up (optional) - remove the created file
    if std::path::Path::new("calculator.rs").exists() {
        println!("\nNote: calculator.rs was created. You can:");
        println!("  - Run: cargo check --lib (to verify)");
        println!("  - Run: rm calculator.rs (to clean up)");
    }

    Ok(())
}
