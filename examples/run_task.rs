//! Example: running a task non-interactively with Selfware.
//!
//! Demonstrates proper unattended execution:
//! - Uses `ExecutionMode::Yolo` so tools never block on confirmation.
//! - Runs inside a temporary directory to avoid polluting the workspace.
//! - Cleans up the temp directory on success *and* on failure.
//!
//! ```sh
//! # Run with a 60-second timeout (requires SELFWARE_API_KEY or config):
//! SELFWARE_TIMEOUT=60 cargo run --example run_task
//! ```

use anyhow::Result;
use selfware::config::{Config, ExecutionMode};
use std::env;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialise tracing so that agent progress is visible in logs.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "selfware=info".into()),
        )
        .init();

    // Parse optional timeout from the environment (seconds, default 120).
    let timeout_secs: u64 = env::var("SELFWARE_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);

    // Create a temporary working directory.  This is cleaned up
    // automatically when `_workdir` is dropped — even on early `?` returns.
    let _workdir = TempDir::new()?;
    let workdir_path = _workdir.path().to_path_buf();

    println!(
        "Working directory: {} (will be cleaned up automatically)",
        workdir_path.display()
    );

    // Load config from the usual selfware.toml search path.
    let mut config = Config::load(None)?;

    // Key: use Yolo mode so no tool requires interactive confirmation.
    // AutoEdit only auto-approves file operations — shell/cargo commands
    // still block, which causes failures in unattended mode.
    config.execution_mode = ExecutionMode::Yolo;

    // Limit iterations so a runaway agent cannot loop forever.
    config.agent.max_iterations = 20;

    // Build the agent.
    let mut agent = selfware::agent::Agent::new(config).await?;

    // Define a self-contained task that does not depend on external state.
    let task = "\
        Create a file called hello.rs in the current directory containing a \
        Rust program that prints 'Hello from Selfware!'. \
        Then compile it with `rustc hello.rs` and run `./hello`. \
        Report the output.";

    // Run the task with a timeout.
    let result = tokio::time::timeout(Duration::from_secs(timeout_secs), agent.run_task(task)).await;

    match result {
        Ok(Ok(())) => {
            println!("Task completed successfully.");
        }
        Ok(Err(e)) => {
            eprintln!("Task failed: {e:#}");
            // _workdir drops here, cleaning up artifacts.
            return Err(e);
        }
        Err(_) => {
            eprintln!("Task timed out after {timeout_secs}s.");
            // _workdir drops here, cleaning up artifacts.
            anyhow::bail!("Task timed out after {timeout_secs}s");
        }
    }

    // _workdir is dropped here, removing the temp directory and any artifacts.
    Ok(())
}
