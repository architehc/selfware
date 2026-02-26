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
            first_try_verification_rate: if verification_passed_first {
                1.0
            } else {
                0.0
            },
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
mod tests {
    use super::*;

    #[test]
    fn test_performance_snapshot_from_checkpoint() {
        let snapshot =
            PerformanceSnapshot::from_checkpoint_data(5, 10, 2, 1, true, 5000, true);
        assert_eq!(snapshot.task_success_rate, 1.0);
        assert_eq!(snapshot.avg_iterations, 5.0);
        assert_eq!(snapshot.avg_tool_calls, 10.0);
        assert_eq!(snapshot.error_recovery_rate, 0.5);
        assert_eq!(snapshot.first_try_verification_rate, 1.0);
    }

    #[test]
    fn test_effectiveness_delta() {
        let before = PerformanceSnapshot::from_checkpoint_data(10, 20, 5, 2, false, 10000, false);
        let after = PerformanceSnapshot::from_checkpoint_data(5, 10, 2, 2, true, 5000, true);
        let delta = after.effectiveness_delta(&before);
        assert!(delta > 0.0, "Improvement should be positive: {}", delta);
    }

    #[test]
    fn test_performance_snapshot_with_label() {
        let snapshot =
            PerformanceSnapshot::from_checkpoint_data(5, 10, 2, 1, true, 5000, true)
                .with_label("pre-improve-42");
        assert_eq!(snapshot.label, Some("pre-improve-42".to_string()));
    }

    #[test]
    fn test_performance_snapshot_failed_task() {
        let snapshot =
            PerformanceSnapshot::from_checkpoint_data(10, 20, 5, 0, false, 8000, false);
        assert_eq!(snapshot.task_success_rate, 0.0);
        assert_eq!(snapshot.first_try_verification_rate, 0.0);
        assert_eq!(snapshot.error_recovery_rate, 0.0);
        assert_eq!(snapshot.compilation_errors_per_task, 5.0);
    }

    #[test]
    fn test_performance_snapshot_no_errors() {
        let snapshot =
            PerformanceSnapshot::from_checkpoint_data(3, 5, 0, 0, true, 2000, true);
        // No errors means recovery rate defaults to 1.0
        assert_eq!(snapshot.error_recovery_rate, 1.0);
        assert_eq!(snapshot.compilation_errors_per_task, 0.0);
    }

    #[test]
    fn test_effectiveness_delta_regression() {
        // After is worse than before
        let before = PerformanceSnapshot::from_checkpoint_data(5, 10, 1, 1, true, 3000, true);
        let after = PerformanceSnapshot::from_checkpoint_data(10, 20, 5, 0, false, 10000, false);
        let delta = after.effectiveness_delta(&before);
        assert!(delta < 0.0, "Regression should be negative: {}", delta);
    }

    #[test]
    fn test_effectiveness_delta_identical() {
        let snap = PerformanceSnapshot::from_checkpoint_data(5, 10, 1, 1, true, 5000, true);
        let delta = snap.effectiveness_delta(&snap);
        assert!(
            delta.abs() < 0.001,
            "Identical snapshots should have ~0 delta: {}",
            delta
        );
    }

