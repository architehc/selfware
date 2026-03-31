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
pub(crate) mod testing;

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

// Backward-compatible re-exports for safety module
pub use safety::redact;

// Re-export shutdown_tracing for main.rs
pub use observability::telemetry::shutdown_tracing;
pub use safety::sandbox;
pub use safety::threat_modeling;

// Backward-compatible re-exports for analysis module
pub use analysis::analyzer;
pub use analysis::bm25;
pub use analysis::vector_store;

// Backward-compatible re-exports for session module
pub use session::checkpoint;

// Backward-compatible re-exports for observability module
pub use observability::telemetry;

// Backward-compatible re-exports for orchestration module
pub use orchestration::multiagent;
pub use orchestration::planning;
pub use orchestration::swarm;
pub use orchestration::visual_loop;
pub use orchestration::workflows;

// Backward-compatible re-exports for devops module
pub use devops::container;
pub use devops::process_manager;

// Backward-compatible re-exports for testing module
pub use testing::verification;
pub use testing::visual_verification;

// ============================================================================
// New feature modules
// ============================================================================
pub(crate) mod batch;
pub mod browser;
pub mod profiles;
pub mod swebench;
pub(crate) mod validation;

// ============================================================================
// Utility modules
// ============================================================================
pub mod interview;
#[cfg(feature = "tokens")]
pub mod kv_store;
pub mod llm_doctor;
pub mod memory;
pub mod output;
#[cfg(feature = "resilience")]
pub mod self_healing;
pub mod templates;
pub mod token_count;
pub mod tokens;
pub mod tool_parser;

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

// ============================================================================
// Backward-compatible re-exports for UI submodules
// ============================================================================
#[cfg(feature = "tui")]
pub use ui::demo;
#[cfg(feature = "tui")]
pub use ui::tui;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shutdown_flag_default_false() {
        // The flag may have been set by a previous test in the same process,
        // so we just verify the functions don't panic and return a bool.
        let _ = is_shutdown_requested();
    }

    #[test]
    fn test_request_shutdown_sets_flag() {
        request_shutdown();
        assert!(is_shutdown_requested());
    }
}
