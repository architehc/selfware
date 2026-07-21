//! Performance Metrics Collection
//!
//! Append-only metrics store for tracking agent performance over time.
//! Used by the self-improvement loop to measure effectiveness of changes.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Snapshot of agent performance at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSnapshot {
    /// Timestamp of the snapshot
    pub timestamp: u64,
    /// Task success rate (0.0 - 1.0)
    pub task_success_rate: f64,
    /// Average iterations per task
    pub avg_iterations: f64,
    /// Average tool calls per task
    pub avg_tool_calls: f64,
    /// Error recovery rate (0.0 - 1.0)
    pub error_recovery_rate: f64,
    /// First-try verification pass rate (0.0 - 1.0)
    pub first_try_verification_rate: f64,
    /// Average tokens consumed per task
    pub avg_tokens: f64,
    /// Test pass rate (0.0 - 1.0)
    pub test_pass_rate: f64,
    /// Compilation errors per task
    pub compilation_errors_per_task: f64,
    /// Optional label (e.g. "pre-improve-42", "post-improve-42")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl PerformanceSnapshot {
    /// Create a snapshot from checkpoint data
    pub fn from_checkpoint_data(
        iterations: usize,
        tool_calls: usize,
        errors_total: usize,
        errors_recovered: usize,
        verification_passed_first: bool,
        tokens: usize,
        task_succeeded: bool,
    ) -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            task_success_rate: if task_succeeded { 1.0 } else { 0.0 },
            avg_iterations: iterations as f64,
            avg_tool_calls: tool_calls as f64,
            error_recovery_rate: if errors_total > 0 {
                errors_recovered as f64 / errors_total as f64
            } else {
                1.0
            },
            first_try_verification_rate: if verification_passed_first { 1.0 } else { 0.0 },
            avg_tokens: tokens as f64,
            test_pass_rate: if task_succeeded { 1.0 } else { 0.0 },
            compilation_errors_per_task: (errors_total - errors_recovered) as f64,
            label: None,
        }
    }

    /// Add a label to this snapshot
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Compute weighted delta between two snapshots (positive = improvement)
    pub fn effectiveness_delta(&self, before: &PerformanceSnapshot) -> f64 {
        let delta_success = self.task_success_rate - before.task_success_rate;
        let delta_verification =
            self.first_try_verification_rate - before.first_try_verification_rate;
        let delta_iterations = before.avg_iterations - self.avg_iterations; // lower is better
        let delta_recovery = self.error_recovery_rate - before.error_recovery_rate;
        let delta_tokens = before.avg_tokens - self.avg_tokens; // lower is better

        // Normalize token delta to 0-1 scale (cap at 50% improvement)
        let norm_tokens = if before.avg_tokens > 0.0 {
            (delta_tokens / before.avg_tokens).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let norm_iterations = if before.avg_iterations > 0.0 {
            (delta_iterations / before.avg_iterations).clamp(-1.0, 1.0)
        } else {
            0.0
        };

        0.3 * delta_success
            + 0.2 * delta_verification
            + 0.2 * norm_iterations
            + 0.15 * delta_recovery
            + 0.15 * norm_tokens
    }
}

/// Append-only JSONL store for performance snapshots
pub struct MetricsStore {
    path: PathBuf,
}

impl MetricsStore {
    /// Create a new metrics store at the default path
    pub fn new() -> Self {
        let path = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("selfware")
            .join("metrics")
            .join("snapshots.jsonl");
        Self { path }
    }

    /// Create a metrics store at a custom path
    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    /// Record a new performance snapshot (append to JSONL)
    pub fn record(&self, snapshot: &PerformanceSnapshot) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let line = serde_json::to_string(snapshot)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    /// Get the latest snapshot
    pub fn latest(&self) -> Result<Option<PerformanceSnapshot>> {
        let snapshots = self.load_all()?;
        Ok(snapshots.into_iter().last())
    }

    /// Get the last N snapshots for trend analysis
    pub fn trend(&self, n: usize) -> Result<Vec<PerformanceSnapshot>> {
        let snapshots = self.load_all()?;
        let start = snapshots.len().saturating_sub(n);
        Ok(snapshots[start..].to_vec())
    }

    /// Compute the running average of the last N snapshots
    pub fn running_average(&self, n: usize) -> Result<Option<PerformanceSnapshot>> {
        let snapshots = self.trend(n)?;
        if snapshots.is_empty() {
            return Ok(None);
        }
        let count = snapshots.len() as f64;
        let avg = PerformanceSnapshot {
            timestamp: snapshots.last().map(|s| s.timestamp).unwrap_or(0),
            task_success_rate: snapshots.iter().map(|s| s.task_success_rate).sum::<f64>() / count,
            avg_iterations: snapshots.iter().map(|s| s.avg_iterations).sum::<f64>() / count,
            avg_tool_calls: snapshots.iter().map(|s| s.avg_tool_calls).sum::<f64>() / count,
            error_recovery_rate: snapshots.iter().map(|s| s.error_recovery_rate).sum::<f64>()
                / count,
            first_try_verification_rate: snapshots
                .iter()
                .map(|s| s.first_try_verification_rate)
                .sum::<f64>()
                / count,
            avg_tokens: snapshots.iter().map(|s| s.avg_tokens).sum::<f64>() / count,
            test_pass_rate: snapshots.iter().map(|s| s.test_pass_rate).sum::<f64>() / count,
            compilation_errors_per_task: snapshots
                .iter()
                .map(|s| s.compilation_errors_per_task)
                .sum::<f64>()
                / count,
            label: Some(format!("avg_of_{}", snapshots.len())),
        };
        Ok(Some(avg))
    }

    fn load_all(&self) -> Result<Vec<PerformanceSnapshot>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = std::fs::File::open(&self.path)?;
        let reader = std::io::BufReader::new(file);
        let mut snapshots = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(snapshot) = serde_json::from_str::<PerformanceSnapshot>(&line) {
                snapshots.push(snapshot);
            }
        }
        Ok(snapshots)
    }
}

impl Default for MetricsStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/cognitive/metrics/metrics_test.rs"]
mod tests;
