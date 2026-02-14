//! Workflow orchestration module
//!
//! This module contains workflow and multi-agent orchestration including:
//! - Workflow execution
//! - Workflow DSL
//! - Parallel execution
//! - Swarm agents
//! - Multi-agent coordination
//! - Planning

pub mod workflows;
pub mod swarm;
pub mod multiagent;
pub mod planning;

#[cfg(feature = "workflows")]
pub mod workflow_dsl;
#[cfg(feature = "workflows")]
pub mod parallel;
