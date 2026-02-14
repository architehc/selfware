//! Basic Chat Interaction Example
//!
//! This example demonstrates how to create a simple chat interaction with Selfware.
//! It shows the fundamental pattern of:
//! 1. Loading configuration
//! 2. Creating an agent
//! 3. Sending a message and receiving a response
//!
//! # Running this example
//!
//! Make sure you have an LLM backend running (e.g., vLLM, Ollama):
//! ```bash
//! # Start vLLM with a compatible model
//! vllm serve Qwen/Qwen3-Coder-Next-FP8
//!
//! # Run the example
//! cargo run --example basic_chat
//! ```
//!
//! # Configuration
//!
//! The agent will look for configuration in this order:
//! 1. `selfware.toml` in the current directory
//! 2. `~/.config/selfware/config.toml`
//! 3. Environment variables (SELFWARE_ENDPOINT, SELFWARE_MODEL, etc.)
//! 4. Built-in defaults

use anyhow::Result;
use selfware::agent::Agent;
use selfware::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging (optional, but helpful for debugging)
    // Set SELFWARE_DEBUG=1 for verbose output
    if std::env::var("SELFWARE_DEBUG").is_ok() {
        tracing_subscriber::fmt()
            .with_env_filter("selfware=debug")
            .init();
    }

    println!("=== Selfware Basic Chat Example ===\n");

    // Load configuration from default locations
    // You can also pass a specific path: Config::load(Some("custom.toml"))
    let config = Config::load(None)?;

    println!("Using endpoint: {}", config.endpoint);
    println!("Using model: {}", config.model);
    println!();

    // Create the agent
    // The agent initializes with:
    // - API client for LLM communication
    // - Tool registry with 53+ built-in tools
    // - Safety checker for path/command validation
    // - Memory system for context management
    let mut agent = Agent::new(config).await?;

    // Run a simple task
    // The agent will:
    // 1. Send the task to the LLM
    // 2. Parse any tool calls from the response
    // 3. Execute tools with safety checks
    // 4. Continue the conversation until the task is complete
    println!("Sending task to agent...\n");

    agent
        .run_task("What's a simple way to check if a number is prime in Rust? Show me the code.")
        .await?;

    println!("\n=== Task Complete ===");

    Ok(())
}
