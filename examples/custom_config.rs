//! Example: Building a Custom Configuration
//!
//! This example demonstrates how to create and use a custom Selfware configuration
//! programmatically, without relying on a TOML file.

use selfware::config::{AgentConfig, Config, SafetyConfig};
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a custom configuration programmatically
    let config = build_custom_config();

    // Print the configuration as TOML for reference
    print_sample_toml();

    // Use the configuration
    println!("Configuration built successfully!");
    println!("Endpoint: {}", config.endpoint);
    println!("Model: {}", config.model);
    println!("Max tokens: {}", config.max_tokens);
    println!("Temperature: {}", config.temperature);

    // Check environment for API key
    if let Ok(api_key) = env::var("SELFWARE_API_KEY") {
        println!("API Key: {}***", &api_key[..std::cmp::min(4, api_key.len())]);
    } else {
        println!("No API key set (use SELFWARE_API_KEY environment variable)");
    }

    Ok(())
}

/// Build a custom configuration programmatically
fn build_custom_config() -> Config {
    Config {
        // API settings
        endpoint: "http://localhost:8000/v1".to_string(),
        model: "Qwen/Qwen3-Coder-Next-FP8".to_string(),
        max_tokens: 32768,
        temperature: 0.7,
        api_key: None,

        // Safety settings
        safety: SafetyConfig {
            allowed_paths: vec![
                "./**".to_string(),             // Current directory
                "/tmp/selfware/**".to_string(), // Temp workspace
            ],
            denied_paths: vec![
                "**/.env".to_string(),
                "**/.env.*".to_string(),
                "**/secrets/**".to_string(),
                "**/*.pem".to_string(),
                "**/*.key".to_string(),
            ],
            protected_branches: vec![
                "main".to_string(),
                "master".to_string(),
                "production".to_string(),
            ],
            require_confirmation: vec![
                "git_push".to_string(),
                "file_delete".to_string(),
                "shell_exec".to_string(),
                "container_exec".to_string(),
            ],
            strict_permissions: false,
            permissions: vec![],
            protected_files: vec![],
        },

        // Agent behavior
        agent: AgentConfig {
            max_iterations: 100,
            step_timeout_secs: 300, // 5 minutes
            token_budget: 500000,
            token_safety_margin: 50000,
            native_function_calling: false,
            streaming: true,
            min_completion_steps: 3,
            require_verification_before_completion: true,
            context_content_ratio: 0.7,
            context_compression_ratio: 0.15,
        },

        // Output settings
        output: Default::default(),

        // Resource limits
        resources: Default::default(),
    }
}

/// Print a sample TOML configuration file
fn print_sample_toml() {
    let toml = r#"[api]
endpoint = "http://localhost:8000/v1"
model = "Qwen/Qwen3-Coder-Next-FP8"
max_tokens = 32768
temperature = 0.7

[safety]
allowed_paths = ["./**", "/tmp/selfware/**"]
denied_paths = ["**/.env", "**/.env.*", "**/secrets/**", "**/*.pem", "**/*.key"]
protected_branches = ["main", "master", "production"]
require_confirmation = ["git_push", "file_delete", "shell_exec", "container_exec"]
strict_permissions = false

[agent]
max_iterations = 100
step_timeout_secs = 300
token_budget = 500000
token_safety_margin = 50000
native_function_calling = false
streaming = true
min_completion_steps = 3
require_verification_before_completion = true
context_content_ratio = 0.7
context_compression_ratio = 0.15
"#;

    println!("\n=== Sample TOML Configuration ===\n");
    println!("{}", toml);
}
