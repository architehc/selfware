//! Basic Chat Interaction Example
//!
//! This example demonstrates how to create a simple chat interaction with Selfware.
//! It shows the fundamental pattern of:
//! 1. Loading configuration
//! 2. Creating an API client
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
use selfware::api::types::Message;
use selfware::api::{ApiClient, ThinkingMode};
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

    // Create a direct API client.
    // Using direct chat in this example avoids tool/confirmation behavior
    // so it works reliably in non-interactive environments.
    let client = ApiClient::new(&config)?;

    println!("Sending prompt to model...\n");
    let messages = vec![
        Message::system(
            "You are a helpful Rust assistant. Reply directly without using tools or external actions.",
        ),
        Message::user("What's a simple way to check if a number is prime in Rust? Show me the code."),
    ];

    let response = client.chat(messages, None, ThinkingMode::Disabled).await?;
    let answer = &response.choices[0].message.content;

    println!("Assistant response:\n{}\n", answer);
    println!("=== Task Complete ===");

    Ok(())
}
