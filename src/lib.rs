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

// ─── Core modules (always available) ───────────────────────────────
pub mod agent;
pub mod api;
pub mod checkpoint;
pub mod cognitive;
pub mod config;
pub mod confirm;
pub mod memory;
pub mod planning;
pub mod redact;
pub mod safety;
pub mod telemetry;
pub mod tool_parser;
pub mod tools;
pub mod tokens;

// ─── UI & Input ────────────────────────────────────────────────────
pub mod input;
pub mod tui;
pub mod ui;

// ─── Analysis & Code Intelligence ─────────────────────────────────
pub mod analytics;
pub mod analyzer;
pub mod code_graph;
pub mod code_review;
pub mod cognitive_load;
pub mod tech_debt;
pub mod log_analysis;

// ─── Memory & Knowledge Systems ───────────────────────────────────
pub mod episodic;
pub mod knowledge_graph;
pub mod rag;
pub mod vector_store;

// ─── Execution & Workflows ────────────────────────────────────────
pub mod autonomy;
pub mod dry_run;
pub mod parallel;
pub mod speculative;
pub mod streaming;
pub mod workflow_dsl;
pub mod workflows;
pub mod yolo;

// ─── Multi-Agent & Collaboration ──────────────────────────────────
pub mod communication;
pub mod multiagent;
pub mod realtime_collaboration;
pub mod swarm;
pub mod team_knowledge;

// ─── Infrastructure & DevOps ──────────────────────────────────────
pub mod cicd;
pub mod cloud_infra;
pub mod container;
pub mod distributed;
pub mod kubernetes;
pub mod mlops;

// ─── Quality & Security ──────────────────────────────────────────
pub mod api_testing;
pub mod contract_testing;
pub mod sandbox;
pub mod security_scanner;
pub mod threat_modeling;
pub mod verification;
pub mod test_dashboard;

// ─── Extensions & Optional Features ──────────────────────────────
pub mod browser_automation;
pub mod cache;
pub mod carbon_tracker;
pub mod database;
pub mod database_tools;
pub mod degradation;
pub mod doc_generator;
pub mod dyslexia_friendly;
pub mod edit_history;
pub mod embedded;
pub mod extensions;
pub mod ide_plugin;
pub mod image_understanding;
pub mod intelligence;
pub mod intent;
pub mod issue_tracker;
pub mod learning;
pub mod literate;
pub mod local_first;
pub mod mcp;
pub mod model_router;
pub mod monorepo;
pub mod observability;
pub mod plugins;
pub mod process_manager;
pub mod qwen_features;
#[cfg(test)]
pub mod qwen_features_test;
pub mod screen_reader;
pub mod self_healing;
pub mod self_improvement;
pub mod session_recording;
pub mod shell_hooks;
pub mod time_travel;
pub mod typed_config;
pub mod voice_interface;
pub mod wellness;
