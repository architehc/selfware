#![allow(dead_code, unused_imports, unused_variables)]
//! Swarm dispatch meta-tool.
//!
//! Allows the primary agent to spin up sub-agents in a swarm for
//! parallelized task execution with consensus-based result aggregation.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::{debug, info};

use super::Tool;

/// Meta-tool that dispatches a task to the multi-agent swarm.
///
/// The swarm creates sub-agents with different roles (Architect, Coder,
/// Tester, Reviewer) and runs them through the task in priority order,
/// collecting and aggregating results.
pub struct SwarmDispatchTool;

impl SwarmDispatchTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for SwarmDispatchTool {
    fn name(&self) -> &str {
        "swarm_dispatch"
    }

    fn description(&self) -> &str {
        "Dispatch a task to a multi-agent swarm for parallel execution. \
         Creates specialized agents (Architect, Coder, Tester, Reviewer) \
         that collaborate on the task. Use for complex tasks that benefit \
         from multiple perspectives."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["task"],
            "properties": {
                "task": {
                    "type": "string",
                    "description": "The task description to dispatch to the swarm"
                },
                "roles": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Optional: specific roles to include (architect, coder, tester, reviewer). Default: all roles."
                },
                "conflict_strategy": {
                    "type": "string",
                    "enum": ["majority", "unanimous", "weighted", "architect_decides"],
                    "description": "How to resolve conflicting results. Default: majority."
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let task = args
            .get("task")
            .and_then(|t| t.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required 'task' argument"))?;

        info!("Swarm dispatch: {}", task);

        use crate::orchestration::swarm::{create_dev_swarm, AgentRole, SwarmTask};

        let mut swarm = create_dev_swarm();

        // Build role-specific sub-tasks
        let phases: Vec<(AgentRole, String)> = vec![
            (
                AgentRole::Architect,
                format!(
                    "Design the architecture and plan the implementation for: {}",
                    task
                ),
            ),
            (
                AgentRole::Coder,
                format!("Implement the solution for: {}", task),
            ),
            (
                AgentRole::Tester,
                format!("Create tests and verify the implementation for: {}", task),
            ),
            (
                AgentRole::Reviewer,
                format!("Review the code and suggest improvements for: {}", task),
            ),
        ];

        let mut results = Vec::new();
        for (role, phase_task) in &phases {
            let swarm_task = SwarmTask::new(phase_task.as_str()).with_role(*role);
            if swarm.queue_task(swarm_task).is_ok() {
                results.push(serde_json::json!({
                    "role": role.name(),
                    "task": phase_task,
                    "status": "queued"
                }));
            }
        }

        let stats = swarm.stats();

        Ok(serde_json::json!({
            "status": "swarm_initialized",
            "agents": stats.total_agents,
            "tasks_queued": results.len(),
            "phases": results,
            "note": "Swarm tasks are queued for execution. Use /swarm command for interactive swarm mode."
        }))
    }
}

impl Default for SwarmDispatchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swarm_tool_metadata() {
        let tool = SwarmDispatchTool::new();
        assert_eq!(tool.name(), "swarm_dispatch");
        assert!(!tool.description().is_empty());

        let schema = tool.schema();
        assert!(schema.get("required").is_some());
        assert!(schema.get("properties").unwrap().get("task").is_some());
    }

    #[tokio::test]
    async fn test_swarm_dispatch_execution() {
        let tool = SwarmDispatchTool::new();
        let result = tool
            .execute(serde_json::json!({
                "task": "Test task for swarm"
            }))
            .await
            .unwrap();

        assert_eq!(
            result.get("status").unwrap().as_str().unwrap(),
            "swarm_initialized"
        );
        assert!(result.get("agents").unwrap().as_u64().unwrap() > 0);
    }
}
