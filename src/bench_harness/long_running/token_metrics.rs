//! Token metrics tracking for long-running tests.
//!
//! Tracks tokens sent (input) and received (output) across all agent interactions,
//! providing detailed analytics on token usage patterns, costs, and efficiency.

use crate::token_count::estimate_content_tokens;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Token usage for a single interaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInteraction {
    pub timestamp: String,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    pub interaction_type: InteractionType,
    pub task_name: String,
}

/// Type of interaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InteractionType {
    ToolCall,
    ToolResult,
    Planning,
    Completion,
    Recovery,
    Verification,
}

impl std::fmt::Display for InteractionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ToolCall => write!(f, "TOOL_CALL"),
            Self::ToolResult => write!(f, "TOOL_RESULT"),
            Self::Planning => write!(f, "PLANNING"),
            Self::Completion => write!(f, "COMPLETION"),
            Self::Recovery => write!(f, "RECOVERY"),
            Self::Verification => write!(f, "VERIFICATION"),
        }
    }
}

/// Token metrics for a single task/project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTokenMetrics {
    pub task_name: String,
    pub interactions: Vec<TokenInteraction>,
    pub total_prompt_tokens: usize,
    pub total_completion_tokens: usize,
    pub total_tokens: usize,
    pub start_time: Instant,
    pub duration: Duration,
}

impl TaskTokenMetrics {
    pub fn new(task_name: String) -> Self {
        Self {
    task_name,
  interactions: Vec::new(),
         total_prompt_tokens: 0,
            total_completion_tokens: 0,
     total_tokens: 0,
         start_time: Instant::now(),
        duration: Duration::ZERO,
        }
    }

    pub fn add_interaction(&mut self, interaction: TokenInteraction) {
        self.total_prompt_tokens += interaction.prompt_tokens;
    self.total_completion_tokens += interaction.completion_tokens;
      self.total_tokens += interaction.total_tokens;
        self.interactions.push(interaction);
    }

    pub fn finalize(&mut self) {
        self.duration = self.start_time.elapsed();
    }

    /// Calculate tokens per minute.
    pub fn tokens_per_minute(&self) -> f64 {
        let mins = self.duration.as_secs_f64() / 60.0;
        if mins > 0.0 {
            self.total_tokens as f64 / mins
        } else {
            0.0
        }
    }

    /// Calculate input/output ratio.
    pub fn io_ratio(&self) -> f64 {
        if self.total_completion_tokens > 0 {
            self.total_prompt_tokens as f64 / self.total_completion_tokens as f64
        } else {
     0.0
        }
    }
}

/// Global token metrics tracker.
#[derive(Debug, Clone)]
pub struct TokenMetricsTracker {
    tasks: Arc<Mutex<HashMap<String, TaskTokenMetrics>>>,
    global_start: Instant,
}

impl TokenMetricsTracker {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
     global_start: Instant::now(),
        }
    }

    /// Start tracking a new task.
    pub fn start_task(&self, task_name: &str) {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.insert(
       task_name.to_string(),
            TaskTokenMetrics::new(task_name.to_string()),
        );
    }

    /// Record an interaction for a task.
    pub fn record_interaction(
    &self,
    task_name: &str,
        prompt: &str,
     completion: &str,
        interaction_type: InteractionType,
    ) {
        let prompt_tokens = estimate_content_tokens(prompt);
        let completion_tokens = estimate_content_tokens(completion);
        
        let interaction = TokenInteraction {
            timestamp: chrono::Local::now().to_rfc3339(),
  prompt_tokens,
     completion_tokens,
      total_tokens: prompt_tokens + completion_tokens,
         interaction_type,
    task_name: task_name.to_string(),
        };

        let mut tasks = self.tasks.lock().unwrap();
        if let Some(metrics) = tasks.get_mut(task_name) {
            metrics.add_interaction(interaction);
        }
    }

    /// Finalize a task's metrics.
    pub fn finalize_task(&self, task_name: &str) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(metrics) = tasks.get_mut(task_name) {
     metrics.finalize();
        }
    }

    /// Get all task metrics.
    pub fn get_all_metrics(&self) -> Vec<TaskTokenMetrics> {
        let tasks = self.tasks.lock().unwrap();
        tasks.values().cloned().collect()
    }

    /// Get metrics for a specific task.
    pub fn get_task_metrics(&self, task_name: &str) -> Option<TaskTokenMetrics> {
        let tasks = self.tasks.lock().unwrap();
        tasks.get(task_name).cloned()
    }

    /// Calculate global totals.
    pub fn global_totals(&self) -> GlobalTokenSummary {
        let tasks = self.tasks.lock().unwrap();
        let mut total_prompt = 0usize;
        let mut total_completion = 0usize;
        let mut total = 0usize;

        for metrics in tasks.values() {
   total_prompt += metrics.total_prompt_tokens;
 total_completion += metrics.total_completion_tokens;
            total += metrics.total_tokens;
        }

        GlobalTokenSummary {
      total_tasks: tasks.len(),
            total_prompt_tokens: total_prompt,
            total_completion_tokens: total_completion,
     total_tokens: total,
            elapsed: self.global_start.elapsed(),
        }
    }

    /// Generate detailed report.
    pub fn generate_report(&self) -> TokenMetricsReport {
        let tasks = self.get_all_metrics();
        let global = self.global_totals();

        // Calculate breakdown by interaction type
        let mut by_type: HashMap<String, usize> = HashMap::new();
        for task in &tasks {
     for interaction in &task.interactions {
       let count = by_type.entry(interaction.interaction_type.to_string()).or_insert(0);
       *count += interaction.total_tokens;
     }
        }

        TokenMetricsReport {
       generated_at: chrono::Local::now().to_rfc3339(),
            global_summary: global,
            task_metrics: tasks,
    tokens_by_type: by_type,
        }
    }
}

