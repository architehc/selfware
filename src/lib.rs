//! Selfware Workshop - Your Personal AI Companion
//!
//! A sophisticated agent framework for autonomous coding tasks, built on the
//! selfware philosophy: software you own, software that knows you, software
//! that lasts.
//!
//! - **Tools**: File operations, git, cargo, search, containers, browser
//! - **Safety**: Multi-layer validation, path protection, command filtering
//! - **Persistence**: Journal entries (checkpoints) for long-running tasks
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
pub mod input;
pub mod tools;
pub mod ui;

// ============================================================================
// Reorganized modules
// ============================================================================
pub mod analysis;
pub mod cognitive;
pub mod collaboration;
pub mod devops;
pub mod observability;
pub mod orchestration;
pub mod safety;
pub mod session;
pub mod testing;

// Backward-compatible re-exports for safety module
pub use safety::autonomy;
#[cfg(feature = "execution-modes")]
pub use safety::confirm;
#[cfg(feature = "execution-modes")]
pub use safety::dry_run;
pub use safety::redact;
pub use safety::sandbox;
pub use safety::scanner as security_scanner;
pub use safety::threat_modeling;
#[cfg(feature = "execution-modes")]
pub use safety::yolo;

// Backward-compatible re-exports for analysis module
pub use analysis::analyzer;
pub use analysis::bm25;
pub use analysis::code_graph;
pub use analysis::tech_debt;
pub use analysis::vector_store;

// Backward-compatible re-exports for session module
#[cfg(feature = "cache")]
pub use session::cache;
pub use session::checkpoint;
pub use session::edit_history;
pub use session::local_first;
pub use session::time_travel;

// Backward-compatible re-exports for observability module
pub use observability::analytics;
pub use observability::carbon_tracker;
pub use observability::dashboard as observability_dashboard;
#[cfg(feature = "log-analysis")]
pub use observability::log_analysis;
pub use observability::telemetry;
pub use observability::test_dashboard;

// Backward-compatible re-exports for cognitive module
pub use cognitive::episodic;
pub use cognitive::intelligence;
pub use cognitive::knowledge_graph;
pub use cognitive::learning;
pub use cognitive::load as cognitive_load;
pub use cognitive::rag;
pub use cognitive::self_improvement;
pub use cognitive::state as cognitive_state;

// Backward-compatible re-exports for orchestration module
pub use orchestration::multiagent;
#[cfg(feature = "workflows")]
pub use orchestration::parallel;
pub use orchestration::planning;
pub use orchestration::swarm;
#[cfg(feature = "workflows")]
pub use orchestration::workflow_dsl;
pub use orchestration::workflows;

// Backward-compatible re-exports for devops module
pub use devops::cicd;
pub use devops::cloud_infra;
pub use devops::container;
pub use devops::database;
pub use devops::database_tools;
pub use devops::distributed;
pub use devops::embedded;
pub use devops::kubernetes;
pub use devops::mlops;
pub use devops::monorepo;
pub use devops::process_manager;

// Backward-compatible re-exports for collaboration module
pub use collaboration::communication;
pub use collaboration::ide_plugin;
pub use collaboration::issue_tracker;
pub use collaboration::realtime as realtime_collaboration;
pub use collaboration::team_knowledge;

// Backward-compatible re-exports for testing module
pub use testing::api_testing;
pub use testing::code_review;
pub use testing::contract_testing;
pub use testing::verification;

// Backward-compatible re-export for config module
pub use config::typed as typed_config;

// ============================================================================
// Modules to be reorganized (kept for now)
// ============================================================================
pub mod browser_automation;
#[cfg(feature = "resilience")]
pub mod degradation;
pub mod doc_generator;
pub mod extensions;
pub mod intent;
pub mod mcp;
pub mod memory;
pub mod model_router;
pub mod output;
#[cfg(feature = "resilience")]
pub mod self_healing;
pub mod shell_hooks;
#[cfg(feature = "speculative")]
pub mod speculative;
pub mod streaming;
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

// Accessibility modules (moved to ui/accessibility/)
pub use ui::accessibility::dyslexia_friendly;
pub use ui::accessibility::image_understanding;
pub use ui::accessibility::literate;
pub use ui::accessibility::screen_reader;
pub use ui::accessibility::session_recording;
pub use ui::accessibility::voice_interface;
pub use ui::accessibility::wellness;
