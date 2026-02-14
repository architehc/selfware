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

pub mod agent;
pub mod analytics;
pub mod analyzer;
pub mod api;
pub mod api_testing;
pub mod autonomy;
pub mod bm25;
pub mod browser_automation;
#[cfg(feature = "extras")]
pub mod cache;
pub mod carbon_tracker;
pub mod checkpoint;
pub mod cicd;
pub mod cloud_infra;
pub mod code_graph;
pub mod code_review;
pub mod cognitive;
pub mod cognitive_load;
pub mod communication;
pub mod config;
#[cfg(feature = "extras")]
pub mod confirm;
pub mod container;
pub mod contract_testing;
pub mod database;
pub mod database_tools;
#[cfg(feature = "extras")]
pub mod degradation;
#[cfg(feature = "extras")]
pub mod demo;
pub mod distributed;
pub mod doc_generator;
#[cfg(feature = "extras")]
pub mod dry_run;
pub mod dyslexia_friendly;
pub mod edit_history;
pub mod embedded;
pub mod episodic;
pub mod extensions;
pub mod ide_plugin;
pub mod image_understanding;
pub mod input;
pub mod intelligence;
pub mod intent;
pub mod issue_tracker;
pub mod knowledge_graph;
pub mod kubernetes;
pub mod learning;
pub mod literate;
pub mod local_first;
#[cfg(feature = "extras")]
pub mod log_analysis;
pub mod mcp;
pub mod memory;
pub mod mlops;
pub mod model_router;
pub mod monorepo;
pub mod multiagent;
pub mod observability;
#[cfg(feature = "extras")]
pub mod parallel;
pub mod planning;
pub mod process_manager;
pub mod rag;
pub mod realtime_collaboration;
pub mod redact;
pub mod safety;
pub mod sandbox;
pub mod screen_reader;
pub mod security_scanner;
#[cfg(feature = "extras")]
pub mod self_healing;
pub mod self_improvement;
pub mod session_recording;
pub mod shell_hooks;
#[cfg(feature = "extras")]
pub mod speculative;
pub mod streaming;
pub mod swarm;
pub mod team_knowledge;
pub mod tech_debt;
pub mod telemetry;
pub mod test_dashboard;
pub mod threat_modeling;
pub mod time_travel;
#[cfg(feature = "extras")]
pub mod tokens;
pub mod tool_parser;
pub mod tools;
#[cfg(feature = "extras")]
pub mod tui;
pub mod typed_config;
pub mod ui;
pub mod vector_store;
pub mod verification;
pub mod voice_interface;
pub mod wellness;
#[cfg(feature = "extras")]
pub mod workflow_dsl;
pub mod workflows;
#[cfg(feature = "extras")]
pub mod yolo;
