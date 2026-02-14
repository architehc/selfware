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
pub mod config;
pub mod input;
pub mod tools;
pub mod ui;

// ============================================================================
// Reorganized modules
// ============================================================================
pub mod safety;
pub mod analysis;
pub mod session;
pub mod observability;

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
pub use session::checkpoint;
pub use session::time_travel;
pub use session::local_first;
pub use session::edit_history;
#[cfg(feature = "cache")]
pub use session::cache;

// Backward-compatible re-exports for observability module
pub use observability::telemetry;
pub use observability::analytics;
pub use observability::carbon_tracker;
pub use observability::dashboard as observability_dashboard;
pub use observability::test_dashboard;
#[cfg(feature = "log-analysis")]
pub use observability::log_analysis;

// Backward-compatible re-export for config module
pub use config::typed as typed_config;

// ============================================================================
// Modules to be reorganized (kept for now)
// ============================================================================
pub mod api_testing;
pub mod browser_automation;
pub mod cicd;
pub mod cloud_infra;
pub mod code_review;
pub mod cognitive;
pub mod cognitive_load;
pub mod communication;
pub mod container;
pub mod contract_testing;
pub mod database;
pub mod database_tools;
#[cfg(feature = "resilience")]
pub mod degradation;
#[cfg(feature = "tui")]
pub mod demo;
pub mod distributed;
pub mod doc_generator;
pub mod dyslexia_friendly;
pub mod embedded;
pub mod episodic;
pub mod extensions;
pub mod ide_plugin;
pub mod image_understanding;
pub mod intelligence;
pub mod intent;
pub mod issue_tracker;
pub mod knowledge_graph;
pub mod kubernetes;
pub mod learning;
pub mod literate;
pub mod mcp;
pub mod memory;
pub mod mlops;
pub mod model_router;
pub mod monorepo;
pub mod multiagent;
pub mod output;
#[cfg(feature = "workflows")]
pub mod parallel;
pub mod planning;
pub mod process_manager;
pub mod rag;
pub mod realtime_collaboration;
pub mod screen_reader;
#[cfg(feature = "resilience")]
pub mod self_healing;
pub mod self_improvement;
pub mod session_recording;
pub mod shell_hooks;
#[cfg(feature = "speculative")]
pub mod speculative;
pub mod streaming;
pub mod swarm;
pub mod team_knowledge;
#[cfg(feature = "tokens")]
pub mod tokens;
pub mod tool_parser;
#[cfg(feature = "tui")]
pub mod tui;
pub mod verification;
pub mod voice_interface;
pub mod wellness;
#[cfg(feature = "workflows")]
pub mod workflow_dsl;
pub mod workflows;
