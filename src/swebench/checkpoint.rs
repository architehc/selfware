//! Evaluation Checkpointing
//!
//! Supports resuming long-running evaluations from where they left off.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::{TaskResult, SWEBenchTask};

/// Checkpoint for resuming evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationCheckpoint {
    /// List of completed task IDs
    pub completed_tasks: Vec<String>,
    /// Results for completed tasks
    pub results: Vec<TaskResult>,
    /// Checkpoint timestamp
    pub timestamp: DateTime<Utc>,
    /// Evaluation configuration hash (to detect config changes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_hash: Option<String>,
    /// Total number of tasks in the evaluation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tasks: Option<usize>,
}

impl EvaluationCheckpoint {
    /// Create a new checkpoint
    pub fn new() -> Self {
        Self {
            completed_tasks: Vec::new(),
            results: Vec::new(),
            timestamp: Utc::now(),
            config_hash: None,
            total_tasks: None,
        }
    }

    /// Add a completed task
    pub fn add_task(&mut self, task_id: &str, result: TaskResult) {
        if !self.completed_tasks.contains(&task_id.to_string()) {
            self.completed_tasks.push(task_id.to_string());
            self.results.push(result);
            self.timestamp = Utc::now();
        }
    }

    /// Save checkpoint to file
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .with_context(|| "Failed to serialize checkpoint")?;

        std::fs::write(path, json)
            .with_context(|| format!("Failed to write checkpoint to: {}", path.display()))?;

        tracing::info!("Checkpoint saved: {} tasks completed", self.completed_tasks.len());
        Ok(())
    }

    /// Load checkpoint from file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read checkpoint from: {}", path.display()))?;

        let checkpoint: EvaluationCheckpoint = serde_json::from_str(&content)
            .with_context(|| "Failed to deserialize checkpoint")?;

        tracing::info!(
            "Checkpoint loaded: {} tasks completed (from {})",
            checkpoint.completed_tasks.len(),
            checkpoint.timestamp
        );

        Ok(checkpoint)
    }

    /// Get remaining tasks from a full task list
    pub fn remaining_tasks<'a>(&self, all_tasks: &'a [SWEBenchTask]) -> Vec<&'a SWEBenchTask> {
        all_tasks
            .iter()
            .filter(|t| !self.completed_tasks.contains(&t.instance_id))
            .collect()
    }

    /// Get completion percentage
    pub fn completion_pct(&self) -> f64 {
        if let Some(total) = self.total_tasks {
            if total > 0 {
                return (self.completed_tasks.len() as f64 / total as f64) * 100.0;
            }
        }
        0.0
    }

    /// Check if a task is completed
    pub fn is_completed(&self, task_id: &str) -> bool {
        self.completed_tasks.contains(&task_id.to_string())
    }

    /// Merge another checkpoint into this one
    pub fn merge(&mut self, other: &EvaluationCheckpoint) {
        for (i, task_id) in other.completed_tasks.iter().enumerate() {
            if !self.completed_tasks.contains(task_id) {
                self.completed_tasks.push(task_id.clone());
                if let Some(result) = other.results.get(i) {
                    self.results.push(result.clone());
                }
            }
        }
        self.timestamp = Utc::now();
    }

    /// Calculate hash of configuration for validation
    pub fn calculate_config_hash(config: &super::SWEConfig) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        config.max_concurrent.hash(&mut hasher);
        config.timeout_secs.hash(&mut hasher);
        config.model.hash(&mut hasher);
        config.endpoint.hash(&mut hasher);
        config.max_tokens.hash(&mut hasher);
        
        format!("{:x}", hasher.finish())
    }

    /// Validate that checkpoint matches current configuration
    pub fn validate_config(&self, config: &super::SWEConfig) -> bool {
        if let Some(ref hash) = self.config_hash {
            return *hash == Self::calculate_config_hash(config);
        }
        true // No hash means no validation
    }
}

impl Default for EvaluationCheckpoint {
    fn default() -> Self {
        Self::new()
    }
}

/// Auto-save checkpoint periodically
pub struct CheckpointManager {
    checkpoint: EvaluationCheckpoint,
    path: std::path::PathBuf,
    interval: usize,
    counter: usize,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new(path: std::path::PathBuf, interval: usize) -> Self {
        Self {
            checkpoint: EvaluationCheckpoint::new(),
            path,
            interval,
            counter: 0,
        }
    }

    /// Load existing checkpoint or create new
    pub fn load_or_create(path: &Path, interval: usize) -> Result<Self> {
        let checkpoint = if path.exists() {
            EvaluationCheckpoint::load(path)?
        } else {
            EvaluationCheckpoint::new()
        };

        Ok(Self {
            checkpoint,
            path: path.to_path_buf(),
            interval,
            counter: 0,
        })
    }

