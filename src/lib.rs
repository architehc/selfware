// Crate is under active development — many types/functions are defined ahead
// of use across modules.

//! # Selfware — Your Personal AI Workshop
//!
//! An agentic coding harness for local LLMs. 70+ tools, multi-agent swarm,
//! evolution engine, hooks, MCP, LSP, and a fox mascot — all local-first,
//! no cloud required.
//!
//! ## Core Features
//!
//! - **70+ Tools**: File, git, cargo, search, shell, PTY, browser automation,
//!   computer control, LSP, MCP, knowledge graph, vision, FIM editing
//! - **Multi-Agent Swarm**: Up to 16 concurrent agents with consensus voting
//! - **Safety**: Multi-layer validation, path protection, command filtering,
//!   JSONL audit logging, permission grants with expiry
//! - **Hooks**: Event-driven automation (PreToolUse, PostToolUse, Stop)
//! - **MCP**: Both client (connect to external tools) and server (expose selfware
//!   tools to other AI systems)
//! - **LSP**: Semantic code intelligence via rust-analyzer, pyright, tsserver, gopls
//! - **Doctor**: System dependency checker (30+ tools) and LLM backend diagnostics
//! - **Templates**: Project scaffolding for Rust, Python, Node.js with QA profiles
//! - **Cognition**: PDVR cycle (Plan-Do-Verify-Reflect), working memory
//! - **Persistence**: Checkpoint system for long-running tasks with session resume
//! - **Self-Healing**: Error classification, recovery strategies, exponential backoff
//! - **Evolution Engine**: Recursive self-improvement with SAB-based fitness
//! - **Local-First**: Runs entirely on your hardware, zero data egress
//!
//! ## Quick Start
//!
//! ```ignore
//! use selfware::{Agent, Config};
//!
//! let config = Config::load(None)?;
//! let mut agent = Agent::new(config).await?;
//! agent.run_task("Add unit tests for the auth module").await?;
//! ```
//!
//! ## CLI
//!
//! ```bash
//! selfware chat                    # Interactive mode
//! selfware run "add unit tests"    # One-shot task
//! selfware doctor                  # Check system dependencies
//! selfware init                    # Project scaffold wizard
//! selfware multi-chat -n 4         # Multi-agent swarm
//! selfware mcp-server              # Run as MCP server
//! ```
//!
//! See [selfware.design](https://selfware.design) for full documentation.

// ============================================================================
// Core modules (public API)
// ============================================================================
pub mod agent;
pub mod api;
pub mod cli;
pub mod computer;
pub mod concurrency;
pub mod config;
pub mod doctor;
pub mod errors;
pub mod hooks;
pub mod input;
pub mod lsp;
pub mod mcp;
pub mod safety;
pub mod skills;
pub mod swl;
#[cfg(test)]
pub(crate) mod test_support;
pub mod tools;
pub mod ui;

// ============================================================================
// Domain modules
// ============================================================================
// Public for external tests and downstream use. Submodule items are selectively
// re-exported below for convenience; the full paths remain available.
// ============================================================================
pub mod analysis;
pub mod cognitive;
pub(crate) mod devops;
pub(crate) mod observability;
pub(crate) mod orchestration;
pub mod resource;
pub(crate) mod session;
pub mod supervision;
pub mod testing;

// Evolution engine — recursive self-improvement via evolutionary mutation
#[cfg(feature = "self-improvement")]
pub mod evolution;

// VLM benchmark suite for visual understanding evaluation
#[cfg(feature = "vlm-bench")]
pub mod vlm_bench;

// Concurrent benchmark harness for throughput testing
#[cfg(feature = "bench-harness")]
pub mod bench_harness;

// Memory consolidation ("sleep") system
#[cfg(feature = "consolidation")]
pub mod consolidation;

// Re-export shutdown_tracing for main.rs
pub use observability::telemetry::shutdown_tracing;

// Backward-compatible re-exports for session module
pub use session::checkpoint;

// Backward-compatible re-exports for observability module
pub use observability::telemetry;

// Backward-compatible re-exports for orchestration module
pub use orchestration::multiagent;
pub use orchestration::swarm;
pub use orchestration::visual_loop;
pub use orchestration::workflows;

// Backward-compatible re-exports for devops module
pub use devops::process_manager;

// Backward-compatible re-exports for testing module
pub use testing::verification;
pub use testing::visual_verification;

// ============================================================================
// New feature modules
pub mod evolve;
pub mod profiles;

// ============================================================================
// Utility modules
// ============================================================================
pub mod interview;
pub mod llm_doctor;
pub mod memory;
pub mod output;
#[cfg(feature = "resilience")]
pub mod self_healing;
pub mod templates;
pub mod token_count;
pub mod tool_parser;
pub mod util;

// ============================================================================
// Global shutdown coordination
// ============================================================================
use std::sync::atomic::{AtomicBool, Ordering};

static SHUTDOWN_FLAG: AtomicBool = AtomicBool::new(false);

/// Signal that a graceful shutdown has been requested.
pub fn request_shutdown() {
    SHUTDOWN_FLAG.store(true, Ordering::SeqCst);
}

/// Check whether a graceful shutdown has been requested.
pub fn is_shutdown_requested() -> bool {
    SHUTDOWN_FLAG.load(Ordering::SeqCst)
}

/// Test-only: clear the process-global shutdown latch. In production the latch
/// is a one-way switch owned by `main`'s signal handler, but a test that
/// exercises `request_shutdown` must clear it — otherwise the flag stays set for
/// the rest of the process and every later test that checks `is_cancelled()`
/// (which observes this latch) spuriously fails with "Task cancelled by user".
#[cfg(test)]
pub(crate) fn reset_shutdown_for_test() {
    SHUTDOWN_FLAG.store(false, Ordering::SeqCst);
}

/// Await until a graceful shutdown has been requested.
///
/// The shutdown state is a one-way latch, so a simple poll loop is race-free
/// and lets long async waits (e.g. a stalled provider connection) abort
/// promptly on Ctrl-C. Returns immediately if shutdown was already requested.
pub async fn shutdown_requested() {
    while !is_shutdown_requested() {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

// Let `#[path]`-included unit tests address this crate as `selfware::...`,
// matching how the `tests/unit` integration target sees it.
#[cfg(test)]
extern crate self as selfware;

#[cfg(test)]
#[path = "../tests/unit/_crate/lib_test.rs"]
mod tests;
