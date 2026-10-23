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
pub(crate) mod primitives;
pub mod swarm;
pub mod workflows;

pub use multiagent::{
    agent_color_index, agent_tag, print_agent_summary, run_multiagent_task, summary_table_rows,
    AgentInstance, AgentLineBuffer, AgentResult, AgentStatus as MultiAgentStatus, MultiAgentChat,
    MultiAgentConfig, MultiAgentEvent, MultiAgentFailurePolicy, MultiChatView, ViewTab,
    MAX_CONCURRENT_AGENTS,
};
pub use swarm::{
    create_dev_swarm, Agent as SwarmAgent, AgentRole, AgentStatus as SwarmAgentStatus,
    ConflictStrategy, Decision, DecisionStatus, MemoryAccess, MemoryAction, MemoryEntry,
    SharedMemory, Swarm, SwarmStats, SwarmTask, TaskStatus as SwarmTaskStatus, Vote,
};
pub use workflows::{
    new_run_id, step_fingerprint, validate_run_id, CheckpointStore, DependencyError, LlmHandler,
    RetryConfig, StepResult, StepStatus, StepType, ToolHandler, VarValue, Workflow, WorkflowBudget,
    WorkflowCheckpoint, WorkflowContext, WorkflowExecutor, WorkflowInput, WorkflowOutput,
    WorkflowResult, WorkflowRun, WorkflowStatus, WorkflowStep, WorkflowStopReason,
    WorkflowTelemetry, WorkflowTemplates, CHECKPOINT_FORMAT_VERSION,
};
