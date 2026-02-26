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
// Core modules -- public API surface
// ============================================================================
pub mod agent;
pub mod api;
pub mod cli;
pub mod config;
pub mod safety;
pub mod tools;

// ============================================================================
// Internal implementation modules
//
// These are `pub(crate)` because they are implementation details that should
// not be part of the public API.  Where external access is needed, items are
// re-exported below so that the public path remains stable while the module
// tree stays private.
//
// NOTE: `errors`, `input`, `ui`, `memory`, `output`, `token_count`, `tokens`,
// and `self_healing` SHOULD ideally be `pub(crate)`, but some are currently
// kept `pub` due to test coupling or feature-gated doc examples.  Comments
// on individual modules explain why.
// ============================================================================

// `errors` -- not imported by external crates/tests; safe to restrict.
pub(crate) mod errors;

// `input` -- not imported externally; safe to restrict.
pub(crate) mod input;

// `ui` -- re-exported via `pub use ui::tui` / `pub use ui::demo` below, but
// also used directly by examples (e.g. `selfware::ui::tui::run_tui_swarm`).
// Must stay `pub` until examples are updated to use re-exports.
pub mod ui;

// Reorganized domain modules -- kept `pub(crate)` with public re-exports.
pub(crate) mod analysis;
pub mod cognitive;
pub(crate) mod devops;
pub(crate) mod observability;
pub mod orchestration;
pub(crate) mod session;
pub(crate) mod testing;

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
//
// `memory`, `output`, `token_count`, `tokens` -- not imported externally;
// SHOULD be `pub(crate)` but kept `pub` for now because internal doc-tests
// or feature-gated modules may reference them.  Restrict once audited.
// ============================================================================
pub(crate) mod memory;
pub(crate) mod output;
#[cfg(feature = "resilience")]
pub(crate) mod self_healing;
pub(crate) mod token_count;
#[cfg(feature = "tokens")]
pub(crate) mod tokens;
pub mod tool_parser;

// ============================================================================
// Backward-compatible re-exports for UI submodules
// ============================================================================
// TUI and demo modules (moved to ui/)
#[cfg(feature = "tui")]
pub use ui::demo;
#[cfg(feature = "tui")]
pub use ui::tui;