impl Default for TokenMetricsTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Global token summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalTokenSummary {
    pub total_tasks: usize,
    pub total_prompt_tokens: usize,
    pub total_completion_tokens: usize,
    pub total_tokens: usize,
    #[serde(skip)]
    pub elapsed: Duration,
}

impl GlobalTokenSummary {
    /// Calculate tokens per second.
    pub fn tokens_per_second(&self) -> f64 {
        let secs = self.elapsed.as_secs_f64();
        if secs > 0.0 {
  self.total_tokens as f64 / secs
        } else {
            0.0
        }
    }

    /// Calculate input/output ratio.
    pub fn io_ratio(&self) -> f64 {
        if self.total_completion_tokens > 0 {
self.total_prompt_tokens as f64 / self.total_completion_tokens as f64
        } else {
            0.0
        }
    }
}

/// Complete token metrics report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetricsReport {
    pub generated_at: String,
    pub global_summary: GlobalTokenSummary,
    pub task_metrics: Vec<TaskTokenMetrics>,
    pub tokens_by_type: HashMap<String, usize>,
}

impl TokenMetricsReport {
    /// Generate markdown report.
    pub fn to_markdown(&self) -> String {
        let mut md = format!(
            r#"# Token Metrics Report

Generated: {}

## Global Summary

| Metric | Value |
|--------|-------|
| Total Tasks | {} |
| Total Prompt Tokens | {:,} |
| Total Completion Tokens | {:,} |
| Total Tokens | {:,} |
| Input/Output Ratio | {:.2} |
| Tokens/Second | {:.1} |

## Token Usage by Interaction Type

| Type | Tokens | Percentage |
|------|--------|------------|
"#,
            self.generated_at,
            self.global_summary.total_tasks,
            self.global_summary.total_prompt_tokens,
            self.global_summary.total_completion_tokens,
     self.global_summary.total_tokens,
            self.global_summary.io_ratio(),
            self.global_summary.tokens_per_second(),
        );

        let total = self.global_summary.total_tokens as f64;
        for (type_name, tokens) in &self.tokens_by_type {
            let pct = if total > 0.0 {
 (*tokens as f64 / total) * 100.0
            } else {
         0.0
            };
   md.push_str(&format!("| {} | {:,} | {:.1}% |\n", type_name, tokens, pct));
        }

        md.push_str("\n## Per-Task Metrics\n\n");
        md.push_str("| Task | Prompt | Completion | Total | Interactions | T/Min | I/O Ratio |\n");
        md.push_str("|------|--------|------------|-------|--------------|-------|-----------|\n");

        for task in &self.task_metrics {
     md.push_str(&format!(
                "| {} | {:,} | {:,} | {:,} | {} | {:.0} | {:.2} |\n",
    task.task_name,
        task.total_prompt_tokens,
         task.total_completion_tokens,
         task.total_tokens,
         task.interactions.len(),
      task.tokens_per_minute(),
         task.io_ratio(),
            ));
     }

        md
    }

    /// Save report to directory.
    pub fn save_to_dir(&self, dir: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;

        // Save markdown
        let md_path = dir.join("token_metrics.md");
        std::fs::write(&md_path, self.to_markdown())?;

        // Save JSON
        let json_path = dir.join("token_metrics.json");
        let json = serde_json::to_string_pretty(self).unwrap_or_default();
     std::fs::write(&json_path, json)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_metrics() {
        let mut metrics = TaskTokenMetrics::new("test".to_string());
        
        let interaction = TokenInteraction {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
    prompt_tokens: 100,
      completion_tokens: 50,
      total_tokens: 150,
   interaction_type: InteractionType::ToolCall,
      task_name: "test".to_string(),
        };
        
        metrics.add_interaction(interaction);
        
        assert_eq!(metrics.total_prompt_tokens, 100);
        assert_eq!(metrics.total_completion_tokens, 50);
        assert_eq!(metrics.total_tokens, 150);
    }

    #[test]
    fn test_tracker() {
        let tracker = TokenMetricsTracker::new();
        tracker.start_task("task1");
        
        tracker.record_interaction(
       "task1",
   "Hello world",
      "Response here",
            InteractionType::ToolCall,
        );
        
   let metrics = tracker.get_task_metrics("task1");
        assert!(metrics.is_some());
        assert_eq!(metrics.unwrap().interactions.len(), 1);
    }
}
