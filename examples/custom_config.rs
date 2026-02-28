//! Custom Configuration Example
//!
//! This example demonstrates various ways to configure Selfware:
//! 1. Loading from TOML files
//! 2. Environment variable overrides
//! 3. Programmatic configuration
//! 4. Runtime mode switching
//!
//! # Running this example
//!
//! ```bash
//! # With defaults
//! cargo run --example custom_config
//!
//! # With environment overrides
//! SELFWARE_ENDPOINT=http://localhost:11434/v1 \
//! SELFWARE_MODEL=qwen2.5-coder \
//! SELFWARE_TIMEOUT=600 \
//! cargo run --example custom_config
//! ```
//!
//! # Configuration Priority
//!
//! Configuration is loaded in this order (later overrides earlier):
//! 1. Built-in defaults
//! 2. Config file (selfware.toml or ~/.config/selfware/config.toml)
//! 3. Environment variables (SELFWARE_*)
//! 4. Programmatic overrides

use anyhow::Result;
use selfware::config::{
    AgentConfig, Config, ExecutionMode, SafetyConfig, UiConfig, YoloFileConfig,
};
use std::path::PathBuf;

fn main() -> Result<()> {
    println!("=== Selfware Custom Configuration Example ===\n");

    // Example 1: Load from default locations
    println!("1. Loading from default locations...");
    let default_config = Config::load(None)?;
    print_config_summary("Default", &default_config);

    // Example 2: Load from a specific file (if it exists)
    println!("\n2. Loading from specific file...");
    let custom_path = "custom-selfware.toml";
    match Config::load(Some(custom_path)) {
        Ok(config) => print_config_summary("Custom file", &config),
        Err(_) => println!("   (File '{}' not found, skipping)", custom_path),
    }

    // Example 3: Build configuration programmatically
    println!("\n3. Programmatic configuration...");
    let programmatic_config = build_custom_config();
    print_config_summary("Programmatic", &programmatic_config);

    // Example 4: Environment variable demonstration
    println!("\n4. Environment variable overrides:");
    println!(
        "   SELFWARE_ENDPOINT = {:?}",
        std::env::var("SELFWARE_ENDPOINT").ok()
    );
    println!(
        "   SELFWARE_MODEL = {:?}",
        std::env::var("SELFWARE_MODEL").ok()
    );
    println!(
        "   SELFWARE_TIMEOUT = {:?}",
        std::env::var("SELFWARE_TIMEOUT").ok()
    );
    println!(
        "   SELFWARE_API_KEY = {:?}",
        std::env::var("SELFWARE_API_KEY").ok().map(|_| "[REDACTED]")
    );

    // Example 5: Execution modes
    println!("\n5. Execution modes:");
    demonstrate_execution_modes();

    // Example 6: Safety configuration
    println!("\n6. Safety configuration:");
    demonstrate_safety_config();

    // Example 7: YOLO mode configuration
    println!("\n7. YOLO mode configuration:");
    demonstrate_yolo_config();

    // Example 8: Create a sample TOML config
    println!("\n8. Sample TOML configuration:");
    print_sample_toml();

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
        },

        // Agent behavior
        agent: AgentConfig {
            max_iterations: 100,
            step_timeout_secs: 300, // 5 minutes
            token_budget: 500000,
            native_function_calling: false,
            streaming: true,
        },

        // YOLO mode settings
        yolo: YoloFileConfig {
            enabled: false,
            max_operations: 1000,
            max_hours: 4.0,
            allow_git_push: false,
            allow_destructive_shell: false,
            audit_log_path: Some(PathBuf::from("/tmp/selfware-audit.log")),
            status_interval: 50,
        },

        // UI settings
        ui: UiConfig {
            theme: "amber".to_string(),
            animations: true,
            compact_mode: false,
            verbose_mode: false,
            show_tokens: false,
            animation_speed: 1.0,
        },

        // Continuous-work settings
        continuous_work: Default::default(),

        // API retry settings
        retry: Default::default(),

        resources: selfware::config::ResourcesConfig::default(),

        // Default execution mode
        execution_mode: ExecutionMode::Normal,

        // CLI-only flags (not persisted in config)
        compact_mode: false,
        verbose_mode: false,
        show_tokens: false,
    }
}

