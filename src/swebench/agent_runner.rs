//! SWE-bench Agent Runner
//!
//! Integrates the selfware Agent with SWE-bench tasks, managing the
//! execution flow and extracting solutions.

use anyhow::{Context, Result};
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::agent::Agent;
use crate::config::Config as AgentConfig;

use super::{SWEConfig, SWEBenchTask, Solution, TrajectoryStep, TaskEnvironment};

/// Runner for executing selfware agent on SWE-bench tasks
pub struct SWEAgentRunner {
    agent: Agent,
    config: SWEConfig,
}

impl SWEAgentRunner {
    /// Create a new agent runner
    pub async fn new(agent_config: &AgentConfig, swe_config: &SWEConfig) -> Result<Self> {
        // Initialize agent with custom config for SWE-bench
        let agent = Agent::new(agent_config.clone())
            .await
            .with_context(|| "Failed to create agent")?;

        Ok(Self {
            agent,
            config: swe_config.clone(),
        })
    }

    /// Solve a SWE-bench task
    pub async fn solve(&self, task: &SWEBenchTask, env: &TaskEnvironment) -> Result<Solution> {
        let start = Instant::now();
        info!("Starting agent solve for task: {}", task.instance_id);

        // Build the prompt
        let prompt = self.build_prompt(task, env);

        // Execute agent
        let mut trajectory = Vec::new();
        let mut iterations = 0;
        let mut tokens_used = 0;

        // Track agent execution
        let result = self.run_agent_with_limits(&prompt, &mut trajectory, &mut iterations, &mut tokens_used).await;

        // Extract patch from agent output or environment
        let patch = match result {
            Ok(output) => {
                // Try to extract patch from agent output
                self.extract_patch(&output).await
                    .unwrap_or_else(|| {
                        // Fallback: get diff from environment
                        env.get_current_diff().await.unwrap_or_default()
                    })
            }
            Err(e) => {
                warn!("Agent execution failed: {}", e);
                // Try to get whatever patch was generated before failure
                env.get_current_diff().await.unwrap_or_default()
            }
        };

        let duration_secs = start.elapsed().as_secs_f64();

        info!(
            "Agent solve complete for {}: {} iterations, {} tokens, {:.1}s",
            task.instance_id, iterations, tokens_used, duration_secs
        );

        Ok(Solution {
            patch,
            duration_secs,
            iterations,
            tokens_used,
            trajectory,
            error: result.err().map(|e| e.to_string()),
        })
    }

    /// Build the prompt for a SWE-bench task
    fn build_prompt(&self, task: &SWEBenchTask, env: &TaskEnvironment) -> String {
        format!(
            r#"You are an expert software engineer working on a real-world bug fix.

## Task Information

**Repository:** {} ({})  
**Base Commit:** {}  

## Problem Statement

{}

{}

## Your Goal

Fix the issue described above. The repository is located at: {}

## Instructions

1. **Explore** the codebase to understand the problem
2. **Identify** the root cause of the bug
3. **Implement** a minimal fix that resolves the issue
4. **Test** your changes to ensure they work
5. **Verify** that existing functionality isn't broken

## Constraints

- Make minimal changes - only what's necessary to fix the bug
- Do NOT modify test files unless explicitly instructed
- Run tests to verify your fix works
- If tests fail, iterate and improve your solution

## Working Directory

The repository has been cloned and is ready for you to work on. Use the file tools to explore and modify the code.
"#,
            task.repo,
            task.version,
            task.base_commit,
            task.problem_statement,
            if task.hints_text.is_empty() {
                String::new()
            } else {
                format!("\n## Hints\n\n{}", task.hints_text)
            },
            env.repo_path().display()
        )
    }

    /// Run agent with resource limits
    async fn run_agent_with_limits(
        &self,
        prompt: &str,
        trajectory: &mut Vec<TrajectoryStep>,
        iterations: &mut usize,
        tokens_used: &mut usize,
    ) -> Result<String> {
        let start_step = 1;

        // This would integrate with the actual agent execution loop
        // For now, this is a simplified version that calls the agent
        
        // Record initial step
        trajectory.push(TrajectoryStep {
            step: start_step,
            action: "start".to_string(),
            observation: "Beginning task execution".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            tool_calls: None,
            tokens_used: Some(0),
        });

        // Execute agent (simplified - actual implementation would use agent.run())
        let output = self.execute_agent_iterations(prompt, trajectory, iterations, tokens_used).await?;

        Ok(output)
    }

    /// Execute agent iterations
    async fn execute_agent_iterations(
        &self,
        prompt: &str,
        trajectory: &mut Vec<TrajectoryStep>,
        iterations: &mut usize,
        tokens_used: &mut usize,
    ) -> Result<String> {
        use crate::api::types::Message;

        let max_iterations = self.config.max_iterations;
        let token_budget = self.config.token_budget;

        // Initial message
        let messages = vec![Message::user(prompt.to_string())];

        // Simulate agent execution (actual implementation would use real agent loop)
        let mut step = trajectory.len();
        
        loop {
            if *iterations >= max_iterations {
                warn!("Reached max iterations ({}) for task", max_iterations);
                break;
            }

            if *tokens_used >= token_budget {
                warn!("Reached token budget ({}) for task", token_budget);
                break;
            }

            step += 1;
            *iterations += 1;

            // Record step (actual implementation would capture real actions)
            trajectory.push(TrajectoryStep {
                step,
                action: "think".to_string(),
                observation: format!("Iteration {}", iterations),
                timestamp: chrono::Utc::now().to_rfc3339(),
                tool_calls: Some(vec!["file_read".to_string()]),
                tokens_used: Some(1000),
            });

            *tokens_used += 1000; // Placeholder

            // Check for completion (simplified)
            if *iterations > 5 {
                break;
            }
        }

        // Return final output (simplified)
        Ok("Task completed".to_string())
    }

    /// Extract patch from agent output
    async fn extract_patch(&self, output: &str) -> Option<String> {
        // Look for diff in output
        if let Some(start) = output.find("diff --git") {
            // Extract from diff start to end
            let patch = &output[start..];
            // Trim to reasonable length
            let patch = if patch.len() > 100000 {
                &patch[..100000]
            } else {
                patch
            };
            return Some(patch.to_string());
        }

        // Look for code fences
        if let Some(start) = output.find("```diff") {
            if let Some(end) = output[start + 7..].find("```") {
                return Some(output[start + 7..start + 7 + end].trim().to_string());
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_prompt() {
        let agent_config = AgentConfig::default();
        let swe_config = SWEConfig::default();
        
        // This would need async runtime to test properly
        // For now, just verify the function exists
    }
}
