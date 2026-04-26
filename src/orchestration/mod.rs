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

pub mod coordinator;
pub mod multiagent;
pub mod planning;
pub mod scratchpad;
pub mod swarm;
pub mod visual_loop;
pub mod workflows;

// Re-export coordinator types
pub use coordinator::{
    CoordinatorAgent, CoordinatorConfig, CoordinatorStatus, PhaseResult, WorkerAgent, WorkerResult,
    WorkflowPhase, WorkflowResult, COORDINATOR_ALLOWED_TOOLS, COORDINATOR_DENIED_TOOLS,
};

// Re-export scratchpad types
pub use scratchpad::{Scratchpad, ScratchpadEntry, WorkerInfo, WorkerStatus};