    #[test]
    fn test_performance_snapshot_serialization_roundtrip() {
        let snapshot = PerformanceSnapshot::from_checkpoint_data(5, 10, 2, 1, true, 5000, true)
            .with_label("test");
        let json = serde_json::to_string(&snapshot).unwrap();
        let deserialized: PerformanceSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.avg_iterations, 5.0);
        assert_eq!(deserialized.label, Some("test".to_string()));
    }

    #[test]
    fn test_metrics_store_roundtrip() {
        let tmp = std::env::temp_dir().join("selfware_test_metrics.jsonl");
        // Clean up from any previous run
        std::fs::remove_file(&tmp).ok();

        let store = MetricsStore::with_path(tmp.clone());

        let s1 = PerformanceSnapshot::from_checkpoint_data(5, 10, 1, 1, true, 5000, true);
        let s2 = PerformanceSnapshot::from_checkpoint_data(3, 8, 0, 0, true, 3000, true);
        store.record(&s1).unwrap();
        store.record(&s2).unwrap();

        let latest = store.latest().unwrap().unwrap();
        assert_eq!(latest.avg_iterations, 3.0);

        let trend = store.trend(10).unwrap();
        assert_eq!(trend.len(), 2);

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_metrics_store_empty() {
        let tmp = std::env::temp_dir().join("selfware_test_metrics_empty.jsonl");
        std::fs::remove_file(&tmp).ok();

        let store = MetricsStore::with_path(tmp.clone());
        assert!(store.latest().unwrap().is_none());
        assert!(store.trend(10).unwrap().is_empty());
        assert!(store.running_average(10).unwrap().is_none());
    }

    #[test]
    fn test_metrics_store_running_average() {
        let tmp = std::env::temp_dir().join("selfware_test_metrics_avg.jsonl");
        std::fs::remove_file(&tmp).ok();

        let store = MetricsStore::with_path(tmp.clone());

        let s1 = PerformanceSnapshot::from_checkpoint_data(10, 20, 2, 1, false, 10000, true);
        let s2 = PerformanceSnapshot::from_checkpoint_data(6, 12, 0, 0, true, 6000, true);
        let s3 = PerformanceSnapshot::from_checkpoint_data(2, 4, 0, 0, true, 2000, true);
        store.record(&s1).unwrap();
        store.record(&s2).unwrap();
        store.record(&s3).unwrap();

        let avg = store.running_average(3).unwrap().unwrap();
        assert!((avg.avg_iterations - 6.0).abs() < 0.001); // (10+6+2)/3
        assert!((avg.avg_tool_calls - 12.0).abs() < 0.001); // (20+12+4)/3
        assert!((avg.avg_tokens - 6000.0).abs() < 0.001); // (10000+6000+2000)/3
        assert!(avg.label.unwrap().contains("avg_of_3"));

        // Running average of last 2 only
        let avg2 = store.running_average(2).unwrap().unwrap();
        assert!((avg2.avg_iterations - 4.0).abs() < 0.001); // (6+2)/2

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_metrics_store_trend_limited() {
        let tmp = std::env::temp_dir().join("selfware_test_metrics_trend.jsonl");
        std::fs::remove_file(&tmp).ok();

        let store = MetricsStore::with_path(tmp.clone());
        for i in 0..5 {
            let s = PerformanceSnapshot::from_checkpoint_data(i, i * 2, 0, 0, true, 1000, true);
            store.record(&s).unwrap();
        }

        // Request last 3 out of 5
        let trend = store.trend(3).unwrap();
        assert_eq!(trend.len(), 3);
        assert_eq!(trend[0].avg_iterations, 2.0);
        assert_eq!(trend[2].avg_iterations, 4.0);

        // Request more than available
        let trend_all = store.trend(100).unwrap();
        assert_eq!(trend_all.len(), 5);

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_metrics_store_append_only() {
        let tmp = std::env::temp_dir().join("selfware_test_metrics_append.jsonl");
        std::fs::remove_file(&tmp).ok();

        let store = MetricsStore::with_path(tmp.clone());
        store
            .record(&PerformanceSnapshot::from_checkpoint_data(1, 1, 0, 0, true, 100, true))
            .unwrap();

        // Create a new store instance pointing to same file — should see previous data
        let store2 = MetricsStore::with_path(tmp.clone());
        store2
            .record(&PerformanceSnapshot::from_checkpoint_data(2, 2, 0, 0, true, 200, true))
            .unwrap();

        let trend = store2.trend(10).unwrap();
        assert_eq!(trend.len(), 2);
        assert_eq!(trend[0].avg_iterations, 1.0);
        assert_eq!(trend[1].avg_iterations, 2.0);

        std::fs::remove_file(&tmp).ok();
    }
}