    /// Record a completed task
    pub fn record_task(&mut self, task_id: &str, result: TaskResult) -> Result<()> {
        self.checkpoint.add_task(task_id, result);
        self.counter += 1;

        // Save every N tasks
        if self.counter >= self.interval {
            self.save()?;
            self.counter = 0;
        }

        Ok(())
    }

    /// Force save checkpoint
    pub fn save(&self) -> Result<()> {
        self.checkpoint.save(&self.path)
    }

    /// Get checkpoint reference
    pub fn checkpoint(&self) -> &EvaluationCheckpoint {
        &self.checkpoint
    }

    /// Get mutable checkpoint reference
    pub fn checkpoint_mut(&mut self) -> &mut EvaluationCheckpoint {
        &mut self.checkpoint
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn create_test_result(task_id: &str) -> TaskResult {
        TaskResult {
            task_id: task_id.to_string(),
            resolved: true,
            patch_quality: 0.8,
            duration_secs: 60.0,
            tokens_used: 5000,
            iterations: 10,
            error: None,
            trajectory: vec![],
            test_output: None,
            files_modified: vec!["file.py".to_string()],
        }
    }

    #[test]
    fn test_checkpoint_new() {
        let cp = EvaluationCheckpoint::new();
        assert!(cp.completed_tasks.is_empty());
        assert!(cp.results.is_empty());
    }

    #[test]
    fn test_checkpoint_add_task() {
        let mut cp = EvaluationCheckpoint::new();
        let result = create_test_result("test-1");
        
        cp.add_task("test-1", result.clone());
        
        assert_eq!(cp.completed_tasks.len(), 1);
        assert_eq!(cp.results.len(), 1);
        assert!(cp.is_completed("test-1"));
    }

    #[test]
    fn test_checkpoint_save_load() {
        let mut cp = EvaluationCheckpoint::new();
        cp.add_task("test-1", create_test_result("test-1"));
        cp.add_task("test-2", create_test_result("test-2"));
        cp.total_tasks = Some(10);

        let file = NamedTempFile::new().unwrap();
        cp.save(file.path()).unwrap();

        let loaded = EvaluationCheckpoint::load(file.path()).unwrap();
        assert_eq!(loaded.completed_tasks.len(), 2);
        assert_eq!(loaded.total_tasks, Some(10));
    }

    #[test]
    fn test_remaining_tasks() {
        let mut cp = EvaluationCheckpoint::new();
        cp.add_task("repo__name-1", create_test_result("repo__name-1"));

        let all_tasks = vec![
            SWEBenchTask {
                repo: "repo/name".to_string(),
                instance_id: "repo__name-1".to_string(),
                problem_statement: "Test 1".to_string(),
                hints_text: String::new(),
                base_commit: "abc".to_string(),
                solution_commit: None,
                test_files: vec![],
                target_files: vec![],
                difficulty: super::super::TaskDifficulty::Easy,
                patch: None,
                test_patch: None,
                version: String::new(),
                fail_to_pass: String::new(),
                pass_to_pass: String::new(),
            },
            SWEBenchTask {
                repo: "repo/name".to_string(),
                instance_id: "repo__name-2".to_string(),
                problem_statement: "Test 2".to_string(),
                hints_text: String::new(),
                base_commit: "def".to_string(),
                solution_commit: None,
                test_files: vec![],
                target_files: vec![],
                difficulty: super::super::TaskDifficulty::Medium,
                patch: None,
                test_patch: None,
                version: String::new(),
                fail_to_pass: String::new(),
                pass_to_pass: String::new(),
            },
        ];

        let remaining = cp.remaining_tasks(&all_tasks);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].instance_id, "repo__name-2");
    }

    #[test]
    fn test_completion_pct() {
        let mut cp = EvaluationCheckpoint::new();
        cp.completed_tasks = vec!["a".to_string(), "b".to_string()];
        cp.total_tasks = Some(10);

        assert_eq!(cp.completion_pct(), 20.0);
    }

    #[test]
    fn test_checkpoint_merge() {
        let mut cp1 = EvaluationCheckpoint::new();
        cp1.add_task("test-1", create_test_result("test-1"));

        let mut cp2 = EvaluationCheckpoint::new();
        cp2.add_task("test-2", create_test_result("test-2"));

        cp1.merge(&cp2);

        assert_eq!(cp1.completed_tasks.len(), 2);
        assert!(cp1.is_completed("test-1"));
        assert!(cp1.is_completed("test-2"));
    }

    #[test]
    fn test_checkpoint_manager() {
        let file = NamedTempFile::new().unwrap();
        let mut manager = CheckpointManager::new(file.path().to_path_buf(), 2);

        manager.record_task("test-1", create_test_result("test-1")).unwrap();
        assert_eq!(manager.checkpoint().completed_tasks.len(), 1);

        // Should auto-save on 2nd task
        manager.record_task("test-2", create_test_result("test-2")).unwrap();
        
        // Verify saved
        let loaded = EvaluationCheckpoint::load(file.path()).unwrap();
        assert_eq!(loaded.completed_tasks.len(), 2);
    }
}
