//! Workflow orchestration module
//!
//! This module contains workflow and multi-agent orchestration including:
//! - Workflow execution
//! - Workflow DSL
//! - Parallel execution
//! - Swarm agents
//! - Multi-agent coordination
//! - Planning
//! - Coordinator Mode (multi-agent orchestration)

#![allow(unused_imports)] // Re-exports are for public API, not internal use

pub mod multiagent;
pub mod swarm;
pub mod visual_loop;
pub mod workflows;
