//! Batch Mode - Execute multiple tasks in parallel
//!
//! Provides true headless parallel execution for maximum throughput

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Semaphore, Mutex};
use tracing::{info, warn};

/// Configuration for batch execution
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum number of concurrent workers
    pub max_workers: usize,
    /// Timeout per task in seconds
    pub timeout_secs: u64,
    /// Aggregate results into single output
    pub aggregate: bool,
    /// Output directory for results
    pub output_dir: PathBuf,
    /// Continue on individual task failure
    pub continue_on_error: bool,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_workers: 16,
            timeout_secs: 300,
            aggregate: false,
            output_dir: PathBuf::from("./batch_results"),
            continue_on_error: true,
        }
    }
}

/// Result of a single batch task
#[derive(Debug, Clone)]
pub struct BatchTaskResult {
    pub task_id: usize,
    pub task: String,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub duration_secs: f64,
}

/// Batch executor for running multiple tasks in parallel
pub struct BatchExecutor {
    config: BatchConfig,
    semaphore: Arc<Semaphore>,
    results: Arc<Mutex<Vec<BatchTaskResult>>>,
}

impl BatchExecutor {
    /// Create a new batch executor
    pub fn new(config: BatchConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_workers));
        Self {
            config,
            semaphore,
            results: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Execute a list of tasks in parallel
    pub async fn execute_tasks(&self, tasks: Vec<String>) -> Result<Vec<BatchTaskResult>> {
        let total_tasks = tasks.len();
        info!("Starting batch execution of {} tasks with {} workers", total_tasks, self.config.max_workers);
        
        let start_time = std::time::Instant::now();
        let mut handles = Vec::new();
        
        // Spawn all tasks limited by semaphore
        for (task_id, task) in tasks.into_iter().enumerate() {
            let permit = self.semaphore.clone().acquire_owned().await?;
            let results = self.results.clone();
            let config = self.config.clone();
            
            let handle = tokio::spawn(async move {
                let _permit = permit;
                let task_start = std::time::Instant::now();
                
                info!("[Task {}/{}] Starting: {}", task_id + 1, total_tasks, &task[..50.min(task.len())]);
                
                // Simulate task execution - would integrate with Agent here
                let result = Self::execute_single_task(task_id, task.clone(), config.timeout_secs).await;
                
                let duration = task_start.elapsed().as_secs_f64();
                let result = match result {
                    Ok(output) => BatchTaskResult {
                        task_id,
                        task: task.clone(),
                        success: true,
                        output,
                        error: None,
                        duration_secs: duration,
                    },
                    Err(e) => BatchTaskResult {
                        task_id,
                        task: task.clone(),
                        success: false,
                        output: String::new(),
                        error: Some(e.to_string()),
                        duration_secs: duration,
                    },
                };
                
                results.lock().await.push(result.clone());
                
                info!("[Task {}/{}] Completed in {:.2}s - {}", 
                    task_id + 1, total_tasks, duration,
                    if result.success { "✓" } else { "✗" }
                );
                
                result
            });
            
            handles.push(handle);
        }
        
        // Wait for all tasks
        let mut results = Vec::new();
        for handle in handles {
            if let Ok(result) = handle.await {
                results.push(result);
            }
        }
        
        let total_duration = start_time.elapsed().as_secs_f64();
        let success_count = results.iter().filter(|r| r.success).count();
        
        info!("Batch complete: {}/{} succeeded in {:.2}s", success_count, total_tasks, total_duration);
        
        // Sort by task_id
        results.sort_by_key(|r| r.task_id);
        
        Ok(results)
    }

    /// Execute a single task (placeholder - would integrate with Agent)
    async fn execute_single_task(
        _task_id: usize,
        task: String,
        _timeout_secs: u64,
    ) -> Result<String> {
        // This is a placeholder - real implementation would use Agent
        info!("Executing task: {}", task);
        Ok(format!("Completed: {}", task))
    }

    /// Aggregate results into a single output
    pub fn aggregate_results(&self, results: &[BatchTaskResult]) -> String {
        let mut output = String::new();
        output.push_str("# Batch Execution Results\n\n");
        output.push_str(&format!("Total tasks: {}\n", results.len()));
        output.push_str(&format!("Successful: {}\n", results.iter().filter(|r| r.success).count()));
        output.push_str(&format!("Failed: {}\n\n", results.iter().filter(|r| !r.success).count()));
        
        for result in results {
            output.push_str(&format!("## Task {} ", result.task_id));
            output.push_str(&format!("{}", if result.success { "✓" } else { "✗" }));
            output.push('\n');
            output.push_str(&format!("**Task:** {}\n", result.task));
            output.push_str(&format!("**Duration:** {:.2}s\n", result.duration_secs));
            
            if !result.success {
                if let Some(ref error) = result.error {
                    output.push_str(&format!("**Error:** {}\n", error));
                }
            }
            
            output.push_str("**Output:**\n");
            output.push_str("```\n");
            output.push_str(&result.output);
            output.push_str("\n```\n\n");
        }
        
        output
    }
}

/// Parse a file containing tasks (one per line)
pub fn parse_tasks_file(path: &PathBuf) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)?;
    let tasks: Vec<String> = content
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .collect();
    Ok(tasks)
}
