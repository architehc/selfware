//! Workflow orchestration module
//!
//! This module contains workflow and multi-agent orchestration including:
//! - Workflow execution
//! - Workflow DSL
//! - Parallel execution
//! - Swarm agents
//! - Multi-agent coordination
//! - Planning

pub mod multiagent;
pub mod planning;
pub mod swarm;
pub mod workflows;

#[cfg(feature = "workflows")]
pub mod parallel;
#[cfg(feature = "workflows")]
pub mod workflow_dsl;