/// Print a summary of a configuration
fn print_config_summary(name: &str, config: &Config) {
    println!("   {} Configuration:", name);
    println!("     Endpoint: {}", config.endpoint);
    println!("     Model: {}", config.model);
    println!("     Max tokens: {}", config.max_tokens);
    println!("     Temperature: {}", config.temperature);
    println!("     Max iterations: {}", config.agent.max_iterations);
    println!("     Timeout: {}s", config.agent.step_timeout_secs);
}

/// Demonstrate different execution modes
fn demonstrate_execution_modes() {
    let modes = [
        (
            ExecutionMode::Normal,
            "Ask for confirmation on destructive operations",
        ),
        (
            ExecutionMode::AutoEdit,
            "Auto-approve file edits, ask for others",
        ),
        (
            ExecutionMode::Yolo,
            "Auto-approve all operations (use with caution)",
        ),
        (ExecutionMode::Daemon, "Run continuously in autonomous mode"),
    ];

    for (mode, description) in modes {
        println!("   {:?}: {}", mode, description);
    }

    println!("\n   Usage:");
    println!("     selfware chat              # Normal mode (default)");
    println!("     selfware chat -m auto-edit # Auto-edit mode");
    println!("     selfware chat --yolo       # YOLO mode");
    println!("     selfware chat --daemon     # Daemon mode");
}

/// Demonstrate safety configuration options
fn demonstrate_safety_config() {
    println!("   Path patterns (glob syntax):");
    println!("     './**'        - Current directory and subdirectories");
    println!("     '/home/**'    - Home directory tree");
    println!("     '**/.env'     - All .env files (typically denied)");
    println!();
    println!("   Protected branches prevent direct pushes to:");
    println!("     main, master, production, release/*");
    println!();
    println!("   Tools requiring confirmation:");
    println!("     git_push, file_delete, shell_exec, container_exec");
}

/// Demonstrate YOLO mode configuration
fn demonstrate_yolo_config() {
    println!("   YOLO mode allows autonomous operation with safeguards:");
    println!();
    println!("   [yolo]");
    println!("   enabled = true");
    println!("   max_operations = 1000     # Stop after N operations");
    println!("   max_hours = 4.0           # Stop after N hours");
    println!("   allow_git_push = false    # Still protect git push");
    println!("   allow_destructive_shell = false  # Block rm -rf, etc.");
    println!("   audit_log_path = \"/tmp/audit.log\"  # Log all actions");
    println!("   status_interval = 50      # Report every 50 ops");
}

/// Print a sample TOML configuration
fn print_sample_toml() {
    let sample = r#"# Selfware Configuration
# Save as: selfware.toml or ~/.config/selfware/config.toml

# LLM Backend
endpoint = "http://localhost:8000/v1"
model = "Qwen/Qwen3-Coder-Next-FP8"
max_tokens = 65536
temperature = 0.7
# api_key = "sk-..."  # Uncomment if your backend requires auth

# Safety Settings
[safety]
allowed_paths = ["./**", "/tmp/selfware/**"]
denied_paths = ["**/.env", "**/secrets/**", "**/*.pem"]
protected_branches = ["main", "master"]
require_confirmation = ["git_push", "file_delete", "shell_exec"]

# Agent Behavior
[agent]
max_iterations = 100
step_timeout_secs = 300
token_budget = 500000
streaming = true

# YOLO Mode (autonomous operation)
[yolo]
enabled = false
max_operations = 1000
max_hours = 4.0
allow_git_push = false
allow_destructive_shell = false
# audit_log_path = "/var/log/selfware-audit.log"
status_interval = 100"#;

    // Print with line numbers
    for (i, line) in sample.lines().enumerate() {
        println!("   {:2} | {}", i + 1, line);
    }
}
