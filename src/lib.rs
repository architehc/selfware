//! Selfware Workshop - Your Personal AI Companion
//!
//! A sophisticated agent framework for autonomous coding tasks, built on the
//! selfware philosophy: software you own, software that knows you, software
//! that lasts.
//!
//! - **Tools**: 54 built-in tools for file, git, cargo, search, and more
//! - **Safety**: Multi-layer validation, path protection, command filtering
//! - **Persistence**: Checkpoint system for long-running tasks
//! - **Self-Healing**: Error classification, recovery strategies, exponential backoff
//! - **Cognition**: PDVR cycle (Plan-Do-Verify-Reflect), working memory
//! - **Garden View**: Visualize your codebase as a living garden
//! - **Local-First**: Runs on your hardware, your rules
//!
//! # Quick Start
//!
//! ```ignore
//! use selfware::{Agent, Config};
//!
//! let config = Config::load(None)?;
//! let mut agent = Agent::new(config).await?;
//! agent.run_task("Tend to the garden").await?;
//! ```

// ============================================================================
// Core modules
// ============================================================================
pub mod agent;
pub mod api;
pub mod cli;
pub mod config;
pub mod errors;
pub mod input;
pub mod tools;
pub mod ui;

// ============================================================================
// Reorganized modules
// ============================================================================
pub mod analysis;
pub mod cognitive;
pub mod devops;
pub mod observability;
pub mod orchestration;
pub mod safety;
pub mod session;
pub mod testing;

// Backward-compatible re-exports for safety module
pub use safety::redact;
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
pub use orchestration::workflows;

// Backward-compatible re-exports for devops module
pub use devops::container;
pub use devops::process_manager;

// Backward-compatible re-exports for testing module
pub use testing::verification;

// ============================================================================
// Modules to be reorganized (kept for now)
// ============================================================================
pub mod memory;
pub mod output;
#[cfg(feature = "resilience")]
pub mod self_healing;
pub mod token_count;
#[cfg(feature = "tokens")]
pub mod tokens;
pub mod tool_parser;

// ============================================================================
// Backward-compatible re-exports for UI submodules
// ============================================================================
// TUI and demo modules (moved to ui/)
#[cfg(feature = "tui")]
pub use ui::demo;
#[cfg(feature = "tui")]
pub use ui::tui;
